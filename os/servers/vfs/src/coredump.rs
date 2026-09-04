//! `coredump` — post-mortem examination: freeze the dead into a volume.
//!
//! Corresponds to Minix3's `coredump.c:1-327` (`write_elf_core_file`,
//! `fill_elf_header`, `fill_prog_header`,
//! `fill_note_segment_and_entries_hdrs`, `adjust_offsets`, `write_buf`,
//! `get_memory_regions`, `dump_notes`, `dump_elf_header`,
//! `dump_program_headers`, `dump_segments`) and `pm_dumpcore`
//! (`misc.c:903-945`).
//!
//! Design decisions (see 26-coredump.md §3):
//! - `DumpPhase` types the seven pipeline stages
//! - `pad_len`/`note_filesize`/`plan_notes` type the note ruler
//! - `adjust_offsets` types the running file offsets purely
//! - `prot_to_pf`/`RegionSource` type the region roll (test doubles)
//! - `truncate_len`/`chunk_plan`/`GapAction` type the flesh policy
//! - `core_name`/`OpenSpec`/`DumpcorePlan` type the coffin checklist
//! - `CoreWriter` trait types the pen (test doubles)
//!
//! Scope note: VM region queries (`vm_info_region`) stay VM-side;
//! register reads (`sys_getregs`) and memory copies (`sys_datacopy_try`)
//! stay kernel-side; FS writes (`read_write`) stay with 16-read-write.md;
//! file opening stays with 15-open-close.md; `free_proc` stays with
//! 10-pm-protocol.md; signal semantics stay with 04-stage-pm/11-13.
//! This module only decides: phase, ruler, offsets, roll, policy,
//! checklist, and pen.
//!
//! Linux models the same rite as `fs/binfmt_elf.c` (`elf_core_dump`:
//! notes first, then segments, missing pages zeroed); Redox models it as
//! a crash volume with header, notes, and segments. Here `DumpPhase` is
//! the rite and [`RegionSource`]/[`CoreWriter`] are the per-world answers.

/// `MAX_REGIONS` (`coredump.c:37`): at most 100 memory segments per volume.
pub const MAX_REGIONS: usize = 100;
/// `NR_NOTE_ENTRIES` (`coredump.c:38`): exactly two notes (identity + regs).
pub const NR_NOTE_ENTRIES: usize = 2;
/// `MAX_VRI_COUNT` (`minix3/minix/include/minix/vm.h:66`): regions per batch.
pub const MAX_VRI_COUNT: usize = 64;
/// `CLICK_SIZE` (`minix3/minix/include/minix/const.h:84`): copy quantum (VM keeps its own private copy).
pub const CLICK_SIZE: u64 = 4096;
/// `LONG_MAX` bound for segment lengths (`coredump.c:306`, 64-bit).
pub const LONG_MAX_U64: u64 = i64::MAX as u64;

/// `ET_CORE` (`minix3/sys/sys/exec_elf.h:201`): core file type.
pub const ET_CORE: u16 = 4;
/// `PT_LOAD` (`exec_elf.h:349`): loadable segment.
pub const PT_LOAD: u32 = 1;
/// `PT_NOTE` (`exec_elf.h:352`): auxiliary notes.
pub const PT_NOTE: u32 = 4;
/// `PF_R` (`exec_elf.h:372`).
pub const PF_R: u32 = 0x4;
/// `PF_W` (`exec_elf.h:373`).
pub const PF_W: u32 = 0x2;
/// `PF_X` (`exec_elf.h:374`).
pub const PF_X: u32 = 0x1;
/// `EV_CURRENT` (`exec_elf.h:169`).
pub const EV_CURRENT: u32 = 1;
/// `ELFOSABI_FREEBSD` (`exec_elf.h:182`).
pub const ELFOSABI_FREEBSD: u8 = 9;

/// `ELFMAG0..3` (`exec_elf.h:149-152`): `0x7f,'E','L','F'`.
pub const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
/// `EI_MAG0..3` (`exec_elf.h:136-139`): magic indexes 0..3.
pub const EI_MAG: [usize; 4] = [0, 1, 2, 3];
/// `EI_CLASS` (`exec_elf.h:140`).
pub const EI_CLASS: usize = 4;
/// `EI_DATA` (`exec_elf.h:141`).
pub const EI_DATA: usize = 5;
/// `EI_VERSION` (`exec_elf.h:142`).
pub const EI_VERSION: usize = 6;
/// `EI_OSABI` (`exec_elf.h:143`).
pub const EI_OSABI: usize = 7;

