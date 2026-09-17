# Prompt 改进规划：卓越优先（教科书文档 × 最美代码）+ 正确性与底线卫生补强

> **文件性质**：规划书（任务卡 + 前因后果 + 验收标准）。后续交给其他 AI 执行。
> **创建**：2026-09-16（2026-09-18 第四轮修订）
> **来源**：用户与 opencode 的讨论，先后四轮——
> - 第一轮：文档质量诊断 + full-review 使用诉求（对应工作流 A/B/C）；
> - 第二轮：review 流程正确性审计（对应工作流 D/E/F，2026-09-16 追加）；
> - 第三轮：代码可读性与代码架构可读性审计（对应工作流 G，2026-09-16 追加）；
> - 第四轮：锚点权重审计 + 卓越层补位（对应 D5 改写与 H/I 批，2026-09-18 追加）。
>
> **北极星（第四轮明确的用户标准）**：最高目标是**最美的代码 + 教科书的文档**（第一流技术博客级），规则为此服务。行号/锚点类规则是 AI 为自己加的校验脚手架——留在底线层（卫生项：批量做、不占轮次），不得进入卓越性的房间。判断任何规则、门、度量是否值得做，先问"读者或代码因此变好了吗"；答不上来的，降级或删除。
>
> **不做什么**：本文件只是计划，它本身不修改规则、不修改文档、不修改代码。第四轮明确**不新增以行号为中心的强制门**，原"行号自动化"方案作废，改为一次性符号锚点迁移（D5 改写）。

---

## 零、执行者先读这一节

### 0.1 启动清单（按顺序读，不要跳）

| 顺序 | 文件 | 读它是为了 |
|---|---|---|
| 1 | `CLAUDE.md`（仓库根） | 项目约束、Hidden Folder Convention、Gate 体系 |
| 2 | `.claude/rules/review-core.md` | 执行模型、Ground Truth 优先级、P0 六分类 |
| 3 | `.claude/rules/review-process.md` | Step 0-7、Gate 证据规则、同 agent 验证局限 |
| 4 | `.claude/rules/fix-guard.md` | 修复规范（本计划要给它加第 5 条） |
| 5 | `prompt/README.md` | 三端同步机制（改规则后必须同步哪些文件、跑哪些脚本） |
| 6 | `prompt/review-rules/review-cmds.md` | 任务命令规范（本次要改的主要源文件之一） |
| 7 | `prompt/review-rules/review-process.md` | 流程权威定义（本次要改的另一个主要源文件） |
| 8 | `prompt/review-rules/review-code-checklist.md` + `prompt/review-rules/review-code-excellence.md` + `prompt/review-rules/review-doc-excellence.md` | 工作流 B/D 的代码维度来源；H 批的卓越性判据来源 |
| 9 | `os/qemu-tests/README.md` | 工作流 D1 的 QEMU 集成测试入口与 PASS 标记格式 |
| 10 | `os/README.md` + `os/arch/README.md` | 工作流 G3 的架构总览、依赖图与设计理由（漂移检查对象） |
| 11 | 本文件 | 任务卡与验收标准 |

### 0.2 执行原则

1. **单一真相源**：规则只改 `prompt/review-rules/` 与 `prompt/skill/`（适配层）。禁止在 `.claude/`、`.codex/`、`.trae/` 里手写一套新规则——那三个目录是派生端。同步用脚本（见 `prompt/README.md` 与本文工作流 C）。
2. **一次一个任务，且先有触发**：任务只有在你（用户）提出改进需求之后才执行（触发方式见 0.8）；执行时按第十一部分（任务汇总）的依赖顺序逐个推进，每个任务有独立验收命令。不要一轮做完所有任务，也不要因为"它在计划里"就去做。
3. **每个任务留证据**：每个任务的验收命令输出（命令 + 结果片段）必须贴进交付说明。只写"已完成"不算完成。
4. **不改存量文档正文**：工作流 A 只做"止损 + 增量门"，不批量重写现有文档。存量清单只登记（见 A4）。
5. **不碰 `minix3/`**：那是 ground truth C 源码，只读。
6. **不动 `prompt/skill/review-agent-ide.md` 的篇幅**：该文件距 Trae 的 10,000 字符硬上限只剩约 250 字符余量（`prompt/README.md` 有实测表）。如果某任务确实需要改它，先精简等量内容再加入。
7. **不 commit**：除非用户明确要求。
8. **卓越优先，机械让位（第四轮）**：任务排序、注意力分配与验收标准都以"最美的代码 + 教科书的文档"为准。锚点、行号、格式等机械项归卫生层：批量做、不占 review 轮次、不进收敛计数。新增任何强制检查必须同时说明它保护读者的什么价值，并降级或合并一个存量机械检查，或明确标注"不占轮次"——这是注意力预算（默认决策见 0.3 第四轮）。
9. **静态计数只作快照**：本文件里的"N 个文件 / N 处 / 行数"都是诊断时点数字，会随并发改动漂移；执行任何任务前先用 `rg` 实测（B4.2/E2 已如此要求，本原则推广到全文）。规则文本以实测结果为准，不以本文件的静态数字为准。

### 0.3 默认决策（用户不在场时按此执行；用户在场则先确认第十二部分的 Open Questions）

第一轮（工作流 A/B/C）：

- 文档头字段统一名：`Rust 实现`（`Rust 模块` 作为兼容别名接受，不强制立刻改存量）。
- 门禁形态：不新增 Gate 字母；把"关联代码清单"并入 Gate 0 已有的 `§Step 0: 预检结果` 段，把"代码维度检查"作为新 Step 3.6 的产物 + `gate-evidence-code` 证据块。
- 文风 lint 对存量：只报告、不阻断；对新增/修改行（`--diff`）：阻断。
- 演练文档：`notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`。

第二轮（工作流 D/E/F）：

- QEMU 门触发面 = 以 D1 列出的 CI 路径清单为准（`os/kernel/**`、`os/arch/**`、`os/plat/**`、`os/libs/{minix-types,minix-boot,minix-elf,minix-platform}/**`、`os/boot-shim/**`、`os/qemu-tests/**` 的非测试代码改动，或文档声称的行为依赖真实硬件路径）；其余写 `N/A + 理由`。不要另造更窄的列表。
- unsafe 审计：存量基线冻结（只登记不修），增量（`--diff`）裸 unsafe 阻断；Miri 先登记 OQ，不默认引入。
- zero-P0 独立复核：单 agent 环境用"跨 session 换模型 + 随机章节完整重跑"兜底；有跨工具条件时优先跨工具只读复核。
- 测试正确性抽样：按分层抽样取 30%（至少 3 个，含 1 个错误路径）。
- 正确性相关 P1：30 天时限，逾期必须修或升级为 OQ。
- 契约覆盖矩阵：替代"Gate B 只看 Top 5"——§2.3 每个函数必须归档（契约表 / 语义未变 / ARCH / 缺口）。
- `[ARCH]` 外部锚点：先限四类机制（异常/中断/分页/时钟与特权级切换），其余领域留观察。

第三轮（工作流 G）：

- 代码可读性 lint：只查新增/修改行（`--diff`）并阻断；存量只报告 + 基线冻结，不做批量重写。
- 注释语言：未裁决前中文注释检查默认 warning，不阻断（裁决见 OQ19）。
- `missing_docs`：全局先 `warn`，不 `deny`；升级 deny 逐 crate 决策（OQ18）。
- 依赖方向：按 `os/README.md` 现状冻结规则，发现违例登记 OQ，不擅自改结构。
- 卓越性提级只对"新增代码"生效（存量仍为 backlog）。

第四轮（D5 改写 + H/I 批）：

- 锚点形态：**符号锚点**（`path:fn name`、`path:struct Name`、C `path.c:func`）取代行号锚点；行号只允许写成工具派生的 `（Lnn，工具生成）`，禁止手工维护。
- 卫生项分层：锚点漂移、拼写、全角/半角、格式等机械项在 scan.md 单列"卫生项"，只记录 + 批量修；**不进 P1/P2 计数、不进 `weighted_new`、不参与 Step 7.1 收敛判定**。
- 复述测试：structure.md §11 升级为必答产物（复述结论进 scan）；Ch1 复述不通过 → 教学类 P1（新增/触碰章节必须修复或写明延期理由；存量登记；H 批完成以 H4 为准，见 OQ25/OQ29）。
- 非法态测试：每个核心状态机 / trait 边界必须产出"非法态封堵清单"；封不住 → 设计类 P1。首批验证样本 = 对话中识别的三处可疑设计（wire 布局裸常量、unsafe 散装、PM Lifecycle 半封装），执行时先复核具体位置。
- 代码块审查：文档中的每段 Rust 代码块（rust 围栏）必须分类（逐字引用 / 教学简化 / 反面示例 / 签名示意）并给出最佳实践判定；未标注的反模式 → P1。
- 内容优先：`18-stage` Requires 回填（P1-1）与 99 汇总篇重写一旦被触发，插到锚点卫生批次之前（I1/I2；默认顺序见第十一部分）。

### 0.4 工作量预估（供另一个 AI 排期，非硬性）

| 任务 | 预估 | 类型 |
|---|---|---|
| A1 规则修订 | 1 小时 | 文档 |
| A2 `doc-style-lint.sh` | 2-3 小时 | 工具 |
| A3 增量门接入 | 1 小时 | 文档 |
| B1 关联代码声明规范 | 1 小时 | 文档 |
| B2 `doc-code-map.sh` | 2-3 小时 | 工具 |
| B3 流程修订（Step 0 / Step 3.6） | 1-2 小时 | 文档 |
| B4 门禁脚本扩展 | 1 小时 | 工具 |
| C1 三端同步 | 1 小时 | 流程 |
| C2 回归 | 0.5 小时 | 流程 |
| B5 端到端演练 | 1-2 小时 | 演练 |
| D1 QEMU 门 | 1 小时（坚持单例；CI 超时 45 分钟） | 文档 |
| D2 `unsafe-audit.sh` | 2-3 小时 | 工具 |
| D3 coverage 多域 | 2-3 小时 | 工具 |
| D4 证据脚本化 | 2 小时 | 工具 |
| D5 符号锚点迁移 + 卫生分层（2026-09-18 改写，原行号自动化作废） | 3-4 小时 | 工具 + 规则 |
| D6 正确性门演练 | 1-2 小时 | 演练 |
| E1 zero-P0 独立复核 | 1 小时 | 文档 |
| E2 测试正确性门 | 1-2 小时 | 文档 |
| E3 正确性 P1 时限 | 1 小时 | 文档 |
| F1 design C 锚点 + 契约矩阵 | 2 小时起（符号多的模块应上调，见执行注意） | 文档 |
| F2 `[ARCH]` 外部锚点 | 1 小时 | 文档 |
| G1 `code-style-lint.sh` | 2-3 小时 | 工具 |
| G2 missing_docs + CI 扩展 | 2-3 小时起（全 workspace 需分批摸，见执行注意） | 配置 |
| G3 架构可读性验收（漂移 + 依赖方向 + 路径抽查） | 3 小时 | 工具 + 文档 |
| G4 可读性提级（新增代码 P1） | 1 小时 | 文档 |
| H1 读者复述测试提级 | 1-2 小时 | 规则 + 演练 |
| H2 非法态不可表达测试 | 2 小时 | 规则 + 演练 |
| H3 文档 Rust 代码块逐块审查 | 2-3 小时 | 工具 + 规则 + 演练 |
| H4 卓越样章示范（文档 + 模块） | 3-4 小时 | 演练 |
| I1 18-stage Requires 回填 | 逐批（每批 1 次 todo-fix） | 内容 |
| I2 99 汇总篇重写 | 3-5 小时 | 内容 |

### 0.5 证据块统一格式（所有 `gate-evidence-*` 适用）

B3.3/D1/D2/D4/E2/G3 都会写证据块，为避免各写各的，统一格式如下（放进 scan.md 时用围栏块，名字 `gate-evidence-{type}`）：

````
```gate-evidence-{type}
command: {实际执行的完整命令}
exit: {退出码}
stdout_key: {关键输出 3-5 行，或计数摘要}
artifact: {磁盘产物路径（log / 报告 / SYMBOLS / 基线）；无产物写 none}
date: {YYYY-MM-DD}
note: {可选：环境限制 / N/A 理由 / 工具缺口}
```
````

约定：

- type ∈ {A, H, code, unsafe, qemu, test, dep, ...}；同一次 review 内同一 type 只写一个块（多次运行合并）。
- `command` 必须是可重放的原文（不许写"已运行"）。
- `artifact` 指向的路径必须真实存在（D4 起由 `review-gate-check.sh` 核对）；纯文本证据（如 grep 结果）写 `none`。
- N/A 的块仍要写：`command: N/A`、`note: 理由`，不允许整个块缺失。

### 0.6 兼容与回滚总则（所有任务适用）

规则类改动（改 `prompt/review-rules/`、`prompt/skill/`、`.claude/rules/`，或改 `CLAUDE.md`/`AGENTS.md` 里的复述段）统一遵守三条：

1. **只对新产物生效**：新规则从它落地的那个 commit 起约束之后新产生的 review 产物；不回溯、不重跑、不迁移旧 `scan.md`/`VERIFY-CHECK.md`/`STATE.md`。OQ13 的"不回扫"与 OQ16 的"不迁移"是这条总则的特例，不是各任务自己的局部约定。
2. **新旧并存期只警告不报错**：新工具与扩展后的门禁脚本遇到旧格式产物时只给 warning（沿用 A2 任务卡的既有执行注意）；引擎不得因旧产物直接失败。
3. **回滚定义**：回滚 = 撤销规则文本改动 + 重跑 `tools/generate-derived-skills.sh` + `tools/check-review-rules.sh` + `tools/lint-review-rules.sh`；三个脚本全绿即回到基线。已经产出的 review 产物不回滚——它们如实记录了当时的规则版本。

落地 commit（= 生效起点）与需要时的回滚 commit 写进执行记录的任务状态行，不要求另外手写日期。

### 0.7 计划生命周期

- **定位**：本文件是执行期工作文件，不是长期规则。正式文档（`CLAUDE.md`、`AGENTS.md`、`prompt/README.md`、`notes/rewrite/` 下的文档）不得引用它——这与 Hidden Folder Convention 禁止正式文档引用中间产物的精神一致。
- **完成判定**：执行记录的任务状态全部为已完成（或有明确的移出范围裁定），度量对比表填齐，遗留问题逐条登记去向（修订 OQ 表 / `edge_todo.md` / `.review/BACKLOG.md`）。
- **完成后去向**：任务全部清空后，如果本文件仍在被使用（后续改进需求继续挂在这里），就把它收敛成一份持续维护的服务层候选池——继续留在 `prompt/` 目录内，可以改名；如果不再使用，直接删除。服务层的记录只属于 `prompt/`，不得迁往 `notes/` 目录。无论哪种去向，都不允许留下一份没人维护、继续腐化的计划文件。
- **中途停止**：保留本文件，并在执行记录写清"停在哪一个任务、下一步是什么、被什么阻塞"。恢复执行前先重跑 C2 的三个脚本，确认基线没有被其他并行会话改动。

### 0.8 触发方式：你在对话里提出改进需求（本计划不按批次推进）

本计划是服务层的候选池，不是排期表，也不是一套流程。第十一部分回答"有哪些可做的事"，本节只回答"这份文件是怎么被用起来的"。任务数量是候选池规模，不是一次性工作量承诺；任何时刻只执行被触发的那一个任务（0.2 第 2 条）。

**唯一的触发方式**：你在真工作里发现问题后，直接向 AI 提问，典型句式：

> "X 这里不对 / X 总是有问题，你阅读一下当前的 `prompt/` 目录，然后思考一下如何改进。"
>
> 也可以直接给出北极星式的问题（第四轮的标准）：
>
> > "这个文档/代码能不能达到教科书的文档、最美的代码？规则有问题就改规则。"

你不需要先填任何表格，本项目里也不会有人替你维护执行记录——提出问题本身就是触发。

**AI 收到问题之后的动作**：

1. 读 `prompt/` 与本文件，把问题对应到已有的候选任务（用下面的对照表快速定位），对应不上就新增一条候选。
2. 把"问题现象、候选改法、涉及哪些文件"讲清楚，与你讨论确认。
3. 你确认之后才执行候选任务。规则类的落地按 C1/C2 做同步与回归；如果是新检查规则，按 `prompt/review-rules/review-process.md` 的检查项注册表加一行，不新开 Step 章节（元规则）。

**常见现象与候选任务对照**（供 AI 定位，是人工查表，不是检测器，也没有自动记录）：

| 真工作里出现的现象 | 对应任务 |
|---|---|
| 正式文档正文出现 review 编号、修复日期、迭代史 | A 批（先跑 A2 的文风 lint） |
| full-review 只审了文档，关联代码没有产物证据 | B1 + B3 |
| 文档引用的行号发生漂移 | 卫生批次（批量换成符号锚点，不占轮次；见 D5 改写） |
| 关键路径代码有裸 unsafe、没有 SAFETY 说明 | D2 |
| 内核启动 / 中断 / 分页 / BKL 改动后没有真实硬件路径证据 | D1 |
| 覆盖率缺口、C 符号漏审 | D3 与覆盖率专项 |
| 测试虚设、断言写错、文档描述的测试不存在 | E2 与 test-audit |
| design 快照缺少 C 源码锚点 | F1 |
| 新读者看不懂模块结构、注释、依赖方向 | G1 / G3 |
| `tools/check-review-rules.sh` 或 `tools/generate-derived-skills.sh --check` 报漂移 | 立即按 C1/C2 处理 |
| 文档读完讲不出机制、只有函数名流水账 | H1（复述测试） |
| 核心状态机靠约定不靠类型封堵 | H2（非法态测试） |
| 文档里的 Rust 示例教了坏习惯 | H3（代码块审查） |
| 命令缺哪些底层 API 说不清 | I1（Requires 回填） |
| 新人读不懂系统结构 | I2（99 汇总篇重写） |

**明确不做的事**：

