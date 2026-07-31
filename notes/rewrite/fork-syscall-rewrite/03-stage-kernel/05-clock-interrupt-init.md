# 05-clock-interrupt-init: 时钟与中断初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c:48-74`, `minix3/minix/kernel/arch/i386/i8259.c:28-63`, `minix3/minix/kernel/arch/i386/arch_system.c:246-288`, `minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:22-44`, `minix3/minix/kernel/arch/earm/arch_system.c:101-132`
> **说明**: cstart() 的后半段——init_clock + intr_init + arch_init，让内核能响应硬件事件
> **前置**: [03-kmain-cstart.md](03-kmain-cstart.md) — 保护模式已初始化

---

## 1. 概述

### 1.0 中断模型：同步 vs 异步，以及为什么内核必须"启用"中断

本章聚焦**时钟与中断控制器的初始化**，并连带讨论同一启动阶段出现的**早期控制台输出**问题：解释 `init_clock()`、`intr_init()`、`arch_init()` 三个 C 函数要回答的问题——操作系统如何获得可量化的时间粒度、如何让设备异步通知 CPU、以及还有哪些架构特定的硬件必须在此阶段就绪。后续章节再说明 Rust 版如何把这些职责拆分为独立的抽象。具体中断/异常 handler、调度与时钟的耦合、SMP/AP 启动、设备驱动的 IRQ 路由等主题留到后续文档。

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

中断是硬件通知 CPU、使 CPU 暂停当前执行并转交控制权给内核处理例程的机制。启用中断 = 内核说"我准备好了，硬件可以打断我了"。boot 阶段必须做到：

1. **保护结构就绪**（03 文档）— 异常/中断向量表已填写，中断来了有入口可去
2. **中断控制器已配置**— 知道哪些 IRQ 有效、如何路由到 CPU
3. **时钟已启动**— 时钟中断是操作系统的"心跳"

没有 1，中断来了就是 triple fault。没有 2，设备中断无法被路由到 CPU，CPU 不知道哪个设备发出了请求。没有 3，内核无法量化时间——进程调度、超时检测、时间片轮转全部依赖时钟中断。

**时钟中断的哲学地位**：时钟中断是操作系统中唯一"一定会来"的中断。键盘可以一直不按，网卡可以没有数据——但时钟每 10ms（100 Hz）一定会触发。它定义了 OS 的时间粒度：调度器的决策周期、系统调用的超时精度、`sleep()` 的实际分辨率都以 tick 长度为粒度，指定更细的时间通常也会被向上取整到下一个 tick。时钟中断初始化是 boot 的最后一步证明——**从这一刻起，内核不再是被动等待事件，而是主动驱动事件**。

### 1.1 为什么时钟和中断必须在 proc_init 之前

03 文档结束时，内核已建立了保护模式基础设施（GDT/IDT 或 VBAR_EL1/stvec），但内核仍然无法响应任何硬件事件——中断控制器未初始化，时钟未启动。

`proc_init()` 和 `arch_boot_proc()` 需要时钟和中断的原因：

1. **进程调度依赖时钟**：Minix3 的调度器基于时间片（quantum），由时钟中断驱动。虽然 boot 阶段不调度，但时钟中断处理程序 `timer_int_handler()` 会更新 `bill_ptr` 和进程的 user/sys 时间统计。
2. **init_clock 只初始化软件变量**：它设置 `kclockinfo.hz`（即 `system_hz`）、清零负载统计，**不触碰硬件定时器**，也不会启用中断。因此它不需要中断控制器先准备好；真正的硬件定时器使能发生在更晚的 `bsp_finish_booting()` 中。
3. **arch_init 会用到时钟频率并依赖中断控制器**：x86-64 的 `arch_init()` 在初始化 APIC 时可能用到 `system_hz`，也会假设中断控制器已经配置好（即使所有 IRQ 仍被屏蔽）。

因此，Minix3 的 `cstart()` 调用顺序是：**prot_init → init_clock → intr_init → arch_init**。这个顺序是源码固定的，但 `init_clock` 与 `intr_init` 之间并没有强硬件依赖——交换它们不会导致错误；真正不能颠倒的是 `arch_init` 必须在 `intr_init` 之后，因为它依赖已配置好的中断控制器。

### 1.2 三个函数的职责

| 函数 | 做什么 | 依赖 |
|------|--------|------|
| `init_clock()` | 初始化时钟变量：tick 频率、定时器队列、负载统计 | prot_init（IDT 已加载，时钟中断有入口） |
| `intr_init(0)` | 初始化中断控制器：8259A（PIC）/APIC/GIC/PLIC，mask 所有 IRQ | prot_init（IDT/VBAR/stvec 已加载） |
| `arch_init()` | 架构特定初始化：栈分配、APIC（本地高级可编程中断控制器）、ACPI（硬件配置/电源管理表）、PMU（性能监控单元）等架构相关设置 | init_clock（提供 `system_hz`）+ intr_init（中断控制器已初始化） |

> **注意**：这里的"tick 频率"不是 CPU 主频，而是 OS 希望定时器每秒产生多少次 tick（如 100 Hz）。硬件定时器本身的输入时钟频率（如 x86 PIT 的 1.193 MHz、ARM Generic Timer 的 CNTFRQ）由架构代码在运行时通过 CPUID/设备树/固件获取，再据此计算分频器；HZ 只是一个策略值。

### 1.3 三架构对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟源 | 8254 PIT / LAPIC Timer | ARM Generic Timer | RISC-V mtime |
| 时钟中断 | IRQ 0 → IDT vector 0x50 | IRQ 30 (GIC SPI) | S-mode 中断 (PLIC) |
| 中断控制器 | LAPIC + IOAPIC | GICv3 (Distributor + Redistributor + CPU Interface) | PLIC + CLINT |
| intr_init | 初始化 8259A 或 APIC | 映射 OMAP INTC / 初始化 GIC | 初始化 PLIC |
| arch_init | TSS（任务状态段）/APIC/ACPI/BIOS mem cut | TSS（软件抽象，保存 sp0）/PMU（性能监控单元）/bsp_init | PMP（物理内存保护） |

> **注**：x86-64 的 `TSS` 是硬件任务状态段，详见 [03-kmain-cstart.md](03-kmain-cstart.md) §1.4c；ARM 端口虽然也有同名 `tss_init()`/`struct tss_s`，但它**不是硬件 TSS**，只是一个软件抽象，里面只存一个 `sp0`（中断时用的内核栈指针），外加在栈顶记录 CPU id。

### 1.4 本章小结

本章覆盖 cstart 的后半段：在 [03-kmain-cstart.md](03-kmain-cstart.md) 建立保护结构之后，内核还需要：

1. **时钟**——让 OS 拥有可量化的时间粒度；
2. **中断控制器**——让设备（包括时钟）能异步通知 CPU；
3. **架构杂项初始化**——完成 PMU（性能监控单元）、ACPI（硬件配置/电源管理表）/APIC（本地高级可编程中断控制器）等架构特定设置。不同架构还可能在此阶段初始化串口、TSS 等硬件。

Minix3 的 `cstart()` 调用顺序是 **prot_init → init_clock → intr_init → arch_init**。这个顺序是源码固定的，但 `init_clock` 与 `intr_init` 之间没有强硬件依赖；真正不能颠倒的是 `arch_init` 必须在 `intr_init` 之后，因为它依赖已配置好的中断控制器。Rust 版会在后续章节说明如何把这些职责拆分为独立的抽象，按"职责"而非"调用顺序"组织代码。具体 handler 实现、调度耦合、SMP/AP 启动等不在本章范围。

