# 05 · 时钟与中断初始化：让时间开始流动，同时先别接收中断

> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:struct Name`、`path:const NAME`），不使用行号；正文里出现的行号（调用链快照、命令预期输出、引文题注的 L 区间）是当期快照数据，不作定位锚点。
> **本篇主题**：`cstart` 后半段与它在 Rust 侧的对位——软件时钟状态、中断控制器、架构杂项三件事怎么建立，硬件定时器为什么不在这一步启动，以及"放开一条中断线"需要先凑齐哪几样东西。
> **前置**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) 交出启动信息与内存记账（`KernelInfo` 字段的存放与 `cut_memmap` 的用法）、[02-higher-half-kernel.md](02-higher-half-kernel.md) 建立高半地址空间（设备寄存器页可被内核直接寻址的前提）、[03-kmain-cstart.md](03-kmain-cstart.md) 装上保护结构（GDT/IDT/TSS 三张表与"门里有真入口"）、[04-platform-discovery.md](04-platform-discovery.md) 产出描述符（本篇的三次实例化都从它取地址与频率）。
> **后续**：tick 到达之后做什么（`15-clock-timer`（时钟与定时器篇，尚未重生成））、中断分发与 claim/complete 协议（`14-exception-interrupt`（异常与中断篇））、`bsp_finish_booting` 的其余步骤与 `switch_to_user`（`08-system-init-boot-finish`（启动收尾与切用户篇））、AP 定时器与 SMP（`16-smp`（SMP 篇，尚未重生成））。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；aarch64 用 GICv3 与 Generic Timer 取代 32 位 ARM 一侧的 OMAP INTC 与 BSP timer（`[ARCH: K-1]`）、riscv64 整套是 minix-rs 新增（`[ARCH: K-2]`）、时钟频率从 60/1000 Hz 统一为 100 Hz（`[ARCH: K-3]`）。
> **目标读者**：熟悉操作系统基本概念（特权级、中断与异常、陷入、地址空间、进程）与 C 语言的开发者；Rust 一侧读到 enum/match/trait 即可。
> **前置知识**：特权级与中断门/陷阱门的大意；8259A 与 PIT 作为"中断控制器与定时器芯片"的概念（不需要数据手册——本篇结论由源码常量互相做差得出）；描述符（04 篇产物）与"IDT 门里有真入口"（03 篇结论）。
> **教学目标**：①三样东西与 cstart 三行调用的次序（哪一步不可交换、为什么）；②init_clock 纯软件与 hz 的回退；③intr_init 的十次写与"全屏蔽"的结束状态；④arch_init 六件事与 system_hz 依赖；⑤定时器为什么不在 cstart 里、三段序列与它的位置；⑥Rust 承接的八条决策与三架构差异；⑦测试与诊断（§5、§6）。

上一篇结束时，CPU 已经能接住异常：IDT 里每一条门都有真入口，从 ring3 进内核时 CPU 知道该换哪根栈。能接住不等于会接到——那一刻全部中断线还是屏蔽的，硬件定时器还没编程，架构侧的杂项（ACPI、PMU、TSS 的剩余部分）也没做。这一篇处理的就是这几件事，以及它们之间那条不能改动的次序。

---

## 1. 概述

### 1.1 03 篇之后缺的三样东西

03 篇的收尾把状态写得很窄：三张表都已装载生效、门里有真入口（[03-kmain-cstart.md](03-kmain-cstart.md) §3.7），而中断线全部处于屏蔽——"有门可走，但没有源被放行"（[03-kmain-cstart.md](03-kmain-cstart.md) §2.2）。这套状态能接住异常，却接不住设备。缺的不是入口，入口已经在了；缺的是入口背后的三样东西：

1. **一份软件时钟状态**。内核需要一组记录：一秒分多少个 tick、从启动到现在过了多少个 tick、实时钟偏移多少、负载历史怎么采样。C 把这些收在 `kclockinfo` 与 `kloadinfo` 两个全局结构里（声明在 [`kernel/glo.h`](minix3/minix/kernel/glo.h)，`kclockinfo` 的定义在 [`kernel/usermapped_data.c`](minix3/minix/kernel/usermapped_data.c) 的 `.usermapped` 段）。
2. **一个处于已知状态的中断控制器**。控制器需要先建立"每条线都在屏蔽"的确定状态，之后"放开某一条线"才是一个有定义的动作（x86 由控制器初始化建立；C 的 ARM 一侧把这一步交给 BSP 与后续钩子注册，§2.5）。
3. **一组架构杂项**。x86 上包括内核栈指针、TSS 的单 CPU 收尾、串口、ACPI、APIC 的建立与内存图裁剪；ARM 一侧是 TSS 与 PMU；riscv64 上是它自己的那几件。

C 的 `cstart` 用三行调用把这三样排出来（`kernel/main.c` 的 [`cstart`](minix3/minix/kernel/main.c:cstart)）：

```text
cstart 的三行调用（`prot_init` 是 03 篇的起点，作上下文列出），以及每一行之后系统多出的能力
  prot_init ──► init_clock ──► intr_init(0) ──► arch_init
     │              │                │               │
     │              │                │               └─ 架构杂项：TSS / 串口 / ACPI / APIC / 内存裁剪
     │              │                └─ 控制器可用，但每条线都处于屏蔽
     │              └─ kclockinfo.hz 与 kloadinfo 有值（纯软件，未碰硬件）
     └─（03 篇）三张表装载：GDT、IDT、TSS
```

三行调用之间的关系并不对称。把三个函数各自读写的状态列出来，就能看出哪条次序是硬件依赖、哪条只是写法：

| 函数 | 读谁的状态 | 改谁的状态 | 与前一行的关系 |
|---|---|---|---|
| `init_clock` | boot 参数（`env_get("hz")`） | `kclockinfo`、`kloadinfo` 两份全局内存 | 不读 `prot_init` 的任何产物，也不碰端口 |
| `intr_init` | 无（参数 `auto_eoi` 来自调用点） | 8259A 的端口 0x20/0x21/0xA0/0xA1 | 不读 `kclockinfo`，与 `init_clock` 无共享状态 |
| `arch_init` | `kinfo`、`system_hz`（即 `kclockinfo.hz`） | 内核栈指针、TSS、串口、ACPI 表、LAPIC/IOAPIC、`kinfo.memmap` | 必须在前两者之后：它读 `system_hz`，且自己重写 IDT 的门 |

第一条次序关系由此可以推出来：`init_clock` 与 `intr_init` 之间没有硬件依赖，交换它们不会造成错误。前者的函数体里没有任何端口写（§2.2），后者的函数体里没有任何 `kclockinfo` 读取（§2.3）。真正不能颠倒的是 `arch_init` 与它前面的两步。

**把 `arch_init` 提到 `init_clock` 之前会怎样**：x86 的 `arch_init` 在 APIC 分支里调用 `apic_single_cpu_init`（`i386/arch_system.c` 的 [`arch_init`](minix3/minix/kernel/arch/i386/arch_system.c:arch_init)），后者会经 `lapic_enable` 走到 `init_8253A_timer(system_hz)` 与 `system_hz * lapic_delta / (PROBE_TICKS - 1)` 这样的算式（[`i386/apic.c`](minix3/minix/kernel/arch/i386/apic.c)）。而 `system_hz` 在 C 里是一个别名宏：

```c
// minix3/minix/kernel/glo.h —— 代码引用（该宏的定义行）
#define system_hz (kclockinfo.hz)		/* HZ value (alias) */
```

`kclockinfo` 声明在 [`kernel/glo.h`](minix3/minix/kernel/glo.h)、定义在 [`kernel/usermapped_data.c`](minix3/minix/kernel/usermapped_data.c) 的 `.usermapped` 段（与 `kinfo`/`machine` 同段，用户态可见；结构体本体在 [`minix/type.h`](minix3/minix/include/minix/type.h)）。`init_clock` 之前它是全零，而它的第一个动作就是 `memset(&kclockinfo, 0, sizeof(kclockinfo))`（`kernel/clock.c` 的 [`init_clock`](minix3/minix/kernel/clock.c:init_clock)）。于是 `system_hz == 0` 会同时污染三处：

- `load_update` 的槽位算式 `(kclockinfo.uptime / system_hz / _LOAD_UNIT_SECS) % _LOAD_HISTORY` 直接除以 0（`kernel/clock.c` 的 [`load_update`](minix3/minix/kernel/clock.c:load_update)）；
- `init_local_timer` 的 legacy 分支里 `tsc_per_tick[0] = cpu_get_freq(0) / system_hz` 除以 0（`i386/arch_clock.c` 的 [`init_local_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_local_timer)）；
- 校准链整体失准：`apic_calibrate_clocks` 的两式 `system_hz * lapic_delta / (PROBE_TICKS - 1)` 与 `(tsc_delta / (PROBE_TICKS - 1)) * make64(system_hz, 0)` 分子为 0（此时 `PROBE_TICKS`（定义式 `system_hz / 10`，[`i386/apic.c`](minix3/minix/kernel/arch/i386/apic.c) 与 `arch_clock.c` 各一份）也是 0，除数退化成 `-1`），频率读数恒为 0；`estimate_cpu_freq` 里 `while(probe_ticks < PROBE_TICKS)` 的继续条件在 `PROBE_TICKS == 0` 时一开始就为假，采样循环几乎不执行，`tsc1` 停在初值。

依据都在常量与式子上：`kernel/clock.c` 的 [`init_clock`](minix3/minix/kernel/clock.c:init_clock) 是唯一给 `kclockinfo.hz` 赋值的地方（结构体其余字段由 tick 与 `set_realtime`/`set_boottime` 各自推进），[`i386/apic.c`](minix3/minix/kernel/arch/i386/apic.c) 里 `system_hz` 出现在分频与样本数的算式里。

第三条次序关系与这三行调用不在同一处：硬件定时器的编程不在 `cstart` 里。C 把它放在 `boot_cpu_init_timer`（`kernel/clock.c` 的 [`boot_cpu_init_timer`](minix3/minix/kernel/clock.c:boot_cpu_init_timer)），由 `bsp_finish_booting` 在 `kmain` 的最后调用（`kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting)）。原因在 §2.6 展开：定时器的编程里含一次"等时钟中断到达"的校准，且 tick 的消费者（记账与时钟任务通知）要到启动末端才就位。

### 1.2 中断控制器在这条链上做什么

CPU 在"意外"面前只有一个异步入口：一条外部中断引脚，或等价的一个陷阱来源。设备却有几十条信号线。中断控制器就是夹在中间的那台转接设备：它把多条线合成一个入口，记住哪条线在请求、按优先级仲裁、并按配置把请求送成某个可被 CPU 识别的编号。

x86 的经典形态把这条链拆得最清楚，因为它分成两级：

```text
x86 上一条设备线到 CPU 的路径（经典 8259A 形态）
   设备（PIT） ──► 8259A 主片 IRQ0 ─┐
                                    ├─► 主片 INT ──► IOAPIC 输入 2 ──► LAPIC ──► CPU
   设备（IRQ8-15） ──► 8259A 从片 ──┘            （级联线 IRQ2 在 IOAPIC 上走输入 0）
              INT_CTL=0x20 / INT_CTLMASK=0x21（主片）
              INT2_CTL=0xA0 / INT2_CTLMASK=0xA1（从片）
```

端口名与线号来自 [`include/interrupt.h`](minix3/minix/include/arch/i386/include/interrupt.h)：`INT_CTL` 是主片命令口 `0x20`、`INT_CTLMASK` 是主片数据口 `0x21`、`INT2_CTL` 与 `INT2_CTLMASK` 是从片的 `0xA0`/`0xA1`，`IRQ0_VECTOR` 是 `0x50`、`IRQ8_VECTOR` 是 `0x70`、`CASCADE_IRQ` 是 `2`、`CLOCK_IRQ` 是 `0`。

CPU 侧的"意外"分两类，本篇只需要其中一类能用的判据：

| 类别 | 触发者 | 与当前指令的关系 | 能否重试 | 本篇的例子 |
|---|---|---|---|---|
| 异常 | CPU 自己执行了某条指令 | 同步：由该指令直接引起 | 缺页处理后可以重试 | IDT 门与 trap 入口（[03-kmain-cstart.md](03-kmain-cstart.md)） |
| 中断 | CPU 之外的硬件 | 异步：与当前指令无关 | 不能重试，设备状态已经变了 | 8254 PIT 的 tick |

完整的异常分类（除法错、断点、越界、缺页……各自的向量与处理）属异常/中断与 trap 入口主题（`14-exception-interrupt`（异常与中断篇））；本篇用到这张表的只有一处：中断是异步的，所以"什么时候可以被打断"必须由内核显式决定——这就是初始化要先把每条线置成屏蔽、再由需要它的人逐条放开的原因。

**控制器的初始化产出什么？** `intr_init` 的函数体是一串定长的端口写（执行路径十次，源码里 12 行，口径见 §2.3），它的结束状态不是"控制器可用了"，而是"控制器可用了，并且每一条线都在屏蔽"。这一点在两个地方能读到：函数头的注释写着 `finishing with all interrupts disabled`，函数体最后两次写把主片与从片的屏蔽字都置成全屏蔽（主片留一个例外，见 §2.3）。

**控制器初始化结束时若留下未屏蔽的线会怎样**：这条线上的设备在 `sti` 之后的任意时刻都能把 CPU 拉进它对应的门。此刻门是有的（03 篇已装好），钩子链却是空的——C 的分发函数在这个状态下的动作是打印一行伪中断（`spurious irq`）提示并保持屏蔽（`kernel/interrupt.c` 的 [`irq_handle`](minix3/minix/kernel/interrupt.c:irq_handle) 的空链表分支：`printf("irq_handle: spurious irq %d …")` 后 `return`）。后果落到对象上是 `irq_handlers[irq]` 这个链表头为空、`nspurious[irq]` 被计数、以及一次落在启动期临时栈上的进出。这不是崩溃，但它把一次无主的进栈塞进了"进程表还没建完、`proc_ptr` 还指向旧值"的时间窗里。

### 1.3 三架构的时钟源、控制器与架构杂项

同一件事在三种机器上落在三套硬件上。把"时钟源"与"中断控制器"分成两列，能看出 RISC-V 的一个特别之处：

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 时钟源 | 8254 PIT（I/O 端口 0x40–0x43）或 LAPIC 定时器 | Generic Timer（`CNTFRQ_EL0` 读频率、`CNTPCT_EL0` 计数、`CNTP_CVAL_EL0` 比较、`CNTP_CTL_EL0` 控制） | CLINT 的 `mtime` 计数器与每 hart 一份的 `mtimecmp` 比较器 |
| 时钟中断怎么到 CPU | PIT 输出接 8259A IRQ0；8259A 的 INT 接 IOAPIC 输入 2，由 IOAPIC 转成向量送 LAPIC | EL1 非安全物理定时器产生 PPI，经 GIC 的 redistributor 与 CPU 接口送 CPU | S 模式定时器中断：`mtime >= mtimecmp` 置起 `sip.STIP`，由 `sie.STIE` 使能后直接进入 S 模式的陷阱入口 |
| 中断控制器 | 8259A（经典）或 LAPIC + IOAPIC（APIC 形态） | GICv3：Distributor + Redistributor + CPU 接口 | PLIC：外部设备中断的仲裁与使能 |
| 定时器寄存器 | PIT divisor（I/O 端口 0x40/0x43） | `CNTP_CVAL_EL0` / `CNTP_CTL_EL0` | `mtimecmp`（每 hart 一份） |
| 架构杂项 | 内核栈指针、TSS、串口、ACPI、APIC、内存裁剪 | 内核栈指针、TSS、PMU、板级初始化 | 无 S 模式写法（PMP 由固件承担）；定时器与 IPI 使能位归 `TimerIrqGate` 与 SMP（§3.5、§4.6） |
| Minix3 C 的覆盖 | 有（`arch/i386`） | 有，但目标是 32 位 ARM：OMAP INTC 与 BSP timer | 无对应实现 |

最后一行是关键前提。Minix3 C 只覆盖两种架构，RISC-V 这一列没有对应的 C 实现；ARM 那一列在 C 里是 32 位机器（OMAP3 的 INTC 与私有定时器），不是本篇实现所对的机器。

**把 RISC-V 的定时器当成 PLIC 上的普通设备线来管会怎样**：PLIC 的屏蔽与放行改的是它自己的 enable 位图，而这条线的中断由 `mtime >= mtimecmp` 比较与 `sie.STIE` 使能位决定——对 PLIC 位图的操作既不能阻止、也不能允许这次中断，等于一次没有效果的屏蔽。同时那条被占用的位在 PLIC 的位图里仍然记在名下，会对后来注册在这个编号上的真实设备产生误导：它看起来"已经被某个所有者使能了"。落到对象：PLIC 的 enable 位图、`sie.STIE` 位、定时器的 `mtimecmp` 比较器。定时器因此需要一个与设备线号分开的"分发身份"，这就是 §4.9 的保留伪向量要解决的问题。

### 1.4 本篇范围与可以安全跳过的内容

本篇的终点是一个可以被证实的状态：定时器已编程、它的处理函数已注册、投递门已打开。越过这个终点之后的事情不属于本篇：

| 不在这里展开的内容 | 它属于 |
|---|---|
| tick 到达之后的六步（推进 uptime/realtime、记账、虚拟与档案定时器、到期 alarm、负载更新）与 `clock_irq_handler` 的批处理细节 | `15-clock-timer`（时钟与定时器篇，尚未重生成） |
| 中断分发链（claim → mask → 链 → unmask → complete）与 claim/complete 协议的硬件理由 | `14-exception-interrupt`（异常与中断篇） |
| `bsp_finish_booting` 的其余步骤（announce、`cycles_accounting_init`、`fpu_init`、`kernel_may_alloc = 0`、`switch_to_user`） | `08-system-init-boot-finish`（启动收尾与切用户篇） |
| AP 的本地定时器、IPI 使能与 SMP 状态 | `16-smp`（SMP 篇，尚未重生成） |
| 统计 profiling 时钟的运行时行为（`SYS_SPROF` 的完整语义） | 内核 profiling 专题文档 |
| 进程表本身的初始化与 boot 进程装载 | `06-proc-init-boot-proc`（进程表与 boot 进程篇） |
| 时间片（quantum）的扣减与调度器如何消费 tick | 调度篇与 `11-scheduling-primitives`；本篇只到"tick 开始进来" |

两条可以安全跳过的说明：本篇**不需要**掌握 8259A 数据手册的完整位定义——§2.3 的三条结论由 [`i386/i8259.c`](minix3/minix/kernel/arch/i386/i8259.c) 里的常量与调用点互相对照得到；本篇也**不需要**掌握 GICv3 的完整寄存器地图——只需要四个地址偏移与两级门的归属（§4.4、§4.7.1）。

**章末复述**：用 §1.1 的那张框架再说一遍这一章——三行调用各自补齐一样东西（软件时钟状态、处于已知屏蔽态的控制器、架构杂项），三样东西里只有"架构杂项"读前两者的产物，因此唯一不能交换的次序是 `arch_init`；而真正让时间开始流动的第四步（编程定时器）不在这一章：C 把它放在启动末端的 `bsp_finish_booting`，因为那次编程里含一次"等时钟中断到达"的校准，函数体也要读那时才存在的进程上下文（实现侧把这一步放在什么位置、为什么可以在进程表建立之前执行，见 §4.7）。

---

## 2. C 源码分析

### 2.1 cstart 的三行调用与它们之间的准备

`kernel/main.c` 的 [`cstart`](minix3/minix/kernel/main.c:cstart) 是这一篇的入口。它从 `prot_init()` 开始，到 `arch_init()` 结束，中间夹着 boot 参数的解析与几处 `kinfo` 填充。逐段看一遍，才能确定三行调用之间有没有隐藏的状态依赖：

```c
// minix3/minix/kernel/main.c:cstart —— 节选（两处省略号：第一处隐去 DEBUGEXTRA(("cstart\n")); 一行，第二处为三段条件编译块；块内其余行逐字保留，原注释保留）
void cstart(void)
{
  register char *value;				/* value in key=value pair */

  /* low-level initialization */
  prot_init();

  /* determine verbosity */
  if ((value = env_get(VERBOSEBOOTVARNAME)))
	  verboseboot = atoi(value);

  /* Initialize clock variables. */
  init_clock();

  /* Get memory parameters. */
  value = env_get("ac_layout");
  if(value && atoi(value)) {
        kinfo.user_sp = (vir_bytes) USR_STACKTOP_COMPACT;
        kinfo.user_end = (vir_bytes) USR_DATATOP_COMPACT;
  }
  ...
  /* Record miscellaneous information for user-space servers. */
  kinfo.nr_procs = NR_PROCS;
  kinfo.nr_tasks = NR_TASKS;
  strlcpy(kinfo.release, OS_RELEASE, sizeof(kinfo.release));
  strlcpy(kinfo.version, OS_VERSION, sizeof(kinfo.version));

  /* Initialize various user-mapped structures. */
  memset(&arm_frclock, 0, sizeof(arm_frclock));

  memset(&kuserinfo, 0, sizeof(kuserinfo));
  kuserinfo.kui_size = sizeof(kuserinfo);
  kuserinfo.kui_user_sp = kinfo.user_sp;
  ...
  DEBUGEXTRA(("intr_init(0)\n"));

  intr_init(0);

  arch_init();
}
```

两处省略号各隐去一样东西：第一处是紧接 `ac_layout` 解析之后的 `DEBUGEXTRA(("cstart\n"));` 一行；第二处才是三段条件编译块（`USE_APIC` 的 `no_apic`/`apic_timer_x` 解析、`USE_WATCHDOG`、`CONFIG_SMP` 的 `no_smp`），它们与本节结论无关，分别属于平台发现（`04-platform-discovery.md`）与 SMP 篇。

逐段读下来的结论：三行调用之间夹着的都是**读 boot 参数、写 `kinfo` 与用户映射结构**的动作。`init_clock` 与 `intr_init` 都没有读这些动作的产物；`arch_init` 会读这些动作的产物——它读 `config_no_apic`（被省略的解析块写入的开关，§2.4 的 `if (config_no_apic)` 分支）与 `kinfo` 的内存图（`cut_memmap` 的两个入参），也读 `system_hz`（`init_clock` 的产物）。所以 `arch_init` 排在最后有两条依据：它读准备段与 `init_clock` 的产物（本节的读值依赖），加上 §2.4 那条写动作之间的约束（门表重建要求控制器处于已定义的屏蔽态）。而 `init_clock` 与 `intr_init` 之间没有任何依赖，可以交换。

`intr_init(0)` 的实参也在这里定下来：它是 `auto_eoi`。C 传 0，于是 `intr_init` 内部走 `else` 分支，写入的是 `ICW4_AT_MASTER` / `ICW4_AT_SLAVE` 这一对常量，而不是自动回执的那一对。这一个数字决定了 §2.3 里哪一组常量出现在那十次 `outb` 中。

顺带看清 `env_get` 与它背后的 `get_value`，它们是"boot 参数"这个说法的实体（按源码顺序引用）：

```c
// minix3/minix/kernel/main.c:get_value 与 env_get —— 代码引用（两个相邻函数；函数上方的星号横幅注释与函数之间的空行未收）
char *get_value(
  const char *params,			/* boot monitor parameters */
  const char *name			/* key to look up */
)
{
/* Get environment value - kernel version of getenv to avoid setting up the
 * usual environment array.
 */
  register const char *namep;
  register char *envp;

  for (envp = (char *) params; *envp != 0;) {
	for (namep = name; *namep != 0 && *namep == *envp; namep++, envp++)
		;
	if (*namep == '\0' && *envp == '=') return(envp + 1);
	while (*envp++ != 0)
		;
  }
  return(NULL);
}

