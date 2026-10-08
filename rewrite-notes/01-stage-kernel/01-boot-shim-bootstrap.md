# 01 · 内核拿到执行权之前，必须先被建立的几件事

> **C 真值源**：`minix3/minix/kernel/arch/i386/head.S`、`pre_init.c`、`pg_utils.c`、`minix3/minix/include/minix/param.h`
> **Rust 实现**：`os/boot-shim/src/{main.rs,uefi_helpers.rs,loader.rs,opensbi_helpers.rs}`、`os/libs/minix-boot/src/{kernel_info.rs,boot_shim.rs,handoff.rs,platform.rs}`、`os/kernel/src/{lib.rs,globals.rs}`、`os/arch/src/x86_64/paging.rs`

内核不能一上来就跑。在它的第一条指令能被正确取到、它的第一个函数能被调用、它拿到的信息在它使用时仍然可读之前，有若干件事必须先成立。这篇按概念逐件处理：每一章先看代码、再从代码里发现一件不看代码想不到的事、然后说明 Rust 重写把这件事落到了哪里。读完这些章，整条引导链的约束是读者自己攒出来的，不是提前被告知的。

---

## 第 1 章　为什么第一个 C 函数之前有一段汇编

Minix3 的 x86 入口在 `minix3/minix/kernel/arch/i386/head.S:multiboot_init`：

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

GRUB 跳进来时手里有两个寄存器：`eax` 是魔数，`ebx` 是启动信息结构的地址。这里先问一个问题：**为什么不直接写一个 C 函数当入口？**

答案在第一行。C 函数要能调用，前提是**栈已经存在**——函数序言要往 `[rsp]/[ebp]` 下面放返回地址与局部变量。固件跳转的那一刻没有准备栈，`esp` 里是固件遗留的值，不能用。所以第一条指令做的事就是把栈指针指到镜像自己数据段里预留的一块：

```asm
.data
load_stack:
	.space 4096
load_stack_start:
```

`mov $0, %ebp` 与紧跟的 `push $0` 是配套的：栈底放一个 0，帧链走到这里为止，栈回溯不会越过入口跑飞。

第三条动作是 `push $0; popf`——把整个标志寄存器置 0。注释写着"set flags to known good state"，括注点明清掉的是嵌套任务位与中断允许位。为什么要关中断允许位，要等到本篇最后再看一次这张地址与表的清单；现在只需要接受一条：**此刻中断入口表还不存在**。中断进来而表不存在，处理器的行为是复位，不是丢弃这次中断。

还剩一行没人解释：`popf` 之后那句独立的 `push $0`。源码在这里没有注释。可以核对的外部约束只有一条：x86 的函数调用约定要求 `call` 之前栈顶按 16 字节对齐（被调用方用它存放宽度较大的局部变量），而 `load_stack_start` 是一段 4096 字节区的末端，此前压入的次数会把对齐性破坏掉——这一句大概是在补对齐。**本篇把它标为 `[待验证]`**：是不是补对齐、有没有同时兼作帧哨兵，源码未写，不做断言。这里想留给读者的是一个习惯：读到一句没有注释、看似多余的动作，先记成问题，别当噪声跳过。

再看压栈顺序。函数签名是 `pre_init(u32_t magic, u32_t ebx)`，`magic` 在前，可代码先压 `ebx`、后压 `eax`，看起来反了。理由只依赖一条约定：**参数从右往左依次压栈，最后压入的那个落在最靠近栈顶的位置，也就是函数读到的第一个参数。**先压结构地址、后压魔数，魔数才在第一位。可以自查：`pre_init` 里第一个动作是断言魔数，它读的是第一个参数——按这条约定，它读到的正是刚压进去的 `eax`。

**重写里这段还在不在？** 在，但换了位置与写法。独立内核镜像的入口同样先装载栈指针再进 Rust（`os/kernel-image/src/main.rs:rust_image_main` 之前的入口汇编段）：

```rust
    _start:
        ...
        lea rsp, [rip + kernel_boot_stack_top]
```

栈不再由一段手写的 `.space` 给出，而是由链接脚本在 `.bss` 里预留 64 KiB，入口用一条相对地址载入取它的顶。入口文件里对此的说明是"与 Linux 内核 `init_stack` 同形"。**"先有栈，才谈得上调用高级语言函数"这条约束与语言无关**，所以它原样穿过重写；变化的只是栈由谁预留。至于标志寄存器：本仓库的镜像入口段没有对应的清标志动作，本篇也不假装它做过这件事。

---

## 第 2 章　打开分页之前，页表里必须已经有什么

`minix3/minix/kernel/arch/i386/pre_init.c:pre_init` 是内核第一个 C 函数：

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

`assert(magic == MULTIBOOT_INFO_MAGIC)` 承担的不是一句礼节。它断言的是"我是按多引导约定被跳进来的"；这个前提不成立，`ebx` 指向的就不是约定的结构，后面所有解析都是在读垃圾。

七行里真正需要解释为什么不能换序的，是 `pg_identity`、`pg_mapkernel`、`vm_enable_paging` 这三行。先看 `pg_identity`（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity`，函数体全文）：

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

现在做一个假设检验：**如果去掉 `pg_identity`，只保留内核高地址映射，然后打开分页，会发生什么？**

`vm_enable_paging` 置起分页位之后，处理器下一条取指用的就是虚拟地址。而此刻正在执行的这段引导代码位于低物理地址，新页表里没有为它准备条目——**当前这条指令之后的那次取指就落空**。取指落空是页错误，页错误需要中断入口表里的处理入口，那张表还没建立。结果不是"一条错误信息"，是复位。

再看 `head.S` 在 `call pre_init` 返回之后干了什么：

```asm
	mov	$k_initial_stktop, %esp
	push	$0	/* Terminate stack */
	push	%eax
        call    _C_LABEL(kmain)
