# 第 4 章摘要：实现详解

## 本章回答什么

设计定了，代码要在两个镜像、五个入口和一次 `ExitBootServices` 之间落地。本章回答三件事：九件事按什么顺序做、每件事失败落在哪、交接载荷最后被谁深拷进内核自己的 `.bss`；同时交代两条产出入径（直核与跨镜像）、六个落地区与门链的逐腿执行序。

## 入口与调用链（§4.1，第 15–77 行）

- UEFI 入口是 `.efi` 的 `main`，只调 `prepare_and_enter_kernel_image(BOOT_PT_BUMP_PAGES)`；后者返回 `!`，内部三段：准备（读 ELF、预订 LMA、分配 root/bump、拷贝段与 12 模块、平台源/参数、arena 序列化）→ 退出固件服务（配对循环 `GetMemoryMap`/分类/`ExitBootServices`）→ 交接（提交计数、构造并复核 `BootReport`、建根、开分页、`enter_kernel_image_v1`）。
- 跨镜像腿随后进 `arch_boot_resume_v1`：解码六个分区进 `.bss` → strict 校验 → `resume_additive`，最后 `kmain`。直核腿把“跳 `kernel.elf`”换成“同镜像直接调 `arch_boot`”，收敛在 `arch_boot_finish`。
- 文件访问必须在 `ExitBootServices` 之前完成；EBS 之后不再有 startup 写入段，动作 6 之后直接进入动作 7。
- 本章有六套易撞编号：Step 0–4、动作 1–9、时间拍 1–10、防线 A–D、Step 2b/2c、第 2 章的 D1–D5。动作 ID 是稳定标签，不是时间序；动作 2 拆“读/预订”与“拷贝/模块”两段。
- `BOOT_PT_BUMP_PAGES = 1024`（4 MiB）是页表页预算，直核面另用 `PT_BUMP_PAGES = 256`；实际用量记 `kernel_allocated_dynamic`，未用部分按整段扣除，不退回。UEFI 腿的 bump 区不在 conventional 列表里，这条扣除对 free list 是空操作；扣整段防的是自引导等路径把 bump 区记进清单。

## UEFI 九动作与分类（§4.2，第 78–172 行）

- 九个动作按十拍执行：读 ELF/预订 → 分配 root/bump → 拷贝段/装 12 模块 → 平台源/参数 → 八页 arena → 写信封 → `GetMemoryMap`/分类/source 记录/`ExitBootServices` → 提交两张表 → 构造并复核 `BootReport` → 建根/开分页/补字段/跳转。
- 动作 2 的读/预订必须在动作 1 之前：布局要读完 ELF 字节才知道；UEFI 用 `AllocateAddress` 把 `[kern_phys_base, +kern_size)` 整段预订为 `LOADER_DATA`，失败即 halt，避免模块/arena/池缓冲落进镜像跨度。
- `MapKey` 是 `GetMemoryMap` 快照标识；分类必须与 `ExitBootServices` 共用同一个 key。`BUFFER_TOO_SMALL` 只允许 EBS 前扩容，`EFI_INVALID_PARAMETER` 允许重分类一次，其他状态立即失败。
- 描述符分类是五步加两道内联闸：步 1 查类型/属性合法性；步 2 把 `UNACCEPTED`/`PAL_CODE` 判 `RefuseToBoot`；`checked_shape`；步 3 把 `RESERVED`/`UNUSABLE` 判 `Excluded`、`MMIO*` 判 `DevicePath`；`checked_len`；步 4 把 `TYPE_INDEPENDENT` 判 `Side`；步 5 无写回判 `Side`，否则按类型表定 `Available`/`ReservedMapped`/`Side`。整图另拒两两重叠与四类容量上限。
- `pa_limit = min(BOOT_IDENTITY_MAP_END, VM DM 窗口物理上界)`，`BOOT_IDENTITY_MAP_END = 0x1_0000_0000`；`checked_len` 要求 `start + len ≤ pa_limit`。

## 取件函数（§4.3，第 173–232 行）

- 两族文件来源只共享 `FileLoader` 的抽象：UEFI 走 `SimpleFileSystem`，U-Boot 走预载的 `BootFileTable`；`read_required` 把“文件不存在”变成带路径 panic，`read` 只返回 `Option`。
- 模块装载有四道防线 A–D：A/B 守内核段、C 守模块、D 在接收侧。三条 v1 腿的模块顺序没有编译期检查，只能靠装载顺序、运行期断言与测试兜住；两个直核腿由 `place_modules` 在装载时按 `NAME_POOL` 逐槽核对 basename，互换会当场停机。编译期常量只核对池自身（截断/NUL），拦不住顺序互换。
- 生产引导模块数为 12（`NR_BOOT_MODULES`）；模块名槽 `BOOT_MODULE_NAME_LEN = 16`，超长截断不 panic，两条路径的截断点不同。

