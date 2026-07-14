# 04-platform-discovery: 平台硬件发现抽象

> **分类**: 平台抽象 / 硬件发现
> **源码**: `os/libs/minix-platform/src/desc.rs`、`os/libs/minix-platform/src/global.rs`、`os/libs/minix-platform/src/qemu_virt.rs`、`os/libs/minix-platform/src/device_tree.rs`、`os/libs/minix-platform/src/acpi.rs`、`os/libs/minix-boot/src/kernel_info.rs:78-118`、`os/boot-shim/src/uefi_helpers.rs:33-78`、`os/boot-shim/src/opensbi_helpers.rs:215-220`、`os/kernel/src/lib.rs:283-293`
> **C 参考源码**: `minix3/minix/kernel/arch/i386/arch_system.c:246-287`（`arch_init()` 调用 `acpi_init()`）、`minix3/minix/kernel/arch/i386/acpi.c:310-342`（`acpi_init()`）、`minix3/minix/kernel/arch/earm/arch_system.c:101-132`（`arch_init()` 调用 `bsp_init()`）
> **说明**: 内核如何在不硬编码地址的前提下，知道自己在什么硬件上运行——`PlatformDesc` trait 的设计与三架构统一抽象
> **前置**: [03-kmain-cstart.md](03-kmain-cstart.md) — cstart 初始化序列已建立保护结构
> **后续**: [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断控制器从 `PlatformDesc` 获取地址

---

## 1. 概念：硬件发现——内核如何"认识"自己运行的硬件

### 1.0 本章引言

本章建立"平台硬件发现"的概念模型：内核启动时在不硬编码 MMIO（Memory-Mapped I/O，**内存映射 I/O**——设备寄存器被映射到物理地址空间，CPU 用普通 load/store 指令访问，与 x86 port-mapped I/O 相对）地址的前提下，如何获取中断控制器基址、定时器频率、CPU 拓扑等硬件参数。

> **本章不讲什么**:
> - GDT/IDT/TSS 初始化细节（[03-kmain-cstart.md](03-kmain-cstart.md)）
> - 时钟/中断控制器的寄存器级编程（[05-clock-interrupt-init.md](05-clock-interrupt-init.md)）
> - Device Tree Blob（**DTB**，ARM/RISC-V 嵌入式生态的硬件描述二进制格式；规范名 Flattened Device Tree / **FDT**）的二进制格式（本章只讲"为什么需要解析 DTB"，不讲 FDT 规范）
> - ACPI（**Advanced Configuration and Power Interface**，x86 固件用的硬件配置与电源管理标准）表的完整规范（本章只讲"ACPI 作为 x86 硬件描述来源"的定位）
>
> 这些在后续章节展开。本章只建概念地基：**硬件参数从哪里来**。

### 1.1 问题：硬编码地址的不可持续性

内核启动时需要知道一批硬件参数。这些参数如果写死在代码里，会带来三类问题：

1. **只支持 QEMU `virt` 机器**（QEMU 提供的通用虚拟平台，无特定真实硬件对应，是 minix-rs 三架构共用的测试目标机器）：例如 RISC-V QEMU `virt` 的 CLINT（**Core Local Interruptor**，RISC-V 平台的本地中断控制器 + 时钟设备）在 `0x200_BFF8`、PLIC（**Platform-Level Interrupt Controller**，RISC-V 平台的外部中断控制器）在 `0x0C00_0000`——这些是 QEMU `virt` 的固定布局。真实板子（树莓派、SiFive Unleashed）的地址完全不同。
2. **三架构各自维护一套常量**：没有统一接口，添加新板子要改多个文件。
3. **与 C 原版语义不一致**：Minix3 C 版在 `acpi_init()`（`arch_system.c:246-287`）中解析 ACPI 表获取 IOAPIC（**I/O APIC**，x86 多中断控制器架构中的外部 I/O 中断控制器）基址。若 Rust 实现直接写死地址，既偏离 C 的语义，也无法移植到不同板子。

下表把"需要什么参数、它们回答 CPU 的哪个问题、典型来源"统一列出：

| 硬件参数 | 回答 CPU 的什么问题 | 为什么需要 | 典型取值来源 |
|---------|-------------------|-----------|-------------|
| 中断控制器基址 | 中断/异常从哪个 MMIO 地址来 | 初始化中断控制器、mask/unmask IRQ | ACPI **MADT**（Multiple APIC Description Table，描述 x86 LAPIC 与 IOAPIC）/ DTB `interrupt-controller` 节点 |
| 定时器基址/频率 | 时间怎么走 | 配置时钟中断频率、读取当前 tick | ACPI / DTB `timer` 节点；ARM Generic Timer 频率来自 `CNTFRQ_EL0`（ARM64 系统寄存器） |
| CPU 拓扑 | 有几颗 CPU、每颗的私有地址在哪 | SMP 启动、per-CPU 地址计算 | ACPI MADT / DTB `cpu` 节点 |
| 早期控制台基址 | boot 阶段日志输出到哪 | boot 阶段日志输出 | ACPI **SPCR**（Serial Port Console Redirection Table）/ DTB `chosen/stdout` 节点 |

> **视角提示**：上表只建立"内核需要哪些硬件参数、它们从哪里发现"的概念地基。具体寄存器级编程（如何初始化 APIC/PLIC/GIC）属于 [05-clock-interrupt-init.md](05-clock-interrupt-init.md)。

### 1.2 CPU 三问视角下的硬件发现

用 [03-kmain-cstart.md](03-kmain-cstart.md) §1.4 的"CPU 三问"框架看硬件发现：

| CPU 三问 | 硬件发现回答什么 |
|---------|----------------|
| 第一问：我在哪个特权级？ | （不属于本章——保护结构负责） |
| 第二问：异常/中断去哪里？ | 中断控制器基址（IRQ 从哪个 MMIO 地址来） |
| 第三问：内核栈在哪里？ | （不属于本章——保护结构负责） |
| 隐含第四问：时间怎么走？ | 定时器基址与频率（时钟中断从哪来） |

硬件发现回答的是"第二问和第四问的**硬件参数**从哪里获取"——不是"如何编程硬件"（那是 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) 的职责），而是"**地址和频率值**从哪里来"。

### 1.3 三架构的硬件描述来源

不同架构的固件以不同格式向 OS 描述硬件：

| 架构 | 固件 | 硬件描述格式 | 典型来源 |
|------|------|------------|---------|
| x86-64 | UEFI（实现）/ BIOS（概念来源） | ACPI（RSDP → XSDT/RSDT → MADT）。**RSDP** = Root System Description Pointer（根入口指针）；**XSDT** = Extended System Description Table（64 位多表索引）；**RSDT** = Root System Description Table（32 位多表索引） | EFI Configuration Table 的 `EFI_ACPI_TABLE_GUID` |
| aarch64 | UEFI（实现）/ U-Boot（概念来源） | Device Tree Blob（DTB）或 ACPI | EFI Configuration Table 或 U-Boot 传递 |
| riscv64 | **OpenSBI**（开源 SBI 实现，运行在 M-mode 的固件）+ U-Boot。**SBI** = Supervisor Binary Interface（RISC-V 的 M-mode 固件提供给 S-mode kernel 的系统调用接口） | Device Tree Blob（DTB） | OpenSBI 在 `a1` 寄存器传递 DTB 物理地址 |