```

**这四条指令是"打开分页之后"执行的，而它们自己就在低地址。** `mov $k_initial_stktop, %esp` 换到内核高地址栈之前，处理器还在用恒等段覆盖的地址取指、取栈；换栈之后 `push %eax` 与 `call kmain` 才用到高地址——所以高地址映射也必须在开分页之前铺好。两条映射各服务链条上相邻的两步，这就是那三行不能换序的全部理由，也说明源码注释那句"but first a 1:1 mapping so this code stays where it should be"里的"this code"指的是谁：**指正在执行这段代码的 CPU，不指内核。**

于是有一条依赖可以直接验证：`pg_identity` 函数体的第一句是 `assert(cbi->mem_high_phys)`。这条线由登记函数 `minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap` 推高（第 5 章）。也就是说**铺表之前必须先把内存登记完**——不是风格问题，是数据依赖：铺表要用这条线判断哪些段落在真实内存之外（第 6 章）。

Rust 侧同一段在 `os/kernel/src/lib.rs:build_bootstrap_root_and_enable` 的第一步：

```rust
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

三处与 C 不同，每处都有理由，而且理由都写在紧挨着的注释里。

**覆盖范围**：C 按 1024 个目录项铺满 4 GB；这里同样铺满整段低地址，但注释给出的 64 位理由是新的——固件加载器的代码与数据可能落在**没有被标成"可用内存"**的那些用途段里，只铺可用段的话，切换页表根之后的第一条指令就会故障。换句话说，"要覆盖谁"这件事变得更宽了：不只是内核自己的代码与栈，还包括引导镜像本身。

**可执行位必须给**：切换页表根之后立刻要取指，没有可执行权限就取不到。

**权限位只给特权级**：`kernel_read_write` 不带用户可访问位。C 版那一份带（`I386_VM_USER`）。原因是一条架构机制：在 RISC-V 的三级分页模式下，**带用户位的页监督器模式不能取指**，除非把状态寄存器里的允许监督器访问用户页位（SUM）打开，而引导期没有设它。这条约束属于架构规范层面，本仓库的证据是那段注释与位选择本身；要亲眼验证它，需要在 RISC-V 真机上把一个带用户位的可执行页交给监督器取指。**因此同一个语义（引导期过渡映射只供内核自己使用）在不同架构上落成了不同的位组合。**

**这两处（覆盖范围、权限位）都是宿主测试测不到的**——它们只在真机取指时表现。三架构各有一条定时器中断的引导回归在 `os/qemu-tests` 目录下，那类门径属于测试篇。

第 1 章那条"先有栈"与本章这条"先有覆盖当前地址的映射"叠在一起，就得到一个暂时够用的结论：**引导代码必须先把"自己接下来要用的地址"准备好，才能启用翻译。**

---

## 第 3 章　"启用分页"在 64 位固件路径上，主要动作是换页表根

C 版打开分页分两步。`minix3/minix/kernel/arch/i386/pg_utils.c:pg_load`：

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
```

源码给了两条位序理由，一条在清位那行（全局页位必须在分页位之后开），一条在置大页位那行（页表里用的是 4 MB 条目，所以大页位必须早于分页位，否则刚开分页就按 4 KB 格式解释那些条目）。这两条是**机器规定的依赖**，属于不变量。

至于"先把分页、全局页、大页三个位清零，再从头点亮"——这是**实现选择**，不是机器要求。它要买的东西是确定性：不管上一棒（BIOS 也好、某个引导器也好）遗留了什么位，归零后按固定顺序点亮，结果唯一。换个写法（读回后只补需要的位）也能工作，代价是结果会带上固件残留。

现在看 64 位的对应实现（`os/arch/src/x86_64/paging.rs:enable`，节选，保留原有的读取动作）：

```rust
const MSR_EFER: u32 = 0xC000_0080;
const EFER_NXE: u64 = 1 << 11;
let mut efer: u64;
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
let mut cr0: u64;
asm!("mov {}, cr0", out(reg) cr0);
cr0 |= 1 << 31;
cr0 |= 1 << 16;
asm!("mov cr0, {}", in(reg) cr0);
```

读这段代码最该注意的一条事实，在中间那句注释里：**在统一可扩展固件接口路径上，分页已经是开的**（固件自己就在长模式里运行，`CR0.PG` 为 1）。于是这一步真正做的事不是"打开分页"，而是**把页表根换成自己那张**——因为固件那张表里没有第 2 章刚铺好的恒等段与内核高地址段。

这条事实会改变读者对整个引导链的理解方式。C 版那一串位操作看起来是"从关到开的仪式"，而同一语义（把地址翻译接到自己这张表上）在 64 位统一固件接口机型上落成了一个不同的动作。第 2 章的依赖仍然成立且更重要了：**因为分页早已开着，换根那一刻新表必须已经完整**，连"从关到开"这段缓冲都没有。

另外两处的写法也值得逐条对上第 2 章的结论：

**先置不可执行位，再换根。** 注释给的理由：不可执行位没开时，页表项的最高位是保留位，一旦表项用到它就直接触发页错误；而本项目的写保护与只读策略都要用那个位。固件"恰好"把它开着——注释随后把责任划清：这个不变量是自己的，不是固件的，所以显式读改写（先读回再或上需要的位，以保留固件已经设好的长模式位）。

**写保护位也要自己设**，因为固件不保证它开着；没有它，内核可以写只读页，只读保护等于不存在。

**全局页位最后按 CPU 特性开。** 而第 2 章用的 `kernel_read_write` 带的就是全局标记——注释直说：全局页位不开，这个标记无效；开了之后，带标记的内核映射能**撑过换页表根**。这解释了为什么第 2 章的恒等段与内核段都选这个权限组合。

ARM64 的跳转前多一条指令（`os/kernel/src/lib.rs:bootstrap_to_kernel_image`）：

```rust
core::arch::asm!(
    "isb",
    "br {entry}",
    in("x0") blob_phys.0,
    entry = in(reg) entry,
    options(noreturn)
);
```

这段代码本身没有为 `isb` 留注释，所以本篇标明它的依据来自 ARM 架构手册对指令同步屏障的定义：它丢弃已经在流水线里的指令，保证下一条取指发生在先前对系统寄存器的修改生效之后。放在这里读就说得通：紧前面刚把页表根写进两个转换基址寄存器，如果跳转目标已被预取，它会用**旧映射**去解算那个高地址，而旧映射里没有它。这一条与"分页早已开着"同样属于**边界条件的差别**：C 版写完 `CR3` 再开分页位，顺序天然把刷新留在中间；ARM64 写完基址寄存器后要立刻跳，屏障就得自己补。要亲眼验证它，需要在真机上观察去掉 `isb` 之后是否还能进入高半区。

---

## 第 4 章　内核和模块不能压在同一块物理内存上

内核与一批启动模块（进程管理、虚拟内存、文件系统、根服务等用户态服务器镜像）都被搬到物理内存里，位置由各自动态决定。现在问一件第 2 章与第 3 章一直默认成立的事：**谁保证它们没有互相覆盖？**

先看数据从哪来。`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 的开头：

