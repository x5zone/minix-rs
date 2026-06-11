# 04-clock-interrupt-init: 时钟与中断初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c:48-74`, `minix3/minix/kernel/arch/i386/i8259.c:28-63`, `minix3/minix/kernel/arch/i386/arch_system.c:246-288`, `minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:22-44`, `minix3/minix/kernel/arch/earm/arch_system.c:101-132`
> **说明**: cstart() 的后半段——init_clock + intr_init + arch_init，让内核能响应硬件事件
> **前置**: [03-kmain-entry-protection.md](03-kmain-entry-protection.md) — 保护模式已初始化

---

## 1. 概述

### 1.0 中断模型：同步 vs 异步，以及为什么内核必须"启用"中断

从 CPU 的视角看，所有"意外事件"分为两类：

| 类型 | 触发者 | CPU 行为 | 举例 | OS 术语 |
|------|--------|---------|------|---------|
| **同步 (Synchronous)** | 指令本身 | 执行某条指令时立即触发 | 除零、缺页、非法指令 | 通常叫 **Exception（异常）** |
| **异步 (Asynchronous)** | 外部设备 | 与当前指令无关，随时到达 | 键盘按下、时钟滴答、网络包到达 | 通常叫 **Interrupt（中断）** |

区别的本质：**异常是"我错了"——CPU 执行了有问题的指令；中断是"你有事"——设备通知 CPU 来处理**。异常可以 retry（缺页处理后重新执行），中断不能 retry（设备状态已变）。

**中断控制器为什么存在？**

CPU 只有一个 INT 引脚（或一个 IRQ 线），但设备有几十个。中断控制器是**转发器 + 优先级仲裁器**：

```
中断控制器          CPU
┌──────────┐      ┌──────┐
│ IRQ 0 ──→│      │      │
│ IRQ 1 ──→│──→──→│ INT  │
│ IRQ 2 ──→│      │      │
│ ...      │      └──────┘
│ IRQ 15 ─→│
└──────────┘
```

它负责：记住哪个设备触发了中断、屏蔽不关心的中断、按优先级排队、告诉 CPU 中断向量号。没有中断控制器，CPU 只能轮询所有设备——这在 boot 阶段也许可行，但在运行阶段是不可接受的性能浪费。

**为什么内核必须显式"初始化 + 启用"中断？**

中断是 CPU 把控制权交给硬件的机制。启用中断 = 内核说"我准备好了，硬件可以打断我了"。boot 阶段必须做到：

1. **保护结构就绪**（03 文档）— 异常向量表已填写，中断来了有地方去
2. **中断控制器已配置**— 知道哪些 IRQ 有效、如何路由
3. **时钟已启动**— 时钟中断是操作系统的"心跳"

没有 1，中断来了就是 triple fault。没有 2，设备中断没地方去。没有 3，内核无法量化时间——进程调度、超时检测、时间片轮转全部依赖时钟中断。

**时钟中断的哲学地位**：时钟中断是操作系统中唯一"一定会来"的中断。键盘可以一直不按，网卡可以没有数据——但时钟每 10ms（100 Hz）一定会触发。它定义了 OS 的时间粒度:调度器的决策周期、系统调用的超时精度、`sleep()` 的最小间隔都基于 tick 长度。时钟中断初始化是 boot 的最后一步证明——**从这一刻起，内核不再是被动等待事件，而是主动驱动事件**。

### 1.1 为什么时钟和中断必须在 proc_init 之前

03 文档结束时，内核已建立了保护模式基础设施（GDT/IDT 或 VBAR_EL1/stvec），但内核仍然无法响应任何硬件事件——中断控制器未初始化，时钟未启动。

`proc_init()` 和 `arch_boot_proc()` 需要时钟和中断的原因：

1. **进程调度依赖时钟**：Minix3 的调度器基于时间片（quantum），由时钟中断驱动。虽然 boot 阶段不调度，但时钟中断处理程序 `timer_int_handler()` 会更新 `bill_ptr` 和进程的 user/sys 时间统计。
2. **中断控制器必须在时钟之前初始化**：时钟中断是 IRQ 0（x86-64）或定时器中断（aarch64/riscv64），需要中断控制器正确路由才能到达 CPU。
3. **arch_init 依赖中断控制器**：x86-64 的 `arch_init()` 初始化 APIC（如果可用），APIC 的定时器替代 8254 PIT 作为时钟源。

因此，cstart 的调用顺序是：**prot_init → init_clock → intr_init → arch_init**——严格有序，不可交换。

### 1.2 三个函数的职责

| 函数 | 做什么 | 依赖 |
|------|--------|------|
| `init_clock()` | 初始化时钟变量：tick 频率、定时器队列、负载统计 | prot_init（IDT 已加载，时钟中断有入口） |
| `intr_init(0)` | 初始化中断控制器：8259A/APIC/GIC/PLIC，mask 所有 IRQ | prot_init（IDT/VBAR/stvec 已加载） |
| `arch_init()` | 架构特定初始化：栈分配、APIC、ACPI、串口、PMU | intr_init（中断控制器已初始化） |

### 1.3 三架构对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟源 | 8254 PIT / LAPIC Timer | ARM Generic Timer | RISC-V mtime |
| 时钟中断 | IRQ 0 → IDT vector 0x50 | IRQ 30 (GIC SPI) | S-mode 中断 (PLIC) |
| 中断控制器 | LAPIC + IOAPIC | GICv3 (Distributor + Redistributor + CPU Interface) | PLIC + CLINT |
| intr_init | 初始化 8259A 或 APIC | 映射 OMAP INTC / 初始化 GIC | 初始化 PLIC |
| arch_init | TSS/APIC/ACPI/串口/BIOS mem cut | TSS/PMU/bsp_init | 串口/PMP |

### 1.4 Rust 版与 C 版的差异

| 方面 | Minix3 C | minix-rs |
|------|---------|----------|
| init_clock | 全局 `kclockinfo` + `kloadinfo` + `clock_timers` | `ClockArch` trait + `ClockState` 结构体 |
| 时钟频率 | `env_get("hz")` 从 boot 参数获取 | 编译时常量 + `KernelInfo` 传递 |
| intr_init | 8259A 硬编码 I/O 端口操作 | `InterruptController::init()` trait 方法 |
| arch_init | 散落在 `arch_system.c` 的全局初始化 | `ArchInit::init()` trait 方法 |
| 中断掩码 | `outb(INT_CTLMASK, ...)` 直接 I/O | `InterruptController::mask/unmask` trait 方法 |

---

## 2. C 源码分析

### 2.1 init_clock()：时钟变量初始化

`clock.c:48-74`：

```c
void init_clock(void)
{
  char *value;

  /* Initialize clock information structure. */
  memset(&kclockinfo, 0, sizeof(kclockinfo));

  /* Get clock tick frequency. */
  value = env_get("hz");
  if (value != NULL)
    kclockinfo.hz = atoi(value);
  if (value == NULL || kclockinfo.hz < 2 || kclockinfo.hz > 50000)
    kclockinfo.hz = DEFAULT_HZ;

  /* Load average data initialization. */
  memset(&kloadinfo, 0, sizeof(kloadinfo));
}
```

