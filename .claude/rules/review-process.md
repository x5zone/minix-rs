# Review Process — always loaded

Every review session must produce these visible artifacts. Do NOT "check in your head."

## ⛔ Blocker Gates (must pass before Final Review output)

| Gate | Check | Pass Criteria | Fail Consequence |
|------|-------|---------------|------------------|
| **0** | Artifact Inventory | Standard paths complete (STATE/scan/structure/SYMBOLS); scan.md contains 9 grep-verifiable anchor sections | DRAFT, no STATE.md write |
| **A** | Step 1.5 Coverage Enumeration | coverage-extract.py executed + SYMBOLS.md on disk + `gate-evidence-A` block in scan.md | DRAFT, no STATE.md write |
| **B** | Step 2 Diff Extraction | Top 5 behavior contract table (3 语义偏移 + 2 覆盖缺口, **8 fields × 5 funcs**) | DRAFT, no STATE.md write |
| **C** | Step 3.5 Precision Check | 5 meta-rules check table output | DRAFT, no STATE.md write |
| **D** | P0 Mandatory Checklist (see patterns §0) | 5 items answered (✅/❌ + grep evidence); PARTIAL/⚠️ = FAIL | DRAFT, no STATE.md write |
| **D-6** | Step 0.5 structure.md Skeleton Review (doc review only) | structure.md generated + 12-section review table + failures in Issue List | DRAFT, no STATE.md write (doc review) |
| **E** | Step 4.5 Test Verification | §5 each test function grep-verified (if doc has §5) | DRAFT, no STATE.md write |
| **G** | Step 5.6 VERIFY-CHECK | VERIFY-CHECK.md produced + verdict PASS (consistency ≥ 90%) | DRAFT, NOT CONVERGED |
| **H** | Step 1.6 Design + outline Alignment Check | 所有 review 模式必检：H.1 `design.md`（非 bagging）/ `design-final.md`（bagging）存在 + design 对齐检查 + design 缺口清单 + P0-design-missing 全处置 + **H.6 outline.v*.md 快照存在 + doc↔outline 无 P0 偏离**（方案 D 新增） | DRAFT, no STATE.md write |

**Any Gate failed → scan.md marked DRAFT, STATE.md NOT updated.**

**⛔ Gate H 不允许 N/A 判定**：每篇文档都必须通过 Gate H 全部 6 项检查。不允许"本文档复用其他文档 design，Gate H N/A"——这是 P0-process-violation。若本文档无专属 design.md，必须**执行 Step 0.3 嵌入生成**（2026-07-17 变更：原"切换 Design-First 模式生成"改为"Step 0.3 嵌入生成"），而不是标 N/A 跳过。

**Gate Evidence Rule**: For every Gate, attach the actual command + output snippet in a `gate-evidence-{X}` block in scan.md. "✅ Gate passed" without evidence is invalid.
- Gate 0: Artifact Inventory table with expected vs actual paths + sizes.
- Gate A: coverage-extract.py command line and stdout coverage summary.
- Gate B: 5-row behavior-contract table with 8 fields per function.
- Gate D: 5 P0 checklist grep/Read results (command + output).
- Gate D-6: structure.md path + 12-section review table.
- Gate E: `rg "fn {name}"` for each test function.
- Gate G: VERIFY-CHECK.md path + sampling consistency percentage.

**Evidence strength**: L1 (tool/grep output) required for A/D/E; L1 or L2 for B/C; L3 inference = FAIL unless `MANUAL_FALLBACK` justified.

**深度模式 rounds 动态化**（新增，2026-07-16）：根据文档行数决定分阶段轮次，避免小文档过度分阶段、大文档一轮过载。

| 文档行数 | rounds 数 | 每 round 范围 | 理由 |
|---------|----------|--------------|------|
| < 500 行 | 1 round（全量） | Step 0-7 一轮完成 | 小文档单轮可完成，无需分阶段 |
| 500-1500 行 | 2 rounds | R1: 正确性（Step 0-4 + Gate 0/A/B/C/D/D-6/E/H）<br>R2: 卓越性（Step 5 + Gate G + patterns/excellence） | 中等文档分两轮：先保正确性，再求卓越 |
| > 1500 行 | 4 rounds | R1: 正确性（Step 0-4）<br>R2: 卓越性（Step 5 + excellence）<br>R3: patterns 对照<br>R4: 跨文档 + Gate G 收敛 | 大文档需 4 轮，避免单轮 context 过载 |

**判定规则**：默认按行数查表；用户明确要求"深度全面 full-review"时按 2 rounds 起步（不强制 4 rounds），避免过度分阶段。

## Skill Invocation Log (mandatory in scan.md)
```
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Step 3 | §6 概念准确性表 |
```
Missing this section → scan.md marked DRAFT.

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

## Step 0: Scope Declaration + State Recovery
1. **Read correct STATE.md path** (tool-isolated, never share intermediate results between tools):
   - **Trae IDE** → `.review/trae/{module}/STATE.md` (project root `.review/`)
   - **Claude Code Runtime** → `.review/claude/{module}/STATE.md` (project root `.review/`)
   - **Codex CLI** → `.review/codex/{module}/STATE.md` (project root `.review/`)
   - If the **same tool** has conflicting STATE.md copies, **do not auto-merge**. Log divergence in scan.md and ask user which is authoritative.
   - **`{module}` resolution**: use the **first directory under `notes/rewrite/`** in the target doc path. E.g. `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/03-kmain-cstart.md` → `{module}=fork-syscall-rewrite`. This is separate from the coverage script's `--module kernel` (Minix3 module name); do not mix them.
   - **`{doc-stem}`** = target doc basename without extension (e.g. `03-kmain-cstart`). **`{agent}`** = model id (Trae: glm/kimi/...; Claude: m3/...).
   - **STATE preflight**: run `tools/review-state-validate.py --state {state_path}` to verify referenced files exist and Open issues map to scan.md entries.
    - **Auto-init**: use `tools/review-init.sh <tool> {doc-path}` to compute `{module}`/`{doc-stem}` and create standard directories.
2. Output:
```
- Target: <file.md> + <file.rs> (if code review)
- Mode: doc / code / full
- Same-dir docs: <list>
- Estimated time: <N> min (scale: 100 lines = ~10 min) OR "omitted, rely on Blocker Gates + VERIFY-CHECK anti-laziness"
- STATE.md: exists → done [X,Y,Z], pending [A,B,C] / N/A
```

**分阶段时间预算**（新增，2026-07-16）：若执行 Step 0.7 TODO 验证，时间预算分两阶段声明：
- **TODO 验证阶段**：按 TODO 数 × 5 分钟预估（含 grep 验证 + 误报否定 + 真实修复）
- **review 阶段**：按文档行数查表（<500 行→15-30 min | 500-1000→30-60 | 1000-1500→60-90 | >1500→90-120）
- 实际耗时与预估对比写入 scan.md，偏差 >50% 需说明原因（避免偷懒）

