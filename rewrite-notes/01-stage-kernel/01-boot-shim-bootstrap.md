# 01 · 引导交接：载荷、内存账与五条生产腿

> 本篇的轴是**跨边界传递的数据**：交接载荷的字段、两张内存表与来源记录、bootstrap 区间与分配器、校验门链，以及把载荷送出去的五条生产腿。
> 链接布局、ELF 摆放、运行期映射的完整机制与换栈进入归 02 篇；本篇 §4.8 只给 Direct Map 的启动期概念入口。

**体裁与两条路线**：本篇同时是重写说明与参考手册。第一次读走主线：§1.1–§1.2 → §2.1–§2.4 → §3.1–§3.2 → §4.1–§4.2 → §4.4–§4.5 → §5.1 → §6.1，约 1100 行；§2.5–§2.8 的对照、§3.2.2/§3.2.3 的字段与版本账、§3.4 的删除账、§3.6 的计数换算表、§4.5 的容量表是参考材料，排障或第二遍再读。查符号先看 §7.5，查失败先看 §6.3。正文的证据标签只有三档：`[结构保证]`（类型/布局/断言保证）、`[观测]`（真机或源码观察，非结构证明）、`[预防性不变量]`（为防已观测故障而设，机制未证实）；完整定义见 §5.3。

| 章 | 本章解决什么问题 | 第一次读 |
|---|---|---|
| 1 概述 | 内核接手时要带走哪三份数据，四条不变量怎么定义“交接成立” | 必读 |
| 2 C 源码分析 | C 侧怎样把引导器数据变成内核记录，哪些行为是 Rust 必须保住的 | 读 §2.1–§2.4；§2.5–§2.8 参考 |
| 3 Rust 设计决策 | 为什么按值交接、两张表为什么带来源记录、五条腿怎样收敛 | 读 §3.1–§3.2、§3.6–§3.7 |
| 4 实现详解 | 两条入口链怎样装载、建根、映射、换栈、跳转 | 读 §4.1–§4.2、§4.4–§4.5 |
| 5 测试 | 每条不变量有哪些证据，各自不证明什么 | 读 §5.1 |
| 6 诊断 | 启动失败时怎样从一行输出反推阶段 | 读 §6.1、§6.3 |
| 7 参见 | C 符号与 Rust 落点的速查索引 | 按需 |

## 1. 概述

### 1.0 本章导读

#### 问题入口

引导器把控制权交给内核时，内核手里只有一小堆由固件和引导器留下的数据。最典型的坏法：内核按引导器给的地址去读内存图，读到的却是自己地址空间里一段无关内容——不报错，只有错的数据。哪些必须带走、哪些只在引导器那边短暂有效、带错了会在哪一步坏掉，是后面所有工作的前提。本章要回答两个问题：内核需要哪几类数据才能开始建立地址映射并使用内存，这些数据为什么不能直接共享地址。本章先定三类数据（第 1.1 节），再给四条通用不变量（第 1.2 节），然后用数值示例检验交接是否成立（第 1.5 节）。

#### 前置概念

- 只需要知道“引导器把控制权交给内核”“固件在引导器之前留下平台数据”；“高半”等术语见本章末尾术语表。
- 知道物理内存与虚拟地址的区别、地址映射大致在做什么、页错误是访问无效地址时的异常；本章只使用这些词，不解释机制。
- 见过“文件描述符”这类进程局部标识即可，本章只用它当反例。
- 本章不要求知道具体载荷类型、协议或字段。

### 1.1 内核接手时拿到什么

这一节先定两个称呼：交出数据的一方叫发送侧，接收并使用的一方叫接收方；在本篇里分别是引导器与内核。

引导器退出前，内核真正需要带走的东西可以归成三类：

| 数据 | 回答什么问题 | 用在哪 |
|---|---|---|
| 可用内存的图（区间清单） | 哪些物理内存区间可以分配、哪些已被固件或引导器占用 | 建立地址映射、物理页分配 |
| 内核镜像的位置 | 内核自己落在哪里、有多大 | 建立地址映射、镜像摆放校验、裁剪内核自身占用区间 |
| 交接结构（记录） | 上面两类数据与其他必要启动参数按什么约定交给内核 | 承载前两类数据与其他启动参数；启动入口、校验 |

前两类数据是内容，第三类是承载它们的交接结构（一份记录）。内存图通常由固件声明、引导器转交；镜像位置由引导器按加载记录声明；交接结构由引导器写入。镜像位置用于从可用内存图中裁掉内核自己占用的区间；交接结构承载前两类数据与其他必要启动参数。三者都必须按值交付（复制内容，不传指针；为什么指针不行见 §1.2 的 I1）。这三类是本章的归类，不排除后续章节补充。

### 1.2 四条通用不变量

这四条不是并列的美德，而是四个失败面。每条都给出可检查的问法；具体载荷类型与协议如何实现它们见第三章。

| 编号 | 概念级不变量 | 可检查的问法 | 破坏后的现象 |
|---|---|---|---|
| I1 | 值可跨边界：接收方不能依赖发送侧维护的地址（虚拟或物理），也不能依赖进程局部句柄或标识（如文件描述符）；数据必须复制到接收方拥有的存储 | 这份数据里有没有指向发送侧地址空间的指针/虚拟地址、指向发送侧内存的物理指针，或进程局部句柄/标识？ | 接收方访问自己地址空间里不存在的地址（触发页错误）、读到发送侧的临时内存，或拿着无效句柄静默读错 |
| I2 | 来源可追溯：每个交付的区间或记录能说出“谁声明了它、依据什么”；转交者不算来源 | 这个区间/记录的声明者是谁、依据什么？答不上来就是来源不可追溯。 | 有交付记录，但说不出来源；坏数据无法归因 |
| I3 | 所有权唯一：一份数据只有一个所有者，一次提交 | 谁负责一次性发布完整数据？有没有第二个所有者？ | 半提交或重复提交，两个所有者同时发布或覆盖 |
| I4 | 校验先于消费：接收方在第一次使用前确认长度（每个交付区间与镜像位置的终点大于起点）、布局（各段互不重叠，镜像落在某个可用区间内且能被裁掉）、版本（交接结构版本与接收方理解的一致）、来源（按 I2 复核） | 第一次使用前，有没有完成这四项确认？ | 直接使用未经检查的长度、布局、版本或来源 |

“一次提交”指所有者一次把完整数据发布出去，之后不再改；“消费”指接收方第一次使用这份数据。I4 是消费侧复核：长度、布局、版本由接收方独立检查，来源按 I2 的问法复核。这里的“互不重叠”指交付的可用区间彼此不重叠；镜像落在某个可用区间内是正常的，I4 要确认的是它能被该区间裁掉，而不是要求镜像与所有区间都不相交。布局检查不能代替 I1：只有当指针被当成区间值、使长度或布局不自洽时，I4 才会顺带暴露；指针本身仍按 I1 的问法查。

I3 属于发布协议，不能从记录内容判定。它可以从交接时序核对：记录只被写入一次，交给内核后发送侧不再修改。示例按这个前提设定。

声明者（来源）是产生数据并给出依据的一方；转交者只搬运、不改内容，不算来源；所有者是负责一次性发布完整数据的一方。三者可以重合，也可以分开，按每个区间或记录分别记。依据指可独立复核的声明凭据（如固件表项、加载记录），不是“转交者说的”。三类数据都要过 I1–I4，差别只在检查对象：内存图检查区间值，镜像位置检查物理区间值，交接结构检查承载两者的记录本身。

纯物理区间值是数值，不含指向发送侧内存的指针；但承载它的记录里若放的是发送侧维护的地址（虚拟或物理），同样违反 I1。记录本身在交接后不再修改，不等于记录里引用的发送侧内存也长期有效；发送侧内存可能被复用或回收，因此物理指针同样不可跨边界。在本篇的启动场景里，具体风险是虚拟地址与物理指针；句柄是这条不变量的通用外延，细节留到后续篇章。

#### 失败对照：以 I1 为例

这里只展开 I1；其余三条的破坏现象见上表第四列。如果只把指向发送侧内存的指针（物理或虚拟）交给接收方，接收方读到的是发送侧维护的那块存储；交接后发送侧可能复用或回收它，读到的内容不再可靠；若该地址在接收方视图里不可访问，还会直接页错误。指针在发送侧有效，不等于在接收侧长期有效。所以跨边界时必须复制内容，而不是复制指针。把这条做反，故障会推迟到接收方第一次使用数据时才出现：可能访问未映射地址触发页错误，也可能读到一段无关映射而不报错。反过来，把内容复制过来，接收方读的是自己地址空间里的副本，出了问题能在交接处定位。

I2 坏的现场是“有记录、没出处”：坏数据无法归因，只能整批不采信。I3 坏的现场是两个发布者各写一半，接收方读到一份自洽但过时的记录，同样不报错。发送侧自己的栈、临时缓冲区和页表只在发送侧有效，不能带走；能带走的是复制出来的值。

### 1.3 与后续篇章的接缝

本篇覆盖“引导器交出控制权”到“数据送进内核入口”这一段：按值交付、来源记录、所有权与校验。

下一篇（镜像篇）负责建立地址空间，使这些数据在高半（高地址虚拟区间，见 §1.6 术语）可被访问。承重接缝只有一句：本篇保证“数据按值、可追溯、有唯一所有者、先校验后使用”，镜像篇保证“这些数据在进入高半后仍可被访问”。

### 1.4 范围与不写什么

本章的主干先回答“交接要带走哪三类数据（§1.1）、四条不变量怎样定义交接成立（§1.2）”；随后用示例走一遍，并在小结与术语中收束。本章不写以下内容：

- 具体载荷类型与协议细节：属于第 3 章。
- C 侧的引导器契约与源码机制：属于第 2 章。
- 镜像映射、换栈与进入高半：属于下一篇（镜像篇）。
- 各目标架构的固件流程：属于对应的架构启动篇。
- 项目内部的类型名、字段表、容量数字与代码块：属于设计与实现章节。

### 1.5 最小示例：四问走两遍

先约定区间记法：[起, 止) 表示含起点、不含终点。

不需要写代码。假设固件报告三段可用物理内存区间：[0x1000, 0x9000)、[0x100000, 0x200000)、[0x300000, 0x400000)；内核镜像位于 [0x180000, 0x1c0000)，落在第二段内；假设交接结构版本与内核预期一致。为简化，示例假定区间与镜像边界都按 4 KiB 页对齐（页大小不在本章展开）。

1. I1：描述这三段区间与镜像位置的记录里，有没有指向发送侧地址空间的指针、虚拟地址、指向发送侧内存的物理指针或句柄？
2. I2：每段区间与镜像位置能不能说出“谁声明了它、依据什么”？
3. I3：谁负责一次性发布这三段区间与镜像位置？有没有第二个所有者？
4. I4：第一次使用前，有没有确认长度（每个交付区间与镜像位置的终点大于起点）、布局（各段互不重叠，镜像落在某段内且能裁掉）、版本（交接结构版本与内核预期一致）、来源（按 I2 复核）？

这套问法对承载这些内容的交接结构同样成立：记录本身按值交付吗？来源可追溯吗？只有一个所有者吗？使用前校验过长度、版本与布局吗？

先遮住下一段，自己答一遍，再对照。

在这个例子里：

- I1：没有（记录里只有物理区间值与版本号，没有指向发送侧内存的指针——物理或虚拟，也没有句柄）。
- I2：三段区间由固件声明，依据是固件内存图的表项；镜像位置由引导器按加载记录声明；引导器对区间只转交。
- I3：示例按“记录只被写入一次、交给内核后发送侧不再修改”的时序设定：引导器是这次交接的唯一发布者，没有第二个所有者；三段区间的来源是固件、镜像位置的来源是引导器，按分记规则分别记。
- I4：四项都成立——长度：0x9000−0x1000=0x8000、0x200000−0x100000=0x100000、0x400000−0x300000=0x100000、0x1c0000−0x180000=0x40000，均大于 0；布局：0x9000<0x100000、0x200000<0x300000，三段互不重叠；镜像 [0x180000, 0x1c0000) 落在第二段 [0x100000, 0x200000) 内，可从第二段裁掉；版本与来源在示例中按“版本一致、来源见表项”的设定视为通过，真实交接要核对交接结构版本与声明表项。
- 交接结构：记录本身按值交付；引导器按约定写入并给出依据；只有一个发布者；使用前校验长度、布局、版本与来源。

在示例设定的交接时序下，四问全过，这份交接才算成立。

若把镜像改到 [0x500000, 0x540000)，它不在任何可用区间内，I4 的布局检查拒绝——内核无法从可用内存图中裁掉它。若第 1 问答“记录里放的是引导器内存图的指针”，直接拒绝，不必再看后三问。

### 1.6 本章回顾

#### 小结

读完本章，你应能对一份交接数据逐条回答 I1、I2、I4，并按交接时序确认 I3；说出它违反了哪条（可多条）不变量、根因是哪条。

- 内核接手时要带走三类数据：可用内存的图、内核镜像的位置、承载前两类与其他必要启动参数的交接结构。
- 四条通用不变量分别防地址/句柄、来源、所有权与校验四类失败。
- 承重接缝是与镜像篇的：本篇保证数据按值、可追溯、有唯一所有者、先校验后使用；镜像篇保证这些数据进入高半后仍可被访问。
- 物理区间值（数值）可以跨边界；指向发送侧内存的物理指针（地址）与虚拟地址都不可以；内容必须复制。
- 自检：如果引导器交来的是指向它自己内存图的指针，它违反了哪条不变量？（I1 是根因；I4 可能顺带暴露，但不能代替 I1；若记录本身也说不出来源，I2 同时坏。）

下一章从 C 侧看引导器怎样把这些数据交进内核。

#### 术语

- **交接**：发送侧把控制权与所需数据交给接收方的过程。
- **可用内存的图（简称内存图）**：记录哪些物理区间可分配、哪些已被占用的区间清单，不是图形。
- **内核镜像的位置**：内核自身在物理内存中的起点与大小。
- **发送侧 / 接收方**：交出数据的一方 / 接收并使用数据的一方。
- **交接结构**：引导器按约定写给内核的一份记录，承载前两类数据与其他必要启动参数；记录本身按值交付，不指向发送侧地址空间。
- **句柄**：进程局部标识，只在所属进程的句柄表里有意义。
- **加载记录**：引导器装载镜像时留下的位置与长度记录。
- **按值交付**：复制内容，不传递指向发送侧内存的指针（物理或虚拟），也不传递进程局部句柄或标识。
- **提交**：所有者一次把完整数据发布出去，之后不再改。
- **发布**：所有者把完整数据一次写入并交给接收方。
- **交接时序**：记录只被写入一次、交给内核后发送侧不再修改的发布顺序。
- **来源 / 所有者**：来源是声明数据并给出依据的一方，转交者不算；所有者是一次发布完整数据的一方。
- **声明凭据**：可独立复核的来源记录，如固件表项、加载记录。
- **消费 / 消费侧**：接收方第一次使用这份数据 / 接收方使用数据时所处的一侧。
- **I1–I4**：四条通用不变量的编号，定义见 §1.2。
- **载荷**：交接时携带的具体数据内容。
- **协议**：发送侧与接收方对记录布局与字段含义的约定。
- **失败面**：四条不变量各自防的一类失败。
- **高半**：内核运行所在的高地址虚拟区间。
- **承重接缝**：本篇与后续篇章之间必须明说的责任分界。

## 2. C 源码分析

### 2.0 本章导读

#### 问题入口

C 侧没有“交接协议”这个类型，却要完成同一件事：在引导器退出前把内存图、模块位置和命令行变成内核能读的记录。最典型的坏法是内核按引导器给的地址去读内存图，读到的却是自己地址空间里一段无关内容——不报错，只有错的数据。

C 的做法一句话概括：先用 `get_parameters` 把引导器的结构体整份拷进自己的 `kinfo`，再用 `add_memmap`/`cut_memmap` 把外来区间裁成内核账本，最后用一条半开区间检查替模块与内核的字节把关。这条链只有一条：`boot.cfg` → `head.S` → `pre_init` → `get_parameters` → `mb_set_param`；每一段只回答两个问题：C 在这里保证了什么，边界在哪。

#### 前置概念

第 1 章的三份数据与 I1–I4；`struct kinfo`、`add_memmap`/`cut_memmap` 这些 C 名字在本章第一次出现。本章的判断只有一句：C 的交接面是三层按值复制加一条区间检查；后面章节的类型与协议，都是把这句判断写进类型系统。

三份数据在 C 侧各有一个落点：

- 可用内存图 → `kinfo.memmap[]`。
- 镜像位置 → `kinfo.bootstrap_*` 与链接期常量。
- 交接结构 → `kinfo.mbi`。

地址空间的建立不在这条链上，属于第 2 篇。

### 2.1 引导器契约与启动信息

引导器与内核之间靠三样东西对接：内核镜像里的识别头、跳转时的寄存器约定、一份由引导器填好的结构体。识别头嵌在内核二进制里，按 Multiboot 规范由引导器扫镜像前 8KB 找到它才认这个内核（[minix3/minix/kernel/arch/i386/head.S:multiboot_magic](minix3/minix/kernel/arch/i386/head.S)）。

- 协议只要求识别头落在镜像开头一小段里，因为这个头必须在引导器还不知道镜像布局的时候就能被找到；`.balign 8` 是比协议本身更严的对齐选择。
- magic、flags、checksum 三个字段满足 `magic + flags + checksum == 0` 这一条加法恒等式——它不是校验和算法，是一次加法恒等式；三个字相加为零就是“这个头部完整”的证明。
- flags 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块按页对齐）。**没有置视频位**，所以同一段里的 EGA/80/25 四个视频字段按结构体注释应当被引导器忽略。`WANT_MEMORY` 让粗档有数可用；`MODS_ALIGNED` 让模块按页对齐，`overlaps` 的页粒度假设才有依据。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`；五个字段同受 `HAS_ADDR` 一位管，而 flags 没置它，所以五个都不被读。这五个 0 是一个选择：**让引导器决定装载位置**，内核不指定。
- 本仓这条路径上跑的是 MINIX 自带的 boot2，它按 ELF 头装载，不读识别头；识别头是给 GRUB 一类合规引导器准备的。

跳转时两个寄存器各放一个魔术值，两个值只差最高半字节（`0x1` 与 `0x2`），传递方向相反：

| 魔术值 | 值 | 方向 | 谁读它 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | Multiboot 合规引导器扫镜像前 8KB（boot2 不读） |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | `pre_init()` 的第一条断言 |

配置文件里的每条指令各把一份信息送到这个交接面上。解释 `menu=`/`load_mods`/`multiboot` 这套指令语言的是 MINIX 自带的第二级引导（[minix3/sys/arch/i386/stand/boot/boot2.c:command_load_mods](minix3/sys/arch/i386/stand/boot/boot2.c) 是其中一条的实现）：

```
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
```

一条 `menu=` 行里，分号前是 `load_mods`，分号后是 `multiboot`；顺序就是契约本身。三条指令各送什么：

| 指令 | 送到哪里 | 内核哪一段读它 |
|---|---|---|
| `multiboot <内核路径> [命令行]` | 镜像被读进物理内存后跳到入口；内核路径与参数拼成一串成为 `mbi->mi_cmdline`（[minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot](minix3/sys/arch/i386/stand/lib/exec.c)） | 命令行由 `get_parameters()` 解析；镜像地址不从引导器读，来自链接期常量 |
| `load_mods <模块通配路径>` | 只把每个匹配文件的**路径**挂进引导器自己的模块链表，不读内容、不分配地址；真正的读取与地址登记发生在 `multiboot` 执行时的 `module_init` | `get_parameters()` 把整个数组拷进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 命令行里的一个 `key=value` 词，随整串进 `mi_cmdline` | `get_parameters()` 逐词交给 `mb_set_param()`，落进 `kinfo.param_buf` |

三条指令有一处不对称：只有 `rootdevname=` 的内容是内核能直接看见的，另两条的产物是一个地址或一段内存，内容要看引导器填的结构体。这里说的“模块”是内核随后要逐个启动的那些服务进程的镜像（名单与顺序见第 4 章）。

`rootdevname` 是根文件系统的设备名，写时不带 `/dev/` 前缀（值形如 `c0d0p0`）：装机脚本 `update_bootcfg.sh` 把 `$ROOT` 过一道 `sed 's/\/dev\///'` 剥掉前缀，再以 `rootdevname=` 写进 `boot.cfg.default`；该值在 setup 期间会变，所以配置里只写变量 `$rootdevname`。它之所以走命令行，是因为 Multiboot 结构体里没有对应字段；它只能作为一个通用 `key=value` 搭乘。

内核自己不解释这个参数——`grep -rn rootdevname minix3/minix/kernel/` 无命中——它只是把它搬进 `param_buf`；需要“根设备是谁”的是用户态的两个读者：RAMDS 的启动脚本与 mib 服务。这条分工就是微内核分层的直接体现：**对纯用户态参数，内核只当搬运工，语义留给用户态**。两个有专用字段的键（`cttyline`/`cttybaud`）与有内核读点的 `board=` 是例外，见 §2.3。

模块数有硬上限。`MULTIBOOT_MAX_MODS` 是 20（[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)），`kinfo` 的 `module_list[]` 按这个数开数组（[minix3/minix/include/minix/param.h:struct kinfo](minix3/minix/include/minix/param.h)），内核用 `assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)` 把它当硬约束：实际模块数最多 19，最后一个槽留给内核；到 20 不是截断，是当场停住。

**`load_mods` 必须写在 `multiboot` 前面。**

- `load_mods` 只登记文件名；告诉内核“有模块”的标志位由 `multiboot` 在装载内核时置上：`exec_multiboot` 按链表当时的内容建表，再置 `MULTIBOOT_INFO_HAS_MODS`、填 `mi_mods_count`/`mi_mods_addr`。链表被看见的时刻就是 `multiboot` 执行的时刻。
- 反序时表照样建、标志照样置（模块开关是配置级全局，默认开），所以查标志的 `assert(mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会通过，真正停住内核的是 `assert(mbi->mi_mods_count > 0)`。
- 若模块表分配失败，`module_init` 提前返回、标志未置，停在前一条断言。**诊断入口因此是模块条数，不是标志位**：按“标志没置”查会去怀疑引导器没解析 `load_mods`，按“条数为零”查才会去看两条指令的先后。

停机不是静默挂死：内核链接的是系统服务库的 assert（[`libsys/assert.c`](minix3/minix/lib/libsys/assert.c) 的 `__assert13`，打印后 panic），不是用户态那份（[`gen/assert.c`](minix3/lib/libc/gen/assert.c)，打印后 `abort()`）。

结论：引导器契约的实质是“三样东西、一处交接面”；内核不猜引导器的内存，凡是它要用的，都必须在 C 侧有一处按值收下的代码。

### 2.2 `head.S` 入口：`multiboot_init` 与 `load_stack_start → pre_init`

boot2 跳到 `MINIX` 标签后，一条 `jmp multiboot_init` 先跳过识别头：`MINIX` 是入口符号，识别头必须在镜像开头 8KB 内可被扫描，而 `multiboot_init` 的代码不必被扫到。[minix3/minix/kernel/arch/i386/head.S:multiboot_init](minix3/minix/kernel/arch/i386/head.S) 只做三件事：设栈、清帧指针与标志、按 C 调用约定把两个寄存器值变成两个参数。

```asm
// 示意，minix3/minix/kernel/arch/i386/head.S:multiboot_init。
multiboot_init:
	mov	$load_stack_start, %esp	/* make usable stack */
	mov	$0, %ebp
	push	$0; popf
	push	%ebx		/* info struct */
	push	%eax		/* magic */
	call	_C_LABEL(pre_init)
```

（节选，止于 `call pre_init`。）

- **设栈**：`load_stack` 是文件末尾 `.data` 段的 4096 字节静态空间，`load_stack_start` 是它的高端地址——取高端是因为栈向低地址生长。这是“能进 C”的全部原因：没有栈，`push`、`call` 与任何 C 函数都无从谈起。
- **对齐是构建保证**：`load_stack_start` 落在 4096 边界不是链接顺序的巧合。`head.o` 固定在 unpaged 对象列表首位（[Makefile.inc](minix3/minix/kernel/arch/i386/Makefile.inc)），其 `.data` 又是 `.unpaged_data ALIGN(4096)` 的第一个输入节（[`kernel.lds`](minix3/minix/kernel/arch/i386/kernel.lds)）；ARM32 入口用的是同一个 `load_stack_start`，同样落在 `.data` 末尾（[`earm/head.S`](minix3/minix/kernel/arch/earm/head.S)）。
- **清帧指针与标志**：`mov $0, %ebp` 终止栈回溯链（panic 打印调用栈与 GDB 远程调试都靠 `%ebp` 链逐帧走）；`push $0`/`popf` 把 EFLAGS 装成全零，净效果是栈深不变，关中断、清嵌套任务位与方向标志；`DF=0` 让随后的 `memcpy` 按地址递增方向走。
- **传参**：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压、第一个参数（魔术值）后压。`_C_LABEL(pre_init)` 是汇编宏，ELF 目标上直接展开为 C 符号名，非 ELF 目标上加 `_` 前缀；本仓是 ELF，可读作 `call pre_init`。

入口只能用汇编：设栈之前不能 `push`，而任何 C 函数的第一条指令就可能用栈；EFLAGS 与 `%ebp` 也只有在汇编里才能按需清零。`load_stack` 放在 `.data` 而不是 `.bss`，差别在这段字节的确定性由谁负责：`.data` 的 4096 个零字节是镜像文件自带的，装载时按普通内容拷进内存；`.bss` 没有这些字节，清零要靠装载器（[`libsa/loadfile_elf32.c`](minix3/sys/lib/libsa/loadfile_elf32.c) 在调用方给了 `LOAD_BSS` 位时才清）。这条选择省下的是对外部组件的依赖，不是一处崩溃；代价是内核镜像文件里多出这 4KB 零字节。

**栈对齐不满足 SysV。** 从 4096 对齐的栈顶算：`push $0; popf` 净零，第二个 `$0` 得 top−4，两个参数到 top−12，`call` 压返回地址后入口 `%esp ≡ 0 (mod 16)`。紧随的第二个 `$0` 不是第五个参数或保留槽，它只是一个用来挪动栈顶的空位；它把入口从 ≡4 变成 ≡0，却仍不满足 ABI。SysV 要求入口 `%esp + 4` 是 16 的倍数（即 `≡ 12`），达到它需要 20 字节，这段布局只压了 16 字节——比标准 ABI 少一次 push。这是风险提示而非设计选择：按当前 GCC 的默认假设，`pre_init` 很可能没有生成依赖栈对齐的 SSE 访问，换编译器或优化档可能改变；Rust 侧的对齐处理见第 3 章。

**两条 `push` 对调会怎样**：`pre_init(magic, ebx)` 收到 `magic = ebx`（`multiboot_info` 的物理地址，一个看起来完全合法的数字），`ebx` 收到 `0x2BADB002`。第一条断言当场失败，但这条 assert 只打印失败的表达式本身，不打印值——**不核对两条 `push` 的次序，容易往“数据结构坏了”的方向查**。

结论：入口的三件事（设栈、清状态、传参）都由汇编承担，因为它们都发生在“能调用 C”之前；`.data` 的选择省下的是对外部装载器的依赖，栈对齐则是原作留下的 ABI 缺口。

### 2.3 `get_parameters`：把别人的数据变成自己的记录

[minix3/minix/kernel/arch/i386/pre_init.c:get_parameters](minix3/minix/kernel/arch/i386/pre_init.c) 的第一件事是**整份拷贝**：不长期持有引导器给的指针，而是把整个结构体复制进自己的全局 `kinfo`。它的两个实参是 `ebx` 与 `pre_init` 传进来的 `&kinfo`。

```c
// 节选，minix3/minix/kernel/arch/i386/pre_init.c:get_parameters。
multiboot_info_t *mbi = &cbi->mbi;
memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));
```

这里有个同名陷阱：`mbi` 是局部指针，`kinfo` 的第一个字段也叫 `mbi`，声明行就是把前者指向后者。看到 `memcpy` 里光秃秃一个 `mbi` 就以为它是全局变量，是这一段最常见的误读。拷贝之后 `ebx` 指向的那份结构体不再被引用。

整份拷贝只是第一层，按值交付要三层都完成：

- `mbi` 里的 `mi_cmdline`、`mmap_addr`、`mods_addr` 仍是指向引导器内存的地址，只拷结构体仍然在解引用发送侧内存。
- 命令行再拷进一个局部缓冲；模块表按 `mi_mods_count` 拷进 `kinfo.module_list[]`；内存图逐条 `add_memmap` 成内核自己的条目。
- 三层都完成，I1（按值交付）才成立；只做第一层，命令行与模块表随时可能被引导器或后续装载覆盖。

同一段里五个字段分两类来源：

- `mem_high_phys = 0` 是“待累计”的初值，稍后由 `add_memmap` 推到最高可用地址。
- `user_sp`、`vir_kern_start`、`bootstrap_start`、`bootstrap_len` 没有一个来自引导器，全部取自链接期常量 `_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`。`vir_bytes` 类型上的 `&符号` 取的是符号的地址值——值在链接期就定了，运行时只是读一个常数。
- 道理是：**内核被装在哪儿、哪一段是分页前用的，在链接那一刻就定下了，内核不必问任何人**。这是 §1.1 框 2 不接收框 1 的根据——反过来，框 2 的区间要拿去把框 1 裁短。
- `user_sp` 先赋 `_kern_vir_base` 只是占位，同一函数后段会被 `USR_STACKTOP` 覆盖；`cbi->kmess = &kmess` 里的 `kmess` 是 `kmessages` 字段的宏别名（[`kernel/glo.h`](minix3/minix/kernel/glo.h)）。

`bootstrap_len` 是两个符号地址相减，得到的那一小段叫 unpaged：链接在**低地址**、虚拟地址等于物理地址，建页表与开分页必须由它完成。分页开启之后这段代码仍在低地址上执行一小段，靠的是恒等映射；`pre_init` 返回后 `head.S` 末段（换栈与 `call kmain`）也属于同一低段，而 `kmain` 等代码不在 unpaged 对象名单里，链接后落在高半。

命令行解析只在 `MULTIBOOT_INFO_HAS_CMDLINE` 置位时进行；没置位时 `param_buf` 里只有后面两条固定键。解析器是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 到 `=`、再收集 `value` 到空格；`if (*p++ != '=') continue;` 跳过非 `key=value` 形式的词，`p++` 已推过一个字符，所以不会死循环。命令行缓冲按 `MULTIBOOT_PARAM_BUF_SIZE`（1024）开。命令行里还可能有 `bootopts=` 等其它词，解析器不关心键名，一律按 `key=value` 进 `param_buf`。

每个键值对都过一遍 `mb_set_param`；随后**无条件**再写 `arch=` 与 `board=`（值来自编译期板卡身份，i386 上固定查 `BOARD_ID_INTEL`）。这两行保证的是“`param_buf` 里至少各有一条内核写入的记录”，不是“内核无条件知道自己是什么板”：`arch=` 在内核侧只有写点、没有读点，但用户态 RAMDS 的 `rc` 会经 `sysenv arch` 读它；`board=` 有读点（[minix3/minix/kernel/main.c:get_value](minix3/minix/kernel/main.c)），当引导器传入同名键且该键不是第一条时，读取返回第一条匹配，外部输入可以遮蔽内核写入的值。

[minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param](minix3/minix/kernel/arch/i386/pre_init.c) 同时做三件事：

- **识别专用字段**：头两行 `strcmp` 是仅有的两个专用键，`SERVARNAME`/`SERBAUDVARNAME`（`"cttyline"`/`"cttybaud"`，[`minix/const.h`](minix3/minix/include/minix/const.h)），命中时同时设 `do_serial_debug`/`serial_debug_baud`——从哪个串口做内核调试是内核启动早期自己要做的事，不能等用户态去读参数。
- **维护“键不重复”**：命中已有同名键时，把该条字符串末尾之后的内容整段左移，覆盖掉旧条目。维护键唯一的目的只有一个：读参数的人返回第一条匹配，重复键会让先写的那条胜出。
- **维护可追加格式**：找末尾的循环走到“当前字节与下一字节都是 NUL”的位置，再把写入点挪到终止 NUL 之后；空缓冲时从头开始写。

“已存在先删”那段有一个真实的 off-by-one：

```c
// 示意，minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param。
while (*p) {
	if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') { /* 删除 */ }
	while (*p++) ;	/* 越过本条 NUL */
	p++;		/* 多走一步：扫描起点偏一字节 */
}
```

`while (*p++)` 越过本条记录的 NUL 时，后缀自增已让 `p` 落在下一条记录的第一个字节上，紧接着的独立 `p++` 又多走一步——从第二条记录起，匹配检查从记录的第二个字符开始，同名键再也匹配不上。于是“键不重复”只在待删键恰好是缓冲区第一条记录时可靠成立：

| 情形 | 扫描行为 | 后果 |
|---|---|---|
| 待删键是首条记录 | 起点正确，命中 | 删除生效，键不重复成立 |
| 待删键在第二条以后 | 起点偏一字节，前缀匹配落空 | 旧记录保留，同名键重复（`A=1\0B=2\0` 再写 `B=3` 得两条 `B`） |
| 中段命中（空键，或 `xboard=` 这类含 `board=` 子串的非首条） | 只按 `name=` 前缀匹配，不要求落在记录起点 | 从记录中部开始删，后续字节前移或被截断，留下伪记录 |
| 外部 `board=` 是首条 | 删除使其后的内核值提升为首条，内核再写时删掉它 | 内核值胜出，仍正常 |

空键（`namelen == 0`）把同一缺陷推到极端：`strncmp(p, "", 0)` 恒真，判定只剩 `p[0] == '='`；若第二条记录形如 `B=2`，扫描落在它的 `=` 上，删除从记录中部开始、后续记录被截断。这是 C 源码的真实行为，本篇照实记录，不替它圆。

这段删除循环的存在意义可以用 `board=` 说清：

| 输入 | `param_buf` 里的记录 | `get_value("board")` 返回 |
|---|---|---|
| 无外部键 | 只有内核写的 `board=intel` | 内核值 |
| 外部 `board=bar` 非首条 | 旧记录被删，内核值补回 | 内核值 |
| 去掉删除循环，外部 `board=bar` 非首条 | 两条都保留 | 第一条，外部值遮蔽内核值 |

所以循环的目的不是“好看”，是让命令行不能改写板卡身份；代价是它只在首条记录上可靠。

`param_buf` 不是一条字符串，而是**一串以 NUL 分隔的 `key=value`，整串以一个额外 NUL 收尾**。容量检查用 `p + namelen + valuelen + 3 > bufend` 提前拒绝并返回 `-1`，但三处调用点全部忽略返回值（命令行解析一处、固定键 `arch`/`board` 各一处）：装不下是静默丢弃，不是启动失败。

`rootdevname` 的完整落链是：`boot.cfg` → 引导器拼进 multiboot 命令行 → `mbi->mi_cmdline` → `get_parameters` 逐词拆出 name/value → `mb_set_param` 写进 `kinfo.param_buf` → PM 启动期先用 `GET_MONPARAMS` 把 `param_buf` 整块取成缓存（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的拷出点）→ 用户态 `sysenv` 的 `PMGETPARAM` 从这份缓存回答。它不经过 `mb_set_param`，就只是一串留在 `mi_cmdline` 里、之后没人再读的字符。

两个读者各取所需：

- RAMDS 的 `rc` 把值拼成 `/dev/xxx` 交给驱动，没设时打印 “rootdevname not set”。
- mib 服务把它作为 `kern.root_device` 节点暴露。

结论：`param_buf` 才是内核自己的参数表，写入它、维护格式的唯一入口是 `mb_set_param`；这条入口的“键不重复”只在首条记录上可靠。

### 2.4 内存账：`add_memmap`/`cut_memmap` 与条目来源

#### 2.4.1 条目来源：细表与粗档

引导器给的是两档信息里的一档，`get_parameters` 先看细的那档，取不到才退回两个数。

- **细档**（`MULTIBOOT_INFO_HAS_MMAP`，bit 6）：逐条遍历 E820 表，只把 `MULTIBOOT_MEMORY_AVAILABLE`（值为 1）的条目交给 `add_memmap`；保留区、ACPI 数据区在这一步就被丢掉，而不是留着让下游判断。类型值与 E820 里“可用”同号，这不是一次翻译，是一次沿用。
- **遍历步长**是 `mm_size + sizeof(mm_size)`：`mm_size` 描述的是除它自己以外那些字段的长度（[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)；本仓发那次查询的代码在 [`lib/biosmemx.S`](minix3/sys/arch/i386/stand/lib/biosmemx.S)）。这个“长度字段不算自己”的约定也是从 E820 的返回缓冲区继承来的。
- **粗档**（`MULTIBOOT_INFO_HAS_MEMORY`，bit 0）：`else` 分支把两个以 KB 计的数硬造成 `[0, mem_lower*1024)` 与 `[0x100000, 0x100000 + mem_upper*1024)` 两段连续区间。这里藏着一个假设：`mem_upper` 的语义是“从 1MB 往上**连续**可用多少”，而 `add_memmap` 收到的是一个长度。`else` 分支先 `assert(HAS_MEMORY)`：两档都没有时当场停住，而不是建一份空账。
- 细档可能有很多条（BIOS 报告的内存空洞），粗档固定两条；这是“清单形态取决于档位”的具体差别。

**引导器只给粗档那一头会静默变坏**：1MB 之上被记成一整段连续可用区间，若保留区（显卡镜像区、ACPI 数据区）落在这段范围内，[minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page](minix3/minix/kernel/arch/i386/pg_utils.c) 从最高条目尾部向下发放、迟早会消耗到那里，把页表页放进本该保留的区域。症状不是“分配失败”，而是写入静默丢失或机器随机挂死，报错位置离真正的原因（引导器只置了 bit 0）很远。这也是 §1.1 框 1 写成“可用物理内存的清单”而不是“引导器给的内存数”的原因：清单的形态取决于引导器给哪一档，而“排除占用区”这一步两档都躲不掉。

结论：两档来源给出的清单形态不同，内核只能保证“至少有一档”，不能保证“细档一定存在”；可用性过滤放在入账这一步，下游才不必各自重判。

##### 2.4.1.1 追加与切除

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 为 40（[`minix/param.h`](minix3/minix/include/minix/param.h)）。用定长数组而不是链表，是因为引导期没有可用的分配器；每一项是一段可用区间，`mmap_size` 是当前遍历边界（切除不回缩，因此可能高于实际最高占用下标；中间可有零长空洞）。[minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap](minix3/minix/kernel/arch/i386/pg_utils.c) 与 [minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap](minix3/minix/kernel/arch/i386/pg_utils.c) 是仅有的两个操作。

```c
// 示意，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap。
if (addr > LIMIT) return;
if (addr + len > LIMIT) len -= addr + len - LIMIT;
assert(cbi->mmap_size < MAXMEMMAP); if (len == 0) return;
```

- `LIMIT` 把账截在 4 GiB 以下，0xFFFFF000 保证低 12 位为零；源码注释给的理由是“the rest of minix currently can't deal with any bigger”。`add_memmap` 写进去的条目类型恒为 `MULTIBOOT_MEMORY_AVAILABLE`——这份数组里不存在别的类型。
- 两条判断的次序有实际意义：先判 `addr` 把整条在界外的描述符整条丢掉，再判跨界并按需要截短长度。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）与一条提前返回的次序有意义：长度为零的区间在通过**容量断言之后**被静默丢弃。
- **满槽断言是粗检，空洞不豁免它**：`mmap_size` 是遍历边界，即使下标 5 有空位，只要最高下标是 39，`add_memmap` 也会在找空位之前停住——`panic("no available memmap slot")` 只在 assert 被 `-DNDEBUG` 编掉时可达。
- **找空位**：从下标 0 往后，第一个 `mm_length == 0` 的格子就是空位（没有单独的“已占用”标志位）。这个等价由入口的 `len == 0` 拒绝兜住；但 `0 < len < 4096` 的退化调用经 `rounddown` 后变成 0 并被写成一个零长度条目，会把 `mmap_size` 推高一格，只能靠“下次从下标 0 找空位时当空格复用”兜住。所以“零长度区间根本不会被写进来”这句话过强。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：新条目可能落在比当前边界更低的下标上，所以 `mmap_size` 取 `m+1` 而不能简单自增。它代表**遍历边界**；消费者必须自己跳过零长项——`pg_alloc_page` 从最高下标往回扫时，第一件事就是 `if(!mmap->mm_length) continue;`。一次切除会清空一条并把余量补到更早的空槽，`mmap_size` 不回缩；所以“当前条目数”与 `mmap_size` 不是一回事。`cut_memmap` 的循环也只走到 `mmap_size`，空洞与零长条目都在这段范围内，靠消费者跳过。
- **取整**：`roundup(addr)/rounddown(len)` 是**起点向大、长度向小**，只保证条目自身页对齐，不保证落在原描述符内。起点上取整会让终点最多外移一页：`addr = 0x1001, len = 0x2000` 得到 `[0x2000, 0x4000)`，原终点是 `0x3001`。已核对过的调用点起点都页对齐，因此这条缺口不可达；长度不保证页对齐，`mi_mem_lower*1024` 与模块文件长度都会被截掉尾页。
- `mem_high_phys` 只增不减，是所有条目的最高地址上界，`pg_identity` 依赖它。它记的是历史最大值，不是“当前最高条目的终点”：切除不回缩它，所以它是恒等映射的上界而不是当前可用上界。

两个分支还有一处不对称：`if` 分支显式清 `mmap_size = 0`，`else` 分支不写——它靠 `kinfo` 落在 `.bss` 的零初值与 `add_memmap` 的下标补救起来；这条零初值前提由两条装载路径兑现（[`loadfile_machdep.h`](minix3/sys/arch/x86/include/loadfile_machdep.h) 的 `LOAD_KERNEL` 含 `LOAD_BSS` 位，[`loadfile.h`](minix3/sys/lib/libsa/loadfile.h) 定义该位）。

`kernel_may_alloc` 是全局开关，分的是“这份账归谁”：[minix3/minix/kernel/main.c:kmain](minix3/minix/kernel/main.c) 开头写 1（“Kernel may use bits of main memory before VM is started”），[minix3/minix/kernel/main.c:bsp_finish_booting](minix3/minix/kernel/main.c) 在 `switch_to_user()` 前写 0。跨过这条线再改账，撞上的就是 `assert(kernel_may_alloc)`——改账动作当场失败，而不是悄悄改写一份已经交给 VM 的账。这个开关有两个同名定义：`pre_init.c` 的 unpaged 副本只在引导期有效，`kmain`/`bsp_finish_booting` 与高半的 `add_memmap`/`cut_memmap` 读写的是 `table.c` 通过 `_TABLE` 生成的 paged 副本。`cut_memmap` 入口也有一条同样的断言，补回余量时又经 `add_memmap` 再撞一次。

切割的方向相反：`start` 向下、`end` 向上取整，因为这里要“不把用过的页留在账上”，宁可多切一页。两处取整真正改变结果的情形只有一个：**退化请求**（`start == end` 且未落在页边界）——带取整时它被扩成整页出局，不带取整时夹取后 `substart >= subend`，一次都不切，被切对象照样留在空闲清单里。`substart/subend` 的两次修正把切除区间夹进当前条目，`if (substart >= subend) continue;` 让“完全在外”与“恰好相切”走同一个分支。命中时先把自己清零，再用 `add_memmap` 把左余量与右余量补回去；补回按“最早空槽”落位，不是末尾追加。一次切除因此可能把一条记录拆成两段，长度守恒：切 `[0x400123, 0x401F00)` 于 `[0x100000, 0x1000000)`，取整后切 `[0x400000, 0x402000)`，得到 `[0x100000, 0x400000)` 与 `[0x402000, 0x1000000)` 两段。

结论：走到这里，机器上第一次有了一份有据可查的可用物理内存清单；但“有据可查”指来源可信，不指内容已正确——粗档可能把保留区记成可用，`mem_high_phys` 只是给出了上界。

#### 2.4.2 从记录里取址：两个消费者与移交截断

`pg_utils.c` 另有两个从记录里取地址的函数，方向相反。[minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest](minix3/minix/kernel/arch/i386/pg_utils.c) 从最低处扫，唯一调用点是 SMP 路径的 [minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline](minix3/minix/kernel/arch/i386/arch_smp.c)：把 16 位启动代码拷到 1MB 以下给辅助处理器执行。跳板必须在 1MB 以下，是因为辅助处理器从实模式起步，16 位代码只能寻址低端内存。

它取段的姿势是 `alloc_lowest(&kinfo, tramp_size)`，而函数体里调用的是 `cut_memmap(cbi, lowest, len)`——`cut_memmap` 的第三个参数是**结束地址**，这里传的是**长度**。这是一次真实的参数语义错位：“实际切掉”的区间是 `[lowest, len)`，“想要切掉”的是 `[lowest, lowest+len)`。

| `lowest` 与 `len` 的关系 | 实际切掉 | 后果 |
|---|---|---|
| `lowest == 0` | `[0, len)`，与想要的一致 | 行为正确，但真实调用序下不可达 |
| `0 < lowest < len` | `[lowest, len)`，比分配量少 `lowest` 字节 | 尾部仍挂在空闲清单上 |
| `len ≤ lowest` | 夹取后一次都不切 | 整块被占用的内存从未从账上除去 |

真实调用序落在第三行，取决于四个前提：

1. `kmain` 先调 `cstart`（内含 `arch_init`），`smp_init` 晚于它，而 `alloc_lowest` 只在 SMP 路径被调用。
2. `arch_init` 末尾切掉含 0 的低段，`add_memmap` 又把起点向上对齐到页，所以记录里的最低基址至少是 `0x1000`。
3. `roundup(tramp_size)` 为 4096（当前跳板约 2212 B），不大于最低基址。
4. `alloc_lowest` 没有第二个调用点。

四条同时成立时，`len ≤ lowest`，**切除一次都不发生，跳板页仍挂在空闲账上**——这就是“第三行常态”。任何一条前提变化（改 `CONFIG_MAX_CPUS`、搬动 `arch_init` 的切除、新增调用点），“常态是第三行”都要重推。

第三行常态只说明“切除不发生”，不说明“不会当场被踩”：后者还要三条不在四前提内的条件——`pg_alloc_page` 从最高下标取页，跳板所在的低条目在常态下不是最高条目；跳板的生命周期在 `smp_init` 完成、VM 启动之前就结束（VM 的分配器是首次适配）。所以四前提是“落第三行”的必要条件，不是“不会暴露”的充分条件。

真正的后果在别处：返回的地址被拷进 16 位启动代码，而这段物理内存在账上仍是空闲；一旦更高下标的条目被耗尽，下一次 `pg_alloc_page` 就可能把它当空闲页发出去，辅助处理器启动到一半时代码被改写成页表内容——症状是某个 AP 起不来或执行到随机地址。**看到“分配”，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址。**

`copy_trampoline` 紧接着只校验 `trampoline_base + tramp_size < (1 << 20)`：它查的是“地址足够低”，不是“这段内存已经从账上切掉”。校验通不过能发现分配得不低，却检查不出“分配了但账没改”。

另一个取址函数 [minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page](minix3/minix/kernel/arch/i386/pg_utils.c) 从**下标最高**的条目取最后一页：先减长度、再返回“基址+新长度”，并把用量记进 `kernel_allocated_bytes_dynamic`。取页前有三条断言，其中“长度是页的整数倍、基址页对齐”两条是同一不变量在“发页”这一步的校验点。`alloc_lowest` 自己则先把 `len` 向上取整到页，断言 `len > 0`、找到的 `lowest != EMPTY`（`0xffffffff`）——没有一条够长时就断言失败，而不是返回 0——也把这一段的长度记进 `kernel_allocated_bytes_dynamic`。按**下标**取不等于按**地址**取：C 不维护“下标序 = 地址序”，进账按引导器报告顺序写入，切除的左右余量又都回填到最早空槽，存在至少一种切割形态使两者不一致。账耗尽时它走 `panic("can't find free memory")`。

两个方向各有其消费者：

- `pg_alloc_page` 服务 `pre_init` 之后的 4KB 细粒度映射路径与 VM 接手后的分配。
- `alloc_lowest` 只服务跳板。

两者读的是同一份账，所以这份账既是建表的上界依据，也是后续所有物理页分配的来源。

**移交 VM 时的 16 条截断**：VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条（`NR_MEMS = 16`，[`minix/const.h`](minix3/minix/include/minix/const.h)），下标 16–39 的 24 条被静默忽略，`vm/utility.c` 的注释原文是 “Any memory chunk in excess of NR_MEMS is silently ignored”。被忽略的 24 段不会以任何形式进入 VM 的位图，等于对 VM 不存在。截断发生在 VM 读取内核交接信息时，内核侧的 40 条账本身不受影响。账在 40 条容量里建、在 16 条口径里被消费。

结论：取址有两套规则、两个方向，而 `alloc_lowest` 的切除参数错位在当前调用序下不发作——这是“核对过的布局”，不是“代码正确”。

### 2.5 `overlaps` 与 `kern_mod`：内核把自己算成第 M+1 个模块

内存账建好后，要切掉模块占用的区间与内核自己占用的区间；每条记录切除之前先检查它与其余记录是否重叠——[minix3/minix/kernel/arch/i386/pre_init.c:overlaps](minix3/minix/kernel/arch/i386/pre_init.c) 就是源码注释里那道 sanity check。检查与切除按记录交错，没有全局预检。

正常布局里 boot2 顺序摆放模块、以页对齐推进 `image_end`（[minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot](minix3/sys/arch/i386/stand/lib/exec.c)）：前一条长度恰为页的整数倍时两者正好**相切**，否则中间留一段对齐补白。所以正常情况下谁也不会盖住谁；但相切恰恰是边界处理最容易写错的相邻关系。真出现重叠（换个引导器、或模块与内核被放在同一处）时，若没有这道检查，症状是两份镜像互相覆盖而无人报错。

```c
// 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:overlaps。
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
		INRANGE(mod1, mod2->mod_end-1))
```

- 区间语义是半开区间 `[start, end)`：`mod_end` 是最后一个字节之后的地址。依据在引导器一侧——boot2 填的是起点加长度；minix3 的 `multiboot.h` 对 `mmo_end` 一句注释都没有，**这个语义只能从填它的那行读出来**。
- `OVERLAP` 判两个点：对方区间的起点与**最后字节**。两点各管一种包含关系：比较对象被包在对方里时，它的最后字节落在对方内；比较对象包住对方时，对方的起点落在它内。第二项写成 `mod_end - 1`，就是把半开区间翻译回最后一个有效字节。
- `overlaps` 对每条记录各调用一次，所以一对重叠会被两个方向各检查一次，返回值 0/1 由调用点变成 panic；`cmp_mod` 是外层当前记录的下标，`n` 是 `mods_with_kernel`。一对记录 O(n²) 在这个规模上（最多 20 条）完全够用，没必要做全局预检。

内核自己不带模块描述符，但要参与核查就只能自报地址范围：[minix3/minix/kernel/arch/i386/pre_init.c:get_parameters](minix3/minix/kernel/arch/i386/pre_init.c) 把它塞进模块数组的第一个空槽——`k = mbi->mi_mods_count`、`assert(k < MULTIBOOT_MAX_MODS)`、`module_list[k] = [kernbase, kernbase + kernsize)`、`mods_with_kernel = k + 1`。`module_list[k]` 的写入发生在模块表整份拷贝之后，`k` 正好是第一个空槽（拷贝只填了 0..M−1）；那条 assert 确认这个槽存在，也是“内核还要占一个槽”的形式化。`kernbase`/`kernsize` 不来自引导器，是链接脚本导出的常量（`_kern_phys_base = 0x00400000`、`_kern_vir_base = 0xF0400000`，`_kern_size` 由链接布局算出）。

如果内核走特例分支，重叠检查与切除就要各写两遍；塞进同一个数组后，一段循环同时覆盖模块与内核——这正是“内核把自己算成第 M+1 个模块”的动机。同一个循环里既检查又切除：`overlaps` 报重叠则 `panic("overlapping boot modules/kernel")`，否则用 `[mod_start, mod_end)` 从账上切除。交错也意味着 panic 可能发生在已经切掉几条之后；boot 反正停住，账本不再有人消费。

**去掉 `-1` 会误报相切**：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`，两者不重叠；不带 `-1` 时 `INRANGE(B, 0x200000)` 为真，合法布局直接 panic。误报只需一个方向成立就够——外层以 A 为比较对象的那一次就已经停住，反方向根本轮不到执行。

**把 `overlaps` 整个去掉，账面不会坏**：同一段内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作。坏的是内容：两份镜像在重叠地址上互相覆盖，内核一声不响。所以 `overlaps` 守的是**模块字节不被踩**，不是内存账不被记错。

#### 2.5.1 三处改账动作与 12 个模块

| 名称 | 对象 | 谁做 | 动作 |
|---|---|---|---|
| 切占用 | 内核与全部模块的字节 | `cut_memmap`（§2.5） | 从可用记录里切掉 `[mod_start, mod_end)` |
| 还 VM 镜像 | 只有 VM 模块的字节 | [minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc](minix3/minix/kernel/arch/i386/protect.c) | 字节搬进目标地址空间后 `add_memmap` 加回，并把 `mod_start`/`mod_end` 置 0 当“已回收”标记 |
| 还引导段 | 内核自己的 unpaged 段 | `kmain` 后段 | `add_memmap(&kinfo, bootstrap_start, bootstrap_len)`；此时 `kernel_may_alloc` 仍为 1，`kinfo` 已在 `kmain` 开头拷到 `.usermapped` 副本 |

三条动作的对象各不相同：切占用处的是模块字节连同内核镜像自己那一块；还 VM 镜像处的是 VM 模块的字节；还引导段处的是 unpaged 段（边界由 `_kern_unpaged_start/end` 给出）。先后上，切占用最早，还 VM 镜像发生在内核装载启动进程时，还引导段在 `kmain` 后段。

`add_memmap` 的第三参数是**长度**（`mod->mod_end - mod->mod_start`），与 `cut_memmap` 的结束地址不同名不同义——两个被调函数同在 `pg_utils.c`，调用点分属 `protect.c` 与 `pg_utils.c`，只能逐个核对签名。

- “还 VM 镜像”的加回发生在字节复制进目标地址空间之后；置零在两个字段上一起做，此后遍历 `module_list[]` 的代码会看到一条“空模块”，这是“已回收”在数据结构里的表示，而不是另设一个布尔位。`kinfo.vm_allocated_bytes` 紧跟着记录 VM 的用量。
- 其余模块在 C 的 kernel 账上不归还：12 个引导模块里，只有 VM 的字节被加回内核账（`NR_BOOT_MODULES = INIT_PROC_NR + 1 = 12`，[`minix/com.h`](minix3/minix/include/minix/com.h)；[minix3/minix/kernel/main.c:kmain](minix3/minix/kernel/main.c) 里有 `if (NR_BOOT_MODULES != kinfo.mbi.mi_mods_count) panic(...)` 的计数核对），另外 11 个的字节区间被 `cut_memmap` 切走后没有 `add_memmap` 加回。
- 但这不等于永久少掉这段内存：VM 接手后先按模块长度把总量补进自己的空闲位图，`exec_bootproc` 装完再 `free_mem` 把 blob 放回位图。代价只发生在 VM 接手之前，且是可用总量的最后几页，不是启动失败的原因。
- 它真正的坏处是安静——不报错、不留痕；要对账也难，因为内核账缺的模块字节两边都不记，残差里还混着低段切除、EOD 取整损失与跳板那一页，不能单归因给模块。启动期唯一会走到 `pg_alloc_page` 的是装载启动镜像那条链（[minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc](minix3/minix/kernel/arch/i386/protect.c) 传 `PG_ALLOCATEME`），真到耗尽那一刻 panic 会打在装载途中，那只是报信的地方，不是出错的地方。

“还引导段”要等到 `kmain` 后段，因为在此之前 `pre_init` 的低段代码与 `kinfo` 还在使用；`kinfo` 拷到 `.usermapped` 副本、活跃的页目录与静态页表池属于高半 paged 副本之后，这段低地址内存才可以归还。

结论：内核与模块占用区只被一段代码处理，模块与内核之间没有第二套逻辑；C 的 kernel 账对非 VM 模块“只切不还”，回收责任移交 VM。

### 2.6 模块装载的检查与失败模式

模块装载的检查分三层，失败现场不同，诊断入口也不同：

| 层 | C 判据 | 失败现场 | 诊断入口 |
|---|---|---|---|
| 次序 | `load_mods` 必须先于 `multiboot` | `assert(mi_mods_count > 0)`，不是查 `HAS_MODS` 的那条 | 先看模块条数，不看标志位 |
| 条数 | `MULTIBOOT_MAX_MODS = 20`，内核自己还要占一个槽 | `assert(k < MULTIBOOT_MAX_MODS)` 或装载器越界 | 模块数上限与内核占槽 |
| 区间 | `overlaps` 用半开区间检查，相切放行 | `panic("overlapping boot modules/kernel")` | 重叠检查的 `-1` 与相切判据 |

条数上限 20 是数组容量，实际引导模块数是 12（`NR_BOOT_MODULES`）；两者不是一回事——12 是名单长度，20 是能装下的最大槽数。

三层问的不是同一件事：

- 次序问“模块表建的时候链表里有没有东西”。
- 条数问“内核之外还有几个槽可用”。
- 区间问“两段字节有没有互相覆盖”。

混层排查会把引导器配置错误当成内核缺陷，或者反过来。“参数缓冲装不下”不在这三层里，因为它是静默丢弃，没有失败现场。

结论：三层检查各自停在不同位置，排障先分层再定位；Rust 侧的对应处理见第 3、4 章。

### 2.7 ARM32 对照：自己造一份 multiboot

同一段引导准备在 x86 与 ARM 两侧各有一份实现，用的是同一串函数名。相同的地方：

- 入口标签同名 `MINIX`，入口之后都跳到 `multiboot_init`。
- 临时栈的放置与写法一致：`load_stack` 在文件末尾 `.data`，取高端符号。
- 清帧指针、随后换高半栈并调 `kmain` 的次序一致。

不同的地方：

- 指令集本身。
- 参数传递：x86 用栈传魔术值与 `multiboot_info` 地址；ARM 用寄存器传 `argc`/`argv`，没有 `magic` 也没有 `ebx`。
- `pre_init` 返回的 `kinfo` 指针：x86 经 `%eax` 压栈交给 `kmain`，ARM 留在 `r0` 里一路带到 `kmain`。

真正体现“引导协议不属于内核”的地方在 `pre_init` 内部：ARM 的 `get_parameters` 第一件事不是读引导器结构体，而是**造一个**——[minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi](minix3/minix/kernel/arch/earm/pre_init.c) 先 `memset`，再自己置 `MODS | MEM_MAP | CMDLINE` 三位（x86 上这三位是引导器置的），模块表是编译期常量 `MB_MODS_BASE + i * MB_MODS_ALIGN`，内存表只有一条硬编码的连续区间，命令行直接取自引导器寄存器传入的 `bootargs`（注释写作 “morph the bootargs into multiboot”）。

ARM 自造 `mbi` 的意义在于：`get_parameters`、`overlaps`、`cut_memmap`、`mb_set_param` 这一整串不需要第二份实现——差异被挡在入口这一层。此后 [minix3/minix/kernel/arch/earm/pre_init.c:get_parameters](minix3/minix/kernel/arch/earm/pre_init.c) 的其余部分与 x86 同构；注意 ARM 侧头文件用的是另一套字段名，同构是调用序与语义上的，不是拼写上的。

[minix3/minix/kernel/arch/earm/pre_init.c:overlaps](minix3/minix/kernel/arch/earm/pre_init.c) 有一处与 x86 不同：`INRANGE` 的上界写成 `<= ...->mod_end`，且上界取的是外层循环变量 `thismod`（宏在展开点捕获它，不是宏形参），`OVERLAP` 第二项直接取 `mod_end`、没有 `- 1`——合起来是闭区间判定，正是 §2.5 论证为错误的那一版。`INRANGE` 捕获外层局部变量是宏的典型陷阱：读这段代码必须把宏放回它在循环里的展开位置，不能按普通函数理解。

它在 ARM 上无害：模块槽距是 8 MiB、每个合成模块的 `mod_end` 又各留了一页（`MB_MODS_ALIGN - ARM_PAGE_SIZE`），模块之间不会相切。内核项是例外：它作为第 M+1 个模块追加时没有减一页，与 0 号模块的 30 MiB 间隔（`MB_MODS_BASE = 0x82000000` 减 `_kern_phys_base = 0x80200000`）是“当前镜像远小于 30 MiB”的构建事实（`[观测]`），镜像恰好长到 `0x1E00000` 字节时内核项与 0 号模块会相切并被误报。内存表只有一条，所以 16 条截断在 ARM 上不构成风险；x86 才可能用满 40 条。

撇开硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 只 `bl pre_init`，再由 `setup_mbi` 自造结构 | 引导协议（各架构不同） |
| 2 | 建立地址映射 | `pg_clear` + `pg_identity` + `pg_mapkernel` | 同名函数（页粒度不同） | 分页硬件（语义相同） |
| 3 | 开启分页/MMU | `pg_load` + `vm_enable_paging` | 同名函数（MMU 寄存器不同） | MMU 寄存器 |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定 |

结论：步骤 2、3 在各架构调用同名函数，因为那串操作的语义与“用哪条指令、页表项多少位”无关；只有第 1 步的入口与参数来源必须按架构另写，因为“参数从哪来”由引导器决定。

### 2.8 本章归纳：C 交付物的形状

x86-32 主链一句话：`boot.cfg` → boot2 → `head.S` 的 `multiboot_init` → `pre_init(magic, ebx)` → `get_parameters`（整份拷贝 `mbi`、命令行进 `mb_set_param`、模块表、内存图进 `add_memmap`）→ 对 M+1 条记录 `overlaps`/`cut_memmap` → `pg_*` 建表与开分页（第 2 篇）→ `return &kinfo` → `kmain`。`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步属第 2 篇的镜像半，本章只保留调用位置与 D3/D5 的落点。

交付物是 `kinfo` 里的五组记录：

- `kinfo.mbi`：引导器结构体的副本，后续读模块表与命令行的指针来源。
- `kinfo.param_buf`：命令行与固定键（`arch`/`board`）的落地表，NUL 分隔。
- `kinfo.memmap[]`/`mmap_size`/`mem_high_phys`：可用物理内存清单与它的上界。
- `kinfo.module_list[]`/`mods_with_kernel`/`kern_mod`：模块表与“内核当模块”的那一项。
- `kinfo.bootstrap_start`/`bootstrap_len` 与 `freepde_start`：镜像的 unpaged 段与建表返回的第一个空闲槽位号；`freepde_start` 不是交接数据，是建表结果。

交付物里没有单独的“可用内存总量”字段；总量要由 `memmap[]` 前 `mmap_size` 条累加。本章只描述 C 的记录形状；类型化与协议留给第 3 章。

启动期的五条次序约束 D1–D5（定义在 §1.4；编号是约束名，不是执行序；D1/D2/D4 的证据在本章，D3/D5 在第 2 篇的 J3）：

- **D1. 内存清单先于建表**。清单在 `get_parameters()` 里建完并切完，`pg_*` 都在它之后才跑（[minix3/minix/kernel/arch/i386/pre_init.c:pre_init](minix3/minix/kernel/arch/i386/pre_init.c) 里的排列顺序）。`pg_identity()` 用 `mem_high_phys` 决定哪些条目标成非缓存，且函数开头就有 `assert(cbi->mem_high_phys)`——反排时该值还是 `.bss` 的零初值，断言当场失败。这条 assert 只覆盖 `add_memmap` 这一半；`cut_memmap` 先于任何清单消费者，由调用序保证。`pg_mapkernel()` 的初始大页映射不依赖记录（它用链接期常量与已清零的静态页目录）；从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度路径，它同样要求记录已就位。
- **D2. 内核位置先于清单裁剪**。清单要排除的区间里包含内核镜像自己；镜像的物理区间来自链接期常量 `_kern_phys_base`/`_kern_size`，不来自引导器的数据结构（§2.3、§2.5）。
- **D3. 页表先于开启分页**。建表五步与 `enable` 的次序在第 2 篇的 J3；本章只保留 `pg_*` 的调用位置。
- **D4. 结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`（[minix3/minix/kernel/arch/i386/pre_init.c:pre_init](minix3/minix/kernel/arch/i386/pre_init.c) 的 `return &kinfo;`），`head.S` 在 `call kmain` 前把它压栈作为第一个参数（同一条 `push %eax` 在引文里是给 `pre_init` 传参用的，同名不同处）。
- **D5. 恒等映射先于内核映射**。覆盖次序的推演在第 2 篇的 J3；本章只保留约束名与调用位置。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`，它落在高半一侧；判据与证据表在第 2 篇的 §2.4。本章只需要这个事实：`pre_init` 返回后换上的栈与紧接着要跳去的高半在同一侧，所以换栈与跳转能成对。

断言不证明布局一定合法，它把“不合法”从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。对回第 1 章的 I1–I4：I1（按值交付）由整份拷贝 `mbi`、模块表与命令行复制承载；I2（来源）由 `add_memmap` 按引导器表项建记录承载；I3（所有权）由内核一次发布 `kinfo` 承载；I4（校验）由 `overlaps`/`cut_memmap` 与各条断言承载。

---

#### 失败对照

`boot.cfg` 里把 `load_mods` 写在 `multiboot` 之后，内核会停在 `get_parameters` 的 `assert(mbi->mi_mods_count > 0)`——不是查 `HAS_MODS` 的那条：标志位由 `multiboot` 置上，表也照建，只是条数为 0。正确写法是把两条指令放在同一条 `menu=` 行里、用分号隔开，`load_mods` 在前。模块表分配失败是另一条路：`module_init` 提前返回，`HAS_MODS` 未置位，内核停在查标志的断言，到不了条数断言。两种现场输出（是否设 `cttyline=`）见第 6 章。这条断言只打印表达式，不打印条数值，所以现场只能看到“`mi_mods_count > 0` 失败”，不能直接看到 0。同类的“边界写错”在 §2.5 还有一例：`OVERLAP` 少写 `-1`，相切的合法布局会被判成重叠并 panic。

#### 最小示例

```bash
$ grep -rn rootdevname minix3/minix/kernel/
$ grep -n 'assert(magic == MULTIBOOT_INFO_MAGIC)' minix3/minix/kernel/arch/i386/pre_init.c
$ grep -n 'add_memmap\|cut_memmap\|mb_set_param' minix3/minix/kernel/arch/i386/pre_init.c
$ grep -n 'MULTIBOOT_MAX_MODS' minix3/sys/arch/i386/include/multiboot.h
```

第一条无输出，正是“内核只当搬运工”的判据；第二条命中一行；第三条给出建账、切账与写参数表的三个入口；第四条给出数组容量 20 的定义处。

#### 小结

- 引导器契约由识别头、寄存器约定与 `mbi` 三样东西对接。
- `get_parameters` 整份拷贝 `mbi`，再把命令行、模块表、内存图变成 `kinfo` 的记录。
- `add_memmap` 建清单，`overlaps`/`cut_memmap` 逐条切除内核与模块占用区。
- D1–D5 是约束名，不是执行序；证据在本章与第 2 篇的建表节。
- ARM32 复用了同名入口与同一串函数名，差异在参数来源与 MMU 寄存器。

#### 术语

- **`mbi`**：引导器填好的 `multiboot_info` 结构体；`get_parameters` 把它整份拷进 `kinfo.mbi`。
- **`kinfo`**：C 侧的全局 `kinfo_t`，`pre_init` 填好、`kmain` 读取的记录。
- **`param_buf`**：内核自己的启动参数表，NUL 分隔的 `key=value` 串，唯一写入口是 `mb_set_param`。
- **粗档 / 细档**：内存图的两种来源——`mem_lower`/`mem_upper` 两段连续区间，或 `mmap` 条目表。
- **`overlaps`**：检查内核与模块、模块与模块区间是否相交的函数；相切必须放行。
- **`kern_mod`**：内核在 `module_list[]` 里的下标；内核以第 M+1 个模块的身份参与重叠检查与切除。
- **unpaged 段**：链接在低地址、虚拟地址等于物理地址、分页开启前执行且分页后不再需要的那段代码与数据。
- **[观测]**：真机或源码观察，不是结构证明。
- **第三行常态**：四前提成立时，`alloc_lowest` 的调用序落在 `len ≤ lowest` 那一行，切除不发生。
- **首条提升**：删除一条记录后字节整体前移，使某条 `board=` 成为 `param_buf` 的第一条。

## 3. Rust 设计决策

### 3.0 本章导读

#### 问题入口

三条 v1 腿跨镜像，两个直核腿同镜像，三个架构的固件接口又各不相同。如果每个组合各写一份交接协议，内核会在进入 `kmain` 之前就背上多套地址语义。本章要解决的问题是：怎样用一套按值交接的 `KernelInfo`、两张带来源记录的内存表和一条校验门链，把五种引导形态收敛到同一个接收侧，同时不把硬件差异带进内核。

#### 前置概念

- 第 1 章的 I1–I4 与三份数据。
- 第 2 章的 C 侧交接：`kinfo` 全局、`get_parameters` 的拷贝、`add_memmap`/`cut_memmap` 的记账、`overlaps` 的区间检查。
- 第 2 章 §2.7 的 ARM32 对照：同语义、不同参数来源。

### 3.1 协议换代：固件接口替换 Multiboot

> `[ARCH: ...]` 是本项目的架构演进标注，表示该处不是逐字重写，而是有意的架构变化；三档术语（Rewrite / Refactor / Architectural Evolution）见 `AGENTS.md`。

C 的实现前提写在 2.1 与 2.2 里（这里的 GRUB 泛指"第三方引导器"这个角色，本仓这条路径上具体是 §2.1 点名的 boot2）：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与两个内存分配接口（页分配 `AllocatePages`、池分配 `AllocatePool`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot 协议 + MINIX boot2） | Rust（UEFI 路径） | Rust（U-Boot shim 路径；`FW_PAYLOAD`（OpenSBI 把下一级镜像打包进固件本体的模式）直核形态不在本表，见 §3.1.1 的例外条款） |
|---|---|---|---|
| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（boot-shim 自身；被装载的 `kernel.elf` 仍是 ELF） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 自己取固件内存图，分成可用表与保留表（交付形状见 §3.2，分流规则见 §4.2） | 读 DTB `/memory` 节点的 `reg`，同样分成两张表 |
| 传递参数 | 寄存器 + `multiboot_info_t` | arena 基址（三条 v1 腿的入口寄存器见 §4.1）——arena 的定义与分区布局见 §3.2；`BootHandoffV1` 已序列化进 v1 arena（`KernelInfo` 的字段按协议分区写入，发送侧不构造进程内 `KernelInfo`），根页表物理地址在信封的运行期字段里 | 同 UEFI 的协议与形状，入口寄存器见 §4.1 |
| 退出固件服务 | 无此步骤（引导器早已离开） | 必须调 `ExitBootServices`：boot-shim 跑在 Boot Services 的**环境之内**，得在某个时刻主动声明"我不再需要你"；声明之后整套服务失效，本篇用到三类——读文件（`SimpleFileSystem`）、分配内存（页分配 `allocate_pages` 与池分配 `allocate_pool`，后者支撑 `Vec`/`Box`）、固件控制台输出（`uefi::println!`） | 无此步骤（U-Boot 已把控制权交出去） |

生产入口的同一约束（文件访问必须在 `ExitBootServices` 之前完成）由 §4.2 的九个动作表承担：读文件在动作 2、ExitBootServices（EBS）在动作 6。


`exit_boot_services`（**测试内核路径的辅助实现，不是生产入口**）自己的文档也把这件事说成契约；生产入口在 §4.2 的配对循环里直接用 raw `ExitBootServices`，内存图分类进两张表，**不丢弃**：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

> `LOADER_DATA` 是 UEFI 内存类型标签，意思是"引导程序向固件分配器租下的内存"（loader 是属格——引导程序的所有物，不是"被加载的数据"——这里只给这次取内存图的缓冲区定标签）；（测试内核路径下）返回的 `_mmap` 可以丢弃，因为测试内核要的清单已由上文 `build_memmap()` 返回的 conventional 列表交付。该 helper 只在 `qemu-test-kernels` 构建里存在；返回的 map 是 best-effort、不与 `build_memmaps` 的 MapKey 配对，生产路径用 `ebs_pair` 保证分类与 EBS 共用同一个 key。

被审视的另一路径与结论（三否一采）：

| 方案 | 结论与理由（可复核） |
|---|---|
| 保留 Multiboot，做 64 位扩展（Multiboot2） | 否决：两个新架构没有等价协议；`head.S` 那套"引导器填结构体 + 寄存器传参"在 aarch64/riscv64 上要各写一份，等于三份引导协议 |
| 各架构独立引导（x86 用 Multiboot、ARM 用 PSCI（ARM 的电源/启动服务接口）、RISC-V 用 SBI） | 否决：分叉的是架构名，不是语义——§2.7 的四步里只有第 1 步（接收引导参数）随引导器变，重写把它收进 §3.5 的固件抽象，其余三步共享同一套语义，不随硬件重新分叉 |
| 内核自带 stub（UEFI 形态：把 boot-shim 链接进内核镜像，内核自己读文件再自举） | 否决，理由是**依赖方向**：boot-shim 与内核并成一个二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与 §3.5「内核不依赖引导层」的单向边界冲突——屏蔽固件差异正是 boot-shim 的职责。riscv64 没有 Rust 支持的 UEFI 目标，但只挡住 UEFI 这一种实现；换成 opensbi 形态则改判为采纳（下一行） |
| 内核自带 stub（opensbi 形态：固件直接跳下一级镜像，引导面长在内核镜像里） | **采纳**：它就是 §3.6 的直核自引导形态（riscv64 真板 `FW_PAYLOAD`、QEMU `-kernel`；aarch64 `-kernel` 测试通道）。它不违反依赖方向——`kernel-image` 反向依赖的是 boot-shim 的**库函数**（库 crate 与可执行体 crate 的区分见 §3.5；取件表——`BootFileTable` 这个文件索引结构；memmap 源），不是引导器产物（下文三词分工：**取件表** = `BootFileTable`，**取件库** = boot-shim 导出的装载辅助函数，**取件车道** = 一次取件的来源通道：UEFI `SimpleFileSystem` 或 U-Boot 表），`minix-kernel` 与 `kernel-image` 都不依赖 `uefi`（§3.5 问题二）。定位是辅助启动路径，不作三架构主线 |

#### 3.1.1 接线状态（唯一权威表）

> **现状标注（本篇接线状态的唯一权威表；其余各节再提到接线状态，都指向这里）**：上面第一张表描述架构终态；第二张表是另一路径的裁决记录（三否一采）。当前各固件路径的接线状态如下。
>
> | 路径 | 接线状态 | 覆盖与理由 |
> |---|---|---|
> | x86_64 / aarch64 UEFI 产线 | 已接线：「boot-shim 建根 + v1 arena + 跳独立 `kernel.elf` 高半入口」 | 切换依据见 §3.2 的收口记录与 02 篇的 4.5 节 的实现现状 |
> | riscv64 U-Boot shim | 已接线：U-Boot 用 `bootelf` 装载 `riscv64-uboot-shim`，shim 建根后跳进 `kernel-uboot` 的高半入口 | 门脚本 `test-riscv64-uboot-shim.sh` 的六个串口路标（直到 `entering scheduler`）全部出现 |
> | riscv64 UEFI shim | **未接线**，原因是工具链事实而非工程欠账：Rust 编译器至今没有提供 riscv64 的 UEFI 目标（`*-unknown-uefi` 目标列表不含 riscv64） | 等编译器支持后再接 |
> | riscv64 直核自引导 | 辅助/嵌入式形态（非主线）；QEMU 的 `test-kernel-image-riscv64-bootface.sh` 走它 | 它是硬件规范支持的真实形态（理由见下段“例外条款的背景”），但该门只验证 DTB 表址与模块定位失败的错误路径（§5.4）；“启动成功”的主线证据是 riscv64 U-Boot shim 的六路标门（`run_all.sh` / CI 覆盖门），两者不能混读 |
> | aarch64 直核面 | 测试通道，无专用启动门脚本；由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动 | 判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门，见 §5.4。**形态由构建产物决定（设计裁决）**：该门用 `fw-aarch64-none` 编出的直核产物（入口只编 `bootface_a64` 自引导臂）；UEFI 盘面装的是 `fw-aarch64-uefi-recv` 产物（入口只编 v1 接收臂，`x0` 为零时诚实停机而非转自举） |
>
> 本表按路径列，不是腿计数：第一行覆盖两条 UEFI 腿，`riscv64 UEFI shim` 行是未接线路径。

> **两个 aarch64 产物（设计裁决）**：仓库现在产出两个 aarch64 `kernel-image` 变体——直核 `fw-aarch64-none`（`bootface_a64` 自引导，`-kernel` 门用）与盘面 `fw-aarch64-uefi-recv`（v1 接收臂，`xtask image --arch aarch64` 装盘用）。先前形态是「一个镜像靠 `x0 == 0` 在运行期判别」，设计裁决把它换成编译期分叉：`x0` 零值不是任何固件承诺（QEMU 观察到不设参数，属实测而非规范），一旦某个加载器在 `x0` 放非零/零值就会把形态判反，而症状是 `handoff v1 envelope invalid` panic——指向交接协议而非入口形态。

> **例外条款的背景**：RISC-V 的固件形态规范（OpenSBI 的 `FW_PAYLOAD` 模式）允许把下一级镜像打包进固件本体，boot 时固件直接跳进 S-mode 目标——真板上这类极简装机（省掉 U-Boot 与文件系统）不需要引导器存在，QEMU 的 `-kernel` 模拟的正是这个时序。所以"引导面长在内核镜像里"是硬件规范支持的真实形态。§3.1 分工表的第三列描述的是 U-Boot shim 路径（`opensbi_helpers::prepare_boot`，由 `os/boot-shim/src/uboot_main.rs:fn uboot_main` 调用）；各路径的接线状态与"未接线的 riscv64 UEFI-shim 路径"指哪一条，见上面的现状标注表，这里不重复。
### 3.2 按值交接：`KernelInfo` wire 与字段台账（I1）

C 里 `kinfo` 是一个全局变量。`pre_init.c` 开头就定义 `kinfo_t kinfo;`，`get_parameters(ebx, &kinfo)` 往里填，`pg_utils.c` 里的记账函数通过 `cbi` 形参收到它的地址，`pre_init` 返回 `&kinfo`，`kmain` 再把这个指针传下去。整条链上传递的是**地址**，而地址之所以有效，是因为所有代码都在同一个地址空间里、链接成同一个镜像。

Rust 侧把这份共享状态改为一段**显式交接**，分两个维度说。

本节的交付形状与另外四节分工如下：C 侧"清单被截到 16 条"在 §2.4.2.4；扣减表与 bump 整段保留在 §3.4；接收方的六个落地区加一个名字池在 §4.5；换手给 VM 的时序在 §4.6。本节只讲"交接的是什么形状"。

**维度一：通道形态**（交接怎么送到接收方）随交接形态变；三种交接形态（同镜像内调用 / 跨镜像跳转 / 直核自引导）的定义在 §3.6，这里先用。x86_64 与 aarch64 的 UEFI 链都走跨镜像跳转：boot-shim 建好自举根、开分页，把一块八页 arena（定义与分区见本节后半）的物理地址递给独立 `kernel.elf` 的高半入口（入口寄存器见 §4.1）；接收方从 arena 解码六个分区、落进自己的 `.bss`，再走只增量的高半 resume。riscv64 的 U-Boot shim 走第三条跨镜像 v1 腿（同样只递 arena 基址，入口寄存器见 §4.1）；riscv64 的直核自引导没有独立 shim，引导面在同一镜像内直接调 `arch_boot`；这是硬件形态决定的辅助启动路径（§3.5 问题二），与两条 UEFI 产线不同源。

**维度二：载荷内容**（送出的是什么）三形态一致：必须按值深拷进接收方的私有存储，不能靠指针跨边界存活。理由：跨镜像档里两个分别编译的二进制各有自己的 `.bss`，指针在接收方指向完全无关的内存，连 `&'static KernelInfo` 也只保证生命周期、不保证页在对方页表里被映射；直核面虽然没有跨镜像指针，但同一套落地区把三形态收成一条路径（少一套特例），也让"落盘之后再消费"成为统一次序（v1 腿是解码落盘，两个直核腿是借用后落盘）。三形态一致的是"必须深拷进私有存储"这一条要求；具体来源分两组：v1 三条腿的切片由固定分区与 `byte_len` 描述符重建，接收方按分区偏移与元素类型解码、不再解释裸 `(ptr, len)`；两个直核面的切片由发送侧 `BootReport<'_>` 借用给出，接收侧落 `.bss` 后再构造 `KernelInfo`，没有 wire 解码步骤（§4.4）。

> **收口记录**：x86_64 与 aarch64 的 UEFI 产线采用独立镜像交接（§3.6），本节与此一致——记录保留是为了给"为什么 §3.2 的通道描述里没有低地址内联窗口"一个可考的出处。riscv64 各路径的接线状态见 §3.1.1 的接线状态表，这里不重复；本段要留的独特信息只有一条：这次切换不改变 §3.2 讲的 KernelInfo 形状与快照口径。

#### 3.2.1 `KernelInfo` 结构与五条腿的发送视图

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。


> [参考] 现役字段速查在 §3.2.1；下面这段 `16 → 13 → 14 → 10` 是设计变迁账，只用于解释字段数。

`kernel_info.rs` 的 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 共 10 个字段。字段计数的完整链条是 16 → 13（去掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，见 §3.4.1 与 §3.4.3）→ 14（加入 `authority`）；wire v10（设计裁决）再删掉 `startup_objects`（当时既是 `KernelInfo` 字段、也是 wire 分区与 builder 参数）、`user_sp`、`kern_stack_top`、`syscall_entry` 四个，落到 10。读表前先分开两套数字：`删除` 行里的 22 是 C `kinfo_t` 的累计退场数，不是本结构的字段数；本结构的字段数就是上面的 16 → 13 → 14 → 10（现役 10），明细见 §3.2.2.1。按与 C 的关系分成下面几类：

读表前先立三个局部术语：`A+D`（完整定义见 §4.2.1 动作 2：先读 ELF 预订 LMA 跨度、再分配 root/bump、最后拷贝段）；`ReservedMapped` = 固件声明为保留、但运行期可经 Direct Map 访问的档；`Side` = 既不进 free list 也不进保留表的侧桶（§4.2.2 完整判定）。

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap` | `memmap` 从 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带。C 那条"VM 只读前 `MIN(MAXMEMMAP, NR_MEMS)=16` 条"的截断（§2.4.2.4 的"另一件必须单说的事"段）**不再继承**：Rust 把落地区容量提到 128 槽（常量现居 [`globals.rs`](os/kernel/src/globals.rs:const MEMMAP_REGION_STORE_LEN)，C `MAXMEMMAP=40` 提额——UEFI 图常超 40 条；`KernelInfo.memmap` 本身是切片、无容量），VM 交接消费上限 64 条（`VM_BOOT_HANDOFF_MAX_REGIONS`）——扣除后仍有 65–128 条自由区间时能过落地区的 128 槽，但会在交接容量断言处 fail-closed（`VmBootHandoff.deducted` 另有 47 条容量；v9 已不存在 `kern_ident`/`VM_BOOT_HANDOFF_MAX_IDENT`，`ident_regions()` 不再存在）——16 条静默截断是 C 的行为，不是 Rust 的。三个数字的完整口径（对象、常量、溢出行为）见 §4.5 的"落地区与交接容量"表 |
| 改型（C 有、形状变） | `param_buf`：`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验<br>`module_list[]` → `boot_modules: &'static [BootModule]` | |
| 改名 | `vir_kern_start` → `kern_virt_base` | （`freepde_start` 曾改名 `free_upper_idx`，字段删除时整体退场，见 §3.4） |
| 升为字段 | `kern_phys_base`<br>`kern_size` | C 里这两个值不是 `kinfo_t` 的字段，而是链接脚本导出的符号 `_kern_phys_base`、`_kern_size`（`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 §3.4）。Rust 把它们放进交接结构，三条 v1 腿的来源改为 boot-shim 解析内核 ELF 的 PT_LOAD 段（§4.4 的字段表）；两个直核面用常量（例外见下段）。`kern_size` 用 `u64` 而不是 `usize`——wire 语义（wire = 跨镜像交接协议里按固定字节宽度写进 arena 的那份表示，与进程内结构相对）需要固定宽度，不随目标指针宽度变（32 位目标是 C 侧的，本仓重写目标全为 64 位，这里钉的是交接协议而非当前目标集） |
| 删除 | 22 个字段：<br>`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`<br>`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`<br>`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`<br>`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic`、`mmap_size`、`freepde_start`<br>`bootstrap_start`、`bootstrap_len`、`user_sp` | 逐个的去向见 §3.2 的字段去向表（`mmap_size` 由切片长度承载；`freepde_start`/`bootstrap_*` 的删除判据见 §3.4.1 与 §3.4.3） |
| 新增 | `reserved_regions`<br>`source_records`<br>`platform_sources`<br>`authority` | 各自来源见下；`source_records` 是 v1 source 记录，供启动期 `validate_requiring_sources`（strict 门）审计，`KernelInfo::validate`（非 strict）在 `source_records` 非空时也查“来源记录覆盖投影内存图”；v10 起 strict 门改为检查内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内（原来靠 `startup_objects` 的一条记录）。`authority` 是这份内存图的输入权威，见下表后的绑定说明 |

> **v10 删掉的四个成员（设计裁决）**：`user_sp`（每腿都填 `USER_STACK_TOP`，接收侧改为直接读该常量）、`kern_stack_top`（每腿都填 `kern_virt_base + kern_size`，改为 `KernelInfo::kern_stack_top()` 访问器在消费点派生）、`syscall_entry`（生产侧填 `kern_virt_base`、仓内零读者，真值来自 `syscall_entry_va()`），以及 `startup_objects` 分区（唯一读者是 strict 门的一条覆盖断言，现由内存图两表回答）。wire 信封 160 → 128 字节（四个成员在信封里各占 8 B：`startup_objects` 只占 `startup_byte_len` 一个槽，加上 `user_sp`/`kern_stack_top`/`syscall_entry` 三个 u64，共 32 B），报文末尾的 startup 分区（20 × 72 = 1440 B）退场，`WIRE_STARTUP_END`（30624）随之消失，八页 arena 的设计余量从 2144 B 变成 3584 B。`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收。

**权威与内存图绑定**。`authority` 记录这份内存图是从哪个输入对象投影出来的：`RamAuthority::DTB`（设备树）或 `RamAuthority::UEFI`（`GetMemoryMap` 快照）；`RamAuthority` 内部包一个 `InputObjectId`；`InputObjectId` 现在只有 `DtbBootInput` 与 `UefiMemoryMap` 两个变体（分别对应 `RamAuthority::DTB`/`UEFI`），`StartupObjects` 与 `startup_record()` 已随 v10 删除，wire 标签 2（输入对象）、6（kind）与 5（origin）只作保留位（这些编号是 wire 分区内的标签位，不是同名的枚举值）。

它由构造点写入——wire 解码把 [`BootHandoffV1`](os/libs/minix-boot/src/handoff_v1.rs) 信封里的 `ram_authority` 复制进来：两个直核面在 `arch_boot` 调用点传 `RamAuthority::DTB`，两条 UEFI 腿发送侧在 `Serializer::new` 里声明 `InputObjectId::UefiMemoryMap`、接收侧由 `decode_envelope` 解码为 `RamAuthority::UEFI`；riscv64 U-Boot shim 发送侧声明 `InputObjectId::DtbBootInput`、接收侧解码为 `RamAuthority::DTB`（它从 DTB `/memory` 取表，没有 EBS 快照）。

`KernelInfo::validate_requiring_sources()` 读结构体字段；`BootReport::validate_requiring_sources(authority)` 由调用者传入权威。如果权威仍是调用方的另一个实参，`KernelInfo` 与权威在类型上可以任意配对；混合记录表里"DTB 记录 + 声明 UEFI 权威"这类错配只会在下一次覆盖审计时才暴露。把权威放进结构体没有让错误的构造变得不可能（构造点仍可传错），但它把"图和它的声明"变成同一个值，解码路径不再有两个可以接反的参数。

**先讲清前两个字段装的是什么：内存图交付成两张表**。`KernelInfo` 携带内存信息只用两个列表，形状固定，来源不固定：

| 列表 | 语义 | 内核侧怎么用 |
|---|---|---|
| `memmap` | 可用内存；物理内存管理器从这里分配帧，但列表**不保证已扣除全部占用区**。内核镜像在**两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）**与 U-Boot shim 腿的表内；两条 UEFI 腿在 A+D 的 A 步（读完 ELF、预订 LMA 跨度，见 §4.2.1 动作 2）用 `AllocateAddress` 把整段 `[kern_phys_base, +kern_size)` 预订成 `LOADER_DATA`，因此**不在 `memmap`（自由表）内**，而是分类落 `ReservedMapped` 时进入 `reserved_regions`；无 `WRITE_BACK` 时落 `Side`，不进这份表（§4.2.2）。模块同理：**DTB 来源腿**（两个直核面与 U-Boot shim）的 bump 页在表内，UEFI 腿是 `LOADER_DATA` 因而不在。分配前仍须扣掉占用区（§3.4 扣除表；`vm_handoff::classify`，UEFI 腿的内核行退化为空操作）。LMA/VMA 的定义见 §1.6 术语与 §3.4，`LOADER_DATA` 的定义见 §3.1，`conventional` 与 `WRITE_BACK` 的定义见 §4.2.2 的令牌表 | 内核不保留可变 memmap 副本（早期引导段已移除）；`vm_handoff::classify` 在交接时对 memmap 做扣除，产出 free list 与 deduction record |
| `reserved_regions` | 可用但不可分配。运行期经 Direct Map 能访问，但不得再分配出去；`KernelLayout::ident_regions()` 删除后，`map_kernel` 改收 `&KernelLayout`（两参），不再单列 identity 参数，VM 调用点同步，“只断言为空”的检查与参数一起消失 | Direct Map 覆盖的候选之一（[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4） |

五条生产腿产出这两张表的方式不同（按来源分三组：UEFI 组两条、U-Boot 组一条、直核组两条；§3.6 把“产线”定义为链的别称，这里的“组”不是独立计数），形状相同：

- **两条 UEFI 链**：在 `ExitBootServices` 的配对循环里读 `GetMemoryMap`，按固件的类型标签分流（分流规则见 §4.2）。分类必须与退出共用同一个 MapKey，所以它只能发生在那一个循环里（`MapKey` 的定义、配对理由与重试语义见 §4.2）。
- **riscv64 的 U-Boot shim**：读 DTB `/memory` 的 `reg`（`DtbMemoryMap`，[`memmap_source.rs`](os/libs/minix-boot/src/memmap_source.rs)），再用固件常驻区上界 `KERN_PHYS_BASE` 削出保留段（OpenSBI 占 `[0x8000_0000, 0x8020_0000)`）。
- **两个直核面**：同一份 DTB 解析实现；aarch64 传 `fw_end = 0`，因为该形态的 DRAM 里没有固件常驻区——`fw_end=0` 只关闭"固件前缀裁剪"，DTB `/reserved-memory` 声明的区间仍进保留表（该形态典型为空，但机制上不保证）。

收敛点是**数据形状加统一的发送视图**（五条腿都构造 `BootReport<'_>`；两个直核腿直接借用，三条 v1 腿把它序列化进 wire），不是行为多态；这里的“统一”是形状收敛，不保证发送侧视图完整——UEFI 腿的 `source_records` 切片为空，source 分区的权威在 wire（§3.7）。两个直核腿把两张表放进 `BootReport` 的同名字段，三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）把同两张表写进各自的 `BootStorage`，报告视图是 `KernelInfo` 去掉 `authority` 的九字段视图（权威由腿在调用点提供）；接收侧统一经 `land_and_store` 落 `.bss` 后构造唯一 `KernelInfo`（其切片字段为 `&'static [T]`）。消费者只认区间、不认来源。这里没有一条"内存图来源"的 trait 承担统一，原因不是懒：UEFI 那条腿的分类时机被固件协议绑住——MapKey 与 `ExitBootServices` 一一配对，缓冲区不足与 MapKey 失效还要重试——一个只返回两张表的签名携带不了这些约束。

**内存图生产的四层责任**：上面三组内存图来源各自产出同一形状的两张表，但"从固件原始字节到两张表"的路径要跨四层，每层有独立的责任边界，不得混谈：

1. **UEFI 固件事务层**：`GetMemoryMap` + `MapKey` + `ExitBootServices` 是一个原子事务——分类必须与退出共用同一个 MapKey，否则固件拒绝退出。这一层决定了"分类只能在配对循环里做"（UEFI 腿），不能用一个独立函数事后补救。
2. **描述符解析层**：固件给的是原始字节，要变成带类型的描述符（类型、物理起点、页数、属性），需要统一的解析器负责字段边界、对齐与溢出校验。UEFI 腿用自建解析器；DTB 腿用设备树 `/memory` 节点解析。这一层的产出是"类型化的原始描述符"，还没做任何政策判断。
3. **OS 内存语义分类层**：政策侧把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Excluded（不可触碰）、Special（延迟处理）四档；这是设计专项的分类（详见 [`01-design-note-memmap-consolidation.md`](01-design-note-memmap-consolidation.md) 的 v12 分类，文件位于 `rewrite-notes/01-stage-kernel/`）。**现行实现把 Special 展开成两档落点**：MMIO 类走 `DevicePath`，其余按属性与类型表走 `Side`（侧桶：既不进 free list 也不进保留表）。因此实现侧的完整落点是五类——可分配 / `ReservedMapped` / `Excluded` / `DevicePath` / `Side`，判定是五步（不是三条规则），逐步判据见 §4.2.2 的表。
4. **内核映射与分配层**：Direct Map 覆盖（`dm_coverage.rs`）与物理帧分配器（VM 的 PMM）是两个独立消费者，它们对"哪些内存可用"的合同与分类层的输出对接——可分配性与可访问性不能脱节（两合同分别见 §3.2 与 §4.6）。

这四层的边界不能混谈：把第 1 层的 MapKey 纪律带入第 3 层的政策讨论（"反正退出前不能改"），或者把第 4 层的 Direct Map（DM）覆盖要求塞进第 3 层的分类判据（"要映射所以得进保留表"），都会让错误的层次为另一个层次的约束背锅。

**"升为字段"行的一个设计细节：为什么三个布局数字由 shim 从 ELF 算出，而不是由调用者传入**。设想把接口改为 `prepare_boot(storage, kern_virt_base, kern_phys_base, kern_size, bump_pages)`。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改为调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，"把内核映射到高地址"那一步会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。让 shim 自己解析镜像，"数字与镜像一致"就不再是调用者的责任，而是唯一数据来源（镜像本身）的自然结果。

`entry_point` 不进 `KernelInfo`/wire：boot-shim 从 ELF `e_entry` 取它并断言等于 `kern_virt_base`，跳转只用发送侧布局值；接收侧从 `kern_virt_base` 进入，不需要单独字段。

**两个直核面是例外**：`bootface`/`bootface_a64` 用自己文件里的 `KERN_VIRT_BASE`/`KERN_PHYS_BASE`/`KERN_SIZE` 常量，不从 ELF 头解析；它们与镜像布局的一致性：两个生产直核面与 x86_64 测试直核入口的跨距都由 `check-layout.sh` 的 L9 对账——x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`，链接期另有双保险：三份 `.ld` 都断言 `__image_end - 首段 VMA <= 0x400000` 且该差值是 2MiB 的整数倍（aarch64/riscv64 减 `KERNEL_VIRT_BASE`，x86_64 减 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`）。源码注释记录过这类常量漂移导致高半只映射半个镜像、`kern_stack_top` 落进 `.bss` 的静默损坏。因此“三个数字来自镜像”只对三条 v1 发送腿成立，直核面是“常量 + 部分构建期对账”的另一条来源。

#### 3.2.2 旧字段去向、GET_KINFO 外部面、控制台与 param_buf

本节按四个读者任务分组：字段去向、GET_KINFO 外部面、控制台、`param_buf`；控制台与 GET_KINFO 是字段的消费者/替身，不是字段去向本身，可先跳过。

字段名以 §3.2 的“删除”行为准；本表只给逐个去向，不重复定义。**三套数字分开记**：22 是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账（C 有、Rust 不再保留的全部字段）；16→13→14→10 是 Rust `KernelInfo` 的结构字段数（现役 10）；两者不是同一集合，交集是 `user_sp`/`freepde_start`/`bootstrap_start`/`bootstrap_len` 这几项。而 §3.4.1/§3.4.3 与 §3.2 说的「字段删除」是一次具体裁决——从 `KernelInfo` 里删掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，使字段数由 16 变成 13、`authority` 加入后为 14。

22 个字段的退场多数是换载体，少数是随读者消失而退役。多数：C 的 `kinfo` 是全局变量，编译期常量、内核自有状态、记账表都顺手堆在里面；Rust 的 `KernelInfo` 收窄为纯交接结构，只装引导侧必须告知、接收方无法自行导出的内容。于是职责归位成三类：**上浮为常量**——编译期已知的布局与规模事实（`nr_procs`/`nr_tasks`）与构建身份（`release[]`/`version[]`）；`user_end` 不属此类（Rust 未回传、无消费者，见下表）；**移交专门设施或记录结构**——内核自有的诊断与记账状态（`kmessages`、`do_serial_debug`/`serial_debug_baud`、`minix_panicing`、三个以 `_bytes` 结尾的记账字段）；**换表达不换信息**——内容改由 `KernelInfo` 现存字段承载或推导（`mbi` 的内容散布进各字段，`mods_with_kernel`/`kern_mod` 由 `kern_phys_base`/`kern_size` 直接表达，`boot_procs[]` 从 `boot_modules` 现算）。找不到新读者的不只 `mem_high_phys`（恒等映射只短暂存在、帧分配直接遍历 memmap，该标量随之退役）；`bootstrap_start/len` 与 `freepde_start` 也在该次删除中因无真实读者退场（§3.4.1/§3.4.3）。

##### 3.2.2.1 22 个字段的去向

被删的 22 个字段逐个的去向（"接管者"一栏给的是 `KernelInfo` 侧去向；`nr_procs`/`nr_tasks`/`release`/`version` 四个字段在 `KernelInfo` 侧上浮为常量/构建身份，但在 `GET_KINFO` 的 `KinfoStruct` 里仍是活字段，见本节 GET_KINFO 段）：

| C 字段 | 接管者 / 去哪了 | 依据 |
|---|---|---|
| `mbi` | 不进内核：UEFI/SBI 没有 Multiboot 结构体，交接信息由 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 另行承载 | - |
| `mem_high_phys` | 删除。C 里它是"最高物理地址"标量，有真实读者：恒等映射铺满整个 4 GiB 目录，靠它把 RAM 顶以上没有内存的段标成禁缓存；PCI 驱动另拿它找 RAM 之上的 MMIO 空洞。Rust 的 boot 恒等映射只覆盖最小窗口；启动后只在 `load_elf_into` 的 ELF 段页与 64 MiB VM heap 窗口做 4 KiB 粒度驱逐（撤销这些页的 4 KiB 映射）（不重建根——重建会清零活动根、下一个冷取指页错误（TLB 残留可能先跑几条），见 §3.6 反向推演；C 侧不重建活动根，它用 paged/unpaged 两份 `pagedir` 副本换根，见 02 篇的 4.3 节；逐页拆解本篇不展开，边界见 §1.4），其余 boot identity 窗口保留在 bootstrap 根上。帧分配器直接遍历 memmap 区域列表——`mem_high_phys` 这个标量在 Rust 里没有读者，维护它只是写后即弃的状态 | C 读者：[`pg_identity()`](minix3/minix/kernel/arch/i386/pg_utils.c)、[`complete_bars()`](minix3/minix/drivers/bus/pci/pci.c) |
| `mods_with_kernel` | 删除：C 里它只为一个用途——把内核临时登记成第 M+1 个模块（M = 模块数），让重叠检查与 memmap 扣除一并处理内核自身（[`pre_init()`](minix3/minix/kernel/arch/i386/pre_init.c) 的登记段）。Rust 里内核布局由 `kern_phys_base`/`kern_size` 单独表达，扣除与检查各自直接用区间，不需要这个计数 | - |
| `kern_mod` | 删除：C 里它是上面那个"临时第 M+1 个模块"的下标，随 `mods_with_kernel` 一起消失 | - |
| `kmessages` | 独立的内核消息/诊断设施 | [`kmess.rs`](os/kernel/src/kmess.rs)（`KernelConsole`） |
| `do_serial_debug` | 早期控制台初始化自己决定串口 | [`early_console.rs`](os/plat/src/early_console.rs) |
| `serial_debug_baud` | 同上（平台实现固定串口分频，不保留可调波特率状态） | 同上 |
| `minix_panicing` | 内核侧由 `PANIC_DIAG_ACTIVE` 维护互斥；boot-shim 的 `fn panic` 只是 panic 出口，不承载该状态 | `PANIC_DIAG_ACTIVE`（`os/kernel/src/lib.rs` 的 panic 诊断互斥静态量）、[`fn register_panic_diagnostic`](<os/kernel/src/lib.rs:fn register_panic_diagnostic>) |
| `user_end` | C 侧是 `USR_DATATOP`（i386 上等于 `USR_STACKTOP`）；Rust 未回传该字段，也没有消费者 | [`const USER_STACK_TOP`](<os/libs/minix-types/src/types/boot.rs:const USER_STACK_TOP>) 是用户栈顶的唯一权威 |
| `user_sp` | v10（设计裁决）删除：每腿都填 `USER_STACK_TOP`，接收侧改为直接读该常量 | 同上一行 |
| `boot_procs[]` | 内核侧从 `boot_modules` 构造 boot image 表 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `nr_procs` / `nr_tasks` | 常量与表长度 | [`const NR_PROCS`](<os/kernel/src/proc_table.rs:const NR_PROCS>)、[`const NR_TASKS`](<os/kernel/src/proc_table.rs:const NR_TASKS>)（12 是启动模块数 `NR_BOOT_MODULES`，不是 `nr_tasks` 的载体） |
| `release[]` / `version[]` | 内核构建身份，来源为 `minix-types` 里与 config.h 对位的固定字符串常量（`OS_RELEASE`/`OS_VERSION`） | [`misc.rs`](os/kernel/src/misc.rs) 的 GET_KINFO 段、[`kerninfo.rs`](os/libs/minix-types/src/types/kerninfo.rs) |
| `vm_allocated_bytes` | VM 交接记录的字段 | [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) |
| `kernel_allocated_bytes` | 同上（记录里的 `kernel_allocated_static`） | 同上 |
| `kernel_allocated_bytes_dynamic` | 同上（记录里的 `kernel_allocated_dynamic`，值来自 bump 分配器用量） | [`fn boot_alloc_used_bytes`](<os/kernel/src/boot_alloc.rs:fn boot_alloc_used_bytes>) |
| `mmap_size` | 未映射为 Rust 字段：C 的遍历边界（最高占用下标加一）与中间空洞是定长数组布局的产物；Rust 的 `memmap` 是紧凑条目列表，`.len()` 只是条目数，不需要单独的高水位标量 | [`globals.rs`](os/kernel/src/globals.rs:const MEMMAP_REGION_STORE_LEN) 的容量常量与 §2.4 的边界语义 |
| `freepde_start` | 曾改型 `Option` 跨 handoff 携带，字段删除时整体退场（删除论证与报告位填 0 的去向见 §3.4） | [`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` |
| `bootstrap_start` / `bootstrap_len` | 曾以 `(PhysBytes(0), 0)` + 硬断言携带，字段删除——误设回收区间的接口面随字段一起消失（论证、过度回收风险表与 wire 版本变更见 §3.4） | §3.4 |

C 的两个专用键在 Rust 侧的处置：`cttyline`/`cttybaud` 不再有专用字段，`normalize_param_buf` 把它们当普通 `key=value` 保留（能进 `param_buf`），但早期串口的选择由 `early_console` 的平台/编译配置固定，运行期不可调。


##### 3.2.2.2 GET_KINFO 收窄与外部面

**外部可见面的一条架构演进** `[ARCH: GET_KINFO 载荷收窄为五个 used 字段 + 两个 C 布局占位；其余字段由 GET_IMAGE/GET_MONPARAMS/GET_KMESSAGES 等分开提供]`：C 的 `GET_KINFO` 把整个 `struct kinfo` 拷给调用者（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的 `length = sizeof(struct kinfo)`）；Rust 只拷 48 字节的 `KinfoStruct`（布局：`freepde_start`/`user_sp`/`vir_kern_start` 三个 `i64` = 24 B，`nr_procs`/`nr_tasks` 两个 `i32` = 8 B，`release`/`version` 两个 `[u8; 6]` = 12 B，再加 4 B 对齐 padding，合计 48 B）（[`kinfo.rs`](os/libs/minix-types/src/types/kinfo.rs)），只含七个字段（五个有读者 + 两个 C 布局位；外加 4 字节 `_padding` 把结构补齐到 48 字节）——其中**五个有 Rust userland 读者的语义字段**：`nr_procs`、`nr_tasks`、`release`、`version` 由 IS（信息服务）读；`user_sp` 由 RS（重生服务器）的 exec 路径经 `GET_KINFO` 读 `KinfoStruct.user_sp`（offset 8，`os/servers/rs/src/trap_api.rs:223-230`、`:430`）。

`KernelInfo.user_sp` 与 `KinfoStruct.user_sp` 同名不同物：前者是交接结构的字段，后者是 GET_KINFO 的布局位；v10 删除的是前者。另两个是保留的 C 布局位：`freepde_start` 恒填 0（Rust VM 不借槽，见 §3.4）、`vir_kern_start` 有取值来源但仓内零读者（复核：`rg -n 'mem_high_phys' os/` 零命中；`rg -n 'vir_kern_start' os/servers os/kernel` 只在 `os/kernel/src/misc.rs` 的 `KinfoStruct` 构造处出现，没有读取该字段的消费者）。

C 侧至少四个活读取点：`dmp_kernel.c` 读四个字段，`procfs/root.c` 读 `nr_procs`/`nr_tasks`，`pci.c` 的 `complete_bars()` 读 `mem_high_phys`，VM 的 `main.c`/`pagetable.c`/`utility.c`/`region.c` 合计读 ≥11 个字段（`memmap`/`mmap_size`、`module_list`/`mods_with_kernel`/`kern_mod`、`boot_procs`、`user_sp`、`vir_kern_start`、`freepde_start`、三个 `_bytes` 等）。Rust 收窄的是**这条系统调用的载荷**，不是"C 只有这些读者"：Rust 收窄后，这些内容改由 `GET_IMAGE`/`GET_MONPARAMS` 与新增的 `GET_KMESSAGES`/`VmBootHandoff` 承载（新增通道标 `[ARCH]`），`mem_high_phys` 在 Rust 侧已不存在。

受影响的 C 字段——`memmap[]`/`mmap_size`、`module_list[]`/`boot_procs[]`、`param_buf`、`kmessages` 指针、`bootstrap_start/len` 等——不再经这条系统调用出现：模块表走 `GET_IMAGE`、`param_buf` 走 `GET_MONPARAMS`、`kmessages` 走 `GET_KMESSAGES`；内存账不走 GETINFO——它在 VM 启动前由内核写进固定地址的 boot handoff 页（`VmBootHandoff.free_regions` 加扣除记录），VM 在 `os/servers/vm/src/boot.rs` 读它。行为基线定为 Rust userland 的 `KinfoStruct`；按 C 全布局读被删字段的程序在本仓没有消费者。

##### 3.2.2.3 两种控制台与五处字段补充

表中两个"控制台"落点名字相近、层次不同：`EarlyConsole`（[`early_console.rs`](os/plat/src/early_console.rs)）是硬件输出通道——`minix-plat` 里的跨架构 trait，每架构至少实现 `write_byte`（`init` 可选；x86-64 用它初始化 COM1），启动与诊断输出经它直写串口；`KernelConsole`（[`kmess.rs`](os/kernel/src/kmess.rs)）是内核诊断收口——对应 C 的 `kputc`，把流经的每个字节先记进 10000 字节环形缓冲（C `struct kmessages` 的重写）再转发给当前架构的 `EarlyConsole` 实现，环内容经 `GET_KMESSAGES` 快照供用户态读取。分工一句话：前者管"字节怎么落到硬件"，后者在其上叠加"内核诊断的记录"。

几处需要展开的字段，各自的来源与理由：

- `reserved_regions`：被分类器标成 `ReservedMapped` 的内存区间（`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`）；`Excluded`/`DevicePath`/`Side` 不进这份表。它曾装两种语义：其一（x86 内联窗口）是内核链进 boot-shim PE、在低物理地址执行时，VM 建进程页表必须逐段恒等复现的代码/GDT/IDT/TSS 窗口；其二（UEFI 链通用）是内核运行期经 Direct Map 访问的固件分配（loader 数据、内核栈一类不在 conventional 列表里的页），建立 DM 覆盖时必须列入候选（依据见 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4）。x86_64 切独立镜像后第一种用途退场：`vm_handoff` 不再携带 identity 字段（v9 删除恒空的 `kern_ident`）；第二种用途保留，作为 Direct Map 覆盖的 source 4。各条腿怎么产出这份列表见 §3.2 的“五条生产腿产出这两张表”三条 bullet。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到。v10（设计裁决）把它们（连同 `user_sp`）从交接面撤掉：三者都是两端可算的同一常量关系，跨镜像传输只是把恒等式抄一遍；现在消费点分别用 `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size`）、`minix_types::USER_STACK_TOP`（每架构编译期常量）与 `syscall_entry_va()`。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。每个 `PlatformDescSource` 是 16 B 句柄 `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`；`len = None` 表示生产者未记录 blob 长度（旧生产者、测试或读头失败）。wire 侧仍写 `u32`：`None` 编码为 0，解码时 0 还原为 `None`。`RSDP` 是 ACPI 根描述符指针，指向 RSDT/XSDT；`DTB` 是设备树 blob。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是 2.3 那张参数表在 Rust 侧的落点，生产侧由 `minix_boot::normalize_param_buf` 统一规范化：输入按 NUL 或 ASCII 空白分词（UEFI 记录以 NUL 分隔，DTB `bootargs` 以空格分隔），逐条要求可打印 ASCII 的 `key=value`；非 ASCII、无等号、空键与内部键（`ARCH`/`BOARD`）逐条拒绝；`ARCH` 由编译目标生成，`BOARD` 只有真实来源才生成（当前生产调用没有可靠来源，显式传 `None` 省略，不伪造——测试 `generates_arch_and_board_first` 传显式板名验证的正是"有来源才生成"这一支）。

##### 3.2.2.4 param_buf 的十条可观察分歧

与 C 的可观察分歧共十条，按三档术语纪律统一标注 `[ARCH: 参数表语义规约化；十条可观察分歧见下]`：

- 其一，重复键——C 的"已存在先删"只对首条记录生效（§2.3 的源码推演），但结果分两支：若重复键的首个副本是第一条记录，旧条被删、新值追加，**后写胜出、只剩一条**；若首个副本不是第一条，旧条留在原位、新值追加到缓冲末尾，账上两条而读取返回第一条。Rust 统一为首条生效、后者逐条拒绝，因此在"重复键是第一条"这一支上与 C 相反（C 后写胜出），属于新增的可观察分歧，不是"C 缺陷语义的载体重构"。同名键的整条删除只对首条记录成立；非首条记录还可能被中段匹配误删（§2.3 的 `xboard=2` → `xarch=i386`），这条不改变上面两支的结论，但"非首条"不等于"完全不受影响"。
- 其二，命令行里的 `ARCH=`/`BOARD=` 词——C 不拒；若外部写的是小写 `arch=`/`board=`，首条时会被内核随后写入的同名键覆写，大写拼写则与 C 的生成键不同名、不会被覆写。Rust 一律拒绝外部输入覆盖内部键，检查大小写不敏感（小写 `arch=`/`board=` 同样被拒，`rejects_internal_key_override` 覆盖）。
- 其三，`BOARD`——C 无条件写一条记录，但读取返回首条匹配、可被外部同名键遮蔽，因此不保证生效（§2.3）；Rust 按来源生成、无来源则省略（上文）。
- 其四，容量耗尽——Rust 一条放不下就丢弃它及其后全部记录，C 只丢当前条，后面的词逐条再试——短的仍能收下，长的仍会被丢（`drops_tail_records_whole_at_capacity` 锁住前者）。
- 其五，内部键拼写——Rust 生成大写 `ARCH=`/`BOARD=`，C 写小写 `arch=`/`board=`。
- 其六，分隔符集合——Rust 按 NUL 与 ASCII 空白（含 tab/CR/LF）分词，C 只按空格，同一 `bootargs` 可得到不同条数（`splits_nul_and_whitespace_records` 锁住前者）。
- 其七，字符集——Rust 拒绝非 ASCII/不可打印记录，C 原样存（`rejects_malformed_and_non_ascii`）。
- 其八，空键——Rust 拒绝 `=value`，C 存成一条 `=value`。C 的删除循环在 `namelen = 0` 时 `strncmp(p, "", 0)` 恒真，`p[0] == '='` 可能命中的是**下一条**记录的等号，于是同名键删除会截断后续记录的值（字节推演见 §2.3 的删除扫描段）；这个副作用只在空键输入下出现，Rust 直接拒绝该输入，因此没有对应路径。
- 其九，`ARCH` 取值——C（i386 路径）写 `arch=i386`，ARM 侧写 `arch=earm` 一类（§2.7）；Rust 写 `ARCH=x86_64`（编译目标）。
- 其十，记录顺序——C 先放命令行记录、后写内核生成键；Rust 生成键在前、外部记录按输入序在后。这十处都落在 `GET_MONPARAMS` 的 wire 上可观察；仓内消费者核查：Rust 侧 `os/kernel` 与 `minix-boot` 没有把 `BOARD`/`board` 当板卡身份读取的消费者——命中只在 `boot_params.rs` 的 `fn normalize_param_buf` 两处（生成内部 `BOARD=` 与拒绝外部同名键）以及测试字面量；C 内核的 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围。参数源分两类，按 §3.6 的三类链落到各腿：两条 UEFI 链读当前镜像的 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`；U-Boot shim 与两个直核面（按 §3.6 归 U-Boot 一族与直核面两类）读 DTB `/chosen/bootargs`。消费者 `GET_MONPARAMS` 仍按 C 的 1024 字节定长零填充；没有外部输入时缓冲里只有生成的那条 `ARCH=...`——与 C 的 `mb_set_param` 必写行为仅就 ARCH 一项同形。

#### 3.2.3 进程内结构不做 ABI 与 arena wire 协议

> [参考] 版本变迁：现役信封版本是 10；下面 `7→8`、`8→9`、`9→10` 只用于解释 wire 形状为什么变化，不影响现役 10 字段/六分区的读法。

`KernelInfo`、`MemoryRegion`、`BootModule` 都不使用 `#[repr(C)]`：字段顺序与切片内部 `(ptr, len)` 的顺序都不是契约。`PlatformDescSource` 是例外：它用 `#[repr(C)]` 只为钉住 16 B 句柄形状，字段顺序仍不是跨镜像 ABI（wire 用 `WirePlatformDescSource`）。它们只作为接收侧进程内结构使用（内核 `.bss` 副本、QEMU 测试内核；五条腿的发送侧只构造 `BootReport<'_>` 借用视图），不承担跨镜像布局契约。

跨镜像三腿（两条 UEFI 产线 + riscv64 U-Boot shim）改用独立协议 `BootHandoffV1`：标量全部写成固定宽度，载荷放进一个八页 arena（arena：boot-shim 预留的一块定长物理内存，里面摆着 v1 交接协议的信封与各载荷分区，分区布局见本节末）的固定分区，描述符只记 `byte_len`——地址由 arena 基址加协议偏移推导，`(ptr, len)` 不再上线。

协议有两层版本号，必须分开：**协议身份**恒为 v1（`BOOT_HANDOFF_V1_MAGIC`/名字里的 v1，指"第一代交接协议"这个族）；**信封版本号**（`BOOT_HANDOFF_V1_VERSION`，当前 10）随信封或 arena 的 wire 布局变更递增、收发双方精确比对——本篇出现的"7→8"、"8→9"与"9→10"都是后者（"wire 版本"是同一计数器的另一个叫法；7→8 删 `bootstrap_start/len` 与 `free_upper_idx`，8→9 删 arena 的语义工作分区并把 source 分区前移，9→10 删三个标量与 startup 分区）。本篇不带限定词的 `v9`/`v10` 一律指 wire 信封版本；`VmBootHandoff` 的版本写 `handoff vN`。同样，`source` 有两个限定用法：`source_records`/source 分区指出内存图来源记录（provenance），`platform_sources`/平台源指 DTB/RSDP 描述符来源；单说 source 时按上下文取前者，不用裸 source 指平台源。

##### 3.2.3.1 wire 字节布局（v10）

信封 `BootHandoffV1` 固定 128 B，字段偏移由 `handoff_v1.rs` 的 `envelope_offsets` 用 `offset_of!` 派生；下表是当前 v10 的读数。所有整数小端；`u64` 8 字节对齐，`u32` 4 字节对齐；偏移都相对 arena 基址。

| 偏移 | 宽度 | 字段 |
|---|---|---|
| 0 | 4 | `magic` |
| 4 | 4 | `version` |
| 8 | 4 | `size` |
| 12 | 4 | `ram_authority`（0 = DTB，1 = UEFI） |
| 16 | 8 | `arena_len` |
| 24 | 8 | `kern_virt_base` |
| 32 | 8 | `kern_phys_base` |
| 40 | 8 | `kern_size` |
| 48 | 8 | `root_page`（运行期补） |
| 56 | 8 | `bump_base`（运行期补） |
| 64 | 8 | `bump_next`（运行期补） |
| 72 | 8 | `bump_end`（运行期补） |
| 80 | 8 | `memmap_byte_len` |
| 88 | 8 | `reserved_regions_byte_len` |
| 96 | 8 | `boot_modules_byte_len` |
| 104 | 8 | `platform_sources_byte_len` |
| 112 | 8 | `param_buf_byte_len` |
| 120 | 8 | `sources_byte_len` |

信封占 128 B，`ENVELOPE_CAP` 为 256 B；之后是固定分区（偏移/容量/元素）：

| 分区 | 偏移 | 容量 | 元素宽度 |
|---|---|---|---|
| `memmap` | 256 | 128 项 | 16 B |
| `reserved_regions` | 2304 | 256 项 | 16 B |
| `boot_modules` | 6400 | 12 项 | 32 B（16 B 名字槽 + start + len） |
| `platform_sources` | 6784 | 4 项 | 16 B |
| `param_buf` | 6848 | 1024 B | 字节 |
| `sources` | 7872 | 296 项 | 72 B |

`WIRE_END = 7872`，`WIRE_SOURCES_END = 29184`；arena 总容量 `ARENA_CAP = 32768`，尾部设计余量 3584 B。描述符只记 `byte_len`，地址由“arena 基址 + 上表偏移”推导，wire 上没有任何指针。任何字段或分区变化都必须升 `BOOT_HANDOFF_V1_VERSION` 并同步收发两侧；当前信封版本是 10。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

arena 的分区是固定偏移：信封住首部，其后依次是内存图、保留区、模块、平台源、参数字节的 wire 形式；v9 之后 arena 里**没有**第二份语义表——发送侧的分类结果同时写进它自己的 `BootStorage`（调用方持有的定长数组）和 wire；UEFI 腿的 source 分区例外，只写 wire，`BootStorage` 的 source 切片为空；U-Boot shim 的 DTB source 记录仍经 `fill_source_records` 写 `BootStorage` 的 40 槽，再序列化进 wire（见 §3.7/§4.5）。接收方再按 `byte_len` 把 wire 复制进自己的 `.bss` 落地区。这样 arena 只有一份权威表示，发送侧构造进程内视图时借用 `BootReport<'a>`（`BootStorage` 的视图），不再用 `'static` 切片指向 arena。平台源 **wire 记录** 是 16 B 的 `kind u32 | len u32 | addr u64`：`len` 复用旧 `reserved` 槽位，旧生产者写 0 表示未知、新读者回退单片；因此该记录仍是 16 B，v10 的版本号变化只来自 §3.2 列出的四个成员。接收方先校验 `magic`、`version`、`size` 与 `arena_len`，再按各 `byte_len` 把六个分区复制进自己的 `.bss` 落地区；越界、不能整除或容量不足都在构造任何引用之前失败。运行期字段（根页表物理地址与 bump 三件套）在 shim 建好根之后原地补进信封，因此三腿都只需传 arena 基址一个数（入口寄存器见 §4.1）。

#### 3.2.4 深拷不变量与真机探针

**不做深拷、把 `KernelInfo` 的切片按原指针存进内核的全局槽会怎样**。两条 UEFI 产线的六个 wire 分区都写在 arena 的固定偏移上；发送侧的语义表在 `BootStorage`，底层字节来自 Boot Services 池或 arena。无论哪一条，接收方都必须在首次进程根切换之前把值搬进自己的 `.bss`——arena 只在恒等窗口存活，池页在历史真机上还出现过零化。

真机记录给了这一现象的两组读数（`[观测]`），零化时点不同：`res=127 zeroed=127`（`res` 是到达分类点的 `reserved_regions` 条目数，`zeroed` 是其中因长度读回零而被拒的条数——127 对 127，也就是全部读成了零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 输出里看不到。两组读数指向同一个设计结论：值必须在入口尽早深拷（`[预防性不变量]`）。

机制上有两条候选。**候选①（页表不重放）**——源码注释给过的一条是“内核自己的页表不重放这些堆页的 1:1 映射”（[`globals.rs`](os/kernel/src/globals.rs:static RESERVED_REGION_STORE) 的容量注释）。**候选②（复用清零）**——“建立 Direct Map 覆盖时还完好”这组读数更像的“被当空闲复用后清零”。两条候选各带可检验的推论——"页表不重放"说与切表时点相关、"复用清零"说与快照/复用的时序相关——`test-kernel-map` 的 N-5 探针是 print-only，不参与 PASS 判据；候选①只按源码注释与人工观察保留、候选②只按读数形态保留，两者都未排除（`[观测]`；结论见 §5.3）。这条不变量与 §2.4.2.3 的“跳板生命周期”同级，都按 `[观测]`/`[预防性不变量]` 读。



### 3.3 两张内存表与来源记录（I2）

两张表不是同一种数据：`memmap` 是“可分配”的候选，`reserved_regions` 是“已占用/不可分配”的候选；同一个物理字节可以在不同阶段从一张表转到另一张表，但不能同时被两张表当成自己的。

`SourceRecord` 给每个投影字节记来源，共 11 个字段：身份组 `id`/`local_source`/`snapshot_id`（记录在输入对象内的编号、输入对象内下标、`GetMemoryMap` 快照号），范围组 `range`/`address_domain`/`virtual_start`（区间、地址域、固件的 `VirtualStart`），分类组 `kind`（`/memory`、MRB（device-tree memory reservation block，设备树内存保留块）、`/reserved-memory`、UEFI 描述符等；审计里的 `kind = Memory` 指 `/memory` 这一类，UEFI conventional 描述符按同一 kind 归类）、`origin`（节点/条目/描述符下标）、`input_object`（输入对象），以及固件原值组 `attribute`（描述符 `Attribute`）与 `raw_type`（固件自己的类型值，与 `kind` 的角色分类不是一回事）。`RamAuthority` 把“这份内存图由哪个输入对象声明”从 `InputObjectId` 里分出来：只有 DTB 与 UEFI 快照两种取值，`StartupObjects` 已随 v10 删除。构造 `KernelInfo` 时 `authority` 与 `source_records` 一起传入；`validate_requiring_sources` 要求 authority 对应的记录覆盖投影。

覆盖审计是 I2 的判据：free 表的每个字节必须能被 `kind = Memory` 的记录解释；reserved 表的每个字节必须能被 reservation 类记录解释。在生产腿的 strict 门里，说不出来源的字节不是“忽略”，是 fail-closed；非 strict 的 `KernelInfo::validate` 只在 `source_records` 非空时复核（§3.2.1）。库函数 `minix_boot::memmap_source::audit_source_coverage`/`audit_source_coverage_for` 是这层检查的两个入口；`KernelInfo::validate_requiring_sources` 在 strict 分支里调用带 authority 过滤的后者，再叠加模块名/位置与内核跨度覆盖。C 侧没有这个结构：C 的 `memmap` 条目只有基址、长度与一个固定为 `AVAILABLE` 的类型标签（`multiboot_memory_map_t.mm_type`），没有“谁声明了这段”的来源记录，来源隐含在“引导器给的”这个前提里；Rust 把它显式化：跨镜像腿的接收侧无法再问引导器，直核腿虽然同镜像，也保留同一份记录以复用同一覆盖审计。

### 3.4 bootstrap 区间、分配器换手与页目录槽位

这一节回答三个彼此相邻但不同的问题：C 的 `bootstrap_start/len` 为什么在 Rust 侧被删、页表页分配器怎样从 shim 换手到接收镜像、`free_upper_idx` 的借槽机制为什么在 64 位退场。三者共同的主题是“进入高半前后，资源和槽位归谁”；阅读顺序先看删除判据，再看换手表，最后看槽位借用链。

#### 3.4.1 `bootstrap_start/len`：从可回收区间到字段删除

> [参考] 设计变迁：本节解释这两个字段为什么被删除；现役 `KernelInfo` 已没有它们（§3.2.1）。判据结论在段末。

C 的这两个字段装的是链接脚本导出的一小段内核镜像内区域的边界（2.3 已看到赋值）。这段区域的设计性质可以压成四条结论（行级细节见第四章与 02 篇）：

- `kernbase`/`kernsize` 来自链接脚本的三个基址赋值，也是 02 篇讲 `AT()` 的两条基址。
- `_kern_size` 不是字面量而是布局算出的符号：`.bss` 收尾处 `__k_unpaged__kern_size = . - _kern_vir_base`（整幅镜像的虚拟跨度），随后 `_kern_size = __k_unpaged__kern_size` 把它导出成 §2.5 引用的那个名字——它的值随每次构建的段布局而定，没有固定数字。
- 链接脚本让第一段的位置计数器落在物理基址上；`unpaged` 三个段（来自文件名匹配 `unpaged_*.o` 的目标文件）放在这里，它们的**执行地址等于装载地址**（没有 `AT()` 偏移）。
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。用一对起止符号而不是单一起点，是因为下游要的是半开区间 `[start, end)` 的长度——回收接口 `add_memmap` 收 `(addr, len)`，只有起点算不出长度。C 代码里的引用名没有双前缀（`_kern_unpaged_start`，§2.3 的 `get_parameters` 引文可核对）；构建期 `objcopy --prefix-symbols=__k_unpaged_` 把它改写成 `__k_unpaged__kern_unpaged_start`，与链接脚本定义相遇——`k_initial_stktop` 的构建期前缀规则是同一机制的另一例，其落点见 02 篇 §2.4。
- 链接脚本在 unpaged 段之后加上 `_kern_offset`，其后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。两个地址必须分开的理由：引导器只按装载地址把字节写进物理内存，而分页开启后 CPU 按执行地址取指；`AT()` 就是链接期把同一个段的两个地址钉在一起的方式。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后不再需要"的代码与数据（它的执行地址等于物理地址，靠恒等映射就能访问）：`kinfo` 已在 `kmain` 开头拷进 `.usermapped` 副本，活跃的 `pagedir[]`/静态页表池在高半 paged `pg_utils.o` 副本里，段内留下的是 unpaged 副本。C 在 `kmain` 后段（`system_init` 之后、`bsp_finish_booting` 之前——精确锚点见 §2.5.1 的"还引导段"行）把它 `add_memmap` 归还给通用分配器，且归还发生在 `kernel_may_alloc` 写 0 之前，不撞 §2.4 那组断言。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上生产内核镜像（两条 UEFI 产线 + U-Boot 接收镜像 + riscv64 直核生产镜像 + aarch64 直核镜像 `fw-aarch64-none`）的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外；QEMU 直核测试内核按 VMA=LMA 平铺，见 02 篇的 4.5 节，不在这条陈述内）；

**实际执行地址**上，两条 UEFI 链的 boot-shim 建好根、开分页后跳进独立 `kernel.elf` 的高半入口，内核从第一条指令起就在高半（§3.6）；riscv64 的自引导链则起步于物理视图（medany——RISC-V 默认的 PC 相对寻址代码模型，代码按"装在哪就按哪解释地址"的方式编译——的平移视图，见 02 篇的 4.5 节），高半地址要等开分页之后才被解引用。

**判据结论**：当前所有生产链接布局（两条 UEFI 链、U-Boot 接收镜像、riscv64 与 aarch64 生产直核镜像，均为高半 VMA）与 VMA=LMA 的 QEMU 测试内核都不存在"分页开启前执行、之后可归还"的**内核段**。内联形态已退役，不在本条判据范围。正面写法：没有一段是"在链接布局上可单独界定、且分页后不再需要"的；`bootface` 代码虽然只在分页前执行，但它是内核镜像的一部分、没有独立链接边界，不能单独归还。因此这个字段对在 Rust 侧没有语义内容——它随后的处置（先写死为 `(0, 0)`、最终整个删除）都从这一句来。

这个字段对在 Rust 侧的处置走过两个阶段，最后停在**删除**：先是把 `(0, 0)` 写死并配一条硬断言（"必须为 0"），字段删除后离开 `KernelInfo`，wire 版本 7→8：本字段对释放 16 字节，同版本删除的 `free_upper_idx` 再释放 8 字节，合计 24 字节，envelope 从 184 缩到 160（该字段的删除论证见本节下文的过度回收风险表；`free_upper_idx` 的删除论证见 §3.4.3）。为什么"写死 + 断言"还不够、要连字段一起删：`bootstrap_start/len` 表达的其实是**一个布尔事实**（"存在分页开启前专用、之后可回收的段"——Rust 侧恒为否），却占两个整数、两个 builder 参数和 16 字节 wire；所有生产调用点都传 `(PhysBytes(0), 0)`，唯一的"读者"是那条断言自己。更重要的是保留字段就保留了一个误设的接口面——`validate` 的断言只能把非零值拦在 `kmain` 入口，拦不住"合法类型、错误语义"的值进入类型系统。误设的后果是本篇推演过的真实事故来源：

这不是纯假设：曾有一次把 `bootstrap_len` 填成 `kern_phys_base.0`（`[观测]`，仅留作反事实推演的引子）。先把时刻说准：误设发生在直核交接载荷的构造点，那一步只是把 `[0, kern_phys_base)` **加进空闲清单**，不写任何内存；真正会覆盖这些区间的是之后的取页者（内核的 `boot_alloc`、以及 VM 接手后的分配器）。因此下表是**反事实推演**：它列的是"如果这块区间之后被分配出去，谁会先读到坏数据"，不是同一时刻四行同时发生。下表列的是那次误设会扫进的同一区间 `[0, kern_phys_base)` 里的四类内容（不是互斥分区）：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0`（下表以 riscv64 链为例：`kern_phys_base = 0x80200000`；OpenSBI 行有地址锚点，其余三行是当时部署的观察，具体地址未记录） | 那块内存实际存着什么 | 误回收后先坏在哪（按"取页时谁还活着"读） |
|---|---|---|
| OpenSBI 区间 `[0x8000_0000, 0x8020_0000)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 内核接手后仍会调 SBI；下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址。这是四行里唯一"取页时仍然活着"的读者 |
| DTB 所在区间 | 设备树 blob；boot-shim 用它建内存表、读 `/chosen/bootargs`，并把物理地址作为 `PlatformDescSource` 传给内核 | 内核在 kmain 之后解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇）；若 DTB 在取页前已被解析完，则这一行不构成先坏点 |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件；表本身是 shim 镜像 `.bss` 的一部分（`build_shim_table` 构建），不是预载物 | **不构成先坏点**：shim 交出控制权后不再从表里取文件；这段区间被覆盖的后果只是"若回跳/重入 shim 会读到垃圾"，正常启动路径不会发生 |
| boot-shim 自身 | shim 的代码与栈（仅在 shim 镜像落在 `[0, kern_phys_base)` 的部署里） | **不构成先坏点**：取页发生在内核接手之后，那时 shim 已不再执行；直接取指失败只在回跳/重入时发生。若某次部署里 shim 仍在执行期间就被取页，才属于这一行的原描述 |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写进空闲清单"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。诚实边界：OpenSBI 行有 `[0x8000_0000, 0x8020_0000)` 这个地址锚点；DTB/U-Boot/shim 三行的具体物理地址当时没有记录，只能按"那次部署把三者放在内核物理基址以下"的观察来读，不要把它们当成可复算的地址证据。表中"不构成先坏点"的两行仍然留在表里，是因为它们说明误回收的**区间范围**；它们不承担"四类内容都会先坏"的结论。

这张表也解释了那次险情的机制：把"上界地址"当"区间长度"，`add_memmap(0, kern_phys_base)` 会把上表所列的全部固件结构扫进"可回收"———早期直核构造点（riscv64 链）曾有一条注释（字段删除时随参数一起删除），原文（源码里分两行，这里合并）把自己的旧取值记为险情来源：`the previous \`PhysBytes(0) + len = kern_phys_base.0\` was a dangerous over-reclaim that swept up OpenSBI firmware, DTB, U-Boot image, and the boot-shim itself`（x86_64 链上同一个误设的区间是 `[0, 0x200000)`，即 [`x86_64.ld`](os/kernel-image/x86_64.ld) 的 `KERNEL_PHYS_BASE`）；现仓在 `opensbi_helpers.rs` 的测试模块保留一句"结构上不可能"的回归说明；当前 `kmain` 的早期校验段已不再要求 `bootstrap_len == 0`（该字段已不存在），早期实现的取证注释仍提到该字段。那次险情靠人工改值解决；字段删除之后，"提供一个可误设的回收区间"这个接口面整体消失，险情在类型上不可复发。C 侧的归还机制照旧运转（`add_memmap` 对零长请求静默吸收的语义，2.4 已见），本篇不再为它携带任何 Rust 侧对应物。
#### 3.4.2 引导期页表页分配器的换手

§3.6 的五条生产腿与交接形态定下来之后，这一节看引导期页表页分配器怎么在这两个镜像之间换手。C 的**页表页**来源是 `alloc_pagetable` 的 6 页静态池，用完 panic（02 篇的 2.4 节）；`pg_alloc_page` 从可用内存记录的最高条目尾部取页，服务的是 4KB 被映射数据页，不是页表页（§2.4.2/02 篇的 2.4 节）。页表分配策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

**把 C 的静态页表池原样搬到 64 位会怎样**（保留"6 页，用完 panic"）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 的一个 4 MB 目录项只需要一个页表页，6 页对常见的小范围映射够用；跨 4 MB 边界或分散映射会多占页，这一点在下面的 64 位推演里放大。4 级页表下一次 4KB 映射要穿过 PML4（x86-64 四级页表的顶层，§3.4.3 详述）→ PDPT → PD → PT 四级，其中下面三级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页按每个 PML4 槽最多 3 页算只够两个槽，第三个槽（第二次跨 PML4 边界）就会耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

Rust 侧把**页表页**分配抽出来（`pt_alloc.rs` 的 [`fn register`](<os/arch/src/arch/pt_alloc.rs:fn register>)、[`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>)）；4KB 数据页的运行期分配归 VM 物理分配器，不在这个槽里：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

boot 阶段的实现是一个 bump 分配器（`boot_alloc.rs` 的 [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:fn alloc)）：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

换手由一次初始化、三次 `PT_ALLOC` 注册与一次 VM `PT_FREE` 归还钩子注册组成（下表三行：第 1 行是初始化、不注册；第 2 行里两个二进制各注册一次，共两次；第 3 行一次；以下按跨镜像腿计，直核面没有 shim，只有接收镜像的一次注册），各有锚点：

| 时刻 | 谁写这个槽 | 代码 | 依据 |
|---|---|---|---|
| shim 进程内、进入内核镜像之前 | 初始化 bump 池（**不写注册槽**） | `boot_alloc::init_boot_pt_alloc(bump_base, bump_end);`（U-Boot 腿写成 `result.bump.start()`/`result.bump.end()`） | [`uefi_helpers.rs`](os/boot-shim/src/uefi_helpers.rs)、[`uboot_main.rs`](os/boot-shim/src/uboot_main.rs) |
| shim 进程内 Step 0 与**接收镜像**的 `resume_additive`（同一 helper `boot_validate_and_prepare`） | 只有"未注册"时才写——每个二进制各有一份 `PT_ALLOC` 静态（`AtomicUsize`，0 表示未注册），接收镜像的这份在它自己的 `.bss` 里首次注册。shim 侧不直接调 `register`，而是在 `boot_validate_and_prepare` 内经 `is_registered()` 守卫间接注册；两次注册不冲突：UEFI 腿写的是 boot-shim 的那份、接收镜像写的是自己的那份，互相独立 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | [`vm_server.rs`](os/servers/vm/src/vm_server.rs) |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；shim 侧不直接调用 `register`，`init_boot_pt_alloc` 只设定 bump 区间，`boot_validate_and_prepare` 在 shim 进程里经 `is_registered()` 守卫间接注册。这个槽是“写一次”的，重写是 bug，因此没有“后注册者覆盖前者”的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池。C 这里有两套池，不能混：内核自举页表的 6 页静态池（`i386/pg_utils.c` 的 `alloc_pagetable`，发完 panic、从不归还）与 VM 侧用户页表的池（`servers/vm/pagetable.c` 的 `pt_free` 把页还回 VM 物理分配器所用的同一池，经 `free_mem`）；Rust 的注册槽同时服务内核自举页表（`boot_pt_alloc`，经 `boot_validate_and_prepare` 守卫注册）与 VM 页表（`vm_pt_alloc`）；`register_free` 只对应 VM 侧池。两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`VmBootAllocator` 是 VM 侧的启动帧分配器：从每个池区间末端向下消耗，方向与从起点向上的 `BootAlloc` 相反）（`vm_handoff.rs` 的 [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) 调用的 [`fn classify`](<os/kernel/src/vm_handoff.rs:fn classify>)）；引导期建表之后的 Direct Map 覆盖建立则在 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs)：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在本节末的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（就是 2.4.2 表里的"还 VM 镜像"那一行），而**实现**把**所有**启动模块的字节区间都算作内核保留。表头把两份清单分开陈述：扣除记录含 VM 自己；交接模块清单（`VmBootHandoff.modules`）排除 VM 是 Rust 侧的选择（C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零，见 §2.4.2）。理由是启动顺序里的一个 Rust 侧前提：**模块字节留在 blob 原地，VM 启动那一刻才被搬进自己的地址空间**——C 侧不同，C 的注释写明回收发生在 "just copied into existence"（复制完成后），所以 C 能在复制后归还而后续装载只伤到无人再读的 blob 原件。Rust 侧复制被推迟到 VM 启动，清单就必须从任何搬运发生之前就把全部模块 blob（含 VM 自己的）划出：内核的 boot-exec——`kmain` 的引导执行阶段，逐个 `exec` 启动镜像里的服务进程——在 VM 服务起来之前只装载 VM 一个服务器（`EntrySpec::loaded`）；其余启动服务是 `EntrySpec::DEFERRED`，由 RS（重生服务器）运行期 EXEC 装载——它们的段帧同样取自这份自由清单。若按 C 的时点归还 VM 的 blob，后续装载与 EXEC 的帧分配可能盖掉尚待搬运的 VM 镜像字节。

**改为按"已用部分"扣除会怎样**（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(storage, 1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时（调用点可以是 `arch_boot_impl` 的 Step 2/2c 设备窗口，或 Step 4 建立 DM 覆盖）`BumpRegion::alloc_pages(1)` 从 `0x1E64000` 切走一页并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）。这些调用点都在 VM 首次发布自由页之前，所以这次写零不会覆盖 VM 已分配的页；真正的问题是**账本边界**：按已用部分扣除把 bump 区间的剩余部分交给 VM，而 bootstrap 路径仍持有 `boot_alloc_region()`，在 `is_registered()` 为真（`PT_ALLOC != 0`）时仍可能从同一区间取页（例如未来新增的启动期消费者），两者对“这块内存归谁”没有共同的截止点。

整段扣除是一个保守的截止点：bump 池整体不进入 VM 自由清单，代价是**VM 发布前未被接收侧取用的尾部页**（已被接收侧用作页表页的部分仍在使用；整段扣除只是不让 VM 碰这段区间），收益是不需要证明“所有 boot 消费者都在 VM 发布前结束”。
#### 3.4.3 `free_upper_idx` 与页目录槽位：字段、借用链与 64 位做法

页表页由谁发说完，剩下的是页目录槽位的账——C 的 `freepde_start` 是 02 篇的 4.3 节 那条借槽链的起点：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy_f` 借窗口做跨地址空间拷贝。

这套机制的三个前提（完整陈述见 02 篇的 4.3 节）在 64 位侧以三种方式不再必要：① 内核不再借当前页表的槽位，Direct Map 窗口是固定虚拟基址到物理地址的线性映射，不需要“空槽”；② Direct Map 不依赖“当前 CR3/当前 satp 指向哪张根”这个可借用槽位——三架构都实现 `Paging::inherit_supervisor_half`，把源根的高半条目复制进新进程根（三个架构专用名先在这里定义：TTBR1 是 AArch64 承载高半地址翻译的页表基址寄存器；Sv39 是 RISC-V 的 39 位三级页表方案；satp 是 RISC-V 的根页表基址寄存器）：aarch64 的 L0 上半 256–511 项就是 TTBR1 区（[`arm64/paging.rs`](os/arch/src/arm64/paging.rs:fn inherit_supervisor_half)），x86-64 复制高半 PML4 槽位（[`x86_64/paging.rs`](os/arch/src/x86_64/paging.rs:fn inherit_supervisor_half)），riscv64 复制 Sv39 L2 的 256–511 项、satp 切换整根但每张进程根都携带内核半区（[`riscv64/paging.rs`](os/arch/src/riscv64/paging.rs:fn inherit_supervisor_half)）；③ 没有借槽动作，就没有归还义务——跨地址空间拷贝由内核用 Direct Map + 软件页表走查完成（`SYS_VIRCOPY`/`cross_space_copy`），不再借当前页表的槽位。

64 位侧跨地址空间访问换了载体：内核有 Direct Map 窗口（一段固定虚拟基址到物理地址的线性映射，由 `arch_boot_impl` 的 Step 4 建立），加上软件页表走查（[`pte_walk.rs`](os/kernel/src/pte_walk.rs) 一类的实现）就能在**不切页表、不借槽位**的前提下读到任意进程的页表项；用户空间的拷贝则由 VM 服务在自己的地址空间里处理。于是“借用空闲槽位”的三个前提在 64 位侧都不再是必要结构。下文用 `inherit_supervisor_half` 展开的 x86-64 反例是**逐条可对照的一支**；aarch64 与 riscv64 的对应实现已在上一段给出锚点，三支走同一个 trait 方法。

这个值先是被保留成 `Option<usize>` 跨 handoff 携带（`None` = boot-shim 未计算），报告面用 `unwrap_or(0)` 抹平。那一步的教训值得留一句：`Option` 能区分"没有值"（`None`）与"某个值"（`Some(0)`——"0 号槽位空闲"，会被下游当成有效索引），却区分不出"算对的槽号"与"算错一位的槽号"——在没有真实计算者时随便填一个数，只会把后者伪装成前者。

字段删除把权衡推到底：既然所有生产者都写 `None`、唯一消费者把 `Option` 归零，"跨 handoff 携带"就没有携带任何信息——字段从 `KernelInfo` 删除（wire 版本 7→8 的字节数变化见 §3.4；信封字段的完整偏移表在 `handoff_v1.rs` 的 `envelope_offsets`，本篇不复制）。报告面保留 C 的布局位：[`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` 固定填 `0`（C `KinfoStruct.freepde_start` 字段不动 48 字节 wire；GET_KINFO 载荷的收窄演进见 §3.2 的 `[ARCH]` 标注）。64 位侧报告 0 是诚实的——没有借槽机制，"第一个空闲槽位"这个概念本身不存在；C 侧那套机制里偏移一位的后果（改写内核映射、下一次访问页错误或静默读错）见 02 篇的 4.3 节。

对照表：

| 维度 | C 的槽位机制 | Rust 的窗口机制 |
|---|---|---|
| 借用者 | `createpde`（跨地址空间拷贝） | Direct Map 窗口 + 软件页表走查 |
| 借用范围 | 当前页表的 4MB 窗口（一个页目录项） | 内核固定映射窗口（DM 资格过滤后的可达物理区间） |
| 归还时机 | 下一次切换地址空间时自然失效 | 不需要（窗口是常驻映射） |
| 失效条件 | 借来的条目只描述对方的一个 4MB 块 | 窗口覆盖范围由 DM 建立时的资格过滤决定 |

**在 64 位侧照搬"借用空闲 PML4 槽位"会怎样**（PML4：x86-64 四级页表的顶层——页映射表第四级——每进程一个 512 项的顶级目录，上文 C 4MB 单级页目录在它这里对应的是这个角色的放大版）。每进程页表的高半槽位是从 bootstrap 根继承的（`X86_64Paging::inherit_supervisor_half` 把源根的 PML4 的 256 至 511 号槽位（即半开区间 256..512，共 256 项）原样复制过来）。被借用的槽位在 `switch_address_space` 之后属于另一个地址空间：借出去的映射在下一次切换时静默失效，不会报错。症状是跨空间拷贝偶发读到"上一张页表留下的地址"——一个只在特定调度顺序下出现的错误，而不是一次可复现的崩溃。
### 3.5 取件抽象与 crate 依赖方向

这一节把“取件”从引导形态里抽出来：哪些代码属于文件读取、哪些属于 ELF 解析、哪些属于 crate 依赖方向。先给 `FileLoader` 这个测试接缝，再给四份依赖表证明内核不反向依赖 boot-shim 可执行体；引导形态本身不在这里合并。

#### 3.5.1 取件抽象：一条普通函数，差异收在 `FileLoader`

本节起反复用"腿"：**一条腿 = 一个完整启动路径（一个引导面 + 它交付的内核镜像）**；腿、链、形态、交接路径的计数口径不同，对照表在 §3.6。

C 里"同一串函数名、两套实现"的机制见 §2.7：两种架构各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

U-Boot 腿的引导准备入口是普通函数 `prepare_boot`，不使用单实现 trait 做抽象。理由有两条：没有 `dyn` 分派，`prepare_boot` 是自由函数，不参与对象分派；测试接缝实际由 `FileLoader` 提供，不是任何引导 trait。单实现 trait 既不是接缝，也不是已经验证过的扩展点，留着只会让"换固件"的承诺看起来比证据强。

现在 U-Boot 腿的入口是一条普通函数：[`fn prepare_boot`](<os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot>)，由 [`fn uboot_main`](<os/boot-shim/src/uboot_main.rs:fn uboot_main>) 直接调用，签名是 `prepare_boot<'a>(&'a mut BootStorage, bump_pages) -> BootPrepareResult<'a>`。UEFI 生产入口 `prepare_and_enter_kernel_image` 仍是独立入口（§3.6）；两个直核面复用 opensbi_helpers 的库函数，同样不经过它。未来若真出现第二条固件腿，等第二个实现落地时再按两个真实实现提炼接口，而不是先猜一个。

```rust
// 节选（`report` 字段后的中文旁注为编者加注），os/libs/minix-boot/src/boot_prepare.rs:struct BootPrepareResult。
pub struct BootPrepareResult<'a> {
    pub report: BootReport<'a>, // 〔编注〕借用调用方 storage 的语义视图
    pub root_page: PhysBytes,
    /// Bump region `[start, end)` (physical).
    pub bump: PhysRange,
    /// Handoff arena base (physical), allocated from the same bump pool after
    /// the module landings.
    pub arena_pa: PhysBytes,
}
```

`bump` 是只前进不回收的线性分配器；`bump_pages` 是给页表页预留的页数，`bump` 区间的 `start`/`end` 是该区间的物理边界（展开见 §3.4 与 §4.6）。

差异被压到一个更小的接口上（`loader.rs` 的 [`trait FileLoader`](os/boot-shim/src/loader.rs)）：两个走 `FileLoader` 的 boot-shim 实现族（UEFI 与 U-Boot；UEFI 族含 x86_64/aarch64 两条链）都要拿到"文件字节"，唯一不同的是字节从哪来。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

`read_required` 是**内核 ELF 取件**的致命落点：它把"文件不存在"从 `None` 变成一个带路径的 panic。启动模块走 §4.3 的 `read + unwrap_or_else(panic!)`，是第二个致命落点，错误消息格式与 `read_required` 不同；两者都是 fail-closed，`read` 本身只返回 `Option`，不决定致命性。两个实现各自只提供 `read`：UEFI 走 `SimpleFileSystem`（并把正斜杠转成 UEFI 要求的分隔符），U-Boot 走预先落在 RAM 里的 `BootFileTable`。除此之外的装载逻辑（解析、段拷贝、模块页分配、结构体构造）在两个 boot-shim 实现族之间只有一份；两个直核面共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`，`bootface.rs`/`bootface_a64.rs` 只剩本架构常量、入口横幅与各自的 console（下文展开）。两个实现都在 `read` 里只处理字节来源，错误折成 `Option` 交给各自的调用方。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

U-Boot 侧的实现见 §4.3 的 `entry_path_eq` 与 `UbootFileLoader`；两条 `read` 的差别在字节来源，U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度（kernel 槽 2 MiB、普通模块槽 1 MiB、`mfs` 专用 16 MiB；超过槽容量时打印 `truncating` 后按槽容量写入长度，即**截断后继续**，不是停机），loader 的二次修剪对已在槽内的输入是 no-op；非 ELF 或畸形输入由 `elf_file_end` 返回 `None`，调用方保留整槽长度，不做修剪；`read_required` 与全部装载逻辑完全共用。范围要说准：这里的“只有一份”指**两个 boot-shim 实现族之间**；两个直核面不走 `FileLoader`，但共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`；入口壳只负责常量、横幅与 console 选择。

`bootface_common` 不是只把函数搬了个家，它把两个直核面共同的模块放置约束集中到一处：`NAME_SLOT = 16`、`NAME_POOL` 是 12 个 16 B 槽，`place_modules` 检查条目数 ≥ 12（一条断言；有效条目不足时 fail-closed 返回 `None`）、每槽 basename 与 `NAME_POOL[i]` 相等、长度非零、源区间整段落在 memmap 的单个区间内、源区间不与 bump 池相交、目标页不落内核跨度；通过后才按槽分配 bump 页、拷贝、清页尾、写入名字池。

另有编译期 `const _` 把 `NAME_POOL[i]` 与 `boot_shim::loader::MODULE_NAMES[i]` 逐字节比较，两个直核腿的名字顺序漂移会直接变成编译错误。`PlacementPool` 存在的理由不是打包参数：五个同型字段（名字池地址、bump 起点、bump 终点、内核起点、内核终点）互换后仍能编译并产生“看似合理”的放置，结构体的命名字段把这类调用点错误挡在编译期。

两条调用腿的差异只落在 `PlacementPool`（名字池地址、bump 起止、内核跨度）与 `alloc_pages`/`say`/`say_hex` 回调上：riscv64 用 `0x8200_0000..0x8400_0000`，aarch64 用 `0x4400_0000..0x4600_0000`，容量同为 32 MiB。模块放置之外的流程（DTB 校验、memmap 收集、表址闸、bump 取页、平台源、参数缓冲、`KernelInfo` 组装）也在同一份 `run_self_boot` 里，两壳不再各存一份顺序相同的代码。

去掉 trait 后，有代码或计数证据的收益有两条：

- **可测试性来自 `FileLoader`**（代码证据，测试形态成立）：`FileLoader` 是 trait——[`loader.rs`](os/boot-shim/src/loader.rs) 的 tests 里有内存表实现 `MockLoader`（`impl FileLoader for MockLoader`），涉及装载的单元测试都经由它喂文件字节。
- **条件编译的落点收敛**（按构建形态成立）：**固件选择**相关的 `cfg` 只出现在 [`lib.rs`](os/boot-shim/src/lib.rs) 的模块与再导出声明处（7 处 `#[cfg(feature ...)]` 属性：6 条 `uefi` 加 1 条 `opensbi`）。另有一组 `qemu-test-kernels` gate（10 处）在 `uefi_helpers.rs`，只门控测试内核辅助物（`UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`、`classify_map_bytes_into_test_tables`、`exit_boot_services_and_classify`）；生产构建不链接它们。因此准确说法是"固件分支不在业务逻辑里"，不是"整个 crate 只有 7 处 feature 属性"。

另外两条只算单腿事实或设计承诺，不并入上面的收益：

- **入口形状固定**（只在 U-Boot 生产腿成立）：`prepare_boot` 的 `storage`/`bump_pages` 语义与 `BootPrepareResult<'a>` 的返回形状由签名钉住；但"统一"的对象只有这一条腿，UEFI 与直核面各有自己的入口，不能把它说成跨固件统一。
- **换固件不动内核**（设计承诺，观察，尚无自动化反例）：新增一种固件形态，改动是 boot-shim 侧的新入口或共享库调用，不需要预先实现一个 trait；`kernel.elf` 不必重新构建（两者分属 `os/kernel-image` 与 `os/boot-shim` 两个产物，这条分界的后果在 §3.6 与 02 篇的 4.5 节）。边界要说准：boot-shim 屏蔽的是**固件差异**，板级差异仍落到内核侧的平台发现（[`minix-platform`](os/libs/minix-platform/src/) 的 `PlatformDesc` 与内核侧平台源解析），不能把它说成"新板子只需写 boot-shim"。

这里故意不列"类型安全"与"封装"——这两条常被记在 trait 头上，实际来自 feature 门控：`uefi_helpers` 与 `opensbi_helpers` 各由 [`lib.rs`](os/boot-shim/src/lib.rs) 的 `#[cfg(feature = ...)]` 门控，开启哪个 feature 就只有哪个固件类型存在，**写错固件名本来就编译不过**。UEFI 生产入口也没有借 trait 做抽象：[`fn main`](<os/boot-shim/src/main.rs:fn main>) 按具体类型写——`prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`。这不是把固件写死在产品里：boot-shim 这个可执行文件只在 UEFI 形态下构建（bin 的 `required-features = ["fw-uefi-image"]` 隐含 `uefi`），固件选择发生在"构建哪个二进制"，不是运行期分派；opensbi 形态的生产路径是 riscv64 U-Boot shim（§3.1.1/§3.6）的 `opensbi_helpers::prepare_boot`；`kernel-image` 自引导面是辅助/嵌入式形态，复用同一组库函数但不经这份 main.rs。

**OpenSBI 路径的两种角色要分清**（接线状态与覆盖脚本的权威表在 §3.1.1 接线状态表，这里只讲与引导入口有关的部分；本篇用“OpenSBI 路径”指经 `opensbi_helpers` 的腿，§4.4 的字段表里特指 U-Boot shim 腿，不把两个直核面算进来）：U-Boot 路径复用 boot-shim 库根的 `BootStorage`/`BootReport` 与 opensbi_helpers 的 `BootFileTable`、`bump_alloc_pages`，并调用 `prepare_boot`；`kernel-image` 的直核自举面是另一条机器状态，不经它。角色一：U-Boot shim 的引导入口，调用 `prepare_boot`；角色二：`kernel-image` 的直核自举面，同镜像直接走 `arch_boot`，不使用这个入口。memmap 源统一下沉的裁决不合并引导形态层——独立 shim 与 bootface 自举对应不同机器状态，`prepare_boot` 只承担前者的固件差异。
#### 3.5.2 crate 依赖方向

C 的引导链同样不止一个二进制：BIOS 场景有 bootblock 与 boot2 两级引导器（[`boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c)），GRUB 场景由 multiboot 引导器代劳——引导器与内核不是链接成一个二进制，而是各自链接出独立的可执行文件，运行期只隔一条 multiboot 协议（寄存器递 `mbi` 指针）。内核侧的引导准备 [`head.S`](minix3/minix/kernel/arch/i386/head.S) 与 [`pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 则编在内核镜像之内，`pre_init` 填的 `kinfo` 本来就是内核自己的全局变量。所以 C 无需回答"引导层与内核谁依赖谁"：引导器与内核的边界由协议承载，协议两侧没有编译期关系。

Rust 侧的问题形态不同：boot-shim 独立成产物，却要与内核共享类型定义（`KernelInfo` 与交接的返回形状），这份复用由 crate 依赖表达——"谁依赖谁"第一次成为必须显式回答的构建问题，且答案必须单向：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

（图只画主边；条件依赖与 leaf crates 以下方四份依赖表为准，其中 `kernel-image ┄→ boot-shim` 仅 riscv64/aarch64 target 生效。`boot-shim → minix-kernel` 的调用面只有 `enter_kernel_image_v1` 与 `boot_alloc::init_boot_pt_alloc`；`arch_boot` 是 `kernel-image` 两个直核面的入口，不由 boot-shim 调用，图上的括号已按这个分工标注。两个名字看起来相近，但指的不是同一个东西：**boot-shim** 是可执行体 crate，**minix-boot** 是共享库 crate——本节论点里"内核不依赖 boot-shim"说的是不依赖前者这个*可执行体*；图中 `minix-kernel → minix-boot` 是对*库*的依赖，二者不矛盾。）

依据是四份依赖表（boot-shim / kernel / minix-boot / kernel-image 四个 manifest；命令：`sed -n '/^\[dependencies\]/,/^\[/p' os/kernel/Cargo.toml`（输出中无 `boot-shim` 也无 `uefi`，即证否定；全仓四 manifest 的依赖表见下方四行（`os/kernel` 的 `bitflags` 从略））。文中另提到的 `minix-elf` 是 boot-shim 的普通依赖，不单列成表）：

- `os/boot-shim/Cargo.toml` 的 `[dependencies]` 里有 `minix-types`、`minix-boot`、`minix-arch`、`minix-kernel`、`minix-plat`；`minix-elf`（共享的 ELF 解析器）；非可选的 `fdt`（U-Boot 腿解 DTB）；`uefi = "0.33"` 与 `uefi-raw = "0.9"` 挂在可选的 `dep:uefi`/`dep:uefi-raw` 上，由 `uefi` feature 一并打开。
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，[`early_console.rs`](os/plat/src/early_console.rs)），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types` 与 `fdt`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`。Cargo 的 `[target.'cfg(target_arch = "…")'.dependencies]` 语法允许一条依赖只在指定 target 架构下生效，kernel-image 用它给 riscv64 与 aarch64 各加了两条：`boot-shim`（只开 `opensbi` feature，`uefi` 不开）与 `fdt`（解 DTB 用）——只有以这两个架构为 target 构建时它们才生效。
  - 符合本项目设计目标的方式（独立引导器 + 内核本体两个产物）里，"内核不依赖 boot-shim"对 `minix-kernel` 与 `boot-shim` 成立——引导器依赖内核、内核不回头依赖引导器。这一分工的接线状态见 §3.1.1 的接线状态表（两条 UEFI 产线已跳独立镜像；riscv64 的门链里，U-Boot shim 的六路标门 `test-riscv64-uboot-shim.sh` 注册在 `os/qemu-tests/run_all.sh`；kernel-image 的直核自引导门 `test-kernel-image-riscv64-bootface.sh` 是手工门，不在 `run_all.sh` 中——接线归属以该表为准）。
  - `kernel-image` 的情况不同：它是自引导镜像（引导面 `bootface` 长在自己体内，§3.1 例外条款），riscv64/aarch64 目标下按上面那两条 target 表反向依赖 boot-shim 的**库**（取件表、memmap 源等函数）——依赖的是库而不是引导器产物，单向边界不因此破例（道理见问题二）。这张 target 表就是终态：memmap 源下沉到 `minix-boot` 之后，反向依赖的仍是库、不是引导器产物，依赖方向没有变化。

这条单向边界回答的是"共享类型住哪"，与"哪种固件能编译"是两个独立的问题，别混在一起：

**问题一：共享类型为什么住 `minix-boot`**。真正**跨两侧**引用的是 `KernelInfo`（接收侧构造、内核读）与 `BootReport`（`arch_boot`、`enter_kernel_image_v1`/`prepare_kernel_image_v1` 的发送视图）；`BootStorage`/`BootPrepareResult` 是引导侧的接口类型。放在同一库是为了让这些签名不把 boot-shim 可执行体或固件实现拖进内核依赖面，而不是因为“内核完全不引用引导侧类型”。若 `KernelInfo` 定义住在引导层，内核引用它就得依赖 boot-shim——反向依赖、且把固件实现拖进内核依赖面；若住在内核里，共享库 `minix-boot` 就得反向依赖 `minix-kernel`，与已存在的 `minix-kernel → minix-boot` 成环（boot-shim → minix-kernel 本身不是环——依赖图里本来就有，boot-shim 调 `enter_kernel_image_v1`）。所以这层定义下沉到不含 UEFI/SBI 固件实现、但共享参数归一化与 DTB 解析等实现的 `minix-boot`（依赖表是 `minix-types` 与 `fdt`），谁依赖它都不会被拖进固件代码。这条边界不是风格洁癖：`os/kernel/Cargo.toml` 的依赖表里没有 boot-shim、也没有 uefi，用上面那条 `sed` 就能一次验证。

**问题二（完整形态论证归 §3.1 例外条款与 §3.6；此处只给与依赖方向有关的结论）：riscv64/aarch64 上的引导面为什么长成"内核镜像自举"**。背景见 §3.1 表后的引用段：RISC-V 与 ARM 的 `-kernel` 极简装机（QEMU `-kernel` 模拟的是真实硬件的这种装载方式）下，下一级镜像直接进 RAM、固件跳转，固件与内核之间没有"装载与被装载"的分界，独立 boot-shim 没有存在位置。

于是两个架构的引导准备各自由 `kernel-image` 自带的引导面承担——riscv64 是 `bootface`（[`bootface.rs`](os/kernel-image/src/bootface.rs)），aarch64 是 `bootface_a64`（[`bootface_a64.rs`](os/kernel-image/src/bootface_a64.rs)），流程由 `bootface_common::run_self_boot` 承载（DTB 解内存 → 取件表 → bump 池（bump 池 = 一段只前进不回收的线性分配器：从区间起点逐页发放，游标只增不减，耗尽即拒绝）→ 模块 → `BootReport<'_>` → 同一个 `arch_boot`；接收侧落 `.bss` 后构造 `KernelInfo`）。两者都复用 boot-shim 的**库函数**，但不是独立引导器产物；这一形态的固件对话走 DTB 与 PSCI/DMB（DMB = 数据内存屏障）一类的裸协议，`kernel-image` 与 `minix-kernel` 两个 crate 都不依赖 `uefi`，单向边界原样成立。

它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库；内核引导收敛层按腿分两处（直核 `arch_boot_finish`、跨镜像 `arch_boot_resume_v1`，最终同落 `jump_to_kmain`，见 §3.6/02 篇的 4.5 节）——统一发生在层内（类型、取件库、收敛层），引导形态层不造伪统一（memmap 源统一下沉的裁决，三形态对照见 §3.6）。

**如果让 `minix-kernel` 直接依赖 boot-shim 会怎样**（动机多半是"想复用 `BootPrepareResult` 的定义"）：复用类型走 `minix-boot` 即可，不必依赖引导层（问题一已答）。剩下的否决理由是本节论证的单向依赖原则本身：**内核不该认识固件形态**——屏蔽固件差异正是 boot-shim（或 bootface）存在的意义。"riscv64 编译不过"（UEFI 目标只有 x86_64/aarch64/i686，riscv64 暂不支持）不构成独立论据，换 opensbi feature 就能过；否决理由从头到尾只有依赖方向一条。
### 3.6 五条生产腿与发送侧（接缝定义）

本节用“五条生产腿”一个主计数；其余计法的换算表在节末。

先把四个容易混的入口按调用关系排成三行，后文只在这个骨架上加细节：

```text
直核面：arch_boot → arch_boot_finish → arch_boot_impl → jump_to_kmain
v1 接收：arch_boot_resume_v1 → resume_additive → jump_to_kmain
v1 发送：prepare_and_enter_kernel_image → enter_kernel_image_v1
```

这是骨架，省略了 `resume_after_v1_decode` 等中间层；U-Boot 发送腿从 `prepare_boot` 进入同一组发送接口。

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

（下文反复用到 AArch64 的 TTBR0/TTBR1：它们是两张页表基址寄存器，分别承载低半与高半地址的翻译表；切换地址空间只换 TTBR0，TTBR1 在上下文切换中不被替换。）

Rust 侧的收敛点分两处：直核家族收敛在 `arch_boot_finish`（`arch_boot_impl` 的调用在其体内；三个 cfg 分支的通用实现锚点见 [`fn arch_boot_impl`](<os/kernel/src/lib.rs:fn arch_boot_impl>)），跨镜像三腿的接收侧收敛在 `arch_boot_resume_v1` → `resume_additive`。三条对外入口分别是 `arch_boot`（直核/测试）、`enter_kernel_image_v1`（跨镜像 v1 发送，三条腿：两条 UEFI 链 + riscv64 U-Boot shim；它不在本镜像内收敛，终点是跳独立镜像的入口指令）与 `arch_boot_resume_v1`（接收）：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

x86_64 的 `arch_boot` 只做诊断打印并调用 `arch_boot_finish::<X86_64BootArch, X86_64HigherHalf>`；`arch_boot_impl` 的调用在这个 helper 内，不在 `arch_boot` 的函数体里。

签名里的参数就是交接的全部内容：一份借用的启动视图（`&BootReport<'_>`）、它声明的 RAM 权威，以及一个根页表页的物理地址；接收侧入口立即把这些切片落进 `.bss`。返回值是 `!`——它不返回。

跨镜像交接的产线上多一层（x86_64、aarch64 与 riscv64 U-Boot shim 三条 v1 发送腿同走 `lib.rs` 的 [`fn enter_kernel_image_v1`](<os/kernel/src/lib.rs:fn enter_kernel_image_v1>)；TTBR0/TTBR1 是 aarch64 侧的细节）：内核以独立 ELF 的形式存在于高半（aarch64 侧经 TTBR1），boot-shim 先分配一块八页 arena 并把 v1 协议写进去，然后建表、开分页、把运行期字段补进 arena，最后**绝对跳转**到入口；内存图来源分两路：两条 UEFI 腿在 `ExitBootServices` 的配对循环里分类最终快照，riscv64 U-Boot shim 没有 EBS，直接从 DTB `/memory` 取表；入口寄存器只带 arena 基址；三条腿各自用哪个寄存器、x86_64 的 trampoline 如何转交，见 §4.1 的入口约定。

「虚拟入口」与「物理 arena 基址」这两个值在跳转那一跳上同处。此前它们是一对相邻的裸 `u64`（`-> (u64, u64)` 与 `jump_kernel_image_v1(entry: u64, arena_pa: u64)`），交换后仍能编译：跳转目标会变成 arena 的物理地址，分页刚开后第一次取指就故障，而 `scause`/`CR2` 指向的地址与真实原因无关。现在改由一个值对象按字段名承载。`PreparedJump`、`prepare_kernel_image_v1`、`jump_kernel_image_v1` 都是 crate 内私有实现，唯一公开入口是 `enter_kernel_image_v1`；它不是 wire 类型，只在本仓两处相邻调用之间传递。

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

三处具体事实：

- 交接载荷带 `bump_next`。原因在 §4.6 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次。两条 UEFI 腿的 arena 由 UEFI `AllocatePages` 单独分配、不占 bump 池，因此没有"载荷页"要跳过；U-Boot shim 腿的 arena 由 `bump_alloc_pages` 从 U-Boot 可用内存中模块落点之后的区域另行分配（它与 `BootAlloc` 是两个独立游标，见 §4.2.1 的两个分配器说明）；其起点不低于 `bump_end`，因此在 1024 页 `boot_alloc_region()` 区间**之外**；arena 的 startup-object 身份随 startup 分区退场（§3.2.1），arena 的归属由下面的生命周期合同单独成立；这里的“same pool”指同一片 U-Boot 可用物理内存，不是同一个游标：shim 用 `bump_alloc_pages` 从模块落点之后发 arena，接收镜像的 `BootAlloc` 只续用页表子区间，两者游标独立。扣除表没有单独排除它，而是按生命周期合同处理：解码是 arena 的最后一次读，VM 自由页在那之后才发布，因此这些帧可以重新发给 VM（§4.6 的 poison 测试钉住这条时序）。
- `in("x0") arena_pa`：arena 基址绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧由 `arch_boot_resume_v1` 先把六个分区直接解码进六个 `.bss` 落地区；随后 `resume_additive` 的 `store_kernel_info` 包装跳过字节拷贝（源与目标同址），但仍构造 `KernelInfo` 并写 `KERNEL_INFO` 槽；顶部一次性守卫使第二次调用 panic（§4.5），再只做 additive 阶段（接收壳的分派次序见 02 篇的 4.5 节，这里不重复）：

> 代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。

additive resume 的 Step 4 就是 02 篇 §4.3 的 `establish the direct map`；本篇 §4.8 只保留启动期概念入口，完整机制归独立内存管理主题；这里只保留时序位置。`new_from_page` 的真实签名是 `new_from_page(root_page, channel: PteChannel)`：根的清零与后续 PTE 访问必须走调用方声明的写视图——bootstrap 建根用 `PteChannel::Identity`（Direct Map 尚未建立），运行期进程根用 `KernelDm`，VM 自有根用 `VmDm`。这个参数不是装饰：三个通道只是同一根的不同写入口。

riscv64 上是**直核自引导形态**的出生地：内核镜像自己就是引导体——在没有 U-Boot 与文件系统的 `-kernel` 场景里，入口是 [`main.rs`](os/kernel-image/src/main.rs) 按架构分道的 `rust_image_main`，riscv64 的实现是 [`fn bootface`](<os/kernel-image/src/bootface.rs:fn bootface>)（aarch64 的对应面是 `bootface_a64.rs` 的 [`fn bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)，只在该架构构建时编译），从设备树解析内存、按 `BootFileTable` 取模块——模块来源只有表这一条，表的**地址**只有 DTB `/chosen` 的 `opensbi,boot-file-table` 属性一条通道（仓库内六个脚本——`test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-kernel-image-riscv64-bootface`、`test-riscv64-boot-full`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64`——都用 `fdtput` 注入表址）。

此前的备用 `a2` 参数通道在启动裁决中删除（仓内从未有生产者：QEMU/OpenSBI 只约定 `a0`/`a1`）；表址先过 `in_memmap_range` 再过魔数，失败即 halt。细节在 `bootface.rs` 的入口段，本篇不展开。

U-Boot shim 腿自建表、不经过此通道——它用自建表取件，把模块描述表（`BootModule`：name/start/len）与 memmap 等载荷序列化进 arena；`BootFileTable` 与模块字节留在物理内存；arena 交给接收镜像的 `arch_boot_resume_v1`；调 `arch_boot` 的是两个直核面。

U-Boot shim 路径落地后也补齐了**跨镜像跳转形态**的完整链：`riscv64-uboot-shim`（物理链接，VMA=LMA，自带 bootstrap 栈）吃下 U-Boot `bootelf -p` 的 `(extra_argc, extra_argv)` 参数通道，把内核 ELF 与 12 个引导模块准备成 `BootHandoffV1` arena，DTB 以 `PlatformDescSource` 指针随信封交付，交给配套特性 `fw-riscv64-uboot` 构建的 `kernel-image` 接收镜像走 `arch_boot_resume_v1`——接收侧与 aarch64 同构，复用同一个加性恢复序列，不重建根表。riscv64 的 `jump_to_kmain` 因此带一个执行位置探针：直核面仍在低地址恒等窗口里执行，medany 代码模型的 PC 相对 `la` 解析出的是装载地址，要加上镜像的链接↔装载偏移；接收侧已被 shim 跳到高半链接地址，`la` 解析的已是真值，偏移不得再加（两节分工：02 篇的 4.3 节 讲 C 侧的两份 `pagedir` 换根，本节讲 Rust 侧为何禁止重建活动根；不合并）——同一段 asm 用 `auipc` 读当前 PC 与 `kern_virt_base` 比较后分道。五条内存来源（两条 UEFI 链的分类结果、U-Boot shim、两个直核面）交付的形状统一为两张表，收敛点与各腿的产出方式见 §3.2；引导形态层本身不合并。

**两个另一做法各会撞什么**（两处）：

其一，aarch64 退回内联执行。内联时内核代码住在 PE 镜像的运行物理地址上（低半、`VA = PA`、在 TTBR0 覆盖范围内）。第一次 `switch_address_space` 会换 TTBR0 到进程页表并发 `tlbi vmalle1is`，内核自身的取指映射随之消失——现场证据（`elr`/`vbar` 钉在同一个低地址上）外包给 [`rewrite-notes/01-stage-kernel/01-code-design.md`](01-code-design.md)；本篇只登记这条指向，不自称独立可核。跨镜像形态把这个问题的根去掉：内核代码链接在高半（TTBR1），而 TTBR1 在上下文切换中不被替换。

其二，接收侧按"干净启动"重建 bootstrap 根（调 `Paging::new_from_page(root_page, channel)`）。该函数的第一件事是把 512 个根条目清零（`write_bytes(ptr, 0, 512)`），而此时这个根**正在被使用**——承载当前函数的那条指令所在的 L0 槽就在其中。清零本身不会立刻作废已缓存的 TLB 项——三个架构都不因改写页表而自动清 TLB——故障落在下一个**冷**取指上（TLB 回填或切换后的 kmain 入口取指）：之前靠 TLB 残留还能跑几条指令，这段"还能跑"的窗口让故障点与根因隔得更远，栈上多半连一条 panic 消息都来不及打印；"resume 面只做 additive 阶段"这条约束就是从这里来的。

**直核自引导在真板上的位置** `[ARCH: FW_PAYLOAD 直核形态（Minix3 无对应）]`。`FW_PAYLOAD` 直核形态的规范背景与真板意义见 §3.1 例外条款段（一句话版：固件打包下一级镜像直接跳 S 模式目标，QEMU `-kernel` 模拟同一时序）。三架构的落位并不对称：

| 架构 | 直核形态的位置 | 原因 |
|---|---|---|
| x86_64 | 无对应形态 | 平台规范强制 BIOS/UEFI 作为唯一引导面，没有"固件直接跳任意 payload"的规范形态 |
| aarch64 | 覆盖测试为主 | QEMU `-kernel` 是主力验证通道；真板启动一般仍经 UEFI/U-Boot |
| riscv64 | 测试与真板嵌入式并存 | `FW_PAYLOAD` 是规范形态，真板极简装机直接使用；`-kernel` 是它在 QEMU 的对应物 |

所以直核自引导是**辅助启动路径**，不是三架构的主线：主线是"独立 boot-shim + 独立内核镜像 + 交接载荷"（§3.1 的分工表；各路径的当前接线状态见 §3.1.1 的接线状态表）。这张矩阵同时解释了 `bootface`/`bootface_a64` 为什么保留而不作主产线——它们服务的是特定嵌入式形态与测试通道。

**启动路径计数的换算基准（本节末表）**。

> **术语对账**：本篇用多个不同维度描述同一组启动路径。**本表是全文启动路径计数的换算基准**；各节可以在服务上下文时复述成员名单，但跨节比较与换算是以本表为准，不另立计数。正文只以“五条生产腿”为主计数；其他计法只在各自引用的节里使用。
>
> **“生产腿”的判据**：当前源码里接线的完整启动路径，不等同于“主线部署形态”。五条生产腿 = 主线 3（两条 UEFI 链 + riscv64 U-Boot shim）+ 辅助/测试 2（riscv64 `bootface`、aarch64 `bootface_a64`）；直载测试链不是腿，只在“三类链”里出现。§3.1.1 的“测试通道”指用途，不否定它是一条接线腿。
>
| 计法 | 数量 | 成员 | 用在哪 |
|---|---|---|---|
| 形态 | 3 | 同镜像内调用 / 跨镜像跳转 / 直核自引导；同镜像是测试/历史形态，当前生产腿只有跨镜像与直核两类。直核自引导也是同镜像，区别是内核镜像自带引导面、自己建根；旧“同镜像内调用”是内核被链进 shim 镜像内联执行 | §3.6 |
| 生产腿（主计数） | 5 | 两条 UEFI 链 + U-Boot shim + riscv64 `bootface` + aarch64 `bootface_a64` | 正文、§3.1.1 |
| 协议段/交接面 | 4 | v1 发送面 / v1 接收面 / riscv64 自引导 / aarch64 direct-core；send/receive 是同三条 v1 腿的两端，不是四条腿 | 02 篇的 4.5 节 |
| 三类链 | 6 | UEFI 一族 2 条 + U-Boot 一族 2 条（shim 生产腿 + 直载测试链）+ 直核面 2 条；其中 5 条是生产腿，直载测试链不是 | §3.6 |
| 架构×链五行 | 5 | 五条生产腿各一行 | 02 篇的 4.5 节 |
| 真机门链五条 | 5 | 按脚本集合，成员以 §5.4 的表格为唯一权威（本节不重复名单）；不含 aarch64 直核面 | §5.4 |
| 有启动门链的四条腿 | 4 | x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface`；是“五条生产腿”去掉 aarch64 直核面，也是“五条真机门链”去掉直载测试链 | §5.1 |
>
> “产线”是链的别称，不单独计数。两个“5”成员不同：架构×链五行含 aarch64 直核面、不含直载测试链；真机门链五条含直载测试链、不含 aarch64 直核面。§4.1 的阶段总览表只按 UEFI / 非 UEFI 两列讲阶段，不是独立计法。
### 3.7 所有权与校验门链（I3/I4）

I3 与 I4 在实现上是两条相邻的检查：**载荷先落到自己的存储，再发布；接收侧校验在消费（建表/映射/分配）之前完成**。发送侧的建根只依赖装载/分类门已经保证的报告，不在接收侧门链内（§5.1 的 I4 行）。注意“消费前”不等于“发布前”：直核面由 `arch_boot_finish` 先 `store_boot_report`、再进 `arch_boot_impl` 的 `boot_validate_and_prepare`；v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`。本节的“发布”指把落地后的 `KernelInfo` 写进 `KERNEL_INFO` 槽（此后 `kernel_info()` 可读）；“消费”指建表/映射/分配。VM 自由页的“发布”是另一件事（§4.6）。这两组腿（直核面与 v1 接收面）的包装调用次序相反。v1 接收侧有两个容易同名的事件：六个分区在 `arch_boot_resume_v1` 里先解码进 `.bss` 落地区（发生在 `resume_additive` 的 additive 阶段之前）；`store_kernel_info` 是之后对同一组地址的包装调用（源与目标同址，跳过复制）。因此“载荷先落盘”对两族都成立；“store 与 boot_validate 次序相反”只指包装调用。

所有权：`BootStorage` 拥有 U-Boot shim 腿上的六类载荷；UEFI 腿只拥有五类（四个 `fill_*` 调用：`fill_map_tables` 一次提交两表；`fill_source_records` 在 U-Boot shim 与两个直核腿使用，UEFI 腿不走它）；两个直核腿各拥有一份栈上的 `BootStorage`，六类载荷齐全（包括经 `fill_source_records` 写入的 source 记录）；UEFI 腿的 source 分区由两个阶段直写 wire：分类器先写 UEFI 记录，EBS 配对循环在 `authorize_and_copy` 返回 `OwnedDtb` 后调用 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录，再经 `push_dtb_sources` 追加并二次 `finish_sources()`；storage 的 source 切片为空，不在这句的覆盖范围内；strict 审计按 authority 过滤，因此这批 DTB 记录不计入 UEFI 权威的覆盖；`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自在闭包返回时提交计数，族内半提交状态不可表示；跨族半提交（例如只填了 modules 未填 map）由接收侧的 `memmap`/模块/来源覆盖检查兜住。

`BootReport<'a>` 只是借用视图；跨镜像发送侧把它序列化进 wire，接收侧在自己的 `.bss` 重建切片。接收侧先把切片深拷进自己的 `.bss`，再构造唯一的 `KernelInfo`；`'static` 只由这些 `.bss` 落地区支撑，不从堆上“泄漏”出来。UEFI 腿的 source 记录由分类器与 DTB claims 两阶段直写 wire，不经过 `fill_source_records`；因此“第二次 fill 会 panic”只对实际走 `fill_*` 的腿成立。

校验的逐腿顺序（§4.4.1 的权威表）：直核面 `validate_requiring_sources` → `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 strict `validate` → `check_boot_postconditions` 复核；v1 接收面 `validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 strict `validate` → `check_boot_postconditions` 复核，不经过 `validate_before_tables`。`check_boot_postconditions` 在 v10 之后只剩 `kern_virt_base > u32::MAX` 一条断言；镜像与模块、模块与模块的半开区间重叠在共享校验体 `validate_view` 里，三档都会跑。直核面的 `arch_boot_finish` 在 `store_boot_report` 之前另有一条可信链断言：记录非空时，`authority` 必须与至少一条记录的 `input_object` 相同；这把“声明”与“记录”在发布前绑在一起（§4.4.1 时间序表第 4 步）。

五类错误各有落点：缺来源记录 → `FreeSpanWithoutMemorySource`；权威与记录不匹配 → “declared RAM authority … has no source records”；重复提交 → 走 `fill_*` 的腿第二次调用 panic；半提交（写而未提交）在现行 API 中不可表示；跨镜像指针 → 接收侧读到零或 fault。

---

#### 失败对照

如果把 boot-shim 与内核并成一个 UEFI 二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与“内核不依赖引导层”的单向边界冲突。所以 UEFI 形态被否决，直核形态只允许 `kernel-image` 反向依赖 boot-shim 的库函数，而不是依赖引导器产物。

#### 最小示例

```bash
# 在 os/ 下跑交接库的宿主测试；wire 与字段台账的形状由这一包守住（§5.1）
$ cd os && cargo test -p minix-boot --lib
```

预期全部通过；§5.1 把 wire 的 encode/decode round-trip 与 `KernelInfo` 字段台账列为 I1 的宿主证据。

#### 小结

- 协议换代把“谁读文件、谁填内存图、谁退出固件服务”重新分工；UEFI 腿自己读，U-Boot 腿由 U-Boot 代劳。
- `KernelInfo` 从全局地址改为按值交接；三条 v1 腿走 wire，两个直核腿同镜像也深拷。
- 内存账从 `kinfo` 数组改为两张切片表加 `SourceRecord`，每个投影字节可追溯。
- bootstrap 区间与页表分配器在进入内核前换手；没有“分页后可回收的内核段”这个对象。
- 五条生产腿收敛数据形状，不合并引导形态层。

#### 术语

- **wire**：跨镜像交接协议里按固定字节宽度写进 arena 的表示；v1 信封带版本号，收发两侧精确比较。
- **arena**：三条跨镜像 v1 腿交接用的八页物理区，承载信封、分区载荷与运行期字段。
- **v1 腿 / 直核面**：前者跨镜像（两条 UEFI 链、riscv64 U-Boot shim），后者在同一镜像内直接调 `arch_boot`。
- **`BootStorage`**：发送侧自有的定长载荷存储；它拥有 U-Boot shim 腿上的六类载荷；UEFI 腿的 storage 只拥有五类，source 分区由分类器与随后的 DTB claims 两阶段直写 wire（见 §3.7）。
- **`SourceRecord`**：记录“这段内存是谁给的”的来源条目，供 `audit_source_coverage` 审计。
## 4. 实现详解

### 4.0 本章导读

#### 问题入口

设计定了，代码仍要在两个镜像、五个入口和一次 `ExitBootServices` 之间落地。本章以两条 UEFI 链的实际入口为主，兼及共用取件与 C 对照：`main` 怎样把页表页预算交给 `prepare_and_enter_kernel_image`，九个动作在哪一步读文件、哪一步退出固件服务、哪一步把分类计数提交，以及 `KernelInfo` 的两条产出入径和六个落地区怎样保证接收侧只碰自己的副本。

#### 前置概念

- 第 1 章的三份数据与 I1–I4。
- 第 2 章的 C 侧调用树：`pre_init` 的 `get_parameters` 与 `pg_*` 五步的接缝。
- 第 3 章的 wire v1、arena、五条生产腿与 `BootStorage` 的所有权规则。

### 4.1 入口与调用链

本章有四套容易相撞的编号，先把它们对齐；后文出现数字时按这张表判断它属于哪一套：

| 编号 | 范围 | 定义处 | 是否时间序 |
|---|---|---|---|
| Step 0–4 | 内核建根：校验/注册、恒等映射、高半映射（2b/2c）、开分页、DM 覆盖 | 本节与 §3.6 | 是 |
| 动作 1–9 | UEFI 链的九件事：分配根/bump、读 ELF/预订/拷贝/模块、平台源/参数、arena、写信封、EBS 分类、提交计数、报告复核、建根跳转 | §4.2.1 | 否，稳定标签 |
| 时间拍 1–10 | 九动作按真实执行序展开（动作 2 拆 2a/2b） | §4.2.1 | 是 |
| 防线 A–D | 模块装载的四道检查：A/B 守内核段、C 守模块、D 在接收侧 | §4.3 | 否，按对象分类 |

UEFI 入口是固件加载 `.efi` 后调用的那个函数。它只做一件事：调用 `prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`，把后续工作交给这个函数。后者内部再分三段：准备（读内核 ELF 并预订其 LMA 跨度 → 分配根页表页与 bump 区 → 拷贝段与装载模块 → 平台源与参数 → 分配 arena 并序列化非内存图载荷）、退出固件服务（配对循环里分类最终内存图并调 `ExitBootServices`）、交接（提交分类计数、构造报告并复核、激活自举根、补运行期字段、跳独立镜像）。入口本身不返回。

下面这张图标的是 x86_64 UEFI 一条腿的完整路标；每一步的“谁写什么、过哪道门、失败落在哪”都在后面小节展开。

```text
UEFI 固件
  │
  ├─ boot-shim .efi main
  │    └─ prepare_and_enter_kernel_image(BOOT_PT_BUMP_PAGES)
  │         ├─ 准备：读 kernel.elf + 12 模块；分配 root/bump/arena；写平台源/参数
  │         ├─ 退出固件服务：配对 GetMemoryMap/ExitBootServices；分类写 arena + BootStorage
  │         └─ 交接：构造 BootReport；boot_validate_and_prepare；建根；开分页
  │              └─ 跳 kernel.elf 高半入口
  │
  ├─ arch_boot_resume_v1
  │    ├─ 解码六个分区进 .bss
  │    ├─ strict validate_requiring_sources
  │    ├─ check_boot_postconditions
  │    └─ resume_additive
  │         ├─ boot_validate_and_prepare
  │         ├─ store_kernel_info（源与目标同址，跳过复制）
  │         └─ establish_boot_dm
  │
  └─ kmain
       ├─ strict validate_requiring_sources
       ├─ check_boot_postconditions
       └─ scheduler
```

失败落点：读文件失败在 shim 的 `read_required` panic；EBS 配对失败在配对循环重试/停机；结构校验失败在发送侧 `boot_validate_and_prepare` 或接收侧 `resume_additive` 的断言；建根/开分页失败在 `build_bootstrap_root_and_enable` 的 halt；`kmain` 的 strict 门失败在进入 scheduler 之前 panic。`arch_boot_finish`/`arch_boot_impl` 只属于直核面；直核腿把“跳 kernel.elf”换成“同镜像直接调 `arch_boot`”，其余门序见 §4.4.1。

本章的 Step 编号在这里立住（02 篇的 4.3 节 与 §6.1 共用）：Step 0 校验与分配器注册检查（`boot_alloc_region()` 为 `None` 即停机；没有从 memmap 切一段作兜底——直核预算的 256 页只是量级，不是这个兜底的实际常量，理由见本节末段）；Step 1 恒等映射；Step 2 内核高半映射（2b 高半设备别名、2c 设备窗口页）；Step 3 开分页（`enable`）；Step 4 DM 覆盖（概念入口见 §4.8；完整布局/策略归独立内存管理主题）。

先给两条固件路径一张总览图——本章后面各节再逐段展开，这张表只负责导航：

| 阶段 | UEFI 路径 | 非 UEFI 路径（自引导/直核面 + U-Boot shim；按“谁建根”归类，跨镜像细节见 02 篇的 4.5 节） |
|---|---|---|
| 谁在跑引导面 | 固件直接调 boot-shim 的 `.efi` 入口（[`fn main`](<os/boot-shim/src/main.rs:fn main>)） | 直核面：内核镜像自带的引导面（[`main.rs`](os/kernel-image/src/main.rs) 的 `rust_image_main` → `bootface`/`bootface_a64`）；U-Boot shim：独立的 `riscv64-uboot-shim` 可执行体（`uboot_main`），不是内核镜像自带 |
| 读内核与模块 | boot-shim 自己读：`SimpleFileSystem`（固件提供的文件系统协议）加 12 项 `MODULE_NAMES`（§4.3） | `-kernel` 直核：从 DTB 解内存、从表/DTB 通道取模块（02 篇的 4.5 节）；U-Boot shim 腿由 shim 自己的 `BootFileTable` 取件（02 篇的 4.5 节） |
| 内存与参数准备 | 两条 UEFI 产线共用 `prepare_and_enter_kernel_image`：分配八页 arena，先写信封与非内存图载荷，再在配对循环里把分类结果同时写进 wire 与 shim 自己的 `BootStorage`；source 分区例外，只写 wire（§3.2、§3.7、§4.2、§4.4） | 直核面共用 DTB 解析实现（`DtbMemoryMap`）；U-Boot shim 走 v1 序列化（02 篇的 4.5 节）；U-Boot/直核面从 DTB `/memory` 取表，不经过 UEFI 配对循环（`param_buf` 由各路径参数源经同一 normalizer 产出；§4.4） |
| 退出固件服务 | 两条 UEFI 产线都在配对循环里用 raw `ExitBootServices`，分类与退出用同一个 MapKey | 无此步骤（引导器早已把控制权交出去） |
| 进入内核 | 两条 UEFI 产线：建根、补运行期字段、跳独立镜像；三条 v1 腿的 arena 入口约定是 x86_64 用 `rbx`（镜像 trampoline 再转入 SysV 第一参数 `rdi`）、aarch64 用 `x0`、riscv64 U-Boot shim 用 `a0`；接收端 `arch_boot_resume_v1`（02 篇的 4.3 节、§4.6、02 篇的 4.5 节） | 直核面：镜像自身建表并进 `arch_boot`；U-Boot shim：独立 shim 建根，接收镜像进 `arch_boot_resume_v1`（02 篇的 4.5 节） |

（注意：上表两列不是同一个入口的两个实现；U-Boot 腿走 `prepare_boot`，UEFI 走独立入口，见 §3.5。）三类链在这一章里各走各的阶段，收敛点按腿分两个：直核家族收敛在 `arch_boot_finish`，跨镜像三腿经 `arch_boot_resume_v1` 汇入 `resume_additive`（各架构委托壳见 02 篇的 4.5 节）——完整交代见 02 篇的 4.5 节。

```rust
// 节选（保留入口骨架；注释有删节与改述），os/boot-shim/src/main.rs:fn main。
#[entry]
fn main() -> Status {
    // 1024 pages (4 MiB): … the handoff arena's fresh chain all draw
    // lower-level page-table pages from this bump pool via boot_pt_alloc.
    boot_shim::uefi_helpers::prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)
}
```

逐行解码：

- `main.rs` 的 [`fn main`](<os/boot-shim/src/main.rs:fn main>) 传给 `prepare_and_enter_kernel_image` 的 `boot_shim::BOOT_PT_BUMP_PAGES` 是**页表页预算**（常量值 1024，定义在 `lib.rs`；1024 页 = 4MiB；两个直核面另用 `bootface_common` 的 `PT_BUMP_PAGES = 256`）。第一段注释把这份预算的构成列了出来：恒等映射、内核高地址映射、Direct Map 覆盖建立、引导身份窗口的逐页拆解（只拆 `load_elf_into` 的 ELF 段页与 64 MiB VM heap 窗口，不是把全部 boot 恒等窗口撤掉）、以及交接 arena 之后的页表链条——它们都从同一个 bump 池里取页。预算是"上限"而不是"必须用完"：`BootAlloc` 只在分配时推进游标。**1024 与 256 的差别来自两条路径各自要建的东西**：跨镜像腿要额外建 Direct Map 覆盖、ELF 段的逐页拆解与 arena 之后的页表链，三架构各自的中层表加起来需要更大余量；两个直核面走的是同一片注册给 `pt_alloc` 的 bump 池，覆盖量级是各架构页表根 + 低半恒等映射 + 高半映射 + 两个 DM 窗口（riscv64 为 Sv39 root，aarch64 为 TTBR0/TTBR1 两套根；窗口构造点见 §3.6 的 `enter_kernel_image_v1` 组装与 §4.4 的 `BootWindows::check`/窗口边界规则，DM 建表同样从 `pt_alloc` 取页）——**DM 窗口也算在这 256 页里**，只是窗口数与逐页拆解面远小于跨镜像腿，所以 1 MiB 够用。两者没有共享常量，因为它们描述的不是同一个上界；实际消耗由 `boot_alloc_used_bytes()` 在交接时记为 `kernel_allocated_dynamic`（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff` 的 `kernel_allocated_dynamic` 赋值点），不靠预算反推。**未用部分不退回**——VM 交接记录按 `boot_alloc_region()` 把整段扣除（见 §3.4 的扣除表），用掉的与剩下的都算内核保留。（UEFI 路径下 bump 区本就不在 conventional 列表里，这条扣除对 free list 是空操作；扣整段防的是自引导等路径把 bump 区记进清单的情形。映射责任移交 VM 之后，`vmctl_set_addr_space` 不再向这个池取 kerninfo 注入页，预算只剩启动期本身的消耗。）
- 两条 UEFI 产线的入口是同一个 `prepare_and_enter_kernel_image`：arena 分配、载荷序列化、配对循环都在 EBS 之前，EBS 之后提交分类计数并复核、构造 `BootReport`、建根、补字段与跳转（五条腿发送侧都不构造 `KernelInfo`；`KernelInfo` 由接收侧落 `.bss` 后形成唯一副本）。分界线之后的代码不再出现任何 `uefi::` 调用（`prepare_and_enter_kernel_image` 与 `ebs_pair` 两个模块都如此，后者的分类计数打印也在 EBS 成功分支改走裸串口）；路标走裸串口。
- 进入内核的分道只剩接收端的一处：x86_64 的 `rust_image_main` 拿到的是 `_start` trampoline 从 `rbx` 转进 `rdi` 的 arena 基址（x86_64 入口约定，见本节总览表），aarch64 从 `x0` 取；两者都调用 `arch_boot_resume_v1`，再从既有的自举根上做增量阶段（`rust_image_main` 的分道臂代码——含 riscv64 的两种交付形态——逐臂引在 02 篇的 4.5 节，本节只讲 boot-shim 侧的入口链）。内核不再有"链进 PE 低地址内联执行"的生产路径，VM 也就不再需要在进程页表里复现低地址窗口（`reserved_regions` 的恒等重放半随恒等窗口退场，DM 候选半保留，见 §3.2）。

**若把页表分配器的注册挪到进入内核之后会怎样**。先锚定视角：这里的"分配器"是**接收镜像自己链接的那份 `BootAlloc` 副本**（§3.2：两个镜像各链接一份）——shim 进程内 `init_boot_pt_alloc` 初始化过的是另一份，与本推演无关；接收副本在 resume 之前 `boot_alloc_region()` 为 `None`。在此视角下：`prepare_and_enter_kernel_image` 动作 1（root/bump 分配之后）那次 `init_boot_pt_alloc` 照常执行，但 `register` 实际发生在 `boot_validate_and_prepare`（由 `enter_kernel_image_v1` 调用，带 `is_registered()` 守卫），把它挪到 `enter_kernel_image_v1` 的跳转之后（接收侧即 `arch_boot_resume_v1` 的增量阶段之后；`arch_boot` 是直核/测试入口，不是 UEFI 链的接收点）。

`boot_validate_and_prepare` 在 `boot_alloc_region()` 为 `None` 时**直接停机**（该兜底已不存在）：从 `memmap` 里挑第一个够大的区间来兜底，等于把内核镜像、已装载模块或 arena 所在的位置当成空闲起点，把活着的物理页再发一次。

本段挪动的是 `register`，不是 bump 区间初始化：`require_boot_region` 仍会通过，故障落在 Step 1/2 第一次 `map_huge` → `alloc_pt_page`，断言消息是 `pt_alloc::alloc_pt_page called before register`。两个对象要分开：`require_boot_region` 看的是 `boot_alloc_region()`（bump 区间），`alloc_pt_page` 看的是 `PT_ALLOC != 0`（页表分配器是否已注册）。注册是进入内核的**前置条件**，不是可以从内存图推断的默认值。
### 4.2 UEFI 路径：九个动作与产出归属

两条 UEFI 产线（x86_64 与 aarch64）共用同一个入口：`os/boot-shim/src/uefi_helpers.rs` 的 [`fn prepare_and_enter_kernel_image`](<os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image>)。它返回 `!`：建根、开分页、跳独立镜像，不返回 boot-shim。U-Boot 腿的入口是 `opensbi_helpers::prepare_boot`；UEFI 侧走自己的入口，两者不共享 trait。

读下面的代码块时按“局部机制”读：块首已标节选/重排，未展开的分支不再要求读者去源码补全整条路径；端到端路标以 §4.1 的图为准。

```rust
// 节选（保留顺序与关键约束；注释与排版有改写/删节，代码表达式也按语义重排、非逐字；source-symbol: prepare_and_enter_kernel_image；未展开段：参数统计的 `println`、`system_table`/`boot_services`/`ops` 构造、arena 的 `assert_range_clear_of_kernel`、`NeedBiggerBuffer` 扩容断言分支、`dumpdtb_blob` 的 `#[cfg]` 构造、入口与首段 VMA 的 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, …)`），os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image。
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    uefi::println!("boot-shim: prepare_and_enter enter (handoff v1 arena)");
    // 捕获 shim 自身镜像与栈窗口；EBS 前若存在 dumpdtb 再把缓冲并入。
    let shim_image = shim_image_window();
    let shim_stack = shim_stack_window();

    // A+D：先读 ELF、预订其 LMA 跨度，再分配 root/bump，最后拷贝段。
    let file_loader = UefiFileLoader;
    let elf_data = file_loader.read_required(loader::KERNEL_PATH); // FileBytes：UEFI 腿是 Owned
    let layout = loader::compute_kernel_layout(elf_data.as_slice())
        .expect(...)
        .validate()
        .expect(...);
    reserve_kernel_span(&layout);      // AllocateAddress([kern_phys_base, +kern_size), LOADER_DATA)
    record_kernel_span(&layout);
    let root_page = alloc_root_page();
    let (bump_base, bump_end) = alloc_bump_region(bump_pages);
    minix_kernel::boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    unsafe { loader::load_segments_into_phys_memory(&layout) }.expect(...);
    let kern = layout.layout();
    uefi::println!("boot-shim: kernel loaded (entry staged)");

    // 语义表住在发送入口自己的栈帧（`BootStorage::new()` 非全零；入口不返回；见 §3.2）；loader 直接写进它的模块槽。
    let mut storage = minix_boot::BootStorage::new();
    // …（root/bump 的 assert_range_clear_of_kernel 略：位置在本行之前，见动作 2）…
    storage.fill_modules(|slots| {
        loader::load_boot_modules_into(&file_loader, alloc_module_pages, slots)
    });
    uefi::println!("boot-shim: 12 boot modules loaded");
    // …（模块的 assert_modules_clear_of_kernel 略：位置在本行之后，
    //    说明见动作 2 与 §5.1 的 I3 宿主证据行）…
    storage.fill_platform(|slots| find_platform_sources_into(slots));
    storage.fill_param(|buf| collect_param_buf_into(buf).bytes);

    let arena_pa = alloc_arena();
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa.0 as *mut u8, ARENA_CAP) };
    let mut serializer =
        Serializer::new(arena, minix_boot::InputObjectId::UefiMemoryMap)
            .expect("handoff arena too small");
    // 标量信封先定稿；报告借用 storage，载荷也从同一份 storage 编码。
    let report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    serializer.write_envelope_report(&report).expect(...);
    serializer.write_boot_modules(report.boot_modules).expect(...);
    serializer.write_platform_sources(report.platform_sources).expect(...);
    serializer.write_param_buf(report.param_buf).expect(...);

    let mut buffer_bytes = probe_map_buffer_size(boot_services_raw);
    // 配对循环整体在 fill_map_tables 闭包内：闭包返回 MapCounts 时
    // storage 两表与计数在同一步提交。
    let mut classification = None;
    storage.fill_map_tables(|memmap_slots, reserved_slots| loop {
        let buffer_ptr = alloc_map_buffer(buffer_bytes);
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_bytes) };
        // …（DTB preflight 省略，见下方说明）…
        match unsafe {
            ebs_pair::pair_and_classify(&ops, boot_services_raw,
                image_handle.as_ptr() as usize, buffer, pa_limit(), &mut serializer,
                memmap_slots, reserved_slots)
        } {
            Ok(result) => {
                classification = Some(result);
                break minix_boot::MapCounts {
                    memmap: result.conventional,
                    reserved: result.reserved,
                };
            }
            Err(PairError::NeedBiggerBuffer(needed)) => { /* 扩容后重试 */ }
            Err(error) => {
                panic!("boot-shim: EBS pairing loop failed: {error:?}");
            }
        }
    });
    let classification = classification.expect("the pairing loop returns only on success");
    crate::raw_serial("boot-shim: [raw] boot services exited (paired)\r\n");

    // 此处没有 startup 写入段；完整账见 §3.2.1。

    let mut report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    // UEFI 腿的 source 记录由分类器直写 wire；storage 的 source 切片为空。
    cross_validate_config_dtb(report.platform_sources, report.memmap);
    assert_bootstrap_outside_memmap(report.memmap, root_page.0, PAGE_SIZE);
    assert_bootstrap_outside_memmap(report.memmap, bump_base, bump_end - bump_base);
    assert_bootstrap_outside_memmap(report.memmap, arena_pa.0, ARENA_CAP as u64);
    report_memmap_source(dumpdtb_blob.as_deref(), report.memmap);

    crate::raw_serial("boot-shim: [raw] arena=");
    raw_serial_hex(arena_pa.0);
    // …（conv/res 两行略）…
    crate::raw_serial("\r\nboot-shim: [raw] entering kernel image v1\r\n");
    minix_kernel::enter_kernel_image_v1(
        arena_pa,
        &report,
        root_page,
        &shim_windows[..shim_window_n],
    )
}

```

引文的省略段说明：EBS 之后没有 startup 写入段，该处只留一条说明注释（删除账见 §3.2.1）；配对循环体内另有一处省略（DTB 头 preflight），它发生在首次 `GetMemoryMap` 之后、`ExitBootServices` 之前，不是配对成功后的额外一次调用；`authorize_and_copy` 也在最后一次 `GetMemoryMap` 之后、`ExitBootServices` 之前执行，正文只用这一句交代它的时序，不展开实现（§1.4 的下游交代清单）。引文里的 `dumpdtb_blob` 是 `Option<Vec<u8>>`（DTB 暂存区的自有缓冲，不是静态切片），`ops` 是 EBS 配对用的函数表包装，两者都在省略段之前的准备步骤中产生。末行的 `shim_windows[..shim_window_n]` 也不是可省略的装饰：函数开头从 `LoadedImage.info()` 与当前 SP 捕获 shim image/stack，EBS 前若存在 `dumpdtb_blob` 再把它的缓冲并入，最多三个窗口；它们与内核 LMA、root/bump/arena、模块、平台源、设备窗口一起交给内核建根，不对应任何一次固件调用。

#### 4.2.1 九个动作与每步的约束

先按执行序读；动作 ID 只是稳定标签，不承担顺序语义。下表按时间拍列出真实执行顺序，后面的表再按动作 ID 给约束。动作 2 拆成 2a（读/预订）与 2b（拷贝/模块）两拍，所以 10 拍对应 9 个动作；动作 2b 与 §4.1 的 Step 2b 是两套编号，不互相指代。

| 时间拍 | 动作 ID | 做什么 |
|---|---|---|
| 1 | 2a | 读 `kernel.elf`，预订 LMA 跨度 |
| 2 | 1 | 分配 root 页表页与 bump 区，`init_boot_pt_alloc` |
| 3 | 2b | 拷贝 ELF 段，加载 12 个模块 |
| 4 | 3 | 定位平台描述符来源，归一化 `param_buf` |
| 5 | 4 | 分配八页 arena |
| 6 | 5 | 从 `BootStorage` 写信封、模块、平台源、参数（非内存图载荷） |
| 7 | 6 | `GetMemoryMap`、分类、写 source 记录，再用同一 MapKey 调 `ExitBootServices` |
| 8 | 7 | 提交 `MapCounts` 与两张内存表 |
| 9 | 8 | 构造 `BootReport`，复核 root/bump/arena 不在 conventional 列表 |
| 10 | 9 | 建根、开分页、补运行期字段、跳转 |

四套编号要分清：本节的“动作 ID”、本节的“时间拍”、§4.2.2 的描述符判定五步、§4.1 的 Step 0–4；四者不互相指代。

**按动作 ID 的约束**：

| 动作 ID（非时序） | 时间序 | 动作 | 约束 |
|---|---|---|---|
| 1 | 2 | 分配根页表页与 bump 区，`init_boot_pt_alloc` | 必须在 DM 可达上界之下；同一物理区间由 shim 建根与接收侧各自续用，两个 `BootAlloc` 实例不共享游标（交接以 `bump_base`/`bump_next`/`bump_end` 三个字段记录游标位置，见 §4.6）。按 A+D 的顺序，本步排在动作 2 的 ELF 预订**之后**（先订跨度、再订自举页，避免自举页落进镜像的拷贝目标） |
| 2 | 1（读/预订）/ 3（拷贝/模块） | 读 `kernel.elf`、预订其 LMA 跨度；拷贝段与加载 12 个模块在动作 1 之后（时间列的 3 是动作 2 的拷贝/模块段；动作 3 在时间 4） | 必须在 EBS 之前（依赖 `SimpleFileSystem`）；A+D 的顺序与 ELF `Vec` 例外见本节表后的 bullet |
| 3 | 4 | 定位平台描述符来源，归一化 `param_buf` | 必须在 EBS 之前（依赖 Configuration Table——固件在 EBS 前交给应用的描述符表，DTB/RSDP 指针挂在其中——与 `LoadOptions`——固件为本镜像保留的命令行字节串） |
| 4 | 5 | 分配八页 arena（`alloc_arena()` 经 UEFI `boot::allocate_pages(MaxAddress(bootstrap_alloc_limit()))`——该上界为无参函数返回的 DM 可达界；`bump_alloc_pages` 是 OpenSBI 侧的另一个分配器，与本动作的 UEFI 分配器不同），`LOADER_DATA` | 整段落在恒等/DM 可达界之下；接收方经恒等窗口读它 |
| 5 | 6 | 从 `BootStorage` 的报告写信封、模块、平台源、参数（非内存图载荷） | 动作 5 开头 `Serializer::new` 对前 `ARENA_CAP` 字节整段零化（源文件 `handoff_v1_write.rs`）；信封的静态标量字段此时定稿（root/bump 等运行期字段在建根后回填）；载荷切片来自 storage，不是 `'static` 指向 arena |
| 6 | 7 | 分配 `GetMemoryMap` 缓冲，进入配对循环 | 循环内先 `GetMemoryMap`，再分类进 memmap/reserved 两个内存图分区并写 source 记录分区，最后用同一 MapKey 调 raw `ExitBootServices` |
| 7 | 8 | 由 `fill_map_tables` 闭包返回 `MapCounts` 提交分类器返回的 `conventional`/`reserved` 计数 | 描述符解码与分类在配对循环内、`ExitBootServices` **之前**完成；EBS 成功后闭包返回计数，`fill_map_tables` 在同一步提交两表（写与提交不可分）。只有分类成功的那一份内存图存活；重试前 `reset_map_partitions` 清掉旧 wire 条目；分类同时写进 storage 与 wire，不存在第二份 arena 语义表 |
| 8 | 9 | 构造 `BootReport<'a>`，再用它复核 root/bump/arena 都不在 conventional 列表里 | 断言消费动作 7 提交的 storage 视图；失败即裸串口打印后停机（此时已无 Boot Services 可退）；报告的生命周期绑定 storage，内核入口只借用它，不提升为 `'static` |
| 9 | 10 | 建根、开分页、把运行期字段补进 arena、跳转 | `build_bootstrap_root_and_enable` 返回之后到跳转之前不再从 UEFI 分配；建根动作内部的 bump 取页已结束，`patch_runtime_fields_in` 不再分配、也不零化 arena。窗口表此时已包含 shim image/stack（UEFI）或 `[SHIM_LINK_BASE, UBOOT_SHIM_STACK_TOP)`（U-Boot），以及可选 `dumpdtb` 临时缓冲窗口（EBS 前存在时才建）、内核 LMA、root/bump/arena、模块、平台源、设备窗口；x86 另含低 2 MiB AP 跳板窗口 |

动作 5 只写非内存图载荷。配对循环成功、`ExitBootServices` 之后没有 startup 写入段（删除账见 §3.2.1），因此执行序在动作 6（拍 7，EBS）之后直接进入动作 7（拍 8）；本节不给这个空缺动作 ID 或拍号。

按时间重建 EBS 前后序列时，正确读法是：拍 7 的 `ExitBootServices` 成功之后直接进入拍 8 的 count 提交与 storage 定稿，中间没有第三个写段；引文里的删除注释只作定位，不是现行动作（完整账见 §3.2.1）。

**MapKey 是什么**：UEFI 给每次 `GetMemoryMap` 的结果发一个标识符（`MapKey`，一个 `usize`）；此后任何改变内存图的操作（包括 `ExitBootServices` 内部的簿记）都会使旧 key 失效。`ExitBootServices` 的契约要求调用方传入"我最后一次 `GetMemoryMap` 时的 key"——固件用它确认调用者看到的内存图与它自己记录的一致；不一致就以 `EFI_INVALID_PARAMETER` 拒绝，调用方重取内存图、重新分类、再试。这就是分类必须与退出共用同一循环的原因：两者必须读同一份快照。

配对循环的失败面按状态码分派：`BUFFER_TOO_SMALL` 只在第一次 EBS 之前允许扩容缓冲；`EFI_INVALID_PARAMETER` 允许重分类一次（MapKey 变了）；其他状态立即失败。总尝试次数用尽或复用缓冲仍不足时 fail-closed，不沿用旧分类结果。

动作 2 的 A+D 顺序展开（表内只留主约束）：

- 读 ELF → `reserve_kernel_span` 用 `AllocateAddress` 把 `[kern_phys_base, +kern_size)` 整段预订为 `LOADER_DATA`（失败即 halt，不降级）→ `record_kernel_span` 记下该跨度（在拷贝**之前**）→ 把段拷进已预订的 LMA → 载模块。
- root/bump 的分配是动作 1、发生在预订之后；预订先于**此后**的一切分配。
- ELF 读入的 `Vec` 是更早的一笔 UEFI 池分配，无法排到预订之前——布局要读完字节才知道；若它落进跨度，`AllocateAddress` 拒绝并停机，不会静默重叠。
- 因此固件此后发出的任何页（模块、arena、池缓冲）都不可能在镜像跨度内，root/bump 也不可能落在镜像的拷贝目标之下；`alloc_module_pages` 的拒绝逻辑与 root/bump/模块的复查保留为第二道防线。

#### 4.2.2 描述符分类：五步判定与两道内联闸

本节的“判定步 N”是描述符分类的五步，不是 §4.1 的 Step 0–4；两套编号不互相指代。

**动作 6 的分流规则**（[`fn classify`](<os/boot-shim/src/ebs_map.rs:fn classify>) 与单条判定 [`fn adjudicate`](<os/boot-shim/src/ebs_map.rs:fn adjudicate>)）。判据不是"类型标签加 RAM 顶两遍扫描"，而是与 `adjudicate` 源码注释逐行对应的五步加两道内联闸：

| 步 | 判据 | 命中动作 | 未命中 |
|---|---|---|---|
| 判定步 1 | 类型已知（≤ `LAST_KNOWN`）、属性仅 `KNOWN` 位、无 `AMBIGUOUS`/`PROFILE_ONLY`、无 ISA 掩码（`ISA_VALID`/`ISA_MASK` 均不置位） | 继续 | 报错（`UnknownType`/`AmbiguousAttribute`/`ProfileAttribute`/`UnknownAttribute`/`IsaAttributes`） |
| 判定步 2 | 类型 ∈ {`UNACCEPTED`, `PAL_CODE`} | `RefuseToBoot`（先于一切后续步） | 继续 |
| 内联闸 | `checked_shape`：描述符结构可表示 | 继续 | 报错 |
| 判定步 3 | 类型 ∈ {`RESERVED`, `UNUSABLE`} → `Excluded`；∈ {`MMIO`, `MMIO_PORT_SPACE`} → `DevicePath` | 定档返回（先于属性判定） | 继续 |
| 内联闸 | `checked_len`：物理范围可表达 | 继续 | 报错 |
| 判定步 4 | 属性含 `TYPE_INDEPENDENT` | `Side` | 继续 |
| 判定步 5 | 属性 `WRITE_BACK == 0` → `Side`；否则按类型表：`CONVENTIONAL`→可分配、`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`→`ReservedMapped`、其余→`Side` | 定档返回 | —（类型表兜底） |

上表已给五类落点；这里只补充两道内联闸的覆盖差异，不再复述落点。**整图级拒绝（不属于单条五步）**：`classify` 在逐条 `adjudicate` 之后还会拒绝两类整图问题——同一地址域的描述符两两重叠（`PolicyError::OverlapsDescriptor`），以及 `WIRE_MEMMAP_CAP`/`WIRE_RESERVED_CAP`/`WIRE_SOURCES_CAP`/`side_cap` 四类容量上限。前者与 §2.5 的模块重叠检查同级，命中即整张图 fail-closed，不是跳过该条继续。**两道内联闸的覆盖范围不同**：`checked_shape` 在 Step 2 之后、Step 3 之前，所以所有未被 Step 2 拒绝的描述符（含 `Excluded`/`DevicePath`）都经过它——到达这一步的前提是已经过了 Step 1；`checked_len` 在 Step 3 之后、Step 4 之前，而 Step 3 命中者已经定档返回——`Excluded` 与 `DevicePath` **不经过** `checked_len`。读表前先把判据令牌立住（与 [`ebs_map.rs`](os/boot-shim/src/ebs_map.rs) 的模块常量逐项对应）。先立两个类型名：`CONVENTIONAL` = UEFI `EfiConventionalMemory`（固件声明可自由分配的 RAM）；`WRITE_BACK` = UEFI 内存属性位 `0x8`（写回缓存；不置位表示这段没有普通可缓存语义，不能当普通 RAM 发给 VM）。

| 令牌 | 取值/含义 | 失败落点 |
|---|---|---|
| `LAST_KNOWN` | `UNACCEPTED`（15）；baseline 已知类型是 0–15 | 类型 > 15 → `UnknownType`（OEM/OS-loader 保留带） |
| `KNOWN` | baseline 定义的全部属性常量（`UNCACHEABLE` 到 `ISA_MASK`，共 16 个常量；`ISA_MASK` 本身是 16 位掩码，合计覆盖 31 个位位置） | `a & !KNOWN != 0` → `UnknownAttribute` |
| `AMBIGUOUS` | 两套官方文本对同一位置命名冲突的位（`WRITE_PROTECT`/`READ_PROTECT`/`EXECUTE_PROTECT`/`NON_VOLATILE`/`MORE_RELIABLE`） | 任一置位 → `AmbiguousAttribute` |
| `PROFILE_ONLY` | 只存在于某一固件编码的位（`READ_ONLY`/`SPECIAL_PURPOSE`/`CPU_CRYPTO`） | 任一置位 → `ProfileAttribute` |
| `ISA_VALID`/`ISA_MASK` | ISA 有效性位与掩码 | 任一非零 → `IsaAttributes` |
| `TYPE_INDEPENDENT` | `RUNTIME \| NON_VOLATILE \| SPECIAL_PURPOSE`（三个"与内存类型无关"的属性位）；当前只有 `RUNTIME` 能到达 Step 4——`NON_VOLATILE` 在 Step 1 被 `AMBIGUOUS` 拦下、`SPECIAL_PURPOSE` 被 `PROFILE_ONLY` 拦下 | 任一置位 → Step 4 落 `Side` |
| `checked_shape` | 结构可表示：页数非零、`pages×4096` 不溢出、起点页对齐（`MMIO_PORT_SPACE` 除外）、`start+len ≤ 2^64`、虚拟起点对齐且不溢出 | `ZeroPageCount`/`LengthOverflow`/`UnalignedStart`/`RangeOverflow`/`UnalignedVirtualStart`/`VirtualRangeOverflow` |
| `checked_len` | 在 `checked_shape` 之上再要求 `start+len ≤ pa_limit` | `OutOfPhysRange` |
| `RefuseToBoot` | Step 2 的错误出口：整张图分类失败、启动中止（fail-closed），不是“跳过该条” | `PolicyError::RefuseToBoot` |
| `Side` | 侧桶：既不入 free list 也不入保留表，只计入诊断侧计数 | — |

手算五例（前两例成功定档；例 2 在 Step 3 即定档返回；后三例分别是 Step 1 报错、`Side` 定档与 `checked_len` 报错）：

| 例 | 输入 | 逐步判定 | 落点 |
|---|---|---|---|
| 1 | `CONVENTIONAL` + `WRITE_BACK` | Step 1 过 → Step 2 不中 → 形状过 → Step 3 不中 → 范围过 → Step 4 不中 → Step 5 类型表命中 | `可分配` |
| 2 | `RESERVED` + `WRITE_BACK=0` | Step 1 过 → Step 2 不中 → 形状过 → Step 3 命中 `Excluded`，定档返回 | `Excluded`；Step 5 的 `Side` 永远不会看到它——这就是“类型强制档位先于属性”的可执行含义 |
| 3 | `CONVENTIONAL` + `attr::UNKNOWN_FOR_TESTING` | Step 1 的 `a & !KNOWN != 0` 命中，报 `UnknownAttribute` | 整张图作废 |
| 4 | `LOADER_DATA` + `WRITE_BACK` + `RUNTIME` | Step 1 过（`NON_VOLATILE`/`SPECIAL_PURPOSE` 已被 `AMBIGUOUS`/`PROFILE_ONLY` 拦下）→ Step 2 不中 → 形状过 → Step 3 不中 → 范围过 → Step 4 命中 `Side` | `Side`（属性独立档先于类型表） |
| 5 | `CONVENTIONAL`，起址 `pa_limit - 0x1000`，页数 2 | Step 1 过 → Step 2 不中 → 形状过 → Step 3 不中 → `checked_len` 命中 `OutOfPhysRange`（范围尾越过 `pa_limit`） | 整张图作废 |

分类器没有 `ram_top` 变量，也没有“第二遍按 RAM 顶分流”这一步。上表五类落点已完整；这里只留一条总括：`Excluded`、`Side` 与 `DevicePath` 都不进 free list，也不进 `reserved_regions`。

`Excluded`、`Side` 与 `DevicePath` 为什么不进两份表：它们是固件声明“不要分配 / 不是内存 / 带独立语义”的区间，交给内核只会让它在 4 KiB 粒度下错误地恒等映射设备窗口，或把保留段当空闲内存。测试内核调用 [`fn build_memmaps`](<os/boot-shim/src/uefi_helpers.rs:fn build_memmaps>) 时的诊断输出是 `boot-shim: memmaps conv=… reserved=… unmapped=…`（空表时打印 `(empty map)`）。

另有一条与分流正交的界：**凡到达 `checked_len` 的描述符**（含 `Side`；`Excluded`/`DevicePath` 在 Step 3 已定档返回，不经过它）都必须让整段落在页表可表达的物理地址上限 `pa_limit` 内，越界即整张图作废（`PolicyError::OutOfPhysRange`，由 `classify` 包成 `MapError::RejectedDescriptor` 上抛），不做截断。`pa_limit` 是页表地址宽度（aarch64 / x86_64 各自的物理上限），不是 RAM 顶；`Side` 虽不进两份表，仍受这道界约束。

`AllocateType::MaxAddress(limit)` 的语义是“分配出来的末字节不超过 `limit`”（固件约束是 `base + size - 1 <= limit`），所以传“最后一个可接受的字节”即 `bound - 1` 就足够；再减请求大小反而更严，会拒绝本可容纳的请求。这个界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口的物理上界)`：bump 池与 arena 既要能被"恒等写通道"写到（构建期间），又要能被 VM 的 Direct Map 窗口表示（构建之后使用者要能通过窗口访问）。**时序要分开**：动作 1–5 直接按物理地址写 root/bump/arena 时，靠的是 Boot Services 仍生效的 1:1 映射；`BootWindows` 那张恒等窗口表是动作 9 建根时才安装的，不是早期写的前提。建根之后再写页表页，才依赖这张窗口表。

动作 1、2 的界（root/bump 分配与 ELF 预订；这里引的是分配上界函数，不是 `adjudicate` 的 Step 1/2）：

```rust
// 代码引用（略去文档注释），os/boot-shim/src/uefi_helpers.rs。
fn bootstrap_alloc_limit() -> u64 {
    boot_dm_admissible_end()
        .checked_sub(1)
        .expect("DM-admissible bound must sit above address zero")
}

pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("Failed to allocate root page below DM-admissible bound");
    PhysBytes(ptr.as_ptr() as u64)
}
```
### 4.3 两种文件来源共用的取件函数

本节的 A/B/C/D 是**装载防线**，不是 §1.1 的次序约束 D1–D5；后文写“防线 A”时不再重复限定。

模块契约是本篇里最容易在改动时踩坏的东西：三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）的“顺序”部分没有编译期检查，只能靠装载顺序、运行期断言与测试兜住；两个直核腿另有 `NAME_POOL` 与 `MODULE_NAMES` 的编译期逐字节对账（本节末）。装载期防线是 A/B/C 三道（接收侧另有 D，共四道）：**A** 每个段的文件字节区间与该段及**所有**段的目标 LMA 区间不相交（`first_source_destination_overlap`）；**B** 各段的目标 LMA 区间两两不相交（`first_overlapping_pair`）；**C** 模块区间不落进内核物理跨度：两条 UEFI 腿用 `alloc_module_pages` 拷贝前拒绝 + `assert_modules_clear_of_kernel` 装载后复查；U-Boot shim 腿用编译期 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 与 root/bump 复查。文件字节区间彼此之间的重叠不查——只读拷贝无害。任一条命中即 halt。

Rust 侧的 A/B 是 ELF 装载新增的两道防线（守内核自身 PT_LOAD 段，本节）；C/D 才是 C 的 `overlaps`（§2.5）两类检查的落点：C 守模块 vs 内核跨度（§4.2 的 `record_kernel_span`、`alloc_module_pages`、`assert_modules_clear_of_kernel`），D 在接收侧重查同一对关系并追加模块 vs 模块（`KernelInfo::validate_requiring_sources` 的两段断言——内核跨度那段与 C 故意重复，因为接收侧不能信任发送侧已查；§4.4）。

```rust
// 结构节选（保留常量名与形状；数组元素以 ... 省略；完整 12 项见源码），os/boot-shim/src/loader.rs 与 os/libs/minix-boot/src/kernel_info.rs。
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";
pub const DUMPDTB_PATH: &str = "/dumpdtb.dtb";
pub const MODULES_DIR: &str = "/EFI/minix/modules";

// 〔编注〕minix-boot：唯一权威表（12 项，名称 + 进程号；完整表见源码）。
pub const BOOT_MODULE_IDENTITIES: [BootModuleIdentity; NR_BOOT_MODULES] = [ ... ];

// 〔编注〕两个派生视图：顺序与长度由权威表钉住，不另立抄本。
pub const MODULE_NAMES: [&str; NR_BOOT_MODULES] = [ ... ];
pub const BOOT_MODULE_PROC_NRS: [ProcNr; NR_BOOT_MODULES] = [ ... ];

// 〔编注〕boot-shim：只重导出同一份派生视图，不另立第二份。
pub use minix_boot::MODULE_NAMES;
```

- 名字表有 12 项（复算：`sed -n '/pub const BOOT_MODULE_IDENTITIES/,/^];/p' os/libs/minix-boot/src/kernel_info.rs | grep -o '"[a-z]*"' | wc -l` 输出 12），`NR_BOOT_MODULES = 12` 与表同住 `minix-boot`；boot-shim 与内核的 strict 门读同一份常量（设计项）。12 不是随手定的数：它等于 C `boot_image` 表的用户态段长度，少一项会有服务器没有启动名，多一项则没有对应的进程号可绑（C 侧 `NR_BOOT_PROCS` 与本篇的 `NR_BOOT_MODULES` 是同一张表的两个计数口径；每个名字绑哪个进程号，由 `BOOT_MODULE_IDENTITIES` 表项的 `proc_nr` 字段给出；`MODULE_NAMES` 与 `BOOT_MODULE_PROC_NRS` 都是该表的 const 派生视图——[`kernel/table.c`](minix3/minix/kernel/table.c) 的 `{DS_PROC_NR, "ds"}` 等条目）。
- 顺序按 C 的 boot image 表（[`kernel/table.c`](minix3/minix/kernel/table.c) 里 `struct boot_image image[NR_BOOT_PROCS]` 的用户态段）排列：`ds, rs, pm, sched, vfs, memory, tty, mib, vm, pfs, mfs, init`。
- **位置不是进程号**。内核把第 `i` 个模块绑定到 `BOOT_MODULE_PROC_NRS[i]`，而后者里的数字来自 `com.h`——例如 `ds` 在位置 0，进程号是 6。名字表这一行代码上方给了这条注解，它是顺序契约的一部分。

装载循环：

```rust
// 代码引用（略去函数文档注释；`assert!` 的容量前置检查保留原样；行内英文注释译为中文并有一处压缩），os/boot-shim/src/loader.rs:fn load_boot_modules_into。
pub fn load_boot_modules_into<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
    out: &mut [BootModule],
) -> usize {
    assert!(
        out.len() >= MODULE_NAMES.len(),
        "boot-shim: module slot array has {} slots, the contract needs {}",
        out.len(),
        MODULE_NAMES.len()
    );
    for (slot, &name) in out.iter_mut().zip(MODULE_NAMES.iter()) {
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
        let data = data.as_slice();

        // 修剪到 ELF 的运行期文件长度（最后一个 PT_LOAD 的
        // 文件尾）——模块装载里唯一需要 ELF 语义的层；两条腿的输入在此统一。
        let used = elf_file_end(data).unwrap_or(data.len());
        let data = &data[..used];

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

        // `name` 已是 `MODULE_NAMES` 常量表里的 `&'static str`——无需堆分配。
        *slot = BootModule {
            name,
            start: PhysBytes(phys_base),
            len: data.len(),
        };
    }
    MODULE_NAMES.len()
}
```

四处具体事实：

- `num_pages = (data.len() + 4095) / 4096`：向上取整到页。分子写 4095 而不是 4096 是按整数除法做 ceil 的标准形——`x/4096` 本身向下取整，先加 `4096-1` 才让任何非零余数进位；写 4096 会在 `data.len()` 恰为整页时多要一页，写小了（加数 A ≤ 4094）时，只有余数 r 满足 `r > 0 && r < 4096 - A` 才会少一页——反例：`data.len()=4095`（r=4095）、`A=4094` 时 `(4095+4094)/4096 = 1`，仍是一页。所以 A=4095 是让「任何非零余数都进位」成立的最小加数。
- 分配出来的页里，`data.len()` 之后的余量被显式清零（`write_bytes(dest.add(data.len()), 0, remaining)`）。模块镜像按 ELF 运行期文件长度（最后 PT_LOAD 的文件尾）拷贝；非 ELF 或畸形头回退整段。页尾的余量清零保证同一页内的任何对齐空洞都是确定值，而不是上一轮使用留下的内容。
- `BootModule.len` 记录的是**修剪后的运行期文件长度**，不是页数，也不是盘上整份文件字节数（`.comment`/`.symtab`/`.debug_*` 等非 ALLOC 尾部被裁掉）。消费者按 `[start, start+len)` 计算占用区间。
- 缺件是 panic 而不是 `continue`。错误消息里带着模块名、路径与总数，落点在 boot-shim 侧；如果改为静默跳过，错误会推迟到内核侧：`validate` 只查 `boot_modules` 非空，按位置绑定会静默少初始化几个进程，现场没有缺失的名字。

OpenSBI 路径的 `FileLoader` 实现是查表：

```rust
// 代码引用（略去文档注释；块内中文注释为改述/译述；source-symbol: entry_path_eq/UbootFileLoader），os/boot-shim/src/opensbi_helpers.rs:fn entry_path_eq 与 FileLoader for UbootFileLoader。
fn entry_path_eq(entry: &BootFileEntry, path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() >= BOOT_FILE_PATH_MAX {
        return false;
    }
    entry.path[..bytes.len()] == *bytes && entry.path[bytes.len()] == 0
}

impl<'a> FileLoader for UbootFileLoader<'a> {
    fn read<'b>(&'b self, path: &str) -> Option<crate::loader::FileBytes<'b>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot's `fatload` placed `entry.len` bytes at
        // `entry.phys_addr`. The region is identity-mapped during boot
        // and not concurrently modified.
        let slice =
            unsafe { slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize) };
        // `read` 只处理字节来源，不做任何 ELF 语义。表里的
        // `entry.len` 已是 `build_shim_table` 用 `staged_elf_used_len` 算出
        // 的修剪值——这里再跑 `segment_iter` 是对已修剪数据的幂等无效功。
        // U-Boot 腿的首次修剪在 `build_shim_table`；`load_boot_modules_into` 的
        // `elf_file_end` 是统一入口的幂等兜底（对已修剪输入 no-op），不是第二个有效修剪点。
        // U-Boot staged the file in RAM; hand out a borrow instead of a copy
        // The table is written once before the shim runs and is
        // not modified while the bytes are in use.
        Some(crate::loader::FileBytes::Borrowed(slice))
    }
}
```

`entry_path_eq` 把"路径必须 NUL 结尾"变成匹配条件的一部分：`entry.path[bytes.len()] == 0`。表由 `uboot_main::build_shim_table` 用 `BootFileTable` 类型在 shim 的 `.bss` 内构建：它写入路径字节 + 一个终止 NUL；字段其余字节由零初值保证，路径超界由构建时的两条 assert 当场拦下（消息 `does not fit the BootFileEntry field`）。因此 `entry_path_eq` 的 NUL 条件是构建器与查表器之间的契约；构建器若写坏路径，`find` 返回 `None`，随后 `read_required` 报"找不到内核"。`BootFileTable::valid_entries` 里 `entry_count` 被夹到容量上限（`(self.entry_count as usize).min(BOOT_FILE_TABLE_MAX_ENTRIES)`），因此一个写坏的 `entry_count` 只会让多余的条目被忽略，不会越界读。

**若把 `MODULE_NAMES` 里 `tty` 与 `mib` 两项互换会怎样**：分腿看。三条 v1 发送腿的装载会全部成功（12 个文件都在），数组长度仍是 `[&str; NR_BOOT_MODULES]`（数量没变），strict 门的**按位置核名**也只会看到“每个位置都有名字”，不会发现两个名字互换了。两个直核腿不同：`bootface_common` 的 `NAME_POOL` 与 `boot_shim::loader::MODULE_NAMES` 有逐字节的编译期对账，互换会让构建直接失败——反事实只对三条 v1 发送腿成立。但是位置 6 上的 `BootModule` 记的是 `mib` 的镜像与名字，内核按位置把它绑到 `tty` 的进程号上；位置 7 上则是 `tty` 的镜像，被绑到 `mib` 的进程号。启动过程依旧能走到 `kmain`，进程表也能建起来；症状出现在第一次对 `tty` 或 `mib` 发系统调用时——调用被打到错误的进程号，表现为"服务没有响应"或"收到协议外的回复"，与"装载时顺序错了"之间隔着整个启动流程。
### 4.4 五条腿的 `BootReport<'_>` 发送视图与接收侧 `KernelInfo` 构造

五条生产腿（两条 UEFI 链、U-Boot shim、两个直核面）在**数据形状**上收敛（形状收敛，不保证发送侧视图完整；UEFI 腿的 `source_records` 视图见 §3.2.1/§3.7；各腿如何产出两张表以 §3.2 为权威，本节只讲构造入口）：五条腿的发送侧都只构造 `BootReport<'_>`——两个直核面直接借用栈上的 `BootStorage`，三条 v1 腿（两条 UEFI 链 + U-Boot shim）把它序列化进 arena 协议；接收侧统一走 `land_and_store`：两个直核腿经 `store_boot_report`，三条 v1 腿经 `arch_boot_resume_v1` 解码后由 `store_kernel_info` 包装；两条路径都先把切片深拷进内核 `.bss`，再生成唯一的 `KernelInfo`。发送侧不构造 `KernelInfo`。这是“层内收敛原则”（§3.5 问题二：收敛数据形状，不合并引导形态层）的落点：每条路径仍各自产出自己的两张表（`memmap` 与 `reserved_regions`），共享的是形状而不是一个构造函数。

```rust
// 代码引用（节选；略去泛型 where 子句、函数体中的诊断打印与注释，`register_panic_diagnostic()` 一行也省略），os/kernel/src/lib.rs:fn arch_boot_finish + os/libs/minix-boot/src/kernel_info.rs:trait KernelInfoView。
fn arch_boot_finish<A, H>(
    report: &minix_boot::BootReport<'_>,
    authority: minix_boot::RamAuthority,
    root_page: PhysBytes,
) -> ! {
    report.validate_before_tables();
    report.check_boot_postconditions();
    let kinfo = store_boot_report(report, authority);
    arch_boot_impl::<A>(kinfo, root_page);
    unsafe { H::jump_to_kmain(kinfo) }
}

pub trait KernelInfoView {
    fn memmap(&self) -> &[MemoryRegion];
    fn reserved_regions(&self) -> &[MemoryRegion];
    fn boot_modules(&self) -> &[BootModule];
    fn platform_sources(&self) -> &[PlatformDescSource];
    fn param_buf(&self) -> &[u8];
    fn source_records(&self) -> &[SourceRecord];
    fn kern_virt_base(&self) -> VirBytes;
    fn kern_phys_base(&self) -> PhysBytes;
    fn kern_size(&self) -> u64;
}
```

`reserved_regions` 的取值按腿的细目以 §3.2 为权威（UEFI `ReservedMapped`、U-Boot/riscv 直核 DTB 保留段、aarch64 直核 `fw_end=0`）；本节只保留构造入口，不逐腿复述。

| 字段 | 谁填（发送侧 `BootReport` 视角；接收侧统一落 `.bss`） | 值从哪来（来源细目以 §3.2 为权威，本表只留构造入口） |
|---|---|---|
| `memmap` | 发送侧（五条腿都填 `BootReport`）；接收侧落 `.bss` 后进入 `KernelInfo` | 见 §3.2 的 `memmap` 行：UEFI 用配对循环最后一次成功 `GetMemoryMap` 的 conventional 列表；U-Boot 走 `argv[0]`（`bootelf -p` 参数通道的首个参数 = DTB 物理地址，由 shim 在 boot.scr 里传入；不是进程 argv），riscv64 直核走 `a1`，aarch64 直核从固定 `DTB_PA = DRAM_BASE` 读（无入参）；同一份解析在 minix-boot |
| `reserved_regions` | 调用方 | 见 §3.2 的 `reserved_regions` 行：UEFI 用 `ReservedMapped` 列表（`Excluded`/`DevicePath`/`Side` 不进），U-Boot/riscv 直核用 DTB 以 `KERN_PHYS_BASE` 裁剪的保留段，aarch64 直核用 `fw_end = 0` 关闭固件前缀裁剪 |
| `kern_virt_base` / `kern_phys_base` / `kern_size` | 装载函数 | ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)`；装载器另断言 `e_entry == min(vaddr)`（UEFI 与 U-Boot 两条 loader 各一处），直核面由 `KERN_*` 常量提供；两个生产直核面与 x86_64 测试直核入口的跨距均由 L9 对账（按架构选 `bootface.rs`/`bootface_a64.rs`） |
| `user_sp`（v10 起不再是字段） | 消费点直接取常量 | `minix_types::USER_STACK_TOP` 单一权威常量；`arch_boot` 的用户栈映射、`build_vm_handoff` 的 `user_sp`、`fill_kerninfo_page` 的 `kui_user_sp` 与 `GET_KINFO` 的 `user_sp` 都读它 |
| `kern_stack_top`（v10 起不再是字段） | 访问器派生 | `KernelInfo::kern_stack_top()` = `kern_virt_base + kern_size`——**镜像尾**（末段 memsz 端点，Rust 链接脚本在 `.bss` 末尾 `ALIGN(0x200000)` 收口后的地址；与 C 的 `_kern_size` 不可混用）。它与 `kernel_boot_stack_top` 的区别、收口填充与清零语义见下表后的 bullets |
| `syscall_entry`（v10 起不再是字段） | 消费点直接取函数 | 实际门地址只由各架构的 `syscall_entry_va()` 提供（C 的 `kinfo_t` 也没有这个字段） |
| `boot_modules` | 装载函数 | 按 `MODULE_NAMES` 顺序的 12 项 |
| `source_records` | 直核腿与 U-Boot shim 放进 `BootReport`；UEFI 腿由分类器与 DTB claims 两阶段直写 wire（storage 切片为空）；接收侧按腿落 `.bss` | U-Boot shim 与两个直核腿填 `BootReport`；两条 UEFI 腿的 source 直写 wire；接收侧按腿落 `.bss` 后构造 `KernelInfo`（§3.2 与 §4.2 的写入段）。startup_objects 参数已退场（§3.2.1） |
| `authority` | 直核面接收侧 `store_boot_report` 参数；v1 接收侧由 wire 解码进 `KernelInfo.authority` | 两个直核面在 `arch_boot` 调用点传 `RamAuthority::DTB`；v1 接收侧由 wire 的 `ram_authority` 解码写入，UEFI 腿的发送侧在 `Serializer::new` 里传 `InputObjectId::UefiMemoryMap`，接收侧解码为 `RamAuthority::UEFI`（§3.2 的绑定说明） |
| `platform_sources` | 调用方 | UEFI：Configuration Table 扫描结果（DTB/RSDP，带 `len`）；OpenSBI：`[DTB]` 或空；两个直核面：DTB（`with_optional_len(DTB, …)`，各一条） |
| `param_buf` | 调用方 | 各条路径的 normalizer 输出：UEFI 读 `LoadOptions`，OpenSBI/直核读 DTB `/chosen/bootargs` |

`kern_stack_top` 的三条展开（v10 起该值是访问器派生，不是字段，三条语义不变）：

- **两个对象**：`kern_stack_top` 是镜像尾（Rust 链接脚本 `ALIGN(0x200000)` 收口后的地址，2MiB 对齐）；`kernel_boot_stack_top` 是 64KiB 引导栈数组自身的顶，定在收口**之前**。C 的 `kernel.lds` 没有这道收口，`_kern_size` 算到 `.bss` 结束，两者不可混用。
- **收口填充**：数组顶按 16 字节对齐、镜像尾按 2MiB 对齐，二者相差 0 到 2MiB−16。`jump_to_kmain` 把 sp 置到镜像尾（[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs:jump_to_kmain) 的 `mv sp, stack_top`），栈向下生长先穿过这段填充；填充属于末段 memsz（三个 `os/kernel-image/*.ld` 的收口注释都写明），已映射、可写。填充等于给引导栈一段额外余量，不改变安全性。
- **清零语义**：只对跨镜像腿成立（loader 按 `memsz` 清零）；两个直核面的 `_start` 只清到 `__bss_end`，收口填充不保证为零——栈是先写后读，安全性不依赖清零。

`boot_validate_and_prepare` 是这份结构在内核侧的入口校验（直核面前置 pre-table 与 postconditions；v1 接收面只有 postconditions 先读过部分字段，见 §4.4.1），它在建表之前先做校验：

```rust
// 节选（保留校验段；尺寸阶梯与诊断路标宏略去——阶梯的代码与解码在 02 篇的 3.4 节；`kern_stack_top` 的 16 字节对齐断言省略；注释省略，分配器注册检查以省略号代替），os/kernel/src/lib.rs:fn boot_validate_and_prepare。签名是 fn boot_validate_and_prepare<A: BootArch>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64) -> BootPageSupport<A>。
    let kv = kern_virt_base.0;
    let kp = kern_phys_base.0;
    let ks = kern_size;
    // 派生，非入参：`kern_stack_top` 在函数内由两个镜像跨度字段算出。
    let kern_stack_top =
        VirBytes(kv.checked_add(ks).expect("arch_boot: kern_virt_base + kern_size overflows u64"));

    assert!(ks > 0, "arch_boot: kern_size must be > 0");
    assert!(
        kp.is_multiple_of(0x1000),
        "arch_boot: kern_phys_base must be page-aligned"
    );
    assert!(
        kv.is_multiple_of(0x1000),
        "arch_boot: kern_virt_base must be page-aligned"
    );
    ... // require_boot_region：boot_alloc_region() 为 None 即停机（无兜底）
    let support = detect_boot_page_support::<A>()
        .unwrap_or_else(|e| panic!("arch_boot: page-size capability facts: {e}"));
    // 〔编注〕窗口表的物理上限不在这里检查：调用方组装完窗口后调
    // `let windows = windows.check(support.phys_addr_limit());`（返回 `CheckedWindows`）。
    support
```

内核侧的校验按**防线层次**组织，不是所有腿共用一条执行序：

1. `boot_validate_and_prepare`（建表前）：`ks > 0`、两个基址页对齐、boot bump 区间已注册（`boot_alloc_region()` 非 `None`，否则停机；`pt_alloc` 由本函数按 `is_registered()` 守卫注册，不是 fail-closed）、页大小能力、`kern_stack_top` 16 字节对齐。物理上限不在这个函数里查——窗口表由调用方随后组装，组装完调 `BootWindows::check(phys_addr_limit)` 逐窗比较 `end <= phys_addr_limit`，并把返回的 `CheckedWindows` 交给建根函数。

**窗口类型的边界规则**（裸地址对与类型化地址对之间的取舍）：裸 `(u64, u64)` 只允许出现在固件/分配器边界——`boot_alloc_region()` 的返回值、U-Boot 主函数把 `BootPrepareResult.bump` 拆成两个数交给 `init_boot_pt_alloc` 的那一处；`enter_kernel_image_v1` 的 `shim_windows` 已是 `&[PhysRange]`。进入内核内部一律 `PhysRange`。`BootWindows::push` 是转换点：`PhysRange::new` 拒绝反向区间，`push` 自己拒绝空区间（`start < end`），两条负控都有宿主测试（`boot_windows_reject_reversed_ranges` / `boot_windows_reject_empty_ranges`）。`EMPTY_PHYS_RANGE` 只是未用槽的填充哨兵，`as_slice` 只暴露已提交前缀，不会把它当成窗口。
2. `validate_before_tables`（pre-table gate）：`kern_size != 0`、`memmap` 非空、`param_buf.len() <= 1024`——它**不查** `boot_modules` 非空（QEMU 测试内核合法携带空模块表）。这里只列与 boot 路径直接相关的项，**不是完整清单**：共享校验体 `validate_view` 还检查 `platform_sources` 的空址/kind/溢出、两表升序与两两不相交、非零长度与页对齐等，完整清单以源码为准。
3. `check_boot_postconditions`：镜像地址下限（`kern_virt_base > u32::MAX`）。v10（设计裁决）之后只剩一条可被剩余字段证伪的关系——`kern_virt_base` 必须大于 `u32::MAX`（4 GiB 门；这不是各架构的用户/内核分界，而是“内核不能在 4 GiB 以下”的保守下界）；“`syscall_entry` 落在镜像跨度内 / `kern_stack_top` 不越过跨度上界 / 用户栈不越入内核半区”三条读取派生字段的断言随字段一起删除（用派生值重算同一条等式只是重抄，不构成检查）。
4. `KernelInfo::validate_requiring_sources`（`kmain` 入口）与生产腿的 `validate_requiring_sources`（同一 strict 变体）：前者复核非地址不变量、检查生产内核的 `boot_modules` 非空，并复核内核镜像与各模块、模块与模块的半开区间不重叠（相切放行；跨度加法用 `checked_add`，防 release 回绕把判断变成恒真）；`KernelInfo::validate`（非 strict，保留给测试/诊断，`kmain` 不再调用）在 `source_records` 非空时也查“来源记录覆盖投影内存图”（条件性检查，见 §3.2）。`ValidationLevel` 三档是 `BeforeTables`/`Standard`/`Strict`；`Strict` 不带载荷，`authority` 从 `KernelInfo.authority` 读取，`validate_requiring_sources()` 无参。后者按结构体自带的 `authority` 要求 `source_records` 覆盖投影内存图，并要求内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内（v10 之前这条由 `startup_objects` 的一条记录回答；该字段与分区已随设计裁决删除）。

#### 4.4.1 门链与逐腿执行序

腿间实际执行序要说清：直核面是 **strict（`validate_requiring_sources`，两个 `bootface` 在调 `arch_boot` 之前显式调用）→ pre-table（2）→ postconditions（3）→ `store_boot_report`（`arch_boot_finish`，完整落一次）→ boot_validate（1，`arch_boot_impl` 开头）→ kmain strict validate → `check_boot_postconditions` 复核**；v1 接收面是 **strict → postconditions（3）→ `resume_boot_pt_alloc` → boot_validate（1，`resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 strict `validate` → `check_boot_postconditions` 复核**，且**不经过** pre-table（`resume_additive` 体内没有 `validate_before_tables`）。接收面的 3 有可见调用点：`arch_boot_resume_v1` 在 `kinfo.validate_requiring_sources()` 之后、`boot_alloc::resume_boot_pt_alloc` 之前调用 `kinfo.check_boot_postconditions()`。

按时间顺序把两条接收腿并排看（从上到下；`kmain` 列是最终入口的复核，不是首道门）。v1 发送侧不在表内：它在建根前只跑 `boot_validate_and_prepare`，不跑 pre-table/strict（§4.2 动作 9）。

| 时间序 | 直核面 | v1 接收面 | 最终入口 |
|---|---|---|---|
| 1 | `run_self_boot` 在 `arch_boot` 前调 strict | `arch_boot_resume_v1` 解码后、`resume_boot_pt_alloc` 前调 strict | — |
| 2 | `arch_boot_finish` 的 pre-table | 不经过 | — |
| 3 | `arch_boot_finish` 的 postconditions | 解码后、建表前的 postconditions | — |
| 4 | `store_boot_report` 完整落一次 | `resume_boot_pt_alloc` | — |
| 5 | `arch_boot_impl` 开头的 boot_validate | `resume_additive` 内的 boot_validate | — |
| 6 | `jump_to_kmain` | `store_kernel_info`（同址跳过字节拷贝）→ `jump_to_kmain` | strict + postconditions 复核 |

下表按门列 bug 类，不按时间排序。

下表按门列出各校验点挡的 bug 类（共十三行：十二行是门、一行是“规划期（不是门）”；门列有五个命名门加一个共享校验体；上面的编号链仍是四项）（同名项在不同门重复挡是故意冗余；`validate_before_tables` 即 §3.6 注释里的 pre-table gate）：

| 门 | 校验点 | 挡的是哪一类 bug |
|---|---|---|
| `boot_validate_and_prepare` | `ks > 0` | boot-shim 没装载成功却继续往下走 |
| `boot_validate_and_prepare` | 两个基址页对齐 | 布局常量写错（链接脚本改了而工具没跟） |
| `boot_validate_and_prepare` | `kern_stack_top` 16 字节对齐 | ABI 要求（AAPCS64 / SysV 的栈对齐） |
| `boot_validate_and_prepare` + 窗口组装后 | `detect_boot_page_support` 成功；每个窗口 `end <= phys_addr_limit`（`BootWindows::check` 返回 `CheckedWindows`，建根只收该 token） | 页大小能力缺失，或某个 boot identity 窗口越过平台物理地址上限（启动最早、最难归因） |
| `validate_before_tables` | `kern_size != 0` | `kmain` 之前就拿到空镜像 |
| `validate_before_tables` | `memmap` 非空 | 内存图丢了 |
| `validate_before_tables` | `param_buf.len() <= 1024` | 参数缓冲超过 C 的固定容量 |
| `check_boot_postconditions` | `kern_virt_base > u32::MAX`（4 GiB 门；v10 后唯一剩余的后置条件） | 内核镜像落进 4 GiB 以下，保守下界被破坏 |
| `KernelInfo::validate_requiring_sources`（`kmain` 入口） | 复核 `kern_size`/`memmap`/`param_buf` 非地址不变量、`boot_modules` 非空（生产内核的 `kmain` 路径；QEMU 测试内核不编译此入口）与 `source_records` 的权威覆盖；按存储后的 `KernelInfo.authority` 复核记录集与内核跨度覆盖 | 同前几行，在最终入口再挡一次；存储后的权威与记录错配 |
| 共享校验体（`BeforeTables`/`Standard`/`Strict` 都跑；`kmain` 入口再跑一次） | `memmap` 与 `reserved_regions` 不相交 | 保留区被当成空闲页发出 |
| 共享校验体（三档都跑；接收侧 strict 再复核一次） | 内核镜像与各模块、模块与模块按半开区间互不重叠（相切放行；跨度加法 `checked_add`） | 跨二进制载荷把模块放到内核或另一个模块身上（信任边界上的第二道复核） |
| `validate_requiring_sources`（生产腿 strict；三条 v1 接收腿与两个直核面都调用） | 按 `KernelInfo.authority` 查 `source_records` 覆盖投影内存图；内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内；按共享 `MODULE_NAMES` 的位置核对 `boot_modules[i].name` | 生产者漏写 provenance、声明了没有记录的权威、伪造了模块名，或内核跨度没有任何内存图记录解释 |
| 规划期（不是门） | `plan_kernel_mapping` 的首选/回退选择 | 镜像跨距不满足回退尺寸对齐 → 规划阶段给出可读错误 |

**"去掉 `bootstrap_len == 0` 断言"在当前代码里已无从谈起**：字段与断言已随字段删除（论证见 §3.4），这条反向推演保留的是它曾对应的真实险情。字段删除后，“提供一个可误设的回收区间”这个接口面整体消失，险情在类型上不可复发（完整故障链见 §3.4）。
### 4.5 `store_boot_report`/`store_kernel_info` 的六个落地区

`arch_boot_finish` 在前置门之后把发送侧 `BootReport<'_>` 深拷进内核自己的 `.bss`（`lib.rs` 的 `store_boot_report` → `land_and_store`），然后只使用这份副本；v1 接收面的 `resume_additive` 仍走 `store_kernel_info` 包装。

```rust
// 节选（保留泛型落地入口与两个 wrapper 的签名；一次性守卫在 land_and_store 入口，本引文省略；六个切片的 land_slice 调用与
// 模块名/param 两块的细节省略），os/kernel/src/lib.rs:fn land_and_store +
// store_kernel_info/store_boot_report。
fn land_and_store<V: minix_boot::KernelInfoView>(
    view: &V,
    authority: minix_boot::RamAuthority,
) -> &'static KernelInfo {
    // …六个切片依次 land_slice 到 .bss store…
    let mut k = KernelInfo {
        memmap: &[],
        reserved_regions: &[],
        boot_modules: &[],
        platform_sources: &[],
        param_buf: &[],
        source_records: &[],
        kern_virt_base: view.kern_virt_base(),
        kern_phys_base: view.kern_phys_base(),
        kern_size: view.kern_size(),
        authority,
    };
    k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
    // …其余五个字段同样回填为 .bss 指针…
    *KERNEL_INFO.get() = Some(k);
    crate::kernel_info().expect("store_kernel_info: KERNEL_INFO just written")
}

pub fn store_kernel_info(kernel_info: &KernelInfo) -> &'static KernelInfo {
    land_and_store(kernel_info, kernel_info.authority)
}

pub(crate) fn store_boot_report(
    report: &minix_boot::BootReport<'_>,
    authority: minix_boot::RamAuthority,
) -> &'static KernelInfo {
    land_and_store(report, authority)
}
```

**表名：《落地区与交接容量》**。六个落地区、一个名字池、一行 `KernelInfo.memmap` 切片视图与两个 `VmBootHandoff` 交接字段的容量与溢出行为；分类器侧的 wire 容量是 `WIRE_MEMMAP_CAP = 128`、`WIRE_RESERVED_CAP = 256`，分类期超过即 fail-closed，因此进入本表落地区之前已经筛过一轮。**这张表是全文交接容量的唯一权威**；§2.4.2.4 与 §3.2 引用本表并可在行内重复数字以服务上下文，但不另立权威口径：

| 落地区 / 交接字段 | 容量常量 | 值 | 溢出行为 |
|---|---|---|---|
| `RESERVED_REGION_STORE` | `RESERVED_REGION_STORE_LEN` | 256 | panic |
| `MEMMAP_REGION_STORE` | `MEMMAP_REGION_STORE_LEN` | 128（C `MAXMEMMAP=40` 提额；常量定居 globals.rs） | panic |
| `BOOT_MODULE_STORE` | `minix_boot::NR_BOOT_MODULES` | 12 | panic |
| `PLATFORM_SOURCE_STORE` | `PLATFORM_SOURCE_STORE_LEN` | 4（真实上限 2 × 有意余量） | panic |
| `PARAM_BUF_STORE` | `PARAM_BUF_STORE_LEN` | 1024（C 的 `MULTIBOOT_PARAM_BUF_SIZE`） | panic（先清零再复制） |
| `SOURCE_RECORD_STORE` | `SOURCE_RECORD_STORE_LEN` | 296 条记录（`WIRE_SOURCES_CAP`；wire 分区容量 21312 B，每条 72 B）。发送侧 `BootStorage` 的 source 槽是 40（DTB 收集器输入上限，超过 fail-closed），wire/接收侧 stores 是 296 | panic |
| `STARTUP_RECORD_STORE`（**当前不存在**） | 无 | 不适用 | 删除记录见 §3.2.1 |
| `BOOT_MODULE_NAME_STORE`（名字字节池） | `BOOT_MODULE_NAME_LEN` | 16（每槽；`store_kernel_info` 复制时截到 15 字节，v1 接收路径按 16 字节原样落） | 截断（不 panic；两条路径的截断点不同） |
| `KernelInfo.memmap`（交接字段） | 无独立容量（`&'static [MemoryRegion]`，长度自带） | 长度 = `MEMMAP_REGION_STORE` 已提交条数 | 不适用（切片） |
| `VmBootHandoff.free_regions` | `VM_BOOT_HANDOFF_MAX_REGIONS` | 64 | 交接前容量断言 panic（fail-closed；计的是 `classify` 扣除后的自由区间数） |
| `VmBootHandoff.deducted` | `VM_BOOT_HANDOFF_MAX_DEDUCTED` | 47（3 条固定扣除 + `MAX_BOOT_REGIONS` 32 + `NR_BOOT_MODULES` 12；`vm_handoff.rs` 有编译期断言） | 交接前容量断言 panic（fail-closed） |

在全部落地区（六个加名字池）里，名字池是唯一允许截断的（`param_buf` 的超长在规范化阶段已按整条记录丢弃，不会超容量落到这里）：`store_kernel_info` 路径取 `n = bytes.len().min(BOOT_MODULE_NAME_LEN - 1)`（16 - 1 = 15），保留一个字节给 NUL 语义上的余量；v1 接收路径由 `arch_boot_resume_v1` 直接写池，`name_len` 取到 16、不做 `min(15)`。截断后用 `core::str::from_utf8` 造 `&'static str`；若 15 字节截断切开了多字节序列，取 `valid_up_to()` 的最长合法前缀，而不是用 `from_utf8_unchecked` 把非法字节当成 `str`。生产名字全是 ASCII，但函数是公开的，不能把安全性寄托在调用者的内容上。测试内核路径用 write-once 静态表（`AtomicBool` 单写者 + 初始化后只读，无共享可变静态/别名面），并收进 `qemu-test-kernels` feature（生产不链接）；完整 `BootStorage` 迁移见 §3.2。

`land_slice` 里的 `if src.as_ptr() != dst.as_ptr()` 不是防御性假设：生产路径有两个 store 调用点——`arch_boot_finish` 的 `store_boot_report` 传发送侧 `BootReport`（真正拷贝一次）；`resume_additive` 的 `store_kernel_info` 源已是前一步解码落进 store 的切片（同址，跳过）。`kmain` 不再调用 `store_kernel_info`：它通过 `kernel_info()` 读同一 `.bss` 槽；`store_kernel_info` 顶部的一次性守卫会让第二次调用直接 panic，不是同址 no-op。守卫是 check-then-write，不是原子 CAS；它依赖启动路径在 BKL/SMP 之前的单线程前提，源码的 SAFETY 注释写明了这一点。`kernel_info()` 不是第二通道：`GET_KINFO`/`GET_IMAGE`、`phys_ram_view`、`harden_kernel_text` 与 `kmain` 都还通过它读取同一 `.bss` 槽；两条取用路径指向同一存储，区别只在“刚存完的这次调用拿返回值、其余调用拿全局访问器”。`ptr::copy` 是为**部分重叠**保留（`copy_nonoverlapping` 对部分重叠是 UB）；精确同址由 `land_slice` 的指针相等检查跳过，不会调用到拷贝。

容量分两组说：下面三个容量按“有独立设计依据”列，后几个按“直接对齐既有契约”列（除六个现役落地区外，另有一行 `STARTUP_RECORD_STORE` 删除记录）。三个容量各有来由：`RESERVED_REGION_STORE_LEN = 256` 是真机记录（`[观测]`）的 127 条 reserved 翻倍后向上取整到 2 的幂（独立容量；与 VM 侧 `free_regions` 64 / `deducted` 47 不是同一常量——那两个是本表下方的交接字段行，分属"内核落地区"与"VM 交接"两层）；`MEMMAP_REGION_STORE_LEN = 128` 直接定居 globals.rs——C 形状账本模块（`memmap.rs` 的 add/cut + 空洞 + 高水位）删除后，交接扣减由 [`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 的 `FreeRangeSet`（有序不相交页对齐集合）承担（扣减换手后），行为契约由"与参考区间减法逐页等价"的对拍测试锁住（读数与归因限定见 §3.2 的"不做深拷…会怎样"，实现侧增量见 §4.7 的 `cut_memmap` 行）；`BOOT_MODULE_NAME_LEN = 16` 与 `vm_handoff::copy_name` 的 16 字节 `proc_name` 字段同宽，落地不会比既有长度契约更紧。

另外几个容量定案：`PLATFORM_SOURCE_STORE_LEN = 4` 是"一个 DTB/ACPI 源加一个回退"的真实上限再乘二；`PARAM_BUF_STORE_LEN = 1024` 直接对齐 C 的定长语义，复制前先清零，短缓冲留下的尾部因此是全零而不是上一轮内容；`SOURCE_RECORD_STORE_LEN` 直接取 wire 分区容量（296），与解码端同一口径；v10（设计裁决）删掉了 startup 落地区（原容量 20 的规范集合不再存在），所以本表的 startup 行只作删除记录，不再有现役第七行。`BOOT_MODULE_STORE = 12` 的来源是 `MODULE_NAMES` 的长度（见 §4.3），它不是独立容量。

**只做一半深拷会怎样**：`reserved_regions` 深拷进去，但 `memmap` 与 `boot_modules` 保持原指针。后果是 free list 退化成空记录，第一次物理页分配失败时，现场离"三个切片里有两个没落地"隔着整个分类阶段。机制与读数见 §3.2.4，证据判别结论见 §5.3；本节不重复。
### 4.6 arena 与分配器换手的落地

三处写者与各自的自守卫在 §3.4.2 已列出。这里补上"游标"的两种恢复方式与扣除表的读法。先立三个名字：arena 是三条 v1 腿交接用的八页物理区；`BootAlloc` 是接收镜像的页表页游标；`VmBootAllocator` 是 VM 启动期的取页游标。三者分属“内核落地区”“wire”“VM 交接”三层，容量表在下方。

同一份 `BootAlloc` 有两种进入方式：boot-shim 侧的 `init` 与跨镜像接收侧的 `resume`（内核侧原来还有一条"从 memmap 切兜底区间"的 `init` 路径，已不存在）。两种方式的代码与语义：

```rust
// 代码引用（`init` 函数体完整；后续 `resume` 的文档注释与省略段见源文件），os/kernel/src/boot_alloc.rs。
    pub fn init(&self, base: u64, end: u64) {
        assert!(
            BumpRegion::resumed(base, base, end).is_consistent(),
            "BootAlloc::init: inconsistent region base={base:#x} end={end:#x}"
        );
        self.base.store(base, Ordering::Release);
        self.next.store(base, Ordering::Release);
        self.end.store(end, Ordering::Release);
    }

    /// Resume a bump allocator whose region was already partly consumed by a
    /// previous stage (the cross-image handoff).
    ///
    /// The three values arrive from the cross-image wire payload, so they are
    /// validated here rather than trusted: `base < end && base <= next <= end`
    /// must hold, otherwise `alloc` would hand out pages outside the declared
    /// region (and `region` would report an inverted range to DM coverage).
    pub fn resume(&self, base: u64, next: u64, end: u64) {
        assert!(
            BumpRegion::resumed(base, next, end).is_consistent(),
            "BootAlloc::resume: inconsistent region base={base:#x} next={next:#x} end={end:#x}"
        );
        self.base.store(base, Ordering::Release);
        self.next.store(next, Ordering::Release);
        self.end.store(end, Ordering::Release);
    }
```

- `init(base, end)` 把游标置回 `base`：跨镜像腿由 boot-shim 在进入内核之前设定区间（UEFI 两条路径都在 `prepare_and_enter_kernel_image` 内；U-Boot shim 在 `prepare_boot` 返回后）；两个直核面经共用的 `run_self_boot` **只初始化 bump 区间**，注册发生在随后调用的 `boot_validate_and_prepare`（`is_registered()` 守卫）。`minix-kernel` 自身不再调用 `init`：Step 0 看到 `boot_alloc_region()` 为 `None` 时直接停机。
- `resume(base, next, end)` 从"已被消耗到的位置"继续：跨镜像路径必须用这一种，因为接收侧链接的是这份分配器的另一份副本（`resume` 的文档注释写明了这一点）。跨镜像接收侧的 `arch_boot_resume_v1`（三架构同一入口）在解码信封后、进入本架构委托壳之前调用它；委托壳的分派见 02 篇的 4.5 节。

两种方式的次序约束：跨镜像路径上必须先 `resume_boot_pt_alloc`、再进各架构委托壳——三架构都经 `resume_after_v1_decode`（三个同名 cfg 壳最终都转 `os/kernel/src/lib.rs:fn resume_additive`），其中 aarch64 的壳再转 `arch_boot_resume_high_half`，x86_64 与 riscv64 的壳直接转 `resume_additive`。顺序反了的话，Step 0 会看到 `boot_alloc_region()` 为 `None`——兜底已删，后果是 fail-closed 停机，而不是把游标重置回某段 memmap 的起点。

扣除表的读法：`build_vm_handoff` 把"内核保留"与"引导已用"两类区间收进同一份记录（见 §3.4.2 引用的表），其中 bump 区是**整段**（UEFI 腿这条是空操作，因为 bump 区不在 conventional；自引导/U-Boot 腿才实际扣）；`VmBootAllocator` 的已用后缀（该分配器从每个区间末端向下消耗，已用部分是连续尾段——表里第二类）是同段内的另一笔。两份记录随交接页一起交给 VM，VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`——也就是说，"内核记账对不对"这件事不是靠 kernel 单方面声明，而是在 VM 侧再复核一次。

**两个 bump 实例共享一套算法**。U-Boot shim 的固定池从 `MODULE_REGION_BASE` 起按"根页 → 1024 页页表子区间 → 模块页 → arena"的顺序发放；内核镜像的 `BootAlloc` 只续用其中的页表子区间。两个实例**不共享游标**——合并曾被试过（`[观测]`）：shim 的游标跨过子区间末尾（模块与 arena 在子区间之外），`BootAlloc::resume` 直接看到 `next > end`，QEMU U-Boot 门在 3/6 个路标处 panic，合并方案已回滚。共享的是纯值核心 `minix_boot::BumpRegion { start, next, end }`：`alloc_pages` 的边界检查、`used_bytes` 与一致性谓词都只有这一份。编译期断言固定常量：池基址与大小按 4 KiB 对齐、`KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`、根页 + 页表子区间 + `ARENA_CAP` 不超过池大小、`BUMP_END <= boot_dm_admissible_end()`；运行期断言实例区间：`prepare_boot` 分配后检查根页就是池首页、页表子区间紧接其后且不越池尾，`BootAlloc::resume`/`region` 用 `BumpRegion::is_consistent` 校验跨镜像三元组。编译期只断言常量，运行时输入的合法性不假装编译期已证（断言清单按源码核对，文档不逐条摘录）。

**arena 生命周期合同**。`arch_boot_resume_v1` 的解码是 arena 的最后一次读取：六个载荷全部复制进内核 `.bss` store，`KernelInfo` 的切片指向这些 store，`arena_pa` 不保留。VM 自由页清单在 `kmain → build_vm_handoff` 才发布，严格晚于解码，于是 `LastRead(arena) ≺ Publish(VMFreePages)`——arena 页可以按生命周期回收，不做永久扣除。这条合同只对 arena 会落进 conventional 的腿有意义（U-Boot shim 与合成测试）；两条 UEFI 腿的 arena 是 `LOADER_DATA`→`ReservedMapped`，从不在 free list 里，回收与否对 VM 无影响。startup 分区的删除账见 §3.2.1；这里只需要结论：内核不再保留 startup-object 记录，也不再有它指向的 arena 身份。宿主测试 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 用真实序列化 arena 解码后整块 poison（写入哨兵值，模拟 arena 被复用），再跑与 VM 相同的 `classify_regions`：自由清单与解码值一致，落在 conventional 区间里的 arena 页被重新发给 VM，内核镜像仍被扣除。若未来有人在跳转后读 arena，必须同时移动解码边界或把 arena 加进扣除表，否则 VM 可能发到一份活的 wire 镜像。

**去掉内核侧 `is_registered()` 守卫会怎样**：让 `boot_validate_and_prepare` 无条件 `register(boot_alloc::boot_pt_alloc)`。`register` 用 `PT_ALLOC.compare_exchange(0, alloc_fn, AcqRel, Acquire)` 抢唯一写权；失败即 `panic!("pt_alloc::register called twice")`，在**两种构建档**都生效，不存在“release 下静默保留第一个注册者”的分支。因此第二次注册不会改写函数指针，也不会让 boot 继续：若第一个注册者是某个测试内核（它用了一个避开内核镜像的池），后续注册会在启动现场停机，而不是悄悄沿用测试池。`is_registered()` 守卫因此是所有构建档的硬前置条件，不再只是给测试内核让路的便利。
### 4.7 与 Minix3 C 的函数落点对照

> 本节按"函数→落点"排，C 侧符号的速查索引在 §7.5；Rust 侧落点以 §3/§4 的定义节为准。三者冲突时统一按：**源码 > 定义节（§3/§4） > 速查表（§7.5）**。

| C 函数（第 2 章） | Rust 落点 | 保留 / 改型 / 淘汰 / 新增 | 差异原因一句话 |
|---|---|---|---|
| [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) | [`fn main`](<os/boot-shim/src/main.rs:fn main>) | 改型 | 引导器跳转到内核入口 → 固件跳转到 boot-shim 入口 |
| `pre_init(magic, ebx)` | `prepare_and_enter_kernel_image`（UEFI 两条路径）与 `opensbi_helpers::prepare_boot`（U-Boot 路径） | 改型 | `pre_init` 的八条语句（02 篇的 2.4 节 计数）被拆成固件入口与共享 builder |
| `get_parameters(ebx, cbi)` | `prepare_and_enter_kernel_image` + `ebs_pair::pair_and_classify` + `arch_boot` 的发送侧 `BootReport`（仅直核面） | 改型 | 拷一份引导器结构体 → 调固件接口拿两份内存列表 |
| `mb_set_param(...)` | [`normalize_param_buf`](<os/libs/minix-boot/src/boot_params.rs:fn normalize_param_buf>) + [`param_buf`](os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo) | 改型 | 统一分词/拒绝/整条丢弃规则（十条可观察分歧见 §3.2），ARCH 生成、BOARD 有来源才生成；UEFI 与 DTB 两源共用 |
| `add_memmap(cbi, addr, len)` | `ebs_pair::pair_and_classify` / `build_memmaps`（构建侧）；内核侧扣减为 [`FreeRangeSet`](os/kernel/src/vm_handoff.rs) 的区间集合（扣减换手后，C 形状账本模块删除） | 改型 | 构建侧改为配对循环分类；内核侧按有序不相交区间集合记账，行为由对拍测试锁等价 |
| `cut_memmap(cbi, start, end)` | [`FreeRangeSet::cut`](os/kernel/src/vm_handoff.rs)（`classify` 扣减步调用） | 改型 | 引导侧报告它认为可分配的 DRAM（UEFI 腿已把内核跨度预订为 `LOADER_DATA`，自举/U-Boot 腿仍在表内）；切除/扣除在内核侧按腿生效。C 的第三个参数是"结束地址"，Rust 集合的 `cut` 收 `PhysRange`（半开区间 `[start, end)`）——C 里 add/cut 参数形不一致（§2.4 那处 `alloc_lowest` 传错参数就出在这个差异上），Rust 用区间类型把端点和长度分开 |
| `alloc_lowest(cbi, len)` | 当前无对应物；低地址代码的拷入端已有（[`fn install_at`](<os/arch/src/x86_64/ap_early_entry.rs:fn install_at>)，地址由调用者给） | 暂未移植 | 唯一的消费者是 32 位 AP 启动跳板（SMP 主题；当前可核对 `install_at` 的注释与 `smp_init` 计划）；`install_at` 的注释写明扫描空闲表找最低区间的动作归 SMP 篇的 `smp_init`，**重做时不得复刻 §2.4 那处"把长度当结束地址"的误用**（这条规则依赖的四个前提与各自的复查判据见 §2.4.2 的前提清单） |
| `overlaps(mod, n, cmp_mod)` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（内核段间）、C UEFI 腿 `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核；U-Boot 腿用编译期上界 + root/bump 复查）、D `KernelInfo::validate_requiring_sources`（模块 × 模块 + 内核 × 模块） | 改型 | `overlaps` 的两类检查落到 C/D；A/B 是 ELF 装载新增的两道防线（§4.3） |
| `pg_clear()` | [`X86_64Paging::new_from_page`](os/arch/src/x86_64/paging.rs:fn new_from_page) 的清零 | 改型 | 根页由调用者提供 |
| `pg_identity(&kinfo)` | [`build_bootstrap_root_and_enable`](os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable) Step 1 | 改型 | 范围由调用方窗口表决定；Step 1 固定 2 MiB piece，Step 2 用 `kern_huge` |
| `pg_mapkernel()` | 同函数 Step 2 | 改型 | 返回值不再被使用 |
| `pg_load()` | [`X86_64Paging::enable`](os/arch/src/x86_64/paging.rs:fn enable) 的 `mov cr3` | 保留（同一动作） | 硬件动作不变 |
| `vm_enable_paging()` | 同函数 `enable` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 改型 | 多了 NXE；少了 `PSE` |
| `pg_alloc_page(cbi)` | 无同名对应物：4KB 被映射数据页的运行期分配归 VM `phys_mem`；引导期页表页见 `alloc_pt_page` | 职责转移 | 从记录尾部取页 → 运行期由 VM 物理分配器承担 |
| `alloc_pagetable(&ph)` | [`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>) + 三架构实现 | 改型 | 6 页静态池 → 注册式分配器 |
| `pg_map(phys, vaddr, vaddr_end, cbi)` | [`X86_64Paging::map`](os/arch/src/x86_64/paging.rs:fn map)（4KB 粒度） | 保留（语义） | 4KB 细粒度映射的用途不变 |

这张表里 `alloc_lowest` 一行标的是"暂未移植"，意思是**当前 64 位路径不需要按原样移植**：`alloc_lowest` 的服务对象（把 16 位启动代码放到 1MB 以下）属于 AP 启动，而 64 位侧的对应机制（AP 停车邮箱 + 启动记录）在 [`smp.rs`](os/kernel/src/smp.rs) 与 [`main.rs`](os/kernel-image/src/main.rs) 的 `park` 段里，不经过"从记录最低处分配"这条路径。若 SMP 篇的低地址分配需求最终出现，它应以新机制重新实现，而不是把 C 的 `alloc_lowest` 原样搬过来；这条未闭环项归 SMP 篇，不是当前的缺陷。

静态页表池为什么不能原样搬到 64 位，见 §3.4 的分配器换手推演；本节只保留 `alloc_pagetable` 的对照行，不重复那段推导。

### 4.8 交接后的 Direct Map（概念入口）

**Direct Map 是内核访问物理内存的一种映射方式**：内核为一定的物理地址范围建立固定的虚拟地址窗口，使内核能够通过该窗口访问对应的物理页。

Direct Map 覆盖建立发生在启动信息存储之后、进入 `kmain` 之前，但两组腿的链不同：直核面是 `store_boot_report` → `arch_boot_impl`（`build_bootstrap_root_and_enable` → `establish_boot_dm`）→ `jump_to_kmain`；v1 接收面在 `resume_additive` 内是 `boot_validate_and_prepare` → `store_kernel_info` → `establish_boot_dm` → `jump_to_kmain`，**不调用** `arch_boot_impl`/`build_bootstrap_root_and_enable`（活动根只做加法，重建会清零活动根）。`establish_boot_dm` 在已经活动的 bootstrap 页表根上追加映射，不重新创建或替换根页表。

**能够通过 Direct Map 访问，不代表该物理页可以分配给 VM。** VM 的可分配集合还要考虑固件内存分类、Direct Map 的覆盖范围，以及启动期间已经占用的物理页。未被 Direct Map 覆盖的候选内存不能进入 VM 自由表，否则 VM 分配出的页面可能无法被内核正常访问。

Direct Map 的完整布局、覆盖策略及其在物理内存管理中的作用，属于独立的内存管理主题；本节只介绍它在启动交接中的位置与作用。

---

#### 失败对照

如果把 `boot_validate_and_prepare` 内的 `pt_alloc::register` 挪到进入内核之后，Step 1/2 第一次 `map_huge` → `alloc_pt_page` 会断言 `pt_alloc::alloc_pt_page called before register`；`init_boot_pt_alloc` 只初始化 bump 区间，不写注册槽。`boot_alloc_region()` 为 `None` 时 `boot_validate_and_prepare` 直接停机。所以注册是进入内核的前置条件，不能从内存图推断。

#### 最小示例

```bash
# 在源码里定位本章的 Step 路标与接收端首行（§6.1 的复算命令）
$ grep -n 'boot_stage!\|### minix-rs kernel-image' os/kernel/src/lib.rs os/kernel-image/src/main.rs os/kernel-image/src/bootface*.rs
```

命中的 `boot_stage!` 是 Step 1/2 与 Step 4 的打印点；`### minix-rs kernel-image` 是接收端进入 Rust 面的第一行（§6.1 的路标表）。

#### 小结

- 入口骨架只有一句：`main` 把页表页预算交给 `prepare_and_enter_kernel_image`，后者不返回。
- 九个动作按“准备 → 退出固件服务 → 交接”三段排列；读文件在 EBS 之前，跳转在 EBS 之后。
- 两条 UEFI 链共用发送侧；接收端按架构从 `rbx`/`x0`/`a0` 取 arena 基址后汇入 `resume_additive`。
- 五条腿的发送侧都只构造 `BootReport<'_>`；两个直核腿经 `store_boot_report`、三条 v1 腿经 `arch_boot_resume_v1` 解码进 `.bss`，随后由 `store_kernel_info` 包装调用（同址跳过复制），两条路径都生成唯一的 `KernelInfo`（切片字段为 `&'static [T]`）。
- 六个落地区把切片深拷进内核 `.bss`，此后只使用这份副本。

#### 术语

- **九个动作**：`prepare_and_enter_kernel_image` 内部按序执行的九步，产出归属见 §4.2 的表。
- **落地区**：内核 `.bss` 里的六个定长存储区；直核腿由 `store_boot_report` 深拷，v1 腿由 `arch_boot_resume_v1` 解码写入，`store_kernel_info` 首次同址跳过复制，但带一次性守卫，第二次调用 panic。
- **门链**：`validate_requiring_sources`、`validate_before_tables`、`check_boot_postconditions`、`boot_validate_and_prepare`、`KernelInfo::validate_requiring_sources` 组成的校验次序。
- **A+D**：先读 ELF 预订 LMA 跨度、再分配 root/bump、最后拷贝段的动作组合。
## 5. 测试：四条不变量各自的证据

### 5.0 本章导读

#### 问题入口

四条不变量听起来都能用“测试通过”背书，但不同测试证明的东西并不一样：宿主测试证明形状与负控，真机门证明某条链能走到终点，二者都不能证明任意畸形输入会被拒绝。本章要解决的问题是把 I1–I4 拆成可核对的证据矩阵：每条不变量有哪些宿主证据、哪些真机证据、各自不证明什么，以及一条失败链应该用哪个路标定位。

#### 前置概念

- 第 1 章的 I1–I4 与五条生产腿。
- 第 2 章 §2.6 的模块装载三层检查。
- 第 3 章的 wire、`BootStorage` 所有权与 `SourceRecord`。
- 第 4 章的 `store_kernel_info` 六个落地区与门链。

### 5.1 四条不变量的证据矩阵

四条不变量各有自己的证据层，不能互相借用。门覆盖不等于成功路径覆盖；逐行读“不证明什么”列。

I3 的真机证据按覆盖深度写成三条陈述（第二条限定第一条中的一个成员）：**有启动门链的四条腿**是 x86 UEFI、aarch64 UEFI、riscv64 U-Boot shim 与 riscv64 `bootface`；其中 **aarch64 UEFI 的门链判据只到发送侧分类点**，未证明走到 `kmain`；**aarch64 直核不在 §5.4 的五条门链内**，只由 `test-smp-aps-aarch64` 驱动到 SMP 交付。另有一条边界：riscv64 `bootface` 的门只验证 fail-closed 错误路径，不证明走到 `kmain`（§5.4 的对应行）。

五条生产腿、§5.4 五条门链与“四条腿”的成员不同，换算见 §3.6 节末表。

| 不变量 | 宿主证据 | 真机证据 | 不证明什么 |
|---|---|---|---|
| I1 值可跨镜像 | `handoff_v1` 的 encode/decode round-trip；`KernelInfo` 字段台账；`store_kernel_info` 的落地区测试 | `test-shim-bootmarks-x86.sh`（x86 v1 交接）、`test-riscv64-uboot-shim.sh` 6/6 | 不证明任意畸形 wire 都被拒绝；负控覆盖已知变体 |
| I2 来源可追溯 | `memmap_source` 的 `SourceRecord`/`audit_source_coverage` 正负控；DTB 收集器 40 槽上限 | riscv64/aarch64 直核的 DTB memmap 启动 | UEFI 腿的 source 由分类器直写 wire，宿主只测分类器 |
| I3 所有权明确 | `BootStorage` 容量/半提交/one-shot 测试；`BootReport` 生命周期编译检查 | 五条腿都有真机证据；其中四条有 §5.4 的启动门链，aarch64 直核由 `test-smp-aps-aarch64` 驱动到 SMP 交付；riscv64 `bootface` 与 aarch64 UEFI 的 DTB_CFGTABLE 形态只验证 fail-closed 错误路径，不证明走到 `kmain`；aarch64 直核只到 SMP 交付（部分覆盖）；走到 `kmain` 的证据覆盖 x86 UEFI 与 riscv64 U-Boot shim；aarch64 UEFI 的判据只到发送侧分类点，未证明 `kmain` | 不证明所有调用点都遵守 one-shot；真机门不覆盖所有失败形态 |
| I4 校验先于消费 | `KernelInfo::validate_*` 的 `should_panic` 负控；布局/跨度被测 | QEMU 各腿的启动门 | 不证明所有畸形布局；只覆盖接收侧已构造的负例；v1 发送侧的窗口消费（`prepare_kernel_image_v1`）不在本门链内，见 §4.2；接收侧顺序由 §4.4.1 的调用序与代码走查保证 |

### 5.2 宿主测试的边界

宿主测试的边界：`cargo test -p minix-boot --lib` 189/0、`-p boot-shim --lib` 136/0、`-p minix-kernel --features mock --lib` 866/0/8、`-p minix-elf --lib` 27/0。真机门：`test-shim-bootmarks-x86.sh` 与 `test-riscv64-uboot-shim.sh` 为发送/接收全链；`test-kernel-image-riscv64-bootface` 直核；aarch64 直核由 `test-smp-aps-aarch64` 到 boot/SMP 标记（其后的用户链 Data Abort 是已登记的 A2 遗留，不作为本表的通过证据）。

### 5.3 机制 → 需要证明什么

下表与 §5.1 使用同一批宿主/真机证据，只按机制重排；它不是第二份独立证据，读证据去 §5.1 的对应行。

**证据强度分档（本节一次性声明，后文只标标签）**：`[结构保证]` 指由类型、常量、编译期断言或代码结构直接保证、不依赖运行环境的结论；`[观测]` 指来自真机记录或仓内探针的读数，读数与锚点随主张给出，但不构成对所有环境的保证（print-only 或未归档的探针同样按此档）；`[预防性不变量]` 指为防已知风险而设、机制尚未证实的约束。

| 机制 | 需要证明 | 证据 | 不证明什么 |
|---|---|---|---|
| 深拷不变量（I1/I3） | 接收侧消费前，载荷已在私有存储 | 见 §5.1 的 I1、I3 行（同一批证据） | 不证明任意畸形 wire；负控只覆盖已知变体 |
| 来源覆盖（I2） | 每个投影字节有来源记录 | 见 §5.1 的 I2 行（同一批证据） | UEFI 腿 source 由分类器直写 wire，宿主只测分类器 |
| 校验先于消费（I4） | 建表/分配之前完成校验 | 见 §5.1 的 I4 行（同一批证据） | 不证明所有畸形布局；不证明顺序（顺序由 §4.4.1 的调用序与代码走查保证） |

**零化读数的结论**（读数与两条候选见 §3.2.4）：候选①“页表不重放”与候选②“被当空闲复用清零”都**未排除**；因此“入口尽早深拷”按 `[预防性不变量]` 读，不是已证机制。真机门只证明五条链能走到各自的终点。

### 5.4 真机门链

`DTB_CFGTABLE=1` 指该固件配置把机器 DTB 发布到 UEFI 配置表；AAVMF 的 DTB 页不是 `EfiACPIReclaimMemory`，所以这是“存在但不可用”的拒绝分支，不是“无 DTB 配置”。默认配置没有可用的 DTB，判据看 `memmap source` 行。

| 链 | 脚本 | 判据（关键路标） |
|---|---|---|
| x86_64 UEFI | `test-shim-bootmarks-x86.sh` | `kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、`resuming high-half boot`、`entering scheduler` |
| aarch64 UEFI | `test-shim-bootmarks-aarch64.sh` | 前两条固定标记 + 第三条变体：默认无 DTB 配置看 `memmap source` 行；`DTB_CFGTABLE=1` 的强制 DT 配置看 `DTB preflight failed`（存在但不可用）行。两者互斥，均须前两条固定标记同时出现 |
| riscv64 U-Boot 直载测试链 | `test-riscv64-uboot.sh` | `### TEST_RESULT: PASS hello-boot-riscv64 ###`；宿主 U-Boot 不兼容时 SKIP（exit 2） |
| riscv64 U-Boot shim | `test-riscv64-uboot-shim.sh` | 6/6：`uboot leg enter`、`table built`、`kernel + 12 modules loaded`、`[raw] entering kernel image v1`、`resuming high-half … riscv64`、`entering scheduler` |
| riscv64 直核 bootface | `test-kernel-image-riscv64-bootface.sh` | 表址行、`table holds 0x` + 十六进制条目数（脚本按 `table holds 0x<条目数> entries` 匹配；归档示例是 0 entries）、`module placement failed` 的行序（故意让模块定位失败以验错误路径） |

aarch64 直核面不在上表：它由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动，判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门；`test-sysboot` 是 x86 用户态 IPC 交换测试，也不是本表的启动门。

### 5.5 测试缺口与关闭记录

本节列出的范围内，没有已登记但未关闭的测试缺口；覆盖度限制已在 §5.1 的“不证明什么”列明（含 aarch64 直核只到 SMP 交付），按范围声明不计为缺口。

aarch64 直核的用户链 Data Abort 是已登记的 A2 遗留：A2 指 aarch64 直核在 SMP 交付之后进入用户链时的 Data Abort，它是已登记的功能遗留，不是测试缺口，单列不计入本表。五条生产腿各有对应的真机证据；其中 aarch64 直核经 `test-smp-aps-aarch64` 到 SMP 交付，不属于 §5.4 的五条门链，按 §5.1 的“部分覆盖”读；不把 fail-closed 形态算成走到 `kmain`。

以下三项由现有证据覆盖：C 侧段归属由 `check-layout.sh` 的 L10（`Makefile.inc` 改名名单 ↔ `kernel.lds` 收段）覆盖；riscv64 载体的 PC 高半判据在 `kmain_verify` 的 PASS 条件内；生产 `kmain` 入口的 ABI 栈形由三份 `jump_to_kmain` 的静态断言覆盖。§6.1 的“step1+2 之后没有发送侧路标”是已知诊断边界，冻结的裁决明确不为此增加打印，由 QEMU `-d int` 覆盖；§6.3 的窗口断言关闭记录指 `BOOT_IDENTITY_MAP_END` 的 Step 1 断言。

---

#### 失败对照

如果把 `KernelInfo::validate` 的负控当成“所有畸形布局都会被拒绝”的证据，就会高估覆盖：§5.1 的“不证明什么”列写明它只覆盖已构造的负例，任意畸形 wire 仍需负控扩展。所以证据矩阵按不变量分行，每行同时列出证明范围与不证明范围。

#### 最小示例

```bash
$ cd os
# 宿主证据：交接库与内核 mock 形态（§5.2 的读数）
$ cargo test -p minix-boot --lib
$ cargo test -p minix-kernel --features mock --lib
```

预期分别全绿；§5.2 记录的读数是 189/0 与 866/0/8。

#### 小结

- 四条不变量各有证据层，不能互相借用。
- 宿主测试证明形状与负控；真机门证明五条链能走到各自终点。
- `store_kernel_info` 的落地区测试与 wire round-trip 是 I1/I3 的深拷证据。
- `audit_source_coverage` 的正负控与 DTB 收集器上限是 I2 的来源证据。
- 每条证据都要读它的“不证明什么”列。

#### 术语

- **宿主证据**：在宿主上跑 `cargo test` 得到的证据，覆盖形状、负控与纯逻辑。
- **真机门**：在 QEMU 或真板上跑脚本、按串口路标判定的门。
- **负控**：故意构造非法输入、要求代码拒绝或 panic 的测试。
- **证据矩阵**：按不变量分行、同时列出宿主/真机证据与“不证明什么”的表（§5.1）。
## 6. 诊断

### 6.0 本章导读

#### 问题入口

本章回答"引导失败了怎么查"：6.1 讲 EBS 前后两条输出通道与里程碑路标的读法，6.2 是从静默到可读的 panic 路径，6.3 列典型失败模式与对应信号，6.4 给可复制命令。这一章按用途查，不按执行顺序读。

#### 前置概念

- 第 1 章的 I1–I4 与五条生产腿。
- 第 3 章的 v1 腿与直核面之分。
- 第 4 章的入口链与 Step 0–4 编号。
- 第 5 章 §5.4 的五个真机门脚本与 §5.5 的已知诊断边界。

### 6.1 EBS 分界、两种裸串口写法与里程碑清单

在 UEFI 路径上，两条通道按 `ExitBootServices` 分界：EBS 之前是固件控制台（`uefi::println!`，经 ConOut——固件的控制台输出协议）；EBS 之后是裸串口（`raw_serial`/`raw_serial_line`）。U-Boot shim 与两个直核面没有 EBS 这一步，只有裸串口。因此里程碑输出分两段。下文的 `raw_serial` 与 `raw_serial_line` 是同一条裸串口通道内的两种写法，不是两条通道。

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

`raw_serial` 与 `raw_serial_line` 的差别：后者在写之前等发送侧腾空（x86_64 读 LSR bit5，有界轮询）并在 `\n` 前补 `\r`；前者在 x86_64 上不等、也不补——aarch64 上这两个函数共用同一个带节流的发射器（源码注释明写本架构上"等腾空"与"不等"没有区别），差别只剩 `\n` 前补不补 `\r`。前者用于"已经知道自己在写什么、只想尽快留下痕迹"的场合（例如 EBS 之后的第一行），后者用于 panic 路径。

里程碑清单（每条给源码位置与**适用腿**；这些字符串是排查时的定位依据。清单里没有独立的 step3 路标——开分页夹在 `step1+2` 与 `step4` 之间，源码未单独打点）：

| 输出 | 阶段 | 适用腿 | 位置 |
|---|---|---|---|
| `boot-shim: prepare_and_enter enter (handoff v1 arena)` | 进入 arena 交接准备 | UEFI 发送腿（x86_64/aarch64） | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| `boot-shim: kernel loaded (entry staged)` | 内核段已拷贝 | UEFI 发送腿 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| `boot-shim: 12 boot modules loaded` | 12 个模块已就位 | UEFI 发送腿 | `uefi_helpers.rs:prepare_and_enter_kernel_image` |
| `boot-shim: param_buf bytes=… rejected=… dropped=…` | 参数源读数 | UEFI 发送腿 + riscv64 U-Boot shim（`prepare_boot` 内同一字符串） | `prepare_and_enter_kernel_image` / `opensbi_helpers::prepare_boot` |
| `boot-shim: [raw] boot services exited (paired)` | 配对循环已退出（裸串口） | UEFI 发送腿 | `prepare_and_enter_kernel_image` |
| `boot-shim: memmap source: …` | 最终内存图来源行（最终快照与分类结果的一致性检查在 EBS 之后跑） | UEFI 发送腿 | 同上 |
| `boot-shim: memory map left N range(s) out of both ordinary tables` | EBS 成功后 `Side` 侧桶去向的唯一说明行 | UEFI 发送腿 | `ebs_pair.rs:219-221` |
| `boot-shim: [raw] arena=… conv=… res=…` | arena 基址与分类条目数 | UEFI 发送腿 | 同上 |
| `boot-shim: [raw] entering kernel image v1` | 跳转前最后一条 shim 路标（裸串口） | 三条 v1 发送腿（x86_64、aarch64、riscv64 U-Boot shim） | `uefi_helpers.rs`（两条 UEFI 腿）/ `uboot_main.rs`（U-Boot shim 腿） |
| `kernel: arch_boot entered` | 内核侧第一个路标；`kernel: preA rdi=…` 是另一条临时取证打印（源码注释自标 `temporary`，不参与门判据），位于 `kmain` 内，不在 `arch_boot` 返回路径上 | **仅 x86_64 测试内核（直核入口）**；x86_64 的 UEFI 生产接收侧走 `arch_boot_resume_v1`，不打印 | `os/kernel/src/lib.rs` 的 x86_64 `arch_boot` |
| `kernel: v0 enter` / `kernel: v1 …` / `kernel: v2 …` / `kernel: v3 …` / `kernel: v4 capability facts ok` | `boot_validate_and_prepare` **内部**的五个子步路标，依次打出（v1–v3 是中间阶段，独立定位价值低于首末两点；表中不逐行列出其文案）。v1 腿上门这组打印**两次**——发送侧 `prepare_kernel_image_v1` 跳转前一次、接收侧 arena 解码后（`resume_additive` 内）再一次，出现第二组才是接收端已开始跑；第二组之前必有 `### minix-rs kernel-image: resuming high-half boot…`，日志不完整时用该行作发送/接收分界。直核面在进入 `arch_boot` 之后也走到这里 | 两个直核面 + 三条 v1 发送腿 + v1 接收腿 | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| `### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)`（x86_64 带 `, x86_64`；aarch64 无后缀；riscv64 带 `, riscv64`） | 接收端已进入 Rust 面 | v1 接收腿 | `rust_image_main`（`os/kernel-image/src/main.rs`） |
| `kernel-image: step4 DM coverage (additive on active root)` / `kernel-image: DM ok, jumping to kmain (high half)` | 接收端 additive 阶段路标 | 仅 aarch64 v1 接收腿；x86_64/riscv64 的 `resume_after_v1_decode` 给 `resume_additive` 传空串，不打印这两行 | [`arch_boot_resume_high_half`](os/kernel/src/lib.rs:fn arch_boot_resume_high_half) |
| `kernel: step0 validate ok` | `boot_validate_and_prepare` 的**完成**标记（内部子步是 `kernel: v0`–`v4` 那一行） | **仅直核面**；v1 发送腿只打子步、不打完成标记，v1 接收腿也不打 | [`lib.rs`](os/kernel/src/lib.rs) 的 `arch_boot_impl` |
| `kernel: step1+2 mappings ok` | Step 1/2 已过（建根后半、**跳转前**） | 两个直核面 + 三条 v1 发送腿（`build_bootstrap_root_and_enable` 内） | [`lib.rs`](os/kernel/src/lib.rs) 的 `boot_stage!` |
| `kernel: step4 DM coverage ok` | 直核建表完成 | **仅直核面**（`arch_boot_impl`）；aarch64 v1 接收腿另打 `kernel-image: step4 …` 那一行（x86_64/riscv64 接收腿传空串，不打印） | [`lib.rs`](os/kernel/src/lib.rs) 的 `boot_stage!` |
| `kernel: step2c device window empty (skip)` | 设备窗口为空时的跳过标记 | 仅 aarch64（直核与 v1 发送侧）；x86/riscv 的 `device_window_regions()` 非空，不打印 | `lib.rs` 的 `boot_stage!` |
| `kernel: step4 DM coverage (preflight + install)` | **直核路径**（`arch_boot_impl`）DM 覆盖的两段（preflight 与 install） | 仅直核面；aarch64 接收侧打印 `kernel-image: step4 DM coverage (additive on active root)`，x86_64/riscv64 接收侧不打印 | 同上 |
| `  param_buf bytes=… first=…`（两个前导空格，无 `boot-shim:` 前缀） | 直核面参数源读数；`first` 是第一条记录 | 两个直核面 | 两个 bootface 的 `normalize_param_buf` 调用点 |

**阶段标签对照**：`v0`–`v4` 是 `boot_validate_and_prepare` 内部的子步路标；`step0 validate ok` 是同一个阶段在直核面上的**完成**标记，不是另一个阶段。三条 v1 发送腿只打子步、不打 `step0`；v1 接收腿经 `resume_additive` 调同一个 helper，也只打子步。因此日志里看到 `v4` 不等于看到 `step0`，没看到 `step0` 也不等于没走校验——先确认这条腿是否属于"仅直核面"那一列。同理，`step1+2 mappings ok` 由 `build_bootstrap_root_and_enable` 在**跳转前**打印；如果它之后没有 `###` 或第二组 `v0`，说明跳转后接收端没出声，不能据此判断已经在独立镜像里。

（`build_memmaps` 的诊断打印是 `boot-shim: memmaps conv=… reserved=… unmapped=…`；空表时打印完整串 `boot-shim: memmaps conv=0 reserved=0 unmapped=0 (empty map)`。这些是测试内核调用 `build_memmaps` 时的诊断读数（`build_memmap()` 是只取 conventional 表的单表包装），不进主路径里程碑。）

复算命令（按文件覆盖表内全部来源；串前缀扫描覆盖表内多数组，未命中的走别的宏或裸字符串，见各行位置列）：`grep -n 'uefi::println!\|raw_serial' os/boot-shim/src/main.rs os/boot-shim/src/uefi_helpers.rs`；`grep -n 'boot_stage!\|### minix-rs kernel-image' os/kernel/src/lib.rs os/kernel-image/src/main.rs os/kernel-image/src/bootface*.rs`。

**删掉 `[raw] entering kernel image v1` 这一行会怎样**：发送侧在调用 `enter_kernel_image_v1` 之前打印 `[raw] arena=…` 与这一行；建根与开分页发生在调用内部。删掉这一行后，串口停点按三档读：`[raw] arena=…` 表示还没进调用；`kernel: v4 capability facts ok` 表示已进调用但 Step 1/2 映射尚未完成（建根前半）；`kernel: step1+2 mappings ok` 表示 Step 1/2 已过（建根后半、**跳转前**）。

**`step1+2` 不是"已经在独立镜像里"的信号**：它由发送侧 shim 进程内的 `build_bootstrap_root_and_enable` 打印（不是 `[raw] entering` 那一行，后者由 `uefi_helpers.rs`/`uboot_main.rs` 在调用 `enter_kernel_image_v1` 之前打印），三条 v1 发送腿共用；要判断接收端是否已起，只能看它之后有没有 `###` 或第二组 `v0`。若串口最后一行是 `step1+2`，可能是“建根完成、还没执行到跳转”，也可能是“已经跳转、接收端没出声”——这一行本身区分不了这两种状态，`step1+2` 之后到 `jump_kernel_image_v1` 之间仍有代码。`kernel:` 前缀同样不代表已经在独立镜像里。

第二档与第三档之间没有专用发送侧路标；这是已知诊断边界，已裁决不为此增加打印：CR3 切换后 shim 侧不可观测属于 PE/CR3 架构特性（三次尝试实证），A→C 间隙由 QEMU `-d int` 异常追踪覆盖，门链全过为实证背书（诊断边界与决策见 §5.5 的已知诊断边界，已裁决不增打印）。

### 6.2 panic 路径：从静默到可读

```rust
// 节选（保留缓冲写入与输出段；注释有删节，英文注释逐字保留），os/boot-shim/src/main.rs:fn panic。
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
                } // Dropping the tail beyond 256 B is fine: truncated diagnostics beat silence.
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 256];
    let mut writer = StackWriter {
        buf: &mut buf,
        len: 0,
    };
    let _ = core::fmt::Write::write_fmt(&mut writer, format_args!("{}", info));
    let written = writer.len;
    let message =
        core::str::from_utf8(&buf[..written]).unwrap_or("boot-shim panic: (non-utf8 message)");
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

四条约束，逐条给理由：

- 用固定栈缓冲而不是 `format!`：堆在 EBS 之后不可用（UEFI 池分配器返回空）。`StackWriter` 只做逐字节搬运。
- 超出 256 字节丢弃尾部而不是截断格式化过程：注释写明"诊断信息截断好于静默"。
- 输出经 `raw_serial_line`（带 `\r` 补全与发送侧等待）：panic 消息只走裸串口，不碰任何可能已失效的固件通道。
- 最后尝试诊断钩子：诊断钩子静态量随 `minix-types` 编进**每个二进制各自的一份**——跨镜像交接意味着 boot-shim 与内核各有自己的 `.bss`，shim 侧的钩子恒未注册（§3.2 的"两个 `.bss`"论证同样适用于静态量）。于是两条腿分开说：**shim 侧**（本节引文的 handler），钩子检查必然落空、兜底信息是*无条件*行为，不携带"早于注册"的诊断含义；**内核侧**——直核家族在 `arch_boot` 入口注册钩子（`register_panic_diagnostic`），v1 接收侧在 `resume_additive` 的 `store_kernel_info` 之后调用同一函数；之后内核阶段的 panic 才会把同一条消息按内核格式再打一遍（带 CPU 号与回溯）。钩子未注册在直核路径才是“panic 早于注册”的真实信号；v1 接收侧同样有注册点，不能把“未注册”直接当成该腿的早期 panic 判据。

**在 panic handler 里调用 `format!` 或固件服务会怎样**：第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生——串口上得到的是"panic 处理自身的崩溃"或什么都没有，而原始 panic 的消息与位置被彻底覆盖。"panic 路径上不能分配、不能调固件服务、只能纯 CPU 操作加裸串口"这条约束就是从这里来的。

### 6.3 典型失败模式与诊断信号

（复现这些失败的可复制命令见 6.4；两条链路的互指：本节给"哪个信号说明什么"，6.4 给"怎么把信号抓下来"。）

表的前两行是 **C 原作的失败模式**（`boot.cfg` 指令次序、`head.S` 的 `push` 次序）——重写产品没有这两个触发点，列出只为对照 §2.1/§2.2 的断言行为，排查重写链时直接跳过；其余各行是 Rust 链上会遇到的，或（划线那行）已不可复发的历史条目。

| 失败模式 | 触发点 | 现场信号 | 下一步确认 |
|---|---|---|---|
| 引导器指令次序错（`load_mods` 在 `multiboot` 之后） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 的 `assert(mbi->mi_mods_count > 0)`（为什么不是查 `HAS_MODS` 的那条，见 §2.1） | 未开串口调试（默认）：屏幕刷出 `Minix panic. System diagnostics buffer:` 与 `press any key to reboot`、等一个按键才复位；命令行里写了 `cttyline=`（`do_serial_debug` 为真）时走**互斥**的另一条通路——panic 由串口同步输出后立即复位，不刷屏、也不等按键。这条断言排在命令行解析之后，所以 `cttyline=` 已经生效；对照下一行那条在解析之前，串口一定不出声 | 核对 `boot.cfg.default` 的 `;` 分隔次序 |
| 参数压栈次序错（x86-32） | [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `assert(magic == MULTIBOOT_INFO_MAGIC)` | 同样刷屏等按键，但串口一定不出声——这条断言在命令行解析之前，`do_serial_debug` 还是 `.bss` 的零初值；报出的表达式里带着 `magic` 这个名字，而它当时装的是地址（值本身不打印，见 §2.2 的说明） | 核对 `head.S` 两条 `push` 的次序 |
| 模块缺失 | `loader.rs:load_boot_modules_into` | `boot-shim: required boot module 'X' not found at <路径>` | 核对盘上 12 个模块名（脚本的 `mdir` 前置检查会先报） |
| 内核 ELF 读不到 | `FileLoader::read_required` | `boot-shim: required file not found: /EFI/minix/kernel.elf` | 核对 ESP 的 `/EFI/minix/kernel.elf` 与固件是否挂载了该分区 |
| bump 区间耗尽 | `BootAlloc::alloc` 返回 `AllocationFailed` | 内核侧 `map_huge`/`boot_pt_alloc` 失败，或 `enter_kernel_image_v1: bump region not registered` | 复算 `prepare_and_enter_kernel_image(N)` 的页数预算与镜像规模 |
| v1 信封/载荷无效 | `arch_boot_resume_v1` 的 `decode_*` | `kernel-image: handoff v1 envelope invalid: <变体>`（或 memmap/reserved/modules/platform/param/source 六个分区中任一段的对应行） | 核对 shim 的序列化顺序与描述符长度；先看 `[raw] arena=… conv=… res=…` 是否已打印 |
| 发送侧走完、接收端未起（无声卡死） | `[raw] entering kernel image v1` 之后、接收侧 `### minix-rs kernel-image: resuming high-half boot…` 之前 | 串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根的后半段：Step 1/2 映射已过、Step 2b/2c 也已完成，卡在 Step 3 `enable` 或跳转），更早则是 `kernel: v4 capability facts ok` 或 `[raw] arena=…`——三档读法见 §6.1 末段；接收侧第二组 `v0` 不出现 | 按 §6.1 末段三档先定位发送侧停点；再按 §6.4 第三层在 `enter_kernel_image_v1` 或接收侧委托链下断点 |
| ~~`bootstrap_len` 非零~~ | （字段与断言均已不存在，该失败模式不可复发） | 历史消息曾是 `bootstrap_len must be 0 (Rust port has no unpaged section), got N`（`[观测]`，历史记录）——现行代码 grep 不到它，排障不要按这条找 | — |
| boot-shim 自身镜像被装载在 4 GiB 之上 | 固件装载器（UEFI 无此保证：规范不约束 loader 镜像的物理落点） | boot identity 窗口的 VA 上限是 `BOOT_IDENTITY_MAP_END`；shim image 取自 `LoadedImage.info()`、stack 取自当前 SP（`shim_stack_window()`）并加入窗口表，Step 1 映射循环断言 `end <= BOOT_IDENTITY_MAP_END`（2 MiB 上取整后）——落在 4 GiB 之上会以可读消息 fail-closed，不再静默落到无映射区 | 已由窗口断言覆盖（见 §5.5 的关闭记录） |
| 快照位置错（自举页被报成可用） | `assert_bootstrap_outside_memmap` | `conventional memmap [a,b) overlaps bootstrap allocation [c,d)` | 核对快照调用点在三次分配（root/bump/arena）之后 |
| 大页阶梯无档可用 | 规划层 `plan_boot_pages`（两个调用点：直核 `arch_boot` 家族与 v1 发送侧 `enter_kernel_image_v1`——后者服务 x86_64、aarch64、riscv64 U-Boot shim 三条腿） | 两条消息前缀对应两族：`arch_boot: page-size policy: …`（直核）/ `enter_kernel_image_v1: page-size policy: …`（v1 发送三腿）；消息体按变体分两种：`NoKernelPageSize` 是 `no kernel page size satisfies the layout`，`FallbackUnsupported` 是 `fallback page size <N> is not supported`（后者表示回退档本身不被硬件支持） | 跑 `check-layout.sh` 的 L3/L4/L5 |
| 页表切换后取指故障 | `Paging::enable` 之后第一条指令 | QEMU `-d int` 日志里的异常记录；`CR2`/`EPC` 指向当前指令 | 核对恒等映射范围是否覆盖该地址（02 篇的 4.3 节） |
| 固件不设 `WRITE_BACK`，内核跨度被分类成 `Side` | `ebs_map` 的属性分类；随后 `KernelInfo::validate_requiring_sources` 的 strict 门 | `declared RAM authority … has no source records` 或内核跨度不落在两张表的任一区间内，strict 门 fail-closed | 看分类计数 `conv/res` 与内核跨度是否在 `reserved_regions`；确认固件内存描述符的 WB 位（§4.2.2/§4.4） |
| `KernelInfo` 切片读回全零 | `vm_handoff::classify` 或后续消费者 | VM 侧"no free memory regions"类 panic | 核对 `store_kernel_info` 是否落了六块（§4.5） |

**把 `assert_bootstrap_outside_memmap` 降级为 `debug_assert` 会怎样**（release 构建下它就消失了）：它挡的是"活的页表页与 bump 池被当作空闲内存交给 VM"这一类问题，而这类问题的发作要经过"VM 取帧 → 覆盖页表页 → 切换 CR3 → 取指失败"四步。这条断言只在两条 UEFI 腿的 `prepare_and_enter_kernel_image` 里运行：配对循环成功之后、建根之前，检查 root/bump/arena 三段（此时消息仍可经裸串口输出，消息里同时给出 conventional 区间与自举区间两个坐标）。它**不在** `prepare_boot` 路径上，也不在两个直核面里：那三条腿的自举页保护是另一组机制——U-Boot shim 在装载前用 ELF 程序头算出的 LMA 跨度对链接上界与 bump 池做编译期/运行期断言，两个直核面用 `place_modules` 的源区间 × bump 池检查，内核侧再由 `vm_handoff::classify` 按 `for_each_live_bootstrap` 整段扣除。把 UEFI 这条断言降级，等于把一段可读的坐标信息换成一次无坐标的崩溃；其余三腿本来就没有这条消息，排查时不要按它去找。

### 6.4 可复制命令

按"看输出 → 看异常 → 看页表走查"三层排列。每段都给出预期输出的关键行，以及关键行缺失时说明什么。

```bash
# 第一层：串口捕获（最常用）。真机门链五条，成员与判据见 §5.4 的表格；
# aarch64 直核面不在五条内（由 test-smp-aps-aarch64.sh 间接驱动，判据是 SMP 交付）。
# 用现成脚本，它会把串口写入日志文件并 grep 标记。
$ bash os/qemu-tests/test-shim-bootmarks-x86.sh         # x86_64 UEFI：shim→独立镜像交接门
$ bash os/qemu-tests/test-shim-bootmarks-aarch64.sh      # aarch64 UEFI：两条固定标记（`kernel loaded (entry staged)`、`12 boot modules loaded`）+ 第三条变体之一（memmap source 行；DTB_CFGTABLE=1 形态则是 DTB preflight 拒绝行）
$ bash os/qemu-tests/test-riscv64-uboot.sh              # riscv64 U-Boot 直载测试链：fatload + bootelf（装 hello-boot 测试内核）
$ bash os/qemu-tests/test-riscv64-uboot-shim.sh         # riscv64 U-Boot shim：六路标门（表、装载、交接、调度器）
$ bash os/qemu-tests/test-kernel-image-riscv64-bootface.sh   # riscv64 直核自引导（bootface 入口腿）
# 关键行：x86 UEFI 至少看 `kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、
#        `resuming high-half boot`、`entering scheduler`；aarch64 UEFI 看前两条固定标记 + 第三变体之一（见 §5.4）；
#        riscv64 三条链的判据各不相同，按 §5.4 对应行判定：`test-riscv64-uboot.sh` 看
#        `### TEST_RESULT: PASS hello-boot-riscv64 ###`，`test-riscv64-uboot-shim.sh` 看六路标，
#        `test-kernel-image-riscv64-bootface.sh` 看表址行与失败行序。
# 若关键行缺失：aarch64 脚本会打印"出现在/未出现"两行；U-Boot 脚本在宿主兼容缺口时打印 SKIP 原因与 U-Boot 版本（不再回打尾部 20 行）；bootface 脚本回打尾部 20 行，按各自脚本的实际输出排查。

# 裸 QEMU（需要自己准备固件与磁盘时）。x86_64 的 OVMF 路径与 aarch64 的 AAVMF 路径参数不同，
# 固件路径按发行版替换：
$ qemu-system-x86_64 -bios <OVMF_CODE.fd> -drive format=raw,file=disk.img -nographic -serial mon:stdio
$ qemu-system-aarch64 -machine virt,gic-version=3 -cpu cortex-a72 -nographic \
      -drive if=pflash,format=raw,file=<AAVMF_CODE.fd> -drive format=raw,file=disk.img
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel bios-or-image.elf

# 第二层：异常与页表走查（-d 追踪项逗号分隔；mmu 会放大日志量，只在定位时开）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf \
      -d int,mmu,cpu_reset -D qemu.log
# 关键行：qemu.log 里的异常记录形如 riscv_cpu_do_interrupt: hart:0, async:0, cause:…, epc:0x…, desc=…
# 若关键行缺失：说明 CPU 没有产生异常，问题在输出通道或更早的停机（例如 halt 驻留）。

# 第三层：GDB 远程调试（-s 开 gdbstub，-S 暂停在第一条指令）
$ qemu-system-riscv64 -machine virt -nographic -bios default -kernel image.elf -s -S
$ gdb-multiarch image.elf
(gdb) target remote :1234
(gdb) break kernel::rust_image_main   # `kernel` 是 kernel-image 这个 bin 的 crate 名（见 os/kernel-image/Cargo.toml）；裸短名匹配不到 mangled 符号
(gdb) break minix_kernel::arch_boot   # 三个 cfg 版本互斥，断点落在当前架构那一份
(gdb) break minix_kernel::arch_boot_resume_high_half   # 仅 aarch64 镜像有；riscv 直核会话改 break minix_kernel::arch_boot（riscv64 U-Boot 接收镜像、x86_64 与 aarch64 三条 v1 接收腿都调用 `resume_additive`，只是各自的委托壳不同：U-Boot 与 x86_64 直接进，aarch64 经 `arch_boot_resume_high_half`）
(gdb) continue
(gdb) info registers
```

三层的分工：

- 第一层回答"哪一步没走到"：里程碑清单（6.1）把 boot-shim 的动作切成十几段，缺哪一段就知道卡在哪个区间。
- 第二层回答"卡的时候 CPU 在做什么"：`int` 给出异常类型与故障地址，`mmu` 给出 MMU/页表活动记录（`page` 是 user-mode 专用项，system 模式不产生页表遍历）。`-d mmu` 的日志量与 `-d int` 不在一个数量级，只用于定位单次页表遍历。
- 第三层回答"值是什么"：在 `arch_boot` 或接收侧委托链（分派见 02 篇的 4.5 节）断下来，检查 `KernelInfo` 的字段与 `root_page` 是否合理（`kern_size`、`memmap` 切片长度、`kern_virt_base + kern_size` 或 `KernelInfo::kern_stack_top()` 是否等于镜像尾；`bootstrap_len` 已随字段删除，不是现存的检查项）。

**只用第三层（GDB）而不开 `-d int` 会怎样**。页表切换后的故障是"取指即失败"：GDB 断在切换之后的第一条指令上时，故障已经发生，而"故障地址是哪个、是取指还是读"这类信息只有 QEMU 的异常记录才给。缺了第二层，现场只剩"停在了某处"，没有 `epc`/`CR2` 可对照页表范围。

---

#### 失败对照

如果在 panic handler 里调用 `format!` 或固件服务，第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生——串口上得到的是 panic 处理自身的崩溃或什么都没有，原始 panic 的消息与位置被覆盖。所以 panic 路径只用固定栈缓冲与 `raw_serial_line`，不分配、不调固件服务。

#### 最小示例

```bash
# 第一层：跑 x86_64 UEFI 交接门，脚本把串口写入日志并 grep 标记（§6.4）
$ bash os/qemu-tests/test-shim-bootmarks-x86.sh
```

预期关键行依次出现：`kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、`resuming high-half boot`、`entering scheduler`（§5.4 的判据列）。

#### 小结

- 两条输出通道按 `ExitBootServices` 分界；U-Boot shim 与直核面只有裸串口。
- `raw_serial` 用于尽快留痕，`raw_serial_line` 等发送侧腾空并补 `\r`，用于 panic。
- 里程碑清单按腿标注适用性；`step1+2` 不是“已经在独立镜像里”的信号。
- 排查按三层：串口捕获、QEMU `-d int`、GDB 断点。
- `assert_bootstrap_outside_memmap` 只在两条 UEFI 腿的发送侧运行。

#### 术语

- **EBS**：`ExitBootServices`；调用后 UEFI 的 Boot Services（文件、分配、控制台）全部失效。
- **裸串口**：EBS 之后直接用 `out`（x86_64）或等价路径写 COM1 的通道，本篇由 `raw_serial`/`raw_serial_line` 承载。
- **里程碑路标**：带固定文案的串口打印，用于判断执行到了哪一段（§6.1 的表）。
- **`-d int`**：QEMU 的异常追踪开关；日志给出异常类型与 `epc`/`CR2` 故障地址。
## 7. 参见


### 7.1 前置与后续篇

- [`02-higher-half-kernel.md`](02-higher-half-kernel.md)（链接布局、ELF 摆放、运行期映射、成对跳转、`k_initial_stktop`）
- [`03-kmain-cstart.md`](03-kmain-cstart.md)（保护结构）
- [`04-platform-discovery.md`](04-platform-discovery.md)（DTB/RSDT 解析与描述符）
- [`05-clock-interrupt-init.md`](05-clock-interrupt-init.md)（时钟与中断控制器）
- [`rewrite-notes/01-stage-kernel/smp_gpt_v3.md`](smp_gpt_v3.md)（SMP 设计评审稿：`online_mask` 的误用与 AP 进入/握手语义。`alloc_lowest` 的消费者与 `smp_init` 计划写进 SMP 成篇，该稿尚未覆盖这两项）

### 7.2 关联 C 源码清单

本清单只列主干文件；正文逐段引用的小文件（`glo.h`、`Makefile.inc`、`archconst.h`、`pci.c`、`boot2.c`、`biosmemx.S`、`loadfile*.h`、`ramdisk/rc`、`pm/main.c`、`vm/main.c`/`utility.c`/`region.c`、`dmp_kernel.c`、`procfs/root.c`）不重复列出，路径见各段行内链接与 `file:line` 锚点。

- `minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,mpx.S,memory.c,arch_smp.c,arch_system.c,protect.c}`
- `minix3/minix/kernel/arch/earm/{head.S,pre_init.c}`
- `minix3/minix/include/minix/{param.h,const.h}`
- `minix3/minix/kernel/{table.c,main.c,system/do_getinfo.c,system/do_copy.c,system/do_safecopy.c,system/do_memset.c,system/do_safememset.c}`
- `minix3/minix/servers/vm/alloc.c`、`minix3/minix/lib/libsys/assert.c`、`minix3/lib/libc/gen/assert.c`
- `minix3/sys/lib/libsa/loadfile_elf32.c`、`minix3/sys/arch/i386/stand/lib/exec.c`、`minix3/minix/kernel/arch/earm/pg_utils.c`
- `minix3/minix/commands/{sysenv/sysenv.c,update_bootcfg/update_bootcfg.sh}`
- `minix3/minix/servers/{mib/kern.c,vm/pagetable.c}`
- `minix3/minix/include/arch/i386/include/vm.h`
- `minix3/sys/arch/i386/include/multiboot.h`
- `minix3/etc/boot.cfg.default`

### 7.3 关联 Rust 源码清单

本清单只列主干文件；逐文件调用点见正文。

- `os/boot-shim/src/{main,lib,loader,uefi_helpers,opensbi_helpers,uboot_main,ebs_map,ebs_pair,dtb_copy,resv_claims}.rs`
- `os/boot-shim/riscv64-uboot.ld`
- `os/libs/minix-boot/src/{kernel_info,boot_prepare,boot_storage,bump,handoff_v1,handoff_v1_read,handoff_v1_write,platform,boot_params,memmap_source,fdt_layout}.rs`
- `os/kernel/src/{lib,boot_alloc,globals,vm_handoff,misc}.rs`
- `os/arch/src/arch/{pt_alloc,paging,paging_ext}.rs`
- `os/arch/src/x86_64/paging.rs`
- `os/kernel-image/check-layout.sh`、`os/kernel-image/src/{main,bootface,bootface_a64,bootface_common}.rs`
- `os/servers/vm/src/{vm_server,alloc_page}.rs`
- `os/libs/minix-types/src/types/boot.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/README.md`
- `os/qemu-tests/{test-shim-bootmarks-aarch64,test-shim-bootmarks-x86,test-riscv64-uboot,test-riscv64-uboot-shim,test-kernel-image-riscv64-bootface,test-kernel-image-riscv64,test-riscv64-boot-full,test-cmd-smoke,test-atf-riscv64,test-cmd-smoke-riscv64,test-smp-aps-aarch64,test-smp-aps-riscv64}.sh`
- `os/qemu-tests/run_all.sh`
- `os/kernel-image/check-layout.sh`

### 7.5 符号速查（C ↔ Rust ↔ 章节）

按符号查读用（跨篇符号在“相关章节”列标注篇名；这是**速查表**，不保证穷举）。三张 C→Rust 表冲突时的完整优先序是：**源码 > 定义节（§3、§4 正文） > 速查表（§7.5）**；速查表只做索引，不裁决落点；表格自身与源码冲突，以源码为准。

| C 侧符号 | Rust 侧对应 | 相关章节 |
|---|---|---|
| `MINIX` / `multiboot_init`（`head.S`） | `os/boot-shim/src/main.rs:fn main` → `prepare_and_enter_kernel_image` → `enter_kernel_image_v1` → `arch_boot_resume_v1` | §2.2 / §3.6 / §4.1 / §4.7 |
| `load_stack` / `load_stack_start` | **没有同名对应物**：C 侧是 4 KiB 临时栈，真正写进 `%esp` 的是 `load_stack_start`；Rust 的 sp 初值是 `kern_stack_top`（镜像尾，`jump_to_kmain` 写入），不是 `kernel_boot_stack_top`（那只是 64 KiB 引导栈数组自身的顶）。两者只角色相似、不是同一对象 | §2.2 / §4.4 / 02 篇的 5.2 节 |
| `pre_init` | `prepare_and_enter_kernel_image`（UEFI 两条路径）/ `opensbi_helpers::prepare_boot`（U-Boot 路径） | §2.1–§2.3 / §2.8 / §3.6 / §4.1–§4.2 / §4.7 |
| `get_parameters` | `prepare_and_enter_kernel_image` + `ebs_pair` / `prepare_boot`（U-Boot）/ `run_self_boot`（直核）的发送侧 `BootReport` | §2.3 / §2.4 / §4.2–§4.4 |
| `add_memmap` / `cut_memmap` | 构建侧：`ebs_pair::pair_and_classify`（UEFI）/ `DtbMemoryMap`（U-Boot 与直核）/ `build_memmaps`（测试辅助）；内核侧：`vm_handoff::FreeRangeSet` 扣减（C 形状账本模块已不存在） | §2.4 / §4.7 |
| `alloc_lowest` | 暂未移植（AP 启动归 SMP 篇；低地址代码拷入端 `install_at` 已有） | §2.4.2 / §4.7 |
| `overlaps` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（段间目标 × 目标）、C `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核跨度，装载期）、D `KernelInfo::validate_requiring_sources`（模块 × 内核跨度 + 模块 × 模块，接收侧复核；模块 × 内核与 C 故意重复，因为接收侧不能信任发送侧已查） | §2.5 / §4.7 / §4.3 |
| `pg_clear` / `pg_identity` / `pg_mapkernel` / `pg_load` / `vm_enable_paging` | Step 1–3、`new_from_page`、`enable` | §2.2 / §2.8 / §3.4.3 / §4.7 / 02 篇的 4.3 节 |
| `freepde_start` | 字段已退场（字段删除）；`GET_KINFO` 报告位固定填 0 | 02 篇的 4.3 节 / §3.4 |
| `alloc_pagetable` | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page`（注册式分配器） | 02 篇的 2.4 节 / §3.4 / §4.6 |
| `pg_alloc_page` | 无同名对应物（运行期 VM 物理分配器；引导期页表页见 `alloc_pt_page`） | §2.4.2 / §3.4 |
| `bootstrap_start` / `bootstrap_len` | 字段已退场（字段删除；C 侧回收机制照旧） | §2.3 / §3.4 |
| `mem_high_phys` | 无对应物（boot identity 窗口） | §2.4 / §2.4.2 / §3.2.2.1 |
| `kinfo`（全局） | `KernelInfo`（按值交接 + `.bss` 深拷） | §2.3 / §3.2 / §4.5 |
| `kmain` | `kmain`（高半执行） | §2.2 / §2.8 |
| `k_initial_stktop` | 角色对应（换栈目标），**不是同一对象**：`kern_stack_top` 是镜像尾，`kernel_boot_stack_top` 是栈数组自身的顶（sp 初值不取它，取镜像尾） | §2.2 / §2.8 |
| —（Rust 侧符号） | `KernelInfo`：接收侧 `.bss` 副本，10 个字段；`BootReport<'_>`：发送侧九字段借用视图 | §3.2 / §4.4 |
| —（Rust 侧符号） | `BootStorage`：发送侧定长载荷存储；`kern_stack_top()`：由 `kern_virt_base + kern_size` 派生 | §3.2 / §3.3 / §4.5 |
| —（Rust 侧符号） | `arena`：三条 v1 腿的八页交接区；`VmBootAllocator`：VM 启动期的取页游标 | §3.2.3 / §4.6 |
| —（Rust 侧符号） | `KERNEL_INFO`：一次性写入的 `.bss` 槽；`arch_boot_resume_v1`：v1 接收入口 | §3.7 / §4.1 / §4.5 |