- 本节不引入新流程：不设登记表、不设检测器、不设审批链；`.review/` 里的 `scan.md`、`STATE.md`、`SYMBOLS.md` 依旧只是 review 自己的过程与准入准出记录。
- 不与 review 侧已有的 Rule Discovery 机制重叠（`prompt/review-rules/review.md` §规则演化机制、`prompt/review-rules/review-process.md` Step 5.7）——那条只服务"review 中发现 ≥2 次的同类新错误模式"，本计划只服务服务层工具的改进。

**执行候选任务之前（并行会话隔离）**：真工作里经常有别的会话在跑（例如逐条处理 `edge_todo.md`、扫描各个 stage 的 `todo.md`）。动手前先确认没有会话正在消费将被修改的文件；有的话选安静窗口执行，变更说明里列出受影响的文件与会话，改完立刻跑 C2 回归并写明"已同步"。

---

## 一、背景：为什么要做这些改进（前因后果）

### 1.1 触发点

用户的两个观察：

1. **文档质量远低于出版物和一流技术博客**。`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/01~05` 是人工审阅过的文档，`02-stage-vm` 也有一部分人工审阅过；但整体仍然达不到"可以直接发出去的教程/博客"水平，未审阅的文档更需要逐字句纠正。
2. **`full-review` 的使用预期没有被满足**。用户常用 `full-review` 的目的，是"某个文档和它关联的 Rust 代码一块 review"；实际执行时，代码侧是否被审、审到什么程度，取决于执行 AI 的临场判断，没有硬性产物证据。

### 1.2 文档质量问题的根因（不是写作能力问题，是流程的奖励函数问题）

现有评审体系把"可审计"当第一目标，把"好读"当副产品。具体机制：

- **Blocker Gates 不管文风**。Gate 0/A/B/C/D/D-6/E/G/H 全部验证据链（锚点、覆盖率、行为契约、design 对齐、测试对账），没有一条检查行文质量。Gate 定义见 `.codex/skills/review-scan/SKILL.md` 的 Blocker Gates 表与 `prompt/review-rules/review-process.md` 的 Gate 权威注册表。
- **`full-review` 明确不做文风重写**：`prompt/review-rules/review-cmds.md` 第二节"不做边界"写着"不做文风重写（发现开发文档味 → 记录，引导 `style-fix`）"。
- **可读性检查大多判 P2**：`prompt/skill/review-doc-skill.md` §3 可读性条目（§3.1-§3.4）整体判 P2，而 P2 不阻塞 CONVERGED。
- **style-bible 只是声明**：`prompt/skill/cmds/style-bible/SKILL.md` 的六条约束没有任何机器化执行，也没有接入任何 Gate。

结果：每跑一轮 review，正文里就多沉积一层"审计痕迹"。这不是假想，有数据：

- **review 内部编号**（形如 `V12-P1-2`、`FIX-20`、`D-13`、`P0-3`）在两个 stage 的正式文档中约 330 处（kernel 约 107、VM 约 222；统计命令见附录 2）。
- **正文里的日期**约 300 余处（其中文档头 `> **创建**:` / `> **重写**:` 是合法的，lint 必须排除）。
- 典型样本：
  - `01-stage-kernel/13-syscall-dispatch.md:884`："r2 回归 review 中实施了 4 项卓越性改进"
  - `01-stage-kernel/15-clock-timer.md:140`："（**注意：旧文档误标为 0**）"
  - `01-stage-kernel/05-clock-interrupt-init.md:600`："**行为变更声明（2026-08-15，V3 P0-1；...D-59 于 2026-09-09 修复...）**"
  - `02-stage-vm/09-slab-allocator.md:132`："**A-3 v2（2026-08-16，todo P1-1）**：v1 用纯 bump…v2 改为 free-list"
  - `01-stage-kernel/06-proc-init-boot-proc.md:519`："**本表不列的存储决策**…不替后续分组章节抢具体字段的存储与寻址细节。"（写作策略元注释）
  - `01-stage-kernel/06-proc-init-boot-proc.md:504`："**本文档不展开评估上述方向的工程细节**"

这些写法同时违反 `prompt/skill/cmds/style-bible/SKILL.md` 的第 2、3、5 条（禁压缩简写/黑话、禁开发文档味、中间结果也要清晰），但没有任何机制在阻止它。

### 1.3 用户已明确的优先级排序

1. **正确性是第一位**：编辑阶段的"逐字句 line-edit 战役"延后，不在本轮执行。
2. **但止损现在就做**：不阻止新增沉积，未来的编辑成本只会越来越大。所以本轮只做"规则 + 增量门 + 工具"，不做存量重写。
3. **full-review 要名副其实**：文档和它关联的 Rust 代码一起审，并且要有产物证据。

### 1.4 目标与非目标

**目标**：

- 工作流 A：让"过程痕迹进正文"这件事从"没人管"变成"写入时即被拦截"，并提供可执行的检查工具。
- 工作流 B：让 `full-review` 在文档范围内默认审查"文档 + 关联 Rust 代码"，关联代码清单与代码维度结论必须出现在产物里。
- 工作流 C：所有规则改动三端同步（`prompt/` → `.trae/` + `.codex/` + `.claude/` 对应的派生位置），并通过回归脚本。
- 工作流 D（第二轮）：把正确性证据从"读代码 + hosted 单测"升级为"可执行 + 可核对磁盘事实"——QEMU 冒烟、unsafe 审计、覆盖率多域、Gate 证据脚本化；行号自动化在第四轮改写为符号锚点迁移（D5 改写）。
- 工作流 E（第二轮）：补独立验证与判定收敛的漏洞——zero-P0 也要独立复核、测试正确性进 P0/Gate、正确性 P1 设时限。
- 工作流 F（第二轮）：给设计快照与架构演进补外部权威——design 决策附 C 锚点、契约覆盖矩阵、`[ARCH]` 断言附 spec 锚点。
- 工作流 G（第三轮）：补代码可读性与代码架构可读性——新增代码的可读性止损（lint + rustc/clippy 配置）、架构可读性验收（README/`//!` 漂移、依赖方向、新读者路径）。
- 工作流 H（第四轮）：给卓越层补可执行判据——读者复述测试（H1）、非法态不可表达测试（H2）、文档 Rust 代码块逐块审查（H3）、卓越样章示范（H4）。
- 工作流 I（第四轮）：内容优先——18-stage Requires 回填（I1）、99 汇总篇重写（I2）。

**非目标（本轮不做）**：

- 不批量清洗存量文档里的 review 编号/日期/修复史（只出清单，见 A4）。
- 不重构正式文档模板（Claims-Evidence 表格等章节是否移出正文，留待编辑战役统一决定）。
- 不给文风加全量 Blocker Gate（会让所有旧文档当场 FAIL，与"正确性优先"的排序冲突）。
- 不改变 review 的事实核验标准与 P0/P1/P2 判定标准（工作流 E2 新增 P0-test-wrong 属于分类补充，不是判定标准变更）。
- 不引入 Miri/loom 等需要 nightly 的动态验证工具（登记为 OQ10，由用户决定是否另立项目）。
- 不批量重写存量代码的注释/命名/文档（工作流 G 只做增量止损 + 基线登记；存量治理另立批次）。
- 不新增以行号为中心的强制门或工具（第四轮作废"行号自动化"方向，改为符号锚点迁移；行号漂移只进卫生项）。
- 不做存量文档的卓越性全量阻断（H 批只对新增/触碰面强制，存量登记 backlog；与 OQ21 同哲学）。

### 1.5 第二轮触发点：正确性保证审计（2026-09-16 追加）

用户追问"当前为保证正确性，review 流程是否还有缺陷"。审计结论：流程对**文档正确性**的保证已经相当硬（覆盖、design 对齐、锚点、修复规范都有门），但对**代码行为正确性**与**独立验证**两层仍是软肋。九个缺口与对应任务：

| # | 缺口 | 证据 | 对应任务 |
|---|---|---|---|
| 1 | 代码维度没有 Blocker Gate：Gate D 只做 5 项存在性 grep，code checklist 只"加载"不要求执行证据 | `prompt/review-rules/review-process.md` Gate 注册表；扫描产物无 `gate-evidence-code` 要求 | B3.3 + B4（第一轮已覆盖），D4 补充证据脚本化 |
| 2 | 没有可执行/差分的行为等价证据：无 Miri/loom/proptest；约 1774 处 unsafe 无清单无门；qemu-tests 存在但 review 规则零引用 | 全仓 grep（Miri/loom 仅草稿提及）；`os/` unsafe 构造 1774 / 206 文件，SAFETY 注释约 848；`os/qemu-tests/run_all.sh` 在 `prompt/` 无引用 | D1 + D2 |
| 3 | Gate A 机器覆盖与文档语义域错位（实测） | `.review/codex/sched/12-kernel-interface/scan.md:42` 写明脚本只覆盖 `servers/sched`，本篇 C 域在 `kernel/`，人工补表后仍 PASS | D3 |
| 4 | 独立验证默认关闭：跨 agent 是"推荐"、`--require-multi-agent` 默认 false、只在 P0≥1 时强制；P0=0 只需同 agent 重放 | `prompt/review-rules/review-process.md` Step 5.6 多 agent 表 | E1 |
| 5 | design 快照自产自审：Gate H 校验的 design 由同一 AI 本轮推导，理解错了会三方一致地错；契约表只有 Top 5 抽样 | Step 0.3 + Gate H；Gate B 定义 | F1 |
| 6 | 证据自证：gate-evidence 是执行者粘贴文本；`tools/verify-check.py` 只查关键字存在，不验真 | `tools/verify-check.py` 功能说明；Gate Evidence Rule | D4 |
| 7 | 测试正确性不在门里：有 P0-test-missing 无 P0-test-wrong；test-audit 不阻塞 CONVERGED；Gate E 只 grep 测试名 | `prompt/review-rules/review.md` P0 表；`review-cmds.md` §五（自记"测试代码就是错的"事故） | E2 |
| 8 | 收敛允许带 P1 交付，而 P1 含语义/安全类问题，且无到期复审 | `prompt/review-rules/review.md:541-547`；Step 7.1 停止规则 | E3 |
| 9 | 注意力错配：强制项集中在文档侧，代码维度无里程碑；行号自动化 Proposal #7 仍是"待开发"——**第四轮修正：错配的根源正是把行号当成了目标；正确处置是卫生分层 + 卓越判据，不是继续自动化行号** | CLAUDE.md 累积改进表（6 篇文档 25+ 处行号漂移）；sched/12 那轮唯一发现是行号偏移 | D5（改写）+ H1/H2/H3 |

> 第 1 项已由第一轮的 B3.3/B4 承担，D4 只补"证据必须来自磁盘事实"这一层；其余八项是本轮新增工作流 D/E/F 的来源。

### 1.6 第三轮触发点：代码可读性审计（2026-09-16 追加）

用户追问"原有架构中有没有代码可读性、以及代码架构可读性"。审计结论：**规则里有覆盖，但和文档文风此前一样——没有门、没有工具、没有验收**；代码架构可读性更弱，有素材（README、`//!`）但没有标准。五个缺口与对应任务：

| # | 缺口 | 证据 | 对应任务 |
|---|---|---|---|
| 1 | 代码可读性规则存在但没有门：§9.1 写"强制"（每个 pub 有 `///`、模块有 `//!`），但 Gate D 只做 5 项存在性 grep，没有任何 Step 要求可读性证据 | `prompt/review-rules/review-code-checklist.md:216-232`；Gate D 定义 | G1 + G4 |
| 2 | rustc/clippy/CI 缺位：全仓无 `missing_docs`；无 `clippy.toml`/`rustfmt.toml`/`[lints]`；CI 只有 `vm-tests.yml` 对 `minix-vm` 跑 `clippy -D warnings`，kernel/arch/servers 不在 lint CI 内 | `rg missing_docs os` 零命中；`.github/workflows/` 目录；CI 配置 | G2 |
| 3 | 没有代码版 style-bible：文档侧有 style-bible/style-fix + A2 增量 lint；代码侧没有"代码风格宪法"，也没有 `code-style-fix` | `prompt/review-rules/review-cmds.md` §六只有文档文风；无代码对应 cmd | G1 |
| 4 | 注释语言规则与现实冲突未裁：§9.2 写"注释必须使用英文（这是原则，不是建议）"，但 `os/` 里约 4267 行中文注释无任何拦截 | `review-code-checklist.md` §9.2；统计命令见附录 2 | G1 + OQ19 |
| 5 | 代码架构可读性无验收：`os/README.md`（架构总览 + 依赖图）与 `os/arch/README.md`（ISA vs board 设计理由）质量不错，但 review 规则零引用；无依赖方向检查；无"新读者路径"检查 | `rg "os/README" prompt/` 零命中；`tools/` 无 dep 脚本 | G3 |

> 与第一轮同哲学：增量止损 + 存量登记，不做批量重写。存量素材其实不差（1010 个 `.rs` 有 1000 个带 `//!`，约 4.97 万行 `///` 对约 9900 个 pub 项），问题在"没有机制防止它退化"。

### 1.7 第四轮触发点：锚点权重审计与卓越层补位（2026-09-18 追加）

用户的最终标准：**行号最不重要，文档要教科书级、代码要美**。审计结论：现有规则把"AI 可校验"误当成了卓越，行号族规则正是这个错觉的产物；而卓越层没有可执行的判据。四个证据与四条纠正：

| # | 证据 | 纠正 |
|---|---|---|
| 1 | 行号/锚点族规则（模式 66 RCPD、73-77，加 Step 1.0a~1.0g）集中诞生于 2026-07-16 与 2026-07-30~31；Blocker Gates 9 个里 4 个（A/D/E/H）本质是引用/存在性检查；D5 原方案还要把行号检查全量化 | **卫生项分层**：锚点漂移/格式等机械项批量修，不进 P1 计数、不进收敛判定；D5 改为一次性符号锚点迁移（`path:fn name`，行号只作工具派生提示） |
| 2 | 文档侧唯一进入 Blocker Gate 的教学性检查是 structure.md 12 节（复述等 12 项与正确性项混在同一张判定表，没有独立证据要求）；`review-doc-excellence.md §4.1-4.5` 虽有 A/B/C/D 分级但明确不阻塞（`:319`）；代码侧 `review-code-excellence.md:28` 的"非法态不可表达"只是 [MEDIUM] 检查项 | 补两条可执行判据：**读者复述测试**（Ch1 不看代码讲出机制，H1）与**非法态不可表达测试**（每个核心状态机说出它用类型封住了什么，H2），配样章示范（H4） |
| 3 | 读者学习 Rust 的入口是文档里的代码块，但现有模式 73 / Step 1.0b 只查"与源码同步"，不评判示例本身是否 idiomatic | **文档 Rust 代码块逐块审查**（H3）：逐字引用/教学简化/反面示例/签名示意四分类 + 最佳实践判据 |
| 4 | 对话明确的优先项在计划里零提及：`18-stage` Requires 回填（P1-1）与 99 汇总篇重写决定"命令能不能跑、新人能不能读懂" | 新增工作流 I（内容优先）；被触发时排在锚点卫生批次之前（默认顺序见第十一部分） |

> 与前三轮的关系：A/B/D1/D2/E2/E3 的正确性建设保留不动（正确性优先）；本轮的改法是"加卓越 + 降机械"，不是"减正确性"。

---

## 二、工作流 A：文档文风止损与增量门

### A0. 判定标准（先把"什么算违规"定死）

**正式文档**指 `notes/rewrite/**/*.md` 下编号文档（如 `01-boot-shim-bootstrap.md`、`15-ipc-dispatch.md`）。以下内容禁止进入正式文档正文：

| 类别 | 例子 | 为什么禁止 |
|---|---|---|
| review 编号 | `V12-P1-2`、`FIX-20`、`D-13`、`R-05`、`W-7`、`P0-3` | 读者不知道这些编号指什么 |
| review 日期 | "（2026-08-15 review）"、"2026-09-09 修复" | 读者关心机制，不关心修复时间线 |
| 修复史叙事 | "原实现将…"、"已修复"、"旧文档误标为"、"v1 用…v2 改为…"、"修复前" | 开发文档味（style-bible 第 3 条） |
| 写作策略元注释 | "本表不列…"、"本文档不展开…"、"论证归属…"、"不替后续章节抢…" | 元注释泄漏：作者在说写作安排，不是在讲知识 |
| review 工具术语 | `scan.md`、`STATE.md`、`VERIFY-CHECK`、`Pattern #66`、`模式 25`、`Gate H`、`ground truth` | 内部流程词汇 |

**允许保留**（lint 不得误报）：

- `[ARCH: ...]` 架构演进标注（三处一致是项目硬要求）。
- `TODO` / `DEFERRED` 标记（由既有 Doc-Sync-4 流程管理）。
- 文档头 `> **创建**:` / `> **重写**:` 行里的日期。
- 事实断言的锚点（符号锚点优先：`path:fn name`；锚点纪律是硬要求，不在禁止范围）。
- 文档内交叉引用 `§X.Y`、`NN-doc.md`。
- 方法/机制本身就叫"模式"的词（保护模式、实模式、长模式）——lint 只匹配 "模式 + 数字"。

**修复既有文档时的正确写法**（替代"打补丁再记一笔"）：

- 写成"如果 X 会 Y，所以 Z"的假设性推理，或直接给正确表述。
- "何时发现、哪个编号修的"写进 `.review/` 下的 scan.md、fix-status，或 `todo.md`，不写正文。

### A1. 规则修订（源 + 适配层）

**任务**：把 A0 的判定标准落到规则文本里，共 5 处源文件。

