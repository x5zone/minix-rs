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
