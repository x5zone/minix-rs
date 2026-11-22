DOC 01-boot-shim-bootstrap.md

VARIANCE: D-024 D1 停跑，D2 采用文档主张↔源码事实回退

## C-CONTRACT（先独立重建，供 DIFF 对照）

重建方法：只读 `minix3/` 原 C 源码（`pre_init.c`、`pg_utils.c`、`param.h`、`kernel.lds`、`table.c`、`do_getinfo.c`），不从文档转述。合同要点：

- **单一地址空间，无交接对象**。`kinfo_t kinfo;` 是内核全局（`minix3/minix/kernel/arch/i386/pre_init.c:20`）；`get_parameters(ebx, &kinfo)` 原地填它（`:94`）；`pre_init` 返回 `&kinfo`（`:217` 起）；`kmain` 在同一地址空间继续用这个指针。C 没有"发送侧/接收侧"，也没有序列化或落地区。
- **`kinfo_t` 的字段是三类东西的混装**（`minix3/minix/include/minix/param.h:14-47`）：引导器载荷（`mbi`、`module_list[]`、`memmap[40]`、`mmap_size`、`mem_high_phys`）、内核自有的记账/状态（`freepde_start`、`kmessages`、`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、三个 `*_allocated_bytes`）、经链接脚本得来的布局事实（`vir_kern_start`、`bootstrap_start/len`、`user_sp`、`user_end`）。
- **`get_parameters` 的顺序语义**：复制 multiboot 结构；cmdline 按 `name=value` 逐条 `mb_set_param`（重复键只在"首条匹配"处删除，删除循环会连带移动后续记录）；`user_sp = USR_STACKTOP`、`user_end = USR_DATATOP`（`pre_init.c:156-157`）；`kernel_allocated_bytes = _kern_size - bootstrap_len`（`:160-163`）；`bootstrap_start` 页对齐断言、`bootstrap_len` 向下取整（`:165-166`）；模块表整段 `memcpy`（`:170`）；multiboot mmap 只收 `AVAILABLE` 项，经 `add_memmap` 记账（`:176-189`）；最后把内核伪装成第 M+1 个模块，逐模块做 `overlaps` 并 `cut_memmap`（`:194-214`）。
- **`add_memmap` / `cut_memmap` 的算术合同**（`minix3/minix/kernel/arch/i386/pg_utils.c:86-125`、`:32-70`）：`add_memmap` 在 4 GiB-4 KiB（`LIMIT 0xFFFFF000`）处截断、首尾页对齐、容量 40 满则 panic、顺带维护 `mem_high_phys` 高水位；`cut_memmap` 把切除区间向外扩到页边界，命中即把原条目清零并按左右残余重新 `add_memmap`。两函数都不维持有序性，槽位"先空先用"。
- **页表页与数据页两套池**：`alloc_pagetable` 是 6 页 `static` 池，发完 panic、永不归还（`pg_utils.c:123-136`）；`pg_alloc_page` 从 memmap **最高条目尾部**切 4 KiB（`:138-160`），不是页表页。
- **链接布局给出布局事实**：`_kern_phys_base = 0x00400000`、`_kern_vir_base = 0xF0400000`；`unpaged_*` 段 VMA=LMA，执行地址等于物理地址；`_kern_size` 在 `.bss` 末尾由位置计数器算出（`minix3/minix/kernel/arch/i386/kernel.lds:3-6`、`:12-34`）。`bootstrap_start/len` 就是 `[__k_unpaged__kern_unpaged_start, __k_unpaged__kern_unpaged_end)` 的长度。
- **启动映像表是另一张表**：`image[NR_BOOT_PROCS]` 前 5 项是内核任务，后 12 项是用户态服务器，顺序 `ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`（`minix3/minix/kernel/table.c:44-64`）；`NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1`（`param.h:6`）。
- **外部面**：`GET_KINFO` 直接 `sizeof(struct kinfo)` 整结构拷贝（`minix3/minix/kernel/system/do_getinfo.c:66-69`）；`GET_IMAGE` 给 `image[]`（`:86-89`）；`GET_MONPARAMS` 给 `param_buf`（`:143-146`）。C 没有 `GET_KMESSAGES`，也没有任何 `VmBootHandoff`。
- **并发与错误**：启动单 CPU；错误只有 `assert`/`panic`，没有 `Result`。

对照结论：文档 Ch3/Ch4 的绝大多数 C 对照（22 个字段去向、`cut_memmap` 页外扩、`alloc_pagetable` 6 页池、`_kern_size` 的链接符号来源、`boot_procs[]` 与 `MODULE_NAMES` 的对应）在 C 源码里成立。下文只列与**现有 Rust 源码事实**不一致或需要裁决的项。

## DIFF-TABLE

| 维度 | 文档 Ch3/Ch4 主张 | 源码事实 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `KernelInfo` 10 字段；`kern_stack_top`/`user_sp`/`syscall_entry` 不再是字段，消费点派生（§3.2.1，doc:1154-1188、2320-2322） | `kernel_info.rs:20-150` 十个字段逐一对应；`kern_stack_top()` 是 `kern_virt_base + kern_size` 的 `checked_add`（`:236-243`）；`check_boot_postconditions` 只剩 `kern_virt_base > u32::MAX`（`:601-617`） | 无实质差异 | 接受（文档准确） |
| 状态表达 | `ValidationLevel` 三档取代两个 bool；`Strict` 不带载荷，权威从 `KernelInfo` 读取（§3.2.1、§4.4.1） | `ValidationLevel` 在 `kernel_info.rs:152-179`；`validate_view` 只在 `if level.is_strict()` 分支读 `authority`（`:484-534`）；BeforeTables/Standard 分支完全不用这个参数 | 非 strict 入口带一个无读者的 `authority` 参数；`arch_boot_finish` 的 `report.validate_before_tables(authority)` 是纯透传（`lib.rs:228`） | 值得改（DESIGN-04） |
| 状态表达 | `BootStorage` 五类/六类载荷；"五个 `fill_*` 入口"（§3.7，doc:1842） | `BootStorage` 是 6 个定长数组 + 6 个长度 + 5 个 `committed` 布尔（`boot_storage.rs:86-131`）；UEFI 腿生产路径只有 4 次 `fill_*`（`uefi_helpers.rs:1087/1099/1103/1167`），其中 `fill_map_tables` 一次提交两表 | "五类载荷"对；"五个 `fill_*` 入口"错（4 个调用 / 5 类落点）；source 一族在 UEFI 腿不经 `fill_*` | 值得改（并入 DESIGN-01 的文档计数） |
| 状态表达 | 报告视图的字段形状与 `KernelInfo` 一致（§3.2，doc:1202） | `BootReport` 9 个字段（`boot_storage.rs:291-314`）；`KernelInfo` 10 个（多 `authority`）；`KernelInfoView` 也是 9 个方法（`kernel_info.rs:186-196`） | 少一个 `authority`；直核腿因此把权威作为独立实参再传两次（`bootface_common.rs:554/556`） | 文档该改；设计上属合理取舍（见 VERDICT） |
| 身份 | `authority` 进 `KernelInfo`，"图和它的声明"同一值；`BootReport::validate_requiring_sources(authority)` 由调用者传（§3.2，doc:1187） | `KernelInfo.authority`（`kernel_info.rs:149`）；`validate_requiring_sources()` 无参（`:286-288`）；`BootReport` 无该字段，方法收参数（`:631-633`） | 接收侧绑定成立；发送侧（直核面）仍是两个独立实参 | 接受（发送视图是瞬时视图；残余风险见 VERDICT） |
| 身份 | `MODULE_NAMES` 12 项、位置即身份、strict 门按位置核名（§4.3、§4.4.1） | `kernel_info.rs:672-682`；strict 核名 `:494-501`；wire 名槽 16 B（`handoff_v1.rs:45-56`） | 与 C 的 `image[]` 顺序一致；C 的 `module_list` 身份是数组下标，Rust 同构 | 接受 |
| 所有权 | 五腿发送侧只构造 `BootReport`；接收侧"统一走 `store_boot_report`"（§4.4 开头 doc:2284；§4.0 小结 doc:1888） | 直核腿 `arch_boot_finish` → `store_boot_report`（`lib.rs:230`）；v1 接收腿 `arch_boot_resume_v1` 先把六个分区 decode 进 `.bss` store 并构造 `KernelInfo`（`:663-746`），再 `resume_additive` → `store_kernel_info`（`:325`）→ `land_and_store`；`store_boot_report` 只有一个调用点 | "统一走 `store_boot_report`"不成立；真正统一的是 `land_and_store`。§4.5（doc:2397）与 §3.7（doc:1842）已写对，形成章内矛盾 | 值得改（DESIGN-01，P1） |
| 所有权 | §4.4 字段表 `source_records` 行："五条腿的发送侧都填 `BootReport`"（doc:2324） | UEFI 腿 `storage.source_records` 为空：分类器与 DTB claims 两阶段直写 wire（`ebs_pair.rs:195-249`；`uefi_helpers.rs:1240-1242` 注释与实参）；`boot_storage.rs:298-302` 也自述此例外 | 与 §3.2/§3.7/§4.5 及源码冲突 | 值得改（并入 DESIGN-01） |
| 所有权 | "UEFI 腿的 source 分区由两个阶段直写 wire；storage 的 source 切片为空"（§3.7，doc:1842；§4.2 doc:2014-2018） | `ebs_pair.rs:195-249`：`authorize_and_copy` → `resv_claims::capture` → `push_dtb_sources` → `finish_sources`；`uefi_helpers.rs` 无 `fill_source_records` | 一致 | 接受 |
| 所有权 | `BootStorage` 住"shim 自己的静态区（`.data`）"（§4.2 引文注释 doc:1961） | 三处生产调用都是 `-> !` 入口的栈局部：`uefi_helpers.rs:1086`、`uboot_main.rs:475`、`bootface_common.rs:354`；`boot_storage.rs` 模块文档写的是"static ... or on its stack"（`:8-11`）；全仓无 `static ... BootStorage` | 位置与生命周期叙述错误 | 值得改（DESIGN-02，P2） |
| 所有权 | "半提交状态不可表示"（§3.7，doc:1842） | 族内确实不可表示（`fill_*` 同一步写长度与 `committed`，`boot_storage.rs:141-223`）；跨族可表示（例如 modules 已提交、map 未提交），`report()` 会照常给出空表（`:256-277`） | 表述过强；跨族半提交由接收侧 `KernelInfo::validate` 的"memmap 非空 / 模块非空 / 来源覆盖"兜住 | 文档收紧措辞；设计接受 |
| 错误 | 可信链 `assert!`（fail-fast），外部/不可信输入用 `Result`；四类错误各有落点（§3.7、§4.4.1） | `validate_view` 全 `assert!`（`kernel_info.rs:290-560`）；wire 解码 `HandoffV1ReadError`（`handoff_v1_read.rs:41-117`）；分类 `PolicyError`（`ebs_map.rs:292-360`）；DTB `DtbMemMapError`（`memmap_source.rs:58`）；四类错误与文档一致 | 一致 | 接受 |
| 并发 | Kernel = SMP + BKL；boot 单线程；`KERNEL_INFO` 写一次 | `KERNEL_INFO: SyncUnsafeCell<Option<KernelInfo>>`（`globals.rs:279`），`BklProtected` 封口（`:105-208`）；`land_and_store` 第二次 store 直接 panic（`lib.rs:1586-1589`）；`PT_ALLOC` 用 CAS 写一次、0 = 未注册（`pt_alloc.rs:57-70`）；`BootAlloc` 自述"非并发安全、boot 单线程"（`boot_alloc.rs:14-22`） | 文档没有点名 `BklProtected`/`SyncUnsafeCell`，但没有相反主张 | 接受（可选补一句类型级守卫） |
| 抽象边界 | `FileLoader` 是唯一取件接缝；`BootShim` trait 已删除（§3.5.1） | `FileLoader` 3 个实现（UEFI/U-Boot/`MockLoader`），trait-audit `MULTI-IMPL`、16 refs；`BootShim` 在源码零命中（只剩 `os/README.md:43` 的旧说法） | 文档正确；README 是章外旧账 | 接受（附记 README） |
| 抽象边界 | 没有"内存图来源"trait，理由 = MapKey 与 EBS 的事务绑定（§3.2，doc:1202） | 无此 trait；`ebs_pair::pair_and_classify` 与 `ebs_map::classify`/`adjudicate` 都是普通函数（`ebs_map.rs:587-646`、`ebs_pair.rs:195-249`） | 一致 | 接受 |
| 抽象边界 | crate 依赖单向；四份依赖表（§3.5.2，doc:1640-1647） | `os/kernel/Cargo.toml` 无 `boot-shim`/`uefi`；`minix-boot` 只有 `minix-types`+`fdt`；`boot-shim` 的 `uefi`/`uefi-raw` 可选；`kernel-image` 的 `boot-shim`/`fdt` 只在 riscv64/aarch64 target 下注入 | 一致 | 接受 |
| 可测试性 | `FileLoader` 提供测试接缝；宿主测试锁 wire 与字段台账（§3.5.1、§5） | `MockLoader`（`loader.rs:678-696`）；`handoff_v1_read`/`handoff_v1_write`/`kernel_info`/`boot_storage` 各有负控测试；`vm_handoff` 有 arena poison 测试（`vm_handoff.rs:1088` 起） | 一致；另发现两个公开校验门零调用点、两个测试模块保留退役形态脚手架 | 接受 + 清理项（DESIGN-05/06/07） |
| 可测试性/状态表达 | `KernelInfo<'static>`（doc:1202、1842、1888、2284 四处） | 类型没有生命周期参数（`kernel_info.rs:20-21`）；`'static` 由字段类型 `&'static [T]` 承载 | 幻影类型参数；源码注释也有同样写法（`bootface.rs:37`、`lib.rs:1570`、`bootface_common.rs:548`） | 值得改（DESIGN-03，P2） |
| 错误/接口面 | "C 侧其余字段由 `GET_IMAGE`/`GET_MONPARAMS`/`VmBootHandoff` 等另一通道承载"（§3.2.2.2，doc:1250-1252） | C 有 `GET_IMAGE`、`GET_MONPARAMS`，**没有** `GET_KMESSAGES`/`VmBootHandoff`；两者是 Rust 新增（`os/libs/minix-types/src/ipc/sysinfo.rs:47`、`os/libs/minix-types/src/types/boot.rs:241`） | 把 Rust 新增通道归到 C 侧；`kmessages` 在 C 里是 `GET_KINFO` 整结构里的一个指针，没有专用请求 | 值得改（DESIGN-09，P3） |

