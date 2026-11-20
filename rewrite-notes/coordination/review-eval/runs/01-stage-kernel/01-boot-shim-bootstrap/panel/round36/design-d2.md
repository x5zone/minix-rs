# 01-boot-shim-bootstrap · round36 · 独立设计审查 D2（回退形态）

> **D1 未运行（D-024），本报告为回退形态。**
>
> 按 review-panel §4.3 的回退形态，本报告把目标文档 §3「Rust 设计决策」与 §4「实现详解」的机制主张当作被审查的“第一遍方案”，逐条与 `related-code.txt` 列出的现有 Rust 实现对照。第一遍的设计方案没有独立产物；下文出现的“文档主张”均指这两章的原话或代码引文。
>
> 读取范围：目标文档第 1392–3139 行（§3–§4）、`related-code.txt` 列出的实现文件、`trait-audit.json` 与 `AGENTS.md`。未读取任何历史评审结论、其他面板角色输出或 `expected/` 目录。
>
> 事实基线：外部行为以 Minix3 C 源码为准；本章的 Rust 设计契约以目标文档 §3/§4 为准；实现事实以源码行号为准。三者冲突时在“差异”列写明是哪一层冲突，不替作者改写设计意图。

## DIFF-TABLE

| 维度 | 文档主张 | 现有实现 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达·wire 版本 | §3.4.1（文档 1625 行）：v10 把 `BOOT_HANDOFF_V1_VERSION` 从 9 升到 10，“v9 与 v10 互相拒收”；§3.4.3（1713 行）却又写“当前 9”。 | `os/libs/minix-boot/src/handoff_v1.rs:288` 仍是 `BOOT_HANDOFF_V1_VERSION: u32 = 9`；信封结构已是 128 B（`BootHandoffV1` 无 `user_sp`/`kern_stack_top`/`syscall_entry`/startup 分区）。`handoff_v1_write.rs:163`、`handoff_v1_read.rs:107` 都按该常量收发。 | 文档自身前后矛盾；实现取了旧版本号。尺寸检查（`handoff_v1_read.rs:110`）仍能拒绝旧 160 B 信封，但版本号这一层契约没有兑现。 | **应改**：把常量升到 10 并补版本历史；否则“v10”这个身份在 wire 上不存在。见 DESIGN-D2-01。 |
| 状态表达·startup 对象 | §3.4.1（1625 行）、§4.5（2847 行）、§4.7（3027 行）与 `arch_boot_resume_v1` 注释（`os/kernel/src/lib.rs:650`）按 v10 前口径描述 startup 分区、`STARTUP_RECORD_STORE` 与“七个载荷/七个分区”。 | 源码已无 `STARTUP_RECORD_STORE`/`WIRE_STARTUP_*`；`HandoffDescriptors` 只有六个 `byte_len` 字段（`handoff_v1_read.rs:124-131`），`arch_boot_resume_v1` 解码六类载荷后构造 `KernelInfo`（`os/kernel/src/lib.rs:686-763`）。`startup_record()` 仍存在，但生产侧没有调用者（全仓 grep 仅 `BootStorage` 的零哨兵 `ZERO_RECORD` 与一个测试夹具）。 | 文档的 v10 清扫只做了一半；实现保留了 v10 前的类型标签但没有生产者。 | **文档应改**；类型侧见 DESIGN-D2-03（死代码清理，非行为变更）。 |
| 状态表达·校验门 | §4.4（2742 行）说 `check_boot_postconditions` 检查 `syscall_entry` 落在镜像跨度、`kern_stack_top` 不越上界、用户栈不越内核半区；同表 2746 行说 strict 门核对三个派生标量（`user_sp`/`syscall_entry`/`kern_stack_top`）。 | `kernel_info.rs:554-571` 的 `check_boot_postconditions` 只有一条断言：`kern_virt_base > u32::MAX`。`kernel_info.rs:432-438` 注释明确写 v10 删除了这三个派生等式；`validate_impl_inner` 中已无对应检查。 | 文档把 v10 删除的检查继续列为现役门。源码的弱化是**有意的**（删除的是恒等式重抄），但文档台账过期。 | **文档应改**；实现无需恢复这些检查。见 DESIGN-D2-02。 |
| 状态表达·落地区 | §4.5（2837–2853 行）的容量表列“七个落地区加一个名字池”，含 `STARTUP_RECORD_STORE`（20 槽、`WIRE_STARTUP_CAP`、溢出 panic）。 | `os/kernel/src/globals.rs` 只有 `RESERVED_REGION_STORE`/`MEMMAP_REGION_STORE`/`BOOT_MODULE_STORE`/`PLATFORM_SOURCE_STORE`/`PARAM_BUF_STORE`/`SOURCE_RECORD_STORE` 六个落地区（`globals.rs:299-419`），无 startup store。`store_kernel_info`（`lib.rs:1574-1692`）落六个切片 + 一个名字池。 | “七”是 v9 口径；v10 后是“六 + 名字池”。代码与 `KernelInfo` 的 10 字段自洽，文档表过期。 | **文档应改**；不新增存储。见 DESIGN-D2-02。 |
| 状态表达·分配器阶段 | §4.7（2986–3021 行）说 `BootAlloc` 有 `init`/`resume` 两种进入方式，接收侧必须 `resume`；§3.8 说“共享的纯值核心 `BumpRegion`：边界检查、`used_bytes` 与一致性谓词都只有这一份”。 | `BootAlloc` 用 `AtomicU64` 重建 `BumpRegion`（`boot_alloc.rs:130-147`），确为共享；但 `os/kernel-image/src/bootface_a64.rs:111-130` 另有一份 `static mut BUMP_PTR` + `*ptr + need` 手写边界检查，没有用 `BumpRegion`。 | aarch64 直核腿是“边界检查只有一份”的反例；两处代数不同（手写用未检查的 `+`）。 | **应改**：让 aarch64 也走 `BumpRegion`（复用现有类型，不新增概念）。见 DESIGN-D2-04。 |
| 身份·模块名/顺序 | §3.4（1621 行）、§4.3（2524–2526 行）说 `MODULE_NAMES` 是唯一位置身份，内核 strict 门按位置核名；`bootface_common` 有编译期逐字节对账。 | `minix_boot::kernel_info.rs:610-612` 是唯一表；`bootface_common.rs:54-68` 的 `NAME_POOL` 与它做 `const` 对账；`kernel_info.rs:442-449` 的 strict 门按位置核对 `BootModule.name`。 | 一致。 | **保留**。 |
| 身份·RAM 权威 | §3.4.1（1627 行）：`authority` 与内存图绑定为同一值；wire 解码只接受 0/1；`StartupObjects` 只是记录身份，不是 authority 的活取值。 | `KernelInfo.authority: InputObjectId`（`kernel_info.rs:154`）仍允许 `StartupObjects` 这个三选一值；`handoff_v1_read.rs:160-163` 的 `ram_authority` 解码只接受 0/1；strict 门查 `memmap_sources.any(input_object == authority)`（`kernel_info.rs:450-459`）。 | 类型上可表示“authority = StartupObjects”，语义上不可能；但会被 strict 门 fail-closed。 | **接受现状**：类型可再收窄（见减法表），但当前失败是显式停机，不是静默错误。 |
| 所有权·发送侧 | §3.4.5（1759–1765 行）：`BootStorage` 是所有者、`BootReport<'a>` 是借用视图、`FileBytes` 显式区分 `Borrowed`/`Owned`。 | `boot_storage.rs:86-312` 与 `loader.rs:97-130` 与文档一致；`UEFI_BOOT_STORAGE`/`BOOT_STORAGE` 是 `BootStorageCell` 静态。 | 一致（UEFI 腿 source 槽为空是文档已承认的例外）。 | **保留**。 |
| 所有权·直核面 `'static` | §3.4.5（1764 行）：两个直核面各在自己的静态区放 `BootStorageCell`，直核腿信任边界上有两个所有者（DTB 收集器静态量 + `BootStorage`）。 | `bootface.rs:147`/`bootface_a64.rs:148` 为函数内 `static STORAGE`，`take()` 后经 `unsafe from_raw_parts` 造 `'static` 切片（`bootface_a64.rs:364-393`）；DTB 收集器已改为 `collect_owned`（栈上），文档的“两个所有者”描述对现有代码已不准确。 | 文档滞后于 I-33 的全部 owned 化；直核腿只剩 `BootStorage` 一个所有者。 | **文档应改**；代码的所有权方向正确。 |
| 错误·缺件 | §3.2（1504 行）：`read_required` 与 `load_boot_modules_into` 的 `unwrap_or_else(panic!)` 是 fail-closed 的两个致命落点；`read` 只返回 `Option`。 | `loader.rs:152-157`、`loader.rs:496-506` 与文档一致；错误消息不同但都 fail-closed。 | 一致。 | **保留**（可信链上用 panic/assert 是正确形态）。 |
| 错误·分类失败 | §4.2.2（2428 行）：只对 `INVALID_PARAMETER` 重试一次；其余状态立即失败；缓冲扩容只在第一次 EBS 前。 | `ebs_pair.rs:156-226` 实现：`BUFFER_TOO_SMALL` 在 `ebs_attempts == 0` 时返回 `NeedBiggerBuffer`，否则 `BufferReuseInsufficient`；`INVALID_PARAMETER` 递增后 `>= 2` 返回 `RetryExhausted`；其他状态 `ExitStatus`。 | 一致（状态机由 `PairError` 枚举表达，没有布尔/字符串状态机）。 | **保留**。 |
| 并发·页表分配器 | §3.8（2104–2147 行）：函数指针槽写一次、`PT_CLAIMED`/`PT_REGISTERED` 用 Acquire/Release 发布；重注册在 release 也 panic。 | `pt_alloc.rs:57-141` 与文档逐行一致；`free_pt_page` 在未注册时是 no-op（文档 §3.8 已交代）。 | 一致。 | **保留**。 |
| 并发·U-Boot 安装全局 | §3.2/§3.8 没有把 `DTB_PTR`/`BOOT_FILE_TABLE_PTR` 列为设计对象；文档只把 `prepare_boot(&mut BootStorage, bump_pages)` 当作入口签名。 | `opensbi_helpers.rs:442`、`:498` 是 `static mut` 安装型全局；`uboot_main.rs:452/459` 调用安装器；`install_write_once.rs` 因“进程级静态无法复位”而被迫作为独立集成测试。 | 入口真正的输入从签名看不见：DTB 与文件表是进程全局。测试接缝也因此变差。 | **需用户裁决**：把两者改成 `prepare_boot` 的参数（去掉全局与 `unsafe` 安装器），或至少改 `AtomicU64`。见 DESIGN-D2-05。 |
| 抽象边界·`FileLoader` | §3.2（1485–1527 行）：`FileLoader` 是两个 boot-shim 实现族的唯一差异点，也是测试接缝（`MockLoader`）。 | `loader.rs:146-158` 是 trait；UEFI（`uefi_helpers.rs:1508` 附近）、U-Boot（`opensbi_helpers.rs:198-215`）、`MockLoader`（`loader.rs:583-590`）三个实现；`trait-audit.json` 对 `FileLoader` 记 `MULTI-IMPL`。 | 一致；不是死抽象。 | **保留**。 |
| 抽象边界·`HugePages` | §3.6（1838–1921 行）：`HugePages: Paging` 是超集扩展 trait，三架构各自回答能力；`map_huge` 不进 `Paging`。 | `paging_ext.rs:75-99` 与文档一致；`trait-audit.json` 记 `HugePages` 5 个实现、`MULTI-IMPL`；x86 `map_huge` 在入口用 `supports_huge_page` 拒绝不支持尺寸（`x86_64/paging.rs:1245-1247`）。 | 一致；不是死抽象。 | **保留**。 |
| 抽象边界·`Paging` 成员表 | §3.6（1872 行）说 `Paging` 有 22 个成员（`PAGE_SIZE` + 21 方法），并列出 21 个方法名。 | `os/arch/src/arch/paging.rs:212-470` 实际还有 `is_supervisor_va`、`is_inherited`、`ensure_writable_range`，以及 `DM_HUGE` 关联常量；文档列的 `adopt_active_root` 已被合并进 `from_active_root`（`paging.rs:193` 注释），`new_from_page`/`from_active_root` 还各多一个 `PteChannel` 参数（`paging.rs:270`、`:284`）。 | 成员数与签名都过期（实际 25 成员 / 23 方法 / 2 常量，文档 22 成员 / 21 方法 / 1 常量；方法多三个、少一个（净 +2），另多 `DM_HUGE` 常量与 `PteChannel` 参数）；结论（`Paging` 职责不窄）反而更强。 | **文档应改**；见 DESIGN-D2-06（含 `PteChannel` 设计缺席）。 |
| 抽象边界·新根清零通道 | §3.7（2071/2086 行）说接收侧若重建根会调 `Paging::new_from_page(root_page)` 并清零 512 项。 | 实际签名是 `new_from_page(root_page, channel: PteChannel)`（`paging.rs:270`）；引导期调用传 `PteChannel::Identity`（`kernel/lib.rs:1340`），运行期进程根传 `KernelDm`（`kernel/lib.rs:1767`）；`PteChannel` 的 `Identity`/`KernelDm`/`VmDm` 三态在文档 §3/§4 零命中。 | 文档没有处理“清零通过哪个视图”这条真实约束；§3.7 的“重建会清零活动根”结论仍成立，但签名与机制描述不完整。 | **文档应补**；实现方向正确。见 DESIGN-D2-06。 |
| 可测试性·EBS 配对循环 | §4.2.1（2437–2474 行）把分类规则与 `adjudicate` 的步骤表逐项列出；§5 另讲覆盖（本报告未读 §5）。 | `ebs_map.rs:594-653` 的 `adjudicate` 与文档五步一致；`PairError`/`MapError`/`PolicyError` 都是枚举；`ebs_pair.rs` 用 `RawOps` 函数指针注入固件调用，宿主可测。 | 一致；状态表达为枚举/注入表，没有字符串或布尔状态机。 | **保留**。 |
| 可测试性·安装全局 | §3.2 把 `prepare_boot` 的签名当作入口契约，未提测试隔离成本。 | `install_dtb_ptr`/`install_boot_file_table` 是进程级 `static mut`（`opensbi_helpers.rs:442/498`）；`tests/install_write_once.rs:6` 明写“无法在进程内复位，所以负控只能放独立集成测试”。 | 可测试性被全局状态削弱；这是 DESIGN-D2-05 的证据之一。 | **需用户裁决**（见 D2-05）。 |
| 可测试性·`SHIM_TABLE` 构造 | §3.2（1522 行）只说 `build_shim_table` 按槽修剪长度；§4.3 只说表由 shim 自己构建。 | `uboot_main.rs:282` 是 `static mut SHIM_TABLE: [u8; 16 + 16*80]`，`build_shim_table`（357–432 行）用硬编码偏移写 magic/entry_count/path/addr/len，再 `boot_file_table()` 把 `*const u8` 强转成 `&BootFileTable`（484 行）。`BootFileEntry` 是 `#[repr(C)]`，对齐 8；`[u8; N]` 的静态只保证对齐 1。 | 布局知识被复制到字节偏移；且静态数组的对齐不满足 `&BootFileTable` 的要求，存在未定义行为/对齐故障面。 | **应改**：用现有 `BootFileTable` 类型构造，删除字节偏移与强制转换。见 DESIGN-D2-07。 |
| 可测试性·死代码 | §3.4.1/§4.5 说 v10 删除了 startup 对象。 | `startup_record` 仍公开导出（`minix-boot/lib.rs:40`、`memmap_source.rs:755`），但只被 `BootStorage` 的 `ZERO_RECORD`（`boot_storage.rs:55`）和一个测试用作零值；`SourceKind::StartupObject`/`SourceOrigin::StartupObject`/`InputObjectId::StartupObjects` 仍参与 wire 编解码（`handoff_v1.rs:134/146/185/203/229`）。 | 保留 wire 数值作为“永不复用”的保留位有理由；但保留 `startup_record()` 构造器和 Rust 枚举分支没有生产语义。 | **应改（小）**：删构造器与无生产者的分支，保留 wire 常量的数值注释即可。见 DESIGN-D2-03。 |

