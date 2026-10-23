#!/usr/bin/env bash
# The desktop dev loop: the app runs under ra-jit; every save of a .rs file
# under src/ is answered with a patch the running app applies (the apply is
# sub-millisecond; the daemon builds it in about a second after the save
# reaches it).
#
#   ./dev.sh          first build (cold: cargo does it once per daemon),
#                     then the window opens and stays patched
#   save src/*.rs     -> cargo watch runs `cargo ra-jit build` -> patch -> remount
#   quit / Ctrl-C     -> everything stops
set -e
cd "$(dirname "$0")"

cargo ra-jit build &          # warm the daemon while the watcher comes up
BUILD=$!
cargo watch -w src -x 'ra-jit build' &
WATCH=$!
trap 'kill $WATCH 2>/dev/null' EXIT
wait $BUILD                   # first build done, then the window opens

cargo ra-jit run