## SUBTRACTION-TABLE（V3.6，C-32）

本轮**没有采纳任何新增抽象**。下表列出考虑过并拒绝的加法，减法一列即"更少概念的替代"；"新增概念数"一列附带教学成本账（C-35：新概念引入数 / 讲解增量 / 与既有叙事冲突）。

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| `BootReport` 加 `authority: RamAuthority` 字段，并让 `arch_boot`/`validate_requiring_sources` 从报告取权威 | 不做：直核两条腿在同一调用点传两次字面量 `RamAuthority::DTB`（`bootface_common.rs:554/556`），文档写明"发送视图不含权威" | 未来只改一处的单边编辑仍可编译：strict 用 DTB、store 用 UEFI 时 `Standard` 覆盖审计不按权威过滤（`kernel_info.rs:523-534`），错配不会被 kmain 门发现 | 加法 = 编译期同一值；减法 = 调用点约定 + 代码审查 | +1 字段（净参数 −1）；讲解 +2 行；与"九字段视图"的新表述要同步 | 否 |
| `ValidatedReport<'a>` token：`validate_requiring_sources` 返回令牌，`arch_boot` 只收令牌 | 不做：保持"先 validate 再 arch_boot"两行相邻代码 | 同上一行；减法也挡不住"先 arch_boot 后 validate"的顺序倒置 | 加法 = 编译期顺序 + 权威绑定；减法 = 文档与门序 | +1 类型 + 1 生命周期关系；讲解 +5 行；与 §3.6/§4.4.1 的门序叙述冲突 | 否 |
| 用阶段类型/泛型把 `BootStorage` 的"跨族半提交"变成不可表示 | 保留 5 个 `committed` 布尔 + 接收侧 `KernelInfo::validate` | 类型仍允许"只填一族"的报告；减法靠消费端 fail-closed，生产腿不会走到这条路径 | 加法 = 生产期不可表示；减法 = 消费端拒绝（可观察结果同为停机） | +N 个泛型参数/阶段类型（≥3 个概念）；讲解 +15 行；与 §3.7 现有叙述冲突 | 否 |
| 把 `source_records` 的"空切片 = wire 是权威"编码成 enum/tag | 保留空切片 + 注释 + strict 门（空记录 fail-closed，`kernel_info.rs:502-505`） | 若未来有人把 `report` 当完整视图再序列化，会写出空 source 分区；但接收侧 strict 门因"无来源记录"拒收，不会误放行 | 加法 = 类型区分；减法 = 消费端拒绝 | +1 枚举 + 1 分支；讲解 +3 行；与 `BootReport: Copy` 的简单性冲突 | 否 |

