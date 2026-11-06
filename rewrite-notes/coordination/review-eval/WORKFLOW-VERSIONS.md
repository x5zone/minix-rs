# WORKFLOW-VERSIONS —— 工作流版本链与减法账本

> 规则：每次升版必须带真实案例、目标文件、期望效果、回归风险，以及“替代或合并了哪个旧装置”。
> 每次最多晋升 2 条；没有替代对象的纯新增，必须写明“为什么旧装置接不住”。
> 连续两篇证明某条新规无效果或制造误报，回退；版本记录保留。

## 一、版本表

| 版本 | 日期 | 变更 | 证据 | 替代物 | 同步门读数 | 状态 |
|---|---|---|---|---|---|---|
| V1 | 2026-11-07 | 建立外层循环脚手架：`review-panel.md`、`review-evals.md`、`review-loop` 技能、`eval-trace-check.py`、`trait-audit.py`、固定语料、盲区账本、冻结决策账本、八个对抗样本；修正 `check-review-rules.sh` 的 `grep -c` 零命中中断与 `diff-trae-skills.sh` 漏认 `review-loop` 两处工具缺口 | W-6、W-16、`prompt/code-review-subagent.md:4`（设计死代码）、`69-75`（冷读）、`484-498`（范围纪律）；DOC-ITER 台账的 14 篇引文事故 | 无（V1 只搭脚手架，不删除旧装置） | 四门全绿（见第四节） | 生效 |

## 二、候选规则登记

候选只登记、不生效；晋升需要版本边界与用户裁决。候选编号与版本表分开，避免把“想法”误读成“规则”。

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-01 | 面板四角色（事实 / 设计 / 读者 / 反证）与材料隔离矩阵 | W-6、W-16、`code-review-subagent.md:69-75`、`548-572` | `prompt/review-rules/review-panel.md` | V1 已落地，待首跑检验 |
| C-02 | 轨迹完整性机械检查：每条面板发现必须有处置行 | DOC-ITER 02 篇 R02 发现约一半冷读发现没有处置行 | `tools/eval-trace-check.py` | V1 已落地，待首跑检验 |
| C-03 | trait 事实供给：单实现、零实现、零调用候选 | `code-review-subagent.md:4`（设计死代码） | `tools/trait-audit.py` | V1 已落地，待首跑检验 |
| C-04 | 合并三套读者装置：doc-gen 学生走查、DOC-ITER V2 盲测学生、V5-A 冷读 | 三套装置解决同一失败类，重复执行浪费成本 | `AI-chats/doc-gen-prompt.md:186-199`、`prompt/DOC-ITER-WORKFLOW.md:272-300` | 候选，待 V2 边界 |
| C-05 | 关键 P0 的复验由面板冷启动实例承担，替代同 agent VERIFY-CROSS | `prompt/review-rules/review-process.md:1916-1926` 承认同 agent 局限 | `prompt/review-rules/review-process.md` Step 5.6 | 候选，待 V2 边界 |
| C-06 | 合并 `structure.md` §11 复述与 V5-B 结构编辑 | 两者都在问“读者拿到什么结构” | `prompt/review-rules/review-process.md:755-760`、`prompt/DOC-ITER-WORKFLOW.md:290` | 候选，待 V2 边界 |
| C-07 | 模式库单一源：`review-scan/checks/patterns.md` 由权威源生成，不再手抄子集 | F-B 三副本漂移，`prompt/WORKFLOW-OPEN-ITEMS.md:317-325` | `tools/generate-derived-skills.sh`、`prompt/skill/review-scan/checks/patterns.md` | 候选，待工具排期 |

## 三、减法账本

每加一个装置，必须在这里说明它替代或合并了什么；没有替代对象的，写“净增”与理由。

| 版本 | 新增 | 替代 / 合并 | 净增理由 |
|---|---|---|---|
| V1 | 面板四角色 | 无（复用 DOC-ITER V5 与 `full-review` 的只读形态） | 现有流程没有独立设计角色与独立读者；这是净增，但复用已有 checklist，不新写一套规则 |
| V1 | `eval-trace-check.py` | 无（DOC-ITER 的 `doc-iter-package-check.py` 只查包内数值，不查发现与处置的覆盖） | 02 篇 R02 的漏处置是真实事故，必须有机械闸门 |
| V1 | `trait-audit.py` | 无（`check-rs-unwired.sh` 查的是占位符接线，不查 trait 实现面） | 设计死代码需要事实供给，不能只靠自然语言规则 |

## 四、同步门读数

每次升版后必须贴实际命令与输出，不允许只写“已通过”。

| 版本 | `generate-derived-skills.sh` | `check-review-rules.sh` | `lint-review-rules.sh` | `diff-trae-skills.sh --only-diff` | 新工具自测 |
|---|---|---|---|---|---|
| V1 | exit 0；派生 10 个技能（含 `review-loop`） | exit 0；输出 “Review source, Trae adapters, Claude runtime references, and Codex adapters are consistent.” | exit 0；0 个失败 | exit 0；源 10 个、派生 10 个，完全同步 | `eval-trace-check.py --self-test` 与 `trait-audit.py --self-test` 均 PASS |

### V1 基线读数

- 内核宿主测试：`cd os && cargo test -p minix-kernel --features mock`，库测试 871 个，863 通过、0 失败、8 忽略；其余测试二进制 3 通过 / 3 忽略、1 通过、2 通过、1 通过、0 通过 / 1 忽略。退出码 0。
- `cargo clippy -p minix-kernel --features mock`：退出码 0，80 条既有警告；没有把警告清零作为本轮目标。
- `trait-audit.py --root os`：NO-IMPL 13 / SINGLE-IMPL-LOW-USE 10 / SINGLE-IMPL 9 / MULTI-IMPL 247。这是事实分档，不是删除建议；首轮设计角色需要逐条核实这 32 个候选。
- 注意：`AGENTS.md` 记录的 831 通过是 2026-10-12 的读数；基线提交 `e364d5f92` 的内核改动使库测试增长到 871 个。后续循环以本节的实测读数为基线，不沿用旧数。

### V1 工具修正

- `check-review-rules.sh`：Codex 正文漂移检查用 `grep -c '^### '`，零命中时 `grep` 退出 1，被 `set -e` 打断，导致新增无三级标题的技能时整门静默失败。改为 `grep -c ... || true`。
- `diff-trae-skills.sh`：只枚举 `review-*-skill.md`，漏掉 `review-loop.md`。枚举条件补上 `review-loop`，源与派生计数从 9 变 10。

## 五、回退记录

| 版本 | 回退的规则 | 原因 | 证据 | 回退到 |
|---|---|---|---|---|
| — | — | — | — | — |
