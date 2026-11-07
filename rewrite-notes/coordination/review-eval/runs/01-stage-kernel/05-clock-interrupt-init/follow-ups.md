# D05 follow-ups —— 本轮转出项与去处

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D05-F01 | FACT-03/07/08/09/10/13/14/16 | 8 处引文块与“逐字/注释按原文”承诺不符：`S-8 stub path`、`pic_init` 中文译文、`init_cpu_interface` 实为 `_current`、漏两条 assert、三处未标省略 | 闭环轮：逐块加“注释有删改/节选”或恢复原文；验收 = 每块与源码 diff |
| D05-F02 | FACT-04/15/18 | FAIL 行号区间偏短；`cargo test --list` 当前因 `minix-arch` 9 处 E0063 不产出；测试钉与 `restart_local_timer` 无关 | 与 D05-F14 合并；先修 `minix-arch` 初始化器 |
| D05-F03 | FACT-12 | `sleep(1)→1.67s` 的后果在本仓没有对应机制；`ClockState` 用 60Hz 构造后 `system_hz()` 就是 60 | 闭环轮：删掉或改写该反事实 |
| D05-F04 | FACT-其余 | 其余 P2 事实项 | 闭环轮逐条核对 |
| D05-F05 | DESIGN-01 | `set_tsc_per_ms` 无调用者 → `TSC_PER_MS` 恒 0，毫秒换算走 `DEFAULT_TSC_PER_MS` 静默默认，C 的校准行为丢失 | code batch：`os/kernel/src/clock.rs:280`；验收 = 启动后 TSC 校准值与 C 同源，或显式记录“不校准”的取舍 |
| D05-F06 | DESIGN-02 | riscv64 外部中断入口 `IrqVector::new(id as u8)` 无范围守卫，PLIC 源号 ≥256 截断成 TIMER_IRQ | code batch：`os/kernel/src/trap_dispatch.rs:1970`；验收 = ≥256 源号返回错误或 panic，不落入 TIMER_IRQ |
| D05-F07 | DESIGN-03 | `os/kernel/src/irq_manager.rs` 有 11 个 `cfg(target_arch)` 块，含 QEMU PLIC context 约定与 ARM 常量名，违反第一原则且有活路径 | code batch：抽 `InterruptRouter`/`TickDelivery` 承载差异；验收 = 语义层 grep 无架构常量 |
| D05-F08 | DESIGN-04 | LAPIC 基址不一致时 `assert_eq!` 拒绝启动；文档已改为如实陈述，但“拒绝 vs 以 MSR 为准”的行为取舍未定 | 用户裁决：保留 fail-closed 或支持重定位 |
| D05-F09 | DESIGN-05..10 | 其余 6 条设计项：`ClockArch::read_ticks` 死方法、三个 `CLOCK_*` 静态量、`set_tsc_per_ms`、`X86_64ArchInit::acpi_tables`、`disable_timer_irq` 无调用者、`PortWriter` 测试接缝 | code-excellence 批次 |
| D05-F10 | DESIGN-11..18 | 其余 8 条设计项：死抽象核实、错误路径、最小化机会 | code-excellence 批次 |
| D05-F11 | READER-A/B 26 条 | A-01..A-12、B-01..B-14：定时器晚做理由、arch_init 次序强度、LAPIC 证据相反、timer_irq_gate 注释、覆盖矩阵矛盾 | 闭环轮；优先 A-02/A-03/B-03/B-04 |
| D05-F12 | READER-C 21 条 | C-01..C-21：全屏蔽口径、定时器理由、三段对照层错位、EFER/CNTP 推演、PROBE_TICKS 除零、LAPIC 证据、arch_init 竞态、向量来源 | 闭环轮；优先 C-01/C-02/C-04/C-07/C-08 |
| D05-F13 | REFUTE P2 | CLAIM 01/02/03 的 P2：`arch_init` 最后缺构建条件、主片屏蔽字位置、APIC 初始化失败回退 | 闭环轮逐条核对 |
| D05-F14 | 复验 | `cargo test -p minix-arch` 的测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |
