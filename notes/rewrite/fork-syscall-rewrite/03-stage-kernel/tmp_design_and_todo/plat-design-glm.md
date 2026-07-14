# plat-design-glm: 平台硬件发现抽象设计（GLM 独立方案）

> **分类**: Kernel 架构抽象 / 硬件发现
> **状态**: 设计方案（待 bagging 汇总后择优实施）
> **作者**: GLM（独立完成，未参考其他 `plat-design-*.md`）
> **关联文档**: `plat-discovery-problem.md`（问题陈述）、`00-kernel-overview.md`、`01-boot-shim-bootstrap.md`、`03-kmain-cstart.md`、`04-clock-interrupt-init.md`
> **关联源码**:
> - `os/arch/src/riscv64/clock.rs`、`os/arch/src/arm64/clock.rs`、`os/arch/src/x86_64/clock.rs`
> - `os/plat/src/riscv64/interrupt.rs`、`os/plat/src/arm64/interrupt.rs`、`os/plat/src/x86_64/interrupt.rs`
> - `os/arch/src/x86_64/arch_init.rs`、`os/arch/src/arch/arch_init.rs`、`os/arch/src/arch/clock.rs`
> - `os/libs/minix-boot/src/kernel_info.rs`、`os/libs/minix-boot/src/boot_shim.rs`
> - `os/boot-shim/src/uefi_helpers.rs`、`os/boot-shim/src/opensbi_helpers.rs`
> - `os/kernel/src/lib.rs`（`kmain` / `init_clock_and_interrupts`）

---

## 0. TL;DR — 八个关键问题的回答速查

| # | 问题（`plat-discovery-problem.md` §8） | GLM 决策 |
|---|----------------------------------------|---------|
| 1 | **归属**：boot-shim 还是 kernel？ | **方案 C（混合）**：boot-shim 只"定位"原始 DTB/RSDP 物理指针并填入 `KernelInfo`；kernel 负责"解析 + 使用"。QEMU 测试用 `QemuVirtDesc` 兜底。 |
| 2 | **命名** | `PlatformDesc`（根 trait）+ 值类型子描述符（`InterruptControllerDesc` / `TimerDesc` / `ConsoleDesc` / `CpuTopology`）。 |
| 3 | **trait 形状** | 根 trait 暴露类型化访问器返回**值类型子结构**（非子 trait），避免接口膨胀与 N 参数传递。 |
| 4 | **传递方式** | 解析一次 → 构造硬件实例 → 存入 `&'static PlatformContext` 全局；`init()` 接收 `&PlatformDesc`/子描述符参数。运行时热路径（`read_ticks`）经全局访问。 |
| 5 | **解析时机** | kernel 解析原始 DTB/RSDP（方案 C 的 kernel 侧）。`KernelInfo` 只加一个 `Option<PlatformDescriptorPtr>` 字段。 |
| 6 | **x86 路径** | ACPI 为主；DTB 不纳入首期范围（抽象允许后续扩展）。QEMU virt x86 不依赖 ACPI，走 `QemuVirtDesc`。 |
| 7 | **多核扩展** | 描述符形状**预留** per-CPU 字段（GICR stride、per-hart mtimecmp 偏移、APIC ID 表），首期实现单核。 |
| 8 | **错误处理** | 解析失败 = **panic 带诊断**，禁止静默回退到 QEMU virt（真实硬件上静默回退 = 不可调试的崩溃）。QEMU 测试构建通过 `QemuVirtDesc` 显式兜底，不经过解析器。 |

**核心一句话**：boot-shim 传一个原始指针，kernel 自己解析成 `PlatformDesc`，硬件 trait 改为"从描述符构造实例 + 实例方法 init"，QEMU 走显式兜底变体，解析失败直接 panic。

---

## 1. 决策 1：归属问题 —— 方案 C（混合），kernel 侧解析

### 1.1 选择

**boot-shim 定位原始指针 → `KernelInfo` 携带 → kernel 解析。**

### 1.2 理由

1. **职责边界清晰**：boot-shim 的本职是"固件交互 + ELF 加载 + ExitBootServices"（见 `os/boot-shim/src/main.rs`、`01-boot-shim-bootstrap.md`）。它已经够重了。把 FDT/ACPI 解析塞进去会让它变成"第二个 BSP"——这正是问题文档 §5.1 列出的代价。

2. **与 Minix3 C 语义对齐**：C 版的 `acpi_init()`（`arch_system.c:246-288`）、`bsp_init()`（`arch_system.c:101-132`）都是 **kernel 内部**调用。虽然 C 版没有 device tree，但"kernel 自己理解硬件"是微内核的职责边界。方案 C 的 kernel 侧解析最接近这个语义。

3. **`KernelInfo` 保持精简**：方案 A（boot-shim 解析成结构化数据）要求 `KernelInfo` 扩展 GICD/GICR/PLIC/CLINT 等十几个字段，变成大而全结构体（见 `os/libs/minix-boot/src/kernel_info.rs` 当前只有 ~12 个字段）。方案 C 只需加 **1 个字段**：`Option<PlatformDescriptorPtr>`。

