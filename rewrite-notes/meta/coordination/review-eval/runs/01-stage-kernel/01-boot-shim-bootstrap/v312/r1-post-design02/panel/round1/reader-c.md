DOC 01-boot-shim-bootstrap.md

# Reader-C 报告（C 因果一致性）

读者画像：具备 C 阅读能力、操作系统启动基本概念、Rust 语法与所有权基础、物理/虚拟地址与页表基本概念；不默认具备本项目内部类型、三架构启动路径，以及尚未在前置章节讲授的内部设计契约。只读本篇，不用源码或其他文档补全作者没有写出的内容。按 review-evals.md §4.0e 的 10 条验收清单报。

准入原则：只有让我卡住（理解路径断裂、必须外部知识才能往下读、两处说法冲突导致无法判断）的写 `## FINDING`；"换一种讲法更好"、"指针可以更近"、"术语可以更统一"写 `## SUGGESTION`。每条 FINDING 都带原文摘录。

---

## 根因 A：`store` 一词同时指“解码落盘”与“包装调用”，导致函数名与次序在两处对不上

### FINDING READER-C-01（P1）

- 位置：§4.4“五条腿的 `BootReport<'_>` 发送视图与接收侧 `KernelInfo` 构造”：“接收侧统一走 `store_boot_report`，先把切片深拷进内核 `.bss`，再生成唯一的 `KernelInfo<'static>`。”；§4.5“`store_boot_report`/`store_kernel_info` 的六个落地区”：“v1 接收面的 `resume_additive` 仍走 `store_kernel_info` 包装。”以及“`resume_additive` 的 `store_kernel_info` 源已是前一步解码落进 store 的切片（同址，跳过）。”
- 严重度：P1
- 问题：同一接收侧（v1 接收面）在 §4.4 被说成走 `store_boot_report`，在 §4.5 被说成走 `store_kernel_info`。§4.5 的代码块里 `store_kernel_info` 与 `store_boot_report` 是两个并列的 wrapper（都调 `land_and_store`），不是“`store_kernel_info` 再转 `store_boot_report`”的关系。读者无法判断 v1 接收面到底调用哪一个。
- 影响：I1/I3 的“接收侧落私有存储”证据要按函数名去找。如果按 §4.4 的“统一 `store_boot_report`”去找，到 §4.5 会发现 v1 走 `store_kernel_info`，进而怀疑整条落盘链是否一致；§4.4 字段表里 `source_records` 行的“接收侧 `store_boot_report` 落 `.bss`”也对 v1 不成立。这是理解 I3 证据的第一个分叉。
- 证据或推理：只引文档内句子。§4.5 的代码块同时给出 `pub fn store_kernel_info(kernel_info: &KernelInfo)` 与 `pub(crate) fn store_boot_report(report: &minix_boot::BootReport<'_>, ...)` 两个签名；§3.7 的 v1 逐腿链末尾是 `store_kernel_info`；§4.4.1 的 v1 链也是 `store_kernel_info`；§4.8 的 v1 链仍写 `store_kernel_info`。所以“接收侧统一走 `store_boot_report`”是少数派，且没有任何一句说明 `store_kernel_info` 是 `store_boot_report` 的别名或包装。
- 最小修复：把 §4.0/§4.4 的“接收侧统一走 `store_boot_report`”改成“直核面走 `store_boot_report`；v1 接收面走 `store_kernel_info`；两者都经 `land_and_store` 落同一组 `.bss` 落地区”。若两者确有 wrapper 关系，补一句“`store_kernel_info` 内部调用 `store_boot_report`”并给出调用位置。
- 置信度：高
- 是否需源码核实：否（文档内两处说法已冲突；若要确认 wrapper 关系才需源码）
- 若我错了：也许作者把“接收侧统一”理解成“最终都落到同一组 `.bss` 落地区”，而不是“调用同一个函数名”；但 §4.4 的原文是“统一走 `store_boot_report`”，按字面读就是同一个函数，读者没有别的线索。