## Step 1: C Source Verification
- `ls minix3/minix/servers/<module>/*.c`
- For each `.c` reference in the doc, verify file exists and line numbers are correct.
Output: | File | Doc Reference | Exists? | Line Range |

### Step 1.0 锚点解析（符号锚点校验，anchor-resolve；2026-09-18 D5 改写收编原 1.0a~1.0g）

> **背景**：原 1.0a~1.0g 是七步行号/锚点族检查（行号抽样、自动化行号脚本 Proposal #7/#13、Rust idiom 同步、路径约定、"参见"范围、注释 doc 归属、注释行号漂移、forward reference）。第四轮裁定：**行号是 AI 的校验脚手架，不是目标**——漂移问题用符号锚点结构性消灭，机械项降级为卫生项，本节由七步收编为一步（原 `tools/review-line-check.sh` 行号自动化方案作废）。
> **职责划分**：符号锚点解析与存在性 → 本步（`tools/anchor-resolve.sh`）；路径约定（`os/` 前缀）→ review-doc-checklist §2.4c（模式 74）；文档 Rust 代码块 idiom（`static mut` 等）→ review-doc-checklist §2.4b（模式 73）+ 文档 Rust 代码块逐块审查（`tools/doc-snippet-extract.sh`）；代码注释 doc 归属 → 模式 76（按卫生项处理）；forward reference 透明声明核对保留在 §2.2 引用验证中执行。

**锚点约定**（写入正式文档与中间产物）：

| 对象 | 锚点形态 | 例子 |
|---|---|---|
| Rust 函数/类型/trait/常量 | `path:fn name`、`path:struct Name`、`path:enum Name`、`path:trait Name`、`path:const NAME` | `os/kernel/src/clock.rs:fn clock_init` |
| 同名方法歧义 | 加 impl 限定 | `os/kernel/src/proc.rs:impl Proc::new` |
| C 函数/结构体/宏 | `path:func`、`path:struct name`、`path:NAME` | `minix3/minix/kernel/proc.c:proc_init` |
| 函数内部的具体片段 | 函数符号 + 引文片段 | `os/kernel/src/proc.rs:fn do_fork（match 分支）` |
| 行号（可选） | 工具派生，写成 `（L123，工具生成）`；**禁止手工维护** | — |

**执行**：
```bash
tools/anchor-resolve.sh --check {doc}.md        # 校验文档内全部符号锚点（fenced 代码块不参与）
tools/anchor-migrate.sh [--write] {doc|dir}     # 旧行号锚点一次性迁移（默认 dry-run；工具不猜，无法解析列清单）
```

**判定**：
- 符号锚点解析 **0 定义** → **P0-fact**（符号消失/改名，引用断言已失效）
- **多定义** → 要求文档补 `impl Type::method` 限定；工具不加特例
- 残留的手工行号锚点、`（Lnnn，工具生成）` 提示过期、全角/半角、拼写、格式 → **卫生项**：scan.md 单列"卫生项"分区，只记录 + 批量修，**不进 P1/P2 计数、不进 `weighted_new`、不参与 Step 7.1 收敛判定**（唯一例外：符号锚点 0 定义按 P0-fact 处理——符号消失属正确性，不属卫生）

**一次性迁移基线（2026-09-18 已完成）**：全仓正式文档中 4,701 处可机械恢复的行号锚点已转为符号锚点（316 个文件，行数零变化）；无法机械恢复的保留原样并登记清单（`.review/anchor-migration-unresolved.txt`），按卫生项批量清理，不阻塞 review、不计入收敛。


- **关联代码清单（Step 0 产物，2026-09-18 B3.2）**：`§Step 0: 预检结果` 段（Gate 0 锚段之一）内必须含**"关联代码清单"子小节**——粘贴 `tools/doc-code-map.sh {doc}.md --check` 的输出表格，并给结论"共 N 个文件；存在性全通过 / 缺失列表"；无关联代码写"无（豁免/理由）"。full-review 入口强制；纯 design/概念文档可 N/A + 理由。
## Step 0.5: structure.md Skeleton Review — Gate D-6 (Doc Review Only)

> **Purpose**: reviewer 必须先提取文档骨架并评审，再执行正确性检查。正确性检查验证"文档说了什么"，structure.md 验证"读者读到了什么"。两者正交。
> **Precondition**: Only for doc review (`.md` files). Skip for pure code review.
> **Template**: `prompt/skill/review-process-skill.md` §Step 0.5.

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
11. **读者复述测试（必答产物，H1 提级 2026-09-18）** — 审阅者自己写一段不看代码、不用函数名的机制语言复述（结论进 structure.md §11）；说不出"解决什么矛盾、状态怎么转、边界在哪" → 教学类 P1（新增/触碰章节必须修复或写明延期理由，存量登记；scan.md 单列）
12. **纵向链路映射** — Ch1 concept ↔ Ch2 C code ↔ Ch3 design ↔ Ch4 impl; broken link → P1

**Step 0.5.2 评审 structure.md**（逐项判定，失败项写入 scan.md §structure.md 评审）：
- 1-12 节每节判定 ✅/P0/P1/P2 + 证据
- 失败项汇总为 Issue List 的 P0/P1/P2 条目
- **元注释章节 review**（NEW 2026-07-31）：同时主动验证文档中的元注释章节（H2/H3 标题含"已知"/"修订"/"元注释"/"自审"/"修复记录"等关键词），验证章节声称的文件/函数/行号引用 → 错误按 Pattern #66/#73/#75 记录到 Issue List（首次发现：04-platform-discovery review 2026-07-31）
Step 0.5.4 通过门槛：structure.md 评审通过后才进入 Step 1（覆盖率穷举）

**Step 0.5.5 跨章节重复内容一致性检查**（新增，2026-07-16）：
> **目的**：同一文档内多个章节描述同一事时（如 §3.5 和 §4.3 都列三架构差异表），必须保证内容一致。
> **触发场景**：02-higher-half-kernel.md review 发现 §3.5 和 §4.3 都列三架构差异表，但寄存器名矛盾。
> **扩展（2026-07-17，模式 72 CSSCM）**：步骤数对齐——§2 C 分析列 N 步、§4 Rust 实现 M 步时，若 M ≠ N 必须在 §4 末尾添加"与 C N 步的差异说明"表（分类：架构演进/设计决策/已知缺口/C bug + 附 C 行号）。检查命令：`rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" {doc}`。

**一致性矩阵格式**（必须输出）：
| 主题 | 章节 A | 章节 B | 一致? | 不一致字段 | 严重度 |
|------|-------|-------|------|----------|--------|
| 三架构寄存器名 | §3.5 L276 | §4.3 L660 | ❌ | aarch64 栈切换寄存器 | P1 |

**判定**：发现不一致 → 按 Issue List 严重度记录（P0/P1/P2）。同一文档内重复描述同一事必须保持一致，否则违反"事实唯一性"原则。

Output: structure.md path + 12-section review table + **跨章节一致性矩阵**（Step 0.5.5 产物） + failures in Issue List.

> ⛔ **Gate D-6**: structure.md generated + 12-section review table complete + failures in Issue List. Failure → scan.md DRAFT.

## Step 0.7: TODO 验证（若输入含 TODO 清单，新增，2026-07-16）

### Step 0.7.1 外部 TODO 验证

> 当 review 输入包含外部 TODO 清单时执行（历史形态为 `tmp_design_and_todo/` 下文件——该临时目录已删除；当前形态为各 stage 的 `todo.md` / `{NN}-todo.md` / `draft/` 产物）。

> **目的**：当 review 输入包含外部 TODO 清单时（历史案例如 `0108-todo-final.md`），TODO 验证是 review 的前置步骤，不是独立任务。TODO 验证结果直接喂入 Step 2 差异提取，避免二次 grep。

**执行步骤**：
1. 逐个验证 TODO 真实性（grep/glob/read 交叉验证）
2. 分类处理：
   - ❌ **误报**：grep 无匹配或位置不符 → 标注"误报原因"，不进入 Step 2
   - ⚠️ **真实（代码任务）**：grep 验证存在，但属于代码修改 → 标注"代码任务"，进入 Step 6 Action Item
   - ✅ **真实（文档任务）**：grep 验证存在，属于文档修改 → 标注"文档任务"，进入 Step 2 差异提取
3. 修复真实 TODO（按 review 流程，不是单独修复）
4. 喂入 Step 2：所有真实 TODO（代码任务 + 文档任务）作为 Step 2 差异提取的输入

**中间产物**：
```
| TODO ID | 描述 | grep 验证 | 分类 | 喂入 Step 2? |
|---------|------|----------|------|-------------|
| T1 | 修复 §3.5 寄存器名 | ✅ 有匹配 | ✅ 真实（文档任务） | 是 |
| T2 | 实现某 trait | ✅ 有匹配 | ⚠️ 真实（代码任务） | 否（进入 Step 6） |
| T3 | 删除某过时 TODO 标记 | ❌ 无匹配 | ❌ 误报 | 否 |
```

### Step 0.7.2 文档内部 TODO 扫描（NEW 2026-07-30）

> **背景**：原 Step 0.7 仅针对外部 TODO 清单（历史形态为 `tmp_design_and_todo/` 下文件，该临时目录已删除），但文档正文常含 **> **TODO** 内部标记**——这些未走任何 review 流程，可能累积成 doc drift。

**执行**：
1. 扫描 doc 内部 TODO：`rg "^\s*>\s*\*\*TODO" notes/rewrite/{module}/{stage}/{doc}.md` 或 `rg "TODO（" notes/rewrite/{module}/{stage}/{doc}.md`
2. 对每个 TODO 标记：
   - 描述类型：跨文档去重 / 测试缺口 / 设计缺口 / 表达式修正 / 其他
   - 分类：✅ 真实（文档任务）/ ⚠️ 真实（代码任务）/ ❌ 误报（已实施但未删除标记）
3. 喂入 Step 2：所有真实 TODO 作为 Step 2 差异提取输入

**中间产物**：
```
| TODO ID | 描述 | grep 验证 | 分类 | 喂入 Step 2? |
|---------|------|----------|------|-------------|
| T-D1 | 跨文档去重 §5.1 测试表 | ✅ 有匹配 | ✅ 真实（文档任务） | 是 |
| T-D2 | 补 QEMU + OpenSBI 真实集成测试 | ✅ 有匹配 | ⚠️ 真实（测试任务） | 是 |
```

**关联**：本次 review (01-boot-shim-bootstrap 2026-07-30) 发现 doc 内 2 处 TODO 标记未走流程。

## Step 1.5: Coverage Enumeration — Gate A
Run coverage-extract.py with full args. `{minix3-module}` = Minix3 module name (vm/pm/kernel/...); `{rw-module}` = rewrite module name (first dir under `notes/rewrite/`). **Claude Code Runtime output paths are hardcoded to `.review/claude/` — do NOT use a `{tool}` variable.**
```bash
# Module-level (servers: vm / pm / vfs / rs / ds / inet ...)
python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
  --output .review/claude/{rw-module}/scans/SYMBOLS.md

# Module-level (kernel)
python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
  --rust-dir os --c-dir minix3/minix/kernel \
  --output .review/claude/{rw-module}/scans/SYMBOLS.md

# Doc-specific review (recommended)
python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
  --output .review/claude/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

# Doc-specific review (kernel)
python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
  --rust-dir os --c-dir minix3/minix/kernel \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --output .review/claude/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
```
> **Gate A evidence rule**: After running, write the command + stdout into a `gate-evidence-A` block in scan.md. Physical unavailability of the script → mark PARTIAL (≠ PASS), no Final Review.
> **Directory creation**: The script auto-creates parent directories when `--output` is used.
- `--rust-dir os` scans the entire `os/` tree to avoid missing cross-crate symbols (e.g. `kmain`, `ProtectionArch`).
- `--c-dir` must be `minix3/minix/servers/{module}` for server modules and `minix3/minix/kernel` for the kernel module.
- `--semantic-map` is required for C→Rust rewrite projects; without it Rust coverage will be near 0% due to name mismatch.
- `--doc-file` ensures two different docs in the same module do not produce identical coverage numbers.
- If Rust coverage is 0%, first check `--rust-dir`/`--semantic-map` correctness before treating it as a real gap.

AI supplements 5 judgments (Rust corr / ARCH / semantic ownership / behavior contract / test coverage).
Output: SYMBOLS.md path + P0 gaps + ARCH marks.

## Step 1.6: Design Alignment Check — Gate H

> **Precondition**: 所有 review 模式都执行。Profile D/A/G 可以裁剪内容检查，但不能跳过 Step 0 预检或 Gate H。
> **Purpose**: 验证 review 对象（doc/code）与 design 的一致性 + design 本身的完整性 + 可实现性。
> **Note**: design 存在性预检已在 Step 0 完成（见下方 Step 0 design 预检）。此处为正式一致性检查。