/// `NT_MINIX_ELFCORE_INFO` (`minix3/minix/include/sys/elf_core.h:31`).
pub const NT_MINIX_ELFCORE_INFO: u32 = 1;
/// `NT_MINIX_ELFCORE_GREGS` (`elf_core.h:32`).
pub const NT_MINIX_ELFCORE_GREGS: u32 = 2;
/// `MINIX_ELFCORE_VERSION` (`elf_core.h:34`).
pub const MINIX_ELFCORE_VERSION: u32 = 1;
/// `ELF_NOTE_MINIX_ELFCORE_NAME` (`elf_core.h:30`): `"MINIX-CORE"`.
pub const ELF_NOTE_NAME: &[u8] = b"MINIX-CORE";

/// `CORE_NAME` (`misc.c:40`): coffin basename.
pub const CORE_NAME: &[u8] = b"core";
/// `CORE_MODE` (`misc.c:41`): `0777` for core image files.
pub const CORE_MODE: u32 = 0o777;
/// `O_CREAT` (`minix3/sys/sys/fcntl.h:99`).
pub const O_CREAT: u32 = 0x0000_0200;
/// `O_TRUNC` (`fcntl.h:100`).
pub const O_TRUNC: u32 = 0x0000_0400;
// `O_WRONLY`: authoritative at `crate::open::O_WRONLY` (single source).
pub use crate::open::O_WRONLY;

/// Coffin open flags (`misc.c:922-923`: `O_WRONLY | O_CREAT | O_TRUNC`).
pub const CORE_OPEN_FLAGS: u32 = 0x0000_0001 | O_CREAT | O_TRUNC;

/// `MAXCOMLEN` (`minix3/sys/sys/param.h:117`): command-name bytes.
pub const MAXCOMLEN: usize = 16;

/// Arch-provided ELF identity triple.
///
/// `ELF_TARG_CLASS/DATA/MACH` differ per architecture (e.g. i386 vs
/// earm `elf.h`); the decision layer takes them as parameters instead
/// of hardcoding one architecture's triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfTarget {
    /// `e_ident[EI_CLASS]` (e.g. `ELFCLASS32`).
    pub class: u8,
    /// `e_ident[EI_DATA]` (e.g. `ELFDATA2LSB`).
    pub data: u8,
    /// `e_machine` (e.g. `EM_386`/`EM_ARM`).
    pub machine: u16,
}

/// The seven pipeline stages (header contract, `coredump.c:34-36,64-74`).
///
/// Bone before flesh: headers, then notes, then segment contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpPhase {
    /// Fill the NOTE segment header + two note entry headers (`49`).
    NoteHeaders,
    /// Collect memory regions into program headers (`52`).
    CollectRegions,
    /// Fill the ELF header (`55`).
    ElfHeader,
    /// Run the file offsets (`62`).
    AdjustOffsets,
    /// Dump the ELF header (`65`).
    DumpHeader,
    /// Dump program headers + note contents (`68-71`).
    DumpNotes,
    /// Dump segment contents (`74`).
    DumpSegments,
}

/// `PADBYTES` (`coredump.c:120`): note alignment quantum.
pub const PADBYTES: usize = 4;

/// `PAD_LEN` (`coredump.c:121`): round up to 4-byte alignment.
pub fn pad_len(n: usize) -> usize {
    (n + (PADBYTES - 1)) & !(PADBYTES - 1)
}

/// Note kind: the two mandatory entries (`coredump.c:149-156`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// `NT_MINIX_ELFCORE_INFO`: identity (version/size/signal/pid/name).
    Info,
    /// `NT_MINIX_ELFCORE_GREGS`: general registers.
    Gregs,
}

impl NoteKind {
    /// Note type number.
    pub fn n_type(self) -> u32 {
        match self {
            Self::Info => NT_MINIX_ELFCORE_INFO,
            Self::Gregs => NT_MINIX_ELFCORE_GREGS,
        }
    }
}

