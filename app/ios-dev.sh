#!/usr/bin/env bash
# The iOS-simulator dev loop: packs + launches the app in the simulator (see
# ./ios.sh), while a watcher answers each save of a .rs file with a daemon
# build for aarch64-apple-ios-sim — the running sim app applies it as a live
# patch, same as the desktop loop.
#
#   ./ios-dev.sh [device name]   default: iPhone 17 Pro
#   save src/*.rs                -> cargo watch -> patch -> remount in the sim
#   quit / Ctrl-C                -> everything stops
set -e
cd "$(dirname "$0")"

cargo watch -w src -x 'ra-jit build --target aarch64-apple-ios-sim' &
WATCH=$!
trap 'kill $WATCH 2>/dev/null' EXIT

exec ./ios.sh "$@"
