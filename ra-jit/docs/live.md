# Live edits

A program that links `ra-jit-live` takes each edit while it runs. This is what happens between
the save and the new code running, and what is refused.

## The two halves

- **The daemon** (`crates/ra-jit/src/live.rs`, `crates/ra-jit/src/splice/macho.rs`). Every edit
  it answers with a patch (the functions that changed, linked against the executable of the
  last full link) is spliced into the executable on disk as ra-jit always did. If a program
  that runs that executable has said where it wants patches, the daemon also lays the same
  patch out for that place (`Patch::place_live`) and writes it next to the program.
- **The program** (`crates/ra-jit-live`). `Live::start` reads the executable's `LC_UUID`,
  sets aside 512 MB right behind the image (within `adrp`'s 4 GB, so new code reaches all of
  the old), says so in a file, and watches for patches on a thread. `Live::poll`, called
  between frames, applies the newest one; `Live::apply_as_they_come` applies each on that
  thread the moment it arrives instead.

They meet in `~/.cache/ra-jit/live/<uuid>/`: `process` (written by the program: pid, uuid,
when it started, the next free address and the end of its room), `patch` (written by the
daemon, `RAJITLV1` followed by the encoding in `crates/ra-jit-live/src/format.rs`) and `wake`,
a datagram socket the program listens on, which the daemon sends a byte once it renamed a new
`patch` into place (the program also looks at `patch` every 30 ms, for a daemon that could not
wake it). The directory is keyed by the UUID because the daemon's path for the executable and
the program's (cargo's hard link in `target/debug`, a clone on macOS) differ.

## Edits from an editor

An editor need not save for the daemon to build. `ra_jit_live::edit(path, text)` sends the
daemon (`daemon.sock`) one line, `edit\t<path>\t<text>` with backslash, newline, tab and
carriage return escaped, the way an editor tells a language server `didChange`. The daemon
hands it to the worker that has the file's checkout loaded; the worker sets the text into
rust-analyzer's database, builds the programs that have the file from it, publishes the patch
and answers as it answers `cargo ra-jit build` (`ok patch=... ms=N`, `ok unchanged`, `error:
...`). An edit that comes while the last one of the file is being built waits, and one typed
over before it was built is answered `ok superseded`. The editor writes the file afterwards;
the worker sees the same text it built and builds nothing for the write. `cargo ra-jit build`
is still the first build and link, and works as before. The Game of Life demo
(`demo/life`) does this on every key, and `harness/live_latency.py socket` times it.

## Applying a patch

1. Map the text at its base, slide its pointers by the program's ASLR slide, make it
   read-and-execute and invalidate the instruction cache. Map the data the same way.
2. Register its unwind information (`__register_frame`, one FDE at a time), so a panic in new
   code unwinds through it.
3. Write the jumps over the old entries and the statics' new values. A page that is writable
   is written in place; any other gets a copy that is written, set to the original protection
   and swapped in with `mach_vm_remap(FIXED | OVERWRITE)`, since macOS refuses a write to text.
4. Put back what an earlier patch wrote and this one no longer does: the daemon sends the
   linker's original bytes for every place a splice ever wrote, because a program started from
   a spliced file has the splice's bytes, not the linker's.

Each patch is laid out behind the last: code of an earlier one may still be on some thread's
stack.

## Calls into the old executable

In the splice and in a live patch, a call or an address taken of a function the executable
already has goes to its original entry, never to a copy a previous splice appended. The entry
is where the jump is written, so a later edit of that function reaches every caller.

## What is refused

The program is told to start again (`Outcome::Restart`) when:

- an edit changes a type the rest of the program sees: the daemon hashes each file's type
  definitions (structs, enums, unions, aliases, traits without default bodies, and `repr`)
  before and after, and a program started before the change can't have its live data
  relaid;
- the edit wrote the whole object again, which the daemon does when it has no compiled base
  to patch against (the first edit after it starts);
- the executable was linked again: the daemon says so for a program it knew, and the program
  watches its own executable for another `LC_UUID`, which covers a daemon started since and a
  plain `cargo build`.

A patch that can't be applied (`Outcome::Declined`) leaves the program as it was.

## Tiers

The same patches also make the program faster while it runs: the daemon asks it to sample
itself, compiles its hot functions again with LLVM and publishes them as patches like an
edit's. See [tiers.md](tiers.md).

## Signing

On macOS the linker signs ad hoc. A package with

```toml
[package.metadata.ra-jit]
entitlements = "jit.entitlements"
```

