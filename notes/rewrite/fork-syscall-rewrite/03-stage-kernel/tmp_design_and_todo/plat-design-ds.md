# plat-design-ds: 平台发现设计方案

> **作者**: AI (DeepSeek)
> **状态**: 设计方案（bagging 阶段，待多方案汇总后选择）
> **关联问题**: `plat-discovery-problem.md`
> **创建**: 2026-06-20

---

## 0. 核心决策摘要

| 决策 | 结论 | 理由 |
|------|------|------|
| **归属** | 方案 C（混合）：boot-shim 定位，kernel 解析 | 微内核职责边界清晰；对齐 C 的 `acpi_init()`/`bsp_init()` |
| **命名** | `PlatformDesc` | 比 `MachineDesc` 精确，比 `BoardDesc` 通用 |
| **trait 形状** | 单 trait + 配置值对象（struct） | 简单；kernel 初始化阶段只需一次查询 |
| **传递方式** | `static` 全局 `&'static PlatformDescEnum` | `no_std` 兼容，零分配，boot 阶段单线程安全 |
| **解析时机** | kernel 解析（T3 前半，在 `init_clock_and_interrupts` 之前） | boot-shim 只传原始指针 |
| **x86 路径** | ACPI 主路径，QEMU virt 硬编码兜底 | 符合 x86 生态 |
| **多核** | `CpuTopology` 包含 per-CPU 信息 | 为 SMP 阶段预留 |
| **错误处理** | 解析失败 → panic；QEMU virt 兜底仅在 `#[cfg(feature)]` 下编译 | 不静默降级 |

---

## 1. 归属问题详析：为什么选方案 C

### 1.1 方案对比

方案 A（boot-shim 解析）和方案 B（kernel 解析）的详细对比见 `plat-discovery-problem.md §5`。这里补充我的判断依据：

**否决方案 A 的核心原因**：

1. **boot-shim 职责过重**：boot-shim 当前职责是"加载内核 ELF + 获取内存映射 + 退出固件服务"。如果加入 FDT/ACPI 解析，boot-shim 变成了"第二个 BSP"，负责理解硬件拓扑——这已经越过了 bootloader 和 kernel 的职责边界。

2. **与 Minix3 C 语义偏离**：C 版本在 kernel 内部调用 `acpi_init()`（`arch_system.c:246-288`）和 `bsp_init()`（`arch_system.c:101-132`），硬件理解是 kernel 的职责。Rewrite 原则要求"不改变外部行为和语义"——把硬件理解从 kernel 移到 boot-shim 是一个语义偏移。

3. **kernel 可移植性受损**：如果 kernel 依赖 boot-shim 解析硬件，那么换一个 bootloader（如直接 U-Boot 启动 kernel、或从 EDK2 加载）就需要重新实现整个硬件解析逻辑。kernel 应该能从多种 bootloader 启动。

**方案 C 与方案 B 的区别**：方案 B 要求 boot-shim 什么都不要做，只传原始指针。方案 C 允许 boot-shim 做"定位"——boot-shim 在 UEFI 环境下找 DTB/RSDP 比 kernel 在 `no_std` 下搜索容易得多（UEFI 有 ConfigurationTable、有 `std` 库、有现成的 Protocol）。但解析工作保留在 kernel。

### 1.2 职责边界定义

```
┌─────────────────────────────────────────────────────────────┐
│ boot-shim 职责（UEFI/OpenSBI 环境，有 std）                   │
│                                                             │
│   1. 获取 UEFI 内存映射 → KernelInfo.memmap[]                │
│   2. 加载 kernel ELF → 段拷贝、BSS 清零                      │
│   3. 定位 DTB / RSDP 物理地址 → KernelInfo.dtb_phys_addr /   │
│      .rsdp_phys_addr                                         │
│   4. 退出固件服务（ExitBootServices）                         │
│                                                             │
│   ⛔ 不做：解析 DTB/ACPI、理解硬件拓扑、填充设备地址            │
└─────────────────────────────────────────────────────────────┘
        │
        │ 传递：KernelInfo（含 memmap + DTB/RSDP 指针）
        ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel 职责（no_std，T3 阶段）                                │
│                                                             │
│   1. 根据 DTB/RSDP 指针，选择合适的解析器                     │
│   2. 解析 DTB/ACPI → 构造 PlatformDescEnum                   │
│   3. 存入全局 static，供 ClockArch / InterruptController /   │
│      ArchInit 等 trait 实现读取                              │
│                                                             │
│   ⛔ 不做：在 trait 实现中直接访问 FDT/ACPI 原始数据            │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. PlatformDesc trait 设计

### 2.1 设计原则

- **单一查询点**：kernel 代码只读 `PlatformDesc`，不接触 FDT/ACPI 原始字节。
- **值对象而非 trait 对象满天飞**：配置数据以 plain struct 传递，避免到处 `&dyn TimerDesc`。
- **`no_std` 兼容**：不使用 `Box`、`Vec`、`String`。固定大小数组 + 枚举分发。
- **`Send + Sync`**：所有配置数据在构造后不可变，满足 SMP 并发约束。

### 2.2 核心 trait 与配置 struct

```rust
// ─── 位置: os/libs/minix-boot/src/platform_desc.rs ───

