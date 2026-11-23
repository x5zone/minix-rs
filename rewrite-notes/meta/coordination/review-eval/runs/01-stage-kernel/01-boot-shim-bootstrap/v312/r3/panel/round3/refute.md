DOC 01-boot-shim-bootstrap.md

# D01 R3 反证报告

范围：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3049 行）与 `v312/related-code.txt` 中与启动交接直接相关的源码。本文只做反证尝试，不重复作者的结论，也不改任何仓库文件。

## CLAIM 01

- 主张：跨镜像交接只传值（I1）。三条 v1 腿的 wire 只写固定宽度标量与字节分区，发送侧的虚拟地址指针不进入协议；接收侧把六个分区解码进自己的 `.bss` 并从私有存储重建 `KernelInfo` 切片。两个直核腿虽然同镜像，也走同一组 `.bss` 落地区深拷（文档 `:101`、`:106`、`:1168-1172`、`:1849-1853`）。
- 反证尝试：
  1. 先在 wire 里找指针或 `(ptr, len)`。`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1.rs:23-115` 的四个元素 wire 类型与 `:289-340` 的 `BootHandoffV1` 全部由 `u32`/`u64`/`[u8; 16]` 组成；`:278` 的版本号与 `:356-392` 的分区容量是编译期常量，信封只带 `byte_len`；`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_read.rs:100-193` 的 `check_header`/`decode_envelope` 只读固定偏移。没有可以跨镜像的地址字段。
  2. 再查接收侧是否真的把切片搬到私有存储。`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:663-751` 先做六次 `decode_*` 写进 `MEMMAP_REGION_STORE`、`RESERVED_REGION_STORE`、`BOOT_MODULE_STORE`/名字池、`PLATFORM_SOURCE_STORE`、`PARAM_BUF_STORE`、`SOURCE_RECORD_STORE`，之后才用这些 store 的 `as_ptr()` 构造 `KernelInfo`；`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1575-1705` 的 `land_and_store` 对六个切片逐个 `land_slice`，并把 `BootModule.name` 改写成名字池里的 `&'static str`（`:1624-1656`）。直核腿的 `arch_boot_finish` 在 `arch_boot_impl` 之前调用 `store_boot_report`（`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:228-232`），发送侧的 `BootStorage` 在 `run_self_boot` 的栈帧上（`/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_common.rs:353-355`），但没有任何 `&'a str` 被留在栈帧之外。
  3. 尝试找“值之外还跨了指针”的反例：`PlatformDescSource.phys_addr` 是物理地址，内核在 `kmain` 的 Phase A.5 直接解析 DTB/RSDP blob，不经过 `.bss` 深拷。文档 `:106` 明确把“物理地址”列为允许跨镜像的值，且 `platform_sources` 不是 `KernelInfo` 的切片字段。按“切片/指针”的限定，这不是反例；只有把 I1 读成“所有跨边界数据都必须复制”时才会误判。
  4. 跨节复述：`:40`（小结）、`:68-70`（1.1 表）、`:93`（1.1.1）、`:1168-1172`（3.2 维度二）、`:1849-1853`（3.6 接收侧）对“三条 v1 腿 + 两个直核腿都深拷”的口径一致；`:1388` 的零化读数被标注为“无归档日志、仅为当时观察”，没有把未证机制写成已证。
- 结果：反证失败（主张成立，按文档自己的“切片/指针”限定）。
- 证据：上面的 `handoff_v1.rs`、`handoff_v1_read.rs`、`os/kernel/src/lib.rs`、`bootface_common.rs` 行号；文档 `:101`、`:106`、`:1168-1172`。
- 若我错了：若评审认为 `platform_sources` 指向的 DTB/RSDP blob 也属于 I1 的“跨镜像载荷”，那么 I1 的失败模式行与 §1.0 术语应当把“物理地址句柄不是深拷对象、它的生命周期由 Phase A.5 的消费顺序保证”写成显式条款，否则读者可以从这条合法路径构造出“没有深拷”的反例。除此之外我没有找到 wire 或落地区上的指针残留。

