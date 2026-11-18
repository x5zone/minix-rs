# D04 开放欠账

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D04-design-batch | P2 | **用户批准 2026-11-12（代理裁决 D-054）延期至下一 design batch**：DESIGN-D2 的 12 条 P2 代码/架构项（R1：`nr_irqs`/`frequency` 无消费者、解析器静默默认、`CpuInfo`/`ArchMiscDesc` 残留、aarch64 GICD 双表示、`Frozen<T>` Sync bound、ARCH 提案；R2：`DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、ACPI 默认值隐藏、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`） |
| R04-T13 | P1 | **用户批准 2026-11-17（代理裁决 D-055 回归）延期至 D04-design-batch**：ACPI 表链（RSDT/XSDT/MADT）不在任何已注册恒等窗口内；QEMU 通过靠 2 MiB 片取整的布局偶然，不是契约。需按 RSDP 内部指针建窗口或显式登记 ACPI 表链 |（r7 注记：本行引用「D-055 回归」作批准来源，但 D-055 判决内容不含此延期——判决行缺失，待用户补裁或并入 D04-design-batch 判决；在位期间按 P1 挂账计，不视为已合法销账）