---

## 2. C 源码分析

### 2.1 init_clock()：时钟变量初始化

`clock.c:48-64`（函数体；L66-70 是 `timer_int_handler` 注释，不在函数内）：

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

   > **设计层面的取舍**：x86 的 60 Hz 来自 IBM PC 8254 PIT 的输入时钟（1.1931816 MHz）与分频器选择，是早期 PC 的遗留值；ARM 的 1000 Hz 提供更高精度但增加中断开销。Rust 版把默认 tick 率统一为 100 Hz，并把这一取舍放在设计决策章节（§3.2）讨论，而不是在 C 源码分析中展开。

3. **频率范围检查**：`kclockinfo.hz` 必须在 2~50000 之间。超出范围则使用默认值。

4. **`memset(&kloadinfo, 0, ...)`**：清零负载统计结构体。`kloadinfo` 用于计算 1/5/15 分钟负载平均值。

**关键观察**：`init_clock()` 只初始化**软件变量**，不触碰硬件。硬件定时器（8254 PIT / LAPIC Timer / ARM Generic Timer）的配置在 `arch_init()` 或时钟中断首次使能时完成。

### 2.2 intr_init()：x86-64 中断控制器初始化

`i8259.c:28-52`（8259A PIC 版本；L54-63 是 `intr_init` 之外的 `mask` 函数）：

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

**注意**：这是 Minix3 ARM（32 位）的实现。ARM64 通常使用 GICv3，初始化序列不同（需要初始化 Distributor、Redistributor 和 CPU Interface），但语义相同——都是配置中断路由并屏蔽所有 IRQ。

### 2.4 arch_init()：x86-64 架构特定初始化

`arch_system.c:246-279`（函数体；L281+ 是 `do_ser_debug` 函数）：

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

这些环境变量解析在 Rust 版中大部分不需要，但取舍各有不同：

1. **verboseboot**：可以用编译期日志级别控制替代 boot 参数。这只是开发阶段的简化；如果以后需要运行时调整内核日志详细程度，仍可保留一个 boot 参数来设置日志 filter。
2. **ac_layout**：地址空间布局本质上是架构/ABI 决定的，固定成编译时常量比 C 版 boot 时协商更合理。具体常量属于地址空间布局设计，不在本章展开。
3. **nr_procs/nr_tasks**：用编译期常量替代 boot 参数是**有意识的简化**，但代价是失去了 Minix3 在 boot 时调整进程表大小的灵活性。最佳实践应是：默认值编译期固定，但允许 boot-shim 通过启动信息覆盖；如果项目目标只是固定配置的 QEMU 环境，当前常量也可以接受，只是要在文档中明确。
4. **release/version**：版本号属于构建元数据，编译期确定是 idiomatic 做法，没问题。

---

## 3. Rust 设计决策

### 3.1 决策：ClockArch trait 分离硬件和软件

**Minix3 C 的做法**：`init_clock()` 混合了软件初始化（`kclockinfo` 清零 + 频率设置）和硬件初始化（隐含在后续的 `arch_init()` 中）。

**minix-rs 将时钟分为两层**：

1. **`ClockState`**（软件层）：tick 频率、定时器队列、负载统计——架构无关
2. **`ClockArch` trait**（硬件层）：配置硬件定时器、读取当前 tick——架构相关

```rust
/// 架构无关的时钟状态。
pub struct ClockState {
    hz: u32,
    uptime: u64,
    // ... 定时器队列、平均负载
}

/// 硬件定时器配置的架构抽象。
pub trait ClockArch {
    /// 配置并启动硬件定时器。
    fn init_timer(hz: u32);

    /// 读取当前 tick 计数。
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
/// 默认时钟 tick 频率（Hz）。
/// x86-64: 100 Hz（10 ms tick），aarch64: 100 Hz，riscv64: 100 Hz
/// C: DEFAULT_HZ — i386: 60 Hz，earm: 1000 Hz（32 位值）
///   路径：minix3/minix/include/arch/i386/include/archconst.h
///         minix3/minix/include/arch/earm/include/archconst.h
/// minix-rs: 统一为 100 Hz（见 §2.5 架构演进）。
/// 选择 100 Hz 的理由：10 ms tick 在响应延迟与上下文切换开销之间取得平衡，
/// 且是 Linux 服务器常见配置之一（CONFIG_HZ=100），便于与现有工具/预期对齐。
///
/// **权威定义位置**：`os/arch/src/arch/clock.rs:32`（`os/arch` crate 内的 `pub const DEFAULT_HZ: u32 = 100`）。
/// `os/kernel/src/clock.rs:148` 处的同值 `const` 是 `os/kernel` crate 内的独立副本（避免 `os/kernel` 反向依赖 `os/arch`），
/// 两处值必须保持一致。修改时**先改 `os/arch/src/arch/clock.rs:32`**，再 sync 到 `os/kernel/src/clock.rs:148`。
pub const DEFAULT_HZ: u32 = 100;
```

### 3.3 决策：把中断控制器抽象为 `InterruptController` trait

**Minix3 C 的做法**：`intr_init()` 直接操作 8259A I/O 端口、OMAP INTC MMIO、GIC 或 PLIC 寄存器。每个架构的 `intr_init()` 都是一份独立实现，上层代码通过 `hw_intr` 宏或全局变量间接访问。

**minix-rs 把"中断控制器"抽象为 `InterruptController` trait**，因为它是一台**独立于 CPU ISA 的外设**。对内核其余部分，中断控制器只需要回答五个问题：

1. 怎么初始化并关闭所有 IRQ 线？（`init` / `mask_all`）
2. 怎么屏蔽某条 IRQ 线？（`mask`）
3. 怎么允许某条 IRQ 线送达 CPU？（`unmask`）
4. 怎么告诉控制器"这条中断我已收到"？（`ack`）
5. 怎么处理完毕，可以接收下一个中断？（`eoi`）

不同架构的回答不同，但**接口完全一致**：

| 操作 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| `init()` | 初始化 LAPIC + IOAPIC | 初始化 GICv3 Distributor + Redistributor + CPU Interface | 初始化 PLIC |
| `mask()` | 设置 IOAPIC redirection entry 的 mask 位 | 写 GICD/GICR `ICENABLER` | 清 PLIC `ENABLE` 位 |
| `unmask()` | 清 IOAPIC mask 位 | 写 GICD/GICR `ISENABLER` | 置 PLIC `ENABLE` 位 |
| `ack()` / `eoi()` | 均写 LAPIC EOI（x86 APIC 无单独 ack 寄存器） | `ack` 读 `ICC_IAR1_EL1`，`eoi` 写 `ICC_EOIR1_EL1` | `ack` 读 PLIC claim，`eoi` 写 PLIC complete |

```rust
pub trait InterruptController: Sized {
    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}
```

**为什么放在 `minix-plat` 而不是 `minix-arch`？**

`minix-arch` 抽象的是 CPU ISA（分页、保护、异常入口、trap），而中断控制器是**板级外设**：同一 CPU 架构可以搭配不同中断控制器（例如 ARM64 可以是 GICv2/GICv3/GICv4，RISC-V 可以是 PLIC/APLIC）。把它放在 `minix-plat` 让"CPU 长什么样"和"主板上有哪些外设"两个维度独立变化。

**实现分布**（三架构均已实现）：

| 架构 | 实现文件 | 硬件对应 |
|------|---------|---------|
| x86-64 | `os/plat/src/x86_64/interrupt.rs` | LAPIC + IOAPIC |
| aarch64 | `os/plat/src/arm64/interrupt.rs` | GICv3 |
| riscv64 | `os/plat/src/riscv64/interrupt.rs` | PLIC |

编译期通过 `minix-plat` 的 `CurrentInterruptController` 类型别名选择当前架构的实现：

```rust
#[cfg(target_arch = "x86_64")]
pub type CurrentInterruptController = crate::x86_64::interrupt::X86_64InterruptController;
#[cfg(target_arch = "aarch64")]
pub type CurrentInterruptController = crate::arm64::interrupt::AArch64InterruptController;
#[cfg(target_arch = "riscv64")]
pub type CurrentInterruptController = crate::riscv64::interrupt::Riscv64InterruptController;
```

**与 `ArchInit` 的边界**：`InterruptController` 处理的是"中断控制器这台外设本身"；`ArchInit` 处理的是"架构杂项初始化"中需要用到中断控制器的地方。x86-64 的 APIC 既是中断控制器硬件，其初始化（LAPIC/IOAPIC 基址、timer 等）自然属于 `InterruptController::init()` 的职责，而不是作为架构杂项重复放进 `ArchInit`。

### 3.4 决策：把早期控制台抽象为 `EarlyConsole` trait

**Minix3 C 的做法**：早期调试输出散落在启动代码里。x86-64 用 COM1 I/O 端口，ARM 用 PL011 UART，RISC-V 通常用 SBI 提供的 `console_putchar`；不同架构的输出函数签名和调用方式各不相同。

**minix-rs 把"早期控制台输出"抽象为 `EarlyConsole` trait**，因为 boot 阶段需要一条**与驱动基础设施无关的最小输出通道**来打印诊断信息。对上层代码，早期控制台只需要回答四个问题：

1. 输出前需要做什么一次性硬件准备？（`init`）
2. 怎么输出一个原始字节？（`write_byte`）
3. 怎么输出一行文本（并把 `\n` 转成 `\r\n`）？（`write_str`）
4. 怎么输出一个 64 位十六进制值？（`write_hex`）

不同架构的回答不同，但接口完全一致：

| 操作 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| `write_byte()` | 写 COM1 I/O 端口 `0x3F8` | 写 PL011 MMIO `0x0900_0000` | 调用 OpenSBI `console_putchar` ecall |
| `write_str()` | 默认方法：逐字节输出，`\n` 前补 `\r` | 同上 | 同上 |
| `write_hex()` | 默认方法：`0x` 前缀 + 16 位小写十六进制 | 同上 | 同上 |

```rust
pub trait EarlyConsole {
    /// 一次性硬件初始化。对于 boot 阶段控制台已可用的架构，默认空实现。
    fn init() {}

    fn write_byte(byte: u8);

    fn write_str(s: &str) {
        for b in s.bytes() {
            if b == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(b);
        }
    }

    fn write_hex(val: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        Self::write_str("0x");
        for i in (0..16).rev() {
            Self::write_byte(HEX[((val >> (i * 4)) & 0xf) as usize]);
        }
    }
}
```

**为什么放在 `minix-plat` 而不是 `minix-arch`？**

与 `InterruptController` 类似，串口/控制台也是**板级外设**：同一 CPU 架构可以搭配不同 UART（例如 ARM64 可以是 PL011、NS16550A 或 SBSA UART），把它放在 `minix-plat` 让"CPU 长什么样"和"板子上用什么输出"两个维度独立变化。

**实现分布**（三架构均已实现）：

| 架构 | 实现文件 | 硬件对应 |
|------|---------|---------|
| x86-64 | `os/plat/src/x86_64/early_console.rs` | COM1 (UART 16550) |
| aarch64 | `os/plat/src/arm64/early_console.rs` | PL011 |
| riscv64 | `os/plat/src/riscv64/early_console.rs` | SBI `console_putchar` |

编译期通过 `minix-plat` 的 `CurrentEarlyConsole` 类型别名选择当前架构的实现：

```rust
#[cfg(target_arch = "x86_64")]
pub type CurrentEarlyConsole = crate::x86_64::early_console::X86_64EarlyConsole;
#[cfg(target_arch = "aarch64")]
pub type CurrentEarlyConsole = crate::arm64::early_console::AArch64EarlyConsole;
#[cfg(target_arch = "riscv64")]
pub type CurrentEarlyConsole = crate::riscv64::early_console::Riscv64EarlyConsole;
```

**与 `ArchInit` 的边界**：`EarlyConsole` 是**自包含的输出设备抽象**：`init()` 负责一次性硬件配置，`write_byte` / `write_str` / `write_hex` 负责输出。把 COM1 初始化放进 `EarlyConsole` 而不是 `ArchInit`，是因为串口既是早期输出设备，又是需要一次性硬件 setup 的外设；把它和输出逻辑放在同一抽象里，比拆成 `ArchInit` 里的硬件 setup 加单独输出函数更符合单一职责。x86-64 的 `EarlyConsole::init()` 配置 UART 115200 8N1 + FIFO；aarch64/riscv64 的控制台在 boot 阶段已可用，`EarlyConsole::init()` 使用默认空实现，`ArchInit` 因此不再含任何串口代码。

### 3.5 决策：ArchInit trait 封装架构特定初始化

**Minix3 C 的做法**：`arch_init()` 是一个散落着 `#ifdef` 的函数，做 TSS、串口、ACPI、APIC、PMU、内存裁剪等各种事情。

**minix-rs 用 `ArchInit` trait**：

```rust
/// 在中断控制器初始化之后执行的架构特定初始化。
///
/// `ArchInit` 采用**实例化模式**（参见 [plat-design.md §5.1](plat-design.md)）：
/// `new(desc)` 从 `ArchMiscDesc`（ACPI 表指针、PMU 使能标志等）构造实例，
/// `init(&mut self)` 执行架构特定的初始化操作。
pub trait ArchInit: Sized + Send + Sync {
    /// 从 `ArchMiscDesc` 构造实例。
    ///
    /// 把架构杂项参数（ACPI 表指针、PMU 使能标志等）存入实例字段。
    /// 上层通过 `minix_platform::platform_desc().arch_misc()` 获取描述符。
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self;

    /// 执行架构特定初始化。
    ///
    /// 由 `init_clock_and_interrupts()` 在 `init_clock()` 和 `intr_init()`
    /// 完成之后调用一次。
    fn init(&mut self);
}
```

**原因**：

1. **统一接口**：三种架构的 `arch_init()` 在**启动阶段**语义相同（完成架构特定初始化），但具体实现完全不同
2. **消除 `#ifdef`**：C 版用 `#ifdef USE_ACPI` / `#ifdef USE_APIC` 选择代码路径，Rust 用 trait 静态分派
3. **可测试性**：mock 实现可以跳过硬件初始化
4. **实例化模式**：把 `ArchMiscDesc` 一次性写入实例字段，避免每次 `init()` 调用时重复传递参数；与 §3.3 `InterruptController::new()` 和 §3.4 `EarlyConsole::new()` 保持一致（参见 [plat-design.md §5.1](plat-design.md) 实例化模式）

