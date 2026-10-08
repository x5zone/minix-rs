# coordination/ 目录说明

本目录管三件事：**待办在哪里记、结论在哪里裁、过程在哪里查**。共 28 个文件（清理后：保留 19 + 新增 9，明细见 `TODO-LEDGER-INDEX.md` §1；另有 3 篇知识类已移入 `../concepts/`，54 个被删文件全文可在 git 历史回捞）。

## 先回答两个操作问题

**新增一条 todo，写哪个文件？**

- 跨阶段 / 内核 / 多核 / 三架构齐平 / 测试门 / 卫生类：直接追加到 `TODO-LEDGER-OPEN.md` 的对应小节。沿用原始编号（`SD-*` / `P-*` / `E-*` / `C-*` 等并列，不重编号），附核实命令与读数、日期、快照号。
- 单个阶段内部的事：写对应 `rewrite-notes/{stage}/todo.md`，`TODO-LEDGER-OPEN.md` §6 只更新索引计数，不复制条目正文。
- 需要人拍板的新问题：另起新清单，不回填已冻结的 `PENDING-DECISIONS-3ARCH-PARITY.md`。
- 禁止新建 `new_todo_*.md`（除非按 `CONCURRENT-WORK-PROTOCOL.md` 正式分线并发）。

**完成一条 todo，更新哪个文件？**

- 只动两本台账：`TODO-LEDGER-OPEN.md` 里翻状态，或搬到 `TODO-LEDGER-DONE.md` §1（已完成）/ §2（裁决执行）。每条状态变更必须附三件套：可重跑的核实命令与读数、核实日期、当时快照提交号，缺一件视同未核实。
- 不改原账本的描述文字（`edge_todo.md` / 各阶段 `todo.md` / 结构债登记册里的论证），不改裁决文件。判据：**状态以本台账为准，描述与修法以原账本为准，裁决以 `PD-*` 为准**。
- 动手前双查避让（`bash tools/check-staged-inflight.sh --list` 与 `git status --porcelain`），收尾跑 `TODO-LEDGER-INDEX.md` §6 列的校验（`todo-reconcile-scan.sh`、`notes-link-check.py`、`todo-staleness-check.sh`、`doc-style-lint.sh --diff`、`check-staged-inflight.sh`）。

## 台账三件套（当前唯一可写入口）

| 文件 | 作用 |
|---|---|
| `TODO-LEDGER-INDEX.md` | 总入口。不装条目，只回答：谁对什么有解释权、28 个文件各自的处置、编号如何归一到同一件事、下一轮如何增量接入。含 §1 文件处置表、§2 编号归一映射、§3 六值状态词表、§4 机器可判据表、§6 增量接入协议。 |
| `TODO-LEDGER-OPEN.md` | 未完成总账。§1 内核与多核、§2 三架构齐平余件、§3 结构债未闭、§4 跨阶段 `E-*`、§5 并行编排 `C-*`、§6 十八份阶段待办索引、§7 测试门对账、§8 卫生项。 |
| `TODO-LEDGER-DONE.md` | 已完成存证。§1 代码坐实的 `closed`（回勾账面滞后用）、§2 已裁决执行表（`PD-01`—`PD-34`、`OQ-N1`—`OQ-N6`）、§3 正当差异与排除项（防重查，下一轮不许当缺口修平）。 |

## 设计 / 裁决 / 认领权威（读为主）

| 文件 | 作用 |
|---|---|
| `STRUCTURAL-DEBT-REGISTER-20261008.md` | 设计级权威。`SD-1`—`SD-43` 的定义、方案空间与推荐；六值状态词表的出处。 |
| `PENDING-DECISIONS-3ARCH-PARITY.md` | 裁决级权威。`PD-01`—`PD-34`，第十轮已定稿冻结，只读存证。 |
| `TODO-3ARCH-PARITY-20261006.md` | 认领级矩阵。`P-*` 三架构 × 能力齐平表、排序、排除清单；被 `tools/check-staged-inflight.sh` 与生产代码注释引用。 |
| `edge_todo.md` | 跨阶段条目 `E-*` 的描述权威。被 17 处 `os/` 源码按文件名引用，原地保留。表头状态普遍滞后，以最新进度注记为准。 |
| `edge_todo_archive.md` | 已闭单条目的原文归档（§0、`E3`、`E-RSSTART`、`E-VMMCPWIRE`）。闭单判定有误时从此恢复重开。 |
| `new_edge4.md` | 当前唯一活跃的编排板。§2 认领锁（`C-*`）、§6 待裁决队列（`OQ-*`）、§7 验收阶梯、§8 归档规则。 |
| `new_edge2.md` | 第二轮共享库线（`os/libs/` 枚举内 crate、`os/drivers/`）的在制线文件。 |