## CLAIM 02

- 主张：来源可追溯（I2）。free 表的每个字节都能被 `kind = Memory` 的来源记录解释，reserved 表的每个字节都能被 reservation 类记录解释；生产腿的 strict 门按 `authority` 过滤并 fail-closed（文档 `:102`、`:1392-1398`、`:2494`）。
- 反证尝试：
  1. 构造未覆盖的 free 字节。`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/memmap_source.rs:317-409` 的 `audit_source_coverage_for` 对每条 free span 调 `records_cover`，`records_cover` 是逐字节贪心并集（`:384-409`）；free 只接受 `SourceKind::Memory`，reserved 只接受 `FirmwarePrefix`/`ReservedOrdinary`/`ReservedReusable`，`Memory`/`MrbEntry`/`ReservedNoMap`/`UefiExcluded` 都不能覆盖 reserved（`:345-377`）。我找不到能绕过 `records_cover` 的空洞。
  2. 构造 authority 错配。strict 分支（`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:480-519`）先把 `Some(authority.input_object())` 传给 audit，再要求 `!source_records.is_empty()` 且至少一条记录的 `input_object` 等于声明权威；UEFI 腿的 DTB claims 记录是 `DtbBootInput`，在 UEFI 权威的审计里被 `admitted` 过滤掉（`memmap_source.rs:341-344`）。而 DTB 区间在 UEFI 腿进入 `reserved_regions` 的路径是 `resv_claims::check_current` 先要求相应类型的 descriptor 覆盖（`/home/xzhao/github/minix-rs/os/boot-shim/src/ebs_pair.rs:195-204`），不是靠 DTB 记录冒充。两条路径没有互相顶替。
  3. 构造“解码成功但引用半成品”的路径。`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_read.rs:362-377` 的 `decode_param_buf` 在 `byte_len > WIRE_PARAM_CAP` 时返回 `DescriptorOverflowsPartition`，在 `count > out.len()` 时返回 `OutputTooSmall`，不截断；memmap/reserved/modules/platform/source 的 `element_count` 也先查可整除与容量（同文件 `:195-350`）。失败即 `Err`，调用方在构造 `KernelInfo` 之前 panic。
  4. 跨节复述：`:102`（I2 行）、`:1392-1398`（3.3）、`:2494`（4.4.1 的 strict 行）、`:2709`（5.1 的 I2 行）都区分了 free 与 reserved 的记录类别；`:2710-2711` 的“不证明什么”列也把“UEFI 腿 source 由分类器直写 wire、宿主只测分类器”写清楚了。没有发现概述与实现互相冲突。
- 结果：反证失败（主张成立）。
- 证据：`memmap_source.rs:317-409`、`kernel_info.rs:480-519`、`handoff_v1_read.rs:195-350` 与 `:362-377`、`ebs_pair.rs:195-204`；文档 `:102`、`:1392-1398`、`:2494`、`:2709`。
- 若我错了：如果审计的贪心并集遇到“同一条记录从 `cursor` 之前开始、但 `end` 不大于 `next`”的组合，可能出现“看似覆盖、实际没推进”的漏洞；我按 `records_cover` 的分支逐条读过，`rs <= cursor && cursor < re && re > next` 的三条件把这种情形排除。若评审有反例样本，应按 `SourceAuditError` 的两个变体构造宿主测试，而不是只读代码。

## CLAIM 03

