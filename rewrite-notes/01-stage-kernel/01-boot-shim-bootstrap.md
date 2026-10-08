# 01 · Boot-shim 引导准备与控制权交接

> **C 真值源**：`minix3/minix/kernel/arch/i386/head.S`、`minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/arch/i386/pg_utils.c`、`minix3/minix/include/minix/param.h`
> **Rust 实现**：`os/boot-shim/src/`、`os/libs/minix-boot/src/`、`os/kernel/src/lib.rs`、`os/kernel/src/globals.rs`、`os/arch/src/x86_64/paging.rs`、`os/kernel-image/src/`

---

## 1. 概述

### 1.1 内核之前有谁：接口与清单

内核不是机器上第一个跑起来的程序。在它之前，固件完成了硬件自检、找到了可引导介质，然后把执行权交出去。问题是固件交出执行权时，并不替内核把后面的事做完。

32 位的 Minix3 走的是引导加载器 GRUB 这条路。GRUB 读配置文件，把内核二进制和一批启动模块（进程管理、虚拟内存、文件系统、根服务等用户态服务器镜像）搬进物理内存，把处理器切到保护模式，跳进内核入口，同时留下两样东西：一个魔数，一份已经整理好的启动信息结构。内核要做的只是解析别人给它的清单。

64 位的多架构世界不给你这份清单。统一可扩展固件接口（UEFI）与开源监督器接口（SBI）提供的是**可以调用的服务**：你可以问固件当前内存布局是什么、可以让它从磁盘读文件、可以让它分配一块标好用途的内存，但它不会替你把内核和模块搬好，也不会给你一张现成的可用内存表。

于是需要一个专门组件，负责把"能从固件问出来的东西"整理成"内核可以直接使用的启动信息"。本项目的这个组件叫引导垫片（boot-shim）。第 2 章看 C 原版把这件事做到了什么程度，第 3、4 章再看重写把它落到哪里。

### 1.2 交接瞬间机器缺了什么

引导代码接手时，机器的状态大致如下（32 位路径为例）：

| 维度 | 状态 |
|---|---|
| 特权级 | 已在高特权级（x86 保护模式 / ARM EL1 / RISC-V 监督器模式），但没有分页 |
| 寄存器 | 带着固件给的入口信息：GRUB 给魔数与结构地址，UEFI 给镜像基址，SBI 移交时给设备树指针 |
| 物理内存 | 内核与模块已被搬进去，但布局要靠引导代码自己问、自己整理 |
| 页表 | 不存在 |
| 堆与分配器 | 不存在 |
| 中断入口表 | 不存在 |

这张表里两个"不存在"的性质不同。**页表不存在是同步问题。** 处理器一旦开始用虚拟地址取指令或取栈，而表里还没有对应条目，错误就发生在当前这条指令上，没有任何回避余地。**中断入口表不存在是异步问题。** 设备的事件进不来，机器不会当场出问题，只是永远不响应。

这条区分决定了后面所有次序问题：真正紧迫的是地址的来源，不是事件的来源。

### 1.3 动作次序

把补上这些缺失的动作排成一条链：

```
固件（BIOS+GRUB / UEFI / SBI 移交）
  │  交出执行权 + 能从固件问出的原始信息
  ▼
① 入口：把执行环境降到最小可确定状态（栈、标志）
  ▼
② 信息整理：把外部的结构变成自己的数据（内存布局 + 模块清单）
  ▼
③ 过渡映射：虚拟地址等于物理地址，覆盖正在执行的这段代码所在地址
  ▼
④ 目的地映射：内核今后要待的高地址一并铺好
  ▼
⑤ 启用地址翻译
  ▼
⑥ 换到最终栈、进入内核主流程
```

图上⑤之后紧跟的不是"开始干活"，而是⑥换栈。为什么换栈要排在启用翻译之后，2.2 节从 `head.S` 的真实指令回答。

### 1.4 本篇范围与可以安全跳过的内容

本篇覆盖"控制权交接前必须成立的条件"：信息整理、模块与内核装入、过渡映射与启用翻译、交接的数据形状。以下内容本篇不讲，读者可以安全跳过而不影响理解：

- **链接布局与高半区目的地怎么算出来**：属于后面的高半区内核篇（内核镜像的链接地址与装载地址不同、栈与程序计数器的成对跳转）。本篇只需要"目的地是高地址"这一事实。
- **中断入口表、任务状态段等保护结构本体**：属于后面的保护结构篇。本篇只用到"此刻还没有它们"这一条前提。
- **运行期平台发现**（把设备树或高级配置与电源接口表解析成硬件描述符）：属于后面的平台发现篇。本篇只讲到"交接时递出的是一个分类标签加一个物理地址"。
- **早期 x86 的兼容机制**：实模式、地址线 A20、分段机制。它们是 8086/80286 时代为兼容性打的补丁，ARM 没有这些东西照样运行操作系统，而且固件早已处理完，引导代码的第一条指令已经在高特权级里。本篇需要理解的是"为什么需要特权级隔离"；全局描述符表寄存器怎么加载不在本篇——那属于固件与引导加载器的领地。

---

## 2. C 源码分析

### 2.0 内核二进制如何被引导器认出来

在 GRUB 能跳进内核之前，它得先认出这个二进制是一个可多引导的内核。协议要求内核文件前 8192 字节内置一个头部，内核自己把它嵌进去（`minix3/minix/kernel/arch/i386/head.S:multiboot_magic`）：

```asm
/* Multiboot header here*/

.balign 8

#define MULTIBOOT_FLAGS (MULTIBOOT_HEADER_WANT_MEMORY | MULTIBOOT_HEADER_MODS_ALIGNED)

multiboot_magic:
	.long MULTIBOOT_HEADER_MAGIC
multiboot_flags:
	.long MULTIBOOT_FLAGS
multiboot_checksum:
	.long -(MULTIBOOT_HEADER_MAGIC + MULTIBOOT_FLAGS)
	.long 0
...
/* Video mode */
multiboot_mode_type:
	.long MULTIBOOT_VIDEO_MODE_EGA
multiboot_width:
	.long MULTIBOOT_CONSOLE_COLS
multiboot_height:
	.long MULTIBOOT_CONSOLE_LINES
multiboot_depth:
	.long 0
```

（`.balign 8` 不是可有可无的排版：协议把“扫描”定义成**从加载起点往后、每 8 字节试一个位置**找魔数，而不是逐字节滑动。所以魔数必须踩在 8 的边界上才可能被碰到，碰不到就报“不是多引导内核”；头部本身也必须在加载后前 8 KB 内出现。`MULTIBOOT_HEADER_MAGIC` 与校验和的定义见 `minix3/sys/arch/i386/include/multiboot.h:MULTIBOOT_HEADER_MAGIC`。）

三个字段各自在做什么：

| 字段 | 值 | 作用 |
|---|---|---|
| 魔数 | `MULTIBOOT_HEADER_MAGIC`（`0x1BADB002`） | 引导器扫描二进制开头，找到它就按本协议启动 |
| 标志 | 需要内存信息 \| 模块页对齐 | 内核反向要求引导器：递内存地图、把模块按页对齐放置 |
| 校验和 | `-(magic + flags)` | 三者相加为 0 才算头部完整 |

这里有一处新手极易混淆的东西，单列一张表——**有两个方向相反的魔数**：

| 魔数 | 值 | 方向 | 含义 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | 嵌在内核二进制里，告知对方"可按本协议启动我" |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | 启动后放进 `eax`，内核据此确认自己真是被多引导起来的（就是 2.2 那个断言） |

两个数字只差一个十六进制位，含义完全相反。读到 `assert(magic == MULTIBOOT_INFO_MAGIC)` 时若脑子里想的是头部那个魔数，整个信任链就会被读反。**区分它们只需一个问题：这个值是谁写在哪里、谁又在哪里读它？**

最后注意标志位里的模块页对齐要求：后面 2.4 的登记函数会把基址向上取整到整页，而 2.5 的重叠体检直接拿模块边界去切除——若引导器把模块放在非页对齐地址，两处都要跟着麻烦。一个引导期的标志位，决定了内核自己代码里的假设能成立。

### 2.1 入口汇编：第一个 C 函数之前的六条指令

`minix3/minix/kernel/arch/i386/head.S:multiboot_init`（节选）：

```asm
multiboot_init:
	mov	$load_stack_start, %esp	/* make usable stack */
	mov	$0, %ebp
	push	$0	/* set flags to known good state */
	popf	/* esp, clear nested task and int enable */
	push	$0

        push    %ebx	/* multiboot information struct */
	push	%eax	/* multiboot magic number */
        call    _C_LABEL(pre_init)
```

**为什么入口是一段汇编而不是一个 C 函数？** C 函数能被调用的前提是栈已经存在：函数序言要往栈上放返回地址和局部变量。固件跳进入口时没有准备过栈，`esp` 里是固件遗留的值。所以必须先有一段不依赖栈的代码把栈搭起来——第一条指令就是干这件事，它把栈指针指到镜像自己数据段里预留的一小块：

```asm
.data
load_stack:
	.space 4096
load_stack_start:
```

`mov $0, %ebp` 与紧随的 `push $0` 配套：帧指针归零、栈底放一个 0，栈回溯走到这里停止。

`push $0; popf` 把整个标志寄存器置为 0，注释点明清掉的是嵌套任务位与中断允许位。这里要问一句：**为什么在做任何实事之前先关中断允许位？** 因为 1.2 那张表里中断入口表还不存在。若固件交接时留着中断允许位，而某个设备恰好在这几条指令之间发火，处理器要去查一张还没建立的表，结果是整机复位，而不是"这次中断丢失"。