```c
	multiboot_info_t *mbi = &cbi->mbi;
	...
	/* get our own copy of the multiboot info struct and module list */
	memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));
```

`ebx` 指向的结构属于 GRUB 放在内存里的东西。**先抄一份进自己的全局结构，才谈得上在后面依赖它**：那块内存不是引导代码的资产，引导服务退场后随时可能被别人使用。这个"先拥有数据"的动作在第 7 章会以另一种形式重现。

然后是查重。C 版的做法比直觉多做了一步：

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
		/* We cut out the bits of memory that we know are
		 * occupied by the kernel and boot modules.
		 */
		cut_memmap(cbi,
			cbi->module_list[m].mod_start, 
			cbi->module_list[m].mod_end);
	}
```

循环遍历的是 `mods_with_kernel`，比固件报的模块数多一个；多出来的那一项是内核自己——`mod_start/mod_end` 填的是内核镜像的物理边界，`kern_mod` 记下它的下标。于是"内核与模块是否重叠"不需要另一套代码：同一套两两查重把它覆盖了。代价是数组要多留一格，`assert(k < MULTIBOOT_MAX_MODS)` 守的就是这一格。

判重叠的宏在这里承担了关键的正确性：

```c
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
			INRANGE(mod1, mod2->mod_end-1))
```

第二个宏比较的是"另一个模块的**末地址减一**"。请拿两个具体数字自查：模块 A 占 `[0x100000, 0x200000)`，模块 B 占 `[0x200000, 0x300000)`。如果按 `mod_end` 比较，B 的起始地址 `0x200000` 会落进 `INRANGE(A, 0x200000)`？不会（A 的上界是严格小于）；但反过来查 A 时，A 的末地址 `0x200000` 恰好等于 B 的 `mod_start`，会被判成 A 落进 B——**于是两个合法相邻的布局互相误判、当场 `panic`。** 用 `mod_end-1`（区间内真实存在的最后一个字节）就把半开区间约定收进来了。

**这是一个"边界写法决定误判"的完整例子，而它被固化在重写的测试里。** Rust 侧的对应函数是 `os/boot-shim/src/loader.rs:first_overlapping_pair`，测试组共 5 条，其中一条专门测首尾相接：

```
test_first_overlapping_pair_disjoint
test_first_overlapping_pair_overlap
test_first_overlapping_pair_abutting
test_first_overlapping_pair_returns_first
test_first_overlapping_pair_empty
```

（均在 `os/boot-shim/src/loader.rs`。）`abutting` 那条就是 `mod_end-1` 的行为对应物：如果有人把判断改成闭区间，这条测试会红。同一文件里还有模块装载契约的两条（`test_load_boot_modules_loads_all_in_contract_order`、`test_load_boot_modules_panics_on_missing`）与一条加载后清零（`test_bss_region_is_zeroed_after_load`）。

顺带回答一个可能浮上来的问题：**为什么内核要在引导期就检查这个？** 因为一旦压住，后果不是报错而是覆盖——第 5 章的切除会把这块重叠内存当作空闲继续分配，第 2 章的页表又会写进这些页，最后坏在第 3 章换根之后的第一次取指。

---

## 第 5 章　可用内存清单的四种操作，其中一种在边界上不成立

内核需要一张"还能用哪些物理内存"的清单，因为建页表要拿页、模块占位要扣减。`minix3/minix/kernel/arch/i386/pg_utils.c` 提供了四种操作。用"账本"称呼它只在一件事上成立：这张清单确实就是一份记录（登记一笔、冲销一笔、剩余额），此外不需要它承载别的意思。

**登记**（`minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`，节选）：

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

三件事：超过 4 GB 的段整段丢弃、跨界那段截短（32 位寻址上限）；基址向上取整、长度向下取整，使登记的每一段都是整页——**向内收**，因为不能把不完整的页交付出去；顺手推高一条"真实内存最高水位"。这条高水位线是第 6 章的输入。

**切除**（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`，节选）：

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

