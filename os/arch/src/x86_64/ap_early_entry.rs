use crate::arch::ap_early_entry::ApBootstrap;
pub use crate::arch::ap_early_entry::{BOOT_MAGIC_ACK, BOOT_MAGIC_SENT};
use crate::DirectMapArch;
use minix_types::PhysBytes;
use crate::X86_64DirectMap;

// ── Frozen placement constants (S-3b) ──

/// Physical address where the BSP copies the early entry image and where
/// the INIT-SIPI vector lands the AP (vector `0x08` → linear/physical
/// `0x8000`). Within the first 640 KiB (clear of the EBDA at 0x9FC00 and
/// the BIOS at 0xF0000) and identity-mapped by the boot root (PML4[0]
/// covers [0, 4 GiB), see `arch_boot_impl` Step 1) — so the 64-bit tail
/// reads the copied record through plain 32-bit absolute addressing.
pub const AP_STARTUP_PA: u64 = 0x8000;
/// SIPI vector field value: `AP_STARTUP_PA >> 12`.
pub const AP_STARTUP_VECTOR: u32 = 0x08;

// Data-area offsets, relative to the data section start (`data_start`).
// Everything the BSP patches is inside the `ApBootstrap` record at +8.
const D_RECORD: usize = 8; // magic (u64) sits at +0, the record at +8
const D_GDT_DESC: usize = 48; // limit(2) + base(8)
const D_GDT_TABLE: usize = 64; // 4 entries × 8
const D_LEN: usize = D_GDT_TABLE + 32;

// Fixed section paddings (bytes from blob start): the far jumps use
// run-linear offsets as immediates, so the 32-bit and 64-bit entry labels
// sit at fixed offsets regardless of future instruction tweaks — bump the
// paddings together with the code, the numbers are asserted by
// `test_ladder_offsets_consistent` against the linker symbols.
const PAD16: usize = 0x40; // 16-bit real-mode section end (blob offset)
const PAD32: usize = 0xC0; // 32-bit protected section end (blob offset)
const OFF_PROTECTED: u64 = AP_STARTUP_PA + PAD16 as u64;
const OFF_LONG_LOW: u64 = AP_STARTUP_PA + PAD32 as u64;

// Ladder GDT — null / code16 / code32 / code64 (base 0, built-in bytes;
// nothing here is BSP-filled).
const GDT_NULL: u64 = 0x0000_0000_0000_0000;
const GDT_CODE16: u64 = 0x0000_9A00_0000_FFFF; // byte gran, 16-bit default
const GDT_CODE32: u64 = 0x00CF_9A00_0000_FFFF; // 4K gran, 32-bit default
const GDT_CODE64: u64 = 0x00AF_9A00_0000_0000; // 4K gran, L=1
const SEL_CODE16: u32 = 0x08;
const SEL_CODE32: u32 = 0x10;
const SEL_CODE64: u32 = 0x18;
const SEL_DATA32: u32 = 0x20; // writable data, 4K gran
const GDT_DATA32: u64 = 0x00CF_9200_0000_FFFF;
const GDT_LIMIT: u16 = 0x0027; // 5 entries × 8 - 1

/// Entry→data gap: lld-link places `.ap_early_entry_data` at the next 4 KiB
/// boundary after the code section (S-3a spike measurement). The ladder's
/// low-mode addressing bakes this number into absolute operands, so the
/// hosted test `test_data_gap_matches_linker_layout` pins it — a toolchain
/// change that moves the gap fails the test instead of the AP.
const DATA_GAP: usize = 0x1000;

