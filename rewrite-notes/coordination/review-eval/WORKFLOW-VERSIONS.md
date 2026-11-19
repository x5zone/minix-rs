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
| **V3.4** | **2026-11-18** | **最小内核重实施（经 D-056/D-057）**：R6-a 确认轮前置修改轮校验、R6-c BLOCKED 两级解除、P1-7 certify 待跑门（schema V3.4 + loop 侧）、N0 轮序双源（--round 作用域）、P2-8 citation C 侧三形态、P0-3 meta 修约、P1-5/R6-b 回归审计入账与 FREEZE append 语义 | r6 四队验收 + D-056（丢失→批准重实施）；traps 边界读数 boundary-V3.4（零翻转） | 逐条见本节减法子表 | 见本节同步门读数 | 生效（D05 起标 `schema: V3.4`） |
| **V3.5** | **2026-11-18** | **MAJOR（证据类型切换）制品链**：面板与 certify 回执入库 + INDEX.md 哈希索引（panel-index.py）+ 轨迹门制品校验；威胁模型（门防事故不防全伪造）与门收敛判据（两轮零新增 P0/P1 + 判别夹具）写死 | r8 后用户质询「每轮出新 P0」+ 四轮对抗判例；用户裁决「按照你的建议来吧」 | 见本节减法子表 | 见本节同步门读数 | 生效（D05 起） |
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

### V3.6 候选：设计形态判据（2026-11-20 用户裁决方向，登记不生效）

> **目标锚（用户原话）**：本项目目标是「中文社区最好的 OS 教程」。在这个目标下，
> 「闭包能解决却新增类型」不是纯工程取舍——它同时伤害设计最优性与教学可读性。
> 判据与下面的减法条款都以这个目标为最终的取舍尺。

| 编号 | 候选 | 案例支撑 | 目标文件 | 现状 |
|---|---|---|---|---|
| C-31 | **防御形态按威胁模型分层**：新增类型级防御（newtype / token / 守卫字段 / 值对象）必须回答三问——①被防的故障由谁制造（外部攻击者 → `Result` + 完整校验，成本不计；本项目其他模块的 bug → **优先 fail-fast 断言**，零类型成本；同一调用点用错 → 才考虑类型级防御）；②是否已被 borrow checker / 类型系统拦住（`map_dirty` 判例：拦不住真故障，删）；③故障发生率×后果代价 vs 新增理解成本（新增字段数 / API 表面倍数 / 调用点改动数） | D-067（用户裁决：删 `map_dirty` / 否决"每族补 dirty" / 取 E 闭包 / 保留浅层 `MapCounts`）；D-030（删三层空通道）；D-026（删单实现 `BootShim` trait）；反例 D-042（`PhysRange` 保留，因接反静默） | `prompt/review-rules/review-panel.md` D2 brief 纪律段、`review-code-excellence.md` §类型/状态机「范围刹车」处 | 候选（用户已认可方向，待边界条款定稿） |
| C-32 | **减法优先（DESIGN 必答）**：D2 报告必须先说明「这件事能否用更少的类型 / 更少的 API 表面达成同样保障」，减法方案与加法方案并列呈报；只报加法 = 报告不完整 | D-067（面板原提案全为加法，减法是用户在裁决时补的）；18 轮门层修复的同类模式（每次修复新增表面积） | `prompt/review-rules/review-panel.md` §4.3 D2 brief 任务项 | 候选 |
| C-33 | **新类型的可判边界**：同 crate 内调用点用错而值得类型防御，需同时满足「该类型的构造点 ≥2」且「接反/错用是静默的（编译器与测试都看不见）」；不满足者判 P2 建议而非缺陷 | D-067 / D-042 为正例（构造点多、接反静默）；D-035（不引入 `CheckedWindows`，当时只两个调用点）为反例侧边界 | `review-code-excellence.md` §类型/状态机、`review-panel.md` §4.3 | 候选（阈值待边界裁决确认） |
| C-34 | **减法也要计数**：`metrics.md` 增「减法提案数 / 采纳数」两键，与既有 `重构: 提出/接受/实施/登记` 并列；用于打破「面板只报加法」的奖励回路 | 18 轮对抗 + 多篇面板的实际分布（加法条目更易被采纳：D-026/D-030/D-042 均为用户侧纠偏） | `review-evals.md` §3.4 `metrics.md` 固定键、`review-loop.md` L5 | 候选 |
| C-35 | **教学面成本入判据**：任何新增抽象须说明它在教程读者视角的代价（新概念引入数 / 讲解段落增量 / 与既有叙事结构的冲突）；「能让读者少记一个概念」的方案在同等保障下优先 | 项目目标（中文社区最好的 OS 教程）；`review-code-excellence.md` §19 代码即文档；D-067 的 E 闭包方案（闭包是教程读者已需掌握的一类概念，而 guard 字段是新概念） | `review-code-excellence.md` §19、`review-panel.md` §4.3 | 候选 |

**候选总纲**：C-31 是判据主干，C-32/C-33 是它在面板与类型边界的可执行化，C-34 是
反奖励回路，C-35 是把项目教学目标写进取舍尺。五条共同回答一个问题——**「这个新增
抽象是必要的，还是只是让代码看起来更安全？」**


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
| V3.3-c | 读者/设计 P3 默认进 suggestions（台账以 SUGG 行登记——覆盖率门与准入门同时满足，回归四修正）；准入需"读者卡住无法继续"测试；G3 不再要求准入 P3=0 | R30 台账 70% 是 P3；R31 §5 自建议同一改法；用户原始意图（"P3 不影响收敛"） | `admission-v33` 门 | P2/P3 长尾（每轮 ~50 条持平）不再阻断收敛 | `review-evals.md` §4.0c/G3、`review-panel.md` 读者 brief；设计 P3 同门槛（R29–R31 设计角色每轮 8–9 条、P2/P3 占多数，不收口则确认轮永远封不了盘） | 已生效 |
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

