# 01 · 从固件到内核：交权之前，机器必须被变成什么样

> **C 真值源**：`minix3/minix/kernel/arch/i386/head.S`、`minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/arch/i386/pg_utils.c`
> **Rust 实现**：`os/boot-shim/src/main.rs`、`os/boot-shim/src/uefi_helpers.rs`、`os/boot-shim/src/loader.rs`、`os/libs/minix-boot/src/{kernel_info.rs,boot_shim.rs,handoff.rs,platform.rs}`、`os/kernel/src/lib.rs`

一篇讲启动的文档很容易写成"第一步做什么、第二步做什么"的清单。这篇不这样写。我们要解决的是两个问题：**为什么内核前面还必须站着一个东西**，以及**为什么打开分页这个动作，必须在它自己脚下的地址还在映射里的时候完成**。看懂这两点之后，代码里那些看似多余的动作、看似任意的顺序，都会变成唯一可能的形状。

---

## 第 1 章　概念：交出去之前，机器必须先变成什么样子

### 1.1 递梯子的人会消失

一台刚上电的机器并不"欠"内核任何东西。处理器复位后先执行固件，固件做完硬件自检、找到可引导介质，然后把执行权交出去。内核镜像此刻已经躺在物理内存里，但它还没有页表、不知道自己被放在哪个地址、也不知道还有哪些内存可以用。

在 32 位的 Minix3 时代，这段空缺由引导加载器 GRUB 填平：GRUB 读取配置、把内核二进制和一批启动模块（进程管理、虚拟内存、文件系统、根服务这些用户态服务器的镜像）搬进物理内存，切换到保护模式，再跳进内核入口，同时用两个寄存器递来一份已经整理好的启动信息。内核要做的只是**解析**别人给它的清单。

到了 64 位、多架构的世界，现代固件不再给这份清单。统一可扩展固件接口（UEFI）和开源监督器接口（SBI，RISC-V 的固件调用约定）只提供**可以调用的服务**：你可以问固件"内存布局是什么"、可以让它读文件、可以拿到一块标好用途的内存页，但固件不会替你把内核和模块搬好、再塞给你一张现成的布局表。

于是一个新问题出现了：必须有一个组件，负责把"能从固件问出来的信息"变成"内核可以直接消费的启动契约"。这个组件在本项目里叫引导垫片（boot-shim）。

它的历史位置可以用一句话概括：**GRUB 的那份工作，被搬进了内核自带的一段引导代码里。**

而这个组件有一个别的组件都没有的处境。它交付完信息之后，就必须把自己脚下的梯子抽掉——退出固件服务、让自己的内存变得可以回收。也就是说：

**它交出去的东西，不能依赖它自己还活着。**

这句话是整篇的轴心。第 3 章讲启动契约的数据形状、第 4 章讲交棒方式时，所有的取舍都是它的推论。现在先看机器缺了什么。

### 1.2 一台刚被交权的机器处于什么状态

控制权重交到引导代码手上时，机器大致是这样（以 32 位 Minix3 的交接点为例，架构无关的部分加粗）：

| 维度 | 状态 |
|---|---|
| **特权级** | 已在高特权级（x86 的保护模式、ARM 的 EL1、RISC-V 的监督器模式），但还没有分页 |
| 寄存器 | 带着固件给的入口信息：GRUB 给的是魔数与结构地址，UEFI 给的是镜像基址，SBI 移交给的是设备树指针 |
| **物理内存** | 内核与模块已经被搬进去，但布局要靠引导代码自己去问、自己去整理 |
| **页表** | 不存在 |
| 堆与分配器 | 不存在 |
| **中断入口** | 不存在 |

这张表里有一处容易滑过去的区别，值得单独说：

**"页表不存在"和"中断入口不存在"是两类完全不同的缺失。**

前者是同步问题——只要处理器开始用虚拟地址取指令、取栈，而那张表还没建立或者没覆盖到这个地址，错误就在**当前这条指令**上发生，躲不开。后者是异步问题——设备的事件进不来，机器不会当场出问题，只是永远不会响应。

于是我们知道：这段引导代码真正的紧迫之处不在"让外设能通知内核"（那是后续文档的事），而在**它自己每走一步都在用地址，而地址的来源还没建立**。

> **认知减负**：表里"保护模式"是 x86 的术语，但它背后的语义——内核跑在高特权级、用户态跑在低特权级——是所有架构共享的。那些讲"自己写操作系统"的书里大段出现的实模式、A20 地址线、分段机制，是 8086 与 80286 时代为兼容性打的补丁：分段是 x86 独有的寻址方式，ARM 一样也没有，照样跑操作系统。而且这些都被固件处理完了，引导代码的第一条指令就已经跑在高特权级里。**值得搞懂的是"为什么需要特权级隔离"，本篇不必深究"怎么加载全局描述符表寄存器"和"A20 怎么打开"**——那是固件与引导加载器的领地，不在引导垫片的职责里。

### 1.3 机制图：一条链上谁必须晚于谁

把"缺的东西"补上的顺序画出来，才能看清哪几步是硬依赖：

```
固件（BIOS+GRUB / UEFI / SBI 移交）
  │  交来：执行权 + 能从固件问出的原始信息
  ▼
① 入口：把执行环境降到最小可确定状态（栈、标志）
  │
  ▼
② 信息整理：把别人的结构变成自己的数据（内存布局 + 模块清单）
  │
  ▼
③ 建过渡映射：虚拟地址 = 物理地址（覆盖正在执行的这段代码所在的地址）
  │
  ▼
④ 建目的地映射：内核该待的高地址一并铺好
  │
  ▼
⑤ 打开分页（此刻起地址开始被翻译）
  │
  ▼
⑥ 换到最终栈、交棒给内核主流程
```

这张图上有一处次序看起来最没必要：**⑤ 之后紧跟的不是"开始干活"，而是 ⑥ 换栈。**

原因恰恰在 ③ 的性质上：过渡映射保证虚拟地址等于物理地址，所以"打开分页"这个动作对正在执行的代码**不改变任何地址值**——它是一条无声的开关。既然无声，两条栈切换指令就能安全地紧接着它执行。

换句话说，图上每一步的位置都不是自由的。**那么到底是哪一个东西在决定它自由不自由？** 先把这个问题留着，读完第 2 章再回来回答。

带着这个判断，我们去看 C 原版怎么把它写成一串真实指令。

---

## 第 2 章　C 源码：Minix3 真正怎样把这件事做完

Minix3 内核侧的这段故事集中在两个文件：`pre_init.c`（入口之后第一个 C 函数与它的信息整理）和 `pg_utils.c`（页表与内存账本）。汇编入口在 `head.S`。

### 2.1 汇编入口：内核第一条 C 语句之前的六条指令

先看一个形状：内核入口不是 C 函数，而是一段汇编。**为什么非要有这段汇编不可？** 因为一个 C 函数能跑的前提是**已经有栈**，而固件跳进入口的那一刻并没有替谁准备过栈——它只把执行权与两个寄存器的内容留下来。所以必须先有一段不用栈就能工作的代码，把栈搭起来，才能调用第一个 C 函数。这就是这段跳板存在的全部理由。

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

逐条看它在补什么：

1. `mov $load_stack_start, %esp`：把栈指针指到镜像自己数据段里预留的 4096 字节（同一文件末尾的 `load_stack`）。**此刻没有分页、没有分配器，能用的栈只能是编译期就占好的那块。**
2. `mov $0, %ebp`：帧指针归零。配合下一条 `push $0`——栈底放一个 0，栈回溯走到这里就停。
3. `push $0; popf`：把标志寄存器整体置成 0。源码注释写得很克制——"set flags to known good state"，括注里点明清掉的是嵌套任务位与中断允许位。

第三条值得停下来看，因为它是那种"看似多余"的动作：

**入口为什么要在做任何实事之前，先把中断允许位清掉？** 答案是 §1.2 那张表：中断入口表还不存在。如果固件在交接时把中断允许位留着，而某个设备恰好在这几指令之间发火，处理器会去查一张还没建立的中断入口表——这不是"这次中断丢了"，而是整机复位。所以先把门关上，是最便宜的保险。

