<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/ix-mark-on-dark.svg">
    <img alt="ix" src="docs/brand/ix-mark-on-light.svg" height="40" align="absmiddle">
  </picture>
  ra-jit
</h1>

Edit Rust. See it change in milliseconds, in the program that is already running.

```
nix run github:indexable-inc/ra-jit
```

That opens a Game of Life window you can edit while it runs (live on macOS Apple silicon).

![Editing the running Game of Life: rule, colours and speed change live](docs/demo.avif)

<p align="center">
  <a href="https://ra-jit.ix.dev/updates?src=readme">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="docs/brand/signup-on-dark.svg">
      <img alt="Tell us you use ra-jit" src="docs/brand/signup-on-light.svg" height="36">
    </picture>
  </a>
</p>

<p align="center">For release news: Watch → Custom → Releases on GitHub.</p>

## What you get

- **Live edits.** Save a file and the running program has the new code about a millisecond
  later. Its state stays. ([docs/live.md](docs/live.md))
- **Fast builds.** After the first build, an edit rebuilds only the functions you changed.
  No `rustc`, no waiting.
- **Same cargo.** `cargo ra-jit build|run|test|check` in any project.

## How fast

Live edit to running code, Game of Life demo (median of 20 edits, one 96-core Linux machine):

| Edit | incremental `cargo build` | ra-jit live | |
|---|---:|---:|---:|
| One function | 169 ms | 7 ms | **24x** |
| One function, 300 new functions added | 282 ms | 60 ms | 4.7x |

<!-- bench:start -->
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/benchmarks/charts/edits-dark.svg"><img alt="How much faster an edit builds under ra-jit than under cargo, by platform: All projects (geometric mean): macOS arm64 3.8× faster, Linux aarch64 17× faster, Linux x86-64 11× faster; ripgrep 15.1.0: macOS arm64 8.1× faster, Linux aarch64 25× faster, Linux x86-64 14× faster; fd 10.3.0: macOS arm64 6.0× faster, Linux aarch64 27× faster, Linux x86-64 12× faster; bat 0.26.0: Linux aarch64 39× faster, Linux x86-64 19× faster; hyperfine 1.19.0: Linux aarch64 19× faster, Linux x86-64 12× faster; just 1.43.0: Linux aarch64 26× faster, Linux x86-64 16× faster; axum 0.8.6: Linux aarch64 16× faster, Linux x86-64 10× faster; nushell 0.108.0: macOS arm64 10× faster, Linux aarch64 40× faster, Linux x86-64 20× faster; helix 25.07.1: Linux aarch64 52× faster, Linux x86-64 28× faster; bevy 0.20.0: Linux aarch64 4.4× slower, Linux x86-64 2.9× slower; bun 1.4.3: macOS arm64 2.3× slower, Linux x86-64 19× faster" src="docs/benchmarks/charts/edits-light.svg" width="760"></picture>

Edit, then build, on real projects at pinned commits: incremental `cargo build` → `cargo ra-jit build`, both warm, same linker, Cranelift, the median of each run's edits. Each platform is a machine of its own: compare numbers within a platform, not across.

An edit to one function, cargo → ra-jit:

| Project | macOS arm64 | Linux aarch64 | Linux x86-64 |
|---|---:|---:|---:|
| ripgrep 15.1.0 | 993 ms → **126 ms** (7.9x) | 1.27 s → **56 ms** (23x) | 787 ms → **69 ms** (11x) |
| fd 10.3.0 | 630 ms → **109 ms** (5.8x) | 1.35 s → **49 ms** (27x) | 752 ms → **61 ms** (12x) |
| bat 0.26.0 | - | 1.71 s → **54 ms** (32x) | 774 ms → **61 ms** (13x) |
| hyperfine 1.19.0 | - | 787 ms → **39 ms** (20x) | 616 ms → **47 ms** (13x) |
| just 1.43.0 | - | 1.57 s → **41 ms** (38x) | 1.04 s → **52 ms** (20x) |
| axum 0.8.6 | - | 855 ms → **52 ms** (16x) | 668 ms → **66 ms** (10x) |
| nushell 0.108.0 | 1.81 s → **454 ms** (4.0x) | 7.50 s → **295 ms** (25x) | 1.83 s → **293 ms** (6.3x) |
| helix 25.07.1 | - | 7.21 s → **78 ms** (92x) | 3.07 s → **90 ms** (34x) |
| bevy 0.20.0 | - | 23.76 s → **105.76 s** (0.2x) | 3.96 s → **11.60 s** (0.3x) |
| bun 1.4.3 | 12.16 s → **1.09 s** (11x) | - | 9.12 s → **489 ms** (19x) |