> **实现范围**：当前 Rust 代码的 boot-shim 仅支持 UEFI（x86-64 / aarch64）和 OpenSBI+U-Boot（riscv64）。BIOS 启动、实模式、Multiboot/GRUB legacy 路径**未覆盖**，只在概念表中作为"x86 世界还可能存在的来源"列出。详见 [01-boot-shim-bootstrap.md §3.1](01-boot-shim-bootstrap.md#31-uefi-替代-multiboot架构演进)。

**关键观察**：虽然格式不同（ACPI vs DTB），但它们回答的是**同一组问题**——中断控制器在哪、定时器在哪、有几个 CPU。这正是统一抽象的基础。

> **为什么 x86 用 ACPI 而非 DTB**：历史原因。x86 世界从 PC/AT 演化而来，ACPI 是 Intel/Microsoft 主导的标准，覆盖电源管理 + 硬件拓扑。DTB 是 ARM/RISC-V 嵌入式世界的标准，更轻量。两者在"描述硬件"这个语义层面是等价的。

### 1.4 硬件发现 vs 硬件编程的职责边界

| 阶段 | 做什么 | 不做什么 |
|------|-------|---------|
| 硬件发现 | 拿到 MMIO 基址、频率、CPU 数量 | 不初始化中断控制器、不配置定时器 |
| 硬件编程 | 用发现的地址去 mask/unmask IRQ、设置比较器 | 不硬编码地址 |

这条边界是后续章节划分的依据：[05-clock-interrupt-init.md](05-clock-interrupt-init.md) 负责"拿到地址后怎么编程"；本章只负责"地址怎么来"。

---

## 2. Minix3 的实现

### 2.0 本章定位

本章只分析 Minix3 C 源代码**如何获取硬件参数**，为第 3 章 Rust 设计决策提供事实基线。C 版的行为是参考基准：Rust 重写必须解释"哪些行为保留、哪些行为演进、为什么演进"。

### 2.1 x86-64: `acpi_init()` 与 ACPI 表解析

x86-64 Minix3 的硬件参数主要走 ACPI 路径。入口在 `arch_system.c:246-287`：

```c
void arch_init(void)
{
    ...
#ifdef USE_ACPI
    acpi_init();
#endif
    ...
}
```

> **C 参考源码**: `minix3/minix/kernel/arch/i386/arch_system.c:246-287`

`acpi_init()` 的实现见 `minix3/minix/kernel/arch/i386/acpi.c:310-342`：

```c
void acpi_init(void)
{
    int s, i;
    read_func = acpi_phys_copy;

    if (!get_acpi_rsdp()) {
        printf("WARNING : Cannot configure ACPI\n");
        return;
    }
    
    s = acpi_read_sdt_at(acpi_rsdp.rsdt_addr, ...);
    sdt_count = (s - sizeof(struct acpi_sdt_header)) / sizeof(u32_t);

    for (i = 0; i < sdt_count; i++) {
        ... /* 遍历 RSDT/XSDT 中的表，记录 signature/length */
    }

    acpi_init_poweroff();
}
```

> **C 参考源码**: `minix3/minix/kernel/arch/i386/acpi.c:310-342`

关键行为：

1. **定位 RSDP**：`get_acpi_rsdp()`（`acpi.c:204-228`）先在 EBDA（`0x40E`）附近搜索，再在 BIOS 只读内存空间 `0xE0000-0x100000` 搜索签名 `"RSD PTR "`。
2. **解析 RSDT/XSDT**：从 RSDP 拿到 RSDT（32 位）或 XSDT（64 位）物理地址，遍历其中的表指针。
3. **MADT 表消费**：后续 `acpi_get_ioapic_next()`（`acpi.c:343-363`）和 `acpi_get_lapic_next()`（`acpi.c:365-387`）从 `"APIC"` 表提取 IOAPIC/LAPIC 信息。

> **C 版的限制**：`acpi_init()` 在 `arch_init()` 中调用，而 `arch_init()` 在 Minix3 C 的启动序列中**晚于**时钟和中断初始化。原因是 C 版时钟/中断控制器依赖硬编码常量（如 `DEFAULT_LAPIC_BASE`），不需要 ACPI 解析结果。Rust 重写要"从解析中获取时钟/中断基址"，因此顺序必须前移——这是与 C 的**有意偏离**。

### 2.2 ARM: `bsp_init()` 与板级常量

ARM Minix3（`earm`）的硬件参数走板级支持包（BSP）常量。入口在 `earm/arch_system.c:101-132`：

```c
void arch_init(void)
{
    ...
    bsp_init();
}
```

> **C 参考源码**: `minix3/minix/kernel/arch/earm/arch_system.c:101-132`

`bsp_init()` 的实现因板子而异，典型行为包括：

- 设置 GICD/GICR 基址（如 ARM RealView 板子的固定值）。
- 启用 PMU cycle counter（`arch_init()` 自身代码，`earm/arch_system.c:113-129`）。
- 硬编码 CPU 频率（例如 `cpu_info[cpu].freq = 660; /* 660 Mhz hardcoded */`，`earm/arch_system.c:98`）。

> **C 参考源码**: `minix3/minix/kernel/arch/earm/arch_system.c:98`

C 版 ARM 没有 Device Tree 解析器，因此 GIC 基址、串口基址等都来自 BSP 头文件中的宏。Rust 重写引入 DTB 解析，把"板子特定常量"变成"固件描述的数据"——这是对 C 版覆盖空白的填补，不是语义偏离。

### 2.3 C 版硬件参数的来源与局限

| 参数 | C x86-64 来源 | C ARM 来源 | C 版局限 |
|------|--------------|-----------|---------|
| 中断控制器基址 | ACPI MADT | BSP 头文件宏 | ARM 无统一发现机制，新板子要改 BSP |
| 定时器频率 | 硬编码（PIT 1193182 Hz） | 硬编码（如 660 MHz） | 真实硬件频率变化需重新编译 |
| CPU 拓扑 | ACPI MADT LAPIC 记录 | 硬编码 `ncpus` | SMP 扩展依赖手工配置 |
| Early console | COM1 `0x3F8` 硬编码 | PL011 地址硬编码 | 真实板子接线不同需改代码 |

这些局限正是 Rust 重写引入 `PlatformDesc` 的动机：把"编译期常量"变成"启动期发现"。

### 2.4 因果链抽样：C 版初始化顺序

C 版启动链（x86-64）的实际顺序（来源：`minix3/minix/kernel/main.c:399-475` 的 `cstart()` 函数）：

```
main()
  → cstart()                                   (main.c:399)
    → prot_init()                              (main.c:411)  ← GDT/TSS 就绪
    → init_clock()                             (main.c:418)  ← 使用硬编码常量
    → intr_init(0)                             (main.c:472)  ← 使用硬编码常量
    → arch_init()                              (main.c:474)  ← 调用 acpi_init()，解析 ACPI
      → acpi_init()                            (acpi.c:310-342)
        → get_acpi_rsdp() → 搜索 EBDA/BIOS 区域 (acpi.c:204-228)
        → 遍历 RSDT/XSDT
        → acpi_init_poweroff()                 (acpi.c:230-308)
```

> **顺序验证**：`prot_init()` 必须是第一步——它建立 GDT/TSS，后续 `init_clock` 和 `intr_init` 才有用栈和中断入口。这是 Minix3 C 的硬性约束。

因果链验证：

- **因**：C 版时钟/中断初始化不需要 ACPI 地址，它们使用硬编码常量。
- **果**：`acpi_init()` 可以放在 `arch_init()` 中，晚于 `init_clock()`/`intr_init()`。
- **Rust 重写的因变果变**：Rust 要求时钟/中断从 `PlatformDesc` 读取基址，所以 `platform::init_from_kinfo()` 必须前移到 `init_clock_and_interrupts()` 之前（见第 4 章 §4.6）。

---

## 3. Rust 设计决策

### 3.0 设计问题清单

把 C 版的硬编码迁移到 Rust 时，面临五个独立但相互关联的设计问题：

1. **解析放在哪一层**：boot-shim 解析完传给 kernel，还是 kernel 自己解析原始指针？
2. **抽象形态**：`PlatformDesc` 用 trait 还是 struct？子描述符用 enum 还是 trait？
3. **硬件 trait 形态**：`ClockArch`/`InterruptController` 保持静态方法，还是改为实例化？
4. **全局存储**：解析产物放在哪里？如何满足 SMP 安全 + `no_std`？
5. **兜底策略**：QEMU 测试在没有真实 DTB/ACPI 时如何快速通过？

### 3.1 解析职责：方案 C——boot-shim 定位原始指针，kernel 解析

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A** | boot-shim 解析 DTB/ACPI 成结构化数据，通过 `KernelInfo` 传给 kernel | kernel 不需要解析器；`KernelInfo` 字段即用 | `KernelInfo` 膨胀（十几个字段）；boot-shim 越界执行 kernel 工作；难测试 |
| **B** | kernel 直接从固件接口读取（UEFI Runtime Services / SBI calls） | 无需 `KernelInfo` 扩展 | kernel 依赖固件运行时；ExitBootServices 后 UEFI 服务不可用；RISC-V SBI 无硬件描述接口 |
| **C** | boot-shim 定位 DTB/RSDP **原始物理指针**，通过 `KernelInfo` 传递，**kernel 解析** | 职责清晰；`KernelInfo` 只加 1 个字段；kernel 可测试；boot-shim 可替换 | kernel 需要实现解析器（但这是 kernel 的本职） |

**选择方案 C**，理由：

1. **职责边界清晰**：boot-shim 的本职是"固件接口桥接 + kernel 装载 + ExitBootServices"（见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)）。把 FDT/ACPI 解析塞给它，等于让"准备器"开始执行"kernel 才该做的工作"——boot-shim 从"可被 GRUB/UEFI/OpenSBI/U-Boot 替换的统一入口"膨胀成"半个 kernel"，破坏 BootShim trait 的统一抽象。
2. **与 Minix3 C 语义对齐**：C 版的 `acpi_init()`、`bsp_init()` 都是 **kernel 内部**调用。"kernel 自己理解硬件"是微内核的职责边界。
3. **`KernelInfo` 保持精简**：方案 A 要求 `KernelInfo` 扩展 GICD/GICR/PLIC/CLINT 等十几个字段。方案 C 只需加 **1 个字段**（`Option<PlatformDescriptorPtr>`）。
4. **可测试性**：kernel 侧解析器可在 `#[cfg(test)]` 中用 mock DTB/ACPI 字节流测试，不需要启动 QEMU。

职责边界如下：

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

### 3.2 `PlatformDesc` 为什么用 trait 而不是 struct

三个来源（DTB/ACPI/QEMU 兜底）需要被上层代码**统一消费**，因此定义公共接口 `PlatformDesc` trait：

- **测试隔离**：单元测试可构造 `MockPlatformDesc` 返回已知值，不必挂真实 DTB/ACPI 解析（参见 Ch5 测试要点）。
- **扩展性**：新增来源（未来 SMBIOS、用户自定义 .toml 配置等）只需新增一个 `impl PlatformDesc`，上层代码与硬件描述来源解耦。
- **配合 enum 做静态分发**：trait 是"接口"，实际存储是 `PlatformDescEnum`（§3.3 的 enum），三选一编译期可知，零 vtable 开销。`platform_desc()` 返回 `&'static dyn PlatformDesc` 仅在 API 边界做一次类型擦除，调用频率低（init 阶段），不构成热路径。

