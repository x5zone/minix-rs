# process: 执行流程与跳过检测（合并 process + skip-check）

> 本文件合并原 review-process 相关流程和 doc/12-skip-check.md。
> **强制规则**：每个 Step 必须产生可见中间产物。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。

---

## 通用规则

1. **先读后判**：每个判定必须先执行 grep 验证。
2. **Evidence 分级**：[DIRECT] / [MEDIUM] / [INFERRED]。
3. **零输出禁令**：0 个问题也必须写"Checked N items, found 0 issues"。
4. **可见中间产物**：每个 Step 必须产生表格、列表或 grep 输出。不允许"在脑子里过一遍"。

---

## ⛔ Blocker Gates（阻断门，必须通过才能输出 Final Review）

| Gate | 检查项 | 通过标准 | 未通过后果 |
|------|--------|---------|-----------|
| **0** | 制品完整性（Artifact Inventory） | 标准路径文件齐全（STATE/scan/structure/SYMBOLS）；scan.md 含 8 个 grep 可验锚段 | 禁止输出 Final Review |
| **A** | Step 1.5 Coverage Enumeration | 已运行 coverage-extract.py + SYMBOLS.md 落盘 + scan.md 附 `gate-evidence-A` 块 | 禁止输出 Final Review |
| **B** | Step 2 Diff Extraction | 已产出 Top 5 行为契约表（3 语义偏移 + 2 覆盖缺口，**8 字段 × 5 函数**） | 禁止输出 Final Review |
| **C** | Step 3.5 Precision Check | 已产出 5 元规则检查表 | 禁止输出 Final Review |
| **D** | P0 必检清单（见 patterns §0） | 5 项已回答（✅/❌ + grep 证据）；PARTIAL/⚠️ = FAIL | 禁止输出 Final Review |
| **D-6** | Step 0.5 structure.md Skeleton Review（文档 review 专用） | structure.md 已生成 + 12 节评审表 + 失败项写入 Issue List | 禁止输出 Final Review（文档 review） |
| **E** | Step 4.5 Test Verification | 文档 §5 每个测试函数已 grep 验证（若文档有 §5） | 禁止输出 Final Review |
| **G** | Step 5.6 VERIFY-CHECK | VERIFY-CHECK.md 已产出 + 判定 PASS（一致性 ≥ 90%） | 禁止标记 CONVERGED |

**任一 Gate 未通过 → scan.md 标记 DRAFT，禁止写入 STATE.md。**

**Gate 证据规则（新增）**：
- scan.md 中每个 Gate 的通过声明必须写入带标签的 `gate-evidence-{X}` 代码块，附带**实际命令 + 输出片段**作为证据。
- Gate 0：Artifact Inventory 表（预期路径 / 实际存在 / 大小）。
- Gate A：附 coverage-extract.py 命令行及 stdout 覆盖率摘要。
- Gate B：附 5 行 × 8 字段行为契约表。
- Gate D：附 5 项 P0 必检的 grep/Read 证据（命令 + 结果）。
- Gate D-6：附 structure.md 路径 + 12 节评审表。
- Gate E：附每个测试函数名的 `rg "fn {name}"` 命令 + 结果表。
- Gate G：附 VERIFY-CHECK.md 路径 + 抽样一致性百分比。
- 仅有 "✅ 通过" 而无证据的 Gate 视为未通过。

**证据强度分级**：L1（工具自动输出，Gate A/D/E 必须）；L2（手动 grep，Gate B/C 可接受）；L3（语义推断，视为 FAIL，除非标 `MANUAL_FALLBACK` 并说明原因）。

**Skill Invocation Log**（mandatory in scan.md）：
```markdown
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Step 3 | §6 概念准确性表 |
```
未含此章节 → scan.md 标记 DRAFT。

---

## 〇、执行模式选择（构造 / 快速 / 深度）

| 模式 | 适用场景 | 执行 Step | 预计耗时 |
|------|---------|----------|---------|
| **构造（Constructive）** | 初稿阶段，引导补全 | 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 |
| **快速（Quick）** | 日常 PR、时间有限 | 0, 1, 2, 5 | 10~20 分钟 |
| **深度（Deep）** | 里程碑验收、关键模块 | 0-7（全量） | 40~120 分钟 |

