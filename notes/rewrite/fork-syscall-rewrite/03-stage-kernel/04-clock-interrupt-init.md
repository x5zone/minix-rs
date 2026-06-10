# 04-clock-interrupt-init: 时钟与中断初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c:48-74`, `minix3/minix/kernel/arch/i386/i8259.c:28-63`, `minix3/minix/kernel/arch/i386/arch_system.c:246-288`, `minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:24-44`, `minix3/minix/kernel/arch/earm/arch_system.c:101-132`
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
2. **ac_layout**：Rust 版的地址空间布局在编译时确定
3. **nr_procs/nr_tasks**：Rust 版用常量，不需要运行时设置
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
| 时钟硬件 | 8254 PIT (I/O port 0x40-0x43) / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | RISC-V mtime (MMIO) |
| 时钟频率 | 100 Hz (可配置) | 100 Hz | 100 Hz |
| 中断控制器 | LAPIC + IOAPIC (MMIO) | GICv3 (Distributor + Redist + CPU IF) | PLIC (MMIO) + CLINT |
| IRQ 数量 | 64 (APIC mode) | 1020 (GICv3 SPI range) | 1024 (PLIC max) |
| arch_init | TSS + 串口 + ACPI + APIC | PMU cycle counter + bsp_init | 串口 + PMP |
| 串口 | COM1 (I/O port 0x3F8) | PL011 (MMIO) | NS16550A (MMIO) |

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
/// Architecture-independent clock state.
///
/// Manages tick frequency, uptime counter, timer queue, and load average.
/// Hardware timer configuration is delegated to `ClockArch`.
///
/// C: kclockinfo + kloadinfo + clock_timers — clock.c:33-40
pub struct ClockState {
    /// Clock tick frequency in Hz.
    hz: u32,

    /// System uptime in ticks since boot.
    uptime: u64,

    /// Real-time offset (for time adjustment).
    realtime_offset: i64,

    /// Load average data (1/5/15 minute averages).
    loadinfo: LoadInfo,
}

/// Load average tracking data.
///
/// C: kloadinfo — clock.c:42
struct LoadInfo {
    /// Decayed load average counters.
    decr: [u32; 3],  // 1, 5, 15 minute
    /// Accumulated tick counts.
    incr: [u32; 3],
}

impl ClockState {
    pub fn new() -> Self {
        Self {
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime_offset: 0,
            loadinfo: LoadInfo {
                decr: [0, 0, 0],
                incr: [0, 0, 0],
            },
        }
    }

    pub fn hz(&self) -> u32 { self.hz }
    pub fn uptime(&self) -> u64 { self.uptime }

