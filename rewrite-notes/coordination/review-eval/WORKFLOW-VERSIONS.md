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
| R-8 | **轮型状态机**：`discovery`（全文全角色，只跑一次）→ 清空 `未闭环=0` → `verification`（P-FACT 定点、READER 只读修改段及邻句、RB 审最终 HEAD，不跑全量 A/B/C/DESIGN）；verification 新 in-scope 真缺陷=0 才 `CONVERGED`；同一验证轮内最多 2 次 delta，第 2 次仍有新缺陷即 `STUCK` 交用户；上一轮未闭环 >0 时禁止出现下一轮小节 | R24→R25→R26 连续全量面板，发现量 118→124；R25 meta 判 FLAT/NOT PASS | `review-evals.md` §4.0a/§6A、`review-loop.md` L0/L1/L6.9/L7、`review-panel.md` §3.5 | **已晋升生效** |
| R-9 | **收敛不再要求“连续两轮无新”**：G8 改为“最近一轮是 verification 且该轮新 in-scope 真缺陷=0”；FP、重复项不计；收敛后写 `FREEZE.md`，只有用户新要求或新证据才重开 | 26 轮中全量面板反复生成 P2/P3/读者偏好，G8 不可达 | `review-evals.md` §6A、`review-loop.md` L7 | **已晋升生效** |
| R-10 | **RB/commit 发现必须入台账**：写入 `defect-ledger.md` 并计入未闭环；只写在 loop/RB 报告不算入账 | R25 RB 的 B1/B2/N1–N11 只在 loop，未计入 metrics | `review-evals.md` §4f/§4.1、`review-loop.md` L6.9 | **已晋升生效** |

同步门读数：`generate-derived-skills` 0 / `check-review-rules` 0 / `lint-review-rules` 0 / `diff-trae-skills` 0；`eval-trace-check --self-test` PASS；R25 仍为 `NOT CONVERGED`（未闭环 10），验证轮尚未开始。

## V2.5（2026-11-10，用户裁决：回归检查“流程必须能走向收敛”）

> 用户指令：“再回归检查一次，流程要保证能走向收敛，不是拖着问题，永远有问题永远不收敛，不能逗我玩。”回归实证发现：轮次关账命令跑全历史目录，历史 R14 stale 面板/旧 RB 格式/缺行使门永久 FAIL；收口检查只认 `# Round N`，R14–R25 的 `## R<N> 面板轮` 全部逃检；`未闭环` 是手填值，没有和台账实际未闭环数比对。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-11 | **轮次关账必须轮次作用域**：新增 `--round R<N>`，只查该轮的覆盖、收口、未闭环与 round-order；`## R<N>` 与 `# Round N` 两种标题都受 `### L6`/`### L6.5` 合同约束。不带 `--round` 是全历史归档审计，历史旧格式/stale 面板不阻断当前轮；不得删除历史证据来“变绿” | R25 修到 0 后仍被 11 条历史 FAIL 永久卡死；R15–R25 缺 L6/L6.5 却零报警 | `tools/eval-trace-check.py`、`review-loop.md` L2/L5/L6.9、`review-evals.md` §3.4/§4.0a | **已晋升生效** |
| R-12 | **`未闭环` 必须与台账交叉校验**：声明值 ≠ 台账逐行实际未闭环数（⑤ + 未批准 ② + 复验写“未修/待修/未实施”的 ①/④/⑥）即 FAIL；手填 0 掩盖未修行直接拦截 | R25 台账 10 项未闭环，metrics 可被手改成 0 而不触发旧门 | `tools/eval-trace-check.py`（`actual_open` 交叉校验）、`review-evals.md` §3.4 | **已晋升生效** |

同步门读数：`eval-trace-check.py --self-test` PASS（新增现代标题缺 L6.5 拦截、metrics 手填 0 拦截）；R25 scoped 门剩 10 项未闭环（预期，非回归）；generate-derived-skills / check-review-rules / lint-review-rules / diff-trae-skills 待本批改动后重跑。

