DOC 01-boot-shim-bootstrap.md

VARIANCE: D-024 D1 停跑，D2 采用文档主张↔源码事实回退

# DESIGN-D2 · 第一遍 C 合同 ↔ 现有 Rust 实现（R3 结构轮）

本报告只读：未读第一遍方案（D-024 停跑），未读其他面板角色输出，未读 `expected/`，未读旧轮面板目录。事实入口为文档 §3/§4、`related-code.txt` 的 48 个文件（按 §7.2/§7.3 符号锚点选择性阅读）与 `trait-audit.json`；trait-audit 的读数逐条手工复核，复核结果见 `## TRAIT-AUDIT-CROSSCHECK`。

## 1. 独立重建的 C 语义合同

先不引用任何 Rust 设计，只把 C 侧必须保住的外部语义写成合同（来源是 §3/§4 引用的 C 事实；不把 C 的内部写法当成合同）：

| 编号 | C 合同 | 由此产生的 Rust 侧硬约束 |
|---|---|---|
| C1 | 引导器把内核与模块读进内存，内核自己不做 IO；参数经 `kinfo_t` 交给内核 | 引导面必须自己完成取件；内核不得依赖固件文件服务 |
| C2 | `kinfo` 是同一地址空间里的全局；`pre_init` 填入、`kmain` 读同一份 | 跨镜像时必须按值深拷；同镜像也不能让引用跨过首次进程根切换 |
| C3 | `memmap` 是可变的 add/cut 记账；内核与模块通过第 M+1 个模块参与重叠检查 | Rust 可以换载体，但“哪些区间被占用、谁先扣”必须等价 |
| C4 | `bootstrap_start/len` 是分页开启前执行、之后可回收的一段内核段 | 只有存在这样一段时才需要这对字段；没有就必须删掉接口面 |
| C5 | `freepde_start` 是当前页表里的可借槽位；跨地址空间拷贝借它开窗口 | 64 位改用 Direct Map + 软件页表走查后，借槽机制和字段都可退场 |
| C6 | 页表页来自 6 页静态池；4KB 数据页来自内存记录尾部；两套池不能混 | 页表页分配器可以换实现，但“页表页 vs 数据页”的责任边界必须保留 |
| C7 | 启动失败的处置是 panic/停机；`GET_KINFO` 返回整个 C `kinfo` 布局 | 错误面以 fail-fast 为主；`GET_KINFO` 的载荷收窄属于对外可观察变化，必须显式标注 |
| C8 | 模块顺序与进程号绑定来自 `image[]`／`com.h`，位置是契约 | 必须用共享权威表钉住顺序与进程号，不能靠两份字面量 |

以这份合同看文档 §3/§4 的主张：绝大多数主张与源码一致；真正的差异集中在“权威/身份的双句柄”和少量源码注释/死状态上，不在五条腿的 `BootReport<'_>` 主结构上。

## DIFF-TABLE

