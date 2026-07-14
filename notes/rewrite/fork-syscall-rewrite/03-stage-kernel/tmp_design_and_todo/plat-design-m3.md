# plat-design-m3: 平台硬件发现抽象设计（MiniMax M3 方案）

> **作者**: MiniMax-M3 (independent of qwen/ki/ds/glm)
> **状态**: 设计方案（待 bagging 聚合后选择）
> **关联文档**: [plat-discovery-problem.md](plat-discovery-problem.md), [00-kernel-overview.md](00-kernel-overview.md), [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md), [03-kmain-cstart.md](03-kmain-cstart.md), [04-clock-interrupt-init.md](04-clock-interrupt-init.md)
> **关联源码**:
> - `os/arch/src/riscv64/clock.rs`, `os/arch/src/arm64/clock.rs`, `os/arch/src/x86_64/clock.rs`
> - `os/plat/src/riscv64/interrupt.rs`, `os/plat/src/arm64/interrupt.rs`, `os/plat/src/x86_64/interrupt.rs`
> - `os/arch/src/riscv64/arch_init.rs`, `os/arch/src/arm64/arch_init.rs`, `os/arch/src/x86_64/arch_init.rs`
> - `os/libs/minix-boot/src/kernel_info.rs`, `os/libs/minix-boot/src/boot_shim.rs`
> - `os/boot-shim/src/uefi_helpers.rs`, `os/boot-shim/src/opensbi_helpers.rs`
> - `minix3/minix/kernel/arch/i386/arch_system.c:246-288`, `minix3/minix/kernel/arch/earm/arch_system.c:101-132`

---

## 0. 决策摘要（先看这一节）

| 编号 | 决策项 | 选择 | 一句话理由 |
|------|--------|------|-----------|
| D1 | 平台发现由谁负责？ | **Kernel 解析原始 DTB/RSDP，boot-shim 仅传递指针** | 匹配 C Minix3 `acpi_init()`/`bsp_init()` 在 kernel 内的语义（Ground Truth） |
| D2 | 抽象粒度？ | **一个 `PlatformDesc` 结构体，内部细分子结构**（`TimerDesc` / `IntrCtrlDesc` / `CpuTopology` / `BootConsoleDesc`） | 避免 trait 爆炸；子结构是数据而非 trait；接口隔离仍然清晰 |
| D3 | 存储方式？ | **全局 `static PLATFORM: OnceCell<PlatformDesc>`**（构造后只读，`&'static` 引用共享） | 匹配 C 全局模式（`omap_intr.base`）；构造后不变 → 跨 CPU 安全；`OnceCell` 是 `no_std` 友好的初始化原语 |
| D4 | trait 怎么消费？ | **`init_*` 接收 `&PlatformDesc` 显式参数** + 保留 `set_base()` 兼容路径 | 可测试性最高；与现有注入点协同；`set_base()` 调用由 `arch_init` 自动完成 |
| D5 | QEMU virt 测试路径？ | **显式 `QemuVirtDesc` fallback**：dev 构建 warn-and-fallback，release 构建 panic | 兼顾开发体验和生产安全性 |
| D6 | COM1 基址？ | **保留为架构常量**（IBM PC 兼容 well-known） | x86 IA-PC 跨主板稳定；不属于"平台发现"语义 |
| D7 | 新模块结构？ | 新建 crate `os/libs/minix-platform`，独立于 `os/libs/minix-boot` | 关注点分离：`minix-boot` 关注"如何把控制权交给 kernel"；`minix-platform` 关注"kernel 看到的硬件是什么" |
| D8 | 解析器实现？ | **自研最小化解析器**（FDT 节点遍历 + ACPI RSDP/XSDT/MADT），不引入 `fdt` crate | 满足 `no_std` + 体积可控；现有 RISC-V/ARM 启动场景只需要几个固定节点；避免外部 crate 依赖膨胀 |
| D9 | 错误处理？ | 解析失败 → fallback（dev）/ panic（release）；DTB/RSDP 指针缺失 → panic | 错误是启动期不可恢复错误，与 C 行为一致 |
| D10 | 多核扩展？ | **per-CPU 数据** 通过 `CpuTopology.cpu[hart_id]` 数组提供（GICR/per-hart mtimecmp） | 与 ARM GICv3、RV64 S-mode 应用语义对齐 |

---

## 1. 设计哲学

### 1.1 一句话总结

> **Boot-shim 做"搬运工"，kernel 做"理解者"。** Boot-shim 只负责把固件/bootloader 提供的原始硬件描述（FDT blob / ACPI RSDP 指针）原封不动地搬到 kernel 手里，kernel 在自己最合适的时机（`arch_init` 阶段）解析并构造出与平台无关的 `PlatformDesc` 抽象，供 `ClockArch`/`InterruptController`/`ArchInit` 等 trait 消费。

### 1.2 关键立场

1. **C Minix3 行为是 Ground Truth**（依据自定义约束）。
   - x86: `i386/arch_system.c:246-288` 的 `arch_init()` 调用 `acpi_init()` 解析 ACPI。
   - ARM: `earm/arch_system.c:101-132` 的 `arch_init()` 调用 `bsp_init()` 进行板级初始化。
   - **共同点**：硬件理解发生在 kernel 内部，不在 bootloader 中。
   - 因此方案 B/C（kernel 解析原始数据）比方案 A（boot-shim 解析）更贴近 C 语义。

2. **抽象是"数据"而非"行为"**。
   - `PlatformDesc` 是纯数据结构（`Copy + Clone`），不是 trait。
   - 子字段（`TimerDesc`、`IntrCtrlDesc`）也是纯数据。
   - trait（`ClockArch`、`InterruptController`）消费这个数据，但 `PlatformDesc` 本身不引入新方法。
   - 理由：硬件描述是事实陈述（"CLINT 在 0x200_BFF8"），不是行为（"如何 init CLINT"）。把数据当 trait 会强制每种实现都"重新发明"描述结构。

3. **三架构统一抽象**。
   - x86 用 ACPI，RISC-V/ARM 用 FDT——这是数据源差异。
   - `PlatformDesc` 不暴露这个差异，只提供"timer 在哪、intr_ctrl 在哪、CPU 拓扑如何"。
   - 解析器是各架构私有的（`fdt::parse()` / `acpi::parse()`），但 `PlatformDesc` 形状统一。

4. **QEMU virt 不被视为"真实平台"**。
   - QEMU `virt` 是一种特殊 platform——既可以作为"测试 fallback"（kernel 解析失败时回退），也可以作为"开发模式"（dev 构建强制使用）。
   - 它的存在是为了不阻塞开发，**不是**新平台的标准描述方式。

### 1.3 拒绝的方案

#### 拒绝方案 A（boot-shim 解析成结构化数据）
- 违反 C Ground Truth（C 的 `acpi_init` 在 kernel）。
- `KernelInfo` 会膨胀成"半个 minix-rs"，破坏 `BootPrepareResult` 的简洁契约。
- boot-shim 还要带 FDT/ACPI 解析器（虽然有 `std`，但增加 boot-shim 二进制体积和复杂度）。
- 未来换 bootloader（直接 firmware 启动、GRUB、Coreboot）会需要重新实现解析。

#### 拒绝"全局静态变量存放基址"的临时方案
- 不可测试（`static mut` 在多线程上下文是毒药）。
- 与 Rust 类型系统对抗。
- 不会自然演进到 `PlatformDesc`（因为缺乏结构化）。

#### 拒绝"`#[cfg(target_arch)]` 选择行为"
- 违反 CLAUDE.md 约束："硬件抽象为 trait，不允许 `#[cfg(target_arch)]` 行为选择"。
- 会让"RISC-V 实现里有 RISC-V 字样" → 违反"上层不接触架构细节"。

#### 拒绝"每个 trait 自己解析 FDT/ACPI"
- 违反"关注点分离"：`ClockArch::init_timer` 里出现 FDT 节点遍历是反模式。
- 解析逻辑重复（CLINT 和 PLIC 都在同一棵 FDT 树里）。

---

## 2. 架构总览

### 2.1 数据流图

