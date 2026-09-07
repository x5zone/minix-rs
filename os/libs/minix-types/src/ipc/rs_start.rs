//! `rs_start_t` wire face — byte-ABI pinning and decode of C `struct rs_start`.
//!
//! C: `<minix/rs.h>`:104-151. `RS_UP`/`RS_EDIT`/`RS_UPDATE` all open with
//! `copy_rs_start` (request.c:37/:306/:542, manager.c:135-150): the caller
//! copies the *whole struct* (`sys_datacopy`, `sizeof(struct rs_start)`
//! bytes) into RS, then RS separately copies the pointed-to buffers (cmd,
//! script, IPC list, labels — rs.h:63 "Labels are copied over separately").
//!
//! Data model: **x86-64 LP64** — pointers, `long` and `size_t` are 8 bytes;
//! `bitchunk_t`/`uid_t` are fixed-width `uint32_t` (sys/sys/types.h:124/:221)
//! and carry no architecture dependence. The offset table below is asserted
//! against a `#[repr(C)]` witness struct with the same field order and types
//! (`offset_of!`/`size_of`), so a wrong constant or a misread field order
//! fails the test build instead of silently mis-decoding wire bytes.
//!
//! The decoded view keeps pointer fields as raw addresses: this crate does
//! not own the caller's memory, and the separate-copy phase belongs to the
//! request handler (via its safecopy seam).

use crate::Errno;

/// Field offsets within `struct rs_start` (single authority).
///
/// Every constant is compile-time-checked against [`RsStartLayout`] in the
/// test build; `decode_rs_start` reads exclusively through these constants.
pub mod rs_start_off {
    /// `unsigned rss_flags` — rs.h:106.
    pub const FLAGS: usize = 0;
    /// `char *rss_cmd` — rs.h:107 (caller-space address).
    pub const CMD_ADDR: usize = 8;
    /// `size_t rss_cmdlen` — rs.h:108.
    pub const CMD_LEN: usize = 16;
    /// `uid_t rss_uid` — rs.h:109.
    pub const UID: usize = 24;
    /// `endpoint_t rss_sigmgr` — rs.h:110.
    pub const SIGMGR: usize = 28;
    /// `endpoint_t rss_scheduler` — rs.h:111.
    pub const SCHEDULER: usize = 32;
    /// `int rss_priority` — rs.h:112.
    pub const PRIORITY: usize = 36;
    /// `int rss_quantum` — rs.h:113.
    pub const QUANTUM: usize = 40;
    /// `int rss_major` — rs.h:114.
    pub const MAJOR: usize = 44;
    /// `long rss_period` — rs.h:115.
    pub const PERIOD: usize = 48;
    /// `char *rss_script` — rs.h:116 (caller-space address).
    pub const SCRIPT_ADDR: usize = 56;
    /// `size_t rss_scriptlen` — rs.h:117.
    pub const SCRIPT_LEN: usize = 64;
    /// `long rss_asr_count` — rs.h:118.
    pub const ASR_COUNT: usize = 72;
    /// `long rss_restarts` — rs.h:119.
    pub const RESTARTS: usize = 80;
    /// `long rss_heap_prealloc_bytes` — rs.h:120.
    pub const HEAP_PREALLOC_BYTES: usize = 88;
    /// `long rss_map_prealloc_bytes` — rs.h:121.
    pub const MAP_PREALLOC_BYTES: usize = 96;
    /// `int rss_nr_irq` — rs.h:122.
    pub const NR_IRQ: usize = 104;
    /// `int rss_irq[RSS_NR_IRQ]` — rs.h:123 (16 × 4 bytes).
    pub const IRQ: usize = 108;
    /// `int rss_nr_io` — rs.h:124.
    pub const NR_IO: usize = 172;
    /// `struct { unsigned base; unsigned len; } rss_io[RSS_NR_IO]` — rs.h:125
    /// (16 entries × 8 bytes).
    pub const IO: usize = 176;
    /// `int rss_nr_pci_id` — rs.h:126.
    pub const NR_PCI_ID: usize = 304;
    /// `struct rs_pci_id rss_pci_id[RS_NR_PCI_DEVICE]` — rs.h:127
    /// (32 entries × 8 bytes, rs.h:73-78).
    pub const PCI_ID: usize = 308;
    /// `int rss_nr_pci_class` — rs.h:128.
    pub const NR_PCI_CLASS: usize = 564;
    /// `struct rs_pci_class rss_pci_class[RS_NR_PCI_CLASS]` — rs.h:129
    /// (4 entries × 8 bytes, rs.h:82-85).
    pub const PCI_CLASS: usize = 568;
    /// `bitchunk_t rss_system[SYS_CALL_MASK_SIZE]` — rs.h:130 (2 × 4 bytes;
    /// `SYS_CALL_MASK_SIZE = BITMAP_CHUNKS(58) = 2`, com.h:270-272).
    pub const SYSTEM: usize = 600;
    /// `struct rss_label rss_label` — rs.h:131 (rs.h:66-70).
    pub const LABEL_ADDR: usize = 608;
    /// `rss_label.l_len`.
    pub const LABEL_LEN: usize = 616;
    /// `struct rss_label rss_trg_label` — rs.h:132.
    pub const TRG_LABEL_ADDR: usize = 624;
    /// `rss_trg_label.l_len`.
    pub const TRG_LABEL_LEN: usize = 632;
    /// `char *rss_ipc` — rs.h:133 (caller-space address).
    pub const IPC_ADDR: usize = 640;
    /// `size_t rss_ipclen` — rs.h:134.
    pub const IPC_LEN: usize = 648;
    /// `bitchunk_t rss_vm[VM_CALL_MASK_SIZE]` — rs.h:135 (2 × 4 bytes;
    /// `VM_CALL_MASK_SIZE = BITMAP_CHUNKS(49) = 2`, com.h:769-770).
    pub const VM: usize = 656;
    /// `int rss_nr_control` — rs.h:136.
    pub const NR_CONTROL: usize = 664;
    /// `struct rss_label rss_control[RS_NR_CONTROL]` — rs.h:137
    /// (8 entries × 16 bytes; 4-byte pad after `nr_control` for the
    /// 8-byte label alignment).
    pub const CONTROL: usize = 672;
    /// `struct rs_state_data rss_state_data` — rs.h:138 (56 bytes,
    /// rs.h:92-101).
    pub const STATE_DATA: usize = 800;
    /// `int devman_id` — rs.h:139.
    pub const DEVMAN_ID: usize = 856;
    /// `char *rss_progname` — rs.h:140 (caller-space address).
    pub const PROGNAME_ADDR: usize = 864;
    /// `size_t rss_prognamelen` — rs.h:141.
    pub const PROGNAME_LEN: usize = 872;
    /// `int rss_nr_domain` — rs.h:142.
    pub const NR_DOMAIN: usize = 880;
    /// `int rss_domain[NR_DOMAIN]` — rs.h:143 (8 × 4 bytes).
    pub const DOMAIN: usize = 884;
    /// `int rss_cpu` — rs.h:150. SMP tail: C's comment (rs.h:144-149) pins
    /// this field last for binary compatibility with non-SMP builds.
    pub const CPU: usize = 916;
    /// `sizeof(struct rs_start)` — 920 bytes on x86-64 LP64.
    pub const SIZE: usize = 920;

