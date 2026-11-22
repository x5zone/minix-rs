DOC 01-boot-shim-bootstrap.md

VARIANCE: D-024 D1 停跑，D2 采用文档主张↔源码事实回退

# D2 · 独立设计第二遍：文档 Ch3/Ch4 主张 ↔ 现有 Rust 源码事实

## 0. 方法与输入边界

- 基线：无 D1 产出（D-024）。本条按 brief 的回退形态执行——先从 `minix3/` 重建 C 语义合同，再把
  `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3/§4 的每条可核对主张落到源码事实上。
- C 侧重建的入口：`minix3/minix/include/minix/param.h`（`kinfo_t` 全字段）、
  `minix3/minix/kernel/arch/i386/pre_init.c`（`get_parameters`/`mb_set_param`/`overlaps`/`pre_init`）、
  `minix3/minix/kernel/arch/i386/pg_utils.c`（`add_memmap`/`cut_memmap`/`alloc_pagetable`/
  `pg_alloc_page`/`pg_identity`/`pg_mapkernel`）、`minix3/minix/kernel/main.c`（`bootstrap_start/len`
  归还点、`kernel_may_alloc` 归零点）。
- Rust 侧按 §7.2/§7.3 的符号锚点选择性阅读（`related-code.txt` 48 文件 / ≈54.9k 行），逐条用
  `grep`/`sed` 落到行号；未读作者解释、未读历史评审、未读其它面板角色输出。
- 本轮审查期间代码冻结，本报告只读源码，未写 `os/`。
- 分类口径（D-047）：`[MODERN]` = C 有该语义、换现代表达；`[DEL]` = C 无此物、删翻译残留；
  `[ARCH]` = 改变 boot 是什么（机制替换、新责任/字段/检查/策略），只进 `## ARCH-PROPOSAL`。
  **本报告另有一类“文档事实校正”**：它不删对象、不改机制，只让既有语义被正确表达，按
  `[MODERN]` 记并在条目内注明“文档校正，无机制变更”；这类不进 ARCH 台账。

---

## DIFF-TABLE

> 每一行的“源码事实”都带文件:行号；“差异”只描述主张与事实之间的距离，不重复判断。

