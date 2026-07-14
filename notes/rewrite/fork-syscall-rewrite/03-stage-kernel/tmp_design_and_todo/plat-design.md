# plat-design: 平台硬件发现抽象设计（综合最佳方案）

> **分类**: Kernel 架构抽象 / 硬件发现
> **状态**: 已定稿（多 AI bagging 汇总后择优融合）
> **关联问题**: [plat-discovery-problem.md](plat-discovery-problem.md)
> **关联文档**: `00-kernel-overview.md`、`01-boot-shim-bootstrap.md`、`03-kmain-cstart.md`、`04-clock-interrupt-init.md`
> **关联源码**:
> - `os/arch/src/riscv64/clock.rs`、`os/arch/src/arm64/clock.rs`、`os/arch/src/x86_64/clock.rs`
> - `os/plat/src/riscv64/interrupt.rs`、`os/plat/src/arm64/interrupt.rs`、`os/plat/src/x86_64/interrupt.rs`
> - `os/arch/src/x86_64/arch_init.rs`、`os/arch/src/arch/arch_init.rs`、`os/arch/src/arch/clock.rs`
> - `os/libs/minix-boot/src/kernel_info.rs`
> - `minix3/minix/kernel/arch/i386/arch_system.c:246-288`（C Ground Truth）

---

## 0. 核心决策摘要

| # | 决策项 | 选择 | 来源 | 一句话理由 |
|---|--------|------|------|-----------|
| D1 | 归属 | **方案 C**：boot-shim 定位原始指针，kernel 解析 | 全员一致 | 职责边界清晰；对齐 C 的 `acpi_init()`/`bsp_init()` 在 kernel 内的语义 |
| D2 | 命名 | **`PlatformDesc`** | DS/GLM/Qwen/M3/Seed | 通用、与 Linux "platform" 概念对齐、不暗示来源 |
| D3 | trait 形状 | **根 trait + enum 值类型子描述符** | GLM | 类型安全（模式匹配）；接口隔离；消费者只需自己关心的子描述符 |
| D4 | 硬件 trait | **实例化**：`ClockArch`/`ArchInit` 从静态 trait 改为 `Sized` + `new(desc) -> Self` | GLM | `read_ticks(&self)` 直接读实例字段，零间接；可测试性最高 |
| D5 | 全局存储 | **`PlatformContext` 全局**（拥有描述符 + 硬件实例），`static mut Option<PlatformContext>` | GLM | 具体类型静态分发，热路径零开销；boot 阶段单线程写入后只读 |
| D6 | KernelInfo 扩展 | **1 个字段**：`Option<PlatformDescriptorPtr>`（enum: `Dtb`/`Rsdp`） | GLM | 最小扩展；语义清晰；前向兼容 |
| D7 | 解析时机 | **T2.5**：`prot_init()` 之后、`init_clock()` 之前 | GLM/M3 | 越早越好；后续所有硬件初始化都依赖它 |
| D8 | 错误处理 | **dev 构建 warn-and-fallback** / **release 构建 panic** | M3 | 兼顾开发体验和生产安全性；不静默降级 |
| D9 | QEMU 兜底 | **`QemuVirtDesc` 显式变体** | 全员一致 | 测试不中断；`#[cfg(target_arch)]` 仅做数据选择非行为选择 |
| D10 | 多核扩展 | **per-CPU 数组**：`CpuTopology.cpus[]` + `gicr_stride`/`mtimecmp_stride` | DS/M3 | 为 SMP 阶段预留；首期实现单核 |
| D11 | 解析器 | **`fdt` crate（no_std）+ 自研最小 ACPI** | KI/M3 | 平衡外部依赖与工作量；FDT 节点遍历交给成熟 crate |
| D12 | COM1 基址 | **保留为架构常量**（`0x3F8`） | M3/KI | IBM PC 兼容机 well-known 端口；不属于"平台发现"语义 |
| D13 | 新 crate | **`os/libs/minix-platform`** | M3/KI | 关注点分离：`minix-boot` 管交接，`minix-platform` 管硬件描述 |

---

## 1. 归属决策：方案 C（混合），kernel 侧解析

### 1.1 选择

**boot-shim 定位原始 DTB/RSDP 物理指针 → `KernelInfo` 携带 → kernel 解析。**

### 1.2 理由

1. **职责边界清晰**：boot-shim 的本职是"固件交互 + ELF 加载 + ExitBootServices"（见 `os/boot-shim/src/main.rs`、`01-boot-shim-bootstrap.md`）。把 FDT/ACPI 解析塞进去会让它变成"第二个 BSP"——这正是问题文档 §5.1 列出的代价。

2. **与 Minix3 C 语义对齐**（Ground Truth）：C 版的 `acpi_init()`（`minix3/minix/kernel/arch/i386/arch_system.c:246-288`）、`bsp_init()`（`earm/arch_system.c:101-132`）都是 **kernel 内部**调用。虽然 C 版没有 device tree，但"kernel 自己理解硬件"是微内核的职责边界。

3. **`KernelInfo` 保持精简**：方案 A（boot-shim 解析成结构化数据）要求 `KernelInfo` 扩展 GICD/GICR/PLIC/CLINT 等十几个字段，变成大而全结构体。方案 C 只需加 **1 个字段**。

4. **boot-shim 可替换性**：kernel 从原始 DTB/RSDP 解析，意味着未来换 bootloader（直接从 firmware 启动、或被 GRUB/U-Boot 直接加载）不需要改 kernel。

5. **可测试性**：kernel 侧解析器可在 `#[cfg(test)]` 中用 mock DTB/ACPI 字节流测试，不需要启动 QEMU。

### 1.3 职责边界

```
┌─────────────────────────────────────────────────────┐
│ boot-shim 职责（UEFI/OpenSBI 环境）                   │
│   1. 获取 UEFI 内存映射 → KernelInfo.memmap[]        │
│   2. 加载 kernel ELF + boot modules                  │
│   3. 定位 DTB / RSDP 物理地址 → KernelInfo 字段       │
│   4. 退出固件服务（ExitBootServices）                 │
│   ⛔ 不做：解析 DTB/ACPI、理解硬件拓扑                  │
└─────────────────────────────────────────────────────┘
        │ 传递：KernelInfo（含 memmap + DTB/RSDP 指针）
        ▼
┌─────────────────────────────────────────────────────┐
│ kernel 职责（no_std，T2.5 阶段）                       │
│   1. 根据 DTB/RSDP 指针，选择合适的解析器              │
│   2. 解析 → 构造 PlatformDesc → 构造硬件实例           │
│   3. 存入全局 PlatformContext，供 trait 实现读取       │
│   ⛔ 不做：在 trait 实现中直接访问 FDT/ACPI 原始数据    │
└─────────────────────────────────────────────────────┘
```

### 1.4 boot-shim 侧的定位逻辑（最小改动）

| 架构 | 固件 | 指针来源 | boot-shim 改动 |
|------|------|---------|---------------|
| x86-64 | UEFI | EFI Configuration Table 的 `EFI_ACPI_TABLE_GUID` 条目 | `uefi_helpers` 增加一行：从 `boot::config_table` 找 ACPI GUID → 填 `PlatformDescriptorPtr::Rsdp` |
| aarch64 | UEFI | EFI Configuration Table 的 `EFI_ACPI_TABLE_GUID`（服务器）或 `EFI_DEVICE_TREE_GUID`（嵌入式） | 按存在性选 Rsdp 或 Dtb |
| riscv64 | OpenSBI + U-Boot | U-Boot/OpenSBI 在 `a1` 传 DTB 物理地址（RISC-V SBI boot 协议） | `opensbi_helpers` 入口 trampoline 额外保存 `a1` → 填 `PlatformDescriptorPtr::Dtb` |

> **待确认（to confirm）**：当前 `os/boot-shim/src/opensbi_helpers.rs` 的入口只保存了 `a0`（BootFileTable）。RISC-V SBI boot 协议下 `a1` 是否确实是 DTB 物理地址，需在实施时验证 OpenSBI 版本约定。若 U-Boot 未传 DTB，则 riscv64 QEMU 路径走 `QemuVirtDesc` 兜底（与现状一致）。

---

## 2. 命名决策：`PlatformDesc`

