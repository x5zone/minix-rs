# Review Core — always loaded

## Execution Model (by module type)
Review must check the correct concurrency model for the module being reviewed:

**User-space servers (VM/PM/VFS/RS/DS/INET)**: Single-threaded event loop. `Rc`/`RefCell`/`AssumeSyncCell`/`!Send`/`!Sync` reasonable. `UnsafeCell` safe under single-thread.

**Kernel**: SMP + BKL (Big Kernel Lock spinlock). Multi-CPU kernel execution with `CONFIG_SMP`. BKL is spinlock (busy-wait) — NO sleep/schedule/IPC inside critical section. `Rc`/`RefCell` NOT safe for cross-CPU sharing (need `Arc`+`Mutex`/`Atomic`). `UnsafeCell` safety must argue BKL-protection or per-CPU isolation, NOT "single-threaded".

## ⛔ PROHIBITED BEHAVIORS
1. **NEVER answer from memory.** Every claim about C code requires a grep/read result as evidence.
2. **NEVER skip a check.** If you cannot execute it, write "UNVERIFIED: <reason>".
3. **NEVER guess.** If uncertain, say so — do NOT fabricate.
4. **NEVER mark a check as ✅ without output.** ZERO findings = write "Checked N items, found 0 issues." Prove the check happened.
5. **NEVER let late checks decay.** Checks after #13 in a long session are at HIGH RISK of being ghosted. Pause before #14, re-read the rules.

## Ground Truth
```
Minix3 C source (minix3/) > Rust code (os/servers/) > docs (notes/) > AI analysis
```

## Review Terms
- **Translate**: 1:1 C→Rust. ❌ Forbidden.
- **Rewrite**: Same external behavior, Rust types internally. ✅ Goal.
- **Refactor**: 在不改外部行为前提下重写表达层。分两类：
  - **代码 Refactor**：改命名/抽函数/改数据结构/改错误表达（语义范围 = 代码）
  - **设计 Refactor**：修正 design doc 本身（语义范围 = design，不动代码）
- **Architectural Evolution**: 必须显式标注 ARCH，标注在 Minix3 行为对照点 + 在 design doc + 在代码注释，三处一致。

## Priority
- **P0-fact**: C 源码事实错误（概念/grep/覆盖）
- **P0-code-bug**: UB/内存安全/`std::` 违规/硬件语义泄漏。Kernel: `Rc`/`RefCell` 跨 CPU、BKL 未持、睡眠在自旋锁内
- **P0-design-deviation**: 代码偏离 design 但 design 正确
- **P0-design-missing**: design 缺关键决策（如未定义 trait 方法签名）
- **P0-design-wrong**: design 决策技术错误（如不安全抽象）
- **P0-test-missing**: design 要求测试但代码无
- **P1**: Architecture diff unstated, design without basis, dev-journal style (✅❌🚧), doc-code mismatch, pub misuse, **detail imprecision** (wrong hardware claim in comment, context leak in universal struct, silently dropped returns, leaks without rationale, dubious comment reasons), **P1-design-missing** (链路断裂)
- **P2**: Readability, naming, cross-refs, diagram quality

## Design First 原则
- **Design 是核心 deliverable**，不是 review 的副产品
- **方案 D（outline 升格）**：三类持久化交付物并存于 `design/` 子目录——`{NN}-outline.md`（doc 结构契约）、`{NN}-outline-review.md`（outline 批准证据）、`{NN}-design.md`/`{NN}-design-final.md`（code 设计契约）。outline 与 design 对称：design 是 code 的契约，outline 是 doc 的契约。
- **判定优先级链**：`Minix3 源码行为 > design doc > Rust 代码 > 设计/技术文档`
- 当 Minix3/design/code/doc 冲突时按上链判定（详见 [review-core-semantics.md §1.5](../prompt/review-rules/review-core-semantics.md)）
- **Profile R（设计优先模式 Review）**：`review xxx-design.md`（非 bagging）/ `review xxx-design-final.md`（bagging）时加载，或 review 中发现 P0-design-wrong 时触发；验证 design 完整性 + 可实现性 + design ↔ code 一致性 + outline ↔ doc 对齐。**注意（2026-07-17）**：design/outline/outline-review **缺失**场景不再触发 Design-First，而是由 **Step 0.3 嵌入生成**处理（不切换模式）。Design-First 仅用于 design **存在但有错误**（design-wrong）场景。

## ⛔ MANDATORY: Explicit Skill Invocation

- **You MUST invoke Skill tools explicitly** via the available `Skill` function. NEVER rely on "rules already loaded" or "context already has it".
- For document review: invoke `review-doc-skill` + `review-patterns-skill`
- For code review: invoke `review-code-skill` + `review-patterns-skill`
- For full/deep review: invoke all relevant Skills in phases per `review-process-skill`
- The Skill Invocation Log in scan.md must reflect actual Skill tool calls, not planned/intended calls.

