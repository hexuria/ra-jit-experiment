//! What a debugger reads of a spliced executable: the lines of the patch's code.
//!
//! The sections a debugger reads are not loaded, and each is one run of the file that whoever
//! reads it finds through the section headers. A section cannot be added to where it is, the
//! next one follows it, so each section the patch has a part of is written again behind the
//! program headers: the executable's, then the patch's, which describes a unit of its own and
//! refers to the other sections by how far into them its part now is. The section headers
//! follow, with those sections where they now are.
//!
//! What the executable says of the functions the patch replaces stays: it is true of their
//! old code, which is still there behind the jump.
//!
//! Where each section goes depends on the executable alone, as long as what a patch adds to
//! a section fits in the room behind it: every splice then finds the sections where the last
//! one wrote them, and so does the first, as they are copied there once the executable is
//! linked ([`Described::laid_out`]). Of a program with hundreds of megabytes of them, the copy
//! is seconds.

use anyhow::{Context, bail, ensure};
use object::{
    Endianness, Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget,
    SectionIndex, SymbolKind, elf,
    read::elf::{ElfFile64, FileHeader, SectionHeader},
};
use rustc_hash::FxHashMap;

use super::{Machine, Patch, Target};

/// The section headers of an executable, and the sections among them a debugger reads.
pub(super) struct Described {
    headers: Vec<elf::SectionHeader64<Endianness>>,
    sections: FxHashMap<Box<[u8]>, usize>,
    /// Which of the headers is of the section their names are in.
    names: usize,
    /// The sections a splice writes again behind everything else, in the order the file has
    /// them, whether or not a patch adds to each: those a debugger reads, but not one that
    /// is loaded or compressed.
    laid: Vec<usize>,
}

/// What a section is given to grow into, at the least, before the one behind it moves.
const ROOM: u64 = 1 << 20;

/// A filesystem block: a run written again as far into a block as it was can share the
/// blocks of the original, where the filesystem shares blocks between files.
pub(super) const BLOCK: u64 = 4096;

/// A run of the file that is written again further on.
#[derive(PartialEq)]
pub(super) struct Moved {
    pub(super) from: u64,
    pub(super) len: u64,
    pub(super) to: u64,
}

/// What a splice writes behind everything else for a debugger.
#[derive(Default)]
pub(super) struct Tail {
    pub(super) moved: Vec<Moved>,
    pub(super) written: Vec<(u64, Vec<u8>)>,
    /// Where the section headers are and how many, when they are among what is written.
    pub(super) headers: Option<(u64, u16)>,
}

impl Described {
    pub(super) fn read(file: &ElfFile64<'_, Endianness>) -> anyhow::Result<Described> {
        let endian = file.endian();
        let headers = file.elf_header().section_headers(endian, file.data())?.to_vec();
        let mut sections = FxHashMap::default();
        for section in file.sections() {
            let name = section.name_bytes()?;
            if is_debug(name) {
                sections.insert(name.into(), section.index().0);
            }
        }
        let names = file.elf_header().shstrndx(endian, file.data())? as usize;
        ensure!(names < headers.len(), "the names of the sections are in none of them");
        let flags = u64::from(elf::SHF_ALLOC | elf::SHF_COMPRESSED);
        let mut laid: Vec<usize> = (sections.values().copied())
            .filter(|&it| {
                headers[it].sh_flags(endian) & flags == 0 && headers[it].sh_size(endian) != 0
            })
            .collect();
        laid.sort_unstable_by_key(|&it| headers[it].sh_offset(endian));
        Ok(Described { headers, sections, names, laid })
    }

    fn says_lines(&self) -> bool {
        self.sections.contains_key(&b".debug_info"[..])
    }

    /// The runs a splice copies behind `offset` before a patch adds anything: the splices
    /// that come find them where they are. Nothing, of an executable that tells a debugger
    /// nothing.
    pub(super) fn laid_out(&self, offset: u64) -> Vec<Moved> {
        if !self.says_lines() {
            return Vec::new();
        }
        let e = Endianness::Little;
        let runs: Vec<(u64, u64, u64)> = (self.laid.iter())
            .map(|&it| (self.headers[it].sh_offset(e), self.headers[it].sh_size(e), 0))
            .collect();
        let at = layout(offset, &runs);
        std::iter::zip(runs, at).map(|((from, len, _), to)| Moved { from, len, to }).collect()
    }
}

