#!/usr/bin/env bash
# Boot smoke test for OpenCode plugins.
#
# An OpenCode v2 plugin can pass every shape check and still deadlock server boot
# forever. Specifically `{ id, effect }` validates against the loader's internal
# schema and then hangs: `effect` is the Effect-based variant used by built-ins,
# while the public contract is `{ id, setup }`. A static check cannot catch that,
# and the previous version of this script certified exactly the broken shape.
#
#   docs/opencode-v2-plugins.md §2 has the measurements.
#
# So the only check that counts is: does a session actually complete?
#
#   exit 0  → the plugin loads and the server boots
#   exit 124 → the boot deadlocked; the plugin is in the broken shape
#
# Run from the repository root.

set -uo pipefail

TIMEOUT="${PLUGIN_SMOKE_TIMEOUT:-180}"
MODEL="${PLUGIN_SMOKE_MODEL:-opencode/space-bunny-free}"
DIR="$(cd "$(dirname "$0")/.." && pwd)"

cd "$DIR"

if [ ! -d .opencode/plugins ]; then
  echo "SKIP  .opencode/plugins is not present (plugins are disabled)"
  exit 0
fi

plugins=$(find .opencode/plugins -maxdepth 1 -name '*.ts' | sort)
if [ -z "$plugins" ]; then
  echo "SKIP  no plugin files found"
  exit 0
fi

echo "boot smoke test — one session must complete"
echo "  timeout: ${TIMEOUT}s   model: ${MODEL}"
echo

start=$(date +%s)
timeout "$TIMEOUT" opencode run --standalone --model "$MODEL" "say pong" >/tmp/opencode/plugin-smoke.log 2>&1
status=$?
elapsed=$(( $(date +%s) - start ))

case "$status" in
  0)
    echo "PASS  session completed in ${elapsed}s"
    exit 0
    ;;
  124)
    echo "FAIL  server boot deadlocked after ${elapsed}s (exit 124)."
    echo
    echo "      Almost certainly an { id, effect } plugin. That shape passes the"
    echo "      loader's schema and then hangs. Change it to { id, setup }."
    echo "      See docs/opencode-v2-plugins.md."
    echo
    echo "      Offending files:"
    for f in $plugins; do
      if grep -qE '^\s*effect\s*:' "$f"; then
        echo "        $f  (exports effect)"
      fi
    done
    exit 1
    ;;
  *)
    echo "FAIL  opencode exited ${status} in ${elapsed}s"
    echo "      log: /tmp/opencode/plugin-smoke.log"
    tail -20 /tmp/opencode/plugin-smoke.log 2>/dev/null | sed 's/^/      /'
    exit 1
    ;;
esac
