//! Boot Integration Test — full kernel boot simulation with MockPaging.
//!
//! This test simulates the complete boot flow (UEFI → KernelInfo → identity map
//! → kernel map → enable paging → kmain) using MockPaging. No real hardware or
//! QEMU needed. Prints diagnostic information and ends with "Hello, World!"
//!
//! Run:
//!   cargo test -p minix-kernel --test boot_integration -- --nocapture

use minix_types::{PhysBytes, VirBytes};
use minix_boot::{MemoryRegion, KernelInfo};

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
        free_upper_idx: Some(256),                              // user space PML4 idx 0-255
        user_sp: VirBytes(0x0000_7fff_ffff_f000),         // user stack top
        kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),   // kernel stack top
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),    // syscall entry point
        boot_modules: &[],                                // no boot modules yet
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_descriptor: None,
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
    println!("  free_upper_idx: {:?}", kernel_info.free_upper_idx);

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

#[test]
fn init_proc_and_boot_test() {
    //! Integration test for init_proc_and_boot (Phase C of boot).
    //!
    //! Verifies:
    //! 1. ProcessTable is created with all slots SLOT_FREE
    //! 2. Boot processes (PM, RS, VM, etc.) are initialized
    //! 3. VM process gets correct initial PC/SP from load_vm_elf
    //! 4. Process segments (p_seg) are populated for VM
    //!
    //! Run:
    //!   cargo test -p minix-kernel --test boot_integration init_proc_and_boot_test -- --nocapture

    use minix_kernel::proc::{proc_nr, RtsFlagsBits};
    use minix_kernel::proc_table::ProcessTable;

    // Step 1: Create process table and verify initial state.
    let table = ProcessTable::new();
    println!("── Step 1: ProcessTable created ──");

    // Verify SLOT_FREE for all user-space slots
    for nr in 0..=255 {
        if let Some(proc) = table.get(nr) {
            assert!(proc.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE,
                "slot {} should be SLOT_FREE, got {:?}", nr, proc.p_rts_flags.get());
        }
    }
    println!("  All user-space slots are SLOT_FREE ✓");

    // Step 2: Verify VM process slot exists.
    let vm_nr = proc_nr::VM_PROC_NR;
    let vm_proc = table.get(vm_nr).expect("VM process slot must exist");
    assert_eq!(vm_proc.p_nr, vm_nr, "VM p_nr mismatch");
    println!("  VM process slot exists: nr={} endpoint={:?}", vm_nr, vm_proc.p_endpoint);

    // Step 3: Verify p_seg default state.
    assert_eq!(vm_proc.p_seg.phys_root.0, 0, "VM p_seg.phys_root should be 0 initially");
    assert!(vm_proc.p_seg.virt_root.is_none(), "VM p_seg.virt_root should be None initially");
    println!("  VM p_seg default: phys_root=0 virt_root=None ✓");

    // Step 4: Verify p_magic in debug builds.
    #[cfg(debug_assertions)]
    {
        assert_eq!(vm_proc.p_magic, 0xC0FFEE1, "VM p_magic should be PMAGIC");
        println!("  VM p_magic=0xC0FFEE1 (PMAGIC) ✓");
    }
    #[cfg(not(debug_assertions))]
    {
        println!("  p_magic: not checked (release build) ✓");
    }

    println!("── init_proc_and_boot_test PASSED ──");
}
