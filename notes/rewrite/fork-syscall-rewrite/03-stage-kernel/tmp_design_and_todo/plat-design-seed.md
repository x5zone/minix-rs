# plat-design-seed: 平台硬件发现——设计方案 v1

> **状态**: 方案设计稿（等待审议与 bagging）
> **关联文档**: `plat-discovery-problem.md`（问题描述）、`03-kmain-cstart.md`（启动阶段）、`04-clock-interrupt-init.md`（时钟/中断初始化）
> **关联源码**: `os/arch/src/riscv64/clock.rs`、`os/plat/src/riscv64/interrupt.rs`、`os/plat/src/arm64/interrupt.rs`、`os/libs/minix-boot/src/kernel_info.rs`

---

## 0. 设计前提与约束回顾

| 维度 | 约束 | 违反后果 |
|------|------|----------|
| **trait 抽象** | 所有硬件操作通过 trait，`ClockArch` / `InterruptController` 不感知 DTB/ACPI | 硬编码问题换皮解决，真实硬件仍需改代码 |
| **no_std** | 内核侧不使用 `std::`；boot-shim 侧可用 `std` 但不做结构化解析 | 违反内核运行环境假设 |
| **职责边界** | boot-shim 不变成"半个 BSP"；内核自己理解硬件描述 | 边界模糊，后续扩展困难 |
| **向后兼容** | 现有 `set_base()` 注入点保留（或迁移），QEMU `virt` 环境无代码改动即可运行 | 破坏现有工作的测试路径 |
| **Minix3 C 语义对齐** | 与 C 版 `acpi_init()` / `bsp_init()` 属于同一启动阶段的同类工作 | 概念层与 C 版脱节 |

---

## 1. 架构决策表（Architecture Decision Records）

### ADR-1: 解析归 kernel，boot-shim 只传指针 + 兜底描述符

**决策**: boot-shim 只传"原始描述块指针"（DTB/RSDP 物理地址）给 kernel，由 kernel 在 `arch_init()` 阶段解析成内部 `PlatformDesc` 实例；同时 kernel 内置一个 `QemuVirtDesc` 硬编码兜底，当指针为空或解析失败时使用。

**理由（对比 §5 的三方案）**:

| 维度 | 方案 A（boot-shim 解析，传结构化数据） | 方案 B（kernel 解析原始指针） | **方案 C（混合 = 本方案）** |
|------|---------------------------------------|-------------------------------|----------------------------|
| boot-shim 膨胀 | 🔴 变成半个 BSP，需要 FDT/ACPI parser | 🟢 只传一个 usize 指针 | 🟢 只传指针 + 可选 fallback flag |
| kernel 依赖 | 🟢 无额外依赖 | 🔴 需要 no_std DTB/ACPI parser | 🟡 需要 parser，但 QEMU 路径可以绕过 parser |
| 从 firmware 启动 | 🔴 不可行（无 boot-shim 传递结构化数据） | 🟢 可行 | 🟢 可行（有兜底） |
| 测试复杂度 | 🔴 boot-shim 集成测试复杂 | 🟡 需构造 mock DTB/ACPI blob | 🟡 同 B，但 QEMU 路径独立可测 |
| C 语义对齐 | 🔴 与 C 版 `acpi_init()` 相反（C 版在内核内部做） | 🟢 对齐 C 版 | 🟢 对齐 C 版 |

**关键论据**:

1. Minix3 C 版 `arch_init()` 内部调用 `acpi_init()`（x86）、`bsp_init()`（ARM）——板级发现明确发生在内核启动阶段，不在 bootloader。这是最有力的 C 语义对齐证据。
2. seL4、Linux 等主流系统采用相同模式：boot-loader 传递 DTB/RSDP 物理地址，kernel 自己解析。不是非主流路径。
3. `QemuVirtDesc` 兜底的设计让开发者可以先跑起来（不阻塞当前开发进度），再渐进完善 DTB/ACPI 解析器——这比"等 parser 写完才能移植"更实际。

**未决问题**:
- x86-64 的 ACPI 解析器工作量显著大于 DTB。初始版本 x86-64 可以只走 QemuVirtDesc 路径，把真 ACPI 解析作为后续里程碑（见 §10 验收标准）。

---

### ADR-2: `PlatformDesc` = 顶层胖 trait + 子 trait 拆分，而非单一大 trait 暴漏一切

**决策**: `PlatformDesc` 作为顶层入口，内部由四个子 trait 组合：

```
PlatformDesc (顶层)
├── TimerDesc            // 时钟源信息
├── InterruptCtrlDesc    // 中断控制器信息
├── UartDesc             // 早期串口信息（可选）
└── CpuTopologyDesc      // CPU 拓扑（SMP 阶段用）
```

每种子 trait 方法返回"架构中立"的枚举（不是裸地址），调用方（`ClockArch` / `InterruptController`）再根据枚举分派到具体实现。

**理由**:

1. **关注点分离** — `ClockArch` 只需要 `TimerDesc`，不需要知道中断控制器在哪。单一大 trait 会让每个实现都必须实现所有方法，违反 ISP（Interface Segregation Principle）。
2. **未来扩展** — 新增"NVMe 控制器描述"或"帧缓冲描述"时，只需加新子 trait，不改已有 trait 签名，不破坏现有 impl。
3. **避免"地址即描述"的谬误** — 只给一个 `plic_base: usize` 会丢失"这是 PLIC 还是 APLIC？"的语义信息。用枚举封装可以把"是什么 + 在哪里"一起传递。

**反模式（拒绝采纳）**:
```rust
// ❌ 不这样做：每类硬件一个全局独立 trait，调用方各自调各自的，
//    没有统一入口，生命周期和初始化顺序难控制
pub trait InterruptControllerSource { fn ic_base(&self) -> usize; }
pub trait TimerSource { fn timer_base(&self) -> usize; }
```

---

### ADR-3: `PlatformDesc` 的生命周期模型 = `&'static dyn PlatformDesc` 全局单例

**决策**: `PlatformDesc` 一旦在 `arch_init()` 早期构造完成，就是不可变的、`Send + Sync` 的静态引用。通过 `minix_plat::current_platform()` 暴露给所有子系统。

```rust
// minix_plat/src/lib.rs 或等价位置
static PLATFORM: OnceCell<&'static dyn PlatformDesc> = OnceCell::new();

pub fn set_platform(p: &'static dyn PlatformDesc) {
    PLATFORM.set(p).expect("set_platform can only be called once");
}

pub fn current_platform() -> &'static dyn PlatformDesc {
    PLATFORM.get().expect("platform not yet initialized")
}
```

**理由**:

1. **不涉及分配器** — 每个架构的 `PlatformDesc` 实现是一个 `const` 结构体（字段填好后泄漏为 `'static`），不需要堆分配。在 `no_std` 下可行。
2. **无运行时可变性** — 硬件描述不会改变（不是热插拔系统），一旦构建就是不可变的。`Cell`/`RefCell` 不需要。
3. **调用简洁** — `ClockArch::init_timer(hz)` 内部只需 `let t = current_platform().timer();`。
4. **SMP 安全** — `&'static dyn Trait` 是 `Send + Sync`，多核都能安全读。

**初始化顺序（与现有启动流程对齐）**:

```
kmain()
 ├── EarlyConsole::init()         // 先于平台发现（串口可能需要固定基址才能输出来）
 │                                  // 注意：如果串口也需要从 DTB 读，需要两阶段 init
 ├── platform_init()              // 新增：从 boot-shim 传的指针解析 PlatformDesc
 │                                  // 失败时回退 QemuVirtDesc
 ├── prot_init()                  // 保护结构（GDT/IDT/TSS...）— 不变
 ├── ClockArch::init_timer(hz)    // 内部读 current_platform().timer()
 ├── InterruptController::init()  // 内部读 current_platform().interrupt_ctrl()
 └── ArchInit::init()             // 架构杂项（CPU topology 在此阶段读）
```

> **注意事项**: `EarlyConsole` 的初始化在 `platform_init()` 之前。如果未来 `EarlyConsole` 的串口基址也需要从 DTB 读（真实硬件场景），需要两阶段 init：先用默认基址/轮询输出，待 `platform_init()` 完成后再 `EarlyConsole::reinit()`。本方案第一版不改 EarlyConsole。

---

### ADR-4: `ClockArch` / `InterruptController` 从 `PlatformDesc` 读配置，不新增 trait 参数

**决策**: 现有 trait 签名保持不变（不把 `&dyn PlatformDesc` 当参数传进 `init_timer()` / `init()`）。实现内部直接调用 `current_platform()` 读平台描述。

```rust
// os/arch/src/riscv64/clock.rs  — 迁移后
impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        let desc = current_platform().timer();   // 从平台描述读
        let base = desc.mmio_base();              // CLINT 基址
        let freq = desc.frequency();              // 输入频率
        // ... 后续用 base/freq 配置
    }

    fn read_ticks() -> u64 {
        let desc = current_platform().timer();
        unsafe { core::ptr::read_volatile((desc.mmio_base() + MTIME_OFF) as *const u64) }
    }
}
```

**理由**:

1. **最小侵入现有代码** — 已有的 `ClockArch` trait 定义不需要改，所有调用点不变。
2. **调用方不需要关心平台描述** — 从 `kmain` 视角看，`ClockArch::init_timer(hz)` 和以前一样，不需要多传一个参数。
3. **测试友好** — 测试中可以用 `set_platform(&MockPlatform)` 注入 mock 描述，测试 `Riscv64ClockArch` 在不同硬件配置下的行为。

**权衡**: 这把 `PlatformDesc` 变成了全局单例依赖。但：
- 平台描述本质上就是全局不变的事实（机器只有一套硬件），用单例符合语义。
- 我们显式声明了初始化顺序（`platform_init()` 必须在 `ClockArch::init_timer()` 之前），不是"任意时刻可以被访问的魔法全局"。

---

### ADR-5: KernelInfo 扩展 = `plat_desc_ptr` (usize) + `plat_desc_kind` (枚举)

**决策**: 新增两个字段到 `minix-boot/src/kernel_info.rs` 的 `KernelInfo`（或等价结构体）：

```rust
/// 平台描述原始块的来源
#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PlatformDescKind {
    None = 0,         // boot-shim 未提供，kernel 走 QemuVirtDesc 兜底
    DeviceTree = 1,   // plat_desc_ptr 指向 DTB 物理地址（ARM/RISC-V）
    AcpiRsdp = 2,     // plat_desc_ptr 指向 RSDP 物理地址（x86-64）
}

pub struct KernelInfo {
    // ... 现有字段（memmap、kernel_phys_start、bsp_hartid 等）不变

    /// 平台描述原始块的物理地址（若 kind == None 则忽略）
    pub plat_desc_ptr: usize,

    /// 原始块的类型
    pub plat_desc_kind: PlatformDescKind,
}
```

**理由**:

1. **最小扩展** — 只加两个字段，不破坏现有 `KernelInfo` 的含义和大小（对现有 boot-shim/kernel ABI 影响最小）。
2. **显式区分来源** — 不需要 kernel 去"猜"传进来的是 DTB 还是 ACPI RSDP。
3. **前向兼容** — 未来可以加新的 `PlatformDescKind`（如 SMBIOS、Multiboot 标签等）。

**boot-shim 实现摘要**:
- RISC-V: OpenSBI 通常把 DTB 地址放在 `a1` 寄存器，boot-shim 已捕获 → 存入 `plat_desc_ptr`，kind = DeviceTree。
- ARM64: UEFI `ConfigTable` 中搜索 `FDT` GUID → 存入 `plat_desc_ptr`，kind = DeviceTree。
- x86-64: UEFI `ConfigTable` 中搜索 `ACPI 2.0` GUID（`EFI_ACPI_20_TABLE_GUID`）取 RSDP → 存入 `plat_desc_ptr`，kind = AcpiRsdp。
- 如果找不到（或 boot-shim 不支持此架构）：kind = None，kernel 走 QemuVirtDesc。

---

### ADR-6: 初始实现优先交付 "QemuVirtDesc + 空的 DTB/ACPI 解析器骨架"，而非一开始就写完整的 DTB 解析器

**决策**: 第一阶段里程碑（MVP）交付：

| 项 | 状态 |
|----|------|
| `PlatformDesc` / 子 trait 定义 | ✅ 完成 |
| `QemuVirtDesc` 三架构硬编码兜底 | ✅ 完成（CLINT 0x200_0000，PLIC 0x0C00_0000，GIC 0x0800_0000 等） |
| `Riscv64ClockArch` 从 `PlatformDesc` 读基址和频率 | ✅ 完成 |
| `Riscv64InterruptController` 从 `PlatformDesc` 读基址 | ✅ 完成 |
| `AArch64InterruptController` 从 `PlatformDesc` 读基址 | ✅ 完成 |
| `KernelInfo` 扩展 + boot-shim 传递 DTB/RSDP 指针 | ✅ 完成 |
| `platform_init()` 解析流程（含兜底回退） | ✅ 完成 |
| 简单 DTB parser（仅解析 `compatible`、`reg`、`clocks` 节点） | 🔶 第一阶段不完成；以空骨架占位 |
| 简单 ACPI parser（仅解析 MADT 获取 LAPIC/IOAPIC） | 🔶 第一阶段不完成；以空骨架占位 |

**理由**:

1. **渐进迁移** — 第一阶段的价值是"架构抽象就位 + 现有代码从硬编码迁移到 trait 读配置"。即使 DTB 解析器还没写好，这个迁移本身就是价值：消除了 `const CLINT_MTIME: usize = 0x200_BFF8` 这种散落各处的假设。
2. **降低风险** — DTB 解析器的实现有独立复杂度（需要处理 flattened device tree binary 格式、字符串表、`#address-cells`/`#size-cells`）。把它拆成独立里程碑，让架构设计的正确性和解析器的正确性可以独立评审。
3. **QEMU 测试路径不变** — 第一阶段结束后，`cargo run --arch riscv64` 与以前一样工作，只是代码内部从 `QemuVirtDesc` 读地址而不是 `const`。

---

## 2. 详细 API 设计

### 2.1 平台描述数据类型

```rust
// minix-plat/src/desc.rs （新文件）
// #![no_std]

use core::fmt;

// ── 时钟源 ───────────────────────────────────────────────

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TimerKind {
    Clint,          // RISC-V CLINT (mtime)
    GenericTimer,   // ARM Generic Timer (CNTFRQ_EL0 / CNTPCT_EL0)
    Pit8254,        // x86 i8254 PIT (I/O port 0x40)
    LapicTimer,     // x86 Local APIC Timer (MMIO)
}

pub trait TimerDesc {
    fn kind(&self) -> TimerKind;
    fn mmio_base(&self) -> Option<usize>;     // None = 通过 CPU 寄存器访问（如 ARM Generic Timer）
    fn mmio_size(&self) -> Option<usize>;     // 寄存器区域大小（用于 sanity check）
    fn input_frequency_hz(&self) -> u64;
}

// ── 中断控制器 ───────────────────────────────────────────

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InterruptCtrlKind {
    Plic,           // RISC-V Platform-Level Interrupt Controller
    Aplic,          // RISC-V Advanced PLIC（未来扩展）
    GicV2,          // ARM Generic Interrupt Controller v2
    GicV3,          // ARM Generic Interrupt Controller v3
    Pic8259,        // x86 Legacy 8259A PIC
    IoApic,         // x86 I/O APIC
}

pub trait InterruptCtrlDesc {
    fn kind(&self) -> InterruptCtrlKind;

    /// 主 MMIO 基址。
    /// - PLIC = PLIC 基址
    /// - GICv3 = Distributor (GICD) 基址
    /// - GICv2 = GIC 基址（Distributor 在此 + 偏移）
    /// - IOAPIC = IOAPIC 基址
    fn primary_mmio_base(&self) -> Option<usize>;

    /// 次 MMIO 基址（架构特定，没有则为 None）。
    /// - GICv3 = Redistributor (GICR) 基址
    /// - x86 Local APIC = LAPIC 基址（通常 0xFEE0_0000）
    fn secondary_mmio_base(&self) -> Option<usize>;

    /// 该控制器支持的最大 IRQ 号（不含本地中断）。
    fn max_external_irqs(&self) -> usize;
}

// ── 早期串口 ─────────────────────────────────────────────

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UartKind {
    Ns16550Mmio,    // MMIO 16550（ARM64 virt、RISC-V virt）
    Ns16550Port,    // I/O port 16550（x86-64 COM1，0x3F8）
    SbiPutchar,     // RISC-V OpenSBI ecall
}

pub trait UartDesc {
    fn kind(&self) -> UartKind;
    fn mmio_base(&self) -> Option<usize>;     // MMIO 架构的基址
    fn io_port(&self) -> Option<u16>;         // I/O port 架构的端口号
    fn frequency_hz(&self) -> u32;            // 输入时钟频率（用于波特率计算）
}

// ── CPU 拓扑 ─────────────────────────────────────────────

#[derive(Copy, Clone, Debug)]
pub struct CpuNode {
    pub hartid: u32,       // RISC-V = hart ID；x86 = APIC ID；ARM = MPIDR affinity
    pub online: bool,      // 是否启用（DTB 中 status = "okay"）
}

pub trait CpuTopologyDesc {
    fn cpu_count(&self) -> usize;
    fn bsp_id(&self) -> u32;          // Boot CPU 的 hartid/apic id
    fn cpu(&self, index: usize) -> Option<CpuNode>;
}

// ── 顶层组合 trait ───────────────────────────────────────

pub trait PlatformDesc: Send + Sync + core::fmt::Debug {
    fn timer(&self) -> &dyn TimerDesc;
    fn interrupt_ctrl(&self) -> &dyn InterruptCtrlDesc;
    fn uart(&self) -> Option<&dyn UartDesc>;
    fn cpu_topology(&self) -> Option<&dyn CpuTopologyDesc>;

    /// 人类可读的平台标识（如 "qemu-virt-riscv64"）。
    fn name(&self) -> &'static str;
}
```

