---
name: review-loop
description: 文档+关联代码的固定 review 循环与工作流评测：L0 基线、L1 独立面板、L2 归因、L3 修复、L4 复验、L5 轨迹、L6 meta-review、L7 升版。用于逐篇试点、判断 review 流程是否进步与收敛。
---

# Review Loop —— 文档 + 代码的统一 review 循环

## 何时使用

- 用户要求“按固定循环 review 一篇文档和它相关的代码，并记录轨迹”；
- 用户要求“判断当前 review 工作流是否进步 / 是否收敛”；
- 用户要求逐篇处理 `rewrite-notes/01-stage-kernel/` 的 01–10 篇，或后续 `02-stage-vm/`、`03-stage-rs/` 的 01–10 篇。

不用于：

- 普通的一次性 review（走 `full-review` 等六个命令）；
- 只改文风（走 `style-fix`）；
- 只做代码卓越性（走 `code-excellence`）；
- 只修一个 TODO（走 `todo-fix`）。

本技能是外层编排，不新增第七个任务命令。它调用现有命令与检查清单，只新增“独立面板、归因、评测、升版”四层。

## 开工前必读

1. `prompt/review-rules/review-panel.md`：角色、材料隔离、7 字段、只读纪律。
2. `prompt/review-rules/review-evals.md`：语料、轨迹格式、指标、meta 问卷、收敛标准。
3. `rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`：当前版本。
4. `rewrite-notes/coordination/review-eval/CORPUS.md`：本篇条目与期望。
5. `rewrite-notes/coordination/review-eval/DECISIONS.md`：与本篇相关的冻结决策。
6. `prompt/review-rules/review-process.md`、`review-doc-checklist.md` 或 `review-code-checklist.md`：按本轮范围加载。

主 agent 必须通过 Skill 工具显式调用 `review-process-skill`，并按范围调用 `review-doc-skill` 或 `review-code-skill`；面板子 agent 使用 `review-panel.md` 第四节的 brief，不加载完整技能，以保持材料隔离。这个例外必须写进 `loop.md §1`。

## L0 基线、范围、语料装载

1. 确认工作树状态；记录 `git rev-parse HEAD`、`git status --porcelain`。
2. 记录文档 sha256 与行数：`sha256sum <doc>`、`wc -l <doc>`。
3. 跑 Step 0 硬预检：
   ```bash
   ls rewrite-notes/<stage>/.design/<NN>-outline.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-outline-review.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-design.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-design-final.v*.md
   tools/design-coverage-check.sh <stage>
   ```
   缺失时按 `review-process.md` Step 0.3 嵌入生成，不中断循环。
4. 关联代码清单：`tools/doc-code-map.sh <doc> --check`；有缺失先登记，不进入 L1。
5. 初始化 review 状态：`tools/review-init.sh zcode <doc>`。
6. 写 `runs/<stage>/<doc-stem>/loop.md §0`：commit、sha256、行数、范围、关联代码、语料条目 ID。
7. 02/03 阶段先读 `review-eval/doc-set/` 的对账简报；没有简报就先做对账，不进入 L1。
8. **读本篇前几轮 follow-ups**：如果本篇已跑过面板轮，读 `runs/<stage>/<doc-stem>/follow-ups.md` 中上一轮的未修 ② 项；按文件交集分成两份——"顺手修"（与本篇关联代码重叠的项，本轮 ① 同批处理）与"核销清单"（需要确认是否已被其他轮修复的项，交 P-FACT 在面板中确认状态）。首轮没有前轮 follow-ups 时跳过此步。

范围声明模板：

```text
本篇范围 = <文档主题> + 以下关联代码：
<doc-code-map 清单>
不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。
```

## L1 独立审查面板

按 `review-panel.md` 第三节选择角色，按第七节决定是否缩编。默认配置：

- 文档少于 500 行：P-FACT、P-DESIGN D2、P-READER 三合一（A+B+C 一个 agent）；
- 文档 500–1500 行：P-FACT、P-DESIGN D2、P-READER 三合一；高风险加 P-REFUTE；
- 文档 1500 行及以上：P-FACT、P-DESIGN D1+D2、P-READER A+B+C 分角色；高风险加 P-REFUTE；
- 关联代码 < 10k 行时跳过 P-DESIGN D1（独立设计成本高于收益，D2 直接对照文档与源码）；
- P-REFUTE 触发条件：P-FACT 报出 ≥1 条 P0，或本篇属于高风险类（启动链/IPC/内存管理/时钟中断）。

派发方式：