4. **boot-shim 可替换性**：kernel 从原始 DTB/RSDP 解析，意味着未来换 bootloader（直接从 firmware 启动、或被 GRUB/U-Boot 直接加载）不需要改 kernel。方案 A 把解析绑死在 boot-shim 里，kernel 离不开当前 boot-shim。

5. **可测试性**：kernel 侧解析器可在 `#[cfg(test)]` 中用 mock DTB/ACPI 字节流测试，不需要启动 QEMU。方案 A 的解析逻辑在 boot-shim 里，测试需要 mock 整个 UEFI 环境。

### 1.3 为什么不是方案 A（boot-shim 解析）

- boot-shim 在 UEFI 下确实能用 `std`/alloc 和成熟 crate，但 kernel 迟早要在 `no_std` 下解析 FDT（真实硬件需要）。把解析放 boot-shim 只是推迟问题，且制造两套代码路径（boot-shim 解析 + kernel 仍需理解结构）。
- `no_std` FDT/ACPI crate 已存在（`fdt`、`acpi` crate 均 `no_std` 兼容），kernel 侧解析的工程成本可控。

### 1.4 为什么不是纯方案 B（kernel 解析，boot-shim 完全不参与）

- boot-shim 是固件交互的天然位置：UEFI 的 ACPI RSDP 在 EFI Configuration Table 里（`EFI_ACPI_TABLE_GUID`），OpenSBI/U-Boot 的 DTB 在 `a1` 寄存器。kernel 自己去捞这些指针要么重复固件交互逻辑，要么依赖特定 bootloader 约定。
- 方案 C 让 boot-shim 只做"定位"（一行代码捞指针），不做"理解"——这是最小职责扩展。

### 1.5 边界声明

- **boot-shim 不解析**：不遍历 FDT 节点、不解析 ACPI 表。只把物理地址传过来。
- **kernel 不定位**：kernel 不去扫 UEFI config table、不读 `a1`。只从 `KernelInfo` 拿指针。
- **QEMU 兜底**：当指针为 `None` 且编译期声明 QEMU virt 时，kernel 直接构造 `QemuVirtDesc`，跳过解析器。

---

## 2. 决策 2：命名 —— `PlatformDesc`

### 2.1 选择

根 trait 叫 `PlatformDesc`。子描述符用具体名称（`InterruptControllerDesc` 等）。

### 2.2 理由（对比问题文档 §8.2 的候选）

| 候选 | 否决理由 |
|------|---------|
| `MachineDesc` | 过于"硬件机器"味，暗示特定机型；与 Minix3 BSP（Board Support Package）概念混淆 |
| `BoardDesc` | 直接对应 BSP，而我们正要摆脱 BSP 模式（问题文档 §4.2） |
| `HardwareTopology` | 过宽——CPU cache 拓扑、NUMA 都算 hardware topology，但本抽象只管"启动期需要的硬件参数" |

`PlatformDesc` 中性、与 Linux "platform" 概念对齐、不暗示来源（DTB/ACPI/硬编码都算 platform description）。

---

## 3. 决策 3：trait 形状 —— 根 trait + 值类型子描述符

### 3.1 选择

```rust
/// 平台硬件描述的统一抽象。
///
/// 一个 `PlatformDesc` 实例回答："我跑在什么硬件上？"。
/// 上层（ClockArch / InterruptController / ArchInit）只读这个抽象，
/// 不接触 FDT/ACPI 原始字节。
pub trait PlatformDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc;
    fn timer(&self) -> TimerDesc;
    fn early_console(&self) -> Option<ConsoleDesc>;
    fn cpu_topology(&self) -> CpuTopology;
    fn arch_misc(&self) -> ArchMiscDesc;
}
```

子描述符是**值类型（enum/struct）**，不是 trait。

### 3.2 为什么不是"一个 mega-trait 包所有方法"

- 30+ 方法的单一 trait 违反接口隔离，消费者被迫依赖全部方法。
- 难以测试：mock 一个 30 方法的 trait 比 mock 一个返回 `TimerDesc` 的根 trait 麻烦。

### 3.3 为什么不是"完全独立的子 trait"（`InterruptControllerDesc` trait 等）

- 消费者（`init_clock_and_interrupts`）要接收 N 个独立 trait 对象，参数列表爆炸。
- 子 trait 之间隐含一致性约束（同一个平台的 timer 频率与中断控制器必须来自同一来源），独立 trait 无法在类型层表达这种"同源"关系。

### 3.4 为什么子描述符用值类型而非 trait

- 子描述符的形状是**有限且已知**的（中断控制器就那几种：GICv3/PLIC/APIC；定时器就那几种）。用 `enum` 表达"有限已知集合"是 Rust idiomatic 做法。
- 值类型可以 `Copy`/`Clone`，便于在 init 阶段传给各硬件 trait。
- 值类型让"同源约束"自然成立：它们都从同一个 `PlatformDesc` 实例的不同访问器返回。

---

## 4. 决策 4：传递方式 —— 实例化硬件 trait + `PlatformContext` 全局

### 4.1 选择