| 维度 | 文档 Ch3/Ch4 主张 | 源码事实 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `KernelInfo` 是 10 字段的纯交接快照；C 的可变 `kinfo` 记账不再保留（§3.2.1、§4.4） | `KernelInfo` 10 字段（`kernel_info.rs:23-152`）；扣减在 `vm_handoff::classify` 用 `FreeRangeSet` 做（`vm_handoff.rs:452-568`） | C 的可变账本被改成“快照 + 消费侧扣减”，文档与源码一致 | 接受 |
| 状态表达 | `BootStorage` 的 `fill_*` 在闭包返回时同时写槽与提交计数，族内半提交不可表示（§3.7） | 五个 `*_committed` 布尔 + 五个长度（`boot_storage.rs:86-106`）；`fill_map_tables` 一次提交两表（`:147-166`） | 族内保证成立；跨族半提交在类型上可表示，文档明说由接收侧门兜住 | 接受（跨族风险见 ILLEGAL-STATES #3） |
| 状态表达 | `ValidationLevel` 三档替代“两个 bool 的四种组合、三种合法”（`kernel_info.rs:154-181`） | 三变体枚举；`requires_boot_modules`/`requires_kernel_span_coverage`/`is_strict` 派生 | 一致；这是“bool 换成 enum”的正例 | 接受 |
| 状态表达 | `bootstrap_start/len` 已删除；`freepde_start` 只作 `GET_KINFO` 布局位、恒 0（§3.4.1、§3.4.3） | `KernelInfo` 无这两个字段；`getinfo_kinfo` 填 0（`misc.rs:711-740`） | 一致；`KinfoStruct` 的 48 B 布局由 `minix-types` 单权威钉住 | 接受 |
| 状态表达 | `PlatformDescSource.len = None` 表示生产者未记录长度；wire 的 0 解码回 `None`（§3.2.2.3） | `Option<NonZeroU32>` + `with_optional_len`（`platform.rs:123-190`）；`WirePlatformDescSource` 的 `len: u32`（`handoff_v1.rs:64-68`） | 一致；`None` 是旧生产者/测试兼容分支，不是当前生产路径 | 接受（legacy 分支保留） |
| 身份 | `KernelInfo.authority` 把“图”和“它的声明”变成同一个值；`BootReport` 没有 authority 字段，权威由腿在调用点提供（§3.2.1） | `KernelInfo.authority` 存在（`kernel_info.rs:151`）；`BootReport` 九字段无 authority（`boot_storage.rs:292-314`）；直核腿在 `run_self_boot` 同时传两次 `DTB`（`bootface_common.rs:552-554`） | 接收侧（v1）确实是一个值；**直核腿的“声明”仍是两条相邻实参**，见 FINDING DESIGN-01 | 需修（P2） |
| 身份 | 五条腿发送侧只构造 `BootReport`；接收侧落 `.bss` 后构造唯一 `KernelInfo`（§3.7、§4.4） | 发送侧：`storage.report(...)`（UEFI/U-Boot/直核各一处）；`KernelInfo` 由 `land_and_store` 写进 `KERNEL_INFO`（`lib.rs:1575-1706`） | 主结构一致；但生产 `kmain` 忽略它的 `&KernelInfo` 参数、改读全局，见 FINDING DESIGN-02 | 需修（P2） |
| 身份 | 模块名是“按位置的运行时身份”，strict 门按 `MODULE_NAMES` 逐位核名（§3.2.1、§4.4.1） | `BOOT_MODULE_IDENTITIES` 单一权威表（`kernel_info.rs:685-698`）；`validate_view` 的 Strict 逐位比较（`:491-498`） | 一致；顺序与进程号没有第二份抄本 | 接受 |
| 所有权 | `BootStorage` 拥有 U-Boot shim 六类载荷；UEFI 腿只拥有五类，source 分区只写 wire（§3.7） | UEFI 入口只调 `fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`（`uefi_helpers.rs:1086-1167`）；U-Boot 另调 `fill_source_records`（`opensbi_helpers.rs:317-320`）；直核调 `fill_source_records`（`bootface_common.rs:539-542`） | 一致；UEFI 的 `BootReport.source_records` 是空切片，`BootReport` 在 UEFI 腿上不是完整视图（文档已声明） | 接受（类型不区分完整/不完整，见 ILLEGAL-STATES #10） |
| 所有权 | `BootReport` 只是借用视图，生命周期绑定 `BootStorage`；`KernelInfo` 由六个 `.bss` 落地区支撑（§3.7、§4.5） | `storage.report(&self) -> BootReport<'_>`（`boot_storage.rs:262-283`）；`land_and_store` 用 `from_raw_parts` 指向六个静态 store（`lib.rs:1602-1702`） | 一致；`land_slice` 的同址跳过只为 v1 接收侧的“已落盘再包装”服务（`lib.rs:1535-1548`） | 接受 |
| 错误 | wire 解码越界/不整除/容量不足在构造任何引用前失败；其余启动错误 panic/停机（§3.2.3、§4.5） | `HandoffV1ReadError` 的 fail-closed 变体（`handoff_v1_read.rs:40-65`）；`element_count`/`decode_*` 先校验再写（`:195-230`）；boot 路径大量 `assert!`/`panic!` | 一致；没有自造 errno | 接受 |
| 错误 | 配对循环：`BUFFER_TOO_SMALL` 只在第一次 EBS 前允许扩容；`EFI_INVALID_PARAMETER` 允许重分类一次；其余立即失败（§4.2.1） | `NeedBiggerBuffer` 仅在 `ebs_attempts == 0` 返回（`ebs_pair.rs:160-166`）；`EBS_ATTEMPT_LIMIT = 2`（`:59`，`:225-231`） | 一致 | 接受 |
| 并发 | Kernel = SMP + BKL；boot 段单线程；注册槽/`KERNEL_INFO` 写一次后只读（§3.4.2、§4.6） | `BootAlloc` 三个 `AtomicU64` 明确标注非线形化、仅 boot 单线程（`boot_alloc.rs:14-22`）；`PT_ALLOC` 用 CAS 写一次（`pt_alloc.rs:55-67`）；`KERNEL_INFO` 一次性守卫（`lib.rs:1583-1586`） | 一致；没有把 boot 单线程假设伪装成可并发 | 接受 |
| 抽象边界 | 没有 boot trait；`prepare_boot` 是自由函数；差异收在 `FileLoader`；内核不依赖 boot-shim 可执行体（§3.5.1、§3.5.2） | `prepare_boot` 自由函数（`opensbi_helpers.rs:241-244`）；`FileLoader` 三个实现（`loader.rs:146-158`、UEFI/U-Boot/Mock）；`os/kernel/Cargo.toml` 无 boot-shim/uefi | 一致；`FileLoader` 确实承担测试接缝与两种字节来源 | 接受 |
| 抽象边界 | `BumpRegion` 是两个 bump 实例共享的纯值核心；`PlacementPool`/`PreparedJump`/`MapCounts` 用命名字段挡住位置错（§3.5.1、§3.6） | `BumpRegion`（`bump.rs:30-105`）；`PlacementPool`（`bootface_common.rs:175-189`）；`PreparedJump`（`lib.rs:409-414, 561-567`）；`MapCounts`（`boot_storage.rs:61-65`） | 一致；这些是“位置实参 → 值对象”的正例 | 接受 |
| 可测试性 | `FileLoader` 的 `MockLoader` 喂文件字节；`BootStorage`/`BootAlloc`/窗口检查有宿主负控（§3.5.1、§4.4.1） | `MockLoader`（`loader.rs:678-692`）；`BootStorage` tests（`boot_storage.rs:316-443`）；`BootAlloc` tests（`boot_alloc.rs:219-359`）；窗口负控（`lib.rs:5940-5990`） | 一致 | 接受 |
| 可测试性 | `KernelInfo::validate` 保留给测试/诊断，`kmain` 调用 strict；`store_kernel_info` 一次性（§4.4.1、§4.5） | `KernelInfo::validate` 的仓内调用点只在自己的 tests（`kernel_info.rs:785-1099`）；`kmain` 用 `validate_requiring_sources`（`lib.rs:1867`）；但方法 doc 仍写“Called by `kmain` at entry”（`kernel_info.rs:247`） | 行为与文档一致，**源码注释与文档/调用不符**，见 FINDING DESIGN-04 | 需修（P3） |
| 可测试性/死状态 | 文档未提 `Serializer` 的内部计数器；wire 的 `byte_len` 由写者提交 | `Serializer` 六个计数器（`handoff_v1_write.rs:99-106`）；`modules_n`/`platform_n`/`param_n` 只赋值、无读取（`:224, :247, :266`） | 死状态；不影响 wire 或行为，见 FINDING DESIGN-03 | 需修（P3） |
| 文档措辞 | §3.7/§4.4.1 把直核 strict 门说成“两个 `bootface` 在调 `arch_boot` 之前显式调用” | 实际调用点在共享的 `bootface_common::run_self_boot`（`:552`），两个 face 只是入口 | 语义不变，措辞把“共享一次”写成“各调一次”；不构成缺陷 | 接受（措辞可精确化） |