| # | 文件 | 改什么 |
|---|---|---|
| A1.1 | `prompt/review-rules/review-cmds.md` §一 通用强制门第 3 条 | 在"文风门"条目后补一句：正文禁过程痕迹；触碰正式文档的修复必须通过 `tools/doc-style-lint.sh --diff`；详见 §六 第 7/8 条 |
| A1.2 | `prompt/review-rules/review-cmds.md` §六 style-bible | 六条扩为八条：第 7 条"正式文档只面向读者，过程痕迹（编号/日期/修复史/元注释/工具术语）禁止入正文"；第 8 条"修复文档用假设性推理或直接正确表述重写，不追加'已修复（Px-y）'式补丁句" |
| A1.3 | `prompt/skill/cmds/style-bible/SKILL.md` | 与 A1.2 同源同步：六条扩为八条（保持与 review-cmds §六 文本一致） |
| A1.4 | `prompt/review-rules/review-doc-checklist.md` §3 可读性 | 新增检查项"过程痕迹扫描（强制）"：跑 `tools/doc-style-lint.sh {doc}`；命中项按来源判级——本次修改引入的判 P1，存量判 P2 |
| A1.5 | `.claude/rules/fix-guard.md` | "Before EVERY fix" 扩为 5 条：文档类修复不得把过程痕迹写进正文；提交前跑 `tools/doc-style-lint.sh --diff`；过程信息写进 report 的 fix-status |
| A1.6 | `prompt/skill/review-doc-skill.md`（适配层） | 与 A1.4 同步（该文件是 `.trae/`/`.codex/` 派生源） |

**执行注意**：

- 先 grep 确认目标段落现状，再改（fix-guard 第 1/2 条）。例如：`rg -n "文风门|不做文风重写" prompt/review-rules/review-cmds.md`。
- 规则文本里引用 lint 时只写工具名和用法，不复制一整张正则表（避免两份真相源）。
- `.claude/rules/review-core.md` 里若有重复的文风条款，一并指向同一句（执行时先 `rg -n "文风" .claude/rules/` 确认）。

**验收**：

```bash
rg -n "过程痕迹|doc-style-lint" prompt/review-rules/review-cmds.md \
  prompt/review-rules/review-doc-checklist.md prompt/skill/cmds/style-bible/SKILL.md \
  .claude/rules/fix-guard.md prompt/skill/review-doc-skill.md
# 预期：5 个文件都有命中，且措辞一致（"过程痕迹"一词口径统一）
```

### A2. 工具：`tools/doc-style-lint.sh`

**任务**：写一个 bash 检查脚本（仓库工具统一用 bash + `rg`，不引入新依赖）。

**CLI**：

```
tools/doc-style-lint.sh [FILE...]            # 全量检查给定文件
tools/doc-style-lint.sh --diff [RANGE]       # 只检查新增/修改行；RANGE 传给 git diff（默认 HEAD）
tools/doc-style-lint.sh --dir DIR            # 检查目录下所有 .md（跳过 archive/draft/.design）
tools/doc-style-lint.sh --strict             # 额外启用 warning 级规则（默认只报 error 级）
tools/doc-style-lint.sh --self-test          # 内置正/反例自测，exit 0 = 自测通过
```

退出码：`0` = 无 error 级命中；`1` = 有命中；`2` = 用法/环境错误。

**规则表**（error 级默认阻断；warning 级只在 `--strict` 下报）：

| ID | 级别 | 正则（示意） | 说明 |
|---|---|---|---|
| SL-1 | error | `\bV[0-9]+-(P[0-9]+\|A[0-9]+\|T[0-9]+)(-[0-9]+)?\b` | review 批次编号 |
| SL-2 | error | `\bFIX-[0-9]+\b` | 修复条目编号 |
| SL-3 | error | `\bP[0-9]+-[0-9]+\b`、`\bD-[0-9]+\b`、`\bR-[0-9]+\b`、`\bW-[0-9]+\b` | issue 编号（注意：不要收录裸 `P0`/`P1`，会误伤 §5 测试优先级表） |
| SL-4 | error | `20[0-9]{2}-[0-9]{2}-[0-9]{2}` | 正文日期；出现在 `> **创建**` / `> **重写**` 行时跳过 |
| SL-5 | error | `旧文档\|旧版\|已修复\|修复前\|原实现\|旧实现\|前版\|原先` | 修复史叙事；**不含裸"最初/曾经"**——合法的 C 历史叙述（如"Minix3 最初把…"）不能用 error 级误伤 |
| SL-5b | warning | `最初\|曾经\|后来` | 单独出现时可能是教学需要的设计历史（见 `review-doc-excellence.md §4.3 历史背景`），只在 `--strict` 下报告，人工判断是设计史还是修复史 |
| SL-6 | error | `本表不列\|本文档不展开\|不替.{0,6}抢\|论证归属\|写作策略\|文档维护者` | 写作策略元注释 |
| SL-7 | error | `scan\.md\|STATE\.md\|VERIFY-CHECK\|Pattern #[0-9]+\|模式 [0-9]+\|Gate [0-9A-Z]\|反查维度` | review 工具术语 |
| SL-8 | warning | `\bP[0-9]\b`（仅在 `--strict`） | 裸优先级词（存量 §5 表格大量存在，故默认不报） |

**实现要求**：

1. **跳过围栏代码块**：``` 与 ~~~ 之间的内容不检查（代码里出现类似 token 不算违规）。
2. **跳过行内代码**：反引号包裹的内容不检查（`D-13` 作为代码标识符时可能是合法的，比如讨论某个内核常量名——脚本按行处理时可用 `sed` 先剥离反引号内容再匹配）。
3. **输出格式**：`{file}:{line}: [{ID}] {message} :: {匹配文本}`；末尾输出各 ID 计数汇总；`--diff` 模式要同时输出被检查的 diff 范围。
4. **`--diff` 实现**：`git diff -U0 [RANGE] -- '*.md'`，解析 `@@` 头拿到新增行的行号集合，再对这些行跑规则。已暂存与未暂存都要覆盖（`git diff` + `git diff --cached` 两路合并）。
5. **`--self-test`**：内置三组 fixture 文本（一组含各类违规 → 期望命中；一组为干净正文含 `[ARCH:...]`、`file:line` 锚点、头部创建日期、`保护模式` 等合法样例 → 期望零命中；一组为**合法历史叙述**样例，如"Minix3 最初把进程表放在 BSS 段"→ 期望零 error 命中、SL-5b 仅在 `--strict` 下命中），自测不通过 = 脚本不可交付。
6. 脚本头部注释写清用途、规则表对应的规则来源（A1.2 第 7 条），与 `tools/lint-review-rules.sh` 的风格保持一致。

**验收**：

```bash
tools/doc-style-lint.sh --self-test                       # 期望 exit 0
tools/doc-style-lint.sh notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md
# 期望：报告出 SL-5（旧文档误标）等命中，exit 1
tools/doc-style-lint.sh --diff                            # 在干净工作区期望 exit 0
# 手工制造一处违规（改一行加 "（2026-01-01 修复）"）后重跑 --diff，期望 exit 1 且只报该行；验完还原
```

### A3. 增量门接入（三个位置）

**任务**：把 `doc-style-lint.sh --diff` 接进修复与写作流程。

| # | 位置 | 接法 |
|---|---|---|
| A3.1 | `.claude/rules/fix-guard.md` | A1.5 已加；此处确认它是"Before EVERY fix"清单的一项，格式与既有 4 条一致 |
| A3.2 | `prompt/review-rules/review-process.md` §修复阶段 | 加一句：文档类修复完成后，跑 `tools/doc-style-lint.sh --diff`；有 error 级命中则修复未完成（先把过程信息移到 scan.md / fix-status，再改写正文） |
| A3.3 | `prompt/skill/cmds/full-review/SKILL.md` "强制门"段 | 追加"文风增量门（文档类）：`tools/doc-style-lint.sh --diff` 零 error 命中"；`style-fix` 薄壳（`prompt/skill/cmds/style-fix/SKILL.md`）在产物段写明"进出各跑一次全量 lint" |

**执行注意**：`review-process.md` 很长（2700+ 行），先用 `rg -n "修复阶段|Fix Phase"` 定位，改动只加一句，不要重组章节。

**验收**：

```bash
rg -n "doc-style-lint" .claude/rules/fix-guard.md prompt/review-rules/review-process.md \
  prompt/skill/cmds/full-review/SKILL.md prompt/skill/cmds/style-fix/SKILL.md
# 预期：4 个文件都有命中
```

### A4. 存量清单（只登记，不执行）

**任务**：实现完 A2 后，跑一次全量扫描，把统计结果追加到本文件 A4 末尾（允许修改本文件），并把"编辑战役"登记为 backlog。

```bash
tools/doc-style-lint.sh --dir notes/rewrite/fork-syscall-rewrite > /tmp/doc-style-report.txt
wc -l /tmp/doc-style-report.txt
# 按文件聚合 top 10：
cut -d: -f1 /tmp/doc-style-report.txt | sort | uniq -c | sort -rn | head -10
```

**登记格式**（追加到本节末尾）：

```
### A4.x 存量统计（{日期} 执行）
- 总命中：{N}（其中 SL-1 编号 {a}、SL-4 日期 {b}、SL-5 修复史 {c}、SL-6 元注释 {d}、SL-7 工具术语 {e}）
- Top 10 文件：...
- 战役启动条件：A/B 与 H 批落地，抽样样章达到"教科书级"标准（H4：复述通过 + §4.1-4.5 至少 B），之后由用户决定是否启动 style-fix 逐文档战役
- 战役执行方式（届时候选）：每篇文档先跑 lint 出清单 → 按"事实信息移回 .review/todo、正文改写"处理 → 出修订版
```

**验收**：本节有统计结果；不修改任何 `notes/rewrite/` 正文。

### A5. 工作流 A 的验收汇总

- [ ] A1 的 grep 验收通过，5 个文件口径一致。
- [ ] A2 `--self-test` 通过，`--diff` 实测能拦截新增违规。
- [ ] A3 的 4 处接入点均有引用。
- [ ] A4 统计结果写入本文件。
- [ ] 工作流 C 同步后（见第四部分），`tools/check-review-rules.sh` 与 `tools/lint-review-rules.sh` 全绿。

---

## 三、工作流 B：full-review 的"文档 + 关联 Rust 代码"一体审查

### B0. 现状与缺口（为什么用户感觉"代码没被一起审"）

**用户的诉求**："我经常使用 full-review，是希望某个文档和它关联的 rust 代码一块 review。"

**现状事实**：

1. `prompt/review-rules/review.md` 的模式 B 定义（约 914-917 行）："用户指定 review xxx.md 但未提及代码 → 不检查：Rust 代码"。`full-review` 命令虽然加载了 `review-code-checklist`（`review-cmds.md` 第二节"加载模块"），但流程里没有"枚举本文档关联代码"的强制步骤。
2. `prompt/review-rules/review-process.md` 的 Step -0.5 写着"仅当 review 涉及 Rust 代码时执行"（约 384 行），但"什么算涉及"没有定义——执行 AI 自行判断，容易跳过。
3. 文档头部有事实上的约定字段（VM 侧统一 `> **Rust 模块**:`，kernel 侧混用 `> **Rust 实现**:`，部分早期文档如 `01-boot-shim-bootstrap.md`、`05-clock-interrupt-init.md` 完全没有该字段；`06-proc-init-boot-proc.md` 用了全角冒号），但没有规范、没有校验、没有工具。
4. 关联代码清单不是任何 Step 的产物，scan.md 里也没有对应段落；代码维度做没做、做到什么程度，事后无法核查。

**结论**：不是"代码检查清单不存在"，而是"文档到代码的入口没有强制化"。需要补三件事：声明规范（B1）、自动抽取工具（B2）、流程与产物（B3/B4），最后端到端演练（B5）。

### B1. "关联代码"定义与声明规范

**定义**：一篇正式文档的**关联代码** = 以下三部分的并集：

1. 文档头部声明字段里的 Rust 路径。
2. 文档 §3（设计）与 §4（实现）中出现的 `os/` 开头的 `.rs` 路径（反引号内）。
3. 文档 §5（测试）中声明的测试文件或测试模块（含同文件 `#[cfg(test)]` 的说明）。

**头部字段规范**（写入 `prompt/review-rules/review-doc-checklist.md` §1 文档结构规范，并同步适配层 `prompt/skill/review-doc-skill.md`）：

```
> **Rust 实现**: `os/kernel/src/clock.rs`, `os/kernel/src/syscall_clock.rs`, `os/arch/src/arch/clock.rs`
```

规则：

- 字段名用 `Rust 实现`；`Rust 模块` 作为兼容别名被工具接受（lint 可给 warning，不强制存量改名）。
- 分隔符统一半角冒号加空格（存量里的全角冒号由工具兼容，文档更新时顺手改）。
- 路径：仓库根相对路径、带 `os/` 前缀、每条路径单独一个反引号、逗号分隔。
- 禁止花括号展开（如 `os/kernel/src/{proc.rs,proc_table.rs}`）——解析工具不认，存量的这类写法在编辑战役时拆开。
- 头部行允许在路径后带说明（例如 `fn clock_init` 或 `（L123，工具生成）`），工具只提取形如 `os/….rs` 的反引号 token，其余忽略。
- 无关联 Rust 代码的文档（如 `00-*-overview.md`、`99-global-concepts.md`、纯 TODO 文档）写 `> **Rust 实现**: 无（{一句理由}）`。
- **与既有 `> **状态**:` 字段的裁决**：`review-doc-checklist.md` §1 模板现有一个可选 `> **状态**: 可选，标注 Rust 实现状态` 字段。`Rust 实现` 字段落地时同步裁决：推荐废弃 `状态` 字段（路径声明职责并入 `Rust 实现`），若保留则明确两字段的分工；不允许两个字段都声称"Rust 实现状态"。

**新文档要求**：从规则生效起，新建文档必须带该字段；缺失在 review 中判 P1（文档缺陷）。

**验收**：

```bash
rg -n "Rust 实现" prompt/review-rules/review-doc-checklist.md prompt/skill/review-doc-skill.md
# 预期：两处都有完整规范（字段名/分隔符/路径格式/无代码豁免/缺失判级）
```

### B2. 工具：`tools/doc-code-map.sh`

**任务**：从一篇文档抽取关联代码清单并校验存在性。

**CLI**：

```
tools/doc-code-map.sh DOC_PATH [--check] [--format=md|plain]
```

行为：

1. 解析头部字段（`Rust 实现` / `Rust 模块`，半角/全角冒号都认）。
2. 按 `## 3` / `## 4` / `## 5` 分节，提取各节内形如 `os/…\.rs` 的反引号 token。
3. 去重，标注来源（头部声明 / §3 / §4 / §5）。
4. `--check`：逐条 `test -f`，缺失条目汇总输出；有缺失时 exit 1。
5. 输出 Markdown 表格（默认可直接粘贴进 scan.md 的 Step 0 段）：

```
| 路径 | 来源 | 类型 | 存在 | 备注 |
|---|---|---|---|---|
| os/kernel/src/clock.rs | 头部声明 + §4 | 实现 | 是 | 412 行 |
```

6. 文档没有头部字段且正文也没有 `os/*.rs` 引用 → 输出"无关联代码（需人工确认理由）"，exit 1（强制人工声明豁免）。头部写 `无（理由）` 时 exit 0。
7. 仓库工具风格：bash + `rg`；脚本头注释写用途与验收命令。

**验收**：

```bash
tools/doc-code-map.sh notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md
# 期望：表格包含 dispatcher.rs / transport.rs / vm_server.rs 等，全部存在
tools/doc-code-map.sh notes/rewrite/fork-syscall-rewrite/01-stage-kernel/05-clock-interrupt-init.md
# 期望：输出"无头部字段"，若正文引用了 .rs 也列出来；用于暴露存量缺口
# 构造一条不存在的路径，验证 --check 正确 exit 1
```

### B3. 流程修订：让"文档 + 关联代码"成为 full-review 的默认语义

**任务**：修改两个源文件与一个适配层，共 4 处。

| # | 文件 | 改什么 |
|---|---|---|
| B3.1 | `prompt/review-rules/review.md` §0.2/0.3 模式定义 | 明确：从 `full-review` 入口进入且目标是编号文档时，语义等于"模式 C（完整 Review：文档 + 关联代码）"；模式 B 的"不检查 Rust 代码"只适用于用户显式说"只看文档"的场景。同步修改 §0.3 的自动判定话术 |
| B3.2 | `prompt/review-rules/review-process.md` Step 0 | 在 `§Step 0: 预检结果` 产物里新增子小节"关联代码清单"：必须粘贴 `tools/doc-code-map.sh` 的输出（或手写等价表格），并给出结论"共 N 个文件；存在性全通过 / 缺失列表"。Step 0 时间预算表加一行 |
| B3.3 | `prompt/review-rules/review-process.md` 新增 Step 3.6 | "Step 3.6 关联代码维度检查（文档 review 强制）"：按 `review-code-checklist.md` 的维度对清单内文件执行（至少覆盖 §1 Rewrite 质量、§2 硬件抽象、§4 执行模型/SMP、§8 命名、§13 设计-代码一致性、§14 C-Rust 语义对齐）；产物是一张"维度 / 文件 / 结论 / 证据"表加 `gate-evidence-code` 块；无发现也要写明"已检查、无发现" |
| B3.4 | `prompt/skill/review-process-skill.md`（适配层） | 同步 B3.2/B3.3（该文件是 `.trae/`/`.codex/` 派生源） |

**执行注意**：

- `review-process.md` 的 Step 总览表（约 31-52 行）与"检查项注册表"（约 54-78 行）都要加 Step 3.6 一行，保持其"新规则以行加入注册表"的约定。
- 不要给 Step 3.6 发明新的 P0 分类；发现的问题沿用既有 P0/P1/P2 判定。
- `review.md` 的模式 B/C 与 Profile 对账表（`review-cmds.md` 第八节）已有"B 代码 PR 质量 → full-review（scope=doc，代码侧重）"的映射，保持措辞一致。

**验收**：

```bash
rg -n "Step 3.6|关联代码清单|gate-evidence-code" prompt/review-rules/review-process.md prompt/skill/review-process-skill.md
rg -n "模式 C|关联代码" prompt/review-rules/review.md | head
# 预期：模式 B 不再无条件排除 Rust 代码；full-review 入口语义指向模式 C
```

### B4. 门禁强化（不新增 Gate 字母）

**任务**：把新要求接进既有门与工具。

