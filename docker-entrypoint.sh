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

# `wait -n` is not POSIX and this image uses dash, so poll instead. If either
# process dies the container dies: a half-up demo is worse than a clean failure.
while :; do
  if ! kill -0 "$ENGINE_PID" 2>/dev/null; then
    echo "engine exited; stopping" >&2
    kill "$WEB_PID" 2>/dev/null || true
    exit 1
  fi
  if ! kill -0 "$WEB_PID" 2>/dev/null; then
    echo "web server exited; stopping" >&2
    kill "$ENGINE_PID" 2>/dev/null || true
    exit 1
  fi
  sleep 2
done
