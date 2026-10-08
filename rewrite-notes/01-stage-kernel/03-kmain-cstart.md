# 03 · cstart 的第一步：三张表，与"填好"和"生效"之间的那一步

> **C 真值源**：`minix3/minix/kernel/main.c:cstart`、`minix3/minix/kernel/arch/i386/protect.c`、`minix3/minix/kernel/arch/earm/protect.c`、`minix3/minix/kernel/proto.h`
> **Rust 实现**：`os/kernel/src/lib.rs:init_protection`、`os/arch/src/arch/protection.rs:trait ProtectionArch`、`os/arch/src/arch/trap_entry.rs:trait TrapEntryArch`、`os/arch/src/x86_64/protection.rs`、`os/kernel/src/trap_dispatch.rs`

01、02 两篇结束时，处理器已经按高地址执行内核代码。但此刻它还不知道"从用户态掉进内核该用哪个栈"、也不知道"某个异常向量进来该执行谁"。这一篇处理内核主流程做的第一件事：把三张表建起来，并搞清楚**表建好了与表生效之间，还差哪几步**。

---

## 1. 概述

### 1.1 内核主流程第一件事为什么是它

`minix3/minix/kernel/main.c:cstart` 的开头（节选）：

```c
void cstart(void)
{
/* Perform system initializations prior to calling main(). Most settings are
 * determined with help of the environment strings passed by MINIX' loader.
 */
  register char *value;				/* value in key=value pair */

  /* low-level initialization */
  prot_init();
```

后面几十行才是解析环境变量、初始化进程表等（那部分归后面的环境变量与进程表篇）。位置本身说明了一切：**`prot_init()` 是 `cstart` 做的第一件事**，比读任何参数都早。

理由要从"如果跳过它会怎样"来看。此刻分页已开、代码在高半区跑，看起来一切正常。但有一次异常发生——比如缺页、比如除零——处理器要查一张表才知道跳转到哪、用什么特权级、要不要换栈。那张表还不存在。

所以顺序不是编排习惯：**任何一条可能引发异常的指令，都排在"表就位"之后更安全**——而内核从 `cstart` 起就在执行这样的指令。

### 1.2 三张表各回答一个问题

| 表 | 回答的问题 | C 里的载体 | 硬件怎么知道它在哪 |
|---|---|---|---|
| 段描述符表 | 给我一个选择子，它对应的基址、限长、权限是什么 | `protect.c` 里的 `gdt` 数组 | 装载一条"把表的位置交给硬件"的指令 |
| 中断描述符表 | 某个向量号进来，执行哪段代码、需要什么特权级才能进 | `protect.c` 里的 `idt` 数组 | 同上 |
| 任务状态段 | 从低特权级掉进高特权级时，用哪个栈、返回到哪 | `protect.c` 里的 `tss` 数组（每 CPU 一份） | 通过表里的一个条目 + 一条装载指令 |

一张表管"我在哪、谁能进"，一张管"进来执行谁"，一张管"进来时踩哪块栈"。缺任何一张，异常处理路径就是断的。

### 1.3 机制图：建表与生效之间

```
① 两张表整体清零
   ▼
② 逐条填条目（段的基址/限长/权限；门的向量号/选择子/特权级；任务状态段的栈与选择子）
   ▼
③ 把表的位置交给硬件（基址寄存器、任务寄存器、选择子重装）
   ▼
④ 用硬件真正执行一遍——此刻任何错误都会当场暴露
   ▼
⑤ 页表重建一次（恒等段此时还不能退休）
   ▼
⑥ 置"已完成"标志
```

图上有两处次序需要解释：④ 为什么要主动把"能用的选择子"换掉（§2.4），以及 ⑤ 为什么都走到这一步了还要再铺一次恒等段（§2.8）。

### 1.4 范围与可以跳过的内容

本篇到"三张表就位、入口机制配好"为止。以下可以安全跳过，不影响本篇结论：

- **异常进来之后做什么**（分发、栈帧形状、系统调用参数语义）：属陷入与系统调用篇。本篇只到"入口地址被填进表、表被装载"。
- **中断控制器怎么配、IRQ 号如何映到向量**：属后面的时钟与中断篇。本篇 2.6 只讲门表本身。
- **辅助处理器的完整启动流程**：属多核篇。本篇只讲它复用了这里哪一段（§2.5、§3.5）。
- **早期 x86 的任务切换机制**：任务状态段在这里只被用作"换栈与返回地址的落脚点"，不用于任务切换。看到 `tss` 这个变量就以为要用硬件做任务切换，是这一节最容易走岔的地方。

---

## 2. C 源码分析

### 2.1 `prot_init` 的骨架

`minix3/minix/kernel/arch/i386/protect.c:prot_init`（节选，保留全部关键行）：

```c
void prot_init(void)
{
  extern char k_boot_stktop;

  if(_cpufeature(_CPUF_I386_SYSENTER))
	minix_feature_flags |= MKF_I386_INTEL_SYSENTER;
  if(_cpufeature(_CPUF_I386_SYSCALL))
	minix_feature_flags |= MKF_I386_AMD_SYSCALL;

  memset(gdt, 0, sizeof(gdt));
  memset(idt, 0, sizeof(idt));

  /* Build GDT, IDT, IDT descriptors. */
  gdt_desc.base = (u32_t) gdt;
  gdt_desc.limit = sizeof(gdt)-1;
  idt_desc.base = (u32_t) idt;
  idt_desc.limit = sizeof(idt)-1;
  tss_init(0, &k_boot_stktop);
```

