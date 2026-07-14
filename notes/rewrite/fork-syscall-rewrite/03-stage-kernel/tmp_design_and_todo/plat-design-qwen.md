# Platform Discovery Design — Qwen 方案

> **作者**: Qwen
> **日期**: 2026-06-20
> **关联问题文档**: `plat-discovery-problem.md`
> **关联源码**: `os/arch/src/riscv64/clock.rs`, `os/plat/src/riscv64/interrupt.rs`, `os/plat/src/arm64/interrupt.rs`, `os/arch/src/x86_64/arch_init.rs`, `os/libs/minix-boot/src/kernel_info.rs`

---

## 0. TL;DR

**核心决策**：采用**方案 C（混合方案）**的变体——boot-shim 只负责**定位并传递原始指针**（DTB 物理地址 / RSDP 物理地址 / QEMU 兼容标志），kernel 在启动早期**自行解析**并构造统一的平台描述结构。

**理由**：
1. 保持 boot-shim 职责单一（加载 + 内存映射），不把硬件理解塞进去
2. kernel 可以从任意 bootloader 启动，不绑定 boot-shim
3. 与 Minix3 C 的 `acpi_init()` / `bsp_init()` 语义对齐（kernel 自己理解硬件）
4. 通过 `PlatformDesc` trait + 多态实现（DeviceTree / ACPI / QemuVirt），统一不同来源

---

## 1. 关键决策

### 1.1 归属决策：Kernel 解析，boot-shim 只传指针

| 选择 | 内容 |
|------|------|
| **boot-shim 职责** | 定位 DTB/RSDP 的物理地址，填入 `KernelInfo` 的新字段；对 QEMU 测试环境设置 `platform_compat` 标志 |
| **kernel 职责** | 在 `kmain` 早期（T3 之前）解析原始数据，构造 `PlatformDesc`，供后续 `ClockArch::init_timer()` / `InterruptController::init()` / `ArchInit::init()` 使用 |

**为什么不选方案 A（boot-shim 全解析）**：
- boot-shim 已经承担了 UEFI 内存映射、ELF 加载、ExitBootServices 等重责，再加硬件理解会导致边界模糊
- `KernelInfo` 会膨胀成一个包含所有平台字段的大结构体，每新增一种硬件就要改 boot-shim + kernel 两侧
- kernel 无法独立于 boot-shim 启动（例如被其他 bootloader 直接加载）

**为什么不选纯方案 B（kernel 全解析，无兜底）**：
- QEMU 测试环境必须继续工作，不能要求每次测试都构造合法 DTB
- 需要一个 `QemuVirtDesc` 硬编码兜底，保持开发体验

### 1.2 命名决策：`PlatformDesc`

选择 `PlatformDesc` 而非 `MachineDesc` / `BoardDesc` / `HardwareTopology`，理由：
- "Platform" 在 OS 语境中最通用，涵盖 SoC、PC、嵌入式
- 与 Linux 的 "platform device" 概念对齐
- 避免 "Machine"（与 QEMU machine 混淆）和 "Board"（暗示 PCB 级别）

### 1.3 接口粒度：分层 trait + 子结构

**不做一个大而全的 trait**，而是：

```
PlatformDesc (顶层入口)
├── InterruptControllerDesc   — 中断控制器类型 + MMIO 基址
├── TimerDesc                 — 定时器类型 + MMIO 基址 + 输入频率
├── CpuTopology               — CPU 数量 + hart/APIC ID 映射
├── SerialDesc                — 串口 MMIO 基址（可选）
└── MemoryMap                 — 已有，在 KernelInfo 中，不纳入 PlatformDesc
```

**理由**：
- `ClockArch` 只需要 `TimerDesc`，不需要知道中断控制器
- `InterruptController` 只需要 `InterruptControllerDesc`
- 细分后每个 trait 实现可以只依赖自己需要的子结构，降低耦合
- 测试时可以单独构造某个子结构

### 1.4 传递方式：全局 `&'static PlatformDesc`

**选择**：解析后的 `PlatformDesc` 以 `&'static dyn PlatformDesc` 形式存储在一个全局静态变量中，通过函数 `platform_desc() -> &'static dyn PlatformDesc` 访问。

**理由**：
- 平台描述在 boot 期间解析一次，之后全局只读
- 作为参数传给每个 `init()` 会导致大量签名变更，且 `ClockArch` 是 trait（无状态）
- `&'static` 保证生命周期安全，`Send + Sync` 保证跨 CPU 安全（BKL 释放窗口内可访问）

**实现方式**：

```rust
static PLATFORM_DESC: OnceLock<&'static dyn PlatformDesc> = OnceLock::new();

pub fn platform_desc() -> &'static dyn PlatformDesc {
    *PLATFORM_DESC.get().expect("PlatformDesc not initialized")
}
```