char *env_get(const char *name)
{
	return get_value(kinfo.param_buf, name);
}
```

`param_buf` 是启动信息里带过来的一段 NUL 分隔的 `key=value` 串；`get_value` 逐字节比较键名，命中后再确认后面紧跟 `=\0` 形式的等号，返回等号之后的指针。这段实现解释了 §2.2 里 `env_get("hz")` 的三种返回：找到（返回指向数字串的指针）、找不到（`NULL`）、键名是某个更长键的前缀（不会命中，因为要等号）。

**把 `intr_init(0)` 改成 `intr_init(1)` 会怎样**：写进 ICW4 的就换成 `ICW4_AT_AEOI_MASTER = 0x07` 与 `ICW4_AT_AEOI_SLAVE = 0x03`，差异落在 ICW4 的 bit 1（自动回执位）。后果不在"少写一次 EOI"，而在整条回执链的语义：C 的分发路径与 `hw_intr_ack` 是按"处理完显式写 EOI"组织的（[`include/hw_intr.h`](minix3/minix/kernel/arch/i386/include/hw_intr.h) 把 `hw_intr_ack` 映射到 `ioapic_eoi` 或 `irq_8259_eoi`），自动回执会让控制器在中断被应答的同时就重新打开同一条线——在中断处理尚未完成时允许同级中断重入。依据：[`i386/i8259.c`](minix3/minix/kernel/arch/i386/i8259.c) 的八条 ICW4 常量与 `if (auto_eoi)` 分支。

### 2.2 init_clock：只清软件状态

第二步是 `init_clock`。它的函数体只有三个动作：

```c
// minix3/minix/kernel/clock.c:init_clock —— 代码引用（L47-64 的完整函数体）
void
init_clock(void)
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

逐行读：

1. `memset(&kclockinfo, 0, sizeof(kclockinfo))` —— `kclockinfo` 是 `glo.h` 里的全局结构（[`kernel/glo.h`](minix3/minix/kernel/glo.h) 声明，定义在 [`kernel/usermapped_data.c`](minix3/minix/kernel/usermapped_data.c) 的 `.usermapped` 段；结构体本体在 [`minix/type.h`](minix3/minix/include/minix/type.h)，与 §1.1 同一组锚点），清零之后 `hz`、`uptime`、`realtime`、`boottime` 都为 0。
2. `env_get("hz")` —— 从 boot 参数串里找 `hz`；找得到就 `atoi`，找不到就保持 0。
3. 范围回退 —— `value == NULL || hz < 2 || hz > 50000` 三者任一成立就用 `DEFAULT_HZ`。这里有一个顺序细节：`atoi(value)` 把非数字串解析成 0，落进 `hz < 2` 的分支，所以"参数给了但写得不是数字"也走回退。
4. `memset(&kloadinfo, 0, sizeof(kloadinfo))` —— 负载历史环形缓冲清零；`kloadinfo` 的字段（`proc_last_slot`、`proc_load_history[]`、`last_clock`）在 [`minix/type.h`](minix3/minix/include/minix/type.h) 的 `struct loadinfo` 里。

`DEFAULT_HZ` 在两种架构上是两个值：

| 架构 | `DEFAULT_HZ` | 定义处 |
|---|---|---|
| i386 | 60 | `include/archconst.h` 的 [`DEFAULT_HZ`](minix3/minix/include/arch/i386/include/archconst.h:DEFAULT_HZ)（行尾注释写着 `clock freq (software settable on IBM-PC)`） |
| earm | 1000 | `include/archconst.h` 的 [`DEFAULT_HZ`](minix3/minix/include/arch/earm/include/archconst.h:DEFAULT_HZ) |

全局别名在 `glo.h` 里：`system_hz` 就是 `kclockinfo.hz`（本篇 §1.1 已引用该宏）。这一点让 `init_clock` 的产物被整个内核的计时算式读到——它是后来 `arch_init` 必须排在它之后的第一条依据（第二条见 §2.4 的门表重建约束；两条的合并陈述见 §2.1 的结论段）。

整个函数体里没有任何端口写、没有任何寄存器访问。硬件定时器的编程在 C 里属于另外两个函数（`init_local_timer` 与 `boot_cpu_init_timer`），它们由 `bsp_finish_booting` 在启动末端调用，见 §2.6。

