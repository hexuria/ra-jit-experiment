//! `ra-jit weaken FILE...`: copies of rustc's Mach-O objects and archives whose definitions
//! give way to ra-jit's.
//!
//! ra-jit's object is linked ahead of what rustc built of the same crates, and defines many of
//! the same names. An ELF linker is told to keep the first definition it meets. Apple's has no
//! such flag: two definitions of a name are an error unless one is weak, and of several weak
//! ones the first on the command line stays. So every definition of the copies is made weak,
//! and private to the program as well, which keeps the loader from looking for the name in
//! the libraries it loads. Nothing else of a file changes, and a file that is no Mach-O
//! object is copied as it is.

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use object::{
    LittleEndian as LE,
    macho::{LC_SYMTAB, MachHeader64, N_EXT, N_PEXT, N_SECT, N_STAB, N_TYPE, N_WEAK_DEF, Nlist64},
    pod,
    read::{
        archive::ArchiveFile,
        macho::{MachHeader as _, Nlist as _},
    },
};

/// The directory of the copies, in the one the files are in.
pub(crate) const DIR: &str = "ra-jit-weak";

/// Writes the copy of each file and prints where it is, in the order given.
pub(crate) fn command(args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    for file in args {
        println!("{}", copy_of(Path::new(&file))?.display());
    }
    Ok(())
}

/// Writes the copy of `file`, unless the one there is of the file as it is, and says where
/// it is.
pub(crate) fn copy_of(file: &Path) -> anyhow::Result<PathBuf> {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        anyhow::bail!("{} names no file", file.display());
    };
    let dir = dir.join(DIR);
    std::fs::create_dir_all(&dir)?;
    let copy = dir.join(name);
    if !up_to_date(&copy, file) {
        let mut data =
            std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
        weaken(&mut data);
        crate::replace(&copy, &data)?;
    }
    Ok(copy)
}

/// Whether the copy was written after the file it is of.
fn up_to_date(copy: &Path, file: &Path) -> bool {
    let written = |path: &Path| std::fs::metadata(path).and_then(|it| it.modified()).ok();
    matches!((written(copy), written(file)), (Some(copy), Some(file)) if copy > file)
}

/// Makes the definitions of an object, or of each object of an archive, weak.
fn weaken(data: &mut [u8]) {
    let members: Vec<Range<usize>> = match ArchiveFile::parse(&*data) {
        Ok(archive) => archive
            .members()
            .filter_map(|member| {
                let (start, len) = member.ok()?.file_range();
                Some(start as usize..(start + len) as usize)
            })
            .collect(),
        Err(_) => vec![0..data.len()],
    };
    for member in members {
        definitions(&mut data[member]);
    }
}

/// Makes the definitions of a Mach-O object weak; an object of another format stays as it is.
pub(crate) fn definitions(data: &mut [u8]) {
    weaken_but(data, |_| false);
}

/// As [`definitions`], for an object of ra-jit's. Of a program's whole, with a C `main` of
/// ra-jit's, that `main` and the functions a stand-in's allocator calls stay as they are: the
/// stand-ins rustc built have weak ones for cargo's link of them, and of two weak ones Apple's
/// linker may keep the stand-in's, which would start the program at whatever is laid out
/// after its `main`. A patch has no `main`, and gives way to the object it patches.
pub(crate) fn ours(data: &mut [u8]) {
    let has_main = defined(data).iter().any(|it| it == b"_main");
    weaken_but(data, |name| {
        let name = name.strip_prefix(b"_").unwrap_or(name);
        has_main
            && (name == b"main"
                || crate::stub::ALLOCATOR_FUNCTIONS.iter().any(|it| it.as_bytes() == name))
    });
}

/// The names an object defines and other objects can see.
fn defined(data: &[u8]) -> Vec<Vec<u8>> {
    let mut data = data.to_vec();
    let mut names = Vec::new();
    weaken_but(&mut data, |name| {
        names.push(name.to_vec());
        true
    });
    names
}

fn weaken_but(data: &mut [u8], mut keep: impl FnMut(&[u8]) -> bool) {
    let Some((symbols, strings)) = symbol_table(data) else { return };
    let (Some(names), Some(_)) = (data.get(strings.clone()), data.get(symbols.clone())) else {
        return;
    };
    let names = names.to_vec();
    let Ok(symbols) = pod::slice_from_all_bytes_mut::<Nlist64<LE>>(&mut data[symbols]) else {
        return;
    };
    for symbol in symbols {
        let defined = symbol.n_type & (N_STAB | N_TYPE) == N_SECT;
        if !defined || symbol.n_type & N_EXT == 0 {
            continue;
        }
        let name = names.get(symbol.n_strx.get(LE) as usize..).unwrap_or_default();
        let name = name.split(|&it| it == 0).next().unwrap_or_default();
        if !keep(name) {
            symbol.n_type |= N_PEXT;
            symbol.n_desc.set(LE, symbol.n_desc(LE) | N_WEAK_DEF);
        }
    }
}

/// Where in a 64-bit Mach-O object its symbols are, and their names.
fn symbol_table(data: &[u8]) -> Option<(Range<usize>, Range<usize>)> {
    let header = MachHeader64::<LE>::parse(data, 0).ok()?;
    let mut commands = header.load_commands(LE, data, 0).ok()?;
    while let Ok(Some(command)) = commands.next() {
        if command.cmd() != LC_SYMTAB {
            continue;
        }
        let symtab = command.symtab().ok()??;
        let start = symtab.symoff.get(LE) as usize;
        let len = symtab.nsyms.get(LE) as usize * size_of::<Nlist64<LE>>();
        let strings = symtab.stroff.get(LE) as usize;
        let strings = strings..strings + symtab.strsize.get(LE) as usize;
        return Some((start..start + len, strings));
    }
    None
}
