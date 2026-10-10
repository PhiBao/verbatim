#!/bin/sh
# The engine and the web server share one container: the engine binds to
# loopback (it is only reached through the Next proxy) and Next serves the page.
set -e

mkdir -p "$(dirname "$VERBATIM_DB")"

/app/engine &
ENGINE_PID=$!

cd /app/web
HOSTNAME=0.0.0.0 PORT=3000 node server.js &
WEB_PID=$!

# If either dies, the container should die: a half-up demo is worse than none.
wait -n "$ENGINE_PID" "$WEB_PID"
STATUS=$?
kill "$ENGINE_PID" "$WEB_PID" 2>/dev/null || true
exit "$STATUS"