    /// Entry strides inside the arrays.
    /// `rss_io` entry: `{ unsigned base; unsigned len; }` (8 bytes).
    pub const IO_STRIDE: usize = 8;
    /// `rs_pci_id` entry: four `u16_t` (8 bytes).
    pub const PCI_ID_STRIDE: usize = 8;
    /// `rs_pci_class` entry: two `u32_t` (8 bytes).
    pub const PCI_CLASS_STRIDE: usize = 8;
    /// `rss_label` entry: pointer + `size_t` (16 bytes).
    pub const LABEL_STRIDE: usize = 16;
}

/// Wire-shaped decode of one `struct rs_start`.
///
/// Pointer-bearing fields keep their caller-space addresses (`cmd_addr`,
/// `script_addr`, `label.addr`, …) exactly as C stores them; the buffer
/// bytes travel by a separate copy in the request handler. Bitmask arrays
/// (`bitchunk_t[2]`) are combined little-endian into one `u64` — bit *i*
/// of the combined value is call number *i*, matching C's chunk layout
/// (`bitmap.h`: chunk *i/32*, bit *i%32*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsStartWire {
    /// `rss_flags` — raw `RSS_*` bits.
    pub flags: u32,
    /// `rss_cmd` — caller-space address of the command string.
    pub cmd_addr: u64,
    /// `rss_cmdlen`.
    pub cmd_len: u64,
    /// `rss_uid`.
    pub uid: u32,
    /// `rss_sigmgr` — raw endpoint.
    pub sigmgr: i32,
    /// `rss_scheduler` — raw endpoint.
    pub scheduler: i32,
    /// `rss_priority`.
    pub priority: i32,
    /// `rss_quantum`.
    pub quantum: i32,
    /// `rss_major`.
    pub major: i32,
    /// `rss_period`.
    pub period: i64,
    /// `rss_script` — caller-space address.
    pub script_addr: u64,
    /// `rss_scriptlen`.
    pub script_len: u64,
    /// `rss_asr_count`.
    pub asr_count: i64,
    /// `rss_restarts`.
    pub restarts: i64,
    /// `rss_heap_prealloc_bytes` (negative = not requested, request.c:768-770).
    pub heap_prealloc_bytes: i64,
    /// `rss_map_prealloc_bytes`.
    pub map_prealloc_bytes: i64,
    /// `rss_nr_irq` (raw `int`: may carry the `RSS_IRQ_ALL` sentinel = 17).
    pub nr_irq: i32,
    /// `rss_irq`.
    pub irq: [i32; 16],
    /// `rss_nr_io` (raw `int`; `RSS_IO_ALL` sentinel = 17).
    pub nr_io: i32,
    /// `rss_io` — (base, len) pairs.
    pub io: [RsIoRangeWire; 16],
    /// `rss_nr_pci_id`.
    pub nr_pci_id: i32,
    /// `rss_pci_id`.
    pub pci_id: [RsPciIdWire; 32],
    /// `rss_nr_pci_class`.
    pub nr_pci_class: i32,
    /// `rss_pci_class`.
    pub pci_class: [RsPciClassWire; 4],
    /// `rss_system` — kernel-call mask, chunks combined little-endian.
    pub system: u64,
    /// `rss_label` — (address, length) in caller space.
    pub label: RsLabelWire,
    /// `rss_trg_label` — live-update target label.
    pub trg_label: RsLabelWire,
    /// `rss_ipc` — caller-space address of the IPC list.
    pub ipc_addr: u64,
    /// `rss_ipclen`.
    pub ipc_len: u64,
    /// `rss_vm` — VM-call mask, chunks combined little-endian.
    pub vm: u64,
    /// `rss_nr_control` (raw `int`; pre-validation state may be negative).
    pub nr_control: i32,
    /// `rss_control`.
    pub control: [RsLabelWire; 8],
    /// `rss_state_data`.
    pub state_data: RsStateDataWire,
    /// `devman_id`.
    pub devman_id: i32,
    /// `rss_progname` — caller-space address.
    pub progname_addr: u64,
    /// `rss_prognamelen`.
    pub progname_len: u64,
    /// `rss_nr_domain` (raw `int`; negatives rejected at init_slot,
    /// manager.c:1733-1736).
    pub nr_domain: i32,
    /// `rss_domain`.
    pub domain: [i32; 8],
    /// `rss_cpu` — `RS_CPU_DEFAULT`/`RS_CPU_BSP` or a CPU index.
    pub cpu: i32,
}

