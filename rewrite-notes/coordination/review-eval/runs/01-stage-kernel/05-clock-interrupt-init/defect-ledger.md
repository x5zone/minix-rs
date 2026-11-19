# D05 缺陷与处置台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-01 | design-d2 | P2 | 设计/抽象面 | ② | 转出：follow-ups.md D05-F09（ClockArch 12 方法混四种职责） | panel/design-d2.md DESIGN-01 | 不适用（转出） |
| DESIGN-02 | design-d2 | P2 | 设计/角色 | ② | 转出：follow-ups.md D05-F09（BSP/AP 只由 bool 承载） | panel/design-d2.md DESIGN-02 | 不适用（转出） |
| DESIGN-03 | design-d2 | P2 | 设计/频率源 | ② | 转出：follow-ups.md D05-F09（频率两个独立来源） | panel/design-d2.md DESIGN-03 | 不适用（转出） |
| DESIGN-04 | design-d2 | P1 | 设计/LAPIC 契约 | ① | 文档半边已修（同 FACT-05）；代码行为转 D05-F08 | panel/design-d2.md DESIGN-04 | L4 文档五门重跑 |
| DESIGN-05 | design-d2 | P0 | 设计/TSC 校准 | ② | 转出：follow-ups.md D05-F05（set_tsc_per_ms 无调用者、TSC_PER_MS 恒 0） | panel/design-d2.md DESIGN-05 | 不适用（转出） |
| DESIGN-06 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-06） | panel/design-d2.md DESIGN-06 | 不适用（转出） |
| DESIGN-07 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-07） | panel/design-d2.md DESIGN-07 | 不适用（转出） |
| DESIGN-08 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-08） | panel/design-d2.md DESIGN-08 | 不适用（转出） |
| DESIGN-09 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-09） | panel/design-d2.md DESIGN-09 | 不适用（转出） |
| DESIGN-10 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-10） | panel/design-d2.md DESIGN-10 | 不适用（转出） |
| DESIGN-11 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-11） | panel/design-d2.md DESIGN-11 | 不适用（转出） |
| DESIGN-12 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-12） | panel/design-d2.md DESIGN-12 | 不适用（转出） |
| DESIGN-13 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-13） | panel/design-d2.md DESIGN-13 | 不适用（转出） |
| DESIGN-14 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-14） | panel/design-d2.md DESIGN-14 | 不适用（转出） |
| DESIGN-15 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-15） | panel/design-d2.md DESIGN-15 | 不适用（转出） |
| DESIGN-16 | design-d2 | P2 | 设计/时钟与中断 | ② | 转出：follow-ups.md D05-F05..F10（DESIGN-16） | panel/design-d2.md DESIGN-16 | 不适用（转出） |
| DESIGN-17 | design-d2 | P0 | 设计/身份映射 | ② | 转出：follow-ups.md D05-F06（riscv64 IrqVector 截断） | panel/design-d2.md DESIGN-17 | 不适用（转出） |
| DESIGN-18 | design-d2 | P0 | 设计/架构泄漏 | ② | 转出：follow-ups.md D05-F07（irq_manager 11 个 cfg(target_arch)） | panel/design-d2.md DESIGN-18 | 不适用（转出） |
| FACT-01 | fact | P1 | 文档事实 | ① | 已修：§5.3 M4 的 9→10 | panel/fact.md FACT-01 | L4 文档五门重跑 |
| FACT-02 | fact | P1 | 文档事实 | ① | 已修：§5.3 M5 的 4→5 | panel/fact.md FACT-02 | L4 文档五门重跑 |
| FACT-03 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-03：引文/注释/命令/测试计数） | panel/fact.md FACT-03 | 不适用（转出） |
| FACT-04 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-04：引文/注释/命令/测试计数） | panel/fact.md FACT-04 | 不适用（转出） |
| FACT-05 | fact | P1 | 文档事实 | ① | 已修：§4.4 改为 assert_eq! 拒绝重定位，旧说法“以 MSR 为准”不成立 | panel/fact.md FACT-05 | L4 文档五门重跑 |
| FACT-06 | fact | P1 | 文档事实 | ① | 已修：§4.9 改为“模块注释已写输入 2；测试名仍叫 input_zero” | panel/fact.md FACT-06 | L4 文档五门重跑 |
| FACT-07 | fact | P1 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-07：引文/注释/命令/测试计数） | panel/fact.md FACT-07 | 不适用（转出） |
| FACT-08 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-08：引文/注释/命令/测试计数） | panel/fact.md FACT-08 | 不适用（转出） |
| FACT-09 | fact | P1 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-09：引文/注释/命令/测试计数） | panel/fact.md FACT-09 | 不适用（转出） |
| FACT-10 | fact | P1 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-10：引文/注释/命令/测试计数） | panel/fact.md FACT-10 | 不适用（转出） |
| FACT-11 | fact | P1 | 文档事实 | ① | 已修：§3.2 的交叉引用改为“见 §3.2 的 hz 守卫说明” | panel/fact.md FACT-11 | L4 文档五门重跑 |
| FACT-12 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-12：引文/注释/命令/测试计数） | panel/fact.md FACT-12 | 不适用（转出） |
| FACT-13 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-13：引文/注释/命令/测试计数） | panel/fact.md FACT-13 | 不适用（转出） |
| FACT-14 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-14：引文/注释/命令/测试计数） | panel/fact.md FACT-14 | 不适用（转出） |
| FACT-15 | fact | P1 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-15：引文/注释/命令/测试计数） | panel/fact.md FACT-15 | 不适用（转出） |
| FACT-16 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-16：引文/注释/命令/测试计数） | panel/fact.md FACT-16 | 不适用（转出） |
| FACT-17 | fact | P1 | 文档事实 | ① | 已修：§4.6 改为“Rust 多 P 位（0x7 vs C 的 C+E=0x5）” | panel/fact.md FACT-17 | L4 文档五门重跑 |
| FACT-18 | fact | P2 | 文档事实 | ② | 转出：follow-ups.md D05-F01..F04（FACT-18：引文/注释/命令/测试计数） | panel/fact.md FACT-18 | 不适用（转出） |
| READER-A-01 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-01） | panel/reader-ab.md READER-A-01 | 不适用（转出） |
| READER-A-02 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-02） | panel/reader-ab.md READER-A-02 | 不适用（转出） |
| READER-A-03 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-03） | panel/reader-ab.md READER-A-03 | 不适用（转出） |
| READER-A-04 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-04） | panel/reader-ab.md READER-A-04 | 不适用（转出） |
| READER-A-05 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-05） | panel/reader-ab.md READER-A-05 | 不适用（转出） |
| READER-A-06 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-06） | panel/reader-ab.md READER-A-06 | 不适用（转出） |
| READER-A-07 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-07） | panel/reader-ab.md READER-A-07 | 不适用（转出） |
| READER-A-08 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-08） | panel/reader-ab.md READER-A-08 | 不适用（转出） |
| READER-A-09 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-09） | panel/reader-ab.md READER-A-09 | 不适用（转出） |
| READER-A-10 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-10） | panel/reader-ab.md READER-A-10 | 不适用（转出） |
| READER-A-11 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-11） | panel/reader-ab.md READER-A-11 | 不适用（转出） |
| READER-A-12 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-A-12） | panel/reader-ab.md READER-A-12 | 不适用（转出） |
| READER-B-01 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-01） | panel/reader-ab.md READER-B-01 | 不适用（转出） |
| READER-B-02 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-02） | panel/reader-ab.md READER-B-02 | 不适用（转出） |
| READER-B-03 | reader-b | P1 | 读者/证据相反 | ① | 同 FACT-05 修复（dup-of FACT-05：LAPIC 基址 assert） | panel/reader-b.md READER-B-03 | L4 文档五门重跑 |
| READER-B-04 | reader-b | P1 | 读者/注释过时 | ① | 同 FACT-06 修复（dup-of FACT-06：timer_irq_gate 注释） | panel/reader-b.md READER-B-04 | L4 文档五门重跑 |
| READER-B-05 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-05） | panel/reader-ab.md READER-B-05 | 不适用（转出） |
| READER-B-06 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-06） | panel/reader-ab.md READER-B-06 | 不适用（转出） |
| READER-B-07 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-07） | panel/reader-ab.md READER-B-07 | 不适用（转出） |
| READER-B-08 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-08） | panel/reader-ab.md READER-B-08 | 不适用（转出） |
| READER-B-09 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-09） | panel/reader-ab.md READER-B-09 | 不适用（转出） |
| READER-B-10 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-10） | panel/reader-ab.md READER-B-10 | 不适用（转出） |
| READER-B-11 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-11） | panel/reader-ab.md READER-B-11 | 不适用（转出） |
| READER-B-12 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-12） | panel/reader-ab.md READER-B-12 | 不适用（转出） |
| READER-B-13 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-13） | panel/reader-ab.md READER-B-13 | 不适用（转出） |
| READER-B-14 | reader-ab | P2 | 读者/结构 | ② | 转出：follow-ups.md D05-F11（READER-B-14） | panel/reader-ab.md READER-B-14 | 不适用（转出） |
| READER-C-01 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-01） | panel/reader-c.md READER-C-01 | 不适用（转出） |
| READER-C-02 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-02） | panel/reader-c.md READER-C-02 | 不适用（转出） |
| READER-C-03 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-03） | panel/reader-c.md READER-C-03 | 不适用（转出） |
| READER-C-04 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-04） | panel/reader-c.md READER-C-04 | 不适用（转出） |
| READER-C-05 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-05） | panel/reader-c.md READER-C-05 | 不适用（转出） |
| READER-C-06 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-06） | panel/reader-c.md READER-C-06 | 不适用（转出） |
| READER-C-07 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-07） | panel/reader-c.md READER-C-07 | 不适用（转出） |
| READER-C-08 | reader-c | P1 | 读者/证据相反 | ① | 同 FACT-05 修复（dup-of FACT-05：LAPIC 基址 assert） | panel/reader-c.md READER-C-08 | L4 文档五门重跑 |
| READER-C-09 | reader-c | P1 | 读者/注释过时 | ① | 同 FACT-06 修复（dup-of FACT-06：timer_irq_gate 注释） | panel/reader-c.md READER-C-09 | L4 文档五门重跑 |
| READER-C-10 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-10） | panel/reader-c.md READER-C-10 | 不适用（转出） |
| READER-C-11 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-11） | panel/reader-c.md READER-C-11 | 不适用（转出） |
| READER-C-12 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-12） | panel/reader-c.md READER-C-12 | 不适用（转出） |
| READER-C-13 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-13） | panel/reader-c.md READER-C-13 | 不适用（转出） |
| READER-C-14 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-14） | panel/reader-c.md READER-C-14 | 不适用（转出） |
| READER-C-15 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-15） | panel/reader-c.md READER-C-15 | 不适用（转出） |
| READER-C-16 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-16） | panel/reader-c.md READER-C-16 | 不适用（转出） |
| READER-C-17 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-17） | panel/reader-c.md READER-C-17 | 不适用（转出） |
| READER-C-18 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-18） | panel/reader-c.md READER-C-18 | 不适用（转出） |
| READER-C-19 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-19） | panel/reader-c.md READER-C-19 | 不适用（转出） |
| READER-C-20 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-20） | panel/reader-c.md READER-C-20 | 不适用（转出） |
| READER-C-21 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D05-F12（READER-C-21） | panel/reader-c.md READER-C-21 | 不适用（转出） |