/// One note descriptor header (`Elf_Nhdr` triple).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteDesc {
    /// `n_namesz`: note name length including NUL.
    pub name_len: usize,
    /// `n_descsz`: payload length.
    pub desc_len: usize,
    /// `n_type`: which note.
    pub kind: NoteKind,
}

/// Note payload size in the volume (`coredump.c:144-145`).
///
/// Sums padded payloads, two note headers (`sizeof(Elf_Nhdr)` = 12 =
/// three words), and two padded names — the padding is part of the
/// size, not an afterthought.
pub fn note_filesize(mei_len: usize, gregs_len: usize, name_len: usize) -> usize {
    pad_len(mei_len) + pad_len(gregs_len) + 2 * 12 + 2 * pad_len(name_len)
}

/// Plan the two note descriptors (`coredump.c:148-156`).
///
/// Both share the name length; identity carries `mei_len`, registers
/// carry `gregs_len`.
pub fn plan_notes(name_len: usize, mei_len: usize, gregs_len: usize) -> [NoteDesc; 2] {
    [
        NoteDesc {
            name_len,
            desc_len: mei_len,
            kind: NoteKind::Info,
        },
        NoteDesc {
            name_len,
            desc_len: gregs_len,
            kind: NoteKind::Gregs,
        },
    ]
}

/// Program-header fill (`fill_prog_header`, `coredump.c:104-118`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProgHeader {
    /// `p_type` (`PT_NOTE`/`PT_LOAD`).
    pub typ: u32,
    /// `p_offset`: file offset (zero until [`adjust_offsets`]).
    pub offset: u64,
    /// `p_vaddr`: virtual address.
    pub vaddr: u64,
    /// `p_flags` (`PF_R/W/X`).
    pub flags: u32,
    /// `p_filesz`: bytes in the volume.
    pub filesz: u64,
    /// `p_memsz`: bytes in memory.
    pub memsz: u64,
}

impl ProgHeader {
    /// Zeroed header with six fields set (`coredump.c:109-116`).
    pub fn new(typ: u32, offset: u64, vaddr: u64, flags: u32, filesz: u64, memsz: u64) -> Self {
        Self {
            typ,
            offset,
            vaddr,
            flags,
            filesz,
            memsz,
        }
    }
}

/// Run the file offsets (`adjust_offsets`, `coredump.c:162-171`).
///
/// Layout invariant (volume order, `57-60`): ELF header, NOTE segment
/// header, remaining program headers, note contents, segment contents.
/// First offset = `ehsize + phnum * phentsize`; each entry takes its
/// running offset and advances by its file size. Fixed arrays (no_std).
pub fn adjust_offsets(
    ehsize: u64,
    phentsize: u64,
    filesizes: &[u64; MAX_REGIONS],
    phnum: usize,
    out: &mut [u64; MAX_REGIONS],
) {
    let mut offset = ehsize + phnum as u64 * phentsize;
    for i in 0..phnum {
        out[i] = offset;
        offset += filesizes[i];
    }
}

/// One VM memory region (one `vm_region_info` answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemRegion {
    /// Region base address (`vri_addr`).
    pub addr: u64,
    /// Region length (`vri_length`).
    pub len: u64,
    /// Readable (`vri_prot & PROT_READ`).
    pub read: bool,
    /// Writable (`vri_prot & PROT_WRITE`).
    pub write: bool,
    /// Executable (`vri_prot & PROT_EXEC`).
    pub exec: bool,
}

/// `prot` → `PF_*` conversion (`get_memory_regions:210-212`).
pub fn prot_to_pf(read: bool, write: bool, exec: bool) -> u32 {
    (if read { PF_R } else { 0 }) | (if write { PF_W } else { 0 }) | (if exec { PF_X } else { 0 })
}

/// Region roll outcome (`get_memory_regions:191-228`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionRoll {
    /// Regions collected (capped at [`MAX_REGIONS`]).
    pub count: usize,
    /// Hit the cap: warned and stopped (`219-223`), not truncated silently.
    pub capped: bool,
}

