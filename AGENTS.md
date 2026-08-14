# Minix-RS — Codex 项目指令

Minix3 kernel modules rewritten in Rust (x86-64, no_std). Not a translation — a semantic rewrite preserving external behavior while using Rust's type system internally.

> **注意**：完整规范在 `CLAUDE.md`（项目指令，check-in 到代码库）。Codex 不读取 CLAUDE.md，本文件是其 Codex 入口。**任何 review / 修复工作开始前，先读 `CLAUDE.md` + `.claude/rules/review-core.md` + `.claude/rules/review-process.md` + `.claude/rules/fix-guard.md`** —— 这些是 review 工作流的规范源，所有 review skill 都依赖它们。

## Build & Test

- Build: `cargo build`（workspace 根在 `os/`）
- Test: `cargo test`
- Lint: `cargo clippy`

## Directory Layout

```
minix3/              — original Minix3 C source (ground truth, do NOT modify)
os/servers/vm/       — VM server Rust rewrite
os/libs/minix-types/ — shared IPC types, constants, codec traits
notes/rewrite/       — documentation (one .md per C source concept)
prompt/              — review rules, skill definitions (source of truth for .claude/ + .trae/ + .codex/)
.claude/             — Claude Code runtime: rules + skills (derived from prompt/)
.trae/               — Trae IDE skills (derived from prompt/)
.codex/              — Codex skills (derived from prompt/skill/ + .claude/skills/)
```

**Hidden Folder Convention（NEW 2026-07-31）**：`design/` 和 `tmp_design_and_todo/` 文件夹视为中间产物，正式文档绝不引用（引用即 P0-process-violation）。正式 doc 引用应使用绝对路径到 doc、代码、C 源。

## Key Constraints

- `#![no_std]` everywhere except `#[cfg(test)]`
- Error types must map to Minix3 errno values — no self-invented error codes
- Hardware is abstracted behind traits — never expose CR3/PTE bits to OS layer
- **Execution Model by module type**：用户态服务器（VM/PM/VFS 等）= 单线程事件循环，`!Send`/`!Sync`/`Rc`/`RefCell` 合理；**Kernel = SMP + BKL（Big Kernel Lock spinlock）**，共享数据需 `Arc`+`Mutex`/`Atomic`，禁止 `Rc`/`RefCell` 跨 CPU；BKL 是 spinlock：临界区内不得 sleep/schedule/IPC
- **Ground Truth Priority Chain**：`Minix3 C source > design doc > Rust code > design/tech docs`。不确定时 grep `minix3/` 读原始 C 代码
- **Three-tier terminology**：Rewrite（保持外部行为）/ Refactor（不改语义的重构）/ Architectural Evolution（必须三处一致标注 `[ARCH: ...]`：doc + design + code）
- **P0 六类**：P0-fact / P0-code-bug / P0-design-deviation / P0-design-missing / P0-design-wrong / P0-test-missing

## Review Workflow（Codex 用）

> **⛔ 开始任何 review 前**：先读 `CLAUDE.md` + `.claude/rules/review-core.md` + `.claude/rules/review-process.md` + `.claude/rules/fix-guard.md`。这些规则文件同时约束 Claude Code / Trae / Codex 三端，内容以 `.claude/rules/` 为准。

1. **声明 scope**：目标文件、模式（doc/code/full）、STATE.md 状态
   - State 路径：`.review/trae/{module}/STATE.md`（Trae IDE）vs `.review/claude/{module}/STATE.md`（Claude Code），两工具隔离，不共享中间产物。Codex 不写入这些路径；如需记录，写 scan.md 到用户指定位置
2. **Step 0 硬阻断预检**（所有 review 模式强制）：跑 4 条 `ls notes/rewrite/{module}/{stage}/.design/{NN}-*.v*.md` + `tools/design-coverage-check.sh {module}`；`outline.v*.md` / `outline-review.v*.md` / `design.v*.md` 缺失 → Gate H.6/H.1 FAIL → Step 0.3 嵌入生成（不中断 review，不标 N/A）。缺失却标 CONVERGED → P0-process-violation（模式 69 PSMD）
3. **Blocker Gates**（0/A/B/C/D/D-6/E/G/H）：每个 Gate 必须附实际命令 + 输出证据（`gate-evidence-{X}` 块）。Gate 失败 → scan.md 标 DRAFT，STATE.md 不更新
4. **产出**：scan.md（单文件汇总，含 Skill Invocation Log / Blocker Gates 状态 / Step 0 预检结果 / Issue List）+ structure.md（doc review，12 节骨架）+ SYMBOLS.md（覆盖率枚举）+ VERIFY-CHECK.md（独立验证，consistency ≥ 90% 才可 CONVERGED）
5. **修复原则**：P0 全部修完才可 CONVERGED；doc 与 code 保持同步；遵守 fix-guard.md（修复前读目标行 ±5 行、grep 确认、单条修复、写 fix-status）
6. **收敛停止规则**（Step 7.1）：≥5 轮强制交付 / 连续两轮新 P1 ≤ 1 视为收敛 / 成本>80% 而新发现<20% 停止 / 首次 review 0 P0/P1/P2 → 立即交付
7. **每次 review 必须回答 Step 5.7 Rule Discovery**（是否发现新模式）——规则集自演进机制

