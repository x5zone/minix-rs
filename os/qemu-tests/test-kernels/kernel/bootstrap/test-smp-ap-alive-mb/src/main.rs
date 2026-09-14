//! Test: AP alive verification (SMP bring-up S-3d, ladder L2) — x86_64,
//! **multiboot 直启变体**（QEMU `-kernel`，绕开 OVMF；见 smp_todo.md S-3d
//! V13 续诊：OVMF(TCG) 的 MpInitLib AP 停放/跳表区与低内存 trampoline 冲突，
//! 且存在 #UD 游走核）。multiboot 路径下 AP 天然 wait-for-SIPI，无固件干扰。
//!
//! BSP：multiboot 进入（32 位）→ 32→64 跳板（恒等页表 + GDT）→ rust_main64：
//! 开 LAPIC → 建恒等页表（跳板已建）→ install blob @0x5000 → fill bootstrap
//! @0x6000 → INIT-SIPI。AP：爬 16→32→64 阶梯（AP_STAGE_MARK 逐级写 0x6F00）
//! → ap_entry → BOOT_ACK。PASS = BSP 观察到 ack 位。

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU64, Ordering};
use minix_arch::arch::ap_early_entry::ApBootstrap;
use minix_arch::smp::SmpArch;
use minix_arch::x86_64::ap_early_entry::install_at;
use minix_arch::x86_64::smp::X86_64SmpArch;
use minix_plat::x86_64::early_console;

// ── 跳板页表（32 位入口填写；.bss 由加载器清零）──
// CR3 装载要求 4KB 对齐（bits 11:0 保留，非对齐即 #GP——S-3d 跳板首障）。
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

/// AP 专属栈（恒等，<4GB）。
#[repr(align(16))]
struct ApStack(pub [u8; 0x10000]);
static mut AP_STACK: ApStack = ApStack([0u8; 0x10000]);

/// Bit n set == AP logical_id n reached the Rust entry.
static BOOT_ACK: AtomicU64 = AtomicU64::new(0);

// 本测试零堆；此空分配器仅为满足依赖链的 alloc 符号需求（永不调用，
// 调用即空指针——若未来真出现分配，链接期后的首次分配会立刻暴露）。
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
    let stage = unsafe { core::ptr::read_volatile(0x6F00 as *const u8) };
    let ip = unsafe { core::ptr::read_volatile(0x6F04 as *const u16) };
    let cs = unsafe { core::ptr::read_volatile(0x6F06 as *const u16) };
    early_console::write_str("### AP stage: 0x");
    early_console::write_hex(stage as u64);
    early_console::write_str(" #UD at CS:IP = 0x");
    early_console::write_hex(((cs as u64) << 16) | ip as u64);
    early_console::write_str("\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC\n");
    // 不读 loc.file()（S-3d：字符串指针可能坏 → FF 风暴），只打行号。
    if let Some(loc) = info.location() {
        early_console::write_str("### PANIC line: ");
        early_console::write_hex(loc.line() as u64);
        early_console::write_str("\n");
    }
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