| 维度 | 文档 Ch3/Ch4 主张 | 源码事实 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `KernelInfo` 共 10 个字段；`user_sp`/`kern_stack_top`/`syscall_entry` 不上 wire，消费点派生（§3.2.1、§3.2.2.3，doc:1170、1259） | `os/libs/minix-boot/src/kernel_info.rs:20-148` 恰 10 字段；`:193` 为 `kern_stack_top()` 访问器；`os/libs/minix-boot/src/boot_storage.rs:309-313` 明写“No user_sp / kern_stack_top / syscall_entry”；`handoff_v1.rs:278` `BOOT_HANDOFF_V1_VERSION = 10` | 无 | 接受：主张与源码逐项一致 |
| 状态表达 | “`fill_*` 各自在闭包返回时提交计数，半提交状态不可表示”（§3.7，doc:1843） | `boot_storage.rs:101-105` 是 5 个 `*_committed: bool` + 5 个长度字段；每个 `fill_*` 先写槽再提交计数（`:141-223`）；但 `report()`（`:256-277`）不检查五个提交位，测试 `a_fresh_storage_reports_empty_lists_and_zero_param_bytes`（`:385`）明确断言“空 storage 也能出一份空报告” | 族内半提交确实不可表示（真）；跨族的“部分族已提交”状态可表示、可出报告（文档没说这一点，但也没有反命题） | 接受；记入 VERDICT：完整报告的约束由下游门兜（`validate_before_tables` 的 memmap 非空 + Standard 的 boot_modules 非空），不值得为它新增类型 |
| 状态表达 | `PlatformDescSource` = 16 B 句柄，`len = 0` 表示生产者未记录长度（§3.2.2.3、§3.2.3，doc:1260、1321） | `os/libs/minix-boot/src/platform.rs:120-183`：`#[repr(C)] {kind: PlatformDescKind, len: u32, phys_addr}`，`new()` 置 `len = 0`，`has_len()/len()` 两个访问器；消费者 `os/kernel/src/lib.rs:1216-1227` 按 `has_len()` 分道，否则 `push_2m` | 事实一致；但“未知”与“已知 0”共用一个整数，两个访问器只是把哨兵重新展开 | 表达可改进：见 DESIGN-04 |
| 状态表达 | boot bump 分配器是“三原子 + 重建 `BumpRegion` 值”（§3.4.2 引文，doc:1443-1458） | `os/kernel/src/boot_alloc.rs:35-155`（`base/next/end: AtomicU64`，`alloc()` 每页重建 `BumpRegion`）；U-Boot shim 的 `BUMP_NEXT` 同为 `AtomicU64`（`os/boot-shim/src/opensbi_helpers.rs:539`） | 无；源码两种实现都自述“原子只替代 `static mut`，不提供线性化”（`boot_alloc.rs:14-22`、`opensbi_helpers.rs:534-537`） | 接受；记录：类型的并发外观比其保证更强，读者需读注释才知道（非缺陷） |
| 身份 | 模块身份 = 位置 + 名字；strict 门按位置核名；换名会让构建失败（§4.3、§4.4.1，doc:2168-2179、2387） | `kernel_info.rs:604-613` `NR_BOOT_MODULES=12` 与 `MODULE_NAMES`；`KernelInfo::validate_impl_inner` 的 strict 分支逐位比较 `module.name == MODULE_NAMES[i]`（`:451-458`）；`os/kernel-image/src/bootface_common.rs:70-84` 编译期逐字节对账 | 无 | 接受 |
| 身份 | `authority` 记录内存图由哪个输入对象声明；只有 DTB/UEFI 两值（§3.2.1、§3.3，doc:1185、1333） | `os/libs/minix-boot/src/memmap_source.rs:591-635`：`InputObjectId` 恰两变体（tag 2 保留位）；`RamAuthority(InputObjectId)` 只有 DTB/UEFI 两个 const；`handoff_v1_read.rs:160-164` 对 tag>1 返回 `BadRamAuthority` | 无；`RamAuthority` 与 `InputObjectId` 在值域上同构，它的实际作用是阻止把某条 `SourceRecord.input_object`（运行期值）当作权威传入 | 接受：newtype 不防“非法值”，防的是“错配来源”，有价值但不进减法表 |
| 身份 | `SourceRecord` 给每个投影字节记来源：kind/origin/range/input_object/attribute（§3.3，doc:1333） | `memmap_source.rs:520-546` 共 11 字段：另有 `id`、`local_source`、`snapshot_id`、`virtual_start`、`address_domain`、`raw_type`；`os/boot-shim/src/ebs_pair.rs:138-145` 明确 `snapshot_id` 不是 MapKey——“两次读返回同一 key 仍是两个快照” | 文档的字段枚举是子集，且漏掉了 `snapshot_id` 这层观测身份；不影响“每个字节可追溯”的结论 | 接受；记录：文档身份模型不完整（不单列 FINDING） |
| 身份 | `KinfoStruct` 七个语义字段，“四个有 Rust userland 读者”，`user_sp` 的语义消费者不是 `KinfoStruct.user_sp`（§3.2.2.2，doc:1248） | `os/kernel/src/misc.rs:732-741` 把 `KinfoStruct.user_sp` 填成 `USER_STACK_TOP`；`os/servers/rs/src/trap_api.rs:225-230` 从 GET_KINFO 的 `buf[8..16]`（即 `user_sp`）取值，`:430` 交给 `stack_fill` 决定 exec 栈帧落点 | 存在第五个 Rust userland 读者：RS 的 exec 路径；文档把它说成“不是读者”，并且与同一段的 `[ARCH]` 标签“五个 used 字段 + 两个占位”自相矛盾 | **P1 文档事实错误**：DESIGN-01 |
| 所有权 | `BootStorage` 拥有六类载荷，`BootReport<'a>` 只是借用视图；跨镜像发送侧序列化，接收侧在 `.bss` 重建切片；“三形态一致的是必须深拷进私有存储”（§3.2、§3.7、§4.4，doc:1146、1843、2286） | `boot_storage.rs:86-314` 确实如此；但直核腿（`bootface_common.rs:357-358`、`:551-558`）必须先 `STORAGE.take()` 再 `report_static()` 把借用抬成 `'static`，才能调 `build_kernel_info` 造 `KernelInfo`；`os/boot-shim/src/lib.rs:137-188` 的 `BootStorageCell` + `report_static` 是这条抬升的唯一桥 | 源码里有三种所有权形态：v1 发送侧借用、直核发送侧伪造 `'static`、接收侧真 `'static`（`.bss`）。文档把三者讲成一条“按值交接”，没区分“同镜像只需活过一次 `-> !` 调用”这一事实 | **减法机会**：DESIGN-02 |
| 所有权 | arena 生命周期合同：解码是 arena 最后一次读，`LastRead(arena) ≺ Publish(VMFreePages)`（§4.6，doc:2545） | `os/kernel/src/lib.rs:642-656` 的 `arch_boot_resume_v1` 文档注释复述同一合同，但随后仍写“the arena is still named by a startup-object record … stays in the boot handoff's deduction inputs”；`store_kernel_info` 的注释（`:1664-1665`）已写“There is no startup-object class” | 行为一致（v10 确无 startup 记录/分区），但源码注释仍留 v9 叙述 | 源码注释残留：DESIGN-05 |
| 错误 | 四类错误各有落点：缺来源 → `FreeSpanWithoutMemorySource`；权威不匹配 → “declared RAM authority … has no source records”；半提交 → 第二次 `fill_*` panic；跨镜像指针 → 读到零或 fault（§3.7，doc:1847） | `memmap_source.rs:310-314`、`:337-401`；`kernel_info.rs:459-479`；`boot_storage.rs:141-223` 的 `assert!(!*_committed)`；`store_kernel_info` 的“真实机器读数”注释（`lib.rs:1574-1585`） | 无 | 接受 |
| 错误 | 失败一律 fail-closed；boot 期无 io/errno 语义（§3.7、§4.2.2） | 内核侧 `panic!`/`assert!`；直核面 `say + crate::halt()`（`bootface_common.rs:368-438`）；两者都是停机，但通道不同（panic handler vs 裸串口） | 无行为差异；文档未并列两种通道，读者可能以为 boot 期错误只有 panic 一条路 | 接受；记录（叙述完整性，非缺陷） |
| 并发 | boot 单线程；`pt_alloc` 写一次、第二次注册 panic（§3.4.2，doc:1416-1429） | `os/arch/src/arch/pt_alloc.rs:55-67` `compare_exchange(0, …)` + `unwrap_or_else(panic!)`；`:99-105` 的 free 槽同构 | 无；文档说“一个没有参数的函数指针槽”，源码其实是 alloc + free 两个槽（`PT_ALLOC`/`PT_FREE`），Paging 侧只看到 `alloc_pt_page` | 接受；措辞不精确，见 DESIGN-06 |
| 并发 | `'static` 只由 `report_static` 的 one-shot 静态存储支撑，不从堆泄漏（§3.7） | `os/boot-shim/src/lib.rs:137-188`：`UnsafeCell<BootStorage>` + `AtomicBool taken`；`take()` 写一次；`report_static` 不再检查 `taken`，依赖 boot 契约 | 无；`take()` 与 `report_static` 之间没有类型级互斥（`report_static` 可在 `&mut` 存活时给出 `&'static`），但生产调用点先 `let _ = storage;` 结束借用 | 接受现状；DESIGN-02 的减法会顺带删掉这个桥 |
| 抽象边界 | `FileLoader` 是唯一固件接缝；`BootShim` trait 已删除、当前树零命中；`prepare_boot` 是普通函数（§3.5，doc:1549-1551） | 全仓 `grep -rn "BootShim" os/ --include=*.rs` 零命中；trait-audit：`FileLoader` impls=3 / refs=16 / dyn=1；`prepare_boot` 在 `opensbi_helpers.rs:240`，`uboot_main.rs:476` 直接调用 | 无 | 接受 |
| 抽象边界 | `KernelInfo`/`MemoryRegion`/`BootModule` 不用 `#[repr(C)]`，字段顺序不是契约（§3.2.3，doc:1280） | 三者确无 `repr(C)`（`kernel_info.rs:19`、`:578`、`:616`）；但同一节后文文档化的 `PlatformDescSource` 是 `#[repr(C)]`（`platform.rs:120`），其 16 B 形状正来自该属性 | 文档的三类型清单不完整：第四个跨侧结构体确实带 `repr(C)`，而理由不同（为 16 B 句柄形状，不是 ABI） | 文档精度：DESIGN-06 |
| 抽象边界 | `pt_alloc` 的槽“同时服务内核自举页表与 VM 页表；`register_free` 只对应 VM 侧池”（§3.4.2，doc:1476） | `os/servers/vm/src/vm_server.rs:379-390` 先 `is_registered()` 再 `register(vm_pt_alloc)`，再 `is_free_registered()` 后 `register_free(vm_pt_free)`；`alloc_page.rs:32/67` 两函数；内核侧只在 `boot_validate_and_prepare` 注册 alloc（`lib.rs:924-926`），不注册 free | 无；这正是文档说的“成对，未注册归还钩子时销毁路径只清零根、中间页泄漏” | 接受（已声明的取舍） |
| 抽象边界 | 四个 manifest 的依赖表逐条列出（§3.5，doc:1645-1652） | 实读四份 `Cargo.toml`：`os/boot-shim` 有 minix-types/minix-boot/minix-arch/minix-kernel/minix-plat/minix-elf/fdt/可选 uefi 0.33 + uefi-raw 0.9；`os/kernel` 无 boot-shim、无 uefi；`os/libs/minix-boot` 只有 minix-types + fdt；`os/kernel-image` 五项 + riscv64/aarch64 target 的 boot-shim/fdt | 无 | 接受 |
| 抽象边界 | 内核侧 `memmap` “不保证已扣除全部占用区”，每个消费者自己扣（§3.2，doc:1191） | `kernel_info.rs:21-29` 同述；`os/kernel/src/vm_handoff.rs:452-517` 是唯一的分配侧扣除；`os/kernel/src/lib.rs:2717-2749` 的 `vm_boot_exclusions` 复用 `for_each_live_bootstrap`（`vm_handoff.rs:741-770`），没有第二份手写清单 | 无；扣除成员表只有一份（文档的“可枚举 choke point”成立） | 接受（合理取舍：把“已扣除视图”从类型里拿掉，换取 C 的 `add/cut` 账本退役） |
| 可测试性 | 契约测试名与负控：`boot_windows_reject_reversed_ranges` / `boot_windows_reject_empty_ranges` / `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` / `drops_tail_records_whole_at_capacity` / `splits_nul_and_whitespace_records` / `rejects_internal_key_override` / `generates_arch_and_board_first`（§3.2.2.4、§3.4、§4.6） | 全部存在：`kernel/src/lib.rs:5910/5917`、`vm_handoff.rs:1088`、`boot_params.rs:216/229/237/276` | 无 | 接受 |
| 可测试性 | `FileLoader` 提供测试接缝（`MockLoader`），装载单元测试经由它喂字节（§3.5，doc:1609） | `os/boot-shim/src/loader.rs` 的 tests 有 `MockLoader`；trait-audit `dyn=1` | 无 | 接受 |
| 可测试性 | 宿主测试不能证明任意畸形输入会被拒绝（§5 章首，doc:2590） | 解码层把畸形输入变成 `Result`（`handoff_v1_read.rs:100-232`）且是宿主可测的；`ebs_map.rs` 的分类器也是宿主可测的一半 | 文档的边界陈述偏保守：解码/分类两层的拒绝面其实可由宿主测试逐条覆盖；“不能证明”的是整链行为 | 接受（保守陈述不是错误）；记录 |

---

## SUBTRACTION-TABLE（V3.6，C-32）