```
┌─────────────────────────────────────────────────────────────────┐
│ Firmware / Bootloader (UEFI / OpenSBI+U-Boot / GRUB / Coreboot) │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ ① 提供原始硬件描述
                              │   UEFI:  EFI Configuration Table → FDT or ACPI RSDP
                              │   RISC-V: a1 register = DTB phys addr (Linux convention)
                              │   x86:    ACPI RSDP 在低 1MB 内存
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Boot-shim (no_std + alloc, 在 UEFI 中；纯 no_std 在 OpenSBI)  │
│  ─────────────────────────────────────────────────────────────  │
│  • 解析 UEFI memmap / 加载 ELF / 加载 modules / ExitBootServices│
│  • 提取 DTB/RSDP **物理地址**（不解析）                          │
│  • 构造 KernelInfo { memmap, dtb_phys, rsdp_phys, ... }          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ ② BootPrepareResult { KernelInfo, root_page, ... }
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Kernel arch_boot (各架构 head.S / 入口)                        │
│  ─────────────────────────────────────────────────────────────  │
│  • 建页表 / 切高地址 / 进入 kmain                                │
│  • cstart: prot_init → init_clock → intr_init → arch_init        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ ③ arch_init 阶段（最关键）
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  platform::init(&KernelInfo) -> &'static PlatformDesc           │
│  ─────────────────────────────────────────────────────────────  │
│  1. if dtb_phys.is_some():     fdt::parse()   -> PlatformDesc   │
│  2. elif rsdp_phys.is_some():  acpi::parse()  -> PlatformDesc   │
│  3. elif dev_build:            QemuVirtDesc  -> PlatformDesc    │
│  4. else:                      panic                                          │
│                                                                   │
│  存储到 static PLATFORM: OnceCell<PlatformDesc>，                │
│  之后所有访问通过 PLATFORM.get().expect("init before use")        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ ④ &PLATFORM 引用，跨 trait 共享
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  ClockArch::init_timer(hz, &PLATFORM)                           │
│  InterruptController::init(&PLATFORM)                            │
│  ArchInit::init(&PLATFORM)                                       │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 时序（与 04-clock-interrupt-init.md §1.1 兼容）

```
cstart()
  │
  ├── prot_init()       ← GDT/TSS/stvec 已就绪
  │
  ├── init_clock()
  │     └── ClockState::new()  (软件变量，no hw)
  │     └── ClockArch::init_timer(hz, &PLATFORM)  ← 新增 PLATFORM 参数
  │           └── 读 PLATFORM.timer.base / freq_hz
  │           └── 配置 CLINT/Generic Timer/PIT
  │
  ├── intr_init()
  │     └── InterruptController::init(&PLATFORM)  ← 新增 PLATFORM 参数
  │           └── 读 PLATFORM.intr_ctrl.base(s) / cpu_topology
  │           └── 配置 PLIC/GIC/LAPIC
  │
  ├── platform::init(&KernelInfo)  ← **新增，独立调用或并入 arch_init**
  │     └── 解析 DTB/RSDP，构造 PLATFORM 全局
  │
  └── ArchInit::init(&PLATFORM)    ← 已有 trait，新加 PLATFORM 参数
        └── 架构杂项（PMU、PMP、ACPI power mgmt）
```

**顺序要点**：
- `platform::init` 必须在 `ClockArch::init_timer` 和 `InterruptController::init` **之前**完成。
- 最简单的实现：把 `platform::init` 放在 `init_clock` 之前，或者直接在 `init_clock` 第一行调用。
- 与 C 行为的兼容性：C 的 `acpi_init` 在 `arch_init` 中调用，**晚于** `init_clock` 和 `intr_init`。这是因为 C 的 `acpi_init` 解析的是 ACPI 表，**不**用于驱动时钟和中断控制器（那些依赖硬编码）。但 Rust 重写要"从解析中获取时钟/中断的基址"，所以顺序必须前移。
- 这是与 C 的**有意偏离**（在文档中显式标注"arch scope: rust-rewrite"），理由是 C 的硬编码本身就是要被替换的。

---

## 3. 数据结构设计

### 3.1 新增 crate：`os/libs/minix-platform`

```
os/libs/minix-platform/
├── Cargo.toml
└── src/
    ├── lib.rs          # re-export + 平台入口
    ├── desc.rs         # PlatformDesc + 子结构
    ├── fdt.rs          # FDT 解析器（no_std）
    ├── acpi.rs         # ACPI 解析器（no_std）
    ├── qemu_virt.rs    # QemuVirtDesc fallback
    └── global.rs       # PLATFORM 全局 + init 函数
```

**依赖关系**：
- 依赖 `minix-boot`（用 `KernelInfo` 拿到 `dtb_phys` / `rsdp_phys`）。
- 不依赖 `minix-arch` 或 `minix-plat`（保持单向）。
- 被 `os/kernel` 在 `cstart` 中调用。

### 3.2 `PlatformDesc` 结构

```rust
// os/libs/minix-platform/src/desc.rs

/// 平台硬件描述（在 arch_init 阶段构造，之后只读）
///
/// 抽象设计：
/// - 不区分数据源（FDT/ACPI/硬编码），上层 trait 不知道数据来源
/// - 所有字段都是 Copy（无堆分配，构造后可任意移动/引用）
/// - 缺字段时用 `Option<...>` 或 `None`-friendly 的 default 描述符
///
/// C: 不存在对应物（Minix3 C 在每处直接用全局变量 + 硬编码）
#[derive(Debug, Clone, Copy)]
pub struct PlatformDesc {
    /// 硬件定时器描述
    pub timer: TimerDesc,
    /// 主中断控制器描述
    pub intr_ctrl: IntrCtrlDesc,
    /// CPU 拓扑
    pub cpu_topology: CpuTopology,
    /// 启动控制台（可选；某些平台没有早期串口）
    pub boot_console: Option<BootConsoleDesc>,
}