1. **硬件 trait 改为实例化**：`ClockArch`、`ArchInit` 从当前的静态 trait（`fn init_timer(hz)`）改为实例 trait（`fn new(desc) -> Self` + `&mut self` 方法）。`InterruptController` 已经是实例 trait，只需把 `new()` + `set_base()` 合并为 `new(desc)`。
2. **解析一次 → 构造实例 → 存入 `&'static PlatformContext` 全局**。
3. **`init()` 接收描述符参数**（通过构造时注入，而非每次调用传参）。
4. **运行时热路径**（`read_ticks`）经全局 `PlatformContext` 访问实例。

### 4.2 为什么实例化（而非保持静态 + 传 desc 参数）

当前 `ClockArch` 是静态 trait：

```rust
// os/arch/src/arch/clock.rs（现状）
pub trait ClockArch {
    fn init_timer(hz: u32);          // 静态
    fn read_ticks() -> u64;          // 静态
}
```

问题：`read_ticks()` 在运行时中断上下文被调用，它需要知道 CLINT mtime 的地址。如果地址来自 `PlatformDesc`，静态方法要么：
- (a) 每次调用都传 `&PlatformDesc` —— 污染所有调用点；
- (b) 读一个全局 static —— 那还不如实例化，让实例自己持有地址。

实例化后：

```rust
pub trait ClockArch: Sized {
    fn new(desc: &TimerDesc) -> Self;
    fn init_timer(&mut self, hz: u32);
    fn read_ticks(&self) -> u64;
}

// riscv64 实现
pub struct Riscv64ClockArch {
    mtime_addr: usize,
    mtimecmp_base: usize,
    freq: u64,
}
```

实例字段持有从 `PlatformDesc` 解析出的地址，`read_ticks` 直接读字段——零额外间接（编译器可把字段 load 提到循环外）。

### 4.3 为什么用 `PlatformContext` 全局而非 `&'static dyn PlatformDesc` 全局

- `&'static dyn PlatformDesc` 要求描述符本身 `'static`。解析产物若放在 bootstrap 内存会被回收（`KernelInfo.bootstrap_start/len` 在 T9 `add_memmap` 回收，见 `kernel-design.md` T9）。强制 `'static` 要么 leak 内存，要么放固定 static buffer——不灵活。
- `PlatformContext` 拥有描述符 **和** 硬件实例。它存放在 kernel BSS 的一个 `static mut Option<PlatformContext>`，在 T2.5（kmain 早期、BKL 持有、IRQ 关闭）写入一次，之后只读。这是 kernel 已有的全局状态模式（BKL、proc 表都是全局的）。
- `PlatformContext` 是具体类型（非 `dyn`），方法调用静态分发，热路径零开销。

### 4.4 Send + Sync 合规性

问题文档 §6.1.3 要求跨 CPU 共享数据 `Send + Sync`。`PlatformContext` 字段全是 `usize`/`u64`/`enum`（无裸指针字段——指针在方法内从 `usize` 现算），因此自动 `Send + Sync`。`static mut Option<PlatformContext>` 的访问受 BKL 保护（init 在 BKL 下，运行时只读），SAFETY 注释说明不变量。

### 4.5 与现有 `set_base()` 的关系

`Riscv64InterruptController::set_base()`（`os/plat/src/riscv64/interrupt.rs:46`）、`AArch64InterruptController::set_base()`（`os/plat/src/arm64/interrupt.rs:53`）是"意识到需要外部注入但缺统一来源"的临时补丁。本设计用 `new(desc)` 取代它们——`set_base()` 删除。

---

## 5. 决策 5：解析时机 —— kernel 解析原始 DTB/RSDP

### 5.1 选择

`KernelInfo` 新增字段：

