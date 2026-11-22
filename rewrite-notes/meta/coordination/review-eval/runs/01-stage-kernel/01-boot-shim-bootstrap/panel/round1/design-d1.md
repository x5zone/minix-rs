# D01 独立 Rust 设计（P-DESIGN 第一遍）

- 角色：独立设计者，只读 C 源与文档第 1、2 章；未读现有 Rust 实现、第 3/4 章、历史评审、设计快照、作者解释。
- 输入：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 1–2 章；`minix3/` 下本文档锚点所指的 C 源。
- 约束依据：`AGENTS.md` 的 Key Constraints（no_std、错误映射 errno、硬件 trait 抽象、Kernel SMP+BKL、抽象语义不描述机制）。

## DESIGN-SUMMARY

**C 侧真正要保证的语义（一段话）。** 引导器拥有它写下的三份外来数据——`multiboot_info`、模块描述符数组、内存图（细表或粗档二选一）——它们的指针只在内核仍与物理地址一一对应时有效；`get_parameters` 在同一个函数里把这些外来物变成内核自己的记录：整份复制 `mbi`（`minix3/minix/kernel/arch/i386/pre_init.c:108`）、复制模块数组（`:170`）、把可用内存条目改写成内核账本（`:173-188`）、把命令行拆成 `param_buf` 文本（`:124-146`），此后再不 dereference 引导器的指针。状态变化只有三类：可用区间入账（首个空槽、起点上取整、长度下取整、4 GiB 截断、`mem_high_phys` 单调抬升，`pg_utils.c:86-121`）；占用区间出账（内核与每个模块先做半开区间重叠检查，相切放行，命中即 panic，然后把自己从账上切掉，必要时一分为二，`pre_init.c:194-214`、`pg_utils.c:32-63`）；以及后续的取页（最高下标尾页，`pg_utils.c:138-160`）与取低地址（`alloc_lowest`）。失败路径分两档：结构性错误当场断言或 panic（魔术值 `pre_init.c:219`、模块数 `:167-169`、重叠 `:206-207`、无槽位 `pg_utils.c:120`、无内存 `:159`），唯一静默降级是参数缓冲写不下（`mb_set_param` 返回 -1 而三处调用都不看，`pre_init.c:144,149,150`）。并发边界不是锁：整段发生在单 CPU、中断已关、调度器未启动之前（`head.S:68-77`）；唯一的跨阶段所有权标记是全局 `kernel_may_alloc`——引导期记账者持有它，切到用户态前两行置 0（`main.c:142`、`:105`），把内存账的所有权交给 VM，此后再改账即断言失败。下游生命周期是三次单向移交：`param_buf` 被 PM 经 `GET_MONPARAMS` 整块取走（`do_getinfo.c:143-145`），`kmain` 再次复制 `kinfo`（`main.c:128`）并用模块描述符建立启动进程（`main.c:179-184`），内存账前 16 条被 VM 取走建自己的位图（`vm/utility.c:58-60`、`const.h:98`），其余条目静默丢弃。

**由此得到的 Rust 设计骨架（摘要，细节见下）。**

1. 语义对象只有一个：**已校验的引导载荷** `BootPayload`。它是纯值：固定容量数组、无指针、无 `&str`、无 `usize`，字段只有内核位置、模块描述符、可用区间（每条带来源）、参数缓冲与可选的 shim 区间；保留区间是由前三者算出的视图，不存储。
2. **身份**用类型表达阶段，不用布尔：`RawPayload`（外来、未校验）→ `ValidatedPayload`（所有 I1–I4 检查已过）→ `BootPayload`（可被消费）。消费侧 API 只接受后两者，先消费后校验在类型上不可写。最小方案把前两态折进 `BootPayload` 的私有校验构造器（见「最小方案」第 5 条）。
3. **所有权**用 move 表达一次提交，不用 `fill_*` 状态机：每个部件（`MemoryLedger`、`ModuleTable`、`ParamBuf`、`KernelPlacement`）先各自按值构造，`BootPayload::new` 一次收齐；第二次提交在类型上不存在。内核入口把 `BootPayload` 放进一个只初始化一次的 `BootCell`，对外只给 `&'static BootPayload`。
4. **来源**是区间的一个字段，不是并行的表：`SourcedSpan { span: PhysSpan, source: Source }`。`Source` 是带载荷的小枚举（固件细表第 n 条 / 粗档 / 平台合成 / 模块第 n 个 / 内核镜像 / 引导器自身）。于是「每个可用字节都有来源」由构造保证，不需要运行期审计函数。
5. **内存账**建模成「一组互不相交、页对齐、非空的可用区间」；`reserve(span)` 是区间减法，`release(span)` 是并集（仅在来源相同且相邻时合并）。占用者（内核、模块、shim）由各自的类型字段表示，`occupants()` 是计算视图，不另存一张保留表。C 的「长度为 0 即空槽」「下标序不等于地址序」都是定长存储的实现痕迹，不进入模型。
6. **平台边界**是三个 trait：`BootPlatform`（提供内存图、命令行、内核装载位置）、`ModuleSource`（按名字读出模块字节，调用方给目标缓冲）、`DiagSink`（早期输出）。core 类型不出现 UEFI、DTB、Multiboot、ELF 任何一个词。
7. **五条生产腿**只在「字节从哪来」上不同；三条跨镜像腿与两条同镜像腿共用同一份语义模型，但传输机制分开：跨镜像走 `Wire` 编解码，同镜像走按值 move。不为了「看起来统一」让同镜像腿也绕一遍字节。
8. **错误**分两类：`BootFault`（致命，带 `errno()` 映射与诊断串，交给 panic 路径）与 `BootError`（可拒绝、可记录，例如参数缓冲溢出、模块名不匹配；在按名定位的腿上可升级为 `BootFault`）。不新造 errno 值；映射只用 Minix3 已有的 `EINVAL`/`ENOMEM`/`E2BIG`/`EFAULT`/`ENOENT`（`minix3/sys/sys/errno.h:43-70`）。
9. **并发**不引入锁：引导期是单核、关中断、调度器未启动；用线性能力（`BootAuthority` 零尺寸 token）保证账本只有一个可变借用者。账本交接给 VM 时把可变借用收走，不是把全局整数置 0。
10. **host 测试**覆盖纯逻辑：账本的入账/出账代数性质、来源全覆盖、最坏容量不溢出、wire 往返与畸形输入拒绝、参数格式逐字节对账、类型层「未校验不可消费」的编译失败测试（`trybuild` 式或等价 doctest）。

## DECISION-01：语义对象是一个按值载荷，不是「发送视图 + 接收副本 + 落地区」

