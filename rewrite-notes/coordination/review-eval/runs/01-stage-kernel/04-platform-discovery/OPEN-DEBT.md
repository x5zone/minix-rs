# D04 开放欠账

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D04-design-batch | P2 | **用户批准 2026-11-12（代理裁决 D-054）延期至下一 design batch**：DESIGN-D2 的 12 条 P2 代码/架构项（R1：`nr_irqs`/`frequency` 无消费者、解析器静默默认、`CpuInfo`/`ArchMiscDesc` 残留、aarch64 GICD 双表示、`Frozen<T>` Sync bound、ARCH 提案；R2：`DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、ACPI 默认值隐藏、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`） |
| R04-T13 | P1 | [CLOSED] 已实施：`minix-platform` 新增 `table_chain_windows`，`push_platform_windows` 对 RSDP source 在分页前注册根表、前置表签名与 MADT 表体的精确窗口；`minix-platform --lib` 19/0，`kernel --features mock` 866/0/8；残留的「D-055 延期判决账行缺失」是流程账问题，不是本缺陷未修 |

（r7 注记：本行曾引用「D-055 回归」作批准来源，但 D-055 判决内容不含此延期——判决行缺失；本行已按实施关闭，判决账行缺失的流程问题另行补账）
| R3-design-batch | P2 | [CLOSED] 三项设计 P2 已实施：early_console/ConsoleDesc 死链删除、CpuInfo.gicr_base/mtimecmp_addr 删除（控制器按 hw_id 推导）、enable_sched_sgi_for_cpu 改用 elected_bsp_hw_id；6 条设计 P3 与 RD04-A1 按原裁决保留登记。证据见 `os/libs/minix-boot/src/platform.rs`、`os/libs/minix-platform/src/{device_tree,acpi}.rs`、`os/kernel/src/lib.rs`；boot 185/0、platform 19/0、kernel mock 866/0/8。 |