对齐方向与登记**相反**：起始向下、末尾向上，切除区间被扩到整页边界。原因很具体：内核与模块的加载地址不保证页对齐，如果只切除对齐后的内层，**一个已经装着代码的半页仍会在清单上显示为可用**，下一次分配就会把那一页交出去。向内收是省内存，向外扩是保正确。

另外那两行 `add_memmap` 补回左右残段：切除可能把一段可用内存劈成两半，所以先作废原条目、再把左右剩下的部分登记回去。

**从尾取页**（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`，节选）：

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

它从**最高那段的尾部**倒着拿一页，专供引导期建表。取高位是为了把低地址留给需要的东西、并让大块连续区域前端少被切碎。

**从低取整块**（`minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest`）就不一样了。请把它的调用行和 `cut_memmap` 的签名并排放：

```c
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end);
...
	assert(lowest != EMPTY);
	cut_memmap(cbi, lowest, len);
```

第三个实参的位置是 `end`，传进去的是 `len`。用具体数字走一遍：最低可用段是 `[0x100000, 0x20000000)`，要分配 4 KB，于是 `start = 0x100000`、`end = 0x1000`。循环里每一段都做 `substart = max(start, memaddr)`、`subend = min(end, memend)`，得到 `substart = 0x100000`、`subend = 0x1000`，`substart >= subend` 成立 → `continue`。**整张清单一段都没被摘掉。**

这个函数确实返回了一个正确的最低地址、也确实记了一笔分配量（`kernel_allocated_bytes_dynamic += len`），但**"占用"这半件事只在最低段基址恰好为 0 时成立**（此时 `start = 0`、`end = len`，边界正常）。C 版里它唯一的调用点在辅助处理器启动路径（`minix3/minix/kernel/arch/i386/arch_smp.c`，那一行写的是 `trampoline_base = alloc_lowest(&kinfo, tramp_size)`）。**这就是"名字说做 X、实际只做 X 的前半"的一类真实样本**，值得留下的方法是：看到"分配"这个词，去确认它有没有真的把那块内存从可用清单里拿掉。

重写没有保留这个分配器：辅助处理器的早期入口是把空闲图扫一遍来挑地址（`os/arch/src/x86_64/ap_early_entry.rs` 的注释指回 C 的 `alloc_lowest`），"挑"与"扣减"由同一段代码完成。清单本身在 Rust 侧换了形状——定长数组加计数变成切片（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo` 的 `memmap` 字段），但容量上限仍是 C 的那个常量（`os/kernel/src/globals.rs:MEMMAP_REGION_STORE_LEN = crate::memmap::MAXMEMMAP`）。

---

## 第 6 章　越过高水位线的地址为什么必须关缓存

第 5 章的登记函数推高了一条"真实内存最高水位"，第 2 章的铺表函数第一件事就是断言它非零。现在可以回答这两者为什么相关。回到 `pg_identity` 里的条件（已在前文全文给出）：

```c
		if((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB)
			<= (phys & I386_VM_ADDR_MASK_4MB)) {
			flags |= I386_VM_PWT | I386_VM_PCD;
		}
```

循环把 4 GB 全铺满，而真实内存只到 `mem_high_phys`。**高水位之上那些地址并不是"空的内存"，而是设备寄存器窗口和地址空洞**——第 4 章与第 5 章反复出现的 `mm_type != MULTIBOOT_MEMORY_AVAILABLE` 被跳过的，正是这一类地址。对设备寄存器开缓存会破坏"写完立刻读回来看状态"这类语义：写可能滞留在缓存里没落到设备，读可能拿到旧值。所以越过水位线的每个 4 MB 段补上两个位：写穿透、不进缓存。源码注释直说："We map memory that does not correspond to physical memory as non-cacheable."

于是那条 `assert(cbi->mem_high_phys)` 的含义清楚了：**它守的是"先登记才能铺表"这条数据依赖**——没有水位线，就无法判断某一段该不该关缓存；而没有关缓存的恒等段，可能在第一次访问设备寄存器时静默地读到错值。这是一处不会当场崩溃、因而更难查的失败。

再看那个不对称，因为它在第 2 章已经出现过一次、这里才是它的完整解释。`pg_identity` 的标志带 `I386_VM_USER`，而 `minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel` 的不带：

```c
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
	        pagedir[pde] = kern_phys | I386_VM_PRESENT | 
			I386_VM_BIGPAGE | I386_VM_WRITE;
```

在 32 位单一架构上，这一位对引导期的访问没有影响（处理器已在最高特权级，用户位不参与判定），所以恒等映射带不带它，行为一样；真正表达意图的是内核高半区**不带**它。而且这份恒等映射活不到运行期：虚拟内存服务建自己的页表时显式跳过大页条目（`minix3/minix/servers/vm/pagetable.c:pt_init`），另起一张表。

到了 64 位多架构，同一位从"无所谓"变成"必须不带"（第 2 章的 RISC-V 结论：带用户位的页监督器不能取指，除非设 SUM）。**同一个语义"引导期过渡映射只供内核自己使用"，在 32 位上落成一位可有可无的位，在 RISC-V 上落成一条硬约束。** 而 Rust 侧不再有"越界段关缓存"这件事：整段低地址统一用同一组权限位铺，既不为设备窗口特殊处理，也就不再需要水位线——`mem_high_phys` 这个字段因此在契约里消失了。这属于第 8 章与第 9 章会反复出现的模式：**被淘汰的字段里，有一类不是不需要了，而是它的职责被另一种做法吸收了。**

---

## 第 7 章　内存映射快照必须取在所有分配之后