/// 平台硬件描述——统一抽象 FDT/ACPI/硬编码三种来源。
///
/// 内核代码通过此 trait 获取硬件参数，不直接接触 FDT/ACPI 原始数据。
/// 每个架构在 T3 阶段构造一个实现，存入全局 static，后续所有 trait 实现
/// 通过 `platform_desc()` 访问。
pub trait PlatformDesc: Send + Sync {
    /// 时钟源硬件参数。
    fn timer_config(&self) -> TimerConfig;

    /// 中断控制器硬件参数。
    fn interrupt_controller_config(&self) -> InterruptControllerConfig;

    /// CPU 拓扑信息（含 per-CPU 数据，为 SMP 预留）。
    fn cpu_topology(&self) -> CpuTopology;

    /// 早期控制台（UART）配置，如果平台有的话。
    fn console_config(&self) -> Option<ConsoleConfig>;
}
```

```rust
// ─── 时钟源配置 ───

#[derive(Debug, Clone, Copy)]
pub struct TimerConfig {
    /// 定时器类型。
    pub timer_type: TimerType,
    /// MMIO 基地址（用于内存映射定时器如 CLINT mtime）。
    /// ARM Generic Timer 是架构寄存器，不需要 MMIO 地址。
    pub mmio_base: Option<usize>,
    /// 比较/匹配寄存器 MMIO 地址（用于 CLINT mtimecmp）。
    /// RISC-V 每个 hart 有独立 mtimecmp；此为 boot CPU（hart 0）地址。
    pub mmio_compare: Option<usize>,
    /// 定时器输入时钟频率（Hz）。
    /// 例如：QEMU virt RISC-V CLINT = 10 MHz，ARM Generic Timer = CNTFRQ_EL0 值。
    pub frequency_hz: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerType {
    /// x86-64: LAPIC Timer（8254 PIT 存在但在 64 位模式下通常不用）。
    X86Lapic,
    /// ARM64: ARM Generic Timer（架构定时器，通过 CNTFRQ_EL0 等系统寄存器访问）。
    ArmGenericTimer,
    /// RISC-V: CLINT mtime（内存映射计数器）。
    RiscvClint,
}
```

```rust
// ─── 中断控制器配置 ───

#[derive(Debug, Clone, Copy)]
pub struct InterruptControllerConfig {
    /// 控制器类型。
    pub controller_type: InterruptControllerType,
    /// 类型相关的 MMIO 基地址。
    pub mmio: InterruptControllerMmio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptControllerType {
    /// x86-64: LAPIC + IOAPIC。
    X86Apic,
    /// ARM64: GICv3（Distributor + Redistributor + CPU Interface）。
    ArmGicv3,
    /// RISC-V: PLIC + CLINT。
    RiscvPlic,
}

#[derive(Debug, Clone, Copy)]
pub enum InterruptControllerMmio {
    /// x86-64: LAPIC 基址（通过 MSR 获取，但也可存 MMIO）+ IOAPIC 基址。
    X86Apic {
        ioapic_base: usize,
        lapic_base: usize,
    },
    /// ARM64: GICv3 Distributor + Redistributor 基址。
    /// 注意：per-CPU Redistributor 地址在 `CpuTopology.cpus[].gicr_base` 中。
    ArmGicv3 {
        gicd_base: usize,
        /// 公共 Redistributor 区域的基址。对于 GICv3，每个 CPU 的
        /// Redistributor 由 GICR_STRIDE 分隔；具体 per-CPU 地址
        /// 在 `CpuTopology` 中计算。
        gicr_base: usize,
    },
    /// RISC-V: PLIC 基址。
    RiscvPlic {
        plic_base: usize,
    },
}
```

```rust
// ─── CPU 拓扑 ───

/// 系统最大 CPU 数。可在编译时通过 feature 调整。
pub const MAX_CPUS: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    /// 系统 CPU/hart 总数。
    pub cpu_count: usize,
    /// 每个 CPU 的信息。索引 = 逻辑 CPU 编号。
    /// 仅 `cpus[0..cpu_count]` 有效。
    pub cpus: [CpuInfo; MAX_CPUS],
}

#[derive(Debug, Clone, Copy)]
pub struct CpuInfo {
    /// 架构特定硬件 ID：
    /// - x86-64: APIC ID
    /// - ARM64: MPIDR_EL1 的 Aff0
    /// - RISC-V: hart ID
    pub hw_id: u64,
    /// ARM64 GICv3: 此 CPU 的 Redistributor 基址。
    /// 计算方式：gicr_base + hw_id * GICR_STRIDE（通常 0x20000）。
    pub gicr_base: Option<usize>,
    /// RISC-V CLINT: 此 hart 的 mtimecmp 地址。
    /// 计算方式：CLINT mtimecmp 基址 + hart_id * 8。
    pub mtimecmp_addr: Option<usize>,
}
```

```rust
// ─── 早期控制台配置 ───

#[derive(Debug, Clone, Copy)]
pub struct ConsoleConfig {
    /// 控制台类型。
    pub console_type: ConsoleType,
    /// MMIO 基地址（或 x86 I/O 端口）。
    pub base: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleType {
    /// x86-64: COM1 I/O 端口 0x3F8（NS16550A 兼容）。
    X86Com1,
    /// ARM64 / RISC-V: NS16550A 兼容 UART，MMIO 访问。
    Ns16550aMmio,
}
```

### 2.3 全局分发枚举

```rust
/// 平台描述分发枚举——编译时确定所有可能变体，运行时选择。
///
/// 构造阶段选择一个变体，存入全局 static。后续所有访问通过
/// `platform_desc()` 获取 `&dyn PlatformDesc`。
///
/// 为什么不直接用 `Box<dyn PlatformDesc>`？
/// - `no_std` 下没有 alloc
/// - 枚举分发是零成本抽象（无 vtable 间接调用）
pub enum PlatformDescEnum {
    DeviceTree(DeviceTreeDesc),
    Acpi(AcpiDesc),
    QemuVirtRiscv64(QemuVirtRiscv64Desc),
    QemuVirtAarch64(QemuVirtAarch64Desc),
    QemuVirtX86_64(QemuVirtX86_64Desc),
}

impl PlatformDesc for PlatformDescEnum {
    fn timer_config(&self) -> TimerConfig {
        match self {
            Self::DeviceTree(d) => d.timer_config(),
            Self::Acpi(d) => d.timer_config(),
            Self::QemuVirtRiscv64(d) => d.timer_config(),
            Self::QemuVirtAarch64(d) => d.timer_config(),
            Self::QemuVirtX86_64(d) => d.timer_config(),
        }
    }