这三条铺完，才把 GRUB 留在 `eax`（魔数）和 `ebx`（启动信息结构地址）里的东西压栈，调用 `pre_init`。

但代码块里还有一条没人认领：`popf` 与 `push %ebx` 之间那条**光裸的 `push $0`**。源码在这里没留注释，能确定的只有一条外部约束：x86 的函数调用约定要求 `call` 之前栈顶指针按 16 字节对齐（供被调用方存放 SIMD 宽的局部变量），而 `load_stack_start` 只是个 4096 字节区的末端，前面已经压过的次数会把对齐性破坏掉——**这一下 push 把对齐补回来**。它在本文里标为 `[待验证]`：对齐是对的吗、除此之外还兼不兼做帧哨兵，源码没写，本文也不替它补一个说法。**值得带走的是这个习惯本身：读到一句无注释的多余动作，先把它标成问题而不是当成噪声跳过。**

这里的**压栈顺序看起来是反的**：函数签名是 `pre_init(u32_t magic, u32_t ebx)`，`magic` 在前，可代码却先压 `ebx`、后压 `eax`。把它想透只需要一条约定：参数从右往左依次压栈，所以**最后压进去的那个落在最靠近栈顶的位置，也就是函数拿到的第一个参数**。先压结构地址、后压魔数，魔数才落在第一个参数位上。顺着这条约定可以自己验证：`pre_init` 里那个断言读的是第一个参数，它读到的应该就是刚压进去的 `eax`。

### 2.2 pre_init：一份只有七行的交接清单

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

这份清单短到容易被当成"顺序无所谓"。逐条对上 §1.3 的图：断言魔数是入口约定的确认；`get_parameters` 是 ②；`pg_clear` + `pg_identity` 是 ③；`pg_mapkernel` 是 ④；`pg_load` 写页表根、`vm_enable_paging` 是 ⑤；返回的 `&kinfo` 供调用方做 ⑥。

先看魔数这一条：

**`assert(magic == MULTIBOOT_INFO_MAGIC)` 不是礼貌检查。** 它断言的是"我是被 GRUB 按多引导（multiboot）约定跳进来的"。这个前提一旦不成立，`ebx` 指向的那块内存里根本不是约定的结构，后面所有对它的解析都是在读垃圾。一个 32 位魔数的作用是**给后面一整段信任盖章**。

再看源码自己给出的那条注释："but first a 1:1 mapping so this code stays where it should be"（先做一比一映射，好让这段代码还待在它该在的地方）。这句注释回答的就是 §1.3 留下的那个问题。把它展开成一个假设：

**如果先只映射内核的高地址、然后打开分页，会发生什么？** `pg_load` 把页表根写进控制寄存器、`vm_enable_paging` 置起分页位之后，处理器下一条要取指用的就是虚拟地址。而此刻正在执行的这段引导代码位于低物理地址，它在新的页表里没有条目——于是**当前这条指令之后的取指就失败**。而页错误需要页错误的处理入口，那张表还没建立。结果不是"报错信息"，是复位。

恒等映射把整个低地址段按"虚拟等于物理"铺好，正是为了让"启用分页"这件事对正在跑的代码完全透明。它不是给内核准备的，是给**执行这段代码的自己**准备的。

### 2.3 get_parameters：先拥有数据，再谈别的

`get_parameters` 是整个清单里最花时间的一步，职责是把 GRUB 给的散落信息整理成内核的全局启动信息结构。它的开头两行就把上面那个陷阱处理掉了（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`，节选）：

```c
	multiboot_info_t *mbi = &cbi->mbi;
	...
	/* get our own copy of the multiboot info struct and module list */
	memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));
```

`ebx` 指向的结构属于 GRUB 放在内存里的东西。为什么要立刻抄一份进自己的全局结构？

**因为那块内存不是引导代码的资产。** 引导服务退场之后，那段内存随时可能被别人占用；而内核后面许多步都要回看这份信息。先拥有数据，才谈得上依赖数据。这个动作在 64 位的重写里以另一种形式重现（见 §4.1 的内存映射快照），值得先记住它的理由。

接着是内存布局的两条来源分支（同一函数，节选）：

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

上半段是遍历固件报的内存映射，只登记"可用"的那些段；`mm_size + sizeof(mmap->mm_size)` 这个步进写法是变长条目的遍历约定（下一条目 = 当前条目地址 + 本条目大小 + 那个"大小"字段本身的宽度）。

回落分支里藏着两处化石，值得注意：

**低内存从 0 算起、单位是千字节，高内存硬编码从 `0x100000`（1 MB）算起。** 这不是策略，是早期个人电脑的内存布局：前 1 MB 是 conventional memory，之上才是 extended memory。当固件只肯给"多少 KB 低端、多少 KB 高端"这两个数字时，代码只能按这个历史形状还原。

这一段之后还有几条断言（`mi_flags` 必须含模块、模块数必须大于 0 且不超过上限），然后是模块清单的 `memcpy`——同样是"先拥有数据"。

最后有一处动作，是把"防止互相踩"这件事做到了超出直觉的程度（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`，节选）：

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
		if(overlaps(cbi->module_list, cbi->mods_with_kernel, m))
			panic("overlapping boot modules/kernel");
		cut_memmap(cbi,
			cbi->module_list[m].mod_start,
			cbi->module_list[m].mod_end);
	}
```

注意这个循环遍历的是 `mods_with_kernel`，比固件报的模块数多一个。多出来的那一项就是内核自己。

**内核把自己伪装成"第 N+1 个模块"，然后用检查模块互踩的同一段代码检查自己。** 好处很实际：不必为"内核与模块是否重叠"另写一套逻辑，一套两两查重的代码覆盖了全部镜像。代价是 `module_list` 数组必须多留一格（`assert(k < MULTIBOOT_MAX_MODS)` 守的就是这一格）。

### 2.4 内存账本的四笔操作

`pg_utils.c` 提供四个对"可用内存"这张视图的原始操作。它们合起来回答一件事：**引导期谁在动这张地图，怎么保证动完还自洽。**

第一笔，登记（`minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`，节选）：

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

三个动作各有各的道理：超过 4 GB 的段整段丢弃、跨界的那段截短（32 位寻址的天花板，也是本结构体里这个字段的位宽的天花板）；基址向上取整、长度向下取整，保证登记进来的每一段都是**整页**；顺手更新一条"真实内存最高水位"。

这条高水位线后面会用到，先记下它从哪来：**只有 `add_memmap` 会推高它。**

第二笔，切除（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`，节选）：

```c
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)
{
        ...
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

先看对齐的方向：`start` 向下取整、`end` 向上取整，也就是说**切除区间被向外扩到整页边界**。

**为什么要往外扩而不是往里收？** 因为内核与模块的加载地址不保证页对齐。如果只切除对齐后的内层部分，一个被占了半页的页表页会在账本上仍然显示为可用——而页是按整页分配的，下一次分配就会把一页里已经装着代码的那半块交出去。往里收是省内存，往外扩是保正确。

再看那两条 `if` 补回来的残段：切除区间可能把一段可用内存**劈成左右两半**，所以先作废原条目、再把左残段和右残段分别登记回去。

第三笔和第四笔是两种分配器，取方向相反（`minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest`、`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`，各节选）：

```c
phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len)
{
	phys_bytes lowest = EMPTY;
	...
	for(m = 0; m < cbi->mmap_size; m++) {
		if(cbi->memmap[m].mm_length < len) continue;
		if(cbi->memmap[m].mm_base_addr < lowest) lowest = cbi->memmap[m].mm_base_addr;
	}
	assert(lowest != EMPTY);
	cut_memmap(cbi, lowest, len);
	...
	return lowest;
}
```

```c
phys_bytes pg_alloc_page(kinfo_t *cbi)
{
	...
	for(m = cbi->mmap_size-1; m >= 0; m--) {
		mmap = &cbi->memmap[m];
		if(!mmap->mm_length) continue;
		...
		mmap->mm_length -= I386_PAGE_SIZE;
		...
		return mmap->mm_base_addr + mmap->mm_length;
	}
	panic("can't find free memory");
}
```

`pg_alloc_page` 从**最高段的尾部**倒着取一页，专供引导期建表；`alloc_lowest` 找**够长的最低段**、从头部取一整块，用于需要低地址的东西（本项目里唯一的调用点是辅助处理器的启动跳板，见 `minix3/minix/kernel/arch/i386/arch_smp.c`）。取最高和取最低互补，是为了让引导期那张恒等映射的覆盖范围尽量连续、少被切出碎片。

现在看 `alloc_lowest` 里那个容易被跳过的细节。把两个签名的实参摆在一起：

```c
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end);   /* 第二个实参是"末地址" */
...
	cut_memmap(cbi, lowest, len);                               /* 传进去的是"长度" */
