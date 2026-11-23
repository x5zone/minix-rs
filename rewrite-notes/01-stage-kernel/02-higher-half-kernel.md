# 02 · 内核镜像：链接布局、装载摆放、运行期映射与进入高半

> 本篇的轴是**镜像这个二进制**：它的符号值从哪来、段体被放到哪里、运行期哪些地址可用、控制流怎样从低地址进入高半。
> 交接载荷（`KernelInfo`、内存账、bootstrap 资源、校验与发送侧）归 01 篇；本篇只在接缝处给一句指针。

## 1. 概述

同一份内核镜像在磁盘上只是一个 ELF 文件，但它要在四个不同时刻以四种身份出现：链接器看到的符号表、装载器写进物理内存的段体、页表建立后可访问的地址，以及最后跳进 `kmain` 的入口。本章要回答的核心问题是：这四个时刻之间靠什么保证不脱节？

### 1.0 本章导读

#### 学习目标

读完本章能：

- 说出内核镜像的四个时刻（链接、摆放、映射、进入），以及每个时刻的输入与产物。
- 用 J1–J4 四条不变量判断一次高半启动是否成立。
- 区分“符号值”与“可访问地址”：前者在链接期确定，后者要等页表建立。

#### 前置概念

- ELF 的 `PT_LOAD`、`p_vaddr`、`p_paddr`、`p_filesz`、`p_memsz` 的基本含义。
- 分页的最小直觉：虚拟地址要经过页表才能访问物理页；本章不要求会写页表项。

#### 最小示例

```bash
# 链接脚本给出一对基址；两者之差就是平移量（§1.1）
$ grep -n 'KERNEL_HIGH_BASE\|KERNEL_PHYS_BASE' os/kernel-image/x86_64.ld
# 内核侧的建根入口；四个时刻在这里汇合（§1.2）
$ grep -n 'fn arch_boot_impl' os/kernel/src/lib.rs
```

第一条给出高半 VMA 与物理基址，第二条给出“建表并进入”的代码位置；J1/J3 的判据都围绕这两处读。

#### 失败对照

如果把“符号值已经是高半 VMA”误当成“现在就能从高半取指”，就会在页表尚未建立时跳到 `kmain`：取指地址正确，但没有页表把它接到物理页，CPU 在第一个高半指令上取指失败。J3 要求映射先于进入，正是为了避免这种“地址对、时刻错”的故障。

#### 小结

- 同一份镜像经过链接、摆放、映射、进入四个时刻，前一时刻的产物是后一时刻的输入。
- J1 管平移一致，J2 管跨距与粒度，J3 管映射先于进入，J4 管换栈与跳转成对。
- 符号值在链接期确定，可访问性在页表建立后才成立。

#### 术语

- **VMA / LMA**：段的执行地址与装载地址；`p_vaddr` 与 `p_paddr` 分别描述它们。
- **平移量**：同一组段里 `vaddr − paddr` 的公共差值；C 侧用 `_kern_offset` 表示。
- **高半 / 低半**：内核执行地址所在的高地址区间，与物理内存所在的低地址区间相对。
- **恒等映射**：虚拟地址等于物理地址的一段映射，用于分页开启前的取指与数据访问。
- **换栈**：进入高半前把 SP 从低半栈切到高半栈的动作。

### 1.1 镜像的四个时刻

同一份内核镜像要经过四个时刻，才会从“磁盘上的 ELF 文件”变成“高半地址上运行的 `kmain`”：

| 时刻 | 谁决定 | 产物 | 判据 |
|---|---|---|---|
| 链接 | 链接脚本与链接器 | 每个符号的 VMA、每个可加载段的 VMA/LMA | 同一组 `PT_LOAD` 的 `vaddr − paddr` 相同（J1；C 侧低侧组与高侧组各自一致） |
| 摆放 | 外部装载器或自引导面 | 段体字节落在物理内存的 LMA | 首段 LMA = 物理基址；末段 memsz 收口（J1/J2） |
| 映射 | 建表代码（直核路径是内核镜像；v1 路径是 boot-shim） | 恒等窗口、内核高半映射、设备窗口 | 进入高半之前，取指与数据都已可访问（J3） |
| 进入 | 换栈与跳转代码 | SP 换到高半栈；PC 的落点按形态分（riscv64 高半，x86_64/aarch64 测试载体低半） | 成对、对齐、不返回（J4） |

这四个时刻不是四个独立话题：前一个时刻的产物是后一个时刻的输入。链接脚本把符号值写成高半 VMA、把段体字节的装载地址写成低半 LMA；装载器读 ELF 的 `p_paddr`，把字节放到 LMA；建表代码用 VMA 与 LMA 的平移量建立“高半虚拟地址 → 低半物理地址”的映射；最后一段汇编把 SP 切到高半栈、把 PC 跳到 `kmain` 的高半 VMA。任何一处单独正确、另一处错位，症状都出现在后一个时刻。

先看最容易被误读的一对概念：**符号值**与**可访问地址**。符号值在链接期就确定了，不需要页表；可访问性要等页表把它映射到物理内存才成立。分页开启之前，`kmain` 的符号值已经是高半 VMA，但 CPU 还不能从那里取指；分页开启之后，符号值没变，可访问性才成立。本篇文章反复用这条区分来判断“某个地址现在能不能用”。

### 1.2 四条不变量 J1–J4

| ID | 不变量 | 可观测判据 | 破坏后的第一现场 |
|---|---|---|---|
| J1 | 平移一致：同一组可加载段的 `vaddr − paddr` 相等。C 侧低侧 `.unpaged_*` 组平移为 0、高侧 `AT()` 组为 `_kern_offset`；Rust 生产镜像只有一组，L4 才要求全表相等 | `check-layout.sh` 的 L4（只对 Rust 生产镜像）；C 侧用 `readelf -lW` 分组核对 | 段体被写到错误的物理地址；符号值与实际字节错位 |
| J2 | 跨距与粒度（Rust 生产镜像）：镜像跨距 2 MiB 对齐，64 KiB 引导栈在末段；C 侧对应物只有 4 MiB 基址对齐与 4 KiB 内核栈，没有 2 MiB 收口 | L5/L9；`kernel_boot_stack_*` 跨度 64 KiB 且落在末段 | 页尺寸策略找不到可用档位；栈顶落进 `.bss` 中间 |
| J3 | 映射先于进入：进入高半之前，取指、栈、数据都已映射 | Step 1 恒等、Step 2 内核、Step 2b/2c 设备窗口、Step 3 `enable` 的次序 | 开分页后第一条取指或第一次压栈页错误 |
| J4 | 换栈与跳转成对且不返回 | 三份 `HigherHalf` 实现；入口 SP/FP 断言（PC 按架构判定：riscv64 并入 PASS，x86_64/aarch64 只记录） | 返回地址留在旧栈；SP 未切到高半；`ret` 跳到不可预期地址 |

J1 与 J2 是静态布局的性质，镜像还没运行就能检查；其中 J1 对 C 侧要按组读，J2 的 2 MiB 收口只对 Rust 生产镜像成立。J3 与 J4 是运行期性质，只能在真机链上观测。第 5 章开头给一张 J1–J4 → 具体小节的映射表；不把“宿主测试通过”当成“真机也通过”。

### 1.3 范围与接缝

本篇覆盖“从镜像文件到高半 `kmain`”这一段。以下内容不在本篇展开：

| 内容 | 归属 |
|---|---|
| `KernelInfo`/wire 字段、内存账来源、bootstrap 资源与发送侧 | `01-boot-shim-bootstrap.md` |
| `kmain` 之后的保护结构（GDT/TSS/IDT、VBAR、stvec） | `03-kmain-cstart.md` |
| 页表多级结构的完整定义 | `02-stage-vm/07-pagetable-struct.md` |
| 平台描述符解析、时钟与中断控制器 | `04-platform-discovery.md`、`05-clock-interrupt-init.md` |

**接缝**（与 01 篇的 1.3 节 同一套语义）：

- 01 管“取到字节、校验、记账、构造载荷、发送”；02 管“把字节解释成镜像、摆放、映射、进入”。
- `pre_init` 是接缝函数：`get_parameters` 属 01，`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步属 02。
- `head.S` 有两次跳转：第一次是 `load_stack_start → pre_init`（属 01），第二次是 `k_initial_stktop → kmain`（属 02）。
- L0–L10 的权威清单只在本篇 §5.2；五条生产腿的计数只在 01 篇。
- 栈顶名字拆开：`load_stack_start`、`user_sp` 归 01；`k_initial_stktop`、`kernel_boot_stack_top`、`kern_stack_top` 归 02。

本篇反复出现“高地址”这个词，它在两处含义不同：链接期算出的符号值，与运行期可访问的地址。区分方法已经在上文给出：符号值在分页开启前就确定，可访问性在分页开启后才成立。下一篇 `03-kmain-cstart.md` 从 `kmain` 的第一条指令开始，讲保护结构的建立。
---

## 2. C 源码分析

C 侧的实现是这套镜像语义的真值来源：链接脚本、`head.S`、`pre_init` 与页表建立代码共同决定了段体落在哪里、符号值是多少、什么时候才能从高半取指。本章逐段读这些 C 代码，目的不是复述实现，而是把 Rust 侧要保住的外部行为与需要重新设计的内部机制分开。

### 2.0 本章导读

#### 学习目标

读完本章能：

- 读懂 `kernel.lds` 的双地址布局：哪些段在低半执行，哪些段只按低半 LMA 摆放。
- 说明 `head.S` 的一次跳转里，取指地址、栈指针与页表各自处在什么状态。
- 复述建表五步与位表，指出每一步失败时会在哪个时刻暴露。

#### 前置概念

- 第 1 章的四个时刻与 J1–J4。
- x86-32 的段式寻址与分页开关的基本区别；本章只要求能读懂链接脚本与汇编的地址表达式。

#### 最小示例

```bash
# 低半执行段与高半段的交界；`_kern_offset` 是两组的平移量（§2.1）
$ grep -n '_kern_offset\|\.unpaged_' minix3/minix/kernel/arch/i386/kernel.lds
# 引导汇编里唯一一次从低半跳到高半的入口（§2.3）
$ sed -n '/multiboot_init:/,+20p' minix3/minix/kernel/arch/i386/head.S
```

第一条给出 `unpaged` 段与高半段的地址安排，第二条给出跳转前后 SP/PC 的写法；两者合起来才是“布局 → 摆放 → 映射 → 进入”的 C 侧证据。

#### 失败对照

如果高半段没有 `AT()` 把装载地址压回低半，链接器会把段体按 VMA 摆到高地址；外部装载器按 `p_paddr` 写到同样高的物理地址，而分页开启前的低半执行面没有这些字节。症状是分页刚开启就取指失败，但故障地址看起来“完全正确”——错的是装载地址，不是虚拟地址。

#### 小结

- C 用 `unpaged` 段保证分页开启前的执行面，用 `AT()` 把高半段的段体压回低半 LMA。
- `head.S` 的跳转同时完成取指地址切换与栈指针切换；两者缺一，症状不同。
- 建表五步的次序是 J3 的 C 侧原型：映射先于进入。

#### 术语

- **unpaged 段**：分页开启前必须能执行的代码与数据；链接在低半，VMA 等于 LMA。
- **`AT()`**：链接脚本里给输出段指定 LMA 的表达式；只改 `p_paddr`，不改 `p_vaddr`。
- **`k_initial_stktop`**：C 侧高半初始栈顶符号；换栈后 SP 落在它上面。
- **位表**：建表五步里描述页表项权限与粒度的位组合。

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

这条默认只适用于脚本显式写出的输出段。链接器自动生成的孤儿段不按这张表走：GNU ld 让它们继承相邻段的 LMA 平移，反例（`.lrodata.*`）与判据见 §4.1。

`AT(ADDR(.text) - _kern_offset)` 不是"把装载地址设成物理基址"——它是"把装载地址设成该段虚拟地址减去同一个常量"。两者只在某个段的虚拟地址恰好是 0xF0400000 时才相等，而本脚本里没有这样的段：低侧 `.unpaged_*` 三段长度非零（对象清单与改名机制见 §2.2），位置计数器加完偏移后已经大于 0xF0400000；接着 `.usermapped_glo` 与 `.usermapped` 两个非空输出段又各占一段、各自对齐到 4KiB（供料方是 [`i386/usermapped_glo_ipc.S`](minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S) 供 `.usermapped_glo`；[`i386/usermapped_data_arch.c`](minix3/minix/kernel/arch/i386/usermapped_data_arch.c) 的三个 `minix_ipcvecs_*` 对象与 [`kernel/usermapped_data.c`](minix3/minix/kernel/usermapped_data.c) 的 8 个对象都带 `__section(".usermapped")`，一起供 `.usermapped`）。`.text` 的虚拟地址因此严格大于 0xF0400000，它的装载地址也严格大于 0x00400000。真正成立的是逐段关系：每个带 `AT()` 的段，装载地址 = 自己的虚拟地址 − 0xF0000000；五处 `AT()` 减的是同一个常量，于是"虚拟地址 − 装载地址"对**每个带 `AT()` 的段**都相等；低侧 `.unpaged_*` 三段不带 `AT()`、平移量为 0——两组各自一致，不是全表一致。这条平移一致是后面一切映射的基础，`kernel.lds` 自己并没有断言它；Rust 侧把这条件写成了可执行检查（§3.1、§5.2）。

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

`mpx.S` 在这个名单里没有出现。它的对象（`mpx.o`）由另一条规则产出：`mpx.S` 只出现在同一个文件的普通源码清单里（`SRCS+= mpx.S arch_clock.c …` 一行），没有经过 `objcopy`。因此它的输出文件名是 `mpx.o`，链接脚本里 `unpaged_*.o(.text)` 这条通配匹配不到它——通配匹配的是**输入文件名**（`objcopy` 从不改段名，它只给符号加前缀），而 `mpx.o` 的文件名里没有 `unpaged_` 前缀。它落进普通的 `.text`/`.data`/`.bss`，也就是 §2.1 里位置计数器加过偏移之后的段。这一点在 §2.5 会被用到。

把"名字"这件事拆开看，这里有**两套互不相同**的机制，写法相似但作用对象不同：

| 机制 | 作用对象 | 产物 | 谁消费它 |
|---|---|---|---|
| 输出文件名前缀 `unpaged_` | 对象文件本身 | `unpaged_head.o` 等 27 个文件 | 链接脚本的输入文件通配 `unpaged_*.o` |
| 符号前缀 `__k_unpaged_` | 对象里的符号表 | `__k_unpaged_pre_init` 等符号 | `ENTRY(__k_unpaged_MINIX)`、以及被改名对象之间的相互引用 |

第三张表把几个代表性对象与它们的落点对上（本节与 §2.5 共用）：

| 对象 | 在改名名单里 | 段收在哪 | 执行时地址在哪一侧 |
|---|---|---|---|
| `head.o` | 是 | `.unpaged_text`/`.unpaged_data` | 低一侧（VMA = LMA） |
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

### 2.3 `head.S`：一次跳转里的两种寻址

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
2. `push $0`：往新栈里压一个 0。源码注释写作 `Terminate stack`——它是初始栈底的终止槽，与回溯链的终止点是两件事：本仓两侧的回溯器都沿**帧指针链**走（C 侧 `libsys/stacktrace.c` 的 [`util_stacktrace`](minix3/minix/lib/libsys/stacktrace.c:util_stacktrace) 从 `get_bp()` 出发 `while(bp)` 逐帧读 `[bp]` 与 `[bp+1]`；内核侧 `i386/exception.c` 的 [`proc_stacktrace_execute`](minix3/minix/kernel/arch/i386/exception.c:proc_stacktrace_execute) 与 Rust 侧 `stacktrace.rs` 的 [`walk_frames_from`](os/arch/src/arch/stacktrace.rs:walk_frames_from) 同形），正常情况下链走到保存帧指针槽里的 0 才停。C 侧入口段 `mov $0, %ebp` 在换栈前清零；`pre_init` 是普通 C 函数，EBP 是 callee-saved，`pre_init` 返回时把它恢复成 0，因此换栈后调用 `kmain` 时保存槽里同样是 0。Rust 侧把 `xor rbp, rbp` 放在换栈之后（§4.4），且生产 `kmain` 的序言不保存 `%rbp`（本仓未启用 `-C force-frame-pointers`），链从 `kmain` 内的回溯起点直接读到 0。两侧都在 0 处终止，差别只在清零发生的位置与是否存在保存槽。边界：清零只保证入口起点的终止；未启用帧指针时，Rust 侧不保证调用链深度可回溯（panic 路径能打出的帧数取决于当时的寄存器与手工回溯实现）。两侧回溯器另有 `saved_fp <= fp`（C 写作 `hbp <= bp`）的防环终止，只在链被破坏时先于 0 触发；压进栈的这个 0 槽不在帧指针链上，本仓两侧的回溯器都没有读这个槽的路径——它只作为初始栈的终止槽保留（Rust 侧保留它的理由与消费边界见 §4.4）。
3. `push %eax`：把 `pre_init` 的返回值（`kinfo` 的地址）压栈。i386 的调用约定从栈上取参数，所以这次压栈就是给即将被调用的 `kmain` 准备第一个参数。
4. `call _C_LABEL(kmain)`：跳转。`call` 在机器码里是一条相对调用：位移在链接期算好，运行时 CPU 用"当前 PC + 位移"得到目标。位移的值是链接器按**两个链接地址**之差算出来的（调用点在 `.unpaged_text` 里，目标 `kmain` 在 `.text` 里），两个链接地址一低一高，差值在 0xF0000000 量级。把补码这笔账算一遍：`call` 的位移定义为"目标地址 − 下一条指令地址"，即 `S − (P + 5)`。代入布局：高侧的位置计数器在低侧三段之后才加 `_kern_offset`（`kernel.lds` 的 `. += _kern_offset` 排在 `.unpaged_bss` 之后），所以 `kmain` 的虚拟地址是 `0xF0400000 + 低侧总长 + 高侧段内偏移`，而调用点在 `.unpaged_text` 里、地址是 `0x00400000 + 调用点偏移`。两者相减：位移 = `0xF0000000 + (低侧总长 + 高侧段内偏移 − 调用点偏移 − 5)`。低侧总长与高侧偏移的差值是一个小量，所以位移的位模式在 `0xF0000000` 附近；精确的低 16 位取决于链接后的段长与 `kmain` 偏移，可用 `nm` 复核，这里只需要量级。32 位有符号位移的可表示范围是 −2^31..2^31−1；`0xF0000000` 按补码读作 `0xF0000000 − 0x1_0000_0000` = `−0x1000_0000`（约 −268 MB），位模式仍在范围内。关键是 CPU 在 32 位模式下把位移加到 EIP 后按 2^32 取模回绕：负位移加到低地址 `0x0040xxxx` 上会回绕到 `0xF040xxxx`，所以高半与低半之间的距离能塞进一条 32 位相对调用，不需要额外指令。

于是这两条指令对"执行视图"的敏感度不同：第 1 条携带绝对值，无论 CPU 在哪个视图中执行，写进 `%esp` 的都是 0xF04xxxxx 一侧的值；第 4 条的相对位移要加上运行时的 PC 才成为目标地址，而调用点所在的 `.unpaged_text` 是 VMA = LMA 的段——它在链接时的地址就是装载后的地址，也就是 CPU 此刻真正执行的那个地址。位移加上这个 PC，落到 `kmain` 的链接地址上。两条路径殊途同归，都停在 0xF04xxxxx 一侧（`k_initial_stktop` 是否在同一侧的判据见 §2.5）。

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

两种架构的两个动作在这里分工相同：先设栈（绝对值），再跳转（x86 靠位移+PC，ARM 靠绝对值）。§1.1 四时刻表“进入”行的前提——栈指针指向的高半地址已经可访问——不是这两条指令建立的；可访问性由 `pre_init` 的五步建表在此之前完成。

**三条推演（按"缺哪一步"分：只切栈不跳；只跳不切；把切栈挪进 `kmain` 内部）**：

其一，只切栈不跳。`%esp` 是 0xF04xxxxx 一侧的值，而 PC 停在 `.unpaged_text` 的下一行（`head.S` 里 `pre_init` 之后的兜底循环：`hang: jmp hang`，就在 `call kmain` 的下面）。此刻恒等映射还在，取指正常，看起来"没坏"；但 `jmp hang` 使内核停在这个兜底循环，主流程不再前进，VM 服务不会运行——延迟页错误的后果不可达。直接症状是停机，不是页错误。

其二，只跳不切。`call` 会把返回地址压进当前栈——`load_stack`（`head.S` 自己 `.data` 段里的 4096 字节：`load_stack` 起、`load_stack_start` 止——设栈取的是后者）。`kmain` 的函数序言、参数保存、后续每次 `push`/`pop` 都发生在这个低地址栈上。低地址栈所在的物理页在引导结束后被交回空闲链、又分配给别的用途之后，第一次栈访问读到被改写的数据。映射还在（`pg_identity` 铺满 4 GiB），所以这里不是页错误，而是静默损坏；这一处与上一处的区别是：上一处是停在 hang 停机（内核不再前进），这一处坏在数据访问，而数据访问在中断发生时也会发生（中断处理要压栈）。

其三，把"切栈"挪到 `kmain` 内部作为第一步。`kmain` 是 C 函数，进入它之前编译器生成的序言已经动过栈（保存帧指针、调整栈指针），也就是在切栈之前至少已经有一次对旧栈的写。要让"切栈发生在任何压栈之前"，它就必须排在 `call` 之前——`head.S` 正是这么排的，设栈与 `call` 之间隔着两条 `push`——`push $0`（终止槽）与 `push %eax`（给 `kmain` 准备的参数）。

### 2.4 建表五步与位表

`pre_init()` 的函数体共八条语句：一条断言 + 一条收集调用 + 五步建表/开启 + 一条 `return`；下面说的"前一条收集、后五条建表与开启"指中间那六条。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:pre_init。
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
| 1 | [`pg_clear`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear) | 静态页目录数组 `pagedir[1024]` 清零 | 无 |
| 2 | [`pg_identity`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity) | 把低地址全部映射成恒等（VA = PA） | `kinfo.mem_high_phys` |
| 3 | [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) | 把内核镜像映射到它的高虚拟基址，返回第一个空闲页目录下标 | 链接期常量 + `pagedir[]` |
| 4 | [`pg_load`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_load) | 把页目录物理地址写进 `CR3` | 页目录地址 |
| 5 | [`vm_enable_paging`](minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging) | 置 `CR0.PG`、`CR0.WP` 与 `CR4.PSE`、`CR4.PGE`（`PGE` 按 CPU 能力置） | `CR0`/`CR4` 当前值 |

步骤 2 与步骤 3 写同一张数组，这是它们必须有序的机制来源。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity。
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

- 函数开头的 `assert(cbi->mem_high_phys)` 只能证明“至少一次 `add_memmap` 已产生非零上界”，不能证明“清单已切完”；完整的“清单先于建表（含切除）”由 `get_parameters` 内的调用序保证，不由这条断言保证。它挡住的是“反排到 `add_memmap` 之前”这种更粗的错序——那时该值还是 `.bss` 的零初值。
- `I386_VM_DIR_ENTRIES = 1024`、`I386_BIG_PAGE_SIZE = I386_PAGE_SIZE * I386_VM_PT_ENTRIES = 4096 * 1024`（4MB，定义在 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)）。两个数一乘正好是 4 GiB——对 32 位来说就是**整个地址空间**。所以这个循环不是在算"该映射哪些"，而是**一次把 1024 项全部填完**：在 4MB 大页下这是最省事的做法，代价固定（只要一个页目录，不需要任何页表页）。

  不要把“省事”当成“必须”。真正必需的覆盖只有两块——当前正在执行的地址与内核要去的地址，它们由 1.2 那两条讲过；全铺是达成它的廉价手段，不是它本身的要求。本篇的 64 位实现正是反例：恒等映射不再取固定上界铺满低 4 GiB，而是只覆盖 boot footprint。具体窗口来源、`BOOT_IDENTITY_MAP_END` 的 VA 上限语义与 `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]` 标注统一放在 §4.3，本节不复制设计清单。

  全铺也带来了代价，方向与"更安全"相反：内存上界（4MB 对齐后）及以上的不存在地址也被标成 present，只是额外加上 `PWT|PCD` 变成非缓存，所以那一段越界访问不会触发页错误，而是读到不确定的内容——相对"只映射真实内存"的方案，全铺不具备"越界即页错误"这条性质；C 本来也没有这条性质（`PWT|PCD` 不产生页错误），Rust 的差异是未登记空洞走可缓存（窗口外未映射的差异见 §4.3）（下面几条接着说）。
- 每个条目都置 `I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_USER | I386_VM_WRITE`：存在、是大页（该条目直接指向 4MB 物理块而不是二级页表）、用户可访问、可写。这里的 `I386_VM_USER` 要说清一句：boot 阶段内核在 Ring 0 执行，U/S 位不参与内核自己的数据访问判定，所以**这一位在 C 里没有实际效果——前提是 `CR4.SMEP = 0` 且 SMAP 不拦数据访问**（SMEP 禁止 supervisor 从 U=1 的页取指；SMAP 禁止 supervisor 对 U=1 页的数据访问，除非 `AC=1`；C 的 `vm_enable_paging` 在 **CR4** 里只清 `PGE|PSE`，不碰这两位（CR0 的 `PG` 另行清除，见本节下文的解码），所以这条依赖固件没有置 SMEP/SMAP）。Rust 侧的恒等映射不置 USER，正是为了不依赖这个前提。有意义的对照是 `pg_mapkernel` **不置**它（内核地址被标成 supervisor-only），而恒等映射也不会留给用户态用——VM 的 `pt_init` 跳过 BIGPAGE 条目、自己新建页目录。新设计里的对应选择（boot 用的恒等映射不置该位）见 §4.3。
- 条件比较的是"内存上界截到 4MB 边界"与"本条目起始地址"：`(mem_high_phys & I386_VM_ADDR_MASK_4MB) <= (phys & I386_VM_ADDR_MASK_4MB)`。注意 `I386_VM_ADDR_MASK_4MB` 的值是 `0xFFC00000`，它是**清掉低 22 位、向下取整到 4MB**（不是向上），且右边的 `phys` 本来就是 4MB 的整数倍、那半边掩码是空操作；真正起作用的是把内存上界抹到边界上。成立时额外加 `I386_VM_PWT | I386_VM_PCD`。源码注释给了理由："We map memory that does not correspond to physical memory as non-cacheable."
- 向下取整在边界上是**保守（多包一圈）**而不是漏包：设内存上界是 `0x2001000`（32MB 加一页），抹到边界得 `0x2000000`，于是 8 号条目（起点正好 `0x2000000`）满足 `<=` 而被标成非缓存——**尽管它开头那一页是真内存**。反方向取整就会漏：那一段里真正不存在的地址会被当成普通可缓存内存，才是真危险。所以这个方向的取舍是"宁可把一段真内存当设备慢一点访问（结果仍正确），也不把设备或空洞当内存去缓存"。
- `PWT`/`PCD`（页写穿透 / 页禁缓存）的用途：那些地址后面不是 DRAM，而是设备寄存器或空洞。设备读写的语义依赖"每次访问都真的到达设备"；若走缓存，读可能命中缓存里没有被设备更新过的旧值，写可能在写缓冲区里停留。所以"超出物理内存上界"这个条件至少圈出内存上界之上那一整圈（设备寄存器与紧贴上界的高端空洞）——上界之下的空洞与保留区不在此列（细表分支下它们进账时就被丢弃，见 01 篇的 2.4 节；粗档分支下保留区可能在账上并被 `pg_alloc_page` 分出——本篇以细表分支为主干，粗档差异见 01 篇的 2.4 节 的推演）；但“不分配”不等于“运行期不访问”，可缓存安全性依赖同一前提（见 §4.3）。
- 这个函数把 4 GiB 全部置为 PRESENT。所以"有没有映射"在这里不构成区分；缓存属性两位只在（4MB 对齐后的）内存上界及以上这一段参与区分"真实内存"与"设备/空洞"，上界之下的空洞与真实内存没有位级区分。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel。
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

- `kern_vir_start`/`kern_phys_start`/`kern_kernlen` 是文件顶部的静态量，初值来自链接脚本导出的 `_kern_vir_base`/`_kern_phys_base`/`_kern_size`（`_kern_vir_base = 0xF0400000`、`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 01 篇的 3.5 节）。它们为什么这里要用文件级变量而不是参数：同一个文件里两个函数签名不同——`pg_identity` 收一个 `kinfo_t *`，而 `pg_mapkernel(void)` **不收参数**，它要拿到"内核在哪、多大"就只能读文件级静态量。拿不到参数也要能建表，这件事也印证了同一句话：**建表所需的布局信息不属于引导器给的数据**。
- 两条断言要求两个基址都按 4MB 对齐。索引从 `kern_vir_start / I386_BIG_PAGE_SIZE` 开始：`_kern_vir_base = 0xF0400000`（`minix3/minix/kernel/arch/i386/kernel.lds`），除以 4MB 得下标 `961`。
- 循环把内核镜像按 4MB 一块地映射进高地址区。条目里**没有** `I386_VM_USER`——内核地址对用户态不可访问，这是与恒等映射故意相反的一处。
- 返回值是循环结束后的 `pde`，也就是内核映射占完之后第一个未被占用的页目录下标。注释写作 `free pde`。C 侧的下游属 VM 页表初始化，本篇不展开。

