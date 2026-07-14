# plat-design-ki: 平台硬件发现抽象设计

> **分类**: Kernel 架构抽象 / 硬件发现
> **状态**: 设计方案（供 bagging 评审）
> **关联问题**: `plat-discovery-problem.md`
> **关联文档**: `00-kernel-overview.md`, `01-boot-shim-bootstrap.md`, `03-kmain-cstart.md`, `04-clock-interrupt-init.md`
> **关联源码**:
> - `os/arch/src/riscv64/clock.rs`
> - `os/plat/src/riscv64/interrupt.rs`
> - `os/plat/src/arm64/interrupt.rs`
> - `os/arch/src/x86_64/arch_init.rs`
> - `os/libs/minix-boot/src/kernel_info.rs`
> - `os/kernel/src/lib.rs:608-645`

---

## 1. 核心决策

### 1.1 方案选择：以 kernel 侧解析为主的混合方案（C → B 过渡）

**平台发现由 kernel 负责解析原始 DTB/RSDP**，boot-shim 只负责**定位并把原始指针传递给 kernel**。为保 QEMU `virt` 测试立刻可用，提供 `QemuVirtMachineDesc` 作为解析失败或未提供指针时的兜底实现。

| 归属 | 职责 |
|------|------|
| **boot-shim** | 从 UEFI/OpenSBI 拿到 DTB/RSDP 物理地址，填入 `KernelInfo`，退出固件服务 |
| **kernel** | 在 `kmain` 阶段解析 DTB/ACPI，构造 `MachineDesc`，供 `ClockArch` / `InterruptController` / `ArchInit` 读取 |
| **QEMU 测试** | 通过 `QemuVirtMachineDesc` 直接返回当前硬编码常量，绕过解析器 |

### 1.2 为什么不是纯 boot-shim 解析（方案 A）

1. **职责边界**：boot-shim 已经有 ELF 加载、内存映射、ExitBootServices 三重职责，再加硬件理解会变成"第二个 BSP"。
2. **可移植性**：kernel 应能从任意 bootloader（OpenSBI/U-Boot/UEFI/直接固件启动）启动，不能假设只有当前 boot-shim。
3. **C 语义对齐**：Minix3 C 的 `acpi_init()` / `bsp_init()` 都在 kernel 内部执行，kernel 侧解析更贴近 Ground Truth。
4. **KernelInfo 膨胀**：boot-shim 解析会把 GICD/GICR/PLIC/CLINT/CPU 拓扑等全部字段塞进 `KernelInfo`，破坏其"启动交接单"的单一职责。

### 1.3 为什么不是纯 kernel 解析（方案 B）

纯 kernel 解析是最终目标，但当前 DTB/ACPI 解析器尚未实现，且必须保证 QEMU 测试不中断。因此用 `QemuVirtMachineDesc` 兜底，形成"解析优先、兜底可用"的混合态。

---

## 2. 命名决策

选用 **`MachineDesc`** 作为顶层抽象 trait。

| 候选 | 评价 | 结论 |
|------|------|------|
| `PlatformDesc` | 最通用，但 "platform" 在代码里已被 `minix-plat` 占用，容易混淆 | 次选 |
| `MachineDesc` | 与 Device Tree 的 "machine" 节点命名一致，明确指代"这块具体的板/SoC"，不会与现有 crate 冲突 | **首选** |
| `BoardDesc` | 过于强调物理板子，QEMU `virt` 这种虚拟机器语义不顺 | 不推荐 |
| `HardwareTopology` | 侧重 CPU/内存拓扑，无法涵盖 timer/interrupt-controller 等资源 | 不推荐 |

子结构命名：
- `TimerDesc` — 定时器描述
- `InterruptControllerDesc` — 中断控制器描述
- `CpuTopology` — CPU 拓扑描述
- `SerialDesc` — 串口描述（可选）

---

## 3. 抽象形状

### 3.1 设计原则