## V3.4（2026-11-18，重实施：最小内核；首版全量实现被并发硬重置丢弃，经 D-056 重立）

> 历史：V3.4 全量版（a–m）于 2026-11-18 实施并通过四队回归，随后被并发会话硬重置
> 丢弃（reflog：`13fe44d6e` 上 `reset: moving to HEAD`；D-056 全弧线）。用户批准按
> r6 验收建议重实施**最小内核**（8 项）；其余项（V3.4-d/e/f 其余、k、档位 <500、
> N9、N12、R4-c、R4-e、R3-c/e、N13、R01–R12 刷新）登记为 V3.5 候选。

| 编号 | 规则 | 判例 | 检测器 | 状态 |
|---|---|---|---|---|
| V3.4-m1（R6-a） | 确认轮宣告 CONVERGED 前必须存在已关闭修改轮（文档级，跨 edition 合法） | r6 伪造确认轮路径（lifecycle 0 FAIL） | `lifecycle-v33` R6-a 检查 + 夹具 | 已生效 |
| V3.4-m2（R6-c） | certify 型 BLOCKED 两级解除（独立冷读仲裁 → 用户讨论落判决账） | R3-b/R6-c 死锁判例 | §4.0b 文本 | 已生效（文本） |
| V3.4-m3（P1-7） | 闭合态轮不得含「certify 待跑/未跑」；覆盖 metrics + loop §5；限 `schema: V3.4` | D03/D04 同轮并存穿透旧门 | eval-trace-check + 夹具（v34 双向） | 已生效 |
| V3.4-m4（N0） | 轮序双源（台账 + metrics 账目键）；`--round` 只对本轮及更早负责 | R31 断链走到 CONVERGED；R26 追溯误红 | round-order 双源 + 夹具 | 已生效（D-057 授权预期红） |
| V3.4-m5（P2-8） | citation 门支持 C 侧三形态标注 + `minix3/*.c|h` 路径 + `/* */` 行跳过 | D03/D04 42/86 块假绿 | doc-citation-check + 夹具 | 已生效 |
| V3.4-m6（P0-3） | meta 修约：收敛时一次独立冷启动 meta；其余主 agent 收口如实标注 | R27 起停摆无修约 | §5.0 文本 | 已生效（文本） |
| V3.4-m7（P1-5+R6-f） | FREEZE append-only（追加回归节 + 更新 sha 行之外不得改写） | 02 sha 原位改写无规则定义 | §4h 文本 | 已生效（文本） |
| V3.4-m8（R6-b） | 回归轮必须在台账建行级账 + loop 索引节；只写 FREEZE 散文 = 轨迹缺口 | 02/03 回归轮（30/33 条）零台账行 | §4h 文本；02/03 已补账 | 已生效（文本+补账落地） |

### V3.4 减法账本（重实施版）

| 版本 | 新增 | 替代 / 合并 | 净效果 |
|---|---|---|---|
| V3.4 | 最小内核 m1–m8 总览（逐条见下 8 行） | 替代「确认轮无前置 / BLOCKED 死锁 / certify 待跑容忍 / 台账单源 / citation 假绿 / meta 停摆 / FREEZE 无 append 语义 / 回归轮散文载体」八项旧态 | 5 项合并进既有门、1 项净减义务、2 项净增（仲裁者一级 + 行级账义务）；全部有判例 |
| V3.4-m1 | 确认轮前置校验 | 替代「确认轮无前置」的执法空洞 | 合并进 lifecycle 门 |
| V3.4-m2 | 两级解除协议 | 替代「交用户」单级出口与死锁 | 净增仲裁者一级（配给层） |
| V3.4-m3 | certify 待跑门 | 替代占位文本容忍 | 合并进 metrics/round-closure |
| V3.4-m4 | 轮序双源 | 替代台账单源 open 集 | 合并进 round-order |
| V3.4-m5 | citation C 侧 | 替代 `/* */` 全跳过假绿 | 净增覆盖；无新工具 |
| V3.4-m6 | meta 修约 | 替代「契约逐轮、实践停摆」 | 净减义务（逐轮→收敛时） |
| V3.4-m7 | FREEZE append 语义 | 替代「可原位改写」无定义态 | 文本合同 |
| V3.4-m8 | 回归轮入账 | 替代 FREEZE 散文载体 | 行级账义务（02/03 已落地） |

**同步门读数（2026-11-18 重实施实跑）**：`generate-derived-skills.sh` 10 skill；
`check-review-rules.sh` exit 0；`lint-review-rules.sh` 0 失败；`diff-trae-skills --only-diff`
10/10；`eval-trace-check.py --self-test` PASS（新增 R6-a/P1-7 双向/N0 三态夹具）；
`doc-citation-check.py --self-test` PASS（C 侧三形态 + 陈旧行）。

**边界回归读数**：`meta/boundary-V3.4.md`（已入库）——T01–T08 阳性 6/6、阴性 2/2、
误报 0，与 V1 边界零翻转（11-18 实测，D-055 规则面；最小内核只增门不删检测，读数
继续有效；下一边界复跑）。

**预期红（D-057 授权；r7 更正为逐项列明；r8 起计数随 strict/制品口径演进，最新数字以 D-057 行为准——下段个别数字为 r7 时点）**：

- D01 `--round R31`：**6 项 FAIL**（round-order 断链 + coverage 53 条无台账 +
  metrics 三项不一致 + loop-sections）——同一根因：R31 自判不收敛、无台账无 metrics；
- D01 `--round R32/R33/R34`：各 1 项 round-order FAIL（R31 断链在前）；
- D01 `--round R26/R28/R29/R30`：PASS；`--round R27` 为已披露存量（metrics 4 项，
  §1–§6 待填遗留，早于 V3.4）；
- D05 `--round R1`：已披露存量（V2 基线未迁移，15 项）——**授权在 R1 面板启动、
  §0 改标 `schema: V3.4` 时一并重立基线消除**（r7 补充授权，D-057 同步）；
- 解除路径 = R31 补清点（53 条补 `dup-of` 回填）或判决行。



**V3.5 候选（未实施项去向）**：V3.4-d/e/f 其余（假完成子串、债务到点 FAIL、超窗
FAIL、核查事件键）、V3.4-k（边界读数门）、档位 <500 冲突、N9/N12、R4-c/R4-e、
R3-c/e、N13 小项、R01–R12 刷新——按 §7.2 每边界 ≤2 条逐批处理。

### V3.4-r7 收敛性验收修正（2026-11-18，四队独立 review + 修复 + 回归）

四支独立队伍（端到端演练 / 对抗猎手 / 机械审计 / 规则一致性冷读，报告
`tmp/process-review/r7/`）对 V3.4 最小内核做了收敛性验收，确认的问题与修复：

| # | 发现（判例） | 修复 |
|---|---|---|
| r7-1 P0 | citation 第四标注形态 `// path —— kind`（grep 形态行 D04 41 / D05 52；实检新增 D04 +40、D05 +45）整族假绿（P2-8 判例复发） | `doc-citation-check` 支持该形态 + `签名示意` kind + `.S/.lds/.inc` 路径 + `#` 注释跳过；实弹 D04 checked 35→75、D05 0→45；D04 新检出 1 条真实陈旧 import（已按源码修正，FREEZE 追加补记）；D05 新检出 3 条漂移（预登记 follow-ups，R1 清点） |
| r7-2 P0 | R6-a 可被两行伪造满足（`full`+`ROUND-CLOSED` 空块计入 mod_closed） | 计入要件加「块内有 `发现总数` 键」+ 夹具 |
| r7-3 P1 | `--round R1` 前缀串扰 R10+ 台账节（`startswith`），反向污染分档聚合 | 新增 `_round_key_matches` 词边界助手，替换 6 处 + 夹具 |
| r7-4 P1 | N0 双源只看「有没有账」不看「账说什么」——metrics 自报未闭环>0 的轮静默越序 | 双源检查读 `未闭环` 值 + 夹具 |
| r7-5 P1 | P1-7 词表可绕（`certify 未执行/没跑/延后/排期未定`） | 词表扩充（metrics 与 loop 两侧） |
| r7-6 P1 | 预期红不逐项：R31 实测 6 项 vs 账面「各 1 项」（违反 D-057 自定标准） | 本节上方更正 + D-057 同步 |
| r7-7 P1 | SUGG 口径四方分裂（进不进台账/metrics、五/六/七项之和） | §3.3/§3.4/§4.0c/§八.6/loop L2 五处统一：SUGG 行入台账、计入 SUGG 键与发现总数（七项）、不参与 G1–G9 |
| r7-8 P1 | loop L0.10 教写 `schema: V3.3`——D05 起新门永不武装（e2e 阻断） | 改为「新开篇目写 V3.4 from R1」+ 基线修正路径（VARIANCE，非轮中改规） |
| r7-9 P1 | 档位表两源冲突残留（<500 形状相反、D1 三口径）——P0-1 判例复发 | loop L1 改纯引用 §七 + D-024 查询义务；panel §七加判据优先级 |
| r7-10 P1 | §八 10–14 幽灵宣称复发（P2-9 判例） | 诚实性标注：10–14 为人工合同，V3.5 候选 |
| r7-11 阻断 | RB 角色五处引用零任务书（R6-d） | panel 新增 §4.7 RB brief |
| r7-12 阻断 | L2 跑 `--round` 必然死循环（R6-e：收口产物在 L5/L6） | L2.8 加 L2 时点豁免（只裁决覆盖与处置类） |
| r7-13 P1 | §4g-2 排在 4g 前（编号乱序） | 改号 §4h 并移位；loop/账本引用同步 |
| r7-14 P1 | BLOCKED 三型三处口径未汇总；解除后轮状态变更无机制 | §4.0b 加三型一览 + 解除后须过门 |
| r7-15 FAIL | fix-guard 副本缺 ⑥ 整节（N7 判例复发，无门覆盖） | 副本同步 + check-review-rules 副本 diff 门 |
| r7-16 P2 | L6 逐轮 meta 残留（修约后仍无条件指令） | L6.2/6.4 限定两个时点 |

**未修登记**（V3.5 候选，见上节去向表）：§3.2 重号、L4 命令清单缺 doc-structure、
「跨篇 meta」执行定义、五门计数漂移、02/03 回归面板仅存 tmp/（克隆不可复现）、
R04-T13 判决行缺失（已在 OPEN-DEBT 注记，待用户裁决或并入 design batch 判决）。