**把 `pg_identity()` 排到 `pg_mapkernel()` 之后会怎样**：两个函数写同一个 `pagedir[]`，索引集合**重叠**：`pg_identity` 写 0..1023 全部 1024 个条目，`pg_mapkernel` 写从 961 开始的若干条目（`_kern_vir_base = 0xF0400000` 除以 4MB）。先写内核映射、后写恒等映射，961 号及之后的若干条目会被"恒等映射 + 4MB 大页"覆盖：`pagedir[961]` 从 `0x00400000 | …`（`_kern_phys_base`）变成 `0xF0400000 | …`（961 × 4MB），两者相差 `_kern_offset = 0xF0000000`，**不相等**。切换到高地址执行时，高半基址 `0xF0400000` 起的映射被 961 号条目解释成物理地址 `0xF0400000`——该物理块不含内核镜像；若它在内存上界之上（被 `pg_identity` 标了 `PWT|PCD`），取指不会产生页错误，若它仍在内存上界之内则是 DRAM、只取到非内核字节——两种都不报错，CPU 随后执行到非预期指令，失败方式不确定。`pg_identity` 覆写下标 0–1023（含 1023）；其中有害的是内核镜像占据的 961 到 `961 + ceil(kern_size/4MB) - 1`。次序错在这里不会报"映射冲突"，也不会在第一条指令上给出可定位的页错误。

第三个函数（本篇逐个解码的三个函数里）是唯一改 `CR0`/`CR4` 模式与特性位的那个；装根（`CR3`）在上一步 `pg_load` 已完成：

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
- **先清，再置**，而且清掉的位里有一个要等到最后才回来：先清 `PG`、清 `PGE|PSE`，随后只重设 `PSE`（4MB 大页成立的前提），再开 `PG`，最后才按 CPU 能力置 `PGE`。**这个先后不能倒**：`pg_identity` 与 `pg_mapkernel` 写的每一条表项都带 `I386_VM_BIGPAGE`（bit 7），而 `PSE = 0` 时这一位是保留位——先开 `PG` 就等于让 CPU 拿着一张每一位都非法的页目录去取指。
- 注释给的两条理由分别是："PGE 必须在 PG 之后启用"与"Our page table contains 4MB entries"（PSE 必须为 1，4MB 大页才成立）。
- `WP`（写保护）在开 `PG` 之后单独置一次。它的效果是"内核态也不能写只读页"，是 W^X 的基础。
- `pgeok` 来自 CPU 能力检测（`_cpufeature(_CPUF_I386_PGE)`）。有能力的 CPU 才置 `PGE`：该位为 1 时，带 G 位的页表项在切换 `CR3` 后仍留在 TLB 里。

> `CR0` 与 `CR4` 是 x86 的控制寄存器，用法都是"读回来、改位、再写回去"。`CR0` 说"机器处在什么模式"，`CR4` 说"启用哪些扩展特性"。上面涉及的四位：`CR0.PE`（bit 0）保护模式，由引导器负责置好、内核只断言它已置；`CR0.PG`（bit 31）分页总开关，置起后每一取指、每一访存都要过页表；`CR4.PSE`（bit 4）大页使能，它允许页目录项直接指向一个 4MB 物理块而不是下一级页表；`CR4.PGE`（bit 7）全局页，配合表项的 `G` 位让映射在切换 `CR3` 时不被冲刷。各位的位值与一处头文件注释错误，见 §2.4 的表。

#### 2.4.1 建表用到的位、掩码与常量

下面三张表（位/常量、地址宏、CR0/CR4 位）把本篇出现的位与掩码集中在一处，值全部取自 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)。

| 常量 | 值 | 含义 | 本篇哪一步用到 |
|---|---|---|---|
| `I386_VM_PRESENT` | `0x001` | 条目存在 | `pg_identity`、`pg_mapkernel`、`pg_map` 写入的每个条目（`pg_map` 是 C 的 4KB 细粒度映射函数，见 `pg_utils.c`） |
| `I386_VM_WRITE` | `0x002` | 可写 | 同上 |
| `I386_VM_READ` | `0x000` | 只读（不置 `WRITE` 即只读） | — |
| `I386_VM_USER` | `0x004` | 用户态可访问 | `pg_identity`、`pg_map`（`pg_mapkernel` 不置，理由见本篇 §2.4 的 `pg_identity` 条目解码） |
| `I386_VM_PWT` | `0x008` | 写穿透 | `pg_identity` 对非 DRAM 区间 |
| `I386_VM_PCD` | `0x010` | 禁缓存 | 同上 |
| `I386_VM_ACC` | `0x020` | 已访问（由 CPU 置位） | 硬件写，本路径不置 |
| `I386_VM_DIRTY` | `1 << 6` | 已写（由 CPU 置位） | 同上 |
| `I386_VM_BIGPAGE`（同值写法 `I386_VM_PS`） | `0x080` | 本条目直接指向 4MB 物理块 | `pg_identity`、`pg_mapkernel` |
| `I386_VM_GLOBAL` | `1 << 8` | 全局页（换 `CR3` 后留在 TLB） | 本路径不置 |
| `I386_VM_ADDR_MASK` | `0xFFFFF000` | 取 4KB 对齐的物理地址 | `pg_map` 挂页表项 |
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

CR0/CR4 表里有一处注释与位号不符，按值与用法判断：`vm.h` 里 `I386_CR0_WP` 那行的注释写的是 `/* Enable paging */`，与下一行 `I386_CR0_PG` 的注释逐字相同——注释在这一行是从相邻行复制来的，`WP` 的含义要看 `vm_enable_paging` 里它被置的时机（开 `PG` 之后单独置一次）。

### 2.5 `k_initial_stktop` 落在哪一段

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

i386 侧的结论是落点在高一侧：`k_initial_stktop` 的值是 0xF04xxxxx 一段的某个地址。这条依据链只在本篇给全：`01-boot-shim-bootstrap.md` 的 1.4 节 只交代边界并声明落点判定归本篇，它的 §2.8 直接引用本篇的结论，两篇的结论一致。名字上带 `__k_unpaged_` 字样**不是**判据：这个字样在定义处是手写的，在引用处是 `objcopy` 加的，两处都不是"它进了 unpaged 段"的证据；把它当线索会得出相反的结论。

**假设栈定义落在 `.unpaged_bss`（低一侧）会怎样**：`head.S` 把 `%esp` 设成低地址值，`kmain` 在高地址执行而栈在低地址。在“栈定义落在 `.unpaged_bss`”这个假设下，栈所在的物理页属于 `_kern_unpaged_start..end`（即 `bootstrap_start/len`），引导结束后会被 `add_memmap` 交回空闲内存。恒等映射在 `pg_identity` 生效期间还在，所以首次访问不会立刻页错误；恒等映射在后续切根时被移除之后，第一次访问是页错误；若映射仍指向该物理页、而页内容被复用作别的用途，才是静默损坏。两种后果不要合并成一种。第二重后果与第一重同源：`.unpaged_bss` 覆盖的那段物理内存会被交回空闲内存——`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 在 `system_init()` 之后调用 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`，源码注释写明 `The bootstrap phase is over, so we can add the physical memory used for it to the free list.`。栈所在的页会进入空闲链，被下一次分配拿走；同时 `pg_mapkernel` 只映射 `[kern_vir_start, kern_vir_start + kern_kernlen)`，不覆盖低地址这一段。两件事叠起来：栈的可用性依赖恒等映射在切根前仍有效，又依赖这段物理内存不被复用；前者在后续切根时不再成立，后者在 `add_memmap` 后随时可能发生。

Rust 侧不再用链接符号给出 `kmain` 用的栈顶（入口 `_start` 仍用 `kernel_boot_stack_top`），改为由接收方从 `kern_virt_base + kern_size` 现算（访问器 `KernelInfo::kern_stack_top()`，决策见 §3.3）。

### 2.6 本章归纳：C 的“布局-摆放-映射-进入”四段

| 四个时刻 | C 里的产物 | 本节的结论 |
|---|---|---|
| 链接布局 | `kernel.lds` 的三行常量与 `AT()` 表达式 | 带 `AT()` 的普通段：符号值高一侧、装载地址低一侧，差是 `_kern_offset`；`unpaged_*` 三段的符号值与装载地址都在低侧 |
| 装载摆放 | `Makefile.inc` 的改名规则 + 外部装载器（GRUB 等）按 `p_paddr` 写段体（树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位，见 01 篇的 2.2 节） | 段体落在低一侧；哪些对象进低一侧由改名名单决定 |
| 运行期映射 | `pg_clear`→`pg_identity`→`pg_mapkernel`→`pg_load`→`vm_enable_paging` 五步（§2.4） | 先恒等后内核高半，最后才开分页；把 `enable` 提前会取指/压栈落空，把 `pg_identity` 排到 `pg_mapkernel` 之后则内核条目被恒等条目覆盖、取指读到非内核字节（§2.4） |
| 成对进入 | x86-32：`head.S` 的四行（设栈、压终止槽、压参数、相对 `call`）；ARM32：`head.S` 的设栈 + `ldr`/`bx` | x86-32 的设栈携带绝对值、跳转携带相对位移；ARM32 两者都是绝对值；i386 侧已按 §2.5 证据落地，ARM 侧同一判据也已静态核实：`earm/Makefile.inc` 的改名名单不含 `mpx.o`、[`earm/mpx.S`](minix3/minix/kernel/arch/earm/mpx.S:k_initial_stack) 在 `.bss` 后切回 `.data`、`earm/kernel.lds` 的 `.unpaged_data` 只收 `unpaged_*.o`，故 `k_initial_stack` 落在高半 `.data` |

影响“进入”这一时刻的架构差异有三处：入口段的段名（ARM 多一个 `.startup`）、跳转指令的写法（x86 用相对 `call`，ARM 用字面量池取值的 `bx`）、`k_initial_stack` 的段归属（i386 在 `.bss`，`earm/mpx.S` 在 `.data`）；另有两处差异不影响结论——物理基址取值与改名名单（§2.2），它们不改变四个时刻的判据。两边的语义相同：先设栈、再跳转；i386 侧两个动作指向同一侧，ARM 侧同一判据已由上述三处源码核实。

---
## 3. Rust 设计决策

Rust 侧不能照搬 C 的两份页目录与借槽机制：镜像仍是同一个 ELF，但建根、开分页、换栈的载体全变了。本章要回答的问题是：在保留 J1–J4 四条不变量的前提下，哪些 C 结构必须换、换到什么程度、换掉之后由谁承担原来的保证。

### 3.0 本章导读

#### 学习目标

读完本章能：

- 说出 Rust 侧三种链接形态，以及每种形态由谁按装载地址摆放段体。
- 解释为什么 ELF 解析器要自建，而不是复用 C 的 `exec_elf.c`。
- 把栈顶、镜像跨距、大页能力、自举根与成对跳转分别落到 J1–J4 的哪一条。

#### 前置概念

- 第 2 章的 C 侧布局与建表五步。
- Rust 的 trait、泛型与 `const` 基本用法；本章不要求读懂所有实现细节。

#### 最小示例

```bash
# 生产镜像脚本的基址常量；三种形态的第一种（§3.1）
$ grep -n 'KERNEL_HIGH_BASE\|KERNEL_PHYS_BASE' os/kernel-image/x86_64.ld
# 自建解析器的入口；宿主测试与三个消费者共用（§3.2）
$ grep -n 'pub fn segment_iter' os/libs/minix-elf/src/lib.rs
```

第一条确认生产镜像带 `AT()` 与低半 LMA，第二条确认解析器是仓内单一实现；后续的栈顶、跨距与跳转都建立在这两处之上。

#### 失败对照

如果把生产镜像的 `AT()` 脚本直接套到测试载体上，载体的链接地址与装载地址会同时被改写，而载体没有外部装载器去读 `p_paddr`；结果是“脚本看起来更统一，运行时却按错误地址取指”。三种形态必须按“谁摆放”选择，不能按“看起来是否整齐”选择。

#### 小结

- 三种链接形态分别对应外部装载、载体自执行与工具链默认布局；选择依据是“谁读 `p_paddr`”。
- 自建 ELF 解析器把装载语义收进一个 crate，宿主测试与三个消费者共用同一套错误集。
- 栈顶是派生值，镜像跨距要满足映射粒度，自举根在活动根上追加，跳转与换栈成对收进 trait。

#### 术语

- **三种形态**：生产镜像带 `AT()`、载体单地址脚本、载体工具链默认布局。
- **`SegmentIter`**：自建 ELF 解析器对 `PT_LOAD` 的迭代器；错误以 `Err` 出现，不静默跳过。
- **`kern_stack_top`**：由 `kern_virt_base + kern_size` 派生的栈顶，不再作为 wire 标量传递。
- **自举根**：分页开启时使用的第一张根页表；后续 Direct Map 在它上面追加映射。
- **成对跳转**：换栈与跳 PC 必须一起完成，且不返回。

### 3.1 链接脚本按“谁按装载地址摆放”分三种形态

C 侧的负担：内核镜像有两个地址，而"按装载地址摆放"这件事由内核之外的代码执行——`AT()` 表达式把期望的装载地址写进 ELF 的 `p_paddr` 字段，装载器读它。Rust 侧这个外部角色有两个候选：一个外部装载器（UEFI 路径的 boot-shim 从文件系统读 `kernel.elf`，或 U-Boot 预载后交给 boot-shim），以及**自引导形态的引导器**（RISC-V：U-Boot 的 `bootelf` 或 `-kernel` 直载按 `p_paddr` 摆放镜像——摆放者是引导器；镜像本身是引导体，但不重拷自己的段，§4.5）。谁读 `p_paddr`，谁就需要脚本里有一个低地址的 LMA；没有读者的时候，脚本多写一套地址只会多一处需要维护的不变量。

按“谁按装载地址摆放”只有三种形态：**生产镜像带 `AT()`**（外部装载者按 `p_paddr` 摆放）、**载体单地址脚本**（载体自己按链接地址执行，没有 `p_paddr` 读者）、**载体工具链默认布局**（没有链接脚本，一套地址）。两种 `AT` 表达式写法只是第一种形态内部的语法差异。

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