> 关于"`&dyn` 会引入 vtable 开销"的顾虑：
>
> vtable 是一张编译期生成、放在 `.rodata` 里的函数指针表——`&dyn PlatformDesc` 是一个胖指针（数据指针 + vtable 指针），每次方法调用多 2~3 条 load 指令。**它不依赖 std、不依赖堆、不依赖运行时初始化**，boot 阶段完全可以工作。
>
> 但这个开销在本设计中**几乎可以忽略**：实际存储是 `PlatformDescEnum`，编译器对每个 `match desc.interrupt_controller() { ... }` 都能静态内联到具体变体的方法体；只有从 enum 切到 trait object 那一刻才有一次虚表查找，而这一步发生在 boot 时 init 阶段（`clock.rs:131` 等几处），不在时钟中断等热路径上。热路径上的 `read_tsc()` 拿到 enum 后就走具体类型的寄存器操作，无 vtable 查找。

参见 `os/libs/minix-platform/src/desc.rs:30-45` 的 `PlatformDesc` trait 定义。

### 3.3 子描述符为什么用 enum 而不是 trait

子描述符（`InterruptControllerDesc`、`TimerDesc` 等）用 enum 而非 trait object：

- **形状有限且已知**：中断控制器就是 GICv3 / PLIC / APIC 三类；定时器就是 CLINT / HPET / ARM Generic Timer 等。有限已知集合用 `enum` 表达是 Rust 的惯用法。
- **零开销分发**：`match desc { InterruptControllerDesc::Plic { plic_base, .. } => ... }` 在编译期生成跳转表，比 `dyn Trait` 的 vtable 间接跳转更快。
- **`Copy`/`Clone` 友好**：enum 变体只持有 `usize`/`u32` 等值类型，可以 `Copy`，便于 init 阶段把 `plic_base` 等参数传进硬件 trait 实例。
- **穷尽性检查**：编译器强制 `match` 覆盖所有变体，新增变体时所有调用点会编译失败，提示补全——比 trait object 漏处理更安全。

参见 `os/libs/minix-platform/src/desc.rs:62-106` 的子描述符 enum 定义。

### 3.4 硬件 trait 为什么要带实例状态

trait 方法是否带 `&self`，决定了硬件参数能不能"住进"实例字段——这是本节设计选择的核心。

#### 3.4.1 不带 `&self` 的 trait 方法：参数无处安放

如果 trait 方法没有 `&self`（即所谓"自由函数式 trait"）：

```rust
pub trait ClockArch {
    fn init_timer(hz: u32);     // 没有 &self
    fn read_ticks() -> u64;     // 没有 &self
}
```

则 `impl` 块对应的 struct **拿不到任何实例字段**（因为根本没有 `self` 可写），硬件地址只能有两个去处：

- **硬编码进函数体**：例如 `unsafe { (0x200_BFF8 as *const u64).read_volatile() }`——这等于绑死 QEMU `virt`，换板子全部失灵。
- **塞进全局 `static`**：写起来跟实例化一样丑，还要手动维护单写多读语义。

两条路都不如直接让实例持有地址。

#### 3.4.2 实例化：让硬件参数住在实例字段里

正确做法是 trait 方法带 `&self`，并提供 `new(desc) -> Self` 构造入口：

```rust
pub trait ClockArch: Sized + Send + Sync {
    fn new(desc: &TimerDesc) -> Self;       // ← 实例从 desc 拿到硬件参数
    fn init_timer(&self, hz: u32);          // ← 现在有 &self
    fn read_ticks(&self) -> u64;            // ← 现在有 &self
}

pub struct Riscv64ClockArch {
    mtime_addr: usize,                       // ← 解析出的地址住在实例字段
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &TimerDesc) -> Self {
        let TimerDesc::Clint { mtime_addr, .. } = desc else { panic!("...") };
        Self { mtime_addr }                  // ← desc → 实例字段的一次性搬迁
    }
    fn read_ticks(&self) -> u64 {
        unsafe { (self.mtime_addr as *const u64).read_volatile() }
    }
}
```

调用方在 init 阶段一次性建好实例，热路径（如时钟中断）只访问 `self.mtime_addr`——经编译器优化后等价于直接 `mov + load`，零间接开销。

#### 3.4.3 同样模式应用到其他硬件 trait

`InterruptController` 和 `ArchInit` 采用同样的实例化模式，区别只是构造参数：

| trait | 实例字段 | 构造参数 |
|-------|---------|---------|
| `ClockArch`（clock.rs:183） | `mtime_addr` / `lapic_base` / 等 | `&TimerDesc` |
| `InterruptController`（interrupt.rs） | `gicd_base` / `plic_base` / 等 | `&InterruptControllerDesc` |
| `ArchInit`（arch_init.rs:47-61） | 架构相关 misc 参数 | `&ArchMiscDesc` |

参见 `os/arch/src/arch/clock.rs:183` 定义带 `&self` 的 `ClockArch` trait；`os/plat/src/interrupt.rs:128` 定义 `InterruptController` trait；`os/arch/src/arch/arch_init.rs:47-61` 定义 `ArchInit` trait。

### 3.5 全局存储为什么用 `AssumeSyncCell` 而非 `Mutex`/`static mut`

| 方案 | 否决理由 |
|------|---------|
| `static mut` | 读写必须包在 `unsafe` 块里，且 `unsafe fn` 内部访问 mutable static 也需独立 `unsafe` 块（Rust 2024 edition 收紧）——把"单写多读"的安全契约藏在散落的 unsafe 里，文档难以集中维护 |
| `Mutex`/`spin::Mutex` | boot 阶段单线程，运行时只读——加锁是多余开销 |
| `OnceLock` | `no_std` 下不可用（需要分配器或 `std`） |

`AssumeSyncCell<T>` 是项目内统一的"UnsafeCell + 手动 `Sync`"原语，定义在 `os/libs/minix-types/src/types/cell.rs`，VM server、heap arena、vmproc 表等都已在用。它的安全契约：**调用方保证单线程独占访问**。`platform_desc` 的"boot 阶段单线程写入一次，之后所有 CPU 只读访问"恰好满足这一契约。

> **参见**: `os/libs/minix-types/src/types/cell.rs:51` 定义 `AssumeSyncCell`；`os/libs/minix-platform/src/global.rs` 中以 `static PLATFORM: AssumeSyncCell<Option<PlatformContext>>` 形式持有全局描述符。

### 3.6 `QemuVirtDesc` 兜底实现

`QemuVirtDesc` 是 `PlatformDesc` 的具体实现，返回 QEMU `virt` 机器各架构固定的硬件参数（PLIC/GICv3/APIC 基地址、CLINT/ArmGenericTimer/PIT 参数、串口地址）。它是 `platform::init_from_kinfo` 在 `kinfo.platform_descriptor` 解析失败或未提供时的兜底路径——硬编码值保证这条路径永远可用，使得即便 boot-shim 异常，仍能输出诊断信息而不至于连 panic 信息都看不到。

> **关于 `#[cfg(target_arch)]`**：`QemuVirtDesc` 里的 `#[cfg]` 仅用于"选择该架构的 QEMU virt 固定值"，是**数据选择**而非**行为选择**——每个分支只返回常量描述符数据，不改变代码控制流。`QemuVirtDesc` 本身是具体类型（不是 `impl` 块里的 `if/else`），不污染上层 trait 抽象。真实硬件路径走 `DeviceTreeDesc`/`AcpiDesc`，完全无 `#[cfg]`。详见 [00-kernel-overview.md](00-kernel-overview.md) §1.5。
>
> 参见 `os/libs/minix-platform/src/qemu_virt.rs:22-144` 定义 `QemuVirtDesc` 与各架构参数。
>
> **已知缺陷**：当前 QEMU 测试路径刻意走 `QemuVirtDesc` 而非 DTB/ACPI parser，导致 parser 主路径在测试中无覆盖——测试与生产路径不一致。QEMU 的 `virt` 机器本就提供 DTB（aarch64/riscv64）和 ACPI（x86-64），测试本应走 parser 主路径。详见下 §11。

