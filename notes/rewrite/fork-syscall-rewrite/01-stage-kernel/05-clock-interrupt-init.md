# 05-clock-interrupt-init: 时钟与中断初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/clock.c:48-74`, `minix3/minix/kernel/arch/i386/i8259.c:28-63`, `minix3/minix/kernel/arch/i386/arch_system.c:246-288`, `minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:22-44`, `minix3/minix/kernel/arch/earm/arch_system.c:101-132`
> **说明**: cstart() 的后半段——init_clock + intr_init + arch_init，让内核能响应硬件事件
> **前置**: [03-kmain-cstart.md](03-kmain-cstart.md) — 保护模式已初始化

---

## 1. 概述

### 1.0 中断模型：同步 vs 异步，以及为什么内核必须"启用"中断

本章聚焦**时钟与中断控制器的初始化**，并连带讨论同一启动阶段出现的**早期控制台输出**问题：解释 cstart() 后半段三个子系统——**时钟子系统**（由 `init_clock()` 建立）、**中断控制器子系统**（由 `intr_init(0)` 建立）、**架构通用子系统**（由 `arch_init()` 建立）——所回答的三个根本问题：操作系统如何获得可量化的时间粒度、如何让设备异步通知 CPU、以及还有哪些架构特定的硬件必须在此阶段就绪。后续章节再说明 Rust 版如何把这些职责拆分为独立的抽象。具体中断/异常 handler、调度与时钟的耦合、SMP/AP 启动、设备驱动的 IRQ 路由等主题留到后续文档。

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

**时钟中断的哲学地位**：时钟中断是操作系统中唯一"一定会来"的中断。键盘可以一直不按，网卡可以没有数据——但时钟到点就会触发一次。它定义了 OS 的时间粒度：调度器的决策周期、系统调用的超时精度、`sleep()` 的实际分辨率都以 tick 长度为粒度，指定更细的时间通常也会被向上取整到下一个 tick。具体的 tick 频率是策略值，Minix3 C 在不同架构上选择不同默认：x86 每 ~16.7 ms 一次（DEFAULT_HZ=60，源自 IBM PC 8254 PIT 的 1.193 MHz 与分频器，`include/arch/i386/include/archconst.h:4`），ARM 每 1 ms 一次（DEFAULT_HZ=1000，`include/arch/earm/include/archconst.h:4`）；minix-rs 统一为 100 Hz（10 ms tick，详见 §2.5 架构演进）。时钟中断初始化是 boot 的最后一步证明——**从这一刻起，内核不再是被动等待事件，而是主动驱动事件**。

### 1.1 为什么时钟和中断必须在 proc_init 之前

03 文档结束时，内核已建立了保护模式基础设施（GDT/IDT 或 VBAR_EL1/stvec），但内核仍然无法响应任何硬件事件——中断控制器未初始化，时钟未启动。

`proc_init()` 和 `arch_boot_proc()` 需要时钟和中断的原因：

1. **进程调度依赖时钟**：Minix3 的调度器基于时间片（quantum），由时钟中断驱动。虽然 boot 阶段不调度，但时钟中断处理程序 `timer_int_handler()` 会更新 `bill_ptr` 和进程的 user/sys 时间统计。
2. **init_clock 只初始化软件变量**：它设置 `kclockinfo.hz`（即 `system_hz`）、清零负载统计，**不触碰硬件定时器**，也不会启用中断。因此它不需要中断控制器先准备好；真正的硬件定时器使能发生在更晚的 `bsp_finish_booting()` 中。
3. **arch_init 通过它所触发的 APIC 子路径强依赖 `system_hz`，并假设中断控制器已 ready**：x86-64 的 `arch_init()` 直接调用 `apic_single_cpu_init()`（`arch/i386/arch_system.c:268`），后者在 `apic.c` 内多处使用 `system_hz` 作为 LAPIC timer 分频与 `cpu_freq` 推导的基数（`arch/i386/apic.c:169,491,517,520,578`）；同时 APIC 寄存器寻址隐含 LAPIC 寄存器页已被识别这一前提，要求 `intr_init` 至少已运行过以建立中断控制器子系统的稳定状态——但这并不等于 IRQs 必须 unmask：`arch/i386/i8259.c:55-58` 在 `intr_init` 结束时把所有非级联 IRQ 都 mask 起来，`arch_init` 在这种全屏蔽状态下依然可以正常运行。

因此，Minix3 的 `cstart()` 调用顺序是：**prot_init → init_clock → intr_init → arch_init**。这个顺序是源码固定的，但 `init_clock` 与 `intr_init` 之间并没有强硬件依赖——交换它们不会导致错误；真正不能颠倒的是 `arch_init` 必须在 `intr_init` 之后，因为它依赖已配置好的中断控制器。

### 1.2 三个函数的职责

| 函数 | 做什么 | 依赖 |
|------|--------|------|
| `init_clock()` | 初始化时钟变量：tick 频率、定时器队列、负载统计 | prot_init（IDT 已加载，时钟中断有入口） |
| `intr_init(0)` | 初始化中断控制器：8259A（PIC）/APIC/GIC/PLIC，mask 所有 IRQ | prot_init（IDT/VBAR/stvec 已加载） |
| `arch_init()` | 架构特定初始化：栈分配、APIC（本地高级可编程中断控制器）、ACPI（硬件配置/电源管理表）、PMU（性能监控单元）等架构相关设置 | init_clock（提供 `system_hz`）+ intr_init（中断控制器已初始化） |

> **注意**：这里的"tick 频率"不是 CPU 主频，而是 OS 希望定时器每秒产生多少次 tick。Minix3 C 在不同架构上选择不同默认值（x86 默认 60 Hz、ARM 默认 1000 Hz，来源同上）；minix-rs 统一为 100 Hz（详见 §2.5）。硬件定时器本身的输入时钟频率（如 x86 PIT 的 1.193 MHz、ARM Generic Timer 的 CNTFRQ、RISC-V `mtime`（CLINT MMIO））由架构代码在运行时通过 CPUID/设备树/固件获取，再据此计算分频器；HZ 只是一个策略值。

### 1.3 三架构对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟源 | 8254 PIT / LAPIC Timer | ARM Generic Timer（`CNTFRQ_EL0` 读频率） | RISC-V `mtime` + `mtimecmp`（CLINT MMIO，S-mode 直接写） |
| 时钟中断 | IRQ 0 → IDT vector 0x50（`include/arch/i386/include/interrupt.h:17` `IRQ0_VECTOR=0x50`） | GIC PPI 27（Generic Timer 物理 PPI，私有外设中断，每个 CPU 一份） | S-mode 定时器中断 → `stvec`（RISC-V Privileged Spec，`sip.STIP=1` 触发，无固定向量号，由 `stvec` 指定 handler 入口） |
| 中断控制器 | LAPIC + IOAPIC（x86 APIC 规范） | GICv3：GICD（Distributor）+ GICR（Redistributor）+ CPU Interface（`ICC_*_EL1` 系统寄存器） | PLIC（外部设备中断）+ CLINT（Core-Local Interruptor，含 `mtimecmp`/software interrupt，`sifive/clint` 规范） |
| intr_init | 初始化 8259A 或 APIC | 初始化 GICv3（GICD/GICR 建表 + `ICC_SRE_EL1.SRE=1`；OMAP INTC 为 32 位 ARM 的 C 行为 架构演进，见 §2.3） | 初始化 PLIC（`enable` 位 + 优先级阈值 架构演进；CLINT 由 `ClockArch` 管理，见 §3.8 注） |
| arch_init | TSS（任务状态段）/APIC/ACPI/BIOS mem cut | TSS（软件抽象，保存 sp0）/PMU（性能监控单元）/bsp_init | PMP（Physical Memory Protection，`pmpaddr0-15`+`pmpcfg0-3` 配置内核地址空间访问权限） |

> **注**：x86-64 的 `TSS` 是硬件任务状态段，详见 [03-kmain-cstart.md](03-kmain-cstart.md) §1.4c；ARM 端口虽然也有同名 `tss_init()`/`struct tss_s`，但它**不是硬件 TSS**，只是一个软件抽象，里面只存一个 `sp0`（中断时用的内核栈指针），外加在栈顶记录 CPU id。

### 1.4 本章小结

本章覆盖 cstart 的后半段：在 [03-kmain-cstart.md](03-kmain-cstart.md) 建立保护结构之后，内核还需要：

1. **时钟**——让 OS 拥有可量化的时间粒度；
2. **中断控制器**——让设备（包括时钟）能异步通知 CPU；
3. **架构杂项初始化**——完成 PMU（性能监控单元）、ACPI（硬件配置/电源管理表）/APIC（本地高级可编程中断控制器）等架构特定设置。不同架构还可能在此阶段初始化串口、TSS 等硬件。

Minix3 的 `cstart()` 调用顺序与依赖关系详见 §1.1。Rust 版会在后续章节说明如何把这些职责拆分为独立的抽象，按"职责"而非"调用顺序"组织代码。具体 handler 实现、调度耦合、SMP/AP 启动等不在本章范围。

---

## 2. C 源码分析

### 2.1 init_clock()：时钟变量初始化

`clock.c:48-64`（函数体；L65-69 是 `timer_int_handler` 的文档注释，L70 起是函数签名，均不在函数内）：

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

2. **`env_get("hz")`**：从 boot 参数获取时钟频率。Minix3 的 boot 参数由 boot monitor 传递，格式是 `key=value` 字符串。默认 `DEFAULT_HZ = 60`（x86，`minix3/minix/include/arch/i386/include/archconst.h:4`）或 `1000`（ARM，`minix3/minix/include/arch/earm/include/archconst.h:4`）。

   > **设计层面的取舍**：x86 的 60 Hz 来自 IBM PC 8254 PIT 的输入时钟（1.1931816 MHz）与分频器选择，是早期 PC 的遗留值；ARM 的 1000 Hz 提供更高精度但增加中断开销。Rust 版把默认 tick 率统一为 100 Hz，并把这一取舍放在设计决策章节（§3.2）讨论，而不是在 C 源码分析中展开。

3. **频率范围检查**：`kclockinfo.hz` 必须在 2~50000 之间。超出范围则使用默认值。

4. **`memset(&kloadinfo, 0, ...)`**：清零负载统计结构体。`kloadinfo` 用于计算 1/5/15 分钟负载平均值。

**关键观察**：`init_clock()` 只初始化**软件变量**，不触碰硬件。硬件定时器（8254 PIT / LAPIC Timer / ARM Generic Timer）的配置发生在更晚的 `bsp_finish_booting()` 中（`bsp_finish_booting` 函数体定义在 `main.c:38`，其内部调用 `boot_cpu_init_timer(system_hz)` 的那行在 `main.c:73`，函数定义见 `clock.c:294`）。

