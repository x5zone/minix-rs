# 01 · Boot-shim 引导准备与控制权交接

> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。行号只以符号锚点出现，不引用具体行；引用位置由符号名定位。
>
> **标注约定**：标题或表格里的 `[ARCH: …]` 是项目的**架构演进标注**——标记"这里相对 Minix3 C 的语义边界发生了改变"（三档术语的第三档），按项目约定在文档、设计与代码三处一致出现。读者可以把它当作"这不是 C 的行为，是重写的选择"的记号。正文里另有少量作者侧记号——工作项编号如 `P9-1`、门禁标记如 `[SD19-…]`、"复算"命令——前两者出现处都有就地说明，复算命令是给读者的自查工具。
>
> **目标读者**：熟悉操作系统基本概念（特权级、分页、进程切换）与 Rust 基本语法（trait、泛型、生命周期）的开发者；能读 C 与基础汇编（`mov`/`push`/`call` 一级的指令不再解释）。不需要预先了解 Minix3 源码——引用的 C 符号都有解码；不需要预先了解 UEFI 协议——用到的调用都在正文里交代。
>
> **前置知识**：虚拟内存、页表、中断与引导流程的大意。读过"自己写 OS"类书籍的读者可跳过 §1.1 的固件/引导器背景段；§1.1 末段"为什么第一跳必须是汇编"是后文论证的前提、不建议跳过；§1.3 是第一章收束，可按需回读。
>
> **教学目标**（读完本篇应能独立讲出）：① 引导器交棒时机器处于什么状态、为什么第一跳必须是汇编（§1.1/§2.2）；② x86-32 从 `head.S` 入口到 `pre_init` 返回的完整链条、内存清单的建立与裁剪（§2.2–§2.5）；③ 页表建立：恒等映射与内核映射的次序、重叠语义、借槽窗口（§2.6）；④ 引导期内存账：页从哪里来、何时换手给 VM、空闲清单怎么传下去（§2.6/§3.8/§4.5）；⑤ 三架构交接形态的差异与设计理由（§3.7/§4.8）；⑥ Rust 实现落点与运行阶段边界：EBS（UEFI 的 `ExitBootServices`，退出固件服务那次调用）前后、快照口径与深拷不变量（§4.1/§4.2/§4.5；本页各条的名词都在正文首次出现处展开）；⑦ 引导期故障怎么用串口路标与断言定位（§6）；⑧ 每条核心机制靠什么证据成立、哪些还没有断言（§5 的证据总账）。

内核不是从零开始的。它被放进内存时，机器已经由固件（UEFI 或 BIOS）与引导加载器（MINIX 自带的 boot2、GRUB 或 U-Boot）跑过一段路：内存控制器已经初始化，可执行文件已经被读进 RAM。地址翻译这一步两条路不一样——UEFI 上的固件是开着分页进来的，而 BIOS 上的引导器（boot2 或 GRUB）按 Multiboot 规范必须**关着分页**就把控制权交出来（这就是 1.1 那张阶段表里"分页未开（x86-32）"的来处）。

但这些成果并不是内核可以直接使用的形式——它们要么散在别人填的结构体里，要么被别的代码写在某个寄存器里，要么只数到"哪几段物理地址可用"这一层：真正空闲的页帧要内核自己记账，而引导器给的这份内存信息还分粗细两档（展开见 §2.4）。

这一篇讲清内核接管之前的那一段：谁把机器带到了"内核可以开始执行"的状态，中间凑齐了哪几样东西，哪一步的次序错了会坏在什么地方。

---

## 1. 概述

### 1.1 内核的第一行代码之前还有一段代码

x86 上加电后，机器先执行固件；固件自检、初始化并找到可引导设备，把控制权交给磁盘上的引导加载器；引导加载器读自己的配置、把内核镜像与若干模块读进物理内存、切到保护模式，最后跳到内核的入口。这一段里只有最后一小截属于内核源码。

| 阶段 | 谁在跑 | 机器状态 | 属于本仓库源码吗 |
|---|---|---|---|
| 加电与自检 | 主板固件（BIOS/UEFI） | 实模式起步，固件自己完成模式切换与硬件初始化 | 否 |
| 加载可执行文件 | 引导加载器（boot2/GRUB/U-Boot） | 保护模式或长模式，已能访问磁盘与内存 | 否（只提供配置模板 `minix3/etc/boot.cfg.default`） |
| 引导准备 | 内核源码的最前一段（`head.S` + `pre_init()`） | 分页未开（x86-32），无栈可用、无堆、无中断 | 是 |
| 内核主流程 | `kmain()` 及其后 | 分页已开、内核在高地址执行 | 是（后续各篇） |

第三行才是本篇的对象。"引导准备"这四个字里包含两类工作：**收集**（从引导器留下的数据结构里读出内存布局、模块位置、启动参数）与**搭架子**（把可用内存记成账、把页表建起来、把分页打开）。两类工作都在同一段没有运行环境的代码里完成——没有栈指针可以用（要自己设一个临时栈）、没有分配器、没有任何报错输出通道以外的设施。

上一段里那句"要自己设一个临时栈"是这一整段代码能成立的前提，而且它只能用汇编写。链条是这样：引导器跳到内核入口时，栈指针指的是它自己用过的位置，那块内存属于谁、还能用多久，没有任何约定——Multiboot 规范在"机器状态"一节把除少数几个寄存器以外的处理器状态都列为未定义，并明写"操作系统必须在自己需要栈时立刻建一个"。而 C 函数一进去就要用栈：局部变量、要保存的寄存器、返回地址都落在栈上。于是"重建栈指针"这件事，正好是那件"必须先有栈才能用 C 去写"的事——用 C 写不出自己的第一条 `mov` 到 `esp`。所以第一跳只能是汇编里的一条指栈命令：`i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 用 `mov $load_stack_start, %esp` 把栈指针指到同一个文件末尾 `.data` 段里预留的 4096 字节上（逐行解码见 §2.2）。挑自己镜像里的这一段还有一层实际好处：这段内存已经被内核当成自己占用的一块、从可用内存清单里切掉（机制见 §2.5），不会被后面的分配再发出去；而引导器自己用过的那段栈没人替它做这件事——沿用它就等于把栈搭在一块随时可能被发给别人的内存上。本篇后面因此不再单独解释"为什么第一跳是一段汇编"。

### 1.2 自举要凑齐的四样东西

（第一次读本节只取每条加粗短句与依赖方向；函数名、断言与"961 号项"这类细节都在 §2.6/§2.8 展开，此处留作回读锚点。）

不论引导器是 MINIX 自带的 boot2、GRUB、U-Boot 还是别的什么，把机器交给内核之前必须凑齐四样：

```
    ┌────────────────────────────────┐
    │ 1. 可用物理内存的清单          │  哪些物理区间可以被分配
    │   （内核与模块占用区已排除）   │
    └───────────────┬────────────────┘
                    ▲ 内核自己占的那一段（来自框 2）要在这份清单里切掉；
                    │ 而这份清单也是框 3 的输入——建表的第一步就要读它
                    │ 算出的内存上界（页表页本身的来源见 2.6）
    ┌───────────────┴────────────────┐
    │ 2. 内核自己的位置与大小        │  内核镜像在物理内存的哪一段
    │ （物理基址 / 虚拟基址 / 长度） │  以及它要运行在哪个虚拟地址
    └───────────────┬────────────────┘
                    │ 决定这张表必须覆盖的两段地址：此刻正在执行的地址，
                    ▼ 与内核要去的目的地址
    ┌────────────────────────────────┐
    │ 3. 一张内核可运行在其上的页表  │  当前地址没被覆盖的话，开启分页后的
    │   （当前地址 + 目的地地址）    │  下一条指令就取不到自己的代码
    └───────────────┬────────────────┘
                    │ 建完这张表才知道第一个空闲页目录项的下标是多少，
                    ▼ 这个数要回填进框 4
    ┌────────────────────────────────┐
    │ 4. 一份把上面三样交下去的结构  │  内核主流程要读的就是它
    └───────────────┬────────────────┘
                    │ 分页开启、内核落到高地址之后，才有人读得到它
                    ▼
                kmain() 的第一条 C 语句
```

这四个框排成一条竖线，但**它们不是流水线上的四道工序**：竖线只保证阅读顺序，其中两条箭头并不顺着它走。四样之间的依赖方向决定了真正的次序，逐条如下：

- **内存清单先于建表**。清单在 `get_parameters()` 里建好并裁剪（`cut_memmap` 把内核与各模块占用的区间切掉）。建表对它的依赖分三层，不是一个笼统的"页表页要在这里领"：`pg_identity()` 直接要 `mem_high_phys`（决定哪些页目录项该标成非缓存——这一层没有它就得不出正确的表）；`pg_mapkernel()` 的初始大页映射只用链接期布局常量与已清零的静态页目录，不向记录领页；真正从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度映射路径（`pg_alloc_page`）。反过来排（先建表、后收集），第一层就断——`pg_identity()` 开头的 `assert(cbi->mem_high_phys)` 当场失败（该值还是 `.bss` 的零初值）。
- 内核的位置先于清单的裁剪。清单要排除的区间里包含内核镜像自己；内核镜像的物理区间从链接期常量来（`_kern_phys_base`、`_kern_size`），不来自引导器的数据结构。
- 页表先于开启分页，这是本节唯一一条"硬件强制"的次序。`CR0.PG` 置位后的**下一条取指**就已经在走页表；此时页表里如果没有当前指令所在的虚拟地址的映射，CPU 立刻产生页错误，而页错误处理程序此刻还不存在。
- 结构先于内核第一条 C 语句。`pre_init()` 返回的就是这份结构；它按值或按指针交给 `kmain()`。

还有一个容易读漏的次序：`pg_identity()` 排在 `pg_mapkernel()` 之前。这两步各管一个时刻：恒等表管的是**开启分页那一瞬间**——`CR0.PG` 置位后的下一条取指仍在原来的低地址上，靠它才不会跑飞；内核映射管的是稍后进高半执行的那一步，把内核链接时的高半地址（从 961 号项起，即 `_kern_vir_base = 0xF0400000` 除以 4MB）指到镜像**实际所在的低物理地址**。两者写的是同一个 `pagedir[]`，下标在 961 往上**重叠**，同一槽位是一次直接赋值——谁后写谁生效，而这里的"覆盖"恰恰是想要的：让高半映射后写，把恒等表本来填在那几项里的线性地址（`i × 4MB`，内核镜像并不住在那里）改成镜像的真实物理地址。反过来排会坏成什么样、961 号条目前后两次写入各是什么，逐数字的推演在 §2.6。

C 的这四样分别落在 `pre_init.c` 与 `pg_utils.c` 两个文件里：收集在 `pre_init.c`，记录与页表在 `pg_utils.c`。这个划分是 Minix3 的既有划分；重写把同一条分界换到了另一处——准备与运行分属两个可执行体，第 3 章讲这件事。

### 1.3 把第一章收成五句话

读到这里可以先合上这一章。后面几章的细节都挂在这五句上：

1. **内核不是从一张白纸开始的，但别人跑过的那段路也不等于准备好了。** 固件把内存初始化好、引导器把镜像读进 RAM，可"哪些页能拿来分配"这件事没人替内核记账——它要么散在引导器填的结构体里，要么写在某个寄存器里，要么干脆还没做。
2. **自举要凑齐四样：可用内存清单、内核自己的位置与大小、一张覆盖"此刻在哪"与"要去哪"两段的页表、一份把前三样交给主流程的结构。** 这四样不是流水线：内核的位置要回头拿去把清单裁短（自己占了哪一段就切掉哪一段），而清单必须先于建表——建表的第一步就要读它（`pg_identity()` 用内存上界决定哪些条目非缓存；页表页本身的来源见 2.6）。
3. **四样之间只有一条次序是硬件强制的：页表先建好，分页才能开。** `CR0.PG` 置位后的下一条取指就在走页表，而那时连错误处理程序都还不存在。
4. **建表内部还有一个容易读漏的先后：恒等映射先写、内核高半映射后写。** 两者写同一个数组的重叠下标，后写的说了算——而这次覆盖恰恰是想要的。
5. **这一段代码什么都要自己造：没有栈、没有分配器、没有中断，除了报错的输出通道几乎没别的设施。** 所以"收集"与"搭架子"两类工作得用手工指令一件件完成；重写把这件事从内核里搬了出去，与运行分成两个可执行体。

带着这五句往下读：第 2 章把它们逐条对到 Minix3 的 C 源码上，第 3 章看 Rust 把同一条分界搬到哪里，第 4 章逐处实现，第 5、6 章是测试与诊断。

### 1.4 本篇范围与可以安全跳过的内容

本篇覆盖从引导器跳进内核（或跳进替它准备的那段代码）起，到控制权交给内核主流程为止的全部动作：引导器契约、参数收集、内存记账、模块装载的检查、页表建立与分页开启、以及这段代码在新设计里的组织方式。

以下内容不在本篇展开，遇到时给出指向：

| 可以跳过的内容 | 归属 |
|---|---|
| 链接脚本的虚拟地址/装载地址区别、`AT()` 的完整语义、两段式布局 | `02-higher-half-kernel.md` §2.1–§2.2（§3.5 就地解释本处引文用到的偏移写法） |
| 换栈与跳转必须成对、`k_initial_stktop` 落在哪一段 | `02-higher-half-kernel.md` §2.3–§2.4 |
| ELF 解析器内部（段迭代、错误分类）、段拷贝与 BSS 清零的实现 | `02-higher-half-kernel.md` §4.2 |
| 页表多级结构与页表接口的完整定义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`（§1 概念与 §3 设计决策）|
| 引导期建表之后的 Direct Map 覆盖建立 | 不在本篇展开：§4.6 只讲到建表与开启，覆盖建立属分页运行期（Direct Map＝一段固定虚拟窗口到物理地址的线性映射），该机制的专篇尚未生成。本篇只交代它的时序事实与三个消费点：覆盖建立在建表之后、读 `memmap` 时那些页还完好（§4.5），以及它在 `reserved_regions` 用途（§3.4）与借槽机制对比（§3.9）中的引用 |
| `kmain()` 起的三张表（GDT/IDT/TSS）与保护结构 | `03-kmain-cstart.md` |
| 设备树/ACPI 表的解析细节与硬件描述符 | `04-platform-discovery.md` |
| 时钟与中断控制器初始化 | `05-clock-interrupt-init.md` |
| UEFI 规范本身（协议、表、Boot Services 分类） | 外部资料；本篇只讲清用到的那几个调用 |

（表里与下文反复出现的一个名字先在这里立住：**boot-shim** 是重写把"引导准备"搬出去的那个独立可执行体——它的产物形态与两个接口见 §3.2。）

两处需要单独说明的边界：

`k_initial_stktop` 是 `head.S` 在 `pre_init` 返回之后换栈用的那个栈顶符号。本篇只在 §2.2 与 §2.8 引用它的结论、不复制它落在哪一段的判据，理由是语义上的划分：换栈与跳进高半必须成对——栈一换，紧接着的那次 `call kmain` 就要在高半取指——所以"栈顶落在低半还是高半"这个问题本身就属于"内核怎么走到高半去跑"那一章，归 `02-higher-half-kernel.md` §2.3–§2.4。本篇的边界停在 `pre_init()` 返回（§2.4.2、§2.6.3、§2.6.4 三处下游延伸是显式声明的例外，只为交代内存账与页目录的最终归属）。读的时候有一个坑要避开：这个符号的名字里带着 `__k_unpaged_` 字样，很像在说"它属于分页开启前那段低地址代码"，但名字不是判据——那串字样在定义处是手写在汇编里的，在引用处是构建时改名加上去的，两处都不能证明它进了低地址段；真判据是链接脚本把哪个对象的 `.bss` 收进哪一条输出段，那条推证和它的证据表在 `02-higher-half-kernel.md` §2.4。

模块启动镜像的 ELF 段拷贝细节同属装载，但它的实现分析（段迭代器、错误变体、BSS 清零）归 `02-higher-half-kernel.md` §4.2。本篇只讲到"哪些文件被读、按什么顺序被读、读不到怎么办"，不展开段拷贝算法。

---

## 2. C 源码分析

本章按**四样东西**归类、组内按 C 代码顺序走：**§1.2 那四样东西，在 C 里是由谁、在哪几步、按什么顺序凑齐的**。引导器把机器交给内核之后，代码只有两处——`head.S` 的几条指令与 `pre_init()` 里的几个函数；`get_parameters` 这一个函数因此分两次出场（拷贝与参数在 §2.3、内存收集在 §2.4），两处合起来才是它的全部：

| 节 | 它回答什么问题 | 产出 §1.2 的哪一样 |
|---|---|---|
| 2.1 | 引导器与内核之间到底靠什么对接 | 契约本身（识别头、寄存器、结构体） |
| 2.2 | 第一跳做了哪三件事（"为什么只能是汇编"在 1.1） | 无（搭起能跑 C 的前提） |
| 2.3 | 别人的数据怎么变成自己的记录 | 框 1 的雏形与框 4 的主体 |
| 2.4、2.5 | 可用内存怎么记账、谁来查重叠 | 框 1 |
| 2.6 | 页表怎么建、分页怎么开 | 框 3 |
| 2.7 | 换一种架构，同一串函数还成立吗 | 两侧对照 |
| 2.8 | 本章收成一张调用树与三条次序 | — |

需要路线图时看 2.8 开头那张图，它同时也是本章的收束。

### 2.1 引导器交给内核的那份启动信息

引导器与内核之间靠三样东西对接：内核镜像里的一个识别头、跳转时的寄存器约定、以及一份由引导器填好的结构体。识别头嵌在内核二进制里（不是配置文件），按 Multiboot 规范由引导器扫描镜像前 8KB 找到它才认这个内核（三个字段定义在 `i386/head.S` 的 [`multiboot_magic`](minix3/minix/kernel/arch/i386/head.S:multiboot_magic)）——不过本仓这条路径上跑的是 MINIX 自带的 boot2，它按 ELF 头装载，并不读这个识别头；识别头是给 GRUB 一类 Multiboot 合规引导器准备的。

```asm
/*
 * 代码引用：整块是 minix3/minix/kernel/arch/i386/head.S:multiboot_magic 所在的
 * 那一段连续源码，从 `.balign 8` 起，到 `multiboot_depth` 的取值为止。
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
- `multiboot_magic` 的值为 `MULTIBOOT_HEADER_MAGIC`，定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 `0x1BADB002`。
- `multiboot_flags` 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块请按页对齐）。**没有置视频位**（`MULTIBOOT_HEADER_HAS_VBE`，值为 `0x00000004`）——下一段那四个视频字段虽然写着 EGA/80/25，但按结构体注释（`Valid if mh_flags sets MULTIBOOT_HEADER_HAS_VBE`）引导器应当忽略它们。
- `multiboot_checksum` 的值是 `-(magic + flags)`。引导器验算 `magic + flags + checksum == 0`；三个 `long` 相加为零是"这个头部完整"的证明。它不是校验和算法，是一次加法恒等式。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`。前三个只在 `flags` 里置了 `HAS_ADDR` 位时才被读，后两个同样按位生效；这里都没置位，所以它们保持 0。这三个 0 是一个选择：**这个内核让引导器决定装载位置**，自己不指定。
- 视频部分的四个字段请求 EGA 文本模式、宽度 80、高度 25（`MULTIBOOT_VIDEO_MODE_EGA`/`MULTIBOOT_CONSOLE_COLS`/`MULTIBOOT_CONSOLE_LINES`）。

跳转时引导器约定两个寄存器：一个放确认魔术值，一个放它填好的信息结构体地址。这两个值在内核侧被一条断言和一次拷贝立刻消化掉（见 2.2、2.3）。两个魔术值只差一个十六进制位，方向相反：

| 魔术值 | 值 | 方向 | 谁读它 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | Multiboot 合规引导器扫镜像前 8KB（本仓 boot2 不读） |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | `pre_init()` 的第一条断言 |

配置文件里的每条指令各把一份信息送到上面这个交接面上。先说清读这份文件的是谁：解释 `menu=`、`load_mods`、`multiboot` 这套指令语言的，是 **MINIX 自带的第二级引导**（[`boot/boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c) 的命令表里逐条写着这三个名字），不是 GRUB——GRUB 用自己的 `menu.lst`/`grub.cfg`，只因为同样遵 Multiboot 协议而能引导同一个内核。

```
# 代码引用，minix3/etc/boot.cfg.default 的第一条 menu 行
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
```

| 指令 | 送到哪里 | 内核哪一段读它 |
|---|---|---|
| `multiboot <内核路径> [命令行]` | 内核镜像被读进某个物理地址，然后跳到它的入口；内核路径与 `[命令行]` 拼成的一整串成为 `mbi->mi_cmdline` | 命令行由 `get_parameters()` 解析；镜像地址不从引导器读，它来自链接期常量（见 2.3） |
| `load_mods <模块通配路径>` | 每个匹配到的文件（模块＝内核随后要逐个启动的那些服务进程的镜像，名单与顺序见 §4.3）被读进物理内存，地址与长度填进模块描述符数组，数组地址进 `mbi->mi_mods_addr`，条数进 `mbi->mi_mods_count` | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 把整个数组 `memcpy` 进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 只是命令行里的一个 `key=value` 词，随整串进 `mbi->mi_cmdline` | `get_parameters()` 解析命令行时逐词交给 `mb_set_param()`，最终以 `key=value` 形式落进 `kinfo.param_buf` |

先说清 `rootdevname` 是什么：**它是根文件系统的设备名，写的时候不带 `/dev/` 前缀**——值形如 `c0d0p0`，对应的完整设备路径才是 `/dev/c0d0p0`。这个值由装机脚本剥前缀得到——`minix3/minix/commands/update_bootcfg/update_bootcfg.sh` 里 `rootdevname=` 那一行就是把 `$ROOT`（形如 `/dev/c0d0p0`）过一道 `sed 's/\/dev\///'`；同文件的注释还说明它在 setup 期间会变，所以 `boot.cfg.default` 里只能写成变量 `$rootdevname`，每次刷配置时重算。

回到这三条指令：只有 `rootdevname=` 是"内核能直接看见内容"的——另两条的产物是一个地址或一段内存，内容要看引导器填的结构体。而它能被内核"认出来"，靠的不是引导器，是内核自己的解析函数（见 2.3）。但要注意：**内核自己不解释这个参数**（`grep -rn rootdevname minix3/minix/kernel/` 无命中），它只是把它搬进 `param_buf`；真正需要"根设备是谁"的是用户态（RAMDS 的 `rc` 与 `mib` 两个读者的具体落点见 §2.3 的落链图）。这条分工就是微内核分层的直接体现：**内核当参数表的搬运工，参数的语义留给用户态。**

`load_mods` 的模块数有一个上限，`MULTIBOOT_MAX_MODS` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 20；`kinfo_t` 里的 `module_list[]` 就是按这个数开的数组（`minix/param.h` 的 [`struct kinfo`](minix3/minix/include/minix/param.h:struct kinfo)）。内核用一条断言把它当硬约束——`assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)`：模块数到 20 不是截断，是当场停住。

**约定：`load_mods` 必须写在 `multiboot` 前面。** 约定内容是这样的：`load_mods` 只做一件事——把匹配到的文件名字挂进引导器自己的模块链表（这条命令在 `boot/boot2.c` 的 [`command_load_mods`](minix3/sys/arch/i386/stand/boot/boot2.c:command_load_mods)，它转手交给 `module_add`）。而告诉内核"有模块"的那个标志位，是 `multiboot` 在装载内核时写下去的：`lib/exec.c` 的 [`exec_multiboot`](minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot) 先看模块开关是否开着，开着就按链表当时的内容建一张模块表，建完把 `mi_flags` 的模块位置上（`MULTIBOOT_INFO_HAS_MODS`，值 `0x00000008`），再填条数 `mi_mods_count` 与表地址 `mi_mods_addr`。换句话说，**链表被看见的时刻就是 `multiboot` 执行的时刻**，`load_mods` 写得再对，只要排在后面就赶不上。

**`load_mods` 排在 `multiboot` 后面时，内核停在哪条断言上**：关键在于建表那一步并不要求链表里有东西：链表为空时表照样建出来，只是条数记成 0。于是标志位照样被置上，`get_parameters()` 里那条检查标志的 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会**顺利通过**；真正把内核停住的是同一函数再往后的 `assert(mbi->mi_mods_count > 0)`。这个差别直接影响去哪儿查：按"标志没置"查，会去怀疑引导器没解析 `load_mods`；按"条数为零"查，才会去看两条指令的先后。

**内核停在这条断言上时并不是静默挂死**：C 里有两份同名同职责的 assert 实现，内核链接的是系统服务库那一份（[`libsys/assert.c`](minix3/minix/lib/libsys/assert.c) 的 `__assert13`，打印完直接调 panic），不是用户态那一份（[`gen/assert.c`](minix3/lib/libc/gen/assert.c)，打印完走 `abort()`）——所以在这里失败是 panic 停住并出声，而不是用户态那种进程退出。停机收尾具体怎么出声、串口与屏幕两条通路怎么分流，归 §6.3 的失败模式表（第一行）。

### 2.2 head.S：第一跳只做三件事

这条路径上的引导器（§2.1 已点名：MINIX 自带的第二级引导 boot2）跳到 `MINIX` 标签后，入口标签紧接的一条 `jmp multiboot_init` 先跳过识别头（这一步在下面引文之外；`MINIX:` 与这条 `jmp` 在源文件里紧挨在 §2.1 引文段之前——那段引文本身从 `.balign 8` 截起），随后 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 只做三件事：设栈、清帧指针与标志、按 C 调用约定把两个寄存器值变成两个参数。

```asm
/*
 * 代码引用，minix3/minix/kernel/arch/i386/head.S:multiboot_init。
 */
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

逐行解码：

1. `mov $load_stack_start, %esp`：给自己设一个栈顶。`load_stack` 是同一个文件末尾 `.data` 段的 4096 字节静态空间（`.data` / `load_stack: .space 4096` / `load_stack_start:`），`load_stack_start` 是它的**高端**地址。这一行是"能进 C"的全部原因。
2. `mov $0, %ebp`：清零帧指针。栈回溯靠 `%ebp` 链逐帧走，`0` 表示链到此终止（内核 panic 打印调用栈、QEMU + GDB 远程调试都要用）。
3. `push $0` / `popf`：先把 0 压栈，再把它弹进 EFLAGS。净效果是栈深不变（`popf` 消耗掉刚压的 4 字节），改的是标志寄存器：关中断（`IF=0`）、清嵌套任务位（`NT=0`）。
4. 紧跟的第二个 `push $0`：栈上的一个空位。它与后面 `push %ebx`、`push %eax`、以及 `call` 推入的返回地址一起，恰好凑满 16 字节（4 个双字）；`load_stack_start` 本身按 4096 对齐（`load_stack` 所在的对象被收进链接脚本里 `ALIGN(4096)` 的 `.unpaged_data` 段），于是 `pre_init` 入口处的栈指针落在 16 字节边界上。这一行源码没有注释，它的用途只能从效果上读：去掉它，入口栈指针就不再是 16 的整数倍。
5. `push %ebx` / `push %eax` / `call pre_init`：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压，第一个参数（魔术值）后压。`pre_init(u32_t magic, u32_t ebx)` 的两个形参就位。
6. 到此为止是本篇的范围。下一行 `mov $k_initial_stktop, %esp` 与随后的 `call kmain` 属于换栈与跳转，归 `02-higher-half-kernel.md` §2.3；`k_initial_stktop` 落在哪一段属于它的 §2.4——本篇只在 §2.8 引用那个结论，不重复那套判据（为什么本篇接得住这个结论，见 §1.4）。

三件事就是：设栈、清帧指针与标志、按调用约定传参（"跳过识别头"是入口标签那一跳，不在 `multiboot_init` 内）。

`load_stack` 放在 `.data` 而不是 `.bss`，差别不在"能不能用"——栈是先写后读的结构，内容脏不脏没人管——而在**这段字节的确定性由谁负责**：`.data` 里那 4096 个零字节是镜像文件自带的，装载时按普通内容拷进内存，不依赖装载器的行为；放进 `.bss` 就没有这些字节，清零得靠装载器（本仓 [`libsa/loadfile_elf32.c`](minix3/sys/lib/libsa/loadfile_elf32.c) 在调用方给了 `LOAD_BSS` 这一位时清）。所以这条选择省下的是一个对外部组件的依赖，不是一处崩溃；2.7 引的 ARM32 入口设的是同一个 `load_stack_start`，它在 [`earm/head.S`](minix3/minix/kernel/arch/earm/head.S) 末尾同样落在 `.data`。