**把范围回退那一行删掉会怎样**：`kclockinfo` 刚被清零，未命中 `env_get` 时 `hz` 保持 0，而 `system_hz` 是它的别名，于是同一张污染清单（§1.1）在这里逐条落地：`load_update` 的槽位算式 `(kclockinfo.uptime / system_hz / _LOAD_UNIT_SECS) % _LOAD_HISTORY` 变成除以 0（`kernel/clock.c` 的 [`load_update`](minix3/minix/kernel/clock.c:load_update)）；`init_local_timer` 的 legacy 分支里 `tsc_per_tick[0] = cpu_get_freq(0) / system_hz` 除以 0（`i386/arch_clock.c` 的 [`init_local_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_local_timer)）；`apic.c` 的校准两式分子为 0、`estimate_cpu_freq` 的采样循环退化（[`i386/apic.c`](minix3/minix/kernel/arch/i386/apic.c)、`:estimate_cpu_freq`）。落到对象：`kloadinfo.proc_last_slot` 永远算不出、`tsc_per_tick[0]` 除零、频率读数为 0。

### 2.3 intr_init 的十次 outb（源码 12 行）

第三步是控制器初始化。它的全部内容是一串定长的写操作，读懂它的关键在"写了什么值、写到哪个口"：

```c
// minix3/minix/kernel/arch/i386/i8259.c —— 代码引用（L13-23 的常量表与 L28-52 的函数体）
#define ICW1_AT         0x11	/* edge triggered, cascade, need ICW4 */
#define ICW1_PC         0x13	/* edge triggered, no cascade, need ICW4 */
#define ICW1_PS         0x19	/* level triggered, cascade, need ICW4 */
#define ICW4_AT_SLAVE   0x01	/* not SFNM, not buffered, normal EOI, 8086 */
#define ICW4_AT_MASTER  0x05	/* not SFNM, not buffered, normal EOI, 8086 */
#define ICW4_PC_SLAVE   0x09	/* not SFNM, buffered, normal EOI, 8086 */
#define ICW4_PC_MASTER  0x0D	/* not SFNM, buffered, normal EOI, 8086 */
#define ICW4_AT_AEOI_SLAVE   0x03 /* not SFNM, not buffered, auto EOI, 8086 */
#define ICW4_AT_AEOI_MASTER  0x07 /* not SFNM, not buffered, auto EOI, 8086 */
#define ICW4_PC_AEOI_SLAVE   0x0B /* not SFNM, buffered, auto EOI, 8086 */
#define ICW4_PC_AEOI_MASTER  0x0F /* not SFNM, buffered, auto EOI, 8086 */

int intr_init(const int auto_eoi)
{
/* Initialize the 8259s, finishing with all interrupts disabled.  */
      outb( INT_CTL, ICW1_AT);
      outb( INT_CTLMASK, IRQ0_VECTOR);
					/* ICW2 for master */
      outb( INT_CTLMASK, (1 << CASCADE_IRQ));
					/* ICW3 tells slaves */
      if (auto_eoi)
          outb( INT_CTLMASK, ICW4_AT_AEOI_MASTER);
      else
          outb( INT_CTLMASK, ICW4_AT_MASTER);
      outb( INT_CTLMASK, ~(1 << CASCADE_IRQ)); /* IRQ 0-7 mask */
      outb( INT2_CTL, ICW1_AT);
      outb( INT2_CTLMASK, IRQ8_VECTOR);
						/* ICW2 for slave */
      outb( INT2_CTLMASK, CASCADE_IRQ);	/* ICW3 is slave nr */
      if (auto_eoi)
         outb( INT2_CTLMASK, ICW4_AT_AEOI_SLAVE);
      else
         outb( INT2_CTLMASK, ICW4_AT_SLAVE);
      outb( INT2_CTLMASK, ~0);		/* IRQ 8-15 mask */

  return OK;
}
```

`cstart` 传的 `auto_eoi` 是 0，所以实际执行的是每个 `if/else` 的 `else` 支路。**口径先说清**：函数里实际执行十次 `outb`（主片五次、从片五次），而源码里有 12 行 `outb(`——两处 `if (auto_eoi)` 各带一行不执行的替代臂。把符号全部换成实数，实际执行的十次写是：

| 次序 | 端口（实数） | 写入值 | 符号 | 它在写什么 |
|---|---|---|---|---|
| 1 | `INT_CTL` = 0x20 | 0x11 | `ICW1_AT` | 主片初始化开始：边沿触发、级联、需要 ICW4 |
| 2 | `INT_CTLMASK` = 0x21 | 0x50 | `IRQ0_VECTOR` | 主片的中断向量基址 |
| 3 | 0x21 | 0x04 | `1 << CASCADE_IRQ` = `1 << 2` | 主片的 ICW3：从片挂在 IRQ2 上（位图） |
| 4 | 0x21 | 0x05 | `ICW4_AT_MASTER` | 主片的 ICW4（不会回执，见下） |
| 5 | 0x21 | 0xFB | `~(1 << CASCADE_IRQ)` | 主片屏蔽字：除 IRQ2 外全部屏蔽 |
| 6 | `INT2_CTL` = 0xA0 | 0x11 | `ICW1_AT` | 从片初始化开始，同样的三位配置 |
| 7 | `INT2_CTLMASK` = 0xA1 | 0x70 | `IRQ8_VECTOR` | 从片的中断向量基址 |
| 8 | 0xA1 | 0x02 | `CASCADE_IRQ` = 2 | 从片的 ICW3：级联身份（编号） |
| 9 | 0xA1 | 0x01 | `ICW4_AT_SLAVE` | 从片的 ICW4 |
| 10 | 0xA1 | 0xFF | `~0` | 从片屏蔽字：全部屏蔽 |

端口名到实数的对应在 [`include/interrupt.h`](minix3/minix/include/arch/i386/include/interrupt.h)：`INT_CTL` 0x20、`INT_CTLMASK` 0x21、`INT2_CTL` 0xA0、`INT2_CTLMASK` 0xA1。整段序列只用这四个端口。

**先看这十次写为什么不能换次序**：第 2、3、4 次写的是**同一个数据口** `0x21`，但芯片把它们分别解释成 ICW2、ICW3、ICW4；第 5 次写 `0x21` 时，同一个端口才被当成屏蔽寄存器（源码在那行尾注写着 `IRQ 0-7 mask`）。也就是说这个端口不看数据是什么，只看"这是收到 ICW1 之后的第几个字"——一串四步的初始化命令字靠**计数**被区分；第 5 次起才切到另一套解释（操作命令字）。从片那一半（第 6–10 次）在同一对端口上重演一遍。这条状态机的两个可观察后果：① 初始化序列中间**不能插入别的写操作**，插一个就把后面的字整体错位解释；② 屏蔽字必须排在 ICW4 之后——排在前面时它会被当成某个初始化字，屏蔽位根本没写到屏蔽寄存器上。

三条结论是"看了码才能发现"的那种，而且都不依赖手册：

**第一条，ICW1 那三个常量互相做差就给出了位含义。** `0x11`、`0x13`、`0x19` 三条注释都是英文短语，值上的差是 `0x13 - 0x11 = 0x02` 与 `0x19 - 0x11 = 0x08`。对照注释可以读出：bit 1 对应 `cascade` 的有无（`0x13` 的注释是 `no cascade`），bit 3 对应触发方式（`0x19` 的注释是 `level triggered`），而三条共有的 bit 0 对应 `need ICW4`、共有的 bit 4 是"这是一次 ICW1"的标识位。三个常量就是三位含义的对照表。

**第二条，主片与从片的 ICW3 用了同一条宏、两种语法。** 主片写的是 `1 << CASCADE_IRQ` = `0x04`：一个位图，第 2 位置位表示"IRQ2 线上挂着从片"。从片写的是 `CASCADE_IRQ` = `0x02`：一个编号，值 2 表示"我是从片，接在主片的 IRQ2 上"。同一个宏 `CASCADE_IRQ` 在两片上是两种编码——位图与编号。这是这段代码里最容易看漏的不对称。

**第三条，ICW4 的主从常量只差一位，而两条注释完全相同。** `ICW4_AT_MASTER = 0x05` 与 `ICW4_AT_SLAVE = 0x01` 的差是 `0x04`，也就是 bit 2；而 [`i386/i8259.c`](minix3/minix/kernel/arch/i386/i8259.c) 里这两行的尾注都是 `not SFNM, not buffered, normal EOI, 8086`。注释没有为这一位的差异提供任何说明，本篇也不据外部资料裁定哪一侧的注释过时——只登记"两个值差 bit 2、注释相同"这个事实。读者若要判定，需要读 8259A 的 ICW4 位定义，那是本仓之外的材料。

两条与屏蔽方向有关的事实。其一是屏蔽口的方向：`interrupt.h` 在 `INT_CTLMASK` 与 `INT2_CTLMASK` 两行都写着同一句注释：

```c
// minix3/minix/include/arch/i386/include/interrupt.h —— 代码引用（L9-12 的四行）
#define INT_CTL         0x20	/* I/O port for interrupt controller */
#define INT_CTLMASK     0x21	/* setting bits in this port disables ints */
#define INT2_CTL        0xA0	/* I/O port for second interrupt controller */
#define INT2_CTLMASK    0xA1	/* setting bits in this port disables ints */
```

置位是禁用。所以第 5 次写的 `~(1 << CASCADE_IRQ)` = `0xFB` 里那唯一的 0 位（bit 2）是唯一放行的线——级联线必须开着，从片的中断才有通路；其余 7 条主片线被屏蔽。最后一次写（`~0` = `0xFF`）把从片的 8 条线全部屏蔽。函数头注释说的 `finishing with all interrupts disabled` 就是这个状态：主片放行一条级联、从片一条不放、主片自己的设备线全关。

**三条推演（主片 ICW3 写成编号；第 5 次写改成全屏蔽；ICW2 写 0）**：

1. 主片的 ICW3 若按"编号"写 `0x02`：位图里变成 bit 1 置位、bit 2 清零，即主片认为"从片挂在我的 IRQ1 上"；而第 5 次写的屏蔽字仍是 `0xFB`（bit 2 打开、bit 1 屏蔽）。从片的中断从 IRQ2 进来，主片却把它理解成"从片在 IRQ1 上"——IRQ1 那条线还是屏蔽的。落到对象：主片 ICW3 的 bit 1 与 bit 2、IRQ2 线的屏蔽状态。
2. 第 5 次写若改成 `~0`（主片也全屏蔽，看起来更"安全"）：从片的 INT 输出走主片的 IRQ2，这条线一封，从片上的 8 条线全部失去通路；此后任何人去 `unmask` 从片自己的线都不起作用。落到对象：主片屏蔽字的 bit 2、从片的全部 8 条线。
3. ICW2 若写 0（把向量基址"从零开始编"）：在 8259A 供向量的形态（legacy PIC 直连，或以 ExtINT 交付）下，IRQ0 的中断会落到 IDT 的向量 0，那是处理器定义的除法错门，而不是 03 篇在 0x50 处装好的那个门（APIC 形态下向量由 IOAPIC 重定向项提供，见 §4.4；这条推演按 8259A 供向量的形态陈述）。落到对象：`IRQ0_VECTOR` 的值、IDT 向量 0 与 0x50 两个门。依据：`include/interrupt.h` 的 [`IRQ0_VECTOR`](minix3/minix/include/arch/i386/include/interrupt.h:IRQ0_VECTOR) 与 `i386/i8259.c` 的 [`intr_init`](minix3/minix/kernel/arch/i386/i8259.c:intr_init) 的第 2 步写。

### 2.4 arch_init：x86 的六件事与它对 system_hz 的依赖

第三步是一个杂项集合：

```c
// minix3/minix/kernel/arch/i386/arch_system.c:arch_init —— 代码引用（L246-279 的完整函数体）
void arch_init(void)
{
	k_stacks = (void*) &k_stacks_start;
	assert(!((vir_bytes) k_stacks % K_STACK_SIZE));

#ifndef CONFIG_SMP
	/*
	 * use stack 0 and cpu id 0 on a single processor machine, SMP
	 * configuration does this in smp_init() for all cpus at once
	 */
	tss_init(0, get_k_stack_top(0));
#endif

#if !CONFIG_OXPCIE
	ser_init();
#endif

#ifdef USE_ACPI
	acpi_init();
#endif

#if defined(USE_APIC) && !defined(CONFIG_SMP)
	if (config_no_apic) {
		DEBUGBASIC(("APIC disabled, using legacy PIC\n"));
	}
	else if (!apic_single_cpu_init()) {
		DEBUGBASIC(("APIC not present, using legacy PIC\n"));
	}
#endif

	/* Reserve some BIOS ranges */
	cut_memmap(&kinfo, BIOS_MEM_BEGIN, BIOS_MEM_END);
	cut_memmap(&kinfo, BASE_MEM_TOP, UPPER_MEM_END);
}
```

六件事，逐条读：

1. **内核栈指针**：`k_stacks = (void*) &k_stacks_start` 把链接脚本给出的 per-CPU 栈区首地址存进全局，紧跟一条对齐断言——栈区必须按 `K_STACK_SIZE` 对齐，否则 `get_k_stack_top(cpu)` 算出的栈顶会落到别的 CPU 的栈区里。
2. **TSS 收尾（单 CPU 配置）**：`tss_init(0, get_k_stack_top(0))` 把 0 号 CPU 的中断栈指针写进 TSS 的 `sp0`。SMP 配置下这一步在 `smp_init` 里对所有 CPU 做，所以它被 `#ifndef CONFIG_SMP` 包住。03 篇已讲过 TSS 在保护结构里的位置（[03-kmain-cstart.md](03-kmain-cstart.md) §2.4），这里是它的单 CPU 收尾。
3. **串口**：`ser_init()` 按 `kinfo.serial_debug_baud` 配置 COM1（若该值非正则保留固件设置，直接返回）。
4. **ACPI**：`acpi_init()` 解析固件表并留下后续步骤要用的状态（`machine.acpi_rsdp` 与 MADT 的遍历状态）；`detect_ioapics()` 正是以它为条件（`if (machine.acpi_rsdp) acpi_get_ioapics(...)`）来填 IOAPIC 条目与数量。平台发现的那一半在 [04-platform-discovery.md](04-platform-discovery.md) §2.7；`apic.c` 内部还会再调一次 `acpi_init`，这是它每次启动跑两次的原因。
5. **APIC**：`apic_single_cpu_init()` 建立 LAPIC 与 IOAPIC：置 `lapic_addr = LOCAL_APIC_DEF_ADDR`、经 `lapic_enable(0)` 打开 LAPIC、跑一次时钟校准、`acpi_init()`、`detect_ioapics()`、`ioapic_enable_all()`，最后 `apic_idt_init(0)` 加 `idt_reload()`（`i386/apic.c` 的 [`apic_single_cpu_init`](minix3/minix/kernel/arch/i386/apic.c:apic_single_cpu_init)、`:detect_ioapics`）。倒数第二步与最后一步是本节的重点：这个函数**会重写 IDT 的门并重新装载 IDT**。
6. **内存裁剪**：两次 `cut_memmap` 把 BIOS 区间（`BIOS_MEM_BEGIN`..`BIOS_MEM_END`）与高内存区间（`BASE_MEM_TOP`..`UPPER_MEM_END`）从 `kinfo.memmap` 里切掉。

第 5 条展开成调用链，就能看到 `arch_init` 为什么必须排在 `init_clock` 后面。链是：

```text
arch_init（arch_system.c:246）
  └─ apic_single_cpu_init（apic.c:1125）
       └─ lapic_enable(0)（apic.c:674）
            └─ apic_calibrate_clocks(cpu)（apic.c:443，调用点在 apic.c:739）
                 ├─ #define PROBE_TICKS (system_hz / 10)     —— apic.c:169
                 ├─ init_8253A_timer(system_hz)             —— apic.c:491
                 ├─ lapic_bus_freq[cpuid] = system_hz * lapic_delta / (PROBE_TICKS - 1)  —— apic.c:517
                 └─ cpu_freq = (tsc_delta / (PROBE_TICKS - 1)) * make64(system_hz, 0)   —— apic.c:520
```

`system_hz` 是 `kclockinfo.hz` 的别名（§1.1 引用的 `glo.h` 宏），而这个字段只在 `init_clock` 里被赋过值。这就是"`arch_init` 不能前置"的第一条依据：它读的值来自第一步。

第二条依据是它自己的写动作。`apic_single_cpu_init` 末尾的 `apic_idt_init(0)` 与 `idt_reload()` 会按 APIC 形态重建门表；这个过程要求控制器处于一个已经定义好的状态，否则重建期间到达的请求会落在一个门刚被改写、钩子还没挂的窗口里。`intr_init` 结束时"每条线都屏蔽"正好提供了这个定义好的状态（§2.3 的第 5、10 步写）。

**两条推演（删掉 `cut_memmap`；删掉 `apic_single_cpu_init` 的调用）**：

1. 删掉两次 `cut_memmap`。`kinfo.memmap` 会把 BIOS 区间与高内存区当成可用内存交给内核的物理内存分配器；第一个按这张表分配页的调用者会拿到被固件或硬件占用的物理页。落到对象：`kinfo.memmap` 的区间表、两条 `cut_memmap` 的起止地址（`BIOS_MEM_BEGIN`/`BIOS_MEM_END`、`BASE_MEM_TOP`/`UPPER_MEM_END`）。这条裁剪换到实现侧之后由谁在哪一步负责，见 §3.6。
2. 删掉 `apic_single_cpu_init` 的调用。`lapic_addr` 保持为 0，`init_local_timer` 走 legacy 分支（`DEBUGBASIC(("Initiating legacy i8253 timer\n"))` 所在的那一支），`tsc_per_ms[0]` 由 `cpu_get_freq(0)` 得到而不是 APIC 校准值；计时仍能走，但 `stop_local_timer`/`restart_local_timer` 里的 `lapic_*` 分支永远不会被执行。落到对象：`lapic_addr`、`tsc_per_ms[0]`、`stop_local_timer` 的两个分支。

### 2.5 ARM 一侧的同一格：`arch_init` 与 `intr_init` 的另一种答法

Minix3 的另一个架构是 32 位 ARM。同一格里它做的是不同的几件事：

```c
// minix3/minix/kernel/arch/earm/arch_system.c:arch_init —— 代码引用（L101-132 的完整函数体）
void arch_init(void)
{
        u32_t value;

	k_stacks = (void*) &k_stacks_start;
	assert(!((vir_bytes) k_stacks % K_STACK_SIZE));

#ifndef CONFIG_SMP
	/*
	 * use stack 0 and cpu id 0 on a single processor machine, SMP
	 * configuration does this in smp_init() for all cpus at once
	 */
	tss_init(0, get_k_stack_top(0));
#endif


        /* enable user space access to cycle counter */
        /* set cycle counter to 0: ARM ARM B4.1.113 and B4.1.117 */
        asm volatile ("MRC p15, 0, %0, c9, c12, 0\t\n": "=r" (value));
        value |= PMU_PMCR_C; /* Reset counter */
        value |= PMU_PMCR_E; /* Enable counter hardware */
        asm volatile ("MCR p15, 0, %0, c9, c12, 0\t\n": : "r" (value));

        /* enable CCNT counting: ARM ARM B4.1.116 */
        value = PMU_PMCNTENSET_C; /* Enable PMCCNTR cycle counter */
        asm volatile ("MCR p15, 0, %0, c9, c12, 1\t\n": : "r" (value));

        /* enable cycle counter in user mode: ARM ARM B4.1.124 */
        value = PMU_PMUSERENR_EN;
        asm volatile ("MCR p15, 0, %0, c9, c14, 0\t\n": : "r" (value));
	bsp_init();
}
```

与 x86 的共同点：栈区赋值与对齐断言、单 CPU 配置下的 `tss_init`。不同点是三行 PMU 寄存器的读写（`PMCR` 的 C+E 两位、`PMCNTENSET` 的 C 位、`PMUSERENR` 的 EN 位），以及一行 `bsp_init()`——板级初始化。这里没有串口、没有 ACPI、没有 APIC、没有内存裁剪。

ARM 一侧的控制器初始化只做一件更基础的事：

```c
// minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:intr_init —— 代码引用（函数体；上方 `omap_intr` 结构体定义与 `static kern_phys_map intr_phys_map;` 未收）
int
intr_init(const int auto_eoi)
{
	if (BOARD_IS_BBXM(machine.board_id)) {
		omap_intr.base = OMAP3_DM37XX_INTR_BASE;
	} else if (BOARD_IS_BB(machine.board_id)) {
		omap_intr.base = OMAP3_AM335X_INTR_BASE;
	} else {
		panic
		    ("Can not do the interrupt setup. machine (0x%08x) is unknown\n",
		    machine.board_id);
	};
	omap_intr.size = 0x1000;	/* 4K */

	kern_phys_map_ptr(omap_intr.base, omap_intr.size,
	    VMMF_UNCACHED | VMMF_WRITE,
	    &intr_phys_map, (vir_bytes) & omap_intr.base);
	return 0;
}
```

它按板号选一个 MMIO 基址、把 4 KB 的寄存器页以"非缓存 + 可写"的映射建进内核地址空间，然后返回。屏蔽与放行的语义不在这里。

放行发生在 BSP 的定时器注册里：

```c
// minix3/minix/kernel/arch/earm/bsp/ti/omap_timer.c:bsp_register_timer_handler —— 代码引用（省略上方静态变量声明）
int
bsp_register_timer_handler(const irq_handler_t handler)
{
	/* Initialize the CLOCK's interrupt hook. */
	omap3_timer_hook.proc_nr_e = NONE;
	omap3_timer_hook.irq = timer->irq_nr;

	put_irq_handler(&omap3_timer_hook, timer->irq_nr, handler);
	/* only unmask interrupts after registering */
	bsp_irq_unmask(timer->irq_nr);

	return 0;
}
```

两行相邻的语句加上一行注释，把本篇的一条核心次序写成了 ARM 一侧的实现：先 `put_irq_handler` 挂链，再 `bsp_irq_unmask` 放行。C 的 ARM 一侧里 `register_local_timer_handler` 就是转发到这里（`earm/arch_clock.c` 的 [`register_local_timer_handler`](minix3/minix/kernel/arch/earm/arch_clock.c:register_local_timer_handler)）。

三处对照：

| | x86-32 | ARM-32 |
|---|---|---|
| `intr_init` 做什么 | 十次端口写：配置向量基、级联、ICW4，并把线置成全屏蔽 | 板号分支选基址，把 4 KB 寄存器页映射进内核 |
| 全屏蔽在哪里 | `intr_init` 结尾（主片留级联线） | 由 BSP 的屏蔽状态与后续 hook 注册承担（`bsp_irq_unmask` 是显式放行） |
| 架构杂项 | 栈指针、TSS、串口、ACPI、APIC、内存裁剪 | 栈指针、TSS、PMU、`bsp_init` |

**把 `bsp_register_timer_handler` 里那两行对调会怎样**：定时器线在钩子链还空着的时候就被打开，任何一次到达都会经 BSP 的中断入口进入 C 的分发函数，而 `irq_handlers[irq]` 恰好为空——C 在空链表分支打印一行伪中断提示后返回（`kernel/interrupt.c` 的 [`irq_handle`](minix3/minix/kernel/interrupt.c:irq_handle)）。落到对象：`omap3_timer_hook` 的挂链时刻、`bsp_irq_unmask` 的调用时刻、`irq_handlers[irq]` 的链表头。这是一条纯次序错误：两种写法在"最终状态"上完全一样，差别只在中途是否存在一个"线开、链空"的窗口——而落在窗口里的 tick 会走空链分支并把线保持在屏蔽态（§2.6 同一机制），此后无人再打开它。

### 2.6 硬件定时器不在 cstart 里：`boot_cpu_init_timer` 的三段与它的位置

`cstart` 的三行调用完成后，内核有了软件时钟状态、有了处于全屏蔽态的控制器、有了架构杂项——但还没有时间在流动：没有任何硬件在按固定间隔产生中断。C 把这一步放在启动的末端，函数体 11 行、只有两个动作：

```c
// minix3/minix/kernel/clock.c:boot_cpu_init_timer —— 代码引用（L294-304 的完整函数体）
int boot_cpu_init_timer(unsigned freq)
{
	if (init_local_timer(freq))
		return -1;

	if (register_local_timer_handler(
				(irq_handler_t) timer_int_handler))
		return -1;

	return 0;
}
```

三段结构：`init_local_timer(freq)` 编程本核的定时器；`register_local_timer_handler(timer_int_handler)` 注册处理函数；返回前不做别的。两个被调者都是架构相关的。x86 的版本按 `lapic_addr` 分两支：

```c
// minix3/minix/kernel/arch/i386/arch_clock.c:init_local_timer —— 代码引用（L131-153 的完整函数体）
int init_local_timer(unsigned freq)
{
#ifdef USE_APIC
	/* if we know the address, lapic is enabled and we should use it */
	if (lapic_addr) {
		unsigned cpu = cpuid;
		tsc_per_ms[cpu] = (unsigned)(cpu_get_freq(cpu) / 1000);
		tsc_per_tick[cpu] = (unsigned)(cpu_get_freq(cpu) / system_hz);
		lapic_set_timer_one_shot(1000000 / system_hz);
	} else {
		DEBUGBASIC(("Initiating legacy i8253 timer\n"));
#else
	{
#endif
		init_8253A_timer(freq);
		estimate_cpu_freq();
		/* always only 1 cpu in the system */
		tsc_per_ms[0] = (unsigned long)(cpu_get_freq(0) / 1000);
		tsc_per_tick[0] = (unsigned)(cpu_get_freq(0) / system_hz);
	}

	return 0;
}
```

legacy 分支里 `init_8253A_timer(freq)` 写的是 8254 PIT 的三个 I/O 字节（`SQUARE_WAVE` 命令字、计数低字节、计数高字节，`i386/arch_clock.c` 的 [`init_8253A_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_8253A_timer)），紧接着 `estimate_cpu_freq()`：

```c
// minix3/minix/kernel/arch/i386/arch_clock.c:estimate_cpu_freq —— 节选（省略号处依次为局部变量声明、`BKL_UNLOCK()`、`BKL_LOCK()` 与 `rm_irq_handler`、结尾的 `cpu_info` 赋值与 `BOOT_VERBOSE` 打印）
static void estimate_cpu_freq(void)
{
	...
	/* set the probe, we use the legacy timer, IRQ 0 */
	put_irq_handler(&calib_cpu, CLOCK_IRQ, calib_cpu_handler);

	...
	/* set the PIC timer to get some time */
	intr_enable();

	/* loop for some time to get a sample */
	while(probe_ticks < PROBE_TICKS) {
		intr_enable();
	}

	intr_disable();
	...
	tsc_delta = tsc1 - tsc0;

	cpu_freq = (tsc_delta / (PROBE_TICKS - 1)) * system_hz;
	cpu_set_freq(cpuid, cpu_freq);
	...
}
```

这段代码里有一个旁证支持"定时器编程不能提前"：校准的办法是挂一个临时钩子在 `CLOCK_IRQ` 上、打开中断、等 `PROBE_TICKS` 个 tick 到达，再用两次读 TSC 的差算出 CPU 频率。校准窗口本身只需要"中断入口与控制器都已经就位"（探针钩子只数 tick，不读进程上下文）；把整段推到启动末端的另有两条依赖：其一，C 的注释点明定时器初始化会间接用到 CPU 记账值，所以 `cycles_accounting_init()` 必须先跑（本节引用的 `bsp_finish_booting` 段）；其二，真实处理函数 `timer_int_handler` 注册之后，tick 的记账要读当前进程（`proc_ptr`）、通知要送到时钟任务，两者都要求进程表与任务端点已经就位。同一退化在合法输入下也存在：`hz ∈ [2,9]` 时 `PROBE_TICKS = system_hz / 10 = 0`——`init_clock` 的范围回退只挡 `< 2`，这组取值会让采样循环一开就退出、`tsc1` 停在初值。

注册那一段在 x86 上分两支，但两支都以"线被放行"收尾：

```c
// minix3/minix/kernel/arch/i386/arch_clock.c:register_local_timer_handler —— 代码引用（L177-194 的完整函数体）
int register_local_timer_handler(const irq_handler_t handler)
{
#ifdef USE_APIC
	if (lapic_addr) {
		/* Using APIC, it is configured in apic_idt_init() */
		BOOT_VERBOSE(printf("Using LAPIC timer as tick source\n"));
	} else
#endif
	{
		/* Using PIC, Initialize the CLOCK's interrupt hook. */
		pic_timer_hook.proc_nr_e = NONE;
		pic_timer_hook.irq = CLOCK_IRQ;

		put_irq_handler(&pic_timer_hook, CLOCK_IRQ, handler);
	}

	return 0;
}
```

PIC 支路的放行藏在 `put_irq_handler` 内部。C 的这个函数在挂链成功之后有一段"顺手打开硬件"的动作：

```c
// minix3/minix/kernel/interrupt.c:put_irq_handler —— 节选（省略号处为该函数前半段的合法性检查、链表遍历与 id 分配）
  ...
  hook->next = NULL;
  hook->handler = handler;
  hook->irq = irq;
  hook->id = id;
  *line = hook;

  /* And as last enable the irq at the hardware.
   *
   * Internal this activates the line or source of the given interrupt.
   */
  if((irq_actids[hook->irq] &= ~hook->id) == 0) {
	  hw_intr_used(irq);
	  hw_intr_unmask(hook->irq);
  }
}
```

这段代码是"注册即放行"的 C 形态：`irq_actids[]` 里该向量的所有 active 位清零时调 `hw_intr_unmask(irq)`。APIC 支路里 LAPIC 定时器的矢量由 `apic_idt_init` 配好，本函数只打印一行说明——两条支路都以"这条线从此可以到达"结束。

三段的调用点在哪里，`bsp_finish_booting` 的几行给出了全部上下文：

```c
// minix3/minix/kernel/main.c:bsp_finish_booting —— 节选（省略号处依次为函数前半段（cpu_identify、announce、RTS 循环等）与 `fpu_init` 后的三段调试检查与 CPU 标志段）
  ...
  /*
   * Enable timer interrupts and clock task on the boot CPU.  First reset the
   * CPU accounting values, as the timer initialization (indirectly) uses them.
   */
  cycles_accounting_init();

  if (boot_cpu_init_timer(system_hz)) {
	  panic("FATAL : failed to initialize timer interrupts, "
			  "cannot continue without any clock source!");
  }

  fpu_init();
  ...
  /* Kernel may no longer use bits of memory as VM will be running soon */
  kernel_may_alloc = 0;

  switch_to_user();
  NOT_REACHABLE;
```

三件事在同一段里：`boot_cpu_init_timer` 之前先 `cycles_accounting_init()`（注释说明定时器初始化会间接用到它），之后是 `fpu_init()` 与 `kernel_may_alloc = 0`。这说明定时器编程在 C 的启动谱上属于"最后一段"，与 `cstart` 的三行调用之间隔着整个进程表初始化与内存初始化。

**把三段的次序改成"先放行、后注册"会怎样**（在 C 里表现为把 `hw_intr_unmask(CLOCK_IRQ)` 移到 `put_irq_handler` 调用之前）：钩子链还空着时线就被打开，落在窗口里的 tick 走 `irq_handle` 的空链分支——该分支打印伪中断提示后**保持屏蔽**（入口先 `hw_intr_mask`，空链分支直接 return），线被关回且没有代码再打开它。窗口无论落在哪一次注册（校准的探针、或真实处理函数），后果形状相同：线停在被关状态，`probe_ticks` 不再增长（校准循环等不到）、或 tick 列车直接停摆。落到对象：`irq_handlers[CLOCK_IRQ]` 的链表头、`hw_intr_unmask(CLOCK_IRQ)` 的调用时刻、`probe_ticks`。

### 2.7 本章归纳

三个函数各自补齐一样东西，两条次序由依赖而不是风格决定：

| 函数 | 补齐的东西 | 它的产物被谁读 |
|---|---|---|
| `init_clock` | 软件时钟状态（`kclockinfo`、`kloadinfo`） | `system_hz` 的全部读者（`load_update`、`apic_calibrate_clocks`、`init_local_timer` 的算式）；tick 处理函数 |
| `intr_init` | 控制器的确定状态（可用 + 全屏蔽） | `arch_init` 的门表重建过程；后来的 `put_irq_handler` 放行者 |
| `arch_init` | 架构杂项（栈指针、TSS、串口、ACPI、APIC、内存裁剪） | 后续所有需要 LAPIC 地址、内存图、串口输出的代码 |

两条次序原则：

1. **软件状态先于读它的人**——`arch_init` 读 `system_hz`，所以它不能早于 `init_clock`；反过来，`init_clock` 不读任何人的产物，`intr_init` 也不读它，这两个之间可以交换。
2. **编程先于放行**——硬件定时器的三段（编程、注册、放行）在同一条链上，任一段提前都会留下一个"源与处理者不匹配"的窗口；C 把这三次动作收进 `boot_cpu_init_timer` 一处，正是为了让这个次序只有一个维护点。

---

## 3. Rust 设计决策

### 3.1 决策：ClockArch —— 把软件状态与硬件定时器分开（→§2.2、§2.6）

C 里"时钟"这件事分成两个文件、两个时间点：`kernel/clock.c` 的 [`init_clock`](minix3/minix/kernel/clock.c:init_clock) 在 `cstart` 里建软件状态，`i386/arch_clock.c` 的 [`init_local_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_local_timer) 在启动末端编程硬件，两者由 `boot_cpu_init_timer` 缝合；每多一个架构就要多一份硬件实现，接口靠 [`kernel/clock.h`](minix3/minix/kernel/clock.h) 的四条函数声明（`init_local_timer`、`stop_local_timer`、`restart_local_timer`、`register_local_timer_handler`）约束。

Rust 把它分成两层：`ClockState` 放软件状态（kernel 层，[`clock.rs`](os/kernel/src/clock.rs)），`ClockArch` 放硬件定时器（arch 层，[`clock.rs`](os/arch/src/arch/clock.rs)）。

```rust
// os/arch/src/arch/clock.rs:trait ClockArch —— 签名示意（只列方法签名，文档注释与默认实现体省略）
pub trait ClockArch: Sized + Send + Sync {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;
    fn init_timer(&mut self, hz: u32, cpu_id: u32);
    fn init_local_timer(&mut self, period_ms: u32, tsc_per_ms: u64, cpu_id: u32) -> u32;
    fn write_local_timer_icr(&mut self, _icr: u32) {}
    fn local_timer_eoi(&mut self) {}
    fn read_ticks(&self) -> u64;
    fn read_tsc(&self) -> u64 { self.read_ticks() }
    fn stop_local_timer(&mut self, cpu_id: u32);
    fn init_profile_clock(&mut self, hz: u32) -> Result<(), ProfileClockError>;
    fn stop_profile_clock(&mut self);
    fn ack_profile_clock(&mut self);
}
```

十一个方法按职责分四组：启动编程（`new`、`init_timer`）、读取（`read_ticks`、`read_tsc`）、AP 与停顿（`init_local_timer`、`write_local_timer_icr`、`local_timer_eoi`、`stop_local_timer`）、统计 profiling（三个方法）。本篇只用第一组与第二组；后两组里 `init_local_timer` 一批属于 AP 拉起（`16-smp`（SMP 篇，尚未重生成）），profile 三个方法属于 profiling 专题。

取舍与否决：

| 方案 | 结论 | 代价 / 否决理由 |
|---|---|---|
| 软件状态与硬件定时器分两个类型、两层 | 采用 | 两处 `hz` 常量要同步（§3.2）；调用点每次现构造一个 `ClockArch` 实例 |
| 合成一个结构体（`ClockState` 直接持有 `ClockArch`） | 否决 | 分层方向：kernel crate 单向依赖 arch crate（`os/arch/Cargo.toml` 的依赖表里没有 `minix-kernel`），状态层持有硬件层本身不成环；否决的是硬件操作与软件状态收进同一 crate 之后的耦合。[`clock.rs`](os/arch/src/arch/clock.rs) 的模块注释另记着一种真会成环的形态：把 `KProcess` 带进 arch 层的方法签名（曾考虑过的 `arch_tick(&mut KProcess)`），那要求 `minix-arch` 反向依赖 `minix-kernel` |
| 把硬件操作做成自由函数（按 `cfg` 分派） | 否决 | 三架构的差异是"配置哪台定时器、写哪个寄存器"，不是"有没有这个动作"；trait 让差异留在实现文件里 |

`Send + Sync` 在这里的含义需要与"线程安全"分开说。`Send` 表达"该类型的所有权可以跨线程转移"，`Sync` 表达"共享引用 `&T` 可以跨线程传递"（`&T: Send` 当且仅当 `T: Sync`）。两者都不保证"用 `&self` 调方法时的访问是安全的"，同步要靠显式的锁或原子量。回到 `ClockArch`：它的实例是**瞬态的**——每个调用点从 `platform_desc().timer()` 现构造一个，完成一次硬件操作后即丢弃（`lib.rs` 的 [`fn boot_init_timer`](os/kernel/src/lib.rs:fn boot_init_timer) 与 `clock.rs` 的 [`fn local_tick`](os/kernel/src/clock.rs:fn local_tick) 都是这个形状），并不存在"每核存一份实例"的结构。因此这里的 `Send + Sync` 编码的是"每核的定时器寄存器物理上互不干扰 + 引用可以传"，而不是"这是一个并发数据结构"。

**让 `ClockState::tick_with` 直接持有 `ClockArch` 实例会怎样**：把硬件读写与软件记账放进一个结构。这不会引入 crate 依赖环——`minix-kernel` 依赖 `minix-arch` 是单向的，`os/arch/Cargo.toml` 的依赖表里没有 `minix-kernel`；代价落在职责划分上：一个类型同时承担"读硬件计数器"与"记 uptime 与负载"两种职责，arch 层"只谈硬件"的边界被抹掉。真能把环引入的是另一种形态：把 `KProcess`（定义在 kernel crate）写进 arch 层的方法签名，那要求 `minix-arch` 反向依赖 `minix-kernel`（[`clock.rs`](os/arch/src/arch/clock.rs) 的模块注释记着这个例子）。落到对象：`os/kernel/Cargo.toml` 与 `os/arch/Cargo.toml` 的依赖表、[`clock.rs`](os/arch/src/arch/clock.rs) 的模块注释。

### 3.2 决策：DEFAULT_HZ 用编译时常量，范围的守卫留在构造器（→§2.2）

C 的 `env_get("hz")` 让 tick 频率成为一个 boot monitor 可以改的运行时量，代价是每个读它的算式都要假设"它已经是一个合法值"，而这个假设由 `init_clock` 里那行 `2..50000` 的范围回退保证。

Rust 的默认频率是一个编译时常量，两份同值副本：

```rust
// os/arch/src/arch/clock.rs:const DEFAULT_HZ —— 代码引用（文档注释与常量定义行；注释内的表格也在原文里）
/// Default clock tick frequency in Hz.
///
/// minix-rs uses 100 Hz (10ms tick) across all architectures.
///
/// # Architecture evolution ([ARCH: K-3], 32-bit → 64-bit)
///
/// | Architecture | Minix3 C (32-bit) | minix-rs (64-bit) |
/// |-------------|-------------------|-------------------|
/// | x86 | 60 Hz (`archconst.h`) | 100 Hz |
/// | ARM | 1000 Hz (`archconst.h`) | 100 Hz |
///
/// C: DEFAULT_HZ — i386/include/archconst.h:4 (60), earm/include/archconst.h:4 (1000)
///
/// minix-rs unifies to 100 Hz: ARM's 1000 Hz was for 32-bit embedded targets
/// with coarse timers; 64-bit platforms have high-resolution timers and
/// don't need 1ms ticks. 100 Hz matches typical server/desktop kernels.
///
/// **Authoritative definition**: this is the arch-level constant. The kernel
/// crate (`os/kernel/src/clock.rs`) has an independent copy to avoid a
/// cross-crate dependency; modify here first, then sync.
pub const DEFAULT_HZ: u32 = 100;
```

这段注释给了三件事：选择 100 Hz 的理由（ARM 的 1000 Hz 是 32 位嵌入式目标配粗粒度定时器的取值；64 位平台有高分辨率定时器，不需要 1 毫秒的 tick；100 Hz 与常见的服务器/桌面内核一致）、`[ARCH: K-3]` 标注（60/1000 → 100，同一处给出对照表）、以及两份副本的权威位置与修改顺序（先改 arch 层，再同步 kernel 层）。kernel 层的副本在 `clock.rs` 的 [`const DEFAULT_HZ`](os/kernel/src/clock.rs:const DEFAULT_HZ)，注释里额外写了一句"需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定"。

取舍与否决：

| 方案 | 结论 | 代价 / 否决理由 |
|---|---|---|
| 编译时常量 100 Hz | 采用 | 失去 boot 期调整 tick 频率的能力；三个架构行为一致 |
| 保留 `env_get("hz")` 的运行时读取 | 否决（保留在构造器） | Rust 侧的启动信息里没有 boot 参数字符串（[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.3 的 `param_buf` 用途已变），且频率是硬件相关的量 |
| 每架构一个默认值 | 否决 | 三个架构的默认值会立刻分叉，读者需要记住三套；统一值有源码注释给出的理由 |
| `with_hz(hz)` 构造器 | 保留 | 它保留了 C 的范围守卫 `2..=50000`（`clock.rs` 的 [`fn with_hz`](os/kernel/src/clock.rs:fn with_hz)），当前没有生产调用者，只在需要与 C 逐值对齐的场合使用 |

范围守卫的落点也在构造器里：

```rust
// os/kernel/src/clock.rs:fn with_hz —— 代码引用（函数体四行）
pub fn with_hz(hz: u32) -> Self {
    let hz = if !(2..=50000).contains(&hz) { DEFAULT_HZ } else { hz };
    let mut state = Self::new();
    state.hz = hz;
    state
}
```

与 C 的差别只有两条：没有"参数非数字"这条路径（Rust 侧没有字符串解析，调用者给的就是整数），以及默认值从 60/1000 变成 100。C 的四条取值路径在 Rust 侧的对应物：无参数 → `ClockState::new()` 直接用 `DEFAULT_HZ`；参数合法 → `with_hz(n)`；参数越界 → `with_hz` 内部回退；参数非数 → 不存在。

**让 `boot_init_timer` 把 `clock.hz()` 而不是 `DEFAULT_HZ` 传给 `init_timer` 会怎样**：如果将来有人用 `with_hz(60)` 建了 `ClockState`，PIT 会按 60 Hz 编程，而软件侧的记账常量仍是 100 Hz；被点名的冲突不是"字段与硬件"（按假设两者都是 60），而是**硬件 60 与仍在用编译期 100 的记账路径**：`uptime` 的推进速率与墙钟时间的比例从 1:1 变成 60:100——`sleep(1)` 会变成约 1.67 秒。落到对象：`DEFAULT_HZ` 的编译期读者、PIT divisor（60 Hz）、`uptime` 的推进速率。依据：`lib.rs` 的 [`fn boot_init_timer`](os/kernel/src/lib.rs:fn boot_init_timer) 传的实参、`clock.rs` 的 [`fn with_hz`](os/kernel/src/clock.rs:fn with_hz) 的存在。

### 3.3 决策：把控制器拆成 InterruptRouter 与 PerCpuInterruptUnit（→§2.3、§2.5）

C 里控制器的操作面由一组宏给出：[`include/hw_intr.h`](minix3/minix/kernel/arch/i386/include/hw_intr.h) 在 `USE_APIC` 时把 `hw_intr_mask`/`hw_intr_unmask`/`hw_intr_ack` 映射到 `ioapic_mask_irq`/`ioapic_unmask_irq`/`ioapic_eoi`，否则映射到 `irq_8259_mask`/`irq_8259_unmask`/`irq_8259_eoi`；ARM 一侧换一套函数。这一组动作在硬件上其实是两类：改全局路由表的（屏蔽/放行一条线，一改所有 CPU 都看见）与写当前核私有寄存器的（应答与结束）。

Rust 按这条缝隙拆成两个 trait：

```rust
// os/plat/src/interrupt.rs:trait InterruptRouter —— 签名示意（文档注释省略）
pub trait InterruptRouter: Sized + Send + Sync {
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self;
    fn init(&mut self);
    fn mask(&mut self, irq: IrqVector);
    fn unmask(&mut self, irq: IrqVector);
    fn mask_all(&mut self);
}

// os/plat/src/interrupt.rs:trait PerCpuInterruptUnit —— 签名示意（文档注释省略）
pub trait PerCpuInterruptUnit: Send {
    fn claim(&mut self) -> Option<u32>;
    fn complete(&mut self, claimed: Option<u32>);
}
```

`InterruptRouter` 的 `init()` 契约写在文档注释里：初始化控制器，之后每一条线都处于屏蔽。`mask_all()` 是这条契约的显式形式。`PerCpuInterruptUnit` 的两个方法成对出现，`claim` 返回的身份必须原样交给 `complete`——它的返回类型是 `Option<u32>` 而不是 `IrqVector`，因为拿回来的东西是硬件给的原始编号（GIC 的 INTID 可以超过 255），与"用哪条线的钩子链"是两个命名空间。

| 方案 | 结论 | 代价 / 否决理由 |
|---|---|---|
| 两个 trait（路由面 + 每核应答面） | 采用 | 调用点需要同时满足两个 bound（`IrqManager` 的泛型参数写的是 `IC: InterruptRouter + PerCpuInterruptUnit`） |
| 单一 `InterruptController`（六方法） | 否决 | 拆分前的六个应答实现（三架构 × `ack`/`eoi`）都忽略传入的 `irq` 参数、而 `mask`/`unmask` 确实按参数路由；把两类语义混在一个接口里，读者会以为 `ack(irq)` 的参数有用 |
| 把两个方法合成一个"原子应答并结束" | 否决 | GIC 的结束写回身份来自应答读的同一个寄存器；合起来就无法表达"claim 在陷阱入口、分发在别的函数"这种拆分（aarch64 的入口要先用 INTID 决定走哪条链） |

**把 `claim` 与 `complete` 合并、按"先回执再处理"调用会怎样**：GIC 上 `ICC_EOIR1_EL1` 写回的身份必须来自一次 `ICC_IAR1_EL1` 读；在处理器还没跑完时先回执，等于把本次中断的 active 优先级放掉，同优先级或更高优先级的待处理中断可以在处理函数返回之前打断它。落到对象：GIC 的 `ICC_IAR1_EL1` 与 `ICC_EOIR1_EL1` 这一对寄存器、PLIC 的 claim/complete 共用同一偏移（读是 claim、写是 complete，`interrupt.rs` 的 [`const PLIC_COMPLETE`](os/plat/src/riscv64/interrupt.rs:const PLIC_COMPLETE)）这一事实。依据：`interrupt.rs` 的 [`trait PerCpuInterruptUnit`](os/plat/src/interrupt.rs:trait PerCpuInterruptUnit) 的注释（"GIC/PLIC: write back the SAME identity `claim` returned — the pairing is the protocol"）与三架构的三份实现（测试与 mock 另有一份）。

### 3.4 决策：控制器的归宿是全局 `IRQ_MANAGER`（→§2.4）

C 的控制器状态是散在文件里的静态变量：[`kernel/interrupt.c`](minix3/minix/kernel/interrupt.c) 的 `irq_handlers[NR_IRQ_VECTORS]`、`i8259.c` 里的端口常量、`apic.c` 里的 `lapic_addr`。任何代码都能直接读到它们——`put_irq_handler` 甚至在同一段里改链表并调 `hw_intr_unmask`。

Rust 的控制器实例在 `init_clock_and_interrupts` 里构造、`init()` 之后立刻移进一个全局：

```rust
// os/kernel/src/lib.rs:fn init_clock_and_interrupts —— 节选（只收 Step 2 的一段；上文为该函数的 Step 1（时钟状态构造），下文为 Step 3（ArchInit））
    ...
    // Step 2: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    //
    // Instance-based design: construct the interrupt controller from the
    // interrupt controller descriptor, then call `init` on the instance.
    // The instance is then moved into the global `IRQ_MANAGER` so that
    // trap entry points and `bsp_finish_booting` can reach it.
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    intr.init();  // mask_all() called internally

    // D10: Store the interrupt controller in the global `IRQ_MANAGER`.
    // This replaces the previous pattern of dropping `intr` after init.
    // SAFETY: Boot is single-threaded before BKL exists; no concurrent access.
    unsafe {
        *IRQ_MANAGER.get() =
            Some(crate::irq_manager::IrqManager::new(intr));
    }
    ...
```

这段节选里的 `D10` 是仓库内部的阶段编号，按原文引用。它说明的事实是：曾经有 "init 完就丢弃实例" 的写法，改成移进全局是因为两处后来者要用控制器——陷阱入口（中断到达时分发）与启动末端（`boot_init_timer` 注册时钟钩子时，控制器侧的解屏蔽要经它）。

全局存的不是裸控制器，而是一个合成体：

```rust
// os/kernel/src/irq_manager.rs:struct IrqManager —— 代码引用（五个字段）
pub struct IrqManager<IC: InterruptRouter + PerCpuInterruptUnit> {
    hooks: [Option<IrqHookSlot>; NR_IRQ_HOOKS],
    handlers: [Option<usize>; NR_IRQ_VECTORS],
    actids: [IrqIdBitmap; NR_IRQ_VECTORS],
    irq_use: u64,
    controller: IC,
}
```

四个字段各自对一份 C 状态（第五个字段 `controller` 装的是控制器实例本身）：`handlers` 对 `irq_handlers[]`、`actids` 对 `irq_actids[]`、`hooks` 对 C 的钩子池（`irq_hooks[NR_IRQ_HOOKS]`）、`irq_use` 对"这条线被用过"的记账。把"哪条线被谁注册"与"控制器的屏蔽位"放在同一个对象里，使 §4.8 的那条规则（第一个注册者解屏蔽）有一个明确的实现位置。

访问这个全局有两套入口，分界是"BKL 是否存在"：启动期用 `lib.rs` 的 [`fn irq_manager_boot_unchecked`](os/kernel/src/lib.rs:fn irq_manager_boot_unchecked)（`unsafe`，契约是"单线程初始化期"），运行期用 `lib.rs` 的 [`fn irq_manager_with`](os/kernel/src/lib.rs:fn irq_manager_with)（接收一个 `BklSection` 见证）。时钟状态同样有两套（`lib.rs` 的 [`fn clock_state_with`](os/kernel/src/lib.rs:fn clock_state_with) 与 `lib.rs` 的 [`fn clock_state_boot_unchecked`](os/kernel/src/lib.rs:fn clock_state_boot_unchecked)）。

**`intr.init()` 之后把实例丢弃、让陷阱入口每次中断现构造控制器会怎样**：构造要做一次 downcast 与基址搬运，这是次要代价；真正的代价落在**实例承载的硬件侧状态**上：GIC 的 `ICC_IAR1_EL1` 读取本身就是应答动作，谁读走那个编号谁就欠一次 `ICC_EOIR1_EL1` 的回执（§4.5 的三架构对照）。实例用完即弃意味着"读"与"写"分属两个不同的对象，而回执必须写回**同一个**被应答的 INTID——编号此刻只存在于栈上的一个局部量里，任何一次重构都可能把它丢掉或写错。落到对象：`ICC_IAR1_EL1` / `ICC_EOIR1_EL1` 这对寄存器，以及 `fn claim(&mut self) -> Option<u32>` 与 `fn complete(&mut self, claimed: Option<u32>)` 之间那条靠参数传递的身份。依据：`interrupt.rs` 的 [`fn claim`](os/plat/src/arm64/interrupt.rs:fn claim) 与 `:fn complete`（`claimed` 是参数、不是实例字段）。

### 3.5 决策：ArchInit 是阶段 trait（→§2.4、§2.5）

C 的 `arch_init` 用 `#ifdef USE_APIC`、`#ifdef USE_ACPI`、`#ifndef CONFIG_SMP` 在一具函数体里选分支；x86 与 ARM 两侧的实现内容几乎不重叠（§2.4 与 §2.5 的对照表）。这些杂项里没有任何一条有跨架构一致的语义。

Rust 把它收进一个 trait：

```rust
// os/arch/src/arch/arch_init.rs:trait ArchInit —— 签名示意（文档注释省略）
pub trait ArchInit: Sized + Send + Sync {
    fn new(desc: &minix_platform::ArchMiscDesc) -> Self;
    fn init(&mut self);
}
```

trait 的文档注释把边界写死在一句话里：`ArchInit` 是**阶段 trait**，不是功能 trait；它收集的是"没有跨架构抽象"的架构杂项，TSS 与 APIC 分别属于保护模块与中断控制器模块，不在这里。这条界定在现码里的落地可以逐行列出——每一行是一条去向与它的判据：

| 内容 | 去向 | 判据 |
|---|---|---|
| 时钟节拍、中断路由、早期控制台 | 各自的 trait（`ClockArch`、`InterruptRouter`、`EarlyConsole`） | 有跨架构一致语义 |
| APIC / GIC / PLIC 的初始化 | `InterruptRouter::init` | 是"中断控制器这台外设"自身 |
| COM1 / PL011 / SBI 控制台 | `EarlyConsole` | 是"输出设备"自身 |
| 内存裁剪 | 不进来（§3.6） | 属于内存管理，不是硬件初始化 |
| ACPI 表地址、PMU 使能、FPEN | `ArchInit` | 本身就没有跨架构一致性 |

最后一条判据的作用是把两类东西分开：`ArchInit` 里放的是"**本身就没有**跨架构一致性"的杂项，不是"所有架构都不同"的东西。两者的差别在于后者里常常藏着可以被抽象出来的公共语义——串口就是一个例子：三个架构的早期输出都是"一个字节一个字节地写"，差异只在寄存器位置，于是它有自己的 trait（[`early_console.rs`](os/plat/src/early_console.rs)）。本篇只用它的"能打字"这一面（§6.2 的命令输出就是它在打）；它的完整编程（串口初始化序列、FIFO 设置、为什么要在 `prot_init` 之前就能输出）属控制台与外设驱动主题（`16-drivers` 方向；串口芯片的寄存器级展开在该 stage 的终端驱动篇与后续架构文档），本篇不展开。

三架构 `ArchInit::init()` 今天的实际内容：

```rust
// os/arch/src/riscv64/arch_init.rs:fn init —— 代码引用（函数体的注释与全部语句）
    fn init(&mut self) {
        // 1. PMP (Physical Memory Protection): NO S-mode setup possible or
        // needed. PMP CSRs (pmpcfg*/pmpaddr*) are M-mode-only per the
        // privileged spec — an S-mode write takes an illegal instruction
        // (live, first timer-irq carrier run: scause=2,
        // stval=0x3b051073 = `csrw pmpaddr0`). Under OpenSBI the firmware
        // owns PMP and its Domain0 regions already delegate S/U (R,W,X)
        // over the whole address space, so the allow-all intent this
        // entry once tried to program is satisfied by the firmware.
        //
        // 2. (Removed by D-59, 2026-09-09.) This step used to set
        // `sie = STIE | SSIE` (0x22) here — enabling the S-mode timer gate
        // before any handler is registered. Both bits now have owners:
        //   - STIE (timer): opened by `TimerIrqGate::enable_timer_irq` at
        //     bsp_finish_booting Step 6, after the handler registration
        //     (C `boot_cpu_init_timer` position, clock.c:294).
        //   - SSIE (software/IPI): belongs to the SMP IPI bring-up
        //     (smp_todo.md S-7/S-10) — enabling it before an IPI producer
        //     and handler exist would arm a second unhandled source.
        // SEIE (external) stays per-source via `InterruptController::unmask`.
    }
```

块内有一处仓库内部的阶段编号与两处工作记录编号（`smp_todo.md` 的条目号），按原文引用；本段要读的是它陈述的机制：这条写入曾经在 `ArchMiscDesc` 的初始化阶段就把 S 模式定时器门打开，而那个时刻定时器源还没编程、处理函数还没注册；现在两个位各有归属——定时器位归 `TimerIrqGate`，软件中断位归 SMP 的 IPI 拉起。

**把 `sie.STIE` 的写入放回 `ArchInit::init()` 会怎样**：这个函数的执行点在 `init_clock_and_interrupts` 里，而 `boot_init_timer`（编程定时器、注册处理函数、开门）在它之后——门在源与处理者都不存在时就被打开。RISCV 的定时器条件比较是 `mtime >= mtimecmp`，而 `mtimecmp` 的复位值是 0，`mtime` 从复位起就在递增，于是条件恒真：一旦 `sie.STIE` 置位，`sip.STIP` 立刻挂起，中断在启动中途反复到达。落到对象：`sie` 的 STIE 位（bit 5）、`sip` 的 STIP 位、`mtimecmp` 的复位值、`irq_handlers[TIMER_IRQ]` 的空链。依据：`arch_init.rs` 的 [`fn init`](os/arch/src/riscv64/arch_init.rs:fn init) 的注释、`timer_irq_gate.rs` 的 [`fn enable_timer_irq`](os/arch/src/riscv64/timer_irq_gate.rs:fn enable_timer_irq) 的写值。

### 3.6 决策：内存裁剪不进 `ArchInit`（→§2.4）

C 的 `arch_init` 末尾两次 `cut_memmap` 改的是系统内存图，不是任何一台外设；它出现在这里只是因为 `arch_init` 是"剩下的事情都放进来"的那具函数。

Rust 不做这两次裁剪，理由有三条，都能从源码指出：

1. `KernelInfo.memmap` 的提供者已经把保留区域排除掉了。内核自己那一侧还有一组 `cut_memmap` 调用，跑在 Phase A.2，切的是 boot module 区间与内核镜像本身（[`lib.rs`](os/kernel/src/lib.rs) 的 Phase A 段；01 篇讲了这条链的完整生命周期，见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4）。
2. `BIOS_MEM_BEGIN`/`BIOS_MEM_END`、`BASE_MEM_TOP`/`UPPER_MEM_END` 是 x86 的固件内存布局常量；aarch64 与 riscv64 上没有这些区间。
3. "裁剪内存图"是内存管理的职责，把它放进 `ArchInit` 会让这个阶段 trait 的边界失效。

**把两次 `cut_memmap` 照抄进 `ArchInit` 会怎样**：这个 trait 的三个实现里，aarch64 与 riscv64 的实现会在自己平台上执行两条来自 x86 常量名的裁剪：`BIOS_MEM_END` 与 `BASE_MEM_TOP` 这些符号在非 x86 平台上没有对应语义，落到 `FREE_MEMMAP` 的区间表上就是两条凭空的保留区间——它们可能正好盖住真实可用的物理页，把这些页从分配器里抹掉。落到对象：`kinfo.memmap`（C）/ `FREE_MEMMAP`（Rust）的区间表、`BIOS_MEM_BEGIN` 与 `BASE_MEM_TOP` 这些常量的取值来源。依据：[`arch_init.rs`](os/arch/src/arch/arch_init.rs) 的模块注释（`No memory cutting`）与 `arch_init.rs` 的 [`fn init`](os/arch/src/x86_64/arch_init.rs:fn init) 的现状（它不做任何裁剪）。

### 3.7 决策：定时器的"编程、注册、放行"三段分离（→§2.6）

C 把编程与注册连在两个函数调用里（`boot_cpu_init_timer`），而"注册即放行"这一半没有独立的名字——它藏在两处调用者的行为里：x86 的 PIC 支路藏在 `put_irq_handler` 末尾的 `hw_intr_unmask`，ARM 一侧显式写成注册之后的 `bsp_irq_unmask`（C 另有具名的 `enable_irq`，那是处理完成后的重新放行，不是首次放行）。三件事在 C 里存在，但首次放行没有自己的名字。

Rust 把它们拆成三个有名字的动作，并收进一个函数：

```rust
// os/kernel/src/lib.rs:fn boot_init_timer —— 节选（省略号处依次为函数头文档注释、两条 `use` 声明、三段之间的解释性注释、一个空的 mock 条件块，以及仅 x86_64 的 `pic_init` 尾调用）
pub fn boot_init_timer() {
    ...
    use minix_arch::{ClockArch, CurrentClockArch, CurrentTimerIrqGate, TimerIrqGate};
    ...
    let pd = platform_desc();
    let mut clock_arch = CurrentClockArch::new(pd.timer());
    clock_arch.init_timer(crate::clock::DEFAULT_HZ, crate::clock::current_cpuid().raw());
    ...
    let clock_ep = Endpoint::from_generation_slot(0, crate::proc::proc_nr::CLOCK.0);
    // A1: boot context — no BKL, no IRQs yet; boot_unchecked accessor.
    unsafe { crate::irq_manager_boot_unchecked() }
        .register_hook(
            minix_plat::TIMER_IRQ,
            crate::clock::clock_irq_handler,
            clock_ep,
            minix_plat::IrqNotifyId(0),
            minix_plat::IrqPolicy::REENABLE,
        )
        .expect("register clock IRQ hook: no free slots in IRQ_MANAGER");
    // (c) — gates last, handler already registered (see the Step 6 header
    // for the per-architecture semantics of this call).
    <CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
    ...
}
```

三段：

1. **编程**：`CurrentClockArch::new(pd.timer())` 从平台描述符取硬件参数，`init_timer` 按 `DEFAULT_HZ` 与当前 CPU 编号配置本核定时器。三架构上它写的东西见 §4.3。
2. **注册**：`IrqManager::register_hook` 挂在 `minix_plat::TIMER_IRQ` 上。这一步同时完成控制器侧的放行——C 的同一条规则（§2.6 的 `put_irq_handler` 末尾）由 `register_hook_at_slot` 里的 `unmask` 承担（§4.8）。
3. **放行**：`TimerIrqGate::enable_timer_irq` 打开"模块本地门"。它与第 2 步是两级不同的门：控制器侧的门管"这条线能不能送到 CPU"，模块本地的门管"这台定时器模块要不要产生中断"。

trait 只有两个方法：

```rust
// os/arch/src/arch/timer_irq_gate.rs:trait TimerIrqGate —— 签名示意（文档注释省略）
pub trait TimerIrqGate: Sized {
    fn enable_timer_irq();
    fn disable_timer_irq();
}
```

三架构的"模块本地门"完全不同，这是这个 trait 存在的理由：

| 启动时钟源 | 模块本地门 | `enable_timer_irq` 写什么 | 控制器侧的门 |
|---|---|---|---|
| x86-64：8254 PIT | 无。PIT 是一台只接输出线的设备，交付门就是 IOAPIC 的那条线 | 文档化的空操作 | IOAPIC 输入 2（ISA IRQ0）的 mask 位，由第 2 步的 unmask 解 |
| aarch64：CNTP | `CNTP_CTL_EL0` 的 Enable/IMASK 两位 | `msr CNTP_CTL_EL0, 1`（Enable=1、IMASK=0）后接 `isb` | `GICR_ISENABLER0` 的 bit 30，由第 2 步的 unmask 置位 |
| riscv64：CLINT `mtimecmp` | `sie.STIE`（bit 5） | `csrs sie, 0x20` | 无——定时器是核本地中断，没有控制器线 |

x86 那一格的两个"没有"是本实现的选择而不是三架构共有的推论：PIT 没有模块本地门，所以两个方法都是空操作（`timer_irq_gate.rs` 的 [`struct X86_64TimerIrqGate`](os/arch/src/x86_64/timer_irq_gate.rs:struct X86_64TimerIrqGate)，文件注释记着"早先的实现会去清 LAPIC LVT Timer 的 mask 位，那是给 LAPIC 定时器准备的门，而 LAPIC 定时器不是当前的时钟源、也没有被编程——提前打开它等于开放一个没有处理者的中断源"）。将来时钟源换成 LAPIC 定时器时，这个空操作会换成 LVT 位的操作。

**三条推演（只做前两段；跳过注册；把第三段提到第一段之前）**：

1. 只做前两段、不做第三段。aarch64 的 `init_timer` 特意把 `CNTP_CTL_EL0` 写成 `0x2`（Enable=0、IMASK=1），定时器模块不产生 PPI——即使控制器侧的 bit 30 已经置位，也没有中断可以投递。`uptime` 永不推进。落到对象：`CNTP_CTL_EL0` 的 bit 0 与 bit 1、PPI 的 INTID 30。
2. 只做第一段与第三段、跳过注册。两级门都开了，处理器不存在；中断到达后分发器在 `irq_handlers[TIMER_IRQ]` 上找不到链，返回 `IrqError::Spurious`。落到对象：钩子池的空槽位、`irq_manager.rs` 的 [`fn dispatch_claimed`](os/kernel/src/irq_manager.rs:fn dispatch_claimed) 里 `slot_idx.is_none()` 的分支。
3. 把第三段提到第一段之前。aarch64 上写 `CNTP_CTL_EL0 = 1` 时 `CNTP_CVAL_EL0` 还是复位值 0，比较条件 `CNTPCT >= 0` 恒真，PPI 立刻持续挂起；riscv64 上写 `sie.STIE` 时 `mtimecmp` 还是 0，`sip.STIP` 同理。这是"中断风暴"的装置，而不是一次可恢复的错误。落到对象：`CNTP_CVAL_EL0` 与 `mtimecmp` 的复位值、`CNTP_CTL_EL0.Enable`、`sie.STIE`、`sip.STIP`。依据：`clock.rs` 的 [`fn init_timer`](os/arch/src/arm64/clock.rs:fn init_timer) 的写值与注释、`clock.rs` 的 [`fn init_timer`](os/arch/src/riscv64/clock.rs:fn init_timer) 的 SBI 调用。

与 C 对照，三条次序约束一一对得上：

| Rust | C 的对应物 | 次序约束 |
|---|---|---|
| `ClockArch::init_timer` | `init_local_timer` | 先于注册与放行（C 中 `boot_cpu_init_timer` 的第一句） |
| `IrqManager::register_hook` | `put_irq_handler` | 在放行之前；它自己内含控制器侧放行 |
| `TimerIrqGate::enable_timer_irq` | ARM 的 `bsp_irq_unmask` / x86 的空缺 | 三段里的最后一段 |

与这条决策线相连的另两条边界决策——handler 注册为什么不进 `TimerIrqGate`、aarch64 两级门的归属——写在 §4.7.1 与 §4.7.2，与本节同属一条决策线。

### 3.8 决策：三架构差异与三档术语的落点（→§2.5）

Minix3 只覆盖 x86-32 与 ARM-32 两种架构，本篇实现面对的是 x86-64、aarch64、riscv64 三种机器。差异因此分成三类，各自要给不同的标注：

| 演进 | 内容 | C 现状 | 本实现 | 依据锚点 |
|---|---|---|---|---|
| `[ARCH: K-1]` | 控制器与定时器型号 | 32 位 ARM 用 OMAP INTC（`ti/omap_intr.c` 的 [`intr_init`](minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:intr_init)）+ BSP 私有定时器（`ti/omap_timer.c` 的 [`bsp_register_timer_handler`](minix3/minix/kernel/arch/earm/bsp/ti/omap_timer.c:bsp_register_timer_handler)） | ARMv8-A：GICv3（Distributor + Redistributor + CPU 接口）+ Generic Timer（CNTP 一组寄存器） | [`interrupt.rs`](os/plat/src/arm64/interrupt.rs)、[`clock.rs`](os/arch/src/arm64/clock.rs) 的模块注释 |
| `[ARCH: K-2]` | 整套 RISC-V 支持 | 无对应实现 | CLINT 的 mtime/mtimecmp + PLIC + `sie.STIE`；读计数走 `time` CSR、编程走 SBI TIME ecall | [`interrupt.rs`](os/plat/src/riscv64/interrupt.rs)、[`clock.rs`](os/arch/src/riscv64/clock.rs) 的模块注释 |
| `[ARCH: K-3]` | 默认 tick 频率 | i386 60 Hz、earm 1000 Hz，且可由 boot 参数覆盖 | 三架构统一 100 Hz 编译期常量 | `clock.rs` 的 [`const DEFAULT_HZ`](os/arch/src/arch/clock.rs:const DEFAULT_HZ) 的注释、`clock.rs` 的 [`const DEFAULT_HZ`](os/kernel/src/clock.rs:const DEFAULT_HZ) |

三类之外的都属于 Rewrite 或 Refactor：`init_clock` 的软件状态语义、`cut_memmap` 的裁剪语义（由 `KernelInfo` 提供者代为排除，结果表等价而非同址重写，§3.6）、`put_irq_handler` 的"注册即放行"规则、`boot_cpu_init_timer` 的三段关系，在 Rust 侧都保持同样的外部行为，只换了内部组织方式。

差异表里有一处不属于 Rewrite：riscv64 的定时器编程通道。C 没有这一架构的实现，所以没有"保持行为"可言；而实现内部还有一处不一致——读计数用 `time` CSR，写比较值用 SBI ecall——理由是 OpenSBI 把 CLINT 的 MMIO 帧放在 PMP 保护的 M 模式内存里，S 模式的 MMIO 读会取访问异常（`clock.rs` 的 [`fn read_ticks`](os/arch/src/riscv64/clock.rs:fn read_ticks) 的注释给出的实测读数是 `scause=5`、`stval=0x200bff8`），而 `time` CSR 是同一个计数器的 S 模式合法视图。

**把 riscv64 的计数读改成 MMIO 会怎样**：这条计数读（`read_ticks`，以及同一 CSR 的兄弟读取 `read_time_csr`）会被 `decrement_quantum`（经 `read_tsc` 的默认委托）、`bsp_finish_booting` 的 TSC 基线、`local_timer_eoi` 的重挂路径调用；第一次调用就取 S 模式访问异常，异常发生在刚建立的陷阱入口上；`scause=5` 的编码就是读访问故障（Load access fault，RISC-V 特权规范），`stval` 给出出错的地址。落到对象：`mtime_addr` 这个 MMIO 地址、PMP 的配置、`time` CSR 的读取契约。依据：`clock.rs` 的 [`fn read_ticks`](os/arch/src/riscv64/clock.rs:fn read_ticks) 的注释与它返回的 CSR 读。

---

## 4. 实现详解

### 4.1 `init_clock_and_interrupts` 的逐步走查（→3.1、3.3、3.4、3.5）

函数不长，每一步都对应第 3 章的一条决策。原文（节选）：

```rust
// os/kernel/src/lib.rs:fn init_clock_and_interrupts —— 节选（省略号处为各段之间的解释性注释：SAFETY 说明与阶段编号注释；开头 `use minix_arch` 的跨行写法已并为一行）
pub fn init_clock_and_interrupts() {
    use minix_arch::{ArchInit, CurrentArchInit};
    use crate::clock::ClockState;
    use minix_plat::{InterruptRouter, CurrentInterruptController};
    use minix_platform::{platform_desc, PlatformDesc};

    // Obtain the platform descriptor (initialized earlier from KernelInfo).
    // This is the single source of truth for all hardware parameters.
    let pd = platform_desc();

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.
    ...
    // Step 2: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    ...
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    intr.init();  // mask_all() called internally
    ...
    unsafe {
        *IRQ_MANAGER.get() =
            Some(crate::irq_manager::IrqManager::new(intr));
    }
    ...
    // Step 3: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    ...
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();
    ...
    unsafe {
        *CLOCK_STATE.get() = Some(clock);
    }
}
```

五步（步 0 是前置的取描述符）与 C 的对应：

| 步 | 动作 | C 对应 | 回指的决策 | 这一步之后多了什么 |
|---|---|---|---|---|
| 0 | 取平台描述符 | C 的 `kinfo` 全局（在 Rust 侧是 Phase A.5 冻结的平台描述，[04-platform-discovery.md](04-platform-discovery.md) §4.7） | [04-platform-discovery.md](04-platform-discovery.md) §3.4、§4.8 | 三次实例化所需的地址与频率来源 |
| 1 | `ClockState::new()` | `init_clock` | §3.1、§3.2 | 软件时钟状态（局部变量，还没入全局） |
| 2 | 控制器 `new` + `init` + 入全局 | `intr_init(0)` | §3.3、§3.4 | 控制器处于全屏蔽态，且可被陷阱入口与启动末端取到 |
| 3 | `ArchInit::new` + `init` | `arch_init` | §3.5、§3.6 | 架构杂项就位 |
| 4 | 时钟状态入全局 | —（C 是全局变量，无需这一步） | §3.4 | `clock_state_with` 的读者可用 |

两处全局写入的时机不同：控制器在 Step 2 后就进了全局（陷阱入口随时可以来取），而时钟状态到 Step 4 才进全局。也就是说，`ArchInit::init()` 执行期间，若真有中断到达并走到 `clock_irq_handler`，它取时钟状态会命中：

```text
CLOCK_STATE not initialized — init_clock_and_interrupts must run first
```

这条 `expect` 的信息本身就是一件证据：它说明这条路径被认为"不该发生"，而保证它不发生的是另外两件事——控制器此刻每条线都还屏蔽着，且定时器的本地门也没开。

**把 Step 2 与 Step 3 对调、让 `ArchInit::init()` 先跑会怎样**：x86 的 `ArchInit` 实现今天只存 ACPI 表地址、不做实际动作（§4.6），所以对 x86 影响为零；但会同时破坏两条约定的**理由**：控制器还没有建立"全屏蔽"的确定状态；而 `ArchInit` 的边界（§3.5：只收没有跨架构一致性的杂项，不碰各 trait 的门）今天不含任何中断使能——一旦某个架构的 `ArchInit` 实现里出现任何一次中断使能，它就会在控制器状态未定义时生效。落到对象：`IRQ_MANAGER` 的写入点、`mask_all` 的执行点、`ArchInit` 三架构实现的语句集合。

### 4.2 `ClockState`：软件状态的形状（→3.1、3.2）

`ClockState` 把 C 的三份全局状态收进一个结构：

```rust
// os/kernel/src/clock.rs:struct ClockState —— 代码引用（字段与注释）
pub struct ClockState {
    /// CPU id this instance belongs to.
    /// C: `cpuid` — `get_cpulocal_var(cpu)`
    cpu_id: CpuId,
    /// `true` iff this is the BSP instance (owns global time + alarm timers).
    /// C: `cpu_is_bsp(cpuid)` — clock.c:91
    is_bsp: bool,
    /// Clock frequency in Hz. C: `kclockinfo.hz`
    hz: u32,
    /// Monotonically increasing ticks since boot. C: `kclockinfo.uptime` (BSP only)
    uptime: u64,
    /// Wall-clock ticks since boot (affected by adjtime). C: `kclockinfo.realtime` (BSP only)
    realtime: u64,
    /// UNIX epoch seconds at boot. C: `kclockinfo.boottime` (BSP only)
    boottime: u64,
    /// Time adjustment delta (positive=speed up, negative=slow down).
    /// C: `adjtime_delta` (clock.c:42, BSP only)
    adjtime_delta: i32,
    /// Head of the alarm timer chain (BSP only): index of the first
    /// `KPriv::runtime.s_alarm_timer` node, or `None` when empty.
    /// C: `static minix_timer_t *clock_timers` — clock.c:37.
    /// Design decision D2: intrusive index chain (nodes embedded in PrivTable)
    /// replaces C's pointer chain — zero heap allocation.
    timers_head: Option<PrivId>,
    /// Load average info. C: `kloadinfo` (all CPUs)
    load_info: LoadInfo,
}
```

字段与 C 的对应关系：

| Rust 字段 | C 对应物 | 谁拥有 |
|---|---|---|
| `cpu_id` | `cpuid`（`get_cpulocal_var(cpu)`） | 每实例 |
| `is_bsp` | 无字段，用 `cpu_is_bsp(cpuid)` 现算（`kernel/clock.c` 的 [`timer_int_handler`](minix3/minix/kernel/clock.c:timer_int_handler) 的第一处分支） | 每实例 |
| `hz` | `kclockinfo.hz`（别名 `system_hz`） | BSP；AP 实例也有值但不参与全局时间 |
| `uptime` | `kclockinfo.uptime` | BSP |
| `realtime` | `kclockinfo.realtime` | BSP |
| `boottime` | `kclockinfo.boottime` | BSP |
| `adjtime_delta` | `static int32_t adjtime_delta`（[`kernel/clock.c`](minix3/minix/kernel/clock.c)） | 每实例（`set_adjtime_delta` 有 BSP 守卫） |
| `timers_head` | `static minix_timer_t *clock_timers` | BSP |
| `load_info` | `kloadinfo` | 每实例（C 里 `load_update` 所有 CPU 都写） |

（`timers_head` 与 `adjtime_delta` 的运行时语义——到期扫描、`EXP_TIME`、`expire_alarm_timers`——属 `15-clock-timer`；本节只讲形状与所有权。）

构造有三个入口（`clock.rs` 的 [`fn new_for_cpu`](os/kernel/src/clock.rs:fn new_for_cpu) 是共同底座）：`new()` 建 BSP 实例、`new_for_cpu(cpu_id, is_bsp)` 显式指定、`with_hz(hz)` 带范围守卫（§3.2）。BSP 实例拥有全局时间与 alarm 定时器链，AP 实例只跟踪负载——这条区分在 C 里是每处现算的 `cpu_is_bsp(cpuid)` 分支，在 Rust 里是实例字段 `is_bsp` 加运行时分支（`clock.rs` 的 [`fn tick_bsp`](os/kernel/src/clock.rs:fn tick_bsp) 与 `:fn tick_ap` 是带 `debug_assert` 的包装）。

BSP-only 的守卫写在挂定时器的自由函数里（`set_alarm_timer` 的断言；摘除路径 `reset_alarm_timer` 不带守卫）——不是 `ClockState` 的方法，定时器的增删改查后来被移出结构体，[`clock.rs`](os/kernel/src/clock.rs) 里的注释记着这次搬迁：

```rust
// os/kernel/src/clock.rs:fn set_alarm_timer —— 节选（函数头与守卫断言）
pub fn set_alarm_timer(
    priv_table: &mut PrivTable,
    clock: &mut ClockState,
    priv_id: PrivId,
    exp_time: u64,
    action: TimerAction,
) {
    assert!(
        clock.is_bsp,
        "set_alarm_timer called on AP ClockState (timers are BSP-only)"
    );
```

**去掉这条断言、让 AP 实例也能挂定时器会怎样**：alarm 链头 `timers_head` 是每实例一份，而 `tick_with` 的到期扫描只在 `is_bsp` 为真时执行（`clock.rs` 的 [`fn tick_with`](os/kernel/src/clock.rs:fn tick_with) 第 5 步）；挂在 AP 实例上的定时器永远不会被弹出，它的持有者会永远等不到通知。落到对象：AP 实例的 `timers_head`、`tick_with` 的 BSP 分支、`expire_alarm_timers` 的唯一调用点。

### 4.3 `ClockArch` 的三架构实现（→3.1）

三个实现都在做同一件事：把"每秒 hz 次"翻译成本架构定时器的编程动作。

x86-64 用 8254 PIT：

```rust
// os/arch/src/x86_64/clock.rs:fn init_timer —— 代码引用（函数体，含守卫断言与三次端口写）
    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // C: intr_init_8254() — i8259.c equivalent
        //
        // The PIT is a single system-wide device; the per-CPU LAPIC timer
        // (used later, and by `stop_local_timer`) is reached through the
        // same MMIO base on every CPU, so `cpu_id` is not needed here.
        //
        // PIT divisor is 16-bit, so hz must be >= 19 (1193182 / 65535 ≈ 18.2).
        // Values below 19 would overflow the divisor.
        assert!(hz >= 19, "PIT divisor overflow: hz must be >= 19, got {}", hz);

        let divisor = (self.pit_base_freq / hz) as u16;

        unsafe {
            // Send command byte: channel 0, lobyte/hibyte, rate generator
            core::arch::asm!("out dx, al", in("dx") PIT_COMMAND, in("al") PIT_CMD_RATE_GEN);
            // Send divisor low byte
            let lo = divisor as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") lo);
            // Send divisor high byte
            let hi = (divisor >> 8) as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") hi);
        }
    }
```

三个常量在文件顶部：`PIT_COMMAND = 0x43`、`PIT_CHANNEL0 = 0x40`、`PIT_CMD_RATE_GEN = 0x36`。引文里那行注释的 C 对位名（`intr_init_8254`）在本仓 C 树里不存在——PIT 编程的 C 对位是 `init_8253A_timer`（`i386/arch_clock.c` 的 [`init_8253A_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_8253A_timer)，端口常量 `TIMER_MODE`/`TIMER0` 在 `ports.h`），`i8259.c` 管的是 8259A 的初始化；正文按实际情况陈述，注释的更新是代码侧的事。前两个端口号与 C 的 `i386/arch_clock.c` 的 [`init_8253A_timer`](minix3/minix/kernel/arch/i386/arch_clock.c:init_8253A_timer) 用的 `TIMER_MODE`/`TIMER0` 是同一对；命令字节 `0x36` 与 C 的 `SQUARE_WAVE 0x36` 同值，注释都描述为"速率发生器/方波、低字节后高字节"。`pit_base_freq` 来自平台描述符（`x86_64.rs` 的 [`struct PitDesc`](os/libs/minix-platform/src/arch/x86_64.rs:struct PitDesc) 的字段注释写着 fixed hardware constant `1_193_182`，构造点填的也是这个数），divisor 是 16 位，所以 `hz < 19` 时 `(pit_base_freq / hz) as u16` 会截断——断言把这条边界挡在编程之前。两个 hz 守卫的域关系在这里说清：`with_hz` 的 `2..=50000` 与这条断言是**交叉而非包含**——`[2,18]` 能通过构造器却会在这里 panic（`with_hz` 当前无生产调用者，这条交叉暂不可达；§3.2 的表记着这一项）。C 侧同区间更安静：`init_clock` 的 `2..50000` 放行 `[2,18]`，而 `TIMER_COUNT(freq)` 的商超过 16 位、低/高字节写入截断——没有断言拦它。

aarch64 用 Generic Timer 的 EL1 物理定时器：

```rust
// os/arch/src/arm64/clock.rs:fn init_timer —— 节选（省略号处依次为函数开头的注释块与 `CNTPCT_EL0` 读取段及其注释）
    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        ...
        let freq: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
        }
        ...
        let interval = freq / hz as u64;
        let compare = current_count + interval;
        unsafe {
            // Set the compare value for the EL1 physical timer
            core::arch::asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // Program the timer module but leave it gated:
            // CNTP_CTL_EL0 = 0b10 (Enable=0, IMASK=1). Opening this gate is
            // `TimerIrqGate::enable_timer_irq`'s job, done at the C
            // `boot_cpu_init_timer` position (bsp_finish_booting Step 6)
            // AFTER the handler is registered — not here. Arming + opening
            // in one step (the pre-D-59 behavior) left the timer live
            // through the rest of boot with no handler attached.
            // C parity: init_local_timer only programs the source; the
            // enable is register_local_timer_handler's `enable_irq`
            // (arch_clock.c:177-196 / interrupt.c:65).
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 0x2u64);
        }
        // Record the interval for the per-tick re-arm (`local_timer_eoi`
        // — CNTP is a one-shot; without the re-arm the tick train stops
        // dead after the first interrupt).
        TICK_INTERVAL.store(interval, core::sync::atomic::Ordering::Relaxed);
    }
