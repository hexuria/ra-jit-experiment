# stack-native — live Rust UI with NSH + ra-jit

An all-Rust app: NSH renders the UI (AppKit on macOS, UIKit on iOS), ra-jit
recompiles edited functions and splices them into the **running** process.
Save a file → both apps update in under a second, state intact, no restart.

## Layout

```
stack-native/
  nsh/      the UI engine (#[component], view(), text(), signals)
  ra-jit/   the JIT: daemon + vendored rust-analyzer + ra-jit-live runtime
  app/      the app: thin shell + hot area + live bridge
    src/lib.rs   root shell: dynamic(remount) + status bar
    src/hot.rs   the patchable part (the Tetris game)
    src/live.rs  poll loop + keep() signals + state anchors
    src/main.rs  macOS entry
    ios.sh       build → pack .app → install → launch the simulator
    dev.sh       cargo watch + run, desktop loop
    ios-dev.sh   cargo watch for ios-sim + ios.sh
    demo/steps/  numbered hot.rs snapshots for the demo
```

## How it's hooked up

```
edit hot.rs ──► cargo watch ──► cargo ra-jit build [--target ios-sim]
                                     │ daemon (vendored r-a compiles fns)
                                     ▼
                       patch file → ~/.cache/ra-jit/live/<LC_UUID>/
                                     │
   running app: Live::poll() (engine thread, every 25ms)
                                     ▼
              mach-o splice: jump over each replaced fn entry
                                     ▼
        live::install sees Applied → snapshot keep()s → remount += 1
                                     ▼
            dynamic rebuilds hot area on new code, state kept
```

- `App` (`lib.rs`) runs once under the root owner — its signals, intervals
  and the `Live` poll are never remounted.
- `HotArea` (`hot.rs`) is the part patches rewrite — every `dynamic` render
  runs the current code generation.
- `Live::poll` runs on the engine thread between tasks — the documented
  safe point (a fn mid-call finishes old, next call runs new).
- iOS sim: a sim process *is* a macOS arm64 process — same splice path.
  `SIMCTL_CHILD_RA_JIT_HOME` points it at the host daemon's live dir.

## State across patches (the part that matters)

Patchable code (`hot.rs`) must not own state:

- `static` in patched code → each patch emits a private copy → empty board.
- `static` with `#[no_mangle]` → matched by name but re-written to its
  initializer on every apply → state reset.
- `thread_local!` → shares on macOS, but **unspliceable on ios-sim** — the
  daemon refuses the patch and relinks instead, orphaning the process.

What works — `ra_jit_live::anchor(name, init)`: a non-generic function in
the `ra-jit-live` **dependency** crate. Dep functions are never re-emitted
into app patches, so its static table is always the executable's own.
`live.rs` uses it for:

- `frame()` — the repaint `Signal<u64>` (created under the root owner).
- `game()` — the `Mutex<Game>` behind `Box::leak`.

`live::keep(name, init)` still works for signal values that survive a
remount (high score etc.) — it rides the executable's TLS on both targets.

Restart boundary (honest): edits that change a public type's layout
(`struct Piece`) → `restart needed: the edit changes 'struct Piece'`.

## Run it

Desktop:

```bash
cd app
./dev.sh          # cargo watch → ra-jit build on save; runs the app
```

iOS simulator:

```bash
cd app
./ios-dev.sh      # cargo watch → ra-jit build --target aarch64-apple-ios-sim
                  #   + ios.sh: pack .app, install, launch
```

Or both targets on one watcher:

```bash
cargo watch -w src -x "ra-jit build" \
            -x "ra-jit build --target aarch64-apple-ios-sim"
```

Demo: `cp demo/steps/01-palette.rs src/hot.rs` → watcher fires → both
apps splice the patch. Steps are cumulative (palette → text → gravity →
ghost → next-box component → cell size → scoring → drops → removals →
levels → struct-layout boundary demo).

## Verify a patch really landed

- App status bar: `applied: N functions in Xms`, `remounts` increments,
  `up Ns` never resets (same process).
- Worker log `~/.cache/ra-jit/workers/<hash>/log`:
  `spliced: N functions redirected` = real patch;
  `the whole object was written again` / `linked again` = relink (relaunch
  needed — running process is orphaned).
- Processes: desktop `ps aux | grep stack-native-app`; sim
  `xcrun simctl spawn <udid> ps aux | grep StackNative`.
- Screenshots: `xcrun simctl io <udid> screenshot out.png`.

## Gotchas learned the hard way

- **`cfg!` inside a `#[component]` fn crashes the vendored analyzer**
  (`ast_id.rs:915: MacroRules in empty MACRO_ITEMS`). Use `#[cfg]`-gated
  consts instead (`SHOW_NOTE` in lib.rs).
- New daemon binary → first `cargo ra-jit build` says "state written by
  another build" → run it again; the fresh worker loads on the second call.
- Daemon deep-down: `pkill -f cargo-ra-jit` then one `cargo ra-jit build`
  cold-loads the workspace (~30–60s).
- Dep changes (`live.rs`, `ra-jit-live`) force a full relink → relaunch
  both apps; only `hot.rs` edits are clean patches.
- `simctl launch --console-pty` blocks streaming the console — use
  `xcrun simctl launch` (no flag) in scripts, run `ios.sh` in background.
- `f32::max`/`min`/`clamp` in const eval is fixed locally (vendored
  `shim.rs`); `to_hsla()`/`hsla()` consts are patchable again.
- Real iOS devices: impossible — no writable+executable pages. Sim only.
- Each target triple gets its own daemon worker (~4.5GB RSS each).

## Upstream pins

- `nsh/` — longbridge/nsh @ `c2f019e6d46cd1bdd191dd736d4443fe6fcb25b9`,
  unmodified.
- `ra-jit/` — indexable-inc/ra-jit @ `4a8d2b5` plus the `live-ios-sim-1023`
  work: const-eval min/max intrinsics, ios-sim live patching,
  `ra_jit_live::anchor`, richer panic context. Its full git history
  (upstream + those commits) is this repo's `main` branch.
- `app/` — original code, this repo only.