**Step 0 design + outline 预检**（**所有 review 模式强制，2026-07-16 扩；原"Profile R/C/I/H-K only"已废除**）：前移自 Step 1.6，**v2 可复用快照语义**：
```bash
# 可复用快照（每次 review 重新评估，保留历史版本，不覆盖）
ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md        # doc 结构快照（v1, v2, ...）
ls notes/rewrite/{module}/{stage}/.design/{NN}-outline-review.v*.md # outline 评审快照
ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md         # 非 bagging code 设计快照
ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md   # bagging
# 跨文档查 design 统一入口（tools/design-index-update.sh 自动生成）
cat notes/rewrite/{module}/{stage}/.design/DESIGN-INDEX.md           # 最新版本锚点（软链为可选）
# 工具支持（NEW，Session #12 落地）
tools/design-coverage-check.sh {module} [--stage {stage}]           # 自动扫描所有 stage 缺失报告
```
- **存在旧快照** → AI 读旧快照作为"前人理解"参考输入，**但必须重新执行** Step 0.3.1-0.3.4 从 C 源码独立推导，产出 `.v{N+1}.md`。旧快照的角色是"语义参考 + 对照对象 + 反面教材"，**不是 ground truth**。
- **无旧快照** → 首次走 Step 0.3 流程，产出 `.v1.md`。
- **⛔ 禁止"复用其他文档 design"判定**：每篇文档必须有**本编号**的快照（`{NN}-outline.v*.md` / `{NN}-outline-review.v*.md` / `{NN}-design.v*.md`，`{NN}` = 本文档编号，如 `02`）。不允许"02 文档复用 01-design.md"——快照是 per-doc 的，不是 per-module 或 per-stage 的。违反 → P0-process-violation
- **Forbidden snapshot sources** (P0-process-violation): `tmp_design_and_todo/`, `/tmp/`, doc §3 inline design (**circular argument**: §3 is review target, cannot be snapshot source), unprefixed `design.md`/`outline.md` (must have `{NN}-` prefix), **其他编号文档的快照**（如 02 文档复用 01-design.md）——每篇文档必须独立，禁止跨文档复用，**把旧快照当作 ground truth**（快照是 input，不是 output）。
- **v2 产物区分**（方案 D 演进）：**可复用快照**（outline / outline-review / design）每次 review **重新执行 Step 0.3 流程**产出新版本（`.v{N+1}.md`），**保留所有历史版本**不覆盖——旧快照作为参考输入，新快照作为该轮 review 的依据。**脚手架产物**（structure / design-structure / SYMBOLS / VERIFY-CHECK）每次 review 重新生成，不复用。

### ⛔ Step 0 硬阻断规则（所有 review 模式强制，NEW 2026-07-16，模式 69 + 71 配套）

> **背景**：Session #12 (06-proc-init-boot-proc) 复盘发现 — 即使 Step 0 已写"design 预检强制"，AI 仍会因"已有 CONVERGED 状态"/"incremental review"等理由**错误跳过预检**。Session #11 模式 69 (PSMD) 发现 04/05 缺快照时已记录此为 P0-process-violation，但缺少硬阻断机制。

**判定**（强制）：
1. **必须跑 4 条 `ls` + 工具扫描**：每次 Step 0 启动时**必须**执行 4 条 `ls` + `tools/design-coverage-check.sh {module}`（无论何种 review 模式）。
2. **缺失判定 + 嵌入生成（NEW 2026-07-17）**：
   - `outline.v*.md` 缺失 → **Gate H.6 FAIL** → **执行 Step 0.3.2 生成**（不中断 review）
   - `outline-review.v*.md` 缺失 → **Gate H.6 FAIL**（⚠️ 2026-07-17 从 WARN 升级，根因：原 WARN 导致 AI 总跳过 outline-review）→ **执行 Step 0.3.3 生成**（AI 自审，不需用户确认）
   - `design.v*.md` 缺失 → **Gate H.1 FAIL** → **执行 Step 0.3.4 生成**（不中断 review）
   - **核心变更**：原"缺失 → 阻断 + 触发附录 C（中断）"改为"缺失 → Step 0.3 嵌入生成 → 继续 review"
   - **不允许**以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"等理由跳过 — 这些都是模式 69 (PSMD) 触发的 P0-process-violation
3. **存在旧快照时**：仍必须执行 v2 评估（重新执行 Step 0.3 产出 `.v{N+1}.md`）。旧快照的角色仅是"语义参考 + 对照对象 + 反面教材"，**不是 ground truth**。
4. **scan.md 必须含 `§Step 0: 预检结果` 段**（Gate 0 锚段，9 个之一；该段必须含**关联代码清单**子小节（B4.2 2026-09-18））：
   ```markdown
   ## Step 0: 预检结果（design + outline 完整性，NEW 2026-07-16）
   | 检查项 | ls 命令 | 结果 | 判定 |
   |--------|---------|------|------|
   | outline 快照 | `ls .design/{NN}-outline.v*.md` | `06-outline.v1.md` ✅ | ✅ 存在（旧版作参考，Step 0.3.2 重新评估） |
   | outline-review 快照 | `ls .design/{NN}-outline-review.v*.md` | （无）| ❌ **缺失 → Gate H.6 FAIL → Step 0.3.3 生成** |
   | design 快照 | `ls .design/{NN}-design.v*.md` | （无）| ❌ **缺失 → Gate H.1 FAIL → Step 0.3.4 生成** |
   ```
5. **决策记录豁免**（仅限一次性用户明确豁免）：Session #11 用户决策"04/05 不回填"**仅适用于当时已 CONVERGED 的 04/05**，**不可泛化**到 06/07/08/...（**模式 71 DOG 触发**）。豁免必须登记在 STATE.md `§豁免列表` 段。

### TODO Staleness Check（NEW 2026-07-16，模式 70 CTOS 配套）

> 当 review 输入包含外部 TODO 清单（当前形态：各 stage 的 todo.md / {NN}-todo.md；历史案例如 tmp_design_and_todo/ 下文件，该临时目录已删除），且 TODO 数 > 5 或含"基于..."/"依赖..."等时间敏感词 → **必须先跑 staleness check**（Step 0.7.4）。Session #12 实测：8 个 TODO-06 中 3 个 (37.5%) 是误报。
>
> **详见**：[prompt/review-rules/review-process.md §Step 0.7.4 TODO Staleness Check](../../prompt/review-rules/review-process.md) + [prompt/review-rules/review-patterns.md 模式 70](../../prompt/review-rules/review-patterns.md)。
- **命名区分**：`-structure.md` = review 骨架（Step 0.5 产物，12 节）；`-design-structure.md` = design 前序（Step 0.3.1 产物，知识点全集，脚手架）。两者内容完全不同，禁止混淆。`{NN}-outline.v{N}.md` 是可复用快照（带版本号），不是脚手架。
- **Step 0.5.3 doc ↔ outline 对齐检查**（方案 D 新增）：`outline.v{N}.md` 存在时，对照**最新版本快照**检查文档正文偏离（遗漏/多余/顺序错位），输出偏离矩阵。P0 偏离 = 核心概念遗漏。快照缺失 → Gate H.6 FAIL。

**偏离类型**：遗漏（坏）/ 多余（需评估）/ 顺序错位（需评估）
**严重度**：P0（核心概念遗漏）/ P1（非核心遗漏或坏偏离多余）/ P2（好偏离多余或轻微错位）

#### Step 0.5.6 6 维反查矩阵（新增，2026-07-16）

> **目的**：把"反查"作为 Step 0.5 的标准化方法。**超越原 Step 0.5.3 的 outline ↔ 文档单维检查**。每次 review 必跑 6 个维度。

**6 维反查维度**：

| # | 维度 | 工具 | 典型偏离 |
|---|------|------|---------|
| **1** | outline ↔ 文档正文 | grep + Read | 遗漏/多余/顺序错位 |
| **2** | design ↔ Rust 代码 | rg + Read | trait 未实现/签名偏移 |
| **3** | design ↔ Minix3 C 源码 | rg + Read | C 行为遗漏/Rust 行为偏移 |
| **4** | outline ↔ design | Read + diff | 概念命名不一致 |
| **5** | 文档 ↔ 代码（横向）| rg + Read | 注释与文档矛盾 |
| **6** | 元层反查 | diff + Read | 跨文档契约违反/修了又出 |

**每条偏离标注三类判定**：🔴 直接判定（P0/P1/P2）/ 🟡 Open Question（❓ OQ-N）/ 🟢 共识一致（✅）。

**反查覆盖率**：6/6 必须全输出，缺任一 → Step 0.5.6 FAIL。

#### Step 0.5.7 章节意图分析（新增，2026-07-16）

> **目的**：对"多余章节"分析其教学意图——"试图向读者强调什么 / 试图解释什么"。
> **格式**：每个多余章节必填「强调/解释/对齐/判定」4 项。

#### Step 0.5.8 issue 反查来源标注（新增，2026-07-16）

> **目的**：scan.md 中每条 issue 必须标注反查维度来源（维度 1-6）。
> **禁止**：无反查来源标注的 issue → 严重度降级。

### ⛔ 反查原则：不必然导向统一结果，可包含 Open Questions（2026-07-16）

> **核心原则**：review 的最终产出不一定非得是一个确定的结果，也可以包含待讨论项（Open Question）。适用所有反查维度（outline ↔ 文档、design ↔ code、doc ↔ code、跨快照 diff 等）。

**三类判定（替代"二值 P0/P1/P2 判定"）**：

| 判定类型 | 含义 | 处理方式 |
|---------|------|---------|
| **🔴 直接判定** | AI 能直接判定哪个更好 | AI 给出推荐 + 理由 + 严重度（P0/P1/P2），写入 scan.md |
| **🟡 Open Question** | AI 无法直接判定（设计权衡 / 命名美学 / 风格选择）| 标 ❓ OQ-N，写入 scan.md `§Open Questions` 段，**上交用户决定**，不阻塞 review |
| **🟢 共识一致** | 反查双方无差异 | 写入 scan.md `§反查一致项` 段，无需处理 |

**Open Question 格式**：
```
| OQ ID | 反查维度 | 两侧方案 | 各自优劣 | AI 倾向（可选） |
|-------|---------|---------|---------|----------------|
| OQ-1 | design ↔ code | design 用 `trait Foo`，code 用 `struct Foo` | trait 更可测 / struct 更简单 | 倾向 trait |
```

**Open Question 收敛机制**：
- 用户回复决定后 → OQ 转为 issue 进入修复流程
- 累计 ≥3 轮未决 OQ → STATE.md 标记"决策阻塞"
- 同一 OQ 跨多轮 review 仍开放 → 升级为"设计争议"

**反"无脑一致"原则**：
- ❌ 禁止"design ↔ Rust 代码不一致 → 直接判 P0 改 code"——必须先判断哪个更好
- ❌ 禁止"doc 和 outline-review.md 不一致 → 直接判 P0 改 doc"——必须先判断哪个更好
- ✅ 若 AI 能判定哪个更好 → 给出推荐 + 理由 + 严重度
- ✅ 若 AI 无法判定 → 标 OQ，上交用户
- ✅ review 报告同时含 issue 清单（确定项）和 OQ 清单（待决项），两者独立

Step 1.6.1 design 存在性确认（Step 0 预检的复核）：
```bash
ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md       # 非 bagging（持久化可复用快照，任意版本命中）
ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md # bagging
```
> **命名规则**：非 bagging 场景产物为 `{NN}-design.v{N}.md`；bagging 场景产物为 `{NN}-design-final.v{N}.md`。两者均需 `{NN}-` 前缀。位置在 `notes/rewrite/{module}/{stage}/.design/`。

Step 1.6.2 design 对齐检查（6 项）：
1. doc/code 中的命名是否与 design 一致？（无 "我用了更合理的命名"）
2. doc/code 中的 trait 定义是否在 design 中有完整方法签名？
3. doc/code 中的错误码策略是否与 design 一致？
4. doc/code 中的不变量是否在 design 中有显式声明？
5. doc/code 中的架构演进是否标注 ARCH？
6. doc/code 中的 unsafe 边界是否与 design 中的安全论证一致？

Step 1.6.3 design 缺口清单（P0-design-missing）：
- 列出所有 "design 缺但 doc/code 需要" 的项
- 每项必须：(a) 给出补充 design 的章节引用，或 (b) 标记 IN_DESIGN 进入 Review 中断协议

Step 1.6.4 IN_DESIGN 状态（替代 DEFERRED 逃避）：
- IN_DESIGN = 主动承认需要先 design，非逃避
- 时间上限：7 天警告、30 天清理
- 月度审计：清理过期 IN_DESIGN 项

Output: gate-evidence-H 块（命令 + 检查表 + 缺口清单 + IN_DESIGN 状态）。
> ⛔ **Gate H**: H.1 design.md/design-final.md 存在 + 6 项检查 + 缺口清单 + P0-design-missing 全处置。Failure → scan.md DRAFT。

## Step 2: Diff Extraction — Gate B
Identify **Top 5** divergences: 3 语义偏移 + 2 覆盖缺口 (from SYMBOLS.md).
Fill 8-field behavior contract table per function (see core-semantics §2.2).
Output: | # | Point | C Behavior | Doc/Code Description | Severity |

**差异类型区分**（新增，2026-07-16）：Gate B 的 8 字段表区分两种差异类型：

| 差异类型 | 适用场景 | 8 字段表适配 |
|---------|---------|------------|
| **C→Rust 行为差异** | C 函数行为 vs Rust 实现行为 | 8 字段全适用（C 行为 / Rust 行为 / 差异类型 / 严重度 / C 证据 / Rust 证据 / Reviewer 备注 等） |
| **doc↔code 描述差异** | 文档声称 vs 代码实际 | "C 行为"字段改为"doc 声称"，C 证据字段为 N/A（仅保留 doc 引用 + Rust 证据） |

**示例**：
- C→Rust 行为差异：`anon_pagefault` refcount 处理 — C 在 refcount<2 时 return OK 泄漏内存（minix3/minix/servers/vm/anon.c:841），Rust 实现先判断再分配
- doc↔code 描述差异：02 §3.5 文档声称 aarch64 用 `mov sp, x0`，Rust 代码实际用 `mov sp, {stktop}`

## Step 3.5: Precision Check — Gate C
After Step 3 (Sanity Check), execute 5 meta-rules:
1. External knowledge marks — comments citing hardware/protocol claims
2. Universal interface purity — shared struct fields meaningful for ALL consumers?
3. Return value completeness — ignored returns documented as safe?
4. Resource lifecycle closure — every acquisition has release path or rationale?
5. Reason questionability — "because/avoid/for" comments hold in context?

Output: table of suspicious points flagged for human confirmation.

## Step 3.5a: Vertical Link Check (Doc Review Only)
Verify end-to-end traceability: Ch1 concept → Ch3 design decision → Ch4 implementation → Ch5 test.
1. Each Ch1 core concept → Ch3 has corresponding design decision? No → P1 (concept not landed)
2. Each Ch3 design decision → Ch4 has corresponding implementation? No → P1 (decision not implemented)
3. Each Ch4 core type/function → Ch5 test covers it? No → P1 (implementation not tested)
4. Each Ch5 test → traceable to Ch3 design decision? No → P2 (test without design basis)

Output: | Ch1 concept | Ch3 decision | Ch4 impl | Ch5 test | Link complete? |

## Step 3.5b: Causal Chain Sampling (Doc Review Only)
Sample 5-10 "why this design" explanations from Ch2 and verify each causal chain step.
1. Extract explanation (A→B→C→conclusion)
2. Verify each step with C semantics / ISA spec
3. Failure → P0 (pattern 48: causal chain fabrication)

Output: | Ch2 location | design explanation | causal chain | each step valid? | verdict |


## Step 3.6: 关联代码维度检查（文档 review 强制，2026-09-18 B3.3）

> full-review = "文档 + 关联 Rust 代码"一体审查（B3.1）。输入：Step 0 关联代码清单（`tools/doc-code-map.sh`）。
> 检查维度（至少）：code-checklist §1 Rewrite 质量 / §2 硬件抽象 / §4 执行模型 SMP / §8 命名 / §13 设计-代码一致性 / §14 C-Rust 语义对齐。
> gate-evidence-code 固定行："代码可读性增量"（`tools/code-style-lint.sh --diff`，G1/G4）+ "非法态封堵"（H2，code-excellence §16.6）+ "文档代码块审查"（H3，doc-checklist §2.4k）。
> gate-evidence-code 附加行："unsafe 审计"（D2）：`tools/unsafe-audit.sh --baseline tools/unsafe-baseline.txt --diff`——新增裸 unsafe = 0 才通过；块内写命令 / 裸 unsafe 总数 / 与基线差值 / 新增违规清单。
> 产物：维度×文件×结论×证据 表 + gate-evidence-code 块；无发现也要写明"已检查、无发现"；沿用既有 P0/P1/P2 判定。

## Step 4.5: Test Verification — Gate E
If doc has §5 (test section): extract each test function name, grep in rust_dir.
- `rg "fn {test_name}" {rust_dir} --type rust -n`
- Missing test → P0 (test missing)
Output: | §5 test name | grep cmd | result | verdict |

### Step 4.5a 测试数量偏差检查（NEW 2026-07-30）

> **背景**：doc §5 声称的测试总数与 `cargo test` 实际数量可能存在显著偏差。本次 doc 02 写 "约 110+ 个通过"，实际 464 passed（4.2 倍偏差）。

**执行**：
1. 从 doc §5 抽取测试数量声称：`rg "约 \d+|总计.*\d+|通过.*\d+ 个" {doc}.md`
2. 跑实际测试：`cargo test -p {crate} --manifest-path os/Cargo.toml --lib 2>&1 | rg "^test result"`
3. 比较声称数 vs 实际数
4. 计算偏差率：`|声称 - 实际| / 实际`

**判定**：
- 偏差率 ≤ 30%：✅ 接受（doc 简写）
- 偏差率 30-50%：⚠️/P2 轻度失精
- 偏差率 > 50%：❌/P2 **测试数量失精**（需更新）
- doc 完全无数量声称：✅ 不适用

**修复**（≤5 分钟）：
```bash
sed -i 's|约 110\+|464|g' {doc}.md  # 直接替换为实际数
# 或加注：截至 YYYY-MM-DD, N 个测试通过
```

**关联**：首次发现 02-higher-half-kernel review 2026-07-30。

### Step 4.5b 测试数量准确性机制（NEW 2026-07-31, Proposal #8）

> **背景**：3 次 review 累计发现 doc §5 测试数量全部 undercount——Doc 01: 4.2 倍（110+ vs 464）+ Doc 02: 1.6 倍（15+ vs 24）+ Doc 03: 1.8 倍（43 vs 120，已说明为子集）。**系统性 undercount** 需要机制化修复。

**根因分析**：
1. doc 作者只在 doc §5 列出"已知"测试，未跑 `cargo test` 统计实际数量
2. doc 写作与代码演化不同步——测试增加但 doc 未更新
3. "约 N+ 个" 这种近似写法本身易过时

**预防机制**（建议落地）：
1. **CI 钩子**：`tools/ci-doc-test-count.sh` 在 CI 中跑 `cargo test --lib` + 扫描 doc §5 测试名，输出 mismatch 警告
2. **doc 模板**：规定 doc §5 必须含 `> 截至 YYYY-MM-DD, cargo test 全通过 N 个（N + M 子集）` 格式
3. **测试名清单**：用 `rg "^\s*fn test_"` 自动生成测试名清单，避免人工列出

**修复模板**（强制格式）：
```markdown
### 5.X 单元测试（截至 YYYY-MM-DD）

- `cargo test -p {crate} --manifest-path os/Cargo.toml --lib`：**N 个通过**
- 本节列出与本模块直接相关的 M 个（子集）
- 完整测试清单：`rg "^\s*fn test_" os/{path}`
```

**关联**：3 次 review 累计发现。

### Step Double-check 关键 Finding 防误判（NEW 2026-07-31, Proposal #9）

> **背景**：03-kmain-cstart review scan 阶段 P2-10 是 misread（误报 `protect.c:217-221` 错位，实际 L515 已写正确的 `arch_proto.h:217-221`）。修复前二次验证发现误判，避免无效修改。

**规则**：
1. **修复前必验证**：每个 P1/P2 finding 在 Edit 前必须重新 grep 验证一次，避免 scan 阶段的误判
2. **验证步骤**：
   - 提取 finding 中声称的 doc 行号（如 `L513`）
   - 实际读取该行附近 5-10 行
   - 确认 doc 原文与 finding 描述一致
   - 若不一致 → finding 是 misread，标 N/A
3. **修复后必验证**：Edit 后再次 grep，确认 row 内容已正确

**关联**：首次发现 03-kmain-cstart review 2026-07-31（scan P2-10 是误判）。

## Step N: Progress Checklist
At the END of every session, output:
```
### Review Progress
- [✅] Step 0: Scope
- [✅] Step 1: C Source Verification
- [✅] Gate D-6: Step 0.5 structure.md Skeleton Review (doc review only)
- [✅] Step 0.5.5: 跨章节一致性检查 (新增, 2026-07-16)
- [✅] Step 0.7: TODO 验证 (若输入含 TODO 清单, 新增, 2026-07-16)
- [✅] Gate A: Step 1.5 Coverage Enumeration
- [✅] Gate B: Step 2 Diff Extraction (Top 5)
- [✅] Gate C: Step 3.5 Precision Check
- [✅] Step 3.5a: Vertical Link Check (doc review only)
- [✅] Step 3.5b: Causal Chain Sampling (doc review only)
- [✅] Gate D: P0 Mandatory Checklist (5 items)
- [✅] Gate E: Step 4.5 Test Verification (§5 = "测试/验证"章节标题。2026-08-15 修复 A-P1-2：触发条件为文档含 `^## §?5(\.|\s)|^# 5(\.|\s)|测试章节|验证章节` 任一标题模式；纯设计文档 / 局部 Review（仅 Ch1&2）跳过；文档无 §5 → "Gate E 不适用" 不阻断）
- [✅] Gate G: Step 5.6 VERIFY-CHECK produced and PASS
- [✅] Step 5.7: Rule Discovery (✅/❌ + draft if ✅)
- [✅] Step 7.1: Convergence Cost Warning assessment
- [ ] doc-00: claims-evidence
- [ ] doc-01: concept accuracy
...

```

## Step 5.5: State Write & Convergence
After ALL checks are done:
1. Write/update tool-specific STATE.md (tool-isolated, never share intermediate results):
   - **Trae IDE** → `.review/trae/{module}/STATE.md`
    - **Claude Code Runtime** → `.review/claude/{module}/STATE.md`
    - **Codex CLI** → `.review/codex/{module}/STATE.md`
2. **Sync new P0/P1/P2** from scan.md into STATE.md Open lists; move fixed issues to Closed Issues with scan/date.
3. **All dimension results → scan.md single file** (NOT 10 dimension check files)
   - Trae default: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
    - Claude default: `.review/claude/{module}/{doc-stem}/scan.md`
    - Codex default: `.review/codex/{module}/{doc-stem}/scan.md`
   - If user explicitly requests another output location, **dual-write**: user-specified path + tool default path (Trae interactive fix doc: `notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`; Claude report: `notes/rewrite/{module}/{stage}/{doc-stem}-claude-report.md`).
4. Update SYMBOLS.md (Step 1.5 machine output) to matching path:
   - Trae: `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
    - Claude: `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
    - Codex: `.review/codex/{module}/{doc-stem}/SYMBOLS.md`
5. Generate VERIFY-CHECK.md **before declaring CONVERGED**:
   - Trae: `.review/trae/{module}/VERIFY-CHECK.md`
    - Claude: `.review/claude/{module}/VERIFY-CHECK.md`
    - Codex: `.review/codex/{module}/VERIFY-CHECK.md`
   - **跨 agent 验证推荐**（新增，2026-07-16）：若条件允许，优先由**不同 agent** 执行 VERIFY-CHECK。同 agent 验证时必须基于 grep 命令重放（非语义回忆），并在 VERIFY-CHECK.md 中标注"同 agent 验证，已重放 grep 命令" + 验证局限说明段。
6. Output **Artifact Inventory** and **Severity Reconciliation** tables in scan.md (Gate 0 requirements).

STATE.md format:
```
# Review State: {module}
- **Phase**: [concept | ref | struct | coverage | design | link | code | cross-doc | claims | verify | complete]
- **Last completed**: <phase>
- **Open P0/P1/P2**: N/M/K
- **Convergence**: CONVERGED / NOT_CONVERGED (N phases left)
- **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G✅ **H✅** (all with evidence, 2026-08-15 修复 C-P0-2：Gate H 明确为 Blocker Gates 之一)
```

**Convergence criteria** (all must pass):
1. All dimensions marked COMPLETE in scan.md
2. P0 new = 0 in latest full pass
3. P1 new ≤ 1
4. **Gate G: VERIFY-CHECK.md = PASS** (mandatory; do NOT mark CONVERGED without it)
5. All P0 in scan.md fixed+verified (or WONTFIX+reason)
6. **Blocker Gates 0/A/B/C/D/D-6/E/G/H all passed with gate-evidence attached**

## Step 5.6: Review Verification Protocol（独立验证，强制，Gate G）

> **目的**：解决"自己审自己"的盲区。在所有维度 COMPLETE 后，必须执行独立验证才能标记 CONVERGED。

**触发条件**：所有维度标记 COMPLETE + P0/P1 收敛后，**必须**执行本步骤并生成 VERIFY-CHECK.md。未执行 VERIFY-CHECK 时，状态必须为 NOT_CONVERGED。

**执行方式**（独立会话中执行）：
1. Agent 读取 STATE.md + scan.md + 原始文档/代码
2. **分层抽样**（2026-08-15 修复 C-P1-5：AI 无内置"随机"，用分层抽样替代）：从 scan.md Issue List 中按 P0/P1/P2 比例各取头 20% + 尾 20% + 中间 20% 的已报告问题。例如 Issue List 共 20 条（P0=3, P1=12, P2=5）→ P0 取第 1 条 + 最后 1 条 = 2 条；P1 取第 1/6/12 条 = 3 条；P2 取第 1 条 = 1 条，合计 6 条（30%）。**禁止**仅抽 P0（应全层级覆盖）
3. **反向验证**：对每个抽样问题，独立重新验证——source evidence 是否充分？判定等级是否合理？
4. **遗漏检查**：抽样 20% 的源码符号（函数/结构体/宏），验证是否都在文档/检查中覆盖了
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有"标记 COMPLETE 但实际未完成"的维度
6. **Blocker Gates 复验**：检查 scan.md 中 Gate 0/A/B/C/D/D-6/E/G/H 是否都附带真实证据（gate-evidence 块）
7. **跨 agent 验证推荐**（新增，2026-07-16）：若条件允许，优先由**不同 agent** 执行 VERIFY-CHECK（如 trae 内 glm 的 scan 由 kimi/ds 验证）。同 agent 验证时必须基于 grep 命令重放（非语义回忆），降低同 agent 系统性盲区风险。跨 agent 验证结果记入 VERIFY-CHECK.md 的"验证局限说明"段。
8. **输出判定**：
   - **PASS**：抽样验证一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过
   - **CONCERN**：抽样验证一致性 70-90% → 特定维度需重新审查
   - **FAIL**：抽样验证一致性 < 70% 或发现关键遗漏 → 整体重新审查

**输出**：写入工具对应的 VERIFY-CHECK.md 路径（Trae: `.review/trae/{module}/VERIFY-CHECK.md`; Claude: `.review/claude/{module}/VERIFY-CHECK.md`）。VERIFY-CHECK.md 必须包含"验证局限说明"段，标注验证者（同 agent / 跨 agent）及验证方法（grep 重放 / 语义回忆）。

**Multi-Agent 强制规则（NEW 2026-07-16）**：

| 报告 P0 数 | 推荐验证 agent | 阻断? |
|-----------|---------------|-------|
| 0 | 同 agent 可（带 grep 重放） | ❌ 不阻断 |
| ≥ 1 | **必须跨 agent 验证** | ⛔ 阻断（不通过 Gate G） |

**触发场景**：
- 若 review 报告 P0 ≥ 1 → Gate G VERIFY-CHECK 必须由不同 agent 二次验证
- Trae 内允许：glm → kimi / glm → ds / kimi → seed 等轮换
- Claude 内允许：m3 review → glm-flash verify（独立 session）
- 同 agent 验证时必须基于 grep 命令重放（非语义回忆），并在 VERIFY-CHECK.md 标注"同 agent 验证，已重放 grep 命令"

**工具层触发**：
- `tools/review-init.sh --require-multi-agent` 启用（默认 false）
- 若启用，review-init 输出"⛔ 已设置 --require-multi-agent：若本 review 报告 P0 ≥ 1，Gate G 必须由不同 agent 执行 VERIFY-CHECK"

**判定**：报告 P0 ≥ 1 但 VERIFY-CHECK 由同 agent 写 → Gate G 判 FAIL → STATE.md 不得标 CONVERGED。

## Step 5.7: Rule Discovery (Mandatory)
After completing the review, answer in scan.md `§Rule Discovery`: "Did this review discover a new pattern? ✅/❌".
If ✅ (≥2 instances of the same new pattern not covered by existing rules):
- Propose new pattern: name / case (file:line) / severity (P0/P1/P2) / category (doc/code/cross-phase/excellence/narrative) / draft rule / target file
- Write to scan.md `§Rule Discovery` section
- After user confirmation, land in the corresponding rules file

This makes the rule set self-evolving — patterns discovered in one review feed back into the rules for the next review. See `prompt/skill/review-process-skill.md` §Step 5.7.

## Step 7.1: Convergence Cost Warning (Mandatory)
To prevent over-convergence (chasing P1→0 across many rounds at cost exceeding benefit), stop and deliver when ANY of these trigger:
1. **Round threshold**: same doc reviewed ≥5 rounds → force deliver, remaining P1/P2 → backlog
2. **P1 marginal decay**: two consecutive rounds with new P1 ≤ 1 → converged, remaining P1 → backlog
3. **Cost/benefit ratio** (2026-08-15 A-P1-3 weighted): current round cost >80% of previous but weighted new findings <20% → stop
   - **Weighted formula**: `weighted_new = P0_count * 10 + P1_count * 3 + P2_count * 1`
   - **First round exempt**: no previous-round baseline in round 1; this rule is skipped
   - **Decision**: stop when `weighted_new < previous_round_weighted_new * 0.2` AND current cost > previous cost * 0.8
4. **首次零发现（漏检自检，NEW 2026-08-14）**：首次 review 0 P0/P1/P2 → 触发**漏检自检**：随机抽 3 个检查项重跑（推荐：Gate D 第 1/3/5 项 + Step 2 因果链抽样）；若仍 0 发现 → 交付；若发现遗漏 → 之前的 review 标记 DRAFT，补完后再交付。
5. **卫生项排除（2026-09-18 D5）**：卫生项（锚点提示过期、全角/半角、拼写、格式等机械项）只记录 + 批量修，**不进 P1/P2 计数、不进 `weighted_new`、不参与本步收敛判定**；scan.md Issue List 单列"卫生项"分区。唯一例外：符号锚点 0 定义（符号消失/改名）按 P0-fact 处理——属正确性，不属卫生（见 Step 1.0）

Output in scan.md tail:
```
### 收敛成本评估
- 当前轮次: N
- 本轮新发现: P0=X, P1=Y, P2=Z
- 触发停止规则: [1/2/3/无]
- 决定: 继续收敛 / 强制交付（剩余转 backlog）
```

**Zero-bias milestone 案例（Doc 07 review, 2026-07-31）**：
- 7 次 review 中**首次** 0 P0/P1/P2（review 即 PASS）
- 一致性 100%（无需修复）
- **启示**：当 review 内容简单且无偏差时，应快速完成并交付，不必深挖
- **不要**误以为 "0 偏差 = review 走流程不深入"——累积改进效果极致也可能导致 0 偏差
- **⚠️ 规则更新（NEW 2026-08-14）**：上述"review 即 PASS，强制交付"已废弃，改为**漏检自检**（Step 7.1 规则 4）。未来首次 0 发现必须先抽 3 项重跑确认无遗漏，再交付。

**关联**：详见 `prompt/skill/review-process-skill.md §Step 7.1` + `prompt/skill/review-patterns-skill.md §模式 66 RCPD`（已被 doc 07 主动应用，避免引入漂移风险）。

### Step 7.1.1 修复成本估算（NEW 2026-07-30）

> **目的**：决定"立即修 vs 移入 backlog"时，需要量化每个 P1/P2 的修复成本。

**估算公式**：
```
总修复成本 = Σ (P1_i × cost_P1) + Σ (P2_i × cost_P2)
cost_P1 = 5 min  # 单个 P1 doc-code 修复
cost_P2 = 2 min  # 单个 P2 行号/表达式修正
```

**示例**：
- 3 P1 + 6 P2 → 3×5 + 6×2 = 27 min
- 0 P1 + 1 P2 → 0 + 2 = 2 min（立即修）
- 5 P1 + 10 P2 → 25 + 20 = 45 min（按修复成本排优先级）

**输出**（追加到 scan.md `### 收敛成本评估` 段）：
```
- 本轮新发现: P0=X, P1=Y, P2=Z
- 修复成本估算: ~N 分钟
- 优先级: 立即修 / 部分修+backlog / 全部 backlog
```

