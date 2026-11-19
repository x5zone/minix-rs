# D05 循环轨迹 —— 05-clock-interrupt-init

## §0 基线

- 工作流版本：V2（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- 基线提交：`c0d74632e`
- 文档：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
- 文档基线 sha256：`f6d4b6440aa378175cb2e761cd0e59437b1c8765b85f47adae099e3bec14cd52`
- 基线行数：2196
- 工作树：除 `prompt/resources/` 未跟踪外干净
- 语料条目：D05（`CORPUS.md` 第二节）；盲区重点 B-01、B-02、B-03、B-04、B-08、B-13
- 设计快照：`05-outline.v4.md`、`05-outline-review.v4.md`、`05-design.v3.md`；`design-coverage-check.sh 01-stage-kernel` 全 PASS
- 关联代码：32 个文件、18,509 行，`tools/doc-code-map.sh --check` 32/32 通过；清单见 `related-code.txt`
- 本篇形态：时钟与中断初始化 / 8259-PIT / 三架构；高风险，按 `review-panel.md` 第七节启用全角色
- 范围声明：本篇范围 = 文档主题（软件时钟状态、中断控制器、架构杂项与持久 tick 源）+ `related-code.txt` 的 32 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。

## §1 面板

- 首轮批量派发 5 个角色时，平台以 `inactive for 600000ms` 回收了 P-FACT、P-DESIGN D1、P-READER A/B、P-REFUTE；随后改为逐个派发并加“边查边写盘”要求，全部补齐。读者 A/B 合并为一个两段式 agent（`panel/reader-ab.md`），独立性弱于分角色，但材料隔离相同。
- P-FACT：文档、32 个关联 Rust 文件、19 个 C 源、trait 读数、冻结决策；输出 `panel/fact.md`，18 条发现。
- P-DESIGN D1：C 语义、文档 Ch1/Ch2、项目约束；禁止 Rust/Ch3/Ch4/`.design/`/读者画像。输出 `panel/design-d1.md`，16 条 DECISION。**部分污染**：用 `grep '^#'` 定位章节时带出了 Ch3 以后的小节标题（无正文），已在报告开头登记；D1 的“独立设计”不成立。
- P-DESIGN D2：D1 报告、文档 Ch3/Ch4、32 个 Rust 文件、trait 读数；输出 `panel/design-d2.md`，18 条发现。
- P-READER A/B：只读文档与读者画像；输出 `panel/reader-ab.md`，A 12 + B 14。
- P-READER C：输出 `panel/reader-c.md`，21 条。
- P-REFUTE：文档、C 源、关联 Rust、设计快照；输出 `panel/refute.md`，5 条主张；CLAIM 04/05 是 P1。
- 无发现面板：`design-d1.md`（16 条 DECISION，只作参考）；`refute.md`（5 条 CLAIM，反证结果见 `defect-ledger.md` 的 `## P-REFUTE 附加` 表）。
- 缩编：读者 A/B 合并。工作树完整性：面板前后只有 `prompt/resources/` 与新增 `runs/`。

## §2 发现

- 面板原始输出：`tmp/review-eval/01-stage-kernel/05-clock-interrupt-init/panel/`。
- 总计 83 条 `## FINDING`：FACT 18、DESIGN-D2 18、READER-A 12、READER-B 14、READER-C 21；P-REFUTE 另有 5 条主张。
- 最高严重度：
  - P0（设计轴代码问题）：DESIGN-01 `set_tsc_per_ms` 无调用者、TSC 校准丢失；DESIGN-02 riscv64 `IrqVector` 截断；DESIGN-03 `irq_manager` 11 个 `cfg(target_arch)` 块；DESIGN-04 LAPIC 基址 assert。
  - P1：FACT-07/09/10/15（引文/命令），REFUTE CLAIM 04（APIC 形态 cstart 内 PIT 校准窗口）、CLAIM 05（mask_all 实际 16 条；LAPIC 拒绝重定位）。
- 完整清单与逐条处置见 `defect-ledger.md`；未修项见 `follow-ups.md`。