还剩一行源码没有注释：`popf` 与 `push %ebx` 之间那条独立的 `push $0`。能核对的外部约束只有一条——x86 函数调用约定要求 `call` 之前栈顶按 16 字节对齐（被调用方用它存放宽度较大的局部变量），而 `load_stack_start` 是一段 4096 字节区的末端，此前压入的次数会破坏对齐，这条 `push` 大概是在把对齐补回来。**本篇把它标为 `[待验证]`**：是不是补对齐、有没有兼作帧哨兵，源码未写，不做断言。无注释、看似多余的动作要记成问题、不能当噪声跳过：冗余与遗漏只在写它的人那里有区别，读者手上两者都只能靠使用处来判。

再看压栈顺序。函数签名是 `pre_init(u32_t magic, u32_t ebx)`，`magic` 在前，代码却先压 `ebx`、后压 `eax`。依据只有一条约定：**参数从右往左依次压栈，最后压入的那个落在最靠近栈顶处，也就是函数读到的第一个参数。**先压结构地址、后压魔数，魔数才在第一个参数位上。可以自查：`pre_init` 的第一个动作是断言魔数，它读第一个参数，读到的应当正是刚压入的 `eax`。

这个文件末尾还剩一小段，它同样属于教学对象：

```asm
	/* not reached */
hang:
	jmp hang
```

一个自己跳自己的死循环。为什么入口代码要为"不应该到达的地方"写收尾？因为这是**失败时的可观察边界**：若 `kmain` 真的返回了（内核主流程不允许返回），这里能保证处理器不会继续沿着后面的字节当指令执行。读内核启动代码时值得养成这个问法：**这里的"不可能"为什么还要写出来？** 通常答案都是"写的人不肯假设它不可能"。

### 2.2 内核第一个 C 函数

`minix3/minix/kernel/arch/i386/pre_init.c:pre_init`（函数体全文）：

```c
kinfo_t *pre_init(u32_t magic, u32_t ebx)
{
	assert(magic == MULTIBOOT_INFO_MAGIC);

	/* Get our own copy boot params pointed to by ebx.
	 * Here we find out whether we should do serial output.
	 */
	get_parameters(ebx, &kinfo);

	/* Make and load a pagetable that will map the kernel
	 * to where it should be; but first a 1:1 mapping so
	 * this code stays where it should be.
	 */
	pg_clear();
	pg_identity(&kinfo);
	kinfo.freepde_start = pg_mapkernel();
	pg_load();
	vm_enable_paging();

	/* Done, return boot info so it can be passed to kmain(). */
	return &kinfo;
}
```

`assert(magic == MULTIBOOT_INFO_MAGIC)` 断言的是"我是按多引导约定被跳进来的"。这个前提不成立，`ebx` 指向的就不是约定的结构，后面所有解析都是在读垃圾——所以它不是礼节性检查，而是把后面一整段对 `ebx` 所指结构的信任先确认下来。

现在回答 1.3 留下的那个问题。看 `pre_init` 返回之后，`head.S` 紧接着执行什么：

```asm
	/* Kernel is mapped high now and ready to go, with
	 * the boot info pointer returnd in %eax. Set the
	 * highly mapped stack, initialize it, push the boot
	 * info pointer and jump to the highly mapped kernel.
	 */
	mov	$k_initial_stktop, %esp
	push	$0	/* Terminate stack */
	push	%eax
        call    _C_LABEL(kmain)
```

**这四条指令是启用翻译之后执行的，而它们自己位于低地址。** `mov $k_initial_stktop, %esp` 把栈换成内核的初始栈（这块栈为什么在那时就能用、它落在哪个地址区间，正是下一篇要回答的问题），换完之前处理器一直在用低地址取指与取栈；换栈之后的 `push $0`、`push %eax` 与 `call kmain` 里，`call kmain` 是**第一次跳往高地址入口**——目的地映射为的就是这一步。

于是上面那个函数里三条映射相关语句（`pg_identity`、`pg_mapkernel`、`vm_enable_paging`）的次序有了完整解释。把假设摊开：**如果去掉 `pg_identity`、只铺内核高地址映射，然后就启用翻译会怎样？** 启用翻译后下一条取指用的是虚拟地址，而此刻正在执行的这段引导代码所在的低地址在新表里没有条目，取指落空就是页错误，页错误需要中断入口表里的处理入口，那张表还没建立——结果是复位。反过来若只铺低地址不铺高地址，`call kmain` 那一步落空，同样没有处理入口。恒等段服务的对象是**正在执行这段代码的处理器**，高地址段服务的对象是**接下来要跳去的地方**，两个都得在开关之前备好。源码注释里那句 "but first a 1:1 mapping so this code stays where it should be" 中的 "this code"，指的就是前者。

### 2.3 把外部的结构变成自己的数据

`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 的开头：

```c
	multiboot_info_t *mbi = &cbi->mbi;
	...
	/* get our own copy of the multiboot info struct and module list */
	memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));
```

`ebx` 指向的结构属于引导加载器放在内存里的东西，不是引导代码的资产。**先复制一份进自己的全局结构，才谈得上在后面 long 地使用它**——这一动作在第 4.3 节会以另一种形式重现，理由相同。

接着是内存布局的两条来源分支：

```c
	if(mbi->mi_flags & MULTIBOOT_INFO_HAS_MMAP) {
		cbi->mmap_size = 0;
	        for (mmap = (multiboot_memory_map_t *) mbi->mmap_addr;
       	     (unsigned long) mmap < mbi->mmap_addr + mbi->mmap_length;
       	       mmap = (multiboot_memory_map_t *)
		      	((unsigned long) mmap + mmap->mm_size + sizeof(mmap->mm_size))) {
			if(mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue;
			add_memmap(cbi, mmap->mm_base_addr, mmap->mm_length);
		}
	} else {
		assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MEMORY);
		add_memmap(cbi, 0, mbi->mi_mem_lower*1024);
		add_memmap(cbi, 0x100000, mbi->mi_mem_upper*1024);
	}
```

上半段遍历固件报的内存映射，只登记"可用"类型；步进写法 `本条目地址 + 本条目大小 + 那个"大小"字段的宽度` 是变长条目的遍历约定。注意被 `continue` 跳过的那些类型——这正是第 2.6 节要用的信息。

回落分支里有两个数字值得看清：低内存从地址 0 起算、单位是千字节（所以乘 1024），高内存硬编码从 `0x100000`（1 MB）起算。这不是策略选择，是早期个人电脑的内存布局：前 1 MB 是常规内存，之上才是扩展内存。固件只肯给两个千字节数时，代码只能按这个历史形状还原。

之后是几条断言与模块清单的拷贝：`mi_flags` 必须含模块、模块数必须大于 0 且不超过上限，然后 `memcpy` 模块清单。

还有三处东西容易被一句带过，但它们各自撑住后面某个结论，分别看。

**第一处：`user_sp` 被赋了两次值。** 先按链接期符号给（`cbi->user_sp = (vir_bytes) &_kern_vir_base;`），随后无条件被盖成 `USR_STACKTOP`——中间没有任何分支会保住第一次的值，所以它实际上是一次被抹掉的预置。源码注释给的是后一次的理由，两个：给内核栈溢出留一片空白区去撞（使溢出在成为无声破坏之前先被发现），以及让内核地址与用户地址看上去就能分辨。这两个理由要成立，需要具体数字而不是形容词：`minix3/minix/kernel/const.h` 里 `USR_DATATOP` 按配置取 `0xF0000000` 或 `0xE0000000`，而 `USR_STACKTOP` 直接等于 `USR_DATATOP`——所谓“一眼可辨”就是高 4 位不同。为什么第一次赋值还要写？源码未说明，本篇标为 `[待验证]`：读到这里可以把它当成一个历史遗留的默认值，但不要替它编一个理由。

**第二处：`kernel_may_alloc` 的定义与使用窗口。** 2.4 的 `cut_memmap` 与 `add_memmap` 开头都有 `assert(kernel_may_alloc)`，这个全局变量定义在 `minix3/minix/kernel/arch/i386/pre_init.c`（初值为 1，声明在 `minix3/minix/kernel/glo.h`）；真正把它关掉的地方在内核主流程末尾（`minix3/minix/kernel/main.c`：`kernel_may_alloc = 0;` 紧接着 `switch_to_user();`，而在此之前还有一处 `kernel_may_alloc = 1;`）。源码注释写得很明白：虚拟内存服务即将运行，内核不能再拿这些位去用了。于是这串断言守的不是一句空话，而是一个**时间窗口**：引导期内核可以向可用清单拿页，交给虚拟内存服务之前必须停手，否则两边的分配器会抢同一块内存。

**第三处：一个函数名会让人上当。** `minix3/minix/kernel/arch/i386/pre_init.c:vir2phys` 的实现是：

```c
/* pg_utils.c uses this; in this phase, there is a 1:1 mapping. */
phys_bytes vir2phys(void *addr) { return (phys_bytes) addr; } 
```

名字是"虚拟转物理"，实现却是一个直返。它不是简化的偷懒，而是**正确地利用了本章的条件**：此刻恒等映射成立，虚拟地址与物理地址数值相等，所以下游（如 2.6 的页目录本身）把指针转成物理地址就是直接换算。但它同时是一个危险函数：**它的有效期就是恒等段还在用的那一段**。一旦将来某个路径在高半区且根表已换后误调它，它不会报错，只会静默地返回一个错的数。这类"名字描述了一个比实现更强的保证"的函数，读的时候要看它的注释写了什么前提、而不是看名字。（4.2 会看到重写把这件事换成了显式传入物理地址。）

另外 `mb_set_param` 在存入参数时顺手识别两个名字（串口调试开关与波特率），写进 `kinfo` 的两个字段；这两个字段在第 4.4 节的对照表里会被归入"淘汰"一类，此处先知道它们在 C 里确实有用途。

### 2.4 可用内存清单的四种操作

`minix3/minix/kernel/arch/i386/pg_utils.c` 提供四个操作。它们回答的是同一件事：引导期谁在修改这张"可用物理内存"清单，怎么保证改完还自洽。

**登记**（`add_memmap`，节选）：

```c
#define LIMIT 0xFFFFF000
        /* Truncate available memory at 4GB as the rest of minix
         * currently can't deal with any bigger.
         */
        if(addr > LIMIT) return;
        if(addr + len > LIMIT) {
                len -= (addr + len - LIMIT);
        }
        assert(cbi->mmap_size < MAXMEMMAP);
        if(len == 0) return;
	addr = roundup(addr, I386_PAGE_SIZE);
	len = rounddown(len, I386_PAGE_SIZE);
	...
		highmark = addr + len;
		if(highmark > cbi->mem_high_phys) {
			cbi->mem_high_phys = highmark;
		}
```

三件事：超过 4 GB 的段整段丢弃、跨界那段截短（32 位寻址上限）；**基址向上取整、长度向下取整**，让登记的每一段都是整页——向内收，因为不能把不完整的页交付出去；顺手推高一条"真实内存最高水位"。这条线后面要用。

**切除**（`cut_memmap`，节选）：

```c
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)
{
        if((o=start % I386_PAGE_SIZE))
                start -= o;
        if((o=end % I386_PAGE_SIZE))
                end += I386_PAGE_SIZE - o;

	assert(kernel_may_alloc);

        for(m = 0; m < cbi->mmap_size; m++) {
                phys_bytes substart = start, subend = end;
                ...
                if(substart < memaddr) substart = memaddr;
                if(subend > memend) subend = memend;
                if(substart >= subend) continue;

                cbi->memmap[m].mm_base_addr = cbi->memmap[m].mm_length = 0;
                if(substart > memaddr)
                        add_memmap(cbi, memaddr, substart-memaddr);
                if(subend < memend)
                        add_memmap(cbi, subend, memend-subend);
        }
}
```

对齐方向与登记**相反**：起始向下、末尾向上，切除区间被向外扩到整页边界。原因是内核与模块的加载地址不保证页对齐；如果只切除对齐后的内层，**一个已经装着代码的半页在清单上仍显示可用**，下一次分配就会把这一页交出去。向内收是省内存，向外扩是保正确。

循环末尾那两行 `add_memmap` 补回左右残段：切除可能把一段可用内存劈成两半，所以先作废原条目、再把左右剩下的部分分别登记回去。

**从尾取页**（`pg_alloc_page`，节选）：

```c
	for(m = cbi->mmap_size-1; m >= 0; m--) {
		mmap = &cbi->memmap[m];
		if(!mmap->mm_length) continue;
		...
		mmap->mm_length -= I386_PAGE_SIZE;
		...
		return mmap->mm_base_addr + mmap->mm_length;
	}
	panic("can't find free memory");
```

它从最高那段的尾部倒着取一页，专供引导期建表使用。取高位是为了让大块连续区域的前端少被切碎。

**从低取整块**（`alloc_lowest`）就不一样了。把它的调用行与 `cut_memmap` 的签名并排放：

```c
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end);
...
	assert(lowest != EMPTY);
	cut_memmap(cbi, lowest, len);
```

第三个实参的位置是 `end`，传进去的是 `len`。用具体数字走一遍：最低可用段是 `[0x100000, 0x20000000)`，要分配 4 KB，于是 `start = 0x100000`、`end = 0x1000`。循环里对每一段都做 `substart = max(start, memaddr)`、`subend = min(end, memend)`，得到 `substart = 0x100000`、`subend = 0x1000`，`substart >= subend` 成立，走 `continue`。**整张清单一段都没有被摘掉。**

这个函数确实返回了一个正确的最低地址，也确实记了一笔分配量（`kernel_allocated_bytes_dynamic += len`），但"把这块内存从可用清单里除去"这半件事只在最低段的基址恰好为 0 时成立（那时 `start = 0`、`end = len`，边界正常）。它在内核里的唯一调用点在辅助处理器启动路径（`minix3/minix/kernel/arch/i386/arch_smp.c`，那一行是 `trampoline_base = alloc_lowest(&kinfo, tramp_size)`）。

这里可以记下一条阅读方法：**看到"分配"，去确认它是否真的把那块内存从可用清单里除去，而不只是返回了一个地址。**

除了这四个函数，`pg_utils.c` 里还有一套完全不碰清单的页表页分配器（`alloc_pagetable`，函数体全文）：

```c
u32_t *alloc_pagetable(phys_bytes *ph)
{
	u32_t *ret;
#define PG_PAGETABLES 6
	static u32_t pagetables[PG_PAGETABLES][1024]  __aligned(4096);
	static int pt_inuse = 0;
	if(pt_inuse >= PG_PAGETABLES) panic("no more pagetables");
	assert(sizeof(pagetables[pt_inuse]) == I386_PAGE_SIZE);
	ret = pagetables[pt_inuse++];
	*ph = vir2phys(ret);
	return ret;
}
```

为什么不直接用 `pg_alloc_page` 拿页，而要先静态预留 6 个？因为顺序上需要一个先有鸡还是先有蛋的问题：`pg_alloc_page` 要从可用清单拿页，而可用清单本身要写进内存——所以页表页必须来自一个不依赖清单的固定池。池子只有 6 页（共 24 KB），用完 `panic`。为什么 6 页在 C 这里够？因为 32 位两级页表的引导期只需要覆盖少量区域；到了四层级、还要盖住整段低地址的 64 位情形，同一个固定池就不够用了——这一段属于后面的页表篇，本篇只需要知道：**这个 `#define PG_PAGETABLES 6` 是一个随架构层级数变化的隐含假设，而不是一个可搬的常量。**另注意它返回物理地址时调的正是上一节那个 `vir2phys`：同一个前提，两处使用。

### 2.5 重叠体检与端点写法

若干镜像（内核 + 模块）各自被放到物理内存里，位置动态决定。谁保证它们不互相覆盖？`get_parameters` 的做法比直觉多做一步：

```c
	/* Sanity check: the kernel nor any of the modules may overlap
	 * with each other. Pretend the kernel is an extra module for a
	 * second.
	 */
	k = mbi->mi_mods_count;
	assert(k < MULTIBOOT_MAX_MODS);
	cbi->module_list[k].mod_start = kernbase;
	cbi->module_list[k].mod_end = kernbase + kernsize;
	cbi->mods_with_kernel = mbi->mi_mods_count+1;
	cbi->kern_mod = k;

	for(m = 0; m < cbi->mods_with_kernel; m++) {
		...
		if(overlaps(cbi->module_list, cbi->mods_with_kernel, m))
			panic("overlapping boot modules/kernel");
		/* We cut out the bits of memory that we know are
		 * occupied by the kernel and boot modules.
		 */
		cut_memmap(cbi,
			cbi->module_list[m].mod_start, 
			cbi->module_list[m].mod_end);
	}
```

循环遍历的是 `mods_with_kernel`，比固件报的模块数多一项，多出来的那一项填的是内核镜像自己的物理边界。于是"内核是否与模块重叠"不需要另写一套逻辑：同一套两两查重把它覆盖了。代价是数组要多留一格，`assert(k < MULTIBOOT_MAX_MODS)` 守的就是这一格。

判重叠的宏在这里决定正确性：

```c
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
			INRANGE(mod1, mod2->mod_end-1))
```

第二个宏比较的是"另一个模块末地址减一"。拿两个具体地址自查：模块 A 占 `[0x100000, 0x200000)`、模块 B 占 `[0x200000, 0x300000)`，两者首尾相接、都不越界。若比较的是 `mod_end`，检查 B 时会算 `INRANGE(B, A->mod_end = 0x200000)`——A 的末地址恰好等于 B 的起始地址，于是这个合法布局被判成重叠、当场 `panic`。用 `mod_end-1`（区间内真实存在的最后一个字节）就把半开区间的约定收进了判断里。

### 2.6 过渡映射的两个标志组

先把名字点齐，否则本节会在四个说法之间迷路：**“过渡映射”、“恒等映射”、“1:1 mapping” 与函数 `pg_identity` 铺的那张“虚拟地址等于物理地址”的表是同一件事**；本文统一称“恒等段”，读到任一个名字都可以回到这个定义。与之对应的是“目的地映射”（`pg_mapkernel` 铺的内核高地址段）。

本节会用到一组页表位名，先把表放在码前（完整查表入口连同控制寄存器位与地址宏在 2.9）：

| 常量 | 值 | 本节怎么用 |
|---|---|---|
| `I386_VM_PRESENT` | `0x001` | 两个映射都设 |
| `I386_VM_WRITE` | `0x002` | 两个映射都设 |
| `I386_VM_USER` | `0x004` | **恒等段设、目的地映射不设**——本节争的就是它 |
| `I386_VM_PWT` | `0x008` | 只给越界段（写穿透） |
| `I386_VM_PCD` | `0x010` | 只给越界段（禁缓存） |
| `I386_VM_BIGPAGE` | `0x080` | 4 MB 大页，循环步长就是它 |
| `I386_VM_ADDR_MASK_4MB` | `0xFFC00000` | 水位线比较用的掩码（为什么用 `<=` 见本节末尾与 2.9 的手算） |

`minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity`（函数体全文）：

```c
void pg_identity(kinfo_t *cbi)
{
	uint32_t i;
	phys_bytes phys;

	/* We map memory that does not correspond to physical memory
	 * as non-cacheable. Make sure we know what it is.
	 */
	assert(cbi->mem_high_phys);

        /* Set up an identity mapping page directory */
        for(i = 0; i < I386_VM_DIR_ENTRIES; i++) {
		u32_t flags = I386_VM_PRESENT | I386_VM_BIGPAGE
			| I386_VM_USER
			| I386_VM_WRITE;
                phys = i * I386_BIG_PAGE_SIZE;
		if((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB)
			<= (phys & I386_VM_ADDR_MASK_4MB)) {
			flags |= I386_VM_PWT | I386_VM_PCD;
		}
                pagedir[i] =  phys | flags;
        }
}
```