> **TODO（重构 `QemuVirtDesc`——按架构分支的硬编码实现不适合当前抽象形态）**：当前 `QemuVirtDesc` 的每个 trait 方法（`interrupt_controller`、`timer`、`early_console`、`cpu_topology`）都**遍布 `#[cfg(target_arch)]` 条件编译**——每个方法、每个架构都是一个独立分支返回 QEMU virt 机器固定硬件参数。这种设计有三个问题：
> 1. **代码高度重复**：每个方法的三个 `cfg` 分支几乎是同一份代码的三份副本（仅返回值不同），新增架构要在 4 个方法 × N 个分支中各加一行。
> 2. **现状已被 §11 重新定义**：[§11.3](04-platform-discovery.md#113-qemuvirtdesc-设计反思兜底该不该存在) 重新定义 `QemuVirtDesc` 角色——从"测试与生产的共同路径"回归到"boot-shim 失败时的诊断通道"。这意味着 `QemuVirtDesc` 本来就会被 §11 修复路径（不在测试/生产正常路径触发）带动——而 §11 修复后实际上 **`QemuVirtDesc` 永远不会被触发**（见 §11.4.5）。也就是说，如果 §11 完全实施，本代码当前的形态自然会被推到边缘。
> 3. **与硬件抽象边界 TODO 联动**：见 [§4.1.2 后 TODO](04-platform-discovery.md#412-子描述符-enum-与三架构映射) ——enum 变体重设计 + trait 抽取是更彻底的方案，`QemuVirtDesc` 的实现也会同步简化（每个方法返回 `&dyn HardwareDesc`，不需要 cfg 分支选择具体硬件类型）。
>
> **建议方向**：
> - **短期**：与 [§11 QEMU 测试路径修复](04-platform-discovery.md#11-qemu-测试与生产路径不一致qemuvirtdesc-替代了-dtbacpi-parser) 同步处理。修复后 `QemuVirtDesc` 仅在 boot-shim panic 前调用一次（用于输出诊断），其代码精简度可以接受——不必为兜底代码做过度的工程化。
> - **长期**：若 §4.1.2 enum 重设计落地（trait object），`QemuVirtDesc` 的 `interrupt_controller` / `timer` / `early_console` 等方法改为返回 `&dyn InterruptControllerDesc` / `&dyn TimerDesc` / `&dyn ConsoleDesc`，每个架构一个具体 struct（如 `QemuVirtRiscv64Desc`），三个 impl——彻底消除 cfg 分支。
> - **不必为 §4.2.1 单独重构**：与上面两个 TODO 合并评审，避免分散改动。
> 优先级：**P2**（与 §11 + §4.1.2 合并评审，避免单点重构）。
>
> ---
>
> ## 11. QEMU 测试与生产路径不一致：`QemuVirtDesc` 替代了 DTB/ACPI parser（来自 [todo.md](todo.md) §11）
>
> > `QemuVirtDesc` **不是设计缺陷**——它的合法用途是 boot-shim 完全失败时的最后兜底（panic 前还能输出诊断信息）。但**当前测试路径刻意走它**而非 DTB/ACPI parser，违背了 "test what you fly" 原则。
>
> ### 11.1 问题陈述
>
> **事实链**：
>
> 1. QEMU `virt` 机器**本身就提供 DTB/ACPI**——aarch64/riscv64 提供 DTB（GICv3/PLIC 基地址、virtio-mmio 设备），x86-64 提供 ACPI 表（RSDP→XSDT→MADT）。QEMU 在启动时把这些嵌入固件接口，boot-shim 完全有条件拿到。
> 2. 当前 `boot-shim` **在某些测试路径显式构造 `platform_descriptor: None`**（见 `os/boot-shim/src/opensbi_helpers.rs:535`、`os/boot-shim/src/uefi_helpers.rs:79` 注释 "use the QEMU fallback"）。
> 3. `platform::init_from_kinfo` 看到 `None` → 走 `QemuVirtDesc` 兜底路径 → 用硬编码常量填充硬件参数。
> 4. 结果：**DTB parser（aarch64/riscv64）和 ACPI parser（x86-64）在测试中根本不跑**。
>
> **后果**：
>
> | 后果 | 严重度 |
> |------|-------|
> | DTB/ACPI parser 有 bug 也发现不了（测试绿但生产挂） | **P0**——隐性故障源 |
> | QEMU 升级后 virt 机器布局漂移（例如 PLIC 基地址变了），硬编码常量过时，测试还过——真实硬件走 parser 拿到的是新值，跟测试常量不一致 | **P1**——版本漂移 |
> | 测试覆盖率统计失真（parser 主路径 0% 覆盖，但报告里看不出来） | **P1**——决策失据 |
> | 本文档 §3.6 原表述 "QEMU 测试不需要 DTB/ACPI 解析器" 是**因果倒置**——不是"不需要"，是"故意不用"，掩盖了上面的问题 | **P0**——文档误导 |
>
> ### 11.2 触发条件
>
> **QEMU 测试路径故意走 `QemuVirtDesc` 的代码位置**：
>
> | 文件 | 行号 | 上下文 |
> |------|------|--------|
> | `os/boot-shim/src/opensbi_helpers.rs` | 535 | `None, // platform_descriptor` 测试用 `KernelInfo` 构造 |
> | `os/boot-shim/src/opensbi_helpers.rs` | 305 | `"Passing 0 is equivalent to 'no DTB available' (kernel uses QEMU fallback)"` |
> | `os/boot-shim/src/uefi_helpers.rs` | 79 | `"use the QEMU fallback"` 注释 |
> | `os/arch/src/x86_64/proc_arch.rs` | 350 | mock 路径，`platform_descriptor: None` |
> | `os/arch/src/riscv64/proc_arch.rs` | 252 | mock 路径 |
> | `os/arch/src/arm64/proc_arch.rs` | 279 | mock 路径 |
>
> ### 11.3 `QemuVirtDesc` 设计反思——兜底该不该存在？
>
> **答：兜底应该保留，但角色要从"测试与生产的共同路径"回归到"boot-shim 失败时的诊断通道"**。
>
> | 场景 | 是否应该触发 `QemuVirtDesc` |
> |------|----------------------|
> | **生产正常路径** | ❌ 不应触发——DTB/ACPI parser 必拿到真实硬件参数 |
> | **测试正常路径** | ❌ 不应触发——QEMU 提供的 DTB/ACPI 一定能拿到 |
> | **boot-shim 完全失败** | ✅ 触发——panic 前**还能输出诊断信息**，不至于连 panic 信息都看不到 |
> | **dev 构建下 parser 失败** | ✅ 触发并 warn——dev 容忍但保留诊断通道；release panic |
>
> 也就是说，**删 `QemuVirtDesc` 是不对的**（删除后 boot-shim 出 bug 就只剩"乱码 panic"，调试成本极高）。但**当前测试问题不是"删它"，是"测试不该走它"**——这是两件事。
>
> ### 11.4 修复方向
>
> **目标**：让 QEMU 测试走完整的 boot-shim → kernel → `platform::init_from_kinfo` → DTB/ACPI parser 主路径，跟真实硬件完全一致。修复后，`QemuVirtDesc` 仅在 boot-shim 完全失败时兜底（panic 前输出诊断）。
>
> **步骤**：
>
> 1. **boot-shim 改造**：在 `find_platform_descriptor()` 中确认 QEMU 提供的 DTB/RSDP 一定可拿到（即便在测试固件中），而不是仅在某些路径返回 `Some(...)`，另一些路径返回 `None`。
> 2. **替换 `None` 为 `Some`**：
>    - `os/boot-shim/src/opensbi_helpers.rs:535` 测试用 `KernelInfo` 改为传 `Some(PlatformDescriptorPtr::Dtb(dtb_phys))`
>    - 三个 `proc_arch.rs:350/252/279` 的 mock 路径改为传真实 QEMU 提供的 DTB/RSDP
> 3. **DTB/ACPI parser 验证**：跑通一次完整 QEMU 测试，验证 parser 正确解析 QEMU 提供的 DTB/ACPI——这一步可能暴露 parser 的既有 bug（这正是目的）。
> 4. **修复 parser bug**：如果在 11.4.3 暴露 parser bug，修复并加单测覆盖。
> 5. **`QemuVirtDesc` 角色回归**：修复后 `QemuVirtDesc` 应该**仅在 boot-shim 完全失败时**作为最后兜底（panic 前还能输出一点诊断）。如果新路径下 `QemuVirtDesc` 永远不会被触发，那是好事——说明 boot-shim 总是能正常工作。
> 6. **更新 §3.6**：完成后删除 §3.6 的"已知缺陷"标注，因为问题已解决。
>
> ### 11.5 优先级
>
> **P0**——这是隐性故障源，会让 parser 的 bug 偷偷溜过去。修复工作量不大（主要是 boot-shim 调整 + parser 验证），但需要先把 §9.3（`os/plat` 拆分未完成）一并处理，否则 test-kernel 无法改 import 路径。
>
> ### 11.6 风险与缓解
>
> | 风险 | 缓解 |
> |------|------|
> | 修复后测试大规模失败（parser bug 暴露） | 这是预期收益，不是风险；记录并修复 |
> | QEMU 不同版本提供的 DTB/ACPI 字段有差异 | 锁版本（CI 用固定 QEMU 版本），并在 parser 中容忍未知字段 |
> | boot-shim 在某些 firmware 配置下确实找不到 DTB/RSDP | `QemuVirtDesc` 兜底保留——这是它的**合法用途** |
> | 修改波及 17 个 test-kernel（todo.md §9.3） | 与 §9.3 同步处理，合并 PR |
>
> ### 11.7 与其他章节的关系
>
> - **§9.3**（`os/plat` 拆分未完成）：本次修改需要 test-kernel 改 import 路径（从 `minix_plat::arm64` 等迁移到 `minix_arch::arch::*`），应在 §9.3 解决时同步做。
> - **§11.4.6** 完成后，**§3.6 的"已知缺陷"标注**应同步删除——避免文档与代码现实脱节。
> - **与 TODO 链**（[01-boot-shim-bootstrap.md TODO#1](01-boot-shim-bootstrap.md) 即"`PlatformDescriptorPtr` 抽象泄漏"，本 TODO 是同一问题的另一面向）应合并评审。

### 3.7 `KernelInfo` 扩展字段设计

boot-shim 通过 `KernelInfo` 向 kernel 传递 DTB/RSDP 的**原始物理指针**（不解析）。用一个 `enum` 字段而非两个独立字段：

- DTB 和 RSDP 是两种**不同格式**的数据。boot-shim 在定位时已经知道它找到的是什么。
- 用一个 `enum` 字段语义清晰：`Dtb` 走 DTB 解析路径，`Rsdp` 走 ACPI 解析路径。

**当前限制**：`PlatformDescriptorPtr` 是 sum type（要么 DTB、要么 RSDP），无法同时持有两者。少数场景两者并存：UEFI 固件把 DTB 作为 Configuration Table 提供 + ACPI 表（x86 嵌入式、Windows-on-ARM、ARM/RISC-V 服务器等）。当前设计无法表达——是已知限制，需要时需扩展为独立字段或元组。

> **TODO（review `PlatformDescriptorPtr` 设计时一并处理，**建议多 AI bagging**）**：当前 `PlatformDescriptorPtr { Dtb, Rsdp }` 在 `KernelInfo` 公共 API 表面**显式列出固件描述符类型**，与本文档 §3.1 "上层代码完全屏蔽设备差异" 的设计哲学矛盾；本文 l371 进一步揭示了单 sum type 的表达局限（无法表达 DTB+ACPI 并存）。该字段的设计需要**重新设计**而非简单修补。详见 [01-boot-shim-bootstrap.md TODO#1](01-boot-shim-bootstrap.md)。
>
> **建议范围**（多 AI bagging 时重点讨论）：
>
> 1. **是否拆分为裸指针 + 私有来源标签**：参照 l371 提到的"独立字段或元组"思路，把 `platform_descriptor: PhysBytes`（裸指针）+ `platform_source: PlatformSource`（仅 `init_from_kinfo` 内部可见的私有 enum）作为备选。
> 2. **是否引入元组/数组承载并存**：如 `(Option<Dtb>, Option<Rsdp>)` 或 `Vec<PlatformDescriptorPtr>`，覆盖多描述符并存的服务器场景。
> 3. **是否完全收束到 trait object**：由 boot-shim crate 实现 `ParseDescriptor` trait，kernel 完全通过 trait object 调用（与 `QemuVirtDesc`/DTB/ACPI 三实现的 §3.2 抽象一致）。
> 4. **是否保持 sum type 但扩展变体**：如新增 `Both { dtb, rsdp }` 变体表达并存——扩展性 vs 不破坏现有调用点的权衡。
>
> **评估维度**：
> - 与 `PlatformDescEnum`（§3.2）抽象的对称性——`PlatformDescEnum` 是 enum 但对外屏蔽（用 trait 消费），`PlatformDescriptorPtr` 是 enum 但**对外暴露**
> - 对 `KernelInfo` 字段数量的影响——当前 1 字段，重构后可能为 2-3 字段
> - 是否仍能被 boot-shim 单一实现（UefiBootShim + OpenSbiBootShim + QemuVirtDesc）覆盖——不引入新的实现门槛
> - 与本文 l371 "需要时需扩展为独立字段或元组" 文字对照——重构后该限制是否完全消除
>
> **bagging 形式建议**：至少 3 份独立 review（不同 AI/不同视角）：
> - 一份专攻"完全屏蔽差异"哲学，给出最激进的 trait object 方案
> - 一份专攻"与现有 KernelInfo 字段数量兼容"，给出最保守的 1 字段方案
> - 一份专攻"形而上学一致性"——`PlatformDescriptorPtr` 与 `PlatformDescEnum` 是否必须共享相同的抽象边界
>
> 三份结果对比后选 1 个方案实施，并在本文档与 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) 同步更新叙述与 TODO。

> 参见 `os/libs/minix-boot/src/kernel_info.rs:78-92` 定义 `platform_descriptor: Option<PlatformDescriptorPtr>` 与 `PlatformDescriptorPtr` enum。

---

## 4. Rust 实现

> **本章导读**：按"抽象 → 实现 → 消费"三个层次组织——
> §4.1 定义 `PlatformDesc` trait（读者先知道"长什么样"）；
> §4.2-§4.4 给出 trait 的三种实现（QemuVirt 兜底、DTB 解析、ACPI 解析）+ 全局存储 + KernelInfo 扩展字段（kernel 如何拿到 PlatformDesc）；
> §4.5 是 §3.4 决策的对应实现章节（硬件 trait 如何用 PlatformDesc 实例化）；
> §4.6-§4.9 是运行时约束（启动时序、硬编码迁移、多核预留、no_std）。

### 4.1 `PlatformDesc` trait

#### 4.1.1 trait 定义

```rust
// os/libs/minix-platform/src/desc.rs:30-45

/// 平台硬件描述的统一抽象。
///
/// 一个 `PlatformDesc` 实例回答："我跑在什么硬件上？"。
/// 上层（ClockArch / InterruptController / ArchInit）只读这个抽象，
/// 不接触 FDT/ACPI 原始字节。
///
/// 实现必须 `Send + Sync`（BKL 释放窗口内可被其他 CPU 访问）。
pub trait PlatformDesc: Send + Sync + core::fmt::Debug {
    fn interrupt_controller(&self) -> InterruptControllerDesc;
    fn timer(&self) -> TimerDesc;
    fn early_console(&self) -> Option<ConsoleDesc>;
    fn cpu_topology(&self) -> CpuTopology;
    fn arch_misc(&self) -> ArchMiscDesc;
    fn source(&self) -> PlatformSource;
}
```

#### 4.1.2 子描述符 enum 与三架构映射

```rust
// os/libs/minix-platform/src/desc.rs:62-106

pub enum InterruptControllerDesc {
    Apic { lapic_base: usize, ioapic_base: usize, nr_irqs: u32 },
    Gicv3 { gicd_base: usize, gicr_base: usize, gicr_stride: usize, nr_irqs: u32 },
    Plic { plic_base: usize, nr_irqs: u32, context: u32 },
}

pub enum TimerDesc {
    Pit { pit_base_freq: u32, lapic_base: usize },
    ArmGenericTimer,
    Clint { mtime_addr: usize, mtimecmp_base: usize, mtimecmp_stride: usize, freq: u64 },
}
```

> **TODO（重设计 `InterruptControllerDesc` / `TimerDesc` enum——硬件泄漏问题）**：[§4.1.2](04-platform-discovery.md#412-子描述符-enum-与三架构映射) 当前 enum 变体直接命名为 `Apic` / `Gicv3` / `Plic` / `Pit` / `Clint` / `ArmGenericTimer`，**变体名硬编码硬件品牌**；字段名（`gicr_stride` / `mtimecmp_stride` 等）也直接暴露硬件寄存器布局。上层代码（`ClockArch::new(desc)`、`InterruptController::new(desc)`）拿到 `InterruptControllerDesc::Plic { .. }` 后，**必须知道当前是哪家硬件**——这本身就违反"上层代码对底层硬件实现无感知"的设计哲学。
>
> **变更范围**：
> - §3.3 论据 "enum 零开销分发" + "穷尽性匹配" 仍然成立——变体数量可控；
> - 但"变体名 = 硬件名" + "字段名 = 寄存器布局" 需要重新设计。
> - 同步影响 §4.1.2 子描述符定义、§4.5 硬件 trait 实例化（trait 拿到 `&dyn InterruptControllerDesc`）、§4.4 KernelInfo 字段（不变，但消费方式变）。
> - 不影响 §4.6 `QemuVirtDesc` 兜底语义（兜底应保留——详见 [§11.3](04-platform-discovery.md#113-qemuvirtdesc-设计反思兜底该不该存在)）。
>
> **建议方向**（`InterruptControllerDesc` 为例，三种方案）：
> 1. **变体命名按功能形状而非硬件品牌**：`Single { base, nr_irqs }` / `Distributed { dist_base, redist_base, redist_stride, nr_irqs }` / `Priority { base, nr_irqs, context }`——最小改动，但调用方需要理解"形状语义"
> 2. **trait object 替代 enum**（推荐）：`pub trait InterruptControllerDesc: Send + Sync { fn base_address(&self) -> usize; fn nr_irqs(&self) -> u32; ... }`，三种 impl（ApicDesc/Gicv3Desc/PlicDesc）；顶层 `PlatformDesc` 改为返回 `&dyn InterruptControllerDesc`。开销在 init 阶段一次性 `new(desc)`，**不在热路径**（与 [§3.2 vtable 开销分析](04-platform-discovery.md#32-platformdesc-为什么用-trait-而不是-struct) 一致）
> 3. **泛型 PlatformDesc<Arch>**：`pub trait PlatformDesc<Arch: ArchName>`，编译期静态分发；零开销但改动面大（§4 全部章节 + 消费者 trait 都需泛型化）
>
> **评估维度**：
> - 现有 §4.5 硬件 trait（`ClockArch`/`InterruptController`/`ArchInit`）的实化路径（`new(desc)`）改造量
> - 新增硬件时的改动范围（riscv64 AIA = APLIC/IMSIC 已迫在眉睫，[todo.md §7](todo.md#7-platformdesc--硬件发现) 提到）
> - 与 `PlatformDescriptorPtr` redesign TODO 的合并可能性（两者都涉及 `KernelInfo` 与 trait 抽象边界）
> - 测试侧改造（mock 实现、QEMU 测试路径）
>
> **触发条件**：建议与 [§11 QEMU 测试路径修复](04-platform-discovery.md#11-qemu-测试与生产路径不一致qemuvirtdesc-替代了-dtbacpi-parser) + [TODO#1 PlatformDescriptorPtr redesign](01-boot-shim-bootstrap.md) 合并评审——三方都是"硬件抽象边界"问题，统一解决比分散多次更可维护。
> 优先级：**P1**（不阻塞当前任务，但新增硬件时必须先解决；riscv64 AIA 是已知需求）。

> **为什么 ARM64 Generic Timer 没有频率字段**：ARM 架构约定固件（UEFI/ATF）在启动时将定时器频率写入 `CNTFRQ_EL0` 系统寄存器。kernel 直接 `mrs CNTFRQ_EL0` 读取，不需要从 DTB 解析。这是架构规范，不是设计遗漏。

三架构映射：

| 架构 | `interrupt_controller()` 返回 | `timer()` 返回 |
|------|------------------------------|----------------|
| x86-64 | `InterruptControllerDesc::Apic { lapic_base, ioapic_base, .. }` | `TimerDesc::Pit { pit_base_freq, lapic_base }` |
| aarch64 | `InterruptControllerDesc::Gicv3 { gicd_base, gicr_base, .. }` | `TimerDesc::ArmGenericTimer` |
| riscv64 | `InterruptControllerDesc::Plic { plic_base, .. }` | `TimerDesc::Clint { mtime_addr, mtimecmp_base, freq, .. }` |

#### 4.1.3 为什么 trait 需要 `source()` —— 类型擦除后的手动 RTTI

`source()` 是 trait 里**唯一不属于硬件参数**的方法——它返回 `PlatformSource` enum，标识"我来自 DTB 解析 / ACPI 解析 / QemuVirt 兜底"。这看似违反了"trait 只描述硬件"的抽象纯度，实际上是 Rust trait object 类型擦除带来的必要妥协。

**问题**：上层代码拿到 `&dyn PlatformDesc` 时，**不知道底层是 `DeviceTreeDesc` 还是 `QemuVirtDesc`**——Rust 没有 RTTI（运行时类型信息），trait object 在 vtable 里只暴露 trait 方法，不暴露具体类型。

**手动 RTTI 方案**：让每个实现自己声明自己的"出身"：

```rust
// 各实现的 source() 直接返回常量
impl PlatformDesc for QemuVirtDesc { fn source(&self) -> PlatformSource { PlatformSource::QemuVirt } }
impl PlatformDesc for DeviceTreeDesc { fn source(&self) -> PlatformSource { PlatformSource::DeviceTree } }
impl PlatformDesc for AcpiDesc { fn source(&self) -> PlatformSource { PlatformSource::Acpi } }
```

**合法用途**：

| 用途 | 例子 | 评价 |
|------|------|------|
| 诊断日志 | `log!("running on {:?} source", desc.source())` | ✅ 合理——便于排查"为什么硬件参数看起来不对" |
| 测试断言 | `assert_eq!(desc.source(), PlatformSource::QemuVirt)` | ✅ 合理——确认 dispatch 路径 |
| panic 信息 | `panic!("descriptor is null ({:?})", desc.source())` | ✅ 合理——区分"没有 descriptor"vs"拿到了但解析失败" |

**反模式（应当避免）**：

```rust
// ❌ 千万别这么写——破坏了 trait 统一抽象
match desc.source() {
    PlatformSource::QemuVirt => skip_acpi_debug(),
    _ => run_acpi_debug(),
}
```

**为什么是反模式**：上层代码应当只关心硬件参数（PLIC 在哪、CLINT 时钟多少 Hz），不应当因为"硬件参数来自 QemuVirt"就走不同分支。这会把 trait 的"屏蔽差异"目标破坏掉。

**设计权衡**：把 `source()` 放在 `PlatformDesc` trait 里 vs 单独搞个 `HasPlatformSource` trait？前者简单（一个 trait 满足所有元信息查询），后者抽象更纯（明确区分"硬件参数"和"元信息"两层）。当前选择前者——**简洁性优先于抽象纯度**——因为元信息查询需求很低，不会演化成主要扩展点。如果未来 `source()` 衍生出 `version()`、`format_revision()` 等多种元信息查询，再考虑拆分独立 trait。

> 参见 `os/libs/minix-platform/src/desc.rs:47-53` 定义 `PlatformSource` enum；`os/libs/minix-platform/src/qemu_virt.rs:140` / `device_tree.rs:382` / `acpi.rs:242` 三处 `source()` 实现各返回自身对应变体。

### 4.2 `PlatformDesc` 的三种实现

> **设计决策**：§3.1 选定"方案 C——boot-shim 定位原始指针，kernel 解析"。本节给出三种具体实现，对应原始指针的两种来源（DTB / ACPI）+ 一种兜底（QemuVirt）。

#### 4.2.1 `QemuVirtDesc`：硬编码兜底

```rust
// os/libs/minix-platform/src/qemu_virt.rs:22-144

#[derive(Debug)]
pub struct QemuVirtDesc;

impl PlatformDesc for QemuVirtDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        #[cfg(target_arch = "riscv64")]
        { InterruptControllerDesc::Plic { plic_base: 0x0C00_0000, nr_irqs: 64, context: 1 } }
        #[cfg(target_arch = "aarch64")]
        { InterruptControllerDesc::Gicv3 { gicd_base: 0x0800_0000, gicr_base: 0x080A_0000, gicr_stride: 0x2_0000, nr_irqs: 64 } }
        #[cfg(target_arch = "x86_64")]
        { InterruptControllerDesc::Apic { lapic_base: 0xFEE0_0000, ioapic_base: 0xFEC0_0000, nr_irqs: 64 } }
    }

    fn timer(&self) -> TimerDesc {
        #[cfg(target_arch = "riscv64")]
        { TimerDesc::Clint { mtime_addr: 0x200_BFF8, mtimecmp_base: 0x200_4000, mtimecmp_stride: 8, freq: 10_000_000 } }
        #[cfg(target_arch = "aarch64")]
        { TimerDesc::ArmGenericTimer }
        #[cfg(target_arch = "x86_64")]
        { TimerDesc::Pit { pit_base_freq: 1_193_182, lapic_base: 0xFEE0_0000 } }
    }

    fn early_console(&self) -> Option<ConsoleDesc> {
        #[cfg(target_arch = "x86_64")]
        { Some(ConsoleDesc::IsaSerial { port_base: 0x3F8 }) }
        #[cfg(target_arch = "aarch64")]
        { Some(ConsoleDesc::MmioSerial { mmio_base: 0x0900_0000 }) }
        #[cfg(target_arch = "riscv64")]
        { Some(ConsoleDesc::SbiConsole) }
    }

    fn cpu_topology(&self) -> CpuTopology {
        const NR_CPUS: u32 = 4;
        let mut cpus = [CpuInfo::default(); MAX_CPUS];

        #[cfg(target_arch = "riscv64")]
        {
            const MTIMECMP_BASE: usize = 0x200_4000;
            const MTIMECMP_STRIDE: usize = 8;
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo {
                    hw_id: i as u64,
                    gicr_base: None,
                    mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE),
                };
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            const GICR_BASE: usize = 0x080A_0000;
            const GICR_STRIDE: usize = 0x2_0000;
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo {
                    hw_id: i as u64,
                    gicr_base: Some(GICR_BASE + i * GICR_STRIDE),
                    mtimecmp_addr: None,
                };
            }
        }

        #[cfg(target_arch = "x86_64")]
        {
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo { hw_id: i as u64, gicr_base: None, mtimecmp_addr: None };
            }
        }

        CpuTopology { nr_cpus: NR_CPUS, bsp_id: 0, cpus }
    }

    fn arch_misc(&self) -> ArchMiscDesc { ArchMiscDesc::default() }
    fn source(&self) -> PlatformSource { PlatformSource::QemuVirt }
}
```

#### 4.2.2 `DeviceTreeDesc` 解析器（aarch64 / riscv64）

```rust
// os/libs/minix-platform/src/device_tree.rs:42-60

#[derive(Clone, Copy)]
pub struct DeviceTreeDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl DeviceTreeDesc {
    pub unsafe fn parse(dtb_phys: usize) -> Result<Self, DtParseError> { ... }
    pub fn from_bytes(dtb: &[u8]) -> Result<Self, DtParseError> { ... }
}
```

关键设计：**eager parsing**。`Fdt` 借用 DTB 切片，但 `PlatformDesc` 必须 `'static + Send + Sync`。`parse()` 一次性遍历 FDT，把所有需要的值提取到 `usize`/`u32`/`u64` 字段中，然后丢弃 `Fdt` 借用。结果 `DeviceTreeDesc` 是 `'static` 且无需分配器。

> 参见 `os/libs/minix-platform/src/device_tree.rs:61-365`。

#### 4.2.3 `AcpiDesc` 解析器（x86-64）

```rust
// os/libs/minix-platform/src/acpi.rs:108-115

#[derive(Clone, Copy)]
pub struct AcpiDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl AcpiDesc {
    pub unsafe fn parse(rsdp_phys: usize) -> Result<Self, AcpiParseError> { ... }
}
```

解析链：RSDP → XSDT/RSDT → MADT（`"APIC"`）。从 MADT 提取：

- LAPIC base（MADT header 的 `Local APIC Address`）。
- IOAPIC base（第一个 `IOAPIC` 结构记录的 `ioapic_addr`）。
- CPU 拓扑（`Processor LAPIC` 结构记录，按 `flags & 1` 判断是否启用）。

> 参见 `os/libs/minix-platform/src/acpi.rs:127-225`。

### 4.3 `PlatformContext` 全局存储

> **设计决策**：§3.5 选定 `AssumeSyncCell` 作为"单线程全局状态"统一原语。本节给出具体实现。

```rust
// os/libs/minix-platform/src/global.rs:40-49

static PLATFORM: AssumeSyncCell<Option<PlatformContext>> = AssumeSyncCell::new(None);

pub struct PlatformContext {
    pub desc: PlatformDescEnum,
}

pub enum PlatformDescEnum {
    DeviceTree(DeviceTreeDesc),
    Acpi(AcpiDesc),
    QemuVirt(QemuVirtDesc),
}
```

初始化与访问 API：

```rust
// os/libs/minix-platform/src/global.rs:159-247

/// 初始化全局平台上下文。在 T2.5 阶段调用。
pub unsafe fn init(desc: PlatformDescEnum) {
    unsafe { *PLATFORM.get() = Some(PlatformContext { desc }) };
}

/// 根据 KernelInfo 构造 PlatformDesc 并初始化全局。
pub unsafe fn init_from_kinfo(kinfo: &KernelInfo) {
    let desc = match kinfo.platform_descriptor {
        Some(PlatformDescriptorPtr::Dtb(pa)) => {
            match unsafe { DeviceTreeDesc::parse(pa.0 as usize) } {
                Ok(d) => PlatformDescEnum::DeviceTree(d),
                Err(_e) => qemu_fallback_or_panic("DTB parse failed"),
            }
        }
        Some(PlatformDescriptorPtr::Rsdp(pa)) => {
            match unsafe { AcpiDesc::parse(pa.0 as usize) } {
                Ok(d) => PlatformDescEnum::Acpi(d),
                Err(_e) => qemu_fallback_or_panic("ACPI parse failed"),
            }
        }
        None => qemu_fallback_or_panic("no platform descriptor provided by boot-shim"),
    };
    unsafe { init(desc) };
}

/// 获取平台描述引用。
pub fn platform_desc() -> &'static dyn PlatformDesc {
    unsafe {
        (*PLATFORM.get())
            .as_ref()
            .expect("platform_desc() called before init_from_kinfo()")
    }
}

/// dev 构建 warn-and-fallback；release 构建 panic。
fn qemu_fallback_or_panic(reason: &str) -> PlatformDescEnum {
    if cfg!(debug_assertions) {
        PlatformDescEnum::QemuVirt(QemuVirtDesc)
    } else {
        panic!("platform::init_from_kinfo: {} and not a dev build", reason);
    }
}
```

`init_from_kinfo()` 的错误处理策略：

- **dev 构建**（`debug_assertions` 启用）：warn-and-fallback——返回 `QemuVirtDesc`，测试不中断。
- **release 构建**：panic——真实硬件不能没有描述符运行。

### 4.4 `KernelInfo` 扩展与 `PlatformDescriptorPtr`

> **设计决策**：§3.7 选定 "enum 字段 + sum type 限制"。本节给出具体定义与 boot-shim 端定位方式。

```rust
// os/libs/minix-boot/src/kernel_info.rs:11-95

pub struct KernelInfo {
    // ... 现有字段保持不变 ...

    /// 平台描述符原始指针（DTB 或 RSDP 的物理地址）。
    /// `None` 表示 boot-shim 未提供（QEMU virt 兜底路径）。
    pub platform_descriptor: Option<PlatformDescriptorPtr>,
}

#[derive(Debug, Clone, Copy)]
pub enum PlatformDescriptorPtr {
    /// Flattened Device Tree 物理地址（ARM64/RISC-V）。
    Dtb(PhysBytes),
    /// ACPI RSDP 物理地址（x86-64）。
    Rsdp(PhysBytes),
}
```

boot-shim 定位原始指针的位置：

- UEFI 路径：`os/boot-shim/src/uefi_helpers.rs:33-78` 扫描 UEFI Configuration Table，x86-64 查找 ACPI GUID，aarch64 优先查找 DTB GUID。
- RISC-V 路径：OpenSBI 在 `a1` 寄存器传递 DTB 物理地址（由 boot-shim 汇编入口保存到 `KernelInfo`）。

### 4.5 硬件 trait 实例化实现（§3.4 的对应实现）

> **设计决策**：§3.4 选定"硬件 trait 必须带实例状态，地址在 `new(desc)` 注入"。本节给出具体实现案例。
>
> 当前只有 RISC-V 64 时钟有完整实现代码作为示例；aarch64/x86-64 时钟与各架构 `InterruptController` / `ArchInit` 严格遵循同一模式，参见 §3.4.3 的"同样模式应用到其他硬件 trait"段。

#### 4.5.1 `ClockArch` 实例化：RISC-V 64

```rust
// os/arch/src/riscv64/clock.rs

pub struct Riscv64ClockArch {
    mtime_addr: usize,
    mtimecmp_base: usize,
    mtimecmp_stride: usize,
    freq: u64,
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &TimerDesc) -> Self {
        match desc {
            TimerDesc::Clint { mtime_addr, mtimecmp_base, mtimecmp_stride, freq } => Self {
                mtime_addr: *mtime_addr,
                mtimecmp_base: *mtimecmp_base,
                mtimecmp_stride: *mtimecmp_stride,
                freq: *freq,
            },
            _ => panic!("Riscv64ClockArch::new: expected TimerDesc::Clint"),
        }
    }

    fn init_timer(&mut self, hz: u32) {
        let mtime: u64 = unsafe { core::ptr::read_volatile(self.mtime_addr as *const u64) };
        let interval = self.freq / hz as u64;
        unsafe { core::ptr::write_volatile(self.mtimecmp_base as *mut u64, mtime + interval) };
        unsafe { core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64) };
    }

    fn read_ticks(&self) -> u64 {
        unsafe { core::ptr::read_volatile(self.mtime_addr as *const u64) }
    }
}
```

**实例化的好处**：实例字段持有从 `PlatformDesc` 解析出的地址，`read_ticks` 直接读字段——零额外间接（编译器可把字段 load 提到循环外）。所有基址都在 `new(desc)` 构造时一次性注入，构造后实例字段不再变更，热路径无需做任何基址检查或二次设置。

#### 4.5.2 同样模式应用到其他硬件 trait

| Trait | 实例类型 | 来源 |
|-------|---------|------|
| `ClockArch` | `Aarch64ClockArch`（读 `CNTFRQ_EL0` + `CNTPCT_EL0`）、`X86ClockArch`（PIT + LAPIC 计数器） | §3.4.3 + `os/arch/src/{aarch64,x86_64}/clock.rs` |
| `InterruptController` | `Riscv64Plic` / `Aarch64Gicv3` / `X86Apic` | §3.4.3 + `os/plat/src/{riscv64,aarch64,x86_64}/interrupt.rs` |
| `ArchInit` | 各架构 `arch_init()` 函数封装为实例方法 | §3.4.3 + `os/arch/src/{riscv64,aarch64,x86_64}/arch_init.rs` |

模式一致：每个实现 `new(desc: &XxxDesc) -> Self`，地址字段在构造时从子描述符 enum 拷贝到实例字段，热路径方法直接读字段，不重新解析 `PlatformDesc`。

### 4.6 启动时序：T2.5 阶段

```
cstart()
  │
  ├── platform::init_from_kinfo(&kinfo)   ← 相比 Minix3 C 的 acpi_init（在 arch_init 中调用），Rust 前移到时钟/中断之前
  │     └── 解析 DTB/RSDP → 构造 PlatformDescEnum → 写入全局 PLATFORM
  │
  ├── prot_init()                         ← GDT/TSS/stvec 就绪（03 文档）
  │
  ├── init_clock_and_interrupts()         ← 从 platform_desc() 读取硬件参数并实例化各硬件 trait（详见 §4.5）
  │     └── let pd = platform_desc();
  │     └── let mut clock = CurrentClockArch::new(&pd.timer());
  │     └── clock.init_timer(hz);
  │     └── let mut intr = CurrentInterruptController::new(&pd.interrupt_controller());
  │     └── intr.init();
  │     └── let mut arch = CurrentArchInit::new(&pd.arch_misc());
  │     └── arch.init();
  │
  └── ...（后续阶段）
```

顺序约束：

- `platform::init_from_kinfo` **必须**在 `init_clock_and_interrupts` **之前**完成——后者从 `platform_desc()` 读取硬件参数。
- 与 C 行为的兼容性：C 的 `acpi_init` 在 `arch_init` 中调用，**晚于** `init_clock` 和 `intr_init`。这是因为 C 的 `acpi_init` 解析的 ACPI 表**不**用于驱动时钟和中断控制器（那些依赖硬编码）。Rust 重写要"从解析中获取时钟/中断的基址"，所以顺序必须前移。
- 这是与 C 的**有意偏离**，理由是 C 的硬编码本身就是要被替换的。

> 参见 `os/kernel/src/lib.rs:283-293` 调用 `minix_platform::init_from_kinfo(kernel_info)`（位于 `init_protection` 之前），紧接 `init_protection` 和 `init_clock_and_interrupts`。

### 4.7 哪些常量迁入 `PlatformDesc`，哪些保留为架构常量

不是所有硬件参数都通过 `PlatformDesc` 发现——区分原则是 **"固件可发现" vs "架构强制标准"**。

| 类型 | 处理方式 | 例子 |
|------|---------|------|
| **固件可发现**（DTB / ACPI 给出） | 迁入 `PlatformDesc` | PLIC base、LAPIC base、IOAPIC base、COM1 / PL011 / SBI console |
| **架构强制标准**（ISA 规定） | 保留为源码常量 | PIT I/O 端口 `0x40/0x43`、x86 APIC 寄存器访问协议、RISC-V 特权指令编码 |

#### 4.7.1 为什么 PIT I/O 端口保留为常量

PIT I/O 端口 `0x40/0x43` 是 PC/UEFI 兼容机的 ISA 规范常量——任何符合 PC 架构的系统都必须将 8254 PIT 映射到这两个端口。**不存在"另一块板子用不同 PIT 端口"的可能**——ISA 强制规范没有自由度，所以不需要 `PlatformDesc` 抽象。

对比：LAPIC base `0xFEE0_0000` 同样是 PC/UEFI 兼容机的 ISA 规范值，为什么它能迁入 `PlatformDesc`？因为：

- LAPIC base 由 **ACPI MADT** 显式报告（`Local APIC Address` 字段），**协议上**是可发现的。
- 现代服务器平台允许通过配置改变 LAPIC base（虽然 99% 的实现仍是 `0xFEE0_0000`），所以走"理论上可发现"的路径更稳妥。

**判别规则**：如果硬件参数是 **ISA 规范字面常量**（无协议报告机制）→ 保留常量；如果 **固件提供发现协议**（即使协议永远返回同一值）→ 走 `PlatformDesc`。

#### 4.7.2 Early Console 的特殊位置

boot 阶段需要输出诊断信息，因此 early console 仍然保留。变化的是它的**基址来源**——从各架构 early console 模块内的常量，迁移到 `PlatformDesc::early_console()` 返回的 `ConsoleDesc`。

| 架构 | Early Console 来源 | 处理 |
|------|-------------------|------|
| x86-64 | `ConsoleDesc::IsaSerial { port_base }` | 由 ACPI SPCR 或 `QemuVirtDesc` 提供 |
| aarch64 | `ConsoleDesc::MmioSerial { mmio_base }` | 由 DTB 或 `QemuVirtDesc` 提供 |
| riscv64 | `ConsoleDesc::SbiConsole`（SBI 调用，无 MMIO） | `QemuVirtDesc` 提供固定变体 |

#### 4.7.3 bootstrap 路径的时序约束

boot-shim 和极简 test-kernel 在 `PlatformDesc` 初始化之前就需输出（例如 panic 信息）。这些路径仍允许使用架构默认常量（x86 COM1、aarch64 PL011、riscv64 SBI），直到 kernel 的 `init_from_kinfo()` 建立 `PlatformDesc` 后再按描述符重新配置。这是一个**启动时序约束**，不是设计回退。

### 4.8 多核扩展预留

`PlatformDesc` 的子结构为 SMP 预留了多核信息：

- `CpuTopology`：CPU 数量、每个 CPU 的 `CpuInfo`（hw_id、gicr_base、mtimecmp_addr）。
- `InterruptControllerDesc::Gicv3`：`gicr_stride` 用于计算 per-CPU Redistributor 地址。
- `TimerDesc::Clint`：`mtimecmp_stride` 用于计算 per-hart mtimecmp 地址。

**当前阶段**：`QemuVirtDesc` 已按 4 核编码（`nr_cpus = 4`，`cpus[0..4]` 填入各自私有地址），QEMU 测试脚本已统一加 `-smp 4`。内核启动代码仍只使用 BSP（cpu 0）；AP 启动逻辑在 [16-smp.md](16-smp.md) 实现。每个 CPU 通过 `CpuTopology.cpus[cpu_id]` 查询自己的私有信息（GICR base / mtimecmp 地址 / APIC ID）。

### 4.9 `no_std` 约束

- `minix-platform` crate 标注 `#![no_std]`（`os/libs/minix-platform/src/lib.rs:1`）。
- 解析结果存储在 `static` 中（一次性写入，不释放）。
- `PlatformDescEnum` 是枚举（无堆分配），`DeviceTreeDesc`/`AcpiDesc` 的字段是固定大小（`CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组，`MAX_CPUS = 64`，见 `desc.rs:12`）。
- Phase 3 的 FDT 解析使用 `fdt` crate（`#![no_std]` 兼容，纯 Rust 实现）。
- Phase 4 的 ACPI 解析自研最小化（RSDP → XSDT → MADT），不使用 `std` 集合或分配器。

---

## 5. 测试与验证

### 5.1 测试矩阵

| 测试层级 | 方法 | 覆盖什么 |
|---------|------|---------|
| **单元测试** | 构造 `PlatformDescEnum` desc，验证 `Riscv64ClockArch::new(desc)` 等提取的字段值 | trait 实例化正确性 |
| **QemuVirtDesc 测试** | 验证三架构 `QemuVirtDesc` 返回值与原硬编码常量逐一对照 | 兜底路径不回归 |
| **DTB 解析测试** | 嵌入 QEMU `virt` DTB 字节数组，验证 `DeviceTreeDesc::parse` 提取的地址 | FDT 解析正确性 |
| **ACPI 解析测试** | 构造模拟 ACPI 表，验证 `AcpiDesc::parse` 提取的 IOAPIC base | ACPI 解析正确性 |
| **QEMU 集成测试** | 保持现有 `qemu_test_*.sh` 通过 | 端到端不回归 |

> 参见 `os/libs/minix-platform/src/desc.rs` 末尾、`os/libs/minix-platform/src/qemu_virt.rs` 末尾、`os/libs/minix-platform/src/global.rs` 末尾均包含 `#[cfg(test)]` 模块。

---

## 6. 本章术语索引

本章首次出现时已通过行内括号注解解释（§1.0 / §1.1 / §1.3）。本节列出**完整索引**便于回查；每个术语列其行内首次出现位置与一句话定位。

**硬件描述格式**

| 术语 | 全称 | 首次出现位置 | 一句话定位 |
|------|------|------------|----------|
| ACPI | Advanced Configuration and Power Interface | [§1.0 l22](04-platform-discovery.md#L22) | x86 固件用的硬件配置与电源管理标准 |
| DTB | Device Tree Blob（规范名 FDT = Flattened Device Tree）| [§1.0 l21](04-platform-discovery.md#L21) | ARM/RISC-V 嵌入式生态的硬件描述二进制格式 |
| RSDP | Root System Description Pointer | [§1.3 l64](04-platform-discovery.md#L64) | ACPI 表的根入口指针（OS 固件扫描目标）|
| XSDT / RSDT | Extended / Root System Description Table | [§1.3 l64](04-platform-discovery.md#L64) | ACPI 多表索引层（XSDT = 64 位、RSDT = 32 位）|
| MADT | Multiple APIC Description Table | [§1.1 l38](04-platform-discovery.md#L38) | 描述 x86 LAPIC + IOAPIC 中断控制器 |
| SPCR | Serial Port Console Redirection Table | [§1.1 l41](04-platform-discovery.md#L41) | ACPI 子表，描述早期串口控制台基址 |
| DTB（再次出现）| — | [§1.1 l38](04-platform-discovery.md#L38) 表格行 | 见上（"DTB" 行 l21） |

**设备与接口**

| 术语 | 全称 | 首次出现位置 | 一句话定位 |
|------|------|------------|----------|
| MMIO | Memory-Mapped I/O | [§1.0 l16](04-platform-discovery.md#L16) | 设备寄存器映射到物理地址，CPU 用 load/store 访问 |
| QEMU virt | QEMU Generic Virtual Platform | [§1.1 l30](04-platform-discovery.md#L30) | minix-rs 三架构共用的测试目标机器（无真实硬件对应）|
| CLINT | Core Local Interruptor | [§1.1 l30](04-platform-discovery.md#L30) | RISC-V 平台的本地中断控制器 + 时钟 |
| PLIC | Platform-Level Interrupt Controller | [§1.1 l30](04-platform-discovery.md#L30) | RISC-V 平台的外部中断控制器 |
| LAPIC / IOAPIC | Local / I/O Advanced Programmable Interrupt Controller | [§1.1 l32](04-platform-discovery.md#L32) | x86 的本地 + I/O 中断控制器 |
| CNTFRQ_EL0 | Counter Frequency Register (EL0) | [§1.1 l39](04-platform-discovery.md#L39) | ARM64 系统寄存器，存定时器频率 |
| SBI | Supervisor Binary Interface | [§1.3 l66](04-platform-discovery.md#L66) | RISC-V 的 M-mode 固件提供给 S-mode kernel 的系统调用接口 |
| OpenSBI | Open Source SBI Implementation | [§1.3 l66](04-platform-discovery.md#L66) | SBI 的开源参考实现（M-mode 固件） |

**注**：本章未覆盖的设备（如 GICv3 / PIT / HPET / ArmGenericTimer 等）的寄存器级细节属于 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) 范围，本索引不展开。术语"UEFI"作为通用缩写使用，未单独注解（详见 [01-boot-shim-bootstrap.md §3.1](01-boot-shim-bootstrap.md#31-uefi-替代-multiboot架构演进)）。

---

## 7. 参见

- [00-kernel-overview.md](00-kernel-overview.md) §1.5 — 内核执行模型约束（SMP/BKL）
- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — boot-shim 职责边界（Phase 2 将扩展）
- [03-kmain-cstart.md](03-kmain-cstart.md) — cstart 初始化序列（`init_from_kinfo` 的调用点）
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟/中断控制器从 `PlatformDesc` 获取地址
- [16-smp.md](16-smp.md) — 多核扩展（`CpuTopology` 的消费者）
- `minix3/minix/kernel/arch/i386/arch_system.c:246-287` — C 参考源码：`arch_init()` 调用 `acpi_init()`
- `minix3/minix/kernel/arch/i386/acpi.c:310-342` — C 参考源码：`acpi_init()`
- `minix3/minix/kernel/arch/earm/arch_system.c:101-132` — C 参考源码：`arch_init()` 调用 `bsp_init()`
