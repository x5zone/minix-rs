# 01 · Boot-shim 引导准备与控制权交接

> **C 真值源**：`minix3/minix/kernel/arch/i386/head.S`、`minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/arch/i386/pg_utils.c`、`minix3/minix/kernel/arch/i386/kernel.lds`、`minix3/minix/include/minix/param.h`、`minix3/sys/arch/i386/include/multiboot.h`、`minix3/etc/boot.cfg.default`、`minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`、`minix3/minix/kernel/arch/i386/{memory.c,arch_smp.c}`、`minix3/minix/kernel/table.c`
> **Rust 实现**：`os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers}.rs`、`os/libs/minix-boot/src/{kernel_info,boot_shim,handoff}.rs`、`os/kernel/src/{lib,boot_alloc,globals,vm_handoff,misc}.rs`、`os/arch/src/arch/{pt_alloc,paging_ext}.rs`、`os/arch/src/x86_64/paging.rs`、`os/kernel-image/{check-layout.sh,src/main.rs}`
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。行号只以符号锚点出现，不引用具体行；引用位置由符号名定位。

内核不是从零开始的。它被放进内存时，机器已经由固件（UEFI 或 BIOS）与引导加载器（GRUB 或 U-Boot）跑过一段路：内存控制器已经初始化、可执行文件已经被读进 RAM、地址翻译已经按固件的安排在跑。但这些成果并不是内核可以直接使用的形式——它们要么散在别人填的结构体里，要么被别的代码写在某个寄存器里，要么干脆还没做（比如"哪些物理页是空闲的"这件事，在 GRUB 的 Multiboot 路径里只有一张粗糙的 BIOS 内存表）。

这一篇讲清内核接管之前的那一段：谁把机器带到了"内核可以开始执行"的状态，中间凑齐了哪几样东西，哪一步的次序错了会坏在什么地方。

---

## 1. 概述

### 1.1 内核的第一行代码之前还有一段代码

x86 上加电后，机器先执行固件；固件自检、初始化并找到可引导设备，把控制权交给磁盘上的引导加载器；引导加载器读自己的配置、把内核镜像与若干模块读进物理内存、切到保护模式，最后跳到内核的入口。这一段里只有最后一小截属于内核源码。

| 阶段 | 谁在跑 | 机器状态 | 属于本仓库源码吗 |
|---|---|---|---|
| 加电与自检 | 主板固件（BIOS/UEFI） | 实模式起步，固件自己完成模式切换与硬件初始化 | 否 |
| 加载可执行文件 | 引导加载器（GRUB/U-Boot） | 保护模式或长模式，已能访问磁盘与内存 | 否（只提供配置模板 `minix3/etc/boot.cfg.default`） |
| 引导准备 | 内核源码的最前一段（`head.S` + `pre_init()`） | 分页未开（x86-32），无栈可用、无堆、无中断 | 是 |
| 内核主流程 | `kmain()` 及其后 | 分页已开、内核在高地址执行 | 是（后续各篇） |

第三行才是本篇的对象。"引导准备"这四个字里包含两类工作：**收集**（从引导器留下的数据结构里读出内存布局、模块位置、启动参数）与**搭架子**（把可用内存记成账、把页表建起来、把分页打开）。两类工作都在同一段没有运行环境的代码里完成——没有栈指针可以用（要自己设一个临时栈）、没有分配器、没有任何报错输出通道以外的设施。

最后一句里"自己设一个临时栈"在 C 里就是一条 `mov` 指令，但它是这一整段代码能成立的前提：C 函数需要栈来放局部变量、参数与返回地址，而引导器跳过来时 `esp` 指向什么由引导器决定。 前提："为什么第一跳是一段汇编"就不用再单独解释了。

### 1.2 自举要凑齐的四样东西

不论引导器是 GRUB、U-Boot 还是别的什么，把机器交给内核之前必须凑齐四样：

```
    ┌──────────────────────────────┐
    │ ① 可用物理内存的清单          │  哪些物理区间可以被分配
    │   （内核与模块占用区已排除）  │
    └───────────────┬──────────────┘
                    │ 交给
                    ▼
    ┌──────────────────────────────┐
    │ ② 内核自己的位置与大小        │  内核镜像在物理内存的哪一段
    │   （物理基址 / 虚拟基址 / 长度）│  以及它要运行在哪个虚拟地址
    └───────────────┬──────────────┘
                    │ 用来判定
                    ▼
    ┌──────────────────────────────┐
    │ ③ 一张内核可运行在其上的页表  │  当前正在执行的地址必须被覆盖，
    │   （当前地址 + 目的地地址）   │  否则开启分页的下一条指令就取不到
    └───────────────┬──────────────┘
                    │ 开启分页后才谈得上
                    ▼
    ┌──────────────────────────────┐
    │ ④ 一份把上面三样交下去的结构  │  内核主流程要读的就是它
    └──────────────────────────────┘
```

四样之间有依赖，而且依赖的方向决定了次序：

- 清单先于用清单的人。`get_parameters()` 自己就是清单的第一个使用者——它把内核与各模块占用的区间从清单里切掉（`cut_memmap`），用的正是它刚建好的那份清单。如果先建页表再收集内存信息，页表页分配这一步就没有可信的可用区间来源。
- 内核的位置先于清单的裁剪。清单要排除的区间里包含内核镜像自己；内核镜像的物理区间从链接期常量来（`_kern_phys_base`、`_kern_size`），不来自引导器的数据结构。
- 页表先于开启分页，这是本节唯一一条"硬件强制"的次序。`CR0.PG` 置位后的**下一条取指**就已经在走页表；此时页表里如果没有当前指令所在的虚拟地址的映射，CPU 立刻产生页错误，而页错误处理程序此刻还不存在。
- 结构先于内核第一条 C 语句。`pre_init()` 返回的就是这份结构；它按值或按指针交给 `kmain()`。

还有一个容易读漏的次序：`pg_identity()`（当前执行地址的映射）排在 `pg_mapkernel()`（目的地地址的映射）之前。两者写同一张页目录数组，而且下标范围**重叠**（前者写 0..1023 全部条目，后者从 961 起），所以这个次序不是风格问题——后写的那一次会覆盖先写的那一段。

C 的这四样分别落在 `pre_init.c` 与 `pg_utils.c` 两个文件里：收集在 `pre_init.c`，记录与页表在 `pg_utils.c`。这个划分是 Minix3 的既有划分；重写把同一条分界换到了另一处——准备与运行分属两个可执行体，第 3 章讲这件事。

### 1.3 本篇范围与可以安全跳过的内容

本篇覆盖从引导器跳进内核（或跳进替它准备的那段代码）起，到控制权交给内核主流程为止的全部动作：引导器契约、参数收集、内存记账、模块装载的检查、页表建立与分页开启、以及这段代码在新设计里的组织方式。

以下内容不在本篇展开，遇到时给出指向：

| 可以跳过的内容 | 归属 |
|---|---|
| 链接脚本的虚拟地址/装载地址区别、`AT()`、两段式布局 | `02-higher-half-kernel.md` §2.1–§2.2 |
| 换栈与跳转必须成对、`k_initial_stktop` 落在哪一段 | `02-higher-half-kernel.md` §2.3–§2.4 |
| ELF 解析器内部（段迭代、错误分类）、段拷贝与 BSS 清零的实现 | `02-higher-half-kernel.md` §4.2 |
| 页表多级结构与页表接口的完整定义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`（§1 概念与 §3 设计决策）|
| 引导期建表之后的 Direct Map 覆盖建立 | 不在本篇展开：§4.6 只讲到建表与开启这三步，覆盖建立属分页运行期，该机制的专篇尚未生成（代码落点见 §3.7）|
| `kmain()` 起的三张表（GDT/IDT/TSS）与保护结构 | `03-kmain-cstart.md` |
| 设备树/ACPI 表的解析细节与硬件描述符 | `04-platform-discovery.md` |
| 时钟与中断控制器初始化 | `05-clock-interrupt-init.md` |
| UEFI 规范本身（协议、表、Boot Services 分类） | 外部资料；本篇只讲清用到的那几个调用 |

两处需要单独说明的边界：

`k_initial_stktop`（`head.S` 在 `pre_init` 返回后用作栈顶的符号）在本篇只作为边界出现，但它的落点在本仓可以判到高半一侧，依据是三条：① `mpx.o` 不在 `objcopy --prefix-symbols=__k_unpaged_` 的名单里（名单在 `minix3/minix/kernel/arch/i386/Makefile.inc` 的 `.for unpaged_obj in head.o pre_init.o … pg_utils.o …` 循环，没有 mpx.o）；② `mpx.S` 把 `k_initial_stack` 与紧随其后的标签写在 `.bss` 里；③ `minix3/minix/kernel/arch/i386/kernel.lds` 的 `.unpaged_bss` 输出段只收 `unpaged_*.o(.bss COMMON)`（被改名后的对象），而通用 `.bss` 输出段排在 `. += _kern_offset` 之后——mpx.o 的 `.bss` 落进后者，执行地址在高半一侧。所以"带 `__k_unpaged_` 字样 ⇒ 低地址"这条线索的前提不成立（该字样不是 `objcopy` 施加的结果）。仍未查清的是符号名最终怎么拼：`LABEL(...)` 这个宏在本仓没有定义（`grep -rn 'define LABEL' minix3/` 只命中无关的 external 代码，`minix3/sys/arch/i386/include/asm.h` 里只有 `_LABEL(x)`），因此 `head.S` 的引用如何解析无法从本仓确定——这不影响落点判定（落点由所在的输出段决定，与名字无关）。这条依据链与 `02-higher-half-kernel.md` §2.4 共享，两篇的判定应当一致。

模块启动镜像的 ELF 段拷贝细节同属装载，但它的实现分析（段迭代器、错误变体、BSS 清零）归 `02-higher-half-kernel.md` §4.2。本篇只讲到"哪些文件被读、按什么顺序被读、读不到怎么办"，不展开段拷贝算法。

---

## 2. C 源码分析

### 2.1 引导器交给内核的那份启动信息

引导器与内核之间靠三样东西对接：内核镜像里的一个识别头、跳转时的寄存器约定、以及一份由引导器填好的结构体。识别头嵌在内核二进制里（不是配置文件），引导器扫描镜像前 8KB 找到它才认这个内核（三个字段定义在 `minix3/minix/kernel/arch/i386/head.S:multiboot_magic`）。

```asm
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/head.S:multiboot_magic 及邻近字段。
 * 五处连续 .long 0 的语义见下方逐字段解码。
 */
.balign 8

#define MULTIBOOT_FLAGS (MULTIBOOT_HEADER_WANT_MEMORY | MULTIBOOT_HEADER_MODS_ALIGNED)

multiboot_magic:
	.long MULTIBOOT_HEADER_MAGIC
multiboot_flags:
	.long MULTIBOOT_FLAGS
multiboot_checksum:
	.long -(MULTIBOOT_HEADER_MAGIC + MULTIBOOT_FLAGS)
	.long 0
	.long 0
	.long 0
	.long 0
	.long 0
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

逐字段解码：

- `.balign 8`：把识别头对齐到 8 字节边界。这是比协议本身更严的选择（`.balign` 的实参由本文件决定）。
- `multiboot_magic` 的值为 `MULTIBOOT_HEADER_MAGIC`，定义在 `minix3/sys/arch/i386/include/multiboot.h` 为 `0x1BADB002`。
- `multiboot_flags` 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块请按页对齐）。**没有置视频位**（`MULTIBOOT_HEADER_HAS_VBE`，值为 `0x00000004`）——下一段那四个视频字段虽然写着 EGA/80/25，但按结构体注释（`Valid if mh_flags sets MULTIBOOT_HEADER_HAS_VBE`）引导器应当忽略它们。
- `multiboot_checksum` 的值是 `-(magic + flags)`。引导器验算 `magic + flags + checksum == 0`；三个 `long` 相加为零是"这个头部完整"的证明。它不是校验和算法，是一次加法恒等式。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`。前三个只在 `flags` 里置了 `HAS_ADDR` 位时才被读，后两个同样按位生效；这里都没置位，所以它们保持 0。这三个 0 是一个选择：**这个内核让引导器决定装载位置**，自己不指定。
- 视频部分的四个字段请求 EGA 文本模式、宽度 80、高度 25（`MULTIBOOT_VIDEO_MODE_EGA`/`MULTIBOOT_CONSOLE_COLS`/`MULTIBOOT_CONSOLE_LINES`）。

跳转时引导器约定两个寄存器：一个放确认魔术值，一个放它填好的信息结构体地址。这两个值在内核侧被一条断言和一次拷贝立刻消化掉（见 2.2、2.3）。两个魔术值只差一个十六进制位，方向相反：

| 魔术值 | 值 | 方向 | 谁读它 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | 引导器扫描内核镜像前 8KB |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | `pre_init()` 的第一条断言 |

配置文件里的每条指令各把一份信息送到上面这个交接面上：

```
# 逐字引用，minix3/etc/boot.cfg.default（三条 menu 行中的一行）
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
```

| 指令 | 送到哪里 | 内核哪一段读它 |
|---|---|---|
| `multiboot <内核路径> [命令行]` | 内核镜像被读到某物理地址，入口被跳转（**不给内核任何变量**）；`[命令行]` 整串成为 `mbi->mi_cmdline` | 镜像地址由链接期常量决定；命令行在 `get_parameters()` 里解析 |
| `load_mods <模块通配路径>` | 每个匹配到的文件被读进物理内存，地址与长度填进模块描述符数组，数组地址进 `mbi->mi_mods_addr`，条数进 `mbi->mi_mods_count` | `minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 把整个数组 `memcpy` 进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 只是命令行里的一个 `key=value` 词，随整串进 `mbi->mi_cmdline` | `get_parameters()` 解析命令行时逐词交给 `mb_set_param()`，最终以 `key=value` 形式落进 `kinfo.param_buf` |

三行里只有 `rootdevname=` 是"内核能直接看见内容"的——另两条指令的产物是一个地址或一段内存，内容要看引导器填的结构体。而 `rootdevname=` 能被内核"认出来"，靠的不是引导器，是内核自己的解析函数（见 2.3）。

`load_mods` 的模块数有一个上限，`MULTIBOOT_MAX_MODS` 定义在 `minix3/sys/arch/i386/include/multiboot.h` 为 20；`kinfo_t` 里的 `module_list[]` 就是按这个数开的数组（`minix3/minix/include/minix/param.h:struct kinfo`）。这个数在内核侧被一条断言当成硬约束，而不是"超了就截断"。

**如果反过来想**：`boot.cfg.default` 里 `load_mods` 与 `multiboot` 是同一条 menu 行里以 `;` 分隔的两个动作。假设把 `load_mods` 挪到 `multiboot` 之后——不是"模块加载不上"这么简单。`mi_flags` 里表示"有模块列表"的那一位由 `load_mods` 的动作置位；它不置位时，`get_parameters()` 里的 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会直接停住内核。停在断言上意味着：`do_serial_debug` 还没被 `mb_set_param` 设过（它排在解析命令行之后），串口上没有任何输出，现场只剩一个停在固定地址的 CPU。这是"次序错"的代价被放大到"连诊断都没有"的例子。

### 2.2 head.S：第一跳只做三件事

GRUB 跳到 `MINIX` 标签后执行的入口是 `minix3/minix/kernel/arch/i386/head.S:multiboot_init`，它只做三件事：设栈、跨过识别头、按 C 调用约定把两个寄存器值变成两个参数。

```asm
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/head.S:multiboot_init。
 */
multiboot_init:
	mov	$load_stack_start, %esp	/* make usable stack */
	mov	$0, %ebp
	push	$0	/* set flags to known good state */
	popf	/* esp, clear nested task and int enable */
	push	$0

        push    %ebx	/* multiboot information struct */
	push    %eax	/* multiboot magic number */
        call    _C_LABEL(pre_init)
```

逐行解码：

1. `mov $load_stack_start, %esp`：给自己设一个栈顶。`load_stack` 是同一个文件末尾 `.data` 段的 4096 字节静态空间（`.data` / `load_stack: .space 4096` / `load_stack_start:`），`load_stack_start` 是它的**高端**地址。这一行是"能进 C"的全部原因。
2. `mov $0, %ebp`：清零帧指针。栈回溯靠 `%ebp` 链逐帧走，`0` 表示链到此终止（内核 panic 打印调用栈、QEMU + GDB 远程调试都要用）。
3. `push $0` / `popf`：先把 0 压栈，再把它弹进 EFLAGS。净效果是栈深不变（`popf` 消耗掉刚压的 4 字节），改的是标志寄存器：关中断（`IF=0`）、清嵌套任务位（`NT=0`）。
4. 紧跟的第二个 `push $0`：栈上的一个空位。它与后面 `push %ebx`、`push %eax`、以及 `call` 推入的返回地址一起，把调用 `pre_init` 时的栈对齐到 16 字节倍数。
5. `push %ebx` / `push %eax` / `call pre_init`：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压，第一个参数（魔术值）后压。`pre_init(u32_t magic, u32_t ebx)` 的两个形参就位。
6. 到此为止是本篇的范围。下一行 `mov $k_initial_stktop, %esp` 与随后的 `call kmain` 属于换栈与跳转，归 `02-higher-half-kernel.md` §2.3；`k_initial_stktop` 落在哪一段属于 §2.4，本篇不采用结论。

三件事就是：设栈、跨过识别头、按调用约定传参。

`load_stack` 放在 `.data` 段而不是 `.bss` 段还有一个后果：它是文件里真实存在的 4096 个字节，装载时会从镜像拷进内存，不需要运行时清零；而如果它是 `.bss`，那么"栈未初始化"这件事就取决于装载器是否真的按 `memsz - filesz` 清了零——这是两件容易混起来的事，2.7 会看到 ARM32 端口在同一位置做了同样的选择。

**如果反过来想**：把 `push %ebx` 与 `push %eax` 的次序对调。i386 约定下最低地址是第一个参数，于是 `pre_init(magic, ebx)` 收到 `magic = ebx`——一个看起来像地址的数字（`multiboot_info` 结构的物理地址），`ebx` 收到 `0x2BADB002`。`pre_init()` 的第一条断言是 `assert(magic == MULTIBOOT_INFO_MAGIC)`：它在条件为假时调用 `panic()`，而此时 `do_serial_debug` 还是默认值 0、串口初始化也没做，所以现场表现是"内核第一步就停住、串口无输出"。报出来的"错误值"是一个合法的地址，很像"数据结构坏了"，而真正的原因是两条 `push` 的顺序。

### 2.3 get_parameters：把别人的数据变成自己的记录

`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 是收集阶段的正文。它做的第一件容易被跳过的事是**整份拷贝**：不长期持有引导器给的指针，而是把整个结构体复制进自己的全局 `kinfo`。

```c
/*
 * 逐字引用（略去函数开头的局部变量声明与 extern 链接符号声明），
 * minix3/minix/kernel/arch/i386/pre_init.c:get_parameters。
 * 省略号为略去的声明行，其后的每条语句逐字保留。
 */
void get_parameters(u32_t ebx, kinfo_t *cbi)
{
	...
	/* get our own copy of the multiboot info struct and module list */
	memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));

	/* Set various bits of info for the higher-level kernel. */
	cbi->mem_high_phys = 0;
	cbi->user_sp = (vir_bytes) &_kern_vir_base;
	cbi->vir_kern_start = (vir_bytes) &_kern_vir_base;
	cbi->bootstrap_start = (vir_bytes) &_kern_unpaged_start;
	cbi->bootstrap_len = (vir_bytes) &_kern_unpaged_end -
		cbi->bootstrap_start;
	cbi->kmess = &kmess;

	/* set some configurable defaults */
	cbi->do_serial_debug = 0;
	cbi->serial_debug_baud = 115200;

	/* parse boot command line */
	if (mbi->mi_flags & MULTIBOOT_INFO_HAS_CMDLINE) {
		...
		memcpy(cmdline, (void *) mbi->mi_cmdline, BUF);
		p = cmdline;
		while (*p) {
			var_i = 0;
			value_i = 0;
			while (*p == ' ') p++;
			if (!*p) break;
			while (*p && *p != '=' && *p != ' ' && var_i < BUF - 1) 
				var[var_i++] = *p++ ;
			var[var_i] = 0;
			if (*p++ != '=') continue; /* skip if not name=value */
			while (*p && *p != ' ' && value_i < BUF - 1) 
				value[value_i++] = *p++ ;
			value[value_i] = 0;
			
			mb_set_param(cbi->param_buf, var, value, cbi);
		}
	}

        /* let higher levels know what we are booting on */
        mb_set_param(cbi->param_buf, ARCHVARNAME, (char *)get_board_arch_name(BOARD_ID_INTEL), cbi);
	mb_set_param(cbi->param_buf, BOARDVARNAME,(char *)get_board_name(BOARD_ID_INTEL) , cbi);
	...
}
```

逐段解码：

- `memcpy((void *) mbi, (void *) ebx, sizeof(*mbi))`：把引导器结构体整份拷进 `cbi->mbi`。`mbi` 是 `&cbi->mbi`，指向 `kinfo` 里的字段。拷贝之后，引导器那块内存再也不被引用。
- 五个字段的初始化分两类。`mem_high_phys = 0` 是"待累计"的初值（`add_memmap` 会把它推到最高可用地址）。`user_sp`、`vir_kern_start`、`bootstrap_start/len` 全部来自**链接期常量**：`_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`——三个符号，来源分两路。`minix3/minix/kernel/arch/i386/kernel.lds` 直接定义 `_kern_vir_base`/`_kern_phys_base`，并给出 `__k_unpaged__kern_unpaged_start/end` 这两个别名；另一路是构建期的符号改名：`minix3/minix/kernel/arch/i386/Makefile.inc` 用一个 `.for unpaged_obj in head.o pre_init.o … pg_utils.o …` 循环对名单里的目标文件执行 `objcopy --prefix-symbols=__k_unpaged_`，把它们定义的符号整体改名，链接脚本再用 `unpaged_*.o(.bss COMMON)` 这类输入模式把这些对象单独归段。`vir_bytes` 类型的 `&符号` 取的是符号的地址值。后面 `user_sp` 会被 `USR_STACKTOP` 覆盖（同一函数后段），这里先赋 `_kern_vir_base` 只是初值。
- `bootstrap_len` 是"两个符号地址相减"，得到一小段内核镜像内的区域长度。这段区域叫 `unpaged`，含义见 §3.5。
- 命令行解析是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 直到 `=` 或空格、遇 `=` 后收集 `value` 直到空格。`if (*p++ != '=') continue;` 那一行处理"不是 `key=value` 形式的词"——注意 `p++` 已经把游标推过一个字符，所以 `continue` 之后从下一个字符继续扫，不会死循环。
- 每个 `key=value` 都过一遍 `mb_set_param`。然后（命令行解析之后）**无条件**再写两次：`arch=` 与 `board=`，值来自编译期的板卡身份（i386 上固定查 `BOARD_ID_INTEL`）。这两行意味着：即使引导器什么都没传，内核也保证自己知道"我是 x86 架构的 Intel 板"。

`minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param` 是这串文本的落地函数，它同时做三件事：识别少数"有专用字段的键"、维护"键不重复"、维护 `param_buf` 的可追加格式。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param。
 */
static int mb_set_param(char *bigbuf, char *name, char *value, kinfo_t *cbi) 
{
	char *p = bigbuf;
	char *bufend = bigbuf + MULTIBOOT_PARAM_BUF_SIZE;
	char *q;
	int namelen = strlen(name);
	int valuelen = strlen(value);

	/* Some variables we recognize */
	if(!strcmp(name, SERVARNAME)) { cbi->do_serial_debug = 1; }
	if(!strcmp(name, SERBAUDVARNAME)) { cbi->serial_debug_baud = atoi(value); }

	/* Delete the item if already exists */
	while (*p) {
		if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') {
			q = p;
			while (*q) q++;
			for (q++; q < bufend; q++, p++)
				*p = *q;
			break;
		}
		while (*p++)
			;
		p++;
	}
	
	for (p = bigbuf; p < bufend && (*p || *(p + 1)); p++)
		;
	if (p > bigbuf) p++;
	
	/* Make sure there's enough space for the new parameter */
	if (p + namelen + valuelen + 3 > bufend)
		return -1;
	
	strcpy(p, name);
	p[namelen] = '=';
	strcpy(p + namelen + 1, value);
	p[namelen + valuelen + 1] = 0;
	p[namelen + valuelen + 2] = 0;
	return 0;
}
```

