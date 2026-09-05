---
name: review-scan
description: "Scan a notes/rewrite/ directory: coverage + doc + code + patterns + excellence checks, write a review report with convergence status. Use when user says review/scan/check a documentation directory."
---

# Review Scan — Orchestrator

You are a review orchestrator. Your job is to execute checks domain by domain, in strict order. Do NOT batch them mentally.

## ⛔ MANDATORY RULES (read before starting)

1. **Explicit Skill Invocation**: You MUST invoke Skill tools explicitly via the available `Skill` function. NEVER rely on "rules already loaded" or "context already has it". For each review domain (doc/code/patterns/process/core-semantics/excellence/coverage/socratic), invoke the corresponding Skill before executing its checks. The Skill Invocation Log in scan.md must reflect actual Skill tool calls.
2. **先读后判（Read-before-Judge）**：每个判定必须先执行 grep/read 验证，再下结论。禁止凭印象判断。
3. **Evidence 分级**：每个判定标注 [DIRECT/MEDIUM/INFERRED]。
   - [DIRECT] grep 直接验证（有明确输出）
   - [MEDIUM] 名称匹配但语义需进一步确认
   - [INFERRED] 基于上下文推断（需标注"待确认"）
4. **零输出禁令**：如果检查发现 0 个问题，必须写"Checked N items, found 0 issues"。禁止留空。
5. **证据强制**：每个检查必须包含 grep count、file count 或 line number range 作为证据。
6. **Attention Decay 警告**：每完成一个领域后暂停，重读规则 #1 和 #4。
7. **You MUST read each domain file before executing it.** Do NOT assume content.
8. **You MUST output the result of each domain BEFORE moving to the next.**

## ⛔ Blocker Gates (must pass before Final Review output)

| Gate | Check | Pass Criteria | Fail Consequence |
|------|-------|---------------|------------------|
| **0** | Artifact Inventory | Standard paths complete (STATE/scan/structure/SYMBOLS); scan.md contains 9 grep-verifiable anchor sections | DRAFT, no STATE.md write |
| **A** | Phase 2 Coverage Enumeration | coverage-extract.py executed + SYMBOLS.md on disk + `gate-evidence-A` block | DRAFT, no STATE.md write |
| **B** | Phase 7 Step 2 Diff Extraction | Top 5 behavior contract table (3 语义偏移 + 2 覆盖缺口, **8 fields × 5 funcs**) | DRAFT, no STATE.md write |
| **C** | Phase 7 Step 3.5 Precision Check | 5 meta-rules check table output | DRAFT, no STATE.md write |
| **D** | P0 Mandatory Checklist (patterns §0) | 5 items answered (✅/❌ + grep evidence); PARTIAL/⚠️/"部分通过" = ❌ FAIL | DRAFT, no STATE.md write |
| **D-6** | Phase 2.5 structure.md Skeleton Review (doc review only) | structure.md generated + 12-section review table + failures in Issue List | DRAFT, no STATE.md write (doc review) |
| **E** | Phase 7 Step 4.5 Test Verification | §5 each test function grep-verified (if doc has §5) | DRAFT, no STATE.md write |
| **G** | Phase 9 VERIFY-CHECK | VERIFY-CHECK.md produced + verdict PASS (consistency ≥ 90%) | DRAFT, NOT CONVERGED |
| **H** | Phase 2.6 Design Alignment Check | **所有 review 模式必检（2026-07-16 扩；原"仅完整/深度/设计优先"已废除）**：H.1 `design.md`（非 bagging）/ `design-final.md`（bagging）存在 + design 对齐检查 + design 缺口清单 + P0-design-missing 全处置 + **H.6 outline.md 存在 + doc↔outline 无 P0 偏离**。**不允许 N/A / [SIMPLIFIED] / "复用其他文档 design"**（**模式 69 PSMD 触发**） | DRAFT, no STATE.md write |

**Any Gate failed → scan.md marked DRAFT, STATE.md NOT updated.**

**⛔ Gate H 不允许 N/A 判定（2026-07-16 强化；2026-07-17 更新）**：每篇文档都必须通过 Gate H 全部 6 项检查。若本文档无专属 design.md / outline.md / outline-review.md，必须**执行 Step 0.3 嵌入生成**（不切换模式，不中断 review），不得标 N/A 跳过（违反 = P0-process-violation + 模式 69 PSMD 触发）。原"切换 Design-First 模式生成"已废除——Design-First 仅用于 design-wrong（design 存在但有错误），不用于缺失场景。

**Gate Evidence Rule**: For every Gate, attach the actual command + output snippet in a `gate-evidence-{X}` block in scan.md. "✅ Gate passed" without evidence is invalid. Evidence strength: L1 (tool/grep output) required for A/D/E; L1 or L2 for B/C; L3 inference = FAIL unless `MANUAL_FALLBACK` justified.

## Skill Invocation Log (mandatory in scan.md)
scan.md must include this section:
```
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Phase 3 | §6 概念准确性表 |
| 2 | review-code-skill | Phase 4 | 代码质量审查 |
```
Missing this section → scan.md marked DRAFT.

---

## Phase 1: Scope
1. `ls $ARGUMENTS` — list all `.md` and `.rs` files in the target directory
2. Output: "Found N .md files, M .rs files. Starting review."
3. **Read correct STATE.md path** (tool-isolated; never share intermediate results between tools):
   - **Codex CLI** → `.review/codex/$MODULE/STATE.md` (project root `.review/`)
   - If the **same tool** has conflicting STATE.md copies, **do not auto-merge**. Log divergence in scan.md and ask user which is authoritative.
   - `$MODULE` = first directory under `notes/rewrite/` in the target doc path. This is separate from the coverage script's `--module` argument (Minix3 module name); do not mix them.
   - `$DOC_STEM` = target doc basename without extension. Codex has no bagging agent suffix.
   - Use `tools/review-init.sh codex {doc-path}` to auto-compute paths and mkdir.
4. **⛔ Step 0 硬阻断预检（NEW 2026-07-16，所有 review 模式强制，模式 69 PSMD + 71 DOG 配套）**：
   - **必须跑 4 条 `ls`**（无论何种 review 模式）：
     ```bash
     ls notes/rewrite/$MODULE/$STAGE/.design/$NN-outline.v*.md
     ls notes/rewrite/$MODULE/$STAGE/.design/$NN-outline-review.v*.md
     ls notes/rewrite/$MODULE/$STAGE/.design/$NN-design.v*.md
     ls notes/rewrite/$MODULE/$STAGE/.design/$NN-design-final.v*.md  # bagging only
     ```
   - **必须跑工具扫描**：
     ```bash
     tools/design-coverage-check.sh $MODULE --stage $STAGE
     ```
   - **缺失判定 + 嵌入生成（2026-07-17）**：
     - `outline.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.2 嵌入生成**（不中断 review）
     - `outline-review.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.3 嵌入生成**（AI 自审）
     - `design.v*.md` 缺失 → **Gate H.1 FAIL** → **Step 0.3.4 嵌入生成**（不中断 review）
     - 不允许以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"为由跳过
   - **scan.md 必须含 `§Step 0: 预检结果` 段**（Gate 0 锚段，9 个之一，缺此段 → Gate 0 FAIL）
   - **豁免必须登记在 STATE.md `§豁免列表` 段，不可泛化**（模式 71 DOG）

---

## Phase 2: Coverage Enumeration (DO THIS FIRST, once for the whole directory)

> Derive from the user's target:
> - `$DOC_DIR` = the directory the user wants reviewed (e.g. `notes/rewrite/fork-syscall-rewrite/01-stage-kernel`)
> - `$MINIX3_MODULE` = the Minix3 module name (e.g. `kernel`, `vm`, `pm`, `vfs`)
> - `$C_DIR` = `minix3/minix/kernel` if `$MINIX3_MODULE == kernel`; otherwise `minix3/minix/servers/$MINIX3_MODULE`
> - `$TARGET_DOC` = basename if the user names a single `.md` file; otherwise leave empty for module-level coverage
> - **Output paths are hardcoded to `.review/codex/` — this is the Codex CLI Skill; do not write to Trae/Claude paths.**
>
> See `checks/process.md` §Step 1.5 for detailed coverage rules.

Execute the coverage-extract script to generate SYMBOLS.md. **Gate A evidence rule**: write command + stdout into a `gate-evidence-A` block in scan.md; script unavailable → PARTIAL (≠ PASS).
```bash
# Module-level
python3 tools/coverage-extract/coverage-extract.py $MINIX3_MODULE $DOC_DIR \
  --rust-dir os --c-dir $C_DIR \
  --output .review/codex/$MODULE/scans/SYMBOLS.md

# Doc-specific review (recommended when user names one doc)
python3 tools/coverage-extract/coverage-extract.py $MINIX3_MODULE $DOC_DIR \
  --rust-dir os --c-dir $C_DIR \
  --doc-file $TARGET_DOC \
  --semantic-map tools/coverage-extract/$MINIX3_MODULE-semantic-map.json \
   --output .review/codex/$MODULE/$DOC_STEM/SYMBOLS.md
```
- `--rust-dir os` scans the entire `os/` tree to avoid missing cross-crate symbols.
- `--c-dir` must be `minix3/minix/servers/$MINIX3_MODULE` for server modules and `minix3/minix/kernel` for the kernel module.
- `--semantic-map` is required for C→Rust rewrite; without it Rust coverage will be near 0% due to name mismatch.
- `--doc-file` ensures two docs in the same module do not produce identical coverage numbers.
- If Rust coverage is 0%, first check `--rust-dir`/`--semantic-map` correctness before treating it as a real gap.

Then AI supplements 5 semantic judgments (each with evidence level):
1. Rust correspondence confirmation
2. Architecture evolution marking (ARCH)
3. Semantic ownership determination
4. Behavior contract table (core functions)
5. Test coverage supplement

Output: SYMBOLS.md + coverage statistics + P0 gaps + ARCH marks.

---

## Phase 2.5: structure.md Skeleton Review (Doc Review Only, Gate D-6)

> **Purpose**: Verify "what the reader reads" (orthogonal to correctness which verifies "what the doc says"). Both must pass.
> **Precondition**: Only for doc review (`.md` files). Skip for pure code review.
> **Template**: `prompt/skill/review-process-skill.md` §Step 0.5.

Generate `structure.md` (12 sections, saved alongside scan.md):
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

Output: structure.md + 12-section review table + failures written to Issue List.

> ⛔ **Gate D-6**: structure.md generated + 12-section review table complete + failures in Issue List. Failure → scan.md DRAFT.

---

## Phase 2.6: Design + outline Alignment Check — Gate H

> **Precondition**: 所有 review 模式都执行。Profile D/A/G 可以裁剪内容检查，但不能跳过 Step 0 预检或 Gate H。
> **Purpose**: 验证 review 对象（doc/code）与 design 的一致性 + design 本身完整性 + 可实现性 + **outline ↔ doc 对齐**（方案 D 新增）。

Phase 2.6.1 design 对齐检查（6 项）：
1. doc/code 中的命名是否与 design 一致？
2. doc/code 中的 trait 定义是否在 design 中有完整方法签名？
3. doc/code 中的错误码策略是否与 design 一致？
4. doc/code 中的不变量是否在 design 中有显式声明？
5. doc/code 中的架构演进是否标注 ARCH？
6. doc/code 中的 unsafe 边界是否与 design 中的安全论证一致？

Phase 2.6.2 design 缺口清单（P0-design-missing）：
- 列出所有 "design 缺但 doc/code 需要" 的项
- 每项必须：(a) 给出补充 design 的章节引用，或 (b) 标记 IN_DESIGN 进入 Review 中断协议

Phase 2.6.3 IN_DESIGN 状态（替代 DEFERRED 逃避）：
- IN_DESIGN = 主动承认需要先 design，非逃避
- 时间上限：7 天警告、30 天清理
- 月度审计：清理过期 IN_DESIGN 项

Phase 2.6.4 outline 对齐检查（H.6，方案 D 新增）：
- 检查 `notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md` 是否存在（持久化可复用快照，任一版本命中）
- 对照 Step 0.5.3 的 outline 偏离矩阵，确认无 P0 偏离（核心概念遗漏）
- outline 快照缺失 → Gate H.6 FAIL，建议补生成 outline（走 Step 0.3.2-0.3.3）

Output: gate-evidence-H 块。
> ⛔ **Gate H**: 6 项 design 检查 + 缺口清单 + P0-design-missing 全处置 + **H.6 outline 对齐无 P0 偏离**。Failure → scan.md DRAFT。
> 详见 `prompt/review-rules/review-process.md` §Step 1.6 + `prompt/skill/review-patterns-skill.md` §X.5 Pattern 63 Design-Missing。

---

## Phase 3: Doc Checks (for each .md file)

Read: `.codex/skills/review-scan/checks/doc.md`

Execute 16 doc checks IN ORDER. After each check, mark it done.
- Check 00: Claims-Evidence Tracing
- Check 01: Concept Accuracy
- Check 02: C Code Reference Verification
- Check 03: Data Structure Coverage
- Check 04: Document-Code Consistency
- Check 05: Architecture Evolution
- Check 06: Cross References
- Check 07: Diagram Quality
- Check 08: C Source Coverage (enhanced by Phase 2 SYMBOLS.md)
- Check 09: Design Decision Quality
- Check 10: Chapter Link Validation
- Check 11: Document Style
- Check 12: Skip/Fake Check Detection (meta, run after all others)
- Check 13: Ch1 Mandatory Skeleton (concept-driven, not implementation-driven)
- Check 14: Explanation Causal Chain Validation (P0 — causal explanations must be technically correct)
- Check 15: Author Intent Transparency (Ch3 decisions must have rationale)

> ⛔ ATTENTION DECAY: After Check 06, pause and re-read MANDATORY RULES #1 and #3.

---

## Phase 4: Code Checks (for each .rs file)

Read: `.codex/skills/review-scan/checks/code.md`

Execute 16 code checks IN ORDER:
- Check 01: Rewrite Quality
- Check 02: Hardware Abstraction
- Check 03: Trait Design Quality
- Check 04: Type Safety
- Check 05: Execution Model & Concurrency
- Check 06: Memory Model
- Check 07: Module Design & pub Hygiene
- Check 08: Naming & Traceability
- Check 09: Testing
- Check 10: Comments & Documentation
- Check 11: 64-bit Assumptions
- Check 12: Complexity & Engineering Judgment
- Check 13: no_std Compliance
- Check 14: Design-Code Consistency
- Check 15: C-Rust Semantic Alignment
- Check 16: Precision Check

> ⛔ ATTENTION DECAY: Code checks are at HIGH RISK of being skipped. After Check 08, pause and re-read MANDATORY RULES #1 and #3.

---

## Phase 5: Pattern Checks (for each .md and .rs file)

Read: `.codex/skills/review-scan/checks/patterns.md`

Execute 9 pattern categories:
- §0 P0 必检清单（Gate D，5 项）
- 文档错误模式（15 个，1-15）
- 跨文档联动错误模式（3 个，A-C）
- 代码错误模式（10 基础 + 4 内核 SMP + 5 跨阶段通用，16-34）
- 测试错误模式（6 个，35-40）
- 卓越性错误模式（7 个，41-47）
- 叙事与概念错误模式（13 个，48-60）— 因果链编造(48)为 P0
- Design-First 反模式（63-65）
- Review 流程反模式（66-78）

---

## Phase 6: Excellence Checks (after correctness gate passed)

Read: `.codex/skills/review-scan/checks/excellence.md`

> **Precondition**: Only execute if Phase 2-5 found 0 new P0 and ≤1 new P1.
> Excellence checks pursue "better", not "correct".

Execute:
- §4.1-4.5 文档卓越性（叙事结构 / 读者体验 / 教学深度 / 可维护性 / 概念教学 Ch1 专项）
- §16-21 代码卓越性（API设计 / 表达力 / 性能 / 代码即文档 / 可测试性 / 测试质量）

---

## Phase 7: Process & Meta-Check

Read: `.codex/skills/review-scan/checks/process.md`

