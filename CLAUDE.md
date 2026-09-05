# Minix-RS

Minix3 kernel modules rewritten in Rust (x86-64, no_std). Not a translation — a semantic rewrite preserving external behavior while using Rust's type system internally.

## Build & Test
- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy`

## Directory Layout
```
minix3/              — original Minix3 C source (ground truth, do NOT modify)
os/servers/vm/       — VM server Rust rewrite
os/libs/minix-types/ — shared IPC types, constants, codec traits
notes/rewrite/       — documentation (one .md per C source concept)
prompt/              — review rules, skill definitions (source of truth for .claude/)
.claude/             — Claude Code runtime: rules + skills (derived from prompt/)
```

## Hidden Folder Convention（NEW 2026-07-31）

**`.design/` 和 `tmp_design_and_todo/` 文件夹视为中间产物，**正式文档绝不引用**：

- **`notes/rewrite/{module}/{stage}/.design/`**：每个 doc 的可复用快照（`{NN}-outline.v*.md` / `{NN}-outline-review.v*.md` / `{NN}-design.v*.md`）。每次 review 时 AI **重新执行** Step 0.3 流程从 C 源码独立推导，旧快照作为"前人理解参考"输入（**非 ground truth**）。**正式 doc 不引用此文件夹**。
- **`notes/rewrite/{module}/{stage}/tmp_design_and_todo/`**：早期手动生成的"design"文件夹（多 AI 设计汇总），已**废弃**。**正式 doc 不引用此文件夹**。
- **配置**：两文件夹均 chmod 700（隐藏）+ `.gitignore` 排除（`**/.design/` + `**/tmp_design_and_todo/`）。
- **正式 doc 自包含**：正式 doc (00-25) 必须是**自包含**的，引用应使用绝对路径到 doc、代码、C 源——**绝不引用中间产物**。
- **Mode 69 PSMD / Mode 71 DOG 触发**：若正式 doc 引用 `.design/` 或 `tmp_design_and_todo/`，视为 P0-process-violation。

## Execution Model (by module type)
- **User-space servers (VM/PM/VFS etc.)**: Single-threaded event loop — `!Send`/`!Sync`/`AssumeSyncCell`/`Rc`/`RefCell` correct
- **Kernel**: SMP + BKL (Big Kernel Lock spinlock) — multi-CPU concurrency possible. Shared data needs `Arc`+`Mutex`/`Atomic`, not `Rc`/`RefCell`. BKL is spinlock: no sleep/schedule/IPC inside critical section.

## Coding Constraints
- `#![no_std]` everywhere except `#[cfg(test)]`
- Error types must map to Minix3 errno values — no self-invented error codes
- Hardware is abstracted behind traits — never expose CR3/PTE bits to OS layer
- **Concept abstraction (Ch1 docs)**: Concept chapters organized from architecture perspective (CPU questions/system mechanisms), NOT from code perspective (function/struct/trait names). Ch1 subject = CPU/OS, not function name. Multi-arch docs give unified abstraction first.

