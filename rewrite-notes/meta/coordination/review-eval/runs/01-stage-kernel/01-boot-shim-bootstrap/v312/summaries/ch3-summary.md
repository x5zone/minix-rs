# 第 3 章摘要：Rust 设计决策

## 本章回答什么

三条跨镜像腿（x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim）与两个直核腿（riscv64、aarch64）的固件接口各不相同。本章用一套按值交接的 `KernelInfo`、两张带来源记录的内存表和一条校验门链，把五条生产腿收敛到同一个接收方，同时不把硬件差异带进内核。实现细节归第 4 章。

## 协议换代与接线状态（§3.1，第 17–65 行）

- C 依赖 Multiboot：引导器读内核与模块、填 `mbi`、寄存器交地址，内核不做 IO。Rust 三架构里后两个没有 Multiboot 等价物，UEFI 只提供 Boot Services 与页/池分配，不替谁加载内核。
- 分工被重切：UEFI 腿由 boot-shim 自己读文件、取固件内存图、分成两表、在 `ExitBootServices` 前完成所有文件访问；U-Boot 腿由 `fatload` 预载、shim 按 `BootFileTable` 查地址；直核腿从 DTB 取内存、从表/DTB 取模块。
- 被否方案：Multiboot2（两个新架构没有等价协议）、各架构独立引导（分叉的是架构名不是语义）、UEFI 形态内核自带 stub（违反依赖方向）。采纳 opensbi 形态的内核自带 stub：它是 riscv64/aarch64 直核自引导，只反向依赖 boot-shim 的库函数，不让内核依赖 `uefi`。
- 门不证明什么：riscv64 `bootface` 门只验 DTB 表址与模块定位失败，aarch64 门判 SMP 交付，直载测试链不是生产腿。
- 接线状态以 §3.1.1 表为唯一权威：两条 UEFI 产线与 riscv64 U-Boot shim 已接线；riscv64 UEFI shim 因工具链无 riscv64 UEFI 目标未接线；riscv64 直核是辅助形态；aarch64 直核是测试通道。两个 aarch64 产物按编译期分叉（`fw-aarch64-none` 直核、`fw-aarch64-uefi-recv` 接收），不再靠 `x0 == 0` 运行期判别。
- 这条编译期分叉是设计裁决：`x0 == 0` 不是固件承诺，运行期判反的症状是 `handoff v1 envelope invalid` panic，指向协议而非入口形态。

## 按值交接：`KernelInfo` 与 wire（§3.2，第 66–274 行）

- P-1：`KernelInfo` 按值交接，不做跨镜像 ABI。跨镜像腿把固定宽度标量与字节分区写进 arena，接收侧解码后落自己的 `.bss`；直核腿由发送侧 `BootReport<'_>` 借用、接收侧落 `.bss`。`&'static` 只保证生命周期，不保证页在对方页表里被映射。
- 现役 `KernelInfo` 10 个字段。字段计数链条 16 → 13（删 `free_upper_idx`/`bootstrap_start`/`bootstrap_len`）→ 14（加 `authority`）→ 10（wire v10 删 `startup_objects`/`user_sp`/`kern_stack_top`/`syscall_entry`）。
- 10 个字段按与 C 的关系分五类：保留同名（`memmap`）、改型（`param_buf` 从定长数组变切片；`module_list[]` → `boot_modules`）、改名（`vir_kern_start` → `kern_virt_base`）、升为字段（`kern_phys_base`、`kern_size`）、新增（`reserved_regions`、`source_records`、`platform_sources`、`authority`）。
- wire v10：信封 128 B 在首部，不算分区；六个分区是 memmap、reserved、modules、platform sources、`param_buf`、source records。`ARENA_CAP = 32768`、尾部余量 3584 B；`BOOT_HANDOFF_V1_VERSION` 9→10，收发精确比较，v9/v10 互拒。描述符只记 `byte_len`，地址由 arena 基址加偏移推导，wire 上没有指针。
- 两张表：`memmap` 是可分配候选、不保证已扣全部占用区；`reserved_regions` 是保留但可经 Direct Map 访问。内核镜像在直核/U-Boot 腿的表内，两条 UEFI 腿在动作 2 的读/预订步用 `AllocateAddress` 预订、落 `ReservedMapped`；模块 bump 页同理，DTB 来源腿在表内、UEFI 腿不在；无 `WRITE_BACK` 时落 `Side`，不进两张表。
- P-2：`RamAuthority` 把“图和它的声明”变成同一个值，只有 `DTB`/`UEFI` 两个输入对象；`validate_requiring_sources` 不再有两个可接反的实参；限度是构造点仍可传错。UEFI 腿发送侧声明 `InputObjectId::UefiMemoryMap`，U-Boot/直核腿声明 `DtbBootInput`。
- `entry_point` 不进 `KernelInfo`/wire：boot-shim 从 ELF `e_entry` 取并断言等于 `kern_virt_base`，接收侧由 `kern_virt_base` 进入。
- 发送侧的统一形状是 `BootReport<'_>` 九字段视图（`KernelInfo` 去掉 `authority`）；权威由腿在调用点提供，接收侧再合成唯一 `KernelInfo`。

