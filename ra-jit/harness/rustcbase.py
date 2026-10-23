#!/usr/bin/env python3
"""rustcbase.py: the first edits after a cold `cargo ra-jit build`, which the daemon splices into
the executable rustc linked (issue #11), against plain cargo.

    rustcbase.py [--work DIR] [SCENARIO...]

Writes a workspace of two crates (a program and a library it calls) into WORK/ws. Each scenario
starts from an empty target directory, RA_JIT_HOME and store: `cargo ra-jit build`, then once
the daemon is ready its edits one after the other, each built by `cargo ra-jit build` and by
`cargo build` into a target directory of its own. Both programs run without arguments and with
`panic`; stdout, stderr and the exit status must be the same. Each edit also has the way it was
built checked: `patch` when it was spliced into the executable it had (rustc's, or the daemon's
own once it built one), `full` when the daemon built the program in full first. A scenario's
`wait` is how long it waits once ready, which lets the daemon build its own base meanwhile.
The `cached-` scenarios have a program with a `#[ra_jit::cache]` function instead, whose
results are kept in WORK/fn-cache.

Exits 1 when an output differs or an edit was built another way than the scenario says.
"""
import os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLCHAIN = open(os.path.join(HERE, "toolchain")).read().strip()

MAIN = """\
use std::fmt::Display;
use std::sync::atomic::{AtomicU32, Ordering};

// Above what the edits move: a patch of rustc's executable can't have code that uses a
// thread-local (the keys of `RandomState`).
fn seen(args: &[String]) -> Option<u32> {
    let mut seen = std::collections::HashMap::new();
    for arg in args {
        *seen.entry(arg.clone()).or_insert(0u32) += 1;
    }
    seen.get("panic").copied()
}

trait Greet {
    fn name(&self) -> String;
    fn greet(&self) -> String {
        format!("hello {}", self.name())
    }
}

struct World;

impl Greet for World {
    fn name(&self) -> String {
        "world".to_owned()
    }
}

fn twice<T: Display>(value: T) -> String {
    format!("{value}{value} at {}", line!())
}

#[inline]
fn small(x: u64) -> u64 {
    x + 1
}

fn counted() -> u32 {
    static CALLS: AtomicU32 = AtomicU32::new(0);
    CALLS.fetch_add(1, Ordering::Relaxed) + 1
}

fn shown() -> impl Display {
    "shown"
}

async fn later(x: u64) -> u64 {
    x * 2
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(it) = future.as_mut().poll(&mut cx) {
            return it;
        }
    }
}

fn work(x: u64) -> u64 {
    x * 3
}

// What it derives is never called: rustc's executable has no copy of it.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
enum Side {
    Left,
    Right,
}

fn mapped(xs: &[u64]) -> Vec<u64> {
    xs.iter().map(|x| x + 10).collect()
}

fn fails(code: u64) {
    if code > 0 {
        panic!("failed with {code}");
    }
}

const ASSET: &str = include_str!("asset.txt");

fn main() {
    // Has rustc's executable take memset from libc, as any real program's does: a patch can
    // only call what the executable it is spliced into imports. Zeros would be bzero.
    let filled = [7u8; 4096];
    std::hint::black_box(&filled);
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("seen {:?}", seen(&args));
    println!("work {}", work(7));
    println!("{}", twice(3));
    println!("small {}", small(4));
    println!("{}", World.greet());
    println!("counted {} {}", counted(), counted());
    println!("{}", shown());
    println!("later {}", block_on(later(5)));
    println!("mapped {:?}", mapped(&[1, 2]));
    println!("dep {}", dep::value(2));
    println!("asset {}", ASSET.trim());
    if args.first().is_some_and(|it| it == "panic") {
        fails(1);
    }
}
"""

FILES = {
    "Cargo.toml": '[workspace]\nmembers = ["app", "dep"]\nresolver = "3"\n',
    "app/Cargo.toml": '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n\n'
                      '[dependencies]\ndep = { path = "../dep" }\n',
    "app/src/main.rs": MAIN,
    "app/src/asset.txt": "one\n",
    "dep/Cargo.toml": '[package]\nname = "dep"\nversion = "0.1.0"\nedition = "2024"\n',
    "dep/src/lib.rs": "pub fn value(x: u64) -> u64 {\n    x + 100\n}\n\n"
                      "#[inline]\npub fn spare_inline(x: u64) -> u64 {\n    x + 7\n}\n\n"
                      "#[allow(dead_code)]\nfn never_called(x: u64) -> u64 {\n    x + 8\n}\n",
}