> **注意：ArchInit 是“阶段抽象”而非“功能抽象”**。它回答的是“除了时钟、中断和早期控制台之外，还有什么架构特定的杂项必须在此时完成”，而不是“所有架构做同一件事”。因此：
> - 凡是能抽象出跨架构一致语义的机制（如时钟节拍、中断路由、早期控制台），都应该有自己的 trait（`ClockArch`、`InterruptController`、`EarlyConsole`），不能塞进 `ArchInit`。
> - `ArchInit` 里只放那些**本身就没有跨架构一致性**的杂项（x86 的 ACPI、ARM 的 PMU/bsp_init、RISC-V 的 PMP/SIE），避免它变成无边界的“垃圾桶”。串口初始化由 `EarlyConsole::init()` 负责，不应再留在 `ArchInit`。

### 3.6 决策：arch_init 不做内存裁剪

**Minix3 C 的做法**：`arch_init()` 末尾调用 `cut_memmap()` 保留 BIOS 区域。

**minix-rs 不在 arch_init 中做内存裁剪**，原因：

1. **内存映射由 boot-shim 提供**：`KernelInfo.memmap` 已排除保留区域
2. **BIOS 区域不存在于 aarch64/riscv64**：`cut_memmap` 是 x86 特有的
3. **内存裁剪不是“架构硬件初始化”**：`cut_memmap` 修改的是系统内存图，属于内存管理/启动协议范畴，而不是配置某个架构硬件。把它放进 `ArchInit` 会把内存管理的职责泄漏到架构初始化阶段，让 `ArchInit` 变成无边界垃圾桶。这与 §3.5 的界定一致：`ArchInit` 只收留“尚未被其他 trait 抽象的架构杂项”，而非“所有架构不同的事情”。

### 3.7 决策：本阶段不实现完整的 `clock_handler`、`intr_handle` 和 `bsp_finish_booting`

**Minix3 C 的做法**：`clock_handler()`（`clock.c:281-355`）除了更新 `uptime`/`realtime`/`loadavg` 外，还更新 `bill_ptr` 和各进程的 user/sys 时间统计；`intr_handle()`（`proc.c`）做完整的中断分发；`bsp_finish_booting()`（`main.c:38-97`）设置 `kernel_may_alloc=0` 并启动 AP。

**Rust 当前做法**：本阶段只让内核具备响应时钟中断和中断控制器的硬件能力，因此：
- `ClockState::tick()` 只更新软件计数（`uptime`/`realtime`/`loadavg`），调度统计（`bill_ptr`、进程 user/sys 时间、定时器队列）留到 [11-scheduling-primitives.md](11-scheduling-primitives.md)；
- `InterruptController` 只完成初始化与 mask/unmask，完整中断分发逻辑（`intr_handle()`）留到 [14-exception-interrupt.md](14-exception-interrupt.md)；
- `bsp_finish_booting()`（AP 启动、`kernel_may_alloc=0`）留到 SMP/调度初始化文档。

这些功能依赖进程表、调度器、SMP 状态，属于后续里程碑（06-proc-init、[11-scheduling-primitives.md](11-scheduling-primitives.md)、[14-exception-interrupt.md](14-exception-interrupt.md) 以及 SMP 文档），所以不在本阶段展开。

### 3.8 架构差异对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟硬件 | 8254 PIT (I/O port 0x40-0x43) / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | RISC-V mtime (CLINT MMIO) |
| 时钟频率 | 100 Hz (可配置) | 100 Hz | 100 Hz |
| **中断控制器** | LAPIC + IOAPIC | GICv3 (GICD + GICR + CPU IF) | **PLIC** (external) + **CLINT** (timer + software) |
| IRQ 数量 | 64 (APIC mode) | 64 (software limit) / 1020 (GICv3 SPI hardware capability) | 64 (software limit) / 1024 (PLIC max) |
| `EarlyConsole::init()` | COM1 UART 配置（115200 8N1 + FIFO） | 默认空实现 | 默认空实现 |
| `EarlyConsole::write_byte()` | COM1 I/O port `0x3F8` | PL011 MMIO `0x0900_0000` | SBI `console_putchar` ecall |
| arch_init | ACPI（APIC 由 `InterruptController` 负责，串口由 `EarlyConsole` 负责） | PMU cycle counter + bsp_init | PMP + SIE |

> **注**: RISC-V "中断控制器" 应明确分为 **PLIC** (external interrupts, 由 `InterruptController` trait 管理) 和 **CLINT** (timer + software interrupts, 由 `ClockArch` 管理)。前表中"RISC-V 中断控制器"列单写"PLIC + CLINT"易混淆——`ClockArch` 用 CLINT 的 mtime/mtimecmp，`InterruptController` 只用 PLIC。

---

## 4. 实现详解

Rust 版将 C 版 `cstart()` 的后三个调用（`init_clock()`、`intr_init()`、`arch_init()`）以及早期调试输出抽象为四个 trait：`ClockArch` 负责**硬件时钟节拍**，`InterruptController` 负责**中断路由与屏蔽**，`EarlyConsole` 负责**早期控制台输出**，`ArchInit` 负责**架构杂项初始化**。拆分的依据是这四个职责在硬件层面完全独立——时钟芯片、中断控制器、串口/ACPI 是三类不同的外设，早期控制台输出又与这些初始化逻辑互不依赖。

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

**`EarlyConsole` 的抽象语义**：

> 回答"boot 阶段还没驱动时，调试信息怎么输出，以及输出前需要做什么一次性准备"。

- `init()`：一次性硬件初始化。x86-64 配置 COM1 UART（115200 8N1 + FIFO）；aarch64/riscv64 的控制台在 boot 阶段已可用，使用默认空实现。
- `write_byte(byte)`：向早期控制台输出一个原始字节。x86-64 写 COM1 I/O 端口，aarch64 写 PL011 MMIO，riscv64 通过 SBI ecall。
- `write_str(s)`：输出字符串，默认实现把 `\n` 翻译为 `\r\n`，避免不同终端显示混乱。
- `write_hex(val)`：输出 `0x` 前缀的 64 位十六进制，默认实现复用 `write_byte`，三架构共享同一份格式化逻辑。

`EarlyConsole` 现在是**自包含的输出设备抽象**：`init()` 负责一次性硬件 setup，`write_*` 负责输出。x86-64 的 COM1 初始化因此由 `X86_64EarlyConsole::init()` 完成，`ArchInit` 不再含任何串口相关代码。

**`ArchInit` 的抽象语义**：

> 回答"除了时钟、中断和早期控制台输出，还有什么架构特定的杂项必须在这个阶段完成"。

- `init()`：执行架构特定的杂项初始化。x86-64 包括 ACPI（电源管理表）；aarch64 包括 PMU（性能监控单元）、`bsp_init`；riscv64 包括 PMP（物理内存保护）、S-mode 中断使能。这些初始化彼此无关，但都是启动的必要步骤。串口初始化由 `EarlyConsole::init()` 负责，不属于 `ArchInit`。
- **边界**：`ArchInit` 是**阶段 trait**，不是**功能 trait**。它只收留那些尚未、也不宜抽象为跨架构一致接口的杂项；像 APIC、GIC、PLIC 这类有明确跨架构语义的中断控制器逻辑已经在 `InterruptController::init()` 中处理，像 COM1/PL011/SBI 这类早期控制台已经在 `EarlyConsole::init()` 中处理，不应再放进 `ArchInit`。

三个初始化 trait 的调用顺序由 `init_clock_and_interrupts()` 保证：`ClockArch::init_timer()` → `InterruptController::init()` → `ArchInit::init()`。`EarlyConsole::init()` 在 `kmain()` 入口（或测试内核的 `kmain_verify()`）最先调用，确保后续诊断输出使用正确的 UART 配置；它不属于 `init_clock_and_interrupts()` 的三步序列，但同样只执行一次。其中：