## FINDING DESIGN-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §4.4 开头（doc:2284）、§4.0 小结（doc:1888）、§4.4 字段表 `source_records` 行（doc:2324）、§3.2 收口段（doc:1202）、§3.7（doc:1842 的"五个 `fill_*` 入口"）。
- 严重度：P1（若按 P0 六分类，本条属 **P0-fact**：文档与源码事实冲突；无 boot 行为影响）。
- 问题：三处把"五腿发送视图 / 统一接收入口"写成不存在的事实。(a) "接收侧统一走 `store_boot_report`"——源码只有直核面走它；v1 面是 `decode_*` 落六个 `.bss` store → 构造 `KernelInfo` → `store_kernel_info` → `land_and_store`；真正统一的是 `land_and_store`。(b) §4.4 字段表说五条腿都填 `BootReport.source_records`——UEFI 腿的 storage source 切片按设计为空，记录由 `ebs_pair` 两阶段直写 wire。(c) §3.2 说"报告视图的字段形状与 `KernelInfo` 一致"——`BootReport` 9 字段，`KernelInfo` 10 字段（少 `authority`）。另 (d) §3.7 的"五个 `fill_*` 入口"与源码的 4 次调用不符。
- 影响：本章的核心卖点是"五条腿收敛到同一接收侧"；上述表述会让读者以为 v1 面也经过 `store_boot_report`、以为 report 是完整视图（含 source 与 authority）、以为 UEFI 腿的 storage 拥有 source 一族。§4.5 与 §3.7 已写对，构成章内自相矛盾，读者无法靠本章自洽。
- 证据或推理：`store_boot_report` 的定义与唯一调用点在 `os/kernel/src/lib.rs:1571-1576`、`:230`；v1 路径在 `os/kernel/src/lib.rs:663-751`、`:300-350`；`BootReport` 字段在 `os/libs/minix-boot/src/boot_storage.rs:291-314`；UEFI 腿 4 次 `fill_*` 在 `os/boot-shim/src/uefi_helpers.rs:1087/1099/1103/1167`，无 `fill_source_records`；`os/boot-shim/src/ebs_pair.rs:195-249`。
- 最小修复：(a) 改成"接收侧统一走 `land_and_store`：直核面经 `store_boot_report`，v1 面经 `store_kernel_info` 包装"；(b) 改成"直核腿与 U-Boot shim 填 `BootReport.source_records`，UEFI 腿由分类器与 DTB claims 两阶段直写 wire"；(c) 改成"报告视图是 `KernelInfo` 去掉 `authority` 的九字段视图，权威由腿在调用点提供"；(d) "五个 `fill_*` 入口"改为"四次 `fill_*` 调用、五类落点"。
- 减法替代（V3.6，C-32；无替代写「无，因…」）：无，因这是删除不成立的文字，不新增任何概念。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无类型提案）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高。
- 是否需源码核实：已核实（上列行号）。
- 若我错了：若存在另一条把 v1 面也接到 `store_boot_report` 的调用路径（全仓 grep 无），或 `BootReport` 另有 `authority` 字段的构造分支（无），则本条不成立；即便如此，(b) 与 (d) 仍独立成立。