**决策树**：初稿→构造；日常PR/时间紧→快速（P0>3 则升级深度）；里程碑/关键模块→深度。

---

## Step 0: 范围声明 + 时间预算 + 状态恢复

- 按 §〇 确定执行模式（构造/快速/深度）
- 声明 Review 模式和范围
- 声明时间预算（可选；若填写，按 | <200行→10-20分 | 200-500→20-40 | 500-1000→40-80 | >1000→80-120 |）
- **读取状态（统一双路径，互不共享中间结果）**：
  - **Trae IDE** → 读取 `.review/trae/{module}/STATE.md`（项目根 `.review/`）
  - **Claude Code Runtime** → 读取 `.review/claude/{module}/STATE.md`（项目根 `.review/`）
  - Trae 与 Claude 各自维护独立 STATE.md；**绝不共享任何中间结果**（STATE/scan/SYMBOLS/structure/VERIFY-CHECK）。Bagging 聚合只发生在 Trae 内。
  - 若**同一工具**下两份 STATE.md 同时存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户以哪个为准。
  - `{module}` = 目标文档路径中 `notes/rewrite/` 下的第一级目录名。`{doc-stem}` = 目标文档去扩展名。`{agent}` = 模型标识。
  - 推荐 `tools/review-init.sh claude {doc-path}` 自动计算路径并 mkdir。

**中间产物**：
```
- **执行模式**：[构造/快速/深度]
- **Review 模式**：[文档/代码/完整/局部]
- **目标文件**：xxx.md / xxx.rs
- **规模**：约 N 行 | **预计**：X~Y 分钟
- **前置状态**：STATE.md 存在 → 已完成 phase [X, Y, Z] / 不存在 → 从零开始
```

---

## Step 1: Ground Truth Lookup（源码定位）

- 识别文档中所有 Minix3 源文件
- 用 `rg` 验证文件存在于 `minix3/` 目录
- 记录每个引用的文件路径和行号范围

**中间产物**：
| 文件路径 | 文档引用位置 | 文件存在? | 引用行号范围 |
|---------|------------|----------|------------|

---

## Step 0.5: structure.md Skeleton Review（骨架评审）— **Gate D-6**（文档 review 专用）

> **核心原则**：reviewer 必须先提取文档骨架并评审，再执行正确性检查。正确性检查验证"文档说了什么"，structure.md 验证"读者读到了什么"。两者正交。
> **前置条件**：仅文档 review（`.md` 文件）。纯代码 review 跳过。
> **模板**：`prompt/skill/review-process-skill.md` §Step 0.5。

Step 0.5.1 按 12 节模板生成 structure.md（概念文档全量 12 节，实现文档简化）：
1. **主题思想（一句话）** — doc in one sentence; cannot say → P1
2. **目标读者** — beginner/intermediate/advanced + prerequisites; undeclared → P1
3. **叙事主语** — prot_init()/CPU/reader/OS (pick one + evidence); subject=function name → P1
4. **驱动方向** — WHY→WHAT→HOW / WHAT→HOW / HOW-only; HOW-only in Ch1 → P1
5. **文档大纲** — H2 outline with page numbers; Ch1 outline = function names → P1
6. **核心概念清单** — list core concepts; missing core concepts → P1
7. **跨架构统一抽象** — multi-arch docs only: unified abstraction first; missing → P1
8. **双向闭环** — entry mechanism covers both entry AND return; one-way → P1
9. **叙事弧** — problem→solution→verification arc; missing → P2
10. **元注释** — author narrating writing strategy in body text; >5 instances → P1
11. **裸概念复述** — reader can retell core concept after reading Ch1; cannot → P1
12. **纵向链路映射** — Ch1 concept ↔ Ch2 C code ↔ Ch3 design ↔ Ch4 impl; broken link → P1

Step 0.5.2 按 12 节评审表逐项判定，失败项写入 scan.md Issue List
Step 0.5.3 structure.md 评审通过后才进入 Step 1（覆盖率穷举）

**中间产物**（Gate D-6）：
```markdown
### Step 0.5 产物：structure.md 骨架评审（Gate D-6）

structure.md 路径：{path}

| # | 节 | 判定 | 失败原因 |
|---|----|------|---------|
| 1 | 主题思想 | ✅/P1 | ... |
| 2 | 目标读者 | ✅/P1 | ... |
| ... | ... | ... | ... |
| 12 | 纵向链路映射 | ✅/P1 | ... |
```