**设计要点**:

1. 子 trait 返回 `Option` — 某些平台可能没有某类硬件（或在当前阶段不需要读）。
2. `PlatformDesc` 要求 `Send + Sync` — 满足 ADR-3 的 SMP 安全要求。
3. `'static` lifetime — `QemuVirtDesc` 是 `const` 数据结构；DTB 解析出的描述是一次性构造后泄漏为 `'static`。

---

### 2.2 `QemuVirtDesc` 的具体实现

```rust
// minix-plat/src/qemu_virt.rs （新文件）
// #![no_std]

use crate::desc::*;

// ── RISC-V 64 QEMU virt ──────────────────────────────────

pub const RV64_QEMU_CLINT_BASE: usize = 0x0200_0000;
pub const RV64_QEMU_CLINT_SIZE: usize = 0x0001_0000;   // 64 KiB
pub const RV64_QEMU_CLINT_FREQ: u64    = 10_000_000;   // 10 MHz
pub const RV64_QEMU_PLIC_BASE:  usize = 0x0C00_0000;
pub const RV64_QEMU_PLIC_NR_IRQS: usize = 54;

pub struct Riscv64QemuVirtTimer;
pub struct Riscv64QemuVirtInterruptCtrl;

impl TimerDesc for Riscv64QemuVirtTimer {
    fn kind(&self) -> TimerKind { TimerKind::Clint }
    fn mmio_base(&self) -> Option<usize> { Some(RV64_QEMU_CLINT_BASE) }
    fn mmio_size(&self) -> Option<usize> { Some(RV64_QEMU_CLINT_SIZE) }
    fn input_frequency_hz(&self) -> u64 { RV64_QEMU_CLINT_FREQ }
}

impl InterruptCtrlDesc for Riscv64QemuVirtInterruptCtrl {
    fn kind(&self) -> InterruptCtrlKind { InterruptCtrlKind::Plic }
    fn primary_mmio_base(&self) -> Option<usize> { Some(RV64_QEMU_PLIC_BASE) }
    fn secondary_mmio_base(&self) -> Option<usize> { None }
    fn max_external_irqs(&self) -> usize { RV64_QEMU_PLIC_NR_IRQS }
}

pub struct Riscv64QemuVirtDesc {
    timer: Riscv64QemuVirtTimer,
    ic:    Riscv64QemuVirtInterruptCtrl,
}

// 一个 const 实例，泄漏为 'static
pub const RISCV64_QEMU_VIRT: Riscv64QemuVirtDesc = Riscv64QemuVirtDesc {
    timer: Riscv64QemuVirtTimer,
    ic:    Riscv64QemuVirtInterruptCtrl,
};

impl PlatformDesc for Riscv64QemuVirtDesc {
    fn name(&self) -> &'static str { "qemu-virt-riscv64" }
    fn timer(&self) -> &dyn TimerDesc { &self.timer }
    fn interrupt_ctrl(&self) -> &dyn InterruptCtrlDesc { &self.ic }
    fn uart(&self) -> Option<&dyn UartDesc> { None }
    fn cpu_topology(&self) -> Option<&dyn CpuTopologyDesc> { None }
}

// ── ARM64 QEMU virt ────────────────────────────────────
// (对称实现，略去琐碎字段赋值；GICD = 0x0800_0000，GICR = 0x080A_0000)
// ── x86-64 QEMU ────────────────────────────────────────
// (PIT i8254 + LAPIC/IOAPIC，略)
```

---

### 2.3 `platform_init()` 的完整流程

