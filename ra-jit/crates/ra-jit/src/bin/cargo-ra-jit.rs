//! `cargo ra-jit ...`: cargo runs this as `cargo-ra-jit ra-jit ...`, and it becomes
//! `ra-jit cargo ...`, the `ra-jit` being the one installed next to it.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(this) => this.with_file_name(format!("ra-jit{}", std::env::consts::EXE_SUFFIX)),
        Err(error) => {
            eprintln!("error: cargo-ra-jit does not know where it is installed: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Cargo names the subcommand first.
    let args = std::env::args_os().skip(1).skip_while(|it| it == "ra-jit");
    match Command::new(&exe).arg("cargo").args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("error: cargo-ra-jit could not run {}: {error}", exe.display());
            ExitCode::FAILURE
        }
    }
}