## §3 归因

- 处置分布：① 11 条、② 72 条、③ 0 条、④ 0 条、⑤ 0 条。
- 冻结决策检查：0 条 `re-litigation`；D-001 四章骨架没有被重新提出。
- 范围门：0 条 adjacent 被当阻塞；所有 ② 项只进 `follow-ups.md`。
- ① 只有 6 条，原因是本轮把设计轴 P0 与引文块 P1 全部转出；这不是“无缺陷”，是代码改动/引文回炉的边界选择。

## §4 修复

### 文档

- FACT-01/02：`irq_manager.rs` 测试 9→10；`x86_64/interrupt.rs` 4→5。
- FACT-05：§4.4 改为“LAPIC 基址不一致时 `assert_eq!` 拒绝启动；旧说法‘以 MSR 为准’不成立”。
- FACT-06：§4.9 改为“模块注释已写输入 2；测试名仍叫 input_zero”。
- FACT-11：§3.2 的交叉引用改为“见 §3.2 的 hz 守卫说明”。
- FACT-17：§4.6 改为“Rust 多 P 位（0x7 vs C 的 C+E=0x5）”。
- REFUTE CLAIM 04：§2.6 标题与正文加“持久 tick 源不在 cstart；APIC/no_apic=0 单 CPU 构建在 cstart 内有一段 PIT 校准窗口（启动 PIT、开中断、采若干 tick、随后停/摘/关）”；篇头主题、教学目标 ⑤、开篇句同步。
- REFUTE CLAIM 05：§4.4 的 `mask_all` 表改为 `min(nr_irq_vectors, 16)`；LAPIC 段与 FACT-05 同批改。

### 代码

- 本轮未改代码；4 条 P0 设计问题与 14 条其余设计发现全部转出（见 `follow-ups.md`），D05 标 design-debt。

### 未修

- FACT 其余 12 条、DESIGN 18 条、READER A/B/C 47 条：见 `follow-ups.md`。

## §5 复验

- 待跑：anchor、doc-code-map、doc-ref-check、doc-code-density、doc-style-lint、eval-trace-check、cargo check。
- QEMU 真机回归：未跑；CLAM 04 的 PIT 校准窗口与 DESIGN-01/02/03 都需要 QEMU 或专门载体。

## §6 指标

- 见 `metrics.md`：发现 83、① 11、② 72、③ 0、④ 0、⑤ 0、`re-litigation` 0、`adjacent 阻塞` 0、重构提出 18/接受 0/实施 0/登记 18。
- 成本：6 个 agent（读者 A/B 合并），面板约 3 小时（含批量 inactive 重试）；修复约 30 分钟；token 未采集。

## §7 状态

**NOT CONVERGED（design-debt + teaching-debt + test-debt pending）**

- 设计债：DESIGN-01..18 共 18 条转出；其中 DESIGN-01/02/03 是 P0，需优先排队。
- 教学债：READER A/B/C 的 47 条转出项。
- 测试债：`cargo test -p minix-arch` 测试模块编译失败（D05-F14，沿用 D01-F01）。
- 已修 11 条；修复率 13.3%，是五篇里最低的；未达到 `CONVERGED` 的原因不是“还有小问题”，而是设计 P0 与引文回炉都尚未关闭。
- meta-review 总体 `FLAT`，运行质量 `NOT PASS`（抽样 22/83 = 26.5%，严格一致性 72.7%）；FACT-01 首修只改一处、CLAIM 04 不完整、台账 DESIGN 编号错位、4 条重复项未 `dup-of`，均已按复核补修。

## §8 给 meta-review 的不确定点

1. 修复率 7.2% 是否说明本轮范围切得太窄；DESIGN-01（TSC 校准丢失）、DESIGN-02（IrqVector 截断）是 P0 代码问题，却被转出。
2. 首轮批量派发被平台回收，读者 A/B 合并；这是平台限制还是流程问题，是否应写入工作流的成本/可执行性约束。
3. D1 的 `grep '^#'` 带出 Ch3 标题，是否足以作废其 16 条 DECISION；D2 已做逐条对照。
4. REFUTE CLAIM 04 的 PIT 校准窗口补写是否改变了原文的教学主线；它现在把“定时器不在 cstart”限定为“持久 tick 源”。
5. `cargo test -p minix-arch` 的 45 个错误使 §5 的测试计数只有静态意义；是否需要把它升级为阻断项。

## V1 历史轮（原第一轮完整 survey；V1 口径，不参与 V3 作用域）

### R1 §0 基线

- 轮型: full
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`da04c7486`
- 文档：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
- 文档 sha256：`f149e932cf7defe1e052794c2673409abba13dfdbd67c819b9341f067b9c533b`
- 行数：2198
- 设计快照：05 组齐备；`design-coverage-check.sh` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 32/32；清单见 `related-code.txt`
- 范围声明：本篇范围 = 时钟与中断初始化（三架构） + 32 个关联文件 + 本轮修复触碰的文件。
- 历史说明：旧 §0–§8 为 V1 记录，保留不追溯。

## R1 完整修改轮（V3.4 四态模型；D-075 授权）

### R1 §0 基线

- 轮型: full
- edition: V3.4
- schema: V3.4 from R1（V3.5 制品链起约束；旧 V1 轮的历史段保留在文件前部，不继承）
- 基线提交：`cd76b73d1`（D04 收尾提交；面板启动时工作树除本篇待提交项外干净）
- 文档：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
- 文档基线 sha256：`f149e932cf7defe1e052794c2673409abba13dfdbd67c819b9341f067b9c533b`（面板启动时点）
- 行数：2198（修复后 2207）
- 设计快照：05 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 32/32（修复后 33/33——Phase A.2 注记新增一处 lib.rs 引用）
- 语料条目：D05（`CORPUS.md` 第二节）
- 范围声明：软件时钟状态、中断控制器、架构杂项与持久 tick 源 + 32 个关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块与下一批次结构性重构
- 重开依据：D-075
- **旧轮处置（L0.8）**：旧 V1 轮（83 条、72 条 ② 无批准日期）不继承——本轮的全新 survey 对当前文档从零独立取证，旧结论既不带入也不丢弃，其历史段与 follow-ups 表保留在文件前部与 `follow-ups.md` 供追溯。

### R1 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 2198 行 ≥ 500）：
  - P-FACT：11 条 FINDING + 3 SUGGESTION（`fact.md`）——定位了 citation 门报的 3 条 failed 引文漂移（全部是文档侧漂移、代码未改），另在 checker 盲区外抓到 8 处
  - P-DESIGN-D2：6 条 FINDING + 1 ARCH-PROPOSAL（`design-d2.md`）；D1 停跑（D-024），基线「文档主张 ↔ 源码事实」，记 VARIANCE
  - P-READER-A：4 + 8（`reader-a.md`）；P-READER-B：2 + 14（`reader-b.md`）；P-READER-C：2 + 14（`reader-c.md`）
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 2 / 反证失败 4），`refute.md`
- **VARIANCE 1（派发与重试）**：并发上限 3；第一波 fact/reader-a/design-d2，第二波 reader-b/reader-c/refute——其中 reader-b 与 refute 首跑 600 秒无输出被平台回收（均未落盘），按 L1.1 重试协议以 SendMessage 续跑并要求边查边写盘，双双补齐。
- **VARIANCE 2（certify 任务书笔误）**：R1 的 certify 任务书把 D04 的两条修复（init_from_kinfo 失败点位、约束放大）误列为 D05 条目；certify agent 实测后在回执中指认，主 agent 按 D05 真实清单（位置三处矛盾）复核——回执的条目 9 以更正后的口径判定。
- 制品入库：`panel/round1/` + `INDEX.md`（6 文件：FINDING 25 / SUGGESTION 39 / CLAIM 6）。
- certify 回执：`certify/round1-certify.md`（三轮复核，终判以回执末行显式结论为准）。

### R1 §2 发现