```
输入: KernelInfo.plat_desc_ptr, KernelInfo.plat_desc_kind
输出: 全局 PLATFORM cell 被设置

Step 1: 根据 kind 分派
    ├── None        → goto Step 4 (QemuVirtDesc fallback)
    ├── DeviceTree  → goto Step 2 (DTB 解析)
    └── AcpiRsdp    → goto Step 3 (ACPI 解析)

Step 2: DTB 解析（第一阶段 = 占位，直接返回 Err）
    a. 验证 DTB header magic (0xD00DFEED)
    b. 解析 /soc 节点下的 interrupt-controller（PLIC/GIC）
    c. 解析 /cpus 节点下的 cpu 节点（hartid）
    d. 解析 /chosen 的 stdout-path（串口）
    e. 组装 PlatformDesc
    f. 成功 → set_platform() 并 return
    g. 失败 → goto Step 4

Step 3: ACPI 解析（第一阶段 = 占位，直接返回 Err）
    a. 验证 RSDP 签名 "RSD PTR "
    b. 遍历 RSDT/XSDT 找 MADT（APIC）
    c. 从 MADT 提取 LAPIC / IOAPIC 条目
    d. 组装 PlatformDesc
    e. 成功 → set_platform() 并 return
    f. 失败 → goto Step 4

Step 4: QemuVirtDesc 兜底
    a. 根据当前 target_arch 选择 Riscv64QemuVirtDesc / AArch64QemuVirtDesc / X86_64QemuVirtDesc
    b. set_platform()
    c. 输出警告日志："Using QEMU virt fallback platform description"

Step 5: 完成 — 后续 ClockArch::init_timer / InterruptController::init
        等函数可以安全调用 current_platform()
```

**为什么解析失败不 panic？** — kernel 启动是不可回退过程。解析失败说明 boot-shim 传了一个坏的 DTB/RSDP 或 kernel 解析器有 bug。在调试阶段我们希望 kernel 还能起来（即使走到 fallback）以便定位问题。未来当解析器稳定后，可以把 fallback 去掉，解析失败直接 panic。

---

### 2.4 现有 `Riscv64ClockArch` 的迁移示例

**迁移前**（`os/arch/src/riscv64/clock.rs`）:

```rust
const CLINT_MTIME:    usize = 0x200_BFF8;   // ❌ 硬编码
const CLINT_MTIMECMP: usize = 0x200_4000;   // ❌ 硬编码
const MTIME_FREQ:     u64   = 10_000_000;   // ❌ 硬编码

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        let mtime: u64 = unsafe { core::ptr::read_volatile(CLINT_MTIME as *const u64) };
        let interval = MTIME_FREQ / hz as u64;
        // ...
    }
    fn read_ticks() -> u64 { /* 用 CLINT_MTIME */ }
}
```

**迁移后**:

```rust
// CLINT 寄存器偏移（相对于 CLINT 基址）— 这些是架构常量，不是板级地址 ✅
const CLINT_MTIME_OFF:    usize = 0xBFF8;   // mtime 寄存器偏移（64-bit）
const CLINT_MTIMECMP_OFF: usize = 0x4000;   // mtimecmp 寄存器偏移（per-hart）

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        let desc = current_platform().timer();
        assert_eq!(desc.kind(), TimerKind::Clint, "Riscv64ClockArch requires CLINT timer");

        let base = desc.mmio_base().expect("CLINT requires MMIO base");
        let freq = desc.input_frequency_hz();

        let mtime = unsafe { core::ptr::read_volatile((base + CLINT_MTIME_OFF) as *const u64) };
        let interval = freq / hz as u64;
        unsafe {
            core::ptr::write_volatile((base + CLINT_MTIMECMP_OFF) as *mut u64, mtime + interval);
        }
        // ...
    }

    fn read_ticks() -> u64 {
        let base = current_platform().timer().mmio_base().unwrap();
        unsafe { core::ptr::read_volatile((base + CLINT_MTIME_OFF) as *const u64) }
    }
}
```

**关键变化**:

| 项目 | 迁移前 | 迁移后 |
|------|--------|--------|
| 基址来源 | `const CLINT_MTIME` 硬编码 | `current_platform().timer().mmio_base()` |
| 频率来源 | `const MTIME_FREQ` 硬编码 | `current_platform().timer().input_frequency_hz()` |
| 换硬件时 | 改三处 `const`，重新编译 | 换一个 `PlatformDesc` 实现（或 DTB 内容），不改 `clock.rs` |
| QEMU 路径 | 直接用常量（巧合正确） | 通过 `QemuVirtDesc` 间接读到相同值，行为一致 |

---

### 2.5 现有 `InterruptController` 的迁移示例

**迁移前**（`os/plat/src/riscv64/interrupt.rs`）:

```rust
const PLIC_BASE: usize = 0x0C00_0000;  // ❌ 硬编码

impl Riscv64InterruptController {
    pub const fn new() -> Self {
        Self { plic_base: PLIC_BASE, /* ... */ }
    }
    // 提供 set_base() 作为外部注入点
    pub fn set_base(&mut self, plic_base: usize) { /* ... */ }
}
```

**迁移后**:

```rust
impl Riscv64InterruptController {
    pub fn new() -> Self {
        let desc = current_platform().interrupt_ctrl();
        assert_eq!(desc.kind(), InterruptCtrlKind::Plic,
                   "Riscv64InterruptController requires PLIC interrupt controller");
        let base = desc.primary_mmio_base()
            .expect("PLIC requires primary MMIO base");
        Self {
            plic_base: base,
            nr_irqs: desc.max_external_irqs(),
            context: 1,
            last_claimed: 0,
        }
    }

    // 保留 set_base() 但标记 deprecated，给外部注入/测试使用
    #[deprecated(note = "Platform info should come from PlatformDesc, not set_base()")]
    pub fn set_base(&mut self, plic_base: usize) { self.plic_base = plic_base; }
}
```