**把两条 `push` 对调会怎样**：i386 约定下最低地址就是第一个参数，没有另一种写法。对调之后 `pre_init(magic, ebx)` 收到 `magic = ebx`（`multiboot_info` 的物理地址，一个看起来完全合法的数字），`ebx` 收到 `0x2BADB002`。`pre_init()` 最顶端那条 `assert(magic == MULTIBOOT_INFO_MAGIC)` 当场失败，走 2.1 末尾那条链：屏幕刷出 panic 与诊断缓冲、等一个按键才复位（串口这个出口此时不会出声——`do_serial_debug` 要等命令行解析才可能被置起来，而这条断言排在解析之前）。值得记住的是**误诊的方向**：这条断言只打印失败的表达式本身（形如 `assert "magic == MULTIBOOT_INFO_MAGIC" failed`），并不打印值；而此刻 `magic` 里躺着的是 `ebx` 送来的地址——一个看起来完全合法的数字。不核对两条 `push` 的次序，容易往"数据结构坏了"的方向查。

### 2.3 get_parameters：把别人的数据变成自己的记录

`i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 是收集阶段的正文。它做的第一件容易被跳过的事是**整份拷贝**：不长期持有引导器给的指针，而是把整个结构体复制进自己的全局 `kinfo`。

```c
/*
 * 代码引用（三处省略号：前两处略去声明行——`mbi` 的声明行保留，因为下面每一条
 * 语句都要用它；第三处略去函数后段的其余语句），
 * minix3/minix/kernel/arch/i386/pre_init.c:get_parameters。
 * 被引到的每一行逐字保留。
 */
void get_parameters(u32_t ebx, kinfo_t *cbi)
{
	multiboot_memory_map_t *mmap;
	multiboot_info_t *mbi = &cbi->mbi;
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

- `memcpy((void *) mbi, (void *) ebx, sizeof(*mbi))`：把引导器结构体整份拷进 `cbi->mbi`。注意这里有个同名陷阱：`mbi` 是局部指针，而 `kinfo` 的第一个字段也叫 `mbi`（`minix/param.h` 的 [`struct kinfo`](minix3/minix/include/minix/param.h:struct kinfo) 里的 `multiboot_info_t mbi;`）——声明行 `multiboot_info_t *mbi = &cbi->mbi;` 就是把前者指向后者。看到 `memcpy` 里光秃秃一个 `mbi` 就以为它是全局变量，是这一段最常见的误读。拷贝之后，`ebx` 指向的那份引导器结构体不再被引用；后面要读模块表与命令行，走的是这份拷贝里记的指针。
- 这五个字段分两类来源，而**分类本身就是这一段的知识**：`mem_high_phys = 0` 是"待累计"的初值，稍后由 `add_memmap` 把它推到最高可用地址；另外四个（`user_sp`、`vir_kern_start`、`bootstrap_start`、`bootstrap_len`）**没有一个来自引导器**，它们取自链接期常量 `_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`——也就是内核镜像自己的布局。道理是：**内核被装在哪儿、内核的哪一段是分页前用的，这些事实在链接那一刻就已经定下，内核不必问任何人**。这正是 1.2 那两条依赖的根据：框 2（内核的位置）不接收框 1（清单），反过来倒是框 2 的区间要拿去把框 1 裁短。读这段代码时有两个小注意：`vir_bytes` 类型上的 `&符号` 取的是符号的地址值（值在链接期就定了，运行时只是读一个常数）；`user_sp` 这里先赋 `_kern_vir_base` 只是占位，同一函数后段会被 `USR_STACKTOP` 覆盖。至于这些符号的名字为何带着 `__k_unpaged_` 前缀、哪些对象被算进"分页前那一段"——那是构建期的安排：段划分与边界符号见 §3.5，改名机制的判据在 `02-higher-half-kernel.md` §2.4，本篇不重复。（引文里 `cbi->kmess = &kmess` 的 `kmess` 是 `kmessages` 字段的宏别名，见 [`kernel/glo.h`](minix3/minix/kernel/glo.h)。）
- `bootstrap_len` 是"两个符号地址相减"，得到一小段内核镜像内的区域长度。这段区域叫 `unpaged`。

  > `unpaged` 指的是链接在**低地址**、虚拟地址等于物理地址的那一小段引导期代码与数据——建页表与开分页这几步必须由它完成（否则刚开启分页的下一条取指，要去查一张还没建好的页表）。它不是"未分页的段"（那是 CPU 状态）；这段内存用完由 `kmain` 归还给空闲清单，细节见 §3.5。
- 命令行解析是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 直到 `=` 或空格、遇 `=` 后收集 `value` 直到空格。`if (*p++ != '=') continue;` 那一行处理"不是 `key=value` 形式的词"——注意 `p++` 已经把游标推过一个字符，所以 `continue` 之后从下一个字符继续扫，不会死循环。
- 每个 `key=value` 都过一遍 `mb_set_param`。然后（命令行解析之后）**无条件**再写两次：`arch=` 与 `board=`，值来自编译期的板卡身份（i386 上固定查 `BOARD_ID_INTEL`）。这两行意味着：即使引导器什么都没传，内核也保证自己知道"我是 x86 架构的 Intel 板"。

`i386/pre_init.c` 的 [`mb_set_param`](minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param) 是这串文本的落地函数，它同时做三件事：识别少数"有专用字段的键"、维护"键不重复"、维护 `param_buf` 的可追加格式。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param。
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

- 头两行 `strcmp` 是**唯一的两个**"有专用字段的键"：`SERVARNAME` 与 `SERBAUDVARNAME`（定义在 [`minix/const.h`](minix3/minix/include/minix/const.h)，值为 `"cttyline"` 与 `"cttybaud"`）。命中时它们不是写进 `param_buf` 就完事，而是同时设置 `cbi->do_serial_debug` / `cbi->serial_debug_baud` 这两个结构体字段——因为"从哪个串口做内核调试输出"是内核在启动早期自己要做决定的事，不能等到用户态去读参数。
- "已存在先删"这一段是本函数的第一个不变量：同名键最多一份。它找到 `name` 后紧跟 `=` 的位置，把该条字符串的末尾（`q` 走到 NUL）之后的内容整段左移，覆盖掉旧条目。
- 找末尾的循环 `for (p = bigbuf; p < bufend && (*p || *(p + 1)); p++)`：它一直走到"当前字节与下一个字节都是 NUL"的位置。也就是说 `param_buf` 不是一条字符串，而是**一串以 NUL 分隔的 `key=value`，整串以一个额外的 NUL 收尾**（末尾效果是两个连续 NUL）。随后 `if (p > bigbuf) p++;` 把写入点挪到那个终止 NUL 之后。空缓冲时 `p == bigbuf`，不跳过，从头开始写。
- 容量检查用 `p + namelen + valuelen + 3 > bufend` 提前拒绝，返回值 `-1`——但调用点全部忽略返回值（`get_parameters` 里三处调用都没有检查：命令行解析循环里一处、固定键 `arch`/`board` 各一处）。这意味着"参数缓冲装不下"是静默丢弃，而不是启动失败。
- 最后五行写入：`name` + `=` + `value` + NUL（`strcpy` 自己带的）+ 再补一个 NUL。两个显式 NUL 写下的是新的终止符。

于是 `rootdevname=<设备名>` 的完整落链是：

```
boot.cfg.default: rootdevname=$rootdevname
   → 引导器把它拼进 multiboot 命令行
   → mbi->mi_cmdline（一整串，含 bootopts=… 等其它词）
   → get_parameters() 逐词扫描，拆出 name="rootdevname" value=<设备名>
   → mb_set_param(kinfo.param_buf, name, value)   ← 这一步是"被认出来"的位置
        · 不是专用字段（只有 cttyline/cttybaud 是），所以只写进 param_buf
        · 以 "rootdevname=<设备名>\0" 的形式追加，并保证同名键只有一份
   → 落到 param_buf 之后，rootdevname 的读者全在用户态，而两条路共用同一个系统调用：
     读者一：命令行 `sysenv rootdevname`（minix3/minix/commands/sysenv/sysenv.c 里
        svrctl(PMGETPARAM, …)）→ 系统任务 GET_MONPARAMS
        （minix3/minix/kernel/system/do_getinfo.c）把 kinfo.param_buf 整块拷给调用者
        → 拿它去用的是 RAMDS 的启动脚本 minix3/minix/drivers/storage/ramdisk/rc：
          取回后拼成 /dev/xxx 交给驱动，没设时打印 "rootdevname not set"
     读者二：同一条 PMGETPARAM 路径，mib 服务也走一次：
        minix3/minix/servers/mib/kern.c:mib_kern_root_device 取值后作为
        MIB 节点 kern.root_device 暴露（人可用 svrctl pm get rootdevname 查）
     注意：内核自身没有 rootdevname 的读者（`grep -rn rootdevname minix3/minix/kernel/`
     无命中）——它是留给用户态的名字
```

上面这张图里要把两件事分开：**内核侧有读这张表的机制，但 `rootdevname` 没分到读者**。`kernel/main.c` 的 [`get_value`](minix3/minix/kernel/main.c:get_value) 与 `env_get` 服务的对象是 `arch=`、`board=` 这两个键（键名定在 [`minix/const.h`](minix3/minix/include/minix/const.h)，`kmain` 开头就用它取板卡身份）——下一段推演讲的正是 `board` 被引导配置里多写的一个词改写的后果。`rootdevname` 不同：它穿过内核，只为了到达用户态。

所以"`rootdevname` 为什么要经 `mb_set_param` 才被认出来"的答案是：命令行在 `mbi` 里只是一串字节，内核里没有任何一处按字符串搜索引导器内存；`param_buf` 才是内核自己的参数表，而写入它、保证键唯一、维护分隔格式的唯一入口就是 `mb_set_param`。不经过它，`rootdevname` 就只是一串留在 `mbi.mi_cmdline` 里、之后没人再读的字符。

**删掉"已存在先删"那段循环会怎样**：`get_parameters` 在解析完命令行之后无条件再写一次 `arch=` 与 `board=`，于是当命令行里已经带有 `arch=` 或 `board=` 时，`param_buf` 里会出现同一键的两条记录。读取方 `get_value()`（[`kernel/main.c`](minix3/minix/kernel/main.c)）是从头扫描、**返回第一个匹配**——于是被返回的是命令行里那一份，而不是内核自己按编译期板卡身份写的那一份。具体后果落在 `kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 开头那句 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`：板卡身份会被一条外部输入改写，而改写者是引导配置里多写的一个词。

### 2.4 一份内存账的两个操作：add_memmap 与 cut_memmap

`memmap[]` 里的条目从哪来，是这一节的第一件事。引导器给的是**两档**信息里的一档，`i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 先看细的那一档，取不到才退回两个数：

```c
/*
 * 代码引用（保留原文件的混合制表符与空格缩进），
 * minix3/minix/kernel/arch/i386/pre_init.c:get_parameters 的内存收集段。
 */
	memset(cbi->memmap, 0, sizeof(cbi->memmap));
	/* mem_map has a variable layout */
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

逐条解码：

- 细的那一档看 `mi_flags` 的 bit 6（`MULTIBOOT_INFO_HAS_MMAP`，值 `0x00000040`，定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是逐区间的表，条目来自 BIOS 的内存图查询（中断 0x15、`EAX=0xE820`）。循环里那句 `if(mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue;` 说明进账从一开始就只留"可用"这一种语义：保留区、ACPI 数据区都在这一步被丢掉，而不是留着让下游判断。可用的类型值是 `1`，与 E820 里"可用"同号——这里不是一次翻译，是一次沿用。
- 遍历步长那句 `mmap + mmap->mm_size + sizeof(mmap->mm_size)` 与头文件的注释同源：[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 里 `struct multiboot_mmap` 的注释写明 `mm_size` 描述的是**除它自己以外**那些字段的长度，要在条目之间跳就得数 `mm_size + 4` 字节。这个"长度字段不算进自己的长度"的约定也是从那次中断的返回缓冲区继承来的；本仓自己发那次查询的代码在 [`lib/biosmemx.S`](minix3/sys/arch/i386/stand/lib/biosmemx.S)（`movl $0xe820, %eax; int $0x15`），可以对照同一份缓冲区布局。
- 粗的那一档看 bit 0（`MULTIBOOT_INFO_HAS_MEMORY`，值 `0x00000001`），只有两个以 KB 计的数。`else` 分支把它们硬造成两段：`[0, mem_lower*1024)` 与 `[0x100000, 0x100000 + mem_upper*1024)`。这里藏着一个假设——`mem_upper` 的语义是"从 1MB 往上**连续**可用多少"，而 `add_memmap` 收到的是一个长度，于是这条分支把"一整段连续区间"当成了事实。
- 两个分支还有一处不对称：`if` 分支显式写 `cbi->mmap_size = 0;`，`else` 分支不写。第一行的 `memset` 只清 `memmap[]` 数组本身，`mmap_size` 是 `kinfo` 的另一个字段，不在那次清零范围内；`else` 分支仍能正确起来，靠的是 `kinfo` 作为全局对象落在 `.bss`、初值为 0，再加上 `add_memmap` 里那句 `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;` 会把下标补上来。

**引导器只给粗档那一头会怎样**：`else` 分支不是"罕见机器上的边角路"，它是一档会静默变坏的降级。走这条分支时，1MB 之上被记成一整段连续可用区间；若这台机器的保留区确实落在这段范围之内（显卡的镜像区、ACPI 数据区都常见），那么后面的 `i386/pg_utils.c` 的 [`pg_alloc_page`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page) 从最高条目尾部向下发放、迟早会消耗到保留区所在的位置，把页表页这类要长期住着的结构放进本该保留的区域（保留区越靠这段的高地址端，撞上得越早）。症状不是"分配失败"，而是写入静默丢失或机器随机挂死，而且报错的位置离真正的原因（引导器只置了 bit 0）很远。这也解释了本篇 1.2 的框 1 为什么写成"可用物理内存的清单（内核与模块占用区已排除）"而不是"引导器给的内存数"：清单的形态取决于引导器给哪一档，而"排除占用区"这一步（`cut_memmap`）两档都躲不掉。

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 定义在 [`minix/param.h`](minix3/minix/include/minix/param.h) 为 40。数组里每一项是"一段可用区间"，`mmap_size` 记录有效项数。两项操作：追加（`add_memmap`）与切除（`cut_memmap`）。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap。
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

- `LIMIT 0xFFFFF000` 与两次判断：`addr > LIMIT` 直接丢弃整条；区间跨越 `LIMIT` 时把长度截到边界。两条判断的次序有实际意义——先判 `addr` 把整条在 4GB 之上的描述符整条丢掉（不改 `len`），再判跨界。`0xFFFFF000` 而不是 `0x100000000` 是因为地址要能安放进 32 位的 `phys_bytes`（`memmap[]` 里 `mm_base_addr`/`mm_length` 本身是 64 位字段，但内核通篇以 32 位的 `phys_bytes` 收口），并且低 12 位为零。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）加一条提前返回：`if(len == 0) return;`——长度为零的区间被**静默丢弃**，不是断言。`kernel_may_alloc` 是全局开关（定义在 [`i386/pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c)，初值 1；声明在 [`kernel/glo.h`](minix3/minix/kernel/glo.h)）。它分的不是"引导期 / 非引导期"这种时段，而是**这份账归谁**：`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 开头显式写一次 1，源码注释是"Kernel may use bits of main memory before VM is started"——此刻内核还能自由取用内存；而 `kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting) 在 `switch_to_user()` 前一行把它写成 0，注释同样直说理由："Kernel may no longer use bits of memory as VM will be running soon"。跨过这条线再想改账，撞上的就是上面那几条 `assert(kernel_may_alloc)`：改账动作当场失败，而不是悄悄改写一份已经交给 VM 的账。
- `roundup(addr) / rounddown(len)`：**起点向大、长度向小**。这是使"返回的区间整体落在原区间之内"的对齐方向；反过来（起点向小、长度向大）会向外扩出一个不存在的页。
- 追加写到哪一格：`memmap[]` 是定长 40 项的数组，**每一项记一段可用区间**。找空位很朴素——从下标 0 往后看，第一个长度为 0 的格子就是空位（`if(cbi->memmap[m].mm_length) continue;`：长度非零，说明这格已被一段区间占着）。这里**没有单独的"已占用"标志位**，用的是"长度为零"等价于"这格没数据"；而这个等价能成立，又正好靠上一条那句 `if(len == 0) return;`——长度为零的区间根本不会被写进来。于是零长度项在数组里只有一种含义：空格。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：因为是"拿第一个空位"而不是末尾追动，新条目可能落在比当前边界更低的下标上，所以 `mmap_size` 要取 `m+1` 而不能简单自增——它代表的是**遍历边界**（最高占用下标加一）。还要说清一点：**数组中间可以有零长度的空洞**（一次切除把整条抹掉时就会留下），所以每个消费者都得自己跳过零长度项——`pg_alloc_page` 从最高下标往回扫时，第一件事就是 `if(!mmap->mm_length) continue;`，那就是空洞存在的证据。
- `mem_high_phys` 只增不减：它是所有条目的最高地址上界（`addr + len`）。`pg_identity()` 依赖它（见 2.6）。

切除是这里的核心操作，因为它要把一段区间从记录里去掉，而记录里的一段区间可能被切成两段（`i386/pg_utils.c` 的 [`cut_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap)；追加那一半在 `i386/pg_utils.c` 的 [`add_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap)）。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap。
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

#### 2.4.1 从记录里取址的两个消费者：`alloc_lowest` 与 `pg_alloc_page`

除这两个函数外，`pg_utils.c` 还有两个从内存记录里取地址的函数，一个从最低处取、一个从最高处取（`i386/pg_utils.c` 的 [`alloc_lowest`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest)、`:pg_alloc_page`）；另有一个 `alloc_pagetable`，它不从记录取页，用的是自己的静态池，放在 §2.6 讲：

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

`pg_alloc_page` 从**最高**的条目开始取（`for(m = cbi->mmap_size-1; ...)`），取该条目的最后一页（先减长度，再返回"基址+新长度"），并把用量记进 `kernel_allocated_bytes_dynamic`。它取页前在循环体内有三条断言（另有函数入口的 `assert(kernel_may_alloc)`），其中 `assert(!(mmap->mm_length % I386_PAGE_SIZE))` 是"记录里所有长度都必须是页的整数倍"这条不变量在"发页"这一步的校验点（写账一侧由 `add_memmap` 的 `rounddown` 保证，见本节末的取整推演）。

`alloc_lowest` 则**不是**一条可行的"从低处分配"路径，问题出在切除那一行：它写的是 `cut_memmap(cbi, lowest, len)`，而 `cut_memmap(cbi, start, end)` 的第三个参数才是结束地址——这行调用把长度当成了结束地址，完整推演见本节末的"若 `alloc_lowest` 的切除参数写错会怎样"。

它的唯一调用点在 SMP 路径：`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 里 `trampoline_base = alloc_lowest(&kinfo, tramp_size);`，用途是把一小段 16 位启动代码拷到 1MB 以下给辅助处理器执行。紧接着的校验只查了返回值的上界：

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline。
 */
	tramp_size = (unsigned) &__trampoline_end - tramp_start;
	trampoline_base = alloc_lowest(&kinfo, tramp_size);

	/* The memory allocator finds the lowest available memory.. 
	 * Verify it's low enough
	 */
	assert(trampoline_base + tramp_size < (1 << 20));
```

注意 `assert(trampoline_base + tramp_size < (1 << 20))` 检查的是"地址足够低"，而不是"这段内存已经从账上切掉"。所以这条校验通不过时能发现"分配得不低"，却检查不出"分配了但账没改"。

**若 `cut_memmap` 不把 `start` 向下、`end` 向上取整**：补回余量时走的仍是 `add_memmap`，它自带 `roundup(addr)`/`rounddown(len)`，所以**记录里不会出现非页对齐长度**——这一条不变量由 `add_memmap` 独立保证，与 `cut_memmap` 的取整无关。取整真正改变的只有一种情形：**退化请求**（`start == end`）且首尾地址未落在页边界上。带取整时它被扩成"这一页整页出局"；不带取整时夹取后 `substart >= subend`，一次循环都不命中，等于什么都没切——被切对象照样留在空闲清单里，等着被第二次发出去。（退化请求恰好页对齐时，两个取整都不生效，取不取整同样是"什么都没切"。）其余情形（起点未对齐、终点未对齐、区间跨页、整条覆盖）取不取整**结果相同**，因为补回的余量都要再过 `add_memmap` 的对齐，边界上的半个页本来就会被丢掉。两处取整的分工因此是：`add_memmap` 保证"账上永远不出现非页对齐的条目"，`cut_memmap` 的取整保证"任何切除请求至少让涉及的那一页出局，不会因为区间退化而漏切"——方向是保守的，代价最多是一页可用内存。

**若 `alloc_lowest` 的切除参数写错会怎样**。`cut_memmap` 的第三个参数是**结束地址**（签名：`void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)`），而这里传的是 `len`——一个长度。于是"实际切掉的区间"是 `[lowest, len)`，"想要切掉的区间"是 `[lowest, lowest+len)`，两者相差正好 `lowest` 个字节。分三种情况：

| `lowest` 与 `len` 的关系 | 实际切掉 | 后果 |
|---|---|---|
| `lowest == 0` | `[0, len)`，与想要的一致 | 行为正确——但这个分支在真实调用序下不可达（`arch_init` 早已把 `[0, 0x1000)` 切掉，见下） |
| `0 < lowest < len` | `[lowest, len)`——切了，但**比分配量少了 `lowest` 字节** | 尾部那 `lowest` 字节仍挂在空闲清单上 |
| `len ≤ lowest` | 夹取后 `substart >= subend`，一次都不切 | 整块被占用的内存从未从账上除去 |

真实调用序下的常态是第三行。`alloc_lowest` 只在 SMP 路径被调用（`copy_trampoline`），那时 `kmain` 已经跑过 `cstart`，而 `cstart` 末尾的 `arch_init` 先切掉了两块低内存（`i386/arch_system.c` 的 [`arch_init`](minix3/minix/kernel/arch/i386/arch_system.c:arch_init) 的两条 `cut_memmap`：BIOS 区 `[0, 0x4FF]` 与 `[0x90000, 0xFFFFF]`，取整后即 `[0, 0x1000)` 与 `[0x90000, 0x100000)`）。所以表里最低条目的基址不可能还是 0（典型 QEMU/PC 上是 `0x1000`），而 `len` 经 `roundup` 是一页（跳板本体约 2KB）——`len ≤ lowest`，切除一次都不发生。也就是说这行代码在真实调用序下**就是**在"分配了但账没改"的状态下运行的；这个缺陷至今没有暴露成可见故障，靠的是 `pg_alloc_page` 从上往下发页、常规启动里够不到低区那几页（本节末"两个取址方向各有其消费者"一段正是这个分工），而不是它写对了。

后两种情况的共同后果是：返回的地址会被 `copy_trampoline` 拷进 16 位启动代码，而这段物理内存**在账上仍然是空闲**（或至少残留一段）；此后任意一次 `pg_alloc_page` 都可以把它当成空闲页发出去，辅助处理器启动到一半时它的启动代码被改成页表内容——症状是某个 AP 起不来或执行到随机地址，而不是"内存分配失败"。这也解释了为什么看到"分配"两个字，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址。

两个取址方向各有其消费者，上面两个函数恰好各走一头。`pg_alloc_page` 从最高条目取页，让记录里最低的那一段保持连续；唯一要求低地址的消费者是辅助处理器的启动跳板——`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 经 `alloc_lowest` 取它，并断言结果落在 1MB 以下——而 `alloc_lowest` 是从最低处扫的。两个方向互不消耗对方要的地址。

#### 2.4.2 模块内存与 unpaged 段的归属变更（下游延伸）

`cut_memmap` 在 `pre_init` 里被调用时只是一次**临时**切除。接下来两处"改这份账"的动作发生在 `pre_init()` 返回之后（本章主线的下游延伸）——要把这份记录的完整生命周期一次讲清，不能只看引导准备那一段。与模块内存和 unpaged 段归属有关的动作共三处，对象各不相同；下面一律**按动作命名**（按先后编号容易和别处的"第几点"混起来）：

| 名称 | 对象 | 时刻 | 谁做 | 动作 | 为什么 |
|---|---|---|---|---|---|
| **切占用** | 内核与全部模块的字节 | `pre_init` → `get_parameters` | `cut_memmap`（本文 2.4） | 从可用记录里切掉 `[mod_start, mod_end)` | 这段内存里躺着模块的字节，页表页与内核数据不能分配到这里 |
| **还 VM 镜像** | 只有 VM 模块的字节 | 内核装载启动进程时（[`arch_boot_proc`](minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc)） | `add_memmap` + 清空模块描述符 | 把模块的字节段复制进目标地址空间后，把这段物理区间加回记录，并把 `mod_start`/`mod_end` 置 0 | 字节已经搬到别处，原地址可以复用；置 0 是"已回收"的标记 |
| **还引导段** | 内核自己的 unpaged 段 | `kmain` 后段（`cstart`、`system_init` 之后） | `add_memmap(bootstrap_start, bootstrap_len)` | 归还分页开启前使用的那一小段 | C 的 `unpaged` 段只在启动期需要（本文 §3.5），与模块无关 |

两条容易混的口径：**"切占用"与"还 VM 镜像"处的是模块字节，"还引导段"处的是内核镜像自己的 unpaged 段**（边界由链接脚本导出的 `_kern_unpaged_start/end` 给出）。另外，"还 VM 镜像"只还 VM 那一块，其余模块的字节区间在 C 里从头到尾都没加回过——代价见本节末。

"还 VM 镜像"的源码（本篇 §3.8 引用的扣除表把这条 C 对应关系**改述**过一行，不是代码引用）：

```c
/*
 * 代码引用（保留与该动作直接相关的行），minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc 中 VM 模块装载完成处。
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

读这段源码要留意的三点：

- `add_memmap` 的实参是 `mod->mod_start` 与 `mod->mod_end - mod->mod_start`（起点与**长度**），与 2.4 里 `cut_memmap(cbi, lowest, len)` 那个把长度当结束地址的写法形成对照：同一个文件里的两组调用对"第三个参数是什么"并不一致（`add_memmap` 的第三个参数是长度、`cut_memmap` 的是结束地址），读的时候只能逐个核对被调函数的签名。
- 置零发生在加回之后，且两个字段一起置 0。此后任何遍历 `module_list[]` 的代码都会看到一条"空模块"；这是"已回收"在数据结构里的表示（而不是另设一个布尔位）。
- 这一次回收只针对 VM 的模块（`vm_allocated_bytes` 紧跟着被记录），因为它的字节已经被复制进 VM 的地址空间。其余模块在 C 里没有对应的加回动作（见本节末的代价分析）。

**其余模块从不归还，代价是什么**：代价只有一条——**账上的可用总量永久少掉这 11 个模块的字节区间**。被切掉的区间停在"已切"形态（条目的 `mm_length` 保持 0，`cut_memmap` 清空之后再没人 `add_memmap` 把它加回）；那段内存本身没有损坏，只是再也不会被领用。

这条代价不该被夸大成病根：内存宽裕时少这几段照样启动完成（C 就是这么跑完的）；内存本来就不够时，就算把 VM 之外那 11 段也还回去，一样会分配失败。它至多是压垮可用总量的最后几页，而不是原因。启动期唯一会走到 `pg_alloc_page` 的是装载启动镜像那条链（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 传 `PG_ALLOCATEME` 给 `pg_map`），所以真到了耗尽那一刻，panic 会打在装载途中——但那只是**报信的地方**，不是**出错的地方**。

它真正的坏处是安静：不报错、不留痕。也因此有确定的查法——引导期的占用都记在两个账上：`kernel_allocated_bytes` 是内核镜像自己那一块（`_kern_size` 减去 `bootstrap_len`），`kernel_allocated_bytes_dynamic` 则是 `pg_alloc_page` 与 `alloc_lowest` 每发一页累加一次；而"从不归还"的那几段**两边都不记**。拿固件报告的总 DRAM 依次减掉这两笔，再去对 `memmap[]` 的可用总量，对不上的那一截就是它。

"还 VM 镜像"在新设计里的处理与这里不同（落点与理由见 §3.8 的扣除表）；"还引导段"这一步则不存在，因为 `bootstrap_len` 恒为 0（§3.5）。

**本节结算**：走到这里，机器上第一次有了**一份可信的可用物理内存清单**——条目来自引导器（细表或粗数两档之一），内核与各模块占用的区间已经从里面切掉，`mem_high_phys` 给出了上界。往后从这份账里取页的，是 `pre_init()` 之后的 4KB 细粒度映射路径与 VM 接手后的所有分配——引导期这次建表本身用的是静态页目录与静态池，不从账上领页（见 2.6）；这些取页动作都以这份账为唯一依据。

### 2.5 overlaps 与 kern_mod：内核把自己当成第 N+1 个模块

内存账建好后，要从中切掉两块东西：模块占用的区间与内核自己占用的区间。切之前先检查它们彼此是否重叠——`overlaps` 就是源码注释里说的那道 sanity check。实际布局里相邻模块由 boot2 顺序摆放、互不重叠：boot2 一边装载一边推进 `image_end`（[`lib/exec.c`](minix3/sys/arch/i386/stand/lib/exec.c) 里 `bi->base = image_end;` … `image_end += len;`，下一条再从页对齐处接着放——前一条长度恰为页的整数倍时两者正好相切，否则中间留一段对齐补白），所以正常情况下谁也不会盖住谁。但"相切"恰恰是边界处理最容易写错的那一种相邻关系——本节末尾就专门推演它。真出现重叠（换个引导器、或模块与内核被放在同一处）时——**若没有这道检查**——症状会非常难追：两个镜像互相覆盖而无人报错。

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:overlaps。
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

- 区间语义是半开区间 `[start, end)`：`INRANGE` 写成 `v >= start && v < end`。`mod_end` 为什么是"最后一个字节之后的那个地址"？依据在引导器一侧：boot2 填的是 `mbm[i].mmo_end = bim->base + bim->len`（起点加长度，同在 [`lib/exec.c`](minix3/sys/arch/i386/stand/lib/exec.c)）。注意 minix3 的 `multiboot.h` 对 `mmo_end` **一句注释都没写**——这个语义只能从填它的那行读出来，而不是从结构体定义里看到。
- `OVERLAP` 判两个点：对方区间的**起点**与对方的**最后字节**。第二项写成 `mod2->mod_end - 1`，就是把半开区间翻译回"最后一个有效字节"。
- `overlaps` 检查 `mod[cmp_mod]` 与其余每一条，只要有一条相交就返回 1。注意它是对每条记录各调用一次（外层循环在 `get_parameters` 里），所以一对重叠会被两个方向各检查一次；这使第二项 `-1` 的必要性变得具体（见下）。

内核自己不带 `multiboot_module_t`，但检查与切除都要把内核算进去（检查那一半是 `i386/pre_init.c` 的 [`overlaps`](minix3/minix/kernel/arch/i386/pre_init.c:overlaps)），于是 `i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 把它塞进模块数组的第一个空槽：

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

- `kernbase`/`kernsize` 不来自引导器，是链接脚本导出的常量（`_kern_phys_base`、`_kern_size`；取值见 §3.5 引文——§2.3 只列过虚拟地址侧那组符号）。这里是它们第一次真正被用到的地方：**内核要参与"谁占了哪段内存"的核查，就只能自己报出自己的地址范围**——所以它把自己造成第 N+1 个模块，而不是走一条特例分支。
- `assert(k < MULTIBOOT_MAX_MODS)` 是"内核还要占一个槽"这一事实的形式化：模块数最多只能是 `MULTIBOOT_MAX_MODS - 1`，因为有 1 个槽要留给内核。它与 2.1 里那条"模块数 < `MULTIBOOT_MAX_MODS`"是同一个条件（`k = mbi->mi_mods_count`），作用是在往 `module_list[k]` 写"内核当模块"这一项之前，把"数组还有空槽"这个前提再确认一次。
- 同一个循环里既检查又切除：`mods_with_kernel` 条记录，每条先与其余所有记录比一次，然后从账上切除。切除用的是 `[mod_start, mod_end)`，与 `cut_memmap` 的起止语义一致。

于是"内核与模块占用区"这一整类内存只被一段代码处理，模块与内核之间没有第二套逻辑。

**把 `OVERLAP` 第二项的 `-1` 去掉会怎样**：那一项就变成 `INRANGE(mod1, mod2->mod_end)`。取两个首尾相接的模块：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`。它们不重叠——A 的最后一个字节是 `0x1FFFFF`，B 的第一个字节是 `0x200000`。用不带 `-1` 的写法检查：

```
外层 m = 0（A 是 cmp），内层遍历到 B：OVERLAP(B, A) = INRANGE(B, A.mod_end = 0x200000)
   = (0x200000 >= 0x200000) && (0x200000 < 0x300000) = 真
```

于是 `panic("overlapping boot modules/kernel")` 落在一个完全合法的布局上。这正是"半开区间必须翻译对"这一类错误的代价：报错信息说"模块重叠了"，而真实情况是模块之间恰好相切。再注意上面那句"两个方向各检查一次"在这里的意思：真重叠时两个方向都会报（各自都能发现对方），而相切的误报只需要一个方向成立就够——外层扫到 `m = 0`（把 A 当比较对象）那一次就已经 panic，反方向那一次根本轮不到执行。

顺带说准一件容易搞反的事：**把 `overlaps` 整个去掉，内存账并不会被弄坏**。同一段物理内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作（按 `add_memmap` 与 `cut_memmap` 的取整规则推演：先切 `[0x120000, 0x150000)`、再切 `[0x130000, 0x140000)`，清单仍是那两截）。代价全在内容侧，就是本节开头那句话：两份镜像在重叠地址上互相覆盖，而内核一声不响——所以 `overlaps` 守的是**模块字节不被踩**，不是内存账不被记错。

### 2.6 页表建立步骤与建表之后的下游（借槽窗口与 VM 继承）

#### 2.6.1 建表五步：C 侧逐函数解码

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
| 5 | [`vm_enable_paging`](minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging) | 置 `CR0.PG`（以及 `PSE`/`WP`/`PGE`） | `CR0`/`CR4` 当前值 |

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

- 函数开头的 `assert(cbi->mem_high_phys)` 就是"内存清单先于建表"（§1.2/§2.8）的硬出口：它不为零才走得下去，而反排时该值还是 `.bss` 的零初值。
- `I386_VM_DIR_ENTRIES = 1024`、`I386_BIG_PAGE_SIZE = I386_PAGE_SIZE * I386_VM_PT_ENTRIES = 4096 * 1024`（4MB，定义在 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)）。两个数一乘正好是 4GB——对 32 位来说就是**整个地址空间**。所以这个循环不是在算"该映射哪些"，而是**一次把 1024 项全部填完**：在 4MB 大页下这是最省事的做法，代价固定（只要一个页目录，不需要任何页表页）。

  不要把"省事"当成"必须"。真正必需的覆盖只有两块——当前正在执行的地址与内核要去的地址，它们由 1.2 那两条讲过；全铺是达成它的廉价手段，不是它本身的要求。反例就在本篇：64 位不可能全铺，我们的 boot 恒等映射于是取一个固定上界 `IDENTITY_MAP_END`（唯一权威是 [`minix-arch` 的 `BOOT_IDENTITY_MAP_END`](os/arch/src/lib.rs)，kernel 侧以 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END` 别名引用，值就是低 4GB；见 §4.6）——同一条决策，只是 32 位下这个上界恰好等于整个空间。

  全铺也带来了代价，方向与"更安全"相反：不存在的物理地址也被标成 present（只是额外加上 `PWT|PCD` 变成非缓存），所以越界访问不会触发页错误，而是读到不确定的内容——fast-fail 在这里是被削弱的，不是被提供的（下面几条接着说）。
- 每个条目都置 `I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_USER | I386_VM_WRITE`：存在、是大页（该条目直接指向 4MB 物理块而不是二级页表）、用户可访问、可写。这里的 `I386_VM_USER` 要说清一句：boot 阶段内核在 Ring 0 执行，U/S 位不参与内核自己的访问判定，所以**这一位在 C 里没有实际效果**。有意义的对照是 `pg_mapkernel` **不置**它（内核地址被标成 supervisor-only），而恒等映射也不会留给用户态用——VM 的 `pt_init` 跳过 BIGPAGE 条目、自己新建页目录。新设计里的对应选择（boot 用的恒等映射不置该位）见 §4.6。
- 条件比较的是"内存上界截到 4MB 边界"与"本条目起始地址"：`(mem_high_phys & I386_VM_ADDR_MASK_4MB) <= (phys & I386_VM_ADDR_MASK_4MB)`。注意 `I386_VM_ADDR_MASK_4MB` 的值是 `0xFFC00000`，它是**清掉低 22 位、向下取整到 4MB**（不是向上），且右边的 `phys` 本来就是 4MB 的整数倍、那半边掩码是空操作；真正起作用的是把内存上界抹到边界上。成立时额外加 `I386_VM_PWT | I386_VM_PCD`。源码注释给了理由："We map memory that does not correspond to physical memory as non-cacheable."
- 向下取整在边界上是**保守（多包一圈）**而不是漏包：设内存上界是 `0x2001000`（32MB 加一页），抹到边界得 `0x2000000`，于是第 8 号条目（起点正好 `0x2000000`）满足 `<=` 而被标成非缓存——**尽管它开头那一页是真内存**。反方向取整就会漏：那一段里真正不存在的地址会被当成普通可缓存内存，才是真危险。所以这个方向的取舍是"宁可把一段真内存当设备慢一点访问（结果仍正确），也不把设备或空洞当内存去缓存"。
- `PWT`/`PCD`（页写穿透 / 页禁缓存）的用途：那些地址后面不是 DRAM，而是设备寄存器或空洞。设备读写的语义依赖"每次访问都真的到达设备"；若走缓存，读可能命中缓存里没有被设备更新过的旧值，写可能在写缓冲区里停留。所以"超出物理内存上界"这个条件至少圈出内存上界之上那一整圈（设备寄存器与紧贴上界的高端空洞）——上界之下的空洞与保留区不在此列（它们进账时就被丢弃，见 §2.4）。
- 这个函数把 4GB 全部置为 PRESENT。所以"有没有映射"在这里不构成区分，区分"真实内存"与"设备/空洞"的只有缓存属性两位。

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

- `kern_vir_start`/`kern_phys_start`/`kern_kernlen` 是文件顶部的静态量，初值来自链接脚本导出的 `_kern_vir_base`/`_kern_phys_base`/`_kern_size`（取值见 §3.5 引文；§2.3 只列过虚拟地址侧那组符号）。它们为什么这里要用文件级变量而不是参数：同一个文件里两个函数签名不同——`pg_identity` 收一个 `kinfo_t *`，而 `pg_mapkernel(void)` **不收参数**，它要拿到"内核在哪、多大"就只能读文件级静态量。拿不到参数也要能建表，这件事也印证了同一句话：**建表所需的布局信息不属于引导器给的数据**。
- 两条断言要求两个基址都按 4MB 对齐。索引从 `kern_vir_start / I386_BIG_PAGE_SIZE` 开始：`_kern_vir_base = 0xF0400000`（`minix3/minix/kernel/arch/i386/kernel.lds`），除以 4MB 得下标 `961`。
- 循环把内核镜像按 4MB 一块地映射进高地址区。条目里**没有** `I386_VM_USER`——内核地址对用户态不可访问，这是与恒等映射故意相反的一处。
- 返回值是循环结束后的 `pde`，也就是内核映射占完之后第一个未被占用的页目录下标。注释写作 `free pde`。它的下游见本节末尾。

**把 `pg_identity()` 排到 `pg_mapkernel()` 之后会怎样**：两个函数写同一个 `pagedir[]`，索引集合**重叠**：`pg_identity` 写 0..1023 全部 1024 个条目，`pg_mapkernel` 写从 961 开始的若干条目（`_kern_vir_base = 0xF0400000` 除以 4MB）。先写内核映射、后写恒等映射，961 号及之后的若干条目会被"恒等映射 + 4MB 大页"覆盖：`pagedir[961]` 从 `0x00400000 | …`（`_kern_phys_base`）变成 `0xF0400000 | …`（961 × 4MB），两者相差 `_kern_offset = 0xF0000000`，**不相等**。切换到高地址执行时，内核镜像的第一条指令地址 `0xF0400000` 被 961 号条目解释成物理地址 `0xF0400000`——那是一片没有 DRAM 的物理块，取指立即失败；被覆盖的范围是 961 到 `961 + kern_size/4MB`，即内核镜像自己占据的前几个 4MB 块。次序错在这里不会报"映射冲突"，只会让内核在高地址的第一条指令上取不到码。

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
- **先清，再置**，而且清掉的位里有一个要等到最后才回来：先清 `PG`、清 `PGE|PSE`，随后只重设 `PSE`（4MB 大页成立的前提），再开 `PG`，最后才按 CPU 能力置 `PGE`。**这个先后不能倒**：`pg_identity` 与 `pg_mapkernel` 写的每一条表项都带 `I386_VM_BIGPAGE`（bit 7），而 `PSE = 0` 时这一位是保留位——先开 `PG` 就等于让 CPU 拿着一张每一位都非法的页目录去取指。
- 注释给的两条理由分别是："PGE 必须在 PG 之后启用"与"Our page table contains 4MB entries"（PSE 必须为 1，4MB 大页才成立）。
- `WP`（写保护）在开 `PG` 之后单独置一次。它的效果是"内核态也不能写只读页"，是 W^X 的基础。
- `pgeok` 来自 CPU 能力检测（`_cpufeature(_CPUF_I386_PGE)`）。有能力的 CPU 才置 `PGE`：该位为 1 时，带 G 位的页表项在切换 `CR3` 后仍留在 TLB 里。

> `CR0` 与 `CR4` 是 x86 的控制寄存器，用法都是"读回来、改位、再写回去"。`CR0` 说"机器处在什么模式"，`CR4` 说"启用哪些扩展特性"。上面涉及的四位：`CR0.PE`（bit 0）保护模式，由引导器负责置好、内核只断言它已置；`CR0.PG`（bit 31）分页总开关，置起后每一取指、每一访存都要过页表；`CR4.PSE`（bit 4）大页使能，它允许页目录项直接指向一个 4MB 物理块而不是下一级页表；`CR4.PGE`（bit 7）全局页，配合表项的 `G` 位让映射在切换 `CR3` 时不被冲刷。各位的位值与一处头文件注释错误，见 §2.6.5 的表。

#### 2.6.2 页表页从哪来：静态池与动态取页（主路径的供给）

页表本身也要占物理页，而 4KB 细粒度映射里"被映射的那一页"也可能要现分配——这是两件事，C 里由两个函数分别承担：`i386/pg_utils.c` 的 [`alloc_pagetable`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable) 管**页表页**（静态池），`:pg_alloc_page` 管**被映射页的现取**（4KB 细粒度映射的入口是 `:pg_map`）：

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

`pg_map()` 在 4KB 粒度映射时挂页表：`pagedir[pde] = (ph & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`，随后 `pt[pte] = (source & I386_VM_ADDR_MASK) | PRESENT | USER | WRITE;`。这两行是"两级页表"的实现：页目录项指向页表页（`ph` 来自 `alloc_pagetable`），页表项指向物理页（`source` 是调用方给好的页，或传 `PG_ALLOCATEME` 时由 `pg_alloc_page` 现取），两级都带 USER，因为 4KB 细粒度映射的用途之一是给用户地址空间做准备。

`pg_map` 入口还有两个约束，它们划定了这条路径的适用范围：

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

两种用法各要什么：调用方**已经握着物理页**时，两边必须页内偏移一致（`vaddr % 4096 == phys % 4096`），因为一个页框只能装下偏移相同的那些地址；两个地址随后都向下取整到页边界。调用方**还没有物理页**时，传哨兵 `PG_ALLOCATEME`——它定义在 [`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h)，值为 `((phys_bytes)-1)`，含义是"物理页由你分配"，所以它不是一个合法地址；此时只要求虚拟地址页对齐，每个虚拟页的物理页由 `pg_alloc_page` 在循环里现取（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 装载启动镜像时就是这么用的）。最后那条 `assert(vaddr < kern_vir_start)` 把这条 4KB 路径限制在**低半地址**：高半内核区已由 `pg_mapkernel` 用大页一次建好，真正的用户页表则由 VM 在它自己的地址空间里建——这条路径只是内核要访问某个用户地址范围时先搭的架子。

#### 2.6.3 建表之后的下游：借槽窗口

`pg_mapkernel()` 那个返回值 `freepde_start` 也是这套机制的起点（消费者是 `i386/memory.c` 的 [`memory_init`](minix3/minix/kernel/arch/i386/memory.c:memory_init) 与 `:createpde`）。它的语义是"内核映射占完之后，页目录里第一个空闲的槽位号"，而它的消费者要的是"两个可以借用的窗口"：

```c
/*
 * 代码引用，minix3/minix/kernel/arch/i386/memory.c:memory_init。
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

链路是：内核要访问"另一个地址空间里的地址"（例如跨进程拷贝），而它只有一个当前生效的页目录；办法是在**当前页表**里临时占用一个空闲槽位，把对方那个地址对应的页目录项写进去，于是对方的一个 4MB 窗口在当前地址空间里可见；用完由调用方在下次切换时自然丢弃。`virtual_copy` 里两个方向各借一个槽位（`free_pde_idx` 为 0 与 1），写完调 `reload_cr3()` 刷新 TLB。另一个消费者在 VM 服务里（[`vm/pagetable.c`](minix3/minix/servers/vm/pagetable.c) 里对 `kernel_boot_info.freepde_start` 的取用），用途相同。

这就把 `freepde_start` 从"一个返回值"变成了"一套借用机制"——注意它真正被使用是在 `kmain` 之后（同属下游延伸）：它成立需要三个前提——当前页表里确实有连续的未用槽位、借来的窗口只在当前 `CR3` 下有效、借出者自己记账并保证归还。新设计为什么不再需要它，见 §3.9。

**若 `pg_mapkernel` 返回的是"最后一个已用槽位"而不是"第一个空闲槽位"会怎样**：`memory_init` 的两行自增会把 `freepdes[0]` 设成内核映射占用的最后一个页目录下标（961 + k - 1，`k` 即内核占用的 4MB 块数 = `kern_size`/4MB），`freepdes[1]` 才落到真正的第一个空闲槽。`createpde` 对借来的槽只做两条范围检查（`free_pde_idx` 在 `nfreepdes` 之内、槽号在 1024 之内），不检查它"是否空闲"；而跨空间拷贝的第一个方向固定借 0 号槽（`createpde(srcproc, srclinaddr, &chunk, 0, &changed)` 在 `i386/memory.c` 的 [`lin_lin_copy`](minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy) 里，入口是 `i386/memory.c` 的 [`virtual_copy_f`](minix3/minix/kernel/arch/i386/memory.c:virtual_copy_f)），`vm_memset` 也借 0 号槽（`i386/memory.c` 的 [`vm_memset`](minix3/minix/kernel/arch/i386/memory.c:vm_memset)）。于是第一次跨地址空间拷贝会把对方地址空间的 PDE 值写进 `p_cr3_v[961 + k - 1]`。这里要说清 `p_cr3_v` 是哪张表：`i386/protect.c` 的 [`arch_post_init`](minix3/minix/kernel/arch/i386/protect.c:arch_post_init) 用 `i386/pg_utils.c` 的 [`pg_info`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_info) 把引导期那张 `pagedir[]` 的物理地址与虚拟指针交给 VM 进程当作它的页表，`lin_lin_copy` 又断言当前 `CR3` 就是它（`i386/memory.c` 的 [`lin_lin_copy`](minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy) 里 `assert(read_cr3() == get_cpulocal_var(ptproc)->p_seg.p_cr3)`）——所以这一改写动的正是**内核此刻正在用的那张表**，不是某个进程的私有页表。而 `961 + k - 1` 那一条正是内核镜像最后一块 4MB 的映射；`reload_cr3()` 让改写立刻生效，内核在那个 4MB 窗口里的下一次访问就撞上它：对方该槽的条目若不存在（值为 0 或非 present），是一次内核态页错误；若存在，则是静默读到另一个进程的 4MB 内容（内核数据被换成了别人的字节，没有任何报错）。两条路径都经系统任务进入：跨空间拷贝走 SYS_COPY 与 SYS_SAFE_COPY（[`system/do_copy.c`](minix3/minix/kernel/system/do_copy.c)、[`system/do_safecopy.c`](minix3/minix/kernel/system/do_safecopy.c)），清零走 SYS_MEMSET 与 SYS_SAFE_MEMSET（[`system/do_memset.c`](minix3/minix/kernel/system/do_memset.c)、[`system/do_safememset.c`](minix3/minix/kernel/system/do_safememset.c)）——发起方可以是任何服务进程，不限于装载启动镜像那一步。所以最早坏在 `memory_init` 之后第一次跨空间拷贝或清零上。

#### 2.6.4 自举页表的下游：VM 接手时的继承规则

这张自举页表有两个下游。第一个是上面讲完的 `freepde_start` 借用窗口——内核自己还要用这张表的高半槽位；**第二个是本节的对象：页目录被交给 VM 的进程表项。** 这一步同样越过 `pre_init()` 返回之后（下游延伸）——放在这里，是为了把这张表的最终归属一次交代清楚。这个交接口（`pg_info` 的两个输出参数、`arch_post_init` 的调用点）在 §2.6 的反向推演里已经见过，下面只看它写下的代码：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:arch_post_init。 */
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
/* 代码引用，minix3/minix/servers/vm/pagetable.c:pt_init 的复制循环。 */
		/* BIGPAGEs are kernel mapping (do ourselves) or boot
		 * identity mapping (don't want).
		 */
		if(!(entry & ARCH_VM_PDE_PRESENT)) continue;
		if((entry & ARCH_VM_BIGPAGE)) continue;
```

两句注释把两件事分开：内核映射由 VM 自己重建；引导期的恒等映射是"不要的"。也就是说 §2.6 建起来的那张页目录里，只有非大页条目会被 VM 继承下来——4KB 细粒度映射搭的架子留下，4MB 大页恒等映射在 VM 接手那一刻被丢弃。VM 侧页表的完整结构见 `rewrite-notes/02-stage-vm/07-pagetable-struct.md`。

#### 2.6.5 本篇用到的位、掩码与常量（参考表）

下面两张表把本篇出现的位与掩码集中在一处，值全部取自 [`include/vm.h`](minix3/minix/include/arch/i386/include/vm.h)。

| 常量 | 值 | 含义 | 本篇哪一步用到 |
|---|---|---|---|
| `I386_VM_PRESENT` | `0x001` | 条目存在 | `pg_identity`、`pg_mapkernel`、`pg_map` 写入的每个条目 |
| `I386_VM_WRITE` | `0x002` | 可写 | 同上 |
| `I386_VM_READ` | `0x000` | 只读（不置 `WRITE` 即只读） | — |
| `I386_VM_USER` | `0x004` | 用户态可访问 | `pg_identity`、`pg_map`（`pg_mapkernel` 不置，理由见 §2.6 前半 `pg_identity` 的解码条目） |
| `I386_VM_PWT` | `0x008` | 写穿透 | `pg_identity` 对非 DRAM 区间 |
| `I386_VM_PCD` | `0x010` | 禁缓存 | 同上 |
| `I386_VM_ACC` | `0x020` | 已访问（由 CPU 置位） | 硬件写，本路径不置 |
| `I386_VM_DIRTY` | `1 << 6` | 已写（由 CPU 置位） | 同上 |
| `I386_VM_BIGPAGE`（同值写法 `I386_VM_PS`） | `0x080` | 本条目直接指向 4MB 物理块 | `pg_identity`、`pg_mapkernel` |
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

**本节结算**：五步走完，机器从"只有一个物理地址空间"变成"页表已就位、内核随时可以站到 0xF04xxxxx 上执行"——低半那 1024 项保证开启分页后的下一条取指不会跑飞，高半那几项保证跳过去之后代码与栈都可用，`freepde_start` 则把"还能借哪些槽位"留给后面的跨空间拷贝（借用机制的展开就在本节前半；64 位侧为什么不再需要它，见 §3.9）。至于换栈与那次跳转本身，归属见 2.2 末。

### 2.7 同一个函数名在 ARM32 上：自己造一份 multiboot

Minix3 同时支持 x86 与 ARM 两种架构，同一段引导准备在两侧各有一份实现、用的是同一串函数名。先看两侧的入口（`earm/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/earm/head.S:multiboot_init) 与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init)）：

```asm
/*
 * 代码引用，minix3/minix/kernel/arch/earm/head.S（i386 侧同一段已在 §2.2 给出，
 * 这里不再重复）。
 */
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

真正体现"引导协议不属于内核"的地方在 `pre_init` 内部（ARM 侧是 `earm/pre_init.c` 的 [`setup_mbi`](minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi)，两侧的 `get_parameters` 分别在 `earm/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/earm/pre_init.c:get_parameters) 与 `i386/pre_init.c` 的 [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters)）。ARM32 的 `get_parameters` 第一件事不是读引导器结构体，而是**造一个**：