**逐行分析**：

1. **`memset(&kclockinfo, 0, ...)`**：清零时钟信息结构体。`kclockinfo` 包含 tick 频率（`hz`）、当前 tick 计数、实时时钟偏移等字段。

2. **`env_get("hz")`**：从 boot 参数获取时钟频率。Minix3 的 boot 参数由 boot monitor 传递，格式是 `key=value` 字符串。默认 `DEFAULT_HZ = 60`（x86，`i386/include/archconst.h:4`）或 `1000`（ARM，`earm/include/archconst.h:4`）。

3. **频率范围检查**：`kclockinfo.hz` 必须在 2~50000 之间。超出范围则使用默认值。

4. **`memset(&kloadinfo, 0, ...)`**：清零负载统计结构体。`kloadinfo` 用于计算 1/5/15 分钟负载平均值。

**关键观察**：`init_clock()` 只初始化**软件变量**，不触碰硬件。硬件定时器（8254 PIT / LAPIC Timer / ARM Generic Timer）的配置在 `arch_init()` 或时钟中断首次使能时完成。

### 2.2 intr_init()：x86-64 中断控制器初始化

`i8259.c:28-63`（8259A PIC 版本）：

```c
int intr_init(const int auto_eoi)
{
  outb( INT_CTL, ICW1_AT);           // ICW1: 边沿触发, 级联, 需 ICW4
  outb( INT_CTLMASK, IRQ0_VECTOR);   // ICW2: 主片中断向量基址
  outb( INT_CTLMASK, (1 << CASCADE_IRQ)); // ICW3: 从片级联引脚
  if (auto_eoi)
    outb( INT_CTLMASK, ICW4_AT_AEOI_MASTER);
  else
    outb( INT_CTLMASK, ICW4_AT_MASTER);   // ICW4: 正常 EOI, 8086 模式
  outb( INT_CTLMASK, ~(1 << CASCADE_IRQ)); // OCW1: 屏蔽除级联外所有 IRQ

  outb( INT2_CTL, ICW1_AT);          // 从片 ICW1
  outb( INT2_CTLMASK, IRQ8_VECTOR);  // 从片 ICW2
  outb( INT2_CTLMASK, CASCADE_IRQ);  // 从片 ICW3
  if (auto_eoi)
    outb( INT2_CTLMASK, ICW4_AT_AEOI_SLAVE);
  else
    outb( INT2_CTLMASK, ICW4_AT_SLAVE);
  outb( INT2_CTLMASK, ~0);           // 从片 OCW1: 屏蔽所有 IRQ

  return OK;
}
```

**8259A 初始化序列**（ICW = Initialization Command Word）：

| 步骤 | 操作 | 含义 |
|------|------|------|
| ICW1 | `0x11` | 边沿触发模式, 级联模式, 需要 ICW4 |
| ICW2 | `IRQ0_VECTOR` | 中断向量基址（IRQ 0 → IDT vector 0x50） |
| ICW3 | 主片 `0x04` | 从片连接到 IRQ2 |
| ICW4 | `0x01`/`0x05` | 正常 EOI / Auto EOI 模式 |
| OCW1 | `~0x04` / `0xFF` | 屏蔽所有 IRQ（级联引脚除外） |

**64 位模式的变化**：x86-64 不使用 8259A PIC，改用 LAPIC + IOAPIC。8259A 的 ICW 序列被替换为 LAPIC 和 IOAPIC 的 MMIO 寄存器配置。但初始化逻辑的语义相同：配置中断路由 + 屏蔽所有 IRQ。

### 2.3 intr_init()：ARM 中断控制器初始化

`omap_intr.c:24-44`（OMAP INTC 版本）：

```c
int intr_init(const int auto_eoi)
{
  if (BOARD_IS_BBXM(machine.board_id)) {
    omap_intr.base = OMAP3_DM37XX_INTR_BASE;
  } else if (BOARD_IS_BB(machine.board_id)) {
    omap_intr.base = OMAP3_AM335X_INTR_BASE;
  } else {
    panic("Can not do the interrupt setup. machine (0x%08x) is unknown\n",
      machine.board_id);
  };
  omap_intr.size = 0x1000;

  kern_phys_map_ptr(omap_intr.base, omap_intr.size,
    VMMF_UNCACHED | VMMF_WRITE,
    &intr_phys_map, (vir_bytes) &omap_intr.base);
  return 0;
}
```

ARM 的 `intr_init()` 比 x86 简单——只需要映射中断控制器的 MMIO 基地址。实际的 GIC 初始化在后续步骤完成。

**注意**：这是 Minix3 ARM（32 位）的实现。minix-rs 使用 ARM64 + GICv3，初始化序列不同（需要初始化 Distributor、Redistributor 和 CPU Interface），但语义相同。

### 2.4 arch_init()：x86-64 架构特定初始化

`arch_system.c:246-288`：

```c
void arch_init(void)
{
  k_stacks = (void*) &k_stacks_start;
  assert(!((vir_bytes) k_stacks % K_STACK_SIZE));

#ifndef CONFIG_SMP
  tss_init(0, get_k_stack_top(0));   // 单 CPU: 初始化 TSS
#endif

#if !CONFIG_OXPCIE
  ser_init();                         // 串口初始化
#endif

#ifdef USE_ACPI
  acpi_init();                        // ACPI 表解析
#endif

#if defined(USE_APIC) && !defined(CONFIG_SMP)
  if (config_no_apic) {
    DEBUGBASIC(("APIC disabled, using legacy PIC\n"));
  }
  else if (!apic_single_cpu_init()) { // APIC 初始化
    DEBUGBASIC(("APIC not present, using legacy PIC\n"));
  }
#endif

  cut_memmap(&kinfo, BIOS_MEM_BEGIN, BIOS_MEM_END);  // 保留 BIOS 区域
  cut_memmap(&kinfo, BASE_MEM_TOP, UPPER_MEM_END);
}
```

`arch_init()` 做的事情比较杂：

1. **内核栈分配**：`k_stacks` 指向预分配的 per-CPU 内核栈区域
2. **TSS 初始化**：单 CPU 模式下初始化 TSS（SMP 模式在 `smp_init()` 中做）
3. **串口初始化**：`ser_init()` 配置 COM1 用于早期调试输出
4. **ACPI 初始化**：解析 ACPI 表获取硬件拓扑信息
5. **APIC 初始化**：如果 APIC 可用，初始化 LAPIC + IOAPIC
6. **内存映射裁剪**：保留 BIOS 区域，防止内核分配器误用

### 2.5 arch_init()：ARM 架构特定初始化

`arch_system.c:101-132`：

