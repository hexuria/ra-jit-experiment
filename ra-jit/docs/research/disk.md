# What ra-jit keeps on disk, against cargo

2026-10-23. Measured on dev-aws-14 (c8a.48xlarge, `/home` on XFS, which shares blocks between
reflinked files), ra-jit at `origin/main` of 2026-10-22 against the branch that compresses the
store, Cranelift, `harness/bench.py` with `--keep`. Each claim is **read** (measured, or read in
the source), **reported** (someone says so) or **unverified** (our inference).

## Short version

- The store was already one per user and machine (`~/.cache/ra-jit/store`), content-addressed,
  every project and checkout reading and writing the same packs (**read**, `daemon.rs`
  `store()`). What it lacked was compression and a bound.
- Sealed packs are now zstd blocks of 128 KB: the store is about a fifth of what it was on every
  project, ripgrep 20 → 6 MB, helix 1,686 → 339 MB, nushell 219 → 64 MB, bevy 8,334 →
  1,623 MB (**read**). A warm start from the store is no slower: ripgrep 4.38 → 4.24 s, helix
  24.03 → 22.86 s, nushell 24.32 → 24.36 s, bevy 55.83 → 54.88 s, one run each (**read**).
- ra-jit is not below cargo on any of the four. Its target directory holds cargo's whole build
  as well as ra-jit's, and that cannot go before rustc stops building the crates ra-jit
  compiles (the no-rustc lane, F/G) (**read**, the breakdowns below).

## Totals

Every block counted once (FIEMAP, `harness/memory.py`), MB, after the bench's edits. cargo's
target is the debug build `bench.py` measures.

| project | ra-jit target | store before | store after | ra-jit total before → after | cargo target |
|---|---:|---:|---:|---:|---:|
| ripgrep | 532 | 20 | 6 | 552 → 538 | 407 |
| helix | 4,774 | 1,686 | 339 | 6,462 → 5,113 | 3,713 |
| nushell | 5,927 | 219 | 64 | 6,159 → 5,991 | 5,343 |
| bevy | 31,575 | 8,334 | 1,623 | 39,919 → 33,198 | 17,549 |

The target directory is the same before and after (**read**). Counted by `du` instead, as
`bench.py` did before #131, bevy's target was 43,063 MB: reflinked copies counted twice.

## Where the excess over cargo is

"Alone" is every block of the part, "frees" what removing it would free (its blocks no other
part shares). On a filesystem without reflinks (ext4, APFS copies), "alone" is what it costs.

bevy (31,583 MB; cargo's 17,548):

| part | alone | frees | cargo has |
|---|---:|---:|---:|
| `debug/.ra-jit-units/*.rlib` | 11,779 | 3,782 | none |
| `debug/incremental` | 8,467 | 8,467 | 8,466 |
| `debug/deps/*.rlib` | 5,887 | 1,672 | 5,886 |
| `target/rust-analyzer` | 4,600 | 554 | none |
| `debug/examples` (the daemon's program) | 3,485 | 3,485 | 2,016 |
| `ra-jit/programs/*/ra-jit.o` | 2,426 | 2,426 | none |
| `ra-jit/programs/*/cargo` (rustc's program, kept) | 2,016 | 2,016 | none |
| `debug/deps/*.rmeta`, `*.so`, `build`, `.fingerprint` | 1,177 | 913 | 1,176 |

nushell: the daemon's program 828 MB against cargo's 538 (debug information), rustc's program
kept beside it 538, `ra-jit.o` 471, `.ra-jit-units` 864 sharing every block with `deps`.
helix: `ra-jit.o` 570, rustc's program kept 425, the daemon's program 993 against 424 (**read**).

What each is (**read** in the source unless marked):

- **`.ra-jit-units`, `deps`, `incremental`, rustc's program kept**: cargo's own build, which
  ra-jit still runs, then the rlibs ra-jit links in place of rustc's. They go when rustc no
  longer builds what ra-jit compiles: the no-rustc lane (F/G) (**reported**, its brief). Not
  touched here, and nothing here depends on it.
- **`ra-jit.o`**: the whole program as ra-jit compiled it, the base every patch is laid out
  against and what a failed splice links in full with. Needed while edits are answered.
- **The daemon's program, larger than cargo's**: a splice copies the sections a debugger reads
  behind the code it appends and leaves the linker's where they were, for the next splice to
  undo the last (`splice/debug.rs`). On bevy the file is 5,959 MB, of which 3,231 MB are
  sections; 3,400 MB of distinct blocks on XFS, where the copies start block-aligned and share
  most of the original's. Punching the linker's copy out would break the undo
  (**unverified**).
- **`target/rust-analyzer`**: rust-analyzer's build of build scripts and proc macros, mostly
  sharing blocks with cargo's (554 MB its own on bevy).

## The store

- 64K blocks compress helix's store 4.2x at zstd 1 and 4.5x at 3; 256K blocks 5.4x and 5.9x;
  about 700-780 MB/s to compress, 1.4-1.6 GB/s to read back, one core (**read**,
  `packstat.py` on helix's 229,358 records). 128K at level 3 is what shipped: 5.0x on helix,
  5.1x on bevy.
- Its bound is a tenth of what the disk has for it (free space plus the store), at least
  256 MiB and at most 32 GiB, recomputed at each collection; `RA_JIT_CACHE_MAX` sets bytes or
  `off`, `ra-jit cache gc` collects, `ra-jit status` says the size and the bound
  ([cache.md](../cache.md)) (**read**).
- `bench.py` says of each project how much of its store no project before it in the run had
  stored (`store_added_mb`), what one store per machine costs that project: bevy 1,598 of
  1,622 MB, helix 331 of 339 (**read**).

## Left

- Below cargo needs F/G. Of bevy's 14 GB over cargo, `.ra-jit-units` (3.8 GB its own) and
  rustc's program kept (2.0 GB) are F/G's; `ra-jit.o` (2.4 GB), the program's second copy of
  its debug sections (1.5 GB) and rust-analyzer's builds (0.6 GB) are not.
- The daemon's program: debug sections written once for both the linker and the splice.
- Packs sent to a remote are compressed again (`.pack.zst` of zstd blocks): a few percent,
  for nothing (**unverified**).
