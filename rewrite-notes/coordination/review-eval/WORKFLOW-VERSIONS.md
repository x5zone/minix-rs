# WORKFLOW-VERSIONS —— 工作流版本链与减法账本

> 规则：每次升版必须带真实案例、目标文件、期望效果、回归风险，以及“替代或合并了哪个旧装置”。
> 每次最多晋升 2 条；没有替代对象的纯新增，必须写明“为什么旧装置接不住”。
> 连续两篇证明某条新规无效果或制造误报，回退；版本记录保留。

## 一、版本表

| 版本 | 日期 | 变更 | 证据 | 替代物 | 同步门读数 | 状态 |
|---|---|---|---|---|---|---|
| V1 | 2026-11-07 | 建立外层循环脚手架：`review-panel.md`、`review-evals.md`、`review-loop` 技能、`eval-trace-check.py`、`trait-audit.py`、固定语料、盲区账本、冻结决策账本、八个对抗样本；修正 `check-review-rules.sh` 的 `grep -c` 零命中中断与 `diff-trae-skills.sh` 漏认 `review-loop` 两处工具缺口 | W-6、W-16、`prompt/code-review-subagent.md:4`（设计死代码）、`69-75`（冷读）、`484-498`（范围纪律）；DOC-ITER 台账的 14 篇引文事故 | 无（V1 只搭脚手架，不删除旧装置） | 四门全绿（见第四节） | 生效 |
| V2 | 2026-11-07 | 晋升 C-20 轨迹门 v2 与 C-23 禁止 follow-up 大桶：无发现面板必须显式豁免、P-REFUTE 主张入附加表、`dup-of` 合并；`eval-trace-check.py` 增加 `panel-exemption` 检查 | D02/D03 meta-review：B-17 轨迹门 WARN 仍 PASS、B-18 跨角色重复无归并、D03-F10 把 36 条读者项压成一行 | C-20 替代 V1 的“WARN 可接受”；C-23 替代“follow-up 大桶”实践 | 待跑 | 生效 |
| V2.1 | 2026-11-07 | 全修策略（P0/P1/P2/④ 一律本轮修，不允许无档期转出）+ ② 收窄为两种带日期形态 + 测试充分性门（回归测试 + 反向验证 + 含 minix-arch 全量绿 + QEMU）+ commit 级回归 review（文档轴/代码双轴/混合）+ meta 分级（1–2 全量 / 3+ 轻量 / 收敛时收口）+ 收敛判据双层（文档 G1–G8 / 工作流原八条）+ 过拟合护栏（§十）+ resource re-consultation（L6.5）+ reader ④ 必修 + Top-20 输出上限 + fact 已知清单 + 派发降档协议 | D01–D04 转出率 56%→83%；B-20/B-23/B-24；D05 面板 5h48m；meta 一致性四轮 <90% | ②大桶转出被排期制替代；全量 review 被 commit 级回归 review 替代；④可选被必修替代；resource 一次性输入被持续引用替代 | 四道门 | 生效 |

### V2 执行状态（D04 复核）

- C-20 的 `panel-exemption` 已落地（D01–D04 轨迹门均通过）；`dup-of` 合并与 P-REFUTE 映射校验尚未工具化（B-23/B-24），V2.1 处理。
- C-23 规则已写，但 D04 的 `D04-F11..F14` 仍是分组大桶；V2 记录不宣称“已完全落地”，V2.1 要拆到逐条。
- 下一轮关联代码基线用 53（D04 修复后 doc-code-map 计数），不再用 51。

### V2.1 执行状态（D05 复核）

- C-25 的规则已写进 `review-evals.md` §8（每条 ① 逐处回读；暂存 diff 的 hunk 必须回指台账行）；工具化的 fix-scope 检查尚未实现（B-28）。
- C-28 的面板约束已写进 `review-loop.md` L1（并发 ≤3、边查边写、重试 1 次后 MISSING、合并角色 VARIANCE、禁止 `grep '^#'`）。
- 四道规则同步门全绿；D01–D05 的轨迹门复跑 PASS。

### D05 首跑结果（05-clock-interrupt-init）

- 发现 83 条：① 11、② 72、③ 0、④ 0、⑤ 0；`re-litigation` 0、`adjacent 阻塞` 0。
- meta-review：总体 `FLAT`，运行质量 `NOT PASS`（抽样 22/83 = 26.5%，严格一致性 72.7%）。
- 新增盲区 B-25（台账 ID 重编号）、B-26（非 FINDING 面板豁免）、B-27（平台回收角色缩编）、B-28（暂存 diff 无反向对账）。
- V2.1 晋升 C-25（fix-scope）与 C-28（面板可执行性）；工具化 fix-scope 待 V2.2。
- 四条 P0 设计项（TSC 校准、riscv `IrqVector`、`irq_manager` 架构泄漏、LAPIC 契约）仍在 follow-up；D05 不因修复率低而关闭。

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
| V2 | exit 0；无漂移 | exit 0 | exit 0；0 个失败 | exit 0；10 个 skill 完全同步 | `eval-trace-check.py --self-test` PASS；D01–D03 轨迹门均 PASS（8 PASS / 2 WARN / 0 FAIL） |
| V2.1 | exit 0；无漂移 | exit 0 | exit 0；0 个失败 | exit 0；10 个 skill 完全同步 | `eval-trace-check.py --self-test` PASS；D01–D05 轨迹门均 PASS |

### V1 基线读数

- 内核宿主测试：`cd os && cargo test -p minix-kernel --features mock`，库测试 871 个，863 通过、0 失败、8 忽略；其余测试二进制 3 通过 / 3 忽略、1 通过、2 通过、1 通过、0 通过 / 1 忽略。退出码 0。
- `cargo clippy -p minix-kernel --features mock`：退出码 0，80 条既有警告；没有把警告清零作为本轮目标。
- `trait-audit.py --root os`：NO-IMPL 13 / SINGLE-IMPL-LOW-USE 10 / SINGLE-IMPL 9 / MULTI-IMPL 247。这是事实分档，不是删除建议；首轮设计角色需要逐条核实这 32 个候选。
- 注意：`AGENTS.md` 记录的 831 通过是 2026-10-12 的读数；基线提交 `e364d5f92` 的内核改动使库测试增长到 871 个。后续循环以本节的实测读数为基线，不沿用旧数。

### V1 工具修正

- `check-review-rules.sh`：Codex 正文漂移检查用 `grep -c '^### '`，零命中时 `grep` 退出 1，被 `set -e` 打断，导致新增无三级标题的技能时整门静默失败。改为 `grep -c ... || true`。
- `diff-trae-skills.sh`：只枚举 `review-*-skill.md`，漏掉 `review-loop.md`。枚举条件补上 `review-loop`，源与派生计数从 9 变 10。
- `eval-trace-check.py`：新增 `--panel-dir`；未给时先在 run 目录找 `panel/`，再向上找 `tmp/review-eval/<stage>/<doc>/panel`。这样 `review-loop.md` 的默认命令可以直接跑，不要求把原始面板输出复制进入库目录。
- `eval-trace-check.py`（D02 补）：`FINDING` 正则接受标题后缀（如 `【低置信 · 高价值】`），避免漏计 reader-c 的 C-14..C-16；D02 因此从 66 条恢复到 69 条。

### V1 首跑结果（D01）

- 发现 85 条：① 37 修、② 48 转出、③ ④ ⑤ 各 0；`re-litigation` 0、`adjacent 阻塞` 0。
- meta-review：总体 `IMPROVED`（V1 基线已建立），运行质量 `NOT PASS`（抽样 24/85 = 28.2%，处置一致性 87.5%）。
- 三条否决已补修：测试计数改为 crate 全量 125/175；`arch_boot_finish` 改用 `validate_before_tables` 并编译检查 7 个 QEMU 测试内核；`kernel_info.rs` 的 `reserved_regions` 注释同步。
- 新增盲区 B-14（测试计数口径）、B-15（公共入口调用方枚举）、B-16（材料违规分级）；对应候选 C-08/C-09/C-10。
- V1 边界对抗样本：T01–T08 全 PASS，阳性召回 6/6、阴性 2/2、误报 0、`adjacent 阻塞` 0；报告见 `meta/boundary-V1.md`。
- V1 保持生效；V2 晋升暂缓到 D02 闭环复核之后，避免一次首跑就改三条规则。

### D02 首跑结果（02-higher-half-kernel）

- 发现 69 条：① 24 修、② 42 转出、③ 1 误报、④ 2 非缺陷；`re-litigation` 0、`adjacent 阻塞` 0。
- meta-review：总体 `FLAT`（相对 D01 基线），运行质量 `NOT PASS`（抽样 26/69 = 37.7%，处置一致性 84.6%）。
- 四条处置错误已按复核改正：READER-A-08 改 ①、READER-A-10 改 ②、READER-B-04 改 ①、READER-C-15 改 ③。
- 新增盲区 B-17（轨迹门只认 `## FINDING` 形状）、B-18（跨角色重复计数）、B-19（材料矩阵未覆盖作者前置区）。
- V2 候选（D03 边界裁决，最多两条）：
  - C-20 轨迹门 v2：反证主张与设计提案入台账；每个面板文件要么有可处置项、要么显式豁免。
  - C-21 假反事实/不可达推演按事实类定级，不默认 reader P2。
  - C-22 材料矩阵对称化：工作产物（如读者画像）也标注允许角色。
  - C-23 禁止 follow-up 大桶：每条转出项给具体目的地与验收判据。
  - C-24 P1 文档侧声明同轮降级：代码转出时，文档句必须同轮改。
  - C-25 L4.5 修复复验：每条 ① 台账列出发现点名的全部位置，修完逐处 grep 回读；缺一处降级（fix-scope = 已改位置数 / 点名位置数）。
  - C-26 引文归属核对：文档修正若依赖另一份注释/载体，必须把该注释列入本轮 diff 或显式转出。
  - C-27 P-REFUTE 映射校验：附加表每行必须与 `refute.md` 总表的 claim ID/结果/分级逐字一致，轨迹门校验。
  - C-28 面板可执行性：L1 并发 ≤3、边查边写盘、每角色重试 1 次后标 MISSING、合并角色标 VARIANCE、禁止 `grep '^#'` 读章节标题。
- V1 继续生效到 D03 边界；D02 的 design-debt 与 teaching-debt 不因升版候选而关闭。

## 五、回退记录

| 版本 | 回退的规则 | 原因 | 证据 | 回退到 |
|---|---|---|---|---|
| — | — | — | — | — |

## 三、候选规则（L6.5 登记，2026-11-08 R13）

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-08 | 结构性修复必须给渲染层读数（`pandoc` 的 `<p>`/`<li>` 计数），不以源文件行数作证据 | D01 R13 READER-R13-F01/F02/F03（B-29） | `prompt/review-rules/fix-guard.md` 的"修后留痕" | 候选，待 L7 |
| C-09 | "文档指控源码注释"类句子必须当场重跑 grep 并记入台账；Doc Code Sync 增"转述性指控复验"一项 | D01 R13 FACT-R13-01 / DESIGN-R13-02（B-30） | `prompt/review-rules/review-doc-checklist.md` §4 | 候选，待 L7 |
| C-10 | 面板与修复批次必须冻结基线并分离：面板期间不改文档，结束后统一分诊；每批修复后跑独立 RBn 回读 | D01 R13（合同第一条/第二条；本轮 d2+reader 各自报出同一拆段问题即为证据） | `prompt/review-rules/review-panel.md`、`review-process.md` | **已生效**（R13 合同），待观察两轮后正式晋升 |

## 四、候选规则（L6.5 登记，2026-11-08 R14）

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-11 | 每条 ① 必须写"旧串 → 新串"；复验时全篇 grep 旧串，命中数必须为 0。替代"复验=五门绿"与人工 fix-scope 计数 | D01 R13 FACT-R13-13 被 READER-C-08 复发；R14 FACT-P2-01 留相反口号（B-39） | `prompt/review-rules/fix-guard.md`"修后留痕"；`tools/eval-trace-check.py` 增旧串检查 | 候选，待 L7 |
| C-12 | `metrics.md` 由台账自动生成（发现数、五类处置、转出档期），`loop.md §6` 只引用生成结果；轨迹门比对三者同一性 | R14 同时存在 90/102/109 三个总数，metrics 停在 R0（B-40） | `tools/eval-trace-check.py`、`review-evals.md` 指标节 | 候选，待 L7 |
| C-13 | RBn 产出必须先入账并写进下一轮冻结说明，R(n+1) 才允许派发面板；并发派发前先数"存活子 agent + 主线程 + 计划派发数" | R14 RB13 与面板并发，同一缺陷两行；R14 首轮 11 并发被 provider 拒绝（B-37） | `prompt/review-rules/review-panel.md` L1 时序 | 候选，待 L7 |

