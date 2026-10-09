# 04-platform-discovery：平台发现

> **C 真值源**：`minix3/minix/kernel/arch/i386/acpi.c`、`minix3/minix/kernel/arch/i386/acpi.h`、`minix3/minix/kernel/arch/i386/arch_system.c`、`minix3/minix/kernel/arch/i386/apic.c`、`minix3/minix/kernel/arch/i386/apic.h`、`minix3/minix/kernel/arch/i386/arch_smp.c`、`minix3/minix/kernel/arch/i386/memory.c`、`minix3/minix/kernel/arch/i386/i8259.c`、`minix3/minix/kernel/arch/i386/Makefile.inc`、`minix3/minix/kernel/main.c`、`minix3/minix/kernel/clock.c`、`minix3/minix/kernel/arch/earm/arch_system.c`、`minix3/minix/kernel/arch/earm/bsp/ti/{omap_init,omap_intr}.c`、`minix3/minix/kernel/usermapped_data.c`、`minix3/minix/kernel/system/do_getinfo.c`、`minix3/minix/drivers/power/acpi/acpi.c`、`minix3/minix/include/minix/{type.h,board.h}`、`minix3/minix/include/arch/i386/include/interrupt.h`
> **Rust 实现**：`os/libs/minix-boot/src/{platform,kernel_info}.rs`、`os/libs/minix-platform/src/{lib,desc,kind,global,acpi,device_tree,qemu_virt}.rs`、`os/libs/minix-platform/src/arch/{mod,x86_64,aarch64,riscv64}.rs`、`os/libs/minix-types/src/types/cell.rs`、`os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs`、`os/kernel/src/{lib,misc}.rs`、`os/arch/src/arch/{clock,arch_init}.rs`、`os/arch/src/{x86_64,arm64,riscv64}/clock.rs`、`os/arch/src/x86_64/arch_init.rs`、`os/plat/src/{lib,interrupt,early_console}.rs`、`os/plat/src/{x86_64,arm64,riscv64}/interrupt.rs`、`os/servers/sched/src/sef.rs`、`os/qemu-tests/run_all.sh`、`os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo{,-aarch64,-riscv64}/src/main.rs`
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:struct Name`、`path:const NAME`），不使用行号。
> **本篇主题**：内核怎么知道自己跑在哪块板子上——把固件给的描述（设备树 / ACPI 表 / QEMU 假定值）变成可用的硬件描述。
> **前置**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) 交出启动信息与内存记账（含 `KernelInfo.platform_sources` 字段的存放）、[02-higher-half-kernel.md](02-higher-half-kernel.md) 建立高半地址空间与恒等映射（DTB 物理地址能被直接解引用就靠它）、[03-kmain-cstart.md](03-kmain-cstart.md) 装上保护结构（`init_protection` 与 `init_clock_and_interrupts` 的先后由 kmain 的 Phase 决定）。
> **后续**：时钟与中断控制器的初始化从本篇产出的描述符里取地址与频率（[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §4.1 是描述符消费的逐步走查）；CPU 拓扑的完整使用在 SMP 篇（`16-smp.md`）。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；riscv64 的整套平台发现是 Architectural Evolution（Minix3 无 RISC-V 端口），涉及处标注 `[ARCH: K-2]`。

---

## 1. 概述

### 1.1 内核开机要问硬件的四组问题

内核在能处理中断、能计时、能输出日志之前，必须先知道四组硬件参数。这四组不是凭经验归纳出来的清单，每一组在 C 侧都有确定的消费者。

| 要问的问题 | 为什么需要 | C 侧消费者（或该值的来源） |
|---|---|---|
| 中断控制器在哪个 MMIO 地址 | 初始化控制器、mask/unmask IRQ 线 | `minix3/minix/kernel/arch/i386/apic.c:acpi_get_ioapics` |
| 计数器在哪、频率是多少 | 配置时钟中断频率、读取当前计数 | `minix3/minix/kernel/arch/i386/arch_clock.c:TIMER_FREQ`（8254 的输入时钟，常量）；CPU 主频由 `minix3/minix/kernel/arch/earm/arch_system.c:cpu_identify` 写进 `cpu_info[cpu].freq` |
| 有几颗 CPU、每颗的私有地址在哪 | SMP 启动、per-CPU 地址计算 | `minix3/minix/kernel/arch/i386/arch_smp.c:discover_cpus` |
| 早期日志写到哪 | 启动期输出诊断信息 | `minix3/minix/kernel/arch/i386/arch_system.c:ser_init` |

四组问题的答案来源**不是同一套机制**：

| 要问的问题 | C x86-64 的答案从哪来 | 是否来自固件描述 |
|---|---|---|
| IOAPIC 基址 | ACPI 的 MADT 记录（`acpi_ioa->address`） | 是 |
| LAPIC 基址 | 常量 `LOCAL_APIC_DEF_ADDR`（`0xfee00000`） | **否** |
| 定时器频率 | 常量（8254 的输入时钟 `1193182` Hz） | 否 |
| CPU 拓扑 | ACPI 的 MADT LAPIC 记录 | 是 |
| 早期控制台 | 常量（COM1 的三个端口宏 `COM1_LCR`/`COM1_DLL`/`COM1_DLM`） | 否 |

LAPIC 那一格是空的：MADT 的表头里明明有一个 `local_apic_address` 字段，C 从头到尾没读过它（§2.5 给出全部检索证据）。四组问题里只有两组真正走固件发现，另两组由编译期常量回答。

把这一格写死会坏在哪，可以落到一个具体地址上：`minix3/minix/kernel/arch/i386/apic.h` 定义 `LOCAL_APIC_DEF_ADDR` 为 `0xfee00000`，`minix3/minix/kernel/arch/i386/apic.c:apic_single_cpu_init` 无条件采用它。同一台机器的 IOAPIC 地址却是从表里读出来的——若固件把 LAPIC 窗口重定位，LAPIC 的读写会持续落在 `0xFEE0_0000` 这一片不再被解码的物理地址上，而 IOAPIC 的编程看起来一切正常：一个通、一个不通，故障现象是"IPI 发不出去但 IOAPIC 初始化成功"。

### 1.2 同一组问题，三种固件三种答法

四组问题的答案由固件以三种互不兼容的形式给出：

| 架构 | 固件 | 描述格式 | 交给内核的方式 |
|---|---|---|---|
| x86-64 | UEFI | ACPI 表链：RSDP → 根表 → MADT | UEFI 配置表里的 ACPI 表项 |
| aarch64 | UEFI（AAVMF）或 U-Boot | 设备树，**或** ACPI | 配置表里的设备树表项，或 ACPI 表项 |
| riscv64 | OpenSBI（+ U-Boot） | 设备树 | OpenSBI 在 `a1` 寄存器传 DTB 物理地址 |

三种格式回答的是同一组问题，但同一个内核的两个 C 端口用了两套完全不同的做法：x86 端口有一整个 ACPI 解析器（`minix3/minix/kernel/arch/i386/acpi.c`，410 行），ARM 端口连解析器都没有——它的中断控制器（OMAP INTCPS）基址、串口基址、定时器源时钟全部来自 BSP 头文件与板号分支（§2.9）。"统一"这件事在 C 里不存在，它是 Rust 侧要解决的问题（§3.2）。

"ARM 用设备树、x86 用 ACPI"是一句常见但不成立的口诀：本仓的 aarch64 路径同时准备了两条发现链，先试设备树、再试 ACPI（§4.2 给出实际扫描顺序）；而实测的 QEMU virt + AAVMF 配置表里没有设备树项，aarch64 的发现链因此实际走 ACPI（§4.3 的分派表与 §5.2 的真机门都按这条事实写）。

把三种格式当成"可以互相替代的命名差异"会坏在哪：C 的 ACPI 解析器只认 4 字节签名——根表签名 `"RSDT"` 由 `minix3/minix/kernel/arch/i386/acpi.c:acpi_check_signature` 比对（`acpi_read_sdt_at` 内），`"APIC"` 则由同文件 `acpi_get_table_base` 里的 `strncmp` 匹配；把它对准一棵设备树时读到的第一个 4 字节不是签名，`acpi_read_sdt_at` 直接返回 -1；反过来，设备树解析要匹配 `compatible` 字符串（如 `riscv,plic0`），而 ACPI 表里根本没有这个属性。两种格式的差别不在名字，在读法。

### 1.3 描述只在启动窗口里存在

固件给出的描述只在启动早期有效，这条约束在 C 侧有一条硬边界：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_phys2vir —— 逐字引用 */
static u32_t acpi_phys2vir(u32_t p)
{
	if(!vm_running) {
		DEBUGEXTRA(("acpi: returning 0x%lx as vir addr\n", p));
		return p;
	}
	panic("acpi: can't get virtual address of arbitrary physical address");
}
```

发现必须在 `vm_running` 变假之前完成——VM 跑起来之后，"把任意物理地址当虚拟地址用"这个动作直接 panic。而 VM 起来之后，固件内存已经按内存记账被收走（01 篇的记账机制见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4），描述所在的字节不再保证可读。

UEFI 侧还有一条更早的边界：系统表与配置表在 `exit_boot_services()` 之后失效，因此"从配置表里取到描述的物理地址"这个动作只能发生在引导阶段。两条边界合起来给出一条切分原则：

| 动作 | 必须发生在 | 理由 |
|---|---|---|
| 取到描述的物理地址 | 启动早期（固件服务退出前） | 配置表/系统表只在窗口内有效 |
| 解析描述 | 可以在内核里慢慢做 | 解析只依赖那份物理地址指向的字节，只要字节还在 |

这条切分要成立，交接物必须能跨二进制存活。Rust 侧为此把交接物做成纯数据对，并把理由写在模块注释里（§3.6）。

把"解析"也挤进窗口内会坏在哪：引导阶段的内存分配器是临时的（01 篇的引导期页表页分配器与固件池分配器都在这个窗口里工作），解析产生的中间结构要么占用那块临时内存、要么要求引导方实现内核侧的类型；而一旦内核与引导方是两个独立 ELF，这些中间结构还要跨二进制传递。当前设计把窗口内的动作压到最小——只搬一个地址。

### 1.4 发现要跨过哪几道关口

```
① 固件把描述放在约定位置
     x86-64: 配置表里的 ACPI 表项 → RSDP 物理地址
     aarch64: 配置表里的设备树表项（优先）或 ACPI 表项 → DTB / RSDP
     riscv64: OpenSBI 的 a1 寄存器 → DTB
        │
        ▼
② 引导阶段只取地址（不解析）——窗口内动作最小化（§1.3）
        │
        ▼
③ 内核按格式分派到对应解析器
     ACPI 链：RSDP → 根表 → MADT → 中断控制器记录 / CPU 记录
     设备树：节点与属性 → 中断控制器 / 定时器 / CPU 节点
        │
        ▼
④ 解析产物保存下来，此后只读
     C 侧：ACPI 模块的私有静态 + 跨特权级的 machine 字段
        │
        ▼
⑤ 各消费者在初始化阶段取用
     C 侧：apic_single_cpu_init / discover_cpus / init_local_timer / ser_init
```

链上每一段的输入与输出：

| 段 | 输入 | 输出 | 谁做的（C 侧） |
|---|---|---|---|
| ① | 固件自己的约定位置 | 描述所在物理地址 | 固件 |
| ② | 物理地址 | 交接记录里的一个地址字段 | 引导器（loader / boot-shim） |
| ③ | 物理地址 | 结构化的一组值 | 内核：`minix3/minix/kernel/arch/i386/acpi.c:acpi_init` |
| ④ | 结构化的一组值 | 可反复读取的全局 | 内核：ACPI 私有静态 + `machine.acpi_rsdp` |
| ⑤ | 全局 | 控制器可用、时钟在走 | 内核：`minix3/minix/kernel/arch/i386/apic.c:apic_single_cpu_init` 等 |

这张图在 §4.1 会用 Rust 侧的实际类型再走一遍，两处对照着看能看出哪些段换了承载物、哪些段的位置变了。

把第 ④ 段（保存为只读全局）省掉、让每个消费者各自去读一遍会坏在哪：C 侧会撞上 `acpi_phys2vir` 的 `vm_running` 检查——发现函数在 VM 起来之后拒绝工作（§1.3），而消费者（`apic_single_cpu_init`、`discover_cpus`、`init_local_timer`）分散在启动序列的不同位置，其中有的在 VM 起来之后才跑；Rust 侧同一错排会让 `init_clock_and_interrupts` 的第一行 `platform_desc()` 触发 `Frozen` 的读取断言（§4.1）。

### 1.5 本篇范围与可以安全跳过的内容

发现与编程的职责边界：

| 阶段 | 输入 | 输出 | 不做什么 |
|---|---|---|---|
| 硬件发现（本篇） | 固件给的只读描述（DTB 字节 / ACPI 表链 / 编译期常量） | 一组可读的硬件参数（基址、频率、CPU 拓扑、来源） | 不初始化控制器、不配置定时器、不写任何 MMIO |
| 硬件编程（后续篇） | 发现得到的值 | 控制器可用、定时器开始计数 | 不硬编码地址 |

可以安全跳过的内容：

