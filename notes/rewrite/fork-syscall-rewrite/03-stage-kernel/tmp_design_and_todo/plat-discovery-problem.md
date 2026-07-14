# plat-discovery-problem: 平台硬件发现问题描述

> **分类**: Kernel 架构抽象 / 硬件发现
> **状态**: 问题描述（待多方案 bagging 后选择实现方案）
> **关联文档**: `01-boot-shim-bootstrap.md`、`03-kmain-cstart.md`、`04-clock-interrupt-init.md`
> **关联源码**:
> - `os/arch/src/riscv64/clock.rs`
> - `os/plat/src/riscv64/interrupt.rs`
> - `os/plat/src/arm64/interrupt.rs`
> - `os/arch/src/x86_64/arch_init.rs`
> - `os/libs/minix-boot/src/kernel_info.rs`

---

## 1. 背景

minix-rs 的内核启动序列（按 `kernel-design.md` 的 T0-T7 时间线）如下：

```
T0: boot-shim 加载 kernel ELF + boot modules，获取 UEFI 内存映射
T1: HigherHalf 切栈跳转到高地址
T2-T3: kmain/cstart → prot_init（GDT/TSS/段寄存器）
T3 后半: init_clock() / intr_init() / arch_init()
T4 之后: 进程表、boot_proc、system_init、调度循环
```

在 `T3 后半`阶段，内核需要初始化：

- **时钟源**：x86-64 的 8254 PIT / LAPIC Timer、ARM64 的 Generic Timer、RISC-V 的 CLINT mtime。
- **中断控制器**：x86-64 的 IOAPIC / LAPIC、ARM64 的 GICv3、RISC-V 的 PLIC。
- **架构杂项**：x86-64 的 ACPI 表、串口（COM1）等。

这些硬件的 MMIO 基址和时钟频率**在不同主板/开发板上是不一样的**。目前实现为了快速在 QEMU `virt` 上跑通，大量使用了**硬编码常量**。

---

## 2. 当前实现的硬编码清单

### 2.1 RISC-V 64 CLINT 定时器

**位置**: [`os/arch/src/riscv64/clock.rs:13-23`](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/clock.rs#L13-L23)

```rust
/// CLINT mtime register address for QEMU virt machine.
/// Currently hardcoded; real hardware needs device tree discovery.
const CLINT_MTIME: usize = 0x200_BFF8;

/// CLINT mtimecmp register address for QEMU virt machine (hart 0).
const CLINT_MTIMECMP: usize = 0x200_4000;

/// CLINT mtime frequency for QEMU virt machine (10 MHz).
/// Currently hardcoded; real hardware needs device tree discovery.
const MTIME_FREQ: u64 = 10_000_000;
```

`Riscv64ClockArch::init_timer()` 和 `read_ticks()` 直接读取这些常量地址。

### 2.2 RISC-V 64 PLIC 中断控制器

**位置**: [`os/plat/src/riscv64/interrupt.rs:9-11`](file:///home/xzhao/github/minix-rs/os/plat/src/riscv64/interrupt.rs#L9-L11)

```rust
/// PLIC base address for QEMU virt machine.
/// Currently hardcoded; real hardware needs device tree discovery.
const PLIC_BASE: usize = 0x0C00_0000;
```

`Riscv64InterruptController::new()` 默认使用 `PLIC_BASE`，同时提供了 `set_base()` 方法允许外部覆盖。这说明实现者已经意识到地址需要外部注入，但缺少一个统一的注入来源。

### 2.3 ARM64 GICv3 中断控制器

**位置**: [`os/plat/src/arm64/interrupt.rs:9-13`](file:///home/xzhao/github/minix-rs/os/plat/src/arm64/interrupt.rs#L9-L13)

```rust
/// GICv3 Distributor base address offset (from GIC base).
const GICD_OFFSET: usize = 0x0000_0000;

/// GICv3 Redistributor base address offset (from GIC base).
const GICR_OFFSET: usize = 0x000A_0000;
```

`AArch64InterruptController::new()` 默认 `gicd_base = 0, gicr_base = 0`，并通过 `set_base()` 外部注入。目前何处调用 `set_base()` 以及注入的值从何而来，需要进一步确认；文档 `04-clock-interrupt-init.md` 暗示是 QEMU virt 固定值。

### 2.4 x86-64 ACPI

**位置**: [`os/arch/src/x86_64/arch_init.rs:32-39`](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/arch_init.rs#L32-L39)

```rust
// 2. ACPI table parsing
// C: acpi_init()
// QEMU virt boot does not depend on ACPI; physical machines need
// RSDP search + table parsing.
```

x86-64 目前完全未实现 ACPI 解析，且 APIC 初始化已经拆到 `X86_64InterruptController::init()` 中。

### 2.5 早期控制台串口

x86-64 的 COM1 串口基址 `0x3F8` 也写在代码里（[`os/arch/src/x86_64/early_console.rs`](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/early_console.rs)）。不过 COM1 是 IBM PC / IA-PC 兼容机的 well-known 端口，在 x86 传统 BIOS/UEFI 环境中通常可以视为架构常量，但某些嵌入式 x86 板子仍可能不同。

---

## 3. 问题陈述

### 3.1 核心问题

当前内核的 `ClockArch`、`InterruptController`、`ArchInit` 等 trait 实现**直接从代码中的硬编码常量获取硬件地址和频率**。这导致：

1. **无法移植到真实硬件**：换一块 RISC-V/ARM 开发板，CLINT/PLIC/GIC 基址可能完全不同，必须改源码、重新编译。
2. **QEMU `virt` 假设泄漏到多处**：同一个假设（CLINT 在 `0x200_BFF8`、PLIC 在 `0x0C00_0000`）分散在多个架构实现文件中。
3. **缺少统一的硬件描述抽象**：ARM/RISC-V 的设备树（Device Tree）和 x86 的 ACPI 是不同机制，但内核需要一个与来源无关的统一接口。
4. **boot-shim 与 kernel 之间的信息传递不完整**：`KernelInfo` 目前主要传递内存映射，没有传递 DTB 或 RSDP 指针。

### 3.2 为什么不是简单地把常量改成配置

几个可能的直觉方案及其问题：

| 直觉方案 | 问题 |
|---------|------|
| 把常量改成 `const fn` 或 `#[cfg(board = "...")]` | 仍然需要为每块板子重新编译，违反"同一份内核镜像适配多板"的目标 |
| 每个 trait 自己解析 FDT/ACPI | `ClockArch` 实现里出现 FDT 节点遍历，违反"硬件操作必须通过 trait"和"关注点分离"原则 |
| 全局静态变量存放基址 | 不利于测试，且 Rust 内核侧需要 `no_std` 下的可变全局状态 |
| 用 boot-shim 直接填好所有地址传给 kernel | boot-shim 变成第二个 BSP，职责过重；且地址空间布局、CPU 拓扑等属于 kernel 初始化阶段需要理解的信息 |

因此，真正需要的是一层**平台描述抽象（Platform Description）**：把 "FDT/ACPI 长什么样" 与 "内核需要哪些硬件参数" 分离开。

---

## 4. 参考：其他系统怎么做

### 4.1 Linux

- ARM/ARM64/RISC-V： mandatory device tree，内核启动时从 bootloader 拿到 DTB 物理地址，统一解析。
- x86：ACPI 为主，DTB 仅在某些嵌入式/固件场景作为补充。
- 硬件驱动通过 `of_*` API（DT）或 `acpi_*` API 读取资源，不直接硬编码地址。

### 4.2 Minix3

Minix3 没有 device tree 机制。其 ARM32 端口采用 **BSP（Board Support Package）** 方式：

- [`minix3/minix/kernel/arch/earm/bsp/ti/omap_init.c`](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/earm/bsp/ti/omap_init.c)
- 不同板子对应不同 BSP 目录，地址写死在板级代码中。

这意味着 minix-rs 如果做 device tree，是**新增能力**，不是从 Minix3 移植。

### 4.3 seL4 / microkernel 思路

- boot-loader 将设备树信息传给 kernel。
- kernel 在初始化阶段解析一次，构造内部的平台描述结构。
- 上层驱动/架构代码只读取内部描述，不接触原始 FDT/ACPI。

---

## 5. 关键归属问题：平台发现属于 boot-shim 还是 kernel？

这是所有方案必须首先回答的问题。两种选择都有强理由，也有明显代价。

### 5.1 方案 A：由 boot-shim 解析，把结构化数据传给 kernel

**理由**：

- boot-shim 运行在 UEFI 环境下，可以较方便地使用 `std` 库、复杂分配器和第三方 FDT/ACPI 解析 crate。
- kernel 保持精简，不需要在 `no_std` 下实现 DTB/ACPI 解析器。
- 解析失败时 boot-shim 可以优雅地报错或回退，不污染 kernel 初始化逻辑。

**代价**：

- boot-shim 变成"半个 BSP"，承担了本属于 kernel 的硬件理解职责。
- `KernelInfo` 需要大幅扩展，包含 GICD/GICR/PLIC/CLINT 等所有平台相关字段，变成一个大而全的结构体。
- kernel 无法在没有 boot-shim 的环境下启动（例如直接从 firmware 启动、或被其他 bootloader 加载）。
- 与 Minix3 C 的语义偏离较大：C 版本在 kernel 内部调用 `acpi_init()`、`bsp_init()` 等板级初始化。

### 5.2 方案 B：由 kernel 解析原始 DTB/RSDP

**理由**：

- 符合微内核"kernel 自己理解硬件"的职责边界。
- boot-shim 只需要传递一个原始指针（DTB 物理地址或 RSDP 物理地址），`KernelInfo` 改动很小。
- 内核可以从多种 bootloader 启动，不限于当前 boot-shim。
- 与 Minix3 C 的 `acpi_init()` / `bsp_init()` 语义更接近。

**代价**：

- kernel 必须实现 `no_std` 兼容的 FDT/ACPI 解析器，或引入合适的 `#![no_std]` crate。
- 解析代码占用内核空间，且在内核初始化早期执行，错误处理需要谨慎。
- 测试更复杂：需要在 `#[cfg(test)]` 中构造模拟 DTB/ACPI 数据。

### 5.3 方案 C：混合方案

- boot-shim 只做"定位"：找到 DTB/RSDP 物理地址，填入 `KernelInfo`。
- kernel 负责解析和使用。
- 对于 QEMU 测试，可额外提供一个 `QemuVirtDesc` 兜底，绕过解析器。

**当前倾向**：多数微内核（seL4、Linux 早期 ARM）采用方案 B 或 C。minix-rs 的 boot-shim 目前职责已经很重（UEFI 内存映射、ELF 加载、ExitBootServices），进一步把硬件理解塞进 boot-shim 可能导致边界模糊。但具体选择仍需 bagging 阶段对比。

---

## 6. 约束与要求

### 6.1 必须遵守的架构原则（来自 `00-kernel-overview.md` §1.5）

1. **所有硬件操作必须通过 trait**：不允许在 `ClockArch` / `InterruptController` 等实现里直接嵌入 FDT/ACPI 解析逻辑或 `#[cfg(target_arch)]` 行为选择。
2. **`#![no_std]`**：设备树/ACPI 解析器必须能在 `no_std` 环境下运行，不能使用 `std` 集合或分配器（或仅在 `#[cfg(test)]` 中使用）。
3. **内核执行模型**：kernel 是 BKL + SMP + 中断上下文，任何跨 CPU 共享的平台描述数据必须是 `Send + Sync` 的。
4. **C 行为是 Ground Truth**：任何新抽象最终都要能解释 Minix3 C 的启动行为，即使 C 版本本身没有 device tree。

### 6.2 平台描述抽象需要回答的问题

1. **来源多样性**：ARM/RISC-V 用 DTB，x86 用 ACPI，QEMU 测试可能继续用硬编码兜底。如何统一表达？
2. **生命周期**：平台描述在 boot-shim 还是 kernel 中解析？原始数据（DTB/ACPI）如何传给 kernel？
3. **接口粒度**：`PlatformDesc` 应该暴露多少信息？至少包括：
   - 中断控制器：类型、MMIO 基址（GICD/GICR、PLIC、IOAPIC 等）
   - 定时器：类型、MMIO 基址、输入频率
   - 串口：MMIO 基址（可选）
   - CPU 拓扑：CPU 数量、hart ID / APIC ID 映射
   - 内存：boot-shim 已传递 memmap，是否也纳入 PlatformDesc？
4. **多核支持**：SMP 阶段（文档 15）需要 per-CPU 数据，平台描述是否要为每个 CPU 提供私有信息？
5. **测试友好**：如何在不启动真实硬件/QEMU 的情况下测试 `PlatformDesc` 解析？

### 6.3 需要保持的兼容性

- 当前 QEMU 测试必须继续通过。
- `Riscv64InterruptController::set_base()`、`AArch64InterruptController::set_base()` 等已有注入点不应被粗暴删除，但应改为从 `PlatformDesc` 自动调用。
- 不应让 boot-shim 承担过多板级初始化职责。

---

## 6. 建议的抽象方向（非最终方案，仅供讨论）

一个可能的分层设计：

```
┌─────────────────────────────────────────┐
│  Kernel 上层：ClockArch / InterruptController │  只读 PlatformDesc，不接触 FDT/ACPI
├─────────────────────────────────────────┤
│  PlatformDesc trait                     │  统一硬件描述接口
├─────────────────────────────────────────┤
│  DeviceTreeDesc  │  AcpiDesc  │  QemuVirtDesc  │  三种来源实现
├─────────────────────────────────────────┤
│  FDT parser      │  ACPI parser│  hardcoded    │  解析原始数据
├─────────────────────────────────────────┤
│  boot-shim 传递 DTB/RSDP 指针            │  数据源入口
└─────────────────────────────────────────┘
```

这个方向是否最优，正是需要 bagging 多方案来决定的部分。

---

## 7. 范围

### 7.1 在本次讨论范围内

- `PlatformDesc` / `MachineDesc` 等抽象接口的设计。
- DTB/ACPI 指针如何从 boot-shim 传递到 kernel（是否扩展 `KernelInfo`）。
- 当前 4 处硬编码（CLINT、PLIC、GIC、ACPI）如何迁移到该抽象。
- `ClockArch::init_timer`、`InterruptController::init` 等 trait 方法是否需要接收平台描述参数。

### 7.2 不在本次讨论范围内（或仅作为输入）

- 完整的 device tree / ACPI 解析器实现细节（节点遍历、字符串表等）。
- 具体到每块真实开发板的移植。
- 替换 boot-shim 获取 UEFI 内存映射的逻辑（内存映射已在 `KernelInfo` 中）。
- 用户态服务（VM/PM/VFS）如何访问平台描述。

---

## 8. 待决策的关键问题

这些问题留给 bagging 阶段各方案回答：

1. **归属问题（最优先）**：平台发现由 boot-shim 还是 kernel 负责？各自职责边界在哪里？
2. **命名**：叫 `PlatformDesc`、`MachineDesc`、`BoardDesc` 还是 `HardwareTopology`？
3. **trait 形状**：是粗粒度一个 trait 包所有，还是细分为 `InterruptControllerDesc`、`TimerDesc`、`CpuTopology` 等子结构？
4. **传递方式**：`PlatformDesc` 以 `&'static dyn PlatformDesc` 全局存在，还是作为参数传给每个 `init()`？
5. **解析时机**：在 boot-shim 解析成结构化数据后传给 kernel，还是只传原始 DTB/RSDP 指针让 kernel 自己解析？
6. **x86 路径**：ACPI 是主要来源，但是否也要支持 DTB 作为补充？QEMU virt 是否需要 ACPI 兜底？
7. **多核扩展**：当前平台描述是否需要考虑 per-CPU redistributor（ARM GICR）、per-hart mtimecmp（RISC-V）？
8. **错误处理**：解析失败是 panic、回退到 QEMU virt、还是进入半初始化状态？

---

## 9. 验收标准（实现方案应满足）

- [ ] 新增平台描述抽象，不暴露 FDT/ACPI 细节给 `ClockArch` / `InterruptController` / `ArchInit`。
- [ ] 当前 4 处硬编码常量被替换为从平台描述读取，或至少提供迁移路径。
- [ ] QEMU `virt` 测试继续通过（可通过 `QemuVirtDesc` 兜底）。
- [ ] 文档 `04-clock-interrupt-init.md` 和相关源码注释同步更新，明确"当前为 QEMU virt 硬编码；真实硬件通过 PlatformDesc 发现"。
- [ ] `KernelInfo` 或等效机制能够传递 DTB/RSDP 原始指针（如果方案选择在 kernel 侧解析）。
- [ ] 新增代码保持 `#![no_std]`，不使用 `std`。
- [ ] 单元测试可验证 `PlatformDesc` 解析结果，无需启动 QEMU。

---

## 10. 参见

- `00-kernel-overview.md` §1.5：内核执行模型约束
- `kernel-design.md` §1：T0-T26 完整时间线
- `04-clock-interrupt-init.md`：当前时钟/中断初始化实现
- `todo.md` §7：设备树 / ACPI / PlatformDesc 相关待办