```rust
/// 平台描述符原始指针（DTB 或 RSDP 的物理地址）。
/// `None` 表示 boot-shim 未提供（QEMU virt 兜底路径）。
pub platform_descriptor: Option<PlatformDescriptorPtr>,
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

kernel 在 T2.5（`prot_init` 之后、`init_clock_and_interrupts` 之前）调用 `platform::init(kinfo)` 解析。

### 5.2 boot-shim 侧的定位逻辑（最小改动）

| 架构 | 固件 | 指针来源 | boot-shim 改动 |
|------|------|---------|---------------|
| x86-64 | UEFI | EFI Configuration Table 的 `EFI_ACPI_TABLE_GUID` 条目 | `uefi_helpers::prepare_boot` 增加一行：从 `boot::config_table` 找 ACPI GUID → 填 `PlatformDescriptorPtr::Rsdp` |
| aarch64 | UEFI | EFI Configuration Table 的 `EFI_ACPI_TABLE_GUID`（服务器）或 `EFI_DEVICE_TREE_GUID`（嵌入式） | 同上，按存在性选 Rsdp 或 Dtb |
| riscv64 | OpenSBI + U-Boot | U-Boot/OpenSBI 在 `a1` 传 DTB 物理地址（RISC-V SBI boot 协议） | `opensbi_helpers` 入口 trampoline（`main.rs`）额外保存 `a1` → 填 `PlatformDescriptorPtr::Dtb` |

> **待确认（to confirm）**：当前 `os/boot-shim/src/opensbi_helpers.rs` 的入口只保存了 `a0`（BootFileTable）。RISC-V SBI boot 协议下 `a1` 是否确实是 DTB 物理地址，需在实施时验证 OpenSBI 版本约定。若 U-Boot 未传 DTB，则 riscv64 QEMU 路径走 `QemuVirtDesc` 兜底（与现状一致）。

### 5.3 kernel 侧解析

```rust
// os/kernel/src/platform/mod.rs（新增）
pub fn init(kinfo: &KernelInfo) {
    let desc = match kinfo.platform_descriptor {
        Some(PlatformDescriptorPtr::Dtb(pa)) => {
            PlatformDesc::DeviceTree(parse_dtb(pa))
        }
        Some(PlatformDescriptorPtr::Rsdp(pa)) => {
            PlatformDesc::Acpi(parse_rsdp(pa))
        }
        None => {
            // 无指针：QEMU virt 兜底（编译期 gate）
            #[cfg(feature = "qemu_virt_fallback")]
            { PlatformDesc::QemuVirt(QemuVirtDesc::default()) }
            #[cfg(not(feature = "qemu_virt_fallback"))]
            { panic!("no platform descriptor provided and qemu_virt_fallback disabled"); }
        }
    };
    // 构造硬件实例并写入全局 PlatformContext（见 §6.4）
    ...
}
```

---

## 6. 具体 API 设计

### 6.1 子描述符值类型

```rust
// os/libs/minix-plat/src/platform_desc.rs（新增）

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
    pub nr_cpus: u32,
    /// 当前 CPU 的硬件 ID（BSP）。
    pub bsp_id: u32,
    /// APIC ID / hart ID / MPIDR 映射（首期仅 [bsp_id]）。
    pub hw_ids: &'static [u32],
}

/// 架构杂项描述符（ACPI 表、PMU、PMP 等 ArchInit 需要的信息）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchMiscDesc {
    /// x86-64: ACPI 表物理地址（若未通过 RSDP 路径提供）。
    pub acpi_tables: Option<usize>,
    /// 是否启用 PMU cycle counter（ARM64）。
    pub pmu_cycle_counter: bool,
}
```

### 6.2 根 trait + 三个具体实现

```rust
pub trait PlatformDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc;
    fn timer(&self) -> TimerDesc;
    fn early_console(&self) -> Option<ConsoleDesc>;
    fn cpu_topology(&self) -> CpuTopology;
    fn arch_misc(&self) -> ArchMiscDesc;
}

/// Device Tree 来源实现。
pub struct DeviceTreeDesc {
    // 持有解析后的字段（从 FDT 节点提取）
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpus: CpuTopology,
    misc: ArchMiscDesc,
}
// impl PlatformDesc for DeviceTreeDesc { ... }

/// ACPI 来源实现（x86-64）。
pub struct AcpiDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpus: CpuTopology,
    misc: ArchMiscDesc,
}
// impl PlatformDesc for AcpiDesc { ... }

/// QEMU virt 硬编码兜底（保持现有测试通过）。
pub struct QemuVirtDesc;
impl PlatformDesc for QemuVirtDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        #[cfg(target_arch = "riscv64")]
        { InterruptControllerDesc::Plic {
            plic_base: 0x0C00_0000,  // 现状常量，见 os/plat/src/riscv64/interrupt.rs:11
            nr_irqs: 64,
            context: 1,
        }}
        #[cfg(target_arch = "aarch64")]
        { InterruptControllerDesc::Gicv3 {
            gicd_base: 0x0800_0000,
            gicr_base: 0x080A_0000,
            gicr_stride: 0x2_0000,
            nr_irqs: 64,
        }}
        #[cfg(target_arch = "x86_64")]
        { InterruptControllerDesc::Apic {
            lapic_base: 0xFEE0_0000,   // 现状 DEFAULT_LAPIC_BASE
            ioapic_base: 0xFEC0_0000,  // 现状 DEFAULT_IOAPIC_BASE
            nr_irqs: 64,
        }}
    }
    fn timer(&self) -> TimerDesc {
        #[cfg(target_arch = "riscv64")]
        { TimerDesc::Clint {
            mtime_addr: 0x200_BFF8,        // 现状 CLINT_MTIME
            mtimecmp_base: 0x200_4000,     // 现状 CLINT_MTIMECMP
            mtimecmp_stride: 8,
            freq: 10_000_000,              // 现状 MTIME_FREQ
        }}
        #[cfg(target_arch = "aarch64")]
        { TimerDesc::ArmGenericTimer }
        #[cfg(target_arch = "x86_64")]
        { TimerDesc::Pit {
            pit_base_freq: 1_193_182,
            lapic_base: 0xFEE0_0000,
        }}
    }
    fn early_console(&self) -> Option<ConsoleDesc> {
        #[cfg(target_arch = "x86_64")]
        { Some(ConsoleDesc::IsaSerial { port_base: 0x3F8 }) }  // COM1 well-known
        #[cfg(target_arch = "aarch64")]
        { Some(ConsoleDesc::MmioSerial { mmio_base: 0x0900_0000 }) }  // PL011 @ QEMU virt
        #[cfg(target_arch = "riscv64")]
        { Some(ConsoleDesc::SbiConsole) }
    }
    fn cpu_topology(&self) -> CpuTopology {
        CpuTopology { nr_cpus: 1, bsp_id: 0, hw_ids: &[0] }
    }
    fn arch_misc(&self) -> ArchMiscDesc { ArchMiscDesc::default() }
}
```

> **注意**：`QemuVirtDesc` 里的 `#[cfg(target_arch)]` 仅用于"选择该架构的 QEMU virt 固定值"，是**数据选择**而非**行为选择**。问题文档 §6.1.1 禁止的是"`#[cfg(target_arch)]` 用于行为选择"（如 `if x86 { foo() } else { bar() }`）。这里每个分支只返回常量数据，且 `QemuVirtDesc` 本身是一个具体实现类型，不污染上层 trait。真实硬件路径走 `DeviceTreeDesc`/`AcpiDesc`，完全无 `#[cfg]`。