    fn interrupt_controller_config(&self) -> InterruptControllerConfig {
        match self {
            Self::DeviceTree(d) => d.interrupt_controller_config(),
            Self::Acpi(d) => d.interrupt_controller_config(),
            Self::QemuVirtRiscv64(d) => d.interrupt_controller_config(),
            Self::QemuVirtAarch64(d) => d.interrupt_controller_config(),
            Self::QemuVirtX86_64(d) => d.interrupt_controller_config(),
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        match self {
            Self::DeviceTree(d) => d.cpu_topology(),
            Self::Acpi(d) => d.cpu_topology(),
            Self::QemuVirtRiscv64(d) => d.cpu_topology(),
            Self::QemuVirtAarch64(d) => d.cpu_topology(),
            Self::QemuVirtX86_64(d) => d.cpu_topology(),
        }
    }

    fn console_config(&self) -> Option<ConsoleConfig> {
        match self {
            Self::DeviceTree(d) => d.console_config(),
            Self::Acpi(d) => d.console_config(),
            Self::QemuVirtRiscv64(d) => d.console_config(),
            Self::QemuVirtAarch64(d) => d.console_config(),
            Self::QemuVirtX86_64(d) => d.console_config(),
        }
    }
}
```

### 2.4 全局存储与访问

```rust
// os/kernel/src/platform.rs (或放在 lib.rs)

use core::cell::UnsafeCell;
use minix_boot::platform_desc::{PlatformDesc, PlatformDescEnum};

/// 全局平台描述。boot 阶段单线程写入，之后只读。
static PLATFORM_DESC: UnsafeCell<Option<PlatformDescEnum>> = UnsafeCell::new(None);

/// 初始化平台描述。在 T3 阶段 `init_clock_and_interrupts()` 之前调用。
///
/// # Safety
/// 只在 boot 阶段单线程调用一次。
pub unsafe fn set_platform_desc(desc: PlatformDescEnum) {
    *PLATFORM_DESC.get() = Some(desc);
}

/// 获取平台描述引用。
///
/// # Panics
/// 在 `set_platform_desc()` 之前调用会 panic。
pub fn platform_desc() -> &'static PlatformDescEnum {
    // SAFETY: boot 阶段写入后不再修改；启动后多 CPU 只读访问安全。
    unsafe {
        (*PLATFORM_DESC.get())
            .as_ref()
            .expect("platform_desc() called before set_platform_desc()")
    }
}
```

**为什么用 `UnsafeCell<Option<...>>` 而非 `static mut`？**
- `static mut` 在 Rust 2024 edition 中访问需要 `unsafe`，但没有提供额外安全性。
- `UnsafeCell` 明确表达了"内部可变性" 的语义，且 `&` 引用在写入后是安全的（不再修改）。
- 不需要同步原语（`Mutex`/`OnceLock`），因为 boot 阶段是单线程的。

---

## 3. KernelInfo 扩展

### 3.1 新增字段

```rust
// os/libs/minix-boot/src/kernel_info.rs — 在现有字段后追加

pub struct KernelInfo {
    // ... 现有字段保持不变 ...

