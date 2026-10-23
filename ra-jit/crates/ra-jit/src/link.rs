//! The link of a program with nothing of rustc's (docs/no-rustc.md, step G): ra-jit's objects
//! and the native libraries the program's crates ask for with `#[link]`, by the system's `cc`,
//! with the arguments rustc gives it for the target. An ELF target links with the toolchain's
//! lld where it has one, as rustc does on x86_64 Linux: GNU ld takes ten times as long to
//! leave out what is not reached.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use hir::Crate;
use hir_def::attrs::{AttrFlags, NativeLib, NativeLibKind};
use ide_db::RootDatabase;
use ide_db::base_db::{self, CrateOrigin, LangCrateOrigin};
use rustc_hash::FxHashSet;

/// Links `objects` into the executable `exe`, with the native libraries of the crates `program`
/// links. How long `cc` took.
pub(crate) fn executable(
    db: &RootDatabase,
    program: &[Crate],
    objects: &[&Path],
    sysroot: Option<&Path>,
    exe: &Path,
) -> anyhow::Result<Duration> {
    let Some(&first) = program.first() else { bail!("nothing to link") };
    let target = Target::of(db, first.into());
    let mut cmd = Command::new("cc");
    if !target.apple
        && let Some(gcc_ld) = sysroot.and_then(lld_dir)
    {
        cmd.arg(format!("-B{}", gcc_ld.display())).arg("-fuse-ld=lld");
    }
    cmd.args(target.leading());
    cmd.args(objects);
    for lib in native_libs(db, program) {
        cmd.args(target.lib(&lib));
    }
    cmd.args(target.trailing());
    cmd.arg("-o").arg(exe);
    let start = Instant::now();
    let ran = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    if !ran.status.success() {
        bail!(
            "the link of {} failed ({}):\n{:?}\n{}",
            exe.display(),
            ran.status,
            cmd,
            String::from_utf8_lossy(&ran.stderr)
        );
    }
    Ok(start.elapsed())
}

/// What the stand-ins left in the executable say: the functions ra-jit could not compile that
/// the program can reach, as the link leaves out every one it can't.
pub(crate) fn stand_ins(exe: &Path) -> anyhow::Result<Vec<String>> {
    let bytes = std::fs::read(exe).with_context(|| format!("reading {}", exe.display()))?;
    let said =
        memchr::memmem::find_iter(&bytes, hir_ty_codegen::STAND_IN_SAYS.as_bytes()).map(|at| {
            let rest = &bytes[at..];
            let end = memchr::memchr(b'\n', rest).unwrap_or(rest.len());
            String::from_utf8_lossy(&rest[..end]).into_owned()
        });
    Ok(said.collect())
}

/// The toolchain's directory with lld as `ld.lld`, for `cc -B`: the `gcc-ld` of the host's
/// `lib/rustlib/<host>/bin`.
fn lld_dir(sysroot: &Path) -> Option<PathBuf> {
    std::fs::read_dir(sysroot.join("lib/rustlib"))
        .ok()?
        .filter_map(|it| Some(it.ok()?.path().join("bin/gcc-ld")))
        .find(|it| it.join("ld.lld").is_file())
}