**r7 修复回归**（独立子 agent 审 fix diff，`tmp/process-review/r7/fix-regression.md`）：
16/16 宣称落地、实弹对账吻合、四门全绿，**判定通过**；新发现 11 条均 P2/P3（账面
勘误、恒绿夹具、死代码、措辞张力），其中 4 条已随本批清除（FREEZE 74→75 勘误、
mk 夹具改为可判、夹具死代码删除、§七优先级措辞），其余登记 V3.5 候选。

### V3.4-r8 跨模型确认修正（2026-11-18，四队独立 review + 修复 + 回归）

跨模型确认轮：4 支独立队（端到端演练 / 对抗猎手 / 机械审计 / 规则冷读，报告
`tmp/process-review/r8/`）复核 r7 后的版本，确认 2 个新 P0、若干 P1 与规则内部冲突，
全部修复如下。

| # | 发现（判例） | 修复 |
|---|---|---|
| r8-1 P0 | R6-a 仍可伪造：`发现总数: 0` 两行空块计入 mod_closed（r7 的 `_has_total` 只查键）；`# R<N>` 单级标题使轮块不可见 | mod_closed 加**台账节要件**（`_has_round_ledger`：该轮台账节且非空）；`_pick_metric_block` 在 V3 的 `--round` 模式**不回退**（块缺失即 FAIL）；夹具 5 处同步 |
| r8-2 P0 | 零面板可关轮（coverage/severity/admission 空转，只 WARN）——D05 式阴影账可机械通关 | V3 关轮**必须有 panel 目录**，缺席 FAIL（实弹：伪造 run 16 FAIL 含面板证据项） |
| r8-3 P1 | N0 双源缺 `未闭环` 键即默认已关闭 | 缺键按开放计 + 夹具 |
| r8-4 P1 | P1-7 词表 17/20 绕过（待复跑/尚未启动/回读未做/pending/not run/推迟/TODO…） | 词表扩充（metrics 与 loop 两侧） |
| r8-5 P1 | citation 声明式豁免（`签名示意`/notes 子串）计为 checked——虚高「引文已对账」读数 | structure-only 块改 **WARN** 不计 checked；实弹新口径：01 28+5W、02 25+3W、03 32、04 69+6W、05 40+3F+5W（failed 数不变，D05 三条为已预登记漂移） |
| r8-6 P1 | 台账行无编号节归属（无 `## R<N>` 标题的行对所有 scope 检查不可见） | §4.0c 明文：台账行必须放在 `## R<N>` 编号节下；§4h 补回归行归属（`## REG-<date>` 节，不参与 --round，由回归审计人工全核） |
| r8-7 P1 | L2 豁免漏 `loop-sections`（同族 L2 时点必然 FAIL） | L2.8 豁免补 loop-sections |
| r8-8 P1 | §八.10 标注不准确：P-REFUTE 的 CLAIM 覆盖有机械面（主台账），附加表形状不被解析 | 标注更正（r8） |
| r8-9 P1 | ARCH「延期 → ⑤」是死选项（⑤ 与 lifecycle ARCH-BLOCKED 强制冲突，永久 BLOCKED） | 延期改为**转 OPEN-DEBT + 判决账行**（P0/P1 须风险接受；P2/P3 批准即销账） |
| r8-10 P1 | RB 回执无入账约定（三类清单无 FINDING ID） | §4.7 补入账约定：rb.md 不按覆盖门核对，NEW 项由主 agent 按 §4.0c 准入 |
| r8-11 P1 | 跨篇 meta 无执行定义（Who/When/Where） | §5.0 补最小定义（`meta/cross-<a>-<b>.md`，输入输出，同 agent 标注） |
| r8-12 P1 | SUGG 适用范围与工具不一致（规则说所有角色，工具限读者/设计 P3） | §3.3/§4.0c/§4.1/panel 两处对齐工具：SUGG 限读者/设计项；事实类证据不足写 ⑤ |
| r8-13 P1 | §3.4「轮型只能是 full」与 structural/confirmation 并存；§4.0b rule2 与 structural 轮种张力 | 轮型枚举补全；rule2 加 structural 输入口径 |
| r8-14 P1 | L6.9.5 无条件 `--todo-file`（D05 无 todo 文件必 FAIL）；L0.8 对上一 V1 轮未关闭的存量旧 ② 无处置口径；D05「预登记不计入台账」制造第四状态 | todo 文件可缺省（注明）；L0.8 补存量裁决口径；D05 note 改为「R1 后按 §4.0c 准入，不得驻留台账外」 |
| r8-15 P1 | AGENTS.md 收敛停止与 review-loop 轨道收敛定义冲突（两套并存未点名） | AGENTS §6 加轨道消歧（普通 review vs loop §6A） |
| r8-16 卫生 | 04 OPEN-DEBT 表格 4 列 vs 3 表头（r7 注记破坏，doc-structure CI 必红）；D04 FREEZE sha 行未更新；README 版本行仍 V1；r7 表「41 块」实测 40 | 全部修正：注记移出表格、sha 行更新为 `ff8aad7e…`、README 导航行更新、r7 表注解为「形态行 41 / 实检新增 40」 |
| r8-17 登记补漏 | panel §4.3 的「D1 缺席回退」块在首轮表内未登记 | 已补入本节（F10） |

**修复回归（独立子 agent，修复后判定）**：首轮判定「需补丁」——F1 面板门只查目录
存在（`mkdir panel` 可绕）、F2 六项新逻辑零判别夹具、F3 预期红计数因 strict 变化
失真（R27 4→9、R31 6→16）、F4 rb.md 与覆盖门冲突、F6 ARCH 延期缺落地处置类、
F8 `todo` 词误伤等；全部修复，复验通过（F1 伪造 19 FAIL 含面板证据、F2 变异抽验
判别有效、F3 计数更新如下）。

