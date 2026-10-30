//! Test: AP alive verification (SMP bring-up ladder) — x86_64,
//! **multiboot direct-boot variant** (QEMU `-kernel`, bypassing OVMF;
//! OVMF(TCG)'s MpInitLib AP parking / jump-table area conflicts with the
//! low-memory trampoline, and there are wandering #UD cores on that path).
//! Under multiboot the APs naturally wait-for-SIPI with no firmware
//! interference.
//!
//! BSP: multiboot entry (32-bit) → 32→64 trampoline (identity page tables
//! + GDT) → rust_main64: enable LAPIC → build identity page tables (the
//! trampoline already built them) → install blob @0x5000 → fill bootstrap
//! @0x6000 → INIT-SIPI. AP: climbs the 16→32→64 ladder (AP_STAGE_MARK
//! writes 0x6F00 rung by rung) → ap_entry → BOOT_ACK. PASS = the BSP
//! observes the ack bit.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU64, Ordering};
use minix_arch::arch::ap_early_entry::ApBootstrap;
use minix_arch::smp::SmpArch;
use minix_arch::x86_64::ap_early_entry::install_at;
use minix_arch::x86_64::protection::X86_64Protection;
use minix_arch::ProtectionArch;
use minix_arch::x86_64::smp::X86_64SmpArch;
use minix_plat::x86_64::early_console;
use minix_types::VirBytes;

// ── Trampoline page tables (filled by the 32-bit entry; .bss is zeroed
// by the loader) ──
// Loading CR3 requires 4KB alignment (bits 11:0 are reserved; a misaligned
// value raises #GP — the trampoline's first trap).
#[repr(align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}
#[unsafe(no_mangle)]
pub static mut MB_PML4: PageTable = PageTable { entries: [0; 512] };
#[unsafe(no_mangle)]
pub static mut MB_PDPT: PageTable = PageTable { entries: [0; 512] };
#[unsafe(no_mangle)]
pub static mut MB_PD0: PageTable = PageTable { entries: [0; 512] };
#[unsafe(no_mangle)]
pub static mut MB_PD1: PageTable = PageTable { entries: [0; 512] };
#[unsafe(no_mangle)]
pub static mut MB_PD2: PageTable = PageTable { entries: [0; 512] };
#[unsafe(no_mangle)]
pub static mut MB_PD3: PageTable = PageTable { entries: [0; 512] };

/// AP-dedicated stack (identity-mapped, <4GB).
#[repr(align(16))]
struct ApStack(pub [u8; 0x10000]);
static mut AP_STACK: ApStack = ApStack([0u8; 0x10000]);
/// AP stack top (computed by the BSP and handed to the AP through this —
/// the same-value bypass of the bootstrap record).
static AP_STACK_TOP: AtomicU64 = AtomicU64::new(0);

/// Bit n set == AP logical_id n reached the Rust entry.
static BOOT_ACK: AtomicU64 = AtomicU64::new(0);

/// Shared per-CPU protection instance: the BSP builds it (all 8 TSS
/// descriptors at stable addresses), stamps the AP slot via
/// set_kernel_stack before the SIPI, and the AP runs `init_ap` on its own
/// core — lgdt the shared GDT image, ltr its own TSS, program its GS MSRs.
static mut PROTECTION: Option<X86_64Protection> = None;

/// AP side-effect channel: the value `ap_cpu_id_readback()` returned on the
/// AP (gs:0x10 travels through that CPU's own GS_BASE MSR).
static AP_CPU_ID_READBACK: AtomicU64 = AtomicU64::new(0);

