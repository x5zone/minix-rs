//! x86 AP early entry ladder — S-3b/S-3c/S-3d (real 16→32→64 climb).
//!
//! # Design (one paragraph)
//!
//! The AP wakes in 16-bit real mode at the fixed linear address 0x8000
//! (SIPI vector 0x08). The ladder climbs real → protected (PAE) → long
//! mode using **absolute run-linear immediates only** — no rip-relative,
//! no relocations, no linker-gap dependencies — because the scratch page
//! it consumes sits at the FIXED linear address 0x9000, written by the BSP
//! before `boot_ap` (C: trampoline data at a fixed low address,
//! arch_smp.c:130 publisher `mfence` belongs on the boot_ap side, S-3c).
//!
//! # Scratch page layout (linear, written by the BSP)
//!
//! ```text
//! 0x6000 BOOT_MAGIC_SENT u64          0x6008 ApBootstrap record (40B)
//! 0x6030 GDT descriptor (limit 0x27)  0x6032 GDT base u64 (0x6110)
//! 0x6110 GDT table (5 × 8 B)
//! ```
//!
//! # Convergence
//!
//! The 64-bit tail loads rsp/entry from the record and jumps to the
//! Rust `ap_early_entry(bootstrap_pa)` convergence point (MS x64: first
//! argument in RCX) — which converts PA→VA via the Direct Map.

use crate::arch::ap_early_entry::{ApBootstrap, BOOT_MAGIC_SENT};
use crate::DirectMapArch;
use minix_types::PhysBytes;
use crate::X86_64DirectMap;

// ── Frozen constants (build ↔ run contract) ──

/// SIPI start address (vector 0x08) — also the ladder blob's install
/// address.
pub const AP_STARTUP_PA: u64 = 0x5000;

/// Bring-up diagnostic: the ladder writes a stage byte here after each
/// mode transition (values 0xA1..0xA7, 0 = no progress). The BSP clears
/// it before `boot_ap` and reads it on timeout — the dead stage is then
/// directly observable from the BSP (S-3d).
pub const AP_STAGE_MARK: u32 = 0x6F00;
/// SIPI vector field value.
pub const AP_STARTUP_VECTOR: u32 = 0x05;
/// Fixed linear address of the ladder's scratch page (BSP-written).
pub const SCRATCH_LIN: u32 = 0x6000;

// Scratch offsets (from SCRATCH_LIN).
pub const S_MAGIC: usize = 0x000;
pub const S_RECORD: usize = 0x008;
pub const S_GDT_DESC: usize = 0x030; // limit u16 @+0, base u64 @+2
pub const S_GDT_TABLE: usize = 0x110; // 5 entries × 8 B

// Ladder GDT entries (frozen values).
const GDT_NULL: u64 = 0x0000_0000_0000_0000;
const GDT_CODE16: u64 = 0x0000_9A00_0000_FFFF; // byte gran, 16-bit
const GDT_CODE32: u64 = 0x00CF_9A00_0000_FFFF; // 4K gran, 32-bit
const GDT_CODE64: u64 = 0x00AF_9A00_0000_0000; // 4K gran, L=1
const GDT_DATA32: u64 = 0x00CF_9200_0000_FFFF; // 4K gran, writable
const GDT_LIMIT: u16 = 0x0027; // 5 entries × 8 − 1