三份脚本的地址写法有两种：x86_64 的位置计数器起点是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`（首段的虚拟地址里直接含有物理基址，`AT` 表达式减去 `KERNEL_HIGH_BASE` 就得到物理地址）；aarch64 与 riscv64 的起点是 `KERNEL_VIRT_BASE`，`AT` 表达式写成 `AT(ADDR(sec) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)`（[`aarch64.ld`](os/kernel-image/aarch64.ld)、[`riscv64.ld`](os/kernel-image/riscv64.ld)）。两种写法保证的是同一条不变量：所有段的"虚拟地址 − 装载地址"相等。这条不变量由 `check-layout.sh` 的 L4 断言检查（断言清单在本篇的 5.2 节；本节只说它与 `AT()` 的因果关系）。

`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` 那套无生产链接读者的副本与只锁常量的 lockstep 测试都不在树内；生产布局只由 `os/kernel-image/*.ld` 承载。载体（QEMU 测试内核；其中一部分目录带自己的 `link.ld`，其余沿用工具链默认布局）在带脚本的形态下各自用单地址脚本，以合成的 `BootReport<'_>` 走进同一段引导代码。

第二种形态的实例在载体侧：[`link.ld`](os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld) 只有一套地址（`. = 0x80200000`），段没有 `AT()`，收尾一行 `__stack_top = . + 0x10000`。载体被固件放在低地址执行，不需要两套地址。第三种形态（没有链接脚本、沿用工具链默认布局）用于其余测试载体；它与第二种形态的共同点是只有一套地址，区别是脚本是否存在。

三种形态的判据归结为一件事：**有谁按 `p_paddr` 摆放到与符号地址不同的位置，才需要低地址的 LMA；摆放位置与符号地址一致（或没有读者）时，一套地址就够。**

**生产交接形态（v1 handoff）**：UEFI（x86_64/aarch64）与 riscv64 的 U-Boot 腿都走 `prepare_and_enter_kernel_image`/`prepare_boot` → `enter_kernel_image_v1` → 独立镜像 `_start` → `arch_boot_resume_v1`。接收侧只做 additive 阶段：把 arena 里的六块载荷（memmap/reserved/modules/platform/param/sources）落进本镜像 `.bss`、记录已活动根、建 Direct Map 覆盖，然后跳 `kmain`；它不重建 bootstrap 根，因为 `new_from_page` 会把正在使用的活动根清零。direct 面（riscv64 自引导、aarch64 direct-core）走 `bootface`/`bootface_a64` → `arch_boot` → `arch_boot_impl`，在物理视图里建根、开分页、建 DM；riscv64 再用条件 delta 跳 `kmain`，aarch64 用 `ldr` 的绝对值跳。测试载体分两类，不是一个视图：x86_64/aarch64 测试载体走 inline `arch_boot`、是低地址视图；riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 3 行）。两类都不是生产路径。

三套分类不是同一件事，后面的表按这张登记表换算：

| 链接脚本形态 | 装载路径 | 进入/执行视图 | 跳转完成时 PC |
|---|---|---|---|
| 生产镜像（带 `AT()`） | v1 send/receive（UEFI x86_64/aarch64、riscv64 U-Boot） | 高半视图 | 高半 VMA |
| 生产镜像（带 `AT()`） | riscv64 自引导（`arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta） |
| 载体单地址脚本 | riscv64 测试载体（直接 `arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta） |
| 生产镜像（带 `AT()`） | aarch64 direct-core（`bootface_a64` → `arch_boot`） | 低地址进入、物理视图 | 高半 VMA（`ldr` 取绝对值） |
| 载体工具链默认布局 | x86_64/aarch64 测试载体（inline `arch_boot`，UEFI helpers） | 低地址视图 | 低地址 |

“需要 `delta`”只落在 riscv64 的 direct leg；v1 receive、aarch64 UEFI 的 v1 跨镜像跳转、x86_64 `call` 都不需要。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 生产镜像：脚本用 `AT()` 给出低地址 LMA，交外部装载器摆放 | 选（`os/kernel-image/*.ld`） | 装载器必须按 `p_paddr` 写（[`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory) 就是这么写的）；脚本要维护"段间平移一致"这条不变量，并靠 `check-layout.sh` L4 兜住 |
| 载体：脚本只给一套地址，x86_64/aarch64 载体在入口时于低地址执行（riscv64 载体低装高进，见上表第 3 行） | 选（测试载体） | 内核符号值与运行地址之间没有平移关系，`BootReport<'_>` 的基址必须由载体自己填对；riscv64 载体仍按 direct leg 加 `delta` |
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

四个量里前三个由外部装载器（boot-shim 的两个 helper）从 ELF 的 PT_LOAD 段现算（`loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout)），不是从脚本里读常量；例外是自引导形态——它的三个量是与脚本同源的手写常量，三架构都由 L9 与 `readelf` 实测等值对账（x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`；x86_64 走这个分支只为校验镜像跨距与常量副本，不存在直核面入口，见 §4.5；§3.5）。除这个例外，脚本与读者之间没有代码依赖，只有约定。

**给 [`x86_64.ld`](os/kernel-image/x86_64.ld) 去掉 `AT()` 会怎样**（保留高半基址）：四条 `AT` 都去掉之后，每个 `PT_LOAD` 的 `p_paddr` 退化成虚拟地址，段拷贝会向 `0xFFFF800000200000` 一类物理地址写字节。宿主侧先红的是 `check-layout.sh` 的 L3c（"首段 LMA = 0x200000"），真机侧的表现是内核字节从未被放到可执行位置。

### 3.2 ELF 解析器自建

C 侧的负担：装载器要按 `p_paddr` 摆放、按 `p_memsz - p_filesz` 清零，这些都是 ELF 程序头里的字段；C 侧外部 multiboot 装载器（如 GRUB）读它们，树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位，内核自己一行解析代码都没有。Rust 侧把 C 内联在引导器里的这段解析搬到独立 boot-shim：boot-shim 与内核镜像分属两个可执行体，装载发生在 shim 内、由 shim 自己按 `p_paddr` 摆放（§3.1 的表把 shim 记为装载方）。因此这些字段由 shim 用本节的共享解析器读，内核侧一行解析代码也没有；与 C 的差别不在“有没有外部装载者”，而在装载者是本仓代码、与内核共用同一份解析器。需要的字段只有几个：`p_paddr`、`p_vaddr`、`p_offset`、`p_filesz`、`p_memsz`、`p_flags`（`p_flags` 在装载内核镜像时用不到，在装载用户 ELF 时要用，见本节末的处置表）。符号表、重定位、section header 都不在射程内。

解析器放在 `os/libs/minix-elf`，是一个独立 crate，被五个 crate 依赖（复算：`grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` 输出 `os/arch/Cargo.toml`、`os/boot-shim/Cargo.toml`、`os/servers/vfs/Cargo.toml`、`os/servers/vm/Cargo.toml`、`os/servers/rs/Cargo.toml` 五行）。用途按调用点分五类：boot-shim 用它装载**内核镜像**（按 `p_paddr` 逐段摆放），并用 `elf_file_end` 把模块 ELF 修剪到最后一个 `PT_LOAD` 的文件尾后整块拷贝（只修剪到最后一个 `PT_LOAD` 文件尾，不重定位、不逐段映射）；`boot.rs` 的 [`fn load_vm_elf`](os/arch/src/arch/boot.rs:fn load_vm_elf) 用它把用户 ELF 逐页映射进进程地址空间；[`trap_api.rs`](os/servers/rs/src/trap_api.rs) 的 exec 路径用它读段与入口点；[`vm_server.rs`](os/servers/vm/src/vm_server.rs) 的 bootstrap 装载用 `entry_point` 与 `segment_iter` 取入口与段表。vfs 的 exec 路径只在常量上借用它（`ET_EXEC`/`PT_LOAD`），表头解析是 [`exec_worker.rs`](os/servers/vfs/src/exec_worker.rs) 自带的 `ElfHead`。同一份"段迭代"语义被多个调用方共用，是它成为共享 crate 而不是某个模块内部实现的理由。

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

迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出。`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法（D2-01：debug 下 panic，release 下可能回绕后再由 `next()` 报 `Err`）（`lib.rs` 的 [`fn next`](os/libs/minix-elf/src/lib.rs:fn next)）：

```rust
/* 节选（保留 next() 的类型过滤与四类 Err 返回；注释有改写；省略号处为返回值构造），os/libs/minix-elf/src/lib.rs:fn next。 */
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

迭代器只把**非 `PT_LOAD` 类型**当作正常过滤（`continue`）；其余畸形一律以 `Some(Err(..))` 浮出：表项偏移溢出/越界是 `PhdrOutOfBounds`，`parse_phdr` 失败按具体变体返回，段文件字节越界是 `SegmentOutOfBounds`，`filesz > memsz` 是 `FileszExceedsMemsz`。调用方必须在第一个 `Err` 上短路：`compute_kernel_layout` 以 `seg_result?` 短路并把段表收进 `KernelImage`；`load_segments_into_phys_memory` 只接受 `ValidatedKernelImage`，构造这个类型的 `validate` 已经跑过段形检查。`SegmentIter::new` 与 `parse_phdr` 的表范围算式仍是未检查加法（D2-01 的真实残留：debug 下 panic，release 下可能回绕后再由 `next()` 报 `Err`），但“畸形段被静默跳过”这个旧缺口已经关闭；仍然开着的是“消费者若忽略 `Err` 项会继续遍历坏镜像”，所以每个调用点都必须短路。

错误分类是枚举而不是整数（`lib.rs` 的 [`enum ElfError`](os/libs/minix-elf/src/lib.rs:enum ElfError)），共 **12 个变体**，按产生层分四组：文件头五道 → `TooShort`、`BadMagic`、`Not64Bit`、`NotLittleEndian`、`NotExecutable`；程序头表两道 → `PhdrOutOfBounds`、`PhdrEntryTooSmall`；迭代器段形两道 → `SegmentOutOfBounds`、`FileszExceedsMemsz`；loader 三道 → `NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall { needed_end, buf_len }`。`filesz > memsz` 由迭代器返回 `FileszExceedsMemsz`，不再走 `InvalidSegment`；`InvalidSegment` 的返回点不止三个：`compute_kernel_layout` 的 `paddr + memsz` 的 `checked_add`、`kern_end_phys - kern_phys_base` 的 `checked_sub`、段间物理重叠三处之外，`segment_spans` 还拒绝 `paddr + filesz`、`mem_end < file_end` 与两处 `usize::try_from`，`load_segments_into_buffer` 路径另有边界检查（见 §4.2）；因此不能把“三个”读成穷举。源/目标重叠仍走 panic（与段间重叠的 `InvalidSegment` 不对称，是 D01-R9 的故意选择）。`TargetBufferTooSmall` 由缓冲区装载器的两循环产生（先验后写，不半装载）。`InvalidSegment` 的归一化仍会丢失“哪一段/哪个地址”的现场；把解析错误与装载错误进一步分层属于设计批 D2-06。

`LoadSegment` 的字段在两个装载场景下的用法不同，这张表把每个字段的去向列全：