**V2.5 追加（同一轮回归检查的第三个洞）**：

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-13 | **轮型状态机必须闭包**：`metrics` 只允许一个 `discovery`；`verification` 之后不得出现任何更高轮号（新缺陷在同一轮 delta 内修，最多 2 次）；02 起 `schema: V2.5 from R1`，01 写 `from R25`；缺 schema 标记的 `--round` 直接 FAIL。否则清空 `未闭环` 后把下一轮再标成 `discovery`，25 轮循环原样复现 | 回归检查发现：旧实现只检查 `未闭环` 与 round-order，未阻止“R26=第二次 discovery”；且 02 的 R1 会因轮号 <25 落回旧 schema | `tools/eval-trace-check.py`（`schema_start_round` / `check_round_type_sequence` / `_round_open_items`）、`review-loop.md` L0 步骤 10、`review-evals.md` §4.0a 规则 8 | **已晋升生效** |

自测新增：R1 在 `schema: V2.5 from R1` 下必须按新 schema 检查；缺 schema 标记必须拦截；R25→R26→R27（verification 后再开轮）必须拦截；R25/R26 双 discovery 必须拦截。

## V3.0（MAJOR，2026-11-10，用户裁决：把无限循环改成有限交易）

> 用户指令：“首先需要做的是 review workflow 的改进与收敛。”V3 不再增加面板角色，而是改三件事：轮次是完整交易而不是发现/验证两种轮型；缺陷有稳定身份并按准入判据收窄；规则只在 edition 边界变更并强制减法。V3 从 01 的 R26 或 02 的 R1 起生效；R25 在 `schema: V2.5 from R25` 下收口，只把收口阶段改成 certify，不另开 verification 轮。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-14 | **轮次交易模型**：删除 `discovery`/`verification` 轮型；一轮 = survey→inventory→fix→certify，同一 `edition` 内完成；下一轮只能在上一轮 `open=0` 且 `certify 新准入=0` 后开；每轮都是完整 review，R26 若存在就是完整 review | R17–R25 连续 9 个全量轮，发现量 87→124；R25 10 项未闭环时曾准备开 R26 verification；R25 meta 判 FLAT/NOT PASS | `review-evals.md` §4.0b/§6A、`review-loop.md` L0/L1/L4/L6.9/L7、`review-panel.md` §3.5、`eval-trace-check.py` | **已晋升生效**（V3 起） |
| R-15 | **缺陷身份**：`根因 ID`/`origin`/`dup-of` 为 V3 台账必填；复发只更新 `last_seen`，不产生新行、不计“新准入”；`发现总数`/`唯一根因数`/`本轮新准入` 三数分列 | R25 124 原始 ≈ 100 唯一、18 个同根簇、约 22 个冗余 ID；READER-A-16 同 ID 出现 7 次；C-14 自 R15 等待 12 轮未落地 | `review-evals.md` §3.4/§4.0c/§10.3、`review-loop.md` L2/L5、`eval-trace-check.py` | **已晋升生效**（V3 起） |
| R-16 | **准入判据 + 读者验收清单**：位置/判据/证据/反证四条全满足才进台账；清单外偏好写 `suggestions.md`，不进台账、不阻塞；读者清单首版 10 条固定 | R17–R23 读者 ④ 338 条（占同期 43.5%）；R25 71 条读者发现里约 55–60 条是真障碍，8–12 条是纯打磨；`review-panel.md` 的 Top-20 配额制造了每轮固定供给 | `review-evals.md` §4.0c/§4.0e/§4.1、`review-panel.md` §4.4/§5 | **已晋升生效**（V3 起） |
| R-17 | **修复批次 ≤20 根因/commit + 100% 复验**：不抽样；每个根因有可重放证据；修复引入新缺陷该批回退；同一根因两次复发升级 ⑥ | R24 修 114 条只抽 29 条（40 文件 +1139/−440），R25 抓到 4 个 R24 引入的 P0；R25 修 98 条只抽 46 条；R6–R12 自引入率 25–30% | `review-evals.md` §4.0d、`review-loop.md` L3/L4 | **已晋升生效**（V3 起） |
| R-18 | **edition 纪律**：版本边界不依赖文档收敛；每个 edition 在同一篇上最多 3 轮，第 3 轮仍 `open>0` 判 `NOT CONVERGING` 并写 `EDITION-RETRO.md`；中途变更当前轮标 `INVALIDATED`；晋升 ≤2 条/边界，强制减法 | 评测期规则改动 7 次（V1→V2.5），每次静默作废旧基线；L7 边界要求 01 收敛，01 不收敛则规则无法正规晋升，只能即时晋升，形成循环依赖；减法账本 3 行 vs 版本表 9 行 | `review-evals.md` §7、`review-loop.md` L7d、`WORKFLOW-VERSIONS.md` 第三节 | **已晋升生效**（V3 起） |
| R-19 | **机械门**：新增 `doc-citation-check.py`（`代码引用` 块逐行 token 对账、`节选` 只查路径/符号）与 `doc-symbol-drift.sh`（旧符号 grep，支持 `--from-diff`）；`eval-trace-check.py` 自测进 CI；新增本地 pre-commit 安装器 | `doc-snippet-extract.sh:6` 明写“判定是 review 的事”；`arch-gated-test-count.py` 不读 `.md`；`eval-trace-check.py` 不在 CI；无生效 hook；R25 的 §3.7/§4.2 两个 P0 是引文漂移 | `tools/doc-citation-check.py`、`tools/doc-symbol-drift.sh`、`tools/install-review-hooks.sh`、`.github/workflows/rules-consistency.yml` | **已晋升生效**（V3 起） |
| R-20 | **G7 改为台账/100% 复验**：`CONVERGED` 不再依赖 meta 抽样 ≥90%；meta 的 PASS/NOT PASS 是流程质量指标，FAIL 不阻止文档关闭。收敛证据 = 根因 ID 唯一、origin 完整、未闭环与 metrics 一致、每个 ①/④/⑥ 有可重放复验 | R25 meta 40/59=67.8% NOT PASS 与 R25 10 个 P0 并发；meta 抽样门本身是随机门（历史读数 37.8%/72.7%/67.8%/88.8%/100%），不能作为停机条件 | `review-evals.md` §5.0/§6A、`review-loop.md` L6/L7a | **已晋升生效**（V3 起） |
| R-21 | **行级证据 / 模板串 / todo 台账三合一门**：①/④/⑥ 的 `复验` 必须含行级 token（path:line / `test::` / grep / 旧串 / commit / 冷读回执）；`五门 PASS`、`见 loop §N` 单独出现不算；同一 `处置`/`复验` 文本 >3 次即 FAIL；reader/design 行额外要求冷读 `CLOSED`/`STILL-BROKEN`；`--audit-templates` 审计历史轮次；`--todo-file` 核对文档级 todo，`允许开放` 必须映射 ②/⑤ | **R24：114/118 行共用同一句 `处置`，113/118 共用同一句 `复验`；R25：97/124 与 97/124。** §2.2 栈对齐同一 P0 被报 7 次，每次记“已修 + 五门 PASS”。全台账 1456 行：机械门-only 933、冷读 2、两者都有 0。`01-todo.md` 4 处 `允许开放` 在门视野外 | `tools/eval-trace-check.py`（`check_v3_evidence` / `audit_template_sections` / `check_todo_ledger`）、`review-loop.md` L6.9、`review-evals.md` §4.0d/§4.1 | **已晋升生效**（V3 起） |