| 可以跳过 | 为什么 | 去哪 |
|---|---|---|
| GDT/IDT/TSS 的装填细节 | 属保护结构的建立，与本篇的"参数从哪来"无因果关系 | [03-kmain-cstart.md](03-kmain-cstart.md) |
| 时钟与中断控制器的寄存器级编程 | 本篇只到"地址与频率值从哪来" | [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.3（8259 的十次 `outb`）与 §2.6（硬件定时器在哪编程） |
| FDT 的二进制格式（头部/结构块/字符串块）与 ACPI 的 AML 字节码 | 本篇只解释解析器实际读的那几个字段与偏移 | 规范原文或 `fdt` crate 文档 |
| SMP 的 AP 拉起过程 | 本篇只产出拓扑数据 | `16-smp.md` |

三条查表入口：QEMU virt 三架构常量表在 §4.6；C 侧四组问题的消费者在 §1.1；术语全称在 §7。

还有一条边界事实：Rust 侧的描述符里有一个"早期控制台"方法、三种来源的实现都写了它，但本篇检索范围内没有活消费者——早期控制台实际走各架构自己的模块。C 侧同一件事更直白：`ser_init` 用的是 COM1 的三个端口宏，与发现结果无关（§3.8 的表给出这条判别规则）。

把控制器初始化这类动作也算进"平台发现"会坏在诊断上：发现阶段的输入是固件给的只读表，编程阶段的输出是对 MMIO 的写。"描述符里缺一个地址"与"地址写不进去"在诊断上是两件事——前者的确认手段是查发现走了哪条路径，后者要查 MMIO 窗口与控制器型号。

---

## 2. C 源码分析

### 2.1 cstart 的调用序里，发现被放在哪一格

x86-64 的 C 启动序列把发现放在最后。`minix3/minix/kernel/main.c:cstart` 的四次调用按序是：

```c
/* minix3/minix/kernel/main.c:cstart —— 节选（只保留四次调用与其间的关键语句，省略号处为其余初始化） */
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

  ...
  DEBUGEXTRA(("intr_init(0)\n"));

  intr_init(0);

  arch_init();
}
```

`arch_init()` 内部的第一步是发现，紧随其后的第二步是发现结果的第一个消费者：

```c
/* minix3/minix/kernel/arch/i386/arch_system.c:arch_init —— 节选（省略号处为栈与串口的初始化） */
void arch_init(void)
{
	k_stacks = (void*) &k_stacks_start;
	assert(!((vir_bytes) k_stacks % K_STACK_SIZE));
	...
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

"发现晚于时钟与中断初始化"这句结论只对了一半。要看清它成立于哪些对象，得看被跳过的两个函数各自做什么。

`init_clock` 一行硬件操作都没有：

```c
/* minix3/minix/kernel/clock.c:init_clock —— 逐字引用 */
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

它做的是清结构体、从启动参数里读 `hz`、越界时回落 `DEFAULT_HZ`。既然它不碰硬件，"时钟初始化在发现之前"就不构成任何依赖。

`intr_init(0)` 是另一回事——它编 8259：

```c
/* minix3/minix/kernel/arch/i386/i8259.c:intr_init —— 节选（省略号处为余下的 ICW 序列） */
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
      ...
      outb( INT2_CTLMASK, ~0);		/* IRQ 8-15 mask */

  return OK;
}
```

它用的端口全部是 ISA 规范常量（`minix3/minix/include/arch/i386/include/interrupt.h` 定义 `INT_CTL 0x20`、`INT_CTLMASK 0x21`、`INT2_CTL 0xA0`、`INT2_CTLMASK 0xA1`），不需要任何发现结果。

于是准确的边界是：`init_clock` 与 8259 路径不依赖发现结果；而**真正消费 ACPI 结果的 APIC 初始化在发现之后、同一个函数体内**——`acpi_init()` 与 `apic_single_cpu_init()` 在 `arch_init` 里一前一后。

把这两步对调会坏在哪，可以逐跳数出来。`apic_single_cpu_init` 的第一步就是常量赋值，随后进 `detect_ioapics()`：

```c
/* minix3/minix/kernel/arch/i386/apic.c:detect_ioapics —— 逐字引用 */
int detect_ioapics(void)
{
	int status;

	if (machine.acpi_rsdp) {
		status = acpi_get_ioapics(io_apic, &nioapics, MAX_NR_IOAPICS);
	} else {
		status = 0;
	}
	if (!status) {
		/* try something different like MPS */
	}

	return status;
}
```

`machine.acpi_rsdp` 由 `get_acpi_rsdp()` 在 `acpi_init` 里填（§2.2）。这里有一个容易读反的地方：`apic_single_cpu_init()` **自己会先调一次 `acpi_init()`**（`minix3/minix/kernel/arch/i386/apic.c:1141`，紧接着 `:1143` 才是 `if (!detect_ioapics())`），所以"把 `arch_init` 里的两步对调"并不能制造出"字段还是 0"的状态——`acpi_init` 由被调函数内部发起，仍排在 `detect_ioapics` 之前。真正的依赖是：`detect_ioapics` 读 `machine.acpi_rsdp`，而这个字段必须有人先填。被对调顺序影响的只是"谁去填它"。

```c
/* minix3/minix/kernel/arch/i386/apic.c:apic_single_cpu_init —— 节选（函数开头的三段） */
int apic_single_cpu_init(void)
{
	if (!cpu_feature_apic_on_chip())
		return 0;

	lapic_addr = LOCAL_APIC_DEF_ADDR;
	ioapic_enabled = 0;

	if (!lapic_enable(0)) {
		lapic_addr = 0x0;
		return 0;
	}
	...
```

节选里这三段都在 `acpi_init()` **之前**执行，而 `detect_ioapics()` 排在它之后——两段次序写在函数体里。所以这条链不依赖 `arch_init` 的调用次序：`apic_single_cpu_init` 被调用时会自己先把 `acpi_init` 跑完再判断。它失败时的现象仍然是把 `lapic_addr` 清成 0 返回、退回 8259 路径、`machine.apic_enabled` 恒为 0、IOAPIC 一次都没初始化，只是那个失败不可能由"对调 `arch_init` 的两步"制造出来。

### 2.2 get_acpi_rsdp：两次扫描与只算 20 字节的校验和

内核要在没有任何指针提示的情况下找到根表。C 的做法是两次线性扫描：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:get_acpi_rsdp —— 逐字引用 */
static int get_acpi_rsdp(void)
{
	u16_t ebda;
	/*
	 * Read 40:0Eh - to find the starting address of the EBDA.
	 */
	acpi_phys_copy (0x40E, &ebda, sizeof(ebda));
	if (ebda) {
		ebda <<= 4;
		if(platform_tbl_ptr(ebda, ebda + 0x400, 16, &acpi_rsdp,
					sizeof(acpi_rsdp), &machine.acpi_rsdp,
					acpi_rsdp_test))
			return 1;
	} 

	/* try BIOS read only mem space */
	if(platform_tbl_ptr(0xE0000, 0x100000, 16, &acpi_rsdp,
				sizeof(acpi_rsdp), &machine.acpi_rsdp,
				acpi_rsdp_test))
		return 1;
	
	machine.acpi_rsdp = 0; /* RSDP cannot be found at this address therefore
				  it is a valid negative value */
	return 0;
}
```

逐条解码：

- `acpi_phys_copy (0x40E, &ebda, sizeof(ebda))` 读的是 `0x40E` 这个**内存单元**里的 16 位值，不是从 `0x40E` 开始扫描——注释 "Read 40:0Eh" 说的是段:偏移，`0x40E` 是 BIOS 数据区里存放 EBDA 段基址的固定位置。
- `ebda <<= 4` 把段值变成物理地址（段左移 4 位）。
- `platform_tbl_ptr(ebda, ebda + 0x400, 16, ...)` 在 `[ebda, ebda + 0x400)` 内以 16 字节为步长扫描——EBDA 的前 1 KiB。
- 第二次扫描是 BIOS 只读区 `[0xE0000, 0x100000)`，同样 16 字节步长。
- 两次扫描都失败时 `machine.acpi_rsdp = 0`，注释写明"0 是一个有效的负值"（"valid negative value"）——它同时是"没找到"的表示与后续判断的开关（§2.6）。
- `machine.acpi_rsdp` 是输出参数：`platform_tbl_ptr` 命中时把命中的物理地址写进去。

比较函数 `acpi_rsdp_test` 是两道闸：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_rsdp_test —— 逐字引用 */
static int acpi_rsdp_test(void * buff)
{
	struct acpi_rsdp * rsdp = (struct acpi_rsdp *) buff;

	if (!platform_tbl_checksum_ok(buff, 20))
		return 0;
	if (strncmp(rsdp->signature, "RSD PTR ", 8))
		return 0;

	return 1;
}
```

第一道闸的校验和覆盖 **20 字节**，而 ACPI 2.0 起的 RSDP 至少 36 字节（多出 `length`、`xsdt_addr`、`extended_checksum`、保留位）。也就是说 RSDP 扩展部分的校验和（偏移 32 的 `extended_checksum`）在 C 里从不校验。第二道闸的签名比对是 8 字节含尾随空格。

两道闸各自挡住什么，可以从缺了它们会怎样看出来。若去掉签名比对只留校验和：一个校验和恰好为 0 的 16 字节窗口就会被当成 RSDP，`rsdt_addr`（偏移 16 的 4 字节）取到任意值。`acpi_init` 下一步就用它调 `acpi_read_sdt_at`（§2.3），那里先读表头再比签名，`"RSDT"` 不匹配时返回 -1，`sdt_count` 由 `(s - sizeof(struct acpi_sdt_header)) / sizeof(u32_t)` 算出负值，循环 `for (i = 0; i < sdt_count; i++)` 一次都不执行——表清单为空，后续 `acpi_get_table_base("APIC")` 恒返回 NULL，`acpi_get_ioapic_next` 直接返回 NULL，`detect_ioapics` 得 0，整条 APIC 路径静默退化到 8259。签名与校验和是这条链唯一的两道闸。

### 2.3 acpi_read_sdt_at 与 sdt_count：一张 32 位表索引

C 只解析 RSDT。`minix3/minix/kernel/arch/i386/acpi.h:acpi_rsdp` 的字段止于 `rsdt_addr` 与 `length`：

```c
/* minix3/minix/kernel/arch/i386/acpi.h:struct acpi_rsdp —— 逐字引用 */
struct acpi_rsdp {
	char	signature[8]; /* must be "RSD PTR " */
	u8_t	checksum;
	char	oemid[6];
	u8_t	revision;
	u32_t	rsdt_addr;
	u32_t	length;
}; 
```

结构体里没有 `xsdt_addr`——ACPI 2.0 的 64 位根表指针（偏移 24）不在这个结构体的定义里。`acpi_init` 从头到尾只用 `rsdt_addr`，于是 C 的 ACPI 支持止步于 32 位表项。

`acpi_init` 把整个 RSDT 读进一个静态结构，再按返回长度算表项数：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_init —— 逐字引用 */
void acpi_init(void)
{
	int s, i;
	read_func = acpi_phys_copy;

	if (!get_acpi_rsdp()) {
		printf("WARNING : Cannot configure ACPI\n");
		return;
	}
	
	s = acpi_read_sdt_at(acpi_rsdp.rsdt_addr, (struct acpi_sdt_header *) &rsdt,
			sizeof(struct acpi_rsdt), ACPI_SDT_SIGNATURE(RSDT));

	sdt_count = (s - sizeof(struct acpi_sdt_header)) / sizeof(u32_t);

	for (i = 0; i < sdt_count; i++) {
		struct acpi_sdt_header hdr;
		int j;
		if (read_func(rsdt.data[i], &hdr, sizeof(struct acpi_sdt_header))) {
			printf("ERROR acpi cannot read header at 0x%x\n",
								rsdt.data[i]);
			return;
		}

		for (j = 0 ; j < ACPI_SDT_SIGNATURE_LEN; j++)
			sdt_trans[i].signature[j] = hdr.signature[j];
		sdt_trans[i].signature[ACPI_SDT_SIGNATURE_LEN] = '\0';
		sdt_trans[i].length = hdr.length;
	}

	acpi_init_poweroff();
}
```

接收缓冲区是文件顶部的静态定义（四段不连续，中间以省略号断开）：

```c
/* minix3/minix/kernel/arch/i386/acpi.c —— 节选（文件顶部的静态定义） */
#define MAX_RSDT	35 /* ACPI defines 35 signatures */
...
static struct acpi_rsdt {
	struct acpi_sdt_header	hdr;
	u32_t			data[MAX_RSDT];
} rsdt;
	
static struct {
	char	signature [ACPI_SDT_SIGNATURE_LEN + 1];
	size_t	length;
} sdt_trans[MAX_RSDT];

static int sdt_count;
...
```

`acpi_read_sdt_at` 在读取前后各设一道闸：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_read_sdt_at —— 节选（只保留签名/容量/校验和三道闸） */
static int acpi_read_sdt_at(phys_bytes addr,
				struct acpi_sdt_header * tb,
				size_t size,
				const char * name)
{
	struct acpi_sdt_header hdr;

	/* if NULL is supplied, we only return the size of the table */
	if (tb == NULL) {
		if (read_func(addr, &hdr, sizeof(struct acpi_sdt_header))) {
			printf("ERROR acpi cannot read %s header\n", name);
			return -1;
		}

		return hdr.length;
	}
	...
	if (acpi_check_signature(tb->signature, name)) {
		printf("ERROR acpi %s signature does not match\n", name);
		return -1;
	}

	if (size < tb->length) {
		printf("ERROR acpi buffer too small for %s\n", name);
		return -1;
	}

	if (read_func(addr, tb, size)) {
		printf("ERROR acpi cannot read %s\n", name);
		return -1;
	}

	if (acpi_check_csum(tb, tb->length)) {
		printf("ERROR acpi %s checksum does not match\n", name);
		return -1;
	}

	return tb->length;
}
```

这里有一个要手算的边界：`acpi_init` 传进去的 `size` 是 `sizeof(struct acpi_rsdt)`，即 `sizeof(struct acpi_sdt_header)` 加 `35 × sizeof(u32_t)`。SDT 头是 4+4+1+1+6+8+4+4+4 = 36 字节，所以缓冲区长 36 + 140 = 176 字节。`if (size < tb->length)` 这道闸的含义就是"RSDT 声明的长度不得超过 176 字节"——**表项数的上限由编译期常量 `MAX_RSDT` 决定，不由 ACPI 规范决定**。

去掉这道闸会坏在哪，要按两层看，**别把受害对象说错**。

第一层不是写穿而是**越界读**：那次拷贝是 `read_func(addr, tb, size)`，`size` 是调用方给的缓冲区长度（`acpi_init` 传 `sizeof(struct acpi_rsdt)`，176），**与表头 `length` 无关**——少了这道闸，这次拷贝仍只写 176 字节。真正拿 `tb->length` 当长度用的是紧随其后的 `acpi_check_csum(tb, tb->length)`（`minix3/minix/kernel/arch/i386/acpi.c:114`）：它按表头声明的长度去读校验和，越过了 176 字节的缓冲区做**越界读**。

第二层才是写越界，路径在别处：`acpi.c:323-338` 由表头长度算出 `sdt_count = (s - 36)/4`，随后 `for (i = 0; i < sdt_count; i++)` 读 `rsdt.data[i]`（越界读）并写 `sdt_trans[i].signature`（越界**写**，`sdt_trans` 只有 35 项）。被污染的是 `sdt_trans[]`：`acpi_get_table_base("APIC")` 从此按被覆盖的签名做 `strncmp`，找表失败或找到错的表。

### 2.4 MADT 记录的游标遍历与 CPU 拓扑的雏形

MADT 的消费被拆成两个独立入口，各自持一份函数内 `static` 游标。两者的遍历都靠同一个辅助函数：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_madt_get_typed_item —— 逐字引用 */
static void * acpi_madt_get_typed_item(struct acpi_madt_hdr * hdr,
					unsigned char type,
					unsigned idx)
{
	u8_t * t, * end;
	int i;

	t = (u8_t *) hdr + sizeof(struct acpi_madt_hdr);
	end = (u8_t *) hdr + hdr->hdr.length;

	i = 0;
	while(t < end) {
		if (type == ((struct acpi_madt_item_hdr *) t)->type) {
			if (i == idx)
				return t;
			else
				i++;
		}
		t += ((struct acpi_madt_item_hdr *) t)->length;
	}

	return NULL;
}
```

记录是变长的，前进步长直接取记录自己的 `length` 字段。这里没有零长保护：一条 `length == 0` 的记录会让 `t` 不再前进，而循环条件 `t < end` 恒真，函数永不返回。C 侧唯一的防护是信任固件；Rust 侧的解析器在同一处加了显式检查（§4.4 给出那两行）。

两个消费入口：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_get_lapic_next —— 逐字引用 */
struct acpi_madt_lapic * acpi_get_lapic_next(void)
{
	static unsigned idx = 0;
	static struct acpi_madt_hdr * madt_hdr;

	struct acpi_madt_lapic * ret;

	if (idx == 0) {
		madt_hdr = (struct acpi_madt_hdr *)
			acpi_phys2vir(acpi_get_table_base("APIC"));
		if (madt_hdr == NULL)
			return NULL;
	}

	for (;;) {
		ret = (struct acpi_madt_lapic *)
			acpi_madt_get_typed_item(madt_hdr,
					ACPI_MADT_TYPE_LAPIC, idx);
		if (!ret)
			break;

		idx++;

		/* report only usable CPUs */
		if (ret->flags & 1)
			break;
	}

	return ret;
}
```

`acpi_get_ioapic_next` 的形状相同，只是没有 `flags` 过滤那一段（它的循环体只有取记录与 `idx++`）。

三个可以逐条读出的事实：

- `acpi_get_lapic_next` 是 C 的 **CPU 拓扑发现**。它的循环在跳过"未启用"的记录（`ret->flags & 1` 为 0，MADT 里 `flags` 位 0 是 Processor Enabled）后返回第一个可用 CPU 记录。
- 两个函数的首次调用各自解析一次 MADT 基址并缓存进函数内 `static`；`acpi_get_table_base("APIC")` 走的是 §2.3 填好的 `sdt_trans[]` 签名表。
- `acpi_phys2vir` 有一条硬约束：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_phys2vir —— 逐字引用 */
static u32_t acpi_phys2vir(u32_t p)
{
	if(!vm_running) {
		DEBUGEXTRA(("acpi: returning 0x%lx as vir addr\n", p));
		return p;
	}
	panic("acpi: can't get virtual address of arbitrary physical address");
}
```

发现只能发生在 VM 起来之前。VM 跑起来之后 `vm_running` 为真，任何"把任意物理地址当虚拟地址用"的动作直接 panic——这条约束与 Rust 侧"boot 早期恒等映射覆盖低 4GB、DTB 可被直接解引用"（§4.5）是同一件事的两侧表达。

`discover_cpus` 是这条链的终点，也是 CPU 拓扑的消费者：

```c
/* minix3/minix/kernel/arch/i386/arch_smp.c:discover_cpus —— 逐字引用 */
static int discover_cpus(void)
{
	struct acpi_madt_lapic * cpu;

	while (ncpus < CONFIG_MAX_CPUS && (cpu = acpi_get_lapic_next())) {
		apicid2cpuid[cpu->apic_id] = ncpus;
		cpuid2apicid[ncpus] = cpu->apic_id;
		printf("CPU %3d local APIC id %3d\n", ncpus, cpu->apic_id);
		ncpus++;
	}

	return ncpus;
}
```

去掉 `acpi_get_lapic_next` 里那句 `if (ret->flags & 1) break;`（只留 `idx++`）会坏在哪：`discover_cpus` 会把 MADT 里所有 LAPIC 记录都计进 `ncpus`，包括 `flags` 位 0 为 0 的未启用项。`apicid2cpuid[]` 与 `cpuid2apicid[]` 两个映射表里于是出现永远不会应答 IPI 的条目；SMP 拉起 AP 时对那个不存在的 APIC ID 发 `APIC_IPI_DEST`，而 `apic_send_ipi` 在 `switch` 的 `case APIC_IPI_DEST` 分支里对未就绪的目标 CPU（`cpu_is_ready(cpu)` 为假）直接返回——函数级的首判是 `if (ncpus == 1) return;`，`cpu_is_ready` 这层保护在分支里（`minix3/minix/kernel/arch/i386/apic.c:apic_send_ipi`），表现为"少一颗 CPU 但不报错"。

### 2.5 LAPIC 基址：C 只发现了半格

`acpi_get_ioapic_next` 与 `acpi_get_lapic_next` 名字对称、实现对称。它们读出来的东西并不都进了使用。MADT 表头的结构（`minix3/minix/kernel/arch/i386/acpi.h:acpi_madt_hdr`）是：

```c
/* minix3/minix/kernel/arch/i386/acpi.h:struct acpi_madt_hdr —— 逐字引用 */
struct acpi_madt_hdr {
	struct acpi_sdt_header	hdr;
	u32_t			local_apic_address;
	u32_t			flags;
};
```

`local_apic_address` 这个字段在 i386 内核里的全部出现是：

```bash
$ rg -n 'local_apic' minix3/minix/kernel/arch/i386/
minix3/minix/kernel/arch/i386/acpi.h:99:	u32_t			local_apic_address;
```

一行声明，没有读取点。LAPIC 基址在两条启动路径上都被赋成同一个常量：

```c
/* minix3/minix/kernel/arch/i386/apic.h —— 逐字引用 */
#define LOCAL_APIC_DEF_ADDR	0xfee00000 /* default local apic address */
```

```c
/* minix3/minix/kernel/arch/i386/apic.c:apic_single_cpu_init —— 逐字引用（函数开头） */
int apic_single_cpu_init(void)
{
	if (!cpu_feature_apic_on_chip())
		return 0;

	lapic_addr = LOCAL_APIC_DEF_ADDR;
	ioapic_enabled = 0;
```

SMP 路径同样：

```c
/* minix3/minix/kernel/arch/i386/arch_smp.c:smp_init —— 节选（省略号处为前半段的入口处理） */
void smp_init (void)
{
	...
	lapic_addr = LOCAL_APIC_DEF_ADDR;
	ioapic_enabled = 0;

	tss_init_all();
	...
```

仓库里确实有一段代码会去读真实的 LAPIC 基址——从 `IA32_APIC_BASE` MSR 取地址位。它被包在 `#if 0` 里：

```c
/* minix3/minix/kernel/arch/i386/apic.c:lapic_enable_in_msr —— 节选（整段 #if 0 块，注释逐字保留） */
	ia32_msr_read(IA32_APIC_BASE, &msr_hi, &msr_lo);

#if 0
	u32_t addr;
	/*FIXME this is a problem on AP */
	/*
	 * FIXME if the location is different (unlikely) then the one we expect,
	 * update it
	 */
	addr = (msr_lo >> 12) | ((msr_hi & 0xf) << 20);
	if (addr != (lapic_addr >> 12)) {
		if (msr_hi & 0xf) {
			printf("ERROR : APIC address needs more then 32 bits\n");
			return 0;
		}
		lapic_addr = msr_lo & ~((1 << 12) - 1);
	}
#endif

	msr_lo |= (1 << IA32_APIC_BASE_ENABLE_BIT);
	ia32_msr_write(IA32_APIC_BASE, msr_hi, msr_lo);
```

块内注释写着 "FIXME this is a problem on AP"，块外的 `msr_lo |= ...` 只做"置使能位"这一件事——读地址那一段是死代码。所以 C 的活路径里，`lapic_addr` 的每一处赋值都是常量。

同一张表里，一个字段被读出来用了（IOAPIC 地址），另一个字段只有声明。要确认这一点，自己跑一遍上面那条 `rg` 就够了。

反过来只信任表头字段、不做任何校验也会坏：MADT 表头里的 `local_apic_address` 由固件填写，一张没填该字段的表会让 `lapic_base` 变成 0，后续所有 LAPIC 寄存器访问落到物理地址 0 附近——而 `detect_ioapics` 仍然成功（IOAPIC 地址是另一条记录）。Rust 侧对同一种"描述符给了 0"的形态有专门防护：aarch64 的中断控制器在 `new()` 里先判零再翻译，给出带类型名的断言信息而不是让 0 混进 MMIO 访问（`os/plat/src/arm64/interrupt.rs:fn new`）。

把这条不对称放到一台具体机器上看：固件把 LAPIC 窗口重定位到别处时，C 的 `lapic_addr` 仍指向 `0xFEE0_0000`（活路径没有别的赋值来源），而 `detect_ioapics` 依然成功——IOAPIC 地址是从表里读出来的正确值。两个控制器于是落在不同的物理窗口上：IOAPIC 的寄存器编程全部生效，LAPIC 的读写则指向一片不再被解码的地址。Rust 侧把这一格补上了：解析器从 MADT 表头读出 `Local APIC Address` 存进描述符（§4.4）。

### 2.6 IOAPIC 引脚数：表给基址、硬件给规模

IOAPIC 的消费分两层：

```c
/* minix3/minix/kernel/arch/i386/apic.c:acpi_get_ioapics —— 逐字引用 */
static int acpi_get_ioapics(struct io_apic * ioa, unsigned * nioa, unsigned max)
{
	unsigned n = 0;
	struct acpi_madt_ioapic * acpi_ioa;

	while (n < max) {
		acpi_ioa = acpi_get_ioapic_next();
		if (acpi_ioa == NULL)
			break;

		assert(acpi_ioa->address);

		ioa[n].id = acpi_ioa->id;
		ioa[n].addr = acpi_ioa->address;
		ioa[n].paddr = (phys_bytes) acpi_ioa->address;
		ioa[n].gsi_base = acpi_ioa->global_int_base;
		ioa[n].pins = ((ioapic_read(ioa[n].addr,
				IOAPIC_VERSION) & 0xff0000) >> 16)+1;
		printf("IO APIC idx %d id %d addr 0x%lx paddr 0x%lx pins %d\n",
				n, acpi_ioa->id, ioa[n].addr, ioa[n].paddr,
				ioa[n].pins);
		n++;
	}

	*nioa = n;
	return n;
}
```

表能回答"控制器在哪"，回答不了"它有多少根线"：

| 每个 IOAPIC 的数据 | 从哪来 |
|---|---|
| `id`（APIC ID） | MADT 记录 |
| `addr` / `paddr`（MMIO 基址） | MADT 记录 |
| `gsi_base`（起始 GSI 号） | MADT 记录 |
| `pins`（引脚数） | **硬件寄存器**：读 `IOAPIC_VERSION` 的位 `[23:16]` 再加 1 |

发现因此是两级的：固件描述给出"设备在哪"，设备自己回答"它有多大"。`assert(acpi_ioa->address)` 把"地址为 0 的 IOAPIC 记录"当作不该出现的输入，直接断言终止。

把引脚数也当成表里的数据（例如取 `global_int_base` 当作线数）会坏在哪：`ioa[n].pins` 被 `ioapic_enable_all()` 一类逻辑用来决定要 unmask 多少根线；用错量级会让若干根真实存在的 IRQ 线永远保持 masked（设备中断收不到），而系统不报任何错。表给的是"从第几号 GSI 开始"，与"有几根线"是两个不同的量。

### 2.7 acpi_init 的调用点在配置之间怎么选

`acpi_init()` 在 i386 源码里有三个调用点：

```bash
$ rg -n 'acpi_init\(\)' minix3/minix/kernel/
minix3/minix/kernel/arch/i386/apic.c:1141:	acpi_init();
minix3/minix/kernel/arch/i386/arch_smp.c:315:	acpi_init();
minix3/minix/kernel/arch/i386/arch_system.c:264:	acpi_init();
```

三个点里，`arch_system.c:264` 那处由 `arch_init` 调用（相对无条件）；另外两处（`apic.c:1141` 与 `arch_smp.c:315`）还要过一道**运行时开关**：`minix3/minix/kernel/main.c:443-447` 会读启动参数 `no_apic`，**未给出时把 `config_no_apic` 置为 1**，于是`arch_system.c:267-273` 的 `if (config_no_apic) {…} else if (!apic_single_cpu_init())` 走前一个分支、`apic.c:1141` 不执行；SMP 构建的 `main.c:304-320` 同样先判它（并且 `main.c:462-463` 还会把 `config_no_smp` 置 1）。**只有启动环境显式给出 `no_apic=0` 时才会执行第二次**——默认配置下 `acpi_init` 只跑 `arch_init` 里那一处。

| 构建配置 | 第一次（`arch_init` 内） | 第二次（还需启动参数 `no_apic=0`） |
|---|---|---|
| 非 SMP | `arch_system.c:264`（`#ifdef USE_ACPI`） | `apic.c:1141`，由 `apic_single_cpu_init()` 发起（守卫是 `#if defined(USE_APIC) && !defined(CONFIG_SMP)`） |
| SMP | `arch_system.c:264`（`#ifdef USE_ACPI`，与 SMP 无关） | `arch_smp.c:315`，由 `smp_init()` 发起（`arch_smp.c` 只在 `CONFIG_SMP` 时参与编译，见 `minix3/minix/kernel/arch/i386/Makefile.inc`） |

第二次不是空转：`acpi_init` 每次都从头扫描 EBDA/BIOS 区、重读 RSDT、重新填 `rsdt`/`sdt_trans`/`sdt_count`、重设 `machine.acpi_rsdp`。

它的失败路径不幂等：

```c
/* minix3/minix/kernel/arch/i386/acpi.c:acpi_init —— 节选（只保留函数开头的失败路径三行） */
	if (!get_acpi_rsdp()) {
		printf("WARNING : Cannot configure ACPI\n");
		return;
	}
```

第二次调用若 `get_acpi_rsdp()` 失败，函数只打印警告然后 `return`——上一次调用填好的 `sdt_count` 与 `sdt_trans[]` 原样留着，而 `machine.acpi_rsdp` 已被 `get_acpi_rsdp` 的结尾置 0。内核此时的状态是"表清单是旧的、开关关了"。

若把失败路径改成"把 `sdt_count` 清零"：`acpi_get_table_base("APIC")` 的循环 `for(i = 0; i < sdt_count; i++)` 一次都不执行，返回 NULL，`acpi_get_ioapic_next` 首次调用即返回 NULL（它的 `if (idx == 0)` 分支里 `madt_hdr == NULL` 直接 return），`detect_ioapics` 得 0——与当前代码的"清单还在但开关关了"相比，那是显式失败。当前代码选择了前者，后果是两条路径的判断依据（`sdt_count` 与 `machine.acpi_rsdp`）会互相矛盾。

### 2.8 发现结果往哪放：machine 结构

C 的发现结果不放进某个专用结构，而是分散进两个全局：ACPI 模块的私有静态（`rsdt` / `sdt_trans` / `sdt_count`，§2.3）与一个对外字段 `machine.acpi_rsdp`。`machine` 的声明在 `minix3/minix/kernel/glo.h:machine`，结构体定义（`struct machine`）在 `minix3/minix/include/minix/type.h`：

```c
/* minix3/minix/include/minix/type.h:struct machine —— 逐字引用 */
struct machine {
  unsigned processors_count;	/* how many cpus are available */
  unsigned bsp_id;		/* id of the bootstrap cpu */
  int padding;			/* used to be protected */
  int apic_enabled; /* does the kernel use APIC or not? */
  phys_bytes	acpi_rsdp; /* where is the acpi RSDP */
  unsigned int board_id;   /* Identifier for the board see   */
                           /* include/minix/board.h for more */
                           /* information.                   */
};
```

它的实例带段属性（`.usermapped`）——这段会被 VM 以用户权限映射进进程地址空间，供用户态直接读取内核发布的数据：

```c
/* minix3/minix/kernel/usermapped_data.c —— 逐字引用 */
struct machine machine __section(".usermapped");	/* machine information for services */
```

| 字段 | 谁写 | 谁读 |
|---|---|---|
| `processors_count` | `main.c:kmain` 的 `machine.processors_count = ncpus`（SMP）/ `= 1`（非 SMP） | 用户态（`do_getinfo.c` 的 `GET_MACHINE` 按 `sizeof(struct machine)` 拷贝） |
| `bsp_id` | 同上的 `machine.bsp_id = bsp_cpu_id` / `= 0` | 同上 |
| `padding` | 无人（注释 "used to be protected"） | 无人 |
| `apic_enabled` | `apic_single_cpu_init()` / `smp_init()` 里的 `machine.apic_enabled = 1` | 用户态 |
| `acpi_rsdp` | `get_acpi_rsdp()` 的成功路径写命中地址、失败路径写 0 | 内核内：`detect_ioapics()`；用户态：ACPI 驱动（经 `SYS_GETMACHINE` 拿一份拷贝） |
| `board_id` | `main.c:kmain` 的 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))` | 内核内：各 `BOARD_IS_*` 分支 |

`acpi_rsdp` 一栏有两个消费者，其中一个在用户态：

```c
/* minix3/minix/drivers/power/acpi/acpi.c —— 节选 */
	if (machine.acpi_rsdp == 0) {
```

这里的 `machine` 不是内核里那一个：驱动文件自己定义 `struct machine machine;`（`minix3/minix/drivers/power/acpi/acpi.c` 文件顶部），`init_acpi()` 开头用 `sys_getmachine(&machine)` 经 `SYS_GETMACHINE` 拿回一份**拷贝**——`minix3/minix/kernel/system/do_getinfo.c` 的 `GET_MACHINE` 分支按 `sizeof(struct machine)` 从内核的 `machine` 复制到调用者缓冲区。发现结果因此同时是内核内消费者与用户态消费者的输入，而这个字段的写入时机变成跨特权级的约定：拷贝读到的是调用那一刻的值。

C 内核另外把 `machine` 放进 `.usermapped` 段（`minix3/minix/kernel/usermapped_data.c` 的 `__section(".usermapped")`），这段由 VM 以用户权限映射进进程地址空间——它是"C 内核数据直接暴露给用户态"的通用机制；ACPI 驱动走的是上面的系统调用拷贝路径。

若内核不在 `machine` 里发布 `acpi_rsdp`（发现结果只在 ACPI 模块私有静态里流转）会坏在哪：用户态的 ACPI 驱动在 `init_acpi()` 里读到 0，打印 `ACPI: no RSDP` 后返回，`acpi_enabled` 保持 0，依赖它取设备信息的驱动拿不到数据；而内核内的 `detect_ioapics` 一切照常——同一个发现结果的两类消费者被切断一半，症状只在用户态出现。

### 2.9 ARM 端口：板级常量、板号位域与启动参数

earm 端口不做硬件发现。它的 `arch_init` 末尾只调 `bsp_init()`（四步板级映射）；CPU 型号与主频由 `bsp_finish_booting` 里的 `cpu_identify()` 填——调用发生在 `minix3/minix/kernel/main.c:bsp_finish_booting` 的开头（`cpu_identify();`），不在 `arch_init` 里：

```c
/* minix3/minix/kernel/arch/earm/arch_system.c:cpu_identify —— 逐字引用 */
void cpu_identify(void)
{
	u32_t midr;
	unsigned cpu = cpuid;

	asm volatile("mrc p15, 0, %0, c0, c0, 0 @ read MIDR\n\t"
		     : [midr] "=r" (midr));

	cpu_info[cpu].implementer = midr >> 24;
	cpu_info[cpu].variant = (midr >> 20) & 0xF;
	cpu_info[cpu].arch = (midr >> 16) & 0xF;
	cpu_info[cpu].part = (midr >> 4) & 0xFFF;
	cpu_info[cpu].revision = midr & 0xF;
	cpu_info[cpu].freq = 660; /* 660 Mhz hardcoded */
}
```

MIDR 是**读**出来的（CPU 型号自报），频率是常量 `660`。`arch_init` 末尾调用的 `bsp_init()`，四件事全是板级配置：

```c
/* minix3/minix/kernel/arch/earm/bsp/ti/omap_init.c:bsp_init —— 逐字引用 */
void
bsp_init(void)
{

	/* map memory for padconf */
	bsp_padconf_init();

	/* map memory for rtc */
	omap3_rtc_init();

	/* map memory for reset control */
	bsp_reset_init();

	/* disable watchdog */
	bsp_disable_watchdog();
}
```

"这是哪块板子"由一个打包位域表示。位域的形状写在头文件的注释里：

```c
/* minix3/minix/include/minix/board.h —— 逐字引用（位域说明与移位宏） */
/* 
  Utility functions to access/parse the board_id defined in the machine
  struct in include/minix/type.h.

   Identifier for the board 
     [31:28] Architecture.  (MINIX_BOARD_ARCH)
     [27:24] Architecture variant (MINIX_BOARD_ARCH_VARIANT) VERSION e.g. ARMV7 
     [23:16] Vendor/Soc (EG TI )  (MINIX_BOARD_VENDOR) 
     [15:8]  Board      (EG Beagle bone , beagle board ) (MINIX_BOARD)
     [7:0]   Board variant (EG BealgeBone white v.s. BeagleBone black ) (MINIX_BOARD_VARIANT)
*/

#define MINIX_BOARD_ARCH_SHIFT         (28)
#define MINIX_BOARD_ARCH_VARIANT_SHIFT (24)
#define MINIX_BOARD_VENDOR_SHIFT       (16)
#define MINIX_BOARD_SHIFT              (8)
#define MINIX_BOARD_VARIANT_SHIFT      (0)
```

五段的掩码与取值（`minix3/minix/include/minix/board.h`）：

| 位段 | 宽度 | 掩码宏 | 已知取值 |
|---|---|---|---|
| `[31:28]` 架构 | 4 | `MINIX_BOARD_ARCH_MASK` | `MINIX_BOARD_ARCH_X86`、`MINIX_BOARD_ARCH_ARM` |
| `[27:24]` 架构变体 | 4 | `MINIX_BOARD_ARCH_VARIANT_MASK` | `..._X86_GENERIC`、`..._ARM_ARMV6`、`..._ARM_ARMV7` |
| `[23:16]` 厂商/SoC | 8 | `MINIX_BOARD_VENDOR_MASK` | `MINIX_BOARD_VENDOR_INTEL`、`MINIX_BOARD_VENDOR_TI` |
| `[15:8]` 板 | 8 | `MINIX_BOARD_MASK` | `MINIX_BOARD_GENERIC`、`MINIX_BOARD_BBXM`、`MINIX_BOARD_BB` |
| `[7:0]` 板变体 | 8 | `MINIX_BOARD_VARIANT_MASK` | `..._GENERIC`、`..._BBW`、`..._BBB` |

四个已知板号由这些段或起来（`BOARD_ID_INTEL` / `BOARD_ID_BBXM` / `BOARD_ID_BBW` / `BOARD_ID_BBB`），匹配宏对板变体位做屏蔽：

```c
/* minix3/minix/include/minix/board.h —— 逐字引用 */
#define BOARD_IS_BBXM(v) \
		( (BOARD_ID_BBXM & ~MINIX_BOARD_VARIANT_MASK) == (v & ~MINIX_BOARD_VARIANT_MASK))
/* Either one of the known BeagleBones */
#define BOARD_IS_BB(v)   \
		( (BOARD_ID_BBW & ~MINIX_BOARD_VARIANT_MASK) == (v & ~MINIX_BOARD_VARIANT_MASK))
```

板号选的是中断控制器的物理基址：

```c
/* minix3/minix/kernel/arch/earm/bsp/ti/omap_intr.c:intr_init —— 节选（省略号处为基址映射调用与返回） */
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
	...
```

而板号本身不是从硬件读出来的——它来自启动参数里的板名字符串查表：

```c
/* minix3/minix/kernel/main.c:kmain —— 节选（省略号处为其余初始化） */
void kmain(kinfo_t *local_cbi)
{
  ...
   /* We have done this exercise in pre_init so we expect this code
      to simply work! */
   machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));
  ...
```

板号在时钟侧同样是一道分支：`minix3/minix/kernel/arch/earm/arch_clock.c:init_local_timer` 按 `BOARD_IS_BBXM` / `BOARD_IS_BB` 给 `tsc_per_ms[0]` 取 `16250` 或 `15000`，两个都不成立时执行 `panic("Can not do the clock setup. machine (0x%08x) is unknown\n", machine.board_id)`。

板号在两步里落地，缺名或查表失败**在更早的一步就已经中止**：

第一步是 `pre_init()`（`minix3/minix/kernel/arch/earm/pre_init.c`）里的 `set_machine_id()`：它从启动参数取**短名**（`A335BONE`/`BBXM`/`A335BNLT`）经 `get_board_id_by_short_name` 查表；`board_name=` 缺失或查表返回 0 时，它执行 `POORMANS_FAILURE_NOTIFICATION`（即 `svc #00`，源码注释写着 "there is no safe escape"）——此时还没进 `kmain`，更没到 `intr_init`。

第二步才是本节的 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`：它用**全名**反查同一张表。因此"板号为 0 导致 `intr_init` 走 `else` 执行 `panic("Can not do the interrupt setup. machine (0x%08x) is unknown\n", machine.board_id)`"这条路，只在**绕过了 pre_init 的情形**下才会走到（例如全名在 `board` 环境里丢了）；正常引导里它在 pre_init 就停住了。板号是这些分支的唯一判据，而它不是硬件给的。

换一块板子要改的地方，从这个结构里可以直接读出来：BSP 头文件里的基址常量、`bsp_init` 的四步映射、每个 `BOARD_IS_*` 分支，以及启动参数里的板名。

### 2.10 本章归纳

| 要问的问题 | C x86-64 的答案来源 | C ARM 的答案来源 | 是否来自固件描述 |
|---|---|---|---|
| IOAPIC 基址 | MADT 记录 | `BOARD_IS_*` 分支选 `OMAP3_*_INTR_BASE` | x86 是 / ARM 否 |
| LAPIC 基址 | 常量 `LOCAL_APIC_DEF_ADDR`（MADT 字段无读者） | 不适用 | 否 |
| 定时器频率 | 常量（8254 输入时钟 `TIMER_FREQ` = `1193182`） | 常量（OMAP 定时器的 32768 Hz 源时钟） | 否 |
| CPU 拓扑 | MADT LAPIC 记录（`flags & 1` 过滤） | 编译期（`CONFIG_SMP` 未定义 → `main.c` 走 `processors_count = 1`） | x86 是 / ARM 否 |
| 早期控制台 | COM1 的三个端口宏 | BSP 串口常量 | 否 |
| CPU 主频 | 不适用（用 TSC 校准） | 常量 `660`（`cpu_info[cpu].freq`） | 否 |

C 的做法可以概括成：x86-64 是**半发现**（IOAPIC 地址与 CPU 拓扑来自表，LAPIC 基址与早期控制台来自常量），ARM 端口是**零发现**（板级常量加启动参数板号）。发现结果一部分留在 ACPI 模块的私有静态里，一部分进跨特权级的 `machine`；发现动作在启动期可能被调用多次（`arch_init` 里一次，`apic_single_cpu_init`/`smp_init` 内部各一次），默认配置下只走其中一次——选哪一次由构建配置与启动参数 `no_apic` 共同决定（§2.7）。

把它搬到"三架构统一"的 Rust 实现里，会立刻遇到三个问题：

1. **发现该放在哪一层**——boot-shim 解析完再交，还是内核自己解析（C 的发现函数全在内核内）。
2. **三种来源怎么统一**——x86 走表链、ARM 走常量、RISC-V 走设备树，上层（时钟、中断）怎么才能不关心来源。
3. **结果怎么保存**——C 用 `static` 加约定，且 `machine` 还是跨特权级的；Rust 需要表达"启动期写一次、之后所有 CPU 只读"这条约定。

第 3 章按这三个问题展开。

照搬 C 的"发现跑两次"会坏在哪：Rust 侧同一份描述符会被解析两次并冻结两次，而 `Frozen::freeze` 里只有 `debug_assert!(!self.frozen.load(Acquire), "Frozen::freeze called twice")`——开发构建在第二次冻结时报错，发布构建静默覆盖第一次的值（§3.7）。C 侧那两次调用是"重扫一遍表、覆盖同一组静态变量"，Rust 侧对应的动作会直接撞上冻结协议。

C 侧的发现结果里没有"来源"这个概念。`machine` 的六个字段里没有任何一个说明 RSDP 是从 EBDA 还是 BIOS 区找到的，也没有字段说明用的是哪张表。诊断"参数看起来不对"时，能查的只有最终地址值。Rust 侧的描述符多带一个"来源"标签（§3.6），"走了兜底"与"解析成功但表内容属于另一台机器"因此成为两种可区分的状态——前者能直接判定，后者要靠地址值比对（§6.1 的诊断表按这条分叉）。

---

## 3. Rust 设计决策

### 3.1 解析职责的切分：boot-shim 只定位，kernel 自己解析（→§2.1、§2.8）

C 的发现全在内核内：`acpi_init` 是 kernel 的函数（`minix3/minix/kernel/arch/i386/acpi.c:acpi_init`），调用点也在 kernel 内（`minix3/minix/kernel/arch/i386/arch_system.c:arch_init`），引导器只负责把 `kinfo` 填好。"内核自己理解硬件"是微内核的职责边界。Rust 保留这条边界，把 boot-shim 的工作限制在"把固件给的物理指针取回来"。

三种方案：

| 方案 | 做法 | 否掉的理由 | 代价 |
|---|---|---|---|
| A | boot-shim 解析 DTB/ACPI 成结构化数据，通过 `KernelInfo` 传给内核 | `KernelInfo` 是 01 篇定义的跨二进制 handoff 结构，为每种来源各开字段（x86 的 LAPIC/IOAPIC 基址、aarch64 的 GICD/GICR 基址与 stride、riscv64 的 PLIC/CLINT 地址与频率）；新增一种控制器要动这个 ABI 结构 | boot-shim 越界执行内核工作 |
| B | 内核直接从固件接口读取（UEFI 运行时服务 / SBI 调用） | UEFI 运行时服务在 `exit_boot_services()` 之后不可用；RISC-V 的 SBI 没有硬件描述接口 | 内核依赖固件运行时 |
| **C（采用）** | boot-shim 定位 DTB/RSDP 的**原始物理指针**，装进 `KernelInfo`；内核解析 | — | 内核需要实现解析器（`os/libs/minix-platform/src/` 里的 `device_tree.rs` 与 `acpi.rs`） |

选 C 的收益不只是"职责清晰"。解析在内核里意味着解析产物可以**只**是内核类型，不必先变成 `KernelInfo` 的 ABI 字段再搬一次：

```rust
// os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo —— 节选（本字段的文档注释截断到第二段；省略号处为其余字段与注释后半）
pub struct KernelInfo {
    ...
    /// Platform descriptor sources — opaque handles carrying firmware table
    /// pointers (DTB, RSDP, or both). Ordered by boot-shim's preference;
    /// the kernel takes the first source that parses successfully.
    ///
    /// Empty slice means boot-shim did not provide any — kernel falls back
    /// to `QemuVirtDesc` (dev) or panics (release). See 04-platform-discovery.md §4.6.
    pub platform_sources: &'static [PlatformDescSource],
    ...
}
```

三种来源的全部差异都在内核侧消化，`KernelInfo` 上只有一个 `platform_sources` 字段。

改成方案 A 会坏在哪，可以落到具体一处新增上：`os/libs/minix-platform/src/arch/mod.rs` 的模块注释写着"新增一种中断控制器（例如 x2APIC）只需要三步——在 arch 子模块定义 struct、为它实现 `InterruptControllerDesc`、在解析器里改用它"。方案 A 下同样的新增要多改一步：`KernelInfo` 的字段布局（boot-shim 与内核两侧必须一致），而这是一个**跨二进制**的结构。

### 3.2 三个来源一个接口：trait 做接口、enum 做存储（→§2.9）

C 的 x86 与 ARM 走两套完全不同的代码：x86 有 ACPI 解析器（`acpi.c` 全文 410 行），ARM 端口连解析器都没有（§2.9）。上层（时钟、中断）无从统一。Rust 把"我跑在什么硬件上"定义成一个 trait：

```rust
// os/libs/minix-boot/src/platform.rs —— 签名示意（文档注释省略）
pub trait PlatformDesc: Send + Sync + fmt::Debug {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;
    fn timer(&self) -> &dyn TimerDesc;
    fn early_console(&self) -> Option<&dyn ConsoleDesc>;
    fn cpu_topology(&self) -> CpuTopology;
    fn arch_misc(&self) -> ArchMiscDesc;
    fn source(&self) -> PlatformSource;
    fn sswi_setip_base(&self) -> Option<usize> { None }
    fn psci_conduit(&self) -> Option<PsciConduit> { None }
}
```

八个方法里有六个是必需的，两个**带默认实现**（`sswi_setip_base()` 返回 `None`、`psci_conduit()` 返回 `None`）。这两个是"只有部分来源能回答的问题"：ACLINT SSWI 是 RISC-V 的能力（`os/libs/minix-platform/src/device_tree.rs:fn parse_sswi` 从 DTB 的 `riscv,aclint-sswi` 节点读基址），PSCI 调用通道是 ARM64 的能力（从 DTB 的 `/psci/method` 属性读 `hvc`/`smc`）。回答不了的来源用默认值表示"没有这个能力"，接口因此不需要为每种来源准备答案。这两个能力的消费者都在 SMP 的 AP 拉起路径上：riscv64 从 `sswi_setip_base()` 取基址，给目标核写 ACLINT SSWI 的 SETIP 寄存器（`os/arch/src/riscv64/smp.rs`）；aarch64 用 `psci_conduit()` 在 `hvc` 与 `smc` 之间选 PSCI 调用通道（`os/kernel/src/smp.rs` 读值后传给 `os/arch/src/arm64/smp.rs:fn set_psci_conduit_smc`）。本篇只把这两个值解析出来存进描述符，消费与后果见 `16-smp.md`。

接口之下，**存储仍是 enum**：

```rust
// os/libs/minix-platform/src/global.rs:enum PlatformDescEnum —— 逐字引用（枚举自身的文档注释省略，三条变体注释保留）
pub enum PlatformDescEnum {
    /// Parsed from a Flattened Device Tree (ARM64/RISC-V only).
    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
    DeviceTree(DeviceTreeDesc),
    /// Parsed from ACPI tables (x86-64, and aarch64 since S-2b).
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    Acpi(AcpiDesc),
    /// Hardcoded QEMU `virt` fallback (always available).
    QemuVirt(QemuVirtDesc),
}
```

三层职责：

| 层 | 承载物 | 变化频率 |
|---|---|---|
| 接口（消费方看到的） | `trait PlatformDesc` | 几乎不变：新增来源不改它 |
| 存储（描述符本体） | `PlatformDescEnum` 三选一 | 新增来源加一个变体 |
| 分派（每个方法的转发） | `impl PlatformDesc for PlatformDescEnum` 的 8 个方法，共 25 条 `match` 臂 | 新增来源时每条转发各加一臂 |

那 25 条的复算命令与口径：

```bash
$ rg -o 'Self::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/global.rs | wc -l
25
```

口径 = **条**（每个 `match` 臂算一条），不含构造点。这 25 条分布在 `Debug` 实现（3 条）与八个转发方法（22 条）里。这里要分清两件事：**消费方只认接口**成立；**维护方只有一处分支**不成立——`PlatformDescEnum` 自己就是那张 25 条的分派表。

改成 `Box<dyn PlatformDesc>` 会坏在哪：`os/libs/minix-platform/src/lib.rs` 是 `#![no_std]`，全局是 `static PLATFORM: Frozen<PlatformContext>`，T2.5 阶段没有分配器，`Box::new` 无处可取内存。`os/libs/minix-platform/src/global.rs` 的 enum 文档把这条理由写在"为什么不用 `Box<dyn PlatformDesc>`"一段里（`no_std`——避免 T2.5 阶段依赖分配器；enum 分派零 vtable 间接）。这不是"多花一点开销"的取舍，而是当前阶段不可用。

"零 vtable 间接"里的 `vtable` 是一张按 trait 方法声明次序排列的函数指针表：`&dyn PlatformDesc` 这类 trait object 由数据指针与 vtable 指针两个机器字组成，调用 `pd.timer()` 时先取表里第 n 项、再间接跳转。enum 分派没有这张表——`PlatformDescEnum` 的变体在编译期已知，`match self { ... }` 被编译成对判别值的直接分支，分支里调用的是具体类型的静态函数地址。两条路径的代价差别因此落在每次调用上：一条是判别加直接调用，另一条要多一次取表项与间接跳转。

### 3.3 子描述符：品牌枚举退到 arch 模块，接口只谈能力（→§2.5、§2.6）

C 的结构体字段名就是硬件名。`minix3/minix/kernel/arch/i386/acpi.h:acpi_madt_ioapic` 的字段是 `id`/`__reserved`/`address`/`global_int_base`——名字绑定在一种具体控制器上。Rust 的上一版把这种绑定写进了公共 API：子描述符是 enum，变体名直接是 `Apic`/`Gicv3`/`Plic`（中断控制器）、`Pit`/`ArmGenericTimer`/`Clint`（定时器）、`IsaSerial`/`MmioSerial`/`SbiConsole`（控制台），字段名也直接暴露寄存器布局（`gicr_stride`、`mtimecmp_stride`）。这些名字出现在 `KernelInfo` 的公共 API 表面上。

改造后它们变成带 `Any` 的 trait：

```rust
// os/libs/minix-boot/src/platform.rs —— 节选（三个 trait 的签名与关键注释）
pub trait InterruptControllerDesc: Send + Sync + fmt::Debug + Any {
    /// Number of IRQ sources supported.
    fn nr_irqs(&self) -> u32;
    /// `Any` support for arch-layer downcast.
    ///
    /// Implementors write `fn as_any(&self) -> &dyn Any { self }` — this
    /// works because in the impl block `Self` is a concrete sized type.
    /// The default impl is omitted because `&Self` → `&dyn Any` requires
    /// `Self: Sized`, which is not guaranteed inside the trait definition.
    fn as_any(&self) -> &dyn Any;
}

pub trait TimerDesc: Send + Sync + fmt::Debug + Any {
    /// Timer frequency in Hz (0 if read from hardware register at runtime).
    fn frequency(&self) -> u64;
    fn as_any(&self) -> &dyn Any;
}

pub trait ConsoleDesc: Send + Sync + fmt::Debug + Any {
    fn as_any(&self) -> &dyn Any;
}
```

`as_any()` 被声明为必需方法、不提供默认实现，原因是一条类型约束而不是风格选择：trait 定义里的 `self: &Self` 不保证 `Self: Sized`，而 `&Self → &dyn Any` 需要它；在 impl 块里 `Self` 是具体 sized 类型，转换才合法（同一段注释即此内容）。

品牌名 struct 退到 `minix-platform/src/arch/<arch>.rs`。每个 struct 的字段（`os/libs/minix-platform/src/arch/`）：

| 架构 | 中断控制器 | 定时器 | 控制台 |
|---|---|---|---|
| x86-64 | `ApicDesc { lapic_base: usize, ioapic_base: usize, nr_irqs: u32 }` | `PitDesc { pit_base_freq: u32, lapic_base: usize }` | `IsaSerialDesc { port_base: u16 }` |
| aarch64 | `Gicv3Desc { gicd_base: usize, gicr_base: usize, gicr_stride: usize, nr_irqs: u32 }` | `ArmGenericTimerDesc`（无字段） | `MmioSerialDesc { mmio_base: usize }` |
| riscv64 | `PlicDesc { plic_base: usize, nr_irqs: u32, context: u32 }` | `ClintDesc { mtime_addr: usize, mtimecmp_base: usize, mtimecmp_stride: usize, freq: u64 }` | `Riscv64ConsoleDesc { kind: Riscv64ConsoleKind }`（枚举 `Sbi` / `Mmio { mmio_base }`） |

上层与 arch 层的分工是：

| 需求 | 走哪条路 | 发生几次 |
|---|---|---|
| "有多少根 IRQ 线" | trait 方法 `nr_irqs()`，经 vtable | 每次调用 |
| "`lapic_base` 是多少" | arch 层 `as_any().downcast_ref::<ApicDesc>()` | 构造实例时一次 |

`os/plat/src/riscv64/interrupt.rs:fn new` 的 downcast 是这条分工的样本：

```rust
// os/plat/src/riscv64/interrupt.rs:fn new —— 节选（构造体）
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let plic = desc.as_any()
            .downcast_ref::<PlicDesc>()
            .expect("Riscv64InterruptController::new: expected PlicDesc");
        Self {
            plic_base: plic.plic_base,
            nr_irqs: (plic.nr_irqs as usize).min(NR_IRQ_VECTORS),
            context: plic.context as usize,
        }
    }
```

样本里的 `NR_IRQ_VECTORS` 是平台层的 IRQ 向量表容量（`os/plat/src/interrupt.rs:const NR_IRQ_VECTORS`，值 64，对位 C 的 `interrupt.h` 里同名常量）：描述符报出的线数超过这个容量时，`min(...)` 把实例字段钳到表长，防止越界的 IRQ 号在后面踩向量表。

这里要加一条限定：被 trait 化的是描述符内部的**品牌枚举**这三个；**"来源"这一层至今仍是枚举**（`PlatformDescEnum`，§3.2），`arch/riscv64.rs` 内部的 `Riscv64ConsoleKind` 也是枚举。三处枚举的角色不同：来源层枚举负责存储分派，品牌层枚举已被 trait 取代，arch 模块内部的枚举是模块私有实现细节。

把品牌名留在 trait 里会坏在哪：若给 `InterruptControllerDesc` 加一个 `fn gicd_base(&self) -> Option<usize>`，riscv64 与 x86-64 的实现只能返回 `None`，调用方要写"如果是 `None` 就说明不是 GIC"这类判断。品牌差异从"arch 层的 downcast 一次"扩散成"每个实现都要为一个自己不支持的概念提供一个空答案"，接口重新变成品牌清单的联合体——与这次改造要消除的形态相同。

### 3.4 描述符的值搬进实例字段（→§2.5、§2.6）

C 的硬件地址住在全局变量与全局数组里：`lapic_addr` 是文件级全局（`minix3/minix/kernel/arch/i386/apic.c` 里被 `apic_single_cpu_init` 赋值），`io_apic[]` 是全局数组，`ioapic_read(ioa[n].addr, ...)` 每次从数组元素取地址。Rust 的硬件 trait 改为实例化：

```rust
// os/arch/src/arch/clock.rs:trait ClockArch —— 节选（三个方法签名与关键注释）
pub trait ClockArch: Sized + Send + Sync {
    /// Create an instance from a timer descriptor.
    ///
    /// Stores the hardware parameters (base address, frequency) from the
    /// descriptor into instance fields. Instances are transient: each call
    /// site (timer init, read, stop, profile clock methods) constructs one
    /// from `platform_desc().timer()` and drops it after use.
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;

    /// Configure and start the hardware timer at the given frequency.
    fn init_timer(&mut self, hz: u32, cpu_id: u32);

    ...
}
```

三个硬件 trait 的实例化形态一致，区别只在构造参数与实例字段：

| trait | 构造参数 | 实例字段 | 实例的寿命 |
|---|---|---|---|
| `ClockArch`（`os/arch/src/arch/clock.rs`） | `&dyn TimerDesc` | 各架构的时钟地址与频率 | 瞬态（每个调用点构造一次、用完即弃） |
| `InterruptRouter`（`os/plat/src/interrupt.rs`） | `&dyn InterruptControllerDesc` | 各架构的控制器基址与线数 | 长期（存进全局 `IRQ_MANAGER`） |
| `ArchInit`（`os/arch/src/arch/arch_init.rs`） | `&ArchMiscDesc` | 架构杂项（x86 的 ACPI 表地址） | 瞬态 |

RISC-V 的时钟实例是搬运的样本：

```rust
// os/arch/src/riscv64/clock.rs:fn new —— 节选（构造体）
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
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
```

`ClockArch` 的实例是**瞬态**的（文档注释写明"每个调用点从 `platform_desc().timer()` 构造一个实例，用完即弃"），所以实例化没有引入"必须长期保存实例"的负担。真正被长期保存的是中断控制器实例——`os/kernel/src/lib.rs:fn init_clock_and_interrupts` 把它移进全局 `IRQ_MANAGER`。

实例化与全局变量相比的差别不在"少一次 load"，而在可变性与并发可见性：`lapic_addr` 是 `static`，任何代码都可以再写它，读者无法从类型上知道"这个值在启动后不再变"；实例字段在构造后没有可变引用（`init_timer(&mut self, ...)` 是唯一的可变入口），读者拿到 `&self` 就知道字段不会再变。

若 trait 方法不带 `&self`（自由函数式）：

```rust
// 教学简化：一种被否掉的形状，本仓不存在
pub trait ClockArch {
    fn init_timer(hz: u32);
    fn read_ticks() -> u64;
}
```

实例字段无处安放，硬件地址只剩两条去处：硬编码进函数体（riscv64 的时钟就会写死 `0x200_BFF8`，换平台全失灵），或塞进 `static mut`（`no_std` 下每次访问包 `unsafe`，且"启动期写一次"的约定没有任何检查）。两条都让 §3.7 的"冻结一次、只读共享"失去落点。

### 3.5 找不到描述时的兜底与它的代价（→§2.2、§2.7）

C 的 `get_acpi_rsdp()` 失败时只打印警告然后返回，`machine.acpi_rsdp` 置 0，整条 APIC 路径静默关闭（§2.6 的 `detect_ioapics` 就是这条开关的读点）。Rust 把这种"静默降级"改成显式的两条分支：

```rust
// os/libs/minix-platform/src/global.rs:fn qemu_fallback_or_panic —— 逐字引用（文档注释省略）
fn qemu_fallback_or_panic(reason: &str) -> PlatformDescEnum {
    if cfg!(debug_assertions) {
        // Dev build: warn-and-fallback. `log` may not be available in no_std
        // kernel context at this early stage; the warning is best-effort.
        // Callers that have log available can check `source()` == QemuVirt.
        let _ = reason; // suppress unused warning in no-log builds
        PlatformDescEnum::QemuVirt(QemuVirtDesc::default())
    } else {
        panic!(
            "platform::init_from_kinfo: {} and not a dev build (no QemuVirt fallback in release)",
            reason
        );
    }
}
```

兜底路径**什么都不打印**：开发构建那一支的函数体只有 `let _ = reason;` 与一次构造。区分"走了兜底"与"解析成功"只能靠 `source()`——注释里也这么写着（"Callers that have log available can check `source()` == QemuVirt"）。

三份 `QemuVirtDesc` 里的值全部是 QEMU virt 的机器布局（`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`）：

| 架构 | 中断控制器 | 定时器 | 控制台 | 拓扑 |
|---|---|---|---|---|
| x86-64 | `ApicDesc { lapic_base: 0xFEE0_0000, ioapic_base: 0xFEC0_0000, nr_irqs: 64 }` | `PitDesc { pit_base_freq: 1_193_182, lapic_base: 0xFEE0_0000 }` | `IsaSerialDesc { port_base: 0x3F8 }` | `nr_cpus: 4` |
| aarch64 | `Gicv3Desc { gicd_base: 0x0800_0000, gicr_base: 0x080A_0000, gicr_stride: 0x2_0000, nr_irqs: 64 }` | `ArmGenericTimerDesc` | `MmioSerialDesc { mmio_base: 0x0900_0000 }` | `nr_cpus: 4` |
| riscv64 | `PlicDesc { plic_base: 0x0C00_0000, nr_irqs: 64, context: 1 }` | `ClintDesc { mtime_addr: 0x200_BFF8, mtimecmp_base: 0x200_4000, mtimecmp_stride: 8, freq: 10_000_000 }` | `Riscv64ConsoleDesc::sbi()` | `nr_cpus: 4` |

场景与触发判定：

| 场景 | 是否触发兜底 | 触发条件 |
|---|---|---|
| 生产路径（真实硬件，boot-shim 找到了表） | 否 | `platform_sources` 非空，且逐源尝试里有一个来源解析成功 |
| QEMU 集成测试 | 否 | QEMU virt 提供 ACPI 或设备树，解析走真实路径 |
| boot-shim 没找到任何来源 | 是 | `platform_sources` 为空 |
| 切片里的来源全部解析失败 | 是 | 每个来源的解析器都返回 `Err` |

兜底不是"给一组安全默认值"，而是"给一组**特定机器**的常量"——"兜底生效"这件事本身就是"内核正在按 QEMU virt 的参数访问别的机器"。

发布构建也走兜底会坏在哪：在一台真实机器上，`init_from_kinfo` 会用 QEMU virt 的 PLIC/GICD 地址构造描述符，消费方在 `new(desc)` 里把这些地址搬进实例字段（aarch64 还会先过一次 `mmio_translate`，`os/libs/minix-platform/src/arch/aarch64.rs:fn mmio_translate` 的 const 断言会因地址落在 `0x0800_0000..0x0A00_0000` 窗口内而通过），随后第一次 MMIO 写落到一片未映射或非该设备的物理地址上。发布构建选择 panic，把故障从"随机 MMIO 写"提前到"启动期一条明确的 panic 信息"。

### 3.6 多来源的交接：不透明标签 + 有序切片（→§2.8）

C 只有一条来源（x86 走 RSDT，ARM 没有来源），"来源"不是一个需要表达的概念。Rust 的交接结构要把"哪几种来源"表达出来，同时不能让固件格式名出现在 handoff 层的公共 API 上：

```rust
// os/libs/minix-boot/src/platform.rs —— 逐字引用（文档注释省略）
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformDescKind(u32);

impl PlatformDescKind {
    /// Construct a kind tag. Only `minix-platform::kind` should call this.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    /// Raw value (for diagnostics / `Display`).
    pub const fn raw(self) -> u32 {
        self.0
    }
}

pub const DTB: PlatformDescKind = PlatformDescKind::new(1);
pub const RSDP: PlatformDescKind = PlatformDescKind::new(2);
```

两个常量定义在 `minix-boot`（handoff 层）而不是 `minix-platform`（解析层），理由是让 boot-shim 能在**不依赖 `minix-platform`** 的前提下构造来源——`minix-platform` 会把各架构的解析代码一起拉进来，而 boot-shim 只需要两个整数。这条理由写在 `os/libs/minix-boot/src/platform.rs` 的常量注释里（"They live here — in the handoff layer — rather than in `minix-platform`, so that boot-shim can construct sources without depending on `minix-platform`"）。

为什么不能带函数指针，模块注释把理由写在这一段：

```text
// os/libs/minix-boot/src/platform.rs —— 节选（模块文档）
//! A function pointer (`fn(PhysBytes) -> Result<...>`) would couple
//! `minix-boot` to `minix-platform` at the binary level — the pointer's
//! address is resolved in the boot-shim binary, but the function body lives
//! in `minix-platform`. Once TODO-02-3 makes the kernel an independent ELF,
//! the boot-shim's memory is reclaimed and the function pointer becomes a
//! dangling reference. An opaque `u32` tag + physical address is pure data
//! — safe to pass across binaries.
```

多来源的规则：`platform_sources` 是有序切片，内核按顺序试、取第一个解析成功的。切片元素是 `PlatformDescSource`（`os/libs/minix-boot/src/platform.rs:struct PlatformDescSource`）：一个 kind 标签加一个物理地址；地址字段的类型 `PhysBytes` 是 `minix-types` 里的一个 `#[repr(transparent)]` newtype（`os/libs/minix-types/src/types/address.rs:struct PhysBytes`，内层是 `u64`）——它比裸 `u64` 多的是类型标签：物理地址与虚拟地址（同文件的 `VirBytes`）在函数签名里不再是同一个可互换的类型，参数位置上混用会在编译期报错。

| 方案 | 跨二进制安全性 | 否掉的理由 |
|---|---|---|
| 不透明 `u32` 标签 + 物理地址（采用） | 纯数据，安全 | — |
| enum（直接列出 `Dtb` / `Rsdp` 变体） | 纯数据，安全 | 把固件格式名写进 handoff 层公共 API（§1.3 的设计意图是"上层不知道 kind 对应什么"） |
| 带解析函数指针 | 拆 ELF 后悬挂 | boot-shim 内存被回收后指针指向已归还的代码页（§1.3） |

加一种新格式要改三步，`os/libs/minix-boot/src/platform.rs` 的 kind 常量注释把它们列在同一处：

```text
// os/libs/minix-boot/src/platform.rs —— 节选（kind 常量块的普通注释）
// # Adding a new firmware table format
//
// 1. Add a new `pub const` here (e.g. `pub const SMBIOS: PlatformDescKind = ...`).
// 2. Add a match arm in `minix_platform::kind::parse_by_kind`.
// 3. Implement the parser in `minix_platform`.
```

这三步在 §4.3 逐条落地。

### 3.7 全局存储：从 AssumeSyncCell 到 Frozen（→§2.8）

C 用 `static` 全局加约定：ACPI 模块的三个私有静态谁写谁读靠时序和注释（§2.3），`machine` 还是跨特权级的 usermapped 全局（§2.8）。Rust 把"启动期写一次、之后只读"这条约定做成一个带协议的类型：

```rust
// os/libs/minix-types/src/types/cell.rs:struct Frozen —— 节选（结构体与关键注释）
/// - `freeze` happens-before any `get`: the writer stores `frozen` with
///   `Release` after writing the value; readers `Acquire`-load `frozen`
///   before touching the value. The first reader that observes `frozen ==
///   true` therefore sees the fully initialized `T`, and all later readers
///   share the same immutable `&T`.
pub struct Frozen<T> {
    value: core::cell::UnsafeCell<core::mem::MaybeUninit<T>>,
    frozen: core::sync::atomic::AtomicBool,
}
```

全局与两个入口：

```rust
// os/libs/minix-platform/src/global.rs —— 节选（三处定义）
static PLATFORM: Frozen<PlatformContext> = Frozen::new();

pub unsafe fn init(desc: PlatformDescEnum) {
    // The single freeze of PLATFORM (Release). Caller guarantees boot-phase
    // single-threaded context; readers wake strictly later (kmain order:
    // init_from_kinfo → … → smp_init wakes APs).
    PLATFORM.freeze(PlatformContext { desc });
}

pub fn platform_desc() -> &'static dyn PlatformDesc {
    // SAFETY: after `init()` completes (single-threaded boot), the cell is
    // never mutated again. All CPUs read the `&'static` reference safely.
    // The returned reference is `&'static` because `PLATFORM` is a static.
    PLATFORM.get()
}
```

方案否决表（`os/libs/minix-platform/src/global.rs:static PLATFORM` 的文档注释逐条给出）：

| 方案 | 否决理由 |
|---|---|
| `static mut` | 每次访问要 `unsafe`，且不给额外的安全性 |
| `Mutex` / `spin::Mutex` | 对不可变数据加锁是纯开销，语义也错——冻结之后没有东西需要保护 |
| `OnceLock` | 需要 `std` 或分配器，`no_std` 启动期不可用 |
| `AssumeSyncCell` | 它的安全契约是"调用方保证单线程独占访问"；AP 上线后 `platform_desc()` 会被多个 CPU 并发调用，契约形状与真实访问模式不符 |
| **`Frozen`（采用）** | `freeze` 一次（`Release`），读者 `Acquire`，之后共享不可变 `&T` |

`AssumeSyncCell` 是这次替换的前身，它仍在仓库里（`os/libs/minix-types/src/types/cell.rs`，与 `Frozen` 同一个文件），只是不再用于 `PLATFORM`。替换的理由写在 `static PLATFORM` 的文档注释里：AP 上线之后会与 BSP 并发读 `platform_desc()`，"永远无并发访问"的证明形状在 AP 上线那一刻失效。

`Frozen` 给的检查不是强制：`freeze` 里只有 `debug_assert!(!self.frozen.load(Acquire), "Frozen::freeze called twice")`——发布构建里重复 `freeze` 不会报错，会直接覆盖值。保证"只冻结一次"的最终依据是启动顺序（`init_from_kinfo` 只在 Phase A.5 调一次，§4.1）。

继续用 `AssumeSyncCell` 会坏在哪：它的契约要求调用方保证单线程独占，而 AP 上线后 `platform_desc()` 会被多个 CPU 并发调用（`os/libs/minix-platform/src/global.rs:static PLATFORM` 的注释记着这个事实）。此时契约不再成立，而类型本身没有任何机制能发现这件事——读者拿到的 `&T` 与写者的关系只能靠人工时序论证。`Frozen` 把读者侧从"独占"改成"只读共享"，契约形状与真实访问模式一致。换成 `Mutex` 也不坏，但读路径会为一份不可变数据付出加锁代价。

### 3.8 常量迁移的判别规则（→§2.5、§2.9）

不是所有硬件参数都该进描述符。C 侧同时存在两类常量：`intr_init` 用的 8259 端口（`minix3/minix/include/arch/i386/include/interrupt.h` 的 `INT_CTL 0x20` / `INT2_CTL 0xA0`）与 `LOCAL_APIC_DEF_ADDR`（`0xfee00000`）。前者永远只有那一个值，后者有报告它的协议（MADT 表头的 `local_apic_address`、`IA32_APIC_BASE` MSR 的地址位）。

判别规则：

| 硬件参数 | 有无协议报告机制 | 放哪 |
|---|---|---|
| PIT 的 I/O 端口 `0x40`/`0x43` | 无（ISA 规范字面量） | 架构常量 |
| 8259 的端口 `0x20`/`0x21`/`0xA0`/`0xA1` | 无 | 架构常量 |
| LAPIC 基址 | 有（MADT 表头字段、`IA32_APIC_BASE` MSR） | 描述符（`ApicDesc.lapic_base`） |
| IOAPIC 基址 | 有（MADT 的 IOAPIC 记录） | 描述符（`ApicDesc.ioapic_base`） |
| GICD / GICR 基址 | 有（设备树 `reg` 属性；aarch64 的 ACPI 路径读 GICD/GICR 记录） | 描述符（`Gicv3Desc`） |
| PLIC / CLINT 基址 | 有（设备树 `reg` 属性） | 描述符（`PlicDesc` / `ClintDesc`） |
| 早期控制台基址 | 有（ACPI 的 SPCR 表；设备树里的串口节点） | 描述符里有这一项（`ConsoleDesc`），但活路径不读它——早期控制台走各架构自己的模块（§4.8） |

判据是"有没有协议报告机制"，不是"值会不会变"。LAPIC 基址进描述符不是因为它会变（绝大多数机器上它不变），而是因为存在报告它的协议；PIT 端口不进描述符不是因为它不变，而是因为不存在任何报告它的协议。

把 ISA 字面量也搬进描述符会坏在哪：若给 `PitDesc` 加一个 `port_base: u16`，三个来源的每一个都要填这个字段，而三个值必然相同（`0x40`/`0x43`）。描述符于是同时含"可变字段"与"恒定字段"，类型上分不出"这个值可能随机器变"与"它处处相同"（§6.1 的诊断表按"描述符字段可疑 / 架构常量不可疑"分叉），而解析器的每个分支都要多写一个恒定值。

---

## 4. 实现详解

### 4.1 kmain Phase A.5：发现被提到所有消费者之前（→3.1、3.7）

`init_from_kinfo` 位于 `kmain` 的 Phase A.5，在 `init_protection` 之前、在 `init_clock_and_interrupts` 之前：

```rust
// os/kernel/src/lib.rs:fn arch_boot_impl —— 节选（Phase A.5 与 Phase B 的分界，注释保留）
    // Phase A.5: Platform discovery — initialize PlatformContext from KernelInfo.
    // This MUST run before init_clock_and_interrupts() because the clock,
    // interrupt controller, and arch_init all read hardware parameters
    // from the global platform descriptor (see 04-platform-discovery.md §3.4).
    // SAFETY: Single-threaded boot context; no concurrent access.
    unsafe {
        minix_platform::init_from_kinfo(kernel_info);
    }

    boot_stage!("kernel: kmain A.5 platform ok\n");
    // Phase B: cstart — protection + clock + interrupt
    ...
    init_protection(kernel_info);        // prot_init equivalent
    init_clock_and_interrupts();         // clock + intr + arch_init (covered in 05)