1. **来源无关**：上层代码只读 `&dyn MachineDesc`，不接触 FDT/ACPI 细节。
2. **trait 分层**：顶层 `MachineDesc` 聚合若干子 trait，子系统只依赖自己需要的子 trait。
3. **object-safe**：所有 desc trait 必须 object-safe，支持 `&dyn`。
4. **`Send + Sync`**：解析后不可变，可在 SMP 下共享。
5. **`#![no_std]`**：描述 trait 与解析器都不依赖 `std`。
6. **不删除已有注入点**：`Riscv64InterruptController::set_base()`、`AArch64InterruptController::set_base()` 保留，但改为由 `init(desc)` 自动调用。

### 3.2 模块与 crate 归属

新建 crate：`os/libs/minix-machine`

```
minix-machine
├── src/lib.rs            # 重新导出
├── src/desc.rs           # MachineDesc / TimerDesc / InterruptControllerDesc / CpuTopology / SerialDesc
├── src/kind.rs           # TimerKind / InterruptControllerKind
├── src/dtb.rs            # DeviceTreeMachineDesc（kernel 侧解析 DTB）
├── src/acpi.rs           # AcpiMachineDesc（kernel 侧解析 ACPI，当前 stub）
└── src/qemu_virt.rs      # QemuVirtMachineDesc（硬编码兜底）
```

- `minix-boot` 保持职责不变：只负责把 `dtb_phys` / `rsdp_phys` 指针填入 `KernelInfo`。
- `minix-arch` 与 `minix-kernel` 依赖 `minix-machine`。
- `minix-machine` 依赖 `minix-types`（`PhysBytes` 等）和外部 no_std FDT crate（如 `fdt-rs`）。

### 3.3 trait 定义草案

```rust
// minix-machine/src/desc.rs
use minix_types::PhysBytes;

/// 顶层机器描述。
pub trait MachineDesc: Send + Sync {
    fn timer(&self) -> &dyn TimerDesc;
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;
    fn cpu_topology(&self) -> &dyn CpuTopology;
    fn serial(&self) -> Option<&dyn SerialDesc>;
}

/// 定时器描述。
pub trait TimerDesc: Send + Sync {
    fn kind(&self) -> TimerKind;
    /// 全局寄存器 MMIO 基址与长度（如 CLINT mtime、ARM Generic Timer 控制寄存器）。
    fn global_mmio(&self) -> Option<(PhysBytes, usize)>;
    /// 每 CPU 寄存器 MMIO 基址与长度（如 CLINT mtimecmp、LAPIC）。
    fn per_cpu_mmio(&self, cpu_id: CpuId) -> Option<(PhysBytes, usize)>;
    /// 输入时钟频率（Hz），用于计算分频/比较值。
    fn input_frequency_hz(&self) -> Option<u64>;
}

/// 中断控制器描述。
pub trait InterruptControllerDesc: Send + Sync {
    fn kind(&self) -> InterruptControllerKind;
    /// 主控制器 MMIO（GICD / PLIC / IOAPIC）。
    fn primary_mmio(&self) -> (PhysBytes, usize);
    /// 次控制器 MMIO（GICR / LAPIC）。可为 None（如 PLIC 无 per-cpu base，x86 LAPIC 由 MSR 读取）。
    fn secondary_mmio(&self) -> Option<(PhysBytes, usize)>;
}

/// CPU 拓扑描述。
pub trait CpuTopology: Send + Sync {
    fn boot_cpu_id(&self) -> CpuId;
    fn cpu_count(&self) -> usize;
    fn cpu(&self, index: usize) -> Option<CpuDesc>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuId(pub usize);

pub struct CpuDesc {
    pub id: CpuId,
    pub arch_id: u64,        // x86 APIC ID / ARM MPIDR / RISC-V hartid
    pub enabled: bool,
}

/// 串口描述（早期控制台）。
pub trait SerialDesc: Send + Sync {
    fn kind(&self) -> SerialKind;
    fn mmio_base(&self) -> PhysBytes;
    fn mmio_size(&self) -> usize;
}
```