| 字段 | 装载内核镜像：摆放用 [`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory)，取基址与跨距用同文件 `:fn compute_kernel_layout` | 内核装载用户 ELF（[`fn load_elf_into`](os/arch/src/arch/boot.rs:fn load_elf_into)） |
|---|---|---|
| `paddr` | 拷贝目标；`compute_kernel_layout` 也用它算 `kern_phys_base` 与 `kern_size` | 不使用：物理页由分配器选，PA 与 VA 无固定关系 |
| `vaddr` | 只进 `compute_kernel_layout` 取最小值 | 映射的目标虚拟地址（逐页映射） |
| `offset` 与 `filesz` | 拷贝来源与长度 | 拷贝来源与长度 |
| `memsz` | BSS 清零的上界（`memsz - filesz` 个字节） | 段的页数上界，页内余量也清零 |
| `flags` | 不使用（内核镜像的权限由内核建表时自己定） | 转成页表权限（`elf_flags_to_page_flags`） |
同一份解析结果、四种去向，差异全部来自"这一侧是谁在摆放"。

**把迭代器的步进从 `e_phentsize` 改为结构体大小会怎样**：ELF 规范里 `e_phentsize` 是文件自己声明的表项大小，允许大于结构体的最小尺寸（56 字节）。用 `size_of::<Elf64Phdr>()` 步进，在表项恰好 56 字节的产物上结果相同；一旦某个链接器写出更宽的表项，第 2 项起的读位置全部落在错误偏移上，`p_type` 读到的是上一个表项尾巴的内容，症状是"段数对得上、地址全错"或"某些段被当成非 `PT_LOAD` 跳掉"。依据：`SegmentIter::next` 的 `offset` 计算式是 `e_phoff + index * e_phentsize`，而 `parse_phdr` 的越界检查用的也是 `entsize`。

### 3.3 栈顶从链接符号变成派生值

C 侧的负担：`k_initial_stktop` 是一个链接符号，`head.S` 用一条绝对立即数指令把它的值写进 `%esp`（§2.3）。这个做法把"栈顶是多少"与"链接脚本怎么摆段"绑在一起：换一个布局，符号值跟着变。Rust 侧如果照搬，内核就要在源码里声明一个外部链接符号，而"内核被链接进谁"有三种形态（§3.1）——符号的值在每种形态下由不同的脚本决定，且测试载体根本没有这个符号。

改为按值派生：栈顶派生只需要两个值——`KernelInfo.kern_virt_base` 与 `KernelInfo.kern_size`（wire 另带 `kern_phys_base` 作为第三个布局标量；它不是栈顶算式的一部分）；栈顶由消费点在需要时现算（`kernel_info.rs` 的 `impl KernelInfo` 的 [`fn kern_stack_top`](<os/libs/minix-boot/src/kernel_info.rs:fn kern_stack_top>)）。wire v10 之前这里曾是一个独立字段 `kern_stack_top`，每腿都要填 `kern_virt_base + kern_size`；该字段与另外两个派生标量（`user_sp`/`syscall_entry`）一起删除，公式只留下访问器这一份，不再需要发送侧/接收侧锁步：

```rust
/* 节选（保留访问器体），os/libs/minix-boot/src/kernel_info.rs（`impl KernelInfo`）。 */
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

同族的 `syscall_entry` 也已在 wire v10 删除：生产路径从来只把 `minix_arch::syscall_entry_va()` 当入口，字段是恒等式的第二份抄写；删除后验证/序列化路径不再读它，`configure_syscall` 的唯一读者是各架构入口，而不是交接字段。

Rust 侧还有第二个栈：链接脚本在 `.bss` 里预留的 64 KiB 引导栈，符号写作 `kernel_boot_stack_bottom` 与 `kernel_boot_stack_top`（三个架构的 `os/kernel-image/*.ld` 同形），入口汇编用它立栈（[`main.rs`](os/kernel-image/src/main.rs) 的 `global_asm!` 里 `lea rsp, [rip + kernel_boot_stack_top]` 等四条序列：x86_64、aarch64、riscv64 direct、riscv64 U-Boot）。两个值不一样：`kernel_boot_stack_top` 是预留区的顶（它与 `__bss_end` 在脚本里相邻赋值，取同一个值），`kern_stack_top` 是收口后的镜像尾，等于前者加上收口填充量（填充为零时两者相等）。分工是"入口汇编用前者、进入 `kmain` 前切到后者"。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 栈顶由接收方从 `kern_virt_base + kern_size` 派生（`KernelInfo::kern_stack_top()`） | 选 | 装载方只需把跨距算对（RISC-V 自引导面把跨距写成了常量，靠 L9 与 ELF 实测对账）；公式只留在访问器一处，三形态通用 |
| 内核自己声明链接符号（照搬 C） | 否 | 符号值随链接形态变化；测试载体没有这个符号，合成 `BootReport<'_>` 的路径无法替换它 |
| 由内核在运行时从自己的段边界符号算 | 否 | 内核早期没有段边界信息（那是装载方读 ELF 才知道的），且这些符号的可见性依赖链接形态 |

换栈这件事与栈的大小无关，与栈**运行期落在哪一侧**有关。C 侧可以直接比：`head.S` 的临时栈 `load_stack` 是 `.space 4096`，`mpx.S` 里给 `kmain` 用的 `k_initial_stack` 也是 `.space K_STACK_SIZE`（4KiB，§2.5）——两次设栈的容量相同，差别只在地址从低一侧换到高一侧。Rust 侧：入口 `_start` 的 64KiB 引导栈的链接符号 `kernel_boot_stack_bottom/top` 在 `.bss`、高半；`kmain` 用的 `kern_stack_top` 不是链接符号，而是消费点按 `kern_virt_base + kern_size` 现算的高半虚拟地址。运行期 `_start` 用 `lea rsp,[rip+...]`（x86_64）/`la sp,...`（riscv64），在物理视图下取到的是低物理地址，在高半视图下才是高 VMA；换栈的目的，是把 SP 从入口栈（可能仍在物理视图）移到镜像尾的高半栈，而不是笼统的“从低侧换到高侧”。

切换之后栈的容身空间也是可以算的：栈向低地址增长，起点是 `kern_stack_top`（收口后的镜像尾）；从它往下到 `kernel_boot_stack_top` 之间是收口填充（0 到 2MiB 减 1 字节不等），再往下是脚本预留的 64KiB 引导栈区（`kernel_boot_stack_bottom` 到 `kernel_boot_stack_top`）。这两段都在 `.bss` 的 `memsz` 内；装载方按 `[filesz,memsz)` 清零，入口汇编清到 `__bss_end`。收口填充位于 `__bss_end` 之后，自引导面不保证清零；它只作为栈空间使用，不需要预清零。再往下是内核全局数据（`.bss` 的其余内容），不属于栈。

**去掉镜像跨距的 2MiB 收口会怎样**（§3.5 的 `. = ALIGN(0x200000)`）：与本节的交接值相关的一面是——填充量变了，`kern_size` 随之改变、`kern_stack_top` 的算式跟着变（栈顶 = 镜像虚拟基址 + 跨距）。收口缺失的完整后果链（粒度选择落空、规划阶段 panic）与宿主复现留在 §3.5；判据在 `loader.rs` 的 [`fn compute_kernel_layout`](os/boot-shim/src/loader.rs:fn compute_kernel_layout)（跨距的定义）与 `bootface.rs` 的 [`const KERN_SIZE`](os/kernel-image/src/bootface.rs:const KERN_SIZE)（自引导面对同一跨距的常量副本，L9 对账）。

### 3.4 `HUGE_PAGE_SIZE` 与大页能力进扩展 trait

C 的建表两个函数都直接用 4MB 大页：`pg_identity` 给每个页目录项置 `I386_VM_BIGPAGE`，`pg_mapkernel` 也置同一位（2.6 已贴）。大页在这套架构上不是可选能力，而是"1024 项 × 4MB = 4 GiB"这套单级页目录布局的必要条件（限定语是"只建一张页目录就覆盖 4 GiB"这个布局目标；对自举真正需要覆盖的两段地址，§2.4 已说明全铺只是省事手段、不是必须）：单个页目录项直接指向 4MB 物理块、不需要再挂页表页，一级页目录就能覆盖 4 GiB。

64 位侧情况不同：层数变多，每个架构支持的大页尺寸集合与"怎么表示这是一个大页"都不同。Rust 侧把这部分能力放进一个扩展 trait（`paging_ext.rs` 的 [`trait HugePages`](<os/arch/src/arch/paging_ext.rs:trait HugePages>)）：

```rust
// 代码引用（节选；`supports_huge_page` 的第二段文档注释省略），os/arch/src/arch/paging_ext.rs:trait HugePages。
pub trait HugePages: Paging {
    /// Supported huge page sizes, sorted largest first.
    const HUGE_PAGE_SIZES: &'static [PageSize];

    /// Preferred huge page size for Direct Map.
    const HUGE_PAGE_SIZE: PageSize;

    /// Fallback huge page size when the preferred size is not available.
    const FALLBACK_HUGE_PAGE_SIZE: PageSize;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    /// Whether this architecture/CPU supports the given huge page size.
    fn supports_huge_page(size: PageSize) -> bool;
}
```

这里没有默认实现，也没有第二个能力入口：每个架构必须显式回答自己的页大小能力（x86-64 在 `supports_huge_page` 内查 `CPUID.80000001H:EDX.GBPAGES`，ARM64/RISC-V 按自己的翻译模式回答）。能力事实由 `BootPageSupport` 快照收集，内核镜像的选页由纯规划函数完成，`map_huge` 对这段映射只执行已经验证过的写入。Step 1 的恒等窗口是另一条路：它按文件级常量 `PageSize::MIB_2` 分片，不经 `BootPageSupport`/`supports_huge_page`；当前三架构的 4 KiB 翻译模式都支持 2 MiB 块，所以这条硬编码成立，但它不是“策略层已验证”的一部分。

四条理由（1）–（4），逐条对应可检查的事实：

（1）`HugePages: Paging` 是**超集**关系：boot 阶段用 `A: BootArch` 一个 bound 就能同时取到 `A::Paging`（`new_from_page`/`enable`）、`A::DirectMap` 的窗口常量与 `A::DmInstaller` 的装叶实现（`arch_boot_impl<A: BootArch>` 就是这么写的；大页入口经 `HugePages` 另取）。运行时只用 `P: Paging` 的消费者不必携带大页负担。
（2）标识方式因架构而异，但标识位不进入 trait。x86-64 靠条目的 PS 位（bit 7）区分大页与"指向下一级表"，`map_huge` 在实现内用局部 `huge_flag` 取它；ARM64 与 RISC-V 没有这一位。把标识位提成"三个架构都要提供、其中两个恒为 0"的 trait 常量，等于把架构差异写进共有接口，所以它留在实现里。
（3）大页是**可选能力**而不是基础能力。x86-64 的首选是 1 GiB、回退 2MB；把 `map_huge` 放进 `Paging` 会让所有 `Paging` 实现都必须实现它，包括那些不打算支持大页的实现。
（4）大页能力不塞进 `Paging`：`Paging` 本身已有 24 个成员（`const PAGE_SIZE` 与 23 个方法：`phys_addr_limit`、`validate_virtual_range`、`new`、`new_from_page`、`from_active_root`、`inherit_supervisor_half`、`enable`、`destroy`、`is_supervisor_va`、`is_inherited`、`ensure_writable_range`、`map`、`remap`、`unmap`、`update_flags`、`query`、`split_huge`、`grant_user_walk`、`root_paddr`、`flush_tlb`、`flush_tlb_addr`、`map_range`、`unmap_range`），职责面并不窄；这里列出的 `new_from_page`/`from_active_root`/`map`/`unmap`/`query`/`enable` 只是与本节论证直接相关的六项。基础 trait 已经承担页表生命周期与范围操作，不应再把可选的块尺寸策略压进去。

被否决的方案：

| 方案 | 否决理由（对应上面哪条） |
|---|---|
| 把 `HugePages` 的常量面（`HUGE_PAGE_SIZES`/`HUGE_PAGE_SIZE`/`FALLBACK_HUGE_PAGE_SIZE`）移入 `Paging` | 违反第 2、3 条：尺寸阶梯是可选能力，放进基础 trait 等于让所有 `Paging` 实现都声明自己支持大页；标识位留在实现内，正因为它不是三架构共有的常量（`map_huge` 方法本身不在这次搬移范围内） |
| 另立一个只服务 boot 的 `PageTableBoot` trait | 与 `Paging::map/query` 重复——同一套硬件操作出现两个 trait，调用方要同时满足两个 bound |
| 不定义 trait，直接把 `map_huge` 写成具体类型的方法 | boot 路径无法用泛型统一三种架构；`arch_boot_impl<A>` 的 `A::Paging` 会退化成三个 `#[cfg]` 分支 |

x86-64 的实现给出这套设计的两个可核对点（`paging.rs` 的 [`fn map_huge`](<os/arch/src/x86_64/paging.rs:fn map_huge>) 的常量与 `fn map_huge`）；标识位不出现在常量里，而是在 `map_huge` 内取局部：

```rust
// 代码引用，os/arch/src/x86_64/paging.rs:fn map_huge 的常量与守卫段。
    const HUGE_PAGE_SIZES: &'static [PageSize] = &[PageSize::GIB_1, PageSize::MIB_2];
    const HUGE_PAGE_SIZE: PageSize = PageSize::GIB_1;
    const FALLBACK_HUGE_PAGE_SIZE: PageSize = PageSize::MIB_2;
...
        let huge_flag = X64PteFlags::PS.bits();
...
        if size_bytes == 1 << 30 {
            if e3 & X64PteFlags::PRESENT.bits() != 0 {
                return Err(PageTableError::AlreadyMapped);
            }
...
        // A 1 GiB leaf blocks the finer install.
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
```

- `HUGE_PAGE_SIZES`/`HUGE_PAGE_SIZE`/回退尺寸的阶梯属于内核纯策略层；选页阶梯与错误分支的完整摘录在 §3.5，本节只保留 trait 的尺寸能力声明。
- `AlreadyMapped` 守卫处理的是"目标块已被占用"的两类情形：1 GiB 分支先拒绝任何已存在的 PDPT 条目（注释见 `if size_bytes == 1 << 30` 段）；2 MiB 分支先拒绝已存在的 1 GiB 叶（"A 1 GiB leaf blocks the finer install."），再拒绝已存在的 2 MiB 叶。这条守卫也回应本节的问题本身：扩展 trait 承担的不只是尺寸查询，还有"不得把已有大页降级、也不得重复安装同一个叶"这条非平凡契约——若把 `map_huge` 收进基础 trait，所有 `Paging` 实现都得背上它，哪怕该架构根本没有 1 GiB 叶。

**去掉 `AlreadyMapped` 守卫会怎样**：直接按 2MB 路径往下走。`e3` 里原本存的是 `paddr | PRESENT | PS`（因为 1 GiB 大页的 PDPT 条目就是叶条目），代码会把 `e3 & ADDR_MASK` 当成下级 PD 页的物理地址。接下来 `read_pte_dm(pd + i2*8, ...)` 与 `write_pte_dm(pd + i2*8, ...)` 访问的是 `(e3 & ADDR_MASK) + i2*8`——也就是那个 1 GiB 物理块的基址加上偏移（这里的 `pd` 与 `map_huge` 的形参 `paddr` 同名不同物；`paddr` 只作为写入值参与）：这段内存正被 1 GiB 映射覆盖，可能就是内核自己的数据页。写完之后 1 GiB 映射还在（条目没变），但它的前 4KB 已经被当成页表改写。守卫的代价是"这次 2MB 映射失败"，去掉守卫的代价是"被映射的那段物理内存头部被写坏"。

### 3.5 镜像跨距收口：布局要满足映射粒度

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
| 三个量（`kern_virt_base`、`kern_phys_base`、`kern_size`）都被 1GiB 整除 | 架构的大页（三架构的 `HUGE_PAGE_SIZE` 都是 1GiB） | 内核映射用 1GiB 块；当前没有任何架构满足这条——三个物理基址（x86_64 `0x200000`、aarch64 `0x40200000`、riscv64 `0x80200000`）都不是 1GiB 对齐（aarch64/riscv64 的虚拟基址虽然对齐，但 `kern_size` 约 4MiB 本身不是 1GiB 的整数倍） |
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

RISC-V 自引导面把跨距写成了常量 `0x40_0000`（4 MiB，`bootface.rs` 的 [`const KERN_SIZE`](os/kernel-image/src/bootface.rs:const KERN_SIZE)），并在 `arch_boot` 之前使用；`check-layout.sh` 的 L9 拿 `readelf` 实测的跨距与这个常量对账（§5.2）。工件当前跨距以 L9 的实测输出为准，不以常量注释里的说法为准。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| 链接脚本收口到 2MiB，运行时按阶梯选粒度 | 选 | 镜像尾部有最多 2MiB 的填充；换来粒度选择永不落进 panic 分支 |
| 不收口，运行时按实际 `kern_size` 用 4KiB 细粒度建表 | 否 | 内核镜像映射约 1024 个页表项（4 MiB ÷ 4 KiB = 1024），按 512 项一张表算约 5 张页表页（2 张末级页表页加各级目录页），这些页要从引导期 bump 区里出（`01-boot-shim-bootstrap.md` §3.4 的换手路径）；建表时间与池容量都要重算 |
| 收口到更大粒度（1GiB） | 否 | 镜像远小于 1GiB，填充浪费以百 MB 计；且 RISC-V 的当前档位是 2MiB |

**把收口改为 4KiB 对齐会怎样**：`kern_size` 不再是 2MiB 的整数倍，粒度选择落进 `else` 分支，panic 发生在任何页表写入之前——现场是"分页还没开就停"，消息只报 `no kernel page size satisfies the layout`，不点名三个量；三个量要用 `readelf -lW` 自己算。根因在链接脚本的段间距上，不在调用方传参上。把 [`riscv64.ld`](os/kernel-image/riscv64.ld) 的 `. = ALIGN(0x200000)` 删掉、其余不动，第一道门是链接期：`SECTIONS` 之外那条 `ASSERT((__image_end - KERNEL_VIRT_BASE) % 0x200000 == 0, …)` 直接失败，根本产不出镜像。要到达运行期 `plan_kernel_mapping` 的 panic，必须连这条 `ASSERT` 一起删；两者都删之后才轮到宿主上的 `check-layout.sh riscv64` 报 L5"跨距非 2MiB 对齐"、L9 跟着红。

---

### 3.6 自举根与映射建立的设计（J3）

本节只讲建根次序（J3）；接收侧“根已由谁建好、由谁只做 additive”的分工见 §3.8。


J3 的要求只有一句：**在 CPU 从高半取指之前，取指地址、栈地址和即将读取的数据都必须已经有映射**。把这句话拆成建表次序，就是四步：

| 步骤 | 建什么 | 为什么必须在这个位置 |
|---|---|---|
| Step 1 | 低地址恒等窗口（VA = PA），含当前执行代码、栈与设备窗口分片 | 分页刚开启时 PC 仍在低地址；这一段没有映射，下一条取指就 fault |
| Step 2 | 内核高半映射：`[kern_virt_base, +kern_size)` → `[kern_phys_base, +kern_size)` | `kmain` 与内核全局都在高半 VMA；跳转目标必须已可访问 |
| Step 2b/2c | 设备窗口的高半别名（LAPIC/IO-APIC、GIC/PLIC/UART） | 设备寄存器按物理地址访问；用户根或内核根缺窗口时，第一次中断写入就 fault |
| Step 3 | `enable`：写 CR3/satp/TTBR + SCTLR，执行屏障 | 前三步只改内存里的表；这一步才让 CPU 开始用新根 |

次序不是实现偏好，而是“当前指令在哪、下一步要访问哪”的直接结果。C 侧的建表次序见 §2.4（`pg_identity` 全铺 4 GiB，因此 C 不存在“低半无条目”这一场景）；Rust 侧的反序后果见 §4.3：先映射高半再开分页，开分页的瞬间 PC 还在低半，取指 fault；只映射可用内存、不映射 loader 代码与栈，切换后的第一条指令或第一次压栈同样 fault。

设计上还要回答“谁来建根”。生产路径有两类：

- **v1 发送/接收腿**：boot-shim 在 `ExitBootServices` 之后、跳转之前建 bootstrap 根并开启分页（U-Boot shim 腿没有 EBS，在建表分类之后做同一件事），然后把独立镜像跳到高半 `_start`；接收侧只做 additive 阶段（记录活动根、落 `.bss`、建 Direct Map、跳 `kmain`），**不重建根**。重建会 `new_from_page` 清零正在使用的活动根，下一次取指立即 fault。
- **直核自引导腿**：镜像自己就是引导体，`arch_boot` → `arch_boot_impl` 在物理视图里建根、开分页、建 DM，再跳 `kmain`。此时 Step 1 的恒等窗口必须覆盖镜像正在执行的物理地址与入口栈。

`AlreadyMapped` 是这条设计里的安全阀：建表接口在目标槽已有映射时拒绝覆盖，而不是静默改写。它挡住两类错误：同一虚拟地址被两个来源映射到不同物理页；以及 1 GiB 叶与已有 2 MiB 子项冲突时把整槽顶掉。冲突要在建表时停下，不能等到取指。



### 3.7 成对的换栈与跳转收进一个 trait 契约

C 侧的负担：换栈与跳转必须成对、且有固定次序（§2.3 的三处反向推演），而这两个动作在 C 里只是相邻的四行汇编（设栈、压终止槽、压参数、跳转），没有任何类型或接口约束它们。调用者（`pre_init` 返回后的那几行）与实现（同一个文件）在一起，所以约束靠位置相邻来维持。

Rust 侧把它写成一个 trait，前置条件写进文档与 `unsafe` 契约，返回类型写成 `!`（`higher_half.rs` 的 [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf)）：

```rust
/* 节选（仅保留 trait 与方法签名；模块与条目说明段未摘录），os/kernel/src/boot/higher_half.rs:trait HigherHalf。 */
pub trait HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> !;
}
```

方法上方的 Safety 段列了三条前置条件：①分页已启用且恒等映射与内核高映射都已建立；②`kinfo` 在跳转这一刻的活动视图里可读（v1 接收面是高地址引用，直核面在分页开启前传物理视图引用；生产 `kmain` 只比较指针、不解引用，消费的是 `.bss` 副本）；③只从引导 CPU 调用一次。栈顶不再是形参：三份实现都在函数体里用 `kinfo.kern_stack_top()` 现算，与 `kmain` 收到的启动信息同源。下表按顺序对上这三条（第 1、2、4 行），第 3 行记的是**返回类型**（`-> !`）——它与三条并列，但**不是前置条件**：

| 前置条件 | 谁保证 | 违反时的落点 |
|---|---|---|
| 分页已启用、两套映射已建立 | 调用者（direct 面是 `arch_boot_finish` 里 `arch_boot_impl` 的返回点；v1 面是 `resume_after_v1_decode`（三架构同名壳，公共体 `resume_additive`）） | 取指或压栈落在无映射地址 |
| 跳转传入的指针与 `kmain` 消费的副本 | 调用者保证传入地址可读；生产 `kmain` 第一条语句即遮蔽为 `.bss` 副本，形参只做交接、不解引用；测试载体若直接解引用则需自行保证 | 生产路径解引用副本时页错误；载体解引用形参时页错误 |
| 不返回 | 类型：`-> !` 让"返回"在编译期不成立 | 无法表达（不返回是签名的一部分） |
| 栈顶 16 字节对齐 | `boot_validate_and_prepare` 断言来源值；三份实现另在本地断言 asm 入口 ABI（x86_64 另查 `(top − 24) ≡ 8 (mod 16)`） | 未对齐时 `call`/`ret` 的 psABI/AAPCS64 前提破坏 |
| 只调用一次 | 调用者（Safety 段的第三条写明；类型系统不区分调用次数） | 第二次调用会在已经切换的栈上再切一次，落点不可预期 |

栈顶的 16 字节对齐不再是调用者要传对的值：`boot_validate_and_prepare` 的断言覆盖现算的 `kern_stack_top`，三份实现拿到的就是同一个值（`kinfo.kern_stack_top()`），形参与断言可能各说各话的那条缝已经消掉。三份实现另在本地断言 `stack_top & 0xF == 0`（x86_64 另查 `(stack_top − 24) ≡ 8 (mod 16)`）：上游断言查“来源值对齐”，本地断言查“asm 入口 ABI”，两处职责不同，不是同一不变量的重复。删除形参的动机是“第二来源”：只要它还以参数形式存在，一个与 `kinfo` 不一致的值在类型上完全合法，编译器和测试都不会拦住。

三架构的实现各自放在自己的模块里（`higher_half.rs` 的 [`struct X86_64HigherHalf`](os/kernel/src/arch/x86_64/higher_half.rs:struct X86_64HigherHalf)、`higher_half.rs` 的 [`struct AArch64HigherHalf`](os/kernel/src/arch/aarch64/higher_half.rs:struct AArch64HigherHalf)、`higher_half.rs` 的 [`struct Riscv64HigherHalf`](os/kernel/src/arch/riscv64/higher_half.rs:struct Riscv64HigherHalf)）：两个生产调用点（`arch_boot_finish`、`resume_additive`）各自按所在腿选中本架构实现：直核家族经 `arch_boot` 的三份 `#[cfg]` 分支，v1 接收家族经 `arch_boot_resume_high_half` 或 `resume_after_v1_decode` 的泛型体；所以“两个调用点”与“三份实现分支”不是同一个计数；§4.4 逐条对照。

取舍表：

| 方案 | 选/否 | 代价 |
|---|---|---|
| trait 加三份内联汇编实现 | 选 | 差异封在实现里；契约（前置条件与不返回）有单一落点 |
| `#[cfg(target_arch)]` 在同一函数里分支 | 否 | 三份 asm 混在一个函数里，前置条件只能写成注释 |
| 三个独立的 `.S` 文件（照搬 C 的 `head.S` 分工） | 否 | 汇编文件无法表达"前置条件已满足"这件事；`core::arch::asm!` 已能精确控制寄存器与栈指针，独立文件多出一层边界 |
| 返回 `()` 并在内部"跳到 `kmain` 再返回" | 否 | 换栈后返回地址已不在当前栈上；任何 `ret` 都会从新栈取一个不可预期的地址，而不是回到调用者（§2.3） |

**把跳转函数写成返回 `()` 会怎样**：编译器要为它保留返回路径；但换栈之后，返回地址已经不在新栈上，`ret` 从新栈顶取到的是一条未定义的"返回地址"，控制流会跳到不可预期的地方。`-> !` 把这条约束写进类型，编译器也不再生成返回路径。

表里三条是调用者要保证的；还有一条是链接布局的前提：“调用点的执行地址 = 链接地址”。x86_64 的 `call` 要求它，代码不做运行时检查，靠链接布局保证；aarch64 的 `ldr` 取链接期绝对值，但 literal 读取是 PC 相对的，同一镜像内 literal 与代码一起被摆放，所以不需要调用点 = 链接地址；riscv64 的 `la` 是 PC 相对，direct leg 不满足它，所以实现用 `auipc/bltu` 读当前 PC，在低于 `kern_virt_base` 时补 `delta`。这不是“实现内部处理”的三合一：x86_64 侧没有检查，riscv64 侧才有显式补偿；把它写成调用者前置条件会把 riscv64 的实现细节推给调用者，所以留在实现里，但 x86_64 的链接期前提必须单独说清。

再算一次对齐这笔账（x86_64）：`mov rsp, stack_top` 之后 RSP = stack_top；`and rsp, -16` 对已经 16 字节对齐的值是空操作；`push 0` 减 8，`sub rsp, 8` 再减 8，使 `call` 前的 RSP 回到 16 字节边界；`call` 压返回地址再减 8，进入 `kmain` 时 RSP = stack_top − 24，即 8 mod 16——这正是 System V 在函数入口要求的 `(RSP + 8)` 为 16 的倍数。生产 `kmain` 是普通 Rust 函数（`naked_asm!` 只用于 `qemu_test` 载体），所以这条 psABI 对齐是必须的；`sp_aligned` 断言相应检查 `sp & 0xF == 8`。`and rsp, -16` 与 `sub rsp, 8` 一起把这条不变量钉在本地，而不是修补上游。真正会坏的有三种情形：①三者都去掉、且 `stack_top` 不是 16 字节对齐时，这个错位会一路传到 `kmain`（对齐的 `stack_top` 去掉三条后，`call` 压栈仍给出 `stack_top − 8 ≡ 8 (mod 16)`，恰好合法）；②**只去掉 `sub rsp, 8` 也会坏**——按上面的算式，`push 0` 之后 `and` 不再有作用，`call` 前 RSP 是 8 mod 16；`call` 压返回地址后入口 RSP 变成 0 mod 16，而 System V 要求入口 `(RSP+8) % 16 == 0` 即 RSP ≡ 8 mod 16，正好违反。`and` 只修正上游的 `stack_top`，`sub` 才是补 `call` 那 8 字节的那一步；③**只去掉 `push 0`** 时终止槽缺失，且 `call` 前 RSP 由 `and` 后的 16 边界直接减 8（`call` 压返回地址），入口 RSP 变成 0 mod 16，同样违反 System V 的 `(RSP+8) % 16 == 0`，`sp_aligned` 会在真机上当场红。

### 3.8 接收侧两条路径（J4）

本节只讲接收侧两条路径的分工（J4）；建表次序与窗口清单见 §3.6 与 §4.3。


发送侧把控制权交出去以后，接收侧只有两种进入方式，分别对应“根是谁建的”：

| 路径 | 根的状态 | 接收侧动作 | 代码落点 |
|---|---|---|---|
| v1 接收（两条 UEFI + U-Boot shim） | boot-shim 已建好并开启 | `arch_boot_resume_v1` 解码 arena，记录活动根，落 `.bss`，建 DM 覆盖，跳 `kmain`；不重建根 | `os/kernel/src/lib.rs:fn arch_boot_resume_v1`、`os/kernel/src/lib.rs:fn resume_additive` |
| 直核（riscv64 `-kernel`、aarch64 direct-core） | 尚无根 | `arch_boot` → `arch_boot_impl` 建根、开分页、建 DM，再跳 `kmain` | `os/kernel/src/lib.rs:fn arch_boot_impl`、`os/kernel/src/lib.rs:fn arch_boot_finish` |

两条路径在 `kmain` 处合流，但不在 `arch_boot_impl`：v1 接收不走 `arch_boot_impl`，它走 `resume_after_v1_decode` 的公共体 `resume_additive`；直核才走 `arch_boot_impl`。把它们写成一条路径会诱导接收侧重建根，而重建根会清零活动根——这是 J3/J4 交界处最贵的错误。

接收侧的次序由 J4 约束：**先完成映射与 `.bss` 落地，再换栈跳转**。`resume_additive` 在跳转前记录活动根、把 wire 载荷落进 `.bss`、建立 DM 覆盖；`arch_boot_finish` 在直核路径上完成建表后调用 `HigherHalf::jump_to_kmain`。两条路径都不返回：返回类型是 `!`，asm 的 `options(noreturn)` 让编译器不生成返回路径。

三架构的跳转差异留在实现里：x86_64 用相对 `call`（要求调用点执行地址 = 链接地址），aarch64 用字面量池取绝对值再 `br`，riscv64 用 PC 相对 `la` 并在 direct leg 补 `delta`。这些差异不改变 J4 的契约，只改变“怎样把 PC 放到高半 VMA”。§3.7 讲这个契约本身。



---

## 4. 实现详解

设计章给出的是判断，实现章要回答“代码到底怎么走”。本章沿两条真实入口——UEFI 的跨镜像接收链与 riscv64/aarch64 的直核自引导链——把装载、建根、映射、换栈、跳转串成可逐步跟读的路径，并标出每一步失败时现场在哪里。

### 4.0 本章导读

#### 学习目标

读完本章能：

- 从 `build.rs` 选链接脚本开始，跟到 `kmain` 的第一条指令。
- 说出自举根建立、`enable` 与三架构跳转的差异，以及每条差异对应的架构事实。
- 用“装载 → 建根 → 开分页 → 换栈跳转”的路径解释一次启动日志。

#### 前置概念

- 第 3 章的三种链接形态、自建解析器与成对跳转契约。
- 能读 Rust 的 `unsafe` 块与内联汇编的地址操作数；不要求会写汇编。

#### 最小示例

```bash
# build.rs 按 target 选择链接脚本（§4.1）
$ grep -n 'x86_64.unknown.none\|aarch64.unknown.none\|riscv64gc' os/kernel-image/build.rs
# 成对跳转的 trait 方法；三架构各一份实现（§4.4）
$ grep -n 'fn jump_to_kmain' os/kernel/src/boot/higher_half.rs
```

第一条确认脚本选择发生在构建期，第二条确认跳转契约只有一处声明；三架构的差异留在各自实现里，不扩散到调用方。

#### 失败对照

如果 target 与链接脚本错配（例如给 aarch64 产物套 x86_64 脚本），`p_paddr` 与 ELF 机器类型会同时出错；症状通常不是链接失败，而是装载后首次取指失败或页表建立时读到错误的镜像跨度。先核对 `readelf -h` 的机器类型与 `readelf -lW` 的首段 VMA/LMA，再怀疑建表代码。

#### 小结

- 构建期选脚本，装载期按 `p_paddr` 摆放，运行期按 VMA 取指；三者必须使用同一套基址。
- 自举根由建根者建立并开启分页（v1 是 boot-shim，直核是内核镜像）；接收侧只在活动根上追加 Direct Map，不替换根。
- 三架构的换栈与跳转差异被收进 `HigherHalf` 实现，调用方只看见一个不返回的入口。

#### 术语

- **建根**：为当前地址空间建立第一张根页表，并把恒等窗口与高半映射写进去。
- **`enable`**：把根页表地址写入架构寄存器并打开分页的动作。
- **additive 阶段**：在已经活动的根上追加映射，不重新创建根。
- **`kmain`**：内核的 C 入口对应物；进入它之前的最后一步是换栈与跳转。

### 4.1 链接脚本的落地

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
- `KEEP(*(.rodata.kernel_anchor))` 与 `static KERNEL_ENTRY_ANCHOR: fn(&BootReport<'_>, RamAuthority, PhysBytes) -> ! = minix_kernel::arch_boot;`（`main.rs` 的 [`static KERNEL_ENTRY_ANCHOR`](os/kernel-image/src/main.rs:static KERNEL_ENTRY_ANCHOR)）是一对：一个持有内核启动面的函数指针，一个把该节钉住，防止 `--gc-sections` 或归档成员粒度裁剪把整个内核启动图裁出镜像。L8 断言 `minix_kernel::arch_boot` 的符号在镜像里，是这件事的可观测证据。

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

`.eh_frame_hdr` 是链接器合成的节，不是 `*(.eh_frame .eh_frame.*)` 输入通配能匹配的对象；它随 `.rodata` 落段（§6.2 的读法），与 `/DISCARD/` 不矛盾。

三份脚本的差异集中在五处：基址常量；`.sdata`/`.srodata`/`.sdata2`/`.sbss` 这几个 RISC-V 小数据优化产生的拆分节（[`riscv64.ld`](os/kernel-image/riscv64.ld) 把它们分别归入 `.rodata`/`.data`/`.bss`，注释写明落到 `.bss` 会丢掉文件内容）；riscv64 的 `/DISCARD/` 多一条 `.riscv.attributes`；x86_64 的 `.text` 额外收 `.ap_early_entry` 与 `.ltext`，且没有 `OUTPUT_ARCH`；链接期跨距断言（三份脚本各两条：上界 `__image_end - 首段 VMA <= 0x400000` 与收口对齐 `(... % 0x200000 == 0)`；x86_64 的首段 VMA 写作 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，aarch64/riscv64 写作 `KERNEL_VIRT_BASE`）。aarch64 与 x86_64 的段序与其余收段相同，差异只在基址、`AT` 表达式和 x86_64 这两条专属条目。

riscv64 的基址常量不是自由选择。Sv39 的虚拟地址是 39 位再按 bit 38 符号扩展到 64 位，规范要求 bits[63:39] 全为 0 或全为 1；把地址右移 30 位再取低 9 位得到顶层页表下标 VPN[2]：

```text
VPN[2] = (vaddr >> 30) & 0x1FF
```

复算：`printf '0x%x\n' $(( (0xFFFFFFC000000000 >> 30) & 0x1FF ))` 输出 `0x100`（256）；把基址换成 `0xFFFFFC0000000000` 或 `0xFFFFC00000000000`，同一条命令输出 `0x0`。后两个地址看起来像高地址，但 bit 38 是 0，符号扩展后落在低半——它们的 VPN[2] 与恒等映射（覆盖低 4GiB，即 VPN[2] = 0..3）共用同一个顶层槽位。[`riscv64.ld`](os/kernel-image/riscv64.ld) 的头注释把这两个地址列为"不是合法的 Sv39 canonical high"的实例；[`direct_map.rs`](os/arch/src/arch/direct_map.rs) 把内核 direct map 放在 VPN[2] = 257 也是同一个理由（不与镜像自己占的 256 号槽共用）。

这类"把下级页表指针写到已有叶子条目上"的动作，映射接口本身会拦一次：各架构的 `map_huge` 命中叶子就返回 `PageTableError::AlreadyMapped`，而不是把叶子降级成表指针。页表多级结构与叶子判定的完整定义在 [`rewrite-notes/02-stage-vm/07-pagetable-struct.md`](../02-stage-vm/07-pagetable-struct.md)（该文已存在）；本节只用到基址选择的结论：riscv64 基址取 `0xFFFFFFC000000000`（VPN[2]=256），`0xFFFFFC…`/`0xFFFFC…` 因 VPN[2]=0 会与恒等映射共用槽位。

**把基址写成 `0xFFFFFC0000000000` 会怎样**：内核高半映射要写的是 VPN[2] = 0 那个槽，而引导期的恒等映射已经在那一槽落下 2 MiB 叶子（Step 1 固定用 2 MiB，不产生 1 GiB 叶）。两个方向的冲突都会在建表时被 `map_huge` 的 `AlreadyMapped` 拦住：往这个槽写 1 GiB 叶时，槽里已是表指针（指向装了 2 MiB 项的下一级表），`map_huge` 报 `AlreadyMapped`；反过来若上层已是 1 GiB 叶、这里写 2 MiB，x86_64 与 riscv64 的 `map_huge` 同样以 `AlreadyMapped` 拒绝降级（见 `os/arch/src/x86_64/paging.rs` 与 `os/arch/src/riscv64/paging.rs`）。假如没有这道检查，1 GiB 分支会直接改写该槽，后写条目把整槽的 2 MiB 项一并顶掉，恒等映射的覆盖范围从「若干 2 MiB 片」缩成一块；CPU 此刻正在低半执行，取指一离开这 2MiB 就失败——故障地址是低半的一条指令地址，而根因写在链接脚本的基址常量上。

**把 `.lbss` 从 `.bss` 的通配里去掉会怎样**：`--gc-sections` 之外，`.lbss` 节会被链接器当作孤儿节处理：放在它遇到的位置之后（通常是 `.bss` 之后、`ALIGN(0x200000)` 之前或之后取决于链接器版本），跨距随之变化——最坏情况是跨距不再是 2MiB 整数倍，链接期 `ASSERT` 直接失败（要看到 L5 红或规划层 `plan_kernel_mapping` 在 `arch_boot` 里 panic，得连 `ASSERT` 一起删；`boot_validate_and_prepare` 只采集能力事实）。把 `KEEP(*(.text.boot))` 从 `.text` 首位挪走：`_start` 不再落在首段开头，L6a 红（它比的是 `e_entry` 与按脚本常量算出的预期首段 VMA）；L6b 仍绿——它比的是 `nm` 出来的 `_start` 符号与 `e_entry`，两者由 `ENTRY(_start)` 绑在一起、同时移动。装载方（或固件）从 `ENTRY` 得到入口后仍然能跳，但 §4.4 里"链接地址与执行地址的关系"这一前提要重新论证。把 `. = ALIGN(0x200000)` 删掉：链接期 `ASSERT` 先失败；要看到 L5 与 L9 红，必须连 `ASSERT` 一起删（§3.5）。

### 4.2 ELF 解析与按装载地址摆放

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

逐段解码：两个初值是遍历取极值的写法（`u64::MAX` 与 0）；循环体内先取 `seg_result?`（畸形程序头由迭代器以 `Err` 项浮出，`filesz > memsz` 的守卫也已收进迭代器），再用 `checked_add`/`checked_sub` 把地址溢出与长度下溢变成 `InvalidSegment`，而不是在 release 下静默回绕；`vaddr` 与 `paddr` 各取一次最小值——它们不是同一个数算两遍，而是两份独立的数据（§3.1 的字段对应表）；`kern_size` 用物理侧的最大端点减物理侧的最小起点，虚拟侧只用最小值。跨距与基址都在这个函数里定下，之后不再重算。`found_load` 的用途是把"一个可加载段都没有"变成 `NoLoadSegments`，而不是返回一组极值初值（`u64::MAX` 做基址的后果会推迟到建表时）。返回值从裸的布局结构改为 `KernelImage<'a>`：它同时持有原始字节的切片与这一次解析出的段表，调用者无法拿 A 镜像算出的布局去装载 B 镜像的字节；布局仍可按值取出（`layout()`，或经 `Deref` 直接读字段）。

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

装载前的校验分成值对象的两步。`compute_kernel_layout` 只做纯解析：循环里逐项 `seg_result?`，遇到第一个 `Err` 就返回（畸形程序头以 `Err` 项浮出，不能先把整批 `Result` 收进 `Vec` 再当成“段比预期少”），合法段收进 `Vec` 并同时算基址与跨距，产出 `KernelImage`。`KernelImage::validate` 再跑两道目标侧检查：段间重叠（`loader.rs` 的 [`fn first_overlapping_pair`](os/boot-shim/src/loader.rs:fn first_overlapping_pair) 用半开区间判交叠：`a.paddr < b_end && b.paddr < a_end`，相邻不算重叠）与源/目标重叠（[`fn first_source_destination_overlap`](os/boot-shim/src/loader.rs:fn first_source_destination_overlap)，命中即 panic——段目标跨度覆盖任何段的 ELF 文件字节都会在装载中破坏源；这是 panic 而不是返回值，与段间重叠的 `InvalidSegment` 不对称）。校验通过才得到 `ValidatedKernelImage`，它的构造器私有，所以摆放入口拿到这个类型就等于拿到了“两道检查已经跑过”的证据；布局不再是“调用者记得先算”的口头约定。最后逐段按 `paddr` 拷贝 `filesz` 字节并清零 `[filesz, memsz)`。注意"清零"的条件是 `memsz > filesz`——`filesz == memsz` 的段（例如 `.text`）不会被额外写零，`filesz == 0` 的段（`.bss`）只清零。

可测的孪生是 `loader.rs` 的 [`fn load_segments_into_buffer`](os/boot-shim/src/loader.rs:fn load_segments_into_buffer)：同构的遍历与同样的两条动作，但目标是内存缓冲区、偏移按 `seg.paddr - buf_base_paddr` 折算。UEFI（x86-64/aarch64）与 OpenSBI U-Boot shim（riscv64）两条路径在生产代码里调用前者（直接写物理地址；"OpenSBI" 指 boot-shim 的 `opensbi_helpers.rs` 腿，不是 §4.5 路径 B 的直核 bootface——后者不重拷自己的段），宿主测试调用后者（写一块可检查的缓冲区）。两版不是同一份循环的薄包装：缓冲区版有目标长度上界（`mem_end <= buf.len()`），物理地址版有重叠检测而缓冲区版没有；这段差异是本篇承认的可测试性缺口，见 §5.1 的装载器测试族与下一段的覆盖边界。`filesz > memsz` 已由迭代器拒绝；`paddr + filesz`、`paddr + memsz` 与源区间 `offset + filesz` 的溢出由两版共用的 `segment_spans` 拒绝，不再只靠调用者先跑 `compute_kernel_layout`。缓冲区版是两循环（先验后写，超界返回 `TargetBufferTooSmall`），物理地址版是“解析成 `KernelImage`、`validate` 后只接受 `ValidatedKernelImage`、再写”；两者共享段形校验、区间推导与 BSS 清零，不共享目标边界策略。宿主测试 `test_buffer_and_phys_loaders_share_segment_spans` 用同一份 ELF 分别走两版，逐字节比对拷贝区与清零区。直接调用这个 `pub unsafe fn` 的读者仍必须先校验布局（覆盖边界见 §5.1 末段）。

**把 BSS 清零这一步从两个环节里都去掉会怎样**（装载方只拷 `filesz` 不写 NOBITS 区，入口汇编也不清）：内核镜像的 `.bss` 里住着镜像堆 `static mut IMAGE_HEAP`（[`main.rs`](os/kernel-image/src/main.rs)）以及内核侧的启动全局；这些格子里会是上一位使用者的字节。两个自引导面共用的 `bootface_common::run_self_boot` 把 `BootStorage` 放在调用方栈上，不产生 `.data` 静态；`bootface_a64.rs` 的 bump 游标 `static BUMP_NEXT: AtomicU64` 初值为 `BUMP_BASE`（非零），落在 `.data`，所以它不属于这个例子。这正是把两个环节都拆掉之后才会出现的形态：[`riscv64.ld`](os/kernel-image/riscv64.ld) 的注释记录过 OpenSBI 只按 `filesz` 写段体、不写 NOBITS 区这一实证，所以三份脚本都让入口汇编自己再清一遍（[`main.rs`](os/kernel-image/src/main.rs) 的注释把它写作"入口自持"）。两个环节各自都能单独成立，叠起来才是三架构一致的保证。

**去掉重叠检测会怎样（第二处）**：一个 `p_paddr` 区间交叠的 ELF（链接脚本写错、或工具链改布局）会让后一个段覆盖前一个段的字节，没有任何报错；后一个段覆盖前一个段的字节是数据损坏，不是 `copy_from_nonoverlapping` 的 UB——该函数的契约是单次调用的 `src`/`dest` 不相交；源/目标重叠另由 `first_source_destination_overlap` panic 拦住。段间重叠检测把前者变成 `ElfError::InvalidSegment`，与 §3.2 的 12 个变体里 loader 侧那三道之一对上。

### 4.3 自举根的建立与开启

沿用 01 篇的 4.1 节 立的 Step 编号（本节展开 Step 1–3；Step 4 的 DM 覆盖本篇不展开，边界见 01 篇的 1.4 节；01 篇的 6.1 节 用它排路标）。这套编号与 §2.4 描述的 C 两级页表流程（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步）**不是同一个体系**：C 的五步是 32 位两级页表的建表顺序，Rust 的 Step 0–4 是 4 级页表的启动阶段编号（多出校验与 DM 两段、没有独立的 `pg_load`）。五步在 Rust 侧的落点：`pg_clear` 并入根页清零（`new_from_page`）、`pg_identity` 并入 Step 1、`pg_mapkernel` 并入 Step 2、`pg_load` 并入 `enable` 的 `mov cr3`、`vm_enable_paging` 即 Step 3。

Step 1 与 Step 2 是两段映射循环；aarch64 另有 Step 2b（设备 MMIO 窗口的高半别名）；Step 2c 是三架构共用的一段附属步骤（编号 2c 不占主步序号；x86/riscv 的 `device_window_regions()` 非空，aarch64 的这张表为空故为 no-op）。Step 1 的范围判据是**调用方给出的 boot identity 窗口表**，Step 2 是内核镜像的 LMA 跨度。下面的代码是直核面的 `build_bootstrap_root_and_enable`；v1 接收面不走这个函数，只做 `resume_additive` 的追加。

```rust
// 节选（保留 Step 1/2 的循环体；注释、#[allow] 与运行期断言有删节），
// os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable。
// （DEV_PIECE 是文件级常量 `const DEV_PIECE: u64 = 1 << 21;`——2 MiB 分片尺寸，
//   此处随调用序一并展示，非函数体局部定义。）
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let device_windows = identity_device_windows();
    for range in windows.0 {
        let raw_start = range.start().0;
        let raw_end = range.end().0;
        let start = raw_start & !(DEV_PIECE - 1);
        let end = raw_end
            .checked_add(DEV_PIECE - 1)
            .expect("boot identity window end overflows u64")
            & !(DEV_PIECE - 1);
        let mut addr = start;
        while addr < end {
            let piece_flags = if identity_piece_is_device(addr, DEV_PIECE, device_windows) {
                PageFlags::kernel_read_write() | PageFlags::DEVICE
            } else {
                id_flags
            };
            match paging.map_huge(VirBytes(addr), PhysBytes(addr),
                                  PageSize::MIB_2, piece_flags) {
                Ok(()) | Err(PageTableError::AlreadyMapped) => {}
                Err(error) => panic!("boot identity map: map_huge failed: {error:?}"),
            }
            addr += DEV_PIECE;
        }
    }

    // Step 2: Kernel high-address mapping. C: pg_mapkernel() — pg_utils.c:186
    let kern_virt = layout.virtual_base;
    let kern_phys = layout.physical_base;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < layout.size {
            paging.map_huge(VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
                            kern_huge, kern_flags)
                .expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge.bytes();
        }
    }
```

Step 1 的注释与循环行为按六条交代：前两条是判据，其后四条是配套说明（第六条的分片规则 aarch64 与 riscv64 都适用）：

- 范围是**调用方给出的 boot identity 窗口表**，不是固定的低 4 GiB。窗口来源按腿不同，不能统一措辞：
  - 两条 UEFI 跨镜像腿：shim 自己的 PE image（`LoadedImage.info()`，取不到即 fail-closed 停止）、shim 栈（从当前 SP 向上取整到页、再向下留 4 MiB，最后按 2 MiB 对齐的保守窗口，不是真实栈范围）、可选的 `dumpdtb` 临时缓冲、内核 LMA、root/bump/arena、12 个模块、固件描述符表与设备窗口。
  - U-Boot shim 腿：shim 自身只加一个窗口 `[SHIM_LINK_BASE, UBOOT_SHIM_STACK_TOP)`；它是 ELF 物理链接产物，没有 UEFI `LoadedImage`，栈在 `.stack` 段（`__bss_end` 之后），窗口终点 `UBOOT_SHIM_STACK_TOP` 覆盖到栈顶。内核 LMA、root/bump/arena、模块、平台源与设备窗口仍由 `push_common_boot_windows` 另加，不在这个窗口里。
  - 两个直核腿：内核 LMA、root/bump、模块、平台源、设备窗口，以及**调用者栈窗口**（从当前 SP 向上取整、向下留 4 MiB、按 2 MiB 对齐）。最后一项不能省：直核腿的调用者栈是固件栈或固件给出的栈，不在内核 LMA 窗口内；缺它时 `enable()` 之后第一条栈访问就会在无 IDT/VBAR/stvec 的情况下静默停机。
  - x86_64 的跨镜像路径与测试内核的直核入口调用（`arch_boot_impl`）在各自窗口表末尾额外压入 `(0, 2 MiB)`，供 AP 实模式跳板使用（`AP_STARTUP_PA = 0x5000`）；aarch64/riscv64 走 PSCI/SBI，没有这个窗口。
  每个窗口按 2 MiB 取整，窗口内的设备 piece 置 `DEVICE`。平台描述符窗口按 `PlatformDescSource.len` 扩到 blob 全长：DTB 生产者读 `totalsize`、ACPI 生产者读 RSDP 的 `length`，`push_platform_windows` 用这个跨度建窗口；RSDP source 还会在分页前由 `table_chain_windows` 把根表与到 MADT 为止的表追加为独立窗口，不再依赖 RSDP 所在 2 MiB 片的布局偶然。`len == 0`（旧生产者、测试，或新生产者读 blob 头失败）才回到“地址所在的那一个 2 MiB 片”。wire 的字段布局与信封版本见 01 篇的 3.4.3 节；本节只取窗口范围判据。是否在 Direct Map 之后才消费不影响窗口语义：窗口现在覆盖整段 blob，不再依赖“只读起始片”的假设。`BootWindows` 的容量是 180 个窗口，编译期 `BOOT_WINDOW_FLOOR` 保证至少容纳 shim/内核/root/bump/arena/模块/平台源（含 ACPI 链）/设备窗口/x86 低 2 MiB；`push` 对 `start >= end` 直接 panic，`push_2m` 用 `checked_add`，窗口表写错在组装点就停机，不会拖到 `enable()` 后取指。公共窗口类别（内核 LMA、root、可选 bump、模块、平台表、设备窗口）由 `push_common_boot_windows` 一处定义，v1 发送侧只额外加 shim/arena，直核侧不加；新增类别不会只落到一条腿。`BOOT_IDENTITY_MAP_END`（唯一权威在 [`minix-arch`](os/arch/src/lib.rs) 的 `BOOT_IDENTITY_MAP_END`）是**VA 上限**（窗口不得越过），不是“铺满 `[0, END)`”的覆盖承诺。`[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]`：4 GiB 全铺是 C 32-bit `pg_identity` 的继承；Rust 只覆盖 boot 期间真正访问的窗口，Direct Map 仍由内核在 Step 4 建立。
- 标志是 `kernel_read_write() | EXECUTABLE`（supervisor-only + 可执行），不是"用户可访问"。三个架构都选 supervisor-only；在 RISC-V Sv39 上这是强制条件：S 模式取指一条 `U=1` 的页被禁止，`sstatus.SUM` 只放宽 supervisor 对用户页的 load/store，取指不受它影响（ISA 语义按 RISC-V 特权规范陈述，不引用源码注释）。x86_64 与 aarch64 不置 USER 是同一设计选择（内核页不给用户态使用），不是取指硬约束：x86_64 的取指限制来自 SMEP，而 C 侧连 SMEP/SMAP 都依赖固件未置位（§2.4）；aarch64 的取指权限由 UXN/PXN 位控制，U/S 位本身不是取指禁令。C 侧口径见 §2.4（`pg_identity` 置 USER 位，`pg_mapkernel` 不置）；Rust 侧的差异是直接不置它。

- 缓存属性是另一处与 C 的差异，两侧的划法不同：C 的 4 MB 上界规则与 `pg_identity` 逐项置 PRESENT 的细节见 §2.4；64 位侧按“登记窗口”划分——登记过的 MMIO 窗口在 Step 1 内按 2MiB 分片置 `DEVICE`（机制见下文“设备窗口分片”一条），未登记的空洞统一可缓存 `id_flags`。这个差异连同它的代价要交代清楚：其一，**前提：未登记空洞在运行期没有访问者**——物理访问只从账本分配的 DRAM 来，会真访问的 MMIO 全部进了登记窗口；这条前提当前没有运行期断言，靠代码审查与平台源登记维持（现行注释按 `map_huge` 的错误集陈述，见下条 `map_huge` 的两类结果）；其二，缓存兜底的削弱被接受——C 的 `PWT|PCD` 也不提供页错误，被不采用的只是“空洞访问不污染缓存”这一层；窗口**外**的地址现在直接未映射，访问会触发页错误，比 C 更早暴露，而窗口**内**未登记的空洞仍是可缓存映射。防线从映射属性换成账本纪律：分配只来自账本、MMIO 只来自登记窗口。

- Step 1 的**设备窗口**清单是 `identity_device_windows()`（aarch64 = 平台 MMIO 基窗；riscv64 直接返回 `minix_arch::kernel_identity_mmio_regions()` 整表，不做筛选，表内只有 PLIC（平台级中断控制器）+ UART（串口）两项；x86 上 `identity_device_windows()` 返回空）。x86 的 LAPIC（本地 APIC）/IO-APIC（I/O APIC）不在 Step 1 置属性；它们由 Step 2c 的高半设备窗口安装，`device_window_flags()` 当前返回 `kernel_read_write() | DEVICE`，`x86_64::paging` 的 `flags_to_pte` 把 `PageFlags::DEVICE` 编码成 `PWT|PCD`（uncached write-through），因此不再依赖固件 MTRR 的正确性（证据见 01 篇的 5.3 节 未闭环项）。这条属性不是只改了编码函数：`device_window_flags()` 返回 `kernel_read_write() | DEVICE` 是 Step 2c 正确性的必要条件；少了 `DEVICE`，Step 2c 安装的就是普通可缓存映射，MMIO 访问在真机上不可靠。当前实现把 `DEVICE` 接进窗口标志并补了断言测试。反向不可恢复：x86 的 `pte_to_flags` 从 `PWT|PCD` 只能还原成 `WRITE_THROUGH|NO_CACHE`，不能还原 `DEVICE`（硬件没有独立 device 位）；要判断一个页是不是设备页，必须查 `device_window_regions()`，不能查 `query()` 的标志。

- Step 1 的 `map_huge(...)` 用 `match` 区分两类结果：`AlreadyMapped` 的完整语义（两类分支各在什么条件下返回）见 §3.4，这里只讲 Step 1 下的可达性。Step 1 的重叠窗口会命中同一 2 MiB 叶：同一片被两个窗口各写一次时，第二次安装看到 `e2` 已 present，三份实现都返回 `AlreadyMapped`（不比较物理地址与标志）；调用方的 `Ok(()) | Err(PageTableError::AlreadyMapped) => {}` 把它当成功吸收，所以这条臂是重叠窗口的常态路径，不是防御性例外。`AlreadyMapped` 也覆盖 1 GiB 叶挡住 2 MiB 安装的情况。**宿主 mock 的判断粒度更细**：`MockPaging::map_huge` 把 2 MiB 展开成 4 KiB 的 `map_range`，目标块内任一页已映射就返回 `AlreadyMapped`；生产实现只判断块叶本身。两者在“目标叶已存在”上同向，差异只在部分覆盖的粒度。其余错误（如 `AllocationFailed`——某块需要新配一级中层页表、而引导 bump 分配器发不出页）立即 `panic!("boot identity map: map_huge failed: …")`。把失败留在建表点而不是拖到 `enable()` 后第一次取指，是为了让现场带 `map_huge failed` 的文字与地址；aarch64 若恰好是 UART 窗口本身没建上，panic 输出可能仍不可见，这是唯一保留的例外。预算在 UEFI/U-Boot 腿是 1024 页（两个直核面是 256 页），实际只需几张中层表，正常不会触发。
  **重叠窗口的真实语义**：同一 2 MiB 片被两个窗口各写一次时，第二次 `map_huge` 返回 `AlreadyMapped`；调用方按“已经是同一片、忽略”吸收。`AlreadyMapped` 的语义是“目标槽已有叶（任意尺寸）”，不是“重复映射”。源码 `os/kernel/src/lib.rs` 的 Step 1 注释仍按旧行为写“同尺寸重写返回 `Ok`；`AlreadyMapped` 只在 1 GiB 叶时出现”，与三份 `map_huge` 实现不符；按真值优先序以实现为准；源码注释与实现不一致时，以实现为准。
- 设备窗口分片对 aarch64 与 riscv64 都适用。Step 1 的**每个 piece 都是 2 MiB**，`identity_piece_is_device(addr, DEV_PIECE, ...)` 决定该 piece 是否置 `DEVICE`；同一窗口表里不存在 1 GiB 叶与 2 MiB 分片混排，因此"先写 1 GiB 后写 2 MiB 覆盖"的互斥问题不成立。Step 2c 是另一条腿（把会从高半访问的固定 MMIO 页装进自举根的窗口集合 `device_window_regions()`），与 Step 1 的窗口表不共享槽位判定。`identity_device_windows()` 返回两类窗口——aarch64 的 MMIO 物理窗口，以及 riscv64 的 PLIC/UART 固定窗口（`minix_arch::kernel_identity_mmio_regions()`）。MMIO 寄存器要求强序访问（不投机、不合并、不重排），普通可缓存映射在 QEMU 上能过、真机上不行；注释同时要求 Step 1 的恒等别名与 Step 2b 的高半别名对同一物理页使用同一套属性（ARM 手册：同一物理页用两种属性映射，结果不可预测）。

Step 2 的守卫 `if kern_virt != kern_phys` 记录在注释里：两基址相等时，Step 1 的窗口已经覆盖内核 LMA 段（Step 2 是它的子集），跳过 Step 2 可以避免同一个 VPN[2] 槽位被二次写入。为什么 `kern_huge` 选中 1 GiB 时不能**做 Step 2**，见本节末“若 `kern_virt == kern_phys` 时照做 Step 2”的推演；这里只留守卫本身，不重复那段因果。

aarch64 上还有一段 Step 2b：把设备 MMIO 窗口按 2 MiB 映射到内核高半（`MMIO_VA_BASE`）。低半那份恒等映射会随第一次 `switch_address_space` 换掉 TTBR0 而消失，而 TTBR1 钉在自举根上、不参与切换——串口与 GIC（ARM 的通用中断控制器）需要一个高半别名才能在切换之后继续可用（Step 3 末尾的 `use_high_mmio_window()` 配套它；属性与 Step 1 的恒等别名一致，理由同上一条）。

之后还有一段 Step 2c：把 `device_window_regions()` 里 `bootstrap = true` 的设备窗口装进自举根（x86 的 LAPIC/IO-APIC、riscv 的 PLIC/UART；另有 ACLINT SSWI（定时器/软件中断设备）运行期窗口，`bootstrap = false`，DTB 门控，不在 Step 2c 安装）。这些窗口的槽位按 2 MiB 对齐，逐段用 `map_huge` 装成 2 MiB 叶；aarch64 的设备表为空，Step 2c 直接跳过（它不是错误，控制器走系统寄存器、设备走高半别名）。注意 Step 1 的清单是 `identity_device_windows()`；在 riscv64 上它就是 `kernel_identity_mmio_regions()` 整表（PLIC + UART），而 Step 2c 用的 `device_window_regions()` 是另一张表，riscv64 上另含 ACLINT SSWI（`bootstrap = false`），`kernel_identity_mmio_regions()` 不含它，所以不能把两张表当成同一张的两个切片。x86_64 的 `kernel_identity_mmio_regions()` 返回空表（函数文档写明原来的低 VA 表没有生产读者）；LAPIC/IO-APIC 只在 `device_window_regions()` 里，由 Step 2c 安装。

Step 3 的 `enable()` 在 x86-64 上做四件事：

```rust
// 节选（保留与硬件状态顺序相关的行；注释有删节，`asm!` 块按等价的单行写法重排；source-symbol: X86_64Paging::enable），os/arch/src/x86_64/paging.rs:fn enable。
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

顺序是：置 `EFER.NXE` → 写 `CR3` → 置 `CR0.PG` 与 `CR0.WP` → 按能力置 `CR4.PGE`。（注意 x86_64 的 UEFI 链进来时固件分页已在运行：这一步实际生效的动作是"换 `CR3` 根"，`CR0.PG` 的置位是与"固件未开分页"形态（如 C 的 BIOS 引导链）统一的写法——本仓生产链不存在那条 BIOS 链，此处指 C 侧语义的统一，不是本仓有一条 BIOS 产线；`NXE` 必须最先的理由见下一条。）

- `EFER.NXE` 必须最先：`NX` 是页表条目里的最高位（bit 63），它的使能开关在 `EFER` 这个 MSR 的 `NXE` 位（bit 11）上；`EFER.NXE = 0` 时任何带 NX 的页表条目在走表时被视为保留位，触发页错误且错误码 bit3（保留位）为 1。内核的 Direct Map 叶与用户数据/栈叶都带 NX，所以准确的要求是"第一条带 NX 的条目被使用之前必须先置 NXE"（放进 `enable` 即可满足）。注释指出 UEFI 交接时恰好置了它，但这条不变量被当作"我们自己的"，不依赖固件。
- `CR3` 写的是内核自己的根：`build_bootstrap_root_and_enable` 返回前会把根页表物理地址记进内核全局（`set_current_root_phys(root_page)`），后续阶段（如装载 VM 的 ELF 时给同一张表加映射）通过 `Paging::from_active_root` 取回一个句柄。`from_active_root` 与 `new_from_page` 的区别是后者会清零根（`write_bytes(ptr, 0, 512)`），前者不会；两者都带 `PteChannel` 参数，声明这次 PTE 访问走哪个写视图。
- `PGE` 与 C 的 `vm_enable_paging` 对应（§2.4 的 `pgeok`）：只在 CPU 支持时置，且必须在 `PG` 之后。

五步与 C 的一一对应及差异，见 01 篇的 4.9 节 的函数对照表（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五行），这里不重复。表外还有 Step 2b（aarch64 设备 MMIO 窗口的高半别名）与 Step 2c（x86/riscv 设备窗口装入自举根）——C 没有对应物：x86 的串口是端口 I/O，ARM 侧是纯 MMIO、必须走内存映射；LAPIC/IO-APIC 与 PLIC/UART 在启动期就要经固定高半 VA 访问。

**若恒等映射只覆盖可用内存、或虚拟基址等于物理基址时照做第二步**（两处）：

其一，恒等映射只覆盖 `memmap` 里报告的可用内存。UEFI 的加载器代码与数据可以落在 `LOADER_CODE`/`LOADER_DATA` 这类非 conventional 内存里；`enable()` 写完 `CR3` 的下一条取指就在这类内存中，而页表里没有它——页错误发生在 `enable()` 内部，此时页错误处理程序（IDT 与异常向量要到 `kmain` 才建立）还不存在；`EarlyConsole` 虽然已经能打印建表路标（01 篇的 6.1 节 的 `step1+2`/`step4`），但没有能接住缺页的异常路径，现场仍是"开启分页后立刻静默停止"。

其二，`kern_virt == kern_phys` 时照做 Step 2。Step 1 已经用 **2 MiB piece** 把这个地址范围建好；Step 2 的 `map_huge` 在目标槽已有叶时直接返回 `AlreadyMapped`（1 GiB 档与 2 MiB 档都先检查 present），而 `build_bootstrap_root_and_enable` 对 Step 2 用 `.expect("kernel map: map_huge failed — kernel image mismatch")`。所以删掉守卫的后果不是“静默覆盖、只改缓存属性”，而是**建表点当场 panic**，失败现场带 `map_huge failed` 文字；这是 fail-closed，不是属性漂移。



**入口栈对齐与 SSE（承接 01 篇的 2.2 节）**：Rust 侧不继承这个问题：三个内核镜像的引导栈顶由链接脚本的 `. = ALIGN(16)` 保证 16 字节对齐；x86_64 的入口 asm 另显式 `and rsp, -16`；`jump_to_kmain` 三架构均再对齐一次，x86_64 另按 psABI 做 `sub rsp, 8` 调整。x86_64 内核镜像以 `x86_64-unknown-none` 构建，该目标默认不启用 SSE/SSE2（`rustc --print cfg` 只给出 `fxsr`）；boot-shim 的 `x86_64-unknown-uefi` 目标另在 `os/.cargo/config.toml` 显式加了 `-C target-feature=-sse,-sse2,-mmx`，两条腿的取舍分开记。

### 4.4 三架构的成对跳转

trait 的三份实现做同一件事，指令不同。x86_64 全文（`higher_half.rs` 的 [`struct X86_64HigherHalf`](os/kernel/src/arch/x86_64/higher_half.rs:struct X86_64HigherHalf)）：

```rust
/* 节选（保留签名与 asm 块；两条 16 字节对齐断言省略，论证见 §3.7；注释有删节），os/kernel/src/arch/x86_64/higher_half.rs。 */
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
- `isb` 的位置在 `br` 之前，作为防御性屏障；`enable()` 内部已经对 TTBR/SCTLR 的写入执行过 `dsb sy; isb`，所以它不是这些写入可见性的唯一来源。删除它不会产生“按旧映射取指”的失败（§4.4 其二）。
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

还有第三种执行形态：x86_64/aarch64 的内联测试载体——内核代码与垫片一起在低地址执行，符号值与执行地址之间没有平移差，相对取址与绝对取值都落在同一个视图里；"PC 在高半"不是这条形态的判据。riscv64 测试载体不是这种：它直接调 `arch_boot`，低装高进，仍按 direct leg 加 `delta`（§5.3）。

**三条推演（riscv64 direct leg 去掉条件 `add t1, t1, t2`；aarch64 去掉 `isb`；x86_64 去掉 `xor rbp, rbp`）**：

其一，riscv64 去掉 `add t1, t1, t2`。`jalr` 的目标落在符号的物理地址上，PC 留在低半（`0x8020…`）；启动能走完（低半此时仍有恒等映射），坏点在首次地址空间切换之后——内核按高半 VMA 建了进程根表，切换后取指落在没有映射的低半地址，`scause` 记下取指错误。源码注释（[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs:fn jump_to_kmain)）对这一形态的表述是：direct leg 的代码仍在 `kern_virt_base` 之下执行，只有跳转目标被抬到高半——即"PC 仍留在低地址"，而不是"内核从未迁到高半 VMA"。

其二，aarch64 去掉 `isb`。`enable()` 在写完 `ttbr0_el1`/`ttbr1_el1` 之后已经执行 `dsb sy; isb`，写完 `sctlr_el1`（置 M=1）之后又执行一次 `isb`；三条 aarch64 路径都经过 `enable()`，`establish_boot_dm` 之后到跳转之间不再写 TTBR/SCTLR。所以删掉 `jump_to_kmain` 里的 `isb` 不会产生“按旧映射取指”的失败；这条屏障是防御性保留，不是正确性前提。

其三，x86_64 去掉 `xor rbp, rbp`（或 aarch64 去掉 `mov x29, #0`、riscv64 去掉 `li s0, 0`——三个架构都靠这一步把回溯起点置 0）。清帧指针缺席时，`kmain` 内 `get_bp()` 读到的是换栈前留下的值，回溯链沿着与调用关系无关的地址走，走到一片合法映射里才停下——症状是 panic 打印的调用栈里出现无关地址。生产 `kmain` 不保存 `%rbp`（未启用 `-C force-frame-pointers`），所以这里没有“第一个带序言的帧把旧帧指针压进保存槽”这一步；链的终止点就是清零后的寄存器本身。（x86_64 序列里的 `push 0` 终止槽不在这条链上；去掉它改变的是入口栈的对齐，`sp_aligned` 断言会当场失败。）

### 4.5 从装载到 `kmain` 的完整路径

把 §4.2 的装载与 §4.4 的跳转接起来，四条路径（v1 send/receive、riscv64 自引导、aarch64 direct-core、inline 测试载体）的分岔点在"谁执行装载、谁建根"：

```text
生产路径 A（v1 send/receive：UEFI x86_64/aarch64、riscv64 U-Boot）
  固件 → boot-shim main → prepare_and_enter_kernel_image / prepare_boot
        → minix_elf 解析 → 按 p_paddr 写段体、清零 BSS
        → 读 12 个模块 → 组 BootReport<'_>（v1 交接载荷）→ 写 v1 arena（信封与 memmap/reserved/modules/platform/param/sources 六个分区）
        → ExitBootServices 配对循环 → enter_kernel_image_v1（建 bootstrap 根、补运行期字段）
        → 独立镜像 _start（清 .bss、立栈）→ rust_image_main
        → arch_boot_resume_v1（解码 arena、落 .bss、记录活动根、建 DM）
        → HigherHalf::jump_to_kmain

生产路径 B（riscv64 自引导）
  OpenSBI → 镜像 _start（清 .bss、立栈）→ rust_image_main
        → bootface：读 a1 的 DTB → 解 memmap → 读模块表 → 模块字节搬进 bump 页
        → 组 BootReport<'_> → minix_kernel::arch_boot
        → arch_boot_finish（校验 → arch_boot_impl（策略/Step1 恒等 → Step2 高半 → Step2b/2c 设备窗口 → Step3 开分页 → Step4 DM）→ HigherHalf::jump_to_kmain）

测试路径（inline）
  x86_64/aarch64 测试载体 → UEFI helpers → arch_boot（进程内）→ arch_boot_finish → arch_boot_impl → HigherHalf::jump_to_kmain
  riscv64 测试载体 → 直接 arch_boot（同一 arch_boot_impl）

生产路径 C（aarch64 direct-core）
  bootface_a64 → arch_boot（内部走 arch_boot_finish → arch_boot_impl）→ HigherHalf::jump_to_kmain
```

交接形态的分岔（v1 send/receive、aarch64 跨镜像跳转、riscv64 自引导、测试/直核）在 `01-boot-shim-bootstrap.md` §3.7 已逐条给出，本节只把装载、建根、跳转三步接起来。注意“aarch64 跨镜像”是 v1 内部的一次绝对跳转，不是独立的装载路径；aarch64 UEFI 的装载路径仍是 v1 send/receive。

§3.1 的登记表是权威，本节只把装载、建根、跳转三步接起来，不重复按形态列表。

> aarch64 direct-core 的“低地址进入”来自固件按装载地址平移入口地址的装载行为，不在本仓链接脚本里；可用 QEMU `-d in_asm` 的第一条记录核对 `_start` 的入口 PC。

自引导路径的正文在 `bootface_common.rs` 的 [`fn run_self_boot`](os/kernel-image/src/bootface_common.rs:fn run_self_boot)：从 DTB 解出 memmap、按模块表装载 12 个模块、构造发送侧 `BootReport` 这三步都在它里面，之后才调用内核的启动入口（`bootface.rs`/`bootface_a64.rs` 只传本架构常量与入口参数）：

```rust
/* 节选（保留 BootReport 构造、发送侧 strict 与 arch_boot 调用；省略号处为 DTB 解析、模块表、模块装载），os/kernel-image/src/bootface_common.rs:fn run_self_boot。 */
    let report = storage.report(
        VirBytes(params.kern_virt_base),
        PhysBytes(params.kern_phys_base),
        params.kern_size,
    );
    report.validate_requiring_sources(minix_boot::RamAuthority::DTB);
    ...
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&report, minix_boot::RamAuthority::DTB, root_page)
```

两个基址与跨距是常量（`KERN_VIRT_BASE`、`KERN_PHYS_BASE`、`KERN_SIZE`，与 `riscv64.ld` 同源，L9 对账），发送侧只构造 `BootReport<'_>` 借用视图（[`fn report`](os/libs/minix-boot/src/boot_storage.rs:fn report)），接收侧在 `arch_boot_finish` 里落 `.bss` 后构造唯一 `KernelInfo`；`reserved` 是 `DtbMemoryMap::new(&fdt, fw_end)` 裁剪出的非传统内存（riscv64 传 `KERN_PHYS_BASE`，aarch64 传 0），`param_buf` 是 `/chosen/bootargs` 经 `normalize_param_buf` 规范化后的字节。两个自引导面收成薄壳：`bootface.rs` 与 `bootface_a64.rs` 只保留本架构常量、入口横幅与各自的 console 实现，DTB 校验、memmap 收集、表址闸、bump 取页、模块摆放、平台源、参数缓冲、`BootReport` 组装与 `arch_boot` 调用全部走 `bootface_common` 的 [`fn run_self_boot`](os/kernel-image/src/bootface_common.rs:fn run_self_boot)。模块字节仍由 [`fn place_modules`](os/kernel-image/src/bootface_common.rs:fn place_modules) 搬运：源字节直接拷进 bump 页；不做整页清零，只把文件字节之后的尾部余量清零，名字槽从定长字节池拷进一页物理名字池——模块名以胖指针（`&str` 是"指针 + 长度"两个机器字，与单个地址不同）形式交给内核，指针必须指向物理视图可达的字节（这段的视图约束写在 `bootface.rs` 的模块文档里）。自引导镜像的早期代码在物理视图下靠 PC 相对寻址自洽（`la` 算出的值与当前 PC 同侧），只有最后跳 `kmain` 需要按 direct leg 加视图差值 `delta`（§4.4）。

自引导路径与 v1 路径的分界写在 `bootface.rs` 的模块文档里：自引导镜像**不重拷自己的段**。v1 的 boot-shim 会把 `kernel.elf` 的段拷进物理内存并清 NOBITS；自引导镜像已经在运行，同样的动作会覆盖正在执行的代码与栈。它只搬**模块**，自己的段不动，`.bss` 清理由入口汇编完成（§4.2 的第一处反向推演——BSS 清零那一条——讲过为什么这一步不能省）。

两条生产路径在 `kmain` 处合流，但不在 `arch_boot_impl`：v1 接收走 `arch_boot_resume_v1` → `resume_after_v1_decode`（记录活动根、落 `.bss`、建 DM），direct 走 `arch_boot_impl`（Step1 恒等 → Step2 高半 → Step2b/2c 设备窗口 → Step3 开分页 → Step4 DM）。两条路径在跳转前都把 `KernelInfo` 落进 `.bss` 副本，跳转时消费的是副本地址。副本落地的原因有两条：**载荷存活**——`reserved_regions`/`memmap`/`boot_modules` 原本指向 boot-shim 堆上的字节，高半切换后那块内存在活动根里没有映射，再读是页错误；历史真机上还出现过读回全零（01 篇的 3.2.4 节），两种结局都不能消费（落地区与一次性守卫见 [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)）；**交接路径**——发送侧曾把 shim 栈上的 `&KernelInfo` 经 `noreturn` asm 直传，现行形状是接收侧先落 `.bss`、再由 `arch_boot_finish` 把同一副本地址交给 `jump_to_kmain`；生产 `kmain` 用 `ptr::eq` 断言形参与 `KERNEL_INFO` 同址，字段读取走全局副本（01 篇 §4.5）。

**让自引导镜像照搬 v1 的做法、先把自己的段重新拷一遍会怎样**：目标区间就是它正在执行的代码与栈所在的物理内存：拷贝完成之前，正在执行的这段代码的字节已被改掉，下一条取指拿到的是新内容（或者被清零的 BSS）。这条分界在 [`bootface.rs`](os/kernel-image/src/bootface.rs) 的模块文档里点明。

跳转完成时系统处在什么状态、下一件必须做的事是什么，可以用一张状态表收束（下表描述的是当前 Rust 实现的状态，C 时代的对照见 §4.6）：

| 项 | 跳转完成时的值 | 依据 |
|---|---|---|
| 程序计数器 | v1 send/receive 与 aarch64 跨镜像：`kmain` 的高半 VMA；aarch64 direct-core：高半 VMA（`ldr` 取绝对值跳）；riscv64 自引导 direct leg：加 `delta` 后的高半 VMA；测试载体：x86_64/aarch64 低地址、riscv64 高半 VMA（§4.4/§4.5） | [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf) 的三份实现 |
| 栈指针 | x86_64：入口 RSP = stack_top − 24（8 mod 16，System V）；aarch64/riscv64：入口 SP = stack_top（16 字节对齐，AAPCS64/RISC-V ABI）；帧指针为 0 | 载体侧行为判据的唯一权威是 [`fn kmain_verify`](os/kernel/src/lib.rs:fn kmain_verify) 的三条断言（见 §5.3）；生产路径的静态断言见 §3.7 |
| 分页 | 已启用；恒等映射与内核高半映射并存；Direct Map 覆盖已建立；设备窗口（Step 2b aarch64 高半别名、Step 2c 设备窗口）已建立 | v1：shim 的 `build_bootstrap_root_and_enable` + 接收侧 `establish_boot_dm`；direct：`arch_boot_impl` 的 Step 1–4 与 2b/2c |
| 保护结构 | **未建立**：`kmain` 的早期阶段（早期校验段，完整划分见 `03-kmain-cstart.md`）只做校验与信息收存，不安装保护结构；保护结构 `init_protection`（GDT/TSS 或 VBAR_EL1/stvec）是保护结构安装段的第一条动作——在那之前任何异常都无法处理 | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的文档注释（`without valid GDT/TSS … any exception will cause an unrecoverable triple fault`；同一注释里的 `very first` 指安装动作的次序，不是 `kmain` 的第一条语句） |
| 中断 | 未接管；时钟与中断控制器在保护结构之后初始化（`kmain` 的早期校验段里 `init_protection` 在前、`init_clock_and_interrupts` 在后） | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection)、`:fn init_clock_and_interrupts`；细节属 `03-kmain-cstart.md` 的范围 |