```c
/*
 * 代码引用，minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi。
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
- 模块列表是**编译期常量**：`MB_MODS_BASE + i * MB_MODS_ALIGN`。这意味着 ARM 这一侧假设了引导器（U-Boot 一类）已经把模块放在这些固定地址上，`mod_start`/`mod_end` 只是把这个约定抄进结构体。
- 内存表只有一条：`mb_memmap` 一个条目，`mm_base_addr = MB_MMAP_START`、`mm_length = MB_MMAP_SIZE`。x86 上这张表可能有十几条（BIOS 报告的内存空洞），ARM 上是一条硬编码的连续区间。
- `mbi->cmdline = (u32_t) bootargs;`：命令行来自引导器通过寄存器传进来的参数（ARM 调用约定下前两个参数在 `r0`/`r1`，`bl pre_init` 把它们原样交给 `argc`/`argv`），注释写作"morph the bootargs into multiboot"。这句话是整个函数的性质说明——它把"另一种引导器给的参数形式"翻译成 Multiboot 的形状。
- 于是 `get_parameters()` 里其余部分（模块表 `memcpy`、`add_memmap` 遍历、`overlaps`、`cut_memmap`、`mb_set_param`）在两种架构上同构（注意 ARM 侧头文件用的是另一套字段名），调用次序一致；但**区间约定不同**：ARM 的 `overlaps` 用的是闭区间（`INRANGE` 写成 `(v) >= mod->mod_start && (v) <= thismod->mod_end`，`OVERLAP` 第二项直接取 `mod2->mod_end`），正是 §2.5 论证为错误的那一版——不过 ARM 的槽距是 8MB、每个 `mod_end` 又各留了一页（`earm/pre_init.c` 的 [`MB_MODS_ALIGN`](minix3/minix/kernel/arch/earm/pre_init.c:MB_MODS_ALIGN) = `0x00800000`，减去 `ARM_PAGE_SIZE`），相切不会出现，所以这一版在它的固定槽位布局下够用；换成会相切的布局就会误报。ARM 这一侧甚至保留了 `multiboot` 这个词。

撇开两种架构的硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 只 `bl pre_init`（两侧形参不同：ARM 是 `pre_init(argc, argv)`，没有 `magic` 也没有 `ebx`），再由 `setup_mbi` 自造结构 | 引导协议（各架构不同） |
| 2 | 建立地址映射 | [`pg_clear`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear) + `:pg_identity` + `:pg_mapkernel` | 同名函数（[`pg_identity`](minix3/minix/kernel/arch/earm/pg_utils.c:pg_identity)），1MB Section 代替 4MB 大页 | 分页硬件（各架构不同，语义相同） |
| 3 | 开启分页/MMU | [`pg_load`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_load) + `:vm_enable_paging`（写 `CR3`，置 `CR0.PG`） | 同名函数（[`pg_load`](minix3/minix/kernel/arch/earm/pg_utils.c:pg_load) 写 `TTBR0`；[`vm_enable_paging`](minix3/minix/kernel/arch/earm/pg_utils.c:vm_enable_paging) 置 `SCTLR.M` 并使能 I/D cache 与分支预测） | MMU 寄存器（各架构不同） |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定（x86 用栈、ARM 用寄存器） |

ARM 侧这两步比 x86 多几个前置：`vm_enable_paging` 先写 `TTBCR`（页表基址控制）与 `DACR`（把 16 个域全设为 Client），再读改写 `SCTLR`；开 MMU 之前还有一次 `dcache_clean()`，调用点在 `earm/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/earm/pre_init.c:pre_init)。

步骤 2、3 在各架构都调用同名函数，是因为"清零页目录 → 建恒等映射 → 把内核映射到高地址 → 装载页表基址 → 开启分页"这一串的**语义**与"用哪条指令、页表项多少位"无关；第 1 步是唯一必须按架构另写的一步，因为"参数从哪来"由引导器决定。第 4 步的差别只在调用约定。

### 2.8 本章归纳

把 2.1 到 2.7 的调用关系收成一张图：

```
引导器（boot2 / GRUB / U-Boot / 固件）
  │  1. 识别头 + 寄存器约定 + 填好的结构体
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
  │     ├── 内核当第 N+1 个模块 → overlaps() × (N+1) → panic 或继续
  │     └── cut_memmap() × (N+1)                   （切掉内核与模块占用区）
  ├── pg_clear()                （页目录清零）
  ├── pg_identity(&kinfo)       （低地址恒等映射，覆盖当前执行点）
  ├── freepde_start = pg_mapkernel()（内核高地址映射，返回第一个空闲槽位号）
  ├── pg_load()                 （CR3 = 页目录物理地址）
  └── vm_enable_paging()        （CR0.PG 置位，PSE/WP/PGE 按序处理）
  ▼
return &kinfo
```

三条次序结论，各配一条源码依据（这三条是**本章能在 C 源码里定位证据**的次序；"页表先于开启分页"与"内核位置先于清单裁剪"的根据在 §1.2/§2.6，这里不重复）：

- **内存清单先于建表**。清单在 `get_parameters()` 里就建完并切完（`add_memmap` × N → `overlaps` → `cut_memmap` × (N+1)），`pg_clear()`、`pg_identity()`、`pg_mapkernel()` 都在它之后才跑（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 里这几句的排列顺序）。**为什么不能反，按依赖的实际层次说**：`pg_identity()` 要用 `mem_high_phys` 决定哪些条目该标成非缓存——这是硬依赖，且它在函数开头就有一条 `assert(cbi->mem_high_phys)`：反排时该值还是 `.bss` 的零初值，断言当场失败（断言拿掉才是"得不出正确的表"）；`pg_mapkernel()` 的初始大页映射不依赖记录（它用链接期常量与已清零的静态页目录）；从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度路径（`pg_alloc_page`），它同样要求记录已就位。
- **恒等映射先于内核映射**。两个函数写同一张 `pagedir[]`；`pg_identity` 覆盖全部 1024 个条目，`pg_mapkernel` 从下标 961 开始写（依据：`i386/pg_utils.c` 的 [`pg_identity`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity) 的循环上界 `I386_VM_DIR_ENTRIES` 与 `i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel) 的起始下标计算）。`pre_init` 里那句注释说的正是这件事：`/* … but first a 1:1 mapping so this code stays where it should be */`——先铺恒等映射，好让当前这段代码在分页开启后继续留在原地。
- **信息结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`，`head.S` 把它压栈作为 `kmain` 的第一个参数（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `return &kinfo;`，与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 里 `call kmain` 之前那条把返回值压栈的 `push %eax`——它在 §2.2 引文块之外；引文里那条 `push %eax` 是给 `pre_init` 传参用的，同名不同处）。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`，它落在高半一侧。本篇不复制这条判据——它属于"内核怎么走到高半去跑"那个主题，推证与证据表在 `02-higher-half-kernel.md` §2.4（§1.4 已给过那条"名字不是判据"的提醒，这里不重复）。本篇只需要这一个事实：`pre_init` 返回后换上的那个栈，与紧接着要跳去的高半在同一侧，所以换栈与跳转能成对。

把本章的断言按用途分一下：`assert(!(cbi->bootstrap_start % I386_PAGE_SIZE))` 这类断言不能证明布局一定合法，它把"不合法"从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。

---

## 3. Rust 设计决策

本章与第 2 章逐项对应：3.1–3.2 换掉引导协议与它的两个接口（对应引导器契约），3.3 固定 crate 依赖方向，3.4–3.7 是交接面本身（结构、段语义、页尺寸能力、三种形态），3.8–3.9 是内存账与页表页供给的换手，3.10 收成一张 C→Rust 对照表。

### 3.1 引导协议换代：Multiboot 与 GRUB 的角色由固件接口替换 `[ARCH: 引导协议从 Multiboot 换成固件接口]`

C 的实现前提写在 2.1 与 2.2 里（本节标题里的 GRUB 泛指"第三方引导器"这个角色，本仓这条路径上具体是 §2.1 点名的 boot2）：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与一个内存分配接口（`AllocatePages`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot 协议 + MINIX boot2） | Rust（UEFI 路径） | Rust（OpenSBI 路径） |
|---|---|---|---|
| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（标准可执行格式） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 调 `GetMemoryMap` 一次，产出两份列表（按固件的内存类型标签分流：`CONVENTIONAL` 是常规可用内存，其余类型交付内核时必须排除） | UEFI/OpenSBI 两腿分别走 `UefiMemoryMap` / `DtbMemoryMap`，三链共享 `MemoryMapSource` trait（minix-boot，I-22） |
| 传递参数 | 寄存器 + `multiboot_info_t` | `KernelInfo`（按值）+ 根页表物理地址 | 同 UEFI |
| 退出固件服务 | 无此步骤（引导器早已离开） | 必须调 `ExitBootServices`：boot-shim 跑在 Boot Services 的**环境之内**，得在某个时刻主动声明"我不再需要你"；声明之后整套服务失效，本篇用到三件——读文件（`SimpleFileSystem`）、分配内存（`allocate_pool`）、固件控制台输出（`uefi::println!`） | 无此步骤（U-Boot 已把控制权交出去） |

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
// 代码引用，os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services。
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

> `LOADER_DATA` 是 UEFI 内存类型标签，意思是"引导程序向固件分配器租下的内存"（loader 是属格——引导程序的所有物，不是"被加载的数据"——这里只给这次取内存图的缓冲区定标签）；返回的 `_mmap` 可以丢弃，因为内核要的内存清单已由上文 `build_memmaps()` 的两份列表交付，这只是规范要求的"EBS 前最后一次取图"，取完即弃。

被审视的替代路径与结论（三否一采）：

| 方案 | 结论与理由（可复核） |
|---|---|
| 保留 Multiboot，做 64 位扩展（Multiboot2） | 否决：两个新架构没有等价协议；`head.S` 那套"引导器填结构体 + 寄存器传参"在 aarch64/riscv64 上要各写一份，等于三份引导协议 |
| 各架构独立引导（x86 用 Multiboot、ARM 用 PSCI、RISC-V 用 SBI） | 否决：引导准备的代码要按架构分叉，而 2.7 展示的正是"这一步的语义与硬件无关"——分叉会把它重新绑回硬件 |
| 内核自带 stub（UEFI 形态：把 boot-shim 链接进内核镜像，内核自己读文件再自举） | 否决，理由是**依赖方向**：boot-shim 与内核并成一个二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与 §3.3「内核不依赖引导层」的单向边界冲突——屏蔽固件差异正是 boot-shim 的职责。riscv64 没有 Rust 支持的 UEFI 目标，但只挡住 UEFI 这一种实现；换成 opensbi 形态则改判为采纳（下一行） |
| 内核自带 stub（opensbi 形态：固件直接跳下一级镜像，引导面长在内核镜像里） | **采纳**：它就是 §3.7 的直核自引导形态（riscv64 真板 `FW_PAYLOAD`、QEMU `-kernel`；aarch64 `-kernel` 测试通道）。它不违反依赖方向——`kernel-image` 反向依赖的是 boot-shim 的**库函数**（取件表、memmap 源），不是引导器产物，`minix-kernel` 与 `kernel-image` 都不依赖 `uefi`（§3.3 问题二）。定位是辅助启动路径，不作三架构主线 |

> **现状标注**：上面两张表描述的是架构终态；x86_64 当前产线仍是 boot-shim 内联内核（引导器与内核同一镜像）。当前产线形态与终态的差距、以及收口项（NK1/C-29）见 §3.4。

> **例外条款的背景**：RISC-V 的固件形态规范（OpenSBI 的 `FW_PAYLOAD` 模式）允许把下一级镜像打包进固件本体，boot 时固件直接跳进 S-mode 目标——真板上这类极简装机（省掉 U-Boot 与文件系统）不需要引导器存在，QEMU 的 `-kernel` 模拟的正是这个时序。所以"引导面长在内核镜像里"是硬件规范支持的真实形态。§3.1 分工表里的 OpenSBI 列描述的是 U-Boot shim 腿（`OpenSbiBootShim::prepare_boot`）——它目前没有生产调用方，是 D-73 的接线主体；riscv64 当前生产链走直核自引导。

### 3.2 一个 trait 两种固件：`BootShim`

C 里"同一串函数名、两套实现"的机制见 2.7：两种架构各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

Rust 侧把这件事显式化：固件相关的部分收进一个只有**一个方法**的 trait（`boot_shim.rs` 的 [`trait BootShim`](os/libs/minix-boot/src/boot_shim.rs)），其余全部共享。

```rust
// 代码引用，os/libs/minix-boot/src/boot_shim.rs:trait BootShim 与 struct BootPrepareResult。
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

差异被压到一个更小的接口上（`loader.rs` 的 [`trait FileLoader`](os/boot-shim/src/loader.rs)）：两条路径都要拿到"文件字节"，唯一不同的是字节从哪来。

```rust
// 代码引用（省略号处为 trait 文档注释），os/boot-shim/src/loader.rs:trait FileLoader。
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

trait 相对"纯 feature gate + 散落条件编译"的收益，只列能给出代码依据的四条：

- **入口与形状统一**：入口只有一个——[`trait BootShim`](os/libs/minix-boot/src/boot_shim.rs) 的 `prepare_boot(bump_pages)`，返回也统一为 `BootPrepareResult`。于是交接与自举不必为两种固件各写一套，新增实现照同一个入口名与签名写（这些类型住在第三方 `minix-boot`，为什么必须，§3.3 讲）。
- **可测试性**：`FileLoader` 同样是 trait——[`loader.rs`](os/boot-shim/src/loader.rs) 的 tests 里有内存表实现 `MockLoader`（`impl FileLoader for MockLoader`），涉及装载的单元测试都经由它喂文件字节。
- **条件编译的落点收敛**：feature 相关的 `cfg` 只出现在 `lib.rs` 的模块与再导出声明处（口径：`grep -c 'cfg(feature' os/boot-shim/src/lib.rs` 三处，另有 `cfg(all(feature = "opensbi", not(feature = "uefi")))` 一处，合计四条），业务逻辑里没有条件编译。
- **换固件不动内核**：新增一种固件形态，改动是 boot-shim 侧的一个新 `impl BootShim for …`，`kernel.elf` 不必重新构建（两者分属 `os/kernel-image` 与 `os/boot-shim` 两个产物，这条分界的后果在 §3.7 与 §4.8）。边界要说准：boot-shim 屏蔽的是**固件差异**，板级差异仍落到内核侧的平台发现（[`boot.rs`](os/arch/src/arch/boot.rs)），不能把它说成"新板子只需写 boot-shim"。

这里故意不列"类型安全"与"封装"——这两条常被记在 trait 头上，实际来自 feature 门控：`uefi_helpers` 与 `opensbi_helpers` 各由 [`lib.rs`](os/boot-shim/src/lib.rs) 的 `#[cfg(feature = ...)]` 门控，开启哪个 feature 就只有哪个固件类型存在，**写错固件名本来就编译不过**，trait 没有为此多提供什么。调用点也没有借 trait 做抽象：[`fn main`](os/boot-shim/src/main.rs:fn main) 按具体类型写——`UefiBootShim::prepare_boot(1024)`。这不是把固件写死在产品里：boot-shim 这个可执行文件只在 UEFI 形态下构建（bin 的 `required-features = ["fw-uefi-image"]` 隐含 `uefi`），固件选择发生在"构建哪个二进制"，不是运行期分派；opensbi 形态的生产路径是 `kernel-image` 自引导面（§3.7），复用 opensbi_helpers 的库函数、不经这份 main.rs。

**opensbi 腿的两种角色要分清**：`OpenSbiBootShim::prepare_boot` 在仓内没有调用方——boot-shim 可执行体只在 UEFI 形态构建（bin 的 `required-features = ["fw-uefi-image"]`），riscv64 的生产装机面是 `kernel-image` 自引导面（§3.7 的直核形态），它复用 opensbi_helpers 的**库函数**（`BootFileTable`、`bump_alloc_pages`、`build_kernel_info`）而不经 `prepare_boot`。作为 `BootShim` 的第二个实现，它保留为独立 shim 腿的接线主体，待 D-73 的入口契约闭合后接线。I-22 丁案 F 条裁决不合并引导形态层——`BootShim` 独立可执行体与 bootface 自举对应不同机器状态，本 trait 只承担前者的固件差异。

### 3.3 crate 依赖方向：boot-shim 依赖内核，内核不依赖 boot-shim

