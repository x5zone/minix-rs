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
Minix3 C source behavior > design contract > Rust code (os/) > design/technical docs (notes/) > AI analysis
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
- **方案 D（outline 升格）**：三类持久化交付物并存于 `.design/` 子目录——`{NN}-outline.md`（doc 结构契约）、`{NN}-outline-review.md`（outline 批准证据）、`{NN}-design.md`/`{NN}-design-final.md`（code 设计契约）。outline 与 design 对称：design 是 code 的契约，outline 是 doc 的契约。
- **判定优先级链**：`Minix3 源码行为 > design doc > Rust 代码 > 设计/技术文档`
- 当 Minix3/design/code/doc 冲突时按上链判定（详见 [review-core-semantics.md §1.5](../../prompt/review-rules/review-core-semantics.md)）
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
2. **trait has ≥2 行为不同的 impl**: `rg "impl.*{TraitName}" {rust_dir}` — 0 impl → P0（死代码/虚构 trait）；1 impl → P1（trait 抽象需 ≥2 行为不同的实现）；≥2 impl → ✅
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

## Review Process Patterns (66-78, NEW 2026-07-16/17/30/31/08-14)

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
- **76 Cross-Doc Attribution Drift** (NEW 2026-07-31): 代码注释中"covered in NN" / "see XX-doc.md §Y" 等指向特定 doc 编号或文件名的引用，因 doc 编号重排或 doc 改名而系统性过时。本次 05 review 发现 `os/kernel/src/lib.rs` 3 处 `(covered in NN)` 注释错位 + **至少 9 处 `see XX-doc.md` 引用旧 doc 命名**（`04-clock-interrupt-init.md` / `05-exception-interrupt.md` / `06-arch-post-init.md` / `06-design-final.md` / `02-page-table-kernel.md`）。检查命令：`rg "covered in 0[0-9]|see 0[0-9]-.+\.md" os/ -t rust -n` + `ls notes/.../ | rg "^[0-9]+"` 验证 doc 编号 + 对每个引用验证目标 doc 是否存在。详细规则见 `prompt/skill/review-patterns-skill.md §模式 76` + `.claude/rules/review-process.md §Step 1.0e`。首次发现：05-clock-interrupt-init review 2026-07-31（3 处 `(covered in NN)` + 9+ 处 `see XX-doc.md`）。
- **77 Code Comment Line Drift** (NEW 2026-07-31): 代码注释中引用 `file:line`（如 `// see proc_table.rs:129`）因代码增量而**漂移**（文件行号下移）。本次 08 doc review 发现 `os/kernel/src/lib.rs:1170/1181/1193` 3 处注释错位：`proc_table.rs:129` 实际 L276（+147，最大）/ `smp.rs:127-132` 实际 L200（+73）/ `smp.rs:80-145` 实际 L135（+55）。检查命令：`rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -n` + `sed -n "{line}p" {path}` 验证首行内容。修复策略：双修（修代码注释 root cause + 同步修所有复述的 doc）。详细规则见 `prompt/skill/review-patterns-skill.md §模式 77` + `.claude/rules/review-process.md §Step 1.0f`。首次发现：08-system-init-boot-finish review 2026-07-31（**根因是 lib.rs 代码注释错误，doc §4.6 复述了错误注释**）。
- **78 CSBU** (C Source Bug Unlabeled, NEW 2026-08-14): Rust 修复了 Minix3 C 源码 bug，但代码注释未标注 `// MINIX3 BUG:`。维护者可能"修复"回 C 的 bug。判定标准：Rust 行为与 C 不一致（因 C 源码有 bug，非设计差异）+ 代码注释无 `// MINIX3 BUG:` + 文档 §2 对应位置无 bug 说明 → **P1**。检查命令：`rg "// MINIX3 BUG:" os/ --type rust -n`。正确示例：`// MINIX3 BUG: region.c:841-842 ignores ev_reference return value` + `// Rust fix: ev_copy returns Err(NotSupported)`。来源案例：region.c:841 ev_reference 忽略、enter_queue 写错进程、anon_pagefault 内存泄漏。详细规则见 `prompt/skill/review-patterns-skill.md §模式 78`。首次发现：2026-08-14 规则集优化（从 project_memory 沉淀的多个 C bug 修复案例抽象）。

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
1. Always start by declaring scope: target file, mode (doc/code/full), estimated time (optional), **STATE.md status** (see tool-isolated path below)
2. **Read correct STATE.md path**: Trae IDE → `.review/trae/{module}/STATE.md`; Claude Code Runtime → `.review/claude/{module}/STATE.md`; Codex CLI → `.review/codex/{module}/STATE.md`. These paths are **isolated** — never share STATE/scan/SYMBOLS/structure/VERIFY-CHECK between tools. If the **same tool** has conflicting STATE.md copies, log divergence in scan.md and ask user which is authoritative.
3. **Step 0 硬阻断预检（NEW 2026-07-16）**：跑 4 条 `ls .design/{NN}-*.v*.md` + `tools/design-coverage-check.sh {module}`，结果写入 scan.md `§Step 0: 预检结果` 段（Gate 0 锚段 9 个之一）
4. Execute checks ONE AT A TIME — never batch them mentally
5. Output a progress checklist showing each check as done/undone
6. Collect all findings into a review report at the end
7. **After all checks**: write STATE.md and convergence assessment
8. **Verify Blocker Gates 0/A/B/C/D/D-6/E/G/H all passed WITH EVIDENCE** before marking scan.md as Final. Gate A/D/E evidence must be L1 (tool/grep output); Gate B/C may be L1 or L2; L3 inference counts as FAIL unless `MANUAL_FALLBACK` is justified.