| # | 位置 | 接法 |
|---|---|---|
| B4.1 | `tools/review-gate-check.sh` | 新增检查项：scan.md 的 `§Step 0: 预检结果` 段内必须出现"关联代码清单"表头；有 `gate-evidence-code` 块（或明确写 N/A + 理由）。缺失 → FAIL |
| B4.2 | Gate 0 描述 | Gate 0 锚段数量不变（9 个）；在 `§Step 0: 预检结果` 的锚段定义里补一句"该段必须含关联代码清单"。**先跑 `rg -ln "锚段\|anchor sections" prompt/ .claude/ .codex/ .trae/ CLAUDE.md` 列全清单再逐处更新**——已核实规则层 14 处（手改 9 + 派生 5）：手改 = `prompt/review-rules/review-process.md`、`prompt/skill/review-process-skill.md`、`prompt/skill/review-agent-ide.md`（字符余量只剩 249，动前先算长度）、`prompt/README.md`、`.claude/rules/review-core.md`、`.claude/rules/review-process.md`、`.claude/skills/review-scan/SKILL.md`、`.claude/skills/review-scan/checks/process.md`、`CLAUDE.md:109`（英文 "9 anchor sections"）；派生 = `.codex/skills/` 与 `.trae/skills/` 下的对应副本（`tools/generate-derived-skills.sh` 默认两端都生成，别只改 `.codex`）。**顺手修**：`prompt/README.md:22` 目录树注释写"9,739 字符，余量 261"已过期，按实测改为"9,751 字符，余量 249"（同文件 :102 表格数字已正确）。注意 `AGENTS.md` 不含 Gate 0 锚段描述，不要机械照抄旧清单 |
| B4.3 | `prompt/skill/cmds/full-review/SKILL.md` | "执行"段补一句：Step 0 必须产出关联代码清单，Step 3.6 必须执行；"产物"段补"关联代码清单 + 代码维度结论（含无发现声明）" |

**执行注意**：`CLAUDE.md`、`AGENTS.md`、`review-agent-ide.md` 里的 Gate 0 描述属于"同一事实多文件复述"，最容易漏改——任务完成标准是 `rg -ln "锚段"` 的输出文件全部打勾，而不是清单里列了几处。这是描述性同步，不是新增 Gate。

**验收**：

```bash
bash -n tools/review-gate-check.sh
rg -n "关联代码清单" tools/review-gate-check.sh .claude/skills/review-scan/SKILL.md \
  .codex/skills/review-scan/SKILL.md prompt/skill/review-process-skill.md prompt/review-rules/review-process.md
# 预期：5 处以上命中
# 用一个缺该段的旧 scan.md 跑 review-gate-check.sh，期望报 FAIL；补齐后 PASS
```

### B5. 端到端演练（用真实文档验证机制）

**任务**：工作流 C 同步完成后，选一篇文档，按修订后的 `full-review` 跑一遍，验证：

1. 入口语义正确：目标是 `02-stage-vm/15-ipc-dispatch.md`，执行者读到的是"文档 + 关联代码"完整 review，而不是只查文档。
2. Step 0 产出关联代码清单（来自 `doc-code-map.sh`，含存在性）。
3. Step 3.6 对清单内文件执行代码维度检查，产出 `gate-evidence-code`。
4. Issue List 里既有文档问题也有代码问题（或明确的"代码维度无发现"结论）。
5. 文风增量门：修复过程中任何新增/修改行通过 `doc-style-lint.sh --diff`。

**范围成本控制**：先做 `scope=chapter` 的干跑（建议第 2 章或第 3 章），机制验证通过后再决定是否补 `scope=doc`。产物写入当前工具的状态目录（Codex 为 `.review/codex/fork-syscall-rewrite/15-ipc-dispatch/`；Trae/Claude 按各自路径）。

**验收**：上述 5 点逐条在产物里可指认；执行者把"演练结论"追加到本文件 B5 末尾。

### B6. 工作流 B 的验收汇总

- [ ] B1 规范写入 checklist 与适配层。
- [ ] B2 工具通过两条实测（正常文档 + 缺字段文档）+ 一个缺失路径反例。
- [ ] B3/B4 的 grep 验收通过。
- [ ] B5 演练 5 点全部满足，产物路径可查。
- [ ] 工作流 C 同步后，回归脚本全绿。

---

## 四、工作流 C：三端同步与回归

### C1. 同步映射（改动源之后必须做的事）

| 改动源 | 派生目标 | 同步方式 |
|---|---|---|
| `prompt/skill/*.md`（领域 skill 源） | `.trae/skills/` + `.codex/skills/` | `tools/generate-derived-skills.sh`（可加 `trae` / `codex` 参数单端生成） |
| `.claude/skills/review-scan/` | `.codex/skills/review-scan/` | 同上（脚本已覆盖） |
| `prompt/skill/cmds/*/SKILL.md`（cmd 薄壳） | `.agents/skills/*`（软链）与 `opencode.json` | 薄壳本身是软链注册；改了 `full-review`/`style-fix` 薄壳不需要重新注册，但要在 `.agents/skills/` 下确认软链仍指向 |
| `prompt/review-rules/*.md` | 无自动派生 | 它们是规则源；只需确认 `prompt/skill/` 适配层是否要同步（按 `prompt/README.md` 的"review-rules ↔ skill 一致性"条目人工检查） |
| `.claude/rules/*.md` | 无自动派生（Claude/Codex 直接读） | Codex 通过 `AGENTS.md` 指向 `.claude/rules/` 规范源，无需复制 |
| `CLAUDE.md` / `AGENTS.md` | 无派生 | 两文件含 Gate 0/累积改进表/Proposal 状态等复述内容；改动源后必须回填（B4.2、D5、E2、F1 已点名具体行） |

**平台边界（现状核实，2026-09-16）**：当前使用中的工具目录可分四类，避免"平台一多规则就乱"的担心：

- **自动派生（不存在手工副本）**：`.trae/skills/`、`.codex/skills/` 由 `tools/generate-derived-skills.sh` 从 `prompt/skill/` 生成；`.agents/skills/` 是指向 `prompt/skill/cmds/` 的软链；`opencode.json` 只登记 `.codex/skills/` 与 `prompt/skill/cmds/` 的路径。这三处改了源就跟着变，没有漂移空间。
- **人工审读的第二源**：`.claude/rules/` 与 `.claude/skills/review-scan/`。Claude 端可以先行（先改这里再回填 `prompt/`），但回填是义务；C1 第二项实测的"源漂移审计"就是查这个。
- **复述内容**：`CLAUDE.md`、`AGENTS.md`、`prompt/README.md` 里的计数与清单（Gate 0 锚段数量、模式总数、Proposal 状态）。这类文字没有派生脚本，靠 B4.2、D5、E2、F1 点名回填，附录 4 管模式计数。
- **不参与同步**：`.lingma/`（教学风格规则）、`.codebuddy/plans/`、`.zcode/plans/`（会话计划）、`.trae/documents/`（评审工作文档）当前都不含 review 规则副本，不在同步链上。将来若要在新平台使用 review 规则，先在本表登记派生方式（脚本或软链），禁止手工复制规则正文。

**同步输出必含两项实测**（防"改完 skill 顶爆 Trae 上限"和"源/派生悄悄漂移"）：

```bash
# 1) Trae Agent 字符上限（prompt/README.md 实测口径，locale 无关）
python3 -c "print(len(open('prompt/skill/review-agent-ide.md',encoding='utf-8').read()))"
# 期望 ≤ 10000；> 9800 就先精简再加内容

# 2) 源 ↔ .claude 漂移审计（check-review-rules.sh 目前只做存在性与少量定点断言，没有源↔.claude 的文本/编号 diff）
for f in review-process review-core fix-guard; do
  echo "== $f =="
  diff <(rg -o "Step [0-9][0-9.]*[a-z]?|Gate [A-Z]([.-][0-9]+)?|模式 [0-9]+" "prompt/review-rules/$f.md" 2>/dev/null | sort -u) \
       <(rg -o "Step [0-9][0-9.]*[a-z]?|Gate [A-Z]([.-][0-9]+)?|模式 [0-9]+" ".claude/rules/$f.md" 2>/dev/null | sort -u)
done
# 目的：发现"只存在于 .claude 或只存在于 prompt"的 Step/Gate/模式编号
# 已知案例：.claude/rules/review-process.md 有 Step 4.5b，prompt/review-rules/review-process.md 无同名节（机制在 review-doc-skill.md）
# 处置：差异写进 C 批交付说明；属于"Claude 先行"的回填到 prompt 源，属于误删的恢复
```

### C2. 回归命令（每次同步后全跑）

```bash
tools/generate-derived-skills.sh
tools/check-review-rules.sh
tools/lint-review-rules.sh
# Codex frontmatter 合规（引用 prompt/README.md 的验证片段）：
for f in .codex/skills/*/SKILL.md; do
  dir=$(basename "$(dirname "$f")")
  name=$(awk '/^name:/{sub(/^name: *"?/,""); sub(/"?$/,""); print; exit}' "$f")
  desc=$(awk '/^description:/{sub(/^description: /,""); gsub(/^"|"$/,""); print; exit}' "$f")
  [ "$name" = "$dir" ] || echo "name != dirname: $f"
  dlen=$(python3 -c 'import sys;print(len(sys.argv[1]))' "$desc")
  [ "$dlen" -le 1024 ] || echo "desc>1024: $f ($dlen)"
  grep -q '^description: "' "$f" || echo "desc 未加引号: $f"
done
```

已知约束（执行时不要踩）：

- `prompt/skill/review-agent-ide.md` 字符余量约 250（Trae 硬上限 10,000）。任何新增都会顶到上限；优先不加，若必须加先减。
- `review-process-skill.md` 是长文件，改动后用 `tools/generate-derived-skills.sh --check` 确认无漂移。
- `prompt/review-rules/` 改动不会自动进 `prompt/skill/`，B3.4 与 A1.6 就是为了补这一点。

### C3. 总验收（Definition of Done，三轮各执行一次）

通用项（每轮都跑）：

- C2 的回归命令全绿 + 同步输出必含的"字符上限"与"源漂移审计"两项实测。
- **成功度量（improvement metrics，2026-09-18 重排为卓越优先）**：先看卓越度量——复述通过文档数（H1）、非法态封堵清单与落地改进数（H2）、文档代码块分类覆盖率与未标注反模式数（H3）、样章达标数（H4）；再看正确性度量——裸 unsafe 数、`missing_docs` 警告数、契约/覆盖率缺口、正确性 P1 未处置数；机械度量（review 编号数、正文日期数、行号漂移数、头部字段缺失数、Proposal 剩余数）只作健康检查，不作为"完成"的主证据。拿不出卓越度量的变化，不算完成。
- **规则集膨胀刹车**：本轮新增/修改的规则条款若超过 20 行，需在执行记录里说明"为什么不能合并/替代既有条款"；三个存量清单（文档过程痕迹 / unsafe / 代码可读性）在 G 批完成后合并登记到同一个"存量债登记处"（建议 `.review/BACKLOG.md`，只登记不定稿）。候选池同样适用：一轮里新增任务数（含新工具/新基线）必须能被用户的触发需求解释，否则合并或删除；本文件是候选池，不是要消化的 backlog。
- 执行记录填写：任务状态、度量对比、遗留问题。
- **落地 commit 与回滚点**：本轮有规则类改动时，把落地 commit 记入任务状态行；需要撤销时按 0.6 回滚（撤销规则行 + 重跑 C2 三脚本），不追溯旧产物。

第一轮（工作流 A/B/C）额外：

1. A5、B6 的验收项全部打勾。
2. 演练（B5）产物可查，且 issue 清单同时含文档与代码两侧内容（或明确的无发现声明）。
3. 未修改任何 `notes/rewrite/` 存量正文（A4 统计除外），未修改 `minix3/`。

第二轮（工作流 D/E/F）额外：

1. D1-D6、E1-E3、F1-F2 的验收命令全部通过并留输出。
2. D6 演练产物可查：`gate-evidence-qemu`（或环境缺失声明）、`gate-evidence-unsafe`（含与基线差值）、coverage 多域输出、`anchor-resolve` 输出（0 定义条数）、`GATE-CHECK` 结论行。
3. `tools/unsafe-baseline.txt` 已产出并入库；CLAUDE.md/review-core.md 的 Proposal #7/#8/#12/#13 状态已回填。

第三轮（工作流 G）额外：

1. G1-G4 的验收命令全部通过；`tools/code-style-baseline.txt` 入库。
2. `missing_docs` 警告清单 + CI 扩展结果（或分批豁免清单）记录在案。
3. G3 路径抽查结论写入执行记录（选中的主线、卡点、判级与去向）。

---

## 五、工作流 D：可执行正确性证据

> 来源：1.5 的缺口 2/3/6/9。目标：把正确性证据从"读代码 + hosted 单测"升级为"可执行 + 可核对磁盘事实"。

### D1. QEMU 集成测试纳入受影响面门禁

**目标**：被审代码位于内核关键路径时，review 必须跑一次 QEMU 集成测试并留证据。

**现状证据**：`os/qemu-tests/run_all.sh` 是 CI 入口，每个测试在串口打印 `### TEST_RESULT: PASS <name> ###`；但 review 规则零引用（全仓仅 `review-patterns.md` 一处无关提及）。内核启动/中断/SMP/BKL 这些关键路径目前只靠 hosted 单测（覆盖不到）加读代码保证。

**改动点**：

1. `prompt/review-rules/review-process.md` 的 Step -0.5 增加第 5 条"QEMU 冒烟（条件触发）"：
   - 触发条件**直接复用 CI 路径清单**（`.github/workflows/qemu-tests.yml` 的 push/PR paths）：`os/kernel/**`、`os/arch/**`、`os/plat/**`、`os/libs/{minix-types,minix-boot,minix-elf,minix-platform}/**`、`os/boot-shim/**`、`os/qemu-tests/**` 中的非测试代码，或文档声称的行为依赖真实硬件路径（启动链/中断/分页/BKL）。不要自造一份更窄的目录列表。
   - 执行：优先 `os/qemu-tests/run_qemu.sh` 跑受影响单例；无法单例时跑 `os/qemu-tests/run_all.sh`。输出尾部保存到 `.review/{tool}/{module}/{doc-stem}/logs/qemu-{YYYYMMDD}.log`。
   - scan.md 写 `gate-evidence-qemu` 块：命令 / exit 码 / PASS 计数 / log 路径。
2. N/A 规则：纯文档改动或只碰 `#[cfg(test)]` → 写 `gate-evidence-qemu: N/A + 一句理由`；环境缺 QEMU/固件时不允许静默 N/A，必须写"环境缺失 + 未验证项清单"并按 P1 登记（沿用既有的"工具物理不可用 → PARTIAL"原则）。
3. `prompt/review-rules/review-cmds.md` §二 强制门追加引用；`prompt/skill/cmds/full-review/SKILL.md` 同步摘要。

**执行注意**：先读 `os/qemu-tests/README.md` 确认依赖（QEMU + OVMF/AA64 UEFI/RISC-V UEFI 固件 + rustup target）；脚本不可用时把具体缺什么记录下来，不要伪造 PASS。还要在任务说明里写清定位：CI（`qemu-tests.yml`）已在 push/PR 时跑同一套测试，review 门是"本地/提交前层"——两者共用测试，不共用证据（review 用 log + `gate-evidence-qemu`）。CI 单次超时 45 分钟（`.github/workflows/qemu-tests.yml:39`）；本地 review 坚持单例优先，默认不走全量 `run_all.sh`。

**验收**：

```bash
rg -n "gate-evidence-qemu|qemu-tests/run_all.sh" \
  prompt/review-rules/review-process.md prompt/review-rules/review-cmds.md \
  prompt/skill/cmds/full-review/SKILL.md
# 环境具备时实跑一次，把输出尾部贴进交付说明
os/qemu-tests/run_all.sh
```

### D2. unsafe / SAFETY 审计工具与增量门

**目标**：让每个 `unsafe` 构造都有安全辩护，新增裸 unsafe 被拦截。

**现状证据**：`os/` 下约 1774 处 unsafe 构造（206 个文件），约 848 条 SAFETY 注释；没有清单、没有覆盖率门、没有动态检测（无 Miri/loom/proptest）。

**改动点**：

1. 写 `tools/unsafe-audit.sh`：
   - 枚举 `unsafe {`、`unsafe fn`、`unsafe impl`、`unsafe extern`；
   - 辩护判定：同一条或前 5 行内出现 `// SAFETY:`（兼容 `// Safety:`）→ 有辩护；否则记为**裸 unsafe**；
   - 模式：`--report`（全量统计 + 裸 unsafe 清单）、`--diff [RANGE]`（只查新增/修改行）、`--baseline FILE --update-baseline`、`--self-test`（内置正反例）。
2. 基线：首次运行产出 `tools/unsafe-baseline.txt`（每文件计数 + 裸 unsafe 行清单）并入库；增量门只阻断"新增裸 unsafe"，存量裸 unsafe 作为专项治理 backlog（登记，不在本轮修）。
3. 接入：`prompt/review-rules/review-process.md` Step 3.6（第一轮 B3.3 新建）增加 `gate-evidence-unsafe`（命令 / 裸 unsafe 总数 / 与基线差值 / 新增违规清单）；`prompt/skill/review-process-skill.md` 同步。
4. Miri：不默认引入（需要 nightly 工具链与逐个模块可行性评估），登记为 OQ10。

**执行注意**：启发式必然有误报（多行注释、宏展开、`unsafe` 出现在字符串里）；允许用 `tools/unsafe-baseline.txt` 收纳误报，不追求零误报；脚本头注释写清判定规则与局限。

**验收**：

```bash
tools/unsafe-audit.sh --self-test
tools/unsafe-audit.sh --report | tail -5          # 统计数与基线一致
# 制造一处无 SAFETY 的 unsafe，--diff 应报；验毕还原
```

### D3. Gate A 多 `--c-dir` 与语义域一致性

**目标**：覆盖率门在跨模块文档上不再给假安全感。

**现状证据**：`.review/codex/sched/12-kernel-interface/scan.md:42` 自己写明脚本只覆盖 `servers/sched`，而本篇 C 语义域在 `kernel/`；人工补表后 Gate A 仍 PASS。