三件事在最前面，各自有一个理由。

**先问 CPU 特性，再动手。** 两条 `if(_cpufeature(...))` 探测的是"快速系统调用"的两条不同硬件通道（Intel 的 `SYSENTER` 与 AMD 的 `SYSCALL`），探测结果记进一个全局标志位。这说明**系统调用入口机制不是编译期定死的**，而是运行期看机器有什么。为什么这一步要放在 `prot_init` 开头：入口机制的配置本身就属于"表就位"的一部分（§2.5 会看到门表里也有一类专门给系统调用与进程间通信的门）。

**两张表先整体清零。** 表是编译期分配的静态数组（`gdt[GDT_SIZE]`、`idt[IDT_SIZE]`，都按描述符大小对齐）。清零要防的不是脏数据，而是"半填的条目"：描述符是硬件按位解释的结构，一个没写过的条目只要某个位碰巧为 1，就会被当成合法项使用——而它指向的地址是没人准备的。清零之后，"没用到的条目"和"填错的条目"才有区别。

**描述符表的边界写成"表大小减一"。** `gdt_desc.limit = sizeof(gdt)-1;`。基址寄存器里的限长字段是**最后一个可用字节的偏移**，不是条目数也不是字节数。差一在这里是硬件规定；同类"边界写法决定对不对"的例子，本篇 2.4 与 01 篇 2.5 各有一处。

### 2.2 段描述符：两档特权级，和一个故意不可用的条目

接上（`prot_init` 中段，逐字）：

```c
  /* Build GDT */
  init_param_dataseg(&gdt[LDT_INDEX],
    (phys_bytes) 0, 0, INTR_PRIVILEGE); /* unusable LDT */
  gdt[LDT_INDEX].access = PRESENT | LDT;
  init_codeseg(KERN_CS_INDEX, INTR_PRIVILEGE);
  init_dataseg(KERN_DS_INDEX, INTR_PRIVILEGE);
  init_codeseg(USER_CS_INDEX, USER_PRIVILEGE);
  init_dataseg(USER_DS_INDEX, USER_PRIVILEGE);
```

后四行是骨架：内核一段代码段、一段数据段，用户一段代码段、一段数据段，两档特权级由 `INTR_PRIVILEGE` / `USER_PRIVILEGE` 区分（同一文件里 `init_codeseg`、`init_dataseg` 把这两档写进访问字）。也就是说，**内核与用户各自的"能做什么"，在这一刻被写进了表里**——之前（01、02 篇）所有地址翻译工作都还没有涉及"谁能访问什么"。

第一、二行与后面四行不同：它占了一个描述符槽位，却把它填成用不了的形状。源码自己的注释是 "unusable LDT"。

这个组合不是笔误，但也不能替它编一个动机。能核实的是它的效果：一个基址与限长都为零、却标记为存在的描述符，一旦被使用就会因为越界而失败；而"条目全零"的另一种写法会把存在位也清成 0，硬件判它是"不存在"——**两种写法都拦住了使用，但失败的类型与时机不同**。把一个槽位显式建成"用不了但存在"，比留一个未定义的槽位更可预期。至于**为什么非要占住这个索引**（而不是干脆不用它），源码没写，本篇标为 `[待验证]`。

### 2.3 任务状态段：每个 CPU 一份

`minix3/minix/kernel/arch/i386/protect.c:tss_init`（节选）：

```c
int tss_init(unsigned cpu, void * kernel_stack)
{
	struct tss_s * t = &tss[cpu];
	int index = TSS_INDEX(cpu);
	struct segdesc_s *tssgdt;

	tssgdt = &gdt[index];
  
	init_param_dataseg(tssgdt, (phys_bytes) t,
			sizeof(struct tss_s), INTR_PRIVILEGE);
	tssgdt->access = PRESENT | (INTR_PRIVILEGE << DPL_SHIFT) | TSS_TYPE;

	/* Build TSS. */
	memset(t, 0, sizeof(*t));
	t->ds = t->es = t->fs = t->gs = t->ss0 = KERN_DS_SELECTOR;
	t->cs = KERN_CS_SELECTOR;
	t->iobase = sizeof(struct tss_s);	/* empty i/o permissions map */

	/* 
	 * make space for process pointer and cpu id and point to the first
	 * usable word
	 */
```

三个点：

而 `cpu` 为什么出现在参数里：它在内核里是一个数组（`tss[cpu]`），表里的槽位由 `TSS_INDEX(cpu)` 算。`prot_init` 里传的是 `tss_init(0, &k_boot_stktop)`——`0` 是启动处理器。剩下的部分是实现细节：结构末尾留给“进程指针与 CPU 号”的位置（源码注释后半句）、以及 I/O 权限位图为什么从 `sizeof(struct tss_s)` 开始，本篇不用到。

`tss_init` 填的全是**选择子**，没有一个是地址。硬件收到一次从低特权级进来的切换时，先用 `ss0` 去段表里找到段，再按那个段的基址去用栈。这就是为什么段表基址的装载必须排在任务寄存器之前：**任务状态段自己也是靠选择子引用别人的。**

还有一问比"填什么"更早：**为什么不干脆继续用用户栈？** 把三件事摆开就清楚了：