```

`cut_memmap` 的形参是 `start` 与 `end`，而这里传的是基址与**长度**。对照 §2.4 第一笔里那三行边界判断（`substart >= subend` 就跳过）：**除非最低可用段恰好从地址 0 开始，否则 `len` 作为末地址会小于 `lowest` 这个起始地址，循环里每一段都会被跳过，账本上那块地址并没有真的被摘掉。**

这就是"名字看起来像做 X，实际只做 X 的前半"最典型的一例：函数确实返回了一个正确的最低地址、也确实记了一笔分配量，但**保留**这半件事只在最低段基址为 0 时成立。读到这里可以自己验证一下：如果最低段从 `0x100000` 开始、要分配 4 KB，`substart` 和 `subend` 各是什么值。

（重写没有保留这个分配器：RISC-V 与 x86 的辅助处理器早期入口是把可用图扫一遍来挑地址，注释里指回 C 的 `alloc_lowest`，见 `os/arch/src/x86_64/ap_early_entry.rs`。）

第四笔之外的最后一个细节在查重宏上（`minix3/minix/kernel/arch/i386/pre_init.c:overlaps`，宏定义）：

```c
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
			INRANGE(mod1, mod2->mod_end-1))
```

第二个宏判断"另一个模块的**末地址减一**是否落在本模块区间内"。

**为什么是 `mod_end - 1` 而不是 `mod_end`？** 因为区间是左闭右开的：模块 A 占据 `[start_A, end_A)`，模块 B 若从 `end_A` 开始，两者首尾相接、谁也没踩谁。如果这里比较的是 `mod_end`，那么"紧接着摆放"的合法布局会被判成重叠、当场 `panic`。一个 `-1` 承担的是半开区间约定的最后一环——而这类边界约定，在重写里被单独写成了一个测试用例（见 §5）。

### 2.5 铺恒等映射与内核高映射：两个标志组的不对称

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

四条观察，各自成一段。

**第一条：函数体第一件事是一个断言。** `assert(cbi->mem_high_phys)` 断言的是"高水位线已经被推高过"。对照 §2.4 第一笔：那条线只有 `add_memmap` 会写。也就是说，**必须先登记完可用内存，才能铺这张映射表**——这不是风格约束，是数据依赖：铺表时要用这条线判断"哪些段是真实内存"，所以账本必须先有内容。

**第二条：这个循环铺满整个目录，而不是只铺有内存的地方。** 每一页目录项对应一个 4 MB 大页，从地址 0 一直铺到 4 GB，虚拟等于物理。

**第三条：越过真实内存高水位的那些段，额外加上两个禁用缓存的位。** 源码注释给出了理由，翻译过来是"把不对应物理内存的地址按不可缓存来映射"。高水位之上的地址并不是"空的内存"，而是**设备寄存器窗口和地址空洞**。对设备寄存器开缓存会破坏"写完立刻读回来看状态"这类语义：写可能滞留在缓存里没落到设备，读可能拿到旧值。所以这段用写穿透位与不可缓存位把缓存关掉。

**第四条，也是最值得留意的一条：这里的标志组带着一个用户可访问位。** 而紧邻的下一个函数没有（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel`，节选）：

```c
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
	        pagedir[pde] = kern_phys | I386_VM_PRESENT |
			I386_VM_BIGPAGE | I386_VM_WRITE;
		...
	}
	return pde;	/* free pde */
```

`pg_identity` 的标志是"存在 + 大页 + 用户可访问 + 可写"，`pg_mapkernel` 的是"存在 + 大页 + 可写"——**内核自己的高地址映射刻意不带用户可访问位**。两处挨在一起、只差一个位，第一眼很像笔误。

**它不是笔误，而且不对称的方向恰好说明这两张映射为谁服务。** 引导期处理器跑在最高特权级，页表项上的用户位对最高特权级的访问本身不生效——所以给恒等映射加那一位，既不多保护什么也不多允许什么，是个"无所谓"的位。真正表达意图的是内核高半区那一位的**缺席**：它把内核地址标成只有特权级能访问。

那么那份"用户可访问"的恒等映射会不会留给用户态用？也不会。虚拟内存服务在建自己的页表时会显式跳过大页条目（`minix3/minix/servers/vm/pagetable.c:pt_init`），另起一张表。也就是说：**引导期这张恒等映射的寿命只到内核把地址空间交给虚拟内存服务为止**，它服务的是"正在执行的这段引导代码"，不是以后的进程。

一个映射的属性是给"以后谁来访问"定的，而不是给"现在正在跑"定的——这个区别在第 4 章看 Rust 侧时会变得关键。

### 2.6 开启分页的位序

`minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`（节选到关键处）：

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

`pg_load` 在此之前已经把页目录的物理地址写进页表基址寄存器（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_load`）。这一段则是一串对控制位的操作，它的价值全在**顺序**上。

把源码给出的两条理由并排看：一条是"全局页位必须在分页位之后开"，另一条是"我们的页表里有 4 MB 大页条目"——后者意味着大页位必须**在分页位之前**开，否则刚打开分页就会按 4 KB 页的格式去解释那些大页条目。

于是"先关掉分页、再把全局页与大页位清零、然后重读、再依次点亮"这一段就不是繁琐，而是**唯一能让结果与固件残留状态无关的写法**：

**先把可能被人遗留的位归零，再按依赖顺序点亮，才使"启用分页"在任何引导器接手的机器上得到同一个结果。**

顺带两条：`assert(cr0 & I386_CR0_PE)` 是引导约定的第二条确认（固件应已把处理器置于保护模式）；写保护位与全局页位是在分页已经打开之后才点的，前者给内核只读段加保护、后者按 CPU 特性开关（`pgeok`）决定，不支持就不点。

### 2.7 从观察归纳：这条链的不变量

把调用关系画全，可以一眼看完交权前的动作序列：

```
head.S:multiboot_init
  ├─ 设临时栈（镜像数据段里 4096 字节）
  ├─ 标志寄存器归零（清中断允许、嵌套任务）
  ├─ push %ebx, push %eax → call pre_init
  │     ├─ assert(magic)                       确认交接约定
  │     ├─ get_parameters(ebx, &kinfo)
  │     │     ├─ memcpy 结构 + memcpy 模块清单   先拥有数据
  │     │     ├─ 遍历内存映射 / 回落两段        登记
  │     │     ├─ 内核追加为第 N+1 个模块        自查重叠
  │     │     └─ cut_memmap × (N+1)            切除占用
  │     ├─ pg_clear()                          页目录清零
  │     ├─ pg_identity(&kinfo)                 恒等（越界段禁缓存）
  │     ├─ pg_mapkernel() → freepde_start      内核高半区（无用户位）
  │     ├─ pg_load()                           写页表根
  │     └─ vm_enable_paging()                  清位→大页→分页→写保护→全局页
  │           → return &kinfo
  ├─ 换到内核高地址栈、栈底放 0
  └─ push %eax → call kmain
