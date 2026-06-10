//! Boot Integration Test — full kernel boot simulation with MockPaging.
//!
//! This test simulates the complete boot flow (UEFI → KernelInfo → identity map
//! → kernel map → enable paging → kmain) using MockPaging. No real hardware or
//! QEMU needed. Prints diagnostic information and ends with "Hello, World!"
//!
//! Run:
//!   cargo test -p minix-kernel --test boot_integration -- --nocapture

use minix_types::{PhysBytes, VirBytes, MemoryRegion, KernelInfo};

#[test]
fn boot_simulation_full_flow() {
    // ── Set up simulated hardware (what UEFI would provide) ──

    let memmap: &'static [MemoryRegion] = &[MemoryRegion {
        base: PhysBytes(0x100000), // 1MB, conventional RAM start
        len:  0x2000000,           // 32MB
    }];

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000), // x86-64 kernel half
        kern_phys_base: PhysBytes(0x100000),              // loaded at 1MB
        kern_size: 0x400000,                              // 4MB kernel
        free_upper_idx: 256,                              // user space PML4 idx 0-255
        user_sp: VirBytes(0x0000_7fff_ffff_f000),         // user stack top
        kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),   // kernel stack top
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),    // syscall entry point
        boot_modules: &[],                                // no boot modules yet
    };

    let root_page = PhysBytes(0x1000); // physical page for PML4

    // ── Print setup diagnostics ──

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║     Minix-RS Kernel Boot Integration Test          ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();
    println!("── Simulated Hardware ──");
    for (i, region) in kernel_info.memmap.iter().enumerate() {
        println!("  memmap[{}]: base=0x{:016x} len=0x{:x} ({} MB)",
            i, region.base.0, region.len, region.len / (1024 * 1024));
    }
    println!("  kernel: phys=0x{:016x} virt=0x{:016x} size={} KB",
        kernel_info.kern_phys_base.0, kernel_info.kern_virt_base.0,
        kernel_info.kern_size / 1024);
    println!("  root_page: phys=0x{:x}", root_page.0);
    println!("  user_sp: 0x{:016x}", kernel_info.user_sp.0);
    println!("  free_upper_idx: {}", kernel_info.free_upper_idx);

    // ── Run boot flow step-by-step (instrumented version of arch_boot_impl) ──

    use minix_arch::paging::mock::MockPaging;
    use minix_arch::paging::{Paging, PageFlags};
    use minix_arch::paging_ext::HugePages;

    println!();
    println!("── Step 1: Create empty page table root ──");
    let mut paging = MockPaging::new_from_page(root_page);

    let huge_size = MockPaging::HUGE_PAGE_SIZE as usize;
    println!("  PAGE_SIZE={} huge_granularity={} KB",
        MockPaging::PAGE_SIZE, huge_size / 1024);

    println!();
    println!("── Step 2: Identity mapping (C: pg_identity) ──");
    let mut identity_pages = 0u64;
    for region in kernel_info.memmap {
        let mut addr = region.base.0;
        let end = addr + region.len as u64;
        while addr < end {
            paging.map_huge(VirBytes(addr), PhysBytes(addr), huge_size, PageFlags::read_write()).unwrap();
            addr += huge_size as u64;
            identity_pages += 1;
        }
    }
    println!("  identity_pages: {}", identity_pages);

    // Verify mapping
    let verify_addr = kernel_info.memmap[0].base.0;
    match paging.query(VirBytes(verify_addr)) {
        Some((phys, flags)) => {
            println!("  verify: vaddr=0x{:x} → padddr=0x{:x} flags={:?}", verify_addr, phys.0, flags);
        }
        None => panic!("identity mapping verification failed"),
    }

    println!();
    println!("── Step 3: Kernel high-address mapping (C: pg_mapkernel) ──");
    let mut kernel_pages = 0u64;
    let mut offset = 0u64;
    while offset < kernel_info.kern_size {
        paging.map_huge(
            VirBytes(kernel_info.kern_virt_base.0 + offset),
            PhysBytes(kernel_info.kern_phys_base.0 + offset),
            huge_size, PageFlags::kernel_read_write(),
        ).unwrap();
        offset += huge_size as u64;
        kernel_pages += 1;
    }
    println!("  kernel_pages: {}", kernel_pages);

    // Verify kernel mapping
    match paging.query(VirBytes(kernel_info.kern_virt_base.0)) {
        Some((phys, flags)) => {
            println!("  verify: vaddr=0x{:x} → padddr=0x{:x} flags={:?}",
                kernel_info.kern_virt_base.0, phys.0, flags);
        }
        None => panic!("kernel mapping verification failed"),
    }

    println!();
    println!("── Step 4: Enable paging (C: pg_load + vm_enable_paging) ──");
    let root_phys = unsafe { paging.enable() };
    println!("  root_phys: 0x{:x}", root_phys.0);

    println!();
    println!("── Summary ──");
    println!("  identity_pages:  {}", identity_pages);
    println!("  kernel_pages:    {}", kernel_pages);
    println!("  total_mapped:    {} × {}KB = {} MB",
        identity_pages + kernel_pages,
        huge_size / 1024,
        (identity_pages + kernel_pages) * huge_size as u64 / (1024 * 1024));
    println!("  arch:            x86-64 (mock, no real HW)");
    println!();
    println!("  Hello, World! 🎉");
    println!();
}
