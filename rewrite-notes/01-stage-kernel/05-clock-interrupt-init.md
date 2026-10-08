# 05 · 时钟与中断初始化：让时间开始流动，同时先别接收中断

> **C 真值源**：`minix3/minix/kernel/clock.c`、`minix3/minix/kernel/arch/i386/i8259.c`、`minix3/minix/kernel/arch/i386/arch_system.c`、`minix3/minix/kernel/arch/i386/arch_clock.c`
> **Rust 实现**：`os/kernel/src/lib.rs`（`init_clock_and_interrupts`、`boot_init_timer`）、`os/kernel/src/{clock.rs,irq_manager.rs}`、`os/plat/src/interrupt.rs:trait InterruptRouter`、`os/arch/src/arch/clock.rs:trait ClockArch`、`os/arch/src/*/timer_irq_gate.rs`

03 篇结束时，异常向量已经能进来；04 篇结束时，内核知道自己跑在哪块板子上、控制器寄存器在哪。这一篇补上最后两块：**时间从哪儿来**，以及**外设的事件怎么被接进来**。它们合在一起决定一件事——内核从此能被打断。

---

## 1. 概述

### 1.1 两类事件，一套入口

处理器被"打断"只有两种原因，混在一起谈最容易出错：

| 类型 | 谁触发 | 什么时候发生 | 能不能重试 |
|---|---|---|---|
| 同步（异常） | 正在执行的那条指令本身 | 执行到那条指令时 | 多数可以（缺页处理完重跑原指令） |
| 异步（中断） | 处理器外部的设备 | 与当前指令无关，随时到达 | 不可以（设备状态已经变了） |

03 篇建的向量表两类都收：异常向量按号直达，外设中断要先经过一个**控制器**翻译。

为什么要多一个控制器：处理器只有有限的几根中断输入线，而设备有几十个。控制器做的是转发与优先级仲裁——记住哪台设备报过、把没用的挡住、按优先级排队、告诉内核该走哪个向量。没有它，内核只能轮流去问每台设备"你有没有事"（轮询），这在引导期也许能用，在运行期是不可接受的浪费。后面两节的屏蔽与解屏蔽动作，管的都是这台转发设备的开关：屏蔽掉一位，就是让某个来源的事件报不上来。这个分工不是实现细节，而是本篇全部顺序问题的来源——异常可以预期、可以屏蔽在指令上；中断不可预期，所以它的接收必须有一个**明确开启的时刻**。

### 1.2 需要三样东西，不是一样

要"能被时间打断"，必须同时成立：

1. **一个周期性事件的来源**：某个计数器到点就发一次信号（本地高级可编程中断控制器定时器、泛型定时器、`mtimecmp`）。
2. **一条把该信号送到内核的通路**：控制器不屏蔽它、向量表里有对应的入口。
3. **一个知道"现在几点"的记账处**：内核里那份随每次滴答推进的状态。

这三样分属不同模块，**任何一样先就位都不算就绪**。顺序与"谁等谁"是本章要讲的东西。

### 1.3 机制图

```
① 时钟软件状态建立          （记账处：不碰硬件）
② 中断控制器初始化          （通路：配置完就把所有 IRQ 挡住）
③ 架构特定初始化            （依赖①的频率值与②的稳定状态）
④ ……引导继续（进程表、服务起来）
⑤ 定时器编程 + 挂处理函数 + 打开接收门   （三件事一次做完，位置在引导末端）
     ↓
   时间开始流动，事件开始进来
```

图上 ② 与 ⑤ 各有一处会被读反的地方：② 配完控制器**同时**把所有中断挡掉了（§2.2）；⑤ 的三件事里，"编程硬件"和"打开门"必须是两步（§4.2、§4.3）。

### 1.4 范围与可以跳过的内容

- **滴答进来之后做什么**（调度决策、虚拟定时器到期、时间片记账）：属时钟与定时器篇。本篇只到"事件开始进来"。
- **每种控制器的寄存器编程细节**（本地控制器的本地向量表、GIC 的 redistributor、PLIC 的优先级阈值）：属驱动与中断深入篇。本篇只讲为什么"配完是挡住的"。
- **关机与重启路径**：另一篇。

---

## 2. C 源码分析

### 2.1 `init_clock` 只清软件状态