> ⛔ **Gate D-6**: structure.md generated + 12-section review table complete + failures in Issue List. Failure → scan.md DRAFT.

---

## Step 1.5: Coverage Enumeration（覆盖率穷举，机器+AI）— **Gate A**

> **详见**：[doc.md Check 00](doc.md#check-00-claims-evidence-tracing论文级质量) — Coverage Enumeration（机器穷举 + AI 补充）

1. **机器生成 SYMBOLS.md**（**强制运行**，见 Gate A 强制运行规则；不允许"语义范围手动验证"代替）。**Claude Code Runtime 输出路径硬编码为 `.review/claude/` — 不要使用 `{tool}` 变量：**
   ```bash
   # 模块级 — 服务器模块
   #   注：脚本第一参数 {minix3-module} 是 Minix3 模块名；--output 路径里的 {rw-module} 是 rewrite 模块名
   python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
     --output .review/claude/{rw-module}/scans/SYMBOLS.md

   # 模块级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --output .review/claude/{rw-module}/scans/SYMBOLS.md

   # 单文档级（推荐）— 服务器模块
   python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
     --output .review/claude/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

   # 单文档级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/kernel-semantic-map.json \
     --output .review/claude/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
   ```
   - `--rust-dir os`：扫描整个 `os/` 目录，避免遗漏跨 crate 符号。
   - `--c-dir`：服务器模块用 `minix3/minix/servers/{minix3-module}`，内核用 `minix3/minix/kernel`。
   - `--semantic-map`：C→Rust 改写必须提供语义映射表，否则 Rust 覆盖率会严重低估。
   - `--doc-file`：限定到单篇文档，避免两篇 doc 的 coverage 数字完全相同。
   - **Gate A 强制运行规则**：运行后必须将命令 + stdout 写入 scan.md 的 `gate-evidence-A` 块。若脚本物理不可用 → 显式记录 PARTIAL 状态（≠ PASS），不允许进 Final Review。
   - 若 Rust 覆盖率为 0%，必须先检查 `--rust-dir`/`--semantic-map` 是否正确，或确实缺失实现。
2. **AI 补充语义判断**（5 项，标注 evidence）：
   - Rust 对应关系确认 / 架构演进标记 / 语义归属 / 行为契约表 / 测试覆盖
3. **更新 STATE.md Coverage Status 段**

**中间产物**：
| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M (M%) |
| Rust 覆盖 | K (K%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口** / **ARCH 标记** 表格。

---

## Step 2: Diff Extraction（差异提取）— **Gate B**

- 列出 **5 个**最背离 Minix3 原始语义的地方：
  - **Top 3 语义偏移**：文档/代码描述与 C 行为不符（行为契约表）
  - **Top 2 覆盖缺口**：来自 Step 1.5 SYMBOLS.md 的"文档覆盖=✅ 但 Rust 覆盖=❌"符号
- 每项填写 8 字段行为契约表（见 core-semantics §2.2）

**中间产物**：
```markdown
### Step 2 产物：Top 5 差异（Gate B）

#### Top 3 语义偏移
| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |

#### Top 2 覆盖缺口（来自 SYMBOLS.md）
| # | 符号 | C 源码 | 文档覆盖 | Rust 覆盖 | 缺口性质 |

#### 行为契约表（8 字段 × 5 函数）
[见 core-semantics §2.2 模板]
```

---

## Step 2.5: Link Validation（链路验证）

> 局部 Review（仅 Ch1&2）时跳过。

1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计和实现？
4. 代码→Ch4：代码是否与文档一致？

**中间产物**：4 个链路验证表格。

---

## Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的所有行号
- 验证所有数值常量
- 验证所有函数签名
- 按 [doc.md](doc.md) 各 Check 要求的表格输出（同目录文件）

---

## Step 3.5: Precision Check（细节精确检查）— **Gate C**

> 5 个元规则跨阶段复用。Agent 标记可疑点，不自动判定。

1. **外部知识标记**：扫描注释中的寄存器/标志位/协议引用
2. **通用接口纯度扫描**：共享结构体/trait/API 字段对所有消费者有意义？
3. **返回值完整性扫描**：外部调用返回值是否被使用/传递/注释说明可丢弃
4. **资源生命周期闭环扫描**：资源获取点是否有释放点或"不释放"理由
5. **理由可质疑性扫描**：扫描"因为/由于/避免/为了"类注释

**中间产物**（Gate C）：
```markdown
### Step 3.5 产物：Precision Check（Gate C）

| 元规则 | 位置 | 内容 | 人工确认建议 |
|--------|------|------|------------|
```

### Step 3.5a: 纵向链路检查（Vertical Link Check，文档 Review 强制）

> **目的**：检查 Ch1 概念 → Ch3 设计决策 → Ch4 实现 → Ch5 测试 的端到端可追溯性。
> **来源**：03-kmain-cstart 案例——Ch1 讲"保护结构"但 Ch4 实现里找不到对应类型。
> **详见**：[review-rules/review-process.md §Step 3.5a](../../../../prompt/review-rules/review-process.md)。

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
```

**判定**：链路断裂 → P1；测试无设计依据 → P2。

### Step 3.5b: 因果链抽样验证（文档 Review 强制）

> **目的**：从 Ch2 抽样"为什么这样设计"的解释，验证其因果链每一步是否成立。
> **来源**：improve.md §6.3。
> **与 §2.0.3 的关系**：§2.0.3 是全量因果链验证（所有带"因为/所以"的 claim）；本步骤是聚焦 Ch2 设计解释的抽样验证。两者互补，不重复。
> **详见**：[review-rules/review-process.md §Step 3.5b](../../../../prompt/review-rules/review-process.md)。

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
```

---

## Step 4: Cross-Document Check（跨文档联动）

> 局部 Review 跳过。

- 优先检查同目录文档，其次"参见"章节外部文档
- 检查共享数据结构、共享常量、跨模块调用
- 重复处理检查：同目录已有完整处理的概念→精简为引用

### 4.1 语义归属判定（grep 辅助）
1. 提取 Ch1&2 中所有 C 符号
2. grep 在同目录 `.md` 中搜索
3. 以**功能语义**为准判定归属

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

> **模式编号映射**：本步骤对应源 [review-patterns.md §六 测试错误模式（35-40）](../../../../prompt/review-rules/review-patterns.md)。Step 4.5 缺失 = 模式 35（L1 对偶缺失）= P0。详见 [patterns.md §四 测试模式（35-40）](patterns.md)。

**适用条件**：文档含 §5 测试章节（或类似测试要点章节）

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
| test_foo | `rg "fn test_foo" os/` | 0 matches | ❌ P0 缺失 |
| test_bar | `rg "fn test_bar" os/` | vm.rs:1333 | ✅ 存在 |

**统计**：文档承诺 N 个测试，实际存在 M 个，缺失 N-M 个 → P0
```

> **若文档无 §5**：输出 "文档无 §5 测试章节，Gate E 不适用"，不阻断。

---

## Step 5: Final Review Output（最终输出）

- 按输出模板整理发现
- 每个问题标优先级（P0/P1/P2）+ 明确修改方向
- 必须含：维度覆盖自检、最弱项自检、时间预算评估、**Skill Invocation Log**、**Blocker Gates 通过状态**

### Step 5.5: 状态写入与收敛判断

**中间产物（精简版）**:
1. 更新或创建工具对应的 `STATE.md`（双路径，互不共享中间结果）：
   - Trae → `.review/trae/{module}/STATE.md`
   - Claude → `.review/claude/{module}/STATE.md`
2. 更新或创建 `SYMBOLS.md`（Step 1.5 机器产物）到对应路径：
   - Trae → `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
   - Claude → `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
3. **所有维度结果写入 scan.md 单文件**（NOT 10 个维度检查文件）。
   - Trae默认：`.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
   - Claude默认：`.review/claude/{module}/{doc-stem}/scan.md`
   - 若用户显式指定输出位置，**双写**：用户指定路径 + 工具默认路径（Trae交互式修复文档：`notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`；Claude最终报告：`notes/rewrite/{module}/{stage}/{doc-stem}-claude-report.md`）。
4. **同步新增/关闭问题**：将 scan.md 中新发现 P0/P1/P2 同步到 STATE.md Open 列表；已修复问题移入 Closed Issues 段落，注明修复 scan/日期。
5. 输出 **Artifact Inventory** 和 **Severity Reconciliation** 表（Gate 0 要求）。

**收敛判断**:
- [ ] 全维度覆盖：10/10 维度 COMPLETE in scan.md
- [ ] P0 收敛：最近 Pass 新增 P0 = 0
- [ ] P1 收敛：最近 Pass 新增 P1 ≤ 1
- [ ] **Gate G**: VERIFY-CHECK.md = PASS（**必须完成，不能跳过**）
- [ ] **Blocker Gates：0/A/B/C/D/D-6/E/G 全部通过，且每个 Gate 都有 gate-evidence 附件**

### Step 5.6: Review Verification Protocol（独立验证，Gate G）

> 所有维度 COMPLETE 后，**必须**生成 VERIFY-CHECK.md 才能标记 CONVERGED。用户指令：「验证 review」

1. 读取 STATE.md + scan.md + 原始文档/代码
2. **随机抽样**：从 scan.md 选取 20% 已报告问题
3. **反向验证**：对每个抽样问题独立重新验证
4. **遗漏检查**：抽样 20% 源码符号，验证覆盖
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist
6. **Blocker Gates 复验**：检查 scan.md 是否真的通过了 0/A-E+G 全部 Gate（含 gate-evidence 块）

**判定**：PASS（≥90%）/ CONCERN（70-90%）/ FAIL（<70%）

**输出**：写入工具对应的 VERIFY-CHECK.md 路径：
- Trae: `.review/trae/{module}/VERIFY-CHECK.md`
- Claude: `.review/claude/{module}/VERIFY-CHECK.md`

### Step 5.7: Rule Discovery（规则发现，强制填写）

> **目的**：将 review 中发现的新模式反馈到规则集，实现规则演化。
> **详见**：[review-rules/review.md §规则演化机制](../../../../prompt/review-rules/review.md)、[review-rules/review-process.md §Step 5.7](../../../../prompt/review-rules/review-process.md)。

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

### Step 7.1: 收敛成本警告（强制）

> **目的**：防止"过度收敛"——为了把 P1 降到 0 而反复 review，成本超过收益。
> **来源**：用户反馈"收敛成本"问题。
> **详见**：[review-rules/review-process.md §Step 7.1](../../../../prompt/review-rules/review-process.md)。

**判定规则**（任一触发即应停止并交付）：
1. **轮次阈值**：同一文档累计 review ≥ 5 轮 → 强制交付当前结果，剩余 P1/P2 转为 backlog
2. **P1 边际递减**：连续 2 轮新发现 P1 ≤ 1 → 视为收敛，剩余 P1 转为 backlog
3. **成本/收益比**：当前轮 review 耗时 > 上一轮 80% 但新发现问题 < 上一轮 20% → 停止

**输出**：在 scan.md 末尾标注"收敛成本评估"：
```markdown
### 收敛成本评估
- 当前轮次: N
- 本轮新发现: P0=X, P1=Y, P2=Z
- 触发停止规则: [1/2/3/无]
- 决定: 继续收敛 / 强制交付（剩余转 backlog）
```

---

## Step 6: Action Item Generation（修改项生成）

> 局部 Review 跳过。

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
- [ ] Step 0 范围声明和时间预算已输出（或已说明省略）
- [ ] Step 0 已读取正确的 STATE.md（Trae/Claude 双路径）
- [ ] Step 1 源码文件清单已输出
- [ ] **Gate D-6**: Step 0.5 structure.md 骨架评审已输出（文档 review 专用，12 节评审表）
- [ ] **Gate A**: Step 1.5 覆盖率穷举已输出（SYMBOLS.md + 缺口/ARCH 判定）
- [ ] **Gate B**: Step 2 Top 5 差异已输出（3 语义偏移 + 2 覆盖缺口 + 8 字段契约表）
- [ ] Step 2.5 链路验证表格已输出（如适用）
- [ ] Step 3 概念准确性表格已输出
- [ ] Step 3 C 代码引用验证表格已输出
- [ ] Step 3 数据结构覆盖表格已输出
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）
- [ ] Step 3 文档风格验证表格已输出
- [ ] **Gate C**: Step 3.5 Precision Check 5 元规则检查表已输出
- [ ] **Step 3.5a 纵向链路检查已输出（文档 Review）**
- [ ] **Step 3.5b 因果链抽样验证已输出（文档 Review）**
- [ ] Step 4 跨文档检查已输出
- [ ] Step 4.1 语义归属判定已输出（如适用）
- [ ] Step 4.2 设计决策质量表格已输出（如适用）
- [ ] **Gate D**: P0 必检清单 5 项已回答 ✅/❌ + grep 证据（PARTIAL=FAIL）
- [ ] **Gate E**: Step 4.5 测试章节验证已输出（若文档有 §5）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 8 问题已确认（含 Blocker Gates）
- [ ] Step 5 时间预算评估已输出（或已说明省略）
- [ ] **Skill Invocation Log** 已输出（真实 tool 调用记录）
- [ ] Step 5.5 STATE.md 和 scan.md 已写入（NOT 10 个维度文件）
- [ ] Step 5.5 Open P0/P1/P2 已同步
- [ ] Step 5.5 收敛状态评估已输出
- [ ] Step 5.6 VERIFY-CHECK.md 已生成（收敛终止必要条件）
- [ ] **Step 5.7 Rule Discovery 已填写（是否发现新模式 ✅/❌ + 草案）**
- [ ] **Step 7.1 收敛成本评估已输出**
- [ ] Step 6 修改项已生成（P0 必须有代码修改项）
- [ ] 所有 grep 命令输出作为证据附在对应表格后
```

---

## Meta-Check: Skip/Fake Check Detection（跳过/虚假检查检测）

> 在所有其他检查完成后、报告前执行。

**Execute**:
1. 回顾本次会话产生的所有检查输出。
2. 统计每个检查输出了多少行（排除检查头）。
3. 对任何零输出检查：很可能是被跳过/虚假执行。

**决策树**:
| Check | Zero Output? | Action |
|-------|-------------|--------|
| coverage (00) | YES | **立即重跑**。运行 coverage-extract.py，生成 SYMBOLS.md。 |
| claims-evidence (00) | YES | **立即重跑**。提取所有事实声明，grep 每个找证据。 |
| concept-accuracy (01) | sampled < 100% | **扩展采样**。 |
| doc-code-consistency (04) | YES | **立即重跑**。对比 Ch4 签名与实际 .rs。 |
| design-quality (09) | YES | **立即重跑**。提取 Ch3 决策，验证 Ch1&2 依据。 |
| trait-design (code 03) | YES | **立即重跑**。列出所有 trait，检查实现数和 bound 使用。 |
| exec-model (code 05) | YES | **立即重跑**。识别模块类型，检查 Send/Sync/UnsafeCell + BKL/CPU-local。 |
| memory-model (code 06) | YES | **立即重跑**。检查 MaybeUninit、alloc/free 配对。 |
| module-design (code 07) | YES | **立即重跑**。列出 pub 项，检查 pub(crate) 适当性。 |
| comments (code 10) | YES | **立即重跑**。检查 pub fn 文档、unsafe safety 段。 |
| 64bit (code 11) | YES | **立即重跑**。搜索 `as u32` 截断无注释。 |
| design-code-consistency (code 14) | YES | **立即重跑**。对比 Ch3 决策与实际代码。 |
| **P0 必检清单 (Gate D)** | YES | **立即重跑**。执行 5 项 grep 检查。 |
| **Step 4.5 测试验证 (Gate E)** | YES | **立即重跑**。grep 文档 §5 每个测试函数。 |

**Output**:
| Check# | Name | Zero Output? | Action Taken |
|--------|------|-------------|---------------|

**Pass condition**: 所有检查至少 1 行证据。
**⛔ 无法重跑 → 写 "NOT RE-RUN: <reason>"。禁止无声跳过。**

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

# 覆盖率穷举（服务器模块示例；内核把 --c-dir 换成 minix3/minix/kernel）
python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{module} \
  --semantic-map tools/coverage-extract/{module}-semantic-map.json

# ===== P0 必检清单命令（Gate D）=====

# 1. 文档 §5 测试是否存在
rg "fn {test_name}" {rust_dir} --type rust -n

# 2. trait 是否有 impl
rg "impl.*{TraitName}" {rust_dir} --type rust -n

# 3. 函数是否在声明的文件中
rg "fn {name}" {file}

# 4. 核心算法是否是 stub
rg "spin_loop\|todo!\|unimplemented!\|unreachable!" {rust_dir} --type rust -n

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