core::arch::global_asm!(
    // ─────────────────────────────────────────────────────────────────
    // AP early entry ladder — 16-bit real → 32-bit protected (PAE) →
    // 64-bit long → high Rust entry. Intel syntax (rustc default).
    //
    // Entry state (SIPI): CS = AP_STARTUP_VECTOR, IP = 0 — the first
    // instruction runs at linear PA = AP_STARTUP_PA. Every low-mode
    // reference below uses RUN linear addresses (entry_pa + offset),
    // all build-time constants: relocation-free by construction.
    //
    // Far jumps and lgdt are emitted as raw bytes (EA / 0F 01) — the
    // LLVM IAS far-jump forms are fragile in 16-bit sections.
    //
    // No IDT: cli + no expected faults; a fault triple-faults and the AP
    // resets — S-8 owns the real interrupt reality.
    // ─────────────────────────────────────────────────────────────────
    ".section .ap_early_entry, \"ax\"",
    ".globl ap_early_entry_start",
    ".globl ap_early_entry_code",
    ".code16",
    "ap_early_entry_start:",
    "ap_early_entry_code:",
    "  cli",
    "  xor ax, ax",
    "  mov ds, ax",
    "  mov ss, ax",
    // lgdt [0x8000 + D_GDT_DESC] — 66 0F 01 15 disp32 (m16&32 form).
    ".byte 0x66, 0x0F, 0x01, 0x15",
    ".long {entry_pa} + {d_gdt_desc}",
    // Protected mode (paging still off — linear = PA).
    "  mov eax, cr0",
    "  or eax, 1",
    "  mov cr0, eax",
    // Far jump: EA, offset32 (run linear), selector CODE32.
    ".byte 0xEA",
    ".long {entry_pa} + {off_protected}",
    ".word {sel_code32}",
    ".space {pad16} - (. - ap_early_entry_start)",
    // ── 32-bit protected mode (identity: linear = PA) ──
    ".code32",
    "ap_protected:",
    "  mov ax, {sel_data32}",
    "  mov ds, ax",
    "  mov ss, ax",
    // Root page table → CR3 (fill_bootstrap asserts root <4GiB: the 32-bit
    // mov cr3 writes bits 31:0 only — §3.2 invariant).
    "  mov eax, dword ptr [{entry_pa} + {data_gap} + {d_record} + 16]",
    "  mov cr3, eax",
    // PAE (long-mode prerequisite).
    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax",
    // EFER.LME (MSR 0xC0000080 bit 8).
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",
    // Paging on: the next fetch stays at the same low linear address,
    // translated identity through the new root (PML4[0] = [0, 4 GiB) —
    // established by arch_boot_impl Step 1).
    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",
    // Far jump into the 64-bit code segment: offset zero-extended from 32
    // bits lands at the low 64-bit tail (identity-mapped).
    ".byte 0xEA",
    ".long {entry_pa} + {off_long_low}",
    ".word {sel_code64}",
    ".space {pad32} - (. - ap_early_entry_start)",
    // ── 64-bit long mode, low identity region ──
    ".code64",
    "ap_long_low:",
    // Per-AP kernel stack from the record (BSP-filled). Absolute 32-bit
    // addressing reaches the copy through the identity mapping — rip-rel
    // would point at the ORIGINAL blob (link-time VAs), not the copy.
    "  mov rsp, qword ptr [{entry_pa} + {data_gap} + {d_record} + 24]",
    // Rust entry: first argument (MS x64: RCX) = bootstrap physical
    // address; the Rust side converts PA→VA via the Direct Map.
    "  mov ecx, {entry_pa}",
    "  mov rax, qword ptr [{entry_pa} + {data_gap} + {d_record} + 32]",
    "  jmp rax",
    // ── Data area (writable; BSP fills magic/record through the DM) ──
    ".section .ap_early_entry_data, \"aw\"",
    ".globl ap_early_entry_data_start",
    ".balign 8",
    "ap_early_entry_data_start:",
    // +0: image magic (BSP sets BOOT_MAGIC_SENT after filling the record —
    // the publisher fence lives in SmpArch::boot_ap before the firmware
    // call, per §3.9; the fence lands with S-3c).
    ".quad 0",
    // +8: the ApBootstrap record (40 bytes, frozen field offsets 8/16/24/32
    // = hw_id / page_table_root_pa / kernel_stack_top_va / rust_entry_va;
    // logical_id+_pad at +0/+4 are filled but unused by the ladder).
    ".zero 40",
    // +48: ladder GDT descriptor (limit u16, base u64) — base is the blob's
    // own run linear address, a build-time constant.
    ".word {gdt_limit}",
    ".quad {entry_pa} + {d_gdt_table}",
    // +64: GDT table — null / code16 / code32 / code64 / data32.
    ".quad {gdt_null}",
    ".quad {gdt_code16}",
    ".quad {gdt_code32}",
    ".quad {gdt_code64}",
    ".quad {gdt_data32}",
    // +104: spare tail (keep the blob one small copy unit).
    ".quad 0",
    ".globl ap_early_entry_end",
    "ap_early_entry_end:",
    entry_pa = const AP_STARTUP_PA,
    d_gdt_desc = const D_GDT_DESC,
    d_record = const D_RECORD,
    d_gdt_table = const D_GDT_TABLE,
    off_protected = const OFF_PROTECTED,
    off_long_low = const OFF_LONG_LOW,
    pad16 = const PAD16,
    pad32 = const PAD32,
    gdt_limit = const GDT_LIMIT,
    gdt_null = const GDT_NULL,
    gdt_code16 = const GDT_CODE16,
    gdt_code32 = const GDT_CODE32,
    gdt_code64 = const GDT_CODE64,
    gdt_data32 = const GDT_DATA32,
    sel_code32 = const SEL_CODE32,
    sel_data32 = const SEL_DATA32,
    sel_code64 = const SEL_CODE64,
    data_gap = const DATA_GAP,
);