## Design First
- **Design is a core deliverable**, not a byproduct of review. Each `.design/{NN}-design.md` (non-bagging) or `.design/{NN}-design-final.md` (bagging) is reviewed in **Profile R (Design-First Mode)** before any code is written against it. Design docs live in `notes/rewrite/{module}/{stage}/.design/` subdirectory.
- **方案 D v2（可复用快照，2026-07-16）**：outline.md / outline-review.md / design.md 不是"持久化交付物 / ground truth / 答案 key"，而是**可复用快照（Reusable Reference Snapshot, RRS）**——每次 review 启动时 AI **重新执行** Step 0.3 流程从 C 源码独立推导，旧快照作为**前人理解参考**输入，产出**新版本快照**（`{NN}-design.v{N+1}.md`）。理由：固化即承诺"永远正确"是错的——错误会永久传播，连正式文档都在迭代，凭什么中间产物反而是"圣旨"？
- **Step 0 design + outline 预检** (**所有 review 模式强制，2026-07-16 扩；原"full/deep/design-first only"已废除**): at Step 0, run 4 `ls` commands + `tools/design-coverage-check.sh {module}`. Snapshots `.v*.md` existence check is mandatory for ALL review modes. Existing snapshots serve as "前人理解" reference input (NOT ground truth); AI must re-execute Step 0.3 each review, producing `.v{N+1}.md`. If no snapshot exists → **Gate H.1/H.6 FAIL → Step 0.3 嵌入生成**（不中断 review，2026-07-17 变更：原"阻断+触发附录 C"改为"Step 0.3 嵌入生成"）。不允许以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"为由跳过（**模式 69 PSMD + 71 DOG 触发**）。
- **Forbidden snapshot sources** (P0-process-violation): `tmp_design_and_todo/`, `/tmp/`, doc §3 inline design (**circular argument**: §3 is review target, cannot be snapshot source), unprefixed `design.md`/`outline.md` (must have `{NN}-` prefix), cross-doc snapshots (e.g. `02` reusing `01-design.md`), **treating old snapshots as ground truth** (snapshots are input, not output).
- **v2 产物区分**（方案 D 演进）：**可复用快照**（outline / outline-review / design）每次 review **重新执行 Step 0.3 流程**产出新版本（`.v{N+1}.md`），**保留所有历史版本**不覆盖——旧快照作为参考输入，新快照作为该轮 review 的依据。**脚手架产物**（structure / design-structure / SYMBOLS / VERIFY-CHECK）每次 review 重新生成，不复用。
- **structure.md 命名区分**：`-structure.md` = review 骨架（Step 0.5 产物，12 节分析）；`-design-structure.md` = design 前序（Step 0.3.1 产物，知识点全集）。两者内容完全不同，禁止混淆。`{NN}-outline.v{N}.md` 是可复用快照（带版本号），不是脚手架。
- **Step 0.5.3 doc ↔ outline 对齐检查**（方案 D 新增）：`outline.v{N}.md` 存在时，对照**最新版本快照**检查文档正文偏离（遗漏/多余/顺序错位），输出偏离矩阵。P0 偏离 = 核心概念遗漏。快照缺失 → Gate H.6 FAIL。
- **outline 阶段可参考来源**：✅ C 源码 + ✅ OS 理论 + ✅ Rust 代码（review 对象） + ✅ 旧快照（`.v{N}.md`，前人理解参考） + ❌ 现有文档 §3（被审对象，循环论证）+ ❌ 临时文件。outline 必须能从"C 源码 + OS 理论 + Rust 现状 + 旧快照参考"独立推导。
- **v2 快照演进规则**（替代旧"outline 演进规则"）：**默认每轮 review 都重新评估**，无例外。重大修改（文档/代码变化）**不构成跳过理由**——反而是重新评估的强信号。新快照必须基于 C 源码**当前状态**独立推导，差异矩阵（v{N} vs v{N+1}）写入 scan.md `§快照演进` 段。
- **⛔ 反查原则：不必然导向统一结果，可包含 Open Questions**（2026-07-16）：review 的最终产出不一定非得是一个确定的结果，也可以包含待讨论项。适用所有反查维度（outline ↔ 文档、design ↔ code、doc ↔ code、跨快照 diff 等）。三类判定：🔴 **直接判定**（AI 能判定哪个更好 → P0/P1/P2 + 推荐理由）；🟡 **Open Question**（AI 无法判定哪个更好 → ❓ OQ-N 上交用户，不阻塞 review）；🟢 **共识一致**（无差异 → 写入 `§反查一致项`）。**反"无脑一致"原则**：禁止"design ↔ code 不一致 → 直接 P0 改 code"——必须先判断哪个更好；若 AI 无法判定则标 OQ。Review 报告同时含 issue 清单（确定项）和 OQ 清单（待决项），两者独立。
- **Step 0.5.6 6 维反查矩阵**（新增，2026-07-16）：把"反查"作为 Step 0.5 的标准化方法，超越原 outline ↔ 文档单维检查。6 维度必跑：① outline ↔ 文档正文 ② design ↔ Rust 代码 ③ design ↔ Minix3 C 源码 ④ outline ↔ design ⑤ 文档 ↔ 代码（横向）⑥ 元层反查（v{N} vs v{N-1} + 跨文档契约）。反查覆盖率 = 6/6 必须 100%。
- **Step 0.5.7 章节意图分析**（新增，2026-07-16）：对反查中标为"多余"的章节，分析其教学意图（试图强调什么 / 试图解释什么），决定保留/删除/转 outline。每个多余章节必填「强调/解释/对齐/判定」4 项。
- **Step 0.5.8 issue 反查来源标注**（新增，2026-07-16）：scan.md 中每条 issue 必须标注反查维度来源（维度 1-6）。无来源标注的 issue 严重度降级。
- **跨 session 续审协议**（Resume Point）：单 session 上下文不够完成全量 review 时，scan.md 末尾必须写 Resume Point 段（当前断点 + 下一 session 启动协议 + 已发现 Issue 清单 + Step 执行清单）。详见 review-process.md §附录 A.2。
- **Three-tier terminology**: **Rewrite** (preserve external behavior) / **Refactor** (code Refactor or design Refactor, no semantic change) / **Architectural Evolution** (explicit `[ARCH: ...]` marker required in doc + design + code, three places consistent).
- **P0 has 6 categories** (was 5): P0-fact / P0-code-bug / P0-design-deviation / P0-design-missing / P0-design-wrong / P0-test-missing.
- **Conflict priority chain**: `Minix3 source behavior > design doc > Rust code > design/tech docs`. When Minix3, design, code, and tech docs conflict, follow this chain.
- **Gate H**: Design + outline alignment check (Step 1.6) — **所有 review 模式必检（2026-07-16 扩，原"only Profile R/C/I/H-K"已废除）**. Verifies doc/code matches design + design is complete + implementable + **outline.md exists + doc↔outline no P0 deviation** (H.6, 方案 D 新增). Failure → scan.md DRAFT. **不允许 N/A / [SIMPLIFIED] / "复用其他文档 design" 判定**（**模式 69 PSMD 触发**）。
- **⛔ Gate H 不允许 N/A 判定**（新增，2026-07-16；2026-07-17 更新）：每篇文档都必须通过 Gate H 全部 6 项检查。不允许"本文档复用其他文档 design，Gate H N/A"——这是 P0-process-violation。若本文档无专属 design.md，必须**执行 Step 0.3 嵌入生成**（不切换 Design-First，不中断 review）。
- **⛔ 禁止跨文档复用 design / outline**（新增，2026-07-16）：每篇文档必须有**本编号**的 `{NN}-outline.md` / `{NN}-outline-review.md` / `{NN}-design.md`（`{NN}` = 本文档编号，如 `02`）。不允许"02 文档复用 01-design.md"——design 是 per-doc 的契约，不是 per-module 或 per-stage 的。违反 → P0-process-violation。
- **Step 0.5.5 跨章节一致性检查**（新增，2026-07-16）：同一文档内多个章节描述同一事时（如 §3.5 和 §4.3 都列三架构差异表），必须保证内容一致。输出一致性矩阵（主题 / 章节 A / 章节 B / 一致? / 不一致字段 / 严重度），不一致按 P0/P1/P2 记录。
- **Step 0.7 TODO 验证**（新增，2026-07-16）：review 输入含外部 TODO 清单时，TODO 验证是 review 前置步骤，结果直接喂入 Step 2 差异提取，避免二次 grep。分类：❌ 误报 / ⚠️ 真实（代码任务）/ ✅ 真实（文档任务）。
- **深度模式 rounds 动态化**（新增，2026-07-16）：< 500 行→1 round | 500-1500 行→2 rounds（R1 正确性 + R2 卓越性）| > 1500 行→4 rounds。用户明确要求"深度全面 full-review"时按 2 rounds 起步。
- **Step 2 差异类型区分**（新增，2026-07-16）：Gate B 8 字段表区分两种差异——C→Rust 行为差异（C 函数行为 vs Rust 实现，8 字段全适用）vs doc↔code 描述差异（文档声称 vs 代码实际，"C 行为"字段改为"doc 声称"，C 证据为 N/A）。
- **Step 5.6 跨 agent 验证推荐**（新增，2026-07-16）：优先由不同 agent 执行 VERIFY-CHECK。同 agent 验证必须基于 grep 命令重放（非语义回忆），并在 VERIFY-CHECK.md 标注"同 agent 验证" + 验证局限说明段。
- **Step 0 分阶段时间预算**（新增，2026-07-16）：若执行 Step 0.7 TODO 验证，时间预算分两阶段——TODO 验证阶段（TODO 数 × 5 分钟）+ review 阶段（按文档行数查表）。偏差 >50% 需说明原因。
- **Review 修复 = 基于 outline 的部分重写**: review finding fixes are not patches but partial rewrites via the outline stage (see Appendix C.2.2). P0 概念/设计错误 → must update outline first, then rewrite body.