- 那块内存是用户自己的。内核异常处理若在它上面压帧，用户程序可以**看着内核把返回地址与变量写进自己能读的内存**，改一个字就能扭曲内核的控制流。
- 用户能把栈指针指向任意位置，包括只读页与未映射页。而异常可能在**任何**时刻发生——包括刚进内核、还没来得及检查任何参数的时刻。用一个由被隔离方选的地址，等于把"能不能处理这个异常"交给被处理者。
- 用户栈可以被故意用到只剩几十字节，一次普通调用就碰上溢出，而溢出发生在内核里。

任务状态段回答的就是"进来踩哪块栈"，而这块栈必须是不受被隔离方控制的那块。

**`iobase` 指向结构末尾**，源码注释写着 "empty i/o permissions map"：I/O 权限位图被故意留成空，也就是"没有额外放行"。

### 2.4 装载那一步，和一次主动的自我测试

`protect.c:prot_load_selectors`（函数体全文）：

```c
void prot_load_selectors(void)
{
  /* this function is called by both prot_init by the BSP and
   * the early AP booting code in mpx.S by secondary CPU's.
   * everything is set up the same except for the TSS that is per-CPU.
   */
  x86_lgdt(&gdt_desc);	/* Load gdt */ 
  idt_init();
  idt_reload();
  x86_lldt(LDT_SELECTOR); 	/* Load bogus ldt */
  x86_ltr(TSS_SELECTOR(booting_cpu));

  x86_load_kerncs();
  x86_load_ds(KERN_DS_SELECTOR);
  x86_load_es(KERN_DS_SELECTOR);
  x86_load_fs(KERN_DS_SELECTOR);
  x86_load_gs(KERN_DS_SELECTOR);
  x86_load_ss(KERN_DS_SELECTOR);
}
```

前四条指令就是把 1.2 那张表里的三个位置"交给硬件"：段表基址、门表基址（`idt_init` 填内容、`idt_reload` 交位置）、本地描述符表（交那个故意不可用的）、任务寄存器。

最后六条是本节要停下来的地方。`prot_init` 里紧跟着有一段注释解释为什么要重装一遍（逐字）：

```c
  /* Currently the multiboot segments are loaded; which is fine, but
   * let's replace them with the ones from our own GDT so we test
   * right away whether they work as expected.
   */
```

**引导器已经给了一套能用的选择子，内核却把自己刚建的那套装进去——理由是"当场测一遍"。**

这里的取舍在于时宜。不换，也能跑：引导器那套段描述符把内核代码与数据都覆盖了，一切正常，而自己的表要到很久之后、第一次需要它们的时候才被发现是错的。换了，风险立刻暴露：如果段的基址、权限位或选择子索引拼错，**下一次取指或访存就失败**，而此刻机器还连着串口、栈还浅、错误最容易归因。

于是 1.3 那张图的 ④ 有了答案：**"表建好了"不是终点；把表的位置交给硬件、再用硬件真跑一遍，才是。**

### 2.5 门表：填什么、填哪些、靠什么收尾

先看填一个门要写哪几样（`minix3/minix/kernel/arch/i386/protect.c:int_gate`，节选）：

```c
  idp = &tab[vec_nr];
  idp->offset_low = offset;
  idp->selector = KERN_CS_SELECTOR;
  idp->p_dpl_type = dpl_type;
  idp->offset_high = offset >> OFFSET_HIGH_SHIFT;
```

除了入口地址（拆成低/高两半，因为描述符格式里两者不连续），门里还填一个**选择子**与一个类型字段（存在位、门类型、允许进来的特权级）。选择子写死成内核代码段——也就是说**"异常进来用哪个段"仍然回到 2.2 那张段表**。门表依赖段表，这就是 2.4 里段表装载排在门表装载之前的原因。

再看填哪些。门表的内容来自两张静态表（节选，保留结构形状与原条目）：

```c
static struct gate_table_s gate_table_pic[] = {
	{ hwint00, VECTOR( 0), INTR_PRIVILEGE },
	...
	{ hwint15, VECTOR(15), INTR_PRIVILEGE },
	{ NULL, 0, 0}
};

static struct gate_table_s gate_table_exceptions[] = {
	{ divide_error, DIVIDE_VECTOR, INTR_PRIVILEGE },
	{ single_step_exception, DEBUG_VECTOR, INTR_PRIVILEGE },
	{ breakpoint_exception, BREAKPOINT_VECTOR, USER_PRIVILEGE },
	{ overflow, OVERFLOW_VECTOR, USER_PRIVILEGE },
	...
	{ page_fault, PAGE_FAULT_VECTOR, INTR_PRIVILEGE },
	...
	{ ipc_entry_softint_orig, IPC_VECTOR_ORIG, USER_PRIVILEGE },
	{ kernel_call_entry_orig, KERN_CALL_VECTOR_ORIG, USER_PRIVILEGE },
	{ ipc_entry_softint_um, IPC_VECTOR_UM, USER_PRIVILEGE },
	{ kernel_call_entry_um, KERN_CALL_VECTOR_UM, USER_PRIVILEGE },
	{ NULL, 0, 0}
};
```

每一行就是"入口函数、向量号、允许从哪一档进来"三项。两张表的最后一行都一模一样：`{ NULL, 0, 0}`——**表自己带一个哨兵条目作为结束**，而遍历靠它停（`for (gtp = first; gtp->gate; gtp++)`）：