> 顺序按 brief 要求：先减法后加法。本轮的**主提案全部是减法**（DESIGN-02/03 删对象，DESIGN-01/05/06
> 删错误断言），下表列出唯一一个“引入 std 类型”的咨询项与两个被否决的加法咨询项。
> 最后一列是精确值 `是`/`否`。

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| `PlatformDescSource.len: u32` 哨兵改为 `Option<NonZeroU32>`（DESIGN-04） | ①不做：保留 `len = 0` + `has_len()`/`len()` 两访问器（现状）；②只加 `assert!(len > 0)` 到 `with_len`（一条断言，不换类型） | ①不拦任何东西（保留魔法值）；②也拦不住：`new()` 合法地把 0 写成“未知”，所以 0 在类型里永远同时是“未记录”与“零长”，断言只能拦显式 `with_len(0)`，拦不住“未知被消费点读成 0 长度” | ①相同（都不校验）；②略强于①但漏掉未知/零长的歧义；`Option` 把两状态在类型上分开 | +1（`NonZero` 变体）/ −1（删 `has_len`）= 净 0；16 B 形状不变（`Option<NonZeroU32>` 占 4 B） | 是 |
| 新增 `ReadyReport` / `finish()` 令牌，让“五族未全提交”不可报告 | 在 `report()` 开头加一条 `assert!(五个 *_committed)`（一个条件，不加类型） | 减法能拦住（因此不采纳加法）：同一处 fail-fast，信息量相同；现有下游门（`validate_before_tables` 的 memmap 非空、Standard 的 boot_modules 非空）已能把空/半报告变成有名停机 | 相同 | +0（一条断言）对 +2（新类型 + 消费方改签名） | 否 |
| 给 `RamAuthority` 再加一层 `AuthorityRecord`/`Snapshot` 值对象，承载 snapshot 身份 | 保留现状：`RamAuthority(InputObjectId)` + `SourceRecord.snapshot_id` 两个既有字段 | 减法能拦住（因此不采纳加法）：`snapshot_id` 已能区分同 key 的两次快照（`ebs_pair.rs:138-145`），再加一层不增加任何可判约束 | 相同 | +2 对 +0 | 否 |

> 本轮没有“新增 trait / 新增层 / 新增字段”的提案；DESIGN-02 是净删除（−2 类型、−1 函数、−1 方法、
> −1 静态存储），因此不进本表的“加法”列。

---

---

> **顺序说明（C-32）**：下面先给减法——DESIGN-01 是先决的事实校正（不删对象，但必须最先改，
> 否则后续删除判断建立在假事实上），DESIGN-02 与 DESIGN-03 是净删除，DESIGN-05 是删源码叙述残留；
> 之后才是本轮唯一的表示替换 DESIGN-04（净概念 0，见减法表）与文档精度校正 DESIGN-06。
> 没有“新增类型/层/字段”的加法提案排在任何减法之前。

## FINDING DESIGN-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.2.2.2（doc:1248），并波及
  §3.2.2.1 的 `user_sp` 行（doc:1232）与 §4.4 字段表的 `user_sp` 行（doc:2316）。
- 严重度：P1。
- 分类：`[MODERN]`（文档事实校正；不删对象、不改机制，按报告开头的口径记）。
- 问题：文档说 `KinfoStruct`“七个语义字段……**四个有 Rust userland 读者**：`nr_procs`、`nr_tasks`、
  `release`、`version`；`user_sp` 的语义消费者走 `VmBootHandoff.user_sp`/`USER_STACK_TOP`，
  **不是 `KinfoStruct.user_sp`**”。源码事实：`KinfoStruct.user_sp` 有一条活的 Rust userland 读取链——
  RS 的 `TrapKernelApi::user_sp()` 用 `sys_getinfo_into(GET_KINFO)` 把结构拷进 48 B 缓冲，
  取 `buf[8..16]`（`KinfoStruct.user_sp` 的固定偏移），再交给 `stack_fill` 决定 exec 时用户栈帧的落点。
  同时文档自己的 `[ARCH]` 标签写的是“五个 used 字段 + 两个 C 布局占位”（doc:1248 同段），
  与正文的“四个读者 + user_sp 不是本字段”自相矛盾。
- 影响：这条主张的用途是论证“GET_KINFO 已收窄为 used-fields 子集”。若后人按“`user_sp` 无读者”
  删除或恒零化该字段，RS 的 exec 路径会拿到 `user_sp = 0`，`stack_fill` 要么 `EINVAL` 要么把 argv/envp
  帧放到错误位置——症状是 exec 失败或子进程一进用户态就崩，且现场离“读者被删”很远。
  这属于“删除依据建立在假事实上”，比一般的措辞错误严重。
- 证据或推理：
  - `os/libs/minix-types/src/types/kinfo.rs:27-48`：`KinfoStruct` 七字段，`user_sp` 在 offset 8（同文件测试
    `test_kinfo_layout_frozen` 只冻结布局，不涉及读者）。
  - `os/kernel/src/misc.rs:729-741`：`getinfo_kinfo` 把 `user_sp` 填成 `minix_types::USER_STACK_TOP`。
  - `os/servers/rs/src/trap_api.rs:223-230`：注释明说 “Rust substitutes a GET_KINFO struct copy
    (`KinfoStruct.user_sp@8`)”，代码 `i64::from_le_bytes(buf[8..16])`。
  - `os/servers/rs/src/trap_api.rs:430`：`let user_sp = self.user_sp()?;` 后直接进
    `stack_fill(&argv, &empty, params.frame_size, user_sp, &mut frame)`——语义消费者，不是诊断输出。
  - 对照：`os/servers/is/src/dump_kernel.rs:570-576` 的 `render_kenv` 读的确实只有四个字段，
    这解释了“四个读者”的来源——它漏掉了 RS 这一条。
  - 同一篇 §4.4 的字段表（doc:2316）已把“`GET_KINFO` 的 `user_sp`”列为 `USER_STACK_TOP` 的消费点
    之一；§3.2.2.2 的“不是 `KinfoStruct.user_sp`”与同篇 §4.4 直接冲突，说明这是 §3.2 一侧的
    局部残留，而不是全篇口径。
- 最小修复：把该句改为“五个有 Rust userland 读者的语义字段（`nr_procs`/`nr_tasks`/`release`/`version`
  由 IS 读，`user_sp` 由 RS 的 exec 经 GET_KINFO 读）……”；`[ARCH]` 标签的“五个 used + 两个占位”
  保持不动即与正文一致。补一句边界：删除的是 `KernelInfo`（handoff 结构）的 `user_sp` 字段，
  `KinfoStruct`（GET_KINFO 布局位）的 `user_sp` 仍是活字段——两个同名对象在 v10 里分了家。
- 减法替代（V3.6，C-32）：无，因本条不是新增抽象，是删掉一句与源码相反的事实断言。
- 三问（V3.6，C-31）：不适用（非类型级提案）。
- 教学概念账（V3.6，C-35）：不适用（非加法提案）；改正后读者只需记住“两个 user_sp 同名不同物”，
  比现在“字段在但没有读者”少一个悬空问号。
- 置信度：高（代码里有调用链，不是推断）。
- 是否需源码核实：已核实（上述四处文件:行）。
- 若我错了：若“Rust userland 读者”被定义为“直接读 `KinfoStruct` 内存布局的读者”，RS 通过
  `GET_KINFO` 结构拷贝读取算“结构拷贝的读者”，仍应计入；退一步说，即便不计入，正文“不是
  `KinfoStruct.user_sp`”也仍与 RS 的注释和代码相反，且与同段 `[ARCH]` 的“五个 used”冲突。
  两种情况都指向同一处文本，修法不变。

## FINDING DESIGN-02（P2）

- 位置：`os/boot-shim/src/lib.rs:111-127`（`build_kernel_info`）、`:137-167`（`BootStorageCell`）、
  `:169-188`（`report_static`）；`os/kernel-image/src/bootface_common.rs:353-358`（`static STORAGE` +
  `take()`）、`:551-561`（`let _ = storage;` → `report_static` → `build_kernel_info` → `arch_boot`）；
  文档对应主张 §3.2、§3.7、§4.4（doc:1146、1843、2286-2306）。
- 严重度：P2（设计层概念债，无运行时缺陷）。
- 分类：`[DEL]`（删 C 没有的“发送侧记录对象”与它的 `'static` 桥；净减法）。
- 问题：五个生产腿里有两种“发送侧记录”形态，但只有一种语义。三条 v1 腿把 `&BootReport<'_>` 交给
  `enter_kernel_image_v1`（`os/kernel/src/lib.rs:538-546`）——借用，生命周期由调用栈绑定；两条直核腿
  却必须先把同一个借用“抬成 `'static`”，因为 `KernelInfo` 的切片字段是 `&'static [T]`
  （`kernel_info.rs:30`）：`BootStorageCell::report_static()` 用 `&'static self` + `UnsafeCell` 造出
  `BootReport<'static>`，再经 `build_kernel_info()` 逐字段搬进 `KernelInfo`。可是直核腿与内核在
  同一镜像里，`arch_boot` 是 `-> !`，`store_kernel_info()` 入口就把字节深拷进 `.bss`
  （`lib.rs:214-233`、`:1562-1679`）——发送侧的 `'static` 活不到任何需要它的时刻。C 侧只有一个
  `kinfo`，由内核自己拥有（`pre_init.c:26` 的全局、`get_parameters(ebx, &kinfo)`、`pre_init` 返回
  `&kinfo`），不存在“发送侧记录对象”。