- 候选 A（采纳）：`BootPayload` 是一个纯值类型，固定数组直接内嵌；通道只负责把这个值搬到对面（move）或编码成 `Wire` 再解回来（copy）。消费侧拿到的永远是同一个类型。
- 候选 B（拒绝）：像 C 的 `kinfo` 一样定义一个全局可变记录，各填充函数按字段写它。拒绝理由：C 这么做是因为没有别的选择（同一个二进制、`.bss` 零初值、`pre_init` 直接返回 `&kinfo`，`pre_init.c:20,237`）；它把「填到一半」变成合法状态，I3 只能用「第二次 fill panic」来兜（文档 §1.2 I3）。Rust 的类型系统可以在构造期就排除半成品，没有理由继承这个状态机。
- 候选 C（拒绝）：`KernelInfo` 只存偏移，切片在每次访问时从落地区重建。拒绝理由：偏移是「存储布局」这一机制的泄漏；同一个语义对象在三条跨镜像腿与两个直核腿上会有不同的偏移基准，正好违反「抽象语义、不描述机制」。按值数组 + 访问器返回切片，既无指针也无需偏移。
- 依据：C 的 `kinfo_t` 字段清单（`minix3/minix/include/minix/param.h:14-47`）本身是一个值；`pre_init` 末尾 `return &kinfo`（`pre_init.c:237`）与 `kmain` 开头再次 `memcpy`（`main.c:128`）说明 C 也在做「值的一次提交」，只是用了同一个全局地址。Rust 把这个语义显式化。

## DECISION-02：阶段用类型表达（Raw → Validated → Payload），消费 API 只收后两态

- 候选 A（采纳）：`RawPayload<'arena>` 只借用 wire 字节或外来描述符，不实现任何消费方法；`validate()` 返回 `Result<ValidatedPayload, BootFault>`；`ValidatedPayload::freeze()` 产出 `BootPayload`。所有建表、映射、分配入口的参数类型是 `&ValidatedPayload` 或 `&BootPayload`。
- 候选 B（拒绝）：单一结构 + `validated: bool` 字段 + 运行期 `assert!(validated)`。拒绝理由：布尔可以被复制、被遗忘、被在错误的分支里置真；这正是 I4「校验先于消费」要防的失败面（`pre_init.c` 里检查与切除交错，`overlaps` 与 `cut_memmap` 在同一循环，`pre_init.c:201-214`——C 的顺序只对当前调用点成立，不是结构保证）。类型状态把这条不变量从「调用点纪律」升级为「编译器检查」。
- 候选 C（拒绝）：校验函数返回 `Result<(), Fault>`，调用方拿到 `Ok` 后继续用原对象。拒绝理由：`Ok` 与被校验对象之间没有所有权关系，任何持有原对象的人都能绕过；且无法表达「校验时已经做了规范化（排序、合并、取整）」这一事实。
- 依据：I4 的定义（文档 §1.2）与 C 的交错结构（`pre_init.c:201-214`）说明「校验与消费分离」在 C 里不是结构保证；Rust 侧用类型把它变成结构保证。注意：这不改变外部行为，只改变非法输入的第一现场。

## DECISION-03：来源记录是区间的字段，不是并行的表

- 候选 A（采纳）：`SourcedSpan { span: PhysSpan, source: Source }`；`Source` 是 `#[repr(u8)]` 的小枚举加一个 `u16` 槽位号（固件第几条、模块第几个）。可用区间带来源；保留区间的来源由它的身份给出（内核位置、模块序号、shim），不重复存一份。I2 的 `audit_source_coverage` 退化为构造函数的后置条件，不需要运行期函数。
- 候选 B（拒绝）：`Span` 数组 + 平行 `SourceRecord` 数组，用下标关联。拒绝理由：两个数组的长度可以不一致，关联是隐式的；C 的 `memmap[]` 与 `module_list[]` 分开正是这种结构，代价是「这段内存是谁给的」只能靠数组外的知识补（例如「还 VM 镜像」要同时改 `module_list` 与 `memmap`，`protect.c:450-451`）。字段内嵌让「无来源区间」不可构造。
- 候选 C（拒绝）：按字节维护来源（每个 4 KiB 页一个标签表）。拒绝理由：容量爆炸且没有必要——所有来源天然是区间级的（固件条目、模块、内核镜像）；区间被切割时来源随子区间复制，语义已经足够。
- 依据：I2 的判据（文档 §1.2）要求每个投影字节有来源；C 里对应的信息散在 `memmap`（没有来源字段，`param.h:18`）、`module_list`（只有地址，`:17`）与链接期常量三处。把来源做成字段是让这条不变量可构造，而不是新增机制。

## DECISION-04：内存账用「区间集合 + 减法」，不复制定长槽位与空洞

- 候选 A（采纳）：`MemoryLedger` 持有 `[SourcedSpan; FREE_CAP]`，不变量是互不相交、非空、页对齐、按基址升序。`insert_free` 做并集（同源相邻合并），`reserve(span)` 做区间减法（可能把一个区间切成两段），`allocate_highest_page` 从最高地址取一页并记账。容量不足返回 `BootError::CapacityExceeded`，不 panic、不静默截断。
- 候选 B（拒绝）：照搬 C 的 `memmap[40]` + `mmap_size` + 「长度为零即空槽」+ 首个空槽回填。拒绝理由：`pg_utils.c:104-118` 的空槽语义要求每个读者自己跳过零长度项（`pg_alloc_page` 的 `if(!mmap->mm_length) continue;`，`:147`），`mmap_size` 是「最高占用下标加一」而不是条目数（`:110-111`），下标序不等于地址序（同一文件 `:145` 反向扫描）。这些是定长存储的实现痕迹，全部进入模型只会让每个消费者重复一遍防御代码。
- 候选 C（拒绝）：有序链表/动态分配。拒绝理由：引导期没有可用分配器（`pre_init` 分页前只有链接期静态空间），这是自举约束，不是偏好。
- 依据：C 的操作语义只有追加与切除两种（`pg_utils.c:86-121`、`:32-63`）；数组布局、空槽、`mmap_size` 都是承载方式。Rust 用固定容量数组 + 集合不变量保留语义、删掉痕迹。容量取值必须由「固件条目上限 + 占用者上限」推出来（见 DECISION-09），不能照抄 40。

## DECISION-05：取整方向由操作名承担，区间类型保证页对齐

- 候选 A（采纳）：两个不同的入口：`insert_free(raw_span)` 做「起点上取整、长度下取整」（只保证写入的条目自身页对齐）；`reserve(raw_span)` 做「起点下取整、终点上取整」（保证被拿走的页一页不少）。`PhysSpan::pages(base, len)` 只接受已经页对齐的输入；`SourcedSpan` 的构造后置条件检查页对齐与非空。
- 候选 B（拒绝）：统一一个 `align_span(span, direction)` 辅助函数，两个调用点各传方向。拒绝理由：C 的两个方向相反（`add_memmap` 的 `roundup(addr)/rounddown(len)`，`pg_utils.c:99-100`；`cut_memmap` 的 `start -= start%page`、`end += page - end%page`，`:37-40`），调用点只看名字无法判断方向；`alloc_lowest` 把长度当结束地址传给 `cut_memmap`（`:81`）正是这种接口形状的判例。让方向成为函数名的一部分，误用在类型上更难发生。
- 候选 C（拒绝）：要求所有输入先对齐，账本内部不做取整。拒绝理由：固件 E820 基址规范不保证页粒度，模块文件长度也不保证；拒绝非对齐输入会把 C 能启动的机器挡在门外。
- 依据：文档 §2.4.2.3 的两条取整推演；`pg_utils.c:37-40,99-100`。注意 `add_memmap` 的上取整可能让终点外移一页（文档 §2.4 已取证），Rust 侧用「只保证条目自身对齐」的同一语义，但把这条不精确性记录在 `insert_free` 的文档里，并把「原描述符内的字节数」与「入账字节数」的差记进 `AccountingResidue`，供来源审计使用。