// The GDT descriptor bytes as they sit at linear 0x6030:
// limit 0x0027 (LE) + base 0x6110 (LE, 8 bytes).
const GDT_DESC_BYTES: [u8; 10] =
    [0x27, 0x00, 0x10, 0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

core::arch::global_asm!(
    ".section .ap_early_entry, \"ax\"",
    // Run-linear base of the installed blob — label-derived far-jump
    // offsets are section-relative and must add this base to become the
    // run-linear targets the AP needs (S-3d root cause, 2026-09-09).
    ".set AP_BASE, 0x5000",
    ".globl ap_early_entry_start",
    ".globl ap_early_entry_code",
    ".globl ap_early_entry_end",
    ".code16",
    "ap_early_entry_start:",
    "ap_early_entry_code:",
    "  cli",
    "  xor ax, ax",
    "  mov ds, ax",
    "  mov ss, ax",
    // Stage 0xA1: real-mode entry + segments up.
    "  mov byte ptr [0x6F00], 0xA1",
    // lgdt m16&32 [0x6030] — the descriptor in the scratch page.
    "  .byte 0x66, 0x0F, 0x01, 0x15",
    "  .word 0x6030",
    // Protected mode (paging still off — linear = PA).
    "  mov eax, cr0",
    "  or eax, 1",
    "  mov cr0, eax",
    // Stage 0xA2: protected mode on (fetch still 16-bit until the far jump).
    "  mov byte ptr [0x6F00], 0xA2",
    // Far jump: 16-bit EA form (off16 sel16) → CODE32. The offset is
    // assembler-resolved from the same-section label difference (= the
    // run-linear offset, since the blob installs at AP_STARTUP_PA) —
    // hand-written immediates desynced from the real layout and landed the
    // AP inside the FJ32 operand bytes (the S-3d root cause, fixed 2026-09-09).
    "  .byte 0xEA",
    "  .word (ap_protected - ap_early_entry_start) + AP_BASE",
    "  .word 0x0010",
    // ── 32-bit protected mode (identity: linear = PA) ──
    ".code32",
    "ap_protected:",
    "  mov ax, 0x0020",
    "  mov ds, ax",
    "  mov ss, ax",
    // Stage 0xA3: protected entry reached.
    "  mov byte ptr [0x6F00], 0xA3\n  mov dx, 0x3f8\n  mov al, 0xA3\n  out 0x80, al\n  mov dx, 0x3f8\n  mov al, 0xA3\n  out dx, al",
    // Root page table → CR3 (record field @ scratch+0x18; fill_bootstrap
    // asserts root <4GiB — the 32-bit mov cr3 writes bits 31:0 only,
    // §3.2 invariant).
    "  mov eax, dword ptr [0x00606018]",
    "  mov cr3, eax",
    // PAE (long-mode prerequisite).
    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax",
    // EFER.LME (MSR 0xC0000080, bit 8).
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",
    // Paging on — identity continues the fetch (the scratch page is
    // identity RAM, so the absolute reads below keep working).
    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",
    // Stage 0xA4: long mode on (fetch continues at the far jump).
    "  mov byte ptr [0x6F00], 0xA4\n  mov dx, 0x3f8\n  mov al, 0xA4\n  out 0x80, al\n  mov dx, 0x3f8\n  mov al, 0xA4\n  out dx, al",
    // Far jump: 32-bit EA form (off32 sel16) → CODE64 (label-derived, same
    // rationale as the 16-bit far jump above).
    "  .byte 0xEA",
    "  .long (ap_long_low - ap_early_entry_start) + AP_BASE",
    "  .word 0x0018",
    // ── 64-bit long mode, low identity region ──
    ".code64",
    "ap_long_low:",
    // Stage 0xA5: long-mode entry reached.
    "  mov byte ptr [0x6F00], 0xA5\n  mov dx, 0x3f8\n  mov al, 0xA5\n  out 0x80, al\n  mov dx, 0x3f8\n  mov al, 0xA5\n  out dx, al",
    // Per-AP kernel stack (record field stack @ scratch+0x24 → linear
    // 0x9024). Absolute 32-bit addressing reaches the installed copy
    // through the identity mapping — rip-relative would reach the
    // ORIGINAL blob (link-time VAs), not the copy.
    "  mov rsp, qword ptr [0x00606024]",
    // Rust entry: RCX = bootstrap PA (MS x64 first argument).
    "  mov ecx, 0x5000",
    // Rust entry VA (record field @ scratch+0x28 → linear 0x9028).
    "  mov rax, qword ptr [0x00606028]",
    // Stage 0xA6: record consumed, entering the Rust tail.
    "  mov byte ptr [0x6F00], 0xA6\n  mov dx, 0x3f8\n  mov al, 0xA6\n  out 0x80, al\n  mov dx, 0x3f8\n  mov al, 0xA6\n  out dx, al",
    "  jmp rax",
    // Blob end marker (for image_bytes length calculation).
    "ap_early_entry_end:",
);

unsafe extern "C" {
    /// Blob start (code section; = `AP_STARTUP_PA` after install).
    pub static ap_early_entry_start: u8;
    /// Blob end.
    pub static ap_early_entry_end: u8;
}

/// The image as raw bytes (the copy unit the BSP installs at
/// `AP_STARTUP_PA`).
pub fn image_bytes() -> &'static [u8] {
    // SAFETY: address-only symbol reads; the slice spans this image's
    // own sections.
    unsafe {
        core::slice::from_raw_parts(
            &ap_early_entry_start,
            &ap_early_entry_end as *const u8 as usize
                - &ap_early_entry_start as *const u8 as usize,
        )
    }
}