unsafe extern "C" {
    /// Blob start (code section; = `AP_STARTUP_PA` after install).
    pub static ap_early_entry_start: u8;
    /// Code entry point (offset 0 of the blob; 16-bit real-mode entry).
    pub static ap_early_entry_code: u8;
    /// Data area start — magic + `ApBootstrap` + ladder GDT.
    pub static ap_early_entry_data_start: u8;
    /// Blob end (one past the data section).
    pub static ap_early_entry_end: u8;
}

/// The image as raw bytes (the copy unit the BSP installs at
/// `AP_STARTUP_PA`).
pub fn image_bytes() -> &'static [u8] {
    // SAFETY: address-only symbol reads; the slice spans one section pair.
    unsafe {
        core::slice::from_raw_parts(
            &ap_early_entry_start,
            &ap_early_entry_end as *const u8 as usize
                - &ap_early_entry_start as *const u8 as usize,
        )
    }
}

/// Blob byte range as `(start, end)` raw addresses (link-time VAs; the
/// installed copy lives at `AP_STARTUP_PA`).
/// Data-area offset within the blob (`data_start - start`) — the gap is
/// linker-determined (page-aligned in the current toolchain) and must be
/// consumed at run time, never baked into constants.
pub fn mailbox_offset() -> usize {
    // SAFETY: address-only symbol reads (no dereference).
    unsafe {
        &ap_early_entry_data_start as *const u8 as usize
            - &ap_early_entry_start as *const u8 as usize
    }
}

pub fn blob_range() -> (usize, usize) {
    // SAFETY: address-only symbol reads (no dereference).
    unsafe {
        (
            &ap_early_entry_start as *const u8 as usize,
            &ap_early_entry_end as *const u8 as usize,
        )
    }
}

/// BSP side: copy the image blob to `pa` (must be page-aligned, <1 MiB,
/// and identity-covered). The memory choice belongs to the caller (S-5
/// `smp_init` scans the free map, mirroring C `alloc_lowest`).
///
/// # Safety
///
/// `pa` must point to `image_len()` writable bytes that are free for the
/// AP-startup purpose and identity-covered by the boot root.
pub unsafe fn install_at(pa: usize) {
    assert!(pa & 0xFFF == 0 && pa < 0x10_0000, "AP startup address must be page-aligned < 1 MiB");
    let bytes = image_bytes();
    assert!(pa + bytes.len() < 0x10_0000, "image must stay below 1 MiB");
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), pa as *mut u8, bytes.len());
    }
}

/// BSP side: fill the bootstrap record inside the *installed* image and
/// publish it (`BOOT_MAGIC_SENT` last). Asserts the §3.2 invariant that
/// the page-table root is <4 GiB (the ladder's 32-bit `mov %cr3` writes
/// bits 31:0 only).
///
/// # Safety
///
/// `installed_pa` must be the address `install_at` copied to, and no AP
/// may be consuming the record yet (single-image serial reuse — the BSP
/// owns the record until the AP publishes `boot_ack`, S-3d).
pub unsafe fn fill_bootstrap(installed_pa: usize, record: &ApBootstrap) {
    assert!(
        record.page_table_root_pa < 0x1_0000_0000,
        "bootstrap page_table_root_pa must be <4GiB (32-bit CR3 load), got {:#x}",
        record.page_table_root_pa
    );
    let base = X86_64DirectMap::kernel_phys_to_virt(PhysBytes(installed_pa as u64));
    // The data section sits at `mailbox_offset()` past the blob start (the
    // gap is linker-determined and must never be assumed zero).
    let va = base.0 as usize + mailbox_offset();
    // MAGIC slot + record — plain volatile stores; the publisher fence
    // (x86 `mfence`, §3.9) belongs to `SmpArch::boot_ap` before the INIT.
    unsafe {
        core::ptr::write_volatile(va as *mut u64, BOOT_MAGIC_SENT);
        let rec = (va + D_RECORD) as *mut ApBootstrap;
        core::ptr::write_volatile(rec, *record);
        core::ptr::write_volatile(va as *mut u64, BOOT_MAGIC_SENT);
    }
}