选择 `PlatformDesc` 而非 `MachineDesc` / `BoardDesc` / `HardwareTopology`：

| 候选 | 否决理由 |
|------|---------|
| `MachineDesc` | 与 QEMU "machine" 概念混淆；暗示特定机型 |
| `BoardDesc` | 直接对应 BSP（Board Support Package），而我们正要摆脱 BSP 模式（问题文档 §4.2） |
| `HardwareTopology` | 过宽——CPU cache 拓扑、NUMA 都算 hardware topology，但本抽象只管"启动期需要的硬件参数" |

`PlatformDesc` 中性、与 Linux "platform" 概念对齐、不暗示来源（DTB/ACPI/硬编码都算 platform description）。

---

## 3. API 设计

### 3.1 设计原则

1. **根 trait + enum 值类型子描述符**：`PlatformDesc` 是 trait（支持 `&dyn` 多态和 mock）；子描述符（`InterruptControllerDesc`、`TimerDesc` 等）是 **enum**（类型安全，模式匹配）。
2. **硬件 trait 实例化**：`ClockArch`、`ArchInit` 从静态 trait 改为实例 trait（`fn new(desc) -> Self` + `&self`/`&mut self` 方法）。实例字段持有从 `PlatformDesc` 解析出的地址，热路径零间接。
3. **`no_std` 兼容**：不使用 `Box`、`Vec`、`String`（解析产物存入 `static mut`）。固定大小数组 + 枚举分发。
4. **`Send + Sync`**：所有配置数据在构造后不可变，满足 SMP 并发约束。

### 3.2 新增 crate：`os/libs/minix-platform`

```
os/libs/minix-platform/
├── Cargo.toml
└── src/
    ├── lib.rs          # re-export + 平台入口
    ├── desc.rs         # PlatformDesc trait + enum 子描述符
    ├── device_tree.rs  # FDT 解析器（使用 fdt crate）
    ├── acpi.rs         # ACPI 解析器（自研最小化）
    ├── qemu_virt.rs    # QemuVirtDesc fallback
    └── global.rs       # PlatformContext 全局 + init 函数
```

**依赖关系**：
- 依赖 `minix-boot`（用 `KernelInfo` 拿到 `PlatformDescriptorPtr`）。
- 依赖 `minix-types`（`PhysBytes` 等）。
- 不依赖 `minix-arch` 或 `minix-plat`（保持单向）。
- 被 `os/kernel` 在 `cstart` 中调用。

### 3.3 根 trait

```rust
// os/libs/minix-platform/src/desc.rs

/// 平台硬件描述的统一抽象。
///
/// 一个 `PlatformDesc` 实例回答："我跑在什么硬件上？"。
/// 上层（ClockArch / InterruptController / ArchInit）只读这个抽象，
/// 不接触 FDT/ACPI 原始字节。
///
/// 实现必须 `Send + Sync`（BKL 释放窗口内可被其他 CPU 访问）。
pub trait PlatformDesc: Send + Sync + core::fmt::Debug {
    /// 中断控制器描述符。
    fn interrupt_controller(&self) -> InterruptControllerDesc;
    /// 定时器描述符。
    fn timer(&self) -> TimerDesc;
    /// 早期控制台描述符（可选）。
    fn early_console(&self) -> Option<ConsoleDesc>;
    /// CPU 拓扑。
    fn cpu_topology(&self) -> CpuTopology;
    /// 架构杂项（ACPI 表地址、PMU 等）。
    fn arch_misc(&self) -> ArchMiscDesc;
    /// 平台来源标识（用于调试和日志）。
    fn source(&self) -> PlatformSource;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSource {
    DeviceTree,
    Acpi,
    QemuVirt,
}
```

### 3.4 enum 子描述符

```rust
/// 中断控制器描述符。
#[derive(Debug, Clone, Copy)]
pub enum InterruptControllerDesc {
    /// x86-64 LAPIC + IOAPIC。
    Apic {
        lapic_base: usize,
        ioapic_base: usize,
        nr_irqs: u32,
    },
    /// ARM64 GICv3。
    Gicv3 {
        gicd_base: usize,
        gicr_base: usize,
        /// Redistributor 间距（多核预留，首期单核可忽略）。
        gicr_stride: usize,
        nr_irqs: u32,
    },
    /// RISC-V PLIC。
    Plic {
        plic_base: usize,
        nr_irqs: u32,
        /// S-mode context ID（首期 hart 0 = 1）。
        context: u32,
    },
}

/// 定时器描述符。
#[derive(Debug, Clone, Copy)]
pub enum TimerDesc {
    /// x86-64 8254 PIT（boot 期）+ LAPIC Timer（运行期）。
    Pit {
        pit_base_freq: u32,
        lapic_base: usize,
    },
    /// ARM64 Generic Timer。
    /// 频率由固件写入 CNTFRQ_EL0，kernel 直接 mrs 读取，故此处不需频率字段。
    ArmGenericTimer,
    /// RISC-V CLINT mtime。
    Clint {
        mtime_addr: usize,
        mtimecmp_base: usize,
        /// per-hart mtimecmp 间距（多核预留）。
        mtimecmp_stride: usize,
        freq: u64,
    },
}

/// 早期控制台描述符。
#[derive(Debug, Clone, Copy)]
pub enum ConsoleDesc {
    /// x86-64 COM1 等 ISA 串口（端口 I/O）。
    IsaSerial { port_base: u16 },
    /// MMIO UART（ARM PL011 等）。
    MmioSerial { mmio_base: usize },
    /// RISC-V SBI console（无 MMIO，走 ecall）。
    SbiConsole,
}

/// CPU 拓扑（多核预留，首期单核）。
#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    /// 系统 CPU/hart 总数。
    pub nr_cpus: u32,
    /// 当前 CPU（BSP）的硬件 ID。
    pub bsp_id: u32,
    /// 每个 CPU 的信息。索引 = 逻辑 CPU 编号。
    /// 仅 `cpus[0..nr_cpus]` 有效。MAX_CPUS 可在编译时调整。
    pub cpus: [CpuInfo; MAX_CPUS],
}

/// 系统最大 CPU 数。可在编译时通过 feature 调整。
pub const MAX_CPUS: usize = 64;

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuInfo {
    /// 架构特定硬件 ID：
    /// - x86-64: APIC ID
    /// - ARM64: MPIDR_EL1 的 Aff0
    /// - RISC-V: hart ID
    pub hw_id: u64,
    /// ARM64 GICv3: 此 CPU 的 Redistributor 基址。
    /// 计算方式：gicr_base + hw_id * gicr_stride（通常 0x20000）。
    pub gicr_base: Option<usize>,
    /// RISC-V CLINT: 此 hart 的 mtimecmp 地址。
    /// 计算方式：CLINT mtimecmp 基址 + hart_id * mtimecmp_stride。
    pub mtimecmp_addr: Option<usize>,
}

/// 架构杂项描述符（ACPI 表、PMU 等 ArchInit 需要的信息）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchMiscDesc {
    /// x86-64: ACPI 表物理地址（若未通过 RSDP 路径提供）。
    pub acpi_tables: Option<usize>,
    /// 是否启用 PMU cycle counter（ARM64）。
    pub pmu_cycle_counter: bool,
}
```

### 3.5 为什么子描述符用 enum 而非 trait

- 子描述符的形状是**有限且已知**的（中断控制器就那几种：GICv3/PLIC/APIC；定时器就那几种）。用 `enum` 表达"有限已知集合"是 Rust idiomatic 做法。
- 值类型可以 `Copy`/`Clone`，便于在 init 阶段传给各硬件 trait。
- 模式匹配让类型安全在编译期保证（`match desc { InterruptControllerDesc::Plic { plic_base, .. } => ... }`）。
- 消费者（`init_clock_and_interrupts`）只需从根 trait 获取子描述符，不需要 N 个独立 trait 对象。

### 3.6 硬件 trait 实例化

**核心变更**：`ClockArch`、`ArchInit` 从静态 trait 改为实例 trait。`InterruptController` 已经是实例 trait，只需把 `new()` + `set_base()` 合并为 `new(desc)`。