- 影响：
  1. 读者要学两套字段完全相同的交接记录（`KernelInfo` 9 个业务字段 vs `BootReport` 同 9 个字段），
     外加一个只做字段搬运的 `build_kernel_info`；
  2. kernel-image `.data` 里常驻一份 `BootStorage`：128×16（memmap）+256×16（reserved）+12×32（modules）
     +4×16（platform）+1024（param）+40×72（sources）≈ 10.5 KiB，内容在进入 `kmain` 前又整份复制进
     `.bss`，同一批字节同时存在两份；
  3. `report_static` 是全仓唯一把借用抬成 `'static` 的安全桥；它依赖“boot 单线程 + fill 已完成”的
     口头契约，而 `take()` 与 `report_static()` 之间没有类型级互斥（`report_static` 不看 `taken`）。
     一个只需活过一次不返回调用的借用，被迫走全仓最需要信任的接口。
- 证据或推理：
  - `os/boot-shim/src/lib.rs:104-110` 的注释自己承认这条路是为了“no `from_raw_parts` laundering”；
    但同一仓库的 v1 发送侧证明了**另一种形态**：`enter_kernel_image_v1(arena, &BootReport<'_>, …)`
    完全不需要 `'static`。
  - `os/kernel/src/lib.rs:214-233`：直核入口收到 `&KernelInfo` 后第一件事是
    `validate_before_tables`/`store_kernel_info`，之后 `arch_boot_impl`；没有任何步骤要求发送侧切片
    活过 `arch_boot` 这次调用。
  - `bootface_common.rs:357-358` 的注释说 “Every `'static` slice handed to `KernelInfo` points here;
    a second entry would overwrite them (UB …)”——这说明 `STORAGE` 的 `'static` 是为 `KernelInfo`
    的类型要求服务，不是因为数据真的要活到程序结束（数据随后被 `.bss` 副本取代）。
  - `boot_storage.rs:13-17` 的模块文档也承认：内核镜像保留 `KernelInfo` 是因为它**深拷**；
    “senders never need that type any more”——既然如此，直核发送侧也不该需要它的 `'static` 形态。
- 最小修复（减法）：把“发送侧交给内核”的入口统一成借用视图，让 `'static` 只在接收侧的 `.bss` 落下：
  - 直核入口改为接受 `&BootReport<'_>`（或 `&KernelInfo<'_>`，见下），接收侧先用报告做门链校验，
    再把字段与字节落进 `.bss`，由落地区生成唯一的 `KernelInfo<'static>` 发布给 `kmain`；
  - 删除 `BootReport`/`build_kernel_info`/`BootStorageCell`/`report_static` 四个公开面，以及
    `bootface_common` 的 10.5 KiB `STORAGE`（数据可直接来自栈上的 `DtbMapOwned`、模块表、
    `[u8; 1024]` 参数缓冲——它们都活到 `-> !` 调用为止）。
  - 两种签名形状的取舍留给实施者：(a) `KernelInfo<'a>` 一个类型 + 生命周期（删 `BootReport`，
    但 `globals::KERNEL_INFO` 要写成 `KernelInfo<'static>`，`BklProtected` 白名单随动）；
    (b) 保持 `KernelInfo` 不变，入口收 `&BootReport<'_>`，校验逻辑改成面向 `BootReport` 的
    一小段（或先造一个借用的临时值）。两条都不改 wire、不改 boot 次序、不改任何外部行为。
  - 迁移成本要说清：`arch_boot` 有三个 cfg 臂 + mock/测试入口；`os/qemu-tests/test-kernels/*`
    直接构造 `KernelInfo` 的固定装置需要改成构造报告或继续走 `arch_boot_impl`。
- 减法替代（V3.6，C-32）：本条本身就是减法（净 −2 类型、−1 函数、−1 方法、−1 静态存储、
  −1 生命周期桥）。
  “不做”这一替代能保住现状，但它不可取：它保留了同一语义的两套类型，以及全仓唯一一处
  “为了类型签名而伪造 `'static`”的桥。
- 三问（V3.6，C-31）：不适用（非防御性类型提案；若选形状 (a)，`KernelInfo<'a>` 是既有类型的
  泛化，不是新值对象）。
- 教学概念账（V3.6，C-35）：这是减法，账目为负——新概念 −3（`BootReport`/`BootStorageCell`/
  `build_kernel_info` 从读者视野消失），讲解增量约 −20 行（§3.7 可删“`'static` 只由
  `report_static` 支撑”整句，§4.4 可删“两条产出入径”的一半），与既有叙事不冲突：
  §3.2 已经讲“三形态都深拷”，本改动只是让类型如实反映这一点。
- 置信度：中高。语义论证确定；成本（入口签名与测试固定装置的迁移面）是主要不确定项。
- 是否需源码核实：已核实发送/接收两侧与本报告引用的行；`os/qemu-tests` 固定装置未逐文件清点
  （只由文档 §4.4 的“builder 只服务直核面”与 `grep build_kernel_info` 确认面）。
- 若我错了：若 `KernelInfo` 的 `'static` 被刻意当作“跨镜像与同镜像统一的接收侧类型”，并且团队
  愿意为“发送侧代码形状一致”付两套类型的代价，那么本条降为 P3 的“记录性取舍”，只保留
  `report_static` 的文档警告。反过来，如果实施成本（测试固定装置）超过收益，可只做一半：
  删 `BootStorageCell`/`report_static`，让 `run_self_boot` 直接用 `static STORAGE` 的
  `report()`（`report_static` 的唯一作用是把 `&self` 变 `&'static self`，而 `STORAGE` 本身就是
  `'static`，`report()` 返回的 `BootReport<'_>` 只要在 `arch_boot` 调用期间有效即可）——
  这样至少消掉一处伪 `'static` 桥，但保留两套字段类型。

## FINDING DESIGN-03（P3）

- 位置：`os/libs/minix-boot/src/memmap_source.rs:240-277`（`SourceClass` 与 `SourceKind::class`）、
  `:489-491`（`SourceRange::physical`）、`:547-552`（`SourceRecord::physical_range`）。
- 严重度：P3。
- 分类：`[DEL]`（删无生产消费者的翻译残留面）。
- 问题：`memmap_source` 里有一组“有声明、无生产消费者”的公开面：
  - `SourceClass`（`InputDeclaration`/`DerivedReservation`）与 `SourceKind::class()`：全仓消费者只有
    `memmap_source.rs` 自己的三个测试断言（`:2041`、`:2052`、`:2383`），`os/libs/minix-boot/src/lib.rs:37-41`
    的再导出清单里也没有它；
  - `SourceRecord::physical_range()`：全仓唯一调用点是 `memmap_source.rs:2572` 的测试；
  - `SourceRange::physical()`：唯一生产调用者是 `physical_range()`，其余是 `:2560-2564` 的测试。
  覆盖审计真正使用的“reservation 类记录”判定是另一段手写匹配
  （`audit_source_coverage_for` 的 `is_reservation_record`，`:404-425`），它与 `SourceClass`
  不是同一根轴（`class()` 把所有非 `FirmwarePrefix` 都算 `InputDeclaration`，而审计只认
  `FirmwarePrefix`/`ReservedOrdinary`/`ReservedReusable` 三类）。于是代码里有两条“来源记录分类”
  规则：一条被审计使用，一条只被测试断言——后者看起来像权威，实际不参与任何判决。
- 影响：读者要花时间判断 `SourceClass` 是否是审计的同义词；两条分类规则将来会漂移（新增
  `SourceKind` 变体时，`class()` 的默认分支会静默把它归为 `InputDeclaration`，审计则可能不认它）。
  这是“死抽象 + 语义重复”的复合体，不是纯风格问题。