/// `struct { unsigned base; unsigned len; }` — rs.h:125.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsIoRangeWire {
    /// `base`.
    pub base: u32,
    /// `len`.
    pub len: u32,
}

/// `struct rs_pci_id` — rs.h:73-78. `NO_SUB_VID`/`NO_SUB_DID` = `0xffff`
/// (rs.h:79-80) mean "no subsystem match requested".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsPciIdWire {
    /// `vid`.
    pub vid: u16,
    /// `did`.
    pub did: u16,
    /// `sub_vid`.
    pub sub_vid: u16,
    /// `sub_did`.
    pub sub_did: u16,
}

/// `struct rs_pci_class` — rs.h:82-85.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsPciClassWire {
    /// `pciclass`.
    pub pciclass: u32,
    /// `mask`.
    pub mask: u32,
}

/// `struct rss_label` — rs.h:66-70: a caller-space (address, length) pair;
/// the label bytes are copied over separately ("Labels are copied over
/// separately" — rs.h:63).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsLabelWire {
    /// `l_addr` — caller-space address (0 = unset).
    pub addr: u64,
    /// `l_len`.
    pub len: u64,
}

/// `struct rs_state_data` — rs.h:92-101.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsStateDataWire {
    /// `size` — total state-data size.
    pub size: u64,
    /// `ipcf_els` — caller-space address.
    pub ipcf_els_addr: u64,
    /// `ipcf_els_size`.
    pub ipcf_els_size: u64,
    /// `ipcf_els_gid` — raw grant id.
    pub ipcf_els_gid: i32,
    /// `eval_addr` — caller-space address.
    pub eval_addr: u64,
    /// `eval_len`.
    pub eval_len: u64,
    /// `eval_gid` — raw grant id.
    pub eval_gid: i32,
}