## SUBTRACTION-TABLE（V3.6，C-32）

> 先减法、后加法。下表列出本报告全部“看起来要加东西”的候选；能复用现有类型/参数/现有能力的一律走减法列。最后一列只填 `是` 或 `否`，含义是**采纳该行的减法替代**（不是采纳左侧的加法提案）。

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| 新增 typed `ShimTableCell`/`SyncUnsafeCell<BootFileTable>` 来承载 U-Boot 文件表 | 直接复用现有 `BootFileTable` 类型：把 `static mut SHIM_TABLE: [u8; N]` 换成 `static mut SHIM_TABLE: BootFileTable`（或把表建在 `uboot_main` 的栈上，以 `&BootFileTable` 传给 `prepare_boot`）；字段按名写入，不再有偏移常量 | 无需新增类型；现有 `BootFileTable`/`BootFileEntry` 已经表达布局，`UnsafeCell` 包装在本仓已有 `BootStorageCell`/`PhaseCell` 先例 | 强于现状：编译期布局与 8 字节对齐由类型保证；字节偏移与 `*const u8` 强转消失 | 0 | 是 |
| 新增 `RamAuthority` 两值枚举来禁止 `authority = StartupObjects` | 不做；保留 `InputObjectId`，靠 strict 门的 `any(input_object == authority)` 与覆盖审计 fail-closed | 现有门在 authority=StartupObjects 时没有对应记录，会立即 panic；不是静默错误 | 类型更强，但当前失败已显式；新增一个概念与一次转换 | 1 | 否 |
| 新增 `BumpCursor`/`Aarch64Bump` 类型给 aarch64 直核腿 | 复用现有 `minix_boot::BumpRegion` + 一个 `AtomicU64` 游标（与 `opensbi_helpers::bump_alloc`、`BootAlloc` 同形），回调只做 `region.alloc_pages(n)` | `BumpRegion` 的 `alloc_pages` 已含 checked mul/add 与零页拒绝；aarch64 手写版是第二份边界检查 | 强于现状：去掉未检查的 `+`，边界检查回到一份 | 0 | 是 |
| 新增 `FileLoader` 的 `Fn` 闭包替代 trait | 不做；trait 已有 UEFI/U-Boot/Mock 三个实现，且 `read_required` 默认方法承载 fail-closed 语义 | 闭包不能承载默认方法与实现族命名；当前 trait 不是死抽象 | 同等 | 0 | 否 |
| 把 `prepare_kernel_image_v1`/`jump_kernel_image_v1` 继续作为公开分裂 API | 不做；合并为 `enter_kernel_image_v1` 内部私有 helper，等 TODO-09 真正需要时再拆 | 当前仓内零外部调用者；但拆分是零成本的两个小函数，且 `PreparedJump` 已承担“防交换”价值 | 合并后 API 面更小；保留也不增加概念 | 0 | 否 |
| 新增 `PteChannel` 的文档专节/类型调整 | 不做类型改动；只在 §3.6/§4.6 补一段解释现有 `PteChannel` 三态与两处调用点 | 文档缺失不能靠代码类型解决；类型本身已存在且正确 | 文档完整度提升；代码零改动 | 0 | 是 |
| 把 `prepare_boot` 加两个入参（`&BootFileTable`、`Option<u64>` DTB）以删除安装型全局 | 参数传递本身即减法：删 `DTB_PTR`/`BOOT_FILE_TABLE_PTR` 两个 `static mut`、两个 `install_*` 与两个读取器；`uboot_main` 栈上建表后直接传引用 | 无——参数化能完整表达两个输入，漏传变成编译错误；唯一代价是公开签名变化（需用户裁决） | 强于现状：入口输入可见、测试可在进程内构造表；删两个全局与两个 unsafe | 0 | 是 |
| 把 `MapStorage`/`Claims` 静态 scratch 改为调用方局部 + `&mut` 参数 | 复用现有 `MapStorage`/`Claim` 类型，只在调用方栈上持有并传参 | 11 KiB 栈预算需实测；但 `pair_and_classify` 的输入/输出会从签名可见，测试不再依赖单线程进程 | 强于现状：删 `static mut` 与 `addr_of_mut!`；代价是 1 个参数 | 0 | 否 |

