# Review Process — always loaded

Every review session must produce these visible artifacts. Do NOT "check in your head."

## ⛔ Blocker Gates (must pass before Final Review output)

| Gate | Check | Pass Criteria | Fail Consequence |
|------|-------|---------------|------------------|
| **0** | Artifact Inventory | Standard paths complete (STATE/scan/structure/SYMBOLS); scan.md contains 8 grep-verifiable anchor sections | DRAFT, no STATE.md write |
| **A** | Step 1.5 Coverage Enumeration | coverage-extract.py executed + SYMBOLS.md on disk + `gate-evidence-A` block in scan.md | DRAFT, no STATE.md write |
| **B** | Step 2 Diff Extraction | Top 5 behavior contract table (3 语义偏移 + 2 覆盖缺口, **8 fields × 5 funcs**) | DRAFT, no STATE.md write |
| **C** | Step 3.5 Precision Check | 5 meta-rules check table output | DRAFT, no STATE.md write |
| **D** | P0 Mandatory Checklist (see patterns §0) | 5 items answered (✅/❌ + grep evidence); PARTIAL/⚠️ = FAIL | DRAFT, no STATE.md write |
| **D-6** | Step 0.5 structure.md Skeleton Review (doc review only) | structure.md generated + 12-section review table + failures in Issue List | DRAFT, no STATE.md write (doc review) |
| **E** | Step 4.5 Test Verification | §5 each test function grep-verified (if doc has §5) | DRAFT, no STATE.md write |
| **G** | Step 5.6 VERIFY-CHECK | VERIFY-CHECK.md produced + verdict PASS (consistency ≥ 90%) | DRAFT, NOT CONVERGED |

**Any Gate failed → scan.md marked DRAFT, STATE.md NOT updated.**

**Gate Evidence Rule**: For every Gate, attach the actual command + output snippet in a `gate-evidence-{X}` block in scan.md. "✅ Gate passed" without evidence is invalid.
- Gate 0: Artifact Inventory table with expected vs actual paths + sizes.
- Gate A: coverage-extract.py command line and stdout coverage summary.
- Gate B: 5-row behavior-contract table with 8 fields per function.
- Gate D: 5 P0 checklist grep/Read results (command + output).
- Gate D-6: structure.md path + 12-section review table.
- Gate E: `rg "fn {name}"` for each test function.
- Gate G: VERIFY-CHECK.md path + sampling consistency percentage.

**Evidence strength**: L1 (tool/grep output) required for A/D/E; L1 or L2 for B/C; L3 inference = FAIL unless `MANUAL_FALLBACK` justified.

## Skill Invocation Log (mandatory in scan.md)
```
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Step 3 | §6 概念准确性表 |
```
Missing this section → scan.md marked DRAFT.

## Step 0: Scope Declaration + State Recovery
1. **Read correct STATE.md path** (tool-isolated, never share intermediate results between Trae and Claude):
   - **Trae IDE** → `.review/trae/{module}/STATE.md` (project root `.review/`)
   - **Claude Code Runtime** → `.review/claude/{module}/STATE.md` (project root `.review/`)
   - If the **same tool** has conflicting STATE.md copies, **do not auto-merge**. Log divergence in scan.md and ask user which is authoritative.
   - **`{module}` resolution**: use the **first directory under `notes/rewrite/`** in the target doc path. E.g. `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md` → `{module}=fork-syscall-rewrite`. This is separate from the coverage script's `--module kernel` (Minix3 module name); do not mix them.
   - **`{doc-stem}`** = target doc basename without extension (e.g. `03-kmain-cstart`). **`{agent}`** = model id (Trae: glm/kimi/...; Claude: m3/...).
   - **STATE preflight**: run `tools/review-state-validate.py {state_path}` to verify referenced files exist and Open issues map to scan.md entries.
   - **Auto-init**: `tools/review-init.sh claude {doc-path}` computes `{module}`/`{doc-stem}` and creates standard directories.
2. Output:
```
- Target: <file.md> + <file.rs> (if code review)
- Mode: doc / code / full
- Same-dir docs: <list>
- Estimated time: <N> min (scale: 100 lines = ~10 min) OR "omitted, rely on Blocker Gates + VERIFY-CHECK anti-laziness"
- STATE.md: exists → done [X,Y,Z], pending [A,B,C] / N/A
```

## Step 1: C Source Verification
- `ls minix3/minix/servers/<module>/*.c`
- For each `.c` reference in the doc, verify file exists and line numbers are correct.
Output: | File | Doc Reference | Exists? | Line Range |

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
11. **裸概念复述** — reader can retell core concept after reading Ch1; cannot → P1
12. **纵向链路映射** — Ch1 concept ↔ Ch2 C code ↔ Ch3 design ↔ Ch4 impl; broken link → P1

Step 0.5.2 按 12 节评审表逐项判定，失败项写入 scan.md Issue List
Step 0.5.3 structure.md 评审通过后才进入 Step 1（覆盖率穷举）

Output: structure.md path + 12-section review table + failures in Issue List.

> ⛔ **Gate D-6**: structure.md generated + 12-section review table complete + failures in Issue List. Failure → scan.md DRAFT.

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

## Step 2: Diff Extraction — Gate B
Identify **Top 5** divergences: 3 语义偏移 + 2 覆盖缺口 (from SYMBOLS.md).
Fill 8-field behavior contract table per function (see core-semantics §2.2).
Output: | # | Point | C Behavior | Doc/Code Description | Severity |

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

## Step 4.5: Test Verification — Gate E
If doc has §5 (test section): extract each test function name, grep in rust_dir.
- `rg "fn {test_name}" {rust_dir} --type rust -n`
- Missing test → P0 (test missing)
Output: | §5 test name | grep cmd | result | verdict |

## Step N: Progress Checklist
At the END of every session, output:
```
### Review Progress
- [✅] Step 0: Scope
- [✅] Step 1: C Source Verification
- [✅] Gate D-6: Step 0.5 structure.md Skeleton Review (doc review only)
- [✅] Gate A: Step 1.5 Coverage Enumeration
- [✅] Gate B: Step 2 Diff Extraction (Top 5)
- [✅] Gate C: Step 3.5 Precision Check
- [✅] Step 3.5a: Vertical Link Check (doc review only)
- [✅] Step 3.5b: Causal Chain Sampling (doc review only)
- [✅] Gate D: P0 Mandatory Checklist (5 items)
- [✅] Gate E: Step 4.5 Test Verification (if applicable)
- [✅] Gate G: Step 5.6 VERIFY-CHECK produced and PASS
- [✅] Step 5.7: Rule Discovery (✅/❌ + draft if ✅)
- [✅] Step 7.1: Convergence Cost Warning assessment
- [ ] doc-00: claims-evidence
- [ ] doc-01: concept accuracy
...

```

## Step Final: State Write & Convergence
After ALL checks are done:
1. Write/update tool-specific STATE.md (tool-isolated, never share intermediate results):
   - **Trae IDE** → `.review/trae/{module}/STATE.md`
   - **Claude Code Runtime** → `.review/claude/{module}/STATE.md`
2. **Sync new P0/P1/P2** from scan.md into STATE.md Open lists; move fixed issues to Closed Issues with scan/date.
3. **All dimension results → scan.md single file** (NOT 10 dimension check files)
   - Trae default: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
   - Claude default: `.review/claude/{module}/{doc-stem}/scan.md`
   - If user explicitly requests another output location, **dual-write**: user-specified path + tool default path (Trae interactive fix doc: `notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`; Claude report: `notes/rewrite/{module}/{stage}/{doc-stem}-claude-report.md`).
4. Update SYMBOLS.md (Step 1.5 machine output) to matching path:
   - Trae: `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
   - Claude: `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
5. Generate VERIFY-CHECK.md **before declaring CONVERGED**:
   - Trae: `.review/trae/{module}/VERIFY-CHECK.md`
   - Claude: `.review/claude/{module}/VERIFY-CHECK.md`
6. Output **Artifact Inventory** and **Severity Reconciliation** tables in scan.md (Gate 0 requirements).

STATE.md format:
```
# Review State: {module}
- **Phase**: [concept | ref | struct | coverage | design | link | code | cross-doc | claims | verify | complete]
- **Last completed**: <phase>
- **Open P0/P1/P2**: N/M/K
- **Convergence**: CONVERGED / NOT_CONVERGED (N phases left)
- **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G✅ (all with evidence)
```

**Convergence criteria** (all must pass):
1. All dimensions marked COMPLETE in scan.md
2. P0 new = 0 in latest full pass
3. P1 new ≤ 1
4. **Gate G: VERIFY-CHECK.md = PASS** (mandatory; do NOT mark CONVERGED without it)
5. All P0 in scan.md fixed+verified (or WONTFIX+reason)
6. **Blocker Gates 0/A/B/C/D/D-6/E/G all passed with gate-evidence attached**

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
3. **Cost/benefit ratio**: current round cost >80% of previous but new findings <20% → stop

Output in scan.md tail:
```
### 收敛成本评估
- 当前轮次: N
- 本轮新发现: P0=X, P1=Y, P2=Z
- 触发停止规则: [1/2/3/无]
- 决定: 继续收敛 / 强制交付（剩余转 backlog）
```

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