## TRANSLATION-RESIDUE

按任务 2 的分类逐项判断。这里只列与本篇 48 个文件相关的项；“只因 C 这样写才存在”不等于“应删”，外部行为与 wire 兼容优先。

| 结构 / 写法 | 只因 C 这样写才存在？ | 分类（任务 2） | 判断与理由 |
|---|---|---|---|
| `KinfoStruct.freepde_start` / `vir_kern_start` 两个布局位 | 是。C 的 `GET_KINFO` 全布局需要这两个位置；Rust 无借槽机制 | 无状态？不，是 C ABI 占位 | 保留。`KinfoStruct` 是 `GET_KINFO` 的外部面，删字段会改系统调用载荷；文档 §3.2.2.2 已标 `[ARCH]` 收窄 |
| `param_buf` 1024 字节定长 | 是。C `MULTIBOOT_PARAM_BUF_SIZE` | 定长 C 通道 | 保留。`GET_MONPARAMS` 按 1024 返回并零填充（`misc.rs:1411-1416`），这是外部行为 |
| `MODULE_NAMES` / `BOOT_MODULE_PROC_NRS` 的顺序与进程号 | 是。C `table.c` 的 `image[]` 与 `com.h` | 位置契约 | 保留。`BOOT_MODULE_IDENTITIES` 是单一权威表，boot-shim 与内核共享（`kernel_info.rs:669-732`） |
| `BootModule.name` 的 16 B 槽、15+NUL 截断 | 是。C 风格定长名字槽与 wire 宽度 | 定长 wire / 字符串状态 | 保留。wire 用 `WireBootModule.name: [u8;16]`（`handoff_v1.rs:50-56`）；截断点两条路径不同但都 fail-safe |
| `PlatformDescSource.len = None`（wire 0 = 未知） | 部分是。旧生产者只给地址、不给长度 | 哨兵/兼容分支 | 保留。当前生产腿都用 `with_optional_len` 给长度；`None` 只在测试/旧 wire 出现。删它要升 wire 版本，收益低 |
| `ZERO_REGION` / `ZERO_MODULE` / `ZERO_SOURCE` / `ZERO_RECORD` | 否。Rust 数组初始化需要值；C 侧没有这些常量 | 哨兵返回值 | 保留。它们只通过 live prefix 读取（`boot_storage.rs:40-55, 231-259`）；换成 `MaybeUninit` 会增加 unsafe，不划算 |
| `EMPTY_PHYS_RANGE`（`BootWindows` 未用槽） | 否。Rust `BootWindows` 的固定数组填充值 | 哨兵返回值 | 保留。`as_slice` 只暴露已提交前缀（文档 §4.4.1；源码 `lib.rs` 的窗口实现），不会把哨兵当窗口 |
| `BootStorage` 五个 `*_committed` 布尔 | 否。Rust 一次性提交守卫；C 没有 | 布尔状态机 | 保留。每个家族一个 bool 是拒绝二次 `fill_*` 的最小手段；五 bool 的组合大多无意义，但跨族半提交由接收侧门兜住（文档 §3.7 已声明） |
| `Serializer` 的 `modules_n` / `platform_n` / `param_n` | 否。Rust 内部计数残渣 | 死状态 | **删除**。只在 `write_boot_modules`/`write_platform_sources`/`write_param_buf` 赋值，没有任何读取（见 FINDING DESIGN-03） |
| `KernelInfo` / `BootReport` 的 `#[derive(Clone, Copy)]` | 否。Rust 值语义便利 | 不是“clone 到能编译” | 保留。仓内没有 `.clone()` 调用；`Copy` 只是让 `KernelInfo` 可按值传递/测试构造，不改变所有权 |
| `KernelInfoView`（9 个访问器 + 两个 impl） | 否。Rust 侧为共享校验体而设 | 单实现依赖注入 trait？不是：两个真实 impl | 保留。`KernelInfo` 与 `BootReport` 两种表示共用 `validate_view`/`land_and_store`，删 trait 需要引入转换/视图类型，见 SUBTRACTION-TABLE 第 4 行 |
| `BklProtected` / `Sealed` | 否。Rust 的 `SyncUnsafeCell` 声音性门槛 | 标记 trait，不是注入 | 保留。审计读数为 NO-IMPL/SINGLE-IMPL 是宏生成 impl 的假阴性，见 TRAIT-AUDIT-CROSSCHECK |
| `PreparedJump` / `PlacementPool` / `MapCounts` / `ValidatedKernelImage` | 否。Rust 用命名字段替代相邻裸参数 | 值对象 | 保留。它们挡的是“交换两个相邻 `u64` 仍能编译”的调用点错误；文档 §3.6/§3.5.1 的论证与源码一致 |
| `SourceRecord.raw_type` / `virtual_start` / `snapshot_id` 等取证字段 | 否。Rust 的来源层；C 没有 SourceRecord | 取证字段，不是正常路径字段 | 保留。D-111 已冻结 72 B 与 wire v11；取证字段不以正常路径读取次数为删除依据（`memmap_source.rs:482-490`） |
| `kernel_allocated_static` / `kernel_allocated_dynamic` / `vm_allocated_bytes` | 是。C 的 `kinfo` 记账字段 | C 会计字段的换载体 | 保留。VM 交接记录仍消费（`vm_handoff.rs:667-668`）；不是死字段 |
| `user_sp` 常量与 `kern_stack_top` 访问器 | 是。C 的 `USR_STACKTOP` / `_kern_size` 关系 | 派生化 | 保留。v10 删除的是交接字段，消费点读常量/派生值；文档 §3.2.1 与源码一致 |