```

C 侧与 Rust 侧的并排时序：

| 动作 | C 的位置 | Rust 的位置 |
|---|---|---|
| 保护结构 | `prot_init()`，`cstart` 的第一步 | `init_protection(kernel_info)`，Phase B 的第一个动作 |
| 软件时钟状态 | `init_clock()`，`cstart` 的第二步 | `ClockState::new()`，在 `init_clock_and_interrupts` 内部 |
| 8259 | `intr_init(0)`，`cstart` 的第三步 | 有：`os/plat/src/x86_64/interrupt.rs:fn pic_init`（重映射 master→0x50 / slave→0x70 并放行 IRQ0 与级联线），在 `boot_init_timer` 末尾被调用；每 tick 的应答走 `os/kernel/src/trap_dispatch.rs` 的 IRQ 分发路径里的 `minix_plat::pic_eoi(irq)`（该调用点的上方注释标了它与 C 侧的 8259 回执对位（`minix3/minix/kernel/arch/i386/i8259.c:irq_8259_eoi`））。aarch64/riscv64 无此路径 |
| 发现 | `acpi_init()`，在 `cstart` 第四步 `arch_init()` 内 | `init_from_kinfo`，Phase A.5，**在保护结构之前** |
| 消费发现的控制器初始化 | `apic_single_cpu_init()`，紧随 `acpi_init()` 之后 | `CurrentInterruptController::new(...)` + `intr.init()`，在 `init_clock_and_interrupts` 内 |

Rust 侧的五段链（与 §1.4 的概念图对照，看哪些段换了承载物）：

```text
① 固件描述 → ② boot-shim 取物理地址，装进 KernelInfo.platform_sources
   → ③ init_from_kinfo 按 kind 分派解析（parse_by_kind）
   → ④ 写进 static PLATFORM: Frozen<PlatformContext>（一次冻结）
   → ⑤ 三个 new(desc) 把值搬进实例字段