```c
void arch_init(void)
{
  k_stacks = (void*) &k_stacks_start;
  assert(!((vir_bytes) k_stacks % K_STACK_SIZE));

#ifndef CONFIG_SMP
  tss_init(0, get_k_stack_top(0));
#endif

  /* enable user space access to cycle counter */
  asm volatile ("MRC p15, 0, %0, c9, c12, 0\t\n": "=r" (value));
  value |= PMU_PMCR_C;   // Reset counter
  value |= PMU_PMCR_E;   // Enable counter hardware
  asm volatile ("MCR p15, 0, %0, c9, c12, 0\t\n": : "r" (value));

  value = PMU_PMCNTENSET_C;  // Enable PMCCNTR cycle counter
  asm volatile ("MCR p15, 0, %0, c9, c12, 1\t\n": : "r" (value));

  value = PMU_PMUSERENR_EN;  // Enable cycle counter in user mode
  asm volatile ("MCR p15, 0, %0, c9, c14, 0\t\n": : "r" (value));

  bsp_init();
}
```

ARM 的 `arch_init()` 主要做 PMU（Performance Monitoring Unit）初始化——启用 cycle counter 供用户态读取。`bsp_init()` 做 board-specific 初始化。

### 2.6 cstart() 中的环境变量解析

`main.c:403-481` 中，`cstart()` 在 `init_clock()` 和 `intr_init()` 之间还做了大量环境变量解析：

```c
/* determine verbosity */
if ((value = env_get(VERBOSEBOOTVARNAME)))
  verboseboot = atoi(value);

/* Get memory parameters. */
value = env_get("ac_layout");
if(value && atoi(value)) {
  kinfo.user_sp = (vir_bytes) USR_STACKTOP_COMPACT;
  kinfo.user_end = (vir_bytes) USR_DATATOP_COMPACT;
}

/* Record miscellaneous information for user-space servers. */
kinfo.nr_procs = NR_PROCS;
kinfo.nr_tasks = NR_TASKS;
strlcpy(kinfo.release, OS_RELEASE, sizeof(kinfo.release));
strlcpy(kinfo.version, OS_VERSION, sizeof(kinfo.version));
```

这些环境变量解析在 Rust 版中大部分不需要——因为：

1. **verboseboot**：Rust 版用 `log` crate 的级别控制，不需要 boot 参数
2. **ac_layout**：Rust 版的地址空间布局在编译时确定（由 `DirectMapArch` trait 提供 `USER_VM_BASE` / `KERNEL_VM_BASE` / `KERNEL_STACK_TOP` 等常量，见 `os/arch/src/arch/paging.rs`），不再需要 boot 时协商
3. **nr_procs/nr_tasks**：Rust 版用常量（编译期 `const NR_PROCS: usize = ...`），不需要运行时设置
4. **release/version**：Rust 版用 `env!("CARGO_PKG_VERSION")`

---

## 3. Rust 设计决策

### 3.1 决策：ClockArch trait 分离硬件和软件

**Minix3 C 的做法**：`init_clock()` 混合了软件初始化（`kclockinfo` 清零 + 频率设置）和硬件初始化（隐含在后续的 `arch_init()` 中）。

**minix-rs 将时钟分为两层**：

1. **`ClockState`**（软件层）：tick 频率、定时器队列、负载统计——架构无关
2. **`ClockArch` trait**（硬件层）：配置硬件定时器、读取当前 tick——架构相关

```rust
/// Architecture-independent clock state.
pub struct ClockState {
    hz: u32,
    uptime: u64,
    // ... timer queue, load average
}

/// Architecture abstraction for hardware timer configuration.
pub trait ClockArch {
    /// Configure and start the hardware timer.
    fn init_timer(hz: u32);

    /// Read the current tick count.
    fn read_ticks() -> u64;
}
```

**原因**：

1. **关注点分离**：软件状态和硬件配置是不同的关注点
2. **可测试性**：`ClockState` 可以在 mock 环境中测试，不需要真实硬件
3. **架构差异**：x86-64 用 8254 PIT / LAPIC Timer，aarch64 用 Generic Timer，riscv64 用 mtime——硬件配置完全不同

### 3.2 决策：init_clock 频率用编译时常量

**Minix3 C 的做法**：`env_get("hz")` 从 boot 参数获取时钟频率，默认 `DEFAULT_HZ = 60`（x86）或 `1000`（ARM）。

**minix-rs 用编译时常量**，原因：

1. **boot 参数不可用**：Rust 版的 `KernelInfo` 不包含 boot 参数字符串
2. **频率是硬件特性**：时钟频率取决于硬件定时器的精度，不应该由 boot 参数决定
3. **简化初始化**：不需要 `env_get` + `atoi` + 范围检查

```rust
/// Default clock tick frequency (Hz).
/// x86-64: 100 Hz (10ms tick), aarch64: 100 Hz, riscv64: 100 Hz
/// C: DEFAULT_HZ — i386: 60 Hz, earm: 1000 Hz (32-bit values)
///   路径: minix3/minix/include/arch/i386/include/archconst.h
///         minix3/minix/include/arch/earm/include/archconst.h
/// minix-rs: unified to 100 Hz (see §2.5 architecture evolution)
pub const DEFAULT_HZ: u32 = 100;
```

### 3.3 决策：InterruptController 已有 trait，只需补 aarch64/riscv64 实现

`InterruptController` trait 已在 `os/arch/src/interrupt.rs` 中定义，x86-64 实现已完成。需要补充 aarch64（GICv3）和 riscv64（PLIC）的实现。

### 3.4 决策：ArchInit trait 封装架构特定初始化

**Minix3 C 的做法**：`arch_init()` 是一个散落着 `#ifdef` 的函数，做 TSS、串口、ACPI、APIC、PMU、内存裁剪等各种事情。

**minix-rs 用 `ArchInit` trait**：

```rust
/// Architecture-specific initialization performed after interrupt controller init.
pub trait ArchInit {
    /// Perform architecture-specific initialization.
    fn init();
}
```

**原因**：

1. **统一接口**：三种架构的 `arch_init()` 语义相同（完成架构特定初始化），但实现完全不同
2. **消除 `#ifdef`**：C 版用 `#ifdef USE_ACPI` / `#ifdef USE_APIC` 选择代码路径，Rust 用 trait 静态分派
3. **可测试性**：mock 实现可以跳过硬件初始化

### 3.5 决策：arch_init 不做内存裁剪

**Minix3 C 的做法**：`arch_init()` 末尾调用 `cut_memmap()` 保留 BIOS 区域。

**minix-rs 不在 arch_init 中做内存裁剪**，原因：

1. **内存映射由 boot-shim 提供**：`KernelInfo.memmap` 已排除保留区域
2. **BIOS 区域不存在于 aarch64/riscv64**：`cut_memmap` 是 x86 特有的
3. **避免架构 trait 中放架构特定逻辑**：如果 x86-64 的 `ArchInit::init()` 做 `cut_memmap`，aarch64/riscv64 的实现就不需要这个步骤——这违反了"trait 方法在所有架构上语义相同"的原则