C 版拿到的是一张已经算好的清单，且占位由自己用 `cut_memmap` 扣除。64 位的统一可扩展固件接口路径不同：内存状态要**问**出来，而问到的结果取决于**什么时候问**。看 `os/boot-shim/src/uefi_helpers.rs:prepare_boot` 的真实顺序（保留原有的里程碑打印）：

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
```

固件的空闲内存报告是**当前状态的快照**：已经分配出去的页在活图里被改标成引导加载数据一类的用途，"只取可用内存"的视图自然排除它们。于是顺序决定一切：

**如果快照在占用之前拍，那份清单就会把正在被使用的页报成空闲，而它接下来要交给内核的页帧管理——等于把自己的页表页送人。**

`prepare_boot` 里的注释把这件事说得更狠：它原本只覆盖前面两笔分配，模块占页在快照里仍被列为可用，结果内核侧要额外扣除十几处，切出的碎片直接超过容量上限。这就是为什么快照必须晚于**全部**分配（内核镜像段、每个模块、页表根、连续区）。

紧跟快照的两条检查，函数体全部在这里（`os/boot-shim/src/uefi_helpers.rs:assert_bootstrap_outside_memmap`）：

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

第 4 章刚看过一遍这种半开区间不相交条件（这次写法是 `base + len <= rbase || rend <= base`，即新块整体落在对方左外或右外）。

但这两条检查**只覆盖页表根与连续区**，没有覆盖内核镜像段和模块段。把两者一起检查会立刻误报：本项目的装载模型是"引导垫片把内核与模块**直接写进可用内存**，不经固件分配"，它们在固件活图里仍然是可用，所以必然出现在清单里。它们需要的是另一套处理：内核侧按引导占用逐段扣除。**只有真正经固件分配的那两笔，才该由快照顺序与这条断言保护。**

> **这里停住。** 那个逐段扣除发生在哪一行、扣完的清单怎么交给页帧管理，本篇不讲——它回答的是"运行期谁能动这张清单"，属于页表与内存篇。**现在只需要拿到一个结论：同一张内存清单被两组规则保护，一组靠分配与快照的先后，一组靠扣除。** 知道有这两组、各守一段，就够往下走。

契约构造那一步还有两个实参值得注意：`PhysBytes(0), 0`，对应 C 的 `bootstrap_start` 与 `bootstrap_len`（第 8 章会在 `param.h` 里看到这两个字段）。C 用它们标记"内核中一小节不参与分页的代码"，引导完成后归还；Rust 的内核从第一条指令就在高半区、从不关着分页跑，那段蹦床在另一个镜像（引导垫片）里而不在内核里，所以长度必须为 0——`os/libs/minix-boot/src/kernel_info.rs:validate` 里有一条断言要求它等于 0。留着非零反而危险：一次过量的归还把固件、设备树、引导垫片自己都扫成可用内存。

最后一行也属于同一类事实：`exit_boot_services()` 之后，代码改用直接写串口的方式输出，不再调用固件的文本输出服务。**能力随服务退出而消失**，而"这一步之后是否还要打印"恰恰是最需要看得见的时候。

---

## 第 8 章　跨边界只能传值，而传过去的指针还有有效期

现在可以处理整篇最难的一件事：引导代码把自己消失之后，内核继续用它给的信息。先看 C 版的交接物长什么样（`minix3/minix/include/minix/param.h:kinfo`，节选，保留原有分段注释）：

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
        int                     freepde_start;  /* lowest pde unused kernel pde */
        char                    param_buf[MULTIBOOT_PARAM_BUF_SIZE];

        /* Minix stuff */
        struct kmessages *kmessages;
        int do_serial_debug;    /* system serial output */
        int serial_debug_baud;  /* serial baud rate */
        int minix_panicing;     /* are we panicing? */
        vir_bytes               user_sp; /* where does kernel want stack set */
        vir_bytes               user_end; /* upper proc limit */
        vir_bytes               vir_kern_start; /* kernel addrspace starts */
        vir_bytes               bootstrap_start, bootstrap_len;
        struct boot_image       boot_procs[NR_BOOT_PROCS];
        int nr_procs;           /* number of user processes */
        int nr_tasks;           /* number of kernel tasks */
        char release[6];        /* kernel release number */
        char version[6];        /* kernel version number */
	int vm_allocated_bytes; /* allocated by kernel to load vm */
	int kernel_allocated_bytes;		/* used by kernel */
	int kernel_allocated_bytes_dynamic;	/* used by kernel (runtime) */
} kinfo_t;
```

源码自己的注释已经把它分成四段：固件直接给的、从固件信息推导的、引导阶段填的、纯运行期杂项。四段住在**同一个全局变量**里，`pre_init` 填、`kmain` 读，同一个镜像、同一个数据段。所以 C 版从来不需要问"传递什么"——它就在那儿。

重写的处境不同。引导垫片与内核最终是两个二进制，而引导垫片这份镜像在交接之后可能被回收或重新映射。假设交接物里放一个函数指针（比如"调用这个回调去解析设备树"）：指针的地址在引导垫片镜像里解析，函数体却属于另一个包。**引导垫片内存被回收之后，这个指针指向的东西随时不再是那段代码；内核照着调用就跳进一个悬垂地址。** 所以重写把这条边界钉成：跨边界传递的东西必须全部是值语义的数据（`os/libs/minix-boot/src/platform.rs` 的文件头就是这条推演；`os/libs/minix-boot/src/kernel_info.rs` 里 `platform_sources` 的说明写着"No function pointers cross the boundary"）。需要传递"能力"时，传一个不透明分类标签（`PlatformDescKind`，本质是一个 `u32`）加一个物理地址，由内核在自己这边查表决定怎么处理。