```rust
// minix-machine/src/kind.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerKind {
    Pit,           // x86 8254 PIT（I/O 端口，非 MMIO）
    LapicTimer,    // x86 LAPIC Timer
    ArmGenericTimer,
    Clint,         // RISC-V CLINT
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptControllerKind {
    IoApic,        // x86 IOAPIC
    Lapic,         // x86 LAPIC
    GicV2,
    GicV3,
    GicV4,
    Plic,          // RISC-V PLIC
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialKind {
    Uart16550,     // x86 COM1
    Pl011,         // ARM QEMU virt
    SbiConsole,    // RISC-V SBI
    Unknown,
}
```

### 3.4 为什么用 trait 而非 struct

- **来源抽象**：DTB、ACPI、QEMU 硬编码、单元测试 mock 都可实现同一 trait。
- **object-safe**：`&'static dyn MachineDesc` 可在运行时选择后端。
- **不可变性**：解析一次后只读，天然 `Send + Sync`，适合 BKL + SMP 场景。

---

## 4. KernelInfo 扩展

在 `KernelInfo` 中新增两个可选的物理地址字段：

```rust
// os/libs/minix-boot/src/kernel_info.rs
pub struct KernelInfo {
    // ... 现有字段 ...

    /// 设备树 blob（DTB）物理地址。ARM64 / RISC-V 使用。
    /// boot-shim 从 UEFI 配置表或 OpenSBI 的 a1 寄存器获得。
    pub dtb_phys: Option<PhysBytes>,

    /// ACPI RSDP 物理地址。x86-64 使用。
    /// boot-shim 从 UEFI 系统表获得。
    pub rsdp_phys: Option<PhysBytes>,
}
```

设计要点：
- **只传指针，不传结构化数据**：保持 `KernelInfo` 简洁，避免 boot-shim 变成 BSP。
- **Option 包装**：模拟器或不支持 DTB/ACPI 的固件可填 `None`，kernel 回退到 `QemuVirtMachineDesc`。
- **与架构无关的字段**：虽然 DTB/RSDP 是架构特定来源，但用统一字段比按架构 cfg 更简洁，也允许未来 x86 支持 DTB。

---

## 5. 内核初始化流程集成

### 5.1 新增 `machine_discover()`

在 `os/kernel/src/lib.rs` 的 `kmain` 早期调用：

```rust
use minix_machine::{MachineDesc, DeviceTreeMachineDesc, AcpiMachineDesc, QemuVirtMachineDesc};

/// 根据 KernelInfo 构造 MachineDesc。
/// 解析失败或未提供指针时回退到 QemuVirtMachineDesc。
fn discover_machine(info: &KernelInfo) -> &'static dyn MachineDesc {
    // 优先使用架构指定的源。
    #[cfg(target_arch = "riscv64")]
    if let Some(dtb) = info.dtb_phys {
        // SAFETY: boot-shim 保证 dtb 是有效的、在内核可映射范围内的物理地址。
        if let Some(desc) = unsafe { DeviceTreeMachineDesc::parse(dtb) } {
            return leak(desc);
        }
    }

    #[cfg(target_arch = "aarch64")]
    if let Some(dtb) = info.dtb_phys {
        if let Some(desc) = unsafe { DeviceTreeMachineDesc::parse(dtb) } {
            return leak(desc);
        }
    }

    #[cfg(target_arch = "x86_64")]
    if let Some(rsdp) = info.rsdp_phys {
        if let Some(desc) = unsafe { AcpiMachineDesc::parse(rsdp) } {
            return leak(desc);
        }
    }

    // 兜底：QEMU virt 硬编码描述。
    leak(QemuVirtMachineDesc::new())
}

fn leak<T: MachineDesc + 'static>(desc: T) -> &'static dyn MachineDesc {
    Box::leak(Box::new(desc))
}
```

> **注意**：`Box::leak` 在内核 `no_std` 环境下需要全局分配器。若当前内核未启用全局分配器，可改用 `static mut` + `Once` 模式或把 `MachineDesc` 实现设计为 zero-sized / 可放在 `static` 中。具体由实现者根据内核当前内存初始化状态选择。

### 5.2 修改 `init_clock_and_interrupts()`

当前签名：

```rust
fn init_clock_and_interrupts() { ... }
```