## Ground Truth Priority
```
Minix3 C source behavior > design contract > Rust implementation > design/technical documentation > AI analysis
```
When in doubt, grep `minix3/` and read the original C code.

## Documentation Structure
Each doc in `notes/rewrite/` follows:
- Ch1: Concepts & Minix3 context (concept-driven, WHY→WHAT→HOW; multi-arch unified abstraction first)
- Ch2: Full C source analysis (functions, structs, macros)
- Ch3: Rust design decisions (WHY, with alternatives & rationale)
- Ch4: Rust implementation details (HOW)
- ChN-1: Test points
- ChN: See-also references

**Ch1&2 must cover ALL C symbols in the semantic scope. Ch3&4 must implement ALL semantics described in Ch1&2.**

## Review System

The review system enforces structured review via 9 domain skills (in `prompt/skill/`, adapted to `.trae/skills/` and `.codex/skills/`). Claude Code Runtime uses the `review-scan` orchestrator (`.claude/skills/review-scan/`) + `review-implementation-skill` (`.claude/skills/review-implementation-skill/`), with always-on rules in `.claude/rules/`. Full process details: `prompt/skill/review-process-skill.md`. The 9th skill `review-implementation-skill` (added 2026-06-22 from the 06-design.md/06-design-final.md implementation) verifies design ↔ code consistency, tracks §X self-review issues, and enforces backward-compatible refactor + test coverage boundary.

### 任务命令（review-cmds，2026-09-05 新增）

6 个独立 cmd 是任务的一级入口（单一目标 + 明确边界 + 章节级默认范围——注意力是 review 质量的第一约束）。规范源 `prompt/review-rules/review-cmds.md`；薄壳 skill 注册于 `.agents/skills/`（软链，ZCode 原生扫描）与 opencode.json（muse/opencode 可直呼）：

| cmd | 用途 |
|-----|------|
| full-review | 文档+代码全面 review+修复+覆盖度（range/dir 走 review-scan 编排器） |
| style-fix | 文档文风与教学性修复（去开发文档味、章节重组，for 读者） |
| code-excellence | 代码卓越度 + 死代码消除（多方案对比、[ARCH] 三处一致） |
| test-audit | 测试 5 维专项（完备/自身正确/冗余/无效/虚构对账） |
| todo-fix | 修一个 TODO（DEFERRED 不算修；一次一个） |
| style-bible | 文风宪法（禁黑话/缩写/文言文；锚点纪律） |

通用强制门（任何 cmd 不可裁剪）：锚点纪律门（模式 83）、测试名对账门（Gate E）、文风门、translate 防线（模式 16/65）、fix-guard、文档-代码同步。旧 Profile A-P/R/AG 保留为别名（对账表 review-cmds.md §八）；新任务一律用 cmd 入口。

### ⛔ Explicit Skill Invocation
You MUST invoke Skill tools explicitly via the available `Skill` function. NEVER rely on "rules already loaded" or "context already has it". The Skill Invocation Log in scan.md must reflect actual Skill tool calls, not planned/intended calls.