那么"能力"由谁保证按次序被调用？重写把两套固件收进一个接口类型（`os/libs/minix-boot/src/boot_shim.rs:BootShim`）。接口体只有一条方法：

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

它买到四件确实有用的东西：两套固件必须提供同名同签名的一步，缺了编译不过；每个固件的实现是一个整体，不会把一边的清单与另一边的分配页拼在一起；可以注入假实现做单元测；调用方不需要按固件写条件编译。

而把这一条与另一条事实并起来看，就能确定它**不**买什么：`prepare_boot` 没有接收者（`self` 一个都没有），它只在引导垫片内部被调用一次，内核从不持有、也不接收一个"引导垫片对象"。方法体内部按什么次序调用哪几步、有没有真的退出固件服务、快照拍得对不对时机，类型系统一概不管。

**所以次序的保证来自两个更朴素的地方：实现体里那几行的排列，以及第 7 章那种运行期断言。** 对照 C 版反而更清楚：`pre_init` 是一条直线函数体，七行写死了次序，天然不可能漏掉一步——它没有抽象层，也就不需要为抽象兜底。加了抽象层，就要另外补回流程保证；把它说成"编译器会保证流程完整"是超出实现的说法。

即使全部换成值，也还剩一个问题：值里的指针指得到东西吗？

先看参数寄存器这一关。x86-64 与 RISC-V 的生产路径里，内核侧的接手函数是 `os/kernel/src/lib.rs:arch_boot`，它做的第一件事是把这个结构**按值收进内核自己的数据段全局**，之后一切（建页表、跳主流程）都用全局副本的引用：

```rust
store_kernel_info(kernel_info);
let kinfo: &'static KernelInfo = crate::kernel_info()
    .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
```

为什么不直接把参数引用递给主流程？因为**在 x86-64 的调用约定里，第一个整型参数寄存器是调用方保存的**（被调用方可以随意复用它）。引导垫片与内核此刻虽在同一镜像内，但中间经过了固件的代码生成，那条寄存器可能被跨调用覆写。若把栈上那个引用经一段不再返回的内联汇编直传下去，寄存器一旦被覆写，主流程拿到的就不是契约而是恰好留在里面的值——它可能指向一段只读字符串数据，校验函数读到一串字符就中止。按值收进数据段，等于**把依赖从寄存器手上收回到链接期常量**：全局副本的地址由链接期决定，不经过任何寄存器交接。

第二关是承载页。`store_kernel_info`（`os/kernel/src/lib.rs:store_kernel_info`）搬进数据段的不是"一个字段"，而是三类清单各配一块专用存放区（`os/kernel/src/globals.rs`）：

```rust
pub(crate) static RESERVED_REGION_STORE: SyncUnsafeCell<
    [minix_boot::MemoryRegion; RESERVED_REGION_STORE_LEN], ...>;
pub(crate) static MEMMAP_REGION_STORE: SyncUnsafeCell<
    [minix_boot::MemoryRegion; MEMMAP_REGION_STORE_LEN], ...>;
pub(crate) static BOOT_MODULE_STORE: SyncUnsafeCell<
    ...>;
```

为什么是这三处？看一眼它们装的东西：`reserved_regions`、`memmap`、`boot_modules` 都是切片，指针部分指向引导垫片向固件要来的内存池。**那片内存只被固件自己的一比一表映射着。** 内核跳进高半区之后，它自己的页表不重放那层映射，同一个虚拟地址读出来会是零。所以必须把字节搬进内核数据段——那块内存整个运行期都在映射里——再把指向它的指针交出去。三处同病同治，不是某个字段特殊。

**这里能看清"值语义"这件事的限度：它是必要条件，不是充分条件。** 契约里放的是值，不代表值里的指针在对面还有效。

那第 7 章留下的 ARM64 跨镜像路径呢？它靠一份文件头把有效期讲明了（`os/libs/minix-boot/src/handoff.rs`，本文摘译）：两个镜像各链接一份内核代码，**不共享数据段全局**，所以引导垫片在物理内存里建立的一切，都要经这块交接对象承载；交接对象被写进一页 bump 分配的物理页，由 `x0` 递给高半镜像的入口；`KernelInfo` 是可复制的、它的切片载荷指向稳定的、恒等映射内的物理内存（那份池在退出固件服务后的引导窗口内仍然存活），所以整块结构按字节拷贝是成立的——**独立镜像会在第一次切换页表根之前把这些切片读完。**

**换句话说：跨边界的数据不仅要全是值，它还有一段有效期，而到期时刻由第一次换根决定。** 这句话同时解释了上面三块存放区（提前把字节搬进永久映射）与第 9 章的两种解法（换根之后自己还得有取指映射）。

第 7 章提到的 `validate`（`os/libs/minix-boot/src/kernel_info.rs:validate`）把其中几条钉成断言：引导代码段长度必须为 0、内核大小非零、内核栈顶 16 字节对齐、内存清单非空、模块清单非空、参数缓冲不越上限。数量现算：

```bash
cd os && rg -c '#\[test\]' libs/minix-boot/src/kernel_info.rs boot-shim/src/loader.rs \
    boot-shim/src/opensbi_helpers.rs boot-shim/src/uefi_helpers.rs libs/minix-boot/src/platform.rs
```

读数是 `kernel_info.rs` 9（校验函数一条正向、六条反向、一条取值）、`loader.rs` 13、`opensbi_helpers.rs` 13、`uefi_helpers.rs` 1（引导垫片包合计 27）、`platform.rs` 8（`minix-boot` 包合计 17）。另外三个文件里一条单元测试都没有：`os/boot-shim/src/main.rs`、`os/boot-shim/src/lib.rs`、`os/libs/minix-boot/src/boot_shim.rs`、`os/libs/minix-boot/src/handoff.rs`。