此处注册的中断 handler 是 `timer_int_handler()`（`clock.c:70`，直接在 IRQ 入口被调用），它在当前上下文里完成 tick 推进、时间账务、虚拟定时器递减，并"在适当时机 notify 内核 task **CLOCK**"——但要注意，CLOCK 的 proc 身份（`com.h:49`）只是 notify 的名义目标，**不存在一个独立运行的"CLOCK 线程"去接收并处理这些通知**。CLOCK 属于三类运行态实体中的 Kernel task（详见 [06-proc-init-boot-proc.md §1.1.4](06-proc-init-boot-proc.md#114-三类运行态实体)）：Ring 0、共用 kernel image、无独立地址空间、编译时静态 link 进内核；它没有独立主循环，实际时钟逻辑全在 `timer_int_handler()` / `tmrs_exptimers()` 这条中断路径上同步执行（含 `setitimer`/`alarm`/`utime` 等用户态定时请求的处理与 `kclockinfo.uptime` 的推进）。本节只覆盖硬件 IRQ 准备——timer handler 跨界协议、定时器队列语义等见 [15-clock-timer.md §1.1](15-clock-timer.md#11-时钟中断在内核生命周期中的角色) 与 [15-clock-timer.md Ch2](15-clock-timer.md#ch2-c-源码分析)。

### 2.2 intr_init()：x86-64 中断控制器初始化

`i8259.c:30-53`（8259A PIC 版本；实际函数体范围，L28-29 是前置注释 + 空行；L54-58 是 `irq_8259_unmask`，L59-63 是 `irq_8259_mask`，均在 `intr_init` 之外）：

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
| ICW3 | 主片 `0x04`（= `1 << CASCADE_IRQ`，CASCADE_IRQ=2，见 `interrupt.h:41`） | 从片连接到 IRQ2 |
| ICW4 | `0x01`（slave, ICW4_AT_SLAVE）/ `0x05`（master, ICW4_AT_MASTER），auto_eoi 关闭时 | 正常 EOI / 8086 模式（auto_eoi=1 时为 AEOI 子常量） |
| OCW1 | 主片 `0xFB`（= `~(1<<CASCADE_IRQ)`）/ 从片 `0xFF` | 主片 mask 除级联 IRQ2；从片全屏蔽所有 IRQ |

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

**注意**：这是 Minix3 ARM（32 位）的实现。minix-rs 的 aarch64 目标用 GICv3 取代 OMAP INTC（`架构演进`，见 §3.8），初始化序列不同（需要初始化 Distributor、Redistributor 和 CPU Interface），但语义相同——都是配置中断路由并屏蔽所有 IRQ。

### 2.4 arch_init()：x86-64 架构特定初始化

`arch_system.c:246-281`（L246 函数签名，L247 `{`，函数体 L248-L281，L282 `}` 封口；L284 起是 `do_ser_debug` 函数）：

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

### 2.6 cstart() 中 `init_clock` 与 `intr_init` 之间的内容（环境变量解析 + 其他初始化）

`main.c:403-475` 中，`cstart()` 在 `init_clock()` 和 `intr_init()` 之间除了环境变量解析外，还夹着若干非 env_get 初始化步骤，下面分别列示：

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

> **注**：`cstart()` 在 `init_clock` 与 `intr_init` 之间**除环境变量外**还做了若干非 `env_get` 步骤：`arm_frclock` 清零 + `kuserinfo` 填充（`main.c:435-440`，user-mapped 结构初始化，详见现有 `04-platform-discovery.md`）、`USE_APIC` 块的 `no_apic`/`apic_timer_x` 配置（`main.c:442-453`，APIC 硬件配置属后续文档范围，详见现有 `08-system-init-boot-finish.md`）、`USE_WATCHDOG`（`main.c:455-459`，watchdog 是 `arch/i386/watchdog.c` 域）、`CONFIG_SMP`/`no_smp`（`main.c:461-469`，SMP-AP 启动属于 `08-system-init-boot-finish.md` 范围）。本章 §2.6 只列示 `env_get` 相关段以聚焦"boot 参数解析"主题。

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
/// （示意：实例化签名 + 完整 8 方法见 §4.2：`new`、`init_timer`、`read_ticks`、`read_tsc`、`stop_local_timer`、`init_profile_clock`、`stop_profile_clock`、`ack_profile_clock`）
pub trait ClockArch: Sized + Send + Sync {
    /// 从定时器描述符构造实例，提取硬件参数。
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;

    /// 配置并启动硬件定时器。
    fn init_timer(&mut self, hz: u32, cpu_id: u32);

    /// 读取当前 tick 计数。
    fn read_ticks(&self) -> u64;
}
```

**原因**：

1. **关注点分离**：软件状态和硬件配置是不同的关注点
2. **可测试性**：`ClockState` 可以在 mock 环境中测试，不需要真实硬件
3. **架构差异**：x86-64 用 8254 PIT / LAPIC Timer，aarch64 用 Generic Timer，riscv64 用 mtime——硬件配置完全不同
4. **`Send + Sync`（SMP 边界）**：先解释 Rust 类型层含义——
   - **`Send`** 表达"该类型的**所有权**可以跨越线程边界转移"。`ClockArch` 实例是**瞬态的**：调用点从全局平台描述符（`platform_desc().timer()`）构造一个 `CurrentClockArch`，完成硬件定时器操作后即丢弃，并不存在"每核存一份实例"的存储结构
   - **`Sync`** 表达"该类型的**共享引用 `&T`** 可以跨越线程边界传递"（`&T: Send` 当且仅当 `T: Sync`——这是 Rust 类型系统中的等价关系，不是 `Sync` 的"额外含义")
   - 二者**都不保证"`&self` 跨线程调用方法时的线程安全"**——Rust 的 Send/Sync 是**引用可传递性**的标记，不是访问同步的标记；具体同步由 Mutex / Atomic / BKL 等显式原语承担。

   回到 `ClockArch: Send + Sync`：
   - **为什么需要 `Send`**：定时器配置必须落在**当前 CPU** 自己的定时器上（RISC-V 的 CLINT `mtimecmp` 按 hart 编址），调用点通过 `current_cpuid()`（读 `SMP_STATE` 的 `bsp_cpu_id`）把 `cpu_id` 显式传给 `init_timer`/`stop_local_timer`。`Send` 约束保证实例所有权在 SMP 场景下可以跨线程转移（当前单核实现每次调用都从描述符现构造，不持有跨线程实例）
   - **为什么需要 `Sync`**：在某些代码路径上需要 `&ClockArch` 引用（如统一封装层、高层 API 接收 `&dyn ClockArch`）
   - **为什么不需要锁**：硬件寄存器本身是 per-CPU 的（LAPIC Timer、ARM Generic Timer、CLINT mtimecmp 每核一份），多核"同时读自己那份"互不干扰；`init_timer(&mut self)` 只编程本 CPU 的定时器（`cpu_id` 参数显式指定），调用点都在 boot 单线程阶段（`init_clock_and_interrupts`，以及 `bsp_finish_booting` Step 6 的幂等 no-op 重调），不走并发路径。**Send/Sync 在这里是"per-CPU 物理隔离 + 引用可传递"的语义编码**，而不是"并发数据结构"的声明——具体同步责任在调用点的 BKL 与 per-CPU 数据（`SMP_STATE` 的 `CpuLocal`，见 16-smp.md §4.3）
   - **对照 `!Sync` 反例**：标准库的 `Rc<T>` 没有实现 Sync（多线程共享同一引用计数会数据竞争）；裸 `RefCell<T>` 没有 Sync（borrow 计数器非原子）。这两类"线程不安全"的类型在 kernel crate 内也被严格隔离——SMP 代码用 `Arc<Mutex<T>>` 或 `Arc<Atomic*>` 替代



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
/// minix-rs: 统一为 100 Hz（架构演进，见 §3.8）。
/// 选择 100 Hz 的理由：10 ms tick 在响应延迟与上下文切换开销之间取得平衡，
/// 且是 Linux 服务器常见配置之一（CONFIG_HZ=100），便于与现有工具/预期对齐。
///
/// **权威定义位置**：`os/arch/src/arch/clock.rs:43`（`os/arch` crate 内的 `pub const DEFAULT_HZ: u32 = 100`）。
/// `os/kernel/src/clock.rs:474` 处的同值 `const` 是 `os/kernel` crate 内的独立副本（避免 `os/kernel` 反向依赖 `os/arch`），
两处值必须保持一致。修改时**先改 `os/arch/src/arch/clock.rs:43`**，再 sync 到 `os/kernel/src/clock.rs:474`**。
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
pub trait InterruptController: Sized + Send + Sync {
    /// 从中断控制器描述符构造实例，把硬件基址存入实例字段。
    /// 上层通过 `minix_platform::platform_desc().interrupt_controller()` 获取描述符。
    /// 属于 [04-platform-discovery.md §3.4](04-platform-discovery.md#34-硬件-trait-为什么要带实例状态) 的实例化模式。
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self;

    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn ack(&mut self, irq: IrqVector);
    fn eoi(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}
```

`InterruptController` 一共 **7 个方法**：`new` 一次性把硬件基址（GICv3 的 GICD/GICR 基址、PLIC 基址与 context、LAPIC/IOAPIC 基址等）从描述符 downcast 出来存入实例字段，其余 6 个 (`init` / `mask` / `unmask` / `ack` / `eoi` / `mask_all`) 是运行期操作。`new` 与 §3.1 `ClockArch::new`、`§3.4` (`EarlyConsole::init` 风格不同——串口 init 是无状态一次性准备，不是从描述符构造) 不完全对称：`InterruptController::new` 与 §3.5 `ArchInit::new` 都属于"实例化模式"，把硬件参数固化进实例以避免每次调用传递。

**为什么放在 `minix-plat` 而不是 `minix-arch`？**

`minix-arch` 抽象的是 CPU ISA（分页、保护、异常入口、trap），而中断控制器是**板级外设**：同一 CPU 架构可以搭配不同中断控制器（例如 ARM64 可以是 GICv2/GICv3/GICv4，RISC-V 可以是 PLIC/APLIC）。把它放在 `minix-plat` 让"CPU 长什么样"和"主板上有哪些外设"两个维度独立变化。

**实现分布**（三架构均已实现）：

| 架构 | 实现文件 | 硬件对应 |
|------|---------|---------|
| x86-64 | `os/plat/src/x86_64/interrupt.rs` | LAPIC + IOAPIC |
| aarch64 | `os/plat/src/arm64/interrupt.rs` | GICv3 |
| riscv64 | `os/plat/src/riscv64/interrupt.rs` | PLIC |
| mock（测试） | `os/plat/src/mock.rs:24` | 用于 IRQ/驱动测试，不进生产路径 |

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

> **⏸ DEFERRED（关于本 trait 的 C-Rust 不对称）**：本 trait 的 6 个方法按硬件动作的 CPU 局部性分属两类：
>
> - `init` / `mask_all` / `mask` / `unmask` —— **全局动作**（一次性 BSP 初始化；或修改 IOAPIC redirection / GICD_ICENABLER / PLIC ENABLE 位等"路由表"——一改全部 CPU 看见）
> - `ack` / `eoi` —— **per-CPU 动作**（写当前核私有寄存器：LAPIC EOI / ICC_EOIR1_EL1 / PLIC per-context complete——印证：`x86_64::ack(_irq)` 与 `arm64::eoi(_irq)` 中 `_irq` 参数完全被忽略）
>
> 现有 trait 把这两类语义不同的动作放进同一接口、同一 trait bound（`Send + Sync`），是**简化抽象**而非**对称抽象**——读者若按"对称接口"理解会错过硬件真相。trait 实际工作由外面 `BKL`（`os/kernel/src/irq_manager.rs`）保证并发安全，`Send + Sync` 是引用层面的类型证明不蕴含实例字段可多 CPU 同时变更。当前阶段可做的改进是文档层强化（本节已延展）；未来重构候选是把 trait 拆为 `InterruptRouter` + per-CPU `InterruptAck` 两个 trait——依赖 16-smp 的 SMP 完整实现。**详细背景与未来重构方案见** [todo.md §7.4.1](todo.md#741-i-13-interruptcontroller-traitc-rust-不对称--send--sync-真实动机)（I-13 项，P2 文档改进 + P2 重构候选，非本阶段落地范围）。

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
/// `ArchInit` 采用**实例化模式**（参见 [04-platform-discovery.md §3.4](04-platform-discovery.md#34-硬件-trait-为什么要带实例状态)）：
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
4. **实例化模式**：把 `ArchMiscDesc` 一次性写入实例字段，避免每次 `init()` 调用时重复传递参数；与 §3.3 `InterruptController::new()` 和 §3.4 `EarlyConsole::init()` 保持一致（参见 [04-platform-discovery.md §3.4](04-platform-discovery.md#34-硬件-trait-为什么要带实例状态) 实例化模式）

> **注意：ArchInit 是“阶段抽象”而非“功能抽象”**。它回答的是“除了时钟、中断和早期控制台之外，还有什么架构特定的杂项必须在此时完成”，而不是“所有架构做同一件事”。因此：
> - 凡是能抽象出跨架构一致语义的机制（如时钟节拍、中断路由、早期控制台），都应该有自己的 trait（`ClockArch`、`InterruptController`、`EarlyConsole`），不能塞进 `ArchInit`。
> - `ArchInit` 里只放那些**本身就没有跨架构一致性**的杂项（x86 的 ACPI、ARM 的 PMU/bsp_init、RISC-V 的 PMP/SIE），避免它变成无边界的“垃圾桶”。串口初始化由 `EarlyConsole::init()` 负责，不应再留在 `ArchInit`。

### 3.6 决策：arch_init 不做内存裁剪

**Minix3 C 的做法**：`arch_init()` 末尾调用 `cut_memmap()` 保留 BIOS 区域。

**minix-rs 不在 arch_init 中做内存裁剪**，原因：

1. **内存映射由 boot-shim 提供**：`KernelInfo.memmap` 已排除保留区域
2. **BIOS 区域不存在于 aarch64/riscv64**：`cut_memmap` 是 x86 特有的
3. **内存裁剪不是“架构硬件初始化”**：`cut_memmap` 修改的是系统内存图，属于内存管理/启动协议范畴，而不是配置某个架构硬件。把它放进 `ArchInit` 会把内存管理的职责泄漏到架构初始化阶段，让 `ArchInit` 变成无边界垃圾桶。这与 §3.5 的界定一致：`ArchInit` 只收留“尚未被其他 trait 抽象的架构杂项”，而非“所有架构不同的事情”。

### 3.7 决策：本阶段不实现完整的 `timer_int_handler`、`intr_handle` 和 `bsp_finish_booting`

**Minix3 C 的做法**：`timer_int_handler()`（`clock.c:70-173`）除了更新 `uptime`/`realtime`/`loadavg` 外，还更新 `bill_ptr` 和各进程的 user/sys 时间统计；`irq_handle()`（`interrupt.c:116-158`，旧注释中的 `intr_handle` 与汇编标签 `intr_handle` 都指向同一函数）做完整的中断分发（mask 当前 IRQ、查 `irq_handlers[]` 表、调用注册 handler，最后 `unmask`）；`bsp_finish_booting()`（`main.c:38-109`）使能定时器中断（`boot_cpu_init_timer`）、初始化 FPU、设置 `kernel_may_alloc=0` 并移交用户态。

**Rust 当前做法**：本阶段只让内核具备响应时钟中断和中断控制器的硬件能力，因此：
- `ClockState::tick()` 已实现软件计数与记账：`uptime`/`realtime`/`loadavg` 更新、进程 user/sys 时间、虚拟/档案定时器到期、BSP 到期 alarm 定时器处理（详见 §4.1）；quantum 递减不在 `tick()` 内（由 `clock::decrement_quantum()` 处理，D9），基于 quantum 的调度决策留到 [11-scheduling-primitives.md](11-scheduling-primitives.md)；
- `InterruptController` 只完成初始化与 mask/unmask，完整中断分发逻辑（`intr_handle()`）留到 [14-exception-interrupt.md](14-exception-interrupt.md)；
- `bsp_finish_booting()`（定时器中断使能、FPU 初始化、`kernel_may_alloc=0`、用户态移交）留到 SMP/调度初始化文档（注：AP 启动在 C 中位于 `arch_smp.c` 的 `smp_start_aps()`，不在 `bsp_finish_booting()` 内）。

这些功能依赖进程表、调度器、SMP 状态，属于后续里程碑（06-proc-init、[11-scheduling-primitives.md](11-scheduling-primitives.md)、[14-exception-interrupt.md](14-exception-interrupt.md) 以及 SMP 文档），所以不在本阶段展开。

**行为变更声明（2026-08-15，V3 P0-1；2026-08-15 复核修正）**：删除 `boot_init_timer`（连同 `ArchBoot` trait、`MockArchBoot` 与三个 `MOCK_*` 全局）后，`enable_timer_irq` 不再被单独调用。**逐架构核对（下表）只有 x86_64 存在真实行为差异**——aarch64/riscv64 的 `ClockArch::init_timer` 写入与旧 `enable_timer_irq` 完全相同的寄存器：

| 架构 | 旧行为（boot 期间，旧 arch_boot.rs L192-230 / L291-318 / L354-378 的 enable_timer_irq） | 新行为（boot 期间） | 差异？ |
|------|------------------------------------------------------------------------------------------|---------------------|--------|
| x86_64 | LAPIC LVT Timer Mask bit 16 = 0 + SVR Enable bit 8 = 1 | LAPIC LVT Timer Mask 保持 1（`init_timer` 只编程 8254 PIT，不碰 LVT）；SVR Enable 仍由 `InterruptController::init`（init_lapic）置位 | ✅ 仅 LVT Mask 0→1（PIT 是 boot 时钟源，差异休眠，待 LAPIC LVT 时钟源启用才可见） |
| aarch64 | CNTP_CTL_EL0 = Enable=1, IMASK=0 | 相同：`AArch64ClockArch::init_timer` 写 `msr cntp_ctl_el0, 1`（Enable=1, IMASK=0） | ❌ 无（timer live，可 firing，尚无 handler） |
| riscv64 | sie.STIE bit 5 = 1 | 相同：`Riscv64ClockArch::init_timer` 执行 `csrs sie, 0x20`（STIE=1） | ❌ 无（timer live，可 firing，尚无 handler） |

因此**不是**"boot 期间 timer IRQ 在三个架构全部保持 masked"：aarch64/riscv64 的 timer 在 boot 期间保持 live（与旧行为一致），可触发但尚无 handler。timer IRQ 在 boot 期间的核心动作是 IRQ-chain 注册（`IrqManager::register_hook`）；仅当 x86_64 采用 LAPIC LVT Timer 作为时钟源时才需额外调用 `<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq()`（骨架见 `os/kernel/src/lib.rs` `bsp_finish_booting` 注释）。

**调用时序约束**：`TimerIrqGate::enable_timer_irq` / `disable_timer_irq` 必须在中断控制器初始化之后调用：

- **x86_64**：`X86_64InterruptController::init_lapic`（`os/plat/src/x86_64/interrupt.rs` L112-120）设置 IA32_APIC_BASE 全局 enable bit 11 并写 SVR Enable，此后 LAPIC MMIO 才可访问。`bsp_finish_booting`（TimerIrqGate 的唯一调用点）晚于该步骤，LAPIC 必已映射 → 旧 LAPIC-null fallback 删除安全；新实现未映射时 `panic!`（不再写 mock 状态）。
- **aarch64**：`AArch64InterruptController::init` 完成 GIC distributor 全局 enable（GICD_CTLR.EnableGrp1NS）、redistributor wake（GICR_WAKER）与 CPU interface enable（ICC_SRE_EL1 / ICC_PMR_EL1 / ICC_IGRPEN1_EL1），timer PPI 的 GIC delivery path 在 `enable_timer_irq` 之前已成立。
- **riscv64**：无前置要求（sie.STIE 是纯 supervisor CSR 写）。

### 3.8 架构差异对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 时钟硬件 | 8254 PIT (I/O port 0x40-0x43) / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | RISC-V mtime (CLINT MMIO) |
| 时钟频率 | 100 Hz (可配置, 架构演进) | 100 Hz | 100 Hz |
| **中断控制器** | LAPIC + IOAPIC | GICv3 (GICD + GICR + CPU IF, 架构演进) | **PLIC** (external) + **CLINT** (timer + software, 架构演进) |
| IRQ 数量 | 64 (APIC mode) | 64 (software limit) / 1020 (GICv3 SPI hardware capability) | 64 (software limit) / 1024 (PLIC max) |
| `EarlyConsole::init()` | COM1 UART 配置（115200 8N1 + FIFO） | 默认空实现 | 默认空实现 |
| `EarlyConsole::write_byte()` | COM1 I/O port `0x3F8` | PL011 MMIO `0x0900_0000` | SBI `console_putchar` ecall |
| arch_init | ACPI（APIC 由 `InterruptController` 负责，串口由 `EarlyConsole` 负责） | PMU cycle counter + bsp_init | PMP + SIE |

> **注**: RISC-V "中断控制器" 应明确分为 **PLIC** (external interrupts, 由 `InterruptController` trait 管理) 和 **CLINT** (timer + software interrupts, 由 `ClockArch` 管理)。前表中"RISC-V 中断控制器"列单写"PLIC + CLINT"易混淆——`ClockArch` 用 CLINT 的 mtime/mtimecmp，`InterruptController` 只用 PLIC。

> **架构演进标注**：本节涉及的架构演进按"本文档 + 代码注释"标注，每项给出 Minix3 现状与 minix-rs 演进，读者无需跳出本文档：
>
> | 演进 | 内容 | Minix3 现状 | minix-rs |
> |------|------|-------------|----------|
> | aarch64 中断路径 | OMAP INTC → GICv3 | 32 位 ARM 用 OMAP INTC（`omap_intr.c:24-40`）+ BSP timer（`omap_timer.c`），见 §2.3 | ARMv8-A GICv3（GICD/GICR + CPU interface）+ Generic Timer（CNTP），见 §4.6/§4.7 |
> | riscv64 全新架构 | Minix3 无 RISC-V 移植（minix-rs 引入） | — | 对标 RISC-V Privileged Spec 1.12：CLINT mtime/mtimecmp + PLIC + `sie.STIE` + PMP，见 §4.5/§4.8/§4.12 |
> | 时钟频率统一 100 Hz | i386 `DEFAULT_HZ=60`、earm `DEFAULT_HZ=1000`（`archconst.h:4`），boot 参数可调 | 三架构统一编译期常量 100 Hz（`os/arch/src/arch/clock.rs:43`），见 §3.2 |

---

## 4. 实现详解

Rust 版将 C 版 `cstart()` 的后三个调用（`init_clock()`、`intr_init()`、`arch_init()`）以及早期调试输出抽象为四个 trait：`ClockArch` 负责**硬件时钟节拍**，`InterruptController` 负责**中断路由与屏蔽**，`EarlyConsole` 负责**早期控制台输出**，`ArchInit` 负责**架构杂项初始化**。拆分的依据是这四个职责在硬件层面完全独立——时钟芯片、中断控制器、串口/ACPI 是三类不同的外设，早期控制台输出又与这些初始化逻辑互不依赖。

**`ClockArch` 的抽象语义**：

> 回答"谁来打节拍、多快打一次"。

- `new(desc)`：从 `TimerDesc` 提取硬件参数（PIT 基准频率、LAPIC 基址、mtime/mtimecmp 地址、频率）存入实例字段。
- `init_timer(&mut self, hz, cpu_id)`：配置硬件定时器以 `hz` Hz 的频率产生周期性中断。x86-64 使用 PIT（可编程间隔定时器）或 LAPIC timer，aarch64 使用 ARM Generic Timer（CNTP_* 寄存器），riscv64 使用 CLINT mtimecmp——CLINT 每 hart 一份 mtimecmp，`cpu_id`（riscv64 上即 hart id）用于定位本核的比较器，其余架构忽略。
- `read_ticks(&self)`：读取硬件 tick 计数，用于精细计时和性能分析。

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
>
> **位置**：`os/kernel/src/clock.rs`（**kernel 层**，非 arch 层）；硬件定时器配置在 `os/arch/src/{arch}/clock.rs` 中的 `ClockArch::init_timer`

`ClockState` 把 C 版的**三份全局状态**收进一个结构体：`kclockinfo`（tick 频率、uptime、realtime、boottime、adjtime_delta）+ `kloadinfo`（平均负载采样）+ `clock_timers`（定时器队列）。所有字段由 **BKL** 保护——时钟中断处理路径持有 BKL 进入 `tick()`，因此结构体不需要内部可变性。

与 C 版的关键差异是**每 CPU 一份实例**：

- **BSP 实例**（`is_bsp = true`）：拥有全局时间（`uptime`/`realtime`/`boottime`）、`adjtime_delta` 和 BSP-only 的定时器队列；
- **AP 实例**（`is_bsp = false`）：只跟踪本地负载，时间字段恒为 0，`set_timer` 会 panic（编程错误）。

C 版用 `cpu_is_bsp(cpuid)` 在运行时判断，Rust 用结构体字段 `is_bsp` 在运行时分支（§3.1 的 D8 决策）——单镜像代码，BSP/AP 靠实例区分，不需要编译期 const generic。

#### 4.1.1 常量与全局时间镜像

**常量**：`DEFAULT_HZ` 与 `TMR_NEVER` 是 `pub`；`LOAD_UNIT_SECS`/`LOAD_HISTORY` 是模块私有。`DEFAULT_HZ` 的权威定义在 arch 层（`os/arch/src/arch/clock.rs:43`，kernel 层是独立副本，见 §3.2）；`LOAD_HISTORY_SIZE` 同样在 arch 层（`os/arch/src/arch/clock.rs:49`），kernel 层的私有 `LOAD_HISTORY` 与它同值（150）。

```rust
/// 默认时钟频率（Hz）。
/// C: DEFAULT_HZ — archconst.h:4（i386=60）/ earm=1000；Rust 统一 100（§3.2）
pub const DEFAULT_HZ: u32 = 100;

/// 定时器"永不触发"哨兵。
/// C: TMR_NEVER = TMRDIFF_MAX + 1 — timers.h:48；Rust 用 u64::MAX（语义等价）
pub const TMR_NEVER: u64 = u64::MAX;

/// 负载采样窗口（秒）。
/// C: _LOAD_UNIT_SECS — type.h:88（6）
const LOAD_UNIT_SECS: u64 = 6;

/// 负载历史槽数量。
/// C: _LOAD_HISTORY — type.h:95（150 = 60s × 15min / 6s）
const LOAD_HISTORY: usize = 150;
```

**全局时间镜像**：调度器代码拿不到 `&ClockState`（把引用穿过整个调度调用链代价太大），因此 BSP 每次 tick 时把 `uptime`/`realtime`/`boottime` 同步进三个 `AtomicU64` 全局，`get_monotonic()`/`get_realtime()`/`get_boottime()` 在任意上下文（持锁或不持锁）都能原子读取。C 版等价物是 `kclockinfo` 的全局变量直接可读。

```rust
/// 单调 uptime（tick 数），由 BSP tick handler 更新。
/// C: kclockinfo.uptime — 全局变量，get_monotonic() 读取（clock.c:203）
static CLOCK_UPTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 启动以来的墙钟 tick 数，由 BSP tick handler 更新。
/// C: kclockinfo.realtime — get_realtime() 读取（clock.c:178）
static CLOCK_REALTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 启动时间（UNIX 纪元秒），由 SYS_STIME 设置。
/// C: kclockinfo.boottime — get_boottime() 读取（clock.c:220）
static CLOCK_BOOTTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
```

#### 4.1.2 定时器身份、动作与队列

C 版定时器是 `minix_timer_t *` 侵入式链表：`set_kernel_timer(tp, ...)` / `reset_kernel_timer(tp)` 以结构体指针为稳定身份。Rust 不能沿用指针（定时器不再是侵入式链表节点），于是引入三个类型（对应 15-clock-timer.md 的 D3/D5/D6 决策）：

- `TimerId(u64)`：稳定身份，每 `ClockState` 单调递增分配，替代 C 的指针身份；
- `TimerEntry`：到期时间 + 动作（替代链表节点）；
- `TimerAction`：到期动作枚举，替代 C 的 `tmr_func_t` 函数指针 + `tmr_arg` 整数；当前只有 `NotifyAlarm`（对应 `cause_alarm()` → `mini_notify(CLOCK, endpoint)`），`KernelCallback` 变体因无调用者被删除（YAGNI）。

```rust
/// 定时器稳定身份，替代 C 的 `minix_timer_t *tp` 指针。
///
/// C 以定时器结构体指针作为 set/reset 的稳定身份
/// （set_kernel_timer(tp, ...) / reset_kernel_timer(tp)）。Rust 不能使用
/// 指针（定时器不是侵入式链表节点），所以用包裹每-ClockState 单调计数器的
/// newtype。`TimerId` 由 `ClockState::set_timer()` 分配并返回给调用者，
/// 调用者保存它以便之后调用 `ClockState::reset_timer(id)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    /// 从原始计数值构造 TimerId。
    pub const fn new(id: u64) -> Self { Self(id) }

    /// 返回原始计数值。
    pub fn raw(self) -> u64 { self.0 }
}

/// 定时器到期时执行的动作。
///
/// 替代 C 的 `tmr_func_t` 函数指针 + `tmr_arg` 整数。
/// 设计决策 D6：用枚举分发替代函数指针以获得类型安全。
///
/// C: cause_alarm(proc_nr_e) — do_setalarm.c:69-76
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerAction {
    /// 通过同步 alarm 通知进程。
    /// C: cause_alarm() → mini_notify(CLOCK, endpoint)
    NotifyAlarm { endpoint: Endpoint },
    // KernelCallback 变体已删除（YAGNI）：无调用者的死代码。
    // 若将来需要内核内部定时器回调（如看门狗或 profile timer），
    // 在此添加新变体并在 TimerQueue::expire() 中接线分发。
}

/// 定时器队列中的一个条目。
///
/// 替代 C 的 `minix_timer_t` 链表节点。
/// 设计决策 D5：存进 `TimerQueue`（BTreeSet + BTreeMap）而非指针链表。
///
/// C: timers.h — struct minix_timer
#[derive(Debug, Clone)]
pub struct TimerEntry {
    /// 到期时间（单调 tick 数）。C: tmr_exp_time
    pub exp_time: u64,
    /// 到期动作。替代 C 的 tmr_func_t 回调。
    pub action: TimerAction,
}
```

队列本体是**双索引结构**（D2 决策），替代 C 的 `clock_timers` 链表：

- `by_expiry: BTreeSet<(exp_time, TimerId)>`：按到期时间排序，`pop_expired()` 做 O(k log N) 到期扫描；
- `by_id: BTreeMap<TimerId, TimerEntry>`：按 id O(log N) 查找，供 `reset_timer(id)` 使用；
- `next_id`：每 `ClockState` 的单调 id 计数器。

选 `BTreeMap` 而非 `HashMap`，因为 `HashMap` 不在 `alloc::collections`（引入需要 `hashbrown` crate）。双索引同时修复了单 `BTreeMap<u64, _>` 会静默覆盖同一 `exp_time` 定时器的问题——C 的链表允许同一到期时间挂多个定时器。

```rust
/// 带稳定身份 + 排序到期的 alarm 定时器队列。
///
/// 替代 C 的 `minix_timer_t *clock_timers` 链表（clock.c:37）。
///
/// 设计决策 D2：双索引数据结构。
#[derive(Debug, Default)]
struct TimerQueue {
    /// 按 (exp_time, id) 排序——支持 O(k log N) 到期扫描。
    by_expiry: BTreeSet<(u64, TimerId)>,
    /// 按 TimerId 查找——支持 O(log N) 的 reset_timer(id)。
    /// 用 BTreeMap 而非 HashMap，因为 HashMap 不在 alloc::collections。
    by_id: BTreeMap<TimerId, TimerEntry>,
    /// 下一个 TimerId 计数器（每 ClockState 单调）。
    next_id: u64,
}

impl TimerQueue {
    /// 插入一个定时器条目。返回分配的 TimerId。
    /// C: tmrs_settimer() — timers.h（插入链表）
    fn insert(&mut self, entry: TimerEntry) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;
        self.by_expiry.insert((entry.exp_time, id));
        self.by_id.insert(id, entry);
        id
    }

    /// 按 TimerId 移除定时器。返回被移除的条目（若存在）。
    /// C: tmrs_clrtimer() — timers.h（按指针从链表移除）
    fn remove(&mut self, id: TimerId) -> Option<TimerEntry> {
        let entry = self.by_id.remove(&id)?;
        self.by_expiry.remove(&(entry.exp_time, id));
        Some(entry)
    }

    /// 弹出下一个已到期（exp_time <= now）的定时器。
    /// C: tmr_has_expired() + tmrs_exptimers() — timers.h / clock.c:159-161
    /// 最早的定时器未到期时返回 None。
    fn pop_expired(&mut self, now: u64) -> Option<TimerEntry> {
        let first = self.by_expiry.iter().next().copied()?;
        if first.0 > now {
            return None;
        }
        self.by_expiry.remove(&first);
        self.by_id.remove(&first.1)
    }
}
```

#### 4.1.3 平均负载状态（LoadInfo）

```rust
/// 平均负载跟踪状态。
///
/// C: struct loadinfo kloadinfo — clock.h
#[derive(Debug)]
pub struct LoadInfo {
    /// 当前负载采样槽索引。
    /// C: proc_last_slot
    proc_last_slot: u16,
    /// 负载历史环形缓冲。
    /// C: proc_load_history[_LOAD_HISTORY] — u16[150]
    proc_load_history: [u16; LOAD_HISTORY],
    /// 上次负载更新的时钟 tick。
    /// C: last_clock
    last_clock: u64,
}

impl LoadInfo {
    pub const fn new() -> Self {
        Self {
            proc_last_slot: 0,
            proc_load_history: [0; LOAD_HISTORY],
            last_clock: 0,
        }
    }
}

impl Default for LoadInfo {
    fn default() -> Self {
        Self::new()
    }
}
```

字段与 C `struct loadinfo`（`include/minix/type.h:98-102`）一一对应：`proc_last_slot: u16` ↔ `u16_t proc_last_slot`；`proc_load_history: [u16; 150]` ↔ `u16_t proc_load_history[_LOAD_HISTORY]`；`last_clock: u64` ↔ `clock_t last_clock`。**类型宽度与 C 严格对齐（`u16`）**，累加用 `wrapping_add` 匹配 C 无符号回绕（见 §4.1.6）。

#### 4.1.4 tick 结果（TimerTickResult / VtimerExpired）

`tick()` 返回两类"需要调用者处理的副作用"：到期 alarm 定时器的动作列表 + 当前（或记账）进程的虚拟/档案定时器到期状态。**它不包含 `quantum_exhausted`**——quantum 递减由 `clock::decrement_quantum()` 单独处理（§3.7 D9），不在 `tick()` 内；C 版等价物是 `context_stop()`（arch_clock.c:326-330）在上下文切换路径做的递减，同样独立于 `timer_int_handler()`。

```rust
/// 虚拟/档案定时器到期检查结果。
/// C: vtimer_check() — do_vtimer.c:81-103
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtimerExpired {
    /// 虚拟定时器到期 → SIGVTALRM。C: VT_VIRTUAL (0)
    Virtual,
    /// 档案定时器到期 → SIGPROF。C: VT_PROF (1)
    Prof,
}

/// `ClockState::tick()` 处理一次定时器中断后返回的结果。
///
/// 包含调用者必须处理的到期 alarm 动作列表（例如通过 mini_notify 发通知），
/// 以及当前（或记账）进程的虚拟/档案定时器到期状态。
///
/// **不包含 quantum_exhausted** —— quantum 递减由 clock::decrement_quantum()
/// 处理（D9），不在 ClockState::tick() 内。
#[derive(Debug, Default)]
pub struct TimerTickResult {
    /// 到期 alarm 定时器的动作（BSP only）。
    /// C: tmrs_exptimers() 输出 — clock.c:159-161
    pub expired_alarms: Vec<TimerAction>,
    /// 当前或记账进程的虚拟/档案定时器到期状态。
    /// C: vtimer_check() 输出 — do_vtimer.c:81-103
    pub vtimer_expired: Option<VtimerExpired>,
}
```

#### 4.1.5 ClockState：结构与构造

```rust
/// 全局时钟状态，等价于 C 的 kclockinfo + kloadinfo + clock_timers。
///
/// 所有字段由 BKL 保护。时钟 handler 在 BKL 下运行，无需内部可变性。
///
/// 设计决策 D1：封装为结构体以获得所有权清晰性。
/// 设计决策 D8：per-CPU 实例 + is_bsp 标志（替代 PerCpuTick const generic——
/// SMP 单镜像需要运行时 BSP/AP 区分，而非编译期）。
///
/// C: clock.h — struct clockinfo kclockinfo + struct loadinfo kloadinfo
/// C: clock.c:37 — static minix_timer_t *clock_timers（BSP only）
#[derive(Debug)]
pub struct ClockState {
    /// 本实例所属的 CPU id。C: cpuid — get_cpulocal_var(cpu)
    cpu_id: CpuId,
    /// 是否为 BSP 实例（拥有全局时间 + alarm 定时器）。
    /// C: cpu_is_bsp(cpuid) — clock.c:91
    is_bsp: bool,
    /// 时钟频率（Hz）。C: kclockinfo.hz
    hz: u32,
    /// 启动以来的单调 tick 数。C: kclockinfo.uptime（BSP only）
    uptime: u64,
    /// 启动以来的墙钟 tick 数（受 adjtime 影响）。C: kclockinfo.realtime（BSP only）
    realtime: u64,
    /// 启动时间（UNIX 纪元秒）。C: kclockinfo.boottime（BSP only）
    boottime: u64,
    /// 时间调整增量（正=加速，负=减速）。C: adjtime_delta（clock.c:42，BSP only）
    adjtime_delta: i32,
    /// 同步 alarm 定时器队列（BSP only）。
    /// C: clock_timers（clock.c:37）
    /// 设计决策 D2：TimerQueue（BTreeSet + BTreeMap）替代链表。
    timers: TimerQueue,
    /// 平均负载信息。C: kloadinfo（所有 CPU）
    load_info: LoadInfo,
}

impl ClockState {
    /// 用默认频率（100Hz）创建 BSP ClockState。
    /// C: init_clock() — clock.c:47-64
    pub fn new() -> Self {
        Self::new_for_cpu(CpuId::BSP, true)
    }

    /// 为指定 CPU 创建 ClockState。
    ///
    /// BSP 实例 is_bsp = true（拥有全局时间 + alarm 定时器）；
    /// AP 实例 is_bsp = false（只跟踪本地负载）。
    ///
    /// C: boot_cpu_init_timer()（BSP）/ app_cpu_init_timer()（AP）— clock.c:294-312
    pub fn new_for_cpu(cpu_id: CpuId, is_bsp: bool) -> Self {
        Self {
            cpu_id,
            is_bsp,
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime: 0,
            boottime: 0,
            adjtime_delta: 0,
            timers: TimerQueue::default(),
            load_info: LoadInfo::new(),
        }
    }

    /// 用自定义频率创建 BSP ClockState。
    ///
    /// C: init_clock() 里的 env_get("hz") 覆盖 — clock.c:56-60
    /// 范围 2..=50000（与 C 的校验一致），越界回退 DEFAULT_HZ。
    pub fn with_hz(hz: u32) -> Self {
        let hz = if !(2..=50000).contains(&hz) { DEFAULT_HZ } else { hz };
        let mut state = Self::new();
        state.hz = hz;
        state
    }

    /// 设置内核定时器。返回 TimerId 供之后 reset。
    ///
    /// C: set_kernel_timer() — clock.c:229-240
    ///
    /// # Panics
    ///
    /// 在 AP 实例上调用会 panic（alarm 定时器 BSP-only）。这是编程错误：
    /// AP 不拥有全局定时器队列。
    pub fn set_timer(&mut self, entry: TimerEntry) -> TimerId {
        assert!(self.is_bsp, "set_timer called on AP ClockState (timers are BSP-only)");
        self.timers.insert(entry)
    }

    /// 按 TimerId 重置（移除）内核定时器。
    ///
    /// C: reset_kernel_timer() — clock.c:245-255
    ///
    /// 返回被移除的条目，未找到（或这是 AP 实例，没有定时器）时返回 None。
    pub fn reset_timer(&mut self, id: TimerId) -> Option<TimerEntry> {
        if !self.is_bsp { return None; }
        self.timers.remove(id)
    }
}
```

其余访问器都是单行读取，语义与 C 的全局访问函数一一对应：

| 方法 | 签名 | C 等价物 | 说明 |
|------|------|---------|------|
| `hz()` | `-> u32` | `kclockinfo.hz` / `system_hz` | 时钟频率 |
| `system_hz()` | `-> i32` | `system_hz` | 供 do_settime.c 做 tick 换算 |
| `uptime()` | `-> u64` | `get_monotonic()` — clock.c:203 | AP 恒 0 |
| `realtime()` | `-> u64` | `get_realtime()` — clock.c:178 | AP 恒 0 |
| `boottime()` | `-> u64` | `get_boottime()` — clock.c:220 | AP 恒 0 |
| `cpu_id()` | `-> CpuId` | `get_cpulocal_var(cpu)` | 本实例所属 CPU |
| `is_bsp()` | `-> bool` | `cpu_is_bsp(cpuid)` | BSP/AP 区分 |
| `set_boottime(u64)` | `&mut self` | `set_boottime()` — clock.c:212 | BSP-only；同步 `CLOCK_BOOTTIME` |
| `set_realtime(u64)` | `&mut self` | `set_realtime()` — clock.c:187 | BSP-only；同步 `CLOCK_REALTIME` |
| `set_adjtime_delta(i32)` | `&mut self` | `set_adjtime_delta()` — clock.c:195 | BSP-only |
| `adjtime_delta()` | `-> i32` | `adjtime_delta` — clock.c:42 | AP 恒 0 |
| `load_history()` | `-> &[u16; LOAD_HISTORY]` | `kloadinfo.proc_load_history` | 负载历史（只读） |

`set_boottime`/`set_realtime` 在写字段的同时把新值同步进全局镜像（§4.1.1），保证 `get_boottime()`/`get_realtime()` 立即可见。

#### 4.1.6 tick 语义：tick_bsp / tick_ap / tick_with / tick / load_update

`tick()` 是主时钟中断处理函数，按 `hz` 频率调用。**BKL 前置条件**：调用方必须已持有 BKL——C 版 BKL 在 `context_stop()`（trap 入口汇编）获取，Rust 版由中断入口负责。`tick()` 内部**不**获取 BKL，原因有二：它可能从已持锁的 syscall 路径进入（BKL 不可重入，重复获取会死锁）；C 的模式就是"调用方持锁"，而非"被调方取锁"。

三个入口方法的关系：

- `tick_bsp()` / `tick_ap()`：`tick()` 的便捷包装，用 `debug_assert!` 校验 `is_bsp` 状态后直接委托；
- `tick_with()`：**零分配热路径变体**——对每个到期定时器内联调用 `on_expired` 回调，不产生 `Vec` 堆分配；生产中断处理器优先使用；
- `tick()`：包装 `tick_with`，用 `Vec::with_capacity(4)` 收集到期动作（测试与便捷代码用，避免首次 push 的再分配）。

```rust
/// 零分配版 tick，供热路径使用。
///
/// 不在 Vec 中收集到期 alarm（push 会堆分配），而是对每个到期定时器
/// 内联调用 on_expired 回调。生产中断处理器应优先使用本变体；
/// 测试和便捷代码可用返回 TimerTickResult（含 Vec<TimerAction>）的 tick()。
pub fn tick_with<F>(
    &mut self,
    current_proc: &mut KProcess,
    billp: Option<&mut KProcess>,
    ready_count: usize,
    mut on_expired: F,
) -> Option<VtimerExpired>
where
    F: FnMut(TimerAction),
{
    // 1. BSP-only：更新 uptime 和 realtime（含 adjtime）
    //    C: clock.c:91-104；D8：运行时按 self.is_bsp 分支。
    if self.is_bsp {
        self.uptime += 1;
        CLOCK_UPTIME.store(self.uptime, Ordering::Release);

        if self.adjtime_delta != 0 && (self.uptime & 0x1) != 0 {
            self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
            self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
        } else {
            self.realtime += 1;
        }
        CLOCK_REALTIME.store(self.realtime, Ordering::Release);
    }

    // 2. 时间记账：给当前进程记用户时间。
    //    C: clock.c:116 — p->p_user_time++
    current_proc.p_time.add_user_time(1);

    // 3. 递减当前进程的虚拟/档案定时器。
    //    C: clock.c:128-133
    let mut vtimer_expired = None;

    if current_proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
        let expired = current_proc.p_time.tick_virt_timer();
        if expired {
            vtimer_expired = Some(VtimerExpired::Virtual);
        }
    }
    if current_proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
        let expired = current_proc.p_time.tick_prof_timer();
        if expired && vtimer_expired.is_none() {
            vtimer_expired = Some(VtimerExpired::Prof);
        }
    }

    // 4. 记账进程（当前进程不可记账时）。
    //    C: clock.c:118-120 — if (!BILLABLE) billp->p_sys_time++
    //    C: clock.c:134-138 — if (!BILLABLE) billp->p_prof_left--
    //    C: clock.c:147-148 — if (p != billp) vtimer_check(billp)
    //    D10：调用方显式传 billp。
    if let Some(billp) = billp {
        billp.p_time.add_sys_time(1);
        if billp.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
            let expired = billp.p_time.tick_prof_timer();
            if expired && vtimer_expired.is_none() {
                vtimer_expired = Some(VtimerExpired::Prof);
            }
        }
    }

    // 5. BSP-only：对每个到期 alarm 定时器调用回调。
    //    C: clock.c:153-161 — if (cpu_is_bsp) tmrs_exptimers(...)
    //    R-06：零分配——用回调而非 Vec::push。
    if self.is_bsp {
        while let Some(entry) = self.timers.pop_expired(self.uptime) {
            on_expired(entry.action);
        }
    }

    // 6. 负载更新（所有 CPU）。
    //    C: clock.c:151 — load_update()
    self.load_update(ready_count);

    vtimer_expired
}
```

六步与 C `timer_int_handler()`（clock.c:70-173）逐步对应：

| 步骤 | Rust | C |
|------|------|---|
| 1 | BSP 更新 `uptime`/`realtime`（adjtime 调整）+ 同步全局镜像 | clock.c:91-104 |
| 2 | 当前进程 `p_time.add_user_time(1)` | clock.c:116 `p->p_user_time++` |
| 3 | 当前进程 VIRT/PROF 定时器递减，到期记 `VtimerExpired` | clock.c:128-133 |
| 4 | 记账进程 `sys_time` + PROF 定时器递减（`billp` 非 None 时） | clock.c:118-120 / 134-138 |
| 5 | BSP 弹出并处理到期 alarm 定时器 | clock.c:153-161 `tmrs_exptimers` |
| 6 | `load_update()`（所有 CPU） | clock.c:151 |

`tick()` 只是把第 5 步的动作收集进 `Vec` 再包成 `TimerTickResult`：

```rust
pub fn tick(
    &mut self,
    current_proc: &mut KProcess,
    billp: Option<&mut KProcess>,
    ready_count: usize,
) -> TimerTickResult {
    // 委托 tick_with，用 Vec 收集器。
    // 预分配容量 4 避免首次 push 再分配；每个 tick 典型的到期定时器
    // 数量是 0-2 个，容量 4 覆盖常见情形。
    let mut expired_alarms = Vec::with_capacity(4);
    let vtimer_expired = self.tick_with(
        current_proc,
        billp,
        ready_count,
        |action| expired_alarms.push(action),
    );

    // 注意：quantum 递减不在这里（D9）。
    // C: context_stop() — arch_clock.c:326-330 按 TSC delta 递减
    // p_cpu_time_left；Rust 由 clock::decrement_quantum() 处理，
    // 调用方在本方法返回后单独调用。

    TimerTickResult {
        expired_alarms,
        vtimer_expired,
    }
}
```

**`load_update()`（所有 CPU，每个 tick 调用）**：负载历史是**每 6 秒一个采样槽**的环形缓冲（150 槽 = 15 分钟窗口）。槽位由 `slot = (uptime / hz / 6) % 150` 计算；**换槽时先把新槽清零再累加**（每 6s 窗口内累加同槽位，而不是每 tick 轮转），累加用 `wrapping_add` 匹配 C 的 `u16_t` 无符号回绕。与 C `load_update()`（clock.c:260-292）逐行对应。

```rust
/// 更新平均负载跟踪。
///
/// C: load_update() — clock.c:260-292
fn load_update(&mut self, ready_count: usize) {
    let slot = ((self.uptime / self.hz as u64 / LOAD_UNIT_SECS) % LOAD_HISTORY as u64) as u16;

    if slot != self.load_info.proc_last_slot {
        self.load_info.proc_load_history[slot as usize] = 0;
        self.load_info.proc_last_slot = slot;
    }

    // u16 与 C 的 u16_t 对齐；C 无符号加法回绕，这里用 wrapping_add 保持一致
    // （实际值 = 6s 窗口内可运行进程数，远小于 65535）。
    let slot_idx = slot as usize;
    self.load_info.proc_load_history[slot_idx] =
        self.load_info.proc_load_history[slot_idx].wrapping_add(ready_count as u16);
    self.load_info.last_clock = self.uptime;
}
```

**quantum 递减不在 `tick()` 内（D9）**：C 版在 `context_stop()`（arch_clock.c:326-330，上下文切换路径）按 TSC delta 递减 `p_cpu_time_left`，与 `timer_int_handler()` 分开；Rust 对应 `clock::decrement_quantum()`，由调用方在 `tick()` 返回后单独调用。因此 `TimerTickResult` 没有 `QuantumExpired` 变体——这与 §3.7 的边界声明一致：tick 负责计数、记账与到期处理，quantum 递减与基于它的调度决策属于 `clock::decrement_quantum()` 和 [11-scheduling-primitives.md](11-scheduling-primitives.md) 的范畴。

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
/// | x86-64 | 8254 PIT / LAPIC Timer | PIT divisor | TSC |
/// | aarch64 | Generic Timer | CNTFRQ_EL0 | CNTPCT_EL0 |
/// | riscv64 | CLINT mtime | mtimecmp | mtime |
///
/// 实例化设计（与 [04-platform-discovery.md §3.4](04-platform-discovery.md#34-硬件-trait-为什么要带实例状态) 一致）：
/// 硬件参数（PIT 基准频率、LAPIC 基址、mtime/mtimecmp 地址、定时器频率）通过
/// `new(desc)` 从 `TimerDesc` 子描述符（`Any` downcast）提取并存入实例字段，
/// 替代早期硬编码常量版本（`PIT_BASE_FREQ` 等）。
pub trait ClockArch: Sized + Send + Sync {
    /// 从定时器描述符构造实例，提取硬件参数。
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;

    /// 按给定频率配置并启动硬件定时器。
    ///
    /// 首次在 `init_clock_and_interrupts()` 中配置；`bsp_finish_booting()`
    /// 的 Step 6 会以幂等 no-op 安全网的形式再调用一次（重复写同一组
    /// 寄存器，无副作用）。调用后定时器以 `hz` Hz 产生周期性中断。
    ///
    /// `cpu_id` 是当前 CPU 的编号（RISC-V 上即 hart id）。只有 riscv64
    /// 会用到它：CLINT 是全局 MMIO 块，每个 hart 的 `mtimecmp` 位于
    /// `mtimecmp_base + cpu_id * mtimecmp_stride`，必须按 hart 寻址才能
    /// 配置本核的定时器。x86-64（PIT + 每核 LAPIC）与 aarch64
    /// （CNTP_* 系统寄存器）天然访问当前 CPU 自己的定时器，忽略该参数。
    ///
    /// C: init_clock() 硬件部分 + arch_init() APIC timer
    fn init_timer(&mut self, hz: u32, cpu_id: u32);

    /// 读取当前硬件 tick 计数。
    ///
    /// 用于精细计时和性能分析。
    fn read_ticks(&self) -> u64;

    /// 读取高分辨率 TSC 计数。默认委托 `read_ticks()`。
    ///
    /// C: read_tsc() — Minix3 中没有，但是 x86-64 标准做法
    fn read_tsc(&self) -> u64 { self.read_ticks() }

    /// 停止本地定时器（SMP AP 停顿时使用）。
    ///
    /// C: smp.c:56-61 — `lapic_stop_timer()` 内联于 `smp_ipi_halt_handler`
    ///
    /// `cpu_id` 与 `init_timer` 相同：riscv64 用它定位本 hart 的
    /// `mtimecmp`（写 `u64::MAX` 屏蔽中断），其余架构忽略。
    fn stop_local_timer(&mut self, cpu_id: u32);

    /// 初始化统计 profiling 时钟。
    ///
    /// C: sprofile.c:init_profile_clock(freq)
    fn init_profile_clock(&mut self, hz: u32) -> Result<(), ProfileClockError>;

    /// 停止统计 profiling 时钟。
    ///
    /// C: sprofile.c:stop_profile_clock()
    fn stop_profile_clock(&mut self);

    /// 确认统计 profiling 时钟中断（x86 读 RTC 寄存器 C 清除 IRQ）。
    ///
    /// C: arch_ack_profile_clock() — profile.c:123
    fn ack_profile_clock(&mut self);
}
```