```

Rust 把发现前移的理由不是"把 C 的顺序反过来"，而是消费者变多了：C 只有 APIC 初始化消费 ACPI 结果（§2.1），Rust 的时钟、中断控制器、架构初始化三者的 `new(desc)` 全都要读描述符。四组问题在 Rust 侧的消费者：

| 要问的问题 | Rust 侧谁问 | 在哪一步 |
|---|---|---|
| 中断控制器在哪 | `os/plat/src/interrupt.rs:trait InterruptRouter` 的 `new(desc)` | `init_clock_and_interrupts` |
| 计数器在哪、频率是多少 | `os/arch/src/arch/clock.rs:trait ClockArch` 的 `new(desc)` | `boot_init_timer` |
| 有几颗 CPU、每颗的私有地址 | `os/kernel/src/lib.rs:fn init_smp_state` | Phase D |
| 早期日志写到哪 | 描述符里有这一项但活路径不读它（§4.8） | — |

```rust
// os/kernel/src/lib.rs:fn init_clock_and_interrupts —— 节选（前三步）
    // Obtain the platform descriptor (initialized earlier from KernelInfo).
    // This is the single source of truth for all hardware parameters.
    let pd = platform_desc();

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    ...
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    intr.init();  // mask_all() called internally
    ...
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();
```

"必须早于谁"的确切名单就是这三个 `new(desc)`，加上后面 `boot_init_timer` 里的 `CurrentClockArch::new(pd.timer())`。

把 `init_from_kinfo` 挪到 `init_clock_and_interrupts` 之后会坏在哪：后者第一行 `platform_desc()` 触发 `Frozen::get` 的断言，内核在时钟初始化处停机。更隐蔽的一种排法是把它放在 `init_protection` 之前、但调用点排到 `boot_init_timer` 之后——此时 `init_clock_and_interrupts` 已经跑过，它内部的 `CurrentInterruptController::new(pd.interrupt_controller())` 会在 `pd` 尚未就绪时先崩在同一断言上。两次排错的症状是同一句 panic——运行时断言文本是 `"Frozen::get before freeze"`（`Frozen::get` 的断言；文档注释里写作 "frozen before freeze"），调用栈不同。

### 4.2 boot-shim 侧：find_platform_sources 的三架构扫描（→3.6）

```rust
// os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources —— 节选（x86-64 与兜底两段，注释保留）
        // On x86-64, look for ACPI RSDP (prefer ACPI 2.0+).
        #[cfg(target_arch = "x86_64")]
        {
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }
        ...
        // On other architectures (e.g., riscv64 with UEFI — currently
        // unsupported as a Rust target), return empty and let the kernel
        // use the QEMU fallback.
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = entries;
        }
    });

    // Leak the Vec so it has 'static lifetime — acceptable for boot-stage
    // code that runs exactly once (same pattern as build_memmap).
    Box::leak(sources.into_inner().into_boxed_slice())