    /// Device Tree Blob 物理地址（ARM64 / RISC-V）。
    ///
    /// 由 boot-shim 定位（UEFI ConfigurationTable 或 OpenSBI fw_dynamic_info）。
    /// `None` 表示没有 DTB 可用（x86-64 场景或 QEMU 无 DTB 启动）。
    pub dtb_phys_addr: Option<u64>,

    /// RSDP（Root System Description Pointer）物理地址（x86-64）。
    ///
    /// 由 boot-shim 定位（UEFI ConfigurationTable 中搜索 ACPI 2.0 RSDP）。
    /// `None` 表示没有 RSDP 可用（ARM64/RISC-V 场景）。
    pub rsdp_phys_addr: Option<u64>,
}
```

### 3.2 为什么同时提供两个字段，而不是一个 `firmware_data: Option<u64>`？

- DTB 和 RSDP 是两种**不同格式**的数据。boot-shim 在定位时已经知道它找到的是什么。
- 用一个字段导致 kernel 需要猜测格式（魔数检测），增加了出错概率和复杂性。
- 两个可选字段语义清晰：`dtb_phys_addr` 有值就走 DTB 路径，`rsdp_phys_addr` 有值就走 ACPI 路径。
- 两者同时存在是合法的（某些 x86 嵌入式场景），kernel 自行决定优先级。

---

## 4. 已有 trait 的签名变更

### 4.1 ClockArch

```rust
// os/arch/src/arch/clock.rs

pub trait ClockArch {
    /// 初始化硬件定时器。
    ///
    /// 参数：
    /// - `config`: 平台描述提供的定时器配置（MMIO 地址、输入频率）。
    /// - `hz`: OS 希望的中断频率（如 100 Hz）。
    fn init_timer(config: TimerConfig, hz: u32);

    /// 读取当前 tick 值。
    fn read_ticks() -> u64;
}
```

**变更说明**：`init_timer` 新增 `config: TimerConfig` 参数。具体实现用 `config` 中的 `mmio_base`、`mmio_compare`、`frequency_hz` 替换现有硬编码常量。

**RISC-V 实现示例**（变更后）：

```rust
// os/arch/src/riscv64/clock.rs

impl ClockArch for Riscv64ClockArch {
    fn init_timer(config: TimerConfig, hz: u32) {
        assert_eq!(config.timer_type, TimerType::RiscvClint);
        let mtime_addr = config.mmio_base.expect("CLINT mtime address required");
        let mtimecmp_addr = config.mmio_compare.expect("CLINT mtimecmp address required");

        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(mtime_addr as *const u64);
        }

        let interval = config.frequency_hz / hz as u64;
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(mtimecmp_addr as *mut u64, mtimecmp);
        }