### V3.0 减法账本

| 版本 | 新增 | 替代 / 合并 | 净增理由 |
|---|---|---|---|
| V3.0 | 轮次交易模型 | 删除 `discovery`/`verification` 两种轮型，合并为同一轮的 survey/certify 阶段 | 净减一个轮型；不再有“验证轮承接” |
| V3.0 | 根因 ID/origin/dup-of | 替代每轮重新编号与“新发现”计数 | 无净增；把重复项从“新增”里去掉 |
| V3.0 | 读者验收清单 + `suggestions.md` | 替代开放式读者偏好 Top-20 与已删除的 ③ 桶 | 无净增；清单外偏好不再进台账 |
| V3.0 | `doc-citation-check.py` | 替代 P-FACT 手工引文逐块对账 | 无净增；机械项从 LLM 循环移出 |
| V3.0 | `doc-symbol-drift.sh` | 替代 RB 抽查符号漂移 | 无净增；commit 门替代下一轮发现 |
| V3.0 | 100% 复验 + 批次 ≤20 根因 | 替代 meta 抽样 ≥90% 作为收敛门 | 无净增；更严但减少返工 |
| V3.0 | 行级证据/模板串门 | 替代旧“复验非空、且不含未修/待修”的弱检查 | 无净增；`五门 PASS` 这类轮级声明不再能给单行背书 |
| V3.0 | edition 3 轮试验 | 替代无限全量轮与“01 收敛才升版”的循环依赖 | 净减轮次；不能收敛的是 edition，不是文档 |
| V3.0 | pre-commit 安装器 | 替代“记得手工跑门” | 无净增；本地兜底，共享门仍在 CI |

**旧候选处置**：C-01..C-18 在第一个 V3 边界统一复核，不再无限等待“L7 / V2 边界 / D02 闭环复核”。每条按“案例 + 检测器 + 替代物”三问裁决：三问齐备的晋升（受每次 ≤2 条约束，可分批）；无检测器、无案例或已被 V3 规则覆盖的标 `OBSOLETE` 并删除；仍有价值但缺检测器的转成“机械门候选”，排在晋升队列前面。

**同步门读数（V3.0，2026-11-10 实跑）**：

| 版本 | `generate-derived-skills.sh` | `check-review-rules.sh` | `lint-review-rules.sh` | `diff-trae-skills.sh --only-diff` | 新工具自测 |
|---|---|---|---|---|---|
| V3.0 | exit 0；10 个 skill 与 review-scan 派生件重新生成 | exit 0；源/三端一致 | exit 0；0 个失败 | exit 0；10/10 完全同步 | `eval-trace-check.py --self-test` PASS（含 V3 full/root/certify 正负样本）；`trait-audit.py --self-test` PASS；`doc-citation-check.py --self-test` PASS；`doc-symbol-drift.sh --self-test` PASS |

## V3.1（2026-11-10，R27 `EDITION-RETRO` 边界裁决）

> 依据：R26 判 `CONVERGED`，R27 第二轮完整 review 找到 70 条，其中 P0 是 R26 的 `BootStorage` 合并引入的 `'static` 别名 UB（D2-13）；34 条 P2/P3 读者/结构项进入 backlog。`EDITION-RETRO.md` 判定 V3.0 的修复单位（线级补丁）错误，R28 必须先做结构轮。

