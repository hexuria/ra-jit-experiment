//! The `llvm` feature links the LLVM that ra-jit's flake pins and no other: its dev shell sets
//! `LLVM_SYS_201_PREFIX` to it and this script links it. llvm-sys is built with `no-llvm-linking`,
//! so its own search (PATH, then Homebrew) links nothing, and a build outside the shell stops here
//! with one message. Windows has no Nix: there llvm-sys links LLVM's own release of the same
//! version (.github/actions/toolchain).

use std::{env, path::Path, process::Command};

const PREFIX: &str = "LLVM_SYS_201_PREFIX";

fn main() {
    println!("cargo::rerun-if-env-changed={PREFIX}");
    if env::var_os("CARGO_FEATURE_LLVM").is_none() || env::var_os("CARGO_CFG_WINDOWS").is_some() {
        return;
    }
    let prefix = env::var(PREFIX).unwrap_or_default();
    let config = Path::new(&prefix).join("bin/llvm-config");
    if !prefix.starts_with("/nix/store/") || !config.exists() {
        let found = if prefix.is_empty() { "unset".to_owned() } else { format!("`{prefix}`") };
        println!(
            "cargo::error=the `llvm` feature builds against the LLVM that ra-jit's flake pins: run \
             cargo inside `nix develop` ({PREFIX} is {found})"
        );
        return;
    }
    let ask = |args: &[&str]| {
        let out = Command::new(&config).args(args).output().expect("llvm-config runs");
        assert!(
            out.status.success(),
            "{} {args:?}: {}",
            config.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("llvm-config prints UTF-8")
    };
    println!("cargo::rustc-link-search=native={}", ask(&["--libdir"]).trim());
    for file in ask(&["--libnames", "--link-shared"]).split_whitespace() {
        let name = file
            .strip_prefix("lib")
            .and_then(|n| n.strip_suffix(".dylib").or_else(|| n.strip_suffix(".so")));
        println!(
            "cargo::rustc-link-lib=dylib={}",
            name.unwrap_or_else(|| panic!("{file} is not a shared library"))
        );
    }
    for flag in ask(&["--system-libs", "--link-shared"]).split_whitespace() {
        let name =
            flag.strip_prefix("-l").unwrap_or_else(|| panic!("{flag} is not a library flag"));
        println!("cargo::rustc-link-lib=dylib={name}");
    }
}