- 70 条机器可计：FINDING 25（fact 11、design-d2 6、reader-a 4、reader-b 2、reader-c 2）+ SUGGESTION 39 + CLAIM 6。
- 严重度：P0 = 0；P1 = 1（READER-A-03：`boot_init_timer` 相对进程表初始化的位置三处说法互相打架，教学目标⑤不可判定）；P2 = 20；P3 = 49。
- 面板要点：citation 门预检的 3 条 failed 全部定位（use 顺序×2、`&result.kernel_info` 虚构——全是文档侧漂移）；fact 另在 checker 盲区外抓到 8 处（含 §3.6 的 Phase A.2 陈旧陈述）；设计侧画像仍是「契约层与实现层缓慢漂移」；refute 实测 §5.1 的 15 个计数 15/15 逐格吻合。

### R1 §3 归因

- 29 个根因（RD-R1-01..29）。
- 冻结决策检查：无命中；6 条设计项按 D-047 边界当轮 ⑥ 实施（[MODERN]×2、[DEL]×4）；ARCH 提案 RD05-A1 进判决账待裁；CLAIM 11..15 反证失败记 FP、CLAIM-16 非阻断已修记 ①。
- 范围：全部 in-scope。
- **自引入 0**：本批没有新增「文档半边修了、代码半边没修」类问题。

### R1 §4 修复

- **文档侧**（13 处修改点）：3 条 citation failed 引文逐字修复（use 顺序×2、`&result.kernel_info`）、§4.7/§4.9/§4.8 三块注释转述标注、`apic.c:139→arch_clock.c:139`（宏行 140）、§3.6 Phase A.2 历史注记、`S-8 stub path`→`trap stub path`、`init_cpu_interface`→`init_cpu_interface_current`、riscv 载体行锚 360-364、ClockArch 示意默认体注、§4.1 两条 assert_eq 补省略、§1.4 三处位置矛盾统一到调用链依据、§2.1 内存图读值句、§2.4「第四步」、§4.7 推演 1 的前提差异、§3.2 推演的记账路径点名（misc.rs:1065）、§3.8 K-3 行锚点改指 arch 定义、L740 代价格改 re-export 口径。
- **代码侧**（4 项 ⑥）：`disable_timer_irq` 从 trait 与三份实现删除（全仓零调用者）；`os/kernel/src/clock.rs` 的 DEFAULT_HZ 改 `pub use minix_arch::arch::clock::DEFAULT_HZ`；arm64/riscv64 重挂回退前加 `debug_assert!`（release 行为不变）；`os/arch/src/arch/clock.rs` 的过期注释（CLINT mtimecmp/MMIO）改 SBI/time-CSR 口径 + 映射表 x86 行修正。
- 文档六门：anchor 106/106、citation `checked=43 failed=0`、structure/ref/style(error=0)/density 通过、doc-code-map 33/33。
- 宿主：`cargo check -p minix-arch` 绿；`cargo test -p minix-kernel --features mock --lib` 866 passed / 0 failed / 8 ignored。

### R1 §5 复验

- 独立回执 `certify/round1-certify.md`：三轮复核。首轮 15 条判 12 CLOSED / 3 STILL-BROKEN（其中 2 条系任务书笔误）；第二轮按证据改正 5 处残留（L1049 残句、s14n 移除、apic.c:140、两个锚点、映射表 x86 行）后判 14/1；第三轮（本轮收口）改最后两处（L740 re-export 口径、§4.7 use 顺序按源码）。
- **回执抓到主 agent 的实质问题**：① L1049 编辑残留断句；② 「374/95」式口径串位的变体——L740 的「要同步」与新 re-export 口径矛盾；③ §4.7 引文 use 顺序声明为真但引文本身未排；④ 我在任务书里混入了 D04 的条目（流程缺陷：certify 任务书是手写的，清单应从台账行生成）。
- `certify 新准入 = 0` 口径：certify 输入闭集 = 13 处文档修改点 + 4 项代码实施；回执覆盖全部，复核后无新增修改点。
- 代码行为：`cargo check -p minix-arch` 绿；kernel lib 测试 866/0；QEMU 未跑（clock/timer_irq_gate 的改动不触三架构真机门面——x86 侧只删了零调用方法与改注释，aarch64/riscv64 侧 debug_assert 在 release 下不生成代码）。