### Blocker Gates (must pass before Final Review)
- **Gate 0**: Artifact inventory — standard paths complete (STATE/scan/structure/SYMBOLS); scan.md contains **9** grep-verifiable anchor sections (Skill Invocation Log / Blocker Gates Status / **Step 0: 预检结果** / Step 1 / 1.5 / 2 / 3.5 / Issue List / Artifact Inventory). Missing `Step 0: 预检结果` section → Gate 0 FAIL + **模式 69 PSMD 触发**.
- **Gate A**: Coverage enumeration — `coverage-extract.py` executed + SYMBOLS.md path in scan.md + `gate-evidence-A` block with command + stdout
- **Gate B**: Diff extraction — Top 5 behavior contract table (3 语义偏移 + 2 覆盖缺口, **8 fields × 5 funcs**)
- **Gate C**: Precision check — 5 meta-rules check table output
- **Gate D**: P0 mandatory checklist — 5 items answered (✅/❌ + grep evidence); **PARTIAL/⚠️/"部分通过" = ❌ FAIL**
- **Gate D-6**: structure.md skeleton review (doc review only) — structure.md generated + 12-section review table + failures in Issue List
- **Gate E**: Test verification — §5 each test function grep-verified (if doc has §5)
- **Gate G**: VERIFY-CHECK independent validation — VERIFY-CHECK.md produced + verdict PASS (consistency ≥ 90%); CONCERN/FAIL may NOT mark CONVERGED
- **Gate H**: Design + outline alignment check (Step 1.6) — **所有 review 模式必检（2026-07-16 扩）**. Verifies doc/code matches **最新版本快照** (`.v{N}.md`) + snapshot is complete + implementable + **outline.v{N}.md exists + doc↔outline no P0 deviation** (H.6, 方案 D v2); P0-design-missing/wrong/deviation must be fixed or IN_DESIGN-tagged before CONVERGED. **⛔ Gate H 不允许 N/A 判定**：若本文档无专属快照（任何版本都不存在），必须**执行 Step 0.3 嵌入生成** `.v1.md`（不切换 Design-First，不中断 review），不得标 N/A 跳过（违反 = P0-process-violation）。

Any Gate failed → scan.md marked DRAFT, STATE.md not updated. **"✅ Gate passed" without attached command+output evidence is invalid.** Gate evidence strength: L1 (tool output, required for A/D/E), L2 (manual grep, acceptable for B/C), L3 (inference, treated as FAIL unless `MANUAL_FALLBACK` justified).

### structure.md (Doc Review Mandatory, Step 0.5)
Before correctness checks, generate `structure.md` (12-section skeleton analysis) to verify "what the reader reads" (orthogonal to correctness which verifies "what the doc says"). Sections: 主题思想/目标读者/叙事主语/驱动方向/文档大纲/核心概念清单/跨架构统一抽象/双向闭环/叙事弧/元注释/裸概念复述/纵向链路映射. Template: `prompt/skill/review-process-skill.md` §Step 0.5.

### P0 Mandatory Checklist (Gate D, see `prompt/skill/review-patterns-skill.md` §0)
1. §5 tests exist: `rg "fn {test_name}" {rust_dir}` — missing → P0
2. trait has ≥2 行为不同的 impl: `rg "impl.*{TraitName}" {rust_dir}` — 0 impl → P0（死代码/虚构 trait）；1 impl → P1（trait 抽象需 ≥2 行为不同的实现）；≥2 impl → ✅
3. function in declared file: `rg "fn {name}" {file}` — not found → P0
4. core algorithm not stub: `rg "todo!|unimplemented!|unreachable!|panic!" {rust_dir}` — stub → P0; `panic!` in non-test code that represents unimplemented functionality or a reachable unhandled path → treat as stub/unhandled path, must be justified in comment
5. §4 signatures match: compare doc §4 vs actual — mismatch → P0

### Key Patterns (48-57, narrative & concept)
- **48 因果链编造 (P0)**: claim correct but causal explanation technically wrong
- **49 元注释泄漏 (P1)**: author narrates writing strategy in body text (>5 → P1)
- **50 架构范围未标注 (P1)**: x86-specific mechanism told as common
- **51 实现驱动概念章 (P1)**: Ch1 subject is function name, not CPU/OS
- **52 单向心智模型 (P1)**: entry mechanism only covers entry, not return
- **53 跨架构共性未提取 (P1)**: multi-arch doc has no unified abstraction
- **54 视角漂移 (P2)**: subject switches within same chapter
- **55 架构特有机制喧宾夺主 (P2)**: arch-specific legacy overshadows core