## TRAIT-AUDIT-CROSSCHECK

`trait-audit.json` 里与本篇 48 个文件相关的条目只有 11 条。逐条核实如下；“读数”一栏保留工具原值，“核实”一栏是我读源码后的结论。

| trait | 读数 | 核实 | 判断 |
|---|---|---|---|
| `FileLoader` | `loader.rs:146`，impls=3，refs=16，dyn=1，MULTI-IMPL | 三个实现：`UefiFileLoader`（`uefi_helpers.rs`）、`UbootFileLoader`（`opensbi_helpers.rs`）、测试 `MockLoader`（`loader.rs:678-692`）；`dyn` 来自测试/参数形态 | 真抽象，保留 |
| `EarlyConsole` | `plat/src/early_console.rs:55`，impls=5，refs=132，MULTI-IMPL | 各架构实现 + 测试/平台实现；boot 诊断输出经它 | 真抽象，保留 |
| `Paging` | `arch/src/arch/paging.rs:212`，impls=5，refs=242，MULTI-IMPL | 三架构 + mock/测试实现 | 真抽象，保留 |
| `HugePages` | `arch/src/arch/paging_ext.rs:75`，impls=5，refs=31，MULTI-IMPL | 三架构 + mock/测试实现 | 真抽象，保留 |
| `PlatformDesc` | `minix-boot/src/platform.rs:342`（当前 347），impls=7，refs=27，MULTI-IMPL | `minix-platform` 的 DTB/ACPI/QEMU 实现族；属平台发现，不在 boot 交接路径 | 真抽象，超出本篇判决范围；保留 |
| `TimerDesc` | `platform.rs:429`（当前 434），impls=4，MULTI-IMPL | 同上 | 保留 |
| `InterruptControllerDesc` | `platform.rs:405`（当前 410），impls=3，MULTI-IMPL | 同上 | 保留 |
| `BklProtected` | `kernel/src/globals.rs:105`，impls=0，refs=31，NO-IMPL | **假阴性**：`bkl_protected_impls!` 宏生成 9+ 个 impl（`globals.rs:157-173`），工具只数字面 `impl` | 声音性标记，保留；不得按 NO-IMPL 删 |
| `Sealed` | `globals.rs:108`，impls=1，refs=6，SINGLE-IMPL | **假阴性**：宏同时生成 `impl Sealed for T {}`；还有 `Option<T>`/`[T; N]` 复合 impl（`globals.rs:193-207`） | 保留 |
| `PrivInfoBuild` / `ProcInfoBuild` | `kernel/src/misc.rs:390/448`，各 impls=1，SINGLE-IMPL-LOW-USE | 不是 boot 路径；是 GET_PRIV/GET_PROC 快照的“内核侧构造半”，类型在 `minix-types`，受 orphan rule 限制 | 保留；不在本篇删改范围 |
| `KernelInfoView` | **evidence 文件里没有**；我重跑 `trait-audit.py --root os/libs/minix-boot` 得到 impls=2、refs=2、MULTI-IMPL | `KernelInfo` 与 `BootReport<'_>` 两个 impl（`kernel_info.rs:200-222`）；调用点是 `validate_view`/`check_boot_postconditions_view`/`land_and_store` | 保留；它不是测试 mock，但承担两种表示的单一校验体 |

结论：`trait-audit.json` 的 `BklProtected`/`Sealed` 行不能作为删除证据；`PlatformDesc`/`TimerDesc`/`InterruptControllerDesc` 的行号已比当前源码少 5 行，`KernelInfoView` 整条缺失——这份 evidence 快照与当前树不同步（至少不是当前 `kernel_info.rs` 的产物）。凡用 NO-IMPL 做删除判决前必须重跑并手工核实。除此之外，48 个文件里没有发现“单实现依赖注入 trait”“无状态 Manager”“对象图共享”式的死抽象：boot 路径的抽象面主要就是 `FileLoader` 与 `KernelInfoView`，两者都有真实职责。

## ILLEGAL-STATES

任务 4：以下组合在类型上可表示，但语义上不应出现。按“今天被谁挡住”判断是否要修。