### 6.3 硬件 trait 重构

```rust
// ClockArch：静态 → 实例
pub trait ClockArch: Sized {
    fn new(desc: &TimerDesc) -> Self;
    fn init_timer(&mut self, hz: u32);
    fn read_ticks(&self) -> u64;
    fn read_tsc(&self) -> u64 { self.read_ticks() }
}

// InterruptController：new() + set_base() → new(desc)
pub trait InterruptController: Sized {
    fn new(desc: &InterruptControllerDesc) -> Self;
    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}

// ArchInit：静态 → 实例
pub trait ArchInit: Sized {
    fn new(desc: &PlatformDesc) -> Self;
    fn init(&mut self);
}
```

### 6.4 `PlatformContext` 全局与 init 流程

```rust
// os/kernel/src/platform/context.rs（新增）

pub struct PlatformContext {
    desc: PlatformDescEnum,        // 枚举持有具体实现
    clock: CurrentClockArch,
    intr: CurrentInterruptController,
    arch: CurrentArchInit,
}

enum PlatformDescEnum {
    DeviceTree(DeviceTreeDesc),
    Acpi(AcpiDesc),
    QemuVirt(QemuVirtDesc),
}

impl PlatformDesc for PlatformDescEnum { /* delegate */ }

/// 全局平台上下文。T2.5 写入一次（BKL 持有、IRQ 关闭），之后只读。
/// SAFETY: 写入在单核、关中断、BKL 持有下完成；运行期只读。
static mut PLATFORM: Option<PlatformContext> = None;

pub fn init(kinfo: &KernelInfo) {
    let desc_enum = parse_descriptor(kinfo);  // §5.3
    let desc: &dyn PlatformDesc = &desc_enum;
    let clock = CurrentClockArch::new(&desc.timer());
    let intr = CurrentInterruptController::new(&desc.interrupt_controller());
    let arch = CurrentArchInit::new(desc);
    // SAFETY: kmain 早期，BKL 持有，IRQ 关闭，单核。仅此一次写入。
    unsafe { PLATFORM = Some(PlatformContext {
        desc: desc_enum, clock, intr, arch,
    }); }
    let ctx = unsafe { PLATFORM.as_mut().unwrap() };
    ctx.clock.init_timer(DEFAULT_HZ);
    ctx.intr.init();
    ctx.arch.init();
}

/// 运行时访问器（热路径）。
pub fn clock() -> &'static CurrentClockArch {
    // SAFETY: 只读，init 后才调用。
    unsafe { &PLATFORM.as_ref().unwrap().clock }
}
pub fn intr() -> &'static CurrentInterruptController {
    unsafe { &PLATFORM.as_ref().unwrap().intr }
}
```

### 6.5 `kmain` 集成

```rust
// os/kernel/src/lib.rs（修改）
pub fn kmain(kernel_info: &KernelInfo) -> ! {
    init_protection(kernel_info);
    platform::init(kernel_info);           // 新增 T2.5
    init_clock_and_interrupts();           // 改为从 PlatformContext 取实例
    // ...
}

fn init_clock_and_interrupts() {
    // 旧: let mut clock = ClockState::new();
    //     CurrentClockArch::init_timer(clock.hz());
    //     let mut intr = CurrentInterruptController::new(); intr.init();
    //     CurrentArchInit::init();
    // 新: init 已在 platform::init 中完成，此函数可合并或保留为空壳。
    // 实际上 platform::init 已经调用了 clock/intr/arch 的 init()，
    // 这里只需保留 ClockState 软件状态初始化。
    let _clock_state = ClockState::new();  // 软件状态（hz/uptime/loadavg）
}
```

> **注意**：`ClockState`（软件层，`os/arch/src/arch/clock.rs`）与 `ClockArch`（硬件层）的分离保持不变（见 `04-clock-interrupt-init.md` §3.1）。`PlatformDesc` 只影响硬件层。

---

## 7. 决策 6：x86 路径 —— ACPI 为主，QEMU virt 兜底

### 7.1 选择