M = "app/src/main.rs"
D = "dep/src/lib.rs"

# A program with a `#[ra_jit::cache]` function (docs/fn-cache.md): rustc's copy of it caches
# nothing, ours does, and a patch of what it reaches must not find what the code before kept.
CACHED_MAIN = """\
#[inline(never)]
fn mix(x: u64) -> u64 {
    x.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(17)
}

#[ra_jit::cache]
fn slow(rounds: u64) -> u64 {
    let mut acc: u64 = 1;
    for i in 0..rounds {
        acc ^= mix(acc.wrapping_add(i));
    }
    acc + 1
}

#[inline(never)]
fn unrelated() -> u64 {
    7
}

fn main() {
    let filled = [7u8; 4096];
    std::hint::black_box(&filled);
    println!("slow {}", slow(1000 + std::env::args().count() as u64));
    println!("unrelated {}", unrelated());
    println!("dep {}", dep::value(2));
}
"""
CACHED_FILES = {
    **FILES,
    "app/Cargo.toml": FILES["app/Cargo.toml"]
    + f'ra-jit-cache = {{ path = "{os.path.dirname(HERE)}/crates/ra-jit-cache" }}\n',
    "app/src/main.rs": CACHED_MAIN,
}


def edit(file, old, new):
    return (file, old, new)


# name: (edits, each a list of replacements, with how each is built; seconds to wait once ready; env)
SCENARIOS = {
    "body": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 0, {}),
    "second": ([[edit(M, "    x * 3\n", "    x * 4\n")],
                [edit(M, "    x * 4\n", "    x * 5\n"), edit(M, "x + 10", "x + 20")]], ["patch", "patch"], 0, {}),
    # Moves every function below: on macOS their frames are more than this small executable's
    # unwind table, so both tables move into the header's room, and the panic unwinds through
    # the patch's frames there.
    "moved-panic": ([[edit(M, "    x * 3\n", "    let y = x * 3;\n    y + 1\n")]], ["patch"], 0, {}),
    # Moves `Side`'s derives, which rustc has no copy of, a line down and changes nothing else.
    "moved-uncopied": ([[edit(M, "    x * 3\n", "    let y = x * 3;\n    y\n")]], ["patch"], 0, {}),
    "closure": ([[edit(M, "x + 10", "x + 11")]], ["patch"], 0, {}),
    # A new function of a crate nothing depends on, whose files did not name it: nothing but
    # what calls it sees it, and what rustc compiled of every file of the crate is taken in
    # before the edit. The patch compiles it under a name of its own.
    "new-fn": ([[edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")]],
               ["patch"], 0, {}),
    "new-fn-then-patch": ([[edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")],
                          [edit(M, "    x + 2\n}\n", "    x + 5\n}\n")]], ["patch", "patch"], 0, {}),
    # Taken away again, as the benchmark's edits are, and added again: no edit links in full.
    "new-fn-removed": ([[edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")],
                        [edit(M, "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n", "    x * 3\n}\n")],
                        [edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")]],
                       ["patch", "patch", "patch"], 0, {}),
    "own-base-new-fn-removed": ([[edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")],
                                 [edit(M, "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n", "    x * 3\n}\n")]],
                                ["patch", "patch"], 8, {}),
    # A function rustc compiled, taken away with its one call, and written back.
    "fn-removed": ([[edit(M, "fn work(x: u64) -> u64 {\n    x * 3\n}\n\n", ""),
                     edit(M, '    println!("work {}", work(7));\n', "")],
                    [edit(M, "fn mapped(", "fn work(x: u64) -> u64 {\n    x * 3\n}\n\nfn mapped("),
                     edit(M, '    println!("{}", twice(3));\n', '    println!("work {}", work(7));\n    println!("{}", twice(3));\n')]],
                   ["patch", "patch"], 0, {}),
    # rustc's copies of what sees the new function may be in the crates that depend on it.
    "new-fn-dep": ([[edit(D, "    x + 100\n}\n", "    extra(x) + 100\n}\n\nfn extra(x: u64) -> u64 {\n    x + 1\n}\n")]],
                   ["full"], 0, {}),
    # The crate names `value` already, which can mean the new function now.
    "new-fn-named": ([[edit(M, "    x * 3\n}\n", "    value(x) * 3\n}\n\nfn value(x: u64) -> u64 {\n    x + 2\n}\n")]],
                     ["full"], 0, {}),
    "dep": ([[edit("dep/src/lib.rs", "x + 100", "x + 200")]], ["patch"], 0, {}),
    "counted-body": ([[edit(M, "Ordering::Relaxed) + 1", "Ordering::Relaxed) + 2")]], ["patch"], 0, {}),
    "unused-std": ([[edit(M, "    x * 3\n", "    let _ = std::os::unix::process::parent_id();\n    x * 3\n")]],
                   [None], 0, {}),
    # rustc's one copy, `twice::<i32>`, is an instance of the base that the patch replaces.
    "generic": ([[edit(M, "{value}{value} at", "{value}-{value} at")]], ["patch"], 0, {}),
    "generic-moved": ([[edit(M, '        "world".to_owned()\n', '        let it = "world";\n        it.to_owned()\n')]],
                      ["full"], 0, {}),
    "inline": ([[edit(M, "    x + 1\n}\n\nfn counted", "    x + 2\n}\n\nfn counted")]], ["full"], 0, {}),
    # Neither has a copy in rustc's executable, nothing there calls them: a patch has the one
    # copy, which the second edit calls.
    "inline-uncopied": ([[edit(D, "    x + 7\n", "    x + 70\n")],
                         [edit(D, "    x + 100\n", "    spare_inline(x) + 100\n")]], ["patch", "patch"], 0, {}),
    "uncompiled": ([[edit(D, "    x + 8\n", "    x + 80\n")],
                    [edit(D, "    x + 100\n", "    never_called(x) + 100\n")]], ["patch", "patch"], 0, {}),
    "trait-default": ([[edit(M, '"hello {}"', '"hi {}"')]], ["full"], 0, {}),
    "counted-static": ([[edit(M, "AtomicU32::new(0);\n    CALLS", "AtomicU32::new(5);\n    CALLS")]], ["full"], 0, {}),
    "async": ([[edit(M, "    x * 2\n", "    x * 6\n")]], ["full"], 0, {}),
    "rpit": ([[edit(M, '    "shown"\n', '    "shown!"\n')]], ["full"], 0, {}),
    "asset": ([[edit("app/src/asset.txt", "one", "two")]], ["full"], 0, {}),
    "full-then-patch": ([[edit(M, "    x + 1\n}\n\nfn counted", "    x + 2\n}\n\nfn counted")],
                         [edit(M, "    x * 3\n", "    x * 4\n")]], ["full", "patch"], 0, {}),
    "own-base": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 8, {}),
    # A patch of rustc's executable when the edit interrupts the daemon's own base, and of that
    # base when it was linking as the edit came: the edit waits for the link, whose executable
    # is then the program. The waits land the edit at points of the base's build.
    "interrupt": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 1.3, {}),
    **{f"interrupt-{ms}": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], ms / 1000, {})
       for ms in (1100, 1200, 1400, 1500, 1700)},
    "off": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 0, {"RA_JIT_RUSTC_BASE": "0"}),
}
# Of CACHED_FILES. The attribute hands the body to a macro: an edit of it is more than bodies.
CACHED_SCENARIOS = {
    "cached-then-reached": ([[edit(M, "    acc + 1\n", "    acc + 2\n")],
                             [edit(M, "rotate_left(17)", "rotate_left(19)")],
                             [edit(M, "rotate_left(19)", "rotate_left(17)")]], ["full", "patch", "patch"], 0, {}),
    # Rustc's copy of the cached function calls the patched one. On macOS the patch's frames
    # are more than this small executable's unwind table.
    "cached-reached": ([[edit(M, "    7\n", "    8\n")],
                        [edit(M, "rotate_left(17)", "rotate_left(19)")]], ["patch", "patch"], 0, {}),
}
SCENARIOS.update(CACHED_SCENARIOS)

# A program whose generic functions sit below what the edits change, as ripgrep's `print_stats`
# sits below `main`: rustc copied each into its callers with the lines it was at.
GENERIC_MAIN = """\
use std::fmt::Write as _;

fn work(x: u64) -> u64 {
    x * 3
}

fn main() {
    let filled = [7u8; 4096];
    std::hint::black_box(&filled);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    show(&mut out, work(7));
    show(&mut out, "text");
    out.push_str(&dep::wrap(5));
    print!("{out}");
    if args.first().is_some_and(|it| it == "panic") {
        boom::<u8>(1);
    }
}

fn show<W: std::fmt::Write, T: std::fmt::Display>(out: &mut W, value: T) {
    writeln!(out, "{value} at {}", line!()).unwrap();
}

fn boom<T: Default + std::fmt::Debug>(code: u64) {
    if code > 0 {
        panic!("boom {:?} {code}", T::default());
    }
}
"""
GENERIC_FILES = {
    **FILES,
    "app/src/main.rs": GENERIC_MAIN,
    "dep/src/lib.rs": "pub fn value(x: u64) -> u64 {\n    x + 100\n}\n\n"
                      "pub fn wrap<T: std::fmt::Display>(value: T) -> String {\n"
                      "    format!(\"wrapped {value} at {}\\n\", line!())\n}\n",
}
GENERIC_SCENARIOS = {
    "generic-below": ([[edit(M, "    x * 3\n", "    let y = x * 3;\n    y\n")]], ["patch"], 0, {}),
    "generic-below-body": ([[edit(M, '"{value} at {}"', '"{value} is at {}"')]], ["patch"], 0, {}),
    # Copied into a crate that calls it from another file, which nothing homed of `dep` knows.
    "generic-elsewhere": ([[edit("dep/src/lib.rs", "    x + 100\n", "    let y = x + 100;\n    y\n")]],
                          ["full"], 0, {}),
}
SCENARIOS.update(GENERIC_SCENARIOS)

# An optional dependency only a dev-dependency's feature turns on, which cargo does not build for
# the program and the daemon loads, as `cargo metadata` has the features of every target (zed's
# `livekit_api`, through `livekit_client/test-support`). Its build script fails, as one needing
# a tool that is not there does: what it includes from `OUT_DIR` is never there, and the
# daemon's own base is to be built anyway.
UNBUILT_FILES = {
    **FILES,
    "Cargo.toml": '[workspace]\nmembers = ["app", "dep"]\ndefault-members = ["app"]\nresolver = "3"\n',
    "app/Cargo.toml": FILES["app/Cargo.toml"] + '\n[dev-dependencies]\ndep = { path = "../dep", features = ["extra"] }\n',
    "dep/Cargo.toml": FILES["dep/Cargo.toml"]
    + '\n[features]\nextra = ["dep:unbuilt"]\n\n[dependencies]\nunbuilt = { path = "../unbuilt", optional = true }\n',
    "unbuilt/Cargo.toml": '[package]\nname = "unbuilt"\nversion = "0.1.0"\nedition = "2024"\n',
    "unbuilt/build.rs": 'fn main() {\n    panic!("protoc is not there");\n}\n',
    "unbuilt/src/lib.rs": 'include!(concat!(env!("OUT_DIR"), "/made.rs"));\n\n'
                          'pub static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blob.bin"));\n',
}
UNBUILT_SCENARIOS = {
    "unbuilt-member": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 8, {}),
}
SCENARIOS.update(UNBUILT_SCENARIOS)
# The own base holds more than the memory it may, so it is given up once and not started again,
# and the edit patches what rustc linked.
SCENARIOS["own-base-budget"] = ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 8, {"RA_JIT_OWN_BASE_MEMORY": "1"})

# A program whose package has a library too, as each example of bevy's package has bevy: the
# library is a crate of the program's files too, and the program depends on it.
LIB_FILES = {
    **FILES,
    "app/Cargo.toml": FILES["app/Cargo.toml"] + '\n[lib]\nname = "applib"\n',
    "app/src/main.rs": MAIN.replace('println!("dep {}", dep::value(2));', 'println!("dep {}", applib::base(dep::value(2)));'),
    "app/src/lib.rs": "pub fn base(x: u64) -> u64 {\n    x + 1000\n}\n",
}
LIB_SCENARIOS = {
    "new-fn-beside-lib": ([[edit(M, "    x * 3\n}\n", "    helper(x) * 3\n}\n\nfn helper(x: u64) -> u64 {\n    x + 2\n}\n")]],
                          ["patch"], 0, {}),
}
SCENARIOS.update(LIB_SCENARIOS)
# The library of the package has the program's name, as most have: rustc's symbols of the two
# tell them apart by their crate ids alone.
SAME_NAME_FILES = {
    **FILES,
    "app/src/main.rs": MAIN.replace('println!("dep {}", dep::value(2));', 'println!("dep {}", app::base(dep::value(2)));'),
    "app/src/lib.rs": LIB_FILES["app/src/lib.rs"],
}
SAME_NAME_SCENARIOS = {
    "body-beside-its-library": ([[edit(M, "    x * 3\n", "    x * 4\n")]], ["patch"], 0, {}),
}
SCENARIOS.update(SAME_NAME_SCENARIOS)