四条观察：

**函数第一件事是断言高水位线非零。** 对照 2.4 的登记函数：那条线只有 `add_memmap` 会推高。所以这里守的是数据依赖——**先把可用内存登记完，才能铺这张表**，因为铺表时要用这条线判断哪些段落在真实内存之外。

**循环铺满整个目录**，每个目录项对应一个 4 MB 大页，从 0 一直铺到 4 GB，虚拟等于物理。

**越过水位线的段额外加两个禁缓存的位。** 2.3 的遍历里被 `continue` 跳过的正是非可用类型，所以水位线之上的地址不是"空的内存"，而是设备寄存器窗口与地址空洞。对设备寄存器开缓存会破坏"写完立刻读回来看状态"这类语义：写可能滞留在缓存里没落到设备，读可能拿到旧值。源码注释写得很直白："We map memory that does not correspond to physical memory as non-cacheable."这类错误的特征是**不会当场崩溃**，因而比崩溃更难查。

**标志组里带着一个用户可访问位**，而下一个函数不带（`pg_mapkernel`，节选）：

```c
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
	        pagedir[pde] = kern_phys | I386_VM_PRESENT | 
			I386_VM_BIGPAGE | I386_VM_WRITE;
		mapped += I386_BIG_PAGE_SIZE;
		kern_phys += I386_BIG_PAGE_SIZE;
		pde++;
	}
	return pde;	/* free pde */
```

两处挨在一起、只差一个位，第一眼像笔误。在 32 位单一架构上，引导期处理器已在最高特权级，用户位不参与对该访问的判定，所以恒等映射带这一位既不增加权限也不减少权限；真正表达意图的是内核高地址段**不带**它。这份恒等映射也活不到运行期：虚拟内存服务建立自己的页表时显式跳过大页条目（`minix3/minix/servers/vm/pagetable.c:pt_init`），另起一张表。

顺带说一句这个函数返回的是什么：`return pde`——第一个还没被用掉的目录项号，`pre_init` 把它存进 `kinfo.freepde_start`。内核高地址映射到这里为止，之后的映射由别人接着用。

### 2.7 启用翻译的位序

`minix3/minix/kernel/arch/i386/pg_utils.c:pg_load` 先把页目录的物理地址写进页表基址寄存器：

```c
phys_bytes pg_load(void)
{
	phys_bytes phpagedir = vir2phys(pagedir);
        write_cr3(phpagedir);
	return phpagedir;
}
```

然后 `minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`（节选）：

```c
        pgeok = _cpufeature(_CPUF_I386_PGE);
        ...
        cr0= read_cr0();
        cr4= read_cr4();

	/* The boot loader should have put us in protected mode. */
	assert(cr0 & I386_CR0_PE);

        /* First clear PG and PGE flag, as PGE must be enabled after PG. */
        write_cr0(cr0 & ~I386_CR0_PG);
        write_cr4(cr4 & ~(I386_CR4_PGE | I386_CR4_PSE));

        cr0= read_cr0();
        cr4= read_cr4();

        /* Our page table contains 4MB entries. */
        cr4 |= I386_CR4_PSE;
        write_cr4(cr4);

        /* First enable paging, then enable global page flag. */
        cr0 |= I386_CR0_PG;
        write_cr0(cr0);
        cr0 |= I386_CR0_WP;
        write_cr0(cr0);

        /* May we enable these features? */
        if(pgeok)
                cr4 |= I386_CR4_PGE;
        write_cr4(cr4);
```

源码自己给了两条位序理由，都写在注释里：一是"全局页位必须在分页位之后开"，二是"我们的页表里有 4 MB 条目"——后者意味着大页位必须早于分页位，否则刚启用翻译就按 4 KB 格式去解释那些大页条目。这两条是机器规定的依赖。

另外"先把分页、全局页、大页三个位清零、重读、再从头点亮"这一手，是一种实现选择。它买的东西是确定性：不论上一棒遗留了什么位，归零后按固定顺序点亮，结果一致。换一种写法（读回当前值、只补需要的位）同样能工作，代价是结果会带上固件残留。这个区分在第 3.4 节会再次出现，因为重写选的正是后一种。

### 2.8 调用关系与本章归纳

```
head.S:multiboot_init
  ├─ 设临时栈（镜像数据段里 4096 字节）
  ├─ 标志寄存器归零（清中断允许与嵌套任务）
  ├─ push %ebx, push %eax → call pre_init
  │     ├─ assert(magic)                        确认交接约定
  │     ├─ get_parameters(ebx, &kinfo)
  │     │     ├─ memcpy 结构 + memcpy 模块清单    先拥有数据
  │     │     ├─ 遍历内存映射 / 回落两段          登记
  │     │     ├─ 内核追加为额外一项 → overlaps    两两查重
  │     │     └─ cut_memmap × (N+1)              切除占用
  │     ├─ pg_clear()                           页目录清零
  │     ├─ pg_identity(&kinfo)                  恒等（越界段禁缓存）
  │     ├─ pg_mapkernel() → freepde_start       内核高地址（无用户位）
  │     ├─ pg_load()                            写页表根
  │     └─ vm_enable_paging()                   清零→大页→分页→写保护→全局页
  │           → return &kinfo
  ├─ 换到内核初始栈、栈底放 0
  └─ push %eax → call kmain
```

把七节合起来，这条链上不可颠倒的东西有一类共同来源：**处理器此刻用哪些地址取指、取栈**。恒等映射必须早于启用翻译，因为正在执行的代码在低地址；内核高地址映射必须早于启用翻译，因为紧接着要跳去的入口与要使用的栈在高地址；信息整理必须早于两个映射，因为登记产生的高水位线是判断"这段是不是真实内存"的依据。至于 `get_parameters` 内部各步之间、以及它与 `pg_clear` 之间，只是数据准备的先后，没有机器状态依赖。

---

### 2.9 本篇用到的常量、位与地址宏（查表入口）

上面的章节反复引用页表位、控制寄存器位与地址换算宏。这里集中列一份（均来自 `minix3/minix/include/arch/i386/include/vm.h`），往后遇到名字忘了就回到这张表，不必从头重读。

页与页表项：

| 常量 | 值 | 含义 |
|---|---|---|
| `I386_PAGE_SIZE` | 4096 | 普通页大小 |
| `I386_BIG_PAGE_SIZE` | `4096 × I386_VM_PT_ENTRIES` = 4 MB | 大页大小（2.6 的循环以它为步长） |
| `I386_VM_DIR_ENTRIES` | 1024 | 页目录项数（乘上大页正好 4 GB） |
| `I386_VM_PRESENT` | `0x001` | 存在位 |
| `I386_VM_WRITE` | `0x002` | 可写 |
| `I386_VM_USER` | `0x004` | 用户可访问（2.6 与 4.2 争的就是它） |
| `I386_VM_PWT` | `0x008` | 写穿透 |
| `I386_VM_PCD` | `0x010` | 禁缓存 |
| `I386_VM_DIRTY` | `1L << 6` | 已写入（硬件自动置，引导期不手动用） |
| `I386_VM_BIGPAGE` | `0x080` | 4 MB 大页标志 |
| `I386_VM_ADDR_MASK` | `0xFFFFF000` | 从表项取物理地址（4 KB 粒度） |
| `I386_VM_ADDR_MASK_4MB` | `0xFFC00000` | 同上，但 4 MB 粒度（2.6 的水位线比较用它） |

控制寄存器位（它们是**位掩码值**，不是位号）：

| 常量 | 值 | 含义 |
|---|---|---|
| `I386_CR0_PE` | `0x00000001` | 保护模式（bit 0），2.7 断言它已开 |
| `I386_CR0_WP` | `0x00010000` | 写保护（bit 16） |
| `I386_CR0_PG` | `0x80000000` | 分页开关（bit 31） |
| `I386_CR4_PSE` | `0x00000010` | 大页扩展（bit 4） |
| `I386_CR4_PGE` | `0x00000080` | 全局页（bit 7） |

这里有一处陷阱要单独拆出来。上面这张表的每一行含义，都不是从源码注释读来的——因为注释本身不可靠：

```c
#define I386_CR0_WP		0x00010000	/* Enable paging */
#define I386_CR0_PG		0x80000000	/* Enable paging */
```

`WP` 那一行的注释写的是"Enable paging"，与下面 `PG` 那行一字不差——但它的值是 `0x00010000`，也就是 bit 16，而分页开关是 bit 31。**同一个注释出现在两个不同的位上，至多一个是真的。** 怎么定哪个对？回看使用处：2.7 里 `vm_enable_paging` 先 `cr0 |= I386_CR0_PG`（启用分页）、再 `cr0 |= I386_CR0_WP`（分页已经开了才设它）——一个已经在开的开关不会被第二次"开启"，所以 `WP` 的注释是错的、名字是对的。这就是**注释不能代替实现**的具体形态：读位定义时要拿三个东西互相校——值（哪一位）、名字（应当是什么）、使用处的上下文（用它干什么），三者对不上就存疑。

地址换算宏（同一文件，注意它们用的是移位常量，不是硬写的数字）：

```c
#define I386_VM_PFA_SHIFT	22	/* Page frame address shift */
#define I386_VM_PTE(v)	(((v) >> I386_VM_PT_ENT_SHIFT) & I386_VM_PT_ENT_MASK)
#define I386_VM_PDE(v)	( (v) >> I386_VM_DIR_ENT_SHIFT)
#define I386_VM_PFA(e)	( (e) & I386_VM_ADDR_MASK)
```