## DECISION-06：wire 是手工小端编解码 + 段目录，不是 `#[repr(C)]` 结构直写

- 候选 A（采纳）：固定宽度小端标量、8 字节对齐的段目录（kind / offset / len）、所有偏移相对 arena 基址、枚举显式 `u8` 判别值、`usize`/`bool`/指针一律禁止。解码分两遍：先验信封与段表（边界、对齐、重叠、重复、必需段齐全），再逐段构造有类型的值。
- 候选 B（拒绝）：`#[repr(C)]` 结构体直接写到 arena。拒绝理由：`repr(C)` 只保证字段顺序，不保证无填充、不保证跨架构的 `u64`/`usize` 宽度、不保证枚举判别值；跨镜像交接的协议不能建立在编译器布局上。C 自己的 `struct multiboot_mmap` 就是反例：`mm_size` 不含自身，跳下一条要数 `mm_size + 4`（`minix3/sys/arch/i386/include/multiboot.h:208-217`），说明「结构体即协议」一旦跨编译器就会长出这种补丁。
- 候选 C（拒绝）：文本协议（key=value 或 JSON 风格）。拒绝理由：解析器更大、错误面更多、二进制模块区间与对齐无法自然表达；引导期只需要一个很小的封闭模式。
- 版本策略：单一 `WIRE_VERSION: u16`，精确匹配，未知版本报 `UnsupportedVersion`。拒绝「特性位协商」：现在只有一个发送方与一个接收方，协商是给不存在的第二版本付复杂度。依据：C 同镜像没有版本问题（`pre_init.c:108` 直接 `memcpy`），版本是跨镜像新增的必要机制，最小化即可。

## DECISION-07：内核位置的权威在接收方链接期常量，发送方的声明只作交叉校验

- 候选 A（采纳）：`KernelPlacement { phys_base, virt_base, size }` 随载荷传过来；接收方用自己的链接期常量（`_kern_phys_base`、`_kern_vir_base`、`_kern_size`）逐项比对，不一致即 `PlacementMismatch`（映射 `EFAULT`）。载荷里的三个数是「发送方声称它把镜像放在哪」的证据，不是事实来源。
- 候选 B（拒绝）：发送方权威，接收方按它建表。拒绝理由：C 从来不问引导器内核在哪——`kernbase`/`kernsize` 取自链接期符号（`pre_init.c:100-103`），`vir_kern_start`/`bootstrap_*` 也取自符号（`:111-116`）。让一个可能出错的装载器决定内核的物理区间，会把「装载错了」变成「按错误布局继续启动」。
- 候选 C（拒绝）：载荷完全不传位置，接收方只信自己的符号。拒绝理由：三条跨镜像腿里发送方是唯一知道装载落点的一方；没有这条声明，接收方无法在消费内存账之前发现「镜像没装到链接地址」——而这正是最该早停的一类错误。声明 + 交叉校验两头都要。
- 依据：D2「内核位置先于清单裁剪」（文档 §1.4/§2.8）与 C 的符号取址方式（`pre_init.c:100-116`）。Rust 把「位置从哪来」与「谁说了算」分开：来源是链接期，声明是对装载结果的断言。

## DECISION-08：模块的硬契约是「条数与顺序」，名字按腿的能力分级校验

- 候选 A（采纳）：`ModuleTable` 有序；硬门是 `count == expected_count`（来自内核启动镜像表的编译期长度），与 C 的 `NR_BOOT_MODULES != mi_mods_count → panic` 对齐（`main.c:160-162`）；模块到启动进程的映射按顺序（`main.c:179-184` 的 `module_list[i - NR_TASKS]`）。名字校验分腿：以名字定位模块的腿（U-Boot 文件表、直核固定表）名字不符即致命；以固件文件系统按顺序取件的腿名字只记诊断。
- 候选 B（拒绝）：照 C 只查条数，名字完全不看。拒绝理由：C 不看名字是因为 `mmo_string` 指向引导器内存且内核从不读（`pre_init.c:170-171` 只复制结构体，`multiboot.h:224-229` 的 `mmo_string` 是 `char *`）；Rust 侧名字已经由发送方付了复制成本，丢掉它等于放弃一次免费的一致性检查。但把它升成所有腿的硬门会拒绝「改了文件名仍能按序启动」的合法配置，所以按腿分级。
- 候选 C（拒绝）：按名字匹配到启动进程，不要求顺序。拒绝理由：C 的顺序映射是外部行为（`main.c` 用 `i - NR_TASKS` 取模块），改成名字映射会改变「同名重复/缺名」时的行为；顺序是契约，名字是证据。
- 依据：`main.c:160-162`、`:179-184`；`param.h:17` 的 `module_list` 是按槽位复制的数组（`pre_init.c:170-171`），顺序即身份。容量上 `MULTIBOOT_MAX_MODS = 20` 且内核占最后一槽（见 `multiboot.h:244`、`pre_init.c:194-195`），所以真实模块上限是 19；Rust 把内核从模块数组里拿出来后，`MODULE_CAP` 直接取编译期期望条数（并 `const_assert!(<= 19)`），不需要「预留一槽」这个概念。

## DECISION-09：容量由「固件条目上限 + 占用者上限」推导，溢出是类型化错误

- 候选 A（采纳）：`FREE_CAP = FW_SPAN_CAP + OCCUPANT_CAP`，其中 `OCCUPANT_CAP = 1（内核） + MODULE_CAP + SHIM_SEGMENT_CAP`；`FW_SPAN_CAP = 64`（典型 E820 可用条目远小于此；规范化后通常个位数）。`const_assert!` 在编译期验算，运行期插入超限返回 `CapacityExceeded`（映射 `ENOMEM`），不 panic、不截断。理由：每次 `reserve` 最多把一条可用区间切成两段，净增一条；最坏情况就是「初始条目 + 每个占用者各一刀」，这个上界可以编译期证明。
- 候选 B（拒绝）：照抄 `MAXMEMMAP = 40`（`param.h:13`）。拒绝理由：40 是 C 的数组槽数，既没有按最坏情况推导，也不得不靠 `assert(cbi->mmap_size < MAXMEMMAP)` 粗检与 `panic("no available memmap slot")` 收场（`pg_utils.c:97,120`）；而且它同时承担「空闲清单容量」与「VM 前 16 条」两个不相干的约束（`const.h:98`、`vm/utility.c:58-60`）。
- 候选 C（拒绝）：动态增长（`Vec`/链表）。拒绝理由：分页前的引导期没有堆分配器，`pre_init` 用的是链接期静态空间（`pre_init.c:20,105,125-126`）；这是自举约束。
- 依据：`pg_utils.c:104-118` 的「首个空槽」与 `:110-111` 的边界更新证明 C 的容量管理是纯承载细节；语义只需要「能放下这份账」。把上界写成公式，溢出就成了「配置与平台不符」的显式错误，而不是某个机器上的 panic。