### 1.5 解析时机

在 `kmain()` 入口（T2）之后、`prot_init()` 之前：

```
T2: kmain 入口
  ├── memcpy(&kinfo)
  ├── platform_desc_init(&kinfo)  ← 新增：解析 DTB/RSDP/QemuVirt
  ├── prot_init()
  ├── init_clock()       — 使用 platform_desc().timer()
  ├── intr_init()        — 使用 platform_desc().interrupt_controller()
  ├── arch_init()        — 使用 platform_desc().cpu_topology() 等
  └── ...
```

**理由**：
- 越早解析越好，后续所有硬件初始化都依赖它
- 在 `prot_init()` 之前解析是安全的——此时只需要读取 DTB 物理地址并映射到虚拟地址
- 解析失败直接 panic（此时还没有进程，无法优雅恢复）

### 1.6 错误处理

| 场景 | 行为 |
|------|------|
| DTB/RSDP 指针无效（NULL 或非法地址） | 检查 `platform_compat` 标志；如果是 QEMU 兼容模式，使用 `QemuVirtDesc`；否则 panic |
| DTB 解析失败（魔数错误、结构损坏） | panic，打印错误位置 |
| ACPI 解析失败 | panic，打印错误位置 |
| 缺少必要节点（无 timer / 无 interrupt-controller） | panic，说明缺少什么 |

**不实现回退链**：要么解析成功，要么 panic。回退到硬编码只在 `platform_compat == QemuVirt` 时发生。

### 1.7 x86 路径

x86-64 以 ACPI 为主要来源，DTB 为可选补充（嵌入式 x86 场景）。

- `PlatformDesc` 的 x86 实现解析 ACPI MADT（获取 IOAPIC/LAPIC 基址）、HPET（获取定时器基址和频率）
- QEMU 测试环境使用 `QemuVirtDesc`（LAPIC + IOAPIC 固定地址、PIT 固定端口）
- COM1 `0x3F8` 继续作为架构常量保留（IBM PC 兼容机的 well-known 端口）

### 1.8 多核扩展

`PlatformDesc` 的子结构需要支持多核信息：

- `CpuTopology`：CPU 数量、每个 CPU 的 hart ID / APIC ID / MPIDR
- `InterruptControllerDesc`：ARM GIC 需要 per-CPU redistributor 基址（GICR），RISC-V PLIC 需要 per-hart context ID
- `TimerDesc`：RISC-V CLINT 需要 per-hart mtimecmp 基址

**当前阶段**：只实现单核（hart 0 / BSP），但接口设计预留多核扩展：

```rust
trait TimerDesc {
    fn timer_type(&self) -> TimerType;
    fn base_addr(&self) -> PhysBytes;
    fn frequency(&self) -> u64;
    // 多核扩展：
    fn per_cpu_offset(&self, cpu_id: usize) -> Option<PhysBytes>;
}
```

---

## 2. 接口设计

### 2.1 顶层 trait

```rust
// os/plat/src/platform_desc.rs

/// 平台硬件描述——统一不同来源（DTB / ACPI / QEMU 硬编码）的硬件参数。
///
/// 所有方法返回 `&dyn XxxDesc`，调用方通过子 trait 获取具体信息。
/// 实现必须 `Send + Sync`（BKL 释放窗口内可被其他 CPU 访问）。
pub trait PlatformDesc: Send + Sync {
    /// 中断控制器描述
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;

    /// 定时器描述
    fn timer(&self) -> &dyn TimerDesc;

    /// CPU 拓扑
    fn cpu_topology(&self) -> &CpuTopology;

    /// 串口描述（可选）
    fn serial(&self) -> Option<&dyn SerialDesc>;

    /// 平台来源标识（用于调试和日志）
    fn source(&self) -> PlatformSource;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSource {
    DeviceTree,
    Acpi,
    QemuVirt,
}
```

### 2.2 子 trait