### 4.3 x86-64 ClockArch 实现

```rust
/// x86-64 时钟，boot 阶段用 8254 PIT，运行时用 LAPIC Timer。
///
/// C: clock.c 硬件初始化 + apic.c lapic_enable()
pub struct X86_64ClockArch {
    /// 8254 PIT 基准频率，单位 Hz（来自 `PitDesc`）。
    pit_base_freq: u32,
    /// LAPIC MMIO 基地址（来自 `PitDesc`）。
    lapic_base: usize,
}

/// PIT 命令端口。
const PIT_COMMAND: u16 = 0x43;
/// PIT channel 0 数据端口。
const PIT_CHANNEL0: u16 = 0x40;
/// PIT 命令：channel 0，低/高字节访问，速率发生器（rate generator）模式。
const PIT_CMD_RATE_GEN: u8 = 0x36;

impl ClockArch for X86_64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        // 从 PitDesc 提取硬件参数（实例化设计，见 §3.4）。
        let pit = desc
            .as_any()
            .downcast_ref::<PitDesc>()
            .expect("X86_64ClockArch::new: expected PitDesc");
        Self {
            pit_base_freq: pit.pit_base_freq,
            lapic_base: pit.lapic_base,
        }
    }

    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // 把 8254 PIT channel 0 配置为周期性模式。
        // C: intr_init_8254() — i8259.c 等价实现
        //
        // PIT divisor 是 16 位，因此 hz 必须 >= 19 (1193182 / 65535 ≈ 18.2)。
        // 低于 19 会导致 divisor 溢出。
        //
        // PIT 是系统级设备；LAPIC 定时器（`stop_local_timer` 用到）在每个
        // CPU 上看到的是同一个 MMIO 基址——各 CPU 的 LAPIC 是"同址异核"的，
        // 因此不需要 `cpu_id`。
        assert!(hz >= 19, "PIT divisor overflow: hz must be >= 19, got {}", hz);

        let divisor = (self.pit_base_freq / hz) as u16;

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

    fn read_ticks(&self) -> u64 {
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
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        // ArmGenericTimerDesc 不携带数据（频率运行时从 CNTFRQ_EL0 读取），
        // new() 仅验证类型。结构体仅用于满足实例化 trait 契约。
        desc.as_any()
            .downcast_ref::<ArmGenericTimerDesc>()
            .expect("AArch64ClockArch::new: expected ArmGenericTimerDesc");
        Self
    }

    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // ARM Generic Timer 由固件（TF-A/U-Boot）配置好。
        // 这里只需使能 EL1 physical timer 并设置比较值。
        //
        // CNTP_CVAL_EL0 / CNTP_CTL_EL0 是每核系统寄存器，天然指向当前
        // CPU 自己的定时器，因此不需要 `cpu_id`。
        let freq: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
        }
        // ARM Generic Timer 是 absolute compare 模式（CNTP_CVAL_EL0
        // 是绝对值，不是 delta）。读当前 count，再加上 freq/hz 作为
        // 下次触发点。CNTP_CTL_EL0 bit 0 = enable, bit 1 = IMASK（屏蔽）。
        let now: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) now);
        }
        let compare = now + freq / hz as u64;
        unsafe {
            // 设置绝对比较值
            core::arch::asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // 使能定时器（ENABLE=1, IMASK=0, ISTATUS=0）
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
        }
    }

    fn read_ticks(&self) -> u64 {
        let count: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) count);
        }
        count
    }
}
```