## DECISION-10：可用区间做排序 + 同源相邻合并；重叠条目合并而非照抄

- 候选 A（采纳）：`MemoryLedger::from_firmware` 先按基址排序，再把同来源类别且相邻/重叠的区间合并；不同来源重叠时按「固件细表序号小者优先」定来源，并在 `AccountingResidue` 记一条重叠告警。之后所有操作维护「升序、互不相交」不变量。
- 候选 B（拒绝）：照 C 原样保留条目顺序、不合并。拒绝理由：C 不排序（`add_memmap` 从下标 0 找空槽，`pg_utils.c:104`），于是 `pg_alloc_page` 的「最高下标」不等于「最高地址」（`:145`）；VM 侧按条目逐条 `mem_add_total_pages`（`vm/main.c:485-489`、`vm/utility.c:59-63`），若固件报了两条重叠的 AVAILABLE，C 会把同一段内存加两次总量，而内核账本也会把它当两段空闲发两次。合并是修正一个真实的重复计数面，不是美化。
- 候选 C（拒绝）：重叠即拒绝启动。拒绝理由：过度严格——重叠的固件条目在真实机器上存在，合并后语义明确（字节集合的并集），没有理由把能启动的机器挡掉。
- 依据：C 的账本语义是「一组可用字节」，不是「一个有序数组」；数组是实现。合并保留字节集合，删除下标序与地址序的偏差（文档 §2.4.2.1 已逐形态取证）。

## DECISION-11：`param_buf` 保留字节格式与首匹配查询；去重语义需要作者裁决

- 候选 A（采纳，待裁决）：保留 `key=value\0...\0` 的双 NUL 结尾格式（`pre_init.c:61-74` 的写点、`do_getinfo.c:143-145` 的整块移交、`main.c:481-501` 的首匹配扫描），但把去重从「只对第一条记录有效」修成「对所有记录有效、后写覆盖先写」。
- 候选 B（拒绝）：照抄 C 的 off-by-one（`pre_init.c:47-59`：`while (*p++)` 后再 `p++`，从第二条记录起匹配从第二个字符开始）。拒绝理由：那是缺陷，不是契约；它导致的可见后果是 `board=` 双记录与外部输入遮蔽内核值（文档 §2.3 已推演），没有任何文档化消费者依赖重复键。修成后「内核在命令行之后写的 `arch=`/`board=` 取代命令行同名键」才与代码意图一致。
- 候选 C（拒绝）：改成结构化键值表（数组 + 索引）。拒绝理由：`param_buf` 的字节形态经 `GET_MONPARAMS` 整块交给用户态（`do_getinfo.c:143-145`），PM 缓存后由 `sysenv`/`mib` 消费（文档 §2.3 的落链）；改内部表示会改外部字节流，属于可观测行为变化。
- 标注：这一条是**唯一一处建议偏离 C 可观测行为**的地方，必须由作者裁决；若不批准，退回候选 B 的逐字节兼容实现，并在 `ParamBuf` 的文档里写明缺陷。依据：`pre_init.c:47-59`、`:61-74`、`main.c:481-501`。

## DECISION-12：参数缓冲溢出记录后继续，不静默、不致命

- 候选 A（采纳）：`ParamBuf::set` 返回 `Result<(), ParamOverflow>`；收集器把它记进 `Diagnostics` 与 `AccountingResidue.param_dropped`，然后继续启动。外部行为与 C 相同（启动不失败，装不下的键不出现），但留下可查的痕迹。
- 候选 B（拒绝）：照 C 静默丢弃（`mb_set_param` 返回 -1，三处调用都不看：`pre_init.c:144,149,150`）。拒绝理由：与项目「失败可追溯」的取向冲突，且这类丢弃会表现为「某个启动参数莫名其妙不生效」，正是诊断章最想避免的安静失败。
- 候选 C（拒绝）：溢出即致命。拒绝理由：C 在长命令行下仍能启动；把「某个可选参数丢了」升级为「不能启动」是行为倒退，会把原本可用的机器挡在门外。
- 依据：`pre_init.c:66-67` 的容量检查与三个忽略返回值的调用点。

## DECISION-13：不在载荷里硬编码 4 GiB 上限；可寻址范围由消费方按架构声明

- 候选 A（采纳）：`PhysAddr` 是 `u64`；账本不设 `LIMIT`。每个消费方（内核早期分配器、VM 交接）用自己声明的 `MAX_PHYS` 过滤并报告被排除的字节。x86-32 形态的 `MAX_PHYS = 0xFFFFF000`，64 位形态取架构可表示上界。
- 候选 B（拒绝）：照抄 `#define LIMIT 0xFFFFF000`（`pg_utils.c:89-96`）。拒绝理由：这个常量的理由是「the rest of minix currently can't deal with any bigger」（源码注释），即 32 位 `phys_bytes` 的承载限制，不是内存语义。把它塞进交接载荷会让 64 位腿白白丢掉高地址内存。
- 候选 C（拒绝）：完全不设上限、消费方也不检查。拒绝理由：到了 VM 的点击运算或 32 位字段才截断，会把「不可表示」变成静默丢字节；必须有显式的过滤点与残差记录。
- 依据：`pg_utils.c:89-96` 的注释；`const.h:98` 的 `NR_MEMS` 与 VM 的 `phys_bytes` 运算（`vm/utility.c:65-78`）。语义是「只发可寻址的内存」，机制（上限是多少）属于架构。

## DECISION-14：错误是类型化故障 + errno 映射，不是裸整数也不是字符串 panic

- 候选 A（采纳）：`BootFault`（致命：坏魔术、版本、畸形 wire、位置不符、占用者重叠、容量不足、模块缺失）与 `BootWarning`/`BootError`（可继续：参数溢出、名字不符、重叠条目）。`BootFault::errno() -> i32` 只映射 Minix3 已有值：`EINVAL`、`EFAULT`、`ENOMEM`、`E2BIG`、`ENOENT`、`EIO`（`minix3/sys/sys/errno.h:43-70`、`:46`、`:153`）。致命路径由入口转成一次 `panic!`，消息只含静态串与整数，不做堆分配。
- 候选 B（拒绝）：全部返回裸 `i32` errno。拒绝理由：引导故障不是系统调用返回值，没有调用者能恢复；裸整数丢掉「哪个模块、哪个区间、期望多少」这些诊断信息。errno 映射仍保留，但它不是主表示。
- 候选 C（拒绝）：全部 `panic!("...")` 字符串。拒绝理由：不可在 host 测试里断言具体失败种类，也无法在发送侧决定「返回固件错误页」还是「停住」；类型化错误让每条失败路径都有测试。
- 依据：AGENTS.md 的「错误类型必须映射到 Minix3 的 errno 值，禁止自造错误码」；Minix3 的 errno 清单。补充：01 的故障不经过 IPC，errno 映射是诊断与将来用户态可见报告用的，不假装它是系统调用语义。