| 编号 | 规则 | 案例支撑 | 目标文件 | 状态 |
|---|---|---|---|---|
| R-22 | **B 类/复发根因结构重写**：同一段/小节被报 ≥2 个根因，或同一根因第二次出现，该批必须重写整段/整小节（`rewrite`），不得逐句打补丁；A 类事实项可线级修 | R26→R27 的邻句类发现；R27 34 条 backlog 大半是同一段被反复报 | `review-evals.md` §4.0b/§4.0d、`review-loop.md` L3 | **已晋升生效**（V3.1） |
| R-23 | **certify 所有权/接口顺序复核**：改动触及 `&mut`/`&'static`/生命周期/跨 crate 签名时，重放“写后读”顺序（或 Miri/Stacked Borrows 等价检查），不能只读 diff | R27 D2-13：R26 的 `BootStorage` 合并先洗 `'static` 再 `&mut`，certify 未抓到 | `review-evals.md` §6A G7、`review-loop.md` L4 | **已晋升生效**（V3.1） |
| R-24 | **读者清单第 9 条三问**：不变量是什么、前提是什么、证据在哪；三问答不出才是 FINDING，“更喜欢另一种写法”进 suggestions | R26/R27 同一段反复被读者报“卡住”，第 9 条无可执行判据 | `review-evals.md` §4.0e、`review-loop.md` L6 | **已晋升生效**（V3.1） |
| R-25 | **设计快照历史输入**：审查合同 = 文档 + 源码 + `DECISIONS.md`；`.design/*.md` 与文档/源码冲突时以文档/源码为准 | R27 REFUTE 仍把 `.design/01-design.v3.md` 的 Ch2–Ch4 旧条款当合同，找出“七项取代节未覆盖”的阻断项 | `review-evals.md` §7.8、`review-loop.md` L7d、`01-design.v3.md` 头部 | **已晋升生效**（V3.1） |
| R-26 | **完整 survey 前最终 HEAD 机械再对账**：citation/anchor/symbol-drift 在 survey 前清零，修复 diff 的漂移不留给面板当“新发现” | R27 FACT-A-01/02 是 R26 改代码未回填引文；任何 commit 级 citation 门都能抓到 | `review-evals.md` §7.1、`review-loop.md` L4 | **已晋升生效**（V3.1） |
| R-27 | **第 3 轮必须是结构轮**：按 `EDITION-RETRO` 重写段/小节 + 机械再对账；若仍是线级补丁或仍 `open>0`，判 `NOT CONVERGING` | R26 的 `CONVERGED` 被 R27 推翻；V3.0 3 轮试验的第 2 轮已暴露修复单位错误 | `review-evals.md` §7.5、`review-loop.md` L7d | **已晋升生效**（V3.1） |

### V3.1 减法账本

| 版本 | 新增 | 替代 / 合并 | 净增理由 |
|---|---|---|---|
| V3.1 | B 类结构重写 | 替代“每条发现逐句修”的默认做法 | 无净增；把同一段的多个根因合并为一次重写 |
| V3.1 | certify 所有权顺序复核 | 替代“certify 只读 diff” | 无净增；合并级 P0 不再漏到下一轮 |
| V3.1 | 读者三问 | 替代第 9 条“可复述”的空泛判据 | 无净增；把偏好挡在 suggestions |
| V3.1 | 设计快照历史化 | 替代“快照=合同”的旧读法 | 净减一个假合同源 |
| V3.1 | survey 前机械再对账 | 替代“下一轮面板发现引文漂移” | 净减面板发现量 |
| V3.1 | 第 3 轮结构轮 | 替代“无限线级轮” | 净减轮次；不结构就判 NOT CONVERGING |

**同步门读数（V3.1，2026-11-10 R28 结构轮后重跑）**：`tools/generate-derived-skills.sh` 生成 10 个 skill；`tools/check-review-rules.sh` 报 “source, Trae adapters, Claude runtime references, and Codex adapters are consistent”；`tools/lint-review-rules.sh` 0 个失败；`tools/diff-trae-skills.sh --only-diff` 报 10 个 skill 完全同步；`tools/eval-trace-check.py --self-test`、`tools/doc-citation-check.py --self-test`、`tools/doc-symbol-drift.sh --self-test`、`tools/doc-ref-check.sh --self-test` 均 PASS。

## V3.2（2026-11-10，R30 后边界裁决：用户指示收紧 ② 与回归双盲区）

> 依据：R26→R27 与 R29→R30 两轮各出现"1 P0 + 2 P1"的**自引入同步债**——代码批改动
> 行为后， certify 没有硬步骤拦"文档仍引用旧代码"（R30 FACT-B-01/02）；同时 ② 的
> "用户裁决"形态被用于同篇设计项转出（R26 的 11 条），与"轮次内修复"的目标冲突。
> 用户裁决：② 收紧至相邻范围；同篇待裁决项当轮 ⑥ 实施或 BLOCKED，绝不进下一轮；
> 引文 `节选` 只许省略不许改写；行为级改动必须有 `--stale-lines` 机械网。