- `ClockArch::init_timer()` 与 `InterruptController::init()` **没有强硬件依赖**，交换顺序不会导致错误。x86-64 的 PIT、ARM 的 Generic Timer、RISC-V 的 CLINT 都与中断控制器是独立外设；本阶段尚未开中断，即使定时器先配好也不会触发中断。
- `ArchInit::init()` 必须在最后，因为它可能依赖时钟和中断控制器已就绪（例如 x86-64 的 `ArchInit` 需要知道 `system_hz`，或若后续把 APIC timer 配置纳入此阶段，也需要它已在时钟初始化之后）。

这个顺序与 Minix3 `cstart()` 的 `init_clock → intr_init → arch_init` 保持一致，但 Rust 版的拆分让每一步的职责比 C 版更清晰。

### 4.1 ClockState：架构无关的时钟状态

> 设计决策：§3.1（分离硬件和软件）、§3.2（编译时常量）

```rust
/// 负载历史采样槽数量，用于计算平均负载。
/// C: _LOAD_HISTORY — include/minix/type.h:97
pub const LOAD_HISTORY_SIZE: usize = 16;

/// 架构无关的时钟状态。
///
/// 管理 tick 频率、运行时间计数、实时时间跟踪和平均负载。
/// 硬件定时器配置委托给 `ClockArch`。
///
/// C: kclockinfo + kloadinfo + clock_timers — clock.c:33-44
pub struct ClockState {
    /// tick 频率，单位 Hz。
    /// C: kclockinfo.hz — type.h:119
    hz: u32,

    /// 系统启动以来的运行时间，单位 tick。
    /// C: kclockinfo.uptime — type.h:107
    uptime: u64,

    /// 系统启动以来的实时时间，单位 tick（可能因 adjtime 与 uptime 不同）。
    /// C: kclockinfo.realtime — type.h:109
    realtime: u64,

    /// 启动时间，UNIX 纪元以来的秒数。
    /// C: kclockinfo.boottime — type.h:105
    boottime: u64,

    /// 调整 realtime 的 tick 数（正数加速，负数减速）。
    /// C: adjtime_delta — clock.c:44
    adjtime_delta: i32,

    /// 平均负载跟踪数据。
    /// C: kloadinfo (struct loadinfo) — type.h:98
    loadinfo: LoadInfo,
}

/// 平均负载跟踪数据。
///
/// 跟踪可运行进程数随时间的变化，用于计算 1/5/15 分钟平均负载。
///
/// C: struct loadinfo — include/minix/type.h:98
struct LoadInfo {
    /// 每个采样槽中的进程数历史。
    /// C: proc_load_history[_LOAD_HISTORY] — type.h:99
    proc_load_history: [u16; LOAD_HISTORY_SIZE],

    /// `proc_load_history` 中最后写入的槽位。
    /// C: proc_last_slot — type.h:100
    proc_last_slot: u16,

    /// 上次采样时的 uptime。
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

    /// 每个时钟 tick 调用。
    /// C: timer_int_handler() — clock.c:70
    pub fn tick(&mut self) {
        self.uptime += 1;

        // 根据 adjtime_delta 调整 realtime。
        // C: clock.c:92-103
        if self.adjtime_delta != 0 && self.uptime & 0x1 != 0 {
            self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
            self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
        } else {
            self.realtime += 1;
        }

        // 平均负载更新与定时器队列到期检查属于运行时 tick 处理逻辑，
        // 不在初始化阶段展开。详见 [15-clock-timer.md §4.3]。
        // C: load_update() — clock.c:260-291
        // C: tmrs_exptimers(&clock_timers) — clock.c:160-161
    }
}
```

### 4.2 ClockArch trait

> 设计决策：§3.1（分离硬件和软件）

```rust
/// 硬件定时器配置的架构抽象。
///
/// 每个架构实现此 trait 以配置其硬件
/// 定时器源并提供读 tick 能力。
///
/// | 架构 | 定时器源 | 频率寄存器 | 计数寄存器 |
/// |------|---------|-----------|-----------|
/// | x86-64 | 8254 PIT / LAPIC Timer | PIT divisor | LAPIC CCR |
/// | aarch64 | Generic Timer | CNTFRQ_EL0 | CNTPCT_EL0 |
/// | riscv64 | CLINT mtime | mtimecmp | mtime |
pub trait ClockArch {
    /// 按给定频率配置并启动硬件定时器。
    ///
    /// 在 `init_clock_and_interrupts()` 中调用一次。调用后定时器以 `hz` Hz
    /// 产生周期性中断。
    ///
    /// C: init_clock() 硬件部分 + arch_init() APIC timer
    fn init_timer(hz: u32);

    /// 读取当前硬件 tick 计数。
    ///
    /// 用于精细计时和性能分析。
    fn read_ticks() -> u64;
}
```

### 4.3 x86-64 ClockArch 实现