// ── multiboot 头 + 32→64 跳板 ──
core::arch::global_asm!(
    // 多重启动头：AOUT kludge（位16）——QEMU -kernel 的 multiboot 装载器只认
    // 32 位 ELF，kludge 使其按"文件镜像 → load_addr"整体装载并以 header 的
    // entry_addr 进入（mb.ld 提供 mb_load_end/mb_bss_end/mb_entry 边界符号；
    // .bss 由装载方按 [load_end, bss_end) 清零）。镜像前 8KB、4 字节对齐。
    ".section .multiboot_header, \"a\"",
    ".align 4",
    ".long 0x1BADB002",
    ".long 0x00010003",          // 位0 页对齐模块 + 位1 mem_info + 位16 AOUT kludge
    ".long -(0x1BADB002 + 0x00010003)",
    ".long 0x100000",            // header_addr：.mbh 的运行地址（kludge seek 补偿后 = 0x100000）
                                 // （QEMU 以 seek = 头文件偏移 − (header_addr − load_addr) 装载；
                                 //  写成 = load_addr 会使整幅镜像 +0x1000 错位——S-3d mb 变体的最后一个根因）
    ".long 0x100000",            // load_addr（文件首字节装载于此）
    ".long mb_load_end",         // load_end_addr（文件背书内容终点）
    ".long mb_bss_end",          // bss_end_addr（[load_end, bss_end) 清零）
    ".long mb_entry",            // entry_addr（32 位 _start）
    "",
    // AP 用的 64 位 GDT（null / code64 / data64）+ 描述符。
    ".section .mb_gdt, \"a\"",
    ".align 8",
    "mb_gdt:",
    "  .quad 0",
    "  .quad 0x00AF9A0000000000", // code64: P1 DPL0 S1 Type1010 L1
    "  .quad 0x00CF920000000000", // data64: P1 DPL0 S1 Type0010 W, 4K G D0
    "mb_gdt_desc:",
    "  .word 0x17",               // limit = 3*8-1
    "  .long mb_gdt",             // base <4GB（低链接保证）
    "",
    // 32 位多重启动入口（QEMU multiboot：EAX=0x2BADB002 EBX=info，平坦段，分页关）。
    ".section .text",
    ".code32",
    ".globl _start",
    "_start:",
    "  cli\n  mov byte ptr [0x6F00], 0x81",
    "  mov esp, offset mb_stack_top",
    // 恒等页表：PML4[0]=PDPT|3；PDPT[0..4]=PD0..3|0x83；
    // 每个 PD 512 项 2MB 页（低 dword = 基址|0x83，高 dword 已清零）。
    "  mov eax, offset MB_PDPT",
    "  or eax, 3",
    "  mov [offset MB_PML4], eax",
    "  mov eax, offset MB_PD0",
    "  or eax, 3",   // PDPT 非叶表项：P|RW，无 PS（PS 在 PDPT 是 1GB 页，qemu64 无此能力）
    "  mov [offset MB_PDPT], eax",
    "  mov eax, offset MB_PD1",
    "  or eax, 3",   // PDPT 非叶表项：P|RW，无 PS（PS 在 PDPT 是 1GB 页，qemu64 无此能力）
    "  mov [offset MB_PDPT + 8], eax",
    "  mov eax, offset MB_PD2",
    "  or eax, 3",   // PDPT 非叶表项：P|RW，无 PS（PS 在 PDPT 是 1GB 页，qemu64 无此能力）
    "  mov [offset MB_PDPT + 16], eax",
    "  mov eax, offset MB_PD3",
    "  or eax, 3",   // PDPT 非叶表项：P|RW，无 PS（PS 在 PDPT 是 1GB 页，qemu64 无此能力）
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
    // GDT（含 64 位码段）→ PAE → EFER.LME → PG → 远跳 64 位。
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
    // ── 64 位 ──
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

/// 真正的 64 位测试体。
#[unsafe(no_mangle)]
extern "C" fn rust_main64() -> ! {
    early_console::write_str("R64-START\n");
    early_console::write_str("### test_smp_ap_alive-mb (x86_64): S-3d via multiboot\n");
    early_console::write_str("STEP1\n");

    // 开 LAPIC（spurious = enable + vector 0xFF）。
    early_console::write_str("STEP2\n");
    unsafe {
        let spur = (0xFEE0_0000_usize + 0xF0_usize) as *mut u32;
        spur.write_volatile(0x1FF);
    }
    early_console::write_str("STEP3\n");

    // 跳板恒等页表根（<4GB，AP 阶梯的 CR3 直接可用）。
    let root = core::ptr::addr_of!(MB_PML4) as u64;

    // 安装阶梯 blob @0x5000，填充 bootstrap @0x6000。
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
    // S-3d 诊断：GDT 页回读（desc@0x6030 的 6 字节 + code32 表项@0x6120 的 8 字节）。
    early_console::write_str("GDT DESC@6030: ");
    for off in 0x30..0x36 {
        let b = unsafe { core::ptr::read_volatile((0x6000 + off) as *const u8) };
        early_console::write_hex(b as u64);
        early_console::write_str(" ");
    }
    early_console::write_str("\nGDT C32@6120: ");
    for off in 0x20..0x28 {
        let b = unsafe { core::ptr::read_volatile((0x6110 + off) as *const u8) };
        early_console::write_hex(b as u64);
        early_console::write_str(" ");
    }
    early_console::write_str("\n");

    // 清级标，发 INIT-SIPI。
    unsafe { core::ptr::write_volatile(0x6F00 as *mut u8, 0); }
    early_console::write_str("STEP6\n");
    <X86_64SmpArch as SmpArch>::boot_ap(1, 0x5000);
    early_console::write_str("STEP7 (SIPI sent)\n");

    // 观察握手。
    let mut spins: u64 = 0;
    while BOOT_ACK.load(Ordering::Acquire) == 0 {
        spins += 1;
        if spins > 20_000_000 {
            fail("AP did not publish boot_ack within timeout");
        }
        core::hint::spin_loop();
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-ap-alive-mb ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}