## 来源记录（§3.3，第 275–282 行）

- P-3/P-4：两张表分离，`SourceRecord` 11 字段带 provenance（`id`、`kind`、`origin`、`range`、`input_object`、`local_source`、`snapshot_id`、`attribute`、`virtual_start`、`address_domain`、`raw_type`）。
- 覆盖审计的判据是两侧：free 表每个字节要被 `kind = Memory` 的记录解释，reserved 表每个字节要被 reservation 类记录解释。`audit_source_coverage` 与 strict 版 `audit_source_coverage_for` 是两个入口；strict 分支按 `authority` 过滤，再查模块名/位置与内核跨度。覆盖缺口报 `FreeSpanWithoutMemorySource`，启动 fail-closed。C 的 `memmap` 只有 base/len/type，I2 的缺口由这层补。

## bootstrap、分配器与页目录槽位（§3.4，第 283–368 行）

- `bootstrap_start/len` 最终从 `KernelInfo` 删除：Rust 生产镜像没有“分页前执行、分页后可归还”的独立内核段；`bootface` 代码虽只在分页前执行，但没有独立链接边界。所有生产链接布局都是高半 VMA；QEMU 测试内核按 VMA=LMA 平铺，不属于可归还段。
- P-6：页表页分配器换手。`init_boot_pt_alloc` 只设定 bump 区间；接收侧 `BootAlloc::resume` 续游标、不重发页；`pt_alloc::register` 写一次，`boot_validate_and_prepare` 用 `is_registered()` 守卫。两个 bump 实例不共享游标。
- `pt_alloc` 注册槽是“写一次”的：内核/VM 侧先用 `is_registered()` 自检，重复注册是 bug，没有“后注册者覆盖前者”的语义。
- `free_upper_idx`/页目录槽位是 C 的借槽机制，64 位侧没有对应物；`GET_KINFO` 的 `freepde_start` 布局位固定填 0。

## 取件抽象与依赖方向（§3.5，第 369–450 行）

- P-7/P-8：取件抽象是一条普通函数 `prepare_boot<'a>(&'a mut BootStorage, bump_pages) -> BootPrepareResult<'a>`，差异收在 `FileLoader`；`FileLoader` 有 UEFI 与 U-Boot 两个实现，`read_required` 把“文件不存在”变成带路径的 panic，`read` 只返回 `Option`。
- crate 方向单向：`kernel-image` 的直核面反向依赖 boot-shim 的库函数，`minix-kernel` 与 `kernel-image` 都不依赖 `uefi`；`kernel-image` 只在 riscv64/aarch64 target 下加 `boot-shim`/`fdt`。

## 五条生产腿与门链（§3.6–§3.7，第 451–586 行）

- 五条腿：两条 UEFI 链（x86_64、aarch64）、一条 riscv64 U-Boot shim、两个直核面（riscv64 `bootface`、aarch64 `bootface_a64`）。发送侧都只构造 `BootReport<'_>`；UEFI 腿的 `source_records` 切片为空，source 分区权威在 wire；接收侧统一落 `.bss` 后构造唯一 `KernelInfo`。
- 门链按腿不同：直核面 strict → 建表前门 → postconditions → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 strict 与 postconditions 复核；v1 面 strict → postconditions → `resume_boot_pt_alloc` → `boot_validate_and_prepare` → `store_kernel_info` → `establish_boot_dm` → `jump_to_kmain` → `kmain` 的 strict 与 postconditions 复核，不经过建表前门。
- P-5：`BootStorage` 单所有者，`fill_*` 在闭包返回时提交计数，半提交不可表示；UEFI 腿的 source 分区由两阶段直写 wire，storage 的 source 切片为空。
- `BootStorage` 承载六类载荷（内存两表、模块、平台源、参数、source 记录）；`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自一次提交。
- I1–I4 落点：I1 由 wire 固定宽度与落 `.bss` 承载；I2 由 `SourceRecord`/覆盖审计承载；I3 由 `BootStorage` 单所有者与 `fill_*` 承载；I4 由门链承载。本章不写建表/映射细节，那归第 4 章与 02 篇；也不重复第 2 章的 C 行为，只引用其结论作设计输入。

- 本章不给代码走读与建表步骤；页表建立、九动作、六个落地区与门链的逐行落点在第 4 章。