/// Where each of `runs` goes behind `offset`, given as where it is in the file, how long, and
/// how much a patch adds to it. Each has room behind it for what is added: the more there is
/// of it the more room, so that the same goes wherever it did as long as what is added fits.
/// Each starts as far into a block as it was.
fn layout(offset: u64, runs: &[(u64, u64, u64)]) -> Vec<u64> {
    let (mut planned, mut end) = (offset, offset);
    runs.iter()
        .map(|&(from, len, added)| {
            let to = planned.max(end).next_multiple_of(ROOM) + from % BLOCK;
            planned = to + len + ROOM.max(len / 16);
            end = to + len + added;
            to
        })
        .collect()
}

fn is_debug(name: &[u8]) -> bool {
    name.starts_with(b".debug_")
}

impl Machine {
    /// How many bytes the place in a section a debugger reads takes, which a relocation of
    /// the type fills in.
    fn described(self, r_type: u32) -> anyhow::Result<usize> {
        Ok(match (self, r_type) {
            (Machine::X86_64, elf::R_X86_64_64) | (Machine::Aarch64, elf::R_AARCH64_ABS64) => 8,
            (Machine::X86_64, elf::R_X86_64_32) | (Machine::Aarch64, elf::R_AARCH64_ABS32) => 4,
            (_, other) => bail!("a relocation of type {other} in what a debugger reads"),
        })
    }
}

impl Patch<'_> {
    /// The sections of the patch a debugger reads.
    pub(super) fn described(
        &self,
    ) -> impl Iterator<Item = object::read::elf::ElfSection64<'_, '_, Endianness>> {
        self.file.sections().filter(|it| it.size() != 0 && it.name_bytes().is_ok_and(is_debug))
    }

    /// What goes from `offset` of the file on so that a debugger finds the lines of the
    /// patch's code, whose addresses `address` gives. Nothing, for an executable that
    /// tells a debugger nothing.
    pub(super) fn describe(
        &self,
        described: &Described,
        machine: Machine,
        offset: u64,
        address: impl Fn(Target, i64) -> anyhow::Result<u64>,
    ) -> anyhow::Result<Tail> {
        let e = Endianness::Little;
        let mut tail = Tail::default();
        if !described.says_lines() || self.described().next().is_none() {
            return Ok(tail);
        }
        let mut headers = described.headers.clone();
        let mut sections = described.sections.clone();
        let mut names = Vec::new();
        let named = headers[described.names].sh_size(e);
        let mut theirs = FxHashMap::<SectionIndex, u64>::default();
        for section in self.described() {
            let name = section.name_bytes()?;
            let shown = String::from_utf8_lossy(name);
            let index = *sections.entry(name.into()).or_insert_with(|| {
                let at = named + names.len() as u64;
                names.extend_from_slice(name);
                names.push(0);
                headers.push(empty(section.elf_section_header(), at as u32));
                headers.len() - 1
            });
            let header = &headers[index];
            ensure!(
                header.sh_flags(e) & u64::from(elf::SHF_COMPRESSED) == 0,
                "the executable's {shown} is compressed"
            );
            theirs.insert(section.index(), header.sh_size(e));
        }
        // What the patch adds to each section, by the header of the section.
        let mut added = FxHashMap::<usize, Vec<u8>>::default();
        let mut order = described.laid.clone();
        for section in self.described() {
            let mut data = section.data()?.to_vec();
            for (at, relocation) in section.relocations() {
                let RelocationFlags::Elf { r_type } = relocation.flags() else {
                    bail!("a relocation that is not ELF's")
                };
                let RelocationTarget::Symbol(index) = relocation.target() else {
                    bail!("a relocation against no symbol")
                };
                let symbol = self.file.symbol_by_index(index)?;
                let within = symbol.section_index().and_then(|it| theirs.get(&it));
                let value = match within {
                    Some(len) if symbol.kind() == SymbolKind::Section => {
                        len.wrapping_add_signed(relocation.addend())
                    }
                    Some(_) => bail!("a symbol in what a debugger reads"),
                    None => {
                        let target = self.targets.get(&index).context("code nothing else names")?;
                        address(*target, relocation.addend())?
                    }
                };
                let len = machine.described(r_type)?;
                let field = data.get_mut(at as usize..at as usize + len);
                let field = field.context("a relocation past its section")?;
                if len == 4 {
                    let value = u32::try_from(value).context("more than a debugger is told of")?;
                    field.copy_from_slice(&value.to_le_bytes());
                } else {
                    field.copy_from_slice(&value.to_le_bytes());
                }
            }
            let index = sections[section.name_bytes()?];
            if !order.contains(&index) {
                order.push(index);
            }
            added.insert(index, data);
        }
        let runs: Vec<(u64, u64, u64)> = (order.iter())
            .map(|&it| {
                let header = &headers[it];
                let added = added.get(&it).map_or(0, |it| it.len() as u64);
                (header.sh_offset(e), header.sh_size(e), added)
            })
            .collect();
        let mut end = offset;
        let at = layout(offset, &runs);
        for ((&index, &(from, len, _)), to) in order.iter().zip(&runs).zip(at) {
            tail.moved.push(Moved { from, len, to });
            let header = &mut headers[index];
            header.sh_offset.set(e, to);
            end = to + len;
            if let Some(data) = added.remove(&index) {
                header.sh_size.set(e, len + data.len() as u64);
                end += data.len() as u64;
                tail.written.push((to + len, data));
            }
        }
        if !names.is_empty() {
            let header = &mut headers[described.names];
            tail.moved.push(Moved { from: header.sh_offset(e), len: named, to: end });
            header.sh_offset.set(e, end);
            header.sh_size.set(e, named + names.len() as u64);
            tail.written.push((end + named, names));
            end += header.sh_size(e);
        }
        let end = end.next_multiple_of(8);
        let count = u16::try_from(headers.len()).ok().filter(|&it| it < elf::SHN_LORESERVE);
        let count = count.context("more sections than the file's header counts")?;
        tail.written.push((end, object::bytes_of_slice(&headers).to_vec()));
        tail.headers = Some((end, count));
        Ok(tail)
    }
}

