#!/usr/bin/env python3
"""growth.py: the code of one function grows in a straight line with the arms of its match.

usage: growth.py [ARMS]

Builds a program whose `serialize` is the shape `#[derive(Serialize)]` gives an internally tagged
enum: a match of ARMS arms (default 160), each opening a value with a destructor through `?`, then
`?` on every field of the variant. It is built again with twice the arms, and ra-jit's `serialize`
in the second has to be less than GROWTH_RATIO (default 2.6) times the size of the first. A
straight line doubles; issue #5 was 4 KB per arm squared, 329 MB of code for 285 arms, out of
reach of an AArch64 branch. Both programs are also run against cargo's.
"""
import os, shutil, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ["RA_JIT_CACHE"] = "off"
from difftest import Fixture

CARGO_TOML = """[package]
name = "growth"
version = "0.1.0"
edition = "2021"

[workspace]
"""

HEAD = """use std::fmt::Write;

pub struct Error(pub String);

pub struct Out<'a>(pub &'a mut String);

pub struct State<'a> {
    out: &'a mut String,
    open: bool,
}

impl Drop for State<'_> {
    fn drop(&mut self) {
        if self.open {
            self.out.push('!');
        }
    }
}

pub trait Ser: Sized {
    type St: St;
    fn begin(self, name: &'static str, len: usize) -> Result<Self::St, Error>;
}

pub trait St {
    fn field<T: std::fmt::Display + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Error>;
    fn end(self) -> Result<(), Error>;
}

impl<'a> Ser for Out<'a> {
    type St = State<'a>;
    fn begin(self, name: &'static str, len: usize) -> Result<State<'a>, Error> {
        if len > 1000 {
            return Err(Error(name.to_owned()));
        }
        self.0.push('{');
        Ok(State { out: self.0, open: true })
    }
}

impl St for State<'_> {
    fn field<T: std::fmt::Display + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        if key.is_empty() {
            return Err(Error(String::new()));
        }
        write!(self.out, "{key}:{value},").map_err(|_| Error(String::new()))
    }

    fn end(mut self) -> Result<(), Error> {
        self.open = false;
        self.out.push('}');
        Ok(())
    }
}
"""


def program(arms):
    variants = "".join(f"    V{i} {{ id: String, n: u32 }},\n" if i % 2 else f"    V{i},\n" for i in range(arms))
    cases = "".join(
        f'            Action::V{i} {{ ref id, ref n }} => {{\n'
        f'                let mut state = serializer.begin("Action", 3)?;\n'
        f'                state.field("type", "a/v{i}")?;\n'
        f'                state.field("id", id)?;\n'
        f'                state.field("n", n)?;\n'
        f'                state.end()\n'
        f'            }}\n' if i % 2 else
        f'            Action::V{i} => {{\n'
        f'                let mut state = serializer.begin("Action", 1)?;\n'
        f'                state.field("type", "a/v{i}")?;\n'
        f'                state.end()\n'
        f'            }}\n'
        for i in range(arms))
    return (HEAD + f"\npub enum Action {{\n{variants}}}\n\nimpl Action {{\n"
            "    pub fn serialize<S: Ser>(&self, serializer: S) -> Result<(), Error> {\n"
            f"        match *self {{\n{cases}        }}\n    }}\n}}\n\n"
            "fn main() {\n    let mut out = String::new();\n"
            f'    for action in [Action::V0, Action::V1 {{ id: "x".to_owned(), n: 3 }}, Action::V{arms - 1} {{ id: "y".to_owned(), n: 4 }}] {{\n'
            "        if action.serialize(Out(&mut out)).is_err() {\n            out.push('?');\n        }\n    }\n"
            '    println!("{out}");\n}\n')


def serialize_size(shapes):
    """The bytes of the last `serialize` the shapes log has code for."""
    names, sizes = {}, {}
    for line in open(shapes):
        tag, *rest = line.rstrip("\n").split("\t")
        if tag == "O":
            names[rest[0]] = rest[3]
        elif tag == "C":
            sizes[rest[0]] = int(rest[1])
    found = [size for symbol, size in sizes.items() if names.get(symbol) == "serialize"]
    if not found:
        raise SystemExit("no code for `serialize` in the shapes log")
    return max(found)


def build(arms):
    ws = tempfile.mkdtemp(prefix=f"ra-jit-growth-{arms}-")
    os.makedirs(os.path.join(ws, "src"))
    open(os.path.join(ws, "Cargo.toml"), "w").write(CARGO_TOML)
    open(os.path.join(ws, "src/main.rs"), "w").write(program(arms))
    fx = Fixture(ws, ["growth"], ["-p", "growth"])
    # What is quadratic takes minutes and gigabytes: past this it has failed already.
    fx.ra_jit_timeout = 600
    fx.rustc_build()
    shapes = os.path.join(fx.work, "shapes.tsv")
    os.environ["RA_JIT_SHAPES"] = shapes
    try:
        fx.ra_jit_build()
    finally:
        del os.environ["RA_JIT_SHAPES"]
    expected, got = fx.behaviour(fx.reference), fx.behaviour(fx.binary)
    if expected != got:
        raise SystemExit(f"{arms} arms: ra-jit's program differs from cargo's:\n  rustc: {expected}\n  ra-jit: {got}")
    size = serialize_size(shapes)
    shutil.rmtree(ws, ignore_errors=True)
    return size


def main():
    arms = int(sys.argv[1]) if sys.argv[1:] else 160
    limit = float(os.environ.get("GROWTH_RATIO", "2.6"))
    small, large = build(arms), build(2 * arms)
    ratio = large / small
    print(f"serialize: {small:,} bytes for {arms} arms, {large:,} for {2 * arms}: x{ratio:.2f} "
          f"({small // arms:,} and {large // (2 * arms):,} bytes an arm)")
    if ratio > limit:
        raise SystemExit(f"FAIL: twice the arms is x{ratio:.2f} the code, more than x{limit}")
    print("ok")


if __name__ == "__main__":
    main()
