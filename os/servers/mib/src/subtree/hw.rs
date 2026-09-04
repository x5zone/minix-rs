//! CTL_HW subtree: strings, constants, and widening math.
//!
//! Mirrors `hw.c` (all 140 lines): three function handlers, ten
//! populated slots of sixteen, and arch-dependent machine strings.
//! VM statistics reads are transport effects (A-12); the widen/narrow
//! math and the table shape are judged here.
//!
//! 14-mib-subtree-vm-hw.md.

use minix_types::{
    HW_ALIGNBYTES, HW_BYTEORDER, HW_CNMAGIC, HW_DISKNAMES, HW_IOSTATNAMES, HW_IOSTATS, HW_MACHINE,
    HW_MACHINE_ARCH, HW_MODEL, HW_NCPU, HW_NCPUONLINE, HW_PAGESIZE, HW_PHYSMEM, HW_PHYSMEM64,
    HW_USERMEM, HW_USERMEM64,
};

/// Handler behind an hw function node.
///
/// C: `mib_hw_physmem` (hw.c:18-40), `mib_hw_usermem` (:45-76),
/// `mib_hw_ncpuonline` (:81-95). physmem/usermem each serve *two* nodes
/// (32-bit narrowing + 64-bit wide) off one body, branched on
/// `node_size` — one function, two doors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwFunc {
    /// Physical memory, both widths. C: `mib_hw_physmem`.
    Physmem,
    /// Non-kernel memory, both widths. C: `mib_hw_usermem`.
    Usermem,
    /// Online CPU count. C: `mib_hw_ncpuonline`.
    Ncpuonline,
}

/// Shape of one populated hw slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwKind {
    /// Constant string leaf. C: `MIB_STRING(_P | _RO, mach/arch, ...)`.
    ConstStr(&'static str),
    /// Constant integer leaf. C: `MIB_INT(_P | _RO, value, ...)`.
    ConstInt(i32),
    /// Constant integer leaf backed by a build macro (cf. kern `BuildInt`).
    /// C: `ncpu` is `CONFIG_MAX_CPUS` (mib.h:13-15, default 1).
    BuildInt(&'static str),
    /// Function node. C: `MIB_FUNC(...)`.
    Func(HwFunc),
}

/// One populated hw slot: id, name, shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwEntry {
    /// Slot id (`HW_*`). C: table index.
    pub id: i32,
    /// Node name. C: `node_name`.
    pub name: &'static str,
    /// Node shape. C: macro + handler.
    pub kind: HwKind,
}

/// Machine class string (ARCH: 64-bit successor to C's arch table).
///
/// C: `mach`/`arch` are `"i386"`/`"evbarm"` with `#error` otherwise
/// (hw.c:5-13) — minix-rs is 64-bit, so the table gains a row rather
/// than a branch: same shape, new strings.
pub const MACH: &str = "x86_64";
/// Machine CPU class string (ARCH: see [`MACH`]).
pub const MACHINE_ARCH: &str = "x86_64";

/// The populated hw slots, id-sorted (10 of 16).
///
/// C: `mib_hw_table[]` — hw.c:98-130. `ncpu` is the *configured* count
/// (`CONFIG_MAX_CPUS`), `ncpuonline` the *measured* one (:92) — two
/// different facts, two different nodes.
pub const HW_ENTRIES: &[HwEntry] = &[
    HwEntry {
        id: HW_MACHINE,
        name: "machine",
        kind: HwKind::ConstStr(MACH),
    },
    HwEntry {
        id: HW_NCPU,
        name: "ncpu",
        kind: HwKind::BuildInt("CONFIG_MAX_CPUS"),
    },
    HwEntry {
        id: HW_BYTEORDER,
        name: "byteorder",
        kind: HwKind::ConstInt(1234),
    },
    HwEntry {
        id: HW_PHYSMEM,
        name: "physmem",
        kind: HwKind::Func(HwFunc::Physmem),
    },
    HwEntry {
        id: HW_USERMEM,
        name: "usermem",
        kind: HwKind::Func(HwFunc::Usermem),
    },
    HwEntry {
        id: HW_PAGESIZE,
        name: "pagesize",
        kind: HwKind::ConstInt(4096),
    },
    HwEntry {
        id: HW_MACHINE_ARCH,
        name: "machine_arch",
        kind: HwKind::ConstStr(MACHINE_ARCH),
    },
    HwEntry {
        id: HW_PHYSMEM64,
        name: "physmem64",
        kind: HwKind::Func(HwFunc::Physmem),
    },
    HwEntry {
        id: HW_USERMEM64,
        name: "usermem64",
        kind: HwKind::Func(HwFunc::Usermem),
    },
    HwEntry {
        id: HW_NCPUONLINE,
        name: "ncpuonline",
        kind: HwKind::Func(HwFunc::Ncpuonline),
    },
];

