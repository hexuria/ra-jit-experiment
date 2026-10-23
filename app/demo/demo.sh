#!/usr/bin/env bash
# The live-edit demo driver: copies a step file over src/hot.rs; the
# `cargo watch` running beside it rebuilds both targets and the two apps
# — macOS window and iOS simulator — splice the patch into the running
# processes and remount the hot area. The game keeps playing throughout.
#
# Start the watcher first (one terminal):
#   cargo watch -w src -x 'ra-jit build' -x 'ra-jit build --target aarch64-apple-ios-sim'
#
# Then:
#   ./demo/demo.sh            -> apply the next step
#   ./demo/demo.sh 4          -> jump to step 04
#   ./demo/demo.sh run 5      -> walk every step, 5 s apart (default 6)
#   ./demo/demo.sh reset      -> back to the plain baseline
#   ./demo/demo.sh list       -> show the steps
cd "$(dirname "$0")/.."

STEPS_DIR=demo/steps
POS_FILE=demo/.step

step_files() { ls "$STEPS_DIR"/[0-9]*.rs 2>/dev/null | sort; }

pos() { cat "$POS_FILE" 2>/dev/null || echo 0; }

apply() {
    cp "$1" src/hot.rs
    echo ">> $(basename "$1")  ($(grep -c '' "$1") lines)"
}

case "${1:-next}" in
    list)
        step_files | cat -n
        ;;
    reset)
        apply "$(step_files | head -1)" && echo 0 > "$POS_FILE"
        ;;
    run)
        delay="${2:-6}"
        i=0
        for f in $(step_files); do
            i=$((i + 1))
            [ "$i" -le 1 ] && continue   # 00 is the baseline
            apply "$f"
            echo "   next in ${delay}s (Ctrl-C to stop)"
            sleep "$delay"
        done
        echo "$((i - 1))" > "$POS_FILE"
        echo ">> all steps applied — the status bars kept counting"
        ;;
    next|"")
        files=($(step_files))
        n=$(( $(pos) + 1 ))
        [ "$n" -ge "${#files[@]}" ] && n=1
        apply "${files[$n]}" && echo "$n" > "$POS_FILE"
        ;;
    *)
        n=$(printf "%02d" "$1")
        f=$(step_files | grep "/$n-" || true)
        [ -n "$f" ] && apply "$f" && echo "$1" > "$POS_FILE" \
            || { echo "no step $1 — run: ./demo/demo.sh list"; exit 1; }
        ;;
esac