        // Enable S-mode timer interrupt (STIE)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn read_ticks() -> u64 {
        let config = platform_desc().timer_config();
        let mtime_addr = config.mmio_base.expect("CLINT mtime address required");
        unsafe {
            core::ptr::read_volatile(mtime_addr as *const u64)
        }
    }
}
```

### 4.2 InterruptController

```rust
// os/plat/src/interrupt.rs

pub trait InterruptController: Sized {
    /// 从平台描述构造中断控制器。
    fn new(config: InterruptControllerConfig) -> Self;

    /// 初始化中断控制器（配置路由、屏蔽所有 IRQ）。
    fn init(&mut self);

    // ... mask / unmask / ack / eoi / mask_all 保持不变 ...
}
```

**变更说明**：`new()` 从 `const fn new() -> Self` 改为 `fn new(config: InterruptControllerConfig) -> Self`。删除 `set_base()` 方法（不再需要外部调用 set_base，地址在构造时直接传入）。

**RISC-V PLIC 实现示例**（变更后）：

```rust
// os/plat/src/riscv64/interrupt.rs

impl Riscv64InterruptController {
    pub fn new(config: InterruptControllerConfig) -> Self {
        assert_eq!(config.controller_type, InterruptControllerType::RiscvPlic);
        let plic_base = match config.mmio {
            InterruptControllerMmio::RiscvPlic { plic_base } => plic_base,
            _ => panic!("Expected RiscvPlic config"),
        };
        Self {
            plic_base,
            nr_irqs: NR_IRQ_VECTORS,
            context: S_MODE_CONTEXT,
            last_claimed: 0,
        }
    }
    // 删除 set_base() 方法
}
```

### 4.3 ArchInit

```rust
// os/arch/src/arch/arch_init.rs

pub trait ArchInit {
    /// 执行架构特定初始化。
    ///
    /// 参数 `platform` 提供平台描述，用于 ACPI 解析等需要硬件拓扑信息的场景。
    fn init(platform: &dyn PlatformDesc);
}
```

**变更说明**：`init()` 新增 `platform: &dyn PlatformDesc` 参数。x86-64 的 ACPI 初始化需要 RSDP 地址，通过 `platform` 获取。

### 4.4 EarlyConsole

```rust
// os/plat/src/early_console.rs (假设存在此 trait)

pub trait EarlyConsole {
    /// 初始化早期控制台。
    fn init(config: ConsoleConfig);
}
```

**变更说明**：`init()` 新增 `config: ConsoleConfig` 参数。x86-64 的 COM1 端口从硬编码 `0x3F8` 改为从 `config.base` 读取。

---

## 5. QemuVirtDesc 兜底实现

### 5.1 设计理由

QEMU `virt` 机器的硬件布局是固定的，且 QEMU 测试不需要 DTB 解析器。为保持测试快速通过，提供三个架构特定的 `QemuVirt*Desc` 实现。

### 5.2 RISC-V 64 QEMU virt

```rust
// os/arch/src/riscv64/platform.rs (新文件)

use minix_boot::platform_desc::*;

pub struct QemuVirtRiscv64Desc;

impl PlatformDesc for QemuVirtRiscv64Desc {
    fn timer_config(&self) -> TimerConfig {
        TimerConfig {
            timer_type: TimerType::RiscvClint,
            mmio_base: Some(0x200_BFF8),    // CLINT mtime
            mmio_compare: Some(0x200_4000),  // CLINT mtimecmp (hart 0)
            frequency_hz: 10_000_000,        // 10 MHz
        }
    }

    fn interrupt_controller_config(&self) -> InterruptControllerConfig {
        InterruptControllerConfig {
            controller_type: InterruptControllerType::RiscvPlic,
            mmio: InterruptControllerMmio::RiscvPlic {
                plic_base: 0x0C00_0000,
            },
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        let mut cpus = [CpuInfo {
            hw_id: 0,
            gicr_base: None,
            mtimecmp_addr: Some(0x200_4000), // hart 0 mtimecmp
        }; MAX_CPUS];
        CpuTopology {
            cpu_count: 1,
            cpus,
        }
    }

    fn console_config(&self) -> Option<ConsoleConfig> {
        Some(ConsoleConfig {
            console_type: ConsoleType::Ns16550aMmio,
            base: 0x1000_0000, // QEMU virt UART0
        })
    }
}
```

### 5.3 ARM64 QEMU virt

```rust
// os/arch/src/arm64/platform.rs (新文件)

pub struct QemuVirtAarch64Desc;

impl PlatformDesc for QemuVirtAarch64Desc {
    fn timer_config(&self) -> TimerConfig {
        TimerConfig {
            timer_type: TimerType::ArmGenericTimer,
            mmio_base: None,       // ARM Generic Timer 是架构寄存器
            mmio_compare: None,    // 通过 CNTFRQ_EL0 等系统寄存器访问
            frequency_hz: 62_500_000, // QEMU virt 默认 CNTFRQ = 62.5 MHz
        }
    }