Execute:
- Step 0.5: structure.md Skeleton Review (Gate D-6, doc review only)
- Step 2: Diff Extraction (Top 5: 3 语义偏移 + 2 覆盖缺口, 8-field behavior contract table)
- Step 2.5: Link Validation (if applicable)
- Step 3.5: Precision Check (Gate C)
- Step 3.5a: Vertical Link Check (doc review only — Ch1 concept ↔ Ch3 design ↔ Ch4 impl ↔ Ch5 test)
- Step 3.5b: Causal Chain Sampling (doc review only — verify Ch2 "why this design" explanations)
- Step 4: Cross-Document Check
- **Step 4.5: Test Verification (Gate E)** — grep each §5 test function
- Step 5.5: State Write & Convergence
- Step 5.6: Review Verification Protocol (if converged)
- **Step 5.7: Rule Discovery (mandatory)** — answer ✅/❌ + draft if ✅
- **Step 7.1: Convergence Cost Warning (mandatory)** — stop if ≥5 rounds / P1≤1 for 2 rounds / cost>80% benefit<20%
- Step 6: Action Item Generation
- Step 7: Self-Check Checklist
- Meta-Check: Skip/Fake Check Detection

---

## Phase 8: Report

After ALL checks are done, collect findings into:
- File: tool-specific scan path
  - Codex default: `.review/codex/$MODULE/$DOC_STEM/scan.md`
  - If user explicitly requests another output location, **dual-write**: user-specified path + Codex default path (`notes/rewrite/$MODULE/$STAGE/$DOC_STEM-codex-report.md`).
- Format: Summary (P0=N, P1=M, P2=K) + per-domain findings + full progress checklist
- **Must include**: Skill Invocation Log + Blocker Gates pass status WITH evidence + Artifact Inventory + Severity Reconciliation

---

## Phase 9: State Write & Convergence

1. Create/update Codex-specific STATE.md (never share intermediate results with Trae or Claude):
   - **Codex CLI** → `.review/codex/$MODULE/STATE.md`
2. **Sync new P0/P1/P2** from scan.md into STATE.md Open lists; move fixed issues to Closed Issues with scan/date.
3. Update `SYMBOLS.md` (Step 1.5 machine output) to matching path:
    - Codex: `.review/codex/$MODULE/$DOC_STEM/SYMBOLS.md`
4. **All dimension results → scan.md single file** (NOT 10 dimension check files). Dual-write if user explicitly specified output path.
5. Generate VERIFY-CHECK.md **before declaring CONVERGED**:
    - Codex: `.review/codex/$MODULE/VERIFY-CHECK.md`
6. Output convergence assessment: CONVERGED / NOT_CONVERGED
7. Output **Artifact Inventory** and **Severity Reconciliation** tables in scan.md (Gate 0 requirements).

STATE.md contents:
   - Phase progress
   - Findings counts (P0/P1/P2)
   - Convergence status
   - Coverage Status section (from Phase 2)
    - **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G✅ H✅ (all with evidence)

**Convergence criteria**:
- All dims COMPLETE in scan.md
- P0 new=0, P1 new≤1
- **Gate G: VERIFY-CHECK.md=PASS** (mandatory; do NOT mark CONVERGED without it)
- All P0 fixed+verified (or WONTFIX+reason)
- **Blocker Gates 0/A/B/C/D/D-6/E/G/H (incl. D-6 for doc review) all passed with gate-evidence attached**

---

## Domain File Mapping (5 files replace 31 checks)

| Domain File | Replaces | Checks |
|-------------|----------|--------|
| `doc.md` | 文档检查清单 | 16 doc correctness checks (Check 00-15) |
| `code.md` | code/01-16 (16 files) | 16 code correctness checks |
| `patterns.md` | patterns/* (3 files) | 15 doc + 3 cross + 19 code patterns |
| `excellence.md` | new | 5 doc + 6 code excellence checks |
| `process.md` | review-process + skip-check | Step 0-7 + meta-check + Coverage Enumeration |

> **Note**: The original 31 individual check files in `doc/`, `code/`, `patterns/` subdirectories are superseded by the 5 domain files above. They are kept for reference but should not be loaded by the orchestrator. Coverage Enumeration is executed in **Phase 2** of this orchestrator and detailed in `checks/process.md` §Step 1.5.