```c
void idt_copy_vectors(struct gate_table_s * first)
{
	struct gate_table_s *gtp;
	for (gtp = first; gtp->gate; gtp++) {
		int_gate(idt, gtp->vec_nr, (vir_bytes) gtp->gate,
				PRESENT | INT_GATE_TYPE |
				(gtp->privilege << DPL_SHIFT));
	}
}
```

这种写法的收益很实际：加一条门只需在哨兵前追加一行，不用同时改一个计数常量。代价是哨兵必须在、且只能在一个——忘了写就会多读一行假条目。

把两张表的特权级列拉直看，能发现一个分布上的信息：16 条外设中断全是 `INTR_PRIVILEGE`（外设信号不应该能被用户程序触发），而异常表里是**三档例外**：断点与溢出（给调试器与软件中断用）、以及最下面的四条 IPC 与内核调用向量。那四条就是**系统调用与进程间通信的进门处**——它们当然得允许从用户级进来。而 2.1 开头那两次能力探测，探的正是"能不能用更快的通道进同一个地方"。

顺带一条容易误读的地方：`idt_init` 之后**中断还不能真正响应**——屏蔽位、控制器配置、向量映射在后续篇。这里只是把"该执行谁"写进表。

### 2.6 同一个名字的另一个端口：ARM 只共享最后两行

`minix3/minix/kernel/arch/earm/protect.c:prot_init`（函数体全文）：

```c
void prot_init(void)
{
	/* tell the HW where we stored our vector table */
	write_vbar((reg_t)&exc_vector_table);

	/* Set up a new post-relocate bootstrap pagetable so that
	 * we can map in VM, and we no longer rely on pre-relocated
	 * data.
	 */

	pg_clear();
	pg_identity(&kinfo); /* Still need 1:1 for device memory . */
	pg_mapkernel();
	pg_load();

	prot_init_done = 1;
}
```

把它和 2.1–2.5 那一份并排看，两份函数除了名字和末尾的页表重建，没有共享任何步骤。ARM 端口没有段描述符表、没有门表、没有任务状态段——它要做的只是"把向量表的位置告诉硬件"（一行 `write_vbar`），因为异常入口表在编译期就按固定形状填好了。

这里要防一个具体的误读：两份 `prot_init` **不是**同一份文件用条件编译编出来的。可以核对：`grep -c '#if' minix3/minix/kernel/arch/i386/protect.c` 的读数是 **0**。它们是两个目录下的两个文件，各自被各自的构建收进来；而 RISC-V 在 C 里根本没有对应端口（本项目新增的架构）。

### 2.7 同名不同实现：这一篇把 01 篇那句告诫兑现了

01 篇 2.3 留过一句话：有一个函数名比它的实现更强。当时 `vir2phys` 的实现是"直接把指针当物理地址返回"，理由是那一刻恒等映射成立。

现在看同一个文件族里的另一份（`minix3/minix/kernel/arch/i386/protect.c:vir2phys`，函数体全文）：

```c
phys_bytes vir2phys(void *vir)
{
	extern char _kern_vir_base, _kern_phys_base;	/* in kernel.lds */
	u32_t offset = (vir_bytes) &_kern_vir_base -
		(vir_bytes) &_kern_phys_base;
	return (phys_bytes)vir - offset;
}
```

**同名函数，实现不同**：这一版减掉了 02 篇 §2.1 算过的那个偏移量。差别不是修 bug，而是它们工作在两个不同时刻——前者在启用翻译之前（虚拟数值等于物理数值），后者在启用之后（内核代码住在高半区，物理地址要减回来）。

这正好是一个可复用的读码动作：**看到"地址转换"类函数，先看它被谁在什么时刻调用，而不是信名字。**01 篇说那份恒等实现"有效期就是恒等段还在用的那一段"，这一篇就是它的另一半。

### 2.8 末尾那次页表重建：恒等段此时还不能退休

回到 2.6 那份 ARM 代码里被完整保留、在 x86 那份里位于末尾的同一段（`prot_init` 结尾，逐字）：

```c
  pg_clear();
  pg_identity(&kinfo); /* Still need 1:1 for lapic and video mem and such. */
  pg_mapkernel();
  pg_load();

  prot_init_done = 1;
}
```

两句注释合起来解释了这个矛盾。先说为什么还要 `pg_clear()` 从头来一次：引导期那张表是在还不知道内核要跑什么的时候拍的，它的范围由账本高水位线推出来（01 篇 2.4–2.6），服务的是"正在执行的这段代码"；而从这里开始，内存管理权要交给虚拟内存服务，内核需要的是一张自己按需要重建的表，不再直接拿引导期那张继续用。

**"Still need 1:1 for lapic and video mem and such"** 这句注释回答的是第二半问题：为什么重铺之后**恒等段还在**。答案是有一类地址天然住低地址：本地中断控制器的寄存器窗口、显存。它们不是"可用的普通内存"（01 篇 2.3 的遍历里正因为如此被 `continue` 跳过），但内核要读写它们，所以它们的地址在翻译生效之后仍然必须可达。**恒等段在这里不是遗留，而是被点名保留的**；它什么时候才真正退休，属运行期页表篇。

### 2.9 调用关系与本章归纳