### Key Patterns (66-78, review process, NEW 2026-07-16/17/30/31/08-14)
- **66 RCPD** (Reference Code Path Drift): TODO 描述引用的 `file:line` 已不存在 → 模式 66 + Step 0.7.1 path existence validation
- **67 CFNOC** (C Function Name vs OS Concept Confusion): 把 C 函数名误读为 OS 概念对象 → Step 0.7.2 AI claim grep verification
- **68 DSC** (Doc Section Confusion): 把 Ch2 C 展示误判为 Ch4 Rust 实现问题 → Step 0.7.3 doc chapter context awareness
- **69 PSMD** (Per-doc Snapshot Missing): per-doc design/outline 快照缺失但 scan.md 标 CONVERGED → Step 0 hard block + `tools/design-coverage-check.sh`
- **70 CTOS** (Cross-Turn Outdated Staleness): TODO 列表跨轮状态陈旧（37.5% 误报率）→ Step 0.7.4 TODO staleness check
- **71 DOG** (Decision Over-Generalization): 把"X 文档豁免"用户决策泛化到"Y/Z 文档" → STATE.md §豁免列表 + 不可泛化原则
- **72 CSSCM** (Cross-Section Step Count Mismatch, NEW 2026-07-17): §2 C 步骤数 ≠ §4 Rust 步骤数且无差异说明表 → P1
- **73 Doc Code Example Rust 2024 Edition Drift** (NEW 2026-07-30): 文档代码示例用 `static mut`（Rust 2024 已弃用），实际代码已迁移至 `Atomic*` / `UnsafeCell`；或文档路径与实际 `find` 结果不一致（目录重组）→ P1。检查命令：`rg "static mut" {doc}.md` + `rg "static mut" os/ -t rust` + `find os/{dir} -name X.rs`。详细规则见 `prompt/skill/review-patterns-skill.md §模式 73`。
- **74 Doc Path Convention Drift** (NEW 2026-07-30): 文档 Rust crate 路径引用漏 `os/` workspace 根前缀（典型：`kernel/src/...` 应为 `os/kernel/src/...`），与 doc 01 等早期 doc 跨文档不一致 → P1。检查命令：`rg "kernel/src/" {doc}.md | wc -l` > 0（应仅匹配 minix3 C 源路径）+ `rg "os/kernel/src/" {doc}.md | wc -l` 低。修复：`sed -i 's|kernel/src/|os/kernel/src/|g'` + `sed -i 's|os/os/|os/|g'`（避免双重前缀）。详见 `.claude/rules/review-process.md §Step 1.0c` + `prompt/skill/review-patterns-skill.md §模式 74`。

### ⛔ Step 0 硬阻断（所有 review 模式强制，NEW 2026-07-16；2026-07-17 更新）
- 每次 review 启动时**必须**执行 4 条 `ls` + `tools/design-coverage-check.sh {module}`
- `outline.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.2 嵌入生成**（不中断 review）
- `outline-review.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.3 嵌入生成**（AI 自审）
- `design.v*.md` 缺失 → **Gate H.1 FAIL** → **Step 0.3.4 嵌入生成**（不中断 review）
- 不允许以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"为由跳过
- 豁免必须登记在 STATE.md `§豁免列表` 段，**不可泛化**（模式 71 DOG）
- **56 决策日志体 Ch3 (P1)**: Ch3 lists decisions without rationale
- **57 例子前置知识泄漏 (P2)**: example introduces unrelated details

### ⛔ Doc Code Sync 强制工作流（NEW 2026-07-30）

> **背景**：本次 review (01-boot-shim-bootstrap) 发现 3 P1 doc-code 不一致（虚构常量 / 路径错误 / `static mut` 过时）—— 文档维护漂移是 2024-2025 年的活跃问题。

**强制检查项**（任何 doc review 必跑）：

| # | 检查项 | 命令 | 触发 P1 |
|---|--------|------|---------|
| **Doc-Sync-1** | 文档代码示例含 `static mut` | `rg "static mut" {doc}.md --type md` | 实际代码已用 `Atomic*` / `UnsafeCell`（pattern #73）|
| **Doc-Sync-2** | 文档路径与 `find` 不一致 | `find os/{dir} -name X.rs` vs `rg "path" {doc}.md` | 目录重组后 doc 未更新（pattern #73b）|
| **Doc-Sync-3** | 虚构常量（C 源码中不存在）| `rg "{MACRO_NAME}" minix3` | 1.0 → 0 hits（pattern #73 / 67）|
| **Doc-Sync-4** | 文档内部 TODO 未走流程 | `rg "^\s*>\s*\*\*TODO" {doc}.md` | 多个 TODO 标记未分类（Step 0.7.2）|
| **Doc-Sync-5** | C 源码行号主动抽样 | `sed -n 'N,Mp' {c_file}` | doc 行号范围与实际不符（Step 1.0a）|
| **Doc-Sync-6** | 文档路径缺 `os/` 前缀 | `rg "kernel/src/" {doc}.md` vs `rg "os/kernel/src/" {doc}.md` | 跨文档路径风格不一致（pattern #74，Step 1.0c）|
| **Doc-Sync-7** | 测试数量声称偏差 >50% | `rg "约 \d+|总计.*\d+" {doc}.md` vs `cargo test` | doc undercount（Step 4.5a）|

**工作流**：
1. Step 1.0a：抽取 doc 中 5-10 个 `file:line` 引用，**主动**用 `sed` 验证
2. Step 1.0b：扫描 doc 代码块，对比实际 Rust 代码 idioms（pattern #73）
3. Step 1.0c：扫描 doc 路径约定，与早期 doc 跨文档一致性（pattern #74）
4. Step 4.5a：扫描 doc 测试数量声称，与 `cargo test` 实际对比
5. Step 0.7.2：扫描 doc 内部 TODO 标记（不依赖外部 `tmp_design_and_todo/`）
6. 修复顺序：P1（P0 安全/内存）→ P1 doc-code 漂移 → P2 → backlog

**详细规则**：
- `.claude/rules/review-process.md §Step 1.0a`（行号主动抽样）
- `.claude/rules/review-process.md §Step 1.0b`（Rust 代码示例同步扫描）
- `.claude/rules/review-process.md §Step 1.0c`（doc path convention 一致性，NEW）
- `.claude/rules/review-process.md §Step 4.5a`（测试数量偏差检查，NEW）
- `.claude/rules/review-process.md §Step 0.7.2`（doc 内部 TODO 扫描）
- `.claude/rules/review-core.md §VERIFY-CHECK 同 Agent 局限`

