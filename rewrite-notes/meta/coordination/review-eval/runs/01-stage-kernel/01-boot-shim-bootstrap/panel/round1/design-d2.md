# D01 设计差异审查（P-DESIGN D2）

- 审查者：只读，未修改仓库。
- 输入：`panel/round1/design-d1.md` 的 DECISION-01..08；`os/libs/minix-boot/src/handoff_v1.rs`、`handoff_v1_write.rs`（写路径取证）、`boot_storage.rs`、`kernel_info.rs`；`os/boot-shim/src/lib.rs`；文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.2/§3.4。

## DIFF-TABLE

| 维度 | 现有实现 | D1 主张 | 判定 |
|---|---|---|---|
| **A. wire 与 arena** | 类型是 `#[repr(C)]`（`handoff_v1.rs:287-315` 的 `BootHandoffV1`、`:38-100` 的四个 wire 元素类型），但**实际编码已经是手工小端逐字段写**：`handoff_v1_write.rs:73-78` 的 `put_u32/put_u64` 用 `to_le_bytes` 写字节，`:161-175` 逐字段落信封，`:316-332` 逐字段落 72 字节 source 记录。`repr(C)` 只承担布局闸门：`offset_of!(BootHandoffV1, …)`（`handoff_v1.rs:321-343`）供收发两侧取偏移，`size_of/offset_of` 断言钉死 128 字节信封与各元素尺寸（`:396-414`）。分区的**地址**由编译期常量串算（`WIRE_MEMMAP_OFFSET` 等，`:372-385`），信封里只有六个 `*_byte_len`（`:309-314`），没有 (kind/offset/len) 段目录 | 手工小端 + **段目录**（kind/offset/len，8 字节对齐），明确拒绝 `repr(C)` 直写（DECISION-06） | **大体已是同一件事，真正的差异是段目录。**「`repr(C)` 直写」在本仓不存在，D1 拒绝的候选 B 是错靶：布局闸门用 `offset_of!` + 尺寸断言，不是 C 的 `mm_size + 4` 式结构即协议。**真实翻译残留**：六个并列 `*_byte_len` 字段加固定偏移（新增一个分区要改三处：偏移链、信封字段、收侧解码），以及 `WireSourceRecord` 的 `a`/`b`/`cut` 三字段复用承载 origin 变体载荷（`handoff_v1.rs:82-99`，靠 tag 解释同名字段）。**合理承载**：固定宽度标量、`end_hi`/`end_lo` 表示 128 位独占端（`:80-89`）、`WireBootModule` 的 16 字节名字槽（`:50-56`，协议上界而非内核存储宽度）、`len = 0` 的平台源旧生产者兼容（`:58-68`）。段目录属于可改可不改的承载选择，一旦改就是 arena 字节形态变更，必须升 `BOOT_HANDOFF_V1_VERSION`（现为 10，`handoff_v1.rs:278`） |
| **B. 所有权模型** | 所有权链是**四层**：`BootStorage`（存储 + 长度 + 五个 `*_committed` 布尔，`boot_storage.rs:86-106`）→ 每个家族一个 `fill_*(closure)` 在 `&mut self` 期间写入并提交计数（`:141-160` 等）→ `BootReport<'_>`（纯借用视图，`:292-308`）→ `build_kernel_info` 把九个字段逐个搬进 `KernelInfo`（`boot-shim/src/lib.rs:111-127`）。跨镜像发送侧另有一层 `BootStorageCell`：`UnsafeCell` + `AtomicBool` 一次性 `take()`（`:137-167`）与 `report_static()` 把普通借用洗成 `'static`（`:169-188`，SAFETY 注释只靠约定） | 每个部件按值构造，`BootPayload::new` 一次收齐，move 进 `BootCell`，对外只给 `&'static BootPayload`（DECISION-01、DESIGN-SUMMARY 第 3 条） | **差异是「多次提交 + 两份同形结构」对「一次构造」。** 可删层两处：① `BootReport` 与 `KernelInfo` 字段逐一同名同型（`boot_storage.rs:292-308` 对 `kernel_info.rs:20-140`，只差 `authority`），`build_kernel_info` 是纯字段搬运，`KernelInfo` 再包一层没有增加任何语义；② 五个 `*_committed` 布尔只为「第二次 fill 必须 panic」而存在（`boot_storage.rs:145,167,182` 等），是 D1 DECISION-01 候选 B 所说的「填到一半合法」的残留。**非法状态**：`BootStorage::new()` 未填任何家族时 `report()` 直接给出全空切片（`:256-290`），与「合法空载荷」不可区分；`BootStorageCell::take()` 的 `&mut` 与 `report_static()` 的 `&'static` 在类型上可同时存活，只靠注释约定不重叠。合理承载：`fill_*` 的「写入与提交一步」确实消除了脏标记，`UnsafeCell` 在单核关中断引导期是合理的静态可变点 |
| **C. `KernelInfo` 形状** | 字段是 `&'static [T]` 切片（`kernel_info.rs:30-140`），`Clone + Copy`，长度自带，无偏移；消费方按切片读。派生量走访问器（`kern_stack_top()`，`:193-206`）。落地区容量在别处：`WIRE_MEMMAP_CAP = 128`、`WIRE_RESERVED_CAP = 256`、`WIRE_MODULE_CAP = 12`、`WIRE_PLATFORM_CAP = 4`、`WIRE_PARAM_CAP = 1024`、`WIRE_SOURCES_CAP = 296`（`handoff_v1.rs:356-367`），文档 §3.2 说明落地区 128 槽与 VM 交接 64 条上限 | 固定容量数组按值内嵌，访问器返回切片，无指针无偏移（DECISION-01 候选 C 的拒绝理由） | **同形不同承载，不是语义差异。** 按值数组只把「数据在落地区、结构里放切片」变成「结构即落地区」，消费侧拿到的仍是同名切片，**不改外部行为**；但它会把 `KernelInfo` 从约十个字长成约 24 KiB 的值类型（16×128 + 16×256 + 32×12 + 16×4 + 1024 + 72×296），凡是 `Copy` 现有结构的调用点（`kernel_info.rs:19`）都会多一次整块拷贝，属实现代价不是语义。**会改外部行为的点有三个，必须标 `[ARCH]`**：① DECISION-09 把容量改成 `FREE_CAP = FW_SPAN_CAP + OCCUPANT_CAP`（D1 取 `FW_SPAN_CAP = 64`），低于现有 128 空槽与 256 保留槽，会拒绝今天能过落地区的固件内存图；② DECISION-07 把内核位置的权威改到接收方链接期常量并逐项比对（现实现无此比对，接收方直接采信发送方 ELF 解析出的三值：`os/kernel/src/dm_coverage.rs:336`、`arch/riscv64/higher_half.rs:106-110`、`syscall.rs:2602,3358`），会拒绝今天能启动的镜像并改变 Direct Map 覆盖基准；③ DESIGN-SUMMARY 第 1 条称「保留区间是由前三者算出的视图，不存储」，但 `reserved_regions` 是独立的固件分类结果（文档 §3.2 的 `ReservedMapped`，含 LOADER_DATA/BOOT_SERVICES/ACPI 与 DTB `/reserved-memory`），无法由空闲区间与占用者推出，改为视图等于删掉这一类内存 |