逐段解码：

- 头两行 `strcmp` 是**唯一的两个**"有专用字段的键"：`SERVARNAME` 与 `SERBAUDVARNAME`（定义在 `minix3/minix/include/minix/const.h`，值为 `"cttyline"` 与 `"cttybaud"`）。命中时它们不是写进 `param_buf` 就完事，而是同时设置 `cbi->do_serial_debug` / `cbi->serial_debug_baud` 这两个结构体字段——因为"从哪个串口做内核调试输出"是内核在启动早期自己要做决定的事，不能等到用户态去读参数。
- "已存在先删"这一段是本函数的第一个不变量：同名键最多一份。它找到 `name` 后紧跟 `=` 的位置，把该条字符串的末尾（`q` 走到 NUL）之后的内容整段左移，覆盖掉旧条目。
- 找末尾的循环 `for (p = bigbuf; p < bufend && (*p || *(p + 1)); p++)`：它一直走到"当前字节与下一个字节都是 NUL"的位置。也就是说 `param_buf` 不是一条字符串，而是**一串以 NUL 分隔的 `key=value`，整串以一个额外的 NUL 收尾**（末尾效果是两个连续 NUL）。随后 `if (p > bigbuf) p++;` 把写入点挪到那个终止 NUL 之后。空缓冲时 `p == bigbuf`，不跳过，从头开始写。
- 容量检查用 `p + namelen + valuelen + 3 > bufend` 提前拒绝，返回值 `-1`——但调用点全部忽略返回值（`get_parameters` 里三处调用都没有检查：命令行解析循环里一处、固定键 `arch`/`board` 各一处）。这意味着"参数缓冲装不下"是静默丢弃，而不是启动失败。
- 最后四行写入：`name` + `=` + `value` + NUL（`strcpy` 自己带的）+ 再补一个 NUL。两个显式 NUL 写下的是新的终止符。

于是 `rootdevname=<设备名>` 的完整落链是：

```
boot.cfg.default: rootdevname=$rootdevname
   → 引导器把它拼进 multiboot 命令行
   → mbi->mi_cmdline（一整串，含 bootopts=… 等其它词）
   → get_parameters() 逐词扫描，拆出 name="rootdevname" value=<设备名>
   → mb_set_param(kinfo.param_buf, name, value)   ← 这一步是"被认出来"的位置
        · 不是专用字段（只有 cttyline/cttybaud 是），所以只写进 param_buf
        · 以 "rootdevname=<设备名>\0" 的形式追加，并保证同名键只有一份
   → 读它的人：内核侧读同一张表的入口是 minix3/minix/kernel/main.c:get_value
                 （例：kmain 开头用 env_get(BOARDVARNAME) 取板卡名）
     用户态：PM 的 PMGETPARAM 路径（minix3/minix/include/sys/svrctl.h:PMGETPARAM）
                 → 系统任务 GET_MONPARAMS（minix3/minix/kernel/system/do_getinfo.c）
                   把 kinfo.param_buf 整块拷给调用者
                 → 例：mib 服务的 minix3/minix/servers/mib/kern.c:mib_kern_root_device
                   取出 rootdevname 名字
     注意：内核自身没有 rootdevname 的读者（`grep -rn rootdevname minix3/minix/kernel/`
     无命中）——它是留给用户态的名字
```

所以"`rootdevname` 为什么要经 `mb_set_param` 才被认出来"的答案是：命令行在 `mbi` 里只是一串字节，内核里没有任何一处按字符串搜索引导器内存；`param_buf` 才是内核自己的参数表，而写入它、保证键唯一、维护分隔格式的唯一入口就是 `mb_set_param`。不经过它，`rootdevname` 就只是一串留在 `mbi.mi_cmdline` 里、之后没人再读的字符。

**如果反过来想**：删掉"已存在先删"那段循环。`get_parameters` 在解析完命令行之后无条件再写一次 `arch=` 与 `board=`，于是当命令行里已经带有 `arch=` 或 `board=` 时，`param_buf` 里会出现同一键的两条记录。读取方 `get_value()`（`minix3/minix/kernel/main.c`）是从头扫描、**返回第一个匹配**——于是被返回的是命令行里那一份，而不是内核自己按编译期板卡身份写的那一份。具体后果落在 `minix3/minix/kernel/main.c:kmain` 开头那句 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`：板卡身份会被一条外部输入改写，而改写者是引导配置里多写的一个词。

### 2.4 两份内存记录：add_memmap 与 cut_memmap

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 定义在 `minix3/minix/include/minix/param.h` 为 40。数组里每一项是"一段可用区间"，`mmap_size` 记录有效项数。两项操作：追加（`add_memmap`）与切除（`cut_memmap`）。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap。
 */
void add_memmap(kinfo_t *cbi, u64_t addr, u64_t len)
{
        int m;
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

	assert(kernel_may_alloc);

        for(m = 0; m < MAXMEMMAP; m++) {
		phys_bytes highmark;
                if(cbi->memmap[m].mm_length) continue;
                cbi->memmap[m].mm_base_addr = addr;
                cbi->memmap[m].mm_length = len;
                cbi->memmap[m].mm_type = MULTIBOOT_MEMORY_AVAILABLE;
                if(m >= cbi->mmap_size)
                        cbi->mmap_size = m+1;
		highmark = addr + len;
		if(highmark > cbi->mem_high_phys) {
			cbi->mem_high_phys = highmark;
		}

                return;
        }

        panic("no available memmap slot");
}
```

逐条解码：

- `LIMIT 0xFFFFF000` 与两次判断：`addr > LIMIT` 直接丢弃整条；区间跨越 `LIMIT` 时把长度截到边界。两条判断的次序有实际意义——先判 `addr` 把整条在 4GB 之上的描述符整条丢掉（不改 `len`），再判跨界。`0xFFFFF000` 而不是 `0x100000000` 是因为地址要能安放进 32 位的 `phys_bytes`（`memmap[]` 的字段类型），并且低 12 位为零。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）加一条提前返回：`if(len == 0) return;`——长度为零的区间被**静默丢弃**，不是断言。`kernel_may_alloc` 是全局开关，定义在 `minix3/minix/kernel/arch/i386/pre_init.c`（声明在 `minix3/minix/kernel/glo.h`），它把"引导期允许改账"与"引导期结束后禁止改账"分开。
- `roundup(addr) / rounddown(len)`：**起点向大、长度向小**。这是使"返回的区间整体落在原区间之内"的对齐方向；反过来（起点向小、长度向大）会向外扩出一个不存在的页。
- 追加位置是"第一个 `mm_length == 0` 的槽"（`if(cbi->memmap[m].mm_length) continue;`）。空槽用 `mm_length == 0` 表示，而不是另设一个占用位；因此"长度为零的条目"在这个数据结构里不存在。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：因为槽位是复用第一个空洞，`mmap_size` 更新为 `m+1` 而不是简单自增——它代表"最后一个有效项的下标+1"，中间不允许有空洞。
- `mem_high_phys` 只增不减：它是所有条目的最高地址上界（`addr + len`）。`pg_identity()` 依赖它（见 2.6）。

切除是这里的核心操作，因为它要把一段区间从记录里去掉，而记录里的一段区间可能被切成两段（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`；追加那一半在 `minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`）。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap。
 */
void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)
{
        int m;
        phys_bytes o;

        if((o=start % I386_PAGE_SIZE))
                start -= o;
        if((o=end % I386_PAGE_SIZE))
                end += I386_PAGE_SIZE - o;

        assert(kernel_may_alloc);

        for(m = 0; m < cbi->mmap_size; m++) {
                phys_bytes substart = start, subend = end;
                phys_bytes memaddr = cbi->memmap[m].mm_base_addr,
                        memend = cbi->memmap[m].mm_base_addr + cbi->memmap[m].mm_length;

                /* adjust cut range to be a subset of the free memory */
                if(substart < memaddr) substart = memaddr;
                if(subend > memend) subend = memend;
                if(substart >= subend) continue;

                /* if there is any overlap, forget this one and add
                 * 1-2 subranges back
                 */
                cbi->memmap[m].mm_base_addr = cbi->memmap[m].mm_length = 0;
                if(substart > memaddr)
                        add_memmap(cbi, memaddr, substart-memaddr);
                if(subend < memend)
                        add_memmap(cbi, subend, memend-subend);
        }
}
```

逐条解码：

- 方向：`start` 向**下**取整到页，`end` 向**上**取整到页。这与 `add_memmap` 的方向相反，因为两处要的保证不同：`add_memmap` 要"不把不属于我的内存写进账"，`cut_memmap` 要"不把用过的页留在账上"。
- `substart/subend` 的两次修正把切除区间夹进当前条目的范围内。这一步让"一条切除跨几个条目"变成若干次互不干扰的子切除。
- `if(substart >= subend) continue;`：本条目与切除区间无重叠，跳过。注意这里比较的是夹取后的值，所以"切除区间完全在本条目之外"与"恰好相切"是同一个分支。
- 命中时先把自己清零（`mm_base_addr = mm_length = 0;`），再用 `add_memmap` 把**左余量**与**右余量**补回去。补回的顺序是从低到高，但由于 `add_memmap` 找的是"第一个空槽"，两条补回会依次占用刚清空的那个槽与它后面的第一个空槽。
- 只有左余量时补一段、只有右余量时补一段、两端都有时补两段。切除区间把整条覆盖时一段都不补。

手算一次。设记录里只有一条 `[0x100000, 0x1000000)`（1MB 起、16MB，`I386_PAGE_SIZE = 4096`），现在要切掉 `[0x400123, 0x401F00)`：一段模块的字节范围，两端都不对齐页。

```
起点向下取整：0x400123 % 0x1000 = 0x123  → start = 0x400000
终点向上取整：0x401F00 % 0x1000 = 0xF00  → end   = 0x401F00 + 0x100 = 0x402000

本条目 [0x100000, 0x1000000) 与 [0x400000, 0x402000) 相交：
  substart = max(0x400000, 0x100000) = 0x400000
  subend   = min(0x402000, 0x1000000) = 0x402000

清空该条目，然后补两段：
  substart(0x400000) > memaddr(0x100000)  → add_memmap(0x100000, 0x300000)  ← [0x100000, 0x400000)
  subend(0x402000)   < memend(0x1000000)  → add_memmap(0x402000, 0xBFE000)  ← [0x402000, 0x1000000)
```

口径：两个 add 的长度由 `substart-memaddr = 0x300000` 与 `memend-subend = 0xBFE000` 直接相减得到；两段长度之和 `0x300000 + 0xBFE000 = 0xEFE000`，加上被切掉的 `0x2000` 等于原来的 `0xF00000`，与手算一致。

除这两个函数外，`pg_utils.c` 还有两个从内存记录里取地址的函数（第三个 `alloc_pagetable` 在 §2.6），一个从最低处取、一个从最高处取（`minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest`、`:pg_alloc_page`）：

```c
/*
 * 节选（保留与分配语义直接相关的行；省略号处为未改动的循环变量声明与方法体其余行），
 * minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest 与 pg_alloc_page。
 */
phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len)
{
	int m;
#define EMPTY 0xffffffff
	phys_bytes lowest = EMPTY;
	assert(len > 0);
	len = roundup(len, I386_PAGE_SIZE);

	assert(kernel_may_alloc);

	for(m = 0; m < cbi->mmap_size; m++) {
		if(cbi->memmap[m].mm_length < len) continue;
		if(cbi->memmap[m].mm_base_addr < lowest) lowest = cbi->memmap[m].mm_base_addr;
	}
	assert(lowest != EMPTY);
	cut_memmap(cbi, lowest, len);
	cbi->kernel_allocated_bytes_dynamic += len;
	return lowest;
}

phys_bytes pg_alloc_page(kinfo_t *cbi)
{
	...
	for(m = cbi->mmap_size-1; m >= 0; m--) {
		mmap = &cbi->memmap[m];
		if(!mmap->mm_length) continue;
		assert(mmap->mm_length > 0);
		assert(!(mmap->mm_length % I386_PAGE_SIZE));
		assert(!(mmap->mm_base_addr % I386_PAGE_SIZE));

		mmap->mm_length -= I386_PAGE_SIZE;

                cbi->kernel_allocated_bytes_dynamic += I386_PAGE_SIZE;

		return mmap->mm_base_addr + mmap->mm_length;
	}

	panic("can't find free memory");
}
```

`pg_alloc_page` 从**最高**的条目开始取（`for(m = cbi->mmap_size-1; ...)`），取该条目的最后一页（先减长度，再返回"基址+新长度"），并把用量记进 `kernel_allocated_bytes_dynamic`。它取页前有三条断言，其中 `assert(!(mmap->mm_length % I386_PAGE_SIZE))` 是"记录里所有长度都必须是页的整数倍"这一不变量的唯一执行点。

`alloc_lowest` 则**不是**一条可行的"从低处分配"路径，问题出在切除那一行：它写的是 `cut_memmap(cbi, lowest, len)`，而 `cut_memmap` 的第二个参数是结束地址。于是它切掉的区间是 `[lowest, len)`，而"想切掉的区间"是 `[lowest, lowest+len)`。两者只在 `lowest == 0` 时重合。写这个调用的人是把它当"起始地址 + 长度"的语义用的。

它的唯一调用点在 SMP 路径：`minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline` 里 `trampoline_base = alloc_lowest(&kinfo, tramp_size);`，用途是把一小段 16 位启动代码拷到 1MB 以下给辅助处理器执行。紧接着的校验只查了返回值的上界：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline。
 */
	tramp_size = (unsigned) &__trampoline_end - tramp_start;
	trampoline_base = alloc_lowest(&kinfo, tramp_size);

	/* The memory allocator finds the lowest available memory.. 
	 * Verify it's low enough
	 */
	assert(trampoline_base + tramp_size < (1 << 20));
```

注意 `assert(trampoline_base + tramp_size < (1 << 20))` 检查的是"地址足够低"，而不是"这段内存已经从账上切掉"。所以这条校验通不过时能发现"分配得不低"，却检查不出"分配了但账没改"。

**如果反过来想**（两处，都要落到具体对象）：

其一，若 `cut_memmap` 不把 `start` 向下、`end` 向上取整。补回余量时走的仍是 `add_memmap`，它自带 `roundup(addr)`/`rounddown(len)`，所以**记录里不会出现非页对齐长度**——这一条不变量由 `add_memmap` 独立保证，与 `cut_memmap` 的取整无关。取整真正决定的是另一种情况：一次**零长度**切除（`start == end`，例如某条模块描述的起止地址相同）在带取整时会切掉该地址所在的一整页，而在不带取整时夹取后 `substart >= subend`，一次循环都不命中，等于什么都没切。也就是说，取整保证的是"切除按页保守、不留下半个页"，而不是"防止记录被写坏"。

两个取址方向各有其消费者。`pg_alloc_page` 从最高条目取页，让记录里最低的那一段保持连续；唯一要求低地址的消费者是辅助处理器的启动跳板——`minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline` 经 `alloc_lowest` 取它，并断言结果落在 1MB 以下——而 `alloc_lowest` 是从最低处扫的。两个方向互不消耗对方要的地址。

其二，退回 `alloc_lowest` 的那一行：若 `lowest` 不为 0（例如引导器的内存表里最低的可用区间不是从 0 开始），`cut_memmap(cbi, lowest, len)` 变成用 `len`（一个长度）当结束地址，夹取之后 `substart >= subend`，每一次循环都 `continue`，函数**什么都不切**就返回。返回的地址会被 `copy_trampoline` 拷进 16 位启动代码；此后任意一次 `pg_alloc_page` 都可以把这段物理内存当成空闲页发出去（这段内存从头到尾没被标记为占用），辅助处理器启动到一半时它的启动代码被改成页表内容——症状是某个 AP 起不来或执行到随机地址，而不是"内存分配失败"。这也解释了为什么"看到'分配'两个字，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址"

#### 2.4.1 模块内存与 unpaged 段的归属变更

`cut_memmap` 在 `pre_init` 里被调用时只是一次**临时**切除。启动过程中有三处把物理区间从可用记录里切掉或加回来的动作；它们同属"改同一份记录"这一族，但作用对象并不相同——前两处是**模块字节**，第三处是内核自己的 **unpaged 段**：

| 次 | 对象 | 时刻 | 谁做 | 动作 | 为什么 |
|---|---|---|---|---|---|
| 1 | 模块字节 | `pre_init` → `get_parameters` | `cut_memmap`（本文 2.4） | 从可用记录里切掉 `[mod_start, mod_end)` | 这段内存里躺着模块的字节，页表页与内核数据不能分配到这里 |
| 2 | 模块字节 | 内核装载启动进程时（`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc`） | `add_memmap` + 清空模块描述符 | 把模块的字节段复制进目标地址空间后，把这段物理区间加回记录，并把 `mod_start`/`mod_end` 置 0 | 字节已经搬到别处，原地址可以复用；置 0 是"已回收"的标记 |
| 3 | unpaged 段 | `kmain` 之后 | `add_memmap(bootstrap_start, bootstrap_len)` | 归还分页开启前使用的那一小段 | C 的 `unpaged` 段只在启动期需要（本文 §3.5），与模块无关 |

口径：**模块字节严格只有前两次**（一次切、一次还），第 3 次的对象是内核镜像里的 unpaged 段（链接脚本导出的 `_kern_unpaged_start/end`）。而且第 2 次只归还 VM 那一块——其余模块的字节区间在 C 里从头到尾都没有加回过（见下方第三点）。

第 2 次的源码（本篇 §3.7 引用的扣除表把这条 C 对应关系**改述**过一行，不是逐字引用）：

```c
/*
 * 逐字引用（保留与该动作直接相关的行），minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc 中 VM 模块装载完成处。
 */
		arch_proc_init(rp, execi.pc, (vir_bytes)sp,
			execi.stack_high - sizeof(struct ps_strings),
			ip->proc_name);

		/* Free VM blob that was just copied into existence. */
		add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
		mod->mod_end = mod->mod_start = 0;

		/* Remember them */
		kinfo.vm_allocated_bytes = alloc_for_vm;
```

三处具体事实：

- `add_memmap` 的实参是 `mod->mod_start` 与 `mod->mod_end - mod->mod_start`（起点与**长度**），与 2.4 里 `cut_memmap(cbi, lowest, len)` 那个把长度当结束地址的写法形成对照：同一个文件里的两组调用对"第二个参数是什么"并不一致，读的时候只能逐个核对被调函数的签名。
- 置零发生在加回之后，且两个字段一起置 0。此后任何遍历 `module_list[]` 的代码都会看到一条"空模块"；这是"已回收"在数据结构里的表示（而不是另设一个布尔位）。
- 这次回收只针对 VM 的模块（`vm_allocated_bytes` 紧跟着被记录），因为它的字节已经被复制进 VM 的地址空间。其余模块的回收时机由它们各自的装载决定。

**第 2 次"不归还"的后果，落到具体对象**：被切掉的那段区间会永久停在"已切"形态——`kinfo.memmap[]` 里对应条目的 `mm_length` 保持 0（`cut_memmap` 清空后的样子），因此它既不会被任何遍历看到，也不会回到可用总量里。启动期的物理页分配器只有一个：`pg_alloc_page` 从最高条目往下扫，取第一个 `mm_length != 0` 的条目的最后一页，扫完一圈都没有可用条目时 `panic("can't find free memory")`（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`）。所以"不归还"的显形形式是这个 panic，而它最先被谁碰到取决于谁先要新页：启动期要新页的入口是装载启动镜像的那条路径——`minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc` 调 `pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo)`，后者对每个虚拟页调一次 `pg_alloc_page`（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_map`）。需要说清的一点是：C 里**本来就只归还 VM 那一块**，其余 11 个模块的区间从不归还，启动照样完成——可见"不归还"的代价不是立刻坏，而是可用总量永久少掉那几段；它要显形，得等其余可用区间也被用尽，此时报错的是 `pg_alloc_page` 的 `can't find free memory`，而调用栈的顶端就是那一次启动镜像装载。

第 2 次在新设计里的处理与这里不同（落点与理由见 §3.7 的扣除表）；第 3 次不存在，因为 `bootstrap_len` 恒为 0（§3.5）。

### 2.5 overlaps 与 kern_mod：内核把自己当成第 N+1 个模块

内存账建好后，要从中切掉两块东西：模块占用的区间与内核自己占用的区间。切之前先检查它们彼此是否重叠——引导器不保证它放的文件互不重叠，而"模块 A 的字节覆盖了模块 B 的开头"这种布局一旦进入后续步骤，症状会非常难追。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pre_init.c:overlaps。
 */
int overlaps(multiboot_module_t *mod, int n, int cmp_mod)
{
	multiboot_module_t *cmp = &mod[cmp_mod];
	int m;

#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
			INRANGE(mod1, mod2->mod_end-1))
	for(m = 0; m < n; m++) {
		multiboot_module_t *thismod = &mod[m];
		if(m == cmp_mod) continue;
		if(OVERLAP(thismod, cmp))
			return 1;
	}
	return 0;
}
```

逐条解码：

- 区间语义是半开区间 `[start, end)`：`INRANGE` 写成 `v >= start && v < end`。这正是 `mod_end` 表示"最后一个字节的下一个地址"（"past the end"）的写法。
- `OVERLAP` 判两个点：对方区间的**起点**与对方的**最后字节**。第二项写成 `mod2->mod_end - 1`，就是把半开区间翻译回"最后一个有效字节"。
- `overlaps` 检查 `mod[cmp_mod]` 与其余每一条，只要有一条相交就返回 1。注意它是对每条记录各调用一次（外层循环在 `get_parameters` 里），所以一对重叠会被两个方向各检查一次；这使第二项 `-1` 的必要性变得具体（见下）。

内核自己不带 `multiboot_module_t`，但检查与切除都要把内核算进去（检查那一半是 `minix3/minix/kernel/arch/i386/pre_init.c:overlaps`），于是 `minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 把它塞进模块数组的第一个空槽：

```c
/*
 * 节选（保留与"内核当模块"直接相关的行），minix3/minix/kernel/arch/i386/pre_init.c:get_parameters 后段。
 */
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

三处具体事实：

- `kernbase`/`kernsize` 来自链接期常量 `&_kern_phys_base` 与 `&_kern_size`，不是引导器给的。
- `assert(k < MULTIBOOT_MAX_MODS)` 是"内核还要占一个槽"这一事实的形式化：模块数最多只能是 `MULTIBOOT_MAX_MODS - 1`，因为有 1 个槽要留给内核。它与 2.1 里那条"模块数 < `MULTIBOOT_MAX_MODS`"是同一个条件（`k = mbi->mi_mods_count`），作用是在往 `module_list[k]` 写"内核当模块"这一项之前，把"数组还有空槽"这个前提再确认一次。
- 同一个循环里既检查又切除：`mods_with_kernel` 条记录，每条先与其余所有记录比一次，然后从账上切除。切除用的是 `[mod_start, mod_end)`，与 `cut_memmap` 的起止语义一致。

于是"内核与模块占用区"这一整类内存只被一段代码处理，模块与内核之间没有第二套逻辑。

**如果反过来想**：把 `OVERLAP` 第二项改为 `INRANGE(mod1, mod2->mod_end)`（即不写 `-1`）。取两个首尾相接的模块：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`。它们不重叠——A 的最后一个字节是 `0x1FFFFF`，B 的第一个字节是 `0x200000`。用不带 `-1` 的写法检查：

```
外层 m = 0（A 是 cmp），内层遍历到 B：OVERLAP(B, A) = INRANGE(B, A.mod_end = 0x200000)
   = (0x200000 >= 0x200000) && (0x200000 < 0x300000) = 真
```

于是 `panic("overlapping boot modules/kernel")` 落在一个完全合法的布局上。这正是"半开区间必须翻译对"这一类错误的代价：报错信息说"模块重叠了"，而真实情况是模块之间恰好相切。

### 2.6 页表建立步骤与被本篇接手的那一半

`pre_init()` 的函数体共八条语句：一条断言 + 一条收集调用 + 五步建表/开启 + 一条 `return`；下面说的"前一条收集、后五条建表与开启"指中间那六条。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pre_init.c:pre_init。
 */
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