| # | 可表示的状态 | 为什么语义上不可能 | 今天被谁挡住 | 判断 |
|---|---|---|---|---|
| 1 | `BootReport` 的 `source_records` 是 DTB 记录，调 `arch_boot(..., RamAuthority::UEFI, ...)` | 记录只由 DTB 输入产生；声明 UEFI 权威与记录矛盾 | 直核 pre-table strict 用**另一个** authority 实参校验 report，store 用传入的 authority；矛盾只在 `kmain` strict（`lib.rs:1867`）暴露 | **FINDING DESIGN-01（P2）** |
| 2 | `kmain` 的 `&KernelInfo` 参数与 `KERNEL_INFO` 全局不是同一个值 | trait 合同说该指针作为第一参数传给 kmain；接收侧应只有一个权威副本 | 生产 `kmain` 直接忽略参数，读全局（`lib.rs:1834-1835`）；参数是死句柄 | **FINDING DESIGN-02（P2）** |
| 3 | `BootStorage` 只填了 modules，未填 memmap | 一个 boot 交接缺自由内存表 | `validate_before_tables` 的 `memmap` 非空（`kernel_info.rs:294-297`）在 `arch_boot_finish` 先跑；UEFI 接收侧 strict 也要求非空 | 接受（fail-closed） |
| 4 | 手写 `KernelInfo { authority: UEFI, source_records: DTB 记录 }` | 声明与记录矛盾 | `validate_requiring_sources` 的 `any(input_object == authority)`（`kernel_info.rs:499-519`）；非 strict 的 `validate` 不查 authority，但生产入口用 strict | 接受（测试/诊断路径明确） |
| 5 | `MapCounts { memmap: 5 }` 但闭包只写了 2 条 | 计数与实际槽位不一致 | 落地区把未写槽当零长区域；`validate` 的 `len > 0`（`kernel_info.rs:421-452`）在 kmain 才拒绝；发送侧闭包是可信代码 | 接受（可信链 assert 的代价低于 token） |
| 6 | `Serializer` 写了数据但没调 `finish_*`，descriptor 仍为 0 | wire 有数据、描述符说没有 | 接收方按 descriptor 解码成空分区；空 memmap 在 strict 门失败（`kernel_info.rs:294-297`） | 接受（fail-closed；手工 finish 协议是简单性的代价） |
| 7 | `SourceRecord.address_domain = Virtual` | v1 记录只应物理域；`Virtual` 是为未来保留的 wire 值 | 解码接受 tag 1（`memmap_source.rs:534-550`），但 `audit_source_coverage_for` 只承认 `Physical`（`:341-344`），因此不能覆盖任何物理投影 | 接受（保留值有明确过滤） |
| 8 | `BumpRegion::resumed(base, next, end)` 的三元组不满足 `start < end && start <= next <= end` | 会产生区间外发页 | `BootAlloc::init/resume/region` 都断言 `is_consistent`（`boot_alloc.rs:60-92, 100-112`）；`decode_envelope` 也先查 bump 三元组（`handoff_v1_read.rs:169-174`） | 接受（双重 fail-fast） |
| 9 | 交换 `PreparedJump.entry` 与 `arena_pa` | 跳转目标变成物理 arena，首次取指故障 | 字段名 + 解构绑定（`lib.rs:561-567`）；类型上仍可写错值，但调用点不再按位置 | 接受（值对象的收益即此） |
| 10 | UEFI 腿的 `BootReport` 调 `validate_requiring_sources(UEFI)` | UEFI 腿的 source 在 wire，report 的 source 切片为空；这个调用必然误报 | 生产 UEFI 不调用；空记录会 fail-closed（`kernel_info.rs:499-502`） | 接受（类型不区分完整/不完整视图；见 VERDICT 的可选改进） |

## SUBTRACTION-TABLE（V3.6，C-32）

本轮没有**采纳的**新增类型/trait/字段/层：本轮无新增抽象提案。下面四行列出我考虑过但**不采纳**的加法提案，以及它们能被现有减法替代的方式；最后一列全部为 `否`，因此不触发 C-31 三问与 C-35 教学概念账的必填项。

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| 给 `BootReport` 加 `authority` 字段（或 `AuthoritativeReport` 包装） | 用既有 `report.source_records` 的 `input_object` + `arch_boot_finish` 里一条 `assert!` 绑定 `authority` | 不适用——减法足够：空记录的测试内核仍可走 lenient；非空记录的直核腿在 store 前即拦住 | 字段只是把错配前移到构造点，仍不能证明字段与记录一致；assert 在同一门当场拦；保障强度相同或更好 | 1（字段）或 1（类型） | 否 |
| 给 `BootStorage` 加“全部家族已提交”的 token/见证类型 | 保留五个 `fill_*` 的一次性守卫；由接收侧 `memmap`/模块/来源门兜住跨族半提交 | 不适用——跨族半提交已经 fail-closed，且生产五条腿的填充顺序是固定的 | token 能把“半提交”变成不可表示，但要多一个生命周期/类型参数；现有门在消费前拦住 | 1（token/类型） | 否 |
| 给 UEFI 的不完整 `BootReport` 加 `Complete`/`Incomplete` 类型状态 | 把 `validate_requiring_sources` 限制在完整视图（或文档化“UEFI 不在 report 上做 strict”）；当前调用已 fail-closed | 不适用——误用会立刻 panic，不是静默错 | 类型状态能在编译期拦住误用，但要拆 `BootPayload` 或加 phantom 参数；收益低于复杂度 | 1–2（类型/参数） | 否 |
| 给 `KernelInfoView` 换成 `KernelInfoRef<'a>` 视图结构（删 trait） | 保留 trait；或加 `From<&KernelInfo> for BootReport<'_>` 后让 `land_and_store`/`validate_view` 只收 `BootReport` | 不适用——trait 的两个 impl 就在同 crate，删它需要一个新的视图类型或转换函数；`validate_view` 的单一来源仍然存在 | 结构视图与 trait 的保障相同；trait 少一个转换点，读者少记一个类型 | 1（视图结构） | 否 |

减法优先的结论：本轮真正该做的是**删**（FINDING 03/04）和**在原调用点补一条可信链 assert**（FINDING 01/02），不是加类型。FINDING 01/02 的 `assert!` 是 `review-code-checklist.md` §3.5 所说的“可信链不变量用 fail-fast”形态，不是类型级防御，因此不填三问；也不是加法抽象，因此不填教学概念账。

## FINDING DESIGN-01（P2）