### FINDING READER-C-02（P1）

- 位置：§3.6“五条生产腿与发送侧（接缝定义）·三处具体事实”：“接收侧先把六个分区直接解码进 `store_kernel_info` 的六个 `.bss` 落地区（`resume_additive` 的 store 因源与目标同址成为 no-op），再只做 additive 阶段”；§4.5：“`resume_additive` 的 `store_kernel_info` 源已是前一步解码落进 store 的切片（同址，跳过）”；§4.8：“v1 接收面是 `store_kernel_info` → `resume_additive` 内的 `establish_boot_dm` → `jump_to_kmain`”。对照 §3.7：“v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`。这两组腿（直核面与 v1 接收面）的 store 与 boot_validate 次序相反。”；§4.4.1 的 v1 链：“boot_validate（1，`resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate`”。
- 严重度：P1
- 问题：v1 接收面的“落盘”与“校验”谁先谁后，四节给了两种读法。若把“解码进 store”算落盘，则 §3.6/§4.5/§4.8 说落盘在 additive（含 `boot_validate_and_prepare`）之前；§3.7/§4.4.1 说 `store_kernel_info` 在 `boot_validate_and_prepare` 之后。文档没有给“解码落盘”与“`store_kernel_info` 包装调用”两个事件分别命名，读者无法判断哪一句说的是哪一个。
- 影响：I3 的“载荷先落到自己的存储，再发布”与 I4 的“校验先于消费”都依赖这个次序。读者会卡在“v1 接收面到底有没有在校验前拿到私有副本”这个问题上，无法用本篇的证据回答。§3.7 的“store 与 boot_validate 次序相反”如果按落盘理解，与 §3.6 直接冲突；如果按包装调用理解，则这句话对 I3 的“载荷先落盘”没有说明力，反而让读者以为 v1 是先校验后落盘。
- 证据或推理：§3.6 的“先把六个分区直接解码进……落地区……再只做 additive 阶段”与 §4.5 的“源已是前一步解码落进 store 的切片”都指向落盘在前；§3.7/§4.4.1 把 `store_kernel_info` 排在 `boot_validate_and_prepare` 后；§4.8 又把 `store_kernel_info` 排在 `resume_additive` 的 `establish_boot_dm` 前。三组句子无法同时按同一个“store”含义读通。
- 最小修复：在 §3.7 或 §4.5 增加一句事件定义：“v1 接收侧有两件事容易同名：解码把六个分区直接写进 `.bss` 落地区（发生在 `resume_additive` 的 additive 阶段之前）；`store_kernel_info` 是之后对同一地址的包装调用（同址跳过复制）。因此‘载荷先落盘’对两族都成立；‘store 与 boot_validate 次序相反’只指包装调用。”并把 §3.7 的“次序相反”改成“包装调用次序相反”。
- 置信度：高
- 是否需源码核实：否（文档内次序已冲突；若要确认实际调用点才需源码）
- 若我错了：也许作者刻意用“解码”与“store”区分两个事件，并在 §3.6 的“store 因源与目标同址成为 no-op”里已经暗示；但 §3.7/§4.8 仍用同一个 `store_kernel_info` 名字，读者无法仅凭文档确定。

---

## 根因 B：总述把只对部分腿成立的结论写成普遍结论

### FINDING READER-C-03（P2）

- 位置：§1.3“与 Minix3 的对应”：“C 的模块来源是引导器的模块链表；Rust 的来源是 UEFI 文件系统、U-Boot `fatload`/`BootFileTable` 或 DTB `/chosen`，取件抽象在 `FileLoader`。”；§3.5.1：“两个直核面不走 `FileLoader`，但共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`；入口壳只负责常量、横幅与 console 选择。”
- 严重度：P2
- 问题：§1.3 是总述，读者会把它读成“五条腿的取件抽象都在 `FileLoader`”；§3.5.1 明确说两个直核面不走 `FileLoader`。总述把一个只对 UEFI/U-Boot 两族成立的事实写成了对整个 Rust 侧成立。
- 影响：读者在 §1 建立的“取件统一在 `FileLoader`”模型，到 §3.5.1/§4.3 必须推翻；后面读“两个直核面共用 `bootface_common`”时会怀疑是不是同一件事的两种说法。五条腿的“谁走哪个取件抽象”这一层认知是错的。
- 证据或推理：§3.5.1 的“两个直核面不走 `FileLoader`”是直接反例；§4.3 的“两个直核面不走 `FileLoader`”再次重复。§1.3 没有加“boot-shim 两族”限定。
- 最小修复：在 §1.3 的句子后加“（UEFI 与 U-Boot 两族经 `FileLoader`；两个直核面共用 `bootface_common` 的放置流程）”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：也许 §1.3 的“Rust 的来源”只指“文件来源”这一层，而直核面的 DTB/`BootFileTable` 也算来源，`FileLoader` 只是其中两族的抽象；但句子没有给这个限定，读者会按最自然的“全部 Rust 侧”读。