改为：

```rust
fn init_clock_and_interrupts(machine: &'static dyn MachineDesc) { ... }
```

内部调用：

```rust
fn init_clock_and_interrupts(machine: &'static dyn MachineDesc) {
    use minix_arch::{ClockState, ClockArch, ArchInit, CurrentClockArch, CurrentArchInit};
    use minix_plat::{InterruptController, CurrentInterruptController};

    let mut clock = ClockState::new();

    let timer_desc = machine.timer();
    CurrentClockArch::init_timer(clock.hz(), timer_desc);

    let ic_desc = machine.interrupt_controller();
    let mut intr = CurrentInterruptController::new();
    intr.init(ic_desc);

    CurrentArchInit::init(machine);
}
```

### 5.3 trait 签名变更

```rust
// os/arch/src/arch/clock.rs
pub trait ClockArch {
    fn init_timer(hz: u32, desc: &dyn TimerDesc);
    fn read_ticks() -> u64;
    fn read_tsc() -> u64 { Self::read_ticks() }
}

// os/plat/src/interrupt.rs
pub trait InterruptController: Sized {
    fn init(&mut self, desc: &dyn InterruptControllerDesc);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}

// os/arch/src/arch/arch_init.rs
pub trait ArchInit {
    fn init(machine: &'static dyn MachineDesc);
}
```

---

## 6. 各架构迁移路径

### 6.1 RISC-V 64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| CLINT_MTIME `0x200_BFF8` | `os/arch/src/riscv64/clock.rs:13` | `TimerDesc::global_mmio()` |
| CLINT_MTIMECMP `0x200_4000` | `os/arch/src/riscv64/clock.rs:17` | `TimerDesc::per_cpu_mmio(CpuId(0))` |
| MTIME_FREQ `10_000_000` | `os/arch/src/riscv64/clock.rs:21` | `TimerDesc::input_frequency_hz()` |
| PLIC_BASE `0x0C00_0000` | `os/plat/src/riscv64/interrupt.rs:9` | `InterruptControllerDesc::primary_mmio()` |

`Riscv64InterruptController::init(desc)` 内部调用 `self.set_base(desc.primary_mmio().0)`。

### 6.2 ARM64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| GICD base `0`（需外部注入） | `os/plat/src/arm64/interrupt.rs:43` | `InterruptControllerDesc::primary_mmio()` |
| GICR base `0`（需外部注入） | `os/plat/src/arm64/interrupt.rs:44` | `InterruptControllerDesc::secondary_mmio()` |
| GICR offset `0x000A_0000` | `os/plat/src/arm64/interrupt.rs:13` | 保留在 `AArch64InterruptController` 内部作为 GICv3 寄存器布局常量；base 来自 desc |

`AArch64InterruptController::init(desc)` 内部调用 `self.set_base(primary, secondary)`。

### 6.3 x86-64

| 当前硬编码 | 当前位置 | 迁移方式 |
|-----------|---------|---------|
| DEFAULT_LAPIC_BASE `0xFEE0_0000` | `os/plat/src/x86_64/interrupt.rs:64` | 保留为 fallback；`init_lapic()` 优先读取 MSR `IA32_APIC_BASE`，其次使用 desc 提供的值 |
| DEFAULT_IOAPIC_BASE `0xFEC0_0000` | `os/plat/src/x86_64/interrupt.rs:66` | `InterruptControllerDesc::primary_mmio()` |
| PIT ports `0x40/0x43` | `os/arch/src/x86_64/clock.rs:17-20` | **保持常量**：PC/UEFI 兼容机上 8254 PIT I/O 端口是架构常量 |
| ACPI 未实现 | `os/arch/src/x86_64/arch_init.rs:32-39` | 通过 `AcpiMachineDesc` 提供 RSDP → MADT → IOAPIC base；当前先 stub |

x86-64 的 `X86_64InterruptController::init(desc)` 优先从 desc 拿 IOAPIC base；LAPIC base 从 MSR 读取（与当前行为一致），desc 可提供辅助校验。

### 6.4 串口

