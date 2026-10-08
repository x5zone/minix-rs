# 文档生成与分阶段 review（可复用 prompt）

> 用途：为某个 stage 生成一篇正式技术文档，并按「正确性 review → 卓越性 review → 测试 → 提交」的顺序做完整保障。
> 用法：填好下面的参数，整段发给 agent（Claude Code / Codex / OpenCode / Qoder 等）。
> 边界依据：`prompt/review-rules/review-cmds.md` §一b 决策树、§二 full-review（不做边界 line 88）、§四 code-excellence（不做边界 line 137）、§八 序列（line 243）。

## 参数

```
stage = 01-stage-kernel      # rewrite-notes 下的 stage 目录名（去 stage 化时替换此值）
num   = 01                   # 本次生成的正式文档序号（去文档号化时用 {DOC_NN}）
tool  = <当前 agent 工具名>  # claude / codex / trae / qoder，决定 .review 状态目录
tree  = rewrite-notes
```

**目标**：为 stage 生成第 `{num}` 篇正式技术文档，再按「正确性 review → 卓越性 review → 测试」的顺序做完整保障。真值优先序恒为：`Minix3 C 源码行为 > 设计契约 > Rust 实现 > 文档 > AI 分析`。

> **为什么正确性和卓越性必须分两轮**（规则硬边界，不许合并）：
> - `full-review` 只管正确性（bug、覆盖度、与 C 源行为一致），**明确不做卓越性重构**，发现卓越性问题只记进 todo；
> - `code-excellence` / `style-fix` 只管卓越性，**明确不修正确性 bug**（卓越建立在正确之上）。
>
> 于是两轮必须**先跑正确性、把 P0 清零，再跑卓越性**——否则卓越性阶段会踩在未修的正确性缺陷上，且违反各自"不做边界"。

---

## 第 0 步 · 强制预加载（不许凭记忆动手，技能必须真调用）

按序完整读取规范源：`prompt/review-rules/` 下 `review.md` → `review-process.md` → `fix-guard.md` → `review-cmds.md` §一b 决策树 → `review-patterns.md` → `review-doc-checklist.md` §4 → `review-core-semantics.md` §1.6。

所有技能必须通过 **Skill 工具真正调用**并记入 Skill Invocation Log，"读过/我记得"不算调用。

## 第 1 步 · 选题与衔接（检查点 A：先汇报再动手）

- 文档序真源：`{tree}/{stage}/doc_rerank_*.md`（各 AI 的重建蓝图）+ `checklist.md`。**注：`01-stage-kernel` 无 `plan.md`**（plan.md 只在 02–18 stage 存在），有则参考，无则以上两者为准。
- `{tree}/{stage}/draft/` 是旧文档，**仅作思路参考、不作事实来源**；正式文档禁止引用 `draft/`、`.design/`、`tmp_design_and_todo/`（引用即 P0-process-violation）。
- 结合 `minix3/` C 码与 `os/` Rust 实现，定 `{num}` 的主题边界，说明与前后文档的知识衔接（哪些概念首次在此出现、哪些留后续）。
- 汇报「选题范围 + 覆盖的 C 文件/函数 + 与相邻文档衔接」，用户确认后进第 2 步。

## 第 2 步 · 生成正式文档 `{num}-{topic}.md`（`style-bible` 全程强制）

- 技术博客文风：讲本质机制（WHY→WHAT→HOW），深入浅出；Ch1 主语 = CPU/OS/机制/矛盾，不是函数名。
- 禁黑话/缩写/文言文/压缩简写；禁开发文档味（用"若 X 会有 Y 问题，所以 Z"的假设性推理）。
- **读者体验优先于审查便利**：纯坐标（如行尾 `/* 33 */`、工具行号提示）移出正文，落 `.review/{tool}/{stage}/{doc-stem}.anchors.md`（用 `tools/anchor-resolve.sh --extract`）；符号锚点与块级区间题注保留。
- 每条事实断言带符号/来源锚点；行号只允许工具派生；给不出标 `[待验证]`，禁静默写成事实。过程痕迹（review 编号/日期/修复史）禁入正文。
- KERNEL 约束：SMP+BKL，共享数据 `Arc`+`Mutex`/`Atomic`，禁 `Rc`/`RefCell` 跨 CPU；抽象语义而非暴露 CR3/PTE。三档术语 Rewrite/Refactor/[ARCH] 一致标注。