## 已冻结的历史线（只读，不要续写）

| 文件 | 作用 |
|---|---|
| `edge1.md` / `edge2.md` / `edge3.md` | 第一轮三条工作线（内核面 / 共享库面 / 服务器面）的分工索引与状态记账。条目多已 `✅`，作范例与来源引用保留。 |
| `edge4.md` | 第一轮编排底座：并发规则、认领板、依赖状态、验收阶梯。纪律条款已抽入 `CONCURRENT-WORK-PROTOCOL.md`。 |
| `NK4A-TODO.md` | 第一轮任务书（x86_64 生产启动链，`Task A`—`E`）。条目多已闭，当时的技术陷阱与记录格式仍被引用。 |
| `NK4B-TODO.md` | 第二轮任务书（三架构启动 + 命令面，`P0`—`P6`）。`NK4A-TODO.md` §5—§8 在此继续有效。 |

## 案卷（单个缺陷或专项的取证现场）

| 文件 | 作用 |
|---|---|
| `CASE-RISCV64.md` | riscv64 索引卷：三条案线地图 + 静态扫描线六轮的判定语义与移交项。只指路，不转述原件。 |
| `NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md` | 未结案卷，工作现场。riscv64 非确定性内存污染（`SD-10` / `P-RV-02` / `PD-16` 继续追缴）。第 4 章“已排除假设 + 手段分辨力边界”是防返工核心材料。 |
| `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` | 已结案卷。栈顶非规范地址案；被 `os/libs/minix-types/src/types/boot.rs` 引用，只接受勘误。 |
| `CASE-AARCH64.md` | 两起已结案归并：向量寄存器巨量分配案 + 执行换根活锁案。含已证伪假设表与会再绊人的坑。 |
| `ADDRESS-CONSTANT-AUDIT.md` | 地址常量审计账本（`A1`—`A13`）。判据表 + `tools/address-constant-scan.py` 复扫命令；被生产代码注释引用。 |
| `PATTERN-SCAN-REPORT-20260923.md` | 模式扫描账本（六族 34 条）。产出落进 `tools/pattern-gate.sh`；被门禁脚本引用。 |

## 过程记录与协议

| 文件 | 作用 |
|---|---|
| `NK4C-WORKLOG.md` | 滚动工作记录（万行级）。顶部“当前状态”必须始终最新，是长程任务的外部记忆。 |
| `REVIEW-HISTORY.md` | 评审史结论卷（19 个原件归并）。跨轮反复未修项追踪 + 每轮可复用判据 + 现场快照。状态以 `TODO-LEDGER-OPEN.md` 为准，本卷只作历史。 |
| `SESSION-HISTORY.md` | 会话史归并卷（29 个开场词 / 交接件 / 扫描清单归并）。只记交接当时的未竟项与可复用判据。 |
| `NK4C-OPENING-PROMPT.md` | 长程任务开场指令。被 `os/kernel-image/src/bootface.rs` 当裁决源引用而保留；其余内容已摘入 `SESSION-HISTORY.md`。 |
| `CONCURRENT-WORK-PROTOCOL.md` | 多会话并发工作协议的过程范例记录（非规范源）。领地切分、领取锁（`tools/claim.sh`）、跨界登记、共享文件串行化。 |
| `STAGE-KERNEL-FREEZE-READY-20261008.md` | `01-stage-kernel` 文档重生成前的冻结就绪度：15 组过期叙述回勾 + 显式推迟边界。与台账互补（台账管待办，它管冻结边界）。 |

## 编号速查

`SD-*` 结构债 43 · `PD-*` 裁决 34 · `P-*` 齐平 27 · `E-*` 跨阶段 44 · `C-*` 认领约 60 · `OQ-*` 待裁决两轮共 14 · 阶段内 `D-*`/`W-*`/`I-*`/`T-*`/`A-*`/`R-*`/`M-*`（`01-stage-kernel/todo.md`）· `S-*`（`smp_todo.md` 主线）· `K*`/`L*`（第一轮线）· `AF-*` 回溯审计 · `A1`—`A13` 地址审计 · `F*`/`G*`/`H*`/`J*` 各轮评审发现。同一件事的归组见 `TODO-LEDGER-INDEX.md` §2，不重编号。

## 被删文件的回捞

清理前 76 个文件，删除 54 个（评审类 19、会话类 28、案卷类 4、其余 3），删除前已在快照入库。按文件名取回：`git log --diff-filter=D --summary -- rewrite-notes/coordination/<文件名>` 找到删除提交，再 `git show <删除提交>^:rewrite-notes/coordination/<文件名>`。处置明细见 `TODO-LEDGER-INDEX.md` §1。