**本轮没有新增类型/trait/字段**；上表“采纳=是”的项都是复用现有类型或纯删除/纯文档修正。`RamAuthority` 与 `FileLoader` 闭包化被否决，理由见上表。

## FINDING DESIGN-D2-01（P1）

- 位置：`os/libs/minix-boot/src/handoff_v1.rs:288`；对照文档 §3.4.1（1625 行）与 §3.4.3（1713 行）。
- 严重度：P1
- 问题：wire 信封的结构已经按 v10 改完（160→128 B、删除 `user_sp`/`kern_stack_top`/`syscall_entry`/startup 分区），但 `BOOT_HANDOFF_V1_VERSION` 仍是 9。文档 §3.4.1 明写“版本 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收”；源码的版本历史注释（`handoff_v1.rs:268-288`）也停在 9。§3.4.3 在同一篇文档里又写“当前 9”，文档自身也没有统一。
- 影响：`handoff_v1_read.rs:107` 与 `handoff_v1_write.rs:163` 都按 9 收发。旧 v9 发送方（160 B 信封）仍会被 `SIZE != size_of::<BootHandoffV1>()`（`handoff_v1_read.rs:110`）拒绝，所以不会静默错读；但“按版本号拒收”的契约没有兑现，任何只看 `version` 的外部工具/测试会把 v10 布局误认成 v9。设计文档的 v10 身份在 wire 上不存在。
- 证据或推理：`git show 2a345f566:os/libs/minix-boot/src/handoff_v1.rs | grep BOOT_HANDOFF_V1_VERSION` 输出仍是 `= 9`；该提交的说明却写“版本 9→10”。`handoff_v1.rs:420` 的 `size_of::<BootHandoffV1>() == 128` 与 v10 注释一致。`handoff_v1_read.rs:627/633` 的负控用 `VERSION ± 1`，所以当前负控实际测的是 8/10 而不是 9/11。
- 最小修复：`BOOT_HANDOFF_V1_VERSION: u32 = 10`，在版本历史注释里补 10 的条目；同步文档 §3.4.3 的“当前 9”为 10。核对 `handoff_v1_read.rs` 的负控仍成立（`±1` 变成 9/11），并在 §4.3 的 arena 说明处不要再写“版本 9”。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无新增概念；这是把已有协议身份改到设计值。减法方案“不做，保留 9”拦不住的原因是：文档与提交说明都已经把 v10 当现行契约，保留 9 会让契约与实现长期分叉；尺寸检查只是第二道防线，不替代版本号。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（仅改常量与文档）。
- 分类（D-047）：[DEL]（删掉过时的 v9 身份；不是新增抽象）
- 置信度：高（源码常量与提交记录直接矛盾）
- 是否需源码核实：已核实（`handoff_v1.rs:288`、`handoff_v1_read.rs:107/110`、`handoff_v1_write.rs:163`、`git show 2a345f566`）。
- 若我错了：如果团队有意让版本停在 9、靠 `size` 字段做判别，那么文档 §3.4.1 的“9 → 10”与提交说明都必须改写；但当前两处都写 10，不能同时为真。

## FINDING DESIGN-D2-02（P1）

- 位置：文档 §4.4（2742/2746 行）、§4.5（2837–2855 行）、§4.7（3027 行）；实现 `os/kernel/src/kernel_info.rs:432-438/554-571`、`os/kernel/src/lib.rs:225/347/1580`、`os/kernel/src/globals.rs:299-419`。
- 严重度：P1
- 问题：v10（D-062）清扫了字段与 wire、I-35(a)（D-071）把 `store_kernel_info` 收成 one-shot、I-33 把所有 DTB 收集改成 owned，但 §3/§4 的“门/落地区/调用点/所有权”台账没有跟完这些收口，形成一组七处可核的过期陈述：
  1. §4.4 表把 `check_boot_postconditions` 写成检查 `syscall_entry` 落在镜像跨度、`kern_stack_top` 不越上界、用户栈不越内核半区；实际只剩 `kern_virt_base > u32::MAX` 一条（`kernel_info.rs:554-571`）。
  2. 同表 strict 门写“核对三条派生标量（`user_sp`/`syscall_entry`/`kern_stack_top`）等于其唯一来源”；实际 v10 已删除这些等式（`kernel_info.rs:432-438`），strict 门现在只核模块位置名、authority/来源覆盖与内核跨度落表（`kernel_info.rs:442-510`）。
  3. §4.5 容量表列 `STARTUP_RECORD_STORE`（20 槽）与“七个落地区”；实际 `globals.rs` 只有六个落地区，`store_kernel_info` 落六个切片 + 名字池（`lib.rs:1574-1692`）。
  4. §4.5（2855 行）说 `store_kernel_info` “有三个调用点……`kmain` 用 `let kernel_info = store_kernel_info(kernel_info);`”；实际只有 `arch_boot_finish` 与 `resume_additive` 两个调用点（`lib.rs:225/347`），`store_kernel_info` 顶部还有 one-shot 断言：第二次调用直接 panic（`lib.rs:1580-1583`），`kmain` 不再调用它（`lib.rs:1876` 注释与代码）。
  5. §4.7（3027 行）说“startup-object 记录仍保留 arena 的身份”；v10 已删除 startup 分区，源码没有 startup 记录的生产者（`startup_record` 只被用作零哨兵/测试夹具）。
  6. §3.4.3（1749 行）说接收方“按各 `byte_len` 把七个分区复制”；`arch_boot_resume_v1` 的文档注释（`kernel/lib.rs:650`）也说“land all seven payloads”；同一口径还散落在 1583/2059/2271/3453 行。实际 `HandoffDescriptors` 只有六个 `byte_len` 字段（memmap/reserved/modules/platform/param/sources，`handoff_v1_read.rs:124-131`），接收方解码六类载荷。
  7. §3.4.5（1764 行）说直核腿“在信任边界上有两个所有者”，`BootStorage` 的 memmap/reserved/source 槽“留空（约 9 KiB 闲置容量）”。实际两个直核面都已把 map 表与 source 记录写进 `BootStorage`（`bootface.rs:376-387`、`bootface_a64.rs:348-359`），DTB 收集器已改为 `collect_owned`（栈上），直核腿只剩一个所有者；被写成“闲置”的槽正是现在唯一的落地区。
