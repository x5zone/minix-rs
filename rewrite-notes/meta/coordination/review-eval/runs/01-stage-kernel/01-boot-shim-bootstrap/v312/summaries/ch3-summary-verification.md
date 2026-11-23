DOC: 01-boot-shim-bootstrap.md
CHAPTER: 第 3 章 Rust 设计决策
VERDICT: FAITHFUL

SLICE: ch3-slice-candidate.md sha256 d709dcfb72a133b0c8e23a4a5a21f4fb3ef39547b24fa6f991b1126ab814421c 已核对。
① PASS — 10 字段、16→13→14→10 链条与五类关系（保留 `memmap`／改型 `param_buf`+`boot_modules`／改名 `kern_virt_base`／升为字段 `kern_phys_base`+`kern_size`／新增四个）逐项一致。
② PASS — 信封 128 B 明示不算分区，六分区 memmap/reserved/modules/platform_sources/param_buf/sources、ARENA_CAP 32768、余量 3584 B、版本 9→10 一致。
③ PASS — v1 门序 strict→postconditions→resume_boot_pt_alloc→boot_validate_and_prepare→store_kernel_info→establish_boot_dm→jump_to_kmain→kmain 复核，且不经 `validate_before_tables`，一致。
④ PASS — riscv64 bootface 门只验 DTB 表址与模块定位失败、aarch64 门判 SMP 交付、直载测试链不计入生产腿，一致。
⑤ PASS — free 每字节需 `kind = Memory` 记录、reserved 每字节需 reservation 类记录，`audit_source_coverage`/`_for` 两入口与 authority 过滤、fail-closed，一致。
⑥ PASS — `FreeSpanWithoutMemorySource` 与 `handoff v1 envelope invalid` 两处错误名与切片一致。
⑦ PASS — 摘要只写 `init_boot_pt_alloc` 设 bump 区间、`BootAlloc::resume` 续游标、两个 bump 实例不共享游标；未出现 CAS、`BootAlloc::init`、`BumpRegion` 共享。
⑧ PASS — 动作 2 读/预订步用 `AllocateAddress` 预订 `[kern_phys_base, +kern_size)` 并落 `ReservedMapped`，与切片引述一致。
⑨ PASS — `RamAuthority` 只有 `DTB`/`UEFI` 两个输入对象；限度写成“构造点仍可传错”，与切片一致。