- 证据或推理：
  - `grep -rn "SourceClass" os/ --include=*.rs` 只命中定义、`class()` 与三个测试断言；
    `grep -rn "physical_range()" os/ --include=*.rs` 只命中定义与一个测试；
    `grep -rn "\.physical(" os/ --include=*.rs` 的 `os/libs` 命中只落在 `SourceRange::physical` 自身
    与测试（`os/drivers/...` 的 `backdoor.physical(...)` 是另一个 API，无关）。
  - `memmap_source.rs:404-425` 的 `is_reservation_record` 是审计真正使用的规则；它的注释说明
    “Only physical records can explain a physical projection”，但没有引用 `SourceClass`。
- 最小修复：删 `SourceClass`、`SourceKind::class()`、`SourceRecord::physical_range()`、
  `SourceRange::physical()` 及依赖它们的测试断言（测试可改成直接断言 `record.range` 与
  `record.address_domain`，信息量不丢）；若团队希望保留“声明 vs 派生”这根轴，则反过来把审计的
  `is_reservation_record` 改成由 `class()` + 一个 kind 子集表达，让枚举变成唯一权威——两种修法
  选一，不要维持两条规则。
- 减法替代（V3.6，C-32）：无，因本条本身就是减法；替代方案是“不做”（保留死面），代价是读者与
  漂移风险。
- 三问（V3.6，C-31）：不适用（非防御性类型提案）。
- 教学概念账（V3.6，C-35）：减法，新概念 −2（`SourceClass` 枚举与它的转换函数），讲解增量约 −5 行；
  与既有叙事一致（§3.3 从未提 `SourceClass`，只讲 kind/origin/range/input_object/attribute）。
- 置信度：高（grep 可复算；分类轴不等价是读代码可判的）。
- 是否需源码核实：已核实（定义、全部调用点、审计判定）。
- 若我错了：若 `SourceClass` 是给仓外消费者/未来第四种输入对象预留的扩展点，那么它应带一条
  文档或测试说明“当前无消费者、保留到 X 落地”，否则按死代码处理；本仓是 no_std 工作区、
  无外部消费者，按现状报死面成立。

## 非法状态（类型上可表示、语义上不可能）

> 这是任务项 4 的独立清单。每项都给出“今天由什么兜住”与“是否值得上类型”；结论是**没有一项**
> 值得为此新增类型，理由随条给出。

| # | 可表示的状态组合 | 为什么语义上不可能 | 今天由什么兜住 | 是否值得类型化 |
|---|---|---|---|---|
| S1 | `BootStorage` 五族中部分已提交（如 `map_committed=false`、其余为真），`report()` 照常返回 | 一份只填了一半的启动报告不描述任何真实启动 | 下游门：`validate_before_tables` 的 `memmap` 非空、Standard 的 `boot_modules` 非空；空/半报告会在有名字的断言处停机 | 否：加 `assert!(五族全真)` 即可（见 SUBTRACTION-TABLE 第 2 行），新增令牌类型不增加可判约束 |
| S2 | `PlatformDescSource { len: 0 }` 既表示“生产者未记录长度”，也表示“生产者想给一个零长 blob” | 零长 blob 没有窗口意义；“未知”与“已知零”是两个状态 | 消费者 `has_len()` 分道 → `push_2m`（`kernel/src/lib.rs:1216-1226`）；解析器随后会因魔数/头部失败而报错 | 见 DESIGN-04：`Option<NonZeroU32>` 让“显式零长”不可构造，16 B 不变；这是表示修正，不是防御层 |
| S3 | `RamAuthority::DTB` 配一组只有 UEFI 记录的 `source_records`（或反之） | 权威声明与记录来源应同源 | `validate_requiring_sources` 的“declared RAM authority … has no source records”与 `audit_source_coverage_for(authority)` 两处 fail-closed（`kernel_info.rs:459-479`） | 否：文档 §3.2.1 已承认“构造点仍可传错”；把 authority 与记录绑成一个值的唯一办法是让构造器自己取记录，那会把“谁声明了内存图”这一外部事实藏进库内部 |
| S4 | `KernelInfo.memmap` 与 `reserved_regions` 相交，或单表乱序/重叠 | 同一个物理字节不能既是可分配又是保留 | `validate_impl_inner` 的升序/不相交/跨表 merge-walk 断言（`kernel_info.rs:288-340`） | 否：这是对不可信 wire 的边界校验，`assert!` 是正确形态（brief 的 C-31 边界条款） |
| S5 | `BootStorageCell::take()` 之后仍可调 `report_static()`，得到与存活 `&mut` 并存的 `&'static` 视图 | 一次性所有权应当只有一个视图 | boot 契约（单线程、fill 完成后不再改）；生产调用点先 `let _ = storage;` 结束借用（`bootface_common.rs:551`） | 否（见 DESIGN-02）：正确的减法是把这条桥整体删掉，而不是给它加令牌 |
| S6 | `BootAlloc` 的 `base/next/end` 三个 `AtomicU64` 可被两个 CPU 同时读改写，得到重复页 | 分配器语义要求每个页只发一次 | 无类型/运行时互斥；源码注释明写“不线性化、只替代 `static mut`”（`boot_alloc.rs:14-22`） | 否：boot 阶段单线程是全局前提（BKL 之前）；把它类型化会引入一个所有调用点都要携带的“单线程令牌”，收益为负 |
| S7 | `PreparedJump { entry, arena_pa }` 交换、`BootWindows` 未检查就建根 | 交换后跳转目标变成物理 arena，首次取指故障；未检查窗口可能越过物理上限 | `PreparedJump` 用命名字段消灭了位置交换（`kernel/src/lib.rs:408-413`）；`CheckedWindows` 令牌让“先检查后建根”成为唯一顺序（`:1292-1310`） | 已是类型化的正面样本，保留 |

## FINDING DESIGN-04（P3）

- 位置：`os/libs/minix-boot/src/platform.rs:110-183`（`PlatformDescSource`、`new`/`with_len`/
  `with_optional_len`/`has_len`/`len`）；消费者 `os/kernel/src/lib.rs:1210-1227`；wire 形态
  `os/libs/minix-boot/src/handoff_v1.rs:62-68`；文档主张 §3.2.2.3 与 §3.2.3（doc:1260、1321）。
- 严重度：P3。
- 分类：`[DEL]`（删 Rust 侧哨兵表示；wire 字段与 16 B 形状不变）。
- 问题：领域类型用 `len: u32` 的哨兵 0 表示“生产者没有记录长度”，再用 `has_len()/len()` 两个访问器
  把它重新展开。`with_len(kind, addr, 0)` 与 `new(kind, addr)` 产生同一个值，`len == 0` 到底是
  “未知”还是“显式零长”在类型上不可区分；消费点必须记住哨兵才能选窗口（2 MiB 回退 vs 精确长度）。
  wire 侧继续写 0 是正确的（`WirePlatformDescSource.len` 是协议字段），问题只在进程内的语义类型。
- 影响：新增一种平台源的作者若“算出了长度但得到 0”或“想把长度留空”，两种意图会汇成同一条
  2 MiB 回退路径；`DTB_MAX_LEN` 允许 64 MiB 的 blob，而回退只覆盖 2 MiB——长度信息丢失的后果
  不是立刻报错，而是映射窗口与 blob 实际范围不一致（解析读到未映射页才 fault，现场离“长度没记”
  很远）。当前没有可复现故障（生产侧 `with_optional_len` 会传真长度，`None` 只在头部不可读时出现），
  所以是表达债而非缺陷。
- 证据或推理：
  - `platform.rs:132-142`：`new()` 置 `len: 0`，文档注释写 “0 = unknown/legacy producer”。
  - `platform.rs:145-183`：`with_len` 无 `len > 0` 校验；`with_optional_len(Some(0))` 也走
    `with_len(…, 0)`，与 `None` 同像；`has_len()` 就是 `self.len != 0`。
  - `os/kernel/src/lib.rs:1215-1227`：`if source.has_len() { 精确窗口 } else { push_2m(start) }`。
  - 文档 doc:1260 与 doc:1321 都把“`len = 0` 表示未记录”写成了契约的一部分——说明这是一个
    有意的哨兵，不是偶然。