- 影响：读者会按文档去找不存在的 `STARTUP_RECORD_STORE`、以为 `check_boot_postconditions` 兜住了 `syscall_entry`/用户栈、或以为 `kmain` 会二次 store；这会把“哪道门在挡什么 bug”的判断建立在过期台账上。对设计评审尤其危险：v10 的收尾是否完整，直接决定 wire 版本与 strict 门是否可信。
- 证据或推理：逐条已在上表给出源行；补充 `rg -n "STARTUP_RECORD" os/` 只命中 `handoff_v1_read.rs:763` 的一条测试注释（引用已删除的 `WIRE_STARTUP_CAP`），`globals.rs` 零命中；`rg -n "store_kernel_info" os/kernel/src/lib.rs` 只有 225/347 两个调用点（1574 是定义，1580 是 one-shot 断言）。
- 最小修复：把 §4.4 两行改成现行断言集（`check_boot_postconditions` 只剩 higher-half；strict 门只剩模块位置名/authority/覆盖/内核跨度落表）；§4.5 删除 `STARTUP_RECORD_STORE` 行、把“七个落地区”改“六个落地区 + 名字池”、把 `store_kernel_info` 的调用点改成两个并写 one-shot 断言；§4.7 删除“startup-object 记录仍保留 arena 身份”或改写成“v10 后 arena 身份不再单独记录”；§3.4.3 与 `arch_boot_resume_v1` 注释的“七个分区/七个载荷”改成六个；§3.4.5 的“两个所有者/槽留空”改成 I-33 后的“`BootStorage` 一个所有者，槽全部使用”。同时把 `handoff_v1_read.rs:763` 的 `WIRE_STARTUP_CAP` 注释改成中性表述。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无新增概念；纯删除/改写过期陈述。减法方案“保留旧台账”拦不住的原因是：这些行是读者判断门强弱的唯一索引，过期即误导。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（纯文档修正）。
- 分类（D-047）：[DEL]（删除 v10 前残留的台账；不新增抽象）
- 置信度：高
- 是否需源码核实：已核实（上述行号与 grep）。
- 若我错了：如果这些表的“v10 前口径”是有意保留的历史对照，那么表头必须标明“v9 历史”而不是“现行门”；当前表头写的是现行校验点，不能两读。

## FINDING DESIGN-D2-03（P3）

- 位置：`os/libs/minix-boot/src/memmap_source.rs:250-272/304-320/598-620/755-768`；`os/libs/minix-boot/src/handoff_v1.rs:102-121/171-205/222-236`；`os/libs/minix-boot/src/boot_storage.rs:55`；`os/libs/minix-boot/src/lib.rs:37-41`。
- 严重度：P3
- 问题：v10 删除 startup 分区后，`InputObjectId::StartupObjects`、`SourceKind::StartupObject`、`SourceOrigin::StartupObject`、`KIND_STARTUP_OBJECT`/`ORIGIN_STARTUP_OBJECT` 与 `startup_record()` 都没有生产语义了。当前 `startup_record` 只被 `BootStorage` 的 `ZERO_RECORD` 零哨兵（`boot_storage.rs:55`）与一个测试的 `SourceOrigin::StartupObject` 排序键（`memmap_source.rs:2406`）使用；`handoff_v1_read.rs:404` 还留着一个未使用的 import；`SourceKind::StartupObject` 在 `resv_claims.rs:100` 与 audit 的排除分支里出现，但没有生产者能造出这条记录。`handoff_v1_read.rs:763` 的测试注释还引用已删除的 `WIRE_STARTUP_CAP`。
- 影响：没有行为风险（记录不可生产，解码分支只是死路）；但概念账多出三个同义枚举变体、一个公开构造器和一段死注释。读者会以为 startup 记录仍是一条活的 wire 通道——这正是 §4.7（3027 行）那条过期陈述的根。
- 证据或推理：`rg -n "startup_record" os/` 只有定义、`ZERO_RECORD` 与测试夹具；`rg -n "STARTUP_RECORD|WIRE_STARTUP" os/` 只剩 `handoff_v1_read.rs:763` 的注释。`handoff_v1.rs:102-121` 的注释说“values are protocol constants: never renumber”，因此 wire 数值 2/6/5 应保留为保留位，不能删；可删的是 Rust 侧的构造器与无生产者的分类分支。
- 最小修复：把 `ZERO_RECORD` 改用已有的 `EMPTY_SOURCE_RECORD`（`memmap_source.rs:332`），删除 `startup_record()` 与 `lib.rs` 的再导出；把 `SourceKind`/`SourceOrigin`/`InputObjectId` 的 Startup 分支标注为“保留 wire 值，无生产/消费路径”，或从 `audit_source_coverage` 的排除分支删除死匹配。`WIRE_STARTUP_CAP` 注释改成“v10 已删除 startup 分区”；`boot_storage.rs:300-301` 那段没有对应字段的“Startup-object records”文档注释也一并删除。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：减法就是“不做”：不新增任何类型；直接复用现有 `EMPTY_SOURCE_RECORD` 作零哨兵。减法方案“保留现状”拦不住的原因是：这些概念会继续产生“还有 startup 通道”的误读，且 `startup_record()` 是公开 API，未来调用者会把它当成活通道。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（纯删除）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（删除后少一个概念）。
- 分类（D-047）：[DEL]（C 没有 startup-object 这条 Rust 专有分类；删翻译残留）
- 置信度：高
- 是否需源码核实：已核实（grep 与上述行号）。
- 若我错了：如果团队有意把 startup 记录保留为未来功能（例如给 VM 记账）的预留通道，那么 `startup_record()` 应在文档里写明未来的生产者与消费者；当前 §3/§4 都写“已删除”，不能两读。

## FINDING DESIGN-D2-04（P2）

- 位置：`os/kernel-image/src/bootface_a64.rs:107-130`；对照 `os/libs/minix-boot/src/bump.rs:92-105`、`os/boot-shim/src/opensbi_helpers.rs:572-583`、`os/kernel/src/boot_alloc.rs:125-148`。
- 严重度：P2
- 问题：§3.8（2229 行）说两个 bump 实例共享 `minix_boot::BumpRegion`，“`alloc_pages` 的边界检查、`used_bytes` 与一致性谓词都只有这一份”。但 aarch64 直核面自己写了 `bump_alloc_pages`：`static mut BUMP_PTR` + `let need = (num_pages as u64) * 4096; if *ptr + need > BUMP_BASE + BUMP_SIZE { return None; }`。这是第二份边界检查，且乘加都没有 `checked_*`；riscv 直核面用的却是 `opensbi_helpers::bump_alloc_pages`（`bootface.rs:71` 导入），后者走 `BumpRegion`（`opensbi_helpers.rs:572-583`）。同一角色的两条直核腿，一条共享核心、一条手写。
- 影响：两处代数一旦漂移，同一 `PlacementPool` 约束在 aarch64 与 riscv64 上的行为不同；手写版的未检查 `+`/`*` 在极端输入下可回绕（虽然 `place_modules` 的 `in_memmap_range` 先把 `len` 限在 DRAM 内，当前不构成可利用路径，但它把“边界检查只有一份”的设计承诺打破了）。这是“翻译残留：C 的全局游标 + 手写算术”而不是“共享纯值核心”。
- 证据或推理：`rg -n "BumpRegion" os/` 的全部消费者是 `boot_alloc.rs`、`opensbi_helpers.rs` 与测试；`bootface_a64.rs` 零命中。`bootface_a64.rs:111` 的 `static mut BUMP_PTR` 与 `opensbi_helpers.rs:540` 的 `AtomicU64 BUMP_NEXT` 是两套状态表达；文档 §3.8 的“只有这一份”与事实不符。
- 最小修复：把 aarch64 的 `bump_alloc_pages` 改成与 `opensbi_helpers::bump_alloc` 同形：`static BUMP_NEXT: AtomicU64`（或一个 `BumpRegion` 值 + 原子游标），每次 `BumpRegion::resumed(BUMP_BASE, cursor, BUMP_END).alloc_pages(n)`；删除手写的 `need` 计算与 `*ptr` 写入。`BUMP_BASE`/`BUMP_SIZE` 仍留在 aarch64 文件（它们是板级常量），共享的是算术与边界检查。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：复用现有 `BumpRegion` + `AtomicU64`，不新增类型；这是减法（删掉第二份边界检查），不是加法。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（零新增概念）。
- 分类（D-047）：[DEL]（删 C 形的手写游标/算术，换现有共享值对象）
- 置信度：高
- 是否需源码核实：已核实（上述行号与 grep）。
- 若我错了：如果 aarch64 有意不用 `BumpRegion`（例如为了在 `.bss` 零初始化前就能用），应在 §3.8 写明例外与理由；当前文档把“只有一份”写成普适结论。

## FINDING DESIGN-D2-05（P2）