为什么这一组要单独列？因为 2.6 的水位线比较直接拿 `I386_VM_ADDR_MASK_4MB` 去掩一个字节地址，而这里告诉你“目录项索引就是高 10 位”（`v >> 22`）。两者合起来才能手算：4 MB 掩码与 22 位移位说的是同一件事，因此 2.6 那个条件是在比“目录项号”，不是在比字节地址。

引导与清单上限：

| 常量 | 含义 |
|---|---|
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002`，2.2 断言读的就是它 |
| `MULTIBOOT_INFO_HAS_MMAP` / `…_HAS_MEMORY` / `…_HAS_CMDLINE` / `…_HAS_MODS` | `mi_flags` 里的四个位，决定 2.3 走哪条来源分支 |
| `MULTIBOOT_MEMORY_AVAILABLE` | 1，2.3 只登记这个类型 |
| `MAXMEMMAP` | 40（`minix3/minix/include/minix/param.h:MAXMEMMAP`），清单条数上限 |
| `LIMIT`（`pg_utils.c` 内） | `0xFFFFF000`，2.4 的 4 GB 截断 |
| `PG_PAGETABLES`（`pg_utils.c` 内） | 6，2.4 的静态页表池 |
| `PG_ALLOCATEME`（`pg_utils.c` 内） | 一个占位值：传到 `pg_map()` 里表示“每页自己分配” |

拿着这张表，2.6 那个条件可以手算一遍。设真实内存高水位 `mem_high_phys = 0x10000000`（256 MB），循环到 `phys = 0x10000000` 与 `phys = 0x0C000000` 时各自加不加禁缓存位？把两个值都用 `0xFFC00000` 掩一下再看大小关系就知道：同一个 4 MB 目录项内部不会“一半可缓存、一半不可”，因为掩码把低 22 位全清掉了——这正好解释了为什么条件用 `<=` 而不是 `<`：与水位线同属一个 4 MB 目录项的那一段，宁可多禁一点缓存，也不能漏禁。

---

## 3. Rust 设计决策

本章凡有 C 对应面的节，第一句都是 C 里真实存在的语义或负担，回指第 2 章具体小节；重写的设计决策由它逼出。

### 3.1 交接只能传值

**C 里所有启动信息住在一个全局结构里**（2.3 的 `kinfo`，定义见 `minix3/minix/include/minix/param.h:kinfo`），`pre_init` 填它、`kmain` 读它，同一个镜像、同一个数据段，传来传去都不成问题。这个安排能成立的前提是：结构与读者住在同一块永久映射里，而且填写它的那段代码即使被回收，数据仍然在。

重写之后，"填写信息的那段代码"与"使用信息的那段代码"最终是两个二进制，前者在交接之后会被回收或重新映射。于是有一条推论必须落实：**跨边界传递的东西不能引用传递它的那一侧。**

具体到设计选择：

| 方案 | 做法 | 结果 |
|---|---|---|
| 选：只传值 | 结构按固定布局、可复制，载荷是静态切片；需要传递"能力"时传一个分类标签（`u32`）加一个物理地址，由接收方自己查表处理（`os/libs/minix-boot/src/platform.rs`，文件头给了这条推演；`os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo`） | 边界两侧不共享任何代码地址或虚表，传出去的东西不依赖发送方是否还活着 |
| 否：放函数指针 | 契约里带 `fn(...)` 回调 | 指针地址在发送方镜像里解析、函数体在另一个包里；发送方内存被回收后指针悬垂，接收方一调用就跳进无效地址 |
| 否：共享全局 | 沿用 C 的做法，两侧读写同一个全局 | 需要两个镜像共享数据段全局，而它们各自链接了一份内核代码，`.bss` 是彼此的（见 4.3） |

**交接物的形状由它要活多久决定**：需要活过发送方寿命的东西，不能引用发送方。

### 3.2 接口类型约束到哪一步

**C 的这段流程是一条直线函数体**（2.2 的七行），次序天然固定，不可能漏步。重写为了统一两套固件的差异，引入了一个接口类型（`os/libs/minix-boot/src/boot_shim.rs:trait BootShim`）。它买到了什么、买不到什么，必须说准。接口体只有一条方法：

```rust
pub trait BootShim {
    /// Prepare boot: discover memory, load kernel and boot modules,
    /// allocate pages, build KernelInfo, exit firmware services.
    ///
    /// This is called exactly once, before the kernel sets up paging.
    /// After this call, firmware boot services are no longer available.
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
```

买到的四件事是真实的：两套固件必须提供同名同签名的这一步，缺了编译不过；每个固件的实现是一个整体，不会把一边的清单与另一边的分配页拼起来；可以注入假实现做单元测试；调用方不需要按固件写条件编译。（上面是个**节选**：方法文档后半那三行——讲内核基址与大小由实现自己解析、调用方只报页数——从略，它与本节的论点直接相关：能报的只有页数，其余都由实现体自己决定，类型系统管不到那里。）

买不到的一件事也要说明：**`prepare_boot` 没有接收者（`self` 一个都没有），它只在发送方内部被调用一次，接收方从不持有、也不接收一个"引导垫片对象"。**方法体内部按什么次序调用哪几步、有没有真的退出固件服务、快照拍在什么时机，类型系统都不检查。所以流程次序的保证来自两个更朴素的地方：实现体里那几行的排列，以及运行期断言（4.1 会给出一条）。把接口类型说成"保证流程完整"是超出实现的说法。

### 3.3 谁铺表、谁使能

**C 把两个映射与启用翻译都写在 `pre_init` 里**（2.2、2.6、2.7）。重写的分布不同：发送方一侧只分配页表根那一页与一段连续区（`os/boot-shim/src/uefi_helpers.rs:fn alloc_root_page`、`alloc_bump_region`），**铺表与启用都在内核侧的同一个函数里**（`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`，三步各自注释了对应的 C 函数）。

"名字叫引导垫片的那段代码不建页表"与直觉相反，所以要问它为什么值得这样搬。答案是有两个调用方需要同一段机器状态切换：

| 调用形态 | 谁在用 | 需求 |
|---|---|---|
| 内联：在同一个二进制里直接调用 | x86-64、RISC-V 的生产路径 | 需要"铺表 + 启用 + 目的地已映射" |
| 跨二进制：先建表启用，再跳进独立内核镜像 | ARM64 | 跳转发生之前，翻译必须已经启用且高地址必须已在表里（3.5） |

| 方案 | 结果 |
|---|---|
| 选：放在内核侧的共用函数 | 一条代码路径服务两个调用方 |
| 否：放在发送方一侧 | 跨二进制那条路只能复制一份表内容，两处会漂 |

### 3.4 启用翻译这一步在固件路径上换了语义

**C 的 `vm_enable_paging` 确实是从关到开的动作**（2.7），并且用"先清零再点亮"来对抗上一棒的遗留位。重写在 x86-64 上遇到的是另一种起点：固件自己就在长模式里运行，翻译已经是开着的。实现里那句注释直说这件事（`os/arch/src/x86_64/paging.rs:fn enable`）：

```rust
// Switch CR3 to our own root page table.
// UEFI already runs in long mode (CR0.PG=1) with its own page
// table; we must replace it with ours, which contains both the
// identity and kernel high mappings set up by map_huge().
asm!("mov cr3, {}", in(reg) self.root_paddr);
```

于是"启用"这一步在这里的真正内容是**把页表根换成自己这张**——因为固件那张表里没有 3.3 那两个映射。这带来两点设计后果：

其一，2.2 那条依赖变得更严而不是更松：既然没有"从关到开"的间隙，换根那一刻新表就必须已经完整。

其二，凡是本项目要依赖的位，都必须自己保证，不能假定上一棒设好了。同一段代码里两处都按这个原则写：不可执行位（NX）在换根前置起，因为页表项最高位在不可执行位关闭时是保留位、用到就触发页错误，而本项目的只读与不可执行策略要用它——注释把责任划清，说固件"恰好"开着它，但这个不变量属于自己；写保护位同样自己设，因为固件不保证它开着，没有它就能写只读页。写法上选的是"读回当前值、或上需要的位、再写回"，以便保留固件已经设好的长模式位——这是与 C 相反的**选择**，不是相反的**依赖**。

### 3.5 换根之后仍需取指映射

**2.2 里高地址映射必须先于启用翻译，理由是"接下来要跳去的地方在高地址"。**这个前提在运行期还会被使用一次，而且用法更危险：内核主流程一旦开始为进程建立地址空间，就会切换页表根。如果内核此刻的取指地址在被换掉的那一侧，这次切换就把自己拆台了。

两种解法，各对应一类平台：

| 解法 | 平台 | 做法 | 代价 |
|---|---|---|---|
| 把取指窗口重放进新表 | x86-64 | 内核被静态链接进发送方镜像，实际从低物理地址执行，因此要求运行期页表以"虚拟等于物理、只给特权级"重放这些窗口；这些窗口由契约里的一个字段带给内核（`os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo` 的 `reserved_regions`） | 契约要新增一类字段，且运行期页表构造方必须照它补映射 |
| 跳进不参与切换的那一侧 | ARM64 | 两个转换基址寄存器按地址位分流，高地址走其中一半；发送方建表并启用翻译后，绝对跳进独立内核镜像的高地址入口，此后内核全部在高半区运行（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`，文件说明见 `os/libs/minix-boot/src/handoff.rs`） | 需要第二个二进制与一份跨二进制的交接物（4.3） |

两条路回答的是同一个问题，所以"换根之后内核还要有取指映射"是不变量，选哪条是**选择**。RISC-V 与宿主测试在 `reserved_regions` 上传空切片，因为那些平台不把内核链接进发送方镜像。

### 3.6 三种平台回答同一个问题（回指 1.1 的两种交接形状与 2.3 的内存清单来源）

先立统一模型再展差异，顺序不能反过来：**三种平台都要问固件"内存布局是什么、文件谁来替我读"（这是 1.1 与 2.3 一直在说的那件事），然后交付同一形状的启动信息，并在同一处完成翻译启用。**

| 环节 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| 信息从哪问 | 统一可扩展固件接口 | 统一可扩展固件接口 | 监督器调用接口移交 + U-Boot |
| 平台信息来源 | 高级配置与电源接口表根指针 | 设备树优先，接口表回落 | 只有设备树（来自移交时的寄存器） |
| 谁替它读文件 | 固件的文件协议 | 固件的文件协议 | U-Boot（`fatload` + `bootelf`） |
| 交接形态 | 同一二进制内直接调用 | 跳进独立内核镜像 | 同一二进制内直接调用（生产由内核镜像自引导） |

差异集中在"从哪问、谁代读"，契约形状与三步映射没有分支——这正是 3.3 那个共用函数的效果。

有一条硬约束解释了 RISC-V 为什么不与前两者共用固件接口那条路：工具链没有该架构的固件接口构建目标，也发不出对应的可执行格式（PE 是一种可执行文件格式，Windows 上的 `.exe` 与该接口世界里的 `.efi` 引导应用都用它；固件只肯加载它能解析的那种二进制）。**因此即使 U-Boot 实现了该接口的一个子集也用不上，因为造不出那个能去消费它的二进制。**测试面能看到这条链的真实形状（`os/qemu-tests/test-riscv64-uboot.sh` 生成的脚本里就是 `fatload virtio 0 ${loadaddr} boot.elf` 再 `bootelf`）。

最后一条属于"读代码时的判别方法"：`os/boot-shim/src/opensbi_helpers.rs:fn build_memmap` 里那份单区硬编码清单**不是**当前 RISC-V 的生产引导面——它的文档注释直说是为 QEMU virt 构造硬编码清单。生产引导由内核镜像自己充当引导体（`os/kernel-image/src/bootface.rs`），复用同一批构件，但改成从移交时带来的设备树指针解析内存布局。**"库里有这段代码"不等于"生产走这段代码"。**

---

## 4. 实现详解

本章每节标注它落实 3.x 的哪条决策；每跳给真实代码行与符号锚点。

### 4.1 统一可扩展固件接口路径的走查（落实 3.1、3.2、3.3）

入口只有一行（`os/boot-shim/src/main.rs`）：

```rust
let result = UefiBootShim::prepare_boot(1024);
```

参数是预留给引导期建表的页数。实现体是 `os/boot-shim/src/uefi_helpers.rs:impl UefiBootShim::prepare_boot`，按源码语句顺序取每步最关键的一行（步与步之间的其它行以省略号表示，承载语义的三处在下方单独展开）：

```rust
let root_page = alloc_root_page();
let (bump_base, bump_end) = alloc_bump_region(bump_pages);
...
let kern = loader::load_kernel_with_loader(&file_loader).expect(...);
...
let boot_modules =
    loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);
...
let platform_sources = find_platform_sources();
...
let (memmap, reserved_regions) = build_memmaps();
uefi::println!("boot-shim: memmap ok (final pre-EBS snapshot)");
...
assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);
...
let kernel_info = build_kernel_info(
    memmap, reserved_regions, ..., PhysBytes(0), 0, platform_sources,
);
...
exit_boot_services();
crate::raw_serial("boot-shim: [raw] boot services exited\r\n");
```

**三处承载语义的地方：**

**一、快照的时机。** 固件的空闲内存报告是当前状态的快照：已分配的页在活图里被改标为引导加载数据一类的用途，"只取可用内存"的视图自然排除它们。所以快照必须晚于**全部**引导期占用——不只晚于页表根与连续区，还要晚于内核镜像段与每个模块段。若拍早了，正在使用的页会以"空闲"名义出现在交给内核的清单里，页帧管理随后就能把这些页再分配出去，而它们上面正放着在用的页表。

紧跟快照的两条检查把这条依赖钉住（`os/boot-shim/src/uefi_helpers.rs:fn assert_bootstrap_outside_memmap`）：

```rust
fn assert_bootstrap_outside_memmap(memmap: &[MemoryRegion], base: u64, len: u64) {
    for r in memmap {
        let rbase = r.base.0;
        let rend = rbase + r.len as u64;
        assert!(
            base + len <= rbase || rend <= base,
            "conventional memmap [{rbase:#x}, {rend:#x}) overlaps bootstrap allocation [{base:#x}, {:#x})",
            base + len
        );
    }
}
```

判据形式与 2.5 那条一样是半开区间不相交。**但它只检查页表根与连续区两笔**，不检查内核镜像段与模块段。原因是本项目的装载模型：内核与模块由引导垫片**直接写进可用内存**，不经固件分配，因此在固件活图里它们仍然是"可用"、必然出现在清单里；把它们一起断言会立刻误报一个预期形状。它们由内核侧按引导占用逐段扣除。

> **这里停住**：那个逐段扣除发生在哪一行、扣完的清单怎么交给页帧管理，本篇不讲——它回答的是"运行期谁能动这张清单"，属于页表与内存篇。**现在只需要一个结论：同一张清单被两组规则保护，一组靠分配与快照的先后，一组靠扣除。**

**二、`PhysBytes(0), 0` 这两个实参**，对应 C 的 `bootstrap_start` 与 `bootstrap_len`（2.6 末提到的 `freepde_start` 同族字段，定义在 `param.h:kinfo`）。C 用这一对标记"内核中一小节不参与分页的代码"，引导完成后归还。重写里这一节不存在：内核自第一条指令起就在高半区、从不关着翻译跑，那段蹦床在另一个镜像里而不在内核里。所以长度必须为 0，而且这个"必须"被写进断言——`os/libs/minix-boot/src/kernel_info.rs:impl KernelInfo::validate` 要求 `bootstrap_len == 0`。留着非零反而危险：一次过量的归还把固件、设备树、引导垫片自己都扫成可用内存。

这正是 3.1 那条原则的落地形态之一：字段的语义变了，就把它由"传递数据"改成"由构造方与断言保证"。

**三、退出服务之后的那行输出。** `exit_boot_services()` 之后改用直接写串口的方式，不再调用固件的文本输出服务：能力随服务退出而消失，而这一步之后恰恰最需要看得见。

### 4.2 内核侧的三步（落实 3.3、3.4）

`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`，第一步（过渡映射）：

```rust
let mut paging = P::new_from_page(root_page);
...
let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
let mut addr: u64 = 0;
while addr < IDENTITY_MAP_END {
    // Ignore errors — some ranges may not be backed by physical memory,
    // but that's fine; the CPU won't access them.
    let _ = paging.map_huge(
        VirBytes(addr), PhysBytes(addr),
        kern_huge as usize, id_flags,
    );
    addr += kern_huge;
}
```

对照 2.6，三个决定各自回答一条约束：

**覆盖范围**：与 C 一样整段低地址全铺，但 64 位的理由写在紧挨着的注释里——固件加载器的代码与数据可能落在没有标成"可用内存"的那些用途段中，只铺可用段的话，换根之后的第一条指令就故障。要覆盖的对象因此不只是内核自己的代码与栈，还包括发送方镜像自身。

**可执行位**：换根后立刻取指，没有可执行权限取不到。

**只给特权级**（`kernel_read_write`，不带用户可访问位）：这是 2.6 那个"用户位无所谓"的观察在 64 位多架构下的变化。依据是 RISC-V 三级分页模式的机制：**带用户位的页，监督器模式不能取指**，除非把状态寄存器里的允许监督器访问用户页位（SUM）打开，而引导期没有设它。这一条属于架构规范层面的事实，本仓库的证据是那段注释与位选择本身；要亲眼验证它，需要在 RISC-V 真机上把一个带用户位的可执行页交给监督器取指。**同一个语义（过渡映射只供内核自己使用）在不同架构上落成不同的位组合。**

而 C 用来判断“哪段该禁缓存”的高水位线，在这里不再需要：整段低地址统一一组权限位，既不为设备窗口特殊处理，也就没有那条线可依赖——`mem_high_phys` 这个字段因此从契约里消失（2.6 的依赖不是变错了，而是不再被需要）。

第二步（目的地映射）：

```rust
let kern_virt = kernel_info.kern_virt_base().0;
let kern_phys = kernel_info.kern_phys_base().0;
if kern_virt != kern_phys {
    let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    ...
```

那个"虚拟不等于物理才做"的判断不是可省的分支：两者相等时第一步已经把目的地铺好，再写一遍会在同一组中间层表项上写两次，在 ARM64 与 RISC-V 的多级页表里两次写入可能落在同一槽位却按不同页大小解释（源码注释点名的是 riscv64，ARM64 同理风险），结果破坏第一步铺好的恒等段。链接布局本身到下一篇（高半区内核）讲，这里只需要知道它守的是"别把刚铺好的段拆掉"。

第三步（启用）对应 `os/arch/src/x86_64/paging.rs:fn enable`。3.4 只引了换根那两行，这里把四步的关键行铺开（注意每步都是"先读回当前值、再置需要的位、再写回"；块里的 `(1)…(4)` 步骤注释是本文加的，源码原注释是英文）：

```rust
const MSR_EFER: u32 = 0xC000_0080;
const EFER_NXE: u64 = 1 << 11;
let mut efer: u64;
// (1) 先置不可执行位（页表项最高位才能用作 NX）
asm!("rdmsr", in("ecx") MSR_EFER, out("rax") efer, out("rdx") _, options(nostack));
asm!("wrmsr", in("ecx") MSR_EFER, in("rax") efer | EFER_NXE, in("rdx") 0u64, options(nostack));
// (2) 换页表根
asm!("mov cr3, {}", in(reg) self.root_paddr);
// (3) 读回控制寄存器，再置分页位与写保护位
let mut cr0: u64;
asm!("mov {}, cr0", out(reg) cr0);
cr0 |= 1 << 31;
cr0 |= 1 << 16;
asm!("mov cr0, {}", in(reg) cr0);
// (4) 按 CPU 特性开全局页位（不开则带全局标记的条目无意义）
if cpu_supports_pge() {
    let mut cr4: u64;
    asm!("mov {}, cr4", out(reg) cr4);
    cr4 |= 1 << 7;
    asm!("mov cr4, {}", in(reg) cr4);
}
```

第 4 步把本章前面接回来：`kernel_read_write` 带全局标记，而注释说明全局页位不开时该标记无效，开了之后带标记的内核映射能撑过换页表根。

### 4.3 两条交接路径（落实 3.1、3.5）

**同一二进制内的路径（x86-64、RISC-V）。** 契约由 `os/kernel/src/lib.rs:fn arch_boot` 接手（同一个名字按架构有三份定义，本节说的是 x86-64 那一份），它做的第一件事是把这个结构按值收进内核自己的数据段全局，之后一律使用全局副本的引用：

```rust
store_kernel_info(kernel_info);
let kinfo: &'static KernelInfo = crate::kernel_info()
    .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
```

依据在调用约定上：**x86-64 的第一个整型参数寄存器是调用方保存的**，被调用方可以复用它。中间经过固件的代码生成时，那条寄存器可能被跨调用覆写；若把发送方栈上那个引用经一段不再返回的内联汇编直传下去，寄存器一旦被覆写，下一步拿到的就不是契约而是恰好留在其中的值——可能指向一段只读字符串数据，校验函数读到一串字符就中止。按值收进数据段，等于把依赖从寄存器手上换成链接期常量：全局副本的地址由链接期决定，不经过寄存器交接。

第二件事是搬字节。`store_kernel_info` 里为三类清单各配了一块专用存放区（`os/kernel/src/globals.rs`，下面是**签名示意**：名称与类型取自源码，数组包装与长度常量部分省略）：

```rust
pub(crate) static RESERVED_REGION_STORE: SyncUnsafeCell<
    [minix_boot::MemoryRegion; RESERVED_REGION_STORE_LEN], ...>;
pub(crate) static MEMMAP_REGION_STORE: SyncUnsafeCell<
    [minix_boot::MemoryRegion; MEMMAP_REGION_STORE_LEN], ...>;
pub(crate) static BOOT_MODULE_STORE: SyncUnsafeCell<...>;
```

三处的理由相同：`reserved_regions`、`memmap`、`boot_modules` 都是切片，指针部分指向发送方向固件要来的内存池，**而那片池只被固件自己的恒等表映射着**。内核跳进高半区之后，它自己的页表不重放那层映射，同一个虚拟地址读出来是零。所以把字节搬进内核数据段——那块内存整个运行期都在映射里——再把指向它的指针交出去。

**这里能看清 3.1 那条原则的限度：值语义是必要条件，不是充分条件。**契约里放的是值，不代表值里的指针在对面还有效。

**跨二进制的路径（ARM64）。** 发送方建表并启用翻译之后，分配一页把交接对象写进去，用绝对跳转进入独立镜像的高地址入口（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`）：

```rust
let handoff = minix_boot::BootHandoff {
    kernel_info: *kernel_info,
    root_page: root_page.0,
    bump_base,
    bump_next,
    bump_end,
};
unsafe { core::ptr::write(blob_phys.0 as *mut minix_boot::BootHandoff, handoff) };
...
unsafe {
    core::arch::asm!(
        "isb",
        "br {entry}",
        in("x0") blob_phys.0,
        entry = in(reg) entry,
        options(noreturn)
    );
}
```

为什么要多这一跳，3.5 已经给出理由（内核若内联跑在参与切换的那一侧，第一次切换地址空间会毁掉自己的取指映射）。这里补三条实现层面的事实：

- 两个镜像各链接一份内核代码，**不共享数据段全局**，所以发送方在物理内存里建立的一切都要经这块交接对象承载（说明在 `os/libs/minix-boot/src/handoff.rs` 文件头）。
- `x0` 的绑定写在汇编块外面（`in("x0")`）而不是在块里手写一条 `mov x0, …`：注释给的理由是后者构成一次未声明的破坏——编译器有权把跳转目标 `{entry}` 也分配到 `x0`，那句移动会先把跳转目标覆写掉。
- `br` 前面的 `isb`：这段代码没有为它留注释，本篇标明依据来自 ARM 架构手册对指令同步屏障的定义（丢弃流水线里已预取的指令，保证下一条取指发生在先前对系统寄存器的修改生效之后）。放在这里读得通：紧前面刚把页表根写进两个基址寄存器，若跳转目标已被预取，它会用旧映射去解算那个高地址，而旧映射里没有它。要亲眼验证它，需要在真机上观察去掉 `isb` 之后能否进入高半区。

交接对象里的静态切片为什么还能读，文件头也给了条件：**那份池在退出服务之后的引导窗口内仍存活，而独立镜像会在第一次切换页表根之前把这些切片读完。**于是 3.1 那句话得到完整形式——跨边界的数据不仅要全是值，还有一段有效期，到期时刻由第一次换根决定。

### 4.4 契约字段的形状（落实 3.1）

2.3 与 2.6 提到的那个全局结构定义在 `minix3/minix/include/minix/param.h:kinfo`（四段混装：固件直接给的、推导的、引导阶段填的、运行期杂项）。对照 `os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo`：

| 类别 | C | Rust | 为什么 |
|---|---|---|---|
| 保留 | `memmap[MAXMEMMAP]` + `mmap_size` | `memmap: &'static [MemoryRegion]` | 语义不变（可用物理段清单），长度不再靠数组大小与计数分别维护 |
| 保留 | `param_buf[1024]` | `param_buf: &'static [u8]` | 引导参数缓冲的对外行为不变，上限由断言守 |
| 改型 | `freepde_start`（一个目录项号，2.6 的返回值） | `free_upper_idx: Option<usize>` | "第一个空闲目录项"在多级页表里没有通用意义，改成可缺省索引 |
| 改型 | `bootstrap_start`、`bootstrap_len` | 同名两字段，被断言为 0 | 4.1 第二处：那一节代码在此不存在 |
| 改型 | 链接期符号（内核物理基址、大小）、模型特定寄存器里的系统调用入口 | 升为正式字段 | 跨二进制交接时不能依赖对方的链接期符号，必须显式递值 |
| 淘汰 | `mbi`（整份外部结构）、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`user_end` | 不存在 | `mbi` 是解析中间产物（3.1 之后不再需要"拥有别人的结构"）；高水位线不再被需要（4.2）；重叠体检改由装载阶段独立函数完成 |
| 淘汰 | `do_serial_debug`、`serial_debug_baud`、`kmessages`、`minix_panicing`、`nr_procs`、`release`、`version` | 不存在 | 早期输出与内核消息缓冲改由接口承担，属后续篇 |
| 新增 | — | `reserved_regions: &'static [MemoryRegion]` | 3.5 的 x86-64 解法要交给内核重放的窗口清单 |
| 新增 | — | `platform_sources: &'static [PlatformDescSource]` | 3.1 的分类标签加物理地址 |

骨架视图（字段按声明顺序，`repr(C)` 固定布局）：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub reserved_regions: &'static [MemoryRegion],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub free_upper_idx: Option<usize>,
    pub user_sp: VirBytes,
    pub kern_stack_top: VirBytes,
    pub syscall_entry: VirBytes,
    pub boot_modules: &'static [BootModule],
    pub bootstrap_start: PhysBytes,
    pub bootstrap_len: u64,
    pub platform_sources: &'static [PlatformDescSource],
    pub param_buf: &'static [u8],
}
```

`validate` 守住的几条（`os/libs/minix-boot/src/kernel_info.rs:impl KernelInfo::validate`）：引导代码段长度为 0、内核大小非零、内核栈顶 16 字节对齐、内存清单非空、模块清单非空、参数缓冲不越上限。另有一条编译期断言把交接对象的尺寸钉在一个页以内（`os/libs/minix-boot/src/handoff.rs`），因为它必须装进一页物理内存。

### 4.5 RISC-V 的生产路径（落实 3.6）

`os/kernel-image/src/bootface.rs` 文件头说明这一面：内核镜像自己充当引导体，从移交时带来的设备树指针解析内存布局，复用 `build_kernel_info`、文件表、bump 分配这些构件，并直接调用 `minix_kernel::arch_boot`；它明确弃用 `opensbi_helpers::build_memmap` 的单区硬编码（指其生产不可用）。这一段适用 3.6 那条判别：**先确认生产走的是哪个入口，再读库代码。**

---

## 5. 测试要点

### 5.1 语义 ↔ 用例映射

数量现算，不抄表：

```bash
cd os && rg -c '#\[test\]' boot-shim/src/loader.rs boot-shim/src/uefi_helpers.rs \
    boot-shim/src/opensbi_helpers.rs libs/minix-boot/src/kernel_info.rs \
    libs/minix-boot/src/platform.rs