### R1 §6 指标

- 与 `metrics.md` 的 `## R1 完整修改轮` 块一致：发现总数 70、唯一根因数 29、本轮新准入 29、certify 新准入 0、处置 ① 15 / ⑥ 6 / ④ 7 / FP 5 / SUGG 37、未闭环 0。
- 轮型 `full`；轮结果 `ROUND-CLOSED`（修改轮无收敛声明权，进 CONVERGENCE-CANDIDATE）。

### R1 §7 状态

- `ROUND-CLOSED` → `CONVERGENCE-CANDIDATE`。收敛声明权在确认轮（R2，见下一节）。
- 未闭环 0；无 OPEN-DEBT 行（本轮 ⑥ 全部当轮实施，无跨 crate 延期项）。

### R1 §8 给 meta-review 的不确定点

1. 旧 V1 轮的 83 条（含 72 条 ②）按 L0.8 由全新 survey 取代——没有逐条对照旧清单，靠「新 survey 从零取证」覆盖。若 meta 认为旧清单里有新 survey 没接住的项，需要一轮对照。
2. certify 任务书笔误混入 D04 条目（VARIANCE 2）：任务书是手写的，应从台账行机械生成——这是流程缺陷，已记 BLIND-SPOTS 候选。
3. `disable_timer_irq` 删除触及 trait 形状（x86 的空实现是「PIT 没有模块本地门」论述的一部分）；§3.7 的表已同步，但若 meta 认为该 trait 改动应触发 QEMU 门（timer_irq_gate 在真机路径上），本轮的「QEMU 未跑」判定需要复核。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量记录：D05 是六篇中唯一从未收敛的，本轮从零 survey 抓到的密度（25 FINDING / 70 条，P1 1 + P2 20）显著高于已收敛的四篇——符合「未收敛文档问题密度高」的预期。certify 三轮才收口（首轮 12/3），其中两条是任务书笔误、两条是我改引文时引入的顺序漂移——「certify 必须独立」在本轮再次生效。
- 流程质量结论：`IMPROVED`（从「只有 V1 遗留」到「完整修改轮收口 + 确认轮候选」）。

### L6.5

- 三问：① 不变量是「修改轮的 ⑥ 实施必须带 cargo check/test 与文档同步」；② 前提是 D-047 边界（[MODERN]/[DEL] 当轮实施）；③ 证据是四项代码实施 + 866/0 测试 + 文档同步段。
- 资源重读：盲区属「流程类」（certify 任务书手写），查 `eai-agent-toolkit`；改进方向「certify 清单从台账机械生成」记 BLIND-SPOTS 候选。

## R2 确认轮（V3.4 四态模型；收敛声明轮）

### R2 §0 基线

- 轮型: confirmation
- edition: V3.4
- schema: V3.4 from R1
- 基线提交：R1 收口后的工作树（含 ⑥ 实施）
- 文档：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
- 文档基线 sha256：`f7d87635…`（R1 修复后）；R2 修复后 `见 FREEZE`
- 行数：2198→2207→2209
- 设计快照：05 组齐备
- 关联代码：`doc-code-map --check` 33/33
- 语料条目：D05
- 范围声明：同 R1
- 确认依据：R1 ROUND-CLOSED（§6A 四态模型）

### R2 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立：
  - P-FACT：5 条 FINDING + 3 SUGGESTION——**抓到 R1 实施留下的同步债**（arch 侧 DEFAULT_HZ 注释未随 re-export 同步，P1）
  - P-DESIGN-D2：9 条 FINDING（P1 1、P2 5、P3 3），0 ARCH——同一 P1 的代码侧视角 + 8 处注释/标注尾巴
  - P-READER-A：3 + 10；P-READER-B：3 + 15；P-READER-C：1 + 14
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 3 / 反证失败 3）；实跑宿主测试 47=46 跑+1 忽略、全套 866/8
- 制品入库：`panel/round2/` + `INDEX.md`（6 文件：FINDING 21 / SUGGESTION 42 / CLAIM 6）。
- certify 回执：`certify/round2-certify.md`（8/8 CLOSED，首轮即 PASS）。