## FINDING DESIGN-02（P2）

- 位置：doc §4.2 引文注释（doc:1961）；同一篇 §4.2.1 动作 8 的约束有相反的正确表述（"报告的生命周期绑定 storage"，doc:2070）。
- 严重度：P2。
- 问题：称 `BootStorage` 住"shim 自己的静态区（`.data`：`BootStorage::new()` 非全零）"。三处生产调用都是 `-> !` 入口函数内的栈局部，不存在 static 存放。
- 影响：所有权/生命周期叙述错误。读者会去 `.data` 找一个不存在的语义表，也会误解"report 借用调用方 storage"的实际生命周期是发送入口的栈帧；若有人据此把 storage 改成 static 反而引入不必要的静态面。
- 证据或推理：`os/boot-shim/src/uefi_helpers.rs:1086`、`os/boot-shim/src/uboot_main.rs:475`、`os/kernel-image/src/bootface_common.rs:354` 均为 `let mut storage = BootStorage::new();`；`os/libs/minix-boot/src/boot_storage.rs:8-11` 自述"static ... or on its stack"；`grep -rn "static.*BootStorage" os --include=*.rs` 零命中（测试里的 `static` 也是别的对象）。
- 最小修复：改成"发送入口自己的栈帧（该入口不返回），`BootReport` 借它"。
- 减法替代（V3.6，C-32）：无，纯文档修正。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若某 cfg/测试构建把 storage 放 static（未发现），原句才成立；但正文描述的 UEFI/U-Boot/直核三腿都不成立。

## FINDING DESIGN-03（P2）

- 位置：doc:1202、1842、1888、2284；源码注释 `os/kernel-image/src/bootface.rs:37`、`os/kernel/src/lib.rs:1570`、`os/kernel-image/src/bootface_common.rs:548`。
- 严重度：P2。
- 问题：`KernelInfo<'static>` 这个写法在类型系统里不存在——`KernelInfo` 没有生命周期参数，`'static` 由六个切片字段的类型承载。
- 影响：读者会以为存在一个借用形态的 `KernelInfo<'a>`（正是 DESIGN-02(b) 之前"发送视图/接收结构共用一张生命周期化类型"的残影）；在"唯一的 `KernelInfo`"论证段里出现，会让"唯一副本"与"静态副本"两件事混起来。
- 证据或推理：`os/libs/minix-boot/src/kernel_info.rs:20-21` 的 `pub struct KernelInfo { ... }` 无泛型参数；字段 `memmap: &'static [MemoryRegion]` 等。
- 最小修复：文档四处改为 `KernelInfo`（可加一句"其切片字段类型均为 `&'static [T]`"）；源码三处注释同批清理（不在本篇 doc 的强制范围，建议同 PR，否则文档与注释仍不一致）。
- 减法替代（V3.6，C-32）：无，删除幻影参数。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用（这是减少一个概念）。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若团队把它当"约定俗成简写"——即便作为简写，它在教学文档里与源码签名冲突，仍应改。

## FINDING DESIGN-04（P2）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:260-288`（`KernelInfo::validate`/`validate_before_tables`）、`:619-638`（`BootReport::validate`/`validate_before_tables`/`validate_requiring_sources`）、`:290-534`（`validate_view`）、`os/kernel/src/lib.rs:228`。
- 严重度：P2。
- 问题：`validate_view` 只在 `if level.is_strict()` 分支读 `authority`；BeforeTables/Standard 入口仍收 `authority` 实参。`arch_boot_finish` 的 `report.validate_before_tables(authority)` 是纯透传——pre-table 门并不检查权威。
- 影响：API 表面让人以为 pre-table 门校验"声明权威与记录集一致"，实际不检查；在直核腿上权威有两份实参（`report.validate_requiring_sources(DTB)` 与 `arch_boot(report, DTB, ...)`，`bootface_common.rs:554/556`），与文档强调的"图和它的声明是同一个值"相悖，且给未来的单边改动留了口子。
- 证据或推理：`validate_view` 的 `authority` 唯一读者在 `is_strict()` 分支（`kernel_info.rs:484-522`）；`KernelInfo::validate` 传入 `self.authority` 后不被读；`BootReport::validate` 全仓零调用点。
- 最小修复：从 BeforeTables/Standard 入口删除 `authority` 参数（`validate_view` 改成只在 strict 路径接收，如 `Option<RamAuthority>`、或拆出 `validate_common` + `validate_strict`）；保留 `validate_requiring_sources(authority)` 与 `store_boot_report(..., authority)`。这样 pre-table 门的签名与实际检查一致。
- 减法替代（V3.6，C-32）：就是减法方案：删一个参数、删一条透传，不新增概念。剩余"直核两处常量可能单边改动"的风险见 SUBTRACTION-TABLE 第 1 行，本轮不采纳加法。
- 三问（V3.6，C-31）：不适用（这是删除参数，不是新增类型）。
- 教学概念账（V3.6，C-35）：不适用（净减少概念）。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若团队计划让 Standard 门也按权威过滤，则该参数是预留扩展点；但当前实现不读，按"无读者即删"的纪律应删，等有读者时再加。

## FINDING DESIGN-05（P3）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:270-272`（`KernelInfo::validate_before_tables`）、`:623-625`（`BootReport::validate`）。
- 严重度：P3。
- 问题：两个公开校验入口全仓零调用点。`KernelInfo::validate_before_tables` 的文档注释还声称"`os/qemu-tests` fixtures 使用它或直接调 `arch_boot_impl`"，实际 fixtures 走后者，前者无人调用；`BootReport::validate`（Standard 档）也没有任何调用点。六个校验入口里两个是死面（live：`KernelInfo::validate`、`KernelInfo::validate_requiring_sources`、`BootReport::validate_before_tables`、`BootReport::validate_requiring_sources`）。
- 影响：公开 API 面大于实际语义；"哪一个是权威门"变模糊，读者会以为存在第三条 pre-table/Standard 的调用路径。
- 证据或推理：`grep -rn "validate_before_tables" --include=*.rs .`（排除 `target/`）只有定义与 `report.validate_before_tables` 一处调用；`grep -rn "\.validate("` 无 `BootReport::validate` 调用点。
- 最小修复：删除这两个方法（或把 `KernelInfo::validate_before_tables` 标成 `#[cfg(test)]` 并在测试中使用；`BootReport::validate` 直接删）。同步修正 `:279` 的注释。
- 减法替代（V3.6，C-32）：就是减法。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用（净减少概念）。
- 置信度：高。
- 是否需源码核实：已核实（全仓 grep）。
- 若我错了：若有仓外工具依赖这两个 crate 的公开 API——本仓是唯一消费者（workspace 内无调用点）；保留无消费者的门只会让门链叙述失真。

## FINDING DESIGN-06（P3）

- 位置：`os/kernel-image/src/bootface_common.rs:507`、`:528`。
- 严重度：P3。
- 问题：`param_ptr` 被赋值后没有任何读者；DESIGN-02(b) 之后发送侧不再构造 `KernelInfo`，这个指针是旧构造路径的残留。
- 影响：无行为变化；若开启 `unused_assignments` 会告警；也误导读者以为参数缓冲除 `storage` 之外还有第二条通道。
- 证据或推理：`grep -n "param_ptr" os/kernel-image/src/bootface_common.rs` 只有 507（初始化）与 528（赋值）两行。
- 最小修复：删除这两行。
- 减法替代（V3.6，C-32）：就是减法。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若 `param_ptr` 通过闭包捕获被间接读取——闭包在 `fill_param` 内只写不读，`fill_param` 返回后也无读者；不存在间接读。

## FINDING DESIGN-07（P3）

- 位置：`os/boot-shim/src/uefi_helpers.rs:1406-1421` 与调用点 `:1453`；`os/boot-shim/src/opensbi_helpers.rs:635-651` 与调用点 `:781`、`:840`。
- 严重度：P3。
- 问题：两个测试模块各有一份 `fn test_kernel_info(report, authority) -> KernelInfo`，把 `BootReport` 的九个字段逐项抄进 `KernelInfo`，然后测试断言抄写结果。这是 DESIGN-02(b) 之前"发送侧构造 `KernelInfo`"的测试脚手架；现在生产路径根本不构造发送侧 `KernelInfo`，这些断言只证明测试辅助函数会抄字段。
- 影响：测试通过不证明生产行为；两份辅助函数是三份同构代码（再加 ebs_pair 的 `write_min_envelope` 载体），未来字段变化要三处跟改。
- 证据或推理：两个函数体相同；调用点只做字段相等断言；生产发送路径使用 `storage.report()`（`bootface_common.rs:549`、`uefi_helpers.rs:1134/1239`、`opensbi_helpers.rs:425`）。
- 最小修复：删除两份 `test_kernel_info` 与三处调用；断言直接落在 `report` 或 `storage.report(...)` 上（`BootReport` 字段是 `pub`，测试可直接读）。
- 减法替代（V3.6，C-32）：就是减法。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若这些测试想覆盖"report → KernelInfo 字段搬运"这一层——但该层已由 `land_and_store` 的宿主测试与 `arch_boot_resume_v1` 路径覆盖，测试辅助函数不是生产搬运器。

## FINDING DESIGN-08（P3）