`minix3/minix/kernel/clock.c:init_clock`（函数体全文）：

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

把它叫 `init_clock` 会让人以为"时钟初始化完了"。逐行看，函数里**没有一条端口写、没有一次寄存器访问**：两次清零加一次取值。

- 频率是从启动环境变量读的（`env_get("hz")`），带一个范围检查（小于 2 或大于 50000 都退回默认），默认值是 `DEFAULT_HZ`。
- 硬件定时器在这里完全没被碰过。真正给定时器编程的是 `clock.c:boot_cpu_init_timer`，而它被 `bsp_finish_booting` 调用——**在引导的最后阶段**，不是这里。

这就产生一个有用的区分：**"内核知道时间怎么记" 早于 "机器开始产生时间"**。中间隔了整个引导过程（进程表建立、服务装载）。为什么可以这样隔？因为产生时间的门还没开（§2.2 会看到它是如何被挡住的）。

`main.c:cstart` 里 `init_clock()` 排在 `intr_init()` 之前，而这两者之间并没有硬件依赖——真正不能颠倒的是 `arch_init()` 必须在 `intr_init()` 之后：它走本地高级可编程中断控制器路径，要用 `kclockinfo.hz` 当分频基数，也要控制器已经建立好稳定状态。

### 2.2 控制器初始化完成的那一刻，所有中断是被挡住的

`minix3/minix/kernel/arch/i386/i8259.c:intr_init`（函数体全文，8259 可编程中断控制器版本）：