这段注释本身就是对本篇 1.2 那次序列表述的原文支持：`but first a 1:1 mapping so this code stays where it should be`。五步各自的职能：

| 步 | 函数 | 改的是什么 | 输入 |
|---|---|---|---|
| 1 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear` | 静态页目录数组 `pagedir[1024]` 清零 | 无 |
| 2 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity` | 把低地址全部映射成恒等（VA = PA） | `kinfo.mem_high_phys` |
| 3 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel` | 把内核镜像映射到它的高虚拟基址，返回第一个空闲页目录下标 | 链接期常量 + `pagedir[]` |
| 4 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_load` | 把页目录物理地址写进 `CR3` | 页目录地址 |
| 5 | `minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging` | 置 `CR0.PG`（以及 `PSE`/`WP`/`PGE`） | `CR0`/`CR4` 当前值 |

步骤 2 与步骤 3 写同一张数组，这是它们必须有序的机制来源。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity。
 */
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

逐条解码：

- `I386_VM_DIR_ENTRIES = 1024`、`I386_BIG_PAGE_SIZE = I386_PAGE_SIZE * I386_VM_PT_ENTRIES = 4096 * 1024`（4MB，定义在 `minix3/minix/include/arch/i386/include/vm.h`）。循环走 1024 次，每次写一个覆盖 4MB 的页目录项，覆盖整 4GB。
- 每个条目都置 `I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_USER | I386_VM_WRITE`：存在、是大页（该条目直接指向 4MB 物理块而不是二级页表）、用户可访问、可写。
- 条件里比较的是"`mem_high_phys` 向上取整到 4MB"与"本条目起始地址"：`（mem_high_phys & 0xFFC00000) <= (phys & 0xFFC00000)`。当本条目已经越过物理内存上界时，额外加 `I386_VM_PWT | I386_VM_PCD`。源码注释给了理由："We map memory that does not correspond to physical memory as non-cacheable."
- `PWT`/`PCD`（页写穿透 / 页禁缓存）的用途：那些地址后面不是 DRAM，而是设备寄存器或空洞。设备读写的语义依赖"每次访问都真的到达设备"；若走缓存，读可能命中缓存里没有被设备更新过的旧值，写可能在写缓冲区里停留。所以"超出物理内存"这个条件恰好圈出这一类地址。
- 这个函数把 4GB 全部置为 PRESENT。所以"有没有映射"在这里不构成区分，区分"真实内存"与"设备/空洞"的只有缓存属性两位。

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel。
 */
int pg_mapkernel(void)
{
	int pde;
	u32_t mapped = 0, kern_phys = kern_phys_start;

        assert(!(kern_vir_start % I386_BIG_PAGE_SIZE));
        assert(!(kern_phys % I386_BIG_PAGE_SIZE));
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
	        pagedir[pde] = kern_phys | I386_VM_PRESENT | 
			I386_VM_BIGPAGE | I386_VM_WRITE;
		mapped += I386_BIG_PAGE_SIZE;
		kern_phys += I386_BIG_PAGE_SIZE;
		pde++;
	}
	return pde;	/* free pde */
}
```

逐条解码：

- `kern_vir_start`/`kern_phys_start`/`kern_kernlen` 是文件顶部的静态量，初值来自链接期符号 `_kern_vir_base`/`_kern_phys_base`/`_kern_size`。
- 两条断言要求两个基址都按 4MB 对齐。索引从 `kern_vir_start / I386_BIG_PAGE_SIZE` 开始：`_kern_vir_base = 0xF0400000`（`minix3/minix/kernel/arch/i386/kernel.lds`），除以 4MB 得下标 `961`。
- 循环把内核镜像按 4MB 一块地映射进高地址区。条目里**没有** `I386_VM_USER`——内核地址对用户态不可访问，这是与恒等映射故意相反的一处。
- 返回值是循环结束后的 `pde`，也就是内核映射占完之后第一个未被占用的页目录下标。注释写作 `free pde`。它的下游见本节末尾。

第三个函数是唯一改硬件状态的那个：

```c
/*
 * 节选（保留与 CR0/CR4 次序直接相关的行；省略号处为 PAE 能力检查与注释），
 * minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging。
 */
void vm_enable_paging(void)
{
        u32_t cr0, cr4;
        int pgeok;

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
}
```

逐条解码：

- 入口先读一次 `CR0`/`CR4`（把"当前值"读回来再按位改），断言保护模式位 `PE` 已置（引导器的责任）。
- **先清，再置**，而且清理的位比后面重设的位多一个：清 `PG`、清 `PGE|PSE`；随后只重设 `PSE`，再开 `PG`，最后按 CPU 能力置 `PGE`。
- 注释给的两条理由分别是："PGE 必须在 PG 之后启用"与"Our page table contains 4MB entries"（PSE 必须为 1，4MB 大页才成立）。
- `WP`（写保护）在开 `PG` 之后单独置一次。它的效果是"内核态也不能写只读页"，是 W^X 的基础。
- `pgeok` 来自 CPU 能力检测（`_cpufeature(_CPUF_I386_PGE)`）。有能力的 CPU 才置 `PGE`：该位为 1 时，带 G 位的页表项在切换 `CR3` 后仍留在 TLB 里。

页表本身也要占物理页。C 里这部分由两个函数承担（`minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable` 与 `:pg_alloc_page`；4KB 细粒度映射的入口是 `minix3/minix/kernel/arch/i386/pg_utils.c:pg_map`）：

```c
/*
 * 节选（保留分配策略相关的行），minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable 与 pg_map 的页表挂载段。
 */
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

- 6 个页表页是**编译期固定的静态池**：`static u32_t pagetables[6][1024] __aligned(4096)`。用完（`pt_inuse >= 6`）直接 `panic("no more pagetables")`，没有扩容路径。
- 页表页的物理地址由 `vir2phys(ret)` 得到，而 `pre_init.c` 里 `vir2phys` 的定义是 `return (phys_bytes) addr;`——引导期恒等映射成立，所以"虚拟地址即物理地址"这句话在这个函数里是可执行的。

页表页的另一个来源是 `pg_alloc_page`（2.4 已贴）。`pg_map()` 在 4KB 粒度映射时挂页表：`pagedir[pde] = (ph & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`，随后 `pt[pte] = (source & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`。这两行是"两级页表"的实现：页目录项指向页表页，页表项指向物理页，两级都带 USER，因为 4KB 细粒度映射的用途之一是给用户地址空间做准备。

最后是 `pg_mapkernel()` 那个返回值 `freepde_start` 的下游（`minix3/minix/kernel/arch/i386/memory.c:memory_init` 与 `:createpde`）。它的语义是"内核映射占完之后，页目录里第一个空闲的槽位号"，而它的消费者要的是"两个可以借用的窗口"：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/memory.c:memory_init。
 */
void memory_init(void)
{
	assert(nfreepdes == 0);

	freepdes[nfreepdes++] = kinfo.freepde_start++;
	freepdes[nfreepdes++] = kinfo.freepde_start++;

	assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
	assert(nfreepdes == 2);
	assert(nfreepdes <= MAXFREEPDES);
}
```

```c
/*
 * 节选（保留"把对方页表的条目写进当前页表"这一段；省略号处为入口参数检查与返回地址计算），
 * minix3/minix/kernel/arch/i386/memory.c:createpde。
 */
static phys_bytes createpde(
	const struct proc *pr,	/* Requested process, NULL for physical. */
	const phys_bytes linaddr,/* Address after segment translation. */
	phys_bytes *bytes,	/* Size of chunk, function may truncate it. */
	int free_pde_idx,	/* index of the free slot to use */
	int *changed		/* If mapping is made, this is set to 1. */
	)
{
	u32_t pdeval;
	...
	/* Write the pde value that we need into a pde that the kernel
	 * can access, into the currently loaded page table so it becomes
	 * visible.
	 */
	assert(get_cpulocal_var(ptproc)->p_seg.p_cr3_v);
	if(get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] != pdeval) {
		get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] = pdeval;
		*changed = 1;
	}
	...
}
```

链路是：内核要访问"另一个地址空间里的地址"（例如跨进程拷贝），而它只有一个当前生效的页目录；办法是在**当前页表**里临时占用一个空闲槽位，把对方那个地址对应的页目录项写进去，于是对方的一个 4MB 窗口在当前地址空间里可见；用完由调用方在下次切换时自然丢弃。`virtual_copy` 里两个方向各借一个槽位（`free_pde_idx` 为 0 与 1），写完调 `reload_cr3()` 刷新 TLB。另一个消费者在 VM 服务里（`minix3/minix/servers/vm/pagetable.c` 里对 `kernel_boot_info.freepde_start` 的取用），用途相同。

这就把 `freepde_start` 从"一个返回值"变成了"一套借用机制"：它成立需要三个前提——当前页表里确实有连续的未用槽位、借来的窗口只在当前 `CR3` 下有效、借出者自己记账并保证归还。新设计为什么不再需要它，见 §3.8。

**偏移一位的后果**：`pg_mapkernel` 的返回值如果是"最后一个已用槽位"而不是"第一个空闲槽位"，`memory_init` 的两行自增会把 `freepdes[0]` 设成内核映射占用的最后一个页目录下标（961 + k - 1），`freepdes[1]` 才落到真正的第一个空闲槽。`createpde` 对借来的槽只做两条范围检查（`free_pde_idx` 在 `nfreepdes` 之内、槽号在 1024 之内），不检查它"是否空闲"；而跨空间拷贝的第一个方向固定借 0 号槽（`createpde(srcproc, srclinaddr, &chunk, 0, &changed)` 在 `minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy` 里，入口是 `minix3/minix/kernel/arch/i386/memory.c:virtual_copy_f`），`vm_memset` 也借 0 号槽（`minix3/minix/kernel/arch/i386/memory.c:vm_memset`）。于是第一次跨地址空间拷贝会把对方地址空间的 PDE 值写进 `p_cr3_v[961 + k - 1]`——那一条正是内核镜像最后一块 4MB 的映射；`reload_cr3()` 让这次改写立刻生效，内核在那个 4MB 窗口里的下一次访问就撞上它：对方该槽的条目若不存在（值为 0 或非 present），是一次内核态页错误；若存在，则是静默读到另一个进程的 4MB 内容（内核数据被换成了别人的字节，没有任何报错）。两条路径都由启动进程发起的系统调用进入，所以最早坏在 `memory_init` 之后第一次跨空间拷贝或清零上。

**如果反过来想**（两处）：

其一，若把 `vm_enable_paging` 的"先清后置"改成"先开 `PG`，再改 `CR4`"。页目录里 1024 个条目都带 `I386_VM_BIGPAGE`（bit 7，`0x080`）。32 位分页在 `PSE = 0` 时把页目录项的 bit 7 当作**保留位**——置位即为非法。于是 `cr0 |= I386_CR0_PG; write_cr0(cr0);` 之后的下一条取指：CPU 走页目录、看到的是一个保留位置位的条目，触发页错误，错误码里 P 位为 1（错误发生在"存在但非法"的条目上）。故障地址 `CR2` 正好是内核当前执行的虚拟地址，而这个时刻 `pg_load()` 已经写过 `CR3`、页表就是自己刚建的那张——现场能看到的只是一个"刚开分页就崩"的现象。

其二，若把 `pg_identity()` 排到 `pg_mapkernel()` 之后。两个函数写同一个 `pagedir[]`，索引集合**重叠**：`pg_identity` 写 0..1023 全部 1024 个条目，`pg_mapkernel` 写从 961 开始的若干条目（`_kern_vir_base = 0xF0400000` 除以 4MB）。先写内核映射、后写恒等映射，961 号及之后的若干条目会被"恒等映射 + 4MB 大页"覆盖：`pagedir[961]` 从 `0x00400000 | …`（`_kern_phys_base`）变成 `0xF0400000 | …`（961 × 4MB），两者相差 `_kern_offset = 0xF0000000`，**不相等**。切换到高地址执行时，内核镜像的第一条指令地址 `0xF0400000` 被 961 号条目解释成物理地址 `0xF0400000`——那是一片没有 DRAM 的物理块，取指立刻失败；被覆盖的范围是 961 到 `961 + kern_size/4MB`，即内核镜像自己占据的前几个 4MB 块。次序错在这里不会报"映射冲突"，只会让内核在高地址的第一条指令上取不到码。

#### 2.6.1 本篇用到的位、掩码与常量

下面两张表把本篇出现的位与掩码集中在一处，值全部取自 `minix3/minix/include/arch/i386/include/vm.h`。

| 常量 | 值 | 含义 | 本篇哪一步用到 |
|---|---|---|---|
| `I386_VM_PRESENT` | `0x001` | 条目存在 | `pg_identity`、`pg_mapkernel`、`pg_map` 写入的每个条目 |
| `I386_VM_WRITE` | `0x002` | 可写 | 同上 |
| `I386_VM_READ` | `0x000` | 只读（不置 `WRITE` 即只读） | — |
| `I386_VM_USER` | `0x004` | 用户态可访问 | `pg_identity`、`pg_map`（`pg_mapkernel` 不置，见 2.6.2） |
| `I386_VM_PWT` | `0x008` | 写穿透 | `pg_identity` 对非 DRAM 区间 |
| `I386_VM_PCD` | `0x010` | 禁缓存 | 同上 |
| `I386_VM_ACC` | `0x020` | 已访问（由 CPU 置位） | 硬件写，本路径不置 |
| `I386_VM_DIRTY` | `1 << 6` | 已写（由 CPU 置位） | 同上 |
| `I386_VM_BIGPAGE`（别名 `I386_VM_PS`） | `0x080` | 本条目直接指向 4MB 物理块 | `pg_identity`、`pg_mapkernel` |
| `I386_VM_GLOBAL` | `1 << 8` | 全局页（换 `CR3` 后留在 TLB） | 本路径不置 |
| `I386_VM_ADDR_MASK` | `0xFFFFF000` | 取 4KB 对齐的物理地址 | `pg_map` 挂页表项、`pg_info` 交出页目录地址 |
| `I386_VM_ADDR_MASK_4MB` | `0xFFC00000` | 取 4MB 对齐的物理地址 | `pg_identity` 的区间比较 |
| `I386_VM_OFFSET_MASK_4MB` | `0x003FFFFF` | 4MB 块内的偏移 | 4MB 窗口内寻址 |
| `I386_VM_PT_ENT_MASK` | `0x3FF` | 页表索引掩码 | `I386_VM_PTE` |
| `I386_VM_DIR_ENTRIES` / `I386_VM_PT_ENTRIES` | `1024` | 页目录/页表条目数 | `pg_identity` 的循环上界、页目录数组长度 |
| `I386_PAGE_SIZE` | `4096` | 页大小 | 全部对齐与取整运算 |
| `I386_BIG_PAGE_SIZE` | `4096 × 1024` = 4MB | 大页大小 | `pg_identity`/`pg_mapkernel` 的步进 |

三条地址换算宏（同文件）：

| 宏 | 展开 | 含义 |
|---|---|---|
| `I386_VM_PDE(v)` | `v >> 22` | 虚拟地址 → 页目录下标（高 10 位） |
| `I386_VM_PTE(v)` | `(v >> 12) & 0x3FF` | 虚拟地址 → 页表下标（中 10 位） |
| `I386_VM_PFA(e)` | `e & 0xFFFFF000` | 页目录/页表项 → 物理地址（低 12 位清零） |

CR0/CR4 的位（同文件）：

| 常量 | 值 | 位 | 含义 | 谁动它 |
|---|---|---|---|---|
| `I386_CR0_PE` | `0x00000001` | bit 0 | 保护模式 | 引导器置；`vm_enable_paging` 只断言它已置 |
| `I386_CR0_WP` | `0x00010000` | bit 16 | 写保护（内核态也不能写只读页） | `vm_enable_paging` |
| `I386_CR0_PG` | `0x80000000` | bit 31 | 开启分页 | `vm_enable_paging` |
| `I386_CR4_PSE` | `0x00000010` | bit 4 | 大页扩展（4MB 页的前提） | `vm_enable_paging` |
| `I386_CR4_PGE` | `0x00000080` | bit 7 | 全局页（按 CPU 能力置） | `vm_enable_paging` |

两张表里有一处注释与位号不符，读的时候按值与用法判断：`vm.h` 里 `I386_CR0_WP` 那行的注释写的是 `/* Enable paging */`，与下一行 `I386_CR0_PG` 的注释逐字相同——注释在这一行是从相邻行复制来的，`WP` 的含义要看 `vm_enable_paging` 里它被置的时机（开 `PG` 之后单独置一次）。

#### 2.6.2 自举页表的三个下游

**其一，页目录被交给 VM 的进程表项。** `pg_info`（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_info`）把页目录的物理地址与虚拟地址分别写进两个输出参数；调用点在 `minix3/minix/kernel/arch/i386/protect.c:arch_post_init`，写的是 VM 进程的 `p_seg.p_cr3` 与 `p_seg.p_cr3_v`。

```c
/* 逐字引用，minix3/minix/kernel/arch/i386/protect.c:arch_post_init。 */
void arch_post_init(void)
{
  /* Let memory mapping code know what's going on at bootstrap time */
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);
  get_cpulocal_var(ptproc) = vm;
  pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
}
```

VM 接手时**显式跳过所有 BIGPAGE 条目**：

```c
/* 逐字引用，minix3/minix/servers/vm/pagetable.c:pt_init 的复制循环。 */
		/* BIGPAGEs are kernel mapping (do ourselves) or boot
		 * identity mapping (don't want).
		 */
		if(!(entry & ARCH_VM_PDE_PRESENT)) continue;
		if((entry & ARCH_VM_BIGPAGE)) continue;
```

两句注释把两件事分开：内核映射由 VM 自己重建；引导期的恒等映射是"不要的"。也就是说 §2.6 建起来的那张页目录里，只有非大页条目会被 VM 继承下来——4KB 细粒度映射搭的架子留下，4MB 大页恒等映射在 VM 接手那一刻被丢弃。VM 侧页表的完整结构见 `rewrite-notes/02-stage-vm/07-pagetable-struct.md`。

**其二，4KB 映射的占位值与地址限制。** `pg_map` 入口的两个约束：

```c
/* 节选（保留入口的占位判断与地址断言），minix3/minix/kernel/arch/i386/pg_utils.c:pg_map。 */
	if(phys == PG_ALLOCATEME) {
		assert(!(vaddr % I386_PAGE_SIZE));
	} else  {
		assert((vaddr % I386_PAGE_SIZE) == (phys % I386_PAGE_SIZE));
		vaddr = pg_rounddown(vaddr);
		phys = pg_rounddown(phys);
	}	
	assert(vaddr < kern_vir_start);
```

`PG_ALLOCATEME` 定义在 `minix3/minix/kernel/arch/i386/include/archconst.h`，值为 `((phys_bytes)-1)`——一个"物理页由你分配"的哨兵值，而不是合法地址。传它的时候调用方只给虚拟地址范围，每个虚拟页的物理页由 `pg_alloc_page` 现取（`minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc` 就是这么用它的）。`assert(vaddr < kern_vir_start)` 说明这条路径只服务低地址区：4KB 细粒度映射在这里的作用是给"内核要访问的用户地址范围"搭架子，真正的用户页表由 VM 在它自己的地址空间里建。

**其三，`I386_VM_USER` 在恒等映射里的实际作用。** `pg_identity` 给每个条目置了 `I386_VM_USER`，而 boot 阶段内核在 Ring 0 执行，U/S 位不参与内核自己的访问判定——这个位在 C 里没有实际效果。有意义的对照是 `pg_mapkernel` 不置它：内核地址被标成 supervisor-only。恒等映射也不会留给用户态用：VM 的 `pt_init` 跳过 BIGPAGE 条目、自己新建页目录。新设计里的对应选择（boot 用的恒等映射不置该位）见 §4.6。

### 2.7 同一个函数名在 ARM32 上：自己造一份 multiboot

Minix3 同时有 x86 与 ARM 两个端口，同一段引导准备在两个端口里用了同一串函数名。先看两侧的入口（`minix3/minix/kernel/arch/earm/head.S:multiboot_init` 与 `minix3/minix/kernel/arch/i386/head.S:multiboot_init`）：

```asm
/*
 * 逐字引用，minix3/minix/kernel/arch/earm/head.S（上半）与
 * minix3/minix/kernel/arch/i386/head.S（下半，作为对照）。
 */
/* earm */
.section .startup
.global MINIX
MINIX:
	b multiboot_init

.text
multiboot_init:
	ldr	sp, =load_stack_start	/* make usable stack */
	mov	fp, #0
	bl	_C_LABEL(pre_init)

	ldr	sp, =k_initial_stktop
	mov	r1, #0
	push	{r1}			/* Terminate stack */
	ldr	r2, =_C_LABEL(kmain)	/* r0 holds kinfo_t ptr */
	bx	r2
```

两侧的相同与不同都很直接：入口标签同名（`MINIX`），第一跳标签同名（`multiboot_init`），临时栈的放置位置与写法一致（`load_stack` 在文件末尾的 `.data` 段，取的是"高端"符号），清帧指针一致（`%ebp` / `fp`），调用 `pre_init` 与随后调用 `kmain` 的一致。不同的是指令集本身，以及参数传递：x86 用栈传两个参数（魔术值 + `multiboot_info` 地址），ARM 用寄存器传参，而 `pre_init` 返回的 `kinfo` 指针留在 `r0` 里一路带到 `kmain`。

真正体现"引导协议不属于内核"的地方在 `pre_init` 内部（ARM 侧是 `minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi`，两侧的 `get_parameters` 分别在 `minix3/minix/kernel/arch/earm/pre_init.c:get_parameters` 与 `minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`）。ARM32 的 `get_parameters` 第一件事不是读引导器结构体，而是**造一个**：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi。
 */
void setup_mbi(multiboot_info_t *mbi, char *bootargs)
{
	memset(mbi, 0, sizeof(*mbi));
	mbi->flags = MULTIBOOT_INFO_MODS | MULTIBOOT_INFO_MEM_MAP |
			MULTIBOOT_INFO_CMDLINE;
	mbi->mi_mods_count = MB_MODS_NR;
	mbi->mods_addr = (u32_t)&mb_modlist;

	int i;
	for (i = 0; i < MB_MODS_NR; ++i) {
		mb_modlist[i].mod_start = MB_MODS_BASE + i * MB_MODS_ALIGN;
		mb_modlist[i].mod_end = mb_modlist[i].mod_start + MB_MODS_ALIGN
		    - ARM_PAGE_SIZE;
		mb_modlist[i].cmdline = 0;
	}

	/* morph the bootargs into multiboot */
	mbi->cmdline = (u32_t) bootargs;

	mbi->mmap_addr =(u32_t)&mb_memmap;
	mbi->mmap_length = sizeof(mb_memmap);

	mb_memmap.size = sizeof(multiboot_memory_map_t);
	mb_memmap.mm_base_addr = MB_MMAP_START;
	mb_memmap.mm_length  = MB_MMAP_SIZE;
	mb_memmap.type = MULTIBOOT_MEMORY_AVAILABLE;
}
```

逐条解码：

- `memset` 之后自己置 `flags` 的三位：有模块、有内存表、有命令行。x86 上这三位是引导器置的，ARM 上是内核自己置的。
- 模块列表是**编译期常量**：`MB_MODS_BASE + i * MB_MODS_ALIGN`。这意味着 ARM 端口假设了"U-Boot 已经把模块放在这些固定地址上"，`mod_start`/`mod_end` 只是把这个约定抄进结构体。
- 内存表只有一条：`mb_memmap` 一个条目，`mm_base_addr = MB_MMAP_START`、`mm_length = MB_MMAP_SIZE`。x86 上这张表可能有十几条（BIOS 报告的内存空洞），ARM 上是一条硬编码的连续区间。
- `mbi->cmdline = (u32_t) bootargs;`：命令行来自 U-Boot 通过 `r0`/`r1` 传进来的参数，注释写作"morph the bootargs into multiboot"。这句话是整个函数的性质说明——它把"另一种引导器给的参数形式"翻译成 Multiboot 的形状。
- 于是 `get_parameters()` 里其余部分（`memcpy(&cbi->module_list, mbi->mi_mods_addr, …)`、`add_memmap` 遍历、`overlaps`、`cut_memmap`、`mb_set_param`）在两个端口上同构，调用次序一致；但**区间约定不同**：ARM 的 `overlaps` 用的是闭区间（`INRANGE` 写成 `(v) >= mod->mod_start && (v) <= thismod->mod_end`，`OVERLAP` 第二项直接取 `mod2->mod_end`），正是 §2.5 论证为错误的那一版。ARM 端口甚至保留了 `multiboot` 这个词。

撇开两个端口的硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 传 `bootargs` + `setup_mbi` 自造结构 | 引导协议（各端口不同） |
| 2 | 建立地址映射 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear` + `:pg_identity` + `:pg_mapkernel` | 同名函数（`minix3/minix/kernel/arch/earm/pg_utils.c:pg_identity`），1MB Section 代替 4MB 大页 | 分页硬件（各端口不同，语义相同） |
| 3 | 开启分页/MMU | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_load` + `:vm_enable_paging`（写 `CR3`，置 `CR0.PG`） | 同名函数（`minix3/minix/kernel/arch/earm/pg_utils.c:pg_load` 写 `TTBR0`；`minix3/minix/kernel/arch/earm/pg_utils.c:vm_enable_paging` 置 `SCTLR.M` 并使能 I/D cache 与分支预测） | MMU 寄存器（各端口不同） |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定（x86 用栈、ARM 用寄存器） |

ARM 侧这两步比 x86 多几个前置：`vm_enable_paging` 先写 `TTBCR`（页表基址控制）与 `DACR`（把 16 个域全设为 Client），再读改写 `SCTLR`；开 MMU 之前还有一次 `dcache_clean()`，调用点在 `minix3/minix/kernel/arch/earm/pre_init.c:pre_init`。

步骤 2、3 在各端口都调用同名函数，是因为"清零页目录 → 建恒等映射 → 把内核映射到高地址 → 装载页表基址 → 开启分页"这一串的**语义**与"用哪条指令、页表项多少位"无关；第 1 步是唯一必须按端口另写的一步，因为"参数从哪来"由引导器决定。第 4 步的差别只在调用约定。

**如果反过来想**：ARM32 侧省掉 `setup_mbi`，让 `get_parameters` 直接用 x86 的第二个形参（那里是 `ebx`，一个由 GRUB 填好的结构体地址）。ARM 的 `head.S` 只传了 `r0`（`bootargs`），`pre_init` 的第二个形参读到的是栈上或寄存器里的残留值。`setup_mbi` 之后才存在的三个 `flags` 位在这里没人置位，于是 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 在 `mi_flags` 为垃圾值时直接失败；即使它偶然通过，下一句 `memcpy(&cbi->module_list, (void *) mbi->mi_mods_addr, mbi->mi_mods_count * sizeof(multiboot_module_t))` 会从任意地址拷进最多 20 条模块描述符，随后 `cut_memmap` 按这些垃圾区间把真实 DRAM 从记录里切掉——记录不会报错，只是可用内存凭空少了几段。

### 2.8 本章归纳

把 2.1 到 2.7 的调用关系收成一张图：

```
引导器（GRUB / U-Boot / 固件）
  │  ① 识别头 + 寄存器约定 + 填好的结构体
  ▼
head.S: multiboot_init
  │  设临时栈、清帧指针、按调用约定传参
  ▼
pre_init(magic, ebx)
  ├── get_parameters(ebx, &kinfo)
  │     ├── memcpy(&kinfo.mbi, ebx)
  │     ├── 链接期常量 → bootstrap_start / bootstrap_len / vir_kern_start
  │     ├── mbi.mi_cmdline → 逐词扫描 → mb_set_param → kinfo.param_buf
  │     ├── mbi.mi_mods_* → kinfo.module_list[]
  │     ├── mbi.mmap_* → add_memmap() × N          （构建可用内存记录）
  │     ├── 内核当第 N+1 个模块 → overlaps() × N → panic 或继续
  │     └── cut_memmap() × (N+1)                   （切掉内核与模块占用区）
  ├── pg_clear()                （页目录清零）
  ├── pg_identity(&kinfo)       （低地址恒等映射，覆盖当前执行点）
  ├── freepde_start = pg_mapkernel()（内核高地址映射，返回第一个空闲槽位号）
  ├── pg_load()                 （CR3 = 页目录物理地址）
  └── vm_enable_paging()        （CR0.PG 置位，PSE/WP/PGE 按序处理）
  ▼
return &kinfo
```

三条次序结论，各配一条源码依据：

- **清单先于用清单者**。`pre_init` 的注释把这件事说成"先建 1:1 映射让这段代码留在原地"，而清单的第一次使用（`cut_memmap`）发生在 `get_parameters` 内部——两者都在 `pg_clear()` 之前完成（依据：`minix3/minix/kernel/arch/i386/pre_init.c:pre_init` 的语句顺序）。
- **恒等映射先于内核映射**。两个函数写同一张 `pagedir[]`；`pg_identity` 覆盖全部 1024 个条目，`pg_mapkernel` 从下标 961 开始写（依据：`minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity` 的循环上界 `I386_VM_DIR_ENTRIES` 与 `minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel` 的起始下标计算）。
- **信息结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`，`head.S` 把它压栈作为 `kmain` 的第一个参数（依据：`minix3/minix/kernel/arch/i386/pre_init.c:pre_init` 的 `return &kinfo;` 与 `minix3/minix/kernel/arch/i386/head.S:multiboot_init` 的 `push %eax`）。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`。按 §1.3 给的依据链，它落在高半一侧（`mpx.o` 不在 `objcopy --prefix-symbols` 名单、`mpx.S` 把它放在 `.bss`、`kernel.lds` 的通用 `.bss` 在 `. += _kern_offset` 之后）；仍未查清的是 `LABEL(...)` 宏（本仓无定义）如何拼出最终符号名。该依据链与 `02-higher-half-kernel.md` §2.4 共享，边界说明见 §1.3。

把本章的断言按用途分一下：`assert(cbi->bootstrap_start % I386_PAGE_SIZE)` 这类断言不能证明布局一定合法，它把"不合法"从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。**如果反过来想**：把这些 `assert` 在编译时全部去掉（`NDEBUG`）。`overlaps` 不再拦重叠布局，而 `cut_memmap` 对同一段物理内存被两个模块声明的情况会切两次——第二次切除在一个已经被清零的条目上做"夹取后无重叠"判断，于是什么也不切，记录把这段内存记成两个洞之间的余量；`pg_alloc_page` 仍会从尾部发页，把这段物理内存发给两个使用者。症状不出现在启动期，而在共享这段内存的两方都开始写入之后，且没有任何一处会报"内存被给了两次"。

---

## 3. Rust 设计决策

### 3.1 引导协议换代：Multiboot 与 GRUB 的角色由固件接口替换 `[ARCH: 引导协议从 Multiboot 换成固件接口]`

C 的实现前提写在 2.1 与 2.2 里：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与一个内存分配接口（`AllocatePages`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot + GRUB） | Rust（UEFI 路径） | Rust（OpenSBI 路径） |
|---|---|---|---|
| 识别可执行文件 | 内核镜像前 8KB 的识别头（2.1） | UEFI 的 PE/COFF 头（标准可执行格式） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 调 `GetMemoryMap` 一次，产出两份列表 | boot-shim 用硬编码的 QEMU virt DRAM 布局 |
| 传递参数 | 寄存器 + `multiboot_info_t` | `KernelInfo`（按值）+ 根页表物理地址 | 同 UEFI |
| 退出固件服务 | 无此步骤（GRUB 早已离开） | 必须调 `ExitBootServices`，之后 Boot Services 全部失效 | 无此步骤（U-Boot 已把控制权交出去） |

其中"退出固件服务"这一行需要展开：C 里引导器跳过来时早已不提供服务，而 UEFI 里 boot-shim 是在 Boot Services 的**环境内**运行的，它必须在某个时刻主动声明"我不再需要你"。这个声明有两个后果：文件系统协议与页分配接口同时消失。

```rust
// 节选（保留与调用约束相关的行；注释有删节与改述，被略处见省略号），os/boot-shim/src/uefi_helpers.rs:fn prepare_boot。
// ── 文件访问与内存快照的先后，注释即源码给出的理由 ──
        // Both file accesses must happen before ExitBootServices because
        // they depend on the SimpleFileSystem protocol.
        let file_loader = UefiFileLoader;
        ...
        let kern = loader::load_kernel_with_loader(&file_loader).expect(...);
        ...
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);
        ...
        // One snapshot, two lists — conventional (free) plus the
        // non-conventional remainder that hosts everything the kernel
        // executes and touches through low addresses at runtime.
        let (memmap, reserved_regions) = build_memmaps();
        ...
        exit_boot_services();
```

`exit_boot_services` 自己的文档也把这件事说成契约：

```rust
// 逐字引用，os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services。
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded because the kernel already received the
    // CONVENTIONAL regions via `build_memmap()`.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}
```

被"否决"的三种替代路径与理由：

| 方案 | 否决理由（可复核） |
|---|---|
| 保留 Multiboot，做 64 位扩展（Multiboot2） | 两个新架构没有等价协议；`head.S` 那套"引导器填结构体 + 寄存器传参"在 aarch64/riscv64 上要各写一份，等于三份引导协议 |
| 各架构独立引导（x86 用 Multiboot、ARM 用 PSCI、RISC-V 用 SBI） | 引导准备的代码要按架构分叉，而 2.7 展示的正是"这一步的语义与硬件无关"——分叉会把它重新绑回硬件 |
| 内核自带 stub（把 boot-shim 链接进内核镜像，内核自己读文件再自举） | boot-shim 与内核变成同一个二进制的两个入口，内核要依赖固件类型才能编译；riscv64 上 UEFI 目标不存在（`riscv64-unknown-uefi` 不是 Rust 支持的目标），这条路径会让内核在该架构上不可构建 |

**如果反过来想**：把 `ExitBootServices` 挪到"读完文件、建完内存图"之前（先退出、再读文件）。Boot Services 消失之后 `SimpleFileSystem` 协议不再可用，`loader::read_required` 拿不到 `kernel.elf` 的字节——它是 `read` 加 `panic!` 的封装，于是启动停在 `boot-shim: required file not found`（`FileLoader::read_required` 的默认实现）。用户看到的是一个"找不到文件"的消息，而真实原因是退出时机，两者之间隔着一次协议失效：文件仍在磁盘上，只是没人能读了。

### 3.2 一个 trait 两种固件：`BootShim`

C 里"同一串函数名、两套实现"的机制见 2.7：两个端口各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

Rust 侧把这件事显式化：固件相关的部分收进一个只有**一个方法**的 trait（`os/libs/minix-boot/src/boot_shim.rs:trait BootShim`），其余全部共享。

```rust
// 逐字引用，os/libs/minix-boot/src/boot_shim.rs:trait BootShim 与 struct BootPrepareResult。
pub struct BootPrepareResult {
    pub kernel_info: KernelInfo,
    pub root_page: PhysBytes,
    /// Bump region base address (physical).
    pub bump_base: u64,
    /// Bump region end address (physical, exclusive).
    pub bump_end: u64,
}

pub trait BootShim {
    /// Prepare boot: discover memory, load kernel and boot modules,
    /// allocate pages, build KernelInfo, exit firmware services.
    ///
    /// This is called exactly once, before the kernel sets up paging.
    /// After this call, firmware boot services are no longer available.
    ///
    /// The kernel's physical/virtual base and size are determined internally
    /// by the implementation (e.g., by parsing the kernel ELF's PT_LOAD
    /// segments). The caller only specifies how many bump pages to allocate
    /// for boot-stage page table construction.
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
```

差异被压到一个更小的接口上（`os/boot-shim/src/loader.rs:trait FileLoader`）：两条路径都要拿到"文件字节"，唯一不同的是字节从哪来。

```rust
// 逐字引用（省略号处为 trait 文档注释），os/boot-shim/src/loader.rs:trait FileLoader。
pub trait FileLoader {
    /// Read a file by path. Returns `None` if the file does not exist or
    /// cannot be read.
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// Read a file that must exist. Default panics with a useful message.
    fn read_required(&self, path: &str) -> Vec<u8> {
        match self.read(path) {
            Some(d) => d,
            None => panic!("boot-shim: required file not found: {}", path),
        }
    }
}
```

`read_required` 是"缺件必须致命"这件事的**唯一**落点：它把"文件不存在"从 `None` 变成一个带路径的 panic。两个实现各自只提供 `read`：UEFI 走 `SimpleFileSystem`（并把正斜杠转成 UEFI 要求的分隔符），OpenSBI 走 U-Boot 预先落在 RAM 里的 `BootFileTable`。除此之外的全部装载逻辑（解析、段拷贝、模块页分配、结构体构造）只有一份。

trait 相对"纯 feature gate + 散落条件编译"的四条理由，都对应可检查的事实：

- 类型安全：`UefiBootShim` 与 `OpenSbiBootShim` 是两个不同的类型，在 riscv64 上写 `UefiBootShim::prepare_boot` 是编译错误，而不是"运行时发现走错了分支"。
- 封装：每个固件的实现是一个整体，不能把 `uefi_helpers::build_memmap` 与 `opensbi_helpers::alloc_root_page` 混着调——它们不在同一个 impl 里，混合调用需要显式指名两个模块。
- 可测试性：`os/boot-shim/src/loader.rs` 的 tests 里有一个 `MockLoader`（内存表实现），它证明 `FileLoader` 可以被替换；`os/boot-shim/src/loader.rs` 的 tests 里 `MockLoader` 就是 `FileLoader` 的一个内存表实现，装载逻辑的全部单元测试都跑在它上面。
- 条件编译的落点收敛：与 feature 有关的 `cfg` 属性只出现在 `lib.rs` 的模块与再导出声明处（口径：`grep -c 'cfg(feature' os/boot-shim/src/lib.rs` 计三处，另有 1 处 `cfg(all(feature = "opensbi", not(feature = "uefi")))`，合计四条），业务逻辑里没有条件编译。

否决的方案：

| 方案 | 否决理由 |
|---|---|
| 把引导逻辑做成 kernel 内部的一个模块 | 内核会永久依赖 UEFI crate（`uefi`），而它没有 riscv64 目标；内核的依赖面被引导需求污染 |
| 只做 feature gate，不定义 trait | 调用方直接依赖具体函数名，新增一种固件时没有任何编译期检查保证新实现补齐了同样的步骤；也无法在宿主上用一个假实现验证上层逻辑 |
| 业务代码里散落 `#[cfg(uefi)]` | 加一种固件要在多处补条件编译，且每处都可能漏；`lib.rs` 里现有的 feature 门只有四条（`grep -c 'cfg(feature'` 计三处 + 一处 `cfg(all(feature = ...))`） |

**如果反过来想**：把"内核物理/虚拟基址与大小"改成 `prepare_boot(kern_virt_base, kern_phys_base, kern_size, bump_pages)` 由调用者传入。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改成调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，Step 2 会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。

### 3.3 crate 依赖方向：boot-shim 依赖内核，内核不依赖 boot-shim

C 里只有一个二进制（内核镜像），引导准备与内核主流程在同一地址空间、同一份全局变量里，因此"依赖方向"这个问题不存在。

Rust 侧有两个二进制与一个共享 crate，方向必须是单向的：

```
boot-shim ──┬─→ minix-boot   （BootShim trait、KernelInfo、BootPrepareResult）
            ├─→ minix-arch   （pt_alloc 注册接口、Paging trait）
            └─→ minix-kernel （arch_boot / bootstrap_to_kernel_image）
minix-kernel ─┬─→ minix-arch
              ├─→ minix-boot
              └─→ minix-plat、minix-platform、minix-types
minix-boot ──→ minix-types

kernel-image ─┬─→ minix-kernel
              ├─→ minix-boot
              ├─→ minix-arch
              └─→ minix-plat
```

依据是四份依赖表（命令：`grep -n 'minix-' os/boot-shim/Cargo.toml os/kernel/Cargo.toml os/libs/minix-boot/Cargo.toml os/kernel-image/Cargo.toml`）：

- `os/boot-shim/Cargo.toml` 的 `[dependencies]` 里有 `minix-types`、`minix-boot`、`minix-arch`、`minix-kernel`、`minix-plat`；`minix-elf`（共享的 ELF 解析器）；`uefi = "0.33"` 挂在可选的 `dep:uefi` 上，由 `uefi` feature 打开。
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，`os/plat/src/early_console.rs`），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`；另有两条**按目标注入**的依赖：`cfg(target_arch = "riscv64")` 与 `cfg(target_arch = "aarch64")` 下各加 `boot-shim`（`opensbi` feature）与 `fdt`。所以"内核不依赖 boot-shim"这条边界对 `minix-kernel` 成立，而 `kernel-image` 的自引导面按目标反向依赖 boot-shim 的库——这是同一份代码的两个产物，方向不同。

"内核不依赖 boot-shim"挡的是一件具体的事：`boot_shim` 包在 `uefi` feature 打开时会把外部 crate `uefi` 拉进来，而 `uefi` crate 只支持 `x86_64/aarch64/i686-unknown-uefi` 三个目标。内核若依赖引导层，riscv64 上就没法构建。

`KernelInfo`、`BootShim`、`BootPrepareResult` 之所以住在第三方 `minix-boot` 而不是住在内核或引导层里，就是为了让两侧都能引用同一份定义而不引入反向依赖。这一层只有类型与 trait，没有实现（`minix-boot` 的依赖表只有 `minix-types`），所以谁依赖它都不会被拖进固件代码。

**如果反过来想**：让 `minix-kernel` 依赖 `boot-shim`（理由是"想复用 `BootPrepareResult` 的定义"）。构建 riscv64 内核时会连 `uefi` crate 一起编译，而它没有该架构的编译目标——直接后果是内核在 riscv64 上不可构建；把共享类型抽到 `minix-boot` 得到的收益正是"不必让内核认识固件"。这条边界不是风格洁癖的产物：`os/kernel/Cargo.toml` 里那条"没有 minix-arch 之外的硬件依赖"的界线，可以用上面那条 `grep` 一次性验证。

### 3.4 从全局 `kinfo` 到按值交接的 `KernelInfo`

C 里 `kinfo` 是一个全局变量。`pre_init.c` 开头就定义 `kinfo_t kinfo;`，`get_parameters(ebx, &kinfo)` 往里填，`pg_utils.c` 里的记账函数通过 `cbi` 形参收到它的地址，`pre_init` 返回 `&kinfo`，`kmain` 再把这个指针传下去。整条链上传递的是**地址**，而地址之所以有效，是因为所有代码都在同一个地址空间里、链接成同一个镜像。

Rust 侧有两个分别编译的二进制，各有自己的 `.bss`。如果沿用"传地址"的形状，接收方会拿到一个在另一个镜像里有效、在自己这里指向一段完全无关内存的指针；甚至同一个 `&'static KernelInfo` 类型也不能保证——它的类型说的是"活到程序结束"，不是"我的页在对方的页表里被映射"。交接因此必须是**值**，而值里的切片要能被子解释。

```rust
// 逐字引用（略去文档注释），os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo。
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

`os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo` 的字段数是 14（复算：`awk '/^pub struct KernelInfo/,/^}/' os/libs/minix-boot/src/kernel_info.rs | grep -cE '^\s+pub [a-z_]+:'`）。按与 C 的关系分成下面几类：

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`user_sp`、`bootstrap_start`、`bootstrap_len` | `memmap` 从 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带；`kern_size` 用 `u64` 而不是 `usize`，避免 32 位目标上的截断 |
| 改型（C 有、形状变） | `param_buf`（`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验）、`module_list[]` → `boot_modules: &'static [BootModule]`、`mmap_size`（并入切片长度） | |
| 改名 | `vir_kern_start` → `kern_virt_base`；`freepde_start` → `free_upper_idx`（类型变 `Option<usize>`，见 §3.8） | |
| 删除 | 16 个字段：`mbi`、`mods_with_kernel`、`kern_mod`、`kmessages`、`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`、`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`、`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic` | 逐个的去向见下表 |
| 新增 | `reserved_regions`、`kern_stack_top`、`syscall_entry`、`platform_sources` | 各自来源见下 |

被删的 16 个字段逐个的去向（"接管者"一栏里给的都是本篇可核的落点）：

| C 字段 | 接管者 / 去哪了 | 依据 |
|---|---|---|
| `mbi` | 不进内核：UEFI/SBI 没有 Multiboot 结构体 | `os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo` 无对应字段 |
| `mods_with_kernel` | 内核布局改由 `kern_phys_base`/`kern_size` 单独表达，"把内核当第 N+1 个模块"的做法消失 | §3.4 的字段表 |
| `kern_mod` | 同上 | 同上 |
| `kmessages` | 独立的内核消息/诊断设施 | `os/kernel/src/kmess.rs`（`KernelConsole`） |
| `do_serial_debug` | 早期控制台初始化自己决定串口 | `os/plat/src/early_console.rs` |
| `serial_debug_baud` | 同上（波特率由平台实现持有） | 同上 |
| `minix_panicing` | panic 路径自己维护状态 | `os/boot-shim/src/main.rs:fn panic`、`os/kernel/src/lib.rs:fn register_panic_diagnostic` |
| `user_end` | 由 `user_sp` 与地址空间布局常量推导 | `os/libs/minix-types/src/types/boot.rs:const USER_STACK_TOP` |
| `boot_procs[]` | 内核侧从 `boot_modules` 构造 boot image 表 | `os/kernel/src/vm_handoff.rs:fn build_vm_handoff` |
| `nr_procs` / `nr_tasks` | 常量与表长度 | `os/kernel/src/proc_table.rs:const NR_PROCS`、`:const NR_TASKS`、`os/libs/minix-boot/src/kernel_info.rs:const NR_BOOT_MODULES` |
| `release[]` / `version[]` | 内核构建身份，来源改为包版本（当前未接线） | `os/kernel/src/misc.rs` 的 GET_KINFO 段 |
| `vm_allocated_bytes` | VM 交接记录的字段 | `os/kernel/src/vm_handoff.rs:fn build_vm_handoff` |
| `kernel_allocated_bytes` | 同上（记录里的 `kernel_allocated_static`） | 同上 |
| `kernel_allocated_bytes_dynamic` | 同上（记录里的 `kernel_allocated_dynamic`，值来自 bump 分配器用量） | `os/kernel/src/boot_alloc.rs:fn boot_alloc_used_bytes` |

新增字段各自的来源与理由：

- `reserved_regions`：非 conventional 类型的内存区间（内核运行期仍会经由低地址访问的那部分）。它的存在理由是执行模型——UEFI 路径下内核代码被链接进 boot-shim 的 PE 镜像里执行，其代码/GDT/IDT/TSS 落在低物理地址；VM 建立进程页表时必须把这些窗口按恒等（VA = PA、supervisor-only）复现，否则第一次 `CR3` 切换后的下一条取指就三次故障。OpenSBI 路径上传 `&[]`，因为那里内核是独立的高半 ELF。
- `free_upper_idx`：`None` 表示 boot-shim 未计算。它对应 C 的 `freepde_start`，语义与消费见 §3.8。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到；这里把它们显式纳入结构体，交接便不再依赖"两边链接脚本碰巧有同名符号"。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 `minix3/sys/arch/i386/include/multiboot.h`）。它是 2.3 那张参数表在 Rust 侧的落点，但**当前 boot-shim 传的是空切片**：UEFI 的 Load Option（固件为启动项保存的那段参数二进制）与 SBI 的启动参数都还没有被解析成 `key=value` 形式。消费者（`GET_MONPARAMS`）对空切片的处理是按 C 的固定 1024 字节零填充语义补齐，因此"没有参数"与 C 里 `param_buf[0] == '\0'` 等价。

`#[repr(C)]` 在这里的作用需要说清楚边界：

```rust
// 节选（还原原注释的换行；括号内的过程标记以省略号代之），os/libs/minix-boot/src/kernel_info.rs 的字段顺序注释。
// `#[repr(C)]` pins the field ORDER (and each POD field's offset) so the
// boot-shim's by-value write and the high-half kernel image's read (…)
// agree. Same-toolchain/profile builds are already
// deterministic, but making it explicit removes the compiler's freedom to
// reorder fields between the two separate `cargo` invocations. NOTE: the
// `&'static [T]` fields' internal (ptr, len) order is still toolchain-defined;
// the cross-image contract therefore relies on both images being produced by
// one `xtask image` run (identical rustc + profile) — see `handoff` module doc.
```

这一段说的是三件事：字段顺序被钉住；切片内部的 `(ptr, len)` 顺序没有被子钉住；因此跨二进制契约成立的前提是两个镜像由同一次构建产出。它不是版本化 ABI，这一点在 `handoff.rs` 的模块注释里被重复说明（"This is *not* a versioned ABI across arbitrary toolchains"）。

跨镜像交接的那份载荷本身是另一个结构体（`os/libs/minix-boot/src/handoff.rs:struct BootHandoff`）：

```rust
// 逐字引用，os/libs/minix-boot/src/handoff.rs:struct BootHandoff。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoff {
    pub kernel_info: KernelInfo,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
}

// The blob is bump-allocated one 4KiB page; if the contract ever outgrows a
// page the boot-shim's single `boot_pt_alloc()` handoff would silently write
// past it. Fail the build instead of corrupting the next bump allocation.
const _: () = assert!(core::mem::size_of::<BootHandoff>() <= 4096);
```

四个字段里有三个是"交接之后还要继续被使用"的状态：根页表物理地址（接收方要往同一张表上加映射）、bump 区间与游标（见 §3.7）。页大小断言把"这份载荷装得下一页"变成编译期检查。

**如果反过来想**：把 `KernelInfo` 的切片按原指针存进内核的全局槽，不做深拷。三块切片的源都在 boot-shim 的 UEFI 池堆里，那些页只被固件的 1:1 页表映射；内核跳到自己建的页表之后，同一个虚拟地址读回来是零。真机记录给了这一现象的两组读数：`res=127 zeroed=127`（`reserved_regions` 全部读成零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 现场看不到。

### 3.5 `bootstrap_start/len`：从"可回收区间"到"必须为 0"

C 的这两个字段装的是链接脚本导出的一小段内核镜像内区域的边界（2.3 已看到赋值）。这段区域的性质写在链接脚本里：

```ld
/*
 * 节选（保留段划分与偏移量相关的行），minix3/minix/kernel/arch/i386/kernel.lds。
 */
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);

SECTIONS
{
	. = _kern_phys_base;
	__k_unpaged__kern_unpaged_start = .;

	.unpaged_text : { unpaged_*.o(.text) }
	.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }
	.unpaged_bss  ALIGN(4096) : { unpaged_*.o(.bss COMMON) }
	__k_unpaged__kern_unpaged_end = .;

	. += _kern_offset;

	...
	.text             : AT(ADDR(.text) - _kern_offset) { *(.text*) }
	...
}
```

逐条解码：

- 前三个赋值就是 2.6 里 `kern_phys_start`/`kern_vir_start` 的来源，也是 02 篇讲 `AT()` 的两条基址。
- `. = _kern_phys_base;` 让第一段的位置计数器落在物理基址上；`unpaged` 三个段（来自文件名匹配 `unpaged_*.o` 的目标文件）放在这里，它们的**执行地址等于装载地址**（没有 `AT()` 偏移）。
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。
- `. += _kern_offset;` 之后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后就不再需要"的代码/数据（它的执行地址等于物理地址，靠恒等映射就能访问）。C 在启动完成之后把它 `add_memmap` 归还给通用分配器。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上内核镜像的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外）；**实际执行地址**上，x86_64 与 aarch64 的 UEFI 链把内核链接进 boot-shim 的 PE 镜像里执行，此时内核跑在固件安排的物理地址上（§3.4 的 `reserved_regions` 与 §4.8 讲的就是这件事），而 riscv64 的自引导链与 aarch64 的独立镜像链从第一条指令起就在高半。两种执行形态下都不存在"分页开启前执行、之后可归还"的**内核段**：UEFI 链的内核代码段归 boot-shim 的 PE 镜像管（它不在内核的回收范围里），独立镜像链的内核段全程需要。因此 `bootstrap_start/len` 保留成"形状对称的字段"，值固定为 `PhysBytes(0)` 与 `0`，语义是"没有需要归还的区间"。

与 C 的差异不止"值不同"，还有**执行方式不同**：C 里这段区域的重启路径是一条守卫（字段非零才归还），Rust 里它被提升成入口校验的一条硬断言（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo::validate`）：

```rust
// 逐字引用（保留断言本身与其前两行注释），os/libs/minix-boot/src/kernel_info.rs:KernelInfo::validate。
        // 1. bootstrap_len must be 0 (Rust port has no unpaged section).
        //    C: kinfo.bootstrap_len = &_kern_unpaged_end - &_kern_unparsed_start
        //    Rust: always 0 (see bootstrap_start field doc).
        assert_eq!(
            self.bootstrap_len, 0,
            "KernelInfo: bootstrap_len must be 0 (Rust port has no unpaged section), got {}",
            self.bootstrap_len
        );
```

变成硬断言的收益是故障前移：一个非零的 `bootstrap_len` 不再有机会进入后续分配逻辑，而是在 `kmain` 入口的校验阶段停下。误设的后果可以逐块列出来：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0` | 那块内存实际存着什么 | 被误分配后先坏在哪 |
|---|---|---|
| `[0, OpenSBI 固件尾)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址 |
| DTB 所在区间 | 设备树 blob；boot-shim 不解析它，只把物理地址作为 `PlatformDescSource` 传下去 | 内核解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇） |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件与那张表 | 后续从表里再取文件时读到垃圾；表魔数校验失败即中止 |
| boot-shim 自身 | 正在执行的 boot-shim 代码与栈 | 直接取指失败（这一块在误回收发生时可能已经不被引用） |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写掉"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。

**如果反过来想**：保留 C 的语义（字段可回收）但不小心把长度按"上界地址"而不是"区间长度"填，例如填成 `kern_phys_base.0`（`build_kernel_info` 的调用点注释把这一取值记为过度回收的来源）。内核会在启动后期执行 `add_memmap(0, kern_phys_base.0)`，把 `[0, 0x200000)` 之类的低内存整段加入空闲清单——这段里包含固件与引导器的活动结构。第一次从这份清单分配物理页的调用者（VM 的启动分配器或内核的页表池）会把页表页写进那里，覆盖掉四行表格里的东西之一。故障点与写入点之间隔着若干次分配，从崩溃现场无法回溯到"多了一个 0x200000 的长度"。

### 3.6 `HUGE_PAGE_SIZE` 与 `map_huge`：放进基础 trait 还是扩展 trait

C 的建表两个函数都直接用 4MB 大页：`pg_identity` 给每个页目录项置 `I386_VM_BIGPAGE`，`pg_mapkernel` 也置同一位（2.6 已贴）。大页在这个端口里不是可选能力，而是"两级页表 + 1024×4MB = 4GB"这套布局的必要条件：单个页目录项直接指向 4MB 物理块，才可能在只有一级页表的情况下覆盖 4GB。

64 位侧情况不同：层数变多，每个架构支持的大页尺寸集合与"怎么表示这是一个大页"都不同。Rust 侧把这部分能力放进一个扩展 trait（`os/arch/src/arch/paging_ext.rs:trait HugePages`）：

```rust
// 逐字引用，os/arch/src/arch/paging_ext.rs:trait HugePages。
pub trait HugePages: Paging {
    /// Supported huge page sizes (in bytes), sorted largest first.
    const HUGE_PAGE_SIZES: &'static [usize];

    /// Preferred huge page size for Direct Map (bytes).
    const HUGE_PAGE_SIZE: u64;

    /// Page table level shift for the preferred huge page.
    const HUGE_PAGE_SHIFT: u32;

    /// Fallback huge page size when the preferred size is not available.
    const FALLBACK_HUGE_PAGE_SIZE: u64;

    /// Hardware-specific bit that identifies a PTE as a huge-page entry
    /// (x86_64: PS bit `1 << 7`; ARM64/RISC-V: 0 — no extra bit needed).
    const PTE_HUGE_IDENTIFIER_BIT: u64;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }

    fn supports_1gb_page() -> bool {
        true
    }
}
```

四条理由，逐条对应可检查的事实：

- `HugePages: Paging` 是**超集**关系：boot 阶段用 `P: HugePages` 一个 bound 就能同时用到 `Paging::new_from_page`/`enable` 与 `HugePages::map_huge`（`arch_boot_impl<P: HugePages>` 就是这么写的）。运行时只用 `P: Paging` 的消费者不必携带大页负担。
- 标识方式因架构而异。x86-64 靠条目的 PS 位（bit 7）区分大页与"指向下一级表"；ARM64 与 RISC-V **没有这一位**，所以常量取 0。一个"三个架构都要提供、但其中两个恒为 0"的常量放在基础 trait 里，等于把架构差异写进共有接口。
- 大页是**可选能力**而不是基础能力。x86-64 的首选是 1GB、回退 2MB；把 `map_huge` 放进 `Paging` 会让所有 `Paging` 实现都必须实现它，包括那些不打算支持大页的实现。
- `Paging` 的职责面保持窄：`new_from_page`、`from_active_root`、`map`、`unmap`、`query`、`enable` 这些是"页表能做什么"；大页参数是"硬件能提供多大的块"。

被否决的方案：

| 方案 | 否决理由（对应上面哪条） |
|---|---|
| `HUGE_PAGE_SIZE` 移入 `Paging` | 违反第 2、3 条：ARM64/RISC-V 会被迫携带一个恒为 0 的标识位常量；不支持大页的实现被迫提供 `map_huge` |
| 另立一个只服务 boot 的 `PageTableBoot` trait | 与 `Paging::map/query` 重复——同一套硬件操作出现两个 trait，调用方要同时满足两个 bound |
| 不定义 trait，直接把 `map_huge` 写成具体类型的方法 | boot 路径无法用泛型统一三种架构；`arch_boot_impl<P>` 的 `P` 会退化成三个 `#[cfg]` 分支 |

x86-64 的实现给出这套设计的两个可核对点（`os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging` 的常量与 `fn map_huge`）：

```rust
// 逐字引用，os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging 的常量与 map_huge 守卫段。
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    const PTE_HUGE_IDENTIFIER_BIT: u64 = X64PteFlags::PS.bits();
...
        if shift == PDPT_SHIFT {
            unsafe { write_entry(pdpt, i3, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
            return Ok(());
        }

        // Check if PDPT entry is already a 1GB leaf (PRESENT + PS both set).
        // If so, demoting it would corrupt the existing mapping.
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
```

- `HUGE_PAGE_SIZES = &[1GB, 2MB]`、`HUGE_PAGE_SIZE = 1GB`、回退 2MB：尺寸阶梯是"首选失败往下退"。谁选尺寸不在这里，而在 `arch_boot_impl` 的校验步骤（`os/kernel/src/lib.rs:fn boot_validate_and_prepare`）：

```rust
// 逐字引用，os/kernel/src/lib.rs:boot_validate_and_prepare 的尺寸选择段。
    // Choose the largest huge-page size that both bases and kern_size are aligned to.
    let kern_huge = if kv.is_multiple_of(huge) && kp.is_multiple_of(huge) && ks.is_multiple_of(huge) {
        huge
    } else if kv.is_multiple_of(fallback) && kp.is_multiple_of(fallback) && ks.is_multiple_of(fallback) {
        fallback
    } else {
        panic!("arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE");
    };
```

  三条件（虚拟基址、物理基址、长度）同时对齐到 1GB 才用 1GB。这条阶梯不是风格：`check-layout.sh` 的 L5 断言要求镜像跨距按 2MiB 对齐，正是为了让它至少能落进回退档。
- `AlreadyMapped` 守卫处理的是"目标槽位已经是一个 1GB 大页"的情形。注释给了理由："demoting it would corrupt the existing mapping."

**如果反过来想**：去掉那条守卫，直接按 2MB 路径往下走。`e3` 里原本存的是 `paddr | PRESENT | PS`（因为 1GB 大页的 PDPT 条目就是叶条目），代码在"条目已存在"分支里做的是 `(e3 & ADDR_MASK) as *mut u64`——也就是把那个 1GB 物理块的基址当成下级页表指针。接下来 `write_entry(pd, i2, ...)` 写的是 `paddr + i2*8`：这段内存正被 1GB 映射覆盖，可能就是内核自己的数据页。写完之后 1GB 映射还在（条目没变），但它的前 4KB 已经被当成页表改写。守卫的代价是"这次 2MB 映射失败"，去掉守卫的代价是"被映射的那段物理内存头部被写坏"。

### 3.7 引导期页表页分配器的换手

C 的两个来源已经看过：`pg_alloc_page` 从可用内存记录的最高条目尾部取一页，`alloc_pagetable` 从 6 页静态池里取一页，用完 panic（2.4、2.6）。这个策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

Rust 侧把两者拆开（`os/arch/src/arch/pt_alloc.rs:fn register`、`:fn alloc_pt_page`）：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

```rust
// 逐字引用（省略号处为模块级的并发安全注释），os/arch/src/arch/pt_alloc.rs。
type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;
...
static PT_ALLOC: PtAllocSlot = PtAllocSlot(UnsafeCell::new(uninit_alloc));
static PT_REGISTERED: AtomicBool = AtomicBool::new(false);
...
pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    // Double registration within one domain is a boot bug — two allocators
    // would fight over the singleton. (The kernel guards its own call with
    // `is_registered()` so a test kernel may register first; the VM server
    // registers in its own address space.)
    debug_assert!(
        !PT_REGISTERED.load(Ordering::Relaxed),
        "pt_alloc::register called twice"
    );
    // SAFETY: write-once contract — callers register during boot
    // (single-threaded) or user-space VM init before any concurrent
    // access. The debug_assert above catches accidental re-registration.
    unsafe {
        core::ptr::write(PT_ALLOC.0.get(), alloc_fn);
    }
    // Release: publish the fn pointer write to any thread that Acquire-loads
    // `is_registered() == true` before calling `alloc_pt_page()`.
    PT_REGISTERED.store(true, Ordering::Release);
}
...
#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    let _ = PT_REGISTERED.load(Ordering::Acquire);
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}
```

boot 阶段的实现是一个 bump 分配器（`os/kernel/src/boot_alloc.rs:BootAlloc::alloc`）：

```rust
// 逐字引用，os/kernel/src/boot_alloc.rs:BootAlloc::alloc。
    pub fn alloc(&self) -> Result<(PhysBytes, VirBytes), PageTableError> {
        let pa = self.next.fetch_add(0x1000, Ordering::Relaxed);
        if pa >= self.end.load(Ordering::Relaxed) {
            // Roll back the increment on failure
            self.next.fetch_sub(0x1000, Ordering::Relaxed);
            return Err(PageTableError::AllocationFailed);
        }
        // Zero-fill the page. In test mode, skip the write (no real memory).
        #[cfg(not(test))]
        // SAFETY: pa is a valid, 4KB-aligned physical address returned by the
        // bump allocator. Under identity mapping (boot stage), VA=PA, so the
        // pointer is valid. The page is not aliased by any other live reference.
        unsafe { core::ptr::write_bytes(pa as *mut u64, 0, 512) };
        Ok((PhysBytes(pa), VirBytes(pa)))
    }
```

换手发生在三个时刻，各有锚点：

| 时刻 | 谁写这个槽 | 代码 | 依据 |
|---|---|---|---|
| boot-shim 进入内核之前 | boot-shim 进程自己 | `boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end); minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);` | `os/boot-shim/src/main.rs:main` |
| `arch_boot_impl` Step 0 | 只有"未注册"时才写 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | `os/kernel/src/lib.rs:boot_validate_and_prepare` |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | `os/servers/vm/src/vm_server.rs` |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；boot-shim 侧是第一个写者，它的 `register` 调用是无条件的（`os/boot-shim/src/main.rs:fn main`）。这个槽是"写一次"的，重写是 bug，因此没有"后注册者覆盖前者"的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池（C 的对应物是 `pt_free` 把页表页还回 `alloc_mem` 池）；两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff`）；引导期建表之后的 Direct Map 覆盖建立则在 `os/kernel/src/dm_coverage.rs`：

```rust
// 节选（略去行号引用），os/kernel/src/vm_handoff.rs 的扣除区间表头与 bump 行。
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base()/kern_size()` |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs | `KernelInfo.boot_modules()` minus the VM
//!   module (reclaimed after ELF copy — C 对应物见本篇 2.4.1) |
```

```rust
// 逐字引用，os/kernel/src/vm_handoff.rs:build_vm_handoff 的 bump 扣除段。
    // Kernel-reserved: the boot bump region, whole range.
    if let Some((base, end)) = boot_alloc_region() {
        record.push(base, end.saturating_sub(base));
    }
```

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在 §3.7 的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（2.4.1 的第 2 次），而这份扣除表把**所有**启动模块的字节区间都算作内核保留——只有 VM 模块在"ELF 复制之后可回收"这一条被记成注释。理由是启动顺序：内核的 boot-exec 在 VM 服务起来之前逐个装载 0..7 号服务器，它们的段帧正是从这份自由清单里取的；若按 C 那样先把 VM 模块的区间还回去，VM 镜像可能在它自己被启动之前就被后续装载覆盖。

**如果反过来想**：按"已用部分"扣除（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时 `fetch_add(0x1000)` 返回 `0x1E64000` 并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）——而这一页刚被交给 VM 的物理内存管理器。若 VM 已经把它分配给某个进程页表，写零就把那张表的一个条目变成 0；切换 `CR3` 之后，被这条目覆盖的虚拟地址取指即页错误。现场只能看到一个"凭空消失"的映射。

### 3.8 `free_upper_idx`：字段保留、消费者搬走

C 的 `freepde_start` 是 2.6 末尾那条链的起点：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy` 借窗口做跨地址空间拷贝。这套机制的三个前提是：当前页表里确实有连续空槽、窗口只在当前 `CR3` 下有效、借出者保证归还。

64 位侧跨地址空间访问换了载体：内核有 Direct Map 窗口（一段固定虚拟基址到物理地址的线性映射，由 `arch_boot_impl` 的 Step 4 建立），加上软件页表走查（`os/kernel/src/pte_walk.rs` 一类的实现）就能在**不切页表、不借槽位**的前提下读到任意进程的页表项；用户空间的拷贝则由 VM 服务在自己的地址空间里处理。于是"借用空闲槽位"的三个前提在 64 位侧都不再是必要结构。

字段因此保留成 `Option<usize>`（`os/libs/minix-boot/src/kernel_info.rs:free_upper_idx`）：

```rust
// 逐字引用（略去 C 对应注释），os/libs/minix-boot/src/kernel_info.rs 的 free_upper_idx 字段注释。
    /// First free page table root-level index after identity+kernel maps.
    /// C: kinfo.freepde_start = pg_mapkernel() — pre_init.c
    /// x86-64: PML4 index, aarch64: TTBR1 L0 index, riscv64: Sv39 VPN[2]
    ///
    /// `None` means the boot-shim has not yet computed this value.
    /// In Minix3 C, `pg_mapkernel()` returns the index; the Rust boot-shim
    /// currently does not compute it, so it is set to `None` and the kernel
    /// must validate it before use.
```

保留下来的用处只有一处：把 C 的 `struct kinfo` 形状报给用户态时占住那个位置（`os/kernel/src/misc.rs` 的 `getinfo_kinfo` 用 `unwrap_or(0)` 把它填进返回结构）。这个出口也说明为什么要用 `Option` 而不是"随便算一个值"：`None` 是"没有这个值"，`Some(0)` 是"0 号槽位空闲"——后者会被下游当成有效索引使用。

在 64 位侧，偏移一位的形态与 C 不同：这个字段的消费者只有 GET_KINFO 的报告（`os/kernel/src/misc.rs`），所以一个"最后一个已用槽位"的值不会造成取指失败，而是让用户态读到一个错的槽号——把它当成有效索引去用。这正是字段保留为 `Option` 并在没有真实计算者时取 `None` 的理由：`None` 与"算错一位的值"在类型上无法区分，只能靠"不猜"避免；C 侧那套机制里偏移一位的后果（改写内核映射、下一次访问页错误或静默读错）见 §2.6。

对照表：

| 维度 | C 的槽位机制 | Rust 的窗口机制 |
|---|---|---|
| 借用者 | `createpde`（跨地址空间拷贝） | Direct Map 窗口 + 软件页表走查 |
| 借用范围 | 当前页表的 4MB 窗口（一个页目录项） | 内核固定映射窗口（整个物理地址空间的可达部分） |
| 归还时机 | 下一次切换地址空间时自然失效 | 不需要（窗口是常驻映射） |
| 失效条件 | 借来的条目只描述对方的一个 4MB 块 | 窗口覆盖范围由 DM 建立时的资格过滤决定 |

**如果反过来想**：在 64 位侧照搬"借用空闲 PML4 槽位"。每进程页表的高半槽位是从 bootstrap 根继承的（`X86_64Paging::inherit_supervisor_half` 把源根的 PML4 的 256 至 512 号槽位原样复制过来）。被借用的槽位在 `switch_address_space` 之后属于另一个地址空间：借出去的映射在下一次切换时静默失效，不会报错。症状是跨空间拷贝偶发读到"上一张页表留下的地址"——一个只在特定调度顺序下出现的错误，而不是一次可复现的崩溃。

### 3.9 控制权交接的两种形态：进程内调用与跨镜像跳转 `[ARCH: 内核从内联执行改为独立镜像 + 交接载荷]`

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

Rust 侧的交接面是一个函数：`arch_boot`（按架构分道的入口，同一份代码里有三个 cfg 分支，因此锚点取它的通用实现 `os/kernel/src/lib.rs:fn arch_boot_impl`）：

```rust
// 逐字引用（保留签名与首段注释），os/kernel/src/lib.rs:fn arch_boot（x86_64 版本）。
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    ...
    store_kernel_info(kernel_info);
    let kinfo: &'static KernelInfo = crate::kernel_info()
        .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
    ...
    arch_boot_impl::<X86_64Paging>(kinfo, root_page);
    ...
    unsafe { X86_64HigherHalf::jump_to_kmain(kinfo, kinfo.kern_stack_top) }
}
```

签名里的两个参数就是交接的全部内容：一份按值的启动信息、一个根页表页的物理地址。返回值是 `!`——它不返回。

aarch64 上多一层（`os/kernel/src/lib.rs:fn bootstrap_to_kernel_image`）：内核以独立 ELF 的形式存在于高半（TTBR1），boot-shim 建表、开分页、写一份 `BootHandoff`，然后**绝对跳转**过去。

```rust
// 逐字引用（省略号处为步骤注释与校验），os/kernel/src/lib.rs:fn bootstrap_to_kernel_image。
    let (blob_phys, _) = boot_alloc::boot_pt_alloc()
        .expect("bootstrap_to_kernel_image: handoff blob page alloc failed");
    let (bump_base, bump_end) = boot_alloc::boot_alloc_region()
        .expect("bootstrap_to_kernel_image: bump region not registered");
    let bump_next = boot_alloc::boot_alloc_next();
    let handoff = minix_boot::BootHandoff {
        kernel_info: *kernel_info,
        root_page: root_page.0,
        bump_base,
        bump_next,
        bump_end,
    };
    ...
    unsafe { core::ptr::write(blob_phys.0 as *mut minix_boot::BootHandoff, handoff) };
    ...
    let entry = kernel_info.kern_virt_base().0;
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

三处具体事实：

- 交接载荷多带了 `bump_next`。原因在 §3.7 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页与载荷页的页再发一次。
- `in("x0") blob_phys.0`：载荷指针绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧被明确要求"只做 additive 阶段"（`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`）：

```rust
// 逐字引用（保留结论句与理由），os/kernel/src/lib.rs:fn arch_boot_resume_high_half 的文档注释。
/// The boot-shim already built the bootstrap page-table root and enabled
/// paging; that root is *currently loaded* in TTBR0/TTBR1 and maps *this* code.
/// This entry runs only the boot phases that are purely *additive* on the live
/// root — validate/register allocators, land the KernelInfo into this image's
/// `.bss`, establish the direct map, then enter `kmain`. It must NOT rebuild the
/// tables: calling [`build_bootstrap_root_and_enable`] here would
/// `new_from_page(root_page)` and zero-fill the *active* root (including the L0
/// slot holding this very function and the MMIO window), so the next
/// instruction fetch would fault.
```

riscv64 上是第三种形态：内核镜像自己就是引导体（入口是 `os/kernel-image/src/main.rs` 里按架构分道的 `rust_image_main`，riscv64 的实现是 `os/kernel-image/src/bootface.rs:fn bootface`；aarch64 的对应面是 `os/kernel-image/src/bootface_a64.rs:fn bootface_a64`，只在该架构构建时编译），在没有 U-Boot 与文件系统的 `-kernel` 场景里从设备树解析内存、从 `BootFileTable` 或 DTB 通道取模块，然后调用同一个 `arch_boot`。三条链的入口同源、收敛点也是同一个函数。

**如果反过来想**（两处）：

其一，aarch64 退回内联执行。内联时内核代码住在 PE 镜像的运行物理地址上（低半、`VA = PA`、在 TTBR0 覆盖范围内）。第一次 `switch_address_space` 会换 TTBR0 到进程页表并发 `tlbi alle1is`，内核自身的取指映射随之消失——`handoff.rs` 的模块注释记下的现场是 `elr`/`vbar` 钉在同一个低地址上。跨镜像形态把这个问题的根去掉：内核代码链接在高半（TTBR1），而 TTBR1 在上下文切换中不被替换。

其二，接收侧按"干净启动"重建 bootstrap 根（调 `Paging::new_from_page`）。该函数的第一件事是把 512 个根条目清零（`write_bytes(ptr, 0, 512)`），而此时这个根**正在被使用**——承载当前函数的那条指令所在的 L0 槽就在其中。清零后的下一次取指立刻页错误，栈上连一条 panic 消息都来不及打印；"resume 面只做 additive 阶段"这条约束就是从这里来的。

### 3.10 从 Ch2 到 Ch3 的对照收束

| Ch2 的 C 事实 | Ch3 的设计回应 | 一句话差异 |
|---|---|---|
| 引导器代劳加载并把结果填进结构体（2.1/2.2） | 3.1 固件接口换代 + 3.2 `BootShim`/`FileLoader` | 加载工作从引导器转移到 boot-shim，固件差异收进两个 trait |
| 全局 `kinfo` + 指针传递（2.3） | 3.4 按值 `KernelInfo` + 深拷落地区 | 地址不能跨镜像，值可以 |
| `unpaged` 段可回收（2.3/2.6） | 3.5 语义改为"必须为 0" + 硬断言 | 没有分页前执行的内核段，就没有要归还的区间 |
| 4MB 大页是两个建表函数的实现细节（2.6） | 3.6 `HugePages: Paging` 扩展 trait | 大页从"实现细节"升级为"可查询的硬件能力" |
| 静态池 6 页 + 从记录尾部取页（2.4/2.6） | 3.7 注册-替换的分配器槽 | 分配策略与页表代码解耦，boot 与 VM 各注册一份 |
| `freepde_start` → 借槽位做临时窗口（2.6） | 3.8 字段保留为 `Option`，机制换 Direct Map + 走查 | 借用会失效，窗口不会 |
| 单二进制内的 `pre_init → kmain` 调用（2.2） | 3.9 两种交接形态（进程内调用 / 跨镜像跳转） | 交接面从一个指针变成一个函数签名 |

**如果反过来想**：若把这九个设计当作九条互不相关的偏好来读，其中两条的关联会被漏掉。3.5（`bootstrap` 区间语义重定义）与 3.6（大页能力放进扩展 trait）在表面上没有关系，但它们各自的成立前提是同一句话——"内核从第一条指令起就在高半执行"：因为不再有分页前执行的内核段，所以没有要归还的区间；因为建表要与运行时共用同一套页表接口，所以大页能力必须能被查询而不是被写死。把这条前提说出来，读者才能判断哪些设计会随执行模型一起变，哪些只是当前的实现选择。

---

## 4. 实现详解

### 4.1 入口与调用链（→3.1、3.2）

UEFI 入口是固件加载 `.efi` 后调用的那个函数。它只做三件事。

```rust
// 节选（保留三步骨架；注释有删节与改述，省略号处为被略内容），
// os/boot-shim/src/main.rs:fn main。
#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via BootShim trait
    //    Internally: GetMemoryMap → AllocatePages → load kernel ELF from ESP
    //    → load boot modules → build KernelInfo → ExitBootServices
    // 1024 pages (4 MiB): the identity mapping, the kernel high mapping,
    // DM coverage establishment (kernel + VM windows), the boot-identity eviction ... and the handoff page's fresh chain all draw
    // lower-level page-table pages from this bump pool via boot_pt_alloc.
    let result = UefiBootShim::prepare_boot(1024);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);
    boot_shim::raw_serial("boot-shim: [raw] pt_alloc registered, entering kernel arch_boot\r\n");

    // 3. Hand control to the kernel's arch-specific boot.
    //    Establishes page tables, enables paging, enters kmain.
    //    NEVER RETURNS.
    uefi::println!("boot-shim: handing control to kernel arch_boot");
    // aarch64：不进入内联的 arch_boot/kmain（见 §3.9），而是建好自举表、
    #[cfg(target_arch = "aarch64")]
    minix_kernel::bootstrap_to_kernel_image(&result.kernel_info, result.root_page);
    #[cfg(not(target_arch = "aarch64"))]
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);

    // This line should never be reached.
    unreachable!()
}
```

逐行解码：

- `os/boot-shim/src/uefi_helpers.rs:fn prepare_boot(1024)`：`1024` 是**页表页预算**（1024 页 = 4MiB）。第一段注释把这份预算的构成列了出来：恒等映射、内核高地址映射、Direct Map 覆盖建立、引导身份窗口的逐页拆解、以及跨镜像交接载荷页的链条——它们都从同一个 bump 池里取页。预算是"上限"而不是"必须用完"：`BootAlloc` 只在分配时推进游标。**未用部分不退回**——VM 交接记录按 `boot_alloc_region()` 把整段扣除（见 4.7 的扣除表），用掉的与剩下的都算内核保留。
- 注释 `--- Below this point, UEFI boot services are gone ---` 标的是分界线：这一行之后任何 `uefi::` 的调用都不可靠。分界线之后的第一件事是注册分配器，第二件事是 `raw_serial`（不依赖任何固件状态的裸串口写）。
- `uefi::println!("boot-shim: handing control to kernel arch_boot");` 是进入内核之前的最后一行固件输出。它存在的理由写在代码注释里：若 `arch_boot` 在内核打印第一个路标之前卡死，本行是最后一根可见路标。
- aarch64 走 `bootstrap_to_kernel_image`，其余架构走 `arch_boot`——分道只有这一处，两条路都会进入同一个 `kmain`。

**如果反过来想**：把步骤 2 的 `init_boot_pt_alloc` + `register` 挪到 `arch_boot(...)` 之后。Step 0 的兜底判据是 `boot_alloc_region()` 为 `None`（不是 `is_registered()`；`is_registered()` 只守注册这一步），于是它会从 `memmap` 里找一段满足 DM 可达上界的区间，取 `FALLBACK_BUMP_LEN`（`0x100_000`，256 页）作为新的 bump 池并注册。此后 `boot_alloc_region()` 指向这 256 页，VM 交接记录的扣除表扣的就是它；而 boot-shim 已经预留的那 1024 页（`assert_bootstrap_outside_memmap` 保证它不在 conventional 列表里）既不在扣除表里、也不在任何可用清单里——那 4MiB 被空置。若 boot-shim 的 `register` 仍然执行（只是挪后），它还会撞上"写一次"的约定：debug 构建里 `debug_assert` 直接失败，release 构建里静默改写函数指针。

### 4.2 UEFI 路径：`prepare_boot` 的顺序（→3.1、3.6）

```rust
// 节选（保留八步顺序；注释有删节与改述，省略号处为被略内容），os/boot-shim/src/uefi_helpers.rs:UefiBootShim::prepare_boot。
impl BootShim for UefiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        uefi::println!("boot-shim: prepare_boot enter");
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        uefi::println!("boot-shim: root+bump allocated");
        // 顺序约束（已由真机证实）：快照必须在**全部**
        // LOADER_DATA 分配之后——root 页、bump 区之外，还有 kernel 段与
        // 12 个 boot module 的 allocate_pages ...
        let file_loader = UefiFileLoader;
        uefi::println!("boot-shim: loading kernel.elf from ESP…");
        let kern = loader::load_kernel_with_loader(&file_loader).expect(...);
        uefi::println!("boot-shim: kernel loaded (entry staged)");
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);
        uefi::println!("boot-shim: 12 boot modules loaded");

        uefi::println!("boot-shim: locating platform sources…");
        let platform_sources = find_platform_sources();

        // One snapshot, two lists — conventional (free) plus the
        // non-conventional remainder ...
        let (memmap, reserved_regions) = build_memmaps();
        uefi::println!("boot-shim: memmap ok (final pre-EBS snapshot)");
        // Wire the bootstrap-outside-memmap defense ...
        assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
        assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);

        uefi::println!("boot-shim: building KernelInfo…");
        let kernel_info = build_kernel_info(...);

        uefi::println!("boot-shim: exiting boot services…");
        exit_boot_services();
        crate::raw_serial("boot-shim: [raw] boot services exited\r\n");

        BootPrepareResult { kernel_info, root_page, bump_base, bump_end }
    }
}
```

八步顺序与每步的约束：

| # | 动作 | 约束 |
|---|---|---|
| 1 | 分配根页表页 | 必须在 DM 可达上界之下（下一步的 `MaxAddress` 界） |
| 2 | 分配 bump 区 | 同上；这是页表页池 |
| 3 | 加载 `kernel.elf` | 必须在 EBS 之前（依赖 `SimpleFileSystem`） |
| 4 | 加载 12 个启动模块 | 同上 |
| 5 | 定位平台描述符来源 | 必须在 EBS 之前（依赖 System Table 的 Configuration Table） |
| 6 | 拍内存快照（两份列表） | 必须在**所有** `LOADER_DATA` 分配之后；这一次 `GetMemoryMap` 同时产出 free 与非 free 两份 |
| 7 | 断言自举分配不在 free 列表里 | 快照之后、EBS 之前（此时还能打印） |
| 8 | 构造 `KernelInfo`，退出 Boot Services | 退出之后只剩裸串口 |

第 1、2 步的界：

```rust
// 逐字引用（略去文档注释），os/boot-shim/src/uefi_helpers.rs。
fn bootstrap_alloc_limit(size: u64) -> u64 {
    boot_dm_admissible_end()
        .checked_sub(size)
        .expect("bootstrap allocation request exceeds the DM-admissible bound")
}

pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit(PAGE_SIZE)),
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("Failed to allocate root page below DM-admissible bound");
    PhysBytes(ptr.as_ptr() as u64)
}
```

`AllocateType::MaxAddress(limit)` 的语义是"分配出来的基址不超过 `limit`"，所以要把 `limit` 减去本次请求的大小，才能保证"整段在界之下"。这个界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口的物理上界)`：bump 池既要能被"恒等写通道"写到（构建期间），又要能被 VM 的 Direct Map 窗口表示（构建之后使用者要能通过窗口访问它）。

步骤 6 的两份列表在同一次遍历里分流：

```rust
// 节选（保留两遍扫描与分流条件；省略号处为诊断打印与注释），
// os/boot-shim/src/uefi_helpers.rs:fn build_memmaps。
pub fn build_memmaps() -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
    let mmap = boot::memory_map(MemoryType::LOADER_DATA).expect("Failed to get UEFI memory map");
    let mut regions: Vec<MemoryRegion> = Vec::new();
    let mut reserved: Vec<MemoryRegion> = Vec::new();
    let mut ram_top = 0u64;
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            let end = desc.phys_start + desc.page_count * 4096;
            if end > ram_top { ram_top = end; }
        }
    }
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            regions.push(MemoryRegion { base: PhysBytes(desc.phys_start), len: desc.page_count as usize * 4096 });
        } else if desc.ty != MemoryType::MMIO && desc.ty != MemoryType::MMIO_PORT_SPACE {
            if desc.phys_start >= ram_top { continue; }
            ...
            reserved.push(MemoryRegion { base: PhysBytes(desc.phys_start), len: desc.page_count as usize * 4096 });
        }
    }
    (Box::leak(regions.into_boxed_slice()), Box::leak(reserved.into_boxed_slice()))
}
```

- 第一遍只求 `ram_top`（所有 conventional 描述符的最高末地址）。第二遍才分流。两遍的原因是描述符集合在类型上无序，而"裁掉物理内存之上的一切"这条规则需要一个先算出来的上界。
- 分流条件：conventional 进 free 列表；非 conventional 且不是 `MMIO`/`MMIO_PORT_SPACE` 且落在 `ram_top` 之下，进 reserved 列表。MMIO 被排除的原因是它们是巨大的设备地址范围，不该被缓存地恒等映射；而 `ram_top` 之上被排除，是"物理上不可能被访问"的窗口——真机上该固件报过一个 192MiB 的 `RESERVED` 描述符（芯片组的高端空洞），它落在 `ram_top` 之上，被这一步裁掉。
- 两个列表被 `Box::leak` 泄露成 `'static`：引导只跑一次，这份内存不需要回收。
- 步骤 7 的断言只覆盖**两笔 UEFI 分配**（根页与 bump 区）。第三行注释记录了一次被证伪的更严格假设：内核段与模块段不在这条断言的范围内，因为它们是被**直接写进 conventional RAM**（不经过 UEFI 分配），由内核 A2 分类逐步扣除；曾经按"UEFI 会把分配改标从而自动排除"的假设把断言扩展到内核段，真机第一次启动就误炸（conventional `[0x100000, 0x800000)` 里包含内核 `[0x200000, 0x400000)` 属于预期形状）。

**如果反过来想**：把内存快照提到第 1、2 步之前（先拍快照、再分配根页与 bump 区）。这两页在拍快照时还是 `CONVENTIONAL`，于是被 `regions` 收下，作为"空闲内存"交给内核。内核 A2 分类把 `memmap` 拷进 `FREE_MEMMAP` 后，这两页仍在账上——VM 的物理内存管理器可以从中取帧，而它们正被用作页表页与页表页池。`assert_bootstrap_outside_memmap` 就是为此设的闸：它遍历 free 列表，若发现 `[root_page, root_page+4096)` 或 `[bump_base, bump_end)` 与某条区间相交就立刻 panic，此时串口还在（EBS 之前），消息里同时给出两个区间。反过来说，若这条断言也被去掉，故障会推到 VM 取帧之后，现场只剩一个"页表内容被改坏"的取指错误。

### 4.3 两条路径共用的装载函数（→3.2）

模块契约是本篇里最容易在改动时踩坏、又最容易在测试里漏掉的东西，因为它的"顺序"部分没有任何编译期检查。

```rust
// 逐字引用（省略号处为文档注释的说明段），os/boot-shim/src/loader.rs。
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";
pub const MODULES_DIR: &str = "/EFI/minix/modules";

pub const MODULE_NAMES: &[&str] = &[
    "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm", "pfs", "mfs", "init",
];

// 逐字引用：编译期计数断言。
const _: () = assert!(MODULE_NAMES.len() == NR_BOOT_MODULES);
```

- 名字表有 12 项（复算：`sed -n '/pub const MODULE_NAMES/,/];/p' os/boot-shim/src/loader.rs | grep -o '"[a-z]*"' | wc -l` 输出 12），`NR_BOOT_MODULES = 12` 定义在 `minix-boot`。
- 顺序按 C 的 boot image 表（`minix3/minix/kernel/table.c` 里 `struct boot_image image[]` 的用户态段）排列：`ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`。
- **位置不是进程号**。内核把第 `i` 个模块绑定到 `BOOT_MODULE_PROC_NRS[i]`，而后者里的数字来自 `com.h`——例如 `ds` 在位置 0，进程号是 6。名字表这一行代码上方给了这条注解，它是顺序契约的一部分。

装载循环：

```rust
// 逐字引用（省略号处为文档注释），os/boot-shim/src/loader.rs:fn load_boot_modules_with_loader。
pub fn load_boot_modules_with_loader<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
) -> &'static [BootModule] {
    let mut modules: Vec<BootModule> = Vec::new();

    for &name in MODULE_NAMES {
        let path = build_module_path(name);
        let data = loader.read(&path).unwrap_or_else(|| {
            panic!(
                "boot-shim: required boot module '{}' not found at {} \
                 (kernel needs all {} in BOOT_MODULE_PROC_NRS order)",
                name,
                path,
                MODULE_NAMES.len()
            )
        });

        let num_pages = (data.len() + 4095) / 4096;
        let phys_base = alloc_pages(num_pages).unwrap_or_else(|| {
            panic!(
                "boot-shim: out of memory loading boot module '{}' ({} pages)",
                name, num_pages
            )
        });

        unsafe {
            let dest = phys_base as *mut u8;
            dest.copy_from_nonoverlapping(data.as_ptr(), data.len());
            let remaining = num_pages * 4096 - data.len();
            core::ptr::write_bytes(dest.add(data.len()), 0, remaining);
        }

        let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
        modules.push(BootModule {
            name: name_static,
            start: PhysBytes(phys_base),
            len: data.len(),
        });
    }

    Box::leak(modules.into_boxed_slice())
}
```

四处具体事实：

- `num_pages = (data.len() + 4095) / 4096`：向上取整到页，注意分子里的 4095 而不是 4096。
- 分配出来的页里，`data.len()` 之后的余量被显式清零（`write_bytes(dest.add(data.len()), 0, remaining)`）。模块镜像按文件字节数拷贝，页尾的余量清零保证同一页内的任何对齐空洞都是确定值，而不是上一轮使用留下的内容。
- `BootModule.len` 记录的是**文件字节数**，不是页数。消费者按 `[start, start+len)` 计算占用区间。
- 缺件是 panic 而不是 `continue`。错误消息里带着模块名、路径与总数，落点在 boot-shim 侧；如果改成静默跳过，错误会推迟到内核侧的数量断言，那里只有"期望 12、实际 N"，没有缺失的名字。

OpenSBI 路径的 `FileLoader` 实现是查表：

```rust
// 逐字引用（略去文档注释），os/boot-shim/src/opensbi_helpers.rs:fn entry_path_eq 与 FileLoader for UbootFileLoader。
fn entry_path_eq(entry: &BootFileEntry, path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() >= BOOT_FILE_PATH_MAX {
        return false;
    }
    entry.path[..bytes.len()] == *bytes && entry.path[bytes.len()] == 0
}

impl<'a> FileLoader for UbootFileLoader<'a> {
    fn read(&self, path: &str) -> Option<alloc::vec::Vec<u8>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot's `fatload` placed `entry.len` bytes at
        // `entry.phys_addr`. The region is identity-mapped during boot
        // and not concurrently modified.
        let slice = unsafe {
            slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize)
        };
        Some(slice.to_vec())
    }
}
```

`entry_path_eq` 把"路径必须 NUL 结尾"变成匹配条件的一部分：`entry.path[bytes.len()] == 0`。若 U-Boot 脚本填表时没在路径后补零，`find` 返回 `None`，随后 `read_required` 报"找不到内核"——表里明明有一个条目，症状却与"没填"相同。`BootFileTable::valid_entries` 里 `entry_count` 被夹到容量上限（`min(BOOT_FILE_TABLE_MAX_ENTRIES)`），因此一个写坏的 `entry_count` 只会让多余的条目被忽略，不会越界读。

**如果反过来想**：把 `MODULE_NAMES` 里 `tty` 与 `mib` 两项互换位置。装载会全部成功（12 个文件都在），`const _: () = assert!(... == NR_BOOT_MODULES)` 也通过——数量没变。但是位置 7 上的 `BootModule` 记的是 `mib` 的镜像与名字，内核按位置把它绑到 `tty` 的进程号上。启动过程依旧能走到 `kmain`，进程表也能建起来；症状出现在第一次对 `tty` 或 `mib` 发系统调用时——调用被打到错误的进程号，表现为"服务没有响应"或"收到协议外的回复"，与"装载时顺序错了"之间隔着整个启动流程。

### 4.4 `KernelInfo` 的构造：两个 builder 与一个校验（→3.4、3.5）

两个固件路径各有一个 `build_kernel_info`，填的是同一个结构。参数表差异反映两种执行模型：

```rust
// 逐字引用（略去文档注释），os/boot-shim/src/uefi_helpers.rs:fn build_kernel_info。
pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    reserved_regions: &'static [MemoryRegion],
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    boot_modules: &'static [BootModule],
    bootstrap_start: PhysBytes,
    bootstrap_len: u64,
    platform_sources: &'static [PlatformDescSource],
) -> KernelInfo {
    KernelInfo {
        memmap,
        reserved_regions,
        kern_virt_base,
        kern_phys_base,
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(minix_types::USER_STACK_TOP),
        kern_stack_top: VirBytes(kern_virt_base.0 as u64 + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        platform_sources,
        param_buf: &[],
    }
}
```

```rust
// 节选（保留与 UEFI 版不同的两行；其余字段逐一对应，省略号处为字段注释），
// os/boot-shim/src/opensbi_helpers.rs:fn build_kernel_info。
    KernelInfo {
        memmap,
        // No runtime identity windows on this path — the kernel
        // executes as the standalone higher-half kernel.elf here (the
        // boot-shim is a separate binary), so no low physical VA range
        // hosts live kernel code/data. UEFI's link-in execution model
        // is what populates this list (uefi_helpers::build_memmaps).
        reserved_regions: &[],
        ...
    }
```

| 字段 | 谁填 | 值从哪来 |
|---|---|---|
| `memmap` | builder 调用方 | UEFI：步骤 6 快照的 conventional 列表；OpenSBI：硬编码单条 DRAM |
| `reserved_regions` | 调用方 | UEFI：快照的非 conventional 列表；OpenSBI：空 |
| `kern_virt_base` / `kern_phys_base` / `kern_size` | 装载函数 | ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)` |
| `free_upper_idx` | builder | 固定 `None`（见 §3.8） |
| `user_sp` | builder | `minix_types::USER_STACK_TOP` 单一权威常量 |
| `kern_stack_top` | builder 派生 | `kern_virt_base + kern_size`（内核镜像末尾即栈顶） |
| `syscall_entry` | builder 派生 | `kern_virt_base`（入口相对基址的偏移由架构侧决定） |
| `boot_modules` | 装载函数 | 按 `MODULE_NAMES` 顺序的 12 项 |
| `bootstrap_start` / `bootstrap_len` | 调用方 | 固定 `PhysBytes(0)` / `0`（见 §3.5） |
| `platform_sources` | 调用方 | UEFI：Configuration Table 扫描结果；OpenSBI：`[DTB]` 或空 |
| `param_buf` | builder | 固定 `&[]`（加载选项解析未接线） |

`boot_validate_and_prepare` 是这份结构在内核侧的第一次被读，它在建表之前先做校验：

```rust
// 节选（保留校验段；诊断路标宏与注释省略，尾部兜底注册块以省略号代替），os/kernel/src/lib.rs:fn boot_validate_and_prepare。
    let kv = kernel_info.kern_virt_base().0;
    let kp = kernel_info.kern_phys_base().0;
    let ks = kernel_info.kern_size();
    let huge = P::HUGE_PAGE_SIZE;
    let fallback = P::FALLBACK_HUGE_PAGE_SIZE;

    assert!(ks > 0, "arch_boot: kern_size must be > 0");
    assert!(kp.is_multiple_of(0x1000), "arch_boot: kern_phys_base must be page-aligned");
    assert!(kv.is_multiple_of(0x1000), "arch_boot: kern_virt_base must be page-aligned");
    let kern_huge = if kv.is_multiple_of(huge) && kp.is_multiple_of(huge) && ks.is_multiple_of(huge) { huge }
        else if kv.is_multiple_of(fallback) && kp.is_multiple_of(fallback) && ks.is_multiple_of(fallback) { fallback }
        else { panic!("arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE") };
    assert!(ks.is_multiple_of(kern_huge), "arch_boot: kern_size must be a multiple of the chosen huge page size");
    assert!(kernel_info.kern_stack_top().0.is_multiple_of(16), "arch_boot: kern_stack_top must be 16-byte aligned");
    ...
```

而 `KernelInfo::validate`（`kmain` 入口调用）是第二道，两条互补：

| 校验点 | 挡的是哪一类 bug |
|---|---|
| `ks > 0` | boot-shim 没装载成功却继续往下走 |
| 两个基址页对齐 | 布局常量写错（链接脚本改了而工具没跟） |
| 大页阶梯（三条件对齐） | 镜像跨距不满足 2MiB 对齐 → 提前给出可读的 panic |
| `kern_stack_top` 16 字节对齐 | ABI 要求（AAPCS64 / SysV 的栈对齐） |
| `KernelInfo::validate` 的 `bootstrap_len == 0` | 字段被误设（见 §3.5） |
| `KernelInfo::validate` 的 `kern_size != 0` | 同 `ks > 0`，在另一个入口再挡一次 |
| `KernelInfo::validate` 的 `memmap` 非空 | 内存图丢了 |
| `KernelInfo::validate` 的 `boot_modules` 非空 | 模块装载失败 |
| `KernelInfo::validate` 的 `param_buf.len() <= 1024` | 参数缓冲超过 C 的固定容量 |

**如果反过来想**：去掉 `KernelInfo::validate` 里的 `bootstrap_len == 0` 断言，只保留文档里的说明。字段非零时不会 panic，`kmain` 会拿一份含"待归还区间"的账继续走下去；如果后续某一步真的按它归还（保留 C 的 `add_memmap` 语义），故障点就落到 §3.5 表里的那块内存上，而"字段被误设"这件事在现场没有任何痕迹。断言的价值是把这条信息从"文档里的约定"变成"入口处的停止"。

### 4.5 内核侧第一步：`store_kernel_info` 的三块落地区（→3.4）

`arch_boot` 的早期动作是把按值传入的结构深拷进内核自己的 `.bss`（`os/kernel/src/lib.rs:fn store_kernel_info`），然后只使用这份副本。

```rust
// 节选（保留三个落地区各自的核心语句；省略号处为注释与相邻块的接续行），
// os/kernel/src/lib.rs:fn store_kernel_info。
pub fn store_kernel_info(kernel_info: &KernelInfo) {
    unsafe {
        let store = &mut *crate::globals::RESERVED_REGION_STORE.get();
        let src = kernel_info.reserved_regions;
        assert!(
            src.len() <= store.len(),
            "store_kernel_info: reserved_regions exceeds the .bss landing pad"
        );
        if src.as_ptr() != store.as_ptr() {
            core::ptr::copy(src.as_ptr(), store.as_mut_ptr(), src.len());
        }
        ...
        let mstore = &mut *crate::globals::MEMMAP_REGION_STORE.get();
        let msrc = kernel_info.memmap;
        assert!(
            msrc.len() <= mstore.len(),
            "store_kernel_info: memmap exceeds the .bss landing pad"
        );
        if msrc.as_ptr() != mstore.as_ptr() {
            core::ptr::copy(msrc.as_ptr(), mstore.as_mut_ptr(), msrc.len());
        }
        let bstore = &mut *crate::globals::BOOT_MODULE_STORE.get();
        let bnames = crate::globals::BOOT_MODULE_NAME_STORE.get();
        let bsrc = kernel_info.boot_modules;
        assert!(
            bsrc.len() <= bstore.len(),
            "store_kernel_info: boot_modules exceeds the .bss landing pad"
        );
        if bsrc.as_ptr() != bstore.as_ptr() {
            for (i, module) in bsrc.iter().enumerate() {
                let bytes = module.name.as_bytes();
                let n = bytes.len().min(crate::globals::BOOT_MODULE_NAME_LEN - 1);
                let slot = &mut (*bnames)[i];
                slot[..n].copy_from_slice(&bytes[..n]);
                let name: &'static str = core::str::from_utf8_unchecked(&slot[..n]);
                bstore[i] = minix_boot::BootModule { name, start: module.start, len: module.len };
            }
        }
        let mut k = *kernel_info;
        k.reserved_regions = core::slice::from_raw_parts(store.as_ptr(), src.len());
        k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
        k.boot_modules = core::slice::from_raw_parts(bstore.as_ptr(), bsrc.len());
        *KERNEL_INFO.get() = Some(k);
    }
}
```

三块落地区的容量与溢出行为：

| 落地区 | 容量常量 | 值 | 溢出行为 |
|---|---|---|---|
| `RESERVED_REGION_STORE` | `RESERVED_REGION_STORE_LEN` | 256 | panic |
| `MEMMAP_REGION_STORE` | `MEMMAP_REGION_STORE_LEN` | `crate::memmap::MAXMEMMAP` | panic |
| `BOOT_MODULE_STORE` | `minix_boot::NR_BOOT_MODULES` | 12 | panic |
| `BOOT_MODULE_NAME_STORE`（名字字节池） | `BOOT_MODULE_NAME_LEN` | 16（每槽，复制时截到 15 字节） | 截断（不 panic） |

名字池是唯一允许截断的：`n = bytes.len().min(BOOT_MODULE_NAME_LEN - 1)`（16 - 1 = 15），保留一个字节给 NUL 语义上的余量。截断后用 `core::str::from_utf8_unchecked` 造 `&'static str`，安全性论证是"模块名是 ASCII 字面量，15 字节的截断不会切开多字节序列"——这条论证的成立依赖 `MODULE_NAMES` 的内容，而不是类型系统。

`if src.as_ptr() != store.as_ptr()` 这个条件不是优化：`kmain` 会在拿到全局副本之后再调一次 `store_kernel_info`（幂等），此时源就是落地区本身，跳过拷贝可以避免 `ptr::copy` 在同址范围内的行为问题（注释里说明这也是用 `ptr::copy` 而非 `copy_nonoverlapping` 的原因）。

三个容量的注释各自记着一次真机故障：`RESERVED_REGION_STORE_LEN = 256` 与 `vm_handoff` 的候选上限共用同一个常量（单一权威，避免两处 256 手工对齐漂移）；`MEMMAP_REGION_STORE_LEN` 的注释记着 vh1 那次"15 条 conventional 条目在 Step 4 时完好、到分类点长度全 0、VM 自由清单塌成零条、VM 在启动参数校验处 panic"；`BOOT_MODULE_NAME_LEN = 16` 与 `vm_handoff::copy_name` 的 16 字节 `proc_name` 字段同宽，落地不会比既有长度契约更紧。

**如果反过来想**：把 `reserved_regions` 深拷进去，但 `memmap` 与 `boot_modules` 保持原指针（只做一半）。现象就是上面那次真机故障：Step 4 建立 Direct Map 覆盖时读 `memmap` 还正常（那些页当时还在），到 `vm_handoff::classify` 再读时全部是零长度条目。`kmain` 的 A.2 步骤用 `if region.len > 0` 过滤拷进 `FREE_MEMMAP`，零条目被静默跳过，于是 `FREE_MEMMAP` 变成空记录——第一次物理页分配失败时，现场离"三个切片里有两个没落地"隔着整个 Phase A。

### 4.6 自举根的建立与开启（→3.6、3.7）

Step 1 与 Step 2 是两段映射循环，它们的范围判据不同：

```rust
// 逐字引用（保留注释与循环体），os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable。
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

```rust
// 逐字引用（保留守卫与循环体），同函数 Step 2。
    let kern_virt = kernel_info.kern_virt_base().0;
    let kern_phys = kernel_info.kern_phys_base().0;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < kernel_info.kern_size() {
            paging.map_huge(
                VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
                kern_huge as usize, kern_flags,
            ).expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge;
        }
    }
```

Step 1 的注释给出两条判据：

- 范围是 `IDENTITY_MAP_END`（固定的低地址上界），不是"可用内存的并集"。理由写在 Step 1 之前的注释里：UEFI 的加载器代码/数据可能落在 `LOADER_DATA`/`LOADER_CODE` 类型的内存里，它们不是 conventional；不覆盖这些区间，切页表后的下一条取指就取不到码。
- 标志是 `kernel_read_write() | EXECUTABLE`（supervisor-only + 可执行），不是"用户可访问"。RISC-V Sv39 上 `U=1` 的页在 supervisor 模式下要被当作可执行，需要 `sstatus` 里的 `SUM` 位（permit Supervisor User Memory access）为 1，而这里没有设它——所以恒等映射必须是 supervisor-only。C 的 `pg_identity` 反而置了 `I386_VM_USER`：boot 阶段内核跑在 Ring 0，U/S 位不参与内核自己的访问判定，那个位在 C 里没有实际作用（有意义的对照是 `pg_mapkernel` 不置它，把内核地址标成 supervisor-only），Rust 侧因此不再置它。
- 循环里 `let _ = paging.map_huge(...)` 忽略错误：某些地址没有物理内存支撑，映射失败是可以接受的，因为 CPU 不会去访问它们。

Step 2 的守卫 `if kern_virt != kern_phys` 记录在注释里：`kern_virt == kern_phys` 时 Step 2 与 Step 1 是同一个映射，跳过它可以避免在 riscv64 上把同一个 VPN[2] 槽用不同的页尺寸写两次，破坏恒等映射。

Step 3 的 `enable()` 在 x86-64 上做四件事：

```rust
// 逐字引用（保留与硬件状态顺序相关的行；省略号处为注释），os/arch/src/x86_64/paging.rs:X86_64Paging::enable。
            const MSR_EFER: u32 = 0xC000_0080;
            const EFER_NXE: u64 = 1 << 11;
            let mut efer: u64;
            asm!("rdmsr", in("ecx") MSR_EFER, out("rax") efer, out("rdx") _, options(nostack));
            asm!("wrmsr", in("ecx") MSR_EFER, in("rax") efer | EFER_NXE, in("rdx") 0u64, options(nostack));
            // Switch CR3 to our own root page table.
            asm!("mov cr3, {}", in(reg) self.root_paddr);
            // Ensure WP (Write Protect, CR0 bit 16) is set so the kernel
            // cannot write read-only pages (W^X enforcement). UEFI firmware
            // does not guarantee WP is enabled.
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 31;
            cr0 |= 1 << 16;
            asm!("mov cr0, {}", in(reg) cr0);
            if cpu_supports_pge() {
                let mut cr4: u64;
                asm!("mov {}, cr4", out(reg) cr4);
                cr4 |= 1 << 7;
                asm!("mov cr4, {}", in(reg) cr4);
            }
```

顺序是：置 `EFER.NXE` → 写 `CR3` → 置 `CR0.PG` 与 `CR0.WP` → 按能力置 `CR4.PGE`。

- `EFER.NXE` 必须最先：`NX` 位的语义由 CR4 控制，而 `EFER.NXE = 0` 时任何带 NX 的页表条目在走表时被视为保留位，触发页错误且错误码 bit3（保留位）为 1。内核的 Direct Map 叶与用户数据/栈叶都带 NX，所以"开分页之前必须先置 NXE"。注释指出 UEFI 交接时恰好置了它，但这条不变量被当作"我们自己的"，不依赖固件。
- `CR3` 写的是内核自己的根：`build_bootstrap_root_and_enable` 返回前会把根页表物理地址记进内核全局（`set_current_root_phys(root_page)`），后续阶段（如装载 VM 的 ELF 时给同一张表加映射）通过 `Paging::from_active_root` 取回一个句柄。`from_active_root` 与 `new_from_page` 的区别是后者会清零根（`write_bytes(ptr, 0, 512)`），前者不会。
- `PGE` 与 C 的 `vm_enable_paging` 对应（2.6 的 `pgeok`）：只在 CPU 支持时置，且必须在 `PG` 之后。

C↔Rust 的五步对应：

| C | Rust | 差异 |
|---|---|---|
| `pg_clear()` | `new_from_page(root_page)` 的清零 | 根页由调用者提供（boot-shim 分配），函数只负责清零与记录地址 |
| `pg_identity(&kinfo)` | Step 1 的 `map_huge` 循环 | 范围从"全部 4GB"变成固定上界；页尺寸从 4MB 变成按架构阶梯选出的值 |
| `pg_mapkernel()` | Step 2 的 `map_huge` 循环 | 返回值（第一个空闲槽位号）不再被使用，见 §3.8 |
| `pg_load()` | `enable()` 里的 `mov cr3` | 无 |
| `vm_enable_paging()` | `enable()` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 多了 NXE；少了对 `PSE` 的显式处理（长模式下大页由条目自身的 PS 位决定） |

**如果反过来想**（两处）：

其一，恒等映射只覆盖 `memmap` 里报告的可用内存。UEFI 的加载器代码与数据可以落在 `LOADER_CODE`/`LOADER_DATA` 这类非 conventional 内存里；`enable()` 写完 `CR3` 的下一条取指就在这类内存中，而页表里没有它——页错误发生在 `enable()` 内部，此时内核还没有任何能打印故障的设施（控制台初始化在 `kmain` 里），现场是"开启分页后立刻静默停止"。

其二，`kern_virt == kern_phys` 时照做 Step 2。Step 1 已经把这个地址范围用 `kern_huge` 建好；第二次用同样的尺寸与标志再写一遍，在 x86-64 上可能只是重复写同一个值，但在 riscv64 上，"同一个 VPN[2] 槽位写两次"意味着第二次的叶尺寸可能与第一次不同（1GiB 与 2MiB 梯度的差别）——一条 1GiB 叶与一条"指向下级页表"的条目互斥，先写的那一条会被覆盖，恒等映射覆盖范围随之改变，跳转到内核高地址之后启动信息里记录的 `kern_phys_base` 仍然可用而镜像的一部分不再可访问。

### 4.7 分配器换手的落地（→3.7）

三处写者与各自的自守卫在 3.7 已列出。这里补上"游标"的两种恢复方式与扣除表的读法。

同一份 `BootAlloc` 有三种进入方式：boot-shim 侧的 `init`、跨镜像接收侧的 `resume`、以及内核兜底路径的 `init`。前两种的代码与语义：

```rust
// 节选（保留签名与两行注释；注释有改述，函数体以省略号代替），os/kernel/src/boot_alloc.rs。
    pub fn init(&self, base: u64, end: u64) {
        self.base.store(base, Ordering::Relaxed);
        self.next.store(base, Ordering::Relaxed);
        self.end.store(end, Ordering::Relaxed);
    }

    /// Resume a bump allocator whose region was already partly consumed by a
    /// previous stage (cross-image handoff).
    ///
    /// The boot-shim builds the bootstrap page tables from this bump region and
    /// then hands off to the high-half kernel image, which links its *own* copy
    /// of this allocator (separate `.bss`). Without resuming the cursor, the
    /// kernel image would re-hand-out pages the boot-shim's page tables (and the
    /// handoff blob) already occupy. `next` is the boot-shim's post-build cursor;
    /// the caller guarantees `base <= next <= end`.
    pub fn resume(&self, base: u64, next: u64, end: u64) { ... }
```

- `init(base, end)` 把游标置回 `base`：两个调用者都用它——boot-shim 在 `prepare_boot` 之后、进入内核之前设定区间；内核在 Step 0 发现 `boot_alloc_region()` 为 `None` 时用它从 `memmap` 里切一段兜底区间（`FALLBACK_BUMP_LEN = 0x100_000`，256 页）。
- `resume(base, next, end)` 从"已被消耗到的位置"继续：跨镜像路径必须用这一种，因为接收侧链接的是这份分配器的另一份副本（`resume` 的文档注释写明了这一点）。

三种方式的次序约束：跨镜像路径上必须先 `resume_boot_pt_alloc`、再进 `arch_boot_resume_high_half`。顺序反了的话，`arch_boot_impl` 的 Step 0 会看到"未注册"而走兜底 `init`，把游标重置回某段 memmap 的起点——boot-shim 已经发出的页表页与交接载荷页会被再发一次。

扣除表的读法：`build_vm_handoff` 把"内核保留"与"引导已用"两类区间收进同一份记录（见 3.7 引用的表），其中 bump 区是**整段**；`VmBootAllocator` 的已用前缀（表里第二类）是同段内的另一笔。两份记录随交接页一起交给 VM，VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`——也就是说，"内核记账对不对"这件事不是靠 kernel 单方面声明，而是在 VM 侧再复核一次。

**如果反过来想**：去掉内核侧 `if !pt_alloc::is_registered()` 的守卫，让 `boot_validate_and_prepare` 无条件 `register(boot_alloc::boot_pt_alloc)`。`register` 里的 `debug_assert!( !PT_REGISTERED.load(...) )` 只在 debug 构建生效；release 构建下第二处 `register` 会直接改写函数指针。若第一个注册者是某个测试内核（它用了一个避开内核镜像的池），改写之后所有页表页都从 `boot_alloc` 的兜底区间发出——那段区间由 `memmap` 的第一个满足条件的区域切出 `0x100_000` 字节，而 `boot_alloc_used_bytes()` 统计的是修改后的实例，于是 VM 交接记录里的 `kernel_allocated_dynamic` 与真实用量不一致，多出来的池子不在任何人的账上。

### 4.8 跨镜像交接与直核自引导（→3.9）

三个架构的交接形态并列：

```rust
// 逐字引用，os/boot-shim/src/main.rs。
    #[cfg(target_arch = "aarch64")]
    minix_kernel::bootstrap_to_kernel_image(&result.kernel_info, result.root_page);
    #[cfg(not(target_arch = "aarch64"))]
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);
```

```rust
// 节选（保留分道与恢复段；注释改述为机制陈述，输出字符串略去过程标记后缀），os/kernel-image/src/main.rs:fn rust_image_main（aarch64）。
#[cfg(target_arch = "aarch64")]
fn rust_image_main(handoff: *const minix_boot::BootHandoff) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    // 两条链的分道：直核 `-kernel`（QEMU 不设参，x0=0）走自引导面；
    // boot-shim UEFI 链 x0=BootHandoff 物理指针走高半 resume 面。
    #[cfg(target_arch = "aarch64")]
    if handoff.is_null() {
        bootface_a64::bootface_a64();
    }
    early_console::use_high_mmio_window();
    if handoff.is_null() {
        early_console::write_str("### minix-rs kernel-image: handoff blob NULL — aborting\n");
        return halt();
    }
    let h = unsafe { &*handoff };
    minix_kernel::boot_alloc::resume_boot_pt_alloc(h.bump_base, h.bump_next, h.bump_end);
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (additive)\n",
    );
    minix_kernel::arch_boot_resume_high_half(&h.kernel_info, PhysBytes(h.root_page))
}
```

```rust
// 逐字引用（保留入口的三行与调用），os/kernel-image/src/main.rs:fn rust_image_main（riscv64）。
#[cfg(target_arch = "riscv64")]
fn rust_image_main(boot_hart: u64, dtb_phys: u64, boot_file_table_pa: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys, boot_file_table_pa)
}
```

| 架构 | 谁建根表 | 谁开分页 | 交接值 | 内核入口 |
|---|---|---|---|---|
| x86_64 | boot-shim（内联内核） | boot-shim（`arch_boot` → `enable`） | `&KernelInfo` + 根页物理地址（寄存器/参数） | 进程内 `arch_boot` |
| aarch64 | boot-shim | boot-shim（`bootstrap_to_kernel_image` 内的 `enable`） | `BootHandoff` 物理指针（`x0`） | 独立镜像高半 `_start` |
| riscv64 | 内核镜像自身（`bootface`） | 内核镜像自身（`arch_boot` → `enable`） | `a0`=hartid、`a1`=DTB、`a2`=`BootFileTable`（可选） | 独立镜像 `_start` |

三条链收敛在同一个 `arch_boot`/`arch_boot_impl`（跨镜像路径收敛在 `arch_boot_resume_high_half`，它调用同一个 Step 4 与同一个 `jump_to_kmain`）。

`bootstrap_to_kernel_image` 里那三条跳转前的准备（分配载荷页、读游标、写载荷）与 `br {entry}` 的写法在 3.9 已给出。这里补一条与"谁建根"有关的约束：跨镜像路径下，`build_bootstrap_root_and_enable` 被 boot-shim 调用一次、生成的表**已经激活**；接收侧必须避免二次建表（3.9 的反向推演已给后果），而 `bootface` 路径下建表发生在内核镜像自身，`arch_boot` 走的是常规 Step 1–4。

两处实现现状：

- x86_64 的独立镜像入口（`os/kernel-image/src/main.rs` 的 `rust_image_main`）当前仍是"打印一行提示后驻留"的形态——把内核链接成独立 ELF 的产出与布局校验已经存在（`check-layout.sh` 的 L0–L9），而 boot-shim → 该镜像的跳转协议按镜像头注释的说明属于另一轮裁决范围。当前 x86_64 的生产路径是内联执行。
- riscv64 在 QEMU `-kernel` 直核场景下镜像按物理基址视图执行（源码注释把这一实测事实记作 `medany`），此时 `kern_virt` 与 `kern_phys` 可能相等，§4.6 的 Step 2 守卫就是为这种情况写的；生产链（`bootface` + 文件系统 + 高半链接）走的是另一套布局。两条布局的基址常量以链接脚本为准，复算命令：`grep -n 'KERNEL_VIRT_BASE\|KERNEL_PHYS_BASE\|ENTRY' os/kernel-image/*.ld`（该命令对三份脚本各命中若干行，这里只摘 riscv64 的三个值： `KERNEL_VIRT_BASE = 0xFFFFFFC000000000`、`KERNEL_PHYS_BASE = 0x80200000`、`ENTRY(_start)`）。

**如果反过来想**：让 riscv64 照 aarch64 那样"先建表、再跳镜像"。`-kernel` 直核场景下 QEMU 不设置任何入参寄存器（`x0 == 0`），镜像拿到的"载荷指针"是空——`rust_image_main` 的分道会走自引导面（`handoff.is_null()` 为真），而不是走 resume 面；若镜像里没有这条分道，一次空指针解引用会发生在 `early_console` 初始化之前，现场没有任何输出。反过来，若 aarch64 退回内联执行，内核代码住在会被 `switch_address_space` 替换的 TTBR0 低半，第一次地址空间切换就把自己的取指映射一起作废——`handoff.rs` 的模块注释记录的现场（`elr`/`vbar` 钉在同一个低地址）就是这个后果。

### 4.9 与 Minix3 C 的函数对照（→3.1–3.8）

| C 函数（第 2 章） | Rust 落点 | 保留 / 改型 / 淘汰 / 新增 | 差异原因一句话 |
|---|---|---|---|
| `minix3/minix/kernel/arch/i386/head.S:multiboot_init` | `os/boot-shim/src/main.rs:fn main` | 改型 | 引导器跳转到内核入口 → 固件跳转到 boot-shim 入口 |
| `pre_init(magic, ebx)` | `os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs:impl BootShim` | 改型 | 六条语句被拆成两条路径的 `prepare_boot` |
| `get_parameters(ebx, cbi)` | `os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs:fn prepare_boot` + `build_memmaps`/`build_kernel_info` | 改型 | 拷一份引导器结构体 → 调固件接口拿两份内存列表 |
| `mb_set_param(...)` | `os/libs/minix-boot/src/kernel_info.rs:param_buf`（空切片 + 校验上限） | 改型 | 参数表暂未接线；字段与 1024 上限保留 |
| `add_memmap(cbi, addr, len)` | `os/boot-shim/src/uefi_helpers.rs:fn build_memmaps` + `os/kernel/src/memmap.rs` | 改型 | 构建侧改为一次快照分流；内核侧仍按区间记账 |
| `cut_memmap(cbi, start, end)` | `os/kernel/src/memmap.rs:fn cut_memmap`（由 `kmain` A.2 调） | 改型 | 引导侧报告全部 DRAM，切除移到内核侧 |
| `alloc_lowest(cbi, len)` | 无对应物 | 淘汰 | 唯一的消费者是 32 位 AP 启动跳板（SMP 归 16 篇） |
| `overlaps(mod, n, cmp_mod)` | `os/boot-shim/src/loader.rs:fn first_overlapping_pair` | 改型 | 检查对象从"模块 + 内核"变成"内核镜像自身的 PT_LOAD 段" |
| `pg_clear()` | `os/arch/src/x86_64/paging.rs:X86_64Paging::new_from_page` 的清零 | 改型 | 根页由调用者提供 |
| `pg_identity(&kinfo)` | `os/kernel/src/lib.rs:build_bootstrap_root_and_enable` Step 1 | 改型 | 范围与页尺寸由常量与阶梯决定 |
| `pg_mapkernel()` | 同函数 Step 2 | 改型 | 返回值不再被使用 |
| `pg_load()` | `os/arch/src/x86_64/paging.rs:X86_64Paging::enable` 的 `mov cr3` | 保留（同一动作） | 硬件动作不变 |
| `vm_enable_paging()` | 同函数 `enable` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 改型 | 多了 NXE；少了 `PSE` |
| `pg_alloc_page(cbi)` | `os/kernel/src/boot_alloc.rs:BootAlloc::alloc` | 改型 | 从记录尾部取页 → 从注册的区间取页 |
| `alloc_pagetable(&ph)` | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page` + 三架构实现 | 改型 | 6 页静态池 → 注册式分配器 |
| `pg_map(phys, vaddr, vaddr_end, cbi)` | `os/arch/src/x86_64/paging.rs:X86_64Paging::map`（4KB 粒度） | 保留（语义） | 4KB 细粒度映射的用途不变 |

这张表里"淘汰"一行也不是"因为不需要所以删掉"：`alloc_lowest` 的服务对象（把 16 位启动代码放到 1MB 以下）属于 AP 启动，而 64 位侧的对应机制（AP 停车邮箱 + 启动记录）在 `os/kernel/src/smp.rs` 与 `os/kernel-image/src/main.rs` 的 `park` 段里，不经过"从记录最低处分配"这条路径。

**如果反过来想**：把 C 的静态页表池原样搬到 64 位（保留"6 页，用完 panic"）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 一次最多挂一个页表页，6 页够用。4 级页表下一次 4KB 映射要穿过 PML4 → PDPT → PD 三级，每级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页会在第一次跨 PML4 边界的映射上耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

---

## 5. 测试要点

### 5.1 宿主上能证明什么（能证明 / 不能证明）

宿主测试跑的是纯逻辑与接口契约，真实启动链要 QEMU。这两层的分界线可以逐条列出。

可复制命令与实测输出关键行（下面两段是实际的命令与输出，预期输出用 `test result:` 行核对）：

```bash
$ cd os && cargo test -p boot-shim --features test-all --lib
...
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cd os && cargo test -p minix-boot -p minix-elf
running 17 tests
test result: ok. 17 passed; 0 failed; ...
running 23 tests
test result: ok. 23 passed; 0 failed; ...
```

测试数口径：本节所有数字按"`#[test]` 出现次数"计，含 `#[should_panic]` 标注的条目，不含构造辅助函数与 mock 类型。逐文件复算：

```bash
$ for f in os/boot-shim/src/loader.rs os/boot-shim/src/opensbi_helpers.rs \
           os/boot-shim/src/uefi_helpers.rs os/libs/minix-boot/src/kernel_info.rs \
           os/libs/minix-elf/src/lib.rs; do printf '%-50s %s\n' "$f" "$(grep -c '#\[test\]' $f)"; done
os/boot-shim/src/loader.rs                         13
os/boot-shim/src/opensbi_helpers.rs                13
os/boot-shim/src/uefi_helpers.rs                   1
os/libs/minix-boot/src/kernel_info.rs              9
os/libs/minix-elf/src/lib.rs                       23
```

13 + 13 + 1 = 27，与 `cargo test -p boot-shim` 的实测一致；`minix-boot` 的 17 = `kernel_info.rs` 的 9 + `platform.rs` 的 8（`grep -c '#\[test\]' os/libs/minix-boot/src/platform.rs` → 8）。

每条测试挡的是哪条契约：

| 测试（文件名:函数名） | 挡的契约 |
|---|---|
| `loader.rs:test_load_boot_modules_loads_all_in_contract_order` | 12 项按 `MODULE_NAMES` 顺序返回（顺序契约） |
| `loader.rs:test_load_boot_modules_panics_on_missing` | 缺件必须 panic 而不是跳过（用 `catch_unwind` 观测） |
| `loader.rs:test_first_overlapping_pair_{disjoint,overlap,abutting,empty,returns_first}` | 半开区间的重叠判定（`abutting` 那一条正是 2.5 的 `-1` 语义） |
| `loader.rs:test_bss_region_is_zeroed_after_load` | 段余量清零到 `memsz`、且不越过 `memsz` |
| `loader.rs:test_load_kernel_with_loader_computes_layout` | 布局派生（`min(vaddr)`、`min(paddr)`、`span`） |
| `opensbi_helpers.rs:test_bump_alloc_rejects_zero_pages` | `bump_alloc(0)` 必须拒绝（否则返回重复地址） |
| `opensbi_helpers.rs:test_bump_alloc_advances_pointer` | 连续分配严格递增 |
| `opensbi_helpers.rs:test_boot_file_table_validates_magic` | 魔数不匹配即拒（防 `a0` 指错） |
| `opensbi_helpers.rs:test_build_kernel_info_bootstrap_zero_means_no_reclaim` | `bootstrap_start/len` 保持 `(0, 0)`，且 `len != kern_phys_base` |
| `opensbi_helpers.rs:test_build_kernel_info_riscv64_fields_match_input` | 字段逐一映射（含派生字段 `kern_stack_top`/`syscall_entry`） |
| `uefi_helpers.rs:test_build_kernel_info_fields` | UEFI builder 的字段装配（该文件唯一的单元测试） |
| `kernel_info.rs:test_validate_rejects_*`（6 条）与 `test_validate_accepts_*`（2 条），另加 `test_getters_return_field_values`（1 条）= 该文件 9 条 | `validate()` 的六条不变量与它们的合法边界；getter 与字段一致 |

一处覆盖缺口：`boot-shim` 的 27 个测试里只有 1 个覆盖 UEFI 路径（`test_build_kernel_info_fields`），`build_memmaps`、`alloc_root_page`、`alloc_bump_region`、`assert_bootstrap_outside_memmap` 这些 UEFI 侧函数没有宿主覆盖——它们的调用点是单根（`efi_main` → Boot Services），在宿主上无法构造。这条缺口只能由真机链补（见 5.3）。

**如果反过来想**：把 `load_boot_modules_with_loader` 的缺件处理从 `panic!` 改回 `if let Some(data) = loader.read(&path) { ... }`（静默跳过）。`test_load_boot_modules_panics_on_missing` 立刻变红——这条测试的可观测形式正是 panic；而如果连测试也一起删掉，错误会推迟到内核侧的数量断言 `debug_assert_eq!(n, NR_BOOT_PROCS, ...)`，那里只有"内核任务 + 启动模块必须填满 boot image 表"这一个笼统信息，缺哪个模块要靠人去数。

### 5.2 布局契约：镜像还没运行就能检查的部分

`check-layout.sh` 把"boot-shim 装载内核时要读的字段"在宿主上判定完。可复制命令与退出码：

```bash
$ bash os/kernel-image/check-layout.sh all      # 或 x86_64 / aarch64 / riscv64
$ echo $?
0     # 全部断言通过；1 = 任一断言失败；2 = 用法/环境错误
```

断言清单（脚本头部注释里逐条列出 L0–L9；L9 只在 riscv64 分支执行）：

| 断言 | 检查什么 | 防哪种漂移 |
|---|---|---|
| L0 | 脚本预期表里的两个基址 = 链接脚本里声明的两个基址 | 两处手写常量漂移（断言对象是"工具与真源的关系"） |
| L1 | ELF 类型 = `EXEC` | 内核装载面没有重定位处理器，`static-pie` 不可用 |
| L2 | `e_machine` 与预期三元组相符 | 拿错架构工件（用 x86 工件当 aarch64 用） |
| L3 | 首段 `PT_LOAD` 的 VMA/LMA = 该架构预期基址 | 布局常量与链接脚本不符 |
| L4 | 所有 `PT_LOAD` 的 `(vaddr - paddr)` 是同一个平移量 | "一个 `kern_virt_base ↔ kern_phys_base` 平移"的前提 |
| L5 | 镜像跨距 2MiB 对齐 | `boot_validate_and_prepare` 的大页阶梯至少能落进回退档 |
| L6 | 入口 = 链接脚本 `ENTRY(_start)`，且落在首段内 | 入口符号与段布局脱节 |
| L7 | 引导栈符号存在、自顶向下 64KiB、落在末段内 | 栈符号与镜像范围脱节 |
| L8 | `minix_kernel::arch_boot` 符号在镜像里 | `--gc-sections`/归档粒度把内核启动图裁出镜像 |
| L9（riscv64） | ELF 实测跨距 = `bootface.rs` 里的 `KERN_SIZE` | 镜像布局常量与自引导面的期望值漂移 |

L4 与 L5 分别对应 02 篇的段平移结论与 §3.6 的大页阶梯：L4 保证"两套地址之间只有一个偏移量"这件事对每个段都成立，L5 保证"选出的页尺寸"有一个可用的档位。L8 是"链接保留"的可观测证据——`KERNEL_ENTRY_ANCHOR` 持有 `arch_boot` 的函数指针，链接脚本 `KEEP` 该节，两者一起挡住裁剪。

**如果反过来想**：去掉 L5（跨距对齐断言）。一个跨距为 `2MiB + 4KiB` 的镜像在 `boot_validate_and_prepare` 里既进不了 1GiB 档也进不了 2MiB 档（`kern_size` 同时对两者都不整除——注意 L5 检查的是**跨距**，而阶梯检查的是 `kern_size`，两者的定义在链接布局里相连），落到 `panic!("... must be aligned to at least FALLBACK_HUGE_PAGE_SIZE")`。现场是"启动第一步就 panic"，根因在链接脚本的段间距；没有宿主断言就只能靠真机二分定位。

### 5.3 真机：三条链各自证明什么

| 链 | 脚本 | 命令要点 | 判定标记 |
|---|---|---|---|
| UEFI + ESP 盘（aarch64） | `os/qemu-tests/test-shim-bootmarks-aarch64.sh` | 制一张含 `BOOTAA64.EFI` 与 `/EFI/minix/kernel.elf` 的 FAT 盘，`qemu-system-aarch64` + AAVMF 启动 | `boot-shim: kernel loaded (entry staged)` 与 `boot-shim: 12 boot modules loaded` 两条都出现，且日志中没有 `panicked at` |
| OpenSBI + U-Boot（riscv64 生产链） | `os/qemu-tests/test-riscv64-uboot.sh` | U-Boot `fatload` + `bootelf` 装载 `boot.elf`，`mkimage` 生成 `boot.scr` | `### TEST_RESULT: PASS hello-boot-riscv64 ###` |
| `-kernel` 直核（riscv64 自引导面） | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` | 内核镜像自带引导体，从 DTB 解内存、从表/DTB 通道取模块 | 镜像自身的串口标记 |

退出码语义（两个脚本一致）：0 = PASS，1 = FAIL，2 = SKIP（前置工具缺失）。这条区分让"没跑"与"跑了但失败"在 CI 里可分辨：没有 U-Boot 的开发机上，riscv64 生产链返回 2 而不是 1。

脚本还顺带证明了模块数与路径：`test-shim-bootmarks-aarch64.sh` 在上盘之前会 `mdir` 检查 `/EFI/minix` 下有 `kernel.elf` 且 12 个模块名逐个存在，任一条缺失就在上盘阶段失败——也就是说"12 个模块文件必须在盘上"这件事在真机链的前置检查里被显式验证。

**如果反过来想**：把 SKIP 当作 FAIL 处理。没有 U-Boot 与 QEMU 的开发机上这条链长期红，随后人的处理方式是"忽略这条输出"；真正切断 `fatload` 的那次改动会淹没在常态红里，而这条链恰好是唯一能证明"U-Boot 预载 → `BootFileTable` → ELF 装载 → `KernelInfo` → `arch_boot`"整条 riscv64 生产链可用的判据。

---

## 6. 诊断

### 6.1 引导期的两条输出通道

`ExitBootServices` 之前与之后能用的输出手段不同，因此里程碑输出分两段。

```rust
// 节选（保留循环与 asm 两行；省略号处为 SAFETY 注释），os/boot-shim/src/lib.rs:fn raw_serial。
#[cfg(target_arch = "x86_64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        // ...
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
        }
    }
}
```
被省略的 SAFETY 注释给出这条写入的安全性依据：COM1 数据口是 QEMU/PC 平台的固定 I/O 资源，而引导准备阶段 boot-shim 是唯一写者（固件已经把控制权交出去、内核还没起来，没有并发写者），`out` 指令没有内存副作用。

`raw_serial` 与 `raw_serial_line` 的差别：后者在写之前等发送侧腾空（读 LSR bit5，有界轮询）并在 `\n` 前补 `\r`；前者不等、也不补。前者用于"已经知道自己在写什么、只想尽快留下痕迹"的场合（例如 EBS 之后的第一行），后者用于 panic 路径。

里程碑清单（每条给源码位置；这些字符串是排查时的定位依据）：

| 输出 | 阶段 | 位置 |
|---|---|---|
| `boot-shim: prepare_boot enter` | 进入固件相关准备 | `uefi_helpers.rs:prepare_boot` |
| `boot-shim: root+bump allocated` | 根页与 bump 区已分配 | 同上 |
| `boot-shim: loading kernel.elf from ESP…` | 开始读内核 | 同上 |
| `boot-shim: kernel loaded (entry staged)` | 内核段已拷贝 | 同上 |
| `boot-shim: 12 boot modules loaded` | 12 个模块已就位 | 同上 |
| `boot-shim: locating platform sources…` | 开始扫配置表 | 同上 |
| `boot-shim: memmap ok (final pre-EBS snapshot)` | 两份内存列表已产出 | 同上 |
| `boot-shim: building KernelInfo…` | 构造结构体 | 同上 |
| `boot-shim: exiting boot services…` | 即将退出 | 同上 |
| `boot-shim: [raw] boot services exited` | 已退出（裸串口） | `crate::raw_serial` 调用点 |
| `boot-shim: [raw] pt_alloc registered, entering kernel arch_boot` | 分配器已注册 | `main.rs:main` |
| `boot-shim: handing control to kernel arch_boot` | 跳转前最后一行固件输出 | `main.rs:main` |
| `kernel: arch_boot entered` | 内核侧第一行 | `os/kernel/src/lib.rs:arch_boot` |
| `kernel: step0 validate ok` / `step1+2 mappings ok` / `step4 DM coverage ok` | 建表逐步路标 | `os/kernel/src/lib.rs` 的 `boot_stage!` |

复算命令：`grep -n 'uefi::println!\|raw_serial' os/boot-shim/src/main.rs os/boot-shim/src/uefi_helpers.rs`。

**如果反过来想**：删掉 `main.rs` 里"handing control to kernel arch_boot"这一行。当 `arch_boot` 在内核打印 `kernel: arch_boot entered` 之前卡死时，串口上最后一行会是 `[raw] pt_alloc registered`——它无法区分"卡在 `arch_boot` 入口"（查 `KernelInfo` 的值）与"卡在注册之后、跳转之前"（查跳转与栈）。两处的排查方向完全不同，而这两行输出正是用来分开它们的。

### 6.2 panic 路径：从静默到可读

```rust
// 节选（保留缓冲写入与输出段；省略号处为注释），os/boot-shim/src/main.rs:fn panic。
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    struct StackWriter<'a> {
        buf: &'a mut [u8; 256],
        len: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.len < self.buf.len() {
                    self.buf[self.len] = b;
                    self.len += 1;
                } // 超出 256B 的尾部丢弃即可：诊断信息截断好于静默。
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 256];
    let mut writer = StackWriter { buf: &mut buf, len: 0 };
    let _ = core::fmt::Write::write_fmt(&mut writer, format_args!("{}", info));
    let written = writer.len;
    let message = core::str::from_utf8(&buf[..written])
        .unwrap_or("boot-shim panic: (non-utf8 message)");
    raw_serial_line("boot-shim panic: ");
    raw_serial_line(message);
    if !minix_types::run_panic_diagnostic_hook(message) {
        raw_serial_line("boot-shim panic: diagnostic hook not registered");
    }
    loop {
        core::hint::spin_loop();
    }
}
```

四处约束与理由：

- 用固定栈缓冲而不是 `format!`：堆在 EBS 之后不可用（UEFI 池分配器返回空）。`StackWriter` 只做逐字节搬运。
- 超出 256 字节丢弃尾部而不是截断格式化过程：注释写明"诊断信息截断好于静默"。
- 输出经 `raw_serial_line`（带 `\r` 补全与发送侧等待）：panic 消息只走裸串口，不碰任何可能已失效的固件通道。
- 最后尝试诊断钩子：内核在 `arch_boot` 入口注册它（`register_panic_diagnostic`），钩子把同一条消息按内核格式再打一遍（带 CPU 号与回溯）。钩子未注册时打印一行兜底信息——它本身就是一个诊断信号：说明 panic 发生在内核把它注册之前。

**如果反过来想**：在 panic handler 里调用 `format!` 或任何一个固件服务。EBS 之后堆与 Boot Services 都已失效，第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生；串口上得到的是"panic 处理自身的崩溃"或什么都没有，而原始 panic 的消息与位置被彻底覆盖。"panic 路径上不能分配、不能调固件服务、只能纯 CPU 操作加裸串口"这条约束就是从这里来的。

### 6.3 典型失败模式与诊断信号

| 失败模式 | 触发点 | 现场信号 | 下一步确认 |
|---|---|---|---|
| 引导器指令次序错（`load_mods` 在 `multiboot` 之后） | `minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 的 `assert(mi_flags & HAS_MODS)` | 内核第一条断言处停止，串口无输出 | 核对 `boot.cfg.default` 的 `;` 分隔次序 |
| 参数压栈次序错（x86-32） | `minix3/minix/kernel/arch/i386/pre_init.c:pre_init` 的 `assert(magic == MULTIBOOT_INFO_MAGIC)` | 停止，无串口输出；报出的"magic"像地址 | 核对 `head.S` 两条 `push` 的次序 |
| 模块缺失 | `loader.rs:load_boot_modules_with_loader` | `boot-shim: required boot module 'X' not found at <路径>` | 核对盘上 12 个模块名（脚本的 `mdir` 前置检查会先报） |
| 内核 ELF 读不到 | `FileLoader::read_required` | `boot-shim: required file not found: /EFI/minix/kernel.elf` | 核对 ESP 的 `/EFI/minix/kernel.elf` 与固件是否挂载了该分区 |
| bump 区间耗尽 | `BootAlloc::alloc` 返回 `AllocationFailed` | `bootstrap_to_kernel_image: handoff blob page alloc failed` 或内核侧 `map_huge` 失败 | 复算 `prepare_boot(N)` 的页数预算与镜像规模 |
| `bootstrap_len` 非零 | `KernelInfo::validate` | `bootstrap_len must be 0 (Rust port has no unpaged section), got N` | 查 boot-shim 的 `build_kernel_info` 调用实参 |
| 快照位置错（自举页被报成可用） | `assert_bootstrap_outside_memmap` | `conventional memmap [a,b) overlaps bootstrap allocation [c,d)` | 核对快照调用点在两次分配之后 |
| 大页阶梯无档可用 | `boot_validate_and_prepare` | `arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE` | 跑 `check-layout.sh` 的 L3/L4/L5 |
| 页表切换后取指故障 | `Paging::enable` 之后第一条指令 | QEMU `-d int` 日志里的异常记录；`CR2`/`EPC` 指向当前指令 | 核对恒等映射范围是否覆盖该地址（§4.6） |
| `KernelInfo` 切片读回全零 | `vm_handoff::classify` 或后续消费者 | VM 侧"no free memory regions"类 panic | 核对 `store_kernel_info` 是否落了三块（§4.5） |

**如果反过来想**：把 `assert_bootstrap_outside_memmap` 降级为 `debug_assert`（release 构建下消失）。它挡的是"活的页表页与 bump 池被当作空闲内存交给 VM"这一类问题，而这类问题的发作要经过"VM 取帧 → 覆盖页表页 → 切换 CR3 → 取指失败"四步。断言在 EBS 之前运行（此时串口还在，消息里同时给出 conventional 区间与自举区间两个坐标），而故障在现场只剩取指错误——把这条断言降级，等于把一个可读的坐标信息换成一次无坐标的崩溃。

### 6.4 可复制命令

按"看输出 → 看异常 → 看页表走查"三层排列。每段都给出预期输出的关键行，以及关键行缺失时说明什么。

```bash
# 第一层：串口捕获（最常用）。用现成脚本，它会把串口写入日志文件并 grep 标记。
$ bash os/qemu-tests/test-shim-bootmarks-aarch64.sh      # aarch64 UEFI：两行 boot-shim 路标
$ bash os/qemu-tests/test-riscv64-uboot.sh              # riscv64 生产链：U-Boot + fatload + bootelf
# 关键行：日志里出现 `boot-shim: kernel loaded (entry staged)` /
#        `boot-shim: 12 boot modules loaded` / `### TEST_RESULT: PASS … ###`
# 若关键行缺失：先看该脚本打印的"出现在/未出现"两行，再看日志尾部 20 行。

# 裸 QEMU（需要自己准备固件与磁盘时）。x86_64 的 OVMF 路径与 aarch64 的 AAVMF 路径参数不同，
# 固件路径按发行版替换：
$ qemu-system-x86_64 -bios <OVMF_CODE.fd> -drive format=raw,file=disk.img -nographic -serial mon:stdio
$ qemu-system-aarch64 -machine virt -cpu cortex-a72 -nographic \
      -drive if=pflash,format=raw,file=<AAVMF_CODE.fd> -drive format=raw,file=disk.img
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel bios-or-image.elf

# 第二层：异常与页表走查（-d 追踪项逗号分隔；page 会放大日志量，只在定位时开）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf \
      -d int,page,cpu_reset -D qemu.log
# 关键行：qemu.log 里的异常记录形如 check_exception old_mode=… cause=1 (LOAD/INSN) epc=0x…
# 若关键行缺失：说明 CPU 没有产生异常，问题在输出通道或更早的停机（例如 halt 驻留）。

# 第三层：GDB 远程调试（-s 开 gdbstub，-S 暂停在第一条指令）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf -s -S
$ gdb-multiarch image.elf
(gdb) target remote :1234
(gdb) break rust_image_main        # 或 arch_boot / arch_boot_resume_high_half
(gdb) continue
(gdb) info registers
```

三层的分工：

- 第一层回答"哪一步没走到"：里程碑清单（6.1）把 boot-shim 的动作切成十几段，缺哪一段就知道卡在哪个区间。
- 第二层回答"卡的时候 CPU 在做什么"：`int` 给出异常类型与故障地址，`page` 给出页表遍历记录。`-d page` 的日志量与 `-d int` 不在一个数量级，只用于定位单次页表遍历。
- 第三层回答"值是什么"：在 `arch_boot` 或 `arch_boot_resume_high_half` 断下来，检查 `KernelInfo` 的字段与 `root_page` 是否合理（`bootstrap_len`、`kern_size`、`memmap` 切片长度）。

**如果反过来想**：只用第三层（GDB）而不开 `-d int`。页表切换后的故障是"取指即失败"：GDB 断在切换之后的第一条指令上时，故障已经发生，而"故障地址是哪个、是取指还是读"这类信息只有 QEMU 的异常记录才给。缺了第二层，现场只剩"停在了某处"，没有 `epc`/`CR2` 可对照页表范围。

---

## 7. 参见

- 前置与后续篇：`02-higher-half-kernel.md`（链接布局、ELF 段拷贝、成对跳转、`k_initial_stktop` 两条线索）、`03-kmain-cstart.md`（保护结构）、`04-platform-discovery.md`（DTB/RSDT 解析与描述符）、`05-clock-interrupt-init.md`（时钟与中断控制器）。
- C 一手锚点：`minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,mpx.S,memory.c,arch_smp.c}`、`minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`、`minix3/minix/include/minix/{param.h,const.h}`、`minix3/minix/include/arch/i386/include/vm.h`、`minix3/sys/arch/i386/include/multiboot.h`、`minix3/etc/boot.cfg.default`、`minix3/minix/kernel/table.c`、`minix3/minix/kernel/system/do_getinfo.c`、`minix3/minix/servers/{mib/kern.c,vm/pagetable.c}`。
- Rust 落点：`os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers}.rs`、`os/libs/minix-boot/src/{kernel_info,boot_shim,handoff,platform}.rs`、`os/kernel/src/{lib,boot_alloc,globals,memmap,vm_handoff,misc}.rs`、`os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`、`os/arch/src/x86_64/paging.rs`、`os/kernel-image/{check-layout.sh,src/main.rs,src/bootface.rs,src/bootface_a64.rs}`、`os/servers/vm/src/{vm_server.rs,alloc_page.rs}`、`os/libs/minix-types/src/types/boot.rs`。
- 构建与回归入口：`os/qemu-tests/README.md`、`os/qemu-tests/test-shim-bootmarks-aarch64.sh`、`os/qemu-tests/test-riscv64-uboot.sh`、`os/qemu-tests/test-kernel-image-riscv64-bootface.sh`、`os/kernel-image/check-layout.sh`。