```
main.c:cstart
  └─ protect.c:prot_init
       ├─ 探测快速系统调用能力（两条 _cpufeature）
       ├─ 段表、门表整体清零
       ├─ 描述符表边界（基址 + 大小减一）
       ├─ tss_init(0, 内核启动栈顶)          每 CPU 一份
       ├─ 填段描述符（内核/用户各一对 + 一个不可用的本地表槽位）
       ├─ prot_load_selectors
       │     ├─ 交段表位置 → idt_init(填门) → 交门表位置
       │     ├─ 交不可用本地表、交任务寄存器
       │     └─ 换用六个自己的选择子         ← 一次主动自我测试
       ├─ pg_clear → pg_identity → pg_mapkernel → pg_load
       └─ prot_init_done = 1
```

三章合起来，不变的东西是这一条：**权限与入口这些"谁能进、进来到哪"的决定，必须由硬件持有的表来兑现；而表要生效，必须经历"填条目 → 把位置交给硬件 → 用硬件跑一遍"三步，少任何一步都只是内存里有数据。**两个端口的差别则说明另一件事：这三步是语义要求，具体几张表、几条指令是架构给的——ARM 只用了其中"交位置"那一步。

---

## 3. Rust 设计决策

本章每节第一句都是上一节里 C 的真实做法。

### 3.1 特权级是硬件概念，但 OS 语义层的名字不能是硬件名

**C 里两档特权级就是两个整数**（`INTR_PRIVILEGE`、`USER_PRIVILEGE`，被 `init_codeseg`/`init_dataseg` 直接写进访问字，见 2.2），而这两个整数的含义在三套硬件上完全不同：x86 是环号、ARM 是异常级别、RISC-V 是模式位。

重写的做法是把"档"做成接口的关联类型，再配两个常量与一对转换函数（`os/arch/src/arch/protection.rs:trait ProtectionArch`，**签名示意**，只列签名）：

```rust
pub trait ProtectionArch: Sized {
    type PrivilegeLevel: Copy + Eq + core::fmt::Debug;
    const KERNEL_PRIVILEGE: Self::PrivilegeLevel;
    const USER_PRIVILEGE: Self::PrivilegeLevel;
    fn to_privilege(level: Self::PrivilegeLevel) -> Privilege;
    fn from_privilege(privilege: Privilege) -> Self::PrivilegeLevel;
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);
    fn load(&self);
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}
```

关键在方向：OS 侧要写"这是内核级/用户级"，硬件侧要知道"这是 0 还是 3、EL1 还是 EL0、S 还是 U"。关联类型让每个架构自己决定表示（x86-64 的实现是一个新类型包着 `u8`，`os/arch/src/x86_64/protection.rs:struct X86PrivilegeLevel`，内核档为 0、用户档为 3），而 `Privilege` 这个枚举是给 OS 语义层用的统一名字。

顺带一条限度，与 01 篇 §3.2 对 `BootShim` 的结论同族：这一层保证的是**每套架构都得回答这四个问题**（两档叫什么、怎么换算、怎么建、怎么装载），**不保证**填进描述符的位组合是对的——位拼错照样编译通过。

### 3.2 填表与装载分离，而且这一步**刻意不装载**

**C 的 `prot_load_selectors` 里 `idt_init()` 与 `idt_reload()` 紧挨着**（2.4），填完就交，因为 C 的门表里填的是编译期就存在的桩入口。

重写不能这样，原因在它的入口注册时机上。`os/kernel/src/lib.rs:init_protection` 的文档注释把这件事写得很直白（转述，原文在此函数说明处）：此刻只准备好描述符表的元数据（权限位、内核栈索引、存在位），**入口地址仍然是占位的零**；如果在真入口装好之前就把表交给硬件，那么每一条异常与中断都会被路由到地址 0。因此这一阶段的表被刻意丢弃，后续启动阶段会重新建表、用 `set_handler` 装上真实入口，最后才 `load()`。

对照着看就是两句话：

| | C | Rust |
|---|---|---|
| 入口地址什么时候确定 | 编译期就写进门表（2.5 的静态表 + 哨兵遍历） | 运行期分阶段注册 |
| 于是"填表"与"装载"能不能连着做 | 能 | 不能 |

于是接口被拆成三个动作：`os/arch/src/arch/trap_entry.rs:trait TrapEntryArch` 的 `fn init`、`fn configure_syscall`、`fn set_handler`、`fn load`（**签名示意**，只列签名）：

```rust
pub trait TrapEntryArch: Sized {
    fn init() -> Self;
    fn configure_syscall(&mut self, entry_point: VirBytes);
    fn configure_ipc_entry(&mut self, entry_point: VirBytes);
    fn set_handler(/* ... */);
    fn load(&self);
    fn load_ap(&self);
}
```

`init` 的文档注释直接给出它的 C 对应（`idt_init`），"交给硬件"则单独成一步；两条 `configure_*` 对应 2.1 那两次能力探测——x86-64 走模型特定寄存器，ARM64 用同步异常走同一张向量表，RISC-V 用环境调用，所以这一步**不可能统一成一条**。

### 3.3 装载要求表原地不动，所以实例必须先住进全局

**C 的两张表是编译期静态数组**（2.1 的 `gdt`/`idt`），地址由链接期定死，交出去之后不会动。

重写里"表"是一个值类型（`os/arch/src/x86_64/protection.rs:struct X86_64Protection`），要交给硬件就必须给它一个**不会搬的家**。`init_protection` 因此先建、立刻存进全局、再装载，而不是在栈上建完就地装：如果表是栈上局部量，函数返回后那块栈会被复用，而硬件寄存器还指着它——下一次异常跳进的就是别人写过的数据。