1. 用 Agent 工具启动子 agent，类型 `general-purpose`；每个角色一个独立子 agent。**默认全量并发**；仅在收到派发失败时降档（6 → 3 → 1），成功后探测回升（连续 3 个成功升并发 2 探测，逐级回升，失败回档位 1 并重新计数）。每次降档/升档事件记入 `loop.md §1`。
2. 每个角色先写报告骨架，每完成 2–3 条发现就追加写盘，不要等全部查完才写。
3. 角色在 10 分钟无输出时重试 1 次；仍失败标 `MISSING`，在 `loop.md §1` 写明缺哪个角色与可能漏什么。失败的 agent 先检查产出文件完整性（有 `## FINDING` 或显式"无发现"声明），不完整即重派一次。
4. 合并角色（如读者 A+B）必须在 `loop.md §1` 标 `VARIANCE`，说明合并理由与独立性损失。
5. 禁止用 `grep '^#'` 读章节标题来定位；需要标题时用 `rg '^## '` 只取二级标题，且不得把设计章标题交给需要盲读的角色。
6. 把 `review-panel.md` 第四节对应 brief 逐字作为任务，替换尖括号占位符；不要补充作者的解释、结论、历史评审。
7. 子 agent 只写 `tmp/review-eval/<stage>/<doc-stem>/panel/<role>.md`；禁止修改仓库文件。
8. 面板运行前后各跑一次 `git status --porcelain`；出现新改动即判违规，丢弃该角色输出并重跑。
9. 面板原始输出逐字保留，不合并、不润色。

`loop.md §1` 记录：角色、拿到什么、没拿到什么、模型、起止时间、原始输出路径、缩编理由与风险、派发降档/升档事件。

## L2 归因与决策

1. 收齐面板输出，按 `review-panel.md` 第五节的编号规则统一编号。
2. 每条发现按 `review-evals.md` §四 新归因表处置：① 修（P0/P1/P2 一律）、② 相邻范围（带日期）、③ 误报、④ 读者体验（本轮修）、⑤ 证据不足。**范围内确认为真缺陷的一律本轮修，不允许无档期转出。**
3. 查 `DECISIONS.md`；命中且无新证据的，记 `re-litigation`，不计文档缺陷。
4. 标范围：`in-scope` 或 `adjacent`；`adjacent` 标 ② 带日期（哪一轮修）。
5. 写 `defect-ledger.md`，表头必须含 `发现 ID`、`来源角色`、`严重度`、`类别`、`处置类`、`处置`、`证据`、`复验`。
6. 写 `loop.md §2` 与 `§3`。
7. 跑轨迹检查：
   ```bash
   python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/<stage>/<doc-stem>
   ```
   退出码非 0 时，回到第 1 步补处置；不允许进入 L3。

## L3 修复

1. 按 `fix-guard`：读目标 ±5 行、grep 确认现状、一次只修一条、修后 grep 验证、写 fix-status。
2. **每条代码修复必须**：带回归测试（覆盖修复前的失败场景）→ 反向验证（还原→红，恢复→绿）→ fix-scope（发现点名的位置逐处改到，缺一处降 ② 带日期）。
3. 文档修复与代码修复同批；代码改动必须同步 Ch3、Ch4、§5 与测试。
4. **”顺手修”清单**与 ① 同批处理：L0 步骤 8 产出的”顺手修”项（前轮 follow-ups 中与本篇关联代码重叠的项），在本轮修复中一并处理。
5. 代码门：`cargo fmt --check`、`cargo clippy`、`cargo test`；触碰 arch 或 boot 时按 `AGENTS.md` 跑 QEMU。
6. **受影响 crate 全量绿**：提交前 `cargo test -p <crate>` 必须通过；含 minix-arch。

## L4 复验与回归

1. 修复段回读：由发现该问题的角色或同角色冷启动实例，只读修改段及邻句；标签写”修复后回读”。
2. 台账重放：每条”已修”行给可重放 `grep` 命令与命中输出。
3. 文档五门：
   ```bash
   tools/anchor-resolve.sh --check <doc>
   tools/doc-code-map.sh <doc> --check
   git add <doc>
   tools/doc-style-lint.sh --diff
   tools/doc-ref-check.sh --check <doc>
   tools/doc-code-density-check.sh --check <doc>
   ```
4. 代码块分类：`tools/doc-snippet-extract.sh <doc>`；引文块逐块对账。
5. 回归扫描：邻句、自引入过程词、端口义、引号、模糊尾巴、跨节复述。
6. **commit 级回归 review**：每个逻辑 commit 提交后，派一个小型 review（只看该 commit 的改动范围，不是全量面板），按 `review-evals.md` §4e 的定义：
   - 文档 commit → 锚点 + doc-code-map + 改动段 fact 抽查 + 改动节 reader 抽查；
   - 代码 commit → 双轴（规范轴 + 设计意图轴）+ 契约/并发/失败路径；
   - 混合 commit → 两个子集都跑。
   发现的问题回 L2 归因。
7. 写 `loop.md §4` 与 `§5`。

## L5 轨迹与指标