### 3.6 架构差异对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟硬件 | 8254 PIT (I/O port 0x40-0x43) / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | RISC-V mtime (CLINT MMIO) |
| 时钟频率 | 100 Hz (可配置) | 100 Hz | 100 Hz |
| **中断控制器 (P1-19 拆分)** | LAPIC + IOAPIC | GICv3 (GICD + GICR + CPU IF) | **PLIC** (external) + **CLINT** (timer + software) |
| IRQ 数量 | 64 (APIC mode) | 1020 (GICv3 SPI range) | 1024 (PLIC max) |
| arch_init | 串口 (COM1) + PMP/PMU + APIC MMIO | PMU cycle counter + bsp_init | 串口 + PMP |
| 串口 | COM1 (I/O port 0x3F8) | PL011 (MMIO) | NS16550A (MMIO) |

> **P1-19 修正**: RISC-V "中断控制器" 应明确分为 **PLIC** (external interrupts, 由 `InterruptController` trait 管理) 和 **CLINT** (timer + software interrupts, 由 `ClockArch` 管理)。前表中"RISC-V 中断控制器"列单写"PLIC + CLINT"易混淆——`ClockArch` 用 CLINT 的 mtime/mtimecmp，`InterruptController` 只用 PLIC。

---

## 4. 实现详解

Rust 版将 C 版 `cstart()` 的后三个调用（`init_clock()`、`intr_init()`、`arch_init()`）抽象为三个 trait：`ClockArch` 负责**硬件时钟节拍**，`InterruptController` 负责**中断路由与屏蔽**，`ArchInit` 负责**架构杂项初始化**。拆分的依据是这三个职责在硬件层面完全独立——时钟芯片、中断控制器、串口/ACPI 是三类不同的外设，各自的初始化逻辑互不依赖。

**`ClockArch` 的抽象语义**：

> 回答"谁来打节拍、多快打一次"。

- `init_timer(hz)`：配置硬件定时器以 `hz` Hz 的频率产生周期性中断。x86-64 使用 PIT（可编程间隔定时器）或 LAPIC timer，aarch64 使用 ARM Generic Timer（CNTP_* 寄存器），riscv64 使用 CLINT mtimecmp。
- `read_ticks()`：读取硬件 tick 计数，用于精细计时和性能分析。

**`InterruptController` 的抽象语义**：

> 回答"中断来了怎么路由、怎么开关"。

- `init()`：初始化中断控制器，屏蔽所有 IRQ 线。x86-64 使用 8259A PIC（单核）或 IOAPIC（多核），aarch64 使用 GIC（Generic Interrupt Controller），riscv64 使用 PLIC（Platform-Level Interrupt Controller）。
- `mask(irq)` / `unmask(irq)`：开关单个 IRQ 线。这是驱动程序注册中断时的核心操作——驱动先注册处理函数，再 `unmask` 启用该中断。
- `ack(irq)` / `eoi(irq)`：中断响应的"握手"协议。CPU 收到中断后必须先 `ack`（acknowledge，告诉控制器"我收到了"），处理完后再 `eoi`（end-of-interrupt，告诉控制器"可以发下一个了"）。缺少这个握手会导致中断丢失或重复触发。
- `mask_all()`：启动早期屏蔽所有 IRQ 线，确保在驱动注册处理函数之前不会有意外中断触发。

**`ArchInit` 的抽象语义**：

> 回答"除了时钟和中断，还有什么架构特定的杂项"。

- `init()`：执行架构特定的杂项初始化。x86-64 包括串口（COM1）、ACPI（电源管理表）、APIC（本地高级可编程中断控制器）；aarch64 包括 PMU（性能监控单元）；riscv64 包括 PMP（物理内存保护）。这些初始化彼此无关，但都是启动的必要步骤。

三个 trait 的调用顺序由 `init_clock_and_interrupts()` 保证：`ClockArch::init_timer()` → `InterruptController::init()` → `ArchInit::init()`。顺序不可交换——`init_timer()` 必须在 `InterruptController::init()` 之前，因为定时器中断需要中断控制器准备好路由；`ArchInit::init()` 在最后，因为它可能依赖时钟和中断控制器已就绪（如 APIC timer 配置）。

### 4.1 ClockState：架构无关的时钟状态

> 设计决策：§3.1（分离硬件和软件）、§3.2（编译时常量）

```rust
/// Number of load history slots for load average calculation.
/// C: _LOAD_HISTORY — include/minix/type.h:97
pub const LOAD_HISTORY_SIZE: usize = 16;

/// Architecture-independent clock state.
///
/// Manages tick frequency, uptime counter, realtime tracking, and load
/// average. Hardware timer configuration is delegated to `ClockArch`.
///
/// C: kclockinfo + kloadinfo + clock_timers — clock.c:33-44
pub struct ClockState {
    /// Clock tick frequency in Hz.
    /// C: kclockinfo.hz — type.h:119
    hz: u32,

    /// System uptime in ticks since boot.
    /// C: kclockinfo.uptime — type.h:107
    uptime: u64,

    /// Real time in ticks since boot (may differ from uptime due to adjtime).
    /// C: kclockinfo.realtime — type.h:109
    realtime: u64,

    /// Boot time in seconds since UNIX epoch.
    /// C: kclockinfo.boottime — type.h:105
    boottime: u64,

    /// Number of ticks to adjust realtime by (positive = speed up, negative = slow down).
    /// C: adjtime_delta — clock.c:44
    adjtime_delta: i32,

    /// Load average tracking data.
    /// C: kloadinfo (struct loadinfo) — type.h:98
    loadinfo: LoadInfo,
}

/// Load average tracking data.
///
/// Tracks the number of runnable processes over time to compute
/// 1/5/15 minute load averages.
///
/// C: struct loadinfo — include/minix/type.h:98
struct LoadInfo {
    /// History of process counts per sample slot.
    /// C: proc_load_history[_LOAD_HISTORY] — type.h:99
    proc_load_history: [u16; LOAD_HISTORY_SIZE],

    /// Last slot written in proc_load_history.
    /// C: proc_last_slot — type.h:100
    proc_last_slot: u16,

    /// Uptime at last load sample.
    /// C: last_clock — type.h:101
    last_clock: u64,
}

impl Default for LoadInfo {
    fn default() -> Self {
        Self {
            proc_load_history: [0; LOAD_HISTORY_SIZE],
            proc_last_slot: 0,
            last_clock: 0,
        }
    }
}

impl ClockState {
    pub fn new() -> Self {
        Self {
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime: 0,
            boottime: 0,
            adjtime_delta: 0,
            loadinfo: LoadInfo::default(),
        }
    }

    pub fn hz(&self) -> u32 { self.hz }
    pub fn uptime(&self) -> u64 { self.uptime }
    pub fn realtime(&self) -> u64 { self.realtime }

    /// Called on each clock tick.
    /// C: timer_int_handler() — clock.c:70
    pub fn tick(&mut self) {
        self.uptime += 1;

        // Update realtime with adjtime_delta adjustment.
        // C: clock.c:92-103
        if self.adjtime_delta != 0 && self.uptime & 0x1 != 0 {
            self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
            self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
        } else {
            self.realtime += 1;
        }

        // TODO: update load average (kloadinfo) — clock.c:275-291
        // TODO: check timer queue (clock_timers) — clock.c:160-161
    }
}
```

