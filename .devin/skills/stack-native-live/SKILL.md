---
name: stack-native-live
description: Work on the stack-native NSH+ra-jit app — edit patchable hot.rs code, run the macOS/iOS-simulator dev loops, or verify a live splice landed with state intact. Use when touching app/, running dev.sh/ios.sh, debugging a patch that didn't apply, or checking whether an edit spliced or relinked.
---

# stack-native live

`nsh/` = UI engine, `ra-jit/` = JIT daemon + live runtime, `app/` = the app.
Read `LIVE.md` at the repo root for the architecture; this file is the
operating procedure.

## Rules for patchable code (`app/src/hot.rs`)

A patch only rewrites functions — never trust it with state:

- Game/persistent state goes through `crate::live::game()` /
  `crate::live::frame()` (anchored in `ra_jit_live::anchor` — dependency
  statics are never re-emitted).
- Per-signal persistence uses `crate::live::keep("name", init)`.
- No `static`, no `thread_local!`, no `#[no_mangle]` data in hot.rs —
  each loses or refuses state (see LIVE.md "State across patches").
- Type-layout changes (`struct` fields) are restart-needed edits — make
  them only to demo the boundary, never mid-session.
- `#[component]` bodies: `#[cfg]`-gated consts, never `cfg!` — `cfg!` in
  macro-expanded code crashes the vendored analyzer at workspace load.
- Colors: `hsla()`/`to_hsla()` consts are fine (shim fixed); plain
  `Hsla {}` literals also work.

## Run

```bash
cd app
./dev.sh        # desktop: cargo watch -w src -x 'ra-jit build' + run
./ios-dev.sh    # sim: watch ios-sim target + pack/install/launch
```

iOS lands on `iPhone 17 Pro` by default (`./ios.sh "<name>"` overrides).
`ios.sh` streams the app console in the foreground — run it `&` or let
`ios-dev.sh` own it.

## Verify (every patch, both platforms)

1. Status bar shows `applied: N functions` and `remounts` increments;
   `up Ns` must not reset. `restart needed:`/`not applied:` = not a patch.
2. `~/.cache/ra-jit/workers/<hash>/log` says `spliced: N functions
   redirected` for BOTH the darwin worker and the ios-sim worker
   (`target:` header distinguishes them). `written again`/`linked again`
   = relink → the running process is orphaned → relaunch it.
3. Screenshot each platform and eyeball the change; never claim state
   survived without seeing the board populated after a remount.

Completion = all three pass on both targets.

## Recovery

- `cargo ra-jit build` prints "could not load the workspace" →
  `pkill -f cargo-ra-jit`, then `cargo ra-jit build` once (cold load
  ~60s); "state written by another build" → run the same command again.
- Sim shows an old UI (e.g. a counter instead of Tetris): the packed
  binary is stale — compare `strings target/aarch64-apple-ios-sim/debug/
  stack-native-app` vs `target/ios-sim/StackNative.app/StackNative`,
  then `./ios.sh` again (it rebuilds, repacks, relaunches).
- Daemon fell back to cargo (no `[ra-jit]` on Finished): the build is
  still valid — it relinks; relaunch the app to pick it up.