- 位置：`os/libs/minix-boot/src/platform.rs:123-132`（`#[repr(C)]` + `const _: () = assert!(size_of::<PlatformDescSource>() == 16)`）；文档 §3.2.3 doc:1282 把它称为"例外"。
- 严重度：P3。
- 问题：`PlatformDescSource` 是纯进程内句柄（`parse_by_kind` 按值消费、`find_platform_sources_into` 写槽、`land_slice` 同型拷贝）；跨镜像表示是独立的 `WirePlatformDescSource`（`handoff_v1.rs:63-68`，有自己的 16 B 断言 `:402`）。`repr(C)` 与 16 B 断言是 v8 arena 语义分区时代的形状锁，今天没有任何按字节读者；去掉 `repr(C)` 后字段顺序改变但 `size_of` 仍为 16（u32+Option<NonZeroU32>+u64，align 8），无消费者可见差异。
- 影响：一个死约束留在交接层，并迫使文档把它写成"例外"；若未来有人误以为它是跨镜像 ABI，会与 `WirePlatformDescSource` 的双轨纪律冲突。
- 证据或推理：全仓 `PlatformDescSource` 使用点（`os/kernel/src/lib.rs:1211/1268/5925/5998`、`os/boot-shim/src/uefi_helpers.rs:207-253`、`os/libs/minix-platform/src/kind.rs:49`）均为值级读写；无 FFI/裸字节转换。wire 侧另有 `WirePlatformDescSource`。
- 最小修复：删除 `#[repr(C)]` 与 16 B 断言，同步改 doc §3.2.3 的"例外"句；若团队想保留形状自证，则至少把 `repr(C)` 去掉、只留断言（但断言也无读者，建议一起删）。
- 减法替代（V3.6，C-32）：就是减法。
- 三问（V3.6，C-31）：不适用（删除，不新增）。
- 教学概念账（V3.6，C-35）：不适用。
- 置信度：中高（仓内已核；若存在仓外按字节序列化该结构的工具则不成立）。
- 是否需源码核实：仓内已核实。
- 若我错了：若未来计划把该结构直接落 wire/落盘，`repr(C)` 是预埋——但当前设计明确"wire 只认 wire 类型"（`handoff_v1.rs` 模块文档），预埋与纪律冲突，应等到有真实消费者再加。

## FINDING DESIGN-09（P3）

- 位置：doc §3.2.2.2（doc:1250-1252）。
- 严重度：P3。
- 问题：句中断言"C 侧其余字段由 `GET_IMAGE`/`GET_MONPARAMS`/`VmBootHandoff` 等另一通道承载"。C 只有 `GET_IMAGE`（`minix3/minix/kernel/system/do_getinfo.c:86-89`）与 `GET_MONPARAMS`（`:143-146`）；`GET_KMESSAGES` 是 Rust 新增的请求号 7（`os/libs/minix-types/src/ipc/sysinfo.rs:43-47`），`VmBootHandoff` 是 Rust 侧交接结构（`os/libs/minix-types/src/types/boot.rs:241`）。C 的 `kmessages` 只作为 `GET_KINFO` 整结构里的一个指针被顺带拷出，没有专用请求。
- 影响：读者会把 Rust 新增通道误当成 C 的既有面，进而误判 GET_KINFO 收窄的"C 行为基线"（C 的基线是"整结构拷贝，含无效指针"，不是"C 本来就用 GET_KMESSAGES 取日志"）。
- 证据或推理：C 全树 `grep -rn "GET_KMESSAGES\|VmBootHandoff" minix3/` 零命中；Rust 侧见上列路径。
- 最小修复：改为"Rust 收窄后，这些内容改由 `GET_IMAGE`/`GET_MONPARAMS` 与**新增的** `GET_KMESSAGES`/`VmBootHandoff` 承载"，并把新增通道标成 `[ARCH]`（与已有的 GET_KINFO 收窄标注同段）。
- 减法替代（V3.6，C-32）：无，纯文档修正。
- 三问（V3.6，C-31）：不适用。
- 教学概念账（V3.6，C-35）：不适用。
- 置信度：高。
- 是否需源码核实：已核实（C 与 Rust 两侧 grep）。
- 若我错了：若 `minix3/` 树只是 subset、另有分支含 `GET_KMESSAGES`——本仓的真值基线就是这棵树；按"Minix3 C 源码行为"优先序，应以此为准。

## ARCH-PROPOSAL

本轮无 `[ARCH]` 提案。以上全部是 `[DEL]`（删除不成立的文字、死参数、死局部、死 API、死形状锁）或文档与源码的对齐；没有任何"改变 boot 是什么"的机制替换、新责任、新字段、新检查或新策略。

## VERDICT

**值得改（按优先级）**

1. **DESIGN-01（P1）**：Ch3/Ch4 关于"统一接收侧"的三处不实表述 + §3.7 的 `fill_*` 计数。这是本章核心主张的事实错误，且与 §4.5/§3.7 的正确段落自相矛盾；改法是收紧三句话，不涉及代码。
2. **DESIGN-02（P2）**：`BootStorage` 的 `.data` 静态区说法。改成"发送入口的栈帧"。
3. **DESIGN-03（P2）**：`KernelInfo<'static>` 幻影生命周期（文档 4 处 + 源码注释 3 处）。这是旧借用视图形态的残影，正好落在 DESIGN-02(b) 之后的清理面上。
4. **DESIGN-04（P2）**：非 strict 校验入口的 `authority` 死参数。属于"删参数"级别的减法，建议与 DESIGN-05 一起做。
5. **DESIGN-05/06/07（P3）**：两个零调用点的公开校验门、`param_ptr` 死局部、两份 `test_kernel_info` 脚手架。都是 DESIGN-02(b) 迁移后的残骸，一次清理即可。
6. **DESIGN-08（P3，需用户裁决）**：`PlatformDescSource` 的 `repr(C)` + 16 B 断言。删除无消费者可见差异，但会改 §3.2.3 的一句"例外"叙述；若团队把"16 B 句柄"当作对外教学形状，可保留并在此处明确"仅为文档形状，不是 ABI"。
7. **DESIGN-09（P3）**：把 Rust 新增的 `GET_KMESSAGES`/`VmBootHandoff` 归到 C 侧的句子。