也就是说，这一篇结束时 CPU 已经站在内核自己的地址空间与自己的栈上，但还没有任何"出事能被接住"的设施。**本篇主线的终点在这里——验证与诊断见 §5、§6**（下一篇从 `kmain` 的第一条指令开始，先把保护结构装上；范围归属见 §1.3）。

#### 4.5.1 跨镜像交接与直核自引导

三架构按“架构 × 链”五行并列。分道代码在 `os/kernel-image/src/main.rs` 的 `rust_image_main`——注意不是 boot-shim 的 `main.rs`（那个文件的入口骨架在 01 篇的 4.1 节 引过，分道臂不在其中）——本节按架构逐臂给出；先从 aarch64 的接收面看起：

```rust
// 代码引用（保留两个 cfg 臂的完整分道；注释节选），os/kernel-image/src/main.rs:fn rust_image_main（aarch64，按交付形态分特性）。
// 盘面产物（fw-aarch64-uefi-recv）：boot-shim 建表、开翻译，然后把 v1 arena 的
// 物理地址留在 x0 上绝对跳进本镜像的高半 `_start`。
#[cfg(all(target_arch = "aarch64", feature = "fw-aarch64-uefi-recv"))]
fn rust_image_main(arena_pa: usize) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    if arena_pa == 0 {
        early_console::write_str(
            "### minix-rs kernel-image: no handoff arena (x0 == 0), halting\n",
        );
        return halt();
    }
    early_console::use_high_mmio_window();
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}

// 直核产物（fw-aarch64-none）：QEMU `-kernel` 不给交接 arena，本镜像自己自举。
#[cfg(all(target_arch = "aarch64", not(feature = "fw-aarch64-uefi-recv")))]
fn rust_image_main(_arena_pa: usize) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    bootface_a64::bootface_a64()
}
```

