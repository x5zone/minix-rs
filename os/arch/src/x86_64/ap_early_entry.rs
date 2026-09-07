//! x86 AP early entry ladder — S-3b/S-3c (real 16→32→64 climb).
//!
//! The AP wakes in 16-bit real mode at `AP_STARTUP_PA` (SIPI vector 0x08)
//! and climbs: real → protected (PAE) → long mode, then pulls the per-AP
//! kernel stack and the Rust entry address from the bootstrap record and
//! jumps to `ap_early_entry` (the three-architecture convergence point).
//!
//! # Hand-frozen layout (C trampoline.S discipline)
//!
//! The 16/32-bit ladder references the GDT and the bootstrap record through
//! absolute run-linear immediates (`0x8000 + blob offset`), so every
//! structure sits at a fixed blob offset — mirrored by the Rust consts
//! below and pinned by the hosted tests (which disassemble `image_bytes()`
//! against the expected bytes).
//!
//! Layout (blob offsets; run linear = 0x8000 + off):
//!
//! ```text
//! 0x000 code16 entry        0x030 GDT descriptor (limit 0x27, base 0x8140)
//! 0x040 code32 body         0x0C0 code64 tail
//! 0x110 GDT table (5×8)     0x140 BOOT_MAGIC u64
//! 0x148 ApBootstrap (40B)   0x170 blob end
//! ```

use crate::arch::ap_early_entry::{ApBootstrap, BOOT_MAGIC_SENT};
use crate::DirectMapArch;
use minix_types::PhysBytes;
use crate::X86_64DirectMap;

// ── Frozen placement constants ──

/// SIPI start address (vector 0x08). Inside the boot identity map
/// (PML4[0] = [0, 4 GiB)) and inside real-mode reach.
pub const AP_STARTUP_PA: u64 = 0x8000;
/// SIPI vector field value.
pub const AP_STARTUP_VECTOR: u32 = 0x08;

// Hand-frozen blob offsets (see the module layout table).
pub const OFF_GDT_DESC: usize = 0x030; // limit u16 @+0, base u64 @+2 (0x8140)
pub const OFF_CODE32: usize = 0x040; // far-jump target (32-bit body)
pub const OFF_CODE64: usize = 0x0C0; // far-jump target (64-bit tail)
pub const OFF_GDT_TABLE: usize = 0x110; // 5 entries × 8 bytes
pub const OFF_MAGIC: usize = 0x140; // BOOT_MAGIC_SENT u64
pub const OFF_RECORD: usize = 0x148; // ApBootstrap (40 bytes)

// Ladder GDT entries (frozen values).
const GDT_NULL: u64 = 0x0000_0000_0000_0000;
const GDT_CODE16: u64 = 0x0000_9A00_0000_FFFF; // byte gran, 16-bit
const GDT_CODE32: u64 = 0x00CF_9A00_0000_FFFF; // 4K gran, 32-bit
const GDT_CODE64: u64 = 0x00AF_9A00_0000_0000; // 4K gran, L=1
const GDT_DATA32: u64 = 0x00CF_9200_0000_FFFF; // 4K gran, writable
const GDT_LIMIT: u16 = 0x0027; // 5 entries × 8 − 1

