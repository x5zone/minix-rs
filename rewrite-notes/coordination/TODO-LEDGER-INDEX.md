> **创建**: `2026-10-08`
> **本文件是什么**: 待办台账体系的总入口。它**不装待办条目**（条目在 `TODO-LEDGER-OPEN.md` 与 `TODO-LEDGER-DONE.md`），只回答四个问题：谁对哪件事有解释权、`coordination/` 这七十六个文件各自被怎样处置、各本账本的编号如何归一到同一件事、以及下一轮扫描怎么在不重扫全量的前提下接上。
> **核实现场基线**: 快照 `87784ca04`（本轮清理前的最后一笔状态固化提交）。

# 待办台账索引

## §0 权威归属声明

本项目同时存在多本账本，它们**不是同一张表的多个副本，而是不同层次的视图**。合并待办时最怕的就是造出第五个"看起来什么都管"的文件，于是每条都规定归属：

| 层次 | 账本 | 它有权威的东西 | 本台账与它的关系 |
|---|---|---|---|
| 设计级 | `STRUCTURAL-DEBT-REGISTER-20261008.md`（`SD-1`—`SD-43`） | 结构性债务的**定义与状态词表**；本台账的六值状态词表直接沿用它的 §0 | 只登记"仍未闭合"的读数与锚点，不改写它的论证 |
| 裁决级 | `PENDING-DECISIONS-3ARCH-PARITY.md`（`PD-01`—`PD-34`，第十轮已定稿冻结） | 需要人拍板的事项的**最终结论与被否方案** | `TODO-LEDGER-DONE.md` §2 逐条引用其结论；裁决之后的执行缺口落 `TODO-LEDGER-OPEN.md` |
| 认领级 | `TODO-3ARCH-PARITY-20261006.md`（`P-*`） | 三架构 × 能力的**齐平矩阵**、排除清单、覆盖度自证格式 | `TODO-LEDGER-OPEN.md` §2 收其未闭项；`TODO-LEDGER-DONE.md` §3 收其排除清单 |
| 编排级 | `edge_todo.md`（`E-*`）与 `new_edge4.md` §2 认领板（`C-*`）、§6 待裁决队列（`OQ-*`） | 跨阶段条目的**描述**与并线协作的持锁状态 | 本台账统一给状态；描述一律看原文件。`edge_todo.md` 被十七个内核与服务端源码按文件名引用，原地保留 |
| 阶段级 | `rewrite-notes/{01..18}-stage-*/todo.md` 与各阶段附属台账 | 单阶段条目的修法与验收 | 本台账**不复制条目**，只在 `TODO-LEDGER-OPEN.md` §6 给一行索引 |
| 案卷级 | `CASE-RISCV64.md`、`CASE-AARCH64.md`（索引与结论卷）、`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`（未结案卷工作现场）、`NK4C-BUG-RISCV64-TRANSIENT-PTE.md`、`ADDRESS-CONSTANT-AUDIT.md`、`PATTERN-SCAN-REPORT-20260923.md` | 单个缺陷或专项的**取证过程与判据** | 未结项按锚点引用进 OPEN，过程叙事留在案卷 |
| 过程记录 | `NK4C-WORKLOG.md`、`REVIEW-HISTORY.md`、`SESSION-HISTORY.md` | 时间序列上发生过什么（滚动日志、评审史、会话史） | 零改写。合并卷只抽结论 |

一句可执行的判据：**状态以本台账为准，描述与修法以原账本为准，裁决以 `PD-*` 为准。** 三处出现分歧时的优先序仍是项目总则那一条：C 源码的实际行为 > 设计契约 > Rust 实现 > 文档 > 分析。

## §1 `coordination/` 的文件处置表（已执行，实况数字）

清理前 76 个文件，清理后 **28 个**：原地保留 19 个（含受保护项）＋ 本轮新增 9 个；另有 3 个知识类文件移入 `../concepts/`。共删除 54 个（评审类 19、会话类 28、案卷类 4、其余 3），全部已在本笔提交前的快照里入库，全文可回捞。

### 1.1 原地保留（十九个）

| 文件 | 为什么留着 |
|---|---|
| `STRUCTURAL-DEBT-REGISTER-20261008.md` | `SD-*` 设计级权威，状态词表的出处 |
| `PENDING-DECISIONS-3ARCH-PARITY.md` | `PD-*` 裁决级权威，已定稿冻结；被 `prompt/review-rules/review-core-semantics.md` 引用 |
| `TODO-3ARCH-PARITY-20261006.md` | `P-*` 认领级矩阵与排除清单；被 `tools/check-staged-inflight.sh` 与 `os/libs/minix-types/src/ipc/message.rs` 按文件名引用 |
| `edge_todo.md` | `E-*` 跨阶段条目的描述权威；被十七个 `os/` 源码与脚本按文件名引用 |
| `edge_todo_archive.md` | **本轮改判保留**：权威台账 `edge_todo.md` 正文里有两个链接指向它，删了就在权威文件里留断链；它体积只有一百余行 |
| `edge1.md`、`edge2.md`、`edge3.md`、`edge4.md` | 第一轮并行分线的原始载体，也是并发纪律的原始记录；`edge2.md`、`edge4.md` 被 `os/` 代码注释引用。纪律条款已抽入 `CONCURRENT-WORK-PROTOCOL.md`，原件作为范例保留 |
| `new_edge2.md`、`new_edge4.md` | 第二轮仍在制的线文件与认领板（仍有进行中的条目与待领项），收线后按 `CONCURRENT-WORK-PROTOCOL.md` §10 归档 |
| `NK4C-WORKLOG.md` | 滚动工作记录，`misc_concepts.md` 与结构债台账的取证来源指针 |
| `ADDRESS-CONSTANT-AUDIT.md` | 地址常量清扫的审计账本；被 `os/libs/minix-types/src/types/boot.rs` 引用 |
| `PATTERN-SCAN-REPORT-20260923.md` | 模式扫描账本，被 `tools/pattern-gate.sh` 引用，且是 `misc_concepts.md` 的来源真源 |
| `NK4C-OPENING-PROMPT.md` | 被 `os/kernel-image/src/bootface.rs` 当裁决源引用；余下内容已摘入 `SESSION-HISTORY.md` |
| `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` | 被 `os/libs/minix-types/src/types/boot.rs` 引用；已转历史案卷，只接受勘误 |
| `NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md` | **本轮改判保留（与清理方案的偏离）**：该案仍在追缴（裁决 `PD-16`），其第四章「已排除假设 + 每条手段的分辨力边界」是全仓库防返工价值最高的一段，压缩入索引卷即毁。索引与地图在 `CASE-RISCV64.md`，原件作为工作现场保留 |
| `NK4A-TODO.md`、`NK4B-TODO.md` | 两轮任务书的阶段台账（当时条目已全闭），仍被 `PATTERN-SCAN-REPORT`、`misc_concepts.md` 与 `edge_todo.md` 当来源引用；体积不大了但仍是台账本体 |
| `TODO-LEDGER-*`、`STAGE-KERNEL-FREEZE-READY-20261008.md`、`CONCURRENT-WORK-PROTOCOL.md`、`REVIEW-HISTORY.md`、`SESSION-HISTORY.md`、`CASE-RISCV64.md`、`CASE-AARCH64.md` | 本轮新增的九个交付物 |

### 1.2 并入 `REVIEW-HISTORY.md` 后删除（十九个）

八份 `NK4C-REVIEW-REPORT-*.md`（`20260923`、`20260923-R2`、`20260927-R3`、`R31`、`R32`、`R33`、`R34`、`R35`）、`NK4A-REVIEW-REPORT.md`、两份 `NK4-REGRESSION-REVIEW-20260922*.md`、`NK4C-RETRO-AUDIT-20261001.md`、`NK4C-REVIEWER-SESSION-RECORD-20260930.md`、六份 `NK4C-R*-P1-AUDIT-TABLE-*.txt`。

合并卷只保留：每轮的评审区间与快照提交、P0 与 P1 计数、**仍未闭合的发现**、该轮得出的可复用判据，以及一张"跨轮反复未修项"的追踪表。

### 1.3 并入 `SESSION-HISTORY.md` 后删除（二十八个）