```rust
// os/arch/src/arch/clock.rs

/// 时钟架构 trait（实例化）。
///
/// 变更：从静态 trait（`fn init_timer(hz)`）改为实例 trait。
/// 实例字段持有从 PlatformDesc 解析出的地址，read_ticks 直接读字段——零额外间接。
pub trait ClockArch: Sized + Send + Sync {
    /// 从定时器描述符构造实例。
    fn new(desc: &TimerDesc) -> Self;
    /// 初始化硬件定时器。
    fn init_timer(&mut self, hz: u32);
    /// 读取当前 tick 值。
    fn read_ticks(&self) -> u64;
    /// 读取 TSC（默认委托 read_ticks）。
    fn read_tsc(&self) -> u64 { self.read_ticks() }
}
```

```rust
// os/plat/src/interrupt.rs

/// 中断控制器 trait（实例化）。
///
/// 变更：new() 从无参改为接收 desc；删除 set_base()。
pub trait InterruptController: Sized + Send + Sync {
    /// 从中断控制器描述符构造实例。
    fn new(desc: &InterruptControllerDesc) -> Self;
    /// 初始化中断控制器（配置路由、屏蔽所有 IRQ）。
    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}
```

```rust
// os/arch/src/arch/arch_init.rs

/// 架构初始化 trait（实例化）。
pub trait ArchInit: Sized + Send + Sync {
    /// 从平台描述构造实例。
    fn new(platform: &dyn PlatformDesc) -> Self;
    /// 执行架构特定初始化。
    fn init(&mut self);
}
```

**为什么实例化（而非保持静态 + 传 desc 参数）**：

当前 `ClockArch` 是静态 trait：

```rust
// 现状
pub trait ClockArch {
    fn init_timer(hz: u32);    // 静态
    fn read_ticks() -> u64;    // 静态
}
```

问题：`read_ticks()` 在运行时中断上下文被调用，它需要知道 CLINT mtime 的地址。如果地址来自 `PlatformDesc`，静态方法要么：
- (a) 每次调用都传 `&PlatformDesc` —— 污染所有调用点；
- (b) 读一个全局 static —— 那还不如实例化，让实例自己持有地址。

实例化后：

```rust
// riscv64 实现
pub struct Riscv64ClockArch {
    mtime_addr: usize,
    mtimecmp_base: usize,
    freq: u64,
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &TimerDesc) -> Self {
        match desc {
            TimerDesc::Clint { mtime_addr, mtimecmp_base, freq, .. } => Self {
                mtime_addr: *mtime_addr,
                mtimecmp_base: *mtimecmp_base,
                freq: *freq,
            },
            _ => panic!("Expected Clint timer desc for RISC-V"),
        }
    }

    fn init_timer(&mut self, hz: u32) {
        let mtime: u64 = unsafe { core::ptr::read_volatile(self.mtime_addr as *const u64) };
        let interval = self.freq / hz as u64;
        unsafe { core::ptr::write_volatile(self.mtimecmp_base as *mut u64, mtime + interval) };
        // Enable S-mode timer interrupt (STIE)
        unsafe { core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64) };
    }

    fn read_ticks(&self) -> u64 {
        unsafe { core::ptr::read_volatile(self.mtime_addr as *const u64) }
    }
}
```

实例字段持有从 `PlatformDesc` 解析出的地址，`read_ticks` 直接读字段——零额外间接（编译器可把字段 load 提到循环外）。

### 3.7 PlatformContext 全局

```rust
// os/libs/minix-platform/src/global.rs

use core::cell::UnsafeCell;
use crate::desc::PlatformDesc;

/// 全局平台上下文——拥有描述符和硬件实例。
///
/// 存放在 kernel BSS 的一个 static，在 T2.5（kmain 早期、BKL 持有、IRQ 关闭）
/// 写入一次，之后只读。
///
/// 为什么用具体类型而非 `&'static dyn PlatformDesc`？
/// - 具体类型方法调用静态分发，热路径零开销
/// - `PlatformContext` 拥有描述符和硬件实例，封装更好
/// - `&'static dyn PlatformDesc` 要求描述符本身 `'static`，解析产物若放在
///   bootstrap 内存会被回收（KernelInfo.bootstrap_start/len 在 T9 回收）
static PLATFORM: UnsafeCell<Option<PlatformContext>> = UnsafeCell::new(None);

/// 平台上下文——拥有描述符和硬件实例。
///
/// 字段全是 usize/u64/enum（无裸指针字段——指针在方法内从 usize 现算），
/// 因此自动 Send + Sync。
pub struct PlatformContext {
    /// 平台描述符（解析产物或 QEMU 兜底）。
    pub desc: PlatformDescEnum,
}

/// 平台描述分发枚举——编译时确定所有可能变体，运行时选择。
///
/// 为什么不直接用 `Box<dyn PlatformDesc>`？
/// - `no_std` 下没有 alloc
/// - 枚举分发是零成本抽象（无 vtable 间接调用）
pub enum PlatformDescEnum {
    DeviceTree(DeviceTreeDesc),
    Acpi(AcpiDesc),
    QemuVirt(QemuVirtDesc),
}

impl PlatformDesc for PlatformDescEnum {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        match self {
            Self::DeviceTree(d) => d.interrupt_controller(),
            Self::Acpi(d) => d.interrupt_controller(),
            Self::QemuVirt(d) => d.interrupt_controller(),
        }
    }
    fn timer(&self) -> TimerDesc {
        match self {
            Self::DeviceTree(d) => d.timer(),
            Self::Acpi(d) => d.timer(),
            Self::QemuVirt(d) => d.timer(),
        }
    }
    fn early_console(&self) -> Option<ConsoleDesc> {
        match self {
            Self::DeviceTree(d) => d.early_console(),
            Self::Acpi(d) => d.early_console(),
            Self::QemuVirt(d) => d.early_console(),
        }
    }
    fn cpu_topology(&self) -> CpuTopology {
        match self {
            Self::DeviceTree(d) => d.cpu_topology(),
            Self::Acpi(d) => d.cpu_topology(),
            Self::QemuVirt(d) => d.cpu_topology(),
        }
    }
    fn arch_misc(&self) -> ArchMiscDesc {
        match self {
            Self::DeviceTree(d) => d.arch_misc(),
            Self::Acpi(d) => d.arch_misc(),
            Self::QemuVirt(d) => d.arch_misc(),
        }
    }
    fn source(&self) -> PlatformSource {
        match self {
            Self::DeviceTree(_) => PlatformSource::DeviceTree,
            Self::Acpi(_) => PlatformSource::Acpi,
            Self::QemuVirt(_) => PlatformSource::QemuVirt,
        }
    }
}

/// 初始化全局平台上下文。在 T2.5 阶段调用。
///
/// # Safety
/// 只在 boot 阶段单线程调用一次。之后所有 CPU 只读访问。
pub unsafe fn init(desc: PlatformDescEnum) {
    *PLATFORM.get() = Some(PlatformContext { desc });
}

/// 获取平台描述引用。
///
/// # Panics
/// 在 `init()` 之前调用会 panic。
pub fn platform_desc() -> &'static dyn PlatformDesc {
    // SAFETY: boot 阶段写入后不再修改；启动后多 CPU 只读访问安全。
    unsafe {
        (*PLATFORM.get())
            .as_ref()
            .expect("platform_desc() called before init()")
            .desc.as_ref() // 返回 &PlatformDescEnum，自动 coerce 到 &dyn PlatformDesc
    }
}
```

**为什么用 `UnsafeCell<Option<...>>` 而非 `static mut`？**
- `static mut` 在 Rust 2024 edition 中访问需要 `unsafe`，但没有提供额外安全性。
- `UnsafeCell` 明确表达了"内部可变性"的语义，且 `&` 引用在写入后是安全的（不再修改）。
- 不需要同步原语（`Mutex`/`OnceLock`），因为 boot 阶段是单线程的。

---

## 4. KernelInfo 扩展

### 4.1 新增字段

```rust
// os/libs/minix-boot/src/kernel_info.rs — 在现有字段后追加

pub struct KernelInfo {
    // ... 现有字段保持不变 ...