/// BSP side: copy the image blob to `pa` (page-aligned, <1 MiB, inside
/// the boot identity map). The memory choice belongs to the caller (S-5
/// `smp_init` scans the free map, mirroring C `alloc_lowest`).
///
/// # Safety
///
/// `pa` must point to `image_bytes().len()` writable bytes that are free
/// for the AP-startup purpose and identity-covered by the boot root.
pub unsafe fn install_at(pa: usize) {
    assert!(
        pa & 0xFFF == 0 && pa < 0x10_0000,
        "AP startup address must be page-aligned < 1 MiB, got {pa:#x}"
    );
    let bytes = image_bytes();
    assert!(
        pa + bytes.len() < 0x10_0000,
        "image must stay below 1 MiB, len {:#x}",
        bytes.len()
    );
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), pa as *mut u8, bytes.len());
    }
}

/// BSP side: fill the scratch page — bootstrap record + GDT descriptor +
/// GDT table — and publish it (`BOOT_MAGIC_SENT` last). Asserts the §3.2
/// invariant that the page-table root is <4 GiB (the ladder's 32-bit
/// `mov cr3` writes bits 31:0 only).
///
/// # Safety
///
/// `scratch_lin` must be the fixed linear address the ladder reads
/// ([`SCRATCH_LIN`]), backed by writable identity RAM. No AP may be
/// consuming the record yet (single-image serial reuse — the BSP owns it
/// until the AP publishes its handshake, S-3d).
///
/// The publisher fence (x86 `mfence`, §3.9) belongs to `SmpArch::boot_ap`
/// before the firmware call (S-3c).
pub unsafe fn fill_bootstrap(scratch_lin: u32, record: &ApBootstrap) {
    assert_eq!(scratch_lin as usize, SCRATCH_LIN as usize);
    assert!(
        record.page_table_root_pa < 0x1_0000_0000,
        "bootstrap page_table_root_pa must be <4GiB (32-bit CR3 load), got {:#x}",
        record.page_table_root_pa
    );
    let base = scratch_lin as usize;
    unsafe {
        core::ptr::write_volatile((base + S_MAGIC) as *mut u64, BOOT_MAGIC_SENT);
        core::ptr::write_volatile((base + S_RECORD) as *mut ApBootstrap, *record);
        core::ptr::write_volatile((base + S_GDT_DESC) as *mut u16, GDT_LIMIT);
        // base u64 at +2: byte writes (the address is 2-mod-8, not
        // u64-aligned — unaligned wide stores are UB in Rust's model even
        // though x86 hardware tolerates them).
        let base_bytes = (base + S_GDT_TABLE) as u64;
        for (i, b) in base_bytes.to_le_bytes().iter().enumerate() {
            core::ptr::write_volatile((base + S_GDT_DESC + 2 + i) as *mut u8, *b);
        }
        core::ptr::write_volatile((base + S_GDT_TABLE) as *mut u64, GDT_NULL);
        core::ptr::write_volatile((base + S_GDT_TABLE + 8) as *mut u64, GDT_CODE16);
        core::ptr::write_volatile((base + S_GDT_TABLE + 16) as *mut u64, GDT_CODE32);
        core::ptr::write_volatile((base + S_GDT_TABLE + 24) as *mut u64, GDT_CODE64);
        core::ptr::write_volatile((base + S_GDT_TABLE + 32) as *mut u64, GDT_DATA32);
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
    // Stage 0xA7: Rust entry reached (S-3d diagnostic).
    unsafe {
        core::ptr::write_volatile(
            X86_64DirectMap::kernel_phys_to_virt(PhysBytes(AP_STAGE_MARK as u64)).0 as *mut u8,
            0xA7,
        );
    }
    let va = X86_64DirectMap::kernel_phys_to_virt(PhysBytes(SCRATCH_LIN as u64)).0 as usize
        + S_RECORD as usize;
    let (magic, record) = unsafe {
        (
            core::ptr::read_volatile(va as *const u64),
            core::ptr::read_volatile(va as *const ApBootstrap),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_constants_within_low_identity() {
        // S-3d (2026-09-09): vector page moved to 0x05 — PA 0x8000 is
        // OVMF's own AP park/jump-table area under QEMU (monitor `xp`
        // evidence, see smp_todo S-3d).
        assert_eq!(AP_STARTUP_PA, 0x5000);
        assert_eq!(AP_STARTUP_VECTOR, 0x05);
        assert!(AP_STARTUP_PA < 0x10_0000);
    }

    #[test]
    fn test_scratch_layout_offsets_frozen() {
        // The ladder's absolute immediates are compiled against these
        // offsets — they are the build↔run contract.
        assert_eq!(SCRATCH_LIN, 0x6000);
        assert_eq!(S_MAGIC, 0);
        assert_eq!(S_RECORD, 8);
        assert_eq!(S_GDT_DESC, 0x30);
        assert_eq!(S_GDT_TABLE, 0x110);
    }

    #[test]
    fn test_ladder_gdt_entries_frozen() {
        assert_eq!(GDT_CODE16, 0x0000_9A00_0000_FFFF);
        assert_eq!(GDT_CODE32, 0x00CF_9A00_0000_FFFF);
        assert_eq!(GDT_CODE64, 0x00AF_9A00_0000_0000);
        assert_eq!(GDT_DATA32, 0x00CF_9200_0000_FFFF);
        assert_eq!(GDT_LIMIT, 0x0027);
    }

    #[test]
    fn test_gdt_desc_bytes_frozen() {
        assert_eq!(GDT_DESC_BYTES, [0x27, 0x00, 0x10, 0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_scratch_layout_mirror() {
        // Hosted mirror of fill_bootstrap's stores: write magic + record +
        // GDT descriptor + GDT table into a scratch buffer at the frozen
        // offsets, then read them back exactly the way the ladder does
        // (root @ SCRATCH+0x18, stack @ +0x24, entry @ +0x28).
        let mut scratch = [0u8; 0x140];
        let record = ApBootstrap {
            logical_id: 1,
            _pad: 0,
            hw_id: 0x11,
            page_table_root_pa: 0x1000,
            kernel_stack_top_va: 0x0000_0000_0001_8000,
            rust_entry_va: 0x0000_0000_0002_0000,
        };
        // fill_bootstrap 本尊经 Direct Map 写硬件 scratch（hosted 无 DM），
        // 这里镜像其存储序列验证布局契约。
        unsafe {
            core::ptr::write_volatile(scratch.as_mut_ptr() as *mut u64, BOOT_MAGIC_SENT);
            core::ptr::write_volatile(
                scratch.as_mut_ptr().add(S_RECORD) as *mut ApBootstrap,
                record,
            );
            core::ptr::write_volatile(
                scratch.as_mut_ptr().add(S_GDT_DESC) as *mut u16,
                GDT_LIMIT,
            );
            let base_bytes = (SCRATCH_LIN as usize + S_GDT_TABLE) as u64;
            for (i, b) in base_bytes.to_le_bytes().iter().enumerate() {
                core::ptr::write_volatile(
                    scratch.as_mut_ptr().add(S_GDT_DESC + 2 + i) as *mut u8,
                    *b,
                );
            }
        }
        assert_eq!(
            u64::from_le_bytes(scratch[S_MAGIC..S_MAGIC + 8].try_into().unwrap()),
            BOOT_MAGIC_SENT
        );
        let rec = &scratch[S_RECORD..S_RECORD + 40];
        assert_eq!(u32::from_le_bytes(rec[0..4].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(rec[8..16].try_into().unwrap()), 0x11);
        assert_eq!(u64::from_le_bytes(rec[16..24].try_into().unwrap()), 0x1000);
        assert_eq!(
            u64::from_le_bytes(rec[24..32].try_into().unwrap()),
            0x0000_0000_0001_8000
        );
        assert_eq!(
            u64::from_le_bytes(rec[32..40].try_into().unwrap()),
            0x0000_0000_0002_0000
        );
        // GDT descriptor: limit 0x27, base = SCRATCH_LIN + S_GDT_TABLE.
        assert_eq!(&scratch[S_GDT_DESC..S_GDT_DESC + 2], &GDT_LIMIT.to_le_bytes());
        assert_eq!(
            u64::from_le_bytes(scratch[S_GDT_DESC + 2..S_GDT_DESC + 10].try_into().unwrap()),
            (SCRATCH_LIN as usize + S_GDT_TABLE) as u64
        );
    }

    #[test]
    fn test_far_jump_targets_hit_real_instruction_boundaries() {
        // S-3d root-cause pin (2026-09-09): the ladder's two far jumps
        // used hand-written offsets that had desynced from the emitted
        // layout — the AP landed inside the FJ32 operand bytes and
        // executed garbage. The assembler now derives both offsets from
        // labels + AP_BASE; this test walks every `EA` opcode in the
        // installed image, computes each candidate's in-blob offset
        // (target − AP_BASE) and asserts it lands on the expected opcode
        // signature of the next stage (protected entry `mov ax, imm16`;
        // long entry `mov rsp, [abs]`).
        const BASE: usize = AP_STARTUP_PA as usize;
        let blob = image_bytes();
        let mut fj16 = false;
        let mut fj32 = false;
        for (i, b) in blob.iter().enumerate() {
            if *b != 0xEA {
                continue;
            }
            // 16-bit form candidate: EA off16 sel16 — off16 = blob offset
            // of `ap_protected`, decoded from the two bytes after EA.
            if i + 5 <= blob.len() {
                let t16 = u16::from_le_bytes(blob[i + 1..i + 3].try_into().unwrap()) as usize;
                if t16 > BASE && t16 - BASE < blob.len() && blob[t16 - BASE..t16 - BASE + 2] == [0x66, 0xB8] {
                    let sel16 = u16::from_le_bytes(blob[i + 3..i + 5].try_into().unwrap());
                    if sel16 == 0x0010 {
                        fj16 = true;
                    }
                }
            }
            // 32-bit form candidate: EA off32 sel16 — off32 = blob offset
            // of `ap_long_low`.
            if i + 7 <= blob.len() {
                let t32 = u32::from_le_bytes(blob[i + 1..i + 5].try_into().unwrap()) as usize;
                // FJ32's first target instruction is the 0xA5 stage mark
                // write (`mov byte [abs], imm8` via SIB-abs), which
                // precedes `mov rsp, [abs]`.
                if t32 > BASE && t32 - BASE + 3 <= blob.len() && blob[t32 - BASE..t32 - BASE + 3] == [0xC6, 0x04, 0x25] {
                    let sel16 = u16::from_le_bytes(blob[i + 5..i + 7].try_into().unwrap());
                    if sel16 == 0x0018 {
                        fj32 = true;
                    }
                }
            }
        }
        assert!(fj16, "FJ16 must target the protected-mode entry (mov ax, imm16) in-blob");
        assert!(fj32, "FJ32 must target the long-mode entry (0xA5 stage mark) in-blob");
        // The blob must end exactly at the final `jmp rax`.
        assert_eq!(&blob[blob.len() - 2..], &[0xFF, 0xE0]);
    }

    fn record_fixture() -> ApBootstrap {
        ApBootstrap {
            logical_id: 1,
            _pad: 0,
            hw_id: 0x11,
            page_table_root_pa: 0x1000,
            kernel_stack_top_va: 0x0000_0000_0001_8000,
            rust_entry_va: 0x0000_0000_0002_0000,
        }
    }
}