## DECISION-15：跨镜像走 wire，同镜像直接传值；不为「统一」让同镜像腿绕字节

- 候选 A（采纳）：三条跨镜像腿（两条 UEFI、riscv64 U-Boot shim）编码 `Wire`；两个直核腿在同镜像内构造 `BootPayload` 后按值移入接收落地区。两条路径共用同一个私有校验构造器与同一组不变量。
- 候选 B（拒绝）：五条腿一律 wire（同镜像也自编码再自解码）。拒绝理由：这只让实现「看起来统一」，代价是多一次编解码与一块 arena，收益是零语义增量；违反「抽象语义，不描述机制」——统一的应该是语义对象，不是传输机制。C 的两个直核/同镜像形态本来就是直接写结构体（`pre_init.c:224` 传 `&kinfo`），没有序列化。
- 候选 C（拒绝）：跨镜像也不用 wire，靠固定物理地址共享结构体。拒绝理由：正是 I1 要防的「指针/句柄跨镜像」；高半切换后发送侧地址不可读（文档 §1.0 失败对照）。
- 依据：I1（文档 §1.2）；C 的同镜像 `pre_init → return &kinfo → kmain`（`pre_init.c:237`、`main.c:128`）。

## DECISION-16：并发用一次性发布单元 + 线性能力 token，不用锁、不用全局可变

- 候选 A（采纳）：`BootAuthority` 是零尺寸、`!Send + !Sync` 的 token（内部 `PhantomData<*mut ()>`），只能由架构入口构造一次；账本的构造与发布要求 `&mut BootAuthority`。`BootCell<BootPayload>` 用 `UnsafeCell<MaybeUninit<T>>` + `AtomicU8` 状态（UNINIT/INITIALIZING/READY）实现一次性发布：`init(value) -> Result<&T, AlreadyInitialized>` 用比较交换；`get()` 用 acquire 读。发布后 `T` 永不改变、永不析构，因此 `&T` 可以安全地长期共享给后续 AP。`unsafe` 只出现在 `BootCell` 一个模块内，并附安全论证。
- 候选 B（拒绝）：自旋锁保护账本。拒绝理由：引导期没有第二个 CPU、没有中断重入（入口先关中断，`head.S:68-77`），锁会暗示一种必须永远不发生的并发修改；而 `kernel_may_alloc` 的语义是「阶段所有权」，不是互斥（`glo.h:76`、`main.c:105,142`）。
- 候选 C（拒绝）：`static mut` + 约定。拒绝理由：每次访问都是 `unsafe`，Rust 的别名规则无法审计；也无法在 host 测试里验证「只初始化一次」。
- 依据：单核关中断的引导上下文（`head.S:68-77`）；`kernel_may_alloc` 在 `glo.h:9-12,76` 与 `pre_init.c:33` 的两份定义说明全局布尔 + 双镜像副本会带来别名混淆，Rust 用类型消除它。

## DECISION-17：内存账在入口处按值移交给内核内存子系统；VM 拿快照，不共享可变引用

- 候选 A（采纳）：`BootPayload` 发布后不可变；`kmain` 把 `payload.memory().clone()`（定长数组拷贝，约 2 KB）移交给内核早期分配器。VM 启动协议再从内核分配器导出一份快照（自己的版本号与容量），VM 拥有那份快照。
- 候选 B（拒绝）：让内核长期持有 `&'static mut MemoryLedger`。拒绝理由：`&'static mut` 与「payload 可以被任意读者共享」冲突；引导后的分配会对同一数组做可变访问，别名无法证明。
- 候选 C（拒绝）：照 C 让 VM 直接读内核全局 `kinfo.memmap`。拒绝理由：C 的 `sys_getkinfo` 是把整块结构复制给 VM（`vm/main.c:442`、`vm/utility.c:53-60`），本来就是快照语义；共享指针只存在于同地址空间的 C 里，Rust 侧没有理由保留。
- 依据：`vm/utility.c:53-60` 的 `mem_chunks` 复制与 `NR_MEMS` 遍历；`vm/main.c:485-489` 的总量累加。

## DECISION-18：引导器自身占用是一个可选的类型化区间，不是通用保留表

- 候选 A（采纳）：`BootPayload` 里只有 `shim: Option<SourcedSpan>`（来源 `BootShim`），表示发送侧 shim 的代码/栈/arena 中不在内核镜像内的部分；模块与内核各自的区间已经在模块表与位置记录里，不需要重复。`occupants()` 是一个计算视图，按「内核、各模块、shim」依次产出。
- 候选 B（拒绝）：维护一张通用 `reserved_regions` 表并随 wire 传输。拒绝理由：它与模块表 + 内核位置 + shim 区间三处信息重复，多一份就必须多一组「两处一致」的校验；C 也没有这张表（`param.h:14-47` 里只有 `module_list`、`bootstrap_start/len`，没有通用保留表）。
- 候选 C（拒绝）：完全不记 shim 占用，假定 shim 在内核镜像内。拒绝理由：U-Boot shim 是独立装载的镜像，它的内存不在内核 `_kern_size` 里；不排除就会把活代码当空闲页发出去，属于静默内存破坏。这是 C 的 `bootstrap_start/len`（`pre_init.c:114-116`、`main.c:301`）在 Rust 侧对应的真实语义。
- 依据：`pre_init.c:114-116` 的 unpaged 区间与 `main.c:301` 的归还；Rust 没有 unpaged 段，但「shim 自身内存」这个事实仍然存在。

## 抽象边界与五条生产腿

core 只认识四个语义概念：物理区间（`ByteSpan`/`PhysSpan`）、模块名（字节串 + `NameId`）、内核位置（三个整数）、诊断通道。core 里不出现 UEFI、DTB、Multiboot、ELF、页表项、`target_arch` 条件编译。平台适配层把「字节从哪来」翻译成这四个概念，翻译错了只会产生 `BootFault`，不会把机制类型漏进语义层。这与项目第一原则一致：同一层代码拥有同一语义，差异待在该待的地方。

平台边界用泛型而不是 trait 对象（no_std、无堆、希望内联）：

```rust
pub trait BootPlatform {
    fn firmware_memory(&self, out: &mut SpanSink<'_>) -> Result<(), PlatformError>;
    fn kernel_load_claim(&self) -> KernelPlacement;      // 发送方声称的落点
    fn command_line(&self, out: &mut [u8]) -> Result<usize, PlatformError>;
    fn shim_span(&self) -> Option<ByteSpan>;             // 同镜像腿返回 None
    fn board(&self) -> BoardIdentity<'_>;                // arch=/board= 的值
}

pub trait ModuleSource {
    fn count(&self) -> usize;
    fn name(&self, index: usize, out: &mut [u8]) -> Result<usize, ModuleError>;
    fn size(&self, index: usize) -> Result<u64, ModuleError>;
    fn read(&self, index: usize, dst: &mut [u8]) -> Result<(), ModuleError>;
}

pub trait DiagSink {
    fn line(&self, level: Level, msg: &core::fmt::Arguments<'_>);
}
```