### ⛔ VERIFY-CHECK 同 Agent 局限（NEW 2026-07-30）

单 session 深度 review 中，VERIFY-CHECK 不可避免是同 agent 验证（同一 LLM 可能继承同样的 bias）。规则：

1. **同 agent 时显式标注**：scan.md / VERIFY-CHECK.md 必须显式写"⚠️ 同 agent VERIFY-CHECK（已重放 grep 命令）"
2. **跨 agent 优先**：条件允许时优先由 Trae ↔ Claude 互为 cross-verification
3. **同 agent 不给 false confidence**：
   - consistency < 90% → 明确写"❌ NOT PASS"，不要模糊表述
   - 修后再跑（post-fix）必须能提升 consistency 到 ≥ 90%
4. **基于 grep 命令重放**：每个验证项附实际执行的 grep + 输出，可由读者手动重放

详见 `.claude/rules/review-core.md §VERIFY-CHECK 同 Agent 局限`。

### Rule Evolution (Rule Discovery, Step 5.7)
Every scan.md MUST include a `§Rule Discovery` section answering: "Did this review discover a new pattern? ✅/❌". If ✅, propose a new pattern (name/case/severity/category/draft rule/target file). This makes the rule set self-evolving — patterns discovered in one review feed back into the rules for the next review. See `prompt/skill/review-process-skill.md` §Step 5.7.

### Convergence Cost Warning (Step 7.1)
To prevent over-convergence (chasing P1→0 across many rounds), stop and deliver when ANY of these trigger:
1. Same doc reviewed ≥5 rounds → force deliver, remaining P1/P2 → backlog
2. Two consecutive rounds with new P1 ≤ 1 → converged, remaining P1/P2 → backlog
3. Current round cost >80% of previous but new findings <20% → stop
4. **首次零发现（漏检自检，NEW 2026-08-14）**：首次 review 0 P0/P1/P2 → 触发**漏检自检**（随机抽 3 个检查项重跑，如 Gate D 第 1/3/5 项 + Step 2 因果链抽样）；若仍 0 发现 → 交付；若发现遗漏 → 之前的 review 标记 DRAFT，补完后再交付。（原"review 即 PASS，强制交付"已废弃）

See `prompt/skill/review-process-skill.md` §Step 7.1.

### Review 累积改进（NEW 2026-07-31）

9 次 review（Doc 01/02/03/04/05/06/07/08/09）累计发现 **5 个新模式 + 17 个新 Step + 11 个 Proposal + 3 个里程碑**（zero-bias + 三 snapshot 全齐 + Perfect Link）：