    /// Called on each clock tick.
    /// C: timer_int_handler() — clock.c:76
    pub fn tick(&mut self) {
        self.uptime += 1;
        // TODO: update load average, check timer queue
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

impl ClockArch for X86_64ClockArch {
    fn init_timer(hz: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // PIT base frequency = 1193182 Hz.
        // Divisor = 1193182 / hz.
        let divisor = (1193182 / hz) as u16;
        unsafe {
            // Command byte: channel 0, lobyte/hibyte, rate generator
            asm!("outb {}, 0x43", in(reg) 0x36u8);
            // Divisor low byte
            asm!("outb {}, 0x40", in(reg) (divisor & 0xFF) as u8);
            // Divisor high byte
            asm!("outb {}, 0x40", in(reg) (divisor >> 8) as u8);
        }
    }

    fn read_ticks() -> u64 {
        // Read LAPIC CCR (Current Count Register) or use TSC.
        // For boot phase, use TSC (Time Stamp Counter).
        let tsc: u64;
        unsafe {
            asm!("rdtsc", out("rax") tsc, out("rdx") _, options(nomem));
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
        // Set compare value: freq / hz ticks per interrupt
        let compare = freq / hz as u64;
        unsafe {
            // Set the compare value
            asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // Enable the timer
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
pub struct Riscv64ClockArch;

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        // CLINT mtime frequency is platform-specific.
        // QEMU virt: 10 MHz.
        // Set mtimecmp = mtime + (freq / hz) to schedule next interrupt.
        let mtime_freq: u64 = 10_000_000; // QEMU virt default
        let interval = mtime_freq / hz as u64;

        // Read current mtime
        let mtime: u64;
        unsafe {
            asm!("ld {}, 0xBFF8(x0)", out(reg) mtime, options(nostack));
        }

        // Set mtimecmp
        let mtimecmp = mtime + interval;
        unsafe {
            asm!("sd {}, 0x4000(x0)", in(reg) mtimecmp, options(nostack));
        }
    }

    fn read_ticks() -> u64 {
        let mtime: u64;
        unsafe {
            asm!("ld {}, 0xBFF8(x0)", out(reg) mtime, options(nostack));
        }
        mtime
    }
}
```

### 4.6 aarch64 InterruptController 实现（GICv3）

```rust
/// ARM64 GICv3 interrupt controller.
///
/// GICv3 has three components:
/// - Distributor (GICD): manages SPI routing and priority
/// - Redistributor (GICR): per-CPU, manages SGI/PPI
/// - CPU Interface (ICC_*_EL1): per-CPU, interrupt ack/eoi
///
/// C: omap_intr.c (OMAP INTC, 32-bit ARM)
/// minix-rs uses GICv3 (64-bit ARM + QEMU virt)
pub struct AArch64InterruptController {
    gicd_base: u64,
    gicr_base: u64,
    nr_irqs: usize,
}

impl AArch64InterruptController {
    pub const fn new() -> Self {
        Self {
            gicd_base: 0,  // Set during init from device tree
            gicr_base: 0,
            nr_irqs: NR_IRQ_VECTORS,
        }
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self) {
        // GICv3 initialization sequence:
        // 1. Enable Distributor (GICD_CTLR)
        // 2. Configure SPI routing (GICD_IGROUPR, GICD_ROUTER)
        // 3. Mask all SPIs (GICD_ICENABLER)
        // 4. Enable Redistributor (GICR_WAKER)
        // 5. Configure CPU Interface (ICC_SRE_EL1, ICC_IGRPEN1_EL1)
        // TODO: implement GICv3 MMIO register writes
    }

    fn mask(&mut self, irq: IrqVector) {
        // GICD_ICENABLER<n> = set bit to disable SPI
        let _ = irq;
        // TODO: write to GICD_ICENABLER register
    }

    fn unmask(&mut self, irq: IrqVector) {
        // GICD_ISENABLER<n> = set bit to enable SPI
        let _ = irq;
        // TODO: write to GICD_ISENABLER register
    }

    fn ack(&mut self, _irq: IrqVector) {
        // Read ICC_IAR1_EL1 to acknowledge interrupt
        // TODO: implement
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write ICC_EOIR1_EL1 to signal end of interrupt
        // TODO: implement
    }

    fn mask_all(&mut self) {
        // Disable all SPIs in GICD_ICENABLER registers
        // TODO: implement
    }
}
```

### 4.7 riscv64 InterruptController 实现（PLIC）

```rust
/// RISC-V 64-bit PLIC interrupt controller.
///
/// PLIC (Platform-Level Interrupt Controller) manages external interrupts.
/// CLINT (Core Local Interruptor) handles timer and IPI.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64InterruptController {
    plic_base: u64,
    nr_irqs: usize,
    context_id: u32,
}

impl Riscv64InterruptController {
    pub const fn new() -> Self {
        Self {
            plic_base: 0,  // Set during init from device tree
            nr_irqs: NR_IRQ_VECTORS,
            context_id: 0,
        }
    }
}

impl InterruptController for Riscv64InterruptController {
    fn init(&mut self) {
        // PLIC initialization:
        // 1. Set priority threshold to 0 (accept all priorities)
        // 2. Disable all interrupt sources (enable=0)
        // 3. Set all priorities to 1 (minimum active priority)
        // TODO: implement PLIC MMIO register writes
    }

    fn mask(&mut self, irq: IrqVector) {
        // Write 0 to PLIC enable register for this IRQ
        let _ = irq;
        // TODO: implement
    }

    fn unmask(&mut self, irq: IrqVector) {
        // Write 1 to PLIC enable register for this IRQ
        let _ = irq;
        // TODO: implement
    }

    fn ack(&mut self, _irq: IrqVector) {
        // Read PLIC claim register to acknowledge interrupt
        // TODO: implement
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // Write IRQ ID to PLIC complete register
        // TODO: implement
    }

    fn mask_all(&mut self) {
        // Write 0 to all PLIC enable registers
        // TODO: implement
    }
}
```

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
pub struct X86_64ArchInit;

impl ArchInit for X86_64ArchInit {
    fn init() {
        // C: arch_init() — arch_system.c:246-288

        // 1. Initialize per-CPU kernel stacks
        // C: k_stacks = &k_stacks_start
        // Already handled by linker script in Rust version

        // 2. Initialize serial port for early debug output
        // C: ser_init()
        // TODO: COM1 initialization (0x3F8)

        // 3. Initialize ACPI tables (if available)
        // C: acpi_init()
        // TODO: RSDP search + table parsing

        // 4. Initialize APIC (if available)
        // C: apic_single_cpu_init()
        // TODO: LAPIC + IOAPIC MMIO initialization


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
            // Enable PMU
            asm!("msr pmcr_el0, {}", in(reg) 0x1u64);  // PMCR.E
            // Enable cycle counter
            asm!("msr pmcntenset_el0, {}", in(reg) 0x80000000u64);  // C bit
            // Allow EL0 access
            asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);  // EN bit
        }

        // 2. Board-specific initialization
        // C: bsp_init()
        // TODO: platform-specific setup


    }
}
```

### 4.11 riscv64 ArchInit 实现

```rust
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn init() {
        // No Minix3 equivalent — derived from RISC-V Privileged Spec

        // 1. Configure PMP (Physical Memory Protection)
        // Allow all access for now (OpenSBI may have already configured)
        unsafe {
            // pmpaddr0 = 0xFFFFFFFFFFFFFFFF (all address)
            asm!("csrw pmpaddr0, {}", in(reg) u64::MAX);
            // pmpcfg0 = L+NAPOT+RWX (allow all, locked)
            asm!("csrw pmpcfg0, {}", in(reg) 0x1Fu64);
        }

        // 2. Enable S-mode interrupts
        unsafe {
            asm!("csrs sstatus, {bits}", bits = in(reg) 0x2u64);  // SIE bit
        }


    }
}
```

### 4.12 init_clock_and_interrupts() 实现

> 设计决策：§3.1~§3.5

```rust
/// Phase 2 of cstart: initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
fn init_clock_and_interrupts() {
    use minix_arch::{InterruptController, CurrentInterruptController, ClockArch, CurrentClockArch, ArchInit, CurrentArchInit};

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize hardware timer.
    // C: hardware portion of init_clock + arch_init() APIC timer
    CurrentClockArch::init_timer(clock.hz());

    // Step 3: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    let mut intr = CurrentInterruptController::new();
    intr.init();  // mask_all() called internally

    // Step 4: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    CurrentArchInit::init();
}
```

---

## 5. 测试要点

### 5.1 QEMU + GDB 验证

```bash
# x86-64: 验证 PIT 已配置
qemu-system-x86_64 -kernel kernel.elf -s -S
(gdb) break init_clock_and_interrupts
(gdb) continue
(gdb) finish  # 执行完 init_clock_and_interrupts
(gdb) info registers  # IDT 应已加载

# aarch64: 验证 Generic Timer 已启用
qemu-system-aarch64 -machine virt -kernel kernel.elf -s -S
(gdb) break init_clock_and_interrupts
(gdb) continue
(gdb) finish
(gdb) print $cntp_ctl_el0  # 应为 1 (enabled)

# riscv64: 验证 mtimecmp 已设置
qemu-system-riscv64 -machine virt -kernel kernel.elf -s -S
(gdb) break init_clock_and_interrupts
(gdb) continue
(gdb) finish
(gdb) x/gx 0x200BFF8  # mtimecmp 应非零
```

### 5.2 单元测试

| 测试 | 验证内容 |
|------|---------|
| `test_clock_state_new` | ClockState 初始 uptime=0, hz=DEFAULT_HZ |
| `test_clock_state_tick` | tick() 递增 uptime |
| `test_interrupt_controller_init` | init() 不 panic，所有 IRQ 被 mask |
| `test_interrupt_mask_unmask` | mask + unmask 不 panic |
| `test_arch_init` | init() 不 panic |

---

## 6. 参见

- [03-kmain-entry-protection.md](03-kmain-entry-protection.md) — cstart 前半段：保护模式初始化
- [05-proc-init-boot-proc.md](05-proc-init-boot-proc.md) — 进程表初始化和 boot 进程加载
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/arch/src/interrupt.rs` — InterruptController trait 定义
- `os/arch/src/x86_64/interrupt.rs` — x86-64 APIC 实现
- `os/kernel/src/clock.rs` — ClockState 实现