gets `codesign --force --sign - --entitlements <file>` run by the daemon's link command on its
own copy of the executable, which cargo then copies into `target/debug`. A splice reads
`codesign`'s superblob (the code directory, requirements, entitlements, DER entitlements and
the empty CMS blob), keeps every blob but the directory, and rewrites the directory's page
hashes with its page size (4 KB from the linker, 16 KB from `codesign`). An executable signed
again between two splices is read again, and the hashes of the pages the last splice put back
are recomputed from what it put back.

## In den

den's native app takes edits to its own code this way: `just dev --ra-jit` in den builds it
through the daemon, starts it with `DEN_RA_JIT=1`, watches its crates, builds again after an
edit, and starts it again when it is asked to. den's module crates keep their own hot reload,
which ra-jit does not touch. Measured on 2026-10-20 on an M-series Mac:

- a cold build of den-native (5,076 of 5,193 functions of its own compiled, 190,067 instances)
  takes about 70 to 95 seconds, once per daemon;
- an edit to a function body is spliced in 1.0 to 1.5 seconds from the save and applied by the
  running program in 0.4 to 0.6 ms, with no restart;
- seven crates of den set `overflow-checks = false` for speed, and den's build passes `--config`
  to make them agree with the rest for the daemon, without which most of den-native went to
  rustc.

## Latency under typing

