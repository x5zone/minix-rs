# Minix-RS — 项目指令（所有 agent 共用的唯一入口）

Minix3 kernel modules rewritten in Rust (x86-64 / ARM64 / RISC-V, no_std). Not a translation — a semantic rewrite preserving external behavior while using Rust's type system internally.

> **本文件是唯一的项目指令入口。** 各运行时的加载方式：Claude Code 在没有 `CLAUDE.md` 的项目里按
> `claude-md-or-agents-md` 模式加载本文件与 `.claude/rules/`；Codex CLI 读本文件与 `.codex/`；
> Trae 用 `.trae/skills/`。**规则的权威内容在 `prompt/`，派生端不得另立一套。**
>
> `CLAUDE.md` 已于 2026-10-07 退役：它的 323 行里唯一未见于规范源的是「Doc Code Sync 七项」，
> 已并入 `prompt/review-rules/review-doc-checklist.md` §4；`.claude/rules/fix-guard.md` 的内容
> 一直只在运行时端，已回填为规范源 `prompt/review-rules/fix-guard.md`。
> 过程与取证见 `prompt/WORKFLOW-OPEN-ITEMS.md`（裁决存档；逐条取证日志已归档于 git 历史）。

## 开工前必读（路由表）

任何 review / 修复工作开始前，按顺序读规范源。**不要凭记忆动手**，也不要引用本文件未覆盖的旧文档：

| 顺序 | 文件 | 它管什么 |
|---|---|---|
| 1 | `prompt/review-rules/review.md` | 核心不变量：执行模型、真值优先序、P0 六分类、三档术语 |
| 2 | `prompt/review-rules/review-process.md` | Step 0–7 全流程、Blocker Gates、Gate 证据规则、设计快照语义 |
| 3 | `prompt/review-rules/fix-guard.md` | 修复守则（动手前 5 件事、一次一条、修后留痕） |
| 4 | `prompt/review-rules/review-cmds.md` | 6 个任务命令入口与不可裁剪的通用强制门 |
| 5 | `prompt/review-rules/review-patterns.md` | 模式库（含 P0 必检清单与验证命令） |
| 6 | `prompt/README.md` | 三端同步机制：改了源必须同步哪些文件、跑哪些校验 |

运行时副本 `.claude/rules/review-core.md`、`.claude/rules/review-process.md`、`.claude/rules/fix-guard.md`
由上面三个源手工同步，内容同源。

## Build & Test

- Build：`cargo build`（workspace 根在 `os/`）
- Test：`cargo test`；Lint：`cargo clippy`
- 权威编译门配方：docker `minix-ci:1.94` + `-m 2g -j 1`；三架构真机门见 `os/qemu-tests/README.md`

## Directory Layout

```
minix3/              — 原始 Minix3 C 源码（ground truth，禁止修改）
os/                  — Rust 实现：内核、用户态服务器、驱动、库
rewrite-notes/       — 活跃重写文档：rewrite-notes/{stage}/{doc}.md
redesign-notes/      — 方向探索文档（重写稳定后启用；现存内容是未收敛的存量思考）
study-notes/         — 早期 Minix3 学习笔记（AI 生成，不作事实基线）
book/                — mdBook 成品区（后续从笔记树抽取）
prompt/              — 规则与技能的唯一真相源（含本文件所路由的规范）
.claude/ .codex/ .trae/ .agents/ — 三端运行时适配（派生自 prompt/，不手写新规则）
tools/               — 门禁与构建脚本（路径真源：tools/notes-layout.conf）
tmp/                 — 会话产物与取证，整域不入库（evidence/ log/ bin/ nk4a/ 等）
```

三棵笔记树于 2026-10-07 取代旧的 notes 伞目录，并剥掉 rewrite 分区下那层已废弃的模块目录：规范路径只剩两节（`rewrite-notes/01-stage-kernel/16-smp.md`）。旧→新逐文件对照见 `rewrite-notes/MIGRATION.md`。

**中间产物目录约定**：`.design/`（每篇文档的可复用设计快照）与已废弃的 `tmp_design_and_todo/` 视为中间产物，**正式文档绝不引用**（引用即 P0-process-violation）；正式文档必须自包含，引用指向文档、代码、C 源的绝对路径。会话产物（日志、镜像、评审证据）一律进 `tmp/` 的功能子目录，仓库根不允许散落文件——细则见 `prompt/WORKFLOW-OPEN-ITEMS.md` F-C。

## Key Constraints