`SpanSink` 是固定容量的收集器，只接受 `Available` 语义的条目（非可用条目在适配层就被丢掉，与 C 的 `if(mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue` 同义，`pre_init.c:181`）；容量溢出当场返回错误，不截断（DECISION-09）。`ModuleSource` 以「索引 + 调用方给缓冲」工作，避免在引导期分配；名字与字节分两次取，名字进名字池，字节直接读进已经分配好的模块页。

五条腿只在三个位置上不同，语义门完全共用：

| 腿 | 镜像关系 | 内存图来源 | 模块来源 | 该腿特有的前置门（在共用门之前） |
|---|---|---|---|---|
| UEFI 链（x86-64） | 跨镜像 | 固件内存图 | 固件文件系统，按序读入已分配页 | 页分配不落内核跨度；模块区间互不相交 |
| UEFI 链（aarch64） | 跨镜像 | 固件内存图 | 同上 | 同上，页粒度按架构 |
| riscv64 U-Boot shim | 跨镜像 | 固件/设备树提供 | 预置在固定装载区，按名读出 | 编译期断言内核最大装载端不越过模块区基址；装载指针的复查 |
| 直核面 A | 同镜像 | 平台给的静态表 | 镜像内静态表 + 名字池 | basename、长度、不落内核跨度、不落名字池 |
| 直核面 B | 同镜像 | 同上 | 同上 | 同上，架构常量不同 |

共用门就是 DECISION-02 的 `Raw → Validated → Payload` 那份顺序：信封与容量、位置交叉校验、模块条数与区间、占用者不重叠、账本不变量、参数格式、名字索引。腿特有的门放在适配层，因为它们依赖「这条腿怎么取件」；它们的作用是把明显不合法的输入挡在共用门之前，而不是替代共用门。依据：文档 §1.4 的接缝声明（01 管取字节、校验、记账、构造、发送）与 §2.6 的三层检查（装载次序、条数上限、区间不重叠）；C 侧对应的是 `pre_init.c:167-171` 的三条断言与 `:201-214` 的重叠检查。

## 身份与状态机

身份分三层，每层只回答一个问题：

- **交接实例的身份**：wire 的 `magic` + `WIRE_VERSION`。一次启动只有一个实例，没有协商、没有多版本共存；接收方只按精确版本接受。
- **模块的身份**：有序下标是契约（对应 C 的 `module_list[i - NR_TASKS]`，`main.c:179-184`），名字是证据；`NameId` 是名字池的索引，不是指针，也不是用户态可见的名字。
- **内存区间的身份**：页对齐的 `(base, len)` 对（`PhysSpan`）；来源标签说明它从哪来；占用者的身份是「内核 / 第 n 个模块 / shim」这种类型化描述，而不是数组槽位。

状态机是一条单向链，没有回退边：

```text
collect（平台适配层，外来数据）
  → validate（共用门，产生可消费值）
  → publish（BootCell 一次性发布，之后不可变）
  → handoff（账本按值移交内核分配器）
  → snapshot（VM 阶段取自己的副本）
```

C 的状态机是同一件事，但用「同一个全局对象被不同阶段的代码改写」实现（`pre_init` 填、`kmain` 复制、`arch_boot_proc` 加回模块、`arch_system` 切 BIOS 区、VM 再截 16 条）。Rust 把每个阶段的所有权写进类型：`pre_init` 对应 collect+validate，`kmain` 对应 publish+handoff，VM 对应 snapshot。阶段的先后由「前一步的返回值是后一步的输入」保证，不靠全局标志。依据：C 的这些改写点分别见 `pre_init.c:224-234`、`main.c:128`、`protect.c:450-451`、`arch_system.c:277-278`、`vm/utility.c:58-60`。

## 构造顺序（把 D1/D2/D4 变成数据依赖）

```text
1. ledger = MemoryLedger::from_firmware(platform.firmware_memory(...), arch.max_phys)?
2. modules = ModuleTable::collect(source, boot_contract)?
3. for occupant in [placement, modules..., shim] {
       ledger.reserve(occupant.span, occupant.source)?
   }
4. params = ParamBuf::build(platform.command_line(...), platform.board())?
5. payload = BootPayload::new(placement, modules, ledger, params, names, shim)?
6. Wire::encode(&payload, arena)?            // 跨镜像腿
   或 publish(payload)                        // 同镜像腿
```

第 1 步只建账，第 3 步才裁剪；裁剪需要内核位置与模块区间，所以「内核位置先于清单裁剪」由参数依赖强制（D2），「内存清单先于建表」由 `BootPayload` 是建表入口的唯一参数类型强制（D1），「结构先于 kmain」由入口签名强制（D4）。第 5 步在完整类型状态版里对应 `validate_then_freeze`（内部先 `validate` 再 `freeze`），在最小方案里就是 `BootPayload::new`；两条路线的构造顺序相同。C 里这三条靠 `get_parameters` 与 `pre_init` 的语句顺序（`pre_init.c:224-234`）；Rust 里顺序错误首先是编译错误，其次才是运行期门失败。取整与来源随每次 `reserve` 写入，`AccountingResidue` 汇总因取整、重叠合并、参数丢弃造成的字节/条目差。

## 最小方案（概念更少）

**一句话**：一个带私有校验构造器的按值 `BootPayload`（内含带来源的可用区间集合、有序模块表、内核位置、参数缓冲，外加可选的 shim 区间），跨镜像腿用一份手工小端 wire 编解码，同镜像腿直接传值；保留区间是计算视图，不存储、不传输；入口把账本按值拷进内核分配器，VM 再拿自己的快照。

相对上面的完整设计，最小方案删掉这些概念：

1. **删 `BootReport` 发送视图**：发送方直接构造 `BootPayload`，`Wire::encode(&BootPayload)` 只读它。少一个类型、一次转换、一处「两个视图字段必须一致」的校验。
2. **删并行的 `SourceRecord` 表**：来源是 `SourcedSpan` 的字段（DECISION-03）。「每个可用字节有来源」由构造器保证，`audit_source_coverage` 这个运行期函数不存在。
3. **删存储的保留区间表**：保留区间由 `occupants()` 从内核位置、模块表、shim 区间计算（DECISION-18）。少一段 wire、少一条「空闲与保留不相交」的跨表不变量。
4. **删 `BootStorage` 的 fill-once 状态机**：各部件按值构造，`BootPayload::new` 一次收齐；「第二次 fill」在类型上不存在，不需要 `Option` 字段与运行期 panic。
5. **删 `ValidatedPayload` 中间态**：`BootPayload` 的构造器私有且唯一，构造即校验；wire 的 `RawPayload` 只是解码器的局部借用视图，不进入公开类型。I4 仍成立（没有别的路能造出可消费值），但少一个公开类型。
6. **删 `mmap_size`、空槽、空洞、40 槽**：账本是区间集合（DECISION-04/09）。
7. **删「内核当第 M+1 个模块」**：内核位置是独立字段，重叠检查接受占用者迭代器（见下「不从 C 复制」第 3 条）。
8. **删 `kernel_may_alloc` 全局布尔**：阶段所有权由 `BootAuthority` 与「按值移交」表达（DECISION-16/17）。
9. **删 VM 的 16 条截断**：01 不把下游容量写进载荷；VM 阶段在自己的协议里显式声明容量与溢出行为。
10. **删 4 GiB 魔数与参数去重循环**：分别由消费方上限与显式 `ParamBuf::set` 取代（DECISION-11/12/13）。