## Skills（10 个，见 `.codex/skills/`）

> 用法：每个 skill 在 `.codex/skills/{name}/SKILL.md`，用 `{baseDir}/.codex/skills/{name}/SKILL.md` 引用。`name` 字段必须等于目录名。

| Skill | 文件 | When to use |
|-------|------|-------------|
| review-scan | (file: .codex/skills/review-scan/SKILL.md) | 编排器：对 notes/rewrite/ 目录做全量 review（coverage + doc + code + patterns + excellence），写 review report + 收敛状态。用户说 "review/scan/check 一个文档目录" 时用 |
| review-process-skill | (file: .codex/skills/review-process-skill/SKILL.md) | Review 执行流程：Step 0-7、Blocker Gates、中间产物格式。进入 review 执行阶段时用 |
| review-core-semantics-skill | (file: .codex/skills/review-core-semantics-skill/SKILL.md) | 核心语义定义 + 行为契约表模板（8 字段）。Step 2 Diff Extraction 识别 Top 5 语义差异时用 |
| review-doc-skill | (file: .codex/skills/review-doc-skill/SKILL.md) | 文档 Review 检查清单（Ch1 骨架 / Claims-Evidence / 概念准确性 / 文档-代码一致性等）。检查 .md 文档质量时用 |
| review-code-skill | (file: .codex/skills/review-code-skill/SKILL.md) | 代码 Review 检查清单（Rewrite 质量 / 硬件抽象 / 类型安全 / SMP 并发 / no_std 等 14 维度）。检查 .rs 代码质量时用 |
| review-patterns-skill | (file: .codex/skills/review-patterns-skill/SKILL.md) | 常见错误模式库（P0 必检 5 项 + 文档/代码/测试/叙事 60+ 模式，含验证命令）。review 中对照典型错误时用 |
| review-excellence-skill | (file: .codex/skills/review-excellence-skill/SKILL.md) | 卓越性检查（正确性 gate 通过后）：教科书级文档 + redox 级代码 |
| review-coverage-skill | (file: .codex/skills/review-coverage-skill/SKILL.md) | 覆盖率穷举：tools/coverage-extract/ 生成 SYMBOLS.md + AI 语义判断。检查 C 源码/Rust 实现覆盖完整性时用 |
| review-socratic-skill | (file: .codex/skills/review-socratic-skill/SKILL.md) | 苏格拉底追问：review 发现可疑点无法判定时，通过提问引导用户澄清/提供证据（12 场景话术模板） |
| review-implementation-skill | (file: .codex/skills/review-implementation-skill/SKILL.md) | 设计→实施 验证：验证 Rust 代码正确实现 design doc + 追踪自我审查问题清单 |

> **一致性约定**：`.codex/skills/` 从 `prompt/skill/`（源）+ `.claude/skills/` + `.trae/skills/` 派生，正文与 .trae 版保持一致（仅 description 按 Codex 200 字符限制精简）。源文件变更后需同步三方。`review-agent-ide` / `review-agent-trigger` 是 agent 定义（无 frontmatter），Codex 无 agent 概念，不复制。

## Fix Phase（修复规范）

1. 修复前重读 STATE.md Open Issues + scan.md Issue List
2. **fix-guard.md 强制**：修复前读目标行 ±5 行（不凭记忆/报告修）、grep 确认现状、一次只修一条、修后 grep 验证 + 写 fix-status
3. 修复顺序：P0 → P1 → P2；P0 未清不得标 CONVERGED
4. 修后验证：`cargo test -p <crate>` 通过 + `cargo check` 无新错误 + 重跑受影响 Gate
5. 同步更新 STATE.md（fixed → Closed Issues，带 scan/date）