### R2 §2 发现

- 69 条机器可计。严重度：P0 = 0；P1 = 2（同一根因的文档/代码两半：arch DEFAULT_HZ 注释与 re-export 不一致）；P2 = 19；P3 = 48。
- **自引入 2 条**（RD05R2-FACT-01 与 RD05R2-DESIGN-D2-01 同根，origin R1）：R1 实施 D2-05 时只改了 kernel 侧与文档引文，arch 侧源注释没同步——被确认轮抓到，正是 §4.0c「自引入记引入轮」规则的活例。

### R2 §3 归因

- 28 个根因（RD-R2-01..28）。
- 冻结决策检查：无命中；同步项（含两处代码注释同步）按 §3.6「事实与同步项」本轮修；设计 P3 记 SUGG；CLAIM 21/22/26 非阻断已修记 ①、23/24/25 反证失败记 FP。
- 范围：全部 in-scope。

### R2 §4 修复

- 10 处修改点：arch/clock.rs 注释改 re-export 口径（P1 根）、文档 §3.2 引文块同步、§3.8 K-3 锚点、L782 尾段（幽灵注释句退役 + with_hz(60) 口径改写）、§1.4 幽灵引文改退役声明、§3.1 Send+Sync 释义（PIT 单系统设备）、§3.7 推演 2 形态限定、§2.1 内存图读值句、§3.2 反事实记账路径点名、三个引文标注（SUG-01/02/03）。
- 文档六门：anchor 106/106、citation `checked=43 failed=0`、structure/ref/style(error=0)/density 通过、doc-code-map 33/33。

### R2 §5 复验

- 独立回执 `certify/round2-certify.md`：8 条全部 CLOSED，首轮即 PASS；含 `diff` 为空的引文逐字核对（QUOTE-IDENTICAL）与 re-export 实证（clock.rs:710）。
- `certify 新准入 = 0`；未闭环 0。
- 代码行为：arch/clock.rs 只改注释；QEMU 未跑。

### R2 §6 指标

- 与 `metrics.md` 的 `## R2 确认轮` 块一致：发现总数 69、唯一根因数 28、本轮新准入 28、certify 新准入 0、处置 ① 17 / ④ 6 / FP 3 / SUGG 43、未闭环 0。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R2 §7 状态

- `CONVERGED`。前提：R1 ROUND-CLOSED；R2 certify 新准入 0；OPEN-DEBT 无未销账行。
- `CONVERGED` 的语义限制同前四篇。

### R2 §8 给 meta-review 的不确定点

1. 确认轮抓到的 P1 是上一轮 ⑥ 实施的同步债——修 bug 的动作本身制造新债，这正是 §4.0d/§6A 设计确认轮的动机；但「确认轮不实施 ⑥」的边界下，两处代码注释同步算「同步项」还是「⑥」，规则没有明文，本轮按同步项处理。
2. 设计 P3 三条（D2-07/08/09）含一处「[ARCH] 标注缺失」，按门槛只能记 SUGG；标注补齐留下一批。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量结论：`IMPROVED`——确认轮的独立面板抓到修改轮实施的同步债（arch 注释），证明「确认轮全量 survey」不是形式；四篇确认轮里 certify 首轮全过的第二篇。
- BLIND-SPOT-DELTA：新增候选「⑥ 实施的代码注释同步债」与「certify 任务书应从台账机械生成」（R1 §8 条目 2）。

### L6.5

- 三问：① 不变量是「确认轮在最终工作树上 certify 新准入 = 0」；② 前提是 R1 ROUND-CLOSED；③ 证据是 certify 回执 8/0 与六门读数。
- 资源重读：查 `fresh-eyes-review`；无新检测模式，并入既有候选。