```c
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

先读注释第二行：**"finishing with all interrupts disabled"**——函数的名字说的是"初始化"，它承诺的结尾状态是"全部禁用"。光看符号名读不出这句话，得把每个符号换成实数。数值定义分在两个文件：`ICW1_AT`、`ICW4_*` 在 `minix3/minix/kernel/arch/i386/i8259.c:13-21`，端口号、向量基址、级联号在 `minix3/minix/include/arch/i386/include/interrupt.h:9-18,41`。

| 次序 | 写到哪个端口 | 写入的实数 | 源码里的符号 |
|---|---|---|---|
| ICW1 | `0x20`（主片命令口） | `0x11` | `ICW1_AT` |
| ICW2 | `0x21` | `0x50` | `IRQ0_VECTOR` |
| ICW3 | `0x21` | `0x04`（= `1 << 2`） | `(1 << CASCADE_IRQ)` |
| ICW4 | `0x21` | `0x05`；自动应答时 `0x07` | `ICW4_AT_MASTER` / `ICW4_AT_AEOI_MASTER` |
| OCW1 | `0x21` | **`0xFB`**（= `~(1 << 2)`） | `~(1 << CASCADE_IRQ)` |
| ICW1 | `0xA0`（从片命令口） | `0x11` | `ICW1_AT` |
| ICW2 | `0xA1` | `0x70` | `IRQ8_VECTOR` |
| ICW3 | `0xA1` | **`0x02`** | `CASCADE_IRQ` |
| ICW4 | `0xA1` | `0x01`；自动应答时 `0x03` | `ICW4_AT_SLAVE` / `ICW4_AT_AEOI_SLAVE` |
| OCW1 | `0xA1` | **`0xFF`**（`~0` 截成一个字节） | `~0` |

十次写操作只落在四个端口上：主片 `0x20`/`0x21`，从片 `0xA0`/`0xA1`。下面三件事中，端口复用（第一件）与 ICW1 的位差（第三件）只能从实数逼出；第二件在源码里已经写成两种语法（`(1 << CASCADE_IRQ)` 与 `CASCADE_IRQ`），展开成 `0x04` 与 `0x02` 是为了看清它们不是同一个意思。

**一、`0x21` 这个端口在四行里换了解释。** ICW2、ICW3、ICW4 都写到 `0x21`，最后一条写 `0x21` 时它才是屏蔽寄存器——源码在那一行的尾注写的正是 `/* IRQ 0-7 mask */`。屏蔽口的注释也说明了位的方向：`setting bits in this port disables ints`（`interrupt.h:10`），所以想放开哪根 IRQ，就把那一位写成 0。于是初始化命令字是一串**按顺序喂给同一个端口的四步**：端口不看数据是什么，只看"这是收到 ICW1 之后的第几个字"。这就是那十几行 `outb` 之间一次都不能颠倒、也不能插入别的写操作的原因——颠倒的不是执行次序，是同一端口对下一个字的解释。

**二、主片的 `0x04` 与从片的 `0x02` 同出一个宏，但一个是位图、一个是编号。** 主片的 ICW3 回答"哪几根线上挂着从片"，要把级联号变成一位：`1 << 2` = `0b100` = `0x04`；从片的 ICW3 回答"我挂在哪根线上"，直接报号：`2` = `0x02`。同一个 `CASCADE_IRQ` 在两个位置上是两种语法。写成对称的（两边都 `1 << CASCADE_IRQ`，或两边都 `CASCADE_IRQ`）不会报错，但硬件会按另一种意思解释它，后果是从片整组事件永远进不来。

同理，`0xFB` 与 `0xFF` 的差别就是级联线自己：`0xFB` = `0b1111_1011`，只有 bit 2 是 0，其余 7 根全挡；从片是 `0xFF` 全挡。**为什么主片要留一级不屏蔽？** 因为从片的事件只能靠这根线转上来；连它一起屏蔽，从片上那 8 根线就永远没有机会被报出。屏蔽策略本身要留一条通路，这是"全关"不等于"关全部位"的一例。

**三、`0x11` 的位含义不必查手册，同族常量差分就能逼出来。** `i8259.c:13-15` 并列给了三个 ICW1：

```c
#define ICW1_AT         0x11	/* edge triggered, cascade, need ICW4 */
#define ICW1_PC         0x13	/* edge triggered, no cascade, need ICW4 */
#define ICW1_PS         0x19	/* level triggered, cascade, need ICW4 */
```

`0x11` 与 `0x13` 只差 bit 1，两条注释只差 cascade；`0x11` 与 `0x19` 只差 bit 3，两条注释只差 edge/level。三个值互相做差，两位的含义就各自孤立出来了；剩下的 bit 0 三条注释都写着 need ICW4，bit 4（`0x10`）三条都有、注释里没有对应词，本篇不解释它。用同一套读法看 ICW4：主片 `0x05` 比从片 `0x01` 多一个 bit 2，而 `i8259.c:16-17` 两条尾注是同一句 `not SFNM`——注释与数值在这里不合。按 8259 的通用位定义那一位是特殊全嵌套，但本仓源码没有给出可依据的说法，**本篇只登记"差一位"这个事实，不裁定哪侧注释过时**（`[待验证]`）。这不影响本章的顺序结论：那一位是什么含义，末尾两条 mask 造成的"配好了但门是关的"都成立。

两条 mask 写在函数末尾，于是**"初始化完成"与"开始接收 IRQ"是两个状态**：完成之后，通路是配好的、门是关的。

这个"关着"的状态对后面几节很关键。`cstart` 里紧跟着的 `arch_init()`（`minix3/minix/kernel/arch/i386/arch_system.c:arch_init`；另有 ARM 端口的一份同名实现 `minix3/minix/kernel/arch/earm/arch_system.c:arch_init`，本句指 i386 那份）会走本地高级可编程中断控制器路径，而此时全部传统中断线仍是屏蔽的——两件事并不矛盾：**架构初始化用控制器，不等于要接收外设事件**。把"配好控制器"读成"事件可以进来"，就会在诊断时找不到方向（§6）。

### 2.3 定时器编程：三段动作，顺序固定

`cstart` 之外的这一小段是真正"让时间开始走"的地方。`minix3/minix/kernel/arch/i386/arch_clock.c` 与 `clock.c:boot_cpu_init_timer` 一起构成三步：

1. **编程事件源**：给定时器算分频、设周期（本地控制器定时器走的是它自己的寄存器）。
2. **登记处理函数**：把"这个 IRQ 进来该执行谁"记进内核的钩子表；登记时有一条规则——**第一个挂上来的处理函数会解掉控制器侧的屏蔽**。
3. **打开器件自己的门**：例如本地控制器侧的使能位。

三段里最容易被忽略的是第 2 段与第 3 段的分工。C 里它们分属两处（登记钩子在 `arch_clock.c`，器件使能在 `init_local_timer`），因此顺序一眼可见。第 4 章会看到重写把三段合进一个函数，而这恰恰引发过一次真实故障。

### 2.4 归纳

C 的形状可以三句话收住：**记账的状态先建（不碰硬件）；通路配好但门先关着；到引导末端才把"编程、登记、开门"三段串起来。**"内核能被时间打断"是最后一步造成的，不是第一步。

---

## 3. Rust 设计决策

本章每节第一句都是上一节里 C 的真实做法或它的语义要求。

### 3.1 频率不再是运行期参数

**C 的 `init_clock` 里频率是读来的**（`env_get("hz")` 带范围检查，§2.1）。重写递过来的启动契约里没有参数字符串可查（01 篇 §4.4 说过这个字段的形状），因此 tick 率改成一个编译期常量：`os/kernel/src/clock.rs` 里的 `DEFAULT_HZ`，值为 100。

这一处必须写成三段的区分，不然很容易说错：**C 是运行期读参数、失败退回架构默认（x86 是 60、ARM 端口是 1000）；Rust 是编译期常量 100**。"tick 率是编译期常量"只对 Rust 成立。这是一次有意的偏离（`[ARCH: K-3]`：60/1000 → 100），并且留了一条回退口——需要与 C 完全一致时，引导阶段可以显式指定 60。为什么选 100：10 毫秒的粒度在响应延迟与切换开销之间平衡，也是常见服务器配置。

两处必须分开看，它们是两回事：**"OS 希望每秒多少个 tick"是策略值（60、1000、100 都是它），"定时器输入时钟是多少"是硬件参数**（x86 的可编程中断定时器输入是固定的高频振荡，要先算分频才能到目标 tick 率；ARM64 读 `CNTFRQ_EL0`、RISC-V 读 `mtime` 频率）。把 100 Hz 与 1.1931816 MHz 当成同一类数字，是这一段最容易犯的理解错误——C 代码里 `init_local_timer` 算的就是从后者到前者的分频。

### 3.2 软件状态与硬件动作分家，分在两个地方

**C 的 `init_clock` 只清状态、硬件留给 `boot_cpu_init_timer`**（§2.1）。重写沿用这个分家，但把它表达成两个类型：`os/kernel/src/clock.rs` 里的 `ClockState`（架构无关的记账），和 `os/arch/src/arch/clock.rs:trait ClockArch`（硬件定时器动作）。控制器那一侧的接口同样把"配好"与"挡住"分列（`os/plat/src/interrupt.rs:trait InterruptRouter`，节选）：

```rust
    fn init(&mut self);

    fn mask_all(&mut self);