- 主张：所有权明确（I3）。`BootStorage` 是发送侧唯一所有者；`fill_*` 的写与提交是同一步，族内半提交不可表示；六个落地区里名字池是唯一允许截断的；接收侧切片由 `.bss` 支撑，不能活过 backing 存储（文档 `:103`、`:1094`、`:1912`、`:2541-2561`）。
- 反证尝试：
  1. 找第二个提交入口或半提交状态。`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/boot_storage.rs:147-226` 的五个 `fill_*` 都是“闭包返回计数后立即写长度并置 committed”，每次先 `assert!(!*_committed)`；长度字段私有，没有公开 setter。`startup` 族已不存在：`grep -rn "startup_objects\|push_startup\|startup_committed\|set_startup_len\|startup_slots_mut" os/ --include=*.rs` 零命中，只剩 `boot_storage.rs:303` 的一句“没有 startup slice”注释。文档 `:1912` 说的“五族各自 one-shot”对应的是 modules/platform/param/map_tables/source_records 五个 `fill_*`，不是已删除的 startup 族。
  2. 找 `'static` 报告或悬空名字。`BootReport<'a>` 的字段全部是 `&'a [T]`（`boot_storage.rs:290-318`）；`land_and_store` 在每个切片拷进 store 后才用 store 的指针构造 `KernelInfo`（`os/kernel/src/lib.rs:1697-1702`），模块名由 `:1647-1655` 从名字池重新造 `&'static str`；没有任何 `'static` 切片指向发送侧栈帧或 arena。
  3. 找非名字池的截断。`handoff_v1_read.rs` 的六个 `decode_*` 在超容量时返回错误；`land_slice` 在 `src.len() > dst.len()` 时断言（`os/kernel/src/lib.rs:1535-1548`）；`PARAM_BUF_STORE` 在异址复制前 `param.fill(0)`（`:1670-1675`）。名字池是唯一用 `min(BOOT_MODULE_NAME_LEN - 1)` 截断的路径（`:1627`），v1 接收路径用最多 16 字节的名字（`os/kernel/src/lib.rs:693-715`），与文档 `:2552-2557` 的“两条路径截断点不同”一致。
  4. 尝试并发或重入。`land_and_store` 的“written exactly once”是 `is_none()` 检查后写（`:1583-1586`），不是原子 CAS；它依赖启动路径在 BKL/SMP 之前单线程——这个前提写在源码的 SAFETY 注释里（`:1587-1588` 的 “boot is single-threaded (before BKL exists)”），文档 §4.5 本身没有重复它。`pt_alloc::register` 反而用 `compare_exchange` 并在 release 也 fail-closed（`/home/xzhao/github/minix-rs/os/arch/src/arch/pt_alloc.rs:55-67`），两者强度不同，但当前没有并发的 `store_kernel_info` 调用点。
- 结果：反证失败（生产路径成立）。附一条非阻断观察：`land_and_store` 的一次性守卫是 check-then-write，不是原子操作；它的保证依赖“boot 单线程”，这个前提只在源码的 SAFETY 注释里（`os/kernel/src/lib.rs:1587-1588`），文档 §4.5 没有写明。建议在“written exactly once”一句旁补上这一前提，避免读者把它当作跨阶段/跨 CPU 的硬不变量。
- 证据：`boot_storage.rs:147-226` 与 `:290-318`、`os/kernel/src/lib.rs:1535-1548` 与 `:1575-1705`、`os/kernel/src/lib.rs:693-715`、`/home/xzhao/github/minix-rs/os/arch/src/arch/pt_alloc.rs:55-67`；文档 `:103`、`:1912`、`:2541-2561`。
- 若我错了：如果将来有 AP 在 `store_kernel_info` 之前被启动，或者某个测试内核在两个线程上调用 `land_and_store`，check-then-write 会让“第二次 store 必须 panic”的保证失效；到那时需要把守卫改成原子状态机。当前 `kmain` 的 `smp_init` 在 `proc_init` 之后（`minix3/minix/kernel/main.c:293-311` 的顺序可对照），生产路径不触发。

## CLAIM 04