1. `loop.md` 补齐 §0–§8 九节。
2. 写 `metrics.md`，固定键：
   ```markdown
   - 发现总数: <整数>
   - 处置: ① <整数> / ② <整数> / ③ <整数> / ④ <整数> / ⑤ <整数>
   - re-litigation: <整数>
   - adjacent 阻塞: <整数>
   - 重构: 提出 <整数> / 接受 <整数> / 实施 <整数> / 登记 <整数>
   - 成本: agent <整数> / token <整数> / 耗时 <文本>
   ```
3. 再跑一次 `eval-trace-check.py`，确认退出码 0。
4. 把本轮新增的 follow-up 写进 `follow-ups.md`，每条给目的地。

## L6 独立 meta-review

1. 按 `review-evals.md` §5.0 的分级决定深度：轮次 1–2 全量九问；轮次 3+ 轻量 3 问；文档收敛时全量收口。
2. 按 `review-panel.md` 4.6 的 brief 启动冷启动子 agent；输入按 `review-evals.md` 第五节。
3. 要求它抽样独立复判至少 20% 的发现，并给抽样一致性。
4. 输出到 `meta/<doc-stem>-review.md`。
5. 结论为 `REGRESSED` 时，不进入下一篇，先回到 L2 检查归因。
6. 新增盲区写入 `BLIND-SPOTS.md`，下一轮回归必须复现。

## L6.5 Resource re-consultation

1. 根据本轮 `BLIND-SPOT-DELTA` 的主导类别，从 `prompt/resources/` 选读一份最相关的文件：
   - 代码设计类盲区 → `adversarial-rust` 的对应规则文件
   - 并发/资源类盲区 → `graham-code-review` 或 `asterinas`
   - 教学类盲区 → `explain-in-simple-language` 或 `systems-programming-course-builder`
   - 流程类盲区 → `eai-agent-toolkit` 或 `fresh-eyes-review`
2. 读完后回答一个问题：本轮的盲区，这份资源有没有我们没用到的检测模式？
3. 有 → 记入 `BLIND-SPOTS.md` 的期望探测器列，下一轮实现。
4. 没有 → 记录"已查，无新模式"，防止同一资源被反复空读。

## L7 收敛检查与工作流升版

### 7a. 文档收敛检查

按 `review-evals.md` §6A 的 G1–G8 判据逐条检查。全满足 → 该篇 `CONVERGED`，进下一篇；不满足 → 继续修。

**第 5 轮结束时 G1–G4 仍有未清项** → 生成 `STUCK-REPORT.md` → 标记该篇为 `STUCK` → 终止该篇循环 → 进下一篇。

### 7b. 陷阱套件

每 2 轮复跑一次 `traps/T01–T08`。有陷阱翻转 → 过拟合警报（`review-evals.md` §十）。

### 7c. 边界批次

本篇收敛后、下一篇 L0 前，检查上篇与下篇的 follow-ups 中是否有 P0/P1 设计债，且其宿主文件在下篇的关联代码清单里。有 → 在两篇之间插一个"边界批次"（只修设计债，不开面板），修完才进下篇 L0。

### 7d. 工作流升版

只在版本边界执行：

- 01 篇之后；
- 之后每 2 篇（03、05、07、09 之后）；
- 10 篇之后；
- 或出现被证实的 Critical/High 盲区。

步骤：

1. 跑版本边界回归：`CORPUS.md` 的 R01–R12 与 T01–T08。
2. 汇总本轮与前几轮的指标，写 `meta/boundary-V<n>.md`。
3. 晋升规则：每次最多 2 条；每条带案例 ID、目标文件、期望效果、回归风险、替代物。
4. 跑同步门：
   ```bash
   tools/generate-derived-skills.sh
   tools/check-review-rules.sh
   tools/lint-review-rules.sh
   tools/diff-trae-skills.sh --only-diff
   python3 tools/eval-trace-check.py --self-test
   python3 tools/trait-audit.py --self-test
   ```
5. 更新 `WORKFLOW-VERSIONS.md` 的版本表、减法账本、同步门读数。
6. 全绿后提交；未全绿不升版。
7. 连续两篇证明某条新规无效果或制造误报，回退并记录。

## 每篇的提交

每篇两个逻辑单元：

1. 修复：文档、代码、测试、配置；
2. 轨迹：`runs/` 下的 `loop.md`、`metrics.md`、`defect-ledger.md`、`follow-ups.md`，以及 `meta/` 报告。

只用具名路径 `git add`；禁止 `git add -A`、`git commit -a`。

## 禁止事项

- 禁止把面板当投票；冲突按证据强度裁决。
- 禁止让读者角色读源码；禁止让设计角色先看现有实现再自称独立设计。
- 禁止没有处置行的发现进入修复。
- 禁止把相邻问题升级为当前阻塞。
- 禁止在同一轮同时改工作流和修文档；只有循环本身跑不通的阻塞修复例外。
- 禁止用“没有新问题”当收敛；收敛判据见 `review-evals.md` 第六节。
- 禁止把 `prompt/resources/` 的外部文本直接抄进规则源。
