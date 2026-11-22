# 02-stage 01 R1 fact-check（同 agent 复核，MANUAL_FALLBACK）

## C 入口行号
61:static int map_service(struct rprocpub *rpub);
67:static void sef_local_startup(void);
79:static int is_first_time(void)
93:int main(void)
219:static void sef_local_startup(void)
262:static struct vmproc *init_proc(endpoint_t ep_nr)
331:static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
755:static int map_service(struct rprocpub *rpub)

## init_vm 步骤
6:	extern void __minix_init(void);
15:	if(OK != (s=sys_getkinfo(&kernel_boot_info))) {
28:	get_mem_chunks(mem_chunks);
31:	memset(vmproc, 0, sizeof(vmproc));
38:	acl_init();
41:	map_region_init();
44:	mem_init(mem_chunks);
47:	init_proc(VM_PROC_NR);
48:	pt_init();
53:	__minix_init();
62:		mem_add_total_pages(len/VM_PAGE_SIZE);
68:	mem_add_total_pages((kern_dyn + kern_static)/VM_PAGE_SIZE);
71:	for (ip = &kernel_boot_info.boot_procs[0];
72:		ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
79:		/* VM has already been set up by the kernel and pt_init().
110:	CALLMAP(VM_MMAP, do_mmap);
151:	num_vm_instances = 1;

## Rust BootParams 字段
    pub root_paddr: PhysBytes,
    pub kerninfo_pa: PhysBytes,
    pub total_pages: usize,
    pub free_regions: &'a [BootMemRegion],
    pub boot_procs: &'a [BootImage],
    pub modules: &'a [BootModule],
    pub kernel_allocated: KernelAllocated,
    pub vm_allocated_bytes: u64,
    pub is_first_time: bool,
    pub kernel_layout: Option<minix_types::KernelLayout>,
    pub user_sp: VirBytes,
    pub fn simple(total_pages: usize, free_regions: &'a [BootMemRegion]) -> Self {

## Rust init 阶段
        crate::global::register_page_alloc(&mut self.ctx.page_alloc);
        //   main.c:474-475  init_proc(VM_PROC_NR)+pt_init → init_vm_slot() + init_vm_self_pt() (new)
        //   main.c:480      __minix_init()              → mark_initialized() (real trap wiring → E1)
        //   main.c:485-495  mem_add_total_pages()       → account_boot_memory()
        //   main.c:497-520  boot procs (exec_bootproc)  → init_boot_procs() + exec_bootproc (stack frame → E-BOOTFRAME)
        //   main.c:577-579  VM instance mark            → mark_vm_instance()
        // Phase 1: Memory detection — initialize global state with total page count.
        self.init_global_state();
        // Phase 2a: init_proc(VM_PROC_NR) — main.c:474.
        self.init_vm_slot();
        // Phase 2b: mem_add_total_pages() call points — main.c:485-495.
        self.account_boot_memory();
        // and exec_bootproc consumes page_frames for segment pages. The
        // former placement after init_boot_procs() was an unreachable-order
        // latent bug that forensics exposed ("page_frames not
        self.ctx.page_frames = Some(PageFrames::new(total_phys));
        if let Some(frames) = self.ctx.page_frames.as_mut() {
        // Phase 2c: boot process slots — main.c:497-520 (exec_bootproc
        self.init_boot_procs();
        // Phase 2d: VM instance mark — main.c:577-579.
        self.mark_vm_instance();
        self.transport.borrow_mut().mark_initialized();

## Rust extra_pages
    pub fn extra_pages(&self) -> usize {
        let mut pages: u64 = 0;

        // C: main.c:485-489 charges every entry in `module_list` except
        // the kernel pseudo-entry appended by pre_init. The handoff list is
        // already "every module except VM's" and contains no kernel entry
        // (vm_handoff.rs), so every entry here is a real reserved module;
        // the old `saturating_sub(1)` dropped a real module.
        for m in self.modules {
            let len = round_up_page(m.len);
            pages += len / CLICK_SIZE as u64;
        }

        // The VM image is the one boot module the handoff list omits; C's
        // module loop charged it because VM's module was still in
        // `kernel_boot_info.module_list`. `vm_allocated_bytes` is the
        // kernel-reported size of that image.
        pages += round_up_page(self.vm_allocated_bytes) / CLICK_SIZE as u64;

        // C: main.c:492-495 — kernel_allocated_bytes_dynamic is already
        // page-rounded by the kernel; static is rounded up here.
        let kern_static = round_up_page(self.kernel_allocated.static_bytes);
        pages += (self.kernel_allocated.dynamic_bytes + kern_static) / CLICK_SIZE as u64;

        pages as usize
    }
}


## 测试基线
test result: ok. 549 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