/// The header of a section the executable has none of yet, like the patch's and with nothing
/// in it, named by what is `name` into the names of the sections.
fn empty(like: &elf::SectionHeader64<Endianness>, name: u32) -> elf::SectionHeader64<Endianness> {
    let e = Endianness::Little;
    elf::SectionHeader64 {
        sh_name: object::U32::new(e, name),
        sh_type: like.sh_type,
        sh_flags: like.sh_flags,
        sh_addr: object::U64::new(e, 0),
        sh_offset: object::U64::new(e, 0),
        sh_size: object::U64::new(e, 0),
        sh_link: object::U32::new(e, 0),
        sh_info: object::U32::new(e, 0),
        sh_addralign: object::U64::new(e, 1),
        sh_entsize: like.sh_entsize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sizes like those of nushell's debug sections, as the linker left them.
    const NUSHELL: [(u64, u64); 6] = [
        (0x7619545, 0x25ce04),
        (0x7876349, 0xc118872),
        (0x1398ebbb, 0x2c92214),
        (0x16620dcf, 0x1e16c50),
        (0x18437a1f, 0xca5992d),
        (0x24e9134c, 0x48351d),
    ];
    const OFFSET: u64 = 0x2a00_0000;

    fn runs(added: &[u64]) -> Vec<(u64, u64, u64)> {
        NUSHELL
            .iter()
            .zip(added.iter().chain(std::iter::repeat(&0)))
            .map(|(&(from, len), &added)| (from, len, added))
            .collect()
    }

    #[test]
    fn what_fits_in_the_room_leaves_every_section_where_it_was_laid_out() {
        let laid = layout(OFFSET, &runs(&[]));
        let added = [0x1000, 0x8_0000, 0x4_0000, ROOM, 0x20_0000, 0x100];
        assert_eq!(layout(OFFSET, &runs(&added)), laid);
    }

    #[test]
    fn a_section_that_outgrows_its_room_moves_only_those_behind_it() {
        let laid = layout(OFFSET, &runs(&[]));
        let moved = layout(OFFSET, &runs(&[0, 0, 0x100_0000]));
        assert_eq!(moved[..3], laid[..3]);
        assert!(moved[3..].iter().zip(&laid[3..]).all(|(moved, laid)| moved > laid));
    }

    #[test]
    fn each_starts_as_far_into_a_block_as_it_was_and_behind_the_one_before() {
        let added = [0, 0x10_0000, 0, 0x200_0000, 0, 0];
        let runs = runs(&added);
        let at = layout(OFFSET, &runs);
        let mut end = OFFSET;
        for (&(from, len, added), &to) in runs.iter().zip(&at) {
            assert_eq!(to % BLOCK, from % BLOCK);
            assert!(to >= end, "{to:#x} is before {end:#x}");
            end = to + len + added;
        }
    }
}