- **真实 x86 硬件**：ACPI 是唯一来源。`AcpiDesc` 从 RSDP 解析。
- **QEMU virt x86**：当前 `arch_init.rs` 的 ACPI 解析是 stub（`os/arch/src/x86_64/arch_init.rs:32-39`），QEMU virt boot 不依赖 ACPI。走 `QemuVirtDesc` 兜底，LAPIC/IOAPIC 用 `DEFAULT_LAPIC_BASE`/`DEFAULT_IOAPIC_BASE`（现状值）。
- **DTB on x86**：不纳入首期范围。`PlatformDescriptorPtr` 枚举允许未来加 `Dtb` 变体给 x86，但首期 x86 只认 `Rsdp`。

### 7.2 为什么不为 QEMU virt x86 实现 ACPI

- QEMU virt 的 LAPIC/IOAPIC 地址是架构固定的（Intel SDM 默认值），ACPI 表只是重复告知这些固定值。
- 为测试去实现完整 ACPI 解析器是过度工程。
- `QemuVirtDesc` 显式兜底比"解析 ACPI 但其实值是固定的"更诚实。

---

## 8. 决策 7：多核扩展 —— 形状预留，首期单核

### 8.1 选择

描述符**形状**支持多核，**实现**首期单核。

| 字段 | 多核语义 | 首期值 |
|------|---------|--------|
| `InterruptControllerDesc::Gicv3 { gicr_stride }` | 每个 CPU 的 Redistributor 间距 | 单核 = 0（只用一个 GICR） |
| `TimerDesc::Clint { mtimecmp_stride }` | 每个 hart 的 mtimecmp 间距 | 单核 = 8（hart 0） |
| `CpuTopology { nr_cpus, hw_ids }` | CPU 数与硬件 ID 表 | `nr_cpus=1, hw_ids=&[0]` |

### 8.2 为什么不在首期实现多核

- 文档 `15-smp.md`（见 `00-kernel-overview.md` §3.7）标注 SMP 当前是"单核占位"。
- 多核平台发现涉及 per-CPU GICR 唤醒序列、IPI 路由——这些是 SMP 文档的范围，不是平台发现文档的范围。
- 但描述符形状必须现在就预留，否则未来加多核要改 enum 形状 → 破坏性变更。预留字段是零成本的（首期填单核值）。

---

## 9. 决策 8：错误处理 —— panic 带诊断，禁止静默回退

### 9.1 选择

| 情况 | 处理 |
|------|------|
| `platform_descriptor = Some(Dtb(pa))` 但 DTB 解析失败 | `panic!("DTB parse failed at {pa:#x}: {err}")` |
| `platform_descriptor = Some(Rsdp(pa))` 但 ACPI 解析失败 | `panic!("ACPI parse failed at {pa:#x}: {err}")` |
| `platform_descriptor = None` 且 `qemu_virt_fallback` 启用 | 构造 `QemuVirtDesc`（不经过解析器，不会失败） |
| `platform_descriptor = None` 且 `qemu_virt_fallback` 禁用 | `panic!("no platform descriptor and qemu_virt_fallback disabled")` |
| 解析成功但缺少必要节点（如 DTB 无 CLINT 节点） | `panic!("DTB missing required node: clint")` |

### 9.2 为什么不静默回退到 QEMU virt

- 真实硬件上，DTB 解析失败意味着我们不知道硬件地址。静默回退到 QEMU virt 的 `0x200_BFF8` 等地址 → 写到真实硬件的错误地址 → **不可调试的随机崩溃**。
- panic 带诊断至少告诉操作者"解析失败在哪一步"，可定位修复。
- QEMU 测试路径通过 `None` + `qemu_virt_fallback` **显式**走兜底，不经过解析器，不会触发解析失败 panic。

### 9.3 `qemu_virt_fallback` feature 的语义

- 默认在 QEMU 测试构建（`qemu_test` feature）启用。
- 真实硬件构建禁用：`None` 直接 panic，强制要求 boot-shim 提供描述符。
- 这把"QEMU 兜底"从隐式假设变成显式编译期声明。

---

## 10. 迁移计划：四处硬编码如何替换

| # | 现状位置 | 现状常量 | 迁移后来源 |
|---|---------|---------|-----------|
| 1 | `os/arch/src/riscv64/clock.rs:13-23` | `CLINT_MTIME=0x200_BFF8`、`CLINT_MTIMECMP=0x200_4000`、`MTIME_FREQ=10MHz` | `TimerDesc::Clint { mtime_addr, mtimecmp_base, freq }` → `Riscv64ClockArch` 实例字段 |
| 2 | `os/plat/src/riscv64/interrupt.rs:11` | `PLIC_BASE=0x0C00_0000` | `InterruptControllerDesc::Plic { plic_base }` → `Riscv64InterruptController::new(desc)` |
| 3 | `os/plat/src/arm64/interrupt.rs:9-13` | `GICD_OFFSET`、`GICR_OFFSET`（+ `set_base()` 注入） | `InterruptControllerDesc::Gicv3 { gicd_base, gicr_base }` → `AArch64InterruptController::new(desc)`，`set_base()` 删除 |
| 4 | `os/arch/src/x86_64/arch_init.rs:32-39` | ACPI stub（未实现） | `ArchMiscDesc { acpi_tables }` → `X86_64ArchInit::init()` 中按需解析（真实硬件）；QEMU 走兜底不解析 |
| (5) | `os/plat/src/x86_64/interrupt.rs:38-39` | `DEFAULT_LAPIC_BASE=0xFEE0_0000`、`DEFAULT_IOAPIC_BASE=0xFEC0_0000` | `InterruptControllerDesc::Apic { lapic_base, ioapic_base }` |
| (6) | `os/plat/src/x86_64/early_console.rs` | COM1 `0x3F8` | `ConsoleDesc::IsaSerial { port_base: 0x3F8 }`（well-known，可视为架构常量，但纳入描述符统一管理） |