**读数（r8 终态实跑）**：四道规则同步门全绿；六自测 PASS（新增 8 组判别夹具：台账
要件/缺键开放/词表扩充/strict 不回退/空面板/ARCH 延期两态/结构豁免 WARN）；轨迹
矩阵 16 格 = 10 PASS / 6 FAIL——**逐项计数更新**：R27=9（存量，strict 后增）、
R31=16（授权，strict 后增，同根因 R31 无账）、R32/R33/R34 各 1、D05 R1=16（授权，
空面板 +1）、02/03/04 六轮全 PASS；citation 新口径五篇 0 failed（D05 3 FAIL 为
预登记漂移）；doc-structure 04 OPEN-DEBT 单文件通过（全目录仍 6 个存量文件红）。

**修复回归轮新增修复（F4–F10）**：rb.md 覆盖门豁免（§4.7 回执）；ARCH 延期三要素
（⑤ + 用户批准 + 日期 + D-xxx）工具豁免 BLOCKED；SUGG 角色范围三处对齐（§3.3/
§4.0c/panel §五）；L0.8 例证事实更正（72 条在 R1 台账与 follow-ups 两处）且
「补日期不得事后追认」；`todo` 词移出 P1-7 词表；panel §4.3 D1 缺席回退块登记入表。

**未修登记**：§3.2 重号；§4.1/§4.1a 点号风格；「五门」三处成员不一致；降档阶梯
6→3→1 对 7 角色；panel「五个角色/五段」计数；字段数 6/7/8 三口径；§3.3 八列旧表头
与 §4.0c 十列并存；review-core/review-process 两份浓缩镜像与「同源」措辞（需用户
裁决是否改措辞或加锚点门）；CI 的 pattern-gate/command-boundary 存量红与 doc-structure 的 6 个存量文件红（DECISIONS/01-suggestions/02-三件/03-ledger，均为 HEAD 前既有、本批未触碰）；R04-T13
判决行；3 个 11-10 untracked 入库——均登记为 V3.5 候选或用户裁决项。

## V3.4-r10/r11（2026-11-18/19，收敛循环第 10/11 轮；协议：单轮零 P0=候选，加验一轮确认，最多 10 轮）

- **r10**：对抗+机械两队。边界外 P0×4——① SUGG 账目死锁（合规记账双 FAIL、漏交全绿）；② 缺 `发现总数` 键四校验静默跳过；③ FREEZE 子串解锁（NOT/尚未/曾）；④ certify 空心回执（纯标点绕过字节门槛）+ P1（结论：REOPEN 放行）。全部修复 + 6 组判别夹具；SUGG 制品口径限定在制品必需文档（历史冻结轮不追溯）。提交 a862e5f60 + 98b95cb05。判定 NOT PASS。
- **r11**：对抗+机械两队。边界外 P0×3——① FREEZE 正向行是黑名单（「还没 CONVERGED。」「目标：达成 CONVERGED 后冻结。」「CONVERGED-CANDIDATE（待确认）」「是否 CONVERGED？」四型绕过）；② certify 否定词表不穷尽（裸 REOPEN/结论 FAIL 空格/没通过/全角 NOT PASS）+ 无肯定式结论要求；③ 带 ID SUGGESTION 借标题形状（###/复数/中文 ID）漏出三账，且规范无形状契约。P1：INDEX 复点与 panel-index 口径分叉。**修复改为肯定式判据**：FREEZE 豁免 = 标题行（`# … FREEZE …`）必须携带终态；certify = 必须自带显式通过性结论行（结论/判定/验证/确认 + PASS/CLOSED/通过/闭环）+ 否定扫描（含全角归一与独行 REOPEN）；SUGG 形状契约写进规则并三正则同形（`#{2,3} SUGGESTION <ASCII-ID>`）。判定 NOT PASS，修复已落地。
- **计数随口径演进**：R27 9→10、R31 16→17（r10 缺总数键检查新增一行）；D05 R1 19（SUGG 制品口径）。D-057 行已注「以最新实测为准」。

## V3.4-r12（2026-11-19，收敛循环第 12 轮）

- **r12**：对抗+机械两队。边界外 P0×3——① FREEZE 标题位六型绕过（否定后缀进标题/`CONVERGED-CANDIDATE`/小写/正文 H1：r11 的标题正则不看极性）；② certify 结论行无极性检查（「判定：两项未闭环。」「验证确认了不可能 PASS」「结论：应该通过吗？」被当通过性结论）；③ panel-index.py 未随 r11 同形（`^##\s+` vs `#{2,3}`，`### SUGGESTION` 官方工具与门必然死锁）。P1×2：metrics 单载体升 V3.4 时历史层被制品链追溯（V33_MARKER_RE 硬编码 V3.3，从不解析 V3.4 from R<N>）；SUGG ID 下划线截断。
- **修复（全部肯定式/同形收口）**：FREEZE 改「标题行以终态收尾 + 标题行否定/未定标记扫描」双重判据；certify 结论行加行内极性（未/没/REOPEN/NOT PASS/FAIL/? 的行不算通过性结论）；panel-index/collect/复点三正则真正同形（`#{2,3} SUGGESTION <ASCII-ID>` 含下划线）并补**经过官方生成器**的夹具；V3.4 起点轮改专用标记解析（loop 优先、metrics 兜底）。判定 NOT PASS，修复已落地，收敛待 r13。
- 另核：pattern-gate/command-boundary 存量红披露升级为逐项列明待办（P18 锚点越界 11 项、violation 3 项，引入 10-19/11-03）。