- 最小修复：进程内类型改为 `len: Option<core::num::NonZeroU32>`：
  - `new()`/`with_optional_len(None)` → `None`；`with_len(kind, addr, n)` 收 `NonZeroU32`
    （或保留 `u32` 但在 `debug_assert!(n != 0)` 后 `NonZeroU32::new(n).expect(...)`）；
  - 消费者把 `has_len()/len()` 两访问器换成一个 `match`；
  - wire 编码 `None → 0`、`Some(n) → n.get()`，解码 `0 → None`（`handoff_v1_write/read` 各一处）；
  - 16 B 形状不变：`{u32, Option<NonZeroU32>, u64}` = 4+4+8，`align = 8`；文档 §3.2.3 的 16 B 声明
    无需改。
- 减法替代（V3.6，C-32）：看似可以“只加 `assert!(len > 0)` 到 `with_len`”，但减法拦不住——`new()`
  合法地把 0 存进去表示“未知”，所以 0 在类型里永远有两种解释；断言只能拦住“显式调用 with_len(0)”，
  拦不住“未知被当成 0 长度”。`Option` 把两条路径在类型上分开，是唯一能让 `has_len()` 消失的写法。
- 三问（V3.6，C-31；本条是值对象表示替换，按防御性提案口径回答）：
  1. **威胁模型**：生产者是可信链上的 boot-shim，不是外部攻击者；风险是“长度信息在传递中静默降级”
     这一类生产侧失误，不是伪造输入。伪造输入仍由 `validate_impl_inner` 的空址/kind/溢出断言 +
     解析器魔数门挡。
  2. **borrow checker 是否已拦住**：不涉及借用关系，拦不住。
  3. **最坏后果与理解成本计数**：最坏后果是 2 MiB 回退窗口与真实 blob 范围不一致（解析期 fault，
     归因远）；理解成本 +1 个 std 词汇（`NonZero` 空值优化）、−1 个访问器；可判边界：字段是
     进程内可信值，不需要 `Result`/完整校验，`Option` 构造点即边界。
- 教学概念账（V3.6，C-35）：新概念 +0（`Option`/`NonZero` 是 std 词汇，读者不必学本仓专名；
  `has_len` 消失抵消）；讲解增量约 2 行（“未知用 `None`，wire 用 0”）；与既有叙事不冲突——
  §3.2.2.3 已经用一整段解释 `len = 0`，改后这段可以更短。
- 置信度：高（形状与调用点可直接核对；16 B 布局结论可由 Rust 布局规则复核，实施时应加一条
  `const _: () = assert!(size_of::<PlatformDescSource>() == 16);` 钉住）。
- 是否需源码核实：已核实（定义、三个构造器、consumer、wire 类型）；`handoff_v1_write/read` 的
  platform 编解码各一处未逐行引用，实施前需定位。
- 若我错了：若团队认为“wire 字段是 u32，领域类型就该同形”以避免一次转换，那么至少应给
  `with_len` 加 `assert!(len != 0)` 并在文档里写清“0 只许由 `new()` 产生”；这样本条降为
  “文档 + 一条断言”的 P3，不必改类型。

## FINDING DESIGN-05（P3）

- 位置：三处源码文档注释仍在描述 v9 及更早的世界，与文档 §4.6/§4.4 的 v10 主张相反：
  1. `os/kernel/src/lib.rs:651-653`（`arch_boot_resume_v1` 的 arena 生命周期合同）：
     “the arena is still named by a startup-object record (identity, not liveness) and stays in the
     boot handoff's deduction inputs”——v10 已删除 startup 对象分区与它唯一的读者（文档 doc:2545
     明确说“v10 之后内核不再保留 startup-object 记录，也不再有它指向的 arena 身份”）。
     同一函数体上方 `:641-656` 的文档块与 `store_kernel_info` 的注释（`:1664-1665`）互相矛盾。
  2. `os/libs/minix-boot/src/kernel_info.rs:28`：`memmap` 字段注释指向 “the `memmap()` accessor
     for the per-consumer split”，但该文件没有 `memmap()` 访问器（只有字段本身与 `kern_stack_top()`）。
  3. `os/boot-shim/src/lib.rs:88-90`：`build_kernel_info` 的注释说 “Every boot leg calls this
     function; no leg carries its own parameter list”，而 v1 接收侧在
     `os/kernel/src/lib.rs:735-748` 直接构造 `KernelInfo`，根本不经过它；文档 §4.4 也只说它服务
     两个直核面。
- 严重度：P3（源码叙述债；不影响行为，但会误导读者与后续设计判断）。
- 分类：`[DEL]`（删源码里的 v9 叙述残留）。
- 问题：这是“代码里按 v9 写的说明”与“文档按 v10 写的主张”之间的漂移。第三处在 DESIGN-02 的
  方向下会自然消失（`build_kernel_info` 被删）；前两处必须单独改。
- 影响：读者若相信 (1)，会认为 arena 有身份记录可依赖，从而在“跳转后再读 arena”时以为有扣除兜底；
  若相信 (2)，会去找一个不存在的访问器；若相信 (3)，会为了让五个腿都调用它而新增一条无意义的
  构造路径。三处都属于“文档与实际互相打脸”的注释，正是出版标准禁止的残留。
- 证据或推理：上述行号逐条可读；`handoff_v1.rs:112` 与 `:121` 的 “retired with the v10
  startup-object set” 是分区已删的证据；`grep -n "fn memmap(" os/libs/minix-boot/src/kernel_info.rs`
  零命中；`build_kernel_info` 的调用点只有 `bootface_common.rs:558` 与三个测试
  （`opensbi_helpers.rs:765/824`、`uefi_helpers.rs:1438`），v1 接收不调用。
- 最小修复：三处注释各改一段：arena 生命周期删掉 startup-record 子句（保留
  “LastRead(arena) ≺ Publish(VMFreePages)”）；`memmap` 注释改成“消费者按腿自行扣除，见
  `vm_handoff::classify` 与 `vm_boot_exclusions`”；`build_kernel_info` 注释改成“只服务两个直核面；
  v1 三条腿由接收侧解码（或按 DESIGN-02 删除该函数）”。
- 减法替代（V3.6，C-32）：无，因本条是删错误叙述；第三处在减法（删函数）后自动消失。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用（非加法）；删除后读者少记一个“startup record”对象。
- 置信度：高。
- 是否需源码核实：已核实（三处行号 + 各自的证据行）；注意审查期代码冻结，本报告不实施。
- 若我错了：若 startup-object 记录被计划以另一种形式恢复（当前没有任何冻结决策支持这一点），
  那么 (1) 应改成“future”而不是删除；但 D-062 的裁决是“全部删除、一次性升 wire 版本”，
  现状源码也确实没有，故按残留处理。

## FINDING DESIGN-06（P3）

- 位置：文档四处（doc:1280、1335、1929、1254），逐项对应下面的 a–d。
- 严重度：P3（不影响设计判断，影响可核性与读者信任）。
- 分类：`[MODERN]`（文档精度校正，无机制变更）。
- 问题：四处“精度级”事实断言，结论都成立、锚点或枚举不完整：
  a. **§3.2.3 的 `repr(C)` 清单不完整**（doc:1280）：文档列举 `KernelInfo`/`MemoryRegion`/`BootModule`
     不用 `repr(C)`，但同一节后文（doc:1321）文档化的 `PlatformDescSource` 是 `#[repr(C)]`
     （`os/libs/minix-boot/src/platform.rs:120`），且它的 16 B 形状正是该属性保证的。读者会误以为
     “四类交接结构全都不用 `repr(C)`”。
  b. **§3.3 “C 的 `memmap` 只有 `base`/`len`”**（doc:1335）：C 的 `multiboot_memory_map_t` 还有
     `mm_type`（`minix3/sys/arch/i386/include/multiboot.h:216`；`add_memmap` 每次都写
     `MULTIBOOT_MEMORY_AVAILABLE`，`pg_utils.c:86-117`），`mem_high_phys` 也是从这些条目派生的。
     结论“C 没有来源记录”不受影响，但“只有 base/len”作为事实陈述不成立。
  c. **§4.1 的常量位置**（doc:1929）：`PT_BUMP_PAGES = 256` 定义在
     `os/kernel-image/src/bootface_common.rs:35`，不在 `bootface.rs`；§3.2.1（doc:1211）也写
     “`bootface` 的 `PT_BUMP_PAGES`”。读者按文档去 `bootface.rs` 找不到。
  d. **§3.2.2.3 “每架构只实现一个 `write_byte`”**（doc:1254）：`EarlyConsole` 还有一个带默认实现的
     `init()`（`os/plat/src/early_console.rs:62-68`），x86-64 实现覆盖了它（`ser_init`，
     `os/plat/src/x86_64/early_console.rs:113-117`）。准确说法是“每架构至少实现 `write_byte`，
     `init` 可选”。