- 位置：`os/boot-shim/src/opensbi_helpers.rs:442/455-489/498-528`；`os/boot-shim/src/uboot_main.rs:452/459`；`os/boot-shim/tests/install_write_once.rs:1-75`；对照文档 §3.2（1468 行）的 `prepare_boot(&mut BootStorage, bump_pages)` 签名。
- 严重度：P2
- 问题：U-Boot 腿的真实输入（DTB 物理地址、`BootFileTable` 地址）不是 `prepare_boot` 的参数，而是两个进程级 `static mut`（`DTB_PTR`、`BOOT_FILE_TABLE_PTR`），经 `unsafe install_dtb_ptr`/`install_boot_file_table` 写入；`prepare_boot` 内部再读全局。这是 C 的“全局 `bootinfo`/全局表”形状的翻译残留：文档把入口写成两个参数，读者看不出这两个值从哪来；测试也无法在进程内复位，`install_write_once.rs:5-11` 明写因此只能开独立集成测试进程。
- 影响：第一，入口契约不完整——`prepare_boot` 的行为依赖调用前是否安装了两个全局，漏装会 panic（`opensbi_helpers.rs:250-252/265-266`）。第二，可测试性被削弱：全局一次性状态迫使命名负控放在单独进程（`install_write_once.rs` 的整个存在理由）。第三，`static mut` + 手写 one-shot 断言在 release 也生效，但 `install_dtb_ptr(0)` 时 `DTB_PTR == 0` 使下一次安装仍可通过断言（`opensbi_helpers.rs:513`），写一次语义对 0 值不成立；当前没有生产者装 0，所以是潜在面不是现行 bug。
- 证据或推理：`prepare_boot` 签名（`opensbi_helpers.rs:240-243`）只有 `storage` 与 `bump_pages`；两个输入在 `uboot_main.rs:452/459` 安装；测试文件第 5-11 行自述全局无法复位。`install_dtb_ptr` 的 `assert!(DTB_PTR == 0)` 对 `addr = 0` 不改变状态，第二次安装仍通过。
- 最小修复（减法优先）：把 `prepare_boot` 改成 `prepare_boot(storage, bump_pages, table: &BootFileTable, dtb_pa: Option<u64>)`，删除两个 `static mut`、两个 `install_*` 函数与 `boot_file_table()`/`dtb_ptr()` 读取器；`uboot_main` 在栈上建表后直接传引用。若担心改动公开签名，退一步至少把两个 `static mut` 换成 `AtomicU64` 并把“0 = 未安装”改成显式的 `Option`/`AtomicBool` 发布标志。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：参数传递是减法：删掉两个全局、两个 unsafe 安装器、一次进程级测试隔离。闭包不适用（值跨多个调用）；现有类型 `&BootFileTable`/`Option<u64>` 已足够。减法拦不住的理由：无——参数化能完整表达这两个输入，且让漏传变成编译错误；唯一代价是 `prepare_boot` 多两个参数（与文档 §3.2 的签名不同，需要用户裁决是否接受）。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新类型；`Option<u64>` 是现有类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净减两个全局概念；参数数 +2）。
- 分类（D-047）：[DEL]（删 C 形安装型全局，换函数参数）
- 置信度：高（源码与测试自述）
- 是否需源码核实：已核实（行号与测试文件）。
- 若我错了：如果这两个全局是为了让汇编入口 `uboot_main` 之外的路径也能读表（当前没有这样的路径），那么参数化会丢一条隐式通道；但 `rg` 显示只有 `uboot_main` 安装与 `prepare_boot` 读取，没有第二个读者。

## FINDING DESIGN-D2-06（P2）

- 位置：文档 §3.6（1872 行）、§3.7（2071/2086 行）、§4.6（2863 行）；实现 `os/arch/src/arch/paging.rs:212-470`、`os/kernel/src/lib.rs:1340/1767`。
- 严重度：P2
- 问题：§3/§4 对 `Paging` 的 API 面有两处过期、一处设计缺席：
  1. §3.6 说 `Paging` 有 22 个成员（`PAGE_SIZE` + 21 个方法）并列出 21 个方法名；实际 trait 还有 `is_supervisor_va`、`is_inherited`、`ensure_writable_range` 与 `const DM_HUGE`，文档列的 `adopt_active_root` 已被合并进 `from_active_root`（`paging.rs:193`），且 `new_from_page`/`from_active_root` 各多一个 `PteChannel` 参数（`paging.rs:270/284`）。成员计数与签名都不对（实际 25 成员 / 23 方法 / 2 常量，文档 22 成员 / 21 方法 / 1 常量；方法多三个、少一个（净 +2），另多 `DM_HUGE` 常量与 `PteChannel` 参数），尽管“职责面不窄”的结论更强。
  2. §3.7（2071 行）说接收侧“调 `Paging::new_from_page(root_page)`”；§4.6（2863 行）说 `pg_clear` 并入 `new_from_page` 的清零。实际签名要求 `channel: PteChannel`；引导期根建传 `PteChannel::Identity`（`kernel/lib.rs:1340`），运行期进程根传 `KernelDm`（`kernel/lib.rs:1767`）。
  3. `PteChannel`（`Identity`/`KernelDm`/`VmDm`，`paging.rs:196-210`）在 §3/§4 零命中。它承载一条真实约束：同一根页清零必须通过调用方声明的视图；`build_bootstrap_root_and_enable` 在 Direct Map 存在之前运行，只能用恒等视图。这是“重建活动根会坏”论证的配套条件，缺了它，读者会把 `new_from_page` 当成无状态的清零函数。
- 影响：读者会按 §3.6 的 22 项清单去理解 trait 的职责边界，漏掉三个方法；按 §3.7 的签名去理解根清零，漏掉“清零走哪个视图”这条启动期关键约束。对于设计评审，这会让“哪一层拥有 PTE 访问视图”的责任边界不可见。
- 证据或推理：`os/arch/src/arch/paging.rs:212-470` 的 trait 定义；`rg -n "PteChannel" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 零命中；`kernel/lib.rs:1340` 与 `:1767` 的两个调用点。`PteChannel` 的文档注释（`paging.rs:186-210`）自己说明 D-081/I-44 与 I-59 A'/D-094 的来源，但目标文档没有转述。
- 最小修复：§3.6 的成员计数改成实际值并补三个方法与 `DM_HUGE`；§3.7/§4.6 的 `new_from_page` 调用改成带 `PteChannel::Identity` 的签名，并加一段（3–5 行）说明 `Identity` 与 `KernelDm`/`VmDm` 的适用阶段与“清零视图与后续访问视图一致”的约束。不要求把整个 PteChannel 设计搬进本篇，只要求本章覆盖它出现的两个调用点。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：不新增类型/字段；`PteChannel` 已经在 `minix-arch` 里，报告只是要求文档转述现有类型。减法方案“删掉 PteChannel、回到无参 `new_from_page`”不可行：两个生产调用点面对相反的 DM 状态（§3.7 的“重建活动根”论证依赖这一点），删掉参数会让清零路径在 Direct Map 建立前访问不存在的映射。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（不新增类型；只文档化现有 `PteChannel`）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净新增概念 0；文档段落 +3–5 行，叙事冲突无）。
- 分类（D-047）：[MODERN]（C 的 `pg_clear` 清零语义在 Rust 侧用 `PteChannel` 显式表达；实现已存在，缺的是文档）
- 置信度：高
- 是否需源码核实：已核实（trait 定义与两个调用点）。
- 若我错了：如果 §3.6 的 22 项计数指的是某个更早的 trait 快照，且本章有意只描述与 `HugePages` 论证相关的六项，那么“22 个成员”这句应改成“当时快照”并删掉“21 个方法”的穷举口吻；但当前文字是“本身已有 22 个成员（……21 个方法：……）”，读者会当成现状。

## FINDING DESIGN-D2-07（P1）

- 位置：`os/boot-shim/src/uboot_main.rs:282`（`static mut SHIM_TABLE: [u8; 16 + 16 * 80]`）、`:349-432`（`build_shim_table` 的字节偏移写入）、`:459`（`install_boot_file_table(table_pa)`）、`os/boot-shim/src/opensbi_helpers.rs:476-489`（`boot_file_table()` 把 `usize` 强转成 `&BootFileTable`）。
- 严重度：P1
- 问题：U-Boot 腿的文件索引表没有用 `BootFileTable` 类型构造，而是把 `[u8; 1296]` 当原始字节按硬编码偏移写（magic 0、entry_count 8、pad 12、entry stride 80、path 0、addr 64、len 72），再把 `*const u8` 强转成 `&BootFileTable`。这同时踩中三件事：
  1. **对齐**：`BootFileTable` 是 `#[repr(C)]`，含 `u64`/`u32`，对齐要求 8；`[u8; N]` 静态只保证对齐 1。`boot_file_table()` 在 `opensbi_helpers.rs:484` 直接 `&*(ptr as *const BootFileTable)`，没有像 `table_magic_ok`（`bootface_common.rs:99-101`）那样检查 `pa % 8`。链接器“通常”会把大数组放到 8 字节边界，但语言层没有保证；RISC-V 上未对齐访问可能触发异常。创建未对齐的 `&BootFileTable` 本身就是 UB。
  2. **布局复制**：`build_shim_table` 的偏移常量是 `BootFileEntry` 布局的第二份真源；`SHIM_TABLE` 的长度 `16 + 16 * 80` 没有 `size_of::<BootFileTable>()` 的编译期断言。改 `BOOT_FILE_PATH_MAX`/`BOOT_FILE_TABLE_MAX_ENTRIES`/字段顺序时，编译器不会拦。
  3. **状态表达**：表是 `static mut` + 逐字节 `write_volatile`，而不是一个可命名字段的 `BootFileTable` 值；读者要同时读 `uboot_main.rs` 的偏移和 `opensbi_helpers.rs` 的结构体定义才能确认布局。