```

两个细节：`CNTP_CVAL_EL0` 是绝对值（写的是"当前计数 + 一个 tick 的间隔"，不是"间隔"）；`CNTP_CTL_EL0` 特意写成 `0x2`（bit 0 = Enable 为 0、bit 1 = IMASK 为 1），把门关着——注释里有一处仓库内部的阶段编号，按原文引用，它陈述的事实是"曾经在这一步连同开门一起做，结果定时器在整个启动剩余过程里都是活的而没有处理者"。

`TICK_INTERVAL` 是一个 `AtomicU64` 静态量，因为实例是瞬态的（§3.1）：`init_timer` 把间隔记进静态量，每次 tick 的重挂路径从它读回（`clock.rs` 的 [`fn rearm_next_tick`](os/arch/src/arm64/clock.rs:fn rearm_next_tick)）。同样因为"CNTP 是一次性比较"——每触发一次就要写下一次的比较值，否则 tick 只来一次。

riscv64 用 CLINT 的 `mtimecmp`，但编程通道是 SBI：

```rust
// os/arch/src/riscv64/clock.rs:fn init_timer —— 节选（省略号处为注释块）
    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        ...
        let interval = self.freq / hz as u64;
        let now = Self::read_time_csr();
        // SAFETY: an SBI ecall with a deadline in the future; see the
        // safety note on `sbi_set_timer`.
        unsafe {
            sbi_set_timer(now + interval);
        }
        TICK_INTERVAL.store(interval, core::sync::atomic::Ordering::Relaxed);
    }
```

`read_time_csr` 读 `time` CSR，`sbi_set_timer` 发 SBI TIME 扩展的 ecall（`clock.rs` 的 [`fn sbi_set_timer`](os/arch/src/riscv64/clock.rs:fn sbi_set_timer)，扩展号 `0x5449_4D45` 是 "TIME" 的 ASCII）。同一份文件里注释给出了两个通道各自的理由：读走 CSR 是因为 CLINT 的 MMIO 帧被 PMP 保护，写走 ecall 是因为 S 模式无法直接写 M 模式的比较器。

三架构对照：

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 编程对象 | 8254 PIT 通道 0 | `CNTP_CVAL_EL0` + `CNTP_CTL_EL0` | `mtimecmp`（经 SBI） |
| 周期还是单次 | 周期（速率发生器模式） | 单次（每次 tick 重挂） | 单次（每次 tick 重挂） |
| `cpu_id` 参数 | 忽略 | 忽略 | 也忽略（写的是"本 hart 的比较器"，SBI 以调用 hart 为对象） |
| 门的初始状态 | 无本地门 | 关（`0x2`） | 关（`sie.STIE` 不动） |

**把 aarch64 的结尾写值改成 `1`、把放行并进编程会怎样**（Enable=1、IMASK=0）：这样 §3.7 的第三段就失去意义，而且 boot 余下的整段（`init_proc_and_boot`、`init_post_and_memory`、`smp_init`）里 PPI 一直可以触发，而 `clock_irq_handler` 要到 `boot_init_timer` 才注册——中间的任何一个 tick 都走进空链。落到对象：`CNTP_CTL_EL0` 的 bit 0、PPI 的 INTID 30、钩子池的空槽位。依据：`clock.rs` 的 [`fn init_timer`](os/arch/src/arm64/clock.rs:fn init_timer) 里那行被注释明确解释过的写值。

### 4.4 `InterruptRouter` 的三架构实现（→3.3、3.8）

控制器的 `init()` 在三架构上做三件不同的事，但都以"每条线屏蔽"结束。

x86-64 分两步：LAPIC 与 IOAPIC。

```rust
// os/plat/src/x86_64/interrupt.rs:fn init_lapic 与 :fn init_ioapic —— 代码引用（两个相邻方法的函数体，注释按原文）
    unsafe fn init_lapic(&mut self) { unsafe {
        let apic_base = wrmsr_msr_read(MSR_IA32_APIC_BASE);
        let apic_phys = apic_base & IA32_APIC_BASE_ADDR_MASK;
        if apic_phys as usize != self.lapic_base {
            self.lapic_base = apic_phys as usize;
        }
        wrmsr_msr_write(MSR_IA32_APIC_BASE, apic_base | IA32_APIC_BASE_EN);
        lapic_write(self.lapic_base, LAPIC_REG_SVR, LAPIC_SPURIOUS_VECTOR as u32 | LAPIC_SVR_ENABLE);
        // Mask every LVT entry (C apic.c init parity: all LVTs masked until
        // their owner opens them — the LAPIC timer gate is S-3c/D-59
        // territory; the LINT lines must not inherit firmware state). OVMF
        // leaves LINT0 in ExtINT mode routing the 8259 PIC into the LAPIC,
        // so an unmasked LINT0 delivers leftover PIC vectors after `sti`.
        for lvt in [LAPIC_REG_LVT_TIMER, LAPIC_REG_LVT_LINT0, LAPIC_REG_LVT_LINT1, LAPIC_REG_LVT_ERROR] {
            lapic_write(self.lapic_base, lvt, LAPIC_LVT_MASK);
        }
    }}

    unsafe fn init_ioapic(&mut self) { unsafe {
        for pin in 0..IOAPIC_NUM_PINS {
            self.ioapic_route_and_mask(pin);
        }
    }}
```

三件事：从 `IA32_APIC_BASE` MSR 取回 LAPIC 的实际基址（描述符里给的值与实际值不一致时以 MSR 为准）、置 MSR 的全局使能位（bit 11）并写 SVR（`0xFF | bit 8`）、把四个 LVT 项全部置上 mask 位。第三件事的注释给了一条只有读代码才能发现的理由：OVMF 会把 `LINT0` 留在 ExtINT 模式，`sti` 之后未屏蔽的 LINT0 会把固件留下的 PIC 向量送进来；那段注释里有两处仓库内部的阶段编号，按原文引用。

IOAPIC 侧逐引脚编程。这里有一个必须看代码才能发现的事实：

```rust
// os/plat/src/x86_64/interrupt.rs:fn isa_irq_to_pin —— 代码引用（文档注释与函数体）
    /// ISA IRQ number → IOAPIC input pin — the classic ISA override present
    /// on the pc machine and real chipsets alike: the PIT (ISA IRQ0) enters
    /// the IOAPIC at pin 2, and the slave-cascade (ISA IRQ2) at pin 0;
    /// every other line is identity. The kernel addresses lines by ISA IRQ
    /// number (C: CLOCK_IRQ=0; IrqVector), the IOAPIC RTEs by pin — every
    /// RTE access must go through this map or the timer line stays masked
    /// while RTE 0 is (wrongly) opened.
    const fn isa_irq_to_pin(irq: u8) -> u8 {
        match irq {
            0 => 2,
            2 => 0,
            other => other,
        }
    }
```

内核用"ISA IRQ 号"寻址线（`IrqVector`，`TIMER_IRQ = 0`），IOAPIC 用"输入引脚号"寻址重定向项；这两套编号在 0 与 2 上互换了。原因在 §1.2 的链路图里：PIT 的 ISA IRQ0 进入 IOAPIC 的 2 号输入，而级联线 ISA IRQ2 走 IOAPIC 的 0 号输入。所有 RTE 访问都必须过这个映射，否则会出现"解了引脚 0 的 mask，而定时器在引脚 2 上"这种错位。

aarch64 的 GICv3 初始化分三步，各自解决一类问题：

```rust
// os/plat/src/arm64/interrupt.rs:fn init_distributor / :fn init_redistributor / :fn init_cpu_interface —— 节选（省略号处为两个循环里的 `reg` 计算行、三段之间的注释块与 redistributor 的两段注释）
    fn init_distributor(&mut self) {
        unsafe {
            for irq in (32..self.nr_irqs).step_by(32) {
                ...
                self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
            }
            for irq in (32..self.nr_irqs).step_by(32) {
                ...
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
            ...
            let ctlr = self.gicd_read32(GICD_CTLR) | GICD_CTLR_ENABLE_GRP1NS;
            self.gicd_write32(GICD_CTLR, ctlr);
        }
    }

    fn init_redistributor(&mut self) {
        unsafe {
            self.gicr_write32(GICR_WAKER, 0);
            while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
                core::hint::spin_loop();
            }
            ...
            self.gicr_write32(GICR_ICENABLER0, 0xFFFF_FFFF);
            ...
            self.gicr_write32(GICR_IGROUPR0, 0xFFFF_FFFF);
        }
    }

    fn init_cpu_interface(&mut self) {
        unsafe {
            let mut sre: u64;
            core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
            sre |= 0x7;
            core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);
            core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);
            core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
        }
    }