| 维度 | 状态 | 来源 |
|------|------|------|
| **Pattern #73 Doc Code Example Drift** | ✅ 已落地（prompt/skill + .trae + .claude 三方同步） | Doc 01 (07-30) |
| **Pattern #74 Doc Path Convention Drift** | ✅ 已落地（同上） | Doc 02 (07-30) |
| **Pattern #75 Doc See-Also Range Drift** | ✅ 已落地（prompt/skill + review-process Step 1.0d） | Doc 04 (07-31) |
| **Pattern #76 Cross-Doc Attribution Drift** | ✅ 已落地（prompt/skill + review-process Step 1.0e） | Doc 05 (07-31) |
| **Pattern #77 Code Comment Line Drift**（NEW 2026-07-31）| ✅ 已落地（prompt/skill + review-process Step 1.0f） | Doc 08 (07-31) |
| **Step 1.0a 行号主动抽样** | ✅ 已落地 | Doc 01 (07-30) |
| **Step 1.0b Rust 习惯用法同步** | ✅ 已落地 | Doc 01 (07-30) |
| **Step 1.0c Path Convention** | ✅ 已落地 | Doc 02 (07-30) |
| **Step 1.0d 参见范围扫描** | ✅ 已落地（Pattern #75 配套） | Doc 04 (07-31) |
| **Step 1.0e 代码注释 doc 归属交叉检查** | ✅ 已落地（Pattern #76 配套） | Doc 05 (07-31) |
| **Step 1.0f 代码注释行号漂移检查**（NEW 2026-07-31）| ✅ 已落地（Pattern #77 配套） | Doc 08 (07-31) |
| **Step 0.5.2 元注释章节 review** | ✅ 已落地 | Doc 04 (07-31) |
| **Step 0.7.2 doc 内部 TODO** | ✅ 已落地 | Doc 02 (07-30) |
| **Step 4.5a 测试数量偏差** | ✅ 已落地 | Doc 02 (07-30) |
| **Step 7.1.1 修复成本估算** | ✅ 已落地 | Doc 01 (07-30) |
| **VERIFY-CHECK 同 Agent 局限** | ✅ 已落地 | Doc 01 (07-30) |
| **Step 0.3.3 outline-review 嵌入生成** | ✅ 首次+第二次+第三次+第四次触发（Doc 04/05/06/07） | Doc 04-07 (07-31) |
| **§2.4g const 权威位置检查** | ✅ 已落地（review-doc-skill） | Doc 05 (07-31) |
| **§2.4h 代码注释 doc 归属交叉检查** | ✅ 已落地（review-doc-skill，Pattern #76 配套） | Doc 05 (07-31) |
| **§2.4i L3 grep 主动验证**（NEW 2026-07-31）| ✅ 已落地（Doc 07 §5.4 6/6 stub 行号验证成功） | Doc 06 → Doc 07 |
| **§2.4j 测试总数末段补充**（NEW 2026-07-31）| ✅ 已落地（Doc 07 末段 29/120 tests 补充） | Doc 06 → Doc 07 |
| **Step 1.0a-自动 反向偏移自动重算** | ⏸ Proposal #7 增强 | Doc 06 (07-31) |
| **Step 7.1 触发停止规则 3**（NEW 2026-07-31）| ✅ 已应用（Doc 07 review 即触发） | Doc 07 (07-31) |
| **Pattern #66 RCPD doc 主动应用**（NEW 2026-07-31）| ✅ Doc 07 §5.4 显式应用 | Doc 07 (07-31) |
| **Proposal #7 自动化行号校验脚本** | ⏸ 待用户确认后开发 `tools/review-line-check.sh`（**增强**：反向偏移自动重算） | Doc 03 (07-31) |
| **Proposal #8 测试数量准确性机制** | ⏸ doc §5 强制格式 + CI 钩子（**增强**：测试总数末段补充） | Doc 03 (07-31) |
| **Proposal #9 Double-check 关键 Finding** | ✅ 已应用（修复前必验证） | Doc 03 (07-31) |
| **Proposal #10 Pattern #75 Doc See-Also Range Drift** | ✅ 已落地 | Doc 04 (07-31) |
| **Proposal #11 Pattern #76 Cross-Doc Attribution Drift** | ✅ 已落地 | Doc 05 (07-31) |
| **Proposal #12 design.md §X-Y "权威位置"段** | ⏸ 待用户确认后落地 | Doc 05 (07-31) |
| **Proposal #13 Step 1.0a-自动 反向偏移自动重算** | ⏸ Proposal #7 增强 | Doc 06 (07-31) |
| **Proposal #14 L3 grep 主动验证（§2.4i）** | ✅ 已落地（Doc 07 验证成功） | Doc 06 → Doc 07 |
| **Proposal #15 测试总数末段补充（§2.4j）** | ✅ 已落地（Doc 07 验证成功） | Doc 06 → Doc 07 |
| **Proposal #16 Step 7.1 触发停止规则 3**（NEW 2026-07-31）| ✅ 已应用 | Doc 07 (07-31) |
| **Proposal #17 Pattern #66 RCPD doc 主动应用**（NEW 2026-07-31）| ✅ Doc 07 显式应用 | Doc 07 (07-31) |
| **Proposal #18 Step 0.3.3 outline-review 批量补齐**（NEW 2026-07-31）| ⏸ 待用户确认后统一触发 user-confirmed review | Doc 07 (07-31) |
| **Proposal #19 Pattern #77 Code Comment Line Drift**（NEW 2026-07-31）| ✅ 已落地 + Step 1.0f 已加 review-process.md | Doc 08 (07-31) |
| **Proposal #20 Step 1.0f 代码注释行号漂移检查**（NEW 2026-07-31）| ✅ 已落地（Pattern #77 配套） | Doc 08 (07-31) |
| **Step 1.0f 主动验证**（NEW 2026-07-31）| ✅ 已应用（Pattern #77 检查无新 drift） | Doc 09 (07-31) |