    /// 平台描述符原始指针（DTB 或 RSDP 的物理地址）。
    /// `None` 表示 boot-shim 未提供（QEMU virt 兜底路径）。
    pub platform_descriptor: Option<PlatformDescriptorPtr>,
}
```

```rust
/// 平台描述符原始指针类型。
#[derive(Debug, Clone, Copy)]
pub enum PlatformDescriptorPtr {
    /// Flattened Device Tree 物理地址（ARM64/RISC-V）。
    Dtb(PhysBytes),
    /// ACPI RSDP 物理地址（x86-64）。
    Rsdp(PhysBytes),
}
```

### 4.2 为什么用一个字段而非两个字段（dtb_phys + rsdp_phys）

- DTB 和 RSDP 是两种**不同格式**的数据。boot-shim 在定位时已经知道它找到的是什么。
- 用一个 `enum` 字段语义清晰：`Dtb` 走 DTB 路径，`Rsdp` 走 ACPI 路径。
- 两者同时存在是合法的（某些 x86 嵌入式场景），kernel 自行决定优先级——此时可扩展为 `Vec` 或优先级字段，但首期不需要。

---

## 5. QemuVirtDesc 兜底实现

### 5.1 设计理由

QEMU `virt` 机器的硬件布局是固定的，且 QEMU 测试不需要 DTB 解析器。为保持测试快速通过，提供 `QemuVirtDesc` 实现。

### 5.2 实现

```rust
// os/libs/minix-platform/src/qemu_virt.rs

/// QEMU virt 硬编码兜底（保持现有测试通过）。
pub struct QemuVirtDesc;

impl PlatformDesc for QemuVirtDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        cfg_match! {
            target_arch = "riscv64" => InterruptControllerDesc::Plic {
                plic_base: 0x0C00_0000,  // 现状常量，见 os/plat/src/riscv64/interrupt.rs:11
                nr_irqs: 64,
                context: 1,
            },
            target_arch = "aarch64" => InterruptControllerDesc::Gicv3 {
                gicd_base: 0x0800_0000,
                gicr_base: 0x080A_0000,
                gicr_stride: 0x2_0000,
                nr_irqs: 64,
            },
            target_arch = "x86_64" => InterruptControllerDesc::Apic {
                lapic_base: 0xFEE0_0000,   // 现状 DEFAULT_LAPIC_BASE
                ioapic_base: 0xFEC0_0000,  // 现状 DEFAULT_IOAPIC_BASE
                nr_irqs: 64,
            },
        }
    }

    fn timer(&self) -> TimerDesc {
        cfg_match! {
            target_arch = "riscv64" => TimerDesc::Clint {
                mtime_addr: 0x200_BFF8,        // 现状 CLINT_MTIME
                mtimecmp_base: 0x200_4000,     // 现状 CLINT_MTIMECMP
                mtimecmp_stride: 8,
                freq: 10_000_000,              // 现状 MTIME_FREQ
            },
            target_arch = "aarch64" => TimerDesc::ArmGenericTimer,
            target_arch = "x86_64" => TimerDesc::Pit {
                pit_base_freq: 1_193_182,
                lapic_base: 0xFEE0_0000,
            },
        }
    }

    fn early_console(&self) -> Option<ConsoleDesc> {
        cfg_match! {
            target_arch = "x86_64" => Some(ConsoleDesc::IsaSerial { port_base: 0x3F8 }),
            target_arch = "aarch64" => Some(ConsoleDesc::MmioSerial { mmio_base: 0x0900_0000 }),
            target_arch = "riscv64" => Some(ConsoleDesc::SbiConsole),
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        let mut cpus = [CpuInfo::default(); MAX_CPUS];
        cpus[0] = CpuInfo {
            hw_id: 0,
            gicr_base: cfg_match!(target_arch = "aarch64" => Some(0x080A_0000), _ => None),
            mtimecmp_addr: cfg_match!(target_arch = "riscv64" => Some(0x200_4000), _ => None),
        };
        CpuTopology { nr_cpus: 1, bsp_id: 0, cpus }
    }

    fn arch_misc(&self) -> ArchMiscDesc { ArchMiscDesc::default() }
    fn source(&self) -> PlatformSource { PlatformSource::QemuVirt }
}
```

> **注意**：`QemuVirtDesc` 里的 `cfg_match!`（或 `#[cfg(target_arch)]`）仅用于"选择该架构的 QEMU virt 固定值"，是**数据选择**而非**行为选择**。问题文档 §6.1.1 禁止的是"`#[cfg(target_arch)]` 用于行为选择"（如 `if x86 { foo() } else { bar() }`）。这里每个分支只返回常量数据，且 `QemuVirtDesc` 本身是一个具体实现类型，不污染上层 trait。真实硬件路径走 `DeviceTreeDesc`/`AcpiDesc`，完全无 `#[cfg]`。

---

## 6. 解析器设计

### 6.1 FDT 解析器（ARM64 / RISC-V）

使用 `fdt` crate（`#![no_std]` 兼容，纯 Rust 实现），避免自行实现完整 FDT 解析器。

```rust
// os/libs/minix-platform/src/device_tree.rs

use crate::desc::*;

/// Device Tree 来源实现。
pub struct DeviceTreeDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpus: CpuTopology,
    misc: ArchMiscDesc,
}

impl DeviceTreeDesc {
    /// 从 DTB 物理地址解析。
    ///
    /// # Safety
    /// `dtb_phys` 必须是有效的 DTB 物理地址，且在内核可映射范围内。
    pub unsafe fn parse(dtb_phys: PhysBytes) -> Result<Self, ParseError> {
        let fdt = fdt::Fdt::from_ptr(dtb_phys.as_ptr())?;

        // 1. 解析定时器
        let timer = parse_timer(&fdt)?;

        // 2. 解析中断控制器
        let ic = parse_interrupt_controller(&fdt)?;

        // 3. 解析 CPU 拓扑
        let cpus = parse_cpus(&fdt);

        // 4. 解析串口（可选）
        let console = parse_console(&fdt);

        Ok(Self { ic, timer, console, cpus, misc: ArchMiscDesc::default() })
    }
}

impl PlatformDesc for DeviceTreeDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc { self.ic }
    fn timer(&self) -> TimerDesc { self.timer }
    fn early_console(&self) -> Option<ConsoleDesc> { self.console }
    fn cpu_topology(&self) -> CpuTopology { self.cpus }
    fn arch_misc(&self) -> ArchMiscDesc { self.misc }
    fn source(&self) -> PlatformSource { PlatformSource::DeviceTree }
}
```

### 6.2 ACPI 解析器（x86-64）

自研最小化 ACPI 解析器（RSDP → XSDT → MADT），仅提取 IOAPIC/LAPIC 基址。

```rust
// os/libs/minix-platform/src/acpi.rs

/// ACPI 来源实现（x86-64）。
pub struct AcpiDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpus: CpuTopology,
    misc: ArchMiscDesc,
}

impl AcpiDesc {
    /// 从 RSDP 物理地址解析。
    ///
    /// 真实实现位于 `os/libs/minix-platform/src/acpi.rs`；此处伪代码描述
    /// Phase 4 的解析步骤。
    pub unsafe fn parse(rsdp_phys: PhysBytes) -> Result<Self, ParseError> {
        // 1. 验证 RSDP 签名 ("RSD PTR ")
        // 2. 解析 XSDT → 找到 MADT
        // 3. 从 MADT 提取 IOAPIC base + LAPIC base
        // 4. 从 HPET 提取定时器基址（如果有）
        // 实际实现参见 acpi.rs 的 `AcpiDesc::parse()`。
        unimplemented!("see os/libs/minix-platform/src/acpi.rs")
    }
}
```

### 6.3 kernel 侧初始化