## ⛔ Review 累积改进追踪（NEW 2026-07-31）

> **目的**：避免每次 review 重复发明流程改进，记录累积效果与 Proposal 状态。

### 累积改进表（10 次 review）

| Review | 发现模式 | P0 | P1 | P2 | 修复成本 | 一致性 Pre→Post | 里程碑 |
|--------|---------|-----|-----|----|----------|-----------------|--------|
| 01-boot-shim-bootstrap (07-30) | Pattern #73 | 0 | 3 | 8 | 50 min | 71.4% → 100% | — |
| 02-higher-half-kernel (07-30) | Pattern #74 | 0 | 1 | 4 | 13 min | 57.1% → 100% | — |
| 03-kmain-cstart (07-31) | n/a（验证 #74 有效）| 0 | 0 | 10 | 15 min | 37.5% → 100% | — |
| 04-platform-discovery (07-31) | Pattern #75 + Step 0.3.3 首次触发 | 0 | 0 | 2 (+2 顺带) | 4 min | 71.4% → 100% | — |
| 05-clock-interrupt-init (07-31) | Pattern #76 + Step 1.0e + §2.4g | 0 | 4 | 4 | 28 min | 62.5% → 100% | — |
| 06-proc-init-boot-proc (07-31) | n/a（验证累积改进 + 反向偏移建议）| 0 | 0 | 6 | 12 min | 50% → 100% | — |
| 07-cross-space-init (07-31) | n/a（验证累积改进 + §2.4i/§2.4j 首次跑 + Zero-bias）| 0 | 0 | 0 | 0 min | 100% ✅ | **Zero-bias #1** |
| 08-system-init-boot-finish (07-31) | Pattern #77 + Step 1.0f + Step Double-check | 0 | 0 | 3 | 6 min | 87.5% → 100% | **首个三 snapshot 全齐 + Double-check** |
| 09-vm-boot-protocol (07-31) | n/a（验证累积改进 + Zero-bias #2 + Perfect Link）| 0 | 0 | 0 | 0 min | 100% ✅ | **Zero-bias #2 + Perfect Link + Perfect line refs** |
| **10-switch-to-user (07-31)** | **n/a（验证累积改进 + Hidden Folder Convention + Double-check）** | **0** | **0** | **2** | **4 min** | **75% → 100%** | **首个 Hidden Folder Convention 完全合规 doc** |

### Proposal 状态