C 的引导链同样不止一个二进制：BIOS 场景有 bootblock 与 boot2 两级引导器（[`boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c)），GRUB 场景由 multiboot 引导器代劳——引导器与内核不是链接成一个二进制，而是各自链接出独立的可执行文件，运行期只隔一条 multiboot 协议（寄存器递 `mbi` 指针）。内核侧的引导准备 [`head.S`](minix3/minix/kernel/arch/i386/head.S) 与 [`pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 则编在内核镜像之内，`pre_init` 填的 `kinfo` 本来就是内核自己的全局变量。所以 C 无需回答"引导层与内核谁依赖谁"：引导器与内核的边界由协议承载，协议两侧没有编译期关系。

Rust 侧的问题形态不同：boot-shim 独立成产物，却要与内核共享类型定义（`KernelInfo` 与交接的返回形状），这份复用由 crate 依赖表达——"谁依赖谁"第一次成为必须显式回答的构建问题，且答案必须单向：

```
boot-shim ──┬─→ minix-boot   （BootShim trait、KernelInfo、BootPrepareResult）
            ├─→ minix-arch   （pt_alloc 注册接口、Paging trait）
            └─→ minix-kernel （arch_boot / enter_kernel_image_v1）
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
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，[`early_console.rs`](os/plat/src/early_console.rs)），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`。Cargo 的 `[target.'cfg(target_arch = "…")'.dependencies]` 语法允许一条依赖只在指定 target 架构下生效，kernel-image 用它给 riscv64 与 aarch64 各加了两条：`boot-shim`（只开 `opensbi` feature，`uefi` 不开）与 `fdt`（解 DTB 用）——只有以这两个架构为 target 构建时它们才生效。
  - 符合本项目设计目标的方式（独立引导器 + 内核本体两个产物）里，"内核不依赖 boot-shim"对 `minix-kernel` 与 `boot-shim` 成立——引导器依赖内核、内核不回头依赖引导器。这一分工描述的是架构终态；x86_64 当前产线仍是内联单镜像，当前产线形态与终态的差距见 §3.4。
  - `kernel-image` 的情况不同：它是自引导镜像（引导面 `bootface` 长在自己体内，§3.1 例外条款），riscv64/aarch64 目标下按上面那两条 target 表反向依赖 boot-shim 的**库**（取件表、memmap 源等函数）——依赖的是库而不是引导器产物，单向边界不因此破例（道理见问题二）。这张 target 表就是终态：I-22 把 memmap 源下沉到 `minix-boot` 之后，反向依赖的仍是库、不是引导器产物，依赖方向没有变化。

这条单向边界回答的是"共享类型住哪"，与"哪种固件能编译"是两个独立的问题，别混在一起：

**问题一：共享类型为什么住 `minix-boot`**。`KernelInfo`、`BootShim`、`BootPrepareResult` 要被 boot-shim 与内核两侧引用。若定义住在引导层，内核引用它就得依赖 boot-shim——反向依赖、且把固件实现拖进内核依赖面；若住在内核里，boot-shim 又得依赖内核，同样成环。所以这层定义下沉到只有类型与 trait、没有任何实现的 `minix-boot`（依赖表只有 `minix-types`），谁依赖它都不会被拖进固件代码。这条边界不是风格洁癖：`os/kernel/Cargo.toml` 的依赖表里没有 boot-shim、也没有 uefi，用上面那条 `grep` 就能一次验证。

**问题二：riscv64/aarch64 上的引导面为什么长成"内核镜像自举"**。背景见 §3.1 表后的引用段：RISC-V 与 ARM 的 `-kernel` 极简装机（QEMU `-kernel` 模拟的是真实硬件的这种装载方式）下，下一级镜像直接进 RAM、固件跳过去，固件与内核之间没有"装载与被装载"的分界，独立 boot-shim 没有存在位置。于是两个架构的引导准备各自由 `kernel-image` 自带的引导面承担——riscv64 是 `bootface`（[`bootface.rs`](os/kernel-image/src/bootface.rs)），aarch64 是 `bootface_a64`（[`bootface_a64.rs`](os/kernel-image/src/bootface_a64.rs)），流程逐段同构（DTB 解内存 → 取件表 → bump 池 → 模块 → `KernelInfo` → 同一个 `arch_boot`）。两者都复用 boot-shim 的**库函数**，但不是独立引导器产物；这一形态的固件对话走 DTB 与 PSCI/DMB 一类的裸协议，`kernel-image` 与 `minix-kernel` 两个 crate 都不依赖 `uefi`，单向边界原样成立。它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库与 `arch_boot` 收敛点——统一发生在层内（类型、取件库、收敛函数），引导形态层不造伪统一（I-22 丁案 F 条裁决，三形态对照见 §3.7）。

**如果让 `minix-kernel` 直接依赖 boot-shim 会怎样**（动机多半是"想复用 `BootPrepareResult` 的定义"）：复用类型走 `minix-boot` 即可，不必依赖引导层（问题一已答）。剩下的否决理由是本节标题本身：**内核不该认识固件形态**——屏蔽固件差异正是 boot-shim（或 bootface）存在的意义。"riscv64 编译不过"（UEFI 目标只有 x86_64/aarch64/i686，riscv64 暂不支持）不构成独立论据，换 opensbi feature 就能过；否决理由从头到尾只有依赖方向一条。

### 3.4 从全局 `kinfo` 到按值交接的 `KernelInfo`

C 里 `kinfo` 是一个全局变量。`pre_init.c` 开头就定义 `kinfo_t kinfo;`，`get_parameters(ebx, &kinfo)` 往里填，`pg_utils.c` 里的记账函数通过 `cbi` 形参收到它的地址，`pre_init` 返回 `&kinfo`，`kmain` 再把这个指针传下去。整条链上传递的是**地址**，而地址之所以有效，是因为所有代码都在同一个地址空间里、链接成同一个镜像。

Rust 侧把这份共享状态改成一段**显式交接**，分两个维度说：**通道形态**随交接形态变。x86_64 当前由 boot-shim 在链接期把内核代码并进自己的 PE 镜像，交接是同一个地址空间里的普通函数调用（`arch_boot(&KernelInfo, …)`）——注意这是**过渡形态**：x86_64 的架构终态与 aarch64 同型（独立内核镜像 + 跳转交接，`kernel-image` 的 x86 布局产出与布局校验已就位），跳转协议已由 OQ-N6 裁决（每进程独立根原语，aarch64 的 `BootHandoffV1` arena 交接即其实现），x86 腿的实施收口 = NK1（C-29）待认领（§4.8 "交付边界"），内联只是实施前的产线形态，不是 x86 的既定设计。aarch64 走跨镜像跳转，交接是一个物理地址（arena 基址经 `x0` 递入，见 §3.7）；riscv64/aarch64 的直核自引导没有独立 shim，引导面在同一镜像内直接调 `arch_boot`，通道与 x86_64 内联同类（也是引用），但其成因是硬件形态（§3.3 问题二），与 x86 的"实施未收口"不同源。**载荷内容**三形态一致：必须按值深拷进接收方的私有存储，不能靠指针跨边界存活。理由分档：跨镜像档里两个分别编译的二进制各有自己的 `.bss`，指针在接收方指向完全无关的内存，连 `&'static KernelInfo` 也只保证生命周期、不保证页在对方页表里被映射；同镜像档里 x86_64 的切片指向固件池页，而这些页在启动过程中会被读坏（真机读数见本节末"不做深拷…会怎样"；机制归因见本节末的 N-5 探针读数——池页可读性不是原因，剩余解释是深拷之前的入口引用链）——所以哪怕同空间也要先深拷；直核没有固件池、本可豁免这条具体故障，但同一套落地区把三形态收成一条路径（少一套特例）。值里的切片还要能被接收方解释：v1 协议用固定分区与 `byte_len` 描述符取代裸 `(ptr, len)`，接收方按分区偏移与元素类型重建切片（本节末的边界说明）。

> **TODO（文档-代码张力登记）**：本段揭示 §3.4 通道叙述与代码实况的缝隙——x86_64 内联是"NK1 实施收口前的过渡形态"，但 §3.1 否决表、§3.3 单向边界的叙述均以架构终态为口径。收敛动作有两个候选：① §3.1/§3.3 在立边界处保留一句"当前产线形态与终态的差距见 §3.4"的指针；② 等 NK1 实施收口后，x86 切到独立镜像交接，本段"过渡形态"括注随之删除。处理时需全文排查其他章节是否存在同类"终态叙述无现状标注"的缝隙。

```rust
// 代码引用（略去文档注释），os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo。
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

`kernel_info.rs` 的 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 共 14 个字段。按与 C 的关系分成下面几类：

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap`<br>`user_sp`<br>`bootstrap_start`<br>`bootstrap_len` | `memmap` 从 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带 |
| 改型（C 有、形状变） | `param_buf`：`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验<br>`module_list[]` → `boot_modules: &'static [BootModule]`<br>`mmap_size`：未映射为字段，仅并入切片长度 | |
| 改名 | `vir_kern_start` → `kern_virt_base`<br>`freepde_start` → `free_upper_idx`（类型变 `Option<usize>`，见 §3.9） | |
| 升为字段 | `kern_phys_base`<br>`kern_size` | C 里这两个值不是 `kinfo_t` 的字段，而是链接脚本导出的符号 `_kern_phys_base`、`_kern_size`（取值见 §3.5 引文；§2.3 未列过这两个）。Rust 把它们放进交接结构，来源改为 boot-shim 解析内核 ELF 的 PT_LOAD 段（§4.4 的字段表）。`kern_size` 用 `u64` 而不是 `usize`，避免 32 位目标上的截断 |
| 删除 | 17 个字段：<br>`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`<br>`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`<br>`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`<br>`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic` | 逐个的去向见下表 |
| 新增 | `reserved_regions`<br>`kern_stack_top`<br>`syscall_entry`<br>`platform_sources` | 各自来源见下 |

**"升为字段"行的一个设计细节：为什么三个布局数字由 shim 从 ELF 算出，而不是由调用者传入**。设想把接口改成 `prepare_boot(kern_virt_base, kern_phys_base, kern_size, bump_pages)`。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改成调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，"把内核映射到高地址"那一步会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。让 shim 自己解析镜像，"数字与镜像一致"就不再是调用者的责任，而是唯一数据来源（镜像本身）的自然结果。

这 17 个字段的删除遵循同一个整体逻辑：删掉的不是信息，而是“由交接结构捎带”这个承载方式。C 的 `kinfo` 是全局变量，编译期常量、内核自有状态、记账表都顺手堆在里面；Rust 的 `KernelInfo` 收窄为纯交接结构，只装引导侧必须告知、接收方无法自行导出的内容。于是职责归位成三类：**上浮为常量**——编译期已知的布局与规模事实（`user_end`、`nr_procs`/`nr_tasks`、`release[]`/`version[]`）；**移交专门设施或记录结构**——内核自有的诊断与记账状态（`kmessages`、`do_serial_debug`/`serial_debug_baud`、`minix_panicing`、三个以 `_bytes` 结尾的记账字段）；**换表达不换信息**——内容改由 `KernelInfo` 现存字段承载或推导（`mbi` 的内容散布进各字段，`mods_with_kernel`/`kern_mod` 由 `kern_phys_base`/`kern_size` 直接表达，`boot_procs[]` 从 `boot_modules` 现算）。唯一找不到新读者的是 `mem_high_phys`：恒等映射在新架构里只短暂存在、帧分配直接遍历 memmap，这个“最高物理地址”标量随之退役。

被删的 17 个字段逐个的去向（"接管者"一栏里给的都是本篇可核的落点）：

| C 字段 | 接管者 / 去哪了 | 依据 |
|---|---|---|
| `mbi` | 不进内核：UEFI/SBI 没有 Multiboot 结构体，交接信息由 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 另行承载 | - |
| `mem_high_phys` | 删除。C 里它是"最高物理地址"标量，有真实读者：恒等映射铺满整个 4GB 目录，靠它把 RAM 顶以上没有内存的段标成禁缓存；PCI 驱动另拿它找 RAM 之上的 MMIO 空洞。Rust 的恒等映射只在 bootstrap 内短暂存在、随后整根换掉（§4.6），帧分配器直接遍历 memmap 区域列表——这个标量在 Rust 里没有读者，维护它只是写后即弃的状态 | [`memmap.rs`](os/kernel/src/memmap.rs) 的模块注释（Known Gaps 段）；C 读者：[`pg_identity()`](minix3/minix/kernel/arch/i386/pg_utils.c)、[`complete_bars()`](minix3/minix/drivers/bus/pci/pci.c) |
| `mods_with_kernel` | 删除：C 里它只为一个用途——把内核临时登记成第 N+1 个模块，让重叠检查与 memmap 扣除一并处理内核自身（[`pre_init()`](minix3/minix/kernel/arch/i386/pre_init.c) 的登记段）。Rust 里内核布局由 `kern_phys_base`/`kern_size` 单独表达，扣除与检查各自直接用区间，不需要这个计数 | - |
| `kern_mod` | 删除：C 里它是上面那个"临时第 N+1 个模块"的下标，随 `mods_with_kernel` 一起消失 | - |
| `kmessages` | 独立的内核消息/诊断设施 | [`kmess.rs`](os/kernel/src/kmess.rs)（`KernelConsole`） |
| `do_serial_debug` | 早期控制台初始化自己决定串口 | [`early_console.rs`](os/plat/src/early_console.rs) |
| `serial_debug_baud` | 同上（波特率由平台实现持有） | 同上 |
| `minix_panicing` | panic 路径自己维护状态 | [`fn panic`](os/boot-shim/src/main.rs:fn panic)、[`fn register_panic_diagnostic`](os/kernel/src/lib.rs:fn register_panic_diagnostic) |
| `user_end` | 由 `user_sp` 与地址空间布局常量推导 | [`const USER_STACK_TOP`](os/libs/minix-types/src/types/boot.rs:const USER_STACK_TOP) |
| `boot_procs[]` | 内核侧从 `boot_modules` 构造 boot image 表 | [`fn build_vm_handoff`](os/kernel/src/vm_handoff.rs:fn build_vm_handoff) |
| `nr_procs` / `nr_tasks` | 常量与表长度 | [`const NR_PROCS`](os/kernel/src/proc_table.rs:const NR_PROCS)、`:const NR_TASKS`、[`const NR_BOOT_MODULES`](os/libs/minix-boot/src/kernel_info.rs:const NR_BOOT_MODULES) |
| `release[]` / `version[]` | 内核构建身份，来源为 `minix-types` 里与 config.h 对位的固定字符串常量（`OS_RELEASE`/`OS_VERSION`） | [`misc.rs`](os/kernel/src/misc.rs) 的 GET_KINFO 段、[`kerninfo.rs`](os/libs/minix-types/src/types/kerninfo.rs) |
| `vm_allocated_bytes` | VM 交接记录的字段 | [`fn build_vm_handoff`](os/kernel/src/vm_handoff.rs:fn build_vm_handoff) |
| `kernel_allocated_bytes` | 同上（记录里的 `kernel_allocated_static`） | 同上 |
| `kernel_allocated_bytes_dynamic` | 同上（记录里的 `kernel_allocated_dynamic`，值来自 bump 分配器用量） | [`fn boot_alloc_used_bytes`](os/kernel/src/boot_alloc.rs:fn boot_alloc_used_bytes) |

表中两个"控制台"落点名字相近、层次不同：`EarlyConsole`（[`early_console.rs`](os/plat/src/early_console.rs)）是硬件输出通道——`minix-plat` 里的跨架构 trait，每架构只实现一个 `write_byte`（x86-64 COM1、aarch64 PL011、riscv64 SBI ecall），启动与诊断输出经它直写串口；`KernelConsole`（[`kmess.rs`](os/kernel/src/kmess.rs)）是内核诊断收口——对应 C 的 `kputc`，把流经的每个字节先记进 10000 字节环形缓冲（C `struct kmessages` 的重写）再转发给当前架构的 `EarlyConsole` 实现，环内容经 `GET_KMESSAGES` 快照供用户态读取。分工一句话：前者管"字节怎么落到硬件"，后者在其上叠加"内核诊断的记录"。

几处需要展开的字段，各自的来源与理由：

- `reserved_regions`：非 conventional 类型的内存区间（内核运行期仍会经由低地址访问的那部分）。它的用途按链分两条：其一（x86_64 特有），内核代码被链接进 boot-shim 的 PE 镜像里执行，其代码/GDT/IDT/TSS 落在低物理地址——VM 建立进程页表时必须把这些窗口按恒等（VA = PA、supervisor-only）复现，否则第一次 `CR3` 切换后的下一条取指就三重故障；其二（UEFI 链通用），快照的非 conventional 侧还装着内核运行期要经 Direct Map 访问的固件分配（loader 数据、内核栈一类不在 conventional 列表里的页），内核建立 DM 覆盖时必须把它们列入候选，否则经 `kernel_phys_to_virt` 的访问会缺页（依据见 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4，那里记着早期启动的缺页读数 `covered_by=NONE`）。OpenSBI 路径上传 `&[]`：内核自引导、镜像落在 DRAM 常规区间里，两条用途都不存在。
    - TODO，x86_64的待退场，此处待核实，并透彻学习。
- `free_upper_idx`：`None` 表示 boot-shim 未计算。它对应 C 的 `freepde_start`，语义与消费见 §3.9。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到；这里把它们显式纳入结构体，交接便不再依赖"两边链接脚本碰巧有同名符号"。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是 2.3 那张参数表在 Rust 侧的落点，但**当前 boot-shim 传的是空切片**：UEFI 的 Load Option（固件为启动项保存的那段参数二进制）与 SBI 的启动参数都还没有被解析成 `key=value` 形式。消费者（`GET_MONPARAMS`）对空切片的处理是按 C 的固定 1024 字节零填充语义补齐，因此"没有参数"与 C 里 `param_buf[0] == '\0'` 等价。空切片是已登记的待办而非终态（代码注释标记 `P9-1`，台账条目 D-66）：接口形状已冻结，接入时只改生产侧，`KernelInfo` 结构零改动。

`#[repr(C)]` 在 `KernelInfo` 上的作用需要说清楚边界：字段顺序被钉住，切片内部 `(ptr, len)` 的顺序仍由工具链决定；因此 `KernelInfo` 继续只用于同镜像内联与直核腿（x86_64、riscv64 与两个 bootface），不承担跨镜像契约。跨镜像腿改用独立协议 `BootHandoffV1`：标量全部写成固定宽度，载荷放进一个四页 arena 的固定分区，描述符只记 `byte_len`——地址由 arena 基址加协议偏移推导，`(ptr, len)` 不再上线。

```rust
// 代码引用（略去字段文档注释），os/libs/minix-boot/src/handoff_v1.rs:struct BootHandoffV1。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoffV1 {
    pub magic: u32,
    pub version: u32,
    pub size: u32,
    pub reserved0: u32,
    pub arena_len: u64,
    pub kern_virt_base: WireVirtAddr,
    pub kern_phys_base: WirePhysAddr,
    pub kern_size: u64,
    pub free_upper_idx: u64,
    pub user_sp: WireVirtAddr,
    pub kern_stack_top: WireVirtAddr,
    pub syscall_entry: WireVirtAddr,
    pub bootstrap_start: WirePhysAddr,
    pub bootstrap_len: u64,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
    pub memmap_byte_len: u64,
    pub reserved_regions_byte_len: u64,
    pub boot_modules_byte_len: u64,
    pub platform_sources_byte_len: u64,
    pub param_buf_byte_len: u64,
}

// 四页 arena：信封 ≤256 B，wire 分区 7616 B，语义工作分区 6144 B，合计 < 16 KiB。
pub const ARENA_CAP: usize = 4 * 4096;
```

arena 的分区是固定偏移：信封住首部，其后依次是内存图、保留区、模块、平台源、参数字节的 wire 形式；再后是同一批内存图的语义形式（`MemoryRegion`），供发送侧在分类之后直接构造进程内 `KernelInfo`。接收方先校验 `magic`、`version`、`size` 与 `arena_len`，再按各 `byte_len` 把五块载荷复制进自己的 `.bss` 落地区；越界、不能整除或容量不足都在构造任何引用之前失败。运行期字段（根页表物理地址与 bump 三件套）在 shim 建好根之后原地补进信封，因此 `x0` 只需传 arena 基址一个数。

协议头带版本号，接收方拒绝不认识的版本。这一点把跨镜像契约从"两个镜像恰好由同一工具链构建"的工序假设，变成可显式验证的版本化 ABI；旧的一页 `BootHandoff` blob 因此在切根落地后删除。

**不做深拷、把 `KernelInfo` 的切片按原指针存进内核的全局槽会怎样**。x86 内联链的五块载荷源在 boot-shim 的 UEFI 池堆里，按快照口径这些页是"空闲"的（§4.2 的不变量）；aarch64 的 v1 arena 虽然由 `LOADER_DATA` 分配、不会被报成 conventional，但它只在首次进程根切换之前经恒等窗口可读——接收方要长期使用的数据同样必须在切根之前进 `.bss`。真机记录给了这一现象的两组读数，零化时点不同：`res=127 zeroed=127`（`res` 是到达分类点的 `reserved_regions` 条目数，`zeroed` 是其中因长度读回零而被拒的条数——127 对 127，也就是全部读成了零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 现场看不到。两组读数指向同一个设计结论（值必须在入口尽早深拷）；机制上有两条候选：源码注释给过的一条是"内核自己的页表不重放这些堆页的 1:1 映射"，另一条是"建立 Direct Map 覆盖时还完好"这组读数更像的"被当空闲复用后清零"。两条候选各带可检验的推论——"页表不重放"说与切表时点相关、"复用清零"说与快照/复用的时序相关——本篇用下面这枚探针对二者做了判别。TODO 这个是TODO是交互式文档类TODO，我读不懂？？

**N-5 探针读数（本仓 QEMU/OVMF）**：在 `test-kernel-map` 加一枚对照页——内存快照之后经 `AllocatePages(MaxAddress(4 GiB − 4 KiB), LOADER_DATA)` 分配、填位置标记，然后在三个时点整页校验和：EBS 前、EBS 后、`arch_boot_impl` 开分页后。读数为 `pa=0x5e9b000`、`pre = post_ebs = post_enable = 0x4a00_0000_0001_ff00`、`verdict=replayed`，即该页的 1:1 映射在内核自举根下落成、内容未被触碰；同一页在快照 conventional 表中的归属为 0（`in_snapshot_free=0`）。结论分层：①"页表不重放 1:1"这一候选对"低于 4 GiB、快照后分配的 LOADER_DATA 页"不成立；②"被当空闲复用清零"的结构前提（快照把该页列为空闲）在本次 OVMF 形态也未出现；因此两组历史零化读数不能用池页可读性解释，剩余解释落在深拷之前的那条链（入口引用被跨 `noreturn` 跳转打坏；深拷把它们移进内核 `.bss` 后症状消失）。探针保留在 `test-kernel-map`（只加打印、不改 PASS 判据），作为该不变量的可复跑证据。

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
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。用一对起止符号而不是单一起点，是因为下游要的是半开区间 `[start, end)` 的长度——回收接口 `add_memmap` 收 `(addr, len)`，只有起点算不出长度。
- `. += _kern_offset;` 之后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。两个地址必须分开的理由：引导器只按装载地址把字节写进物理内存，而分页开启后 CPU 按执行地址取指；`AT()` 就是链接期把同一个段的两个地址钉在一起的方式。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后就不再需要"的代码/数据（它的执行地址等于物理地址，靠恒等映射就能访问）。C 在启动完成之后把它 `add_memmap` 归还给通用分配器。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上内核镜像的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外）；**实际执行地址**上，x86_64 的 UEFI 链把内核链接进 boot-shim 的 PE 镜像里执行，此时内核跑在固件安排的物理地址上（§3.4 的 `reserved_regions` 与 §4.8 讲的就是这件事）；aarch64 的 UEFI 链不内联执行——boot-shim 侧建好自举表后绝对跳进独立加载的 `kernel.elf`，内核从第一条指令起就在高半（§3.7）；riscv64 的自引导链则起步于物理视图（medany 平移视图，见 §4.8），高半地址要等开分页之后才被解引用。两类执行形态（内联 / 独立镜像）下都不存在"分页开启前执行、之后可归还"的**内核段**：x86_64 的内核代码段归 boot-shim 的 PE 镜像管（它不在内核的回收范围里），独立镜像链的内核段全程需要。因此 `bootstrap_start/len` 保留成"形状对称的字段"，值固定为 `PhysBytes(0)` 与 `0`，语义是"没有需要归还的区间"。

与 C 的差异不止"值不同"，还有**执行方式不同**：C 里这段区域的归还没有前置判断——长度为 0 的归还请求由 `add_memmap` 内部静默吸收（`if(len == 0) return;`，2.4 已见），Rust 里它被提升成入口校验的一条硬断言（`kernel_info.rs` 的 [`KernelInfo::validate`](os/libs/minix-boot/src/kernel_info.rs:KernelInfo::validate)）：

```rust
// 代码引用（保留断言本身与其前两行注释），os/libs/minix-boot/src/kernel_info.rs:KernelInfo::validate。
        // 1. bootstrap_len must be 0 (Rust port has no unpaged section).
        //    C: kinfo.bootstrap_len = &_kern_unpaged_end - &_kern_unparsed_start
        //    Rust: always 0 (see bootstrap_start field doc).
        assert_eq!(
            self.bootstrap_len, 0,
            "KernelInfo: bootstrap_len must be 0 (Rust port has no unpaged section), got {}",
            self.bootstrap_len
        );
```

（引文第二行里的 `unparsed` 是源码注释自带的笔误——同一文件其他注释拼作 `_kern_unpaged_start`；代码引用照抄原样。）

变成硬断言的收益是故障前移：一个非零的 `bootstrap_len` 不再有机会进入后续分配逻辑，而是在 `kmain` 入口的校验阶段停下。误设的后果可以逐块列出来：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0`（下表以 riscv64 链为例：`kern_phys_base = 0x80200000`） | 那块内存实际存着什么 | 被误分配后先坏在哪 |
|---|---|---|
| `[0, OpenSBI 固件尾)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址 |
| DTB 所在区间 | 设备树 blob；boot-shim 不解析它，只把物理地址作为 `PlatformDescSource` 传下去 | 内核解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇） |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件与那张表 | 后续从表里再取文件时读到垃圾；表魔数校验失败即中止 |
| boot-shim 自身 | 正在执行的 boot-shim 代码与栈 | 直接取指失败（这一块在误回收发生时可能已经不被引用） |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写掉"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。

**把 `bootstrap_len` 按"上界地址"而不是"区间长度"填会怎样**（即上面那张表的误设，下面展开它的因果链）：这保留了 C 的语义（字段可回收），但会按长度 `kern_phys_base.0` 归还（`build_kernel_info` 的调用点注释把这一取值记为过度回收的来源）。内核会在启动后期执行 `add_memmap(0, kern_phys_base.0)`，把 `[0, kern_phys_base.0)` 这一整段低内存加入空闲清单——这段里包含固件与引导器的活动结构（上表四行按 riscv64 链列出；x86_64 链上该区间是 `[0, 0x200000)`，`0x200000` 即 x86_64 镜像的 `KERNEL_PHYS_BASE`，[`x86_64.ld`](os/kernel-image/x86_64.ld)——与本节上面引的 C 内核不是同一个镜像）。第一次从这份清单分配物理页的调用者（VM 的启动分配器或内核的页表池）会把页表页写进那里，覆盖上表所列结构之一（按各链的取值域）。故障点与写入点之间隔着若干次分配，从崩溃现场无法回溯到"多了一个 `kern_phys_base.0` 的长度"。

### 3.6 `HUGE_PAGE_SIZE` 与 `map_huge`：放进基础 trait 还是扩展 trait

C 的建表两个函数都直接用 4MB 大页：`pg_identity` 给每个页目录项置 `I386_VM_BIGPAGE`，`pg_mapkernel` 也置同一位（2.6 已贴）。大页在这套架构上不是可选能力，而是"1024 项 × 4MB = 4GB"这套单级页目录布局的必要条件：单个页目录项直接指向 4MB 物理块、不需要再挂页表页，一级页目录就能覆盖 4GB。

64 位侧情况不同：层数变多，每个架构支持的大页尺寸集合与"怎么表示这是一个大页"都不同。Rust 侧把这部分能力放进一个扩展 trait（`paging_ext.rs` 的 [`trait HugePages`](os/arch/src/arch/paging_ext.rs:trait HugePages)）：

```rust
// 代码引用（`supports_1gb_page` 上方的文档注释见下方说明段），os/arch/src/arch/paging_ext.rs:trait HugePages。
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

引文末尾的 `supports_1gb_page()` 值得单独说一句：它的默认实现恒真，源码注释给出的适用范围是"ARM64 与 RISC-V 恒支持，x86-64 依一次 CPUID 检查（`CPUID.80000001H:EDX.GBPAGES`）判定"；它目前没有生产调用者（注释把这一条记为 latent capability）——Direct Map 覆盖建立处的运行时 1GiB 探测走的是另一个 trait（`DmCoverageArch::supports_1gb_page`），不经过这里。

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

x86-64 的实现给出这套设计的两个可核对点（`paging.rs` 的 [`impl HugePages for X86_64Paging`](os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging) 的常量与 `fn map_huge`）：

```rust
// 代码引用，os/arch/src/x86_64/paging.rs:impl HugePages for X86_64Paging 的常量与 map_huge 守卫段。
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

- `HUGE_PAGE_SIZES = &[1GB, 2MB]`、`HUGE_PAGE_SIZE = 1GB`、回退 2MB：尺寸阶梯是"首选失败往下退"。谁选尺寸不在这里，而在 `arch_boot_impl` 的校验步骤（`lib.rs` 的 [`fn boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare)）：

```rust
// 代码引用，os/kernel/src/lib.rs:boot_validate_and_prepare 的尺寸选择段。
    // Choose the largest huge-page size that both bases and kern_size are aligned to.
    let kern_huge = if kv.is_multiple_of(huge) && kp.is_multiple_of(huge) && ks.is_multiple_of(huge) {
        huge
    } else if kv.is_multiple_of(fallback) && kp.is_multiple_of(fallback) && ks.is_multiple_of(fallback) {
        fallback
    } else {
        panic!("arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE");
    };
```

  三条件（虚拟基址、物理基址、长度）同时对齐到 1GB 才用 1GB。这条阶梯不是风格：`check-layout.sh` 的 L5 断言要求镜像跨距按 2MiB 对齐，正是为了让它至少能落进回退档（断言清单见 §5.2）。
- `AlreadyMapped` 守卫处理的是"目标槽位已经是一个 1GB 大页"的情形。注释给了理由："demoting it would corrupt the existing mapping."

**去掉 `AlreadyMapped` 守卫会怎样**：直接按 2MB 路径往下走。`e3` 里原本存的是 `paddr | PRESENT | PS`（因为 1GB 大页的 PDPT 条目就是叶条目），代码在"条目已存在"分支里做的是 `(e3 & ADDR_MASK) as *mut u64`——也就是把那个 1GB 物理块的基址当成下级页表指针。接下来 `write_entry(pd, i2, ...)` 写的是 `paddr + i2*8`：这段内存正被 1GB 映射覆盖，可能就是内核自己的数据页。写完之后 1GB 映射还在（条目没变），但它的前 4KB 已经被当成页表改写。守卫的代价是"这次 2MB 映射失败"，去掉守卫的代价是"被映射的那段物理内存头部被写坏"。

### 3.7 控制权交接的三种形态：同镜像内调用、跨镜像跳转与直核自引导 `[ARCH: 内核从内联执行改为独立镜像 + 交接载荷]`

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

（下文反复用到 AArch64 的 TTBR0/TTBR1：它们是两张页表基址寄存器，分别承载低半与高半地址的翻译表；切换地址空间只换 TTBR0，TTBR1 在上下文切换中不被替换。）

Rust 侧的交接面是一个函数：`arch_boot`（按架构分道的入口，同一份代码里有三个 cfg 分支，因此锚点取它的通用实现 `lib.rs` 的 [`fn arch_boot_impl`](os/kernel/src/lib.rs:fn arch_boot_impl)）：

```rust
// 代码引用（保留签名与首段注释），os/kernel/src/lib.rs:fn arch_boot（x86_64 版本）。
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

aarch64 上多一层（`lib.rs` 的 [`fn enter_kernel_image_v1`](os/kernel/src/lib.rs:fn enter_kernel_image_v1)）：内核以独立 ELF 的形式存在于高半（TTBR1），boot-shim 先分配一块四页 arena 并把 v1 协议写进去，在 ExitBootServices 的配对循环里分类最终内存图，然后建表、开分页、把运行期字段补进 arena，最后**绝对跳转**过去；`x0` 只带 arena 基址。

```rust
// 代码引用（省略号处为步骤注释与校验），os/kernel/src/lib.rs:fn enter_kernel_image_v1。
    let kern_huge = boot_validate_and_prepare::<AArch64Paging>(kernel_info);
    build_bootstrap_root_and_enable::<AArch64Paging>(kernel_info, root_page, kern_huge);
    let (bump_base, bump_end) = boot_alloc::boot_alloc_region()
        .expect("enter_kernel_image_v1: bump region not registered");
    let bump_next = boot_alloc::boot_alloc_next();
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa as *mut u8, ARENA_CAP) };
    patch_runtime_fields_in(
        arena,
        RuntimeFields {
            root_page: root_page.0,
            bump_base,
            bump_next,
            bump_end,
        },
    );
    let entry = kernel_info.kern_virt_base().0;
    unsafe {
        core::arch::asm!(
            "isb",
            "br {entry}",
            in("x0") arena_pa,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
```

三处具体事实：

- 交接载荷带 `bump_next`。原因在 §4.7 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次。arena 本身由 UEFI 单独分配，不占 bump 池，因此这里没有"载荷页"要跳过。
- `in("x0") arena_pa`：arena 基址绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧先把五块载荷解码进本镜像 `.bss` 的落地区，再只做 additive 阶段（`arch_boot_resume_v1` 之后调用 [`fn arch_boot_resume_high_half`](os/kernel/src/lib.rs:fn arch_boot_resume_high_half)）：

```rust
// 代码引用（首行与结尾的设计史注记略去——结尾那段从源文件 354 行句中起、共约三行半；末行在源文件里与后续内容同行，此处截断至句号），
// os/kernel/src/lib.rs:fn arch_boot_resume_high_half 的文档注释。
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

riscv64 上是第三种形态：内核镜像自己就是引导体（入口是 [`main.rs`](os/kernel-image/src/main.rs) 里按架构分道的 `rust_image_main`，riscv64 的实现是 `bootface.rs` 的 [`fn bootface`](os/kernel-image/src/bootface.rs:fn bootface)；aarch64 的对应面是 `bootface_a64.rs` 的 [`fn bootface_a64`](os/kernel-image/src/bootface_a64.rs:fn bootface_a64)，只在该架构构建时编译），在没有 U-Boot 与文件系统的 `-kernel` 场景里从设备树解析内存、从 `BootFileTable` 或 DTB 通道取模块，然后调用同一个 `arch_boot`。三链的 memmap 层已通过 minix-boot 的同一 `MemoryMapSource` 合流（I-22）——引导形态层不合并，memmap 源单一下沉。三条链的入口同源、收敛点也是同一个函数。

**两个替代做法各会撞什么**（两处）：

其一，aarch64 退回内联执行。内联时内核代码住在 PE 镜像的运行物理地址上（低半、`VA = PA`、在 TTBR0 覆盖范围内）。第一次 `switch_address_space` 会换 TTBR0 到进程页表并发 `tlbi alle1is`，内核自身的取指映射随之消失——[`fn arch_boot_resume_high_half`](os/kernel/src/lib.rs:fn arch_boot_resume_high_half) 的文档注释记下的现场是 `elr`/`vbar` 钉在同一个低地址上。跨镜像形态把这个问题的根去掉：内核代码链接在高半（TTBR1），而 TTBR1 在上下文切换中不被替换。

其二，接收侧按"干净启动"重建 bootstrap 根（调 `Paging::new_from_page`）。该函数的第一件事是把 512 个根条目清零（`write_bytes(ptr, 0, 512)`），而此时这个根**正在被使用**——承载当前函数的那条指令所在的 L0 槽就在其中。清零后的下一次取指立刻页错误，栈上连一条 panic 消息都来不及打印；"resume 面只做 additive 阶段"这条约束就是从这里来的。

**直核自引导在真板上的位置** `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]`。RISC-V 的固件规范 OpenSBI 提供 `FW_PAYLOAD` 模式——把下一级镜像打包进固件本体，boot 时固件直接跳进 S 模式目标；真板上这类极简装机（省掉 U-Boot 与文件系统）不需要独立引导器存在，QEMU 的 `-kernel` 模拟的正是同一时序（§3.1 例外条款的背景段）。三架构的落位并不对称：

| 架构 | 直核形态的位置 | 原因 |
|---|---|---|
| x86_64 | 无对应形态 | 平台规范强制 BIOS/UEFI 作为唯一引导面，没有"固件直接跳任意 payload"的规范形态 |
| aarch64 | 覆盖测试为主 | QEMU `-kernel` 是主力验证通道；真板启动一般仍经 UEFI/U-Boot |
| riscv64 | 测试与真板嵌入式并存 | `FW_PAYLOAD` 是规范形态，真板极简装机直接使用；`-kernel` 是它在 QEMU 的对应物 |

所以直核自引导是**辅助启动路径**，不是三架构的主线：主线是"独立 boot-shim + 独立内核镜像 + 交接载荷"（§3.1 的分工表；x86 的收口见 §3.4 的 NK1 待办）。这张矩阵同时解释了 `bootface`/`bootface_a64` 为什么保留而不作主产线——它们服务的是特定嵌入式形态与测试通道。

### 3.8 引导期页表页分配器的换手

交接形态（3.7）定下来之后，这一节看引导期页表页分配器怎么在这两个镜像之间换手。C 的两个来源已经看过：`pg_alloc_page` 从可用内存记录的最高条目尾部取一页，`alloc_pagetable` 从 6 页静态池里取一页，用完 panic（2.4、2.6）。这个策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

Rust 侧把两者拆开（`pt_alloc.rs` 的 [`fn register`](os/arch/src/arch/pt_alloc.rs:fn register)、`:fn alloc_pt_page`）：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

```rust
// 代码引用（省略号处为模块级的并发安全注释），os/arch/src/arch/pt_alloc.rs。
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
    // Acquire: pairs with `register()`'s Release store — guarantees the
    // fn pointer write is visible to this thread once registration is
    // observed (write-once-then-read-only; see module docs).
    let _ = PT_REGISTERED.load(Ordering::Acquire);
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}
```

boot 阶段的实现是一个 bump 分配器（`boot_alloc.rs` 的 [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:BootAlloc::alloc)）：

```rust
// 代码引用，os/kernel/src/boot_alloc.rs:BootAlloc::alloc。
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
| boot-shim 进入内核之前 | boot-shim 进程自己 | `boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end); minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);` | [`main`](os/boot-shim/src/main.rs:main) |
| `arch_boot_impl` Step 0 | 只有"未注册"时才写 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:boot_validate_and_prepare) |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | [`vm_server.rs`](os/servers/vm/src/vm_server.rs) |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；boot-shim 侧是第一个写者，它的 `register` 调用是无条件的（`main.rs` 的 [`fn main`](os/boot-shim/src/main.rs:fn main)）。这个槽是"写一次"的，重写是 bug，因此没有"后注册者覆盖前者"的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池（C 的对应物是 `pt_free` 把页表页还回 `alloc_mem` 池）；两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`vm_handoff.rs` 的 [`fn build_vm_handoff`](os/kernel/src/vm_handoff.rs:fn build_vm_handoff) 调用的 `:fn classify`）；引导期建表之后的 Direct Map 覆盖建立则在 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs)：

```rust
// 节选（略去行号引用；其中 C 对应关系一行按 §2.4.2 的指针改述，见上文），os/kernel/src/vm_handoff.rs 的扣除区间表头与 bump 行。
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base()/kern_size()` |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs (deduction record) | `KernelInfo.boot_modules()`
//!   — **every** blob including VM's own (VM's segment frames come from this
//!   free list before VM starts) |
//!
//! Note the two module lists are different on purpose: the deduction record
//! keeps every blob, while the handoff module list (`VmBootHandoff.modules`)
//! excludes VM — C: `kinfo.module_list[]`.
```

```rust
// 代码引用，os/kernel/src/vm_handoff.rs:fn classify 的 bump 扣除段。
    // Kernel-reserved: the boot bump region, whole range.
    if let Some((base, end)) = boot_alloc_region() {
        record.push(base, end.saturating_sub(base));
    }
```

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在本节末的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（就是 2.4.2 表里的"还 VM 镜像"那一行），而**实现**把**所有**启动模块的字节区间都算作内核保留。表头把两份清单分开陈述：扣除记录含 VM 自己，交接模块清单（`VmBootHandoff.modules`）才按 C 的 `kinfo.module_list[]` 排除 VM。理由是启动顺序：内核的 boot-exec 在 VM 服务起来之前逐个装载 0..7 号服务器，它们的段帧正是从这份自由清单里取的；若按 C 那样先把 VM 模块的区间还回去，VM 镜像可能在它自己被启动之前就被后续装载覆盖。

**改成按"已用部分"扣除会怎样**（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时 `fetch_add(0x1000)` 返回 `0x1E64000` 并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）——而这一页刚被交给 VM 的物理内存管理器。若 VM 已经把它分配给某个进程页表，写零就把那张表的一个条目变成 0；切换 `CR3` 之后，被这条目覆盖的虚拟地址取指即页错误。现场只能看到一个"凭空消失"的映射。

### 3.9 `free_upper_idx`：字段保留、消费者搬走

页表页由谁发说完，剩下的是页目录槽位的账——C 的 `freepde_start` 是 2.6 末尾那条链的起点：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy` 借窗口做跨地址空间拷贝。这套机制的三个前提是：当前页表里确实有连续空槽、窗口只在当前 `CR3` 下有效、借出者保证归还。

64 位侧跨地址空间访问换了载体：内核有 Direct Map 窗口（一段固定虚拟基址到物理地址的线性映射，由 `arch_boot_impl` 的 Step 4 建立），加上软件页表走查（[`pte_walk.rs`](os/kernel/src/pte_walk.rs) 一类的实现）就能在**不切页表、不借槽位**的前提下读到任意进程的页表项；用户空间的拷贝则由 VM 服务在自己的地址空间里处理。于是"借用空闲槽位"的三个前提在 64 位侧都不再是必要结构。

字段因此保留成 `Option<usize>`（`kernel_info.rs` 的 [`free_upper_idx`](os/libs/minix-boot/src/kernel_info.rs:free_upper_idx)）：

```rust
// 节选（保留字段注释前两行；`pre_init.c` 后的行号后缀按本篇路径约定略去），os/libs/minix-boot/src/kernel_info.rs:free_upper_idx。
    /// First free page table root-level index after identity+kernel maps.
    /// C: kinfo.freepde_start = pg_mapkernel() — pre_init.c
    /// x86-64: PML4 index, aarch64: TTBR1 L0 index, riscv64: Sv39 VPN[2]
    ///
    /// `None` means the boot-shim has not yet computed this value.
    /// In Minix3 C, `pg_mapkernel()` returns the index; the Rust boot-shim
    /// currently does not compute it, so it is set to `None` and the kernel
    /// must validate it before use.
```