- **macOS arm64**: MacBook Pro (Mac17,6), macOS 27.0.1, with other work running (Apple M5 Max, 18 cores, 128 GiB), ra-jit [`84760c3f51`](https://github.com/indexable-inc/ra-jit/commit/84760c3f517a76f6e8a3aa41ccd81fc3f26b87c7) on 2026-10-21, median of 3 edits, load average 48 at the start, by hand.
- **Linux aarch64**: AWS c8g.16xlarge, pinned to 16 cores with taskset (Neoverse-V2, 16 of its 64 cores, 124 GiB), ra-jit [`2cf189feb6`](https://github.com/indexable-inc/ra-jit/commit/2cf189feb641a25a8c0d26391756f9c217c56bc2) on 2026-10-22, median of 5 edits, load average 10 at the start, by hand.
- **Linux x86-64**: AWS c7a.48xlarge (dev-aws-8), pinned to 16 cores with taskset (AMD EPYC 9R14, 16 of its 192 cores, 370 GiB), ra-jit [`2cf189feb6`](https://github.com/indexable-inc/ra-jit/commit/2cf189feb641a25a8c0d26391756f9c217c56bc2) on 2026-10-22, median of 5 edits, load average 13 at the start, by hand.
- **bun** is measured on its own, by the `bench-bun` workflow: macOS arm64 on one machine (Apple M5 Max, 18 cores, 128 GiB), ra-jit [`1645168dc4`](https://github.com/indexable-inc/ra-jit/commit/1645168dc4abbdfab2e234232efb73a5c1ec2935) on 2026-10-21, median of 3 edits, load average 28 at the start, by hand; Linux x86-64 on AWS c8a.24xlarge (dev-aws-eph-rajit-bun), pinned to 16 cores with taskset (AMD EPYC 9R45, 16 of its 96 cores, 185 GiB), ra-jit [`7dcfe73922`](https://github.com/indexable-inc/ra-jit/commit/7dcfe739224e0f1b5a65c2bc744375bb0ef33714) on 2026-10-22, median of 3 edits, by hand. It is the Rust port of [bun](https://github.com/oven-sh/bun), 1.25M lines of Rust in 103 crates on a nightly beside a JavaScript engine's C and C++, built with frame pointers as bun's own build is (`-C force-frame-pointers=yes`; [what it took](docs/research/bun.md)). Its test suites are not measured yet.

<details><summary>Every kind of edit, by platform</summary>

**macOS arm64**

| Project | Edit a function | Edit a widely used function | Change a signature | Nothing changed |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 993 ms → **126 ms** (7.9x) | 1.31 s → **159 ms** (8.2x) | 828 ms → **175 ms** (4.7x) | 67 ms → **54 ms** (1.2x) |
| fd 10.3.0 | 630 ms → **109 ms** (5.8x) | 734 ms → **102 ms** (7.2x) | 681 ms → **113 ms** (6.0x) | 144 ms → **49 ms** (2.9x) |
| nushell 0.108.0 | 1.81 s → **454 ms** (4.0x) | 14.39 s → **850 ms** (17x) | 10.28 s → **716 ms** (14x) | 407 ms → **45 ms** (9.0x) |
| bun 1.4.3 | 12.16 s → **1.09 s** (11x) | 26.75 s → **1.81 s** (15x) | 9.83 s → **126.44 s** (c) (0.1x) | 168 ms → **73 ms** (2.3x) |

**Linux aarch64**

| Project | Edit a function | Edit a widely used function | Change a signature | Nothing changed |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 1.27 s → **56 ms** (23x) | 1.65 s → **43 ms** (38x) | 1.44 s → **87 ms** (16x) | 27 ms → **20 ms** (1.3x) |
| fd 10.3.0 | 1.35 s → **49 ms** (27x) | 1.30 s → **36 ms** (36x) | 1.36 s → **65 ms** (21x) | 35 ms → **18 ms** (1.9x) |
| bat 0.26.0 | 1.71 s → **54 ms** (32x) | 2.00 s → **41 ms** (49x) | 1.95 s → **47 ms** (42x) | 67 ms → **18 ms** (3.7x) |
| hyperfine 1.19.0 | 787 ms → **39 ms** (20x) | 828 ms → **41 ms** (20x) | 836 ms → **91 ms** (9.2x) | 46 ms → **18 ms** (2.6x) |
| just 1.43.0 | 1.57 s → **41 ms** (38x) | 1.66 s → **42 ms** (40x) | 1.59 s → **73 ms** (22x) | 45 ms → **17 ms** (2.6x) |
| axum 0.8.6 | 855 ms → **52 ms** (16x) | 1.44 s → **44 ms** (33x) |  | 78 ms → **19 ms** (4.1x) |
| nushell 0.108.0 | 7.50 s → **295 ms** (25x) | 11.86 s → **273 ms** (43x) | 11.75 s → **323 ms** (36x) | 198 ms → **22 ms** (9.0x) |
| helix 25.07.1 | 7.21 s → **78 ms** (92x) | 8.58 s → **91 ms** (94x) | 8.00 s → **246 ms** (32x) | 96 ms → **19 ms** (5.1x) |
| bevy 0.20.0 | 23.76 s → **105.76 s** (0.2x) | 33.52 s → **105.54 s** (0.3x) |  | 226 ms → **23 ms** (9.8x) |

**Linux x86-64**

| Project | Edit a function | Edit a widely used function | Change a signature | Nothing changed |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 787 ms → **69 ms** (11x) | 1.18 s → **54 ms** (22x) | 957 ms → **95 ms** (10x) | 36 ms → **25 ms** (1.4x) |
| fd 10.3.0 | 752 ms → **61 ms** (12x) | 716 ms → **49 ms** (15x) | 759 ms → **76 ms** (10.0x) | 50 ms → **23 ms** (2.2x) |
| bat 0.26.0 | 774 ms → **61 ms** (13x) | 1.12 s → **50 ms** (22x) | 1.06 s → **57 ms** (19x) | 95 ms → **23 ms** (4.1x) |
| hyperfine 1.19.0 | 616 ms → **47 ms** (13x) | 660 ms → **48 ms** (14x) | 667 ms → **98 ms** (6.8x) | 67 ms → **24 ms** (2.8x) |
| just 1.43.0 | 1.04 s → **52 ms** (20x) | 1.07 s → **52 ms** (21x) | 1.05 s → **83 ms** (13x) | 65 ms → **23 ms** (2.8x) |
| axum 0.8.6 | 668 ms → **66 ms** (10x) | 1.31 s → **57 ms** (23x) |  | 117 ms → **25 ms** (4.7x) |
| nushell 0.108.0 | 1.83 s → **293 ms** (6.3x) | 6.76 s → **301 ms** (22x) | 6.83 s → **372 ms** (18x) | 282 ms → **30 ms** (9.4x) |
| helix 25.07.1 | 3.07 s → **90 ms** (34x) | 4.66 s → **105 ms** (44x) | 3.98 s → **270 ms** (15x) | 134 ms → **26 ms** (5.2x) |
| bevy 0.20.0 | 3.96 s → **11.60 s** (0.3x) | 15.07 s → **12.85 s** (1.2x) |  | 305 ms → **33 ms** (9.2x) |
| bun 1.4.3 | 9.12 s → **489 ms** (19x) | 16.48 s → **487 ms** (34x) | 9.17 s → **635 ms** (14x) | 74 ms → **19 ms** (3.9x) |

</details>

(c): the daemon handed this edit to cargo (a fallback, so about cargo's time).

Measured by hand with `harness/bench.py`, or by the `bench-readme` workflow (a project measured on its own, by its own workflow) on each platform's benchmark machine once it is registered. The ra-jit measured is `cargo build --release`; the binaries a release ships are the `dist` profile with PGO, which start and edit faster ([how](https://github.com/indexable-inc/ra-jit/blob/main/docs/reports/RELEASE-BUILD.md)). Cold builds, the spread of every number, the LLVM tier and how fast the built programs run: [docs/benchmarks.md](docs/benchmarks.md). Raw JSON: [macOS arm64](docs/benchmarks/macos-arm64.json), [Linux aarch64](docs/benchmarks/linux-aarch64.json), [Linux x86-64](docs/benchmarks/linux-x86-64.json); bun: [macOS arm64](docs/benchmarks/bun-macos-arm64.json), [Linux x86-64](docs/benchmarks/bun-linux-x86-64.json).
<!-- bench:end -->

<!-- benchmem:start -->
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/benchmarks/charts/memory-dark.svg"><img alt="Peak memory while an edit to one function builds, ra-jit against cargo, by platform: All projects (geometric mean): macOS arm64 12× more, Linux aarch64 7.7× more, Linux x86-64 6.6× more; ripgrep 15.1.0: macOS arm64 12× more · 2.9 GB vs cargo's 235 MB, Linux aarch64 7.9× more · 3.7 GB vs cargo's 463 MB, Linux x86-64 6.7× more · 3.5 GB vs cargo's 531 MB; fd 10.3.0: macOS arm64 12× more · 2.6 GB vs cargo's 212 MB, Linux aarch64 6.6× more · 3.4 GB vs cargo's 519 MB, Linux x86-64 5.4× more · 3.3 GB vs cargo's 605 MB; bat 0.26.0: Linux aarch64 6.0× more · 4.0 GB vs cargo's 672 MB, Linux x86-64 5.2× more · 3.9 GB vs cargo's 749 MB; hyperfine 1.19.0: Linux aarch64 10× more · 3.6 GB vs cargo's 360 MB, Linux x86-64 8.1× more · 3.4 GB vs cargo's 422 MB; just 1.43.0: Linux aarch64 9.5× more · 4.1 GB vs cargo's 434 MB, Linux x86-64 7.5× more · 4.0 GB vs cargo's 539 MB; axum 0.8.6: Linux aarch64 10× more · 4.0 GB vs cargo's 399 MB, Linux x86-64 8.4× more · 3.9 GB vs cargo's 468 MB; nushell 0.108.0: macOS arm64 12× more · 12.5 GB vs cargo's 1.1 GB, Linux aarch64 5.3× more · 12.7 GB vs cargo's 2.4 GB, Linux x86-64 4.6× more · 12.5 GB vs cargo's 2.7 GB; helix 25.07.1: Linux aarch64 7.9× more · 14.2 GB vs cargo's 1.8 GB, Linux x86-64 6.6× more · 13.9 GB vs cargo's 2.1 GB; bevy 0.20.0: Linux aarch64 7.5× more · 50.4 GB vs cargo's 6.7 GB, Linux x86-64 6.6× more · 55.8 GB vs cargo's 8.5 GB; bun 1.4.3: macOS arm64 14× more · 28.7 GB vs cargo's 2.1 GB, Linux x86-64 7.9× more · 25.8 GB vs cargo's 3.3 GB" src="docs/benchmarks/charts/memory-light.svg" width="760"></picture>

<picture><source media="(prefers-color-scheme: dark)" srcset="docs/benchmarks/charts/disk-dark.svg"><img alt="What a project's build keeps on disk, ra-jit against cargo, by platform: All projects (geometric mean): macOS arm64 1.5× more, Linux aarch64 1.5× more, Linux x86-64 1.5× more; ripgrep 15.1.0: macOS arm64 1.4× more · 598 MB vs cargo's 416 MB, Linux aarch64 1.6× more · 629 MB vs cargo's 404 MB, Linux x86-64 1.6× more · 631 MB vs cargo's 407 MB; fd 10.3.0: macOS arm64 1.3× more · 460 MB vs cargo's 355 MB, Linux aarch64 1.3× more · 976 MB vs cargo's 780 MB, Linux x86-64 1.4× more · 863 MB vs cargo's 636 MB; bat 0.26.0: Linux aarch64 1.4× more · 1.3 GB vs cargo's 980 MB, Linux x86-64 1.3× more · 1.3 GB vs cargo's 973 MB; hyperfine 1.19.0: Linux aarch64 1.4× more · 466 MB vs cargo's 340 MB, Linux x86-64 1.4× more · 466 MB vs cargo's 342 MB; just 1.43.0: Linux aarch64 1.5× more · 971 MB vs cargo's 664 MB, Linux x86-64 1.5× more · 980 MB vs cargo's 667 MB; axum 0.8.6: Linux aarch64 1.3× more · 511 MB vs cargo's 405 MB, Linux x86-64 1.3× more · 507 MB vs cargo's 404 MB; nushell 0.108.0: macOS arm64 1.3× more · 6.9 GB vs cargo's 5.3 GB, Linux aarch64 1.4× more · 7.5 GB vs cargo's 5.3 GB, Linux x86-64 1.4× more · 7.5 GB vs cargo's 5.3 GB; helix 25.07.1: Linux aarch64 2.1× more · 7.8 GB vs cargo's 3.7 GB, Linux x86-64 2.1× more · 7.8 GB vs cargo's 3.7 GB; bevy 0.20.0: Linux aarch64 2.2× more · 37.8 GB vs cargo's 17.5 GB, Linux x86-64 2.0× more · 35.9 GB vs cargo's 17.6 GB; bun 1.4.3: macOS arm64 2.2× more · 12.7 GB vs cargo's 5.9 GB, Linux x86-64 1.4× more · 7.8 GB vs cargo's 5.7 GB" src="docs/benchmarks/charts/disk-light.svg" width="760"></picture>

Memory and disk, Cranelift, from the same runs as the build table. Peak: the most every process doing the work held at once, cargo → ra-jit. For ra-jit that is the daemon, its workers and what they run, and `cargo ra-jit build` with any cargo it ran; for plain cargo, cargo itself, rustc, build scripts and the linker. How it is measured: [docs/benchmarks.md](docs/benchmarks.md#memory-and-disk).

Peak while an edit to one function builds, cargo → ra-jit:

| Project | macOS arm64 | Linux aarch64 | Linux x86-64 |
|---|---:|---:|---:|
| ripgrep 15.1.0 | 235 MB → **2.9 GB** | 463 MB → **3.7 GB** | 531 MB → **3.5 GB** |
| fd 10.3.0 | 212 MB → **2.6 GB** | 519 MB → **3.4 GB** | 605 MB → **3.3 GB** |
| bat 0.26.0 | - | 672 MB → **4.0 GB** | 749 MB → **3.9 GB** |
| hyperfine 1.19.0 | - | 360 MB → **3.6 GB** | 422 MB → **3.4 GB** |
| just 1.43.0 | - | 434 MB → **4.1 GB** | 539 MB → **4.0 GB** |
| axum 0.8.6 | - | 399 MB → **4.0 GB** | 468 MB → **3.9 GB** |
| nushell 0.108.0 | 1.1 GB → **12.5 GB** | 2.4 GB → **12.7 GB** | 2.7 GB → **12.5 GB** |
| helix 25.07.1 | - | 1.8 GB → **14.2 GB** | 2.1 GB → **13.9 GB** |
| bevy 0.20.0 | - | 6.7 GB → **50.4 GB** | 8.5 GB → **55.8 GB** |
| bun 1.4.3 | 2.1 GB → **28.7 GB** | - | 3.3 GB → **25.8 GB** |

<details><summary>The daemon between edits, cold builds and disk, by platform</summary>

**macOS arm64**

| Project | Edit a function, peak | Daemon between edits | Cold until ready, peak | On disk |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 235 MB → **2.9 GB** | 2.2 GB ready, 2.8 GB after 39 builds | 541 MB → **2.4 GB** | 416 MB → 598 MB |
| fd 10.3.0 | 212 MB → **2.6 GB** | 1.8 GB ready, 2.6 GB after 33 builds | 897 MB → **1.9 GB** | 355 MB → 460 MB |
| nushell 0.108.0 | 1.1 GB → **12.5 GB** | 11.5 GB ready, 12.5 GB after 39 builds | 2.4 GB → **16.1 GB** | 5.3 GB → 6.9 GB |
| bun 1.4.3 | 2.1 GB → **28.7 GB** | 27.6 GB ready, 28.2 GB after 29 builds | 3.1 GB → **37.6 GB** | 5.9 GB → 12.7 GB |

**Linux aarch64**

| Project | Edit a function, peak | Daemon between edits | Cold until ready, peak | On disk |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 463 MB → **3.7 GB** | 1.9 GB ready, 3.6 GB after 65 builds | 1.9 GB → **2.7 GB** | 404 MB → 629 MB |
| fd 10.3.0 | 519 MB → **3.4 GB** | 2.0 GB ready, 3.4 GB after 55 builds | 2.2 GB → **3.6 GB** | 780 MB → 976 MB |
| bat 0.26.0 | 672 MB → **4.0 GB** | 2.6 GB ready, 4.0 GB after 55 builds | 3.4 GB → **6.1 GB** | 980 MB → 1.3 GB |
| hyperfine 1.19.0 | 360 MB → **3.6 GB** | 2.2 GB ready, 3.6 GB after 55 builds | 2.2 GB → **2.6 GB** | 340 MB → 466 MB |
| just 1.43.0 | 434 MB → **4.1 GB** | 2.4 GB ready, 4.1 GB after 55 builds | 3.1 GB → **3.7 GB** | 664 MB → 971 MB |
| axum 0.8.6 | 399 MB → **4.0 GB** | 2.7 GB ready, 4.0 GB after 35 builds | 2.1 GB → **3.2 GB** | 405 MB → 511 MB |
| nushell 0.108.0 | 2.4 GB → **12.7 GB** | 5.8 GB ready, 12.6 GB after 65 builds | 4.6 GB → **8.3 GB** | 5.3 GB → 7.5 GB |
| helix 25.07.1 | 1.8 GB → **14.2 GB** | 4.6 GB ready, 14.1 GB after 55 builds | 3.5 GB → **6.4 GB** | 3.7 GB → 7.8 GB |
| bevy 0.20.0 | 6.7 GB → **50.4 GB** | 8.9 GB ready, 42.1 GB after 35 builds | 6.8 GB → **11.1 GB** | 17.5 GB → 37.8 GB |

**Linux x86-64**

| Project | Edit a function, peak | Daemon between edits | Cold until ready, peak | On disk |
|---|---:|---:|---:|---:|
| ripgrep 15.1.0 | 531 MB → **3.5 GB** | 1.8 GB ready, 3.5 GB after 65 builds | 2.4 GB → **2.9 GB** | 407 MB → 631 MB |
| fd 10.3.0 | 605 MB → **3.3 GB** | 1.9 GB ready, 3.3 GB after 55 builds | 2.7 GB → **3.2 GB** | 636 MB → 863 MB |
| bat 0.26.0 | 749 MB → **3.9 GB** | 2.5 GB ready, 3.9 GB after 55 builds | 3.7 GB → **5.8 GB** | 973 MB → 1.3 GB |
| hyperfine 1.19.0 | 422 MB → **3.4 GB** | 2.1 GB ready, 3.4 GB after 55 builds | 2.5 GB → **3.0 GB** | 342 MB → 466 MB |
| just 1.43.0 | 539 MB → **4.0 GB** | 2.2 GB ready, 4.0 GB after 55 builds | 3.4 GB → **4.2 GB** | 667 MB → 980 MB |
| axum 0.8.6 | 468 MB → **3.9 GB** | 2.6 GB ready, 3.9 GB after 35 builds | 2.5 GB → **3.3 GB** | 404 MB → 507 MB |
| nushell 0.108.0 | 2.7 GB → **12.5 GB** | 6.1 GB ready, 12.5 GB after 65 builds | 4.6 GB → **9.0 GB** | 5.3 GB → 7.5 GB |
| helix 25.07.1 | 2.1 GB → **13.9 GB** | 4.5 GB ready, 13.9 GB after 55 builds | 3.8 GB → **6.5 GB** | 3.7 GB → 7.8 GB |
| bevy 0.20.0 | 8.5 GB → **55.8 GB** | 10.4 GB ready, 43.3 GB after 35 builds | 8.5 GB → **10.4 GB** | 17.6 GB → 35.9 GB |
| bun 1.4.3 | 3.3 GB → **25.8 GB** | 12.5 GB ready, 26.9 GB after 33 builds | 4.5 GB → **15.5 GB** | 5.7 GB → 7.8 GB |

On disk: cargo's target directory → ra-jit's target directory, store and home.

</details>
<!-- benchmem:end -->

<!-- runbench:start -->
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/benchmarks/charts/runtime-dark.svg"><img alt="How fast each benchmark runs built by ra-jit, against cargo build --release, by platform: All benchmarks (geometric mean): macOS arm64 1.18× slower, Linux aarch64 1.16× slower, Linux x86-64 1.24× slower; sieve: macOS arm64 1.12× slower, Linux aarch64 1.06× slower, Linux x86-64 1.02× slower; collatz: macOS arm64 1.01× slower, Linux aarch64 1.00×, Linux x86-64 1.00×; fnv: macOS arm64 1.00×, Linux aarch64 1.03× slower, Linux x86-64 1.00×; qsort: macOS arm64 1.05× faster, Linux aarch64 1.08× slower, Linux x86-64 1.11× slower; matmul: macOS arm64 1.21× slower, Linux aarch64 1.04× slower, Linux x86-64 1.04× slower; nbody: macOS arm64 1.31× slower, Linux aarch64 1.28× slower, Linux x86-64 1.27× slower; mandelbrot: macOS arm64 1.01× slower, Linux aarch64 1.00×, Linux x86-64 1.03× faster; words: macOS arm64 1.37× slower, Linux aarch64 1.30× slower, Linux x86-64 1.24× slower; numeric: macOS arm64 1.31× slower, Linux aarch64 1.18× slower, Linux x86-64 1.11× faster; jsonrt: macOS arm64 ≈1.38× slower, Linux aarch64 1.46× slower, Linux x86-64 1.25× slower; ripgrep: macOS arm64 ≈1.50× slower, Linux aarch64 1.33× slower, Linux x86-64 1.21× slower; fd: macOS arm64 ≈1.12× slower, Linux aarch64 1.31× slower, Linux x86-64 1.23× slower; tokei: macOS arm64 1.29× slower, Linux aarch64 1.27× slower, Linux x86-64 1.34× slower; eza: macOS arm64 ≈1.06× slower, Linux aarch64 1.06× slower, Linux x86-64 1.07× slower; bat: macOS arm64 ≈1.16× slower, Linux aarch64 1.16× slower, Linux x86-64 1.19× slower; bun: Linux x86-64 5.80× slower" src="docs/benchmarks/charts/runtime-light.svg" width="760"></picture>

How fast the programs run. Each program is built by `cargo build --release` and by ra-jit with LLVM at `-O3`, every crate of it compiled by ra-jit, with the release profile; each benchmark is then run several times, the builds taking turns, median. A run is published only when every program did what the release build's did. Each platform is a machine of its own, and each number is ra-jit's time ÷ release's on it: 1x is as fast.

| Benchmark | macOS arm64 | Linux aarch64 | Linux x86-64 |
|---|---:|---:|---:|
| sieve | 1.12x | 1.06x | 1.02x |
| collatz | 1.01x | 1.00x | 1.00x |
| fnv | 1.00x | 1.03x | 1.00x |
| qsort | 0.95x | 1.08x | 1.11x |
| matmul | 1.21x | 1.04x | 1.04x |
| nbody | 1.31x | 1.28x | 1.27x |
| mandelbrot | 1.01x | 1.00x | 0.97x |
| words | 1.37x | 1.30x | 1.24x |
| numeric | 1.31x | 1.18x | 0.90x |
| jsonrt | ≈1.38x | 1.46x | 1.25x |
| ripgrep | ≈1.50x | 1.33x | 1.21x |
| fd | ≈1.12x | 1.31x | 1.23x |
| tokei | 1.29x | 1.27x | 1.34x |
| eza | ≈1.06x | 1.06x | 1.07x |
| bat | ≈1.16x | 1.16x | 1.19x |
| bun | - | - | 5.80x |
| **All benchmarks (geometric mean)** | **1.18x** | **1.16x** | **1.24x** |

≈: that benchmark's runs varied by more than 1.5× from fastest to slowest on that machine; read it as rough.

- **macOS arm64**: GitHub-hosted macos-15 runner (a shared VM) (Apple M1 (Virtual), 3 cores, 7 GiB), ra-jit [`9c31e64d83`](https://github.com/indexable-inc/ra-jit/commit/9c31e64d83cd4cc2d0bbbd2e56b7e48d8a9326c6) on 2026-10-23, each benchmark run 5 times, load average 12 at the start, [by CI](https://github.com/indexable-inc/ra-jit/actions/runs/39876666206).
- **Linux aarch64**: AWS c8g.8xlarge (Graviton4), pinned to 16 cores with taskset (Neoverse-V2, 16 of its 32 cores, 62 GiB), ra-jit [`9c31e64d83`](https://github.com/indexable-inc/ra-jit/commit/9c31e64d83cd4cc2d0bbbd2e56b7e48d8a9326c6) on 2026-10-23, each benchmark run 10 times, load average 12 at the start, by hand.
- **Linux x86-64**: AWS c8a.metal-24xl (dev-aws-11), pinned to 16 cores with taskset (AMD EPYC 9R45, 16 of its 96 cores, 188 GiB), ra-jit [`9c31e64d83`](https://github.com/indexable-inc/ra-jit/commit/9c31e64d83cd4cc2d0bbbd2e56b7e48d8a9326c6) on 2026-10-23, each benchmark run 10 times, load average 12 at the start, by hand.
- **bun** is measured on its own, by the `bench-bun` workflow: Linux x86-64 on AWS c8a.24xlarge (dev-aws-eph-rajit-bun), pinned to 16 cores with taskset (AMD EPYC 9R45, 16 of its 96 cores, 185 GiB), ra-jit [`7dcfe73922`](https://github.com/indexable-inc/ra-jit/commit/7dcfe739224e0f1b5a65c2bc744375bb0ef33714) on 2026-10-22, each benchmark run 5 times, by hand.

<details><summary>Times, memory and builds, by platform</summary>

**macOS arm64**

| Benchmark | `cargo build --release` | ra-jit, LLVM | ra-jit ÷ release | release with dev checks ÷ release | Peak memory, release → ra-jit |
|---|---:|---:|---:|---:|---:|
| sieve | 45 ms | **50 ms** | 1.12x | 1.01x | 17 MB → 17 MB |
| collatz | 150 ms | **151 ms** | 1.01x | 1.49x | 1 MB → 2 MB |
| fnv | 98 ms | **98 ms** | 1.00x | 0.96x | 2 MB → 2 MB |
| qsort | 263 ms | **250 ms** | 0.95x | 0.91x | 13 MB → 13 MB |
| matmul | 32 ms | **39 ms** | 1.21x | 0.99x | 4 MB → 4 MB |
| nbody | 30 ms | **39 ms** | 1.31x | 1.01x | 1 MB → 2 MB |
| mandelbrot | 328 ms | **333 ms** | 1.01x | 1.00x | 1 MB → 2 MB |
| words | 71 ms | **98 ms** | 1.37x | 1.22x | 2 MB → 2 MB |
| numeric | 95 ms | **124 ms** | 1.31x | 1.17x | 14 MB → 14 MB |
| jsonrt | 1.23 s | **1.69 s** | ≈1.38x | 1.14x | 597 MB → 590 MB |
| ripgrep | 134 ms | **201 ms** | ≈1.50x | 1.10x | 5 MB → 7 MB |
| fd | 63 ms | **71 ms** | ≈1.12x | 0.98x | 7 MB → 8 MB |
| tokei | 643 ms | **828 ms** | 1.29x | 1.09x | 26 MB → 27 MB |
| eza | 417 ms | **440 ms** | ≈1.06x | 0.92x | 19 MB → 20 MB |
| bat | 156 ms | **181 ms** | ≈1.16x | 0.89x | 12 MB → 15 MB |
| **All benchmarks (geometric mean)** | | | **1.18x** | 1.05x | |

What each program takes, release → ra-jit: its build from cold (for ra-jit, cargo running the build scripts and proc macros, ra-jit's compile of every crate and the link), its size, and its build's memory (the most the build's processes held at once, and the largest one process: rustc or ra-jit, or the linker).

| Program | Build | Size | Build, peak | Build, largest process |
|---|---:|---:|---:|---:|
| kernels | 600 ms → 6.80 s | 0.48 MB → 1.64 MB | 146 MB → 1.1 GB | 146 MB → 1.1 GB |
| numeric | 3.90 s → 37.80 s | 0.75 MB → 2.23 MB | 288 MB → 1.3 GB | 288 MB → 1.3 GB |
| jsonrt | 11.60 s → 68.60 s | 0.97 MB → 2.63 MB | 570 MB → 1.8 GB | 428 MB → 1.8 GB |
| ripgrep | 33.40 s → 227.10 s | 6.66 MB → 10.92 MB | 553 MB → 2.8 GB | 403 MB → 2.8 GB |
| fd | 49.70 s → 220.30 s | 3.0 MB → 8.77 MB | 757 MB → 2.8 GB | 757 MB → 2.8 GB |
| tokei | 53.30 s → 258.70 s | 4.22 MB → 9.63 MB | 547 MB → 3.2 GB | 429 MB → 3.2 GB |
| eza | 97.50 s → 213.50 s | 2.0 MB → 6.66 MB | 545 MB → 2.9 GB | 545 MB → 2.8 GB |
| bat | 129.70 s → 679.00 s | 6.65 MB → 16.8 MB | 1.2 GB → 4.6 GB | 1.2 GB → 4.6 GB |

**Linux aarch64**

| Benchmark | `cargo build --release` | ra-jit, LLVM | ra-jit ÷ release | release with dev checks ÷ release | Peak memory, release → ra-jit |
|---|---:|---:|---:|---:|---:|
| sieve | 44 ms | **47 ms** | 1.06x | 1.02x | 23 MB → 23 MB |
| collatz | 106 ms | **106 ms** | 1.00x | 1.92x | 23 MB → 23 MB |
| fnv | 71 ms | **73 ms** | 1.03x | 1.00x | 23 MB → 23 MB |
| qsort | 198 ms | **215 ms** | 1.08x | 1.06x | 23 MB → 23 MB |
| matmul | 28 ms | **29 ms** | 1.04x | 1.00x | 23 MB → 23 MB |
| nbody | 36 ms | **46 ms** | 1.28x | 1.00x | 23 MB → 23 MB |
| mandelbrot | 270 ms | **270 ms** | 1.00x | 1.00x | 23 MB → 23 MB |
| words | 61 ms | **79 ms** | 1.30x | 1.03x | 23 MB → 23 MB |
| numeric | 23 ms | **27 ms** | 1.18x | 1.10x | 41 MB → 42 MB |
| jsonrt | 862 ms | **1.26 s** | 1.46x | 1.05x | 546 MB → 548 MB |
| ripgrep | 57 ms | **76 ms** | 1.33x | 1.02x | 26 MB → 26 MB |
| fd | 18 ms | **24 ms** | 1.31x | 1.02x | 26 MB → 26 MB |
| tokei | 88 ms | **112 ms** | 1.27x | 1.01x | 50 MB → 53 MB |
| eza | 246 ms | **261 ms** | 1.06x | 1.07x | 46 MB → 46 MB |
| bat | 88 ms | **102 ms** | 1.16x | 1.02x | 31 MB → 31 MB |
| **All benchmarks (geometric mean)** | | | **1.16x** | 1.07x | |

What each program takes, release → ra-jit: its build from cold (for ra-jit, cargo running the build scripts and proc macros, ra-jit's compile of every crate and the link), its size, and its build's memory (the most the build's processes held at once, and the largest one process: rustc or ra-jit, or the linker).

| Program | Build | Size | Build, peak | Build, largest process |
|---|---:|---:|---:|---:|
| kernels | 500 ms → 4.10 s | 0.55 MB → 1.62 MB | 190 MB → 1.6 GB | 168 MB → 1.5 GB |
| numeric | 3.30 s → 11.00 s | 0.88 MB → 2.14 MB | 602 MB → 2.0 GB | 310 MB → 2.0 GB |
| jsonrt | 6.30 s → 18.20 s | 1.09 MB → 2.57 MB | 2.0 GB → 2.8 GB | 440 MB → 2.8 GB |
| ripgrep | 9.00 s → 48.60 s | 32.22 MB → 19.43 MB | 2.4 GB → 4.2 GB | 719 MB → 4.2 GB |
| fd | 47.10 s → 69.00 s | 3.74 MB → 7.03 MB | 2.2 GB → 4.1 GB | 789 MB → 4.1 GB |
| tokei | 13.80 s → 52.30 s | 5.43 MB → 9.32 MB | 3.4 GB → 4.8 GB | 536 MB → 4.8 GB |
| eza | 35.70 s → 39.20 s | 2.38 MB → 5.59 MB | 3.4 GB → 4.2 GB | 575 MB → 4.2 GB |
| bat | 51.20 s → 122.70 s | 6.99 MB → 13.03 MB | 3.7 GB → 6.6 GB | 1.3 GB → 6.6 GB |

**Linux x86-64**

| Benchmark | `cargo build --release` | ra-jit, LLVM | ra-jit ÷ release | release with dev checks ÷ release | Peak memory, release → ra-jit |
|---|---:|---:|---:|---:|---:|
| sieve | 30 ms | **30 ms** | 1.02x | 1.04x | 21 MB → 21 MB |
| collatz | 94 ms | **94 ms** | 1.00x | 1.86x | 21 MB → 21 MB |
| fnv | 59 ms | **59 ms** | 1.00x | 1.00x | 21 MB → 21 MB |
| qsort | 212 ms | **236 ms** | 1.11x | 1.09x | 21 MB → 21 MB |
| matmul | 23 ms | **24 ms** | 1.04x | 0.98x | 21 MB → 21 MB |
| nbody | 20 ms | **25 ms** | 1.27x | 1.00x | 21 MB → 21 MB |
| mandelbrot | 179 ms | **174 ms** | 0.97x | 1.02x | 21 MB → 21 MB |
| words | 40 ms | **49 ms** | 1.24x | 1.04x | 21 MB → 21 MB |
| numeric | 21 ms | **19 ms** | 0.90x | 1.04x | 21 MB → 21 MB |
| jsonrt | 963 ms | **1.20 s** | 1.25x | 1.05x | 542 MB → 543 MB |
| ripgrep | 33 ms | **40 ms** | 1.21x | 1.03x | 21 MB → 21 MB |
| fd | 14 ms | **18 ms** | 1.23x | 1.02x | 21 MB → 21 MB |
| tokei | 80 ms | **107 ms** | 1.34x | 0.98x | 32 MB → 36 MB |
| eza | 144 ms | **155 ms** | 1.07x | 1.04x | 28 MB → 28 MB |
| bat | 56 ms | **67 ms** | 1.19x | 1.02x | 28 MB → 28 MB |
| bun | 392 ms | **2.27 s** | 5.80x | 1.09x | 57 MB → 75 MB |
| **All benchmarks (geometric mean)** | | | **1.24x** | 1.07x | |

What each program takes, release → ra-jit: its build from cold (for ra-jit, cargo running the build scripts and proc macros, ra-jit's compile of every crate and the link), its size, and its build's memory (the most the build's processes held at once, and the largest one process: rustc or ra-jit, or the linker).

| Program | Build | Size | Build, peak | Build, largest process |
|---|---:|---:|---:|---:|
| kernels | 400 ms → 3.10 s | 0.5 MB → 1.52 MB | 179 MB → 1.5 GB | 143 MB → 1.5 GB |
| numeric | 2.50 s → 9.20 s | 0.81 MB → 2.17 MB | 470 MB → 2.0 GB | 285 MB → 1.9 GB |
| jsonrt | 4.70 s → 14.40 s | 1.07 MB → 2.73 MB | 1.6 GB → 2.8 GB | 410 MB → 2.7 GB |
| ripgrep | 6.70 s → 43.30 s | 31.39 MB → 21.09 MB | 1.9 GB → 4.2 GB | 579 MB → 4.2 GB |
| fd | 38.80 s → 60.40 s | 4.16 MB → 7.67 MB | 1.9 GB → 4.2 GB | 454 MB → 4.1 GB |
| tokei | 10.30 s → 45.70 s | 5.51 MB → 10.81 MB | 3.0 GB → 4.9 GB | 477 MB → 4.8 GB |
| eza | 28.30 s → 33.60 s | 2.68 MB → 6.04 MB | 3.0 GB → 4.2 GB | 463 MB → 4.2 GB |
| bat | 42.40 s → 104.30 s | 7.55 MB → 14.22 MB | 3.4 GB → 7.0 GB | 689 MB → 7.0 GB |
| bun | 135.20 s → 86.90 s | 314.01 MB → 363.54 MB | 6.3 GB → 28.1 GB | 3.7 GB → 28.0 GB |

Release with dev checks: `cargo build --release` with `-C overflow-checks=on -C debug-assertions=on`.

</details>

Measured by hand with `harness/bench.py --runtime` (harness/runbench.py), or by the `bench-readme` workflow (a project measured on its own, by its own workflow) on each platform's benchmark machine once it is registered. Raw JSON: [macOS arm64](docs/benchmarks/runtime-macos-arm64.json), [Linux aarch64](docs/benchmarks/runtime-linux-aarch64.json), [Linux x86-64](docs/benchmarks/runtime-linux-x86-64.json); bun: [Linux x86-64](docs/benchmarks/runtime-bun-linux-x86-64.json).
<!-- runbench:end -->

## Try it on your project

```
nix run github:indexable-inc/ra-jit#ra-jit -- build
```

No nix? Put the binaries from a [release](https://github.com/indexable-inc/ra-jit/releases)
on your PATH, or build them with `cargo +1.99.0 install --locked --profile dist ra-jit`. Then
use Rust 1.99.0 with its source:
`rustup toolchain install 1.99.0 -c rust-src && rustup override set 1.99.0`.

A release's binaries and nix build the same ra-jit: the `dist` profile (thin LTO), optimized
with the profile of its own work that [pgo/profiles.json](pgo/profiles.json) pins
([how](docs/reports/RELEASE-BUILD.md)). `cargo install` builds `dist` without that last step,
which on Linux starts about 10% slower and edits about 3% slower than a release's binaries:
cargo takes no compiler flags from a crate, and the crates it builds from crates.io are not
the ones the profile was made of, so none could be handed to it either. (`--profile dist`
comes with the first release after 0.1.0.)

### Use it as your normal `cargo build`

```
nix profile install github:indexable-inc/ra-jit#ra-jit
ra-jit enable
```

From then on plain `cargo build`, `run`, `test` and `check` in any project go through ra-jit.
`ra-jit disable` puts your cargo config back.

### Live edits

Add the crate and poll once per frame:

```toml
ra-jit-live = "0.1"
```

```rust
let mut live = ra_jit_live::Live::start().ok();
loop {
    if let Some(outcome) = live.as_mut().and_then(|it| it.poll()) {
        eprintln!("ra-jit: {outcome}");
    }
    // ...your frame
}
```

### Cached functions

`#[ra_jit::cache]` on a pure function keeps its results across runs, until an edit changes
the code it reaches ([docs/fn-cache.md](docs/fn-cache.md)).

## What to know

- Live edits work on macOS Apple silicon only. Elsewhere you still get fast builds.
- An edit with a borrow-check error is refused, but the borrow checker still misses about a
  third of rustc's errors: `cargo check` stays the final word.
- Rust 1.99.0 only.
- Changing a struct the program is using asks for a restart.

More: [how it works](docs/architecture.md), [full results](docs/results.md),
[what is supported](docs/support.md).

## Credits

ra-jit stands on the shoulders of giants. Thank you to the people who built:

- [rust-analyzer](https://github.com/rust-lang/rust-analyzer), whose analysis and MIR ra-jit compiles from
- [Cranelift](https://cranelift.dev) (Bytecode Alliance), the fast code generator
- [LLVM](https://llvm.org) and [inkwell](https://github.com/TheDan64/inkwell), for the optimised tiers
- [salsa](https://github.com/salsa-rs/salsa), the incremental engine underneath
- [object](https://github.com/gimli-rs/object), for reading and writing executables
- [Rust](https://github.com/rust-lang/rust) and its compiler crates
- [rustd](https://github.com/DioxusLabs/rustd) (DioxusLabs), whose code ra-jit includes

None of these projects endorses ra-jit.

Made at [ix](https://ix.dev) for den's native app. MIT or Apache-2.0.