### 4.5 riscv64 ClockArch 实现

Minix3 没有 RISC-V 移植，本实现是全新架构（`架构演进`，见 §3.8）：直接用 CLINT mtime/mtimecmp（MMIO，M-mode 固件如 OpenSBI 已映射）。

```rust
/// RISC-V 64 位时钟，使用 CLINT mtime。
///
/// C: 无 Minix3 对应实现（Minix3 没有 RISC-V 端口 架构演进，见 §3.8）。
pub struct Riscv64ClockArch {
    /// CLINT mtime 寄存器 MMIO 地址（来自 `ClintDesc`）。
    mtime_addr: usize,
    /// CLINT mtimecmp 基地址（hart 0，来自 `ClintDesc`）。
    /// 当前 hart 的比较器位于 `mtimecmp_base + hart_id * mtimecmp_stride`。
    mtimecmp_base: usize,
    /// 相邻两个 hart 的 mtimecmp 之间的字节间距（来自 `ClintDesc`）。
    mtimecmp_stride: usize,
    /// mtime 计数器频率，单位 Hz（来自 `ClintDesc`）。
    freq: u64,
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        // 从 ClintDesc 提取硬件参数（实例化设计，见 §3.4），
        // 替代早期硬编码的 QEMU virt 地址/频率常量。
        let clint = desc
            .as_any()
            .downcast_ref::<ClintDesc>()
            .expect("Riscv64ClockArch::new: expected ClintDesc");
        Self {
            mtime_addr: clint.mtime_addr,
            mtimecmp_base: clint.mtimecmp_base,
            mtimecmp_stride: clint.mtimecmp_stride,
            freq: clint.freq,
        }
    }

    fn init_timer(&mut self, hz: u32, cpu_id: u32) {
        // 读取当前 mtime 值
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(self.mtime_addr as *const u64);
        }

        // 计算两次中断之间的间隔
        let interval = self.freq / hz as u64;

        // CLINT 每个 hart 有一份 mtimecmp，调用方传入当前 hart id，
        // 各核配置自己的比较器，而不是都写 hart 0 的。
        // C: app_cpu_init_timer() — clock.c:306
        let mtimecmp_addr = self.mtimecmp_base + (cpu_id as usize) * self.mtimecmp_stride;

        // 设置 mtimecmp = mtime + interval，安排第一次中断
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(mtimecmp_addr as *mut u64, mtimecmp);
        }

        // 使能 S-mode 定时器中断（sie 中的 STIE 位）
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn read_ticks(&self) -> u64 {
        unsafe {
            core::ptr::read_volatile(self.mtime_addr as *const u64)
        }
    }
}
```

