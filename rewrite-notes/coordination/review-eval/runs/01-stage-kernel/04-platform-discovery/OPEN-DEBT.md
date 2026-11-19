# D04 开放欠账

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D04-design-batch | P2 | **用户批准 2026-11-12（代理裁决 D-054）延期至下一 design batch**：DESIGN-D2 的 12 条 P2 代码/架构项（R1：`nr_irqs`/`frequency` 无消费者、解析器静默默认、`CpuInfo`/`ArchMiscDesc` 残留、aarch64 GICD 双表示、`Frozen<T>` Sync bound、ARCH 提案；R2：`DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、ACPI 默认值隐藏、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`） |
| R04-T13 | P1 | **用户批准 2026-11-17（代理裁决 D-055 回归）延期至 D04-design-batch**：ACPI 表链（RSDT/XSDT/MADT）不在任何已注册恒等窗口内；QEMU 通过靠 2 MiB 片取整的布局偶然，不是契约。需按 RSDP 内部指针建窗口或显式登记 ACPI 表链 |

（r7 注记：本行引用「D-055 回归」作批准来源，但 D-055 判决内容不含此延期——判决行缺失，待用户补裁或并入 D04-design-batch 判决；在位期间按 P1 挂账计，不视为已合法销账）
| R3-design-batch | P2 | **用户批准 2026-11-20（代理裁决 D-070）延期至下一 design batch**：RD04-DESIGN-D2-01/02/03 三项设计 P2（[DEL] early_console/ConsoleDesc 零消费者死链、[DEL] CpuInfo.gicr_base/mtimecmp_addr 只写不读、[MODERN] enable_sched_sgi_for_cpu 的 bsp_id hint 与 elected_bsp_hw_id 选举分叉）与 6 条设计 P3、ARCH 提案 RD04-A1（PSCI conduit）。确认轮不实施 ⑥（§3.6），登记后销账 |