## 五、候选规则（L6.5 登记，2026-11-08 R15）

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-14 | 跨轮缺陷身份键：同一缺陷第二次出现时合并计数并标注"复发"，不再当作新增；`open-debt` 表为唯一欠账源 | R15 DESIGN-02=R14 DESIGN-14、A-26=R14 A-13、BootForm 第三次（B-42） | `review-evals.md` 归因节、台账 schema | 候选，待 L7 |
| C-15 | 轨迹门三源恒等（面板/台账/metrics）+ 处置类桶检测 + severity 非空 | R15 97/107/112 三数并存、sev 全 `—`、② 五模板（B-40 复发） | `tools/eval-trace-check.py` | 候选，待 L7 |

## 六、候选规则（L6.5 登记，2026-11-08 R16；编号已按 meta 终稿合并）

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-16 | 轨迹设"判决"字段：用户裁决（结论/截止轮次/闭案证据）与回读（VERDICT/残留项/处置）必须落账，不得压成条目清单 | RB15/RB16 两次 NOT PASS 被压成"全修"（B-44）；A+D 截止 R13 未闭案 | `review-evals.md` 轨迹 schema、`loop.md` 模板 | 候选，待 L7 |
| C-17 | 台账行 ID 强制 `R<nn>/` 前缀；轨迹门按短 ID 归一匹配，禁止无前缀行 | R16 117/119 条 ID 与更早轮次同名，覆盖门近乎失效（B-31） | `tools/eval-trace-check.py`、`review-evals.md` | **已部分生效**（门已支持前缀；R14–R16 台账均已加前缀） |
| C-18 | 表块连续性检查 + 全文 `.md` 链接按文档目录解析，失败即门红 | R16 §4.4 表被空行劈开、1 个外链从文档目录解析不到（B-46） | `tools/doc-ref-check.sh`、`tools/doc-style-lint.sh` | 候选，待 L7 |

## V2.2（2026-11-09，用户裁决即时晋升——无观察期）

> 用户指令："这些应该是立刻晋升的，不需要观察的（后续观察也是为了确保它们成功正确落地，它们的语义无需考验了）"。晋升依据 = 用户直接裁决（语义由用户定，不需要试点验证）；后续轮次观察的是**落地质量**，不是语义。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-1 | commit 级回归 review 分级 + 当场修 + 复验终止（递归上限 2 层） | R19–R22 修复自引入持续再生（R21 36 条 ④ 未动、R22 18 条未动） | `review-evals.md` §4e、`review-loop.md` L4.6 | **已晋升生效** |
| R-2 | 轮转闸：无证据不进下一轮（①③④ 全部可修项必须已修且附证据：旧串 grep 归零/测试输出） | R21→R22→R23 恒定滞后一轮的"修上轮欠本轮"模式 | `review-evals.md` §4f、`review-loop.md` L6.9、`eval-trace-check.py`（①/④ 复验列非空 + R13+ 收口小节检查） | **已晋升生效** |
| R-3 | 拖欠须人工批准：② 转出未经用户批准按未修计、阻断收敛 | F-R17-01..04、F-R20-01..04 从 R17 逾期至 R23 | `review-evals.md` §4g、`review-loop.md` L6.9 | **已晋升生效** |
| R-4 | 判决字段 8 项（reviewer_verdict 与 user_decision 分列，不得覆盖 NOT PASS） | RB21/RB22 两次 NOT PASS 被压成"全修" | `review-evals.md` §三轨迹契约 | **已晋升生效** |
| R-5 | 裁决档流程：问题域锚 = Minix3 语义（偏离须 [ARCH] 理由）；不设轮次上限；第三方为完整共同设计者；域外即出界 | 用户裁决（BOOT-SCOPE 档 §5）；纠正"轮数上限+挑漏者"的过度矫正 | `DECISIONS-PENDING-*.md` 模板 | **已晋升生效** |

同步门读数：generate-derived-skills 0 / check-review-rules 0 / lint-review-rules 0 / diff-trae-skills 0（全绿）。