```

读数：`loader.rs` 13、`opensbi_helpers.rs` 13、`uefi_helpers.rs` 1（引导垫片包合计 27）；`kernel_info.rs` 9、`platform.rs` 8（`minix-boot` 包合计 17）。

| 本篇的约束 | 真实用例 | 能证明什么 |
|---|---|---|
| 契约字段必须成立（4.4） | `test_validate_accepts_valid_info`，以及 `os/libs/minix-boot/src/kernel_info.rs` 里六条反向用例（引导代码段长度非零、内核大小为零、栈未对齐、内存清单为空、模块清单为空、参数缓冲越界各一条） | 每条断言都有对应的反向用例，改坏任一条会红 |
| 首尾相接不算重叠（2.5） | `os/boot-shim/src/loader.rs` 的 `test_first_overlapping_pair_abutting`，同组还有分离、重叠、返回首个、空表四条 | 端点约定被单独固化；有人改成闭区间比较就会被拦下 |
| 模块按约定顺序装入、缺失即中止（4.1） | `test_load_boot_modules_loads_all_in_contract_order`、`test_load_boot_modules_panics_on_missing` | 交出去那份清单与装载协议一致 |
| 加载后未初始化段被清零 | `test_bss_region_is_zeroed_after_load` | 内核镜像不继承上一段内存的残留 |
| 清单构造与分配区自洽（4.1） | `test_build_memmap_default_region`、`test_alloc_bump_region_round_trip`、`test_alloc_root_page_is_4k_aligned` | 根页对齐与分配区首尾闭合可在宿主侧检查 |

### 5.2 只能真机验证的部分

三类语义宿主测不到，各自回指一处约束：**快照与分配的真实先后**（4.1 第一处，依赖固件活内存图）、**换根之后首次取指能否命中**（4.2 与 3.5，依赖真实页表根与地址转译缓冲行为）、**控制位在硅片上的效果**（4.2 第三步与 `isb`，依赖处理器）。这些由三架构的真机回归覆盖（`os/qemu-tests` 下按架构分列的脚本），门径属测试篇。

### 5.3 覆盖盲区（如实披露）

`os/boot-shim/src/main.rs`、`os/boot-shim/src/lib.rs`、`os/libs/minix-boot/src/boot_shim.rs`、`os/libs/minix-boot/src/handoff.rs` 里一条单元测试都没有（用同一命令复核即为空）。分布本身合理——入口与跨二进制载荷都属于"只在真机上成立"的部分——但它同时意味着 4.3 那条"必须先按值收进全局"的守卫**没有自动化回归保护**，靠代码形状与注释维持。本篇不把它说成已覆盖。

---

## 6. 诊断：起不来时先看哪一步

引导期失败的共同形状是"没有输出，机器复位或静默死循环"。定位方法是把断点认到 4.1 那条链的两个里程碑之间，而不是整段重试。

两条先做的检查（可直接复制）：

```bash
# 一：确认交权前的动作序列与守卫都在位
rg -n "build_memmaps\(|assert_bootstrap_outside_memmap|exit_boot_services|\
build_bootstrap_root_and_enable|store_kernel_info|validate\(" \
    os/boot-shim/src/uefi_helpers.rs os/kernel/src/lib.rs

