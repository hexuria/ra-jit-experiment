#!/usr/bin/env bash
# Prepares $NR_ROOT (default ~/no-rustc) on a Linux dev host for count.py: ra-jit at a
# commit, built with the real toolchain, and copies of 1.99.0 and bun's nightly whose rustc,
# rustdoc and cargo log every run (wrap.sh), linked as the rustup toolchains nr-stable and
# nr-nightly. Remove with `rustup toolchain uninstall nr-stable nr-nightly` and rm -rf.
set -euo pipefail
L=${NR_ROOT:-$HOME/no-rustc}
REV=${REV:-origin/main}
HERE=$(cd "${0%/*}" && pwd)
pin=()
[ -n "${NR_CPUS:-}" ] && pin=(taskset -c "$NR_CPUS")
mkdir -p "$L/logs" "$L/apps"
cd "$L"
[ -d src ] || git clone -q https://github.com/indexable-inc/ra-jit src
git -C src fetch -q origin
git -C src checkout -q --detach "$REV"

counted() { # toolchain-dir name
  local from=$HOME/.rustup/toolchains/$1 to=$L/tc-$2
  if [ ! -d "$to" ]; then
    cp -a "$from" "$to"
    for t in rustc rustdoc cargo; do
      mv "$to/bin/$t" "$to/bin/$t.real"
      cp "$HERE/wrap.sh" "$to/bin/$t"
      chmod +x "$to/bin/$t"
    done
  fi
  rustup toolchain link "nr-$2" "$to"
}
counted 1.99.0-x86_64-unknown-linux-gnu stable
counted nightly-2026-09-15-x86_64-unknown-linux-gnu nightly
ls "$L/tc-stable/lib/rustlib/src/rust/library" >/dev/null

cd "$L/src"
CARGO_TARGET_DIR=$L/target "${pin[@]}" cargo +1.99.0 build -p ra-jit --release --bins 2>&1 | tail -3
ls "$L/target/release/ra-jit"