### 4.6 aarch64 InterruptController 实现（GICv3）

Minix3 ARM（32 位）用 OMAP INTC（`omap_intr.c`），本实现针对 ARMv8-A GICv3（`架构演进`，见 §3.8）：GICD（Distributor）+ GICR（Redistributor）+ CPU interface。

```rust
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
/// GICR_WAKER.ChildrenAsleep 位（只读）。
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 中断控制器。
pub struct AArch64InterruptController {
    /// GIC Distributor MMIO 基址（QEMU virt 0x0800_0000，来自 Gicv3Desc）。
    gicd_base: usize,
    /// GIC Redistributor MMIO 基址（QEMU virt 0x080A_0000，来自 Gicv3Desc）。
    gicr_base: usize,
    nr_irqs: usize,
    /// 上次应答的中断 ID（从 ICC_IAR1_EL1 读取保存）。
    last_iar: u32,
}

impl AArch64InterruptController {
    // 私有构造：仅由 `InterruptController::new(desc)` 调用。
    // `set_base()` 不再保留——所有参数均从 `desc.downcast_ref::<Gicv3Desc>()` 获取。
}

impl InterruptController for AArch64InterruptController {
    /// 从 `InterruptControllerDesc` 单步构造（Gicv3Desc downcast 提取 gicd_base / gicr_base）。
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let gicv3 = desc
            .as_any()
            .downcast_ref::<Gicv3Desc>()
            .expect("AArch64InterruptController::new: expected Gicv3Desc");
        Self {
            gicd_base: gicv3.gicd_base,
            gicr_base: gicv3.gicr_base,
            nr_irqs: (gicv3.nr_irqs as usize).min(NR_IRQ_VECTORS),
            last_iar: 0,
        }
    }

    fn init(&mut self) {
        // 防御：若 desc 未提供非零 base，拦截此错误（不应到达此分支）。
        assert!(self.gicd_base != 0, "AArch64InterruptController: gicd_base is zero (descriptor did not provide GICD base)");
        assert!(self.gicr_base != 0, "AArch64InterruptController: gicr_base is zero (descriptor did not provide GICR base)");
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
        // 3. 使能 Distributor（Group 1 Non-secure）。
        //    只置 EnableGrp1NS 位：GICD_CTLR 还含固件配置的位
        //    （如 ARE_NS/ARE_S，bit 31:30，选择 affinity routing），
        //    整寄存器写会清掉它们，真机上具破坏性 —— 读-改-写保留。
        let ctlr = self.gicd_read32(GICD_CTLR) | GICD_CTLR_ENABLE_GRP1NS);
        self.gicd_write32(GICD_CTLR, ctlr);
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

### 4.7 TimerIrqGate trait：定时器 IRQ 的 enable/disable

**位置**: [`os/arch/src/arch/timer_irq_gate.rs`](os/arch/src/arch/timer_irq_gate.rs)（trait 定义）+ [`os/arch/src/x86_64/timer_irq_gate.rs`](os/arch/src/x86_64/timer_irq_gate.rs) / [`os/arch/src/arm64/timer_irq_gate.rs`](os/arch/src/arm64/timer_irq_gate.rs) / [`os/arch/src/riscv64/timer_irq_gate.rs`](os/arch/src/riscv64/timer_irq_gate.rs)（三架构 impl）+ `os/arch/src/lib.rs` `CurrentTimerIrqGate` alias

**职责**：本 trait 控制 timer 中断**在 CPU 入口的硬件投递**——允许或屏蔽 timer 触发的中断到达 trap 入口。对应 C 中的 timer IRQ 开关硬件位：

- x86：LAPIC LVT Timer mask bit（offset `0x320` bit 16）
- aarch64：`CNTP_CTL_EL0`（`Enable=1, IMASK=0` ↔ `Enable=0, IMASK=1`）
- riscv64：`sie.STIE`（CSR `sie` bit 5）

**职责边界（正交性原则）**：本 trait 只控制"是否允许 timer 中断到达 CPU 入口"，**不包含** handler 注册——handler 与 trap 入口的绑定由 `IrqManager::register_hook` 完成（见 `os/kernel/src/lib.rs` `bsp_finish_booting` Step 6，与 `bsp_init_clock` 的 `register_local_timer_handler`，`clock.c:294` / `arch_clock.c:177` 同源）。两件事各自独立：timer IRQ enable/disable 与 handler 注册是两条正交链路，调用顺序与归属彼此无关。

```rust
pub trait TimerIrqGate: Sized {
    fn enable_timer_irq();
    fn disable_timer_irq();
}
```

**三架构实现**（per-arch ZST，经 `CurrentTimerIrqGate` cfg alias 静态分派，不向使用方泄漏 `#[cfg]`）：