## V3.4-r13（2026-11-19，收敛循环第 13 轮）

- **r13**：对抗+机械两队。边界外 P0×5——① certify「验证确认了不可能 PASS」仍放行（r12 宣称失实，行内极性缺「不」）；② 未定/限定语绕过（暂时通过/PASS 待确认/通过（除 X 外）/待下轮闭环/通过性存疑）；③ FREEZE 极性扫描无全角归一（ＮＯＴ/ＣＡＮＤＩＤＡＴＥ/全角？）；④ 未定词残缺（暂判/拟）+ 双 H1 一否定一肯定时 any() 取一；⑤ schema from 轮零校验（metrics 一行 `from R99` 豁免全部轮）。P1：certify 误伤真实句式（结论：回归通过，未发现 P0——fail-closed 方向，如实记录）；FREEZE 正文 H1 型（改为标题限前三非空行后关闭）。
- **修复**：FREEZE 全角归一进极性扫描 + 标题候选限前三非空行 + 否定即毒化（all 语义）+ 未定词补全（暂/拟/存疑）；certify 行内极性补 不/暂/拟/待/存疑/可能；from 轮封顶校验（起点轮 ≤ 已见最大轮号+1，越界钳回 schema_start——r12 合成夹具 from R5>已见 R2 判为不自洽形态，随判据修正）；判别夹具 +1 组（r13 幽灵宣称勘误：原写 +3，git diff 实为 +1 组+1 处标记值改写——r14 回归员抓出）。
- 判定 NOT PASS（P0×5 已修），收敛顺延 r14。**口径注记**：certify 行内极性 fail-closed 会误伤「未发现 P0」类真实肯定句——如实登记为可用性代价（方向安全），执行者可改写结论行为「结论：通过」体例。

## V3.4-r14（2026-11-19，收敛循环第 14 轮）

- **r14**：对抗+修复回归两队。边界外 P0×3——① **from 封顶锚点可注入**（`schema: V3.4 from R99` 写进 loop 时 schema_start=99：V3 门对 <99 轮休眠、钳回锚点=注入值；r13 P0-5 的根因是标记本身未自校验）；② FREEZE 词表缺口（非/无/曾/将/勿/可能）；③ 毒化逃逸（否定行不带终态 + 肯定行双 H1；H2 否定行不在候选集）。P1×3：判别夹具缺口（r13 五项修复仅 from 封顶满足「删除必红」）、r12 (b) 夹具污染（不可能 PASS 断言空洞的根因）、「判别夹具 +3」幽灵宣称。
- **修复**：schema 标记**自校验**（起点轮 > 已见最大轮号+1 → 钳回 1——注入使整个 V3 机制绑定失效，属事故类）；FREEZE 词表补全；毒化扫描扩到全部含 FREEZE 的 H1/H2 行（否定行无终态也毒化）；certify 行内极性补 非/无/曾/将/勿/除；r12 (b) 夹具隔离化（每断言独立 run）；r13 幽灵宣称勘误。
- 判定 NOT PASS（P0×3 已修），收敛顺延 r15。P2 登记：certify「不」误伤 D03 FREEZE 附录真实句（fail-closed 方向）、封顶轮号形状覆盖、前三非空行对导语式 FREEZE 的误关。

## V3.4-r15（2026-11-19，收敛循环第 15 轮）

- **r15**：对抗+机械两队。边界外 P0×3——① schema 自校验锚点仍可注入（`from R99` + 一行伪造 `## R99` 标题使 max_seen=99，钳回失效且 N0 双源因无后继轮静默）；② FREEZE 正文一行「NOT CONVERGED」解锁（毒化只扫含 FREEZE 的标题行）；③ certify 跨行否定（「结论：通过。」+ 异行「X 未修」）。
- **修复与裁定**：① N0 双源**去掉「须有后继轮」条件**——无账轮次本身即 FAIL（D05 暂停轮如实计入，计数 19→20）；③ certify 全局否定补 未(修|完成|实施|落地)；② **裁定为边界**——正文毒化邻接规则实测误伤合法 FREEZE（02 篇历史段「NOT CONVERGED 保留为历史」），撤销正文毒化、保留标题契约，「伪造含肯定式标题的整个 FREEZE 文档」显式写入 §6B-2 全伪造边界（D-058 同步）。
- 判定 NOT PASS（P0-1/P0-3 已修；P0-2 转边界），收敛顺延 r16。

## V3.4-r16（2026-11-19，收敛循环第 16 轮）

- **r16**：对抗+机械两队。边界外 P0×4——① N0 登记不对称（metrics 块标题非锚定匹配，「标题提及他轮」即视为有账，R1 无账可关）；② FREEZE 词表缺 FAIL/解除/撤/吊销 + 终态左边界（`UN-CONVERGED` 的 \b 匹配）；③ certify 位置脱钩（bullet `- FAIL（REOPEN…）` 行、跨行结论、「复验失败」族不在否定面）；④ 全角 Ｒ 载体对 lifecycle/round-type/N0 隐身（修改轮宣告 CONVERGED 在归档审计 exit 0）。机械队另证：r14 P0-1 判别夹具零判别力（r13 版工具同样红）、r10–r15 六节入账完整但未写提交号、pattern-gate/command-boundary 存量红与披露一致。
- **修复**：三件套读入即全角 ASCII 归一（loop/metrics/ledger + 面板 collect/severities）；metrics_rounds 登记改锚定（块标题开头轮号 + 显式区间，修复过程中两处夹具与正则联动修正：`_metric_blocks` 标题已剥 `##`、PLACEHOLDERS 补半角括号变体）；FREEZE _NEG 补 FAIL/解除/撤/吊销 + 终态 `(?<![A-Za-z-])` 左边界；certify 否定补 bullet-FAIL/REOPEN 行 + (复验|验证|回读|确认)失败族；过期注释勘误。
- 判定 NOT PASS（P0×4 已修），收敛顺延 r17。

