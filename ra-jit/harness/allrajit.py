#!/usr/bin/env python3
"""allrajit.py: programs with nothing rustc built in them (docs/no-rustc.md, steps F and G).

Four programs, a hello, one that uses collections, `catch_unwind`, threads, channels,
formatting, a file and an uncaught panic, one with a `#[global_allocator]` of its own, and
one built with `panic = "abort"`, are built by ra-jit with every crate its own, the standard
library included, and linked by ra-jit (`--exe`): the native libraries are the ones the
crates' `#[link]`s ask for, and nothing it could not compile may be left in the executable.
Each is built three ways:

  - whole: the program and the standard library in one build, one object;
  - over the unit: the standard library built once, alone, into an object of its own
    (`std.o`), and each program built over it with `--prebuilt`;
  - stored: each program built alone on a store that starts empty and a remote (a directory)
    that does too. The first build compiles the standard library as a unit, keeps it in the
    store and pushes it; the others take it from the store. Last, a build on a second store
    that starts empty takes it from the remote.

What each prints, and how it exits, has to be what cargo's build of the same source does,
thread ids aside.

    python3 allrajit.py            # every way
    python3 allrajit.py unit stored
"""
import os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from difftest import RA_JIT
from storelib import scratch

# The crates of the standard library a program on Linux links, by the names cargo gives them,
# but the panic runtime.
STD = ["std", "core", "alloc", "compiler_builtins", "unwind", "libc", "cfg_if", "hashbrown",
       "rustc_demangle", "std_detect", "addr2line", "gimli", "object", "miniz_oxide", "adler2",
       "memchr"]


def std_of(panic):
    return STD + ["panic_abort" if panic == "abort" else "panic_unwind"]


HELLO = """fn main() {
    let v: Vec<u32> = (1..=10).collect();
    println!("hello {} {:?}", v.iter().sum::<u32>(), std::env::args().count());
}
"""
RICH = r"""use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Debug, Clone, PartialEq)]
enum Shape { Circle(f64), Rect { w: f64, h: f64 } }

trait Area { fn area(&self) -> f64; }
impl Area for Shape {
    fn area(&self) -> f64 {
        match self { Shape::Circle(r) => std::f64::consts::PI * r * r, Shape::Rect { w, h } => w * h }
    }
}

fn main() {
    let mut words = HashMap::new();
    for w in "the quick brown fox jumps over the lazy dog the end".split(' ') {
        *words.entry(w).or_insert(0usize) += 1;
    }
    let sorted: BTreeMap<_, _> = words.iter().collect();
    println!("words {:?}", sorted);
    let shapes: Vec<Box<dyn Area>> = vec![Box::new(Shape::Circle(1.5)), Box::new(Shape::Rect { w: 2.0, h: 3.25 })];
    let total: f64 = shapes.iter().map(|s| s.area()).sum();
    println!("area {total:.6} {:e} {}", total, 1.0f64 / 3.0);
    let caught = std::panic::catch_unwind(|| {
        let v: Vec<i32> = Vec::new();
        v[3]
    });
    println!("caught {}", caught.is_err());
    let r = std::panic::catch_unwind(|| -> i32 { panic!("boom {}", 7) });
    let payload = r.unwrap_err();
    let msg = payload.downcast_ref::<String>().cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    println!("msg {msg}");
    let counter = Arc::new(Mutex::new(0u64));
    let (tx, rx) = mpsc::channel();
    let handles: Vec<_> = (0..4u64).map(|i| {
        let counter = Arc::clone(&counter);
        let tx = tx.clone();
        std::thread::spawn(move || {
            for j in 0..1000 { *counter.lock().unwrap() += i * j; }
            tx.send(i).unwrap();
        })
    }).collect();
    drop(tx);
    for h in handles { h.join().unwrap(); }
    let mut got: Vec<u64> = rx.iter().collect();
    got.sort();
    println!("threads {} {:?}", *counter.lock().unwrap(), got);
    let mut s = String::new();
    write!(s, "{:>8}|{:<6}|{:^7}|{:#x}|{:08.3}", "right", "left", "mid", 255, -3.14159).unwrap();
    println!("{s}");
    let text = std::fs::read_to_string("Cargo.toml").unwrap();
    println!("lines {}", text.lines().count());
    println!("parsed {:?} {:?}", "42".parse::<i64>(), "x".parse::<u8>().is_err());
    std::io::stdout().flush().unwrap();
    if std::env::args().count() > 1 { let none: Option<u8> = None; none.expect("asked to fail"); }
}
"""
ALLOC = r"""use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}

#[global_allocator]
static A: Counting = Counting;

fn main() {
    let before = ALLOCS.load(Ordering::Relaxed);
    let v: Vec<Box<u64>> = (0..100).map(Box::new).collect();
    let s: String = v.iter().map(|b| b.to_string()).collect();
    println!("{} {} counted {}", s.len(), v.len(), ALLOCS.load(Ordering::Relaxed) - before >= 101);
}
"""
ABORT = r"""fn main() {
    let caught = std::panic::catch_unwind(|| 1);
    println!("caught {:?}", caught.ok());
    if std::env::args().count() > 1 { panic!("aborting {}", 3); }
}
"""
# (program, its source, the runs to compare: the arguments of each, its panic strategy)
PROGRAMS = [("hello", HELLO, [["a", "b"]], "unwind"), ("rich", RICH, [[], ["fail"]], "unwind"),
            ("counting", ALLOC, [[]], "unwind"), ("abort", ABORT, [[], ["fail"]], "abort")]


def run(cmd, cwd, what, env=None):
    start = time.monotonic()
    ran = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True,
                         env={**os.environ, "RUST_BACKTRACE": "0", **(env or {})})
    took = time.monotonic() - start
    if ran.returncode != 0:
        raise SystemExit(f"FAIL: {what} exited {ran.returncode}:\n{ran.stdout}{ran.stderr}")
    return ran, took