- 主张：校验先于消费（I4）。接收侧在首次建表/映射/分配之前完成校验；`kmain` 入口的 strict `validate_requiring_sources` 是消费后的复核门（文档 `:104`、`:1910`、`:2477`、`:2491-2494`）。
- 反证尝试：
  1. 查 v1 接收侧的首个 PTE 写之前是否已完成 strict。`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:663-751` 的顺序是：六次 decode（只写 `.bss`）→ 构造 `KernelInfo` → `validate_requiring_sources`（strict，包含 source 覆盖、模块名/位置、模块与内核跨度、模块与模块、内核跨度覆盖）→ `check_boot_postconditions` → `resume_boot_pt_alloc` → `resume_after_v1_decode`。`resume_additive`（`:300-350`）的第一件事是 `boot_validate_and_prepare`，之后才是 `plan_dm_policy`、`set_current_root_phys`、`store_kernel_info`、`establish_boot_dm`。首个页表写确实在 strict 之后。
  2. 查直核面。`/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_common.rs:534-556` 在 `arch_boot` 之前调用 `report.validate_requiring_sources(DTB)`；`arch_boot_finish`（`os/kernel/src/lib.rs:228-232`）在建表之前先 `validate_before_tables`、`check_boot_postconditions`、`store_boot_report`，`arch_boot_impl` 再在 `boot_validate_and_prepare` 之后才建表。`store_boot_report` 是落地，不是消费；这条顺序成立。
  3. 查 `kmain`。`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1861-1870` 在 Phase A 调用 `kernel_info.validate_requiring_sources()` 与 `check_boot_postconditions()`；此时页表与 DM 已建立，所以它确实是“消费后的复核门”，而不是首道门。`kmain` 不再调用 `store_kernel_info`（`:1879-1885` 的注释与调用点均如此）。
  4. **反例（发送侧）**：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:437-528` 的 `prepare_kernel_image_v1` 是三条 v1 发送腿在跳转前实际执行的函数。它先 `boot_validate_and_prepare`（只校验 `kern_size > 0`、两个基址页对齐、bump 区间已注册、页大小能力），随后直接用 `report.boot_modules` 与 `report.platform_sources` 组装 `BootWindows`（`:479-500`），再 `build_bootstrap_root_and_enable`（`:502`）建根并开分页。它不调用 `validate_before_tables`（全仓唯一调用点是 `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:228`，属于直核接收 `arch_boot_finish`），也不调用 `validate_requiring_sources` 或 `check_boot_postconditions`。因此，文档 `:1910` 的无条件句“校验在消费（建表/映射/分配）之前完成”若按字面读，对 v1 发送侧不成立；文档自己的 `:2711` 已经承认“v1 发送侧的窗口消费（`prepare_kernel_image_v1`）不在本门链内”。这是概述/设计章与证据章的措辞冲突，不是代码缺陷：发送侧的报告由它自己的装载/分类门产生（UEFI 的 `assert_modules_clear_of_kernel`/`assert_bootstrap_outside_memmap`、U-Boot 的 `audit_source_coverage`、直核的 `place_modules`），接收侧再 fail-closed。
  5. 跨节复述：`:104` 把 I4 明确限定为“接收侧”；`:1910` 的第一句没有这个限定；`:2711` 又把发送侧排除在门链之外。三处放在一起读，读者会得到两个不同的 I4 范围。
- 结果：非阻断（P2，跨节措辞不一致；按 §1.2 的“接收侧”定义，I4 成立，没有复现出内存破坏或半提交消费）。
- 证据：文档 `:104`、`:1910`、`:2711`；源码 `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:228`、`:437-528`、`:663-751`、`:300-350`、`:1861-1870`；`bootface_common.rs:534-556`。
- 若我错了：如果 `enter_kernel_image_v1` 的契约是“调用者已经在自己的腿内跑过结构校验”，那么 §3.7 的第一句应当写成“接收侧校验在消费之前完成；发送侧的建根只依赖装载/分类门已经保证的报告”，或者按 §1.2 的写法加“接收侧”。这只需要一句限定，不要求改代码。如果评审认为 `:2711` 的披露已经足够让读者不误读，那么这条降为观察；我仍建议改，因为 §3.7 是该章的 I3/I4 总起句，读者通常不会带着 §5.1 的脚注读它。

## CLAIM 05

- 主张：五条生产腿在数据形状上收敛（两张内存表加统一的 `BootReport`/`KernelInfo` 形状），但不合并引导形态层；bootstrap 区间在 Rust 侧没有对应物；bump 池整段扣除，arena 的最后一次读在解码（文档 `:1086-1087`、`:1222-1228`、`:1448-1452`、`:1543-1585`、`:2605-2609`）。
- 反证尝试：
  1. 逐腿找不同形状。UEFI：`ebs_pair::pair_and_classify` 把 conventional/ReservedMapped 分进两表，source 由分类器直写 wire（`/home/xzhao/github/minix-rs/os/boot-shim/src/ebs_pair.rs:172-250`；`ebs_map.rs:849-851` 的 kind 映射）；U-Boot：`prepare_boot` 用 DTB collector 产出两表与 40 槽 source（`/home/xzhao/github/minix-rs/os/boot-shim/src/opensbi_helpers.rs:256-335`）；两个直核面：`bootface_common.rs:531-556` 用同一 collector，aarch64 的 `fw_end = 0`、riscv64 的 `fw_end = KERN_PHYS_BASE`（`bootface_a64.rs:149-153`、`bootface.rs:143-147`）。三条 v1 腿与两个直核腿都构造 `BootReport`（UEFI `storage.report`、U-Boot `result.report`、直核 `storage.report`），没有第三条注入通道。
  2. 找“内核不依赖引导层”的反例。`/home/xzhao/github/minix-rs/os/kernel/Cargo.toml` 的依赖表没有 `boot-shim`，也没有 `uefi`；`os/kernel-image/Cargo.toml` 只在 riscv64/aarch64 target 下加 `boot-shim`（opensbi feature）与 `fdt`。文档 `:1691-1717` 的图表与四份依赖表一致。
  3. 找“分页后可回收内核段”。三个链接脚本（`os/kernel-image/x86_64.ld:26-119`、`aarch64.ld:38-120`、`riscv64.ld:41-128`）都把 `.text/.rodata/.data/.bss` 放在高半 VMA、用 `AT()` 指到低 LMA；没有 bootface 专用输出节；`__bss_end` 与 `__image_end` 的差别只是 `.bss` 末尾的 2 MiB 对齐收口，且 `kernel_boot_stack` 在收口之前。`bootface` 代码虽然只在分页前执行，但在 `.text` 里，没有独立的链接边界；文档 `:1452` 的判断成立。QEMU 测试内核的 VMA=LMA 平铺被文档明确排除。
  4. 找 bump/arena 的归属反例。`/home/xzhao/github/minix-rs/os/kernel/src/vm_handoff.rs:741-772` 的 `for_each_live_bootstrap` 对 `boot_bump` 推 `(base, end - base)` 整段；`classify` 用这份记录从 `FreeRangeSet` 里 `cut`（`:452-510`）。U-Boot 的 arena 由 `bump_alloc_pages(ARENA_CAP / 4096)` 在 1024 页页表子区间与模块落点之后分配（`opensbi_helpers.rs:417-421`），因此不在整段扣除的子区间内；解码是 arena 的最后一次读，VM free list 在 `build_vm_handoff` 才发布，`/home/xzhao/github/minix-rs/os/kernel/src/vm_handoff.rs:1088` 的 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 用整块 poison 钉住这条时序。没有找到“解码之后还读 arena”的路径。
  5. 跨节复述：`:95`（两个“五”的成员不同）、`:1900-1907`（换算表）、`:2704`（5.1 的 I3 计数）、`:2731-2741`（5.4 五条门链）口径一致；`:1154` 的“只验证失败路径”与 `:2710` 的“不证明 `kmain`”也一致。
- 结果：反证失败（主张成立）。附一条非阻断观察：UEFI 腿的 `BootReport` 对 `source_records` 不是完整视图（storage 切片为空，wire 才是权威；`boot_storage.rs:12-30` 与 `:290-318` 的注释都说明了这一点），所以“五条腿都只构造 `BootReport`”只应理解为“形状收敛”，不能理解为“报告携带了全部载荷”。文档 `:1912` 与 `:2427` 已披露，建议在“收敛点”那一句加上“形状收敛，不保证发送侧视图完整”。
- 证据：上面的 `ebs_pair.rs`、`opensbi_helpers.rs`、`bootface_common.rs`、`vm_handoff.rs`、三个 `.ld`、两份 `Cargo.toml`；文档 `:1086-1087`、`:1222-1228`、`:1448-1452`、`:1543-1585`、`:2605-2609`。
- 若我错了：如果某条腿把 `source_records` 同时放进 `BootStorage` 和 wire，就会出现两份权威；当前 UEFI 路径刻意让 storage 的 source 切片为空，只有 wire 一份。若未来改成两份，必须同步改 `validate_requiring_sources` 的输入选择，否则“wire 唯一权威”的合同会被打破。

## CLAIM 06

- 主张：第 2 章的 C 侧判据成立：`alloc_lowest` 把长度传给 `cut_memmap` 的结束地址参数、当前调用序下切除不发生（第三行常态）；`overlaps` 是半开区间且相切放行，ARM32 的闭区间差异在当前镜像尺寸下无害；`mb_set_param` 的“已存在先删”只对首条记录可靠；`pre_init` 入口的栈对齐不满足 SysV（文档 `:722-747`、`:816-882`、`:430-441`、`:291-297`、`:987`）。
- 反证尝试：
  1. `alloc_lowest`：`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/pg_utils.c:65-84` 确实调用 `cut_memmap(cbi, lowest, len)`，而 `cut_memmap` 的签名是 `(start, end)`（同文件 `:32-62`）。`alloc_lowest` 在 `arch/i386/` 下只有定义、原型与 `arch_smp.c:76` 一个调用（`grep -n "alloc_lowest" minix3/minix/kernel/arch/i386/` 三次命中）；`copy_trampoline` 的校验只查 `trampoline_base + tramp_size < (1 << 20)`（`arch_smp.c:76-79`）。`arch_init` 的两条 `cut_memmap` 在 `arch_system.c:277-278`，`BIOS_MEM_END = 0x4FF`、`BASE_MEM_TOP = 0x90000`、`UPPER_MEM_END = 0xFFFFF`，取整后确实把含 0 的低段切掉。四个前提成立时 `len = roundup(tramp_size) = 0x1000 <= lowest`，落第三行。`CONFIG_MAX_CPUS` 当前为 1（`minix3/minix/kernel/kernel.h:8`），GDT 7×8 = 56 B，trampoline 总量远小于 4096；前提三成立。
  2. `overlaps`：`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/pre_init.c:77-92` 的 `OVERLAP` 第二项是 `mod2->mod_end-1`；文档 `:873-880` 的相切反例按该宏展开成立，去掉 `-1` 会在 `[0x100000,0x200000)` 与 `[0x200000,0x300000)` 上误报。`get_parameters` 的循环对每条记录调用一次 `overlaps` 再 `cut_memmap`（同文件 `:201-214`），与文档 `:852-862` 一致。
  3. ARM32：`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/earm/pre_init.c:167-177` 的 `INRANGE` 上界是 `<= thismod->mod_end`（不是宏形参 `mod`），`OVERLAP` 第二项没有 `-1`；`:182-184` 的 `MB_MODS_BASE = 0x82000000`、`MB_MODS_ALIGN = 0x00800000`；`earm/kernel.lds:4` 的 `_kern_phys_base = 0x80200000`，间隔 30 MiB；`grep "ASSERT" minix3/minix/kernel/arch/earm/kernel.lds` 无命中，所以“当前镜像远小于 30 MiB”确实是构建事实而非结构保证。文档 `:975-987` 的表述成立。
  4. `mb_set_param`：`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/pre_init.c:33-59` 的扫描是 `while (*p++) ; p++;`；对 `A=1\0B=2\0\0`，越过第一条 NUL 后 `p` 落在 `B`，独立 `p++` 再把它推到 `=`，因此从第二条起匹配从第二个字符开始，`B=` 永远匹配不上。文档 `:430-438` 的字节图与结论成立。
  5. 栈对齐：`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/head.S:69-96` 先 `push $0; popf`（净零），再 `push $0`、`push %ebx`、`push %eax`，最后 `call pre_init`；从 4096 对齐的 `load_stack_start` 起算，`call` 前 `%esp = top-12 ≡ 4`，入口 `%esp = top-16 ≡ 0 (mod 16)`；SysV 要求入口 `%esp ≡ 12 (mod 16)`。文档 `:288-297` 的推导成立，且它明确写了“没有保存反汇编证据、只作风险提示”，没有把未证事实写成结论。
  6. 跨节复述：`:174`（2.0 失败对照）、`:263-265`（2.1 断言行为）、`:722-749`（2.4.2.3）、`:882`（2.5 的 `overlaps` 守内容不守账）、`:1001-1044`（2.8 调用树）对同一条 C 断言的描述一致；没有发现“概述说一套、实现说另一套”。
- 结果：反证失败（四条 C 判据均成立）。
- 证据：`pg_utils.c:65-84`、`arch_smp.c:76-79`、`arch_system.c:277-278`、`pre_init.c:33-59` 与 `:201-214` 与 `:77-92`、`earm/pre_init.c:167-184`、`earm/kernel.lds:4`、`head.S:69-96`；文档 `:288-297`、`:430-441`、`:722-747`、`:816-882`、`:975-987`。
- 若我错了：若 `CONFIG_MAX_CPUS` 被改到使 `roundup(tramp_size)` 超过 4096（按当前式子，需要把 trampoline + IDT + GDT 推过 4 KiB），`alloc_lowest` 的 `len` 会大于最低基址 0x1000，前提三失效，“第三行常态”就要按新调用序重推。文档 `:742` 已经写了这条重算规则；这不是当前缺陷，但不能把“第三行常态”当成结构保证。

## 总体结论

- 六条核心主张里，五条（I1、I2、I3、五腿收敛、C 基线）在我构造的反例下成立；I4 按文档 `:104` 的“接收侧”定义成立，但 §3.7 的第一句把范围写成无条件的“校验在消费（建表/映射/分配）之前完成”，与 v1 发送侧 `prepare_kernel_image_v1` 的实际执行序（只做标量校验就建根开分页）不一致，而同章 `:2711` 又承认发送侧不在门链内。这条是 P2 级跨节措辞不一致，不是代码缺陷，建议在 §3.7 加“接收侧”限定或补一句发送侧的分工。
- 两条附带的非阻断观察：(1) `land_and_store` 的“一次性”守卫是 check-then-write，依赖 boot 单线程前提，建议在 §4.5 写明；(2) UEFI 腿的 `BootReport` 对 `source_records` 是不完整视图，“五条腿都只构造 `BootReport`”只应理解为形状收敛。
- 我按要求跑了两个宿主读数以核对 §5.2：`cargo test -p minix-boot --lib` 为 189 passed / 0 failed；`cargo test -p minix-kernel --features mock --lib` 为 866 passed / 0 failed / 8 ignored，与文档 §5.2 一致。真机门链没有在本环境重跑，本文不引用它们的通过状态作为主张成立的证据。