- 影响：这是启动主路径上的未定义行为面（对齐）和布局漂移面（第二份偏移表），故障形态会是“表 magic 读错/路径查找失败/加载长度读错”，而不是可读的构造错误。文档 §3.2 只把 `build_shim_table` 描述成“按最后一个 PT_LOAD 的文件尾修剪槽长度”，没有交代这个字节级构造器；设计评审看不到它。
- 证据或推理：`uboot_main.rs:282/357-432` 的硬编码偏移与 `opensbi_helpers.rs:122-160` 的 `#[repr(C)] BootFileTable`/`BootFileEntry`；`rg -n "size_of::<BootFileTable>|size_of::<BootFileEntry>" os/boot-shim` 零命中；`bootface_common.rs:99-101` 对 DTB 提供的表有对齐检查，反而 shim 自建的表没有。
- 最小修复：把 `SHIM_TABLE` 改成类型化的静态（例如 `static mut SHIM_TABLE: BootFileTable` 并用 `addr_of_mut!` 写字段，或直接在 `uboot_main` 内建一个栈上的 `BootFileTable`），`build_shim_table` 通过 `table.magic = ...; table.entry_count = ...; table.entries[i] = BootFileEntry { path, phys_addr, len };` 赋值；`boot_file_table()` 的指针来自 `&BootFileTable`，对齐由类型保证。若不想新增 cell 类型，最小改法是 `static mut SHIM_TABLE: BootFileTable = BootFileTable { magic: 0, entry_count: 0, _pad: 0, entries: [BootFileEntry { path: [0; BOOT_FILE_PATH_MAX], phys_addr: 0, len: 0 }; 16] }`（或用同义零字面量），仍用现有类型、零新增概念。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：复用现有 `BootFileTable`/`BootFileEntry` 类型，删除手写偏移与强制转换；不需要新类型。减法方案“保留字节数组但加一条对齐断言”拦不住布局复制与长度漂移，只能挡住对齐一项。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（不新增类型；只是把现有 `BootFileTable` 用作静态的实际类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净新增概念 0；删除 5 组硬编码偏移）。
- 分类（D-047）：[DEL]（C 的“按偏移手写结构体”是翻译残留；用现有类型删除它）
- 置信度：高（类型对齐与偏移复制均可静态核实）
- 是否需源码核实：已核实（`SHIM_TABLE` 类型、`build_shim_table` 偏移、`boot_file_table` 强转、`BootFileTable` 的 `repr(C)`）。
- 若我错了：如果 `SHIM_TABLE` 被链接脚本 `. = ALIGN(8)` 或段对齐保证到 8 字节，对齐这一条会降为“语言层仍不保证、但当前链接结果成立”；布局复制与无 `size_of` 断言这两条不受影响。即使链接器当前对齐，仍应改成类型化静态，避免下一次链接脚本调整把它变成故障。

## FINDING DESIGN-D2-08（P2）

- 位置：文档 §3.2（1518–1520 行）与 §4.3（2530–2582、2591–2627 行）；实现 `os/boot-shim/src/loader.rs:485-546`、`os/boot-shim/src/opensbi_helpers.rs:198-215`。
- 严重度：P2
- 问题：§4.3 的两段“代码引用”都不是当前源码：
  1. `UbootFileLoader::read` 的引文（文档 2603–2626 行）包含 `let used = minix_elf::segment_iter(slice)...; Some(FileBytes::Borrowed(&slice[..used]))`，并加了一段编注说源码里“The table stores the slot capacity, not the file length”这句注释已与实现不符。当前源码（`opensbi_helpers.rs:198-215`）**没有** `segment_iter` 二次修剪，直接 `Some(FileBytes::Borrowed(slice))`；注释已改成“D-082：read 只处理字节来源，表里的 len 已被 build_shim_table 修剪；这里再跑 segment_iter 是幂等无效功”。也就是说，文档引文描述的是被删掉的旧实现，且编注在纠正一句已经被源码改正的注释。
  2. `load_boot_modules_into` 的引文（文档 2530–2582 行）从 `let data = data.as_slice();` 直接到 `num_pages`；当前源码在两者之间还有 `let used = elf_file_end(data).unwrap_or(data.len()); let data = &data[..used];`（`loader.rs:509-518`）。§3.2（1520 行）的“loader 的二次修剪”把两层混在一起：`UbootFileLoader::read` 的 trim 已删除，而 `load_boot_modules_into` 的 `elf_file_end` 是**唯一**的 trim；对非 ELF，`elf_file_end` 返回 `None` → 保持全长，不是“非 ELF 回退整槽的输入才生效”。
- 影响：这是本章最容易被读者当作“源码事实”直接引用的两段；引文里的 `segment_iter` 与 `elf_file_end` 位置，正好决定“ELF 修剪归哪一层”这条设计边界。当前文档把“唯一修剪点在 loader 层”写成了“loader 二次修剪 + U-Boot 一次修剪”，与源码的“U-Boot 生产者修剪 + loader 唯一层修剪”相反。会误导后续修改者把 `segment_iter` 加回 `read`（重复一次无意义解析）或以为 `load_boot_modules_into` 不做修剪。
- 证据或推理：逐行对照上述行号；`rg -n "segment_iter|elf_file_end" os/boot-shim/src/opensbi_helpers.rs os/boot-shim/src/loader.rs` 显示 `segment_iter` 只在 loader 的 `compute_kernel_layout`/`elf_file_end` 内，`opensbi_helpers.rs` 的 `read` 无调用；源码注释（`opensbi_helpers.rs:206-210`）明写“这里再跑 segment_iter 是幂等无效功”。`loader.rs:517` 的 `elf_file_end` 是模块装载的唯一修剪点。
- 最小修复：按当前源码重写这两段引文（删除 `used` 计算与旧编注；在 `load_boot_modules_into` 引文里补 `elf_file_end` 两行）；把 §3.2（1520 行）的“二次修剪”改成“U-Boot 生产者按最后一个 PT_LOAD 修剪 `entry.len`；loader 在模块装载层再做唯一一次 `elf_file_end` 修剪（对已修剪输入是 no-op）”。这同时与 D-082/I-36 的现行注释一致。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无新增概念；纯删除过期引文/编注。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新类型）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（纯文档修正）。
- 分类（D-047）：[DEL]（删除已不存在的二次修剪叙述；不新增抽象）
- 置信度：高
- 是否需源码核实：已核实（两段源码逐行）。
- 若我错了：如果文档引文有意保留“历史实现”作对照，必须把引文头标成“历史版本（已被 D-082 删除）”；当前引文头写的是“代码引用，os/boot-shim/src/opensbi_helpers.rs:fn entry_path_eq 与 FileLoader for UbootFileLoader”，读者会当成现行源码。

## FINDING DESIGN-D2-09（P3）

- 位置：`os/boot-shim/src/ebs_pair.rs:32-37`（`static mut PAIR_STORAGE`）、`:171/292`（`addr_of_mut!` 取用）、`os/boot-shim/src/resv_claims.rs:248-270`（`DTB_RECORDS`/`DTB_RECORDS_N`/`CLAIMS`/`CLAIMS_N`/`DTB_RAW` 五个 `static mut`）。
- 严重度：P3
- 问题：配对循环的 `MapStorage` 与 DTB 覆盖检查的 claim/record 存储都是进程级 `static mut`。理由（EBS 循环内不能分配、栈可能紧）成立，但状态表达仍是 C 的“全局静态数组 + 计数器”：`MapStorage` 的 `len`/`side_len`/`snapshot_id` 与数组分离，`resv_claims` 的五个 `static mut` 与 `capture` 的重置顺序耦合。`BootStorage` 已经示范了“调用方持有的 owner + 借用视图”，`dtb_copy` 已经示范了 `Phase` 枚举；这两处没有跟进。
- 影响：不是现行 bug（单线程 boot + 注释已写清单线程契约），但有两个可测性/可维护性成本：`PAIR_STORAGE` 是跨 `preflight_dtb`/`pair_and_classify` 的隐式通道，测试只能靠 `RUST_TEST_THREADS=1`（`lib.rs:186-214` 的 test_serial 锁注释）；`resv_claims` 的 `capture` 先重置四个静态量再解析，任何未来的第二消费者都必须遵守同一顺序。这是“C 全局变量”形状的翻译残留。
- 证据或推理：上述行号；`tests/install_write_once.rs` 与 `lib.rs` 的 `test_serial` 注释都记录了进程级状态对测试的约束。`PAIR_STORAGE` 的大小（256×40 + 64×16 ≈ 11 KiB）在 `prepare_and_enter_kernel_image` 的栈上完全放得下；EBS 循环禁止的是“分配”，不是“栈上局部”。
- 最小修复（可选，P3 不阻断）：把 `MapStorage` 作为 `&mut` 参数穿过 `preflight_dtb`/`pair_and_classify`（调用方在 `prepare_and_enter_kernel_image` 内 `let mut map_storage = MapStorage::new();`），删除 `static mut` 与 `addr_of_mut!`；`resv_claims` 同理改成一个 `ClaimsState` 局部值 + `&mut` 参数，或在 `capture` 入口用 `Phase` 式状态断言（`Idle` → `Captured`）。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：复用调用方栈存储 + `&mut`，不新增类型（`MapStorage`/`Claims` 已存在）。减法方案“保留 static mut + 注释”拦不住的是：隐式全局通道让 `pair_and_classify` 的输入/输出无法从签名读出，测试隔离继续依赖进程模型。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（不新增类型；只是把现有存储从静态改成参数）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净新增概念 0；参数 +1，全局 -1）。
- 分类（D-047）：[DEL]（删 C 形全局 scratch，换调用方持有）
- 置信度：中高（设计上成立；是否值得改取决于 11 KiB 栈预算的实测）
- 是否需源码核实：已核实行号；未实测 boot-shim 栈余量（若栈只有 64 KiB，11 KiB 局部仍可接受，但需确认）。
- 若我错了：如果 UEFI 固件给的栈在 EBS 前小于 16 KiB，局部 `MapStorage` 会溢出；这种情况下应保留静态但改成 `SyncUnsafeCell`/one-shot 的显式 owner（与 `BootStorageCell` 同形），而不是裸 `static mut`。

## 翻译残留分类核查（任务书第 2 项）

| 类别 | 本篇范围内的实例 | 判决 |
|---|---|---|
| 单实现依赖注入 trait | 无。`BootShim` 已删除且全仓零命中；`FileLoader` 有 UEFI/U-Boot/Mock 三个实现；`Paging`/`HugePages` 有多架构实现。 | 无残留。 |
| 无状态 Manager | 无。`BootStorage` 是数据所有者，`Serializer` 是有计数器的写者，`MapStorage` 是 scratch；没有“只有方法的 Manager”。 | 无残留。 |
| 布尔/字符串状态机 | `ValidationLevel`、`dtb_copy::Phase`、`Disposition`、`PairError`/`MapError`/`PolicyError` 都是枚举；`BootStorage` 的四个 `*_committed: bool` 是 one-shot 标志（只有“未提交→已提交”一条转移），不是多状态机。找不到“用字符串表示状态”的启动代码。 | 布尔标志可保留；没有字符串状态机。 |
| 哨兵返回值 | `Option<FileBytes>`、`Option<(u64,u64)>`（bump region 未初始化）、`EMPTY_PHYS_RANGE`/`ZERO_*` 槽位哨兵、`DTB_PTR == 0`、`BootFileTable.magic`。前四类是显式 `Option`/私有填充值；`DTB_PTR == 0` 是 C 形全局哨兵，已归入 D2-05。 | `DTB_PTR` 那类应删；其余可保留。 |
| clone 到能编译 | 生产路径未见“为过借用检查而 clone”：`BootModule.name` 用 `&'static str` 代替 `String`，`FileBytes::Borrowed` 避免 U-Boot 腿的 `to_vec`，`BootModule`/`MemoryRegion` 是 `Copy` 的 POD。`MockLoader::read` 的 `d.clone()` 只在测试里。 | 无生产残留。 |
| 对象图共享 | 发送侧 `BootReport<'a>` 借用 `BootStorage`；接收侧 `KernelInfo` 的 `'static` 切片指向 `.bss` 落地区；没有 `Rc`/`Arc`/`RefCell` 对象图。`BootStorageCell`/`dtb_copy::PhaseCell` 是单线程静态 cell，不是共享对象图。 | 所有权方向正确；无需改。 |

**本节结论**：本篇的翻译残留集中在三个“C 形全局/手写构造”点（安装型全局 D2-05、`SHIM_TABLE` 字节表 D2-07、aarch64 手写 bump D2-04），而不是 trait/对象图层。trait 层没有死抽象。

## 最小化机会汇总（任务书第 5 项）

| 机会 | 删掉什么 | 外部行为变化 | 代价 | 判决 |
|---|---|---|---|---|
| D2-03：删 startup 记录构造器与无生产者分支 | `startup_record()`、一处未使用 import、三个枚举的活分支（wire 数值保留为保留位） | 无（无生产者） | 需同步一个测试排序键与 `ZERO_RECORD` | 接受（P3） |
| D2-05：`prepare_boot` 参数化 | 两个 `static mut`、两个 `install_*`、两个读取器 | 无（只改入口签名） | 公开签名变化，需用户裁决 | 需用户裁决 |
| D2-07：typed `BootFileTable` 静态 | 5 组硬编码偏移、一次 `*const u8` 强转、无对齐保证 | 无（表内容逐字节相同） | 需给 `BootFileTable` 一个零值构造字面量 | 接受（P1） |
| D2-04：复用 `BumpRegion` | aarch64 的 `need` 算术与 `static mut BUMP_PTR` 手写边界检查 | 无（同一池、同一顺序） | 需把游标换成 `AtomicU64` 或等价的单线程 cell | 接受（P2） |
| 合并 `BootAlloc::init` 与 `resume` | 一个方法 | 无 | 调用点要写 `resume(base, base, end)`，可读性下降 | 拒绝（不为一方法牺牲命名） |
| 合并 `prepare_kernel_image_v1`/`jump_kernel_image_v1` | 两个 `pub fn` 的公开面 | 无（仓内零外部调用者） | TODO-09 的路标拆分要重做 | 拒绝（零成本保留，且 `PreparedJump` 有防交换价值） |
| 删除 `FileLoader` trait 改闭包 | 一个 trait | 无 | 失去默认方法与两个实现族的命名 | 拒绝（测试接缝真实） |

## 死抽象核查（trait-audit 对账，非机械判决）

`trait-audit.json` 中落在本篇相关文件里的 trait 读数（已逐条回源码核实）：

| trait | 读数 | 回源码核实 | 判决 |
|---|---|---|---|
| `FileLoader` | `MULTI-IMPL`（impls 3 / refs 16 / dyn 1） | `loader.rs:146`；UEFI（`uefi_helpers.rs`）、U-Boot（`opensbi_helpers.rs:198`）、`MockLoader`（`loader.rs:583`） | 不是死抽象：两个生产实现 + 测试接缝，`read_required` 默认方法承载 fail-closed 语义。保留。 |
| `Paging` | `MULTI-IMPL`（impls 5 / refs 241） | `arch/paging.rs:212`；三架构 + mock + 测试桩 | 不是死抽象；但文档成员表过期（D2-06）。 |
| `HugePages` | `MULTI-IMPL`（impls 5 / refs 31） | `arch/paging_ext.rs:75`；三架构 + mock | 不是死抽象；`map_huge` 的 `supports_huge_page` 入口校验在 x86 上真实存在（`x86_64/paging.rs:1245`）。 |
| `BootArch` / `DirectMapArch` / `BootDmInstaller` / `HigherHalf` | 均 `MULTI-IMPL`（4–4–4–4） | `arch/boot_arch.rs:20`、`arch/direct_map.rs:31`、`arch/boot_dm.rs:353`、`kernel/boot/higher_half.rs:31` | 不是死抽象：三架构实例化。 |
| `PteWalkArch` / `ProtectionArch` / `TrapEntryArch` 等 | `MULTI-IMPL` 4 | `arch/pte_walk_arch.rs:60` 等 | 章外，保留。 |
| `EarlyConsole` / `ConsoleDesc` / `PlatformDesc` / `TimerDesc` / `InterruptControllerDesc` | `MULTI-IMPL` 3–7 | `plat/early_console.rs:55`、`minix-boot/platform.rs:350/415/434/448` | 不是死抽象：每架构/每平台实例化。 |
| `BootShim`（文档声称已删除） | `trait-audit.json` 无条目；`rg -n "trait BootShim\|impl BootShim\|dyn BootShim" os/` 零命中 | `boot-shim/lib.rs:3-9` 的模块注释也说明已删除 | 文档正确；无残留。 |
| `BklProtected` | `NO-IMPL`（refs 31） | `kernel/globals.rs:106` 是 marker/别名用法，不是 boot 篇对象 | 章外，不适用。 |
| `SchedulerDebugExt` / `PrivInfoBuild` / `ProcInfoBuild` | `SINGLE-IMPL-LOW-USE` 1 | `kernel/debug.rs:294`、`kernel/misc.rs:448/390` | 章外（调度/信息接口），不据本篇判死。 |

**结论**：本篇范围内没有“单实现依赖注入 trait”型死抽象；`FileLoader` 有真实测试接缝与两个生产实现，`HugePages`/`Paging` 有架构多实现。真正死掉的是**结构与构造器**：`startup_record()` 与 Startup 枚举分支（D2-03）、`SHIM_TABLE` 的字节级构造器（D2-07）；以及两个“未来才用”的公开分裂函数（`prepare_kernel_image_v1`/`jump_kernel_image_v1`，仓内零外部调用者）——但它们是零成本小函数且 `PreparedJump` 有防交换价值，按“不为一时的 API 洁净而删”保留，只在减法表里记为不采纳。

## ILLEGAL-STATE 清单（类型上可表示、语义上不可能）