### 10.1 迁移步骤（建议顺序）

1. **新增 `platform_desc` 模块**（`os/libs/minix-plat/src/platform_desc.rs`）：定义 trait + 子描述符 enum + `QemuVirtDesc`。纯类型，无硬件依赖，可单测。
2. **扩展 `KernelInfo`**：加 `platform_descriptor: Option<PlatformDescriptorPtr>` 字段。boot-shim 两个实现各加一行定位逻辑。
3. **重构硬件 trait 为实例化**：`ClockArch`/`ArchInit` 加 `new(desc)`；`InterruptController::new()` 签名改为 `new(desc)`，删 `set_base()`。各架构实现从 desc 取地址存字段。
4. **新增 `PlatformContext` + `platform::init`**：在 `os/kernel/src/platform/` 下。`kmain` 调用 `platform::init(kinfo)`。
5. **`QemuVirtDesc` 兜底**：把现有硬编码常量搬进 `QemuVirtDesc` 的 `impl PlatformDesc`。QEMU 测试先跑通。
6. **DTB 解析器**（riscv64/arm64）：引入 `no_std` FDT crate，实现 `DeviceTreeDesc::parse(pa)`。真实硬件路径打通。
7. **ACPI 解析器**（x86-64）：引入 `acpi` crate，实现 `AcpiDesc::parse(pa)`。真实 x86 硬件路径打通。
8. **删除旧常量**：`CLINT_MTIME`、`PLIC_BASE`、`GICD_OFFSET` 等从源码删除（已被 `QemuVirtDesc` 取代）。

每步可独立提交、独立测试。步骤 1-5 是"搭骨架 + QEMU 不回归"，步骤 6-7 是"真实硬件能力"，步骤 8 是清理。

---

## 11. no_std 与 Send+Sync 合规性

### 11.1 no_std

- `platform_desc` 模块纯类型定义，`#![no_std]` 兼容。
- FDT 解析：用 `fdt` crate（`no_std`，无 alloc 依赖，解析只读切片）。
- ACPI 解析：用 `acpi` crate（`no_std`，但需要 `alloc`——在 kernel 的永久分配器上分配少量结构体，可接受；或限制为栈上解析）。
- `PlatformContext` 全局：`static mut Option<PlatformContext>`，无 alloc。

### 11.2 Send + Sync

- `PlatformContext` 字段：`PlatformDescEnum`（持有 `DeviceTreeDesc`/`AcpiDesc`/`QemuVirtDesc`，全 `usize`/`u64`/`&'static [u32]`）+ 硬件实例（全 `usize` 字段）。自动 `Send + Sync`。
- `static mut PLATFORM` 的 `unsafe` 访问：写入在 T2.5（BKL 持有、IRQ 关闭、单核），读取在运行时（BKL 持有或只读）。SAFETY 注释明确不变量。
- 不使用 `AssumeSyncCell`（问题文档 §1.5.2 禁止跨 CPU 共享）。

---

## 12. 测试策略

### 12.1 L1：纯类型单测（无需 QEMU）

- `QemuVirtDesc` 各访问器返回值正确（断言 `plic_base == 0x0C00_0000` 等）。
- 子描述符 enum 构造与匹配。
- `PlatformContext` 构造流程（mock `PlatformDesc` 实现）。

### 12.2 L2：解析器单测（mock 字节流）

- `DeviceTreeDesc::parse`：喂入预构造的 DTB 字节流（QEMU `virt` 的 DTB 可 dump 出来作为 test fixture），断言解析出的 CLINT/PLIC 地址与 `QemuVirtDesc` 一致。
- `AcpiDesc::parse`：喂入 mock RSDP+XSDT 字节流，断言 LAPIC/IOAPIC 地址。

### 12.3 L3：QEMU 集成测试

- 现有 QEMU 测试（`os/qemu-tests/`）走 `QemuVirtDesc` 兜底，必须继续通过。
- 新增 QEMU 测试：boot-shim 传 `platform_descriptor = Some(Dtb(pa))`，kernel 走 `DeviceTreeDesc` 解析，断言解析结果与兜底值一致（证明解析器正确）。

### 12.4 不需要真实硬件

- `DeviceTreeDesc`/`AcpiDesc` 的正确性由 L2（mock 字节流）保证。
- 真实硬件移植是"换 DTB/ACPI 输入"，不是"改代码"——这正是抽象的目标。