**注意**: `new()` 从 `const fn` 变成普通 `fn` — 因为它调用了 `current_platform()`（运行时读）。这不破坏任何调用方，因为 `new()` 的调用点都是启动期的普通函数调用。

---

## 3. 文件布局与模块归属

```
os/libs/minix-plat/          ← 新增 crate（平台发现抽象）
├── Cargo.toml
└── src/
    ├── lib.rs                    // pub mod desc, pub mod qemu_virt;
    │                             // pub fn current_platform() → &'static dyn PlatformDesc
    │                             // pub fn set_platform(...)
    │                             // pub fn platform_init(kinfo)
    ├── desc.rs                   // PlatformDesc / TimerDesc / InterruptCtrlDesc / ...
    ├── qemu_virt.rs              // Riscv64QemuVirtDesc / AArch64QemuVirtDesc / X86_64QemuVirtDesc
    ├── fdt.rs                    // DTB 解析器（第一阶段 = 占位，返回 Err）
    └── acpi.rs                   // ACPI 解析器（第一阶段 = 占位，返回 Err）

os/libs/minix-boot/src/kernel_info.rs   // 扩展：新增 plat_desc_ptr, plat_desc_kind

os/arch/src/riscv64/clock.rs            // 迁移：读 PlatformDesc.timer()
os/plat/src/riscv64/interrupt.rs        // 迁移：读 PlatformDesc.interrupt_ctrl()
os/plat/src/arm64/interrupt.rs          // 迁移：读 PlatformDesc.interrupt_ctrl()
os/arch/src/x86_64/arch_init.rs         // 迁移：platform_init() 在这里被调用
```

**crate 依赖**:

```toml
# minix-plat/Cargo.toml
[dependencies]
# 无外部依赖（no_std 下的纯 Rust 实现）
```

`minix-plat` crate 的引入不会给 kernel 增加外部依赖。`minix-boot` 只需简单地把 DTB/RSDP 地址塞到 `KernelInfo` 中，不涉及 DTB 解析。

---

## 4. 启动时序的精确说明（与现有文档对齐）

```
时间线（与 03-kmain-cstart.md §1.8 / 04-clock-interrupt-init.md §1 对齐）

  kmain()
    │
    ├─ EarlyConsole::init()       ── 用默认基址输出（保持不变；两阶段 init 延后）
    │
    ├─ platform_init(kinfo)        ── 新增阶段：解析 DTB/ACPI，构建 PlatformDesc
    │                               │   Step 1: 检查 plat_desc_kind
    │                               │   Step 2: 尝试 DTB/ACPI 解析
    │                               │   Step 3: 解析失败 → QemuVirtDesc 兜底
    │                               └─ set_platform(&'static desc)   ◀─ 此刻起 current_platform() 可用
    │
    ├─ prot_init()                  ── 保护模式/GDT/IDT/TSS/异常向量（保持不变）
    │
    ├─ ClockArch::init_timer(hz)    ── 内部调用 current_platform().timer() ◀─ 新读点
    │
    ├─ InterruptController::new()   ── 内部调用 current_platform().interrupt_ctrl() ◀─ 新读点
    │   + init()
    │
    └─ ArchInit::init()             ── 架构杂项；可在此阶段读取 cpu_topology()（SMP 阶段）
```

**关键不变式**:

- `platform_init()` 必须发生在 **任何 trait 读平台描述之前**。
- `EarlyConsole` 在 `platform_init()` 之前 — 因此 EarlyConsole 目前仍使用架构默认基址（`0x3F8` for x86，SBI ecall for RISC-V，PL011 固定基址 for ARM64）。这与 §ADR-3 的注意事项一致。
- 一旦 `platform_init()` 返回，所有后续代码可以随意调用 `current_platform()`。

---

## 5. 错误处理策略

| 错误场景 | 策略 | 日志/断言 |
|----------|------|-----------|
| DTB header magic 不匹配 | 回退 `QemuVirtDesc` | WARN: "Invalid DTB magic, using QEMU virt fallback" |
| DTB 解析器在第一阶段未实现 | 回退 `QemuVirtDesc` | WARN: "DTB parser stub — using QEMU virt fallback" |
| ACPI RSDP signature 不匹配 | 回退 `QemuVirtDesc` | WARN: "Invalid RSDP signature, using QEMU virt fallback" |
| ACPI 解析器在第一阶段未实现 | 回退 `QemuVirtDesc` | WARN: "ACPI parser stub — using QEMU virt fallback" |
| `ClockArch` 实现读到的 `TimerKind` 不匹配 | `panic!`（编程错误） | ASSERT: "XxxClockArch requires YYY timer" |
| `InterruptController` 实现读到的 `InterruptCtrlKind` 不匹配 | `panic!`（编程错误） | ASSERT: "XxxInterruptController requires YYY" |
| `current_platform()` 被调用但 `platform_init()` 未跑 | `panic!`（初始化顺序错误） | ASSERT: "platform not yet initialized — check boot sequence" |

**关键原则**:
- **硬件描述缺失/不正确** → 尝试 fallback，不 panic（除非 fallback 也不可用）。
- **trait 实现与描述语义冲突** → panic（这是编程错误，不是硬件问题）。
- **初始化顺序错误** → panic（这是启动逻辑 bug）。

---