- 位置：`os/kernel-image/src/bootface_common.rs:552-554`（直核发送侧的两次 `RamAuthority::DTB`）；`os/kernel/src/lib.rs:215-238`（`arch_boot_finish` 的 pre-table/store 次序）、`:1568-1573`（`store_boot_report`）、`:1867`（`kmain` 的 strict）；文档 §3.7 的 I3/I4 与 §4.4.1 的直核门序。
- 严重度：P2
- 分类（D-047）：[MODERN]（把已声明的 I3/I4 次序兑现到实际存储值上，不改变 boot 是什么）
- 问题：`BootReport` 没有 authority 字段（这是 DESIGN-02(b) 的既定形状，本报告不建议回退），但直核腿的“声明”仍以两条独立实参出现：`report.validate_requiring_sources(RamAuthority::DTB)` 与 `arch_boot(&report, RamAuthority::DTB, root_page)`。`arch_boot_finish` 只做 `validate_before_tables`/`check_boot_postconditions`，随后 `store_boot_report(report, authority)` 把 authority 写进唯一 `KernelInfo`；它**不**对“刚存进去的 authority”做 strict 校验。也就是说，若两处实参不一致（例如一处被改成 `UEFI`），pre-table strict 校验的是 DTB 记录与 DTB 声明，store 却发布 UEFI 声明；矛盾只在 `kmain` 的 strict 门（`:1867`）暴露，此时页表与 DM 已经建完。
- 影响：这是一条“合法类型、错误语义”的构造面：直核腿的 authority 与 report 记录可以在类型上任意配对。今天的唯一生产调用点两处都写 `DTB`，所以没有现场故障；但一旦未来有人改一处、或新增直核入口，故障会以“kmain 的 provenance 错误”出现，而根因是调用点接反，诊断距离被拉长。文档 §3.7 的“校验在消费（建表/映射/分配）之前完成”在 authority/记录这一对上没有被兑现到 store 后的值。
- 证据或推理：`BootReport` 字段列表（`boot_storage.rs:292-314`）无 authority；`arch_boot_finish` 的三步（`lib.rs:228-232`）不调用 strict；`store_boot_report` 只转调 `land_and_store`（`:1568-1573`）；`kmain` 的 strict 在 `:1867`。仓内 `RamAuthority::UEFI` 只出现在 `ebs_pair.rs` 的测试载体（`:427, :848`）与 QEMU 测试内核，没有覆盖“report 记录 vs store authority”错配的测试。
- 最小修复：在 `arch_boot_finish` 的 `store_boot_report` 之前加一条可信链断言：若 `report.source_records` 非空，则 `assert!(report.source_records.iter().any(|r| r.input_object == authority.input_object()))`（或等价地，让 `store_boot_report` 在非空记录时做同一断言）。空记录的 QEMU 测试内核保持现状；直核生产腿在页表建立之前 fail-fast。不新增类型、不改 wire、不改 `BootReport`。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：有，且优先：用现有 `source_records` + 一条 `assert!`；不给 `BootReport` 加字段，不引入 `AuthoritativeReport` 包装。见 SUBTRACTION-TABLE 第 1 行。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（非类型级提案；这是可信链 fail-fast 断言）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（非加法提案）。
- 置信度：高（源码直接可读；无运行时依赖）。
- 是否需源码核实：是，已核实 `arch_boot_finish`、`store_boot_report`、`run_self_boot`、`kmain` 四处调用序。
- 若我错了：如果项目把“authority 只是构造期声明、构造错误一律由最终 strict 门兜住”作为明确裁决，那么本条降为 P3 文档说明；但文档 §3.7 的 I3/I4 主张仍应改成“直核 authority 的复核在 kmain”，否则读者会以为 store 前的 strict 已经覆盖了它。

## FINDING DESIGN-02（P2）

- 位置：`os/kernel/src/lib.rs:1820-1886`（生产 `kmain`，尤其 `:1834-1835` 的全局读取与 `:1836-1860` 的临时诊断块）；`os/kernel/src/boot/higher_half.rs:31-46`（`jump_to_kmain` 的合同）。
- 严重度：P2
- 分类（D-047）：[MODERN]（入口身份断言）；其中的临时诊断块/延期注释删除属 [DEL] 清理
- 问题：生产 `kmain` 的签名是 `pub fn kmain(_kernel_info: &KernelInfo) -> !`——参数带下划线，函数体完全不读它，转而 `crate::kernel_info().expect(...)` 读 `KERNEL_INFO` 全局。`HigherHalf::jump_to_kmain` 的文档却写“`kinfo`: Pointer to KernelInfo, passed as the first argument to kmain”，并把它列为调用者必须保证有效的指针。同一逻辑对象于是有两个句柄：类型系统承诺的实参，与真正被信任的全局。源码注释还写着“This shadow can be deleted once the diagnostics below confirm it”，并保留了 “Evidence capture (temporary, v3)” 块与 `bootstrap_len==0` 的旧字段描述（该字段已删除）。这既是身份分裂，也是生产代码里的临时/迭代叙事。
- 影响：任何调用者传入一份合法但与全局不同的 `KernelInfo`（例如测试载体、未来的第二个入口）都会被静默忽略；真正被消费的是全局。当前生产跳转恰好把 `store_boot_report` 返回的 `'static` 引用传给 `jump_to_kmain`，所以两者同值；但类型上无法区分“同值”与“不同值”，`kmain` 也不提供任何 fail-fast。诊断块与“可删”的注释留在生产路径，违反“代码禁临时/过程叙事”的出版标准，并让 boot 入口的权威来源在文档与代码之间不一致。
- 证据或推理：`kmain` 参数名 `_kernel_info`（`lib.rs:1820`）；函数体读 `crate::kernel_info()`（`:1834-1835`）；`jump_to_kmain` 的 Arguments 段（`higher_half.rs:38-40`）；`store_boot_report` 返回 `&'static KernelInfo`（`:1568-1573`），`arch_boot_finish` 把同一个引用传给 `H::jump_to_kmain(kinfo)`（`:237`）。如果参数真如注释所说被栈破坏，那么 `jump_to_kmain` 的合同本身不成立，应该修栈/别名问题或改合同，而不是在生产入口永久保留影子。
- 最小修复：在 `kmain` 开头 `assert!(core::ptr::eq(kernel_info, crate::kernel_info().expect(...)))`，然后把 `_kernel_info` 改回 `kernel_info` 并删除临时诊断块与“can be deleted”注释。断言是同址可信链的 fail-fast；如果它真的触发，说明注释里的栈破坏是真实缺陷，应当按缺陷修，而不是继续用全局遮蔽。若确认参数在 ABI 上不可信，则必须把 `HigherHalf` 的合同改成“参数仅为 ABI 占位、权威在全局”，并同步文档；那属于入口合同的用户裁决项。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：有，且优先：删掉临时块、用全局一处作为唯一权威，再加一条 `ptr::eq` 断言；不新增类型/字段。另一种“去掉参数”的做法会改 `HigherHalf` 的 ABI 合同，代价更大，本轮不提议。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（非类型级提案；可信链 assert）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（非加法提案）。
- 置信度：高（源码直接可读）。
- 是否需源码核实：是，已核实生产 `kmain`、`jump_to_kmain` 合同与 `arch_boot_finish` 的传参。
- 若我错了：如果作者明确裁决“参数在 QEMU/固件栈上确实不可信，全局是唯一合同”，那么本条的主体从“改代码”转为“改文档/合同”：`HigherHalf` 的参数说明、`kmain` 的签名语义、以及 §4.1/§4.4 的入口叙述都必须写明这一点；临时诊断块仍应删除或转为受控的诊断设施，不能以“可删”注释留在生产路径。