| 编号 | 规则 | 判例 | 检测器 | 期望效果 | 目标文件 | 状态 |
|---|---|---|---|---|---|---|
| V3.2-a | ② 只允许相邻范围；同篇待裁决项当轮 ⑥ 实施或 BLOCKED，不得开下一轮 | R26 的 11 条 ② design batch；R30 的 0 条 ② | `eval-trace-check.py` 的 `disposition-v32` 门（V3.2 标记起生效） | 转出不再承接"同篇没修完" | `review-evals.md` §4.1/§4.1a/G5/G9、`review-loop.md` L3 | 已生效 |
| V3.2-b | `节选` 引文只许省略不许改写；显式声明 改写/示意/反例 的块豁免 | R30 FACT-B-01（§4.6 引文停在 `let _ =`，源码已 `match`） | `doc-citation-check.py` 逐行对账扩到 `节选` | 陈旧引文在 commit 门当场拦截 | `tools/doc-citation-check.py` | 已生效 |
| V3.2-c | 代码批→文档同步硬步骤：`--stale-lines <批前 rev> --strict` 零命中 | R30 FACT-B-01/02（行为改动不删符号，`--from-diff` 扫不到） | `doc-symbol-drift.sh --stale-lines` | 自引入同步债在本轮清零 | `tools/doc-symbol-drift.sh`、`review-loop.md` L4、`review-evals.md` §4.0d/G7 | 已生效 |
| V3.2-d | 所有权顺序复核升硬步骤：loop §5 必须留逐条推演记录，缺记录 G7 FAIL | R27 D2-13（certify 只读 diff 漏别名 UB） | `review-loop.md` L4 第 9 条 + G7 文本 | 合并级 P0 不再漏到下一轮 | `review-loop.md` L4、`review-evals.md` G7 | 已生效 |

### V3.2 减法账本

| 版本 | 新装置 | 替代/合并了什么 | 净效果 |
|---|---|---|---|
| V3.2 | `--stale-lines` 陈旧行网 | 合并进既有 `doc-symbol-drift.sh`（不新增工具文件） | 零净增；把"下一轮面板发现引文漂移"改成"本轮 commit 门拦截" |
| V3.2 | `disposition-v32` 台账门 | 替代"② 裁决"形态（删除一个处置子类） | 净减：② 从两种形态收到一种 |
| V3.2 | 节选逐行对账 | 替代"节选 structure-only"的豁免 | 净减一个放行面；改写必须显式声明才豁免 |

**同步门读数（V3.2，2026-11-10）**：`generate-derived-skills.sh` / `check-review-rules.sh` / `lint-review-rules.sh` / `diff-trae-skills.sh --only-diff` 四道门全绿；`doc-citation-check.py --self-test`（含新增 节选陈旧行/省略/显式改写 三夹具）与 `doc-symbol-drift.sh --self-test`（含 `--stale-lines` 临时仓库夹具）PASS；`eval-trace-check.py` 无 V3.2 标记时四轮回归 PASS（休眠验证），注入 `schema: V3.2` 探针后 ② 非相邻形态全部 FAIL（强制验证）。

## V3.3（2026-11-10，R30/R31 边界裁决：用户批准四态模型 + 确认轮 + 债务入门）

> 依据：R26/R30 **两次假阳性**（修改轮自判 `CONVERGED`，均被下一轮完整 survey 推翻；
> R30 借"代理口径 A"越权）；R31（用户要求的追加轮，事实上是一次确认轮）找到
> 1 P0 + 3 P1，证明强定义正确但规则组合不完整——确认轮在 §7.5 的 3 轮窗口内无处
> 安放；R26 的 11 条设计债经 ② 批准转入 `OPEN-DEBT.md` 后，在 R27–R31 五轮之间
> 无任何执行器（阴影账不在门视野内，文件头还自宣布"不是收敛的必要条件"）；
> R30 台账 70% 是 P3，而旧 G3 要求"准入 P3 = 0"——不可达判据；R29 重写 §2.6.4、
> R31 在同段找到两份页目录的机制级 P0——"重写段是下轮 P0 的最大来源"三度应验。