/// Decodes `struct rs_start` bytes into the wire view.
///
/// The buffer must carry the whole struct (`rs_start_off::SIZE` bytes; a
/// larger scratch buffer is accepted — C's `copy_rs_start` copies exactly
/// `sizeof(struct rs_start)` into a same-size local, so a short buffer
/// cannot happen on the C side and is rejected here with `EINVAL`).
pub fn decode_rs_start(bytes: &[u8]) -> Result<RsStartWire, Errno> {
    if bytes.len() < rs_start_off::SIZE {
        return Err(Errno::EINVAL);
    }
    let u32at = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let i32at = |o: usize| i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let u64at = |o: usize| {
        u64::from_le_bytes([
            bytes[o],
            bytes[o + 1],
            bytes[o + 2],
            bytes[o + 3],
            bytes[o + 4],
            bytes[o + 5],
            bytes[o + 6],
            bytes[o + 7],
        ])
    };
    let i64at = |o: usize| u64at(o) as i64;
    let u16at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let label = |o: usize| RsLabelWire {
        addr: u64at(o),
        len: u64at(o + 8),
    };

    let mut irq = [0i32; 16];
    for (i, v) in irq.iter_mut().enumerate() {
        *v = i32at(rs_start_off::IRQ + i * 4);
    }
    let mut io = [RsIoRangeWire { base: 0, len: 0 }; 16];
    for (i, v) in io.iter_mut().enumerate() {
        *v = RsIoRangeWire {
            base: u32at(rs_start_off::IO + i * rs_start_off::IO_STRIDE),
            len: u32at(rs_start_off::IO + i * rs_start_off::IO_STRIDE + 4),
        };
    }
    let mut pci_id = [RsPciIdWire {
        vid: 0,
        did: 0,
        sub_vid: 0,
        sub_did: 0,
    }; 32];
    for (i, v) in pci_id.iter_mut().enumerate() {
        let o = rs_start_off::PCI_ID + i * rs_start_off::PCI_ID_STRIDE;
        *v = RsPciIdWire {
            vid: u16at(o),
            did: u16at(o + 2),
            sub_vid: u16at(o + 4),
            sub_did: u16at(o + 6),
        };
    }
    let mut pci_class = [RsPciClassWire {
        pciclass: 0,
        mask: 0,
    }; 4];
    for (i, v) in pci_class.iter_mut().enumerate() {
        *v = RsPciClassWire {
            pciclass: u32at(rs_start_off::PCI_CLASS + i * rs_start_off::PCI_CLASS_STRIDE),
            mask: u32at(rs_start_off::PCI_CLASS + i * rs_start_off::PCI_CLASS_STRIDE + 4),
        };
    }
    let mut control = [label(0); 8];
    for (i, v) in control.iter_mut().enumerate() {
        *v = label(rs_start_off::CONTROL + i * rs_start_off::LABEL_STRIDE);
    }

    let state_data = RsStateDataWire {
        size: u64at(rs_start_off::STATE_DATA),
        ipcf_els_addr: u64at(rs_start_off::STATE_DATA + 8),
        ipcf_els_size: u64at(rs_start_off::STATE_DATA + 16),
        ipcf_els_gid: i32at(rs_start_off::STATE_DATA + 24),
        eval_addr: u64at(rs_start_off::STATE_DATA + 32),
        eval_len: u64at(rs_start_off::STATE_DATA + 40),
        eval_gid: i32at(rs_start_off::STATE_DATA + 48),
    };

    Ok(RsStartWire {
        flags: u32at(rs_start_off::FLAGS),
        cmd_addr: u64at(rs_start_off::CMD_ADDR),
        cmd_len: u64at(rs_start_off::CMD_LEN),
        uid: u32at(rs_start_off::UID),
        sigmgr: i32at(rs_start_off::SIGMGR),
        scheduler: i32at(rs_start_off::SCHEDULER),
        priority: i32at(rs_start_off::PRIORITY),
        quantum: i32at(rs_start_off::QUANTUM),
        major: i32at(rs_start_off::MAJOR),
        period: i64at(rs_start_off::PERIOD),
        script_addr: u64at(rs_start_off::SCRIPT_ADDR),
        script_len: u64at(rs_start_off::SCRIPT_LEN),
        asr_count: i64at(rs_start_off::ASR_COUNT),
        restarts: i64at(rs_start_off::RESTARTS),
        heap_prealloc_bytes: i64at(rs_start_off::HEAP_PREALLOC_BYTES),
        map_prealloc_bytes: i64at(rs_start_off::MAP_PREALLOC_BYTES),
        nr_irq: i32at(rs_start_off::NR_IRQ),
        irq,
        nr_io: i32at(rs_start_off::NR_IO),
        io,
        nr_pci_id: i32at(rs_start_off::NR_PCI_ID),
        pci_id,
        nr_pci_class: i32at(rs_start_off::NR_PCI_CLASS),
        pci_class,
        system: u64at(rs_start_off::SYSTEM),
        label: label(rs_start_off::LABEL_ADDR),
        trg_label: label(rs_start_off::TRG_LABEL_ADDR),
        ipc_addr: u64at(rs_start_off::IPC_ADDR),
        ipc_len: u64at(rs_start_off::IPC_LEN),
        vm: u64at(rs_start_off::VM),
        nr_control: i32at(rs_start_off::NR_CONTROL),
        control,
        state_data,
        devman_id: i32at(rs_start_off::DEVMAN_ID),
        progname_addr: u64at(rs_start_off::PROGNAME_ADDR),
        progname_len: u64at(rs_start_off::PROGNAME_LEN),
        nr_domain: i32at(rs_start_off::NR_DOMAIN),
        domain: {
            let mut domain = [0i32; 8];
            for (i, v) in domain.iter_mut().enumerate() {
                *v = i32at(rs_start_off::DOMAIN + i * 4);
            }
            domain
        },
        cpu: i32at(rs_start_off::CPU),
    })
}