## FINDING DESIGN-03（P3）

- 位置：`os/libs/minix-boot/src/handoff_v1_write.rs:99-106`（字段声明）、`:126-131`（初始化）、`:224`（`modules_n` 赋值）、`:247`（`platform_n` 赋值）、`:266`（`param_n` 赋值）。
- 严重度：P3
- 分类（D-047）：[DEL]（C 没有此物，删 Rust 侧死状态）
- 问题：`Serializer` 的六个计数器里，只有 `wire_memmap_n`、`reserved_n`、`sources_n` 被读取（容量检查、`finish_*`、`push_*` 的偏移）；`modules_n`、`platform_n`、`param_n` 只在对应的 `write_*` 里被赋值，之后没有任何读取。它们不是 wire 描述符的来源（描述符用的是输入切片长度），也不是任何校验的输入。
- 影响：纯死状态。它让 `Serializer` 看起来在跟踪三类载荷的“已写数量”，但真正决定 `byte_len` 的是 `write_boot_modules`/`write_platform_sources`/`write_param_buf` 写入时的 `modules.len()`/`sources.len()`/`bytes.len()`。后续读者可能误以为有“部分写/重复写”保护而依赖不存在的状态。没有运行时行为影响。
- 证据或推理：文件内 `grep` 六个名字：`modules_n`/`platform_n`/`param_n` 只出现在字段声明、初始化和各自赋值处；`finish_sources`/`finish_memmap`/`finish_reserved` 只读 `sources_n`/`wire_memmap_n`/`reserved_n`。`Serializer` 没有 `Debug` 或其他读取这些字段的 impl。
- 最小修复：删除三个字段、三处初始化和三处赋值；`write_boot_modules`/`write_platform_sources`/`write_param_buf` 的 `byte_len` 仍由输入长度写，行为不变。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无，因这是纯删除；不需要替代物。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（非类型级提案）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（非加法提案）。
- 置信度：高。
- 是否需源码核实：是，已核实整个 `handoff_v1_write.rs` 的读取点。
- 若我错了：如果这三个字段是给未来的诊断/调试输出预留的，那么它们应带使用点或 `#[expect(dead_code)]` 式说明；当前既无读者也无注释，按“取证字段不以正常路径读取次数为删除依据”不适用——它们不是 `SourceRecord` 的取证字段，只是序列化器的内部计数。

## FINDING DESIGN-04（P3）

- 位置（四处同类，均属源码注释与当前 DESIGN-02(b)/wire v10 不一致）：
  1. `os/boot-shim/src/main.rs:63`：“the v1 handoff arena (address space, seven payload partitions, pairing loop)”——当前 v10 是六个 payload 分区（memmap/reserved/modules/platform/param/sources；`handoff_v1.rs:348-385`），第七个是已删除的 startup 分区。
  2. `os/kernel-image/src/bootface_common.rs:334`：“platform source → parameter buffer → `KernelInfo` → `arch_boot`”，同文件 `:530` “KernelInfo plus the real arch_boot gate”——发送侧实际构造的是 `BootReport<'_>`（`:547-554`），`KernelInfo` 由接收侧 `arch_boot_finish`/`land_and_store` 构造。
  3. `os/kernel-image/src/bootface_a64.rs:39-44`：“12 modules→KernelInfo→`minix_kernel::arch_boot`”以及“deduplication convergence ... left to a separate cleanup increment”——前半句是旧发送形状；后半句是已由 `bootface_common` 完成的清理留下的延期语句。
  4. `os/libs/minix-boot/src/kernel_info.rs:247`：“Validate kernel-info invariants. Called by `kmain` at entry.”——`kmain` 现在调用 `validate_requiring_sources`（`lib.rs:1867`），`validate` 的仓内调用点只在自身 tests；`kmain` 的注释 `lib.rs:1827` 还提到已删除的 `bootstrap_len`。