```

三处细节各自解决一个问题：

- **Distributor 的 `GICD_CTLR` 用读改写**。注释说的是"整寄存器写会清掉固件配置的位（`ARE_NS`/`ARE_S`，bit 31:30 的亲和性路由），真机上具有破坏性"；所以先读、只置 `EnableGrp1NS`（`0x2`）、再写回。（按 GICv3 规范，`ARE_S`/`ARE_NS` 位于 `GICD_CTLR` 的 bit 4 与 bit 5——注释里的 31:30 与规范不符；此处按注释原样引用，差异随源码侧登记。）
- **Redistributor 的 PPI/SGI 半在另一个地址帧**。`GICR_SGI_FRAME = 0x1_0000`（`interrupt.rs` 的 [`const GICR_SGI_FRAME`](os/plat/src/arm64/interrupt.rs:const GICR_SGI_FRAME)）：Redistributor 的 RD 帧（0x0000–0xFFFF）放控制寄存器，SGI/PPI 帧（0x10000–0x1FFFF）复用 Distributor 风格的偏移。所有 `GICR_IGROUPR0`/`ISENABLER0`/`ICENABLER0` 访问都要加这个基址，否则写落在保留槽位、被 QEMU 忽略，使能静默失效。
- **PPI 的分组要单独写**。复位值 0 把 PPI 放进 Group 0，Group 0 以 FIQ 交付；而 CPU 接口使能的是 Group 1（`ICC_IGRPEN1_EL1`），于是"已经使能的定时器 PPI"在 IRQ 交付通路上听不到。`GICR_IGROUPR0 = 0xFFFF_FFFF` 是把 SGI/PPI 全部归到 Group 1 的动作。

riscv64 的 PLIC 初始化做三件事：

```rust
// os/plat/src/riscv64/interrupt.rs:fn init —— 代码引用（函数体）
    fn init(&mut self) {
        unsafe {
            for irq in 1..self.nr_irqs {
                self.plic_write32(PLIC_PRIORITY + irq * 4, 1);
            }
            for word in 0..(self.nr_irqs + 31) / 32 {
                self.plic_write32(PLIC_ENABLE + self.context * 0x80 + word * 4, 0);
            }
            self.plic_write32(Self::threshold_offset(self.context), 0);
        }
    }
```

把每条线的优先级写成 1（最低有效优先级）、enable 位图清零（全部屏蔽）、阈值写 0（接受所有优先级）。`mask_all` 与第二段重复一次，保持"`init` 结束时全部屏蔽"的契约显式。

三架构 `mask_all` 覆盖的"全部"口径并不相同，这是读对照表时最容易滑过去的地方：

| | "全部"指什么 | 实现位置 |
|---|---|---|
| x86-64 | `nr_irq_vectors` 条内核线号（描述符给，clamp 到 64），每条经 `isa_irq_to_pin` 落到某个 IOAPIC RTE | `interrupt.rs` 的 [`fn mask_all`](os/plat/src/x86_64/interrupt.rs:fn mask_all) |
| aarch64 | Distributor 的 SPI 半（从 32 起）**加上** Redistributor 的 SGI/PPI 半（`GICD_ICENABLER` + `GICR_ICENABLER0`） | `interrupt.rs` 的 [`fn mask_all`](os/plat/src/arm64/interrupt.rs:fn mask_all) |
| riscv64 | PLIC 的 enable 位图（按 context 划分的字） | `interrupt.rs` 的 [`fn mask_all`](os/plat/src/riscv64/interrupt.rs:fn mask_all) |

**三条推演（删掉 0↔2 交换；去掉 `GICR_SGI_FRAME` 偏移；删掉 `GICR_IGROUPR0` 那一行）**：

1. 删掉 `isa_irq_to_pin` 的 0↔2 交换，直接拿 IRQ 号当引脚号。`register_hook(TIMER_IRQ = 0)` 的"第一个处理者解屏蔽"会去解引脚 0 的 mask（那是级联线），而 PIT 真正所在的引脚 2 保持屏蔽；`clock_irq_handler` 一次都不会被调用。落到对象：IOAPIC RTE 2 的 mask 位、`ioapic_route_and_mask` 给引脚 0 编程的向量（它是 `0x50 + 2 = 0x52`）。依据：`interrupt.rs` 的 [`fn isa_irq_to_pin`](os/plat/src/x86_64/interrupt.rs:fn isa_irq_to_pin) 与 `:fn ioapic_route_and_mask` 的映射表。
2. aarch64 的 GICR 偏移去掉 `GICR_SGI_FRAME`。写落到 RD 帧的保留槽位，被实现忽略：timer PPI 的使能静默丢弃、`mask_all` 的 PPI 半也无效——于是"`init` 结束时全部屏蔽"这条不变量在 PPI 半上不成立，固件留下的 PPI 使能状态会留在原处。落到对象：`GICR_ISENABLER0` 与 `GICR_ICENABLER0` 的地址。依据：`interrupt.rs` 的 [`const GICR_SGI_FRAME`](os/plat/src/arm64/interrupt.rs:const GICR_SGI_FRAME) 的注释。
3. aarch64 删掉 `GICR_IGROUPR0` 那一行。复位值把 PPI 放进 Group 0，而 CPU 接口使能的是 Group 1；定时器 PPI 即使被 `IrqManager` 解了屏蔽也到不了 IRQ 交付通路（它以 FIQ 交付）。落到对象：`GICR_IGROUPR0`、`ICC_IGRPEN1_EL1`、每个中断的 Group 归属。依据：`interrupt.rs` 的 [`fn init_redistributor`](os/plat/src/arm64/interrupt.rs:fn init_redistributor) 的注释（它记着第一次真机载体运行时"定时器 PPI 保持沉默"的现象）。

### 4.5 `PerCpuInterruptUnit`：claim/complete 的三种回答（→3.3）

三个实现里，claim 与 complete 各自读写的寄存器不同，而"回执必须原样写回 claim 的身份"这条协议是同一个。x86 的实现最短：

```rust
// os/plat/src/x86_64/interrupt.rs:fn claim 与 :fn complete —— 代码引用（两个方法的函数体与注释；注释内含仓库内部阶段编号，按原文引用）
    fn claim(&mut self) -> Option<u32> {
        // The x86 APIC has no claim step: unlike GIC (read ICC_IAR1_EL1)
        // or PLIC (read claim), nothing must be read before the handler
        // runs, and there is no claim token to hand back — the IDT vector
        // already names the line. The LAPIC EOI register is the
        // *completion* only — writing it here (the pre-D-61
        // implementation) would send the EOI before the handler and
        // re-open the interrupt early.
        None
    }

    fn complete(&mut self, claimed: Option<u32>) {
        // `claimed` is ignored by design: the gate-driven flow has no
        // claim token to pair with, and the LAPIC EOI completes whatever
        // the gate delivered — it must be written even when claim
        // returned None.
        let _ = claimed;
        unsafe { self.lapic_eoi() };
    }
```

`claim` 返回 `None` 不是"没有中断"，而是"没有身份可写回"。`complete` 里的 `lapic_eoi()` 写的是 LAPIC 的 EOI 寄存器（偏移 `0x0B0`），它对"门交付过来的这次中断"整体生效——即使 claim 返回 `None` 也要写。aarch64 的实现把身份带过来：

```rust
// os/plat/src/arm64/interrupt.rs:fn claim 与 :fn complete —— 代码引用（函数体；claim 与 complete 各有一段两行 SAFETY 注释未收，共四行）
    fn claim(&mut self) -> Option<u32> {
        let iar: u64;
        unsafe {
            core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) iar);
        }
        let intid = (iar as u32) & 0x00FF_FFFF;
        // GIC spurious INTID (1023): nothing claimable — handling and
        // completion are both skipped (K8 pairing).
        (intid != 1023).then_some(intid)
    }

    fn complete(&mut self, claimed: Option<u32>) {
        let Some(intid) = claimed else {
            return; // spurious claim: nothing to complete
        };
        unsafe {
            core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) intid as u64);
        }
    }
```

`ICC_IAR1_EL1` 的读取本身就是应答动作，返回值低 24 位是 INTID；1023 是 GIC 的超时/伪中断编号，映射成 `None`。complete 只在 `Some` 时写 `ICC_EOIR1_EL1`。riscv64 与它同形，只是寄存器换成 PLIC 的 claim/complete 共用偏移：

```rust
// os/plat/src/riscv64/interrupt.rs:fn claim 与 :fn complete —— 节选（省略号处为两段 SAFETY 注释；后一段注释含仓库内部阶段编号）
    fn claim(&mut self) -> Option<u32> {
        ...
        unsafe {
            let id = self.plic_read32(Self::claim_offset(self.context));
            (id != 0).then_some(id)
        }
    }

    fn complete(&mut self, claimed: Option<u32>) {
        let Some(id) = claimed else {
            return; // nothing claimed: nothing to complete
        };
        ...
        unsafe {
            self.plic_write32(PLIC_COMPLETE + self.context * 0x1000, id);
        }
    }
```

三架构对照：

| | claim 读什么 | "没有可处理的中断"长什么样 | complete 写什么 |
|---|---|---|---|
| x86-64 | 无（返回 `None`） | 不适用（门交付就是有中断） | LAPIC EOI 寄存器 `0x0B0` |
| aarch64 | `ICC_IAR1_EL1`（低 24 位） | INTID 1023 | `ICC_EOIR1_EL1`，值 = claim 返回的 INTID |
| riscv64 | PLIC claim 寄存器（按 context 偏移） | ID 0 | 同一偏移写 complete，值 = claim 返回的 ID |

分发链本身（mask → 链 → unmask → complete 的次序与理由）在 `14-exception-interrupt` §2.4（IRQ hook 链）与 §4.3（Rust 对位）；本篇只用到这条协议的初始化侧结论：`claim` 与 `complete` 的配对必须在同一个实例上进行，所以控制器实例不能"用完即弃"（§3.4）。

**让 x86 的 `claim` 也写一次 LAPIC EOI 会怎样**（看起来与另外两个架构"对称"）：EOI 是完成动作，在中断处理函数运行之前发出等于把 LAPIC 的 in-service 位清掉：同向量的中断可以在处理函数返回之前再进来，而 `IrqManager` 的 active 位图机制正是为了防止这种重入（`irq_manager.rs` 的 [`fn dispatch_claimed`](os/kernel/src/irq_manager.rs:fn dispatch_claimed) 里先 `mask` 再走链）。落到对象：LAPIC EOI 寄存器偏移 `0x0B0`、LAPIC 的 ISR 位、`actids[irq_idx]` 的置位时刻。

### 4.6 `ArchInit` 的三架构实现（→3.5、3.6）

三个实现今天都很短，短得有信息量——它们显示了这个阶段还剩下什么。

x86-64 的 `new()` 只把 ACPI 表地址收进实例字段，`init()` 的函数体里没有任何语句，只剩注释：

```rust
// os/arch/src/x86_64/arch_init.rs:fn init —— 代码引用（函数体，含注释）
    fn init(&mut self) {
        // C: arch_init() — arch_system.c:246-288

        // 1. Per-CPU kernel stacks
        // C: k_stacks = &k_stacks_start
        // Already handled by linker script in Rust version

        // 2. ACPI table parsing
        // C: acpi_init()
        // QEMU virt boot does not depend on ACPI; physical machines need
        // RSDP search + table parsing. When `acpi_tables` is `Some`,
        // Phase 4 will parse the tables starting from that address.

        // APIC initialization (C: apic_single_cpu_init()) is now handled by
        // X86_64InterruptController::init() (called from
        // init_clock_and_interrupts before arch_init).
    }
```

三条注释各说明一件事：内核栈由链接脚本处理（C 的 `k_stacks = &k_stacks_start` 在 Rust 里是链接器符号）；ACPI 表解析尚未实现（QEMU virt 不依赖它，物理机需要另做，见 §5.4 的缺口表）；APIC 初始化已经归控制器（§3.3/§3.5 的边界判据）。

aarch64 的 `init()` 有两个动作：一个与 C 等价的 FPEN 写、一个由描述符门控的 PMU 初始化。

```rust
// os/arch/src/arm64/arch_init.rs:fn init —— 节选（省略号处依次为 CPACR_EL1 写入上方的注释块、PMU 分支前的两行注释与函数末尾板级初始化的注释块；`unsafe` 块内三条 `msr` 各自上方的两行注释也未收）
    fn init(&mut self) {
        ...
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            cpacr |= 0x0030_0000; // FPEN = 0b11
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }
        ...
        if self.pmu_cycle_counter {
            unsafe {
                let pmcr: u64 = 0x7; // E + C + P bits
                core::arch::asm!("msr pmcr_el0, {}", in(reg) pmcr);
                core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);
                core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
            }
        }
        ...
    }
```

被省略的那段注释解释了 `FPEN = 0b11` 这行写的缘由：自引导（直核）链下固件留下的 `CPACR_EL1.FPEN` 可能是 `0b00`，而预编译的 core 里存在 NEON 的 `memset` 残差，`kmain` 之后第一次 4 字节 `memset` 就会触发未定义指令类异常；UEFI 链上的同一份代码从未暴露过这个问题，是因为 AAVMF 固件把 FPEN 留成了 `0b11`。这行写入与固件的遗留值等价，UEFI 链上重复写是幂等的。

PMU 三个寄存器与 C 的三条 `MCR`（§2.5）是同一组动作：`PMCR_EL0 = 0x7`（E+C+P）、`PMCNTENSET_EL0 = 0x8000_0000`（使能 cycle counter）、`PMUSERENR_EL0 = 0x1`（允许 EL0 访问）。差别是门控：C 无条件执行，Rust 由 `ArchMiscDesc` 的 `pmu_cycle_counter` 决定，而当前所有平台描述符（含 QEMU virt）都是 `false`，因此这段在本阶段实际不执行。

riscv64 的 `init()` 是空的，理由写在注释里：

```rust
// os/arch/src/riscv64/arch_init.rs:fn init —— 节选（省略号处为第 2 项的注释块，它记述了 `sie = STIE | SSIE` 写入被移出的原因；注释内含仓库内部阶段编号，按原文引用）
    fn init(&mut self) {
        // 1. PMP (Physical Memory Protection): NO S-mode setup possible or
        // needed. PMP CSRs (pmpcfg*/pmpaddr*) are M-mode-only per the
        // privileged spec — an S-mode write takes an illegal instruction
        // (live, first timer-irq carrier run: scause=2,
        // stval=0x3b051073 = `csrw pmpaddr0`). Under OpenSBI the firmware
        // owns PMP and its Domain0 regions already delegate S/U (R,W,X)
        // over the whole address space, so the allow-all intent this
        // entry once tried to program is satisfied by the firmware.
        ...
    }
```

这里有一处与 C 形态最不同的地方：C 的 `arch_init` 有一件事可做（内存裁剪），而 riscv64 的 `init()` 什么都不做——原因是它能做的那些写入不属于 S 模式。注释里的现场读数（`scause=2`、`stval=0x3b051073` 是 `csrw pmpaddr0` 的编码）说明这不是"暂时没写"，而是"这个动作在 S 模式下没有合法形式"。

**把 aarch64 的 `pmu_cycle_counter` 门控去掉、无条件使能 PMU 会怎样**：这个门控的值来自平台描述符，`false` 的含义是"这台机器的描述里没有承诺这个能力"；无条件写 `PMUSERENR_EL0.EN = 1` 会把一个未在描述中承诺的能力开放给 EL0，而上面的时钟状态与异常处理都没有为"用户态读 cycle counter"准备任何约束。落到对象：`pmuserenr_el0` 的 bit 0、`ArchMiscDesc` 的该字段、描述符在三个平台上的取值。依据：`arch_init.rs` 的 [`fn init`](os/arch/src/arm64/arch_init.rs:fn init) 的门控与 `arch_init.rs` 的 [`trait ArchInit`](os/arch/src/arch/arch_init.rs:trait ArchInit) 的实例化说明。

### 4.7 `boot_init_timer`：编程、注册、放行（→3.7）

这是本篇标题里"让时间开始流动"实际发生的地方，也是全篇唯一碰定时器硬件的函数（§3.7）。三段之间夹着两处注释，把次序的理由写在调用点上：

```rust
// os/kernel/src/lib.rs:fn boot_init_timer —— 节选（省略号处依次为函数头文档注释、三段之间的解释性注释（含仓库内部阶段编号）、一个空的 mock 条件块，以及仅 x86_64 的 `pic_init` 尾调用）
pub fn boot_init_timer() {
    ...
    use minix_arch::{ClockArch, CurrentClockArch, CurrentTimerIrqGate, TimerIrqGate};
    use minix_types::Endpoint;
    use minix_platform::platform_desc;
    let pd = platform_desc();
    let mut clock_arch = CurrentClockArch::new(pd.timer());
    clock_arch.init_timer(crate::clock::DEFAULT_HZ, crate::clock::current_cpuid().raw());
    ...
    let clock_ep = Endpoint::from_generation_slot(0, crate::proc::proc_nr::CLOCK.0);
    // A1: boot context — no BKL, no IRQs yet; boot_unchecked accessor.
    unsafe { crate::irq_manager_boot_unchecked() }
        .register_hook(
            minix_plat::TIMER_IRQ,
            crate::clock::clock_irq_handler,
            clock_ep,
            minix_plat::IrqNotifyId(0),
            minix_plat::IrqPolicy::REENABLE,
        )
        .expect("register clock IRQ hook: no free slots in IRQ_MANAGER");
    // (c) — gates last, handler already registered (see the Step 6 header
    // for the per-architecture semantics of this call).
    <CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
    ...
}
```

三段各自写什么，按架构列出来：

| 段 | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| `init_timer`（编程） | 写 PIT 命令字节与 divisor 低/高字节（§4.3） | 写 `CNTP_CVAL_EL0` = 当前计数 + 间隔；`CNTP_CTL_EL0` = `0x2`（门保持关） | 发 SBI TIME ecall 写 `mtimecmp` 为 `mtime + 间隔`；`sie.STIE` 不动 |
| `register_hook`（注册 + 控制器侧放行） | 挂 `TIMER_IRQ = 0` 的钩子；first-handler 规则解 IOAPIC 输入 2（ISA IRQ0）的 mask | 挂 `TIMER_IRQ = 30` 的钩子；解 `GICR_ISENABLER0` 的 bit 30 | 挂 `TIMER_IRQ = 0` 的钩子；PLIC 对 0 提前返回，无硬件动作 |
| `enable_timer_irq`（模块本地门） | 空操作（PIT 无本地门） | `CNTP_CTL_EL0 = 1`（Enable=1、IMASK=0）+ `isb` | `csrs sie, 0x20`（STIE = bit 5） |
| `pic_init`（x86 附加段，接在函数末尾） | 8259A 初始化与 IMR 放行（向量基 0x50/0x70、只放行 IRQ0 与级联线） | — | — |

x86-64 端到端因此是"三段 + `pic_init`"四件事：控制器侧的门（IOAPIC 引脚 2）由第 2 段解、8259A 的 IMR 由 `pic_init` 放行、本地门不存在（PIT 无本地门）——三层门的口径在这里收拢为"控制器侧的门 / 模块本地的门 / CPU 的中断允许位"。

注册传的五个参数对应 C `put_irq_handler` 所需的全部信息：向量（`minix_plat::TIMER_IRQ`）、处理函数（`crate::clock::clock_irq_handler`）、拥有者端点（CLOCK 任务）、通知位（`IrqNotifyId(0)`）与策略（`REENABLE`）。`REENABLE` 的含义在 15 篇展开；在本篇它只决定"这个处理函数返回时是否算完成"，而 `clock_irq_handler` 的实现按这条策略返回 `Completed`（`clock.rs` 的 [`fn clock_irq_handler`](os/kernel/src/clock.rs:fn clock_irq_handler) 的末段）。

x86 多一段，接在函数末尾：

```rust
// os/kernel/src/lib.rs:fn boot_init_timer —— 节选（x86_64 条件编译块与它的注释，注释有删节）
    // x86_64 timer bring-up 缺口修复（C i8259.c intr_init 对位）：8259A PIC
    // 此前从未初始化——固件遗留的向量基/mask 使 PIT 的 IRQ0 永远到不了
    // 0x50 门（s14n：150s 全程 tick 臂 <1000 次调用）。重映射 master→0x50 /
    // slave→0x70 并只放行 IRQ0+级联线。
    ...
    #[cfg(target_arch = "x86_64")]
    minix_plat::pic_init();
```

这段注释记着一个只有查代码才能理解的分工：x86-64 的启动路径上，PIT 的中断实际经"8259A → IOAPIC 输入 2 → LAPIC"这条链（§1.2），而 8259A 是固件留下的状态；`X86_64InterruptController::init()` 只管 LAPIC 与 IOAPIC，不碰 8259A。所以 8259A 的向量基重映射与 IMR 设置（`minix_plat::pic_init`，`interrupt.rs` 的 [`fn pic_init`](os/plat/src/x86_64/interrupt.rs:fn pic_init)）被放在同一个函数里：

```rust
// os/plat/src/x86_64/interrupt.rs:fn pic_init —— 代码引用（函数体；注释按原文）
pub fn pic_init() {
    unsafe {
        outb(PIC1_CMD, ICW1_INIT);
        io_wait();
        outb(PIC2_CMD, ICW1_INIT);
        io_wait();
        outb(PIC1_DATA, IRQ0_VECTOR); // master ICW2：向量基 0x50
        io_wait();
        outb(PIC2_DATA, IRQ0_VECTOR + 8); // slave ICW2：向量基 0x70
        io_wait();
        outb(PIC1_DATA, 0x04); // master ICW3：IRQ2 接 slave
        io_wait();
        outb(PIC2_DATA, 0x02); // slave ICW3：级联身份 = 2
        io_wait();
        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();
        // IMR：全 mask（IRQ0/IRQ2 由 unmask_irq 按需打开）。
        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }
    unmask_irq(0); // PIT
    unmask_irq(2); // 级联线
}
```

把这段与 §2.3 的十次 `outb` 并列，能看到同一个动作的两种实现：C 的 ICW1/ICW2/ICW3/ICW4/OCW1 序列在 Rust 里重写了一遍，端口用 `PIC1_CMD = 0x20`、`PIC1_DATA = 0x21`、`PIC2_CMD = 0xA0`、`PIC2_DATA = 0xA1`（与 C 的四个宏同值），ICW3 的两个值也是 `0x04` 与 `0x02`（与 §2.3 第二条结论相同的不对称）；ICW4 则相反：C 主片写 `ICW4_AT_MASTER = 0x05`、从片写 `ICW4_AT_SLAVE = 0x01`（§2.3 第三条结论），而 Rust 两片同写 `ICW4_8086 = 0x01`——C 主片那一位（bit 2）的差异在 Rust 侧不存在，此处按现状登记。最后两张屏蔽字都是 `0xFF`（全屏蔽），再由 `unmask_irq(0)` 与 `unmask_irq(2)` 打开 PIT 与级联线——与 C 的 `0xFB`（除级联外全屏蔽）等价：C 留级联线开着是因为它随后要靠 `put_irq_handler` 打开时钟线；Rust 的 `pic_init` 把两条都打开，因为时钟线的打开本来就要发生。

`boot_init_timer` 在启动路径上只被调用一次，位置在 `bsp_finish_booting` 里标注为 Step 6 的那一段（`lib.rs` 的 [`fn bsp_finish_booting`](os/kernel/src/lib.rs:fn bsp_finish_booting)，即 C `boot_cpu_init_timer` 的位置；它之后还有 `fpu_init`、`kernel_may_alloc = 0` 与 `switch_to_user` 三步）。`init_clock_and_interrupts` 不碰定时器——三段序列只有一个维护点，与 C 把三段收进 `boot_cpu_init_timer` 一处（§3.7）对位。

这次调用之所以安全（三段门都开着而系统不受干扰），是因为**此刻 CPU 还没有开始取中断**：BSP 的启动路径上没有 `sti`，中断使能位在切到用户态的恢复路径上才被置起（`trap_return.rs` 的 [`const IF_MASK`](os/arch/src/x86_64/trap_return.rs:const IF_MASK)）。控制器与定时器模块两级门开着，但 CPU 侧的中断允许位还关着——这正是两级门与 CPU 使能位三者可以分开操作的原因：放行一条线不等于 CPU 会立刻响应它。`test-timer-irq` 载体同样按 kmain 的顺序走一遍，只是它把 `boot_init_timer` 放在进程表初始化之后、自己再显式 `sti`（§5.2）。

**两条推演（把 `register_hook` 与放行的次序对调；删掉 x86 的 `pic_init` 调用）**：

1. 把 `register_hook` 换成"先 `enable_timer_irq` 再注册"——这是 §3.7 推演 3 在 `boot_init_timer` 内的落地位置，机制与后果同那一处（复位值 0 的比较器使门一开就持续挂起），此处不重复展开。
2. 把 x86 的 `pic_init()` 调用删掉、只留前两段。IOAPIC 输入 2 的 mask 已被 `register_hook` 解开，但 8259A 还停在固件留下的向量基与屏蔽字上：PIT 的输出进不了主片 IRQ0（屏蔽字里它是关的），或者即使进来，主片的 INT 也到不了 IOAPIC（取决于固件的屏蔽状态）；`clock_irq_handler` 一次都不会被调用，`uptime` 停在 0。落到对象：8259A 主片的 IMR bit 0、主片 ICW2 的向量基（固件值 vs `0x50`）、IOAPIC RTE 2 的 mask 位。

#### 4.7.1 aarch64 真硬件路径与职责边界

aarch64 的定时器中断要经过两个独立的门，各自的所有者不同：

```text
CNTP 模块（per-CPU） ──► PPI INTID 30 ──► GIC Redistributor ──► CPU 接口 ──► CPU
     ▲                        ▲                    ▲                 ▲