| 编号 | 规则 | 判例 | 检测器 | 期望效果 | 目标文件 | 状态 |
|---|---|---|---|---|---|---|
| V3.3-a | 四态模型：ROUND-CLOSED / CONVERGENCE-CANDIDATE / CONVERGED / FREEZE-WITH-BACKLOG；修改轮无收敛声明权 | R26、R30 两次假阳性 | `eval-trace-check.py` 的 `lifecycle-v33` 门（V3.3 标记起生效） | 假阳性在声明前被拦截 | `review-evals.md` §6A、`review-loop.md` L0/L6.9/L7a | 已生效 |
| V3.3-b | 确认轮：全量 survey、只修新准入 P0/P1/P2 事实/同步项、不实施 ⑥、每个 CONVERGENCE-CANDIDATE 最多 2 次（edition 更替或批准重开后重置）、不占修改预算；`certify 新准入=0` 才可 CONVERGED | R31（事实上的首次确认轮，找到 1 P0 + 3 P1，正确拒绝收敛） | `lifecycle-v33` 门 | 收敛声明有唯一合法出口，且出口可机械判定 | `review-evals.md` §6A/§7.5、`review-loop.md` L0/L6.9/L7a、`review-panel.md` §3.6 | 已生效 |
| V3.3-c | 读者/设计 P3 默认进 suggestions；准入需"读者卡住无法继续"测试；G3 不再要求准入 P3=0 | R30 台账 70% 是 P3；R31 §5 自建议同一改法；用户原始意图（"P3 不影响收敛"） | `admission-v33` 门 | P2/P3 长尾（每轮 ~50 条持平）不再阻断收敛 | `review-evals.md` §4.0c/G3、`review-panel.md` 读者 brief；设计 P3 同门槛（R29–R31 设计角色每轮 8–9 条、P2/P3 占多数，不收口则确认轮永远封不了盘） | 已生效 |
| V3.3-d | OPEN-DEBT 入门：未销账行计入未闭环，三个合法终态（实施/被覆盖/dup-of）+ 判决账行（含**用户批准的延期**——工具识别 `用户批准`+日期，WARN 留痕）；逾期逐条点名 | D-043 的 11 条五轮无执行器；`OPEN-DEBT.md:5` 自宣布"不是收敛必要条件" | `eval-trace-check.py` 的 `open-debt` 门 | "有债务的循环"从白费变成不可启动 | `review-evals.md` §4.0f、`eval-trace-check.py` | 已生效 |
| V3.3-e | 改写段源码再推演：本轮改写的机制段，certify 附逐断言 vs C 源推演记录，缺记录 G7 FAIL | R31 §2.6.4（R29 重写、R30 触碰、R31 P0：两份页目录/TLB 论证不存在） | `review-loop.md` L4 第 10 条 + G7 文本 | 机制语义错误在重写当轮被回查 | `review-loop.md` L4、`review-evals.md` G7 | 已生效 |
| V3.3-f | origin 记引入轮（自引入不记发现轮）；severity 列与面板标题机械同步 | R30：3 条 R29 自引入记 origin R30；面板 P0 1/P1 3 vs 台账 P1 2/P2 15/P3 40 无 P0 行 | `admission-v33` 门（severity）+ §4.0c 文本 | "本轮新准入"跨轮可比；确认轮的 0 准入判据有区分度 | `review-evals.md` §4.0c/§3.3、`eval-trace-check.py` | 已生效 |
| V3.3-g | 同篇 edition 上限 K=3（软）：超出须判决账行 | V3.1 在第 4 轮（R31）突破窗口，无记录 | `review-evals.md` §7.5、`review-loop.md` L7d 文本 | 无限换规则重试变成显式决定 | `review-evals.md` §7.5、`review-loop.md` L7d | 已生效 |

### V3.3 减法账本