- x86 COM1 `0x3F8` 是 PC 架构常量，可继续留在 `X86_64EarlyConsole` 中，不纳入 `MachineDesc`。
- ARM PL011 / RISC-V SBI 的基址若未来需要真实板子支持，再纳入 `SerialDesc`。

---

## 7. `QemuVirtMachineDesc` 兜底实现

```rust
// minix-machine/src/qemu_virt.rs
pub struct QemuVirtMachineDesc;

impl QemuVirtMachineDesc {
    pub const fn new() -> Self { Self }
}

impl MachineDesc for QemuVirtMachineDesc {
    fn timer(&self) -> &dyn TimerDesc { self }
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc { self }
    fn cpu_topology(&self) -> &dyn CpuTopology { self }
    fn serial(&self) -> Option<&dyn SerialDesc> { None }
}

#[cfg(target_arch = "riscv64")]
impl TimerDesc for QemuVirtMachineDesc {
    fn kind(&self) -> TimerKind { TimerKind::Clint }
    fn global_mmio(&self) -> Option<(PhysBytes, usize)> { Some((0x200_BFF8, 8)) }
    fn per_cpu_mmio(&self, cpu_id: CpuId) -> Option<(PhysBytes, usize)> {
        if cpu_id.0 == 0 { Some((0x200_4000, 8)) } else { None }
    }
    fn input_frequency_hz(&self) -> Option<u64> { Some(10_000_000) }
}

#[cfg(target_arch = "riscv64")]
impl InterruptControllerDesc for QemuVirtMachineDesc {
    fn kind(&self) -> InterruptControllerKind { InterruptControllerKind::Plic }
    fn primary_mmio(&self) -> (PhysBytes, usize) { (0x0C00_0000, 0x40_0000) }
    fn secondary_mmio(&self) -> Option<(PhysBytes, usize)> { None }
}

#[cfg(target_arch = "riscv64")]
impl CpuTopology for QemuVirtMachineDesc {
    fn boot_cpu_id(&self) -> CpuId { CpuId(0) }
    fn cpu_count(&self) -> usize { 1 }
    fn cpu(&self, index: usize) -> Option<CpuDesc> {
        if index == 0 { Some(CpuDesc { id: CpuId(0), arch_id: 0, enabled: true }) } else { None }
    }
}
```

> ARM64 / x86-64 的 `QemuVirtMachineDesc` 实现同理，返回各自当前硬编码值。

---

## 8. SMP 与 no_std 考虑

### 8.1 SMP

- `MachineDesc` 及其子 trait 都要求 `Send + Sync`。
- 解析完成后置为 `&'static dyn MachineDesc`，不可变，所有 CPU 共享只读引用。
- 每 CPU 数据（GICR、mtimecmp、LAPIC）通过 `CpuTopology::cpu()` 查询，不在全局可变状态中维护。

### 8.2 no_std

- `minix-machine` crate 标注 `#![no_std]`。
- FDT 解析使用 no_std crate（如 `fdt-rs`），避免自行实现完整解析器。
- ACPI 解析当前 stub，未来逐步添加；MADT 解析不需要复杂分配器，可用固定大小数组或扫描式解析。

### 8.3 内存分配时机

`discover_machine()` 需要在全局分配器可用后执行。当前 `kmain` 在 `init_protection()` 之后、进程表之前已有 bump/page allocator 可用，满足条件。若实现时内核尚未初始化分配器，可将 `MachineDesc` 实现体放在 `static` 中（`QemuVirtMachineDesc` 天然零大小；DTB 解析结果若需堆内存，可改用固定大小缓冲）。

---

## 9. 测试策略

| 测试层级 | 方法 |
|---------|------|
| **单元测试** | 构造 `MockMachineDesc`（返回已知地址/频率），验证 `Riscv64ClockArch::init_timer` 写入的 mtimecmp 值正确 |
| **DTB 解析测试** | 在 `#[cfg(test)]` 中嵌入 QEMU `virt` DTB 字节数组，验证 `DeviceTreeMachineDesc::parse` 提取的 CLINT/PLIC 地址与频率 |
| **QEMU 集成测试** | 保持现有 `qemu_test_*.sh` 通过；逐步把 `QemuVirtMachineDesc` 接入 `init_clock_and_interrupts` |
| **架构 mock 测试** | 利用 `minix-plat` 的 `MockInterruptController`，传入 `MockInterruptControllerDesc` 验证 `init` 调用链路 |

