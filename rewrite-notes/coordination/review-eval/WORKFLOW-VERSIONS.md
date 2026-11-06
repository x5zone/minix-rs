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
| C-08 | 材料违规分级：现有实现或 Ch3/Ch4 正文 = 作废；清单外 C 真值 = 降级并标注；清单内材料 = 正常 | D01 P-DESIGN D1 读了清单外 C 真值，协议只有“作废”一档，主 agent 先标了规则外的 `PARTIAL-VIOLATION` | `prompt/review-rules/review-panel.md` §6 | V2 候选，待 D02 闭环复核 |
| C-09 | 修改公共入口必须枚举调用方，并跑对应形态门（至少编译检查，能跑则 QEMU） | D01 DESIGN-10 在 `arch_boot_finish` 加 `validate()`，三个 QEMU 测试内核会 panic；meta-review N2 | `prompt/skill/review-loop.md` L3/L4、`review-panel.md` P-FACT | V2 候选，待 D02 闭环复核 |
| C-10 | 测试计数用 crate 全量 `src/*.rs` 或 `cargo test -- --list`，不以文档点名的文件表为清单 | D01 FACT-09 按 15 个文件复算 97/144，实际 125/175；meta-review N1 判 B-08 复发 | `prompt/skill/review-loop.md` L4、`review-evals.md` | V2 候选，待 D02 闭环复核 |

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
- `eval-trace-check.py`：新增 `--panel-dir`；未给时先在 run 目录找 `panel/`，再向上找 `tmp/review-eval/<stage>/<doc>/panel`。这样 `review-loop.md` 的默认命令可以直接跑，不要求把原始面板输出复制进入库目录。

### V1 首跑结果（D01）

- 发现 85 条：① 37 修、② 48 转出、③ ④ ⑤ 各 0；`re-litigation` 0、`adjacent 阻塞` 0。
- meta-review：总体 `IMPROVED`（V1 基线已建立），运行质量 `NOT PASS`（抽样 24/85 = 28.2%，处置一致性 87.5%）。
- 三条否决已补修：测试计数改为 crate 全量 125/175；`arch_boot_finish` 改用 `validate_before_tables` 并编译检查 7 个 QEMU 测试内核；`kernel_info.rs` 的 `reserved_regions` 注释同步。
- 新增盲区 B-14（测试计数口径）、B-15（公共入口调用方枚举）、B-16（材料违规分级）；对应候选 C-08/C-09/C-10。
- V1 保持生效；V2 晋升暂缓到 D02 闭环复核之后，避免一次首跑就改三条规则。

## 五、回退记录

| 版本 | 回退的规则 | 原因 | 证据 | 回退到 |
|---|---|---|---|---|
| — | — | — | — | — |