---

## 13. 与方案 A/B 的对比

| 维度 | 方案 A（boot-shim 解析） | 方案 B（kernel 解析，boot-shim 不参与） | **方案 C（GLM 推荐）** |
|------|------------------------|--------------------------------------|----------------------|
| boot-shim 职责 | 重（解析 FDT/ACPI） | 不变 | 极轻（一行捞指针） |
| `KernelInfo` 复杂度 | 高（十几个字段） | 中（1 字段，但 kernel 自己定位指针） | 低（1 字段） |
| kernel 可脱离当前 boot-shim | 否 | 是 | 是 |
| 与 Minix3 C 语义 | 偏离（C 在 kernel 内 `acpi_init`） | 对齐 | 对齐 |
| 解析器可测性 | 难（需 mock UEFI） | 易（`#[cfg(test)]`） | 易 |
| 工程量 | boot-shim 重 + kernel 仍需理解结构 | kernel 重（重复固件交互） | 均衡 |

---

## 14. 开放问题与未来工作

1. **RISC-V DTB 指针来源验证**（§5.2 待确认）：OpenSBI/U-Boot 是否在 `a1` 传 DTB。实施时需验证；若不可用，riscv64 真实硬件路径需另寻指针来源（如 SBI `get_mimpid` 扩展），但 QEMU 路径不受影响。

2. **ACPI 解析器的 alloc 依赖**：`acpi` crate 需要 `alloc`。kernel 永久分配器（非 bootstrap）需在 `platform::init` 前就绪，或改用栈上解析。实施时评估。

3. **`PlatformContext` 与 SMP `IrqManager<IC>` 的对接**：当前 `IrqManager<IC>` 泛型于 `InterruptController` 但未实例化（`os/kernel/src/lib.rs:1160` 注释）。`PlatformContext` 提供全局 `&'static IC`，`IrqManager` 可据此构造。这是 SMP 文档的范围。

4. **用户态服务访问平台描述**：问题文档 §7.2 明确排除。未来若 VM 需要平台信息（如 memmap 已通过 `sys_getkinfo` 暴露），可考虑 `sys_getplatformdesc` IPC。不在本期范围。

5. **热插拔/运行时平台变更**：不支持。`PlatformDesc` 在 boot 期解析一次，运行期不可变。热插拔设备走设备驱动模型，不是平台发现。

---

## 15. 验收标准映射（对照问题文档 §9）

| 验收标准 | GLM 方案如何满足 |
|---------|-----------------|
| 新增平台描述抽象，不暴露 FDT/ACPI 给 ClockArch/InterruptController/ArchInit | `PlatformDesc` trait + 值类型子描述符；硬件 trait 只见 `TimerDesc`/`InterruptControllerDesc`，不见 FDT/ACPI（§6） |
| 4 处硬编码替换为从平台描述读取 | §10 迁移表，6 处常量（含 COM1/LAPIC/IOAPIC）统一迁入描述符 |
| QEMU virt 测试继续通过 | `QemuVirtDesc` 显式兜底 + `qemu_virt_fallback` feature（§7、§9.3） |
| 文档与源码注释同步更新 | 迁移步骤 8 删除旧常量时同步更新 `04-clock-interrupt-init.md` 与源码注释 |
| `KernelInfo` 能传递 DTB/RSDP 原始指针 | `platform_descriptor: Option<PlatformDescriptorPtr>`（§5.1） |
| 新增代码保持 `#![no_std]` | §11.1 论证 |
| 单元测试可验证解析结果，无需启动 QEMU | §12.1 L1 + §12.2 L2 |

---

## 16. 设计自检

- **是否改变外部行为**？否。QEMU virt 行为不变（`QemuVirtDesc` 复用现有常量值）。真实硬件是新增能力。
- **是否违反"硬件操作必须通过 trait"**？否。`PlatformDesc` 本身是 trait；硬件 trait（`ClockArch` 等）仍是唯一硬件操作入口。`QemuVirtDesc` 内的 `#[cfg(target_arch)]` 是数据选择非行为选择（§6.2 注）。
- **是否引入 `std`**？否（§11.1）。
- **跨 CPU 共享是否 `Send+Sync`**？是（§11.2）。
- **是否与 Minix3 C 语义对齐**？是——kernel 内解析对应 C 的 `acpi_init()`/`bsp_init()`（§1.2）。
- **`set_base()` 临时补丁是否清除**？是，由 `new(desc)` 取代（§4.5）。

---

## 17. 参见

- `plat-discovery-problem.md` —— 问题陈述（本文回答其 §8 全部 8 个问题）
- `00-kernel-overview.md` §1.5 —— 内核执行模型约束（本文 §11 遵守）
- `04-clock-interrupt-init.md` §3.1-§3.5 —— 现有 ClockArch/InterruptController/ArchInit trait 设计（本文 §6.3 重构）
- `kernel-design.md` T2-T4 —— 启动时间线（本文 `platform::init` 插入 T2.5）
- `os/libs/minix-boot/src/kernel_info.rs` —— `KernelInfo` 现状（本文 §5.1 扩展）