# 二：确认契约递出的清单条数（内核侧第一行打印的正是这些字段）
rg -n "arch_boot arg=|blen=|reslen=|KernelInfo:" \
    os/kernel/src/lib.rs os/libs/minix-boot/src/kernel_info.rs
```

预期输出是一条里程碑序列：进入准备阶段 → 分配完成 → 装入内核 → 装入模块 → 清单可用（快照已拍）→ 构造契约 → 退出固件服务 → 内核侧第一行。日志断在哪两行之间，问题就在对应的两步之间。

三类典型故障：

**启用翻译（或换根）之后立刻复位。** 查当前执行所在那段映射的覆盖范围：正在执行的代码地址是否落在 4.2 第一步那个循环铺到的区间内，特别看有没有把"没被标成可用内存"的那些用途段算进去——只铺可用段是这类故障最常见的成因。若走的是 3.5 的 x86-64 那条路，还要确认运行期页表是否重放了内核实际执行所在的窗口。

**第一次切换地址空间之后复位。** 与上一类方向不同，别混着查。此刻旧表没问题，坏的是新表：内核代码是否还待在参与切换的那一侧（3.5），或者带全局标记的映射是否真的生效（4.2 末：全局页位不开，标记无效）。

**一进校验函数就中止。** 先分辨字段是"没递过来"还是"递过来但读不到"。第二条命令的输出给了可分辨的信号：内核侧第一时间打印契约指针、引导代码段长度、保留窗口条数与首条目。条数为 0 说明丢在跨边界之前；条数对而内容全零，说明载荷指向的内存已不在映射里（4.3 那三块存放区存在的理由）。

**同一类动作在三个架构上的写法**（`os/kernel/src/arch/x86_64/higher_half.rs`、`os/kernel/src/arch/aarch64/higher_half.rs`、`os/kernel/src/arch/riscv64/higher_half.rs`）：每一个都在跳转前把栈顶按 16 字节对齐（`and rsp, -16` / `and sp, sp, -16`），再把帧指针归零。这与 2.1 里那条没有注释的 `push $0` 是同一个约定的两种处理：**C 把对齐当作隐式约定，靠之前几次压栈的个数腾出来；重写把它做成一条看得见的运算。**读者可以拿这个例子记住一条区分：**隐式约定不是不存在，只是没被写下；一旦平台或编译器变了（调用约定的对齐要求、代码模型），它就必须变成显式的。**跳进高半区那一段的完整机制与一个 riscv64 的真机反例属于下一篇（高半区内核），本篇到此为止。

为什么上两段提到的两个守卫（快照顺序、按值收进全局）本身没有回归测试，第 5 章统一说明。

调试入口最省事的仍是串口：引导侧与内核侧都往同一个端口写，退出固件服务之后由直接写端口接管，所以"日志里出现了哪一行"就是"走到了哪一步"。

---

## 收束

本篇得到的东西可以分成两类。

**依赖（换写法也绕不开）**：C 函数被调用之前必须已有栈；中断入口表不存在时中断允许位必须为关；启用翻译之前，新表必须已经覆盖接下来要取指与取栈的地址；判断某段是否为真实内存要先有登记结果；若干镜像不得压在同一段物理内存上，而区间端点的写法决定会不会误判；切除已占用区间必须向外取整到页边界；设备寄存器窗口的映射要禁缓存；向固件问出的内存状态是瞬时值，所以问的时机要晚于自己的占用；跨边界传递的东西不得引用会先消失的一侧；第一次换根之后内核自己还要有取指映射。

**选择（可以不同）**：C 用"先清零再点亮"控制位序，重写在 x86-64 上用"读回再置位"；ARM64 选择跳进独立镜像，x86-64 选择把窗口重放进新表；把内核静态链接进发送方镜像省一次跨镜像装入，代价是需要一个 `reserved_regions` 字段；RISC-V 走监督器调用接口加 U-Boot，因为工具链发不出该架构的可执行格式；用接口类型统一两套固件，买到形状与可测性，买不到流程次序。

下一篇处理本篇当作前提使用的那个东西：内核镜像的链接地址与装载地址为什么不同，以及高半区的目的地是怎么被铺出来的。

---

## 7. 参见

**本篇的 C 真值源**

- `minix3/minix/kernel/arch/i386/head.S` —— 多引导头部与入口汇编（2.0、2.1）
- `minix3/minix/kernel/arch/i386/pre_init.c` —— 入口函数、参数整理、重叠体检与页表页辅助（2.2–2.5）
- `minix3/minix/kernel/arch/i386/pg_utils.c` —— 清单四操作与两个映射、启用序列（2.4、2.6、2.7）
- `minix3/minix/include/minix/param.h` —— 启动信息结构与 `MAXMEMMAP`（4.4）
- `minix3/minix/include/arch/i386/include/vm.h` —— 页表位、控制寄存器位与地址宏（2.9）
- `minix3/minix/kernel/const.h` —— 用户地址上限常量（2.3 第一处）
- `minix3/minix/kernel/glo.h` —— `kernel_may_alloc` 的对外声明（2.3 第二处）
- `minix3/minix/kernel/main.c` —— 分配窗口的开关时机（2.3 第二处）
- `minix3/minix/kernel/arch/i386/arch_smp.c` —— `alloc_lowest` 的唯一调用点（2.4）
- `minix3/sys/arch/i386/include/multiboot.h` —— 多引导魔数与头部字段（2.0）

**本篇的 Rust 实现**

- `os/boot-shim/src/main.rs`、`os/boot-shim/src/uefi_helpers.rs` —— 引导侧入口与准备步序（4.1）
- `os/boot-shim/src/loader.rs`、`os/boot-shim/src/opensbi_helpers.rs` —— 镜像装载、重叠检查与监督器调用接口路径（2.5、3.6、4.5）
- `os/libs/minix-boot/src/kernel_info.rs`、`boot_shim.rs`、`handoff.rs`、`platform.rs` —— 启动契约、接口类型、跳二进制载荷与平台来源（3.1、3.2、4.3、4.4）
- `os/kernel/src/lib.rs` —— 内核侧接手、三步映射与跳入独立镜像（3.3、3.5、4.2、4.3）
- `os/kernel/src/globals.rs` —— 三块专用存放区（4.3）
- `os/arch/src/x86_64/paging.rs`、`os/arch/src/arm64/paging.rs`、`os/arch/src/riscv64/paging.rs` —— 三个架构的启用动作（3.4、4.2）
- `os/kernel-image/src/bootface.rs`、`os/kernel-image/src/main.rs` —— 独立镜像自引导面与入口（4.3、4.5）
- `os/qemu-tests/test-riscv64-uboot.sh` —— U-Boot 代读文件的真实例子（3.6）

**相关主题篇**（尚未生成的以主题名列出，不给出链接）

- 高半区内核篇：链接地址与装载地址为何不同、栈与程序计数器的成对跳转（本篇 2.2、4.2 当作前提使用）
- 保护结构篇：中断入口表与任务状态段（本篇多处用到"此刻它们还不存在"）
- 平台发现篇：设备树与高级配置与电源接口表解析成描述符（3.1 交出的分类标签在那里被消费）
- 页表与内存篇：运行期页表页分配、引导占用的逐段扣除（4.1 故意停住之处）
- 多核启动篇：辅助处理器跳板（2.4 里 `alloc_lowest` 的唯一调用场景）

**外部规范来源**（本篇仅两处引用仓库外的依据，均已就地标明）

- 统一可扩展固件接口的镜像格式与调用约定（3.6）
- ARM 架构手册关于指令同步屏障与转换基址寄存器的定义，以及 RISC-V 特权规范关于监督器访问用户页的限制（3.4、3.5、4.2）