def outcome(binary, args, cwd):
    ran = subprocess.run([binary, *args], cwd=cwd, capture_output=True, text=True,
                         env={**os.environ, "RUST_BACKTRACE": "0"})
    unthreaded = lambda text: re.sub(r"thread '([^']*)' \(\d+\)", r"thread '\1' (N)", text)
    return ran.returncode, unthreaded(ran.stdout), unthreaded(ran.stderr)


def package(work, name, source, panic):
    root = os.path.join(work, name)
    os.makedirs(os.path.join(root, "src"))
    with open(os.path.join(root, "Cargo.toml"), "w") as f:
        f.write(f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n[workspace]\n')
        if panic == "abort":
            f.write('[profile.dev]\npanic = "abort"\n')
    with open(os.path.join(root, "src/main.rs"), "w") as f:
        f.write(source)
    subprocess.run(["cargo", "generate-lockfile", "-q"], cwd=root, check=True)
    return root


def ra_jit(root, crates, out, *extra, env=None):
    args = sum((["--crate", c] for c in crates), [])
    return run([RA_JIT, root, *args, "--out", out, *extra], root, f"ra-jit {' '.join(crates[:1])}", env)


def linked(ran, what):
    """How long ra-jit's link took, failing when it left something it could not compile."""
    said = re.search(r"^linked .* in (\S+), (\d+) functions in it ra-jit could not compile$",
                     ran.stderr, re.M)
    if not said:
        raise SystemExit(f"FAIL: {what}: ra-jit did not say it linked:\n{ran.stderr}")
    if said.group(2) != "0":
        raise SystemExit(f"FAIL: {what}: the executable can reach {said.group(2)} functions ra-jit "
                         f"could not compile:\n{ran.stderr}")
    return said.group(1)


def main():
    if not sys.platform.startswith("linux"):
        print("not run: the programs ra-jit links with nothing of rustc's are Linux's so far")
        return
    ways = [way for way in ["whole", "unit", "stored"] if way in sys.argv[1:] or len(sys.argv) == 1]
    work = scratch("ra-jit-allrajit-")
    passed = False
    try:
        roots = {name: package(work, name, source, panic) for name, source, _, panic in PROGRAMS}
        expected = {}
        for name, _, runs, _ in PROGRAMS:
            root = roots[name]
            target = os.path.join(root, "target")
            run(["cargo", "build", "-q", "--target-dir", target], root, f"cargo build of {name}")
            binary = os.path.join(root, "target/debug", name)
            expected[name] = [outcome(binary, args, root) for args in runs]
        lines = []
        units = {}
        if "unit" in ways:
            for panic, program in [("unwind", "hello"), ("abort", "abort")]:
                units[panic] = os.path.join(work, f"std-{panic}.o")
                _, took = ra_jit(roots[program], std_of(panic), units[panic])
                lines.append(f"the standard library alone, panic = \"{panic}\": {took:.2f} s, "
                             f"{os.path.getsize(units[panic]) >> 20} MB")
        remote = {"RA_JIT_REMOTE": os.path.join(work, "remote"), "RA_JIT_PROJECT": "allrajit",
                  "RA_JIT_REMOTE_UNSIGNED": "1"}
        os.makedirs(remote["RA_JIT_REMOTE"])
        stores = [{"RA_JIT_CACHE": os.path.join(work, "store"), **remote},
                  {"RA_JIT_CACHE": os.path.join(work, "store2"), **remote}]
        for way in ways:
            # The first program again once the store has the library, and the second on
            # another store.
            programs = PROGRAMS + PROGRAMS[:1] + PROGRAMS[1:2] if way == "stored" else PROGRAMS
            # The panic strategies whose library some store or the remote has.
            compiled = set()
            for at, (name, _, runs, panic) in enumerate(programs):
                root = roots[name]
                obj = os.path.join(work, f"{name}-{way}.o")
                binary = os.path.join(work, f"{name}-{way}")
                exe = ["--exe", binary]
                how = ""
                if way == "whole":
                    ran, took = ra_jit(root, [name, *std_of(panic)], obj, *exe)
                elif way == "unit":
                    ran, took = ra_jit(root, [name], obj, "--prebuilt", units[panic], *exe)
                else:
                    store = stores[at == len(programs) - 1]
                    ran, took = ra_jit(root, [name], obj, *exe, env=store)
                    said = re.findall(r"^standard library: (.*)$", ran.stderr, re.M)
                    stored = bool(said) and said[0].startswith("from the store")
                    if not said or stored != (panic in compiled):
                        raise SystemExit(f"FAIL: {name} ({way}, build {at + 1}) "
                                         f"{'should have taken' if panic in compiled else 'took'} the "
                                         f"standard library from the store: {said}\n{ran.stderr}")
                    compiled.add(panic)
                    how = (", the standard library from the remote" if store is stores[1]
                           else ", the standard library from the store" if stored
                           else ", the standard library compiled, stored and pushed")
                link_took = linked(ran, f"{name} ({way})")
                for args, want in zip(runs, expected[name]):
                    got = outcome(binary, args, root)
                    if got != want:
                        raise SystemExit(f"FAIL: {name} {' '.join(args)} ({way}) differs from cargo's:\n"
                                         f"cargo:  {want}\nra-jit: {got}")
                lines.append(f"{name} ({way}): built and linked in {took:.2f} s{how}, the link "
                             f"{link_took}, {len(runs)} runs as cargo's")
        print("\n".join(lines))
        print(f"PASS: {len(PROGRAMS)} programs with nothing of rustc's, {' and '.join(ways)}")
        passed = True
    finally:
        if passed:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