**关键洞察**：
1. **Pattern #74 跨文档传播继续有效**：Doc 02 → Doc 03/04/05/06/07 自然合规（16/67/31/33/16 处 `os/` 前缀，0 双重前缀）
2. **Pattern #75 暴露 Step 1.0a 漏检**：单点行号抽样 vs 范围引用抽样——本次 Doc 04 漏检 L831/L883 顺带修复
3. **Pattern #76 暴露 Step 1.0a/b/c/d 漏检**：代码注释 doc 归属系统性过时——本次 Doc 05 发现 3 处 `(covered in NN)` + 9+ 处 `see XX-doc.md`（旧 doc 命名）→ 已加 Step 1.0e + Pattern #76
4. **测试数量 undercount 系统性**：4 个 doc 都存在（4.2×/1.6×/1.8×/5.2%），需建立机制（Proposal #8 + #15 增强 → Doc 07 已落地）
5. **行号漂移是结构性弱点**：6 个 doc 累计 25+ 处 P2 偏移（含范围引用漂移 + 反向偏移），需自动化（Proposal #7 + #13 增强）
6. **"安静的"doc 可能反映 review 走流程不深入**：Doc 03 0 P1 不一定意味着 doc 完美——下次对 0 P1 doc 做反向抽查（Proposal #9）→ **Doc 07 验证：0 P1 不等于 review 不深入，而是累积改进极致效果**
7. **元注释章节盲点**：Doc 04 §11.1-§11.6 主动验证通过——已加 Step 0.5.2 元注释章节 review 标准化
8. **Step 0.3.3 嵌入生成连续 4 次触发**：Doc 04/05/06/07 outline-review.v*.md 都缺失，自审路径工作正常（Gate H.6 修复）—— 但**自审不构成用户确认**，后续需 user-confirmed review 复核（建议批量补齐，Proposal #18）
9. **代码注释 doc 归属系统性过时**：本次 Doc 05 发现 `(covered in NN)` 注释错位（3 处）+ `see XX-doc.md` 引用旧 doc 命名（9+ 处）—— **之前 4 次 review 都未深入代码注释交叉**
10. **跨 crate const 重复定义盲点**：DEFAULT_HZ 在 os/arch + os/kernel 两 crate 独立定义（不会编译错误）→ 已加 **"权威位置"段说明**（Proposal #12）
11. **行号反向偏移是系统性问题（NEW）**：本次 Doc 06 发现 6 处反向偏移（-7/-9/-13），代码增量后 doc 未同步 → **Step 1.0a-自动 + auto_resync_line**（Proposal #13 增强 Proposal #7）
12. **§5.4 L3 grep 证据未主动验证（NEW）**：本次 Doc 06 review 依赖 doc 自证 → **L3 grep 主动验证（§2.4i）**（Proposal #14 → Doc 07 验证成功）
13. **测试总数末段补充（NEW）**：本次 Doc 06 无测试总数声称但实际 610 tests 通过 → **测试总数末段补充（§2.4j）**（Proposal #15 → Doc 07 验证成功）
14. **Zero-bias milestone（NEW, Doc 07）**：7 次 review 中**首次** 0 P0/P1/P2（review 即 PASS）—— 累积改进极致效果 + **触发 Step 7.1 停止规则 4**（**2026-08-14 更新为漏检自检**：原"强制交付"已废弃，改为抽 3 项重跑确认无遗漏再交付）
15. **Doc 主动应用 Pattern #66 RCPD（NEW, Doc 07）**：doc 07 §5.4 显式声明"不引用 syscall_copy.rs 行号避免漂移传播（Pattern #66 RCPD）"—— **首个 doc 主动标注已应用 review pattern**，doc 作者已具备 review pattern 意识
16. **代码注释 doc 复述传递性 drift（NEW, Doc 08）**：本次 08 review 发现 3 处 P2 行号偏移全部源自 `lib.rs:1170/1181/1193` 的代码注释错误（不是 doc 错）—— doc §4.6 复述了错误注释。**根因诊断**：必须修代码注释（root cause）+ 同步修所有复述的 doc（避免传递性 drift）→ **Pattern #77 + Step 1.0f**（Proposal #19/#20，已落地）
17. **首个 doc 三 snapshot 全齐（NEW, Doc 08）**：08 review 是 8 次 review 中**首个** tool scan ✅ PASS（outline + outline-review + design 都真实存在）—— 卓越设计维护的标志 + 不需要 Step 0.3.3 嵌入生成
18. **Zero-bias 里程碑 #2 + Perfect Link 首次（NEW, Doc 09）**：09 review 是 9 次 review 中**第二个** zero-bias（继 Doc 07 后）+ **首次 perfect line refs（16/16 全行号零偏差）+ 首次 perfect vertical link（15/15 全链路完整）**—— 累积改进极致效果
19. **Step 1.0f 主动验证首次应用（NEW, Doc 09）**：Pattern #77 检查无新 drift（仅 backlog 9+ 处旧 doc 命名）—— 累积改进已固化为系统自动检查
20. **Hidden Folder Convention + forward reference 验证（NEW, Doc 10）**：10 review 首个完全合规 doc（无 design/tmp_design 引用）+ §4.1 trap_return.rs forward reference 透明声明（合规）+ **Step 1.0g 新增**（forward reference 验证机制）

**详见**：`.claude/rules/review-core.md §Review 累积改进追踪`。

### Skill Invocation Log (mandatory in scan.md)
Every scan.md MUST include a Skill Invocation Log table (Skill name, 调用时机, 关键产出). Missing → scan.md DRAFT.

### State Management: Dual-Path (Trae vs Claude)
- **Trae IDE** → `.review/trae/{module}/STATE.md` (project root `.review/`, not inside `notes/`)
  - Per-doc/cross-AI scans: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
  - Bagging aggregate: `.review/trae/{module}/scans/AGGREGATED-{doc-stem}.md`
- **Claude Code Runtime** → `.review/claude/{module}/STATE.md` (project root `.review/`)
  - Module-level scan: `.review/claude/{module}/{doc-stem}/scan.md`
  - Verification: `.review/claude/{module}/VERIFY-CHECK.md`
- **No shared intermediate results** between tools: STATE/scan/SYMBOLS/structure/VERIFY-CHECK are isolated. Bagging aggregation happens only inside Trae (multi-AI scan merge). Cross-tool divergence must NOT be auto-merged; log it in scan.md and ask the user.
- `{module}` = first directory under `notes/rewrite/` (e.g. `fork-syscall-rewrite`). `{doc-stem}` = target doc basename without extension (e.g. `03-kmain-cstart`). `{agent}` = model id (Trae: glm/kimi/...; Claude: m3/...).
- Recommended: `tools/review-init.sh claude {doc-path}` auto-computes `{module}`/`{doc-stem}` and creates standard directories.

### Intermediate Artifacts
- `STATE.md` — review progress (tool-specific path, see above)
- `SYMBOLS.md` — coverage enumeration (machine-generated + AI judgment)
  - Trae doc-level: `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
  - Claude doc-level: `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
- `structure.md` — skeleton analysis (doc review, 12 sections, saved alongside scan.md)
- `scan.md` — single-file aggregation of all dimension results (NOT 10 separate check files)
  - Trae: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
  - Claude: `.review/claude/{module}/{doc-stem}/scan.md`
  - If user explicitly requests another output location, **dual-write** to user-specified path + tool default path (Trae interactive fix doc: `notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`; Claude report: `{doc-stem}-claude-report.md`).
- `VERIFY-CHECK.md` — independent validation result (mandatory before CONVERGED)
  - Trae: `.review/trae/{module}/VERIFY-CHECK.md`
  - Claude: `.review/claude/{module}/VERIFY-CHECK.md`