```rust
/// x86-64 时钟，boot 阶段用 8254 PIT，运行时用 LAPIC Timer。
///
/// C: clock.c 硬件初始化 + apic.c lapic_enable()
pub struct X86_64ClockArch;

/// 8254 PIT 基准频率，单位 Hz。
const PIT_BASE_FREQ: u32 = 1_193_182;
/// PIT 命令端口。
const PIT_COMMAND: u16 = 0x43;
/// PIT channel 0 数据端口。
const PIT_CHANNEL0: u16 = 0x40;
/// PIT 命令：channel 0，低/高字节访问，速率发生器（rate generator）模式。
const PIT_CMD_RATE_GEN: u8 = 0x36;

impl ClockArch for X86_64ClockArch {
    fn init_timer(hz: u32) {
        // 把 8254 PIT channel 0 配置为周期性模式。
        // C: intr_init_8254() — i8259.c 等价实现
        //
        // PIT divisor 是 16 位，因此 hz 必须 >= 19 (1193182 / 65535 ≈ 18.2)。
        // 低于 19 会导致 divisor 溢出。
        assert!(hz >= 19, "PIT divisor overflow: hz must be >= 19, got {}", hz);

        let divisor = (PIT_BASE_FREQ / hz) as u16;

        unsafe {
            // 发送命令字节：channel 0，低/高字节，速率发生器（rate generator）
            core::arch::asm!("out dx, al", in("dx") PIT_COMMAND, in("al") PIT_CMD_RATE_GEN);
            // 发送 divisor 低字节
            let lo = divisor as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") lo);
            // 发送 divisor 高字节
            let hi = (divisor >> 8) as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") hi);
        }
    }

    fn read_ticks() -> u64 {
        // 用 TSC（Time Stamp Counter）做高分辨率 tick 读取。
        // C: read_tsc() — Minix3 中没有，但是 x86-64 标准做法
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
/// ARM64 时钟，使用 Generic Timer。
///
/// C: earm/arch_system.c PMU 初始化（用户态 cycle counter）
pub struct AArch64ClockArch;

impl ClockArch for AArch64ClockArch {
    fn init_timer(hz: u32) {
        // ARM Generic Timer 由固件（TF-A/U-Boot）配置好。
        // 这里只需使能 EL1 physical timer 并设置比较值。
        let freq: u64;
        unsafe {
            asm!("mrs {}, cntfrq_el0", out(reg) freq);
        }
        // ARM Generic Timer 是 absolute compare 模式（CNTP_CVAL_EL0
        // 是绝对值，不是 delta）。读当前 count，再加上 freq/hz 作为
        // 下次触发点。CNTP_CTL_EL0 bit 0 = enable, bit 1 = IMASK（屏蔽）。
        let now: u64;
        unsafe {
            asm!("mrs {}, cntpct_el0", out(reg) now);
        }
        let compare = now + freq / hz as u64;
        unsafe {
            // 设置绝对比较值
            asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // 使能定时器（ENABLE=1, IMASK=0, ISTATUS=0）
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
/// RISC-V 64 位时钟，使用 CLINT mtime。
///
/// C: 无 Minix3 对应实现（Minix3 没有 RISC-V 端口）。

/// QEMU virt 机器的 CLINT mtime 寄存器地址。
/// 当前硬编码；支持正式硬件时需从设备树获取。
const CLINT_MTIME: usize = 0x200_BFF8;

/// QEMU virt 机器的 CLINT mtimecmp 寄存器地址（hart 0）。
const CLINT_MTIMECMP: usize = 0x200_4000;

/// QEMU virt 机器的 CLINT mtime 频率（10 MHz）。
/// 当前硬编码；支持正式硬件时需从设备树获取。
const MTIME_FREQ: u64 = 10_000_000;

pub struct Riscv64ClockArch;

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        // 读取当前 mtime 值
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(CLINT_MTIME as *const u64);
        }

        // 计算两次中断之间的间隔
        let interval = MTIME_FREQ / hz as u64;

        // 设置 mtimecmp = mtime + interval，安排第一次中断
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(CLINT_MTIMECMP as *mut u64, mtimecmp);
        }

        // 使能 S-mode 定时器中断（sie 中的 STIE 位）
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
/// GICv3 Distributor 基地址偏移（相对于 GIC 基址）。
/// QEMU virt: GICD 位于 0x08000000
const GICD_OFFSET: usize = 0x0000_0000;

/// GICv3 Redistributor 基地址偏移（相对于 GIC 基址）。
/// QEMU virt: GICR 位于 0x080A0000
const GICR_OFFSET: usize = 0x000A_0000;

/// GICD_CTLR：Distributor 控制寄存器。
const GICD_CTLR: usize = 0x0000;
/// GICD_CTLR.EnableGrp1NS 位。
const GICD_CTLR_ENABLE_GRP1NS: u32 = 0x2;
/// GICD_ISENABLER<n>：中断置能寄存器。
const GICD_ISENABLER: usize = 0x0100;
/// GICD_ICENABLER<n>：中断禁用寄存器。
const GICD_ICENABLER: usize = 0x0180;
/// GICD_IGROUPR<n>：中断分组寄存器。
const GICD_IGROUPR: usize = 0x0080;
/// GICR_ISENABLER0：Redistributor 中断置能寄存器（SGI+PPI，INTID 0-31）。
const GICR_ISENABLER0: usize = 0x0100;
/// GICR_ICENABLER0：Redistributor 中断禁用寄存器（SGI+PPI，INTID 0-31）。
const GICR_ICENABLER0: usize = 0x0180;
/// GICR_WAKER：Redistributor 唤醒寄存器。
const GICR_WAKER: usize = 0x0014;
/// GICR_WAKER.ProcessorSleep 位。
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 0x2;
/// GICR_WAKER.ChildrenAsleep 位（只读）。
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 中断控制器。
pub struct AArch64InterruptController {
    gicd_base: usize,
    gicr_base: usize,
    nr_irqs: usize,
    /// 上次应答的中断 ID（从 ICC_IAR1_EL1 读取保存）。
    last_iar: u32,
}

impl AArch64InterruptController {
    /// 构造一个基地址未初始化的控制器。
    /// 必须在 `init()` 前调用 `set_base()`，否则 `assert_ne!()` 会 panic。
    pub const fn new() -> Self {
        Self {
            gicd_base: 0,
            gicr_base: 0,
            nr_irqs: NR_IRQ_VECTORS,
            last_iar: 0,
        }
    }

    /// 从设备树/平台发现设置 GIC 基地址。
    pub fn set_base(&mut self, gicd_base: usize, gicr_base: usize) {
        self.gicd_base = gicd_base;
        self.gicr_base = gicr_base;
    }
}

impl InterruptController for AArch64InterruptController {
    fn init(&mut self) {
        // 尽早拦截“忘记 set_base()”的错误。
        assert!(self.gicd_base != 0, "...: gicd_base not set; call set_base() before init()");
        assert!(self.gicr_base != 0, "...: gicr_base not set; call set_base() before init()");
        self.init_distributor();
        self.init_redistributor();
        self.init_cpu_interface();
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num < 32 {
            // PPI/SGI：由 Redistributor 处理（GICR_ICENABLER0）
            // GICv3 规范：GICR_ICENABLER0 的第 N 位控制本 CPU 的 INTID N。
            unsafe { self.gicr_write32(GICR_ICENABLER0, bit); }
        } else {
            // SPI：由 Distributor 处理
            let reg = (irq_num / 32) as usize;
            unsafe { self.gicd_write32(GICD_ICENABLER + reg * 4, bit); }
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num < 32 {
            // PPI/SGI：由 Redistributor 处理（GICR_ISENABLER0）
            // GICv3 规范：GICR_ISENABLER0 的第 N 位控制本 CPU 的 INTID N。
            unsafe { self.gicr_write32(GICR_ISENABLER0, bit); }
        } else {
            let reg = (irq_num / 32) as usize;
            unsafe { self.gicd_write32(GICD_ISENABLER + reg * 4, bit); }
        }
    }

    fn ack(&mut self, _irq: IrqVector) {
        // 读取 ICC_IAR1_EL1 应答最高优先级的待处理中断。
        let iar: u64;
        unsafe { core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) iar); }
        self.last_iar = (iar as u32) & 0x00FF_FFFF;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // 把从 IAR 读到的中断 ID 写回。
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

> **实现说明**: GICv3 SPI 路径（IRQ ≥ 32）和 PPI/SGI 路径（IRQ < 32）均已实现。PPI/SGI 通过 Redistributor 的 `GICR_ISENABLER0`/`GICR_ICENABLER0` 寄存器控制（`os/plat/src/arm64/interrupt.rs`）。Boot-stage 只需 SPI，PPI 支持为后续中断处理阶段准备。

### 4.6.1 init_distributor / init_redistributor / init_cpu_interface 细节

```rust
fn init_distributor(&mut self) {
    unsafe {
        // 1. 把所有 SPI 分配到 Group 1（Non-secure）。
        //    循环从 32 开始，因为 SGI/PPI（0..32）是 per-CPU 的，
        //    由 Redistributor 管理，而不是 Distributor。
        //    当 nr_irqs = NR_IRQ_VECTORS = 64 时，这里处理 SPI 32..63。
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
        }
        // 2. 禁用所有 SPI
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
        }
        // 3. 使能 Distributor（Group 1 Non-secure）
        self.gicd_write32(GICD_CTLR, GICD_CTLR_ENABLE_GRP1NS);
    }
}

fn init_redistributor(&mut self) {
    unsafe {
        // 唤醒 Redistributor
        self.gicr_write32(GICR_WAKER, 0);
        // 等待 ChildrenAsleep 被清除
        while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
            core::hint::spin_loop();
        }
    }
}

fn init_cpu_interface(&mut self) {
    unsafe {
        // 使能系统寄存器接口（ICC_SRE_EL1）
        let mut sre: u64;
        core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
        sre |= 0x7; // SRE 使能 + 系统寄存器接口使能
        core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);
        // 把优先级掩码设为最低（接受所有中断）
        core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);
        // 使能 Group 1 Non-secure 中断
        core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
    }
}
```

### 4.7 riscv64 InterruptController 实现（PLIC）

```rust
/// QEMU virt 机器的 PLIC 基地址。
/// 当前硬编码；支持正式硬件时需从设备树获取。
const PLIC_BASE: usize = 0x0C00_0000;