这条约束在接口的文档里被写成契约（"表要在装载期间留在固定的静态位置"），也就是 `fn load` 的前置条件之一。它是那种"编译器不管、错了也不立刻发作"的约束，所以文档与全局槽是唯一防线。

### 3.4 搬一次家，会让表里内嵌的地址失效

3.3 说"先存进全局再装载"。紧跟在它前面还有一步容易被当成冗余的刷新，它是本篇因果链最长的一处。

重写里在存进全局之后、装载之前，多了一步专门刷新任务状态段描述符的调用（`os/arch/src/x86_64/protection.rs` 里的 `refresh_tss_descriptors`，x86-64 专属分支）。原因：段描述符表里那些"任务状态段"条目，**记的是结构的地址而不是指针**。实例在栈上建好时，条目里编的是栈上那个地址；把它搬进全局之后，条目不会自动跟着改（3.3 说的是地址稳定，不是地址相同）。

如果不做这一步刷新，后果不在这一步暴露：装载照旧成功、启动照旧往下走。**第一次从用户态掉进内核时**，硬件按表里那个旧地址去取"该用哪个栈"，读到的是一个早已失效的位置——取到的栈指针是 0。栈向低地址长，压一个 8 字节的入口就写到 `0 - 8`，也就是虚拟地址 `-8` 上的缺页；而缺页本身又要靠同一套机制处理，于是死循环。

这个例子给出的读法可以复用：**当一段数据会被硬件按值复制走（描述符、页表项、门），任何"搬移"都必须重新生成这些副本。**Rust 的借用与移动只描述语言里的所有权，不描述"别人抄了一份你的地址"。

而 x86 之外不需要这一步：ARM64 与 RISC-V 的换栈信息存在寄存器里（3.6），没有"表里内嵌地址"这回事，所以这一步只在 x86-64 分支上编译。

### 3.5 每 CPU 一份的结构，接口要成对

**C 用 `TSS_INDEX(cpu)` 与一份数组处理多颗处理器**（2.3），并且 `prot_load_selectors` 的注释声明它同时被启动处理器与辅助处理器复用，"除任务状态段以外完全一样"（2.4）。

重写把这句话变成接口形状：`ProtectionArch` 有 `init`/`load` 与 `init_ap`/（门表侧的）`load_ap` 两对，辅助处理器那一对接收 CPU 号、只补每 CPU 的那部分；`set_kernel_stack(cpu_id, stack_top)` 同样带 CPU 号，因为"换栈用哪个栈"是每 CPU 会变的。

这也解释了为什么 `init_protection` 被特意做成可被外部复用的一段：辅助处理器的启动尾巴要消费同一个全局，不能再建第二个实例（第二个实例就等于第二份表，而硬件只记一个位置）。

### 3.6 三架构对照：同一个语义，三种落点

`os/arch/src/arch/protection.rs` 的文件头给了一张对应表，摘其要点：

| 接口的动作 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| `init` | 清段表、填段描述符、建任务状态段 | 把停机栈基址放进一个核私有寄存器 | 配置换栈用的暂存寄存器 |
| `set_kernel_stack` | 更新任务状态段里的 `sp0` | 空实现（见下） | 更新 `sscratch` |
| `load` | 交段表、门表、任务寄存器 | 交向量表基址寄存器 | 交向量基址与状态位 |

ARM64 那格为什么是空实现？因为它不需要"预先告诉硬件内核栈在哪"：硬件自己知道——每次进异常时用的是异常栈寄存器（`SP_EL1`），而那个值在异常入口处已由启动代码写好；异常返回时又按规则换回 `SP_EL0`。所以"换内核栈"这件事在 ARM64 上不属于"装载期一次性设定"，而是"每次异常进出都发生的动作"，本方法自然无事可做。

三张表在三套硬件上的对应物并不齐平：x86 是"内存里的表 + 三个装载动作"（GDT/IDT/TSS），ARM64 与 RISC-V 更接近"几个寄存器的值"（`VBAR_EL1` 与 `SP_EL1`；`stvec` 与 `sscratch`）。**"换特权级时用哪个栈"这个语义是共同的，落在哪块存储上是架构差异**——这正好把 01 篇 §4.2 那条方法用反了一次：那里是"同一个语义落成不同位组合"，这里是"落成不同存储位置"。

### 3.7 末尾那次页表重建不在这一步了

**C 的 `prot_init` 末尾会重铺一次页表，并保留恒等段**（2.8）。重写里 `init_protection` 不做这件事：映射的建立在引导阶段就完成（01 篇 §4.2 那三步），运行期映射的建立属页表与内存篇。

但 2.8 那条语义没有消失，只是换了地方：**设备寄存器窗口在换根之后仍要可达**——它由契约里那份"保留窗口"清单交给运行期页表重放（01 篇 §3.5、§9 那两处）。也就是说，"Still need 1:1" 这句注释在重写里的回答不再是"再铺一次恒等段"，而是"把需要重放的窗口列出来交给建表的一方"。

---

## 4. 实现详解

本章每节标注它落实 §3 的哪条决策。

### 4.1 内核侧的调用序列（→3.1、3.2）

`kmain` 的 Phase B 第一件事就是它（`os/kernel/src/lib.rs` 里的 `kmain`，mock 与真实两份定义，此处指真实那份；节选）：

```rust
    init_protection(kernel_info);        // prot_init equivalent
```