**分布本身就是结论**：入口段与跨镜像载荷都是"只在真机上才成立"的部分，宿主测不到；这也意味着本章"必须先按值收进全局"这条守卫没有回归测试，它靠代码形状与注释维持。这是本篇披露的真实盲区，不伪装成已覆盖。

---

## 第 9 章　换根之后内核自己还得有取指映射：两种解法

第 8 章末尾那句"到期时刻由第一次换根决定"需要展开，因为这里是引导链与运行期衔接处最容易读错的一步。

问题是这样：**内核此刻用来取指的那段映射，正好在会被换掉的那张表里。** 一旦开始为进程建立地址空间、切换页表根，内核就把自己取指所用的那张表撤了。x86-64 与 ARM64 选了两种不同的解法，而它们回答的是同一个问题。

**ARM64 的解法：跳进永不切换的那半边。** 它有两个转换基址寄存器，按地址位 55 分流（置 1 的高地址走其中半边），而引导垫片建的这张根表把内核高半区映射在那里，且**这半边从不参与切换**。于是引导垫片做完"必须在自己的上下文里做的事"（收信息、建表、开分页）之后，绝对跳进独立内核镜像的高半区入口，从那条指令起内核全部在高地址运行，对换根免疫（`os/kernel/src/lib.rs:bootstrap_to_kernel_image` 与 `os/libs/minix-boot/src/handoff.rs` 的文件头说明）。

跳之前那一步的形态在第 3 章与第 8 章都出现过：一页 bump 分配的物理页装交接对象，`x0` 带指针，`isb` 之后 `br`。第 8 章解释过为什么不能沿数据段全局传，这里补一条同类的谨慎：`x0` 的绑定写在汇编块外面（`in("x0") blob_phys.0`），注释给的理由是——如果在块里手写一条 `mov x0, …`，那会是一次未声明的破坏：编译器有权把跳转目标 `{entry}` 也分配到 `x0`，于是那句移动会先把跳转目标覆写掉。

**x86-64 的解法：不换地方，而是把自己取指用的那些窗口补进新表。** 根据写在契约的字段说明里（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`；原文在该段里夹了两处供人追溯问题的编号与日志路径，下面引文只删去这两处括号，其余逐行照录）：

```rust
    /// ExitBootServices (everything the memmap snapshot reports as
    /// non-conventional RAM: the loaded boot-shim/kernel PE image, UEFI
    /// boot-services heap objects such as `KernelInfo` itself, ACPI
    /// tables, LOADER_DATA allocations including the bump region and
    /// module blobs).
    ///
    /// the kernel EXECUTES from these addresses — it is
    /// statically linked into the boot-shim PE image, so after EDKII
    /// relocation its code / GDT / IDT / TSS live at low physical
    /// addresses, not at the higher-half link addresses of the
    /// (never-executed) standalone kernel.elf copy. The VM-built process
    /// page tables must replay these windows identity (VA = PA,
    /// supervisor-only) or the first CR3 switch to such a table
    /// fault-triples on the next instruction fetch.
    ///
    /// OpenSBI/U-Boot and host tests pass `&[]` (no runtime identity
    /// window; those platforms do not link the kernel into the shim).
    pub reserved_regions: &'static [MemoryRegion],
```

这段说明同时给了三件事：谁需要这些窗口（运行期页表）、为什么需要（内核就在此处取指：它被静态链接进引导垫片镜像，固件重定位后代码、全局描述符表、中断入口表、任务状态段都落在低物理地址那一串上，而不是在那个从未被执行的内核文件的链接地址上）、以及哪些平台传空切片（RISC-V 与宿主测试——那些平台不把内核链进引导垫片）。

于是两条路对上了：

| 问题 | ARM64 | x86-64 |
|---|---|---|
| 内核第一次换根后从哪取指 | 跳到高半区，落在永不切换的半边 | 仍在低物理地址，但运行期页表补重放了那些窗口 |
| 为此需要什么数据 | 高半区地址、根页、分配区游标（交接对象） | `reserved_regions` 清单 |
| 换根是否致命 | 否（那半边不切） | 否（新表里有内核取指用的窗口） |

**"内核代码在第一次换根之后还要有取指映射"是不变量；"跳高半区"与"重放恒等窗口"是两种选择。** 这个区分值得带走：读重写代码时遇到一个看起来多余的字段或一次奇怪跳转，先问它在满足哪条不变量，再判断它是唯一解还是解法之一。

RISC-V 的形态与前两者都不同，值得补上，因为它解释了一个容易被当成"实现偷懒"的事实。该模块文件头给出了引导链（以下是英文原图的逐行转译，不是逐字照录）：

```text
OpenSBI (M 模式)      ← 监督器调用接口的运行时服务：控制台、定时器、处理器间中断
  ↓ jr a1
U-Boot (S 模式)       ← 提供磁盘与文件系统驱动，有 fatload、go
  ↓ fatload + go
boot-shim (S 模式 ELF) ← 本模块
  ↓ 解析 ELF、复制