/// PLIC 寄存器偏移。
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_PENDING: usize = 0x1000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
const PLIC_COMPLETE: usize = 0x200004;

/// hart 0 的 S-mode context 偏移。
/// Context 0 = M-mode，Context 1 = S-mode（QEMU virt）。
const S_MODE_CONTEXT: usize = 1;

/// RISC-V 64 位 PLIC 中断控制器。
pub struct Riscv64InterruptController {
    plic_base: usize,
    nr_irqs: usize,
    /// 当前 hart 的 S-mode context ID。
    context: usize,
    /// 上次 claim 的中断 ID（从 claim 寄存器读取保存）。
    last_claimed: u32,
}

impl Riscv64InterruptController {
    pub const fn new() -> Self {
        Self {
            plic_base: PLIC_BASE,  // QEMU virt 默认值；可通过 set_base() 覆盖
            nr_irqs: NR_IRQ_VECTORS,
            context: S_MODE_CONTEXT,
            last_claimed: 0,
        }
    }

    /// 从设备树/平台发现设置 PLIC 基地址。
    pub fn set_base(&mut self, plic_base: usize) {
        self.plic_base = plic_base;
    }
}

impl InterruptController for Riscv64InterruptController {
    fn init(&mut self) {
        // PLIC 初始化序列：
        // 1. 把所有中断优先级设为 1（最低有效优先级）
        // 2. 禁用所有中断（enable = 0）
        // 3. 把阈值设为 0（接受所有优先级）
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
        if irq_num == 0 { return; } // PLIC 中没有 IRQ 0
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
        // 读取 claim 寄存器应答最高优先级的待处理中断，
        // 返回中断 ID。
        let claimed: u32;
        unsafe { claimed = self.plic_read32(PLIC_CLAIM + self.context * 0x1000); }
        self.last_claimed = claimed;
    }

    fn eoi(&mut self, _irq: IrqVector) {
        // 把中断 ID 写回 complete 寄存器。
        unsafe { self.plic_write32(PLIC_COMPLETE + self.context * 0x1000, self.last_claimed); }
    }

    fn mask_all(&mut self) {
        for word in 0..(self.nr_irqs + 31) / 32 {
            unsafe { self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0); }
        }
    }
}
```

> **实现说明**: PLIC 已实现，base 默认 QEMU virt 地址 0x0C00_0000。Timer 中断由 CLINT 处理（见 `clock.rs`），不在 PLIC 路径。

### 4.8 ArchInit trait

> 设计决策：§3.5（ArchInit trait）、§3.6（不做内存裁剪）

```rust
/// 在中断控制器初始化之后执行的架构特定初始化。
///
/// 这个 trait 封装了 Minix3 C 的 `arch_init()` 函数，
/// 负责在保护结构和中断控制器初始化完成后进行硬件相关设置。
///
/// C: arch_init() — arch/i386/arch_system.c:246 / earm/arch_system.c:101
pub trait ArchInit {
    /// 执行架构特定初始化。
    ///
    /// 在 `init_clock_and_interrupts()` 中调用一次，位于 `init_clock()` 和
    /// `intr_init()` 完成之后。
    fn init();
}
```

### 4.9 x86-64 ArchInit 实现

```rust
pub struct X86_64ArchInit;

impl ArchInit for X86_64ArchInit {
    fn init() {
        // 1. 每 CPU 内核栈（由链接脚本处理）

        // 2. ACPI 表解析
        //    QEMU virt 启动阶段暂不依赖；支持物理机时需实现 RSDP 搜索与表解析。

        // APIC 初始化由 X86_64InterruptController::init() 完成。
        // COM1 串口初始化由 X86_64EarlyConsole::init() 完成。
    }
}
```

### 4.10 aarch64 ArchInit 实现

```rust
pub struct AArch64ArchInit;

impl ArchInit for AArch64ArchInit {
    fn init() {
        // C: arch_init() — earm/arch_system.c:101-132

        // 1. 使用户态可以访问 PMU cycle counter
        // C: PMU_PMCR_E + PMU_PMCNTENSET_C + PMU_PMUSERENR_EN
        unsafe {
            // PMCR：E（使能）+ C（复位事件计数器）+ P（复位 cycle counter）= 0x7
            core::arch::asm!("msr pmcr_el0, {}", in(reg) 0x7u64);
            // PMCNTENSET：C 位（bit 31）使能 cycle counter
            core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);
            // PMUSERENR：EN 位（bit 0）— 允许 EL0（用户态）访问
            core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
        }

        // 2. 板级相关初始化（bsp_init）
        //    平台相关设置（例如 GIC 基地址发现）在正式硬件上需补充。
        //    当前 QEMU virt 中，GIC base 由 `AArch64InterruptController::set_base()`
        //    单独传入，不经过此路径。
    }
}
```

> **实现说明**: PMU 三个寄存器 (PMCR_EL0 / PMCNTENSET_EL0 / PMUSERENR_EL0) 已实现；`bsp_init` 中的 GIC base 发现由 `AArch64InterruptController::set_base()` 单独处理，不在 `ArchInit::init()` 路径内。

### 4.11 riscv64 ArchInit 实现

```rust
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn init() {
        // 无 Minix3 对应实现 —— 来自 RISC-V Privileged Spec

        // 1. 配置 PMP（Physical Memory Protection）
        // pmpaddr0 = u64::MAX（通过 NAPOT 编码匹配所有地址）
        unsafe {
            core::arch::asm!("csrw pmpaddr0, {}", in(reg) u64::MAX);
            // pmpcfg0 = A=NAPOT（0x18）+ X+R+W（0x7）= 0x1F
            // A=NAPOT 表示用 NAPOT 编码匹配地址，X+R+W 表示可执行/可读/可写。
            // 合在一起允许对所有内存区域的访问。
            core::arch::asm!("csrw pmpcfg0, {}", in(reg) 0x1Fu64);
        }

        // 2. 使能 S-mode 中断
        // 设置的 SIE 位：STIE（bit 5）+ SSIE（bit 1）= 0x22
        // （不设置 SEIE bit 9 —— 外部中断源在中断控制器中单独配置时处理）
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x22u64);
        }
    }
}
```

> **实现说明**: PMP entry 0 已实现（pmpaddr0 + pmpcfg0），SIE 0x22 设置 STIE+SSIE。SEIE 由 InterruptController::unmask() 在使能 PLIC external 中断时单独处理。

### 4.12 init_clock_and_interrupts() 实现

> 设计决策：§3.1~§3.6、§4.1（实例化 trait）

```rust
/// cstart 的第二阶段：初始化时钟和中断控制器。
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
fn init_clock_and_interrupts() {
    use minix_arch::{ClockArch, ArchInit};
    use minix_platform::{platform_desc, PlatformDesc};

    // 平台描述符在 Phase A.5 已由 KernelInfo 初始化。
    // 所有硬件参数（定时器、中断控制器、架构杂项）均从此获取。
    let pd = platform_desc();

    // 步骤 1：初始化时钟状态（软件部分）。
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // 不需要 env_get("hz") —— DEFAULT_HZ 是编译时常量。

    // 步骤 2：初始化硬件定时器。
    // C: init_clock() 硬件部分 + arch_init() APIC timer
    // 实例化设计：从 timer 描述符构造 ClockArch，再调用 init_timer。
    let mut clock_arch = CurrentClockArch::new(&pd.timer());
    clock_arch.init_timer(clock.hz());

    // 步骤 3：初始化中断控制器。
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:22
    // 实例化设计：从 interrupt_controller 描述符构造 InterruptController，再调用 init。
    let mut intr = CurrentInterruptController::new(&pd.interrupt_controller());
    intr.init();  // 内部会调用 mask_all()

    // 步骤 4：架构特定初始化。
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    // 实例化设计：从 arch_misc 描述符构造 ArchInit，再调用 init。
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();

    let _ = clock; // `hz()` 已消费，后续接入时钟子系统
}
```

### 4.13 EarlyConsole 实现（x86-64）

> 设计决策：§3.4

`EarlyConsole` trait 定义在 `os/plat/src/early_console.rs`；x86-64 实现位于 `os/plat/src/x86_64/early_console.rs`。

```rust
pub trait EarlyConsole {
    /// 一次性硬件初始化。默认空实现。
    fn init() {}