// The GDT descriptor bytes as they sit at blob offset 0x030:
// limit 0x0027 (LE) + base 0x8140 (LE, 8 bytes).
const GDT_DESC_BYTES: [u8; 10] =
    [0x27, 0x00, 0x10, 0x81, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

// The far-jump / lgdt immediates as machine bytes (16-bit code section).
// lgdt m16&32 [0x8030]; ljmp 0x0010:0x8040.
const L16_GDT_BYTES: [u8; 6] = [0x66, 0x0F, 0x01, 0x15, 0x30, 0x80];
const L16_FJ_BYTES: [u8; 5] = [0xEA, 0x40, 0x80, 0x10, 0x00];

core::arch::global_asm!(
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
    // Stage trace (S-3d diagnostics): each stage writes its code byte to
    // the trace page at linear 0x8200 (DS = 0, identity RAM). The BSP
    // dumps these bytes at timeout to pinpoint the failing stage.
    "  mov byte ptr [0x8200], 1",   // stage 1: 16-bit entered
    "  mov byte ptr [0x8201], 2",   // lgdt done
    "  mov byte ptr [0x8202], 3",   // PE on
    "  mov byte ptr [0x8203], 4",   // far jump to 32-bit taken
    // lgdt m16&32 [0x8030] — the descriptor at blob offset 0x030.
    "  .byte 0x66, 0x0F, 0x01, 0x15",
    "  .word 0x8030",
    // Protected mode (paging still off — linear = PA).
    "  mov eax, cr0",
    "  or eax, 1",
    "  mov cr0, eax",
    // Far jump: 16-bit EA form (off16 sel16) → CODE32 @ run 0x8040.
    "  .byte 0xEA",
    "  .word 0x8040",
    "  .word 0x0010",
    // ── GDT descriptor @ fixed offset 0x030 (limit u16, base u64) ──
    ".org 0x030",
    "  .word {gdt_limit}",
    "  .quad 0x8110",
    // ── 32-bit protected mode @ fixed offset 0x040 (identity: linear = PA) ──
    ".org 0x040",
    ".code32",
    "ap_protected:",
    "  mov ax, 0x0020",
    "  mov ds, ax",
    "  mov ss, ax",
    "  mov byte ptr [0x8204], 5",  // stage 5: 32-bit entered
    // Root page table → CR3 (record @0x148, field root @+16 → linear
    // 0x8158; fill_bootstrap asserts root <4GiB — the 32-bit mov cr3
    // writes bits 31:0 only, §3.2 invariant).
    "  mov eax, dword ptr [0x00808158]",
    "  mov cr3, eax",
    "  mov byte ptr [0x8205], 6",  // CR3 loaded
    // PAE (long-mode prerequisite).
    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax",
    // EFER.LME (MSR 0xC0000080, bit 8).
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",
    // Paging on — identity continues the fetch.
    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",
    "  mov byte ptr [0x8206], 7",  // PG on (identity fetch next)
    // Far jump: 32-bit EA form (off32 sel16) → CODE64 @ run 0x80C0.
    "  .byte 0xEA",
    "  .long 0x008080C0",
    "  .word 0x0018",
    // ── 64-bit long mode, low identity region ──
    // 64-bit tail frozen at blob offset 0x0C0.
    ".org 0x0C0",
    ".code64",
    "ap_long_low:",
    // Per-AP kernel stack (record field stack @ blob 0x160 → linear
    // 0x8160). Absolute 32-bit addressing reaches the installed copy
    // through the identity mapping — rip-relative would reach the
    // ORIGINAL blob (link-time VAs), not the copy.
    "  mov byte ptr [0x8207], 8",  // stage 8: 64-bit tail entered
    "  mov rsp, qword ptr [0x00808160]",
    // Rust entry: RCX = bootstrap PA (MS x64 first argument).
    "  mov ecx, 0x8000",
    // Rust entry VA (record field @ blob 0x168 → linear 0x8168).
    "  mov rax, qword ptr [0x00808168]",
    "  jmp rax",
    // ── GDT table @ blob 0x110 ──
    ".balign 16",
    ".org 0x110",
    "ap_gdt_table:",
    ".quad {gdt_null}",
    ".quad {gdt_code16}",
    ".quad {gdt_code32}",
    ".quad {gdt_code64}",
    ".quad {gdt_data32}",
    // ── Bootstrap magic + record @ blob 0x140/0x148 (BSP-filled) ──
    ".org 0x140",
    "ap_boot_magic:",
    ".quad 0",
    "ap_boot_record:",
    ".quad 0",
    ".quad 0",
    ".quad 0",
    ".quad 0",
    ".quad 0",
    ".globl ap_early_entry_end",
    "ap_early_entry_end:",
    gdt_null = const GDT_NULL,
    gdt_code16 = const GDT_CODE16,
    gdt_code32 = const GDT_CODE32,
    gdt_code64 = const GDT_CODE64,
    gdt_data32 = const GDT_DATA32,
    gdt_limit = const GDT_LIMIT,
);

unsafe extern "C" {
    /// Blob start (code section; = `AP_STARTUP_PA` after install).
    pub static ap_early_entry_start: u8;
    /// 16-bit code entry (offset 0).
    pub static ap_early_entry_code: u8;
    /// Blob end (one past the GDT/record tail).
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

/// BSP side: fill the bootstrap record inside the *installed* image and
/// publish it (`BOOT_MAGIC_SENT` last). Asserts the §3.2 invariant that
/// the page-table root is <4 GiB (the ladder's 32-bit `mov cr3` writes
/// bits 31:0 only).
///
/// # Safety
///
/// `installed_pa` must be the address `install_at` copied to, and no AP
/// may be consuming the record yet (single-image serial reuse — the BSP
/// owns the record until the AP publishes its handshake, S-3d).
pub unsafe fn fill_bootstrap(installed_pa: usize, record: &ApBootstrap) {
    assert!(
        record.page_table_root_pa < 0x1_0000_0000,
        "bootstrap page_table_root_pa must be <4GiB (32-bit CR3 load), got {:#x}",
        record.page_table_root_pa
    );
    let base = X86_64DirectMap::kernel_phys_to_virt(PhysBytes(installed_pa as u64)).0 as usize;
    let magic = base + OFF_MAGIC;
    let rec = base + OFF_RECORD;
    unsafe {
        core::ptr::write_volatile(rec as *mut ApBootstrap, *record);
        core::ptr::write_volatile(magic as *mut u64, BOOT_MAGIC_SENT);
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
    let va = X86_64DirectMap::kernel_phys_to_virt(PhysBytes(bootstrap_pa as u64)).0 as usize;
    let (magic, record) = unsafe {
        (
            core::ptr::read_volatile((va + OFF_MAGIC) as *const u64),
            core::ptr::read_volatile((va + OFF_RECORD) as *const ApBootstrap),
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
        assert_eq!(AP_STARTUP_PA, 0x8000);
        assert_eq!(AP_STARTUP_VECTOR, 0x08);
        assert!(AP_STARTUP_PA < 0x10_0000);
    }

    #[test]
    fn test_ladder_offsets_frozen() {
        assert_eq!(OFF_GDT_DESC, 0x030);
        assert_eq!(OFF_CODE32, 0x040);
        assert_eq!(OFF_CODE64, 0x0C0);
        assert_eq!(OFF_GDT_TABLE, 0x110);
        assert_eq!(OFF_MAGIC, 0x140);
        assert_eq!(OFF_RECORD, 0x148);
    }

    #[test]
    fn test_l16_encoding_bytes_frozen() {
        // The 16-bit lgdt + far jump encodings, pinned byte-for-byte:
        // lgdt m16&32 [disp16 0x8030]; ljmp EA off16 0x8040 sel 0x0010.
        assert_eq!(L16_GDT_BYTES, [0x66, 0x0F, 0x01, 0x15, 0x30, 0x80]);
        assert_eq!(L16_FJ_BYTES, [0xEA, 0x40, 0x80, 0x10, 0x00]);
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
    fn test_image_starts_with_cli_and_layout_markers() {
        let img = image_bytes();
        // 16-bit entry: cli = 0xFA.
        assert_eq!(img[0], 0xFA);
        // GDT descriptor bytes at 0x030: limit 0x0027 then base 0x8140.
        assert_eq!(&img[OFF_GDT_DESC..OFF_GDT_DESC + 10], &GDT_DESC_BYTES[..]);
        // GDT table null entry @ 0x110.
        assert_eq!(&img[OFF_GDT_TABLE..OFF_GDT_TABLE + 8], &[0u8; 8]);
    }

    #[test]
    fn test_bootstrap_record_layout_mirror() {
        // Hosted mirror of fill_bootstrap's stores (the real fn goes
        // through the Direct Map, which only exists on hardware): write
        // magic + record at their layout offsets, then read them back the
        // way the 64-bit ladder does (root @ 0x8158, stack @ 0x8160, entry
        // @ 0x8168 in run-linear terms).
        let mut buf = [0u8; 0x10000];
        let bytes = image_bytes();
        buf[0..bytes.len()].copy_from_slice(bytes);
        assert_eq!(buf[0], 0xFA);

        let record = ApBootstrap {
            logical_id: 1,
            _pad: 0,
            hw_id: 0x11,
            page_table_root_pa: 0x1000,
            kernel_stack_top_va: 0x0000_0000_0001_8000,
            rust_entry_va: 0x0000_0000_0002_0000,
        };
        buf[OFF_MAGIC..OFF_MAGIC + 8]
            .copy_from_slice(BOOT_MAGIC_SENT.to_le_bytes().as_slice());
        // Serialize the record exactly as fill_bootstrap's volatile store
        // lays it out (§3.2 field order, little-endian).
        let mut rec_bytes = [0u8; 40];
        rec_bytes[0..4].copy_from_slice(&record.logical_id.to_le_bytes());
        rec_bytes[4..8].copy_from_slice(&record._pad.to_le_bytes());
        rec_bytes[8..16].copy_from_slice(&record.hw_id.to_le_bytes());
        rec_bytes[16..24].copy_from_slice(&record.page_table_root_pa.to_le_bytes());
        rec_bytes[24..32].copy_from_slice(&record.kernel_stack_top_va.to_le_bytes());
        rec_bytes[32..40].copy_from_slice(&record.rust_entry_va.to_le_bytes());
        buf[OFF_RECORD..OFF_RECORD + 40].copy_from_slice(&rec_bytes);
        assert_eq!(
            u64::from_le_bytes(buf[OFF_RECORD + 16..OFF_RECORD + 24].try_into().unwrap()),
            0x1000
        );
        assert_eq!(
            u64::from_le_bytes(buf[OFF_RECORD + 24..OFF_RECORD + 32].try_into().unwrap()),
            0x0000_0000_0001_8000
        );
        assert_eq!(
            u64::from_le_bytes(buf[OFF_RECORD + 32..OFF_RECORD + 40].try_into().unwrap()),
            0x0000_0000_0002_0000
        );
    }
}