- 影响：a 会让“进程内结构不做 ABI”这条边界看起来比实际更宽；b 削弱“来源记录是新结构”的对照
  力量（C 的 `mm_type` 也是一种（弱）来源信息）；c 浪费读者定位时间；d 让读者以为只需实现一个方法，
  接新架构时会漏掉 x86 那类需要初始化的控制台。
- 证据或推理：上述行号逐条可读；`repr(C)` 与 16 B 的因果可由 `platform.rs:120-139` 直接核对；
  `PT_BUMP_PAGES` 的唯一定义点用 `grep -rn "PT_BUMP_PAGES" os/` 可复算；`init` 的默认实现与
  x86 覆盖可直接读。
- 最小修复：四处各改一句：a 补“`PlatformDescSource` 例外：`repr(C)` 只为钉住 16 B 句柄形状，
  字段顺序仍不是跨镜像 ABI（wire 用 `WirePlatformDescSource`）”；b 改成“C 的条目只有基址、长度与
  一个固定为 AVAILABLE 的类型标签，没有‘谁声明了这段’的来源记录”；c 改成
  `bootface_common` 的 `PT_BUMP_PAGES`；d 改成“每架构实现 `write_byte`（`init` 可选，x86-64 用它
  初始化 COM1）”。
- 减法替代（V3.6，C-32）：无，因本条是删/改错误或不完整断言。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用（非加法）；a 的补句会让读者多知道一个例外，但省掉“为什么
  16 B”这一悬空问题，净持平。
- 置信度：高。
- 是否需源码核实：已核实（四处）。
- 若我错了：b 若按“memmap 数组元素在本项目关心的语义上只有 base/len（类型恒为 AVAILABLE）”
  的窄读法，可判为表述风格而非错误；此时可只修 a/c/d，b 降为可选。a/c/d 无歧义。

## ARCH-PROPOSAL

本轮无 `[ARCH]` 提案。三条候选（DESIGN-02 的入口签名统一、DESIGN-04 的 `Option<NonZeroU32>`、
DESIGN-03 的死面删除）都不改变 boot 是什么：没有新增责任、字段、检查或策略，不触碰 wire 版本、
syscall ABI、syscall 数量或任何外部可观察行为；它们只是让既有语义被更少的类型与更正确的
注释表达。按 D-047，`[ARCH]` 应写进本节并交用户裁决——本轮没有需要用户裁决的设计扩张。

## 翻译残留分类（任务项 2）

> 判据：这个结构的存在理由是否只剩“C 当年这样写”。逐类给结论；没有命中的类别也写明，避免读者以为
> 漏查。

| 类别 | 本仓命中 | 判定 |
|---|---|---|
| 单实现依赖注入 trait | `FileLoader` 是 3 实现 + 1 处 dyn（trait-audit），是真接缝；`BootShim` 已删（全仓零命中）；`ProcInfoBuild`/`PrivInfoBuild` 虽是单实现，但它们给“外部类型 + crate 私有参数类型”挂构造器，是可见性需要，不是注入 | 无残留。`FileLoader`/`BootArch`/`HigherHalf`/`Paging`/`HugePages`/`EarlyConsole`/`PlatformDesc` 都是多实现，保留 |
| 无状态 Manager | boot 路径没有 `*Manager`；`KernelConsole`（`kmess.rs`）是有状态环形缓冲，且是 C `kstruct kmessages` 的语义等价物；`BootStorage` 是有状态的定长所有者，不是无状态包装 | 无残留 |
| 布尔 / 字符串状态机 | `BootStorage` 的 5 个 `*_committed` 是“按族一次性提交”的状态位（不构成非法值，见 S1）；`ValidationLevel` 已把旧的“两个 bool 四组合”收成三态枚举（`kernel_info.rs:154-177`，正确样本）；`BootStorageCell.taken` 是纯粹的“一次性所有权”bool；`BootAlloc` 用 `end == 0` 表示未初始化 | 唯一该删的是 `BootStorageCell.taken`（随 DESIGN-02 整类消失）；`BootAlloc` 的 `end == 0` 由 `init/resume` 的一致性断言排除非法组合，保留；`BootStorage` 的五个 bool 若只防“同族二次 fill”，一条位图或 `assert!` 都等价，但现状不构成缺陷，不单列 |
| 哨兵返回值 | `PlatformDescSource.len = 0`（DESIGN-04）是唯一的领域哨兵；`ZERO_REGION`/`ZERO_MODULE`/`ZERO_SOURCE`/`EMPTY_SOURCE_RECORD` 是“定长数组未用槽”的零填充，只经已提交前缀暴露（`boot_storage.rs:40-55`、`:226-253`），与 C 的零初始化数组同形，合理；`elf_file_end(data).unwrap_or(data.len())` 是“非 ELF 回退整段”的显式规则，注释写明，不是隐式哨兵 | 只报 DESIGN-04 |
| clone 到能编译 | `KernelInfo: Clone+Copy`（POD）、`BootReport: Clone+Copy`（借用视图）都是值语义；`store_kernel_info` 里的 `let mut k = *kernel_info`（`kernel/src/lib.rs:1669-1676`）是 Copy 后重指切片到 `.bss`，是落地步骤不是绕过借用检查；`prepare_and_enter_kernel_image` 前后构造两次 `report`（`uefi_helpers.rs:1135`、`:1240`）是因为信封在 EBS 前定稿、EBS 后要用同一 storage 做复核，不是 clone 凑类型 | 无残留 |
| 对象图共享 | boot 路径没有 `Arc`/`Rc`；全局 `KERNEL_INFO` 与六个 store 是 C 全局 `kinfo`/静态数组的对应物；`BootStorageCell` 是唯一的“共享静态 + 内部可变”，它由 DESIGN-02 删除；`ebs_pair::PAIR_STORAGE`（`ebs_pair.rs:41`）是跨重试保留的解析暂存区，单线程独占，必要 | 只报 DESIGN-02 |

## trait-audit 读数核对（任务项 3）

> brief 要求“用 trait-audit 的读数作事实，但要自己核实，不要机械判决”。下表只列本章相关的 trait；
> 读数取自 `tmp/review-loop/01-stage/evidence/trait-audit.json`，核对列是我自己 grep/读源的结果。

| trait | 审计读数（impls/refs/dyn） | 我的核对 | 判决 |
|---|---|---|---|
| `FileLoader` | 3 / 16 / 1 | `UefiFileLoader`（`uefi_helpers.rs`）、`UbootFileLoader`（`opensbi_helpers.rs`）、`MockLoader`（`loader.rs` tests）三实现；`grep "dyn FileLoader"` 零命中，审计的 `dyn=1` 未被源码复现 | 活的测试/固件接缝，保留（这正是 `BootShim` 删除后剩下的那个接缝） |
| `Paging` / `HugePages` | 5 / 242；5 / 31 | 三架构实现 + mock + test 实现；`map_kernel` 等泛型消费 | 活，保留 |
| `BootArch` / `HigherHalf` | 4 / 58；4 / 31 | 三架构 + mock；`arch_boot_finish`/`resume_additive` 以类型参数消费 | 活，保留 |
| `EarlyConsole` | 5 / 132 | 三架构 + `KernelConsole`（装饰器）+ `MockEarlyConsole` | 活，保留 |
| `PlatformDesc` / `InterruptControllerDesc` / `TimerDesc` | 7 / 27 / 3；3 / 35 / 7；4 / 30 / 7 | `dyn PlatformDesc` 在 `minix-platform/src/global.rs:283`；后两者经 `&dyn` 在 device_tree/acpi/aarch64 返回 | 活，保留 |
| `BklProtected` | 0 / 31 / 0 | **假阴性**：实现由 `bkl_protected_impls!` 宏生成（`globals.rs:132-207`），审计不展开宏；`Sealed` 的 `1` 同理（宏内的 `impl Sealed for $ty`） | 不是死抽象；但审计对宏生成的 impl 计数不可用，需人工看 |
| `PrivInfoBuild` / `ProcInfoBuild` | 1 / 1；1 / 2 | 单实现，但用途是给外部类型（`ProcInfoStruct`/`PrivInfoStruct` 定义在 `minix-types`）挂一个需要 crate 私有参数类型（`KProcess`/`KPriv`）的构造器；固有 impl 无法在 kernel crate 里对外部类型写 | 不是注入残留，保留（单实现 trait 不等于坏） |

