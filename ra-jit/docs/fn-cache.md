# Cached functions

`#[ra_jit::cache]` on a pure function keeps what it returns, in memory for the run and on
disk for the runs after it. A run that calls it again with the same arguments gets the
result without running it, for as long as the function's code, and the code of everything
it calls, is what it was. An edit to a comment, the formatting, or a function it does not
reach keeps the results; an edit to anything it reaches drops them.
([#17](https://github.com/indexable-inc/ra-jit/issues/17))

```toml
[dependencies]
ra-jit-cache = { git = "https://github.com/indexable-inc/ra-jit" }
```

```rust
#[ra_jit::cache]
fn simulate(seed: u64, steps: u32) -> Vec<f64> {
    // ...
}
```

Under plain `cargo build` the attribute does nothing: every call runs, as it is written. Only
a program ra-jit built caches.

## What a result is kept by

The key of a call is three things:

- **The arguments**, written with serde (postcard).
- **The code.** The hash of the machine code ra-jit compiled for the function's body and of
  everything that code can reach: every function it calls, directly or through others, as the
  instance it is (`largest::<u32>` and `largest::<i64>` are two), every vtable it builds and the
  methods in it, and the value of every static it reads. It is the code without what says where
  it was written (the lines of each instruction, where variables are, where a panic happened),
  so moving lines or reformatting keeps it. Another cached function called from it counts by
  its body's code, so recursion is no problem.
- **The build**: the target, the toolchain, and the overflow checks the code was built with.

The hash is a salsa query, `cache_code`, of the database ra-jit compiles from
(`crates/hir-ty-codegen/fncache.rs`), so a daemon works it out once per edit, and two daemons
work out the same one.

Calls through a `dyn` reach only the vtables that code builds, and those are hashed with the
rest. A `dyn` can't be an argument.

## What can't be cached

A function is refused when what it returns could depend on more than its arguments. ra-jit
says why when the program first calls it, with where the reason is written:

```
ra-jit: `app::counted` is not cached: it reads the static `app::CALLS`, which can change, at src/main.rs:37:5
ra-jit: `app::noisy` is not cached: it reaches `std::io::stdio::_print` → …, which calls the foreign function `write`, at src/main.rs:43:5
```

It is refused when anything it reaches:

- reads or writes a static that can change (`static mut`, an atomic, a `Mutex`), a thread-local
  or a foreign static;
- calls a foreign function other than the allocator, `memcpy` and its kin and libm, which is
  how I/O, time, randomness, the environment and threads are seen: the standard library gets
  them from the system;
- in a crate of the workspace, dereferences a raw pointer, takes an address as a number or has
  assembly in it;
- turns an address into data, which another run has another of: formats a pointer (`{:p}`),
  hashes or orders pointers, or asks for an address (`addr`, `expose_provenance`). Comparing
  two pointers for equality is fine;
- can't be compiled by ra-jit.

Or when an argument can change behind a shared reference (it holds an `UnsafeCell`, as `Cell`,
`RefCell`, `Mutex`, `Rc`, `Arc` and atomics do), holds a raw pointer of a type of the
workspace, a union of one, or a trait object.

A refused function runs every call, as under cargo.

This is conservative. `HashMap::new` reads random keys, so a function that makes a `HashMap` is
refused (its iteration order is random anyway); `BTreeMap` is fine.

## What can be a value

Arguments are `Serialize`, the result `Serialize + DeserializeOwned`: numbers, strings,
`Vec`s, tuples, `Option`, `Result` and your own types with `#[derive(Serialize, Deserialize)]`.
The attribute takes plain `name: Type` parameters; not `self` (take `this: &Self`), patterns,
`impl Trait`, or an `async`, `const` or `unsafe` function.

## Where results are kept

In memory, up to 256 MiB for the run. On disk under `target/ra-jit/fn-cache` of the target
directory the program was built in, or `RA_JIT_FN_CACHE_DIR`, up to 512 MiB
(`RA_JIT_FN_CACHE_SIZE`), the entries used least recently going first. Each entry is written to
a file of its own and renamed into place, so a run that stops partway leaves a whole entry or
none, and each holds its key, so a damaged one is never taken for another.

| Variable | |
|---|---|
| `RA_JIT_FN_CACHE=off` | caches nothing |
| `RA_JIT_FN_CACHE=memory` | keeps results for the run alone |
| `RA_JIT_FN_CACHE_DIR` | where results are kept on disk |
| `RA_JIT_FN_CACHE_SIZE` | how much, `512M` by default |
| `RA_JIT_FN_CACHE_LOG=1` | says for each call whether the result was found or computed |
| `RA_JIT_FN_CACHE_TRACE=1`, set for the daemon | prints each function the code hash goes through |

A function's results on disk are kept for the code it has and the two codes before it: the
first call of a run with code the function did not have drops what any older code computed,
at once rather than when the size is reached.

`cargo ra-jit cache` lists what each function holds, for its code and for the older ones;
`cargo ra-jit cache clear --stale` forgets all but what each function's code computed, and
`cargo ra-jit cache clear` everything.

## Live edits

When a live edit changes the code a cached function reaches, the function is in the patch with
its new hash: the next call computes again, and what the old code computed is dropped from
memory. Results on disk are kept under the old hash, with those of the code before it, and are
found again if the edit is taken back.

## How it is built

- `crates/ra-jit-cache-macros`: the attribute. It keeps the body in a closure and has the
  function ask `ra_jit::__private::__ra_jit_cache_code(&closure)` for the code of it, then look
  the call up.
- `crates/ra-jit-cache` (the `ra_jit` library): the store and that function, whose body
  answers "unknown", so under rustc nothing is cached.
- `crates/hir-ty-codegen/fncache.rs`: ra-jit compiles a call of `__ra_jit_cache_code` into the
  hash (or why the function can't be cached) as a constant. The function asking is compiled
  again after any edit, since its constant can change with code in any file
  (`patch.rs`, `cached_roots`). Only crates with `ra_jit` as a dependency are looked through
  for such functions, and nothing is hashed or compiled for the hash unless one is called, so
  a workspace without the attribute builds and patches as it did before.

`harness/fncache.py` checks it end to end, and `fncache.py --live` the live edit on macOS.