```

现在回头复述 §1.3 那张图。**先别看下面的结论，自己试一句：这一章里出现的三次"不能颠倒"——恒等映射不能晚于分页、内核高半区映射不能晚于分页、信息整理不能晚于铺表——它们共同遵守的是同一条什么规则？**

想好之后可以对答案：

**这条链上真正不可颠倒的，是"谁在此刻取指、谁在此刻取栈"决定的那几步。** 恒等映射必须在打开分页之前，因为正在执行的代码在低地址；内核高半区映射也必须在打开分页之前，因为交棒后的第一条指令就在高地址（换栈与 `call kmain` 用的都是恒等段覆盖的地址，所以那两句可以安全地紧跟在开关之后）；而"信息整理"必须在两者之前，因为账本的高水位线是铺表时判断"这段是不是真内存"的依据。

至于 `get_parameters` 与 `pg_clear` 之间，只是数据准备的先后，没有硬件状态的依赖——这一点在下一章会有回报。

到这里，读者手里有了一份"C 版做了什么、为什么必须这样排"的完整图。Rust 重写要回答的问题也随之清楚了：**哪些是必须原样保住的机器语义，哪些只是 32 位单一固件这个载体的偶然负担。** 第 3 章就按这条线切。

---

## 第 3 章　Rust 设计：从 C 的语义负担逼出的重新表达

### 3.1 交接为什么只能是数据

C 版把一切塞进一个编译期就存在的全局结构 `kinfo`，`pre_init` 填它、`kmain` 读它，同一个镜像、同一个数据段，指针传来传去都没问题。

先把这个结构本体打开看一眼（`minix3/minix/include/minix/param.h:kinfo`，节选）：

```c
typedef struct kinfo {
        /* Straight multiboot-provided info */
        multiboot_info_t        mbi;
        multiboot_module_t      module_list[MULTIBOOT_MAX_MODS];
        multiboot_memory_map_t  memmap[MAXMEMMAP]; /* free mem list */
        phys_bytes              mem_high_phys;
        int                     mmap_size;

        /* Multiboot-derived */
        int                     mods_with_kernel; /* no. of mods incl kernel */
        int                     kern_mod; /* which one is kernel */

        /* Minix stuff, started at bootstrap phase */
        int                     freepde_start;
        char                    param_buf[MULTIBOOT_PARAM_BUF_SIZE];

        /* Minix stuff */
        struct kmessages *kmessages;
        int do_serial_debug;
        int serial_debug_baud;
        int minix_panicing;
        vir_bytes               user_sp;
        vir_bytes               user_end;
        vir_bytes               vir_kern_start;
        vir_bytes               bootstrap_start, bootstrap_len;
        struct boot_image       boot_procs[NR_BOOT_PROCS];
        int nr_procs;
        int nr_tasks;
        char release[6];
        char version[6];
	int vm_allocated_bytes;
	int kernel_allocated_bytes;
	int kernel_allocated_bytes_dynamic;
} kinfo_t;
```

看一眼就能发现它是个**混装的东西**：源码自己用注释分了四段——固件直接给的（整份 `mbi` 结构、模块清单、内存映射）、从固件信息推导出来的（含内核的模块数、内核自己排第几）、引导阶段开始填充的（第一个空闲页目录项、参数字符缓冲），以及纯运行期的杂项（串口调试开关、内核消息缓冲、是否正在中止、进程数、版本号、几笔分配量统计）。四段住在同一个全局结构里，靠"大家住在一个镜像内"这件事互相通气。

重写之后，这个"传"变得需要论证了。§1.1 埋下的那句话在这里兑现：**引导垫片交完东西就要抽掉自己的梯子。**

把假设摊开：假如启动契约里放一个函数指针，比如"调用这个回调去解析设备树"。这个指针的地址是在引导垫片的镜像里解析出来的，函数体却编译进了另一个包。一旦引导垫片的内存被回收或重新映射（它退出固件服务之后就该让位），指针指向的东西随时可能不再是那段代码——**内核照着跳过去，跳进的是一个悬垂地址。**

因此重写把这条边界钉成一条硬约束：跨边界传递的东西，必须全部是值语义的纯数据。需要传递"能力"的时候，不传函数指针，传一个**不透明分类标签**加一个**物理地址**，由内核在自己这边决定怎么处理（`os/libs/minix-boot/src/platform.rs` 的模块说明把这条理由直接写在了文件头，标签类型是 `os/libs/minix-boot/src/platform.rs:PlatformDescKind`）。

这一节先得到一个结论就够了：**契约的形状由"它要活多久"决定。** 需要活过引导垫片自身寿命的东西，就不能引用引导垫片自己。

那么"能力"到底长什么样、由谁保证被调用？这个问题在接口层有一个很容易答错的地方，值得专门看。

### 3.2 那个接口到底管到哪一步

重写把两套固件（统一可扩展固件接口与监督器调用接口）的差异收进一个接口类型（`os/libs/minix-boot/src/boot_shim.rs:BootShim`）。把它的全体亮出来——接口体只有一条方法（节选）：

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

这个接口能约束什么、不能约束什么，必须说准。先把它的正面价值摆出来——这段就是实现体自己文件头的理由（`os/libs/minix-boot/src/boot_shim.rs`，逐条转述）：

1. **类型安全**：两套固件必须一起提供那几个步骤，缺一步就编译不过，不用把参数散落成一堆自由函数；
2. **封装**：每个固件的实现是一个整体，不会无意间把一边的内存映射与另一边的分配页拼在一起；
3. **可测**：可以注入一个假实现做单元测；
4. **免分支**：调用方不需要按架构写条件编译。

这四条读下来会让人点头：抽象确实买到了东西。那它买到的边界到底到哪里？把接口体再看一眼——它只有一个方法，而且**没有接收者**（`self` 一个都没有），返回一个装着四个数据字段的结果（`os/libs/minix-boot/src/boot_shim.rs:BootPrepareResult`：启动契约、页表根、分配区首尾）：

这个接口能约束什么、不能约束什么，必须说准：

**它约束的是"每个固件实现都必须提供一个叫这个名字、拿这个参数、给这个返回的东西"，仅此而已。方法体内部按什么次序调用哪几步、有没有真的退出固件服务、有没有在合适的时候取内存映射快照，类型系统一概不管。**

这一点很容易写过头，因为接口方法自己的文档注释写着"发现内存、加载内核与模块、分配页、构造启动信息、退出固件服务"，读起来像一份被编译器守护的清单。但把两个事实放在一起看就清楚了：第一，`prepare_boot` 没有接收者，内核从不持有、也从不接收一个"引导垫片对象"——它只在引导垫片内部被调用一次（`os/boot-shim/src/main.rs` 里那一行调用）；第二，编译器对方法体的要求只有类型匹配。

所以这条边界真正提供的价值是**接口的形状统一**，让两套固件实现可以被同一处代码调用，而不是"流程完整性"。流程完整性靠什么？靠两样更朴素的东西：**实现体自身的语句顺序**，以及运行期的断言（§4.1 就会看到一条专门看守顺序的断言）。

对照 C 版反而能看清这件事：C 的 `pre_init` 是一条直线函数体，七行写死了次序，天然不可能"漏掉一步"——它没有抽象，也就不需要为抽象兜底。Rust 加了抽象层，就必须另外补回流程的保证。

> 这也划出了一条项目级的抽象边界：抽象只保证数据面与接口面存在，流程面与调用次序不属于它能表达的东西。启动契约本身属于"平台把系统送进操作系统状态"的约定，不强行和运行时抽象统一。

### 3.3 谁铺表、谁使能

C 的那份七行清单里，后五行是页表与开关。重写在这一点上做了一个容易误读的选择，必须说准。

引导垫片这一侧只做两件事：分配页表根那一页、分配一段后续用来建表的连续区（`os/boot-shim/src/uefi_helpers.rs:alloc_root_page`、`alloc_bump_region`）。**它不铺表、也不打开分页。**

铺表与使能在内核侧：`os/kernel/src/lib.rs:build_bootstrap_root_and_enable`，三步依次标注着自己对位的 C 函数（恒等对 `pg_identity`、内核高半区对 `pg_mapkernel`、使能对 `vm_enable_paging`）。

**"名字里带 shim 的那段代码其实不建页表"——这是本篇最容易读错的一件事**，因为它和直觉相反：既然它负责引导，建页表听起来就该归它。

那为什么要把它放在内核侧？看 §2.5 末尾那句"引导代码不依赖自己还活着"就明白了：需要这段机器状态切换的调用方不止一个。

| 调用形态 | 谁在用 | 为什么需要共用 |
|---|---|---|
| 内联引导：引导垫片镜像里直接调用 | x86-64 与 RISC-V 的生产路径 | 引导垫片此刻把内核代码链接在自己镜像内，`arch_boot` 就地执行 |
| 跨镜像引导：先建表开分页，再绝对跳进独立内核镜像 | ARM64 | 跳过去之前必须已经开着分页，且高地址已被映射（见 §4.2） |

**把"启用分页"放在内核侧的共用函数里，才让同一份铺表代码可以被这两条路径复用**；放在引导垫片里，跨镜像那条路径就只能复制一份。

于是这条搬迁不是分层洁癖，而是被"谁复用"逼出来的取舍。被否掉的另一个方案是"引导垫片自己建表、内核只接管"——它的问题是两条路径对表内容的需求不同（跨镜像那条还要保证高半区与设备窗口都在同一张根表里），复制会漂。

### 3.4 三种硬件回答同一个问题

先立统一模型，再看差异——顺序不能反过来，否则这一章会长成一份配置说明。

三种平台都必须回答同一个问题：**内存布局从哪问出来，以及交棒时处理器处于什么状态。**

| 环节 | 统一语义 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|---|
| 信息从哪问 | 向固件问布局与文件 | 统一可扩展固件接口 | 统一可扩展固件接口 | 监督器调用接口移交 + U-Boot |
| 平台信息来源 | 递一个不透明标签加物理地址 | 先高级配置与电源接口表根指针 | 设备树优先、接口表回落 | 只有设备树 |
| 交棒形态 | 纯数据跨边界 | 内联直调 | 跨镜像绝对跳 | 内联直调（生产由内核镜像自引导） |
| 页表根 | 引导期一张根表同时覆盖恒等段与目的地 | 四级页表 | 两个转换基址寄存器（低半/高半） | 三级或四级按需 |

差异被安排在"信息从哪问"和"交棒形态"两处，契约的形状与三步机器状态切换没有架构分支——这正是 §3.3 那个共用的结果。

这里有一条必须写准的事实，否则会把"代码存在"读成"代码在跑"：

**引导垫片目录里那套监督器调用接口的实现，并不是当前 RISC-V 的生产引导面。** 该库里的内存布局构建是一段单区硬编码（`os/boot-shim/src/opensbi_helpers.rs:build_memmap`，文档注释自己写着"内存映射→硬编码，不调用固件枚举内存"），供库内测试与被复用的构件使用；真实生产引导由内核镜像自己充当引导体（`os/kernel-image/src/bootface.rs`），它复用同一批构件（构造启动信息、文件表、bump 分配），但**改成从移交时带来的设备树指针解析内存布局**，并直接调用内核侧的 `arch_boot`。

最后把"为什么 RISC-V 不能共用前面那条路"这件事做得具体一点，因为它的理由不在策略层，而在工具链层。该模块文件头把整条链画了出来（`os/boot-shim/src/opensbi_helpers.rs`，原样照录）：

```text
OpenSBI (M-mode)            ← 监督器调用接口的运行时服务（控制台、定时器、处理器间中断）
  ↓ jr a1