八份 `NK4C-接续PROMPT-*.md`、`NK4C-RESUME-PROMPT.md`、`NK4C-GLM53-PROMPT.md`、`NK4C-NEW-MACHINE-OPENING-PROMPT.txt`、`NK4A-QWEN-OPENING-PROMPT.md`、`NK4B-OPENING-PROMPT.md`、`HANDOFF-NK4A-boot-first-light.md`、`HANDOFF-NK4A-review-and-redo.md`、`NK4A-HANDOFF-STATUS.md`、`MISC-CONCEPTS-SESSION-HANDOFF-20260927.md`、`PATTERN-GATE-SESSION-HANDOFF-20260927.md`、`nk4c-glm-riscv-session-handoff-20261002.md`、`NK4C-MIGRATION-20260930.md`、五份 `new_todo_{HY4,deepseek,glm,muse,qwen}.md`、`NK4A-QWEN-WORKLOG.md`、`NK4B-WORKLOG.md`、`claim-prompt.md`（它的规则部分归 `CONCURRENT-WORK-PROTOCOL.md` §11，它的会话坐标归本卷）。

合并卷保留：每条任务线的推进主线与终点状态、每次交接留下的未竟项、五路独立扫描的共识与分歧归并、以及"被后续账本覆盖"的指针。`new_todo_*` 原件的价值在于独立覆盖度，其结论已归并进台账，逐条读数不再维护（`OQ-N1` 已裁定命令全集口径以规划文件为准）。

### 1.4 并入案卷后删除（四个）

`riscv-reviewlog.md` → `CASE-RISCV64.md`（静态扫描线六轮账）；`NK4C-BUG-AARCH64-VEC-CAP.md`、`NK4C-BUG-AARCH64-VEC-CAP-GLM.md`、`NK4C-AARCH64-EXEC-REBIND-LIVELOCK.md` → `CASE-AARCH64.md`。

案卷合并的取舍：现象、定性、**已排除假说及其证伪方法**、可迁移判据、结案状态与关联编号入新卷；逐轮取证对话与日志摘录不入新卷。两处改判：未结案的 `NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md` **不并不解**（见 1.1 的改判理由）；已结案的 `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` 本就因被生产代码注释引用而保留。

### 1.5 折干后直接删除（三个）

`new_edge1.md` 与 `new_edge3.md`（进行中的条目进 OPEN §5，所有权与前置信息进协议卷；`new_edge2.md` 受保护、`new_edge4.md` 为活跃认领板保留）、`STRUCTURAL-DEBT-REGISTER-20260930.md`（十六行取代说明存根，指向已在位的新册；删除同一笔提交内同步摘掉 `tools/check-staged-inflight.sh` 里那条在制登记）。`claim-prompt.md` 计入 1.3。

### 1.6 移入 `rewrite-notes/concepts/`（三个）

| 原路径 | 新路径 | 为什么算知识 |
|---|---|---|
| `riscv瞬态页表崩溃取证方法论.md` | `concepts/transient-fault-forensics.md` | 自陈"只讨论怎么观察、不讨论怎么修"，四条结论都带命令凭据，跨任务可复用 |
| `trap-boundary-message-materialization.md` | `concepts/trap-boundary-message-materialization.md` | 跨层的边界与消息实体化概念，`misc_concepts.md` 三次把它当知识源引用 |
| `NK4C-QEMU-ENVIRONMENTS.md` | `concepts/qemu-environments.md` | 环境矩阵与构建参数，不属于任何阶段也不属于待办 |

`ADDRESS-CONSTANT-AUDIT.md` 与 `PATTERN-SCAN-REPORT-20260923.md` **不搬**：前者是审计账本且被生产代码注释引用，后者被门禁脚本引用且是知识点汇总的来源真源。

### 1.7 本轮新增（九个）

`TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`TODO-LEDGER-INDEX.md`（本文件）、`STAGE-KERNEL-FREEZE-READY-20261008.md`、`CONCURRENT-WORK-PROTOCOL.md`、`REVIEW-HISTORY.md`、`SESSION-HISTORY.md`、`CASE-RISCV64.md`、`CASE-AARCH64.md`。

**净效果**：目录内文件从 76 降到 28（保留 19 + 新增 9），知识类 3 篇移出；`rewrite-notes/concepts/` 从 6 增到 9。

### 1.8 回捞方式

删除的原件按文件名在 Git 历史取回：`git log --diff-filter=D --summary -- rewrite-notes/coordination/<文件名>` 找到删除提交，再 `git show <删除提交>^:rewrite-notes/coordination/<文件名>`。本轮删除发生在清理提交中，其前一笔父提交即含全文。

### 1.9 修链与遗留

- 指向被删文件的 **Markdown 链接形态**：目录外零命中（全仓实测）；目录内唯一一处（已删的 `new_edge1.md` 指向 `NK4A-REVIEW-REPORT.md`）随该文件一并消失。`edge_todo.md` 正文里指向 `edge_todo_archive.md` 的两个链接，因为归档件改判保留而不受影响。其余引用全是反引号或散文形态，`tools/notes-link-check.py` 不检这类形态。
- 需要更新指向的正式文档：`rewrite-notes/misc/misc_concepts.md`（命中最多的引用者；本轮已先把三个被移知识文档的路径改指 `concepts/`，并在头注加一行指向本节——其约二十个指向已删原件的反引号来源行**不回溯改写**，因为那是当时的取证记录）、`rewrite-notes/README.md`（目录描述与计数）。
- 历史账本内文（`NK4C-WORKLOG.md`、`PATTERN-SCAN-REPORT-20260923.md` 等）**零改写**：它们记的是当时的事实，路径失效由本节处置表兜底。
- 遗留一项工具面待办：`tools/anchor-suspect-baseline-c.txt` 里仍列被删案卷路径。该文件正由另一条线程修改，本轮**不代跑再生**，已登记在 `TODO-LEDGER-OPEN.md` §8。

## §2 编号归一映射（同一件事的多个名字）

不重编号。下表是"同一件事"的归组，来源是三处既有映射：`TODO-3ARCH-PARITY-20261006.md` §四第 96 行给出的 `SD-*` 到 `P-*` 对应、`STRUCTURAL-DEBT-REGISTER-20261008.md` §5.1 的"对应作业账本行"列、以及 `edge_todo.md` 正文的交叉引用。

| 一件事 | 结构债 | 齐平清单 | 裁决 | 跨阶段条目 | 阶段或编排 |
|---|---|---|---|---|---|
| 早期串口端口判断与有界轮询 | `SD-5` | `P-X86-02` | — | — | — |
| 引导镜像清未初始化数据段 | `SD-7` | `P-X86-03` | — | — | — |
| riscv64 非确定性内存污染 | `SD-10`（并 `SD-15`、`SD-26`、`SD-36` 三面） | `P-RV-02` | `PD-16` | — | 案卷 `CASE-RISCV64.md` |
| 进程间通信状态车道占用被调用者保存寄存器 | `SD-14` | `P-X86-05` | `PD-17` | — | — |
| 缺页地址填而不读 | `SD-15b` | `P-ALL-02` | `PD-04` | — | — |
| 指令缓存维护 | `SD-20`（并 `SD-42`） | `P-A64RV-02` | — | — | — |
| 用户栈指针对齐 | `SD-21` | `P-A64RV-03` | — | — | — |
| 内核访用户内存的特权屏蔽位 | `SD-22` | `P-A64RV-04` | `PD-21`、`PD-22` 同批 | — | — |
| aarch64 浮点跨陷入保存 | `SD-23` | `P-A64-03` | `PD-05` | — | 案卷 `CASE-AARCH64.md` |
| 真多核被钳在主核、次级核承载进程 | `SD-24`（并 `SD-37`、`SD-41`） | `P-ALL-03` | `PD-02`、`PD-19` | `E-SCHEDSMP` | `K1`/`K2`/`K3`（`edge1.md`）、`D-36`—`D-38`（`01-stage-kernel/todo.md`） |
| 结案后探针未滚除 | `SD-25` | `P-ALL-04` | `PD-10` | — | 案卷待办 `T8` |
| 语义与 C 真源不一致 | `SD-16`、`SD-17`、`SD-43` | `P-ALL-05` | `PD-12` | — | 评审编号 `F10`、`X-7` |
| 死代码与服务装配欠账 | `SD-29` | `P-ALL-06`、`P-ALL-07` | `PD-06` | `E-*WIRE` 全族 | `C-17`—`C-26` |
| 测试入口吞掉构建失败 | `SD-28` | `P-ALL-09` | `PD-30`、`PD-31` | — | — |
| 替身特性默认导致假绿 | `SD-30`、`SD-31` | `P-ALL-10` | `PD-29` | `E-VMMOCK` | 旧 `OQ-N5` |
| 启动入口形态并存 | `SD-1`—`SD-4` | `P-ALL-11` | `PD-07`、`PD-14`、`PD-15` | `E-BOOTFRAME` | — |
| 消息体尺寸不变量 | `SD-35` | `P-ALL-12` | `PD-20` | `E-MINTYPES-RUNTIME` | — |
| 服务框架语义 | — | `P-ALL-08`（子项 `T1`—`T7`） | `PD-24`—`PD-27`、`PD-34` | `E-IPCWIRE`、`E-ISWIRE` | `C-21` |
| 宿主不可达的门内测试 | `SD-34` | `P-ALL-01`、`P-A64-02` | — | — | — |
| 次级核早期入口体 | `SD-34` 入口面 | `P-A64RV-01` | — | — | `S-4`、`K9`/`K10` |
| 信号集位宽与信号面 | `SD-39` | — | — | `E7` | `D-16`、`D-17`（`04-stage-pm`） |
| 网络字节序与套接字事件手抄 | `SD-19` | — | `PD-13` | `E-SDEVOWN` | 17 阶段 |

编号空间清单（避免下轮误以为能合并编号）：`SD-*`（43）、`PD-*`（34）、`P-*`（27）、`E-*`（44）、`C-*`（约 60）、`OQ-*`（两轮共 14）、`D-*`/`W-*`/`I-*`/`T-*`/`A-*`/`R-*`/`M-*`（`01-stage-kernel/todo.md` 内联）、`S-*`（`smp_todo.md` 主线）、`V*`/`R*`/`N*`/`IN-*`/`NS-*`/`NL-*`/`NK-*`（各阶段与两轮线）、`K*`/`L*`（第一轮线）、`AF-*`（回溯审计）、`A1`—`A13`（地址常量审计）、`F*`/`G*`/`H*`/`J*`/`K1`—`K8`（各轮评审发现）。

## §3 状态词表与别名折叠

唯一合法的六个取值（出处 `STRUCTURAL-DEBT-REGISTER-20261008.md` §0），其余写法一律折叠：

| 原写法 | 折叠到 | 折叠规则 |
|---|---|---|
| `✅`、`已修复`、`已落地`、`完成`、`Done`、`Fixed` | `closed` | **必须代码坐实**：符号在生产路径被调用或有守护测试。只在测试里存在不等于闭 |
| `☐`、`未开工`、`待修`、`未实现`、`未接线`、`Not ported` | `open` | 直译 |
| `DEFERRED`、`stub`、`占位`、`🚧` | 看代码分流：脚手架已落只差接线 → `stopgap landed`；刻意不做且有理由 → `adjudicated`；完全没有 → `open` | 本项目 `DEFERRED` 一词至少承载三种意思，必须按代码分流 |
| `⚠️`、`部分`、`半截`、`partially` | `partially fixed` | 直译 |
| `🔄`、`进行中`、`接线中` | 主体已合入 → `partially fixed`；否则 `open` | 「进行中」不是状态，是状态的变化中 |
| `⏸`、`等待`、`WONTFIX`、`恒边界`、`归他阶段` | `adjudicated`（有裁决记录时）或 `open`（只是没人做时） | 必须写明等谁；写不出等谁就按 `open` |
| `🚫`、维持登记不排期 | `adjudicated`，并保留"为什么现在不做" | — |
| 已从账本删除但债务仍在 | `dropped-from-ledgers` | 这个取值专门用于防"删除即解决"的错觉 |
| 需要真机才能判、或需要人判 | 状态照常填，**另置证据级** `待验证` | 不新增第七个状态值，避免"待验证"变成逃避判定的口袋 |

## §4 判据表（本轮全部现算，不抄二手计数）

下表里一条命令可定案的行，已由 `tools/todo-reconcile-scan.sh` 集中成可重跑的执行器（只读，输出四列：判据名、本次读数、台账基线、判定）。**它只覆盖机器可判的那部分**：需要追调用链（证据级 `L2`）与需要真机（`待验证`）的条目它不接，输出里固定留一行 `NA` 提醒，免得把「脚本没报漂移」当成「台账已核实」。基线失配时的处置是复核后改台账，不是改脚本里的基线。

| 类别 | 判据命令（只读，可重跑） | 快照 `87784ca04` 上的读数 | 能定案到什么程度 |
|---|---|---|---|
| 结案后探针未滚除 | `grep -rn "用后即滚" os --include=*.rs \| wc -l`；`grep -rl "用后即滚" os --include=*.rs \| wc -l` | 15 行 / 7 个文件 | 可定案（`L1`） |
| 探针命名族 | `grep -rnE "nk4[ac]:" os --include=*.rs \| wc -l` | 15 行 | 可定案；注意计数正则必须覆盖两个族，历史上因只搜 `nk4a:` 而漏 |
| 生产码静默占位 | `grep -rn "todo!()\|unimplemented!()" os/kernel/src os/arch/src \| wc -l` | 0 | 可定案：**内核与架构层没有静默占位**，缺口以注释与显式停止形态存在，只能读码枚举 |
| 服务入口未通电 | `grep -rln "loop {}" os --include=main.rs \| wc -l` | 55 | 可定案 |
| 占位传输实现 | `grep -rn "UnimplementedTransport" os --include=*.rs \| wc -l` | 5 | 可定案 |
| 架构测试套件装配白名单 | `grep -n "ATF_BOOT_LEG_READY:" os/xtask/src/image.rs` | `["aarch64", "riscv64"]` | 可定案（x86_64 缺席是事实；能否跑绿需上机） |
| 宿主不可达的门内测试数 | 逐目录 `grep -c "#\[test\]"` 相加：arm64、riscv64 的架构目录与平台目录 | 88（39 + 42 + 3 + 4） | 可定案；每轮须重算，账面写的是"约 90" |
| 次级核入口体是否还是骨架 | `wc -l os/arch/src/{arm64,riscv64,x86_64}/ap_early_entry.rs` | 226 / 193 / 533 | 可定案（账面"41/40 行骨架"已过期） |
| 主核钳位是否仍在 | `grep -rn "clamp_cpu_to_bsp" os --include=*.rs` | 定义、调用、专项测试三处俱在 | 可定案（读调用上下文，`L2`） |
| 空闲核唤醒臂 | `grep -rn "fn context_stop_idle" os --include=*.rs` | 0 | 可定案 |
| 空闲标志清零 | `grep -rn "cpu_is_idle" os --include=*.rs` | 置起 1 处、清零 0 处 | 可定案 |
| 次级核放行邮箱 | `grep -rn "AP_GO" os --include=*.rs` | 声明 2 处、等待与注释若干、写入 0 处 | 可定案 |
| 浮点归属写入点 | `grep -rn "fpu_owner" os --include=*.rs` | riscv64 臂 1 处、aarch64 臂 1 处、迁移释放腿 1 处、x86_64 0 处 | 可定案 |
| 指令缓存维护 | `grep -rn "fence\.i\|dc *cvau\|ic *iallu\|sync_icache" os --include=*.rs` | 2（同一个 riscv64 文件，其一为注释） | 可定案 |
| 特权屏蔽位是否在生产置起 | `grep -rn "PAN\|SUM" os/arch/src os/kernel/src --include=*.rs` | 命中全在注释与位定义 | 可定案 |
| 内核包默认特性 | `grep -n "^default" os/kernel/Cargo.toml` | `default = ["mock"]` | 可定案 |
| 死码候选 | 逐符号 `grep -rn "<符号>" os --include=*.rs \| grep -v <定义所在文件>` | `dispatch_unused`、`console_write_str`、`runqueues_ok` 的外部命中全在测试内 | 可定案（消费方归属需人裁） |
| 地址常量残留 | `python3 tools/address-constant-scan.py os` | 有输出，需按审计的豁免清单逐条判读 | 工具在、判读未做 → 标 `待验证` |
| 真机门读数 | `os/qemu-tests/` 下各门脚本；`tools/check-rs-unwired.sh` | 本轮**不跑**（无人值守下禁止构建与虚拟机） | 全部标 `待验证`，写明解锁需要哪道门 |

## §5 覆盖度自证

**顶部警示**：本项目结构债台账自记"账面腐化率约两成每审计周期"。本轮实测支持这个量级，且方向单边——**代码普遍新于账面**：本轮共发现十五组"文档说未做、代码已做"（见 `TODO-LEDGER-DONE.md` §1 与 `STAGE-KERNEL-FREEZE-READY-20261008.md` §2），反向的"文档说已做、代码查无"只有三组，且都属于"注释落后于代码"而非"结论造假"。所以任何后续扫描都要先重算再写状态。

全量逐字读的文件：三份台账（`STRUCTURAL-DEBT-REGISTER-20261008.md` 的 §0/§5.1 与状态表、`TODO-3ARCH-PARITY-20261006.md` 全文、`PENDING-DECISIONS-3ARCH-PARITY.md` 的总览表与全部决议行）、`edge_todo.md` 的条目头与尾部未闭段、`new_edge4.md` 的 §1/§2/§6/§7/§8、`edge4.md` 的 §1/§6/§7/§8、`claim-prompt.md`、`edge1.md`、`edge2.md` 头部与状态列、18 份阶段待办的未闭部分、`01-stage-kernel/` 的 35 篇正式文档的标记命中行与对应章节。

抽样读的文件（抽样口径与理由）：`NK4C-WORKLOG.md`（一万四千余行，只读顶部当前状态块、文末最近若干节、以及"未结 / 待验证 / 盲区 / 收尾清单"命中处）；`NK4B-WORKLOG.md`、`riscv-reviewlog.md`、`PATTERN-SCAN-REPORT-20260923.md`、两份缺陷案卷（只读结论章、结案章与待办章）；八份评审报告（读结论与未修清单，不读逐轮对话）；五份独立扫描（读缺口清单，不读推导过程）。

未覆盖、必须标 `待验证` 的三类：
1. **一切需要真机的判据**：x86_64 目标③ 能否跑绿、riscv64 多核套件、riscv64 内存污染复现、端到端联调族（`E5` 各子面）、浮点用户态影响面、`test-smp-aps` 断言、真机测试脚本的清理行为。
2. **需要逐臂对照 C 真源的语义判断**：正错误码上线、虚存控制未知请求返回码、控制台魔术失配的错误码选型、系统信息交换格式约定。
3. **文档正文内嵌待办的全量清点**：约九十四份阶段文档含待办字样命中（本轮现算 160 份文件、701 行命中），只做了命中行定位与代表项核实，未逐条判读。下一轮可以按 `grep -rl` 的清单逐目录推进。

抽样声明之外的一条诚实边界：本台账的状态判定是**静态读码**结论。静态读码能确定"符号存在且被生产路径调用"，不能确定"行为对 C 真源正确"。凡涉及外部行为正确性的条目，本台账一律给 `待验证` 而不是 `closed`。

## §6 增量接入协议（下一轮怎么接上而不必重扫）

1. **只改两本台账**：翻转 `TODO-LEDGER-OPEN.md` 与 `TODO-LEDGER-DONE.md` 里的条目状态，或追加新条目。**不重编号**（沿用原始 ID 并列），**不新建 `new_todo_*.md`**（除非按 `CONCURRENT-WORK-PROTOCOL.md` 正式分线并发），**不复制阶段条目**（阶段侧只加状态注记）。
2. **每条状态变更必须附三件套**：可重跑的核实命令与读数、核实日期、当时的快照提交号。缺任一件，视同未核实。
3. **先查避让清单再动手**：`bash tools/check-staged-inflight.sh --list` 与 `git status --porcelain` 双查；清单内的文件属于别的线程，只读不写。
4. **每轮收尾跑五道校验**：`bash tools/todo-reconcile-scan.sh`（先重算可机器判据再写状态）、`python3 tools/notes-link-check.py rewrite-notes/`（与上一轮基线做集合差，新增断链必须为零）、`bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`bash tools/doc-style-lint.sh --diff`、`bash tools/check-staged-inflight.sh`。
5. **本文件 §1 的处置表是活表**：新增文件要立刻登记进 1.1—1.6 的某一类；某类文件清零后，其合并卷的对应章节标注"已清空，可归档"。
6. **阶段文档重生成前必读** `STAGE-KERNEL-FREEZE-READY-20261008.md`：那份文档管的是"哪些阶段文档可以安全定稿、哪些必须等代码"，与本台账互补——本台账管待办，它管冻结边界。