## FINDINGS

## FINDING D2-F1 `[DEL]` `BootReport` 与 `build_kernel_info` 是纯搬运层，`*_committed` 是状态机残留
- 位置：`os/libs/minix-boot/src/boot_storage.rs:86-106,141-160,256-308`；`os/boot-shim/src/lib.rs:111-127,137-188`；`os/libs/minix-boot/src/kernel_info.rs:20-140`
- 严重度：P1（不是行为缺陷；是 D1 指出的「两次提交」在实现里的真实投影，且带一处只能靠约定维持的 `'static` 洗白）
- 问题：`BootReport` 的九个字段与 `KernelInfo` 逐一同名同型，`build_kernel_info` 无逻辑；五个 `*_committed` 布尔只服务于「重复 fill 必须 panic」；`report_static()` 用 `UnsafeCell` 把借用洗成 `'static`，与 `take()` 的 `&mut` 没有任何类型级互斥。D1 的「一次构造 + move」如果落地，这三样都应消失。
- 最小修复：不引入新的 wire 变化，先把发送侧收敛为一个报告值类型（`BootReport` 与 `KernelInfo` 留一个，另一个用类型别名或 `From`），把 `fill_*` 的「写数组 + 返回计数」改为「构造该家族的值并一次提交」，让 `*_committed` 与 `report_static` 的洗白同时退场。真正把九个字段按值内嵌留给独立裁决，因为它牵动 24 KiB 的 `Copy` 代价与落地区布局。