    fn write_byte(byte: u8);

    fn write_str(s: &str) {
        for b in s.bytes() {
            if b == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(b);
        }
    }

    fn write_hex(val: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        Self::write_str("0x");
        for i in (0..16).rev() {
            Self::write_byte(HEX[((val >> (i * 4)) & 0xf) as usize]);
        }
    }
}

// os/plat/src/x86_64/early_console.rs
pub const COM1_BASE: u16 = 0x3F8;
const COM1_DIVISOR_115200: u8 = 0x01;
const COM1_LCR_DLAB: u8 = 0x80;
const COM1_LCR_8N1: u8 = 0x03;
const COM1_FCR_ENABLE: u8 = 0xC7;
const COM1_MCR_DTR_RTS_OUT2: u8 = 0x0B;

unsafe fn ser_init() {
    outb(COM1_BASE + 1, 0x00);          // 禁用 UART 中断
    outb(COM1_BASE + 3, COM1_LCR_DLAB); // 使能 divisor latch 访问
    outb(COM1_BASE + 0, COM1_DIVISOR_115200); // Divisor 低字节 = 1
    outb(COM1_BASE + 1, 0x00);          // Divisor 高字节 = 0
    outb(COM1_BASE + 3, COM1_LCR_8N1);  // 8N1，关闭 DLAB
    outb(COM1_BASE + 2, COM1_FCR_ENABLE); // 使能 FIFO
    outb(COM1_BASE + 4, COM1_MCR_DTR_RTS_OUT2); // DTR + RTS + OUT2
}

pub struct X86_64EarlyConsole;

impl EarlyConsole for X86_64EarlyConsole {
    fn init() {
        // SAFETY：只在 BSP 上、串口 I/O 使能前运行。
        unsafe { ser_init(); }
    }

    fn write_byte(byte: u8) {
        unsafe {
            while (inb(COM1_BASE + 5) & 0x20) == 0 {}
            outb(COM1_BASE, byte);
        }
    }
}
```

`kmain()` 和 `kmain_verify()` 在入口首先调用 `CurrentEarlyConsole::init()`，随后所有诊断输出都走 `CurrentEarlyConsole::write_*`。

---

## 5. 测试要点

测试分为两层：**单元测试**（验证软件状态机逻辑，无硬件依赖）和 **QEMU 集成测试**（验证硬件寄存器配置，需 QEMU + GDB）。

### 5.1 单元测试

单元测试位于多个 `#[cfg(test)]` 模块中，按文件分布如下：

| 文件 | 测试数 | 验证内容 |
|------|-------|---------|
| `os/arch/src/arch/clock.rs` | 12 | `ClockState` / `LoadInfo` / `DEFAULT_HZ` / `LOAD_HISTORY_SIZE` |
| `os/plat/src/interrupt.rs` | 10 | `IrqVector` / `IrqId` / `IrqNotifyId` / `IrqPolicy` / `IrqAction` / `NR_IRQ_*` |
| `os/plat/src/x86_64/interrupt.rs` | 3 | LAPIC/IOAPIC 默认基址、set_base 覆盖、`IRQ0_VECTOR` |
| `os/plat/src/x86_64/early_console.rs` | 6 | COM1 寄存器常量 (DLAB, 8N1, FIFO, MCR) |
| `os/arch/src/x86_64/arch_init.rs` | 0 | 串口初始化由 `EarlyConsole` 负责；ACPI 解析尚未实现测试 |
| `os/plat/src/arm64/interrupt.rs` | 3 | GIC 寄存器偏移、WAKER bits、QEMU virt GICD/GICR 偏移、gicd_base=0 防御 panic |
| `os/arch/src/{x86_64,arm64,riscv64}/trap_entry.rs` | 15/3/3 | IDT 门描述符、set_handler 行为、VBAR/stvec 配置（详见 doc 03 §5.3）|
| `os/plat/src/early_console.rs` | 0 | trait 声明 `init`/`write_byte` 接口；`write_str`/`write_hex` 为默认方法，目前通过 `os/plat/src/mock.rs` 的 `MockEarlyConsole` 在测试构建中复用 |
| **当前所列文件总计** | **55** | |

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
| `test_read_tsc_default_delegates_to_read_ticks` | `ClockArch::read_tsc` 默认实现委托 `read_ticks` |
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
| x86-64 APIC 构造 | `X86_64InterruptController::new(&InterruptControllerDesc::Apic)` 提取 lapic/ioapic base | ✅ 2 tests (`os/plat/src/x86_64/interrupt.rs`) |
| aarch64 GICv3 构造 | `AArch64InterruptController::new(&InterruptControllerDesc::Gicv3)` 提取 distributor/cpuif base | ✅ 1 test (`os/plat/src/arm64/interrupt.rs`) |
| riscv64 PLIC 构造 | `Riscv64InterruptController::new(&InterruptControllerDesc::Plic)` 提取 base | ✅ 1 test (`os/plat/src/riscv64/interrupt.rs`) |
| Mock 中断控制器 | `MockInterruptController` 提供 `InterruptController` trait 空实现，供单元测试编译 | ✅ (`os/plat/src/mock.rs`) |
| QEMU x86-64 | PIT 配置, 断点命中 | ✅ 自动化脚本 |
| QEMU aarch64 | Generic Timer, GICv3 寄存器 | ✅ 自动化脚本 |
| QEMU riscv64 | CLINT mtimecmp, sie 寄存器 | ✅ 自动化脚本 |

**未覆盖（后续文档）**：
- Timer queue（`clock_timers`）测试 — 属于定时器超时管理模块
- Load average 更新（`kloadinfo`）测试 — 属于调度器模块
- 时钟中断处理程序（`timer_int_handler`）测试 — 属于异常/中断处理文档

---

## 6. 参见

- [03-kmain-cstart.md](03-kmain-cstart.md) — cstart 前半段：保护模式初始化
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 进程表初始化和 boot 进程加载
- [16-smp.md](16-smp.md) — SMP 启动和 AP 引导（与 §3.7 `bsp_finish_booting` 范围声明呼应）
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/plat/src/interrupt.rs` — `InterruptController` trait 定义
- `os/plat/src/x86_64/interrupt.rs` — x86-64 APIC 实现
- `os/plat/src/arm64/interrupt.rs` — aarch64 GICv3 实现
- `os/plat/src/riscv64/interrupt.rs` — riscv64 PLIC 实现
- `os/kernel/src/clock.rs` — ClockState 实现