`harness/live_latency.py` times an edit from the keystroke to the running program having code
at least as new, on four programs that link `ra-jit-live`: `harness/live` (probe, a generic
function and a struct), `demo/life` (the Game of Life, headless), `harness/live-bevy` (bevy
0.20, 600 lines of generic systems, components and messages) and `harness/live-egui` (egui
0.37, 580 lines of generic widgets over an invoice ledger, a frame every 11 ms). Three modes
time edits ([#9](https://github.com/indexable-inc/ra-jit/issues/9)):

- **socket**: one edit at a time, the whole file sent as the demo's editor sends it, each
  waited for;
- **burst**: 20 keys 30 to 100 ms apart, each of which compiles;
- **replay**: scripted edits typed one key at a time, 30 to 100 ms a key, longer at the end of
  a word, with a typo and its backspace now and then, the whole file sent on each key with one
  edit in flight. Most texts in between do not compile; n is the keys whose text does. A key
  is run by a patch, or by a restart: a program told to restart, or whose executable the daemon
  linked again, is built (`cargo ra-jit build`) and started again as a launcher would, and the
  keys up to the text built are timed to it.

Measured on 2026-10-20 on a MacBook Pro (Mac17,6, Apple M5 Max, 18 cores, 128 GiB, macOS
27.0.1) on AC power at 100%, ra-jit at `ad07271066` (the second bevy pass and the later
passes at `140f90a582`, which changes only the harness). Two passes per program in opposite
orders, each with a daemon, home and store of its own; other agents' builds held the load
average at 11 to 56 throughout, so these are a busy laptop's numbers. All times in ms; every
sample is in [benchmarks/live](benchmarks/live), and each run is a line in
`benchmarks/history/macos-arm64-live.jsonl` (`harness/benchhistory.py live`).

| program, mode | n | p50 | p90 | p95 | p99 | max |
|---|---|---|---|---|---|---|
| probe, socket | 40 | 8.8 | 39.1 | 58.0 | 58.9 | 58.9 |
| probe, burst | 200 | 7.4 | 24.2 | 40.0 | 62.9 | 79.1 |
| probe, replay | 173 | 364 | 1,242 | 1,629 | 2,674 | 2,813 |
| life, socket | 40 | 13.0 | 32.1 | 34.8 | 35.2 | 35.2 |
| life, burst | 200 | 13.2 | 23.8 | 28.9 | 46.4 | 64.7 |
| life, replay | 462 | 651 | 3,097 | 11,238 | 16,955 | 18,310 |
| bevy, socket | 40 | 20.4 | 50.8 | 52.6 | 64.7 | 64.7 |
| bevy, burst | 200 | 20.4 | 49.7 | 78.0 | 203 | 224 |
| bevy, replay | 180 | 2,028 | 7,078 | 9,150 | 14,569 | 14,734 |
| egui, socket | 40 | 23.4 | 67.4 | 88.9 | 110 | 110 |
| egui, burst | 200 | 20.1 | 62.7 | 76.1 | 106 | 119 |
| egui, replay | 84 | 1,590 | 8,718 | 9,535 | 10,271 | 10,271 |

The passes agree within about 2x at p50 (socket: probe 7 and 18, bevy 15 and 34; replay: probe
441 and 324, life 695 and 592, egui 1,903 and 1,392) and closer at p95. The replay keys split
by what ran them:

| program | keys typed | compile | edits sent | run by a patch | by a restart | restarts | restart p50 (max) |
|---|---|---|---|---|---|---|---|
| probe | 488 | 174 | 469 | 5 (p50 9.8) | 168 | 83 | 130 (312) |
| life | 1,746 | 462 | 1,308 | 129 (p50 245) | 333 | 83 | 380 (1,806) |
| bevy | 1,060 | 180 | 837 | 54 (p50 55) | 126 | 12 | 290 (446) |
| egui | 1,172 | 84 | 661 | 0 | 84 | 21 | 367 (512) |

A key that is typed over before it is sent is not sent; whether its text compiles is asked of
the daemon after the round, off the clock. One probe key was never run.

**Restarts.** `restart` sends a session of edits one at a time, mostly bodies and a few type
changes, and times each restart (the edit, `cargo ra-jit build`, the program to its first
answer) against plain incremental `cargo build` of the same source and its start (`cargo
run`), in a target directory of its own:

| program | session edits | answered Restart | ra-jit restart p50 (max) | `cargo run` p50 (max) |
|---|---|---|---|---|
| probe | 16 | 4 of 4 type changes | 186 (202) | 370 (401) |
| life | 24 | 4 of 6 type changes | 451 (484) | 1,424 (2,014) |
| egui | 20 | 6 of 6 type changes | 405 (524) | 1,136 (1,275) |
| bevy | 6 | none: refused | `cargo ra-jit build` fails | 2,303 and 1,631 |

Every body edit of a session was applied live (probe 4 to 9 ms, life 11 to 29, egui 13 to 78).
life's two type changes that did not restart add a struct no running code has values of.
The ra-jit restart is mostly the start: the build takes 15 to 90 ms, the program 110 to 400
ms to its first answer (it reserves its patch room and reads its executable first).

**What fails under typing.**

- After a key that does not compile, the next one that does was not patched in these
  measurements: the text that did not compile was built whole, the executable linked again
  and the running program told to restart, and the next key was a patch of that build, so
  nearly every key was run by a restart: 168 of 173 on probe and all 84 on egui
  ([#31](https://github.com/indexable-inc/ra-jit/issues/31)). Now a text that does not
  compile is not built whole, and a whole build refused once it is made is undone, so the
  next key that compiles is a patch of the last build that did. One replay pass of each,
  before (`5f26408566`) and after, on 2026-10-21 on the same MacBook under other agents'
  load: probe 2 of 29 compiling keys run by a patch before and 29 of 29 after, p50 964 to
  19.8 ms, p95 2,260 to 58.2; egui 0 of 14 and 14 of 14, p50 6,508 to 41.7, p95 10,758 to
  84.8; life's keys run by a patch 46 to 50 of some 74, p50 504 to 39.2, p95 1,875 to 393.
  life's other keys are restarts the worker asks for (``the edit changes `struct Life`, which
  the running program may have values of``), before as after.
- The program is not always told. The daemon relinks its own copy of the executable; the copy
  in `target/debug`, whose `LC_UUID` `ra-jit-live` watches, changes only with a build through
  cargo. probe's program said `restart needed` for 36 of 82 relinks; the harness restarts on
  the answer too, which an editor's launcher would also have to.
- bevy's `main` is left to rustc (`callee add_systems: ... writable static INTERNER without a
  rustc symbol`). Every edit that reaches it is answered `` error: `main` is not compiled by
  ra-jit`` though the text compiles: 108 of its 180 compiling keys, run only by the restart
  after the next key ra-jit takes. A type change its systems use (a field of `Ship`) makes
  `cargo ra-jit build` itself fail with the same error, so the program cannot be started again
  through ra-jit at all; the session stops there, and plain cargo builds the same source in 1.3
  to 2.0 s.
- `error: no program built has <file>` answered 22 keys on probe and 15 on egui, right after a
  relink, while the restart's `cargo ra-jit build` and the next edits arrived together
  ([#32](https://github.com/indexable-inc/ra-jit/issues/32)). The program was being built
  for that build, in the background, and an edit asked meanwhile saw it as never built; now
  it waits for that build. The passes above went from 4 such answers on probe and 1 on egui
  to none.
- life has errors ra-jit has no diagnostic for (`` `step` has an error ra-jit cannot say more
  of: frontend: mir: IncompleteExpr``, `UnresolvedName`): the text does not compile, and the
  answer has no line to show.
- egui's error answers take 13 to 21 ms at p50 but 320 to 390 ms at p90, up to 1.6 s: an
  unclosed string literal while retyping a `format!` swallows the rest of the file, the worker
  checks some 3,200 functions again, and the worker tells the program to restart ("the edit
  changes what the rest of the program sees") for texts that never build (9 to 11 times a pass). Keys queue
  behind those answers.

Not measured yet: keystroke to changed pixels, and these modes on the CI runner
(`.github/workflows/live-latency.yaml` runs `socket burst replay` on all four programs).
