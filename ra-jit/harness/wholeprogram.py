#!/usr/bin/env python3
"""wholeprogram.py: programs ra-jit builds whole, with a `main` of its own, run as cargo's do.

usage: wholeprogram.py [CASE...] [--release] [--keep]

Each case is a program of the workspace `harness/wholeprogram`, copied to a directory of
its own. Cargo builds it as it is, and again with ra-jit as `RUSTC_WRAPPER` and
`RUSTC_WORKSPACE_WRAPPER` as `runbench.py` builds a program: rustc builds the registry's
crates and stand-ins of no code for the workspace's, and ra-jit compiles every crate of the
program into one object, with the C `main` of it, which is linked in their place. Both
programs run, and what they print on either stream and the status they exit with have to be
the same.

  default_alloc  the default allocator; output on both streams and a status of its own
  custom_alloc   a `#[global_allocator]` of the program that counts what it is asked for
  lib_alloc      the `#[global_allocator]` is in a library of the workspace
  cfg_alloc      a `#[global_allocator]` under a `#[cfg]` that is false, as ripgrep's

The backend is `RA_JIT_BACKEND`'s; `--release` builds both with the release profile.
"""
import argparse, os, shutil, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import apps

CASES = ["default_alloc", "custom_alloc", "lib_alloc", "cfg_alloc"]


def check(ws, case, release):
    spec = {"package": case, "cargo_args": ["--release"] if release else []}
    fx = apps.App(ws, spec, alone=True)
    fx.ra_jit_args = ["--release"] if release else []
    try:
        fx.rustc_build()
        fx.ra_jit_build()
    except SystemExit as failed:
        return f"  ra-jit's build failed: {str(failed)[-800:]}"
    program, _ = apps.reference(ws, {"package": case}, "release" if release else "debug")
    theirs, ours = fx.behaviour(program), fx.behaviour(fx.binary)
    if theirs == ours:
        return None
    lines = []
    for what, a, b in zip(("status", "stdout", "stderr"), theirs, ours):
        if a != b:
            lines.append(f"  {what}: cargo's {a!r}, ra-jit's {b!r}")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("cases", nargs="*", default=CASES)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--keep", action="store_true", help="keep the copy of the workspace")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="ra-jit-wholeprogram-")
    ws = os.path.join(work, "ws")
    shutil.copytree(os.path.join(HERE, "wholeprogram"), ws, ignore=shutil.ignore_patterns("target*"))
    failed = 0
    for case in args.cases:
        wrong = check(ws, case, args.release)
        print(f"{case:15} {'WRONG' if wrong else 'same'}")
        if wrong:
            print(wrong)
            failed += 1
    if args.keep:
        print(f"kept {ws}")
    else:
        shutil.rmtree(work, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
