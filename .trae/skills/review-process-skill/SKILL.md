---
name: review-process-skill
description: Minix-RS Review 执行流程。定义强制步骤 Step 0-7（含 Step 0.5 structure.md 骨架评审、Step 3.5a 纵向链路检查、Step 3.5b 因果链抽样验证）、Blocker Gates（0/A/B/C/D/D-6/E/G/H — 2026-08-15 修复 C-P0-2 明确 Gate H 属于 Blocker Gates）、每个 Step 的中间产物格式、自检清单、以及工具命令速查。当 Agent 进入 Review 执行阶段时调用此 Skill。
---

# Minix-RS Review 执行流程

> 每个 Step 必须产生**可见的中间产物**（表格、列表、grep 输出）。不允许"在脑子里过一遍"然后跳到最终输出。

## 快速导航（Quick Nav）

> **AI 长 session 注意力衰减时，优先用此表定位所需章节，避免线性扫描。**

| 需要找... | 跳转到 | 行数 |
|----------|--------|------|
| 执行模式选择（构造/快速/深度/设计优先） | [§〇 执行模式选择](#〇执行模式选择构造--快速--深度) | ~33 行 |
| Blocker Gates 状态表 | [§Blocker Gates](#⛔-blocker-gates阻断门必须通过才能输出-final-review) | 搜索 "Blocker Gates" |
| **Step 0-7 强制步骤** | [§Step 0](#step-0-范围声明--时间预算--状态恢复) 起各 Step 节 | ~1500 行 |
| Gate H design 门控 | [§Step 1.6 设计对齐检查](#step-16-设计对齐检查design-alignmentgate-h) | 搜索 "Gate H" |
| structure.md 12 节骨架模板 | [§1. 主题思想](#1-主题思想一句话) 起 12 节 | ~200 行 |
| 6 维反查矩阵 | [§Step 0.5.6 6 维反查矩阵](#step-0566-维反查矩阵新增2026-07-16) | ~50 行 |
| Resume Point（跨 session 续审） | [§Step 0 Resume Point 模板](#statemd-resume-point-模板多-session-续审强制new-2026-07-16) | 搜索 "Resume Point" |
| Review 输出格式 | [§Step 5 Final Review Output](#step-5-final-review-output最终输出) | ~190 行 |
| 收敛停止规则 | [§Step 7.1 收敛成本警告](#step-71-收敛成本警告强制) | 搜索 "Step 7.1" |
| Review 工具命令 | [§工具命令速查](#工具命令速查) | ~67 行 |
| 修复阶段工作流 | [§五 修复阶段工作流](#五修复阶段工作流fix-phase) | ~33 行 |

---

## 〇、执行模式选择（构造 / 快速 / 深度）

> 根据任务规模和精度要求选择模式，不同模式裁剪不同 Step。

| 模式 | 适用场景 | 执行 Step | 预计耗时 | 对应 Profile |
|------|---------|----------|---------|-------------|
| **构造（Constructive）** | 初稿阶段，引导补全 | 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 | A/B/G |
| **快速（Quick）** | 日常 PR、时间有限 | 0, 1, 2, 5 | 10~20 分钟 | D |
| **深度（Deep）** | 里程碑验收、关键模块 | 0-7（全量） | 40~120 分钟 | C/H→I→J→K/O/P |
| **设计优先（Design-First）**| design 缺失/错误 | Step 0 + **Step 0.3** + Step 1.6 + 2 | 30~60 分钟 | R |

**决策树**：初稿→构造；日常PR/时间紧→快速（P0>3 则升级深度）；里程碑/关键模块→深度；design 缺失/错误→设计优先（Profile R）。

**深度模式 rounds 动态化**（新增，2026-07-16）：根据文档行数决定分阶段轮次，避免小文档过度分阶段、大文档一轮过载。

| 文档行数 | rounds 数 | 每 round 范围 | 理由 |
|---------|----------|--------------|------|
| < 500 行 | 1 round（全量） | Step 0-7 一轮完成 | 小文档单轮可完成，无需分阶段 |
| 500-1500 行 | 2 rounds | R1: 正确性（Step 0-4 + Gate 0/A/B/C/D/D-6/E/**H**）<br>R2: 卓越性（Step 5 + Gate G + patterns/excellence） | 中等文档分两轮：先保正确性，再求卓越 |
| > 1500 行 | 4 rounds | R1: 正确性（Step 0-4）<br>R2: 卓越性（Step 5 + excellence）<br>R3: patterns 对照<br>R4: 跨文档 + Gate G 收敛 | 大文档需 4 轮，避免单轮 context 过载 |

**判定规则**：默认按行数查表；用户明确要求"深度全面 full-review"时按 2 rounds 起步（不强制 4 rounds），避免过度分阶段。

> **2026-08-15 修复 C-P0-4（rounds 表与决策树阈值统一）**：rounds 表阈值（<500 / 500-1500 / >1500）与决策树完全一致；500-1500 行文档分 2 rounds 但属 Profile C 单 session，> 1500 行才需 Profile H/I/J/K 多 session。**Gate H 必检**（修复 C-P0-2）已写入 rounds 表第 2 行 R1 列。

**Design-First 模式要点**：
- 触发：自动（**Step 0 design 预检**找不到 `design.md`/`design-final.md`，或 Step 1.6 一致性 < 80%）+ 手动（用户指定）
- **design 文件命名规则**：非 bagging 场景默认产物为 `{NN}-design.v{N}.md`；bagging 场景（多 AI 聚合）产物为 `{NN}-design-final.v{N}.md`（保留所有历史版本，见 Step 1.6.1）
- 输出：scan.md 含 Design Feedback §8 + IN_DESIGN.md（如中断）+ **Step 0.3 生成的 design.md**
- 配套 Gate：必须通过 Gate H（design 门控）
- 与日常 review 关系：Step 0.3 已嵌入所有 review 模式（2026-07-17 变更），Design-First 模式不再是独立前置
- 详见 [review-rules/review-process.md §Design-First 模式](../review-rules/review-process.md) + [§Step 0.3 缺失即生成](../review-rules/review-process.md)

---

## ⛔ Blocker Gates（阻断门，必须通过才能输出 Final Review）

| Gate | 检查项 | 通过标准 | 未通过后果 |
|------|--------|---------|-----------|
| **0** | 制品完整性（输出前必检，在所有 Gate 之前） | 对每个目标文档，标准路径下文件齐全（STATE/scan/structure/SYMBOLS）；scan.md 含 8 个 grep 可验锚段（见下） | 禁止输出 Final Review |
| **A** | Step 1.5 Coverage Enumeration | 已运行 coverage-extract.py（**强制，不允许"手动验证"代替**）+ scan.md 附 gate-evidence-A 块 + SYMBOLS.md 落盘 | 禁止输出 Final Review |
| **B** | Step 2 Diff Extraction | 已产出 Top 5 行为契约表（3 语义偏移 + 2 覆盖缺口，**8 字段 × 5 函数**） | 禁止输出 Final Review |
| **C** | Step 3.5 Precision Check | 已产出 5 元规则检查表 | 禁止输出 Final Review |
| **D** | P0 必检清单（见 patterns-skill §0） | 5 项已回答（✅/❌ + grep 证据）；**PARTIAL/⚠️/部分通过 均视为 FAIL** | 禁止输出 Final Review |
| **D-6** | Step 0.5 structure.md 骨架评审（文档 Review） | structure.md 已生成 + 12 节评审表 + 失败项写入 Issue List | 禁止输出 Final Review（文档 Review） |
| **E** | Step 4.5 Test Verification | 文档 §5 每个测试函数已 grep 验证（若文档有 §5） | 禁止输出 Final Review |
| **G** | Step 5.6 VERIFY-CHECK 独立验证 | VERIFY-CHECK.md 已产出 + 判定 PASS（一致性 ≥90%）；CONCERN/FAIL 不得标 CONVERGED | 禁止标记 CONVERGED |
| **H**| Step 1.6 design 门控（**所有 review 模式必检，2026-07-16 扩**） | H.1 `design.md`（非 bagging）或 `design-final.md`（bagging）存在 + H.2 一致性 ≥80% + H.3 Minix3 对齐无 P0 缺失 + H.4 无 design-wrong + H.5 code/design Refactor 区分清楚 + **H.6 outline.md 存在 + doc↔outline 无 P0 偏离**（方案 D 新增） | 缺失→Step 0.3 嵌入生成（不中断）；design-wrong→触发 Profile R |

**任一 Gate 未通过 → scan.md 标记 DRAFT，禁止写入 STATE.md。**

**⛔ Gate H 不允许 N/A 判定**：每篇文档都必须通过 Gate H 全部 6 项检查。不允许"本文档复用其他文档 design，Gate H N/A"——这是 P0-process-violation。若本文档无专属 design.md，必须**执行 Step 0.3 嵌入生成**（2026-07-17 变更：原"切换 Design-First 模式生成"改为"Step 0.3 嵌入生成"），而不是标 N/A 跳过。

> **2026-08-15 修复 E-P2-4（允许"不适用 + 理由"例外）**：以下场景允许在 Gate H 中标"不适用 + 详细理由"，**而非默认 N/A**：
> - **纯算法设计文档**（如 `notes/rewrite/.../04-algorithm-X.md`）：无 Rust 实现对应，仅讨论算法 → H.1 标"不适用（纯算法，无 design）"，H.2-H.6 同样标"不适用"
> - **历史/概念文档**（如 Minix3 C 源码导论）：纯知识介绍，无代码 → 全部 6 项可标"不适用"
> - **判定**：review 文档分类时显式标注 `doc_category: pure_algorithm | historical | conceptual` → 允许部分 Gate H 项标"不适用 + 理由"。**不适用 ≠ 跳过**——必须在 STATE.md 中记录理由（详见 IN_DESIGN 健康度审计）

**Gate 0 锚段校验**（scan.md 必须含以下 9 个 grep 可验锚段，缺任一 → Gate 0 FAIL）：
```
## Skill Invocation Log
## Blocker Gates Status
## Step 0: 预检结果（design + outline 完整性，NEW 2026-07-16）  ← NEW 必含
## Step 1: C Source Ground Truth Lookup
## Step 1.5: Coverage Enumeration
## Step 2: Diff Extraction
## Step 3.5: Precision Check
## Issue List
## Artifact Inventory
```

**⛔ Gate 0 新增 `§Step 0 预检结果` 锚段**（2026-07-16，模式 69 PSMD 配套）：
- 必含内容：4 条 `ls design/{NN}-*.md` 命令 + 输出
- 缺失 → Gate 0 FAIL + **触发模式 69 PSMD**
- 模板：
  ```markdown
  ## Step 0: 预检结果（design + outline 完整性，NEW 2026-07-16）
  | 检查项 | ls 命令 | 结果 | 判定 |
  |--------|---------|------|------|
  | outline 快照 | `ls design/06-outline.v*.md` | `06-outline.v1.md` | ✅ 存在 |
  | outline-review 快照 | `ls design/06-outline-review.v*.md` | （无）| ❌ **缺失 → Gate H.6 FAIL → Step 0.3.3 生成** |
  | design 快照 | `ls design/06-design.v*.md` | （无）| ❌ **缺失 → Gate H.1 FAIL → Step 0.3.4 生成** |
  | design-final 快照 | `ls design/06-design-final.v*.md` | （无）| ➖ 非 bagging 不需要 |
  ```

**Gate 证据规则（机器可验证，gate-evidence 块）**：
- scan.md 中每个 Gate 的通过声明必须用带标签的 `gate-evidence-{X}` 代码块附带**实际命令 + 输出片段**作为证据。仅有 "✅ 通过" 而无证据块 → 该 Gate 判 FAIL，不允许"自报 ✅"。
- Gate A：`gate-evidence-A` 块必须含 `coverage-extract.py` + `Coverage Summary` + artifact 路径，且 `ls artifact` 成功。
- Gate B：必须含 5 行差异表 + "行为契约" 字样（8 字段 × 5 函数）。
- Gate D：必须含 5 项 P0 必检的 `rg`/Read 命令 + 结果（5 条）。
- Gate D-6：附 structure.md 路径 + 评审表（12 节判定结果）。
- Gate E：附每个测试函数名的 `rg "fn {name}"` 命令 + 结果表。
- Gate G：附 VERIFY-CHECK.md 路径 + 抽样一致性百分比。

**证据强度分级（L1/L2/L3）**：
| 级别 | 含义 | Gate 要求 |
|------|------|----------|
| **L1** | 工具自动输出（最强）— coverage-extract.py / grep / verify-line-refs.py | Gate A/D/E 必须 L1 |
| **L2** | 手动 grep + 行号引用（中等）— 人工验证但附命令 | Gate B/C 必须 L1 或 L2 |
| **L3** | 语义推断（最弱）— 无命令输出，仅逻辑推理 | 视为 FAIL，除非标 `MANUAL_FALLBACK` 并说明原因 |

**Gate 证据评分**（合并脚本用）：命令+输出=2 分；仅输出=1 分；纯文字=0 分。低于 1 分的 Gate 标 ⚠️；Gate A/D/E 低于 1 分直接判 FAIL。

**Skill Invocation Log**（mandatory in scan.md）：
```markdown
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Step 3 | §6 概念准确性表 |
| 2 | review-code-skill | Step 5 | 代码质量审查 |
```
未含此章节 → scan.md 标记 DRAFT。

---

## Step -0.5: 工具辅助检查（review 前置，2026-08-15 修复 B-P1-6）

> **目的**：复用 `cargo` 生态工具的检查结果，避免 review 与 lint 结果矛盾。
> **执行步骤**（仅当 review 涉及 Rust 代码时执行）：
> 1. `cargo check` — 编译检查，确保无新 error（warning 不阻断 review）
> 2. `cargo clippy -- -W clippy::all` — lint 检查，记录 clippy 警告列表（**作为 review Step 4 的输入**，但不替代 review）
> 3. `cargo fmt --check` — 格式检查（仅当 review 关注代码风格时）
> 4. `cargo test` — 运行测试，记录失败的测试（**作为 review Step 4.2 测试覆盖度的输入**）
> **判定**：
> - `cargo check` 失败 → 阻断 review（先修编译错误）
> - `cargo clippy` 警告作为 P2 候选（review 决定是否升级）
> - **不允许**用 `cargo clippy` 替代 review 的人工判断

## Step 0: 范围声明 + 时间预算 + 状态恢复

- 按 §〇 确定执行模式（构造/快速/深度）
- 声明 Review 模式和范围
- 声明时间预算（可选；若填写，按 | <200行→10-20分 | 200-500→20-40 | 500-1000→40-80 | >1000→80-120 |）
  - **分阶段时间预算**（新增，2026-07-16）：若执行 Step 0.7 TODO 验证，时间预算分两阶段声明：
    - **TODO 验证阶段**：按 TODO 数 × 5 分钟预估（含 grep 验证 + 误报否定 + 真实修复）
    - **review 阶段**：按文档行数查表（<500 行→15-30 min | 500-1000→30-60 | 1000-1500→60-90 | >1500→90-120）
    - 实际耗时与预估对比写入 scan.md，偏差 >50% 需说明原因（避免偷懒）
- **读取状态（统一双路径，互不共享中间结果）**：
  - **Trae IDE** → 读取 `.review/trae/{module}/STATE.md`
  - **Claude Code Runtime** → 读取 `.review/claude/{module}/STATE.md`
  - 两套工具各自维护独立 STATE.md，**绝不共享任何中间结果**（STATE/scan/SYMBOLS/structure/VERIFY-CHECK）。不跨工具互验；Bagging 聚合只发生在 Trae 内（多 AI 的 scan 聚合）。
  - 若同一工具下出现两份 STATE.md 且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
  - **STATE 预检**：Step 0 启动时运行 `tools/review-state-validate.py {state_path}`，校验 STATE 引用的文件是否存在、Open 列表条目能否在 scan.md 中找到对应条目。预检失败 → 在 scan.md 标注并先修复再继续。
- **⛔ design + outline 预检（所有 review 模式强制，前移自 Step 1.6，2026-07-16 扩）**：
  - **背景**：原流程在 Step 1.6 才检查 design 存在性，AI 已完成 Step 0/0.5/1/1.5 大量工作，沉没成本心理易导致违规找替代品（如 `tmp_design_and_todo/` 下的讨论稿、`/tmp/` 下的实施稿）。前移到 Step 0 让 AI 一开始就知道是日常 review 还是 Design-First。
  - **方案 D 演进（v2：可复用快照，2026-07-16）**：outline.md / outline-review.md / design.md 不是"持久化交付物 / ground truth / 答案 key"，而是**可复用快照（Reusable Reference Snapshot, RRS）**——每次 review 启动时，AI **重新执行**附录 C 流程从 C 源码独立推导，旧快照作为**前人理解参考**输入，产出**新版本快照**（`{NN}-design.v{N+1}.md`）。理由：固化即承诺"永远正确"是错的——错误会永久传播，连正式文档都在迭代，凭什么中间产物反而是"圣旨"？每轮 review 重新评估是独立 review 原则的体现。
  - **检查命令**（v2：快照是版本化的）：
    ```bash
    # 可复用快照（每次 review 重新评估，保留历史版本，不覆盖）
    ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md        # doc 结构快照（v1, v2, ...）
    ls notes/rewrite/{module}/{stage}/.design/{NN}-outline-review.v*.md # outline 评审快照
    ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md         # 非 bagging code 设计快照
    ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md   # bagging
# 跨文档查 design 统一入口（tools/design-index-update.sh 自动生成）
cat notes/rewrite/{module}/{stage}/.design/DESIGN-INDEX.md           # 最新版本锚点（软链为可选）
    ```
  - **判定**（v2：每次 review 重新评估）：
    - **存在旧快照** → AI 读旧快照作为"前人理解"参考输入，**但必须重新执行** Step 0.3.1-0.3.4 从 C 源码独立推导，产出 `.v{N+1}.md`。旧快照的角色是"语义参考 + 对照对象 + 反面教材"，**不是 ground truth**。
    - **无旧快照** → 首次走 Step 0.3 流程，产出 `.v1.md`。
    - **⛔ 禁止"复用其他文档 design"判定**：每篇文档必须有**本编号**的快照（`{NN}-outline.v*.md` / `{NN}-outline-review.v*.md` / `{NN}-design.v*.md`，`{NN}` = 本文档编号，如 `02`）。不允许"02 文档复用 01-design.md"——快照是 per-doc 的，不是 per-module 或 per-stage 的。违反 → P0-process-violation
  - **⛔ 禁止的快照依据**（违反 = P0-process-violation）：
    - ❌ `tmp_design_and_todo/` 下任何文件（临时讨论池，非定稿）
    - ❌ `/tmp/` 下任何文件（实施稿，未走 design 流程）
    - ❌ 现有文档 §3 内嵌设计章节（**循环论证**：§3 是被审对象，禁止作为快照来源）
    - ❌ 无 `{NN}-` 前缀的 `design.md` / `outline.md` / `kboot-design.md` 等（必须有 `{NN}-` 前缀）
    - ❌ **其他编号文档的快照**（如 02 文档复用 01-design.md）——每篇文档必须独立，禁止跨文档复用
    - ❌ **把旧快照当作 ground truth**——快照是 input，不是 output；每轮 review 必须独立推导
  - **中间产物生成原则（v2 快照语义）**：
    - **脚手架产物**（structure / design-structure / SYMBOLS / VERIFY-CHECK）：每次 review 重新生成，**不寻找、不复用**已有产物。
    - **可复用快照 / 持久化可复用快照（PRRS, Persisted Reusable Reference Snapshot）**（outline / outline-review / design / design-final）：每次 review **重新执行 Step 0.3 流程**产出新版本（`.v{N+1}.md`），**保留所有历史版本**不覆盖。旧快照作为参考输入，新快照作为该轮 review 的依据。触发条件：**默认每轮 review 都重新评估**，无例外。
    - **2026-08-15 修复 C-P0-3（术语统一）**：PRRS 核心属性 = (a) 持久化（不删除）+ (b) 可复用（作为输入）+ (c) 允许更新（新版本并存）+ (d) 不是 ground truth（旧快照仅作参考）
    - **禁止复用旧快照作为 ground truth**：旧快照仅作"前人理解"参考；新快照必须基于 C 源码 + OS 理论 + Rust 代码**当前状态**独立推导。差异矩阵（v{N} vs v{N+1}）写入 scan.md，便于追踪设计演进。
  - **详见**：[review-rules/review-process.md §Step 0 design + outline 预检](../review-rules/review-process.md) + [§Step 0.3 缺失即生成](../review-rules/review-process.md) + [§一.附录 C 生成规范参考](../review-rules/review-process.md)

- **⛔ 硬阻断规则（所有 review 模式强制，NEW 2026-07-16，模式 69 + 71 配套）**：
  > **背景**：Session #12 (06-proc-init-boot-proc) 复盘发现 — 即使 Step 0 已写"design 预检强制"，AI 仍会因"已有 CONVERGED 状态"/"incremental review"等理由**错误跳过预检**。Session #11 模式 69 (PSMD) 发现 04/05 缺快照时已记录此为 P0-process-violation，但缺少硬阻断机制。
  >
  > **判定（新增）**：
  > 1. **必须跑 4 条 `ls`**：每次 Step 0 启动时，无论何种 review 模式，都**必须**执行：
  >    ```bash
  >    ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md
  >    ls notes/rewrite/{module}/{stage}/.design/{NN}-outline-review.v*.md
  >    ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md
  >    ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md  # bagging only
  >    ```
  >    输出必须写入 scan.md `§Step 0: 预检结果` 段。
  > 2. **缺失判定 + 嵌入生成（NEW 2026-07-17）**：
  >    - `outline.v*.md` 缺失 → **Gate H.6 FAIL** → **执行 Step 0.3.2 生成**（不中断 review）
  >    - `outline-review.v*.md` 缺失 → **Gate H.6 FAIL**（⚠️ 2026-07-17 从 WARN 升级，根因：原 WARN 导致 AI 总跳过 outline-review）→ **执行 Step 0.3.3 生成**（AI 自审）
  >    - `design.v*.md` 缺失 → **Gate H.1 FAIL** → **执行 Step 0.3.4 生成**（不中断 review）
  >    - **核心变更**：原"缺失 → 阻断 + 触发附录 C（中断）"改为"缺失 → Step 0.3 嵌入生成 → 继续 review"
  >    - 不允许以"已有 CONVERGED 状态"/"incremental review"/"复用 03/04 design"为由跳过 — 都是模式 69 (PSMD) 触发
  > 3. **存在旧快照时**：仍必须执行 v2 评估（重新执行 Step 0.3 产出 `.v{N+1}.md`）；旧快照仅作"前人理解"参考，**不是 ground truth**。
  > 4. **工具支持**：`tools/design-coverage-check.sh {module} --stage {stage}`（NEW）自动扫描所有 stage，输出缺失报告。Session 启动时跑此工具 → 报告写入 STATE.md `§启动预检` 段。
  > 5. **决策记录豁免**：仅一次性用户明确豁免；豁免必须登记在 STATE.md `§豁免列表` 段；**不可泛化**（模式 71 DOG）。

- **STATE.md Resume Point 模板（多 session 续审强制，NEW 2026-07-16）**：
  > **目的**：避免续 session 跳过 Step 0 预检，导致快照缺失但 scan.md 标 CONVERGED。Session #12 真实案例：续 session 时仅读取了 5 个必读文件清单，没把 `ls design/` 列为强制预检。
  >
  > **模板**（写入 STATE.md `## Resume Point` 段）：
  > ```markdown
  > ## Resume Point（续 session 必读 + 必跑）
  > ### 必读文件清单（顺序）
  > 1. `.review/trae/{module}/STATE.md`（本文件）
  > 2. `.review/trae/{module}/scans/workflow-improvement-suggestions.md`
  > 3. `tmp_design_and_todo/0108-todo-final.md`（若有 TODO 清单）
  > 4. `notes/rewrite/{module}/{stage}/{target-doc}.md`
  > 5. 上一个 CONVERGED 文档的 scan
  >
  > ### 必跑预检命令（不可跳过）
  > ```bash
  > # 1. STATE 校验
  > tools/review-state-validate.py .review/trae/{module}/STATE.md
  >
  > # 2. design 预检（模式 69 PSMD 配套）
  > ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md
  > ls notes/rewrite/{module}/{stage}/.design/{NN}-outline-review.v*.md
  > ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md
  > ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md
  >
  > # 3. design 全局扫描（可选，工具支持）
  > tools/design-coverage-check.sh {module} --stage {stage}
  >
  > # 4. TODO staleness check（若 TODO 数 > 5）
  > tools/todo-staleness-check.sh tmp_design_and_todo/0108-todo-final.md  # NEW
  > ```
  >
  > ### 豁免列表（仅一次性用户明确豁免，模式 71 DOG）
  > | Doc | 豁免项 | 豁免原因 | 用户确认日期 |
  > |-----|--------|---------|------------|
  > | 04-platform-discovery | 缺 design.md | 用户决策"04/05 不回填"（仅适用 04/05）| 2026-06-30 |
  >
  > **⛔ 不可泛化**：用户对 X 的豁免仅适用 X，**不可**推广到 Y/Z。AI 认为需要类似豁免时必须先询问用户。

- **TODO Staleness Check（NEW 2026-07-16，模式 70 CTOS 配套）**：
  > 当 review 输入包含 `tmp_design_and_todo/` 下 TODO 清单，且 TODO 数 > 5 或含"基于..."/"依赖..."等时间敏感词 → **必须先跑 staleness check**（Step 0.7.4）。详见 [review-rules/review-process.md §Step 0.7.4](../review-rules/review-process.md)。
  >
  > **工具支持**（未来实施）：`tools/todo-staleness-check.sh {todo-file}` 自动扫描所有 TODO 的前提依赖并输出 staleness 报告。

- **路径变量与统一布局**（项目根 `.review/` 下分 `trae/` 与 `claude/`）：
  - `{module}` = rewrite 模块名 = `notes/rewrite/{module}/` 的目录名（如 `fork-syscall-rewrite`）。取目标文档所在路径中 `notes/rewrite/` 下的**第一级目录名**。
  - `{stage}` = 模块下的阶段子目录（如 `03-stage-kernel`），仅作 `{module}` 内分组，不替代 `{module}`。
  - `{doc-stem}` = 目标文档去扩展名（如 `03-kmain-cstart`）。
  - `{agent}` = AI 模型标识（Trae 内：glm/kimi/ds/qwen/seed/...；Claude 内：m3/glm-flash）。
  - **统一布局**（扁平 scans/，不启用 `{stage}/` 子目录，`{doc-stem}` 已含 stage 编号前缀，足够区分）：
    ```
    .review/                                         # 脚手架产物（每次重新生成）
    ├── trae/{module}/
    │   ├── STATE.md                 # Trae 专属，与 claude 隔离
    │   ├── VERIFY-CHECK.md          # Trae 专属
    │   ├── IN_DESIGN.md             # design 中断时生成
    │   ├── session-plan.md          # 多 session 续审计划
    │   └── scans/
    │       ├── {doc-stem}-{agent}-scan.md               # 某 AI 的 scan（bagging 输入）
    │       ├── {doc-stem}-{agent}-structure.md           # review 骨架（Step 0.5 产物，12 节分析）
    │       ├── {doc-stem}-{agent}-design-structure.md    # design 前序（Step 0.3.1 产物，知识点全集）
    │       ├── {doc-stem}-{agent}-SYMBOLS.md             # 覆盖率穷举
    │       ├── MANIFEST-{doc-stem}.md                    # 该 doc 所有 agent scan 清单
    │       └── AGGREGATED-{doc-stem}.md                  # bagging 聚合后主 scan
    └── claude/{module}/
        ├── STATE.md                 # Claude 专属，与 trae 隔离
        ├── VERIFY-CHECK.md          # Claude 专属
        └── {doc-stem}/{scan,structure,design-structure,SYMBOLS}.md

notes/rewrite/{module}/{stage}/                     # 持久化交付物（永久保留）
└── design/                                          # design 子目录集中存放三类持久化契约
    ├── {NN}-outline.md                                  # doc 结构契约（Gate H.6 依据）
    ├── {NN}-outline-review.md                            # outline 批准证据
    └── {NN}-design.md / {NN}-design-final.md            # code 设计契约（Gate H.1-H.5 依据）
    ```
    > **命名区分**（重要）：`-structure.md` = review 骨架（12 节）；`-design-structure.md` = design 前序（知识点全集，脚手架）。两者内容完全不同，禁止混淆。`{NN}-outline.md` 是持久化 doc 契约，不是脚手架。
  - **双写模式**（交互式修复场景）：需交互式修复 review todo 时，除标准中间产物外，**额外**在被 review 文档同目录下写可读修复文档：Trae → `notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`；Claude → `{doc-stem}-claude-report.md`。修复文档 = 标准 scan 的可读子集 + todo 进度跟踪（按 issue ID/位置匹配的子集关系，非 checksum 一致）。
  - **`{module}` 与覆盖率脚本 `--module` 是两个不同概念**：本路径的 `{module}` 是 rewrite 模块名；覆盖率脚本的 `--module kernel` 是 Minix3 模块名。不得混用。
  - 推荐用 `tools/review-init.sh trae {doc-path}` 自动计算 `{module}`/`{doc-stem}` 并 mkdir 标准目录，输出 Derived Paths 表。

**中间产物**：
```
- **执行模式**：[构造/快速/深度]
- **Review 模式**：[文档/代码/完整/局部]
- **目标文件**：xxx.md / xxx.rs
- **Step 2.5/Step 4.5 是否适用**：[适用/不适用（原因）]
- **规模**：约 N 行 | **预计**：X~Y 分钟
- **前置状态**：STATE.md 存在 → 已完成 phase [X, Y, Z]，待完成 [A, B, C] / STATE.md 不存在 → 从零开始
```

---

## Step 0.3: 缺失即生成（NEW 2026-07-17，替代原"中断去附录 C"）

> **核心变更**：原逻辑"outline/design 缺失 → 阻断 + 触发附录 C（中断）"改为"缺失 → Step 0.3 嵌入生成 → 继续 review"。AI 不需切换模式，不需用户确认，生成是 review 流程的一部分。
>
> **修复 5 个根因**：①嵌入 review 流程（非中断）②AI 自审 outline-review（不需用户确认）③统一一套流程 ④生成后立即用于 review ⑤旧快照作参考输入

**执行子步骤**（当 Step 0 预检发现缺失时）：

| 子步骤 | 产物 | 位置 | 关键要求 |
|--------|------|------|---------|
| **0.3.1** design-structure.md | `{doc-stem}-{agent}-design-structure.md` | `.review/{tool}/{module}/scans/`（脚手架） | C 源码 → OS 理论 → Rust 对照；知识点全集 + 诊断 |
| **0.3.2** outline.md | `{NN}-outline.v{N}.md` | `design/`（持久化） | Ch1 主语 CPU/OS/机制；每小节含讲什么+知识点+教学要点；末尾附覆盖矩阵 |
| **0.3.3** outline-review.md | `{NN}-outline-review.v{N}.md` | `design/`（持久化） | **AI 自审 4 维**：教学性/本质深度/概念覆盖/组织合理性；P0=0 自动批准 |
| **0.3.4** design.md | `{NN}-design.v{N}.md` | `design/`（持久化） | Ch1 设计决策/Ch2 Minix3 对齐/Ch3 Rust 类型/Ch4 限制/附录差异矩阵 |

**outline-review.md 双重用途**：①审 outline（4 维自审）②审文档正文（review Step 2-5：文档是否包含规定内容？漏了/多了/为什么？差异矩阵写入 scan.md）

**design.md 双重用途**：①Gate H 依据（design↔code 一致性）②审 Rust 实现（设计错误？更优设计？差异矩阵写入 scan.md）

**生成后 review 使用**：Step 0.5 用 outline 检查骨架；Step 2 用 design 提取差异；Step 2.5 用 outline-review 检查文档覆盖；Step 1.6 用 design 做 Gate H.2；Step 3.5 用 outline-review 做纵向链路。

**通用禁令**：禁止跳过 structure 写 outline；禁止跳过 outline-review 写 design；禁止用 §3/tmp/bak 作来源；禁止旧快照当 ground truth；禁止迭代叙事。

> **详见**：[review-rules/review-process.md §Step 0.3](../review-rules/review-process.md)

---

## Step 0.5: 生成 structure.md 并评审骨架（文档 Review 强制）

> **核心原则**：reviewer 必须先提取文档骨架并评审，再执行正确性检查。
> 正确性检查验证"文档说了什么"，structure.md 验证"读者读到了什么"。两者正交。
> 来源：03-kmain-cstart
> **详见**：[review-rules/review-process.md §Step 0.5](../review-rules/review-process.md)、[review-rules/review.md §5 structure.md](../review-rules/review.md)。

**Step 0.5.1 生成 structure.md**（按以下模板，概念文档全量 12 节，实现文档简化为 6 节）：

```markdown
# structure.md — {文档名} 结构分析

## 1. 主题思想（一句话）
> 例：prot_init() 给 CPU 配置回答"特权级/异常入口/内核栈"三问的数据结构
> 说不出 → P1（读者无法建立概念模型）

## 2. 目标读者
> 初学者/中级/高级 + 前置知识
> 未声明 → P1（无法验证教学性是否匹配读者）

## 3. 叙事主语
> prot_init() / CPU / 读者 / OS —— 选一个，附证据（Ch1 开篇第一句主语）
> 主语是函数名 → P1（实现驱动）

## 4. 驱动方向
> 概念驱动 / 实现驱动 / 混合 —— 附判定证据
> 实现驱动 → P1（模式 51）

## 5. 文档大纲（每章核心命题）
| 章节 | 核心命题（一句话） | 回答的问题 | 叙事弧角色 |
|------|------------------|-----------|-----------|
| Ch1 | 保护结构是 CPU 三问的答案 | WHAT/WHY | 起(问题) |
| Ch2 | Minix3 C 如何回答三问 | HOW(C) | 承(证据) |
| Ch3 | Rust trait 如何抽象三问 | HOW(抽象) | 转(设计) |
| Ch4 | 具体实现如何落地 | HOW(代码) | 合(落地) |
| Ch5 | 测试如何验证 | VERIFY | 合(验证) |
> 叙事弧缺环节 → P1；章节核心命题说不出 → P1

## 6. 核心概念清单（按引入顺序）
| 顺序 | 概念 | 来源层级(L0/L1/L2) | 依赖的概念 | 依赖已引入? | 文字行数 | 占比 |
|------|------|-------------------|-----------|-----------|---------|------|
| 1 | 保护 | L1 | 无 | — | 15 | 10% |
| 2 | 特权级 | L1 | 保护 | ✅ | 20 | 13% |
| 3 | GDT | L0 | 特权级 | ✅ | 40 | 27% |
> 依赖倒置 → P1；L2 术语作概念定义 → P1；
> 历史包袱占比 > 核心概念 → P2

## 7. 跨架构统一抽象（多架构文档）
| CPU 问题 | x86 | aarch64 | riscv64 | 文档位置 |
|---------|-----|--------|--------|---------|
| 当前特权级 | ring | EL | mode | §1.2 |
| 异常入口 | IDT | VBAR | stvec | §1.4 |
| 内核栈 | TSS.sp0 | SP_EL1 | sscratch | §1.5 |
> 无统一抽象层 → P1（模式 53）

## 8. 双向闭环完整性
| 机制 | 进入 | 返回 | 完整? |
|------|------|------|-------|
| 特权级切换 | user→kernel ✅ | kernel→user ✅/❌ | |
> 只单向 → P1（模式 52）

## 9. 起承转合（叙事弧）
> 起(问题) → 承(概念) → 转(证据/抽象) → 合(落地/验证)
> 缺失环节标注 ❌ → P1

## 10. 元注释位置标记
| 位置 | 类型(六类之一) | 判定 |
|------|--------------|------|
| §1.2 L39 | 过渡类 | 删除 |
| §1.4 L106 | 深度策略类 | 删除 |
> >5 处 → P1（模式 49）

## 11. 裸概念复述测试
> 把 Ch1 正文中所有函数名/结构体名/trait 名用 [XXX] 替换后，
> 核心概念是否仍能被理解？
> 不能 → P1

## 12. 纵向链路映射
| Ch1 概念 | Ch2 C 函数 | Ch3 Rust trait | Ch4 实现 | Ch5 测试 |
|---------|-----------|---------------|---------|---------|
| 特权级 | tss_init() | ProtectionArch | X8664Protection | test_privilege |
| 异常入口 | idt_init() | TrapEntryArch | X8664TrapEntry | test_trap_entry |
> 缺映射 → P2（要求补映射表）
```

**Step 0.5.2 评审 structure.md**（逐项判定，失败项写入 scan.md §structure.md 评审）：
- 1-12 节每节判定 ✅/P0/P1/P2 + 证据
- 失败项汇总为 Issue List 的 P0/P1/P2 条目

**Step 0.5.3 doc ↔ outline 对齐检查**（方案 D 新增，仅当 outline.md 存在时执行）：

> **目的**：对照持久化的 outline.md（doc 结构契约），检查文档正文是否遵循大纲。发现三种偏离：遗漏 / 多余 / 顺序错位。详见 [review-rules/review-process.md §Step 0.5.3](../review-rules/review-process.md)。
> **前提**：Step 0 预检 outline.md 存在。缺失 → 跳过本步，Gate H.6 将 FAIL。

**输出偏离矩阵**（6 列 × N 行）：
| outline 小节 | outline 规定的知识点 | 文档实际覆盖? | 偏离类型 | 严重度 | 评估 |

**偏离类型**：遗漏（坏）/ 多余（需评估）/ 顺序错位（需评估）
**严重度**：P0（核心概念遗漏）/ P1（非核心遗漏或坏偏离多余）/ P2（好偏离多余或轻微错位）

### ⛔ 反查原则：不必然导向统一结果，可包含 Open Questions（2026-07-16）

> **核心原则**：review 的最终产出不一定非得是一个确定的结果，也可以包含待讨论项（Open Question）。适用所有反查维度（outline ↔ 文档、design ↔ code、doc ↔ code、跨快照 diff 等）。

**三类判定（替代"二值 P0/P1/P2 判定"）**：

| 判定类型 | 含义 | 严重度标注 | 处理方式 |
|---------|------|----------|---------|
| **🔴 直接判定** | AI 能直接判定哪个更好（依据 C 源码语义 / OS 理论 / 类型系统 / 架构原则） | P0/P1/P2 | AI 给出推荐 + 理由，写入 scan.md |
| **🟡 Open Question** | AI 无法直接判定哪个更好（涉及设计权衡、命名美学、风格选择） | ❓ OQ-N（编号） | 写入 scan.md `§Open Questions` 段，**上交用户决定**，不阻塞 review 流程 |
| **🟢 共识一致** | 反查双方无差异 | — | 写入 scan.md `§反查一致项` 段，无需处理 |

**Open Question 格式**：
```
| OQ ID | 反查维度 | 两侧方案 | 各自优劣 | AI 倾向（可选） |
|-------|---------|---------|---------|----------------|
| OQ-1 | design ↔ code | design 用 `trait Foo`，code 用 `struct Foo` | trait 更可测 / struct 更简单 | 倾向 trait |
```

**Open Question 收敛机制**：
- 用户回复决定后，OQ 转为 issue（P0/P1/P2）进入修复流程
- 累计 ≥3 轮未决 OQ → 在 STATE.md 标记"决策阻塞"
- 同一 OQ 跨多轮 review 仍开放 → 升级为"设计争议"

**反"无脑一致"原则**：
- ❌ 禁止"design ↔ Rust 代码不一致 → 直接判 P0 改 code"——必须先判断哪个更好
- ❌ 禁止"doc 和 outline-review.md 不一致 → 直接判 P0 改 doc"——必须先判断哪个更好
- ✅ 若 AI 能判定哪个更好 → 给出推荐 + 理由 + 严重度
- ✅ 若 AI 无法判定 → 标 OQ，上交用户
- ✅ review 报告同时含 issue 清单（确定项）和 OQ 清单（待决项），两者独立

**Step 0.5.4 通过门槛**（原 Step 0.5.3）：
- structure.md 评审通过（无 P0）+ outline 对齐通过（无 P0）后才进入 Step 1（覆盖率穷举）
- 若 structure.md 有 P0 → 在 scan.md 标注"骨架层 P0，建议先修骨架再继续"，但**不阻塞**后续 Step（用户可能希望一次性看到所有问题）
- 若 outline 对齐有 P0 → 在 scan.md 标注"outline 对齐 P0"，**不阻塞**后续 Step

#### Step 0.5.6 6 维反查矩阵（新增，2026-07-16）

> **目的**：把"反查"作为 Step 0.5 的标准化方法。**超越原 Step 0.5.3 的 outline ↔ 文档单维检查**。每次 review 必跑 6 个维度，输出统一反查矩阵。

**6 维反查维度**：

| # | 维度 | 工具 | 反查对象 | 典型偏离类型 |
|---|------|------|---------|------------|
| **1** | **outline ↔ 文档正文** | grep + Read | outline.v{N}.md vs {doc}.md | 遗漏/多余/顺序错位/强调失配 |
| **2** | **design ↔ Rust 代码** | rg + Read | design.v{N}.md vs os/**/*.rs | trait 未实现/签名偏移/ARCH 未标注 |
| **3** | **design ↔ Minix3 C 源码** | rg + Read | design.v{N}.md vs minix3/**/*.c | C 行为遗漏/Rust 行为偏移 |
| **4** | **outline ↔ design** | Read + diff | outline.v{N}.md vs design.v{N}.md | 概念命名不一致/API 未声明 |
| **5** | **文档 ↔ 代码**（横向）| rg + Read | {doc}.md vs os/**/*.rs | 注释与文档矛盾/测试未验证 |
| **6** | **元层反查** | diff + Read | v{N} vs v{N-1} 快照、跨文档 | 概念增删/跨文档契约违反/修了又出 |

**每条偏离标注三类判定**：

| 判定类型 | 标注 | 含义 |
|---------|------|------|
| 🔴 直接判定 | P0/P1/P2 | AI 能判定哪个更好 |
| 🟡 Open Question | ❓ OQ-N | AI 无法判定，上交用户 |
| 🟢 共识一致 | ✅ | 无差异 |

**反查覆盖率**：6/6 维度必须全部输出，缺任一 → Step 0.5.6 FAIL。
**通过门槛**：🔴 直接判定中 P0 = 0；🟢 共识一致项 ≥ 50%。

#### Step 0.5.7 章节意图分析（新增，2026-07-16）

> **目的**：对"多余章节"（维度 1、4 标为多）分析其教学意图——"试图向读者强调什么 / 试图解释什么"。

**意图分析格式**（对每个多余章节必填）：
```
#### 多余章节：[doc §X.Y L###]
- 试图强调什么（教学意图）
- 试图解释什么（内容意图）
- 是否与 outline 教学目标对齐（✅ 对齐 / ⚠️ 偏离 / ❌ 无关）
- 判定（✅ 合理 → 更新 outline / ⚠️ 偏离 → 改写章节 / ❌ 无关 → 删除）
```

#### Step 0.5.8 issue 反查来源标注（新增，2026-07-16）

> **目的**：scan.md 中每条 issue 必须标注反查维度来源，便于追溯。

**强制格式**：
```
| Issue ID | 严重度 | 反查维度 | 反查维度号 | 位置 | 判定类型 |
| P1-1 | P1 | outline ↔ doc | 维度 1 | doc §3.5 L276 | 🔴 直接判定 |
| OQ-1 | ❓ | design ↔ code | 维度 2 | design §3.4 vs lib.rs:71-104 | 🟡 Open Question |
```

**禁止**：无反查来源标注的 issue → 严重度降级（P0 → P1，P1 → P2）。

**Step 0.5 产物**：structure.md + 评审结果表 + outline 偏离矩阵 + 跨章节一致性矩阵（Step 0.5.5）+ **6 维反查矩阵**（Step 0.5.6）+ **章节意图分析**（Step 0.5.7）+ **issue 反查来源标注**（Step 0.5.8）

**Step 0.5.5 跨章节重复内容一致性检查**（新增，2026-07-16）：
> **目的**：同一文档内多个章节描述同一事时（如 §3.5 和 §4.3 都列三架构差异表），必须保证内容一致。
> **触发场景**：02-higher-half-kernel.md review 发现 §3.5 和 §4.3 都列三架构差异表，但寄存器名矛盾。
> **扩展（2026-07-17，模式 72 CSSCM）**：步骤数对齐——§2 C 分析列 N 步、§4 Rust 实现 M 步时，若 M ≠ N 必须在 §4 末尾添加"与 C N 步的差异说明"表（分类：架构演进/设计决策/已知缺口/C bug + 附 C 行号）。`rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" {doc}`。

**一致性矩阵格式**（必须输出）：
| 主题 | 章节 A | 章节 B | 一致? | 不一致字段 | 严重度 |
|------|-------|-------|------|----------|--------|
| 三架构寄存器名 | §3.5 L276 | §4.3 L660 | ❌ | aarch64 栈切换寄存器 | P1 |

**判定**：发现不一致 → 按 Issue List 严重度记录（P0/P1/P2）。同一文档内重复描述同一事必须保持一致，否则违反"事实唯一性"原则。

**Step 0.5 产物**：structure.md（保存到与 scan.md 同目录） + 评审结果表 + **outline 偏离矩阵**（若 outline.md 存在）+ **跨章节一致性矩阵**（Step 0.5.5 产物）

---

## Step 0.7: TODO 验证（若输入含 TODO 清单，新增，2026-07-16）

> **目的**：当 review 输入包含外部 TODO 清单时（如 `0108-todo-final.md`），TODO 验证是 review 的前置步骤，不是独立任务。TODO 验证结果直接喂入 Step 2 差异提取，避免二次 grep。

**执行步骤**：
1. 逐个验证 TODO 真实性（grep/glob/read 交叉验证）：
   ```bash
   # 对每个 TODO 描述的符号/位置执行 grep
   rg "TODO_symbol_or_pattern" notes/rewrite/{module}/{stage}/{doc}.md
   # 用 Read 验证文档/代码行号是否匹配
   ```
2. 分类处理：
   - ❌ **误报**：grep 无匹配或位置不符 → 标注"误报原因"，不进入 Step 2
   - ⚠️ **真实（代码任务）**：grep 验证存在，但属于代码修改 → 标注"代码任务"，进入 Step 6 Action Item
   - ✅ **真实（文档任务）**：grep 验证存在，属于文档修改 → 标注"文档任务"，进入 Step 2 差异提取
3. 修复真实 TODO（按 review 流程，不是单独修复）
4. 喂入 Step 2：所有真实 TODO（代码任务 + 文档任务）作为 Step 2 差异提取的输入

**中间产物**：
```markdown
### Step 0.7 产物：TODO 验证表

| TODO ID | 描述 | grep 验证 | 分类 | 喂入 Step 2? |
|---------|------|----------|------|-------------|
| T1 | 修复 §3.5 寄存器名 | ✅ 有匹配 | ✅ 真实（文档任务） | 是 |
| T2 | 实现某 trait | ✅ 有匹配 | ⚠️ 真实（代码任务） | 否（进入 Step 6） |
| T3 | 删除某过时 TODO 标记 | ❌ 无匹配 | ❌ 误报 | 否 |
```

**与 Step 2 的关系**：TODO 验证完成的真实文档任务直接进入 Step 2 Top 5 差异提取，避免重复 grep 同一符号。

---

## Step 1: Ground Truth Lookup（源码定位）

- 识别文档中所有 Minix3 源文件
- 用 `rg` 验证文件是否存在于 `minix3/` 目录
- 记录每个引用的文件路径和行号范围

> 效率：文档>500行，先用 grep 提取 `.c`/`.h` 引用，抽样验证行号；Step 3 再精确验证。

**中间产物**：
```markdown
### Step 1 产物：源码文件清单

| 文件路径 | 文档引用位置 | 文件存在? | 引用行号范围 |
|---------|------------|----------|------------|
| minix3/minix/servers/vm/pb.c | Ch2§2.3 | ✅ | 33-168 |
```

### Step 1.0a 行号主动抽样比对（NEW 2026-07-30）

> **背景**：原 Step 1 仅被动比对"doc 声明的行号 vs grep 找到的位置"，但行号偏移（如 head.S:47-66 vs 实际 43-66）经常被遗漏。

**执行**：
1. 从 doc 中提取 5-10 个 `file:line` 引用（覆盖 `head.S` + `pre_init.c` + `pg_utils.c` 等关键文件）
2. 用 `sed -n 'Np' {file}` 抽取实际行内容，确认 doc 引用的行确实包含所描述内容
3. 验证 doc 行号范围是否覆盖完整（如 doc 写 `head.S:36-82`，但 kmain call 实际在 L87）

**判定**：
- doc 行号范围过短（遗漏关键内容）→ **P2 行号偏移**
- doc 行号范围过长（超出实际内容）→ **P2 行号偏移**
- doc 行号完全错位（指向其他符号）→ **P1 行号偏移**

**关联**：本次 review (01-boot-shim-bootstrap 2026-07-30) 发现 4 处行号偏移（head.S:47-66 / 36-82 / pg_info:295 / print_memmap:17）。

### Step 1.0a-自动 自动化行号校验脚本（NEW 2026-07-31, Proposal #7）

> **背景**：3 次 review 累计发现 19 处 P2 行号偏移（Doc 01: 5 + Doc 02: 4 + Doc 03: 10）。手动抽样只能发现**被抽到的**行号，需要自动化。

**工具位置**：`tools/review-line-check.sh`（建议落地）或临时 `extract_doc_lines` 函数。

**实现**：
```bash
# 1. 抽取 doc 中所有 file:line 引用
extract_doc_lines() {
    rg -o "(?:os/|minix3/)?[a-z_/0-9]+\.(?:rs|c|h|ld|sh|asm|S):\d+(?:-?\d+)?" "$1" \
       | sort -u > /tmp/doc_lines.txt
}

# 2. 对每个 file:line 跑 sed -n 验证
verify_line() {
    local file=$1 line=$2
    local actual=$(sed -n "${line}p" "$file" 2>/dev/null)
    echo "${file}:${line}: ${actual}"
}

# 3. 用法（在 review session 中）
extract_doc_lines notes/.../{doc}.md
# 然后扫描输出，对每个 file:line 跑 verify_line
```

**判定**：
- 自动脚本输出 **mismatch 表格**：doc 声称的行 vs `sed -n` 实际内容
- 偏差类型：
  - doc 写 `:267 (lgdt)` 但 L267 不是 lgdt → **P2 行号偏移**
  - doc 写 `protect.c:217-221` 但实际在 `arch_proto.h:217-221` → **P2 文件错位**
  - doc 写 `:574` 但代码已迁移到 `:607` → **P2 代码漂移**

**关联**：
- 首次发现：03-kmain-cstart review (2026-07-31)，10 P2 集中在一张表内
- 落地状态：⏸ 待用户确认后开发 `tools/review-line-check.sh`

#### Step 1.0a-自动 增强：反向偏移自动重算（NEW 2026-07-31, Doc 06 review）

> **背景**：6 次 review 累计发现 19+6=25 处 P2 行号偏移，其中**反向偏移**（doc 写靠前、实际靠后，如 `proc.rs:754` 实际 `767`）占多数。原因：doc 写作时文件较小，**代码增量后 doc 未同步更新行号**。**Step 1.0a-自动 + 自动重算**可消除此漂移。

**增强实现**（在 `extract_doc_lines` + `verify_line` 基础上）：
```bash
# 1. 检测反向偏移（doc 行号 < 实际行号）
auto_resync_line() {
    local file=$1 claimed=$2
    local actual=$(rg -n "^$(rg "$claimed" "$file" | head -1 | rg -o "\w+")" "$file" | head -1 | rg -o "^[0-9]+")
    if [ -n "$actual" ] && [ "$actual" != "$claimed" ]; then
        local delta=$((actual - claimed))
        echo "${file}:${claimed} → ${file}:${actual} (Δ=${delta})"
    fi
}

# 2. 批量 sed 重算（按上下文判断符号）
#   例：proc.rs:754 → 767 (KProcess struct)
sed -i 's|proc\.rs:754|proc.rs:767|g' {doc}.md
sed -i 's|proc\.rs:1376|proc.rs:1385|g' {doc}.md
sed -i 's|lib\.rs:699|lib.rs:706|g' {doc}.md
sed -i 's|lib\.rs:1155|lib.rs:1162|g' {doc}.md
```

**已知反向偏移案例**（Doc 06 review）：
- `proc.rs:754` → `proc.rs:767`（KProcess struct，Δ=-13）
- `proc.rs:1376` → `proc.rs:1385`（fork_from，Δ=-9）
- `lib.rs:699` → `lib.rs:706`（init_proc_and_boot，Δ=-7，×2 处）
- `lib.rs:1155` → `lib.rs:1162`（bsp_finish_booting，Δ=-7，×2 处）

**关联**：
- 首次发现：06-proc-init-boot-proc review 2026-07-31（6 处反向偏移）
- 落地状态：⏸ 待用户确认后整合进 `tools/review-line-check.sh`

### Step 1.0b Rust 代码示例同步扫描（NEW 2026-07-30, 模式 #73）

### Step 1.0b Rust 代码示例同步扫描（NEW 2026-07-30, 模式 #73）

> **背景**：文档代码示例可能与实际 Rust 代码 idioms 不一致（特别是 Rust 2024 edition 迁移：`static mut` → `Atomic*` / `UnsafeCell`）。

**执行**：
1. 从 doc §3 / §4 抽取 ` ```rust ... ``` ` 代码块
2. grep 实际代码：`rg "static mut" os/ -t rust`（应 0 hits 表示已迁移）
3. 对比：doc 示例含 `static mut` 而实际代码无 → P1（模式 #73）
4. 路径一致性：`rg "arch/src/(pt_alloc|paging\.rs|paging_ext)" {doc}` + `find os/arch/src -name X.rs`

**判定**：
- doc 示例 + 实际代码 + 路径三方不一致 → **P1 模式 #73**
- 仅 doc 示例过时（实际代码正确）→ **P1 doc-code 漂移**
- 路径重组后 doc 未更新 → **P1 子类型 b**

**修复**：复制实际代码到 doc 代码块，加注释说明 Rust 2024 edition 兼容性选择。

### Step 1.0c Doc Path Convention 一致性检查（NEW 2026-07-30, 模式 #74）

> **背景**：文档路径引用与实际仓库路径可能不一致（doc 写作时漏写 workspace 根前缀）。本次 doc 02 review 发现 18 处路径漏 `os/` 前缀（`kernel/src/...` 应为 `os/kernel/src/...`），与 doc 01 跨文档不一致。

**执行**：
1. **裸路径扫描**：`rg "kernel/src/|boot-shim/src/|arch/src/|servers/vm/|servers/pm/" {doc}.md`
   - 命中非 `minix3/...` 前缀位置 → **P1**（doc 路径漂移）
2. **正确路径统计**：`rg "os/(kernel|boot-shim|arch|servers|libs)" {doc}.md | wc -l`
   - 应与 doc 中所有 Rust 路径引用总数接近
3. **双重前缀检查**：`rg "os/os/" {doc}.md` 必须 0 hits（避免 sed 批量替换副作用）
4. **跨文档一致性**：比对同一 stage 早期 doc 的路径风格（`os/` 前缀使用率）

**判定**：
- doc 漏 `os/` 前缀 → **P1**（模式 #74 默认）
- doc 仅个别遗漏（<5 处）→ **P2**（可接受范围）
- `os/os/` 双重前缀 → **P1**（sed 副作用，必须修）

**修复**（≤10 分钟）：
```bash
# 1. 批量加 os/ 前缀
sed -i 's|kernel/src/|os/kernel/src/|g' {doc}.md

# 2. 修双重前缀
sed -i 's|os/os/|os/|g' {doc}.md

# 3. 验证 minix3 C 源路径未受影响
rg "minix3/.*kernel/src/" {doc}.md  # 应保留
```

**关联**：
- 详细规则见 `prompt/skill/review-patterns-skill.md §模式 74`
- 首次发现：02-higher-half-kernel review 2026-07-30（18 处路径缺 `os/`）

### Step 1.0d "参见" 范围引用扫描（NEW 2026-07-31, 模式 #75）

> **背景**：Step 1.0a 行号主动抽样**只检查**"`// path:line`"形式的**单行代码注释引用**，**漏检**"`参见 path:line-line`"形式的**范围引用**（典型 doc 元注释：参见 X.rs:55-399 含 XXX）。本次 04 doc review 即因此漏检 2 处范围漂移（L831 device_tree.rs:55-399 → 实际 :56-423；L883 acpi.rs:110-248 → 实际 :111-483）。

**执行**：
1. **抽取"参见"型引用**：
   ```bash
   rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" notes/.../{doc}.md | sort -u
   ```
2. **对每个范围验证起止行号**（用 `sed -n` 检查首行内容是否正确）：
   ```bash
   for ref in $(rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" {doc}.md); do
       # 解析 path:start-end
       path=$(echo "$ref" | rg -o "[^\`]+\.rs")
       start=$(echo "$ref" | rg -o ":[0-9]+-" | rg -o "[0-9]+")
       end=$(echo "$ref" | rg -o "-[0-9]+" | rg -o "[0-9]+")
       echo "=== $path:$start-$end ==="
       sed -n "${start}p" "$path"  # 验证首行内容
   done
   ```
3. **对上界验证**：用 `rg "^impl PlatformDesc for X|^impl fmt::Display"` 找 `impl` 结束位置，对比 doc 声称的上界
4. **修复**：起止 +1 偏移 + 上界漂移一并 sed 修复

**判定**：
- 起止行号 ±1 偏移 → **P2 行号偏移**
- 上界 < 实际 impl 结束 → **P2 范围过短**（doc 写作时文件较小，未随代码演化更新）
- 起止偏移 > 1 → **P1 行号漂移**

**已知漏检场景**：
- doc 中 "参见 X.rs:Y-Z" 形式的元注释引用（Step 1.0a 单点抽样漏检）
- doc 写作时文件较小，演化后上界过短（如 :55-399 实际文件 523 行）
- "参见"引用位置在 doc 主体（§4.x）或附录（§10+）

**修复**（≤5 分钟）：
```bash
# 1. 修正 +1 偏移
sed -i 's|device_tree.rs:55-|device_tree.rs:56-|g' {doc}.md
sed -i 's|acpi.rs:110-|acpi.rs:111-|g' {doc}.md

# 2. 更新上界到 impl 结束
rg -n "^impl PlatformDesc for DeviceTreeDesc" os/libs/minix-platform/src/device_tree.rs
# → 找到 impl 起始行 + 下一个 impl/fn test_/fn parse 的位置 = 新上界
sed -i 's|device_tree.rs:55-399|device_tree.rs:56-423|g' {doc}.md

# 3. 验证
rg "参见" {doc}.md  # 列所有"参见"引用，逐个确认
```

**关联**：
- 详细规则见 `prompt/skill/review-patterns-skill.md §模式 75`
- 首次发现：04-platform-discovery review 2026-07-31（2 处范围漂移漏检）

### Step 1.0e 代码注释 doc 归属交叉检查（NEW 2026-07-31, 模式 #76）

> **背景**：代码注释中"covered in NN" / "see XX-doc.md §Y" 等**指向特定 doc 编号或文件名的引用**，容易因 (a) doc 编号重排 或 (b) doc 改名 而**系统性过时**。本次 05 doc review 发现 `os/kernel/src/lib.rs` 3 处 `(covered in NN)` 注释错位（实为 04/05/06 而非 05/06/07），同时发现**至少 9 处代码注释引用旧 doc 命名**（如 `04-clock-interrupt-init.md`、`05-exception-interrupt.md`、`06-arch-post-init.md`、`06-design-final.md`、`02-page-table-kernel.md` 等已过时）。
>
> 之前 4 次 review 都**未深入代码注释交叉引用**。本次 05 review 是首次系统检查，发现 Pattern #76 实质化（不只是 1-2 处，而是 9+ 处）。

**执行**：
1. **扫描所有代码注释中的 doc 归属引用**：
   ```bash
   # 类型 1: (covered in NN) 注释
   rg "covered in 0[0-9]" os/ -t rust -n
   
   # 类型 2: see XX-doc.md 注释
   rg "see 0[0-9]-.+\.md" os/ -t rust -n
   
   # 类型 3: design doc 引用 (见 §X.Y)
   rg "见 §[0-9]+|详见 §[0-9]+" notes/.../.design/ -n
   ```
2. **验证当前 doc 编号是否一致**：
   ```bash
   ls notes/rewrite/{module}/{stage}/ | rg "^[0-9]+"
   ```
3. **对每个引用，验证目标 doc 是否存在 + 内容匹配**：
   ```bash
   for ref in $(rg "see [0-9]+-.+\.md" os/ -t rust -o); do
       doc_file=$(echo "$ref" | rg -o "[0-9]+-.+\.md")
       if [ ! -f "notes/.../$doc_file" ]; then
           echo "❌ STALE: $ref"
       fi
   done
   ```
4. **修复**：根据上下文判断目标 doc → 批量 sed 修复

**判定**：
- `(covered in NN)` 注释错位 → **P1 注释错位**
- `see XX-doc.md` 引用已删除 doc → **P1 注释失效**
- `see XX-doc.md` 引用已重命名 doc → **P1 注释失效**
- design doc 中 `§X.Y` 引用错位 → **P1 doc 内部漂移**

**已知过时 doc 命名映射**（本次 05 review 发现）：
| 旧命名 | 新命名（推测） |
|--------|---------------|
| `04-clock-interrupt-init.md` | `05-clock-interrupt-init.md` |
| `05-exception-interrupt.md` | `14-exception-interrupt.md`（推测）|
| `06-arch-post-init.md` | `08-system-init-boot-finish.md`（推测）|
| `06-design-final.md` | `06-design.md`（bagging 重命名推测）|
| `02-page-table-kernel.md` | `02-higher-half-kernel.md` |

**修复**（批量 sed，需先确认 doc 重命名映射）：
```bash
# 1. 修 (covered in NN) 注释
sed -i 's|(covered in 04)|(covered in 05)|g' os/kernel/src/lib.rs
sed -i 's|(covered in 05)|(covered in 06)|g' os/kernel/src/lib.rs
sed -i 's|(covered in 06)|(covered in 07)|g' os/kernel/src/lib.rs

# 2. 修 see XX-doc.md 注释（确认重命名映射后批量替换）
sed -i 's|04-clock-interrupt-init.md|05-clock-interrupt-init.md|g' os/arch/src/arch/{clock.rs,arch_init.rs}
sed -i 's|05-exception-interrupt.md|14-exception-interrupt.md|g' os/arch/src/arch/*.rs os/plat/src/interrupt.rs os/kernel/src/irq_manager.rs
sed -i 's|06-arch-post-init.md|08-system-init-boot-finish.md|g' os/arch/src/arch/post_init.rs

# 3. 验证
rg "covered in 0[0-9]" os/ -t rust | wc -l  # 应为 0
rg "see [0-9]+-.+\.md" os/ -t rust | wc -l  # 应为 0（除非所有引用都正确）
```

**关联**：
- 详细规则见 `prompt/skill/review-patterns-skill.md §模式 76`
- 首次发现：05-clock-interrupt-init review 2026-07-31（3 处 `(covered in NN)` + 9+ 处 `see XX-doc.md`）
- 本次 review 范围内仅修复 3 处 `(covered in NN)`；其余 9+ 处 `see XX-doc.md` 因超出本次 review scope，记录到 backlog 待后续修复

### Step 1.0f 代码注释行号漂移检查（NEW 2026-07-31, 模式 #77）

> **背景**：代码注释中引用的 `file:line`（如 `// see proc_table.rs:129`）可能因代码增量而**漂移**（文件行号下移）。doc 复述这些注释时，会产生**传递性 drift**（doc 错误根因在代码注释）。
>
> 本次 08 doc review 发现 3 处 P2 行号偏移全部源自 `os/kernel/src/lib.rs:1170/1181/1193` 的代码注释错误（不是 doc 错）：
> - `proc_table.rs:129` 实际 L276（rts_unset，+147 偏移，最大）
> - `smp.rs:127-132` 实际 L200（set_running，+73）
> - `smp.rs:80-145` 实际 L135（CpuLocal struct，+55）

**执行**：
1. **扫描所有代码注释中的 file:line 引用**：
   ```bash
   # 类型 1: see X.rs:N 注释
   rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -n
   
   # 类型 2: see X.rs:N-M 范围注释
   rg "see [a-z_/0-9]+\.rs:[0-9]+-[0-9]+" os/ -t rust -n
   
   # 类型 3: doc X §X.Y 交叉引用（已在 Step 1.0e 覆盖）
   rg "see [0-9]+-.+\.md" os/ -t rust -n
   ```
2. **对每个引用验证实际行号**：
   ```bash
   for ref in $(rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -o); do
       file=$(echo "$ref" | rg -o "[a-z_/0-9]+\.rs")
       line=$(echo "$ref" | rg -o "[0-9]+")
       actual=$(sed -n "${line}p" "$file" 2>/dev/null)
       echo "$ref: $actual"
   done
   ```
3. **修复策略**（双修避免传递性 drift）：
   ```bash
   # 1. 修代码注释（root cause）
   sed -i 's|(see proc_table.rs:129)|(see proc_table.rs:276)|' os/kernel/src/lib.rs
   
   # 2. 同步修所有复述的 doc（如果 doc 复述了错误注释）
   sed -i 's|proc_table.rs:129|proc_table.rs:276|g' notes/.../{doc}.md
   
   # 3. 验证全项目干净
   rg "proc_table\.rs:129|smp\.rs:127-132|smp\.rs:80-145" os/ notes/
   # (empty = ✅)
   ```

**判定**：
- 引用行号 ±1 偏移 → ✅（允许小漂移）
- 引用行号偏差 2-5 → **P2 代码注释轻微漂移**
- 引用行号偏差 > 5 → **P2 代码注释漂移**
- 引用行号偏差 > 50 → **P1 代码注释显著漂移**

**与已有 Step 区分**：
- **Step 1.0a** = doc 中 `file:line` 引用（doc-code 单向）
- **Step 1.0e** = doc 中"see XX-doc.md" / "covered in NN" 引用（doc-doc 单向）
- **Step 1.0f** = **代码注释中 `see X.rs:N` 引用**（code-code 单向）—— **重点是代码注释行号漂移**

**关联**：
- 详细规则见 `prompt/skill/review-patterns-skill.md §模式 77`
- 首次发现：08-system-init-boot-finish review 2026-07-31（3 处 `smp.rs:127-132`/`proc_table.rs:129`/`smp.rs:80-145`）

### Step 1.0g forward reference 验证（NEW 2026-07-31, Doc 10 review）

> **背景**：doc §4 可能引用未来文件（如 doc 10 §4.1 `os/arch/src/arch/trap_return.rs`），但文件尚未创建。本次 10 review 是 10 次 review 中**首次发现** doc 引用未存在的文件但显式标注 forward reference。
>
> 关键判断：若 doc **显式标注** "待落地" / "forward reference" / "未来"，则视为**合规**；若 doc **未标注**且引用未存在文件，则视为 P1（虚构位置）。

**执行**：
```bash
# 1. 扫描 doc 中所有引用文件路径
rg "os/[a-z_/0-9]+\.rs" notes/.../{doc}.md | rg -o "os/[a-z_/0-9]+\.rs" | sort -u

# 2. 验证每个文件路径存在
for path in $(rg -o "os/[a-z_/0-9]+\.rs" notes/.../{doc}.md | sort -u); do
    [ -f "$path" ] && echo "✅ $path" || echo "❌ $path NOT FOUND"
done

# 3. 对未找到的文件，检查 doc 是否标注 forward reference
rg -B 3 "trap_return\.rs|forward|待落地" notes/.../{doc}.md | head -10
```

**判定**：
- 文件存在 → ✅
- 文件不存在但 doc 显式标注 "待落地" / "forward reference" / "未来" → ✅ **合规**（forward reference）
- 文件不存在但 doc 未标注 → **P1 虚构文件位置**

**已知案例**（Doc 10 review 发现）：
- doc 10 §4.1 引用 `os/arch/src/arch/trap_return.rs`（文件不存在）
- doc 显式标注 "trait 定义 + asm impl 待本文 return-path 落地时加入 os/arch/src/arch/trap_return.rs"
- **判定**：✅ 合规（forward reference 透明声明）

**关联**：
- 首次发现：10-switch-to-user review 2026-07-31（§4.1 trap_return.rs forward reference）
- 已落地：CLAUDE.md Hidden Folder Convention 已生效（10 次 review 首个完全合规 doc）

---

## Step 1.5: Coverage Enumeration（覆盖率穷举，机器+AI）— **Gate A**

> **目的**：机器生成穷举清单，AI 只负责语义判断。解决覆盖率不足和跨轮次累积问题。
> **详见**：[review-coverage-skill](review-coverage-skill.md)

**执行步骤**：

1. **机器生成 SYMBOLS.md 骨架**（Trae IDE 输出路径硬编码为 `.review/trae/` — 不要使用 `{tool}` 变量）：
   ```bash
   # 变量说明：{minix3-module}=Minix3 模块名(vm/pm/kernel...); {rw-module}=rewrite 模块名(notes/rewrite/ 下第一级目录); {doc-stem}=目标文档去扩展名; {agent}=模型标识
   # 服务器模块（vm / pm / vfs / rs / ds / inet ...）
   python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
     --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
     --doc-file {target-doc-name}.md \
     --output .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

   # 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --semantic-map tools/coverage-extract/kernel-semantic-map.json \
     --doc-file {target-doc-name}.md \
     --output .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
   ```
   > **目录创建**：脚本已修复为使用 `--output` 时自动创建父目录；若使用旧版本脚本，请先 `mkdir -p $(dirname .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md)`。
   - `--rust-dir os`：扫描整个 `os/` 目录，避免遗漏跨 crate 实现（如 `kmain` 在 `os/kernel/src`，`ProtectionArch` 在 `os/arch/src`）。
   - `--c-dir`：服务器模块用 `minix3/minix/servers/{minix3-module}`，内核用 `minix3/minix/kernel`。
   - `--semantic-map`：对 C→Rust 改写项目必须提供语义映射表，否则 Rust 覆盖率会严重低估。
   - `--doc-file`：当 review 单篇文档时，必须限定到该文档，确保 coverage 数字与文档一一对应。
   - 若 `{minix3-module}-semantic-map.json` 不存在，先用空文件或从 `kernel-semantic-map.json` 裁剪。
   - **2026-08-15 修复 E-P0-1（Gate A 补救路径）**：脚本不可用时禁止直接标 PARTIAL 通过，必须按以下补救路径：
     - **路径 A**：定位 root cause（`which python3`、`ls tools/coverage-extract/coverage-extract.py`）→ 修复工具链 → 重跑
     - **路径 B（永久不可用）**：在 STATE.md `§Gate A PARTIAL 处置` 记录 (a) 不可用原因 (b) 人工覆盖范围 (c) 覆盖率差异 (d) 用户确认 → scan.md 显式标 "Gate A: PARTIAL (人工覆盖)"
     - **路径 C**：放弃 Gate A → 整个 review 暂停（不允许跳过 Gate A 后 CONVERGED）

2. **AI 补充语义判断**（5 项，每项标注 evidence [DIRECT/MEDIUM/INFERRED]）：
   - Rust 对应关系确认（名称匹配 ≠ 语义对应）
   - 架构演进标记（ARCH: 不需要 + 理由）
   - 语义归属判定（以功能语义为准）
   - 行为契约表（核心函数：输入/输出/副作用/错误码/时序）
   - 测试覆盖补充（L1对偶/L2契约/L3 doctest）

3. **更新 STATE.md Coverage Status 段**

**中间产物**：
```markdown
### Step 1.5 产物：覆盖率穷举（Gate A）

**SYMBOLS.md**: .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md（文档级）或 .review/trae/{rw-module}/scans/SYMBOLS.md（模块级）

| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M (M%) |
| Rust 覆盖 | K (K%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口**:
| 符号 | C 源码 | 判定 | evidence |
|------|--------|------|----------|
| `func_name` | file.c:N | P0 缺口 | DIRECT: rg 无结果 |

**ARCH 标记**:
| 符号 | 理由 | evidence |
|------|------|----------|
| `map_service` | IPC 协议演进 | INFERRED |
```

> **反幻觉**：每个覆盖判定必须先执行 grep 验证，再下结论。

---

## Step 1.6: 设计对齐检查（Design Alignment）— **Gate H**

> **前提**：所有 review 模式都执行此步骤；快速/构造模式只可裁剪内容检查，不能跳过 Gate H。
> **注**：design 存在性预检已在 Step 0 完成，此处为正式一致性检查。
> **目的**：验证当前 Rust 实现的 trait/类型/架构是否与 design.md（非 bagging）/ design-final.md（bagging）一致。

**Step 1.6.1**：design 存在性确认（Step 0 预检的复核）
```bash
# 持久化可复用快照（位置：notes/rewrite/{module}/{stage}/.design/，保留所有历史版本）
ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md       # 非 bagging（任意版本命中）
ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md # bagging
```
> **命名规则**：非 bagging 场景产物为 `{NN}-design.v{N}.md`；bagging 场景产物为 `{NN}-design-final.v{N}.md`。两者均需 `{NN}-` 前缀。位置在 `notes/rewrite/{module}/{stage}/.design/`。

**Step 1.6.2**：design ↔ code 一致性矩阵

| design 决策 | design 中的实现路径 | code 中的实际路径 | 一致性 |
|------------|-------------------|-----------------|--------|
| PlatformDescriptorPtr 用 trait object | `&'static dyn PlatformDesc` | `pub enum PlatformDescriptorPtr` | ❌ 偏离 |
| DirectMapArch 替代 MemoryInitArch | 仅保留 DirectMapArch | 两者并存 | ⚠️ 部分偏离 |

**Step 1.6.3**：design ↔ Minix3 对齐检查

| Minix3 概念 | design 中是否有对应 | code 中是否有对应 | 缺失位置 |
|------------|-------------------|-----------------|---------|
| 进程表 | ✅ §3.2 | ✅ | - |
| 特权表 | ✅ §3.4 | ✅ | - |
| 三类运行态实体区分 | ❌ | ❌ | design + code 都缺 |

**Step 1.6.4**：design 状态判定

| 状态 | 触发条件 | 后续动作 |
|------|---------|---------|
| **PASS** | 一致性 ≥ 80%，无 design-wrong | 进入 Step 2 |
| **DESIGN_MISSING** | design 缺失 | 中断 review，触发 design Refactor + Design-First 模式 |
| **DESIGN_WRONG** | design 错（如漏核心概念） | 阻断 review，design Refactor 必须 |
| **DESIGN_DIVERGED** | design ↔ code 严重偏离（≥30%）且 design 正确 | code Refactor（修 code） |
| **DESIGN_DIVERGED_WRONG** | design ↔ code 严重偏离（≥30%）且 design 错 | design Refactor 必须 + code Refactor |

> **Gate H 最小证据要求**（6 项，方案 D 新增 H.6）：
> - **H.1**: `ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md`（非 bagging）或 `ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md`（bagging）命中（本目录版本通配 `.v*.md`，必须含 `{NN}-` 前缀）
> - **H.2**: 一致性矩阵 ≥ 5 行 + 一致性 ≥ 80%
> - **H.3**: Minix3 对齐矩阵 ≥ 3 行 + 无 P0 缺失
> - **H.4**: `rg "design-wrong" scan.md` 无命中
> - **H.5**: `rg "code Refactor|design Refactor" scan.md` 明确区分两类
> - **H.6**（方案 D 新增）：`ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md` 命中 + Step 0.5.3 偏离矩阵无 P0 偏离

详见 [review-rules/review-process.md §Step 1.6](../review-rules/review-process.md)。

---

## Step 2: Diff Extraction（差异提取）— **Gate B**

- 列出 **5 个**最背离 Minix3 原始语义的地方：
  - **Top 3 语义偏移**：文档/代码描述与 C 行为不符（行为契约表）
  - **Top 2 覆盖缺口**：来自 Step 1.5 SYMBOLS.md 的"文档覆盖=✅ 但 Rust 覆盖=❌"符号
- 说明：Minix3 实际行为、文档/代码中的描述、差异性质
- 每项填写 8 字段行为契约表（见 [core-semantics-skill](review-core-semantics-skill.md)）

**中间产物**：
```markdown
### Step 2 产物：Top 5 差异（Gate B）

#### Top 3 语义偏移
| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |
|---|--------|------------|-------------|---------|
| 1 | xxx | pagetable.c:333 实际 | 文档 L245 描述 | 概念错误 |

#### Top 2 覆盖缺口（来自 SYMBOLS.md）
| # | 符号 | C 源码 | 文档覆盖 | Rust 覆盖 | 缺口性质 |
|---|------|--------|---------|----------|---------|
| 4 | func_x | file.c:N | ✅ | ❌ | 实现缺失 |

#### 行为契约表（8 字段 × 5 函数）
[见 core-semantics-skill §2.2 模板]
```

**差异类型区分**（新增，2026-07-16）：Gate B 的 8 字段表区分两种差异类型：

| 差异类型 | 适用场景 | 8 字段表适配 |
|---------|---------|------------|
| **C→Rust 行为差异** | C 函数行为 vs Rust 实现行为 | 8 字段全适用（C 行为 / Rust 行为 / 差异类型 / 严重度 / C 证据 / Rust 证据 / Reviewer 备注 等） |
| **doc↔code 描述差异** | 文档声称 vs 代码实际 | "C 行为"字段改为"doc 声称"，C 证据字段为 N/A（仅保留 doc 引用 + Rust 证据） |

**示例**：
- C→Rust 行为差异：`anon_pagefault` refcount 处理 — C 在 refcount<2 时 return OK 泄漏内存（minix3/minix/servers/vm/anon.c:841），Rust 实现先判断再分配
- doc↔code 描述差异：02 §3.5 文档声称 aarch64 用 `mov sp, x0`，Rust 代码实际用 `mov sp, {stktop}`

---

## Step 2.5: Link Validation（链路验证）

> 局部 Review（仅 Ch1&2）时跳过。

1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计和实现细节？
4. 代码→Ch4：代码是否与文档一致？

**中间产物**：
```markdown
### Step 2.5 产物：链路验证

**Ch3→Ch1&2**：
| Ch3 设计决策 | Ch3 位置 | Ch1&2 依据 | 链路状态 |

**Ch4→Ch3**：
| Ch4 实现 | Ch4 位置 | Ch3 设计依据 | 链路状态 |

**测试→Ch3+Ch4**：
| 测试要点 | 位置 | 覆盖的设计/实现 | 链路状态 |

**代码→Ch4**（如适用）：
| Ch4 描述 | Ch4 位置 | 代码位置 | 一致? |
```

---

## Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的所有行号
- 验证所有数值常量
- 验证所有函数签名
- 按文档检查清单格式输出各维度验证表格

---

## Step 3.5: Precision Check（细节精确检查）— **Gate C**

> 大方向检查之后的第二层。5 个元规则跨阶段复用。Agent 标记可疑点，不要求自动判定正确性。

**3.5.1 外部知识标记**
- 扫描注释中的寄存器/标志位/协议/规范引用
- 输出标记列表供人工验证
- 口令："这个硬件行为/协议要求在上下文中成立吗？"

**3.5.2 通用接口纯度扫描**
- 扫描共享结构体/trait/公共 API 的字段/方法
- 问："对所有消费者上下文都有意义吗？"
- 标记仅在特定上下文有意义的元素

**3.5.3 返回值完整性扫描**
- 扫描外部调用返回值是否被使用/传递/注释说明可丢弃
- 标记被忽略且无说明的返回值

**3.5.4 资源生命周期闭环扫描**
- 扫描资源获取点，检查释放点或"不释放"理由
- 口令："谁释放？什么时候？不释放的理由？"

**3.5.5 理由可质疑性扫描**
- 扫描"因为/由于/避免/为了"类注释
- 输出"理由需质疑"标记列表

**中间产物**（Gate C）：
```markdown
### Step 3.5 产物：Precision Check（Gate C）

| 元规则 | 位置 | 内容 | 人工确认建议 |
|--------|------|------|------------|
| 3.5.1 外部知识 | vm.rs:267 | CR4.PSE 注释 | 验证 x86-64 长模式 |
| 3.5.4 资源生命周期 | vm.rs:801 | Option 未 take() | 验证清理路径 |
```

### Step 3.5a: 纵向链路检查（Vertical Link Check，文档 Review 强制）

> **目的**：Step 2.5 的 Link Validation 检查"横向链路"（Ch3→Ch4→Ch5 之间），本步骤检查"纵向链路"——Ch1 概念 → Ch3 设计决策 → Ch4 实现 → Ch5 测试 的端到端可追溯性。
> 来源：03-kmain-cstart
> **详见**：[review-rules/review-process.md §Step 3.5a](../review-rules/review-process.md)。

**检查项**：
1. Ch1 引入的每个核心概念 → Ch3 是否有对应设计决策？无 → P1（概念无落地）
2. Ch3 每个设计决策 → Ch4 是否有对应实现？无 → P1（决策无实现）
3. Ch4 每个核心类型/函数 → Ch5 测试是否覆盖？无 → P1（实现无测试）
4. Ch5 每个测试 → 是否能追溯到 Ch3 设计决策？无 → P2（测试无设计依据）

**输出格式**：
```markdown
### Step 3.5a 产物：纵向链路检查

| Ch1 概念 | Ch3 决策 | Ch4 实现 | Ch5 测试 | 链路完整? |
|---------|---------|---------|---------|----------|
| 保护结构 | §3.2 ProtectionArch trait | §4.1 ProtectionArchImpl | §5.2 test_protection | ✅ |
| CPU 三问 | §3.1 三问框架 | §4.2 prot_init | §5.1 test_three_questions | ✅ |
| ... | ... | ... | ... | ... |
```

**判定**：链路断裂 → P1；测试无设计依据 → P2。

### Step 3.5b: 因果链抽样验证（文档 Review 强制）

> **目的**：从 Ch2 抽样"为什么这样设计"的解释，验证其因果链每一步是否成立。
> **与 §2.0.3 的关系**：§2.0.3 是全量因果链验证（所有带"因为/所以"的 claim）；本步骤是聚焦 Ch2 设计解释的抽样验证。两者互补，不重复。
> **详见**：[review-rules/review-process.md §Step 3.5b](../review-rules/review-process.md)。

**执行步骤**：
1. 从 Ch2 抽取 5-10 个"为什么这样设计"的解释
2. 对每个解释，识别其因果链（A→B→C→结论）
3. 验证因果链每一步是否成立（用 C 语义/ISA 规范）
4. 失败 → P0（模式 48 因果链编造）

**输出格式**：
```markdown
### Step 3.5b 产物：因果链抽样验证

| Ch2 位置 | 设计解释 | 因果链 | 每步成立? | 判定 |
|---------|---------|--------|----------|------|
| §2.3 L45 | "memcpy 必要因为栈帧被覆盖" | 栈帧被覆盖→需复制 | ❌ (C 语义错) | P0 |
| §2.4 L78 | "refcount=0 因为初始化" | 初始化→refcount=0 | ✅ | ✅ |
```

---

## Step 4: Cross-Document Check（跨文档联动）

> 局部 Review 跳过。

- 优先检查同目录文档，其次"参见"章节外部文档
- 检查共享数据结构（`vmproc`/`vir_region`）、共享常量（`CLICK_SIZE`）、跨模块调用
- 重复处理检查：同目录已有完整处理的概念→精简为引用

### 4.1 语义归属判定（grep 辅助）

1. 提取 Ch1&2 中所有 C 符号（函数/结构体/宏名）
2. grep 在同目录 `.md` 中搜索
3. 以**功能语义**为准判定归属（如 `vm_mappages` 定义在 `mmap.c` 但语义属"页表操作"）

```markdown
| 符号 | 类型 | 语义归属 | 当前覆盖状态 | 处理建议 |
|------|------|---------|-------------|---------|
| vm_mappages | 函数 | 07-pagetable-ops.md | 未覆盖 | 在 07 中补充 |
| pt_t | 结构体 | 06-pagetable-struct.md | 已覆盖 | 无需处理 |
```

### 4.2 Design Quality Check（设计质量检查）

1. 列出 Ch3 所有设计决策
2. 检查：可追溯性、场景覆盖、no_std 可行性
3. 发现更好替代方案→Ch3 加 TODO

### 4.2.1 Rust 代码设计质量检查

1. 列出所有自定义 trait
2. 检查：多态必要性、trait bound 使用、机制vs策略分离
3. 不必要的 trait→P1

---

## Step 4.5: Test Verification（测试章节验证）— **Gate E**

**适用条件**（2026-08-15 修复 A-P1-2 明确触发语义）：文档含 §5 测试章节（或类似测试要点章节）时执行。触发条件为文档标题含 `^## §?5(\.|\s)|^# 5(\.|\s)|测试章节|验证章节` 任一模式；纯设计文档 / 局部 Review（仅 Ch1&2）跳过。

**执行步骤**：
1. 提取文档 §5 列出的所有测试函数名
2. 对每个测试函数名执行 grep：
   ```bash
   rg "fn {test_name}" {rust_dir} --type rust -n
   ```
3. 判定：
   - 存在 → ✅
   - 不存在 → ❌ **P0（测试缺失）**

**中间产物**（Gate E）：
```markdown
### Step 4.5 产物：测试章节验证（Gate E）

| 文档 §5 测试名 | grep 命令 | grep 结果 | 判定 |
|---------------|----------|----------|------|
| test_vmctl_clear_pagefault | `rg "fn test_vmctl_clear_pagefault" os/` | 0 matches | ❌ P0 缺失 |
| test_vmctl_param_from_u32 | `rg "fn test_vmctl_param_from_u32" os/` | vm.rs:1333 | ✅ 存在 |

**统计**：文档承诺 N 个测试，实际存在 M 个，缺失 N-M 个 → P0
```

> **若文档无 §5**：输出 "文档无 §5 测试章节，Gate E 不适用"，不阻断。

---

## Step 5: Final Review Output（最终输出）

- 按输出模板整理发现
- 每个问题标优先级（P0/P1/P2）+ 明确修改方向
- 必须含：维度覆盖自检、最弱项自检、时间预算评估、**Skill Invocation Log**、**Blocker Gates 通过状态**

### Step 5.5: 状态写入与收敛判断

> 完成当前 phase 的验证后，将结果持久化写入工具对应的路径（见 Step 0 统一布局）。**此步是强制步骤，不是可选**。即使用户约束 "read-only"，agent 也必须在 scan.md 写 "STATE 未更新原因：用户约束"，并在 STATE 追加 `## Pending Updates (blocked)` 列出应同步的 P0/P1。不允许静默跳过。

**中间产物（精简版）**：
1. 更新或创建工具对应的 `STATE.md`：
   - Trae → `.review/trae/{module}/STATE.md`
   - Claude → `.review/claude/{module}/STATE.md`
2. 更新或创建 `SYMBOLS.md`（Step 1.5 机器产物）：
   - Trae 单文档 → `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
   - Claude 单文档 → `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
3. **所有维度结果写入 scan.md 单文件**（NOT 10 个维度检查文件）。
   - Trae → `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
   - Claude → `.review/claude/{module}/{doc-stem}/scan.md`
   - **双写模式**：若需交互式修复 todo，额外在被 review 文档同目录写 `{doc-stem}-trae-review.md`（Trae）/ `{doc-stem}-claude-report.md`（Claude）。修复文档 = 标准 scan 的可读子集 + todo 进度跟踪。

**Artifact Inventory**（scan.md 末尾必备，Gate 0 校验项；含双写校验）：
```markdown
## Artifact Inventory
| 产物 | 预期路径 | 实际存在 | 大小 | SHA256 |
|------|---------|---------|------|--------|
| scan.md (标准) | .review/trae/{module}/scans/{doc-stem}-{agent}-scan.md | ✅ | 12KB | abc123… |
| structure.md | .review/trae/{module}/scans/{doc-stem}-{agent}-structure.md | ✅ | 4KB | def456… |
| SYMBOLS.md | .review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md | ✅ | 8KB | 789xyz… |
| 交互式修复文档（双写，Trae） | notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md | ✅ | 6KB | ghi012… |
```
**双写校验**：修复文档/最终报告与 scan.md 不是 checksum 一致，而是"子集关系"——修复文档的每条 issue 必须能在 scan.md 中找到对应条目（按 ID/位置匹配），反向不要求。

**Severity Reconciliation**（scan.md 必备新段，各自工具内部跨轮次调和，不跨工具）：
```markdown
## Severity Reconciliation
| Issue | Prior severity (STATE) | New severity | Rationale | C-source evidence | Confirmed? |
|-------|------------------------|--------------|-----------|-------------------|------------|
| IDT handler=0 | P0 (06-17) | P1 (本次) | 文档 §4.3 已声明设计决策 | trap_entry.rs:175 | ✅ 降级 |
```
降级 P0 必须附 C 源或设计文档 `file:line` 证据；无证据则维持 P0。

**STATE.md 格式**：

```markdown
# Review State: {module-name}

- **Phase**: [concept-check | ref-check | struct-check | coverage | design | link | code | cross-doc | claims | verify | complete]
- **Last completed phase**: concept-check
- **Open P0 issues**: 3 (#1, #2, #3 from scan.md)
- **Open P1 issues**: 7
- **Open P2 issues**: 2
- **Convergence status**: NOT_CONVERGED (5 phases remaining)
- **Next action**: Run ref-check phase with fresh context
- **Blocker Gates**: A✅ B✅ C✅ D✅ E✅ (all passed)

## Phase Completion Log
| Phase | Date | Passes | P0 found | P1 found | P2 found |
|-------|------|--------|----------|----------|----------|

## 新增/关闭问题同步规则（强制）
- 每次 review 结束后，必须将 scan.md 中的 **新发现 P0/P1/P2** 同步到 STATE.md 的 Open P0/P1/P2 列表。
- 不能仅在 Phase Completion Log 中记录；Open 列表必须实时更新。
- 已修复的问题从 Open 列表移除，并移动到 "Closed Issues" 段落，注明修复 scan/日期。

## Per-Doc Status
| Doc | Last scan | Agent | P0 | P1 | P2 | VERIFY? |
|-----|-----------|-------|----|----|----|---------|
| 03-kmain-cstart | 2026-06-19 | glm | 0 | 1 | 3 | ❌ |
| 04-clock-interrupt-init | 2026-06-19 | glm | 0 | 1 | 3 | ❌ |

## Session Status
| # | Session | 范围 | Status | Started | Completed | 产出 |
|---|---------|------|--------|---------|-----------|------|
| 1 | doc-03 | 1114 行 | ✅ DONE | 2026-06-19 | 2026-06-19 | scan-glm.md |
| 2 | doc-04 | 1319 行 | ⏳ PENDING | — | — | scan-glm.md (append) |
> 未完成 Session 标 `⏳ PENDING` + 截止位置；下个 Session 启动时读取本表 + session-plan.md 自动 resume。Session 间隔 >48h 自动标 STALE。

## Convergence Checklist
- [ ] §2.1 概念准确性 — COMPLETE / 0 new P0
- [ ] §2.2 C引用验证 — COMPLETE / 0 new P0
- [ ] §2.3 数据结构覆盖 — COMPLETE / 0 new P0
- [ ] §2.8 源码覆盖完整性 — COMPLETE / 0 new P0
- [ ] §2.9 设计决策质量 — COMPLETE / 0 new P0
- [ ] §2.10 章节链路 — COMPLETE / 0 new P0
- [ ] Code §1-14 — COMPLETE / 0 new P0
- [ ] 跨文档联动 — COMPLETE / 0 new P0
- [ ] Claims-Evidence — COMPLETE / 0 new P0
- [ ] 独立验证 — COMPLETE / PASS
```

**增量 Review 策略**：
1. Trae IDE 启动时读取 `.review/trae/{module}/STATE.md`（Claude Code Runtime 使用 `.review/claude/{module}/STATE.md`，两者隔离）
2. COMPLETE 的维度→跳过（读 scan.md 对应章节总结即可，不重做）
3. UNCHECKED 的维度→执行完整验证
4. 代码/文档有修改→检查是否影响已 COMPLETE 维度（有影响→标记 NEEDS_RECHECK）
5. 更新 STATE.md 和 scan.md

**收敛判断**：
```markdown
### 收敛状态评估

- [ ] 全维度覆盖：10/10 维度 COMPLETE
- [ ] P0 收敛：最近 Pass 新增 P0 = 0
- [ ] P1 收敛：最近 Pass 新增 P1 ≤ 1
- [ ] 独立验证：**Gate G** VERIFY-CHECK = PASS（**必须完成，不能跳过**）
- [ ] Blocker Gates：0/A/B/C/D/D-6/E/G/H 全部通过，且每个 Gate 都有 gate-evidence 附件

**当前状态**：CONVERGED / NOT_CONVERGED (N phases remaining)
**下一步**：[下一阶段名称] 或 [执行独立验证] 或 [审查已收敛，可结束]
```

> **重要**：VERIFY-CHECK.md 是收敛终止的必要条件。未完成 VERIFY-CHECK 时，状态必须为 NOT_CONVERGED，即使 P0=0。

#### Review 中断协议

当 review 遇到 design 缺失/错误时，必须触发 IN_DESIGN 状态（不是 DEFERRED）：

- **触发条件**：Step 0 design 预检发现 `design.md`/`design-final.md` 缺失（**主入口，前移**）；Step 1.6 发现 design 严重 outdated；Step 2 发现 design 未覆盖 Minix3 核心概念；Step 3.5 发现 design 抓错本质
- **中断流程**：标注 scan.md 为 IN_DESIGN → 生成 IN_DESIGN.md → 不动文档/代码 → design 完成自动恢复
- **状态机扩展**：`PENDING → IN_PROGRESS → IN_DESIGN（去 design） → RESUMED → IN_PROGRESS → CONVERGED`

详见 [review-rules/review-process.md §一.附录 A](../review-rules/review-process.md)。

#### 收敛判定：双层 Layer 模型

| 层级 | 目标 | 通过条件 | 状态标记 |
|------|------|---------|---------|
| **Layer 1（Correctness）** | C → Rust 语义对齐 | P0 修复 + Gate H + Gate 0/A/B/C/D/D-6/E/G/H + Step 5.6 | NOT_CONVERGED if FAIL |
| **Layer 2（Excellence）** | redox/textbook 级质量 | §4.3.5 + §4.4 + §15.5 + §2.0 架构演进 ≥80% | allowed CONVERGED w/ warning if PARTIAL |

**关键判定**：
- 必须先 1 后 2（正确性优先）
- Layer 1 FAIL → NOT CONVERGED（强制修复）
- Layer 1 PASS + Layer 2 PARTIAL → CONVERGED with warning
- Layer 1 PASS + Layer 2 PASS → 完全 CONVERGED

详见 [review-rules/review-process.md §三 状态管理与收敛判断](../review-rules/review-process.md)。

### Step 5.6: Review Verification Protocol（独立验证，Gate G）

> **目的**：解决"自己审自己"的盲区。分两阶段：VERIFY-SELF（Step 4.5 后立即自检）+ VERIFY-CROSS（收敛前独立验证）。
> 用户指令：「验证 review」或「review of review」
> **VERIFY-CHECK 在各自工具内部独立进行**：Trae 内可通过多 AI bagging 互为验证；Claude 内部需独立会话验证。**不跨工具互验**。

#### VERIFY-SELF（Step 4.5 后立即执行）
- 验证自己 review 中的 5 个 P0 检查项是否都有 grep 证据（Gate D 自检）
- 抽样 20% 自报 Issue 反向验证（source evidence 是否充分？判定等级是否合理？）
- 输出 self-check 表

#### VERIFY-CROSS（Step 5.6 原位置，收敛前）
**执行步骤**（独立会话 / Trae 内跨 AI 聚合）：
1. 读取 `.review/trae/{module}/STATE.md` + `scan.md` + 原始文档/代码
2. **分层抽样**（2026-08-15 修复 C-P1-5：AI 无内置"随机"，用分层抽样替代）：从 scan.md 的 Issue List 中按 P0/P1/P2 比例各取头 20% + 尾 20% + 中间 20% 的已报告问题。例如 Issue List 共 20 条（P0=3, P1=12, P2=5）→ P0 取第 1 条 + 最后 1 条 = 2 条；P1 取第 1/6/12 条 = 3 条；P2 取第 1 条 = 1 条，合计 6 条（30%）。**禁止**仅抽 P0（应全层级覆盖）
3. **反向验证**：对每个抽样问题——source evidence 是否充分？判定等级是否合理？
4. **遗漏检查**：抽样 20% 的源码符号，验证是否都在文档/检查覆盖
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有已标记 COMPLETE 但实际未完成的维度
6. **Blocker Gates 复验**：检查 scan.md 是否真的通过了 0/A/B/C/D/D-6/E/G/H 全部 Gate（含 gate-evidence 块）
7. **跨 agent 验证推荐**（新增，2026-07-16）：若条件允许，优先由**不同 agent** 执行 VERIFY-CHECK。同 agent 验证时必须基于 grep 命令重放（非语义回忆），并在 VERIFY-CHECK.md 中标注"同 agent 验证，已重放 grep 命令"。

**中间产物**（写入 `.review/trae/{module}/VERIFY-CHECK.md`）：
```markdown
### Review Verification Result

**抽样一致性**：X/Y = Z%
**遗漏检查**：N 符号抽样，M 遗漏
**收敛验证**：K/10 维度可信
**Blocker Gates 复验**：0/A/B/C/D/D-6/E/G/**H** 全部真实通过? ✅/❌（2026-08-15 修复 C-P0-2）
**验证 agent**：[agent name] / 同 agent（已重放 grep 命令）

| 抽样问题 | scan.md 判定 | 独立重新判定 | 一致? |
|---------|-------------|------------|-------|

**判定**：PASS / CONCERN / FAIL

**验证局限说明**：本验证 [是/否] 由不同 agent 执行；同 agent 验证时已重放 grep 命令但可能存在确认偏差，建议关键 P0 由跨 agent 二次验证。
```

**判定标准**：
- **PASS**：一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过 → Gate G 通过，审查完成
- **CONCERN**：一致性 70-90% → 特定维度需重新审查（标注在 STATE.md）；Gate G 未通过，不得标 CONVERGED
- **FAIL**：一致性 < 70% 或发现关键遗漏 → 标记 STATE.md 中相关维度为 NEEDS_RECHECK

**轻量 VERIFY 入口**：任何 review 结束后若未达 CONVERGED，仍可触发轻量 VERIFY——抽样 20% 已报 issue 反向验证 + 抽样 20% 源码符号覆盖检查。STATE 的 `VERIFY` 字段细分为 `LIGHT (cross-AI)` / `FULL (independent session)`。

---

## Step 6: Action Item Generation（修改项生成）

> 局部 Review 跳过。将发现转化为可执行修改项。

对每个 P0/P1 问题生成：

```
### TODO #N: [简述]
- **优先级**: P0/P1
- **类型**: 设计缺陷 / 代码-设计不一致 / no_std 违规 / 语义偏移 / ...
- **文件**: `path/to/file.rs`
- **问题**: [详细描述]
- **修改方案**: [具体方案]
- **验证**: [如何验证修改正确]
```

P0 必须有代码修改项。P1 涉及设计改进→Ch3 加 TODO 段落。

---

## Step 7: 自检清单确认（强制）

> 以下任何一项未完成，回到对应 Step 重新执行。

```markdown
### Step 7 产物：自检清单

- [ ] Step 0 范围声明和时间预算已输出
- [ ] Step 0 STATE.md 状态已检查
- [ ] **Step 0.5 structure.md 已生成 + 评审表已输出（文档 Review）→ Gate D-6**
- [ ] **Step 0.5.3 outline 偏离矩阵已输出**（若 outline.md 存在，方案 D 新增）
- [ ] **Step 0.5.5 跨章节一致性矩阵已输出**（新增，2026-07-16）
- [ ] **Step 0.7 TODO 验证表已输出**（若输入含 TODO 清单，新增，2026-07-16）
- [ ] Step 1 源码文件清单已输出
- [ ] **Gate A**: Step 1.5 覆盖率穷举已输出（SYMBOLS.md + 缺口/ARCH 判定）
- [ ] **Gate B**: Step 2 Top 5 差异已输出（3 语义偏移 + 2 覆盖缺口 + 8 字段契约表）
- [ ] Step 2.5 链路验证表格已输出（如适用）
- [ ] Step 3 概念准确性表格已输出
- [ ] Step 3 C 代码引用验证表格已输出
- [ ] Step 3 数据结构覆盖表格已输出
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）
- [ ] Step 3 文档风格验证表格已输出（§2.11）
- [ ] **Gate C**: Step 3.5 Precision Check 5 元规则检查表已输出
- [ ] **Step 3.5a 纵向链路检查已输出（文档 Review）**
- [ ] **Step 3.5b 因果链抽样验证已输出（文档 Review）**
- [ ] Step 4 跨文档检查已输出
- [ ] Step 4.1 语义归属判定已输出（如适用）
- [ ] Step 4.2 设计决策质量表格已输出（如适用）
- [ ] **Gate D**: P0 必检清单 5 项已回答（见 patterns-skill §0，PARTIAL=FAIL）
- [ ] **Gate E**: Step 4.5 测试章节验证已输出（若文档有 §5）
- [ ] **Gate H**: design 门控 H.1-H.6 已 grep 验证（**所有 review 模式必检**，2026-08-15 修复 C-P0-2；H.6 = outline.md 存在 + doc↔outline 无 P0 偏离，方案 D 新增）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 8 个问题已确认（含 Blocker Gates + Design 对齐检查，v6）
- [ ] **Layer 1/2 双层判定已输出**
- [ ] Step 5 时间预算评估已输出
- [ ] **Skill Invocation Log** 已输出
- [ ] Step 5.5 STATE.md 和 scan.md 已写入（NOT 10 个维度文件）
- [ ] Step 5.5 收敛状态评估已输出
- [ ] **Artifact Inventory 已输出（Gate 0 校验项，含双写校验）**
- [ ] **Severity Reconciliation 已输出（若有跨轮次严重性调和）**
- [ ] **Gate G**: Step 5.6 VERIFY-CHECK 已产出（PASS；CONCERN/FAIL 不得标 CONVERGED）
- [ ] **Step 5.7 Rule Discovery 已填写（是否发现新模式 ✅/❌ + 草案）**
- [ ] **Step 7.1 收敛成本评估已输出**
- [ ] Step 6 修改项已生成（P0 必须有代码修改项）
- [ ] 所有 grep 命令输出作为证据附在对应表格后
```

### Step 7.1: 收敛成本警告（强制）

> **目的**：防止"过度收敛"——为了把 P1 降到 0 而反复 review，成本超过收益。
> 来源：用户反馈
> **详见**：[review-rules/review-process.md §Step 7.1](../review-rules/review-process.md)。

**判定规则**（任一触发即应停止并交付）：
1. **轮次阈值**：同一文档累计 review ≥ 5 轮 → 强制交付当前结果，剩余 P1/P2 转为 backlog
2. **P1 边际递减**：连续 2 轮新发现 P1 ≤ 1 → 视为收敛，剩余 P1 转为 backlog
3. **成本/收益比**：当前轮 review 耗时 > 上一轮 80% 但新发现问题 < 上一轮 20% → 停止
4. **首次零发现**：首次 review 0 P0/P1/P2 → 触发**漏检自检**：随机抽 3 个检查项重跑（推荐：Gate D 第 1/3/5 项 + Step 2 因果链抽样）；若仍 0 发现 → 交付；若发现遗漏 → 之前的 review 标记 DRAFT，补完后再交付

**输出**：在 scan.md 末尾标注"收敛成本评估"：
```markdown
### 收敛成本评估
- 当前轮次: N
- 本轮新发现: P0=X, P1=Y, P2=Z
- 触发停止规则: [1/2/3/无]
- 决定: 继续收敛 / 强制交付（剩余转 backlog）
```

### Step 5.7: Rule Discovery（规则发现，强制填写）

> **目的**：将 review 中发现的新模式反馈到规则集，实现规则演化。
> **详见**：[review-rules/review.md §规则演化机制](../review-rules/review.md)、[review-rules/review-process.md §Step 5.7](../review-rules/review-process.md)。

**执行步骤**：
1. 回顾本轮 review 发现的所有问题
2. 判断是否有 ≥2 次同类新模式（现有规则未覆盖的）
3. 若有 → 生成新模式提案（含案例、判定、归类、规则草案）
4. 写入 scan.md §Rule Discovery 段落
5. 用户确认后，落地到对应 rules 文件

**输出格式**：
```markdown
### Rule Discovery
- 本次 Review 是否发现新模式？[✅/❌]
- 若 ✅：
  - 新模式名: [名称]
  - 案例: [file:line + 描述]
  - 判定: [P0/P1/P2]
  - 归类: [文档/代码/跨阶段/卓越性/叙事概念]
  - 规则草案: [一句话描述]
  - 建议落地文件: [review-patterns.md / review-doc-checklist.md / ...]
```

---

## 工具命令速查

```bash
# ===== {module} = vm/pm/vfs/kernel 等 =====

# 验证常量
rg "^#define CONSTANT" minix3/minix/servers/{module}/ -n
rg "^#define CONSTANT" minix3/minix/kernel/ -n

# 验证函数
rg "^return_type function_name\(" minix3/minix/servers/{module}/ -n

# 验证结构体
rg "^struct struct_name " minix3/minix/servers/{module}/ -n
rg "^typedef struct" minix3/minix/servers/{module}/ -A 5

# 验证宏
rg "MACRO_NAME" minix3/minix/servers/{module}/ --type c -n

# 枚举值（头文件）
rg "ENUM_VALUE" minix3/minix/include/ --type h -n

# 全局搜索
rg "SYMBOL_NAME" minix3/minix/ --type c --type h -n

# 列出 C 源文件
ls minix3/minix/servers/{module}/*.c
ls minix3/minix/servers/{module}/*.h

# 提取函数定义
rg "^[a-z_].*\w+\(.*\)\s*$" minix3/minix/servers/{module}/FILE.c -n

# 跨文档搜索
rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n

# 同目录常量重复
rg "CONSTANT\s*=" "notes/rewrite/{module}/" --type md -n

# ===== P0 必检清单命令（Gate D）=====

# 1. 文档 §5 测试是否存在
rg "fn {test_name}" {rust_dir} --type rust -n

# 2. trait 是否有 impl
rg "impl.*{TraitName}" {rust_dir} --type rust -n

# 3. 函数是否在声明的文件中
rg "fn {name}" {file}

# 4. 核心算法是否是 stub（含 panic! 检查）
rg "spin_loop\|todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n

# 5. 文档 §4 签名是否与实际一致（逐函数对比）
```

### 模块路径速查

| 模块 | 源码路径 |
|------|----------|
| VM | `minix3/minix/servers/vm/` |
| PM | `minix3/minix/servers/pm/` |
| VFS | `minix3/minix/servers/vfs/` |
| Kernel | `minix3/minix/kernel/` |
| Drivers | `minix3/minix/drivers/` |
| 公共头文件 | `minix3/minix/include/` |

---

## 五、修复阶段工作流（Fix Phase）

> Review 结束后进入修复阶段时，AI 必须按本流程执行，确保修复不违反 review 规则。

### 1. 修复前准备
1. 重读 STATE.md 中的 Open Issues 列表与对应的 scan.md Issue List。
2. 按问题类型显式加载 Skill：
   - 代码修复（Rust） → `review-code-skill` + `review-patterns-skill`
   - 文档修复（Markdown） → `review-doc-skill` + `review-patterns-skill`
   - 涉及核心语义（IPC/生命周期/错误/权限/地址空间） → `review-core-semantics-skill`
   - 涉及覆盖率/状态追踪 → `review-process-skill` + `review-coverage-skill`
3. 对每个修复项确认：修改范围、验证方法、是否引入新的 P0/P1。

### 2. 修复执行原则
- **先 P0 后 P1/P2**：P0 全部修复并验证前，不标记收敛。
- **文档与代码同步修**：改代码若影响 Ch4 描述，必须同步改文档；改文档若已要求代码实现，必须同步改代码。
- **禁止引入新的违反**：修复过程中仍需满足 no_std、硬件抽象 trait、SMP/BKL、Claims-Evidence 等约束。
- **保留证据**：每个修复项在 scan.md / STATE.md 中记录：修复日期、修改文件、验证命令输出。

### 3. 修复后验证
1. **单元测试**：`cargo test -p <crate>` 必须全部通过。
2. **编译检查**：`cargo check` 无新增 error；新增 warning 需说明理由。
3. **重新跑相关 Gate**：
   - 修了代码语义 → 重新跑 Gate B（Top 5 差异表）抽样验证。
   - 修了代码/测试 → 重新跑 Gate D（P0 必检）和 Gate E（§5 测试存在性）。
   - 修了文档 claim → 重新跑 Gate A/C 相关部分。
4. **更新 STATE.md**：将已修复问题从 Open 列表移入 Closed Issues，注明修复 scan/日期，更新 Convergence Checklist。

### 4. 修复结束标准
- 本次计划修复的所有 P0 已修复并验证。
- 未修复的 P0 必须标记为 `WONTFIX` 并给出不可辩驳的理由（如架构演进明确替代）。
- 最新一次完整 Pass：新增 P0 = 0，新增 P1 ≤ 1。
- 完成 VERIFY-CHECK.md 后才可标记 **CONVERGED**。