```rust
pub trait InterruptControllerDesc: Send + Sync {
    fn controller_type(&self) -> InterruptControllerType;
    /// 主 MMIO 基址（GICD / PLIC / IOAPIC）
    fn base_addr(&self) -> PhysBytes;
    /// 辅助 MMIO 基址（GICR / 无 / LAPIC）
    fn secondary_addr(&self) -> Option<PhysBytes>;
    /// 支持的 IRQ 数量
    fn nr_irqs(&self) -> usize;
}

#[derive(Debug, Clone, Copy)]
pub enum InterruptControllerType {
    GicV3,
    Plic,
    IoApic,       // x86-64: IOAPIC + LAPIC
    LegacyPic,    // x86-64: 8259A（兼容模式）
}

pub trait TimerDesc: Send + Sync {
    fn timer_type(&self) -> TimerType;
    fn base_addr(&self) -> PhysBytes;
    /// 输入时钟频率（Hz）
    fn frequency(&self) -> u64;
}

#[derive(Debug, Clone, Copy)]
pub enum TimerType {
    ArmGenericTimer,
    RiscvClintMtime,
    X86Pit,
    X86LapicTimer,
    X86Hpet,
}

pub struct CpuTopology {
    pub nr_cpus: usize,
    /// CPU 0 的架构特定 ID（hart ID / APIC ID / MPIDR）
    pub boot_cpu_id: u64,
    // 多核扩展：per-CPU ID 数组
    // pub cpu_ids: &'static [u64],
}

pub trait SerialDesc: Send + Sync {
    fn serial_type(&self) -> SerialType;
    fn base_addr(&self) -> PhysBytes;
}
```

### 2.3 KernelInfo 扩展

```rust
// os/libs/minix-boot/src/kernel_info.rs — 新增字段

pub struct KernelInfo {
    // ... 现有字段 ...

    /// DTB 物理地址（ARM/RISC-V）。
    /// `None` 表示 boot-shim 未找到 DTB（可能是 x86 或 QEMU 兼容模式）。
    pub dtb_phys_addr: Option<PhysBytes>,

    /// RSDP 物理地址（x86-64）。
    /// `None` 表示 boot-shim 未找到 RSDP。
    pub rsdp_phys_addr: Option<PhysBytes>,

    /// 平台兼容模式标志。
    /// 当 DTB/RSDP 不可用时，kernel 使用此标志选择硬编码兜底。
    pub platform_compat: PlatformCompat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformCompat {
    /// 标准模式：必须从 DTB/RSDP 解析
    Standard,
    /// QEMU virt 兼容：允许使用硬编码地址
    QemuVirt,
}
```

### 2.4 三种实现

```rust
// os/plat/src/platform_desc/device_tree.rs
pub struct DeviceTreeDesc { /* 解析后的 DTB 数据 */ }

// os/plat/src/platform_desc/acpi.rs
pub struct AcpiDesc { /* 解析后的 ACPI 表数据 */ }

// os/plat/src/platform_desc/qemu_virt.rs
pub struct QemuVirtDesc { /* 硬编码地址 */ }
```

每种实现都实现 `PlatformDesc` trait。

### 2.5 初始化函数

```rust
// os/plat/src/platform_desc/init.rs

pub fn platform_desc_init(kinfo: &KernelInfo) -> &'static dyn PlatformDesc {
    let desc: &'static dyn PlatformDesc = match kinfo.platform_compat {
        PlatformCompat::QemuVirt => {
            // QEMU 兼容模式：直接使用硬编码
            Box::leak(Box::new(QemuVirtDesc::new()))
        }
        PlatformCompat::Standard => {
            if let Some(dtb_addr) = kinfo.dtb_phys_addr {
                // ARM/RISC-V：解析 DTB
                let dtb = unsafe { DeviceTreeDesc::from_phys(dtb_addr) }
                    .expect("DTB parsing failed");
                Box::leak(Box::new(dtb))
            } else if let Some(rsdp_addr) = kinfo.rsdp_phys_addr {
                // x86-64：解析 ACPI
                let acpi = unsafe { AcpiDesc::from_rsdp(rsdp_addr) }
                    .expect("ACPI parsing failed");
                Box::leak(Box::new(acpi))
            } else {
                panic!("No platform description available (no DTB, no RSDP, not QEMU compat)");
            }
        }
    };
    PLATFORM_DESC.set(desc).expect("PlatformDesc already initialized");
    desc
}
```

---

## 3. 当前硬编码迁移路径

### 3.1 RISC-V CLINT 定时器

**当前**：`os/arch/src/riscv64/clock.rs` 中 `CLINT_MTIME` / `CLINT_MTIMECMP` / `MTIME_FREQ` 硬编码。

**迁移后**：

```rust
impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        let timer = platform_desc().timer();
        assert!(matches!(timer.timer_type(), TimerType::RiscvClintMtime));
        let base = kernel_phys_to_virt(timer.base_addr());
        let freq = timer.frequency();
        // ... 使用 base 和 freq 代替硬编码常量 ...
    }
}
```

### 3.2 RISC-V PLIC

**当前**：`os/plat/src/riscv64/interrupt.rs` 中 `PLIC_BASE` 硬编码，`set_base()` 已存在。

**迁移后**：`set_base()` 不再需要外部调用，改为在 `Riscv64InterruptController::new()` 中从 `platform_desc()` 读取：