### FINDING READER-C-04（P2）

- 位置：§3.2“按值交接：`KernelInfo` wire 与字段台账（I1）·维度二”：“直核面虽然没有跨镜像指针，但同一套落地区把三形态收成一条路径（少一套特例），也让‘解码之后再消费’成为统一次序。”；同段随后：“两个直核面的切片由发送侧 `BootReport<'_>` 借用给出，接收侧落 `.bss` 后再构造 `KernelInfo`，没有 wire 解码步骤（§4.4）。”
- 严重度：P2
- 问题：同一段先说“解码之后再消费”是三形态统一的次序，随后又说两个直核面“没有 wire 解码步骤”。读者无法判断 `direct-core` 到底有没有“解码”这一步：如果“解码”是泛指“把切片落进 `.bss`”，那么“没有 wire 解码步骤”只是限定 wire，不算矛盾；但句子没有给“解码”与“落盘”的区分，读者按字面会认为“解码”是 v1 专有动作。
- 影响：三形态的“统一次序”是 I3“深拷到私有存储”的跨腿论证；如果直核腿没有解码，读者会以为三形态在这一点上不统一，进而怀疑 I3 的“三形态一致”说法。这是总述层面的因果链断裂，虽然下一句能部分补救。
- 证据或推理：§3.2 同段自己给出了“没有 wire 解码步骤”；§3.5.1 也说两个直核面不走 `FileLoader`。因此“解码之后再消费”不能按字面覆盖直核腿；能覆盖三形态的只能是“落盘之后再消费”。
- 最小修复：把“也让‘解码之后再消费’成为统一次序”改成“也让‘落盘之后再消费’成为统一次序（v1 腿是解码落盘，两个直核腿是借用后落盘）”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：也许作者把“解码”当作“把外部表示变成进程内切片”的上位词，直核腿的“落 `.bss` 后再构造 `KernelInfo`”也算解码；但同段的“没有 wire 解码步骤”让这个上位读法不自然。

---

## 根因 C：计数/台账口径在同一段内对不上

### FINDING READER-C-05（P2）