// This test has no heap; this null allocator only satisfies the alloc
// symbol requirements of the dependency chain (it is never called, and a
// call would hand back a null pointer — if an allocation ever does happen,
// the first one after link time exposes it immediately).
struct NullAlloc;
unsafe impl core::alloc::GlobalAlloc for NullAlloc {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static ALLOCATOR: NullAlloc = NullAlloc;

unsafe extern "C" fn ap_entry(_bootstrap_pa: usize) -> ! {
    early_console::write_str("### AP IN RUST (ladder complete)\n");

    // Per-CPU protection bring-up on THIS core —
    // lgdt/ltr the shared GDT (cpu 1's 16-byte TSS descriptor), program the
    // GS area + both GS_BASE MSRs, re-write the per-CPU SYSCALL MSRs (the
    // BSP's MSR writes do not propagate), then prove the identity:
    // gs:0x10 must read back the value program_gs stored for cpu 1.
    let ap_stack_top = AP_STACK_TOP.load(Ordering::Acquire);
    unsafe {
        let prot = &*core::ptr::addr_of!(PROTECTION);
        if let Some(prot) = prot {
            prot.init_ap(1, VirBytes::new(ap_stack_top));
        }
    }
    minix_arch::ap_write_syscall_msrs(minix_arch::syscall_entry_va());
    let readback = minix_arch::ap_cpu_id_readback();
    AP_CPU_ID_READBACK.store(readback, Ordering::Release);
    early_console::write_str("### AP cpu_id readback done\n");

    BOOT_ACK.fetch_or(1 << 1, Ordering::Release);
    loop {
        asm!("cli", options(nomem, nostack));
        asm!("hlt", options(nomem, nostack));
    }
}

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    // Diagnostic: the last ladder rung the AP reached (0 = it did not even
    // respond to the SIPI; 0xA1..0xA7 = mode-ladder progress, see the
    // AP_STAGE_MARK documentation).
    let stage = unsafe { core::ptr::read_volatile(0x6F00 as *const u8) };
    early_console::write_str("### AP stage: 0x");
    early_console::write_hex(stage as u64);
    early_console::write_str("\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC\n");
    // Do not read loc.file() (the string pointer may be corrupt → FF
    // storm); print the line number only.
    if let Some(loc) = info.location() {
        early_console::write_str("### PANIC line: ");
        early_console::write_hex(loc.line() as u64);
        early_console::write_str("\n");
    }
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

// ── multiboot header + 32→64 trampoline ──
core::arch::global_asm!(
    // Multiboot header: AOUT kludge (bit 16) — QEMU -kernel's multiboot
    // loader only accepts 32-bit ELF; the kludge makes it load the whole
    // "file image → load_addr" and enter at the header's entry_addr (mb.ld
    // provides the mb_load_end/mb_bss_end/mb_entry boundary symbols; .bss is
    // zeroed by the loader over [load_end, bss_end)). The header occupies the
    // first 8KB of the image, 4-byte aligned.
    ".section .multiboot_header, \"a\"",
    ".align 4",
    ".long 0x1BADB002",
    ".long 0x00010003",          // bit0 page-align modules + bit1 mem_info + bit16 AOUT kludge
    ".long -(0x1BADB002 + 0x00010003)",
    ".long 0x100000",            // header_addr: the .mbh run address (= 0x100000 after kludge seek compensation)
                                 // (QEMU loads with seek = header file offset − (header_addr − load_addr);
                                 //  writing it equal to load_addr shifts the whole image by +0x1000 — the mb variant's last root cause)
    ".long 0x100000",            // load_addr (the first file byte is loaded here)
    ".long mb_load_end",         // load_end_addr (end of the file-backed content)
    ".long mb_bss_end",          // bss_end_addr ([load_end, bss_end) is zeroed)
    ".long mb_entry",            // entry_addr (the 32-bit _start)
    "",
    // 64-bit GDT used by the AP (null / code64 / data64) + descriptor.
    ".section .mb_gdt, \"a\"",
    ".align 8",
    "mb_gdt:",
    "  .quad 0",
    "  .quad 0x00AF9A0000000000", // code64: P1 DPL0 S1 Type1010 L1
    "  .quad 0x00CF920000000000", // data64: P1 DPL0 S1 Type0010 W, 4K G D0
    "mb_gdt_desc:",
    "  .word 0x17",               // limit = 3*8-1
    "  .long mb_gdt",             // base <4GB (guaranteed by the low link address)
    "",
    // 32-bit multiboot entry (QEMU multiboot: EAX=0x2BADB002 EBX=info, flat segments, paging off).
    ".section .text",
    ".code32",
    ".globl _start",
    "_start:",
    "  cli\n  mov byte ptr [0x6F00], 0x81",
    "  mov esp, offset mb_stack_top",
    // Identity page tables: PML4[0]=PDPT|3; PDPT[0..4]=PD0..3|0x83;
    // each PD holds 512 2MB pages (low dword = base|0x83, high dword already zeroed).
    "  mov eax, offset MB_PDPT",
    "  or eax, 3",
    "  mov [offset MB_PML4], eax",
    "  mov eax, offset MB_PD0",
    "  or eax, 3",   // PDPT non-leaf entry: P|RW, no PS (PS at PDPT would be a 1GB page, which qemu64 lacks)
    "  mov [offset MB_PDPT], eax",
    "  mov eax, offset MB_PD1",
    "  or eax, 3",   // PDPT non-leaf entry: P|RW, no PS (PS at PDPT would be a 1GB page, which qemu64 lacks)
    "  mov [offset MB_PDPT + 8], eax",
    "  mov eax, offset MB_PD2",
    "  or eax, 3",   // PDPT non-leaf entry: P|RW, no PS (PS at PDPT would be a 1GB page, which qemu64 lacks)
    "  mov [offset MB_PDPT + 16], eax",
    "  mov eax, offset MB_PD3",
    "  or eax, 3",   // PDPT non-leaf entry: P|RW, no PS (PS at PDPT would be a 1GB page, which qemu64 lacks)
    "  mov [offset MB_PDPT + 24], eax",
    "  mov edi, offset MB_PD0",
    "  mov eax, 0x00000083",
    "  mov ecx, 512",
    "1:",
    "  mov [edi], eax",
    "  add eax, 0x200000",
    "  add edi, 8",
    "  loop 1b",
    "  mov edi, offset MB_PD1",
    "  mov eax, 0x40000083",
    "  mov ecx, 512",
    "2:",
    "  mov [edi], eax",
    "  add eax, 0x200000",
    "  add edi, 8",
    "  loop 2b",
    "  mov edi, offset MB_PD2",
    "  mov eax, 0x80000083",
    "  mov ecx, 512",
    "3:",
    "  mov [edi], eax",
    "  add eax, 0x200000",
    "  add edi, 8",
    "  loop 3b",
    "  mov edi, offset MB_PD3",
    "  mov eax, 0xC0000083",
    "  mov ecx, 512",
    "4:",
    "  mov [edi], eax",
    "  add eax, 0x200000",
    "  add edi, 8",
    "  loop 4b\n  mov byte ptr [0x6F01], 0x82",
    // GDT (including the 64-bit code segment) → PAE → EFER.LME → PG → far jump to 64-bit.
    "  lgdt [mb_gdt_desc]\n  mov byte ptr [0x6F02], 0x83",
    "  mov eax, cr4",
    "  or eax, 0x20",
    "  mov cr4, eax\n  mov byte ptr [0x6F03], 0x84",
    "  mov eax, offset MB_PML4",
    "  mov cr3, eax",
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",
    "  mov eax, offset MB_PML4",
    "  mov cr3, eax",
    "  mov ecx, 0xC0000080",
    "  rdmsr",
    "  or eax, 0x100",
    "  wrmsr",
    "  mov eax, cr0",
    "  or eax, 0x80000000",
    "  mov cr0, eax",
    "  .byte 0xEA",
    "  .long mb64",
    "  .word 0x08",
    // ── 64-bit ──
    ".code64",
    "mb64:\n  mov dx, 0x3f8\n  mov al, 0x46\n  out dx, al",
    "  mov ax, 0x10",
    "  mov ds, ax",
    "  mov es, ax",
    "  mov ss, ax",
    "  mov esp, offset mb_stack_top",
    "  xor ebp, ebp",
    "  call rust_main64",
    "mb_halt:",
    "  hlt",
    "  jmp mb_halt",
);

/// The real 64-bit test body.
#[unsafe(no_mangle)]
extern "C" fn rust_main64() -> ! {
    early_console::write_str("R64-START\n");
    early_console::write_str("### test_smp_ap_alive-mb (x86_64): S-3d via multiboot\n");
    early_console::write_str("STEP1\n");

    // Enable LAPIC (spurious = enable + vector 0xFF).
    early_console::write_str("STEP2\n");
    unsafe {
        let spur = (0xFEE0_0000_usize + 0xF0_usize) as *mut u32;
        spur.write_volatile(0x1FF);
    }
    early_console::write_str("STEP3\n");

    // Trampoline identity page-table root (<4GB, directly usable as the AP
    // ladder's CR3).
    let root = core::ptr::addr_of!(MB_PML4) as u64;

    // Build the shared protection instance (all 8 TSS descriptor addresses
    // are fixed here) and, before the SIPI, write sp0 + the cpu-id stamp
    // into the AP slot with set_kernel_stack. The BSP slot's stack top is a
    // stand-in (this test's BSP keeps using the trampoline GDT; the sp0
    // value only has to satisfy the layout); the asm label mb_stack_top is
    // not in the Rust symbol table, so an AP_STACK page stands in.
    let bsp_stack_top = core::ptr::addr_of!(AP_STACK) as usize as u64 + 0x8000;
    let ap_stack_top = core::ptr::addr_of!(AP_STACK) as usize as u64 + 0x10000;
    AP_STACK_TOP.store(ap_stack_top, Ordering::Release);
    unsafe {
        let prot = X86_64Protection::init(0, VirBytes::new(bsp_stack_top));
        *core::ptr::addr_of_mut!(PROTECTION) = Some(prot);
        // SAFETY: single-threaded boot phase — the AP has not been woken.
        let prot = &mut *core::ptr::addr_of_mut!(PROTECTION);
        if let Some(prot) = prot {
            prot.set_kernel_stack(1, VirBytes::new(ap_stack_top));
        }
    }
    early_console::write_str("STEP3.5\n");

    // Install the ladder blob @0x5000 and fill bootstrap @0x6000.
    unsafe { install_at(0x5000) };

    let record = ApBootstrap {
        logical_id: 1,
        _pad: 0,
        hw_id: 0x11,
        page_table_root_pa: root,
        kernel_stack_top_va: (core::ptr::addr_of!(AP_STACK) as usize as u64) + 0x10000,
        rust_entry_va: ap_entry as u64,
    };
    early_console::write_str("STEP4\n");
    unsafe { minix_arch::x86_64::ap_early_entry::fill_bootstrap(0x6000, &record); }
    early_console::write_str("STEP5\n");
    // Diagnostic: read back the GDT page (6 bytes of desc@0x6030 + 8 bytes
    // of the code32 entry@0x6120).
    early_console::write_str("GDT DESC@6030: ");
    for off in 0x30..0x36 {
        let b = unsafe { core::ptr::read_volatile((0x6000 + off) as *const u8) };
        early_console::write_hex(b as u64);
        early_console::write_str(" ");
    }
    early_console::write_str("\nGDT C32@6120: ");
    // code32 entry = GDT base 0x6110 + selector 0x10 = 0x6120 (an earlier
    // revision misread 0x6130 = the data entry, mistaking data bytes for a
    // correct code32 entry).
    for off in 0x10..0x18 {
        let b = unsafe { core::ptr::read_volatile((0x6110 + off) as *const u8) };
        early_console::write_hex(b as u64);
        early_console::write_str(" ");
    }
    early_console::write_str("\n");

    // Clear the rung markers and send INIT-SIPI.
    unsafe { core::ptr::write_volatile(0x6F00 as *mut u8, 0); }
    early_console::write_str("STEP6\n");
    <X86_64SmpArch as SmpArch>::boot_ap(1, 0x5000);
    early_console::write_str("STEP7 (SIPI sent)\n");

    // Observe the handshake.
    let mut spins: u64 = 0;
    while BOOT_ACK.load(Ordering::Acquire) == 0 {
        spins += 1;
        if spins > 20_000_000 {
            fail("AP did not publish boot_ack within timeout");
        }
        core::hint::spin_loop();
    }

    // Verdict: the AP's GS cpu_id readback must == 1 (the value is visible
    // only through the AP's own GS_BASE MSR — a successful readback proves
    // the per-CPU MSR/GS wiring holds on that core).
    let readback = AP_CPU_ID_READBACK.load(Ordering::Acquire);
    early_console::write_str("### AP cpu_id readback = ");
    early_console::write_hex(readback);
    early_console::write_str("\n");
    if readback != 1 {
        early_console::write_str("### FAIL: AP cpu_id readback != 1\n");
        loop { unsafe { asm!("cli", options(nomem, nostack)); } }
    }
    early_console::write_str("### TEST_RESULT: PASS test-smp-ap-alive-mb ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}