## 6. 测试策略

### 6.1 单元测试: mock PlatformDesc

```rust
// os/plat/src/riscv64/interrupt.rs — tests 模块（已有的 tests 模块改造）
#[cfg(test)]
mod tests {
    use super::*;
    use minix_plat::desc::*;
    use minix_plat::{set_platform, current_platform};

    // ── Mock 描述 ──
    struct MockTimer { base: usize, freq: u64 }
    impl TimerDesc for MockTimer {
        fn kind(&self) -> TimerKind { TimerKind::Clint }
        fn mmio_base(&self) -> Option<usize> { Some(self.base) }
        fn mmio_size(&self) -> Option<usize> { Some(0x10000) }
        fn input_frequency_hz(&self) -> u64 { self.freq }
    }

    struct MockIc { base: usize, nr_irqs: usize }
    impl InterruptCtrlDesc for MockIc {
        fn kind(&self) -> InterruptCtrlKind { InterruptCtrlKind::Plic }
        fn primary_mmio_base(&self) -> Option<usize> { Some(self.base) }
        fn secondary_mmio_base(&self) -> Option<usize> { None }
        fn max_external_irqs(&self) -> usize { self.nr_irqs }
    }

    struct MockPlatform {
        timer: MockTimer,
        ic: MockIc,
    }
    impl PlatformDesc for MockPlatform {
        fn name(&self) -> &'static str { "mock" }
        fn timer(&self) -> &dyn TimerDesc { &self.timer }
        fn interrupt_ctrl(&self) -> &dyn InterruptCtrlDesc { &self.ic }
        fn uart(&self) -> Option<&dyn UartDesc> { None }
        fn cpu_topology(&self) -> Option<&dyn CpuTopologyDesc> { None }
    }

    #[test]
    fn new_uses_platform_desc_base() {
        // 把 mock 泄漏为 'static（测试场景下的合法用法）
        let mock: &'static dyn PlatformDesc = Box::leak(Box::new(MockPlatform {
            timer: MockTimer { base: 0x1234_0000, freq: 8_000_000 },
            ic:    MockIc    { base: 0x5678_0000, nr_irqs: 32 },
        }));
        set_platform(mock);

        let ic = Riscv64InterruptController::new();
        assert_eq!(ic.plic_base, 0x5678_0000);
        assert_eq!(ic.nr_irqs, 32);
    }
}
```

### 6.2 集成测试: `platform_init()` 回退路径

- 测试 `platform_init(kind=None)` → 全局 PLATFORM = QemuVirtDesc
- 测试 `platform_init(kind=DeviceTree, ptr=无效地址)` → 回退 QemuVirtDesc
- 测试 `platform_init(kind=AcpiRsdp, ptr=无效地址)` → 回退 QemuVirtDesc

### 6.3 未来: 真实 DTB blob 解析测试（DTB/ACPI parser 完成后）

- 把一个小的 QEMU virt DTB blob 以 `include_bytes!()` 嵌入测试
- 让 DTB 解析器跑在这个 blob 上
- 断言 CLINT/PLIC 基址、CPU hartid、串口地址与 QEMU 约定一致

---

## 7. 对 SMP 阶段的前向兼容（给 15-smp.md 的接口预留）

本方案的 `CpuTopologyDesc` 虽然在当前阶段返回 `None`（`QemuVirtDesc` 暂不实现 CPU 拓扑），但为 SMP 阶段预留了以下信息：

| SMP 阶段需要的信息 | 从哪里获得 |
|--------------------|-----------|
| CPU 数量 | `CpuTopologyDesc::cpu_count()` |
| BSP hartid / APIC ID | `CpuTopologyDesc::bsp_id()` |
| 每个 hart 的 id 列表 | `CpuTopologyDesc::cpu(i)` |
| 哪些 CPU 已启用 | `CpuNode.online` |

SMP 阶段的 `smp_init()` 可以直接：
```rust
let topo = current_platform().cpu_topology()
    .expect("SMP requires CPU topology description");
for i in 0..topo.cpu_count() {
    let cpu = topo.cpu(i).unwrap();
    if cpu.online && cpu.hartid != topo.bsp_id() {
        wake_up_ap(cpu.hartid);
    }
}
```

---

## 8. 向后兼容性声明

| 项目 | 影响 | 说明 |
|------|------|------|
| `Riscv64InterruptController::set_base()` | 保留但 deprecated | 用于测试注入；不推荐普通路径调用 |
| `AArch64InterruptController::set_base()` | 保留但 deprecated | 同上 |
| `ClockArch` trait 签名 | 不变 | `fn init_timer(hz: u32)` 签名保持 |
| `InterruptController` trait 签名 | 不变 | `fn init(&mut self)` 等保持 |
| `KernelInfo` 大小/布局 | **扩展了两个字段** | 需要同步更新 boot-shim 侧的结构体定义 |
| QEMU `virt` 测试路径 | **零破坏** | `QemuVirtDesc` 兜底读到的值与旧 `const` 完全一致 |

---

## 9. 里程碑与拆分建议（给实施阶段的进度拆解参考）