保留下来的用处只有一处：把 C 的 `struct kinfo` 形状报给用户态时占住那个位置（[`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` 用 `unwrap_or(0)` 把它填进返回结构）。这个出口也说明为什么要用 `Option` 而不是"随便算一个值"：`None` 是"没有这个值"，`Some(0)` 是"0 号槽位空闲"——后者会被下游当成有效索引使用。

在 64 位侧，偏移一位的形态与 C 不同：这个字段的消费者只有 GET_KINFO 的报告（[`misc.rs`](os/kernel/src/misc.rs)），所以一个"最后一个已用槽位"的值不会造成取指失败，而是让用户态读到一个错的槽号——把它当成有效索引去用。这正是字段保留为 `Option` 并在没有真实计算者时取 `None` 的理由：类型能区分"没有值"（`None`）与"某个值"（`Some`），却区分不出"算对的槽号"与"算错一位的槽号"——在没有真实计算者时随便填一个数，只会把后者伪装成前者；C 侧那套机制里偏移一位的后果（改写内核映射、下一次访问页错误或静默读错）见 §2.6。

对照表：

| 维度 | C 的槽位机制 | Rust 的窗口机制 |
|---|---|---|
| 借用者 | `createpde`（跨地址空间拷贝） | Direct Map 窗口 + 软件页表走查 |
| 借用范围 | 当前页表的 4MB 窗口（一个页目录项） | 内核固定映射窗口（整个物理地址空间的可达部分） |
| 归还时机 | 下一次切换地址空间时自然失效 | 不需要（窗口是常驻映射） |
| 失效条件 | 借来的条目只描述对方的一个 4MB 块 | 窗口覆盖范围由 DM 建立时的资格过滤决定 |

**在 64 位侧照搬"借用空闲 PML4 槽位"会怎样**。每进程页表的高半槽位是从 bootstrap 根继承的（`X86_64Paging::inherit_supervisor_half` 把源根的 PML4 的 256 至 512 号槽位原样复制过来）。被借用的槽位在 `switch_address_space` 之后属于另一个地址空间：借出去的映射在下一次切换时静默失效，不会报错。症状是跨空间拷贝偶发读到"上一张页表留下的地址"——一个只在特定调度顺序下出现的错误，而不是一次可复现的崩溃。

### 3.10 从 Ch2 到 Ch3 的对照收束

| Ch2 的 C 事实 | Ch3 的设计回应 | 一句话差异 |
|---|---|---|
| 引导器代劳加载并把结果填进结构体（2.1/2.2） | 3.1 固件接口换代 + 3.2 `BootShim`/`FileLoader` | 加载工作从引导器转移到 boot-shim，固件差异收进两个 trait |
| 全局 `kinfo` + 指针传递（2.3） | 3.4 按值 `KernelInfo` + 深拷落地区 | 地址不能跨镜像，值可以 |
| `unpaged` 段可回收（2.3/2.6） | 3.5 语义改为"必须为 0" + 硬断言 | 没有分页前执行的内核段，就没有要归还的区间 |
| 4MB 大页是两个建表函数的实现细节（2.6） | 3.6 `HugePages: Paging` 扩展 trait | 大页从"实现细节"升级为"可查询的硬件能力" |
| 单二进制内的 `pre_init → kmain` 调用（2.2） | 3.7 交接形态（同镜像内调用 / 跨镜像跳转 / 直核自引导） | 交接面从一个指针变成一个函数签名 |
| 静态池 6 页 + 从记录尾部取页（2.4/2.6） | 3.8 注册-替换的分配器槽 | 分配策略与页表代码解耦，boot 与 VM 各注册一份 |
| `freepde_start` → 借槽位做临时窗口（2.6） | 3.9 字段保留为 `Option`，机制换 Direct Map + 走查 | 借用会失效，窗口不会 |

**换一种读法会漏掉什么**：若把这些设计当作互不相关的偏好来读，其中两条的关联会被漏掉。3.5（`bootstrap` 区间语义重定义）与 3.6（大页能力放进扩展 trait）在表面上没有关系，但它们各自的成立前提是同一组事实——"内核镜像里没有一段'分页开启前专用、之后可归还'的段"，且建表代码与运行时同处一份镜像：因为不再有这样的段，所以没有要归还的区间；因为建表与运行时共用同一份实现与接口，大页能力必须能被查询而不是被写死。把这条前提说出来，读者才能判断哪些设计会随执行模型一起变，哪些只是当前的实现选择。

---

## 4. 实现详解

本章把第 3 章的设计决策落到代码：4.1–4.3 走一遍入口与两条固件路径的调用链（含共用装载函数），4.4–4.5 看 `KernelInfo` 的单一 builder、校验与五块 `.bss` 落地区，4.6–4.7 是自举根建立/开分页与分配器换手的落地，4.8 是跨镜像交接与直核自引导，4.9 收成 C 函数对照表。各节标题后的 `→` 指回它所实现的设计决策。

### 4.1 入口与调用链（→3.1、3.2、3.8）

UEFI 入口是固件加载 `.efi` 后调用的那个函数。它只做三件事。

先给两条固件路径一张总览图——本章后面各节再逐段展开，这张表只负责导航：

| 阶段 | UEFI 路径 | 非 UEFI 链（OpenSBI / U-Boot / `-kernel` 直核） |
|---|---|---|
| 谁在跑引导面 | 固件直接调 boot-shim 的 `.efi` 入口（[`fn main`](os/boot-shim/src/main.rs:fn main)） | 内核镜像自带的引导面（[`main.rs`](os/kernel-image/src/main.rs) 的 `rust_image_main` → `bootface`/`bootface_a64`，复用 boot-shim 库的取件车道） |
| 读内核与模块 | boot-shim 自己读：`SimpleFileSystem` 加 12 项 `MODULE_NAMES`（§4.3） | 生产链（U-Boot）：预载 → `BootFileTable` 查地址读字节（§4.3）；`-kernel` 直核：从 DTB 解内存、从表/DTB 通道取模块（§4.8） |
| 内存与参数准备 | aarch64：分配四页 arena，先写信封与非内存图载荷，再在 EBS 配对循环里分类最终内存图；x86_64：`GetMemoryMap` 一次快照分流两份列表。两条都进同一个 `build_kernel_info`（§4.2、§4.4） | 生产链与直核共用 `DtbMemoryMap`（DTB 的内存节点，I-22）；UEFI 链的 x86 快照走 `UefiMemoryMap`——两种来源都进同一个 builder，只有 `reserved_regions` 一行取值不同（`param_buf` 两边都是空切片；§4.4） |
| 退出固件服务 | aarch64：配对循环里用 raw `ExitBootServices`，分类与退出用同一个 MapKey；x86_64：`uefi::boot::exit_boot_services` | 无此步骤（引导器早已把控制权交出去） |
| 进入内核 | aarch64：建根、补运行期字段、`x0 = arena` 跳独立镜像（`enter_kernel_image_v1` / `arch_boot_resume_v1`）；x86_64：注册页表页分配器后进 `arch_boot`（§4.6、§4.7、§4.8） | 镜像自身完成建表并进 `arch_boot`（§4.6、§4.8） |

三条链在这一章里各走各的阶段，但收敛点相同（同一个 `arch_boot`/`arch_boot_impl`；跨镜像路径另走 `arch_boot_resume_v1` → `arch_boot_resume_high_half`）——收敛点的完整交代见 §4.8。

```rust
// 节选（保留分道骨架；注释有删节与改述，省略号处为被略内容），
// os/boot-shim/src/main.rs:fn main。
#[entry]
fn main() -> Status {
    // 1024 pages (4 MiB): … the handoff arena's fresh chain all draw
    // lower-level page-table pages from this bump pool via boot_pt_alloc.
    #[cfg(target_arch = "aarch64")]
    {
        boot_shim::uefi_helpers::prepare_and_enter_kernel_image(1024)
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let result = UefiBootShim::prepare_boot(1024);

        // --- Below this point, UEFI boot services are gone ---
        boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
        minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);
        minix_kernel::arch_boot(&result.kernel_info, result.root_page)
    }
}
```

逐行解码：

- `main.rs` 的 [`fn main`](os/boot-shim/src/main.rs:fn main) 里的 `1024` 是**页表页预算**（1024 页 = 4MiB）。第一段注释把这份预算的构成列了出来：恒等映射、内核高地址映射、Direct Map 覆盖建立、引导身份窗口的逐页拆解（把引导期铺的恒等映射逐页撤掉、给 Direct Map 让位）、以及交接 arena 之后的页表链条——它们都从同一个 bump 池里取页。预算是"上限"而不是"必须用完"：`BootAlloc` 只在分配时推进游标。**未用部分不退回**——VM 交接记录按 `boot_alloc_region()` 把整段扣除（见 4.7 的扣除表），用掉的与剩下的都算内核保留。（UEFI 路径下 bump 区本就不在 conventional 列表里，这条扣除对 free list 是空操作；扣整段防的是自引导等路径把 bump 区记进清单的情形。）
- x86_64 的注释 `--- Below this point, UEFI boot services are gone ---` 标的是分界线：这一行之后任何 `uefi::` 的调用都不可靠。分界线之后的第一件事是注册分配器，其后两条路标（`pt_alloc registered` 与 `handing control`）都走 `raw_serial`，不依赖任何固件状态。
- aarch64 的 `prepare_and_enter_kernel_image` 自己完成同一分界：arena 分配、载荷序列化、配对循环都在 EBS 之前，EBS 之后只做建根、补字段与跳转；路标同样走裸串口。
- 进入内核的分道只有这一处，两条路都会进入同一个 `kmain`。另有一件容易混的事要分清：x86_64 的引导路径执行的是链接进 boot-shim 的内联内核副本——`arch_boot` 与 `kmain` 都在 PE 镜像的低地址（`VA = PA`）上执行，而且**一直如此**（VM 因此在进程页表里复现这些低地址窗口，见 §3.4；aarch64 走不了这条路，见 §3.7）。从 ESP（EFI System Partition，固件按 FAT 挂载的启动分区）读到的 `kernel.elf` 段被直接写进物理内存，§4.6 Step 2 把内核的高半地址映射到那一份副本——高半栈等由这份映射承载。

**若把页表分配器的注册挪到进入内核之后会怎样**：步骤 2 的 `init_boot_pt_alloc` + `register` 排在 `arch_boot(...)` 之后。Step 0 的兜底判据是 `boot_alloc_region()` 为 `None`（不是 `is_registered()`；`is_registered()` 只守注册这一步），于是它会从 `memmap` 里找一段满足 DM 可达上界的区间，取 `FALLBACK_BUMP_LEN`（`0x100_000`，256 页）作为新的 bump 池并注册。此后 `boot_alloc_region()` 指向这 256 页，VM 交接记录的扣除表扣的就是它；而 boot-shim 已经预留的那 1024 页（`assert_bootstrap_outside_memmap` 保证它不在 conventional 列表里）既不在扣除表里、也不在任何可用清单里——那 4MiB 被空置。若 boot-shim 的 `register` 仍然执行（只是挪后），它还会撞上"写一次"的约定——二次注册的后果（debug 直接失败、release 静默改写）在 §4.7 的"去掉内核侧守卫"推演里展开，此处不重复。

### 4.2 UEFI 路径：`prepare_boot` 的顺序（→3.1、3.6）

```rust
// 节选（保留八步顺序；注释有删节与改述，代码处的省略号标记被略内容），os/boot-shim/src/uefi_helpers.rs:UefiBootShim::prepare_boot。
impl BootShim for UefiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        uefi::println!("boot-shim: prepare_boot enter");
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        uefi::println!("boot-shim: root+bump allocated");
        // 顺序约束（已由真机证实）：快照必须在**全部**
        // UEFI LOADER_DATA 分配之后——root 页、bump 区之外，还有 12 个
        // boot module 的 allocate_pages、以及各文件的读取缓冲（后拍让活图
        // 已把这些页标成 LOADER_DATA，conventional-only 快照天然排除它们）；
        // 内核段的落位是直写 conventional RAM、不经 UEFI 分配，不属这条
        // 约束，它的排除由内核 A2 分类自报（见步骤 7 的断言范围说明）...
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

        BootPrepareResult {
            kernel_info,
            root_page,
            bump_base,
            bump_end,
        }
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
| 6 | 拍内存快照（两份列表） | 必须在**要被内核排除的那些**分配（root 页、bump 区、模块页）之后；这一次 `GetMemoryMap` 同时产出 free 与非 free 两份（两个列表自身的存储页在快照之后才分配，见下方不变量说明） |
| 7 | 断言自举分配不在 free 列表里 | 快照之后、EBS 之前（此时还能打印） |
| 8 | 构造 `KernelInfo`，退出 Boot Services | 退出之后只剩裸串口 |

第 1、2 步的界：

```rust
// 代码引用（略去文档注释），os/boot-shim/src/uefi_helpers.rs。
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

aarch64 的 UEFI 入口不走上面这条「一次快照分流」的顺序，而是 `uefi_helpers.rs` 的 [`fn prepare_and_enter_kernel_image`](os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image)：

| # | 动作 | 约束 |
|---|---|---|
| 1 | 分配根页表页与 bump 区 | 与 x86 同界（`MaxAddress(bootstrap_alloc_limit(size))`） |
| 2 | 加载 `kernel.elf`、12 个模块、平台描述符来源 | 必须在 EBS 之前（文件系统与 System Table 只在此时有效） |
| 3 | 分配四页 arena（`AllocateMaxAddress(bootstrap_alloc_limit(ARENA_CAP))`，`LOADER_DATA`） | 整段落在恒等/DM 可达界之下；接收方经恒等窗口读它 |
| 4 | 写信封、模块、平台源、参数（非内存图载荷） | arena 已零化；信封的标量字段此时已定稿 |
| 5 | 分配 `GetMemoryMap` 缓冲（`allocate_pool`），进入配对循环 | 循环内先 `GetMemoryMap`，再分类进 arena 的两个内存图分区，最后用同一 MapKey 调 raw `ExitBootServices` |
| 6 | EBS 成功后按描述符长度从 arena 语义分区构造 `KernelInfo` | 只有分类成功的那一份内存图存活；重试前 `reset_map_partitions` 清掉旧条目 |
| 7 | 复核 root/bump/arena 都不在 conventional 列表里 | 失败即裸串口打印后停机；此时已无 Boot Services 可退 |
| 8 | 建根、开分页、把运行期字段补进 arena、`x0 = arena` 跳转 | 建根之后到跳转之前不再分配；`patch_runtime_fields_in` 不零化 arena |

配对循环的失败面按状态码分派：`BUFFER_TOO_SMALL` 只在第一次 EBS 之前允许扩容缓冲；`EFI_INVALID_PARAMETER` 允许重分类一次（MapKey 变了）；其他状态立即失败。总尝试次数用尽或复用缓冲仍不足时 fail-closed，不沿用旧分类结果。

`AllocateType::MaxAddress(limit)` 的语义是"分配出来的基址不超过 `limit`"，所以要把 `limit` 减去本次请求的大小，才能保证"整段在界之下"。这个界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口的物理上界)`：bump 池与 arena 既要能被"恒等写通道"（`VA = PA` 的自举写路径——进入内核前固件页表仍在生效、低 4GB 已被恒等覆盖，自举建表依赖这条通道写页表页；覆盖建立机制不在本篇展开，见 §1.4 的边界声明与 §3.8 的代码落点）写到（构建期间），又要能被 VM 的 Direct Map 窗口表示（构建之后使用者要能通过窗口访问它）。

步骤 6 的两份列表在同一次遍历里分流（x86 路径）：

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
- 快照的"最终"要按**内核需要排除的对象**来理解：`build_memmaps()` 里两个列表自身的存储页（`Vec` 扩容与 `Box::leak`）是在这次 `GetMemoryMap` **之后**从同一个 UEFI 池里分配的——按快照口径它们仍是"空闲"。这条交接仍然安全，靠的是一条次序不变量：内核在 `arch_boot` 入口、**在任何会动用这份空闲清单的代码之前**就 `store_kernel_info`，把全部切片深拷进自己的 `.bss`（§4.5；x86_64 内联入口是这个次序）；此后再没人读这两个页，它们确已可以当空闲发出去（`build_memmaps` 的文档注释记着这条不变量，改结构时必须保住它）。aarch64 的 v1 链不用这份快照：它的 conventional 来自配对循环里最后一次成功的 `GetMemoryMap`，分类结果直接落在 arena 里；接收方在建立 DM 之前把五块载荷解码进同一组 `.bss` 落地区，因此同样不存在"之后还读发送方页"的窗口。
- 步骤 7 的断言只覆盖**两笔 UEFI 分配**（根页与 bump 区）。`build_memmaps` 调用点的注释（不在上面的节选里）记录了一次被证伪的更严格假设：内核段不在这条断言的范围内——它是被**直接写进 conventional RAM** 的（不经 UEFI 分配），由内核 A2 分类逐步扣除（`kmain` 启动阶段批处理的第二步：把引导期内存记录搬进内核自己的空闲清单 `FREE_MEMMAP`，见 `03-kmain-cstart.md`）；曾经按"UEFI 会把分配改标从而自动排除"的假设把断言扩展到内核段，真机第一次启动就误炸（conventional `[0x100000, 0x800000)` 里包含内核 `[0x200000, 0x400000)` 属于预期形状）。aarch64 的同一条闸在 EBS 之后跑在 arena 的语义分区上，覆盖根页、bump 区与 arena 三段。

**若先拍内存快照、后做分配会怎样**：快照被提到第 1、2 步之前。这两页在拍快照时还是 `CONVENTIONAL`，于是被 `regions` 收下，作为"空闲内存"交给内核。内核 A2 分类把 `memmap` 拷进 `FREE_MEMMAP` 后，这两页仍在账上——VM 的物理内存管理器可以从中取帧，而它们正被用作页表页与页表页池。`assert_bootstrap_outside_memmap` 就是为此设的闸：它遍历 free 列表，若发现 `[root_page, root_page+4096)` 或 `[bump_base, bump_end)` 与某条区间相交就立刻 panic，此时串口还在（EBS 之前），消息里同时给出两个区间。反过来说，若这条断言也被去掉，故障会推到 VM 取帧之后，现场只剩一个"页表内容被改坏"的取指错误。

### 4.3 两条路径共用的装载函数（→3.2）

模块契约是本篇里最容易在改动时踩坏、又最容易在测试里漏掉的东西，因为它的"顺序"部分没有任何编译期检查。

```rust
// 代码引用（省略号处为文档注释的说明段），os/boot-shim/src/loader.rs。
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";
pub const MODULES_DIR: &str = "/EFI/minix/modules";

pub const MODULE_NAMES: &[&str] = &[
    "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm", "pfs", "mfs", "init",
];

// 代码引用：编译期计数断言。
const _: () = assert!(MODULE_NAMES.len() == NR_BOOT_MODULES);
```

- 名字表有 12 项（复算：`sed -n '/pub const MODULE_NAMES/,/];/p' os/boot-shim/src/loader.rs | grep -o '"[a-z]*"' | wc -l` 输出 12），`NR_BOOT_MODULES = 12` 定义在 `minix-boot`。12 不是随手定的数：它等于 C `boot_image` 表的用户态段长度，少一项会有服务器没有启动名，多一项则没有对应的进程号可绑（§2.7 的 `NR_BOOT_PROCS` 分工）。
- 顺序按 C 的 boot image 表（[`kernel/table.c`](minix3/minix/kernel/table.c) 里 `struct boot_image image[]` 的用户态段）排列：`ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`。
- **位置不是进程号**。内核把第 `i` 个模块绑定到 `BOOT_MODULE_PROC_NRS[i]`，而后者里的数字来自 `com.h`——例如 `ds` 在位置 0，进程号是 6。名字表这一行代码上方给了这条注解，它是顺序契约的一部分。

装载循环：

```rust
// 代码引用（略去函数文档注释），os/boot-shim/src/loader.rs:fn load_boot_modules_with_loader。
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

        // SAFETY: `alloc_pages` returned `num_pages` pages of writable
        // physical memory; `data.len() <= num_pages * 4096`.
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

- `num_pages = (data.len() + 4095) / 4096`：向上取整到页。分子写 4095 而不是 4096 是按整数除法做 ceil 的标准形——`x/4096` 本身向下取整，先加 `4096-1` 才让任何非零余数进位；写 4096 会在 `data.len()` 恰为整页时多要一页，写小了（≤4094）则只要余数非零就会少一页，最后一段字节落进未分配的页。
- 分配出来的页里，`data.len()` 之后的余量被显式清零（`write_bytes(dest.add(data.len()), 0, remaining)`）。模块镜像按文件字节数拷贝，页尾的余量清零保证同一页内的任何对齐空洞都是确定值，而不是上一轮使用留下的内容。
- `BootModule.len` 记录的是**文件字节数**，不是页数。消费者按 `[start, start+len)` 计算占用区间。
- 缺件是 panic 而不是 `continue`。错误消息里带着模块名、路径与总数，落点在 boot-shim 侧；如果改成静默跳过，错误会推迟到内核侧的数量断言，那里只有"期望 12、实际 N"，没有缺失的名字。

OpenSBI 路径的 `FileLoader` 实现是查表：

```rust
// 代码引用（略去文档注释），os/boot-shim/src/opensbi_helpers.rs:fn entry_path_eq 与 FileLoader for UbootFileLoader。
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

**若把 `MODULE_NAMES` 里 `tty` 与 `mib` 两项互换会怎样**：装载会全部成功（12 个文件都在），`const _: () = assert!(... == NR_BOOT_MODULES)` 也通过——数量没变。但是位置 7 上的 `BootModule` 记的是 `mib` 的镜像与名字，内核按位置把它绑到 `tty` 的进程号上。启动过程依旧能走到 `kmain`，进程表也能建起来；症状出现在第一次对 `tty` 或 `mib` 发系统调用时——调用被打到错误的进程号，表现为"服务没有响应"或"收到协议外的回复"，与"装载时顺序错了"之间隔着整个启动流程。

### 4.4 `KernelInfo` 的构造：单一 builder 与一个校验（→3.4、3.5）

两条固件路径与两个直核面共用**一个** `build_kernel_info`（boot-shim 库根导出）。收敛的是构造面，不是引导形态层：每条腿仍各自产出自己的列表，只有 `reserved_regions` 一项按腿取值。

```rust
// 代码引用（略去函数文档注释；代码内注释有删节与改述），os/boot-shim/src/lib.rs:fn build_kernel_info。
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
        // Per-arch user stack top; the single authority is
        // `minix_types::USER_STACK_TOP`.
        user_sp: VirBytes(minix_types::USER_STACK_TOP),
        kern_stack_top: VirBytes(kern_virt_base.0 + kern_size),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        platform_sources,
        // 启动参数尚未解析成 key=value；空切片等价于 C 的
        // `param_buf[0] == '\0'`（内核 GET_MONPARAMS 拷贝 0 字节）。
        param_buf: &[],
    }
}
```

`reserved_regions` 的取值按腿：UEFI 腿传快照里的非 conventional 列表；OpenSBI 腿传 `DtbMemoryMap` 削出的 reserved（`fw_end = 0` 时为空）；两个直核面同 OpenSBI（aarch64 的 `fw_end = 0` 亦为空）。

| 字段 | 谁填 | 值从哪来 |
|---|---|---|
| `memmap` | builder 调用方 | UEFI：`UefiMemoryMap::collect()` 的 conventional 列表；OpenSBI：`DtbMemoryMap::collect(dtb_ptr())`（a1 DTB 的 `/memory`，I-22） |
| `reserved_regions` | 调用方 | UEFI：快照的非 conventional 列表；OpenSBI/直核：`DtbMemoryMap` 的 reserved（`fw_end = 0` 时为空） |
| `kern_virt_base` / `kern_phys_base` / `kern_size` | 装载函数 | ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)` |
| `free_upper_idx` | builder | 固定 `None`（见 §3.9） |
| `user_sp` | builder | `minix_types::USER_STACK_TOP` 单一权威常量 |
| `kern_stack_top` | builder 派生 | `kern_virt_base + kern_size`（内核镜像末尾即栈顶） |
| `syscall_entry` | builder 派生 | `kern_virt_base`（入口相对基址的偏移由架构侧决定） |
| `boot_modules` | 装载函数 | 按 `MODULE_NAMES` 顺序的 12 项 |
| `bootstrap_start` / `bootstrap_len` | 调用方 | 固定 `PhysBytes(0)` / `0`（见 §3.5） |
| `platform_sources` | 调用方 | UEFI：Configuration Table 扫描结果；OpenSBI：`[DTB]` 或空 |
| `param_buf` | builder | 固定 `&[]`（加载选项解析未接线） |

`boot_validate_and_prepare` 是这份结构在内核侧的第一次被读，它在建表之前先做校验：

```rust
// 节选（保留校验段；尺寸阶梯与诊断路标宏略去——阶梯的代码与解码在 §3.6；注释省略，尾部兜底注册块以省略号代替），os/kernel/src/lib.rs:fn boot_validate_and_prepare。
    let kv = kernel_info.kern_virt_base().0;
    let kp = kernel_info.kern_phys_base().0;
    let ks = kernel_info.kern_size();
    let huge = P::HUGE_PAGE_SIZE;
    let fallback = P::FALLBACK_HUGE_PAGE_SIZE;

    assert!(ks > 0, "arch_boot: kern_size must be > 0");
    assert!(kp.is_multiple_of(0x1000), "arch_boot: kern_phys_base must be page-aligned");
    assert!(kv.is_multiple_of(0x1000), "arch_boot: kern_virt_base must be page-aligned");
    // （尺寸阶梯的代码与解码在 §3.6，这里不重复。）
    ...
    assert!(ks.is_multiple_of(kern_huge),
        "arch_boot: kern_size must be a multiple of the chosen huge page size");
    assert!(kernel_info.kern_stack_top().0.is_multiple_of(16),
        "arch_boot: kern_stack_top must be 16-byte aligned");
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

**去掉 `bootstrap_len == 0` 断言会怎样**：只保留文档里的说明。字段非零时不会 panic，`kmain` 会拿一份含"待归还区间"的账继续走下去——故障链与 §3.5 的"误设后果"表逐行相同，差别只在"字段被误设"在现场没有任何痕迹。断言的价值是把这条信息从"文档里的约定"变成"入口处的停止"。

### 4.5 内核侧第一步：`store_kernel_info` 的五块落地区（→3.4）

`arch_boot` 的早期动作是把按值传入的结构深拷进内核自己的 `.bss`（`lib.rs` 的 [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)），然后只使用这份副本。

```rust
// 节选（保留五个落地区各自的核心语句；省略号处为注释与相邻块的接续行），
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
        let pstore = &mut *crate::globals::PLATFORM_SOURCE_STORE.get();
        let psrc = kernel_info.platform_sources;
        assert!(
            psrc.len() <= pstore.len(),
            "store_kernel_info: platform_sources exceeds the .bss landing pad"
        );
        if psrc.as_ptr() != pstore.as_ptr() {
            core::ptr::copy(psrc.as_ptr(), pstore.as_mut_ptr(), psrc.len());
        }
        let param = &mut *crate::globals::PARAM_BUF_STORE.get();
        let csrc = kernel_info.param_buf;
        assert!(
            csrc.len() <= param.len(),
            "store_kernel_info: param_buf exceeds the .bss landing pad"
        );
        if csrc.as_ptr() != param.as_ptr() {
            param.fill(0);
            param[..csrc.len()].copy_from_slice(csrc);
        }
        let mut k = *kernel_info;
        k.reserved_regions = core::slice::from_raw_parts(store.as_ptr(), src.len());
        k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
        k.boot_modules = core::slice::from_raw_parts(bstore.as_ptr(), bsrc.len());
        k.platform_sources = core::slice::from_raw_parts(pstore.as_ptr(), psrc.len());
        k.param_buf = core::slice::from_raw_parts(param.as_ptr(), csrc.len());
        *KERNEL_INFO.get() = Some(k);
    }
}
```

五块落地区的容量与溢出行为（另配一个名字字节池）：

| 落地区 | 容量常量 | 值 | 溢出行为 |
|---|---|---|---|
| `RESERVED_REGION_STORE` | `RESERVED_REGION_STORE_LEN` | 256 | panic |
| `MEMMAP_REGION_STORE` | `MEMMAP_REGION_STORE_LEN` | `crate::memmap::MAXMEMMAP` | panic |
| `BOOT_MODULE_STORE` | `minix_boot::NR_BOOT_MODULES` | 12 | panic |
| `PLATFORM_SOURCE_STORE` | `PLATFORM_SOURCE_STORE_LEN` | 4（真实上限 2 × 有意余量） | panic |
| `PARAM_BUF_STORE` | `PARAM_BUF_STORE_LEN` | 1024（C 的 `MULTIBOOT_PARAM_BUF_SIZE`） | panic（先清零再复制） |
| `BOOT_MODULE_NAME_STORE`（名字字节池） | `BOOT_MODULE_NAME_LEN` | 16（每槽，复制时截到 15 字节） | 截断（不 panic） |

名字池是唯一允许截断的：`n = bytes.len().min(BOOT_MODULE_NAME_LEN - 1)`（16 - 1 = 15），保留一个字节给 NUL 语义上的余量。截断后用 `core::str::from_utf8_unchecked` 造 `&'static str`，安全性论证是"模块名是 ASCII 字面量，15 字节的截断不会切开多字节序列"——这条论证的成立依赖 `MODULE_NAMES` 的内容，而不是类型系统。

`if src.as_ptr() != store.as_ptr()` 这个条件不是优化：`kmain` 会在拿到全局副本之后再调一次 `store_kernel_info`（幂等），此时源就是落地区本身，跳过拷贝可以避免 `ptr::copy` 在同址范围内的行为问题（注释里说明这也是用 `ptr::copy` 而非 `copy_nonoverlapping` 的原因）。

三个容量各有来由：`RESERVED_REGION_STORE_LEN = 256` 与 `vm_handoff` 的候选上限共用同一个常量（单一权威，避免两处 256 手工对齐漂移）；`MEMMAP_REGION_STORE_LEN` 的注释记着一次真机故障的探针读数（读数与归因限定见 §3.4 的"不做深拷…会怎样"，实现侧增量见本节末）；`BOOT_MODULE_NAME_LEN = 16` 与 `vm_handoff::copy_name` 的 16 字节 `proc_name` 字段同宽，落地不会比既有长度契约更紧。另外两个（D-70）：`PLATFORM_SOURCE_STORE_LEN = 4` 是"一个 DTB/ACPI 源加一个回退"的真实上限再乘二；`PARAM_BUF_STORE_LEN = 1024` 直接对齐 C 的定长语义，复制前先清零，短缓冲留下的尾部因此是全零而不是上一轮内容。