| 架构 | impl 类型 | enable_timer_irq | disable_timer_irq | C 源码 |
|------|----------|------------------|-------------------|--------|
| x86_64 | `X86_64TimerIrqGate` | 清 LAPIC LVT Timer Mask bit (offset 0x320, bit 16) + 置 LAPIC SVR Enable bit (offset 0xF0, bit 8) | 置 LAPIC LVT Timer Mask bit | `arch_clock.c:177`（APIC 路径）+ `apic.c:44` / `apic.c:475-477`（LVT Mask）+ `apic.c:lapic_enable()`（SVR Enable 职责归属见 §4.7.1） |
| aarch64 | `AArch64TimerIrqGate` | `msr CNTP_CTL_EL0, 1` (Enable=1, IMASK=0) + `isb` | `msr CNTP_CTL_EL0, 2` (Enable=0, IMASK=1) + `isb` | `earm/arch_clock.c:182`（BSP 转发）+ `bsp/ti/omap_intr.c:22-44`（32 位 ARM；aarch64 为架构演进 架构演进，见 §2.5） |
| riscv64 | `Riscv64TimerIrqGate` | `csrs sie, 0x20` (STIE=bit 5) | `csrc sie, 0x20` | **（Minix3 无 riscv64 移植 架构演进；对标 RISC-V Privileged Spec 1.12 §4.1.3 Supervisor Interrupt Registers）** |

**设计要点**：

- **关联函数（无 `&self`）、无实例字段**：`TimerIrqGate` 与 `TrapEntryArch` / `ProtectionArch` 同类（纯 static 方法），trait bound 仅 `Sized`——`Send + Sync` 由 trait 中的实例字段触发，有实例字段的 `ClockArch` / `FpuArch` 才需要
- **静态分派**：调用点写 `<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq()`；target-specific `cfg` 只存在于 `os/arch/src/lib.rs` 的 `CurrentTimerIrqGate` alias（照抄 `CurrentArchInit` 模式）
- **x86_64 LAPIC base 探测**：从 IA32_APIC_BASE MSR (`0x1B`) 读取 base，同时检查 APIC global enable bit（bit 11）；未启用时 `panic!`——这表明调用方跳过了 `X86_64InterruptController::init` 中的 LAPIC 启用步骤
- **riscv64 不调 SBI**：`ClockArch::init_timer` 直接写 CLINT mtimecmp MMIO（M-mode 固件如 OpenSBI 已映射该 MMIO；S-mode 下未通过 ecall）。`TimerIrqGate` 只控制 S-mode 中断 enable (sie.STIE)，是 supervisor CSR write

#### 4.7.1 aarch64 真硬件路径与职责边界

aarch64 timer IRQ 投递链是 CNTP（per-CPU timer 模块）→ PPI → GIC redistributor → CPU。`CNTP_CTL_EL0.Enable` 仅让 per-CPU timer 模块产生 PPI；PPI 能否到达 CPU 取决于 GIC delivery path（`GICD_CTLR.EnableGrp1NS` + `GICR_WAKER` wake + `ICC_IGRPEN1_EL1=1`），由 `AArch64InterruptController::init` 建立（`os/plat/src/arm64/interrupt.rs`）。

因此本 trait 的职责边界为：

- `TimerIrqGate` **不写 ICC_IGRPEN1_EL1**——GIC global enable 属于中断控制器初始化（`InterruptController::init`，与 `SmpArch::init_ap` 仅负责 IPI（GICD_SGIR / ICC_EOIR）的职责正交）
- 与 x86 LAPIC 同理：x86 SVR Enable（`SVR` bit 8）属于 LAPIC 初始化（`apic.c:lapic_enable()`；Rust 中由 `X86_64InterruptController::init_lapic` 完成），LVT Timer Mask（`0x320` bit 16）属于 `TimerIrqGate`。当前 `X86_64TimerIrqGate::enable_timer_irq` 同时写两处（与 `init_lapic` 重复但幂等），拆分时机由后续 LAPIC 初始化时序设计统一处理。