- 位置：§3.7“所有权与校验门链（I3/I4）”开头：“所有权：`BootStorage` 拥有 U-Boot shim 腿上的六类载荷；UEFI 腿只拥有五类（五个 `fill_*` 入口；`fill_map_tables` 一次提交两表）；……`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自在闭包返回时提交计数，半提交状态不可表示。”同段后文：“UEFI 腿的 source 记录由分类器与 DTB claims 两阶段直写 wire，不经过 `fill_source_records`；因此‘第二次 fill 会 panic’只对实际走 `fill_*` 的腿成立。”
- 严重度：P2
- 问题：五个 `fill_*` 入口里，`fill_source_records` 在 UEFI 腿被同一段的下一句明确排除。如果 UEFI 用四个入口，`fill_map_tables` 一次提交两表，正好覆盖五类（memmap、reserved、modules、platform、param）；“五个 `fill_*` 入口”把第五个（source）也算给 UEFI，与下一句矛盾。读者无法确定 UEFI 到底走四个还是五个 fill 入口。
- 影响：I3 的“`fill_*` 一次提交”证据和“第二次 fill 会 panic”的适用范围都建立在这个计数上；数错一个入口，就会把 U-Boot 的 source 提交规则误套到 UEFI 上。读者在核对 I3 的“半提交状态不可表示”时会发现两个互斥的计数，无法判断哪一个是证据。
- 证据或推理：同段先列五个 `fill_*` 名字，后说 UEFI 的 source 不经过 `fill_source_records`；五个名字减去 source 是四个，四个入口中 `fill_map_tables` 提交两表，覆盖五类。因此“五类（五个 `fill_*` 入口）”的括号与正文不能同时成立。
- 最小修复：改成“UEFI 腿只拥有五类（四个 `fill_*` 入口：`fill_map_tables` 一次提交两表；`fill_source_records` 只在 U-Boot shim 腿使用）”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：也许作者把 `fill_map_tables` 算成一个入口、把 source 的直写也算作“第五个 fill 入口”的等价物；但“五个 `fill_*` 入口”字面上指向同段列出的五个函数名，其中 source 已被排除。

---

## 根因 D：I2 审计用的 kind 名称与同节的 kind 枚举不是一套

### FINDING READER-C-06（P2）

- 位置：§3.3“两张内存表与来源记录（I2）”：“`SourceRecord` 给每个投影字节记来源：`kind`（`/memory`、MRB（device-tree memory reservation block，设备树内存保留块）、`/reserved-memory`、UEFI 描述符等）、`origin`（节点/条目/描述符下标）、`range`、`input_object`、`attribute`。”同节：“覆盖审计是 I2 的判据：free 表的每个字节必须能被 `kind = Memory` 的记录解释；reserved 表的每个字节必须能被 reservation 类记录解释。”
- 严重度：P2
- 问题：前一枚举给的 kind 值是路径/来源名（`/memory`、MRB、`/reserved-memory`、UEFI 描述符），后一句的审计判据突然要求 `kind = Memory`。文档没有说 `Memory` 是 `/memory` 的枚举变体名，也没有说 UEFI conventional 描述符记录是否同属这个 kind。读者无法判断哪些记录能满足 free 表的覆盖审计。
- 影响：I2 的“每个投影字节可追溯”是四条不变量之一，证据就是这条审计；如果 `kind = Memory` 的所指不清，读者只能猜“大概就是 `/memory`”，无法核对来源覆盖。后面 §5.1 的 I2 证据行又依赖 `audit_source_coverage` 的正负控，术语不清会一路带到测试证据。
- 证据或推理：同节两句话使用了两套 kind 词汇；文中没有 `Memory` 的定义或到 `/memory` 的回指。§1.2 的 I2 行只说“每个投影字节有 `SourceRecord`；`audit_source_coverage`”，也没有定义 kind 取值。
- 最小修复：在 §3.3 的枚举后加一句“（审计里的 `kind = Memory` 指 `/memory` 这一类来源；UEFI conventional 描述符的记录按同一 kind 归类）”，或直接列出 `SourceRecord` 的 kind 枚举值与路径名的对应。
- 置信度：中
- 是否需源码核实：否（文档内应能给出对应；若作者认为枚举名只有源码能确认，可标“是”）
- 若我错了：也许 `Memory` 在源码里就是 `/memory` 的变体名，作者认为读者能从上文推断；但本读者按画像不具备项目内部类型，无法仅凭文档确定。

---

## SUGGESTION

以下各项命中读者验收清单，但按本轮严重度门槛未到“卡住无法继续阅读”，只作改进建议。

### S-01（命中清单第 4 条）

- 位置：§3.2.1“`KernelInfo` 结构”：“wire 信封 160 → 128 字节，报文末尾的 startup 分区（20 × 72 = 1440 B）退场，`WIRE_STARTUP_END`（30624）随之消失，八页 arena 的设计余量从 2144 B 变成 3584 B。`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收。”；同节：“wire v10（设计裁决）又删掉 `startup_objects`（当时既是 `KernelInfo` 字段、也是 wire 分区与 builder 参数）、`user_sp`、`kern_stack_top`、`syscall_entry` 四个，落到 10。”
- 问题：四个删除成员里 `startup_objects` 被描述成“wire 分区”（20 × 72 = 1440 B），不是信封字段；而信封 160 → 128 的 32 B 差需要四个 8 B 字段。读者按给出的四项只能算出三个 8 B 字段（`user_sp`、`kern_stack_top`、`syscall_entry`），无法核对 32 B。若 v9 信封还含 `startup_objects` 的 `byte_len`（8 B），文中没有出现这个字段。
- 最小修复：在“四个成员”后补“（v9 信封还含 `startup_objects` 的 byte_len 8 B；四个 8 B 字段合计 32 B）”，或给出 160/128 的逐字段对照。
- 置信度：中
- 是否需源码核实：否（这是文档内的算术自证问题）