/// The VM region cursor behind a trait.
///
/// `vm_info_region` (`coredump.c:205`) pages through the address space
/// `MAX_VRI_COUNT` regions at a time; the VM is the only untestable
/// point, so only the batching is abstracted. Negative values are raw
/// VM errnos, passed through untouched (`206`).
pub trait RegionSource {
    /// Next batch into `out`: count written; `0` means done (`r == 0`,
    /// `207`). `Err(r)` passes a negative VM errno through.
    fn next_batch(&mut self, out: &mut [MemRegion; MAX_VRI_COUNT]) -> Result<usize, i32>;
}

/// Scripted regions (test double with programmed batches).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedRegions {
    /// Programmed batches (each entry = one batch of regions).
    pub batches: [[MemRegion; 2]; 4],
    /// Lengths per batch.
    pub lens: [usize; 4],
    /// Number of batches.
    pub nbatches: usize,
    /// Next batch index.
    pub pos: usize,
}

impl ScriptedRegions {
    /// One batch of `len` regions.
    pub fn single(regions: [MemRegion; 2], len: usize) -> Self {
        Self {
            batches: [
                regions,
                [MemRegion::default(); 2],
                [MemRegion::default(); 2],
                [MemRegion::default(); 2],
            ],
            lens: [len, 0, 0, 0],
            nbatches: 1,
            pos: 0,
        }
    }
}

impl RegionSource for ScriptedRegions {
    fn next_batch(&mut self, out: &mut [MemRegion; MAX_VRI_COUNT]) -> Result<usize, i32> {
        if self.pos >= self.nbatches {
            return Ok(0);
        }
        let len = self.lens[self.pos];
        for (i, r) in self.batches[self.pos].iter().take(len).enumerate() {
            out[i] = *r;
        }
        self.pos += 1;
        Ok(len)
    }
}

/// Empty address space (test double: no regions at all).
///
/// Behaves differently from [`ScriptedRegions`] (blanket empty vs
/// programmed batches), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NoRegions;

impl RegionSource for NoRegions {
    fn next_batch(&mut self, _out: &mut [MemRegion; MAX_VRI_COUNT]) -> Result<usize, i32> {
        Ok(0)
    }
}

/// Roll the region cursor into load headers (`coredump.c:201-227`).
///
/// Each region becomes `PT_LOAD` with `filesz == memsz == length`
/// (`214-216`); the NOTE header at index 0 is the caller's business.
/// Stops — with `capped` set — at [`MAX_REGIONS`] (`219-223`).
pub fn collect_regions<S: RegionSource>(
    src: &mut S,
    phdrs: &mut [ProgHeader; MAX_REGIONS],
) -> Result<RegionRoll, i32> {
    let mut batch = [MemRegion::default(); MAX_VRI_COUNT];
    let mut count = 0;
    loop {
        let n = src.next_batch(&mut batch)?;
        if n == 0 {
            break;
        }
        for region in batch.iter().take(n) {
            if count >= MAX_REGIONS {
                return Ok(RegionRoll {
                    count,
                    capped: true,
                });
            }
            phdrs[count] = ProgHeader::new(
                PT_LOAD,
                0,
                region.addr,
                prot_to_pf(region.read, region.write, region.exec),
                region.len,
                region.len,
            );
            count += 1;
        }
        if count >= MAX_REGIONS {
            return Ok(RegionRoll {
                count,
                capped: true,
            });
        }
    }
    Ok(RegionRoll {
        count,
        capped: false,
    })
}

/// Truncate over-long segments (`dump_segments:306-309`).
///
/// Past-`LONG_MAX` segments truncate (with the C printf note); they do
/// not fail the dump — half a corpse beats none.
pub fn truncate_len(len: u64) -> u64 {
    len.min(LONG_MAX_U64)
}

/// Chunk plan for one segment (`dump_segments:311-324`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPlan {
    /// Full `CLICK_SIZE` chunks.
    pub full: u64,
    /// Trailing partial bytes.
    pub tail: u64,
}

/// Split a segment into full clicks plus a tail (`311-324`).
pub fn chunk_plan(len: u64) -> ChunkPlan {
    ChunkPlan {
        full: len / CLICK_SIZE,
        tail: len % CLICK_SIZE,
    }
}

/// Missing-page policy (`dump_segments:317-321`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapAction {
    /// Copy failed: write zeroes, continue with the next chunk.
    ///
    /// Absent pages in a dead process are routine; aborting would void
    /// the whole volume for one missing page.
    ZeroAndContinue,
}