| 里程碑 | 内容 | 验收 |
|--------|------|--------|
| **M1: PlatformDesc 骨架** | 定义 `PlatformDesc` + 子 trait + `current_platform()`/`set_platform()` + `QemuVirtDesc` 三架构 | 编译通过；`cargo test --package minix-plat` 通过 |
| **M2: KernelInfo 扩展** | boot-shim 传递 DTB/RSDP 指针；kernel 侧 `platform_init()` 解析骨架（永远回退 QemuVirtDesc） | kernel 可以在三架构下启动并输出 "Using QEMU virt fallback" 日志 |
| **M3: ClockArch 迁移** | `Riscv64ClockArch` 从 `PlatformDesc` 读基址/频率 | QEMU riscv64 时钟中断正常工作 |
| **M4: InterruptController 迁移** | `Riscv64InterruptController` / `AArch64InterruptController` 从 `PlatformDesc` 读基址 | QEMU riscv64/arm64 中断控制器正常工作 |
| **M5: DTB 解析器** | 实现 `fdt.rs` 中至少 `compatible` / `reg` / `#address-cells` / `#size-cells` / `cpus` 节点解析 | 在 RISC-V / ARM64 QEMU virt 上可以从真实 DTB 读出与 QemuVirtDesc 相同的值（日志从 "fallback" 变为 "parsed from DTB"） |
| **M6: ACPI 解析器** | 实现 `acpi.rs` 中至少 RSDP → RSDT → MADT 解析 | x86-64 上可以从 ACPI 读出 LAPIC/IOAPIC 基址（日志从 "fallback" 变为 "parsed from ACPI"） |
| **M7: EarlyConsole 两阶段 init** | `EarlyConsole::init()` 先用默认基址输出；`EarlyConsole::reinit()` 从 PlatformDesc 读真实串口基址 | 真实硬件上 EarlyConsole 可以对非标准串口地址正常输出 |

**并行性**: M1/M2 可以独立做。M3/M4 依赖 M1。M5/M6 依赖 M2（有 DTB/RSDP 指针之后才需要 parser）。M7 可以在任何时刻独立做。

---

## 10. 验收标准（对应问题描述 §9 的重述与细化）

- [x] 新增 `PlatformDesc` 抽象，`ClockArch` / `InterruptController` 不感知 DTB/ACPI
- [x] 当前 4 处硬编码常量（CLINT mtime/mtimecmp base、PLIC base、GICD/GICR base、ACPI）被替换为从平台描述读取
- [x] QEMU `virt` 测试继续通过（`QemuVirtDesc` 兜底保证）
- [x] `KernelInfo` 可以传递 DTB/RSDP 原始指针（M2 完成后）
- [x] 新增代码保持 `#![no_std]`
- [x] 单元测试可验证 `PlatformDesc` 注入行为（mock PlatformDesc + 测试 `InterruptController::new()` 读取正确基址）
- [ ] DTB 解析器（M5 完成后）可在单元测试中用 `include_bytes!()` 的真实 DTB blob 验证
- [ ] ACPI 解析器（M6 完成后）可在单元测试中用 mock RSDP/MADT blob 验证

---

## 11. 未解决的开放问题（留给后续讨论 / bagging）

1. **`EarlyConsole` 两阶段 init 是否纳入本方案范围？** — 本方案将其标记为 M7 独立里程碑；是否与 M1-M4 一起做，还是延迟到 SMP 之后？
2. **`PlatformDesc` 的不可变性** — 本方案假设平台描述是不可变的（硬件不会热插拔）。如果未来支持设备热插拔（如 PCIe），需要重新审视这一假设。目前的微内核场景不涉及，暂时没问题。
3. **`'static` 泄漏 vs. 真正的全局分配** — 本方案用 `Box::leak`（测试）或 `&'static QEMU_VIRT`（正式路径）把描述变成 `'static`。如果内核需要动态卸载/替换描述（如 DTB 解析成功后替换之前的 QemuVirtDesc），`OnceCell::set` 的一次性语义会成为限制。目前我们假设不需要替换，因此 `OnceCell` 足够。
4. **x86-64 ACPI 解析器的复杂度** — x86 的 RSDP 可能在 EBDA / BIOS ROM 两个位置搜索（非 UEFI 场景）；本方案假设 boot-shim 已经在 UEFI `ConfigTable` 中找到了 RSDP。如果未来支持 legacy BIOS 启动，需要额外的 RSDP 搜索逻辑。
5. **`set_platform` 的可测试性** — 在 `cargo test` 中（多线程环境），`OnceCell` 的一次性语义意味着同一个 test binary 中不同测试用例不能各自注入不同的 mock PlatformDesc（只有第一个 `set_platform` 成功，后续会 panic）。解决路径有二：(a) 给每个测试 `cargo test --test xxx` 独立的 binary；(b) `PLATFORM` 改成 `RefCell<Option<&'static ...>>`（单线程测试场景下可行，但违反 SMP `Send+Sync`）—— 倾向于 (a)，把集成测试拆成独立 binary。

---

## 12. 关键变更速查（30 秒理解整体影响）

```
新增 1 个 crate:  minix-plat          // PlatformDesc 抽象 + QemuVirtDesc + fdt/acpi 骨架
修改 1 个 struct: minix-boot/KernelInfo // +plat_desc_ptr, +plat_desc_kind
修改 3 个 impl:   Riscv64ClockArch      // 从 current_platform().timer() 读
                  Riscv64InterruptCtrl  // 从 current_platform().interrupt_ctrl() 读
                  AArch64InterruptCtrl  // 从 current_platform().interrupt_ctrl() 读
新增 1 个函数:    platform_init(kinfo)  // 在 kmain() 中 EarlyConsole 之后、prot_init 之前调用
```

对 QEMU `virt` 测试路径的行为变化: **零**（`QemuVirtDesc` 读出的基址/频率与旧的 `const` 完全相同）。

---

*[End of plat-design-seed.md]*