```rust
impl Riscv64InterruptController {
    pub fn new() -> Self {
        let ic = platform_desc().interrupt_controller();
        assert!(matches!(ic.controller_type(), InterruptControllerType::Plic));
        Self {
            plic_base: kernel_phys_to_virt(ic.base_addr()),
            nr_irqs: ic.nr_irqs(),
            // ...
        }
    }
}
```

`set_base()` 保留但标记为 `#[cfg(test)]` 或 deprecated。

### 3.3 ARM64 GICv3

**当前**：`os/plat/src/arm64/interrupt.rs` 中 `gicd_base = 0, gicr_base = 0`，`set_base()` 注入。

**迁移后**：同 3.2，在 `new()` 中从 `platform_desc()` 读取。

### 3.4 x86-64 ACPI

**当前**：`os/arch/src/x86_64/arch_init.rs` 中 ACPI 未实现。

**迁移后**：`AcpiDesc` 实现解析 MADT/HPET，`X86_64ArchInit::init()` 和 `X86_64InterruptController::init()` 从 `platform_desc()` 读取。

---

## 4. no_std 约束

- FDT 解析器：使用 `fdt` crate（`#![no_std]` 兼容，纯 Rust 实现）
- ACPI 解析器：自行实现（ACPI 表结构相对简单，只需解析 RSDP → XSDT → MADT/HPET）
- 不使用 `std` 集合或分配器；解析结果存储在 `Box::leak()` 分配的 `'static` 引用中（一次性分配，不释放）

---

## 5. 测试策略

- `QemuVirtDesc`：直接构造，零依赖
- `DeviceTreeDesc`：在 `#[cfg(test)]` 中构造模拟 DTB 二进制数据，验证解析结果
- `AcpiDesc`：在 `#[cfg(test)]` 中构造模拟 ACPI 表，验证解析结果
- 所有测试在用户态运行，不需要 QEMU

---

## 6. 与现有代码的兼容性

| 现有代码 | 处理方式 |
|---------|---------|
| `Riscv64InterruptController::set_base()` | 保留，标记 `#[cfg(test)]`；生产代码改为 `new()` 中自动读取 |
| `AArch64InterruptController::set_base()` | 同上 |
| `X86_64EarlyConsole` COM1 `0x3F8` | 保留为架构常量（IBM PC 兼容机的 well-known 端口） |
| `KernelInfo` | 新增 3 个字段（`dtb_phys_addr` / `rsdp_phys_addr` / `platform_compat`），向后兼容 |

---

## 7. 待进一步讨论的问题

1. **`Box::leak()` vs 静态分配**：`platform_desc_init()` 使用 `Box::leak()` 分配 `'static` 生命周期。在 `no_std` 环境下需要全局分配器（或改用 `static mut` + `MaybeUninit`）。这是一个实现细节，不影响接口设计。

2. **DTB 映射**：解析 DTB 需要把物理地址映射到虚拟地址。在 `platform_desc_init()` 执行时（T2 之后、T3 之前），内核已经有恒等映射或高地址映射，可以直接访问。但如果 DTB 位于物理内存高端（未被恒等映射覆盖），可能需要临时映射。

3. **ACPI 表完整性**：某些 x86 主板 ACPI 表不完整或有错误。是否需要容错解析？建议初版严格解析，失败即 panic；后续根据需要添加容错。

4. **多核 per-CPU 数据**：`PlatformDesc` 当前设计为全局只读。SMP 启动后，每个 CPU 可能需要自己的 redistributor 基址（ARM GICR）。建议通过 `CpuTopology::per_cpu_data(cpu_id)` 方法提供，但当前阶段不实现。

5. **boot-shim 如何定位 DTB/RSDP**：
   - ARM/RISC-V：UEFI 通过 `GetSystemTable()` 提供 DTB 地址（或从 `EFI_CONFIGURATION_TABLE` 中查找 DTB GUID）
   - x86-64：RSDP 通过 `EFI_ACPI_20_TABLE_GUID` 在 `EFI_CONFIGURATION_TABLE` 中查找
   - 这些是 boot-shim 的实现细节，不影响 kernel 侧接口

---

## 8. 总结

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 归属 | Kernel 解析，boot-shim 传指针 | 职责分离、可独立启动、与 C 语义对齐 |
| 命名 | `PlatformDesc` | 通用、与 Linux 对齐 |
| 接口粒度 | 分层 trait + 子结构 | 降低耦合、测试友好 |
| 传递方式 | 全局 `&'static dyn PlatformDesc` | 一次解析全局只读、避免签名污染 |
| 解析时机 | T2 之后、T3 之前 | 越早越好、后续都依赖 |
| 错误处理 | panic（QEMU 模式兜底） | 简单、boot 阶段无法优雅恢复 |
| x86 路径 | ACPI 为主、QemuVirt 兜底 | 标准做法 |
| 多核 | 接口预留、当前单核 | 渐进式实现 |