```rust
// os/libs/minix-platform/src/global.rs

/// 根据 KernelInfo 构造 PlatformDesc 并初始化全局。
pub fn init_from_kinfo(kinfo: &KernelInfo) {
    let desc = match kinfo.platform_descriptor {
        Some(PlatformDescriptorPtr::Dtb(pa)) => {
            // SAFETY: boot-shim 保证 dtb 是有效的物理地址。
            let dt = unsafe { DeviceTreeDesc::parse(pa) }
                .expect("FDT parse failed");
            PlatformDescEnum::DeviceTree(dt)
        }
        Some(PlatformDescriptorPtr::Rsdp(pa)) => {
            // SAFETY: boot-shim 保证 rsdp 是有效的物理地址。
            let acpi = unsafe { AcpiDesc::parse(pa) }
                .expect("ACPI parse failed");
            PlatformDescEnum::Acpi(acpi)
        }
        None => {
            // 无指针：QEMU virt 兜底
            if cfg!(debug_assertions) {
                // dev 构建：warn-and-fallback
                log::warn!("No platform descriptor provided, falling back to QemuVirtDesc");
                PlatformDescEnum::QemuVirt(QemuVirtDesc)
            } else {
                // release 构建：panic
                panic!("No platform descriptor provided and not a dev build");
            }
        }
    };
    // SAFETY: T2.5 阶段单线程调用。
    unsafe { init(desc) };
}
```

---

## 7. 启动时序

```
cstart()
  │
  ├── prot_init()                    ← GDT/TSS/stvec 已就绪
  │
  ├── platform::init_from_kinfo(&kinfo)   ← 新增（T2.5）
  │     └── 解析 DTB/RSDP → 构造 PlatformDescEnum → 写入全局 PLATFORM
  │
  ├── init_clock()
  │     └── let timer_desc = platform_desc().timer();
  │     └── let mut clock = CurrentClockArch::new(&timer_desc);
  │     └── clock.init_timer(hz);
  │
  ├── intr_init()
  │     └── let ic_desc = platform_desc().interrupt_controller();
  │     └── let mut intr = CurrentInterruptController::new(&ic_desc);
  │     └── intr.init();
  │
  └── arch_init()
        └── let mut arch = CurrentArchInit::new(platform_desc());
        └── arch.init();
```

**顺序要点**：
- `platform::init_from_kinfo` 必须在 `init_clock` 和 `intr_init` **之前**完成。
- 与 C 行为的兼容性：C 的 `acpi_init` 在 `arch_init` 中调用，**晚于** `init_clock` 和 `intr_init`。这是因为 C 的 `acpi_init` 解析的是 ACPI 表，**不**用于驱动时钟和中断控制器（那些依赖硬编码）。但 Rust 重写要"从解析中获取时钟/中断的基址"，所以顺序必须前移。
- 这是与 C 的**有意偏离**（在文档中显式标注 "arch scope: rust-rewrite"），理由是 C 的硬编码本身就是要被替换的。

---

## 8. 当前硬编码迁移路径

### 8.1 RISC-V 64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| `CLINT_MTIME` `0x200_BFF8` | `os/arch/src/riscv64/clock.rs:13` | `TimerDesc::Clint { mtime_addr }` |
| `CLINT_MTIMECMP` `0x200_4000` | `os/arch/src/riscv64/clock.rs:17` | `TimerDesc::Clint { mtimecmp_base }` |
| `MTIME_FREQ` `10_000_000` | `os/arch/src/riscv64/clock.rs:21` | `TimerDesc::Clint { freq }` |
| `PLIC_BASE` `0x0C00_0000` | `os/plat/src/riscv64/interrupt.rs:9` | `InterruptControllerDesc::Plic { plic_base }` |

**迁移后**：`Riscv64ClockArch::new(desc)` 从 `TimerDesc::Clint` 提取地址存入实例字段。`Riscv64InterruptController::new(desc)` 从 `InterruptControllerDesc::Plic` 提取 base。`set_base()` 删除。

### 8.2 ARM64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| GICD base `0`（需外部注入） | `os/plat/src/arm64/interrupt.rs:43` | `InterruptControllerDesc::Gicv3 { gicd_base }` |
| GICR base `0`（需外部注入） | `os/plat/src/arm64/interrupt.rs:44` | `InterruptControllerDesc::Gicv3 { gicr_base }` |
| `GICR_OFFSET` `0x000A_0000` | `os/plat/src/arm64/interrupt.rs:13` | 保留在 `AArch64InterruptController` 内部作为 GICv3 寄存器布局常量；base 来自 desc |

**迁移后**：`AArch64InterruptController::new(desc)` 从 `InterruptControllerDesc::Gicv3` 提取 base。`set_base()` 删除。

### 8.3 x86-64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| `DEFAULT_LAPIC_BASE` `0xFEE0_0000` | `os/plat/src/x86_64/interrupt.rs:64` | 保留为 fallback；`init_lapic()` 优先读取 MSR `IA32_APIC_BASE`，其次使用 desc 提供的值 |
| `DEFAULT_IOAPIC_BASE` `0xFEC0_0000` | `os/plat/src/x86_64/interrupt.rs:66` | `InterruptControllerDesc::Apic { ioapic_base }` |
| PIT ports `0x40/0x43` | `os/arch/src/x86_64/clock.rs:17-20` | **保持常量**：PC/UEFI 兼容机上 8254 PIT I/O 端口是架构常量 |
| ACPI 未实现 | `os/arch/src/x86_64/arch_init.rs:32-39` | 通过 `AcpiDesc` 提供 RSDP → MADT → IOAPIC base；当前先 stub |

### 8.4 串口

- x86 COM1 `0x3F8` 是 PC 架构常量，可继续留在 `X86_64EarlyConsole` 中，不纳入 `PlatformDesc`。
- ARM PL011 / RISC-V SBI 的基址若未来需要真实板子支持，再纳入 `ConsoleDesc`。

---

## 9. 多核扩展

`PlatformDesc` 的子结构支持多核信息：

- `CpuTopology`：CPU 数量、每个 CPU 的 `CpuInfo`（hw_id、gicr_base、mtimecmp_addr）
- `InterruptControllerDesc::Gicv3`：`gicr_stride` 用于计算 per-CPU Redistributor 地址
- `TimerDesc::Clint`：`mtimecmp_stride` 用于计算 per-hart mtimecmp 地址

**当前阶段**：只实现单核（hart 0 / BSP），但接口设计预留多核扩展。SMP 启动后，每个 CPU 通过 `CpuTopology.cpus[cpu_id]` 查询自己的私有信息。

---

## 10. no_std 约束

- `minix-platform` crate 标注 `#![no_std]`。
- FDT 解析使用 `fdt` crate（`#![no_std]` 兼容，纯 Rust 实现）。
- ACPI 解析自研最小化（RSDP → XSDT → MADT），不使用 `std` 集合或分配器。
- 解析结果存储在 `static mut Option<PlatformContext>` 中（一次性写入，不释放）。
- `PlatformDescEnum` 是枚举（无堆分配），`DeviceTreeDesc`/`AcpiDesc` 的字段是固定大小（`CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组）。

---

## 11. 测试策略

| 测试层级 | 方法 |
|---------|------|
| **单元测试** | 构造 `PlatformDescEnum`（`QemuVirtDesc` 或 `DeviceTreeDesc`），验证 `Riscv64ClockArch::new(desc)` 等提取的字段值正确 |
| **DTB 解析测试** | 在 `#[cfg(test)]` 中嵌入 QEMU `virt` DTB 字节数组，验证 `DeviceTreeDesc::parse` 提取的 CLINT/PLIC 地址与频率 |
| **ACPI 解析测试** | 在 `#[cfg(test)]` 中构造模拟 ACPI 表（RSDP + XSDT + MADT），验证 `AcpiDesc::parse` 提取的 IOAPIC base |
| **QEMU 集成测试** | 保持现有 `qemu_test_*.sh` 通过；`QemuVirtDesc` 接入 `init_clock_and_interrupts` |
| **架构 trait 测试** | 利用 `PlatformDescEnum` 传入不同子描述符，验证 `InterruptController::new(desc)` 与 `init()` 调用链路 |

---

## 12. 实施阶段（四阶段）

### Phase 1：基础设施（不破坏 QEMU）

1. 创建 `os/libs/minix-platform` crate，定义 `desc.rs` trait + enum。
2. 扩展 `KernelInfo` 加 `platform_descriptor` 字段（boot-shim 先填 `None`）。
3. 实现 `QemuVirtDesc`（各架构），返回当前硬编码值。
4. 修改 `ClockArch` / `InterruptController` / `ArchInit` trait 为实例化签名。
5. 各架构实现 `new(desc)`，从 `QemuVirtDesc` 获取值。
6. 修改 `cstart()` 调用 `platform::init_from_kinfo()` → `init_clock()` → `intr_init()` → `arch_init()`。
7. **验收**：QEMU `virt` 测试继续通过。