- **#7 自动化行号校验脚本**（NEW 2026-07-31）：⏸ 待用户确认后开发 `tools/review-line-check.sh`（**06 review 增强**：反向偏移自动重算）
- **#8 测试数量准确性机制**（NEW 2026-07-31）：⏸ doc §5 强制格式 + CI 钩子（**06 review 增强**：测试总数末段补充）
- **#9 Double-check 关键 Finding**（NEW 2026-07-31）：✅ 已应用（03 review 中 P2-10 misread 被及时发现）
- **#10 Pattern #75 Doc See-Also Range Drift**（NEW 2026-07-31）：✅ 已落地 + Step 1.0d 已加 review-process.md
- **#11 Pattern #76 Cross-Doc Attribution Drift**（NEW 2026-07-31）：✅ 已落地 + Step 1.0e 已加 review-process.md
- **#12 design.md §X-Y "权威位置"段**（NEW 2026-07-31）：⏸ 待用户确认后落地（DEFAULT_HZ 跨 crate 重复定义暴露需求）
- **#13 Step 1.0a-自动 反向偏移自动重算**（NEW 2026-07-31，Doc 06 review）：⏸ Proposal #7 增强（auto_resync_line 函数）
- **#14 L3 grep 主动验证（§2.4i）**（NEW 2026-07-31，Doc 06 review）：✅ **已落地（Doc 07 验证成功）**
- **#15 测试总数末段补充（§2.4j）**（NEW 2026-07-31，Doc 06 review）：✅ **已落地（Doc 07 验证成功）**
- **#16 Step 7.1 触发停止规则 3**（NEW 2026-07-31，Doc 07 review；**2026-08-14 更新为漏检自检**）：✅ 已应用（zero-bias 时触发漏检自检，原"强制交付"已废弃）
- **#17 Pattern #66 RCPD doc 主动应用**（NEW 2026-07-31，Doc 07 review）：✅ Doc 07 §5.4 显式标注
- **#18 Step 0.3.3 outline-review 批量补齐**（NEW 2026-07-31，Doc 07 review）：⏸ 待用户确认后统一触发 user-confirmed review（doc 04/05/06/07 都缺失）
- **#19 Pattern #77 Code Comment Line Drift**（NEW 2026-07-31，Doc 08 review）：✅ 已落地 + Step 1.0f 已加 review-process.md
- **#20 Step 1.0f 代码注释行号漂移检查**（NEW 2026-07-31，Doc 08 review）：✅ 已落地（Pattern #77 配套）
- **#21 Zero-bias 里程碑 #2 + Perfect Link**（NEW 2026-07-31，Doc 09 review）：✅ Doc 09 review 触发 Step 7.1 规则 4（第二次）+ 首次 perfect line refs（16/16 全行号零偏差）+ 首次 perfect vertical link（15/15 全链路完整）
- **#22 Hidden Folder Convention + forward reference 验证**（NEW 2026-07-31，Doc 10 review）：✅ Doc 10 review 验证 CLAUDE.md Hidden Folder Convention（首个完全合规 doc）+ forward reference 验证机制（trap_return.rs 不存在但 doc 显式标注"待落地"—— ✅ 合规）

### 关键洞察