### S-02（命中清单第 2 条）

- 位置：§3.2.4“深拷不变量与真机探针”：“两条 UEFI 产线的六个分区都由 boot-shim 在启动服务池或 arena 里产出；无论哪一条，接收方都必须在首次进程根切换之前把值搬进自己的 `.bss`——arena 只在恒等窗口存活，池页在历史真机上还出现过零化。”对照 §3.2.3：“发送侧的语义表住进 shim 自己的 `BootStorage`，arena 里只剩 wire 这一份表示。”；§4.2：“语义表住在 shim 自己的静态区（`.data`：`BootStorage::new()` 非全零；见 §3.2）。”
- 问题：五个非 source 分区的语义表在 `BootStorage` 的静态 `.data`，arena 里是 wire 副本；把“六个分区都由 boot-shim 在启动服务池或 arena 里产出”当作统一口径，会让读者把“池页零化”的风险误套到静态 `.data`，也会让“深拷”的因果链看起来依赖一个不存在的统一存储位置。
- 最小修复：改成“六个分区的 wire 副本都在 arena；发送侧的语义表在 `BootStorage`（静态 `.data`）。零化风险作用于 arena 与历史池页，不作用于静态 `.data`。”
- 置信度：高
- 是否需源码核实：否

### S-03（命中清单第 1 条）

- 位置：§3.1：“读文件在动作 2、EBS 在动作 6。”；§3.2.1：“或者把第 4 层的 `DM` 覆盖要求塞进第 3 层的分类判据”；§2.4.2.3：“按当前 `C = 1` 计算总量约 2212 B”。
- 问题：`EBS`、`DM`、`C` 首用时都没有展开；读者要自己猜 `EBS = ExitBootServices`、`DM = Direct Map`、`C = CONFIG_MAX_CPUS`。按画像（具备基本 OS 概念，不默认具备本项目缩写）这会在跨节阅读时造成小停顿。
- 最小修复：首用处写全称或定义；`C = 1` 改成 `CONFIG_MAX_CPUS = 1`。
- 置信度：高
- 是否需源码核实：否

### S-04（命中清单第 1/4 条）

- 位置：§1.1“两个直核腿同镜像”；§3.6“两个直核面”；§3.6“直核家族收敛在 `arch_boot_finish`”；§3.6 的计数表“辅助/测试 2（riscv64 `bootface`、aarch64 `bootface_a64`）”。
- 问题：`直核腿`、`直核面`、`直核家族` 三个名字指同两条腿；在计数、边界声明和设计段之间混用。读者需要在心里做一次合并，跨节引用时容易怀疑是不是多了一类。
- 最小修复：首次出现时定义“直核腿（= 直核面；两条：riscv64 `bootface`、aarch64 `bootface_a64`）”，或统一用同一个词。
- 置信度：高
- 是否需源码核实：否

