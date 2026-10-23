# Benchmarks

The README's benchmark tables come from here. Each is a matrix over three platforms, macOS
arm64, Linux aarch64 and Linux x86-64, each measured on a dedicated machine of its own, on real
projects at pinned commits, against plain cargo on the same machine, and written back to the
README by a bot commit. A platform's numbers compare with each other and with its own history,
not with another platform's: the machines differ in more than their architecture. This page
says how, so that anyone can check it or run it again.

## What is measured

`harness/bench.py` copies each project's pinned checkout and builds it two ways side by side:
`cargo ra-jit build` (the daemon) and plain `cargo build` into a target directory of its own.
Both build the same source after every edit, one after the other, so they see the same machine
state.

| Phase | What happens | ra-jit | cargo |
|---|---|---|---|
| Cold | build from an empty target directory and an empty ra-jit store; crates are already downloaded | the build returns (cargo builds the first time) and then the daemon loads the workspace in the background; both are reported | `cargo build` |
| Cold, warm store | after everything else, `--cold-runs` times: a second copy of the project with an empty target directory and a daemon of its own, but the store the measured copy filled with its cold build and edits (as a second checkout of the same project, or a teammate's store, would have) | build and daemon ready, as cold | |
| Nothing changed | build again with no edit | | |
| Edit a function | one statement is added to a function in the main crate | | incremental |
| Edit a widely used function | one statement in a function many places call: in a library crate of the workspace when there is one (ripgrep's `grep-matcher`, axum's `axum-core`, nushell's `nu-path`, bat's library), otherwise in a module the rest of the crate uses (fd, hyperfine, just) | | incremental |
| Change a signature | a method gets another parameter and every caller passes it | | incremental |
| Edit a generic function | a statement in a generic function used at many types (ripgrep and nushell) | | incremental |
| Add a function | a new function in the main crate, called from the function of "Edit a function" | | incremental |
| Add a struct field | a struct gets one more field (`_bench_edit: ()`) and every place that builds it sets it; in a library crate where there is one (nushell's `nu-protocol::Alias`, built in `nu-parser`) | | incremental |
| Tests | (`--tests`) the project's test suite, or the crates of it named in `PROJECTS`, after the edits: `cargo ra-jit test` and `cargo test`, run twice each, the second timed; every test has to end the same way under both | | |
| Runtime | (ripgrep) the program ra-jit linked, cargo's debug build and cargo's release build run the same search over the toolchain's own library source | | |

The edits do not change what the program does. After the first edit of each kind the program
ra-jit linked is run and has to print what cargo's prints (exit status, stdout and stderr); in
the runtime phase all three programs have to print the same. A run in which any program differs
is not published (`benchreport.py` refuses it and the workflow fails).

Each edit is made and taken back `--runs` times (5 in CI). The README shows the median of the
edits; this page also shows the fastest and slowest run of each side, and the raw JSON keeps
every sample, the time of taking each edit back, who built each step (the daemon, or cargo and
why), whether the daemon spliced the change into the program or linked it again, how many
functions it compiled, the memory of both sides (below) and the daemon's own timeline of each
build.
Cold is `--cold-runs` runs of each (5 in CI), each with a new target directory, home and store;
the store size after it is kept too.

An edit the daemon hands to cargo (a fallback: ra-jit always builds what cargo would) is measured
once, marked `(c)` in the README, and shows about cargo's time.

## Memory and disk

Every build in the table is also measured for memory, on both sides, and so is the disk each
side leaves behind. The README shows a summary under the time table; this page has all of it.

| Number | ra-jit | cargo |
|---|---|---|
| Peak, each phase and edit | the most `cargo ra-jit build`, the daemon and every process under either held at once: cargo when the client hands it the build, its rustc, build scripts and linker, the daemon's workers and what they run | the most `cargo build`, rustc, build scripts and the linker held at once |
| Largest process | the peak of the largest of those processes on its own | the same |
| Daemon between edits | what the daemon and its workers hold once it is ready, after nothing changed, after each kind of edit and at the end of the run, with the number of builds so far: memory that grows with every edit shows here | |
| On disk | the target directory, the store and ra-jit's home | the target directory |
| Cold, warm store | the peak, and what the daemon holds once ready | |
| Runtime | the peak of the program ra-jit linked while it runs | the peak of cargo's debug and release programs |

`harness/runbench.py` (runtime parity) adds the peak of each program while it runs, the size of
each binary, and the peak of each build: `cargo build --release` against ra-jit's whole-program
LLVM build.

**How.** One helper, `harness/memory.py`, measures all of it, and every harness that reports
memory uses it. It combines two readings:

- **The exact peak of one process.** When the harness waits for a command (`wait4`), the
  kernel reports the largest resident size any single process of what it waited for reached
  (`ru_maxrss`; KiB on Linux, bytes on macOS). This is exact, but it is one process: cargo's 16
  rustc at once count as the largest of them.
- **The sum over a tree, sampled.** A thread reads what every process under the command, and
  under the daemon, holds every 10 ms (`RA_JIT_MEMORY_INTERVAL`) and keeps the largest sum. On
  Linux that is the resident size, from `/proc/PID/statm`, with the children of each thread in
  `/proc/PID/task/*/children`. On macOS it is the physical footprint, from
  `proc_pid_rusage`, with `proc_listchildpids`: what Activity Monitor shows, the memory a
  process owns, still counted when macOS has compressed it or swapped it out. Resident size
  would not do on a Mac: when memory is short macOS compresses the pages of a process that is
  idle, and the daemon between two edits read 119 MB on nushell on a Mac whose memory was full,
  where it held 7 GB a minute before. The kernel also keeps each process's peak footprint,
  which the sampler reads as the largest of that process. Each process it saw stays watched,
  so the daemon, which the first build starts and which then outlives it, is followed with its
  workers.

The peak reported is the sum, and never less than the peak of the largest process. What the
daemon holds between edits is one reading of its tree when it is idle.

**Error.** The sum can miss a rise shorter than 10 ms and a process that lives less than
10 ms (a short `cc` or build script): it reads low by at most what such a process held. On
Linux it can also read high: a page of a shared library that several processes have mapped
counts once in each (resident size, not proportional size). That is mostly libLLVM and
`librustc_driver`, so it inflates cargo's peak with many rustc at once more than ra-jit's,
whose daemon is one process with a few workers; proportional size costs a walk of every
mapping per reading. The footprint macOS counts leaves those pages out. A Linux number and a
macOS number are therefore not the same count; within one machine both sides are.

**Cost.** The sampling thread wakes every 10 ms and reads a few dozen small files or calls;
while it runs, Python switches threads every 0.1 ms instead of 5 ms, so the end of a timed
command is seen within that. On the MacBook of the macOS results, a no-op `cargo build` of ra-jit itself took a
median 322 ms without it and 319 ms with it (30 runs each, taking turns): no cost the noise
shows. Two runs of the Mac's measurement gave the same peaks within 1%.

**Disk** is the blocks the files use, a file with several hard links once
(`memory.disk_mb`). On Linux each block is counted once however many files share it: a
copy the file system made by sharing the blocks (a reflink, which `std::fs::copy` makes on
XFS and Btrfs, and which ra-jit's copies of rustc's objects and libraries are) takes no
more disk than the one file, and is counted so, where `du` counts it once per file. The
kernel says which blocks are shared (`FIEMAP`); where it does not, a file is counted as
`du` counts it. On macOS a clone is counted as `du` counts it, once per file.

The history (`rss_mb`) still has the daemon's workers' high-water marks summed, as before; the
history's memory alerts check the new numbers too (below).

Each platform's memory tables are with the rest of its numbers, under [Results](#results).

## Projects

Pinned by tag and commit in `harness/bench.py` (`PROJECTS`); the clone has to be at that commit
or the run stops.

| Project | Tag | Commit | Run to check the output |
|---|---|---|---|
| ripgrep | 15.1.0 | `af60c2de9d85` | `rg -n --no-heading --sort path "fn main" ...` |
| fd | v10.3.0 | `d38148f0aabd` | `fd --extension rs main src` |
| bat | v0.26.0 | `c734087e1d15` | `bat --plain --color=never rustfmt.toml` |
| hyperfine | v1.19.0 | `12fec4209864` | `hyperfine --help` |
| just | 1.43.0 | `b7e7c46495b3` | `just --list --unsorted` |
| axum | axum-v0.8.6 | `c1bb9c3e9062` | built (`-p example-hello-world`), not run: it is a server |
| nushell | 0.108.0 | `da141be11b3a` | `nu -n -c "[3 1 2] \| sort \| str join ','"` |
| helix | 25.07.1 | `a05c151bb6e8` | `hx --version` (grammars are not built: `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1`) |
| bevy | v0.20.0 | `b3bdd75becaf` | built (`--example breakout`), not run: it opens a window |
| zed | v1.24.4 | `aed37547d854` | `zed --help`; not in the CI run, see below |

zed (1.7 million lines in 249 crates, 1,291 crates with its dependencies) is in `PROJECTS` and
every edit of it builds, but it is not measured yet: ra-jit's daemon does not fit it in 123 GB.
On a c8i.16xlarge (2026-10-21, ra-jit `2b2f2dcd5b`) the daemon loaded the workspace (20 s),
its 1,756,350 symbols (26 s) and resolved the crates (25 s), then went from 20 GB to more
than 111 GB in 40 seconds and was stopped at a 112 GB limit before it was ready; cargo alone
builds it in a few minutes on the same machine. Its build needs `nix develop .#bench-zed`.

`cargo` 0.92.0 is in `PROJECTS` too but not in the CI run (it doubles the run's length).

## Fairness

- Same machine, same source, same moment: the two builds alternate after every edit.
- Same toolchain on both sides: Rust 1.99.0 (`harness/toolchain`), from the flake's
  `devShells.bench`, which also pins LLVM 20 for the LLVM tier.
- Cargo is warm and incremental: the dev profile's default (`CARGO_INCREMENTAL` unset).
- Same linker: the toolchain's default (`cc`, which runs GNU ld on the benchmark machine). No mold,
  no lld, no sccache or other `RUSTC_WRAPPER` on either side (bench.py removes it).
- Crates are downloaded before the cold build on both sides; downloading is not timed.
- ra-jit's store starts empty for each project and backend and for every cold run. "Cold, warm
  store" is the one phase that starts with a store, and it is reported apart. On a machine
  the store is one for every project, so the Store column is what a project adds to a store
  that has nothing of it. `store_added_mb` in the results is what it adds to one that has
  what the projects before it in the run stored: the disk of its entries whose keys none of
  them had (a pack's size shared among its entries by the length of their values).
- The machine runs nothing else: it is a dedicated instance used only by this workflow.

## Tiers

Each project is measured with each code producer ra-jit has: Cranelift (the default, in the
README) and LLVM (`RA_JIT_BACKEND=llvm RA_JIT_OPT=2 RA_JIT_EMIT=fast`). Cranelift is measured
with ra-jit as it is installed and released, without the `llvm` feature (`RA_JIT_BIN`), and LLVM
with a build that has it (`RA_JIT_LLVM_BIN`): a binary that links libLLVM loads it each time it
starts, about 6 ms on the benchmark machine, and every `cargo ra-jit` starts it once. A new tier
is one more entry in `BACKENDS` in `harness/bench.py`; the report makes a table for every
backend in the result.

## The fastest setups

With `--setups` (all of them in the nightly run, `--setups-on ripgrep,nushell`), every step plain
cargo builds is built again by each of the fastest ways to build Rust there already are, each
into a target directory of its own, right after plain cargo, on the same source:

| Setup | What it is |
|---|---|
| mold | cargo with mold 3.0 as the linker (`-C link-arg=-fuse-ld=mold`) |
| wild | cargo with wild 0.10 as the linker (`-C link-arg=-fuse-ld=wild`) |
| `-Zthreads=8` | rustc's parallel front end, on the same 1.99.0 (`RUSTC_BOOTSTRAP=1`) |
| nightly | plain cargo on the nightly below, to tell the nightly's own difference from Cranelift's |
| cg_clif | rustc_codegen_cranelift (`-Zcodegen-backend=cranelift`), on the nightly: it ships with nightlies only |
| cg_clif + wild + `-Zthreads=8` | all three at once, the fastest cargo build there is |
| `cargo check` | builds no code, so there is nothing to run after it: the floor of what rustc does |

The flags go in through `--config target.'cfg(all())'.rustflags`, which cargo adds to each
project's own (helix sets `--cfg tokio_unstable` there; `RUSTFLAGS` would replace it).
`-Zshare-generics` is not a column: it is already on in every debug build. The nightly is
`rust-bin.nightly."2026-10-09"` with `rustc-codegen-cranelift-preview`, from the flake
(`RA_JIT_NIGHTLY`). A setup that cannot build a project is reported as failed and not timed
after that.

Hot-patching a running program is compared on its own (`harness/hotpatch.py`): a small program
that calls one function every millisecond through `subsecond::call`, edited 30 times by each of
Dioxus's `dx serve --hotpatch` (subsecond, which patches the running program on Linux),
`cargo ra-jit build` and incremental `cargo build`, the last two followed by starting the program
again (ra-jit patches a running program on macOS only). Each edit is timed from writing the file
to the program running the new code. It is a small program on purpose: subsecond needs the
program to call through `subsecond::call` and to keep running, which a command-line tool like
ripgrep does not. The restart flows stop the program before building, as `cargo watch` does.

On Linux (dev-aws-7, 16 cores of a c7i.16xlarge, the bench shell), 30 edits each, milliseconds
from the save to the new code running ([raw JSON](benchmarks/hotpatch/linux-x86_64.json)):

| Tool | median | p90 | min | max | of which the build |
|---|---:|---:|---:|---:|---:|
| subsecond (`dx serve --hotpatch`), patched, 0 restarts | 289 | 463 | 283 | 535 | |
| `cargo ra-jit build`, then a restart (all 30 built by the daemon and spliced) | **40** | 72 | 39 | 78 | 39 |
| `cargo build`, then a restart | 361 | 413 | 353 | 427 | 360 |

On a program this small, starting it again is nearly free, so ra-jit's build plus a restart
beats subsecond's patch of the running program; a program with state to rebuild after a
restart would lose that. On a MacBook Pro (M5 Max, 18 cores, on AC, other work running: a load
of 7 to 28), the same 30 edits each ([raw JSON](benchmarks/hotpatch/macbook.json)):

| Tool | median | p90 | min | max | of which the build |
|---|---:|---:|---:|---:|---:|
| subsecond (`dx serve --hotpatch`), patched, 0 restarts | 346 | 417 | 320 | 421 | |
| `cargo ra-jit build`, then a restart (all 30 built by the daemon) | 469 | 527 | 410 | 841 | 299 |
| `cargo build`, then a restart | 520 | 593 | 486 | 651 | 365 |

On macOS ra-jit also patches a running program without a restart: a program that links
`ra-jit-live` (`harness/live`, as small as this one) takes an edit in a median of 12.5 ms (p90
28.0, p99 48.9) when it goes through `cargo ra-jit build`, and 4.0 ms (p90 11.6, p99 13.8) when an
editor sends it over the daemon's socket (`harness/live_latency.py --program probe cargo socket`,
30 edits each, same MacBook, [raw JSON](benchmarks/live/macbook-probe-cargo.json)). That is the
flow to set against subsecond's; the restart flow above is what ra-jit does where it cannot
patch, today Linux.

## The machines

Every result records its machine and platform: the name it was given (`RA_JIT_BENCH_MACHINE`),
the CPU, the cores the run could use (it may be pinned with `taskset`) and the machine's total,
memory, OS, kernel, rustc, C compiler, linker, LLVM version and the load average when it
started (below and in the JSON). The platforms, their runners' labels and how a result is
filed are `harness/benchplatforms.py`.

Each platform is meant to run on a dedicated machine that runs nothing else, registered as a
self-hosted GitHub Actions runner. A shared runner is no use: GitHub's hosted macOS arm64 runner
(`macos-15`) is a 3-core VM with 7 GB of memory, less than nushell's daemon holds, with
neighbours that move the numbers. The three:

| Platform | Runner labels | Machine | Why |
|---|---|---|---|
| Linux x86-64 | `self-hosted, linux, x64, bench-x86` | `dev-aws-bench-x86` (i-01666717f0de39201, us-west-2a), resized to m8a.4xlarge: 16 AMD EPYC 9R45 (Zen 5) cores, one thread per core, 64 GiB | already launched for it, on demand, ttl a year, tagged `ix-dev-purpose=ra-jit-bench`; launched as a c8a.4xlarge (32 GiB), too little for nushell's daemon (15 GB) beside cargo |
| Linux aarch64 | `self-hosted, linux, ARM64, bench-arm64` | a new `dev-aws-bench-arm64`, m8g.4xlarge: 16 Graviton4 (Neoverse V2) cores, 64 GiB, on demand | the same core count and memory as the x86 machine; a cloud VM is not bare metal, but its cores are not shared with other tenants |
| macOS arm64 | `self-hosted, macOS, ARM64, bench-macos` | a Mac mini M4 Pro (14 cores, 64 GB) kept for it, plugged in, not sleeping (`sudo pmset -a sleep 0`), or an EC2 mac-m4pro.metal host if it has to be in AWS | bare metal, and the only way to measure macOS: a MacBook in use is busy with other work |

`harness/benchrunner.sh PLATFORM MACHINE` sets a runner up: it checks the platform is the
machine's, installs the runner in `~/actions-runner`, registers it with the platform's labels and
runs it at boot (on Linux as user `andrew` from a user systemd unit,
`ra-jit-bench-runner.service`, lingering; on macOS as the runner's launchd service), and its
`.env` names the machine for the results. It needs Nix with flakes on the machine:

```
scp harness/benchrunner.sh HOST:
gh api -X POST repos/indexable-inc/ra-jit/actions/runners/registration-token --jq .token \
  | ssh HOST bash benchrunner.sh linux-aarch64 "'AWS m8g.4xlarge (dev-aws-bench-arm64)'"
```

Then name the platform in the repository variable `BENCH_RUNNERS`, comma-separated, which turns
its jobs on: `gh variable set BENCH_RUNNERS -R indexable-inc/ra-jit -b linux-x86-64,linux-aarch64,macos-arm64`.
A platform it does not name is skipped by every run rather than left waiting for a runner that
is not there, and the README keeps its last numbers, under their machine, date and commit.
(`BENCH_RUNNER=true`, from before there were platforms, still means Linux x86-64.)

Until the runners exist the numbers are measured by hand with the same harness, pinned to 16
cores with `taskset` so that cargo and the daemon have the 16 cores of the machines above. The
edits: on 2026-10-22, Linux x86-64 on `dev-aws-8` (a c7a.48xlarge, AMD EPYC 9R14, idle and
claimed for the run) and Linux aarch64 on a c8g.16xlarge (Graviton4, Neoverse-V2) launched for
it; macOS arm64's are the MacBook Pro (M5 Max) run of 2026-10-21, made while it was busy with
other work (its load average is in the README), so its times are noisier than the others'. The
runtime, each benchmark run 10 times on 2026-10-23: Linux x86-64 on `dev-aws-11` (a bare-metal
c8a.metal-24xl, AMD EPYC 9R45, idle and claimed for the run) and Linux aarch64 on a c8g.8xlarge
(Graviton4) launched for it. macOS arm64's runtime is one run on GitHub's `macos-15` runner, a
shared virtual M1 with 3 cores and 7 GiB, whose runs of a benchmark have varied by up to 17×
(eza, which reads the disk), so the README marks every benchmark whose runs varied by more than
1.5× with ≈. The first published run (2026-10-20) was on `dev-aws-11` too.

## When it runs and what it commits

`.github/workflows/bench-readme.yaml` runs on every push to main that touches code (not docs,
the site or the README), every night at 09:17 UTC, and by hand (`gh workflow run bench-readme
--repo indexable-inc/ra-jit`, with `-f force=true` to write the README even when nothing moved).
One run at a time; a push during a run waits, and a newer push replaces the waiting one. A run
has a job per registered platform, on its runner, at the same time, and then one job that
publishes them all.

That job hands each platform's result to `harness/benchreport.py`, which compares it with the
stored one of the same platform (`docs/benchmarks/<platform>.json`, e.g. `linux-x86-64.json`;
`runtime-<platform>.json` for the runtime). It replaces the stored result, and writes the
README's tables and this page's results from the newest stored result of every platform, only
when:

- a median of either side, in any phase, moved by more than 10% and more than 50 ms,
- a step the daemon built is now built by cargo, or the other way round,
- a peak of either side moved by more than 10% and 100 MB,
- a project or backend was added or removed, or
- the stored run is more than 24 hours old (so the commit the README names stays within a day
  of main).

It then commits those files to main as `github-actions[bot]` with `[skip ci]` in the message (so
the commit starts no other workflow), once for every platform, rebasing onto main if main moved
during the run. It uses the job's `GITHUB_TOKEN` with `contents: write`; main has no branch
protection. `bench-bun.yaml` does the same for bun, nightly, on the same runners: bun takes
hours, so it has a workflow and a stored result of its own (`docs/benchmarks/bun-<platform>.json`
and `runtime-bun-<platform>.json`, written by `benchreport.py --name bun`), but it is a project like
the others in the README's tables and charts and in their geometric means. Each table folds a
platform's bun result into that platform's own (`benchreport.py`'s `stored_all`), whichever
workflow wrote last, and a footnote names bun's machine, commit and date; its every number is
under "macOS arm64, bun" and so on below.

## History and regressions

Every run of the workflow appends one line per platform to
`docs/benchmarks/history/<platform>.jsonl` (`harness/benchhistory.py build`), whether or not the
README changed: the medians, spreads, who built each step, memory and store size of each project
and phase, the platform, the machine and the run's link. The site's
[/bench](https://ra-jit.ix.dev/bench) page charts those files, a platform and machine at a time,
and the workflow starts the site's deploy after it commits.

`benchhistory.py check` then compares the new line with the median of the last seven from the
same machine with the same cores. A number regressed when it is above that median by more than
its noise band and by more than a floor: the band is 10% or three times the spread the history
shows (1.4826 times the median absolute deviation), whichever is larger, and the floors are
50 ms for times, 100 MB for memory and 50 MB for the store and disk. Memory checked is
ra-jit's: its peak in each phase, what the daemon holds after each phase, once ready and at the
end of the run, and its disk; cargo's memory is charted, not checked. A step the daemon built that cargo
builds now is a regression whatever its time. On a regression the workflow opens an issue
labelled `bench-regression` with a table of what moved (or comments on the one already open),
writes the table to the run's summary and fails.

Live edits are checked the same way. `.github/workflows/live-latency.yaml` runs every night on
main on a GitHub macOS arm64 runner (`macos-15`), times socket, burst and replay edits of the
four live programs (`harness/live_latency.py`, [live.md](live.md#latency-under-typing)), appends
one line for all of them to `docs/benchmarks/history/macos-arm64-live.jsonl` and checks the p50
and p90 of each program and mode (floor 1 ms). A shared runner is noisier than the benchmark
machine; the band widens with the spread its own history shows.

## Run it yourself

On any of the platforms with Nix (or with Rust 1.99.0, rust-src, LLVM 20 and Python 3 installed):

```
nix develop .#bench
cargo build -p ra-jit --release
CARGO_TARGET_DIR=target-llvm cargo build -p ra-jit --release --features llvm
python3 harness/bench.py ripgrep fd --runs 5 --out /tmp/bench.json   # any of the projects
python3 harness/benchreport.py /tmp/bench.json --check               # what would change
```

`--backend cranelift` measures one tier. `--cold-runs 5` repeats the cold builds, `--setups
all --setups-on ripgrep,nushell` adds the setups above, `--tests` the test suites, and
`--reference DIR` puts cargo's target directories outside the work directory.
`python3 harness/hotpatch.py --edits 30` times hot-patching (needs `dx`: the bench shell has it). Projects are cloned once into `RA_JIT_PROJECTS`
(default `~/ra-jit-projects`). The whole CI set (seven projects, both tiers, five runs) took 38
minutes on 16 Zen 5 cores, plus the build of ra-jit. It needs about 20 GB of memory (nushell)
and 30 GB of disk.
`python3 harness/bench.py --compare OLD.json NEW.json` lists what got slower or holds more
memory. On macOS give `--work` a short directory (`--work /tmp/bench`): the daemon's socket
path has to fit in 104 bytes, which a work directory under `$TMPDIR` does not leave room for.
`python3 harness/benchreport.py RESULT` publishes a run as its platform's (the machine it ran
on): `RA_JIT_BENCH_MACHINE` names that machine in the tables, and `taskset -c 0-15` before
`python3` gives it the 16 cores of the benchmark machines. `benchreport.py --render` writes
every table and chart again from the stored results.

## Caveats

- Live edits (the patch applied to a running program) work on macOS on Apple silicon only and are
  measured there by `harness/live_latency.py`, per keystroke under typing as well as one edit at
  a time: [live.md](live.md#latency-under-typing) has the numbers from a MacBook, and
  `history/macos-arm64-live.jsonl` the runs.
- One machine, one OS. The numbers on a laptop will differ; the ratios are what carries over.
- Cold is the least precise number here: five runs at most, each minutes long on the large
  projects.
- The setups compared are measured on ripgrep and nushell only (`--setups-on`): seven more
  builds of every step on every project would make the nightly run too long for one machine.
- An edit is a small, fixed change. A larger change (many functions, a new type) costs more;
  `harness/agents.py` makes random edits of the kinds coding agents make.
- axum's output is not checked (it is a server); its build is.
- ripgrep's runtime is one short workload (tens of milliseconds, over 16 threads), so it says more
  about start-up and directory walking than about generated code; the program ra-jit linked has
  had the edits spliced into it and taken back, as a program in use would.
- Cold for `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet), so it matches
  plain cargo; the daemon then loads the workspace in the background ("Daemon ready after").
- The C compiler comes from Nix (gcc 16) with the wrapper's hardening turned off, as a
  distribution's gcc behaves; it is the same for both sides. Nix's linker wrapper runs
  `realpath` for each library directory of the shell on every link cargo makes (ra-jit's links
  do not go through it), so the bench shell holds only what the measured projects need, 72
  directories (a link through `cc` costs 0.23 s); zed's libraries are in `nix develop
  .#bench-zed`. With them in the bench shell (282 directories) every cargo link cost 0.85 s, which
  made cargo and subsecond look 0.6 s slower. A result records the count (`library_dirs`).

## What the 2026-10-21 run found

The results below come from two runs on dev-aws-7 (16 cores of a c7i.16xlarge, ra-jit
`2b2f2dcd5b`): ripgrep to nushell on both tiers in one, bevy and helix on Cranelift with one
cold run each in the second, since helix with its test targets needs more memory than the
machine has. ra-jit loses or fails on these:

- **Doctests.** Without `-q`, `cargo ra-jit test` builds the doctests of every library itself, in
  a process that loads the workspace each time (about 2 s per library), on every run: ripgrep's
  suite takes 27.8 s with nothing changed against 4.3 s under `cargo test`, nushell's 12.4 s
  against 2.5 s. With `-q` cargo hands the doctests to rustdoc and ripgrep's run takes 4.9 s
  against 4.1 s. fd, hyperfine and just, whose libraries have few or no doctests, are faster than
  cargo after an edit and the same with nothing changed. In ripgrep, nushell and axum the edited
  library is one whose unit tests the daemon leaves to rustc, so that edit is cargo's build.
- **axum's `example-testing`.** The test binary the daemon builds crashes (SIGSEGV) in
  `the_real_deal`, which serves axum on a TCP socket and calls it with hyper's client: tokio's
  `Registration::readiness` reads a null `Arc<ScheduledIo>` from `TcpListener::accept`. Five of
  the six tests pass; cargo's binary passes all six. Seen on Linux and on macOS.
- **bevy's breakout.** The daemon cannot type `(apply_velocity, move_paddle,
  check_for_collisions).chain()` (the method comes from impls bevy generates for tuples of
  systems), and `cargo ra-jit build` fails at the first edit instead of handing the build to
  cargo. Cold and no-op are measured; the edits are not.
- **helix.** After a 31 s cold build (the same as cargo's) the daemon takes 638 s to be ready
  (636 to 641 s over five cold runs) and holds 98 GB; with a warm store it is no faster (681 s).
  With the test targets loaded too it was killed for memory on a 123 GB machine. Its edits, once
  ready, are 10x to 48x faster than cargo's.
  nushell, a larger workspace, is ready 33 s after its build, at 16 GB.
- **zed** (1.7M lines, 1,291 crates) does not fit: the daemon needs more than 111 GB.
- **Cold.** `cargo ra-jit build` from nothing is cargo's build plus the start of the daemon:
  6.29 s against 5.56 s on ripgrep, and the daemon is ready 6.2 s later.
- **No-op** on ripgrep: 33 ms against cargo's 30 ms. This run's Cranelift column used the
  binary built with the `llvm` feature, which spends about 6 ms loading libLLVM at every start;
  the bench now measures that column with ra-jit as released (19 ms on ripgrep in
  [results.md](results.md#warm-edits-on-linux-x86-64-where-the-time-goes-and-build-scripts-2026-10-20)).

Two commits after `2b2f2dcd5b` (`20708cd2da`, `c8f58ac7d9`) stop the daemon from running build
scripts again that cargo just ran, which roughly halves the edits of fd and hyperfine (fd's
function edit 88 to 43 ms on another machine, same page). The next run of the workflow
measures them here.

## Earlier one-off measurements

Edit, then build, on a workspace of 200 generated crates (`harness/genws.py`), Linux x86-64,
8 CPUs, both warm and incremental, same linker, median of three. Measured once by hand before
this page existed; not updated by CI.

| Edit | cargo | ra-jit | |
|---|---:|---:|---:|
| A function in the main crate | 1.76 s | 0.42 s | 4.2x |
| A function in a crate everything depends on | 6.33 s | 0.60 s | 10.6x |
| Functions in 30 crates at once | 6.26 s | 0.62 s | 10.1x |
| A new function | 2.09 s | 0.56 s | 3.7x |
| A signature used in every crate | 13.87 s | 10.87 s | 1.3x |

## Results

<!-- bench:start -->
Every number of the newest run of each platform, then of each project measured on its own on it (bun, by the `bench-bun` workflow), under its own machine. Each platform is a machine of its own: its numbers compare with each other and with its own history, not with another platform's.

### macOS arm64

ra-jit [`84760c3f51`](https://github.com/indexable-inc/ra-jit/commit/84760c3f517a76f6e8a3aa41ccd81fc3f26b87c7), 2026-10-21T21:03:27Z, Rust 1.99.0, 3 runs of each edit, measured by hand. Raw: [benchmarks/macos-arm64.json](benchmarks/macos-arm64.json).

| Machine | |
|---|---|
| Name | MacBook Pro (Mac17,6), macOS 27.0.1, with other work running |
| CPU | Apple M5 Max |
| Cores | 18 |
| Memory (GiB) | 128 |
| OS | macOS |
| Kernel | 27.0.0 |
| rustc | rustc 1.99.0 (b940084d7 2026-09-28) |
| C compiler | Apple clang version 21.0.0 (clang-2100.3.34.2) |
| Linker | - |
| LLVM | - |
| Incremental | profile default (on for dev) |
| Compiler cache | none |
| Load average at the start | 48.0, 46.1, 48.4 |

#### Edits, Cranelift, macOS arm64

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| ripgrep 15.1.0 | Edit a function | 993 ms | 126 ms | 7.9x | 646 ms to 1.11 s | 125 ms to 191 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a widely used function | 1.31 s | 159 ms | 8.2x | 1.02 s to 4.28 s | 104 ms to 160 ms | daemon | spliced |
| ripgrep 15.1.0 | Change a signature | 828 ms | 175 ms | 4.7x | 785 ms to 1.36 s | 102 ms to 305 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a generic function | 1.93 s | 100 ms | 19x | 1.27 s to 1.95 s | 92 ms to 157 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a function | 1.81 s | 248 ms | 7.3x | 706 ms to 2.86 s | 104 ms to 271 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a struct field | 1.88 s | 191 ms | 9.8x | 1.35 s to 1.96 s | 94 ms to 298 ms | daemon | spliced |
| ripgrep 15.1.0 | Nothing changed | 67 ms | 54 ms | 1.2x | 63 ms to 72 ms | 50 ms to 61 ms | daemon |  |
| fd 10.3.0 | Edit a function | 630 ms | 109 ms | 5.8x | 611 ms to 1.29 s | 99 ms to 140 ms | daemon | spliced |
| fd 10.3.0 | Edit a widely used function | 734 ms | 102 ms | 7.2x | 691 ms to 1.01 s | 87 ms to 105 ms | daemon | spliced |
| fd 10.3.0 | Change a signature | 681 ms | 113 ms | 6.0x | 563 ms to 835 ms | 100 ms to 205 ms | daemon | spliced |
| fd 10.3.0 | Add a function | 712 ms | 126 ms | 5.7x | 593 ms to 745 ms | 106 ms to 228 ms | daemon | spliced |
| fd 10.3.0 | Add a struct field | 1.09 s | 127 ms | 8.6x | 894 ms to 1.31 s | 113 ms to 143 ms | daemon | spliced |
| fd 10.3.0 | Nothing changed | 144 ms | 49 ms | 2.9x | 85 ms to 369 ms | 48 ms to 84 ms | daemon |  |
| nushell 0.108.0 | Edit a function | 1.81 s | 454 ms | 4.0x | 1.66 s to 1.89 s | 383 ms to 713 ms | daemon | spliced |
| nushell 0.108.0 | Edit a widely used function | 14.39 s | 850 ms | 17x | 12.49 s to 15.24 s | 840 ms to 861 ms | daemon | spliced |
| nushell 0.108.0 | Change a signature | 10.28 s | 716 ms | 14x | 9.54 s to 12.34 s | 694 ms to 867 ms | daemon | spliced |
| nushell 0.108.0 | Edit a generic function | 20.61 s | 2.12 s | 9.7x | 9.59 s to 26.96 s | 986 ms to 2.50 s | daemon | full link: a program too large for frames in front of its own: one is 0x1000020 bytes in |
| nushell 0.108.0 | Add a function | 2.47 s | 444 ms | 5.6x | 1.74 s to 2.86 s | 353 ms to 480 ms | daemon | spliced |
| nushell 0.108.0 | Add a struct field | 25.84 s | 2.51 s | 10x | 23.58 s to 30.94 s | 2.21 s to 3.54 s | daemon | full link: a program too large for frames in front of its own: one is 0x1000008 bytes in |
| nushell 0.108.0 | Nothing changed | 407 ms | 45 ms | 9.0x | 347 ms to 433 ms | 41 ms to 56 ms | daemon |  |

#### Cold, Cranelift, macOS arm64

From an empty target directory, crates already downloaded, one run. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 9.83 s | 12.08 s | 14.68 s | 17.62 s | 18,437 | 2,360 MB | 72 MB |
| fd 10.3.0 | 9.17 s | 11.57 s | 12.62 s | 13.16 s | 8,478 | 1,946 MB | 32 MB |
| nushell 0.108.0 | 49.69 s | 77.23 s | 92.67 s | 139.83 s | 167,893 | 16,137 MB | 1,121 MB |

#### Memory and disk, Cranelift, macOS arm64

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | Cold, until the daemon is ready | 541 MB | 2.4 GB | 356 MB | 2.2 GB | 2.2 GB |
| ripgrep 15.1.0 | Edit a function | 235 MB | 2.9 GB | 235 MB | 1.9 GB | 2.9 GB |
| ripgrep 15.1.0 | Edit a widely used function | 243 MB | 2.9 GB | 243 MB | 1.9 GB | 2.8 GB |
| ripgrep 15.1.0 | Change a signature | 235 MB | 2.9 GB | 235 MB | 1.9 GB | 2.8 GB |
| ripgrep 15.1.0 | Edit a generic function | 237 MB | 2.9 GB | 237 MB | 1.9 GB | 2.8 GB |
| ripgrep 15.1.0 | Add a function | 234 MB | 2.9 GB | 234 MB | 1.9 GB | 2.8 GB |
| ripgrep 15.1.0 | Add a struct field | 286 MB | 2.9 GB | 286 MB | 1.9 GB | 2.8 GB |
| ripgrep 15.1.0 | Nothing changed | 40 MB | 2.8 GB | 40 MB | 1.9 GB | 2.8 GB |
| fd 10.3.0 | Cold, until the daemon is ready | 897 MB | 1.9 GB | 389 MB | 1.8 GB | 1.8 GB |
| fd 10.3.0 | Edit a function | 212 MB | 2.6 GB | 212 MB | 1.6 GB | 2.6 GB |
| fd 10.3.0 | Edit a widely used function | 199 MB | 2.6 GB | 192 MB | 1.6 GB | 2.6 GB |
| fd 10.3.0 | Change a signature | 226 MB | 2.6 GB | 226 MB | 1.6 GB | 2.6 GB |
| fd 10.3.0 | Add a function | 211 MB | 2.6 GB | 211 MB | 1.6 GB | 2.6 GB |
| fd 10.3.0 | Add a struct field | 258 MB | 2.6 GB | 258 MB | 1.6 GB | 2.6 GB |
| fd 10.3.0 | Nothing changed | 48 MB | 2.5 GB | 48 MB | 1.6 GB | 2.5 GB |
| nushell 0.108.0 | Cold, until the daemon is ready | 2.4 GB | 16.1 GB | 1.1 GB | 15.3 GB | 11.5 GB |
| nushell 0.108.0 | Edit a function | 1.1 GB | 12.5 GB | 1.1 GB | 11.5 GB | 12.5 GB |
| nushell 0.108.0 | Edit a widely used function | 1.1 GB | 12.5 GB | 1.1 GB | 11.5 GB | 12.5 GB |
| nushell 0.108.0 | Change a signature | 1.1 GB | 12.5 GB | 1.1 GB | 11.5 GB | 12.5 GB |
| nushell 0.108.0 | Edit a generic function | 1.1 GB | 13.2 GB | 1.1 GB | 11.3 GB | 12.2 GB |
| nushell 0.108.0 | Add a function | 1.1 GB | 12.5 GB | 1.1 GB | 11.5 GB | 12.5 GB |
| nushell 0.108.0 | Add a struct field | 1.1 GB | 13.3 GB | 1.1 GB | 11.4 GB | 12.2 GB |
| nushell 0.108.0 | Nothing changed | 156 MB | 12.1 GB | 156 MB | 11.2 GB | 12.1 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 2.2 GB | 2.8 GB | 2.8 GB after 39 builds | +23 MB | 2.3 GB, 2.0 GB | 416 MB | 526 MB | 72 MB | 0 MB |
| fd 10.3.0 | 1.8 GB | 2.5 GB | 2.6 GB after 33 builds | +50 MB | 1.9 GB, 1.6 GB | 355 MB | 427 MB | 33 MB | 0 MB |
| nushell 0.108.0 | 11.5 GB | 12.1 GB | 12.5 GB after 39 builds | +367 MB | 16.5 GB, 11.7 GB | 5.3 GB | 5.8 GB | 1.1 GB | 1 MB |

#### How fast the program runs, Cranelift, macOS arm64

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| ripgrep 15.1.0 | `rg -uuu --sort path -c \w+_unchecked\(\|unsafe \{ <the toolchain's library source>` | 1.01 s (676 ms to 1.43 s); 12 MB, 28.5 MB | 753 ms (679 ms to 1.07 s); 8 MB, 15.6 MB | 167 ms (129 ms to 308 ms); 6 MB, 6.6 MB |

### macOS arm64, bun

ra-jit [`1645168dc4`](https://github.com/indexable-inc/ra-jit/commit/1645168dc4abbdfab2e234232efb73a5c1ec2935), 2026-10-21T21:34:57Z, Rust 1.99.0, 3 runs of each edit, measured by hand. Raw: [benchmarks/bun-macos-arm64.json](benchmarks/bun-macos-arm64.json).

| Machine | |
|---|---|
| Name | - |
| CPU | Apple M5 Max |
| Cores | 18 |
| Memory (GiB) | 128 |
| OS | macOS |
| Kernel | 27.0.0 |
| rustc | rustc 1.99.0 (b940084d7 2026-09-28) |
| C compiler | Apple clang version 21.0.0 (clang-2100.3.34.2) |
| Linker | - |
| LLVM | - |
| Incremental | profile default (on for dev) |
| Compiler cache | none |
| Load average at the start | 27.8, 39.6, 44.9 |

#### Edits, Cranelift, macOS arm64, bun

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| bun 1.4.3 | Edit a function | 12.16 s | 1.09 s | 11x | 11.41 s to 15.04 s | 1.02 s to 2.41 s | daemon | spliced |
| bun 1.4.3 | Edit a widely used function | 26.75 s | 1.81 s | 15x | 20.91 s to 29.59 s | 779 ms to 2.37 s | daemon | spliced |
| bun 1.4.3 | Change a signature | 9.83 s | 126.44 s | 0.1x | 9.83 s to 9.83 s | 126.44 s to 126.44 s | cargo: `run` is not compiled by ra-jit and rustc's copy is from before the edit: | full link: no executable to add to |
| bun 1.4.3 | Edit a generic function | 31.80 s | 72.52 s | 0.4x | 25.09 s to 34.14 s | 71.86 s to 78.34 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Add a function | 12.25 s | 50.50 s | 0.2x | 10.94 s to 19.06 s | 48.01 s to 54.18 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Nothing changed | 168 ms | 73 ms | 2.3x | 161 ms to 203 ms | 61 ms to 75 ms | daemon |  |

#### Cold, Cranelift, macOS arm64, bun

From an empty target directory, crates already downloaded, one run. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 106.42 s | 163.72 s | 291.41 s | 195.47 s | 440,560 | 37,556 MB | 2,825 MB |

#### Memory and disk, Cranelift, macOS arm64, bun

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| bun 1.4.3 | Cold, until the daemon is ready | 3.1 GB | 37.6 GB | 2.7 GB | 37.5 GB | 27.6 GB |
| bun 1.4.3 | Edit a function | 2.1 GB | 28.7 GB | 2.1 GB | 27.7 GB | 28.6 GB |
| bun 1.4.3 | Edit a widely used function | 2.1 GB | 28.3 GB | 2.1 GB | 27.3 GB | 28.3 GB |
| bun 1.4.3 | Change a signature | 2.1 GB | 39.6 GB | 2.1 GB | 38.6 GB | 27.1 GB |
| bun 1.4.3 | Edit a generic function | 2.2 GB | 39.7 GB | 2.2 GB | 38.7 GB | 28.2 GB |
| bun 1.4.3 | Add a function | 2.2 GB | 39.5 GB | 2.2 GB | 38.5 GB | 28.2 GB |
| bun 1.4.3 | Nothing changed | 77 MB | 28.1 GB | 77 MB | 27.1 GB | 28.1 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 27.6 GB | 28.1 GB | 28.2 GB after 29 builds | +165 MB | 43.6 GB, 29.2 GB | 5.9 GB | 9.9 GB | 2.8 GB | 7 MB |

#### How fast the program runs, Cranelift, macOS arm64, bun

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| bun 1.4.3 | `bun -e const t = new Bun.Transpiler({ loader: 'ts', minifyWhitespace: true }); const fs = require('fs'); const sources = ['http2', 'net', '_http_server', 'fs', 'child_process'].map(n => fs.readFileSync('src/js/node/' + n + '.ts', 'utf8')); let n = 0; for (let i = 0; i < 20; i++) for (const s of sources) n += t.transformSync(s).length; console.log(n)` | 1.06 s (954 ms to 1.11 s); 71 MB, 384.4 MB | 754 ms (690 ms to 824 ms); 56 MB, 189.6 MB | 117 ms (106 ms to 122 ms); 42 MB, 91.8 MB |

### Linux aarch64

ra-jit [`2cf189feb6`](https://github.com/indexable-inc/ra-jit/commit/2cf189feb641a25a8c0d26391756f9c217c56bc2), 2026-10-22T03:36:58Z, Rust 1.99.0, 5 runs of each edit, measured by hand. Raw: [benchmarks/linux-aarch64.json](benchmarks/linux-aarch64.json).

| Machine | |
|---|---|
| Name | AWS c8g.16xlarge, pinned to 16 cores with taskset |
| CPU | Neoverse-V2 |
| Cores | 16 |
| Memory (GiB) | 124 |
| OS | Linux |
| Kernel | 6.18.53 |
| rustc | rustc 1.99.0 (b940084d7 2026-09-28) |
| C compiler | gcc (GCC) 16.2.0 |
| Linker | GNU ld (GNU Binutils) 2.46 |
| LLVM | 20.1.8 |
| Incremental | profile default (on for dev) |
| Compiler cache | none |
| Load average at the start | 10.1, 5.8, 2.4 |

#### Edits, Cranelift, Linux aarch64

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| ripgrep 15.1.0 | Edit a function | 1.27 s | 56 ms | 23x | 1.26 s to 1.32 s | 55 ms to 137 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a widely used function | 1.65 s | 43 ms | 38x | 1.64 s to 1.67 s | 43 ms to 88 ms | daemon | spliced |
| ripgrep 15.1.0 | Change a signature | 1.44 s | 87 ms | 16x | 1.43 s to 1.45 s | 87 ms to 125 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a generic function | 1.61 s | 47 ms | 34x | 1.60 s to 1.63 s | 46 ms to 50 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a function | 1.26 s | 65 ms | 19x | 1.25 s to 1.26 s | 65 ms to 70 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a struct field | 1.80 s | 64 ms | 28x | 1.80 s to 1.84 s | 64 ms to 84 ms | daemon | spliced |
| ripgrep 15.1.0 | Nothing changed | 27 ms | 20 ms | 1.3x | 26 ms to 27 ms | 20 ms to 20 ms | daemon |  |
| fd 10.3.0 | Edit a function | 1.35 s | 49 ms | 27x | 1.34 s to 1.38 s | 49 ms to 73 ms | daemon | spliced |
| fd 10.3.0 | Edit a widely used function | 1.30 s | 36 ms | 36x | 1.29 s to 1.30 s | 36 ms to 39 ms | daemon | spliced |
| fd 10.3.0 | Change a signature | 1.36 s | 65 ms | 21x | 1.34 s to 1.38 s | 65 ms to 164 ms | daemon | spliced |
| fd 10.3.0 | Add a function | 1.39 s | 57 ms | 24x | 1.36 s to 1.40 s | 56 ms to 70 ms | daemon | spliced |
| fd 10.3.0 | Add a struct field | 1.64 s | 54 ms | 30x | 1.61 s to 1.66 s | 54 ms to 78 ms | daemon | spliced |
| fd 10.3.0 | Nothing changed | 35 ms | 18 ms | 1.9x | 35 ms to 35 ms | 18 ms to 18 ms | daemon |  |
| bat 0.26.0 | Edit a function | 1.71 s | 54 ms | 32x | 1.68 s to 1.73 s | 53 ms to 349 ms | daemon | spliced |
| bat 0.26.0 | Edit a widely used function | 2.00 s | 41 ms | 49x | 1.98 s to 2.02 s | 41 ms to 44 ms | daemon | spliced |
| bat 0.26.0 | Change a signature | 1.95 s | 47 ms | 42x | 1.94 s to 1.97 s | 46 ms to 83 ms | daemon | spliced |
| bat 0.26.0 | Add a function | 1.69 s | 60 ms | 28x | 1.67 s to 1.78 s | 59 ms to 69 ms | daemon | spliced |
| bat 0.26.0 | Add a struct field | 2.38 s | 61 ms | 39x | 2.34 s to 2.46 s | 60 ms to 63 ms | daemon | spliced |
| bat 0.26.0 | Nothing changed | 67 ms | 18 ms | 3.7x | 67 ms to 68 ms | 17 ms to 18 ms | daemon |  |
| hyperfine 1.19.0 | Edit a function | 787 ms | 39 ms | 20x | 780 ms to 791 ms | 38 ms to 52 ms | daemon | spliced |
| hyperfine 1.19.0 | Edit a widely used function | 828 ms | 41 ms | 20x | 826 ms to 838 ms | 40 ms to 41 ms | daemon | spliced |
| hyperfine 1.19.0 | Change a signature | 836 ms | 91 ms | 9.2x | 828 ms to 844 ms | 89 ms to 93 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a function | 786 ms | 42 ms | 19x | 782 ms to 790 ms | 42 ms to 44 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a struct field | 928 ms | 78 ms | 12x | 925 ms to 947 ms | 77 ms to 101 ms | daemon | spliced |
| hyperfine 1.19.0 | Nothing changed | 46 ms | 18 ms | 2.6x | 45 ms to 46 ms | 17 ms to 18 ms | daemon |  |
| just 1.43.0 | Edit a function | 1.57 s | 41 ms | 38x | 1.55 s to 1.60 s | 41 ms to 110 ms | daemon | spliced |
| just 1.43.0 | Edit a widely used function | 1.66 s | 42 ms | 40x | 1.62 s to 1.69 s | 41 ms to 44 ms | daemon | spliced |
| just 1.43.0 | Change a signature | 1.59 s | 73 ms | 22x | 1.56 s to 1.63 s | 72 ms to 85 ms | daemon | spliced |
| just 1.43.0 | Add a function | 1.70 s | 65 ms | 26x | 1.69 s to 1.71 s | 64 ms to 68 ms | daemon | spliced |
| just 1.43.0 | Add a struct field | 3.19 s | 135 ms | 24x | 3.16 s to 3.44 s | 132 ms to 141 ms | daemon | spliced |
| just 1.43.0 | Nothing changed | 45 ms | 17 ms | 2.6x | 45 ms to 45 ms | 17 ms to 18 ms | daemon |  |
| axum 0.8.6 | Edit a function | 855 ms | 52 ms | 16x | 840 ms to 874 ms | 52 ms to 66 ms | daemon | spliced |
| axum 0.8.6 | Edit a widely used function | 1.44 s | 44 ms | 33x | 1.42 s to 1.45 s | 44 ms to 52 ms | daemon | spliced |
| axum 0.8.6 | Add a function | 850 ms | 52 ms | 16x | 835 ms to 855 ms | 52 ms to 57 ms | daemon | spliced |
| axum 0.8.6 | Nothing changed | 78 ms | 19 ms | 4.1x | 78 ms to 78 ms | 18 ms to 20 ms | daemon |  |
| nushell 0.108.0 | Edit a function | 7.50 s | 295 ms | 25x | 7.47 s to 7.57 s | 288 ms to 512 ms | daemon | spliced |
| nushell 0.108.0 | Edit a widely used function | 11.86 s | 273 ms | 43x | 11.70 s to 11.99 s | 253 ms to 288 ms | daemon | spliced |
| nushell 0.108.0 | Change a signature | 11.75 s | 323 ms | 36x | 11.71 s to 11.87 s | 313 ms to 533 ms | daemon | spliced |
| nushell 0.108.0 | Edit a generic function | 14.06 s | 215 ms | 65x | 13.99 s to 14.69 s | 211 ms to 225 ms | daemon | spliced |
| nushell 0.108.0 | Add a function | 7.46 s | 300 ms | 25x | 7.41 s to 7.49 s | 297 ms to 305 ms | daemon | spliced |
| nushell 0.108.0 | Add a struct field | 16.40 s | 332 ms | 49x | 16.16 s to 16.55 s | 318 ms to 389 ms | daemon | spliced |
| nushell 0.108.0 | Nothing changed | 198 ms | 22 ms | 9.0x | 196 ms to 201 ms | 22 ms to 23 ms | daemon |  |
| helix 25.07.1 | Edit a function | 7.21 s | 78 ms | 92x | 7.17 s to 7.35 s | 75 ms to 93.14 s | daemon | full link: no executable to add to |
| helix 25.07.1 | Edit a widely used function | 8.58 s | 91 ms | 94x | 8.55 s to 8.65 s | 89 ms to 95 ms | daemon | spliced |
| helix 25.07.1 | Change a signature | 8.00 s | 246 ms | 32x | 7.96 s to 8.13 s | 244 ms to 405 ms | daemon | spliced |
| helix 25.07.1 | Add a function | 7.22 s | 138 ms | 52x | 7.14 s to 7.31 s | 131 ms to 149 ms | daemon | spliced |
| helix 25.07.1 | Add a struct field | 15.96 s | 437 ms | 37x | 15.86 s to 16.67 s | 409 ms to 454 ms | daemon | spliced |
| helix 25.07.1 | Nothing changed | 96 ms | 19 ms | 5.1x | 95 ms to 98 ms | 18 ms to 242 ms | daemon |  |
| bevy 0.20.0 | Edit a function | 23.76 s | 105.76 s | 0.2x | 23.47 s to 24.15 s | 104.88 s to 297.05 s | daemon | full link: no executable to add to |
| bevy 0.20.0 | Edit a widely used function | 33.52 s | 105.54 s | 0.3x | 33.23 s to 34.23 s | 104.45 s to 105.89 s | daemon | full link: the program is too large: number too small to fit in target type |
| bevy 0.20.0 | Add a function | 23.65 s | 104.75 s | 0.2x | 23.49 s to 23.89 s | 104.45 s to 106.14 s | daemon | full link: a function the executable was linked with a patch of is as it first was |
| bevy 0.20.0 | Nothing changed | 226 ms | 23 ms | 9.8x | 225 ms to 246 ms | 23 ms to 332 ms | daemon |  |

#### Cold, Cranelift, Linux aarch64

From an empty target directory, crates already downloaded, median of 5 runs, each from nothing. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 6.72 s (6.64 s to 6.80 s) | 6.79 s (6.75 s to 6.89 s) | 7.08 s (7.04 s to 7.18 s) | 6.86 s (6.82 s to 6.94 s) | 0 | 2,588 MB | 21 MB |
| fd 10.3.0 | 33.75 s (33.34 s to 34.17 s) | 33.39 s (33.03 s to 33.52 s) | 33.68 s (33.32 s to 33.81 s) | 33.61 s (33.39 s to 33.93 s) | 0 | 2,267 MB | 16 MB |
| bat 0.26.0 | 13.56 s (13.49 s to 13.67 s) | 13.24 s (13.10 s to 13.49 s) | 13.79 s (13.65 s to 14.05 s) | 13.78 s (13.62 s to 13.96 s) | 0 | 2,875 MB | 25 MB |
| hyperfine 1.19.0 | 6.50 s (6.46 s to 6.61 s) | 6.58 s (6.56 s to 6.68 s) | 6.87 s (6.85 s to 6.97 s) | 6.84 s (6.79 s to 6.99 s) | 0 | 2,292 MB | 12 MB |
| just 1.43.0 | 9.72 s (9.69 s to 9.78 s) | 9.35 s (9.25 s to 9.56 s) | 9.91 s (9.81 s to 10.12 s) | 9.97 s (9.72 s to 10.23 s) | 0 | 3,226 MB | 34 MB |
| axum 0.8.6 | 9.58 s (9.55 s to 9.73 s) | 9.72 s (9.61 s to 9.85 s) | 10.01 s (9.90 s to 10.13 s) | 9.97 s (9.91 s to 10.07 s) | 0 | 2,767 MB | 16 MB |
| nushell 0.108.0 | 48.40 s (47.82 s to 48.53 s) | 43.22 s (42.53 s to 43.29 s) | 45.10 s (44.42 s to 45.18 s) | 44.36 s (43.99 s to 44.42 s) | 0 | 15,490 MB | 210 MB |
| helix 25.07.1 | 43.45 s (42.79 s to 44.23 s) | 38.99 s (38.94 s to 39.54 s) | 42.17 s (41.94 s to 42.75 s) | 42.57 s (42.10 s to 42.91 s) | 0 | 12,895 MB | 1,678 MB |
| bevy 0.20.0 | 124.12 s (122.75 s to 125.89 s) | 102.88 s (102.33 s to 104.19 s) | 132.55 s (131.74 s to 133.83 s) | 134.37 s (133.47 s to 135.88 s) | 0 | 57,076 MB | 6,527 MB |

#### Memory and disk, Cranelift, Linux aarch64

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | Cold, until the daemon is ready | 1.9 GB | 2.7 GB | 413 MB | 1.8 GB | 1.9 GB |
| ripgrep 15.1.0 | Edit a function | 463 MB | 3.7 GB | 275 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Edit a widely used function | 487 MB | 3.6 GB | 284 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Change a signature | 463 MB | 3.6 GB | 271 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Edit a generic function | 480 MB | 3.6 GB | 283 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Add a function | 469 MB | 3.6 GB | 276 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Add a struct field | 504 MB | 3.6 GB | 329 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Nothing changed | 35 MB | 3.7 GB | 35 MB | 2.3 GB | 3.7 GB |
| fd 10.3.0 | Cold, until the daemon is ready | 2.2 GB | 3.6 GB | 590 MB | 1.9 GB | 2.0 GB |
| fd 10.3.0 | Edit a function | 519 MB | 3.4 GB | 304 MB | 2.0 GB | 3.4 GB |
| fd 10.3.0 | Edit a widely used function | 510 MB | 3.4 GB | 304 MB | 2.0 GB | 3.4 GB |
| fd 10.3.0 | Change a signature | 521 MB | 3.4 GB | 304 MB | 2.0 GB | 3.4 GB |
| fd 10.3.0 | Add a function | 519 MB | 3.4 GB | 304 MB | 2.0 GB | 3.4 GB |
| fd 10.3.0 | Add a struct field | 545 MB | 3.4 GB | 304 MB | 2.0 GB | 3.4 GB |
| fd 10.3.0 | Nothing changed | 37 MB | 3.5 GB | 37 MB | 2.1 GB | 3.5 GB |
| bat 0.26.0 | Cold, until the daemon is ready | 3.4 GB | 6.1 GB | 460 MB | 2.5 GB | 2.6 GB |
| bat 0.26.0 | Edit a function | 672 MB | 4.0 GB | 460 MB | 2.6 GB | 4.0 GB |
| bat 0.26.0 | Edit a widely used function | 670 MB | 4.0 GB | 460 MB | 2.6 GB | 4.0 GB |
| bat 0.26.0 | Change a signature | 656 MB | 4.0 GB | 460 MB | 2.6 GB | 4.0 GB |
| bat 0.26.0 | Add a function | 672 MB | 4.0 GB | 460 MB | 2.6 GB | 4.0 GB |
| bat 0.26.0 | Add a struct field | 687 MB | 4.0 GB | 460 MB | 2.6 GB | 4.0 GB |
| bat 0.26.0 | Nothing changed | 53 MB | 4.1 GB | 53 MB | 2.7 GB | 4.1 GB |
| hyperfine 1.19.0 | Cold, until the daemon is ready | 2.2 GB | 2.6 GB | 342 MB | 2.1 GB | 2.2 GB |
| hyperfine 1.19.0 | Edit a function | 360 MB | 3.6 GB | 222 MB | 2.1 GB | 3.6 GB |
| hyperfine 1.19.0 | Edit a widely used function | 370 MB | 3.6 GB | 238 MB | 2.1 GB | 3.6 GB |
| hyperfine 1.19.0 | Change a signature | 371 MB | 3.6 GB | 242 MB | 2.1 GB | 3.6 GB |
| hyperfine 1.19.0 | Add a function | 361 MB | 3.6 GB | 224 MB | 2.1 GB | 3.6 GB |
| hyperfine 1.19.0 | Add a struct field | 385 MB | 3.6 GB | 268 MB | 2.1 GB | 3.6 GB |
| hyperfine 1.19.0 | Nothing changed | 43 MB | 3.6 GB | 43 MB | 2.2 GB | 3.6 GB |
| just 1.43.0 | Cold, until the daemon is ready | 3.1 GB | 3.7 GB | 600 MB | 2.3 GB | 2.4 GB |
| just 1.43.0 | Edit a function | 434 MB | 4.1 GB | 320 MB | 2.7 GB | 4.1 GB |
| just 1.43.0 | Edit a widely used function | 422 MB | 4.1 GB | 344 MB | 2.7 GB | 4.1 GB |
| just 1.43.0 | Change a signature | 422 MB | 4.1 GB | 342 MB | 2.7 GB | 4.1 GB |
| just 1.43.0 | Add a function | 434 MB | 4.1 GB | 359 MB | 2.7 GB | 4.1 GB |
| just 1.43.0 | Add a struct field | 631 MB | 4.1 GB | 588 MB | 2.7 GB | 4.1 GB |
| just 1.43.0 | Nothing changed | 43 MB | 4.2 GB | 43 MB | 2.8 GB | 4.1 GB |
| axum 0.8.6 | Cold, until the daemon is ready | 2.1 GB | 3.2 GB | 507 MB | 2.6 GB | 2.7 GB |
| axum 0.8.6 | Edit a function | 399 MB | 4.0 GB | 229 MB | 2.6 GB | 4.0 GB |
| axum 0.8.6 | Edit a widely used function | 397 MB | 4.0 GB | 318 MB | 2.6 GB | 4.0 GB |
| axum 0.8.6 | Add a function | 400 MB | 4.0 GB | 232 MB | 2.6 GB | 4.0 GB |
| axum 0.8.6 | Nothing changed | 74 MB | 4.1 GB | 74 MB | 2.7 GB | 4.1 GB |
| nushell 0.108.0 | Cold, until the daemon is ready | 4.6 GB | 8.3 GB | 2.0 GB | 5.8 GB | 5.8 GB |
| nushell 0.108.0 | Edit a function | 2.4 GB | 12.7 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Edit a widely used function | 2.3 GB | 12.7 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Change a signature | 2.3 GB | 12.6 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Edit a generic function | 2.4 GB | 12.7 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Add a function | 2.4 GB | 12.7 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Add a struct field | 2.4 GB | 12.7 GB | 2.0 GB | 11.2 GB | 12.6 GB |
| nushell 0.108.0 | Nothing changed | 126 MB | 12.9 GB | 126 MB | 11.5 GB | 12.9 GB |
| helix 25.07.1 | Cold, until the daemon is ready | 3.5 GB | 6.4 GB | 1.5 GB | 4.5 GB | 4.6 GB |
| helix 25.07.1 | Edit a function | 1.8 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Edit a widely used function | 1.8 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.1 GB |
| helix 25.07.1 | Change a signature | 1.7 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.1 GB |
| helix 25.07.1 | Add a function | 1.8 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.1 GB |
| helix 25.07.1 | Add a struct field | 1.8 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.1 GB |
| helix 25.07.1 | Nothing changed | 68 MB | 14.0 GB | 68 MB | 12.6 GB | 14.0 GB |
| bevy 0.20.0 | Cold, until the daemon is ready | 6.8 GB | 11.1 GB | 6.3 GB | 10.9 GB | 8.9 GB |
| bevy 0.20.0 | Edit a function | 6.7 GB | 50.4 GB | 6.3 GB | 41.3 GB | 42.0 GB |
| bevy 0.20.0 | Edit a widely used function | 6.7 GB | 50.4 GB | 6.3 GB | 41.2 GB | 42.0 GB |
| bevy 0.20.0 | Add a function | 6.7 GB | 50.3 GB | 6.3 GB | 41.2 GB | 42.1 GB |
| bevy 0.20.0 | Nothing changed | 123 MB | 58.6 GB | 123 MB | 57.1 GB | 58.4 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 1.9 GB | 3.7 GB | 3.6 GB after 65 builds | −57 MB | 2.7 GB, 1.9 GB | 404 MB | 608 MB | 21 MB | 0 MB |
| fd 10.3.0 | 2.0 GB | 3.5 GB | 3.4 GB after 55 builds | −66 MB | 3.5 GB, 1.9 GB | 780 MB | 960 MB | 16 MB | 0 MB |
| bat 0.26.0 | 2.6 GB | 4.1 GB | 4.0 GB after 55 builds | −58 MB | 6.0 GB, 2.6 GB | 980 MB | 1.3 GB | 25 MB | 0 MB |
| hyperfine 1.19.0 | 2.2 GB | 3.6 GB | 3.6 GB after 55 builds | −52 MB | 2.6 GB, 2.2 GB | 340 MB | 454 MB | 12 MB | 0 MB |
| just 1.43.0 | 2.4 GB | 4.1 GB | 4.1 GB after 55 builds | −40 MB | 4.4 GB, 2.4 GB | 664 MB | 937 MB | 34 MB | 0 MB |
| axum 0.8.6 | 2.7 GB | 4.1 GB | 4.0 GB after 35 builds | −34 MB | 3.2 GB, 2.7 GB | 405 MB | 495 MB | 16 MB | 0 MB |
| nushell 0.108.0 | 5.8 GB | 12.9 GB | 12.6 GB after 65 builds | −261 MB | 8.3 GB, 5.8 GB | 5.3 GB | 7.3 GB | 211 MB | 1 MB |
| helix 25.07.1 | 4.6 GB | 14.0 GB | 14.1 GB after 55 builds | +181 MB | 6.4 GB, 4.6 GB | 3.7 GB | 6.2 GB | 1.7 GB | 1 MB |
| bevy 0.20.0 | 8.9 GB | 58.4 GB | 42.1 GB after 35 builds | −16.4 GB | 11.0 GB, 8.9 GB | 17.5 GB | 31.2 GB | 6.5 GB | 1 MB |

#### How fast the program runs, Cranelift, Linux aarch64

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| ripgrep 15.1.0 | `rg -uuu --sort path -c \w+_unchecked\(\|unsafe \{ <the toolchain's library source>` | 1.10 s (1.10 s to 1.10 s); 35 MB, 199.1 MB | 1.09 s (1.09 s to 1.09 s); 35 MB, 51.4 MB | 56 ms (56 ms to 57 ms); 35 MB, 31.6 MB |

#### Edits, LLVM, Linux aarch64

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| ripgrep 15.1.0 | Edit a function | 1.26 s | 65 ms | 19x | 1.25 s to 1.27 s | 64 ms to 143 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a widely used function | 1.65 s | 55 ms | 30x | 1.64 s to 1.66 s | 55 ms to 99 ms | daemon | spliced |
| ripgrep 15.1.0 | Change a signature | 1.43 s | 110 ms | 13x | 1.42 s to 1.47 s | 110 ms to 138 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a generic function | 1.61 s | 65 ms | 25x | 1.61 s to 1.66 s | 65 ms to 66 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a function | 1.27 s | 78 ms | 16x | 1.26 s to 1.28 s | 78 ms to 82 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a struct field | 1.82 s | 78 ms | 23x | 1.81 s to 1.86 s | 77 ms to 90 ms | daemon | spliced |
| ripgrep 15.1.0 | Nothing changed | 25 ms | 29 ms | 0.9x | 25 ms to 26 ms | 29 ms to 30 ms | daemon |  |
| fd 10.3.0 | Edit a function | 1.31 s | 59 ms | 22x | 1.30 s to 1.32 s | 57 ms to 82 ms | daemon | spliced |
| fd 10.3.0 | Edit a widely used function | 1.28 s | 48 ms | 27x | 1.28 s to 1.29 s | 48 ms to 50 ms | daemon | spliced |
| fd 10.3.0 | Change a signature | 1.31 s | 74 ms | 18x | 1.31 s to 1.32 s | 72 ms to 169 ms | daemon | spliced |
| fd 10.3.0 | Add a function | 1.32 s | 65 ms | 20x | 1.31 s to 1.33 s | 64 ms to 78 ms | daemon | spliced |
| fd 10.3.0 | Add a struct field | 1.59 s | 67 ms | 24x | 1.59 s to 1.65 s | 66 ms to 93 ms | daemon | spliced |
| fd 10.3.0 | Nothing changed | 35 ms | 28 ms | 1.2x | 35 ms to 35 ms | 27 ms to 28 ms | daemon |  |
| bat 0.26.0 | Edit a function | 1.69 s | 64 ms | 26x | 1.68 s to 1.74 s | 63 ms to 363 ms | daemon | spliced |
| bat 0.26.0 | Edit a widely used function | 2.01 s | 51 ms | 39x | 1.99 s to 2.06 s | 50 ms to 54 ms | daemon | spliced |
| bat 0.26.0 | Change a signature | 1.96 s | 63 ms | 31x | 1.94 s to 1.98 s | 62 ms to 91 ms | daemon | spliced |
| bat 0.26.0 | Add a function | 1.68 s | 70 ms | 24x | 1.67 s to 1.69 s | 70 ms to 78 ms | daemon | spliced |
| bat 0.26.0 | Add a struct field | 2.38 s | 70 ms | 34x | 2.33 s to 2.42 s | 69 ms to 78 ms | daemon | spliced |
| bat 0.26.0 | Nothing changed | 69 ms | 28 ms | 2.5x | 69 ms to 70 ms | 28 ms to 28 ms | daemon |  |
| hyperfine 1.19.0 | Edit a function | 778 ms | 56 ms | 14x | 774 ms to 796 ms | 55 ms to 69 ms | daemon | spliced |
| hyperfine 1.19.0 | Edit a widely used function | 824 ms | 51 ms | 16x | 821 ms to 827 ms | 50 ms to 53 ms | daemon | spliced |
| hyperfine 1.19.0 | Change a signature | 838 ms | 102 ms | 8.2x | 833 ms to 857 ms | 101 ms to 105 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a function | 778 ms | 59 ms | 13x | 776 ms to 778 ms | 59 ms to 63 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a struct field | 921 ms | 88 ms | 10x | 913 ms to 932 ms | 87 ms to 111 ms | daemon | spliced |
| hyperfine 1.19.0 | Nothing changed | 45 ms | 27 ms | 1.7x | 45 ms to 46 ms | 27 ms to 28 ms | daemon |  |
| just 1.43.0 | Edit a function | 1.57 s | 52 ms | 30x | 1.56 s to 1.59 s | 51 ms to 119 ms | daemon | spliced |
| just 1.43.0 | Edit a widely used function | 1.61 s | 52 ms | 31x | 1.60 s to 1.62 s | 52 ms to 55 ms | daemon | spliced |
| just 1.43.0 | Change a signature | 1.60 s | 103 ms | 16x | 1.58 s to 1.63 s | 100 ms to 105 ms | daemon | spliced |
| just 1.43.0 | Add a function | 1.73 s | 76 ms | 23x | 1.69 s to 1.77 s | 75 ms to 85 ms | daemon | spliced |
| just 1.43.0 | Add a struct field | 3.21 s | 148 ms | 22x | 3.17 s to 3.35 s | 146 ms to 166 ms | daemon | spliced |
| just 1.43.0 | Nothing changed | 45 ms | 27 ms | 1.7x | 44 ms to 45 ms | 26 ms to 28 ms | daemon |  |
| axum 0.8.6 | Edit a function | 859 ms | 62 ms | 14x | 844 ms to 867 ms | 61 ms to 76 ms | daemon | spliced |
| axum 0.8.6 | Edit a widely used function | 1.43 s | 54 ms | 26x | 1.42 s to 1.46 s | 54 ms to 59 ms | daemon | spliced |
| axum 0.8.6 | Add a function | 860 ms | 62 ms | 14x | 851 ms to 870 ms | 61 ms to 69 ms | daemon | spliced |
| axum 0.8.6 | Nothing changed | 79 ms | 29 ms | 2.7x | 79 ms to 81 ms | 28 ms to 29 ms | daemon |  |
| nushell 0.108.0 | Edit a function | 7.50 s | 8.93 s | 0.8x | 7.44 s to 7.55 s | 8.89 s to 9.17 s | daemon | full link: ra-jit::{closure}::106954 cannot be jumped from: a function of 8 bytes, too short to jump 676 MB from |
| nushell 0.108.0 | Edit a widely used function | 11.94 s | 260 ms | 46x | 11.73 s to 12.07 s | 254 ms to 270 ms | daemon | spliced |
| nushell 0.108.0 | Change a signature | 11.88 s | 329 ms | 36x | 11.75 s to 12.15 s | 324 ms to 532 ms | daemon | spliced |
| nushell 0.108.0 | Edit a generic function | 14.27 s | 9.14 s | 1.6x | 14.17 s to 14.40 s | 9.10 s to 9.20 s | daemon | full link: _RNvXs3_NtNtCsav5BWmFfmPX_11nu_protocol8pipeline13pipeline_dataNtB5_21PipelineExecutionDataNtNtNtCslpfLnFkgLmC_4core3ops5deref5Deref5deref cannot be jumped from |
| nushell 0.108.0 | Add a function | 7.52 s | 8.99 s | 0.8x | 7.48 s to 7.71 s | 8.86 s to 9.11 s | daemon | full link: ra-jit::{closure}::106920 cannot be jumped from: a function of 4 bytes, too short to jump 676 MB from |
| nushell 0.108.0 | Add a struct field | 16.30 s | 9.10 s | 1.8x | 16.16 s to 16.57 s | 9.07 s to 9.24 s | daemon | full link: ra-jit::{closure}::111745 cannot be jumped from: a function of 4 bytes, too short to jump 673 MB from |
| nushell 0.108.0 | Nothing changed | 194 ms | 31 ms | 6.3x | 194 ms to 196 ms | 31 ms to 34 ms | daemon |  |
| helix 25.07.1 | Edit a function | 7.21 s | 89 ms | 81x | 7.11 s to 7.28 s | 84 ms to 94.14 s | daemon | full link: no executable to add to |
| helix 25.07.1 | Edit a widely used function | 8.54 s | 101 ms | 85x | 8.45 s to 8.62 s | 100 ms to 104 ms | daemon | spliced |
| helix 25.07.1 | Change a signature | 7.95 s | 7.39 s | 1.1x | 7.88 s to 7.99 s | 7.25 s to 7.57 s | daemon | full link: ra-jit::{closure}::139171 cannot be jumped from: a function of 4 bytes, too short to jump 626 MB from |
| helix 25.07.1 | Add a function | 7.20 s | 142 ms | 51x | 7.16 s to 7.43 s | 136 ms to 7.39 s | daemon | full link: a function the executable was linked with a patch of is as it first was |
| helix 25.07.1 | Add a struct field | 15.92 s | 7.57 s | 2.1x | 15.76 s to 16.39 s | 7.46 s to 7.59 s | daemon | full link: _RNvMs_NtCskzVSIoZsrdC_10helix_term10compositorNtB4_10Compositor6resize cannot be jumped from: a function of 8 bytes, too short to jump 648 MB from |
| helix 25.07.1 | Nothing changed | 94 ms | 28 ms | 3.4x | 93 ms to 95 ms | 28 ms to 62 ms | daemon |  |
| bevy 0.20.0 | Edit a function | 23.82 s | 110.78 s | 0.2x | 23.77 s to 31.41 s | 110.13 s to 485.87 s | daemon | full link: no executable to add to |
| bevy 0.20.0 | Edit a widely used function | 33.92 s | 110.77 s | 0.3x | 33.43 s to 38.12 s | 110.22 s to 111.04 s | daemon | full link: the program is too large: number too small to fit in target type |
| bevy 0.20.0 | Add a function | 23.76 s | 111.13 s | 0.2x | 23.53 s to 24.03 s | 110.75 s to 111.20 s | daemon | full link: a function the executable was linked with a patch of is as it first was |
| bevy 0.20.0 | Nothing changed | 224 ms | 35 ms | 6.4x | 221 ms to 254 ms | 33 ms to 116 ms | daemon |  |

#### Cold, LLVM, Linux aarch64

From an empty target directory, crates already downloaded, median of 5 runs, each from nothing. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 6.82 s (6.77 s to 6.91 s) | 6.68 s (6.59 s to 6.81 s) | 6.99 s (6.90 s to 7.12 s) | 6.96 s (6.93 s to 7.03 s) | 0 | 2,594 MB | 19 MB |
| fd 10.3.0 | 34.01 s (33.97 s to 34.30 s) | 33.22 s (33.12 s to 33.36 s) | 33.53 s (33.43 s to 33.66 s) | 33.36 s (33.26 s to 33.57 s) | 0 | 2,265 MB | 15 MB |
| bat 0.26.0 | 14.70 s (14.41 s to 14.89 s) | 13.28 s (13.08 s to 13.84 s) | 13.87 s (13.66 s to 14.42 s) | 13.77 s (13.57 s to 14.15 s) | 0 | 2,878 MB | 23 MB |
| hyperfine 1.19.0 | 6.72 s (6.66 s to 6.73 s) | 6.62 s (6.55 s to 6.65 s) | 6.93 s (6.86 s to 6.96 s) | 6.89 s (6.86 s to 6.94 s) | 0 | 2,320 MB | 11 MB |
| just 1.43.0 | 10.72 s (10.66 s to 10.99 s) | 9.69 s (9.47 s to 9.90 s) | 10.27 s (10.05 s to 10.48 s) | 9.98 s (9.96 s to 10.13 s) | 0 | 3,263 MB | 32 MB |
| axum 0.8.6 | 9.58 s (9.55 s to 9.63 s) | 9.62 s (9.61 s to 9.66 s) | 9.93 s (9.92 s to 9.97 s) | 10.07 s (10.02 s to 10.12 s) | 0 | 2,861 MB | 15 MB |
| nushell 0.108.0 | 52.57 s (51.77 s to 52.78 s) | 42.59 s (42.50 s to 42.62 s) | 44.31 s (44.20 s to 44.56 s) | 44.96 s (44.63 s to 45.35 s) | 0 | 15,334 MB | 199 MB |
| helix 25.07.1 | 45.66 s (45.14 s to 46.91 s) | 39.22 s (39.08 s to 40.56 s) | 42.72 s (42.14 s to 43.92 s) | 42.68 s (42.47 s to 43.42 s) | 0 | 12,855 MB | 1,662 MB |
| bevy 0.20.0 | 127.73 s (126.50 s to 129.80 s) | 103.86 s (102.68 s to 104.45 s) | 133.87 s (132.13 s to 135.61 s) | 136.22 s (134.91 s to 137.78 s) | 0 | 36,833 MB | 6,475 MB |

#### Memory and disk, LLVM, Linux aarch64

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | Cold, until the daemon is ready | 2.0 GB | 3.0 GB | 413 MB | 1.9 GB | 2.0 GB |
| ripgrep 15.1.0 | Edit a function | 464 MB | 3.9 GB | 274 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Edit a widely used function | 488 MB | 3.9 GB | 285 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Change a signature | 464 MB | 3.9 GB | 270 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Edit a generic function | 484 MB | 3.9 GB | 281 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Add a function | 471 MB | 3.9 GB | 275 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Add a struct field | 500 MB | 3.9 GB | 327 MB | 2.3 GB | 3.8 GB |
| ripgrep 15.1.0 | Nothing changed | 35 MB | 3.8 GB | 35 MB | 2.3 GB | 3.8 GB |
| fd 10.3.0 | Cold, until the daemon is ready | 2.2 GB | 3.6 GB | 591 MB | 2.0 GB | 2.1 GB |
| fd 10.3.0 | Edit a function | 518 MB | 3.6 GB | 304 MB | 2.1 GB | 3.6 GB |
| fd 10.3.0 | Edit a widely used function | 511 MB | 3.6 GB | 304 MB | 2.1 GB | 3.5 GB |
| fd 10.3.0 | Change a signature | 521 MB | 3.6 GB | 304 MB | 2.1 GB | 3.5 GB |
| fd 10.3.0 | Add a function | 519 MB | 3.6 GB | 304 MB | 2.1 GB | 3.5 GB |
| fd 10.3.0 | Add a struct field | 546 MB | 3.6 GB | 304 MB | 2.1 GB | 3.5 GB |
| fd 10.3.0 | Nothing changed | 37 MB | 3.6 GB | 37 MB | 2.1 GB | 3.6 GB |
| bat 0.26.0 | Cold, until the daemon is ready | 3.3 GB | 7.3 GB | 460 MB | 2.6 GB | 2.7 GB |
| bat 0.26.0 | Edit a function | 672 MB | 4.2 GB | 460 MB | 2.6 GB | 4.1 GB |
| bat 0.26.0 | Edit a widely used function | 670 MB | 4.2 GB | 460 MB | 2.6 GB | 4.1 GB |
| bat 0.26.0 | Change a signature | 655 MB | 4.2 GB | 460 MB | 2.6 GB | 4.1 GB |
| bat 0.26.0 | Add a function | 672 MB | 4.2 GB | 460 MB | 2.6 GB | 4.1 GB |
| bat 0.26.0 | Add a struct field | 687 MB | 4.2 GB | 460 MB | 2.6 GB | 4.1 GB |
| bat 0.26.0 | Nothing changed | 53 MB | 4.2 GB | 53 MB | 2.7 GB | 4.2 GB |
| hyperfine 1.19.0 | Cold, until the daemon is ready | 2.2 GB | 2.8 GB | 342 MB | 2.2 GB | 2.3 GB |
| hyperfine 1.19.0 | Edit a function | 361 MB | 3.8 GB | 223 MB | 2.2 GB | 3.7 GB |
| hyperfine 1.19.0 | Edit a widely used function | 371 MB | 3.8 GB | 238 MB | 2.2 GB | 3.7 GB |
| hyperfine 1.19.0 | Change a signature | 372 MB | 3.8 GB | 242 MB | 2.2 GB | 3.7 GB |
| hyperfine 1.19.0 | Add a function | 362 MB | 3.8 GB | 226 MB | 2.2 GB | 3.7 GB |
| hyperfine 1.19.0 | Add a struct field | 384 MB | 3.7 GB | 270 MB | 2.2 GB | 3.7 GB |
| hyperfine 1.19.0 | Nothing changed | 44 MB | 3.8 GB | 44 MB | 2.2 GB | 3.7 GB |
| just 1.43.0 | Cold, until the daemon is ready | 3.1 GB | 3.8 GB | 600 MB | 2.4 GB | 2.5 GB |
| just 1.43.0 | Edit a function | 435 MB | 4.3 GB | 320 MB | 2.8 GB | 4.3 GB |
| just 1.43.0 | Edit a widely used function | 422 MB | 4.3 GB | 344 MB | 2.8 GB | 4.3 GB |
| just 1.43.0 | Change a signature | 422 MB | 4.3 GB | 341 MB | 2.8 GB | 4.3 GB |
| just 1.43.0 | Add a function | 434 MB | 4.3 GB | 360 MB | 2.8 GB | 4.3 GB |
| just 1.43.0 | Add a struct field | 632 MB | 4.3 GB | 592 MB | 2.8 GB | 4.3 GB |
| just 1.43.0 | Nothing changed | 43 MB | 4.3 GB | 43 MB | 2.8 GB | 4.3 GB |
| axum 0.8.6 | Cold, until the daemon is ready | 2.1 GB | 3.3 GB | 508 MB | 2.7 GB | 2.8 GB |
| axum 0.8.6 | Edit a function | 399 MB | 4.2 GB | 230 MB | 2.7 GB | 4.2 GB |
| axum 0.8.6 | Edit a widely used function | 397 MB | 4.2 GB | 318 MB | 2.7 GB | 4.2 GB |
| axum 0.8.6 | Add a function | 400 MB | 4.2 GB | 233 MB | 2.7 GB | 4.2 GB |
| axum 0.8.6 | Nothing changed | 74 MB | 4.2 GB | 74 MB | 2.8 GB | 4.2 GB |
| nushell 0.108.0 | Cold, until the daemon is ready | 4.5 GB | 8.5 GB | 2.0 GB | 5.9 GB | 5.9 GB |
| nushell 0.108.0 | Edit a function | 2.4 GB | 15.1 GB | 2.0 GB | 11.3 GB | 12.7 GB |
| nushell 0.108.0 | Edit a widely used function | 2.3 GB | 12.8 GB | 2.0 GB | 11.2 GB | 12.8 GB |
| nushell 0.108.0 | Change a signature | 2.4 GB | 12.8 GB | 2.0 GB | 11.2 GB | 12.8 GB |
| nushell 0.108.0 | Edit a generic function | 2.4 GB | 14.8 GB | 2.0 GB | 11.3 GB | 12.7 GB |
| nushell 0.108.0 | Add a function | 2.4 GB | 14.8 GB | 2.0 GB | 11.3 GB | 12.7 GB |
| nushell 0.108.0 | Add a struct field | 2.4 GB | 14.8 GB | 2.0 GB | 11.3 GB | 12.7 GB |
| nushell 0.108.0 | Nothing changed | 126 MB | 13.1 GB | 126 MB | 11.5 GB | 13.0 GB |
| helix 25.07.1 | Cold, until the daemon is ready | 3.5 GB | 6.5 GB | 1.5 GB | 4.6 GB | 4.3 GB |
| helix 25.07.1 | Edit a function | 1.8 GB | 14.3 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Edit a widely used function | 1.8 GB | 14.2 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Change a signature | 1.7 GB | 16.1 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Add a function | 1.8 GB | 14.3 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Add a struct field | 1.8 GB | 16.1 GB | 1.5 GB | 12.7 GB | 14.2 GB |
| helix 25.07.1 | Nothing changed | 68 MB | 14.0 GB | 68 MB | 12.5 GB | 14.0 GB |
| bevy 0.20.0 | Cold, until the daemon is ready | 6.8 GB | 11.1 GB | 6.3 GB | 10.9 GB | 8.9 GB |
| bevy 0.20.0 | Edit a function | 6.7 GB | 50.0 GB | 6.3 GB | 41.2 GB | 42.0 GB |
| bevy 0.20.0 | Edit a widely used function | 6.7 GB | 50.1 GB | 6.3 GB | 41.1 GB | 42.0 GB |
| bevy 0.20.0 | Add a function | 6.7 GB | 50.0 GB | 6.3 GB | 41.2 GB | 42.0 GB |
| bevy 0.20.0 | Nothing changed | 123 MB | 38.3 GB | 123 MB | 36.7 GB | 38.1 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 2.0 GB | 3.8 GB | 3.8 GB after 65 builds | −42 MB | 3.0 GB, 1.9 GB | 404 MB | 607 MB | 19 MB | 0 MB |
| fd 10.3.0 | 2.1 GB | 3.6 GB | 3.5 GB after 55 builds | −52 MB | 3.7 GB, 2.1 GB | 780 MB | 958 MB | 15 MB | 0 MB |
| bat 0.26.0 | 2.7 GB | 4.2 GB | 4.1 GB after 55 builds | −51 MB | 6.2 GB, 2.6 GB | 980 MB | 1.3 GB | 23 MB | 0 MB |
| hyperfine 1.19.0 | 2.3 GB | 3.7 GB | 3.7 GB after 55 builds | −52 MB | 2.8 GB, 2.3 GB | 340 MB | 452 MB | 11 MB | 0 MB |
| just 1.43.0 | 2.5 GB | 4.3 GB | 4.3 GB after 55 builds | −33 MB | 4.3 GB, 2.5 GB | 664 MB | 931 MB | 32 MB | 0 MB |
| axum 0.8.6 | 2.8 GB | 4.2 GB | 4.2 GB after 35 builds | −64 MB | 3.3 GB, 2.8 GB | 405 MB | 493 MB | 15 MB | 0 MB |
| nushell 0.108.0 | 5.9 GB | 13.0 GB | 12.7 GB after 65 builds | −293 MB | 9.1 GB, 6.0 GB | 5.3 GB | 7.2 GB | 200 MB | 1 MB |
| helix 25.07.1 | 4.3 GB | 14.0 GB | 14.2 GB after 55 builds | +207 MB | 6.5 GB, 4.6 GB | 3.7 GB | 6.1 GB | 1.7 GB | 1 MB |
| bevy 0.20.0 | 8.9 GB | 38.1 GB | 42.0 GB after 35 builds | +3.9 GB | 11.1 GB, 8.9 GB | 17.5 GB | 31.1 GB | 6.5 GB | 1 MB |

#### How fast the program runs, LLVM, Linux aarch64

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| ripgrep 15.1.0 | `rg -uuu --sort path -c \w+_unchecked\(\|unsafe \{ <the toolchain's library source>` | 1.09 s (1.09 s to 1.09 s); 35 MB, 196.8 MB | 1.09 s (1.09 s to 1.09 s); 35 MB, 51.4 MB | 56 ms (56 ms to 57 ms); 35 MB, 31.6 MB |

### Linux x86-64

ra-jit [`2cf189feb6`](https://github.com/indexable-inc/ra-jit/commit/2cf189feb641a25a8c0d26391756f9c217c56bc2), 2026-10-22T03:31:08Z, Rust 1.99.0, 5 runs of each edit, measured by hand. Raw: [benchmarks/linux-x86-64.json](benchmarks/linux-x86-64.json).

| Machine | |
|---|---|
| Name | AWS c7a.48xlarge (dev-aws-8), pinned to 16 cores with taskset |
| CPU | AMD EPYC 9R14 |
| Cores | 16 |
| Memory (GiB) | 370 |
| OS | Linux |
| Kernel | 6.18.53 |
| rustc | rustc 1.99.0 (b940084d7 2026-09-28) |
| C compiler | gcc (GCC) 16.2.0 |
| Linker | GNU ld (GNU Binutils) 2.46 |
| LLVM | 20.1.8 |
| Incremental | profile default (on for dev) |
| Compiler cache | none |
| Load average at the start | 13.4, 7.8, 3.8 |

#### Edits, Cranelift, Linux x86-64

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| ripgrep 15.1.0 | Edit a function | 787 ms | 69 ms | 11x | 784 ms to 790 ms | 67 ms to 137 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a widely used function | 1.18 s | 54 ms | 22x | 1.17 s to 1.18 s | 53 ms to 99 ms | daemon | spliced |
| ripgrep 15.1.0 | Change a signature | 957 ms | 95 ms | 10x | 953 ms to 959 ms | 94 ms to 140 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a generic function | 1.14 s | 59 ms | 19x | 1.14 s to 1.15 s | 59 ms to 60 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a function | 788 ms | 80 ms | 9.8x | 785 ms to 795 ms | 78 ms to 85 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a struct field | 1.31 s | 78 ms | 17x | 1.30 s to 1.36 s | 76 ms to 92 ms | daemon | spliced |
| ripgrep 15.1.0 | Nothing changed | 36 ms | 25 ms | 1.4x | 36 ms to 37 ms | 25 ms to 31 ms | daemon |  |
| fd 10.3.0 | Edit a function | 752 ms | 61 ms | 12x | 742 ms to 760 ms | 61 ms to 87 ms | daemon | spliced |
| fd 10.3.0 | Edit a widely used function | 716 ms | 49 ms | 15x | 713 ms to 716 ms | 48 ms to 49 ms | daemon | spliced |
| fd 10.3.0 | Change a signature | 759 ms | 76 ms | 10.0x | 757 ms to 768 ms | 75 ms to 170 ms | daemon | spliced |
| fd 10.3.0 | Add a function | 756 ms | 67 ms | 11x | 755 ms to 757 ms | 67 ms to 82 ms | daemon | spliced |
| fd 10.3.0 | Add a struct field | 1.01 s | 64 ms | 16x | 1.00 s to 1.04 s | 64 ms to 67 ms | daemon | spliced |
| fd 10.3.0 | Nothing changed | 50 ms | 23 ms | 2.2x | 50 ms to 50 ms | 23 ms to 29 ms | daemon |  |
| bat 0.26.0 | Edit a function | 774 ms | 61 ms | 13x | 771 ms to 778 ms | 60 ms to 359 ms | daemon | spliced |
| bat 0.26.0 | Edit a widely used function | 1.12 s | 50 ms | 22x | 1.11 s to 1.12 s | 49 ms to 54 ms | daemon | spliced |
| bat 0.26.0 | Change a signature | 1.06 s | 57 ms | 19x | 1.06 s to 1.07 s | 56 ms to 89 ms | daemon | spliced |
| bat 0.26.0 | Add a function | 774 ms | 69 ms | 11x | 769 ms to 777 ms | 67 ms to 76 ms | daemon | spliced |
| bat 0.26.0 | Add a struct field | 1.41 s | 71 ms | 20x | 1.41 s to 1.45 s | 71 ms to 74 ms | daemon | spliced |
| bat 0.26.0 | Nothing changed | 95 ms | 23 ms | 4.1x | 94 ms to 96 ms | 23 ms to 24 ms | daemon |  |
| hyperfine 1.19.0 | Edit a function | 616 ms | 47 ms | 13x | 612 ms to 620 ms | 46 ms to 60 ms | daemon | spliced |
| hyperfine 1.19.0 | Edit a widely used function | 660 ms | 48 ms | 14x | 658 ms to 664 ms | 47 ms to 52 ms | daemon | spliced |
| hyperfine 1.19.0 | Change a signature | 667 ms | 98 ms | 6.8x | 660 ms to 672 ms | 97 ms to 99 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a function | 620 ms | 51 ms | 12x | 617 ms to 621 ms | 50 ms to 61 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a struct field | 748 ms | 83 ms | 9.0x | 746 ms to 752 ms | 82 ms to 107 ms | daemon | spliced |
| hyperfine 1.19.0 | Nothing changed | 67 ms | 24 ms | 2.8x | 65 ms to 81 ms | 23 ms to 25 ms | daemon |  |
| just 1.43.0 | Edit a function | 1.04 s | 52 ms | 20x | 1.04 s to 1.05 s | 51 ms to 114 ms | daemon | spliced |
| just 1.43.0 | Edit a widely used function | 1.07 s | 52 ms | 21x | 1.06 s to 1.07 s | 51 ms to 53 ms | daemon | spliced |
| just 1.43.0 | Change a signature | 1.05 s | 83 ms | 13x | 1.05 s to 1.05 s | 82 ms to 94 ms | daemon | spliced |
| just 1.43.0 | Add a function | 1.16 s | 78 ms | 15x | 1.15 s to 1.17 s | 77 ms to 81 ms | daemon | spliced |
| just 1.43.0 | Add a struct field | 2.58 s | 158 ms | 16x | 2.56 s to 2.73 s | 151 ms to 162 ms | daemon | spliced |
| just 1.43.0 | Nothing changed | 65 ms | 23 ms | 2.8x | 64 ms to 66 ms | 23 ms to 24 ms | daemon |  |
| axum 0.8.6 | Edit a function | 668 ms | 66 ms | 10x | 665 ms to 669 ms | 65 ms to 80 ms | daemon | spliced |
| axum 0.8.6 | Edit a widely used function | 1.31 s | 57 ms | 23x | 1.30 s to 1.31 s | 57 ms to 58 ms | daemon | spliced |
| axum 0.8.6 | Add a function | 668 ms | 67 ms | 10.0x | 666 ms to 682 ms | 67 ms to 70 ms | daemon | spliced |
| axum 0.8.6 | Nothing changed | 117 ms | 25 ms | 4.7x | 117 ms to 118 ms | 24 ms to 28 ms | daemon |  |
| nushell 0.108.0 | Edit a function | 1.83 s | 293 ms | 6.3x | 1.82 s to 1.84 s | 289 ms to 489 ms | daemon | spliced |
| nushell 0.108.0 | Edit a widely used function | 6.76 s | 301 ms | 22x | 6.72 s to 6.80 s | 281 ms to 333 ms | daemon | spliced |
| nushell 0.108.0 | Change a signature | 6.83 s | 372 ms | 18x | 6.65 s to 6.87 s | 345 ms to 550 ms | daemon | spliced |
| nushell 0.108.0 | Edit a generic function | 8.99 s | 230 ms | 39x | 8.96 s to 9.22 s | 223 ms to 244 ms | daemon | spliced |
| nushell 0.108.0 | Add a function | 1.84 s | 304 ms | 6.0x | 1.83 s to 1.85 s | 300 ms to 317 ms | daemon | spliced |
| nushell 0.108.0 | Add a struct field | 11.24 s | 362 ms | 31x | 11.08 s to 11.58 s | 346 ms to 390 ms | daemon | spliced |
| nushell 0.108.0 | Nothing changed | 282 ms | 30 ms | 9.4x | 280 ms to 284 ms | 29 ms to 31 ms | daemon |  |
| helix 25.07.1 | Edit a function | 3.07 s | 90 ms | 34x | 3.04 s to 3.08 s | 89 ms to 80.78 s | daemon | full link: no executable to add to |
| helix 25.07.1 | Edit a widely used function | 4.66 s | 105 ms | 44x | 4.61 s to 4.69 s | 101 ms to 111 ms | daemon | spliced |
| helix 25.07.1 | Change a signature | 3.98 s | 270 ms | 15x | 3.95 s to 4.00 s | 268 ms to 410 ms | daemon | spliced |
| helix 25.07.1 | Add a function | 3.13 s | 159 ms | 20x | 3.12 s to 3.15 s | 151 ms to 166 ms | daemon | spliced |
| helix 25.07.1 | Add a struct field | 11.94 s | 427 ms | 28x | 11.84 s to 12.54 s | 421 ms to 429 ms | daemon | spliced |
| helix 25.07.1 | Nothing changed | 134 ms | 26 ms | 5.2x | 131 ms to 136 ms | 24 ms to 53 ms | daemon |  |
| bevy 0.20.0 | Edit a function | 3.96 s | 11.60 s | 0.3x | 3.92 s to 4.10 s | 11.57 s to 200.81 s | daemon | full link: no executable to add to |
| bevy 0.20.0 | Edit a widely used function | 15.07 s | 12.85 s | 1.2x | 14.86 s to 15.10 s | 11.54 s to 13.23 s | daemon | full link: the program is too large: number too small to fit in target type |
| bevy 0.20.0 | Add a function | 3.95 s | 11.79 s | 0.3x | 3.92 s to 3.95 s | 11.69 s to 13.39 s | daemon | full link: a function the executable was linked with a patch of is as it first was |
| bevy 0.20.0 | Nothing changed | 305 ms | 33 ms | 9.2x | 300 ms to 322 ms | 32 ms to 219 ms | daemon |  |

#### Cold, Cranelift, Linux x86-64

From an empty target directory, crates already downloaded, median of 5 runs, each from nothing. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 6.18 s (6.12 s to 6.22 s) | 6.23 s (6.19 s to 6.26 s) | 6.55 s (6.51 s to 6.58 s) | 6.54 s (6.52 s to 6.57 s) | 0 | 2,383 MB | 21 MB |
| fd 10.3.0 | 40.20 s (40.07 s to 40.24 s) | 39.94 s (39.68 s to 40.21 s) | 40.26 s (40.01 s to 40.54 s) | 40.19 s (40.07 s to 40.37 s) | 0 | 2,032 MB | 16 MB |
| bat 0.26.0 | 13.34 s (13.31 s to 13.37 s) | 12.89 s (12.80 s to 12.92 s) | 13.49 s (13.40 s to 13.51 s) | 13.42 s (13.38 s to 13.48 s) | 0 | 2,645 MB | 25 MB |
| hyperfine 1.19.0 | 6.42 s (6.40 s to 6.51 s) | 6.50 s (6.47 s to 6.53 s) | 6.82 s (6.80 s to 6.85 s) | 6.82 s (6.78 s to 6.87 s) | 0 | 2,114 MB | 11 MB |
| just 1.43.0 | 9.42 s (9.39 s to 9.79 s) | 9.08 s (9.02 s to 9.24 s) | 9.68 s (9.62 s to 9.84 s) | 9.73 s (9.71 s to 9.79 s) | 0 | 3,021 MB | 34 MB |
| axum 0.8.6 | 9.43 s (9.40 s to 9.46 s) | 9.54 s (9.51 s to 9.56 s) | 9.87 s (9.83 s to 9.88 s) | 9.89 s (9.81 s to 9.95 s) | 0 | 2,607 MB | 15 MB |
| nushell 0.108.0 | 46.09 s (45.65 s to 46.48 s) | 39.43 s (39.27 s to 39.84 s) | 41.40 s (41.24 s to 41.80 s) | 41.63 s (41.26 s to 41.79 s) | 0 | 15,139 MB | 208 MB |
| helix 25.07.1 | 40.67 s (40.49 s to 41.17 s) | 35.94 s (35.81 s to 35.97 s) | 39.28 s (39.12 s to 39.44 s) | 39.30 s (38.99 s to 40.12 s) | 0 | 12,684 MB | 1,677 MB |
| bevy 0.20.0 | 110.94 s (109.60 s to 112.49 s) | 86.59 s (85.20 s to 87.09 s) | 116.98 s (115.63 s to 117.47 s) | 117.94 s (117.64 s to 119.89 s) | 0 | 49,044 MB | 6,524 MB |

#### Memory and disk, Cranelift, Linux x86-64

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | Cold, until the daemon is ready | 2.4 GB | 2.9 GB | 421 MB | 1.7 GB | 1.8 GB |
| ripgrep 15.1.0 | Edit a function | 531 MB | 3.5 GB | 296 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Edit a widely used function | 539 MB | 3.5 GB | 305 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Change a signature | 527 MB | 3.5 GB | 295 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Edit a generic function | 536 MB | 3.5 GB | 303 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Add a function | 531 MB | 3.5 GB | 297 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Add a struct field | 558 MB | 3.5 GB | 354 MB | 2.1 GB | 3.5 GB |
| ripgrep 15.1.0 | Nothing changed | 27 MB | 3.5 GB | 27 MB | 2.1 GB | 3.5 GB |
| fd 10.3.0 | Cold, until the daemon is ready | 2.7 GB | 3.2 GB | 554 MB | 1.8 GB | 1.9 GB |
| fd 10.3.0 | Edit a function | 605 MB | 3.3 GB | 371 MB | 1.9 GB | 3.3 GB |
| fd 10.3.0 | Edit a widely used function | 598 MB | 3.3 GB | 371 MB | 1.9 GB | 3.3 GB |
| fd 10.3.0 | Change a signature | 610 MB | 3.3 GB | 372 MB | 1.9 GB | 3.3 GB |
| fd 10.3.0 | Add a function | 605 MB | 3.3 GB | 371 MB | 1.9 GB | 3.3 GB |
| fd 10.3.0 | Add a struct field | 633 MB | 3.3 GB | 371 MB | 1.9 GB | 3.3 GB |
| fd 10.3.0 | Nothing changed | 34 MB | 3.3 GB | 34 MB | 1.9 GB | 3.3 GB |
| bat 0.26.0 | Cold, until the daemon is ready | 3.7 GB | 5.8 GB | 519 MB | 2.3 GB | 2.5 GB |
| bat 0.26.0 | Edit a function | 749 MB | 3.9 GB | 517 MB | 2.4 GB | 3.9 GB |
| bat 0.26.0 | Edit a widely used function | 749 MB | 3.9 GB | 520 MB | 2.4 GB | 3.9 GB |
| bat 0.26.0 | Change a signature | 741 MB | 3.9 GB | 519 MB | 2.4 GB | 3.9 GB |
| bat 0.26.0 | Add a function | 750 MB | 3.9 GB | 520 MB | 2.4 GB | 3.9 GB |
| bat 0.26.0 | Add a struct field | 765 MB | 3.9 GB | 520 MB | 2.4 GB | 3.9 GB |
| bat 0.26.0 | Nothing changed | 51 MB | 3.9 GB | 51 MB | 2.5 GB | 3.9 GB |
| hyperfine 1.19.0 | Cold, until the daemon is ready | 2.5 GB | 3.0 GB | 356 MB | 2.0 GB | 2.1 GB |
| hyperfine 1.19.0 | Edit a function | 422 MB | 3.4 GB | 246 MB | 2.0 GB | 3.4 GB |
| hyperfine 1.19.0 | Edit a widely used function | 428 MB | 3.4 GB | 262 MB | 2.0 GB | 3.4 GB |
| hyperfine 1.19.0 | Change a signature | 431 MB | 3.4 GB | 265 MB | 2.0 GB | 3.4 GB |
| hyperfine 1.19.0 | Add a function | 421 MB | 3.4 GB | 249 MB | 2.0 GB | 3.4 GB |
| hyperfine 1.19.0 | Add a struct field | 446 MB | 3.4 GB | 292 MB | 2.0 GB | 3.4 GB |
| hyperfine 1.19.0 | Nothing changed | 41 MB | 3.5 GB | 41 MB | 2.0 GB | 3.4 GB |
| just 1.43.0 | Cold, until the daemon is ready | 3.4 GB | 4.2 GB | 627 MB | 2.2 GB | 2.2 GB |
| just 1.43.0 | Edit a function | 539 MB | 4.0 GB | 356 MB | 2.6 GB | 4.0 GB |
| just 1.43.0 | Edit a widely used function | 534 MB | 4.0 GB | 363 MB | 2.6 GB | 4.0 GB |
| just 1.43.0 | Change a signature | 535 MB | 4.0 GB | 360 MB | 2.6 GB | 4.0 GB |
| just 1.43.0 | Add a function | 536 MB | 4.0 GB | 382 MB | 2.6 GB | 4.0 GB |
| just 1.43.0 | Add a struct field | 650 MB | 4.0 GB | 612 MB | 2.6 GB | 4.0 GB |
| just 1.43.0 | Nothing changed | 40 MB | 4.0 GB | 40 MB | 2.6 GB | 4.0 GB |
| axum 0.8.6 | Cold, until the daemon is ready | 2.5 GB | 3.3 GB | 513 MB | 2.4 GB | 2.6 GB |
| axum 0.8.6 | Edit a function | 468 MB | 3.9 GB | 254 MB | 2.5 GB | 3.9 GB |
| axum 0.8.6 | Edit a widely used function | 454 MB | 3.9 GB | 339 MB | 2.5 GB | 3.9 GB |
| axum 0.8.6 | Add a function | 469 MB | 3.9 GB | 256 MB | 2.5 GB | 3.9 GB |
| axum 0.8.6 | Nothing changed | 72 MB | 3.9 GB | 72 MB | 2.5 GB | 3.9 GB |
| nushell 0.108.0 | Cold, until the daemon is ready | 4.6 GB | 9.0 GB | 2.4 GB | 6.1 GB | 6.1 GB |
| nushell 0.108.0 | Edit a function | 2.7 GB | 12.5 GB | 2.4 GB | 11.1 GB | 12.5 GB |
| nushell 0.108.0 | Edit a widely used function | 2.7 GB | 12.5 GB | 2.4 GB | 11.0 GB | 12.5 GB |
| nushell 0.108.0 | Change a signature | 2.7 GB | 12.5 GB | 2.4 GB | 11.0 GB | 12.5 GB |
| nushell 0.108.0 | Edit a generic function | 2.7 GB | 12.5 GB | 2.4 GB | 11.1 GB | 12.5 GB |
| nushell 0.108.0 | Add a function | 2.7 GB | 12.5 GB | 2.4 GB | 11.1 GB | 12.5 GB |
| nushell 0.108.0 | Add a struct field | 2.7 GB | 12.5 GB | 2.4 GB | 11.1 GB | 12.5 GB |
| nushell 0.108.0 | Nothing changed | 122 MB | 12.8 GB | 122 MB | 11.3 GB | 12.8 GB |
| helix 25.07.1 | Cold, until the daemon is ready | 3.8 GB | 6.5 GB | 1.8 GB | 4.4 GB | 4.5 GB |
| helix 25.07.1 | Edit a function | 2.1 GB | 13.9 GB | 1.8 GB | 12.5 GB | 13.9 GB |
| helix 25.07.1 | Edit a widely used function | 2.1 GB | 13.9 GB | 1.8 GB | 12.5 GB | 13.9 GB |
| helix 25.07.1 | Change a signature | 2.1 GB | 13.9 GB | 1.8 GB | 12.5 GB | 13.9 GB |
| helix 25.07.1 | Add a function | 2.1 GB | 13.9 GB | 1.8 GB | 12.5 GB | 13.9 GB |
| helix 25.07.1 | Add a struct field | 2.1 GB | 13.9 GB | 1.8 GB | 12.5 GB | 13.9 GB |
| helix 25.07.1 | Nothing changed | 65 MB | 13.8 GB | 65 MB | 12.4 GB | 13.6 GB |
| bevy 0.20.0 | Cold, until the daemon is ready | 8.5 GB | 10.4 GB | 8.1 GB | 10.2 GB | 10.4 GB |
| bevy 0.20.0 | Edit a function | 8.5 GB | 55.8 GB | 8.1 GB | 42.3 GB | 43.3 GB |
| bevy 0.20.0 | Edit a widely used function | 8.5 GB | 55.8 GB | 8.1 GB | 42.2 GB | 43.3 GB |
| bevy 0.20.0 | Add a function | 8.5 GB | 85.1 GB | 8.1 GB | 42.2 GB | 43.3 GB |
| bevy 0.20.0 | Nothing changed | 121 MB | 50.9 GB | 121 MB | 49.4 GB | 51.8 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 1.8 GB | 3.5 GB | 3.5 GB after 65 builds | −29 MB | 2.8 GB, 1.8 GB | 407 MB | 609 MB | 21 MB | 1 MB |
| fd 10.3.0 | 1.9 GB | 3.3 GB | 3.3 GB after 55 builds | −37 MB | 3.3 GB, 1.9 GB | 636 MB | 847 MB | 16 MB | 0 MB |
| bat 0.26.0 | 2.5 GB | 3.9 GB | 3.9 GB after 55 builds | −31 MB | 5.8 GB, 2.5 GB | 973 MB | 1.2 GB | 25 MB | 0 MB |
| hyperfine 1.19.0 | 2.1 GB | 3.4 GB | 3.4 GB after 55 builds | −47 MB | 2.9 GB, 2.1 GB | 342 MB | 455 MB | 11 MB | 0 MB |
| just 1.43.0 | 2.2 GB | 4.0 GB | 4.0 GB after 55 builds | −15 MB | 4.1 GB, 2.2 GB | 667 MB | 946 MB | 34 MB | 0 MB |
| axum 0.8.6 | 2.6 GB | 3.9 GB | 3.9 GB after 35 builds | −46 MB | 3.2 GB, 2.6 GB | 404 MB | 492 MB | 15 MB | 0 MB |
| nushell 0.108.0 | 6.1 GB | 12.8 GB | 12.5 GB after 65 builds | −265 MB | 9.0 GB, 6.5 GB | 5.3 GB | 7.3 GB | 209 MB | 1 MB |
| helix 25.07.1 | 4.5 GB | 13.6 GB | 13.9 GB after 55 builds | +270 MB | 6.6 GB, 4.5 GB | 3.7 GB | 6.1 GB | 1.7 GB | 0 MB |
| bevy 0.20.0 | 10.4 GB | 51.8 GB | 43.3 GB after 35 builds | −8.5 GB | 10.5 GB, 10.6 GB | 17.6 GB | 29.3 GB | 6.5 GB | 1 MB |

#### How fast the program runs, Cranelift, Linux x86-64

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| ripgrep 15.1.0 | `rg -uuu --sort path -c \w+_unchecked\(\|unsafe \{ <the toolchain's library source>` | 2.46 s (2.44 s to 2.46 s); 29 MB, 199.6 MB | 2.39 s (2.35 s to 2.41 s); 29 MB, 53.2 MB | 73 ms (73 ms to 74 ms); 29 MB, 30.7 MB |

#### Edits, LLVM, Linux x86-64

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| ripgrep 15.1.0 | Edit a function | 785 ms | 83 ms | 9.5x | 777 ms to 786 ms | 82 ms to 151 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a widely used function | 1.17 s | 73 ms | 16x | 1.17 s to 1.18 s | 71 ms to 117 ms | daemon | spliced |
| ripgrep 15.1.0 | Change a signature | 951 ms | 119 ms | 8.0x | 946 ms to 953 ms | 118 ms to 153 ms | daemon | spliced |
| ripgrep 15.1.0 | Edit a generic function | 1.14 s | 82 ms | 14x | 1.13 s to 1.14 s | 81 ms to 83 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a function | 788 ms | 96 ms | 8.2x | 785 ms to 797 ms | 94 ms to 100 ms | daemon | spliced |
| ripgrep 15.1.0 | Add a struct field | 1.31 s | 91 ms | 14x | 1.29 s to 1.35 s | 91 ms to 104 ms | daemon | spliced |
| ripgrep 15.1.0 | Nothing changed | 36 ms | 40 ms | 0.9x | 35 ms to 36 ms | 40 ms to 47 ms | daemon |  |
| fd 10.3.0 | Edit a function | 755 ms | 76 ms | 9.9x | 751 ms to 759 ms | 74 ms to 101 ms | daemon | spliced |
| fd 10.3.0 | Edit a widely used function | 715 ms | 66 ms | 11x | 709 ms to 716 ms | 66 ms to 69 ms | daemon | spliced |
| fd 10.3.0 | Change a signature | 767 ms | 92 ms | 8.3x | 763 ms to 768 ms | 91 ms to 187 ms | daemon | spliced |
| fd 10.3.0 | Add a function | 756 ms | 82 ms | 9.2x | 746 ms to 759 ms | 81 ms to 98 ms | daemon | spliced |
| fd 10.3.0 | Add a struct field | 1.01 s | 81 ms | 12x | 1.00 s to 1.04 s | 80 ms to 101 ms | daemon | spliced |
| fd 10.3.0 | Nothing changed | 49 ms | 40 ms | 1.2x | 49 ms to 50 ms | 38 ms to 41 ms | daemon |  |
| bat 0.26.0 | Edit a function | 769 ms | 77 ms | 10.0x | 761 ms to 774 ms | 76 ms to 374 ms | daemon | spliced |
| bat 0.26.0 | Edit a widely used function | 1.11 s | 67 ms | 17x | 1.10 s to 1.11 s | 65 ms to 73 ms | daemon | spliced |
| bat 0.26.0 | Change a signature | 1.05 s | 77 ms | 14x | 1.05 s to 1.06 s | 77 ms to 105 ms | daemon | spliced |
| bat 0.26.0 | Add a function | 772 ms | 84 ms | 9.2x | 769 ms to 773 ms | 82 ms to 93 ms | daemon | spliced |
| bat 0.26.0 | Add a struct field | 1.41 s | 86 ms | 16x | 1.40 s to 1.45 s | 86 ms to 88 ms | daemon | spliced |
| bat 0.26.0 | Nothing changed | 95 ms | 39 ms | 2.4x | 94 ms to 96 ms | 38 ms to 40 ms | daemon |  |
| hyperfine 1.19.0 | Edit a function | 615 ms | 72 ms | 8.5x | 607 ms to 618 ms | 71 ms to 84 ms | daemon | spliced |
| hyperfine 1.19.0 | Edit a widely used function | 659 ms | 65 ms | 10x | 653 ms to 660 ms | 65 ms to 68 ms | daemon | spliced |
| hyperfine 1.19.0 | Change a signature | 664 ms | 116 ms | 5.7x | 663 ms to 670 ms | 115 ms to 117 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a function | 617 ms | 75 ms | 8.2x | 616 ms to 620 ms | 74 ms to 80 ms | daemon | spliced |
| hyperfine 1.19.0 | Add a struct field | 745 ms | 101 ms | 7.4x | 744 ms to 749 ms | 98 ms to 123 ms | daemon | spliced |
| hyperfine 1.19.0 | Nothing changed | 66 ms | 39 ms | 1.7x | 64 ms to 67 ms | 38 ms to 41 ms | daemon |  |
| just 1.43.0 | Edit a function | 1.04 s | 68 ms | 15x | 1.04 s to 1.05 s | 66 ms to 129 ms | daemon | spliced |
| just 1.43.0 | Edit a widely used function | 1.06 s | 69 ms | 15x | 1.06 s to 1.08 s | 68 ms to 73 ms | daemon | spliced |
| just 1.43.0 | Change a signature | 1.05 s | 119 ms | 8.8x | 1.04 s to 1.06 s | 117 ms to 121 ms | daemon | spliced |
| just 1.43.0 | Add a function | 1.15 s | 92 ms | 13x | 1.15 s to 1.18 s | 90 ms to 97 ms | daemon | spliced |
| just 1.43.0 | Add a struct field | 2.60 s | 174 ms | 15x | 2.58 s to 2.71 s | 166 ms to 182 ms | daemon | spliced |
| just 1.43.0 | Nothing changed | 65 ms | 38 ms | 1.7x | 64 ms to 65 ms | 37 ms to 40 ms | daemon |  |
| axum 0.8.6 | Edit a function | 664 ms | 80 ms | 8.3x | 662 ms to 668 ms | 79 ms to 92 ms | daemon | spliced |
| axum 0.8.6 | Edit a widely used function | 1.30 s | 73 ms | 18x | 1.29 s to 1.33 s | 71 ms to 74 ms | daemon | spliced |
| axum 0.8.6 | Add a function | 668 ms | 81 ms | 8.2x | 667 ms to 675 ms | 79 ms to 86 ms | daemon | spliced |
| axum 0.8.6 | Nothing changed | 116 ms | 42 ms | 2.8x | 113 ms to 130 ms | 39 ms to 43 ms | daemon |  |
| nushell 0.108.0 | Edit a function | 1.83 s | 313 ms | 5.8x | 1.81 s to 1.84 s | 307 ms to 528 ms | daemon | spliced |
| nushell 0.108.0 | Edit a widely used function | 6.75 s | 309 ms | 22x | 6.72 s to 6.77 s | 295 ms to 315 ms | daemon | spliced |
| nushell 0.108.0 | Change a signature | 6.79 s | 387 ms | 18x | 6.76 s to 6.81 s | 349 ms to 583 ms | daemon | spliced |
| nushell 0.108.0 | Edit a generic function | 9.09 s | 510 ms | 18x | 8.92 s to 9.22 s | 494 ms to 535 ms | daemon | spliced |
| nushell 0.108.0 | Add a function | 1.85 s | 318 ms | 5.8x | 1.81 s to 1.86 s | 315 ms to 334 ms | daemon | spliced |
| nushell 0.108.0 | Add a struct field | 11.27 s | 383 ms | 29x | 11.24 s to 11.40 s | 361 ms to 414 ms | daemon | spliced |
| nushell 0.108.0 | Nothing changed | 279 ms | 46 ms | 6.1x | 276 ms to 283 ms | 44 ms to 48 ms | daemon |  |
| helix 25.07.1 | Edit a function | 3.08 s | 110 ms | 28x | 3.07 s to 3.10 s | 105 ms to 78.06 s | daemon | full link: no executable to add to |
| helix 25.07.1 | Edit a widely used function | 4.63 s | 121 ms | 38x | 4.61 s to 4.69 s | 121 ms to 125 ms | daemon | spliced |
| helix 25.07.1 | Change a signature | 3.98 s | 294 ms | 14x | 3.95 s to 4.00 s | 279 ms to 421 ms | daemon | spliced |
| helix 25.07.1 | Add a function | 3.12 s | 167 ms | 19x | 3.09 s to 3.16 s | 163 ms to 180 ms | daemon | spliced |
| helix 25.07.1 | Add a struct field | 11.84 s | 443 ms | 27x | 11.65 s to 12.46 s | 439 ms to 458 ms | daemon | spliced |
| helix 25.07.1 | Nothing changed | 136 ms | 43 ms | 3.2x | 135 ms to 136 ms | 39 ms to 323 ms | daemon |  |
| bevy 0.20.0 | Edit a function | 3.83 s | 11.21 s | 0.3x | 3.81 s to 3.87 s | 11.09 s to 304.36 s | daemon | full link: no executable to add to |
| bevy 0.20.0 | Edit a widely used function | 14.93 s | 12.14 s | 1.2x | 14.49 s to 14.97 s | 11.07 s to 12.26 s | daemon | full link: the program is too large: number too small to fit in target type |
| bevy 0.20.0 | Add a function | 3.91 s | 11.28 s | 0.3x | 3.88 s to 3.94 s | 11.25 s to 12.83 s | daemon | full link: a function the executable was linked with a patch of is as it first was |
| bevy 0.20.0 | Nothing changed | 307 ms | 49 ms | 6.3x | 302 ms to 312 ms | 47 ms to 117 ms | daemon |  |

#### Cold, LLVM, Linux x86-64

From an empty target directory, crates already downloaded, median of 5 runs, each from nothing. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 6.68 s (6.65 s to 6.85 s) | 6.39 s (6.32 s to 6.43 s) | 6.74 s (6.68 s to 6.79 s) | 6.70 s (6.69 s to 6.71 s) | 0 | 2,483 MB | 19 MB |
| fd 10.3.0 | 40.96 s (40.76 s to 41.01 s) | 40.00 s (39.84 s to 40.03 s) | 40.36 s (40.21 s to 40.39 s) | 40.31 s (40.28 s to 40.71 s) | 0 | 2,079 MB | 15 MB |
| bat 0.26.0 | 14.30 s (14.17 s to 14.47 s) | 13.01 s (12.87 s to 13.39 s) | 13.65 s (13.52 s to 14.04 s) | 13.61 s (13.51 s to 13.70 s) | 0 | 2,731 MB | 23 MB |
| hyperfine 1.19.0 | 6.72 s (6.66 s to 6.77 s) | 6.54 s (6.50 s to 6.62 s) | 6.89 s (6.86 s to 6.98 s) | 6.92 s (6.91 s to 6.96 s) | 0 | 2,147 MB | 10 MB |
| just 1.43.0 | 10.53 s (10.24 s to 10.65 s) | 9.29 s (9.27 s to 9.35 s) | 9.94 s (9.92 s to 10.00 s) | 9.91 s (9.84 s to 10.10 s) | 0 | 3,054 MB | 32 MB |
| axum 0.8.6 | 9.74 s (9.71 s to 9.78 s) | 9.71 s (9.66 s to 9.73 s) | 10.07 s (10.02 s to 10.09 s) | 10.04 s (10.01 s to 10.10 s) | 0 | 2,697 MB | 15 MB |
| nushell 0.108.0 | 53.84 s (53.17 s to 54.31 s) | 40.15 s (39.88 s to 40.57 s) | 42.26 s (41.98 s to 42.67 s) | 42.17 s (41.81 s to 42.41 s) | 0 | 14,956 MB | 199 MB |
| helix 25.07.1 | 45.13 s (44.69 s to 46.11 s) | 36.32 s (36.21 s to 36.69 s) | 39.56 s (39.45 s to 39.94 s) | 39.38 s (39.26 s to 40.03 s) | 0 | 12,579 MB | 1,665 MB |
| bevy 0.20.0 | 121.55 s (120.87 s to 123.11 s) | 86.65 s (86.34 s to 87.02 s) | 116.94 s (116.34 s to 117.31 s) | 118.03 s (117.27 s to 119.04 s) | 0 | 30,593 MB | 6,481 MB |

#### Memory and disk, LLVM, Linux x86-64

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | Cold, until the daemon is ready | 2.4 GB | 3.0 GB | 424 MB | 1.7 GB | 1.9 GB |
| ripgrep 15.1.0 | Edit a function | 527 MB | 3.7 GB | 297 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Edit a widely used function | 536 MB | 3.7 GB | 308 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Change a signature | 529 MB | 3.7 GB | 296 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Edit a generic function | 536 MB | 3.7 GB | 301 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Add a function | 535 MB | 3.7 GB | 297 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Add a struct field | 551 MB | 3.7 GB | 352 MB | 2.2 GB | 3.6 GB |
| ripgrep 15.1.0 | Nothing changed | 29 MB | 3.7 GB | 29 MB | 2.2 GB | 3.7 GB |
| fd 10.3.0 | Cold, until the daemon is ready | 2.6 GB | 3.4 GB | 554 MB | 1.8 GB | 2.0 GB |
| fd 10.3.0 | Edit a function | 605 MB | 3.5 GB | 372 MB | 1.9 GB | 3.4 GB |
| fd 10.3.0 | Edit a widely used function | 599 MB | 3.5 GB | 372 MB | 1.9 GB | 3.4 GB |
| fd 10.3.0 | Change a signature | 610 MB | 3.5 GB | 372 MB | 1.9 GB | 3.4 GB |
| fd 10.3.0 | Add a function | 606 MB | 3.5 GB | 372 MB | 1.9 GB | 3.4 GB |
| fd 10.3.0 | Add a struct field | 635 MB | 3.5 GB | 373 MB | 1.9 GB | 3.4 GB |
| fd 10.3.0 | Nothing changed | 34 MB | 3.5 GB | 34 MB | 2.0 GB | 3.5 GB |
| bat 0.26.0 | Cold, until the daemon is ready | 3.7 GB | 6.5 GB | 519 MB | 2.4 GB | 2.6 GB |
| bat 0.26.0 | Edit a function | 750 MB | 4.1 GB | 520 MB | 2.5 GB | 4.0 GB |
| bat 0.26.0 | Edit a widely used function | 748 MB | 4.1 GB | 519 MB | 2.5 GB | 4.0 GB |
| bat 0.26.0 | Change a signature | 740 MB | 4.1 GB | 518 MB | 2.5 GB | 4.0 GB |
| bat 0.26.0 | Add a function | 750 MB | 4.1 GB | 519 MB | 2.5 GB | 4.0 GB |
| bat 0.26.0 | Add a struct field | 763 MB | 4.1 GB | 517 MB | 2.5 GB | 4.0 GB |
| bat 0.26.0 | Nothing changed | 51 MB | 4.1 GB | 51 MB | 2.5 GB | 4.0 GB |
| hyperfine 1.19.0 | Cold, until the daemon is ready | 2.5 GB | 3.0 GB | 358 MB | 2.1 GB | 2.2 GB |
| hyperfine 1.19.0 | Edit a function | 422 MB | 3.6 GB | 247 MB | 2.1 GB | 3.5 GB |
| hyperfine 1.19.0 | Edit a widely used function | 428 MB | 3.6 GB | 260 MB | 2.1 GB | 3.5 GB |
| hyperfine 1.19.0 | Change a signature | 434 MB | 3.6 GB | 267 MB | 2.1 GB | 3.5 GB |
| hyperfine 1.19.0 | Add a function | 421 MB | 3.6 GB | 249 MB | 2.1 GB | 3.5 GB |
| hyperfine 1.19.0 | Add a struct field | 451 MB | 3.6 GB | 293 MB | 2.1 GB | 3.5 GB |
| hyperfine 1.19.0 | Nothing changed | 41 MB | 3.6 GB | 41 MB | 2.1 GB | 3.6 GB |
| just 1.43.0 | Cold, until the daemon is ready | 3.4 GB | 4.2 GB | 632 MB | 2.2 GB | 2.3 GB |
| just 1.43.0 | Edit a function | 542 MB | 4.2 GB | 359 MB | 2.6 GB | 4.2 GB |
| just 1.43.0 | Edit a widely used function | 533 MB | 4.2 GB | 362 MB | 2.6 GB | 4.2 GB |
| just 1.43.0 | Change a signature | 534 MB | 4.2 GB | 360 MB | 2.6 GB | 4.2 GB |
| just 1.43.0 | Add a function | 539 MB | 4.2 GB | 384 MB | 2.6 GB | 4.2 GB |
| just 1.43.0 | Add a struct field | 645 MB | 4.2 GB | 606 MB | 2.6 GB | 4.2 GB |
| just 1.43.0 | Nothing changed | 39 MB | 4.2 GB | 39 MB | 2.6 GB | 4.2 GB |
| axum 0.8.6 | Cold, until the daemon is ready | 2.5 GB | 3.3 GB | 514 MB | 2.5 GB | 2.8 GB |
| axum 0.8.6 | Edit a function | 466 MB | 4.1 GB | 253 MB | 2.5 GB | 4.0 GB |
| axum 0.8.6 | Edit a widely used function | 455 MB | 4.1 GB | 339 MB | 2.5 GB | 4.0 GB |
| axum 0.8.6 | Add a function | 469 MB | 4.1 GB | 257 MB | 2.5 GB | 4.0 GB |
| axum 0.8.6 | Nothing changed | 72 MB | 4.1 GB | 72 MB | 2.6 GB | 4.0 GB |
| nushell 0.108.0 | Cold, until the daemon is ready | 4.6 GB | 9.2 GB | 2.4 GB | 6.3 GB | 6.6 GB |
| nushell 0.108.0 | Edit a function | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Edit a widely used function | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Change a signature | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Edit a generic function | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Add a function | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Add a struct field | 2.7 GB | 12.7 GB | 2.4 GB | 11.1 GB | 12.6 GB |
| nushell 0.108.0 | Nothing changed | 122 MB | 12.9 GB | 122 MB | 11.3 GB | 12.9 GB |
| helix 25.07.1 | Cold, until the daemon is ready | 3.8 GB | 6.8 GB | 1.8 GB | 4.4 GB | 4.6 GB |
| helix 25.07.1 | Edit a function | 2.1 GB | 14.1 GB | 1.8 GB | 12.6 GB | 14.0 GB |
| helix 25.07.1 | Edit a widely used function | 2.1 GB | 14.1 GB | 1.8 GB | 12.6 GB | 14.0 GB |
| helix 25.07.1 | Change a signature | 2.1 GB | 14.1 GB | 1.8 GB | 12.6 GB | 14.0 GB |
| helix 25.07.1 | Add a function | 2.1 GB | 14.1 GB | 1.8 GB | 12.6 GB | 14.0 GB |
| helix 25.07.1 | Add a struct field | 2.1 GB | 14.1 GB | 1.8 GB | 12.6 GB | 14.0 GB |
| helix 25.07.1 | Nothing changed | 65 MB | 13.9 GB | 65 MB | 12.4 GB | 13.8 GB |
| bevy 0.20.0 | Cold, until the daemon is ready | 8.5 GB | 10.5 GB | 8.1 GB | 10.2 GB | 10.5 GB |
| bevy 0.20.0 | Edit a function | 8.5 GB | 55.9 GB | 8.1 GB | 42.3 GB | 43.5 GB |
| bevy 0.20.0 | Edit a widely used function | 8.5 GB | 55.9 GB | 8.1 GB | 42.3 GB | 43.5 GB |
| bevy 0.20.0 | Add a function | 8.5 GB | 55.9 GB | 8.1 GB | 42.3 GB | 43.5 GB |
| bevy 0.20.0 | Nothing changed | 121 MB | 32.1 GB | 121 MB | 30.5 GB | 31.9 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep 15.1.0 | 1.9 GB | 3.7 GB | 3.6 GB after 65 builds | −36 MB | 3.1 GB, 1.8 GB | 407 MB | 640 MB | 19 MB | 0 MB |
| fd 10.3.0 | 2.0 GB | 3.5 GB | 3.4 GB after 55 builds | −36 MB | 3.4 GB, 2.0 GB | 636 MB | 848 MB | 15 MB | 0 MB |
| bat 0.26.0 | 2.6 GB | 4.0 GB | 4.0 GB after 55 builds | −38 MB | 6.5 GB, 2.6 GB | 973 MB | 1.2 GB | 23 MB | 0 MB |
| hyperfine 1.19.0 | 2.2 GB | 3.6 GB | 3.5 GB after 55 builds | −38 MB | 3.1 GB, 2.2 GB | 342 MB | 457 MB | 10 MB | 0 MB |
| just 1.43.0 | 2.3 GB | 4.2 GB | 4.2 GB after 55 builds | −18 MB | 4.2 GB, 2.2 GB | 667 MB | 945 MB | 32 MB | 0 MB |
| axum 0.8.6 | 2.8 GB | 4.0 GB | 4.0 GB after 35 builds | −38 MB | 3.3 GB, 2.7 GB | 404 MB | 494 MB | 15 MB | 0 MB |
| nushell 0.108.0 | 6.6 GB | 12.9 GB | 12.6 GB after 65 builds | −217 MB | 9.2 GB, 6.5 GB | 5.3 GB | 7.4 GB | 200 MB | 1 MB |
| helix 25.07.1 | 4.6 GB | 13.8 GB | 14.0 GB after 55 builds | +280 MB | 6.7 GB, 4.6 GB | 3.7 GB | 6.0 GB | 1.7 GB | 1 MB |
| bevy 0.20.0 | 10.5 GB | 31.9 GB | 43.5 GB after 35 builds | +11.6 GB | 10.6 GB, 10.6 GB | 17.6 GB | 29.3 GB | 6.5 GB | 1 MB |

#### How fast the program runs, LLVM, Linux x86-64

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| ripgrep 15.1.0 | `rg -uuu --sort path -c \w+_unchecked\(\|unsafe \{ <the toolchain's library source>` | 2.47 s (2.46 s to 2.48 s); 29 MB, 199.8 MB | 2.41 s (2.39 s to 2.44 s); 29 MB, 53.2 MB | 74 ms (73 ms to 75 ms); 29 MB, 30.7 MB |

### Linux x86-64, bun

ra-jit [`7dcfe73922`](https://github.com/indexable-inc/ra-jit/commit/7dcfe739224e0f1b5a65c2bc744375bb0ef33714), 2026-10-22T21:42:28Z, Rust 1.99.0, 3 runs of each edit, measured by hand. Raw: [benchmarks/bun-linux-x86-64.json](benchmarks/bun-linux-x86-64.json).

| Machine | |
|---|---|
| Name | AWS c8a.24xlarge (dev-aws-eph-rajit-bun), pinned to 16 cores with taskset |
| CPU | AMD EPYC 9R45 |
| Cores | 16 |
| Memory (GiB) | 185 |
| OS | Linux |
| Kernel | 6.18.53 |
| rustc | rustc 1.100.0-nightly (574ff7d98 2026-09-14) |
| C compiler | gcc (GCC) 16.2.0 |
| Linker | GNU ld (GNU Binutils) 2.46 |
| LLVM | 23.1.3 |
| Incremental | profile default (on for dev) |
| Compiler cache | none |
| Load average at the start | 3.5, 3.4, 4.0 |

#### Edits, Cranelift, Linux x86-64, bun

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| bun 1.4.3 | Edit a function | 9.12 s | 489 ms | 19x | 9.08 s to 9.26 s | 479 ms to 498 ms | daemon | spliced |
| bun 1.4.3 | Edit a widely used function | 16.48 s | 487 ms | 34x | 16.40 s to 16.51 s | 480 ms to 498 ms | daemon | spliced |
| bun 1.4.3 | Change a signature | 9.17 s | 635 ms | 14x | 9.16 s to 9.24 s | 621 ms to 21.13 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Edit a generic function | 16.73 s | 524 ms | 32x | 16.64 s to 16.76 s | 506 ms to 527 ms | daemon | spliced |
| bun 1.4.3 | Add a function | 14.49 s | 19.80 s | 0.7x | 14.38 s to 14.91 s | 19.73 s to 19.95 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Nothing changed | 74 ms | 19 ms | 3.9x | 74 ms to 93 ms | 17 ms to 20 ms | daemon |  |

#### Cold, Cranelift, Linux x86-64, bun

From an empty target directory, crates already downloaded, one run. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 73.06 s | 70.93 s | 73.34 s | 73.01 s | 0 | 33,820 MB | 183 MB |

#### Memory and disk, Cranelift, Linux x86-64, bun

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| bun 1.4.3 | Cold, until the daemon is ready | 4.5 GB | 15.5 GB | 3.0 GB | 12.3 GB | 12.5 GB |
| bun 1.4.3 | Edit a function | 3.3 GB | 25.8 GB | 3.0 GB | 25.6 GB | 25.8 GB |
| bun 1.4.3 | Edit a widely used function | 3.3 GB | 26.8 GB | 3.0 GB | 26.7 GB | 26.8 GB |
| bun 1.4.3 | Change a signature | 3.3 GB | 26.9 GB | 3.0 GB | 26.7 GB | 26.8 GB |
| bun 1.4.3 | Edit a generic function | 3.3 GB | 26.9 GB | 3.0 GB | 26.7 GB | 26.8 GB |
| bun 1.4.3 | Add a function | 3.3 GB | 33.5 GB | 3.0 GB | 33.3 GB | 26.9 GB |
| bun 1.4.3 | Nothing changed | 56 MB | 25.7 GB | 56 MB | 25.6 GB | 25.7 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 12.5 GB | 25.7 GB | 26.9 GB after 33 builds | +1.2 GB | 15.5 GB, 12.5 GB | 5.7 GB | 7.6 GB | 183 MB | 1 MB |

#### How fast the program runs, Cranelift, Linux x86-64, bun

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| bun 1.4.3 | `bun -e const t = new Bun.Transpiler({ loader: 'ts', minifyWhitespace: true }); const fs = require('fs'); const sources = ['http2', 'net', '_http_server', 'fs', 'child_process'].map(n => fs.readFileSync('src/js/node/' + n + '.ts', 'utf8')); let n = 0; for (let i = 0; i < 20; i++) for (const s of sources) n += t.transformSync(s).length; console.log(n)` | 1.58 s (1.58 s to 1.58 s); 106 MB, 586.1 MB | 966 ms (965 ms to 966 ms); 79 MB, 397.9 MB | 135 ms (135 ms to 137 ms); 54 MB, 314.0 MB |

#### Edits, LLVM, Linux x86-64, bun

Median, then the fastest and slowest run. Built: who built the edit (daemon, or cargo and why). Link: the change was spliced into the program, or it was linked again.

| Project | Edit | cargo | ra-jit | | cargo runs | ra-jit runs | Built | Link |
|---|---|---:|---:|---:|---|---|---|---|
| bun 1.4.3 | Edit a function | 9.13 s | 494 ms | 18x | 9.11 s to 9.21 s | 493 ms to 510 ms | daemon | spliced |
| bun 1.4.3 | Edit a widely used function | 16.37 s | 502 ms | 33x | 16.34 s to 16.41 s | 499 ms to 514 ms | daemon | spliced |
| bun 1.4.3 | Change a signature | 9.16 s | 618 ms | 15x | 9.15 s to 9.30 s | 602 ms to 21.94 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Edit a generic function | 16.78 s | 527 ms | 32x | 16.71 s to 16.91 s | 516 ms to 535 ms | daemon | spliced |
| bun 1.4.3 | Add a function | 14.62 s | 18.38 s | 0.8x | 14.47 s to 14.85 s | 18.33 s to 18.55 s | daemon | full link: the whole object was written again |
| bun 1.4.3 | Nothing changed | 73 ms | 25 ms | 2.9x | 73 ms to 75 ms | 24 ms to 32 ms | daemon |  |

#### Cold, LLVM, Linux x86-64, bun

From an empty target directory, crates already downloaded, one run. `cargo ra-jit build` is cargo's own build (the daemon is not loaded yet); the daemon then loads the workspace and builds it once, and edits are fast from then on. Warm store: the same in another checkout of the same commit, with the store the first one filled.

| Project | `cargo build` | `cargo ra-jit build` | Daemon ready, empty store | Daemon ready, warm store | Functions compiled | Daemon memory | Store |
|---|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 79.24 s | 73.28 s | 75.78 s | 72.76 s | 0 | 32,393 MB | 115 MB |

#### Memory and disk, LLVM, Linux x86-64, bun

Peak: the most every process doing the work held at once, sampled every 10 ms and never below the largest process, median of the runs. Largest process: the peak of the one process that held the most, as the kernel kept it: `wait4`'s for the command's tree, and each process's peak sampled (on macOS the kernel's peak footprint of each, counting for the daemon only a peak it passed during the step). ra-jit is the daemon, its workers and what they run, and the `cargo ra-jit build` with any cargo it ran; cargo is cargo itself, rustc, build scripts and the linker. Daemon after: what the daemon and its workers hold once the step is over ([how](#memory-and-disk)).

| Project | Step | cargo, peak | ra-jit, peak | cargo, largest process | ra-jit, largest process | Daemon after |
|---|---|---:|---:|---:|---:|---:|
| bun 1.4.3 | Cold, until the daemon is ready | 4.5 GB | 15.4 GB | 3.0 GB | 12.1 GB | 12.3 GB |
| bun 1.4.3 | Edit a function | 3.3 GB | 25.3 GB | 3.0 GB | 25.1 GB | 25.2 GB |
| bun 1.4.3 | Edit a widely used function | 3.3 GB | 26.5 GB | 3.0 GB | 26.2 GB | 26.4 GB |
| bun 1.4.3 | Change a signature | 3.3 GB | 26.5 GB | 3.0 GB | 26.2 GB | 26.4 GB |
| bun 1.4.3 | Edit a generic function | 3.3 GB | 26.5 GB | 3.0 GB | 26.2 GB | 26.4 GB |
| bun 1.4.3 | Add a function | 3.3 GB | 32.9 GB | 3.0 GB | 32.6 GB | 26.5 GB |
| bun 1.4.3 | Nothing changed | 56 MB | 25.3 GB | 56 MB | 25.0 GB | 25.2 GB |

The daemon between builds, and what each side keeps on disk once the edits are done (as `du` counts it). After the first build: once the step after the cold build (nothing changed) is done; growth is from then to the end, over every edit made and taken back. Warm store: the cold start of another checkout with the store the first one filled.

| Project | Daemon ready | After the first build | At the end | Growth | Warm store: peak, ready | cargo target | ra-jit target | Store | Home |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| bun 1.4.3 | 12.3 GB | 25.2 GB | 26.5 GB after 33 builds | +1.3 GB | 15.3 GB, 12.2 GB | 5.7 GB | 7.4 GB | 116 MB | 1 MB |

#### How fast the program runs, LLVM, Linux x86-64, bun

The same command run by the program ra-jit linked and by cargo's debug and release builds, in turn. Median, then fastest and slowest; where measured, then the program's peak resident memory while it ran (median of the runs, as the kernel kept it) and the size of the program.

| Project | Command | ra-jit | cargo debug | cargo release |
|---|---|---:|---:|---:|
| bun 1.4.3 | `bun -e const t = new Bun.Transpiler({ loader: 'ts', minifyWhitespace: true }); const fs = require('fs'); const sources = ['http2', 'net', '_http_server', 'fs', 'child_process'].map(n => fs.readFileSync('src/js/node/' + n + '.ts', 'utf8')); let n = 0; for (let i = 0; i < 20; i++) for (const s of sources) n += t.transformSync(s).length; console.log(n)` | 3.72 s (3.71 s to 3.72 s); 91 MB, 524.2 MB | 969 ms (968 ms to 970 ms); 79 MB, 397.9 MB | 135 ms (135 ms to 136 ms); 54 MB, 314.0 MB |

<!-- bench:end -->