```

`init_timer` 为什么要 `cpu_id`？注释与实现给出理由：定时器是**每核一份**的（`mtimecmp` 按 hart 编址、本地控制器定时器是核内寄存器、泛型定时器的比较值也是核内），所以"给哪个核编程"必须是参数，不能靠"当前核"的隐式假设——引导期当前核只有一个，但接口要为多核留好形状。

这条与 03 篇 §3.5 的每 CPU 结构是同一类形状约束。

### 3.3 控制器初始化自带"全屏蔽"，这件事放在接口里

**C 的 `intr_init` 末尾写了两条屏蔽**（§2.2），是函数体的行为。重写把它变成接口的显式约定：`os/plat/src/interrupt.rs:trait InterruptRouter` 同时给出 `fn init`（初始化，实现内部会调用）与 `fn mask_all`（全部屏蔽）。也就是说，"配好了但一条都不收"不是一句注释，而是接口的两条能力，实现必须都提供。

统一成接口的收益在 04 篇立过：控制器种类差异（传统控制器、本地控制器、GIC、PLIC）留在实现里，OS 语义层只问"初始化、屏蔽、解屏蔽"。**这里要防的误读是**：`init` 之后能不能收中断，答案不由名字决定，而由实现是否调了 `mask_all` 决定——读任何一个具体控制器的 `init` 时，这一步要实地看，不能推。

### 3.4 三段的顺序是硬要求，位置也是

**C 的三段（编程、登记、开门）在时间上天然分散**（§2.3）：编程与开门在 `bsp_finish_booting` 一侧，登记在 `arch_clock.c` 一侧，中间还隔着引导其余部分。这种分散反而保住了一个性质：**在登记与开门之前，事件源即使被编程好也不会打进来**（控制器侧还屏蔽着）。

重写在"什么时候做"上比 C 更紧：三段被合进一个函数（`os/kernel/src/lib.rs:fn boot_init_timer`）。合并没有问题，**但顺序不能省**。这一点不是纸面推演，实现注释里留着一次真实的教训：早期版本把"编程事件源"提前到 Phase B，并把"开门"折叠进同一步——结果是**从那一刻起到引导结束之间，存在一个已激活却无人处理、也没登记处理函数的定时器**。症状不是"滴答没来"，而是引导中途出现无主的定时器中断。

于是这一节留下两条要求：**三段各自独立、顺序固定、位置在引导末端**。第 4 章看它们落成什么代码形状。

### 3.5 事件源与投递门是两个不同的开关

**C 的屏蔽是控制器侧的（`intr_init` 的 mask、以及登记钩子时的解屏蔽）**，器件自己还有一层使能（本地控制器的屏蔽位、`sie.STIE`、泛型定时器的使能与屏蔽位）。

重写在接口上把两层分开，各架构自己实现"开门"这件事（`os/arch/src/*/timer_irq_gate.rs`）。三层门是：

| 层 | 谁的开关 | 例子 |
|---|---|---|
| 事件源自己 | 定时器模块 | 本地控制器的屏蔽位；`CNTP_CTL_EL0` 的使能与屏蔽；`sie.STIE` |
| 投递通路 | 中断控制器 | I/O 控制器的某条线；GIC 的使能位；PLIC 的 enable |
| 内核侧记账 | 处理器是否允许异步事件 | 标志寄存器里的中断允许位（在 01 篇的入口代码里被清掉） |

x86-64 上第一层是**没有对应物的**（可编程中断定时器没有器件级屏蔽位），所以那条路径上"开门"只剩控制器侧一层；这一点被明确标成架构差异（`[ARCH: gate-semantics]`）而不是偷偷统一掉。为什么不能统一？因为一旦假设"每个平台都有器件门"，代码就会去写一个 x86 上不存在的位。

---

## 4. 实现详解

本章每节标注它落实 §3 的哪条决策。

### 4.1 引导期这一段做了什么（→3.1、3.2、3.3）

`os/kernel/src/lib.rs:fn init_clock_and_interrupts`（函数体节选，保留全部关键行）：

```rust
    let pd = platform_desc();

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    intr.init();  // mask_all() called internally

    unsafe {
        *IRQ_MANAGER.get() = Some(crate::irq_manager::IrqManager::new(intr));
    }

    // Step 3: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();

    unsafe {
        *CLOCK_STATE.get() = Some(clock);
    }
}
```

四个动作，逐条对上 C：`pd` 来自 04 篇冻结的那份描述；Step 1 只建软件状态（等价 `init_clock`，只是不再读参数）；Step 2 从描述符**构造出控制器实例**再 `init()`（等价 `intr_init(0)`），随后把实例移进全局供后续使用；Step 3 等价 `arch_init()`。

上面这四行里，有两处的先后顺序是有讲究的：

- `intr.init(); // mask_all() called internally`——"初始化包含全屏蔽"这件事写在这里，也要求实现兑现（§3.3）。读一个具体控制器的 `init` 时，这是必须实地核对的一行。
- 时钟状态是**最后**才存进全局的，而它的建立是第一步。中间那段引导里，处理函数还不能读它——因为门还没开（§3.4）。这两行隔着几十行的距离，正是"记账先建、事件后进来"在代码里的形状。

### 4.2 三段各在哪（→3.4）

`os/kernel/src/lib.rs:fn boot_init_timer` 的前半（节选）：

```rust
pub fn boot_init_timer() {
    // Step 6: boot_cpu_init_timer(system_hz)
    // ... this is the ONLY place the boot clock source's hardware is touched, and it
    // mirrors the C three-part sequence exactly:
    //   (a) program the source — C `init_local_timer(freq)`:
    //       PIT divisor on x86_64, CNTP_CVAL (gate kept closed,
    //       CNTP_CTL_EL0 = Enable=0/IMASK=1) on aarch64, mtimecmp
    //       (gate `sie.STIE` untouched) on riscv64.
    //   (b) register the handler — C `register_local_timer_handler`
    //       → `put_irq_handler` (arch_clock.c:190); the Rust
    //       `IrqManager::register_hook` carries the same first-handler
    //       unmask rule (interrupt.c:65) ...
    //   (c) open the module-local gate — ...
    use minix_arch::{ClockArch, CurrentClockArch, CurrentTimerIrqGate, TimerIrqGate};
    let pd = platform_desc();
    let mut clock_arch = CurrentClockArch::new(pd.timer());
    clock_arch.init_timer(crate::clock::DEFAULT_HZ, crate::clock::current_cpuid().raw());
```

注释里的三件事与 §2.3、§3.4 完全对上，并且它把每个架构"(a) 到底写了什么"都列了出来：x86-64 写分频器、ARM64 写比较值而**把器件门保持关闭**（使能位为 0、屏蔽位为 1）、RISC-V 写比较值而**不动 `sie.STIE`**。这一步之后事件源已就绪，但还不会打进来。

(b) 的"第一个处理函数会解掉控制器侧屏蔽"这条规则，与 01 篇 §2.4 是同一种形状：`add_memmap` 自己在登记时把长度截到 `LIMIT`，不要求调用方先检查。这里的解屏蔽行为也写在登记逻辑里，而不是留给调用方自己判断。

(c) 单独一步，才把器件门打开。三段合在一个函数里，顺序由代码顺序表达。

### 4.3 一次编程完成两件事，为什么不行（→3.4）

4.2 的注释里"gate kept closed / 不动 `sie.STIE`"是刻意写成这样的。理由就是 §3.4 那次教训：如果 (a) 顺手把 (c) 也做了，事件源从那一刻起就在产生中断，而 (b) 还没登记处理函数、控制器还屏蔽着——三者不一致。

不一致的具体坏处分两层：若控制器已屏蔽，中断进不来，定时器却一直计数，症状是"某一次解屏蔽之后瞬间来一堆积压的中断"；若控制器未屏蔽（例如在别处已经被解开），症状是引导中途出现无主中断。两种都难归因，因为出问题的那一刻看起来什么都没发生。

RISC-V 那条 `sie.STIE` 不动、等 (c) 才置起，是同一件事的另一种写法。**"编程事件源"与"允许它打断我"是两个动作，混成一步就是这个 bug 的形状**：三架构的 (a) 各自写了什么完全不同，但都把这一刀留在同一个位置。

### 4.4 三架构对照（→3.5）

| 语义 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| 事件源 | 可编程中断定时器分频器 / 本地控制器定时器 | 泛型定时器比较值 | `mtimecmp` |
| 器件门 | 无对应物（`[ARCH: gate-semantics]`） | 使能与屏蔽位 | `sie.STIE` |
| 投递通路 | I/O 控制器某条线 | GIC 的使能 | PLIC 的 enable（本地定时器不走 PLIC） |
| 抽象在哪 | `trait ClockArch` + `os/arch/src/*/timer_irq_gate.rs` | 同 | 同 |

统一的是"三件事各有各的名字"，不是它们的值。x86 那一格"没有对应物"必须留在表里——抹掉它，代码就会去写一个不存在的位。

---

## 5. 测试要点

| 约束 | 守它的东西 | 强度 |
|---|---|---|
| 初始化包含全屏蔽（§3.3） | 各控制器 `init` 内的 `mask_all` + 用假控制器断言屏蔽位随调用翻转的测试（`os/kernel/src/irq_manager.rs`） | 宿主可测 |
| 第一个处理函数解屏蔽（§4.2） | 登记钩子的用例：先登记后屏蔽位变化 | 宿主可测 |
| 三段顺序与器件门位置（§3.4、§4.3） | 无宿主可判定的形式；靠 `boot_init_timer` 的代码顺序与注释 | **只靠顺序** |
| tick 率是编译期常量 100（§3.1） | 常量本身 + 与 C 默认值不同的 `[ARCH]` 标注 | 静态 |

数量与名称由作者现场跑过（命令可复算）：

```bash
ls os/qemu-tests/ | grep -E 'test-timer-irq'      # 真机滴答中断载体，按架构分列
cd os && rg -c '#\[test\]' kernel/src/clock.rs kernel/src/irq_manager.rs
```

只能真机判定的：**事件是否真的进来了**（需要硅片上的定时器与控制器行为）、"引导中途不该有主"这条性质（§4.3）。宿主测试可以证明屏蔽位随调用变化，证明不了"引导期间没有任何定时器中断投递进来"——那是顺序问题，不是状态问题。

**盲区**（如实列出）：§3.4 那条历史故障说明现有测试**没能覆盖**"三段合并过早"这一类错误；本篇没有新增能覆盖它的宿主测试，它仍然只由代码顺序维持。这是本篇披露的最大缺口。

---

## 6. 诊断

先分诊两类完全不同的"没心跳"：

```bash
# 一：引导期这一段做了什么、门在哪一步开
rg -n "ClockState::new|CurrentInterruptController::new|intr\.init\(\)|CurrentArchInit::new|\
init_timer|CurrentTimerIrqGate|STIE|CNTP_CTL" os/kernel/src/lib.rs os/arch/src/*/*.rs

# 二：谁解了屏蔽、定时器中断号是多少
rg -n "fn mask_all|fn register_hook|\.unmask\(|pub const TIMER_IRQ" \
    os/plat/src/interrupt.rs os/kernel/src/irq_manager.rs os/plat/src/lib.rs os/plat/src/*/interrupt.rs