代价与边界：最小方案放弃了「公开的两阶段类型状态」，因此如果将来出现多个互相独立的收集器（例如一条腿先收内存图、另一条腿后收模块），需要重新引入阶段类型；当前五条腿都是「一个平台对象一次收齐」，单构造器足够。最小方案也放弃了存储的保留表，因此保留区间的审计是每次计算的；占用者数量很小（≤ 21），成本可忽略。

## 其余决策候选（补充说明）

### D-补 1：账本取页方向

- 采纳：`allocate_highest_page()` 从最高地址区间取尾页；`allocate_lowest_run(len)` 从最低地址找第一个足够长的区间并整段减去。两者都返回 `PhysSpan`，都以「区间减法」实现，不共享 `(start, end)`/`(start, len)` 这种裸参数对。
- 拒绝：照 C 的 `pg_alloc_page` 按下标倒扫（`pg_utils.c:145-156`）。理由：下标序不是地址序（DECISION-10），语义是「从高地址取」，不是「从最后一个数组元素取」。
- 拒绝：保留 `alloc_lowest` 的 `cut_memmap(cbi, lowest, len)` 形状（`pg_utils.c:81`）。理由：第三个参数是结束地址还是长度，调用点看不出来；`ByteSpan`/`PhysSpan` 类型把这个错误变成编译错误。
- 依据：`pg_utils.c:65-84`、`:138-160`；文档 §2.4.2.3 的第三行常态推演。

### D-补 2：模块区间用字节区间，账本用页区间

- 采纳：`ModuleEntry.span: ByteSpan`（非空、`end > start`，允许非页对齐，因为模块文件长度不保证页粒度）；`reserve()` 负责向外取整并在 `AccountingResidue` 记录补白。内核位置与 shim 也用 `ByteSpan`，进账时同样向外取整。
- 拒绝：模块一进来就转成页区间。理由：VM 需要模块的精确字节长度（`mod_end - mod_start` 用于 ELF 解析与 blob 长度，`protect.c:412`），提前取整会丢信息。
- 依据：`protect.c:412` 的 `filesize = mod_end - mod_start`；`pg_utils.c:37-40` 的向外取整。

### D-补 3：`arch=`/`board=` 的值由平台提供，写入顺序保留

- 采纳：`BootPlatform::board()` 返回的 `BoardIdentity` 提供 `arch_name` 与 `board_name` 两个字节串；收集器在命令行解析之后无条件写入两条记录，保留「内核值覆盖命令行同名键」的次序（配合 DECISION-11 的修复）。
- 拒绝：照抄 `BOARD_ID_INTEL` 硬编码（`pre_init.c:149-150`）。理由：x86 UEFI 腿未必是 Intel 板；ARM 侧本来就是动态查 `machine.board_id`（`earm/pre_init.c:274-275`）。语义是「告诉上层在什么板上启动」，不是「写死一个常量」。
- 依据：`pre_init.c:148-150`、`earm/pre_init.c:274-275`；文档 §2.3 的遮蔽推演。

## 故意不从 C 复制什么（清单与理由）

| # | C 的东西 | 锚点 | 为什么不复制 |
|---|---|---|---|
| 1 | 全局可变 `kinfo`，`pre_init` 返回 `&kinfo`，`kmain` 再 `memcpy` 一次 | `pre_init.c:20,237`；`main.c:128` | 同一个二进制 + 全局地址是 C 的承载方式；Rust 用按值载荷与一次性发布，消除「半填状态」与「二次复制」。第二次复制在 C 里是因为跨栈切换后仍要一个全局地址，不是语义。 |
| 2 | `memmap[40]` + `mmap_size` + 零长度空槽 + 首个空槽回填 | `param.h:13,18-20`；`pg_utils.c:104-118` | 定长存储的实现痕迹；每个消费者都要自己跳过空洞（`pg_utils.c:147`），`mmap_size` 是最高占用下标加一而非条目数。Rust 用区间集合，容量按最坏情况推导。 |
| 3 | 把内核塞进 `module_list` 的第 M+1 槽以复用重叠检查 | `pre_init.c:194-199,201-207` | 这是复用检查代码的技巧，不是语义。Rust 让内核位置成为独立类型，重叠检查接受「占用者迭代器」，同样只有一套逻辑，但不需要伪造一个模块，也不需要 `kern_mod` 下标。 |
| 4 | `cut_memmap(start, end)` 与 `add_memmap(addr, len)` 的参数种类不一致 | `pg_utils.c:32,86`；`:81` 的误用 | 两种裸参数对无法从调用点区分；`alloc_lowest` 把 `len` 当 `end` 传就是判例（`pg_utils.c:81`）。Rust 全部用 `PhysSpan`/`ByteSpan`，错误在编译期。 |
| 5 | `mb_set_param` 的去重 off-by-one（只对第一条记录生效） | `pre_init.c:47-59` | 缺陷，不是契约；见 DECISION-11（需裁决）。 |
| 6 | 参数缓冲溢出静默丢弃 | `pre_init.c:66-67,144,149,150` | 保留「继续启动」的结果，但记录丢弃；见 DECISION-12。 |
| 7 | `kernel_may_alloc` 的两份同名定义（unpaged 与 paged 各一份） | `glo.h:9-12,76`；`pre_init.c:33`；`table.c` | 全局布尔 + 镜像副本的别名混淆；Rust 没有 unpaged 副本，用 token 与按值移交表达阶段所有权（DECISION-16/17）。 |
| 8 | VM 只读前 `NR_MEMS = 16` 条，其余静默忽略 | `const.h:98`；`vm/utility.c:58-60` | 下游消费者的容量不是交接载荷的字段；01 交完整账本，VM 阶段显式声明自己的容量与溢出行为（DECISION-17/最小方案第 9 条）。 |
| 9 | 硬编码 `LIMIT 0xFFFFF000` | `pg_utils.c:89-96` | 32 位 `phys_bytes` 的承载限制，不是内存语义；上限归消费方按架构声明（DECISION-13）。 |
| 10 | `OVERLAP` 的 `mod_end - 1` 与「相切必须放行」的隐式约定 | `pre_init.c:82-84` | 半开区间应直接用 `a.end <= b.start || b.end <= a.start` 判无重叠；`-1` 对空区间会下溢，且把「最后一个字节」的翻译错误变成误报。Rust 的 `ByteSpan` 非空，判据无下溢。 |
| 11 | 把 `mmo_string` 指针复制进内核记录 | `pre_init.c:170-171`；`multiboot.h:224-229` | 指针指向引导器内存，跨镜像/高半后失效，I1 禁止；Rust 把名字复制进名字池，用 `NameId` 引用。 |
| 12 | ARM 侧 `setup_mbi` 伪造一份 multiboot 结构（固定模块基址、单条内存表） | `earm/pre_init.c:190-217` | 这是「把另一种引导协议翻译成 multiboot 形状」的适配器；Rust 的平台 trait 直接描述事实（固定放置表、一条可用区间），不需要在语义层伪造一个协议结构。 |
| 13 | Multiboot 识别头与 `0x1BADB002`/`0x2BADB002` 魔术值 | `multiboot.h:40-44,86`；`head.S:43-66` | 只对 GRUB 形态的腿有意义；UEFI/U-Boot/直核腿不读它。魔术值留在各腿的适配层，core 只有「入口约定」的抽象。 |
| 14 | 4 KiB `.data` 的 `load_stack` 与入口 `%esp ≡ 0`（不满足 SysV） | `head.S:69,94-96`；文档 §2.2 第 5 条 | 入口栈大小与对齐是链接/入口约定，不是交接语义；Rust 入口 asm 显式对齐，栈大小由链接脚本按需给出。 |
| 15 | `arch=`/`board=` 固定写 `BOARD_ID_INTEL` | `pre_init.c:149-150` | 值应来自平台；写入次序与「内核值覆盖命令行」的语义保留（D-补 3）。 |
| 16 | 整份 `memcpy` `multiboot_info`（含指针字段） | `pre_init.c:108` | 外来结构里的指针跨镜像不可用；Rust 只复制语义字段（内存图、模块区间、命令行字节），I1。 |