# How the daemon's own base is to go before the first edit, in the scenarios that say.
OWN_BASE = {"unbuilt-member": "built", "own-base-budget": "given up"}


def opt(name, default=None):
    if name in sys.argv:
        at = sys.argv.index(name)
        value = sys.argv[at + 1]
        del sys.argv[at:at + 2]
        return value
    return default


def write_workspace(ws, files):
    shutil.rmtree(ws, ignore_errors=True)
    for path, text in files.items():
        os.makedirs(os.path.dirname(os.path.join(ws, path)), exist_ok=True)
        open(os.path.join(ws, path), "w").write(text)


def run(argv, cwd, env, timeout=900):
    return subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)


def outputs(exe, cwd):
    env = dict(os.environ, NO_COLOR="1")
    env.pop("RUST_BACKTRACE", None)
    env.pop("RA_JIT_FN_CACHE_LOG", None)
    runs = (run([exe, *args], cwd, env, 60) for args in ([], ["panic"]))
    # A panic names the thread by its id, which is another in every run.
    return [(r.returncode, r.stdout, re.sub(r"thread '(.*?)' \(\d+\)", r"thread '\1'", r.stderr)) for r in runs]


def logs(home):
    """Every line the daemon and its workers logged so far."""
    lines = []
    for root, _, names in os.walk(home):
        for name in sorted(names):
            if name in ("log", "daemon.log"):
                lines += open(os.path.join(root, name), errors="replace").read().splitlines()
    return lines


def how_built(lines):
    """`patch` when the build spliced into the executable it had, `full` when the daemon
    linked the program in full, from what it logged. The last link is the edit's: one before
    it can be of the daemon's own base, finished as the edit came."""
    links = [it for it in lines if re.search(r"link: .*spliced", it)]
    return "patch" if links and "(spliced:" in links[-1] else "full"