## V3.4-r17（2026-11-19，收敛循环第 17 轮）

- **r17**：对抗+机械两队。边界外 P0×2——① FREEZE 终态左边界 `(?<![A-Za-z-])` 挡不住 Unicode 连字符族（– — ‐ ‑ ‒）与数字/下划线前缀（0CONVERGED/_CONVERGED，IME 诚实误打在威胁模型内）；② certify 否定面只认 bullet（编号列表 `1. FAIL`/纯段落 `FAIL：`/`> FAIL`/`### FAIL` 配一行肯定结论放行——r15/r16 同链残留）。P1：数学字母 𝐑𝟏 对 N0 隐身（同源缺口，登记）。夹具判别力修复：r16 P0-3 夹具文本改用 r16 新增形态专属触发（原文本含 r15 旧分支触发词「未落地」，删新行仍绿——r17 机械队双重反转实证）；certify 检查顺序修正（否定结论先于长度判定，短而明确的否定回执不再误报「过短」）。
- **修复**：FREEZE 归一扩 Unicode 连字符族（–—‐‑‒−→-）与数学粗体大写/数字（→ASCII）；终态左边界扩 `(?<![A-Za-z0-9_-])`；certify 否定补 编号/引用/标题行与 `FAIL[:：]`；检查顺序调整。判定 NOT PASS，收敛顺延 r18。
- **提交号索引（r10–r16 账本补记）**：r10=a862e5f60+98b95cb05、r11=94b50aed6、r12=8262b34d4、r13=aead42815、r14=73e5b7001、r15=23ae8f8e5、r16=fbe3000fe。

## V3.4-r18（2026-11-19，收敛循环第 18 轮）