## V2.3（2026-11-10，用户裁决即时晋升：删除 ③ 桶）

> 用户指令：“‘③ 成了合法延期桶’，我甚至想要删除它……先解决这个问题！”晋升依据 = 用户直接裁决；R25 meta 的 B-67 提供案例：26 条 ③ 中 19 条没有 §4.1 要求的反证，1 条是真缺陷已修、3 条是待裁决/未实施、15 条是读者体验项。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-6 | **删除 `③`**。误报只能写 `FP`，且 `证据` 列必须给源码/测试/文档原文反证；无证据的“误报”按 `⑤` 未闭环处理。`FP` 不计修复数，不得承接作者取舍/待裁决/未实施 | R25 meta B-67；`CLAIM-03` 真修记 ③、`DESIGN-02/09/10` 待裁决/未实施记 ③、15 条读者项记 ③ | `review-evals.md` §3.3/§4.1/§4.1a/§4.3/§4f/G5；`review-loop.md` L2/L5/L6.9；`eval-trace-check.py` | **已晋升生效** |
| R-7 | **本轮不闭环不准开下一轮**：`①/④/⑥` 必须有复验；`②` 必须有“用户批准 + YYYY-MM-DD”；`⑤` 与未批准 `②` 一律未闭环；R25+ 的 `metrics.md` 必须有 `未闭环: 0`，否则 `eval-trace-check.py` FAIL | R25 在 ③ 未清、DESIGN-09/10 未闭环时曾准备进 R26；R24 meta PASS 与 R25 10 个 P0 并发 | `review-evals.md` §4f/§5.0/§6A；`review-loop.md` L6.9；`eval-trace-check.py`（当前轮 strict 检查 + ③ 禁用 + 未闭环） | **已晋升生效** |

同步门读数：generate-derived-skills 0 / check-review-rules 0 / lint-review-rules 0 / diff-trae-skills 0；`eval-trace-check.py --self-test` PASS（含 R25+ ③ 拦截与 FP 反证拦截）；R25 scoped 运行现为 FAIL，未闭环 10 项（这是预期，不是回归）。

## V2.4（2026-11-10，用户裁决：直接把流程改向收敛）

> 用户指令：“还是再次回归检查，确保这是一个能收敛的流程，别管什么晋升规则……直接把流程给我往收敛的改！”V2.4 不再增加面板角色，而是改轮次状态机：发现轮只跑一次全集，清空后进入验证轮；验证轮只查修复 hunk/未闭环项/随机 20%，不重开全量读者与设计。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-8 | **轮型状态机**：`discovery`（全文全角色，只跑一次）→ 清空 `未闭环=0` → `verification`（P-FACT 定点、READER 只读修改段及邻句、RB 审最终 HEAD，不跑全量 A/B/C/DESIGN）；verification 新 in-scope 真缺陷=0 才 `CONVERGED`；最多 2 次 verification，第 3 次 `STUCK` 交用户 | R24→R25→R26 连续全量面板，发现量 118→124；R25 meta 判 FLAT/NOT PASS | `review-evals.md` §4.0a/§6A、`review-loop.md` L0/L1/L6.9/L7、`review-panel.md` §3.5 | **已晋升生效** |
| R-9 | **收敛不再要求“连续两轮无新”**：G8 改为“最近一轮是 verification 且该轮新 in-scope 真缺陷=0”；FP、重复项不计；收敛后写 `FREEZE.md`，只有用户新要求或新证据才重开 | 26 轮中全量面板反复生成 P2/P3/读者偏好，G8 不可达 | `review-evals.md` §6A、`review-loop.md` L7 | **已晋升生效** |
| R-10 | **RB/commit 发现必须入台账**：写入 `defect-ledger.md` 并计入未闭环；只写在 loop/RB 报告不算入账 | R25 RB 的 B1/B2/N1–N11 只在 loop，未计入 metrics | `review-evals.md` §4f/§4.1、`review-loop.md` L6.9 | **已晋升生效** |

同步门读数：`generate-derived-skills` 0 / `check-review-rules` 0 / `lint-review-rules` 0 / `diff-trae-skills` 0；`eval-trace-check --self-test` PASS；R25 仍为 `NOT CONVERGED`（未闭环 10），验证轮尚未开始。