（`bootface_a64()` 的返回类型是 `!`——自引导面正常不会返回。接收臂里的 `arena_pa == 0` 只剩「门面没交接」这一种含义：它诚实停机，而**不再**被解释成「该走直核自举」——直核形态是另一个构建产物，判据在编译期就定了。）

```rust
// 代码引用（保留两个 cfg 臂的完整分道；注释节选），os/kernel-image/src/main.rs:fn rust_image_main（riscv64，按交付形态分特性）。
#[cfg(all(target_arch = "riscv64", not(feature = "fw-riscv64-uboot")))]
fn rust_image_main(boot_hart: u64, dtb_phys: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys)
}

// 〔编注〕U-Boot 接收镜像：a0 = boot-shim 的 enter_kernel_image_v1 跳转递来的 v1 arena
// 物理地址；同一 v1 解码与加性恢复，不重建 shim 已激活的根表。
#[cfg(all(target_arch = "riscv64", feature = "fw-riscv64-uboot"))]
fn rust_image_main(arena_pa: usize) -> ! {
    if arena_pa == 0 {
        early_console::write_str(
            "### minix-rs kernel-image: no handoff arena (a0 == 0), halting\n",
        );
        halt();
    }
    let _kernel_entry: usize = core::hint::black_box(KERNEL_ENTRY_ANCHOR as usize);
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, riscv64)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}
```