**合理取舍（判为"保留"，不报缺陷）**

- **`KernelInfoView` 保留**：它不是死抽象。两个实现（`KernelInfo`/`BootReport`）各守信任边界一侧，`validate_view` 只写一份校验体，`land_and_store` 也只写一份落地体；删掉它必须让直核面"先落地再校验"，会改变 §4.4.1 的门序并丢掉"发送侧先校验自己的报告"这一层。trait 无 `dyn`、无运行期分派，成本只是 9 个机械访问器。
- **`BootStorage` 的 5 个 `committed` 布尔保留**：它们不是状态机，而是"每族一次提交"的哨兵；删掉后第二次 `fill_*` 会静默覆盖，从"启动现场 panic"退化成"最后写入者获胜"。跨族半提交确实可表示，但消费侧 `KernelInfo::validate` 已经 fail-closed。
- **`BootReport` 不带 `authority`**：它是瞬时借用视图，权威在直核面由同一调用点的字面量决定；接收侧才需要把权威绑定进 `KernelInfo`。残余风险（未来单边改两处常量之一）只影响审计声明，不影响内存图或映射决策；加法方案见 SUBTRACTION-TABLE 第 1/2 行，本轮不采纳。若要彻底关闭，需用户裁决。
- **UEFI 腿 `source_records` 为空**：wire 是唯一权威，strict 门对空记录 fail-closed（`kernel_info.rs:502-505`）；不用 enum/tag 区分（SUBTRACTION-TABLE 第 4 行）。
- **固定数组 + 计数的哨兵形态**（`ZERO_REGION`/`ZERO_MODULE`/`ZERO_SOURCE`/`ZERO_RECORD`、`EMPTY_PHYS_RANGE`、`PT_ALLOC = 0`、`BootAlloc` 的 `end == 0`、`HandoffMemRegion::ZERO`、`VM_PMM_FREE`）：`no_std` + 静态落地区的合理形态；每个都有"只暴露活前缀"或"使用前校验"的纪律（`as_slice` 截到 `len`、`is_registered`、`BumpRegion::is_consistent`、`vm_pmm_free_regions` 带 count），不构成可观察非法态。
- **`MemoryRegion.len`/`BootModule.len` 用 `usize`**：它们是进程内表示；wire 用 `u64` 且有独立 `WireMemoryRegion`/`WireBootModule`。改成 `u64` 只会把 wire 宽度带进进程内。
- **`#[doc(hidden)] write_envelope(&KernelInfo)`**：它是 14 个测试调用点的载体，已注明"非生产发送 API"；删除要改 14 处测试而收益只是形式统一，保留。
- **`KernelInfo`/`BootReport` 的 `Copy`**：10 个字以内的纯值拷贝，无共享可变面。
- **`KernelInfo::source_audit`**：只有测试读者，但它是 D-111 取证字段的诊断访问器；按 D-111 的"取证字段不以正常路径读取次数为删除依据"保留。

**最小化机会（task 第 5 项，逐项判）**

- 去掉 `KernelInfoView`（1 trait / 9 方法 / 2 实现）：否。理由见上（两份校验体与两份落地体各只有一份；删掉会改变门序并丢掉发送侧先校验）。
- 合并 `store_boot_report`/`store_kernel_info` 两个 wrapper：否。二者只差权威来源（参数 vs `KernelInfo.authority` 字段），共同核心已是 `land_and_store`；合并要么把权威假设带回视图，要么在函数里分支，都比现状多概念。
- 把直核面的 strict 门收进 `arch_boot_finish`、删掉两个 `bootface` 的显式 `validate_requiring_sources` 调用：技术可行（两处 `RamAuthority::DTB` 字面量合一），但它把"发送侧先校验自己的报告再交给入口"改成"入口落地后再校验"，改变 §4.4.1 记录的门序，收益只是少一行调用；判：不做（若要做，属 `[MODERN]` 重构，需用户裁决）。
- 删掉 `BootStorage` 的 5 个 `committed` 布尔：否（理由见上）。
- 删掉一个能力层：`BootStorage` → `BootReport` → wire → decode → `.bss` store 这条链没有一层是纯转发，每层有独立故障面（填充纪律、借用视图、wire 布局、解码校验、`.bss` 所有权）；无层可删。

**死抽象核实（trait-audit 读数 + 自核）**

- 本 scope 相关 trait 的 audit 读数：`FileLoader` `MULTI-IMPL`（3 impl / 16 refs / 1 dyn）、`HigherHalf` `MULTI-IMPL`（4/31）、`BootArch` `MULTI-IMPL`（4/58）、`DirectMapArch` `MULTI-IMPL`（4/93）、`Paging`/`HugePages`/`EarlyConsole` 均 `MULTI-IMPL`、`PlatformDesc` `MULTI-IMPL`（7/27/3 dyn）、`BklProtected` `NO-IMPL`（0/31）。
- **`BklProtected` 是 audit 假阴性**：impl 由 `bkl_protected_impls!` 宏生成（`os/kernel/src/globals.rs:105-208`），且它是封口 marker trait，作用是给 `SyncUnsafeCell` 加 `Sync` 闸门；不是死抽象。`impls: 0` 只说明审计工具不展开宏。
- **`KernelInfoView` 不在 audit 清单**；自核 2 实现 / 9 方法，保留理由见上。
- **结论**：本 scope 没有"今天不承担语义职责"的 trait。真正的死面是**方法/局部**（DESIGN-05/06/07），不是 trait。`BootShim` 已在源码零命中（只剩 `os/README.md:43` 的旧说法，章外附记）。

**翻译残留分类（task 第 2 项逐个口径）**