- **r18**：对抗+机械两队。边界外 P0×3——① FREEZE 零宽族（ZWSP/ZWNJ/ZWJ/BOM）使极性扫描失明；② certify 否定面缺 dash 后缀/行中 REOPEN/全角否定——15 形态放行；③ from R2（=max_seen+1）一行注入 + discovery 轮型 + 无制品面板 = 整门 exit 0。
- **修复**：FREEZE 零宽剥离；certify 否定词表扩 dash 后缀/全角归一/**任意 REOPEN**（fail-closed，执行者可改写体例）；from 轮封顶收紧 max_seen+1 → max_seen（V3.4 标记声明历史事实不是未来计划；r12 合成夹具 from R3>已见 R2 随判据修正为 from R2）。判定 NOT PASS，收敛顺延 r19（第 10 轮，协议最后一轮）。
- **机械队事故披露**：审计中误执行 checkout 覆写 462 路径，已完整恢复并重跑全部污染读数，现树净（3 untracked）；作废读数已在报告中标注。

## V3.4-r19（2026-11-19，收敛循环第 19 轮——协议第 10 轮）

- **r19**：对抗红队。边界外 P0×4——① 幽灵轮头（只有标题无台账无 metrics）抬 max_seen 使 from 合法化，R1 逃逸全制品链；② FREEZE Unicode 残余 7 区段（U+00AD/180E/2060-2064/FE0F/1D173-17A/E0000-E007F tag）令「N?OT CONVERGED」全解锁；③ certify 零宽盲区 + 裸 P0 遗留组合逃逸；④ schema_start +1 余量残留。
- **修复**：① max_seen 只计有账轮次（台账或 metrics 标题头交集）；② FREEZE 不可见字符剥离扩 7 区段；③ certify 全局否定补 `^\s*[-*]?\s*P[01]\b` + 零宽剥离；④ schema_start 钳回阈值 max_seen+1 → max_seen。
- 判定 NOT PASS（P0×4 已修），协议第 10 轮结束。**用户指示补 8 轮（r20–r27）**。

## V3.4-r26（2026-11-19，收敛循环第 26 轮——结构性修复）

- **r26**：对抗+机械两队。边界外 P0×2——① strip_invisible 第 4 轮换码点逃逸（U+115A/115B/E0300/31EF/E000）；② 裸 P0 CJK 左邻（遗留P0/遗留P00/𝐏𝟎/全角Ｐ０）。另有 process-violation：r20–r25 六轮落账虚假（WORKFLOW-VERSIONS 零条目）、r19–r25 五轮夹具判别力存疑。
- **结构性修复**：_strip_invisible 改白名单过滤器——只保留 ASCII 可打印 + CJK Unified/Extension A + CJK Symbols/Punctuation + Fullwidth Forms + newline/tab。FREEZE 与 certify 是项目内部工件，任何不在白名单内的字符一律剥离。**关闭 r9–r26 六轮换码点对抗的整类攻击面**（不是又一个补丁，是类级消除）。裸 P0 CJK 左邻修复（\b 在 CJK 旁失效改 (?<![a-zA-Z0-9])）。
- 判定待 r27 确认（最后一轮）。

## V3.4-r26（2026-11-19，收敛循环第 26 轮——结构性修复）

- **r26**：对抗+机械两队。边界外 P0×2——① strip_invisible 第 4 轮换码点逃逸（U+115A/115B/E0300/31EF/E000）；② 裸 P0 CJK 左邻（遗留P0/遗留P00/𝐏𝟎/全角Ｐ０）。
- **结构性修复**：_strip_invisible 改白名单过滤器——只保留 ASCII 可打印 + CJK Unified/Extension A + CJK Symbols/Punctuation + Fullwidth Forms + newline/tab。关闭 r9–r26 六轮换码点对抗的整类攻击面（类级消除）。裸 P0 CJK 左邻修复（\b → (?<![a-zA-Z0-9])）。
- 判定待 r27 确认。

## V3.5（2026-11-18，用户裁决：制品链换证据类型 + 威胁模型写死）

> 依据：r6–r8 三轮对抗审查每轮在门层发现新 P0（R31 断链 → 自报键伪造 → 空目录
> 伪造），证明「文本匹配 + 自报 + 未入库证据」有结构性上限。用户在 r8 后质询
> 「持续 P0 是否说明未收敛」并要求按建议实施。V3.5 不再加词表，改证据类型。

| 编号 | 规则 | 判例 | 检测器 | 状态 |
|---|---|---|---|---|
| V3.5-a | 面板制品入库（适用面：V3.4 起新篇目 + **V3.3+ 未 FREEZE**；FREEZE 豁免需正向状态行，r10）：`runs/<stage>/<doc>/panel/round<N>/*.md` + `INDEX.md`（sha256 索引）；tmp 只作工作副本 | r8「空目录伪造」「克隆不可复核」 | 轨迹门 `panel-artifact`（轮关闭：制品存在 + 哈希/计数一致 + 无未入索引文件 + 非空制品） | 已生效 |
| V3.5-b | certify 独立回执入库：`runs/<stage>/<doc>/certify/round<N>-*.md` | r5/r8 回执只在 tmp | 轨迹门 `certify-artifact`（闭合态轮缺失即 FAIL） | 已生效 |
| V3.5-c | 威胁模型写死：门防事故与诚实犯错，不防全伪造制品链；全伪造不再列为待修 P0 | r6–r8 三簇 P0 的收窄轨迹 | `review-evals.md` §6B-2 文本 | 已生效 |
| V3.5-d | 门自身收敛判据：连续两轮独立对抗审查新增 P0/P1=0（边界类除外）+ 每个历史 P0 有判别夹具；r9 为首个考场 | r7/r8 回归轮判例 | §6B-2 + 各轮审查记录 | 已生效 |

### V3.5 减法账本

| 版本 | 新增 | 替代 / 合并 | 净效果 |
|---|---|---|---|
| V3.5 | `tools/panel-index.py`（生成+校验+自测） | 替代「面板自查清单」；合并进既有轨迹门（不新增门步骤） | 净化：独立工具 + 门内复用 |
| V3.5 | 轨迹门 `panel-artifact` / `certify-artifact` | 替代 r6–r8 的「文本词表加固」路线 | 换证据类型：伪造成本从「一行字」升到「完整哈希自洽制品链」 |
| V3.5 | 威胁模型 + 门收敛判据（§6B-2） | 替代「无限逼近零伪造」的不收敛目标 | 净减不可达目标：宣告边界，判据可判定 |

**同步门读数（2026-11-18 实跑）**：四道规则同步门 + 六自测全绿；`panel-index.py
--self-test` PASS（生成/校验/篡改检测/新增检测/no-finding 形态）；轨迹门新增 4 组
制品链夹具（缺失/合法链/certify 缺失/回执补齐）。矩阵 16 格维持 10 PASS / 6 存量
或授权 FAIL（01–04 历史轮不追溯）。05 的 V1 旧面板已生成 INDEX（tmp 内试跑，不入
库）；05 R1 启动时其面板与回执按本条入库。

**r9 考场记录（2026-11-18，§6B-2 判据第一轮）**：两支独立队（攻制品链 / 机械核验）
判定 **r9 不通过**——边界外 P1×5：① 空制品（0 字节面板+回执+空台账+全零 metrics）
整轮 PASS；② certify 只查文件名存在（0 字节/NOT PASS/REOPEN/SUGGESTION-only 均放行）；
③ 0 字节或自相矛盾的 FREEZE 永久豁免制品要求；④ `--panel-dir` 解耦（门校验 run 目录
空壳、coverage 读外部真实面板）；⑤ INDEX 计数与 metrics `发现总数` 从未对账。另有
边界内 I1（载体分脑：loop 无 schema、metrics 有 → 新门整体静默）、I2（v34 标记未按
起点轮作用域）、I3（strict/空目录夹具无判别力）、I4（`--check` 与门计数口径分叉）、
版本表合并行（已修）。**修复**：制品块加组合 schema 作用域、FREEZE 内容豁免门槛、
空制品拒绝、INDEX 计数↔文件↔metrics 三边对账、`--panel-dir` 禁外部目录、certify
回执非空+否定结论拦截、指标不变式（发现总数 ≥ 新准入/唯一根因）、6 组新判别夹具
（含单级标题完整块、空目录、计数漂移、缺 INDEX、否定回执、空 FREEZE）；版本表拆分；
`review-evals.md:565` 的「tmp 不入库」改为「工作副本」；rb.md 与 certify 路径补约定。
修复后自测全绿、矩阵 10/6 维持；05 R1 现 18 FAIL（含 panel-artifact 三项：制品缺失/空面板/外部面板），随 D-057 同步。**按 §6B-2 判据，r9 不通过，需连续第二轮（r10）
零新增 P0/P1 方可判收敛**；r10 未跑前，状态为「修复已落地，收敛待第二轮确认」。