---

## 第 3 步 · R1 正确性 REVIEW（`full-review`，scope=doc）——先把 P0 清零

只判对错与覆盖度，**不改设计、不改文风**。

- Step 0 预检：`tools/design-coverage-check.sh {stage}`、`ls {tree}/{stage}/.design/{num}-*.v*.md`；`tools/doc-code-map.sh {num}-{topic}.md` 得关联 Rust 代码清单。
- Blocker Gates（0/A/B/C/D/D-6/E/G/H）逐门附**命令+输出证据**（L1）；无证据的"✅ 通过"作废，L3 推断视为 FAIL（除非写 `MANUAL_FALLBACK`）。
- Doc Code Sync 七项（`review-doc-checklist.md` §4）必跑；覆盖率穷举产 `SYMBOLS.md`；对照 C 源查遗漏。
- **发现的设计/文风/卓越性问题 → 只登记进 `{num}-todo.md`，本轮不动**（交 R2）。
- `VERIFY-CHECK.md`（独立验证，标"同 agent + 已重放 grep"）。
- **检查点 B1**：P0（六类：fact/code-bug/design-deviation/design-missing/design-wrong/test-missing）全部修完才可继续；可延后的移 `rewrite-notes/coordination/`。

## 第 4 步 · R2 卓越性 REVIEW（消费 R1 todo，建立在正确代码之上）

分文档侧与代码侧，各自通过 Skill 工具调用：

- **文档卓越 → `style-fix`**（前置 `style-bible`）：章节组织、逐节开发文档味、细节深度归位；裸概念复述自检（把 Ch1 函数名换 `[XXX]` 是否仍可读）。
- **代码卓越 → `code-excellence`**：分层审视（整体→模块→trait→函数）+ 再三比对 Redox/Linux/OS 理论/Rust 社区；**§16.7 跨架构判定栏强制**；死代码消除子轮（每项写"为何死+消除影响"，拿不准标 OQ 上交，不擅自删）；非法态封堵清单（§16.6）；发现正确性 bug 退回 `full-review`，不在本轮修。
- 产物：设计对比表（旧 vs 新 vs Redox 参照）+ 死代码清单 + 文档同步记录。
- **检查点 B2**：与用户确认哪些卓越性改动当场落地、哪些进 `{num}-todo.md` / `coordination/`。

## 第 5 步 · 测试回归（有代码改动必做）

- 加载 `test-audit`：5 维度——完备性、测试自身正确性（防"测试代码就是错的"）、冗余、无效、虚构（文档测试描述与代码双向对账）。
- 单元：`cargo test -p {crate}`（工作区根 `os/`）；lint `cargo clippy`。
- 真机门：按 `os/qemu-tests/README.md` 跑 `run_qemu.sh` 三架构回归；权威编译门配方见 `AGENTS.md`（docker `minix-ci:1.94` + `-m 2g -j 1`）。
- 思考并补上后续保障测试，缺口记进 `{num}-todo.md`。

## 第 6 步 · todo 归档与执行轨迹

- 未当场修复项写 `{num}-todo.md`（清晰详细，标准是"换个 AI 也能看明白"，禁速记黑话）。
- （可选，建议开）记 `REVIEW-TRACE` 存档（命令/输出/技能调用/各 Gate 判定），供优化流程。

## 第 7 步 · 提交

- 全部 Gate 通过、P0 清零后，将本次所有改动（新文档 + 代码 + `draft/` 移动 + `.review` 外的产物）提交；正文降噪与锚点底账**同一笔提交**落地。
- 提交信息按项目既有风格；**不跳过任何 hook**；如任一门 FAIL 或未确认，则**先回报、不提交**。

---

## 完成定义（DoD）

`{num}` 文档自包含、无过程痕迹、坐标已外移底账；

- **R1 正确性**：所有 Blocker Gate 有 L1 证据、Doc Code Sync 七项过、P0 全闭环、`SYMBOLS.md` 覆盖对账齐；
- **R2 卓越性**：`style-fix` + `code-excellence` 已跑、设计对比与死代码清单产出；
- **测试**：`test-audit` 五维度过、`cargo test` + qemu 回归绿；
- todo 已归档；（若执行第 7 步）改动已提交。