/// Classify a failed chunk copy (`317-321`): always zero-fill.
pub fn gap_action() -> GapAction {
    GapAction::ZeroAndContinue
}

/// Coffin name (`pm_dumpcore:921`): `"core.<pid>"`, built byte-wise (no_std).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreName {
    /// Name bytes.
    pub bytes: [u8; 24],
    /// Live length.
    pub len: usize,
}

/// Build `"core.<pid>"` (`misc.c:921`).
pub fn core_name(pid: u32) -> CoreName {
    let mut bytes = [0u8; 24];
    bytes[0] = b'c';
    bytes[1] = b'o';
    bytes[2] = b'r';
    bytes[3] = b'e';
    bytes[4] = b'.';
    // Decimal digits, reversed then flipped (no `format!` under no_std).
    let mut digits = [0u8; 10];
    let mut ndigits = 0;
    let mut rest = pid;
    loop {
        digits[ndigits] = b'0' + (rest % 10) as u8;
        ndigits += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    let mut len = 5;
    for i in (0..ndigits).rev() {
        if len < bytes.len() {
            bytes[len] = digits[i];
            len += 1;
        }
    }
    CoreName { bytes, len }
}

/// Coffin open specification (`misc.c:922-924`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenSpec {
    /// `O_WRONLY | O_CREAT | O_TRUNC`.
    pub flags: u32,
    /// `CORE_MODE` (`0777`).
    pub mode: u32,
}

/// The coffin open spec (`misc.c:922-924`).
pub const CORE_OPEN_SPEC: OpenSpec = OpenSpec {
    flags: CORE_OPEN_FLAGS,
    mode: CORE_MODE,
};

/// Terminate the process name (`misc.c:930-931`): force the last byte NUL.
pub fn terminate_name(name: &mut [u8; MAXCOMLEN]) {
    name[MAXCOMLEN - 1] = 0;
}

/// `pm_dumpcore` checklist (`misc.c:903-945`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpcorePlan {
    /// Blocked: unpause first (`916-917`) — a dump needs a still process,
    /// and stillness needs no pending business.
    pub unpause_first: bool,
    /// The coffin fd closes with the process exit (`943-944`), not here.
    pub close_with_exit: bool,
}

/// Plan the opening (`misc.c:916-917,943-944`).
pub fn plan_dumpcore(was_blocked: bool) -> DumpcorePlan {
    DumpcorePlan {
        unpause_first: was_blocked,
        close_with_exit: true,
    }
}

/// The pen behind a trait.
///
/// `write_buf` (`coredump.c:176-186`) writes through `read_write` with a
/// dummy fd (`-1`): regular files never suspend, and suspension is the
/// only thing the fd would be for (the TODO note, `179-184`). The FS is
/// the only untestable point, so only the write is abstracted.
pub trait CoreWriter {
    /// Append `bytes` to the volume.
    fn write(&mut self, bytes: &[u8]);
}

/// Scripted pen (test double recording content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedWriter {
    /// Bytes recorded (up to the buffer).
    pub recorded: [u8; 256],
    /// Bytes recorded so far.
    pub nrecorded: usize,
    /// Total bytes offered (including overflow past the buffer).
    pub total: usize,
}

impl Default for ScriptedWriter {
    fn default() -> Self {
        Self {
            recorded: [0; 256],
            nrecorded: 0,
            total: 0,
        }
    }
}

impl CoreWriter for ScriptedWriter {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            if self.nrecorded < self.recorded.len() {
                self.recorded[self.nrecorded] = *b;
            }
            self.nrecorded += 1;
        }
        self.total += bytes.len();
    }
}

/// Counting pen (test double: measures, never stores).
///
/// Behaves differently from [`ScriptedWriter`] (volume accounting vs
/// content capture), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CountingWriter {
    /// Total bytes offered.
    pub total: usize,
    /// Write calls made.
    pub calls: usize,
}

impl CoreWriter for CountingWriter {
    fn write(&mut self, bytes: &[u8]) {
        self.total += bytes.len();
        self.calls += 1;
    }
}

/// What the dumper tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreVerdict {
    /// Synchronous to the end; reply when done (no SUSPEND here).
    Done,
}