**只做一半深拷会怎样**：`reserved_regions` 深拷进去，但 `memmap` 与 `boot_modules` 保持原指针。现象就是 §3.4 那组真机读数的后半段（Step 4 建立 Direct Map 覆盖时读 `memmap` 还正常，到 `vm_handoff::classify` 再读时全部是零长度条目）；零化若更早发生（A2 分类之前），实现侧的增量是：`kmain` 的 A2 分类用 `if region.len > 0` 过滤拷进 `FREE_MEMMAP`，零条目被静默跳过，于是 `FREE_MEMMAP` 变成空记录——第一次物理页分配失败时，现场离"三个切片里有两个没落地"隔着整个 Phase A。

### 4.6 自举根的建立与开启（→3.6、3.8）

Step 1 与 Step 2 是两段映射循环（aarch64 上还有一段设备窗口专用的 Step 2b，见下文），它们的范围判据不同：

```rust
// 节选（保留循环体与 aarch64 分片的 cfg 块；注释、#[allow] 属性与两行编译期断言段有删节），
// os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable。
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut addr: u64 = 0;
    while addr < IDENTITY_MAP_END {
        let block_end = addr + kern_huge;
        let mut piece_addr = addr;
        #[cfg(target_arch = "aarch64")]
        let piece_size = {
            use minix_platform::arch::aarch64::{MMIO_PA_BASE, MMIO_WINDOW_LEN};
            const DEV_BLOCK: u64 = 1 << 21; // 2 MiB
            let dev_hi = MMIO_PA_BASE + MMIO_WINDOW_LEN;
            if addr < dev_hi && MMIO_PA_BASE < block_end {
                DEV_BLOCK
            } else {
                kern_huge
            }
        };
        #[cfg(not(target_arch = "aarch64"))]
        let piece_size = kern_huge;
        while piece_addr < block_end {
            #[cfg(target_arch = "aarch64")]
            let piece_flags = {
                use minix_platform::arch::aarch64::{MMIO_PA_BASE, MMIO_WINDOW_LEN};
                if piece_addr >= MMIO_PA_BASE && piece_addr < MMIO_PA_BASE + MMIO_WINDOW_LEN {
                    PageFlags::kernel_read_write() | PageFlags::DEVICE
                } else {
                    id_flags
                }
            };
            #[cfg(not(target_arch = "aarch64"))]
            let piece_flags = id_flags;
            // Ignore errors — some ranges may not be backed by physical memory,
            // but that's fine; the CPU won't access them.
            let _ = paging.map_huge(
                VirBytes(piece_addr),
                PhysBytes(piece_addr),
                piece_size as usize,
                piece_flags,
            );
            piece_addr += piece_size;
        }
        addr = block_end;
    }
```

```rust
// 代码引用（保留守卫与循环体），同函数 Step 2。
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

Step 1 的注释给出两条判据（另有一处 aarch64 专有的分片规则）：

- 范围是 `IDENTITY_MAP_END`（唯一权威在 [`minix-arch`](os/arch/src/lib.rs) 的 `BOOT_IDENTITY_MAP_END`，值就是低 4GB；kernel 侧是别名 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END`，boot-shim 侧 §4.2 的分配上界直接用同一常量），不是"可用内存的并集"。理由写在 Step 1 之前的注释里：UEFI 的加载器代码/数据可能落在 `LOADER_DATA`/`LOADER_CODE` 类型的内存里，它们不是 conventional；不覆盖这些区间，切页表后的下一条取指就取不到码。
- 标志是 `kernel_read_write() | EXECUTABLE`（supervisor-only + 可执行），不是"用户可访问"。RISC-V Sv39 上，S 模式取指一条 `U=1` 的页是被禁止的——`sstatus.SUM` 只放宽 supervisor 对用户页的 load/store，取指不受它影响——所以恒等映射必须是 supervisor-only。（源码在同一处的注释写成"除非 `sstatus.SUM=1`"，与规范相反；规范原文是 `Irrespective of SUM, the supervisor may not execute code on pages with U=1`，这里按规范陈述。）C 的 `pg_identity` 反而置了 `I386_VM_USER`：boot 阶段内核跑在 Ring 0，U/S 位不参与内核自己的访问判定，那个位在 C 里没有实际作用（有意义的对照是 `pg_mapkernel` 不置它，把内核地址标成 supervisor-only），Rust 侧因此不再置它。缓存属性是另一处与 C 的差异：C 对内存上界之上的条目置 `PWT|PCD`（设备/空洞不走缓存），64 位侧的恒等映射不逐条目区分（统一 `id_flags`）；内核运行期真正会访问的固定 MMIO 页（LAPIC/IO-APIC）改由 `minix_arch::kernel_identity_mmio_regions` 以固定区间登记（fix27），不在这一步处理。
- 循环里 `let _ = paging.map_huge(...)` 忽略错误：恒等映射铺满低 4GB，没有物理内存支撑的区间 CPU 不会去访问，失败因此可以接受（源码注释如此，该处带 `[SD19-gate]` 标记）。按机制说，这里实际可能失败的类别是页表页分配与槽位已是大页（`AlreadyMapped`）——`map_huge` 本身不检查物理内存是否存在，忽略的射程以此为准。
- aarch64 上循环多一层：设备 MMIO 窗口所在的那一块按 2 MiB 分片，落在窗口内的分片改置 `DEVICE` 标志——MMIO 寄存器要求强序访问（不投机、不合并、不重排），普通可缓存映射在 QEMU 上能过、真机上不行；注释同时要求这里的分片与 Step 2b 的高半别名对同一物理页使用同一套属性（ARM 手册：同一物理页用两种属性映射，结果不可预测）。其余架构上 `piece_size == kern_huge`、`piece_flags == id_flags`。

Step 2 的守卫 `if kern_virt != kern_phys` 记录在注释里：`kern_virt == kern_phys` 时 Step 2 与 Step 1 是同一个映射，跳过它可以避免在 riscv64 上把同一个 VPN[2] 槽用不同的页尺寸写两次，破坏恒等映射。

aarch64 上还有一段 Step 2b：把设备 MMIO 窗口按 2 MiB 映射到内核高半（`MMIO_VA_BASE`）。低半那份恒等映射会随第一次 `switch_address_space` 换掉 TTBR0 而消失，而 TTBR1 钉在自举根上、不参与切换——串口与 GIC 需要一个高半别名才能在切换之后继续可用（Step 3 末尾的 `use_high_mmio_window()` 配套它；属性与 Step 1 的恒等别名一致，理由同上一条）。

Step 3 的 `enable()` 在 x86-64 上做四件事：

```rust
// 节选（保留与硬件状态顺序相关的行；注释有删节，`asm!` 块按等价的单行写法重排），os/arch/src/x86_64/paging.rs:X86_64Paging::enable。
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

顺序是：置 `EFER.NXE` → 写 `CR3` → 置 `CR0.PG` 与 `CR0.WP` → 按能力置 `CR4.PGE`。（注意 x86_64 的 UEFI 链进来时固件分页已在运行：这一步实际生效的动作是"换 `CR3` 根"，`CR0.PG` 的置位是与 BIOS 链的统一写法；`NXE` 必须最先的理由见下一条。）

- `EFER.NXE` 必须最先：`NX` 是页表条目里的最高位（bit 63），它的使能开关在 `EFER` 这个 MSR 的 `NXE` 位（bit 11）上；`EFER.NXE = 0` 时任何带 NX 的页表条目在走表时被视为保留位，触发页错误且错误码 bit3（保留位）为 1。内核的 Direct Map 叶与用户数据/栈叶都带 NX，所以准确的要求是"第一条带 NX 的条目被使用之前必须先置 NXE"（放进 `enable` 即可满足）。注释指出 UEFI 交接时恰好置了它，但这条不变量被当作"我们自己的"，不依赖固件。
- `CR3` 写的是内核自己的根：`build_bootstrap_root_and_enable` 返回前会把根页表物理地址记进内核全局（`set_current_root_phys(root_page)`），后续阶段（如装载 VM 的 ELF 时给同一张表加映射）通过 `Paging::from_active_root` 取回一个句柄。`from_active_root` 与 `new_from_page` 的区别是后者会清零根（`write_bytes(ptr, 0, 512)`），前者不会。
- `PGE` 与 C 的 `vm_enable_paging` 对应（2.6 的 `pgeok`）：只在 CPU 支持时置，且必须在 `PG` 之后。

五步与 C 的一一对应及差异，见 §4.9 的函数对照表（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五行），这里不重复。表外还有一段 aarch64 专有的 Step 2b（设备 MMIO 窗口的高半别名，见上）——C 没有对应物：x86 的串口是端口 I/O，ARM 侧是纯 MMIO、必须走内存映射。

**若恒等映射只覆盖可用内存、或地址对称时照做第二步**（两处）：

其一，恒等映射只覆盖 `memmap` 里报告的可用内存。UEFI 的加载器代码与数据可以落在 `LOADER_CODE`/`LOADER_DATA` 这类非 conventional 内存里；`enable()` 写完 `CR3` 的下一条取指就在这类内存中，而页表里没有它——页错误发生在 `enable()` 内部，此时内核还没有任何能打印故障的设施（控制台初始化在 `kmain` 里），现场是"开启分页后立刻静默停止"。

其二，`kern_virt == kern_phys` 时照做 Step 2。Step 1 已经把这个地址范围用 `kern_huge` 建好；第二次用同样的尺寸与标志再写一遍，在 x86-64 上可能只是重复写同一个值，但在 riscv64 上，"同一个 VPN[2] 槽位写两次"意味着第二次的叶尺寸可能与第一次不同（1GiB 与 2MiB 梯度的差别）——一条 1GiB 叶与一条"指向下级页表"的条目互斥，先写的那一条会被覆盖，恒等映射覆盖范围随之改变，跳转到内核高地址之后启动信息里记录的 `kern_phys_base` 仍然可用而镜像的一部分不再可访问。

### 4.7 分配器换手的落地（→3.8）

三处写者与各自的自守卫在 3.8 已列出。这里补上"游标"的两种恢复方式与扣除表的读法。

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
    /// kernel image would re-hand-out pages the boot-shim's page tables already
    /// occupy. `next` is the boot-shim's post-build cursor;
    /// the caller guarantees `base <= next <= end`.
    pub fn resume(&self, base: u64, next: u64, end: u64) { ... }
```

- `init(base, end)` 把游标置回 `base`：两个调用者都用它——boot-shim 在进入内核之前设定区间（x86_64 在 `prepare_boot` 返回后，aarch64 在 `prepare_and_enter_kernel_image` 内）；内核在 Step 0 发现 `boot_alloc_region()` 为 `None` 时用它从 `memmap` 里切一段兜底区间（`FALLBACK_BUMP_LEN = 0x100_000`，256 页）。
- `resume(base, next, end)` 从"已被消耗到的位置"继续：跨镜像路径必须用这一种，因为接收侧链接的是这份分配器的另一份副本（`resume` 的文档注释写明了这一点）。aarch64 的 `arch_boot_resume_v1` 在解码信封后、进入 `arch_boot_resume_high_half` 之前调用它。

三种方式的次序约束：跨镜像路径上必须先 `resume_boot_pt_alloc`、再走 `arch_boot_resume_high_half`。顺序反了的话，Step 0 会看到 `boot_alloc_region()` 为 `None` 而走兜底 `init`（兜底判据是它，不是 `is_registered()`），把游标重置回某段 memmap 的起点——boot-shim 已经发出的页表页会被再发一次。