> 结论：本章范围内**没有**“审计判死而实际已死”的 trait；唯一的死面是枚举/访问器（DESIGN-03），
> trait-audit 不覆盖这类，需要另用 grep。

## VERDICT

### 值得改（按优先级）

1. **DESIGN-01（P1，文档事实）**：`KinfoStruct.user_sp` 有活的 Rust userland 读者（RS exec 经
   GET_KINFO 读 offset 8），文档“四个读者/不是 `KinfoStruct.user_sp`”与源码相反，且与同段
   `[ARCH]` 的“五个 used”自相矛盾。**接受，必须改**；代价：一段话，零代码。分类 `[MODERN]`
   （文档校正，无机制变更）。这是本轮唯一可能引发错误删除的误导，优先级最高。
2. **DESIGN-02（P2，减法）**：直核腿的 `'static` 发送侧（`BootReport` + `BootStorageCell` +
   `report_static` + `build_kernel_info` + ≈10.5 KiB `STORAGE`）只为满足 `KernelInfo` 的
   `&'static` 字段；同镜像调用只需活过一次 `-> !`。**接受为设计结论**；代价：`arch_boot` 三个
   cfg 臂 + 测试固定装置的签名迁移（中），不改 wire/次序/外部行为。建议实施时选“入口收借用视图，
   `.bss` 里生成唯一 `'static`”的形状，而不是给 `KernelInfo` 加生命周期（后者涟漪更大）。
   分类 `[DEL]`。
3. **DESIGN-03（P3，死面）**：`SourceClass`/`SourceKind::class`/`SourceRecord::physical_range`/
   `SourceRange::physical` 无生产消费者，且与审计的 reservation 判定构成两条分类规则。**接受**；
   代价：删 4 个公开项 + 改 3 个测试断言（小）。分类 `[DEL]`。
4. **DESIGN-04（P3，表示）**：`PlatformDescSource.len = 0` 哨兵。**接受**（净概念 0，16 B 不变）；
   代价：两个构造器、两个访问器、wire 编解码各一处、文档 1 段（小到中）。分类 `[DEL]`。
   若团队坚持 wire 同形，退一步只加 `assert!(len != 0)` + 文档，其余不变。
5. **DESIGN-05（P3，源码叙述）**：三处源码注释仍按 v9 写（startup record、不存在的 `memmap()`
   访问器、“Every boot leg calls build_kernel_info”）。**接受**；代价：三段注释（小）。
   分类 `[DEL]`。第三处在 DESIGN-02 落地后自然消失。
6. **DESIGN-06（P3，文档精度）**：`repr(C)` 清单、C memmap 字段、`PT_BUMP_PAGES` 位置、
   `EarlyConsole` 方法数四处精度问题。**接受**；代价：四句话（小）。分类 `[MODERN]`
   （文档校正）。b 项若按窄读法可判风格，a/c/d 无歧义。

### 合理取舍（不改，记录理由）

- **`KernelInfo.memmap` 不预先扣除**（保留 C 的“引导器原始视图”）：源码头注释与文档 §3.2 都说清了
  “不保证已扣除、每个消费者自己扣”；扣除成员表只有一份（`for_each_live_bootstrap`），
  `vm_boot_exclusions` 与 `classify` 共用它，没有第二份手写清单。把这层从类型里拿掉（例如只暴露
  已扣除视图）需要 C 的 `add/cut` 账本回归或每阶段一个视图类型，收益不抵成本。**接受现状**。
- **`RamAuthority` newtype**：值域与 `InputObjectId` 同构，但它阻止“把某条记录的运行期
  `input_object` 当作权威传入”这类错配，且无运行时成本。**保留**。
- **`pt_alloc` 的两个函数指针槽**（alloc + free）：语义上就是“分配器域”的一对操作，VM 侧成对注册、
  内核侧只注册 alloc；文档的“一个槽”指的是 Paging 侧看到的单一调用点。**保留**（DESIGN-06 只改措辞）。
- **boot 期两种停机通道**（内核 `panic!` vs 直核面 `say+halt`）：前者在 Rust 已接管后，
  后者在裸串口/无 panic handler 的极早阶段；两者都 fail-closed，合并会引入一个新的统一出口层。**保留**。
- **`BootStorage` 的五个提交 bool**：per-family 一次性提交是真约束；跨族完整性由下游门兜住（S1）。
  上类型（`ReadyReport`）不增加可判约束。**保留现状**（可选加一条 `report()` 断言，见减法表第 2 行）。
- **boot 期原子量**：`BootAlloc`/`BUMP_NEXT`/`PT_ALLOC`/`BootStorageCell.taken` 的原子性是
  “Rust 2024 不许 `static mut` 引用”的替代，不是并发保证；源码注释已写明。**保留**，读者按注释读。
- **直核面 `freepde_start`/`vir_kern_start` 两个 C 布局位**：恒 0 / 有值无仓内读者，但它们是
  `KinfoStruct` 48 B 布局的组成部分（`kinfo.rs:25-46` 的布局测试钉住），删掉会改 GET_KINFO 的
  ABI。**保留**；注意 `user_sp` 不属于这一类（DESIGN-01）。
- **两个 `KernelLayout`**：`os/kernel/src/boot_policy.rs:22`（镜像跨度：virtual/physical/size）与
  `os/libs/minix-types/src/types/boot.rs:443`（映射布局：text/data/dm 页数）是两个不同语义对象，
  同名不同物。不建议合并（合并会造出一个“部分字段只对一半消费者有意义”的类型）；若要降认知成本，
  只做重命名（例如前者叫 `ImageSpan`），那是纯 Refactor、与 boot 语义无关，本轮不报 FINDING。

### 需要用户裁决

**无。** 本轮没有 `[ARCH]` 提案（见 `## ARCH-PROPOSAL`）：所有建议要么删对象/删断言，要么只改
进程内表示与注释；没有新增责任、检查、字段或策略，不改 wire 版本、syscall ABI 或外部可观察行为。

### 被拒的提案（我考虑过但不报，避免下一轮重复提出）

- 给 `BootStorage` 加 `ReadyReport`/`finish()` 令牌：与一条断言保障相同，多两个概念（减法表第 2 行）。
- 给 `RamAuthority` 再加一层 snapshot/record 值对象：`snapshot_id` 已在记录里，新增层不增加可判约束
  （减法表第 3 行）。
- 给 `BootAlloc` 加“单线程令牌”参数：把全局前提变成每个调用点的负担，收益为负（S6）。
- 统一四种内存区间表示（`MemoryRegion`/`PhysRange`/`PhysicalRange`/`SourceRange`）：D-110 已裁决
  分属不同语义边界，不统一，不启动 wire v11、不改 syscall ABI——本报告遵守该冻结决策。
- 把 `SourceRecord` 缩减为 5 字段：D-111 已裁决保留 72 B 与 wire v11，取证字段不以正常路径读取
  次数为删除依据——不报。
- 把 `collect_param_buf_into` 的 `BOARD` 生成接上真实来源：当前生产显式 `None` 是“不伪造”的正确
  形态（文档十条分歧之三），不是欠账。
- 把 `KernelInfo::validate` 的四道门合并成一道：文档已声明“同名项重复挡是故意冗余”，且接收侧
  对发送侧的不信任需要在落地区之后再看一次；合并会把信任边界论证一起删掉。

### 一句话总结

文档 Ch3/Ch4 与源码的主体是一致的：10 字段 `KernelInfo`、三形态深拷、v10 的删除、arena 布局、
五步分类、`FileLoader` 接缝、依赖方向、容量表、门链次序，逐项都能在源码里对上。真正的欠账只有
一处会误导删除的 P1 事实错误（`user_sp` 的读者），一处值得做的减法（直核发送侧的伪 `'static`），
以及一组 P3 的死面、哨兵与叙述漂移；没有需要用户裁决的架构扩张。

