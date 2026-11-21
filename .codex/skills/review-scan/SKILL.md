---
name: review-scan
description: "Scan a rewrite-notes/ directory: coverage + doc + code + patterns + excellence checks, write a review report with convergence status. Use when user says review/scan/check a documentation directory."
---

# Review Scan — Orchestrator

You are a review orchestrator. Load the canonical rule file for each phase and execute it; this file does not restate the checks.
## MANDATORY RULES

1. **Explicit Skill Invocation**：每个领域开始前用 `Skill` 工具真正调用对应技能（`review-doc-skill` / `review-code-skill` / `review-patterns-skill` / `review-process-skill` / `review-core-semantics-skill` / `review-excellence-skill` / `review-coverage-skill` / `review-socratic-skill`）。scan.md 的 Skill Invocation Log 只记实际调用。
2. **先读后判**：每个判定先 grep/read 取得证据，再下结论。
3. **证据分级**：判定标 `[DIRECT]`（grep 输出）/ `[MEDIUM]`（名称匹配，语义待确认）/ `[INFERRED]`（推断，标“待确认”）。Gate 证据块的级别按 `review-process.md` Gate 注册表。
4. **零输出禁令**：某检查 0 发现时写“Checked N items, found 0 issues”，禁止留空；零输出检查按 `review-process.md` §零输出复跑重跑。
5. **一次一域**：完成一个领域并写入 scan.md 后再开下一个，不并行记忆。

## 规范源（按阶段加载，不复制）

| 阶段 | 规范源 |
|---|---|
| 范围 / Step 0 | `prompt/review-rules/review-process.md` §Step 0；`prompt/skill/review-process-skill.md` |
| 覆盖率 | `prompt/skill/review-coverage-skill.md` |
| 文档正确性 | `prompt/review-rules/review-doc-checklist.md` |
| 代码正确性 | `prompt/review-rules/review-code-checklist.md` |
| 错误模式 | `prompt/review-rules/review-patterns.md`（全量）+ `prompt/skill/review-patterns-skill.md`（适配） |
| 卓越性 | `prompt/review-rules/review-doc-excellence.md` + `prompt/review-rules/review-code-excellence.md` |
| 收敛 / 出版 | `prompt/review-rules/review-evals.md` §6A（G1–G14）+ `prompt/review-rules/review-publication.md` |
| Gate 证据 | `prompt/review-rules/review-process.md` Gate 注册表 + scan 契约 |

## Phase 1: Scope + Step 0

1. `ls $ARGUMENTS`，输出“Found N .md files, M .rs files.”
2. 读取工具隔离的 STATE.md：
   - Codex → `.review/codex/$STAGE/STATE.md`
   `$STAGE` = 文档路径在笔记树根下的第一级目录；`$DOC_STEM` = 文档 basename。
3. 跑 `python3 tools/verify-check.py --preflight` 与 `python3 tools/review-state-validate.py --state <STATE.md> --scan <scan.md> --strict`。
4. scan.md 必须含 `## Step 0: 预检结果`（含关联代码清单）与 `## Skill Invocation Log`（真实调用表）。
5. design/outline 快照是可选输入：不要求 `ls` 四条，不因缺失补生成，也不影响 Gate H。

## Phase 2: Coverage

调用 `review-coverage-skill`，按 `prompt/skill/review-coverage-skill.md` 运行 `coverage-extract.py`，生成 SYMBOLS.md。命令、stdout 与 exit code 写入 `gate-evidence-A`（`level: L1`）。脚本不可用 → PARTIAL，不得标 PASS。

## Phase 3: Doc Checks

调用 `review-doc-skill`，按 `prompt/review-rules/review-doc-checklist.md` 逐项执行；证据写入 scan.md Issue List 与 gate-evidence 块。不要另造检查清单。

## Phase 4: Code Checks

调用 `review-code-skill`，按 `prompt/review-rules/review-code-checklist.md` 逐项执行。C↔Rust 语义对齐、执行模型、no_std 证据不足时按 P0/P1 入 Issue List。

## Phase 5: Pattern Checks

调用 `review-patterns-skill`，对照 `prompt/review-rules/review-patterns.md` 的 P0 六类与模式库。模式编号引用必须能在模式库中找到；已退役编号（61/62/69）不作现行判据。

## Phase 6: Excellence Checks

调用 `review-excellence-skill`，按 `review-doc-excellence.md` 与 `review-code-excellence.md` 执行。卓越性缺口按 G2 的 P1/P2 入 Issue List；不另设卓越性门槛。正确性 P0/P1 未清零时先修，不进入卓越性扩张。

## Phase 7: Process & Verification

调用 `review-process-skill`，按 `prompt/review-rules/review-process.md` 执行 Step 1.6 / 2 / 3.5 / 3.6 / 4.5 / 5.6 / 5.7 / 7.1：

- Gate H 只查 H.2–H.5 语义项；无 design 文档时用 C 源码矩阵或 N/A + 理由。
- `gate-evidence-{X}` 块首行写 `level: L1|L2|L3`；最低级别按 Gate 注册表，L3 只允许 B/C/D-6/H 且附 `MANUAL_FALLBACK`。
- Step 5.6 产出 VERIFY-CHECK.md；consistency ≥90% 才可 CONVERGED。
- Step 5.7 必答 Rule Discovery；Step 7.1 按 3 轮硬上限停止修改。

## Phase 8: Report

把全部发现写进单份 scan.md：
- Codex 默认：`.review/codex/$STAGE/$DOC_STEM/scan.md`
- 必含：Skill Invocation Log、Blocker Gates 状态及 gate-evidence、Step 0 预检结果、Issue List、Artifact Inventory、Severity Reconciliation。

## Phase 9: State + Convergence

1. 更新工具专属 STATE.md（只写自己工具的目录）。
2. 把 scan.md 的 P0/P1/P2 同步到 STATE Open/Closed 列表。
3. 生成/更新 VERIFY-CHECK.md；未生成不得标 CONVERGED。
4. 出口判据只按 `review-evals.md` §6A 的 G1–G14 与 `review-publication.md` 的出版门；普通 review 与 review-loop 共用这一套。
5. 输出 CONVERGED / NOT CONVERGED，并附 Gate 证据与 Artifact Inventory。