#### 4.7.2 handler 注册不在此 trait（YAGNI）

`TimerIrqGate` 不提供 handler 注册。handler 与 trap 入口的绑定由 `IrqManager::register_hook` 完成（`os/kernel/src/lib.rs` `bsp_finish_booting`，与 `bsp_init_clock` 的 `register_local_timer_handler`，`clock.c:294` / `arch_clock.c:177` 同源）。**理由**：handler register 的硬件 wiring 在三架构上形态不一（x86 IOAPIC RTE 绑定、aarch64 CNTP LVT 设置、riscv64 SBI timer dispatch），但真硬件 binding 出现之前没有任何真实读者。在差异出现之前预建抽象只会得到"未实现的一致"，抽象形态由届时差异本身决定。代码中保留 `os/arch/src/arch/timer_irq_gate.rs` 顶部 doc-comment 一行 breadcrumb 指向本节作为定位指针。

### 4.8 riscv64 InterruptController 实现（PLIC）

Minix3 没有 RISC-V 移植，PLIC 实现对标 RISC-V PLIC 规范（`架构演进`，见 §3.8）：外部设备中断经 PLIC 路由，时钟/软件中断由 CLINT 负责（`ClockArch` 管理）。

```rust
/// PLIC 寄存器偏移。
const PLIC_PRIORITY: usize = 0x0000;
const PLIC_ENABLE: usize = 0x2000;
const PLIC_THRESHOLD: usize = 0x200000;
const PLIC_CLAIM: usize = 0x200004;
/// complete 寄存器与 claim 共用同一偏移：读 = claim，写 = complete。
const PLIC_COMPLETE: usize = PLIC_CLAIM;

/// RISC-V 64 位 PLIC 中断控制器。
pub struct Riscv64InterruptController {
    plic_base: usize,
    nr_irqs: usize,
    /// 当前 hart 的 S-mode context ID（来自 PlicDesc；QEMU virt S-mode = 1）。
    context: usize,
    /// 上次 claim 的中断 ID（从 claim 寄存器读取保存）。
    last_claimed: u32,
}

impl Riscv64InterruptController {
    // 私有构造：仅由 `InterruptController::new(desc)` 调用。
    // `set_base()` 不再保留——PLIC base 由 `desc.downcast_ref::<PlicDesc>()` 提供。
}

impl InterruptController for Riscv64InterruptController {
    /// 从 `InterruptControllerDesc` 单步构造（PlicDesc downcast 提取 plic_base）。
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let plic = desc
            .as_any()
            .downcast_ref::<PlicDesc>()
            .expect("Riscv64InterruptController::new: expected PlicDesc");
        Self {
            plic_base: plic.plic_base,
            nr_irqs: (plic.nr_irqs as usize).min(NR_IRQ_VECTORS),
            context: plic.context as usize,
            last_claimed: 0,
        }
    }
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

> **实现说明**: PLIC 已实现，base 由 `PlicDesc.plic_base` 提供（QEMU virt = 0x0C00_0000）。Timer 中断由 CLINT 处理（见 `clock.rs`），不在 PLIC 路径。

### 4.9 ArchInit trait

> 设计决策：§3.5（ArchInit trait）、§3.6（不做内存裁剪）

```rust
/// 在中断控制器初始化之后执行的架构特定初始化。
///
/// 这个 trait 封装了 Minix3 C 的 `arch_init()` 函数，
/// 负责在保护结构和中断控制器初始化完成后进行硬件相关设置。
///
/// 实例化设计：每个架构是一个**带字段**的类型，由 `new(desc)` 从 `ArchMiscDesc`
/// 构造，再调用 `init(&mut self)`。带字段而非 ZST 是因为——
///   - 部分实现需要从 `ArchMiscDesc` 读取运行时参数（如 x86 ACPI 表地址、
///     aarch64 PMU cycle counter 是否可用），这些无法在编译期固化。
///   - trait bound `Send + Sync` 隐含了"拥有非 `Copy` 数据的所有权要求"。
///
/// C: arch_init() — arch/i386/arch_system.c:246 / earm/arch_system.c:101
pub trait ArchInit: Sized + Send + Sync {
    /// 从 `ArchMiscDesc` 构造实例。
    ///
    /// 不同架构从 `desc` 读取不同字段（`desc.acpi_tables()` /
    /// `desc.pmu_cycle_counter()` 等），因此签名固定为 `&ArchMiscDesc`。
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self;

    /// 执行架构特定初始化。
    ///
    /// 在 `init_clock_and_interrupts()` 中调用一次，位于 `init_clock()` 和
    /// `intr_init()` 完成之后。
    fn init(&mut self);
}
```

**调用方式**：

```rust
use minix_arch::{ArchInit, CurrentArchInit};
let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
arch_init.init();
```

### 4.10 x86-64 ArchInit 实现

```rust
pub struct X86_64ArchInit {
    /// ACPI 表基地址（由 `ArchMiscDesc.acpi_tables()` 提供）。
    /// QEMU virt 启动阶段不使用；支持物理机时按 RSDP 搜索结果填入。
    acpi_tables: Option<usize>,
}

impl ArchInit for X86_64ArchInit {
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self {
        Self {
            acpi_tables: desc.acpi_tables(),
        }
    }

    fn init(&mut self) {
        // 1. 每 CPU 内核栈（由链接脚本处理）

        // 2. ACPI 表解析（当前未启用；QEMU virt 不依赖 ACPI）
        if let Some(rsdp_addr) = self.acpi_tables {
            // TODO: 解析 RSDP → RSDT/XSDT → MADT（FADT/MCFG/...）
            //       见 [08-system-init-boot-finish.md §4.x]
            let _ = rsdp_addr;
        }

        // APIC 初始化由 X86_64InterruptController::init() 完成。
        // COM1 串口初始化由 X86_64EarlyConsole::init() 完成。
    }
}
```

### 4.11 aarch64 ArchInit 实现

```rust
pub struct AArch64ArchInit {
    /// 是否使能 PMU cycle counter（用户态 `gettime()` 性能计数器读依赖）。
    /// 由 `ArchMiscDesc.pmu_cycle_counter()` 决定。
    pmu_cycle_counter: bool,
}

impl ArchInit for AArch64ArchInit {
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self {
        Self {
            pmu_cycle_counter: desc.pmu_cycle_counter(),
        }
    }

    fn init(&mut self) {
        // C: arch_init() — earm/arch_system.c:101-132

        // 1. PMU cycle counter（用户态可见）
        //    仅在 `pmu_cycle_counter == true` 时初始化；当前所有平台描述符
        //    （含 QEMU virt）均为 `false`，本阶段实际不初始化。C 版
        //    earm/arch_system.c:101-132 无条件启用，描述符门控是刻意设计。
        // C: PMU_PMCR_E + PMU_PMCNTENSET_C + PMU_PMUSERENR_EN
        if self.pmu_cycle_counter {
            unsafe {
                // PMCR：E（使能）+ C（复位事件计数器）+ P（复位 cycle counter）= 0x7
                core::arch::asm!("msr pmcr_el0, {}", in(reg) 0x7u64);
                // PMCNTENSET：C 位（bit 31）使能 cycle counter
                core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);
                // PMUSERENR：EN 位（bit 0）— 允许 EL0（用户态）访问
                core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
            }
        }