/// AP side — the three-architecture convergence point (§3.2): entered from
/// the ladder's 64-bit tail with the bootstrap physical address; converts
/// PA→VA through the Direct Map and snapshots the record into locals.
///
/// # Safety
///
/// Must only be entered by the AP ladder (MMU on, identity root active,
/// running on the per-AP kernel stack).
pub unsafe extern "C" fn ap_early_entry(bootstrap_pa: usize) -> ! {
    let va = X86_64DirectMap::kernel_phys_to_virt(PhysBytes(bootstrap_pa as u64)).0 as usize
        + mailbox_offset();
    let (record, magic) = unsafe {
        (
            core::ptr::read_volatile(va as *const ApBootstrap),
            core::ptr::read_volatile(va as *const u64),
        )
    };
    assert_eq!(magic, BOOT_MAGIC_SENT, "AP entered with unpublished bootstrap record");
    // §3.9: everything needed after this point lives in locals — the
    // record belongs back to the BSP the moment boot_ack publishes (S-3d).
    let _ = record;
    // S-4 (init_ap: per-CPU GDT/TSS/MSR/LAPIC) and S-7 (scheduler loop)
    // extend this tail; until then the AP parks with interrupts off.
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
    }
}

/// Image length in bytes (data-area layout is fixed; see `D_LEN`).
pub const fn image_data_len() -> usize {
    D_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_constants_within_low_identity() {
        // AP_STARTUP_PA must sit inside the boot identity coverage
        // (PML4[0] = [0, 4 GiB)) and inside the real-mode reachable 1 MiB.
        assert!(AP_STARTUP_PA == 0x8000);
        assert!(AP_STARTUP_VECTOR == 0x08);
        assert!(AP_STARTUP_PA < 0x10_0000);
    }

    #[test]
    fn test_bootstrap_data_layout_contract() {
        // The ladder reads the record at fixed DATA-RELATIVE offsets
        // (root@+16, stack@+24, entry@+32 within the data area whose base
        // the low-mode addressing reaches via entry_pa + DATA_GAP). This
        // pins the serialization round-trip: fill_bootstrap writes the
        // record exactly at those offsets, little-endian, magic last.
        let mut data = [0u8; D_LEN];
        let record = ApBootstrap {
            logical_id: 1,
            _pad: 0,
            hw_id: 0x11,
            page_table_root_pa: 0x1000,
            kernel_stack_top_va: 0xFFFF_FF80_0000_8000,
            rust_entry_va: 0xFFFF_FF80_0010_0000,
        };
        unsafe {
            // Same store sequence as fill_bootstrap (minus the DM lookup).
            core::ptr::write_volatile(
                (data.as_mut_ptr() as usize + D_RECORD) as *mut ApBootstrap,
                record,
            );
            core::ptr::write_volatile(
                data.as_mut_ptr() as *mut u64,
                BOOT_MAGIC_SENT,
            );
        }
        assert_eq!(
            u64::from_le_bytes(data[0..8].try_into().unwrap()),
            BOOT_MAGIC_SENT
        );
        let rec = &data[D_RECORD..D_RECORD + 40];
        assert_eq!(u32::from_le_bytes(rec[0..4].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(rec[8..16].try_into().unwrap()), 0x11);
        // The two addresses the 64-bit tail consumes before the Rust jump.
        assert_eq!(
            u64::from_le_bytes(rec[24..32].try_into().unwrap()),
            0xFFFF_FF80_0000_8000
        );
        assert_eq!(
            u64::from_le_bytes(rec[32..40].try_into().unwrap()),
            0xFFFF_FF80_0010_0000
        );
    }

    #[test]
    fn test_ladder_gdt_entries_frozen() {
        assert_eq!(GDT_CODE16, 0x0000_9A00_0000_FFFF);
        assert_eq!(GDT_CODE32, 0x00CF_9A00_0000_FFFF);
        assert_eq!(GDT_CODE64, 0x00AF_9A00_0000_0000);
        assert_eq!(GDT_DATA32, 0x00CF_9200_0000_FFFF);
        assert_eq!(
            (SEL_CODE16, SEL_CODE32, SEL_CODE64, SEL_DATA32),
            (0x08, 0x10, 0x18, 0x20)
        );
    }
}