### Phase 2：boot-shim 传递指针

1. UEFI boot-shim 从系统表读取 ACPI RSDP / DTB 地址，填入 `KernelInfo.platform_descriptor`。
2. OpenSBI boot-shim 把 `a1` 寄存器的 DTB 地址填入 `KernelInfo.platform_descriptor`。
3. **验收**：`KernelInfo` 能携带 DTB/RSDP 指针（QEMU 仍走兜底）。

### Phase 3：DTB 解析器

1. 引入 `fdt` crate，实现 `DeviceTreeDesc::parse()`。
2. RISC-V / ARM64 `init_from_kinfo()` 优先解析 DTB，失败回退 `QemuVirtDesc`（dev）/ panic（release）。
3. **验收**：RISC-V/ARM64 在有 DTB 时从解析获取地址；无 DTB 时走兜底。

### Phase 4：ACPI 解析器

1. 实现最小 ACPI RSDP → XSDT → MADT 解析，提取 IOAPIC base。
2. x86-64 `AcpiDesc::parse()` 实现。
3. **验收**：x86-64 在有 RSDP 时从解析获取地址；无 RSDP 时走兜底。

### 实施状态（2026-06-20）

| Phase | 步骤 | 状态 | 实际文件 |
|-------|------|------|----------|
| Phase 1 | 1.1-1.8 | ✅ 完成 | `os/libs/minix-platform/{Cargo.toml,src/lib.rs,src/desc.rs,src/qemu_virt.rs,src/global.rs}` |
| Phase 2 | 2.1 UEFI 定位 RSDP/DTB | ✅ 完成 | `os/boot-shim/src/uefi_helpers.rs` |
| Phase 2 | 2.2 OpenSBI 保存 a1 | ✅ 完成 | `os/boot-shim/src/opensbi_helpers.rs` |
| Phase 3 | 3.1 `DeviceTreeDesc::parse()` | ✅ 完成 | `os/libs/minix-platform/src/device_tree.rs`（注：实际文件名 `device_tree.rs`，非设计稿的 `fdt.rs`） |
| Phase 3 | 3.2 RISC-V PLIC/CLINT 解析 | ✅ 完成 | 同上 |
| Phase 3 | 3.3 ARM64 GIC 解析 | ✅ 完成 | 同上 |
| Phase 3 | 3.4 `init_from_kinfo` 接入 DTB | ✅ 完成 | `os/libs/minix-platform/src/global.rs` |
| Phase 4 | 4.1 `AcpiDesc::parse()` RSDP→XSDT→MADT | ✅ 完成 | `os/libs/minix-platform/src/acpi.rs` |
| Phase 4 | 4.2 LAPIC/IOAPIC/CPU 拓扑提取 | ✅ 完成 | 同上 |
| Phase 4 | 4.3 `init_from_kinfo` 接入 ACPI | ✅ 完成 | `os/libs/minix-platform/src/global.rs` |
| Phase 4 | 4.4 x86-64 脱离硬编码（ACPI 存在时） | ✅ 完成 | `Some(Rsdp)` 分支调用 `AcpiDesc::parse`，失败才走兜底 |

**测试状态**：`cargo test -p minix-platform` 19/19 通过（含 DTB/ACPI 合成表解析测试）。

**构建状态**：`minix-platform` + `minix-kernel` + `boot-shim --features test-all` 均编译通过。

**已知偏差**：
- 文件名 `device_tree.rs` ≠ 设计稿 `fdt.rs`（实现时选择更语义化的命名，功能等价）。
- `AcpiDesc` 当前仅支持 ACPI 1.0 RSDT 和 2.0 XSDT 的 MADT 表；HPET、x2APIC、Interrupt Source Override 等扩展项暂未实现（最小化解析器，满足 QEMU `virt` x86_64 需求）。
- `test-memmap-riscv64` 编译错误为**预存在问题**（`minix_plat::riscv64` 模块缺失），与本阶段改动无关。

---

## 13. 风险与未决问题

| 风险 | 缓解 |
|------|------|
| 内核解析 DTB/ACPI 占用代码空间 | 仅 boot 阶段执行一次；解析器本身可裁剪；`QemuVirtDesc` 路径不链接解析器 |
| 解析失败导致 boot panic | dev 构建回退 QEMU virt；release 构建 panic（无 timer/intr controller 无法运行） |
| `ClockArch` 实例化改动较大 | 分阶段实施：Phase 1 先改签名，内部仍用 `QemuVirtDesc` 值；现有调用点逐步迁移 |
| `fdt` crate 依赖膨胀 | `fdt` crate 是纯 Rust no_std，体积可控；若不可接受可回退到自研最小化解析器（M3 方案） |
| ACPI 复杂度 | 先实现 MADT 提取 IOAPIC/LAPIC；其他表（SRAT/SLIT）后续按需添加 |
| DTB 物理地址映射 | T2.5 阶段内核已有恒等映射或高地址映射，可直接访问；若 DTB 在物理高端需临时映射 |

---

## 14. 验收标准

- [ ] 新增 `os/libs/minix-platform` crate，`PlatformDesc` trait + enum 子描述符定义完成。
- [ ] `KernelInfo` 新增 `platform_descriptor` 字段，boot-shim 能传递原始指针。
- [ ] `ClockArch`、`InterruptController`、`ArchInit` trait 改为实例化签名（`new(desc) -> Self`）。
- [ ] RISC-V / ARM64 / x86-64 当前硬编码常量迁移到 `QemuVirtDesc`（Phase 1）或 DTB/ACPI 解析器（后续 phase）。
- [ ] `set_base()` 方法删除，地址在 `new(desc)` 构造时注入。
- [ ] QEMU `virt` 测试继续通过。
- [ ] 所有新增代码保持 `#![no_std]`。
- [ ] `PlatformDesc` 及子描述符实现 `Send + Sync`。
- [ ] 单元测试可在不启动 QEMU 的情况下验证 `PlatformDesc` 解析与注入行为。
- [ ] 文档 `04-clock-interrupt-init.md` 和相关源码注释同步更新。

---

## 15. 参见

- `plat-discovery-problem.md` — 问题完整描述与备选方案对比
- `00-kernel-overview.md` §1.5 — 内核执行模型约束
- `01-boot-shim-bootstrap.md` — boot-shim 职责边界
- `03-kmain-cstart.md` — cstart 初始化序列
- `04-clock-interrupt-init.md` — 当前时钟/中断初始化实现
- `minix3/minix/kernel/arch/i386/arch_system.c:246-288` — C Ground Truth: `acpi_init()`
- `minix3/minix/kernel/arch/earm/arch_system.c:101-132` — C Ground Truth: `bsp_init()`

---

## 16. 实施计划拆解（逐步执行）

> §12 给出四阶段概览；本节把每个阶段拆成可独立提交的步骤，明确每步的**文档变更**、**代码变更**、**依赖**和**验收**。按步骤顺序执行，每步完成后跑 QEMU 测试确认不回归。

### 16.0 变更总览

**新增临时辅助文档**（最终清理）：

| 文档 | 阶段 | 说明 |
|------|------|------|
| `plat-design.md`（本文档） | Phase 1 | 设计定稿（临时，设计落地后清理） |
| `plat-fdt-parsing.md`（可选） | Phase 3 | FDT 解析器实现细节（若 plat-design.md §6 已够则不单独开） |
| `plat-acpi-parsing.md`（可选） | Phase 4 | ACPI 解析器实现细节 |

**新增正式文档**（`00~99` 系列）：

| 文档 | 阶段 | 说明 |
|------|------|------|
| `04-platform-discovery.md` | Phase 1 | **新增**：平台发现子系统完整教学（设计动机/方案选择/trait/三架构统一/解析器/兜底/时序）。内容从本文档（临时设计稿）转化而来。 |

**重编号**（`04~24` → `05~25`）：

| 原编号 | 新编号 | 阶段 |
|--------|--------|------|
| `04-clock-interrupt-init.md` | `05-clock-interrupt-init.md` | Phase 1 |
| `05-proc-init-boot-proc.md` | `06-proc-init-boot-proc.md` | Phase 1 |
| ... | ... | Phase 1 |
| `24-syscall-dispatch.md` | `25-syscall-dispatch.md` | Phase 1 |