CNTP_CTL_EL0.Enable      GICR_ICENABLER0/      GICR_WAKER /       ICC_SRE_EL1 /
（模块本地门）            ISENABLER0 bit 30     GICR_IGROUPR0      ICC_PMR_EL1 /
  所有者：TimerIrqGate   （控制器侧的门）       所有者：控制器      ICC_IGRPEN1_EL1
                          所有者：IrqManager                       所有者：控制器
```

第一级门是定时器模块自己的 `CNTP_CTL_EL0.Enable`，第二级是 Redistributor 上的 PPI 使能位，后面还有 Redistributor 唤醒与 CPU 接口的三处寄存器。归属因此是这样切的：

- `AArch64InterruptController::init` 负责分发器全局使能、Redistributor 唤醒与 PPI/SGI 分组、CPU 接口三寄存器（`interrupt.rs` 的 [`fn init_cpu_interface`](os/plat/src/arm64/interrupt.rs:fn init_cpu_interface)）；
- `IrqManager::register_hook` 负责 PPI 使能位（由 first-handler 规则调 `unmask`）；
- `TimerIrqGate` 只写 `CNTP_CTL_EL0`，不碰 `ICC_IGRPEN1_EL1`。

四级门里，前两级就是正文说的"模块本地门"与"控制器侧的门"；后两级（Redistributor 唤醒与 CPU 接口）属于控制器初始化的产物（`init_cpu_interface`），不在放行序列里。

这条边界的判据是"谁写这个寄存器，在别处还会不会再写"。`ICC_IGRPEN1_EL1` 的写者只有控制器初始化一处；如果让门也写它，同一条使能位就有了两个所有者，将来控制器需要重新配置（AP 上线、GIC 复位）时，门的写入会掩盖控制器的意图。

**让 `TimerIrqGate` 顺手写 `ICC_IGRPEN1_EL1` 会怎样**：两个所有者写同一位，写入的先后决定最终值：AP 拉起时若控制器先关全局使能、门后写 1，关门就失败了。落到对象：`ICC_IGRPEN1_EL1` 的写者集合、AP 拉起的顺序。依据：[`timer_irq_gate.rs`](os/arch/src/arm64/timer_irq_gate.rs) 的模块注释（它明确列出 `ICC_IGRPEN1_EL1` 属于控制器初始化而不是门）与 `interrupt.rs` 的 [`fn init_cpu_interface`](os/plat/src/arm64/interrupt.rs:fn init_cpu_interface) 的写。

#### 4.7.2 handler 注册不在此 trait

`TimerIrqGate` 只有两个方法，没有 `register_handler`。这个缺省是一个决策：曾经存在过一个 `ArchBoot::register_timer_handler`，它的实现是空的占位，没有读者被删掉（[`timer_irq_gate.rs`](os/arch/src/arch/timer_irq_gate.rs) 的模块注释记着这段）。

判据是"差异出现之前不预建抽象"：handler 注册的硬件形态在三架构上并不统一——PIT 路径不需要任何本地注册（挂链即放行），aarch64 的 CNTP 也没有独立的注册寄存器，riscv64 更没有；三者真正共有的动作只有一件，"把处理函数挂到 `IrqManager` 的钩子池里"，而这件动作已经有唯一的入口 `irq_manager.rs` 的 [`fn register_hook`](os/kernel/src/irq_manager.rs:fn register_hook)。把它再包一层到 `TimerIrqGate` 里，得到的只是一组互相转发的空实现。

**给 `TimerIrqGate` 加一个 `register_handler(handler)` 会怎样**：三个实现里只有一个能做真事（转发到 `IrqManager`），另两个是空操作；调用点会面临"该调 trait 还是直接调 `IrqManager`"的二选一，而正确答案永远是后者。落到对象：trait 的方法集合、三份实现文件里会多出的空方法。依据：[`timer_irq_gate.rs`](os/arch/src/arch/timer_irq_gate.rs) 的模块注释（`Timer handler *registration* is intentionally NOT part of this trait`）。

### 4.8 `IrqManager::register_hook` 的注册路径（→3.3、3.4、3.7）

`boot_init_timer` 的第二段调用的就是这个函数。它与 C 的 `put_irq_handler` 是同一段逻辑的两种写法：

```rust
// os/kernel/src/irq_manager.rs:fn register_hook_at_slot —— 节选（省略号处为该函数开头的向量合法性检查、空槽检查与 id 分配循环）
        ...
        // C: interrupt.c:65 — `(irq_actids[irq] &= ~hook->id) == 0`.
        // Clear this hook's active bit (no-op for a brand-new hook, but
        // faithful to C and safe if a slot is ever reused), then unmask
        // the IRQ only if NO handler on this vector is still active.
        self.actids[irq_idx] &= !id;
        if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        Ok(IrqId(id))
    }
```

注册路径的完整步骤：先做向量范围检查（越界 panic），再让调用者拿到一个空闲槽（`register_hook` 用 `find_free_slot` 找），把处理函数、拥有的端点、通知位与策略写进槽位；然后在**该向量的既有链**上求一个最低空闲位作为这个钩子的 id（`bitmap` 逐位或、从 1 开始左移找第一个空位）；接着把槽位追加到该向量的链尾、置 `irq_use` 的对应位；最后清本钩子的 active 位，若该向量已无任何 active 钩子就 `unmask`。

C 的对应段落（§2.6 已引用过末尾几行）逐项对得上：

| 步骤 | C（`kernel/interrupt.c` 的 [`put_irq_handler`](minix3/minix/kernel/interrupt.c:put_irq_handler)） | Rust（`irq_manager.rs` 的 [`fn register_hook_at_slot`](os/kernel/src/irq_manager.rs:fn register_hook_at_slot)） |
|---|---|---|
| 合法性检查 | `if( irq < 0 \|\| irq >= NR_IRQ_VECTORS ) panic(...)` | `if irq_idx >= NR_IRQ_VECTORS { panic!(...) }` |
| 求最低空闲 id | `for (id = 1; id != 0; id <<= 1) if (!(bitmap & id)) break;` | `let mut id = 1u32; while id != 0 && (bitmap & id) != 0 { id <<= 1; }` |
| 挂链 | `hook->next = NULL; … *line = hook;` | `append_to_chain(irq_idx, slot_idx)` |
| 放行条件 | `if((irq_actids[hook->irq] &= ~hook->id) == 0) { hw_intr_used(irq); hw_intr_unmask(hook->irq); }` | `self.actids[irq_idx] &= !id; if self.actids[irq_idx] == 0 { self.controller.unmask(irq); }` |

`id` 的取值形态两边也一致：第一个注册者拿到 1，第二个拿到 2，依次左移；`irq_actids[]` 里存的是这些位的或。这条规则的作用是支持共享线：同一条线上多个设备各自注册钩子，只有全部"完成"之后线才重新打开。

**把 `unmask` 从注册路径里整体删掉会怎样**：所有线会永远停在 `init()` 留下的屏蔽态；`boot_init_timer` 第三段打开的是模块本地门，而控制器侧的门没人解——两级门只开了一级，tick 一个也到不了。落到对象：`actids[irq_idx]` 的清零动作、`controller.unmask(TIMER_IRQ)` 的调用点、IOAPIC 输入 2 的 mask 位（x86）/`GICR_ISENABLER0` 的 bit 30（aarch64）。

### 4.9 三架构 `TIMER_IRQ` 的取值与硬件理由（→3.8）

`TIMER_IRQ` 是"启动时钟源的中断在内核侧用哪个编号寻址"这一个量。三个值都是本实现的选择，但各自的理由来自硬件：

```rust
// os/plat/src/lib.rs:const TIMER_IRQ —— 节选（三条 cfg 转发；各架构常量定义在各自文件，见下表）
#[cfg(target_arch = "x86_64")]
pub const TIMER_IRQ: IrqVector = crate::x86_64::interrupt::TIMER_IRQ;
#[cfg(target_arch = "aarch64")]
pub const TIMER_IRQ: IrqVector = crate::arm64::interrupt::TIMER_IRQ;
#[cfg(target_arch = "riscv64")]
pub const TIMER_IRQ: IrqVector = crate::riscv64::interrupt::TIMER_IRQ;
```

| 架构 | 值 | 定义处 | 硬件理由 |
|---|---|---|---|
| x86-64 | 0 | `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/x86_64/interrupt.rs:const TIMER_IRQ) | PIT 是 ISA IRQ0（C 的 `CLOCK_IRQ` 也是 0）；实际投递经 IOAPIC 输入 2（§4.4 的 0↔2 映射） |
| aarch64 | 30 | `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/arm64/interrupt.rs:const TIMER_IRQ) | EL1 非安全物理定时器（CNTP）是 PPI INTID 30（GIC 的 PPI 分配 CNTPNSIRQ）；30 < 32，所以它走 Redistributor 而不是 Distributor |
| riscv64 | 0 | `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/riscv64/interrupt.rs:const TIMER_IRQ) | 保留伪向量：S 模式定时器是核本地中断，不经 PLIC，0 只作分发身份 |

三处来源各自写明"改这个值要同时改什么"：aarch64 的 30 与"用哪一族定时器寄存器（`CNTP_*` 而不是 `CNTV_*`/`CNTPS_*`）"绑定，且它小于 32 这一事实决定了控制器侧的放行必须走 `GICR_ISENABLER0`（写在 `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/arm64/interrupt.rs:const TIMER_IRQ) 的文档注释里）；riscv64 的 0 与"陷阱入口把 `scause == SupervisorTimer` 映射到哪个编号、PLIC 的 mask/unmask 把 0 当无控制器线"绑定（写在 `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/riscv64/interrupt.rs:const TIMER_IRQ) 的文档注释里）；x86 的这一条不在常量的文档注释里，而在测试注释 `test_timer_irq_is_ioapic_input_zero`（[`interrupt.rs`](os/plat/src/x86_64/interrupt.rs)）里：时钟源若换成 LAPIC 定时器，这个值与 `TimerIrqGate` 的 x86 语义要一起改。

x86 侧的"输入 0"表述仍有残留载体：`const TIMER_IRQ` 的文档注释与 [`lib.rs`](os/plat/src/lib.rs) 的 x86_64 条目都已按操作代码写成"经 `isa_irq_to_pin` 落引脚 2"，但测试名 `test_timer_irq_is_ioapic_input_zero`（[`interrupt.rs`](os/plat/src/x86_64/interrupt.rs)）与 [`timer_irq_gate.rs`](os/arch/src/x86_64/timer_irq_gate.rs) 的模块注释仍写着"IOAPIC 输入 0"。本篇按操作代码陈述（PIT 经 8259A 的 INT 输出进 IOAPIC 输入 2，见 §1.2 的链路图与 §4.4）——名字与注释的更新是代码侧的事。

riscv64 的 PLIC 驱动把 0 当"没有控制器线"处理：

```rust
// os/plat/src/riscv64/interrupt.rs:fn mask 与 :fn unmask —— 节选（两处开头三行）
    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 {
            return;
        }
    ...
    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        if irq_num == 0 {
            return;
        }
```

这一段与 §1.3 的推演合成一条结论：riscv64 的定时器在 PLIC 上没有任何位可以被改，所以 `IrqManager` 在"第一个处理者注册"时对它的 `unmask` 是一次约定内的空操作；投递门完全由 `TimerIrqGate` 承担。

**把 riscv64 的 `TIMER_IRQ` 改成 1 会怎样**：PLIC 的 enable 位图 bit 1 会被"第一个处理者"规则打开（`mask`/`unmask` 对非 0 值都会做读改写）；一旦那条线上的设备中断经陷阱入口进入分发，它以 `IrqVector(1)` 落到链上挂着的 `clock_irq_handler`——一次设备中断被记成一次时钟 tick，`uptime` 被虚假推进。落到对象：PLIC 的 enable 位图 bit 1、`handlers[1]` 的链、`clock_irq_handler` 的调用来源。

### 4.10 全局存储与访问契约（→3.4）

本篇有四个全局在本阶段被写入，它们的写入点决定了"谁在什么时候可以看到什么"：

| 全局 | 写入点 | 启动期访问器 | 运行期访问器 |
|---|---|---|---|
| `PLATFORM` | `kmain` Phase A.5 的 `minix_platform::init_from_kinfo`（04 篇） | `platform_desc()` | 同左（只读） |
| `IRQ_MANAGER` | `init_clock_and_interrupts` Step 2 的 `intr.init()` 之后 | `lib.rs` 的 [`fn irq_manager_boot_unchecked`](os/kernel/src/lib.rs:fn irq_manager_boot_unchecked) | `lib.rs` 的 [`fn irq_manager_with`](os/kernel/src/lib.rs:fn irq_manager_with) |
| `CLOCK_STATE` | `init_clock_and_interrupts` 的末尾 | `lib.rs` 的 [`fn clock_state_boot_unchecked`](os/kernel/src/lib.rs:fn clock_state_boot_unchecked) | `lib.rs` 的 [`fn clock_state_with`](os/kernel/src/lib.rs:fn clock_state_with) |
| `SMP_STATE` | `kmain` 的 `init_smp_state`（调用点在 Phase F 段，函数注释自称 Phase D） | `lib.rs` 的 [`fn try_smp_state`](os/kernel/src/lib.rs:fn try_smp_state) 与 `:fn smp_state_boot_unchecked` | `lib.rs` 的 [`fn smp_state_with`](os/kernel/src/lib.rs:fn smp_state_with) |

两套访问器的分界不是"调用者是谁"，而是"BKL 是否已经存在"：`boot_unchecked` 的契约是"单线程初始化期、BKL 还不存在"，`with` 系列接收一个 `BklSection` 见证、证明锁已被持有。触发这条区分的现实原因是时钟处理函数：它在中断上下文里跑，而中断可能在任意时刻到达，因此必须走带见证的路径进入全局。

缺少初始化时，两套访问器的报错信息都指向初始化函数：

```text
IRQ_MANAGER not initialized — init_clock_and_interrupts must run first
CLOCK_STATE not initialized — init_clock_and_interrupts must run first
```

**删掉 `init_clock_and_interrupts` 末尾那次 `CLOCK_STATE` 写入会怎样**：编译与启动都不会报错（写入点只是少了一句），第一次 tick 到达时 `clock_irq_handler` 走到 `clock_state_with`，在中断上下文里命中上面第二条 `expect` 并 panic——故障现场在中断里，而不是在启动期，排查时需要从 panic 信息反推回初始化函数。落到对象：`CLOCK_STATE` 的写入点、`clock.rs` 的 [`fn clock_irq_handler`](os/kernel/src/clock.rs:fn clock_irq_handler) 的第一个全局读取。

---

## 5. 测试要点

### 5.1 宿主测试：读数与口径

本篇相关的宿主测试分布在三个 crate。读数的口径先写清楚：下表第一列是**文件里 `#[test]` 属性的条数**（复算命令用 `grep -c`），第二列是**在 x86_64 宿主机上实际编入并运行的测试数**（用 `cargo test --list` 数），两列的差全部来自 `cfg(target_arch = ...)` 门控。

| 文件 | 源码里 `#[test]` 条数 | x86_64 宿主实跑 | 说明 |
|---|---|---|---|
| [`clock.rs`](os/kernel/src/clock.rs) | 47 | 47 | 软件时钟状态、定时器链、负载、quantum 递减；其中 2 条在 `clock_irq_handler_tests` 模块里 |
| [`irq_manager.rs`](os/kernel/src/irq_manager.rs) | 9 | 9 | 注册/移除/分发、共享线、策略与通知 |
| [`clock.rs`](os/arch/src/arch/clock.rs) | 3 | 3 | `DEFAULT_HZ`、`LOAD_HISTORY_SIZE`、`read_tsc` 默认委托 |
| [`timer_irq_gate.rs`](os/arch/src/arch/timer_irq_gate.rs) | 4 | 2 | 三条按 `target_arch` 门控的编译检查里只有本架构那条参与编译 |
| [`interrupt.rs`](os/plat/src/interrupt.rs) | 0 | 0 | 原 10 条包装器/派生的自指测试已删，模块注释写明"本模块目前无行为可单元测试" |
| [`interrupt.rs`](os/plat/src/x86_64/interrupt.rs) | 4 | 4 | 描述符构造、线数钳制、`TIMER_IRQ` 与 `PROFILE_CLOCK_IRQ` 的取值钉 |
| [`interrupt.rs`](os/plat/src/arm64/interrupt.rs) | 3 | 0 | 全部是 aarch64 门控 |
| [`interrupt.rs`](os/plat/src/riscv64/interrupt.rs) | 4 | 0 | 全部是 riscv64 门控 |
| [`arch_init.rs`](os/arch/src/x86_64/arch_init.rs) | 3 | 3 | ACPI 表地址的存取与 `init` 的可重复性 |
| [`arch_init.rs`](os/arch/src/arm64/arch_init.rs) | 0 | 0 | 无 |
| [`arch_init.rs`](os/arch/src/riscv64/arch_init.rs) | 0 | 0 | 无 |

复算命令：

```bash
cd os
# 第一口径：源码里的 #[test] 条数（逐文件）
for f in kernel/src/clock.rs kernel/src/irq_manager.rs arch/src/arch/clock.rs \
         arch/src/arch/timer_irq_gate.rs plat/src/interrupt.rs \
         plat/src/x86_64/interrupt.rs plat/src/arm64/interrupt.rs plat/src/riscv64/interrupt.rs \
         arch/src/x86_64/arch_init.rs arch/src/arm64/arch_init.rs arch/src/riscv64/arch_init.rs; do
  printf "%s: " "$f"; grep -c '#\[test\]' "$f"
done
# 第二口径：宿主实跑（--list 只编译不运行；两条特性旗标是形态要求：
#   minix-kernel 裸跑会被 form_contract 编译期绊线挡住，minix-arch 的测试在 std 语境编写）
cargo test -p minix-kernel --features mock --lib -- --list | grep -c '^clock'
cargo test -p minix-kernel --features mock --lib -- --list | grep -c '^irq_manager'
cargo test -p minix-arch --features runtime-window --lib -- --list | grep -c '^arch::clock'
cargo test -p minix-plat   --lib -- --list | grep -c '^x86_64::interrupt'
```

这套测试覆盖的行为维度：`ClockState` 的构造与 `hz` 边界（含越界回退）、BSP 与 AP 的区分（`test_ap_state_set_alarm_timer_panics` 断言 AP 实例上的 `set_alarm_timer` 会 panic，`test_alarm_reset_is_idempotent` 覆盖定时器摘除的幂等）、tick 的记账（uptime/realtime 与 adjtime 的正负偏差）、负载槽位轮转、定时器链的插入/移除/到期顺序、以及 `IrqManager` 的注册-放行语义（`register_and_dispatch`、`dispatch_claims_before_mask_and_completes_last` 断言的调用次序是 `unmask → claim → mask → unmask → complete`）。

[`interrupt.rs`](os/plat/src/interrupt.rs) 的 0 条不等于"没有覆盖"：真实行为覆盖在各架构文件的构造测试与 `IrqManager` 的 9 条里，那个文件的模块注释也写明了这一点。

### 5.2 真机（QEMU）载体：`test-timer-irq`

宿主测试不能证明任何寄存器写入真的发生了。证明这件事的载体是 `test-timer-irq`，三个架构各一份（`os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq{,-aarch64,-riscv64}/src/main.rs`），入口登记在 `os/qemu-tests/run_all.sh` 的三条架构清单里。

载体的装配顺序就是 kmain 的顺序，每一段之间打印一行进度，便于定位卡在哪一步：

```rust
// os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq/src/main.rs —— 节选（省略号处为各段之间的空行与少量语句）
    // 5. Clock + interrupt controller (kmain Phase B order) — PIT IRQ line
    // stays masked here; the controller mask_all is the D-59 boot contract.
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized (all lines masked)\n");

    // 6. Process table (Phase C) + SMP state (Phase D) — clock_irq_handler
    // reads both for accounting.
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    ...
    // 7. boot_cpu_init_timer parity: program PIT, register clock hook
    // (first-handler rule unmasks the IOAPIC line), open gates.
    minix_kernel::boot_init_timer();
    early_console::write_str("  PIT programmed, clock hook registered, gate open\n");

    // 8. Hold the BKL ...
    let guard = minix_kernel::smp::bkl_lock();
    core::mem::forget(guard);
    let section = unsafe { minix_kernel::smp::BklSection::assume_held() };
    unsafe { asm!("sti", options(nomem, nostack)); }
    early_console::write_str("  IF=1 — polling uptime\n");

    let start = minix_kernel::clock::get_monotonic(&section);
    let target = start + PASS_TICKS;
    ...
```

块内有一处仓库内部的阶段编号，按原文引用。判定标准写在文件顶部：`PASS = uptime reaches 5 ticks with interrupts delivered through the S-8 stub path`，通过时串口出现 `### TEST_RESULT: PASS test-timer-irq ###`。失败没有一条统一的横幅：x86_64 载体里有一个打印 `### TEST_RESULT: FAIL test-timer-irq ###` 的 `fail()`，但它从未被调用——载体停住时是 panic 处理器打 `### PANIC in test-timer-irq: <文件>:<行号> ###`，或者什么都不打，此时由 `os/qemu-tests/run_qemu.sh` 兜底判定（它只认 `### TEST_RESULT: PASS` 标记，没有标记即判失败，报 `no PASS marker found`）。aarch64 与 riscv64 载体的源码里只有 PASS 一处，`### TEST_RESULT: FAIL test-timer-irq-aarch64`（`…-riscv64`）由外层脚本在超时后补打（`os/qemu-tests/test-timer-irq-aarch64.sh`、`os/qemu-tests/test-timer-irq-riscv64.sh`）。

运行方式（工件需先构建，构建配方见 `os/qemu-tests/README.md`）：

```bash
# 三架构全跑（载体在清单里）
bash os/qemu-tests/run_all.sh
# 单跑 x86_64 载体
os/qemu-tests/run_qemu.sh x86_64 os/target/x86_64-unknown-uefi/release/test-timer-irq.efi
```