/// Unpopulated hw slots (A-9, 6 of 16).
///
/// C: the `/* ... not yet supported */` rows — hw.c:102,114-115,
/// 119,121,129.
pub const HW_UNIMPLEMENTED: &[i32] = &[
    HW_MODEL,
    HW_DISKNAMES,
    HW_IOSTATS,
    HW_ALIGNBYTES,
    HW_CNMAGIC,
    HW_IOSTATNAMES,
];

/// Find a populated entry by id.
pub fn find_entry(id: i32) -> Option<&'static HwEntry> {
    HW_ENTRIES.iter().find(|e| e.id == id)
}

/// Widen pages to bytes: `total * pagesize` in 64 bits.
///
/// C: `(u_quad_t)vsi_total * vsi_pagesize` — hw.c:29,57. The cast-then-
/// multiply order matters: 32-bit multiply first would wrap past 4 GiB
/// of pages before widening.
pub const fn physmem_bytes(total_pages: u64, pagesize: u64) -> u64 {
    total_pages * pagesize
}

/// Subtract kernel usage, saturating at zero.
///
/// C: `if (usermem64 >= vui_total) -= else = 0` — hw.c:62-65. Kernel
/// accounting bigger than the total is corrupt data, not negative
/// memory — saturate, don't wrap.
pub const fn usermem_minus_kernel(total: u64, kernel: u64) -> u64 {
    total.saturating_sub(kernel)
}

/// Narrow to 32 bits, clamping at `UINT_MAX`.
///
/// C: `if (x64 > UINT_MAX) = UINT_MAX else (unsigned)` — hw.c:32-35,
/// :68-71. Which door (32/64-bit node) serves is branched on
/// `node_size == sizeof(int)` (:31, :67) — same body, two widths.
pub const fn clamp_u32(v: u64) -> u32 {
    if v > u32::MAX as u64 {
        u32::MAX
    } else {
        v as u32
    }
}

/// Whether this node takes the 32-bit door (`node_size == sizeof(int)`).
/// C: hw.c:31,67.
pub const fn is_narrow_door(node_size: u64) -> bool {
    node_size == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_shape() {
        // 10 populated of 16 (hw.c:98-130); sorted; disjoint from A-9.
        assert_eq!(HW_ENTRIES.len(), 10);
        assert_eq!(HW_UNIMPLEMENTED.len(), 6);
        assert_eq!(HW_ENTRIES.len() + HW_UNIMPLEMENTED.len(), 16);
        let mut prev = 0;
        for e in HW_ENTRIES {
            assert!(e.id > prev);
            assert!(!HW_UNIMPLEMENTED.contains(&e.id));
            prev = e.id;
        }
        assert_eq!(find_entry(HW_MACHINE).unwrap().name, "machine");
        assert_eq!(
            find_entry(HW_PHYSMEM).unwrap().kind,
            find_entry(HW_PHYSMEM64).unwrap().kind
        );
        assert_eq!(find_entry(2), None);
    }

    #[test]
    fn test_mem_math() {
        // Widen-then-multiply (hw.c:29,57).
        assert_eq!(physmem_bytes(1 << 20, 4096), 1 << 32);
        // Saturate, don't wrap (:62-65).
        assert_eq!(usermem_minus_kernel(100, 30), 70);
        assert_eq!(usermem_minus_kernel(30, 100), 0);
        // Clamp at UINT_MAX (:32-35).
        assert_eq!(clamp_u32(u64::from(u32::MAX)), u32::MAX);
        assert_eq!(clamp_u32(u64::from(u32::MAX) + 1), u32::MAX);
        assert_eq!(clamp_u32(42), 42);
        // Door by node_size (:31, :67).
        assert!(is_narrow_door(4));
        assert!(!is_narrow_door(8));
    }
}