> 重编号需同步更新：`00-kernel-overview.md` §3 文档导航表 + 所有文档内交叉引用（"见 04-xxx" → "见 05-xxx"）。

**修改正式文档**：

| 文档 | 阶段 | 修改内容 |
|------|------|---------|
| `01-boot-shim-bootstrap.md` | Phase 2 | 补充 boot-shim "定位 DTB/RSDP 物理地址" 职责 |
| `03-kmain-cstart.md` | Phase 1 | cstart 序列新增 `platform::init_from_kinfo()` 调用点 |
| `05-clock-interrupt-init.md`（原 04） | Phase 1 | 标注硬编码已迁移到 `QemuVirtDesc`；trait 改为实例化 |

> **注意**：`plat-design.md` 本身是临时辅助文档（与 `plat-discovery-problem.md`、`kboot-*`、`runtime-*` 同类），最终全部清理。设计决策落地为正式文档 `04-platform-discovery.md`（新增）+ 修改 `01`/`03`/`05`。`00-kernel-overview.md` §3 文档导航收录新增的 `04-platform-discovery.md`。

**新增代码**：

| 路径 | 阶段 | 说明 |
|------|------|------|
| `os/libs/minix-platform/Cargo.toml` | Phase 1 | 新 crate 清单 |
| `os/libs/minix-platform/src/lib.rs` | Phase 1 | re-export + `#![no_std]` |
| `os/libs/minix-platform/src/desc.rs` | Phase 1 | `PlatformDesc` trait + enum 子描述符 |
| `os/libs/minix-platform/src/qemu_virt.rs` | Phase 1 | `QemuVirtDesc` 三架构兜底 |
| `os/libs/minix-platform/src/global.rs` | Phase 1 | `PlatformContext` 全局 + `init_from_kinfo()` |
| `os/libs/minix-platform/src/device_tree.rs` | Phase 3 | `DeviceTreeDesc::parse()` |
| `os/libs/minix-platform/src/acpi.rs` | Phase 4 | `AcpiDesc::parse()` |

**修改代码**：

| 路径 | 阶段 | 修改内容 |
|------|------|---------|
| `os/libs/minix-boot/src/kernel_info.rs` | Phase 1 | 加 `platform_descriptor: Option<PlatformDescriptorPtr>` 字段 |
| `os/libs/minix-boot/src/lib.rs` | Phase 1 | 导出 `PlatformDescriptorPtr` enum |
| `os/boot-shim/src/uefi_helpers.rs` | Phase 2 | 从 EFI Configuration Table 定位 RSDP/DTB |
| `os/boot-shim/src/opensbi_helpers.rs` | Phase 2 | 保存 `a1` 寄存器（DTB 物理地址） |
| `os/arch/src/arch/clock.rs` | Phase 1 | `ClockArch` 改为实例化 trait（`Sized + new(desc)`） |
| `os/arch/src/riscv64/clock.rs` | Phase 1 | 实现 `new(desc)`，删除硬编码常量 |
| `os/arch/src/arm64/clock.rs` | Phase 1 | 同上 |
| `os/arch/src/x86_64/clock.rs` | Phase 1 | 同上 |
| `os/plat/src/interrupt.rs` | Phase 1 | `InterruptController::new(desc)`，删除 `set_base()` |
| `os/plat/src/riscv64/interrupt.rs` | Phase 1 | 实现 `new(desc)`，删除 `PLIC_BASE` 常量 + `set_base()` |
| `os/plat/src/arm64/interrupt.rs` | Phase 1 | 实现 `new(desc)`，删除 `set_base()` |
| `os/plat/src/x86_64/interrupt.rs` | Phase 1 | 实现 `new(desc)` |
| `os/arch/src/arch/arch_init.rs` | Phase 1 | `ArchInit` 改为实例化 trait |
| `os/kernel/src/lib.rs`（cstart） | Phase 1 | 插入 `platform::init_from_kinfo()`，改用实例化 trait |
| `Cargo.toml`（workspace） | Phase 1 | 注册 `minix-platform` 成员 |

---

### 16.1 Phase 1：基础设施（不破坏 QEMU）

> **目标**：搭好抽象骨架，三架构走 `QemuVirtDesc` 兜底，QEMU 测试不回归。本阶段不引入任何解析器。

#### 步骤 1.1：新建 `minix-platform` crate 骨架

- **动作**：创建 crate 目录 + `Cargo.toml` + `lib.rs`（`#![no_std]`）+ `desc.rs`（仅 trait + enum 定义，无实现）
- **新增文件**：`os/libs/minix-platform/{Cargo.toml,src/lib.rs,src/desc.rs}`
- **修改文件**：workspace `Cargo.toml` 注册成员
- **依赖**：无
- **验收**：`cargo check -p minix-platform` 通过

#### 步骤 1.2：实现 `QemuVirtDesc`（三架构）

- **动作**：实现 `PlatformDesc` for `QemuVirtDesc`，用 `cfg_match!` 返回三架构当前硬编码值
- **新增文件**：`os/libs/minix-platform/src/qemu_virt.rs`
- **依赖**：步骤 1.1
- **验收**：`QemuVirtDesc` 返回值与现有硬编码常量逐一对照一致（CLINT/PLIC/GIC/LAPIC/IOAPIC）

#### 步骤 1.3：实现 `PlatformContext` 全局 + `init_from_kinfo()`

- **动作**：实现 `PlatformDescEnum` + `PlatformContext` + `static PLATFORM` + `init()` + `platform_desc()` + `init_from_kinfo()`（本阶段只处理 `None` → `QemuVirtDesc` 分支）
- **新增文件**：`os/libs/minix-platform/src/global.rs`
- **依赖**：步骤 1.2
- **验收**：`init_from_kinfo()` 在 `platform_descriptor = None` 时构造 `QemuVirtDesc` 并写入全局

#### 步骤 1.4：扩展 `KernelInfo`

- **动作**：加 `platform_descriptor: Option<PlatformDescriptorPtr>` 字段 + 定义 `PlatformDescriptorPtr` enum
- **修改文件**：`os/libs/minix-boot/src/kernel_info.rs`、`os/libs/minix-boot/src/lib.rs`
- **修改所有 `KernelInfo` 构造点**：boot-shim 两侧（`uefi_helpers`、`opensbi_helpers`）先填 `None`
- **依赖**：无（可与 1.1-1.3 并行）
- **验收**：`cargo check` 全 workspace 通过；QEMU 测试不变（字段为 `None`）

#### 步骤 1.5：硬件 trait 改为实例化签名

- **动作**：
  - `ClockArch` → `Sized + fn new(desc: &TimerDesc) -> Self` + `&mut self`/`&self` 方法
  - `InterruptController::new(desc: &InterruptControllerDesc)`，删除 `set_base()`
  - `ArchInit` → `Sized + fn new(platform: &dyn PlatformDesc) -> Self` + `&mut self::init()`
- **修改文件**：`os/arch/src/arch/clock.rs`、`os/plat/src/interrupt.rs`、`os/arch/src/arch/arch_init.rs`
- **依赖**：步骤 1.1（需要 `desc.rs` 类型）
- **验收**：trait 定义编译通过（实现尚未改）

#### 步骤 1.6：各架构实现 `new(desc)`

- **动作**：三架构的 `ClockArch`/`InterruptController`/`ArchInit` 实现 `new(desc)`，从 desc 提取地址存入实例字段；删除硬编码常量（`CLINT_MTIME`/`PLIC_BASE`/`GICD_OFFSET` 等）和 `set_base()`
- **修改文件**：
  - `os/arch/src/{riscv64,arm64,x86_64}/clock.rs`
  - `os/plat/src/{riscv64,arm64,x86_64}/interrupt.rs`
  - `os/arch/src/{riscv64,arm64,x86_64}/arch_init.rs`
- **依赖**：步骤 1.5
- **验收**：各架构 `new(&QemuVirtDesc.timer())` 产生的实例字段值 == 原硬编码常量

#### 步骤 1.7：修改 cstart 调用序列