**改动点**：

1. `tools/coverage-extract/coverage-extract.py`：`--c-dir` 支持重复传入（或逗号分隔），提取结果按目录分区合并；SYMBOLS.md 增加"C 域"列（kernel / servers/xxx）。
2. Gate A 证据规则（`prompt/skill/review-coverage-skill.md` 的 `gate-evidence-A` 模板 + `prompt/README.md` 覆盖率节）：scan.md 必须写"语义域声明"——本文档 C 源码分布（`rg -l` 结果）与本次 `--c-dir` 的一致性；若 doc 引用目录未被覆盖，必须补跑多域命令，否则 Gate A 不得 PASS。
3. 手工补表保留为兜底，但必须写明"工具缺口原因 + 补表范围"，且补表行带 C 行号（沿用既有惯例）。

**执行注意**：改 python 工具时保持向后兼容——单 `--c-dir` 调用行为不变，避免破坏历史命令与既有产物格式。

**验收**：

```bash
python3 tools/coverage-extract/coverage-extract.py sched notes/rewrite/fork-syscall-rewrite/06-stage-sched \
  --rust-dir os --c-dir minix3/minix/servers/sched --c-dir minix3/minix/kernel \
  --doc-file 12-kernel-interface.md --output /tmp/SYMBOLS-multi.md
rg -c "kernel/" /tmp/SYMBOLS-multi.md      # 非零，证明 kernel 域被纳入
```

### D4. 证据脚本化（Gate 结论从磁盘事实推导）

**目标**：把 Gate 通过与否从"执行者粘贴的文本"改为"工具从磁盘事实推导的结论"。

**现状证据**：scan.md 的 `gate-evidence` 是执行者粘贴文本；`tools/verify-check.py` 只做关键字/锚段存在性检查，不验真。

**改动点**：

1. 扩展 `tools/review-gate-check.sh`：
   - 保留既有检查（design/outline 快照、STATE/scan/SYMBOLS/structure/VERIFY-CHECK 存在性）；
   - 新增：产物非空 + 关键锚段 grep（`gate-evidence-A`、`gate-evidence-code`、`gate-evidence-qemu|unsafe`（按触发条件））；
   - 新增：证据块引用的磁盘产物真实存在（SYMBOLS.md 路径、qemu log 路径、unsafe 报告路径、anchor-resolve 输出路径）；
   - 输出机器可引用的结论行：`GATE-CHECK: PASS|FAIL <明细>`。
2. 规则：`prompt/review-rules/review-process.md` Step 5.5 要求 scan.md 的 Gate 状态表引用该工具结论行（粘贴原文），"✅ PASS" 无工具行视为无效证据（按 L3 处理）。
3. `tools/verify-check.py`：把"gate-evidence 块存在性检查"升级为"证据块 ↔ 磁盘产物交叉核对"。

**执行注意**：保持脚本对旧产物（本轮之前生成的 scan.md）不报错——对旧格式只给 warning；新规则从落地之后的 review 起强制。

**验收**：

```bash
bash -n tools/review-gate-check.sh
# 负例：临时移走某文档的 SYMBOLS.md → 期望 FAIL；还原 → PASS
tools/review-gate-check.sh codex fork-syscall-rewrite 12-kernel-interface | tail -3
```

### D5. 符号锚点迁移与卫生项分层（2026-09-18 改写，取代"行号自动化"）

**目标**：消灭行号漂移这类机械耗注意力检查。方式不是"把行号检查全量化"（那是让拐杖变承重墙），而是**把锚点符号化**：事实断言用符号定位，行号只在工具输出里派生。符号化之后，漂移问题结构性消失，review 轮次的注意力全部让给语义与卓越性检查。

**现状证据**：CLAUDE.md 累积改进表记录"6 个 doc 累计 25+ 处 P2 偏移"；Proposal #7 `tools/review-line-check.sh` 至今"⏸ 待开发"；sched/12 那轮唯一发现就是 5 处行号偏移。用户裁定：行号是 AI 的校验脚手架，不是目标。

**锚点约定（新，写入规则源）**：

| 对象 | 锚点形态 | 例子 |
|---|---|---|
| Rust 函数/类型/trait/常量 | `path:fn name`、`path:struct Name`、`path:trait Name`、`path:const NAME` | `os/kernel/src/clock.rs:fn clock_init` |
| 同名方法歧义 | 加 impl 限定 | `os/kernel/src/proc.rs:impl Proc::new` |
| C 函数/结构体/常量 | `path:func`、`path:struct name`、`path:NAME` | `minix3/minix/kernel/proc.c:proc_init` |
| 函数内部的具体片段 | 函数符号 + 引文片段 | `os/kernel/src/proc.rs:fn do_fork（match 分支）` |
| 行号（可选） | 工具派生，写成 `（L123，工具生成）`；**禁止手工维护** | — |

**改动点**：

1. **工具**：写 `tools/anchor-migrate.sh`（一次性迁移）与 `tools/anchor-resolve.sh`（解析/校验）。
   - 迁移：读文档中旧 `path:line` 锚点，从该行向上找最近的定义行（fn/struct/enum/trait/const/impl），重写为符号锚点；指向函数内部的引用改为"函数符号 + 引文片段"；无法解析的列清单人工过一遍。C 源用启发式（非缩进、非 `;` 结尾的 `NAME(` 行），歧义即报告。
   - 解析（review 时用）：按声明形态正则定位（Rust `^\s*(pub(\([^)]*\))?\s+)?(unsafe\s+)?(async\s+)?fn\s+NAME\b`，类型同型），**调用点与注释不参与匹配**；输出当前行号供摘录引用。错误只有两种：0 定义 → P0-fact（符号消失/改名）；多定义 → 要求文档补限定。
   - 原 `tools/review-line-check.sh`（Proposal #7）方案作废，能力并入这两个脚本。
2. **卫生项分层（对话第 1 条纠正）**：`prompt/review-rules/review-process.md` Step 7.1（`:2278-2299`）与 scan.md Issue List 增加"卫生项"分区——除"锚点 0 定义"（按 P0-fact 处理）外的机械项（行号提示过期、全角/半角、拼写、格式）只记录 + 批量修，**不进 P1/P2 计数、不进 `weighted_new`、不参与收敛判定**。
3. **规则收编**（源文件清单，按 C1/C2 同步）：
   - `prompt/review-rules/review-process.md`：检查项注册表 `:70-77` 的 1.0a~1.0g 收缩为一个"锚点解析"步骤；实现段 `:1196-1547`（至 Step 1.5 前）同步收缩；Step 7.1 加卫生项条款。
   - `prompt/review-rules/review-doc-checklist.md`：`:313-320`、`:449-451`、`:482-485` 的"±1 行 P2 / 偏移 >5 行 P1 / 范围上界 P2"整组阈值表删除，改为"0 定义 → P0-fact；改名/歧义 → P1/P2（要求补限定）"。
   - `prompt/review-rules/review-patterns.md`：模式 66/73/74/75/76/77 中与行号漂移相关的判据改写为符号判据；模式 66 保留"路径存在性"职责；模式 83 的锚点形态更新为符号锚点。
   - `prompt/review-rules/review-cmds.md` §一 锚点纪律门、§六 style-bible 第 6 条与 `prompt/skill/cmds/style-bible/SKILL.md` 同步措辞。
4. **时间预算（保留原 D5 第 3 项）**：Step 0 声明预算时，若文档有关联代码，写明"代码维度预算 ≥ 总预算 30%"；锚点迁移/卫生批量不计入 review 预算（它是独立批次）。
5. **Proposal 状态回填**：CLAUDE.md 累积改进表与 `.claude/rules/review-core.md` §Proposal 状态中，Proposal #7 与 Step 1.0a-自动（#13）改为"✅ 已落地（D5 改写，{日期}）：行号自动化作废，改为符号锚点迁移"。

**执行注意**：迁移是批量机械活，一次 git 提交完成，不逐文档开 review 轮次；解析工具的错报（同名测试、宏生成）允许在迁移报告里登记为"歧义"，要求文档补限定而不是工具加特例。代码注释里的 `see X.rs:N`（模式 77）不在本轮迁移范围，随模式 77 的符号化改写单独批量处理。规则收编后必须跑 C1/C2。

**验收**：

```bash
tools/anchor-resolve.sh --self-test
tools/anchor-resolve.sh --check notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md | tail -5
# 期望：0 定义条数为 0；多定义条目已补限定或登记为待补
rg -n "卫生项" prompt/review-rules/review-process.md        # Step 7.1 含卫生项条款
# 迁移统计写入执行记录：旧行号锚点数 → 新符号锚点数 → 无法解析清单 → 首轮卫生项计数
```

**行为验证（必做，self-test 不能代替）**：用一篇含行号漂移的文档（旧 scan 或现场造样）跑一次新口径 scan，对比新旧 `weighted_new` 与收敛评估，确认漂移项只出现在"卫生项"分区、不进入加权计数。输出贴进执行记录；不做这一步，Step 7.1 的卫生排除条款视为未验证。

### D6. 正确性门演练（D1-D5 的合并验证）

**任务**：选一篇内核代码文档（建议 `01-stage-kernel/13-syscall-dispatch.md` 或 `01-boot-shim-bootstrap.md`）跑一轮修订后的 review，验证：QEMU 冒烟（或环境缺失声明）、unsafe 报告与基线差值、coverage 多域、锚点解析（符号锚点 0 定义）、Gate 状态表引用 `GATE-CHECK` 结论行。

**验收**：5 项证据块可指认，产物写 `.review/{tool}/{module}/{doc-stem}/`；结论追加到本文件"执行记录"。

---

## 六、工作流 E：独立验证与判定收敛

> 来源：1.5 的缺口 4/7/8。目标：堵住"自己审自己"和"带病收敛"。

### E1. zero-P0 独立复核升级

**现状证据**：`prompt/review-rules/review-process.md` Step 5.6 的多 agent 表：P0≥1 → 跨 agent 强制；P0=0 → 同 agent 可（带 grep 重放）。`tools/review-init.sh --require-multi-agent` 默认 false；Codex 单 agent 环境无法跨 agent。

**改动点**：

1. Step 5.6 多 agent 表改为：
   - P0≥1 → 跨 agent 强制（不变）；
   - P0=0 → **至少执行一次独立复核**，三选一：a. 跨 agent（Trae 名单轮换）；b. 跨工具只读复核（如 Codex 产物由 Claude/Trae 复核一个章节，状态目录仍隔离、不共享中间产物）；c. 跨 session 换模型（单 agent 环境兜底）。
2. 复核范围：从文档随机抽**一个章节**（不是 3 个检查项），完整重跑该章节的 Step 1-4 核心检查；结论写入 VERIFY-CHECK.md 新增段"独立复核范围"（写明选中的章节、复核者类型、发现）。
3. 同步：`prompt/skill/review-process-skill.md`、`.claude/rules/review-core.md`、`prompt/review-rules/review-process.md` 的 VERIFY-CHECK 模板。

**执行注意**：跨工具复核只读对方 `.review/{tool}/` 产物，不写入、不合并（维持三工具隔离规则）；单 agent 兜底时必须在 VERIFY-CHECK.md 显式写"同 agent、跨 session"。

**验收**：

```bash
rg -n "独立复核范围|zero-P0|P0=0" \
  prompt/review-rules/review-process.md prompt/skill/review-process-skill.md
# 用既有 VERIFY-CHECK.md 试填"独立复核范围"段作为样例
```

### E2. 测试正确性抽样门 + P0-test-wrong

**现状证据**：P0 六分类有 `P0-test-missing`，没有 `P0-test-wrong`；`test-audit` 是独立 cmd、不进 Blocker Gates；Gate E 只 grep 测试函数名存在。`prompt/review-rules/review-cmds.md` §五自记"测试代码就是错的"是项目真实事故。

**改动点**：

1. `prompt/review-rules/review.md` P0 表新增第 7 类 **P0-test-wrong**：测试断言与文档/design 声称的行为不符、assert 永真、fixture 与生产路径脱节；等级与 `P0-test-missing` 相同（阻断 CONVERGED）。
2. Step 4.5 扩展：对 §5 测试做**抽样执行验证**——分层抽样取 30%（至少 3 个，含 1 个错误路径）：读函数体确认断言对应描述 + `cargo test <name>` 实跑；输出 `gate-evidence-test` 块（抽样清单 / 实跑输出 / 判定）。
3. **收编 Proposal #8（测试数量准确性机制）**：该机制已经落地在 `prompt/review-rules/review-doc-checklist.md`（测试总数末段补充）与 `.claude/rules/review-process.md §Step 4.5b`。本任务不重复发明，只在 Step 4.5 里引用它，并把它的状态从"⏸"回填为"✅ 已落地"（位置同 D5 的 Proposal 回填清单）。
4. **同步面（先跑 rg 列全，不要照抄本清单）**：`P0-test-missing` 实测出现在 16 个文件（2026-09-18，含本计划；执行时先跑 `rg -ln "P0-test-missing" prompt/ .claude/ .codex/ .trae/ CLAUDE.md AGENTS.md` 重测）——逐处决定是否加 `P0-test-wrong`。特别不能漏：`.claude/rules/review-core.md`（P0 列表）、`.claude/skills/review-scan/checks/patterns.md`、`prompt/skill/review-patterns-skill.md`、`prompt/skill/review-implementation-skill.md`、`prompt/README.md`、`CLAUDE.md`、`AGENTS.md`；`.trae/` 下的派生副本由生成脚本同步，不要手改。
5. **计数宣称同步（易漏）**："P0 六分类"实测出现在 10 个文件（2026-09-18，含本计划与 `.trae/documents/` 下的过程文档；执行时先跑 `rg -ln "六分类" prompt/ .claude/ .codex/ .trae/ CLAUDE.md AGENTS.md` 重测）——其中需要手改的规则/适配源为：`review.md`、`review-patterns.md`、`review-core-semantics.md`、`review-doc-excellence.md`、`README.md`、`prompt/skill/review-patterns-skill.md`——新增 `P0-test-wrong` 后全部改为"七分类"；`.codex/`、`.trae/` 派生副本由生成脚本同步。另：本计划 0.1 启动清单里的"P0 六分类"描述顺手同步为"七分类"（计划不是规则源，完成即退场，不列入规则同步面）。
6. **若同时新增测试族模式**（可选）：按附录 4 的 Pattern housekeeping 清单执行（计数宣称 5 处 + lint L5 + 派生同步）。

**执行注意**：只抽 §5 明确列出的测试名；实跑输出记入证据（`cargo test -p <crate> <name>`）；抽样未覆盖的测试在 scan.md 标注"未抽样"，不得表述为"已验证"。

**验收**：

```bash
rg -n "P0-test-wrong" prompt/review-rules/review.md .claude/rules/review-core.md \
  prompt/skill/review-process-skill.md
cargo test -p minix-kernel <抽到的测试名>
```

### E3. 语义 / 安全类 P1 时限与 OQ 化

**现状证据**：P1 定义包含"叶函数语义不对齐且未注释说明"、"硬件未抽象为 trait"等（`prompt/review-rules/review.md:541-547`）；停止规则允许"连续两轮新 P1 ≤1"即收敛（Step 7.1）。

**改动点**：

1. `prompt/review-rules/review.md` P1 分类标注"正确性相关 P1 子类"清单：叶函数语义不对齐（§14.2）、硬件语义未抽象、错误码/errno 不对齐、BKL 保护声明与实际不符、代码注释中的 C 引用错误（§14.4）等。
2. 规则：正确性相关 P1 不允许无期限进 backlog——STATE.md Open 列表必须带"发现日期"；超过 30 天未修必须二选一：修，或升级为 OQ（附不修理由）交用户裁决。Step 7.1 停止规则追加一句："存在未处置的正确性 P1 → 不得标 CONVERGED"。
3. STATE.md 模板（`prompt/skill/review-process-skill.md`）Open 列表增加"发现日期 / 类别（correctness | other）"两列。

**执行注意**：类别判定由执行者做，但"是否 correctness"不允许含糊——拿不准时宁可按 correctness 处理（保守）。

**验收**：

```bash
rg -n "正确性相关 P1|发现日期|correctness" \
  prompt/review-rules/review.md prompt/skill/review-process-skill.md
```

---

## 七、工作流 F：设计快照与架构权威

> 来源：1.5 的缺口 5/2（外锚部分）。目标：打破"design/doc/code 三方一致地错"的回音室，给'自产'的设计与架构断言补外部锚点。

### F1. design 快照 C 锚点 + 契约覆盖矩阵

**现状证据**：Gate H 校验 doc 对 design，而 design 是同一 AI 本轮从 C 源推导；Gate B 契约表只有 Top 5，抽样之外没有归档要求。

**改动点**：

1. `prompt/review-rules/review-process.md` Step 0.3.4 的 design 模板要求：每个设计决策附 **C 证据**（`file:line` + 一句行为描述）；Step 1.6 的 H.3 从"比对文本"改为逐条 `sed` 验证锚点。
2. Gate B 增加"契约覆盖矩阵"：§2.3 列出的每个函数必须归档到四类之一——`契约表`（语义变更/有风险，需全 8 字段）/ `语义未变` / `ARCH`（附外部锚点）/ `缺口`（P0/P1）；缺归档视为 Gate B 不完整。Top 5 保留为"必须全 8 字段展开"的最小集，其余函数可用一行归档。
3. 同步：`prompt/skill/review-core-semantics-skill.md`、`prompt/skill/review-process-skill.md`。
4. **收编 Proposal #12（design.md §X-Y"权威位置"段）**：该 Proposal 至今"⏸ 待用户确认后落地"。F1 的"C 证据"要求与其同源——若 design 模板采用"权威位置"段（同一常量/函数在多个 crate 有定义时标注唯一权威），一并实现并把状态回填为"✅"（`CLAUDE.md` 与 `.claude/rules/review-core.md` 两处 Proposal 列表）；不实现则在本任务说明里显式写"Proposal #12 保持 ⏸，理由 X"。