**决策矩阵**：

| 条件 | 决策 |
|------|------|
| 修复成本 ≤ 30 min + 全是 doc-code | 立即修（当前 session） |
| 修复成本 30-60 min + 含设计偏离 | 部分修 + backlog（OQ 上交） |
| 修复成本 > 60 min + 含架构决策 | 全部 backlog（仅记录） |
| 修复成本任意 + 含 P0 安全/内存 | **必须**立即修 |

**关联**：本次 review (01-boot-shim-bootstrap 2026-07-30) 修复成本 ~50 分钟（3 P1 + 6 P2），全部 doc-code 无安全/内存，按"立即修"决策。

---

## Fix Phase Workflow

When fixing issues found by review:

1. **Pre-fix**: Re-read STATE.md Open Issues and scan.md Issue List. Load relevant skills:
   - Code fixes → `review-code-skill` + `review-patterns-skill`
   - Doc fixes → `review-doc-skill` + `review-patterns-skill`
   - Core semantics → `review-core-semantics-skill`
   - Coverage/state → `review-process-skill` + `review-coverage-skill`
2. **Fix principles**:
   - Fix all P0 before P1/P2; do not mark CONVERGED with open P0.
   - Keep docs and code in sync; changes to Ch4 descriptions must update code, and vice versa.
   - No new violations: no_std, hardware-as-trait, SMP/BKL, Claims-Evidence.
   - Record evidence in scan.md/STATE.md: date, changed files, verification command output.
3. **Post-fix verification**:
   - `cargo test -p <crate>` passes.
   - `cargo check` has no new errors; new warnings need rationale.
   - Re-run affected Gate(s): Gate B for semantic fixes, Gate D/E for code/test fixes, Gate A/C for doc claim fixes.
   - Update STATE.md: move fixed issues to Closed Issues with scan/date.
- **文风增量门（2026-09-18 A3.2）**：文档类修复完成后，跑 `tools/doc-style-lint.sh --diff`；有 error 级命中则修复未完成——先把过程信息移到 scan.md / fix-status，再把正文改写为假设性推理或直接正确表述。
