//! Running what was built, as cargo does: the program of `cargo run`, the test executables
//! of `cargo test`.

use std::{
    path::Path,
    process::{Command, ExitStatus},
};

use anyhow::Context as _;

use crate::plan::Selection;

use super::{
    cli::Invocation,
    say,
    state::{Held, Kind, Program, State, as_built, cargo_path},
};

/// The exit status of cargo when a test fails or a program can't be run.
const FAILED: i32 = 101;

/// Runs the program `asked` names, and ends as it ended. `None` when cargo has to say
/// which program that is: there are several, or none.
pub(crate) fn program(
    asked: &Invocation,
    state: &State,
    cwd: &Path,
    held: Option<Held>,
) -> anyhow::Result<Option<i32>> {
    let runnable = |it: &&Program| !it.test && matches!(it.kind, Kind::Bin | Kind::Example);
    let mut programs: Vec<&Program> = state.programs.iter().filter(runnable).collect();
    if let Some(name) = asked.flags.bin.as_ref().or(asked.flags.example.as_ref()) {
        programs.retain(|it| it.name == *name);
    } else if let [default] = programs.iter().filter(|it| it.default_run).collect::<Vec<_>>()[..] {
        // Cargo runs a `default-run` when one package it builds has one.
        programs = vec![*default];
    }
    let &[program] = programs.as_slice() else { return Ok(None) };
    let shown = shown_path(&program.executable, cwd);
    let args = asked.program_args().map(|it| format!(" {it}")).collect::<String>();
    say::status(asked.flags.quiet, "Running", &format!("`{shown}{args}`"));
    let mut command = Command::new(&program.executable);
    configured(&mut command, state);
    command.args(asked.program_args()).envs(program.env.iter().cloned());
    as_built(&mut command)?;
    let status =
        ended(&mut command, held, state).with_context(|| format!("could not execute {shown}"))?;
    Ok(Some(exit_code(status)))
}

/// Sets for `command` what the `[env]` of cargo's configuration sets, as cargo does: what
/// cargo sets for the program itself goes over it.
fn configured(command: &mut Command, state: &State) {
    for it in &state.configured {
        if it.force || std::env::var_os(&it.name).is_none() {
            command.env(&it.name, &it.value);
        }
    }
}

/// Runs `command` to its end, started under `held` or under cargo's locks on the build
/// directories taken now: a cargo building there meanwhile removes the executables and
/// writes them again, and one started then is not there.
fn ended(command: &mut Command, held: Option<Held>, state: &State) -> std::io::Result<ExitStatus> {
    let held = held.unwrap_or_else(|| state.hold());
    let started = started(command);
    drop(held);
    started?.wait()
}

/// Starts `command` once the daemon is not writing its program (`splice::unwritten`): one
/// started while the file is open for writing is refused ("text file busy").
pub(crate) fn started(command: &mut Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    let _unwritten = crate::splice::unwritten(command.get_program().as_ref());
    command.spawn()
}

/// Runs the test executables one after another, each in the directory of its package, and
/// stops at the first that fails unless told not to. The result is cargo's exit status.
pub(crate) fn tests(
    asked: &Invocation,
    state: &State,
    cwd: &Path,
    mut held: Option<Held>,
) -> anyhow::Result<i32> {
    let mut failed = Vec::new();
    for test in state.programs.iter().filter(|it| it.test) {
        let source = test.source.strip_prefix(&test.package_dir).unwrap_or(&test.source);
        let unit = if matches!(test.kind, Kind::Test | Kind::Bench) { "" } else { "unittests " };
        let shown = shown_path(&test.executable, cwd);
        say::status(asked.flags.quiet, "Running", &format!("{unit}{} ({shown})", source.display()));
        let mut command = Command::new(&test.executable);
        configured(&mut command, state);
        command.args(asked.program_args()).envs(test.env.iter().cloned());
        command.current_dir(&test.package_dir);
        as_built(&mut command)?;
        let status = ended(&mut command, held.take(), state)
            .with_context(|| format!("could not execute {shown}"))?;
        if !status.success() {
            failed.push(rerun_flags(test));
            if !asked.flags.no_fail_fast {
                break;
            }
        }
    }
    match failed.as_slice() {
        [] => Ok(0),
        [one] => {
            say::error(&format!("test failed, to rerun pass `{one}`"));
            Ok(FAILED)
        }
        several => {
            say::error(&format!("{} targets failed:", several.len()));
            several.iter().for_each(|it| eprintln!("    `{it}`"));
            Ok(FAILED)
        }
    }
}

/// Runs the doc tests with cargo, which hands them to ra-jit in rustdoc's place: ra-jit
/// builds each example as its own program over its code of the library.
pub(crate) fn doc_tests(asked: &Invocation) -> anyhow::Result<i32> {
    let mut args = vec!["--doc".to_owned()];
    args.extend(asked.build_args.iter().cloned());
    if asked.flags.no_fail_fast {
        args.push("--no-fail-fast".to_owned());
    }
    args.extend(asked.positional.iter().cloned());
    args.push("--".to_owned());
    args.extend(asked.trailing.iter().cloned());
    only_doc_tests(asked, &args)
}

/// Runs `cargo test` with `args`, which ask for the doc tests alone, with ra-jit in
/// rustdoc's place. ra-jit is told the build `asked` is, whose features cargo builds the
/// libraries with.
pub(crate) fn only_doc_tests(asked: &Invocation, args: &[String]) -> anyhow::Result<i32> {
    let rustdoc = std::env::var_os("RUSTDOC").unwrap_or_else(|| "rustdoc".into());
    let cwd = std::env::current_dir()?;
    let asked_in = asked.flags.manifest_path.as_ref().map_or(cwd.clone(), |it| cwd.join(it));
    let build = Selection { asked_in: Some(asked_in), ..asked.selection() };
    let mut cargo = crate::doctest::as_asked(cargo_path());
    cargo
        .env("RUSTDOC", std::env::current_exe().context("finding ra-jit itself")?)
        .env(crate::doctest::ENABLED, rustdoc)
        .env(crate::doctest::BUILD, serde_json::to_string(&build)?);
    cargo.arg("test").args(args);
    as_built(&mut cargo)?;
    Ok(exit_code(cargo.status().context("running `cargo test --doc`")?))
}

/// The flags that have cargo run this test target alone.
fn rerun_flags(test: &Program) -> String {
    let target = match test.kind {
        Kind::Lib => "--lib".to_owned(),
        Kind::Bin => format!("--bin {}", test.name),
        Kind::Example => format!("--example {}", test.name),
        Kind::Test => format!("--test {}", test.name),
        Kind::Bench => format!("--bench {}", test.name),
    };
    format!("-p {} {target}", test.package)
}

/// The path as cargo shows it: from the directory the command runs in, when it is below.
fn shown_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

/// What this command ends with after a process that ended with `status`: its code, or
/// the code a shell gives one a signal ended.
pub(crate) fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128 + signal;
    }
    status.code().unwrap_or(1)
}