/// The native libraries of the crates `program` links, as rustc orders them: a crate's before
/// those of the crates it depends on, each crate's in the order written, and each library
/// last where it is asked for more than once.
fn native_libs(db: &RootDatabase, program: &[Crate]) -> Vec<NativeLib> {
    let mut seen = FxHashSet::default();
    let mut order = Vec::new();
    let unlinked = crate::unlinked_panic_runtime(db, program);
    for &krate in program {
        depth_first(db, krate.into(), unlinked, &mut seen, &mut order);
    }
    let mut libs: Vec<NativeLib> = order
        .iter()
        .rev()
        .flat_map(|&krate| {
            hir_def::nameres::crate_def_map(db, krate)
                .modules(db)
                .flat_map(|(_, module)| module.scope(db).extern_blocks())
                .flat_map(|block| AttrFlags::native_libs(db, block).iter().cloned())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut later = FxHashSet::default();
    libs.reverse();
    libs.retain(|lib| later.insert((lib.name.clone(), lib.kind)));
    libs.reverse();
    libs
}

/// Puts `krate` in `order` after every crate it links: what a proc macro, `test` or the
/// panic runtime named `unlinked` links is not linked.
fn depth_first(
    db: &RootDatabase,
    krate: base_db::Crate,
    unlinked: &str,
    seen: &mut FxHashSet<base_db::Crate>,
    order: &mut Vec<base_db::Crate>,
) {
    let data = krate.data(db);
    let left_out = data.target_kind.is_proc_macro()
        || matches!(
            data.origin,
            CrateOrigin::Lang(LangCrateOrigin::ProcMacro | LangCrateOrigin::Test)
        )
        || Crate::from(krate)
            .display_name(db)
            .is_some_and(|name| name.crate_name().as_str() == unlinked);
    if left_out || !seen.insert(krate) {
        return;
    }
    for dep in &data.dependencies {
        depth_first(db, dep.crate_id, unlinked, seen, order);
    }
    order.push(krate);
}

/// What the link line is made of for the target, from the cfgs of the program's crate.
struct Target {
    apple: bool,
    arch: String,
}

impl Target {
    fn of(db: &RootDatabase, krate: base_db::Crate) -> Target {
        let cfg = krate.cfg_options(db);
        let value = |key: &str| {
            cfg.get_cfg_values(key).next().map(|it| it.as_str().to_owned()).unwrap_or_default()
        };
        Target { apple: value("target_vendor") == "apple", arch: value("target_arch") }
    }

    fn leading(&self) -> Vec<String> {
        if self.apple {
            let arch = if self.arch == "aarch64" { "arm64" } else { &self.arch };
            ["-arch", arch].map(str::to_owned).to_vec()
        } else {
            let width = match &*self.arch {
                "x86_64" => Some("-m64"),
                "x86" => Some("-m32"),
                _ => None,
            };
            width.into_iter().chain(["-Wl,--as-needed"]).map(str::to_owned).collect()
        }
    }

    fn lib(&self, lib: &NativeLib) -> Vec<String> {
        let name = lib.name.as_str();
        if self.apple {
            return match lib.kind {
                NativeLibKind::Framework => vec!["-framework".to_owned(), name.to_owned()],
                _ if lib.verbatim => vec![name.to_owned()],
                NativeLibKind::Static if lib.whole_archive => {
                    vec![format!("-Wl,-force_load,lib{name}.a")]
                }
                _ => vec![format!("-l{name}")],
            };
        }
        let l = if lib.verbatim { format!("-l:{name}") } else { format!("-l{name}") };
        match lib.kind {
            NativeLibKind::Static if lib.whole_archive => [
                "-Wl,-Bstatic",
                "-Wl,--whole-archive",
                &l,
                "-Wl,--no-whole-archive",
                "-Wl,-Bdynamic",
            ]
            .map(str::to_owned)
            .to_vec(),
            NativeLibKind::Static => {
                ["-Wl,-Bstatic", &l, "-Wl,-Bdynamic"].map(str::to_owned).to_vec()
            }
            _ => vec![l],
        }
    }

    /// The rest of rustc's arguments: what is not reached is left out, and nothing is linked
    /// that the crates did not ask for.
    fn trailing(&self) -> Vec<String> {
        let args: &[&str] = if self.apple {
            &["-Wl,-dead_strip", "-nodefaultlibs"]
        } else {
            &[
                "-Wl,--eh-frame-hdr",
                "-Wl,-z,noexecstack",
                "-Wl,--gc-sections",
                "-pie",
                "-Wl,-z,relro,-z,now",
                "-nodefaultlibs",
            ]
        };
        args.iter().map(|it| (*it).to_owned()).collect()
    }
}