- **单实现依赖注入 trait**：本 scope 无。`FileLoader` 是 3 实现（UEFI/U-Boot/Mock）的取件接缝；`KernelInfoView` 是 2 实现、无 dyn 的去重接缝，不是"为单一实现注入依赖"。
- **无状态 Manager**：无。`BootStorage`/`FreeRangeSet`/`BootAlloc` 都有状态且改名不能减少概念；没有一个只转发调用的 `Manager`。
- **布尔/字符串状态机**：只有 `BootStorage` 的 5 个 `committed` 布尔（判：保留，见上）；`ValidationLevel` 已从"两个 bool"升成三态 enum（`kernel_info.rs:152-179`），是本篇做对的方向；模块名当身份有位置 + 严格核名的双保险，不是字符串状态机。
- **哨兵返回值**：`PT_ALLOC = 0` / `BootAlloc.end == 0` / `EMPTY_PHYS_RANGE` / `ZERO_*` / `HandoffMemRegion::ZERO`（判：保留，见上）。`BootAlloc::region()` 对外返回 `Option`，哨兵只在内部；`PT_ALLOC` 收的是 fn 类型，null 不可表示。
- **clone 到能编译**：发送路径零 `.clone()`/`Box::leak`/`Vec` 发送载荷（`grep -rn "\.clone()\|Box::leak" os/libs/minix-boot/src os/boot-shim/src os/kernel-image/src` 的命中都在测试或 loader 的 `FileBytes` 物化，不在交接视图）；`storage.report()` 构造视图而非 clone 数据。
- **对象图共享**：`BootReport` 借 `BootStorage`、`KernelInfo` 在 `.bss`、无 `Rc`/`RefCell` 跨对象；`RefCell` 只出现在 `find_platform_sources_into` 的局部收集器（Boot Services 单线程环境，合理）。
- **旧形态残骸（新增归类）**：`param_ptr`、两份 `test_kernel_info`、`BootReport::validate`/`KernelInfo::validate_before_tables`、`KernelInfo<'static>` 写法、`PlatformDescSource` 的 repr(C) 形状锁——即 DESIGN-03/05/06/07/08。

**非法状态（task 第 4 项）**

- 可表示且**由消费侧 fail-closed**：`BootReport` 可由结构体字面量直接构造（字段全 `pub`，测试就这么做），绕过 `BootStorage` 的填充/提交纪律；`KernelInfo` 也可由 `authority` 与记录集错配。生产路径上 v1 面在 `arch_boot_resume_v1` 跑 strict、直核面在 `bootface_common` 跑 strict，kmain 的 Standard 只是复核；只有"直调 `arch_boot_impl` 的测试内核"能绕过 strict，且它们的目的是测映射机器而非生产交接。按"威胁模型 = 仓内可信链"判：`assert!` 是正确形态，不需要把 `KernelInfo` 变成 newtype/Builder。若未来新增一条绕过 strict 的生产腿，应把 strict 门收进该腿入口，而不是加类型。
- 可表示且**生产不可达**：`BootStorage` 跨族半提交、`BootReport` 的 `source_records` 为空但 memmap 非空（UEFI 腿的真实形态）；前者由消费门兜住，后者由 wire 作权威 + strict 门兜住。
- **类型上已排除**：`CheckedWindows` 只能由 `BootWindows::check` 构造（`lib.rs:1290-1312`），"未检查的窗口交给建根"不可表示；`MapCounts` 用命名字段防两计数互换（`boot_storage.rs:57-65`）；`PreparedJump` 用命名字段防 entry/arena 互换（`lib.rs:408-414`）；`PhysRange::new` 拒绝反向区间；`PlatformDescSource.len` 用 `Option<NonZeroU32>` 排除 `Some(0)`。
- **不一致但可接受**：v1 接收路径对模块名做 `from_utf8(...).expect`（非 UTF-8 即 panic），`store_kernel_info` 路径取最长合法前缀或 `"?"` 回落（`lib.rs:1650-1653`）。两条路径的非法输入处理不同，但生产名全是 ASCII 且 strict 门按位置核名；若要把"名字必须是 ASCII"变成类型级约束，收益不抵成本。

**需用户裁决**

1. `authority` 是否并入 `BootReport`（SUBTRACTION-TABLE 第 1 行）：减法（不改类型）拦不住"未来只改两处常量之一"的单边错配，但错配后果仅限审计声明，不影响映射/分配；我建议**不改类型**，只按 DESIGN-01 (c) 把"九字段视图"写清。
2. `PlatformDescSource` 的 `#[repr(C)]` + 16 B 断言（DESIGN-08）：删或保留都合理；我倾向删除并与 §3.2.3 同步，但这是"教学形状 vs 死约束"的取舍，交作者裁。
3. `#[doc(hidden)] write_envelope(&KernelInfo)` 是否随测试迁移删除：我建议**保留**（测试载体，已注明非生产）。

**附记（章外，不阻断本篇）**

- `os/README.md:43` 仍把 `BootShim trait` 列为 `minix-boot` 的内容，与本篇"`BootShim` 已删除"及源码零命中冲突；建议另开一条文档清理。
- 本篇 §3.2.1 的 `KernelInfo` 字段表、§4.2.2 的五步判定表、§4.3 的 `MODULE_NAMES` 计数、§3.5.1 的 `cfg(feature)` 计数（7 = 6 uefi + 1 opensbi）、§3.5.2 的四份依赖表、§4.4.1 的十三行门表，逐项与源码核对**一致**，未发现问题。
- 减法优先结论：本轮采纳 0 个新增抽象；若按"删除即减法"计，建议的减法共 7 项：删 §4.4/§4.0/§3.2/§3.7 的四处不实表述（DESIGN-01）、删 §3.2.2.2 的一处归属错误（DESIGN-09）、删 1 个死参数（DESIGN-04）、删 1 个死局部 + 2 个死方法与 2 份测试脚手架（DESIGN-05/06/07）、删 1 个死形状锁（DESIGN-08，待裁决）。全部通过后，本 scope 的交接面概念数净减，不增。