1. **Pattern #74 跨文档传播有效**：doc 02 发现 → 修复 → doc 03/04/05/06/07 自然合规（16/67/31/33/16 处 `os/` 前缀，0 双重前缀）
2. **测试数量 undercount 系统性**：3 个 doc 都存在（4.2×/1.6×/1.8×），需建立机制（Proposal #8 + #15 增强 → Doc 07 已落地）
3. **行号漂移是结构性弱点**：7 个 doc 累计 25+ 处 P2 偏移（含反向偏移），需自动化（Proposal #7 + #13 增强）
4. **"安静的"doc 可能反映 review 走流程不深入**：doc 03 0 P1 不一定意味着 doc 完美——下次对 0 P1 doc 做反向抽查（Proposal #9）→ **Doc 07 验证：0 P1 不等于 review 不深入，而是累积改进极致效果**
5. **Step 1.0a 漏检"参见"范围引用**：本次 04 doc review 即因此漏检 2 处 L831/L883 → 已加 **Step 1.0d + Pattern #75**（Proposal #10）
6. **Step 0.3.3 嵌入生成连续 4 次触发**：04/05/06/07 doc outline-review.v*.md 都缺失，自审路径工作正常（Gate H.6 修复）—— 但**自审不构成用户确认**，后续需 user-confirmed review 复核（建议批量补齐，Proposal #18）
7. **代码注释 doc 归属系统性过时**：05 doc review 发现 `(covered in NN)` 注释错位（3 处）+ `see XX-doc.md` 引用旧 doc 命名（9+ 处）—— **Pattern #76 实质化**（之前 4 次 review 未深入代码注释交叉）→ 已加 **Step 1.0e + Pattern #76**（Proposal #11）
8. **跨 crate const 重复定义盲点**：DEFAULT_HZ 在 os/arch + os/kernel 两 crate 独立定义（不会编译错误）→ 已加 **"权威位置"段说明**（Proposal #12）
9. **行号反向偏移是系统性问题（NEW）**：06 doc review 发现 6 处反向偏移（-7/-9/-13），代码增量后 doc 未同步 → **Step 1.0a-自动 + auto_resync_line**（Proposal #13 增强 Proposal #7）
10. **§5.4 L3 grep 证据未主动验证（NEW）**：06 doc review 依赖 doc 自证 → **L3 grep 主动验证（§2.4i）**（Proposal #14 → Doc 07 验证成功）
11. **测试总数末段补充（NEW）**：06 doc 无测试总数声称但实际 610 tests 通过 → **测试总数末段补充（§2.4j）**（Proposal #15 → Doc 07 验证成功）
12. **Zero-bias 里程碑（NEW, Doc 07）**：7 次 review 中**首次** 0 P0/P1/P2（review 即 PASS）—— 累积改进极致效果 + **触发 Step 7.1 停止规则 4**（**2026-08-14 更新为漏检自检**：原"强制交付"已废弃，改为抽 3 项重跑确认无遗漏再交付）
13. **Doc 主动应用 Pattern #66 RCPD（NEW, Doc 07）**：doc 07 §5.4 显式声明"不引用 syscall_copy.rs 行号避免漂移传播"—— **首个 doc 主动标注已应用 review pattern**，doc 作者已具备 review pattern 意识
14. **代码注释 doc 复述传递性 drift（NEW, Doc 08）**：08 doc review 发现 3 处 P2 行号偏移全部源自 `lib.rs:1170/1181/1193` 的代码注释错误（不是 doc 错）—— doc §4.6 复述了错误注释。**根因诊断**：必须修代码注释（root cause）+ 同步修所有复述的 doc（避免传递性 drift）→ **Pattern #77 + Step 1.0f**（Proposal #19/#20，已落地）
15. **首个 doc 三 snapshot 全齐（NEW, Doc 08）**：08 review 是 8 次 review 中**首个** tool scan ✅ PASS（outline + outline-review + design 都真实存在）—— 卓越设计维护的标志 + 不需要 Step 0.3.3 嵌入生成
16. **Zero-bias 里程碑 #2 + Perfect Link（NEW, Doc 09）**：09 review 是 9 次 review 中**第二个** zero-bias（继 Doc 07 后）+ **首次 perfect line refs（16/16 全行号零偏差）+ 首次 perfect vertical link（15/15 全链路完整）**—— 累积改进极致效果
17. **Step 1.0f 主动验证首次应用（NEW, Doc 09）**：Pattern #77 检查无新 drift（仅 backlog 9+ 处旧 doc 命名）—— 累积改进已固化为系统自动检查
18. **Hidden Folder Convention + forward reference 验证（NEW, Doc 10）**：10 review 首个完全合规 doc（无 design/tmp_design 引用）+ §4.1 trap_return.rs forward reference 透明声明（合规）

**详见**：
- `.claude/rules/review-process.md §Step 1.0a-自动`（Proposal #7 + #13）
- `.claude/rules/review-process.md §Step 1.0d`（Pattern #75，Proposal #10）
- `.claude/rules/review-process.md §Step 1.0e`（Pattern #76，Proposal #11）
- `.claude/rules/review-process.md §Step 4.5b`（Proposal #8 + #15）
- `.claude/rules/review-process.md §Step Double-check`（Proposal #9）
- `.claude/rules/review-process.md §Step 0.5.2 元注释章节 review`（NEW 2026-07-31）
- `.claude/rules/review-process.md §L3 grep 主动验证（§2.4i）`（NEW 2026-07-31，Proposal #14，已落地）
- `.claude/rules/review-process.md §测试总数末段补充（§2.4j）`（NEW 2026-07-31，Proposal #15，已落地）
- `.claude/rules/review-process.md §Step 7.1 触发停止规则 3`（NEW 2026-07-31，Proposal #16）
- `prompt/skill/review-doc-skill.md §2.4g const 权威位置检查`（Proposal #12）
- `prompt/skill/review-doc-skill.md §2.4i L3 grep 主动验证`（Proposal #14，已落地）
- `prompt/skill/review-doc-skill.md §2.4j 测试总数末段补充`（Proposal #15，已落地）
- `prompt/skill/review-patterns-skill.md §Pattern #66 RCPD`（已被 doc 07 主动应用，Proposal #17）
- `prompt/skill/review-patterns-skill.md §Pattern #77 CCLD`（NEW 2026-07-31，Proposal #19）
- `.claude/rules/review-process.md §Step 1.0f 代码注释行号漂移检查`（NEW 2026-07-31，Proposal #20）