## host 测试清单（按不变量组织）

所有测试跑在 `cfg(test)` 的宿主形态；核心 crate 保持 `no_std`，测试模块可用 `std`。不引入属性测试依赖，用确定性伪随机（xorshift）与小域穷举。

**I1（值可跨镜像）**
- `wire_roundtrip_identity`：任意合法载荷编码再解码，逐字段相等（含名字池与参数字节）。
- `wire_has_no_pointers_by_construction`：`Wire` 的字段全是整数/字节数组；用 `const` 断言尺寸与字段偏移只依赖显式常量，不依赖 `repr(C)`。
- `decode_never_panics_on_arbitrary_bytes`：固定种子的任意字节串语料（长度 0..4 KiB），解码只返回 `Err`，不越界、不 panic。
- `wire_rejects_bad_magic_version_truncation_section_overlap_duplicate`：每条畸形各一个用例，断言具体 `BootFault` 变体。

**I2（来源可追溯）**
- `every_free_span_has_source`：构造后遍历，来源字段总在；`SourcedSpan` 没有无来源构造器（编译失败测试或私有字段检查）。
- `merge_preserves_source_precedence`：重叠固件条目合并后来源取序号小者，`AccountingResidue` 记一条告警。
- `coverage_equation`：`sum(firmware available bytes) == sum(free bytes) + sum(reserved∩firmware bytes) + rounding_residue`，在随机操作序列后成立。

**I3（所有权明确）**
- `payload_built_once_by_value`：`BootPayload::new` 消费各部件；同一部件二次使用是编译错误（doc/compile-fail 测试）。
- `bootcell_single_init_under_race`：多线程竞争 `init`，恰好一个成功，其余 `AlreadyInitialized`，读到的值一致。
- `ledger_operations_keep_invariants`：任意 `insert_free`/`reserve`/`release` 序列后，区间升序、互不相交、页对齐、非空。

**I4（校验先于消费）**
- `consume_requires_payload_type`：建表/分配 API 的签名只接受 `&BootPayload`；用 compile-fail 测试证明裸 `RawPayload` 不能传入。
- `placement_mismatch_rejected`：发送方声明与接收方链接常量差一页即 `PlacementMismatch`。
- `overlap_tangent_allowed_containment_rejected`：A=[0x1000,0x2000)、B=[0x2000,0x3000) 通过；B=[0x1800,0x2800) 拒绝；空区间不可构造。
- `module_count_mismatch_rejected`、`module_name_policy_per_leg`：条数硬门；名字在「按名定位」腿致命、在「按序取件」腿只记诊断。

**容量与代数**
- `worst_case_capacity_no_overflow`：`FW_SPAN_CAP` 条初始区间 + `OCCUPANT_CAP` 次内部切割，长度不超过 `FREE_CAP`，不返回错误。
- `capacity_exceeded_is_error_not_truncation`：多一条即 `CapacityExceeded`，且账本内容不变（强异常安全）。
- `cut_restores_free_bytes`：切掉再放回，字节总量回到原值（页对齐区间上精确成立）。
- `cut_idempotent_on_account`：对已切走的区间重复 `reserve`，空闲账不变，保留视图不重复计数。
- `allocate_highest_returns_highest_address`、`allocate_lowest_run_returns_lowest_fit`：与地址序一致，不受插入顺序影响。

**参数**
- `param_format_matches_c`：`A=1\0B=2\0\0` 的黄金字节；首匹配查询；双 NUL 结尾。
- `param_dedup_policy`：若采纳修复，`A=1` 后写 `A=2` 只剩一条且值为 2；若裁决保留 C 行为，则用「第二条记录不生效」的黄金用例锁定。
- `param_overflow_records_and_continues`：超长值被丢弃、`param_dropped` 计数加一、其余键完好。

**类型与预算**
- `const_free_cap_equation`、`const_module_cap_le_19`：编译期断言。
- `payload_size_budget`：`size_of::<BootPayload>()` 不超过给定预算（例如 8 KiB），防止容量参数悄悄膨胀。

## 与四条不变量的对应（收口）

| 不变量 | 设计承载 | 证据 |
|---|---|---|
| I1 值可跨镜像，指针/句柄不可 | 按值载荷、名字池 + `NameId`、wire 偏移相对 arena、禁止 `usize`/指针 | `wire_roundtrip_identity`、`wire_has_no_pointers_by_construction` |
| I2 来源可追溯 | `SourcedSpan.source` 字段；占用者来源由身份给出；`AccountingResidue` 记取整与重叠 | `every_free_span_has_source`、`coverage_equation` |
| I3 所有权明确 | 各部件按值构造、`BootPayload::new` 一次收齐、`BootCell` 一次性发布、账本按值移交 | `payload_built_once_by_value`、`bootcell_single_init_under_race` |
| I4 校验先于消费 | 私有唯一构造器、消费 API 只收 `&BootPayload`、wire 两遍解码 | `consume_requires_payload_type`、`decode_never_panics_on_arbitrary_bytes` |

## 未决与需裁决

1. **DECISION-11 的参数去重修复**是唯一建议偏离 C 可观测行为之处，需作者裁决；不批准则逐字节兼容。
2. `FW_SPAN_CAP = 64` 需要在目标固件样本上实测（可在 host 测试里用真实 E820 转储回放）；若样本超过，调常量而不是改语义。
3. 名字校验在 UEFI 腿是硬门还是诊断，取决于启动配置是否允许改名；建议先诊断，直核/U-Boot 腿硬门。
4. 64 位腿的 `MAX_PHYS` 与 VM 点击运算的可表示范围需要与 02/VM 阶段对齐；01 只负责不丢字节、显式报告残差。


## 无发现

本报告是独立设计提案，不报缺陷；决策见 DECISION-01..18，供 D2 差异审查。