**执行注意**：矩阵的每个"语义未变"判定也必须有依据（C 函数与 Rust 函数行为对照一句话），不能只写标签。契约覆盖矩阵对符号量大的模块（>100 个 §2.3 符号）是重活，允许按文件分批归档；执行时估时按实测上调（原 2 小时是"Top 5 抽样"时代的估算）。

**验收**：

```bash
rg -n "契约覆盖矩阵|C 证据" prompt/review-rules/review-process.md \
  prompt/skill/review-core-semantics-skill.md
# 1) 快跑：用 sched/12（C 语义域 12 个符号）试跑矩阵并统计归档率
# 2) 成本试金石：再选一个符号量大的模块（如 RS/VM）实测归档耗时，写进执行记录并校准估时
```

### F2. `[ARCH]` 外部权威锚点

**现状证据**：C ground truth 只有 i386/earm 32 位；aarch64/riscv64 行为没有 C 对照；`[ARCH]` 标注只保证"已声明"，不保证"声明正确"。

**改动点**：

1. `prompt/review-rules/review-doc-checklist.md` §2.5（架构演进说明）与 `prompt/review-rules/review.md` 的 `[ARCH]` 标注机制补充：架构敏感领域（异常/中断/分页/时钟/特权级切换）的 `[ARCH]` 决策必须附**外部权威锚点**——spec 名称 + 章节号 + 原文引句（如 ARM ARM 的 `D13.2`、RISC-V Privileged Spec 的 `§3.1.6`、Intel SDM 卷章），或 Minix3 C 源码锚点。
2. 检查接入：Step 3.5 的 3.5.1"外部知识标记"把"无外部锚点的 ARCH 断言"列为可疑点；影响行为又给不出锚点 → P1。
3. `prompt/review-rules/review-patterns.md` 增加一条模式："ARCH 断言无外部权威锚点"（下一个空闲编号是 **84**；新增按附录 4 的 Pattern housekeeping 清单执行，含计数宣称同步与 lint L5 验证）。

**执行注意**：外部 spec 锚点无法机器校验，要求"章节号 + 原文引句"以便人工复核；本项目不要引用不可访问的付费规范全文，给章节号即可。

**验收**：

```bash
rg -n "外部权威锚点" prompt/review-rules/review-doc-checklist.md prompt/review-rules/review.md
# 抽一篇含 riscv64/aarch64 决策的文档（如 16-smp 或 15-clock-timer）试查锚点
```

---

## 八、工作流 G：代码可读性与代码架构可读性

> 来源：1.6 的第三轮审计。目标：给代码可读性建"止损 + 机检 + 架构验收"，对存量同样只登记不重写。

### G0. 判定标准（什么算代码可读性问题）

**微观（代码可读性）**：

- 新增 `pub` 项无 `///` 文档注释；新增模块（`mod.rs`/`lib.rs`）无 `//!` 职责说明；
- 注释语言在规则（§9.2 英文）与现实之间混用且未登记；
- 注释只解释"是什么"不解释"为什么"；复杂算法/非常规实现无行内理由；
- 命名违反 Rust 规范，或与 Minix3 C 名脱节导致无法双向 grep 对照。

**宏观（代码架构可读性）**：

- 新模块/新 trait 不能从 `//!` + `os/README.md` 看出职责、边界与依赖方向；
- 依赖方向违例（如 `arch` 依赖 `kernel`、`libs` 依赖 `servers`）；
- 核心路径无法只靠代码 + 注释走通（如 boot → kmain → scheduler → IPC）；
- trait 边界没有"为什么这样切"的理由说明（patterns 79-82 已覆盖部分 smell，但缺总体可读性标准）。

### G1. 代码可读性止损：`tools/code-style-lint.sh`

**现状证据**：`prompt/review-rules/review-code-checklist.md:216-232` 写"强制"，但无 Gate、无证据要求；全仓无 `missing_docs`；无 `clippy.toml`/`rustfmt.toml`；CI 只有 `vm-tests.yml` 对 `minix-vm` 跑 clippy。

**改动点**：

1. 写 `tools/code-style-lint.sh`（bash + `rg`，风格对齐 `tools/doc-style-lint.sh`）：
   - 检查项：新增 `pub` 项缺 `///`；新增模块文件缺 `//!`；`unsafe` 无 SAFETY（直接复用 `tools/unsafe-audit.sh`，不重复实现）；注释语言（默认 warning 级）。
   - 模式：`--diff [RANGE]`（只查新增/修改行，默认阻断）、`--report`（全量统计，只报告）、`--self-test`（内置正反例）。
2. 首跑 `--report` 产出存量基线 `tools/code-style-baseline.txt`（入库）；增量门只阻断新增违规。
3. 注释语言检查的级别取决于 OQ19 的裁决；未裁决前默认 warning，不阻断。

**执行注意**：`pub` 文档注释检查对宏生成项会误报，基线收纳误报即可；本脚本与 G2 的 rustc lints 是互补关系（一个查增量、一个全库编译期检查），不要互相替代。

**验收**：

```bash
tools/code-style-lint.sh --self-test
tools/code-style-lint.sh --report | tail -5
# 新增一个无文档的 pub fn → --diff 应报；验毕还原
```

### G2. 让 rustc/clippy 接管（配置与 CI）

**改动点**：

1. 各 crate 根加 `#![warn(missing_docs)]`——先 `warn` 不 `deny`；首跑记录各 crate 警告数；文档覆盖高的 crate（如 `minix-types`、`minix-arch`）可考虑升级 `deny`，升级决策登记 OQ18。
   - 落点：有 `lib.rs` 的 crate 放 `lib.rs`；纯 binary/no_main crate（如 `boot-shim`、`qemu-tests` 内核）放 `main.rs` 顶部（`#![no_main]` 相邻）；两种情况都要确认 no_std 属性顺序合法（内部属性区）。
2. CI（`.github/workflows/`）：把 `cargo clippy -D warnings` 从只有 `minix-vm` 扩到 workspace 全部可构建 crate（按 crate 分批，跑不动的先排除并登记原因）。
   - 现状：`vm-tests.yml` 的触发路径只含 `os/servers/vm/**` 等，clippy 也只对 `minix-vm`；扩 workspace 时可新建独立 `lint.yml`，或把 lint 作业抽成独立 job（避免被单 crate 的路径过滤挡住）。
3. `cargo doc --no-deps` 检查：默认 rustdoc warning 不会让命令失败，必须用 `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`（或 CI 里显式导出该环境变量）才具备阻断力；先跑出警告清单，再决定是否阻断。
4. 可 hosted 测试的 crate 增加 `cargo test --doc`（doctest 当前是卓越性 P2；先纳入报告，不阻断，升级时机登记 OQ22）。

**执行注意**：workspace 含 no_std/裸机 crate，clippy 可能因 target 不可用而失败；按 `os/.cargo/config.toml` 现有 target 配置分批跑，失败清单登记而非跳过。`qemu-tests` 与 `boot-shim` 的构建 target 是 UEFI/no_main，lint 前先确认它们能被 stable clippy 检查（不行则登记豁免 + 理由）。全 workspace 约 1010 个 `.rs` 文件、19 个 libs，2-3 小时大概率不够；先产失败清单分批推进，必要时拆成独立批次并上调估时。

**验收**：

```bash
rg -n "missing_docs" os/*/src/lib.rs os/*/*/src/lib.rs 2>/dev/null | head
rg -n "clippy|cargo doc|test --doc" .github/workflows/*.yml
```

### G3. 代码架构可读性验收

**现状证据**：`os/README.md`（架构总览 + 依赖图）与 `os/arch/README.md`（ISA vs board 设计理由）质量不错，但 `rg "os/README" prompt/` 零命中；`tools/` 下无依赖方向脚本；无"新读者路径"检查。

**改动点**：

1. 漂移检查：改模块/trait/依赖时，要求同步 `os/README.md` 与对应 crate 的 `//!`；review 中 README 与代码不一致列为 P1。接入点：`prompt/review-rules/review-code-checklist.md` §6 与 §9.1（加一条"架构文档同步"）。
2. 写 `tools/check-dep-direction.sh`：用 `cargo metadata` 校验分层方向——
   - `arch` 不得依赖 `kernel`、`servers`、`commands`；
   - `kernel` 不得依赖 `servers`、`commands`；
   - `libs` 不得依赖 `arch`、`kernel`、`servers`、`commands`；
   - `servers` 不得依赖 `kernel`、`commands`；
   - `commands` 不得依赖 `kernel`、`servers`。
   违例 → 非零退出；接入 Step 3.6 证据块（`gate-evidence-dep`）。
3. 新读者路径抽查：`prompt/skill/review-code-skill.md` 增加"路径可读性"检查项——选 1 条核心主线（候选：boot → kmain → scheduler → IPC），只读代码 + `//!` + README 走通，记录卡点；卡点判 P1（缺文档/注释）或 P2（组织问题）。
4. `os/README.md` 的依赖图与实际 `cargo metadata` 输出对账（并入 `check-dep-direction.sh` 的检查项）。

**执行注意**：依赖方向规则先按 `os/README.md` 现状冻结，不从"理想分层"出发；发现现状违例登记 OQ，不擅自改结构。

**验收**：

```bash
tools/check-dep-direction.sh | tail -5
rg -n "check-dep-direction|路径可读性" prompt/review-rules/review-process.md prompt/review-rules/review-code-checklist.md
```

### G4. 卓越性提级：新增代码的可读性违规升级为正确性 P1

**现状证据**：`prompt/skill/review-excellence-skill.md` 写"卓越性 P1 是'可以更好'……建议修复"；`prompt/review-rules/review-code-excellence.md` §八写"卓越性问题不阻塞 Review 收敛"；`full-review` 不做卓越性重构。于是"新增 pub 无文档、新增模块无 `//!`、注释语言混用"永远不阻塞。

**改动点**：

1. `prompt/review-rules/review.md` P1 表与 `review-code-checklist.md` §9.1：把三类明确为**新增代码**的正确性 P1（存量仍按卓越性/backlog）：新增 pub 缺 `///`、新增模块缺 `//!`、复杂算法缺"为什么"注释。
2. Step 3.6 的 `gate-evidence-code` 增加一行"代码可读性增量"（G1 `--diff` 结果）。
3. 同步 `.claude/rules/review-core.md`、`prompt/skill/review-code-skill.md`、`prompt/skill/review-process-skill.md`。

**执行注意**："新增"以 G1 的 `--diff` 为准，避免存量被判 P1 造成全库 FAIL。

**验收**：

```bash
rg -n "新增代码.*P1|可读性增量" prompt/review-rules/review.md prompt/review-rules/review-code-checklist.md
```

### G5. 工作流 G 的验收汇总

- [ ] G1 自测通过 + 基线入库 + `--diff` 实测能拦截新增违规。
- [ ] G2 配置与 CI 落地，各 crate 警告清单记录在案。
- [ ] G3 依赖方向脚本可用，README 对账完成，路径抽查执行一次并记录卡点。
- [ ] G4 规则同步，`gate-evidence-code` 含可读性增量行。
- [ ] C 批同步 + 回归全绿。

---

## 九、工作流 H：卓越层——把"最高目标"变成可执行判据（2026-09-18 新增）

> 来源：1.7 的缺口 2。目标：让"最美的代码 + 教科书的文档"从口号变成带证据的交付物。与 A-G 的本质区别：A-G 的产物是"防御性规则 + 工具"（避免变差），H 的产物是"读者与代码真的变好"（追求变美）。范围默认只对新增/触碰面阻断，存量只登记（与 OQ21 同哲学）。
>
> **前置**：H 批开工前先完成 D5 改写（符号锚点 + 卫生分层），否则卓越批的注意力会被行号机械活持续挤占。
>
> **"触碰"的判定（防误伤存量）**：以本次 git diff 实际改动的章节/代码为准；纯审查、未修改的存量内容只登记不阻断（OQ21/OQ25/OQ26/OQ27）。H1-H3 的注意力预算来自 D5 收缩 1.0a~1.0g 释放出的机械检查时间。
>
> **收敛门**：H 批是否完成以 **H4 为准**（样章文档 + 模块双达标 + 用户认可，用户门）。H1-H3 是方法：它们的发现进 P1 池（新增/触碰面修复或写明延期理由，存量登记），不单独改变全局 CONVERGED 口径——是否给卓越 P1 单列收敛口径，见 OQ29，由用户裁决。

### H1. 读者复述测试提级（文档）

**现状证据**：`prompt/review-rules/review-process.md:705-708` 的 structure.md §11 是 12 节之一（Step 0.5.2 与其它 11 节同表逐项判定，没有"复述证据"这一必答产物；Gate D-6 只要求 structure.md 生成 + 12 节评审完成）；`prompt/review-rules/review-cmds.md:90` 把它只当 style-fix 的自检；`review-doc-excellence.md:126` 有"能让读者闭眼复述状态机"的表述，但不是可判定判据。

**改动点**：

1. structure.md 模板 §11 升级为**必答产物**：写出"不看代码、不用函数名，用机制语言复述 Ch1"的结论（一页以内），判断标准 = 能否说出"这个机制解决什么矛盾、状态怎么转、边界在哪"。
2. `prompt/review-rules/review-doc-checklist.md:86` 的 1.3 与 `review-process.md` Step 0.5.2 的通过门槛同步：复述不通过 → 教学类 P1（新增/触碰章节必须修复或写明延期理由，存量登记；OQ25/OQ29），并在 scan.md §structure.md 评审与 Issue List 显著位置单列。
3. `review-doc-excellence.md` §4.3"运行时视角三问可泛化性"与 §4.5 概念教学升级：A/B/C/D 分级必须引用复述结果；`review-process-skill.md` / `review-doc-skill.md` / `.claude/skills/review-scan/checks/doc.md` 按 C1/C2 同步。

**执行注意**：复述文本必须由审阅者自己写（不是把 Ch1 抄一遍），在 scan.md 里留痕；允许标注"复述在 X 处卡住"。

**验收**：

```bash
rg -n "读者复述|复述测试" prompt/review-rules/review-process.md prompt/review-rules/review-doc-checklist.md \
  prompt/skill/review-process-skill.md prompt/skill/review-doc-skill.md prompt/review-rules/review-doc-excellence.md
# 用 H4 样章跑一次，复述文本与卡点写入样章产物
```

### H2. 非法态不可表达测试（代码）

**现状证据**：`prompt/review-rules/review-code-excellence.md:28,36-38` 的"make wrong state unrepresentable"只是 [MEDIUM] 检查项；`review-code-checklist.md:20,95` 有语义（"能否构造出逻辑非法但类型合法的状态"）但没有门；Gate D 第 2 项（trait ≥2 impl）只是它的弱形式。

**改动点**：

1. 新增产物模板"非法态封堵清单"：每个核心状态机 / 句柄 / 地址类型一行——状态集 → 类型如何表达 → 封堵了哪些非法态 → 封不住哪些 → 结论（合格 / 设计类 P1）。
2. 判级：核心状态机封不住关键非法态 → 设计类 P1（新增/触碰代码必须修复或写明延期理由，存量登记；OQ26/OQ29）；普通类型表达力问题 → 卓越性 P2。
3. 接入 `review-cmds.md` §四 code-excellence 的产物（设计对比表旁列一份封堵清单）与 Step 3.6 的 `gate-evidence-code`（增一行"非法态封堵"）。Step 3.6 由 B3.3 新建；若尚未落地，先接 code-excellence 产物，B3.3 落地后再迁移。
4. 首批样本（对话识别的三处可疑设计，均能躲过现有机械模式；执行时先复核具体位置）：wire 布局裸常量（常量散落，无 newtype 承载）、unsafe 散装（边界未收敛到少数 safe 封装）、PM Lifecycle 半封装（状态机未全封）。

**执行注意**：不要求"所有类型都 newtype"（那是过度抽象，模式 24）；只要求核心状态机的合法状态集能被类型枚举。判定要写"为什么封不住"，避免标签化。

**验收**：

```bash
rg -n "非法态封堵|invalid states" prompt/review-rules/review-code-excellence.md prompt/review-rules/review-code-checklist.md prompt/review-rules/review-cmds.md
# 对一个模块（建议 PM lifecycle 或 VM phys_pagestate）产出清单，至少一条改进落地并配 cargo test
```

### H3. 文档 Rust 代码块逐块审查（文档 × 代码）

**现状证据**：模式 73 / Step 1.0b（`review-process.md:1283-1298`）与 doc-checklist §2.4b 只查"示例与源码同步"（`static mut` / edition 漂移），不评判示例本身是否 idiomatic；读者学 Rust 的入口恰恰是这些块。

**改动点**：

1. 写轻量抽取脚本 `tools/doc-snippet-extract.sh`：按围栏抽出 `rust` / `ignore` / `no_run` 块，输出 `{doc}:{start-end}:{语言}` 清单；不发明复杂解析。
2. **四类分类**（每块必须归一类）：逐字引用（贴仓库代码）/ 教学简化（示意，可省略但必须标注）/ 反面示例（必须标 ❌ 且给正确版本）/ 签名示意（伪代码或签名骨架，必须标注）。
3. 最佳实践判据（`review-code-excellence.md` §16/§17/§19 的教学简版 + 本项目特例）：所有权/借用是否展示正确；`unsafe` 是否带安全论据（"这里假设 X 成立"）；kernel 语境是否出现 `Rc`/`RefCell`/`Box<dyn Error>`；`unwrap`/`panic!` 在被教路径是否配解释；`clone()` 是否用来绕借用且不解释；`static mut` 是否已迁移；命名与类型表达是否表达意图。
4. 产物表：`| 位置 | 类型 | 是否可编译 | 是否 idiomatic | 问题 | 判级 | 建议 |`；判级：未标注的反模式 P1（新增/触碰块必须修复或写明延期理由；存量登记 OQ27）；逐字引用漂移 P1（模式 73）；简化块风格瑕疵卓越性 P2；分类缺失 P2。
5. 接入：`full-review` Step 3.6 与 `style-fix` 的文档类产物；不新增 Gate 字母（沿用 OQ2 的裁决）。Step 3.6 由 B3.3 新建；若尚未落地，先接 style-fix 产物，B3.3 落地后再迁移。