```

命令一实测会打出这些行（下面的行号是 `rg` 当时输的，随代码漂；本仓不手工维护它们）：

```
os/kernel/src/lib.rs:1499:    let mut clock = ClockState::new();
os/kernel/src/lib.rs:1509:    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
os/kernel/src/lib.rs:1510:    intr.init();  // mask_all() called internally
os/kernel/src/lib.rs:1525:    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
os/kernel/src/lib.rs:2697:    clock_arch.init_timer(crate::clock::DEFAULT_HZ, crate::clock::current_cpuid().raw());
os/kernel/src/lib.rs:2721:    <CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
os/kernel/src/lib.rs:922:    boot_init_timer();
```

这张清单读的是**相对位置**，不是行号本身。`init_timer` 与 `enable_timer_irq` 都在 `boot_init_timer` 的函数体里（它从 `os/kernel/src/lib.rs:fn boot_init_timer` 开始），而 `boot_init_timer()` 的调用点排在 `init_clock_and_interrupts()` 之后。于是"编程、登记、开门"整体落在引导后段，控制器初始化那四条（上面的 1499–1525）远在前面——这就是 §4.2 那个顺序在代码里的形状。要怀疑 (a) 与 (c) 被合回一步，就看使能位写法（`STIE`、`CNTP_CTL_EL0` 的 Enable=1）出现在 `init_timer` 的实现里还是 `enable_timer_irq` 里：前者就是 §4.3 那个故障形状又回来了。

命令二里最关键的是这一行：

```
os/kernel/src/irq_manager.rs:437:            self.controller.unmask(irq);
```

它在登记钩子那条路径的函数体内（`fn register_hook_at_slot`）；同文件其余四处 `unmask` 分别属于两个摘钩子路径（`remove_hook`、`remove_hook_by_slot`）、分发收尾（`dispatch_claimed`）与一个显式启用入口（`enable_irq`）。位置差异回答的就是"谁解了屏蔽"：**登记的那条路自己解**，不是调用方替它判断（§4.2）。如果这一行从登记函数里挪出去、改由调用方显式启用，"钩子登记上了但门没开"就是必然结果。

命令二的后半段还顺便给出一个数值事实：`pub const TIMER_IRQ` 三架构各一份，x86_64 与 riscv64 都是 `IrqVector::new(0)`、arm64 是 `IrqVector::new(30)`（`os/plat/src/lib.rs` 按目标架构选其一）。但这三个 0 **不在同一个编号空间里**：x86_64 的 0 是传统控制器上的 0 号线（可编程中断定时器）；riscv64 的 0 不是控制器线号，而是 `os/plat/src/riscv64/interrupt.rs:const TIMER_IRQ` 的文件注释里说清的保留伪向量——S 模式定时器是核本地中断（`mtime` 越过 `mtimecmp` 触发、由 `sie.STIE` 屏蔽），从不经过平台级中断控制器，所以根本没有控制器源号，trap 入口按异常原因把定时器事件归到这一个号上；PLIC 的屏蔽与解屏蔽路径也把 0 当作"无控制器线"。arm64 的 30 则是通用中断控制器的私享中断编号。**同一个数字在三个平台上分属三个完全不同的空间**，这正是 §4.4 那次对照的数值版。只有那个异常原因字段的具体取值属于 RISC-V 特权规范，本仓不裁定（`[待验证]`）。

| 症状 | 首要怀疑对象 | 怎么确认 |
|---|---|---|
| 引导中途莫名出现一次无主中断 | §4.3：编程事件源时顺手开了器件门 | 命令一里看 (a) 是否同时写了使能位 |
| 引导一切正常，但永远没有滴答 | (c) 那一步没执行，或 (b) 没登记上 | 命令一找 `CurrentTimerIrqGate` 是否被调用；命令二看登记路径 |
| 一开始能收，某一刻之后再也收不到 | 屏蔽位被别处改掉；或解屏蔽时该中断线尚未配置 | 命令二的 `unmask` 与控制器实现 |
| 滴答来了但时间不对 | tick 率来源搞混（§3.1）：C 读参数、Rust 是常量 | 看 `DEFAULT_HZ`，以及有没有传 `with_hz` |
| 平台相关，控制器基址不对 | 拿到的是 04 篇的兜底描述 | 问 `source()` 是不是 `QemuVirt` |

**一条最容易混的方向**：控制器"初始化完成"意味着**所有线被屏蔽**（§2.2、§3.3）。所以"外设没反应"的第一步不该怀疑硬件，而该问"谁解了这根线的屏蔽"——正常路径只有 §4.2 的登记逻辑会解它。反过来，若怀疑"中断在乱进"，就顺着 §4.3 去查是不是哪一步提前开了门。

---

## 7. 参见

**C 真值源**

- `minix3/minix/kernel/clock.c` —— `init_clock`（§2.1）、`boot_cpu_init_timer`（§2.3）
- `minix3/minix/kernel/arch/i386/i8259.c` —— `intr_init` 与末尾两条屏蔽（§2.2）
- `minix3/minix/kernel/arch/i386/arch_system.c` —— `arch_init` 与它对频率、控制器状态的依赖（§2.1）
- `minix3/minix/kernel/arch/i386/arch_clock.c` —— 登记处理函数与解屏蔽规则（§2.3）
- `minix3/minix/kernel/main.c` —— `cstart` 里三者的先后、`bsp_finish_booting` 的位置（§2.1、§2.3）
- `minix3/minix/include/arch/i386/include/archconst.h`、`include/arch/earm/include/archconst.h` —— 两个架构默认的 tick 率（§3.1）

**Rust 实现**

- `os/kernel/src/lib.rs:fn init_clock_and_interrupts`、`fn boot_init_timer` —— 本篇两条主线（§4.1、§4.2）
- `os/kernel/src/clock.rs` —— `ClockState` 与 `DEFAULT_HZ`（§3.1、§3.2）
- `os/kernel/src/irq_manager.rs` —— 钩子登记与解屏蔽规则（§4.2）
- `os/plat/src/interrupt.rs:trait InterruptRouter` —— `fn init` 与 `fn mask_all`（§3.3）
- `os/arch/src/arch/clock.rs:trait ClockArch`、`os/arch/src/*/timer_irq_gate.rs` —— 每核定时器与器件门（§3.2、§3.5）
- `os/libs/minix-platform/src/desc.rs` 与 `os/libs/minix-boot/src/platform.rs` —— 定时器与控制器描述符（04 篇的消费端）

**相关主题篇**（尚未生成的只给主题名）

- 01 篇：引导期映射、`cstart` 之前的世界；03 篇：向量表如何建与装载
- 04 篇：控制器基址、拓扑从哪来；本篇所有参数都消费它的产物
- 时钟与定时器篇：滴答进来之后（调度、虚拟定时器、时间记账）
- 多核启动篇：每核定时器如何被逐个建立，以及 `cpu_id` 的真实来源
- 驱动与中断深入篇：各控制器的寄存器编程细节（本篇 §1.4 明确不讲的整块）

**外部规范来源**（引用处均已就地标明）

- Intel 64 与 IA-32 架构手册：本地高级可编程中断控制器的定时器与屏蔽位
- ARM 架构参考手册：泛型定时器的比较值与 `CNTP_CTL_EL0` 的使能/屏蔽语义
- RISC-V 特权规范：`mtimecmp`、`sie.STIE` 与监督器中断投递条件
- 8259 可编程中断控制器的初始化与操作命令字约定（`intr_init` 的 ICW/OCW 序列）