## FINDING D2-F2 `[MODERN]` D1 的段目录相对现有「固定偏移 + 六个 `byte_len`」是真实改进，但被「拒绝 `repr(C)`」的错靶包装
- 位置：`os/libs/minix-boot/src/handoff_v1.rs:309-314,372-385`；`os/libs/minix-boot/src/handoff_v1_write.rs:73-78,161-175,316-332`
- 严重度：P2
- 问题：写路径已经是手工小端，`repr(C)` 只做布局闸门；D1 DECISION-06 把现状当成「结构直写」来拒绝，结论对但论证错靶，会给后续审查留下「现有 wire 是 C 式结构直写」的错误事实印象。真正的可改进点是「每加一个分区要同时改偏移链、信封字段、收侧解码」与 `WireSourceRecord` 的 `a`/`b`/`cut` 复用。
- 最小修复：文档侧把 D1-06 记为「已满足度约九成，差异仅在段目录」；代码侧不为此改 wire。若确要段目录，单独开 wire v11 提案，连带处理 `a/b/cut` 复用，不要在 D01 范围内顺手改。

## FINDING D2-F3 `[ARCH]` D1 的位置权威与「保留区间是视图」会改变外部行为
- 位置：D1 `design-d1.md:66-71`（DECISION-07）、`:24-29`（DECISION-01 候选 A/C 与 summary 第 1 条）；实现侧 `os/kernel/src/dm_coverage.rs:336`、`os/kernel/src/arch/riscv64/higher_half.rs:106-110`、`os/kernel/src/syscall.rs:2602,3358`、`os/libs/minix-boot/src/kernel_info.rs:55-68`
- 严重度：P1
- 问题：DECISION-07 让接收方链接期常量做权威、发送方声明只作交叉校验，等于新增一类启动期拒绝（今天接收方直接采信发送方的 ELF 解析结果，且 `kern_phys_base` 参与 Direct Map 覆盖与 riscv64 平移计算）。DESIGN-SUMMARY 第 1 条「保留区间是算出的视图」与 `ReservedMapped` 的独立分类矛盾：保留表包含固件保留但可经 Direct Map 访问的区间，不能由空闲区间与占用者重建。两者都不是内部重构。
- 最小修复：D1 若采纳这两点，须在文档与设计两处标 `[ARCH: …]` 并写明拒绝新输入的判据与后果；「保留区间是视图」应改为「保留区间是独立输入表，来源为固件分类」，否则不得进入实现。DECISION-09 的容量公式同理（`FW_SPAN_CAP = 64` 低于现有 128 槽），若采纳须标注它会收紧今天可接受的固件内存图。

## ARCH-PROPOSAL

不新开架构变更。若作者批准 D1 的 DECISION-07/09 与「保留区间视图」，建议合成一条 `[ARCH: boot-handoff-authority]`：同时改「位置权威」「容量收紧」「保留表来源」三处，因为它们共享同一条外部行为——哪些固件内存图与镜像被接受——分开标注会让其中一处被当成普通重构。

## VERDICT

- A：现有 wire **不是** D1 假设的 `repr(C)` 直写；已经是手工小端 + `offset_of!` 布局闸门，唯一实质差异是段目录，属可选承载升级。真实残留是六字段 `byte_len` 与 `a/b/cut` 复用。
- B：所有权模型确有可删层（`BootReport`/`build_kernel_info`/五个 `*_committed`/`report_static` 的 `'static` 洗白），D1 的「一次构造 + move」方向与实现证据一致，可先行落地而不牵动 wire。
- C：按值数组本身不改外部行为，但 D1 的容量公式、位置权威、保留区间视图三项会改，必须 `[ARCH]`；未标不得实现。
- 结论：**D1 可用于收敛方向，但 DECISION-06 的现状描述需更正，DECISION-07/09 与 summary 第 1 条需补 `[ARCH]` 标注或被作者否决。** 无 P0。

## SUBTRACTION-TABLE（V3.6）

| 加法提案 | 减法替代 | 减法拦不住的理由 | 保障强度 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| D1 的段目录 | 保留现有固定偏移 + 六个 byte_len | 段目录只是扩展性；现协议已能表达全部载荷，新增分区才是真实需求 | 同等 | 0 | 否 |