- 严重度：P3
- 分类（D-047）：[DEL]（删除/改写与 DESIGN-02(b)/v10 相反的过期注释与延期语句）（发送侧构造 `KernelInfo`、七个分区）或已删除字段（`bootstrap_len`）。在一篇以“五条腿发送侧统一 `BootReport`”为核心主张的文档旁边，源码注释的旧形状会直接误导维护者；`left to a separate cleanup increment` 与“can be deleted”同属过程叙事，按出版标准不应留在生产代码里。
- 影响：文档-代码同步失败；读者可能按注释重建“发送侧 KernelInfo”或“七个分区”，从而提出与 DESIGN-02(b)/v10 相反的重构。没有运行时影响。
- 证据或推理：逐条对照 `handoff_v1.rs` 的分区常量（`WIRE_OFFSET` 到 `WIRE_SOURCES_END` 共六段）、`bootface_common.rs` 的 `storage.report`/`arch_boot` 调用、`kernel_info.rs` 的 `validate`/`validate_requiring_sources` 调用点。
- 最小修复：把第 1 处改为“six payload partitions”；第 2/3 处改为“构建 `BootReport<'_>`，接收侧落 `.bss` 后构造 `KernelInfo`”，删掉“left to a separate cleanup increment”；第 4 处改为“Validate kernel-info invariants (lenient/test gate)；production uses validate_requiring_sources”。同时删掉 `lib.rs:1827` 的 `bootstrap_len==0` 旧字段描述（随 FINDING DESIGN-02 一起处理）。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无，因这是删除/改写过期注释；不新增概念。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（非类型级提案）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（非加法提案）。
- 置信度：高。
- 是否需源码核实：是，已核对四处注释与对应实现。
- 若我错了：如果项目有意保留“旧形状注释”作为迁移史，那么按出版标准应放在 `rewrite-notes/meta/` 而不是生产源码；当前位置仍不合格。

## VERDICT

**值得改（接受）**

- FINDING DESIGN-01（P2）：在 `arch_boot_finish` 用一条可信链断言把 `authority` 与 `report.source_records` 绑定；不改 `BootReport` 形状，不回退 DESIGN-02(b)。这是本轮最值得改的一条：它把 §3.7 已声明的 I3/I4 次序落实到 store 前的实际值上。
- FINDING DESIGN-02（P2）：给生产 `kmain` 加 `ptr::eq` 断言，删除“Evidence capture (temporary, v3)”块与“can be deleted”注释；若断言暴露真实栈破坏，按缺陷修，不要用全局影子长期遮蔽。
- FINDING DESIGN-03（P3）：删 `Serializer` 的三个死计数器。
- FINDING DESIGN-04（P3）：改四处过期注释（六个分区、发送侧 `BootReport`、删除 cleanup 延期语、`validate` 的 kmain 调用说明）。

**合理取舍（拒绝改动）**

- 五条腿发送侧统一 `BootReport<'_>`、接收侧 `store_boot_report`/`store_kernel_info` 落 `.bss` 后构造唯一 `KernelInfo`：与文档 §3.7/§4.4 一致，且 `BootStorage` 的按家族提交 + 接收侧门链已经覆盖跨族半提交；不回退到 `build_kernel_info`/`BootStorageCell`/`report_static`/`STORAGE`。
- 不设 boot trait：`prepare_boot` 自由函数 + `FileLoader` 接缝是正确的减法；`FileLoader` 的三个实现（UEFI/U-Boot/Mock）和 `EarlyConsole`/`Paging`/`HugePages` 的多实现都承担真实职责，不属于死抽象。
- `KernelInfoView` 保留：两个真实 impl、没有 `dyn`、不是 mock 接缝，但它让 `KernelInfo` 与 `BootReport` 共用同一校验体，删它需要引入转换/视图类型；在本篇的可靠性收益上不值得。
- `BootStorage` 的五个 `*_committed` 布尔、`ZERO_*` 哨兵、`EMPTY_PHYS_RANGE` 保留：前者是最小的一次性提交守卫，后两者只通过 live prefix/已提交前缀读取；换成 `MaybeUninit` 或类型 token 只会增加 unsafe/概念。
- `PlatformDescSource.len = None`、`AddressDomain::Virtual`、`KinfoStruct.freepde_start`/`vir_kern_start` 保留：分别是旧生产者 wire 兼容、未来 wire 值、`GET_KINFO` 外部布局；删它们要改 wire 版本或系统调用载荷，收益低于代价。
- `KernelInfo::validate` 的公开测试面保留，但其 doc 必须改（FINDING DESIGN-04）；生产入口继续用 `validate_requiring_sources`。

**需用户裁决**

- 本轮没有 `[ARCH]` 提案，因此没有 `## ARCH-PROPOSAL`。如果裁决层认为 FINDING 01/02 的“新增一条预表/入口断言”属于 D-047 的 `[ARCH]`（新检查），则这两条应转 ARCH-PROPOSAL 并按“扩张内容、[ARCH] 三处一致计划、不做会坏在哪、代价”重新立案；我的判断是它们只是把既有 I3/I4/入口合同兑现，不改变 boot 是什么，故按 `[MODERN]` 记。
- 如果作者确认 `kmain` 的 `&KernelInfo` 参数在目标栈上确实不可信，那么 `HigherHalf` 的“passed as the first argument”合同与文档 §4.1/§4.4 的入口叙述需要用户裁决改成“参数仅 ABI 占位、全局唯一权威”；这是入口合同变更，不是本轮面板能单方面裁定的。

**证据强度自述**

- 本报告为同 session 的独立设计角色；源码事实来自当前工作树的选择性阅读与 `trait-audit.py` 的重跑，不依赖历史评审结论。`trait-audit.json` 快照与当前源码不同步（`PlatformDesc` 行号 342→347、`KernelInfoView` 缺失），因此我没有把任何 `NO-IMPL` 读数直接当删除依据；`BklProtected`/`Sealed` 的宏生成 impl 是我手工核实的假阴性。若后续用 `cargo test -p minix-boot`/`cargo test -p minix-kernel --features mock` 复跑，FINDING 03/04 的删除/改注释应保持零行为变化；FINDING 01/02 需要新增宿主负控（见各条的最小修复）。