## 五条腿与发送视图（§4.4，第 233–311 行）

- 五条腿：两条 UEFI 链、riscv64 U-Boot shim、两个直核面。发送侧都只构造 `BootReport<'_>` 九字段视图（`KernelInfo` 去掉 `authority`）；接收侧落 `.bss` 后构造唯一 `KernelInfo`，切片字段为 `&'static [T]`。
- UEFI 腿的 `source_records` 切片为空，source 分区权威在 wire；U-Boot shim 与两个直核腿走 `fill_source_records`。`BootStorage` 的 `fill_*` 在闭包返回时提交计数，半提交不可表示。
- 门链按腿不同：直核面 strict → 建表前门 → postconditions → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 strict 与 postconditions 复核；v1 面 strict → postconditions → `resume_boot_pt_alloc` → `boot_validate_and_prepare` → `store_kernel_info` → `kmain` 的 strict 与 postconditions 复核，跳过建表前门。
- `check_boot_postconditions` 在 v10 后只剩 `kern_virt_base > u32::MAX`；共享校验体 `validate_view` 查两表不相交、内核/模块半开区间不重叠、模块名按 `MODULE_NAMES` 对账。

## 六个落地区与分配器换手（§4.5–§4.6，第 312–375 行）

- 六个落地区 + 一个名字池 + `KernelInfo.memmap` 切片视图 + 两个 `VmBootHandoff` 字段；容量权威表在 §4.5。`RESERVED_REGION_STORE_LEN = 256`、`MEMMAP_REGION_STORE_LEN = 128`、`BOOT_MODULE_NAME_LEN = 16`、`PARAM_BUF_STORE_LEN = 1024`、`SOURCE_RECORD_STORE_LEN` 取 wire 分区容量。
- `store_boot_report`/`store_kernel_info` 把切片深拷进内核 `.bss`，此后只使用副本；`land_and_store` 用一次性守卫，第二次调用直接 panic，不是同址 no-op；同址由指针相等检查跳过拷贝，`ptr::copy` 为部分重叠保留。
- 只做一半深拷（如 `reserved_regions` 拷了、`memmap`/`boot_modules` 仍指原处）会让 free list 退化，第一次物理页分配才暴露；现场离原因隔着整个分类阶段。
- arena 的最后一次读取是 `arch_boot_resume_v1` 的解码：六个载荷全部复制进内核 `.bss` 后，arena 页可回收；`LastRead(arena) ≺ Publish(VMFreePages)` 对会落进 conventional 的腿成立，两条 UEFI 腿的 arena 是 `LOADER_DATA`/`ReservedMapped`，本就不在 free list。
- `validate_before_tables` 不查 `boot_modules` 非空（QEMU 测试内核合法携带空模块表）；模块非空由 strict 门单独查。
- `BootAlloc::init`/`resume` 两种进入方式；跨镜像先 `resume_boot_pt_alloc` 再进委托壳，反序会让 Step 0 看到 `boot_alloc_region() == None` 并 fail-closed。两个 bump 实例不共享游标，共享纯值核心 `BumpRegion`；`pt_alloc::register` 写一次，失败 panic。

## C 对照、Direct Map 与章末（§4.7–§4.12，第 376–440 行）

- §4.7 是 C 函数 → Rust 落点的一表对照；冲突时源码优先，本节与第 2 章 C 行为叙述冲突时以源码为准。
- `arch_boot_finish` 的链是 `store_boot_report` → `arch_boot_impl`（`build_bootstrap_root_and_enable` + `establish_boot_dm`）→ `jump_to_kmain`；v1 面在 `resume_additive` 内是 `boot_validate_and_prepare` → `store_kernel_info` → `establish_boot_dm` → `jump_to_kmain`，不调用 `arch_boot_impl`（活动根只做加法，重建会清零）。
- 能经 Direct Map 访问不等于该页可分配给 VM；VM 自由表还要扣除固件分类、DM 覆盖与启动期占用。完整布局与覆盖策略属独立内存管理主题。
- 失败现场：读文件在 `read_required` panic；直核腿模块名对不上由 `place_modules` 停机，v1 腿没有同名跨二进制检查；EBS 配对失败重试或停机；结构校验在 `boot_validate_and_prepare`/`resume_additive` 断言；建根/开分页在 `build_bootstrap_root_and_enable` halt；`kmain` 门失败在 scheduler 前 panic。

- 本章不写页表建立与映射细节；Direct Map 只给概念入口，完整覆盖策略归独立内存管理主题。