| 版本 | 新装置 | 替代/合并了什么 | 净效果 |
|---|---|---|---|
| V3.3 | 四态模型 + 确认轮 | 替代旧 §6A"修改轮自判 CONVERGED"的弱定义与 §4.0a 残留的 verification 概念 | 净减一个声明权（修改轮的）；确认轮复用既有全量面板与交易阶段，零新角色零新工具 |
| V3.3 | `open-debt` / `admission-v33` / `lifecycle-v33` 三门 | 合并进既有 `eval-trace-check.py`；OPEN-DEBT 门替代"follow-ups 大桶"与"阴影账"实践 | 零净增工具文件；删除 `OPEN-DEBT` 的"非必要条件"豁免面与 G3 的"准入 P3=0"不可达判据 |
| V3.3 | 改写段再推演 | 合并进既有 certify 硬步骤序列（与所有权复核同型），不新增面板角色 | 零净增；把"下一轮 survey 发现机制错"改成"本轮 certify 回查" |

**同步门读数（V3.3，2026-11-10）**：`generate-derived-skills.sh` / `check-review-rules.sh` / `lint-review-rules.sh` / `diff-trae-skills.sh --only-diff` 四道门全绿（注意：本地非交互 shell 的 `rg` 是 alias、PATH 无 cargo bin 时这四道门会整批误报 command-not-found——须 `PATH="$HOME/.cargo/bin:$PATH"` 后运行，CI 环境不受影响）；`eval-trace-check.py --self-test` PASS（新增 OPEN-DEBT 未销账/逾期、读者 P3 准入、severity 同步、修改轮自判 CONVERGED、确认轮实施 ⑥ 五个负样本夹具）；`trait-audit.py --self-test` PASS。实弹验证：`--round R31` 现况 FAIL，`open-debt` 门点名 R26-design-batch（逾期，截止 R27）——设计批清账前 R31 不得关闭，符合 §4.0f 预期。

### V3.3 追加（D-047：代裁决授权边界 = Minix3 语义锚 + ARCH 硬阻断）

> 用户裁决（2026-11-10）：全代裁 + 事后审阅的效率方案成立，但边界 = Minix3 语义锚
> （用户判例：借槽→directMap 是删除 Minix3 机制 = ARCH；删翻译残留 = 回归对齐 = 代裁；
> "把 boot 往小 OS 发展"的趋势由 ARCH 提案的集合级审阅拦截）。ARCH 级硬阻断——
> 即便用户口头说"继续"，解除只认判决账行（门不认识聊天，只认识行）。

| 编号 | 规则 | 判例 | 检测器 | 期望效果 | 目标文件 | 状态 |
|---|---|---|---|---|---|---|
| V3.3-h | 代裁决授权边界：设计 ⑥ 行必须标 `[MODERN]`/`[DEL]`/`[ARCH]`；未分类按 ARCH 阻断（举证责任在代裁侧）；`[ARCH]` 实施缺 `D-xxx` 判决行引用 FAIL；`[ARCH]` P0/P1/P2 → ⑤+`轮结果: BLOCKED`（硬阻断），P3 → 判决账提案（不阻断）；治理仪表 `代裁决`/`ARCH 提案`/`用户否决` 进 metrics | 用户否决史：批准的全是"把 boot 变小"（删 trait/删 kern_ident/删 ident_regions），否决的全是"加责任"（诊断打印/records_in_wire 语义化/4 GiB 断言）；借槽→directMap 判例 | `eval-trace-check.py` 的 `design-class` 门 + `lifecycle-v33` ARCH-BLOCKED 一致性 + metrics 三仪表 | 80% 代裁零等待；20% 纠结区强制用户亲裁且必须落账；语义漂移在集合级可见 | `review-evals.md` §4.1b/§3.4、`review-loop.md` L2/L5、`review-panel.md` DESIGN brief 第 7 条、`eval-trace-check.py` | 已生效 |

减法：`design-class`/治理仪表合并进既有 `admission-v33`/`lifecycle-v33`/`check_metrics`，零新工具文件；`BLOCKED` 载体复用既有 ⑤/round-order 门。ARCH 提案走判决账，不新增第八个处置桶。
