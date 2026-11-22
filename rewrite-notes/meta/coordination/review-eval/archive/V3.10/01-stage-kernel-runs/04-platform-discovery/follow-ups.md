# D04 follow-ups —— 本轮转出项与去处

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D04-F01 | FACT-11 | §4.9 两处代码块的注释不是源码原文，未标“转述/删改” | 闭环轮：`init_smp_state`/`init_protection` 块加标签或恢复注释 |
| D04-F02 | FACT-17 | 篇头承诺 riscv64 逐处标 `[ARCH: K-2]`，正文没有一处 | 闭环轮：在 §3.2/§3.3/§4.5/§4.8 的 riscv64 段加标注，或改篇头为“统一属 K-2”并核对 README 约定 |
| D04-F03 | DESIGN-01 | `Frozen::freeze` 在 release 下不拒绝二次冻结，且 `Sync` 只约束 `T: Send`，安全 API 可触发 UB | code batch：文件 `os/libs/minix-types/src/types/cell.rs`；验收 = 二次冻结返回错误或 panic，loom/宿主测试覆盖 |
| D04-F04 | DESIGN-02 | `CpuTopology.nr_cpus` 与 64 槽数组无不变式，消费者裸下标 | code batch：`nr_cpus <= 64` 构造校验或数组长度绑定；验收 = 越界拓扑在构造期失败 |
| D04-F05 | DESIGN-03 | `CpuInfo` 的 `gicr_base`/`mtimecmp_addr` 零生产读者，与 `Gicv3Desc`/`ClintDesc` 构成两个真源 | code batch：删字段或改由描述符查询；验收 = 拓扑体积与读者核对 |
| D04-F06 | DESIGN-04 | `PlatformContext` 是纯转发壳（16 条转发） | code batch：删壳或让它承担真实状态；验收 = 外部行为不变 |
| D04-F07 | DESIGN-05 | `PlatformSource`/`source()` 与 `ConsoleDesc`/`early_console()` 生产消费者为零 | code batch：删或补消费者；验收 = grep 生产调用点 |
| D04-F08 | DESIGN-06 | 三层错误类型在 `parse_by_kind` 压成 3 个粗变体、在 `init_from_kinfo` 被整体丢弃 | code batch：错误透传或至少记录；验收 = 解析失败可从日志/返回值区分 |
| D04-F09 | DESIGN-07 | MADT 记录长度非法时 `break` 静默截断；IOAPIC“最后一条覆盖”与注释“保留第一条”矛盾 | code batch：显式错误/保留首条；验收 = 坏记录测试 |
| D04-F10 | DESIGN-08 | `lapic_base == 0` 无 C 常量回退；单 IOAPIC 丢 id/gsi_base；x2APIC 发现期接受、消费期断言拒绝；多来源 `nr_irqs` 不一致 | 用户裁决 + code batch：逐项给取舍与测试 |
| D04-F11 | DESIGN-09..21 | 其余 13 条设计项：`PlatformDescEnum` 分派、`ClockArch` 面、`BoardArch`、`ArchInit`、`ConsoleDesc` 等 | code-excellence 批次；按 finding 编号逐条给文件与验收 |
| D04-F12 | READER-A 16 条 | A-01..A-16：名词先于定义、`AcpiDesc` downcast、`CpuInfo` 地址算式、`lapic_addr` PA→VA 等 | 闭环轮；优先 A-07/A-08/A-10 |
| D04-F13 | READER-B 12 条 | B-01..B-12：四组问题表 4/5/6 行、①–⑥ 三种切法、`apic_enabled` 保留语义、`/memory` 例外等 | 教学债批次；优先 B-02/B-08 |
| D04-F14 | READER-C 22 条 | C-01..C-22：三种来源两套划分、默认配置 `no_smp` 矛盾、首失败点、`apic_enabled`、扩展校验和、错误变体清单等 | 闭环轮；优先 C-01/C-06/C-08/C-12 |
| D04-F15 | REFUTE CLAIM 02 | acpi 表头 36 字节节选缺语义行、错误变体 6→13、ARM 零发现、`phys_bytes` 注成 u64 | 闭环轮逐条核对 |
| D04-F16 | 复验 | `cargo test -p minix-arch` 测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |
