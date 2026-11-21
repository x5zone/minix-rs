# 02 · 高半区：链接布局的双地址、按装载地址摆放、成对跳转到高地址执行

> 归档：v3 原文（2026-11-21）。现行版本见 `../02-higher-half-kernel.md`；节号映射见 `rewrite-notes/MIGRATION.md`。

> 路径与标注约定（`[ARCH: …]` 等记号的解码处）全局只说一次：见 [README.md](../../README.md) 的「全局阅读约定」。


> **目标读者**：熟悉操作系统基本概念（特权级、分页、地址空间、链接与装载）与 C 语言的开发者；Rust 一侧读到 enum/match/trait/内联汇编大意即可。

> **前置知识**：Rust 生产路径的起点按腿不同（详见 §3.1 五腿分类表与 §4.4 路径图），C 侧也不同。C 侧 i386 自举从“分页未开”开始（§2.2 的 `vm_enable_paging` 正是开分页的那一步）。另需 ELF 的 `PT_LOAD` 段与 `p_paddr`/`vaddr` 的区别、链接脚本的 `AT()` 与位置计数器。**前置**：[01-boot-shim-bootstrap.md](../01-boot-shim-bootstrap.md) 的 §2.6（`pg_mapkernel` 的位表）、§3.7（交接形态）与 §5.2（`check-layout.sh` 的 L0–L9 断言清单——本篇四处结论依赖它）。

> **后续**：[03-kmain-cstart.md](../03-kmain-cstart.md)（保护结构与 `cstart` 三行调用）、[07 篇页表结构与接口](../../02-stage-vm/07-pagetable-struct.md)（页表多级结构与接口的完整定义——本篇 §4.1 只用到基址选择这一面）。

> **教学目标**：①同一个镜像的两套地址与"三件事必须对齐"（§1.1、§1.2）；②`kernel.lds` 的双地址常量、`AT()` 与两段式布局（§2.1、§2.2）；③`head.S` 一次跳转里的两种寻址与 `k_initial_stktop` 的落段（§2.3、§2.4）；④Rust 三种链接脚本形态的判据（§3.1、§4.1）；⑤ELF 解析器自建与"按装载地址摆放"（§3.2、§4.2）；⑥成对换栈跳转的 trait 契约与三条前置条件（§3.4、§4.3）；⑦镜像跨距与映射粒度的收口（§3.5、§4.1）；⑧从装载到 `kmain` 的完整路径（§4.4；C↔Rust 对照见 §4.5）；⑨测试与诊断（§5、§6）。

上一篇结束时，引导器已经把启动信息交给内核、页表也建好并开启了分页。剩下的动作是让 CPU 真正站到高地址上去执行内核主流程。这一篇把这段路拆成三件事：镜像的两个地址是链接期定下的、段体字节由装载者按其中的一个地址摆放、交接代码成对地换栈并跳到另一个地址。

---

## 1. 概述

### 1.1 同一个镜像的两套地址

x86-32 内核的链接脚本开头给出两个基址（物理 0x00400000、虚拟 0xF0400000）和两者之差（0xF0000000）；具体的脚本行、`AT()` 表达式与复算见 §2.1。同一份镜像因此有两套地址，但**只对带 `AT()` 的高半段成立**：链接器把高半段里的函数与全局变量的地址解析成 0xF0400000 一段的值，而这段字节在内存里位于 0x00400000 一段。低侧段（`.unpaged_*`，`head.o`/`pre_init.o`/`pg_utils.o` 等）不走 `AT()`，它们的符号值与字节都在 0x00400000 一段（判据见 §2.1 的脚本摘录）。

`head.S` 里有一条指令与一个调用可以看见这两套地址各自的用法（`call _C_LABEL(kmain)` 里的 `_C_LABEL` 是 Minix3 的汇编宏，给 C 符号加架构约定的前缀；ELF 构建下它是恒等宏、不添下划线，展开路径见 §2.4；整段的出处是 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 的尾部）：

```asm
/* 代码引用，minix3/minix/kernel/arch/i386/head.S:multiboot_init（pre_init 返回之后的换栈与跳转段）。 */
	mov	$k_initial_stktop, %esp
	push	$0	/* Terminate stack */
	push	%eax
        call    _C_LABEL(kmain)
```

`kmain` 是一个符号，它的值由链接脚本决定；`k_initial_stktop` 也是一个符号，它落在哪一侧要到 §2.4 才有判据——本节后文（§2.3 末）用到「写进 `%esp` 的是 0xF04… 一侧的值」时，那是**待 §2.4 判据确认后**的引用，不是本节已证的结论。这两条指令要成立，前提是"这段代码知道自己运行在哪个地址上"，而这件事在分页开启前后是两个问题。把三个参与者分开看：

```text
链接期（链接脚本说话）          装载期（装载器说话）              运行期（页表说话）
┌──────────────────────┐      ┌────────────────────────┐      ┌─────────────────────────┐
│ .text   符号值 0xF04…│      │ 把 .text 的字节写到     │      │ VA 0xF04…  ──┐          │
│ .data   符号值 0xF04…│ ───▶ │ 物理 0x004…（LMA）      │ ───▶ │              ├──▶ PA 0x004…
│ .bss    符号值 0xF04…│      │ 把 .bss 按 memsz 清零   │      │ VA 0x004…  ──┘  （同一块 DRAM）
└──────────────────────┘      └────────────────────────┘      └─────────────────────────┘
   决定"符号是什么值"             决定"字节放在哪里"               决定"CPU 从哪个地址取指/读写"
```

三列各有自己的规则，也各有自己的读者：链接脚本的地址由链接器写进指令与数据；段体落点由装载这个 ELF 的那段代码决定；运行期能访问哪些地址由页表决定。第 2 章先读 C 侧的三个决定点，第 3、4 章读 Rust 侧对应的三个决定点。

内核为什么要在高一侧，而不是和用户代码挤在低地址？`kernel.lds` 在 `_kern_vir_base` 那一行的行尾给了理由：`/* map kernel high for max. user vir space */`——内核占高半，低地址整段留给用户进程；`pre_init` 再把用户栈与用户数据往下挪一段，留出“越界就能撞上”的栈溢出间隙（`i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters)；两个取值定义在 [`kernel/const.h`](minix3/minix/kernel/const.h) 的 "User limits" 段，`_MINIX_MAGIC` 构建下用户数据顶是 `0xE0000000`，默认构建是 `0xF0000000`）。另一条依据在 `01-boot-shim-bootstrap.md` §2.6.4 的位表里已经出现过：`pg_mapkernel` 写内核映射时不置 `I386_VM_USER`（`i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel)），而 `pg_identity` 的恒等映射置了它——内核段对用户不可访问。

### 1.2 三件事必须对齐

| 对象 | 决定什么 | 不自洽时最先坏的地方 | 展开在 |
|---|---|---|---|
| 链接布局 | 每个符号的值、每个可加载段的两个地址 | 符号值指向一个没有映射的地址 | §2.1–§2.2、§3.1、§3.5、§4.1 |
| 装载摆放 | 段体字节落在哪个物理地址 | 拷贝的目标不存在，或与别人重叠 | §2.2、§3.2、§4.2 |
| 成对跳转 | CPU 从哪个地址开始取指、用哪个栈 | 取指或压栈落在已经失效的映射上 | §2.3–§2.4、§3.3–§3.4、§4.3–§4.4 |

三项里有一项在 C 里由内核之外的代码执行：段体摆放是装载器按 ELF 的 `p_paddr` 字段做的，内核只声明"我希望被放在哪里"（`AT()` 表达式就是在声明这件事）。射程声明：树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位（`minix3/sys/arch/x86/include/loadfile_machdep.h:40`），`p_paddr` 只对外部 multiboot 装载器（如 GRUB）成立；下文的 `p_paddr` 读法按外部装载器写。另外两项——链接布局与跳转——在两边都是内核自己的产物。

三项各自的失败形态落到具体对象上：符号值指向没有映射的地址时，后果是取指发生在某个虚拟地址而页表没有对应条目（`CR2` 记下那个地址）；拷贝目标不存在时，后果是那段物理内存里根本没有内核字节；取指或压栈落在失效映射上时，后果分别落在程序计数器与栈指针上。

### 1.3 本篇范围与可以安全跳过的内容

本篇覆盖"从镜像文件到高地址上的 `kmain`"这一段。

| 可以跳过的内容 | 归属 |
|---|---|
| 引导器契约（识别头、寄存器约定、启动信息结构体）、内存记账、模块装载的检查、页表建立与开启 | `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| 页表多级结构与页表接口的完整定义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md` |
| `kmain` 之后的三张保护结构表 | `03-kmain-cstart.md` |
| 平台描述符的解析 | `04-platform-discovery.md` |
| 时钟与中断控制器初始化 | `05-clock-interrupt-init.md` |
| GNU ld 的语法全貌、`AT()` 之外的链接脚本技巧 | 外部资料；本篇只按"谁读这个地址"讲用到的部分 |
| UEFI 与 OpenSBI 的规范细节 | 外部资料；本篇只提到它们各自把文件字节放在哪里 |

可以安全跳过的一条：链接脚本里 `AT()` 表达式之外还有大量与内存布局无关的语法（`PROVIDE`、`KEEP`、`/DISCARD/` 的完整语义、符号赋值与节的相对关系）。本篇只解释其中被当前脚本实际使用、且直接决定"符号值"或"装载位置"的那几条；其余的读到时不理解也不影响把这条链走通。

**编号约定**：正文里的 `I-<nn>` 是评审台账的发现号，`D-<nnn>` 是裁决号（正文常把同一条的发现号与裁决号并写，如 `I-48`/`D-084`），`D2-<nn>` 是设计批编号（`D2-01` 指把 `SegmentIter` 的表范围算式改成 checked 运算这一项，`D2-06` 指把解析错误与装载错误进一步分层这一项，`D2-09` 指删掉进程内结构的 `repr(C)` 标注这一项，`D2-13` 指临时打印的清理项），`R02-T<nn>` 是测试缺口编号（`R02-T01` C 侧段归属、`R02-T03` riscv64 PC 高半判据、`R02-T04` 生产入口 ABI 栈形，三条均已闭合），`NK4-A` 是一次寄存器覆写缺陷的案例编号（`kmain` 收到被覆写的指针，修复见 §4.4）。这些编号只用于回指台账，读正文时不必先查它们。

本篇反复出现"高地址"这个词，它在两处含义不同：一处是链接期算出的符号值（编译产物里的一个常量，不需要任何运行时设施就存在），另一处是运行期可以访问的地址（要页表把它映射到某块物理内存才成立）。区分这两者的一条判据：符号值在分页开启之前就已经确定；可访问性在分页开启之后才成立。

下一篇（`03-kmain-cstart.md`）从 `kmain` 的第一条指令开始，讲保护结构的建立。

---

## 2. C 源码分析

### 2.1 kernel.lds：双地址与 `AT()`

链接脚本的 `SECTIONS` 块从位置计数器（`.`）开始设置地址。x86-32 的做法是：先把位置计数器放到物理基址上，收完一批"分页开启前必须能执行"的段，加上偏移，再开始收普通段（`minix3/minix/kernel/arch/i386/kernel.lds`）：

```ld
/* 节选（先给三个基址常量定义行，再保留位置计数器起点与各段头部；省略号处为段体、别名与 .text 之后的 .data/.bss/_end），minix3/minix/kernel/arch/i386/kernel.lds。
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);
	. = _kern_phys_base;
	__k_unpaged__kern_unpaged_start = .;

	.unpaged_text : { unpaged_*.o(.text) }
	...
	__k_unpaged__kern_unpaged_end = .;

	. += _kern_offset;

	. = ALIGN(4096); usermapped_start = .;
	.usermapped_glo : AT(ADDR(.usermapped_glo) - _kern_offset) { *(.usermapped_glo) }
	. = ALIGN(4096); usermapped_nonglo_start = .;
	.usermapped : AT(ADDR(.usermapped) - _kern_offset) { *(.usermapped) }
	. = ALIGN(4096); usermapped_end = .;
	.text             : AT(ADDR(.text) - _kern_offset) { *(.text*) }
```

逐行解码：

- `. = _kern_phys_base;`：位置计数器从 0x00400000 开始。它决定此后每个段的默认虚拟地址。这一段位置计数器落在低地址一侧。
- `.unpaged_text : { unpaged_*.o(.text) }`：这段**没有** `AT()`。不给 `AT()` 的输出段，它的装载地址与虚拟地址相同——两个地址都是位置计数器当前的值，也就是 0x00400000 一段里的某个地址。
- `. += _kern_offset;`：位置计数器加上 0xF0000000。此后创建的段，虚拟地址落在 0xF04xxxxx 一侧。
- `.text : AT(ADDR(.text) - _kern_offset) { *(.text*) }`：这段**有** `AT()`。`ADDR(.text)` 是它的虚拟地址（位置计数器当前值，在高一侧），表达式把它减去 0xF0000000，得到装载地址。

输出段有两个地址，下文按链接器术语写成 VMA（virtual memory address，链接器解析符号时用的地址）与 LMA（load memory address，装载器写段体时用的地址）。`AT()` 的参数是一段表达式，它在链接期求值，结果写进 ELF 程序头表的 `p_paddr` 字段。三种写法得到的两套地址互不相同：

| 写法 | VMA（符号解析用） | LMA（写进 `p_paddr`） | 本脚本里的例子 |
|---|---|---|---|
| 不给 `AT()` | 位置计数器当前值 | 与 VMA 相同 | `.unpaged_text` |
| `AT(ADDR(sec) - _kern_offset)` | 位置计数器当前值 | VMA − 0xF0000000 | `.text`、`.data`、`.bss` |
| `AT(_kern_phys_base)` | 位置计数器当前值 | 恒为 0x00400000 | 反例，见本节末 |

`AT(ADDR(.text) - _kern_offset)` 不是"把装载地址设成物理基址"——它是"把装载地址设成该段虚拟地址减去同一个常量"。两者只在某个段的虚拟地址恰好是 0xF0400000 时才相等，而本脚本里没有这样的段：低侧 `.unpaged_*` 三段长度非零（对象清单与改名机制见 §2.2），位置计数器加完偏移后已经大于 0xF0400000；接着 `.usermapped_glo` 与 `.usermapped` 两个非空输出段又各占一段、各自对齐到 4KiB（供料方是 [`i386/usermapped_glo_ipc.S`](minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S) 供 `.usermapped_glo`；[`i386/usermapped_data_arch.c`](minix3/minix/kernel/arch/i386/usermapped_data_arch.c) 的三个 `minix_ipcvecs_*` 对象与 [`kernel/usermapped_data.c`](minix3/minix/kernel/usermapped_data.c) 的 8 个对象都带 `__section(".usermapped")`，一起供 `.usermapped`）。`.text` 的虚拟地址因此严格大于 0xF0400000，它的装载地址也严格大于 0x00400000。真正成立的是逐段关系：每个带 `AT()` 的段，装载地址 = 自己的虚拟地址 − 0xF0000000；五处 `AT()` 减的是同一个常量，于是"虚拟地址 − 装载地址"对**每个带 `AT()` 的段**都相等；低侧 `.unpaged_*` 三段不带 `AT()`、平移量为 0——两组各自一致，不是全表一致。这条平移一致是后面一切映射的基础，`kernel.lds` 自己并没有断言它；Rust 侧把这条件写成了可执行检查（§3.1、§5.3）。

**把五处 `AT()` 都写成 `AT(_kern_phys_base)` 会怎样**：五个带 `AT()` 的输出段（`.usermapped_glo`、`.usermapped`、`.text`、`.data`、`.bss`）的装载地址都会是 0x00400000。装载器按每段的 `p_paddr` 与 `p_filesz` 逐个写字节，于是后写的段覆盖先写的段（`.bss` 是 `.data` 所在 RW 段的 NOBITS 尾段，不写文件字节；清零从 `.data` 文件字节之后开始，若 `.bss` 独立成段才从段首 0x00400000 起）。现行 GNU ld 对 LMA 重叠报错并拒绝产出 ELF；旧工具链若不检查 LMA 重叠，才会出现后写段覆盖前写段的镜像。症状是内核镜像里先写的那些段的字节被后写的段覆盖，`kmain` 的入口与 `minix_ipcvecs_*` 表的内容都落在被覆盖的字节上：取到的第一条"指令"是别人的数据。

**只删掉 `AT()`、保留高地址的 VMA 会怎样（第二处）**：此时每段的 LMA 退化成 VMA，装载器会向 0xF04xxxxx 一段写段体。这一段落远高于引导器报告的内存上界（`kinfo.mem_high_phys`），通常没有 DRAM 背书，写与读落到未分配的物理地址上；这种行为由平台决定（QEMU 丢弃写、读回 0，真机上可能命中 MMIO）。装载动作本身不报错（ELF 里 `p_paddr` 是合法数字），后果推迟到执行时——高地址窗口下的取指读到的是未被写入的内容（若读到 0，0x00 0x00 在 32 位下解码为 `add [eax], al`），CPU 在第一条指令上就偏离。

### 2.2 两段式布局：unpaged 段与高半段

前一小节里 `.unpaged_text` 收的是 `unpaged_*.o`，`.text` 收的是所有输入文件（`*(.text*)`）。同一个函数体落进哪一边，取决于它所在的**对象文件名**。这个名字是构建规则产生的（`minix3/minix/kernel/arch/i386/Makefile.inc`）：

```make
/* 代码引用，minix3/minix/kernel/arch/i386/Makefile.inc（__k_unpaged_ 前缀的说明与其后的 .for unpaged_obj in 块）。 */
# some object files we give a symbol prefix (or namespace) of __k_unpaged_
# that must live in their own unique namespace.
#
.for unpaged_obj in head.o pre_init.o direct_tty_utils.o \
	pg_utils.o klib.o utility.o arch_reset.o \
	io_inb.o io_outb.o \
	${MINLIB_OBJS_UNPAGED} ${MINC_OBJS_UNPAGED} ${SYS_OBJS_UNPAGED}
```

```make
/* 节选（`.for` 循环体的 `.else` 分支；`USE_BITCODE=yes` 分支另有三条命令，先转 bitcode 再改名），minix3/minix/kernel/arch/i386/Makefile.inc。 */
unpaged_${unpaged_obj}: ${unpaged_obj}
	${OBJCOPY} --prefix-symbols=__k_unpaged_ ${.OBJDIR}/${unpaged_obj} $@
```

逐段解码：

- `.for unpaged_obj in A B C …` 是 BSD make 的循环：对名单里每个对象，生成一条改名规则。规则的名字（`unpaged_${unpaged_obj}`）就是**输出文件名**：`head.o` 变成 `unpaged_head.o`。
- 规则体只有一条命令：`objcopy --prefix-symbols=__k_unpaged_`。它把输入对象里**所有符号**加前缀（定义与引用都加——`head.o` 里的 `MINIX`、`pre_init` 变成 `__k_unpaged_MINIX`、`__k_unpaged_pre_init`；`call kmain` 的引用同样变成 `__k_unpaged_kmain`），再写出到目标文件。前缀命名的两半合起来才闭合：`kmain` 的**定义**在 `main.c` 里由 [`kernel/proto.h`](minix3/minix/kernel/proto.h) 的宏改名（`#ifndef UNPAGED` 下 `#define kmain __k_unpaged_kmain`），与 objcopy 改过的引用同名；`head.S` 的 `call _C_LABEL(kmain)` 里 `_C_LABEL` 在 ELF 构建下是恒等宏（`sys/arch/i386/include/asm.h`），前缀完全由 objcopy 那一侧承担。
- 名单里显式列了 9 个对象（`head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o arch_reset.o io_inb.o io_outb.o`），再从三个库的未分页清单里各取一批：`${MINLIB_OBJS_UNPAGED}` 3 个、`${MINC_OBJS_UNPAGED}` 13 个、`${SYS_OBJS_UNPAGED}` 2 个。合计 27 个对象被改名。复算：`awk '/^MINC_OBJS_UNPAGED=/{f=1} f{print} f&&/udivdi3\.o/{exit}' minix3/minix/kernel/arch/i386/Makefile.inc | grep -oE '[A-Za-z0-9_]+\.o' | wc -l` 输出 13；另两条单行清单用 `grep -E '^MINLIB_OBJS_UNPAGED=' minix3/minix/kernel/arch/i386/Makefile.inc | grep -oE '[A-Za-z0-9_]+\.o' | wc -l` 与把名字换成 `SYS_OBJS_UNPAGED` 的同一条命令，分别输出 3 与 2（注意只取 `=` 那一行：清单之后紧跟同名目标规则行，把范围放大到文件其余部分会把这些行也算进来）；显式列出的 9 个就在 `.for` 块的第一行与续行里（`head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o arch_reset.o io_inb.o io_outb.o`）。

名单里的库对象（`klib.o`、`io_inb.o`、`assert.o` 等）通常还会以普通对象的形式被链接一次；`unpaged_` 副本只服务分页开启前的调用，普通副本留在高半供运行期使用。`head.o`/`pre_init.o`/`pg_utils.o` 这类对象在普通清单里也有一份（或只以 `unpaged_` 形式入链），判断某次调用落到哪一份，看调用点引用的是哪个符号名。名单只决定哪一份进低侧，不意味着源文件只编译一次。

`mpx.S` 在这个名单里没有出现。它的对象（`mpx.o`）由另一条规则产出：`mpx.S` 只出现在同一个文件的普通源码清单里（`SRCS+= mpx.S arch_clock.c …` 一行），没有经过 `objcopy`。因此它的输出文件名是 `mpx.o`，链接脚本里 `unpaged_*.o(.text)` 这条通配匹配不到它——通配匹配的是**输入文件名**（`objcopy` 从不改段名，它只给符号加前缀），而 `mpx.o` 的文件名里没有 `unpaged_` 前缀。它落进普通的 `.text`/`.data`/`.bss`，也就是 §2.1 里位置计数器加过偏移之后的段。这一点在 §2.4 会被用到。

把"名字"这件事拆开看，这里有**两套互不相同**的机制，写法相似但作用对象不同：

| 机制 | 作用对象 | 产物 | 谁消费它 |
|---|---|---|---|
| 输出文件名前缀 `unpaged_` | 对象文件本身 | `unpaged_head.o` 等 27 个文件 | 链接脚本的输入文件通配 `unpaged_*.o` |
| 符号前缀 `__k_unpaged_` | 对象里的符号表 | `__k_unpaged_pre_init` 等符号 | `ENTRY(__k_unpaged_MINIX)`、以及被改名对象之间的相互引用 |

第三张表把几个代表性对象与它们的落点对上（本节与 §2.4 共用）：

| 对象 | 在改名名单里 | 段收在哪 | 执行时地址在哪一侧 |
|---|---|---|---|
| `head.o` | 是 | `.unpaged_text`/`.unpaged_data`/`.unpaged_bss` | 低一侧（VMA = LMA） |
| `pre_init.o` | 是 | `.unpaged_text` | 低一侧 |
| `pg_utils.o` | 是 | `.unpaged_text` | 低一侧 |
| `mpx.o` | **否** | `.text`/`.data`/`.bss` | 高一侧 |
| `memory.c`、`protect.c` 等 | 否 | `.text` | 高一侧 |
| `usermapped_data_arch.o` 等 | 否 | `.usermapped*` | 高一侧（同样带 `AT()`） |

这张表里"名单"是判据，段是结果；`mpx.o` 那一行是后一小节的入口。

链接脚本还给同样三个常量各写了两份名字：

```ld
/* 代码引用，minix3/minix/kernel/arch/i386/kernel.lds（三个常量的第二份名字）。 */
__k_unpaged__kern_offset = _kern_offset;
__k_unpaged__kern_vir_base = _kern_vir_base;
__k_unpaged__kern_phys_base = _kern_phys_base;
```

两套名字对两类读者：被改名的对象引用的是带前缀的那份（`pre_init.c` 里的 `extern char _kern_vir_base` 经 objcopy 变成 `__k_unpaged__kern_vir_base`），没有改名的对象引用不带前缀的那份（[`i386/protect.c`](minix3/minix/kernel/arch/i386/protect.c) 里 `extern char _kern_vir_base, _kern_phys_base; /* in kernel.lds */`）。两类都取到同一个值。C 代码读这些值的方式是取符号的地址：`static phys_bytes kern_vir_start = (phys_bytes) &_kern_vir_base;`（[`i386/pg_utils.c`](minix3/minix/kernel/arch/i386/pg_utils.c) 顶部的三行静态量），随后 `pg_mapkernel` 用它算出第一个页目录下标。`&symbol` 在这里读的是链接脚本给这个符号赋的**值**，不是某个变量的地址。

`i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) 里那句下标计算就是把内核高半基址除以大页尺寸（复算见 §6.2 的两条 `printf`）：

```c
/* 节选（保留函数体的头与尾，缩进照源；省略号处为循环体。两行 4MB 对齐断言在节选起点之前，见 §3.5），minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel。 */
        pde = kern_vir_start / I386_BIG_PAGE_SIZE; /* start pde */
	while(mapped < kern_kernlen) {
		...
	}
	return pde;	/* free pde */
```

`kern_vir_start` 是 0xF0400000，`I386_BIG_PAGE_SIZE` 是 4MB，两者相除得 961（复算：`printf '%d\n' $((0xF0400000 / 0x400000))`）。这个数字在 01 篇讲页表时出现过；在这里它的来源是链接脚本的第一行常量。

ARM32 一侧是同一形状，逐字对照 `minix3/minix/kernel/arch/earm/kernel.lds`：三个常量的名字与用法相同，只有物理基址不同（`_kern_phys_base = 0x80200000`；注意 `0x80200000` 不是 4MiB 对齐，4MiB 对齐是 i386 侧 `pg_mapkernel` 的大页前提，ARM 侧的映射粒度与基址约束不同，不能把 i386 的判据直接套过来）；条目段多收一个 `.startup` 段（`.unpaged_text ALIGN(4096) : { unpaged_*.o(.startup); unpaged_*.o(.text) }`），因为 ARM 的入口标签写在那个段里；`.unpaged_data` 之后、`.unpaged_bss` 之前多一个 `__k_unpaged__kern_unpaged_edata`，`.unpaged_bss` 之后是 `__k_unpaged__kern_unpaged_end`。两边的改名名单也不同：ARM 侧没有 `io_inb.o`/`io_outb.o`，`MINLIB_OBJS_UNPAGED` 只保留 `get_bp.o`（没有 `_cpufeature.o`/`_cpuid.o`），另多若干 `compiler_rt` 除法辅助对象，并追加 `${BSP_OBJS_UNPAGED}`。

**把 `pg_utils.o` 从改名名单里去掉会怎样**：正面结论是链接期直接失败——`pre_init.o` 一侧的引用带 `__k_unpaged_` 前缀，普通 `pg_utils.o` 只有不带前缀的定义，于是 `__k_unpaged_pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging`（另有 `__k_unpaged_add_memmap`/`cut_memmap`）成为未定义符号。名单在 `.for unpaged_obj in …` 块里；函数在 `i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel)，调用点在 `i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的函数体中部，五个调用全在同一对象里。反方向不等价：`objcopy --prefix-symbols` 同时改定义名与引用名，引用与定义前缀不匹配时同样是链接期报未定义符号；所有引用一起改名时函数整体落低，需要在链接产物里核对符号值——这是可检查的错位，不是“低侧副本静默执行”。（`pre_init.o` 不能做同型替换：`pre_init.c` 不在任何 `SRCS` 里，去掉名单项会连唯一生成规则一起消失；要构造“函数落高半、分页前执行”的故障，必须同时把调用方也移出名单，是另一个更粗的错法。）所以本节只保留“该在低侧的对象被留在高侧”这一方向的推演。

### 2.3 head.S：一次跳转里的两种寻址

`pre_init` 返回之后，`head.S` 的剩余四行完成交接（`i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init)，注释一并保留）：

```asm
/* 代码引用，minix3/minix/kernel/arch/i386/head.S:multiboot_init（含 pre_init 返回之后的注释段）。 */
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

逐行解码：

1. `mov $k_initial_stktop, %esp`：把栈指针设到 `k_initial_stktop` 的值。`$符号` 形式的立即数在汇编后就是符号的**值**——链接期填进去的一个 32 位常量。这条指令携带的是绝对值，它与"当前 PC 在哪"无关。
2. `push $0`：往新栈里压一个 0。源码注释写作 `Terminate stack`——它是初始栈底的终止槽，与回溯链的终止点是两件事：本仓两侧的回溯器都沿**帧指针链**走（C 侧 `libsys/stacktrace.c` 的 [`util_stacktrace`](minix3/minix/lib/libsys/stacktrace.c:util_stacktrace) 从 `get_bp()` 出发 `while(bp)` 逐帧读 `[bp]` 与 `[bp+1]`；内核侧 `i386/exception.c` 的 [`proc_stacktrace_execute`](minix3/minix/kernel/arch/i386/exception.c:proc_stacktrace_execute) 与 Rust 侧 `stacktrace.rs` 的 [`walk_frames_from`](os/arch/src/arch/stacktrace.rs:walk_frames_from) 同形），正常情况下链走到保存帧指针槽里的 0 才停。C 侧入口段 `mov $0, %ebp` 在换栈前清零；`pre_init` 是普通 C 函数，EBP 是 callee-saved，`pre_init` 返回时把它恢复成 0，因此换栈后调用 `kmain` 时保存槽里同样是 0。Rust 侧把 `xor rbp, rbp` 放在换栈之后（§4.3），且生产 `kmain` 的序言不保存 `%rbp`（本仓未启用 `-C force-frame-pointers`），链从 `kmain` 内的回溯起点直接读到 0。两侧都在 0 处终止，差别只在清零发生的位置与是否存在保存槽。边界：清零只保证入口起点的终止；未启用帧指针时，Rust 侧不保证调用链深度可回溯（panic 路径能打出的帧数取决于当时的寄存器与手工回溯实现）。两侧回溯器另有 `saved_fp <= fp`（C 写作 `hbp <= bp`）的防环终止，只在链被破坏时先于 0 触发；压进栈的这个 0 槽不在帧指针链上，本仓两侧的回溯器都没有读这个槽的路径——它只作为初始栈的终止槽保留（Rust 侧保留它的理由与消费边界见 §4.3）。
3. `push %eax`：把 `pre_init` 的返回值（`kinfo` 的地址）压栈。i386 的调用约定从栈上取参数，所以这次压栈就是给即将被调用的 `kmain` 准备第一个参数。
4. `call _C_LABEL(kmain)`：跳转。`call` 在机器码里是一条相对调用：位移在链接期算好，运行时 CPU 用"当前 PC + 位移"得到目标。位移的值是链接器按**两个链接地址**之差算出来的（调用点在 `.unpaged_text` 里，目标 `kmain` 在 `.text` 里），两个链接地址一低一高，差值在 0xF0000000 量级。把补码这笔账算一遍：`call` 的位移定义为"目标地址 − 下一条指令地址"，即 `S − (P + 5)`。代入布局：高侧的位置计数器在低侧三段之后才加 `_kern_offset`（`kernel.lds` 的 `. += _kern_offset` 排在 `.unpaged_bss` 之后），所以 `kmain` 的虚拟地址是 `0xF0400000 + 低侧总长 + 高侧段内偏移`，而调用点在 `.unpaged_text` 里、地址是 `0x00400000 + 调用点偏移`。两者相减：位移 = `0xF0000000 + (低侧总长 + 高侧段内偏移 − 调用点偏移 − 5)`。低侧总长与高侧偏移的差值是一个小量，所以位移的位模式在 `0xF0000000` 附近；精确的低 16 位取决于链接后的段长与 `kmain` 偏移，可用 `nm` 复核，这里只需要量级。32 位有符号位移的可表示范围是 −2^31..2^31−1；`0xF0000000` 按补码读作 `0xF0000000 − 0x1_0000_0000` = `−0x1000_0000`（约 −268 MB），位模式仍在范围内。关键是 CPU 在 32 位模式下把位移加到 EIP 后按 2^32 取模回绕：负位移加到低地址 `0x0040xxxx` 上会回绕到 `0xF040xxxx`，所以高半与低半之间的距离能塞进一条 32 位相对调用，不需要额外指令。

于是这两条指令对"执行视图"的敏感度不同：第 1 条携带绝对值，无论 CPU 在哪个视图中执行，写进 `%esp` 的都是 0xF04xxxxx 一侧的值；第 4 条的相对位移要加上运行时的 PC 才成为目标地址，而调用点所在的 `.unpaged_text` 是 VMA = LMA 的段——它在链接时的地址就是装载后的地址，也就是 CPU 此刻真正执行的那个地址。位移加上这个 PC，落到 `kmain` 的链接地址上。两条路径殊途同归，都停在 0xF04xxxxx 一侧（`k_initial_stktop` 是否在同一侧的判据见 §2.4）。

把两种寻址方式与它们各自的前提列成一张表：

| 指令 | 机器码里携带什么 | 目标/结果由什么决定 | 前提 |
|---|---|---|---|
| `mov $k_initial_stktop, %esp` | 符号值（32 位立即数） | 链接期填值 | 无（与执行地址无关） |
| `call kmain` | 相对位移 | 链接期算位移，运行期加 PC | 调用点的执行地址 = 它的链接地址 |

ARM32 侧的做法见 `earm/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/earm/head.S:multiboot_init)：

```asm
/* 代码引用，minix3/minix/kernel/arch/earm/head.S:multiboot_init（pre_init 返回之后的段）。 */
	/* Kernel is mapped high now and ready to go, with
	 * the boot info pointer returned by pre_init in r0.
	 * Set the highly mapped stack and initialize it.
	 *
	 * Afther that call kmain with r0 still pointing to boot info
	 */
	ldr	sp, =k_initial_stktop
	mov	r1, #0
	push	{r1}			/* Terminate stack */
	ldr	r2, =_C_LABEL(kmain)	/* r0 holds kinfo_t ptr */
	bx	r2
```

ARM 这里用 `ldr r2, =sym` 加 `bx r2`，不用 `bl kmain`。两个原因叠在一起：`bl` 的立即数范围是 ±32MB，而 `kmain` 与调用点相距 `0xF0400000 − 0x80200000 = 0x70200000`（注意这不是 x86-32 的 `0xF0000000`；ARM 的物理基址不同），远超范围；`ldr rX, =sym` 取的是符号的**值**（汇编器把常量放进字面量池，指令从池里读），`bx` 直接跳到这个值上。与 x86 的 `call` 相比，ARM 这条路径携带的是绝对值，不依赖"执行地址等于链接地址"这条前提。

两种架构的两个动作在这里分工相同：先设栈（绝对值），再跳转（x86 靠位移+PC，ARM 靠绝对值）。第 1 章图里"运行期（页表说话）"那一列不是这两条指令建立的；它由 `pre_init` 的五步建表在此之前完成——栈指针指向的高地址必须已经在页表里有映射。

**三条推演（按"缺哪一步"分：只切栈不跳；只跳不切；把切栈挪进 `kmain` 内部）**：

其一，只切栈不跳。`%esp` 是 0xF04xxxxx 一侧的值，而 PC 停在 `.unpaged_text` 的下一行（`head.S` 里 `pre_init` 之后的兜底循环：`hang: jmp hang`，就在 `call kmain` 的下面）。此刻恒等映射还在，取指正常，看起来"没坏"；但 `jmp hang` 使内核停在这个兜底循环，主流程不再前进，VM 服务不会运行——延迟页错误的后果不可达。直接症状是停机，不是页错误。

其二，只跳不切。`call` 会把返回地址压进当前栈——`load_stack`（`head.S` 自己 `.data` 段里的 4096 字节：`load_stack` 起、`load_stack_start` 止——设栈取的是后者）。`kmain` 的函数序言、参数保存、后续每次 `push`/`pop` 都发生在这个低地址栈上。恒等映射移除之后，第一次栈访问就落在没有映射的页上，`CR2` 是那个低地址栈页的地址。这一处与上一处的区别是：上一处是停在 hang 停机（内核不再前进），这一处坏在数据访问，而数据访问在中断发生时也会发生（中断处理要压栈）。

其三，把"切栈"挪到 `kmain` 内部作为第一步。`kmain` 是 C 函数，进入它之前编译器生成的序言已经动过栈（保存帧指针、调整栈指针），也就是在切栈之前至少已经有一次对旧栈的写。要让"切栈发生在任何压栈之前"，它就必须排在 `call` 之前——`head.S` 正是这么排的，设栈与 `call` 之间隔着两条 `push`——`push $0`（终止槽）与 `push %eax`（给 `kmain` 准备的参数）。

### 2.4 `k_initial_stktop` 落在哪一段

`head.S` 引用的 `k_initial_stktop` 定义在 `mpx.S` 里（[`i386/mpx.S`](minix3/minix/kernel/arch/i386/mpx.S) 末尾）：

```asm
/* 节选（保留段指示与栈定义，注释有删节；省略号处为同文件此前的代码与 k_boot_stktop 之后的 10 行），minix3/minix/kernel/arch/i386/mpx.S（文件尾部附近的段指示与栈定义）。 */
.data
.short	0x526F	/* this must be the first data entry (magic #) */

.bss
k_initial_stack:
.space	K_STACK_SIZE
LABEL(__k_unpaged_k_initial_stktop)

/*
 * the kernel stack
 */
k_boot_stack:
.space	K_STACK_SIZE	/* kernel stack */ /* FIXME use macro here */
LABEL(k_boot_stktop)
...
```

逐段解码：

- `.data` 起头那两行是另一个话题：`.short 0x526F` 是引导监视器读的魔术字；源码注释说它必须是 **第一个 data entry**（文件头注释讲的是 text 段常量放在文件开头，好让只读第一个扇区的监视器读到；两处说的不是同一个位置）。
- `.bss` 之后是初始栈：`k_initial_stack:` 是标签（栈的底），`.space K_STACK_SIZE` 预留一段空间，紧随其后的标签就是栈的顶（栈向低地址增长，标签落在预留区的高端）。`K_STACK_SIZE` 定义在 [`include/arch_proto.h`](minix3/minix/kernel/arch/i386/include/arch_proto.h)（`#define K_STACK_SIZE	I386_PAGE_SIZE`），i386 上是 `I386_PAGE_SIZE`，即 4096 字节（[`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h) 里 `#define I386_PAGE_SIZE 4096`）。
- 后面还有 `k_boot_stack`（同样 4096 字节，注释写作 `kernel stack`）与 `k_stacks_start`/`k_stacks_end`（每 CPU 两页的栈数组，`mpx.S` 的注释写作 one for data, other as a sandbox）；[`i386/protect.c`](minix3/minix/kernel/arch/i386/protect.c) 里 `tss_init(0, &k_boot_stktop)` 取的是 `k_boot_stack` 的顶。同一个 `mpx.o` 里的这几处栈定义落点相同。
- 定义处的标签写作 `LABEL(__k_unpaged_k_initial_stktop)`：前缀是**手写在汇编里**的。`head.S` 那一边写的是 `k_initial_stktop`——`head.o` 被改名后，这个引用变成 `__k_unpaged_k_initial_stktop`，与定义处的字面量对上。宏的展开路径在 [`include/asm.h`](minix3/sys/arch/i386/include/asm.h) 里：`LABEL(y)` 展开为 `_LABEL(_C_LABEL(y))`（该行 `#define` 与宏名之间是一个制表符，用空格做模式检索会漏命中）；`_C_LABEL(x)` 在 `__ELF__` 下原样返回 `x`（不添下划线），`_LABEL(x)` 展开为 `.globl x; x:`。三跳之后定义处的符号名就是字面量 `__k_unpaged_k_initial_stktop`，与引用侧被 objcopy 改名后的名字同名。

要判的事只有一件：这个符号落在哪个输出段，也就是它的值是低一侧还是高一侧。三处证据各排除一种可能：

| 证据 | 位置 | 排除了什么 |
|---|---|---|
| `mpx.o` 不在 `objcopy` 改名名单里 | `minix3/minix/kernel/arch/i386/Makefile.inc`（`.for` 块逐项列出；同文件 `SRCS+=` 一行里 `mpx.S` 只进普通源码清单） | 排除"它会被 `unpaged_*.o(.text)` 这类通配收走"——通配匹配的是输入文件名，`mpx.o` 的名字里没有 `unpaged_` 前缀 |
| 栈定义在 `mpx.S` 的 `.bss` 段里（i386）；`earm/mpx.S` 的等价物在 `.data`（`k_initial_stack` 前后有 `.data`/`.bss`/`.data` 切换） | [`k_initial_stack`](minix3/minix/kernel/arch/i386/mpx.S:k_initial_stack)（栈底标签所在处） | 排除“定义在 `.text`”——栈定义在 `.bss`；`.bss` 是否被低地址输出段收走，留给第 3 行的收段规则判断 |
| `kernel.lds` 的 `.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`，通用 `.bss` 排在 `. += _kern_offset` 之后 | `minix3/minix/kernel/arch/i386/kernel.lds`（三条语句按 `.unpaged_bss` → `. += _kern_offset` → `.bss` 的次序出现） | 落点唯一：`mpx.o` 的 `.bss` 只能进通用 `.bss`，也就是位置计数器加过偏移之后的段 |

i386 侧的结论是落点在高一侧：`k_initial_stktop` 的值是 0xF04xxxxx 一段的某个地址。这条依据链只在本篇给全：`01-boot-shim-bootstrap.md` 的 §1.4 只交代边界并声明落点判定归本篇，它的 §2.8 直接引用本篇的结论，两篇的结论一致。名字上带 `__k_unpaged_` 字样**不是**判据：这个字样在定义处是手写的，在引用处是 `objcopy` 加的，两处都不是"它进了 unpaged 段"的证据；把它当线索会得出相反的结论。

**假设栈定义落在 `.unpaged_bss`（低一侧）会怎样**：`head.S` 把 `%esp` 设成低地址值，`kmain` 在高地址执行而栈在低地址。恒等映射被移除之前一切正常；移除之后，栈所在的物理页已经没有映射，第一次栈访问是页错误。还有第二重后果：`.unpaged_bss` 覆盖的那段物理内存在引导阶段结束后会被交回空闲内存——`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 在 `system_init()` 之后调用 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`，源码注释写明 `The bootstrap phase is over, so we can add the physical memory used for it to the free list.`。栈所在的页会进入空闲链，被下一次分配拿走；同时 `pg_mapkernel` 只映射 `[kern_vir_start, kern_vir_start + kern_kernlen)`，不覆盖低地址这一段。两件事叠起来：栈的可用性既依赖一张会被丢弃的映射，又依赖一段会被回收的物理内存。

Rust 侧不再用链接符号给出 `kmain` 用的栈顶（入口 `_start` 仍用 `kernel_boot_stack_top`），改为由装载方算出镜像尾、随启动信息按值交接（决策见 §3.3，实现见 §4.2 与 §4.4）。

### 2.5 本章归纳

| 三件事 | C 里的产物 | 本节的结论 |
|---|---|---|
| 链接布局 | `kernel.lds` 的三行常量与 `AT()` 表达式 | 带 `AT()` 的普通段：符号值高一侧、装载地址低一侧，差是 `_kern_offset`；`unpaged_*` 三段的符号值与装载地址都在低侧 |
| 装载摆放 | `Makefile.inc` 的改名规则 + 外部装载器（GRUB 等）按 `p_paddr` 写段体（树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位，见 §1.2） | 段体落在低一侧；哪些对象进低一侧由改名名单决定 |
| 成对跳转 | x86-32：`head.S` 的四行（设栈、压终止槽、压参数、相对 `call`）；ARM32：`head.S` 的设栈 + `ldr`/`bx` | x86-32 的设栈携带绝对值、跳转携带相对位移；ARM32 两者都是绝对值；i386 侧已按 §2.4 证据落地，ARM 侧同一判据也已静态核实：`earm/Makefile.inc` 的改名名单不含 `mpx.o`、`earm/mpx.S:265-272` 在 `.bss` 后切回 `.data`、`earm/kernel.lds` 的 `.unpaged_data` 只收 `unpaged_*.o`，故 `k_initial_stack` 落在高半 `.data` |

影响本章三个动作结论的差异有三处：入口段的段名（ARM 多一个 `.startup`）、跳转指令的写法（x86 用相对 `call`，ARM 用字面量池取值的 `bx`）、`k_initial_stack` 的段归属（i386 在 `.bss`，`earm/mpx.S` 在 `.data`）；另有两处差异不影响结论——物理基址取值与改名名单（§2.2），它们不改变三个动作的结论。两边的语义相同：先设栈、再跳转；i386 侧两个动作指向同一侧，ARM 侧同一判据已由上述三处源码核实。

---

## 3. Rust 设计决策

本章五条决策，每条都从第 2 章里一个真实存在的负担开始。

### 3.1 链接脚本按"谁按装载地址摆放"分三种形态（→§2.1、§2.2）

C 侧的负担：内核镜像有两个地址，而"按装载地址摆放"这件事由内核之外的代码执行——`AT()` 表达式把期望的装载地址写进 ELF 的 `p_paddr` 字段，装载器读它。Rust 侧这个外部角色有两个候选：一个外部装载器（UEFI 路径的 boot-shim 从文件系统读 `kernel.elf`，或 U-Boot 预载后交给 boot-shim），以及**自引导形态的引导器**（RISC-V：U-Boot 的 `bootelf` 或 `-kernel` 直载按 `p_paddr` 摆放镜像——摆放者是引导器；镜像本身是引导体，但不重拷自己的段，§4.4）。谁读 `p_paddr`，谁就需要脚本里有一个低地址的 LMA；没有读者的时候，脚本多写一套地址只会多一处需要维护的不变量。

`os/kernel-image/*.ld` 三份都有 `AT()`，以 x86_64 为例：

```ld
/* 节选（保留基址常量与四个输出段的头部；省略号处为各段内的节收集语句），os/kernel-image/x86_64.ld（基址常量与 SECTIONS 的四个段头）。 */
KERNEL_HIGH_BASE = 0xFFFF800000000000;
KERNEL_PHYS_BASE = 0x200000;

SECTIONS
{
    . = KERNEL_HIGH_BASE + KERNEL_PHYS_BASE;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_HIGH_BASE)
    ...
    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_HIGH_BASE)
    ...
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_HIGH_BASE)
    ...
    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_HIGH_BASE)
```

三份脚本的地址写法有两种：x86_64 的位置计数器起点是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`（首段的虚拟地址里直接含有物理基址，`AT` 表达式减去 `KERNEL_HIGH_BASE` 就得到物理地址）；aarch64 与 riscv64 的起点是 `KERNEL_VIRT_BASE`，`AT` 表达式写成 `AT(ADDR(sec) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)`（[`aarch64.ld`](os/kernel-image/aarch64.ld)、[`riscv64.ld`](os/kernel-image/riscv64.ld)）。两种写法保证的是同一条不变量：所有段的"虚拟地址 − 装载地址"相等。这条不变量由 `check-layout.sh` 的 L4 断言检查（该脚本在 `01-boot-shim-bootstrap.md` §5.2 已逐条列出，本节只说它与 `AT()` 的因果关系）。

`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` 那套无生产链接读者的副本已按 `D-107` 删除（连同只锁常量的 lockstep 测试）；生产布局只由 `os/kernel-image/*.ld` 承载。载体（QEMU 测试内核；其中一部分目录带自己的 `link.ld`，其余沿用工具链默认布局）在带脚本的形态下各自用单地址脚本，以合成的 `KernelInfo` 走进同一段引导代码。

第三种形态在载体侧：[`link.ld`](os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld) 只有一套地址（`. = 0x80200000`），段没有 `AT()`，收尾一行 `__stack_top = . + 0x10000`。载体被固件放在低地址执行，不需要两套地址。

三种形态的判据归结为一件事：**有谁按 `p_paddr` 摆放到与符号地址不同的位置，才需要低地址的 LMA；摆放位置与符号地址一致（或没有读者）时，一套地址就够。**

**生产交接形态（v1 handoff）**：UEFI（x86_64/aarch64）与 riscv64 的 U-Boot 腿都走 `prepare_and_enter_kernel_image`/`prepare_boot` → `enter_kernel_image_v1` → 独立镜像 `_start` → `arch_boot_resume_v1`。接收侧只做 additive 阶段：把 arena 里的六块载荷（memmap/reserved/modules/platform/param/sources）落进本镜像 `.bss`、记录已活动根、建 Direct Map 覆盖，然后跳 `kmain`；它不重建 bootstrap 根，因为 `new_from_page` 会把正在使用的活动根清零。direct 面（riscv64 自引导、aarch64 direct-core）走 `bootface`/`bootface_a64` → `arch_boot` → `arch_boot_impl`，在物理视图里建根、开分页、建 DM；riscv64 再用条件 delta 跳 `kmain`，aarch64 用 `ldr` 的绝对值跳。测试载体分两类，不是一个视图：x86_64/aarch64 测试载体走 inline `arch_boot`、是低地址视图；riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 3 行）。两类都不是生产路径。

三套分类不是同一件事，后面的表按这张登记表换算：

| 链接脚本形态 | 装载路径 | 进入/执行视图 | 跳转完成时 PC |
|---|---|---|---|
| 生产镜像（带 `AT()`） | v1 send/receive（UEFI x86_64/aarch64、riscv64 U-Boot） | 高半视图 | 高半 VMA |
| 生产镜像（带 `AT()`） | riscv64 自引导（`arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta） |
| 载体单地址脚本 | riscv64 测试载体（直接 `arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta） |
| 生产镜像（带 `AT()`） | aarch64 direct-core（`bootface_a64` → `arch_boot`） | 低地址进入、物理视图 | 高半 VMA（`ldr` 取绝对值） |
| 载体单地址（工具链默认布局） | x86_64/aarch64 测试载体（inline `arch_boot`，UEFI helpers） | 低地址视图 | 低地址 |

“需要 `delta`”只落在 riscv64 的 direct leg；v1 receive、aarch64 UEFI 的 v1 跨镜像跳转、x86_64 `call` 都不需要。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 生产镜像：脚本用 `AT()` 给出低地址 LMA，交外部装载器摆放 | 选（`os/kernel-image/*.ld`） | 装载器必须按 `p_paddr` 写（[`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory) 就是这么写的）；脚本要维护"段间平移一致"这条不变量，并靠 `check-layout.sh` L4 兜住 |
| 载体：脚本只给一套地址，x86_64/aarch64 载体在入口时于低地址执行（riscv64 载体低装高进，见下表第 3 行） | 选（测试载体） | 内核符号值与运行地址之间没有平移关系，`KernelInfo` 的基址必须由载体自己填对；riscv64 载体仍按 direct leg 加 `delta` |
| 给载体形态的脚本写 `AT()` | 否 | 载体（测试内核）被固件或 `-kernel` 直接放在低地址执行，它自己不用 `p_paddr` 布局；写 `AT()` 只会多一套没有读者的地址。生产镜像（含 riscv64 的自引导形态）则必须有 `AT()`——它的 `p_paddr` 有读者：U-Boot 的 `bootelf` 读 `p_paddr` 摆放段体；`-kernel` 直载由固件/加载器按约定放置；OpenSBI 只负责跳转，不解析 ELF |
| 用常量表达式而不是逐段表达式（`AT(KERNEL_PHYS_BASE)`） | 否 | 段之间在物理上互相覆盖，见 §2.1 的反向推演 |

链接脚本里的量与 `KernelInfo`/`KernelLoadResult` 字段的对应关系：

| `.ld` 里的量 | `KernelInfo` 字段 | 谁读它 | 读者关心的语义 |
|---|---|---|---|
| x86_64：`KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`（首段 VMA） | `kern_virt_base` | 内核建页表（映射的源侧） | 符号值所在的一侧 |
| aarch64/riscv64：`KERNEL_VIRT_BASE` | `kern_virt_base` | 内核建页表（映射的源侧） | 符号值所在的一侧 |
| `KERNEL_PHYS_BASE` | `kern_phys_base` | 内核建页表（映射的目标侧）、装载器写段体 | 字节所在的一侧 |
| 镜像跨距（末段 `paddr + memsz` − 首段 `paddr`；平移一致时等于按 `vaddr` 算的差） | `kern_size` | 映射循环的上界、`kern_stack_top` 的算式 | 要映射多大的范围 |
| `ENTRY(_start)` | `entry_point`（`KernelLoadResult`） | 装载方决定从哪里进入镜像 | 第一条指令的地址 |

x86_64 的 `kern_virt_base` 不是 `KERNEL_HIGH_BASE` 本身：首段 VMA 是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，`compute_kernel_layout` 取 `min(p_vaddr)` 得到它。aarch64/riscv64 的首段 VMA 就是 `KERNEL_VIRT_BASE`。两种约定不能混用。

四个量里前三个由外部装载器（boot-shim 的两个 helper）从 ELF 的 PT_LOAD 段现算（`loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout)），不是从脚本里读常量；例外是自引导形态——它的三个量是与脚本同源的手写常量，三架构都由 L9 与 `readelf` 实测等值对账（x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`；§3.5、§4.4）。除这个例外，脚本与读者之间没有代码依赖，只有约定。

**给 [`x86_64.ld`](os/kernel-image/x86_64.ld) 去掉 `AT()` 会怎样**（保留高半基址）：四条 `AT` 都去掉之后，每个 `PT_LOAD` 的 `p_paddr` 退化成虚拟地址，段拷贝会向 `0xFFFF800000200000` 一类物理地址写字节。宿主侧先红的是 `check-layout.sh` 的 L3c（"首段 LMA = 0x200000"），真机侧的表现是内核字节从未被放到可执行位置。

### 3.2 ELF 解析器自建（→§2.1）

C 侧的负担：装载器要按 `p_paddr` 摆放、按 `p_memsz - p_filesz` 清零，这些都是 ELF 程序头里的字段；C 侧外部 multiboot 装载器（如 GRUB）读它们，树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位，内核自己一行解析代码都没有。Rust 侧把 C 内联在引导器里的这段解析搬到独立 boot-shim：boot-shim 与内核镜像分属两个可执行体，装载发生在 shim 内、由 shim 自己按 `p_paddr` 摆放（§3.1 的表把 shim 记为装载方）。因此这些字段由 shim 用本节的共享解析器读，内核侧一行解析代码也没有；与 C 的差别不在“有没有外部装载者”，而在装载者是本仓代码、与内核共用同一份解析器。需要的字段只有几个：`p_paddr`、`p_vaddr`、`p_offset`、`p_filesz`、`p_memsz`、`p_flags`（`p_flags` 在装载内核镜像时用不到，在装载用户 ELF 时要用，见本节末的处置表）。符号表、重定位、section header 都不在射程内。

解析器放在 `os/libs/minix-elf`，是一个独立 crate，被五个 crate 依赖（复算：`grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` 输出 `os/arch/Cargo.toml`、`os/boot-shim/Cargo.toml`、`os/servers/vfs/Cargo.toml`、`os/servers/vm/Cargo.toml`、`os/servers/rs/Cargo.toml` 五行）。用途按调用点分五类：boot-shim 用它装载**内核镜像**（按 `p_paddr` 逐段摆放），并用 `elf_file_end` 把模块 ELF 修剪到最后一个 `PT_LOAD` 的文件尾后整块拷贝（D-082/I-36；不重定位、不逐段映射）；`boot.rs` 的 [`fn load_vm_elf`](os/arch/src/arch/boot.rs:fn load_vm_elf) 用它把用户 ELF 逐页映射进进程地址空间；[`trap_api.rs`](os/servers/rs/src/trap_api.rs) 的 exec 路径用它读段与入口点；[`vm_server.rs`](os/servers/vm/src/vm_server.rs) 的 bootstrap 装载用 `entry_point` 与 `segment_iter` 取入口与段表。vfs 的 exec 路径只在常量上借用它（`ET_EXEC`/`PT_LOAD`），表头解析是 [`exec_worker.rs`](os/servers/vfs/src/exec_worker.rs) 自带的 `ElfHead`。同一份"段迭代"语义被多个调用方共用，是它成为共享 crate 而不是某个模块内部实现的理由。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 自建只读解析器，零堆分配（返回迭代器而不是 `Vec`） | 选 | 自己维护边界检查与错误分类；换来的是"读字节数组"这一种接口，两侧（boot-shim 与内核早期）都不依赖分配器 |
| 引入通用 ELF 库 | 否 | 通用库覆盖符号表、section、重定位等本路径不读的部分；两侧环境的分配能力不同（boot-shim 早期只有页粒度的固件分配，内核早期堆未初始化），共用的最小面反而更难对齐 |
| 用 `#[repr(C)]` 结构体加 `transmute` 直接解释字节 | 否 | 输入是文件字节切片，其对齐不可控；逐字段 `from_le_bytes` 与对齐无关，也不需要假设结构体无填充 |

这条取舍只针对**文件字节解析**。进程内的 `KernelInfo`/`MemoryRegion`/`BootModule` 也不再声称 `repr(C)`（D2-09）：它们不跨镜像边界按字节互相解释，没有 C ABI 读者，字段顺序与填充可以交给 Rust 布局决定。

三个结构体各对应 ELF 规范的一层（[`lib.rs`](os/libs/minix-elf/src/lib.rs)）：

```rust
/* 节选（保留字段名；注释有改写——源文件用 `///` 文档注释，此处压缩为行内注释；省略号处为未保留的字段），os/libs/minix-elf/src/lib.rs:struct Elf64Ehdr、struct Elf64Phdr、struct LoadSegment。 */
pub struct Elf64Ehdr {
    pub e_type: u16,       // 必须是 ET_EXEC(2)
    pub e_machine: u16,    // 架构：EM_X86_64=62, EM_AARCH64=183, EM_RISCV=243
    pub e_entry: u64,      // 入口点虚拟地址
    pub e_phoff: u64,      // program header 表的文件偏移
    pub e_phentsize: u16,  // 每个 Phdr 的大小
    pub e_phnum: u16,      // Phdr 数量
}
...
pub struct Elf64Phdr {
    pub p_type: u32,    // 段类型（PT_LOAD=1 表示可加载段）
    ...
    pub p_memsz: u64,   // 内存中的字节数（memsz > filesz 时，多出部分为 BSS）
}
...
pub struct LoadSegment {
    pub paddr: u64,    // 目标物理地址
    ...
    pub flags: u32,    // 段权限（PF_R=4, PF_W=2, PF_X=1，用于设置页表项属性）
}
```

字段只保留本仓要读的：`Elf64Ehdr` 丢了 `e_ident`、`e_version`、`e_flags`、`e_shoff`、`e_shentsize`、`e_shnum`、`e_shstrndx`、`e_ehsize`（`e_ident` 的前 6 个字节由 `parse_ehdr` 单独检查，不放进结构体）；`Elf64Phdr` 与 `LoadSegment` 也不保留 `p_align`：本仓没有消费者，多存一份只会让读者以为有对齐契约。

这些字段在文件里的字节位置写在解析函数里，`lib.rs` 的 [`fn parse_ehdr`](os/libs/minix-elf/src/lib.rs:fn parse_ehdr) 与 `:fn parse_phdr` 逐字段用 `from_le_bytes` 读：

| 字段 | 头部内偏移 | 读它的代码 |
|---|---|---|
| `e_type` | 16 | `parse_ehdr` 的 `ET_EXEC` 检查 |
| `e_machine` | 18 | `parse_ehdr`（`machine_type` 一侧的调用方另取） |
| `e_entry` | 24 | `parse_ehdr` → `SegmentIter::ehdr()` |
| `e_phoff` / `e_phentsize` / `e_phnum` | 32 / 54 / 56 | `SegmentIter::new` 的表范围检查与步进 |
| `p_type` / `p_flags` / `p_offset` / `p_vaddr` / `p_paddr` / `p_filesz` / `p_memsz` | 0 / 4 / 8 / 16 / 24 / 32 / 40 | `parse_phdr` 逐字段读取（`p_align` 在偏移 48，无读者，不读） |

迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出（I-48/D-084）。`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法（D2-01：debug 下 panic，release 下可能回绕后再由 `next()` 报 `Err`）（`lib.rs` 的 [`impl Iterator for SegmentIter`](os/libs/minix-elf/src/lib.rs:impl Iterator for SegmentIter)）：

```rust
/* 节选（保留 next() 的类型过滤与四类 Err 返回；注释有改写；省略号处为返回值构造），os/libs/minix-elf/src/lib.rs:impl Iterator for SegmentIter。 */
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.ehdr.e_phnum {
            let Some(entry_off) = (self.index as usize)
                .checked_mul(self.ehdr.e_phentsize as usize)
            else {
                self.index += 1;
                return Some(Err(ElfError::PhdrOutOfBounds));
            };
            let Some(offset) = (self.ehdr.e_phoff as usize).checked_add(entry_off) else {
                self.index += 1;
                return Some(Err(ElfError::PhdrOutOfBounds));
            };
            self.index += 1;

            // 表项越界/太小是畸形，不是过滤：以 Err 项浮出（I-48/D-084）
            let phdr = match parse_phdr(self.image, offset, self.ehdr.e_phentsize) {
                Ok(p) => p,
                Err(e) => return Some(Err(e)),
            };

            if phdr.p_type != PT_LOAD {
                continue; // 非可加载段是正常过滤
            }

            // 段文件字节必须落在镜像内；越界是畸形，以 Err 项浮出
            let Some(seg_end) = (phdr.p_offset as usize).checked_add(phdr.p_filesz as usize)
            else {
                return Some(Err(ElfError::SegmentOutOfBounds));
            };
            if seg_end > self.image.len() {
                return Some(Err(ElfError::SegmentOutOfBounds));
            }
            if phdr.p_filesz > phdr.p_memsz {
                return Some(Err(ElfError::FileszExceedsMemsz));
            }
            ...
        }
        None
    }
```

迭代器只把**非 `PT_LOAD` 类型**当作正常过滤（`continue`）；其余畸形一律以 `Some(Err(..))` 浮出：表项偏移溢出/越界是 `PhdrOutOfBounds`，`parse_phdr` 失败按具体变体返回，段文件字节越界是 `SegmentOutOfBounds`，`filesz > memsz` 是 `FileszExceedsMemsz`（I-48/D-084）。调用方必须在第一个 `Err` 上短路：`compute_kernel_layout` 以 `seg_result?` 短路并把段表收进 `KernelImage`；`load_segments_into_phys_memory` 只接受 `ValidatedKernelImage`，构造这个类型的 `validate` 已经跑过段形检查。`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法（D2-01 的真实残留：debug 下 panic，release 下可能回绕后再由 `next()` 报 `Err`），但“畸形段被静默跳过”这个旧缺口已经关闭；仍然开着的是“消费者若忽略 `Err` 项会继续遍历坏镜像”，所以每个调用点都必须短路。

错误分类是枚举而不是整数（`lib.rs` 的 [`enum ElfError`](os/libs/minix-elf/src/lib.rs:enum ElfError)），共 **12 个变体**，按产生层分四组：文件头五道 → `TooShort`、`BadMagic`、`Not64Bit`、`NotLittleEndian`、`NotExecutable`；程序头表两道 → `PhdrOutOfBounds`、`PhdrEntryTooSmall`；迭代器段形两道 → `SegmentOutOfBounds`、`FileszExceedsMemsz`；loader 三道 → `NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall { needed_end, buf_len }`。`filesz > memsz` 自 I-48/D-084 起由迭代器返回 `FileszExceedsMemsz`，不再走 `InvalidSegment`；loader 侧生产代码里返回 `InvalidSegment` 的点是三个：`paddr + memsz` 的 `checked_add`、`kern_end_phys - kern_phys_base` 的 `checked_sub`、段间物理重叠；源/目标重叠仍走 panic（与段间重叠的 `InvalidSegment` 不对称，是 D01-R9 的故意选择）。`TargetBufferTooSmall` 由缓冲区装载器在 I-47/D-089 的两循环里产生（先验后写，不半装载）。`InvalidSegment` 的归一化仍会丢失“哪一段/哪个地址”的现场；把解析错误与装载错误进一步分层属于设计批 D2-06。

`LoadSegment` 的字段在两个装载场景下的用法不同，这张表把每个字段的去向列全：

| 字段 | 装载内核镜像：摆放用 [`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory)，取基址与跨距用同文件 `:fn compute_kernel_layout` | 内核装载用户 ELF（[`fn load_elf_into`](os/arch/src/arch/boot.rs:fn load_elf_into)） |
|---|---|---|
| `paddr` | 拷贝目标；`compute_kernel_layout` 也用它算 `kern_phys_base` 与 `kern_size` | 不使用：物理页由分配器选，PA 与 VA 无固定关系 |
| `vaddr` | 只进 `compute_kernel_layout` 取最小值 | 映射的目标虚拟地址（逐页映射） |
| `offset` 与 `filesz` | 拷贝来源与长度 | 拷贝来源与长度 |
| `memsz` | BSS 清零的上界（`memsz - filesz` 个字节） | 段的页数上界，页内余量也清零 |
| `flags` | 不使用（内核镜像的权限由内核建表时自己定） | 转成页表权限（`elf_flags_to_page_flags`） |
同一份解析结果、四种去向，差异全部来自"这一侧是谁在摆放"。

**把迭代器的步进从 `e_phentsize` 改成结构体大小会怎样**：ELF 规范里 `e_phentsize` 是文件自己声明的表项大小，允许大于结构体的最小尺寸（56 字节）。用 `size_of::<Elf64Phdr>()` 步进，在表项恰好 56 字节的产物上结果相同；一旦某个链接器写出更宽的表项，第 2 项起的读位置全部落在错误偏移上，`p_type` 读到的是上一个表项尾巴的内容，症状是"段数对得上、地址全错"或"某些段被当成非 `PT_LOAD` 跳掉"。依据：`SegmentIter::next` 的 `offset` 计算式是 `e_phoff + index * e_phentsize`，而 `parse_phdr` 的越界检查用的也是 `entsize`。

### 3.3 栈顶从链接符号变成派生值（→§2.4）

C 侧的负担：`k_initial_stktop` 是一个链接符号，`head.S` 用一条绝对立即数指令把它的值写进 `%esp`（§2.3）。这个做法把"栈顶是多少"与"链接脚本怎么摆段"绑在一起：换一个布局，符号值跟着变。Rust 侧如果照搬，内核就要在源码里声明一个外部链接符号，而"内核被链接进谁"有三种形态（§3.1）——符号的值在每种形态下由不同的脚本决定，且测试载体根本没有这个符号。

改成按值派生：栈顶派生只需要两个值——`KernelInfo.kern_virt_base` 与 `KernelInfo.kern_size`（wire 另带 `kern_phys_base` 作为第三个布局标量；它不是栈顶算式的一部分）；栈顶由消费点在需要时现算（`kernel_info.rs` 的 [`impl KernelInfo`](os/libs/minix-boot/src/kernel_info.rs:impl KernelInfo) 的 [`fn kern_stack_top`](<os/libs/minix-boot/src/kernel_info.rs:fn kern_stack_top>)）。v10（D-062）之前这里曾是一个独立字段 `kern_stack_top`，每腿都要填 `kern_virt_base + kern_size`；该字段与另外两个派生标量（`user_sp`/`syscall_entry`）一起删除，公式只留下访问器这一份，不再需要发送侧/接收侧锁步：

```rust
/* 节选（保留访问器体），os/libs/minix-boot/src/kernel_info.rs:impl KernelInfo。 */
    pub fn kern_stack_top(&self) -> VirBytes {
        VirBytes(
            self.kern_virt_base
                .0
                .checked_add(self.kern_size)
                .expect("KernelInfo: kern_virt_base + kern_size overflows u64"),
        )
    }
```

式中 `kern_size` 来自 `loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout) 的 `kern_end_phys - kern_phys_base`，`kern_end_phys` 是所有段 `paddr + memsz` 的最大值（取物理侧极值而不是虚拟侧：映射的目标侧就是物理地址——高半映射把 `[kern_virt_base, +kern_size)` 指向 `[kern_phys_base, +kern_size)`；平移一致（§2.1 的逐段关系，L4 是它的可执行检查）时两者数值相等，这里选与映射循环同一口径）。于是栈顶 = 镜像虚拟基址 + 镜像跨距 = 镜像尾的虚拟地址。这个值成立要两个前提：

- 前提一：镜像的最后一个可加载段覆盖到镜像尾（`.bss` 是最后一段，它的 `memsz` 含链接脚本末尾那段对齐填充）。
- 前提二：镜像尾按 16 字节对齐——`lib.rs` 的 [`fn boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) 里有一条断言 `kern_stack_top must be 16-byte aligned`，而它由链接脚本的 2MiB 收口顺带满足（§3.5）。

同族的 `syscall_entry` 也已在 v10（D-062）删除：生产路径从来只把 `minix_arch::syscall_entry_va()` 当入口，字段是恒等式的第二份抄写；删除后验证/序列化路径不再读它，`configure_syscall` 的唯一读者是各架构入口，而不是交接字段。

Rust 侧还有第二个栈：链接脚本在 `.bss` 里预留的 64 KiB 引导栈，符号写作 `kernel_boot_stack_bottom` 与 `kernel_boot_stack_top`（三个架构的 `os/kernel-image/*.ld` 同形），入口汇编用它立栈（[`main.rs`](os/kernel-image/src/main.rs) 的 `global_asm!` 里 `lea rsp, [rip + kernel_boot_stack_top]` 等四条序列：x86_64、aarch64、riscv64 direct、riscv64 U-Boot）。两个值不一样：`kernel_boot_stack_top` 是预留区的顶（它与 `__bss_end` 在脚本里相邻赋值，取同一个值），`kern_stack_top` 是收口后的镜像尾，等于前者加上收口填充量（填充为零时两者相等）。分工是"入口汇编用前者、进入 `kmain` 前切到后者"。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 栈顶由接收方从 `kern_virt_base + kern_size` 派生（`KernelInfo::kern_stack_top()`） | 选 | 装载方只需把跨距算对（RISC-V 自引导面把跨距写成了常量，靠 L9 与 ELF 实测对账）；公式只留在访问器一处，三形态通用 |
| 内核自己声明链接符号（照搬 C） | 否 | 符号值随链接形态变化；测试载体没有这个符号，合成 `KernelInfo` 的路径无法替换它 |
| 由内核在运行时从自己的段边界符号算 | 否 | 内核早期没有段边界信息（那是装载方读 ELF 才知道的），且这些符号的可见性依赖链接形态 |

换栈这件事与栈的大小无关，与栈**运行期落在哪一侧**有关。C 侧可以直接比：`head.S` 的临时栈 `load_stack` 是 `.space 4096`，`mpx.S` 里给 `kmain` 用的 `k_initial_stack` 也是 `.space K_STACK_SIZE`（4KiB，§2.4）——两次设栈的容量相同，差别只在地址从低一侧换到高一侧。Rust 侧：入口 `_start` 的 64KiB 引导栈的链接符号 `kernel_boot_stack_bottom/top` 在 `.bss`、高半；`kmain` 用的 `kern_stack_top` 不是链接符号，而是消费点按 `kern_virt_base + kern_size` 现算的高半虚拟地址。运行期 `_start` 用 `lea rsp,[rip+...]`（x86_64）/`la sp,...`（riscv64），在物理视图下取到的是低物理地址，在高半视图下才是高 VMA；换栈的目的，是把 SP 从入口栈（可能仍在物理视图）移到镜像尾的高半栈，而不是笼统的“从低侧换到高侧”。

切过去之后栈的容身空间也是可以算的：栈向低地址增长，起点是 `kern_stack_top`（收口后的镜像尾）；从它往下到 `kernel_boot_stack_top` 之间是收口填充（0 到 2MiB 减 1 字节不等），再往下是脚本预留的 64KiB 引导栈区（`kernel_boot_stack_bottom` 到 `kernel_boot_stack_top`）。这两段都在 `.bss` 的 `memsz` 内；装载方按 `[filesz,memsz)` 清零，入口汇编清到 `__bss_end`。收口填充位于 `__bss_end` 之后，自引导面不保证清零；它只作为栈空间使用，不需要预清零。再往下是内核全局数据（`.bss` 的其余内容），不属于栈。

**去掉镜像跨距的 2MiB 收口会怎样**（§3.5 的 `. = ALIGN(0x200000)`）：与本节的交接值相关的一面是——填充量变了，`kern_size` 随之改变、`kern_stack_top` 的算式跟着变（栈顶 = 镜像虚拟基址 + 跨距）。收口缺失的完整后果链（粒度选择落空、规划阶段 panic）与宿主复现留在 §3.5；判据在 `loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout)（跨距的定义）与 `bootface.rs` 的 [`const KERN_SIZE`](os/kernel-image/src/bootface.rs:const KERN_SIZE)（自引导面对同一跨距的常量副本，L9 对账）。

### 3.4 成对的换栈与跳转收进一个 trait 契约（→§2.3）

C 侧的负担：换栈与跳转必须成对、且有固定次序（§2.3 的三处反向推演），而这两个动作在 C 里只是相邻的四行汇编（设栈、压终止槽、压参数、跳转），没有任何类型或接口约束它们。调用者（`pre_init` 返回后的那几行）与实现（同一个文件）在一起，所以约束靠位置相邻来维持。

Rust 侧把它写成一个 trait，前置条件写进文档与 `unsafe` 契约，返回类型写成 `!`（`higher_half.rs` 的 [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf)）：

```rust
/* 节选（仅保留 trait 与方法签名；模块与条目说明段未摘录），os/kernel/src/boot/higher_half.rs:trait HigherHalf。 */
pub trait HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> !;
}
```

方法上方的 Safety 段列了三条前置条件：①分页已启用且恒等映射与内核高映射都已建立；②`kinfo` 在高地址有效可访问；③只从引导 CPU 调用一次。栈顶不再是形参：三份实现都在函数体里用 `kinfo.kern_stack_top()` 现算，与 `kmain` 收到的启动信息同源。下表按顺序对上这三条（第 1、2、4 行），第 3 行记的是**返回类型**（`-> !`）——它与三条并列，但**不是前置条件**：

| 前置条件 | 谁保证 | 违反时的落点 |
|---|---|---|
| 分页已启用、两套映射已建立 | 调用者（direct 面是 `arch_boot_finish` 里 `arch_boot_impl` 的返回点；v1 面是 `resume_after_v1_decode`（三架构同名壳，公共体 `resume_additive`）） | 取指或压栈落在无映射地址 |
| 跳转传入的指针与 `kmain` 消费的副本 | 调用者保证传入地址可读；生产 `kmain` 第一条语句即遮蔽为 `.bss` 副本，形参只做交接、不解引用；测试载体若直接解引用则需自行保证 | 生产路径解引用副本时页错误；载体解引用形参时页错误 |
| 不返回 | 类型：`-> !` 让"返回"在编译期不成立 | 无法表达（不返回是签名的一部分） |
| 只调用一次 | 调用者（Safety 段的第三条写明；类型系统不区分调用次数） | 第二次调用会在已经切过去的栈上再切一次，落点不可预期 |

栈顶的 16 字节对齐不再是调用者要传对的值：`boot_validate_and_prepare` 的断言覆盖现算的 `kern_stack_top`，三份实现拿到的就是同一个值（`kinfo.kern_stack_top()`），形参与断言可能各说各话的那条缝已经消掉。删除形参的动机是“第二来源”：只要它还以参数形式存在，一个与 `kinfo` 不一致的值在类型上完全合法，编译器和测试都不会拦住。

三架构的实现各自放在自己的模块里（`higher_half.rs` 的 [`struct X86_64HigherHalf`](os/kernel/src/arch/x86_64/higher_half.rs:struct X86_64HigherHalf)、`higher_half.rs` 的 [`struct AArch64HigherHalf`](os/kernel/src/arch/aarch64/higher_half.rs:struct AArch64HigherHalf)、`higher_half.rs` 的 [`struct Riscv64HigherHalf`](os/kernel/src/arch/riscv64/higher_half.rs:struct Riscv64HigherHalf)）：两个生产调用点（`arch_boot_finish`、`resume_additive`）各自经 `arch_boot` 的三份 `#[cfg]` 分支选中本架构实现，所以“两个调用点”与“三份实现分支”不是同一个计数；§4.3 逐条对照。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| trait 加三份内联汇编实现 | 选 | 差异封在实现里；契约（前置条件与不返回）有单一落点 |
| `#[cfg(target_arch)]` 在同一函数里分支 | 否 | 三份 asm 混在一个函数里，前置条件只能写成注释 |
| 三个独立的 `.S` 文件（照搬 C 的 `head.S` 分工） | 否 | 汇编文件无法表达"前置条件已满足"这件事；`core::arch::asm!` 已能精确控制寄存器与栈指针，独立文件多出一层边界 |
| 返回 `()` 并在内部"跳到 `kmain` 再返回" | 否 | 换栈后返回地址已不在当前栈上；任何 `ret` 都会从新栈取一个不可预期的地址，而不是回到调用者（§2.3） |

**把跳转函数写成返回 `()` 会怎样**：编译器要为它保留返回路径；但换栈之后，返回地址已经不在新栈上，`ret` 从新栈顶取到的是一条未定义的"返回地址"，控制流会跳到不可预期的地方。`-> !` 把这条约束写进类型，编译器也不再生成返回路径。

表里三条是调用者要保证的；还有一条是链接布局的前提：“调用点的执行地址 = 链接地址”。x86_64 的 `call` 要求它，代码不做运行时检查，靠链接布局保证；aarch64 的 `ldr` 取链接期绝对值，但 literal 读取是 PC 相对的，同一镜像内 literal 与代码一起被摆放，所以不需要调用点 = 链接地址；riscv64 的 `la` 是 PC 相对，direct leg 不满足它，所以实现用 `auipc/bltu` 读当前 PC，在低于 `kern_virt_base` 时补 `delta`。这不是“实现内部处理”的三合一：x86_64 侧没有检查，riscv64 侧才有显式补偿；把它写成调用者前置条件会把 riscv64 的实现细节推给调用者，所以留在实现里，但 x86_64 的链接期前提必须单独说清。

再算一次对齐这笔账（x86_64）：`mov rsp, stack_top` 之后 RSP = stack_top；`and rsp, -16` 对已经 16 字节对齐的值是空操作；`push 0` 减 8，`sub rsp, 8` 再减 8，使 `call` 前的 RSP 回到 16 字节边界；`call` 压返回地址再减 8，进入 `kmain` 时 RSP = stack_top − 24，即 8 mod 16——这正是 System V 在函数入口要求的 `(RSP + 8)` 为 16 的倍数。生产 `kmain` 是普通 Rust 函数（`naked_asm!` 只用于 `qemu_test` 载体），所以这条 psABI 对齐是必须的；`sp_aligned` 断言相应检查 `sp & 0xF == 8`。`and rsp, -16` 与 `sub rsp, 8` 一起把这条不变量钉在本地，而不是修补上游。真正会坏的有三种情形：①三者都去掉时，一个非 16 对齐的 `stack_top` 会一路传到 `kmain`；②**只去掉 `sub rsp, 8` 也会坏**——按上面的算式，`push 0` 之后 `and` 不再有作用，`call` 前 RSP 是 8 mod 16；`call` 压返回地址后入口 RSP 变成 0 mod 16，而 System V 要求入口 `(RSP+8) % 16 == 0` 即 RSP ≡ 8 mod 16，正好违反。`and` 只修正上游的 `stack_top`，`sub` 才是补 `call` 那 8 字节的那一步；③**只去掉 `push 0`** 时终止槽缺失，且 `call` 前 RSP 由 `and` 后的 16 边界直接减 8（`call` 压返回地址），入口 RSP 变成 0 mod 16，同样违反 System V 的 `(RSP+8) % 16 == 0`，`sp_aligned` 会在真机上当场红。

### 3.5 镜像跨距收口：布局要满足映射粒度（→§2.2）

C 侧的负担：`pg_mapkernel` 按 4MB 一块映射内核（§2.2 的 961 与循环步进），因此**两个基址**都要按 4MB 对齐——`kernel.lds` 用 `_kern_phys_base = 0x00400000` 与 `_kern_vir_base = 0xF0400000` 提供这件事，`i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) 入口的两行断言把它当硬约束（`assert(!(kern_vir_start % I386_BIG_PAGE_SIZE));` 与 `assert(!(kern_phys % I386_BIG_PAGE_SIZE));`）。镜像大小没有对齐断言：映射循环以 `mapped < kern_kernlen` 为条件、每次步进 4MB，`kern_kernlen` 不是 4MB 整数倍时最后一块会向上取整到 4MB（多映射不足 4MB 的一段），而不是断言失败。

Rust 侧的页尺寸在内核的纯策略层选：`boot_policy.rs` 的 `plan_kernel_mapping` 先校验能力快照与布局，再按“首选支持且三量整除 → 回退支持且三量整除 → 否则错误”的阶梯选择（`boot_validate_and_prepare` 只负责采集能力事实与注册分配器）：

```rust
// 代码引用（保留选择阶梯；前置校验略），os/kernel/src/boot_policy.rs:fn plan_kernel_mapping。
let preferred = support.preferred();
let fallback = support.fallback();
let page_size = if preferred.supported() && layout_satisfies(preferred.size(), layout) {
    preferred.size()
} else if fallback.supported() && layout_satisfies(fallback.size(), layout) {
    fallback.size()
} else if !fallback.supported() {
    return Err(KernelMapError::FallbackUnsupported(fallback.size()));
} else {
    return Err(KernelMapError::NoKernelPageSize);
};
```

`arch_boot_impl` 调用 `plan_boot_pages`，后者以 `BootPagePolicyError` 返回失败；`arch_boot` 把该错误 panic 成两层包装的消息：外层 `arch_boot: page-size policy: {e}`（`os/kernel/src/lib.rs`），内层 `Display` 是 `kernel mapping policy: {e}`（`os/kernel/src/boot_policy.rs`），完整形如 `arch_boot: page-size policy: kernel mapping policy: no kernel page size satisfies the layout`，消息只报"无可用粒度"，不含三个量的数值。这一步发生在任何页表写入之前。

三档的判据与后果：

| 情况 | 选中的粒度 | 后果 |
|---|---|---|
| 三个量（`kern_virt_base`、`kern_phys_base`、`kern_size`）都被 1GiB 整除 | 架构的大页（三架构的 `HUGE_PAGE_SIZE` 都是 1GiB） | 内核映射用 1GiB 块；当前没有任何架构满足这条——三个物理基址都不是 1GiB 对齐（aarch64/riscv64 的虚拟基址虽然对齐，但 `kern_size` 约 4MiB 本身不是 1GiB 的整数倍） |
| 只能被 2MiB 整除（三个架构的当前值都落这一档） | 回退档（`FALLBACK_HUGE_PAGE_SIZE` = 2MiB） | 用 2MiB 叶子，步进次数等于镜像跨距除以 2MiB（当前工件跨距 4MiB，即两次） |
| 两者都不整除且回退档受支持 | 无 | panic（消息只报 `no kernel page size satisfies the layout`，不点名三个量），发生在任何页表写入之前 |
| 两者都不整除且回退档不受支持 | 无 | `FallbackUnsupported`（消息 `fallback page size … is not supported`），不是 `NoKernelPageSize` |

链接脚本为此在 `.bss` 末尾加了一条对齐语句（[`riscv64.ld`](os/kernel-image/riscv64.ld)，x86_64 与 aarch64 同形）。**注意脚本注释与现行实现不符**：选映射粒度的是 `plan_kernel_mapping`（`os/kernel/src/boot_policy.rs`），`boot_validate_and_prepare` 只采集能力事实——下面引文保留原文，按此口径读：

```ld
/* 代码引用，os/kernel-image/riscv64.ld（.bss 末尾的收口语句及其注释）。 */
        /* 镜像跨距 2MiB 收口（Sv39 的大页阶梯就是 2MiB 叶子）：
           boot_validate_and_prepare 按 huge-page 阶梯选映射粒度，
           kern_size 非 2MiB 对齐即 panic；kern_stack_top（镜像尾）同时
           获得对齐。收口属于末段 memsz，boot-shim 的 BSS 清零与
           kern_size 计算都以末段 memsz 端点为准。 */
        . = ALIGN(0x200000);
```

这条语句在 `.bss` 的括号里，所以它加出来的是**末段的 `memsz`**。装载方算 `kern_size` 用的是"末段 `paddr + memsz`"，因此收口的长度直接进 `kern_size`；`kern_stack_top = kern_virt_base + kern_size` 也因此拿到同一个对齐。三个量（`kern_size`、`kern_stack_top`、映射循环的上界）由这一条 `ALIGN` 同时决定。三份脚本在收口之后各记一个 `__image_end`，并在 `SECTIONS` 之外用两条 `ASSERT` 把这件事钉在链接期：一条查 `__image_end` 减首段 VMA 不超过 `KERN_SIZE`（0x400000），另一条查该差值是 2MiB 的整数倍——后者正是“收口语句还在”的机械证据，删掉 `ALIGN` 会先在链接期失败，轮不到运行期 panic。x86_64 的首段 VMA 是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，所以它的断言减这个和；aarch64/riscv64 减 `KERNEL_VIRT_BASE`。

RISC-V 自引导面把跨距写成了常量 `0x40_0000`（4 MiB，`bootface.rs` 的 [`const KERN_SIZE`](os/kernel-image/src/bootface.rs:const KERN_SIZE)），并在 `arch_boot` 之前使用；`check-layout.sh` 的 L9 拿 `readelf` 实测的跨距与这个常量对账（§5.3）。工件当前跨距以 L9 的实测输出为准，不以常量注释里的说法为准。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 链接脚本收口到 2MiB，运行时按阶梯选粒度 | 选 | 镜像尾部有最多 2MiB 的填充；换来粒度选择永不落进 panic 分支 |
| 不收口，运行时按实际 `kern_size` 用 4KiB 细粒度建表 | 否 | 内核镜像映射约 1024 个页表项（4 MiB ÷ 4 KiB = 1024），按 512 项一张表算约 5 张页表页（2 张末级页表页加各级目录页），这些页要从引导期 bump 区里出（`01-boot-shim-bootstrap.md` §3.8 的换手路径）；建表时间与池容量都要重算 |
| 收口到更大粒度（1GiB） | 否 | 镜像远小于 1GiB，填充浪费以百 MB 计；且 RISC-V 的当前档位是 2MiB |

**把收口改成 4KiB 对齐会怎样**：`kern_size` 不再是 2MiB 的整数倍，粒度选择落进 `else` 分支，panic 发生在任何页表写入之前——现场是"分页还没开就停"，消息只报 `no kernel page size satisfies the layout`，不点名三个量；三个量要用 `readelf -lW` 自己算。根因在链接脚本的段间距上，不在调用方传参上。把 [`riscv64.ld`](os/kernel-image/riscv64.ld) 的 `. = ALIGN(0x200000)` 删掉、其余不动，第一道门是链接期：`SECTIONS` 之外那条 `ASSERT((__image_end - KERNEL_VIRT_BASE) % 0x200000 == 0, …)` 直接失败，根本产不出镜像。要到达运行期 `plan_kernel_mapping` 的 panic，必须连这条 `ASSERT` 一起删；两者都删之后才轮到宿主上的 `check-layout.sh riscv64` 报 L5"跨距非 2MiB 对齐"、L9 跟着红。

---

## 4. 实现详解

### 4.1 链接脚本的落地（→3.1、3.5）

三份生产脚本由 [`build.rs`](os/kernel-image/build.rs) 按 `--target` 选择：

```rust
/* 节选（保留选脚本与追加链接参数的主体），os/kernel-image/build.rs。 */
    let script = match target.as_str() {
        "x86_64-unknown-none" => Some("x86_64.ld"),
        "aarch64-unknown-none" => Some("aarch64.ld"),
        "riscv64gc-unknown-none-elf" => Some("riscv64.ld"),
        _ => None,
    };
    if let Some(script) = script {
        println!("cargo:rustc-link-arg-bins=-T{manifest}/{script}");
    }
```

一个 bin、三份脚本，宿主目标不追加链接参数（`None` 分支）。这里的"独立 ELF"不是形式选择：内核要控制入口点、段布局与符号地址三件事，而这三件事都写在自己的链接脚本里——`os/kernel-image/Cargo.toml` 把内核声明成 `[[bin]] name = "kernel"`（不是 `[lib]`），`build.rs` 把脚本按 `--target` 追加成链接参数，`ENTRY(_start)` 与四个输出段决定地址。编成 rlib 被别的程序链接时，这三件事由那个程序的链接脚本决定（载体形态正是如此，见 §3.1），内核对自己的布局就没有发言权。装载面也只接受这一种形态：`check-layout.sh` 的 L1 要求 ELF 类型是 `EXEC`，理由是装载路径没有重定位处理器（`os/.cargo/config.toml` 对 `x86_64-unknown-none` 显式设了 `relocation-model=static`）。

下面按 [`x86_64.ld`](os/kernel-image/x86_64.ld) 的顺序读一遍，逐段对上 §3.1 的四个量。

起点与首段：

```ld
/* 节选（保留 .text 与 .rodata 的段体；注释有删节；段头与 AT() 表达式见 §3.1），os/kernel-image/x86_64.ld。 */
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_HIGH_BASE)
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
        *(.ap_early_entry)
        *(.ltext .ltext.*)
        PROVIDE(__text_end = .);
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_HIGH_BASE)
    {
        /* 内核启动图锚点（src/main.rs KERNEL_ENTRY_ANCHOR）：KEEP 防
           --gc-sections 把无引用静态连同整个内核启动图一起裁掉。 */
        KEEP(*(.rodata.kernel_anchor))
        *(.rodata .rodata.* .lrodata .lrodata.*)
    }
```

- `KEEP(*(.text.boot))` 排在 `.text` 的第一位，而 `_start` 就在 `.text.boot` 里（[`main.rs`](os/kernel-image/src/main.rs) 的 `global_asm!`）。于是入口地址等于镜像首字节。这条等式在宿主上由两条断言合起来检查：L3b 比"按脚本基址常量算出的预期首段 VMA"与实测首段 VMA，L6a 比 `e_entry` 与同一个预期值。
- `KEEP(*(.rodata.kernel_anchor))` 与 `static KERNEL_ENTRY_ANCHOR: fn(&KernelInfo, PhysBytes) -> ! = minix_kernel::arch_boot;`（`main.rs` 的 [`static KERNEL_ENTRY_ANCHOR`](os/kernel-image/src/main.rs:static KERNEL_ENTRY_ANCHOR)）是一对：一个持有内核启动面的函数指针，一个把该节钉住，防止 `--gc-sections` 或归档成员粒度裁剪把整个内核启动图裁出镜像。L8 断言 `minix_kernel::arch_boot` 的符号在镜像里，是这件事的可观测证据。

`.data` 与 `.bss`：

```ld
/* 节选（注释有删节），os/kernel-image/x86_64.ld（.data 与 .bss 两段）。 */
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_HIGH_BASE)
    {
        *(.data .data.* .ldata .ldata.*)
        *(.got .got.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_HIGH_BASE)
    {
        __bss_start = .;
        *(.bss .bss.* .lbss .lbss.*)
        *(COMMON)

        . = ALIGN(16);
        kernel_boot_stack_bottom = .;
        . += 0x10000;    /* 64 KiB 引导栈（_start 立栈用） */
        kernel_boot_stack_top = .;

        __bss_end = .;

        . = ALIGN(0x200000);
    }
```

- 三处通配各多收了一个族：`.rodata` 收 `.lrodata`、`.data` 收 `.ldata`、`.bss` 收 `.lbss`（rustc 对大块数据的只读/可写/BSS 拆分节）。三者都容易被漏掉；脚本注释记录过漏匹配的后果，但落点由链接器版本决定：孤儿节可能落在收口之前（不破坏 L5），也可能落在收口之后（把跨距推成非 2MiB 整数倍，L5 才红）。R4 之前 x86_64 工件的 `.lrodata.*` 就是未被 `.rodata` 通配收走的孤儿（986 个节、独立成一个 `PT_LOAD`）。旧工件的 `readelf -lW` 显示该孤儿段的 VMA=`0xffff800000290000`、LMA=`0x290000`，平移量与其它段一致，所以 L4 仍通过——GNU ld 对无 `AT()` 的孤儿段按相邻段继承 LMA 平移，不套用 §2.1 的“无 `AT()` 即 VMA=LMA”默认规则；该段又落在 2 MiB 收口之前，所以 L5 也通过。本轮把三份脚本的归并补齐后，x86_64 的 `PT_LOAD` 从 5 个回到 4 个。
- `.bss` 里的三个符号各有用处：`__bss_start`/`__bss_end` 是入口汇编清零循环的边界（[`main.rs`](os/kernel-image/src/main.rs) 的 x86_64 序列用 `lea rdi, [rip + __bss_start]` 到 `lea rcx, [rip + __bss_end]`）；`kernel_boot_stack_bottom`/`kernel_boot_stack_top` 是 §3.3 说的 64 KiB 入口栈。
- 末尾 `. = ALIGN(0x200000)` 落在 `.bss` 的括号里，加的是末段 `memsz`（§3.5）。
- 上面这四个符号（`__bss_start`、`__bss_end`、`kernel_boot_stack_bottom`、`kernel_boot_stack_top`）都是普通符号，不是 `PROVIDE`：引用它们的程序如果编译了、而符号不存在，链接期就失败（[`main.rs`](os/kernel-image/src/main.rs) 的注释把这条写作 fail-fast）。

`/DISCARD/` 与三份脚本的差异：

```ld
/* 代码引用，os/kernel-image/x86_64.ld（/DISCARD/ 段）。 */
    /DISCARD/ :
    {
        *(.eh_frame .eh_frame.*)
        *(.comment)
        *(.note .note.*)
    }
```

三份脚本的差异集中在五处：基址常量；`.sdata`/`.srodata`/`.sdata2`/`.sbss` 这几个 RISC-V 小数据优化产生的拆分节（[`riscv64.ld`](os/kernel-image/riscv64.ld) 把它们分别归入 `.rodata`/`.data`/`.bss`，注释写明落到 `.bss` 会丢掉文件内容）；riscv64 的 `/DISCARD/` 多一条 `.riscv.attributes`；x86_64 的 `.text` 额外收 `.ap_early_entry` 与 `.ltext`，且没有 `OUTPUT_ARCH`；链接期跨距断言（三份脚本各两条：上界 `__image_end - 首段 VMA <= 0x400000` 与收口对齐 `(... % 0x200000 == 0)`；x86_64 的首段 VMA 写作 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，aarch64/riscv64 写作 `KERNEL_VIRT_BASE`）。aarch64 与 x86_64 的段序与其余收段相同，差异只在基址、`AT` 表达式和 x86_64 这两条专属条目。

riscv64 的基址常量不是自由选择。Sv39 的虚拟地址是 39 位再按 bit 38 符号扩展到 64 位，规范要求 bits[63:39] 全为 0 或全为 1；把地址右移 30 位再取低 9 位得到顶层页表下标 VPN[2]：

```text
VPN[2] = (vaddr >> 30) & 0x1FF
```

复算：`printf '0x%x\n' $(( (0xFFFFFFC000000000 >> 30) & 0x1FF ))` 输出 `0x100`（256）；把基址换成 `0xFFFFFC0000000000` 或 `0xFFFFC00000000000`，同一条命令输出 `0x0`。后两个地址看起来像高地址，但 bit 38 是 0，符号扩展后落在低半——它们的 VPN[2] 与恒等映射（覆盖低 4GiB，即 VPN[2] = 0..3）共用同一个顶层槽位。[`riscv64.ld`](os/kernel-image/riscv64.ld) 的头注释把这两个地址列为"不是合法的 Sv39 canonical high"的实例；[`direct_map.rs`](os/arch/src/arch/direct_map.rs) 把内核 direct map 放在 VPN[2] = 257 也是同一个理由（不与镜像自己占的 256 号槽共用）。

这类"把下级页表指针写到已有叶子条目上"的动作，映射接口本身会拦一次：各架构的 `map_huge` 命中叶子就返回 `PageTableError::AlreadyMapped`，而不是把叶子降级成表指针。页表多级结构与叶子判定的完整定义在 [`rewrite-notes/02-stage-vm/07-pagetable-struct.md`](../../02-stage-vm/07-pagetable-struct.md)（该文已存在）；本节只用到基址选择的结论：riscv64 基址取 `0xFFFFFFC000000000`（VPN[2]=256），`0xFFFFFC…`/`0xFFFFC…` 因 VPN[2]=0 会与恒等映射共用槽位。

**把基址写成 `0xFFFFFC0000000000` 会怎样**：内核高半映射要写的是 VPN[2] = 0 那个槽，而引导期的恒等映射已经在那一槽落下 2 MiB 叶子（Step 1 固定用 2 MiB，不产生 1 GiB 叶）。两个方向的冲突都会在建表时被 `map_huge` 的 `AlreadyMapped` 拦住：往这个槽写 1 GiB 叶时，槽里已是表指针（指向装了 2 MiB 项的下一级表），`map_huge` 报 `AlreadyMapped`；反过来若上层已是 1 GiB 叶、这里写 2 MiB，x86_64 与 riscv64 的 `map_huge` 同样以 `AlreadyMapped` 拒绝降级（见 `os/arch/src/x86_64/paging.rs` 与 `os/arch/src/riscv64/paging.rs`）。假如没有这道检查，1 GiB 分支会直接改写该槽，后写条目把整槽的 2 MiB 项一并顶掉，恒等映射的覆盖范围从「若干 2 MiB 片」缩成一块；CPU 此刻正在低半执行，取指一离开这 2MiB 就失败——故障地址是低半的一条指令地址，而根因写在链接脚本的基址常量上。

**把 `.lbss` 从 `.bss` 的通配里去掉会怎样**：`--gc-sections` 之外，`.lbss` 节会被链接器当作孤儿节处理：放在它遇到的位置之后（通常是 `.bss` 之后、`ALIGN(0x200000)` 之前或之后取决于链接器版本），跨距随之变化——最坏情况是跨距不再是 2MiB 整数倍，链接期 `ASSERT` 直接失败（要看到 L5 红或规划层 `plan_kernel_mapping` 在 `arch_boot` 里 panic，得连 `ASSERT` 一起删；`boot_validate_and_prepare` 只采集能力事实）。把 `KEEP(*(.text.boot))` 从 `.text` 首位挪走：`_start` 不再落在首段开头，L6a 红（它比的是 `e_entry` 与按脚本常量算出的预期首段 VMA）；L6b 仍绿——它比的是 `nm` 出来的 `_start` 符号与 `e_entry`，两者由 `ENTRY(_start)` 绑在一起、同时移动。装载方（或固件）从 `ENTRY` 得到入口后仍然能跳，但 §4.3 里"链接地址与执行地址的关系"这一前提要重新论证。把 `. = ALIGN(0x200000)` 删掉：链接期 `ASSERT` 先失败；要看到 L5 与 L9 红，必须连 `ASSERT` 一起删（§3.5）。

### 4.2 ELF 解析与按装载地址摆放（→3.2）

这段链上有三个函数与一个纯函数孪生。先看解析入口的五道校验（`lib.rs` 的 [`fn parse_ehdr`](os/libs/minix-elf/src/lib.rs:fn parse_ehdr)）：

```rust
/* 代码引用，os/libs/minix-elf/src/lib.rs:fn parse_ehdr 的校验段。 */
    if image.len() < ELF64_EHDR_SIZE {
        return Err(ElfError::TooShort);
    }

    // Validate magic
    if image[0..4] != ELFMAG {
        return Err(ElfError::BadMagic);
    }

    // e_ident[4] = EI_CLASS
    if image[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }

    // e_ident[5] = EI_DATA
    if image[5] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }

    // e_type at offset 16 (2 bytes, little-endian)
    let e_type = u16::from_le_bytes([image[16], image[17]]);
    if e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
```

五道校验的次序有实际影响：长度检查排在第一位，因为其余四道都要读数组下标；魔术、类、字节序三道读的是 `e_ident` 的固定位置；类型检查读偏移 16 的两个字节。**`ET_EXEC` 这道检查把"可重定位"挡在门外**：本仓用户的 freestanding 镜像以 `-C relocation-model=static` 构建（`os/.cargo/config.toml` 的 `[target.x86_64-unknown-none]` 一节），装载面没有重定位处理器，遇到 `ET_DYN`（`static-pie`）就直接拒绝，而不是带着错位的地址继续。

接着是算布局（`loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout)）：

```rust
/* 节选（保留函数体；循环体内注释压缩/改写），os/boot-shim/src/loader.rs:fn compute_kernel_layout。 */
pub fn compute_kernel_layout(elf_data: &[u8]) -> Result<KernelImage<'_>, minix_elf::ElfError> {
    let mut iter = minix_elf::segment_iter(elf_data)?;
    let entry_point = iter.ehdr().e_entry;

    let mut kern_phys_base = u64::MAX;
    let mut kern_virt_base = u64::MAX;
    let mut kern_end_phys = 0u64;
    let mut found_load = false;
    let mut segments = Vec::new();

    while let Some(seg_result) = iter.next() {
        // I-48 / D-084: a malformed program header is an `Err` item, not a
        // silent skip; `filesz > memsz` is rejected inside the iterator.
        let seg = seg_result?;
        found_load = true;
        let seg_end = seg
            .paddr
            .checked_add(seg.memsz)
            .ok_or(minix_elf::ElfError::InvalidSegment)?;
        kern_phys_base = kern_phys_base.min(seg.paddr);
        kern_virt_base = kern_virt_base.min(seg.vaddr);
        kern_end_phys = kern_end_phys.max(seg_end);
        segments.push(seg);
    }

    if !found_load {
        return Err(minix_elf::ElfError::NoLoadSegments);
    }

    let kern_size = kern_end_phys
        .checked_sub(kern_phys_base)
        .ok_or(minix_elf::ElfError::InvalidSegment)?;

    Ok(KernelImage {
        elf_data,
        segments,
        layout: KernelLoadResult {
            kern_phys_base: PhysBytes(kern_phys_base),
            kern_virt_base: VirBytes(kern_virt_base),
            kern_size,
            entry_point,
        },
    })
}
```

逐段解码：两个初值是遍历取极值的写法（`u64::MAX` 与 0）；循环体内先取 `seg_result?`（I-48 / D-084：畸形程序头由迭代器以 `Err` 项浮出，`filesz > memsz` 的守卫也已收进迭代器），再用 `checked_add`/`checked_sub` 把地址溢出与长度下溢变成 `InvalidSegment`，而不是在 release 下静默回绕；`vaddr` 与 `paddr` 各取一次最小值——它们不是同一个数算两遍，而是两份独立的数据（§3.1 的字段对应表）；`kern_size` 用物理侧的最大端点减物理侧的最小起点，虚拟侧只用最小值。跨距与基址都在这个函数里定下，之后不再重算。`found_load` 的用途是把"一个可加载段都没有"变成 `NoLoadSegments`，而不是返回一组极值初值（`u64::MAX` 做基址的后果会推迟到建表时）。返回值从裸的布局结构改成 `KernelImage<'a>`：它同时持有原始字节的切片与这一次解析出的段表，调用者无法拿 A 镜像算出的布局去装载 B 镜像的字节；布局仍可按值取出（`layout()`，或经 `Deref` 直接读字段）。

最后是摆放（`loader.rs` 的 [`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory)）：

```rust
/* 节选（保留函数体），os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory。 */
pub unsafe fn load_segments_into_phys_memory(
    image: &ValidatedKernelImage<'_>,
) -> Result<(), minix_elf::ElfError> {
    for seg in &image.0.segments {
        let spans = segment_spans(seg)?;
        let file_len = usize::try_from(spans.file_end - spans.dst_start)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let bss_len = usize::try_from(spans.mem_end - spans.file_end)
            .map_err(|_| minix_elf::ElfError::InvalidSegment)?;
        let dest = spans.dst_start as *mut u8;
        let src = &image.0.elf_data[spans.src_start..spans.src_end];
        unsafe {
            dest.copy_from_nonoverlapping(src.as_ptr(), file_len);
            if bss_len > 0 {
                let bss_start = dest.add(file_len);
                core::ptr::write_bytes(bss_start, 0, bss_len);
            }
        }
    }
    Ok(())
}
```

装载前的校验分成值对象的两步。`compute_kernel_layout` 只做纯解析：循环里逐项 `seg_result?`，遇到第一个 `Err` 就返回（I-48 / D-084：畸形程序头以 `Err` 项浮出，不能先把整批 `Result` 收进 `Vec` 再当成“段比预期少”），合法段收进 `Vec` 并同时算基址与跨距，产出 `KernelImage`。`KernelImage::validate` 再跑两道目标侧检查：段间重叠（`loader.rs` 的 [`fn first_overlapping_pair`](os/boot-shim/src/loader.rs:fn first_overlapping_pair) 用半开区间判交叠：`a.paddr < b_end && b.paddr < a_end`，相邻不算重叠）与源/目标重叠（[`fn first_source_destination_overlap`](os/boot-shim/src/loader.rs:fn first_source_destination_overlap)，命中即 panic——段目标跨度覆盖任何段的 ELF 文件字节都会在装载中破坏源；这是 panic 而不是返回值，与段间重叠的 `InvalidSegment` 不对称）。校验通过才得到 `ValidatedKernelImage`，它的构造器私有，所以摆放入口拿到这个类型就等于拿到了“两道检查已经跑过”的证据；布局不再是“调用者记得先算”的口头约定。最后逐段按 `paddr` 拷贝 `filesz` 字节并清零 `[filesz, memsz)`。注意"清零"的条件是 `memsz > filesz`——`filesz == memsz` 的段（例如 `.text`）不会被额外写零，`filesz == 0` 的段（`.bss`）只清零。

可测的孪生是 `loader.rs` 的 [`fn load_segments_into_buffer`](os/boot-shim/src/loader.rs:fn load_segments_into_buffer)：同构的遍历与同样的两条动作，但目标是内存缓冲区、偏移按 `seg.paddr - buf_base_paddr` 折算。UEFI（x86-64/aarch64）与 OpenSBI U-Boot shim（riscv64）两条路径在生产代码里调用前者（直接写物理地址；"OpenSBI" 指 boot-shim 的 `opensbi_helpers.rs` 腿，不是 §4.4 路径 B 的直核 bootface——后者不重拷自己的段），宿主测试调用后者（写一块可检查的缓冲区）。两版不是同一份循环的薄包装：缓冲区版有目标长度上界（`mem_end <= buf.len()`），物理地址版有重叠检测而缓冲区版没有；这段差异是本篇承认的可测试性缺口，见 §5.1 的装载器测试族与下一段的覆盖边界。`filesz > memsz` 已由迭代器拒绝；`paddr + filesz`、`paddr + memsz` 与源区间 `offset + filesz` 的溢出由两版共用的 `segment_spans` 拒绝，不再只靠调用者先跑 `compute_kernel_layout`。缓冲区版在 I-47 后是两循环（先验后写，超界返回 `TargetBufferTooSmall`），物理地址版是“解析成 `KernelImage`、`validate` 后只接受 `ValidatedKernelImage`、再写”；两者共享段形校验、区间推导与 BSS 清零，不共享目标边界策略。宿主测试 `test_buffer_and_phys_loaders_share_segment_spans` 用同一份 ELF 分别走两版，逐字节比对拷贝区与清零区。直接调用这个 `pub unsafe fn` 的读者仍必须先校验布局（覆盖边界见 §5.1 末段）。

**把 BSS 清零这一步从两个环节里都去掉会怎样**（装载方只拷 `filesz` 不写 NOBITS 区，入口汇编也不清）：内核镜像的 `.bss` 里住着镜像堆 `static mut IMAGE_HEAP`（[`main.rs`](os/kernel-image/src/main.rs)）以及内核侧的启动全局；这些格子里会是上一位使用者的字节。两个自引导面共用的 `bootface_common::run_self_boot` 里的 `BootStorageCell`（`STORAGE`）初值非零、落在 `.data`；`bootface_a64.rs` 的 bump 游标 `static BUMP_NEXT: AtomicU64` 初值为 `BUMP_BASE`（非零），也落在 `.data`，所以它们不是这个例子的成员。这正是把两个环节都拆掉之后才会出现的形态：[`riscv64.ld`](os/kernel-image/riscv64.ld) 的注释记录过 OpenSBI 只按 `filesz` 写段体、不写 NOBITS 区这一实证，所以三份脚本都让入口汇编自己再清一遍（[`main.rs`](os/kernel-image/src/main.rs) 的注释把它写作"入口自持"）。两个环节各自都能单独成立，叠起来才是三架构一致的保证。

**去掉重叠检测会怎样（第二处）**：一个 `p_paddr` 区间交叠的 ELF（链接脚本写错、或工具链改布局）会让后一个段覆盖前一个段的字节，没有任何报错；后一个段覆盖前一个段的字节是数据损坏，不是 `copy_from_nonoverlapping` 的 UB——该函数的契约是单次调用的 `src`/`dest` 不相交；源/目标重叠另由 `first_source_destination_overlap` panic 拦住。段间重叠检测把前者变成 `ElfError::InvalidSegment`，与 §3.2 的 12 个变体里 loader 侧那三道之一对上。

### 4.3 三架构的成对跳转（→3.4、3.3）

trait 的三份实现做同一件事，指令不同。x86_64 全文（`higher_half.rs` 的 [`struct X86_64HigherHalf`](os/kernel/src/arch/x86_64/higher_half.rs:struct X86_64HigherHalf)）：

```rust
/* 节选（保留 impl 与 asm 块，注释有删节），os/kernel/src/arch/x86_64/higher_half.rs。 */
impl HigherHalf for X86_64HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> ! {
        let stack_top = kinfo.kern_stack_top();
        unsafe {
            core::arch::asm!(
                "mov rsp, {stktop}",
                "and rsp, -16",
                "xor rbp, rbp",
                "push 0",
                "sub rsp, 8",
                "call {kmain}",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("rdi") kinfo,
                options(noreturn)
            );
        }
    }
}
```

六条对四行（§2.3 摘录的 C 版本是四行，这里六条），逐条对应：`mov rsp` ↔ `mov %esp`；`push 0` ↔ `push $0`；`call {kmain}` ↔ `call kmain`；C 的 `push %eax` 由 `in("rdi") kinfo` 的寄存器绑定承担（不在 asm 指令里）；多出来的三条各有理由。`and rsp, -16` 把栈对齐到 16 字节——C 的 `head.S` 里没有对应（x86-32 的 4 字节栈不需要 16 字节对齐）。`xor rbp, rbp` 对应 `head.S` 入口段的 `mov $0, %ebp`（不在 §2.3 摘录的四行里）——差别只在位置与保存槽：C 在换栈前清零，`pre_init` 返回时把 callee-saved 的 EBP 恢复成 0，`kmain` 的保存槽因此也是 0；Rust 在换栈后清零，且生产 `kmain` 不保存 `%rbp`，回溯链从 `kmain` 内直接读到 0。两侧都在 0 处终止。`sub rsp, 8` 是 x86-64 特有的 psABI 垫片：`push 0` 后 RSP 是 8 mod 16，再减 8 回到 16 边界，`call` 压返回地址后入口 RSP = stack_top − 24，即 8 mod 16，满足 System V 的 `(RSP + 8)` 为 16 的倍数。生产 `kmain` 是普通 Rust 函数（`naked_asm!` 只用于 `qemu_test` 载体），所以这条对齐是必须的，不是可选的“跨形态一致”。`push 0` 仍是初始栈的终止槽（C 注释 `Terminate stack`），不在帧指针链上。`kmain = sym kmain` 让汇编器把 `{kmain}` 换成符号名，`call` 的位移由链接期决定；`in("rdi") kinfo` 把启动信息指针绑到 System V 的第一个参数寄存器上。

aarch64 的对齐与跳转（`higher_half.rs` 的 [`struct AArch64HigherHalf`](os/kernel/src/arch/aarch64/higher_half.rs:struct AArch64HigherHalf)）：

```rust
/* 节选（仅保留 asm 块；注释未摘录），os/kernel/src/arch/aarch64/higher_half.rs。 */
            core::arch::asm!(
                "mov sp, {stktop}",
                "mov x1, #-16",
                "mov x2, sp",
                "and x2, x2, x1",
                "mov sp, x2",
                "mov x29, #0",
                "isb",                      // defensive barrier; enable() already synchronized TTBR/SCTLR
                "ldr x1, ={kmain}",
                "br x1",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("x0") kinfo,
                options(noreturn)
            );
```

- 对齐用了四条指令：aarch64 不允许 SP 作为 `and` 的目的寄存器，所以经 `x2` 中转。这是"同一个语义在不同指令集里的指令条数不同"的一个实例。
- `isb` 的位置在 `br` 之前，作为防御性屏障；`enable()` 内部已经对 TTBR/SCTLR 的写入执行过 `dsb sy; isb`，所以它不是这些写入可见性的唯一来源。删除它不会产生“按旧映射取指”的失败（§4.3 其二）。
- `ldr x1, ={kmain}` 加 `br x1`：相对分支不能改变执行视图，低地址执行时 `b`/`bl` 会落到镜像的低地址副本，所以必须从字面量池取链接期绝对值再寄存器跳转；`ldr` 读出的值绝对、literal 的读取位置 PC 相对，这在低地址视图里仍然给出正确的高半目标（逐项对照见下表“取入口地址”行，与 §2.3 的 ARM32 `ldr`+`bx` 同一手法）。

riscv64 多一件事（`higher_half.rs` 的 [`struct Riscv64HigherHalf`](os/kernel/src/arch/riscv64/higher_half.rs:struct Riscv64HigherHalf)）：

```rust
/* 节选（仅保留 delta 计算与 asm 块；注释段未摘录），os/kernel/src/arch/riscv64/higher_half.rs。 */
        let delta = kinfo
            .kern_virt_base.0
            .checked_sub(kinfo.kern_phys_base.0)
            .expect("higher_half: kern_virt_base must be >= kern_phys_base");
        let kvirt = kinfo.kern_virt_base.0;
        unsafe {
            core::arch::asm!(
                "mv sp, {stktop}",
                "li t0, -16",
                "and sp, sp, t0",
                "li s0, 0",
                "fence.i",
                "auipc t3, 0",
                "la t1, {kmain}",
                "bltu t3, t4, 3f",
                "jalr x0, t1, 0",
                "3:",
                "add t1, t1, t2",
                "jalr x0, t1, 0",
                stktop = in(reg) stack_top.0,
                in("t2") delta,
                in("t4") kvirt,
                kmain = sym kmain,
                in("a0") kinfo,
                options(noreturn)
            );
        }
```

- `delta = kern_virt_base - kern_phys_base` 与 `kvirt = kern_virt_base` 在跳转前算出来，分别用 `in("t2")`/`in("t4")` 传进 asm。
- `auipc t3, 0` 把当前 PC 读进 `t3`；`bltu t3, t4, 3f` 比较当前 PC 与 `kern_virt_base`：低于它（direct leg，物理视图）跳到标号 `3`，执行 `add t1, t1, t2` 再加 `delta`；不低于它（resume leg，shim 已跳到高半）直接 `jalr`，不再加 `delta`。标号 `3` 之后是第二条 `jalr`。这就是“同一段 asm 服务两条腿”的实现。
- `la t1, {kmain}` 在 `-mcmodel=medany` 下展开为 `auipc` 加 `addi`，是一个 PC 相对取址：它算出的值是“链接期位移 + 当前 PC”。direct leg 的当前 PC 在物理视图，所以取到低物理地址；resume leg 的当前 PC 已在高半，取到的就是真值。
- `fence.i` 同步指令缓存与此前的存储；`jalr x0, t1, 0` 的 `rd=x0` 显式丢弃返回地址（x86 的 `call` 会压返回地址；x86 的 `push 0` 是终止槽，不是返回地址；aarch64 的 `br` 不写返回地址）。asm 块不改动 a0（`in("a0")` 载着 `kinfo`，须保留到 `kmain`），也不使用 a6/a7（SBI 调用约定占用的寄存器）——源码注释用 "Do NOT use a0/a7/a6 inside this asm block!" 写明这条约束。

`delta` 要存在的原因，是"链接地址"与"执行地址"之间可能有一个固定的差。§2.3 给出的判据是：PC 相对取址要求调用点的执行地址等于它的链接地址。当前 riscv64 有两条腿，正好一条满足、一条不满足：

| 形态 | 镜像被装到 | CPU 从哪个地址执行 | PC 相对取址的目标落在 |
|---|---|---|---|
| direct leg（RISC-V 自引导） | PA = VMA − delta | 低地址（PA） | 符号的低物理地址，需要加上 `delta` 才是高半地址 |
| resume leg（v1 接收，shim 已建根并跳到高半） | PA = VMA − delta | 高地址（VMA） | 符号的高半地址，不能再加 `delta` |
| aarch64 跨镜像/高半视图 | PA = VMA − delta | 高地址（VMA） | 符号的高半地址（`ldr` 的 literal 值） |

direct leg 里 `la` 算出的是符号的物理地址，`jalr` 会把 PC 留在低半；内核后续按高半 VMA 建进程根表，首次切换地址空间后取指就落到没有映射的低半地址上。resume leg 里 shim 已经跳到高半，`la` 取到的就是真值，再 `add delta` 会把目标推过头；源码注释记录的读数是 `0xffffffc00000477a + 0xffffffbf7ffe0000 → 0xffffff7f7fe0477a`。加数与结果不自洽：加数有一位十六进制错位（`7ffe` 对 `7fe0`，正确的加数是 `0xffffffbf7fe00000`）；按视图差 `kern_virt_base − kern_phys_base = 0xffffffbf7fe00000` 复算，`0xffffffc00000477a + 0xffffffbf7fe00000 = 0xffffff7f7fe0477a`，正是注释写的故障地址，随后取指页错误。源码注释里的 `0x8020…`（低半）是 direct leg 修正前的读数，`0xffffffc0…` 是修正后的高半读数。`checked_sub` 与 `expect` 是给这条算式加的前提检查：`kern_virt_base >= kern_phys_base` 不成立时在启动期报错，而不是在 release 构建下静默回绕成一个错的 `delta`。

| 步骤 | x86_64 | aarch64 | riscv64 |
|---|---|---|---|
| 切栈 | `mov rsp, {stktop}` | `mov sp, {stktop}` | `mv sp, {stktop}` |
| 栈对齐 16 字节 | `and rsp, -16` | `mov x1,#-16` / `mov x2,sp` / `and x2,x2,x1` / `mov sp,x2` | `li t0,-16` / `and sp,sp,t0` |
| 终止槽 / psABI 垫片 | `push 0`；`sub rsp, 8` | —（`br` 不压返回地址） | —（`jalr x0` 不写返回地址） |
| 清帧指针 | `xor rbp, rbp` | `mov x29, #0` | `li s0, 0` |
| 取入口地址 | `call` 的相对位移 | `ldr x1, ={kmain}`（字面量池） | `la t1, {kmain}`（PC 相对）；`auipc/bltu` 决定是否加 `delta` |
| 屏障 | 无 | `isb` | `fence.i` |
| 跳转 | `call {kmain}` | `br x1` | `jalr x0, t1, 0`（两条：直接跳 / 加 delta 后跳） |
| 参数寄存器 | `rdi` | `x0` | `a0` |

表注：三者不是一句“绝对 vs 相对”能概括的——`call` 要求调用点的执行地址等于链接地址；`ldr` 的 literal 读取是 PC 相对的，但同一镜像内 literal 与代码一起摆放、读出的是链接期绝对值；`la` 也是 PC 相对，direct leg 不满足前提，才需要 `auipc/bltu` 显式补 `delta`。

还有第三种执行形态：x86_64/aarch64 的内联测试载体——内核代码与垫片一起在低地址执行，符号值与执行地址之间没有平移差，相对取址与绝对取值都落在同一个视图里；"PC 在高半"不是这条形态的判据。riscv64 测试载体不是这种：它直接调 `arch_boot`，低装高进，仍按 direct leg 加 `delta`（§5.2）。

**三条推演（riscv64 direct leg 去掉条件 `add t1, t1, t2`；aarch64 去掉 `isb`；x86_64 去掉 `xor rbp, rbp`）**：

其一，riscv64 去掉 `add t1, t1, t2`。`jalr` 的目标落在符号的物理地址上，PC 留在低半（`0x8020…`）；启动能走完（低半此时仍有恒等映射），坏点在首次地址空间切换之后——内核按高半 VMA 建了进程根表，切换后取指落在没有映射的低半地址，`scause` 记下取指错误。源码注释（`os/kernel/src/arch/riscv64/higher_half.rs:42-51`）对这一形态的表述是：direct leg 的代码仍在 `kern_virt_base` 之下执行，只有跳转目标被抬到高半——即"PC 仍留在低地址"，而不是"内核从未迁到高半 VMA"。

其二，aarch64 去掉 `isb`。`enable()` 在写完 `ttbr0_el1`/`ttbr1_el1` 之后已经执行 `dsb sy; isb`，写完 `sctlr_el1`（置 M=1）之后又执行一次 `isb`；三条 aarch64 路径都经过 `enable()`，`establish_boot_dm` 之后到跳转之间不再写 TTBR/SCTLR。所以删掉 `jump_to_kmain` 里的 `isb` 不会产生“按旧映射取指”的失败；这条屏障是防御性保留，不是正确性前提。

其三，x86_64 去掉 `xor rbp, rbp`（或 aarch64 去掉 `mov x29, #0`、riscv64 去掉 `li s0, 0`——三个架构都靠这一步把回溯起点置 0）。清帧指针缺席时，`kmain` 内 `get_bp()` 读到的是换栈前留下的值，回溯链沿着与调用关系无关的地址走，走到一片合法映射里才停下——症状是 panic 打印的调用栈里出现无关地址。生产 `kmain` 不保存 `%rbp`（未启用 `-C force-frame-pointers`），所以这里没有“第一个带序言的帧把旧帧指针压进保存槽”这一步；链的终止点就是清零后的寄存器本身。（x86_64 序列里的 `push 0` 终止槽不在这条链上；去掉它改变的是入口栈的对齐，`sp_aligned` 断言会当场失败。）

### 4.4 从装载到 `kmain` 的完整路径（→3.1、3.3）

把 §4.2 的装载与 §4.3 的跳转接起来，四条路径（v1 send/receive、riscv64 自引导、aarch64 direct-core、inline 测试载体）的分岔点在"谁执行装载、谁建根"：

```text
生产路径 A（v1 send/receive：UEFI x86_64/aarch64、riscv64 U-Boot）
  固件 → boot-shim main → prepare_and_enter_kernel_image / prepare_boot
        → minix_elf 解析 → 按 p_paddr 写段体、清零 BSS
        → 读 12 个模块 → 组 KernelInfo（v1 交接载荷）→ 写 v1 arena（信封与 memmap/reserved/modules/platform/param/sources 六个分区）
        → ExitBootServices 配对循环 → enter_kernel_image_v1（建 bootstrap 根、补运行期字段）
        → 独立镜像 _start（清 .bss、立栈）→ rust_image_main
        → arch_boot_resume_v1（解码 arena、落 .bss、记录活动根、建 DM）
        → HigherHalf::jump_to_kmain

生产路径 B（riscv64 自引导）
  OpenSBI → 镜像 _start（清 .bss、立栈）→ rust_image_main
        → bootface：读 a1 的 DTB → 解 memmap → 读模块表 → 模块字节搬进 bump 页
        → 组 KernelInfo → minix_kernel::arch_boot
        → arch_boot_finish（校验/策略 → arch_boot_impl Step1 恒等 → Step2 高半 → Step2b/2c 设备窗口 → Step3 开分页 → Step4 DM）→ HigherHalf::jump_to_kmain

测试路径（inline）
  x86_64/aarch64 测试载体 → UEFI helpers → arch_boot（进程内）→ arch_boot_finish → arch_boot_impl → HigherHalf::jump_to_kmain
  riscv64 测试载体 → 直接 arch_boot（同一 arch_boot_impl）

生产路径 C（aarch64 direct-core）
  bootface_a64 → arch_boot（内部走 arch_boot_finish → arch_boot_impl）→ HigherHalf::jump_to_kmain
```

交接形态的分岔（v1 send/receive、aarch64 跨镜像跳转、riscv64 自引导、测试/直核）在 `01-boot-shim-bootstrap.md` §3.7 已逐条给出，本节只把装载、建根、跳转三步接起来。注意“aarch64 跨镜像”是 v1 内部的一次绝对跳转，不是独立的装载路径；aarch64 UEFI 的装载路径仍是 v1 send/receive。

§3.1 的登记表是权威，本节只按装载路径展开，不重复：

| 装载路径 | 进入/执行视图 | 链接脚本形态 | 跳转完成时 PC | 需要视图差 `delta`？ |
|---|---|---|---|---|
| v1 send/receive（UEFI x86_64/aarch64、riscv64 U-Boot） | 高半视图 | 生产镜像（带 `AT()`） | 高半 VMA | 不需要（按高半 VMA 进入，PC 与符号值同侧） |
| riscv64 自引导与 riscv64 测试载体（`arch_boot`） | 物理视图 | 生产镜像（带 `AT()`）或载体单地址 | 高半 VMA（direct leg 已加 `delta`） | **需要**（direct leg，§4.3） |
| aarch64 direct-core（`bootface_a64` → `arch_boot`） | 低地址进入、物理视图 | 生产镜像（带 `AT()`） | 高半 VMA（`ldr` 取绝对值） | 不需要 |
| x86_64/aarch64 测试载体（inline `arch_boot`，UEFI helpers） | 低地址视图 | 载体单地址（工具链默认布局；这两个载体目录没有 `link.ld`） | 低地址 | 不需要 |

> aarch64 direct-core 的“低地址进入”来自固件按装载地址平移入口地址的装载行为，不在本仓链接脚本里；可用 QEMU `-d in_asm` 的第一条记录核对 `_start` 的入口 PC。

自引导路径的正文在 `bootface_common.rs` 的 [`fn run_self_boot`](os/kernel-image/src/bootface_common.rs:fn run_self_boot)：从 DTB 解出 memmap、按模块表装载 12 个模块、组装 `KernelInfo` 这三步都在它里面，之后才调用内核的启动入口（`bootface.rs`/`bootface_a64.rs` 只传本架构常量与入口参数）：

```rust
/* 节选（保留 KernelInfo 组装与 arch_boot 调用；省略号处为 DTB 解析、模块表、模块装载），os/kernel-image/src/bootface_common.rs:fn run_self_boot。 */
    let report = STORAGE.report_static(
        VirBytes(params.kern_virt_base),
        PhysBytes(params.kern_phys_base),
        params.kern_size,
    );
    let kernel_info: KernelInfo =
        build_kernel_info(&report, minix_boot::RamAuthority::DTB);
    ...
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
```

两个基址与跨距是常量（`KERN_VIRT_BASE`、`KERN_PHYS_BASE`、`KERN_SIZE`，与 `riscv64.ld` 同源，L9 对账），`KernelInfo` 由共享 builder [`fn build_kernel_info`](os/boot-shim/src/lib.rs:fn build_kernel_info) 组装；`reserved` 是 `DtbMemoryMap::new(&fdt, fw_end)` 裁剪出的非传统内存（riscv64 传 `KERN_PHYS_BASE`，aarch64 传 0），`param_buf` 是 `/chosen/bootargs` 经 `normalize_param_buf` 规范化后的字节。两个自引导面在 D-107 清算里收成薄壳：`bootface.rs` 与 `bootface_a64.rs` 只保留本架构常量、入口横幅与各自的 console 实现，DTB 校验、memmap 收集、表址闸、bump 取页、模块摆放、平台源、参数缓冲、`KernelInfo` 组装与 `arch_boot` 调用全部走 `bootface_common` 的 [`fn run_self_boot`](os/kernel-image/src/bootface_common.rs:fn run_self_boot)。模块字节仍由 [`fn place_modules`](os/kernel-image/src/bootface_common.rs:fn place_modules) 搬运：源字节直接拷进 bump 页；不做整页清零，只把文件字节之后的尾部余量清零，名字槽从定长字节池拷进一页物理名字池——模块名以胖指针（`&str` 是"指针 + 长度"两个机器字，与单个地址不同）形式交给内核，指针必须指向物理视图可达的字节（这段的视图约束写在 `bootface.rs` 的模块文档里）。自引导镜像的早期代码在物理视图下靠 PC 相对寻址自洽（`la` 算出的值与当前 PC 同侧），只有最后跳 `kmain` 需要按 direct leg 加视图差值 `delta`（§4.3）。

自引导路径与 v1 路径的分界写在 `bootface.rs` 的模块文档里：自引导镜像**不重拷自己的段**。v1 的 boot-shim 会把 `kernel.elf` 的段拷进物理内存并清 NOBITS；自引导镜像已经在运行，同样的动作会覆盖正在执行的代码与栈。它只搬**模块**，自己的段不动，`.bss` 清理由入口汇编完成（§4.2 的第一处反向推演——BSS 清零那一条——讲过为什么这一步不能省）。

两条生产路径在 `kmain` 处合流，但不在 `arch_boot_impl`：v1 接收走 `arch_boot_resume_v1` → `resume_after_v1_decode`（记录活动根、落 `.bss`、建 DM），direct 走 `arch_boot_impl`（Step1 恒等 → Step2 高半 → Step2b/2c 设备窗口 → Step3 开分页 → Step4 DM）。两条路径在跳转前都把 `KernelInfo` 落进 `.bss` 副本，跳转时消费的是副本地址。副本落地的原因有两条：**载荷存活**——`reserved_regions`/`memmap`/`boot_modules` 原本指向 boot-shim 堆上的字节，高半切换后那块内存不再被映射（同一 VA 读回全零），落进内核 `.bss` 才能整段运行期可达（`lib.rs` 的 [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info) 函数体注释：`The reserved_regions payload lives in the boot-shim's UEFI-pool heap…`）；**交接路径**——早先把 shim 栈上的 `&KernelInfo` 经 `noreturn` asm 直传，后来改为全局副本的链接期 RIP 相对地址，指针仍按 ABI 传入 RDI/X0/A0，但生产 `kmain` 的第一条语句就把形参遮蔽为 `KERNEL_INFO` 全局副本，形参此后无读者，真正被消费的是副本地址。早期读数（`kmain` 收到的指针 `0x1ddda2ee` 与 `.rdata` 字符串常量地址相合）与 x86_64 的两块临时打印见 §6.1 的诊断注记。

**让自引导镜像照搬 v1 的做法、先把自己的段重新拷一遍会怎样**：目标区间就是它正在执行的代码与栈所在的物理内存：拷贝完成之前，正在执行的这段代码的字节已被改掉，下一条取指拿到的是新内容（或者被清零的 BSS）。这条分界在 [`bootface.rs`](os/kernel-image/src/bootface.rs) 的模块文档里点明。

跳转完成时系统处在什么状态、下一件必须做的事是什么，可以用一张状态表收束（下表描述的是当前 Rust 实现的状态，C 时代的对照见 §4.5）：

| 项 | 跳转完成时的值 | 依据 |
|---|---|---|
| 程序计数器 | v1 send/receive 与 aarch64 跨镜像：`kmain` 的高半 VMA；aarch64 direct-core：高半 VMA（`ldr` 取绝对值跳）；riscv64 自引导 direct leg：加 `delta` 后的高半 VMA；测试载体：x86_64/aarch64 低地址、riscv64 高半 VMA（§4.3/§4.4） | [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf) 的三份实现 |
| 栈指针 | x86_64：入口 RSP = stack_top − 24（8 mod 16，System V）；aarch64/riscv64：入口 SP = stack_top（16 字节对齐，AAPCS64/RISC-V ABI）；帧指针为 0 | [`fn kmain_verify`](os/kernel/src/lib.rs:fn kmain_verify) 的三条断言（展开见 §5.2） |
| 分页 | 已启用；恒等映射与内核高半映射并存；Direct Map 覆盖已建立；设备窗口（Step 2b aarch64 高半别名、Step 2c 设备窗口）已建立 | v1：shim 的 `build_bootstrap_root_and_enable` + 接收侧 `establish_boot_dm`；direct：`arch_boot_impl` 的 Step 1–4 与 2b/2c |
| 保护结构 | **未建立**：`kmain` 的早期阶段（Phase A，完整划分见 `03-kmain-cstart.md`）只做校验与信息收存，不安装保护结构；保护结构 `init_protection`（GDT/TSS 或 VBAR_EL1/stvec）是 Phase B 的第一条安装动作——在那之前任何异常都无法处理 | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的文档注释（`without valid GDT/TSS … any exception will cause an unrecoverable triple fault`；同一注释里的 `very first` 指安装动作的次序，不是 `kmain` 的第一条语句） |
| 中断 | 未接管；时钟与中断控制器在保护结构之后初始化（`kmain` 的 Phase B 里 `init_protection` 在前、`init_clock_and_interrupts` 在后） | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection)、`:fn init_clock_and_interrupts`；细节属 `03-kmain-cstart.md` 的范围 |

也就是说，这一篇结束时 CPU 已经站在内核自己的地址空间与自己的栈上，但还没有任何"出事能被接住"的设施。**本篇主线的终点在这里——验证与诊断见 §5、§6**（下一篇从 `kmain` 的第一条指令开始，先把保护结构装上；范围归属见 §1.3）。

### 4.5 与 Minix3 C 的对照（→2.1–2.4）