---

## 10. 实施顺序（四阶段）

### Phase 1：基础设施（不破坏 QEMU）

1. 创建 `minix-machine` crate，定义 `desc.rs` / `kind.rs` trait。
2. 扩展 `KernelInfo` 加 `dtb_phys` / `rsdp_phys`（boot-shim 先填 `None`）。
3. 实现 `QemuVirtMachineDesc`（各架构），返回当前硬编码值。
4. 修改 `ClockArch` / `InterruptController` / `ArchInit` trait 签名，各架构实现接收 desc 但先走 `QemuVirtMachineDesc`。

### Phase 2：boot-shim 传递指针

1. UEFI boot-shim 从系统表读取 ACPI RSDP / DTB 地址，填入 `KernelInfo`。
2. OpenSBI boot-shim 把 a1 寄存器的 DTB 地址填入 `KernelInfo`。

### Phase 3：DTB 解析器

1. 引入 no_std FDT crate，实现 `DeviceTreeMachineDesc::parse()`。
2. RISC-V / ARM64 `discover_machine()` 优先解析 DTB，失败回退 `QemuVirtMachineDesc`。

### Phase 4：ACPI 解析器

1. 实现最小 ACPI RSDP → RSDT/XSDT → MADT 解析，提取 IOAPIC base。
2. x86-64 `X86_64InterruptController::init(desc)` 使用 desc 提供的 IOAPIC base。

---

## 11. 风险与未决问题

| 风险 | 缓解 |
|------|------|
| 内核解析 DTB/ACPI 占用代码空间 | 仅 boot 阶段执行一次；解析器本身可裁剪；`QemuVirtMachineDesc` 路径不链接解析器 |
| 解析失败导致 boot panic | 生产环境应 panic（无 timer/interrupt controller 无法运行）；测试环境回退 QEMU virt |
| `Box::leak` 需要分配器 | 提供 `static` 兜底模式；`QemuVirtMachineDesc` 可零大小静态存储 |
| 多架构 `QemuVirtMachineDesc` 用 `#[cfg(target_arch)]` | 这是 crate 内部类型别名/实现选择，不是上层行为选择，符合现有 `CurrentClockArch` 模式 |
| ACPI 复杂度 | 先实现 MADT 提取 IOAPIC/LAPIC；其他表（SRAT/SLIT）后续按需添加 |

---

## 12. 验收标准

- [ ] 新增 `minix-machine` crate，`MachineDesc` / `TimerDesc` / `InterruptControllerDesc` / `CpuTopology` trait 定义完成且 object-safe。
- [ ] `KernelInfo` 新增 `dtb_phys` / `rsdp_phys` 字段，boot-shim 能传递原始指针。
- [ ] `ClockArch::init_timer`、`InterruptController::init`、`ArchInit::init` 签名更新为接收 desc。
- [ ] RISC-V / ARM64 / x86-64 当前硬编码常量迁移到 `QemuVirtMachineDesc`（Phase 1）或 DTB/ACPI 解析器（后续 phase）。
- [ ] QEMU `virt` 测试继续通过。
- [ ] 所有新增代码保持 `#![no_std]`。
- [ ] `MachineDesc` 及子 trait 实现 `Send + Sync`。
- [ ] 单元测试可在不启动 QEMU 的情况下验证 `MachineDesc` 解析与注入行为。

---

## 13. 参见

- `plat-discovery-problem.md` — 问题完整描述与备选方案对比
- `00-kernel-overview.md` §1.5 — 内核执行模型约束
- `01-boot-shim-bootstrap.md` — boot-shim 职责边界
- `04-clock-interrupt-init.md` — `init_clock_and_interrupts()` 当前实现
- `os/kernel/src/lib.rs:608-645` — 当前 init 流程