预期关键行 `[待验证]`：开头一行 `### test_timer_irq (x86_64): S-8 L3 — PIT→IOAPIC→stub→IrqManager→uptime`，随后是装配进度（`paging enabled`、`protection loaded`、`IDT live with S-8 stubs (no empty gate)`、`clock + controller initialized (all lines masked)`、`proc table + smp state initialized`、`PIT programmed, clock hook registered, gate open`、`IF=1 — polling uptime`），再往后是逐 tick 的 `tick: uptime = 0x…`，最后 `### TEST_RESULT: PASS test-timer-irq ###`。这些行取自载体源码里的 `write_str` 调用与判定分支，本篇写作期未实跑；实际输出以串口为准。

这个载体证明的边界，文件顶部写得很直白：`the scheduler is NOT entered; BKL held throughout`，且"syscall 通路与用户帧的重调度出口虽然接线但在这里不可达——在调度器与用户态通路接线之前没有 CPL3 代码"。也就是说它证明的是"从 PIT 到 `uptime` 推进"这条链，不证明 tick 的六步语义，也不证明分发协议在用户态上下文里的行为。

### 5.3 机制 → 需要证明什么（反转核对表）

本节从**机制**出发列出"要证明它成立需要什么断言"，再标注覆盖状态——不是从现有测试出发（那份视角在 §5.1/§5.2 的表；本节是它的反转面）。反转核对查出 6 条覆盖缺口（R05-T01~T06），已按 TODO 登记。

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 覆盖状态 | 现有证据 |
|---|---|---|---|---|
| M1 | `ClockState` 构造与 hz 边界（§3.2、§4.2） | 越界回退、BSP/AP 区分、挂/摘守卫 | 已覆盖 | 宿主 `clock.rs` 47 条（含 `with_hz` 三例与 `test_ap_state_set_alarm_timer_panics`） |
| M2 | tick 记账与负载槽位（§4.2） | uptime/realtime/adjtime 推进、槽位轮转 | 已覆盖 | 宿主 `clock.rs`（记账与负载测试族） |
| M3 | 定时器链的插入/移除/到期（§4.2） | 到期顺序、摘除幂等 | 已覆盖 | 宿主 `clock.rs`（含 `test_alarm_reset_is_idempotent`） |
| M4 | `IrqManager` 注册-放行语义（§4.8） | 注册/移除/分发、共享线、`unmask→claim→mask→unmask→complete` 次序 | 已覆盖 | 宿主 `irq_manager.rs` 9 条 |
| M5 | 三架构 `TIMER_IRQ` 取值（§4.9） | 0/30/0 钉住、改值联动 | 已覆盖（x86 宿主；arm/riscv 门控 0 宿主） | `x86_64/interrupt.rs` 4 条 + 真机载体三架构 |
| M6 | `claim`/`complete` 配对（§4.5） | 身份原样回写、spurious 语义 | 部分覆盖 | `IrqManager` 层有两条（`dispatch_claims_before_mask_and_completes_last`、`not_completed_keeps_active`）；三架构实现本体（寄存器读写）无宿主测试——真机载体承担 |
| M7 | `init_clock` 纯软件（§2.2） | 函数体无端口/寄存器访问 | 已覆盖 | §6.2 命令 1（可重放 grep） |
| M8 | `intr_init` 十次写（§2.3） | 实参序列、全屏蔽结束态、**位级写入** | 部分覆盖 | §6.2 命令 2/3 静态核；**位级未证** → **R05-T01** |
| M9 | x86 APIC 建立与门表重建（§2.4） | LAPIC 使能、RTE 编程、IDT 重载 | 部分覆盖 | 载体隐含（tick 到达）；**RTE/门表位级未证** → **R05-T01** |
| M10 | x86-64 ACPI 表解析（§4.6） | RSDP 搜索 + MADT 解析 | **未覆盖**（实现只存地址不解析；QEMU virt 不依赖） | → **R05-T02**（物理机路径） |
| M11 | aarch64 PMU 使能（§4.6） | PMCR/PMCNTENSET/PMUSERENR 写 | **未覆盖**（描述符全 `false`，段不执行） | → **R05-T03**（需承诺 PMU 的机器） |
| M12 | riscv64 `mtimecmp` 直接 MMIO 写（§4.3） | MMIO 路径对照 SBI | **未覆盖**（实现走 SBI；MMIO 被 PMP 挡——刻意不走） | → **R05-T04**（M 模式参与的环境） |
| M13 | 定时器三段（编程/注册/放行）（§3.7、§4.7） | 三架构编程 + 注册 + 门打开 → tick 到达 | 已覆盖 | 真机载体 `test-timer-irq`（PASS = uptime 5 ticks） |
| M14 | x86 第四段 `pic_init`（§4.7） | 8259A ICW 重映射 + IMR 放行 | 部分覆盖 | 载体隐含；位级同 M8 → **R05-T01** |
| M15 | 注册即放行（C↔Rust 同位）（§4.8） | 首处理者解屏蔽、actids 清零 | 已覆盖 | 宿主 `irq_manager.rs` + 载体 |
| M16 | 时钟处理函数在用户态上下文的行为（§5.2 边界） | CPL3 进入 + 调度循环下 tick | **未覆盖**（载体不进调度器） | → **R05-T05**（后续文档的调度链） |
| M17 | 关中断窗口与中断风暴的真机表现（§3.7 推演 3） | 门先于源打开 → 风暴/停摆 | **未覆盖**（宿主无法注入） | → **R05-T06**（故障注入变体） |
| M18 | tick 六步语义（§1.4 范围外） | 推进/记账/定时器/alarm/负载 | 范围外 | 属 `15-clock-timer` |

**新 TODO（对源码/测试 workflow 的需求输入）**：`R05-T01`（位级真值：8259A ICW 序列、IOAPIC RTE、GICD/GICR、PLIC 位图的 GDB/MMIO 单步核对——覆盖 M8/M9/M14）｜`R05-T02`（x86-64 ACPI 表解析补实现或明确豁免）｜`R05-T03`（aarch64 PMU 使能在承诺该能力的平台上验证）｜`R05-T04`（riscv64 MMIO 路径在 M 模式环境验证或保持 SBI 唯一通道）｜`R05-T05`（时钟处理函数的用户态上下文行为——随调度链文档）｜`R05-T06`（关中断窗口/风暴的故障注入载体变体）。

### 5.4 缺口与诚实披露

宿主测试与真机载体都不能证明的部分，以及本篇写作期未做的事：

| 缺口 | 为什么现在不证 | 谁能证 |
|---|---|---|
| 寄存器写入的位级真值（8259A 的 ICW 序列、GICD/GICR 的每个偏移、PLIC 的位图布局） | 单元测试没有硬件；QEMU 载体只断言最终效果（tick 到达），不断言中间写入 | GDB 单步 + MMIO 读取（`-s -S` 起 QEMU，在各段之间读寄存器），或真机 |
| x86-64 的 ACPI 表解析 | `X86_64ArchInit::init` 目前只存地址、不解析（§4.6） | 物理机启动路径；MADT 的解析已在平台发现侧（[04-platform-discovery.md](04-platform-discovery.md)） |
| aarch64 的 PMU 使能 | 所有平台描述符的 `pmu_cycle_counter` 都是 `false`，这段代码当前不执行 | 需要一台在描述里承诺 PMU 的机器 |
| riscv64 的 `mtimecmp` 直接 MMIO 写 | 实现走 SBI ecall，MMIO 路径被 PMP 挡（§4.3） | OpenSBI 之外的环境，或 M 模式参与 |
| 时钟处理函数在用户态上下文里的行为 | 载体不进调度器（§5.2） | 需要 CPL3 与调度循环（后续文档） |
| 关中断窗口与中断风暴的真机表现 | 宿主无法注入中断 | QEMU 载体的故障注入变体 |

另外一处需要披露的是代码注释与实现的状态差：`clock.rs` 的 [`trait ClockArch`](os/arch/src/arch/clock.rs:trait ClockArch) 里 `init_timer` 的文档注释仍写着"在 `init_clock_and_interrupts()` 中首次调用；`bsp_finish_booting()` 会以幂等空操作的形式再调一次"，而现码里 `ClockArch::init_timer` 的唯一调用点在 `lib.rs` 的 [`fn boot_init_timer`](os/kernel/src/lib.rs:fn boot_init_timer) 内（`init_clock_and_interrupts` 不再碰定时器）。这段注释是按早期形态写的，读者按注释去找调用点会找不到；本篇按现码陈述，注释的更新是代码侧的事，不在文档范围内完成。同类的"注释与现码不符"还有三处，都不影响本篇的断言：[`timer_irq_gate.rs`](os/arch/src/riscv64/timer_irq_gate.rs) 的模块注释仍写"直接写 CLINT MMIO、不经 SBI"（现码走 SBI ecall）、`clock.rs` 的 [`fn stop_local_timer`](os/arch/src/x86_64/clock.rs:fn stop_local_timer) 的注释称 C 侧没有具名函数（实有 `lapic_stop_timer` 一类）、[`interrupt.rs`](os/plat/src/interrupt.rs) 的 trait 表把 PLIC 的 `mask_all` 写成 threshold/max 口径（现码是 enable 位图清零）。

本篇写作期未运行真机门（`run_all.sh` 需要完整的工件构建与 QEMU/GDB 环境）。上面 §5.2 的命令与预期输出关键行取自载体源码与 `run_all.sh` 的登记，实际读数需要跑一次才能确认；这类"我未实跑"的项已在文中逐处标明来源，而不是写成已验证的事实。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象（状态 / 位 / 线 / 地址 / 次序） | 确认动作 |
|---|---|---|
| `uptime` 恒为 0，载体停在 `IF=1 — polling uptime` | 控制器侧的门没开：`register_hook` 的 first-handler `unmask` 没执行，或引脚映射把 mask 解到了别的引脚 | 核 `irq_manager.rs` 的 [`fn register_hook_at_slot`](os/kernel/src/irq_manager.rs:fn register_hook_at_slot) 的 `unmask` 行；核 `interrupt.rs` 的 [`fn isa_irq_to_pin`](os/plat/src/x86_64/interrupt.rs:fn isa_irq_to_pin) 的 0↔2 映射 |
| x86-64 上 tick 一次都不来，且 8259A 从未初始化 | `pic_init` 没被调用（它在 `boot_init_timer` 的末尾），或固件留下 IMR 把 IRQ0 关着 | 核 `lib.rs` 的 [`fn boot_init_timer`](os/kernel/src/lib.rs:fn boot_init_timer) 的 x86_64 尾调用；用 `grep -n 'pic_init' os/kernel/src/lib.rs os/plat/src/lib.rs` 确认调用链 |
| tick 只来一次就停 | 定时器是一次性的（CNTP/CVAL 或 `mtimecmp`），每次 tick 的重挂路径没走到 | 核 `clock.rs` 的 [`fn rearm_next_tick`](os/arch/src/arm64/clock.rs:fn rearm_next_tick) 与 `clock.rs` 的 [`fn local_timer_eoi`](os/arch/src/riscv64/clock.rs:fn local_timer_eoi)；核 `clock.rs` 的 [`fn local_tick`](os/kernel/src/clock.rs:fn local_tick) 是否调了 `local_timer_eoi` |
| 中断风暴（`uptime` 飞涨或系统卡在中断里） | 门先于源打开：`CNTP_CVAL_EL0` / `mtimecmp` 还是复位值 0 时 Enable 已置位 | 核 `clock.rs` 的 [`fn init_timer`](os/arch/src/arm64/clock.rs:fn init_timer) 的写值（应为 `0x2`）与 `arch_init.rs` 的 [`fn init`](os/arch/src/riscv64/arch_init.rs:fn init) 里是否又出现了 `sie` 写入 |
| aarch64 上定时器 PPI 已使能但仍无 tick | PPI 被分到 Group 0（以 FIQ 交付）而 CPU 接口只开了 Group 1；或 GICR 的 SGI/PPI 帧偏移写错 | 核 `interrupt.rs` 的 [`fn init_redistributor`](os/plat/src/arm64/interrupt.rs:fn init_redistributor) 的 `GICR_IGROUPR0` 与 `GICR_ICENABLER0` 两行；核 `interrupt.rs` 的 [`const GICR_SGI_FRAME`](os/plat/src/arm64/interrupt.rs:const GICR_SGI_FRAME) 是否参与了所有 GICR 访问 |
| riscv64 上 PLIC 的使能位被改而定时器没反应 | 把定时器当成 PLIC 线上的一条普通线 | 核 `interrupt.rs` 的 [`const TIMER_IRQ`](os/plat/src/riscv64/interrupt.rs:const TIMER_IRQ) 的值与 `:fn mask`/`:fn unmask` 的 `irq_num == 0` 提前返回 |
| 启动期 panic 信息是 `CLOCK_STATE not initialized …` | 时钟状态入全局的那一步没执行，或某条路径在 `init_clock_and_interrupts` 完成前就进入了时钟处理函数 | 核 `lib.rs` 的 [`fn init_clock_and_interrupts`](os/kernel/src/lib.rs:fn init_clock_and_interrupts) 末尾的写入；核 `lib.rs` 的 [`fn clock_state_with`](os/kernel/src/lib.rs:fn clock_state_with) 的 `expect` |
| PIT divisor 断言 panic（`hz must be >= 19`） | `DEFAULT_HZ` 或调用点传的 hz 小于 19 | 核 `clock.rs` 的 [`fn init_timer`](os/arch/src/x86_64/clock.rs:fn init_timer) 的断言与 `lib.rs` 的 [`fn boot_init_timer`](os/kernel/src/lib.rs:fn boot_init_timer) 传的实参 |
| 中断落到了 `#DE`（向量 0）而不是 0x50 | 被解屏蔽的那条线对应的 RTE 向量未编程（复位值 0，S-8 注释记着这条）；或 8259A 仍停在固件状态、向量基没被重映射 | 核 `interrupt.rs` 的 [`fn ioapic_route_and_mask`](os/plat/src/x86_64/interrupt.rs:fn ioapic_route_and_mask) 的向量算式（引脚 2 = 0x50）与 `:fn pic_init` 的两处 ICW2 写 |

### 6.2 可复制命令与预期输出

第一组：C 侧的事实与常量（在仓库根目录运行；每条给出预期关键行与口径）。

```bash
# 1) init_clock 是纯软件：函数体内没有任何端口/寄存器访问
sed -n '/^init_clock/,/^}/p' minix3/minix/kernel/clock.c
# 预期：函数体只有 memset / env_get / atoi / 范围判断四类语句，无 outb/read/write 调用。

# 2) outb 的口径：实际执行十条，源码里 12 行
awk '/^int intr_init/,/^}/' minix3/minix/kernel/arch/i386/i8259.c | grep -c 'outb('
# 预期：12。口径：函数体内 outb( 出现的行数；两处 if (auto_eoi) 各带一行不执行的替代臂，
#       所以执行路径是 10 条写、源码是 12 行。若只想要执行数，减去两条替代臂（见第 3 条输出的位置）。

# 3) 十次写的实参（把符号换成实数的复核）
awk '/^int intr_init/,/^}/' minix3/minix/kernel/arch/i386/i8259.c | grep -nE 'ICW1_AT|IRQ0_VECTOR|CASCADE_IRQ|ICW4_AT_MASTER|ICW4_AT_SLAVE|~0'
# 预期：9 行——ICW1_AT 2 行、IRQ0_VECTOR 1 行、CASCADE_IRQ 3 行（含 ~(1 << CASCADE_IRQ) 一行）、
#       ICW4_AT_MASTER 与 ICW4_AT_SLAVE 各 1 行（同一个 if/else 的两臂）、~0 1 行；
#       IRQ8_VECTOR 不在模式里，从片向量基那一行不出现。
#       配合 i8259.c 顶部的常量表把 0x50 / 0x04 / 0x02 / 0xFB / 0xFF 算出来。

# 4) ICW4 主从常量只差 bit 2 且注释相同
grep -nE 'ICW4_AT_(SLAVE|MASTER)' minix3/minix/kernel/arch/i386/i8259.c | head -2
# 预期：两行，值为 0x01 与 0x05，尾注都是 not SFNM, not buffered, normal EOI, 8086。

# 5) system_hz 的别名与它在 APIC 校准里的读者
grep -rn 'define system_hz' minix3/minix/kernel/glo.h
grep -n 'system_hz' minix3/minix/kernel/arch/i386/apic.c
# 预期：第一行输出 #define system_hz (kclockinfo.hz)；
#       第二行输出 5 行（169 的 PROBE_TICKS、491、517、520，以及 578 的 lapic_set_timer_one_shot 参数）。

# 6) C 的"注册即放行"行与 ARM 的"注册后放行"两行
grep -n 'hw_intr_unmask(hook->irq)' minix3/minix/kernel/interrupt.c
grep -n -A1 'only unmask interrupts after registering' minix3/minix/kernel/arch/earm/bsp/ti/omap_timer.c
# 预期：第一条两行——67 行的 put_irq_handler 内（首次挂链即放行）与 164 行的 enable_irq
#       （钩子完成后的重新放行）。irq_handle 末尾（155 行）与 rm_irq_handler（104 行）写的是
#       hw_intr_unmask(irq)，实参是裸 irq，不匹配本模式。第二条两行（注释 + bsp_irq_unmask 调用）。
```

第二组：Rust 侧的结构与常量（在 `os/` 目录运行）。

```bash
# 7) 定时器硬件编程的唯一调用点
rg -n 'clock_arch\.init_timer\(' --type rust kernel/src arch/src plat/src
# 预期：一行——kernel/src/lib.rs 的 boot_init_timer 内，实参为 DEFAULT_HZ 与当前 CPU 编号。
#       口径：调用表达式 clock_arch.init_timer(；不含 trait 声明与各架构实现的方法定义。

# 8) 三架构 TIMER_IRQ 的值
rg -n 'pub const TIMER_IRQ' plat/src
# 预期：七行——lib.rs 的三条 cfg 转发 + 一条 mock 兜底，以及
#       plat/src/{x86_64,arm64,riscv64}/interrupt.rs 各一行常量定义（0 / 30 / 0）。

# 9) 控制器侧的门与本地门是两个写者
rg -n 'fn enable_timer_irq' arch/src
rg -n 'self.controller.unmask' kernel/src/irq_manager.rs
# 预期：第一条四行（trait 声明 + 三个 impl）；第二条五处——register_hook_at_slot 的注册路径、
#       remove_hook 的移除路径、dispatch_claimed 的分发中重开、enable_irq 的显式重新放行、
#       remove_hook_by_slot 的按槽位移除路径。
# 10) 测试读数（宿主口径）
cargo test -p minix-kernel --features mock --lib -- --list | grep -c '^clock'
cargo test -p minix-arch --features runtime-window --lib -- --list | grep -c '^arch::clock'
# 预期：47 与 3（口径见 §5.1；两边都以 x86_64 宿主编译为准）。
```

第三组：真机载体（需要先构建工件；命令与预期见 §5.2，预期行为 `[待验证]`）。

```bash
bash os/qemu-tests/run_all.sh
os/qemu-tests/run_qemu.sh x86_64 os/target/x86_64-unknown-uefi/release/test-timer-irq.efi
# 预期关键行：### TEST_RESULT: PASS test-timer-irq ###。失败不写在这条横幅上——
#       x86_64 载体停住时由 panic 处理器打 ### PANIC in test-timer-irq: … ###，或者无输出，
#       由 run_qemu.sh 以"没有 PASS 标记"判失败；"  FAIL:" 前缀的分解行只出现在别的载体
#       （test-protection 一类），test-timer-irq 系列没有。
# 本篇写作期未实跑这一组；关键行取自载体源码，实际串口输出以运行为准。
```

---

## 7. 参见

### 7.1 前置与后续篇

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 启动信息的字段与内存记账（`KernelInfo` 的来源、`cut_memmap` 的用法、`param_buf` 的现状）
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 高半地址空间与恒等映射（设备寄存器页可被内核寻址的前提）；§4.4 的状态表给出本篇入口
- [03-kmain-cstart.md](03-kmain-cstart.md) — 保护结构（GDT/IDT/TSS）与 cstart 的调用序列；本篇 §1.1 的次序分析是它 §2.2 的下一段
- [04-platform-discovery.md](04-platform-discovery.md) — 描述符的来源与消费（本篇三次实例化取地址与频率；§3.4 的实例化模式、§4.8 的 downcast 表）
- 后续：tick 的六步语义与定时器队列（`15-clock-timer`（时钟与定时器篇，尚未重生成））、中断分发与 claim/complete 协议（`14-exception-interrupt`（异常与中断篇））、`bsp_finish_booting` 的其余步骤（`08-system-init-boot-finish`（启动收尾与切用户篇））、SMP 与 AP 定时器（`16-smp`（SMP 篇，尚未重生成））、进程表初始化（`06-proc-init-boot-proc`（进程表与 boot 进程篇））

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/main.c` — `cstart`（三行调用与参数解析）、`bsp_finish_booting`（定时器的调用点）、`get_value` / `env_get`
- `minix3/minix/kernel/clock.c` — `init_clock`、`boot_cpu_init_timer`、`app_cpu_init_timer`、`load_update`、`timer_int_handler`
- `minix3/minix/kernel/interrupt.c` — `put_irq_handler`、`rm_irq_handler`、`irq_handle`、`enable_irq` / `disable_irq`
- `minix3/minix/kernel/arch/i386/i8259.c` — `intr_init`、`irq_8259_mask` / `irq_8259_unmask`、`i8259_disable`
- `minix3/minix/kernel/arch/i386/arch_clock.c` — `init_local_timer`、`register_local_timer_handler`、`estimate_cpu_freq`、`init_8253A_timer`
- `minix3/minix/kernel/arch/i386/arch_system.c` — `arch_init`、`fpu_init`、`ser_init`
- `minix3/minix/kernel/arch/i386/apic.c` — `apic_single_cpu_init`、`lapic_enable`、`apic_calibrate_clocks`、`lapic_set_timer_one_shot`
- `minix3/minix/kernel/arch/earm/arch_system.c`、`arch/earm/arch_clock.c`、`arch/earm/bsp/ti/omap_intr.c`、`arch/earm/bsp/ti/omap_timer.c` — ARM-32 一侧的同一格
- `minix3/minix/include/arch/i386/include/interrupt.h` — 端口宏、向量基与线号定义；`minix3/minix/kernel/glo.h` — `system_hz` 别名宏与 `kclockinfo` 声明；`minix3/minix/include/arch/{i386,earm}/include/archconst.h` — 两个 `DEFAULT_HZ`

### 7.3 关联 Rust 源码清单

- `os/kernel/src/lib.rs` — `init_clock_and_interrupts`、`boot_init_timer`、`bsp_finish_booting`、四个全局的两套访问器
- `os/kernel/src/clock.rs` — `ClockState`、`ClockArch` 的消费者、`clock_irq_handler`、`local_tick`、`decrement_quantum`
- `os/kernel/src/irq_manager.rs` — `IrqManager`、`register_hook` / `register_hook_at_slot`、`dispatch` / `dispatch_claimed`
- `os/plat/src/lib.rs`、`os/plat/src/interrupt.rs` — `TIMER_IRQ`、`PROFILE_CLOCK_IRQ`、`InterruptRouter`、`PerCpuInterruptUnit`
- `os/plat/src/{x86_64,arm64,riscv64}/interrupt.rs` — 三架构控制器实现与线号常量
- `os/arch/src/arch/{clock,arch_init,timer_irq_gate}.rs` — 三个 trait 与它们的边界说明
- `os/arch/src/{x86_64,arm64,riscv64}/{clock,arch_init,timer_irq_gate}.rs` — 三架构实现
- `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq{,-aarch64,-riscv64}/src/main.rs`、`os/qemu-tests/run_all.sh`、`os/qemu-tests/README.md` — 真机载体与运行方式
- `os/kernel/src/globals.rs` — 时钟/中断相关的全局槽位与 `kinfo` 持有
- `os/arch/src/lib.rs` — 架构再导出与 clock/interrupt 相关入口