| C 侧的机制 | Rust 侧落点 | 处置 | 差异原因 |
|---|---|---|---|
| `AT()` 表达式给出 LMA，装载器按 `p_paddr` 写段体（§2.1） | `os/kernel-image/*.ld` 的 `AT()` + [`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory) | 保留（形态） | 装载者从 GRUB 换成 boot-shim 或引导器（U-Boot；OpenSBI 只提供 SBI，不摆放镜像），表达式与读法不变 |
| `objcopy --prefix-symbols` + `unpaged_*.o` 两段式布局（§2.2） | 无对应物：内核镜像只有高半一段，引导侧代码在 boot-shim 或入口汇编里 | 淘汰 | 分页开启前的主体在 boot-shim（v1 建根）或入口汇编（direct 面）；内核镜像没有 C 那种由同一装载器放到低物理地址再执行的 unpaged 段（`01-boot-shim-bootstrap.md` §3.7 的交接形态） |
| `k_initial_stktop` 链接符号（§2.4） | `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size` 的派生访问器） | 改型 | 值由接收方现算，内核不声明外部符号，也不在交接载荷里多传一份 |
| `head.S` 的成对动作：设栈 + 跳转（C 侧四行，§2.3） | [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf) 与三份内联汇编实现 | 改型 | 收进 trait 契约；栈对齐、屏障、执行视图差值三件事各自显式 |
| —— | `check-layout.sh` 的 L0–L9 宿主断言 | 新增 | C 侧由 Makefile 与链接脚本的常量约束，没有独立断言脚本 |

三行"改型/新增"的共同点：把 C 里靠"位置相邻、文档说明、读者经验"维持的约束，换成一种可以被工具或类型系统检查的形式——脚本不变量进断言脚本，栈顶值进派生访问器，跳转前提进 `unsafe` 契约。

**把 C 的两段式布局照搬进 Rust 会怎样**（给内核镜像也加一个低地址的 unpaged 段，把"分页开启前要执行的代码"放进去）：Rust 侧已经没有"由同一个装载器把整个镜像放到物理地址上、再从低地址开始执行"的形态。v1 路径的 boot-shim 已经建好根并把镜像按 `p_paddr` 摆放，进入镜像是高半 VMA；direct 面的入口汇编在低物理地址执行、自己建根。把一段"分页前代码"放进按高 VMA 链接的低地址段，在 v1 会把取指引到高 VMA（映射已建，但段体在低地址，语义错位）；在 direct 面则与入口汇编的 PC 相对寻址冲突。C 的两段式能成立，是因为 GRUB 按 LMA 把整个镜像放到物理地址、低地址段与建表代码在同一个镜像里；Rust 侧这两件事被拆给了 boot-shim 与入口汇编（`01-boot-shim-bootstrap.md` §3.7 的交接形态）。

---

## 5. 测试要点

### 5.1 宿主：解析器与装载器的单元测试（→4.2）

§4.2 那条链里，解析、布局与缓冲区语义的每个环节都有宿主测试，分属两个 crate；按 `p_paddr` 写物理地址那一步不在这批测试的射程内，只由真机链证明（见本节末段）。

`os/libs/minix-elf` 的 27 个测试（复算：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 27；运行：`cd os && cargo test -p minix-elf`）按对象分四族：

| 族 | 测试名 | 证明什么 |
|---|---|---|
| 文件头五道校验 | `test_parse_ehdr_valid`、`test_parse_ehdr_bad_magic`、`test_parse_ehdr_too_short`、`test_parse_ehdr_not_64bit`、`test_parse_ehdr_not_le`、`test_parse_ehdr_not_executable` | 每种畸形输入各返回对应的 `ElfError` 变体 |
| 迭代器行为 | `test_segment_iter_valid`、`test_segment_iter_no_pt_load`、`test_segment_iter_two_segments`、`test_segment_iter_phdr_out_of_bounds`、`test_segment_iter_phdr_entry_too_small`、`test_segment_iter_empty_image`、`test_segment_iter_header_only`、`test_segment_iter_reports_segment_data_out_of_bounds`、`test_segment_iter_filesz_exceeds_memsz_is_err`、`test_segment_iter_rejects_filesz_greater_than_memsz`、`test_segment_iter_truncated_mid_segment_is_err`、`test_segment_iter_non_load_filter_stays_silent` | 表范围检查、非 `PT_LOAD` 过滤、四类畸形以 `Err` 返回（`PhdrOutOfBounds`、`PhdrEntryTooSmall`、`SegmentOutOfBounds`、`FileszExceedsMemsz`；I-48/D-084） |
| 段字段 | `test_segment_with_zero_filesz`、`test_mixed_pt_load_and_other`、`test_segment_flags`、`test_entry_point`、`test_machine_type` | 字段取值的忠实性（含 `filesz == 0` 与混合段序） |
| 装载仿真 | `test_kernel_load_simulation_single_segment`、`test_kernel_load_simulation_two_segments`、`test_kernel_load_simulation_with_bss_segment`、`test_segment_copy_and_bss_fill` | 用解析结果做一次"拷贝 + 清零"，逐字节核对 |

`os/boot-shim` 的 `loader.rs` 部分有 32 个测试（复算：`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 32；运行：`cd os && cargo test -p boot-shim --lib` 在默认特性下共 136 个测试，其中 32 个属于 `loader::tests`），与本节直接相关的有 28 条，代表项如下：`test_compute_kernel_layout_from_mock_elf`（布局三个量的取值）、`test_read_required_panics_on_missing_kernel`（缺内核文件是 panic 而不是继续）、`test_bss_region_is_zeroed_after_load`（BSS 清零的结果逐字节核对）、`test_buffer_and_phys_loaders_share_segment_spans`（两版装载器逐字节一致）、`test_load_segments_into_phys_memory_rejects_overlapping_elf`（重叠段在 `validate` 被拒）、`test_compute_kernel_layout_rejects_filesz_greater_than_memsz`（`filesz > memsz` 被拒）、`test_compute_kernel_layout_rejects_paddr_memsz_overflow`（地址溢出被拒）、`test_compute_kernel_layout_zero_size_segment_yields_zero_kern_size`（零长段边界），六个 `first_overlapping_pair_*`（区间判交叠：不相交、真重叠、相邻不算、零长度被忽略、首个命中、空表——见下段）、`test_elf_file_end_{returns_last_load_file_end,non_elf_is_none,malformed_segment_is_none}`、`test_load_boot_modules_into_trims_elf_to_file_end`、`test_compute_kernel_layout_rejects_truncated_mid_segment` 与三条 `load_segments_into_buffer_*`。

这些测试覆盖三类对象，各自走的路径不同：`test_bss_region_is_zeroed_after_load` 直接调用孪生函数 `load_segments_into_buffer`，逐字节核对三件事——`[0, filesz)` 保留文件字节、`[filesz, memsz)` 清零、`memsz` 之后一个哨兵字节不被触碰（判据是两种预填充值 0xAB 与 0xCC）；`test_compute_kernel_layout_from_mock_elf` 直接调用 `compute_kernel_layout`，测试文件里写明了原因（大意：不能在单元测试里做那次物理内存拷贝）；六个 `first_overlapping_pair_*` 直接调纯函数（不相交、真重叠、相邻不算、零长度被忽略、首个命中、空表）。模块那一侧的三条测试走 `load_boot_modules_into` 加一个模拟分配器，它的地址指向测试期间不释放的静态 `Mutex<Vec<Vec<u8>>>` 缓冲区——同一文件里的注释写明（大意）"绝不在单元测试里写真实物理内存"。

因此宿主测试证明的是"解析出的字段、拷贝与清零的语义、区间判交叠"，不证明"按 `p_paddr` 写物理地址"这一步：那一步的唯一执行者是 `load_segments_into_phys_memory`，它的目标地址来自 ELF，在宿主上无法安全执行，留给真机链。这条分界也解释了为什么孪生函数存在——把可测的语义从不可测的那一步里剥出来。

**砍掉装载仿真族（四个测试）只留解析族会怎样**：解析结果对、但"拷贝时把 `filesz` 写到哪、BSS 从哪开始、清零多长"这些语义没有断言；这三处任一处的偏移错误（例如用 `memsz` 当拷贝长度、或 BSS 起点用 `paddr + memsz`）都只在真机上以随机崩溃的形式出现——而真机测试的现场只有串口日志，定位要重新走一遍宿主就能做完的排查。

### 5.2 真机：`test-higher-half` 的三条断言（→4.3）

真机侧验证 §4.3 的产物是 `test-higher-half` 测试内核：它走完整的 `arch_boot` 路径，最后停在 `kmain`；`kmain` 在 `qemu_test` 形态下是裸函数，在函数序言改动栈指针之前把四个值抓下来交给验证函数（`lib.rs` 的 [`fn kmain_verify`](os/kernel/src/lib.rs:fn kmain_verify)）——启动信息指针，以及 SP、PC、FP 三个寄存器：

```rust
/* 节选（保留三条断言与判定；注释有改写），os/kernel/src/lib.rs:fn kmain_verify。 */
    // SP must be at a high virtual address (kern_stack_top was used)
    let sp_ok = sp >= kern_high;
    // x86_64 System V: (RSP + 8) % 16 == 0 -> RSP & 0xF == 8
    #[cfg(target_arch = "x86_64")]
    let sp_aligned = sp & 0xF == 8;
    // aarch64 AAPCS64 / riscv64 psABI: SP 16-byte aligned -> SP & 0xF == 0
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    let sp_aligned = sp & 0xF == 0;
    // Frame pointer must be zero (set by HigherHalf transition)
    let fp_zero = fp == 0;
```

三条断言各对应 §4.3 的一条指令：`sp_ok` 对应"切栈"（栈指针落在高半地址空间）、`sp_aligned` 对应"对齐"（x86_64 检查 `RSP ≡ 8 (mod 16)`，即 `(RSP+8)` 为 16 的倍数；aarch64/riscv64 检查 `SP ≡ 0 (mod 16)`，AAPCS64/RISC-V ABI 要求 SP 16 字节对齐）、`fp_zero` 对应"清帧指针"。捕获方式按架构写在裸函数 `kmain` 的 `naked_asm!` 里（x86_64 用 `mov r9, rbp`、`lea r8, [rip]`、`mov rdx, rsp`；aarch64 用 `mov x3, x29`、`adr x2, .`、`mov x1, sp`；riscv64 用 `mv a3, s0`、`auipc a2, 0`、`mv a1, sp`）。

**PC 不在成败判定里**，这条取舍写在源码注释里（"PC check is architecture-dependent… PC may not be in the high range"）：三个载体都被固件装在低地址，但**进入方式不同**——x86_64 与 aarch64 用镜像自己的（低）链接地址进入，PC 停在低半；riscv64 的载体走"低装高进"（`la` 加上 `delta` 位移，[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs) 的说明段），PC 落在 `kmain` 的高半 VMA，源码注释记录了实机读数（`0xffffffc0…`，修正前是 `0x8020…`）。PC 一项因此按架构分开判定：x86_64 与 aarch64 只打印（`PC 不在高半` 时输出 `INFO`，只在失败分支里出现）；riscv64 把 `pc_at_high` 并入 PASS 条件，PC 落在低半即 `FAIL: PC not at high address (riscv64 delta path broken)`——这正是 `delta` 路径的机械判据（`kmain_verify` 的 `pc_ok` 分支，`target_arch = "riscv64"`）。测试载体的 `KernelInfo` 是合成的（各载体同形、取值按架构；以 x86_64 载体为例：[`main.rs`](os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs) 里 `kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（镜像窗口按 2 MiB 取整后的跨度）），栈顶由 `kern_stack_top()` 现算，也就是 §3.3 的算式在测试形态下的取值。

运行方式与判定标记：`os/qemu-tests/run_qemu.sh` 用 `-serial file:` 收串口、用 `strings | grep '### TEST_RESULT: PASS'` 判定（`os/qemu-tests/run_qemu.sh` 的判定段），退出码里 `SKIP` 与 `FAIL` 分开（0 表示 PASS 或 SKIP，1 表示失败）。测试本身的输出形如 `### TEST_RESULT: PASS test-higher-half ###`。

**把 PC 无条件加进成败判定会怎样**：x86_64 与 aarch64 的载体按低地址进入、PC 常驻低半，这条断言在这两个架构上恒红；红色成为常态之后，`sp_ok`/`sp_aligned`/`fp_zero` 三条真正的失败会淹没在同一条常红里。因此判据按架构分开：这两个架构仍只打印 `INFO`，riscv64 才把 PC 高半并入 PASS 条件——在 riscv64 上它不会恒红，因为 `delta` 路径正常时 PC 就该落在高半；它一旦恒红，说明 `delta` 补偿坏了，这正是要抓的故障。

### 5.3 布局契约与 01 篇断言体系的分工（回指 01 篇 §5.2）

镜像还没运行就能检查的那批断言（`check-layout.sh` 的 L0–L10）在 `01-boot-shim-bootstrap.md` §5.2 已逐条列出完整措辞，本节不重复；写给未读 01 篇读者的编号速查：L0 脚本预期表与链接脚本声明的两个基址互相对账；L1 ELF 类型 = `EXEC`；L2 机器类型与预期相符；L3a 段数 ≥ 1、L3b 首段 VMA = 预期、L3c 首段 LMA = 物理基址；L4 所有 `PT_LOAD` 的 `vaddr − paddr` 一致；L5 镜像跨距 2MiB 对齐；L6a 入口 = 预期、L6b `_start` 符号 = 入口、L6c 入口落在首段内；L7 引导栈符号存在、64 KiB、落在末段；L8 `minix_kernel::arch_boot` 在镜像里；L9 跨距对账（ELF 实测 = 对应 direct face 的 `KERN_SIZE`；x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`）；L10 C 侧段归属（`Makefile.inc` 的 unpaged 名单与 `kernel.lds` 收段规则对账，断言 `mpx.o` 不在名单、`k_initial_stktop` 落高半；只在 x86_64/all 复跑时输出）。与本章有因果关系的是四条：L3c（首段 LMA = 物理基址）与 L4（所有段平移一致）是 §3.1 那条"按 `p_paddr` 摆放到与符号地址不同处就需要低地址 LMA"判据的可执行形式；L5（跨距 2MiB 对齐）与 L9（跨距对账）是 §3.5 与 §3.3 的可执行形式（L9 三架构均执行，按架构选 direct face；01 篇 §5.2 的清单注明了这一条）。四条断言的作用是把"脚本里的常量、boot-shim 读出的布局、自引导面硬写的常量"这三处互相拽住；三架构 L9 都会把任一处单独改动打红。

### 5.4 机制 → 需要证明什么（反转核对表）

本节从**机制**出发列出"要证明它成立需要什么断言"，再标注覆盖状态——不是从现有测试出发（那份视角在 §5.1–§5.3；本节是它的反转面）。反转核对查出 3 条覆盖缺口（R02-T01/T03/T04），三条均已闭合：T01 由 `check-layout.sh` 的 L10、T03 由 riscv64 的 PC 高半判据、T04 由生产入口的 ABI 栈形断言；R02-T02 的 arch link.ld 常量锁步随 `D-107` 删除该形态而销账（无生产读者，锁步对象不存在）。

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 覆盖状态 | 现有证据 |
|---|---|---|---|---|
| M1 | `kernel.lds` 的双地址与 `AT()`（§2.1） | 每个带 `AT()` 段的 `vaddr − paddr` = 同一常量 | 部分覆盖 | Rust 生产脚本的 L4 机器断言（三架构工件）；**C `kernel.lds` 的 `AT()` 是静态阅读，无机器断言**；§6.2 的复算是一次性手工 |
| M2 | 两段式布局的段归属（§2.2） | `.unpaged_*` 段 VMA=LMA；改名名单决定归属 | 已覆盖 | L10 机械对账 `Makefile.inc` 的 `.for unpaged_obj` 名单与 `kernel.lds` 的三个 `unpaged_*.o` 输出段 |
| M3 | `head.S` 的成对换栈跳转（§2.3） | 设栈+跳转的次序、位移与绝对值各自的前提 | 部分覆盖 | 真机链（§5.2 三断言覆盖结果）；**C 侧次序本身无自动断言** |
| M4 | `k_initial_stktop` 落段（§2.4） | 栈符号落在高半段、栈区不与其他内容重叠 | 部分覆盖 | L10 断言 `mpx.o` 不在 unpaged 名单（因而 `k_initial_stktop` 落在高半 `.bss`）且 `head.S` 引用该标签；栈区与其他内容不重叠仍靠静态阅读 |
| M5 | Rust 生产脚本的判据（§3.1） | 生产脚本有 `AT()` 且常量与预期表一致 | 已覆盖 | L0/L3c/L4；无读者的 arch `link.ld` 形态已按 `D-107` 删除 |
| M6 | ELF 解析器的五道校验与 12 个变体（§3.2） | 每种畸形输入返回对应变体 | 已覆盖 | minix-elf 27 测试（五道校验/迭代器/字段/仿真）；`NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall` 由 boot-shim loader 的测试覆盖，不在 minix-elf 的 27 个里 |
| M7 | 按 `p_paddr` 摆放与 BSS 清零（§4.2） | 段体写到 `paddr`；NOBITS 区清零；重叠拒绝 | 部分覆盖 | loader 的 32 个测试中 28 个直接相关（`bss_zeroed`、`rejects_overlapping`、三个新守卫等，名单见 §5.1）；**`p_paddr` 物理写入只有真机链**，宿主只跑 buffer 孪生 |
| M8 | 三架构成对跳转的指令序列（§4.3） | 切栈+跳转按序、对齐、清帧指针、`delta` 处理 | 已覆盖 | 真机载体三架构（`sp_ok`/`sp_aligned`/`fp_zero`）；riscv64 另把 `pc_at_high` 并入 PASS 条件（R02-T03），低 PC 即报 `delta path broken` |
| M9 | 从装载到 `kmain` 的合流（§4.4） | 两条生产路径各自到达 `kmain` 并完成跳转 | 部分覆盖 | direct 面（riscv64 自引导）走 `arch_boot_impl`；v1 send/receive 走 `arch_boot_resume_v1`/`resume_after_v1_decode`，不经过 `arch_boot_impl`；两条都到 `kmain`，但入口不同 |
| M10 | 镜像跨距的 2MiB 收口（§3.5） | 跨距 2MiB 对齐、与 `bootface` 常量一致 | 已覆盖 | L5/L9（L9 三架构，按架构选 `bootface.rs`/`bootface_a64.rs`） |
| M11 | 交接的寄存器约定（§4.3、§4.4） | `kinfo` 经 `.bss` 副本交接（形参按 ABI 传入 RDI/X0/A0，生产 `kmain` 第一条语句即遮蔽为全局副本） | 部分覆盖 | NK4-A 的寄存器覆写修复与注释；**行为性，无自动断言** |
| M12 | 布局断言的持续成立（§5.3） | L0–L10 全过 | 已覆盖（手工复跑） | `check-layout.sh` 三架构实跑（本轮复跑全 PASS）；这是一次性复跑，不是常驻 CI 断言 |
| M13 | 生产 `kmain` 入口的 ABI 栈形（§3.4、§4.3） | x86_64 `call` 前 RSP 16 对齐、入口 `RSP ≡ 8 (mod 16)`；aarch64/riscv64 入口 SP 16 字节对齐 | 已覆盖 | 三份 `jump_to_kmain` 在生产路径上断言 `kern_stack_top` 16 字节对齐，x86_64 另断言 `(stack_top − 24) ≡ 8 (mod 16)`（R02-T04）；`kmain_verify` 的 `sp_aligned` 仍在载体侧复核入口值 |

本节已无未闭合的机制缺口；下一次新增机制时继续按同一格式补行。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状（可观察到的现象） | 怀疑对象 | 确认动作 |
|---|---|---|
| 取指失败、故障地址是低地址（出现时刻三选一：分页开启后立刻／恒等映射被移除时／首次切地址空间后——三个时刻的区分见 §2.1、§2.3、§4.1 与 §4.3 的反向推演） | 恒等映射被移除而 PC 仍在低半（§4.3 的 riscv64 `delta` 缺失）；或恒等映射从未建立 | 读 QEMU 异常日志里的故障地址与出错类型；核对该架构 `higher_half.rs` 的跳转目标算式 |
| 分页开启后立刻取指失败，故障地址是高地址 | 高半映射未建立或范围不足（`kern_vir_base`/`kern_size` 与 `map_huge` 的范围；不是 `AT()`——`AT()` 只改 `p_paddr`） | `readelf -lW` 看 `p_vaddr`/`memsz`；`check-layout.sh` 的 L3b/L5；核对 `arch_boot_impl` 的 Step 2 |
| 内核镜像里某段的内容像另一段 | `AT()` 用了常量表达式，段体在物理上互相覆盖 | `readelf -lW` 看多个 `LOAD` 行的 `p_paddr` 是否相同（§2.1 反向推演） |
| 进入 `kmain` 后第一次压栈/中断失败，故障地址是低地址 | 栈指针未切换（§2.3 的"只跳不切"）或栈顶值算错（§3.3） | 在 `kmain` 入口打印栈指针与 `kern_stack_top`；比对其高半一侧范围 |
| 分页还没开启就 panic，消息是 `no kernel page size satisfies the layout` | 镜像跨距不是大页粒度的整数倍（§3.5） | `readelf -lW` 算跨距；`check-layout.sh` 的 L5/L9；三个量（`kern_virt_base`/`kern_phys_base`/`kern_size`）自己比对 |
| 栈回溯出现与调用关系无关的地址 | 帧指针链的终止点缺失（§4.3 的清帧指针：`xor rbp` / `mov x29,#0` / `li s0,0`；`push 0` 的终止槽不在链上） | 检查 `higher_half.rs` 的 asm 块是否保留了清帧指针动作（§5.2 的 `fp_zero` 断言覆盖它） |
| 模块字节或模块名错位、内核报数量不符 | 装载顺序或名字契约被破坏（模块装载一侧，01 篇 §4.3） | 比对 boot-shim 的 `MODULE_NAMES` 与内核的 `BOOT_MODULE_PROC_NRS` |

**诊断注记（临时打印与早期读数）**：x86_64 生产路径上有两块临时打印（`kernel: arch_boot arg=…` 与 `kernel: preA rdi=…`，源码注释自标 `temporary`），它们不属于 §6.1 的正式路标；按本篇复现串口日志时按噪声过滤。这两块打印是诊断噪声；是否删除由后续清理决定，当前不影响判据（原登记为设计批 D2-13）。早期版本 `kmain` 收到的指针 `0x1ddda2ee` 与 `.rdata` 里一个字符串常量的地址相合，是 RDI 被覆写的现场读数；现行实现改为 `.bss` 副本交接（§4.4），这条读数只用于识别旧日志。

### 6.2 可复制命令

```bash
# 第一组：镜像自身的布局（不需要运行，任何架构的工件都能查）
readelf -hW  os/target/x86_64-unknown-none/release/kernel     # Type: EXEC；Entry point address
readelf -lW  os/target/x86_64-unknown-none/release/kernel     # LOAD 行的 VMA/LMA/Filesz/Memsz
# 关键行：所有 LOAD 行的第三列（VirtAddr）与第四列（PhysAddr）之差一致（L4 断言的这一条）。
#         当前工件的 LOAD 行数：三架构各 4 行（x86_64 的 .lrodata.* 已在 R4 收进 .rodata；
#         复算：readelf -lW <工件> | grep -c '^  LOAD'）。
#         4 行依次是 .text（RX）、.rodata（R，含 .lrodata 与 .eh_frame_hdr）、.data（RW）、.bss（RW、NOBITS）；
#         逐段对名：readelf -lW <工件> | sed -n '/Section to Segment/,$p'。
#         首段 PhysAddr 应当是链接脚本里的 KERNEL_PHYS_BASE。

# 第二组：脚本里的常量与表达式
grep -c ': AT(' os/kernel-image/*.ld
# 口径：只匹配段头里的 ": AT(" 表达式，不匹配注释里出现的 "AT()" 字样。
# 关键行：三份生产脚本各 4 条（.text/.rodata/.data/.bss）。
# 对照：若把模式放宽成 'AT('，x86_64 那份仍 4 条、另两份各 5 条（多出的一条来自头注释里的说明）。

printf '%d\n' $((0xF0400000 / 0x400000))        # 961：内核高半基址占第几个 4MB 页目录项
printf '0x%x\n' $((0xF0400000 - 0x00400000))    # 0xf0000000：C 侧的 _kern_offset

# 第三组：整批布局断言（会在目标目录里重新构建，需要对应 toolchain）
bash os/kernel-image/check-layout.sh x86_64     # 或 aarch64 / riscv64 / all
echo $?                                         # 0 = 全过；1 = 任一断言失败；2 = 用法/环境错误
# 关键行：每个断言行形如 "  [L4] PASS  所有 PT_LOAD 的 vaddr-paddr 平移量一致（= 0xffff800000000000，违例 0 段）"（PASS 后两个空格，与脚本输出一致）。
#         L4 的平移量按架构不同：x86_64 是 0xffff800000000000，aarch64 是 0xffff7fffbfe00000，riscv64 是 0xffffffbf7fe00000；不要拿 x86 的读数套另外两个架构。

# 第四组：真机链的异常追踪
os/qemu-tests/run_qemu.sh x86_64 <test-efi>     # 脚本内部用 -d int,cpu_reset -D /tmp/qemu_int.log
# 关键行：qemu_int.log 里的 check_exception 行给向量（出错类型）；故障地址在紧随其后的异常明细行（x86 看 CR2，arm/riscv 看对应字段）；
#         串口日志里的 ### TEST_RESULT: PASS test-higher-half ###。

# 第五组：GDB 远程调试（确认跳转后 PC 与 SP 各落在哪一侧；下面的 $rip/$rsp 是 x86_64 寄存器名，aarch64/riscv64 用 $pc/$sp）
# 给上一组的 QEMU 调用加两个调试开关：-s 开 gdbstub，-S 在第一条指令前暂停；
# 裸 QEMU 的起法（含各架构固件路径）见 `01-boot-shim-bootstrap.md` §6.4 的三层调试法，本节不重复。
gdb <被调试的工件>
(gdb) break kmain
(gdb) continue
(gdb) print/x $rip
(gdb) print/x $rsp
# 预期输出：$rip 与 `nm <工件> | grep kmain` 读出的符号值一致（v1 send/receive 与自引导 direct leg
#           都应当是高半地址；x86_64/aarch64 测试载体内联形态下是垫片镜像里的低地址，riscv64 测试载体是高半 VMA 但等于 `nm` 值加 `delta`，见下一条汇总与 §4.3 的第三种形态）；
#           $rsp 落在该架构的 kern_virt_base 之上（x86_64 是 0xffff8000… 一段；aarch64 是 0xffff8000… 一段（`aarch64.ld` 的 `KERNEL_VIRT_BASE = 0xFFFF800000000000`；`0xffff7fffbfe00000` 是 L4 平移量，不是栈地址）；riscv64 是 0xffffffc0… 一段），即 §4.3 的"切栈已生效"。
```

五组命令的预期输出：

- 第一组：`readelf -hW` 的 `Type:` 行应为 `EXEC`（不是 `DYN`），`Entry point address:` 应等于首段虚拟地址；`readelf -lW` 的所有 `LOAD` 行两列地址之差一致（= 高半平移量），首段 `PhysAddr` 等于脚本里的物理基址。
- 第二组：`grep -c ': AT('` 对三份生产脚本各输出 4。这条读数必须带口径：把模式放宽成 `AT(` 会把注释里的字样算进来（x86_64 仍 4、另两份各 5）。两条 `printf` 分别输出 `961` 与 `0xf0000000`。
- 第三组：全绿时最后一行是 `CHECK-LAYOUT=PASS（x86_64）`、退出码 0；任一断言失败时对应行是 `  [Ln] FAIL …` 且最后一行 `CHECK-LAYOUT=FAIL（…）`。
- 第四组：PASS 时串口日志里出现 `### TEST_RESULT: PASS test-higher-half ###`（判定标记见 `os/qemu-tests/run_qemu.sh` 的判定段）；失败时同一位置是 `### TEST_RESULT: FAIL test-higher-half ###`，它之前的若干行里，失败项以 `FAIL:` 开头（`SP not at high address`、`SP not at the architecture's entry alignment`、`FP not zero`，riscv64 另有 `PC not at high address (riscv64 delta path broken)`），而 x86_64/aarch64 的 `INFO: PC not at high address (expected in test kernel)` 是参考信息行、不参与成败判定，别把它读成失败原因（它只在失败分支里出现；riscv64 不打印这一行，PC 低半在 riscv64 是 `FAIL`）；`qemu_int.log` 里另有 `check_exception` 行给出故障类型与地址。
- 第五组：断在 `kmain` 后 `$rip` 应等于该腿的入口值——v1 send/receive 与自引导 direct leg 下等于 `nm <工件> | grep kmain` 的符号值（高半）；x86_64/aarch64 测试载体内联形态下是垫片镜像里的低地址；**riscv64 测试载体不是 `nm` 值本身**：它链接在物理地址上，`$rip` = `nm` 值 + `delta`（`delta = kern_virt_base − kern_phys_base`），例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000000160`；`$rsp` 应落在该架构的 `kern_virt_base` 之上（x86_64 `0xffff8000…`、aarch64 `0xffff8000…`、riscv64 `0xffffffc0…`）——两条读数合起来即"跳转与切栈都已生效"的现场证据。

---

## 7. 参见

### 7.1 前置与后续篇

- [01-boot-shim-bootstrap.md](../01-boot-shim-bootstrap.md) — 引导器契约、内存记账、页表建立与开启、布局断言 L0–L9、`KernelInfo` 的字段来源
- [03-kmain-cstart.md](../03-kmain-cstart.md) — `kmain` 入口与保护结构初始化
- [04-platform-discovery.md](../04-platform-discovery.md) — 平台描述符的解析
- [05-clock-interrupt-init.md](../05-clock-interrupt-init.md) — 时钟与中断控制器初始化
- [../02-stage-vm/07-pagetable-struct.md](../../02-stage-vm/07-pagetable-struct.md) — 页表多级结构与页表接口

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/arch/i386/{kernel.lds,head.S,mpx.S,Makefile.inc,pre_init.c,pg_utils.c,protect.c}`
- `minix3/minix/kernel/main.c`
- `minix3/minix/kernel/{const.h,proto.h}`、`minix3/share/mk/sys.mk`、`minix3/sys/arch/i386/include/asm.h`
- `minix3/minix/kernel/arch/i386/{include/arch_proto.h,exception.c,usermapped_data_arch.c,usermapped_glo_ipc.S}`、`minix3/minix/kernel/usermapped_data.c`
- `minix3/minix/lib/libsys/stacktrace.c`
- `minix3/minix/kernel/arch/earm/{kernel.lds,head.S,mpx.S}`
- `minix3/minix/servers/vm/pagetable.c`（[07 篇页表结构](../../02-stage-vm/07-pagetable-struct.md)的背景阅读，不在本篇论证链上）
- `minix3/minix/include/arch/i386/include/vm.h`

### 7.3 关联 Rust 源码清单

- `os/kernel-image/{x86_64,aarch64,riscv64}.ld`、`os/kernel-image/{build.rs,check-layout.sh}`
- `os/kernel-image/src/{main,bootface,bootface_a64,bootface_common}.rs`
- `os/kernel/src/boot/higher_half.rs`、`os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs`
- `os/kernel/src/{lib,boot_policy}.rs`
- `os/boot-shim/src/{loader,main,uefi_helpers,opensbi_helpers,uboot_main}.rs`
- `os/libs/minix-elf/src/lib.rs`
- `os/libs/minix-boot/src/{kernel_info,handoff_v1_read}.rs`
- `os/arch/src/arch/{boot,direct_map,stacktrace}.rs`
- `os/servers/rs/src/trap_api.rs`、`os/servers/vfs/src/exec_worker.rs`、`os/servers/vm/src/vm_server.rs`
- `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs`

### 7.4 构建与回归入口

- `os/kernel-image/check-layout.sh`、`os/qemu-tests/run_qemu.sh`