函数内部（`os/kernel/src/lib.rs:fn init_protection`，按源码顺序取关键行，步与步之间的其它行以省略号标出）：

```rust
let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);
store_protection(prot);
...
#[cfg(target_arch = "x86_64")]
with_protection_mut(|prot| prot.refresh_tss_descriptors());
with_protection(|prot| prot.load());
...
let mut trap = CurrentTrapEntry::init();
...
store_trap_entry(trap);
with_trap_entry(|trap| trap.load());
```

三处与 §3 的对应：`store_protection` 之后再 `load`（3.3 的家）；中间那句只在 x86-64 编译（3.4 的刷新）；末尾 `load` 才是"交给硬件"的那一步，而在它之前 `set_handler` 已经把真实入口装上去了（3.2 的分阶段）。

栈顶的值来自契约字段 `kern_stack_top`——02 篇 §4.3 说过它从"脚本符号"变成了"契约字段"，这里是它第一次被消费。

### 4.2 段描述符与选择子（→3.1）

x86-64 侧的表示是一个新类型包着 `u8`（`os/arch/src/x86_64/protection.rs` 里的 `X86PrivilegeLevel`），内核档 `0`、用户档 `3`，与 2.2 的 `INTR_PRIVILEGE`/`USER_PRIVILEGE` 数值一致。

一个选择子在硬件上是三段拼起来的：索引（表里第几项）、表（全局表还是本地表）、请求特权级。读表时可以直接问这三段：**"索引决定用哪一条描述符，表位决定去全局表还是本地表找，请求特权级决定这次访问算谁的"**。这也是为什么 2.4 要换掉引导器给的选择子——不换，索引这一段的对应关系根本没被内核自己验证过。

### 4.3 向量表填写与入口注册（→3.2）

`os/arch/src/x86_64/protection.rs` 与 `os/arch/src/x86_64/trap_entry.rs` 提供每架构的实现；分发侧的入口函数在 `os/kernel/src/trap_dispatch.rs`，`init_protection` 里按架构把三类入口登记进去（同侧处理"当前异常级别"与"来自低一级"两条腿，因为 ARM64 的停机分支在解完栈帧后还要一个可跳转的再调度入口）。

与 2.5 的对照是这样的：C 的门表把入口地址与选择子一起写死在静态表里，靠一个哨兵条目收尾遍历；重写把"填入口"变成运行期动作（`set_handler`），因此才有 3.2 那条"先不装载"的决定。**门表结构没变，变的是谁在什么时候往里写。**

### 4.4 每 CPU 与辅助处理器（→3.5）

`init_ap` 与门表侧的 `load_ap` 是同一份表的第二次消费：辅助处理器不再建第二份，而是复用 §4.1 存进全局的那个实例，只补自己那份任务状态段/栈。2.4 那句 "everything is set up the same except for the TSS that is per-CPU" 在这里成了接口的两个方法名。

### 4.5 C 与 Rust 的形状对照

| 类别 | C | Rust | 为什么 |
|---|---|---|---|
| 保留 | 建表 → 交位置 → 用硬件跑一遍三步 | 同样三步，只是"交位置"与"填入口"被拆到不同阶段 | 语义要求，与语言无关 |
| 保留 | 每 CPU 一份任务状态段 | `init_ap`/`load_ap` + 带 `cpu_id` 的方法 | 同上 |
| 改型 | 两档特权级是裸整数 | 关联类型 + 新类型 + 一对转换函数 | OS 语义层不该出现环号/异常级别 |
| 改型 | 表是静态数组，地址链接期定死 | 表是一个值类型 + 必须先进全局再装载 | 值类型会被搬移，硬件按值记地址 |
| 淘汰 | 引导器给的选择子被换掉以自测 | 没有对应动作（引导方不是 GRUB，段寄存器不是运行前提） | 外部约定不同；自测改由入口注册与真机回归承担 |
| 新增 | — | `refresh_tss_descriptors`（仅 x86-64） | 3.4：搬移后重建内嵌地址 |
| 新增 | — | 入口地址运行期注册 | 3.2：分发逻辑与架构解耦的代价与收益 |

---

## 5. 测试要点

数量由作者现场跑过（命令可复算）：`trap_dispatch.rs` 13 条、`arch/src/x86_64/trap_entry.rs` 14 条、`exception.rs` 7 条、`cpu_identity.rs` 4 条。复算命令：

```bash
cd os && rg -c '#\[test\]' arch/src/x86_64/trap_entry.rs kernel/src/trap_dispatch.rs \
    arch/src/x86_64/exception.rs arch/src/x86_64/cpu_identity.rs
```

| 语义 | 真实用例 | 能证明什么 |
|---|---|---|
| 换栈信息按 CPU 更新（3.5、4.4） | `os/arch/src/x86_64/protection.rs` 的 `set_kernel_stack_updates_sp0` | 给定 CPU 号，`sp0` 被改成传入值——3.4 那条"读一个死 TSS"的直接反向保护 |
| CPU 号不合法时不许静默改写 | 同文件 `set_kernel_stack_panics_on_invalid_cpu_id` | 每 CPU 数组的索引错误当场暴露，而不是写到邻居的槽 |
| 分发侧的栈帧与入口约定 | `os/kernel/src/trap_dispatch.rs` 的用例组（上面命令读数 13） | 陷入帧形状、寄存器恢复路径 |
| 核身份锚点（3.5 的前置） | `os/arch/src/x86_64/cpu_identity.rs` 的用例组 | `current_cpu_id` 在启动处理器上也可读 |