    fn interrupt_controller_config(&self) -> InterruptControllerConfig {
        InterruptControllerConfig {
            controller_type: InterruptControllerType::ArmGicv3,
            mmio: InterruptControllerMmio::ArmGicv3 {
                gicd_base: 0x0800_0000,
                gicr_base: 0x080A_0000,
            },
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        let mut cpus = [CpuInfo {
            hw_id: 0,
            gicr_base: Some(0x080A_0000), // CPU 0 redistributor
            mtimecmp_addr: None,
        }; MAX_CPUS];
        CpuTopology {
            cpu_count: 1,
            cpus,
        }
    }

    fn console_config(&self) -> Option<ConsoleConfig> {
        Some(ConsoleConfig {
            console_type: ConsoleType::Ns16550aMmio,
            base: 0x0900_0000, // QEMU virt PL011 UART
        })
    }
}
```

### 5.4 x86-64 QEMU microvm

```rust
// os/arch/src/x86_64/platform.rs (新文件)

pub struct QemuVirtX86_64Desc;

impl PlatformDesc for QemuVirtX86_64Desc {
    fn timer_config(&self) -> TimerConfig {
        TimerConfig {
            timer_type: TimerType::X86Lapic,
            mmio_base: None,       // LAPIC Timer 通过 MSR 配置
            mmio_compare: None,    // LAPIC Timer 是 per-CPU 的
            frequency_hz: 0,       // LAPIC Timer 频率通过 CPUID 或校准获取
        }
    }