## ⛔ VERIFY-CHECK 同 Agent 局限（NEW 2026-07-30）

> **核心原则**：单 session 深度 review 中，VERIFY-CHECK 不可避免是同 agent 验证——同一 LLM 可能继承同样的 bias。

### 规则

1. **同 agent 时显式标注**：scan.md / VERIFY-CHECK.md 必须显式写"⚠️ 同 agent VERIFY-CHECK（已重放 grep 命令）"，附验证局限说明段
2. **跨 agent 优先**：条件允许时，优先由不同 agent（Trae IDE / Claude Code）独立验证；Trae ↔ Claude 互为 cross-verification
3. **同 agent 不给 false confidence**：
   - 若 consistency < 90%，明确写"❌ NOT PASS"，不要给"基本通过"或"轻微不一致"等模糊表述
   - 修后再跑（post-fix）必须能提升 consistency 到 ≥ 90%
4. **基于 grep 命令重放**：每个验证项附实际执行的 grep + 输出，可由读者手动重放验证
5. **不可推断验证**：避免"应该如此"、"应该是"等语义回忆；只接受可重放的 grep 输出

### 已知同 agent 局限

| 局限类型 | 表现 | 缓解措施 |
|---------|------|---------|
| 路径偏差继承 | Step 1 与 Step 2 都用同一 LLM 的"路径直觉" | cross-agent 验证 + `find` 实际路径 |
| 字段计数偏差 | 上次 review 错 9，实际 12；本次可能继承类似偏差 | grep `pub` 字段实际数量 + AI 计数交叉验证 |
| 代码示例过时 | doc 写 `static mut`，code 已 `Atomic*` | pattern #73 检查 + grep 命令重放 |

### 关联

- 详见 `.claude/rules/review-process.md §Step 5.6 VERIFY-CHECK`
- 详见 `prompt/skill/review-process-skill.md §Step 5.6`
- 首次发现：01-boot-shim-bootstrap review 2026-07-30（consistency 71.4% → fix → 100%）
Every scan.md MUST include this table (5 items, each with grep evidence):
1. **§5 tests exist**: `rg "fn {test_name}" {rust_dir}` — missing → P0
2. **trait has ≥1 impl**: `rg "impl.*{TraitName}" {rust_dir}` — 0 impl → P0
3. **function in declared file**: `rg "fn {name}" {file}` — not found → P0
4. **core algorithm not stub**: `rg "todo!|unimplemented!|unreachable!|panic!" {rust_dir}` — stub → P0; `panic!` in non-test code that represents unimplemented functionality or a reachable unhandled path → treat as stub/unhandled path, must be justified in comment
5. **§4 signatures match**: compare doc §4 vs actual — mismatch → P0

**Strict pass rule**: Each item must be ✅. **PARTIAL / ⚠️ / "部分通过" / "基本通过" counts as ❌ FAIL.** Any ❌ item means Gate D failed → scan.md DRAFT.

## ⛔ Gate D-6: structure.md Skeleton Review (Doc Review Only)
Before correctness checks, generate `structure.md` (12-section skeleton analysis) to verify "what the reader reads" (orthogonal to correctness). Sections: 主题思想/目标读者/叙事主语/驱动方向/文档大纲/核心概念清单/跨架构统一抽象/双向闭环/叙事弧/元注释/裸概念复述/纵向链路映射. Template: `prompt/skill/review-process-skill.md` §Step 0.5. Failure → scan.md DRAFT.

## Concept Abstraction Principle (Ch1 Docs)
**Core principle**: Concept chapters (Ch1) must be organized from architecture perspective (CPU questions/system mechanisms), NOT from code perspective (function/struct/trait names).

| Dimension | ❌ Code perspective | ✅ Architecture perspective |
|-----------|---------------------|----------------------------|
| Organization | trait name / function name / struct name | CPU questions / system mechanisms |
| Ch1 subject | function name | CPU / OS / system |
| Driving direction | HOW-only (jumps to implementation) | WHY→WHAT→HOW (concept first) |
| Multi-arch | arch-specific first, no unified abstraction | unified abstraction first, then arch-specific |

Violation → P1 (pattern 51: 实现驱动概念章).

## Causal Chain Validation (P0)
Every causal explanation ("因为 X 所以 Y") must have:
- X verified as a **real mechanism** (grep C/Rust source for evidence)
- X→Y technically correct (not just plausible-sounding)

Causal chain fabrication (X is wrong or X→Y is technically wrong) → P0 (pattern 48).