### 4.2 ClockArch trait

> 设计决策：§3.1（分离硬件和软件）

```rust
/// Architecture abstraction for hardware timer configuration.
///
/// Each architecture implements this trait to configure its hardware
/// timer source and provide tick-reading capability.
///
/// | Architecture | Timer Source | Frequency Register | Counter Register |
/// |-------------|-------------|-------------------|-----------------|
/// | x86-64 | 8254 PIT / LAPIC Timer | PIT divisor | LAPIC CCR |
/// | aarch64 | Generic Timer | CNTFRQ_EL0 | CNTPCT_EL0 |
/// | riscv64 | CLINT mtime | mtimecmp | mtime |
pub trait ClockArch {
    /// Configure and start the hardware timer at the given frequency.
    ///
    /// Called once during `init_clock_and_interrupts()`. After this call, the timer
    /// generates periodic interrupts at `hz` Hz.
    ///
    /// C: init_clock() hardware portion + arch_init() APIC timer
    fn init_timer(hz: u32);

    /// Read the current hardware tick count.
    ///
    /// Used for fine-grained timing and profiling.
    fn read_ticks() -> u64;
}
```

### 4.3 x86-64 ClockArch 实现

```rust
/// x86-64 clock using 8254 PIT as boot timer, LAPIC Timer as runtime timer.
///
/// C: clock.c hardware init + apic.c lapic_enable()
pub struct X86_64ClockArch;

/// 8254 PIT base frequency in Hz.
const PIT_BASE_FREQ: u32 = 1_193_182;
/// PIT command port.
const PIT_COMMAND: u16 = 0x43;
/// PIT channel 0 data port.
const PIT_CHANNEL0: u16 = 0x40;
/// PIT command: channel 0, lobyte/hibyte access, rate generator mode.
const PIT_CMD_RATE_GEN: u8 = 0x36;

impl ClockArch for X86_64ClockArch {
    fn init_timer(hz: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // C: intr_init_8254() — i8259.c equivalent
        let divisor = (PIT_BASE_FREQ / hz) as u16;

        unsafe {
            // Send command byte: channel 0, lobyte/hibyte, rate generator
            core::arch::asm!("out 0x43, al", in("al") PIT_CMD_RATE_GEN);
            // Send divisor low byte
            let lo = divisor as u8;
            core::arch::asm!("out 0x40, al", in("al") lo);
            // Send divisor high byte
            let hi = (divisor >> 8) as u8;
            core::arch::asm!("out 0x40, al", in("al") hi);
        }
    }

    fn read_ticks() -> u64 {
        // Use TSC (Time Stamp Counter) for high-resolution tick reading.
        // C: read_tsc() — not in Minix3, but standard x86-64 practice
        let tsc: u64;
        unsafe {
            core::arch::asm!("rdtsc", out("rax") tsc, out("rdx") _, options(nomem));
        }
        tsc
    }
}
```

### 4.4 aarch64 ClockArch 实现

```rust
/// ARM64 clock using Generic Timer.
///
/// C: earm/arch_system.c PMU init (cycle counter for user mode)
pub struct AArch64ClockArch;

impl ClockArch for AArch64ClockArch {
    fn init_timer(hz: u32) {
        // ARM Generic Timer is configured by firmware (TF-A/U-Boot).
        // We just need to enable the EL1 physical timer and set
        // the compare value for the desired frequency.
        let freq: u64;
        unsafe {
            asm!("mrs {}, cntfrq_el0", out(reg) freq);
        }
        // P1-15: ARM Generic Timer 模式是 absolute compare（CNTP_CVAL_EL0
        // 是 absolute 值，不是 delta）。读当前 count，再加上 freq/hz 作为
        // 下次触发点。CNTP_CTL_EL0 bit 0 = enable, bit 1 = IMASK (masked)。
        let now: u64;
        unsafe {
            asm!("mrs {}, cntpct_el0", out(reg) now);
        }
        let compare = now + freq / hz as u64;
        unsafe {
            // Set the absolute compare value
            asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // Enable the timer (ENABLE=1, IMASK=0, ISTATUS=0)
            asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
        }
    }

    fn read_ticks() -> u64 {
        let count: u64;
        unsafe {
            asm!("mrs {}, cntpct_el0", out(reg) count);
        }
        count
    }
}
```

### 4.5 riscv64 ClockArch 实现

```rust
/// RISC-V 64-bit clock using CLINT mtime.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).

/// CLINT mtime register address for QEMU virt machine.
/// TODO: Should be discovered from device tree.
const CLINT_MTIME: usize = 0x200_BFF8;

/// CLINT mtimecmp register address for QEMU virt machine (hart 0).
const CLINT_MTIMECMP: usize = 0x200_4000;

/// CLINT mtime frequency for QEMU virt machine (10 MHz).
/// TODO: Should be discovered from device tree.
const MTIME_FREQ: u64 = 10_000_000;

pub struct Riscv64ClockArch;

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        // Read current mtime value
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(CLINT_MTIME as *const u64);
        }

        // Calculate interval between interrupts
        let interval = MTIME_FREQ / hz as u64;

        // Set mtimecmp = mtime + interval to schedule first interrupt
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(CLINT_MTIMECMP as *mut u64, mtimecmp);
        }

        // Enable S-mode timer interrupt (STIE bit in sie)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn read_ticks() -> u64 {
        unsafe {
            core::ptr::read_volatile(CLINT_MTIME as *const u64)
        }
    }
}
```

### 4.6 aarch64 InterruptController 实现（GICv3）

```rust
/// GICv3 Distributor base address offset (from GIC base).
/// QEMU virt: GICD at 0x08000000
const GICD_OFFSET: usize = 0x0000_0000;

/// GICv3 Redistributor base address offset (from GIC base).
/// QEMU virt: GICR at 0x080A0000
const GICR_OFFSET: usize = 0x000A_0000;

/// GICD_CTLR: Distributor Control Register.
const GICD_CTLR: usize = 0x0000;
/// GICD_CTLR.EnableGrp1NS bit.
const GICD_CTLR_ENABLE_GRP1NS: u32 = 0x2;
/// GICD_ISENABLER<n>: Interrupt Set-Enable Register.
const GICD_ISENABLER: usize = 0x0100;
/// GICD_ICENABLER<n>: Interrupt Clear-Enable Register.
const GICD_ICENABLER: usize = 0x0180;
/// GICD_IGROUPR<n>: Interrupt Group Register.
const GICD_IGROUPR: usize = 0x0080;
/// GICR_WAKER: Redistributor Wake Register.
const GICR_WAKER: usize = 0x0014;
/// GICR_WAKER.ProcessorSleep bit.
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 0x2;
/// GICR_WAKER.ChildrenAsleep bit (read-only).
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 interrupt controller.
pub struct AArch64InterruptController {
    gicd_base: usize,
    gicr_base: usize,
    nr_irqs: usize,
    /// Last acknowledged interrupt ID (saved from ICC_IAR1_EL1 read).
    last_iar: u32,
}

impl AArch64InterruptController {
    /// Construct a new controller with uninitialized base addresses.
    /// MUST call set_base() before init() — assert_ne!() will panic otherwise.
    pub const fn new() -> Self {
        Self {
            gicd_base: 0,
            gicr_base: 0,
            nr_irqs: NR_IRQ_VECTORS,
            last_iar: 0,
        }
    }

    /// Set GIC base addresses from device tree / platform discovery.
    pub fn set_base(&mut self, gicd_base: usize, gicr_base: usize) {
        self.gicd_base = gicd_base;
        self.gicr_base = gicr_base;
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self) {
        // Catch the "forgot set_base()" footgun early.
        assert!(self.gicd_base != 0, "...: gicd_base not set; call set_base() before init()");
        assert!(self.gicr_base != 0, "...: gicr_base not set; call set_base() before init()");
        self.init_distributor();
        self.init_redistributor();
        self.init_cpu_interface();
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num < 32 {
            // PPI/SGI: handled by Redistributor (GICR_ICENABLER0)
            // TODO: implement PPI masking
        } else {
            // SPI: handled by Distributor
            let reg = (irq_num / 32) as usize;
            let bit = 1u32 << (irq_num % 32);
            unsafe { self.gicd_write32(GICD_ICENABLER + reg * 4, bit); }
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num < 32 {
            // TODO: implement PPI unmasking
        } else {
            let reg = (irq_num / 32) as usize;
            let bit = 1u32 << (irq_num % 32);
            unsafe { self.gicd_write32(GICD_ISENABLER + reg * 4, bit); }
        }
    }

    fn ack(&mut self, _irq: IrqVector) {
        // Read ICC_IAR1_EL1 to acknowledge the highest-priority pending interrupt.
        let iar: u64;
        unsafe { core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) iar); }
        self.last_iar = (iar as u32) & 0x00FF_FFFF;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write the same interrupt ID that was read from IAR.
        unsafe { core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) self.last_iar as u64); }
    }

    fn mask_all(&mut self) {
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            unsafe { self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF); }
        }
    }
}
```

> **当前实现状态（2026-06-11）**: GICv3 SPI 路径（IRQ ≥ 32）已完整实现；PPI/SGI（IRQ < 32）mask/unmask 仍为 TODO（需要写 GICR_ICENABLER0/GICR_ISENABLER0，未实现）。Boot-stage 只需要 SPI，因此可工作。

### 4.6.1 init_distributor / init_redistributor / init_cpu_interface 细节

```rust
fn init_distributor(&mut self) {
    unsafe {
        // 1. Assign all SPIs to Group 1 (Non-secure)
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
        }
        // 2. Disable all SPIs
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
        }
        // 3. Enable Distributor (Group 1 Non-secure)
        self.gicd_write32(GICD_CTLR, GICD_CTLR_ENABLE_GRP1NS);
    }
}

fn init_redistributor(&mut self) {
    unsafe {
        // Wake the Redistributor
        self.gicr_write32(GICR_WAKER, 0);
        // Wait until ChildrenAsleep is cleared
        while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
            core::hint::spin_loop();
        }
    }
}

fn init_cpu_interface(&mut self) {
    unsafe {
        // Enable System Register Interface (ICC_SRE_EL1)
        let mut sre: u64;
        core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
        sre |= 0x7; // SRE + Enable + SRE-el1
        core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);
        // Set priority mask to lowest (accept all interrupts)
        core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);
        // Enable Group 1 Non-secure interrupts
        core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
    }
}
```

### 4.7 riscv64 InterruptController 实现（PLIC）

```rust
/// PLIC base address for QEMU virt machine.
/// TODO: Should be discovered from device tree.
const PLIC_BASE: usize = 0x0C00_0000;

/// PLIC register offsets.
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_PENDING: usize = 0x1000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
const PLIC_COMPLETE: usize = 0x200004;

/// S-mode context offset for hart 0.
/// Context 0 = M-mode, Context 1 = S-mode (QEMU virt).
const S_MODE_CONTEXT: usize = 1;

/// RISC-V 64-bit PLIC interrupt controller.
pub struct Riscv64InterruptController {
    plic_base: usize,
    nr_irqs: usize,
    /// S-mode context ID for the current hart.
    context: usize,
    /// Last claimed interrupt ID (saved from claim register read).
    last_claimed: u32,
}

impl Riscv64InterruptController {
    pub const fn new() -> Self {
        Self {
            plic_base: PLIC_BASE,  // QEMU virt default; override via set_base()
            nr_irqs: NR_IRQ_VECTORS,
            context: S_MODE_CONTEXT,
            last_claimed: 0,
        }
    }

    /// Set PLIC base address from device tree / platform discovery.
    pub fn set_base(&mut self, plic_base: usize) {
        self.plic_base = plic_base;
    }
}

impl InterruptController for Riscv64InterruptController {
    fn init(&mut self) {
        // PLIC initialization sequence:
        // 1. Set all interrupt priorities to 1 (minimum active)
        // 2. Disable all interrupts (enable = 0)
        // 3. Set threshold to 0 (accept all priorities)
        unsafe {
            for irq in 1..self.nr_irqs {
                self.plic_write32(PLIC_PRIORITY + irq * 4, 1);
            }
            for word in 0..(self.nr_irqs + 31) / 32 {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }
            self.plic_write32(PLIC_THRESHOLD + self.context * 0x1000, 0);
        }
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 { return; } // IRQ 0 does not exist in PLIC
        let word = irq_num / 32;
        let bit = irq_num % 32;
        unsafe {
            let offset = PLIC_ENABLE + self.context * 0x80 + word * 4;
            let mut val = self.plic_read32(offset);
            val &= !(1u32 << bit);
            self.plic_write32(offset, val);
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 { return; }
        let word = irq_num / 32;
        let bit = irq_num % 32;
        unsafe {
            let offset = PLIC_ENABLE + self.context * 0x80 + word * 4;
            let mut val = self.plic_read32(offset);
            val |= 1u32 << bit;
            self.plic_write32(offset, val);
        }
    }

    fn ack(&mut self, _irq: IrqVector) {
        // Read the claim register to acknowledge the highest-priority
        // pending interrupt. Returns the interrupt ID.
        let claimed: u32;
        unsafe { claimed = self.plic_read32(PLIC_CLAIM + self.context * 0x1000); }
        self.last_claimed = claimed;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write the interrupt ID to the complete register.
        unsafe { self.plic_write32(PLIC_COMPLETE + self.context * 0x1000, self.last_claimed); }
    }

    fn mask_all(&mut self) {
        for word in 0..(self.nr_irqs + 31) / 32 {
            unsafe { self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0); }
        }
    }
}
```

> **当前实现状态（2026-06-11）**: PLIC 已完整实现，base 默认 QEMU virt 地址 0x0C00_0000。Timer 中断由 CLINT 处理（见 `clock.rs`），不在 PLIC 路径。

### 4.8 ArchInit trait

> 设计决策：§3.4（ArchInit trait）、§3.5（不做内存裁剪）

```rust
/// Architecture-specific initialization performed after interrupt controller init.
///
/// This trait encapsulates the `arch_init()` function from Minix3 C,
/// which performs hardware-specific setup that must happen after
/// protection structures and interrupt controller are initialized.
///
/// C: arch_init() — arch/i386/arch_system.c:246 / earm/arch_system.c:101
pub trait ArchInit {
    /// Perform architecture-specific initialization.
    ///
    /// Called once during `init_clock_and_interrupts()`, after `init_clock()` and
    /// `intr_init()` have completed.
    fn init();
}
```

### 4.9 x86-64 ArchInit 实现

```rust
/// COM1 base port (standard PC UART 16550).
pub const COM1_BASE: u16 = 0x3F8;

/// Divisor value for 115200 baud from 1.8432 MHz crystal (divisor = 1).
const COM1_DIVISOR_115200: u8 = 0x01;
const COM1_LCR_DLAB: u8 = 0x80;        // DLAB bit
const COM1_LCR_8N1: u8 = 0x03;          // 8 data bits, no parity, 1 stop
const COM1_FCR_ENABLE: u8 = 0xC7;       // FIFO enable + clear buffers
const COM1_MCR_DTR_RTS_OUT2: u8 = 0x0B; // DTR + RTS + OUT2

unsafe fn ser_init() {
    // Disable all UART interrupts
    outb(COM1_BASE + 1, 0x00);
    // Enable divisor latch access
    outb(COM1_BASE + 3, COM1_LCR_DLAB);
    // Set baud rate to 115200 (divisor = 1)
    outb(COM1_BASE + 0, COM1_DIVISOR_115200);
    outb(COM1_BASE + 1, 0x00);
    // 8 bits, no parity, 1 stop, DLAB off
    outb(COM1_BASE + 3, COM1_LCR_8N1);
    // Enable FIFO
    outb(COM1_BASE + 2, COM1_FCR_ENABLE);
    // DTR + RTS + OUT2
    outb(COM1_BASE + 4, COM1_MCR_DTR_RTS_OUT2);
}

pub struct X86_64ArchInit;

impl ArchInit for X86_64ArchInit {
    fn init() {
        // 1. Per-CPU kernel stacks (handled by linker script)
        // 2. Serial port initialization (COM1 at 0x3F8)
        unsafe { ser_init(); }
        // 3. ACPI table parsing — TODO: not required for QEMU virt bring-up
        // 4. APIC initialization — done by X86_64InterruptController::init()
    }
}
```

### 4.10 aarch64 ArchInit 实现

```rust
pub struct AArch64ArchInit;

impl ArchInit for AArch64ArchInit {
    fn init() {
        // C: arch_init() — earm/arch_system.c:101-132

        // 1. Enable PMU cycle counter for user mode access
        // C: PMU_PMCR_E + PMU_PMCNTENSET_C + PMU_PMUSERENR_EN
        unsafe {
            // PMCR: E (enable) + C (reset event counters) + P (reset cycle counter) = 0x7
            core::arch::asm!("msr pmcr_el0, {}", in(reg) 0x7u64);
            // PMCNTENSET: C bit (bit 31) enables cycle counter
            core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);
            // PMUSERENR: EN bit (bit 0) — allow EL0 (user mode) access
            core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
        }

        // 2. Board-specific initialization (bsp_init)
        // TODO: platform-specific setup (e.g., GIC base address discovery)
    }
}
```

> **当前实现状态（2026-06-11）**: PMU 三个寄存器 (PMCR_EL0 / PMCNTENSET_EL0 / PMUSERENR_EL0) 完整实现。`bsp_init` 仍为 TODO（GIC base 实际由 `AArch64InterruptController::set_base()` 单独调用，不在此路径）。

### 4.11 riscv64 ArchInit 实现

```rust
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn init() {
        // No Minix3 equivalent — derived from RISC-V Privileged Spec

        // 1. Configure PMP (Physical Memory Protection)
        // pmpaddr0 = u64::MAX (match all addresses via NAPOT encoding)
        unsafe {
            core::arch::asm!("csrw pmpaddr0, {}", in(reg) u64::MAX);
            // pmpcfg0 = A=NAPOT (0x18) + X+R+W (0x7) = 0x1F
            // Allows all access to all memory regions.
            core::arch::asm!("csrw pmpcfg0, {}", in(reg) 0x1Fu64);
        }

        // 2. Enable S-mode interrupts
        // SIE bits set: STIE (bit 5) + SSIE (bit 1) = 0x22
        // (SEIE bit 9 not set — handled separately when external sources
        // are configured in interrupt controller)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x22u64);
        }
    }
}
```

> **当前实现状态（2026-06-11）**: PMP entry 0 完整实现（pmpaddr0 + pmpcfg0），SIE 0x22 设置 STIE+SSIE。SEIE 由 InterruptController::unmask() 在使能 PLIC external 中断时单独处理。

### 4.12 init_clock_and_interrupts() 实现

> 设计决策：§3.1~§3.5

```rust
/// Phase 2 of cstart: initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
fn init_clock_and_interrupts() {
    use minix_arch::{ClockArch, ArchInit};

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize hardware timer.
    // C: hardware portion of init_clock + arch_init() APIC timer
    // Architecture-specific ClockArch is selected at compile time.
    CurrentClockArch::init_timer(clock.hz());

    // Step 3: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:22
    // Architecture-specific InterruptController is selected at compile time.
    let mut intr = CurrentInterruptController::new();
    intr.init();  // mask_all() called internally

    // Step 4: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    CurrentArchInit::init();
}
```

---

## 5. 测试要点

测试分为两层：**单元测试**（验证软件状态机逻辑，无硬件依赖）和 **QEMU 集成测试**（验证硬件寄存器配置，需 QEMU + GDB）。

### 5.1 单元测试

单元测试位于多个 `#[cfg(test)]` 模块中，按文件分布如下（**P1-20 验证, 2026-06-11**）：

| 文件 | 测试数 | 验证内容 |
|------|-------|---------|
| `os/arch/src/arch/clock.rs` | 11 | `ClockState` / `LoadInfo` / `DEFAULT_HZ` / `LOAD_HISTORY_SIZE` |
| `os/arch/src/plat/interrupt.rs` | 13 | `IrqVector` / `IrqId` / `IrqNotifyId` / `IrqPolicy` / `IrqAction` / `NR_IRQ_*` |
| `os/arch/src/x86_64/interrupt.rs` | 13 | LAPIC/IOAPIC 寄存器偏移、set_base 覆盖、SVR enable bit、IA32_APIC_BASE MSR index 等 |
| `os/arch/src/x86_64/arch_init.rs` | 7 | COM1 寄存器常量 (DLAB, 8N1, FIFO, MCR)、ser_init 不 panic |
| `os/arch/src/arm64/interrupt.rs` | 7 | GIC 寄存器偏移、WAKER bits、QEMU virt GICD/GICR 偏移、gicd_base=0 防御 panic |
| `os/arch/src/{x86_64,arm64,riscv64}/trap_entry.rs` | 15/3/3 | IDT 门描述符、set_handler 行为、VBAR/stvec 配置（详见 doc 03 §5.3）|
| **总计** | **72** | |

#### 5.1.1 ClockState 测试（`arch/clock.rs`）

| 测试 | 验证内容 |
|------|---------|
| `test_clock_state_new` | `ClockState::new()` 初始 `hz=100`、`uptime=0`、`realtime=0` |
| `test_clock_state_default` | `ClockState::default()` == `ClockState::new()` |
| `test_clock_state_tick_increment_uptime` | 100 次 `tick()` 后 `uptime==100` |
| `test_clock_state_tick_realtime_no_adjtime` | 无 adjtime 时 `realtime == uptime` |
| `test_clock_state_tick_realtime_with_positive_adjtime` | `adjtime_delta=10`：20 次 tick 后 `realtime=30`、`adjtime_delta=0` |
| `test_clock_state_tick_realtime_with_negative_adjtime` | `adjtime_delta=-10`：20 次 tick 后 `realtime=10`、`adjtime_delta=0` |
| `test_clock_state_tick_realtime_adjtime_stops_when_zero` | 逐 tick 验证 adjtime_delta 从 2→0 过程中 realtime 的精确变化 |
| `test_clock_state_large_uptime_no_overflow` | 1M 次 tick（~2.8h@100Hz）无溢出，uptime 和 realtime 精确 |
| `test_load_info_default` | `LoadInfo::default()` 初始化 16 槽全 0、slot=0、clock=0 |
| `test_default_hz_value` | `DEFAULT_HZ == 100` |
| `test_load_history_size` | `LOAD_HISTORY_SIZE == 16` |

**运行命令**：
```bash
cd os/arch && cargo test --lib -- tests:: 2>&1
```

#### 5.1.2 IRQ 类型测试（`plat/interrupt.rs`）

| 测试 | 验证内容 |
|------|---------|
| `test_irq_vector_new` | `IrqVector::new(32).get() == 32` |
| `test_irq_vector_const` | `IrqVector` 可在 const 上下文中使用 |
| `test_irq_vector_boundaries` | min=0, max=63 |
| `test_irq_vector_equality` | `IrqVector` 支持 `==` 和 `!=` |
| `test_irq_id_new` | `IrqId::new(1).get() == 1` |
| `test_irq_id_const` | `IrqId` 可在 const 上下文中使用 |
| `test_irq_notify_id_new` | `IrqNotifyId::new(42).get() == 42` |
| `test_irq_notify_id_const` | `IrqNotifyId` 可在 const 上下文中使用 |
| `test_irq_policy_reenable` | `IrqPolicy::REENABLE.bits() == 0x001` |
| `test_irq_policy_empty` | `IrqPolicy::empty().bits() == 0` |
| `test_nr_irq_constants` | `NR_IRQ_VECTORS == 64`, `NR_IRQ_HOOKS == 64` |
| `test_irq_action_discriminants` | `Completed` != `NotCompleted` |
| `test_irq_vector_debug_format` | `Debug` 输出包含类型名 |

### 5.2 QEMU + GDB 集成测试

QEMU 测试使用批处理 GDB 脚本，自动启动 QEMU、打断点、验证寄存器状态。

测试脚本位于 `os/arch/tests/`：
- `qemu_test_x86_64.sh`
- `qemu_test_aarch64.sh`
- `qemu_test_riscv64.sh`

#### 5.2.1 x86-64 验证

```bash
cd os/arch/tests && ./qemu_test_x86_64.sh build/x86_64/kernel.elf
```

验证内容：
1. `init_clock_and_interrupts()` 断点命中 — 确认执行路径正确
2. 8254 PIT 已配置（端口 0x43/0x40 写入了 rate generator 模式）
3. ClockState 初始化（hz、uptime 初始值正确）

#### 5.2.2 aarch64 验证

```bash
cd os/arch/tests && ./qemu_test_aarch64.sh build/aarch64/kernel.elf
```

验证内容：
1. `init_clock_and_interrupts()` 断点命中 — 确认执行路径正确
2. ARM Generic Timer 已启用：`CNTP_CTL_EL0 & 0x1 == 1`（ENABLE bit）
3. Counter frequency 合理：`CNTFRQ_EL0 > 0`
4. GICv3 CPU Interface 已配置：`ICC_SRE_EL1 & 0x7 == 0x7`（SRE + Enable）
5. Priority mask 已设置：`ICC_PMR_EL1 == 0xFF`

#### 5.2.3 riscv64 验证

```bash
cd os/arch/tests && ./qemu_test_riscv64.sh build/riscv64/kernel.elf
```

验证内容：
1. `init_clock_and_interrupts()` 断点命中 — 确认执行路径正确
2. CLINT mtimecmp 已配置：`*(uint64_t*)0x2004000 != 0`
3. CLINT mtime 在递增：`*(uint64_t*)0x200BFF8` 非零
4. S-mode 中断已使能：`sie & 0x22 == 0x22`（SEIE + STIE）
5. S-mode 状态已配置：`sstatus.SIE` bit 设置

### 5.3 测试覆盖分析

| 维度 | 覆盖项 | 覆盖情况 |
|------|--------|---------|
| ClockState 初始化 | new/default 创建 | ✅ 2 tests |
| ClockState tick 正常路径 | 递增 uptime, realtime | ✅ 2 tests |
| ClockState tick adjtime 正偏差 | realtime 加速, delta 收敛 | ✅ 2 tests |
| ClockState tick adjtime 负偏差 | realtime 减速, delta 收敛 | ✅ 1 test |
| ClockState 大数稳定性 | 无溢出 | ✅ 1 test |
| LoadInfo 初始化 | 字段正确 | ✅ 1 test |
| 常量值验证 | DEFAULT_HZ, LOAD_HISTORY_SIZE | ✅ 2 tests |
| IRQ 类型安全 | IrqVector, IrqId, IrqNotifyId | ✅ 8 tests |
| IRQ 策略/动作 | IrqPolicy, IrqAction 语义 | ✅ 3 tests |
| QEMU x86-64 | PIT 配置, 断点命中 | ✅ 自动化脚本 |
| QEMU aarch64 | Generic Timer, GICv3 寄存器 | ✅ 自动化脚本 |
| QEMU riscv64 | CLINT mtimecmp, sie 寄存器 | ✅ 自动化脚本 |

**未覆盖（后续文档）**：
- Timer queue（`clock_timers`）测试 — 属于定时器超时管理模块
- Load average 更新（`kloadinfo`）测试 — 属于调度器模块
- 时钟中断处理程序（`timer_int_handler`）测试 — 属于异常/中断处理文档

---

## 6. 参见

- [03-kmain-entry-protection.md](03-kmain-entry-protection.md) — cstart 前半段：保护模式初始化
- [05-proc-init-boot-proc.md](05-proc-init-boot-proc.md) — 进程表初始化和 boot 进程加载
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/arch/src/interrupt.rs` — InterruptController trait 定义
- `os/arch/src/x86_64/interrupt.rs` — x86-64 APIC 实现
- `os/kernel/src/clock.rs` — ClockState 实现
