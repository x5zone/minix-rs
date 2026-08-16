# 04-platform-discovery: 平台硬件发现抽象

> **分类**: 平台抽象 / 硬件发现
> **源码**: `os/libs/minix-boot/src/platform.rs`（`PlatformDesc`/`PlatformDescKind`/`PlatformDescSource`/子描述符 trait）、`os/libs/minix-platform/src/desc.rs`（trait re-export）、`os/libs/minix-platform/src/kind.rs`（`parse_by_kind` 分派）、`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`（品牌 struct + per-arch `QemuVirtDesc`）、`os/libs/minix-platform/src/global.rs`（`PlatformContext`/`init_from_kinfo`）、`os/libs/minix-platform/src/device_tree.rs`、`os/libs/minix-platform/src/acpi.rs`、`os/libs/minix-boot/src/kernel_info.rs:81-102`、`os/boot-shim/src/uefi_helpers.rs:52-98`、`os/boot-shim/src/opensbi_helpers.rs:222-260`、`os/kernel/src/lib.rs:359`
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
2. **与 Minix3 C 语义对齐**：C 版的 `acpi_init()`、`bsp_init()` 都是 **kernel 内部**调用。"kernel 自己理解硬件"是微内核的职责边界。这也是主流 OS 的统一模式：Linux 在 kernel 解析 DTB/ACPI（`of_*` / `acpi_*` 子系统）、seL4 在 kernel 的 `boot.c` 解析、Minix3 C 在 `bsp_init()` 解析——bootloader 一律只传物理指针，硬件描述由 kernel 自己读。粒度差异：Linux 把每个 device tree 节点建模成 `struct device`（支撑用户态 device model），而微内核没有用户态 device model、所有硬件 init 都在 kernel 内，一个全局 `PlatformDesc` 即足够——粗粒度是与微内核结构匹配的选择。
3. **`KernelInfo` 保持精简**：方案 A 要求 `KernelInfo` 扩展 GICD/GICR/PLIC/CLINT 等十几个字段。方案 C 只需加 **1 个字段**（`platform_sources: &'static [PlatformDescSource]`）——一个有序切片，承载 `(PlatformDescKind, PhysBytes)` 这种纯数据对。切片天然支持多源共存（DTB + RSDP 共存的服务器场景），上层 KernelInfo 仍是 1 字段，没有膨胀。详见 [§3.7](#37-kernelinfo-扩展字段设计)。
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

> 命名先决：为什么叫 `PlatformDesc`，而不是 `MachineDesc` / `BoardDesc` / `HardwareTopology`？"Machine"与 QEMU machine 概念混淆（两者语义不同）；"Board"暗示 PCB 级别，而平台描述涵盖 SoC/PC/嵌入式全谱系；"HardwareTopology"过于聚焦拓扑，遗漏中断控制器/定时器信息。"Platform" 是 OS 语境最通用的词，且与 Linux "platform device" 概念对齐。

三个来源（DTB/ACPI/QEMU 兜底）需要被上层代码**统一消费**，因此定义公共接口 `PlatformDesc` trait：

- **测试隔离**：单元测试可构造 `MockPlatformDesc` 返回已知值，不必挂真实 DTB/ACPI 解析（参见 Ch5 测试要点）。
- **扩展性**：新增来源（未来 SMBIOS、用户自定义 .toml 配置等）只需新增一个 `impl PlatformDesc`，上层代码与硬件描述来源解耦。
- **配合 enum 做静态分发**：trait 是"接口"，实际存储是 `PlatformDescEnum`（§3.3 的 enum），三选一编译期可知，零 vtable 开销。`platform_desc()` 返回 `&'static dyn PlatformDesc` 仅在 API 边界做一次类型擦除，调用频率低（init 阶段），不构成热路径。

> 关于"`&dyn` 会引入 vtable 开销"的顾虑：
>
> vtable 是一张编译期生成、放在 `.rodata` 里的函数指针表——`&dyn PlatformDesc` 是一个胖指针（数据指针 + vtable 指针），每次方法调用多 2~3 条 load 指令。**它不依赖 std、不依赖堆、不依赖运行时初始化**，boot 阶段完全可以工作。
>
> 但这个开销在本设计中**几乎可以忽略**：实际存储是 `PlatformDescEnum`，编译器对每个 `match desc.interrupt_controller() { ... }` 都能静态内联到具体变体的方法体；只有从 enum 切到 trait object 那一刻才有一次虚表查找，而这一步发生在 boot 时 init 阶段（`clock.rs:131` 等几处），不在时钟中断等热路径上。热路径上的 `read_tsc()` 拿到 enum 后就走具体类型的寄存器操作，无 vtable 查找。

参见 `os/libs/minix-boot/src/platform.rs:247-260` 的 `PlatformDesc` trait 定义（由 `os/libs/minix-platform/src/desc.rs` re-export）。

### 3.3 子描述符为什么用 trait + `Any` downcast（TODO-01-2 修复后）

> **状态**：✅ 修复完成（TODO-01-2，2026-07-16）。本节原论述"子描述符为什么用 enum 而不是 trait"，对应 `InterruptControllerDesc`/`TimerDesc`/`ConsoleDesc` 三个 enum，变体名硬编码硬件品牌（`Apic`/`Gicv3`/`Plic`/`Pit`/`ArmGenericTimer`/`Clint`/`IsaSerial`/`MmioSerial`/`SbiConsole`）。修复后这三个 enum 全部改为带 `Any` 的 trait；品牌名 struct 退居 `minix-platform/src/arch/<arch>.rs` 子模块，对上层不可见。

修复后的子描述符是 **trait + `Any` downcast**，而不是 enum。核心原则：**"描述机制，而不是描述硬件。关注它们能做什么，而不是它们叫什么。"** 上层只看到 trait 暴露的通用方法（`nr_irqs()`、`frequency()` 等）；arch 层通过 `Any` downcast 拿到具体 struct 读取品牌相关字段。

```rust
// os/libs/minix-boot/src/platform.rs
//
// 子描述符 trait 定义。注意：品牌名（Apic/Gicv3/Plic/...）在这里完全不可见。
// 具体 struct（ApicDesc/Gicv3Desc/PlicDesc/...）定义在 minix-platform/src/arch/ 下。

pub trait InterruptControllerDesc: Send + Sync + fmt::Debug + Any {
    fn nr_irqs(&self) -> u32;
    fn as_any(&self) -> &dyn Any;  // 必需方法，无默认实现
}

pub trait TimerDesc: Send + Sync + fmt::Debug + Any {
    fn frequency(&self) -> u64;
    fn as_any(&self) -> &dyn Any;
}

pub trait ConsoleDesc: Send + Sync + fmt::Debug + Any {
    fn as_any(&self) -> &dyn Any;
}
```

**关键设计要点**：

1. **品牌名隐藏在 `arch/` 子模块**：`ApicDesc`/`Gicv3Desc`/`PlicDesc`/`PitDesc`/`ArmGenericTimerDesc`/`ClintDesc`/`IsaSerialDesc`/`MmioSerialDesc`/`Riscv64ConsoleDesc` 都定义在 `minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`，通过 `#[cfg(target_arch)]` 选择编译，对 `minix-boot` 与上层完全不可见。

2. **`as_any()` 是必需方法，不提供默认实现**：理论上 `fn as_any(&self) -> &dyn Any { self }` 看起来可以作默认实现，但 `&Self` → `&dyn Any` 要求 `Self: Sized`，而 trait 内部的 `self: &Self` 并不保证 `Self: Sized`（trait object 可以 `?Sized`）。因此把 `as_any()` 声明为必需方法，由各 impl 块显式写 `fn as_any(&self) -> &dyn Any { self }`——此时 `Self` 已是具体 sized 类型，转换合法。

3. **上层消费通用方法，arch 层 downcast 品牌字段**：
   - 上层 `nr_irqs()` 通过 trait vtable 调用，零品牌信息。
   - arch 层消费代码（如 `X86_64InterruptController::new`）拿到 `&dyn InterruptControllerDesc` 后，调用 `as_any().downcast_ref::<ApicDesc>()` 拿到具体 struct 读取 `lapic_base`/`ioapic_base` 等品牌字段。

   ```rust
   // 上层调用通用方法（无需知道品牌）：
   let nr = desc.interrupt_controller().nr_irqs();

   // arch 层 downcast 拿到品牌字段（仅在 init 时调用一次）：
   impl InterruptController for X86_64InterruptController {
       fn new(desc: &dyn InterruptControllerDesc) -> Self {
           let apic = desc.as_any()
               .downcast_ref::<ApicDesc>()
               .expect("expected ApicDesc");
           Self { lapic_base: apic.lapic_base, ioapic_base: apic.ioapic_base, ... }
       }
   }
   ```

4. **零 vtable 开销在 arch 热路径**：downcast 仅在 `new()` 构造时发生一次（init 阶段）。构造完成后实例字段直接持有 `lapic_base`/`plic_base` 等值，热路径（时钟中断、IRQ mask/unmask）只读字段，无 vtable 查找——与 §3.4.2 实例化模式完全一致。

5. **开放-封闭原则（Open-Closed）**：新增中断控制器（如 x2APIC）只需：
   - 在 `minix-platform/src/arch/x86_64.rs` 定义 `X2ApicDesc` struct + `impl InterruptControllerDesc for X2ApicDesc`；
   - 在 parser（`AcpiDesc`/`DeviceTreeDesc`）里改用新 struct。
   
   **不需要**修改 `minix-boot`、上层 trait 定义、或 `PlatformDescEnum`——上层消费代码无感知。

6. **`PlatformDescEnum` 保留**：dispatch enum 仍然是 `DeviceTree(DeviceTreeDesc)`/`Acpi(AcpiDesc)`/`QemuVirt(QemuVirtDesc)` 三选一（变体按 arch cfg-gate），用于编译期分发 + `no_std` 下避免 `Box<dyn PlatformDesc>` 的分配器依赖。子描述符 trait 化不影响 `PlatformDescEnum` 的角色——它分发的是"解析来源"而非"硬件品牌"。

参见：
- `os/libs/minix-boot/src/platform.rs:281-317` 定义 `InterruptControllerDesc`/`TimerDesc`/`ConsoleDesc` 三个 trait（含 `as_any()` 必需方法）。
- `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 定义各品牌 struct 与 trait impl。
- `os/libs/minix-platform/src/desc.rs:1-29` 解释"为何 enum 改 trait"的设计说明。

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
    fn new(desc: &dyn TimerDesc) -> Self;  // ← 实例从 desc 拿到硬件参数
    fn init_timer(&self, hz: u32);         // ← 现在有 &self
    fn read_ticks(&self) -> u64;           // ← 现在有 &self
}

pub struct Riscv64ClockArch {
    mtime_addr: usize,                       // ← 解析出的地址住在实例字段
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &dyn TimerDesc) -> Self {
        // arch 层 downcast 到具体品牌 struct（仅在 init 时一次）：
        let clint = desc.as_any()
            .downcast_ref::<ClintDesc>()
            .expect("expected ClintDesc");
        Self {
            mtime_addr: clint.mtime_addr,    // ← desc → 实例字段的一次性搬迁
        }
    }
    fn read_ticks(&self) -> u64 {
        unsafe { (self.mtime_addr as *const u64).read_volatile() }
    }
}
```

调用方在 init 阶段一次性建好实例，热路径（如时钟中断）只访问 `self.mtime_addr`——经编译器优化后等价于直接 `mov + load`，零间接开销。

#### 3.4.3 同样模式应用到其他硬件 trait

`InterruptController` 和 `ArchInit` 采用同样的实例化模式，区别只是构造参数（全部从 `&dyn` 子描述符 downcast 一次）：

| trait | 实例字段 | 构造参数 |
|-------|---------|---------|
| `ClockArch`（clock.rs:86） | `mtime_addr` / `lapic_base` / 等 | `&dyn TimerDesc` |
| `InterruptController`（interrupt.rs） | `gicd_base` / `plic_base` / 等 | `&dyn InterruptControllerDesc` |
| `ArchInit`（arch_init.rs:47-61） | 架构相关 misc 参数 | `&ArchMiscDesc` |

参见 `os/arch/src/arch/clock.rs:86` 定义带 `&self` 的 `ClockArch` trait；`os/plat/src/interrupt.rs:129` 定义 `InterruptController` trait；`os/arch/src/arch/arch_init.rs:47-61` 定义 `ArchInit` trait。

### 3.5 全局存储为什么用 `AssumeSyncCell` 而非 `Mutex`/`static mut`

| 方案 | 否决理由 |
|------|---------|
| `static mut` | 读写必须包在 `unsafe` 块里，且 `unsafe fn` 内部访问 mutable static 也需独立 `unsafe` 块（Rust 2024 edition 收紧）——把"单写多读"的安全契约藏在散落的 unsafe 里，文档难以集中维护 |
| `Mutex`/`spin::Mutex` | boot 阶段单线程，运行时只读——加锁是多余开销 |
| `OnceLock` | `no_std` 下不可用（需要分配器或 `std`） |

`AssumeSyncCell<T>` 是项目内统一的"UnsafeCell + 手动 `Sync`"原语，定义在 `os/libs/minix-types/src/types/cell.rs`，VM server、heap arena、vmproc 表等都已在用。它的安全契约：**调用方保证单线程独占访问**。`platform_desc` 的"boot 阶段单线程写入一次，之后所有 CPU 只读访问"恰好满足这一契约。

> **参见**: `os/libs/minix-types/src/types/cell.rs:51` 定义 `AssumeSyncCell`；`os/libs/minix-platform/src/global.rs` 中以 `static PLATFORM: AssumeSyncCell<Option<PlatformContext>>` 形式持有全局描述符。

> **TODO(P1, SMP 阶段处理)**：上例"boot 阶段单线程写入一次，之后所有 CPU 只读访问"的安全契约在 SMP（`[16-smp.md](16-smp.md)`）就绪后**失效**——`AP_STARTUP` 路径下应用处理器的 BSP 同步握手可能并发访问 `PLATFORM`。届时此 `AssumeSyncCell` 必须替换为 `Mutex`/`AtomicXxx` 或拆分为 per-CPU 数据。当前单 BSP boot 路径不触发该风险，故不在本文档范围内处理。
> **参见文件**: `os/libs/minix-types/src/types/cell.rs:51`（`AssumeSyncCell` 定义）；`os/libs/minix-platform/src/global.rs:38-51`（`PLATFORM` 静态）。
> **跟踪**: 由 [16-smp.md](16-smp.md)（SMP 阶段文档）继承此 TODO 并在 AP bring-up 之前完成替换。

### 3.6 `QemuVirtDesc` 兜底实现

`QemuVirtDesc` 是 `PlatformDesc` 的具体实现，返回 QEMU `virt` 机器各架构固定的硬件参数（PLIC/GICv3/APIC 基地址、CLINT/ArmGenericTimer/PIT 参数、串口地址）。它是 `platform::init_from_kinfo` 在 `kinfo.platform_sources` 为空、或所有 source 解析失败时的兜底路径——硬编码值保证这条路径永远可用，使得即便 boot-shim 异常，仍能输出诊断信息而不至于连 panic 信息都看不到。

> **关于 `#[cfg(target_arch)]`**：TODO-01-2 修复后，`QemuVirtDesc` 已拆分为三个 per-arch 文件（`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`），每个文件持有该架构的具体子描述符字段（如 riscv64 版本持 `PlicDesc` + `ClintDesc` + `Riscv64ConsoleDesc`），方法体内**无 `#[cfg(target_arch)]`**。`#[cfg]` 只用在文件级 mod 选择（`arch/mod.rs`），不再散布到方法体内。真实硬件路径走 `DeviceTreeDesc`/`AcpiDesc`，同样无方法级 `#[cfg]`。详见 [00-kernel-overview.md](00-kernel-overview.md) §1.5。
>
> 参见 `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 中三个 `QemuVirtDesc` 实现与各架构参数。
>
> **覆盖证据（2026-07-16 复核）**：经 grep 验证，当前 DTB/ACPI parser 主路径**已存在单测覆盖**——
> - `os/libs/minix-platform/src/device_tree.rs:490` `fn test_parse_riscv64_qemu_virt_dtb` 用真实 DTB 二进制（`os/libs/minix-platform/tests/data/qemu_virt_riscv.dtb`）做端到端解析验证
> - `os/libs/minix-platform/src/acpi.rs:551` `fn test_parse_synthetic_acpi` 用合成的 RSDP→XSDT→MADT 字节链验证完整 ACPI 解析
> - `os/arch/tests/qemu_test_x86_64.sh` 等 QEMU 集成测试通过 GDB checkpoint 验证 `init_clock_and_interrupts`，**必然**触发 `platform::init_from_kinfo` → parser 主路径（无 `QemuVirtDesc` 介入，因为 `platform_sources` 非空，由 boot-shim 端 find 来源填入）
>
> `os/kernel/tests/boot_integration.rs:34`、`os/kernel/src/lib.rs:2266/2330/2412/2507/2538/2779/2803/2827`、`os/libs/minix-boot/src/kernel_info.rs:303` 共 10 处 `platform_sources: &[]` 均为 `#[cfg(test)]` 单元测试 fixture（另有 boot.rs:455 与 boot-shim 两处 helpers 测试传参，详见 §11.2 表格），**不**针对 parser 主路径——单元测试绕过固件读取是正常设计，不构成 "test what you fly" 违反。`boot-shim/src/{uefi,opensbi}_helpers.rs:228/432` 是**生产** `KernelInfo` 构造函数的 `platform_sources` 参数（boot 期由固件发现来源后填充，见 `uefi_helpers.rs:222` / `opensbi_helpers.rs:222` 的发现逻辑）。原 §3.6 + §11 描述已纠正，详见 §11 重写。

> **状态**：✅ 修复完成（TODO-01-2，2026-07-16）。原 TODO 指出 `QemuVirtDesc` 的每个方法都遍布 `#[cfg(target_arch)]` 条件编译，代码重复且难以维护。修复方案是把 `QemuVirtDesc` 拆成三个 per-arch 文件（`arch/x86_64.rs`、`arch/aarch64.rs`、`arch/riscv64.rs`），每个文件持有该架构的具体子描述符字段（`ApicDesc`+`PitDesc`+`IsaSerialDesc` / `Gicv3Desc`+`ArmGenericTimerDesc`+`MmioSerialDesc` / `PlicDesc`+`ClintDesc`+`Riscv64ConsoleDesc`），方法体内直接 `&self.ic` 等返回具体 struct 引用（自动 trait object 化为 `&dyn InterruptControllerDesc`），**消除所有方法级 `#[cfg]` 分支**。`#[cfg(target_arch)]` 现在只用在 `arch/mod.rs` 的 mod 选择语句上，与 `PlatformDescEnum` 的变体 cfg-gate 一致。

> 
> ## 11. `QemuVirtDesc` 与测试覆盖：原则保留、事实纠正（2026-07-16 重写）
>
> > **本节重写原因**（2026-07-16 复核）：原 §11（来自 todo.md §11）描述的"QEMU 测试路径刻意走 QemuVirtDesc 而非 DTB/ACPI parser"**事实链有误**。当前 DTB/ACPI parser 已有充分的单测 + 集成测试覆盖（见 §11.1 的 grep 证据）。原描述把单元测试 fixture 的 `platform_sources: &[]`（这是正常设计——单元测试不依赖固件读取）误判为"测试刻意走 QemuVirtDesc 规避 parser"，导致 §11.1 / §11.2 的论据与代码现状不符（例如 §11.2 引用的 `os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs` 文件**已不存在**——已被移除，对应 `os/arch/src/arch/mod.rs:26` 注释 "proc_arch was removed"）。下述 §11.1-§11.3 是事实纠正后的版本，§11.4-§11.6 保留 "test what you fly" 原则与 QemuVirtDesc 角色定位作为设计警示。
>
> ### 11.1 parser 主路径覆盖证据（grep 验证）
>
> **2026-07-16 复核 grep 命令**：
>
> ```bash
> rg "fn test_.*dtb|fn test_.*acpi|fn test_.*parse" os/libs/minix-platform/ --type rust -n
> ```
>
> **结果**：
>
> | 测试函数 | 文件:行号 | 覆盖目标 |
> |---------|---------|---------|
> | `test_dt_parse_error_variants` | `os/libs/minix-platform/src/device_tree.rs:452` | DTB parser 错误路径 |
> | `test_from_bytes_bad_magic` | `os/libs/minix-platform/src/device_tree.rs:474` | DTB parser magic 校验 |
> | `test_parse_riscv64_qemu_virt_dtb` | `os/libs/minix-platform/src/device_tree.rs:490` | DTB parser **端到端**（用真实 `qemu_virt_riscv.dtb`） |
> | `test_parse_unsupported_arch_returns_error` | `os/libs/minix-platform/src/device_tree.rs:524` | DTB parser arch 拒绝路径 |
> | `test_acpi_parse_error_variants` | `os/libs/minix-platform/src/acpi.rs:504` | ACPI parser 错误路径 |
> | `test_acpi_desc_from_parsed` | `os/libs/minix-platform/src/acpi.rs:519` | ACPI descriptor 构造 |
> | `test_parse_synthetic_acpi` | `os/libs/minix-platform/src/acpi.rs:551` | ACPI parser **端到端**（合成 RSDP→XSDT→MADT） |
> | `test_parse_by_kind_unknown_returns_error` | `os/libs/minix-platform/src/kind.rs:86` | parser 调度路径 |
> | `test_u32_le_from_slice` | `os/libs/minix-platform/src/acpi.rs:543` | parser 内部 helper |
>
> 集成测试（QEMU 端到端）：
>
> | 测试脚本 | 覆盖路径 |
> |---------|---------|
> | `os/arch/tests/qemu_test_x86_64.sh` | x86-64 init_clock_and_interrupts GDB checkpoint → 必然触发 `platform::init_from_kinfo` → ACPI parser |
> | `os/arch/tests/qemu_test_aarch64.sh` | aarch64 init_clock_and_interrupts → 触发 DTB parser |
> | `os/arch/tests/qemu_test_riscv64.sh` | riscv64 init_clock_and_interrupts → 触发 DTB parser |
> | `os/qemu-tests/run_qemu.sh` | 通用 QEMU runner，被上述脚本调用 |
>
> **结论**："DTB/ACPI parser 主路径 0% 覆盖"为**事实错误**——单测 + 集成测试双重覆盖，端到端走完 boot-shim → kernel → parser → ClockArch 链。
>
> ### 11.2 `platform_sources: &[]` 的真实分布
>
> 复核 grep 命令：
>
> ```bash
> rg "platform_sources:\s*&\[\]" os/ --type rust -n
> ```
>
> 单元/集成测试 fixture 12 处全部为 `#[cfg(test)]` 模块的 fixture（另有 `os/qemu-tests/test-kernels/` 下 17 处极简 test-kernel 直接传 `&[]`——见 §11.2 后注）：
>
> | 文件 | 行号 | 上下文（均为测试代码） |
> |------|------|----------------------|
> | `os/boot-shim/src/uefi_helpers.rs` | 333 | `#[cfg(test)] mod tests`（`&[], // platform_sources (empty = no source)`） |
> | `os/boot-shim/src/opensbi_helpers.rs` | 573, 670-672 | `#[cfg(test)] mod tests`（同上 + `assert!(info.platform_sources.is_empty())`） |
> | `os/arch/src/arch/boot.rs` | 455 | `#[cfg(test)] mod tests`（mock `load_vm_elf`） |
> | `os/kernel/tests/boot_integration.rs` | 34 | 集成测试 fixture（paging mock） |
> | `os/kernel/src/lib.rs` | 2266, 2330, 2412, 2507, 2538, 2779, 2803, 2827 | `#[cfg(test)] mod tests`（paging 测试） |
> | `os/libs/minix-boot/src/kernel_info.rs` | 303 | `#[cfg(test)] mod tests`（KernelInfo 构造 fixture） |
>
> **§11.2 后注（2026-08-14 复核）**：`os/qemu-tests/test-kernels/kernel/bootstrap/` 下 17 个极简 test-kernel（hello-boot、test-paging-enable、test-protection、test-kernel-map、test-higher-half、test-proc-init 等各架构变体）在 `main.rs` 直接构造 `KernelInfo` 并传 `platform_sources: &[]`——它们不经过 boot-shim 固件发现阶段，是 §4.7.3 "bootstrap 路径的时序约束"的实例（极简 test-kernel 在 `PlatformDesc` 初始化前工作，走 `QemuVirtDesc` 兜底或完全不依赖平台描述），与单元测试 fixture 语义一致，不构成 "test what you fly" 违反。
>
> **这些 fixture 的语义**：单元/集成测试通过构造人造 `KernelInfo` 直接喂给 paging/ELF/process 模块，**绕过** boot-shim 固件读取阶段。这与 "test what you fly" 原则不冲突——被测单元（paging 映射/ELF 加载/process 初始化）**不依赖**平台发现，所以不需要喂真实 DTB/ACPI。
>
> **§11.2（重写前）引用的错误路径**：`os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs:350/252/279` 已删除——参见 `os/arch/src/arch/mod.rs:20` 注释 "proc_arch was removed"。故 §11.2 表格中的三行**整行失效**，应在重写中删除。
>
> ### 11.3 `QemuVirtDesc` 角色定位（保留 §3.6 + 原 §11.3 的设计原则）
>
> **保留**：兜底用途合法，不删除。`QemuVirtDesc` 的设计意图是在 boot-shim 异常时**仍能输出诊断信息**——panic 前还能有最后一帧日志，而不是乱码。
>
> **场景与触发判定**：
>
> | 场景 | 是否触发 `QemuVirtDesc` | 触发条件 |
> |------|----------------------|--------|
> | 生产路径（真实硬件） | ❌ | DTB/ACPI parser 必拿到，参数来自固件 |
> | QEMU 集成测试 | ❌ | QEMU `virt` 提供 DTB/ACPI，parser 走真实路径 |
> | boot-shim 完全失败 | ✅ | boot-shim 在某些 firmware 配置下确实找不到 DTB/RSDP；保留作为最后诊断通道 |
> | dev 构建下 parser 失败 | ✅ + warn | dev 容忍，但保留诊断通道；release build panic |
>
> **消除误解**：单元测试 fixture 的 `&[]` ≠ "规避 parser 主路径"——这两件事在概念上互不相关，前者是测试设计选择（解耦被测单元），后者要求 parser 在生产路径被覆盖。当前状态：两者都正确，无须"修复"。
>
> ### 11.4 "test what you fly" 原则保留（避免未来回归）
>
> > 本节是**设计警示**，不是当前缺陷。
>
> **原则**：当新增一个测试（特别是集成测试）涉及 platform 路径时，**优先**让 boot-shim 端 `find_platform_sources()` 真实工作，而非手工传 `&[]`。
>
> **触发条件**（未来 review 的判定准则）：
>
> | 信号 | 应避免？ |
> |------|--------|
> | 新增 `KernelInfo` fixture 在测试路径显式传 `&[]` **且**被测代码需要 `PlatformDesc` 参数 | ⚠️ 应考虑用 mock `PlatformDesc` 而非空切片，以保留 "走 parser" 的能力 |
> | 新增 `#[cfg(test)]` fixture 用真实 DTB 字节而非 `&[]` | ✅ 这才是 "test what you fly" 的正确路径 |
> | QEMU 集成测试添加 GDB checkpoint 未走到 `platform::init_from_kinfo` | ⚠️ 检查是否被 `QemuVirtDesc` 兜底吃掉 |
>
> ### 11.5 与其他章节的关系
>
> - **§3.6** 的"覆盖证据"标注同步**已修复**：见 §3.6 现在的"覆盖证据（2026-07-16 复核）"段，不再声称"已知缺陷"。
> - **TODO 链** [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §6 "PlatformDescSource 抽象泄漏" 与本节无依赖——前者已修复，本节为独立事实纠正。
> - **§9.3**（`os/plat` 拆分未完成）仍待处理，与本节无依赖。
> - **本重写的 reverify 触发条件**：未来若 parser 测试覆盖率下降（如 `os/libs/minix-platform/tests/data/qemu_virt_riscv.dtb` 文件删除，或 `test_parse_*` 函数被移除），应自动触发本节 §11.1 重新编写。
> - **本节的元注释风险提醒**：原 §11 章节本身就是元注释（描述"已知缺陷"但 grep 证据全无），现重写为事实陈述。如读者发现 §11.1 grep 命令输出与实际不一致，请按本节事实纠正段重新复核。

### 3.7 `KernelInfo` 扩展字段设计

boot-shim 通过 `KernelInfo` 向 kernel 传递 DTB/RSDP 的**原始物理指针**（不解析）。修复后字段为一个 **有序切片** `&'static [PlatformDescSource]`，每个 `PlatformDescSource` 是 `(PlatformDescKind, PhysBytes)` 的纯数据对：

- `PlatformDescKind(u32)` 是**不透明标签**：上层看到的只是一个 `u32`，不知道它对应 DTB 还是 RSDP。常量 `DTB`/`RSDP` 定义在 `minix-boot::platform`（handoff 层），由 `minix-platform::kind` re-export 并提供 `parse_by_kind()` 分派函数。
- `PlatformDescSource` 是**纯数据**（`Copy` + 不含函数指针），可以安全地跨二进制传递——即便 TODO-02-3 把 boot-shim 与 kernel 拆成独立 ELF，boot-shim 内存被回收后也不留悬挂指针。
- **多源并存支持**：切片天然支持多个 source。ARM64 服务器（SBBR）场景下，boot-shim 可同时传 `[dtb_source, rsdp_source]`，kernel 按 boot-shim 的优先顺序尝试解析，取第一个成功的——与 Linux `acpi=on/off/force` 模型一致。

> **状态**：✅ 修复完成（TODO-01-2，2026-07-16）。原设计的 `PlatformDescriptorPtr` 是 sum type（要么 DTB、要么 RSDP），无法同时持有两者，且 `Dtb`/`Rsdp` 变体在 `KernelInfo` 公共 API 表面**显式列出固件描述符类型**，与 §3.1 "上层代码完全屏蔽设备差异" 的设计哲学矛盾。修复方案：把 enum 改为不透明 `PlatformDescKind(u32)` + `PlatformDescSource` 纯数据对，并把单值 `Option<PlatformDescriptorPtr>` 改为有序切片 `&'static [PlatformDescSource]`，同时解决"品牌名暴露"与"无法多源并存"两个问题。原 §3.7 提到的多 AI bagging 与 TODO#1 合并评审路径已通过此修复落地，不再需要进一步评估。

> 参见 `os/libs/minix-boot/src/kernel_info.rs:101` 定义 `platform_sources: &'static [PlatformDescSource]`；`os/libs/minix-boot/src/platform.rs:54-150` 定义 `PlatformDescKind`/`PlatformDescSource`/`DTB`/`RSDP` 常量。

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
// os/libs/minix-boot/src/platform.rs:247-260

/// 平台硬件描述的统一抽象。
///
/// 一个 `PlatformDesc` 实例回答："我跑在什么硬件上？"。
/// 上层（ClockArch / InterruptController / ArchInit）只读这个抽象，
/// 不接触 FDT/ACPI 原始字节，也不接触硬件品牌名。
///
/// 实现必须 `Send + Sync`（BKL 释放窗口内可被其他 CPU 访问）。
///
/// 子描述符返回 `&dyn` 引用——子描述符本身是 trait（见 §4.1.2），
/// 品牌名 struct 隐藏在 `minix-platform/src/arch/` 子模块。
pub trait PlatformDesc: Send + Sync + core::fmt::Debug {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;
    fn timer(&self) -> &dyn TimerDesc;
    fn early_console(&self) -> Option<&dyn ConsoleDesc>;
    fn cpu_topology(&self) -> CpuTopology;
    fn arch_misc(&self) -> ArchMiscDesc;
    fn source(&self) -> PlatformSource;
}
```

#### 4.1.2 子描述符 trait 定义与三架构映射（TODO-01-2 修复后）

> **状态**：✅ 修复完成（TODO-01-2，2026-07-16）。原 enum 变体直接命名为 `Apic`/`Gicv3`/`Plic`/`Pit`/`Clint`/`ArmGenericTimer`，**变体名硬编码硬件品牌**，字段名（`gicr_stride`/`mtimecmp_stride` 等）也直接暴露硬件寄存器布局。修复后子描述符全部改为带 `Any` 的 trait（见 §3.3 完整论述）；品牌名 struct 退居 `minix-platform/src/arch/<arch>.rs`。

```rust
// os/libs/minix-boot/src/platform.rs:281-317
//
// 子描述符 trait 定义。品牌名（Apic/Gicv3/Plic/...）在这里完全不可见。
// 具体 struct 定义在 minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs。

pub trait InterruptControllerDesc: Send + Sync + fmt::Debug + Any {
    fn nr_irqs(&self) -> u32;
    fn as_any(&self) -> &dyn Any;  // 必需方法，无默认实现
}

pub trait TimerDesc: Send + Sync + fmt::Debug + Any {
    fn frequency(&self) -> u64;
    fn as_any(&self) -> &dyn Any;
}

pub trait ConsoleDesc: Send + Sync + fmt::Debug + Any {
    fn as_any(&self) -> &dyn Any;
}
```

各架构具体 struct（实现上述 trait，仅在对应 arch 文件可见）：

```rust
// os/libs/minix-platform/src/arch/x86_64.rs
pub struct ApicDesc { pub lapic_base: usize, pub ioapic_base: usize, pub nr_irqs: u32 }
pub struct PitDesc { pub pit_base_freq: u32, pub lapic_base: usize }
pub struct IsaSerialDesc { pub port_base: u16 }

// os/libs/minix-platform/src/arch/aarch64.rs
pub struct Gicv3Desc { pub gicd_base: usize, pub gicr_base: usize, pub gicr_stride: usize, pub nr_irqs: u32 }
pub struct ArmGenericTimerDesc;  // 频率从 CNTFRQ_EL0 运行时读取
pub struct MmioSerialDesc { pub mmio_base: usize }

// os/libs/minix-platform/src/arch/riscv64.rs
pub struct PlicDesc { pub plic_base: usize, pub nr_irqs: u32, pub context: u32 }
pub struct ClintDesc { pub mtime_addr: usize, pub mtimecmp_base: usize, pub mtimecmp_stride: usize, pub freq: u64 }
pub struct Riscv64ConsoleDesc { /* SBI ecall 或 MMIO UART，详见源码 */ }
```

> **为什么 ARM64 Generic Timer 没有频率字段**：ARM 架构约定固件（UEFI/ATF）在启动时将定时器频率写入 `CNTFRQ_EL0` 系统寄存器。kernel 直接 `mrs CNTFRQ_EL0` 读取，不需要从 DTB 解析。`ArmGenericTimerDesc::frequency()` 返回 `0` 作为"运行时读寄存器"的哨兵值。这是架构规范，不是设计遗漏。

三架构映射（消费方拿到 `&dyn` 后通过 `as_any().downcast_ref::<ConcreteDesc>()` 拿到品牌字段）：

| 架构 | `interrupt_controller()` 返回的 `&dyn` | `timer()` 返回的 `&dyn` |
|------|--------------------------------|----------------|
| x86-64 | `&ApicDesc { lapic_base, ioapic_base, .. }` | `&PitDesc { pit_base_freq, lapic_base }` |
| aarch64 | `&Gicv3Desc { gicd_base, gicr_base, .. }` | `&ArmGenericTimerDesc` |
| riscv64 | `&PlicDesc { plic_base, .. }` | `&ClintDesc { mtime_addr, mtimecmp_base, freq, .. }` |

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

> 参见 `os/libs/minix-boot/src/platform.rs:155-163` 定义 `PlatformSource` enum；`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`（行 146/140/179）+ `device_tree.rs:403` + `acpi.rs:247` 五处 `source()` 实现各返回自身对应变体。

### 4.2 `PlatformDesc` 的三种实现

> **设计决策**：§3.1 选定"方案 C——boot-shim 定位原始指针，kernel 解析"。本节给出三种具体实现，对应原始指针的两种来源（DTB / ACPI）+ 一种兜底（QemuVirt）。

#### 4.2.1 `QemuVirtDesc`：硬编码兜底（per-arch 文件）

TODO-01-2 修复后，`QemuVirtDesc` 拆分为三个 per-arch 文件（`arch/x86_64.rs`、`arch/aarch64.rs`、`arch/riscv64.rs`）。每个文件持有该架构的具体子描述符字段，方法体内**无 `#[cfg(target_arch)]`**——`#[cfg]` 只用在 `arch/mod.rs` 的 mod 选择上。

以 `riscv64` 为例（其他架构结构一致，仅 sub-descriptor 类型与硬编码值不同）：

```rust
// os/libs/minix-platform/src/arch/riscv64.rs
//
// 注意：本文件只在 target_arch = "riscv64" 时编译（由 arch/mod.rs 的 cfg 选择）。
// 因此方法体内不需要再写 #[cfg(target_arch)]。

use minix_boot::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformSource, TimerDesc,
};

// 品牌 struct 在本文件定义（对上层 minix-boot 不可见）。
pub struct PlicDesc { pub plic_base: usize, pub nr_irqs: u32, pub context: u32 }
pub struct ClintDesc {
    pub mtime_addr: usize, pub mtimecmp_base: usize,
    pub mtimecmp_stride: usize, pub freq: u64,
}
pub struct Riscv64ConsoleDesc { /* SBI ecall 或 MMIO UART，详见源码 */ }

// 各品牌 struct 实现 trait（含 as_any() 必需方法）。
impl InterruptControllerDesc for PlicDesc {
    fn nr_irqs(&self) -> u32 { self.nr_irqs }
    fn as_any(&self) -> &dyn core::any::Any { self }
}
impl TimerDesc for ClintDesc {
    fn frequency(&self) -> u64 { self.freq }
    fn as_any(&self) -> &dyn core::any::Any { self }
}
impl ConsoleDesc for Riscv64ConsoleDesc {
    fn as_any(&self) -> &dyn core::any::Any { self }
}

// QEMU `virt` riscv64 兜底描述符——直接持有具体 sub-descriptor struct。
#[derive(Debug)]
pub struct QemuVirtDesc {
    ic: PlicDesc,
    timer: ClintDesc,
    console: Riscv64ConsoleDesc,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl QemuVirtDesc {
    pub const fn new() -> Self {
        Self {
            ic: PlicDesc { plic_base: 0x0C00_0000, nr_irqs: 64, context: 1 },
            timer: ClintDesc {
                mtime_addr: 0x200_BFF8, mtimecmp_base: 0x200_4000,
                mtimecmp_stride: 8, freq: 10_000_000,
            },
            console: Riscv64ConsoleDesc::sbi(),
            cpu_topology: CpuTopology {
                nr_cpus: 4, bsp_id: 0,
                cpus: [CpuInfo::zero(); MAX_CPUS],  // 用 const 构造器
            },
            arch_misc: ArchMiscDesc::default(),
        }
    }
}

impl Default for QemuVirtDesc {
    fn default() -> Self {
        // per-CPU mtimecmp_addr 需要运行时填充（const fn 限制）。
        let mut d = Self::new();
        const MTIMECMP_BASE: usize = 0x200_4000;
        const MTIMECMP_STRIDE: usize = 8;
        for i in 0..d.cpu_topology.nr_cpus as usize {
            d.cpu_topology.cpus[i] = CpuInfo {
                hw_id: i as u64,
                gicr_base: None,
                mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE),
            };
        }
        d
    }
}

impl PlatformDesc for QemuVirtDesc {
    // 返回 &dyn 引用——具体 struct 自动 trait object 化。
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc { &self.ic }
    fn timer(&self) -> &dyn TimerDesc { &self.timer }
    fn early_console(&self) -> Option<&dyn ConsoleDesc> { Some(&self.console) }
    fn cpu_topology(&self) -> CpuTopology { self.cpu_topology }
    fn arch_misc(&self) -> ArchMiscDesc { self.arch_misc }
    fn source(&self) -> PlatformSource { PlatformSource::QemuVirt }
}
```

`aarch64`/`x86_64` 的 `QemuVirtDesc` 结构完全一致，区别仅在：
- 持有的 sub-descriptor 类型不同（如 aarch64 持 `Gicv3Desc` + `ArmGenericTimerDesc` + `MmioSerialDesc`，x86_64 持 `ApicDesc` + `PitDesc` + `IsaSerialDesc`）。
- 硬编码 MMIO 地址/频率不同（见 `arch/aarch64.rs:84-104`、`arch/x86_64.rs:87-111` 的 `new()` 实现）。

参见 `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 三个 per-arch `QemuVirtDesc` 实现。

#### 4.2.2 `DeviceTreeDesc` 解析器（aarch64 / riscv64）

```rust
// os/libs/minix-platform/src/device_tree.rs:63-85
//
// ic/timer/console 字段类型按 arch cfg-gate：riscv64 持 PlicDesc/ClintDesc/Riscv64ConsoleDesc，
// aarch64 持 Gicv3Desc/ArmGenericTimerDesc/MmioSerialDesc。其他 arch 上 DeviceTreeDesc
// 编译为占位结构（parse() 直接返回 UnsupportedArch）。

#[derive(Clone, Copy)]
pub struct DeviceTreeDesc {
    /// 中断控制器描述符（arch 特化具体类型，上层只看到 &dyn InterruptControllerDesc）。
    #[cfg(target_arch = "riscv64")]
    ic: PlicDesc,
    #[cfg(target_arch = "aarch64")]
    ic: Gicv3Desc,
    /// 定时器描述符（arch 特化）。
    #[cfg(target_arch = "riscv64")]
    timer: ClintDesc,
    #[cfg(target_arch = "aarch64")]
    timer: ArmGenericTimerDesc,
    /// 早期控制台描述符（可选，arch 特化）。
    #[cfg(target_arch = "riscv64")]
    console: Option<Riscv64ConsoleDesc>,
    #[cfg(target_arch = "aarch64")]
    console: Option<MmioSerialDesc>,
    /// CPU 拓扑（跨架构共用）。
    cpu_topology: CpuTopology,
    /// 架构杂项（跨架构共用）。
    arch_misc: ArchMiscDesc,
}

impl DeviceTreeDesc {
    pub unsafe fn parse(dtb_phys: usize) -> Result<Self, DtParseError> { ... }
    pub fn from_bytes(dtb: &[u8]) -> Result<Self, DtParseError> { ... }
}

// trait impl 仅在支持 DTB 的 arch 上编译（cfg-gated）。
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
impl PlatformDesc for DeviceTreeDesc {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc { &self.ic }
    fn timer(&self) -> &dyn TimerDesc { &self.timer }
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        self.console.as_ref().map(|c| c as &dyn ConsoleDesc)
    }
    fn cpu_topology(&self) -> CpuTopology { self.cpu_topology }
    fn arch_misc(&self) -> ArchMiscDesc { self.arch_misc }
    fn source(&self) -> PlatformSource { PlatformSource::DeviceTree }
}
```

关键设计：**eager parsing**。`Fdt` 借用 DTB 切片，但 `PlatformDesc` 必须 `'static + Send + Sync`。`parse()` 一次性遍历 FDT，把所有需要的值提取到具体 arch sub-descriptor struct 的字段中（`PlicDesc`/`Gicv3Desc`/`ClintDesc`/`ArmGenericTimerDesc`/...），然后丢弃 `Fdt` 借用。结果 `DeviceTreeDesc` 是 `'static` 且无需分配器。

> **`dtb_phys` 为何可直接解引用**：`parse()` 把物理地址直接当虚拟地址用（`fdt::Fdt::from_ptr(dtb_phys as *const u8)`）。安全前提：boot 早期（T2.5）内核已启用分页，且恒等映射覆盖低 4GB（C `pg_identity()` 语义，见 [02-higher-half-kernel.md](02-higher-half-kernel.md)）；DTB 由 boot-shim 留在静态固件内存（低地址，远低于 4GB，见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)），VA == PA 成立。若未来 DTB 位于映射范围外（物理高端），`parse` 前需先建立临时映射——当前 UEFI/OpenSBI 路径不触发。

> 参见 `os/libs/minix-platform/src/device_tree.rs:63-430`（含 cfg-gated 字段定义、arch 特化解析函数、`impl PlatformDesc`）。

#### 4.2.3 `AcpiDesc` 解析器（x86-64）

```rust
// os/libs/minix-platform/src/acpi.rs:111-118
//
// AcpiDesc 只在 x86_64 编译（由 kind::parse_by_kind 的 cfg 选择）。
// 字段类型直接用具体 arch struct（ApicDesc/PitDesc/IsaSerialDesc），
// 上层只看到 &dyn InterruptControllerDesc 等 trait object。

#[derive(Clone, Copy)]
pub struct AcpiDesc {
    ic: ApicDesc,                          // 具体 x86-64 中断控制器 struct
    timer: PitDesc,                        // 具体 x86-64 定时器 struct
    console: Option<IsaSerialDesc>,        // 具体 x86-64 控制台 struct
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl AcpiDesc {
    pub unsafe fn parse(rsdp_phys: usize) -> Result<Self, AcpiParseError> { ... }

    /// Construct from pre-parsed values (for tests).
    pub fn from_parsed(
        ic: ApicDesc,
        timer: PitDesc,
        console: Option<IsaSerialDesc>,
        cpu_topology: CpuTopology,
    ) -> Self { ... }
}

impl PlatformDesc for AcpiDesc {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc { &self.ic }
    fn timer(&self) -> &dyn TimerDesc { &self.timer }
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        self.console.as_ref().map(|c| c as &dyn ConsoleDesc)
    }
    fn cpu_topology(&self) -> CpuTopology { self.cpu_topology }
    fn arch_misc(&self) -> ArchMiscDesc { self.arch_misc }
    fn source(&self) -> PlatformSource { PlatformSource::Acpi }
}
```

解析链：RSDP → XSDT/RSDT → MADT（`"APIC"`）。从 MADT 提取：

- LAPIC base（MADT header 的 `Local APIC Address`）。
- IOAPIC base（第一个 `IOAPIC` 结构记录的 `ioapic_addr`）。
- CPU 拓扑（`Processor LAPIC` 结构记录，按 `flags & 1` 判断是否启用；x2APIC 结构 type 9 同样处理）。

构造 `ApicDesc { lapic_base, ioapic_base, nr_irqs }` + `PitDesc { pit_base_freq: 1_193_182, lapic_base }` + `IsaSerialDesc { port_base: 0x3F8 }`，存入 `AcpiDesc` 字段。上层通过 `&dyn InterruptControllerDesc` 拿到引用，arch 层消费代码再 `as_any().downcast_ref::<ApicDesc>()` 读取 `lapic_base` 等字段。

> 参见 `os/libs/minix-platform/src/acpi.rs:111-483`（含 struct 定义、RSDP/XSDT/MADT 解析、`impl PlatformDesc`）。

### 4.3 `PlatformContext` 全局存储

> **设计决策**：§3.5 选定 `AssumeSyncCell` 作为"单线程全局状态"统一原语。本节给出具体实现。

```rust
// os/libs/minix-platform/src/global.rs:44-105

static PLATFORM: AssumeSyncCell<Option<PlatformContext>> = AssumeSyncCell::new(None);

pub struct PlatformContext {
    pub desc: PlatformDescEnum,
}

/// Compile-time-fixed dispatch enum over all descriptor sources.
///
/// `DeviceTree` 和 `Acpi` 变体按 arch cfg-gate（DTB 路径只在 ARM64/RISC-V 编译，
/// ACPI 路径只在 x86-64 编译）。`QemuVirt` 变体始终存在（每 arch 都有兜底）。
/// 注意：变体名是"解析来源"（DeviceTree/Acpi/QemuVirt），不是"硬件品牌"
/// （Apic/Gicv3/Plic）——品牌名隐藏在 sub-descriptor 的具体 struct 里。
pub enum PlatformDescEnum {
    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
    DeviceTree(DeviceTreeDesc),
    #[cfg(target_arch = "x86_64")]
    Acpi(AcpiDesc),
    QemuVirt(QemuVirtDesc),
}
```

初始化与访问 API：

```rust
// os/libs/minix-platform/src/global.rs:185-267

/// 初始化全局平台上下文。在 T2.5 阶段调用。
pub unsafe fn init(desc: PlatformDescEnum) {
    unsafe { *PLATFORM.get() = Some(PlatformContext { desc }) };
}

/// 根据 KernelInfo 构造 PlatformDesc 并初始化全局。
///
/// 遍历 `kinfo.platform_sources`（boot-shim 偏好顺序），调用 `parse_by_kind()`
/// 分派到对应解析器。**取第一个解析成功的**——Linux `acpi=on/off/force` 模型：
/// 多源并存时按 boot-shim 给定的顺序尝试，第一个成功即用。
pub unsafe fn init_from_kinfo(kinfo: &KernelInfo) {
    let mut parsed: Option<PlatformDescEnum> = None;
    for source in kinfo.platform_sources {
        // SAFETY: caller guarantees each source's phys_addr points to a
        // valid firmware table.
        match unsafe { parse_by_kind(*source) } {
            Ok(desc) => {
                parsed = Some(desc);
                break;  // 第一个成功即停止
            }
            Err(_e) => {
                // 解析失败：继续尝试下一个 source（dev 构建可日志，release 静默）。
            }
        }
    }

    let desc = match parsed {
        Some(d) => d,
        None => qemu_fallback_or_panic("no platform source parsed successfully"),
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
        PlatformDescEnum::QemuVirt(QemuVirtDesc::default())
    } else {
        panic!(
            "platform::init_from_kinfo: {} and not a dev build (no QemuVirt fallback in release)",
            reason
        );
    }
}
```

`init_from_kinfo()` 的错误处理策略：

- **dev 构建**（`debug_assertions` 启用）：warn-and-fallback——返回 `QemuVirtDesc::default()`，测试不中断。
- **release 构建**：panic——真实硬件不能没有描述符运行。
- **多源并存**：`platform_sources` 切片按 boot-shim 偏好顺序尝试；任意一个解析成功即用，全失败才走兜底。

### 4.4 `KernelInfo` 扩展与 `PlatformDescSource`

> **设计决策**：§3.7 选定 "不透明 `PlatformDescKind` + `PlatformDescSource` 切片"（TODO-01-2 修复后）。本节给出具体定义与 boot-shim 端定位方式。

```rust
// os/libs/minix-boot/src/kernel_info.rs:81-102

pub struct KernelInfo {
    // ... 现有字段保持不变 ...

    /// 平台描述符源——不透明 handle 切片，承载 (kind, phys_addr) 纯数据对。
    ///
    /// 空切片表示 boot-shim 未提供（QEMU virt 兜底路径）。非空时按 boot-shim
    /// 偏好顺序排列，kernel 取第一个解析成功的（Linux `acpi=on/off/force` 模型）。
    pub platform_sources: &'static [PlatformDescSource],
}
```

`PlatformDescSource` 与 `PlatformDescKind` 定义在 `minix-boot::platform`：

```rust
// os/libs/minix-boot/src/platform.rs:54-150

/// 不透明 kind 标签（u32 包装）。上层只看到一个整数，不知道它对应 DTB 还是 RSDP。
/// `minix-platform::kind` 模块 re-export `DTB`/`RSDP` 常量并提供 `parse_by_kind()` 分派。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformDescKind(u32);

/// 平台描述符源——纯数据 (kind, phys_addr)，跨二进制安全。
#[derive(Clone, Copy)]
pub struct PlatformDescSource {
    kind: PlatformDescKind,
    phys_addr: PhysBytes,
}

impl PlatformDescSource {
    pub const fn new(kind: PlatformDescKind, phys_addr: PhysBytes) -> Self {
        Self { kind, phys_addr }
    }
    pub fn kind(&self) -> PlatformDescKind { self.kind }
    pub fn phys_addr(&self) -> PhysBytes { self.phys_addr }
}

// 已知 kind 常量（boot handoff 协议契约）。
pub const DTB: PlatformDescKind = PlatformDescKind::new(1);  // ARM64/RISC-V
pub const RSDP: PlatformDescKind = PlatformDescKind::new(2); // x86-64
```

boot-shim 定位原始指针的位置（`find_platform_sources()`）：

- UEFI 路径：`os/boot-shim/src/uefi_helpers.rs:52-98` 扫描 UEFI Configuration Table。
  - x86-64：查找 ACPI GUID，构造 `PlatformDescSource::new(RSDP, PhysBytes(addr))`。
  - aarch64：**优先**查找 DTB GUID，**再**查找 ACPI GUID（DTB 在前，ACPI 兜底）——支持 SBBR 服务器场景的 DTB+ACPI 共存。
  - riscv64：UEFI 路径暂未实现，返回空切片。
- OpenSBI 路径（riscv64）：`os/boot-shim/src/opensbi_helpers.rs:222-260` 从 `a1` 寄存器拿 DTB 物理地址，构造 `PlatformDescSource::new(DTB, PhysBytes(addr))`，返回单元素切片。

> 参见 `os/libs/minix-boot/src/platform.rs:97-104` 定义 `DTB`/`RSDP` 常量；`os/libs/minix-platform/src/kind.rs:28` re-export 这两个常量并定义 `parse_by_kind()` 分派函数。

### 4.5 硬件 trait 实例化实现（§3.4 的对应实现）

> **设计决策**：§3.4 选定"硬件 trait 必须带实例状态，地址在 `new(desc)` 注入"。本节给出具体实现案例。
>
> 当前只有 RISC-V 64 时钟有完整实现代码作为示例；aarch64/x86-64 时钟与各架构 `InterruptController` / `ArchInit` 严格遵循同一模式，参见 §3.4.3 的"同样模式应用到其他硬件 trait"段。

#### 4.5.1 `ClockArch` 实例化：RISC-V 64

```rust
// os/arch/src/riscv64/clock.rs
//
// 注意：new() 接收 &dyn TimerDesc（trait object），不是 enum。
// arch 层通过 as_any().downcast_ref::<ClintDesc>() 拿到具体品牌 struct 读取字段。

use minix_platform::arch::riscv64::ClintDesc;  // 品牌 struct 在 arch 子模块可见

pub struct Riscv64ClockArch {
    mtime_addr: usize,
    mtimecmp_base: usize,
    mtimecmp_stride: usize,
    freq: u64,
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &dyn TimerDesc) -> Self {
        // downcast 一次（仅在 init 阶段）：拿到具体 ClintDesc struct。
        let clint = desc.as_any()
            .downcast_ref::<ClintDesc>()
            .expect("Riscv64ClockArch::new: expected ClintDesc");
        Self {
            mtime_addr: clint.mtime_addr,
            mtimecmp_base: clint.mtimecmp_base,
            mtimecmp_stride: clint.mtimecmp_stride,
            freq: clint.freq,
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

**实例化的好处**：实例字段持有从 `PlatformDesc` 解析出的地址，`read_ticks` 直接读字段——零额外间接（编译器可把字段 load 提到循环外）。所有基址都在 `new(desc)` 构造时一次性注入（通过 `Any` downcast 从具体品牌 struct 拷贝过来），构造后实例字段不再变更，热路径无需做任何基址检查或二次设置，也无需再次 vtable 查找。

#### 4.5.2 同样模式应用到其他硬件 trait

| Trait | 实例类型 | 来源 |
|-------|---------|------|
| `ClockArch` | `Aarch64ClockArch`（读 `CNTFRQ_EL0` + `CNTPCT_EL0`）、`X86ClockArch`（PIT + LAPIC 计数器） | §3.4.3 + `os/arch/src/{aarch64,x86_64}/clock.rs` |
| `InterruptController` | `Riscv64Plic` / `Aarch64Gicv3` / `X86Apic` | §3.4.3 + `os/plat/src/{riscv64,aarch64,x86_64}/interrupt.rs` |
| `ArchInit` | 各架构 `arch_init()` 函数封装为实例方法 | §3.4.3 + `os/arch/src/{riscv64,aarch64,x86_64}/arch_init.rs` |

模式一致：每个实现 `new(desc: &dyn XxxDesc) -> Self`，构造时通过 `as_any().downcast_ref::<ConcreteDesc>()` 从具体品牌 struct 拷贝字段到实例字段；热路径方法直接读字段，不重新解析 `PlatformDesc`，也不再次 downcast。

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

> 参见 `os/kernel/src/lib.rs:359` 调用 `minix_platform::init_from_kinfo(kernel_info)`（位于 `init_protection` 之前，L354-363 为 Phase A.5 平台发现段），紧接 `init_protection` 和 `init_clock_and_interrupts`。

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
| x86-64 | `IsaSerialDesc { port_base }`（实现 `ConsoleDesc` trait） | 由 ACPI SPCR 或 `QemuVirtDesc` 提供 |
| aarch64 | `MmioSerialDesc { mmio_base }`（实现 `ConsoleDesc` trait） | 由 DTB 或 `QemuVirtDesc` 提供 |
| riscv64 | `Riscv64ConsoleDesc::sbi()`（SBI ecall，无 MMIO）或 `::mmio(base)` | `QemuVirtDesc` 提供固定变体 |

#### 4.7.3 bootstrap 路径的时序约束

boot-shim 和极简 test-kernel 在 `PlatformDesc` 初始化之前就需输出（例如 panic 信息）。这些路径仍允许使用架构默认常量（x86 COM1、aarch64 PL011、riscv64 SBI），直到 kernel 的 `init_from_kinfo()` 建立 `PlatformDesc` 后再按描述符重新配置。这是一个**启动时序约束**，不是设计回退。

### 4.8 多核扩展预留

`PlatformDesc` 的子结构为 SMP 预留了多核信息：

- `CpuTopology`：CPU 数量、每个 CPU 的 `CpuInfo`（hw_id、gicr_base、mtimecmp_addr）。
- ARM64 `Gicv3Desc`：`gicr_stride` 字段用于计算 per-CPU Redistributor 地址（消费方通过 `as_any().downcast_ref::<Gicv3Desc>()` 拿到）。
- RISC-V `ClintDesc`：`mtimecmp_stride` 字段用于计算 per-hart mtimecmp 地址（消费方通过 `as_any().downcast_ref::<ClintDesc>()` 拿到）。

**当前阶段**：`QemuVirtDesc` 已按 4 核编码（`nr_cpus = 4`，`cpus[0..4]` 填入各自私有地址），QEMU 测试脚本已统一加 `-smp 4`。内核启动代码仍只使用 BSP（cpu 0）；AP 启动逻辑在 [16-smp.md](16-smp.md) 实现。每个 CPU 通过 `CpuTopology.cpus[cpu_id]` 查询自己的私有信息（GICR base / mtimecmp 地址 / APIC ID）。`CpuInfo::zero()` const 构造器（`os/libs/minix-boot/src/platform.rs:213-220`）用于 `const fn` 上下文（如 `QemuVirtDesc::new()` 中初始化 `cpus` 数组），运行时再填充 per-CPU 字段。

### 4.9 `no_std` 约束

- `minix-platform` crate 标注 `#![no_std]`（`os/libs/minix-platform/src/lib.rs:1`）。
- 解析结果存储在 `static` 中（一次性写入，不释放）。
- `PlatformDescEnum` 是枚举（无堆分配），`DeviceTreeDesc`/`AcpiDesc` 的字段是固定大小（`CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组，`MAX_CPUS = 64`，见 `os/libs/minix-boot/src/platform.rs:168`）。
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

> 参见 `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 末尾与 `os/libs/minix-platform/src/global.rs` 末尾均包含 `#[cfg(test)]` 模块（per-arch 测试覆盖各架构的 `QemuVirtDesc` + 品牌 struct downcast）；`desc.rs` 仅为 trait re-export 文件（29 行），无独立测试模块。

> **2026-08-16 P0 重构说明**：本次 review 发现并删除了 3 个 Pattern #38 测试：
> - `os/libs/minix-platform/src/kind.rs:test_kind_constants_are_distinct` —— 新类型 trivial equal 检查
> - `os/libs/minix-platform/src/global.rs:test_platform_desc_panics_before_init` —— 空 body placeholder（自我承认 "no-op"）
> - 强化 `test_qemu_virt_dispatch_via_enum` 把 5 个 `let _ =` 改为真实断言
>
> 详见 [01-04-test-audit.md](../review/codex/01-stage-kernel/01-04-test-audit.md)。

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