## Review Process Patterns (66-74, NEW 2026-07-16/17/30)

- **66 RCPD** (Reference Code Path Drift): TODO/issue 描述引用的 `file:line` 已不存在（重构/重命名）→ Step 0.7.1 path existence validation 必跑
- **67 CFNOC** (C Function Name vs OS Concept Confusion): 把 C 函数名（如 `proc_init`）误读为 OS 概念对象（如 `Process`）→ Step 0.7.2 AI claim grep verification 必跑
- **68 DSC** (Doc Section Confusion): 把 Ch2 C 源码展示误判为 Ch4 Rust 实现问题 → Step 0.7.3 doc chapter context awareness 必跑
- **69 PSMD** (Per-doc Snapshot Missing): per-doc design/outline 快照缺失但 scan.md 标 CONVERGED → **Step 0 硬阻断** + `tools/design-coverage-check.sh`
- **70 CTOS** (Cross-Turn Outdated Staleness): TODO 列表跨轮状态陈旧（37.5% 误报率）→ Step 0.7.4 TODO staleness check 必跑
- **71 DOG** (Decision Over-Generalization): 把"X 文档豁免"用户决策泛化到"Y/Z 文档" → STATE.md §豁免列表 + 不可泛化原则
- **72 CSSCM** (Cross-Section Step Count Mismatch, NEW 2026-07-17): 文档 §2 C 分析步骤数 ≠ §4 Rust 实现步骤数且无差异说明表 → P1；差异表必须分类（架构演进/设计决策/已知缺口/C bug）+ 附 C 行号。检查命令：`rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" {doc}`
- **73 Doc Code Example Rust 2024 Edition Drift** (NEW 2026-07-30): 文档代码示例用 `static mut`（Rust 2024 已弃用），实际代码已迁移至 `Atomic*` / `UnsafeCell`；或文档路径引用与实际 `find` 结果不一致（目录重组）。检查命令：`rg "static mut" {doc}.md` + `rg "static mut" os/ -t rust`（应对比）+ `find os/arch/src -name "X.rs"`。首次发现：01-boot-shim-bootstrap review（2026-07-30）。
- **74 Doc Path Convention Drift** (NEW 2026-07-30): 文档 Rust crate 路径引用漏 `os/` workspace 根前缀（典型：`kernel/src/...` 应为 `os/kernel/src/...`），与早期 doc 跨文档不一致 → P1。检查命令：`rg "kernel/src/" {doc}.md | wc -l` > 0（应仅匹配 minix3 C 源）+ `rg "os/kernel/src/" {doc}.md | wc -l` 低 + `rg "os/os/" {doc}.md` 应为 0。修复：`sed -i 's|kernel/src/|os/kernel/src/|g'` + `sed -i 's|os/os/|os/|g'`（避免双重前缀）。首次发现：02-higher-half-kernel review（2026-07-30）。
- **75 Doc See-Also Range Drift** (NEW 2026-07-31): 文档"参见 X.rs:Y-Z"形式的范围引用出现两类漂移——起止行号 +1 偏移（如 :55-76 → 实际 :56-78）+ 上界范围过短（如 :55-399 → 实际 :56-523，doc 写作时文件较小未随代码演化更新）。检查命令：`rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" {doc}.md` + `sed -n "{start}p" {path}` 验证首行 + `rg "^impl PlatformDesc for X" {path}` 找 impl 结束位置。详细规则见 `prompt/skill/review-patterns-skill.md §模式 75` + `.claude/rules/review-process.md §Step 1.0d`。首次发现：04-platform-discovery review 2026-07-31（2 处 L831/L883 范围漂移漏检）。

## ⛔ Step 0 硬阻断（所有 review 模式强制，NEW 2026-07-16）

> 每次 review 启动时**必须**执行：
> 1. 4 条 `ls notes/rewrite/{module}/{stage}/.design/{NN}-*.v*.md`（结果写入 scan.md `§Step 0: 预检结果` 段）
> 2. `tools/design-coverage-check.sh {module}`（自动扫描所有 stage 缺失报告）
> 3. **缺失判定 + 嵌入生成（2026-07-17）**：`outline.v*.md` 缺失 → Gate H.6 FAIL → **Step 0.3.2 嵌入生成**；`outline-review.v*.md` 缺失 → Gate H.6 FAIL → **Step 0.3.3 嵌入生成**（AI 自审）；`design.v*.md` 缺失 → Gate H.1 FAIL → **Step 0.3.4 嵌入生成**。**不中断 review，不切换模式**（原"阻断 Step 1 + 触发 Design-First"已废除）。
> 4. 不允许以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"为由跳过（**模式 69 PSMD 触发**）
> 5. 豁免必须登记在 STATE.md `§豁免列表` 段，**不可泛化**（**模式 71 DOG 触发**）
>
> **详见**：`.claude/rules/review-process.md §Step 0 硬阻断规则` + `prompt/review-rules/review-patterns.md 模式 69/71`