kernel (S 模式 ELF)
```

文件头接着给了那条硬约束：工具链没有 `riscv64-unknown-uefi` 这个目标，也发不出 PE 可执行格式。PE 是一种可执行文件格式（Windows 上的 `.exe` 与统一可扩展固件接口世界里的 `.efi` 引导应用都用它）；固件接口只肯加载它能解析的那种二进制，所以调用它的程序本身必须是那个格式。**即使 U-Boot 实现了固件接口的一个子集也用不上，因为造不出那个能去消费它的二进制。** 测试面能直接看到这条链的形状（`os/qemu-tests/test-riscv64-uboot.sh` 生成的 `boot.cmd`）：

```
fatload virtio 0 ${loadaddr} boot.elf
```

**读文件这件事是 U-Boot 替我们做的**，而引导垫片在 x86-64 与 ARM64 上用固件的文件协议自己读。三种平台在"信息从哪问出来、文件谁替我读"上各有差异，但契约的形状与"先备好映射再换根"这条不变量没有分叉——差异被安排在同一个问题的不同答案上，而不是各说一套。

最后有一条容易被读歪的事实要挑明：`os/boot-shim/src/opensbi_helpers.rs:build_memmap` 里那份单区硬编码清单**不是**当前 RISC-V 的生产引导面。它的文档注释直说是为 QEMU virt 构造硬编码清单、返回一个覆盖 DRAM 的单段。真实生产引导由内核镜像自己充当引导体（`os/kernel-image/src/bootface.rs`），它复用同一批构件（构造契约、文件表、bump 分配），但改成从移交时带来的设备树指针解析内存布局。**"库里有这段代码"不等于"生产走这段代码"**——读任何一侧重写时都值得带上这条判别。

---

## 第 10 章　两类复位各查什么

引导期失败的共同形状是"没有输出，机器复位或静默死循环"。前九章已经把这条链上的依赖排出来，所以定位方法不是重试，而是**把断点认到某一章**。

两条先做的检查（可直接复制）：

```bash
# 一：确认交权前的动作序列与守卫都在位
#    预期能看到：build_memmaps（快照）、两条 assert_bootstrap_outside_memmap、
#    exit_boot_services、build_bootstrap_root_and_enable、store_kernel_info
rg -n "build_memmaps\(|assert_bootstrap_outside_memmap|exit_boot_services|\
build_bootstrap_root_and_enable|store_kernel_info|validate\(" \
    os/boot-shim/src/uefi_helpers.rs os/kernel/src/lib.rs

# 二：确认契约递出的清单条数（内核侧第一行打印的就是这些字段）
rg -n "arch_boot arg=|blen=|reslen=|KernelInfo:" os/kernel/src/lib.rs \
    os/libs/minix-boot/src/kernel_info.rs
```

正常启动日志里的里程碑顺序是：进入准备阶段 → 分配完成 → 装入内核 → 装入模块 → 清单可用（快照已拍）→ 构造契约 → 退出固件服务 → 内核侧第一行。日志断在哪两行之间，问题就在两步之间。

**开分页（或换根）之后立刻复位。** 查当前执行所在那段映射的覆盖范围：正在执行的代码所在地址，是否落在第 2 章那个循环铺到的区间内；重点看有没有把"没被标成可用内存"的那些用途段算进去——第 2 章说过，只铺可用段是这类故障最常见的成因。若走的是第 9 章 x86-64 那条路，还要确认新表里是否重放了内核实际执行所在的窗口。

**第一次切换地址空间之后复位。** 这与上一类不同，别混着查。此刻旧表是好的，坏的是新表：内核代码是否还待在那半边会被切换掉的位置（第 9 章），或者带全局标记的映射是否真的生效（第 3 章：全局页位不开，标记无效）。

**内核一进校验函数就中止。** 先分辨两件事：字段是"没递过来"还是"递过来但读不到"。第二条检查命令的输出给了可分辨的信号：内核侧第一时间打印契约指针、引导代码段长度与保留窗口条数；条数为 0 说明丢在跨边界之前，条数对但内容全零说明载荷指向的内存已经不在映射里（第 8 章那三块存放区存在的理由）。

---

## 收束：这条链上哪些是不变量，哪些是实现选择

由各章分别得到的结论汇总如下，不再复述论证。

**不变量（机器或数据规定的，换任何写法都绕不开）**

1. 高级语言函数被调用之前必须已经有栈（第 1 章）。
2. 中断入口表不存在时，中断允许位必须为关（第 1 章）。
3. 启用翻译之前，新表里必须已经覆盖"接下来要取指与取栈的地址"（第 2 章）。
4. 判断一段地址是否真实内存，必须先有登记结果——铺表依赖登记（第 2 章与第 6 章）。
5. 若干镜像不得压在同一块物理内存上；区间是半开的，边界写法决定误判（第 4 章）。
6. 切除已占用区间必须向外取整到页边界，否则半个已占用页会被当成空闲（第 5 章）。
7. 设备寄存器窗口的映射必须禁缓存（第 6 章）。
8. 向固件问出的内存状态是瞬时值，所以快照必须晚于本方全部占用（第 7 章）。
9. 跨边界传递的东西，不得引用会先消失的那一侧（第 8 章）。
10. 第一次换根之后，内核自己还得有取指映射（第 9 章）。

**实现选择（可以不同，本项目这样做有它的理由）**

1. C 版"先清零再按依赖点亮控制位"（换确定性）；Rust 版"读回当前值再置需要的位"（换不冲掉固件已设的位）——第 3 章。
2. ARM64 用"跳进永不切换的半边"；x86-64 用"把自己取指用的窗口重放进新表"——第 9 章。
3. 把内核静态链接进引导垫片镜像（省一次跨镜像装入，代价是需要 `reserved_regions`）——第 9 章。
4. RISC-V 走监督器调用接口加 U-Boot，因为工具链发不出该架构的 PE 镜像——第 9 章。
5. 用接口类型统一两套固件的差异（买到接口形状统一与可测性，买不到流程次序）——第 8 章。

下一篇处理本篇当作前提使用的那个东西：内核镜像的链接地址与装载地址为什么不同，以及高半区的目的地是怎么被铺出来的。