```

| 架构 | 查哪些 GUID | 命中后是否继续 | 切片内容 |
|---|---|---|---|
| x86-64 | `ACPI2_GUID` 或 `ACPI_GUID`（同一次判断） | `break` | 最多 1 个元素（RSDP） |
| aarch64 | 先 `DEVICE_TREE_GUID`，再 `ACPI2_GUID` / `ACPI_GUID`（两段独立循环） | 每段各自 `break` | 最多 2 个元素（DTB 在前） |
| 其余（含 riscv64 的 UEFI 路径） | 不查 | — | 空切片 |

`Box::leak` 让切片获得 `'static`：这个函数在启动期只跑一次，泄漏是可接受的（注释里写明与 `build_memmap` 同一模式）。

riscv64 的来源走另一条路：

```rust
// os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot —— 节选（注释保留）
        // OpenSBI passes the DTB physical address in register a1.
        // The entry trampoline saves it via `install_dtb_ptr(a1)`.
        // If no DTB was passed (e.g., unit test), this returns None
        // and the kernel falls back to QemuVirtDesc.
        //
        // On RISC-V only DTB is supported (no ACPI). The sources list is
        // either `[DTB source]` or empty.
        let platform_sources: &'static [PlatformDescSource] = match dtb_ptr() {
            Some(pa) => {
                use alloc::boxed::Box;
                Box::leak(
                    alloc::vec![PlatformDescSource::new(DTB, PhysBytes(pa))]
                        .into_boxed_slice(),
                )
            }
            None => &[],
        };
```

aarch64 若写成单次 GUID 判断（`if e.guid == DEVICE_TREE_GUID || e.guid == ACPI2_GUID`）会坏在哪：循环按配置表项顺序扫描，先遇到哪个 GUID 就取哪个——来源由固件给出的表项顺序决定，不由设计意图决定。AAVMF 的表里 ACPI2 在、FDT 两个 GUID 都不在（§5.2 的实测），单次判断在这台机器上取到 ACPI 与两段循环的结果相同；但一台同时提供两者的服务器若把 ACPI 项排在前面，"DTB 优先"这条设计就失效——设计意图是先用更具体的设备树。顺序控制靠两段独立循环实现，不靠 GUID 判断的写法。

### 4.3 分派：parse_by_kind 与它的构造点（→3.6）

```rust
// os/libs/minix-platform/src/kind.rs:fn parse_by_kind —— 节选（两个分支与兜底臂，注释保留）
pub unsafe fn parse_by_kind(source: PlatformDescSource) -> Result<PlatformDescEnum, PlatformParseError> {
    match source.kind() {
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        DTB => {
            // SAFETY: caller guarantees phys_addr points to a valid FDT blob.
            match unsafe {
                crate::device_tree::DeviceTreeDesc::parse(source.phys_addr().0 as usize)
            } {
                Ok(d) => Ok(PlatformDescEnum::DeviceTree(d)),
                Err(_e) => Err(PlatformParseError::DtbParse),
            }
        }
        // S-2b: aarch64 joins the RSDP arm — AAVMF exposes the ACPI 2.0
        // RSDP via config table (empirically verified: 8 entries, ACPI2
        // present, both DTB GUIDs absent), and `AcpiDesc::parse` handles
        // MADT GICC for aarch64 topology.
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        RSDP => {
            // SAFETY: caller guarantees phys_addr points to a valid RSDP.
            match unsafe { crate::acpi::AcpiDesc::parse(source.phys_addr().0 as usize) } {
                Ok(d) => Ok(PlatformDescEnum::Acpi(d)),
                Err(_e) => Err(PlatformParseError::AcpiParse),
            }
        }
        k => Err(PlatformParseError::UnknownKind(k.raw())),
    }
}
```

§3.6 列的三步在这里逐条落地：

| 步骤（§3.6） | 落点 |
|---|---|
| 在 handoff 层加一个 kind 常量 | `os/libs/minix-boot/src/platform.rs:const DTB` / `:const RSDP`，由 `os/libs/minix-platform/src/kind.rs` re-export |
| 在 `parse_by_kind` 加一个 match 臂 | 上面代码里的两个臂（各自的 `#[cfg]` 决定它在哪些架构编译） |
| 在 `minix-platform` 实现解析器 | `os/libs/minix-platform/src/device_tree.rs:fn parse` / `os/libs/minix-platform/src/acpi.rs:fn parse` |

分派里的两个分支各构造一个变体，全仓另有一处兜底构造（`qemu_fallback_or_panic`）与一处测试构造：

```bash
$ rg -c 'PlatformDescEnum::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/kind.rs os/libs/minix-platform/src/global.rs
os/libs/minix-platform/src/global.rs:2
os/libs/minix-platform/src/kind.rs:4

$ rg -n 'Ok\(PlatformDescEnum::' os/libs/minix-platform/src/kind.rs
57:                Ok(d) => Ok(PlatformDescEnum::DeviceTree(d)),
69:                Ok(d) => Ok(PlatformDescEnum::Acpi(d)),
```

两条命令的口径不同：`rg -c` 数的是**含匹配的行**，`kind.rs` 那 4 行里有 2 行是 `parse_by_kind` 文档注释里的分派表（`os/libs/minix-platform/src/kind.rs` 顶部），代码里的构造表达式只有上面两条；`global.rs` 的 2 行是兜底与测试各一处。

`parse_by_kind` 的 `RSDP` 臂在 aarch64 上也编译，`DTB` 臂在 x86-64 上不存在——同一份分派表在三架构下臂数不同。只看 x86-64 的编译结果会以为 DTB 臂不存在于仓库。

把分派写成兜底式（`if kind == DTB { ... } else { AcpiDesc::parse(...) }`）会坏在哪：一个未知 kind（例如未来新增的 SMBIOS 标签被旧内核遇到）会被送进 ACPI 解析器，`AcpiDesc::parse` 读到的第一个 8 字节不是 `"RSD PTR "` 就返回 `BadRsdpSignature`，失败原因被报成"ACPI 解析失败"而不是"未知标签"。现设计里 `k => Err(PlatformParseError::UnknownKind(k.raw()))` 让这一类错误有自己的名字（`os/libs/minix-boot/src/platform.rs:enum PlatformParseError` 的三个变体 `UnknownKind(u32)` / `DtbParse` / `AcpiParse`）。

### 4.4 ACPI 解析器（→3.3、3.8）

`AcpiDesc::parse` 从 RSDP 开始：

```rust
// os/libs/minix-platform/src/acpi.rs:fn parse —— 节选（注释有删节：规范节号改写为不带 § 的形式）
        // RSDP layout (ACPI 6.4 规范 5.2.5):
        //   0: signature (8)
        //   8: checksum (1)
        //   9: oem_id (6)
        //  15: revision (1)
        //  16: rsdt_addr (4)
        //  20: length (4)        [rev 2+]
        //  24: xsdt_addr (8)     [rev 2+]
        //  32: extended_checksum  [rev 2+]
        //  33: reserved (3)       [rev 2+]
        let revision = unsafe { *((rsdp_phys + 15) as *const u8) };

        // Prefer XSDT (64-bit) on revision 2.0+; fall back to RSDT.
        let xsdt_phys = if revision >= RSDP_REV_2 {
            let xsdt_bytes = unsafe {
                core::slice::from_raw_parts((rsdp_phys + 24) as *const u8, 8)
            };
            ...
        } else {
            let rsdt_bytes = unsafe {
                core::slice::from_raw_parts((rsdp_phys + 16) as *const u8, 4)
            };
            u32_le_from_slice(rsdt_bytes) as usize
        };
```

与 C 的差别直接体现在这张表里：C 的 `struct acpi_rsdp` 没有 `xsdt_addr` 字段（§2.3），Rust 按 revision 选择 XSDT（偏移 24 的 8 字节）或 RSDT（偏移 16 的 4 字节）。

MADT 的解析是遍历骨架加记录分派：

```rust
// os/libs/minix-platform/src/acpi.rs:fn parse_madt —— 节选（表头读法与遍历骨架）
    let madt_len = u32_le_from_slice(length_bytes) as usize;

    // MADT header follows the SDT header.
    let madt_header_size = core::mem::size_of::<SdtHeader>() + core::mem::size_of::<MadtHeader>();
    if madt_len < madt_header_size {
        return Err(AcpiParseError::MadtTooShort);
    }
    ...
    let lapic_base = u32_le_from_slice(lapic_addr_bytes) as usize;
    ...
    let mut offset = madt_header_size;
    while offset + core::mem::size_of::<MadtEntryHeader>() <= madt_len {
        ...
        let entry_type = entry_hdr_bytes[0];
        let entry_len = entry_hdr_bytes[1] as usize;
        if entry_len < core::mem::size_of::<MadtEntryHeader>() || offset + entry_len > madt_len {
            break;
        }
```