## Review Workflow
1. Always start by declaring scope: target file, mode (doc/code/full), estimated time (optional), **STATE.md status** (see dual-path rule below)
2. **Read correct STATE.md path**: Trae IDE → `.review/trae/{module}/STATE.md`; Claude Code Runtime → `.review/claude/{module}/STATE.md`. These two paths are **isolated** — never share STATE/scan/SYMBOLS/structure/VERIFY-CHECK between tools. If the **same tool** has conflicting STATE.md copies, log divergence in scan.md and ask user which is authoritative.
3. **Step 0 硬阻断预检（NEW 2026-07-16）**：跑 4 条 `ls design/{NN}-*.v*.md` + `tools/design-coverage-check.sh {module}`，结果写入 scan.md `§Step 0: 预检结果` 段（Gate 0 锚段 9 个之一）
4. Execute checks ONE AT A TIME — never batch them mentally
5. Output a progress checklist showing each check as done/undone
6. Collect all findings into a review report at the end
7. **After all checks**: write STATE.md and convergence assessment
8. **Verify Blocker Gates 0/A/B/C/D/D-6/E/G all passed WITH EVIDENCE** before marking scan.md as Final. Gate A/D/E evidence must be L1 (tool/grep output); Gate B/C may be L1 or L2; L3 inference counts as FAIL unless `MANUAL_FALLBACK` is justified.

## ⛔ Review 累积改进追踪（NEW 2026-07-31）

> **目的**：避免每次 review 重复发明流程改进，记录累积效果与 Proposal 状态。

### 累积改进表（4 次 review）

| Review | 发现模式 | P1 | P2 | 修复成本 | 一致性 Pre→Post |
|--------|---------|-----|-----|----------|-----------------|
| 01-boot-shim-bootstrap (07-30) | Pattern #73 | 3 | 8 | 50 min | 71.4% → 100% |
| 02-higher-half-kernel (07-30) | Pattern #74 | 1 | 4 | 13 min | 57.1% → 100% |
| 03-kmain-cstart (07-31) | n/a（验证 #74 有效）| 0 | 10 | 15 min | 37.5% → 100% |
| **04-platform-discovery (07-31)** | **Pattern #75 + Step 0.3.3 首次触发** | **0** | **2 (+2 顺带)** | **4 min** | **71.4% → 100%** |

### Proposal 状态

- **#7 自动化行号校验脚本**（NEW 2026-07-31）：⏸ 待用户确认后开发 `tools/review-line-check.sh`
- **#8 测试数量准确性机制**（NEW 2026-07-31）：⏸ doc §5 强制格式 + CI 钩子
- **#9 Double-check 关键 Finding**（NEW 2026-07-31）：✅ 已应用（03 review 中 P2-10 misread 被及时发现）
- **#10 Pattern #75 Doc See-Also Range Drift**（NEW 2026-07-31）：✅ 已落地 + Step 1.0d 已加 review-process.md

### 关键洞察

1. **Pattern #74 跨文档传播有效**：doc 02 发现 → 修复 → doc 03/04 自然合规（16/67 处 `os/` 前缀，0 双重前缀）
2. **测试数量 undercount 系统性**：3 个 doc 都存在（4.2×/1.6×/1.8×），需建立机制（Proposal #8）
3. **行号漂移是结构性弱点**：4 个 doc 都有 5-10 处 P2 偏移，需自动化（Proposal #7）
4. **"安静的"doc 可能反映 review 走流程不深入**：doc 03 0 P1 不一定意味着 doc 完美——下次对 0 P1 doc 做反向抽查（Proposal #9）
5. **Step 1.0a 漏检"参见"范围引用**：本次 04 doc review 即因此漏检 2 处 L831/L883 → 已加 **Step 1.0d + Pattern #75**（Proposal #10）
6. **Step 0.3.3 嵌入生成首次触发**：04 doc outline-review.v*.md 缺失，自审路径工作正常（Gate H.6 修复）—— 但**自审不构成用户确认**，后续需 user-confirmed review 复核

**详见**：
- `.claude/rules/review-process.md §Step 1.0a-自动`（Proposal #7）
- `.claude/rules/review-process.md §Step 1.0d`（Pattern #75，Proposal #10）
- `.claude/rules/review-process.md §Step 4.5b`（Proposal #8）
- `.claude/rules/review-process.md §Step Double-check`（Proposal #9）
- `.claude/rules/review-process.md §Step 0.5.2 元注释章节 review`（NEW 2026-07-31）
