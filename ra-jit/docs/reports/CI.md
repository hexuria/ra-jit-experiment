# CI: where the fast tier's time went, and the merge queue (2026-10-22)

What CI does is in [harness.md](../harness.md#ci). This is what I measured before changing it,
and why it changed as it did.

## What I measured

I pulled every run of `ra-jit.yaml` since 2026-10-20 (269 runs) with `gh api
.../actions/workflows/ra-jit.yaml/runs` and the jobs and steps of each, and read the logs of the
slowest jobs. The numbers below are for the runs after the fast and full tiers split
([#67](https://github.com/indexable-inc/ra-jit/pull/67), 03:52Z), unless they say otherwise.
Times are wall times from the run's start; "queue wait" is a job's creation to its start.

- **The fast tier on main: 13.3 minutes at the median, 20.4 at p90** (22 runs that built code,
  03:52Z to 20:13Z). From #97's merge at 20:13Z on, every run took 33 to 36 minutes (4 of 4).
- **Queue wait is nothing**: p50 0.1 minutes, p90 under 0.5, on every kind of runner. The
  organization is on GitHub Enterprise (500 concurrent jobs, 50 of them macOS) and the
  repository is public, so hosted runners cost nothing and are never short.
- **Building is not the long part.** sccache hit 75 to 97 percent of compilations on most runs
  (a few after a `Cargo.lock` change hit 0 to 8 percent); the `cargo build` step took 1.8 minutes
  at the median on Linux x86-64, 1.7 on AArch64 and 2.4 on macOS.
- **macOS `quick-2`: `norustc.py` took 1489 of the part's 1730 seconds** (run 39851906782), where
  it takes 66 to 70 seconds on Linux. The harness came with #97. On macOS its `RA_JIT_HOME` is
  under `$TMPDIR` (`/var/folders/...`), the daemon's socket path inside it passes the 104 bytes
  macOS allows, so the daemon never listened: the cold build ran cargo 623 times (28 on Linux)
  and each scenario ran until the log was still. It records nothing there either: the baseline
  has counts for `linux-x86_64 cranelift` alone.
- **Linux x86-64 `quick-1`: `apt-get install valgrind gdb` stalls.** 24 of 929 runs of that step
  since 2026-10-21 took over two minutes, 230 minutes over one in all; the stalls are 8 to 12
  minutes each (11 min 41 s for 26 MB at 37 kB/s from azure.archive.ubuntu.com in one) and came
  in clusters, in every run from 17:28Z to 20:06Z. Of the 14 runs on main over 15 minutes since
  the split, 7 ended on such a job and 5 on macOS `quick-2`. The LLVM install (`apt.llvm.org`) in
  the toolchain step took over two minutes in 59 of 1693 runs.
- **The longest path when nothing stalls** is the macOS build (about 4 minutes, with the toolchain
  and LLVM from Homebrew) and then its slowest harness part (`quick-1` under Cranelift, 5.6
  minutes of harness and 0.7 of setup): about 11 minutes.
- **The Actions cache is over its 10 GB**: 10.7 GB in 1813 entries. rust-cache's entries are
  0.8 GB each (two per key for six keys of this workflow, more for others); sccache's are
  1.6 GB in about 1300 small entries, 0.5 GB of them written by pull requests, where nothing
  else can read them; Nix's from `nix-live.yaml` 1.2 GB, half from pull requests. The oldest
  sccache entry was 18 hours old: eviction, not the 7-day limit, is what clears it.
- **Failures.** No run was re-run. On main, 7 code runs in a row failed from 04:54Z to 17:22Z
  (12.5 hours), all on macOS `quick-1`, until the fix for #74 landed: lanes merged onto a red main
  and pushed, since nothing stopped them. One job failed in infrastructure (an artifact download
  on macOS). Pull requests: of 23 finished runs with code, 8 failed and 8 were cancelled by a
  newer push.
- **How main moved.** 82 merges on main's first-parent line on 2026-10-21 and 22; 727 of the
  commits since 2026-10-15 are Andrew's identity (the lanes), each pushed straight to main past
  the ruleset, which was in `evaluate`. The ruleset's log shows every such push as `bypass`, and
  the `github-actions[bot]` pushes (`pgo.yaml`'s pin, the bench numbers) as `fail`: they are
  refused once it is active.

## What changed

- `norustc.py` keeps its work directory under `/tmp` on macOS, as `cargo_ra_jit.py`,
  `loadedits.py` and `headerpad.py` already did. Expected: macOS `quick-2` back to about 5
  minutes, the fast tier back to about 11.
- The harness jobs install their Ubuntu packages through `.github/actions/apt`, which keeps the
  `.deb` files in the Actions cache per runner image (written by main). A stall then costs the
  first run on a new image, not one run in thirty.
- sccache is read-only off main (`SCCACHE_GHA_RW_MODE`), as rust-cache already was.
- **The merge queue.** The ruleset is active with a `merge_queue` rule: merge commits, up to five
  groups built at once and five pull requests merged together, every entry green
  (`ALLGREEN`), a check timeout of 60 minutes, `ci` required. A pull request's own run is
  `fmt` alone, so it enters the queue within a minute and its landing waits for one fast tier,
  not two. Main moves only to a commit the fast tier passed on. Lanes land with
  `.github/scripts/land.py`, which opens the pull request, queues it and waits for the result.
  The repository now deletes a merged branch, allows auto-merge (a pull request waits for its
  checks and then queues itself), and titles a merge commit with the pull request's title.
- The workflows that commit to main check out with `RA_JIT_BOT_TOKEN` when the secret is set,
  since the workflow's own token cannot pass the ruleset.

## What was not done, and why

- **Larger or self-hosted runners.** Nothing waits for a runner, and the longest path is two
  jobs of 4 and 6 minutes; a larger macOS runner (paid) would take perhaps 2 minutes off it. A
  self-hosted runner on a public repository runs the code of whoever opens a pull request, and
  with the queue only queued pull requests run the fast tier at all. Neither is worth it yet.
- **Skipping the run on main after a queue merge.** The queue moves main to the commit it
  tested, so that run tests it again; but it is the run that writes main's caches, which every
  queue entry reads, and its runners are free.
- **The cache's size.** GitHub sells more than 10 GB; with pull requests and queue entries no
  longer writing sccache, the space should go to main's entries. `nix-live.yaml`'s pull
  request entries (0.65 GB) are still written.