U-Boot (S-mode)             ← 提供磁盘与文件系统驱动、fatload、go
  ↓ fatload + go
boot-shim (S-mode, ELF)     ← 本模块
  ↓ 解析 ELF + 复制
kernel (S-mode, ELF)
```

现在能自己回答那个问题了。文件头给的硬约束是：**工具链没有 `riscv64-unknown-uefi` 这个目标，也发不出对应的 PE 可执行格式**。这里需要一句解释，否则这一步会卡住读者：**PE 就是一种可执行文件格式**（Windows 上的 `.exe` 与 UEFI 世界里的 `.efi` 引导应用都是这个格式）；固件接口之所以挑镜像格式，是因为它只肯加载它能解析的那种二进制。而统一可扩展固件接口的调用约定要求调用者本身就是一个 PE 镜像——所以即使 U-Boot 自己实现了一个固件接口子集也用不上，因为我们造不出那个能去消费它的二进制。能直接对照的真实例子在测试面（`os/qemu-tests/test-riscv64-uboot.sh`）：它生成一个 `boot.cmd` 脚本，内容就一行 `fatload virtio 0 ${loadaddr} boot.elf` 再 `bootelf`——**读文件这件事是 U-Boot 替我们做的**，而引导垫片在 x86 上是用固件的文件协议自己读的。两边对比，差异就落在同一个统一问题上：**"能不能向固件要文件"**。

读架构差异时值得带上的判断方法是：**先看它在回答哪个统一问题，再看它把哪一步换了。** 如果一段差异回答不出任何统一问题，那它多半只是历史形状（§2.3 的 1 MB 硬编码就是这种）。

---

## 第 4 章　实现与实地走查：打开仓库跟着走一遍

前两章给了模型和取舍。这一章把生产路径走通一遍，每一步都指到真实代码——读者应该能在自己的仓库里把这条链点完。

### 4.1 统一可扩展固件接口路径的一次真实走查

入口只有一行（`os/boot-shim/src/main.rs`）：

```rust
let result = UefiBootShim::prepare_boot(1024);
```

参数 `1024` 是给引导期建表预留的页数。实现体很长（`os/boot-shim/src/uefi_helpers.rs:prepare_boot`），先给一份**步序总览**：每个步只取它最关键的那一行，步与步之间的其它行用省略号标出。总览只负责"先看清有多少步、谁在谁前面"；真正承载语义的三处（快照时机、两个断言只守两笔、退出服务后的打印）在下面第一处／第二处／第三处三段里各自贴完整上下文，不靠省略号吞掉：

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
...
assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);
...
let kernel_info = build_kernel_info(
    memmap, reserved_regions, ..., PhysBytes(0), 0, platform_sources,
);
...
exit_boot_services();
```

对着 §1.3 的图，每一步都能落到位。下面把三处"顺序本身在承载语义"的地方各自摊开。

**第一处：内存映射快照的取用时机。** 快照（`build_memmaps` 内部调用固件的内存映射服务）被刻意放在**所有引导期分配之后**——不只要晚于页表根与连续区，还要晚于内核镜像与全部启动模块的分页占用。把这一段接在总览里展开看（实现体里相邻的三行，中间只有注释）：

```rust
let (memmap, reserved_regions) = build_memmaps();

assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);
```

这两条断言检查的就是区间不相交，函数体全部在这里（`os/boot-shim/src/uefi_helpers.rs:assert_bootstrap_outside_memmap`）：

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

断言的条件是区间不相交。

**为什么这条顺序要紧？** 固件的空闲内存报告是"当前状态"的快照：已经分配出去的页在那份活图里被改标成引导加载数据一类的用途，"只取可用内存"的视图天然排除它们。如果快照在占用之前拍，那些正在被使用的页就会以"空闲"的名义被交给内核的页帧管理——等于把自己的页表页送人。

紧跟着的两条断言把这条依赖钉住：任何一笔标成可用的段，都不允许和已经分配的引导页有交集，否则立刻中止——趁着引导垫片还能打印的时候失败。

把断言体与那个顺序再连起来看一句：**这条检查能拦住的东西，止于"快照拍得对不对时机"之外的那一半**——它拿到的 `memmap` 已经是快照的结果，如果快照本身拍早了，那张图里就带着已被占用的页，断言会跟着一起错。**守顺序靠的是实现体里那几行的排列，断言守的是排列之后的不自洽。**

但这两条断言**只覆盖页表根与连续区这两笔**，没有覆盖内核镜像段与模块段。这不是遗漏，值得单独看清，因为它是那种"看似应该全覆盖的守卫为什么只守两处"：

本项目采用的装载模型是"引导垫片把内核与模块**直接写进可用内存**，不经固件分配"，只有页表根与连续区是真正的固件分配。前者在固件的活图里仍然是"可用"，所以它必然出现在映射里，需要的是内核侧按引导占用逐段扣除；后者的排除靠的正是上面那条快照顺序。把两者一起断言，会误炸一个预期形状。