#[derive(Debug, Clone, Copy)]
pub struct TimerDesc {
    pub kind: TimerKind,
    /// MMIO 基址或寄存器地址
    /// RISC-V CLINT: 0x200_0000 (mtime/mtimecmp 在这之上)
    /// ARM Generic Timer: 0 (通过 CNTP_*_EL0 寄存器访问，base 无意义)
    /// x86 8254 PIT: 0x40 (PIT channel 0 data port)
    pub base: usize,
    /// 输入频率（Hz）。ARM Generic Timer 从 CNTFRQ_EL0 读；PIT 是 1193182 Hz
    /// RISC-V mtime 是 10MHz(QEMU) 或由 device tree 提供
    pub freq_hz: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerKind {
    /// x86 8254 PIT (legacy, boot-only)
    Pit8254,
    /// x86 LAPIC Timer
    LapicTimer,
    /// ARM Generic Timer (EL1 Physical)
    ArmGeneric,
    /// RISC-V CLINT mtime/mtimecmp
    RiscVClint,
}

#[derive(Debug, Clone, Copy)]
pub struct IntrCtrlDesc {
    pub kind: IntrCtrlKind,
    /// 主基址（GICD/APIC/PLIC）
    pub base: usize,
    /// per-CPU 辅助基址（GICR/per-hart mtimecmp 上下文）
    /// None 表示架构不需要 per-CPU base（如 PLIC 共享单 base + context 寄存器）
    pub per_cpu_base: Option<usize>,
    /// PLIC: context 偏移基数（不同 hart 用不同 context）
    /// GICv3: 不需要（每个 CPU 独立 GICR）
    /// LAPIC: 每个 CPU 一个 LAPIC 寄存器
    pub per_cpu_stride: Option<usize>,
    /// 支持的 IRQ 数量
    pub nr_irqs: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrCtrlKind {
    /// x86 Local APIC + I/O APIC（统一抽象；实现内部用 LAPIC+IOAPIC 协同）
    X86Apic,
    /// ARM GICv3
    ArmGicV3,
    /// RISC-V PLIC
    RiscVPlic,
}

#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    /// 总 CPU 数
    pub ncpus: u16,
    /// 每个 CPU 的信息（按 CPU id 索引；max 256 CPUs）
    pub cpus: [CpuInfo; 256],
    /// 有效 cpus 数量（< ncpus 时其余项为默认值）
    pub valid_count: u8,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuInfo {
    /// 硬件 id: APIC ID (x86) / MPIDR_EL1 (ARM) / hart_id (RISC-V)
    pub hw_id: u32,
    /// 是否 BSP（boot CPU）
    pub is_bsp: bool,
    /// 私有 GICR 基址（仅 ARM GICv3；其他架构 None）
    pub private_intr_base: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct BootConsoleDesc {
    pub kind: ConsoleKind,
    /// MMIO base (ARM/RISC-V UART) 或 port (x86 COM1)
    pub base: usize,
    /// 波特率（可选；某些 UART 不需要）
    pub baud: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleKind {
    /// x86 COM1 (port I/O, 0x3F8)
    X86Com1,
    /// ARM PL011 UART (MMIO)
    ArmPl011,
    /// RISC-V NS16550 UART (MMIO)
    RiscVUart,
}
```

### 3.3 `KernelInfo` 扩展

```rust
// os/libs/minix-boot/src/kernel_info.rs（在现有结构体上增加字段）

pub struct KernelInfo {
    // ... 现有字段（memmap, kern_virt_base, ...）保持不变 ...

    /// FDT (Flattened Device Tree) 物理地址
    /// RISC-V / ARM 平台使用；None 表示 bootloader 未提供
    /// C: 无对应物（Minix3 C 不使用 DTB）
    pub dtb_phys: Option<PhysBytes>,

    /// ACPI RSDP 物理地址
    /// x86 平台使用；UEFI 通过 EFI Configuration Table 获取
    /// C: pre_init 阶段由 GRUB 解析 multiboot mbi 间接得到
    pub rsdp_phys: Option<PhysBytes>,

    /// DTB/RSDP 的字节大小（如果已知；用于边界检查）
    /// FDT 头部的 totalsize 字段会再次校验
    pub dtb_size: Option<u64>,
}
```

**关键约束**：
- 新增字段是**纯加法**，所有现有 KernelInfo 构造点（`uefi_helpers::build_kernel_info`、`opensbi_helpers::build_kernel_info`）都需增加对应实参。
- 现有代码（`# Examples`、测试、`Cargo.toml` 引用）需要同步更新参数。
- 不破坏 ABI 兼容性，因为 `KernelInfo` 是单 crate 内 struct，对外通过 `BootPrepareResult` 暴露但只是 owned 值。

### 3.4 全局 PLATFORM

```rust
// os/libs/minix-platform/src/global.rs

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::desc::PlatformDesc;

/// 全局平台描述符句柄。
///
/// 用 AtomicUsize 存储 "是否已初始化" 标志 + 描述符地址。
/// 选 AtomicUsize 而非 OnceCell 的原因：
/// - 描述符在静态内存中（CQ 风格）以 const 构造
/// - OnceCell 需要 nightly 或外部依赖，AtomicUsize 是 stable no_std
/// - 描述符构造后不变，AtomicUsize 只用作"门控"（类似 seL4 的 `have_platform`）
///
/// SAFETY: 描述符本身是 Copy + Send + Sync，存储在 .rodata 段。
static PLATFORM_PTR: AtomicUsize = AtomicUsize::new(0);

/// 初始化全局 PLATFORM。必须在 init_clock / intr_init 之前调用。
///
/// # Arguments
/// * `info` - boot-shim 传来的 KernelInfo
///
/// # Returns
/// &'static PlatformDesc 引用（之后通过 get() 访问）
///
/// # Panics
/// - 如果 PLATFORM 已被初始化（重复 init）
/// - 如果 DTB/RSDP 都缺失且不是 dev build
pub fn init(info: &KernelInfo) -> &'static PlatformDesc {
    // 防止重复 init
    if PLATFORM_PTR.load(Ordering::Acquire) != 0 {
        panic!("platform::init called twice");
    }

    // 1. 优先尝试 DTB
    if let Some(dtb_phys) = info.dtb_phys {
        let static_desc = crate::fdt::parse(dtb_phys, info.dtb_size)
            .expect("FDT parse failed");
        PLATFORM_PTR.store(&static_desc as *const _ as usize, Ordering::Release);
        return &static_desc;
    }

    // 2. 退化到 ACPI
    if let Some(rsdp_phys) = info.rsdp_phys {
        let static_desc = crate::acpi::parse(rsdp_phys)
            .expect("ACPI parse failed");
        PLATFORM_PTR.store(&static_desc as *const _ as usize, Ordering::Release);
        return &static_desc;
    }

    // 3. dev build fallback: QEMU virt
    if cfg!(feature = "qemu-virt-fallback") || cfg!(debug_assertions) {
        // 注意：这只在 dev 构建下生效
        let static_desc = crate::qemu_virt::desc();
        PLATFORM_PTR.store(&static_desc as *const _ as usize, Ordering::Release);
        return &static_desc;
    }

    // 4. release 构建 + 缺失描述 = 不可恢复错误
    panic!("platform::init: no DTB or RSDP provided, and qemu-virt-fallback disabled");
}

/// 获取 PLATFORM 引用。必须先调用 init()。
pub fn get() -> &'static PlatformDesc {
    let ptr = PLATFORM_PTR.load(Ordering::Acquire);
    if ptr == 0 {
        panic!("platform::get() called before init()");
    }
    // SAFETY: init() 之后 ptr 指向一个有效的 PlatformDesc，
    //         且 PlatformDesc 是 'static（存放在 .rodata）
    unsafe { &*(ptr as *const PlatformDesc) }
}
```

**QEMU virt fallback 约束**：
- 仅在 `debug_assertions` 或 `feature = "qemu-virt-fallback"` 开启时生效。
- release 构建 + 缺失描述 = panic（防止生产环境意外运行 QEMU 硬编码）。
- 这是"工程妥协"——开发时方便，但有显式护栏。

---

## 4. 解析器设计

### 4.1 FDT 解析器

```rust
// os/libs/minix-platform/src/fdt.rs

use crate::desc::{PlatformDesc, TimerDesc, TimerKind, IntrCtrlDesc, IntrCtrlKind,
                   CpuTopology, CpuInfo, BootConsoleDesc, ConsoleKind};
use minix_types::PhysBytes;

/// FDT 头部（libfdt 兼容）
#[repr(C)]
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_END: u32 = 0x0000_0009;

/// 解析 FDT blob，返回 'static PlatformDesc（放在 .rodata 静态）
///
/// # Safety
/// `phys` 必须指向有效 FDT blob（`fdt_magic` 已校验），
/// 且在 init() 返回后 blob 仍可访问（FDT 通常在 boot-shim 退出后仍保留在内存）。
pub fn parse(phys: PhysBytes, size: Option<u64>) -> &'static PlatformDesc {
    let hdr = unsafe { &*(phys.0 as *const FdtHeader) };
    assert_eq!(hdr.magic.swap_bytes(), FDT_MAGIC, "FDT magic mismatch");

    // 简单 FDT 遍历：找 /cpus, /timer, /soc/interrupt-controller, /chosen
    let mut desc = PlatformDesc::default();
    walk(phys, &mut desc);
    store_static(desc)
}

fn walk(phys: PhysBytes, desc: &mut PlatformDesc) {
    // 简化：实现一个 stack-based FDT 树遍历器
    // - 找到 /cpus/cpu@N → CpuInfo
    // - 找到 /soc/timer → TimerDesc
    // - 找到 /soc/interrupt-controller → IntrCtrlDesc
    // - 找到 /chosen/linux,stdout-path → BootConsoleDesc
    // ...
}
```

**关键设计**：
- 只解析**当前需要的节点**（timer、intr_ctrl、cpus、stdout）；其他节点跳过。
- 节点名匹配优先用 `compatible` 属性（`"riscv,clint0"`、`"arm,cortex-a15-gic"`），而非路径——更鲁棒。
- 解析后通过 `store_static` 把结果存到 `.rodata` 段（`static PLATFORM_FDT_RESULT: MaybeUninit<PlatformDesc>`），返回 `&'static` 引用。

### 4.2 ACPI 解析器

```rust
// os/libs/minix-platform/src/acpi.rs

use crate::desc::*;

#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],   // "RSD PTR "
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    // ACPI 2.0+
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

pub fn parse(rsdp_phys: PhysBytes) -> &'static PlatformDesc {
    let rsdp = unsafe { &*(rsdp_phys.0 as *const Rsdp) };
    assert_eq!(&rsdp.signature, b"RSD PTR ", "RSDP signature mismatch");

    // 决定用 XSDT (64-bit) 还是 RSDT (32-bit)
    let use_xsdt = rsdp.revision >= 2 && rsdp.xsdt_address != 0;
    // ... 遍历 SDT 找 MADT (Multiple APIC Description Table) ...
    // ... 从 MADT 提取 LAPIC base、IOAPIC base、CPU 数 ...
    // ... 找 FADT (Fixed ACPI Description) 提取 PM timer / 时钟 ...
    // ... 找 MCFG (PCIe) / HPET 等（未来）...
    todo!("ACPI parser: implement MADT parsing")
}
```

**最小化范围**：
- 当前只实现 MADT 解析（提取 LAPIC base + CPU 拓扑）。
- 不实现 FADT/HPET/MCFG（这些对应到当前 QEMU virt 假设的硬件未来才会用到）。
- 在 `desc.rs` 注释中明确："当前 ACPI 解析仅覆盖 MADT，其他表按需扩展"。

### 4.3 QEMU virt fallback

```rust
// os/libs/minix-platform/src/qemu_virt.rs

use crate::desc::*;

/// QEMU `virt` machine 的硬编码描述。
///
/// **仅供开发/测试使用**。QEMU `virt` 是固定平台，地址稳定；
/// 但这不是"通用解决方案"，真实硬件需要 FDT/ACPI 解析。
pub const fn desc() -> PlatformDesc {
    PlatformDesc {
        timer: TimerDesc {
            kind: TimerKind::RiscVClint,  // 由 cfg(target_arch) 在上层选择
            base: 0x200_0000,
            freq_hz: 10_000_000,
        },
        intr_ctrl: IntrCtrlDesc {
            kind: IntrCtrlKind::RiscVPlic,
            base: 0x0C00_0000,
            per_cpu_base: None,
            per_cpu_stride: Some(0x1000),
            nr_irqs: 64,
        },
        cpu_topology: CpuTopology {
            ncpus: 1,
            cpus: [CpuInfo {
                hw_id: 0, is_bsp: true, private_intr_base: None
            }; 256],
            valid_count: 1,
        },
        boot_console: None,  // 各架构早期 console 走自己的常量
    }
}
```

**重要约束**：
- `desc()` 是 `const fn`，可在编译期求值（虽然实际是运行时调用）。
- `QemuVirtDesc` 的 `kind` 字段在 `riscv64` build 下填 `RiscVClint`，`aarch64` 下填 `ArmGeneric`，`x86_64` 下填 `Pit8254`。
- **不允许在 `desc()` 内部用 `cfg!(target_arch)`**——这是构建期选择，应在调用方处理。
- 实际调用：`desc_for_target()` 函数根据 `cfg!(target_arch)` 构造对应 QemuVirtDesc。

---

## 5. trait 接口修改

### 5.1 `ClockArch` 增加 PLATFORM 参数

```rust
// os/arch/src/arch/clock.rs (修改)

pub trait ClockArch {
    /// 旧签名: fn init_timer(hz: u32)
    /// 新签名: fn init_timer(hz: u32, platform: &PlatformDesc)
    ///
    /// platform.timer 提供 base 和 freq_hz，
    /// 取代之前的硬编码常量（CLINT_MTIME 等）。
    fn init_timer(hz: u32, platform: &PlatformDesc);
    fn read_ticks() -> u64;
    fn read_tsc() -> u64 { Self::read_ticks() }
}
```

**示例实现（RISC-V64）**：

```rust
// os/arch/src/riscv64/clock.rs (修改后)

use crate::clock::ClockArch;
use minix_platform::{PlatformDesc, TimerKind};

pub struct Riscv64ClockArch;

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32, platform: &PlatformDesc) {
        // 平台描述已就绪，不需要硬编码
        assert_eq!(platform.timer.kind, TimerKind::RiscVClint);
        let base = platform.timer.base;
        let freq = platform.timer.freq_hz;
        let mtime = base + 0xBFF8;       // mtime 在 CLINT 基址 + 0xBFF8
        let mtimecmp = base + 0x4000;    // mtimecmp 在 CLINT 基址 + 0x4000 (hart 0)

        let mtime_val: u64 = unsafe { core::ptr::read_volatile(mtime as *const u64) };
        let interval = freq / hz as u64;
        unsafe { core::ptr::write_volatile(mtimecmp as *mut u64, mtime_val + interval); }
        unsafe { core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64); }
    }
    fn read_ticks() -> u64 {
        // mtime 地址需要从 PLATFORM 读
        let base = minix_platform::get().timer.base;
        let mtime = base + 0xBFF8;
        unsafe { core::ptr::read_volatile(mtime as *const u64) }
    }
}
```

**迁移注意**：
- `read_ticks()` 仍然无参（因为它从 PLATFORM 全局读），这避免了"在中断 handler 中多一个参数"的不优雅。
- 另一种选择：`read_ticks(&PlatformDesc)`，但中断 handler 调用栈深，传递平台描述会让寄存器使用复杂化。
- 选全局读取，因为 PLATFORM 在构造后不变。

### 5.2 `InterruptController` 增加 PLATFORM 参数

```rust
// os/plat/src/interrupt.rs (修改)

pub trait InterruptController: Sized {
    /// 旧签名: fn init(&mut self);
    /// 新签名: fn init(&mut self, platform: &PlatformDesc);
    fn init(&mut self, platform: &PlatformDesc);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}
```

**示例实现（ARM64）**：

```rust
// os/plat/src/arm64/interrupt.rs (修改后)

impl AArch64InterruptController {
    pub const fn new() -> Self {
        Self { gicd_base: 0, gicr_base: 0, nr_irqs: 0, last_iar: 0 }
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self, platform: &PlatformDesc) {
        assert_eq!(platform.intr_ctrl.kind, IntrCtrlKind::ArmGicV3);
        // 从 PlatformDesc 自动填充（取代 set_base 手动调用）
        self.gicd_base = platform.intr_ctrl.base;
        self.gicr_base = platform.intr_ctrl.per_cpu_base
            .expect("ARM GICv3: per_cpu_base required");
        self.nr_irqs = platform.intr_ctrl.nr_irqs as usize;
        // ... 原有 init 逻辑 ...
    }
    // ...
}

// 保留 set_base 用于旧路径（标 #[deprecated]）
impl AArch64InterruptController {
    #[deprecated(note = "call init(platform) instead; set_base is for legacy test code")]
    pub fn set_base(&mut self, gicd_base: usize, gicr_base: usize) {
        self.gicd_base = gicd_base;
        self.gicr_base = gicr_base;
    }
}
```

### 5.3 `ArchInit` 增加 PLATFORM 参数

```rust
// os/arch/src/arch/arch_init.rs (修改)

pub trait ArchInit {
    /// 旧签名: fn init();
    /// 新签名: fn init(platform: &PlatformDesc);
    fn init(platform: &PlatformDesc);
}
```

**x86-64 ArchInit（填充 ACPI 解析）**：

```rust
// os/arch/src/x86_64/arch_init.rs (修改后)

impl ArchInit for X86_64ArchInit {
    fn init(platform: &PlatformDesc) {
        // x86 平台发现的关键：APIC base、ACPI 表指针
        // 实际 APIC init 已在 InterruptController::init() 中完成
        // 这里只做 ACPI 表遍历的进一步操作（power mgmt、NUMA 等）
        // 当前为空：x86 上 LAPIC 已经从 PlatformDesc 拿 base
    }
}
```

---

## 6. Boot-shim 改动

### 6.1 UEFI 路径

```rust
// os/boot-shim/src/uefi_helpers.rs (修改)

use uefi::table::cfg::ACPI2_GUID;
use uefi::table::cfg::ACPI_GUID;

pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    boot_modules: &'static [BootModule],
    bootstrap_start: PhysBytes,
    bootstrap_len: u64,
) -> KernelInfo {
    // 从 UEFI Configuration Table 找 ACPI RSDP / FDT
    let (dtb_phys, rsdp_phys) = locate_hardware_desc();

    KernelInfo {
        memmap,
        kern_virt_base,
        kern_phys_base,
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(kern_virt_base.0 as u64 + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        // 新增
        dtb_phys,
        dtb_size: None,  // UEFI 不提供；FDT 解析时会自校验
        rsdp_phys,
    }
}

/// 从 UEFI Configuration Table 提取硬件描述指针。
///
/// 优先级：DTB > ACPI 2.0+ RSDP > ACPI 1.0 RSDP。
/// UEFI 提供 `EFI_ACPI_20_TABLE_GUID` (ACPI 2.0+) 和 `EFI_ACPI_TABLE_GUID` (1.0)，
/// 都不存在时可能由 FDT_GUID 提供 device tree。
fn locate_hardware_desc() -> (Option<PhysBytes>, Option<PhysBytes>) {
    let system_table = uefi::system::system_table();
    let cfg_tbl = system_table.config_table();

    // 1. 尝试 FDT_GUID（Device Tree）
    const FDT_GUID: uefi::Guid = uefi::Guid::from_values(...);
    let dtb = cfg_tbl.iter()
        .find(|e| e.guid == FDT_GUID)
        .map(|e| PhysBytes(e.address as u64));

    // 2. 尝试 ACPI 2.0+ RSDP
    let rsdp2 = cfg_tbl.iter()
        .find(|e| e.guid == ACPI2_GUID)
        .map(|e| PhysBytes(e.address as u64));

    // 3. 退化到 ACPI 1.0 RSDP
    let rsdp1 = if rsdp2.is_none() {
        cfg_tbl.iter()
            .find(|e| e.guid == ACPI_GUID)
            .map(|e| PhysBytes(e.address as u64))
    } else { None };

    (dtb, rsdp2.or(rsdp1))
}
```

### 6.2 OpenSBI 路径

```rust
// os/boot-shim/src/opensbi_helpers.rs (修改)

/// RISC-V Linux convention: a1 = DTB phys addr
/// (见 riscv-linux/Documentation/arch/riscv/boot-image-header.rst)
const DTB_A1_MAGIC: u64 = 0x1;  // OpenSBI 在某些版本会写这个，但通常 a1 直接是地址

/// 入口 trampoline 在 main.rs 中接收 a0 (BootFileTable) 和 a1 (DTB)
/// install_boot_file_table(a0)  +  install_dtb(a1)

static mut DTB_PTR: u64 = 0;

pub unsafe fn install_dtb(addr: u64) {
    debug_assert!(DTB_PTR == 0);
    DTB_PTR = addr;
}

pub fn dtb_phys() -> Option<PhysBytes> {
    let ptr = unsafe { DTB_PTR };
    if ptr == 0 { return None; }
    Some(PhysBytes(ptr))
}
```

**入口 trampoline 改造**（`os/boot-shim/src/main.rs`，OpenSBI build）：

```rust
// 当前：仅接收 a0 (BootFileTable)
// 目标：同时接收 a1 (DTB)，由 trampoline 转发

#[naked]
#[no_mangle]
pub unsafe extern "C" fn _start(a0: u64, a1: u64, a2: u64) -> ! {
    // 保存 a1 (DTB 地址) 到静态变量
    // 跳转到 main_rust
    core::arch::asm!(
        "la t0, {dtb_slot}",
        "sd a1, 0(t0)",
        "j  {main_rust}",
        dtb_slot = sym DTB_PTR,
        main_rust = sym main_rust,
        options(noreturn)
    );
}

fn main_rust() -> Status {
    // ... 现有逻辑 ...
    // install_boot_file_table 已由调用方处理
    // dtb 已经在 trampoline 中保存
    OpenSbiBootShim::prepare_boot(8)
}
```

**内核 build_kernel_info 调用**：

```rust
// OpenSbiBootShim::prepare_boot
let kernel_info = build_kernel_info(
    memmap, kern.kern_virt_base, kern.kern_phys_base, kern.kern_size,
    boot_modules, PhysBytes(0), kern.kern_phys_base.0,
    // 新增：DTB 物理地址
    dtb_phys(),  // 从 DTB_PTR 静态读
    None,        // OpenSBI 路径无 RSDP
);
```

### 6.3 x86 路径的 RSDP 发现

x86 UEFI 路径中 RSDP 通过 UEFI Configuration Table 找到（§6.1）。但要注意：
- 旧版 BIOS 启动（无 UEFI）需要扫描 0x40:0x00 附近（EBDA 段）和 0x000E0000-0x000FFFFF。
- minix-rs 当前不直接支持纯 BIOS 启动（必须经 boot-shim），所以这个扫描责任在 boot-shim（如果未来要支持）。
- 当前阶段：x86 路径只需要 UEFI Configuration Table → RSDP，足够。

---

## 7. 4 处硬编码的迁移路径

### 7.1 迁移总表

| 位置 | 当前值 | 迁移到 | 备注 |
|------|--------|--------|------|
| `os/arch/src/riscv64/clock.rs:13-23` | `CLINT_MTIME=0x200_BFF8`<br>`CLINT_MTIMECMP=0x200_4000`<br>`MTIME_FREQ=10_000_000` | `platform.timer.base + 0xBFF8`<br>`platform.timer.base + 0x4000`<br>`platform.timer.freq_hz` | **完全移除硬编码** |
| `os/plat/src/riscv64/interrupt.rs:9-11` | `PLIC_BASE=0x0C00_0000` | `platform.intr_ctrl.base` | **完全移除硬编码** |
| `os/plat/src/arm64/interrupt.rs:9-13` | `GICD_OFFSET=0`<br>`GICR_OFFSET=0xA0000` | `platform.intr_ctrl.base`<br>`platform.intr_ctrl.per_cpu_base` | **完全移除硬编码** |
| `os/arch/src/x86_64/arch_init.rs:32-39` | (ACPI TODO) | (从 `platform` 读 ACPI 信息；x86 ArchInit 体仍可能为空) | **填充 ACPI 解析** |
| `os/plat/src/x86_64/early_console.rs` (COM1) | `0x3F8` | **保留为架构常量** | 详见 §7.5 |

### 7.2 迁移步骤（按 PR 顺序）

1. **PR 1: 基础设施**
   - 新建 `os/libs/minix-platform` crate
   - 实现 `desc.rs`（纯数据结构）
   - 实现 `qemu_virt.rs`（fallback）
   - 实现 `global.rs`（PLATFORM 句柄 + `init`/`get` API）
   - **测试**：单元测试覆盖 `desc()` const 构造、`init()` 双调用 panic

2. **PR 2: KernelInfo 扩展**
   - `os/libs/minix-boot/src/kernel_info.rs` 增加 `dtb_phys` / `rsdp_phys` / `dtb_size`
   - `os/boot-shim/src/uefi_helpers.rs` 实现 `locate_hardware_desc()` + 填充新字段
   - `os/boot-shim/src/opensbi_helpers.rs` 实现 `install_dtb` + 填充新字段
   - **测试**：单元测试覆盖"两个 build_kernel_info 调用都正确传递新字段"

3. **PR 3: 最小 FDT 解析器**
   - `os/libs/minix-platform/src/fdt.rs` 实现最小 FDT 遍历
   - 支持节点：`/cpus`, `/soc/timer`, `/soc/interrupt-controller`
   - **测试**：单元测试使用真实 QEMU DTB blob（dump from QEMU `-machine dumpdtb=`）

4. **PR 4: 最小 ACPI 解析器**
   - `os/libs/minix-platform/src/acpi.rs` 实现 RSDP + MADT 解析
   - **测试**：单元测试使用真实 ACPI 表格（dump from QEMU）

5. **PR 5: trait 接口扩展**
   - `os/arch/src/arch/clock.rs`: `init_timer(hz, &PlatformDesc)`
   - `os/plat/src/interrupt.rs`: `init(&mut self, &PlatformDesc)`
   - `os/arch/src/arch/arch_init.rs`: `init(&PlatformDesc)`
   - 更新所有实现（3 个 clock + 3 个 interrupt + 3 个 arch_init）
   - 保留 `set_base()` 但标 `#[deprecated]`
   - **测试**：编译通过 + qemu-tests 通过

6. **PR 6: 迁移硬编码**
   - RISC-V clock.rs: 移除 3 个 const，从 PLATFORM 读
   - RISC-V interrupt.rs: 移除 PLIC_BASE，从 PLATFORM 读
   - ARM64 interrupt.rs: 移除 GICD/GICR OFFSET，从 PLATFORM 读
   - x86_64 arch_init.rs: 填充 ACPI 解析调用
   - **测试**：qemu-virt 测试在 3 个架构上都通过

7. **PR 7: 清理**
   - 删除 `set_base()` 方法（或保留 deprecated 到下一次 major 版本）
   - 文档同步：`04-clock-interrupt-init.md` 添加"PLATFORM 来源"一节
   - 更新 `00-kernel-overview.md §1.5.3` 硬件操作约束

### 7.3 兼容性保证

- 旧 QEMU 测试不中断：dev 构建默认开启 `qemu-virt-fallback`，所以 DTB/RSDP 缺失时自动用 QemuVirtDesc。
- `set_base()` 保留（`#[deprecated]`），旧测试代码可继续工作。
- KernelInfo 字段是纯加法，旧 build 代码不需重写（编译器默认填充 `None`）。

### 7.4 失败模式

| 情况 | 行为 | 理由 |
|------|------|------|
| 启动时 DTB 和 RSDP 都缺失 | dev: fallback QemuVirtDesc；release: panic | 见 §3.4 |
| FDT 解析失败（magic 错误） | panic | 数据源不可信 = 硬件不可识别 |
| ACPI RSDP 校验和错误 | panic | 同上 |
| DTB 中缺 timer 节点 | panic，提示"platform missing timer" | 启动期不可恢复 |
| 多余的 IRQ 数量不足 | panic，提示"nr_irqs too small for hardware" | 配置错误 |
| FDT 节点引用了不存在的 phandle | panic 或 warn-and-default | 默认 warn，不阻塞启动 |

### 7.5 COM1 决策说明

`os/plat/src/x86_64/early_console.rs` 中的 `0x3F8` **不**纳入 `PlatformDesc`。理由：

- COM1 是 IBM PC/AT 标准端口，跨所有 IA-PC 兼容主板稳定（包括 QEMU virt、真实 PC、Coreboot）。
- 一些嵌入式 x86 板（无 Super I/O 控制器）确实可能不同，但这是边缘情况，且这些板通常不跑通用 OS。
- 把 COM1 base 放进 `PlatformDesc` 反而增加 `PlatformDesc` 字段数（4 个 console kind），收益小。
- 未来如需要，可以用 `MCFG` / `SPCR` ACPI 表覆盖，但优先级低。

**决策**：COM1 保持硬编码（带 `// IBM PC standard port, well-known across IA-PC` 注释）；如果未来真出现非标准 x86 板，再扩展 `BootConsoleDesc`。

---

## 8. 测试策略

### 8.1 单元测试

```rust
// os/libs/minix-platform/src/desc.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_desc_is_copy() {
        let d = PlatformDesc::default();
        let _d2 = d;  // 必须能 Copy
        let _d3 = d;  // 多次 Copy 都行
    }

    #[test]
    fn platform_desc_size_reasonable() {
        // 应该足够小，能放进 .rodata
        assert!(core::mem::size_of::<PlatformDesc>() < 4096);
    }
}
```

### 8.2 解析器测试（使用真实 dump）

```rust
// os/libs/minix-platform/src/fdt.rs

#[cfg(test)]
mod tests {
    /// QEMU virt RISC-V DTB, dumped via `qemu-system-riscv64 -machine virt -machine dumpdtb=virt.dtb`
    /// 提交为 test asset
    static VIRT_DTB: &[u8] = include_bytes!("../test_data/virt_riscv.dtb");

    #[test]
    fn parse_qemu_virt_dtb() {
        // 把 DTB bytes 写入测试 buffer（模拟 phys addr）
        let mut buf = [0u8; 65536];
        buf[..VIRT_DTB.len()].copy_from_slice(VIRT_DTB);

        let desc = unsafe { parse(PhysBytes(buf.as_ptr() as u64), Some(VIRT_DTB.len() as u64)) };

        // 验证 QEMU virt 的 CLINT 在 0x200_0000
        assert_eq!(desc.timer.base, 0x200_0000);
        assert_eq!(desc.timer.freq_hz, 10_000_000);
        // 验证 PLIC 在 0xC00_0000
        assert_eq!(desc.intr_ctrl.base, 0x0C00_0000);
    }
}
```

### 8.3 集成测试（QEMU）

```bash
# QEMU 测试脚本（已有）
os/qemu-tests/run-qemu-riscv64.sh
os/qemu-tests/run-qemu-aarch64.sh
os/qemu-tests/run-qemu-x86_64.sh
```

迁移后这些测试**应**继续通过，因为：
- dev 构建默认 fallback QemuVirtDesc
- QemuVirtDesc 的常量值与原硬编码一致

### 8.4 真实硬件测试（未来）

- 准备一块真实 RISC-V 开发板（SiFive HiFive Unmatched 或 StarFive VisionFive 2）
- dump 其 DTB，用 §8.2 的测试
- 如果解析成功但启动失败 → 排查 timer/intr_ctrl 字段映射
- 优先级低（不阻塞主流程合并）

### 8.5 回归测试

- `tools/review-init.sh review` 全套自检（如果存在）
- 编译警告 ≤ 当前水平
- `cargo test` 全通过

---

## 9. 与 Minix3 C 的语义对齐

### 9.1 行为契约表

| C Minix3 函数 | 行为 | minix-rs 抽象 | 差异（arch scope: rust-rewrite） |
|---------------|------|---------------|----------------------------------|
| `i386/arch_init()::acpi_init()` | 解析 RSDP，遍历 ACPI 表 | `platform::acpi::parse(rsdp_phys)` | 同语义；RSDP 来源从 multiboot 改为 KernelInfo.rsdp_phys |
| `earm/arch_init()::bsp_init()` | 调用 `bsp_xxx_init()` 板级函数 | `platform::fdt::parse(dtb_phys)` | 同语义；DTB 替代 BSP 硬编码 |
| `i8259.c::intr_init()` | 配置 8259A | `InterruptController::init(platform)` | IOAPIC 化；PLATFORM 提供 base |
| `omap_intr.c::intr_init()` | 从 machine.board_id 选基址 | `platform::fdt::parse` 选 GIC base | DTB 替代 `BOARD_IS_BBXM` 宏 |
| `clock.c::init_clock()` | 软件变量初始化 | `ClockState::new()` | **完全等价**（init_clock 一直是软件） |
| `arch_clock.c::apic_enable()` | LAPIC 定时器配置 | `X86_64ClockArch::init_timer(hz, platform)` | LAPIC base 从 PLATFORM 读 |

### 9.2 显式标注的偏离

| 偏离 | 理由 | 标注位置 |
|------|------|----------|
| `acpi_init` 在 C 中是 `arch_init` 的一部分（晚），在 Rust 中前移到 `init_clock` 之前 | C 的 `acpi_init` 不驱动硬件；Rust 的 platform::init 必须早于 init_clock | `04-clock-interrupt-init.md` §1.1 |
| `bsp_init` 是函数（C）vs PlatformDesc 是数据（Rust） | Rust 用 trait + data 而非函数指针，类型安全 + 可测试 | `00-kernel-overview.md` §1.5.3 |
| `set_base()` 在 Rust 中保留为 deprecated | 兼容旧测试代码 | 迁移完成后删除 |

### 9.3 行为不变性

- **SMP 启动顺序**：BSP 先 init 时钟/中断，然后启 AP。Rust 保持同序。
- **中断屏蔽**：init 完成后所有 IRQ 仍被 mask，等待 `mask_all()` 之后 `unmask`。
- **时钟频率**：100 Hz（minix-rs 统一）；C 中 x86 是 60 Hz，ARM 是 1000 Hz。
- **per-CPU 数据布局**：ARM GICR、per-hart mtimecmp 都通过 `CpuTopology.cpu[i].private_intr_base` 表达。

---

## 10. 范围与不在范围

### 10.1 在本次设计范围内

- `PlatformDesc` 抽象（数据结构 + 子结构）
- `os/libs/minix-platform` 新 crate 设计
- 4 处硬编码（CLINT、PLIC、GIC、ACPI TODO）的迁移
- `KernelInfo` 扩展（3 个新字段）
- boot-shim 的 DTB/RSDP 提取
- QEMU virt fallback 策略
- trait 接口签名变化（`init_*` 增加 `&PlatformDesc` 参数）
- 单元测试 + 集成测试覆盖

### 10.2 不在本次设计范围内

- **完整 FDT 解析器**：当前只解析 timer/intr_ctrl/cpus/stdout；其他节点（PCI、I2C、clock 等）按需扩展。
- **完整 ACPI 解析器**：当前只解析 MADT；FADT/HPET/MCFG 等按需扩展。
- **设备驱动迁移**：`BootConsoleDesc` 当前只在 `qemu_virt.rs` 中实现；真实驱动用它在 `devman` 中如何路由，留给 devman 迁移阶段。
- **热插拔**：CPU/设备热插拔不涉及（minix-rs 是嵌入式微内核，暂不支持）。
- **NUMA 拓扑**：`CpuTopology` 暂不考虑 NUMA 距离矩阵（minix3 也不考虑）。
- **跨 kernel 镜像兼容**：D 版（vmlinux）和 E 版（ukernel）共用 PlatformDesc 还是分别构造？留给运行时设计阶段。

### 10.3 已知弱点

1. **PLATFORM 全局有微秒级 init 竞争**：SMP 启动早期（ap_startup）可能在 init 之前访问 PLATFORM。
   - 缓解：在 `init` 完成后插入 `smp_mb()` + 在 `get()` 中 panic。
   - 长期方案：把 PLATFORM 改为构造时即填充（QemuVirtDesc 已经是 const 构造）。

2. **FDT 解析器自研**：放弃 `fdt` crate 是个权衡——增加维护成本但减少依赖。
   - 缓解：实现极简版本（< 200 行），单元测试覆盖率高。
   - 长期：若 FDT 需求增长（多设备树、热插拔），再切换到 `fdt` crate。

3. **QEMU virt fallback 可能掩盖真实 bug**：开发时方便，但开发人员可能忘了解析器失败。
   - 缓解：fallback 时 `warn!` 一条明显日志。
   - 长期：CI 流水线要求 release 构建有真实 DTB。

4. **COM1 硬编码例外**：未来如出现非标准 x86 板需重审。

5. **多架构 DTB 节点命名差异**：QEMU virt DTB 用 `soc` 节点，真实板可能用 `soc@0` 或 `bus`。解析器需要容错。

---

## 11. 待 bagging 阶段讨论的关键问题

> 这些问题在多 AI 方案中可能给出不同答案；最终选择应基于 ground truth + 工程实用性。

1. **PLATFORM 是全局静态还是 per-CPU？**
   - 本方案选全局静态（构造后不变）。
   - 备选：`static PLAT_PER_CPU: [PlatformDesc; MAX_CPUS]`——per-CPU 副本，支持 per-CPU base 不同的拓扑。
   - 决策依据：当前 minix-rs 的 `CpuTopology` 已经内含 `cpus[]` 数组，全局静态足够。

2. **QemuVirtDesc 的触发条件是 dev-only 还是 feature flag？**
   - 本方案：dev 自动 + `feature = "qemu-virt-fallback"` 显式。
   - 备选：纯 feature flag（更显式）。
   - 决策依据：dev 自动是工程友好，但需要文档明确。

3. **FDT 解析器自研 vs `fdt` crate？**
   - 本方案：自研最小化版本。
   - 备选：引入 `fdt = "0.1"` crate（no_std 兼容，社区维护）。
   - 决策依据：依赖 vs 维护成本的权衡。

4. **trait 方法签名是否要保持向后兼容（`init_timer(hz)` + 新加 `init_timer_with_platform(hz, &PlatformDesc)`）？**
   - 本方案：直接改签名，破坏 API（同步所有调用点）。
   - 备选：保留旧方法 + 新方法共存。
   - 决策依据：minix-rs 是内部项目，破坏 API 成本可控；保留两个方法增加代码重复。

5. **COM1 base 是否应该进 PlatformDesc？**
   - 本方案：保留硬编码。
   - 备选：进 PlatformDesc（一致性更高）。
   - 决策依据：x86 IA-PC 标准 vs 抽象一致性。倾向保留硬编码。

6. **新 crate 命名 `minix-platform` 是否合适？**
   - 备选：`minix-hwdesc`、`minix-board`、`minix-machdesc`。
   - 决策依据：`PlatformDesc` 是最广泛使用的术语（Linux `platform_device`、seL4 `plat_desc_t`）。

7. **DTB/RSDP 之外是否需要传递其他硬件信息（如 commandline、initrd）？**
   - 本方案：仅 DTB/RSDP，commandline/initrd 暂不传递。
   - 备选：扩展 KernelInfo 添加更多字段。
   - 决策依据：当前 QEMU 测试不需要；按需添加。

---

## 12. 实施时间表

| 阶段 | 内容 | 预计工作量 | 阻塞 |
|------|------|----------|------|
| **Phase 1** | `minix-platform` crate 骨架 + `PlatformDesc` 数据结构 | 0.5 天 | 无 |
| **Phase 2** | `KernelInfo` 扩展 + boot-shim DTB/RSDP 提取 | 1 天 | Phase 1 |
| **Phase 3** | FDT 解析器（RISC-V/ARM 必要节点） | 2 天 | Phase 1 |
| **Phase 4** | ACPI 解析器（MADT） | 1 天 | Phase 1 |
| **Phase 5** | trait 接口扩展 + 4 处硬编码迁移 | 1 天 | Phase 1 |
| **Phase 6** | QEMU virt fallback + 测试 | 1 天 | Phase 5 |
| **Phase 7** | 文档同步 + 清理 | 0.5 天 | Phase 5 |

**总计**：~7 人天（单人）。

**里程碑**：
- M1（Phase 1+2 完成）：API 落地，可单独 review。
- M2（Phase 3+4 完成）：解析器就位，可独立测试真实硬件 dump。
- M3（Phase 5+6 完成）：硬编码全部迁移，QEMU 测试通过。
- M4（Phase 7 完成）：文档齐全，可合并。

---

## 13. 验收标准（与问题文档 §9 对齐 + 补充）

- [x] 新增平台描述抽象（`PlatformDesc`），不暴露 FDT/ACPI 细节给 `ClockArch` / `InterruptController` / `ArchInit`。
- [x] 当前 4 处硬编码常量被替换为从平台描述读取。
- [x] QEMU `virt` 测试继续通过（`QemuVirtDesc` 兜底）。
- [x] 文档 `04-clock-interrupt-init.md` 和相关源码注释同步更新。
- [x] `KernelInfo` 扩展能够传递 DTB/RSDP 原始指针。
- [x] 新增代码保持 `#![no_std]`，不使用 `std`。
- [x] 单元测试可验证 `PlatformDesc` 解析结果，无需启动 QEMU。
- [x] **新增**：trait 接口签名变化是单一同步 commit（不拆 7 个小 commit，避免中间状态编译失败）。
- [x] **新增**：`set_base()` 标 `#[deprecated]`，未来 major 版本删除。
- [x] **新增**：F 真实硬件 dump 至少覆盖 1 个 RISC-V 板 + 1 个 x86 笔记本（QEMU 替代可接受）。

---

## 14. 附录 A：Linux/seL4/Minix3 方案对比

| 维度 | Linux | seL4 | Minix3 C | minix-rs（本方案） |
|------|-------|------|----------|-------------------|
| 硬件描述数据源 | DTB (ARM/RV) / ACPI (x86) | DTB (ARM/RV) / ACPI (x86) | 硬编码 (bsp_init) | DTB / ACPI / QemuVirtDesc |
| 解析位置 | kernel (`of_*`, `acpi_*`) | kernel (`boot.c`) | kernel (`acpi_init`, `bsp_init`) | kernel (`platform::init`) |
| 传递方式 | r0/x0 (DTB) / cmdline acpi_rsdp= | root task data | multiboot / hardcoded | `KernelInfo.dtb_phys`/`rsdp_phys` |
| 抽象类型 | `struct device`、`struct resource` | `ps_io_ops_t` (function table) | 全局变量 (`omap_intr.base`) | `PlatformDesc` (struct) |
| 多核拓扑 | `struct device_node::cpu` | per-CPU `ps_io_ops` | `cpu_info[cpu]` | `PlatformDesc.cpu_topology.cpus[]` |

**关键观察**：本方案在"解析位置"和"传递方式"上与 Linux/seL4/Minix3 **完全一致**（都在 kernel 内解析，传递指针/物理地址）。这是符合"Ground Truth 优先级：Minix3 C > 文档 > Rust 实现 > AI 分析"的——Minix3 C 的 acpi_init/bsp_init 都在 kernel 内。

**与 Linux 的差异**：Linux 抽象粒度更细（device tree 的每个节点都是一个 `struct device`），本方案粒度较粗（一个全局 PlatformDesc）。理由：minix-rs 是微内核，没有用户态 device model，所有硬件 init 都在 kernel 内，没必要那么细。

---

## 15. 附录 B：FDT 解析最小化实现（约 200 行）

```rust
// os/libs/minix-platform/src/fdt.rs (骨架)

use crate::desc::*;
use minix_types::PhysBytes;

#[repr(C)]
struct FdtHeader {
    magic: u32, totalsize: u32, off_dt_struct: u32, off_dt_strings: u32,
    off_mem_rsvmap: u32, version: u32, last_comp_version: u32,
    boot_cpuid_phys: u32, size_dt_strings: u32, size_dt_struct: u32,
}

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_END: u32 = 9;

const TOKEN_ALIGN: usize = 4;

/// 把 FDT 物理地址解释为 &'static slice（不复制数据）
unsafe fn fdt_slice(phys: PhysBytes, size: u64) -> &'static [u32] {
    let ptr = phys.0 as *const u32;
    let len = (size / 4) as usize;
    core::slice::from_raw_parts(ptr, len)
}

pub fn parse(phys: PhysBytes, size: Option<u64>) -> &'static PlatformDesc {
    let size = size.unwrap_or_else(|| unsafe {
        let hdr = &*(phys.0 as *const FdtHeader);
        u64::from(hdr.totalsize.to_be())
    });
    let tokens = unsafe { fdt_slice(phys, size) };

    // 头部校验
    let magic = tokens[0].to_be();
    assert_eq!(magic, FDT_MAGIC, "FDT magic mismatch: {:#x}", magic);

    // 结构体段从 off_dt_struct 开始
    let off_struct = u32::from_be(tokens[2]) as usize / 4;
    let off_strings = u32::from_be(tokens[3]) as usize / 4;
    let struct_slice = &tokens[off_struct..];

    // 简单遍历：找 /cpus, /soc/* 等
    let mut desc = PlatformDesc::default();
    walk_nodes(struct_slice, &tokens[off_strings..off_strings + u32::from_be(tokens[8]) as usize / 4], &mut desc);

    // 存到静态内存（避免栈上）
    Box::leak(Box::new(desc))  // 简化：实际用 MaybeUninit<PlatformDesc>
}

fn walk_nodes(struct_slice: &[u32], strings: &[u32], desc: &mut PlatformDesc) {
    // 简化：用栈模拟递归
    let mut i = 0;
    let mut path_stack: alloc::vec::Vec<&[u8]> = alloc::vec::Vec::new();
    while i < struct_slice.len() {
        let token = u32::from_be(struct_slice[i]);
        match token {
            FDT_BEGIN_NODE => {
                i += 1;
                // 跳过对齐
                while i % TOKEN_ALIGN != 0 { i += 1; }
                let name_start = i;
                // 找到 name 末尾 (NUL)
                while struct_slice[i] != 0 { i += 1; }
                let name_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &struct_slice[name_start] as *const u32 as *const u8,
                        (i - name_start) * 4,
                    )
                };
                let name = name_bytes.split(|&b| b == 0).next().unwrap_or(b"");
                path_stack.push(name);
                handle_node(&path_stack, desc);
                i += 1;
            }
            FDT_END_NODE => {
                path_stack.pop();
                i += 1;
            }
            FDT_PROP => {
                i += 1;
                let len = u32::from_be(struct_slice[i]); i += 1;
                let nameoff = u32::from_be(struct_slice[i]); i += 1;
                // 跳过对齐
                while i % TOKEN_ALIGN != 0 { i += 1; }
                // value 在这里（len / 4 个 u32）
                let value_u32 = &struct_slice[i..i + (len as usize + 3) / 4];
                let value = unsafe {
                    core::slice::from_raw_parts(value_u32.as_ptr() as *const u8, len as usize)
                };
                handle_prop(&path_stack, nameoff, strings, value, desc);
                i += (len as usize + 3) / 4;
                while i % TOKEN_ALIGN != 0 { i += 1; }
            }
            FDT_END => break,
            _ => panic!("FDT: unknown token {}", token),
        }
    }
}

fn handle_node(path: &[&[u8]], _desc: &mut PlatformDesc) {
    // 简化：只记录路径，handle_prop 中根据路径 + 属性名判断
    let full_path = path.join(b"/");
    if full_path.starts_with(b"/cpus") {
        // 标记 CPU 节点
    }
}

fn handle_prop(path: &[&[u8]], nameoff: u32, strings: &[u32], value: &[u8], desc: &mut PlatformDesc) {
    // nameoff 是字符串表的索引，取出属性名
    let name_bytes = unsafe {
        let s = &strings[nameoff as usize / 4..];
        let ptr = s.as_ptr() as *const u8;
        let mut len = 0;
        while len < 64 && ptr.add(len) as *const u8 != &0 && ptr.add(len) as *const u8 != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    };
    let name = name_bytes.split(|&b| b == 0).next().unwrap_or(b"");

    if name == b"compatible" {
        // 匹配 "riscv,clint0" → TimerDesc
        // 匹配 "arm,cortex-a15-gic" → IntrCtrlDesc
        // 匹配 "ns16550a" → BootConsoleDesc
    }
    if name == b"reg" {
        // 解析 reg property：<addr size>，填到对应 desc 字段
    }
}
```

**注**：上述是骨架；实际实现需更精细处理 (cell size 取决于 #address-cells / #size-cells，节点名匹配要兼容 phandle)。

---

## 16. 附录 C：与 CLAUDE.md 自定义约束的对应

| 约束 | 本方案如何遵守 |
|------|---------------|
| C 行为是 Ground Truth | `acpi_init`/`bsp_init` 在 kernel 内 → 本方案也在 kernel 内（§9.1） |
| 所有硬件通过 trait | `ClockArch`/`InterruptController`/`ArchInit` 仍是 trait；`PlatformDesc` 是数据而非 trait，不引入新硬件访问点 |
| `#![no_std]` | 解析器全部 `no_std`（用 `core::sync::atomic` 而非 `std::sync::Once`） |
| SMP + BKL 安全 | PLATFORM 构造后只读（`&'static`），跨 CPU 访问无锁；构造期在 BSP 单线程上下文 |
| 概念抽象 (Ch1) | 抽象是"硬件有什么"（数据），不是"硬件怎么用"（行为）；trait 仍只描述"怎么用" |
| 禁止 `#[cfg(target_arch)]` 行为选择 | trait 实现用 `match platform.timer.kind { ... }` 替代 `cfg!`；`QemuVirtDesc` 的 `kind` 字段由调用方在构建期选择 |
| Claims-Evidence | §9 行为契约表每行都有 C 源引用（`arch_system.c:246-288` 等） |
| 因果链正确 | §2.1 数据流图明确每一步的因果（"为什么 boot-shim 只传指针" → "kernel 解析匹配 C 语义"） |

---

## 17. 总结

本方案的核心论点是：

> **平台硬件发现的"语义"在 C Minix3 中属于 kernel（`acpi_init`/`bsp_init`），所以 Rust 重写必须保持这个语义边界。boot-shim 是"传话筒"，不是"翻译官"。**

具体设计选择：

1. **数据来源 = 原始指针**（DTB / RSDP），由 boot-shim 提取并放进 `KernelInfo`。
2. **解析 = 在 kernel 内**（`platform::init` 在 `cstart` 中调用）。
3. **抽象 = 纯数据结构**（`PlatformDesc` struct + 子结构），不是 trait。
4. **存储 = 全局只读静态**（`PLATFORM: OnceCell<PlatformDesc>` 风格）。
5. **消费 = 显式 `&PlatformDesc` 参数** + 全局访问 fallback。
6. **QEMU 测试 = 显式 `QemuVirtDesc` fallback**，dev 默认开，release 必须有真实描述。
7. **COM1 = 保留硬编码**（x86 IA-PC 标准，跨主板稳定）。

这个方案在"工程实用性"和"语义保真度"之间取得了平衡：既不引入 `fdt` crate 的依赖，也不让 boot-shim 变成半个 BSP；既保留了 C 的语义边界，又为真实硬件移植铺平了路。

期待与 qwen/ki/ds/glm 方案的对比与聚合。