`lapic_base` 这一行就是 §2.5 里 C 没做的那一步：从 MADT 表头读出 Local APIC Address。遍历的两道闸（`entry_len` 不小于记录头长度、`offset + entry_len` 不越界）是 C 的 `acpi_madt_get_typed_item` 没有的（§2.4）。

MADT 记录类型：Rust 侧定义六个常量，C 侧声明十一个、实际处理两个：

| 记录类型 | 值 | Rust 处理 | C 处理 |
|---|---|---|---|
| LAPIC（Processor Local APIC） | 0 | 是（`flags & 1` 过滤） | 是 |
| IOAPIC | 1 | 是 | 是 |
| X2APIC | 9 | 是（`flags & 1` 过滤） | 否 |
| GICC（Generic Interrupt Controller CPU interface） | 11 | 是（aarch64） | 否 |
| GICD（GIC Distributor） | 12 | 是（aarch64） | 否 |
| GICR（GIC Redistributor） | 14 | 是（aarch64） | 否 |
| 其余（Interrupt Source Override 等） | — | 跳过（`_ => {}`） | 未声明处理 |

aarch64 的 GICR 记录有一个要看清的地方：

```rust
// os/libs/minix-platform/src/acpi.rs —— 节选（注释有删节：规范节号改写为不带 § 的形式）
/// GICC (Generic Interrupt Controller CPU interface) structure type — aarch64.
/// ACPI 6.x 规范 5.2.12.14: 80-byte record. Per-CPU identity is read from the ACPI
/// Processor UID (u32 at +8); Flags (bit 0 = Processor Enabled) at +12. QEMU
/// zeroes the embedded GICR Base field (+60) for GICv3, so the redistributor
/// base is NOT sourced here — see [`MADT_TYPE_GICR`].
const MADT_TYPE_GICC: u8 = 11;
...
/// GIC Redistributor structure type — aarch64. ACPI 6.x 规范 5.2.12.16, 16 bytes as
/// emitted by QEMU `build_append_gicr` (Reserved u16 at +2, Discovery Range Base
/// Address u64 at +4, Discovery Range Length u32 at +12). This is the
/// authoritative GICv3 redistributor source: the GICC embedded GICR field is
/// zero on QEMU virt (and only carries a v4.1 compatibility value on real
/// firmware), so this subtable is the only discovery route that boots aarch64.
const MADT_TYPE_GICR: u8 = 14;
```

遍历结束后 aarch64 过一道决策函数：

```rust
// os/libs/minix-platform/src/acpi.rs:fn check_gic_madt —— 逐字引用（文档注释省略）
fn check_gic_madt(
    nr_cpus: u32,
    gicd_base: usize,
    gicr_base: usize,
    gicd_version: u8,
) -> Result<(), AcpiParseError> {
    if nr_cpus == 0 {
        return Ok(());
    }
    if gicd_base == 0 {
        return Err(AcpiParseError::GicdNotFound);
    }
    if matches!(gicd_version, 1 | 2) {
        return Err(AcpiParseError::GicVersionUnsupported(gicd_version));
    }
    if gicr_base == 0 {
        return Err(AcpiParseError::GicrNotFound);
    }
    Ok(())
}
```

它的决策表（对应单测 `os/libs/minix-platform/src/acpi.rs:fn test_check_gic_madt_decision_table` 的八条断言）：

| 输入（`nr_cpus`, `gicd_base`, `gicr_base`, `gicd_version`） | 结果 |
|---|---|
| `(0, 0, 0, 0)` | `Ok(())`（无 CPU 记录，不是这道闸的业务） |
| `(1, 0x0800_0000, 0x080A_0000, 3)` | `Ok(())`（GICv3，QEMU `gic-version=3` 的形状） |
| `(1, 0x0800_0000, 0, 3)` | `Err(GicrNotFound)` |
| `(1, 0x0800_0000, 0x0803_0000, 2)` | `Err(GicVersionUnsupported(2))`（QEMU `gic-version=2` 默认形状：版本字节 2、那个字段是 GICV 帧基址） |
| `(1, 0x0800_0000, 0x080A_0000, 1)` | `Err(GicVersionUnsupported(1))` |
| `(1, 0, 0x080A_0000, 3)` | `Err(GicdNotFound)`（GICD 缺失的判定优先于其余检查） |
| `(1, 0x0800_0000, 0x080A_0000, 0)` | `Ok(())`（版本字节未读——GICD 记录短于 21 字节） |
| `(1, 0x0800_0000, 0, 0)` | `Err(GicrNotFound)` |

`check_gic_madt` 的作用是在描述符交给驱动之前把 GICv1/v2 与"没有 GICR 子表"两种表拒绝掉。不做这道检查会坏在哪：QEMU `virt` 的 GICv2 默认配置下，GICC 里那个 GICR 字段是 GICV 帧基址 `0x0803_0000`，AArch64 中断控制器的 `new()` 会把它当 GICR 基址搬进实例字段（零基址断言通过，因为非 0），第一次对"重定位寄存器"的写落到 GICV 帧上——`check_gic_madt` 的注释写明这条路径的实测后果是外部中止（externally abort）。拒绝一次 MADT 的代价是 `init_from_kinfo` 落到下一个来源或兜底，比驱动写错寄存器后中止可控。

### 4.5 设备树解析器（→3.3、3.4）

`DeviceTreeDesc::parse` 用 `fdt` crate 借用 DTB 字节，一次性把需要的值全部提取出来（eager parsing）：

```rust
// os/libs/minix-platform/src/device_tree.rs:fn from_fdt —— 逐字引用
    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
    fn from_fdt(fdt: &fdt::Fdt<'_>) -> Result<Self, DtParseError> {
        let ic = Self::parse_interrupt_controller(fdt)?;
        let timer = Self::parse_timer(fdt)?;
        let console = Self::parse_console(fdt);
        let cpu_topology = Self::parse_cpu_topology(fdt);
        let arch_misc = ArchMiscDesc::default();
        let sswi_setip_base = Self::parse_sswi(fdt);
        let psci_conduit = fdt.find_node("/psci").and_then(|n| {
            n.property("method")
                .and_then(|p| p.as_str())
                .and_then(|m| match m {
                    "hvc" => Some(PsciConduit::Hvc),
                    "smc" => Some(PsciConduit::Smc),
                    _ => None,
                })
        });

        Ok(Self {
            ic,
            timer,
            console,
            cpu_topology,
            arch_misc,
            sswi_setip_base,
            psci_conduit,
        })
    }
```

七行提取对应描述符的七个字段（`ic` / `timer` / `console` / `cpu_topology` / `arch_misc` / `sswi_setip_base` / `psci_conduit`）。为什么必须这样提取，`os/libs/minix-platform/src/device_tree.rs` 的模块注释把理由写全：

```text
// os/libs/minix-platform/src/device_tree.rs —— 节选（模块文档）
//! - No heap allocation: `Fdt` borrows the DTB slice for `'a`.
//! - The borrowed `Fdt` cannot outlive the DTB bytes — but `PlatformDesc`
//!   must be `Send + Sync + 'static`. We solve this by **eager parsing**:
//!   `DeviceTreeDesc::parse` walks the FDT once, extracts all needed values
//!   into owned `usize`/`u32`/`u64` fields (stored as concrete arch structs),
//!   then drops the `Fdt` borrow. The resulting `DeviceTreeDesc` is `'static`
//!   and safe to store globally.
```

`parse` 拿到手的只是一个物理地址，能被直接解引用，靠的是 boot 早期的恒等映射：`os/libs/minix-platform/src/device_tree.rs:fn parse` 里 `fdt::Fdt::from_ptr(dtb_phys as *const u8)` 把物理地址当指针传给 `fdt` crate，前提是此时内核已启用分页、低 4GB 铺了 VA == PA 的恒等映射（[02-higher-half-kernel.md](02-higher-half-kernel.md) 建立的映射，C 侧对应 `pg_identity()`），而 DTB 由 boot-shim 留在低地址的静态固件内存里。模块注释的 Safety 段把这条前提列成解析前置条件：

```text
// os/libs/minix-platform/src/device_tree.rs —— 节选（模块文档 Safety 段）
//! - Early boot identity mapping covers the physical address: during boot
//!   the kernel enables paging with an identity mapping over the low 4 GB
//!   (C `pg_identity()` semantics, `os/kernel/src/lib.rs:27-28`), and the
//!   DTB lives in static firmware memory far below that bound — so the raw
//!   physical address can be dereferenced as a virtual address (VA == PA).
//!   If a future path hands a DTB above the identity-mapped range, `parse`
//!   must be preceded by a temporary mapping.
```

同一份约束在 C 侧的表现就是 §2.4 的 `acpi_phys2vir`：`vm_running` 为假时把物理地址原样返回，为真时直接 panic；两侧说的是同一件事——"把任意物理地址当虚拟地址用"只在启动窗口内成立。

把 `Fdt` 借用存进 `DeviceTreeDesc` 会坏在哪：描述符要进 `static PLATFORM: Frozen<PlatformContext>`（`os/libs/minix-platform/src/global.rs:static PLATFORM`），而 `static` 要求 `'static`；`Fdt<'a>` 借的是 boot-shim 交来的 DTB 字节，其内存在固件内存被回收后不再有效（§1.3 的窗口），于是描述符里的每个字段都成了悬挂读。eager parsing 的代价是解析时多走一遍树，收益是描述符与 DTB 字节解耦。

字段的来源分两类：

| 字段 | 来源 | 缺省/失败 |
|---|---|---|
| `ic`（riscv64：`PlicDesc`） | `compatible` 匹配 `riscv,plic0` / `sifive,plic-1.0.0`；基址取 `reg` 首格；线数取 `riscv,ndev` 属性 | 节点找不到或 `reg` 缺失 → `Err`；`riscv,ndev` 缺失 → 64 |
| `ic`（aarch64：`Gicv3Desc`） | `compatible` 匹配 `arm,gic-v3`；`reg` 两格分别是 GICD 与 GICR | 节点或 `reg` 缺失 → `Err`；`gicr_stride` 与 `nr_irqs` 是常量 |
| `timer`（riscv64：`ClintDesc`） | CLINT 节点 `reg` 首格作基址，`mtime` = 基址 + `0xBFF8`，`mtimecmp` = 基址 + `0x4000`；频率取 `/cpus/timebase-frequency` | 节点缺失时改用 ACLINT MTIMER 节点（两个 `reg` 格）；频率缺失 → `Err` |
| `timer`（aarch64） | 只确认存在（`ArmGenericTimerDesc` 无字段） | 频率运行时从 `CNTFRQ_EL0` 读 |
| `console` | riscv64：`ns16550a` / `sifive,uart0` 节点存在则取 `reg` 首格，否则 `Riscv64ConsoleDesc::sbi()`；aarch64：`arm,pl011` / `arm,primecell` 节点 | 节点缺失 → `None`（riscv64 例外，回落到 SBI） |
| `cpu_topology` | 遍历所有节点，按节点名 `@` 前的部分与 `cpu` 精确相等过滤（`n.name.split('@').next() == Some("cpu")`），`reg` 首格作 `hw_id` | 无 CPU 节点 → `CpuTopology::default()`（单核） |
| `sswi_setip_base` | `compatible` 匹配 `riscv,aclint-sswi`，取 `reg` 首格 | 缺失 → `None`（正常） |
| `psci_conduit` | `/psci` 节点的 `method` 属性，`"hvc"` / `"smc"` | 缺失或值不认识 → `None` |

CLINT 的地址要手算一次。QEMU virt 的 DTB 里 CLINT 节点是 `clint@2000000`，`reg` 首格给基址 `0x0200_0000`；两个寄存器的地址是基址加固定偏移：

```text
mtime     = 0x0200_0000 + 0xBFF8 = 0x0200_BFF8
mtimecmp  = 0x0200_0000 + 0x4000 = 0x0200_4000
```

同一条 `parse_timer` 还有一条 ACLINT 备用路径：QEMU `aclint=on` 时 `riscv,clint0` 节点被 `riscv,aclint-mswi` + `riscv,aclint-sswi` + `riscv,aclint-mtimer` 取代，MTIMER 节点的两个 `reg` 格分别是 mtime 与 mtimecmp。`os/libs/minix-platform/src/device_tree.rs:fn parse_aclint_mtimer` 的注释记录了实测值（`0x200bff8` / `0x2004000`，与 CLINT 偏移一致）。代码里的选择顺序是先试 CLINT 再试 ACLINT（`parse_timer` 的 `Self::parse_clint(fdt).or_else(|_| Self::parse_aclint_mtimer(fdt))`）。

CPU 拓扑的提取走 `all_nodes` 过滤而不是 `Fdt::cpus()`：

```rust
// os/libs/minix-platform/src/device_tree.rs:fn parse_cpu_topology —— 节选（注释有删节：行首的追踪编号未保留）
        // 走 all_nodes 过滤而非 `Fdt::cpus()`——后者内部
        // `find_node("/cpus")` 在 QEMU `-kernel` 直核链重建出的 blob 上
        // 返回 None 并 expect panic（fdt 0.1.5 find_node 对该 blob 的
        // 既有缺陷，chosen 同症）；all_nodes 遍历无此问题。节点名 "cpu"
        // 按 DT 规范只出现在 /cpus 下，过滤语义等价（两架构同形）。
        for cpu in fdt.all_nodes().filter(|n| n.name.split('@').next() == Some("cpu")) {
```

每个 CPU 的 `hw_id` 直读 `reg` 首格（`#address-cells` 为 1 或 2 两种形态都认），per-CPU 私有地址按架构计算：aarch64 是 `gicr_base + hw_id * gicr_stride`，riscv64 是 `mtimecmp_base + hw_id * mtimecmp_stride`。

### 4.6 QemuVirtDesc：三份 per-arch 常量表（→3.5、3.8）

`QemuVirtDesc` 有三个独立定义，`os/libs/minix-platform/src/qemu_virt.rs` 只做 re-export：

```rust
// os/libs/minix-platform/src/qemu_virt.rs —— 逐字引用（文档注释省略）
#[cfg(target_arch = "x86_64")]
pub use crate::arch::x86_64::QemuVirtDesc;
#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::QemuVirtDesc;
#[cfg(target_arch = "riscv64")]
pub use crate::arch::riscv64::QemuVirtDesc;
```

每个架构的实例只含本架构的字段：

```rust
// os/libs/minix-platform/src/arch/riscv64.rs:impl QemuVirtDesc —— 逐字引用（注释保留）
#[derive(Debug)]
pub struct QemuVirtDesc {
    ic: PlicDesc,
    timer: ClintDesc,
    console: Riscv64ConsoleDesc,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl QemuVirtDesc {
    /// Construct the hardcoded QEMU `virt` riscv64 descriptor.
    pub const fn new() -> Self {
        Self {
            ic: PlicDesc {
                plic_base: 0x0C00_0000,
                nr_irqs: 64,
                context: 1,
            },
            timer: ClintDesc {
                mtime_addr: 0x200_BFF8,
                mtimecmp_base: 0x200_4000,
                mtimecmp_stride: 8,
                freq: 10_000_000,
            },
            console: Riscv64ConsoleDesc::sbi(),
            cpu_topology: CpuTopology {
                nr_cpus: 4,
                bsp_id: 0,
                cpus: [CpuInfo::zero(); MAX_CPUS],
            },
            arch_misc: ArchMiscDesc {
                acpi_tables: None,
                pmu_cycle_counter: false,
            },
        }
    }
}
```

三份常量表里哪些值在真实机器上会变、哪些不会：

| 值 | 性质 |
|---|---|
| `plic_base: 0x0C00_0000` | QEMU virt 的机器布局（真实板子不同） |
| `mtimecmp_stride: 8` | CLINT 的设备布局（SiFive CLINT 规范） |
| `pit_base_freq: 1_193_182` | 8254 的输入时钟频率（ISA 设备常量） |
| `lapic_base: 0xFEE0_0000` | ISA 默认值，但有报告协议（§3.8 的判据） |
| `gicr_stride: 0x2_0000` | GICv3 的每帧跨度（RD_base 64 KiB + SGI_base 64 KiB） |
| `mmio_base: 0x0900_0000` | QEMU virt 的 PL011 地址（真实板子不同） |
| `nr_cpus: 4` | 测试配置（QEMU `-smp 4`） |

`new()` 与 `default()` 的分工由 `const fn` 的语言限制逼出来：

```rust
// os/libs/minix-platform/src/arch/riscv64.rs:impl Default for QemuVirtDesc —— 逐字引用
impl Default for QemuVirtDesc {
    fn default() -> Self {
        // `cpus` needs runtime initialization for per-CPU mtimecmp_addr fields.
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
```

`#[derive(Default)]` 生成的 `default()` 不是 `const fn`，所以 `const fn new()` 里不能调 `CpuInfo::default()`——源码因此提供了 `CpuInfo::zero()` 这个 `const fn`（`os/libs/minix-boot/src/platform.rs:fn zero`，注释写明"`#[derive(Default)]` 生成的是非 const 的 `default()`，所以 `const fn` 不能调它"）。per-CPU 私有地址需要循环填充，只能放在非 const 的 `default()` 里。

`#[cfg]` 的分布：

| 位置 | `#[cfg]` 用途 |
|---|---|
| `os/libs/minix-platform/src/arch/mod.rs` | 模块选择（三个 `pub mod` 各带一个 `#[cfg]`） |
| `os/libs/minix-platform/src/qemu_virt.rs` | re-export 选择（三个 `pub use` 各带一个 `#[cfg]`） |
| `os/libs/minix-platform/src/global.rs:enum PlatformDescEnum` | 枚举变体门控 |
| 三个 per-arch 文件的方法体内 | **0 处** |

把三份实现合回一份、在方法体里写 `#[cfg(target_arch)]` 会坏在哪：每个方法体都要为三架构各写一段，`arch_misc()` 这类跨架构相同的方法也会被迫带上三路分支；更具体的一处代价是 `QemuVirtDesc` 的字段类型会退化成"三架构字段的并集"（`ApicDesc` 与 `PlicDesc` 同时作为字段存在），编译期不再保证"riscv64 的实例里不会出现 LAPIC 基址"。per-arch 文件方案让每个架构的实例只含本架构的字段（`os/libs/minix-platform/src/arch/mod.rs` 的开放-封闭注释记着这条）。

### 4.7 全局写入与读取（→3.7）

`init_from_kinfo` 是写入侧：

```rust
// os/libs/minix-platform/src/global.rs:fn init_from_kinfo —— 逐字引用（文档注释省略）
pub unsafe fn init_from_kinfo(kinfo: &KernelInfo) {
    // Try each source in boot-shim's preference order. The first that
    // parses successfully wins.
    let mut parsed: Option<PlatformDescEnum> = None;
    for source in kinfo.platform_sources {
        // SAFETY: caller guarantees each source's phys_addr points to a
        // valid firmware table.
        match unsafe { parse_by_kind(*source) } {
            Ok(desc) => {
                parsed = Some(desc);
                break;
            }
            Err(_e) => {
                // Continue to next source on parse failure.
                // (In dev builds, log the failure; in release, silently
                // proceed to the next source or fallback.)
            }
        }
    }

    let desc = match parsed {
        Some(d) => d,
        None => qemu_fallback_or_panic("no platform source parsed successfully"),
    };

    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { init(desc) };
}
```

失败分支里的注释写着"开发构建记录失败"——但这一支的实际动作只是继续下一个来源，没有任何输出（§3.5 的兜底路径同样不打日志）。

读取侧是 `PlatformContext` 的八次转发：

```rust
// os/libs/minix-platform/src/global.rs:impl PlatformDesc for PlatformContext —— 节选（前两个方法与 source）
impl PlatformDesc for PlatformContext {
    fn interrupt_controller(&self) -> &dyn crate::desc::InterruptControllerDesc {
        self.desc.interrupt_controller()
    }
    fn timer(&self) -> &dyn crate::desc::TimerDesc {
        self.desc.timer()
    }
    ...
    fn source(&self) -> crate::desc::PlatformSource {
        self.desc.source()
    }
```

`source()` 存在的技术原因是 trait object 不暴露具体类型——上层拿到 `&dyn PlatformDesc` 时不知道底层是 `DeviceTreeDesc` 还是 `QemuVirtDesc`。让每个实现自己声明出身是类型擦除后的手工 RTTI：

| 用途 | 评价 |
|---|---|
| 单测断言分派走到了哪个变体（`os/libs/minix-platform/src/arch/*.rs` 的 `test_qemu_virt_source`） | 合理 |
| 诊断时区分"走了兜底"与"解析成功" | 合理 |
| 上层按来源走不同逻辑（例如"来自 ACPI 才做某段调试"） | 反模式，见下 |

上层按 `source()` 分支会坏在哪：同一种硬件参数会因为来源不同而走不同逻辑，`source()` 从诊断信息变成控制输入。具体坏在诊断：一台 SBBR 服务器（DTB 与 ACPI 都在，切片顺序决定取哪个）在两次启动间若固件配置变化导致来源改变，按来源分支的逻辑会跟着变，而硬件参数本身没变——故障现象随启动次序漂移。

`psci_conduit()` 是八个转发方法里唯一一条不是三分支的：`os/libs/minix-platform/src/global.rs:enum PlatformDescEnum` 的转发实现里它只有 `DeviceTree` 一臂是真的，其余走 `_ => None`。这是"接口统一但能力分布不均"的一处落点，也是 §3.2 那个计数（25 条）里唯一的非三分支方法。

### 4.8 消费侧：三处 downcast 与一次地址翻译（→3.3、3.4）

`init_clock_and_interrupts` 里发生两次实例化，`boot_init_timer` 里第三次。三个 `new` 各做一次 downcast，期望的具体类型不同：

| 消费点 | trait | 期望的具体类型（三架构） | 搬运的字段 |
|---|---|---|---|
| `init_clock_and_interrupts` | `InterruptRouter` | x86-64 `ApicDesc` / aarch64 `Gicv3Desc` / riscv64 `PlicDesc` | 基址与线数 |
| `init_clock_and_interrupts` | `ArchInit` | 各架构的 arch-misc 参数（`&ArchMiscDesc` 直接传，不 downcast） | x86-64 的 `acpi_tables`（`os/arch/src/x86_64/arch_init.rs:struct X86_64ArchInit` 收下，字段标 `#[allow(dead_code)]`，当前无消费者） |
| `boot_init_timer` | `ClockArch` | x86-64 `PitDesc` / aarch64 `ArmGenericTimerDesc` / riscv64 `ClintDesc` | 频率与计数器地址 |

类型不匹配时是 `expect` panic，错误信息里带本架构期望的类型名（如 `"Riscv64ClockArch::new: expected ClintDesc"`）。

aarch64 的中断控制器多一步地址翻译：

```rust
// os/plat/src/arm64/interrupt.rs:fn new —— 节选（注释有删节：外部记录编号改写为不带 § 的形式）
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let gicv3 = desc.as_any()
            .downcast_ref::<Gicv3Desc>()
            .expect("AArch64InterruptController::new: expected Gicv3Desc");
        // NK4-C 记录里的编号 1.114 复核(SF1)：先判零再翻译——descriptor 缺 GICD/GICR base
        // （物理 `0`，ACPI/UEFI 表解析失败后结构体零初始化的最常见故障形态）
        // 时给精准诊断，避免被 `mmio_translate` 的越界 const-assert 抢先 panic
        // 成泛化的「address outside the mapped MMIO window」。
        assert!(
            gicv3.gicd_base != 0,
            "AArch64InterruptController::new: descriptor did not provide GICD base"
        );
        assert!(
            gicv3.gicr_base != 0,
            "AArch64InterruptController::new: descriptor did not provide GICR base"
        );
        // 路由到内核高半（TTBR1）MMIO 别名。descriptor 里的
        // `gicd_base`/`gicr_base` 是物理地址；本控制器在 `enable()` 之后才
        // 构造（bsp_finish_booting），高半窗口已装入 bootstrap 根的 TTBR1，
        // 因而 mask/unmask/init 的 MMIO 访问跨切根持久。若仍存物理地址，
        // 首次 `switch_address_space` + `tlbi alle1is` 后就会因 TTBR0 身份
        // 映射被切走而同步异常。
        Self {
            gicd_base: minix_platform::arch::aarch64::mmio_translate(gicv3.gicd_base as u64)
                as usize,
            gicr_base: minix_platform::arch::aarch64::mmio_translate(gicv3.gicr_base as u64)
                as usize,
            nr_irqs: (gicv3.nr_irqs as usize).min(NR_IRQ_VECTORS),
        }
    }
```

描述符里的地址在三个架构上不是同一种地址：

| 架构 | 描述符里的地址 | 实例字段里的地址 | 是否需要翻译 |
|---|---|---|---|
| x86-64 | 物理地址（LAPIC/IOAPIC 的 MMIO 基址） | 同值 | 否（恒等映射在低 4GB，且 x86 侧不做切根） |
| aarch64 | 物理地址（GICD/GICR） | 内核高半（TTBR1）别名 | 是（`mmio_translate`） |
| riscv64 | 物理地址（PLIC/CLINT） | 同值 | 否 |

两个零基址断言排在 `mmio_translate` 之前，理由是"描述符缺基址"（物理 0，解析失败后结构体零初始化的常见形态）要给精准诊断，否则会被 `mmio_translate` 的越界 const 断言抢先 panic 成一条泛化的"地址在映射窗口之外"（`os/libs/minix-platform/src/arch/aarch64.rs:fn mmio_translate` 的断言信息是 `"mmio_translate: address outside the mapped MMIO window"`）。

省掉翻译会坏在哪：aarch64 在 boot 期靠 TTBR0 的身份映射（低半）访问设备；第一次 `switch_address_space` 换成进程根并执行 `tlbi alle1is` 之后，低半身份条目被冲刷，此后任何一次走该地址的 MMIO 访问都触发同步异常。上面那段注释写明这是实测故障（启动死在第一次 `set_active_root` 上）。翻译发生在 `new()` 里，为的是在"切根之前"就把地址换成跨根持久的别名。

描述符覆盖的范围有一条边界：PLIC 的线数与定时器无关。riscv64 的 S 模式定时器是核本地中断、不经平台级控制器，因此它的分发身份是保留伪向量 0：

```rust
// os/plat/src/riscv64/interrupt.rs:const TIMER_IRQ —— 逐字引用（注释保留）
/// The boot clock source's dispatch identity: pseudo-vector 0.
///
/// The S-mode timer is a CPU-local interrupt — it fires when `mtime`
/// crosses `mtimecmp` and is gated by `sie.STIE`; it never crosses the
/// PLIC, so it has no controller source number. Vector 0 is reserved as
/// the timer's dispatch identity (the future trap entry maps
/// `scause == SupervisorTimer` to it), and the PLIC mask/unmask paths
/// already treat vector 0 as "no controller line" — a reserved pseudo
/// vector, not an accident. The module-local gate lives in
/// `TimerIrqGate` (`sie.STIE`).
pub const TIMER_IRQ: IrqVector = IrqVector::new(0);
```

描述符只覆盖"平台级控制器能报告的那部分线"。定时器有它自己的分发身份，不占 PLIC 的线号。

### 4.9 拓扑的消费：SMP 状态、GS 槽位与 GET_MACHINE（→3.4）

`cpu_topology()` 的产物有三处消费者。第一处是 SMP 状态的构造：

```rust
// os/kernel/src/lib.rs:fn init_smp_state —— 节选（只保留到第一个 `unsafe` 块结束；函数余下 9 行是 BSP 的 ptproc 槽位安装）
pub fn init_smp_state() {
    use minix_platform::{platform_desc, PlatformDesc};
    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        let topo = platform_desc().cpu_topology();
        *SMP_STATE.get() = Some(smp::SmpState::with_ncpus(
            topo.nr_cpus.max(1),
            crate::proc::CpuId::new_unchecked(topo.bsp_id),
        ));
    }
```

第二处是 BSP 的 GS 槽位编程：

```rust
// os/kernel/src/lib.rs:fn arch_boot_impl —— 节选（注释保留）
    // S-6.1 (D-40 identity anchor): program the BSP's GS area so
    // `current_cpu_id()` works on the BSP too. The BSP's logical id comes
    // from the topology match (MADT/DTB order does not guarantee slot 0).
    // Ordering contract: kmain runs `init_from_kinfo` (Phase A.5) before
    // this phase — the topology read below panics otherwise (Frozen::get
    // before freeze).
    #[cfg(target_arch = "x86_64")]
    {
        let topo = minix_platform::platform_desc().cpu_topology();
        let bsp_logical = (0..topo.nr_cpus as usize)
            .find(|&i| topo.cpus[i].hw_id == topo.bsp_id as u64)
            .unwrap_or(0) as u32;
        minix_arch::x86_64::trap_stub::program_gs(bsp_logical, kernel_info.kern_stack_top);
    }
```

注释里"槽位顺序不保证"这条正是 `bsp_id` 成为必需字段的原因。第三处是发给用户态的 `GET_MACHINE`：

```rust
// os/kernel/src/misc.rs:fn getinfo_machine —— 逐字引用（函数上方的文档注释省略）
fn getinfo_machine(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    val_ptr: u64,
    val_len: i32,
) -> KcallResult {
    let smp = unsafe { crate::try_smp_state() };
    let machine = MachineStruct {
        processors_count: smp.as_ref().map(|s| s.ncpus()).unwrap_or(1),
        bsp_id: smp.as_ref().map(|s| s.bsp_cpu_id().raw()).unwrap_or(0),
        ..Default::default()
    };
    copy_struct_to_caller(caller_nr, proc_table, &machine, val_ptr, val_len)
}
```

六个字段里只填两个，其余用 `..Default::default()`（全零）。用户态消费方在 SCHED：

```rust
// os/servers/sched/src/sef.rs:struct MachineInfo —— 逐字引用（注释保留）
/// Machine news (`struct machine`, `type.h:123-124`).
///
/// Only the two numbers SCHED consumes travel here; the rest of the
/// struct (`padding`/`apic_enabled`/`acpi_rsdp`/`board_id`) is kernel
/// business SCHED never reads.
pub struct MachineInfo {
    /// How many CPUs are available. C: `processors_count` — type.h:123.
    pub processors_count: u32,
    /// The bootstrap CPU. C: `bsp_id` — type.h:124.
    pub bsp_id: u32,
}
```

`program_gs` 直接用槽位 0 当 BSP 会坏在哪：MADT/DTB 的 CPU 记录顺序由固件决定（注释写明"不保证槽 0 是 BSP"），在一台把非引导 CPU 记录排在前面的机器上，`gs:0x10` 会被编程成**另一颗 CPU**的逻辑号；`current_cpu_id()` 随后在这颗 CPU 上返回错误身份，per-CPU 数据（`ptproc` 槽、时钟状态）全部落到错槽位，而系统不报错。

### 4.10 与 C 的对照：struct machine 六字段的去向（→2.8）

C 的发现结果发布结构是 `struct machine`（六字段，§2.8）。Rust 侧的对应物是 `GET_MACHINE` 的内核调用返回结构：

```rust
// os/kernel/src/misc.rs:struct MachineStruct —— 逐字引用（注释保留）
/// C: `struct machine` — type.h:122-131
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MachineStruct {
    /// C: `processors_count`
    processors_count: u32,
    /// C: `bsp_id`
    bsp_id: u32,
    /// C: `padding`
    padding: i32,
    /// C: `apic_enabled`
    apic_enabled: i32,
    /// C: `acpi_rsdp` — phys_bytes (u64)
    acpi_rsdp: u64,
    /// C: `board_id`
    board_id: u32,
}
```

逐字段去向：

| 字段 | C 的写入者 | C 的读者 | Rust 侧处置 | 依据 |
|---|---|---|---|---|
| `processors_count` | `kmain`（`ncpus` 或 1） | 用户态（`GET_MACHINE`） | **改型**：来源从"全局 `ncpus`"改为 `cpu_topology().nr_cpus`，经 `SmpState` 中转 | `os/kernel/src/lib.rs:fn init_smp_state`、`os/kernel/src/misc.rs:fn getinfo_machine` |
| `bsp_id` | `kmain`（`bsp_cpu_id` 或 0） | 用户态 | **改型**：来源从常量改为 `cpu_topology().bsp_id`（发现出来的值），并经 `SmpState` 中转 | 同上；`os/kernel/src/lib.rs:fn arch_boot_impl` 的 `program_gs` 用它定位逻辑槽位 |
| `padding` | 无人（注释 "used to be protected"） | 无人 | **保留位置、淘汰语义**：仍占布局里的位置、恒为 0 | `os/kernel/src/misc.rs:struct MachineStruct` |
| `apic_enabled` | `apic_single_cpu_init()` / `smp_init()` | 用户态 | **淘汰**：Rust 没有"用不用 APIC"这条分支（APIC/GIC/PLIC 是唯一路径），字段恒 0 | `os/libs/minix-platform/src/arch/*.rs`（无对应概念） |
| `acpi_rsdp` | `get_acpi_rsdp()` | 内核内 `detect_ioapics()`；用户态 ACPI 驱动 | **改型**：用户态这一份不再发布（`MachineStruct.acpi_rsdp` 恒 0）；内核侧地址仍随描述符发布——`ArchMiscDesc.acpi_tables` 在 `os/libs/minix-platform/src/acpi.rs:fn parse` 里填 `Some(rsdp_phys)`，由 `os/arch/src/x86_64/arch_init.rs:struct X86_64ArchInit` 取走，字段标 `#[allow(dead_code)]`、当前无消费者；"来自哪种格式"由 `source()` 承担 | `os/libs/minix-platform/src/acpi.rs:fn parse`、`os/arch/src/x86_64/arch_init.rs:struct X86_64ArchInit`、`os/kernel/src/misc.rs:struct MachineStruct` |
| `board_id` | `kmain`（`get_board_id_by_name(env_get(BOARDVARNAME))`） | 内核内各 `BOARD_IS_*` 分支 | **淘汰**：Rust 侧无板卡标识；板级差异由描述符字段承载（x86/aarch64/riscv64 三份 `QemuVirtDesc` 或对应解析器） | `os/libs/minix-platform/src/arch/mod.rs` |

C 行为到 Rust 重表达的四类汇总：

| 类别 | 项 |
|---|---|
| 保留 | 四组硬件参数的概念划分；"发现必须早于消费"这条时序；多来源按顺序尝试（C 只有一条来源，但 `acpi_get_table_base` 的按签名查找是同一形状） |
| 改型 | 发现结果的发布结构：从 `machine` 的单一字段改为一个描述符（六个方法）；CPU 拓扑从两个数（`ncpus` + `bsp_id`）扩展为带 per-CPU 私有地址的数组；"来源"从隐式变成显式的 `source()` |
| 淘汰 | `padding` / `apic_enabled` / `board_id` 三个字段的语义（位置保留、值恒 0）；C 的"发现跑两次"（Rust 只解析一次，见 §4.1） |
| 新增 | 有序多来源切片 `platform_sources`；不透明 kind 标签；`Frozen` 的冻结协议；`CpuInfo` 的 per-CPU 私有地址；`sswi_setip_base()` 与 `psci_conduit()` 两个可选能力；`MAX_CPUS` 定长数组（`no_std` 无堆） |

`no_std` 约束在本篇落成三条：`minix-platform` crate 是 `#![no_std]`（`os/libs/minix-platform/src/lib.rs`）；解析结果进 `static`（一次性冻结，不释放）；描述符的字段是固定大小——`CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组，`MAX_CPUS = 64`（`os/libs/minix-boot/src/platform.rs:const MAX_CPUS`），这个常量同时决定 `parse_cpu_topology` 与 `parse_madt` 的 CPU 收集上限。

---

## 5. 测试要点

### 5.1 宿主：三个解析器的输入从哪来

解析器的单测不需要固件。ACPI 侧手工在堆缓冲里搭一条 RSDP → XSDT → MADT 字节链：

```rust
// os/libs/minix-platform/src/acpi.rs:fn test_parse_synthetic_acpi —— 节选（布局注释与三张表的放置）
    /// Build a minimal synthetic ACPI table set in memory and parse it.
    ///
    /// This verifies the full RSDP → XSDT → MADT chain works correctly.
    #[test]
    fn test_parse_synthetic_acpi() {
        // Layout (all little-endian):
        //   0x0000: RSDP (36 bytes for rev 2)
        //   0x1000: XSDT (header + 1 entry pointing to MADT)
        //   0x2000: MADT (header + 1 IOAPIC + 1 LAPIC)
        // Use a heap-allocated buffer via `Box` (tests can use alloc).
        extern crate alloc;
        use alloc::boxed::Box;
        let mut buf = Box::new([0u8; 0x3000]);
```

它验证的断言（同一测试函数末尾）：`source()` 是 `Acpi`、`ApicDesc.lapic_base == 0xFEE0_0000`、`ioapic_base == 0xFEC0_0000`、`nr_irqs == 64`、`PitDesc.pit_base_freq == 1_193_182`、`PitDesc.lapic_base == 0xFEE0_0000`、`topo.nr_cpus == 1`、`topo.bsp_id == 0`、`topo.cpus[0].hw_id == 0`。

合成表里的所有内部指针用的是**堆缓冲的虚拟地址**（`buf.as_ptr() as usize` 加偏移），解析器把它们当物理地址直接解引用——这个测试只在宿主上成立（宿主上"物理地址"就是可解引用的用户地址），它证明的是解析逻辑，不证明真机上的地址语义。真机那一段由 §5.2 覆盖。

设备树侧直接 `include_bytes!` 一份真实的 DTB 二进制：

```rust
// os/libs/minix-platform/src/device_tree.rs —— 节选（测试数据与端到端断言）
    static TEST_DTB: &[u8] = include_bytes!("../tests/data/qemu_virt_riscv.dtb");

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_parse_riscv64_qemu_virt_dtb() {
        // Parse the RISC-V QEMU virt DTB. Expect PLIC + CLINT extraction.
        let desc =
            DeviceTreeDesc::from_bytes(TEST_DTB).expect("DTB parse should succeed on riscv64");
        assert_eq!(desc.source(), PlatformSource::DeviceTree);

        // PLIC: base 0x0C00_0000 (from `plic@c000000`).
        let ic = desc.interrupt_controller();
        let plic = ic
            .as_any()
            .downcast_ref::<PlicDesc>()
            .expect("expected PlicDesc");
        assert_eq!(plic.plic_base, 0x0C00_0000, "PLIC base mismatch");
        assert_eq!(plic.context, 1, "S-mode context for hart 0");
```

它验证 PLIC 基址、S 模式 context、CLINT 的 `mtime_addr` / `mtimecmp_base` / `mtimecmp_stride`、频率大于 0、至少一个 CPU 节点。

用自造 DTB 取代真实 DTB 会坏在哪：自造件会按解析器的期望去写属性名，解析器的期望一旦与真实固件不同（例如 `reg` 的 `#address-cells` 形态、`compatible` 的字符串顺序），测试仍然通过。用一份真实 QEMU 产出的 DTB 才能钉住"解析器认的是固件实际写的形状"——这是该文件放在 `tests/data/` 而不是内联生成的原因。

测试计数：

```bash
$ grep -rc '#\[test\]' os/libs/minix-platform --include=*.rs | awk -F: '{s+=$2} END {print s}'
22
```

逐文件分解（口径 = 条，即 `#[test]` 属性的出现次数）：

| 文件 | 条数 |
|---|---|
| `os/libs/minix-platform/src/acpi.rs` | 5 |
| `os/libs/minix-platform/src/device_tree.rs` | 4 |
| `os/libs/minix-platform/src/arch/x86_64.rs` | 4 |
| `os/libs/minix-platform/src/arch/riscv64.rs` | 4 |
| `os/libs/minix-platform/src/arch/aarch64.rs` | 3 |
| `os/libs/minix-platform/src/kind.rs` | 1 |
| `os/libs/minix-platform/src/global.rs` | 1 |
| 合计 | 22 |

这 22 条里覆盖解析逻辑的几条：`acpi.rs` 的 `test_parse_synthetic_acpi`（端到端）、`test_check_gic_madt_decision_table`（aarch64 决策表，八条断言，§4.4 的表就是它的输入输出）、`test_acpi_desc_from_parsed`（描述符构造）、`test_acpi_parse_error_variants`（错误变体互异）、`test_u32_le_from_slice`（字节读取助手）；`device_tree.rs` 的 `test_parse_riscv64_qemu_virt_dtb`（端到端）、`test_from_bytes_bad_magic`、`test_dt_parse_error_variants`、`test_parse_unsupported_arch_returns_error`；`kind.rs` 的 `test_parse_by_kind_unknown_returns_error`。

### 5.2 真机：test-smp-topo 三架构的同一份契约

`test-smp-topo` 有三个变体，走的是与生产内核相同的交接路径：

| 变体 | 来源获取 | 解析路径 | 契约 |
|---|---|---|---|
| `test-smp-topo`（x86_64） | 扫 UEFI 配置表的 `ACPI2_GUID` / `ACPI_GUID` | `PlatformDescSource::new(RSDP, ...)` → `parse_by_kind` → MADT LAPIC | `nr_cpus == 4`；`hw_id` 两两互异；`bsp_id` 在发现集合内 |
| `test-smp-topo-aarch64` | 同上（注释写明 AAVMF 不给 FDT，配置表只有 ACPI2） | `RSDP` → `parse_by_kind` → MADT GICC | 同上 |
| `test-smp-topo-riscv64` | OpenSBI 的 `a1`（非 UEFI 二进制，QEMU 直接 `-kernel` 装载 ELF） | `PlatformDescSource::new(DTB, ...)` → `parse_by_kind` → 设备树 `cpus` 节点 | 同上 |

x86_64 变体的关键段落：

```rust
// os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo/src/main.rs —— 节选（注释保留）
    // 2. Parse through the same handoff path the kernel uses: kind tag →
    //    parser dispatch (`parse_by_kind`), not an ACPI-specific shortcut.
    let source = PlatformDescSource::new(RSDP, PhysBytes(rsdp_phys));
    let desc = match unsafe { parse_by_kind(source) } {
        Ok(d) => d,
        Err(_) => fail("ACPI/MADT parse failed"),
    };
    let topo = desc.cpu_topology();
    ...
    // 3. Contract assertion: QEMU runs with -smp 4.
    if topo.nr_cpus != 4 {
        fail("nr_cpus != 4");
    }
```

它明确不走"ACPI 专用捷径"，而是经过 kind 标签与 `parse_by_kind`——被测路径与生产内核的交接路径相同。

三条契约与一条观察项：

| 断言 | 它排除什么 |
|---|---|
| `nr_cpus == 4` | 记录遍历漏掉或多算 |
| `hw_id` 两两互异 | 解析器把 LAPIC 记录合并、或把同一条记录算两次 |
| `bsp_id` 属于发现集合 | MADT 遍历跳过 BSP 记录、或凭空造 ID |
| 具体 APIC ID 值（`{0,1,2,3}`） | 只打印不断言——QEMU 的 `-smp sockets=…,cores=…` 拓扑可以合法地改变 ID 分配 |

aarch64 变体的头部注释直接写明它走的是 ACPI 而不是 DTB（"the discovery path is ACPI, not DTB"），并记录了实测读数：配置表里两个 DTB GUID 都不在。这与 §1.2 的表一致。

三个变体的构建与运行项在 `os/qemu-tests/run_all.sh`：

```bash
$ rg -n 'test-smp-topo' os/qemu-tests/run_all.sh
82:for pkg in hello-boot test-memmap test-paging-enable test-kernel-map test-higher-half test-protection test-proc-init test-smp-topo test-smp-ap-alive test-timer-irq test-smp-aps test-smp-ipi test-smp-shutdown test-user-trap test-paging-faultloop; do
87:for pkg in hello-boot-aarch64 test-memmap-aarch64 test-paging-enable-aarch64 test-kernel-map-aarch64 test-higher-half-aarch64 test-protection-aarch64 test-smp-topo-aarch64 test-rt-birth-aarch64 test-shutdown-aarch64 test-timer-irq-aarch64; do
92:for pkg in hello-boot-riscv64 test-memmap-riscv64 test-paging-enable-riscv64 test-kernel-map-riscv64 test-higher-half-riscv64 test-protection-riscv64 test-smp-topo-riscv64 test-smp-ipi-riscv64 test-rt-birth-riscv64 test-shutdown-riscv64 test-timer-irq-riscv64; do
115:    run_test "test-smp-topo"           x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-topo.efi"
195:    run_test "test-smp-topo-aarch64"   aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-smp-topo-aarch64.efi"
230:    run_test "test-smp-topo-riscv64"   riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-smp-topo-riscv64"
```

预期输出的关键行（三个变体各自的横幅）：`### test_smp_topo: discovering topology (RSDP → MADT)...`（x86_64）、`### test_smp_topo (aarch64): topology discovery pin (ACPI via S-2b)`、`### test_smp_topo (riscv64): discovering topology (DTB via a1)...`；随后是 `  nr_cpus = 0x0000000000000004`——三个架构的早期控制台都把 `u64` 固定写成 16 位十六进制（`os/plat/src/x86_64/early_console.rs:fn write_hex`，另外两个架构同形），所以个位数也带前导零。接着 x86_64 与 riscv64 变体各打印一行 `  cpu[i] hw_id = 0x…`（aarch64 变体不打印这一组），末行按变体分别是 `### TEST_RESULT: PASS test-smp-topo ###`、`### TEST_RESULT: PASS test-smp-topo-aarch64 ###`、`### TEST_RESULT: PASS test-smp-topo-riscv64 ###`。

宿主单测证明解析逻辑，真机门证明两件宿主测试覆盖不到的事：固件实际写的字节形状，以及 `find_platform_sources` 的 GUID 扫描（§4.2）。

把契约写成"APIC ID 必须是 0,1,2,3"会坏在哪：QEMU 的 `-smp` 拓扑参数可以合法地改变 ID 分配，测试会在机器配置变化时误报失败，而解析器是对的。现测试把这组值降级为打印项，只对"数量、互异、BSP 在内"三条与解析器行为直接相关的事实设断言。

### 5.3 测试缺口与 `&[]` 的口径

三处缺口与现有替代覆盖：

| 缺口 | 影响 | 现有替代覆盖 |
|---|---|---|
| `os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources` 没有单测（该文件只有 1 条 `#[test]`，测的是 `&[]` 夹具） | GUID 扫描的匹配逻辑只靠真机验证 | 10 个测试内核在真机上调用它（`test-smp-aps`、`test-timer-irq`、`test-smp-topo-aarch64`、`test-sysboot` 等），`os/qemu-tests/run_all.sh` 覆盖三架构；`test-smp-topo`（x86_64）自己内联扫描配置表、不经这个函数 |
| `os/libs/minix-platform/src/kind.rs:fn parse_by_kind` 只测了未知 kind 一条 | 两个真实分支的返回类型靠各自解析器的单测间接覆盖 | `test_parse_synthetic_acpi`、`test_parse_riscv64_qemu_virt_dtb` 分别覆盖 `Acpi` 与 `DeviceTree` 变体 |
| `aarch64` 的 `device_tree.rs` 分支（`parse_gic` 等）在本篇检索范围内没有真机覆盖 | aarch64 的真机门走 ACPI（§5.2） | 无：`parse_gic` 是 aarch64 专有代码路径，本篇检索范围内没有任何测试（宿主或真机）触及它——riscv64 变体走的是 `parse_plic` / `parse_clint`，共享的只有 `parse_cpu_topology` 的遍历骨架；`test_check_gic_madt_decision_table` 覆盖 aarch64 的 ACPI 侧决策 |

`platform_sources: &[]` 的分布与口径：

```bash
$ rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l
48
```

口径 = **条**（每一处 `&[]` 表达式算一条）。分布：

| 位置 | 条数 | 性质 |
|---|---|---|
| `os/kernel/src/dm_coverage.rs` | 10 | `#[cfg(test)]` 夹具 |
| `os/arch/src/arch/boot.rs` | 7 | `#[cfg(test)]` 夹具 |
| `os/kernel/src/lib.rs` | 6 | `#[cfg(test)]` 夹具 |
| `os/kernel/src/vm_handoff.rs` / `os/kernel/src/kerninfo.rs` | 各 1 | 夹具 |
| `os/libs/minix-boot/src/kernel_info.rs` | 1 | `KernelInfo` 构造夹具 |
| `os/kernel/tests/boot_integration.rs` | 1 | 集成测试夹具 |
| 21 个极简测试内核（`os/qemu-tests/test-kernels/kernel/bootstrap/` 下，如 `hello-boot` / `test-paging-enable` / `test-protection` 各架构变体） | 各 1 | 这些测试内核在 `main.rs` 里直接构造 `KernelInfo`，不经过 boot-shim 的固件发现阶段 |

判断一处 `&[]` 是正常夹具还是覆盖漏洞的判据：**被测单元是否依赖平台发现**。上述 48 处的被测单元（页表映射、ELF 装载、进程初始化、内核映像布局）都不读平台描述符，所以不需要喂真实 DTB/ACPI；而三个 `test-smp-topo` 变体不在这份名单里——它们自己扫配置表 / 读 `a1` 并构造真实来源，发现链的真机覆盖由它们承担。

给每个用 `&[]` 的夹具都塞一份真实 DTB 会坏在哪：被测单元不读平台描述符，塞进去的字节没有任何断言会碰它，测试数上升而覆盖面不变；更糟的是这些测试会开始依赖"夹具里的 DTB 与解析器实现同步"，解析器一改就集体失败，失败原因与被测单元无关。

一条给后续新增测试的判定准则：新增测试若涉及平台路径，优先让 boot-shim 端的 `find_platform_sources` 真实工作（走配置表扫描），而不是手工传 `&[]`；确实需要隔离被测单元时，用 mock 描述符而不是空切片。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

平台发现的故障分三类，确认手段不同：

| 症状 | 怀疑对象 | 确认手段 | 预期输出关键行 |
|---|---|---|---|
| 硬件参数看起来是 QEMU 的值（例如 aarch64 的 GICD 停在 `0x0800_0000`），机器不是 QEMU | 走了兜底：`platform_sources` 为空或全部解析失败 | 检查 `source()` 的返回值（`os/libs/minix-platform/src/global.rs:fn platform_desc` 返回的 trait 对象上有 `source()`） | `PlatformSource::QemuVirt`——只有这一条能判定走了兜底，兜底路径不打日志（§3.5） |
| 启动期 panic `"Frozen::get before freeze"` | 发现没跑，或跑在消费者之后 | 看调用栈（断言在 `os/libs/minix-types/src/types/cell.rs:fn freeze` 所在的 `impl` 块里 `get`） | 栈里出现 `init_clock_and_interrupts` 或 `boot_init_timer` ⇒ 发现排在它们之后；栈里没有 `init_from_kinfo` ⇒ 根本没调（§4.1） |
| 启动期 panic 带 `"no platform source parsed successfully"` | 发布构建下的兜底分支 | 看 panic 文本与构建档位（`cfg!(debug_assertions)` 为假才可能命中这一支） | `platform::init_from_kinfo: … and not a dev build (no QemuVirt fallback in release)` |
| 启动期 panic 带 `"expected ApicDesc"` / `"expected Gicv3Desc"` / `"expected ClintDesc"` 一类 | 描述符与消费者不匹配（描述符来自另一种架构的解析器，或走了兜底但类型不符） | 看 panic 文本里的类型名与 `source()` | `…::new: expected <具体类型>` |
| aarch64 在第一次切地址空间后同步异常 | 设备 MMIO 地址是物理地址而非高半别名 | 看异常发生点与 `mmio_translate` 的窗口常量 | 故障点在 `switch_address_space` 之后的第一处 MMIO 访问（§4.8） |
| IOAPIC 编程看起来成功但 IPI 发不出去 | LAPIC 基址不对（描述符里的值与实际窗口不一致） | 比对 `ApicDesc.lapic_base` 与固件给出的值（MADT 表头的 Local APIC Address） | 两者不等 ⇒ 该机器把 LAPIC 重定位（§2.5） |

三类故障的判据来源不同，其中第一条只能靠 `source()`——这一点在 §3.5 已说明（兜底路径的函数体是 `let _ = reason;`，没有任何输出）。

riscv64 有一条专属边界：定时器不在 PLIC 的线里。若诊断"定时器中断收不到"时去查 PLIC 的线号与描述符的 `nr_irqs`，方向是错的——S 模式定时器是核本地中断，它的分发身份是保留伪向量 0（§4.8 的 `const TIMER_IRQ`），门控在 `sie.STIE`。

把兜底路径改成打印日志会坏在哪：它会调用内核的日志设施，而 `init_from_kinfo` 位于 Phase A.5、早于 `kerninfo` 页建立（`os/kernel/src/lib.rs:fn arch_boot_impl` 的 Phase B.5 才是 `kerninfo::init_kerninfo`）。在控制台尚未就绪时打印要么落到早期控制台的硬编码通道（与"发现失败"这件事无关的模块），要么静默丢失。当前选择是把这条信息留给 `source()`，让"要不要报告"由已经就绪的调用方决定。

### 6.2 可复制命令与预期输出

```bash
# 1) global.rs 里按变体分支的条数（口径 = 条，不含构造点）
$ rg -o 'Self::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/global.rs | wc -l
25

# 2) 变体构造点（口径 = 条，带变体名的构造表达式）
$ rg -c 'PlatformDescEnum::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/kind.rs os/libs/minix-platform/src/global.rs
os/libs/minix-platform/src/global.rs:2
os/libs/minix-platform/src/kind.rs:4

# 3) minix-platform 的单测总数与逐文件分布
$ grep -rc '#\[test\]' os/libs/minix-platform --include=*.rs | awk -F: '{s+=$2} END {print s}'
22

# 4) platform_sources 空切片的总条数与分布
$ rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l
48
$ rg -c 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | sort -t: -k2 -rn
os/kernel/src/dm_coverage.rs:10
os/arch/src/arch/boot.rs:7
os/kernel/src/lib.rs:6
...

# 5) C 侧 acpi_init 的调用点（三个点；默认配置下只有 arch_init 里那一次会执行，见 §2.7）
$ rg -n 'acpi_init\(\)' minix3/minix/kernel/
minix3/minix/kernel/arch/i386/apic.c:1141:	acpi_init();
minix3/minix/kernel/arch/i386/arch_smp.c:315:	acpi_init();
minix3/minix/kernel/arch/i386/arch_system.c:264:	acpi_init();

# 6) MADT 表头字段的读者（LAPIC 基址那一格）
$ rg -n 'local_apic' minix3/minix/kernel/arch/i386/
minix3/minix/kernel/arch/i386/acpi.h:99:	u32_t			local_apic_address;
```

第 1 条与第 2 条的口径必须分开看：第 1 条用 `rg -o | wc -l` 数**匹配条数**；第 2 条用 `rg -c` 数**含匹配的行数**，注释里的分派表也占行——`kind.rs` 那 4 行里有 2 行是文档注释（§4.3 已注明），代码里的构造表达式只有 2 条。第 1 条的 25 是 `Self::…` 匹配的条数：八个转发方法的 22 条加 `Debug` 实现的三条（§3.2 的分解表即此口径，不含变体构造点）。`psci_conduit()` 的 `_ => None` 兜底臂两条命令都数不到（它不是一个 `Self::` 模式），它的分布见 §4.7。

第 6 条是 §2.5 那条结论的复核手段：输出只有一行声明，没有任何读取点。

---

## 7. 参见

文档：

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 启动信息的来源与内存记账；`KernelInfo.platform_sources` 字段的存放；boot-shim 与内核的 crate 依赖方向
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 高半地址空间与恒等映射（DTB 物理地址可被直接解引用的前提）
- [03-kmain-cstart.md](03-kmain-cstart.md) — 保护结构的建立；`init_protection` 与 `init_clock_and_interrupts` 的先后
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断控制器的初始化（§4.1 是描述符消费的逐步走查；§4.10 的全局表列出 `PLATFORM` 的写入点与访问器）
- SMP 篇 — `CpuTopology` 的完整消费者（AP 拉起）

C 源码：

- `minix3/minix/kernel/arch/i386/acpi.c` — `acpi_init` / `get_acpi_rsdp` / `acpi_read_sdt_at` / `acpi_madt_get_typed_item` / `acpi_get_ioapic_next` / `acpi_get_lapic_next` / `acpi_get_ioapics`（`apic.c`）
- `minix3/minix/kernel/arch/i386/arch_system.c` — `arch_init`（发现与 APIC 初始化的相邻两步）、`ser_init`
- `minix3/minix/kernel/main.c` — `cstart`（调用序）、`kmain`（板号赋值）
- `minix3/minix/kernel/arch/i386/apic.c` — `apic_single_cpu_init` / `detect_ioapics` / `lapic_enable_in_msr`
- `minix3/minix/kernel/arch/i386/arch_smp.c` — `smp_init` / `discover_cpus`
- `minix3/minix/kernel/arch/i386/memory.c` — `platform_tbl_ptr` / `platform_tbl_checksum_ok`
- `minix3/minix/kernel/arch/earm/arch_system.c` / `bsp/ti/omap_init.c` / `bsp/ti/omap_intr.c` — ARM 端口的板级常量与板号分支
- `minix3/minix/include/minix/type.h`（`struct machine`）、`minix3/minix/include/minix/board.h`（板号位域）、`minix3/minix/kernel/usermapped_data.c`

Rust 落点：

- `os/libs/minix-boot/src/platform.rs` — `PlatformDesc` 与三个子 trait、`PlatformDescKind` / `PlatformDescSource` / `DTB` / `RSDP` / `CpuTopology` / `CpuInfo` / `MAX_CPUS`
- `os/libs/minix-boot/src/kernel_info.rs` — `KernelInfo.platform_sources`
- `os/libs/minix-platform/src/kind.rs` — `parse_by_kind` 与 kind 常量的 re-export
- `os/libs/minix-platform/src/acpi.rs` / `device_tree.rs` — 两个解析器
- `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` — 品牌 struct 与三份 `QemuVirtDesc`
- `os/libs/minix-platform/src/global.rs` — `PLATFORM` / `PlatformContext` / `PlatformDescEnum` / `init_from_kinfo` / `platform_desc`
- `os/libs/minix-types/src/types/cell.rs` — `Frozen`
- `os/kernel/src/lib.rs` — `arch_boot_impl` 的 Phase A.5、`init_clock_and_interrupts`、`init_smp_state`
- `os/kernel/src/misc.rs` — `MachineStruct` / `getinfo_machine`
- `os/plat/src/{x86_64,arm64,riscv64}/interrupt.rs` / `os/arch/src/{x86_64,arm64,riscv64}/clock.rs` — 消费侧的 `new(desc)`
- `os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo*/` — 三架构的真机门

术语（首次出现处已作行内注解，此处给全称便于回查）：

| 术语 | 全称 | 一句话定位 |
|---|---|---|
| MMIO | Memory-Mapped I/O | 设备寄存器被映射到物理地址空间，CPU 用普通 load/store 访问 |
| ACPI | Advanced Configuration and Power Interface | x86 世界主导的硬件配置与电源管理标准，本仓只用它的硬件描述部分 |
| RSDP | Root System Description Pointer | ACPI 表链的入口，签名 `"RSD PTR "` |
| RSDT / XSDT | Root / Extended System Description Table | ACPI 的多表索引层（32 位 / 64 位表项） |
| MADT | Multiple APIC Description Table | 描述中断控制器与 CPU 的记录表（签名 `"APIC"`） |
| DTB | Device Tree Blob | 设备树的二进制形式；规范名 FDT（Flattened Device Tree），本仓代码统一用 DTB（`os/libs/minix-boot/src/platform.rs:const DTB`） |
| CLINT | Core Local Interruptor | RISC-V 的核本地中断与计时设备（`mtime` / `mtimecmp` / `msip`） |
| ACLINT | Advanced Core Local Interruptor | CLINT 的后续规范，把 MSWI / SSWI / MTIMER 拆成独立设备 |
| PLIC | Platform-Level Interrupt Controller | RISC-V 的平台级外部中断控制器 |
| LAPIC / IOAPIC | Local / I/O Advanced Programmable Interrupt Controller | x86 的本地中断控制器与外部中断控制器 |
| GICv3 | Generic Interrupt Controller v3 | ARM 的中断控制器，含 Distributor（GICD）与 Redistributor（GICR） |
| PIT | Programmable Interval Timer | x86 的 8254 定时器（I/O 端口 `0x40`/`0x43`） |
| SBI / OpenSBI | Supervisor Binary Interface / Open Source SBI | RISC-V 的 M 模式固件给 S 模式内核的调用接口 / 它的开源实现 |
| PSCI | Power State Coordination Interface | ARM 的电源与 CPU 启动接口（本仓从设备树读调用通道） |
| QEMU virt | QEMU Generic Virtual Platform | 本仓三架构共用的测试目标机器，无特定真实硬件对应 |

两个名字指向同一件事的地方只有一处：DTB 与 FDT。术语表把两个名字都列上，是因为按 FDT 去源码里搜不到定义处——`const DTB` 的文档注释里才提到 FDT 这个名字。