> **这里故意停住**：那个"逐段扣除"到底发生在哪一行、扣完的图怎么交给页帧管理，本篇不讲——它回答的是"运行期谁能动这张图"，属于后面的页表与内存篇。**现在只需要拿到一个结论：同一张内存图，在引导侧与运行侧被两组不同的规则保护——一组靠分配与快照的先后，一组靠扣除。** 知道有这两组、且它们各守一段，就够往下走了。

**第二处：`build_kernel_info` 传进去的 `PhysBytes(0), 0`。** 这两个实参对应 C 的 `bootstrap_start` 与 `bootstrap_len`（§2.2 的结构里那两个字段）。C 里它标记的是"内核中一小节不参与分页的代码"，引导完成后通过归还可用内存回收。

**Rust 侧这一节不存在**：内核从第一条指令就在高半区、从不关着分页跑，那个蹦床位于另一个镜像（引导垫片本身）里，不在内核里。所以长度必须为零——`os/libs/minix-boot/src/kernel_info.rs:KernelInfo::validate` 里有一条硬断言要求它等于 0。把它留成非零反而是危险的：一次过量的归还会把固件、设备树、引导垫片自己都扫成可用内存。

这里能看到一种重要的重写模式：**"淘汰"的字段里有一类不是不需要了，而是改由构造方与断言保证。**

**第三处：退出固件服务之后的那行打印。** 服务退出之后，代码改用直接写串口的方式输出，不再调用固件的文本输出服务。

**理由和 §3.1 是同一条：能力随边界消失。** 固件的输出服务在退出服务之后就不可信了（它可能依赖已经不存在的引导服务状态），而"这一步之后还要不要打印"恰恰是最需要看得见的时候。

### 4.2 交棒的两种形态

契约是纯数据这件事，落到"怎么递"上有两种真实形态。

**内联直调（x86-64 与 RISC-V）**：契约由 `os/kernel/src/lib.rs:arch_boot` 接手。这个函数在入口第一时间做一件看起来多余的事——把传进来的契约**按值收进内核自己的全局变量**，之后一切（建页表、跳主流程）都使用全局副本的引用（`os/kernel/src/lib.rs:store_kernel_info`，注释明确它是一次性的）。

为什么要绕这一道？把交接的载体摆出来看：

```rust
store_kernel_info(kernel_info);
let kinfo: &'static KernelInfo = crate::kernel_info()
    .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
```

（`os/kernel/src/lib.rs:arch_boot`、`store_kernel_info`。）

**在 x86-64 的调用约定里，第一个整型参数寄存器是调用方保存的。** 引导垫片的镜像与内核共用同一份代码，但中间经过了固件的代码生成，那条寄存器随时可能被跨调用复用覆写。如果直接把引导垫片栈上那个引用经一段不再返回的内联汇编传给主流程，一旦该寄存器被覆写，主流程拿到的就不是契约，而是恰好留在里面的地址——曾经出现过的情形是它指向一段只读字符串数据，校验函数读到一串字符就中止。

所以这个"先按值收进自己的数据段"是**把依赖从寄存器手上收回到链接期常量**：全局副本的地址由链接期决定，不经过任何寄存器交接。

这一道不是多此一举，字段里还藏着一层更细的理由。`store_kernel_info` 的函数体里，真正被搬进数据段的不只是结构体本体，还包括一块专门给它准备的存放区（`os/kernel/src/lib.rs:store_kernel_info`）：

```rust
let store = &mut *crate::globals::RESERVED_REGION_STORE.get();
let src = kernel_info.reserved_regions;
```

为什么偏偏是 `reserved_regions` 这一个字段需要另起一块存放区？看一眼它装的是什么就明白了：

**这些窗口的内容住在引导垫片的固件内存池里，而那片内存只被固件的一比一表映射着。** 跳进高半区之后，内核自己的页表不会重放那层映射，同一个虚拟地址读出来会是零。所以必须把字节搬进内核数据段——**那块内存整个运行期都在映射里**——再把全局副本的指针交出去。换句话说：§3.1 那条"纯数据"只保证递过去的不是函数指针，**保证不了递过去的指针指得到东西**。

**跨镜像引导（ARM64）**：`os/kernel/src/lib.rs:bootstrap_to_kernel_image` 里，引导垫片建完表、开完分页，然后分配一页把交接块写进去，用绝对跳转进高半区镜像的入口，由第一个参数寄存器携带这页的物理地址（同函数内的内联汇编 `"br {entry}"`，寄存器绑定 `in("x0") blob_phys.0`）；独立镜像侧的入口把它当指针收下（`os/kernel-image/src/main.rs:rust_image_main`）。

这里有一个必须解释清楚的追问：**内核不是有一个全局数据段吗？为什么不沿用内联那条路？**

因为这是两个镜像。引导垫片镜像和独立内核镜像各自链接了一份内核代码，它们**不共享数据段全局**——一个镜像写进自己数据段的东西，另一个镜像读不到。所以必须有一个承载体放在物理内存里，由寄存器递指针、由内存放数据。交接块的类型定义就写明了这个分工（`os/libs/minix-boot/src/handoff.rs`，文件头说明与 `BootHandoff` 定义）。

递出的那一句真实写在这里（`os/kernel/src/lib.rs:bootstrap_to_kernel_image`）：