- `#![no_std]` everywhere except `#[cfg(test)]`
- 错误类型必须映射到 Minix3 的 errno 值，禁止自造错误码
- 硬件通过 trait 抽象，不把 CR3/PTE 位之类暴露给 OS 语义层
- **执行模型按模块类型区分**：用户态服务器（VM/PM/VFS 等）= 单线程事件循环，`!Send`/`!Sync`/`Rc`/`RefCell` 合理；**Kernel = SMP + BKL（自旋锁）**，共享数据需 `Arc`+`Mutex`/`Atomic`，禁止 `Rc`/`RefCell` 跨 CPU；BKL 临界区内不得 sleep / schedule / IPC
- **⛔ 抽象语义，不描述机制（本项目的第一原则）**：区分「OS 语义对象」与「硬件承载对象」。同一层的代码拥有同一语义，不同层的差异待在该待的地方——优美不是让三个实现长得像，而是让接口根本看不见硬件差异。判据三问、边界条款（arch trait 自身可谈硬件名；启动契约不在射程内；禁止为漂亮提前改契约）见 `prompt/review-rules/review-core-semantics.md` 与模式库的对应总括模式
- **真值优先序**：`Minix3 C 源码行为 > 设计契约 > Rust 实现 > 设计/技术文档 > AI 分析`。拿不准就去 `minix3/` 读原始 C 代码
- **三档术语**：Rewrite（保持外部行为）/ Refactor（不改语义的重构）/ Architectural Evolution（必须三处一致标注 `[ARCH: ...]`：文档 + 设计 + 代码）
- **P0 六类**：P0-fact / P0-code-bug / P0-design-deviation / P0-design-missing / P0-design-wrong / P0-test-missing
- **⛔ 显式调用技能**：必须通过 Skill 工具真正调用技能，scan.md 的 Skill Invocation Log 只记实际调用；"规则已加载"不算调用
- **⛔ Gate 证据强度分级**：L1 工具输出（A/D/E 门必需）、L2 手工 grep（B/C 可接受）、L3 推断（视为 FAIL，除非写明 `MANUAL_FALLBACK` 理由）。**没有附命令与输出的「✅ 通过」一律作废**
- **文档与代码同步**：任何文档 review 必跑 Doc Code Sync 七项（`prompt/review-rules/review-doc-checklist.md` §4）
- **VERIFY-CHECK 的同 agent 局限**：单 session 里验证者与被验证者是同一个模型，必须显式标注"同 agent 验证 + 已重放 grep 命令"；consistency < 90% 就写 NOT PASS，不给模糊结论；有条件时优先跨工具只读复核

## Review Workflow

1. **声明 scope**：目标文件、模式（doc / code / full）、STATE.md 状态。状态目录按 `{stage}` 分组且三工具隔离：`.review/{tool}/{stage}/`（Trae 用 `scans/` 扁平结构，Claude 与 Codex 用 `{doc-stem}/` 子目录）
2. **Step 0 硬阻断预检**（所有模式强制）：4 条 `ls {tree}/{stage}/.design/{NN}-*.v*.md` + `tools/design-coverage-check.sh {stage}`；快照缺失 → 对应 Gate FAIL → Step 0.3 嵌入生成（不中断 review、不标 N/A、不许复用他篇快照）。缺失却标 CONVERGED → P0-process-violation（模式 69）
3. **Blocker Gates**（0 / A / B / C / D / D-6 / E / G / H）：每门必须附实际命令与输出证据；任一门失败 → scan.md 标 DRAFT，STATE.md 不更新
4. **产出**：scan.md（单文件汇总：Skill Invocation Log / Blocker Gates 状态 / Step 0 预检结果 / Issue List / structure 摘要）+ structure.md（文档 review 的 12 节骨架分析）+ SYMBOLS.md（覆盖率穷举）+ VERIFY-CHECK.md（独立验证，consistency ≥ 90% 才可 CONVERGED）
5. **修复原则**：P0 全修完才可 CONVERGED；文档与代码保持同步；严格遵守 `prompt/review-rules/fix-guard.md`
6. **收敛停止**（Step 7.1）：同篇 ≥5 轮强制交付 / 连续两轮新增 P1 ≤ 1 视为收敛 / 本轮成本 > 上一轮 80% 而新发现 < 20% 停止 / 首轮零发现触漏检自检（随机抽 3 项重跑，仍零发现才交付）
7. **每次 review 必答 Step 5.7 Rule Discovery**（是否发现新模式）——规则集靠此自演进

## 任务命令与技能

任务一律先走**决策树**再落命令：`prompt/review-rules/review-cmds.md` §一b 给「任务性质 → 命令 → 技能」的路由树（新会话第一眼该看它，别先看名字最像的技能）。6 个命令入口：`full-review`、`style-fix`、`code-excellence`、`test-audit`、`todo-fix`、`style-bible`。旧 Profile A–P / R / AG 保留为别名，对账表见该文件 §八。**通用强制门任何命令都不可裁剪**：锚点纪律门、测试名对账门、文风门、translate 防线、fix-guard、文档-代码同步。

技能清单（10 个）与调用时机见 `.codex/skills/`、`.claude/skills/`、`.trae/skills/`；源在 `prompt/skill/`。

> **一致性约定**：`.trae/skills/`、`.codex/skills/` 与 `.claude/skills/review-scan/` 全部由 `prompt/skill/` 经 `tools/generate-derived-skills.sh` 生成（编排器源在 `prompt/skill/review-scan/`，运行时差异用 `<!-- @if:claude -->` / `<!-- @if:codex -->` 条件块表达，Claude 专有的 `allowed-tools` 字段只进 Claude 份）；只有 `.claude/rules/*` 三份运行时副本属手工同步。改完源必须跑：`tools/generate-derived-skills.sh` → `tools/check-review-rules.sh` → `tools/lint-review-rules.sh` → `tools/diff-trae-skills.sh --only-diff`，四条全绿才算改完。这四道门已由 `.github/workflows/rules-consistency.yml` 在 CI 常驻执行（连同门自身的 `--self-test` 与六个工具自测），不依赖任何人记得跑。