| 架构 | 谁建根表 | 谁开分页 | 交接值 | 内核入口 |
|---|---|---|---|---|
| x86_64 | boot-shim | boot-shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（入口寄存器见 01 篇的 4.1 节） | 独立镜像高半 `_start` |
| aarch64（UEFI 链） | boot-shim | boot-shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（入口寄存器见 01 篇的 4.1 节） | 独立镜像高半 `_start` |
| aarch64（直核） | 内核镜像自身（[`bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)） | 内核镜像自身（`arch_boot` → `enable`） | 无入参（直核产物 `fw-aarch64-none` 的入口只编 `bootface_a64` 臂、忽略传入值；UEFI 接收产物 `fw-aarch64-uefi-recv` 的 `x0 == 0` 只作诚实停机；DTB 从 RAM 基址固定位置读）；模块来源是 `BootFileTable`，表址只有 DTB `/chosen` 的 `opensbi,boot-file-table` 属性一条通道（由 `run_self_boot` 的 DTB 解析给出，与 riscv64 直核面同名属性；取件库复用 opensbi_helpers——01 篇的 3.2 节 断言的证据落点） | 独立镜像 `_start` |
| riscv64（`-kernel` 直核） | 内核镜像自身（`bootface`） | 内核镜像自身（`arch_boot` → `enable`） | `a0`=hartid、`a1`=DTB；表址只从 DTB `/chosen` 的 `opensbi,boot-file-table` 属性取；原有备用 `a2` 参数通道已在 D01 裁决项 3 删除（仓内从未有生产者） | 独立镜像 `_start` |
| riscv64（U-Boot 一族 / shim 生产腿） | `riscv64-uboot-shim` | shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（入口寄存器见 01 篇的 4.1 节） | 独立镜像高半 `_start` |

（表的行按架构 × 链列；每条腿的计数与协议段换算以 01 篇的 3.7 节为唯一权威，本节不另立计数。）

"四种协议段/交接面"是 01 篇的 3.7 节 对账按**协议两段 + 直核两类**的切分（v1 send / v1 receive / riscv64 自引导 / aarch64 direct-core），不是腿数——落到腿上有五条（三条 v1 发送腿同时属于 send 与 receive 两段）。这些腿有两个收敛点，按腿各归其一：直核家族（`arch_boot` 三个 cfg 分支）收敛在 [`arch_boot_finish`](<os/kernel/src/lib.rs:fn arch_boot_finish>)——`arch_boot_impl` 建表后经它跳 `kmain`；跨镜像三腿收敛在 `arch_boot_resume_v1`——三架构都先经 `resume_after_v1_decode`（aarch64 的壳再转 `arch_boot_resume_high_half`，带两行路标），三者在 调用序合并后都委托到泛型体 `resume_additive`（validate/落 KernelInfo/记录根/establish DM 的同一组加性阶段在其体内），最后落到同一个 `jump_to_kmain`。发送侧没有"收敛到 arch_boot"——它的终点是跳独立镜像的入口指令：aarch64 用 `br {entry}`、riscv64 用 `jr {entry}`、x86_64 用一段 global-asm stub：它的两个 SysV 实参是 `(entry, arena_pa)`——`rdi` 收 entry、`rsi` 收 arena——stub 内 `mov rbx, rsi; mov rax, rdi; cli; cld; jmp rax`，所以接收侧看到的仍是 `rbx` = arena。01 篇的 3.7 节 只引了 aarch64 的 asm；x86 的完整 stub 就是上面这一段。`arch_boot_resume_v1` 里的 `resume_boot_pt_alloc` 只是 `BootAlloc::resume` 的转发（01 篇的 4.6 节 的两种进入方式之一；第三种“从 memmap 切兜底区间”的 `init` 已不存在），注册不发生在它里面。

`enter_kernel_image_v1` 里跳转前的准备（建根、读游标、补运行期字段）与跳转写法（aarch64 `br {entry}` / riscv64 `jr {entry}` / x86_64 的 stub：实参与跳转序列见本节上段，不再重复）——01 篇的 3.7 节 只给了 aarch64 的跳转（riscv64 的 `jr` 在源文件，未在该引文中展开）。这里补一条与"谁建根"有关的约束：跨镜像路径下，`build_bootstrap_root_and_enable` 被 boot-shim 调用一次、生成的表**已经激活**；接收侧必须避免二次建表（3.7 的反向推演已给后果），而 `bootface` 路径下建表发生在内核镜像自身，`arch_boot` 走的是常规 Step 1–4。

两处实现现状：

- x86_64 的独立镜像入口已实现（接线状态见 01 篇的 3.1.1 节）：`rust_image_main(arena_pa)` 的 arena 基址来自 `_start` trampoline 转进 `rdi` 的 `rbx`（入口约定见 01 篇的 4.1 节），随后调用 `arch_boot_resume_v1`；`_start` trampoline 按源码次序执行 `cli`、`cld`、清 `.bss`（只碰 rdi/rcx）、设栈、把入口寄存器转进 SysV 第一参数 `rdi`、再 `call rust_image_main`（入口寄存器约定见 01 篇的 4.1 节）。内核镜像的产出与布局校验仍由 `check-layout.sh` 的 L0–L10 守住。
- riscv64 在 QEMU `-kernel` 直核场景下镜像按物理基址视图执行（源码注释把这一实测事实记作 `medany`）。"两基址相等"发生在哪类产物要说准：**部分 QEMU 直核测试内核**按 VMA=LMA 平铺链接（如 `test-paging-enable-riscv64` 的 `link.ld`，其位置计数器一行即 `. = 0x80200000`——VMA=LMA 平铺），`kern_virt_base == kern_phys_base`，§4.3 的 Step 2 守卫（相等即跳过 Step 2）就是为这类产物写的；riscv64 的 `test-higher-half` 载体则低装高进（§5.3），`delta` 非零，不走这条守卫；**生产直核镜像**与 U-Boot 接收镜像共用高半 VMA 的 `riscv64.ld`，两基址不等、守卫走正常分支。生产链（`bootface` + 文件系统 + 高半链接）走的是另一套布局。U-Boot 链的 `kernel-image`（启用 `fw-riscv64-uboot`）接收镜像与直核镜像共用同一份 `riscv64.ld`（高半 VMA `0xFFFFFFC000000000`、物理装载基址 `0x80200000`），差别只在入口分道特性与交给它的第一个参数；shim 侧则是一片独立的物理链接产物（VMA=LMA，见 `os/boot-shim/riscv64-uboot.ld`）。两条布局的基址常量以链接脚本为准，复算命令：`grep -n 'KERNEL_VIRT_BASE\|KERNEL_PHYS_BASE\|ENTRY' os/kernel-image/*.ld`（该命令对三份脚本各命中若干行，这里只摘 riscv64 的三个值： `KERNEL_VIRT_BASE = 0xFFFFFFC000000000`、`KERNEL_PHYS_BASE = 0x80200000`、`ENTRY(_start)`）。

**让 riscv64 照搬 x86_64 的「一个镜像 + 空指针诚实失败」形态会怎样**（问题不在“建表/跳转次序”——两者都先建根再跳；差别在入口参数通道）。三条架构现在都按**交付形态分构建产物**：aarch64 的盘面产物（`fw-aarch64-uefi-recv`）只编 v1 接收臂、直核产物（`fw-aarch64-none`）只编自引导臂，盘面产物里的 `arena_pa == 0` 只作诚实失败；riscv64 的 `fw-riscv64-uboot` 接收镜像同理（`a0` 为零只作诚实失败），直核产物只编 `bootface`。x86_64 没有直核面，`0` 只作诚实失败。riscv64 的直核入口（`rust_image_main(boot_hart, dtb_phys)`）没有"交接指针"这个入参——直核场景只给 hartid 与 DTB，表址从 DTB `/chosen` 取；若照搬 x86_64 的「一个镜像承载两种形态」而不补一条分道，空指针解引用会发生在 `early_console` 初始化之前（主语限定为**非 U-Boot 特性**的 riscv64 直核入口——`fw-riscv64-uboot` 构建的接收臂有 `arena_pa` 入参，这个反事实只适用于直核形态，串口上没有任何输出）。反过来，若 aarch64 退回内联执行，撞的是 01 篇的 3.7 节 第一处另一做法已经推演过的同一个问题（TTBR0 被换掉、取指映射作废），现场证据（`elr`/`vbar` 钉在同一低地址）也在那里。

### 4.6 与 Minix3 C 的对照

| C 侧的机制 | Rust 侧落点 | 处置 | 差异原因 |
|---|---|---|---|
| `AT()` 表达式给出 LMA，装载器按 `p_paddr` 写段体（§2.1） | `os/kernel-image/*.ld` 的 `AT()` + [`fn load_segments_into_phys_memory`](os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory) | 保留（形态） | 装载者从 GRUB 换成 boot-shim 或引导器（U-Boot；OpenSBI 只提供 SBI，不摆放镜像），表达式与读法不变 |
| `objcopy --prefix-symbols` + `unpaged_*.o` 两段式布局（§2.2） | 无对应物：内核镜像只有高半一段，引导侧代码在 boot-shim 或入口汇编里 | 淘汰 | 分页开启前的主体在 boot-shim（v1 建根）或入口汇编（direct 面）；内核镜像没有 C 那种由同一装载器放到低物理地址再执行的 unpaged 段（`01-boot-shim-bootstrap.md` §3.7 的交接形态） |
| `k_initial_stktop` 链接符号（§2.5） | `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size` 的派生访问器） | 改型 | 值由接收方现算，内核不声明外部符号，也不在交接载荷里多传一份 |
| `head.S` 的成对动作：设栈 + 跳转（C 侧四行，§2.3） | [`trait HigherHalf`](os/kernel/src/boot/higher_half.rs:trait HigherHalf) 与三份内联汇编实现 | 改型 | 收进 trait 契约；栈对齐、屏障、执行视图差值三件事各自显式 |
| —— | `check-layout.sh` 的 L0–L10 宿主断言 | 新增 | C 侧由 Makefile 与链接脚本的常量约束，没有独立断言脚本 |

三行"改型/新增"的共同点：把 C 里靠"位置相邻、文档说明、读者经验"维持的约束，换成一种可以被工具或类型系统检查的形式——脚本不变量进断言脚本，栈顶值进派生访问器，跳转前提进 `unsafe` 契约。

**把 C 的两段式布局照搬进 Rust 会怎样**（给内核镜像也加一个低地址的 unpaged 段，把"分页开启前要执行的代码"放进去）：Rust 侧已经没有"由同一个装载器把整个镜像放到物理地址上、再从低地址开始执行"的形态。v1 路径的 boot-shim 已经建好根并把镜像按 `p_paddr` 摆放，进入镜像是高半 VMA；direct 面的入口汇编在低物理地址执行、自己建根。把一段"分页前代码"放进按高 VMA 链接的低地址段，在 v1 会把取指引到高 VMA（映射已建，但段体在低地址，语义错位）；在 direct 面则与入口汇编的 PC 相对寻址冲突。C 的两段式能成立，是因为 GRUB 按 LMA 把整个镜像放到物理地址、低地址段与建表代码在同一个镜像里；Rust 侧这两件事被拆给了 boot-shim 与入口汇编（`01-boot-shim-bootstrap.md` §3.7 的交接形态）。

---
## 5. 测试

四条不变量的证据分布如下（小节号是本章的入口，不代表一一对应）：

| 不变量 | 主要证据 |
|---|---|
| J1 平移一致 | §5.1 的解析/布局测试；§5.2 的 L4（Rust 生产镜像）；C 侧分组静态阅读见 §2.1 |
| J2 跨距与粒度 | §5.2 的 L5/L9、L7；§5.4 的 M10/M12 |
| J3 映射先于进入 | §4.3 的 Step 1→2→3 次序（代码审查）；§5.3 的启动链到达 `kmain`（隐含映射已完成）；**没有“映射先于进入”的独立断言** |
| J4 换栈与跳转 | §5.3 的 SP/PC/FP；§5.4 的 M8/M13 |

### 5.0 本章导读

#### 学习目标

读完本章能：

- 把 J1–J4 分别映射到宿主测试、布局断言与真机门，指出哪一条没有独立断言。
- 运行解析器、装载器与 `check-layout.sh` 的最小测试集。
- 读懂 `test-higher-half` 的判据，不把“到达 `kmain`”误当成 J3 的独立证明。

#### 前置概念

- 第 4 章的完整启动路径。
- 会读 `cargo test` 的通过计数与 shell 脚本的退出码；不要求会写 QEMU 测试。

#### 最小示例

```bash
# 解析器与装载器的宿主测试（§5.1）
$ cd os && cargo test -p minix-elf
# 生产镜像的静态布局断言 L0–L10（§5.2）
$ bash os/kernel-image/check-layout.sh all
```

第一条验证 ELF 语义与缓冲区契约，第二条验证链接布局；两者都不依赖 QEMU，适合在改代码后先跑。

#### 失败对照

如果只看 `test-higher-half` 打印了“到达 `kmain`”，就宣称 J3“映射先于进入”已被证明，会把链尾证据当成整条链的证据：该测试证明的是“这条载体能走到 `kmain`”，不是“任何映射次序都正确”。J3 目前只有代码审查与链尾证据，缺口要在结论里写明，不能用一条绿门掩盖。

#### 小结

- J1/J2/J4 有宿主测试或布局断言，J3 只有代码审查与链尾证据；这是已知的证据边界。
- `check-layout.sh` 是生产镜像的静态契约，`test-higher-half` 是载体行为门，两者回答不同的问题，不能互相顶替。
- 测试计数要按当前源码复算，不沿用历史读数。

#### 术语

- **L0–L10**：`check-layout.sh` 的布局断言编号；L10 只对 x86_64/all 输出。
- **宿主测试**：在开发机上运行的 `cargo test`，不启动 QEMU。
- **`test-higher-half`**：x86_64 载体，验证换栈、PC 与帧指针在进入 `kmain` 后的状态。
- **链尾证据**：只证明启动链最终到达某一点，不证明中间每一步的次序。

### 5.1 宿主：解析器、装载器与建表

§4.2 那条链里，解析、布局与缓冲区语义的每个环节都有宿主测试，分属两个 crate；按 `p_paddr` 写物理地址那一步不在这批测试的射程内，只由真机链证明（见本节末段）。

`os/libs/minix-elf` 的 27 个测试（复算：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 27；运行：`cd os && cargo test -p minix-elf`）按对象分四族：

| 族 | 测试名 | 证明什么 |
|---|---|---|
| 文件头五道校验 | `test_parse_ehdr_valid`、`test_parse_ehdr_bad_magic`、`test_parse_ehdr_too_short`、`test_parse_ehdr_not_64bit`、`test_parse_ehdr_not_le`、`test_parse_ehdr_not_executable` | 每种畸形输入各返回对应的 `ElfError` 变体 |
| 迭代器行为 | `test_segment_iter_valid`、`test_segment_iter_no_pt_load`、`test_segment_iter_two_segments`、`test_segment_iter_phdr_out_of_bounds`、`test_segment_iter_phdr_entry_too_small`、`test_segment_iter_empty_image`、`test_segment_iter_header_only`、`test_segment_iter_reports_segment_data_out_of_bounds`、`test_segment_iter_filesz_exceeds_memsz_is_err`、`test_segment_iter_rejects_filesz_greater_than_memsz`、`test_segment_iter_truncated_mid_segment_is_err`、`test_segment_iter_non_load_filter_stays_silent` | 表范围检查、非 `PT_LOAD` 过滤、四类畸形以 `Err` 返回（`PhdrOutOfBounds`、`PhdrEntryTooSmall`、`SegmentOutOfBounds`、`FileszExceedsMemsz`） |
| 段字段 | `test_segment_with_zero_filesz`、`test_mixed_pt_load_and_other`、`test_segment_flags`、`test_entry_point`、`test_machine_type` | 字段取值的忠实性（含 `filesz == 0` 与混合段序） |
| 装载仿真 | `test_kernel_load_simulation_single_segment`、`test_kernel_load_simulation_two_segments`、`test_kernel_load_simulation_with_bss_segment`、`test_segment_copy_and_bss_fill` | 用解析结果做一次"拷贝 + 清零"，逐字节核对 |

`os/boot-shim` 的 `loader.rs` 部分有 32 个测试（复算：`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 32；运行：`cd os && cargo test -p boot-shim --lib` 在默认特性下共 136 个测试，其中 32 个属于 `loader::tests`），与本节直接相关的有 28 条，代表项如下：`test_compute_kernel_layout_from_mock_elf`（布局三个量的取值）、`test_read_required_panics_on_missing_kernel`（缺内核文件是 panic 而不是继续）、`test_bss_region_is_zeroed_after_load`（BSS 清零的结果逐字节核对）、`test_buffer_and_phys_loaders_share_segment_spans`（两版装载器逐字节一致）、`test_load_segments_into_phys_memory_rejects_overlapping_elf`（重叠段在 `validate` 被拒）、`test_compute_kernel_layout_rejects_filesz_greater_than_memsz`（`filesz > memsz` 被拒）、`test_compute_kernel_layout_rejects_paddr_memsz_overflow`（地址溢出被拒）、`test_compute_kernel_layout_zero_size_segment_yields_zero_kern_size`（零长段边界），六个 `first_overlapping_pair_*`（区间判交叠：不相交、真重叠、相邻不算、零长度被忽略、首个命中、空表——见下段）、`test_elf_file_end_{returns_last_load_file_end,non_elf_is_none,malformed_segment_is_none}`、`test_load_boot_modules_into_trims_elf_to_file_end`、`test_compute_kernel_layout_rejects_truncated_mid_segment` 与三条 `load_segments_into_buffer_*`。

这些测试覆盖三类对象，各自走的路径不同：`test_bss_region_is_zeroed_after_load` 直接调用孪生函数 `load_segments_into_buffer`，逐字节核对三件事——`[0, filesz)` 保留文件字节、`[filesz, memsz)` 清零、`memsz` 之后一个哨兵字节不被触碰（判据是两种预填充值 0xAB 与 0xCC）；`test_compute_kernel_layout_from_mock_elf` 直接调用 `compute_kernel_layout`，测试文件里写明了原因（大意：不能在单元测试里做那次物理内存拷贝）；六个 `first_overlapping_pair_*` 直接调纯函数（不相交、真重叠、相邻不算、零长度被忽略、首个命中、空表）。模块那一侧的三条测试走 `load_boot_modules_into` 加一个模拟分配器，它的地址指向测试期间不释放的静态 `Mutex<Vec<Vec<u8>>>` 缓冲区——同一文件里的注释写明（大意）"绝不在单元测试里写真实物理内存"。

因此宿主测试证明的是"解析出的字段、拷贝与清零的语义、区间判交叠"，不证明"按 `p_paddr` 写物理地址"这一步：那一步的唯一执行者是 `load_segments_into_phys_memory`，它的目标地址来自 ELF，在宿主上无法安全执行，留给真机链。这条分界也解释了为什么孪生函数存在——把可测的语义从不可测的那一步里剥出来。

**砍掉装载仿真族（四个测试）只留解析族会怎样**：解析结果对、但"拷贝时把 `filesz` 写到哪、BSS 从哪开始、清零多长"这些语义没有断言；这三处任一处的偏移错误（例如用 `memsz` 当拷贝长度、或 BSS 起点用 `paddr + memsz`）都只在真机上以随机崩溃的形式出现——而真机测试的现场只有串口日志，定位要重新走一遍宿主就能做完的排查。

### 5.2 布局契约 L0–L10（唯一权威清单；L10 仅 x86_64/all 输出）

`check-layout.sh` 把"boot-shim 装载内核时要读的字段"在宿主上判定完。可复制命令与退出码：

```bash
$ bash os/kernel-image/check-layout.sh all      # 或 x86_64 / aarch64 / riscv64
$ echo $?
0     # 全部断言通过；1 = 任一断言失败；2 = 用法/环境错误
```

断言清单（脚本头部注释逐条列出 L0–L8；L9 在内联实现处，L10 是 C 侧段归属检查，x86_64/all 时输出）：

| 断言 | 检查什么 | 防哪种漂移 |
|---|---|---|
| L0 | 脚本预期表里的两个基址 = 链接脚本里声明的两个基址 | 两处手写常量漂移（断言对象是"工具与真源的关系"） |
| L1 | ELF 类型 = `EXEC` | 内核装载面没有重定位处理器，`static-pie` 不可用 |
| L2 | `e_machine` 与预期三元组相符 | 拿错架构工件（用 x86 工件当 aarch64 用） |
| L3 | 首段 `PT_LOAD` 的 VMA/LMA = 该架构预期基址 | 布局常量与链接脚本不符 |
| L4 | 所有 `PT_LOAD` 的 `(vaddr - paddr)` 是同一个平移量 | "一个 `kern_virt_base ↔ kern_phys_base` 平移"的前提 |
| L5 | 镜像跨距 2MiB 对齐 | `plan_kernel_mapping` 的页尺寸选择至少能落进回退档 |
| L6 | 入口等于首段 VMA，且 `_start` 符号等于入口、落在首段内 | 入口符号与段布局脱节 |
| L7 | 引导栈符号存在、跨度 64KiB、落在末段内（不检查自顶向下方向）——这是宿主侧的弱检查；sp 初值与数组顶的真实关系（含收口填充余量）由 ld 脚本的排布保证，见本篇 §3.3 的栈顶派生 | 栈符号与镜像范围脱节 |
| L8 | `minix_kernel::arch_boot` 符号在镜像里 | `--gc-sections`/归档粒度把内核启动图裁出镜像 |
| L9（全架构） | ELF 实测跨距 = 共享的 `KERN_SIZE` 常量副本（x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`；x86_64 无直核面） | 镜像布局常量与自引导面的期望值漂移 |
| L10（C 侧，x86_64/all 时输出） | `Makefile.inc` 的 `.for unpaged_obj` 名单含 `head.o`/`pre_init.o`/`pg_utils.o`/`klib.o` 且不含 `mpx.o`；`kernel.lds` 有 `.unpaged_text`/`.unpaged_data`/`.unpaged_bss` 三个收段；`mpx.S` 定义 `k_initial_stktop`、`head.S` 引用它 | 改名名单与收段规则脱节，`k_initial_stktop` 被误收进低地址 unpaged 段 |

L3 的子断言：L3a = `PT_LOAD` 段数 ≥ 1；L3b = 首段 VMA 等于按脚本基址常量算出的预期值；L3c = 首段 LMA 等于预期装载基址（x86_64 为 `0x200000`）。L6 的子断言：L6a = 入口等于预期入口；L6b = `_start` 符号等于入口；L6c = 入口落在首段 `[VMA, VMA+memsz)` 内。

L4 与 L5 分别对应本篇 §2.1 的段平移结论与 §3.5 的大页阶梯：L4 保证"两套地址之间只有一个偏移量"这件事对每个段都成立，L5 保证"选出的页尺寸"有一个可用的档位。L8 是"链接保留"的可观测证据——`KERNEL_ENTRY_ANCHOR` 持有 `arch_boot` 的函数指针，链接脚本 `KEEP` 该节，两者一起挡住裁剪。

**去掉 L5（跨距对齐断言）会怎样**：一个跨距为 `2MiB + 4KiB` 的镜像在规划层 `plan_boot_pages`/`plan_kernel_mapping` 里既进不了 1 GiB 档也进不了 2MiB 档（`kern_size` 同时对两者都不整除——L5 检查的是**VMA 跨距**（末段 `VMA + memsz` 减首段 VMA），阶梯检查的是 `kern_size`（**LMA 跨度**：同一组段 `min(paddr)` 与 `max(paddr+memsz)` 之差）；L4 保证前后两段共用同一平移量时二者相等，当前三个链接脚本满足这个前提，L5 因此保证阶梯至少落进 2MiB 档。若将来在段间插入只改 VMA 的填充，这个等式会断，需要把填充也落进 LMA，或把阶梯改为按 VMA 跨距判断），落到 `KernelMapError::NoKernelPageSize`（启动入口把它转成可读 panic）。现场是"启动第一步就 panic"，根因在链接脚本的段间距；没有宿主断言就只能靠真机二分定位。

### 5.3 真机：`test-higher-half` 与入口面

真机侧验证 §4.4 的产物是 `test-higher-half` 测试内核：它走完整的 `arch_boot` 路径，最后停在 `kmain`；`kmain` 在 `qemu_test` 形态下是裸函数，在函数序言改动栈指针之前把四个值抓下来交给验证函数（`lib.rs` 的 [`fn kmain_verify`](os/kernel/src/lib.rs:fn kmain_verify)）——启动信息指针，以及 SP、PC、FP 三个寄存器：

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

三条断言各对应 §4.4 的一条指令：`sp_ok` 对应"切栈"（栈指针落在高半地址空间）、`sp_aligned` 对应"对齐"（x86_64 检查 `RSP ≡ 8 (mod 16)`，即 `(RSP+8)` 为 16 的倍数；aarch64/riscv64 检查 `SP ≡ 0 (mod 16)`，AAPCS64/RISC-V ABI 要求 SP 16 字节对齐）、`fp_zero` 对应"清帧指针"。捕获方式按架构写在裸函数 `kmain` 的 `naked_asm!` 里（x86_64 用 `mov r9, rbp`、`lea r8, [rip]`、`mov rdx, rsp`；aarch64 用 `mov x3, x29`、`adr x2, .`、`mov x1, sp`；riscv64 用 `mv a3, s0`、`auipc a2, 0`、`mv a1, sp`）。

**PC 不进入 x86_64/aarch64 的成败判定（riscv64 相反，见下）**，这条取舍写在源码注释里（"PC check is architecture-dependent… PC may not be in the high range"）：三个载体都被固件装在低地址，但**进入方式不同**——x86_64 与 aarch64 用镜像自己的（低）链接地址进入，PC 停在低半；riscv64 的载体走"低装高进"（`la` 加上 `delta` 位移，[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs) 的说明段），PC 落在 `kmain` 的高半 VMA，源码注释记录了实机读数（`0xffffffc0…`，修正前是 `0x8020…`）。PC 一项因此按架构分开判定：x86_64 与 aarch64 只打印（`PC 不在高半` 时输出 `INFO`，只在失败分支里出现）；riscv64 把 `pc_at_high` 并入 PASS 条件，PC 落在低半即 `FAIL: PC not at high address (riscv64 delta path broken)`——这正是 `delta` 路径的机械判据（`kmain_verify` 的 `pc_ok` 分支，`target_arch = "riscv64"`）。测试载体的 `BootReport<'_>` 是合成的（各载体同形、取值按架构；以 x86_64 载体为例：[`main.rs`](os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs) 里 `kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（镜像窗口按 2 MiB 取整后的跨度）），栈顶由 `kern_stack_top()` 现算，也就是 §3.3 的算式在测试形态下的取值。

运行方式与判定标记：`os/qemu-tests/run_qemu.sh` 用 `-serial file:` 收串口、用 `strings | grep '### TEST_RESULT: PASS'` 判定（`os/qemu-tests/run_qemu.sh` 的判定段），退出码里 `SKIP` 与 `FAIL` 分开（0 表示 PASS 或 SKIP，1 表示失败）。测试本身的输出形如 `### TEST_RESULT: PASS test-higher-half ###`。

**把 PC 无条件加进成败判定会怎样**：x86_64 与 aarch64 的载体按低地址进入、PC 常驻低半，这条断言在这两个架构上恒红；红色成为常态之后，`sp_ok`/`sp_aligned`/`fp_zero` 三条真正的失败会淹没在同一条常红里。因此判据按架构分开：这两个架构仍只打印 `INFO`，riscv64 才把 PC 高半并入 PASS 条件——在 riscv64 上它不会恒红，因为 `delta` 路径正常时 PC 就该落在高半；它一旦恒红，说明 `delta` 补偿坏了，这正是要抓的故障。

### 5.4 机制 → 需要证明什么（反转核对表）

本节从**机制**出发列出"要证明它成立需要什么断言"，再标注覆盖状态——不是从现有测试出发（那份视角在 §5.1–§5.2；本节是它的反转面）。反转核对查出三项覆盖缺口（C 侧段归属、riscv64 PC 判据、生产入口 ABI 栈形），三项均已闭合：分别由 `check-layout.sh` 的 L10、riscv64 的 PC 高半判据、生产入口的 ABI 栈形断言覆盖；arch `link.ld` 形态没有生产读者，常量锁步对象不存在。

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 覆盖状态 | 现有证据 |
|---|---|---|---|---|
| M1 | `kernel.lds` 的双地址与 `AT()`（§2.1） | 每个带 `AT()` 段的 `vaddr − paddr` = 同一常量 | 部分覆盖 | Rust 生产脚本的 L4 机器断言（三架构工件）；**C `kernel.lds` 的 `AT()` 是静态阅读，无机器断言**；§6.2 的复算是一次性手工 |
| M2 | 两段式布局的段归属（§2.2） | `.unpaged_*` 段 VMA=LMA；改名名单决定归属 | 已覆盖 | L10 机械对账 `Makefile.inc` 的 `.for unpaged_obj` 名单与 `kernel.lds` 的三个 `unpaged_*.o` 输出段 |
| M3 | `head.S` 的成对换栈跳转（§2.3） | 设栈+跳转的次序、位移与绝对值各自的前提 | 部分覆盖 | **C 侧次序本身无自动断言**；§5.3 的三断言是 Rust 侧 `HigherHalf` 的覆盖，不能反证 C 的 `head.S` 次序 |
| M4 | `k_initial_stktop` 落段（§2.5） | 栈符号落在高半段、栈区不与其他内容重叠 | 部分覆盖 | L10 断言 `mpx.o` 不在 unpaged 名单（因而 `k_initial_stktop` 落在高半 `.bss`）且 `head.S` 引用该标签；栈区与其他内容不重叠仍靠静态阅读 |
| M5 | Rust 生产脚本的判据（§3.1） | 生产脚本有 `AT()` 且常量与预期表一致 | 已覆盖 | L0/L3c/L4；无读者的 arch `link.ld` 形态不在树内 |
| M6 | ELF 解析器的五道校验与 12 个变体（§3.2） | 每种畸形输入返回对应变体 | 已覆盖 | minix-elf 27 测试（五道校验/迭代器/字段/仿真）；`NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall` 由 boot-shim loader 的测试覆盖，不在 minix-elf 的 27 个里 |
| M7 | 按 `p_paddr` 摆放与 BSS 清零（§4.2） | 段体写到 `paddr`；NOBITS 区清零；重叠拒绝 | 部分覆盖 | loader 的 32 个测试中 28 个直接相关（`bss_zeroed`、`rejects_overlapping`、三个新守卫等，名单见 §5.1）；**`p_paddr` 物理写入只有真机链**，宿主只跑 buffer 孪生 |
| M8 | 三架构成对跳转的指令序列（§4.4） | 切栈+跳转按序、对齐、清帧指针、`delta` 处理 | 已覆盖 | 真机载体三架构（`sp_ok`/`sp_aligned`/`fp_zero`）；riscv64 另把 `pc_at_high` 并入 PASS 条件，低 PC 即报 `delta path broken` |
| M9 | 从装载到 `kmain` 的合流（§4.5） | 两条生产路径各自到达 `kmain` 并完成跳转 | 部分覆盖 | 路径与代码落点已给出；**“到达 `kmain`”本身没有独立断言**，真机门只证明启动链走到调度器，不区分入口路径 |
| M10 | 镜像跨距的 2MiB 收口（§3.5） | 跨距 2MiB 对齐、与 `bootface` 常量一致 | 已覆盖 | L5/L9（L9 三架构，按架构选 `bootface.rs`/`bootface_a64.rs`） |
| M11 | 交接的寄存器约定（§4.4、§4.5） | `kinfo` 经 `.bss` 副本交接（形参按 ABI 传入 RDI/X0/A0，生产 `kmain` 第一条语句即遮蔽为全局副本） | 部分覆盖 | NK4-A 的寄存器覆写修复与注释；**行为性，无自动断言** |
| M12 | 布局断言的持续成立（§5.2） | L0–L10 全过 | 已覆盖（一次性手工，非常驻） | `check-layout.sh` 三架构实跑（本轮复跑全 PASS）；这是一次性复跑，不是常驻 CI 断言 |
| M13 | 生产 `kmain` 入口的 ABI 栈形（§3.7、§4.4） | x86_64 `call` 前 RSP 16 对齐、入口 `RSP ≡ 8 (mod 16)`；aarch64/riscv64 入口 SP 16 字节对齐 | 已覆盖 | 三份 `jump_to_kmain` 在生产路径上断言 `kern_stack_top` 16 字节对齐，x86_64 另断言 `(stack_top − 24) ≡ 8 (mod 16)`；`kmain_verify` 的 `sp_aligned` 仍在载体侧复核入口值 |

本节列出的机制都有对应证据或明确的证据边界；M1/M3/M4/M7/M9/M11 的“部分覆盖”是已登记边界，不是未登记缺口；下一次新增机制时继续按同一格式补行。

---
## 6. 诊断

启动失败时，串口上通常只剩几行输出；如果不知道每条输出属于哪个阶段、下一个路标应该是什么，就很难把“没走到”与“走到了但报错”分开。本章按 EBS 分界、panic 路径、失败模式与可复制命令四层组织诊断，让读者能从一行输出反推它所在的位置。

### 6.0 本章导读

#### 学习目标

读完本章能：

- 按“症状 → 怀疑对象 → 确认命令”定位三个故障时刻：分页开启后立刻、恒等映射移除时、首次切地址空间后。
- 用 `readelf -lW` 与 `check-layout.sh` 区分 LMA 错、VMA 错与映射缺失。
- 识别并过滤临时打印，不把诊断噪声当成正式路标。

#### 前置概念

- 第 1–5 章的四个时刻、J1–J4 与测试证据边界。
- 会读 QEMU 异常日志里的故障地址与出错类型。

#### 最小示例

```bash
# 静态布局：先排除 LMA/VMA 与跨距问题（§6.2）
$ bash os/kernel-image/check-layout.sh all
# 单个工件的段头：对照 p_vaddr / p_paddr / memsz（§6.2）
$ readelf -lW os/target/x86_64-unknown-none/release/kernel
```

第一条给出布局级结论，第二条给出单段读数；先静态、后动态，能避免把映射问题误判成链接问题。

#### 失败对照

如果把“分页开启后立刻取指失败、故障地址是高地址”直接归因于 `AT()`，会改错层：`AT()` 只改 `p_paddr`，不影响高半映射是否建立。正确顺序是先看 `readelf -lW` 的 VMA/LMA 是否一致，再看 `arch_boot_impl` 的 Step 2 是否覆盖了故障地址；两层都排除后，才怀疑换栈或跳转。

#### 小结

- 先静态布局、再动态异常、最后 GDB 读值；三层各自回答不同的问题。
- 三个故障时刻要靠故障地址与日志位置区分，不能只看“取指失败”四个字。
- 临时打印不是正式路标；复现串口日志时先过滤，再做判据。

#### 术语

- **故障地址**：QEMU 异常日志里的 `CR2`/`stval`/`FAR`；它回答“哪个地址不可访问”。
- **恒等映射移除**：切地址空间后低半映射消失，仍在低半取指的代码会立刻失败。
- **LMA 重叠**：多个 `PT_LOAD` 的 `p_paddr` 相同；现行 GNU ld 会在链接期拒绝。
- **临时打印**：源码自标 `temporary` 的诊断输出，不属于正式路标。

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状（可观察到的现象） | 怀疑对象 | 确认动作 |
|---|---|---|
| 取指失败、故障地址是低地址（出现时刻三选一：分页开启后立刻／恒等映射被移除时／首次切地址空间后——三个时刻的区分：分页开启后立刻见 §4.3；恒等映射被移除见 §4.4 其二；C 侧对照见 §2.3/§2.4） | 恒等映射被移除而 PC 仍在低半（§4.4 的 riscv64 `delta` 缺失）；或恒等映射从未建立 | 读 QEMU 异常日志里的故障地址与出错类型；核对该架构 `higher_half.rs` 的跳转目标算式 |
| 分页开启后立刻取指失败，故障地址是高地址 | 高半映射未建立或范围不足（`kern_vir_base`/`kern_size` 与 `map_huge` 的范围；不是 `AT()`——`AT()` 只改 `p_paddr`） | `readelf -lW` 看 `p_vaddr`/`memsz`；`check-layout.sh` 的 L3b/L5；核对 `arch_boot_impl` 的 Step 2 |
| 内核镜像里某段的内容像另一段 | `AT()` 用了常量表达式，且链接器未拒绝 LMA 重叠（旧工具链产出的镜像） | `readelf -lW` 看多个 `LOAD` 行的 `p_paddr` 是否相同（§2.1 反向推演）；现行 GNU ld 会在链接期先报 LMA 重叠，当前工件根本产不出来 |
| 进入 `kmain` 后初期无异常、随后数据静默损坏 | 栈指针未切换（§2.3 的“只跳不切”），栈仍在低侧别名上 | 在 `kmain` 入口打印 SP 与 `kern_stack_top`；SP 落在低半即命中 |
| 进入 `kmain` 后第一次压栈/中断即低地址页错误 | 栈顶值算错（§3.3），SP 落进未映射洞 | 读 QEMU 异常日志的故障地址；核对 `kern_stack_top` 派生 |
| 分页还没开启就 panic，消息是 `no kernel page size satisfies the layout` | 镜像跨距不是大页粒度的整数倍（§3.5） | `readelf -lW` 算跨距；`check-layout.sh` 的 L5/L9；三个量（`kern_virt_base`/`kern_phys_base`/`kern_size`）自己比对 |
| 栈回溯出现与调用关系无关的地址 | 帧指针链的终止点缺失（§4.4 的清帧指针：`xor rbp` / `mov x29,#0` / `li s0,0`；`push 0` 的终止槽不在链上） | 检查 `higher_half.rs` 的 asm 块是否保留了清帧指针动作（§5.3 的 `fp_zero` 断言覆盖它） |
| 模块字节或模块名错位、内核报数量不符 | 装载顺序或名字契约被破坏（模块装载一侧，01 篇的 4.4 节） | 比对 boot-shim 的 `MODULE_NAMES` 与内核的 `BOOT_MODULE_PROC_NRS` |

**诊断注记（临时打印与早期读数）**：x86_64 生产路径上有一块临时打印（`kernel: preA rdi=…`，源码注释自标 `temporary`）；另有一条 `kernel: arch_boot entered` 路标，不是临时打印，它们不属于 §6.1 的正式路标；按本篇复现串口日志时按噪声过滤。这两块打印是诊断噪声；是否删除由后续清理决定，当前不影响判据（原登记为设计批 D2-13）。早期版本 `kmain` 收到的指针 `0x1ddda2ee` 与 `.rdata` 里一个字符串常量的地址相合，是 RDI 被覆写的现场读数；现行实现改为 `.bss` 副本交接（§4.5），这条读数只用于识别旧日志。

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
# 调试对象是带符号的 ELF。riscv64 用 test-higher-half-riscv64 载体（直接调 arch_boot，能到 kmain）：
$ qemu-system-riscv64 -machine virt -smp 1 -m 256M -nographic -bios default \
    -kernel os/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64 -s -S
# x86_64/aarch64 的生产 ELF 用各自固件/盘面参数启动后追加 -s -S，见 `01-boot-shim-bootstrap.md` §6.4。
# 工件必须是带符号的 ELF；UEFI 测试载体的 .efi 是 stripped PE，nm/gdb 都读不到 kmain，不要用它调载体。
# 跨架构宿主用 gdb-multiarch；x86_64 工件用 gdb 即可。
$ gdb-multiarch os/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64
(gdb) target remote :1234
# x86_64 生产 ELF：break minix_kernel::kmain
# riscv64 载体：链接值 0x80200144 + delta（kern_virt_base - kern_phys_base）；
# 下面按本轮的 test-higher-half-riscv64 实算为 0xffffffc000200144。
(gdb) break *0xffffffc000200144
(gdb) continue
(gdb) print/x $pc   # x86_64 用 $rip
(gdb) print/x $sp   # x86_64 用 $rsp
# 预期输出：$pc 与 `nm <工件> | grep kmain` 读出的符号值一致（v1 send/receive 与自引导 direct leg
#           都应当是高半地址；x86_64/aarch64 测试载体内联形态下是垫片镜像里的低地址，riscv64 测试载体是高半 VMA 但等于 `nm` 值加 `delta`，见下一条汇总与 §4.4 的第三种形态）；
#           $sp（x86_64 用 $rsp）落在该架构的 kern_virt_base 之上（x86_64 是 0xffff8000… 一段；aarch64 是 0xffff8000… 一段（`aarch64.ld` 的 `KERNEL_VIRT_BASE = 0xFFFF800000000000`；`0xffff7fffbfe00000` 是 L4 平移量，不是栈地址）；riscv64 是 0xffffffc0… 一段），即 §4.4 的"切栈已生效"。
```

五组命令的预期输出：

- 第一组：`readelf -hW` 的 `Type:` 行应为 `EXEC`（不是 `DYN`），`Entry point address:` 应等于首段虚拟地址；`readelf -lW` 的所有 `LOAD` 行两列地址之差一致（= 高半平移量），首段 `PhysAddr` 等于脚本里的物理基址。
- 第二组：`grep -c ': AT('` 对三份生产脚本各输出 4。这条读数必须带口径：把模式放宽成 `AT(` 会把注释里的字样算进来（x86_64 仍 4、另两份各 5）。两条 `printf` 分别输出 `961` 与 `0xf0000000`。
- 第三组：全绿时最后一行是 `CHECK-LAYOUT=PASS（x86_64）`、退出码 0；任一断言失败时对应行是 `  [Ln] FAIL …` 且最后一行 `CHECK-LAYOUT=FAIL（…）`。
- 第四组：PASS 时串口日志里出现 `### TEST_RESULT: PASS test-higher-half ###`（判定标记见 `os/qemu-tests/run_qemu.sh` 的判定段）；失败时同一位置是 `### TEST_RESULT: FAIL test-higher-half ###`，它之前的若干行里，失败项以 `FAIL:` 开头（`SP not at high address`、`SP not at the architecture's entry alignment`、`FP not zero`，riscv64 另有 `PC not at high address (riscv64 delta path broken)`），而 x86_64/aarch64 的 `INFO: PC not at high address (expected in test kernel)` 是参考信息行、不参与成败判定，别把它读成失败原因（它只在失败分支里出现；riscv64 不打印这一行，PC 低半在 riscv64 是 `FAIL`）；`qemu_int.log` 里另有 `check_exception` 行给出故障类型与地址。
- 第五组：断在 `kmain` 后 `$pc`（x86_64 用 `$rip`）应等于该腿的入口值——v1 send/receive 与自引导 direct leg 下等于 `nm <工件> | grep kmain` 的符号值（高半）；x86_64/aarch64 测试载体内联形态下是垫片镜像里的低地址；**riscv64 测试载体不是 `nm` 值本身**：它链接在物理地址上，`$pc` = `nm` 值 + `delta`（`delta = kern_virt_base − kern_phys_base`）。以本轮 `test-higher-half-riscv64` 为例：`nm` 给出 `0x80200144`，`kern_phys_base = 0x80000000`、`kern_virt_base = 0xffffffc000000000`，`delta = 0xffffffbf80000000`，所以断点/`$pc` 是 `0xffffffc000200144`；`$sp`（x86_64 用 `$rsp`）落在该架构的 `kern_virt_base` 之上（本轮 riscv64 实测 `0xffffffc000400000`；x86_64 `0xffff8000…`、aarch64 `0xffff8000…`）——两条读数合起来即"跳转与切栈都已生效"的现场证据。

---
## 7. 参见


### 7.1 前置与后续篇

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 引导器契约、内存记账、`KernelInfo` 字段与来源记录、五条生产腿的发送侧
- [03-kmain-cstart.md](03-kmain-cstart.md) — `kmain` 入口与保护结构初始化
- [04-platform-discovery.md](04-platform-discovery.md) — 平台描述符的解析
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断控制器初始化
- [../02-stage-vm/07-pagetable-struct.md](../02-stage-vm/07-pagetable-struct.md) — 页表多级结构与页表接口

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/arch/i386/{kernel.lds,head.S,mpx.S,Makefile.inc,pre_init.c,pg_utils.c,protect.c}`
- `minix3/minix/kernel/main.c`
- `minix3/minix/kernel/{const.h,proto.h}`、`minix3/share/mk/sys.mk`、`minix3/sys/arch/i386/include/asm.h`
- `minix3/minix/kernel/arch/i386/{include/arch_proto.h,exception.c,usermapped_data_arch.c,usermapped_glo_ipc.S}`、`minix3/minix/kernel/usermapped_data.c`
- `minix3/minix/lib/libsys/stacktrace.c`
- `minix3/minix/kernel/arch/earm/{kernel.lds,head.S,mpx.S}`
- `minix3/minix/servers/vm/pagetable.c`（[07 篇页表结构](../02-stage-vm/07-pagetable-struct.md)的背景阅读，不在本篇论证链上）
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