```rust
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

逐行读这段汇编之前，先看一个细节：`x0` 这个入口参数是**写在汇编块外面的**（`in("x0")`），而不是在块里写一条 `mov x0, …`。

注释给了理由：后者会是一次未声明的破坏——编译器完全有权把 `{entry}` 这个跳转目标也分配到 `x0`，那么那句隐式的移动就会把跳转目标自己覆写掉。**把寄存器绑定交给汇编块的声明，而不是自己手写移动，是让编译器知道这个寄存器已被占用。**

还剩一条指令没人解释：`br {entry}` 前面那句 `isb`。本仓库这段代码没有为它留注释，所以本文把它的依据标清楚：**`isb`（指令同步屏障）来自 ARM 架构手册的定义——它丢弃已经预取、排在流水线里的指令，保证下一条取指是在之前对系统寄存器的修改生效之后才发出的。** 放在这里读就说得通了：前一行刚把页表根写进两个基址寄存器，如果跳转目标已经被预取在流水线里，它会用**旧映射**去解算——而旧映射里根本没有那个高地址。**屏障把"已经取出来的指令"丢弃，跳转才会用新表取指。**（这一条属架构规范层面的事实，不是本仓库的代码事实；要亲眼验证它，需真机上观察"去掉了 `isb` 后是否还能进高半区"。）

同一份文件头还回答了一个更细的问题：契约里的载荷是切片（指针加长度），指针指向引导服务的内存池。这些指针在退出固件服务之后为什么还能读？因为**那份池在引导窗口内仍然存活**，而独立内核镜像会在第一次切换页表根之前把这些切片读完。

也就是说：跨边界的数据不只是"值语义"就够了，**它还有一段有效期限**，而这段期限由页表根切换的时刻决定。

顺着这一点，才能理解 ARM64 为什么要多跳一次镜像——它不是编排偏好：

**如果内核就像 x86-64 那样内联跑在引导垫片镜像的低恒等地址上会怎样？** 低地址段属于"恒等映射"这一半，它在第一个转换基址寄存器里；而内核主流程一旦开始为进程建立地址空间，就会切换第一个转换基址寄存器并做一次全局转译缓冲刷新——**被换掉的正是内核自己此刻取指所用的那半张表**。跳进高半区之后，内核的代码与栈都落在永不切换的那半个基址寄存器里，切换地址空间不再是自杀动作。

这两条路径共同回答了 §3.3 那个"谁铺表"的选择：正因为两条路径都需要"建表 + 开分页 + 高半区已映射"这个前提，这段代码才被放在内核侧共用。

### 4.3 内核侧的三步：恒等、高半、使能

`os/kernel/src/lib.rs:build_bootstrap_root_and_enable` 是 C 那五行的对位。

**第一步，恒等映射**（节选真实行）：

```rust
let mut paging = P::new_from_page(root_page);
...
let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
let mut addr: u64 = 0;
while addr < IDENTITY_MAP_END {
    let _ = paging.map_huge(
        VirBytes(addr), PhysBytes(addr),
        kern_huge as usize, id_flags,
    );
    addr += kern_huge;
}
```

对照 §2.5，三个选择变了，每一个都有理由。

**覆盖范围**：C 铺满整个 4 GB 目录；这里也是"整段低地址全铺"，但函数里的注释给出了 64 位时代新的理由——固件加载器的代码与数据可能落在**没有标成可用内存**的那些用途段里，只铺可用段会导致切换页表根的下一条指令就失败。这和 §2.5 的理由形状相同（恒等段要覆盖正在执行的东西），但触发条件换了。

**可执行位必须给**：切换页表根之后处理器立刻要取指，没有可执行权限就取不到。

**权限位只给特权级**（`kernel_read_write`，不含用户可访问）。先把这句当作一个待验的观察放着：上一章刚刚看到 C 的恒等映射带着用户位、而它"无所谓"；这里却直接把用户位去掉，而且去得很干净。

为什么？实现里紧挨着那段代码的注释给了硬理由：在 RISC-V 的三级分页模式里，**一个带用户位的页，监督器模式不能执行它**，除非把状态寄存器里的"允许监督器访问用户页"那一位（SUM）打开；而引导期还没有设置它。所以恒等映射必须是只给特权级的。

（这条约束是架构机制层面的事实，本篇能给的证据是注释与位选择本身；要亲眼验证它，需要在 RISC-V 真机上把一个带用户位的可执行页交给监督器取指——那属于测试篇。）

于是 C 版那一句"用户位无所谓"在 64 位多架构下不再成立。

把 §2.5 与这一节并起来看，读者现在可以自己填出那张 C 与 Rust 的对照：恒等映射的用户位，在 32 位单一架构上是"不起作用的装饰"，在三级分页的监督器模式下是"会让取指失败的开关"。**同一个语义（引导期过渡映射只给内核自己用），因为架构机制不同，落成了不同的位组合。**

铺表要往新分配的页表页里**写条目**，而此刻高半区还没映射——为什么写得到？回到第一步铺的那段恒等：bump 区里的页也是物理页，而它落在"虚拟等于物理"那一段里，所以直接用物理地址当作虚拟地址写进去就行。**恒等映射不只保住了取指与取栈，也保住了"正在建表的那只手"。**

**第二步，内核高半区映射**（节选真实行）：

```rust
let kern_virt = kernel_info.kern_virt_base().0;
let kern_phys = kernel_info.kern_phys_base().0;
if kern_virt != kern_phys {
    let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
```

**注意那个"虚拟不等于物理才做"的判断。** 当两者相等时，第一步的恒等映射已经把目的地铺好了，第二步若再写一遍，就会在同一组页表项上写两次。在 ARM64 与 RISC-V 的多级页表里，两次写入可能落在同一个中间层槽位、却按不同页大小解释，结果是把第一步的恒等映射覆盖掉、留下一个坏掉的中间层。链接布局与这个撞槽问题到下一篇（高半区内核）讲，这里只需知道：**这个看起来可以省掉的判断，守的是"别把刚铺好的恒等段拆了"。**

**第三步，使能**：调用页表对象的启用方法并取回根页物理地址，随后把当前地址空间记为该根页（同函数内 `paging.enable()` 与其后的登记调用）。启用本身按架构分流，但 x86-64 那一份最值得逐行读，因为它里面藏着一个会让人读错整章的事实（`os/arch/src/x86_64/paging.rs:enable`，节选）：

```rust
// EFER.NXE (bit 11) must be ON before any NX-tagged paging entry
// is walked: with NXE=0, bit 63 of every paging-structure entry is
// RESERVED (#PF err=8). …  The UEFI handoff happens to leave NXE set
// on the BSP, but the invariant is ours, not the firmware's — set it
// explicitly (read-modify-write preserves firmware SCE/LME bits).
asm!("rdmsr", in("ecx") MSR_EFER, out("rax") efer, out("rdx") _, options(nostack));
asm!("wrmsr", in("ecx") MSR_EFER, in("rax") efer | EFER_NXE, in("rdx") 0u64, options(nostack));
// Switch CR3 to our own root page table.
// UEFI already runs in long mode (CR0.PG=1) with its own page
// table; we must replace it with ours, which contains both the
// identity and kernel high mappings set up by map_huge().
asm!("mov cr3, {}", in(reg) self.root_paddr);
// Ensure WP (Write Protect, CR0 bit 16) is set so the kernel
// cannot write read-only pages (W^X enforcement). UEFI firmware
// does not guarantee WP is enabled.
cr0 |= 1 << 31;
cr0 |= 1 << 16;
asm!("mov cr0, {}", in(reg) cr0);
```

先盯着那句注释读一个令人意外的形状：**在统一可扩展固件接口路径上，分页早就开着了**（固件自己就在长模式里跑，`CR0.PG` 已经是 1）。于是这一步真正干的事不是"打开分页"，而是**把页表根换成自己那张**——因为固件那张表里没有我们刚铺好的恒等段与高半区。

这修正了一个很容易从 C 版带过来的预设。C 的 `vm_enable_paging` 确实是在"从关到开"；而在 64 位统一固件接口机型上，同一个语义对象（把地址翻译接到自己这张表上）落到了一个完全不同的动作上。**同一个语义、不同的机器动作——这正是 §3.4 那张表说的"先看它在回答哪个统一问题"。**

剩下两处都是同一类守卫：

**先置不可执行位（NXE）再换表。** 注释给的理由很硬：不可执行位没开时，页表项的最高位是**保留位**，一旦碰到就直接页错误；而本页的重写保护与只读策略都要用那个位。固件"恰好"把它开着——但注释紧跟着一句把责任划清：**这个不变量是我们的，不是固件的**，所以自己显式读写设定（先读后写，以便保留固件已经设好的长模式位）。

**写保护位也要自己设**，因为固件不保证它开着。没有它，内核就能写只读页——只读保护形同不存在。

最后一段把这个函数与本章前面接回一条线：全局页位开着之后，带"全局"标记的条目能**撑过换页表根**——而 §4.3 第一步用的 `kernel_read_write` 就带这个标记（注释直说：全局页位不开，这个标记无效）。于是读者现在可以回答 §1.3 留下的那个问题了：

**恒等段整段给特权级、带全局标记、先置好不可执行与写保护再换根——这三件事共同服务的是"换根那一刻不能掉下去"这一条规则。** 决定链上次序的从来不是代码美观，而是**此刻谁在取指、谁在取栈**。

（ARM64 与 RISC-V 的使能于同一个入口下分别写两个转换基址寄存器与系统控制寄存器、地址翻译与保护寄存器并做一次地址翻译刷新；它们回答的都是 `vm_enable_paging` 那个问题：**怎样让"地址开始被翻译"这件事对正在跑的代码透明。**）

### 4.4 数据形状对照：哪些留下、哪些改型、哪些淘汰、哪些新增

C 的那个全局结构定义在 `minix3/minix/include/minix/param.h:kinfo`（不含调试别名，条目上限是一个编译期常量 40）。Rust 侧的对应物是 `os/libs/minix-boot/src/kernel_info.rs:KernelInfo`。按四类摊开：

| 类别 | C | Rust | 为什么 |
|---|---|---|---|
| 保留 | `memmap[]` 定长数组 + `mmap_size` | `memmap: &'static [MemoryRegion]` | 语义完全保留（可用物理段清单），只是长度不再靠数组大小与计数分别维护 |
| 保留 | `param_buf[1024]` | `param_buf: &'static [u8]` | 引导参数缓冲的对外行为不变；长度上限由校验断言守住 |
| 改型 | `freepde_start`（一个页目录项号） | `free_upper_idx: Option<usize>` | "第一个空闲的目录项"在多级页表里没有通用意义，改为可缺省的索引语义 |
| 改型 | `bootstrap_start/bootstrap_len` | 同名两字段，但被断言为 `0` | §4.1 第二处：那一节代码在 Rust 里不存在 |
| 改型 | 链接期符号 `&_kern_phys_base`、`&_kern_size`；模型特定寄存器里的系统调用入口 | 升为正式字段 `kern_phys_base`、`kern_size`、`kern_stack_top`、`syscall_entry` | 跨镜像交棒时不能依赖对方的链接期符号，必须显式递值 |
| 淘汰 | `mbi`（整份固件结构）、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`user_end` | 不存在 | 前两者是 32 位引导约定的解析中间产物；高水位线在 Rust 侧不再需要（§4.3 第一步的理由）；重叠体检在装载阶段用独立函数完成 |
| 淘汰 | `do_serial_debug`、`serial_debug_baud`、`kmessages`、`minix_panicing` | 不存在 | 早期输出与内核消息缓冲改由 trait 承担，属后续篇 |
| 新增 | — | `reserved_regions: &'static [MemoryRegion]` | 非可用内存里"内核运行时会从低地址访问"的那部分窗口，需要单独传给内核重放 |
| 新增 | — | `platform_sources: &'static [PlatformDescSource]` | 不透明标签加物理地址的平台信息来源清单（§3.1） |

这张表里"淘汰"的 `mem_high_phys` 值得多看一眼，因为它牵回 §2.5 那条断言：C 用它决定哪些段要禁缓存；Rust 侧整段低地址一律覆盖、并且用统一的可执行加特权读写位（§4.3 第一步），**"先记账再铺表"的数据依赖因此消失了**——不是那条判断变错了，而是它不再被需要。

---

## 第 5 章　测试：怎么知道这些语义真的成立

测试数量现场取，不抄表：

```bash
cd os && rg -c '#\[test\]' boot-shim/src/loader.rs boot-shim/src/uefi_helpers.rs \
    boot-shim/src/opensbi_helpers.rs libs/minix-boot/src/kernel_info.rs libs/minix-boot/src/platform.rs
```

分解：`boot-shim/src/loader.rs` 与 `opensbi_helpers.rs` 各 13、`uefi_helpers.rs` 1（引导垫片包合计 27）；`minix-boot` 包内 `kernel_info.rs` 9、`platform.rs` 8，合计 17。

按"语义 → 测试 → 为什么这条测试能证明它"来读，比数用例更有意义：

| 本篇的语义 | 真实测试 | 能证明什么 |
|---|---|---|
| 启动契约的字段必须成立 | `os/libs/minix-boot/src/kernel_info.rs` 的 `test_validate_accepts_valid_info` 以及六条反向用例（引导代码段长度非零、内核大小为零、栈未对齐、内存映射为空、模块列表为空、参数缓冲越界各自一条） | 校验函数逐条守着 §4.1/§4.4 那几处断言，改坏任一条会有反向用例失败 |
| 首尾相接不算重叠（§2.4 的 `-1`） | `os/boot-shim/src/loader.rs` 的 `test_first_overlapping_pair_abutting`，同组还有分离、重叠、返回首个、空表四条 | 边界约定被单独固化：若有人把判断改成闭区间，这条会红 |
| 模块按约定顺序装入、缺失即中止 | `test_load_boot_modules_loads_all_in_contract_order`、`test_load_boot_modules_panics_on_missing` | 交出去的那份"清单"与固件侧布局协议一致 |
| 加载后未初始化段被清零 | `test_bss_region_is_zeroed_after_load` | 内核镜像不该继承上一段内存的残留 |
| 内存映射与分配区可复原 | `os/boot-shim/src/opensbi_helpers.rs` 的 `test_build_memmap_default_region`、`test_alloc_bump_region_round_trip`、`test_alloc_root_page_is_4k_aligned` | 页表根对齐与分配区首尾闭合可被宿主侧检查 |

一处必须诚实标注的边界：**上面这些都在宿主测试里跑，靠的是可注入的模拟对象**。有三类语义宿主测不到——快照与分配的真实先后（依赖固件的活内存图）、跨镜像跳进高半区后第一次取指能否命中（依赖真实页表根与转译）、控制位次序在真芯片上的效果。它们只能由真机回归覆盖（三架构各一条定时器中断用例，见 `os/qemu-tests` 目录下相关脚本，具体门径属测试篇）。

另外，`os/boot-shim/src/main.rs`、`lib.rs`、`boot_shim.rs`、`handoff.rs` 里一条单元测试都没有（用同一命令复核即为空）。这个分布本身合理：入口与跨镜像载荷都是"只在真机上才成立"的部分——但也意味着**§4.2 那个"必须先按值收进全局"的守卫没有回归测试**，它靠的是代码形状与文档说明。这是本篇披露的一处真实盲区。

---

## 第 6 章　诊断：起不来时先看哪一步

引导期的故障有一个共同形状：**没有输出，机器直接复位或静默死循环**。所以定位手段是"把这条链切成几段，看断在哪一段"。

两条先做的检查（可直接复制）：

```bash
# 一：确认交权前的动作序列与守卫都在位（应看到断言校验、快照、
#    两次 outside-memmap 断言、退出服务、建表三步的行）
rg -n "validate\(|build_memmaps\(|assert_bootstrap_outside_memmap|\
exit_boot_services|build_bootstrap_root_and_enable|store_kernel_info" \
    os/boot-shim/src/uefi_helpers.rs os/kernel/src/lib.rs

# 二：确认契约里递出的清单条数（内核侧第一行打印的就是它）
rg -n "KernelInfo:|arch_boot arg=|reslen=" os/kernel/src/lib.rs os/libs/minix-boot/src/kernel_info.rs
```

预期输出的关键行是一条里程碑序列：先出现引导垫片侧进入准备阶段的打印，随后是分配完成、装入完成、映射快照可用、构造完成、退出服务，最后由内核侧第一行打印接上。任何一段之后不再出现新行，就说明断在那一段与下一段之间。

三类典型故障的判别：

**开分页之后立刻复位。** 查恒等段的覆盖范围：正在执行的代码所在的地址，是否落在第一步那个循环铺到的区间内。特别看有没有把非可用内存的那些用途段算进去——只铺"可用内存"是这类故障最常见的成因（§4.3 第一步那条理由）。

**第一次切换地址空间之后复位。** 这跟上一类是两种不同的故障，别混：此刻恒等段是好的，坏的是**高半区与设备寄存器窗口有没有和恒等段在同一张根表里**。内核代码若还跑在可被切换掉的那半个基址寄存器下，刷新转译缓冲就等于把自己的取指映射拆掉（§4.2 那段推演）。

**内核一启动就在校验函数里中止，报的是"字段为空"或值离谱。** 先分辨两件事：字段是**没递过来**还是**递过来但读错了**。第一条检查命令的内核侧输出给出了可分辨的信号——契约被收下的那一刻打印长度与首条目：长度为 0 说明丢在跨边界之前，长度对但内容全零说明载荷指向的内存已经不再可读（§4.2 末尾那条"有效期限"）。

调试入口最省事的仍是串口：引导垫片与内核都往同一个端口写，退出固件服务之后由直接写端口的方式接管，所以日志里出现"哪一行"就是"走到了哪一步"。

---

## 收尾：这一篇真正要留下的三件事

1. **递梯子的人会消失。** 引导期组件交出去的东西不能依赖它自己还活着——这一句同时解释了为什么契约只能是纯数据、为什么快照要在分配之后拍、为什么退出固件服务之后不能再调用固件的输出。
2. **打开分页这件事，必须在它自己脚下的地址还在映射里的时候完成。** 恒等段不是"顺便铺的"，它服务的是正在取指的这段代码；理解了这一条，顺序、可执行位、覆盖范围三个决定都变得无可选择。
3. **抽象只保证接口面与数据面，流程面要靠实现体的顺序与运行期断言。** 一个接口类型不会替你保证"按次序退出了固件服务"；C 版不需要说这句话，因为它是一条直线函数体。

下一篇（02）处理这条链上被本篇当作前提的那个东西：内核镜像的链接地址与装载地址为什么不同、以及"成对跳"怎样让高半区的目的地真正可用。