### S-05（命中清单第 5/1 条）

- 位置：§3.2.1：“或者把第 4 层的 `DM` 覆盖要求塞进第 3 层的分类判据（……）都会让错误的层次为另一个层次的约束背锅。**（两合同分别见 §3.2 与 §4.6）**”；同节：“wire 标签 2（输入对象）、6（kind）与 5（origin）只作保留位。”
- 问题：§3.2.1 里的“见 §3.2”指向本节所属的父节，读者不知道具体是哪张表；“wire 标签 2/6/5”没有定义，读者无法判断这些保留位指什么。
- 最小修复：把“见 §3.2”改成“见本节开头的两张内存表”；给标签编号一个定义（或删掉编号，只保留“保留位”）。
- 置信度：高
- 是否需源码核实：否

### S-06（命中清单第 2 条）

- 位置：§3.4.1：“那次险情靠人工改值解决；字段删除之后，‘提供一个可误设的回收区间’这个接口面整体消失，险情在类型上不可复发。”；§3.4.2：“合并曾被试过：shim 的游标跨过子区间末尾（模块与 arena 在子区间之外），`BootAlloc::resume` 直接看到 `next > end`，QEMU U-Boot 门在 3/6 个路标处 panic，合并方案已回滚。”
- 问题：这两条历史因果只给结论，没有日志/提交/测试名的锚点；读者无法复核“人工改值”和“3/6 个路标处 panic”的具体现场。它们被用来支撑设计取舍（字段删除、两个游标不合并），但证据强度低于本篇其他“可复跑探针”的写法。
- 最小修复：各补一个取证位置（日志路径、提交号或测试脚本名），或明确标为“观察，无归档日志”。
- 置信度：中
- 是否需源码核实：否（若要补锚点才需查源码/日志）

### S-07（命中清单第 3 条）

- 位置：§2.4.2.3：“在当前调用序与四个前提下，切除一次都不发生（第三行常态）……四前提是核对过的调用序，不是结构保证”；同节前提三：“**前提三：`roundup(tramp_size)` ≤ 最低基址。**”
- 问题：前提三是地址/尺寸关系，不是调用序；把四条统称“核对过的调用序”会让读者误以为结论只依赖执行顺序，从而低估前提三（跳板大小 ≤ 最低基址）的分量。后文虽然逐条列了前提，但“调用序”这个总称与前提三的类别不一致。
- 最小修复：改成“四前提是核对过的事实（调用序 + 地址/尺寸关系），不是结构保证”。
- 置信度：高
- 是否需源码核实：否

---

## 附带发现（超出 C 因果一致性角色，不计入本职读数）

- **术语表位置**：§1.0 的“最小示例”在“术语”小节之前使用 `KernelInfo`；“前置概念”说“本章会给出 `KernelInfo`、`memmap`、`reserved_regions` 与五条生产腿的定义”，但术语表没有 `memmap`/`reserved_regions`，它们首次出现在 §1.1 的表里。建议把术语表提前，或把这两个词补进术语表。（清单第 1 条）
- **§5.4 的示例串**：riscv64 直核行写“`table holds 0x` + 十六进制条目数（脚本完整串为 `table holds 0x0000000000000000 entries`）”。例子是 0 entries，读者会以为该门要求条目数为 0；若它只是格式占位，建议改成“形如 `table holds 0x<条目数> entries`（示例中的 0 只是占位）”。（清单第 2 条）
- **§6.1 的“临时”**：`kernel: preA rdi=…` 被标为“另一条临时取证打印（源码注释自标 `temporary`）”。这不是评审过程词，但读者会问“临时打印是否会影响 PASS 判据”；建议补一句“不参与门判据”。（清单第 2 条）
