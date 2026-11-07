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