        // 2. 板级相关初始化（bsp_init）
        //    平台相关设置（例如 GIC 基地址发现）在正式硬件上需补充。
        //    当前 QEMU virt 中，GIC base 由 `AArch64InterruptController::new()` 从
        //    `desc.interrupt_controller()` 单独传入，不经过此路径。
    }
}
```

> **实现说明**: PMU 三个寄存器 (PMCR_EL0 / PMCNTENSET_EL0 / PMUSERENR_EL0) 已实现；`bsp_init` 中的 GIC base 由 `AArch64InterruptController::new(desc)` 从 `Gicv3Desc` 单步构造传入，不在 `ArchInit::init()` 路径内。

### 4.12 riscv64 ArchInit 实现

```rust
/// riscv64 无需外部运行时参数 —— riscv64 是 ZST。
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn new(_desc: &minix_platform::ArchMiscDesc) -> Self {
        Self
    }

    fn init(&mut self) {
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

### 4.13 init_clock_and_interrupts() 实现

> 设计决策：§3.1~§3.6、§4.1（实例化 trait）

```rust
/// cstart 的第二阶段：初始化时钟和中断控制器。
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-475
fn init_clock_and_interrupts() {
    use minix_arch::{ClockArch, ArchInit};
    use minix_platform::{platform_desc, PlatformDesc};

    // 平台描述符在 boot 早期已由 KernelInfo 初始化。
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
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    // 实例化设计：从 interrupt_controller 描述符构造 InterruptController，再调用 init。
    let mut intr = CurrentInterruptController::new(&pd.interrupt_controller());
    intr.init();  // 内部会调用 mask_all()

    // 步骤 3.5：将中断控制器存入全局 IRQ_MANAGER，而不是 init 后丢弃。
    // trap 入口与 bsp_finish_booting 需要经 IRQ_MANAGER 访问中断控制器
    // （IrqManager::register_hook 负责把 handler 绑定到 trap 入口）。
    // SAFETY: boot 阶段在 BKL 存在前是单线程，无并发访问。
    unsafe {
        *IRQ_MANAGER.get() =
            Some(crate::irq_manager::IrqManager::new(intr));
    }

    // 步骤 4：架构特定初始化。
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    // 实例化设计：从 arch_misc 描述符构造 ArchInit，再调用 init。
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();

    let _ = clock; // `hz()` 已消费，后续接入时钟子系统
}
```

### 4.14 EarlyConsole 实现（x86-64）

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

fn com1_write_byte(byte: u8) {
    unsafe {
        while (inb(COM1_BASE + 5) & 0x20) == 0 {}
        outb(COM1_BASE, byte);
    }
}

pub struct X86_64EarlyConsole;

impl EarlyConsole for X86_64EarlyConsole {
    fn init() {
        // SAFETY：只在 BSP 上、串口 I/O 使能前运行。
        unsafe { ser_init(); }
    }

    fn write_byte(byte: u8) {
        // 转发给模块私有 helper（忙等 + outb 在 com1_write_byte 内）。
        com1_write_byte(byte);
    }
}
```

> **实现要点**：COM1 字节写入统一由模块私有函数 `com1_write_byte` 承担（忙等 THR 空 + `outb`）。trait 方法 `write_byte` 直接转发给它，`write_str` / `write_hex` 内部同样走 `com1_write_byte`——helper 命名为 `com1_write_byte` 而非与 trait 方法同名的 `write_byte`，避免名称解析歧义。

`kmain()` 和 `kmain_verify()` 在入口首先调用 `CurrentEarlyConsole::init()`，随后所有诊断输出都走 `CurrentEarlyConsole::write_*`。

---

## 5. 测试要点

测试当前覆盖**单元测试**（验证软件状态机逻辑，无硬件依赖）。QEMU + GDB 硬件集成测试脚本（验证 8254 PIT / Generic Timer / CLINT 等硬件寄存器配置）已存在于 `os/arch/tests/qemu_test_{x86_64,aarch64,riscv64,x86_64_procd}.sh`（含 QEMU/GDB 前置检查、GDB checkpoint、退出码 0/1/2），但运行需要目标 `kernel.elf` 构建产物与 QEMU/GDB 环境，端到端执行尚未接入 CI——见 §5.2 末尾"未覆盖（后续文档）"。

### 5.1 单元测试

单元测试位于多个 `#[cfg(test)]` 模块中，按文件分布如下：

| 文件 | 测试数 | 验证内容 |
|------|-------|---------|
| `os/arch/src/arch/clock.rs` | 3 | `ClockArch` trait 默认方法（`read_tsc` 委托）+ `DEFAULT_HZ` / `LOAD_HISTORY_SIZE` 常量 |
| `os/kernel/src/clock.rs` | 44 | `ClockState` / `LoadInfo` / adjtime / billp 记账 / 虚拟定时器 / 定时器队列 / load 更新 / decrement_quantum（详见 §5.1.1）|
| `os/plat/src/interrupt.rs` | 0 | 原 10 个 newtype/bitflags/enum 派生属性测试为 Pattern #38 自指测试，已删除。Newtype/bitflags/enum 正确性由类型签名、宏、派生 trait 编译期保证；真实行为测试在各架构 `interrupt.rs` 的 `test_new_from_*_descriptor` 中。 |
| `os/plat/src/x86_64/interrupt.rs` | 2 | `X86_64InterruptController::new()` 构造（验证 `lapic_base` / `ioapic_base` 字段注入 + clamp 边界）|
| `os/plat/src/x86_64/early_console.rs` | 0 | 原 6 个 COM1 寄存器常量自指测试已删除（常量在声明位置天然正确）。运行时初始化需待 QEMU 集成测试 |
| `os/arch/src/x86_64/arch_init.rs` | 0 | 串口初始化由 `EarlyConsole` 负责；ACPI 解析尚未实现测试 |
| `os/plat/src/arm64/interrupt.rs` | 1 | `AArch64InterruptController::new()` 构造 |
| `os/plat/src/riscv64/interrupt.rs` | 2 | `Riscv64InterruptController::new()` 构造 |
| `os/arch/src/x86_64/trap_entry.rs` | 14 | IDT 门描述符 `size_of`、set_handler 行为、IDT init、STAR/LSTAR/SFMASK/SYSCALL/SYSRET layout（详见 doc 03 §5.3）|
| `os/arch/src/{arm64,riscv64}/trap_entry.rs` | 1 + 1 | 各 1 个编译期 trait bound 检查 (`_check<T: TrapEntryArch>()`)。原 4 个 unit-struct 烟雾测试已迁移为编译期约束 |
| `os/plat/src/early_console.rs` | 0 | trait 声明 `init`/`write_byte` 接口；`write_str`/`write_hex` 为默认方法，目前通过 `os/plat/src/mock.rs` 的 `MockEarlyConsole` 在测试构建中复用 |
| `os/arch/src/arch/timer_irq_gate.rs` | 4 | `test_current_timer_irq_gate_compiles`（`CurrentTimerIrqGate` alias 编译检查）+ 三架构 `test_*_timer_irq_gate_compiles`（按 `target_arch` 门控，各验证一次硬件 impl 编译通过）；真硬件行为需待 QEMU + GDB 集成测试脚本（见 §5.2 "未覆盖"） |
| **当前所列文件总计** | **72** | |

> **统计口径说明**：§5.1 上方表格按**文件**维度统计每个 `#[cfg(test)]` 模块的 `#[test]` 函数总数；§5.2 表格按**功能**维度分类统计。各架构 `interrupt.rs` 在 §5.1 含全部测试（如 aarch64 1 = 构造），§5.2 仅计入"该架构中断控制器构造"维度。
>
> **2026-08-16 P0 重构说明**：原 doc 声称 100 tests；本次 review 发现 48 个为虚假/烟雾测试（Pattern #38 自指测试 + unit struct noop 烟雾测试），删除后真实行为测试数 = 72。详见 `.review/codex/01-stage-kernel/05-clock-interrupt-init/test-audit.md`。

#### 5.1.1 时钟状态与定时器测试（`os/kernel/src/clock.rs`）

> 注：`ClockState` / `LoadInfo` 的权威定义位于 `os/kernel/src/clock.rs`（内核态时钟状态机，与 `os/arch/src/arch/clock.rs` 的 `ClockArch` 硬件抽象分离，分层决策见 §3.1）；`DEFAULT_HZ` / `LOAD_HISTORY_SIZE` 的权威定义位于 `os/arch/src/arch/clock.rs:43,49`，`os/kernel/src/clock.rs:474` 的同值 `DEFAULT_HZ` 与私有 `LOAD_HISTORY`（L487）是独立副本（修改先改 arch 再 sync，见 §3.2）。`arch/clock.rs` 的 3 个测试（`test_default_hz_value` / `test_load_history_size` / `test_read_tsc_default_delegates_to_read_ticks`）验证的正是 arch 层常量与 `read_tsc` 默认委托。

**ClockState 初始化与 tick 行为**：

| 测试 | 验证内容 |
|------|---------|
| `test_clock_state_init` | `ClockState::new()` 初始 `hz=100`、`uptime=0`、`realtime=0` |
| `test_clock_state_custom_hz` | 自定义 `hz` 构造 |
| `test_clock_state_hz_bounds` | `hz` 边界校验 |
| `test_ap_clock_state_no_global_time` | AP 时钟状态不更新全局时间 |
| `test_tick_bsp_increments_uptime_realtime` | BSP tick 递增 uptime/realtime |
| `test_tick_ap_no_uptime_update` | AP tick 不更新 uptime |

**adjtime 调速**：

| 测试 | 验证内容 |
|------|---------|
| `test_adjtime_speed_up` | 正 delta：`realtime` 增长快于 `uptime` |
| `test_adjtime_slow_down` | 负 delta：`realtime` 增长慢于 `uptime` |
| `test_adjtime_zero_delta` | delta=0：`realtime == uptime` |

**billp 记账**：

| 测试 | 验证内容 |
|------|---------|
| `test_billp_sys_time_accounting` | 系统时间记账 |
| `test_billp_no_accounting_when_none` | 无进程时不记账 |
| `test_billp_prof_timer_decrement` | profiling 定时器递减 |
| `test_billp_prof_timer_expiry_reports_prof` | profiling 定时器到期上报 PROF |

**虚拟定时器（vtimer）**：

| 测试 | 验证内容 |
|------|---------|
| `test_vtimer_virtual_expiry` | 虚拟时间到期 |
| `test_vtimer_prof_expiry` | profiling 时间到期 |
| `test_vtimer_no_expiry_when_flag_not_set` | flag 未设置时不触发 |

**定时器队列**：

| 测试 | 验证内容 |
|------|---------|
| `test_timer_set_and_expire` | set + expire 基本路径 |
| `test_tick_with_invokes_callback_on_expiry` | tick 时到期定时器触发回调 |
| `test_tick_with_no_allocation_on_empty_expiry` | 空到期队列不分配 |
| `test_tick_with_matches_tick_behavior` | `tick_with` 与 `tick` 行为一致 |
| `test_timer_reset_by_id` | 按 id reset |
| `test_timer_id_uniqueness` | 定时器 id 唯一性 |
| `test_timer_queue_same_exp_time` | 同时到期的排队顺序 |
| `test_multiple_timers_pop_order` | 多定时器弹出顺序 |
| `test_timer_never_expires` | 永不到期定时器 |
| `test_timer_queue_insert_remove` | 队列插入/移除 |
| `test_timer_queue_pop_expired_order` | 到期弹出顺序 |
| `test_ap_state_set_timer_panics` | AP 状态 set_timer panic |
| `test_ap_state_reset_timer_returns_none` | AP 状态 reset_timer 返回 None |

**load 更新与 boottime**：

| 测试 | 验证内容 |
|------|---------|
| `test_load_update_accumulates` | `LoadInfo` 槽位累加 |
| `test_load_update_slot_rotation` | 槽位轮转 |
| `test_set_boottime` | 设置 boottime |
| `test_set_realtime` | 设置 realtime |
| `test_ap_set_boottime_no_op` | AP 设置 boottime 为 no-op |

**decrement_quantum 与 misc**：

| 测试 | 验证内容 |
|------|---------|
| `test_decrement_quantum_no_smp_state_returns_false` | 无 SMP 状态返回 false |
| `test_decrement_quantum_first_call_no_baseline` | 首次调用无 baseline |
| `test_decrement_quantum_decrements_cpu_time_left` | 递减剩余 CPU 时间 |
| `test_decrement_quantum_reports_exhaustion` | 时间耗尽上报 |
| `test_decrement_quantum_overshoot_saturates_to_zero` | 过冲饱和到 0 |
| `test_decrement_quantum_skips_kernel_tasks` | 跳过内核任务 |
| `test_decrement_quantum_zero_delta_returns_false` | delta=0 返回 false |
| `test_decrement_quantum_multiple_ticks_accumulate` | 多 tick 累加 |
| `test_user_time_accounting` | 用户时间记账 |
| `test_read_tsc_returns_zero_in_test_build` | 测试构建中 `read_tsc` 返回 0 |

**运行命令**：
```bash
cd os/kernel && cargo test --lib clock 2>&1
```

#### 5.1.2 IRQ 共享类型测试（`plat/interrupt.rs`）

`os/plat/src/interrupt.rs` 当前**无 `#[test]` 函数**——原 10 个测试（`test_irq_vector_new/const/boundaries/equality` / `test_irq_id_new` / `test_irq_notify_id_new` / `test_irq_policy_reenable/empty` / `test_nr_irq_constants` / `test_irq_action_discriminants`）于 2026-08-16 删除（Pattern #38 自指测试）。

**删除理由**：这些测试断言类型包装器的往返或编译期已保证的属性——
- `IrqVector::new(32).get() == 32` 仅测构造器+访问器的对称 round-trip，由 `pub const fn new` / `pub const fn get` 签名保证
- `bitflags!` 宏的 `bits()` 行为由宏本身保证
- enum `#[derive(...)]` 派生的 `PartialEq` / `Debug` 由宏展开保证

**真实行为覆盖**：
- 各架构 `interrupt.rs` 的 `test_new_from_*_descriptor`（验证 `ApicDesc` / `Gicv3Desc` / `PlicDesc` 字段注入）
- 各架构的 `test_new_clamps_nr_irqs_to_max`（验证 `nr_irqs` clamp 边界）

**取舍说明**：保留这些 self-reference 测试的代价是**虚增测试数 + 误导读者**，收益是零（新行为无法被它们捕获）。文档显式说明这些"理论测试" 删除，使测试数与 doc §5.1 表格保持一致。

### 5.2 测试覆盖分析

| 维度 | 覆盖项 | 覆盖情况 |
|------|--------|---------|
| ClockState 初始化 | init/custom_hz/hz_bounds/AP 无全局时间 | ✅ 4 tests |
| ClockState tick 正常路径 | BSP 递增 uptime/realtime、AP 不更新 | ✅ 2 tests |
| ClockState adjtime 调速 | 正偏差加速 / 负偏差减速 / 零 delta | ✅ 3 tests |
| billp 记账 | 系统时间记账、无进程不记账、prof timer 递减/到期 | ✅ 4 tests |
| 虚拟定时器 | virtual/prof 到期、flag 未设不触发 | ✅ 3 tests |
| 定时器队列 | set/expire/reset、id 唯一性、同时到期顺序、AP panic/None | ✅ 13 tests |
| decrement_quantum | 递减/耗尽/过冲饱和/跳过内核任务/多 tick 累加 | ✅ 8 tests |
| load 更新 | 槽位累加 + 轮转 | ✅ 2 tests |
| boottime/realtime | set_boottime/set_realtime/AP no-op | ✅ 3 tests |
| misc | 用户时间记账、测试构建 read_tsc=0 | ✅ 2 tests |
| 常量值验证 | DEFAULT_HZ, LOAD_HISTORY_SIZE（`arch/clock.rs`） | ✅ 2 tests |
| IRQ 共享类型编译期正确性 | IrqVector / IrqId / IrqNotifyId newtype、`bitflags!` 派生、enum `derive(*)` 行为 | ✅ 编译期保证（原 self-reference 测试已删除，详见 §5.1.2）|
| IRQ 策略/动作 | IrqPolicy / IrqAction bitflags & enum 派生属性 | ✅ 编译期保证（自指测试已删除）|
| x86-64 APIC 构造 | `X86_64InterruptController::new(&InterruptControllerDesc::Apic)` 提取 lapic/ioapic base | ✅ 2 tests (`os/plat/src/x86_64/interrupt.rs`) |
| aarch64 GICv3 构造 | `AArch64InterruptController::new(&InterruptControllerDesc::Gicv3)` 提取 distributor/cpuif base | ✅ 1 test (`os/plat/src/arm64/interrupt.rs`) |
| riscv64 PLIC 构造 | `Riscv64InterruptController::new(&InterruptControllerDesc::Plic)` 提取 base | ✅ 2 tests (`os/plat/src/riscv64/interrupt.rs`) |
| Mock 中断控制器 | `MockInterruptController` 提供 `InterruptController` trait 空实现，供单元测试编译 | ✅ (`os/plat/src/mock.rs`) |
| trap_entry 表层 (x86-64/arm64/riscv64) | IDT 门描述符大小与字段、`set_handler` 行为、`STAR`/`LSTAR`/`SFMASK` 寄存器值、VBAR/stvec 配置、PIC/syscall/IPC vector 初始化（详见 doc 03 §5.3） | ✅ 14 + 1 + 1 = 16 tests |

**未覆盖（后续文档）**：
- Load average 计算（`kloadinfo`）测试 — 槽位更新已测（`test_load_update_*`），计算逻辑属于调度器模块
- 时钟中断处理程序（`timer_int_handler`）测试 — 属于异常/中断处理文档（doc 15-clock-timer）
- QEMU + GDB 硬件集成测试 — `os/arch/tests/qemu_test_*.sh` 四个脚本已实现（QEMU/GDB 前置检查、GDB checkpoint、退出码 0/1/2），但尚未端到端运行：需目标 `kernel.elf` 构建产物与 QEMU/GDB 环境；运行后覆盖 8254 PIT / Generic Timer / CLINT / GICv3 等硬件寄存器配置的真实行为

---

## 6. 参见

- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 更高半内核映射（clock 初始化的前置条件：内核虚拟地址 + 物理地址映射就绪后才可启动 timer）
- [03-kmain-cstart.md](03-kmain-cstart.md) — cstart 前半段：保护模式初始化
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 进程表初始化和 boot 进程加载
- [16-smp.md](16-smp.md) — SMP 启动和 AP 引导（与 §3.7 `bsp_finish_booting` 范围声明呼应）
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/plat/src/interrupt.rs` — `InterruptController` trait 定义
- `os/plat/src/x86_64/interrupt.rs` — x86-64 APIC 实现
- `os/plat/src/arm64/interrupt.rs` — aarch64 GICv3 实现
- `os/plat/src/riscv64/interrupt.rs` — riscv64 PLIC 实现
- `os/kernel/src/clock.rs` — ClockState 实现