| 状态 | 类型上可表示？ | 语义检查 | 判决 |
|---|---|---|---|
| `KernelInfo.authority == InputObjectId::StartupObjects` | 是（三值枚举，`kernel_info.rs:154`） | `validate_requiring_sources` 要求 `memmap_sources` 中存在同 authority 的记录；Startup 记录无生产者，故 fail-closed panic（`kernel_info.rs:450-459`） | 接受现状；不新增 `RamAuthority`（见减法表）。类型收窄的收益小于多一个概念，且当前失败是显式停机。 |
| `BootReport.memmap_sources` 为空 | 是 | 两义：DTB 腿“无来源记录”或 UEFI 腿“记录在 wire”（`boot_storage.rs:295-299`）；UEFI 腿的消费者不读该切片，接收侧 strict 门读解码后的 `KernelInfo` | 接受现状（文档已声明）；若未来有消费者在 UEFI 腿读它，必须显式分支，不能把空当“无记录”。 |
| `BootFileTable.entry_count` 大于容量 | 是（`u32` 裸字段） | `valid_entries` 用 `min(capacity)` 夹取（`opensbi_helpers.rs:164-167`） | 接受：夹取是安全兜底，不是静默越界。 |
| `BumpRegion`/`BootAlloc` 的 inverted/out-of-range 三元组 | 是（`resumed`/`init` 公开构造不校验） | `is_consistent` 在 `init`/`resume`/`region` 与 `BumpRegion::alloc_pages` 的调用点断言；可信链上用 `assert!` 正确 | 接受：不改成类型/Result（可信链 fail-fast 是正确形态）。 |
| `BootStorage` 部分填充（某族未 fill） | 是（`fill_*` 各自独立） | 下游 `KernelInfo` 校验：memmap 非空、生产内核 boot_modules 非空、param<=1024；platform 空合法 | 接受：可选载荷空是合法状态，必需载荷由接收侧门兜住。 |
| `SHIM_TABLE` 被当成 `&BootFileTable` 但地址未对齐 8 | 是（`[u8; N]` 静态对齐 1，`boot_file_table()` 裸强转） | 无运行期对齐断言；`table_magic_ok` 只检查 DTB 提供的表地址，不检查自建表 | **不接受**：D2-07 修。 |
| `MemoryRegion.len` 为零/未页对齐 | 是（`usize` 裸字段） | `KernelInfo::validate` 逐条断言非零、base/len 页对齐、span 不溢出（`kernel_info.rs:372-403`） | 接受：交接结构是进程内值，边界校验在接收侧；可信链上的 `assert!` 是正确形态。 |
| `BootFileEntry.len` 为任意 u64 | 是（原始表字段） | 内核 `place_modules` 用 `in_memmap_range` 先证明源区间在 memmap 内（`bootface_common.rs:229`）；boot-shim `UbootFileLoader` 只读 shim 自建表 | 接受：两条消费者各有源区间门；不需要在类型上做长度上限。 |
| `PageSize::try_from_bytes` 接受任意 2 的幂 | 是（任何 >=4K 的 2 的幂） | `map_huge` 各实现用 `supports_huge_page`/`HUGE_PAGE_SIZES` 拒绝不支持尺寸（x86 `paging.rs:1245`） | 接受：能力查询是运行期事实，不能压进 `PageSize` 类型。 |

## ARCH-PROPOSAL

本轮无 ARCH-PROPOSAL（无“改变 boot 是什么”的提案）。所有采纳项都是删除翻译残留、复用现有类型/参数或文档修正，未引入新机制、新责任、新检查或新策略；因此不产生 `[ARCH]` 三处一致计划。

## VERDICT

**逐条结论**

| FINDING | 结论 | 理由一句话 |
|---|---|---|
| D2-01（P1，wire 版本） | 接受（修） | 文档/提交说明都写 10，源码 9；一行常量与文档，不改会让版本契约失真。 |
| D2-02（P1，v10/D-071 台账） | 接受（修文档） | 七处过期台账是读者判断门强弱/所有权归属的索引，必须与源码同步。 |
| D2-03（P3，startup 死代码） | 接受（P3 清理） | 无生产者；删除构造器与死分支，wire 数值保留为保留位。 |
| D2-04（P2，aarch64 bump） | 接受（修） | 第二份边界检查违反 §3.8 的“只有一份”承诺，且手写 `+` 未检查。 |
| D2-05（P2，安装型全局） | 需用户裁决 | 参数化是减法，但改公开签名；至少应把 `static mut` 换 `AtomicU64` 并补 0 值写一次漏洞。 |
| D2-06（P2，Paging 文档） | 接受（修文档） | 成员数/签名过期，`PteChannel` 章内缺席；代码无需改。 |
| D2-07（P1，SHIM_TABLE） | 接受（修） | 未对齐 `&BootFileTable` + 第二份偏移表；用现有类型即可消除。 |
| D2-08（P2，§4.3 引文） | 接受（修文档） | 两段“代码引用”都不是现行源码，会把 ELF 修剪层读反。 |
| D2-09（P3，静态 scratch） | 需用户裁决（P3 可选） | 11 KiB 栈预算需实测；不改也能用，只是测试隔离成本继续存在。 |

**值得改（P1/P2，按优先级）**

1. **D2-07（P1）**：`SHIM_TABLE` 的字节偏移构造 + 未对齐 `&BootFileTable` 强转。这是启动主路径上的 UB/布局漂移面，修法零新增类型（用现有 `BootFileTable`）。
2. **D2-01（P1）**：wire 版本常量仍是 9，而设计/提交说明都写 v10。一行常量 + 文档；不改会让“版本精确比较”这条契约长期失真。
3. **D2-02（P1）**：§3.4.3/§3.4.5/§4.4/§4.5/§4.7 的 v10/D-071/I-33 台账过期（七处）。这是读者判断“哪道门在挡什么”的索引，必须与源码同步。
4. **D2-04（P2）**：aarch64 直核腿的手写 bump 算术，违反 §3.8“边界检查只有一份”的承诺；复用 `BumpRegion` 即可。
5. **D2-08（P2）**：§4.3 两段代码引文过期（`segment_iter` 二次修剪 / `elf_file_end` 缺失）。直接改文档，避免把“唯一修剪层”读反。
6. **D2-06（P2）**：`Paging` 成员表与 `new_from_page` 签名过期，且 `PteChannel` 在章内缺席；补文档，不改代码。
7. **D2-05（P2，需用户裁决）**：`prepare_boot` 的两个输入藏在安装型全局里。参数化是减法，但会改公开签名；若团队要保签名，至少把 `static mut` 换成 `AtomicU64` 并把“0=未安装”的写一次漏洞补上。

**合理取舍（不建议改）**

- `FileLoader` trait：两个生产实现 + Mock，测试接缝真实；不是“单实现 DI”。
- `HugePages: Paging` 扩展 trait：三架构实现 + `supports_huge_page` 能力查询；`map_huge` 的 `AlreadyMapped` 守卫在 1 GiB 叶场景有真实语义（§3.6 的 1919–1921 行推演与源码一致）。
- `BootStorage`/`BootReport`/`BootStorageCell`：所有者 + 借用视图 + one-shot cell 的划分与源码一致；`fill_*` 的写与提交同步、`MapCounts` 防交换，都是有效的现代表达。
- 直核腿的 `'static` 切片：指向函数内 `static STORAGE`，是真实静态存储；虽然要 `unsafe`，但没有“假所有权”（对比 v8 的 `Box::leak`）。文档只需把“两个所有者”改成 I-33 后的“一个所有者”。
- 可信链上的 `assert!`/panic（`KernelInfo::validate`、`BumpRegion::is_consistent`、`FileLoader` 缺件、`pt_alloc` 重注册）：按任务书的判据，这是正确形态，不改成类型或 `Result`。
- `InputObjectId` 兼作 authority 与记录身份：存在“authority=StartupObjects”的非法组合，但 strict 门 fail-closed；不为此新增 `RamAuthority`。
- `BootReport.memmap_sources` 的 UEFI 空切片：文档已声明“wire 是唯一权威”；空切片 + 静态约定是可接受的减法（不需要 `records_in_wire: bool`）。
- `BootStorage::fill_map_tables` 没有 one-shot 标志（其余四族各有一个）：EBS 重试循环**在闭包内部**，生产只调用一次；再加一个 `bool` 只能防未来误用，不改变当前可表示状态。保留。

**需要用户裁决**

- D2-05 的 `prepare_boot` 签名变更（参数化 vs 保留 `AtomicU64` 安装全局）。
- D2-01 若团队其实想保留 version 9（只靠 `size` 判别），请改设计文档与提交说明；两者不能同时写 10。
- D2-09（P3，可选）：`PAIR_STORAGE`/`resv_claims` 的 `static mut` 是否值得为可测性改成调用方局部；取决于 boot-shim 栈余量实测。

**不建议做**

- 不为“更 Rust”恢复 §4.4 已删除的 `syscall_entry`/`kern_stack_top` 派生等式（它们是恒等式重抄，不是安全检查）。
- 不新增 `RamAuthority`、`ShimTableCell`、`BumpCursor` 等类型（减法表已给出复用方案）。
- 不把 `FileLoader` 改成闭包；不把 `HugePages` 并回 `Paging`；不把 `PreparedJump` 拆掉。
- 不把 `KernelInfo` 的 `&'static [T]` 改成带生命周期的泛型（会改动全局槽/接收侧落地区，外部行为不变但成本高，收益只是类型洁癖）。

## 完成声明

- 本报告已按回退形态覆盖目标文档 §3（1392–2267 行）与 §4（2269–3139 行）的机制主张，并对 `related-code.txt` 中 boot-shim、minix-boot、kernel、arch、kernel-image 的核心文件逐段核对。
- FINDING 条数：**9**（P1×3：D2-01/D2-02/D2-07；P2×4：D2-04/D2-05/D2-06/D2-08；P3×2：D2-03/D2-09）。
- ARCH-PROPOSAL 条数：**0**。
- SUBTRACTION-TABLE 行数：**8**；其中“采纳=是”4 行（typed `BootFileTable`、`BumpRegion` 复用、`PteChannel` 文档、`prepare_boot` 参数化），其余 4 行不采纳并写明理由。
- 本轮没有新增类型/trait/字段；所有采纳项都复用现有类型或纯删除/纯文档修正。
- 未运行任何写操作于源码/文档/轨迹；唯一写入是本报告文件。