def main():
    work = os.path.realpath(opt("--work", "/tmp/rustcbase"))
    names = sys.argv[1:] or list(SCENARIOS)
    ws = os.path.join(work, "ws")
    reference = os.path.join(work, "t-cargo")
    base = dict(os.environ, RUSTUP_TOOLCHAIN=TOOLCHAIN, CARGO_TERM_COLOR="never", CARGO_NET_OFFLINE="true",
                RA_JIT_IDLE_SECS="0")
    for name in ("RUSTFLAGS", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET", "RUST_BACKTRACE", "RA_JIT_CACHE",
                 "CARGO_INCREMENTAL", "RA_JIT_RUSTC_BASE",
                 "RA_JIT_FN_CACHE_LOG"):
        base.pop(name, None)
    # The programs run with os.environ.
    fn_cache = os.path.join(work, "fn-cache")
    os.environ["RA_JIT_FN_CACHE_DIR"] = base["RA_JIT_FN_CACHE_DIR"] = fn_cache
    cargo_env = dict(base, CARGO_TARGET_DIR=reference)
    failed = []
    for name in names:
        edits, expected, wait, extra = SCENARIOS[name]
        files = FILES
        if name in CACHED_SCENARIOS:
            files = CACHED_FILES
        elif name in GENERIC_SCENARIOS:
            files = GENERIC_FILES
        elif name in UNBUILT_SCENARIOS:
            files = UNBUILT_FILES
        elif name in LIB_SCENARIOS:
            files = LIB_FILES
        elif name in SAME_NAME_SCENARIOS:
            files = SAME_NAME_FILES
        write_workspace(ws, files)
        home, target, store = (os.path.join(work, it) for it in ("home", "target", "store"))
        for path in (home, target, store, fn_cache):
            shutil.rmtree(path, ignore_errors=True)
        os.makedirs(home, mode=0o700)
        env = dict(base, RA_JIT_HOME=home, CARGO_TARGET_DIR=target, RA_JIT_CACHE=store, **extra)
        began = time.monotonic()
        r = run(["cargo", "ra-jit", "build"], ws, env)
        if r.returncode:
            failed.append(f"{name}: cold build failed: {r.stderr[-1500:]}")
            print(f"{name}: cold build FAILED\n{r.stderr[-1500:]}", flush=True)
            continue
        built = time.monotonic() - began
        while True:
            out = run(["cargo", "ra-jit", "status"], ws, env).stdout
            states = re.findall(rf"^    {re.escape(ws)}: (.*)$", out, re.M)
            if states and not any(s.startswith("loading") for s in states):
                break
            time.sleep(0.05)
        ready = time.monotonic() - began
        rustcs = any("the first edits patch what rustc linked" in it for it in logs(home))
        print(f"{name}: build {built:.2f}s ready {ready:.2f}s ({'rustc base' if rustcs else 'own base'})", flush=True)
        time.sleep(wait)
        if name in OWN_BASE:
            lines = logs(home)
            started = [at for at, it in enumerate(lines) if "while nobody asks" in it]
            linked = bool(started) and any(it.startswith("link: ") for it in lines[started[-1]:])
            interrupted = sum(it.startswith("interrupted:") for it in lines)
            given_up = sum("for edits to patch any more" in it for it in lines)
            print(f"  own base: started {len(started)} times, interrupted {interrupted}, given up {given_up}, "
                  f"{'linked' if linked else 'not linked'}", flush=True)
            went = {(1, True, 0, 0): "built", (1, False, 1, 1): "given up"}.get(
                (len(started), linked, interrupted, given_up), "neither")
            if went != OWN_BASE[name]:
                failed.append(f"{name}: the own base was {went}, not {OWN_BASE[name]} ({len(started)} starts, "
                              f"{interrupted} interrupted, {given_up} given up)")
        try:
            for step, (replacements, want) in enumerate(zip(edits, expected), 1):
                for file, old, new in replacements:
                    path = os.path.join(ws, file)
                    text = open(path).read()
                    assert text.count(old) == 1, f"{name}: {old!r} is not once in {file}"
                    open(path, "w").write(text.replace(old, new))
                seen = len(logs(home))
                t = time.monotonic()
                r = run(["cargo", "ra-jit", "build"], ws, env)
                took = time.monotonic() - t
                if r.returncode:
                    raise RuntimeError(f"ra-jit build {step} failed: {r.stderr[-1500:]}")
                time.sleep(0.2)
                lines = logs(home)[seen:]
                r = run(["cargo", "build"], ws, cargo_env)
                if r.returncode:
                    raise RuntimeError(f"cargo build {step} failed: {r.stderr[-1500:]}")
                ours = outputs(os.path.join(target, "debug", "app"), ws)
                theirs = outputs(os.path.join(reference, "debug", "app"), ws)
                how = how_built(lines)
                same = ours == theirs
                print(f"  edit {step}: {took:.2f}s {how} (wanted {want or 'either'}) same={same}", flush=True)
                notable = [it for it in lines if re.search(
                    r"rustc|declin|no patch|built in full|spliced|not kept|nobody asks|outdated|interrupt", it)]
                for it in notable[:12]:
                    print(f"      {it[:240]}", flush=True)
                if not same:
                    failed.append(f"{name} edit {step}: output differs")
                    print(f"    ra-jit: {ours}\n    cargo:  {theirs}", flush=True)
                if want and how != want:
                    failed.append(f"{name} edit {step}: {how}, not {want}")
                    for it in lines[-60:]:
                        print(f"      | {it[:240]}", flush=True)
        except Exception as e:
            failed.append(f"{name}: {e}")
            print(f"  FAILED: {e}", flush=True)
        finally:
            run(["cargo", "ra-jit", "stop"], ws, env)
    print("\n" + ("\n".join(failed) if failed else f"all {len(names)} scenarios as expected"))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