扣除表的读法：`build_vm_handoff` 把"内核保留"与"引导已用"两类区间收进同一份记录（见 3.8 引用的表），其中 bump 区是**整段**；`VmBootAllocator` 的已用后缀（该分配器从每个区间末端向下消耗，已用部分是连续尾段——表里第二类）是同段内的另一笔。两份记录随交接页一起交给 VM，VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`——也就是说，"内核记账对不对"这件事不是靠 kernel 单方面声明，而是在 VM 侧再复核一次。

**去掉内核侧 `is_registered()` 守卫会怎样**：让 `boot_validate_and_prepare` 无条件 `register(boot_alloc::boot_pt_alloc)`。`register` 里的 `debug_assert!( !PT_REGISTERED.load(...) )` 只在 debug 构建生效；release 构建下第二处 `register` 会直接改写函数指针。若第一个注册者是某个测试内核（它用了一个避开内核镜像的池），改写之后所有页表页都从 `boot_alloc` 的兜底区间发出——那段区间由 `memmap` 的第一个满足条件的区域切出 `0x100_000` 字节，而 `boot_alloc_used_bytes()` 统计的是修改后的实例，于是 VM 交接记录里的 `kernel_allocated_dynamic` 与真实用量不一致，多出来的池子不在任何人的账上。

### 4.8 跨镜像交接与直核自引导（→3.7）

三个架构的交接形态并列。boot-shim 侧的分道代码（`main.rs` 里那两行 `cfg` 分支）已在 §4.1 逐字引过，这里不重复；先从 aarch64 的接收面看起：

```rust
// 节选（保留分道与 resume 调用；注释改述为机制陈述），os/kernel-image/src/main.rs:fn rust_image_main（aarch64）。
#[cfg(target_arch = "aarch64")]
fn rust_image_main(arena_pa: usize) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    // 两条链的分道：直核 `-kernel`（QEMU 不设参，x0=0）走自引导面；
    // boot-shim UEFI 链 x0=arena 物理地址走 v1 解码 + 高半 resume 面。
    if arena_pa == 0 {
        bootface_a64::bootface_a64();
    }
    early_console::use_high_mmio_window();
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}
```

（`bootface_a64()` 的返回类型是 `!`——自引导面正常不会返回；`arena_pa == 0` 是唯一走直核的取值，v1 链不会传 0。）

```rust
// 代码引用（保留入口的三行与调用），os/kernel-image/src/main.rs:fn rust_image_main（riscv64）。
#[cfg(target_arch = "riscv64")]
fn rust_image_main(boot_hart: u64, dtb_phys: u64, boot_file_table_pa: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys, boot_file_table_pa)
}
```

| 架构 | 谁建根表 | 谁开分页 | 交接值 | 内核入口 |
|---|---|---|---|---|
| x86_64 | boot-shim（内联内核） | boot-shim（`arch_boot` → `enable`） | `&KernelInfo` + 根页物理地址（寄存器/参数） | 同镜像内 `arch_boot` |
| aarch64 | boot-shim | boot-shim（`enter_kernel_image_v1` 内的 `enable`） | `BootHandoffV1` arena 物理地址（`x0`） | 独立镜像高半 `_start` |
| riscv64 | 内核镜像自身（`bootface`） | 内核镜像自身（`arch_boot` → `enable`） | `a0`=hartid、`a1`=DTB、`a2`=`BootFileTable`（可选） | 独立镜像 `_start` |

三条链收敛在同一个 `arch_boot`/`arch_boot_impl`（跨镜像路径收敛在 `arch_boot_resume_v1` → `arch_boot_resume_high_half`，后者调用同一个 Step 4 与同一个 `jump_to_kmain`）。`arch_boot_resume_v1` 里的 `resume_boot_pt_alloc` 只是 `BootAlloc::resume` 的转发（§4.7 的三种进入方式之一），注册不发生在它里面。

`enter_kernel_image_v1` 里跳转前的准备（建根、读游标、补运行期字段）与 `br {entry}` 的写法在 3.7 已给出。这里补一条与"谁建根"有关的约束：跨镜像路径下，`build_bootstrap_root_and_enable` 被 boot-shim 调用一次、生成的表**已经激活**；接收侧必须避免二次建表（3.7 的反向推演已给后果），而 `bootface` 路径下建表发生在内核镜像自身，`arch_boot` 走的是常规 Step 1–4。

两处实现现状：

- x86_64 的独立镜像入口（[`main.rs`](os/kernel-image/src/main.rs) 的 `rust_image_main`）当前仍是"打印一行提示后驻留"的形态——把内核链接成独立 ELF 的产出与布局校验已经存在（`check-layout.sh` 的 L0–L9，清单见 §5.2），而 boot-shim → 该镜像的跳转协议已由 OQ-N6 裁决（每进程独立根原语，aarch64 的 `BootHandoffV1` arena 交接即其实现），x86 腿的实施收口 = NK1（C-29）待认领（镜像头注同步）。当前 x86_64 的生产路径是内联执行。
- riscv64 在 QEMU `-kernel` 直核场景下镜像按物理基址视图执行（源码注释把这一实测事实记作 `medany`），此时 `kern_virt` 与 `kern_phys` 可能相等，§4.6 的 Step 2 守卫就是为这种情况写的；生产链（`bootface` + 文件系统 + 高半链接）走的是另一套布局。两条布局的基址常量以链接脚本为准，复算命令：`grep -n 'KERNEL_VIRT_BASE\|KERNEL_PHYS_BASE\|ENTRY' os/kernel-image/*.ld`（该命令对三份脚本各命中若干行，这里只摘 riscv64 的三个值： `KERNEL_VIRT_BASE = 0xFFFFFFC000000000`、`KERNEL_PHYS_BASE = 0x80200000`、`ENTRY(_start)`）。

**让 riscv64 照 aarch64 那样"先建表、再跳镜像"会怎样**。aarch64 的镜像入口之所以直核与 UEFI 两种场景都能用，靠的是入口里的一条分道：`rust_image_main(handoff)` 在 `handoff.is_null()` 时走自引导面（`-kernel` 直核下 QEMU 不设 `x0`）、非空时走高半 resume 面。riscv64 的入口（`rust_image_main(boot_hart, dtb_phys, boot_file_table_pa)`）没有"交接指针"这个入参——直核场景只给 hartid 与 DTB；若照搬 aarch64 的协议而不补一条分道，空指针解引用会发生在 `early_console` 初始化之前，现场没有任何输出。反过来，若 aarch64 退回内联执行，撞的是 §3.7 第一处替代做法已经推演过的同一个问题（TTBR0 被换掉、取指映射作废），现场证据（`elr`/`vbar` 钉在同一低地址）也在那里。

### 4.9 与 Minix3 C 的函数对照（→3.1–3.9）

| C 函数（第 2 章） | Rust 落点 | 保留 / 改型 / 淘汰 / 新增 | 差异原因一句话 |
|---|---|---|---|
| [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) | [`fn main`](os/boot-shim/src/main.rs:fn main) | 改型 | 引导器跳转到内核入口 → 固件跳转到 boot-shim 入口 |
| `pre_init(magic, ebx)` | `os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs:impl BootShim` | 改型 | 六条语句被拆成两条路径的 `prepare_boot` |
| `get_parameters(ebx, cbi)` | `os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs:fn prepare_boot` + `build_memmaps`/`build_kernel_info` | 改型 | 拷一份引导器结构体 → 调固件接口拿两份内存列表 |
| `mb_set_param(...)` | [`param_buf`](os/libs/minix-boot/src/kernel_info.rs:param_buf)（空切片 + 校验上限） | 改型 | 参数表暂未接线；字段与 1024 上限保留 |
| `add_memmap(cbi, addr, len)` | [`fn build_memmaps`](os/boot-shim/src/uefi_helpers.rs:fn build_memmaps) + [`memmap.rs`](os/kernel/src/memmap.rs) | 改型 | 构建侧改为一次快照分流；内核侧仍按区间记账 |
| `cut_memmap(cbi, start, end)` | [`fn cut_memmap`](os/kernel/src/memmap.rs:fn cut_memmap)（由 `kmain` A.2 调） | 改型 | 引导侧报告全部 DRAM，切除移到内核侧；第三个参数从"结束地址"改成**长度**（`memmap.rs:fn cut_memmap` 收 `start, len`），与同族的 `add_memmap` 统一——C 里两者不一致，§2.4 那处 `alloc_lowest` 传错参数就出在这个差异上 |
| `alloc_lowest(cbi, len)` | 当前无对应物；低地址代码的拷入端已有（[`fn install_at`](os/arch/src/x86_64/ap_early_entry.rs:fn install_at)，地址由调用者给） | 暂未移植 | 唯一的消费者是 32 位 AP 启动跳板（SMP 归 16 篇）；`install_at` 的注释写明扫描空闲表找最低区间的动作归 SMP 篇的 `smp_init`，**重做时不得复刻 §2.4 那处"把长度当结束地址"的误用** |
| `overlaps(mod, n, cmp_mod)` | [`fn first_overlapping_pair`](os/boot-shim/src/loader.rs:fn first_overlapping_pair) | 改型 | 检查对象从"模块 + 内核"变成"内核镜像自身的 PT_LOAD 段" |
| `pg_clear()` | [`X86_64Paging::new_from_page`](os/arch/src/x86_64/paging.rs:X86_64Paging::new_from_page) 的清零 | 改型 | 根页由调用者提供 |
| `pg_identity(&kinfo)` | [`build_bootstrap_root_and_enable`](os/kernel/src/lib.rs:build_bootstrap_root_and_enable) Step 1 | 改型 | 范围与页尺寸由常量与阶梯决定 |
| `pg_mapkernel()` | 同函数 Step 2 | 改型 | 返回值不再被使用 |
| `pg_load()` | [`X86_64Paging::enable`](os/arch/src/x86_64/paging.rs:X86_64Paging::enable) 的 `mov cr3` | 保留（同一动作） | 硬件动作不变 |
| `vm_enable_paging()` | 同函数 `enable` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 改型 | 多了 NXE；少了 `PSE` |
| `pg_alloc_page(cbi)` | [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:BootAlloc::alloc) | 改型 | 从记录尾部取页 → 从注册的区间取页 |
| `alloc_pagetable(&ph)` | [`fn alloc_pt_page`](os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page) + 三架构实现 | 改型 | 6 页静态池 → 注册式分配器 |
| `pg_map(phys, vaddr, vaddr_end, cbi)` | [`X86_64Paging::map`](os/arch/src/x86_64/paging.rs:X86_64Paging::map)（4KB 粒度） | 保留（语义） | 4KB 细粒度映射的用途不变 |

这张表里 `alloc_lowest` 一行标的是"暂未移植"，它也不是"因为不需要所以删掉"：`alloc_lowest` 的服务对象（把 16 位启动代码放到 1MB 以下）属于 AP 启动，而 64 位侧的对应机制（AP 停车邮箱 + 启动记录）在 [`smp.rs`](os/kernel/src/smp.rs) 与 [`main.rs`](os/kernel-image/src/main.rs) 的 `park` 段里，不经过"从记录最低处分配"这条路径。

**把 C 的静态页表池原样搬到 64 位会怎样**（保留"6 页，用完 panic"）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 一次最多挂一个页表页，6 页够用。4 级页表下一次 4KB 映射要穿过 PML4 → PDPT → PD → PT 四级，其中下面三级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页会在第一次跨 PML4 边界的映射上耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

---

## 5. 测试：先推导需要证明什么，再看覆盖（独立评测方视角）

这一章的写法与前几章相反：不从现有测试出发，而是**从本篇讲过的机制出发**，逐条推导"要证明它成立，需要什么可观测、可证伪的断言"，然后标注覆盖状态。未覆盖项以 **TODO** 形式登记——它们是对源码工作流的**需求输入**（唯一登记处是 5.3 的表）。

三类证据来源：**宿主断言**（镜像还没运行就能查）、**宿主测试**（纯逻辑与接口契约）、**真机链**（QEMU 串口日志断言）。

### 5.1 机制 → 需要证明什么（推导表）

| # | 机制（正文位置） | 需要证明什么（可观测、可证伪） | 证据类型 | 覆盖状态 | TODO |
|---|---|---|---|---|---|
| M1 | 第一跳设栈与传参（§2.2，C 侧） | 内核入口 esp 指向 `load_stack_start`；`pre_init` 收到 `(magic, ebx)` | 引文对账＋真机 | 引文块逐字对账＋真机链兜底；C 侧无执行断言（不在本仓测试面） | — |
| M2 | 内存清单的建与裁（§2.4，Rust 等价物） | `add_memmap` 只收页对齐、非零、<4GB 区间；`cut_memmap` 起点向下、终点向上取整 | 宿主测试 | **已覆盖**：[`memmap.rs`](os/kernel/src/memmap.rs) 17 条（含 `test_cut_memmap_alignment_round_down_start` / `round_up_end` / `preserves_total_free_memory` / aarch64 DTB 真实 totalsize 记账） | — |
| M3 | 清单先于建表（§1.2/§2.8，Rust 侧） | 正常注册路径下 Step 0/1 不向清单领页（兜底路径除外：`boot_alloc_region()` 为 `None` 时从清单切 256 页，见 §4.1——它发生在建表之前，不违反"清单先于建表"）；清单的动态取用发生在 `pre_init` 等价的 4KB 路径 | 真机 | 真机链覆盖（`kernel: step1+2 mappings ok` 路标） | — |
| M4 | **深拷先于一切动用空闲清单者**（§4.5 不变量） | 三个入口在 `store_kernel_info` 之前不发生页分配、不读空闲清单；交接分类点读 `KernelInfo` 切片非零 | 真机＋调用序证据 | 宿主测试 `test_store_kernel_info_lands_firmware_slices`（落地区行为）+ 调用序钉 `os/kernel/tests/store_before_alloc_pin.rs`（四入口源码序：store 先于 `arch_boot_impl`/`establish_boot_dm`）。运行期钩不可行：QEMU 测试内核直接调 `arch_boot_impl` 分阶段验证，无法区分生产入口与测试 | — |
| M5 | 页表建立与开分页（§2.6 C / §4.6 Rust） | 开分页后的下一条取指有映射（能打印第一个内核路标） | 真机 | 真机链覆盖 | **TODO-05**（见 5.3） |
| M6 | 恒等映射覆盖低 4GB 固定上界（§4.6 Step 1） | `IDENTITY_MAP_END` 以下全部有映射、标志为 supervisor-only | 宿主测试 | **已覆盖**：[`paging.rs`](os/arch/src/x86_64/paging.rs) 15 条（`map_huge` 系列） | — |
| M7 | 分配器"写一次"（§3.8） | 二次 `register` 在 debug 构建失败、release 构建拒绝改写 | 宿主测试 | **已覆盖**：[`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条（`register` / `register_free` 各一：debug 下二次注册 panic，release 下被拒且指针不变） | — |
| M8 | 落地区容量与 fail-fast（§4.5） | 超过容量时快速失败而非截断 | 宿主测试 | **已覆盖**：落地区行为（`test_store_kernel_info_lands_firmware_slices`）+ 五枚超容量负控（`test_store_kernel_info_rejects_{reserved,memmap,boot_module,platform,param}_overflow`） | — |
| M9 | bump 分配器（§3.8/§4.2） | 顺序推进、耗尽拒绝、页对齐、reinit 后归位、跨镜像区间自洽 | 宿主测试 | **已覆盖**：[`boot_alloc.rs`](os/kernel/src/boot_alloc.rs) 10 条 | — |
| M10 | 布局契约（§5.2 与 §3.6） | L0–L9 十条主编号（L0–L8 全架构 + L9 仅 riscv64） | 宿主断言 | **已覆盖**：`check-layout.sh`（全量，见 5.2.2） | — |
| M11 | 模块顺序契约（§4.3） | 12 项按 `MODULE_NAMES` 顺序返回、缺件 panic | 宿主测试＋真机前置检查 | **已覆盖**：`loader.rs` 顺序/缺件测试＋真机链的 `mdir` 前置 | — |
| M12 | EBS 阶段边界（§4.1） | EBS 之后不再出现固件调用；裸串口路标仍输出 | 真机＋静态检查 | 真机覆盖（两条 `raw_serial` 路标）；文本级静态检查 `os/boot-shim/tests/ebs_boundary_audit.rs` 覆盖两条入口（`prepare_boot` 的 `exit_boot_services()` 之后、`prepare_and_enter_kernel_image` 的配对循环成功标记之后），零 `uefi::`/分配/释放/drop，带两枚注入负控与注释豁免自检 | — |
| M13 | UEFI 侧快照函数（§4.2：`build_memmaps`、`assert_bootstrap_outside_memmap` 等） | 两列表互斥完备；root/bump 不与 free 列表相交 | 宿主测试→真机 | **部分覆盖**：`assert_bootstrap_outside_memmap` 3 条宿主测试（相离/贴边通过、包含/跨界 panic）；`build_memmaps`/`alloc_root_page`/`alloc_bump_region` 调用点单根（Boot Services），宿主无法构造 | **TODO-07**（见 5.3） |
| M14 | 交接三形态（§3.7/§4.8） | aarch64 resume 只做 additive（不重建根）；`x0` 分道（空指针→自引导面）；x86 shim 三路标可见 | 真机＋引文 | 真机链覆盖（aarch64 UEFI、riscv64 两条）；x86 `test-cmd-smoke.sh` Stage 3b 现已断言三条 shim 路标（kernel loaded / 12 boot modules loaded / handing control） | — |
| M15 | 失败路径防线（§4.2 步骤 7） | root/bump 与 free 列表相交时立刻 panic（EBS 前、可打印） | 真机 | 断言在源码内、每次启动都执行 | — |

「待源码侧确认」的覆盖状态**不推测**——拿不准的写"待确认"，由源码工作流核实后回填。

### 5.2 覆盖现状：三层证据各自证明什么

#### 5.2.1 宿主测试层（纯逻辑与接口契约）

宿主测试跑的是纯逻辑与接口契约，真实启动链要 QEMU。这一层与真机层的分界线可以逐条列出。

可复制命令与实测输出关键行（下面两段是实际的命令与输出，预期输出用 `test result:` 行核对）：

```bash
$ cd os && cargo test -p boot-shim --features test-all --lib
...
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cd os && cargo test -p minix-boot -p minix-elf
running 32 tests
test result: ok. 32 passed; 0 failed; ...
running 23 tests
test result: ok. 23 passed; 0 failed; ...
```

测试数口径：本节所有数字按"`#[test]` 出现次数"计，含 `#[should_panic]` 标注的条目，不含构造辅助函数与 mock 类型。逐文件复算：

```bash
$ for f in os/boot-shim/src/loader.rs os/boot-shim/src/opensbi_helpers.rs \
           os/boot-shim/src/uefi_helpers.rs os/libs/minix-boot/src/kernel_info.rs \
           os/libs/minix-boot/src/platform.rs os/libs/minix-boot/src/boot_form.rs \
           os/libs/minix-boot/src/memmap_source.rs os/libs/minix-boot/src/handoff_v1.rs \
           os/libs/minix-boot/src/handoff_v1_write.rs os/libs/minix-boot/src/handoff_v1_read.rs \
           os/libs/minix-elf/src/lib.rs \
           os/boot-shim/src/ebs_map.rs os/boot-shim/src/ebs_pair.rs; do
  printf '%-50s %s\n' "$f" "$(grep -c '#\[test\]' $f)"; done
os/boot-shim/src/loader.rs                         15
os/boot-shim/src/opensbi_helpers.rs                12
os/boot-shim/src/uefi_helpers.rs                   4
os/boot-shim/src/ebs_map.rs                        12
os/boot-shim/src/ebs_pair.rs                       10
os/boot-shim/tests/ebs_boundary_audit.rs           3
os/libs/minix-boot/src/kernel_info.rs              9
os/libs/minix-boot/src/platform.rs                 8
os/libs/minix-boot/src/boot_form.rs                6
os/libs/minix-boot/src/memmap_source.rs            9
os/libs/minix-boot/src/handoff_v1.rs               3
os/libs/minix-boot/src/handoff_v1_write.rs         11
os/libs/minix-boot/src/handoff_v1_read.rs          11
os/libs/minix-elf/src/lib.rs                       23
```

15 + 12 + 4 + 12 + 10 = 53（`--lib`），另有集成测试 `tests/ebs_boundary_audit.rs` 5 条；与 `cargo test -p boot-shim --features test-all --lib` 的实测一致（默认 uefi-only 形态只编 loader + uefi + ebs_map + ebs_pair，53 这个读数必须带 `test-all`）；`minix-boot` 的 57 = `kernel_info.rs` 的 9 + `platform.rs` 的 8 + `boot_form.rs` 的 6 + `memmap_source.rs` 的 9 + `handoff_v1.rs` 的 3 + `handoff_v1_write.rs` 的 11 + `handoff_v1_read.rs` 的 11；`minix-elf` 的 23 与实测一致。

推导表（5.1）里引用的其余宿主测试层（kernel 侧，`cargo test -p minix-kernel --features mock` 形态）：[`memmap.rs`](os/kernel/src/memmap.rs) 17 条、[`boot_alloc.rs`](os/kernel/src/boot_alloc.rs) 10 条、[`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 7 条、[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 10 条、[`paging.rs`](os/arch/src/x86_64/paging.rs) 15 条、[`lib.rs`](os/kernel/src/lib.rs) 的 `store_kernel_info` 系列 4 条（1 枚落地区行为 + 3 枚超容量负控）。[`pt_alloc.rs`](os/arch/src/arch/pt_alloc.rs) 自身为零，其单例契约由集成测试 [`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条覆盖（单元测试二进制里 paging.rs 的 destroy 测试已占用同一全局单例，无法在其中再测注册）；v1 协议的布局门禁由 [`handoff_v1.rs`](os/libs/minix-boot/src/handoff_v1.rs) 的编译期断言与 3 条尺寸/偏移测试覆盖。

列出的测试各自挡的是哪条契约（选列，未逐条穷举）：

| 测试（文件名:函数名） | 挡的契约 |
|---|---|
| `loader.rs:test_load_boot_modules_loads_all_in_contract_order` | 12 项按 `MODULE_NAMES` 顺序返回（顺序契约） |
| `loader.rs:test_load_boot_modules_panics_on_missing` | 缺件必须 panic 而不是跳过（用 `catch_unwind` 观测） |
| `loader.rs:test_first_overlapping_pair_{disjoint,overlap,abutting,empty,returns_first}` | 半开区间的重叠判定（`abutting` 那一条正是 2.5 的 `-1` 语义） |
| `loader.rs:test_bss_region_is_zeroed_after_load` | 段余量清零到 `memsz`、且不越过 `memsz`（实现分析归 02 篇 §4.2，此处只登记它的存在） |
| `loader.rs:test_load_kernel_with_loader_computes_layout` | 布局派生（`min(vaddr)`、`min(paddr)`、`span`） |
| `opensbi_helpers.rs:test_bump_alloc_rejects_zero_pages` | `bump_alloc(0)` 必须拒绝（否则返回重复地址） |
| `opensbi_helpers.rs:test_bump_alloc_advances_pointer` | 连续分配严格递增 |
| `opensbi_helpers.rs:test_boot_file_table_validates_magic` | 魔数不匹配即拒（防 `a0` 指错） |
| `opensbi_helpers.rs:test_build_kernel_info_bootstrap_zero_means_no_reclaim` | `bootstrap_start/len` 保持 `(0, 0)`，且 `len != kern_phys_base` |
| `opensbi_helpers.rs:test_build_kernel_info_riscv64_fields_match_input` | 字段逐一映射（含派生字段 `kern_stack_top`/`syscall_entry`） |
| `uefi_helpers.rs:test_build_kernel_info_fields` | UEFI builder 的字段装配（该文件唯一的单元测试） |
| `kernel_info.rs:test_validate_rejects_*`（6 条）与 `test_validate_accepts_*`（2 条），另加 `test_getters_return_field_values`（1 条）= 该文件 9 条 | `validate()` 的六条不变量与它们的合法边界；getter 与字段一致 |
| `boot_form.rs:test_boot_form_count_is_accounted` 等 6 条 | 启动形态家族的记账（新增形态必须登记）与跨形态后置条件电池（对每种形态逐一断言 `check_boot_postconditions`） |

一处覆盖缺口（对应推导表 M13）：UEFI 侧四个函数里，纯判定函数 `assert_bootstrap_outside_memmap` 已有三案宿主测试（相离/贴边通过、包含/跨界 panic）；其余三个（`build_memmaps`、`alloc_root_page`、`alloc_bump_region`）的调用点单根（`main.rs` 的 [`fn main`](os/boot-shim/src/main.rs:fn main) → Boot Services），宿主无法构造；明细与去向见 5.3 的 TODO-07。

**把缺件处理改回静默跳过会怎样**：`load_boot_modules_with_loader` 里换成 `if let Some(data) = loader.read(&path) { ... }`。`test_load_boot_modules_panics_on_missing` 立刻变红——这条测试的可观测形式正是 panic，它挡的正是"错误被推迟到内核侧"那条路；那条链的完整走向（内核侧只剩"期望 12、实际 N"）在 §4.3 已经给过。

#### 5.2.2 宿主断言层（镜像还没运行就能查）

`check-layout.sh` 把"boot-shim 装载内核时要读的字段"在宿主上判定完。可复制命令与退出码：

```bash
$ bash os/kernel-image/check-layout.sh all      # 或 x86_64 / aarch64 / riscv64
$ echo $?
0     # 全部断言通过；1 = 任一断言失败；2 = 用法/环境错误
```

断言清单（脚本头部注释逐条列出 L0–L8；L9 的说明在实现处、只在 riscv64 分支执行）：

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

**去掉 L5（跨距对齐断言）会怎样**：一个跨距为 `2MiB + 4KiB` 的镜像在 `boot_validate_and_prepare` 里既进不了 1GiB 档也进不了 2MiB 档（`kern_size` 同时对两者都不整除——L5 检查的是**跨距**（全部 `PT_LOAD` 段的跨度），阶梯检查的是 `kern_size`（同一组段 `min(paddr)` 与 `max(paddr+memsz)` 之差）；无额外填充时二者相等，L5 因此保证阶梯至少落进 2MiB 档），落到 `panic!("... must be aligned to at least FALLBACK_HUGE_PAGE_SIZE")`。现场是"启动第一步就 panic"，根因在链接脚本的段间距；没有宿主断言就只能靠真机二分定位。

#### 5.2.3 真机层（三条链各自证明什么）

| 链 | 脚本 | 这条链证明什么 | 判定标记 |
|---|---|---|---|
| UEFI + ESP 盘（aarch64） | `os/qemu-tests/test-shim-bootmarks-aarch64.sh` | boot-shim 能自己从 ESP 读内核与 12 个模块（读文件这一半） | `boot-shim: kernel loaded (entry staged)` 与 `boot-shim: 12 boot modules loaded` 两条路标都出现——两者都在 `ExitBootServices` 之前，脚本不判内核随后是否存活（`panicked at` 只作提前收尾的快速失败，不是通过条件） |
| OpenSBI + U-Boot（riscv64 生产链） | `os/qemu-tests/test-riscv64-uboot.sh` | U-Boot 的 `fatload` → `bootelf` 交棒透明可用：与 OpenSBI 直载腿同一个测试内核（`hello-boot-*`，非生产内核）经 U-Boot 装载也能走到一致的 PASS（内核入口之后不依赖固件服务） | `### TEST_RESULT: PASS hello-boot-riscv64 ###`（由被装载的测试内核打印，脚本从串口日志匹配） |
| `-kernel` 直核（riscv64 自引导面） | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh` | 自引导面的两条输入通道可用：DTB 里的表址被解析、表被消费并 fail-fast（脚本故意让模块定位失败、不假装模块存在） | `### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###`——条件含表址行、`table holds 0x0000000000000000 entries` 与 `module placement failed` 的行序 |

三条链的跑法与"关键行缺失时怎么办"见 §6.4 第一层（本节只负责"证明什么"与通过标准）。`BootFileTable` 取件车道不在这条 U-Boot 链的测试内核里（它直构 `BootPrepareResult`、绕过 `BootShim`），覆盖它的是 OpenSBI 直载腿的脚本（`test-riscv64-boot-full.sh` 一类会在设备树上注入 `opensbi,boot-file-table`）。

退出码语义（表中三个脚本一致）：0 = PASS，1 = FAIL，2 = SKIP（前置工具缺失）。这条区分让"没跑"与"跑了但失败"在 CI 里可分辨：没有 U-Boot 的开发机上，riscv64 生产链返回 2 而不是 1。

脚本还顺带证明了模块数与路径：`test-shim-bootmarks-aarch64.sh` 在上盘之前会 `mdir` 检查 `/EFI/minix` 下有 `kernel.elf` 且 12 个模块名逐个存在，任一条缺失就在上盘阶段失败——也就是说"12 个模块文件必须在盘上"这件事在真机链的前置检查里被显式验证。x86_64 侧的对等物是 `test-cmd-smoke.sh` 的 ESP 前置检查（同样的 `mdir` 三查），但它的串口断言只到"进入调度器"，不判 shim 侧各路标（推导表 M14 的 TODO-06 即指此）。

**把 SKIP 当作 FAIL 处理会怎样**。没有 U-Boot 与 QEMU 的开发机上这条链长期红，随后人的处理方式是"忽略这条输出"；真正切断 `fatload` 的那次改动会淹没在常态红里，而这条链恰好是唯一能证明"U-Boot 交棒（fatload → bootelf）下同一个内核照样跑通"的判据。

### 5.3 TODO 交接表（对源码工作流的需求输入）

| TODO ID | 机制 | 断言（可观测） | 为什么现在没覆盖 | 交接状态 |
|---|---|---|---|---|
| TODO-01 | M4 深拷序 | 三入口在首个页分配前已完成 `store_kernel_info`（调试钩检查点） | 已覆盖：源码序钉（四入口）；mock 入口 `arch_boot_test` 与 QEMU 测试内核的分阶段调用均绕过 store，故运行期断言不成立（钉注释已写明这次回退的判据） | 已交接完成 |
| TODO-02 | M7 写一次 | `register` 二次调用在 debug 下失败、release 下被拒 | 已覆盖：[`pt_alloc_write_once.rs`](os/arch/tests/pt_alloc_write_once.rs) 2 条（`register` 与 `register_free`），debug/release 两形态实跑 | 已交接完成 |
| TODO-03 | M8 落地区溢出 | 超容量输入触发 fail-fast（不截断） | 已覆盖：五枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025），各持 boot-globals 锁 | 已交接完成 |
| TODO-04 | M12 EBS 边界 | EBS 之后的代码路径中不出现 `uefi::` 调用 | 已覆盖（文本级）：`os/boot-shim/tests/ebs_boundary_audit.rs` 切出 `prepare_boot` 体与 `prepare_and_enter_kernel_image` 体，去注释后扫 EBS 边界之后的语句（零 `uefi::`/`uefi::boot::`/allocate/free/Box/Vec/alloc/drop），带两枚注入负控与注释豁免自检 | 已交接完成（文本级） |
| TODO-05 | §4.6 大页回退档 | 2MiB 回退档下的真机启动 | 真机矩阵不全 | 已登记待交接 |
| TODO-06 | M14 x86_64 路标链 | x86_64 上 `kernel loaded` / `12 boot modules loaded` / `handing control` 三条路标出现 | 已覆盖：`test-cmd-smoke.sh` Stage 3b 逐条 `grep -qF`，缺任一条即 FAIL 并回打串口尾 12 行 | 已交接完成（脚本后续 du/stat/find 腿的现状失败与本条无关，见批次记录） |
| TODO-07 | §4.2 UEFI 侧宿主可测面 | 两列表互斥完备；root/bump 不与 free 列表相交 | `assert_bootstrap_outside_memmap` 已覆盖（3 条：相离/贴边通过、包含/跨界 panic）；`build_memmaps` 的调用点单根，可测面随 T1 分类循环纯函数化后重定义 | 纯函数三案已交接完成；其余随 T1 |

**交接方式**：TODO-01~07 以本表为唯一登记处；源码工作流修复后，本表的覆盖状态随之回填。

### 5.4 自检：写不出可观测断言的机制

逐条问过推导表：**M1–M15 全部写出了可观测断言**，没有落到"写不出"的机制——本节的第二重收益（暴露不可证伪的机制）此处为空。两条断言的观测强度偏弱、如实标出：M4 的调用序（只能靠调试钩/真机检查点弱观测）与 M14 的"resume 只做 additive"（只能靠真机"没有崩"反证）。

---

## 6. 诊断

本章回答"引导失败了怎么查"：6.1 讲 EBS 前后两条输出通道与里程碑路标的读法，6.2 是从静默到可读的 panic 路径，6.3 列典型失败模式与对应信号，6.4 给可复制命令。这一章按用途查，不按执行顺序读。

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

`raw_serial` 与 `raw_serial_line` 的差别：后者在写之前等发送侧腾空（x86_64 读 LSR bit5，有界轮询）并在 `\n` 前补 `\r`；前者在 x86_64 上不等、也不补——aarch64 上两条通道共用同一个带节流的发射器（源码注释明写本架构上"等腾空"与"不等"没有区别），差别只剩 `\n` 前补不补 `\r`。前者用于"已经知道自己在写什么、只想尽快留下痕迹"的场合（例如 EBS 之后的第一行），后者用于 panic 路径。

里程碑清单（每条给源码位置；这些字符串是排查时的定位依据。清单里没有独立的 step3 路标——开分页夹在 `step1+2` 与 `step4` 之间，源码未单独打点）：

| 输出 | 阶段 | 位置 |
|---|---|---|
| `boot-shim: prepare_boot enter`（x86_64） | 进入固件相关准备 | `uefi_helpers.rs:prepare_boot` |
| `boot-shim: prepare_and_enter enter (handoff v1 arena)`（aarch64） | 进入 arena 交接准备 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| `boot-shim: root+bump allocated` | 根页与 bump 区已分配 | `prepare_boot` |
| `boot-shim: loading kernel.elf from ESP…` | 开始读内核 | 两条入口共用 |
| `boot-shim: kernel loaded (entry staged)` | 内核段已拷贝 | 两条入口共用 |
| `boot-shim: 12 boot modules loaded` | 12 个模块已就位 | 两条入口共用 |
| `boot-shim: locating platform sources…` | 开始扫配置表 | `prepare_boot` |
| `boot-shim: memmap ok (final pre-EBS snapshot)`（x86_64） | 两份内存列表已产出 | `prepare_boot` |
| `boot-shim: building KernelInfo…`（x86_64） | 构造结构体 | `prepare_boot` |
| `boot-shim: exiting boot services…`（x86_64） | 即将退出 | `prepare_boot` |
| `boot-shim: [raw] boot services exited`（x86_64） / `boot services exited (paired)`（aarch64） | 已退出（裸串口） | 两条入口 |
| `boot-shim: memmap source: …`（aarch64） | 最终内存图来源行（D1 闸在 EBS 之后跑） | `prepare_and_enter_kernel_image` |
| `boot-shim: [raw] arena=… conv=… res=…`（aarch64） | arena 基址与分类条目数 | 同上 |
| `boot-shim: [raw] pt_alloc registered, entering kernel arch_boot`（x86_64） | 分配器已注册 | `main.rs:main` |
| `boot-shim: handing control to kernel arch_boot`（x86_64） | 跳转前最后一条 shim 路标（裸串口） | `main.rs:main` |
| `boot-shim: [raw] entering kernel image v1`（aarch64） | 跳转前最后一条 shim 路标（裸串口） | `prepare_and_enter_kernel_image` |
| `kernel: arch_boot entered` | 内核侧入口路标（x86_64；它之前诊断块还打印 `kernel: arch_boot arg=…`/`reslen=…` 两行） | [`arch_boot`](os/kernel/src/lib.rs:arch_boot) |
| `kernel: v0 enter` | `boot_validate_and_prepare` 的首个路标（v0–v3 四个标记依次打出）；v1 链在 arena 解码之后走到这里，直核腿在进入 `arch_boot` 之后走到这里 | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:boot_validate_and_prepare) |
| `### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)`（aarch64） | 接收端已进入 Rust 面 | `kernel-image/src/main.rs:rust_image_main` |
| `kernel-image: step4 DM coverage (additive on active root)` / `DM ok, jumping to kmain` | 接收端 additive 阶段路标 | [`arch_boot_resume_high_half`](os/kernel/src/lib.rs:arch_boot_resume_high_half) |
| `kernel: step0 validate ok` / `step1+2 mappings ok` / `step4 DM coverage ok` | 建表逐步路标 | [`lib.rs`](os/kernel/src/lib.rs) 的 `boot_stage!` |

（`build_memmaps` 内另有两条诊断打印、只在有料时出声：`boot-shim: memmaps conv=… reserved=…` 与 ≥4MiB 描述符的 `reserved-big …` 行；它们是诊断读数，不进主路径里程碑。）

复算命令：`grep -n 'uefi::println!\|raw_serial' os/boot-shim/src/main.rs os/boot-shim/src/uefi_helpers.rs`。

**删掉 `handing control to kernel arch_boot` 这一行会怎样**：当 `arch_boot` 在内核打印自己的任何路标（x86_64 上先是诊断块两行，然后才是 `arch_boot entered`）之前卡死时，串口上最后一行会是 `[raw] pt_alloc registered`——它无法区分"卡在 `arch_boot` 入口"（查 `KernelInfo` 的值）与"卡在注册之后、跳转之前"（查跳转与栈）。两处的排查方向完全不同，而这两行输出正是用来分开它们的。

### 6.2 panic 路径：从静默到可读

```rust
// 节选（保留缓冲写入与输出段；注释有删节），os/boot-shim/src/main.rs:fn panic。
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

**在 panic handler 里调用 `format!` 或固件服务会怎样**：第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生——串口上得到的是"panic 处理自身的崩溃"或什么都没有，而原始 panic 的消息与位置被彻底覆盖。"panic 路径上不能分配、不能调固件服务、只能纯 CPU 操作加裸串口"这条约束就是从这里来的。

### 6.3 典型失败模式与诊断信号

| 失败模式 | 触发点 | 现场信号 | 下一步确认 |
|---|---|---|---|
| 引导器指令次序错（`load_mods` 在 `multiboot` 之后） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 的 `assert(mbi->mi_mods_count > 0)`（同函数前面那条查 `HAS_MODS` 的会放过去，理由见 2.1） | 未开串口调试（默认）：屏幕刷出 `Minix panic. System diagnostics buffer:` 与 `press any key to reboot`、等一个按键才复位；命令行里写了 `cttyline=`（`do_serial_debug` 为真）时走**互斥**的另一条通路——panic 由串口同步输出后立即复位，不刷屏、也不等按键。这条断言排在命令行解析之后，所以 `cttyline=` 已经生效；对照下一行那条在解析之前，串口一定不出声 | 核对 `boot.cfg.default` 的 `;` 分隔次序 |
| 参数压栈次序错（x86-32） | [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `assert(magic == MULTIBOOT_INFO_MAGIC)` | 同样刷屏等按键，但串口一定不出声——这条断言在命令行解析之前，`do_serial_debug` 还是 `.bss` 的零初值；报出的表达式里带着 `magic` 这个名字，而它当时装的是地址（值本身不打印，见 §2.2 的说明） | 核对 `head.S` 两条 `push` 的次序 |
| 模块缺失 | `loader.rs:load_boot_modules_with_loader` | `boot-shim: required boot module 'X' not found at <路径>` | 核对盘上 12 个模块名（脚本的 `mdir` 前置检查会先报） |
| 内核 ELF 读不到 | `FileLoader::read_required` | `boot-shim: required file not found: /EFI/minix/kernel.elf` | 核对 ESP 的 `/EFI/minix/kernel.elf` 与固件是否挂载了该分区 |
| bump 区间耗尽 | `BootAlloc::alloc` 返回 `AllocationFailed` | 内核侧 `map_huge`/`boot_pt_alloc` 失败，或 `enter_kernel_image_v1: bump region not registered` | 复算 `prepare_boot(N)` 的页数预算与镜像规模 |
| v1 信封/载荷无效 | `arch_boot_resume_v1` 的 `decode_*` | `kernel-image: handoff v1 envelope invalid: <变体>`（或 memmap/reserved/modules/platform/param 对应行） | 核对 shim 的序列化顺序与描述符长度；先看 `[raw] arena=… conv=… res=…` 是否已打印 |
| `bootstrap_len` 非零 | `KernelInfo::validate` | `bootstrap_len must be 0 (Rust port has no unpaged section), got N` | 查 boot-shim 的 `build_kernel_info` 调用实参 |
| 快照位置错（自举页被报成可用） | `assert_bootstrap_outside_memmap` | `conventional memmap [a,b) overlaps bootstrap allocation [c,d)` | 核对快照调用点在两次分配之后 |
| 大页阶梯无档可用 | `boot_validate_and_prepare` | `arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE` | 跑 `check-layout.sh` 的 L3/L4/L5 |
| 页表切换后取指故障 | `Paging::enable` 之后第一条指令 | QEMU `-d int` 日志里的异常记录；`CR2`/`EPC` 指向当前指令 | 核对恒等映射范围是否覆盖该地址（§4.6） |
| `KernelInfo` 切片读回全零 | `vm_handoff::classify` 或后续消费者 | VM 侧"no free memory regions"类 panic | 核对 `store_kernel_info` 是否落了五块（§4.5） |

**把 `assert_bootstrap_outside_memmap` 降级为 `debug_assert` 会怎样**（release 构建下它就消失了）：它挡的是"活的页表页与 bump 池被当作空闲内存交给 VM"这一类问题，而这类问题的发作要经过"VM 取帧 → 覆盖页表页 → 切换 CR3 → 取指失败"四步。断言在 EBS 之前运行（此时串口还在，消息里同时给出 conventional 区间与自举区间两个坐标），而故障在现场只剩取指错误——把这条断言降级，等于把一个可读的坐标信息换成一次无坐标的崩溃。

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

**只用第三层（GDB）而不开 `-d int` 会怎样**。页表切换后的故障是"取指即失败"：GDB 断在切换之后的第一条指令上时，故障已经发生，而"故障地址是哪个、是取指还是读"这类信息只有 QEMU 的异常记录才给。缺了第二层，现场只剩"停在了某处"，没有 `epc`/`CR2` 可对照页表范围。

---

## 7. 参见

### 7.1 前置与后续篇

- [`02-higher-half-kernel.md`](02-higher-half-kernel.md)（链接布局、ELF 段拷贝、成对跳转、`k_initial_stktop` 两条线索）
- [`03-kmain-cstart.md`](03-kmain-cstart.md)（保护结构）
- [`04-platform-discovery.md`](04-platform-discovery.md)（DTB/RSDT 解析与描述符）
- [`05-clock-interrupt-init.md`](05-clock-interrupt-init.md)（时钟与中断控制器）

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,mpx.S,memory.c,arch_smp.c}`
- `minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`
- `minix3/minix/include/minix/{param.h,const.h}`
- `minix3/minix/kernel/{table.c,system/do_getinfo.c}`
- `minix3/minix/servers/{mib/kern.c,vm/pagetable.c}`
- `minix3/minix/include/arch/i386/include/vm.h`
- `minix3/sys/arch/i386/include/multiboot.h`
- `minix3/etc/boot.cfg.default`

### 7.3 关联 Rust 源码清单

- `os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers}.rs`
- `os/libs/minix-boot/src/{kernel_info,boot_shim,handoff,platform}.rs`
- `os/kernel/src/{lib,boot_alloc,globals,memmap,vm_handoff,misc}.rs`
- `os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`
- `os/arch/src/x86_64/paging.rs`
- `os/kernel-image/check-layout.sh`、`os/kernel-image/src/{main,bootface,bootface_a64}.rs`
- `os/servers/vm/src/{vm_server,alloc_page}.rs`
- `os/libs/minix-types/src/types/boot.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/README.md`
- `os/qemu-tests/{test-shim-bootmarks-aarch64,test-riscv64-uboot,test-kernel-image-riscv64-bootface}.sh`
- `os/kernel-image/check-layout.sh`

### 7.5 符号速查（C ↔ Rust ↔ 章节）

按符号查读用（跨篇符号只给本篇章节；同一符号的多处出现按首次出现排序）：

| C 侧符号 | Rust 侧对应 | 本篇章节 |
|---|---|---|
| `MINIX` / `multiboot_init`（`head.S`） | `os/boot-shim/src/main.rs:fn main` → `arch_boot` | §2.2 / §3.7 / §4.1 / §4.9 |
| `load_stack` / `load_stack_start` | 引导栈符号（`check-layout.sh` 的 L7） | §2.2 / §5.2.2 |
| `pre_init` | `prepare_boot`（UEFI / OpenSBI 两路径） | §2.6 / §3.7 / §4.1–§4.2 / §4.9 |
| `get_parameters` | `prepare_boot` + `build_memmaps` / `build_kernel_info` | §2.3 / §2.4 / §4.2–§4.4 |
| `add_memmap` / `cut_memmap` | `os/kernel/src/memmap.rs` | §2.4 / §4.9 |
| `alloc_lowest` | 无对应物（AP 启动归 SMP 篇） | §2.4.1 / §4.9 |
| `overlaps` | `os/boot-shim/src/loader.rs:fn first_overlapping_pair` | §2.5 / §4.9 |
| `pg_clear` / `pg_identity` / `pg_mapkernel` / `pg_load` / `vm_enable_paging` | Step 1–3、`new_from_page`、`enable` | §2.6 / §4.6 / §4.9 |
| `freepde_start` | `free_upper_idx`（`Option<usize>`） | §2.6.3 / §3.9 / §4.6 / §4.9 |
| `alloc_pagetable` | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page`（注册式分配器） | §2.6.2 / §3.8 / §4.7 |
| `pg_alloc_page` | `os/kernel/src/boot_alloc.rs:BootAlloc::alloc` | §2.4.1 / §3.8 / §4.2 |
| `bootstrap_start` / `bootstrap_len` | 同名字段（恒 `(0, 0)`） | §2.3 / §3.5 / §4.4 |
| `mem_high_phys` | 无对应物（Step 1 固定上界） | §1.2 / §2.4 / §3.4 |
| `kinfo`（全局） | `KernelInfo`（按值交接 + `.bss` 深拷） | §2.3 / §3.4 / §4.5 |
| `kmain` | `kmain`（高半执行） | §2.2 / §2.8 / §4.6 |
| `k_initial_stktop` | `kern_stack_top` | §2.2 / §4.4 |