    fn interrupt_controller_config(&self) -> InterruptControllerConfig {
        InterruptControllerConfig {
            controller_type: InterruptControllerType::X86Apic,
            mmio: InterruptControllerMmio::X86Apic {
                ioapic_base: 0xFEC0_0000, // QEMU microvm default
                lapic_base: 0xFEE0_0000,  // x86 LAPIC default
            },
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        let mut cpus = [CpuInfo {
            hw_id: 0, // APIC ID 0
            gicr_base: None,
            mtimecmp_addr: None,
        }; MAX_CPUS];
        CpuTopology {
            cpu_count: 1,
            cpus,
        }
    }

    fn console_config(&self) -> Option<ConsoleConfig> {
        Some(ConsoleConfig {
            console_type: ConsoleType::X86Com1,
            base: 0x3F8, // COM1 I/O port
        })
    }
}
```

---

## 6. 启动流程变更

### 6.1 变更前（T3 后半）

```
init_clock_and_interrupts():
    CurrentClockArch::init_timer(clock.hz())     // 内部硬编码 CLINT_MTIME / MTIME_FREQ
    let mut intr = CurrentInterruptController::new()  // 内部硬编码 PLIC_BASE
    intr.init()
    CurrentArchInit::init()                      // 内部硬编码 RSDP 搜索 / 空实现
```

### 6.2 变更后（T3）

```
T3 前半: init_platform_desc()
    ├── 从 KernelInfo 读取 dtb_phys_addr / rsdp_phys_addr
    ├── 有 DTB → DeviceTreeDesc::parse(dtb_addr) → PlatformDescEnum::DeviceTree(_)
    ├── 有 RSDP → AcpiDesc::parse(rsdp_addr) → PlatformDescEnum::Acpi(_)
    ├── 都没有 + cfg(qemu-virt-fallback) → QemuVirtDesc
    └── 都没有 + 无 fallback → panic

T3 后半: init_clock_and_interrupts()
    ├── let platform = platform_desc()
    ├── CurrentClockArch::init_timer(platform.timer_config(), clock.hz())
    ├── let mut intr = CurrentInterruptController::new(platform.interrupt_controller_config())
    ├── intr.init()
    └── CurrentArchInit::init(platform.as_dyn())
```

### 6.3 kmain 中的调用顺序

```rust
// os/kernel/src/lib.rs — kmain() 函数

pub fn kmain(kernel_info: &KernelInfo) -> ! {
    // Phase A: 早期控制台
    // ... 不变 ...

    // Phase A.5: 构造平台描述（NEW）
    // 必须在 init_clock_and_interrupts() 之前执行。
    // SAFETY: boot 阶段单线程。
    unsafe {
        init_platform_desc(kernel_info);
    }

    // Phase B: cstart
    init_protection(kernel_info);
    init_clock_and_interrupts();  // 内部使用 platform_desc()

    // ... 后续不变 ...
}
```

### 6.4 init_platform_desc 实现

```rust
/// 从 KernelInfo 构造平台描述，存入全局 static。
///
/// 解析优先级：
/// 1. DTB（ARM64 / RISC-V 主路径）
/// 2. ACPI RSDP（x86-64 主路径）
/// 3. QEMU virt 硬编码兜底（仅测试用，feature-gated）
unsafe fn init_platform_desc(kernel_info: &KernelInfo) {
    use minix_boot::platform_desc::PlatformDescEnum;

    // 路径 1: Device Tree
    if let Some(dtb_addr) = kernel_info.dtb_phys_addr {
        if let Some(desc) = DeviceTreeDesc::parse(dtb_addr) {
            set_platform_desc(PlatformDescEnum::DeviceTree(desc));
            return;
        }
        // DTB 解析失败不 fallback 到 ACPI——DTB 和 ACPI 是互斥的主路径
        panic!("Device tree parsing failed at physical address {:#x}", dtb_addr);
    }

    // 路径 2: ACPI
    if let Some(rsdp_addr) = kernel_info.rsdp_phys_addr {
        if let Some(desc) = AcpiDesc::parse(rsdp_addr) {
            set_platform_desc(PlatformDescEnum::Acpi(desc));
            return;
        }
        panic!("ACPI RSDP parsing failed at physical address {:#x}", rsdp_addr);
    }

    // 路径 3: QEMU virt 兜底（仅在 feature = "qemu-virt-fallback" 时编译）
    #[cfg(feature = "qemu-virt-fallback")]
    {
        use minix_arch::CurrentPlatform;
        set_platform_desc(CurrentPlatform::qemu_virt_desc());
        return;
    }

    // 没有 DTB、没有 RSDP、也没有 fallback → 无法启动
    panic!(
        "No platform description available. \
         Boot-shim did not provide DTB or RSDP pointers, \
         and QEMU virt fallback is not compiled in."
    );
}
```

---

## 7. 分阶段实施计划

### 7.1 Phase 1: 基础设施 + QEMU 迁移（当前）

**目标**：建立 `PlatformDesc` trait 和配置 struct，将现有硬编码常量迁移到 `QemuVirt*Desc`，QEMU 测试继续通过。

**具体步骤**：

1. 在 `os/libs/minix-boot/src/` 创建 `platform_desc.rs`，定义 §2.2-§2.4 的所有类型。
2. 在 `KernelInfo` 中新增 `dtb_phys_addr` 和 `rsdp_phys_addr` 字段（默认 `None`）。
3. 在各架构 `os/arch/src/{arch}/` 创建 `platform.rs`，实现 `QemuVirt*Desc`。
4. 修改 `ClockArch::init_timer()` 签名，各架构实现从 `config` 读取参数。
5. 修改 `InterruptController::new()` 签名，删除 `set_base()` 方法。
6. 修改 `ArchInit::init()` 签名，新增 `platform` 参数。
7. 修改 `EarlyConsole::init()` 签名，新增 `config` 参数（如果 trait 已存在）。
8. 在 `os/kernel/src/lib.rs` 的 `kmain()` 中添加 `init_platform_desc()` 调用。
9. 修改 `init_clock_and_interrupts()` 通过 `platform_desc()` 获取配置。
10. 更新 `CurrentClockArch` / `CurrentInterruptController` 等 type alias 保持不变（只是实现内部变化）。
11. 运行 QEMU 测试确保回归通过。

**Phase 1 不引入**：DTB/ACPI 解析器。`dtb_phys_addr` 和 `rsdp_phys_addr` 保持 `None`，QEMU 测试走 `QemuVirt*Desc` 兜底。

### 7.2 Phase 2: DTB 解析器（后续）

**目标**：引入 `no_std` 兼容的 FDT 解析器，支持 ARM64 和 RISC-V 真实硬件。

**具体步骤**：

1. 引入或实现 `no_std` FDT 解析库（`fdt` crate 或自研）。
2. 实现 `DeviceTreeDesc::parse(phys_addr: u64) -> Option<DeviceTreeDesc>`。
3. 在对应的 boot-shim 实现中设置 `KernelInfo.dtb_phys_addr`。
4. 在至少一块真实 RISC-V 或 ARM64 开发板上验证。

### 7.3 Phase 3: ACPI 解析器 + 多核（后续）

**目标**：支持 x86-64 真实硬件 ACPI 启动，SMP 多核。

**具体步骤**：

1. 实现 `no_std` ACPI RSDP 搜索 + 表解析（MADT、FADT、HPET、IOAPIC 等）。
2. 实现 `AcpiDesc::parse(rsdp_addr: u64) -> Option<AcpiDesc>`。
3. 扩展 `CpuTopology` 以支持多核（从 MADT 填充 APIC ID 列表）。
4. 在 SMP 初始化阶段使用 `platform_desc().cpu_topology()` 启动 AP。

---

## 8. 验收标准对照

| 标准 | 满足方式 | Phase |
|------|---------|-------|
| 新增平台描述抽象，不暴露 FDT/ACPI 细节 | `PlatformDesc` trait + 配置 struct | Phase 1 |
| 4 处硬编码常量被替换 | `QemuVirt*Desc` 中集中定义，`ClockArch`/`InterruptController` 从 `platform_desc()` 读取 | Phase 1 |
| QEMU virt 测试通过 | `QemuVirt*Desc` 兜底，`#[cfg(feature = "qemu-virt-fallback")]` | Phase 1 |
| 文档同步更新 | `04-clock-interrupt-init.md` 添加"平台描述"章节 | Phase 1 |
| KernelInfo 传递 DTB/RSDP 指针 | `dtb_phys_addr` / `rsdp_phys_addr` 可选字段 | Phase 1 |
| `#![no_std]` | 无 `Box`、`Vec`、`String`；枚举分发 + 固定数组 | Phase 1 |
| 单元测试可验证 | `QemuVirtDesc` 可构造 | Phase 1 |
| 真实硬件 FDT 解析 | `DeviceTreeDesc::parse()` | Phase 2 |
| 真实硬件 ACPI 解析 | `AcpiDesc::parse()` | Phase 3 |

---

## 9. 设计决策记录

### 9.1 为什么不在 `ClockArch` trait 里加 `fn init(config: &TimerConfig)`，而是用全局 `platform_desc()`？

两种方式都满足"硬件操作通过 trait"。选择全局 `platform_desc()` 的理由：

- `ClockArch` 的方法都是关联函数（`fn init_timer(...)` 没有 `&self`），因为硬件定时器是全局单例。加 `&self` 或 `config` 参数不影响 trait 的本质。
- 全局 `platform_desc()` 使 `read_ticks()` 这样的高频调用也能获取 platform 信息（如果需要的话），而不需要调用方传递 context。
- 避免 trait 膨胀：如果每个 trait 方法都带 `config` 参数，调用方需要层层传递 platform 引用。

### 9.2 为什么 `InterruptControllerConfig` 用 enum 而非 trait 对象？

- `InterruptControllerConfig` 是一个值类型（plain data），不需要多态行为。它只是从 `PlatformDesc` 传递到 `InterruptController::new()` 的载体。
- 用 enum 而非 trait 避免了 `no_std` 下的虚表分配问题。
- 编译器可以检查 `match` 的穷尽性。

### 9.3 为什么 x86-64 QEMU virt 的 LAPIC Timer 频率设为 0？

x86-64 的 LAPIC Timer 频率不是架构常量，需要通过 CPUID 或校准（用 PIT/HPET 作为参考）在运行时测量。`frequency_hz = 0` 表示"需要在运行时校准"。在 `X86_64ClockArch::init_timer()` 中，如果 `frequency_hz == 0`，则执行校准逻辑；否则直接使用配置值。

### 9.4 为什么 ARM Generic Timer 不需要 mmio_base？

ARM Generic Timer 是架构定时器，通过以下系统寄存器访问：
- `CNTFRQ_EL0`：计数器频率
- `CNTPCT_EL0`：物理计数器值
- `CNTP_CVAL_EL0` / `CNTP_TVAL_EL0`：比较器/定时器值

这些寄存器是 CPU 的架构寄存器，不是 MMIO 地址。因此 `TimerConfig` 中 `mmio_base` 和 `mmio_compare` 为 `None`。

---

## 10. 参见

- `plat-discovery-problem.md`：问题描述与完整背景
- `00-kernel-overview.md` §1.5：内核执行模型约束
- `04-clock-interrupt-init.md`：当前时钟/中断初始化实现
- `kernel-design.md` §1：T0-T26 完整时间线