只能真机判定的：装载指令真的生效后异常才进得来（表交出去那一刻没有返回值可检查）、入口地址为 0 的路由（3.2 那条决定，测不到是因为它要求"故意装错再验"）、快速系统调用通道的可用性（2.1 的能力探测结果依赖具体 CPU 型号）。

**盲区**（如实列出）：3.4 那条刷新是 x86-64 专属分支，宿主测试能覆盖 `set_kernel_stack`，但**"不刷新会读到死 TSS"这个因果链只有真机跑到第一次用户态陷入才会显现**——也就是说它的回归保护是间接的。另外本篇 §2.2 那个"为什么占住这个索引"与 §2.3 里"栈帧附近挂每 CPU 状态"两处，都标了未追全，属本篇自陈的未知。

---

## 6. 诊断

这一层的故障共同形状是**当场复位或无日志死循环**，因为出问题的是"异常发生时该做什么"这件事本身。定位方法是先分清坏在哪一步。

```bash
# 一：装载动作是不是都在填表之后（3.2、4.1）
rg -n 'store_protection|refresh_tss_descriptors|with_protection\(|store_trap_entry|\
with_trap_entry|set_handler|load_ap|init_ap' os/kernel/src/lib.rs

# 二：特权级与栈的对应关系（3.1、3.5、4.4）
rg -n 'fn set_kernel_stack|fn init_ap|fn load\b|TSS_INDEX|cpu_id' \
    os/arch/src/arch/protection.rs os/arch/src/x86_64/protection.rs \
    minix3/minix/kernel/arch/i386/protect.c
```

读法：命令一应当能看到"存进全局"出现在"装载"之前、`set_handler` 出现在门表的 `load` 之前；若顺序反了，就是 3.2/3.3 的契约被破。命令二里 `set_kernel_stack` 与 `init_ap` 的存在，对应 2.3/2.4 的每 CPU 语义。

三类症状：

**刚进内核主流程就没输出。** 查"表的位置有没有交给硬件"（2.4、4.1）。表建了但没装载，症状就是"任何异常都无处可去"——与"表建错"看起来一样，区别在装载日志/断点是否到达。

**跑着没事，第一次从用户态掉进内核就挂。** 这是 3.4 的形状：任务状态段描述符里的地址是旧的。查刷新是否在该架构分支上执行、以及它是否发生在装载之前；如果没做过刷新，症状一定伴随"栈指针为 0 一类的低地址缺页"。

**异常进来了但分发对不上号。** 门表元数据（权限位、内核栈索引）与入口地址是两次写入（3.2），若只做了前者，所有向量都会落到同一个入口。查 `set_handler` 的调用是否覆盖了用到的每一个向量号——2.5 的哨兵遍历在重写里对应的是显式逐个登记，漏一个就少一个。

---

## 7. 参见

**C 真值源**

- `minix3/minix/kernel/main.c:cstart` —— `prot_init` 的位置（2.1）
- `minix3/minix/kernel/arch/i386/protect.c` —— `fn prot_init`、`fn tss_init`、`fn prot_load_selectors`、`fn int_gate`、`fn idt_init`、`fn vir2phys`（2.1–2.8）
- `minix3/minix/kernel/arch/earm/protect.c` —— `prot_init` 的另一端实现（2.6）
- `minix3/minix/kernel/proto.h`、`minix3/minix/kernel/arch/i386/mpx.S` —— 声明与辅助处理器复用点（2.4、3.5）

**Rust 实现**

- `os/kernel/src/lib.rs:fn init_protection`、`fn kmain`、`fn with_trap_entry` —— 调用序列与两阶段装载（4.1）
- `os/arch/src/arch/protection.rs:trait ProtectionArch`、`os/arch/src/arch/trap_entry.rs:trait TrapEntryArch` —— 两个接口与三架构对照表（3.1–3.6）
- `os/arch/src/x86_64/protection.rs` —— `struct X86PrivilegeLevel`、`struct X86_64Protection`、`impl X86_64Protection::refresh_tss_descriptors`、换栈与辅助处理器方法（3.4、4.4）
- `os/arch/src/x86_64/trap_entry.rs`、`os/kernel/src/trap_dispatch.rs`、`os/arch/src/x86_64/cpu_identity.rs` —— 门表实现、分发入口、核身份（4.3、5）

**相关主题篇**（尚未生成的只给主题名）

- 01 篇（控制权交接）：恒等段的建立与"名字强于实现"那句告诫（2.7 的上文）
- 02 篇（高半区）：偏移量的来历、栈顶值如何变成契约字段（4.1）
- 平台发现篇：设备寄存器窗口从哪被发现（2.8 的"为什么还要保留恒等段"的后半）
- 时钟与中断篇：中断控制器配置、屏蔽与向量映射（2.5 末尾那条"还不能响应"）
- 陷入与系统调用篇：分发逻辑、栈帧、参数语义（1.4 列为不讲的整块）
- 多核篇：辅助处理器完整流程（3.5、4.4 只讲它复用哪一段）

**外部规范来源**（引用处均已就地标明为规范层面事实）

- x86-64 的段描述符、门描述符与任务状态段格式；特权级与选择子编码
- ARM 架构参考手册的异常向量表基址寄存器与异常级别；RISC-V 特权规范的模式与暂存寄存器约定