## P-REFUTE 附加（不属于 `## FINDING` 台账）

| 主张 | 结果 | 处置 |
|---|---|---|
| CLAIM 01 | 反证失败；P2（`arch_init` 必须最后缺构建条件） | 转 D05-F13 |
| CLAIM 02 | 反证失败；P2 ×3（主片屏蔽字位置、全屏蔽口径、插入写操作口径） | 转 D05-F13 |
| CLAIM 03 | 反证失败；P2（APIC 初始化失败回退未写） | 转 D05-F13 |
| CLAIM 04 | **反证成功 P1**：APIC/no_apic=0 单 CPU 构建在 cstart 内启动 PIT 校准窗口 | 已修 §2.6/篇头/教学目标 |
| CLAIM 05 | **反证成功 P1**：mask_all 实际 min(nr_irq_vectors,16)；LAPIC 不一致 assert 拒绝 | 已修 §4.4 表与 §4.4 LAPIC 段 |

## R1 完整修改轮（V3.4；D-075 授权的从头收敛）

> 面板：`panel/round1/`（fact 11、design-d2 6、reader-a 4、reader-b 2、reader-c 2 条 FINDING；SUGGESTION 39；refute 6 条 CLAIM；INDEX.md 由 `panel-index.py` 生成）。
> 这是 D05 的**首个完整修改轮**（轮型 full，有 ⑥ 实施权）：旧 V1 轮（83 条、72 条 ② 未获批准）按 L0.8 口径由本轮的全新 survey 取代，旧台账保留为历史段不继承、不丢弃。
> 身份键：`RD05-` 前缀；refute 机器索引 `CLAIM 11..16`。
> 独立回执：`certify/round1-certify.md`（首轮 12/3——其中两条是任务书笔误混入的 D04 条目；修正 5 处后终判以复核为准）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R1-01 | RD05-FACT-01、RD05-FACT-02、RD05-FACT-03 | R1 | fact | P2 | finding | ① | 已修：三条逐字漂移。① 文档 1053 行 `use minix_plat::{InterruptRouter, CurrentInterruptController};`，源码 2410 行是 `use minix_plat::{CurrentInterruptController, | panel/round1/ 的 finding | RD05-FACT-01 等 3 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-02 | RD05-FACT-04、RD05-FACT-05、RD05-FACT-06 | R1 | fact | P2 | finding | ① | 已修：① 文档 1730 行"（s14n：150s 全程 tick 臂 <1000 次调用）"——源码 3782 行是 `(over 150 s the tick arm was called fewer than 1000 times)`，没有 `s14n` 这个记号；整段注释在文档 | panel/round1/ 的 finding | RD05-FACT-04 等 3 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-03 | RD05-FACT-07、RD05-FACT-08、RD05-FACT-09 | R1 | fact | P3 | finding | ① | 已修：文档引 `PASS = uptime reaches 5 ticks with interrupts delivered through the S-8 stub path`；源文件顶部注释实际是 "…delivered through the trap stub path…"。 | panel/round1/ 的 finding | RD05-FACT-07 等 3 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-04 | RD05-FACT-10、RD05-FACT-11、RD05-SUG-01 | R1 | fact | P3 | finding | ① | 已修：示意块把 `init_local_timer` 画成无默认体的必选方法（分号结尾）；源码 54 行它有默认体 `fn init_local_timer(…) -> u32 {`。块注自称"默认实现以空体/零值占位"，示意内其他四个默认方法（write_local_timer_ic | panel/round1/ 的 finding | RD05-FACT-10 等 3 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-05 | RD05-SUG-02、RD05-SUG-03 | R1 | fact | P3 | suggestion | ① | 已修：按 lib.rs:1741-1751 的现行设计重写理由 1 后半句（Phase A.2 为空、保留由构造与 A2 deduction record 保证），并顺带检查 `01-boot-shim-bootstrap.md` 引用的 §2.4 是否同样描述了已移除的 cut_me | panel/round1/ 的 suggestion | RD05-SUG-02 等 2 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-06 | RD05-DESIGN-D2-01、RD05-DESIGN-D2-02、RD05-DESIGN-D2-04 | R1 | design-d2 | P2 | finding | ⑥ | [DEL]、[MODERN]、[DEL] 已实施（修改轮 ⑥）：⑥ 已实施（修改轮）：trait 的文档注释仍描述被放弃的上一代设计——「Only RISC-V uses it: the CLINT is a single MMIO block with one mtimecmp per hart at mtimecmp_base + cpu_id * mtime | panel/round1/ 的 finding | RD05-DESIGN-D2-01 等 3 条的实施由 certify/round1-certify.md 代码侧条目回读确认；`cargo check`/`cargo test -p minix-kernel --features mock` 866/0 |
| RD-R1-07 | RD05-DESIGN-D2-05 | R1 | design-d2 | P2 | finding | ⑥ | [MODERN] 已实施（修改轮 ⑥）：⑥ 已实施（修改轮）：`DEFAULT_HZ` 在两个 crate 各存一份 100，arch 副本注释给出的理由是「The kernel crate (os/kernel/src/clock.rs) has an independent copy to avoid a cross-crate dep | panel/round1/ 的 finding | RD05-DESIGN-D2-05 等 1 条的实施由 certify/round1-certify.md 代码侧条目回读确认；`cargo check`/`cargo test -p minix-kernel --features mock` 866/0 |
| RD-R1-08 | RD05-DESIGN-D2-03、RD05-DESIGN-D2-06 | R1 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：与 FACT-11 同一节选的补省略项，随 fact 批一并落地（dup-of: RD-R1-04） | panel/round1/ 的 finding | dup-of RD-R1-04；certify/round1-certify.md 对应条目确认该节选已补省略 |
| RD-R1-09 | RD05-READER-A-01、RD05-READER-A-02 | R1 | reader-a | P2 | finding | ④ | 已修：图把"8259A 从片"的输出画成汇入"主片 INT ──► IOAPIC 输入 2"那条线（┘ 汇入 ├─► 主片 INT），而紧挨着的括注说级联线在 IOAPIC 上走输入 0；§4.4 的散文也明确说从片级联走 IOAPIC 引脚 0。同一张图内两条信息对"从片中断从 IO | panel/round1/ 的 finding | RD05-READER-A-01 等 2 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-10 | RD05-READER-A-03 | R1 | reader-a | P1 | finding | ④ | 已修：①L129 断言实现侧"可以在进程表建立之前执行"并让 §4.7 解释为什么；②L1772 说这个位置"即 C `boot_cpu_init_timer` 的位置"，而 §2.6 L612 刚论证过 C 的这个位置要求进程表已经就位（在进程表建立之后）；③L1774 的"只是它把 | panel/round1/ 的 finding | RD05-READER-A-03 等 1 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-11 | RD05-READER-A-04、RD05-RA-S01、RD05-RA-S02 | R1 | reader-a | P3 | finding | SUGG | 读者 P3（§4.0c 门槛，不阻断）：推演的核心前提——"存在仍在用编译期 `DEFAULT_HZ`（100）的记账路径"——文档从未指认对象：`ClockState` 的字段在 `with_hz(60)` 下已是 60，§3.2 只说过 `new()` 用 `DEFAULT_HZ` 构造。哪条记账路径绕过字段直接读 | panel/round1/ 的 finding | RD05-READER-A-04 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-12 | RD05-RA-S03、RD05-RA-S04、RD05-RA-S05 | R1 | reader-a | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：一批硬件/仓库术语在首用处无定义（①），其中 `hart`、`BKL`、`PPI`、`SPI`、`LVT`、`S-8`、`PD-22`、`earm`、`直核链` 全文从未展开；读者画像只承诺 OS 基本概念与 8259A/PIT 概念级认识，不承诺这些词。 - 影响：多数词可凭上 | panel/round1/ 的 suggestion | RD05-RA-S03 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-13 | RD05-RA-S06、RD05-RA-S07、RD05-RA-S08 | R1 | reader-a | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：两处小表述失准（④/①）：所谓"三种返回"实际是三种输入情形、两种返回值（第三种"前缀不命中"同样返回 `NULL`）；"=`\0` 形式的等号"与紧邻代码不符——引文里代码只检查 `*envp == '='`，并不验证等号之后是什么，"`\0`"从何而来读者无从知晓。 - 影响 | panel/round1/ 的 suggestion | RD05-RA-S06 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-14 | RD05-READER-B-01、RD05-READER-B-02 | R1 | reader-b | P2 | finding | ④ | 已修：三个小节对同一调用序列给出互相冲突的步骤序数。§2.2 与 §2.3 采用"含 prot_init 的四节点链"基准（init_clock=第二步、intr_init=第三步），§2.4 的 arch_init 也标"第三步"，与 §2.3 撞号。任何单一计数基准都无法同时满足三 | panel/round1/ 的 finding | RD05-READER-B-01 等 2 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-15 | RD05-RB-S01、RD05-RB-S02、RD05-RB-S03 | R1 | reader-b | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：同一份三项污染清单（load_update 除零、init_local_timer 除零、APIC 校准链失准）在 §1.1 与 §2.2 近乎逐字出现两次；§2.2 已自认"同一张污染清单（§1.1）在这里逐条落地"，却仍整段重抄。 - 影响：约十余行重复推高篇幅，两处将来更新 | panel/round1/ 的 suggestion | RD05-RB-S01 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-16 | RD05-RB-S04、RD05-RB-S05、RD05-RB-S06 | R1 | reader-b | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：同一篇内"门"的层级数随章节漂移：两级（§3.7、§4.1"本地门与控制器门"）→ 三层（§4.7）→ 四级（§4.7.1）。每处都做了局部对账，但全篇没有一处给出统一的门层级总表。 - 影响：读者每换一节都要重新确认"这次数到几层、多出来的层是谁"；四级里"后两级"（Redis | panel/round1/ 的 suggestion | RD05-RB-S04 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-17 | RD05-RB-S07、RD05-RB-S08、RD05-RB-S09 | R1 | reader-b | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：同一组缺口以两张相邻表格出现两次（§5.3 按"机制→断言"行，§5.4 按"缺口→为什么→谁能证"行），内容一一对应（M10↔ACPI、M11↔PMU、M12↔MMIO、M16↔用户态、M17↔风暴、M8/M9/M14↔位级）；TODO 也在两节各提一次。 - 影响：相邻两节读 | panel/round1/ 的 suggestion | RD05-RB-S07 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-18 | RD05-RB-S10、RD05-RB-S11、RD05-RB-S12 | R1 | reader-b | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）："S-8""s14n"是仓库内部阶段/工作记录编号。文档在引用注释时对 `smp_todo.md` 有"块内有一处工作记录编号……按原文引用"的声明惯例，但 S-8 作为判定标准（§5.2）与诊断依据（§6.1 的确认动作）的权威指针出现时无任何解码，冷读者无法解析它指什么。 - | panel/round1/ 的 suggestion | RD05-RB-S10 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-19 | RD05-RB-S13、RD05-RB-S14 | R1 | reader-b | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：§3.4 的 C 侧主锚点是 `put_irq_handler`/`irq_handlers`（正文在 §2.6 引用）与端口常量（§2.3），标题回指却只写"（→§2.4）"；§2.4 只贡献了 `lapic_addr` 一项。 - 影响：按回指链回读 C 章的读者落到 arc | panel/round1/ 的 suggestion | RD05-RB-S13 等 2 条已登记 suggestions.md 的 R1 段 |
| RD-R1-20 | RD05-READER-C-01、RD05-READER-C-02 | R1 | reader-c | P2 | finding | ④ | 已修：同一因果主张（"持久 tick 源为什么不在 cstart"）在文档内有两套互斥的理由集。§1.1 与 §2.6 一致：校准只被当作"旁证"，真正的两条依赖是 cycles_accounting_init 与真实处理函数的进程上下文需求；而章末复述把"那次编程里含一次'等时钟中断 | panel/round1/ 的 finding | RD05-READER-C-01 等 2 条的改动由 certify/round1-certify.md 对应条目回读确认 |
| RD-R1-21 | RD05-RC-S01、RD05-RC-S02、RD05-RC-S03 | R1 | reader-c | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：术语"全屏蔽"在本文档承载两个互斥状态——实际结束态"除级联外全屏蔽"（0xFB/0xFF）与推演 2 反例中的"含级联全屏蔽"（~0）。§2.4/§2.7/教学目标③把实际态复述成"每条线都屏蔽/全屏蔽"，与推演 2 的用法正面相撞：同一个词在一处是事实、在另一处是被论证的错误 | panel/round1/ 的 suggestion | RD05-RC-S01 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-22 | RD05-RC-S04、RD05-RC-S05、RD05-RC-S06 | R1 | reader-c | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：这句话有两种读法且一真一假。读作位置陈述（"它在进程表建立之前执行"）时，与 §2.6、§5.2、§4.7 自己的位置描述全部冲突；读作能力陈述（"即使放在进程表之前也安全"）时与 §4.7 的 IF=0 安全论证相容——但 §4.7 全节未出现"进程表"字样，"见 §4.7"的 | panel/round1/ 的 suggestion | RD05-RC-S04 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-23 | RD05-RC-S07、RD05-RC-S08、RD05-RC-S09 | R1 | reader-c | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：教学目标②；缺陷类型：证据不足（退化清单不完整）。 | panel/round1/ 的 suggestion | RD05-RC-S07 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-24 | RD05-RC-S10、RD05-RC-S11、RD05-RC-S12 | R1 | reader-c | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：损害链的中间项（"仍在用编译期 100 的记账路径"）没有点名任何读者。"DEFAULT_HZ 的编译期读者"被列为落点对象却从未列举；两个依据锚点（boot_init_timer 的实参、with_hz 的存在）都证不出"存在读编译期 100 的记账路径"这一步。若所有记账消费 | panel/round1/ 的 suggestion | RD05-RC-S10 等 3 条已登记 suggestions.md 的 R1 段 |
| RD-R1-25 | RD05-RC-S13、RD05-RC-S14 | R1 | reader-c | P3 | suggestion | SUGG | 读者 P3（§4.0c 门槛，不阻断）：小结的两条次序原则里，"arch_init 不得早于 intr_init"的写侧依据（门表重建需要已定义屏蔽态）消失了。仅凭 §2.7/§1.4 的复述，arch_init 与 intr_init 互不读对方产物，交换会显得无害——与 §2.4 的结论相抵。 - 影响读者：用小结 | panel/round1/ 的 suggestion | RD05-RC-S13 等 2 条已登记 suggestions.md 的 R1 段 |
| RD-R1-26 | CLAIM-11、CLAIM-13、CLAIM-14 | R1 | refute | P3 | claim | FP | 反证失败（主张成立） | i8259.c 的 ICW1/ICW2/ICW3/ICW4/OCW1 序列与 0xFB/0xFF 屏蔽字（源码原文核对）；clock.c:294 boot_cpu_init_timer 与 main.c 的调用链（源码原文核对）；clock.rs:709 DEFAULT_HZ、isa_irq_to_pin 0→2 映射、pic_init 尾调用（源码原文核对） | CLAIM-11 反证失败即主张成立；其非阻断精度瑕疵已按源码改正 |
| RD-R1-27 | CLAIM-15 | R1 | refute | P3 | claim | FP | 反证失败（主张成立） | TIMER_IRQ 30/0 取值、CNTP 门控 0x2、GICR 帧+分组、PLIC 空操作、SBI 通道、sie 门控（源码原文核对） | CLAIM-15 反证失败即主张成立；其非阻断精度瑕疵已按源码改正 |
| RD-R1-28 | CLAIM-12 | R1 | refute | P2 | claim | FP | 反证失败（主张成立） | arch_clock.c:33,55-56 的 TIMER_FREQ/0 除零路径（源码原文核对） | CLAIM-12 反证失败即主张成立；其非阻断精度瑕疵已按源码改正 |
| RD-R1-29 | CLAIM-16 | R1 | refute | P2 | claim | ① | 已修：非阻断（CLAIM-02 污染枚举场景混用 + CLAIM-03 锚点勘误已按源码改正） | panel/round1/ 的 claim | CLAIM-16 等 1 条的改动由 certify/round1-certify.md 对应条目回读确认 |

## R2 确认轮（V3.4；全量 survey，收敛声明轮）

> 面板：`panel/round2/`（fact 5、design-d2 9、reader-a 3、reader-b 3、reader-c 1 条 FINDING；SUGGESTION 42；refute 6 条 CLAIM；INDEX.md 同源 FINDING 21 / SUGGESTION 42 / CLAIM 6）。
> 确认轮权限按 §3.6：只修本轮新准入的 P0/P1/P2 事实与同步项（全部本轮修完）；不实施 ⑥。
> 身份键：`RD05R2-` 前缀；refute 机器索引 `CLAIM 21..26`。
> 独立回执：`certify/round2-certify.md`（8/8 CLOSED，首轮即 PASS）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R2-01 | RD05R2-FACT-01 | R2 | fact | P1 | finding | ① | 已修：引文块（标注"代码引用（文档注释与常量定义行）"）的最后一段不是原文。文档写成 "**Authoritative definition**: this is the arch-level constant. The kernel crate re-exports it（`os/k | panel/round2/ 的 finding | RD05R2-FACT-01 等 1 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-02 | RD05R2-FACT-02、RD05R2-FACT-03、RD05R2-FACT-04 | R2 | fact | P2 | finding | ① | 已修："kernel 层的副本在……注释里额外写了一句「需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定」"——这句在 `os/kernel/src/clock.rs` 与 `os/arch/src/arch/clock.rs` 的任何注释里都不存在（两文件 | panel/round2/ 的 finding | RD05R2-FACT-02 等 3 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-03 | RD05R2-FACT-05 | R2 | fact | P2 | finding | ① | 已修：这是全篇端到端证明（M13、§5.4 收束段）的关键事实断言，但证据（三份串口日志）不在文档内，也未给出日志路径；本清单允许范围内无法核验。载体源码侧的判定文本（PASS 横幅、riscv64 的 `matched > 0 && now >= target`、`target +  | panel/round2/ 的 finding | RD05R2-FACT-05 等 1 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-04 | RD05R2-SUG-01、RD05R2-SUG-02、RD05R2-SUG-03 | R2 | fact | P3 | suggestion | ① | 已修：'随后停 PIT、摘临时钩子、关中断（`apic.c:491`、`:499-512`）'中，"停 PIT"实际发生在校准钩子内 `stop_8253A_timer()`（`apic.c:427`，`probe_ticks == PROBE_TICKS` 分支），不在所引的 499 | panel/round2/ 的 suggestion | RD05R2-SUG-01 等 3 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-05 | RD05R2-DESIGN-D2-01 | R2 | design-d2 | P1 | finding | ① | 已修：RD05R2-DESIGN-D2-01 | panel/round2/ 的 finding | RD05R2-DESIGN-D2-01 等 1 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-06 | RD05R2-DESIGN-D2-02、RD05R2-DESIGN-D2-03、RD05R2-DESIGN-D2-04 | R2 | design-d2 | P2 | finding | ① | 已修：RD05R2-DESIGN-D2-02 | panel/round2/ 的 finding | RD05R2-DESIGN-D2-02 等 3 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-07 | RD05R2-DESIGN-D2-05、RD05R2-DESIGN-D2-06 | R2 | design-d2 | P2 | finding | ① | 已修：RD05R2-DESIGN-D2-05 | panel/round2/ 的 finding | RD05R2-DESIGN-D2-05 等 2 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-08 | RD05R2-DESIGN-D2-07、RD05R2-DESIGN-D2-08、RD05R2-DESIGN-D2-09 | R2 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-DESIGN-D2-07 | panel/round2/ 的 finding | RD05R2-DESIGN-D2-07 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-09 | RD05R2-READER-A-01、RD05R2-READER-A-02、RD05R2-READER-A-03 | R2 | reader-a | P2 | finding | ④ | 已修：推演断言的机制链在文档内无法闭合，且与所引代码存在表观矛盾。推演场景里，窗口内的 tick 之后紧随的就是 `put_irq_handler` 调用；按所引代码，只要此刻 `irq_actids[该向量]` 清零后等于 0，注册路径会**再次 unmask**。"线被关回且没有代 | panel/round2/ 的 finding | RD05R2-READER-A-01 等 3 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-10 | RD05R2-RA-S01、RD05R2-RA-S02、RD05R2-RA-S03 | R2 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RA-S01 | panel/round2/ 的 suggestion | RD05R2-RA-S01 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-11 | RD05R2-RA-S04、RD05R2-RA-S05、RD05R2-RA-S06 | R2 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RA-S04 | panel/round2/ 的 suggestion | RD05R2-RA-S04 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-12 | RD05R2-RA-S07、RD05R2-RA-S08、RD05R2-RA-S09 | R2 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RA-S07 | panel/round2/ 的 suggestion | RD05R2-RA-S07 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-13 | RD05R2-RA-S10 | R2 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RA-S10 | panel/round2/ 的 suggestion | RD05R2-RA-S10 等 1 条已登记 suggestions.md 的 R2 段 |
| RD-R2-14 | RD05R2-READER-B-01、RD05R2-READER-B-02 | R2 | reader-b | P2 | finding | ④ | 已修：RD05R2-READER-B-01 | panel/round2/ 的 finding | RD05R2-READER-B-01 等 2 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-15 | RD05R2-READER-B-03、RD05R2-RB-S01、RD05R2-RB-S02 | R2 | reader-b | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-READER-B-03 | panel/round2/ 的 finding | RD05R2-READER-B-03 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-16 | RD05R2-RB-S03、RD05R2-RB-S04、RD05R2-RB-S05 | R2 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RB-S03 | panel/round2/ 的 suggestion | RD05R2-RB-S03 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-17 | RD05R2-RB-S06、RD05R2-RB-S07、RD05R2-RB-S08 | R2 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：机制与措辞统一到 §2.6 一处，§1.2 只留一句后果加回指，§2.5 留一行回指。 | panel/round2/ 的 suggestion | RD05R2-RB-S06 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-18 | RD05R2-RB-S09、RD05R2-RB-S10、RD05R2-RB-S11 | R2 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：统一收进各节末尾的修订括注或脚注，正文只保留单一现行陈述。 | panel/round2/ 的 suggestion | RD05R2-RB-S09 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-19 | RD05R2-RB-S12、RD05R2-RB-S13、RD05R2-RB-S14 | R2 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RB-S12 | panel/round2/ 的 suggestion | RD05R2-RB-S12 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-20 | RD05R2-RB-S15 | R2 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：3 条 FINDING，15 条 SUGGESTION。 | panel/round2/ 的 suggestion | RD05R2-RB-S15 等 1 条已登记 suggestions.md 的 R2 段 |
| RD-R2-21 | RD05R2-READER-C-01 | R2 | reader-c | P2 | finding | ④ | 已修：RD05R2-READER-C-01 | panel/round2/ 的 finding | RD05R2-READER-C-01 等 1 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-22 | RD05R2-RC-S01、RD05R2-RC-S02、RD05R2-RC-S03 | R2 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：校准窗口的依赖是旁证，真正的位置依据是调用链与 §2.6 的两条依赖。 | panel/round2/ 的 suggestion | RD05R2-RC-S01 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-23 | RD05R2-RC-S04、RD05R2-RC-S05、RD05R2-RC-S06 | R2 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RC-S04 | panel/round2/ 的 suggestion | RD05R2-RC-S04 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-24 | RD05R2-RC-S07、RD05R2-RC-S08、RD05R2-RC-S09 | R2 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RC-S07 | panel/round2/ 的 suggestion | RD05R2-RC-S07 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-25 | RD05R2-RC-S10、RD05R2-RC-S11、RD05R2-RC-S12 | R2 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RC-S10 | panel/round2/ 的 suggestion | RD05R2-RC-S10 等 3 条已登记 suggestions.md 的 R2 段 |
| RD-R2-26 | RD05R2-RC-S13、RD05R2-RC-S14 | R2 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：RD05R2-RC-S13 | panel/round2/ 的 suggestion | RD05R2-RC-S13 等 2 条已登记 suggestions.md 的 R2 段 |
| RD-R2-27 | CLAIM-21、CLAIM-22、CLAIM-26 | R2 | refute | P2 | claim | ① | 已修：非阻断（C main.c USE_APIC 块的 no_apic 缺省=config_no_apic=1（形态条件已写入 §1.4）；污染清单的落地链已补 cpu_freq 间接项） | 已修：§1.4 已写形态条件与 cpu_freq 间接落地链 | CLAIM-21 等 3 条的改动由 certify/round2-certify.md 对应条目回读确认 |
| RD-R2-28 | CLAIM-23、CLAIM-24、CLAIM-25 | R2 | refute | P3 | claim | FP | 反证失败（主张成立） | 源码原文核对：intr_init 十次写与全屏蔽 | CLAIM-23 反证失败即主张成立 |