/// Errors of this module, each mapping to one Minix3 errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    /// `EIO`: corrupt ranges, failed stat/header reads.
    Io,
    /// `ENOMEM`: over-limit tables or stacks.
    NoMem,
    /// `EINVAL`: bad dumper inputs.
    Inval,
    /// `ENFILE`: vnode/fd table exhaustion on coffin paths.
    NoSpace,
}

impl CoreError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Io => minix_types::EIO,
            Self::NoMem => minix_types::ENOMEM,
            Self::Inval => minix_types::EINVAL,
            Self::NoSpace => minix_types::ENFILE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_and_ruler() {
        // Seven stages (`coredump.c:34-36,64-74`): bone before flesh.
        let phases = [
            DumpPhase::NoteHeaders,
            DumpPhase::CollectRegions,
            DumpPhase::ElfHeader,
            DumpPhase::AdjustOffsets,
            DumpPhase::DumpHeader,
            DumpPhase::DumpNotes,
            DumpPhase::DumpSegments,
        ];
        assert_eq!(phases.len(), 7);
        // `PAD_LEN` rounds up to fours (`120-121`).
        assert_eq!(pad_len(0), 0);
        assert_eq!(pad_len(1), 4);
        assert_eq!(pad_len(4), 4);
        assert_eq!(pad_len(5), 8);
        assert_eq!(pad_len(11), 12);
        // Note filesize sums padded parts (`144-145`).
        // "MINIX-CORE\0" is 11 bytes → pads to 12.
        let name_len = ELF_NOTE_NAME.len() + 1;
        assert_eq!(name_len, 11);
        assert_eq!(pad_len(name_len), 12);
        let size = note_filesize(32, 64, name_len);
        assert_eq!(size, pad_len(32) + pad_len(64) + 2 * 12 + 2 * 12);
        // Two descriptors share the name, split the payloads (`149-156`).
        let notes = plan_notes(name_len, 32, 64);
        assert_eq!(notes[0].kind.n_type(), NT_MINIX_ELFCORE_INFO);
        assert_eq!(notes[1].kind.n_type(), NT_MINIX_ELFCORE_GREGS);
        assert_eq!(notes[0].desc_len, 32);
        assert_eq!(notes[1].desc_len, 64);
    }

    #[test]
    fn test_offsets_and_headers() {
        // ELF32 canonical sizes (spec, not Minix): ehdr 52, phdr 32.
        let ehsize = 52u64;
        let phentsize = 32u64;
        // Three entries: NOTE (fixed 104) + two segments.
        let mut filesizes = [0u64; MAX_REGIONS];
        filesizes[0] = 104;
        filesizes[1] = 4096;
        filesizes[2] = 8192;
        let mut out = [0u64; MAX_REGIONS];
        adjust_offsets(ehsize, phentsize, &filesizes, 3, &mut out);
        // First offset past headers (`165`), then running sums (`167-170`).
        assert_eq!(out[0], 52 + 3 * 32);
        assert_eq!(out[1], out[0] + 104);
        assert_eq!(out[2], out[1] + 4096);
        // `fill_prog_header` zeroes then sets six fields (`109-116`).
        let h = ProgHeader::new(PT_LOAD, 0, 0x1000, PF_R | PF_W, 100, 100);
        assert_eq!(
            (h.typ, h.flags, h.filesz, h.memsz),
            (PT_LOAD, 0x6, 100, 100)
        );
        // Magic spells 0x7f,'E','L','F' (`84-87`); core type is ET_CORE.
        assert_eq!(ELFMAG, [0x7f, b'E', b'L', b'F']);
        assert_eq!(
            (ET_CORE, PT_NOTE, EV_CURRENT, ELFOSABI_FREEBSD),
            (4, 4, 1, 9)
        );
    }

    #[test]
    fn test_region_roll() {
        // prot bits convert independently (`210-212`).
        assert_eq!(prot_to_pf(true, false, false), PF_R);
        assert_eq!(prot_to_pf(true, true, true), PF_R | PF_W | PF_X);
        assert_eq!(prot_to_pf(false, false, false), 0);
        // One batch rolls into load headers (`214-216`, filesz == memsz).
        let regions = [
            MemRegion {
                addr: 0x1000,
                len: 0x2000,
                read: true,
                write: true,
                exec: false,
            },
            MemRegion {
                addr: 0x8000,
                len: 0x1000,
                read: true,
                write: false,
                exec: true,
            },
        ];
        let mut src = ScriptedRegions::single(regions, 2);
        let mut phdrs = [ProgHeader::default(); MAX_REGIONS];
        let roll = collect_regions(&mut src, &mut phdrs).unwrap();
        assert_eq!((roll.count, roll.capped), (2, false));
        assert_eq!(phdrs[0].typ, PT_LOAD);
        assert_eq!(phdrs[0].filesz, phdrs[0].memsz);
        assert_eq!(phdrs[1].flags, PF_R | PF_X);
        // Empty spaces roll nothing (second impl).
        let mut empty = NoRegions;
        let roll = collect_regions(&mut empty, &mut phdrs).unwrap();
        assert_eq!((roll.count, roll.capped), (0, false));
        // Negative VM errnos pass through (`206`).
        struct BadRegions;
        impl RegionSource for BadRegions {
            fn next_batch(&mut self, _out: &mut [MemRegion; MAX_VRI_COUNT]) -> Result<usize, i32> {
                Err(-5)
            }
        }
        assert_eq!(collect_regions(&mut BadRegions, &mut phdrs), Err(-5));
    }

    #[test]
    fn test_flesh_policy() {
        // Past-LONG_MAX truncates (`306-309`); the dump goes on.
        assert_eq!(truncate_len(u64::MAX), LONG_MAX_U64);
        assert_eq!(truncate_len(100), 100);
        // Clicks plus a tail (`311-324`).
        assert_eq!(chunk_plan(8192), ChunkPlan { full: 2, tail: 0 });
        assert_eq!(chunk_plan(5000), ChunkPlan { full: 1, tail: 904 });
        assert_eq!(chunk_plan(100), ChunkPlan { full: 0, tail: 100 });
        // Missing pages zero-fill and continue (`317-321`).
        assert_eq!(gap_action(), GapAction::ZeroAndContinue);
    }

    #[test]
    fn test_coffin_checklist() {
        // `"core.<pid>"` (`misc.c:921`).
        let name = core_name(12);
        assert_eq!(&name.bytes[..name.len], b"core.12");
        let name = core_name(0);
        assert_eq!(&name.bytes[..name.len], b"core.0");
        // Open spec is WRONLY|CREAT|TRUNC at 0777 (`922-924`).
        assert_eq!(CORE_OPEN_SPEC.flags, 0x1 | O_CREAT | O_TRUNC);
        assert_eq!(CORE_OPEN_SPEC.mode, CORE_MODE);
        // Blocked dumps unpause first; coffins close with the exit.
        assert_eq!(
            plan_dumpcore(true),
            DumpcorePlan {
                unpause_first: true,
                close_with_exit: true
            }
        );
        assert_eq!(
            plan_dumpcore(false),
            DumpcorePlan {
                unpause_first: false,
                close_with_exit: true
            }
        );
        // Names terminate (`930-931`).
        let mut buf = [b'x'; MAXCOMLEN];
        terminate_name(&mut buf);
        assert_eq!(buf[MAXCOMLEN - 1], 0);
        // Pens: content capture vs volume accounting.
        let mut scripted = ScriptedWriter::default();
        scripted.write(b"ELF");
        assert_eq!(scripted.total, 3);
        assert_eq!(&scripted.recorded[..3], b"ELF");
        let mut counting = CountingWriter::default();
        counting.write(b"ELF");
        counting.write(b"!!");
        assert_eq!((counting.total, counting.calls), (5, 2));
    }

    #[test]
    fn test_errno_map_covers_coredump_c() {
        for (err, errno) in [
            (CoreError::Io, minix_types::EIO),
            (CoreError::NoMem, minix_types::ENOMEM),
            (CoreError::Inval, minix_types::EINVAL),
            (CoreError::NoSpace, minix_types::ENFILE),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // Volume bounds hold: 100 regions + 1 NOTE header (`37-41`).
        assert_eq!((MAX_REGIONS, NR_NOTE_ENTRIES, MAX_VRI_COUNT), (100, 2, 64));
        assert_eq!((MINIX_ELFCORE_VERSION, CORE_MODE), (1, 0o777));
    }
}