- **动作**：在 `cstart()` 中 `prot_init()` 之后、`init_clock()` 之前插入 `platform::init_from_kinfo(&kinfo)`；`init_clock`/`intr_init`/`arch_init` 改用实例化 trait（`let mut clock = CurrentClockArch::new(&platform_desc().timer()); clock.init_timer(hz);`）
- **修改文件**：`os/kernel/src/lib.rs`（cstart / `init_clock_and_interrupts`）
- **依赖**：步骤 1.3、1.6
- **验收**：**QEMU `virt` 三架构测试全部通过**（关键里程碑）

#### 步骤 1.8：重编号 + 创建新 04 文档 + 更新正式文档

- **动作**：
  1. **重编号**：`git mv` 将 `04-clock-interrupt-init.md` ~ `24-syscall-dispatch.md` 依次重命名为 `05-` ~ `25-`
  2. **更新导航**：`00-kernel-overview.md` §3 文档导航表，所有编号 +1，并在 03 和 05 之间插入 `04-platform-discovery.md`
  3. **更新交叉引用**：全目录 grep "04-"/"05-"/.../"24-" 等旧编号引用，更新为新编号
  4. **创建 `04-platform-discovery.md`**：从本文档（plat-design.md 设计定稿）转化为面向读者的正式教学文档，覆盖：设计动机（硬编码问题）/ 方案选择（A/B/C 对比）/ `PlatformDesc` trait 设计 / 三架构统一抽象 / DTB 与 ACPI 数据源 / QEMU 兜底 / 全局存储 / 启动时序（T2.5）
  5. **更新 `03-kmain-cstart.md`**：cstart 时序图新增 `platform::init_from_kinfo()` 调用点
  6. **更新 `05-clock-interrupt-init.md`**（原 04）：标注硬编码已迁移到 `QemuVirtDesc`；trait 签名更新为实例化
- **修改文件**：`04~24` 重编号 + 新建 `04-platform-discovery.md` + 修改 `00`/`03`/`05` + 全目录交叉引用
- **依赖**：步骤 1.7
- **验收**：正式文档编号连续无空洞；`00` §3 导航表完整；所有交叉引用指向正确；`04-platform-discovery.md` 内容完整覆盖设计决策

---

### 16.2 Phase 2：boot-shim 传递指针

> **目标**：boot-shim 能定位 DTB/RSDP 物理地址并填入 `KernelInfo`。kernel 仍走兜底（解析器未实现）。

#### 步骤 2.1：UEFI boot-shim 定位 RSDP/DTB

- **动作**：在 `uefi_helpers` 构造 `KernelInfo` 时，从 `boot::config_table` 搜索 `EFI_ACPI_TABLE_GUID`（→ `Rsdp`）和 `EFI_DEVICE_TREE_GUID`（→ `Dtb`），填入 `platform_descriptor`
- **修改文件**：`os/boot-shim/src/uefi_helpers.rs`
- **依赖**：步骤 1.4
- **验收**：x86-64 UEFI 启动时 `KernelInfo.platform_descriptor = Some(Rsdp(...))`；ARM64 UEFI 同理

#### 步骤 2.2：OpenSBI boot-shim 保存 a1

- **动作**：`opensbi_helpers` 入口 trampoline 额外保存 `a1` 寄存器（DTB 物理地址），构造 `KernelInfo` 时填 `Some(Dtb(...))`
- **修改文件**：`os/boot-shim/src/opensbi_helpers.rs`
- **依赖**：步骤 1.4
- **待确认**：RISC-V SBI boot 协议下 `a1` 是否为 DTB 物理地址（验证 OpenSBI 版本约定）；若 U-Boot 未传 DTB 则填 `None`，走兜底
- **验收**：riscv64 启动时 `KernelInfo.platform_descriptor` 有值（或确认 `None` 后走兜底）

#### 步骤 2.3：更新文档

- **动作**：`01-boot-shim-bootstrap.md` 补充 boot-shim 第 3 项职责"定位 DTB/RSDP 物理地址"
- **修改文件**：`01-boot-shim-bootstrap.md`
- **依赖**：步骤 2.1、2.2
- **验收**：文档与代码一致

---

### 16.3 Phase 3：DTB 解析器

> **目标**：RISC-V/ARM64 从 DTB 解析硬件地址，不再依赖 `QemuVirtDesc`。

#### 步骤 3.1：引入 fdt crate + 实现 `DeviceTreeDesc::parse()`

- **动作**：`Cargo.toml` 加 `fdt` 依赖；实现 `DeviceTreeDesc` 结构 + `parse(dtb_phys)` 方法（遍历 `/clint`、`/plic`、`/cpus`、`/soc/uart` 等节点）
- **新增文件**：`os/libs/minix-platform/src/device_tree.rs`
- **依赖**：步骤 1.1
- **验收**：单元测试嵌入 QEMU `virt` DTB 字节数组，验证解析出的 CLINT/PLIC 地址/频率 == `QemuVirtDesc` 值

#### 步骤 3.2：`init_from_kinfo()` 接入 DTB 解析

- **动作**：`init_from_kinfo()` 中 `Some(Dtb(pa))` 分支调用 `DeviceTreeDesc::parse()`；解析失败 dev 回退 / release panic
- **修改文件**：`os/libs/minix-platform/src/global.rs`
- **依赖**：步骤 3.1、1.3
- **验收**：RISC-V/ARM64 在 `platform_descriptor = Some(Dtb)` 时从解析获取地址；QEMU 测试仍通过（有 DTB 则解析，无则兜底）

#### 步骤 3.3：更新文档

- **动作**：若 FDT 解析细节较多，新增 `plat-fdt-parsing.md`；否则在 `plat-design.md` §6 补充节点遍历说明
- **依赖**：步骤 3.2
- **验收**：文档覆盖 FDT 解析的节点路径与字段提取逻辑

---

### 16.4 Phase 4：ACPI 解析器

> **目标**：x86-64 从 ACPI 解析 IOAPIC/LAPIC 基址，不再依赖 `QemuVirtDesc`。

#### 步骤 4.1：实现 `AcpiDesc::parse()`

- **动作**：实现 RSDP → XSDT → MADT 遍历，提取 IOAPIC base / LAPIC base；HPET 定时器基址（如有）
- **新增文件**：`os/libs/minix-platform/src/acpi.rs`
- **依赖**：步骤 1.1
- **验收**：单元测试构造模拟 ACPI 表（RSDP + XSDT + MADT），验证解析出的 IOAPIC base

#### 步骤 4.2：`init_from_kinfo()` 接入 ACPI 解析

- **动作**：`init_from_kinfo()` 中 `Some(Rsdp(pa))` 分支调用 `AcpiDesc::parse()`
- **修改文件**：`os/libs/minix-platform/src/global.rs`
- **依赖**：步骤 4.1、1.3
- **验收**：x86-64 在 `platform_descriptor = Some(Rsdp)` 时从解析获取地址

#### 步骤 4.3：更新文档

- **动作**：新增 `plat-acpi-parsing.md` 或在 `plat-design.md` §6 补充 ACPI 表遍历说明
- **依赖**：步骤 4.2
- **验收**：文档覆盖 ACPI 解析的表结构与字段提取逻辑

---

### 16.5 步骤依赖图

```
1.1 ──► 1.2 ──► 1.3 ──────────────┐
                                   ▼
1.4 ──────────────────► 1.7 ──► 1.8 (Phase 1 完成)
                                   │
1.5 ──► 1.6 ──────────────────────┘
                                   │
          2.1 ──┐                  │
                ├──► 2.3 (Phase 2) │
          2.2 ──┘                  │
                                   │
          3.1 ──► 3.2 ──► 3.3 (Phase 3)
                                   │
          4.1 ──► 4.2 ──► 4.3 (Phase 4)
```

**关键里程碑**：
- **步骤 1.7 完成** = Phase 1 完成 = QEMU 测试不回归，抽象骨架就位（最重要里程碑）
- **步骤 2.2 完成** = Phase 2 完成 = 指针可传递（但 kernel 仍走兜底）
- **步骤 3.2 完成** = Phase 3 完成 = RISC-V/ARM64 脱离硬编码
- **步骤 4.2 完成** = Phase 4 完成 = x86-64 脱离硬编码，全架构真实硬件就绪