**执行注意**：不得把"教学简化"一律判违规——判据是"是否标注 + 是否教了反模式"，不是"是否达到生产级"。

**验收**：

```bash
tools/doc-snippet-extract.sh notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md | head
# 对抽取出的块逐块分类并出表；至少修 1 处未标注的反模式或漂移示例
```

### H4. 卓越样章示范（把卓越性变成可见交付）

**任务**：选一篇文档 + 一个代码模块，完整走 H1/H2/H3 + `review-doc-excellence.md` §4.1-4.5 + `review-code-excellence.md` §16-21，产出第一个 A/B 级样板。

**建议样本**：文档选已人工审阅过、代码集中的一篇（候选 `01-stage-kernel/05-clock-interrupt-init.md` 或 `02-stage-vm/15-ipc-dispatch.md`）；模块选 PM Lifecycle 或 VM phys_pagestate（H2 样本所在域，执行时先复核）。

**验收**：文档 §4.1-4.5 至少 B 且复述通过；模块非法态清单闭环且至少一条改进落地；用户按"第一流技术博客"标准实测认可（**用户门，不是工具门**：结论写成"用户认可记录"，不得包装成机器 PASS）。结论追加到本文件 H4 段。

### H5. 工作流 H 的验收汇总

- [ ] H1 规则同步，复述结论成为 structure.md 必答产物。
- [ ] H2 清单模板进 code-excellence 与 Step 3.6，样章模块至少一条落地。
- [ ] H3 抽取脚本可用，样章文档全块分类出表。
- [ ] H4 样章双达标（文档 + 模块），用户认可。
- [ ] H 批收敛门 = H4（H1-H3 发现进 P1 池并留痕；卓越 P1 是否单列口径由 OQ29 裁决）。
- [ ] C 批同步 + 回归全绿。

---

## 十、工作流 I：内容优先——让命令能跑、让新人能读懂（2026-09-18 新增）

> 来源：1.7 的缺口 4。这两项是真实内容交付，不属"存量正文清洗"（A4 编辑战役）；未被触发时按第十一部分默认顺序，被触发时插到锚点卫生批次之前。

### I1. 18-stage 命令契约表 Requires 回填（P1-1）

**现状证据**：`notes/rewrite/fork-syscall-rewrite/18-stage-commands/todo.md:15,44-60`——命令契约表缺 Requires 列，读者无从知道实现一个命令缺哪个 `minix-sys` 函数；模板与示例已写入 `18-stage-commands/99-global-concepts.md` §3，逐命令回填待做。

**任务**：按 99 §3 模板逐命令回填 Requires；跨文档统计同一 API 的消费者数量，作为 14-stage-runtime 补齐顺序的依据。

**验收**：

```bash
rg -c "Requires" notes/rewrite/fork-syscall-rewrite/18-stage-commands/06-file-ops.md
# 各篇计数与 §5.2 命令归属表的命令数一致；跨文档 API 消费统计写入 18-stage todo
```

### I2. 99 汇总篇重写

**现状证据**：对话裁定 99 汇总篇决定新人能不能读懂系统，应优先于锚点修复。`01-stage-kernel/99-global-concepts.md` 与 `18-stage-commands/99-global-concepts.md` 是两处载体（后者已含 §3 Requires 模板）；本条不作内容断言——执行时先按 H1 方法做一次复述测试，用卡点定位重写范围（该文档可能正被并发会话改动，动前按 0.8 的隔离规则确认）。

**任务**：以 H1 的复述判据为目标重写（读者不看代码能讲出系统分层与关键机制）；与前一阶段文档的交叉引用一并校验。

**验收**：重写后按 H1 方法做一次复述，卡点数写进结论；`style-bible` 与文风 lint（A2）零 error。

---

## 十一、任务汇总与依赖顺序

本表是能力清单（有哪些任务、依赖关系、产物），不是执行排期；什么时候执行由 0.8 的触发方式决定（你在对话里提出改进需求后才动手）。

| 顺序 | ID | 任务 | 依赖 | 产物 |
|---|---|---|---|---|
| 1 | A1 | 规则修订（过程痕迹禁入正文） | - | 5 个规则文件的改动 |
| 2 | A2 | `tools/doc-style-lint.sh` | A1（规则表与规则文本对应） | 脚本 + 自测 |
| 3 | A3 | 增量门接入 | A2 | 4 处引用 |
| 4 | B1 | 关联代码声明规范 | - | checklist + 适配层 |
| 5 | B2 | `tools/doc-code-map.sh` | B1 | 脚本 |
| 6 | B3 | 流程修订（Step 0 / Step 3.6） | B1、B2 | review.md + review-process.md + 适配层 |
| 7 | B4 | 门禁与薄壳扩展 | B3 | gate 脚本 + skill 文档 |
| 8 | C1 | 三端同步 | A1、A3、B3、B4 | 派生目录更新 |
| 9 | C2 | 回归 | C1 | 脚本输出 |
| 10 | B5 | 端到端演练 | C2 | 演练产物 + B5 结论 |
| 11 | A4 | 存量统计（可与 B 并行） | A2 | 本文件 A4 段 |
| 12 | C3 | 总验收（第一轮） | 全部 | 本文件执行记录 |
| 13 | D2 | unsafe 审计工具与增量门 | A2 的脚本风格 | 脚本 + 基线 + 规则 |
| 14 | D3 | coverage 多 `--c-dir` 与语义域一致性 | - | 工具改造 + 规则 |
| 15 | D5 | 符号锚点迁移 + 卫生项分层（2026-09-18 改写；定义以 §五 工作流 D 的 D5 为准） | - | 脚本 + 规则 + 迁移统计 |
| 16 | D1 | QEMU 门 | - | 规则（Step -0.5 + 强制门） |
| 17 | D4 | 证据脚本化 | D1、D2（证据产物路径） | gate 脚本 + 规则 |
| 18 | E2 | 测试正确性抽样门 + P0-test-wrong | - | P0 分类 + 规则 |
| 19 | E3 | 正确性 P1 时限 | - | 规则 + STATE 模板 |
| 20 | E1 | zero-P0 独立复核 | - | 规则 + VERIFY 模板 |
| 21 | F1 | design C 锚点 + 契约覆盖矩阵 | - | 规则 + 模板 |
| 22 | F2 | `[ARCH]` 外部锚点 | - | 规则 + patterns |
| 23 | C1/C2 | 三端同步 + 回归（第二轮） | 13-22 全部 | 派生目录 + 脚本输出 |
| 24 | D6 | 正确性门演练 | 23 | 演练产物 + 执行记录 |
| 25 | C3 | 总验收（第二轮） | 13-24 | 本文件执行记录 |
| 26 | G1 | `tools/code-style-lint.sh` + 基线 | A2 的脚本风格、D2（SAFETY 复用） | 脚本 + 基线 |
| 27 | G2 | `missing_docs` + CI 扩展 | - | 配置 + CI |
| 28 | G3 | 架构可读性验收（漂移 + 依赖方向 + 路径抽查） | - | 脚本 + 规则 |
| 29 | G4 | 可读性提级（新增代码 P1） | G1 | 规则 + Step 3.6 证据 |
| 30 | C1/C2 | 三端同步 + 回归（G 批） | 26-29 | 派生目录 + 脚本输出 |
| 31 | C3 | 总验收（G 批） | 26-30 | 本文件执行记录 |

> 顺序说明：第一轮（1-12）先把文风与"文档+代码"机制建起来；第二轮（13-25）补正确性证据与独立验证；第三轮（26-31）补代码可读性与架构可读性。每批的 C1/C2/C3 各跑一次，不要合并——但如果 G 批与 D/E/F 批同一次实施，可以共用一次同步与回归（在两批的 C3 记录里注明合并事实）。
>
> **触发密集时的优先顺序（不是排期；2026-09-18 按北极星重排）**：D5 改写（符号锚点 + 卫生分层，先止住机械噪声）→ H1 + H3（复述测试与代码块审查，直接服务"教科书文档"）→ H2（非法态测试，服务"最美代码"）→ A2 + A3（文风增量门，防止继续腐化）→ B1 + B2 + B3（关联代码清单与 Step 3.6，恢复 full-review 本意）→ D2（unsafe 审计，风险最大面）。每批做完跑 C1/C2 让机制生效，然后回到真工作，其余任务继续待命。
>
> **内容优先项（独立于工具批）**：I1（Requires 回填）与 I2（99 重写）不属机械批次；未被触发时按默认顺序在 H 批之后执行，被用户触发时插到锚点卫生批次（D5）之前（见 OQ28）。
>
> **不建议先做的**：D3（coverage 工具改造，工作量中等且影响既有命令）、E1（独立复核规则，需要用户拍板复核形态）、F1/F2（设计与架构锚点，属于长期质量）、G2（CI 扩展，需要全 crate 摸一遍构建），以及任何"以行号为中心"的新工具或新门（第四轮已作废该方向）。

> **第四轮追加任务（2026-09-18）**——"层"是本轮新增的排序依据：**卓越层**任务优先于**机械层**任务；D5 已改写，不再做行号自动化。行 32 是 D5 改写后的重新入表，**定义以 §五 工作流 D 的 D5 为准**，本表只作索引。

| 顺序 | ID | 层 | 任务 | 依赖 | 产物 |
|---|---|---|---|---|---|
| 32 | D5 | 底线/卫生 | 符号锚点迁移 + 卫生项分层（改写） | - | 脚本 + 规则 + 迁移统计 |
| 33 | I1 | 内容 | 18-stage Requires 回填 | - | 各篇契约表 Requires 列 |
| 34 | I2 | 内容 | 99 汇总篇重写 | H1（复述判据；未落地时用人工等效方法） | 重写稿 + 复述结论 |
| 35 | H1 | 卓越 | 读者复述测试提级 | D5（先完成符号锚点迁移） | 规则 + 样章复述 |
| 36 | H2 | 卓越 | 非法态不可表达测试 | B3.3（Step 3.6；未落地则先接 code-excellence） | 模板 + 样章模块落地 |
| 37 | H3 | 卓越 | 文档 Rust 代码块逐块审查 | B3.3（Step 3.6；未落地则先接 style-fix） | 抽取脚本 + 块清单表 |
| 38 | H4 | 卓越 | 卓越样章示范 | H1/H2/H3 | 样板文档 + 模块 + 用户认可 |
| 39 | C1/C2 | 流程 | 四轮同步 + 回归 | 32-38 | 派生目录 + 脚本输出 |
| 40 | C3（第四轮） | 流程 | 总验收 | 39 | 卓越度量变化 + 执行记录 |

**第四轮推荐顺序**：D5 改写（一次性符号锚点迁移，立刻消除机械噪声）→ H1 + H3（文档侧卓越判据与代码块审查）→ H2（代码美学判据）→ I1/I2（内容优先项）→ H4（样章收口）。

---

## 十二、Open Questions（执行者需向用户确认的点）

| # | 问题 | 推荐默认 |
|---|---|---|
| OQ1 | 头部字段统一名用 `Rust 实现` 还是 `Rust 模块`？ | `Rust 实现`，`Rust 模块` 兼容别名 |
| OQ2 | 关联代码维度用"Step 3.6 + gate-evidence-code"还是新增 Gate I？ | Step 3.6 方案（改动面小；新增 Gate 需同步 8 处 Gate 列表） |
| OQ3 | 文风 lint 对存量是否零容忍？ | 存量只报告；增量阻断 |
| OQ4 | 演练文档用 `02-stage-vm/15-ipc-dispatch.md` 是否同意？ | 同意（头部字段完整、代码集中） |
| OQ5 | 是否需要提交钩子（pre-commit）或 CI 钩子？ | 本轮不做，跑两轮 review 后再评估 |
| OQ6 | 编辑战役（存量正文清洗）何时启动？ | 用户已表态延后；A4 只登记 |
| OQ7 | `[ARCH: ...]`、`TODO`、`DEFERRED` 保留在正文，确认？ | 保留（前者是项目硬要求，后两者有既有流程管理） |
| OQ8 | QEMU 门触发面用 `os/kernel/**`、`os/arch/**`、`os/boot-shim/**` 是否足够？每次 review 都跑还是每批改动跑一次？ | 上述三目录触发；单例优先，跑不动才全量 |
| OQ9 | unsafe 基线策略：存量冻结只登记，还是设"逐月递减"目标？ | 冻结 + 专项 backlog；递减目标等治理批次再定 |
| OQ10 | 是否要引入 Miri/loom（需 nightly 与逐模块可行性评估）？ | 暂不；留待用户决定是否另立项目 |
| OQ11 | `coverage-extract.py` 改多 `--c-dir` 是否接受（影响既有命令习惯与产物格式）？ | 接受；保持单域向后兼容 |
| OQ12 | zero-P0 独立复核优先哪种实现：跨工具只读复核还是跨 session 换模型？ | 有跨工具条件时优先跨工具；单 agent 用跨 session 兜底 |
| OQ13 | 新增 P0-test-wrong 后，是否需要回扫既有 scan 产物？ | 不回扫；新规则只对新 review 生效 |
| OQ14 | 正确性 P1 时限 30 天是否合适？超期升级 OQ 是否要用户逐条裁决？ | 30 天；超期批量上交用户，一次裁决 |
| OQ15 | 契约覆盖矩阵会不会把 Gate B 变成"全函数归档"的重活？ | 允许"语义未变"一行归档，只有语义变更/风险函数才全 8 字段 |
| OQ16 | D4 证据脚本化后，旧格式 scan.md 产物是否迁移？ | 不迁移；新规则只约束落地之后的 review |
| OQ17 | `[ARCH]` 外部锚点先限四类机制（异常/中断/分页/时钟与特权级切换）是否足够？ | 先四类；观察两轮再决定是否扩面 |
| OQ18 | `missing_docs` 用全局 `warn` 还是逐 crate 升级 `deny`？哪些 crate 先升？ | 全局 warn；`minix-types`/`minix-arch` 评估后先升 |
| OQ19 | 注释语言裁决：统一英文（执行 §9.2）、允许中文（改规则）、还是双轨登记？ | 新增代码统一英文；存量中文登记为 backlog（需用户确认后才可改 §9.2） |
| OQ20 | 依赖方向规则按现状冻结还是按"理想分层"修正？发现现状违例怎么办？ | 按现状冻结；违例登记 OQ，不擅自改结构 |
| OQ21 | 可读性提级是否只限新增代码（存量不判 P1）？ | 只限新增（以 `--diff` 为准），存量留给治理批次 |
| OQ22 | `cargo test --doc` / doctest 何时从"报告"升级为"阻断"？ | 先观察一轮；对已有 doctest 的 crate 先接 |
| OQ23 | 符号锚点约定：`path:fn name` 前缀风格是否统一？C 源启发式解析的歧义如何处理？ | 统一 `fn/struct/enum/trait/const` 前缀；歧义要求文档补 `impl Type::method` 限定，工具不加特例 |
| OQ24 | 卫生项是否需要判级？是否允许任何情况下升级为 P1？ | 不判级、只记录；唯一例外："锚点 0 定义"（符号消失）按 P0-fact 处理，属正确性不属卫生 |
| OQ25 | 复述测试不通过（教学类 P1）是否阻塞 CONVERGED？ | 新增/触碰章节进 P1 池并必须在 scan 单列（修复或写明延期理由）；存量进 backlog；H 批完成以 H4 为准（见 OQ29） |
| OQ26 | 非法态封堵清单对存量模块是否强制？ | 只对新增/触碰的核心状态机强制；存量登记（与 OQ21 一致） |
| OQ27 | 代码块审查范围：全量还是增量？ | 新增/触碰块强制；存量只出清单（与 A4 同哲学） |
| OQ28 | I1/I2 与 D5/H 批的执行次序？ | 默认 D5 → H1/H3 → H2 → I1 → I2 → H4；内容项（I1/I2）被用户触发时插到 D5 之前 |
| OQ29 | 卓越类 P1（复述/非法态/反模式）是否单独设全局收敛口径？ | 默认不单列（避免与正确性 P1 两套收敛规则），H4 作 H 批完成门；用户要更强约束再升级 |

**OQ 阻塞关系与默认执行**（用户不在场时：可先按默认执行，事后复核）：

| OQ | 阻塞的任务 | 说明 |
|---|---|---|
| OQ8 | D1 | 触发面/频率未定；默认复用 CI 路径清单 + 每次 review 跑受影响单例 |
| OQ12 | E1 | 复核形态未定；默认"有跨工具条件优先跨工具，否则跨 session 兜底" |
| OQ18 | G2 | 升级 `deny` 的 crate 名单未定；全局先 `warn` 不阻塞 |
| OQ19 | G1 | 注释语言检查级别未定；默认 warning 不阻断 |
| OQ20 | G3 | 现状依赖违例处置未定；脚本先只报告不阻断 |
| OQ21 | G4 | 提级范围默认只限新增代码 |
| OQ23 | D5 | 锚点约定未定；先按推荐默认执行，迁移报告里复核歧义清单 |
| OQ25 | H1 | 新增/触碰章节进 P1 池并在 scan 单列；存量登记；H 批完成以 H4 为准（OQ29） |
| OQ27 | H3 | 存量只登记，新增/触碰块强制 |
| OQ29 | H1-H3（口径；不阻塞执行） | 默认不单列口径；H4 用户门不受影响 |

其余 OQ（1-7、9-11、13-17、22、24、26、28）不阻塞开工，按推荐默认执行。

---

## 附录 1：关键路径地图（执行者速查）

| 类型 | 路径 | 说明 |
|---|---|---|
| 规则源 | `prompt/review-rules/review-cmds.md` | cmd 定义、通用门、style-bible 文本（§六） |
| 规则源 | `prompt/review-rules/review-process.md` | Step 0-7、Gate 注册表、修复阶段 |
| 规则源 | `prompt/review-rules/review.md` | 模式 A/B/C、优先级链 |
| 规则源 | `prompt/review-rules/review-doc-checklist.md` | 文档结构规范 + 检查清单 |
| 规则源 | `prompt/review-rules/review-code-checklist.md` | 代码检查清单（Step 3.6 的检查维度来源） |
| 适配源 | `prompt/skill/*.md` | 9 个领域 skill；`.trae/`/`.codex/` 的派生源 |
| cmd 薄壳 | `prompt/skill/cmds/{full-review,style-fix,...}/SKILL.md` | 任务入口摘要 |
| 派生端 | `.claude/rules/`、`.claude/skills/review-scan/` | Claude 运行时（always-on + on-demand） |
| 派生端 | `.codex/skills/` | Codex 运行时（自动派生） |
| 派生端 | `.trae/skills/` | Trae 运行时（自动派生） |
| 工具 | `tools/generate-derived-skills.sh` | 派生生成 |
| 工具 | `tools/check-review-rules.sh` | 源↔派生一致性 |
| 工具 | `tools/lint-review-rules.sh` | 规则集自检（L1-L9） |
| 工具 | `tools/review-gate-check.sh` | Gate 制品检查（B4、D4 要扩展） |
| 工具 | `tools/coverage-extract/coverage-extract.py` | 覆盖率穷举（D3 要改多域） |
| 工具（新建） | `tools/doc-style-lint.sh` | 文风增量门（A2） |
| 工具（新建） | `tools/doc-code-map.sh` | 关联代码抽取（B2） |
| 工具（新建） | `tools/unsafe-audit.sh` | unsafe/SAFETY 审计（D2） |
| 工具（新建） | `tools/anchor-migrate.sh` / `tools/anchor-resolve.sh` | 符号锚点一次性迁移 + 解析校验（D5 改写；原 review-line-check 方案作废） |
| 工具（新建） | `tools/doc-snippet-extract.sh` | 文档 Rust 代码块抽取（H3） |
| 工具（基线） | `tools/unsafe-baseline.txt` | unsafe 存量基线（D2 产出，入库） |
| 工具（新建） | `tools/code-style-lint.sh` | 代码可读性增量门（G1） |
| 基线 | `tools/code-style-baseline.txt` | 代码可读性存量基线（G1 产出，入库） |
| 工具（新建） | `tools/check-dep-direction.sh` | 分层依赖方向检查 + README 对账（G3） |
| 架构文档 | `os/README.md` + `os/arch/README.md` + `os/libs/*/README.md` | 架构总览/依赖图/设计理由（G3 漂移检查对象） |
| CI | `.github/workflows/vm-tests.yml` 等 | G2 要把 clippy/doc 检查扩到全 workspace |
| 集成测试 | `os/qemu-tests/run_all.sh` / `run_qemu.sh` | QEMU 冒烟门（D1）；PASS 标记 `### TEST_RESULT: PASS <name> ###` |
| 正式文档 | `notes/rewrite/fork-syscall-rewrite/{stage}/{NN}-*.md` | 被审查对象；本轮不改存量正文 |
| 状态目录 | `.review/{tool}/{module}/` | review 产物（scan/STATE/SYMBOLS/...）；D1 的 qemu log 放 `{doc-stem}/logs/` |

## 附录 2：本次诊断的证据与统计命令（供执行者复核）

```bash
cd notes/rewrite/fork-syscall-rewrite

# 1) review 编号统计（正式编号文档，排除 *todo*/checklist/smp_* 等过程文件；本口径只数 Vx-Py / Px-y / FIX- / R- 四类）
for stage in 01-stage-kernel 02-stage-vm; do
  rg -o "V[0-9]+-P[0-9]+|P[0-9]+-[0-9]+|FIX-[0-9]+|R-[0-9]+" $stage/[0-9][0-9]-*.md | wc -l
done
# 参考结果（2026-09-16）：01-stage-kernel 107，02-stage-vm 222
# 口径说明：该正则不数正文里真实存在的 D-13、W-xx 两类编号。补上 D- 后 kernel 226、VM 222；
# 再加 W- 后 kernel 229（合计约 448-451）。做度量对比时必须固定同一口径，否则数字不可比。

# 1b) 宽口径复核（含 D- / W-）
for stage in 01-stage-kernel 02-stage-vm; do
  rg -o "V[0-9]+-P[0-9]+|P[0-9]+-[0-9]+|FIX-[0-9]+|R-[0-9]+|D-[0-9]+|W-[0-9]+" $stage/[0-9][0-9]-*.md | wc -l
done
# 参考结果（2026-09-16）：01-stage-kernel 229，02-stage-vm 222

# 2) 正文日期统计
for stage in 01-stage-kernel 02-stage-vm; do
  rg -o "20[0-9]{2}-[0-9]{2}-[0-9]{2}" $stage/[0-9][0-9]-*.md | wc -l
done
# 参考结果：01-stage-kernel 204，02-stage-vm 112（含合法的头部创建/重写日期）

# 3) 头部字段缺失/不统一抽查
rg -l "Rust (实现|模块)" 01-stage-kernel/[0-9][0-9]-*.md | wc -l   # 约 27/37
rg -n '^> \*\*Rust (实现|模块)\*\*' 02-stage-vm/15-ipc-dispatch.md 01-stage-kernel/15-clock-timer.md
```

第二轮（正确性审计）的证据与统计命令：

```bash
# 4) unsafe 规模（os/ 下 206 个文件，约 1774 处 unsafe 构造；SAFETY 注释约 848 条）
rg -o "unsafe (fn|impl|extern)|unsafe \{" os --glob '*.rs' | wc -l
rg -c "SAFETY" os --glob '*.rs' | awk -F: '{s+=$2} END {print s}'

# 5) 动态验证工具缺失（仅草稿与 test util 提及，非生产工具链）
rg -ln "miri|loom|proptest|kani" --glob '!target' --glob '!minix3' . | head

# 6) qemu-tests 在 review 规则中零引用（唯一命中是 review-patterns 的无关行）
rg -n "qemu" prompt/ .claude/ .codex/ .trae/

# 7) Gate A 域错位实例
sed -n '36,43p' .review/codex/sched/12-kernel-interface/scan.md

# 8) 独立验证默认关闭
rg -n "require-multi-agent|P0=0" prompt/review-rules/review-process.md
```

第三轮（代码可读性审计）的证据与统计命令：

```bash
# 9) 规则覆盖 vs 执行缺口
rg -n "missing_docs" os --glob '*.rs'                      # 0 命中
ls os/clippy.toml os/rustfmt.toml 2>/dev/null              # 不存在（只有 os/.cargo/config.toml）
rg -n "clippy|fmt|cargo doc" .github/workflows/*.yml       # 仅 vm-tests.yml 的 clippy
rg -n "os/README" prompt/ .claude/ .codex/                 # 0 命中（README 未被 review 流程引用）
ls tools/ | grep -i "dep\|arch" || echo "(无依赖方向工具)"

# 10) 现状素材（数量不等于质量，但没有机制防退化）
rg -l "" os --glob '*.rs' | wc -l                          # 约 1010 个 .rs
rg -l "^//!" os --glob '*.rs' | wc -l                      # 约 1000 个带模块文档
rg -c "^\s*///" os --glob '*.rs' | awk -F: '{s+=$2} END {print s}'   # 约 49705 行 /// 
rg -c "pub (fn|struct|enum|trait|const|type)" os --glob '*.rs' | awk -F: '{s+=$2} END {print s}'  # 约 9949 个 pub 项
rg -c "//.*[\x{4e00}-\x{9fff}]" os --glob '*.rs' | awk -F: '{s+=$2} END {print s}'  # 约 4267 行含中文注释
```

第四轮（锚点与卓越性）的证据与统计命令：

```bash
# 11) 锚点形态普查：行号锚点 vs 符号锚点（D5 迁移基线；口径按本篇文档调整）
rg -o "\.rs:[0-9]+" notes/rewrite/fork-syscall-rewrite/01-stage-kernel/[0-9][0-9]-*.md | wc -l
rg -o "\.rs:(fn|struct|enum|trait|const) " notes/rewrite/fork-syscall-rewrite/01-stage-kernel/[0-9][0-9]-*.md | wc -l

# 12) 文档 Rust 代码块普查（H3 基线）
rg -c '^```rust' notes/rewrite/fork-syscall-rewrite/*/[0-9][0-9]-*.md | awk -F: '{s+=$2} END {print s}'

# 13) 复述结论档案普查（H1 基线；structure.md 在 .review/{tool}/{module}/{doc-stem}/ 或 scans/ 下）
rg -l "裸概念复述" .review/codex/*/*/structure.md 2>/dev/null | wc -l
```

样本证据（正文位置）：

- `01-stage-kernel/13-syscall-dispatch.md:884`、`:403`、`:715`
- `01-stage-kernel/15-clock-timer.md:119`、`:124`、`:140`
- `01-stage-kernel/05-clock-interrupt-init.md:600`、`:615`
- `02-stage-vm/09-slab-allocator.md:132`、`:427`
- `01-stage-kernel/06-proc-init-boot-proc.md:504`、`:519`

## 附录 3：术语表（避免执行者误解）

| 术语 | 本计划中的含义 |
|---|---|
| 正式文档 | `notes/rewrite/**/{NN}-*.md` 编号文档（被审对象、面向读者） |
| 过程痕迹 | review 编号/日期/修复史/写作策略元注释/流程工具术语 |
| 关联代码 | 文档头部字段 + §3/§4 引用的 `.rs` 文件 + §5 测试文件 |
| 增量门 | 只检查新增/修改行（`lint --diff`），不检查存量 |
| 止损 | 阻止新的过程痕迹写进正文；不清理存量 |
| 编辑战役 | 未来对存量正文做逐字句重写的大批次工作（本轮不做） |
| 演练 | 用真实文档跑一次修订后的 full-review，验证机制可用 |
| 正确性相关 P1 | 影响 C↔Rust 语义等价或内存/并发安全的 P1（如叶函数语义不对齐、BKL 声明与实际不符）；有 30 天时限 |
| 契约覆盖矩阵 | §2.3 每个 C 函数必须归档（契约表 / 语义未变 / ARCH / 缺口）的表，替代"Gate B 只看 Top 5" |
| 独立复核 | zero-P0 也必须做的第二次检查：跨 agent / 跨工具只读 / 跨 session 换模型，随机抽一个章节完整重跑 |
| `gate-evidence-X` | scan.md 里的证据块；X ∈ {qemu, code, unsafe, test, dep, A, H...}，内容为命令 + 输出 + 磁盘产物路径（D4 起由脚本核对） |
| QEMU 冒烟 | 跑 `os/qemu-tests/` 的集成测试（真实固件 + 内核启动到串口 PASS 标记），补 hosted 单测覆盖不到的硬件路径 |
| 代码可读性 | 微观层面的"新读者能否读懂"：文档注释、模块文档、注释语言、命名、复杂度（对应 checklist §6/§7/§9/§11 与 excellence §17/§19） |
| 代码架构可读性 | 宏观层面的"新读者能否看懂结构"：模块职责/边界/依赖方向可读、核心路径可走通、trait 划分有理由（对应 checklist §2/§6、patterns 79-82、`os/README.md`/`//!`） |
| 路径可读性 | G3 的抽查方法：只读代码 + `//!` + README，沿一条核心主线（如 boot → kmain → scheduler → IPC）走通并记录卡点 |
| 新增代码 | G 批的判级边界：以 `--diff`/git diff 的新增或修改行为准，存量不计入 P1 |
| 符号锚点 | 用定义点（`path:fn name` / `path:struct Name` / C `path.c:func`）定位源码；行号只作工具派生提示 |
| 卫生项 | 锚点提示过期、拼写、全角/半角、格式等机械项；批量修，不进 P1/P2 计数、不进收敛判定 |
| 读者复述测试 | 不看代码、不用函数名，用机制语言复述 Ch1；说不出机制 → 教学类 P1（H1） |
| 非法态封堵清单 | 每个核心状态机/句柄/地址类型一行：状态集 → 类型表达 → 封住的非法态 → 封不住的 → 结论（H2） |
| 代码块四分类 | 文档 Rust 块分为逐字引用 / 教学简化 / 反面示例 / 签名示意，分类缺失或未标注反模式即问题（H3） |
| 卓越度量 | 复述通过数、非法态清单与落地数、代码块分类覆盖与反模式数、样章达标数；验收主证据（其余为健康检查） |

## 附录 4：Pattern 新增 housekeeping 清单（E2/F2/H3 适用）

新增任何模式（编号或字母）时，必须同步以下位置，缺一处即规则集自检不过：

| # | 位置 | 改什么 |
|---|---|---|
| 1 | `prompt/review-rules/review-patterns.md` | 模式正文 + 顶部"模式编号索引"表 + 总量宣称行（当前："84 个检查模式 = 81 个编号模式（1-60、63-83）+ A/B/C"） |
| 2 | `prompt/skill/review-patterns-skill.md` | description 与正文中的总数（当前 84） |
| 3 | `prompt/README.md` | §review-rules 条目（当前 84）+ `.claude/skills/review-scan/checks/patterns.md` 描述处的总数 |
| 4 | `AGENTS.md` | skill 表里 review-patterns-skill 的描述（当前 84） |
| 5 | `.claude/skills/review-scan/checks/patterns.md` | Claude 版按领域合并的模式库正文（如该模式属于必检族） |
| 6 | 派生（自动） | `tools/generate-derived-skills.sh` 后由脚本处理 `.trae/`/`.codex/` |

> **编号冲突提示（2026-09-18）**：F2 与 H3 都声明可能新增模式（F2 的"ARCH 断言无外部权威锚点"、H3 的"未标注反模式示例"）。当前下一个空闲编号是 84；两者按落地先后取号，后者顺延（85），并各自按本清单同步计数。

验证：

```bash
tools/lint-review-rules.sh          # L5 会校验"编号模式总数与索引表宣称一致"
rg -n "84 个|81 个" prompt/ AGENTS.md .claude/skills/review-scan/checks/ | head
# 新增后按实际数字更新；下一个空闲编号是 84（1-60 + 63-83 用满 81 个编号）
```

---

## 执行记录（由执行者填写）

### 任务状态

| ID | 状态 | 完成日期 | 证据位置 |
|---|---|---|---|
| A1 | 未开始 | | |
| A2 | 未开始 | | |
| A3 | 未开始 | | |
| A4 | 未开始 | | |
| B1 | 未开始 | | |
| B2 | 未开始 | | |
| B3 | 未开始 | | |
| B4 | 未开始 | | |
| B5 | 未开始 | | |
| C1 | 未开始 | | |
| C2 | 未开始 | | |
| C3（第一轮） | 未开始 | | |
| D1 | 未开始 | | |
| D2 | 未开始 | | |
| D3 | 未开始 | | |
| D4 | 未开始 | | |
| D5（原行号自动化） | 已作废，见 D5 改写 | — | — |
| D6 | 未开始 | | |
| E1 | 未开始 | | |
| E2 | 未开始 | | |
| E3 | 未开始 | | |
| F1 | 未开始 | | |
| F2 | 未开始 | | |
| C3（第二轮） | 未开始 | | |
| G1 | 未开始 | | |
| G2 | 未开始 | | |
| G3 | 未开始 | | |
| G4 | 未开始 | | |
| C3（G 批） | 未开始 | | |
| D5（改写） | 未开始 | | |
| H1 | 未开始 | | |
| H2 | 未开始 | | |
| H3 | 未开始 | | |
| H4 | 未开始 | | |
| I1 | 未开始 | | |
| I2 | 未开始 | | |
| C3（第四轮） | 未开始 | | |

### 度量对比（每轮 C3 填写）

基线数字（2026-09-16，取自附录 2 的审计命令；第四轮新增卓越组，机械组不再作为"完成"主证据）：

| 组 | 指标 | 基线 | 第一轮后 | 第二轮后 | 第三轮后 | 第四轮后 |
|---|---|---|---|---|---|---|
| 卓越 | 复述通过文档数（H1） | 待测 | | | | |
| 卓越 | 非法态封堵清单产出的模块数 / 落地改进数（H2） | 待测 | | | | |
| 卓越 | 文档 Rust 代码块已分类数 / 未标注反模式数（H3） | 待测 | | | | |
| 卓越 | 卓越样章达标数（H4：文档 A/B + 模块闭环） | 0 | | | | |
| 正确性 | 裸 unsafe 数（无 SAFETY） | 待 D2 首跑 | | | | |
| 正确性 | `missing_docs` 警告数 | 待 G2 首跑 | | | | |
| 正确性 | 契约/覆盖率缺口数（Gate B/C 归档） | 待 F1 统计 | | | | |
| 正确性 | 正确性相关 P1 未处置数（E3） | 待统计 | | | | |
| 机械 | 正式文档 review 编号数（窄口径：Vx-Py/Px-y/FIX-/R-，见附录 2 第 1 条） | 约 330 | | | | |
| 机械 | 正式文档 review 编号数（宽口径：再加 D-/W-） | 约 451 | | | | |
| 机械 | 正文日期数 | 约 316 | | | | |
| 机械 | 行号锚点数 / 符号锚点数（D5 迁移统计） | 待 D5 首跑 | | | | |
| 机械 | 头部字段缺失文档数 | 待 B1 统计 | | | | |
| 机械 | CLAUDE.md ⏸ Proposal 剩余数 | 4（#7/#8/#12/#13） | | | | |

### A4 存量统计

（待 A2 完成后填写）

### D5 迁移统计

（待 D5 完成后填写：旧行号锚点数 → 新符号锚点数 → 无法解析清单 → 首轮卫生项计数）

### H4 样章结论

（待 H4 完成后填写：文档 §4.1-4.5 等级 + 复述卡点 + 模块非法态清单 + 用户认可记录）

### B5 演练结论

（待演练完成后填写）

### D6 演练结论

（待 D6 完成后填写）

### G3 路径抽查结论

（待 G3 完成后填写：选中的主线、走读卡点、判级与去向）

### 遗留问题

（执行过程中发现的新问题；不要顺手修，登记即可）

### 计划收尾（按 0.7 执行）

（全部任务完成或中途停止时填写：落地 commit 与回滚点、本文件的归档或删除去向、正式文档零引用确认）