/// Compile-time layout witness: the C field order and C-equivalent types,
/// nothing else. `repr(C)` gives it GCC's x86-64 ABI layout; the test build
/// asserts every [`rs_start_off`] constant against `offset_of!` of this type,
/// so the offset table can never drift from the ABI-shaped layout it claims
/// to describe. Never instantiated at runtime.
#[repr(C)]
#[cfg(test)]
struct RsStartLayout {
    rss_flags: u32,
    rss_cmd: u64,
    rss_cmdlen: u64,
    rss_uid: u32,
    rss_sigmgr: i32,
    rss_scheduler: i32,
    rss_priority: i32,
    rss_quantum: i32,
    rss_major: i32,
    rss_period: i64,
    rss_script: u64,
    rss_scriptlen: u64,
    rss_asr_count: i64,
    rss_restarts: i64,
    rss_heap_prealloc_bytes: i64,
    rss_map_prealloc_bytes: i64,
    rss_nr_irq: i32,
    rss_irq: [i32; 16],
    rss_nr_io: i32,
    rss_io: [RsStartIoRange; 16],
    rss_nr_pci_id: i32,
    rss_pci_id: [RsStartPciId; 32],
    rss_nr_pci_class: i32,
    rss_pci_class: [RsStartPciClass; 4],
    rss_system: [u32; 2],
    rss_label: RsStartLabel,
    rss_trg_label: RsStartLabel,
    rss_ipc: u64,
    rss_ipclen: u64,
    rss_vm: [u32; 2],
    rss_nr_control: i32,
    rss_control: [RsStartLabel; 8],
    rss_state_data: RsStartStateData,
    devman_id: i32,
    rss_progname: u64,
    rss_prognamelen: u64,
    rss_nr_domain: i32,
    rss_domain: [i32; 8],
    rss_cpu: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RsStartIoRange {
    base: u32,
    len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RsStartPciId {
    vid: u16,
    did: u16,
    sub_vid: u16,
    sub_did: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RsStartPciClass {
    pciclass: u32,
    mask: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RsStartLabel {
    l_addr: u64,
    l_len: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(test)]
struct RsStartStateData {
    size: u64,
    ipcf_els: u64,
    ipcf_els_size: u64,
    ipcf_els_gid: i32,
    eval_addr: u64,
    eval_len: u64,
    eval_gid: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    // Compile-time pinning: every offset constant must equal the compiler's
    // repr(C) layout of the witness. One wrong constant (or one misread C
    // field order) fails here, not on the wire.
    const _: () = assert!(offset_of!(RsStartLayout, rss_flags) == rs_start_off::FLAGS);
    const _: () = assert!(offset_of!(RsStartLayout, rss_cmd) == rs_start_off::CMD_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_cmdlen) == rs_start_off::CMD_LEN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_uid) == rs_start_off::UID);
    const _: () = assert!(offset_of!(RsStartLayout, rss_sigmgr) == rs_start_off::SIGMGR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_scheduler) == rs_start_off::SCHEDULER);
    const _: () = assert!(offset_of!(RsStartLayout, rss_priority) == rs_start_off::PRIORITY);
    const _: () = assert!(offset_of!(RsStartLayout, rss_quantum) == rs_start_off::QUANTUM);
    const _: () = assert!(offset_of!(RsStartLayout, rss_major) == rs_start_off::MAJOR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_period) == rs_start_off::PERIOD);
    const _: () = assert!(offset_of!(RsStartLayout, rss_script) == rs_start_off::SCRIPT_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_scriptlen) == rs_start_off::SCRIPT_LEN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_asr_count) == rs_start_off::ASR_COUNT);
    const _: () = assert!(offset_of!(RsStartLayout, rss_restarts) == rs_start_off::RESTARTS);
    const _: () = assert!(
        offset_of!(RsStartLayout, rss_heap_prealloc_bytes) == rs_start_off::HEAP_PREALLOC_BYTES
    );
    const _: () = assert!(
        offset_of!(RsStartLayout, rss_map_prealloc_bytes) == rs_start_off::MAP_PREALLOC_BYTES
    );
    const _: () = assert!(offset_of!(RsStartLayout, rss_nr_irq) == rs_start_off::NR_IRQ);
    const _: () = assert!(offset_of!(RsStartLayout, rss_irq) == rs_start_off::IRQ);
    const _: () = assert!(offset_of!(RsStartLayout, rss_nr_io) == rs_start_off::NR_IO);
    const _: () = assert!(offset_of!(RsStartLayout, rss_io) == rs_start_off::IO);
    const _: () = assert!(offset_of!(RsStartLayout, rss_nr_pci_id) == rs_start_off::NR_PCI_ID);
    const _: () = assert!(offset_of!(RsStartLayout, rss_pci_id) == rs_start_off::PCI_ID);
    const _: () =
        assert!(offset_of!(RsStartLayout, rss_nr_pci_class) == rs_start_off::NR_PCI_CLASS);
    const _: () = assert!(offset_of!(RsStartLayout, rss_pci_class) == rs_start_off::PCI_CLASS);
    const _: () = assert!(offset_of!(RsStartLayout, rss_system) == rs_start_off::SYSTEM);
    const _: () = assert!(offset_of!(RsStartLayout, rss_label) == rs_start_off::LABEL_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_trg_label) == rs_start_off::TRG_LABEL_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_ipc) == rs_start_off::IPC_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_ipclen) == rs_start_off::IPC_LEN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_vm) == rs_start_off::VM);
    const _: () = assert!(offset_of!(RsStartLayout, rss_nr_control) == rs_start_off::NR_CONTROL);
    const _: () = assert!(offset_of!(RsStartLayout, rss_control) == rs_start_off::CONTROL);
    const _: () = assert!(offset_of!(RsStartLayout, rss_state_data) == rs_start_off::STATE_DATA);
    const _: () = assert!(offset_of!(RsStartLayout, devman_id) == rs_start_off::DEVMAN_ID);
    const _: () = assert!(offset_of!(RsStartLayout, rss_progname) == rs_start_off::PROGNAME_ADDR);
    const _: () = assert!(offset_of!(RsStartLayout, rss_prognamelen) == rs_start_off::PROGNAME_LEN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_nr_domain) == rs_start_off::NR_DOMAIN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_domain) == rs_start_off::DOMAIN);
    const _: () = assert!(offset_of!(RsStartLayout, rss_cpu) == rs_start_off::CPU);
    const _: () = assert!(size_of::<RsStartLayout>() == rs_start_off::SIZE);

    /// Serializes the witness struct field-by-field at its own `offset_of!`
    /// positions — the same bytes GCC would produce for the equivalent C
    /// initializer — so the decoder is exercised against the ABI layout
    /// itself, not against a hand-assembled second copy of the offsets.
    fn serialize(layout: &RsStartLayout) -> [u8; rs_start_off::SIZE] {
        fn put32(buf: &mut [u8], off: usize, v: u32) {
            buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        fn put64(buf: &mut [u8], off: usize, v: u64) {
            buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
        }
        let mut buf = [0u8; rs_start_off::SIZE];
        put32(&mut buf, rs_start_off::FLAGS, layout.rss_flags);
        put64(&mut buf, rs_start_off::CMD_ADDR, layout.rss_cmd);
        put64(&mut buf, rs_start_off::CMD_LEN, layout.rss_cmdlen);
        put32(&mut buf, rs_start_off::UID, layout.rss_uid);
        put32(&mut buf, rs_start_off::SIGMGR, layout.rss_sigmgr as u32);
        put32(
            &mut buf,
            rs_start_off::SCHEDULER,
            layout.rss_scheduler as u32,
        );
        put32(&mut buf, rs_start_off::PRIORITY, layout.rss_priority as u32);
        put32(&mut buf, rs_start_off::QUANTUM, layout.rss_quantum as u32);
        put32(&mut buf, rs_start_off::MAJOR, layout.rss_major as u32);
        put64(&mut buf, rs_start_off::PERIOD, layout.rss_period as u64);
        put64(&mut buf, rs_start_off::SCRIPT_ADDR, layout.rss_script);
        put64(&mut buf, rs_start_off::SCRIPT_LEN, layout.rss_scriptlen);
        put64(
            &mut buf,
            rs_start_off::ASR_COUNT,
            layout.rss_asr_count as u64,
        );
        put64(&mut buf, rs_start_off::RESTARTS, layout.rss_restarts as u64);
        put64(
            &mut buf,
            rs_start_off::HEAP_PREALLOC_BYTES,
            layout.rss_heap_prealloc_bytes as u64,
        );
        put64(
            &mut buf,
            rs_start_off::MAP_PREALLOC_BYTES,
            layout.rss_map_prealloc_bytes as u64,
        );
        put32(&mut buf, rs_start_off::NR_IRQ, layout.rss_nr_irq as u32);
        for (i, v) in layout.rss_irq.iter().enumerate() {
            put32(&mut buf, rs_start_off::IRQ + i * 4, *v as u32);
        }
        put32(&mut buf, rs_start_off::NR_IO, layout.rss_nr_io as u32);
        for (i, e) in layout.rss_io.iter().enumerate() {
            put32(&mut buf, rs_start_off::IO + i * 8, e.base);
            put32(&mut buf, rs_start_off::IO + i * 8 + 4, e.len);
        }
        put32(
            &mut buf,
            rs_start_off::NR_PCI_ID,
            layout.rss_nr_pci_id as u32,
        );
        for (i, e) in layout.rss_pci_id.iter().enumerate() {
            let o = rs_start_off::PCI_ID + i * 8;
            put16(&mut buf, o, e.vid);
            put16(&mut buf, o + 2, e.did);
            put16(&mut buf, o + 4, e.sub_vid);
            put16(&mut buf, o + 6, e.sub_did);
        }
        put32(
            &mut buf,
            rs_start_off::NR_PCI_CLASS,
            layout.rss_nr_pci_class as u32,
        );
        for (i, e) in layout.rss_pci_class.iter().enumerate() {
            put32(&mut buf, rs_start_off::PCI_CLASS + i * 8, e.pciclass);
            put32(&mut buf, rs_start_off::PCI_CLASS + i * 8 + 4, e.mask);
        }
        for (i, chunk) in layout.rss_system.iter().enumerate() {
            put32(&mut buf, rs_start_off::SYSTEM + i * 4, *chunk);
        }
        put_label(&mut buf, rs_start_off::LABEL_ADDR, &layout.rss_label);
        put_label(
            &mut buf,
            rs_start_off::TRG_LABEL_ADDR,
            &layout.rss_trg_label,
        );
        put64(&mut buf, rs_start_off::IPC_ADDR, layout.rss_ipc);
        put64(&mut buf, rs_start_off::IPC_LEN, layout.rss_ipclen);
        for (i, chunk) in layout.rss_vm.iter().enumerate() {
            put32(&mut buf, rs_start_off::VM + i * 4, *chunk);
        }
        put32(
            &mut buf,
            rs_start_off::NR_CONTROL,
            layout.rss_nr_control as u32,
        );
        for (i, e) in layout.rss_control.iter().enumerate() {
            put_label(&mut buf, rs_start_off::CONTROL + i * 16, e);
        }
        let sd = &layout.rss_state_data;
        put64(&mut buf, rs_start_off::STATE_DATA, sd.size);
        put64(&mut buf, rs_start_off::STATE_DATA + 8, sd.ipcf_els);
        put64(&mut buf, rs_start_off::STATE_DATA + 16, sd.ipcf_els_size);
        put32(
            &mut buf,
            rs_start_off::STATE_DATA + 24,
            sd.ipcf_els_gid as u32,
        );
        put64(&mut buf, rs_start_off::STATE_DATA + 32, sd.eval_addr);
        put64(&mut buf, rs_start_off::STATE_DATA + 40, sd.eval_len);
        put32(&mut buf, rs_start_off::STATE_DATA + 48, sd.eval_gid as u32);
        put32(&mut buf, rs_start_off::DEVMAN_ID, layout.devman_id as u32);
        put64(&mut buf, rs_start_off::PROGNAME_ADDR, layout.rss_progname);
        put64(&mut buf, rs_start_off::PROGNAME_LEN, layout.rss_prognamelen);
        put32(
            &mut buf,
            rs_start_off::NR_DOMAIN,
            layout.rss_nr_domain as u32,
        );
        for (i, v) in layout.rss_domain.iter().enumerate() {
            put32(&mut buf, rs_start_off::DOMAIN + i * 4, *v as u32);
        }
        put32(&mut buf, rs_start_off::CPU, layout.rss_cpu as u32);
        return buf;

        fn put16(buf: &mut [u8], off: usize, v: u16) {
            buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
        }
        fn put_label(buf: &mut [u8], off: usize, l: &RsStartLabel) {
            put64(buf, off, l.l_addr);
            put64(buf, off + 8, l.l_len);
        }
    }

    fn sample_layout() -> RsStartLayout {
        RsStartLayout {
            rss_flags: 0x0000_0007,
            rss_cmd: 0x0000_7fff_1234_5678,
            rss_cmdlen: 25,
            rss_uid: 0,
            rss_sigmgr: 0x1234_5678,
            rss_scheduler: -2,
            rss_priority: 8,
            rss_quantum: 200,
            rss_major: 0,
            rss_period: -1,
            rss_script: 0x0000_7fff_aaaa_0000,
            rss_scriptlen: 64,
            rss_asr_count: 3,
            rss_restarts: 1,
            rss_heap_prealloc_bytes: -1,
            rss_map_prealloc_bytes: 1 << 20,
            rss_nr_irq: 2,
            rss_irq: {
                let mut a = [0; 16];
                a[0] = 4;
                a[1] = -1;
                a
            },
            rss_nr_io: 1,
            rss_io: {
                let mut a = [RsStartIoRange { base: 0, len: 0 }; 16];
                a[0] = RsStartIoRange {
                    base: 0x3f8,
                    len: 8,
                };
                a
            },
            rss_nr_pci_id: 1,
            rss_pci_id: {
                let mut a = [RsStartPciId {
                    vid: 0,
                    did: 0,
                    sub_vid: 0xffff,
                    sub_did: 0xffff,
                }; 32];
                a[0] = RsStartPciId {
                    vid: 0x8086,
                    did: 0x100e,
                    sub_vid: 0xffff,
                    sub_did: 0xffff,
                };
                a
            },
            rss_nr_pci_class: 1,
            rss_pci_class: {
                let mut a = [RsStartPciClass {
                    pciclass: 0,
                    mask: 0,
                }; 4];
                a[0] = RsStartPciClass {
                    pciclass: 0x0200_0000,
                    mask: 0xffff_0000,
                };
                a
            },
            rss_system: [0xdead_beef, 0x0000_00ff],
            rss_label: RsStartLabel {
                l_addr: 0x0000_7fff_bbbb_0000,
                l_len: 6,
            },
            rss_trg_label: RsStartLabel {
                l_addr: 0,
                l_len: 0,
            },
            rss_ipc: 0x0000_7fff_cccc_0000,
            rss_ipclen: 42,
            rss_vm: [0x1, 0x8000_0000],
            rss_nr_control: 2,
            rss_control: {
                let mut a = [RsStartLabel {
                    l_addr: 0,
                    l_len: 0,
                }; 8];
                a[0] = RsStartLabel {
                    l_addr: 0x1000,
                    l_len: 5,
                };
                a[1] = RsStartLabel {
                    l_addr: 0x2000,
                    l_len: 7,
                };
                a
            },
            rss_state_data: RsStartStateData {
                size: 256,
                ipcf_els: 0x0000_7fff_dddd_0000,
                ipcf_els_size: 128,
                ipcf_els_gid: 7,
                eval_addr: 0x0000_7fff_eeee_0000,
                eval_len: 32,
                eval_gid: -1,
            },
            devman_id: 5,
            rss_progname: 0x0000_7fff_ffff_0000,
            rss_prognamelen: 9,
            rss_nr_domain: 1,
            rss_domain: {
                let mut a = [0; 8];
                a[0] = 2;
                a
            },
            rss_cpu: -1,
        }
    }

    #[test]
    fn test_decode_rs_start_roundtrip_against_abi_layout() {
        // The layout witness is serialized at its own offset_of positions and
        // decoded back: every field lands where the compiler would have put
        // it, so a decode offset can only be right if it matches the ABI.
        let layout = sample_layout();
        let bytes = serialize(&layout);
        assert_eq!(bytes.len(), rs_start_off::SIZE);
        let w = decode_rs_start(&bytes).expect("decode");
        assert_eq!(w.flags, layout.rss_flags);
        assert_eq!(w.cmd_addr, layout.rss_cmd);
        assert_eq!(w.cmd_len, layout.rss_cmdlen);
        assert_eq!(w.uid, layout.rss_uid);
        assert_eq!(w.sigmgr, layout.rss_sigmgr);
        assert_eq!(w.scheduler, layout.rss_scheduler);
        assert_eq!(w.priority, layout.rss_priority);
        assert_eq!(w.quantum, layout.rss_quantum);
        assert_eq!(w.major, layout.rss_major);
        assert_eq!(w.period, layout.rss_period);
        assert_eq!(w.script_addr, layout.rss_script);
        assert_eq!(w.script_len, layout.rss_scriptlen);
        assert_eq!(w.asr_count, layout.rss_asr_count);
        assert_eq!(w.restarts, layout.rss_restarts);
        assert_eq!(w.heap_prealloc_bytes, layout.rss_heap_prealloc_bytes);
        assert_eq!(w.map_prealloc_bytes, layout.rss_map_prealloc_bytes);
        assert_eq!(w.nr_irq, layout.rss_nr_irq);
        assert_eq!(w.irq, layout.rss_irq);
        assert_eq!(w.nr_io, layout.rss_nr_io);
        for i in 0..16 {
            assert_eq!(w.io[i].base, layout.rss_io[i].base);
            assert_eq!(w.io[i].len, layout.rss_io[i].len);
        }
        assert_eq!(w.nr_pci_id, layout.rss_nr_pci_id);
        for i in 0..32 {
            assert_eq!(w.pci_id[i].vid, layout.rss_pci_id[i].vid);
            assert_eq!(w.pci_id[i].did, layout.rss_pci_id[i].did);
            assert_eq!(w.pci_id[i].sub_vid, layout.rss_pci_id[i].sub_vid);
            assert_eq!(w.pci_id[i].sub_did, layout.rss_pci_id[i].sub_did);
        }
        assert_eq!(w.nr_pci_class, layout.rss_nr_pci_class);
        for i in 0..4 {
            assert_eq!(w.pci_class[i].pciclass, layout.rss_pci_class[i].pciclass);
            assert_eq!(w.pci_class[i].mask, layout.rss_pci_class[i].mask);
        }
        // bitchunk_t[2] little-endian chunk combine: bit i of the u64 is
        // call i (chunk i/32, bit i%32 — bitmap.h).
        assert_eq!(w.system, 0x0000_00ff_dead_beefu64);
        assert_eq!(w.vm, 0x8000_0000_0000_0001);
        assert_eq!(w.label.addr, layout.rss_label.l_addr);
        assert_eq!(w.label.len, layout.rss_label.l_len);
        assert_eq!(w.trg_label.addr, 0);
        assert_eq!(w.trg_label.len, 0);
        assert_eq!(w.ipc_addr, layout.rss_ipc);
        assert_eq!(w.ipc_len, layout.rss_ipclen);
        assert_eq!(w.nr_control, layout.rss_nr_control);
        for i in 0..8 {
            assert_eq!(w.control[i].addr, layout.rss_control[i].l_addr);
            assert_eq!(w.control[i].len, layout.rss_control[i].l_len);
        }
        assert_eq!(w.state_data.size, 256);
        assert_eq!(w.state_data.ipcf_els_addr, layout.rss_state_data.ipcf_els);
        assert_eq!(w.state_data.ipcf_els_size, 128);
        assert_eq!(w.state_data.ipcf_els_gid, 7);
        assert_eq!(w.state_data.eval_addr, layout.rss_state_data.eval_addr);
        assert_eq!(w.state_data.eval_len, 32);
        assert_eq!(w.state_data.eval_gid, -1);
        assert_eq!(w.devman_id, layout.devman_id);
        assert_eq!(w.progname_addr, layout.rss_progname);
        assert_eq!(w.progname_len, layout.rss_prognamelen);
        assert_eq!(w.nr_domain, layout.rss_nr_domain);
        assert_eq!(w.domain, layout.rss_domain);
        assert_eq!(w.cpu, layout.rss_cpu);
    }

    #[test]
    fn test_decode_rs_start_short_buffer_rejected() {
        // C's copy_rs_start copies exactly sizeof(struct rs_start); a short
        // buffer cannot come from a completed sys_datacopy and is EINVAL.
        let bytes = [0u8; rs_start_off::SIZE - 1];
        assert_eq!(decode_rs_start(&bytes), Err(Errno::EINVAL));
        assert_eq!(decode_rs_start(&[]), Err(Errno::EINVAL));
    }

    #[test]
    fn test_decode_rs_start_accepts_larger_scratch() {
        // Callers may reuse a larger scratch buffer; the leading SIZE bytes
        // are the struct.
        let mut bytes = [0u8; rs_start_off::SIZE + 16];
        bytes[rs_start_off::QUANTUM..rs_start_off::QUANTUM + 4]
            .copy_from_slice(&200i32.to_le_bytes());
        let w = decode_rs_start(&bytes).expect("decode");
        assert_eq!(w.quantum, 200);
    }
}
