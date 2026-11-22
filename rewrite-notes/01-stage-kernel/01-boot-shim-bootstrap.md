# 01 · 引导交接：载荷、内存账与五条生产腿

> 本篇的轴是**跨边界传递的数据**：`KernelInfo` 的字段、两张内存表与来源记录、bootstrap 区间与分配器、校验门链，以及把载荷送出去的五条生产腿。
> 链接布局、ELF 摆放、运行期映射的完整机制与换栈进入归 02 篇；本篇 §4.8 只给 Direct Map 的启动期概念入口。

## 1. 概述

内核第一次拿到控制权时，手里只有引导器留下的一小堆数据。哪些必须带走、哪些只是发送侧的临时视图、带错了会在哪一步坏掉，是接下来所有工作的前提。本章先把这次交接要解决的问题摆清楚：内核需要哪些数据才能建表、映射和启动 VM，这些数据为什么不能直接共享指针，以及四条不变量如何把“交接成立”变成可以检查的条件。

### 1.0 本章导读

#### 学习目标

读完本章能：

- 说出引导器交给内核的三份数据，以及每份数据的消费者。
- 用 I1–I4 四条不变量判断一次交接是否成立。

#### 前置概念

- 只需要知道“引导器把控制权交给内核”；“高半”在本篇指内核运行所在的高地址虚拟区间，布局与换栈细节归 02 篇。本章会给出 `KernelInfo`、`memmap`、`reserved_regions` 与五条生产腿的定义。

#### 最小示例

```bash
# 交接结构的字段清单（§1.1 的第三份数据）
$ grep -n 'pub struct KernelInfo' os/libs/minix-boot/src/kernel_info.rs
# 内核侧深拷入口（I1/I3 在接收侧的落点，§1.2）
$ grep -n 'pub fn store_kernel_info' os/kernel/src/lib.rs
```

两条命令各命中一行：前者给出载荷结构的位置，后者给出“只使用私有副本”的入口；§1.2 的四条判据就是围绕这两处读的。

#### 失败对照

如果只把 `KernelInfo` 的切片指针跨镜像交给内核，接收侧会在自己的地址空间里读到发送侧的内存；高半切换后该地址在接收侧没有对应映射，按地址语义必然失效。真机另观测到整段归零；归零机制尚未定，见 §3.2.4。所以三条 v1 腿的 wire 只写固定宽度标量与字节分区，接收侧从私有存储重建切片。

#### 小结

- 内核接手时要带走三份数据：可用内存清单、内核位置记录、交接结构。
- 框 2 的区间裁剪框 1；框 1 不进入框 2。
- I1–I4 是四个失败面，不是并列的美德。
- `pre_init` 是数据与镜像的接缝：`get_parameters` 属本篇，`pg_*` 五步属 02 篇。

#### 术语

- **`KernelInfo`**：按值交接的载荷结构；三条 v1 腿跨镜像时序列化进 wire，两个直核腿同镜像也深拷进接收方 `.bss`。
- **wire**：跨镜像交接协议里按固定字节宽度写进 arena 的那份表示，与进程内结构相对。
- **v1**：交接协议的族名（第一代），与信封版本号 `BOOT_HANDOFF_V1_VERSION`（当前 10）不是同一个计数器；v9/v10 指后者。
- **v1 腿**：三条跨镜像交接路径（两条 UEFI 链与 riscv64 U-Boot shim），与两个直核面相对。
- **Direct Map**：把物理区间线性映射到一段固定虚拟地址的内核访问方式；本篇只在 §4.8 给启动期概念入口。
- **五条生产腿**：两条 UEFI 链、riscv64 U-Boot shim、两个直核面。
- **框 1 / 框 2**：C 侧旧称，分别指可用内存清单与内核位置记录。
- **`PT_LOAD` / LMA / VMA**：`PT_LOAD` 是 ELF 程序头里需要装载的段；`p_paddr` 是装载地址（LMA），`p_vaddr` 是执行地址（VMA）。
- **`p_filesz` / `p_memsz`**：段在文件里的字节数与在内存里的字节数；两者之差是 `.bss` 清零范围。
- **`e_entry`**：ELF 头里的入口虚地址；装载器只把它当作 `e_entry == min(vaddr)` 的断言，不写进 `KernelInfo`。

### 1.1 内核接手时拿到什么

引导器把控制权交给内核时，内核真正需要“带走”的东西只有三份数据：

| 数据 | 形状 | 消费者 | 为什么必须交接 |
|---|---|---|---|
| 可用内存清单 | `memmap` + `reserved_regions` 两张表，外加来源记录 | 内核建表、VM 启动、Direct Map | 固件/引导器知道哪些物理区间可用；内核无法凭空导出 |
| 内核位置记录 | `kern_virt_base`、`kern_phys_base`、`kern_size` | 建表、栈顶派生、镜像摆放校验 | 三条是 `KernelInfo` 字段；`entry_point` 不在其中，只被装载器用作 `e_entry == min(vaddr)` 断言 |
| 交接结构 | `KernelInfo`（接收侧进程内）/ wire（跨镜像）；`BootReport` 是发送侧借用视图，不是载荷 | 内核启动入口、校验门链 | 上表三份数据必须按值深拷；三条 v1 腿跨镜像，两个直核腿同镜像但仍走同一落地区（统一存储去向；校验与落地先后按腿不同，见 §3.7/§4.4.1） |

后文沿用 C 侧的两个旧称：**框 1 = 可用内存清单**（最终形态已排除内核与模块占用区）；**框 2 = 内核位置记录**。两者只有一条依赖：框 2 的区间用于裁剪框 1，框 1 不进入框 2。

三份数据之间还有一条**接缝**：`pre_init` 这个函数同时碰数据与镜像——`get_parameters` 把引导器的数据变成内核记录（属本篇），`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步建表（属 02 篇）。`head.S` 内有两次跳转：第一次是 `load_stack_start → pre_init`（属本篇），第二次是 `k_initial_stktop → kmain`（属 02 篇）。

### 1.2 四条不变量 I1–I4

| ID | 不变量 | 可观测判据 | 破坏后的第一现场 |
|---|---|---|---|
| I1 | 值可跨镜像，指针/句柄不可 | 三条 v1 腿：wire 只写固定宽度标量与字节分区；五条腿共同：接收/消费侧从私有存储重建切片 | 接收侧读到发送侧堆上的地址；高半切换后结构上失效，真机观测到归零（机制未定，见 §3.2.4） |
| I2 | 来源可追溯 | 每个投影字节有 `SourceRecord`；`audit_source_coverage` | `FreeSpanWithoutMemorySource` 或 `ReservedSpanWithoutSource` |
| I3 | 所有权明确 | `BootStorage` 单所有者；`fill_*` 一次提交（所有权实现见 §3.7 开头，bootstrap 换手见 §3.4） | 第二次 fill panic；slice 活过 backing 存储 |
| I4 | 校验先于消费 | 首次消费（建表/映射/分配）前的校验必须完成；`kmain` 入口的 `validate` 是消费后的复核门；各腿门集与顺序不同，见 §4.4 | 非法布局进入建表；消费到半提交的表 |

第一次读到这些标识符时，可以先用下面的白话替换，精确定义在第 3 章：`BootStorage` = 发送侧自有的定长载荷存储（接收侧对应物是六个 `.bss` 落地区）；`SourceRecord` = “这段内存是谁给的”记录；`audit_source_coverage` = 检查每个可用字节都有来源；`fill_*` = 把一类载荷一次性写进 `BootStorage` 的闭包。这些名字不改变四条判据的读法，只影响“证据长什么样”。

这四条不是并列的美德，而是四个失败面：I1 防跨镜像指针，I2 防“账上有字节但说不出谁给的”，I3 防半提交与悬空，I4 防先消费后校验。D1–D5 的定义在 §1.4；§2.8 按编号给证据。`freepde_start` 的字段级回填不计入 D1–D5，见 §2.8。第 3 章按四条不变量组织设计决策，第 5 章按四条列证据。

### 1.3 与 Minix3 的对应

C 侧没有“交接结构”这个类型名，但形状相同：`struct kinfo` 是 `pre_init` 填好、`kmain` 读取的那份数据；`get_parameters` 把 multiboot 信息、模块链表、命令行参数变成内核记录；`add_memmap`/`cut_memmap` 维护内存账。区别在边界：

- C 的 `kinfo` 是**同一个二进制里的全局变量**，指针与数组都活在同一地址空间；Rust 的三条 v1 腿跨镜像，wire 只能传值；两个直核腿同镜像，仍用同一套落地区深拷（统一存储去向；校验与落地先后按腿不同，见 §3.7/§4.4.1）。
- C 的 `pre_init` 同时做数据与建表；Rust 把数据与发送侧的建自举根、开分页、绝对跳转留在 boot-shim/自引导面，把内核侧的加性阶段（`arch_boot_resume_v1` 之后）与 `kmain` 进入留给 02 篇。
- C 的模块来源是引导器的模块链表；Rust 的来源是 UEFI 文件系统、U-Boot `fatload`/`BootFileTable` 或 DTB `/chosen`，取件抽象在 `FileLoader`。

### 1.4 范围、术语与接缝

本篇覆盖“引导器交出控制权”到“载荷送进内核入口”这一段：引导器契约、参数收集、内存记账、模块装载检查、载荷构造、校验、五条生产腿的发送侧。以下内容不在本篇展开：

| 内容 | 归属 |
|---|---|
| 链接布局、ELF 摆放、内核侧运行期映射的完整机制、`arch_boot_resume_v1` 之后的加性阶段与换栈进入 | `02-higher-half-kernel.md` |
| Direct Map 的启动期概念入口（建立位置与“可访问≠可分配”） | 本篇 §4.8；完整布局/策略归独立内存管理主题 |
| `kmain` 之后的保护结构 | `03-kmain-cstart.md` |
| 平台描述符解析、时钟与中断控制器 | `04-platform-discovery.md`、`05-clock-interrupt-init.md` |
| VM 接手后的内存账生命周期（§2.4.2 的下游交代） | `02-stage-vm/` 的启动交接与页表初始化篇 |

**接缝声明**（与 02 篇 §1.3 同一套语义）：

- 01 管“取到字节、校验、记账、构造载荷、发送”；02 管“把字节解释成镜像、摆放、映射、进入”。
- `pre_init`：`get_parameters` 属 01；`pg_*` 五步属 02。
- `head.S`：第一次跳转 `load_stack_start → pre_init` 属 01；第二次跳转 `k_initial_stktop → kmain` 属 02。
- 五条生产腿的计数只在 01；L0–L10 指 `os/kernel-image/check-layout.sh` 的检查项编号，权威清单只在 02。
- 栈顶名字拆开：`load_stack_start`、`user_sp` 归 01；`k_initial_stktop`、`kernel_boot_stack_top`、`kern_stack_top` 归 02。

D1–D5 只在本篇定义（02 不使用 D 编号）：D1 = 内存清单先于建表输入；D2 = 内核位置先于清单裁剪；D3 = 页表先于开分页；D4 = 交接结构先于 `kmain`；D5 = 恒等映射先于内核映射。D3/D5 的语义展开在 02 篇的 J3，本篇只保留它们的调用位置。

**编号约定**：`I-<nn>` 是评审发现号，`D-<nnn>` 是裁决号，`D2-<nn>` 是设计批编号，`R02-T<nn>` 是测试缺口编号；它们只用于回查台账，读正文不必先查。
---

## 2. C 源码分析

C 侧没有“交接协议”这个类型，却要完成同一件事：在引导器退出前把内存图、模块位置和命令行变成内核能读的记录。要理解 Rust 为什么选择按值交接，先得看清 C 的 `get_parameters` 怎样整份拷贝 `mbi`、`add_memmap`/`cut_memmap` 怎样把外来区间裁成内核账本，以及 `overlaps` 为什么必须允许相切。本章沿 `boot.cfg`、`head.S` 与 `pre_init` 的调用树把这条路径走一遍。

### 2.0 本章导读

#### 学习目标

读完本章能：

- 按 `boot.cfg` 的指令次序、`head.S` 的入口与 `pre_init` 的调用树，复述 C 侧从引导器到 `kinfo` 的完整交接路径。
- 说出 `add_memmap`/`cut_memmap` 与 `overlaps` 检查把“别人的数据”变成“自己的记录”的步骤。
- 指出 D1/D2/D4 的证据锚点，并说出 D3/D5 为何归 02 篇。

#### 前置概念

- 第 1 章的三份数据与 I1–I4；本章逐段给出它们在 C 侧的形状。
- 第 1 章 §1.3 的 `struct kinfo`、`get_parameters`、`add_memmap`/`cut_memmap` 三个名字；本章展开它们的实现。

#### 最小示例

```bash
# 内核不解释 rootdevname：消费者在用户态（§2.1）
$ grep -rn rootdevname minix3/minix/kernel/
# 魔术值断言：pre_init 的第一道门（§2.2）
$ grep -n 'assert(magic == MULTIBOOT_INFO_MAGIC)' minix3/minix/kernel/arch/i386/pre_init.c
```

第一条无输出——这正是“内核只当搬运工”的判据；第二条命中一行，给出 `pre_init` 校验寄存器的位置。

#### 失败对照

如果把 `load_mods` 写在 `multiboot` 之后，`exec_multiboot` 建表时模块链表为空，表仍会建、`HAS_MODS` 仍会置上，内核会停在 `get_parameters` 的 `assert(mbi->mi_mods_count > 0)`——不是查标志的那条断言；默认（未设 `cttyline=`）现场是 panic 刷屏并等按键；设 `cttyline=` 时 panic 走串口同步输出后立即复位，不刷屏、不等按键（§6.3）。所以 `boot.cfg` 里 `load_mods` 必须排在 `multiboot` 前面。

#### 小结

- 引导器契约由识别头、寄存器约定与 `mbi` 三样东西对接。
- `get_parameters` 整份拷贝 `mbi`，再把命令行、模块表、内存图变成 `kinfo` 的记录。
- `add_memmap` 建清单，`overlaps`/`cut_memmap` 逐条切除内核与模块占用区。
- D1–D5 是约束名，不是执行序；证据都在本章与 02 篇的建表节。
- ARM32 复用了同名入口与同一串函数名，差异在参数来源与 MMU 寄存器。

#### 术语

- **`mbi`**：引导器填好的 `multiboot_info` 结构体；`get_parameters` 把它整份拷进 `kinfo.mbi`。
- **`kinfo`**：C 侧的全局 `kinfo_t`，`pre_init` 填好、`kmain` 读取的同一地址空间记录。
- **粗档 / 细档**：内存图的两种来源——`mem_lower`/`mem_upper` 两段连续区间，或 `mmap` 条目表。
- **`overlaps`**：检查内核与模块、模块与模块区间是否相交的函数；相切必须放行。

### 2.1 引导器契约与启动信息

引导器与内核之间靠三样东西对接：内核镜像里的一个识别头、跳转时的寄存器约定、以及一份由引导器填好的结构体。识别头嵌在内核二进制里（不是配置文件），按 Multiboot 规范由引导器扫描镜像前 8KB 找到它才认这个内核（三个字段定义在 `i386/head.S` 的 [`multiboot_magic`](minix3/minix/kernel/arch/i386/head.S:multiboot_magic)）——不过本仓这条路径上跑的是 MINIX 自带的 boot2，它按 ELF 头装载，并不读这个识别头；识别头是给 GRUB 一类 Multiboot 合规引导器准备的——所以下面这段识别头解码只对 GRUB 形态有效，读本仓 boot2 路径可以整段跳过，直接看后面的寄存器约定与 `mbi` 结构（`mbi` = 引导器填的 `multiboot_info` 结构体，§2.3 展开）。

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

- `.balign 8`：把识别头对齐到 8 字节边界。这是比协议本身更严的选择（`.balign` 的实参由本篇件决定）。
- `multiboot_magic` 的值为 `MULTIBOOT_HEADER_MAGIC`，定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 `0x1BADB002`。
- `multiboot_flags` 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块请按页对齐）。**没有置视频位**（`MULTIBOOT_HEADER_HAS_VBE`，值为 `0x00000004`）——同一个代码块里的四个视频字段虽然写着 EGA/80/25，但按结构体注释（`Valid if mh_flags sets MULTIBOOT_HEADER_HAS_VBE`）引导器应当忽略它们。
- `multiboot_checksum` 的值是 `-(magic + flags)`。引导器验算 `magic + flags + checksum == 0`；三个 `long` 相加为零是"这个头部完整"的证明。它不是校验和算法，是一次加法恒等式。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`。这五个字段在 `multiboot.h` 的同一段注释下都标着 `Valid if mh_flags sets MULTIBOOT_HEADER_HAS_ADDR`，即同一个 `HAS_ADDR` 位管全部五个；本仓的 `flags` 只置了 `WANT_MEMORY` 与 `MODS_ALIGNED`，没有 `HAS_ADDR`，所以五个都不被读、保持 0。这五个 0 是一个选择：**这个内核让引导器决定装载位置**，自己不指定。

跳转时引导器约定两个寄存器：一个放确认魔术值，一个放它填好的信息结构体地址。这两个值在内核侧被一条断言和一次拷贝立刻消化掉（见 2.2、2.3）。两个魔术值只差最高半字节（`0x1` 与 `0x2`），传递方向相反：`MULTIBOOT_HEADER_MAGIC` 由内核写给引导器，`MULTIBOOT_INFO_MAGIC` 由引导器交给内核：

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
| `load_mods <模块通配路径>` | 把每个匹配到的文件**路径**挂进**引导器自己的模块链表**——只登记名字，不读文件内容、不分配地址（模块＝内核随后要逐个启动的那些服务进程的镜像，名单与顺序见 §4.3）；真正的读取与物理地址登记发生在 `multiboot` 执行时的 `module_init`（它是 `exec_multiboot` 内部的一个步骤，不是另一个顶层入口）。`mbi->mi_mods_addr`/`mi_mods_count` 两个字段也不由它填，要等 `multiboot` 执行时按链表现状建表再填（见下面次序段的原文；[`exec_multiboot`](minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot) 759-760 行） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 把整个数组 `memcpy` 进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 只是命令行里的一个 `key=value` 词，随整串进 `mbi->mi_cmdline` | `get_parameters()` 解析命令行时逐词交给 `mb_set_param()`，最终以 `key=value` 形式落进 `kinfo.param_buf` |

先说清 `rootdevname` 是什么：**它是根文件系统的设备名，写的时候不带 `/dev/` 前缀**——值形如 `c0d0p0`，对应的完整设备路径才是 `/dev/c0d0p0`。这个值由装机脚本剥前缀得到——`minix3/minix/commands/update_bootcfg/update_bootcfg.sh` 里 `rootdevname=` 那一行就是把 `$ROOT`（形如 `/dev/c0d0p0`）过一道 `sed 's/\/dev\///'`；同文件的注释还说明它在 setup 期间会变，所以 `boot.cfg.default` 里只能写成变量 `$rootdevname`，每次刷配置时重算。本节只回答"它为什么出现在命令行、为什么内核不解释它"；从命令行到 `param_buf` 再到用户态读者的完整落链见 §2.3，那里不重复前缀剥离的来历。

回到这三条指令：只有 `rootdevname=` 是"内核能直接看见内容"的——另两条的产物是一个地址或一段内存，内容要看引导器填的结构体。而它能被内核"认出来"，靠的不是引导器，是内核自己的解析函数（见 2.3）。但要注意：**内核自己不解释这个参数**（`grep -rn rootdevname minix3/minix/kernel/` 无命中），它只是把它搬进 `param_buf`；真正需要"根设备是谁"的是用户态（RAMDS——RAM 盘驱动 `ramdisk` 及其启动脚本 `rc`，经 `sysenv` 命令行取值——与 `mib`（暴露内核/系统信息的树状节点表服务）两个读者的具体落点见 §2.3 的落链图）。这条分工就是微内核分层的直接体现：**对 `rootdevname` 这类纯用户态参数，内核只当搬运工，语义留给用户态**；两个专用键（`cttyline`/`cttybaud`，有结构体字段）与 `board=`（内核有读点，但走参数表查询）是"内核只当搬运工"这条分工的例外，见 §2.3。

`load_mods` 的模块数有一个上限：`MULTIBOOT_MAX_MODS` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h) 为 20（数组槽数）；`kinfo_t` 里的 `module_list[]` 就是按这个数开的数组（`minix/param.h` 的 [`struct kinfo`](<minix3/minix/include/minix/param.h:struct kinfo>)）。内核用一条断言把它当硬约束——`assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)`：实际模块数最多 19（最后一个槽，下标 19，要留给内核），到 20 不是截断，是当场停住。

**约定：`load_mods` 必须写在 `multiboot` 前面。** 约定内容是这样的：`load_mods` 只做一件事——把匹配到的文件名字挂进引导器自己的模块链表（这条命令在 `boot/boot2.c` 的 [`command_load_mods`](minix3/sys/arch/i386/stand/boot/boot2.c:command_load_mods)，它转手交给 `module_add`）。而告诉内核"有模块"的那个标志位，是 `multiboot` 在装载内核时写下去的：`lib/exec.c` 的 [`exec_multiboot`](minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot) 先看模块开关是否开着，开着就按链表当时的内容建一张模块表，建完把 `mi_flags` 的模块位置上（`MULTIBOOT_INFO_HAS_MODS`，值 `0x00000008`），再填条数 `mi_mods_count` 与表地址 `mi_mods_addr`。换句话说，**链表被看见的时刻就是 `multiboot` 执行的时刻**，`load_mods` 写得再对，只要排在后面就赶不上。

**`load_mods` 排在 `multiboot` 后面时，内核停在哪条断言上**：关键在于建表那一步并不要求链表里有东西：链表为空时表照样建出来，只是条数记成 0。这里的"模块开关"是 `exec.c` 的全局 `boot_modules_enabled`（配置级开关，默认开，与 `load_mods` 是否执行无关），所以反序时表仍会建、标志位仍会置上，`get_parameters()` 里那条检查标志的 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会**顺利通过**；真正把内核停住的是同一函数再往后的 `assert(mbi->mi_mods_count > 0)`。这个差别直接影响去哪儿查：按"标志没置"查，会去怀疑引导器没解析 `load_mods`；按"条数为零"查，才会去看两条指令的先后。

**内核停在这条断言上时并不是静默挂死**：C 里有两份同名同职责的 assert 实现，内核链接的是系统服务库那一份（[`libsys/assert.c`](minix3/minix/lib/libsys/assert.c) 的 `__assert13`，打印完直接调 panic），不是用户态那一份（[`gen/assert.c`](minix3/lib/libc/gen/assert.c)，打印完走 `abort()`）——所以在这里失败是 panic 停住并出声，而不是用户态那种进程退出。停机收尾具体怎么出声、串口与屏幕两条通路怎么分流，归 §6.3 的失败模式表（第一行）。
### 2.2 `head.S` 入口：`multiboot_init` 与 `load_stack_start → pre_init`

这条路径上的引导器（§2.1 已点名：MINIX 自带的第二级引导 boot2）跳到 `MINIX` 标签后，入口标签紧接的一条 `jmp multiboot_init` 先跳过识别头（这一步在下面引文之外；`MINIX:` 与这条 `jmp` 在源文件里紧挨在 §2.1 引文段之前——那段引文本身从 `.balign 8` 截起），随后 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 只做三件事：设栈、清帧指针与标志、按 C 调用约定把两个寄存器值变成两个参数。**进入 `multiboot_init` 后的第一条指令必须是设栈**——这条必要性对 Multiboot 合规引导器成立，对 boot2 只取证到它实际执行这条指令（边界见 §2.1）；这一条只能是汇编，理由见本节下文（设栈是进入 C 的前提）；下面先看完整的入口，再逐行解码它为什么先设栈。

```asm
/*
 * 代码引用（节选，止于 `call pre_init`；其后的换栈与 `call kmain` 见 §2.2 第 6 条），minix3/minix/kernel/arch/i386/head.S:multiboot_init。
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
3. `push $0` / `popf`：先把 0 压栈，再把它弹进 EFLAGS。净效果是栈深不变（`popf` 消耗掉刚压的 4 字节），改的是标志寄存器：关中断（`IF=0`）、清嵌套任务位（`NT=0`），方向标志 `DF` 也清零（装入全零 EFLAGS，字符串操作按地址递增方向走）。
4. 紧跟的第二个 `push $0`：栈上的一个空位。按引文逐条算：`load_stack_start` 记作 top（4096 对齐）→ `push $0; popf` 净零 → 这个 `$0` 得 top−4 → `push %ebx` 得 top−8 → `push %eax` 得 top−12 → `call` 压返回地址后入口 `%esp` = top−16 ≡ 0。所以 `call` **前**是 top−12 ≡ 4，不是 16 字节边界；这个空位把入口从 ≡4 变成 ≡0，但仍不满足 SysV 要求的 `%esp ≡ 12`（见下文）。它不是第五个参数或保留槽。

5. `push %ebx` / `push %eax` / `call pre_init`：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压，第一个参数（魔术值）后压。`pre_init(u32_t magic, u32_t ebx)` 的两个形参就位。引文里的 `_C_LABEL(pre_init)` 是 Minix3 的汇编宏（定义在 `minix3/sys/arch/i386/include/asm.h`）：ELF 目标上直接展开为 C 符号名，非 ELF 目标上给 C 符号加 `_` 前缀；本仓是 ELF，所以这里可读作 `call pre_init`。

第 4 条的空位与第 5 条的 `push %ebx`、`push %eax`、以及 `call` 推入的返回地址一起，合计 16 字节（4 个双字）；`load_stack_start` 本身按 4096 对齐，这条对齐不是当前链接顺序下的巧合，而是一整条可以逐环验证的构建保证：`i386/Makefile.inc` 的 `.for unpaged_obj in head.o pre_init.o …` 把 `head.o` 固定在 unpaged 对象列表的首位（[Makefile.inc](minix3/minix/kernel/arch/i386/Makefile.inc) 51 行起），该列表经 `OBJS.kernel += ${UNPAGED_OBJS}`（同文件 83 行）按原序进入链接命令行；`kernel.lds` 的 `.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`（[`kernel.lds`](minix3/minix/kernel/arch/i386/kernel.lds) 18 行）按命令行顺序收输入节，所以 head.o 的 `.data` 是 `.unpaged_data` 的第一个输入节，起点就是 `ALIGN(4096)` 的输出段起点；而 head.o 的 `.data` 以 `load_stack: .space 4096` 开头（[`head.S`](minix3/minix/kernel/arch/i386/head.S) 93–96 行），`load_stack_start` 落在段起点加 4096——仍是 4096 边界。中间还有一步改名：链接模式收的是 `unpaged_*.o`，而对象列表写的是 `head.o`；构建规则把该列表里的每个对象复制/改名为 `unpaged_<name>.o` 后再交给链接器（同一个 `objcopy --prefix-symbols` 批次处理符号前缀，§3.4.1 会再用到这条机制）。没有这一步，“head.o 在列表首位”接不到“`.unpaged_data` 的第一个输入节”。

于是 `pre_init` 入口处的栈指针落在 16 字节边界上——这不是 ABI 的目标形态。这一行源码的注释只写 `/* make usable stack */`，不涉及对齐。（Rust 侧的入口对齐与构建目标细节见 02 篇的 4.3 节，本段只判 C。）i386 SysV ABI 要求的是**入口处 `%esp + 4` 是 16 的倍数**（等价于 `%esp ≡ 12 (mod 16)`，因为 `call` 已压入 4 字节返回地址）；从 4096 对齐的栈顶算起，达到 `≡ 12` 需要 20 字节，而这段布局只压了 16 字节，给出的是 `%esp ≡ 0 (mod 16)`——比标准 ABI 少一次 push。**结论**：C 原作的入口栈对齐不满足 SysV 约定；按当前 GCC 的默认假设，`pre_init` 很可能没有生成依赖栈对齐的 SSE 访问；本项目没有保存反汇编证据，因此这条只能作为风险提示（换编译器/优化档可能改变），不是设计选择。Rust 侧不继承这个问题（入口 asm 显式对齐；构建目标与 SSE 细节见 02 篇的 4.3 节）。
6. 到此为止是本篇的范围。下一行 `mov $k_initial_stktop, %esp` 与随后的 `call kmain` 属于换栈与跳转，归 `02-higher-half-kernel.md` §2.3；`k_initial_stktop` 落在哪一段属于它的 §2.4——本篇只在 §2.2、02 篇的 4.3 节、§2.8、§7.5 与 §1.4 这条边界声明里引用那个结论，不重复那套判据（为什么本篇接得住这个结论，见 §1.4）。

三件事就是：设栈、清帧指针与标志、按调用约定传参（"跳过识别头"是入口标签那一跳，不在 `multiboot_init` 内）。

`load_stack` 放在 `.data` 而不是 `.bss`，差别不在"能不能用"——栈是先写后读的结构，内容脏不脏没人管——而在**这段字节的确定性由谁负责**：`.data` 里那 4096 个零字节是镜像文件自带的，装载时按普通内容拷进内存，不依赖装载器的行为；放进 `.bss` 就没有这些字节，清零得靠装载器（本仓 [`libsa/loadfile_elf32.c`](minix3/sys/lib/libsa/loadfile_elf32.c) 在调用方给了 `LOAD_BSS` 这一位时清）。所以这条选择省下的是一个对外部组件的依赖，不是一处崩溃；2.7 引的 ARM32 入口设的是同一个 `load_stack_start`，它在 [`earm/head.S`](minix3/minix/kernel/arch/earm/head.S) 末尾同样落在 `.data`。

**把两条 `push` 对调会怎样**：i386 约定下最低地址就是第一个参数，没有另一种写法。对调之后 `pre_init(magic, ebx)` 收到 `magic = ebx`（`multiboot_info` 的物理地址，一个看起来完全合法的数字），`ebx` 收到 `0x2BADB002`。`pre_init()` 最顶端那条 `assert(magic == MULTIBOOT_INFO_MAGIC)` 当场失败，走 2.1 末尾那条链：屏幕刷出 panic 与诊断缓冲、等一个按键才复位（串口这个出口此时不会出声——`do_serial_debug` 要等命令行解析才可能被置起来，而这条断言排在解析之前）。误诊的方向在这里：这条断言只打印失败的表达式本身（形如 `assert "magic == MULTIBOOT_INFO_MAGIC" failed`），并不打印值；而此刻 `magic` 里躺着的是 `ebx` 送来的地址——一个看起来完全合法的数字。不核对两条 `push` 的次序，容易往"数据结构坏了"的方向查。

本节的三处教学点就是上面这三段：**SysV 栈对齐**（入口 `%esp ≡ 0` 而非 ABI 要求的 `≡ 12`）、**`.data` 与 `.bss` 的差别**（`load_stack` 为什么要自带 4096 个零字节）、**两条 `push` 对调**（参数顺序写反后断言能停住但打印不出值）。
### 2.3 `get_parameters`：把别人的数据变成自己的记录

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

- `memcpy((void *) mbi, (void *) ebx, sizeof(*mbi))`：把引导器结构体整份拷进 `cbi->mbi`。注意这里有个同名陷阱：`mbi` 是局部指针，而 `kinfo` 的第一个字段也叫 `mbi`（`minix/param.h` 的 [`struct kinfo`](<minix3/minix/include/minix/param.h:struct kinfo>) 里的 `multiboot_info_t mbi;`）——声明行 `multiboot_info_t *mbi = &cbi->mbi;` 就是把前者指向后者。看到 `memcpy` 里光秃秃一个 `mbi` 就以为它是全局变量，是这一段最常见的误读。拷贝之后，`ebx` 指向的那份引导器结构体不再被引用；后面要读模块表与命令行，走的是这份拷贝里记的指针。
- 这五个字段分两类来源，而**分类本身就是这一段的知识**：`mem_high_phys = 0` 是"待累计"的初值，稍后由 `add_memmap` 把它推到最高可用地址；另外四个（`user_sp`、`vir_kern_start`、`bootstrap_start`、`bootstrap_len`）**没有一个来自引导器**，它们取自链接期常量 `_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`——也就是内核镜像自己的布局。道理是：**内核被装在哪儿、内核的哪一段是分页前用的，这些事实在链接那一刻就已经定下，内核不必问任何人**。这正是 §1.1 那两条依赖的根据：框 2（内核的位置）不接收框 1（清单），反过来倒是框 2 的区间要拿去把框 1 裁短。这段代码有两个细节：`vir_bytes` 类型上的 `&符号` 取的是符号的地址值（值在链接期就定了，运行时只是读一个常数）；`user_sp` 这里先赋 `_kern_vir_base` 只是占位，同一函数后段会被 `USR_STACKTOP` 覆盖。至于这些符号的名字为何带着 `__k_unpaged_` 前缀、哪些对象被算进"分页前那一段"——那是构建期的安排：段划分与边界符号见 §3.4，改名机制的判据在 `02-higher-half-kernel.md` §2.4，本篇不重复。（引文里 `cbi->kmess = &kmess` 的 `kmess` 是 `kmessages` 字段的宏别名，见 [`kernel/glo.h`](minix3/minix/kernel/glo.h)。）
- `bootstrap_len` 是"两个符号地址相减"，得到一小段内核镜像内的区域长度。这段区域叫 `unpaged`。

  > `unpaged` 指的是链接在**低地址**、虚拟地址等于物理地址的那一小段引导期代码与数据——建页表与开分页这几步必须由它完成（否则刚开启分页的下一条取指，要去查一张还没建好的页表）。它不是"未分页的段"（那是 CPU 状态）；分页开启之后这段代码**仍在低地址上执行一小段**，靠的正是 `pg_identity` 铺下的恒等映射：`pre_init.c` 的 `pre_init` 里 `return &kinfo` 之后（`pre_init.c` 开头 `#define UNPAGED`；`pre_init.o` 与 `head.o` 同在 `i386/Makefile.inc` 的 unpaged 对象名单里），控制权回到 `head.S` 的 `multiboot_init` 末段（`mov $k_initial_stktop, %esp` / `push $0` / `push %eax` / `call kmain`）——这四条指令与 `pre_init` 同属低段 `.unpaged_text`（`kernel.lds` 的 `.unpaged_text` 收段规则），都发生在 `vm_enable_paging()` 之后。反过来，`kmain` 等后续代码不在 `i386/Makefile.inc` 的 unpaged 对象名单里，它们的符号不被 `unpaged_*.o` 的收段规则收走，链接后落在高半 VMA——这正是 `head.S` 换栈并 `call kmain` 时已经站在高半的原因。整段之所以仍可在 `kmain` 后段归还，是因为此后再没有回到它的控制流：`kinfo` 在 `kmain` 开头已 `memcpy` 到 `.usermapped` 副本（`main.c` 的 `kmain`），活跃的 `pagedir[]`/静态页表池属于高半 paged `pg_utils.o` 副本（细节见 §2.4.2 的"还引导段"行；Rust 侧为何没有对应物见 §3.4）。
- 命令行解析是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 直到 `=` 或空格、遇 `=` 后收集 `value` 直到空格。`if (*p++ != '=') continue;` 那一行处理"不是 `key=value` 形式的词"——注意 `p++` 已经把游标推过一个字符，所以 `continue` 之后从下一个字符继续扫，不会死循环。
- 每个 `key=value` 都过一遍 `mb_set_param`。然后（命令行解析之后）**无条件**再写两次：`arch=` 与 `board=`，值来自编译期的板卡身份（i386 上固定查 `BOARD_ID_INTEL`）。这两行的效果有边界：`arch=` 在内核侧只有写点、没有读点（内核的架构身份来自编译期），但用户态 RAMDS 的启动脚本会经 `sysenv arch` 读它（`minix3/minix/drivers/storage/ramdisk/rc#L9`、`#L46`）；`board=` 有读点，但当引导器传入同名键时，读取返回第一条匹配，外部输入可以遮蔽 `board=` 这一行写入的值（`arch=` 在内核侧无读者，不在内核的遮蔽对象内；用户态 `rc` 会读到被保留的第一条）（仅当外部同名键不是 `param_buf` 的第一条记录；若是首条，内核的 `mb_set_param` 会先删掉它，内核值胜出，见下文反排推演）。因此它们保证的是"`param_buf` 里至少各有一条内核写入的记录"，不是"内核无条件知道自己是什么板"。

`i386/pre_init.c` 的 [`mb_set_param`](minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param) 是这串文本的落地函数，它同时做三件事：识别少数"有专用字段的键"、维护"键不重复"（效力边界见下面对删除扫描的分析：只有待删键是第一条记录时成立）、维护 `param_buf` 的可追加格式。

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
- "已存在先删"这一段想给的不变量是"同名键最多一份"：它找到 `name` 后紧跟 `=` 的位置，把该条字符串的末尾（`q` 走到 NUL）之后的内容整段左移，覆盖掉旧条目。但这条不变量在真实代码里**只对第一条记录成立**：扫描用的 `while (*p++) ;` 在越过本条记录的 NUL 时，后缀自增已经让 `p` 落在下一条记录的第一个字节上，紧接着那条独立的 `p++` 又多走一步——于是从第二条记录起，匹配检查从记录的第二个字符开始，同名键再也匹配不上。按本节引文即可推演。先看三行字节图：

| 下标 | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| 字节 | A | = | 1 | \0 | B | = | 2 | \0 | \0 |

`\0` 是一个 NUL 字节（这里用两个字符排版）。`while (*p++)` 越过下标 3 的 NUL 后，p 落在下标 4（`B`，下一条的首字节）；紧接的独立 `p++` 又多走一步，p 落在下标 5（`=`，第二条记录的第二个字符）。所以前缀匹配从 `=` 开始，`B=` 这个键永远匹配不上。

再写 `A=1` 后写 `B=2`，缓冲区是 `A=1\0B=2\0\0`（相邻记录之间单 NUL）；此时再写 `B=3`，删除扫描找不到已有的 `B`，留下 `A=1\0B=2\0B=3\0\0` 两条 `B`；而再写 `A=3` 时待删键是第一条记录，删除正常生效。也就是说"键不重复"只在待删键恰好是缓冲区第一条记录时可靠成立——这是 C 源码的真实行为，本篇照实记录，不替它圆。
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
        · 以 "rootdevname=<设备名>\0" 的形式追加，并做同名键删除
          （删除对非首条记录失效，见上面逐段解码里对删除扫描的分析）
   → 落到 param_buf 之后，rootdevname 的读者全在用户态，而两条路共用同一个系统调用：
     读者一：命令行 `sysenv rootdevname`（最终由 RAMDS 的 `rc` 消费）（minix3/minix/commands/sysenv/sysenv.c 里
        svrctl(PMGETPARAM, …)）。时间序要倒过来读：**PM 启动期先用 GET_MONPARAMS
        （minix3/minix/kernel/system/do_getinfo.c 把 kinfo.param_buf 整块拷给调用者）取一份缓存**
        （PM 缓存细节见 PM 篇（尚未生成）；起点 `pm/main.c` 的 `main`）；之后 sysenv 的 PMGETPARAM
        从这份缓存回答，不再每次走系统任务。
        → 拿它去用的是 RAMDS 的启动脚本 minix3/minix/drivers/storage/ramdisk/rc：
          取回后拼成 /dev/xxx 交给驱动，没设时打印 "rootdevname not set"
     读者二：同一条 PMGETPARAM 路径，mib 服务也走一次：
        minix3/minix/servers/mib/kern.c:mib_kern_root_device 取值后作为
        MIB（服务用来暴露内核/系统信息的树状节点表）节点 kern.root_device 暴露（人可用 svrctl pm get rootdevname 查）
     注意：内核自身没有 rootdevname 的读者（同一检索已在 §2.1 给出）——它是留给用户态的名字
```

上面这张图里要把两件事分开：**内核侧有读这张表的机制，但 `rootdevname` 没分到读者**。`kernel/main.c` 的 [`get_value`](minix3/minix/kernel/main.c:get_value) 与 `env_get` 在这条用途上读的是 `board=`（键名定在 [`minix/const.h`](minix3/minix/include/minix/const.h)，`kmain` 开头就用它取板卡身份）；`arch=` 在内核侧只有写点、没有读点（用户态 RAMDS 的 `rc` 经 `sysenv arch` 读它，走的是与 `rootdevname` 同形的 `PMGETPARAM` 路径，见上文的读者链）。`env_get` 还服务 verbose/ac_layout/no_apic 等若干键，与本段的板卡身份无关——下一段推演讲的正是 `board` 被引导配置里多写的一个词改写的后果。`rootdevname` 不同：它穿过内核，只为了到达用户态。

所以"`rootdevname` 为什么要经 `mb_set_param` 才被认出来"的答案是：命令行在 `mbi` 里只是一串字节，内核里没有任何一处按字符串搜索引导器内存；`param_buf` 才是内核自己的参数表，而写入它、维护键唯一（效力边界见前文）与分隔格式的唯一入口就是 `mb_set_param`。不经过它，`rootdevname` 就只是一串留在 `mbi.mi_cmdline` 里、之后没人再读的字符。

**删掉"已存在先删"那段循环会怎样**。分三件事说：第一条是删掉循环的后果，后两条是循环仍在时的另两个表现（不是删除的后果）：

- **删掉循环的后果**：`get_parameters` 在解析完命令行之后无条件再写一次 `arch=` 与 `board=`，于是当命令行里已经带有 `arch=` 或 `board=` 时，`param_buf` 里会出现同一键的两条记录。读取方 `get_value()`（[`kernel/main.c`](minix3/minix/kernel/main.c)）从头扫描、**返回第一个匹配**——于是被返回的是命令行里那一份，而不是内核自己按编译期板卡身份写的那一份。删掉循环只是把"有时失效"变成"必然失效"。
- **同一个 off-by-one 的第二种表现**：删除扫描只按 `name=` 前缀匹配，不要求命中点位于记录起点——当 `p` 被推到记录中部时，前缀匹配可能从记录中部命中。例如 `param_buf` 里先有 `xboard=2`（它不是首条记录，前面还有别的键），内核再写 `board=` 时，中段 `board=2` 会被删掉，后一条 `arch=i386` 的前半截被补进原位，留下伪记录 `xarch=i386`。这一步**不会**改写 `machine.board_id`：`get_value("board")` 返回的仍是内核随后写的 `board=intel`。要改写板卡身份，外部键本身必须是 `board=` 且它不是 `param_buf` 的第一条记录——那正是下一条要排除的“首条提升分支”的反面，不是 `xboard=2` 的后果。
- **要排除的首条提升分支**：若两次内核写之间 `board=bar` 因为前一条被删而被提升为首条，随后写 `board=` 会把它删掉，最终只剩内核值；双记录/遮蔽要求该键在两次写入后仍不是首条。循环还在时，当命令行里的 `board=` 不是 `param_buf` 的第一条记录（它前面还有别的键），同样的双记录与同样的改写后果仍会出现（`arch=` 也会留下双记录；内核侧没有读者，但用户态 RAMDS 的 `rc` 会用 `sysenv arch` 读到第一条）。
### 2.4 内存账：`add_memmap`/`cut_memmap` 与条目来源

#### 2.4.1 条目来源：细表与粗档

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
- 两个分支还有一处不对称：`if` 分支显式写 `cbi->mmap_size = 0;`，`else` 分支不写。第一行的 `memset` 只清 `memmap[]` 数组本身，`mmap_size` 是 `kinfo` 的另一个字段，不在那次清零范围内；`else` 分支仍能正确起来，靠的是 `kinfo` 作为全局对象落在 `.unpaged_bss`（`pre_init.o` 的 `.bss` 输入节被 `kernel.lds` 的 `.unpaged_bss` 收段规则收进这个输出节）、初值为 0，再加上 `add_memmap` 里那句 `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;` 会把下标补上来。这个零初值前提不是免费的——§2.2 刚说过 `.bss` 的清零由装载器负责——它在 C 的两条装载路径上都被兑现：Multiboot 路径由装载器按 ELF 程序头把 `filesz` 之外的 `memsz` 清零（GRUB 履行的 Multiboot 装载义务）；Minix 自有引导路径在 `loadfile` 时传 `LOAD_KERNEL = LOAD_ALL & ~LOAD_TEXTA`（[`loadfile_machdep.h`](minix3/sys/arch/x86/include/loadfile_machdep.h)），其中含 `LOAD_BSS` 位（[`loadfile.h`](minix3/sys/lib/libsa/loadfile.h)），调用点在 [`exec.c`](minix3/sys/arch/i386/stand/lib/exec.c) 的内核装载处——两条路的 `.bss` 都是零起步。

**引导器只给粗档那一头会怎样**：`else` 分支不是"罕见机器上的边角路"，它是一档会静默变坏的降级。走这条分支时，1MB 之上被记成一整段连续可用区间；若这台机器的保留区确实落在这段范围之内（显卡的镜像区、ACPI 数据区都常见），那么后面的 `i386/pg_utils.c` 的 [`pg_alloc_page`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page) 从最高条目尾部向下发放、迟早会消耗到保留区所在的位置，把页表页这类要长期住着的结构放进本该保留的区域（保留区越靠这段的高地址端，撞上得越早）。症状不是"分配失败"，而是写入静默丢失或机器随机挂死，而且报错的位置离真正的原因（引导器只置了 bit 0）很远。这也解释了本篇 §1.1 的框 1 为什么写成"可用物理内存的清单（内核与模块占用区已排除）"而不是"引导器给的内存数"：清单的形态取决于引导器给哪一档，而"排除占用区"这一步（`cut_memmap`）两档都躲不掉。

#### 2.4.1.1 追加与切除：两个操作

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 定义在 [`minix/param.h`](minix3/minix/include/minix/param.h) 为 40。数组里每一项是"一段可用区间"，`mmap_size` 是遍历边界（最高占用下标加一，中间可有零长空洞）。两项操作：追加（`add_memmap`）与切除（`cut_memmap`）。

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

- `LIMIT 0xFFFFF000` 与两次判断：`addr > LIMIT` 直接丢弃整条；区间跨越 `LIMIT` 时把长度截到边界。两条判断的次序有实际意义——先判 `addr` 把整条在 4 GiB 之上的描述符整条丢掉（不改 `len`），再判跨界。`0xFFFFF000` 而不是 `0x100000000`，源码注释给的理由是"the rest of minix currently can't deal with any bigger"；地址要能安放进 32 位的 `phys_bytes`（`memmap[]` 里 `mm_base_addr`/`mm_length` 本身是 64 位字段，但内核通篇以 32 位的 `phys_bytes` 截断）是这条限制的后果之一，同时低 12 位为零。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）加一条提前返回：`if(len == 0) return;`——长度为零的区间被**静默丢弃**，不是断言。`kernel_may_alloc` 是全局开关，但它有**两个同名定义**：`glo.h#L76` 写的是 `EXTERN int kernel_may_alloc;`，而 `glo.h#L9-L12` 在 `_TABLE` 定义存在时把 `EXTERN` 改定义为空——`table.c` 的 `_TABLE` 定义正是先 `#define _TABLE` 再包含 `kernel/kernel.h`（`kernel.h#L42` 再包含 `glo.h`），于是 `table.c` 里另有一份 paged 副本（高半 `.bss`、编译期初值 0）；`pre_init.c#L33` 的 `int kernel_may_alloc = 1;` 是 unpaged 副本。`kmain`/`bsp_finish_booting` 与 paged 的 `add_memmap`/`cut_memmap` 都在高半代码里，读写的是 `table.c` 那一份；`pre_init.c` 那一份只在引导期有效。它分的不是"引导期 / 非引导期"这种时段，而是**这份账归谁**：`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain) 开头显式写一次 1，源码注释是"Kernel may use bits of main memory before VM is started"——此刻内核还能自由取用内存；而 `kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting) 在 `switch_to_user()` 前两行（中间隔一空行）把它写成 0，注释同样直说理由："Kernel may no longer use bits of memory as VM will be running soon"。跨过这条线再想改账，撞上的就是上面那几条 `assert(kernel_may_alloc)`：改账动作当场失败，而不是悄悄改写一份已经交给 VM 的账。
- `roundup(addr) / rounddown(len)`：**起点向大、长度向小**。这个方向只保证条目本身页对齐，**不保证**新区间落在原描述符 `[addr, addr+len)` 之内：起点上取整会让终点最多外移一页（例如 `addr=0x1001, len=0x2000` 得到 `[0x2000, 0x4000)`，原区间终点是 `0x3001`）。真正的向内夹取要按 `rounddown(addr+len) - roundup(addr)` 算长度。当前内核的调用点传入的**起点**都页对齐（代码控制的那些都是；固件 E820 基址通常按页粒度报告，但规范本身不保证）。因此“终点外移一页”的缺口在**已核对过的调用点**上不可达，不是所有合法输入都不可达；如果未来接入非页对齐的固件基址，需要先把起点向下取整或显式拒绝；但**长度**不保证页对齐——`mi_mem_lower*1024`、模块文件长度都会被 `rounddown(len)` 截掉尾页。
- 追加写到哪一格：`memmap[]` 是定长 40 项的数组，**每一项记一段可用区间**。找空位很朴素——从下标 0 往后看，第一个长度为 0 的格子就是空位（`if(cbi->memmap[m].mm_length) continue;`：长度非零，说明这格已被一段区间占着）。这里**没有单独的"已占用"标志位**，用的是"长度为零"等价于"这格没数据"。这个等价由两件事兜住：`len == 0` 的调用被入口提前拒绝；`0 < len < 4096` 的退化调用会在 `rounddown` 后变成 0 并被写成一个零长度条目（它会把 `mmap_size` 推高一格，只能靠"下次从下标 0 找空位时当空格复用"兜住）。所以零长度项在数组里通常只有一种含义——空格；但"零长度区间根本不会被写进来"这句话过强。

入口的 `assert(cbi->mmap_size < MAXMEMMAP)` 是粗检，**空洞不豁免它**：`mmap_size` 是"最高占用下标加一"，即使下标 5 有空位，只要最高下标是 39（`mmap_size == 40`），`add_memmap` 也会在找空位循环之前停住。因此"有空洞就一定能复用槽位"只在 `mmap_size < MAXMEMMAP` 时成立。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：因为是"拿第一个空位"而不是末尾追动，新条目可能落在比当前边界更低的下标上，所以 `mmap_size` 要取 `m+1` 而不能简单自增——它代表的是**遍历边界**（最高占用下标加一）。还要说清一点：**数组中间可以有零长度的空洞**（一次切除把整条抹掉时就会留下），所以每个消费者都得自己跳过零长度项——`pg_alloc_page` 从最高下标往回扫时，第一件事就是 `if(!mmap->mm_length) continue;`，那就是空洞存在的证据。
- `mem_high_phys` 只增不减：它是所有条目的最高地址上界（`addr + len`）。`pg_identity()` 依赖它（见 02 篇的 2.4 节）。

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

- 方向：`start` 向**下**取整到页，`end` 向**上**取整到页。这与 `add_memmap` 的方向相反，因为两处要的保证不同：`add_memmap` 只保证"写进账的条目自身页对齐"（不保证整段落在原描述符内），`cut_memmap` 要"不把用过的页留在账上"（起终点都向外取整，宁可多切一页；取整在非退化请求上不改变净账本，真正改变结果的是退化请求，见 §2.4.2.3）。
- `substart/subend` 的两次修正把切除区间夹进当前条目的范围内。这一步让"一条切除跨几个条目"变成若干次互不干扰的子切除。
- `if(substart >= subend) continue;`：本条目与切除区间无重叠，跳过。注意这里比较的是夹取后的值，所以"切除区间完全在本条目之外"与"恰好相切"是同一个分支。
- 命中时先把自己清零（`mm_base_addr = mm_length = 0;`），再用 `add_memmap` 把**左余量**与**右余量**补回去。补回的顺序是从低到高，但 `add_memmap` 找的是"从下标 0 起的第一个空槽"（`i386/pg_utils.c` 的 [`add_memmap`](minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap) 的循环从 `m = 0` 起），所以两条补回落在哪取决于数组此前的状态：若此前没有空洞，它们依次占用刚清空的那个槽与它后面的第一个空槽；若此前已有空洞（前一条解码说过，切除会留），左余量会落进**最早的**那个空洞，而不是刚清空的槽。手算切除后的槽位布局——进而算 `mmap_size`——时按"先填最早空洞"这条规则走才不会错。
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

**2.4.1 结算**：走到这里，机器上第一次有了**一份有据可查的可用物理内存清单**——条目来自引导器（细表或粗数两档之一），内核与各模块占用的区间将在 §2.5 从里面切掉（到这里尚未切；这是 §1.1 框 1 的原材料，框 1 要到 §2.5 的 `cut_memmap` 之后才完成）。"有据可查"指来源可信，不指内容已正确：粗档可能把保留区记成可用（见本节末），`mem_high_phys` 给出了上界。

#### 2.4.2 从记录里取址：两个消费者、两条取整规则与移交截断

除 §2.4 开头那两项操作（追加 `add_memmap`、切除 `cut_memmap`）外，`pg_utils.c` 还有两个从内存记录里取地址的函数——一个从最低处取、一个从最高处取（`i386/pg_utils.c` 的 [`alloc_lowest`](minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest)、`:pg_alloc_page`），本节讲的就是这两个；另有一个 `alloc_pagetable`，它不从记录取页，用的是自己的静态池，放在 §3.4.2 讲。2.4.2.4 另交代第三件事——移交 VM 时的 16 条截断；它不属于"取址"，但属于同一份清单的生命周期，紧接 `mmap_size` 的边界讨论放在这里。

```c
/*
 * 节选（保留与分配语义直接相关的行；省略号处为未改动的循环变量声明与方法体其余行；
 * 并略去源码原有的函数头注释），minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest 与 pg_alloc_page。
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

##### 2.4.2.1 `pg_alloc_page`：从最高下标取页

**下标序与地址序的关系。** `pg_alloc_page` 从**下标最高**的条目开始取（`for(m = cbi->mmap_size-1; ...)`），取该条目的最后一页（先减长度，再返回"基址+新长度"）。"下标最高 = 地址最高"并不是一条被维护的不变量：进账按引导器报告的条目顺序写入（固件通常升序，规范不保证；本句只用于说明"进账不排序"）；切除时左余量从下标 0 找第一个空槽回填，右余量同样从下标 0 找第一个空槽，落点取决于数组此前状态：

| 数组此前状态 | 左余量落点 | 右余量落点 | 序是否断开 |
|---|---|---|---|
| 无更早空洞（含 m+1 已被占用） | 刚清空的槽 m | m 之后第一个空槽；若其后没有空槽，`add_memmap` 在 `panic("no available memmap slot")` 处停机（§2.4 引文） | 若 m+1 已被占用且其后有空槽，右余量下标高于 m+1、地址可能更低（取决于更早切割形态） → 断开；m+1 空闲时不断开 |
| 恰有一个更早空洞 h < m | 空洞 h | 槽 m（若有第二个空洞 h2，右余量落 h2，不是 m） | 通常不断开；若空洞 h 的地址反而高于槽 m（取决于更早切割形态），仍会断开 |

所以不能写成"只要上方有条目，升序必破"；能站住的结论只有一条：C 不维护序，并且存在至少一种切割形态使下标序 ≠ 地址序（例如 m+1 被占用时右余量落到更后的空槽），因此 `pg_alloc_page` 按**下标**取不等于按**地址**取。它在取页时把用量记进 `kernel_allocated_bytes_dynamic`。

它取页前在循环体内有三条断言（另有函数入口的 `assert(kernel_may_alloc)`），其中 `assert(!(mmap->mm_length % I386_PAGE_SIZE))` 是"记录里所有长度都必须是页的整数倍"这条不变量在"发页"这一步的校验点（写账一侧由 `add_memmap` 的 `rounddown` 保证，见 §2.4.2.3 的取整推演）。

##### 2.4.2.2 `alloc_lowest` 的消费者与校验缺口

本小节讲 `alloc_lowest` 的消费者与校验缺口；参数错误本身（把长度当结束地址传给 `cut_memmap`）以及它在核对过的调用序下为什么不立刻暴露，放到 2.4.2.3 一次推完：先看三种取值关系，再逐条核对四个前提，最后给结论。本节不重复那三段推演。

`alloc_lowest` 的唯一调用点在 SMP 路径：`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 里 `trampoline_base = alloc_lowest(&kinfo, tramp_size);`，用途是把一小段 16 位启动代码拷到 1MB 以下给辅助处理器执行。紧接着的校验只查了返回值的上界：

```c
/*
 * 代码引用（自函数中部起引；前段为 16 位跳板字节数组与准备代码，此处省略），
 * minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline。
 */
	tramp_size = (unsigned) &__trampoline_end - tramp_start;
	trampoline_base = alloc_lowest(&kinfo, tramp_size);

	/* The memory allocator finds the lowest available memory.. 
	 * Verify it's low enough
	 */
	assert(trampoline_base + tramp_size < (1 << 20));
```

注意 `assert(trampoline_base + tramp_size < (1 << 20))` 检查的是"地址足够低"，而不是"这段内存已经从账上切掉"。所以这条校验通不过时能发现"分配得不低"，却检查不出"分配了但账没改"。

##### 2.4.2.3 两条取整规则、退化请求与 `alloc_lowest` 的参数错误

**判决**：`alloc_lowest` 把长度传给 `cut_memmap` 的结束地址参数，是一次真实的参数语义错位；在当前调用序与四个前提下，切除一次都不发生（第三行常态），跳板页仍挂在空闲账上。四前提是核对过的调用序，不是结构保证；现有证据只支持“本仓观测布局下不立刻发作”，不支持“永不发作”。

**若 `cut_memmap` 不把 `start` 向下、`end` 向上取整**：补回余量时走的仍是 `add_memmap`，它自带 `roundup(addr)`/`rounddown(len)`，所以**记录里不会出现非页对齐长度**——这一条不变量由 `add_memmap` 独立保证，与 `cut_memmap` 的取整无关。取整真正改变的只有一种情形：**退化请求**（`start == end`）且首尾地址未落在页边界上。带取整时它被扩成"这一页整页出局"；不带取整时夹取后 `substart >= subend`，一次循环都不命中，等于什么都没切——被切对象照样留在空闲清单里，等着被第二次发出去。（退化请求恰好页对齐时，两个取整都不生效，取不取整同样是"什么都没切"。）其余情形（起点未对齐、终点未对齐、区间跨页、整条覆盖）取不取整**结果相同**，因为补回的余量都要再过 `add_memmap` 的对齐，边界上的半个页本来就会被丢掉。两处取整的分工因此是：`add_memmap` 保证"账上永远不出现非页对齐的条目"，`cut_memmap` 的取整保证"带取整的切除请求至少让涉及的那一页出局"；退化且未对齐的请求在不取整时会漏切，这正是上一段的情形。方向是保守的，代价最多是一页可用内存。

**若 `alloc_lowest` 的切除参数写错会怎样**。`cut_memmap` 的第三个参数是**结束地址**（签名：`void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)`），而这里传的是 `len`——一个长度。于是"实际切掉的区间"是 `[lowest, len)`，"想要切掉的区间"是 `[lowest, lowest+len)`，两者相差正好 `lowest` 个字节。分三种情况：

| `lowest` 与 `len` 的关系 | 实际切掉 | 后果 |
|---|---|---|
| `lowest == 0` | `[0, len)`，与想要的一致 | 行为正确——但这个分支在真实调用序下不可达（`arch_init` 早已把 `[0, 0x1000)` 切掉，见下） |
| `0 < lowest < len` | `[lowest, len)`——切了，但**比分配量少了 `lowest` 字节** | 尾部那 `lowest` 字节仍挂在空闲清单上 |
| `len ≤ lowest` | 夹取后 `substart >= subend`，一次都不切 | 整块被占用的内存从未从账上除去 |

真实调用序落在哪一行，取决于四个前提。先把四个前提逐条核对，再回到结论；任何一条不成立，下面的“第三行常态”就不成立，需要按新的调用序重推。

1. **前提一：调用嵌套成立。** `kmain` 先调 `cstart`（`kmain` 的第一段 C 启动函数，内含 `arch_init`），`arch_init` 在 `cstart` 体内、其两条 `cut_memmap` 在 `arch_system.c` 的 `arch_init` 末尾；`smp_init` 在 `kmain` 里晚于 `cstart`。`alloc_lowest` 只在 SMP 路径被调用（`copy_trampoline`）。
2. **前提二：最低条目基址非零且页对齐（≥ `0x1000`）。** `arch_system.c` 的 `arch_init` 末尾两条 `cut_memmap` 切掉含 0 的低段（BIOS 区 `[0, 0x4FF]` 与 `[0x90000, 0xFFFFF]`，取整后即 `[0, 0x1000)` 与 `[0x90000, 0x100000)`）；而 `add_memmap` 会把每条记录的起点向上对齐到页，所以记录里的最低基址至少是 `0x1000`。论证只需要"非零且页对齐"这一条，不依赖任何具体读数；两条 `cut_memmap` 已经保证含 0 的低段被切掉，`add_memmap` 的向上对齐再保证基址是页边界。
3. **前提三：`roundup(tramp_size)` ≤ 最低基址。** 跳板约 2212 B（`trampoline.S` 的 16 位代码 + 固定 IDT 2048 B + GDT 表），`roundup` 为 `0x1000`。GDT 表不是“每 CPU 一个 8 B 槽”，而是 `GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + CONFIG_MAX_CPUS` 个 8 B 描述符（`archconst.h`；当前 `CONFIG_MAX_CPUS = 1`，所以表是 7×8 = 56 B）。按当前 `C = 1` 计算总量约 2212 B，`roundup` 后 4096 B，前提成立；若改 `CONFIG_MAX_CPUS`，用同一式子重算，不要沿用旧的 `8×C` 近似。
4. **前提四：`alloc_lowest` 没有第二个调用点。** `rg -n "alloc_lowest" minix3/minix/kernel/arch/i386/` 命中定义、原型与唯一调用（`arch_smp.c` 的 `copy_trampoline`）。

**第一级结论**：四条同时成立时，真实调用序落在第三行：`len ≤ lowest`，切除一次都不发生。本篇把“在核对过的调用序与四个前提下落在第三行”称为**第三行常态**，它不是结构保证；任何一条前提变化，"常态是第三行"就要重推。

**第二级结论**：第三行常态只说明“切除不发生”，它本身不说明“这块陈旧空闲内存不会立刻被踩”。后者还要两条**不在四前提内**的条件：其一，`pg_alloc_page` 从最高下标取页，跳板所在的低条目在第三行常态下不是最高条目（下标序与地址序的偏差见 2.4.2.1 小节的"下标序与地址序"段）——这同样是观测常态，不是结构保证；其二，跳板的生命周期在 `smp_init` 完成、VM 启动之前就结束（本节末“两个取址方向各有其消费者”一段给出的是分工，不是时序锚点；这条目前是**观测/推断，尚未证明**，与第一条同属“观测常态”），而 VM 的分配器是首次适配（`minix3/minix/servers/vm/alloc.c` 的 `alloc_mem` 注释明写 first fit），它即使把低地址当空闲页发出，也发生在跳板不使用之后。所以准确的说法是：**四前提保证落第三行；第三行 + 取页方向 + “跳板条目不是最高条目”这一布局观察 + 跳板生命周期，才说明在本仓观测到的布局下不会当场发作**。四前提不是“不会暴露”的充分条件，也不是无关项——它们是第三行结论的必要条件；把它写成结构保证会超出当前证据。没有运行历史证据能证明它"从未"暴露；这不是它写对了。

后两种缺陷形态（`0 < lowest < len` 与 `len ≤ lowest`）的共同后果是：返回的地址会被 `copy_trampoline` 拷进 16 位启动代码，而这段物理内存**在账上仍然是空闲**（或至少残留一段）；一旦更高下标的条目被耗尽（或下标序被切割形态打乱），下一次 `pg_alloc_page` 就可能把它当成空闲页发出去，辅助处理器启动到一半时它的启动代码被改为页表内容——症状是某个 AP 起不来或执行到随机地址，而不是"内存分配失败"。这也解释了为什么看到"分配"两个字，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址。

两个取址方向各有其消费者，上面两个函数恰好各走一头。`pg_alloc_page` 从最高条目取页，意在让记录里最低的那一段保持连续；唯一要求低地址的消费者是辅助处理器的启动跳板——`i386/arch_smp.c` 的 [`copy_trampoline`](minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline) 经 `alloc_lowest` 取它，并断言结果落在 1MB 以下——而 `alloc_lowest` 是从最低处扫的。但"两个方向互不消耗"只是**观测常态**而非结构保证：下标序可破（2.4.2.1 小节的"下标序与地址序"段），跳板内存仍挂在空闲账上时，任何一次 `pg_alloc_page` 都可能把它发出去——这正是"切除一次都不发生"的后果；四前提只保证落在第三行，真正兜住"不立刻发作"的是取页方向与跳板生命周期这两点。

往后从这份账里取页的，是 `pre_init()` 之后的 4KB 细粒度映射路径与 VM 接手后的所有分配——引导期这次建表本身用的是静态页目录与静态池，不从账上领页（见 §3.4.2）；这些取页动作都以这份账为唯一依据。
##### 2.4.2.4 清单移交 VM 时的 16 条截断

另一件必须单说的事：**C 的 VM 接手这份账时会把它截到 16 条**。VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条（`NR_MEMS = 16` 定义在 [`minix/const.h`](minix3/minix/include/minix/const.h)），17–40 号条目（按 C 的 1-based 编号；对应 0-based 下标 16..=39）被静默忽略——`vm/utility.c` 的注释原文是 "Any memory chunk in excess of NR_MEMS is silently ignored"。账在 40 条容量里建、在 16 条口径里被消费，这条截断是"清单怎么传下去"的一部分。Rust 侧不继承这条截断：`KernelInfo.memmap` 是切片，没有独立容量；落地区与 VM 交接的容量、溢出行为见 §4.5 的"落地区与交接容量"表。`VmBootHandoff` 的版本号与 arena 信封版本是两个独立计数器，定义见 §3.2。

##### 2.4.2.5 模块内存与 unpaged 段的归属变更（下游延伸；§1.4 已声明的越界交代）

`cut_memmap` 在 `pre_init` 里被调用时只覆盖引导期占用，切除本身不会在 `pre_init` 返回前撤销；这份账在 VM 接手后由 VM 的空闲位图取代，只有内核 unpaged 段会在 `kmain` 后段被加回（下表“还引导段”行）。接下来两处"改这份账"的动作发生在 `pre_init()` 返回之后（本章主线的下游延伸）——要把这份记录的完整生命周期一次讲清，不能只看引导准备那一段。下表把这两处连同 `pre_init` 内的"切占用"一处并列（第一行属 §2.5，只作对照，不是本节主体），对象各不相同；下面一律**按动作命名**（按先后编号容易和别处的"第几点"混起来）：

| 名称 | 对象 | 时刻 | 谁做 | 动作 | 为什么 |
|---|---|---|---|---|---|
| **切占用**（§2.5 执行，不是 §2.4） | 内核与全部模块的字节 | `pre_init` → `get_parameters` | `cut_memmap`（本篇 2.4） | 从可用记录里切掉 `[mod_start, mod_end)` | 这段内存里躺着模块的字节，页表页与内核数据不能分配到这里 |
| **还 VM 镜像** | 只有 VM 模块的字节 | 内核装载启动进程时（[`arch_boot_proc`](minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc)） | `add_memmap` + 清空模块描述符 | 把模块的字节段复制进目标地址空间后，把这段物理区间加回记录，并把 `mod_start`/`mod_end` 置 0 | 字节已经搬到别处，原地址可以复用；置 0 是"已回收"的标记 |
| **还引导段** | 内核自己的 unpaged 段 | `kmain` 后段（`cstart`、`system_init` 之后） | `add_memmap(bootstrap_start, bootstrap_len)`（[`main.c`](minix3/minix/kernel/main.c) 的 `kmain` 体内、`system_init` 之后；此时 `kernel_may_alloc` 仍为 1——它要到 `bsp_finish_booting` 末尾、`switch_to_user()` 前两行（中间隔一空行）才写 0） | 归还分页开启前使用的那一小段 | C 的 `unpaged` 段只在启动期需要：`kinfo` 在 `kmain` 开头已拷贝到 `.usermapped` 副本，活跃的 `pagedir[]` 与静态页表池属于高半 paged `pg_utils.o` 副本；与模块无关 |

三条容易混的口径：**"切占用"处的是模块字节连同内核镜像自己那一块，"还 VM 镜像"处的是模块字节，"还引导段"处的是 unpaged 段**（边界由链接脚本导出的 `_kern_unpaged_start/end` 给出）。另外，"还 VM 镜像"只还 VM 那一块，其余模块的字节区间在 C 里从头到尾都没加回过——代价见本节末。

"还 VM 镜像"的源码（本篇 §3.4 引用的扣除表把这条 C 对应关系**改述**过一行，不是代码引用）：

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

- `add_memmap` 的实参是 `mod->mod_start` 与 `mod->mod_end - mod->mod_start`（起点与**长度**），与 2.4 里 `cut_memmap(cbi, lowest, len)` 那个把长度当结束地址的写法形成对照：两个被调函数同在 `pg_utils.c`，但调用点分属 `protect.c` 与 `pg_utils.c`，对"第三个参数是什么"并不一致（`add_memmap` 的第三个参数是长度、`cut_memmap` 的是结束地址），两组调用只能逐个核对被调函数的签名。
- 置零发生在加回之后，且两个字段一起置 0。此后任何遍历 `module_list[]` 的代码都会看到一条"空模块"；这是"已回收"在数据结构里的表示（而不是另设一个布尔位）。
- 这一次回收只针对 VM 的模块（`vm_allocated_bytes` 紧跟着被记录），因为它的字节已经被复制进 VM 的地址空间。其余模块在 C 里没有对应的加回动作（见本节末的代价分析）。

**其余模块在“内核账”上不归还，代价是什么**：内核 `kinfo.memmap` 的可用总量里，12 个引导模块里除 VM 之外的 11 个（名单与来历见 §4.3；这是本仓 `MODULE_NAMES` 的计数口径，C 侧实际装载数由 boot2 与配置决定，不保证同为 12/11）的字节区间一直缺着：`cut_memmap` 把命中区间从记录中切走，再用 `add_memmap` 把左右余量补回；模块落在一条大区间内部时，结果是原条目被切成两段，那一段地址不再以任何条目存在（只有模块恰好覆盖整条目时，才留下一个 `mm_length` 为 0 的空条目），此后没有 `add_memmap` 把它加回。但这不等于系统永久少掉这段内存：VM 启动时先对每个模块 `mem_add_total_pages(len/VM_PAGE_SIZE)` 把总量补进自己的位图，随后 `exec_bootproc` 对其余 11 个模块把内容装进进程地址空间，再用 `free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len))` 把 blob 放回 VM 的空闲位图（`minix3/minix/servers/vm/main.c` 的 `exec_bootproc` 与 `free_mem` 调用段；循环对 VM 自己的模块 `continue`，VM 的 blob 由内核侧 `arch_boot_proc` 的 `add_memmap` 归还，见 §2.4.2 的“还 VM 镜像”行）。所以准确说法是两本账：内核引导期的 memmap 不再收回，VM 接手后由 VM 回收；在 VM 接手之前，这段内存不可用。

这条代价不该被夸大成病根：内存宽裕时少这几段照样启动完成（C 就是这么跑完的）；内存本来就不够时，就算内核账把 VM 之外那 11 段也还回去，只要缺口大于还回量，一样会分配失败（若缺口小于还回量，结果可能改变；这条代价只讨论常态）。它至多是压垮可用总量的最后几页，而不是原因。启动期唯一会走到 `pg_alloc_page` 的是装载启动镜像那条链（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 传 `PG_ALLOCATEME` 给 `pg_map`；检索命令 `rg -n "PG_ALLOCATEME" minix3/minix/kernel/arch/i386/` 只有 `protect.c` 的 `libexec_pg_alloc` 把它作为实参传入，`pg_utils.c` 的三处是消费点），所以真到了耗尽那一刻，panic 会打在装载途中——但那只是**报信的地方**，不是**出错的地方**。

它真正的坏处是安静：不报错、不留痕。也因此有查法，但**残差不是单因**：引导期的占用记在两个账上——`kernel_allocated_bytes` 是内核镜像自己那一块（`_kern_size` 减去 `bootstrap_len`），`kernel_allocated_bytes_dynamic` 在 `pg_alloc_page` 上每发一页累加一次、在 `alloc_lowest` 上每分配一段累加一次；内核账缺的模块字节两边都不记。拿固件报告的 E820 可用总量依次减掉这两笔，再去对 `memmap[]` 的可用总量，残差至少还混着三项：`arch_init` 的两条 BIOS 区 `cut_memmap`（典型样本约 `0xF000` 字节）、`add_memmap` 对每条 E820 条目尾部不足一页的取整损失、以及 `alloc_lowest` 已记入动态计数但切除为空操作的那一页（§2.4.2.3）。所以这条对账只能证明"残差存在"，不能把残差全部归因给模块字节；要单指模块字节，需先在样本上扣掉这三项。

"还 VM 镜像"在新设计里的处理与这里不同（落点与理由见 §3.4 的扣除表）；"还引导段"这一步则不存在——Rust 侧没有 unpaged 段，回收区间字段也从交接结构里删掉了（§3.4）。

**2.4.2 结算**（C 侧）：三处改账动作（本节表的前三行——本小节主体的两处加 §2.5 的“切占用”一处）在 C 里把 VM 模块的字节交回系统（其余模块不归还，见本节末）、把内核镜像排除在可用清单之外、并在 `kmain` 后段归还 unpaged 段。Rust 侧没有 unpaged 段，模块字节改由 `boot_modules` 交接并由扣除表整段保留——这些落点属于第 3 章，见 §3.4.2 与 §3.4.3，本节不再展开。
### 2.5 `overlaps` 与 `kern_mod`：内核把自己算成第 M+1 个模块

内存账建好后，要从中切掉两块东西：模块占用的区间与内核自己占用的区间。对每条记录，切除之前先检查它与其余记录是否重叠——`overlaps` 就是源码注释里说的那道 sanity check；检查与切除按记录交错，没有全局预检。实际布局里相邻模块由 boot2 顺序摆放、互不重叠：boot2 一边装载一边推进 `image_end`（[`lib/exec.c`](minix3/sys/arch/i386/stand/lib/exec.c) 里每轮先 `image_end = (image_end + PAGE_SIZE - 1) & ~(PAGE_SIZE - 1);`（584 行），再 `bi->base = image_end;`（595 行）、`image_end += len;`（617 行），下一条再从页对齐处接着放——前一条长度恰为页的整数倍时两者正好相切，否则中间留一段对齐补白），所以正常情况下谁也不会盖住谁。但"相切"恰恰是边界处理最容易写错的那一种相邻关系——本节末尾就专门推演它。真出现重叠（换个引导器、或模块与内核被放在同一处）时——**若没有这道检查**——症状会非常难追：两个镜像互相覆盖而无人报错。

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

- `kernbase`/`kernsize` 不来自引导器，是链接脚本导出的常量（`_kern_phys_base = 0x00400000`、`_kern_vir_base = 0xF0400000`；`_kern_size` 由链接布局算出、不是字面量。完整摘录与解读见 §3.4）。这两个符号在 `pre_init.c` 的 `get_parameters` 里各有更早的读点：`_kern_phys_base` 与 `_kern_size` 已在 `kernbase`/`kernsize` 初始化处取址，`_kern_vir_base` 已在 `user_sp`/`vir_kern_start` 赋值处取址，`_kern_size` 另用来算 `kinfo.kernel_allocated_bytes`（减去 `bootstrap_len`；§2.4.2 也据此说明这个字段）。这一步是 `kernbase`/`kernsize` 首次作为**内核与模块重叠核查的输入**：**内核要参与"谁占了哪段内存"的核查，就只能自己报出自己的地址范围**——所以它把自己造成第 M+1 个模块（M = 模块数），而不是走一条特例分支。
- `assert(k < MULTIBOOT_MAX_MODS)` 是"内核还要占一个槽"这一事实的形式化：模块数最多只能是 `MULTIBOOT_MAX_MODS - 1`，因为有 1 个槽要留给内核。它与 2.1 里那条"模块数 < `MULTIBOOT_MAX_MODS`"是同一个条件（`k = mbi->mi_mods_count`），作用是在往 `module_list[k]` 写"内核当模块"这一项之前，把"数组还有空槽"这个前提再确认一次。
- 同一个循环里既检查又切除：`mods_with_kernel` 条记录，每条先与其余所有记录比一次，然后从账上切除。切除用的是 `[mod_start, mod_end)`，与 `cut_memmap` 的起止语义一致。

于是"内核与模块占用区"这一整类内存只被一段代码处理，模块与内核之间没有第二套逻辑。

**把 `OVERLAP` 第二项的 `-1` 去掉会怎样**：那一项就变成 `INRANGE(mod1, mod2->mod_end)`。取两个首尾相接的模块：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`。它们不重叠——A 的最后一个字节是 `0x1FFFFF`，B 的第一个字节是 `0x200000`。用不带 `-1` 的写法检查：

```
外层 m = 0（A 是 cmp），内层遍历到 B：OVERLAP(B, A) = INRANGE(B, A.mod_end = 0x200000)
   = (0x200000 >= 0x200000) && (0x200000 < 0x300000) = 真
```

于是 `panic("overlapping boot modules/kernel")` 落在一个完全合法的布局上。这正是"半开区间必须翻译对"这一类错误的代价：报错信息说"模块重叠了"，而真实情况是模块之间恰好相切。再注意上面那句"两个方向各检查一次"在这里的意思：真重叠时只要有一个方向报出即可（包含关系只在"被包含者作 `cmp`、包含者作 `thismod`"的那一次被发现），而相切的误报只需要一个方向成立就够——外层以 A 为比较对象的那一次（`cmp_mod = 0`）在内层遍历到 B（`m = 1`）时就已经 panic，反方向那一次根本轮不到执行。

顺带说准一件容易搞反的事：**把 `overlaps` 整个去掉，内存账并不会被弄坏**。同一段物理内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作（按 `add_memmap` 与 `cut_memmap` 的取整规则推演。真包含的例子：先切 `[0x120000, 0x150000)`、再切 `[0x130000, 0x140000)`，第二次切除的区间是第一次结果的子集，清单仍是那两截。非包含的例子：再切 `[0x140000, 0x160000)`，它与第一个区间部分相交并伸出边界，伸出部分照常从账上切走。`overlaps` 要防的是后者那种两个模块字节互相覆盖的布局；两种情形下账本本身都不会被记错——第二次切除只是在已有空闲区间上再做一次减法）。代价全在内容侧，就是本节开头那句话：两份镜像在重叠地址上互相覆盖，而内核一声不响——所以 `overlaps` 守的是**模块字节不被踩**，不是内存账不被记错。到这里，§2.4 的“原材料已就位”加上本节的 `overlaps`/`cut_memmap`，框 1（可用物理内存清单）才真正完成——§2.4 的结算只承诺原材料。
### 2.6 模块装载的检查与失败模式

模块装载的检查分三层，失败现场不同，诊断入口也不同。这一节只留 C 侧基线；Rust 侧的载体与新增防线见 §3.5 与 §4.3，不在本章展开。

| 层 | C 判据 | 失败现场 | 诊断入口 |
|---|---|---|---|
| 次序 | `load_mods` 必须写在 `multiboot` 前面；反序时链表为空，`HAS_MODS` 仍置位 | `get_parameters` 的 `assert(mi_mods_count > 0)`，不是查 `HAS_MODS` 的那条 | 先看模块条数，不看标志位 |
| 条数 | `MULTIBOOT_MAX_MODS = 20`，内核自己还要占一个槽 | `assert(k < MULTIBOOT_MAX_MODS)` 或装载器越界 | 模块数上限与内核占槽 |
| 区间 | `overlaps` 用半开区间检查内核与模块、模块与模块；相切放行 | `panic("overlapping boot modules/kernel")` | 重叠检查的 `-1` 与相切判据 |

### 2.7 ARM32 对照：自己造一份 multiboot

Minix3 同时支持 x86 与 ARM 两种架构，同一段引导准备在两侧各有一份实现、用的是同一串函数名。先看两侧的入口（`earm/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/earm/head.S:multiboot_init) 与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init)）：

```asm
/*
 * 代码引用（节选，止于 `bx r2`；其后的 `hang` 段不引），
 * minix3/minix/kernel/arch/earm/head.S（i386 侧同一段已在 §2.2 给出，
 * 这里不再重复）。
 */
.section .startup
/* ……（3 行标题注释省略，原文见 earm/head.S:22-24）…… */
.global MINIX
MINIX:
	b multiboot_init

.text
multiboot_init:
	ldr	sp, =load_stack_start	/* make usable stack */
	mov	fp, #0
	bl	_C_LABEL(pre_init)

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

两侧的相同与不同都很直接：入口标签同名（`MINIX`），入口后的第一个标签同名（`multiboot_init`），临时栈的放置位置与写法一致（`load_stack` 在文件末尾的 `.data` 段，取的是"高端"符号），清帧指针一致（`%ebp` / `fp`），调用 `pre_init` 与随后调用 `kmain` 的一致。不同的是指令集本身，以及参数传递：x86 用栈传两个参数（魔术值 + `multiboot_info` 地址），ARM 用寄存器传参，而 `pre_init` 返回的 `kinfo` 指针留在 `r0` 里一路带到 `kmain`。

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
- 于是 `get_parameters()` 里其余部分（模块表 `memcpy`、`add_memmap` 遍历、`overlaps`、`cut_memmap`、`mb_set_param`）在两种架构上同构（注意 ARM 侧头文件用的是另一套字段名），调用次序一致；但**区间约定不同**。ARM 的 `overlaps` 节选（只保留宏与循环体，函数签名与结尾略；[`overlaps`](minix3/minix/kernel/arch/earm/pre_init.c:overlaps)）：

```c
// 节选（只保留宏与循环体；函数签名与结尾略），minix3/minix/kernel/arch/earm/pre_init.c:overlaps。
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) <= thismod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
	    INRANGE(mod1, mod2->mod_end))
	for(m = 0; m < n; m++) {
		multiboot_module_t *thismod = &mod[m];
		if(m == cmp_mod) continue;
		if(OVERLAP(thismod, cmp)) {
			return 1;
		}
	}
```

两处与 x86 版不同：**其一**，`INRANGE` 的上界写成 `<= thismod->mod_end`。`mod` 是宏形参，`thismod` 不是——它是外层循环里的局部变量，宏在展开点捕获它，所以这个上界取的是"当前外层条目"的结束地址；`OVERLAP` 总以 `thismod` 作第一实参，这个读法才成立。**其二**，`OVERLAP` 第二项直接取 `mod2->mod_end`，没有 x86 版的 `- 1`。合起来是闭区间判定，正是 §2.5 论证为错误的那一版——不过 ARM 的模块槽距是 8 MiB、每个合成模块的 `mod_end` 又各留了一页（`earm/pre_init.c` 的 `MB_MODS_ALIGN` = `0x00800000`，减去 `ARM_PAGE_SIZE`），所以模块—模块不会相切。**内核项是例外**：它作为第 `M+1` 个模块追加时 `mod_end = kernbase + kernsize`，没有减一页；内核与 0 号模块的 30 MiB 间隔是"当前镜像远小于 30 MiB"的构建事实，不是结构保证（`earm/kernel.lds` 没有尺寸断言）：这个间隔由两个常量相减得来——0 号模块的物理基址 `MB_MODS_BASE = 0x82000000`（`earm/pre_init.c` 的 `MB_MODS_BASE`）减去内核物理基址 `_kern_phys_base = 0x80200000`（`earm/kernel.lds#L4`），即 `0x1E00000` = 30 MiB。因此准确说法是：在当前镜像尺寸下闭区间判定**无害**，不是"ARM 也有一处待修缺陷"；若内核恰好长到 `0x1E00000` 字节，内核项与 0 号模块会相切并被误报。

撇开两种架构的硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 只 `bl pre_init`（两侧形参不同：ARM 是 `pre_init(argc, argv)`，没有 `magic` 也没有 `ebx`），再由 `setup_mbi` 自造结构 | 引导协议（各架构不同） |
| 2 | 建立地址映射 | `pg_clear` + `pg_identity` + `pg_mapkernel` | 同名函数（页粒度不同） | 分页硬件（语义相同；机制细节归 02 篇） |
| 3 | 开启分页/MMU | `pg_load` + `vm_enable_paging` | 同名函数（MMU 寄存器不同） | MMU 寄存器（各架构不同；机制细节归 02 篇） |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定（x86 用栈、ARM 用寄存器） |

ARM 侧这两步的寄存器写入顺序与 cache 清理属于 02 篇的映射机制，本篇只保留“同名函数、语义与调用序相同”这一层；展开见 02 篇。

步骤 2、3 在各架构都调用同名函数，是因为"清零页目录 → 建恒等映射 → 把内核映射到高地址 → 装载页表基址 → 开启分页"这一串的**语义**与"用哪条指令、页表项多少位"无关；第 1 步的**入口与参数来源**是唯一必须按架构另写的**接口层**，因为"参数从哪来"由引导器决定；步骤 2、3 的**实现体**仍按架构写（页尺寸与 MMU 寄存器不同），但语义与调用序相同。第 4 步的差别只在调用约定。
### 2.8 本章归纳：C 交付物的形状

把 2.1 到 2.7 的调用关系收成一张图。树里的 `pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步属 02 篇的镜像半，这里保留它们只为给出调用位置与 D3/D5 的落点，不重复语义。这张树是 **x86-32 主链**：`head.S` + `pre_init(magic, ebx)`；ARM32 的同名入口与参数差异见 §2.7，U-Boot 与固件的 Rust 入口见 §3.6，二者都不在这棵树里。

```
引导器（x86-32 主链：boot2 / GRUB）
  │  1. 识别头（GRUB 类）/ ELF 头（boot2）+ 寄存器约定 + 填好的结构体
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
  │     ├── mbi.mmap_* → add_memmap() × R          （构建可用内存记录；R = 可用（AVAILABLE）条目数；粗档分支固定为 2）
  │     │   └─ 取不到细表时退回粗档：mem_lower/mem_upper → add_memmap() × 2（只造两段连续区间）
  │     ├── 内核当第 M+1 个模块登记（M = 模块数）
  │     └── 对每条记录 m：overlaps(m) → panic 或 cut_memmap(m)
  │        （检查与切除逐条交错，共 M+1 轮——两行合成一行以贴近 §2.5 的实际次序）
  ├── pg_clear()                （页目录清零）
  ├── pg_identity(&kinfo)       （低地址恒等映射，覆盖当前执行点）
  ├── freepde_start = pg_mapkernel()（内核高地址映射，返回第一个空闲槽位号）
  ├── pg_load()                 （CR3 = 页目录物理地址）
  └── vm_enable_paging()        （CR0.PG 置位，PSE/WP/PGE 按序处理）
  ▼
return &kinfo
```

启动期的五条次序约束（跨三份交接数据与建表/进入动作）共五条（D1–D5），定义在 §1.4。D1/D2/D4 的证据在本章，D3/D5 的语义与证据在 02 篇 J3；下面**按编号**逐条给陈述与锚点，不按执行序重排——编号是约束名，不是执行序。另有字段级回填（`kinfo.freepde_start`），不计入五条次序约束。

- **D1. 内存清单先于建表**。清单在 `get_parameters()` 里就建完并切完（`add_memmap` × R；随后对 M+1 条记录逐条 `overlaps` → `cut_memmap`，检查与切除交错），`pg_clear()`、`pg_identity()`、`pg_mapkernel()` 都在它之后才跑（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 里这几句的排列顺序）。**为什么不能反，按依赖的实际层次说**：`pg_identity()` 要用 `mem_high_phys` 决定哪些条目该标成非缓存——这是硬依赖，且它在函数开头就有一条 `assert(cbi->mem_high_phys)`：反排时该值还是 `.bss` 的零初值，断言当场失败（断言拿掉才是"得不出正确的表"）；这条 `assert` 只覆盖 `add_memmap` 这一半——`cut_memmap` 先于任何清单消费者，是由 `get_parameters()` 的调用序保证的，不是建表的正确性前提，不要把它算进 `assert` 的证明力。`pg_mapkernel()` 的初始大页映射不依赖记录（它用链接期常量与已清零的静态页目录）；从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度路径（`pg_alloc_page`），它同样要求记录已就位。
- **D2. 内核位置先于清单裁剪**。清单要排除的区间里包含内核镜像自己；镜像的物理区间来自链接期常量 `_kern_phys_base` / `_kern_size`，不来自引导器的数据结构。证据锚点：§2.3 的链接期常量、§2.4 的 `add_memmap` 与 §2.5 的 `cut_memmap` 调用序。
- **D3. 页表先于开启分页**。语义与证据在 02 篇的 J3（建表五步与 `enable` 的次序）；本篇的调用树保留 `pg_*` 的位置，不在这里展开。
- **D4. 结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`，`head.S` 把它压栈作为 `kmain` 的第一个参数（依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `return &kinfo;`，与 `i386/head.S` 的 [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) 里 `call kmain` 之前那条把返回值压栈的 `push %eax`（`i386/head.S` 的 `multiboot_init` 段内）——它在 §2.2 引文块之外；引文里那条 `push %eax` 是给 `pre_init` 传参用的，同名不同处）。
- **D5. 恒等映射先于内核映射**。覆盖次序的逐数字推演在 02 篇的 J3；本篇只保留这条约束名与调用位置。

ARM32 的主要差异是第 1 步（`setup_mbi`）与 MMU 寄存器；`overlaps` 的闭区间差异见 §2.7，不在“唯一”之列。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`，它落在高半一侧。本篇不复制这条判据——它属于"内核怎么走到高半去跑"那个主题，推证与证据表在 `02-higher-half-kernel.md` §2.4（02 篇 §2.5 已给过那条"名字不是判据"的提醒，这里不重复）。本篇只需要这一个事实：`pre_init` 返回后换上的那个栈，与紧接着要跳去的高半在同一侧，所以换栈与跳转能成对。

把本章的断言按用途分一下：`assert(cbi->mem_high_phys)`（02 篇的 2.4 节 已引）这类断言不能证明布局一定合法，它把"不合法"从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。

---

## 3. Rust 设计决策

三条 v1 腿跨镜像，两个直核腿同镜像，三个架构的固件接口又各不相同。如果每个组合各写一份交接协议，内核会在进入 `kmain` 之前就背上多套地址语义。本章要解决的问题是：怎样用一套按值交接的 `KernelInfo`、两张带来源记录的内存表和一条校验门链，把五种引导形态收敛到同一个接收侧，同时不把硬件差异带进内核。

### 3.0 本章导读

#### 学习目标

读完本章能：

- 说明为什么三个架构共用一套交接协议，而不是各写一份引导协议。
- 按 I1–I4 复述 `KernelInfo` 字段的取舍、两张内存表与来源记录、bootstrap 区间与分配器换手。
- 说出五条生产腿各自的通道形态与校验门顺序。

#### 前置概念

- 第 1 章的 I1–I4 与三份数据。
- 第 2 章的 C 侧交接：`kinfo` 全局、`get_parameters` 的拷贝、`add_memmap`/`cut_memmap` 的记账、`overlaps` 的区间检查。
- 第 2 章 §2.7 的 ARM32 对照：同语义、不同参数来源。

#### 最小示例

```bash
# 在 os/ 下跑交接库的宿主测试；wire 与字段台账的形状由这一包守住（§5.1）
$ cd os && cargo test -p minix-boot --lib
```

预期全部通过；§5.1 把 wire 的 encode/decode round-trip 与 `KernelInfo` 字段台账列为 I1 的宿主证据。

#### 失败对照

如果把 boot-shim 与内核并成一个 UEFI 二进制，内核就得认识固件类型、把 `uefi` 拉进自己的依赖表，与“内核不依赖引导层”的单向边界冲突。所以 UEFI 形态被否决，直核形态只允许 `kernel-image` 反向依赖 boot-shim 的库函数，而不是依赖引导器产物。

#### 小结

- 协议换代把“谁读文件、谁填内存图、谁退出固件服务”重新分工；UEFI 腿自己读，U-Boot 腿由 U-Boot 代劳。
- `KernelInfo` 从全局地址改为按值交接；三条 v1 腿走 wire，两个直核腿同镜像也深拷。
- 内存账从 `kinfo` 数组改为两张切片表加 `SourceRecord`，每个投影字节可追溯。
- bootstrap 区间与页表分配器在进入内核前换手；没有“分页后可回收的内核段”这个对象。
- 五条生产腿收敛数据形状，不合并引导形态层。

#### 术语

- **wire**：跨镜像交接协议里按固定字节宽度写进 arena 的表示；v1 信封带版本号，收发两侧精确比较。
- **arena**：三形态 v1 交接用的八页物理区，承载信封、分区载荷与运行期字段。
- **v1 腿 / 直核面**：前者跨镜像（两条 UEFI 链、riscv64 U-Boot shim），后者在同一镜像内直接调 `arch_boot`。
- **`BootStorage`**：发送侧自有的定长载荷存储；它拥有 U-Boot shim 腿上的六类载荷；UEFI 腿的 storage 只拥有五类，source 分区由分类器与随后的 DTB claims 两阶段直写 wire（见 §3.7）。
- **`SourceRecord`**：记录“这段内存是谁给的”的来源条目，供 `audit_source_coverage` 审计。

### 3.1 协议换代：固件接口替换 Multiboot

> `[ARCH: ...]` 是本项目的架构演进标注，表示该处不是逐字重写，而是有意的架构变化；三档术语（Rewrite / Refactor / Architectural Evolution）见 `AGENTS.md`。

C 的实现前提写在 2.1 与 2.2 里（本节标题里的 GRUB 泛指"第三方引导器"这个角色，本仓这条路径上具体是 §2.1 点名的 boot2）：引导器把内核与模块都读进内存，把它们的地址填进一份结构体，再用两个寄存器把结构体地址交给内核的第一段代码。内核自己不做任何 IO——它连"文件"这个概念都不需要。

重写的目标是 x86-64、aarch64、riscv64 三个架构，其中后两个没有 Multiboot 的等价物，而 UEFI 的启动模型只提供一个可以做 IO 的环境（Boot Services）与两个内存分配接口（页分配 `AllocatePages`、池分配 `AllocatePool`），并不替谁加载内核。于是第一步的分工被重新切开：

| 步骤 | C（Multiboot 协议 + MINIX boot2） | Rust（UEFI 路径） | Rust（U-Boot shim 路径；`FW_PAYLOAD`（OpenSBI 把下一级镜像打包进固件本体的模式）直核形态不在本表，见 §3.1.1 的例外条款） |
|---|---|---|---|
| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（标准可执行格式） | ELF 头 + U-Boot `bootelf` 约定 |
| 读内核与模块到内存 | 引导器代劳：`multiboot` / `load_mods` | boot-shim 自己读：`SimpleFileSystem` 协议 + `loader::KERNEL_PATH` / `MODULE_NAMES` | U-Boot 代劳读文件（`fatload`），boot-shim 按 `BootFileTable` 查地址 |
| 内存布局 | 引导器填 `mbi.mmap_*`；内核遍历 | boot-shim 自己取固件内存图，分成可用表与保留表（交付形状见 §3.2，分流规则见 §4.2） | 读 DTB `/memory` 节点的 `reg`，同样分成两张表 |
| 传递参数 | 寄存器 + `multiboot_info_t` | arena 基址（三条 v1 腿的入口寄存器见 §4.1）——arena 的定义与分区布局见 §3.2；`BootHandoffV1` 已序列化进 v1 arena（`KernelInfo` 的字段按协议分区写入，发送侧不构造进程内 `KernelInfo`），根页表物理地址在信封的运行期字段里 | 同 UEFI 的协议与形状，入口寄存器见 §4.1 |
| 退出固件服务 | 无此步骤（引导器早已离开） | 必须调 `ExitBootServices`：boot-shim 跑在 Boot Services 的**环境之内**，得在某个时刻主动声明"我不再需要你"；声明之后整套服务失效，本篇用到三类——读文件（`SimpleFileSystem`）、分配内存（页分配 `allocate_pages` 与池分配 `allocate_pool`，后者支撑 `Vec`/`Box`）、固件控制台输出（`uefi::println!`） | 无此步骤（U-Boot 已把控制权交出去） |

生产入口的同一约束（文件访问必须在 `ExitBootServices` 之前完成）由 §4.2 的九个动作表承担：读文件在动作 2、EBS 在动作 6。


`exit_boot_services`（**测试内核路径的辅助实现，不是生产入口**）自己的文档也把这件事说成契约；生产入口在 §4.2 的配对循环里直接用 raw `ExitBootServices`，内存图分类进两张表，**不丢弃**：

```rust
// 代码引用，os/boot-shim/src/uefi_helpers.rs:fn exit_boot_services。
#[cfg(feature = "qemu-test-kernels")]
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded: this helper is the test-kernel path only.
    // Its snapshot is best-effort and not MapKey-paired with
    // `build_memmaps`; production uses `ebs_pair` so classification and EBS
    // share one key.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}
```

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

本节的交付形状与另外四节分工，读的时候不要混：C 侧"清单被截到 16 条"在 §2.4.2.4；扣减表与 bump 整段保留在 §3.4；接收方的六个落地区加一个名字池在 §4.5；换手给 VM 的时序在 §4.6。本节只讲"交接的是什么形状"。

**维度一：通道形态**（交接怎么送到接收方）随交接形态变；"跨镜像 / 直核 / 三形态"的定义在 §3.6，这里先用。x86_64 与 aarch64 的 UEFI 链都走跨镜像跳转：boot-shim 建好自举根、开分页，把一块八页 arena（定义与分区见本节后半）的物理地址递给独立 `kernel.elf` 的高半入口（入口寄存器见 §4.1）；接收方从 arena 解码六个分区、落进自己的 `.bss`，再走只增量的高半 resume。riscv64 的 U-Boot shim 走第三条跨镜像 v1 腿（同样只递 arena 基址，入口寄存器见 §4.1）；riscv64 的直核自引导没有独立 shim，引导面在同一镜像内直接调 `arch_boot`；这是硬件形态决定的辅助启动路径（§3.5 问题二），与两条 UEFI 产线不同源。

**维度二：载荷内容**（送出的是什么）三形态一致：必须按值深拷进接收方的私有存储，不能靠指针跨边界存活。理由：跨镜像档里两个分别编译的二进制各有自己的 `.bss`，指针在接收方指向完全无关的内存，连 `&'static KernelInfo` 也只保证生命周期、不保证页在对方页表里被映射；直核面虽然没有跨镜像指针，但同一套落地区把三形态收成一条路径（少一套特例），也让"解码之后再消费"成为统一次序。三形态一致的是"必须深拷进私有存储"这一条要求；具体来源分两组：v1 三条腿的切片由固定分区与 `byte_len` 描述符重建，接收方按分区偏移与元素类型解码、不再解释裸 `(ptr, len)`；两个直核面的切片由 `build_kernel_info` 的 `&'static` 输入直接给出，没有 wire 解码步骤（§4.4）。

> **收口记录**：x86_64 与 aarch64 的 UEFI 产线采用独立镜像交接（§3.6），本节与此一致——记录保留是为了给"为什么 §3.2 的通道描述里没有低地址内联窗口"一个可考的出处。riscv64 各路径的接线状态见 §3.1.1 的接线状态表，这里不重复；本段要留的独特信息只有一条：这次切换不改变 §3.2 讲的 KernelInfo 形状与快照口径。

#### 3.2.1 `KernelInfo` 结构与它的两条产出入径

```rust
// 代码引用（略去文档注释），os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo。
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub reserved_regions: &'static [MemoryRegion],
    pub source_records: &'static [SourceRecord],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub boot_modules: &'static [BootModule],
    pub platform_sources: &'static [PlatformDescSource],
    pub param_buf: &'static [u8],
    pub authority: RamAuthority,
}
```


`kernel_info.rs` 的 [`struct KernelInfo`](os/libs/minix-boot/src/kernel_info.rs) 共 10 个字段。字段删除时去掉了 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个（见 §3.4.1 与 §3.4.3）；`authority` 是随后加入的那一个，总数由 13 变成 14；wire v10（设计裁决）又删掉 `startup_objects`（当时既是 `KernelInfo` 字段、也是 wire 分区与 builder 参数）、`user_sp`、`kern_stack_top`、`syscall_entry` 四个，落到 10。按与 C 的关系分成下面几类：

读表前先立三个局部术语：`A+D`（完整定义见 §4.2.1 动作 2：先读 ELF 预订 LMA 跨度、再分配 root/bump、最后拷贝段）；`ReservedMapped` = 固件声明为保留、但运行期可经 Direct Map 访问的档；`Side` = 既不进 free list 也不进保留表的侧桶（§4.2.2 完整判定）。

| 关系 | 字段 | 说明 |
|---|---|---|
| 保留（同名或近义） | `memmap` | `memmap` 从 `multiboot_memory_map_t memmap[MAXMEMMAP]` 变成 `&'static [MemoryRegion]`，长度自带。C 那条"VM 只读前 `MIN(MAXMEMMAP, NR_MEMS)=16` 条"的截断（§2.4.2.4 的"另一件必须单说的事"段）**不再继承**：Rust 把落地区容量提到 128 槽（常量现居 [`globals.rs`](os/kernel/src/globals.rs:const MEMMAP_REGION_STORE_LEN)，C `MAXMEMMAP=40` 提额——UEFI 图常超 40 条；`KernelInfo.memmap` 本身是切片、无容量），VM 交接消费上限 64 条（`VM_BOOT_HANDOFF_MAX_REGIONS`）——扣除后仍有 65–128 条自由区间时能过落地区的 128 槽，但会在交接容量断言处 fail-closed（`VmBootHandoff.deducted` 另有 47 条容量；v9 已不存在 `kern_ident`/`VM_BOOT_HANDOFF_MAX_IDENT`，`ident_regions()` 不再存在）——16 条静默截断是 C 的行为，不是 Rust 的。三个数字的完整口径（对象、常量、溢出行为）见 §4.5 的"落地区与交接容量"表 |
| 改型（C 有、形状变） | `param_buf`：`char[MULTIBOOT_PARAM_BUF_SIZE]` → `&'static [u8]`，上限仍按 1024 校验<br>`module_list[]` → `boot_modules: &'static [BootModule]` | |
| 改名 | `vir_kern_start` → `kern_virt_base` | （`freepde_start` 曾改名 `free_upper_idx`，字段删除时整体退场，见 §3.4） |
| 升为字段 | `kern_phys_base`<br>`kern_size` | C 里这两个值不是 `kinfo_t` 的字段，而是链接脚本导出的符号 `_kern_phys_base`、`_kern_size`（`_kern_phys_base = 0x00400000`、`_kern_size` 由链接布局算出；完整摘录见 §3.4）。Rust 把它们放进交接结构，来源改为 boot-shim 解析内核 ELF 的 PT_LOAD 段（§4.4 的字段表）。`kern_size` 用 `u64` 而不是 `usize`——wire 语义（wire = 跨镜像交接协议里按固定字节宽度写进 arena 的那份表示，与进程内结构相对）需要固定宽度，不随目标指针宽度变（32 位目标是 C 侧的，本仓重写目标全为 64 位，这里钉的是交接协议而非当前目标集） |
| 删除 | 22 个字段：<br>`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`<br>`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`<br>`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`<br>`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic`、`mmap_size`、`freepde_start`<br>`bootstrap_start`、`bootstrap_len`、`user_sp` | 逐个的去向见 §3.2 的字段去向表（`mmap_size` 由切片长度承载；`freepde_start`/`bootstrap_*` 的删除判据见 §3.4.1 与 §3.4.3） |
| 新增 | `reserved_regions`<br>`source_records`<br>`platform_sources`<br>`authority` | 各自来源见下；`source_records` 是 v1 source 记录，供启动期 `validate_requiring_sources`（strict 门）审计，`KernelInfo::validate`（非 strict）在 `source_records` 非空时也查“来源记录覆盖投影内存图”；v10 起 strict 门改为检查内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内（原来靠 `startup_objects` 的一条记录）。`authority` 是这份内存图的输入权威，见下表后的绑定说明 |

> **v10 删掉的四个成员（设计裁决）**：`user_sp`（每腿都填 `USER_STACK_TOP`，接收侧改为直接读该常量）、`kern_stack_top`（每腿都填 `kern_virt_base + kern_size`，改为 `KernelInfo::kern_stack_top()` 访问器在消费点派生）、`syscall_entry`（生产侧填 `kern_virt_base`、仓内零读者，真值来自 `syscall_entry_va()`），以及 `startup_objects` 分区（唯一读者是 strict 门的一条覆盖断言，现由内存图两表回答）。wire 信封 160 → 128 字节，报文末尾的 startup 分区（20 × 72 = 1440 B）退场，`WIRE_STARTUP_END`（30624）随之消失，八页 arena 的设计余量从 2144 B 变成 3584 B。`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收。

**权威与内存图绑定**。`authority` 记录这份内存图是从哪个输入对象投影出来的：`RamAuthority::DTB`（设备树）或 `RamAuthority::UEFI`（`GetMemoryMap` 快照）；`RamAuthority` 内部包一个 `InputObjectId`；`InputObjectId` 现在只有 DTB/UEFI 两个变体，`StartupObjects` 与 `startup_record()` 已随 v10 删除，wire 标签 2（输入对象）、6（kind）与 5（origin）只作保留位。它由构造点写入——wire 解码把 [`BootHandoffV1`](os/libs/minix-boot/src/handoff_v1.rs) 信封里的 `ram_authority` 复制进来：两个直核面在 `build_kernel_info` 调用点传 `RamAuthority::DTB`，两条 UEFI 腿发送侧在 `Serializer::new` 里声明 `InputObjectId::UefiMemoryMap`、接收侧由 `decode_envelope` 解码为 `RamAuthority::UEFI`；riscv64 U-Boot shim 发送侧声明 `InputObjectId::DtbBootInput`、接收侧解码为 `RamAuthority::DTB`（它从 DTB `/memory` 取表，没有 EBS 快照）。`validate_requiring_sources()` 因此不再收参数，直接读字段。改之前权威是调用方的另一个实参，`KernelInfo` 与权威在类型上可以任意配对；混合记录表里"DTB 记录 + 声明 UEFI 权威"这类错配只会在下一次覆盖审计时才暴露。把权威放进结构体没有让错误的构造变得不可能（构造点仍可传错），但它把"图和它的声明"变成同一个值，解码路径不再有两个可以接反的参数。

**先讲清前两格装的是什么：内存图交付成两张表**。`KernelInfo` 携带内存信息只用两个列表，形状固定，来源不固定：

| 列表 | 语义 | 内核侧怎么用 |
|---|---|---|
| `memmap` | 可用内存；物理内存管理器从这里分配帧，但列表**不保证已扣除全部占用区**。内核镜像在**两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）**与 U-Boot shim 腿的表内；两条 UEFI 腿在 A+D 的 A 步（读完 ELF、预订 LMA 跨度，见 §4.2.1 动作 2）用 `AllocateAddress` 把整段 `[kern_phys_base, +kern_size)` 预订成 `LOADER_DATA`，因此**不在 `memmap`（自由表）内**，而是分类落 `ReservedMapped` 时进入 `reserved_regions`；无 `WRITE_BACK` 时落 `Side`，不进这份表（§4.2.2）。模块同理：**DTB 来源腿**（两个直核面与 U-Boot shim）的 bump 页在表内，UEFI 腿是 `LOADER_DATA` 因而不在。分配前仍须扣掉占用区（§3.4 扣除表；`vm_handoff::classify`，UEFI 腿的内核行退化为空操作）。LMA/VMA 的定义见 §3.4/§2.4，`LOADER_DATA` 的定义见 §3.1，`conventional` 与 `WRITE_BACK` 的定义见 §4.2.2 的令牌表 | 内核不保留可变 memmap 副本（早期引导段已移除，内核不再保留可变 memmap 副本）；`vm_handoff::classify` 在交接时对 memmap 做扣除，产出 free list 与 deduction record |
| `reserved_regions` | 可用但不可分配。运行期经 Direct Map 能访问，但不得再分配出去；`KernelLayout::ident_regions()` 删除后，`map_kernel` 改收 `&KernelLayout`（两参），不再单列 identity 参数，VM 调用点同步，“只断言为空”的检查与参数一起消失 | Direct Map 覆盖的候选之一（[`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4） |

五条生产腿产出这两张表的方式不同（按来源分三组：UEFI 组两条、U-Boot 组一条、直核组两条；§3.6 把“产线”定义为链的别称，这里的“组”不是独立计数），形状相同：

- **两条 UEFI 链**：在 `ExitBootServices` 的配对循环里读 `GetMemoryMap`，按固件的类型标签分流（分流规则见 §4.2）。分类必须与退出共用同一个 MapKey，所以它只能发生在那一个循环里（`MapKey` 的定义、配对理由与重试语义见 §4.2）。
- **riscv64 的 U-Boot shim**：读 DTB `/memory` 的 `reg`（`DtbMemoryMap`，[`memmap_source.rs`](os/libs/minix-boot/src/memmap_source.rs)），再用固件常驻区上界 `KERN_PHYS_BASE` 削出保留段（OpenSBI 占 `[0x8000_0000, 0x8020_0000)`）。
- **两个直核面**：同一份 DTB 解析实现；aarch64 传 `fw_end = 0`，因为该形态的 DRAM 里没有固件常驻区——`fw_end=0` 只关闭"固件前缀裁剪"，DTB `/reserved-memory` 声明的区间仍进保留表（该形态典型为空，但机制上不保证）。

收敛点是**数据形状加两条产出入径**（直核腿共用 `build_kernel_info`，v1 腿共用 `BootStorage`/`BootReport`），不是行为多态。两条直核腿把两张表放进 [`fn build_kernel_info`](<os/boot-shim/src/lib.rs:fn build_kernel_info>) 的同样两个参数位；三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）把同两张表写进各自的 `BootStorage`，报告视图的字段形状与 `KernelInfo` 一致。消费者只认区间、不认来源。这里没有一条"内存图来源"的 trait 承担统一，原因不是懒：UEFI 那条腿的分类时机被固件协议绑住——MapKey 与 `ExitBootServices` 一一配对，缓冲区不足与 MapKey 失效还要重试——一个只返回两张表的签名携带不了这些约束。

**内存图生产的四层责任**：上面三组内存图来源各自产出同一形状的两张表，但"从固件原始字节到两张表"的路径要跨四层，每层有独立的责任边界，不得混谈：

1. **UEFI 固件事务层**：`GetMemoryMap` + `MapKey` + `ExitBootServices` 是一个原子事务——分类必须与退出共用同一个 MapKey，否则固件拒绝退出。这一层决定了"分类只能在配对循环里做"（UEFI 腿），不能用一个独立函数事后补救。
2. **描述符解析层**：固件给的是原始字节，要变成带类型的描述符（类型、物理起点、页数、属性），需要统一的解析器负责字段边界、对齐与溢出校验。UEFI 腿用自建解析器；DTB 腿用设备树 `/memory` 节点解析。这一层的产出是"类型化的原始描述符"，还没做任何政策判断。
3. **OS 内存语义分类层**：政策侧把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Excluded（不可触碰）、Special（延迟处理）四档；这是设计专项的分类（详见 [`01-design-note-memmap-consolidation.md`](01-design-note-memmap-consolidation.md) 的 v12 分类，文件位于 `rewrite-notes/01-stage-kernel/`）。**现行实现把 Special 展开成两档落点**：MMIO 类走 `DevicePath`，其余按属性与类型表走 `Side`（侧桶：既不进 free list 也不进保留表）。因此实现侧的完整落点是五类——可分配 / `ReservedMapped` / `Excluded` / `DevicePath` / `Side`，判定是五步（不是三条规则），逐步判据见 §4.2.2 的表。
4. **内核映射与分配层**：Direct Map 覆盖（`dm_coverage.rs`）与物理帧分配器（VM 的 PMM）是两个独立消费者，它们对"哪些内存可用"的合同与分类层的输出对接——可分配性与可访问性不能脱节（两合同分别见 §3.2 与 §4.6）。

这四层的边界不能混谈：把第 1 层的 MapKey 纪律带入第 3 层的政策讨论（"反正退出前不能改"），或者把第 4 层的 DM 覆盖要求塞进第 3 层的分类判据（"要映射所以得进保留表"），都会让错误的层次为另一个层次的约束背锅。

**"升为字段"行的一个设计细节：为什么三个布局数字由 shim 从 ELF 算出，而不是由调用者传入**。设想把接口改为 `prepare_boot(storage, kern_virt_base, kern_phys_base, kern_size, bump_pages)`。这三个数字必须与内核镜像的链接布局一致，而它们现在的来源是 ELF 的 PT_LOAD 段（`loader.rs:compute_kernel_layout` 取所有 PT_LOAD 的 `min(paddr)`、`min(vaddr)`、`max(paddr+memsz)-min(paddr)`）。改为调用者传参后，写错的地方不会被 `check-layout.sh` 拦住：那个脚本校验的是**镜像自身**（首段 VMA/LMA、段间平移一致、跨距对齐），不校验调用者手里的常量。错值进入 `arch_boot_impl` 之后，"把内核映射到高地址"那一步会按错误的 `kern_virt_base` 建映射，症状是开启分页后取指失败——一个"参数错了"的问题表现为"页表建错了"。让 shim 自己解析镜像，"数字与镜像一致"就不再是调用者的责任，而是唯一数据来源（镜像本身）的自然结果。**两个直核面是例外**：`bootface`/`bootface_a64` 用自己文件里的 `KERN_VIRT_BASE`/`KERN_PHYS_BASE`/`KERN_SIZE` 常量，不从 ELF 头解析；它们与镜像布局的一致性：三架构直核面的跨距都由 `check-layout.sh` 的 L9 对账——x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`，链接期另有双保险：三份 `.ld` 都断言 `__image_end - 首段 VMA <= 0x400000` 且该差值是 2MiB 的整数倍（aarch64/riscv64 减 `KERNEL_VIRT_BASE`，x86_64 减 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`）。源码注释记录过这类常量漂移导致高半只映射半个镜像、`kern_stack_top` 落进 `.bss` 的静默损坏。因此“三个数字来自镜像”只对三条 v1 发送腿成立，直核面是“常量 + 部分构建期对账”的另一条来源。

#### 3.2.2 旧字段去向、GET_KINFO 外部面、控制台与 param_buf

字段名以 §3.2 的“删除”行为准；本表只给逐个去向，不重复定义。**先分清两个计数**：这 22 个是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账（C 有、Rust 不再保留的全部字段）；而 §3.4.1/§3.4.3 与 §3.2 说的「字段删除」是一次具体裁决——从 `KernelInfo` 里删掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，使字段数由 16 变成 13、`authority` 加入后为 14。22 个字段的退场多数是换载体，少数是随读者消失而退役。多数：C 的 `kinfo` 是全局变量，编译期常量、内核自有状态、记账表都顺手堆在里面；Rust 的 `KernelInfo` 收窄为纯交接结构，只装引导侧必须告知、接收方无法自行导出的内容。于是职责归位成三类：**上浮为常量**——编译期已知的布局与规模事实（`nr_procs`/`nr_tasks`）与构建身份（`release[]`/`version[]`）；`user_end` 不属此类（Rust 未回传、无消费者，见下表）；**移交专门设施或记录结构**——内核自有的诊断与记账状态（`kmessages`、`do_serial_debug`/`serial_debug_baud`、`minix_panicing`、三个以 `_bytes` 结尾的记账字段）；**换表达不换信息**——内容改由 `KernelInfo` 现存字段承载或推导（`mbi` 的内容散布进各字段，`mods_with_kernel`/`kern_mod` 由 `kern_phys_base`/`kern_size` 直接表达，`boot_procs[]` 从 `boot_modules` 现算）。找不到新读者的不只 `mem_high_phys`（恒等映射只短暂存在、帧分配直接遍历 memmap，该标量随之退役）；`bootstrap_start/len` 与 `freepde_start` 也在该次删除中因无真实读者退场（§3.4.1/§3.4.3）。

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
| `mmap_size` | 未映射为 Rust 字段：C 的"遍历边界（最高占用下标加一）"由切片长度 `.len()` 直接承载——`memmap` 从定长数组 + 边界标量变成 `&'static [MemoryRegion]`，长度自带 | [`globals.rs`](os/kernel/src/globals.rs:const MEMMAP_REGION_STORE_LEN) 的容量常量与 §2.4 的边界语义 |
| `freepde_start` | 曾改型 `Option` 跨 handoff 携带，字段删除时整体退场（删除论证与报告位填 0 的去向见 §3.4） | [`misc.rs`](os/kernel/src/misc.rs) 的 `getinfo_kinfo` |
| `bootstrap_start` / `bootstrap_len` | 曾以 `(PhysBytes(0), 0)` + 硬断言携带，字段删除——误设回收区间的接口面随字段一起消失（论证、过度回收风险表与 wire 版本变更见 §3.4） | §3.4 |

C 的两个专用键在 Rust 侧的处置：`cttyline`/`cttybaud` 不再有专用字段，`normalize_param_buf` 把它们当普通 `key=value` 保留（能进 `param_buf`），但早期串口的选择由 `early_console` 的平台/编译配置固定，运行期不可调。


##### 3.2.2.2 GET_KINFO 收窄与外部面

**外部可见面的一条架构演进** `[ARCH: GET_KINFO 载荷收窄为五个 used 字段 + 两个 C 布局占位；其余字段由 GET_IMAGE/GET_MONPARAMS/GET_KMESSAGES 等分开提供]`：C 的 `GET_KINFO` 把整个 `struct kinfo` 拷给调用者（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的 `length = sizeof(struct kinfo)`）；Rust 只拷 48 字节的 `KinfoStruct`（布局：`freepde_start`/`user_sp`/`vir_kern_start` 三个 `i64` = 24 B，`nr_procs`/`nr_tasks` 两个 `i32` = 8 B，`release`/`version` 两个 `[u8; 6]` = 12 B，再加 4 B 对齐 padding，合计 48 B）（[`kinfo.rs`](os/libs/minix-types/src/types/kinfo.rs)），只含七个语义字段（外加 4 字节 `_padding` 把结构补齐到 48 字节）——其中**五个有 Rust userland 读者的语义字段**：`nr_procs`、`nr_tasks`、`release`、`version` 由 IS 读；`user_sp` 由 RS 的 exec 路径经 `GET_KINFO` 读 `KinfoStruct.user_sp`（offset 8，`os/servers/rs/src/trap_api.rs:223-230`、`:430`）。`KernelInfo.user_sp` 与 `KinfoStruct.user_sp` 同名不同物：前者是交接结构的字段，后者是 GET_KINFO 的布局位；v10 删除的是前者。另两个是保留的 C 布局位：`freepde_start` 恒填 0（Rust VM 不借槽，见 §3.4）、`vir_kern_start` 有取值来源但仓内零读者。C 侧至少四个活读取点：`dmp_kernel.c` 读四个字段，`procfs/root.c` 读 `nr_procs`/`nr_tasks`，`pci.c` 的 `complete_bars()` 读 `mem_high_phys`，VM 的 `main.c`/`pagetable.c`/`utility.c`/`region.c` 合计读 ≥11 个字段（`memmap`/`mmap_size`、`module_list`/`mods_with_kernel`/`kern_mod`、`boot_procs`、`user_sp`、`vir_kern_start`、`freepde_start`、三个 `_bytes` 等）。Rust 收窄的是**这条系统调用的载荷**，不是"C 只有这些读者"：C 侧其余字段由 `GET_IMAGE`/`GET_MONPARAMS`/`VmBootHandoff` 等另一通道承载，`mem_high_phys` 在 Rust 侧已不存在。

受影响的 C 字段——`memmap[]`/`mmap_size`、`module_list[]`/`boot_procs[]`、`param_buf`、`kmessages` 指针、`bootstrap_start/len` 等——不再经这条系统调用出现：模块表走 `GET_IMAGE`、`param_buf` 走 `GET_MONPARAMS`、`kmessages` 走 `GET_KMESSAGES`；内存账不走 GETINFO——它在 VM 启动前由内核写进固定地址的 boot handoff 页（`VmBootHandoff.free_regions` 加扣除记录），VM 在 `os/servers/vm/src/boot.rs` 读它。行为基线定为 Rust userland 的 `KinfoStruct`；按 C 全布局读被删字段的程序在本仓没有消费者。

##### 3.2.2.3 两种控制台与四个字段的补充

表中两个"控制台"落点名字相近、层次不同：`EarlyConsole`（[`early_console.rs`](os/plat/src/early_console.rs)）是硬件输出通道——`minix-plat` 里的跨架构 trait，每架构至少实现 `write_byte`（`init` 可选；x86-64 用它初始化 COM1），启动与诊断输出经它直写串口；`KernelConsole`（[`kmess.rs`](os/kernel/src/kmess.rs)）是内核诊断收口——对应 C 的 `kputc`，把流经的每个字节先记进 10000 字节环形缓冲（C `struct kmessages` 的重写）再转发给当前架构的 `EarlyConsole` 实现，环内容经 `GET_KMESSAGES` 快照供用户态读取。分工一句话：前者管"字节怎么落到硬件"，后者在其上叠加"内核诊断的记录"。

几处需要展开的字段，各自的来源与理由：

- `reserved_regions`：被分类器标成 `ReservedMapped` 的内存区间（`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`）；`Excluded`/`DevicePath`/`Side` 不进这份表。它曾装两种语义：其一（x86 内联窗口）是内核链进 boot-shim PE、在低物理地址执行时，VM 建进程页表必须逐段恒等复现的代码/GDT/IDT/TSS 窗口；其二（UEFI 链通用）是内核运行期经 Direct Map 访问的固件分配（loader 数据、内核栈一类不在 conventional 列表里的页），建立 DM 覆盖时必须列入候选（依据见 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs) 的 source 4）。x86_64 切独立镜像后第一种用途退场：`vm_handoff` 不再携带 identity 字段（v9 删除恒空的 `kern_ident`）；第二种用途保留，作为 Direct Map 覆盖的 source 4。各条腿怎么产出这份列表见 §3.2 的“五条生产腿产出这两张表”三条 bullet。
- `kern_stack_top`、`syscall_entry`：C 里这两个值通过链接期符号与 `prot_init()` 内部计算得到。v10（设计裁决）把它们（连同 `user_sp`）从交接面撤掉：三者都是两端可算的同一常量关系，跨镜像传输只是把恒等式抄一遍；现在消费点分别用 `KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size`）、`minix_types::USER_STACK_TOP`（每架构编译期常量）与 `syscall_entry_va()`。
- `platform_sources`：固件描述符来源列表（DTB / RSDP），按 boot-shim 的偏好排序，内核取第一个解析成功的源。每个 `PlatformDescSource` 是 16 B 句柄 `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`；`len = None` 表示生产者未记录 blob 长度（旧生产者、测试或读头失败）。wire 侧仍写 `u32`：`None` 编码为 0，解码时 0 还原为 `None`。`RSDP` 是 ACPI 根描述符指针，指向 RSDT/XSDT；`DTB` 是设备树 blob。解析归 `04-platform-discovery.md`，本篇只讲它怎么被放进结构体。
- `param_buf`：对应 C 的 `char param_buf[MULTIBOOT_PARAM_BUF_SIZE]`（1024，`MULTIBOOT_PARAM_BUF_SIZE` 定义在 [`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)）。它是 2.3 那张参数表在 Rust 侧的落点，生产侧由 `minix_boot::normalize_param_buf` 统一规范化：输入按 NUL 或 ASCII 空白分词（UEFI 记录以 NUL 分隔，DTB `bootargs` 以空格分隔），逐条要求可打印 ASCII 的 `key=value`；非 ASCII、无等号、空键与内部键（`ARCH`/`BOARD`）逐条拒绝；`ARCH` 由编译目标生成，`BOARD` 只有真实来源才生成（当前生产调用没有可靠来源，显式传 `None` 省略，不伪造——测试 `generates_arch_and_board_first` 传显式板名验证的正是"有来源才生成"这一支）。

  ##### 3.2.2.4 param_buf 的十条可观察分歧

与 C 的可观察分歧共十条，按三档术语纪律统一标注 `[ARCH: 参数表语义规约化；十条可观察分歧见下]`：

- 其一，重复键——C 的"已存在先删"只对首条记录生效（§2.3 的源码推演），但结果分两支：若重复键的首个副本是第一条记录，旧条被删、新值追加，**后写胜出、只剩一条**；若首个副本不是第一条，旧条留在原位、新值追加到缓冲末尾，账上两条而读取返回第一条。Rust 统一为首条生效、后者逐条拒绝，因此在"重复键是第一条"这一支上与 C 相反（C 后写胜出），属于新增的可观察分歧，不是"C 缺陷语义的载体重构"。
- 其二，命令行里的 `ARCH=`/`BOARD=` 词——C 不拒；若外部写的是小写 `arch=`/`board=`，首条时会被内核随后写入的同名键覆写，大写拼写则与 C 的生成键不同名、不会被覆写。Rust 一律拒绝外部输入覆盖内部键，检查大小写不敏感（小写 `arch=`/`board=` 同样被拒，`rejects_internal_key_override` 覆盖）。
- 其三，`BOARD`——C 无条件写一条记录，但读取返回首条匹配、可被外部同名键遮蔽，因此不保证生效（§2.3）；Rust 按来源生成、无来源则省略（上文）。
- 其四，容量耗尽——Rust 一条放不下就丢弃它及其后全部记录，C 只丢当前条，后面的词逐条再试——短的仍能收下，长的仍会被丢（`drops_tail_records_whole_at_capacity` 锁住前者）。
- 其五，内部键拼写——Rust 生成大写 `ARCH=`/`BOARD=`，C 写小写 `arch=`/`board=`。
- 其六，分隔符集合——Rust 按 NUL 与 ASCII 空白（含 tab/CR/LF）分词，C 只按空格，同一 `bootargs` 可得到不同条数（`splits_nul_and_whitespace_records` 锁住前者）。
- 其七，字符集——Rust 拒绝非 ASCII/不可打印记录，C 原样存（`rejects_malformed_and_non_ascii`）。
- 其八，空键——Rust 拒绝 `=value`，C 存成一条 `=value`。C 的删除循环在 `namelen = 0` 时 `strncmp(p, "", 0)` 恒真，`p[0] == '='` 可能命中的是**下一条**记录的等号，于是同名键删除会截断后续记录的值；这个副作用只在空键输入下出现，Rust 直接拒绝该输入，因此没有对应路径。
- 其九，`ARCH` 取值——C 写 `arch=i386`，Rust 写 `ARCH=x86_64`（编译目标）。
- 其十，记录顺序——C 先放命令行记录、后写内核生成键；Rust 生成键在前、外部记录按输入序在后。这十处都落在 `GET_MONPARAMS` 的 wire 上可观察；仓内消费者核查：Rust 侧 `os/kernel` 与 `minix-boot` 没有把 `BOARD`/`board` 当板卡身份读取的消费者——命中只在 `boot_params.rs` 的 `fn normalize_param_buf` 两处（生成内部 `BOARD=` 与拒绝外部同名键）以及测试字面量；C 内核的 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围。参数源分两类，按 §3.6 的三类链落到各腿：两条 UEFI 链读当前镜像的 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`；U-Boot shim 与两个直核面（按 §3.6 归 U-Boot 一族与直核面两类）读 DTB `/chosen/bootargs`。消费者 `GET_MONPARAMS` 仍按 C 的 1024 字节定长零填充；没有外部输入时缓冲里只有生成的那条 `ARCH=...`——与 C 的 `mb_set_param` 必写行为仅就 ARCH 一项同形。

#### 3.2.3 进程内结构不做 ABI 与 arena wire 协议

`KernelInfo`、`MemoryRegion`、`BootModule` 都不使用 `#[repr(C)]`：字段顺序与切片内部 `(ptr, len)` 的顺序都不是契约。`PlatformDescSource` 是例外：它用 `#[repr(C)]` 只为钉住 16 B 句柄形状，字段顺序仍不是跨镜像 ABI（wire 用 `WirePlatformDescSource`）。它们只作为进程内结构使用（直核面、QEMU 测试内核，以及 UEFI 接收侧解码后在进程内构造的值；发送侧只构造 `BootReport` 借用视图），不承担跨镜像布局契约。跨镜像三腿（两条 UEFI 产线 + riscv64 U-Boot shim）改用独立协议 `BootHandoffV1`：标量全部写成固定宽度，载荷放进一个八页 arena（arena：boot-shim 预留的一块定长物理内存，里面摆着 v1 交接协议的信封与各载荷分区，分区布局见本节末）的固定分区，描述符只记 `byte_len`——地址由 arena 基址加协议偏移推导，`(ptr, len)` 不再上线。协议有两层版本号，读时不要混：**协议身份**恒为 v1（`BOOT_HANDOFF_V1_MAGIC`/名字里的 v1，指"第一代交接协议"这个族）；**信封版本号**（`BOOT_HANDOFF_V1_VERSION`，当前 10）随信封或 arena 的 wire 布局变更递增、收发双方精确比对——本篇出现的"版本 8→9"与"9→10"都是后者（"wire 版本"是同一计数器的另一个叫法；7→8、8→9 与 9→10 都指它）。本篇不带限定词的 `v9`/`v10` 一律指 wire 信封版本；`VmBootHandoff` 的版本写 `handoff vN`。同样，`source` 有两个限定用法：`source_records`/source 分区指出内存图来源记录（provenance），`platform_sources`/平台源指 DTB/RSDP 描述符来源；单说 source 时按上下文取前者，不用裸 source 指平台源。

```rust
// 代码引用（略去字段文档注释；ram_authority 行的编注为读者说明、不在源码；末尾 arena 分区注释为改述，源文件为英文原文且其 ARENA_CAP 文档另见），os/libs/minix-boot/src/handoff_v1.rs:struct BootHandoffV1。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoffV1 {
    pub magic: u32,
    pub version: u32,
    pub size: u32,
    pub ram_authority: u32,   // 〔编注〕内存图的权威来源：0 = boot DTB、1 = UEFI 内存图；接收方只认这两个值，声明了权威却无对应 source 记录则 fail-closed（§3.2 的 source 规范集合与 §4.4 的 `validate_requiring_sources`）
    pub arena_len: u64,
    pub kern_virt_base: WireVirtAddr,
    pub kern_phys_base: WirePhysAddr,
    pub kern_size: u64,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
    pub memmap_byte_len: u64,
    pub reserved_regions_byte_len: u64,
    pub boot_modules_byte_len: u64,
    pub platform_sources_byte_len: u64,
    pub param_buf_byte_len: u64,
    pub sources_byte_len: u64,
}

// 八页 arena（32768 B）自低往高：信封（容量上限 256 B）→ wire 分区（memmap/reserved/modules/
// platform/param 五段，至 WIRE_END=7872）→ source 分区（至 29184）。余 3584 B 是
// `ARENA_CAP` 与 29184 的差，编译期断言钉住。发送侧的语义表住进 shim 自己的
// `BootStorage`，arena 里只剩 wire 这一份表示。
pub const ARENA_CAP: usize = 8 * 4096;
```

arena 的分区是固定偏移：信封住首部，其后依次是内存图、保留区、模块、平台源、参数字节的 wire 形式；v9 之后 arena 里**没有**第二份语义表——发送侧的分类结果同时写进它自己的 `BootStorage`（调用方持有的定长数组）和 wire；UEFI 腿的 source 分区例外，只写 wire，`BootStorage` 的 source 切片为空；U-Boot shim 的 DTB source 记录仍经 `fill_source_records` 写 `BootStorage` 的 40 槽，再序列化进 wire（见 §3.7/§4.5）。接收方再按 `byte_len` 把 wire 复制进自己的 `.bss` 落地区。这样 arena 只有一份权威表示，发送侧构造进程内视图时借用 `BootReport<'a>`（`BootStorage` 的视图），不再用 `'static` 切片指向 arena。平台源 **wire 记录** 是 16 B 的 `kind u32 | len u32 | addr u64`：`len` 复用旧 `reserved` 槽位，旧生产者写 0 表示未知、新读者回退单片；因此该记录仍是 16 B，v10 的版本号变化只来自 §3.2 列出的四个成员。接收方先校验 `magic`、`version`、`size` 与 `arena_len`，再按各 `byte_len` 把六个分区复制进自己的 `.bss` 落地区；越界、不能整除或容量不足都在构造任何引用之前失败。运行期字段（根页表物理地址与 bump 三件套）在 shim 建好根之后原地补进信封，因此三腿都只需传 arena 基址一个数（入口寄存器见 §4.1）。

#### 3.2.4 深拷不变量与真机探针

**不做深拷、把 `KernelInfo` 的切片按原指针存进内核的全局槽会怎样**。两条 UEFI 产线的六个分区都由 boot-shim 在启动服务池或 arena 里产出；无论哪一条，接收方都必须在首次进程根切换之前把值搬进自己的 `.bss`——arena 只在恒等窗口存活，池页在历史真机上还出现过零化。真机记录给了这一现象的两组读数（无归档日志，仅为当时观察；可靠性低于可复跑探针），零化时点不同：`res=127 zeroed=127`（`res` 是到达分类点的 `reserved_regions` 条目数，`zeroed` 是其中因长度读回零而被拒的条数——127 对 127，也就是全部读成了零），以及"15 条 conventional 条目在建立 Direct Map 覆盖时还完好、到 VM 交接分类点长度全读成 0"。后者的后果不是某个字段错，而是 VM 的自由内存清单塌成零条——VM 在启动参数校验处 panic，而"内存图丢了"这件事在 panic 输出里看不到。两组读数指向同一个设计结论（值必须在入口尽早深拷——这是一条预防性不变量，它的依据是"零化发生过且无法在故障输出里归因"，不是"某条机制已由实验证实"）；机制上有两条候选：**候选①（页表不重放）**——源码注释给过的一条是“内核自己的页表不重放这些堆页的 1:1 映射”（[`globals.rs`](os/kernel/src/globals.rs:static RESERVED_REGION_STORE) 的容量注释）；**候选②（复用清零）**——“建立 Direct Map 覆盖时还完好”这组读数更像的“被当空闲复用后清零”。两条候选各带可检验的推论——"页表不重放"说与切表时点相关、"复用清零"说与快照/复用的时序相关——本篇没有归档的探针读数可重放（`test-kernel-map` 的 N-5 探针是 print-only，不参与 PASS 判据）；候选①只按源码注释与人工观察保留为“未排除”，候选②同样未排除（读数与结论见 §5.3）。这条不变量与 §2.4.2.3 的“跳板生命周期”同级：都是**观测/预防性**前提，不是“某条机制已由实验证实”的结论；把它写成已证设计会高估证据强度。



### 3.3 两张内存表与来源记录（I2）

两张表不是同一种数据：`memmap` 是“可分配”的候选，`reserved_regions` 是“已占用/不可分配”的候选；同一个物理字节可以在不同阶段从一张表转到另一张表，但不能同时被两张表当成自己的。

`SourceRecord` 给每个投影字节记来源：`kind`（`/memory`、MRB（device-tree memory reservation block，设备树内存保留块）、`/reserved-memory`、UEFI 描述符等）、`origin`（节点/条目/描述符下标）、`range`、`input_object`、`attribute`。`RamAuthority` 把“这份内存图由哪个输入对象声明”从 `InputObjectId` 里分出来：只有 DTB 与 UEFI 快照两种取值，`StartupObjects` 已随 v10 删除。构造 `KernelInfo` 时 `authority` 与 `source_records` 一起传入；`validate_requiring_sources` 要求 authority 对应的记录覆盖投影。

覆盖审计是 I2 的判据：free 表的每个字节必须能被 `kind = Memory` 的记录解释；reserved 表的每个字节必须能被 reservation 类记录解释。说不出来源的字节不是“忽略”，是 fail-closed。C 侧没有这个结构：C 的 `memmap` 条目只有基址、长度与一个固定为 `AVAILABLE` 的类型标签（`multiboot_memory_map_t.mm_type`），没有“谁声明了这段”的来源记录，来源隐含在“引导器给的”这个前提里；Rust 把它显式化：跨镜像腿的接收侧无法再问引导器，直核腿虽然同镜像，也保留同一份记录以复用同一覆盖审计。

### 3.4 bootstrap 区间与分配器换手

#### 3.4.1 `bootstrap_start/len`：从可回收区间到字段删除

C 的这两个字段装的是链接脚本导出的一小段内核镜像内区域的边界（2.3 已看到赋值）。这段区域的性质写在链接脚本里：

```ld
/*
 * 节选（保留段划分与偏移量相关的行），minix3/minix/kernel/arch/i386/kernel.lds。
 */
_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);
...
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
	.bss ALIGN(4096)  : AT(ADDR(.bss) - _kern_offset) { *(.bss* COMMON)
		__k_unpaged__kern_size = . - _kern_vir_base;
		_kern_size = __k_unpaged__kern_size;
	}
	_end = .;
}
```

逐条解码：

- 前三个赋值就是 §2.5 里 `kernbase`/`kernsize` 的来源，也是 02 篇讲 `AT()` 的两条基址。
- `_kern_size` 不是字面量而是布局算出的符号：`.bss` 收尾处 `__k_unpaged__kern_size = . - _kern_vir_base`（整幅镜像的虚拟跨度），随后 `_kern_size = __k_unpaged__kern_size` 把它导出成 §2.5 引用的那个名字——它的值随每次构建的段布局而定，没有固定数字。
- `. = _kern_phys_base;` 让第一段的位置计数器落在物理基址上；`unpaged` 三个段（来自文件名匹配 `unpaged_*.o` 的目标文件）放在这里，它们的**执行地址等于装载地址**（没有 `AT()` 偏移）。
- `__k_unpaged__kern_unpaged_start/end` 把这段夹出边界，交给 `get_parameters` 填进 `bootstrap_start/len`。用一对起止符号而不是单一起点，是因为下游要的是半开区间 `[start, end)` 的长度——回收接口 `add_memmap` 收 `(addr, len)`，只有起点算不出长度。C 代码里的引用名没有双前缀（`_kern_unpaged_start`，§2.3 的 `get_parameters` 引文可核对）；构建期 `objcopy --prefix-symbols=__k_unpaged_` 把它改写成 `__k_unpaged__kern_unpaged_start`，与链接脚本定义相遇——`k_initial_stktop` 的构建期前缀规则是同一机制的另一例，其落点见 02 篇 §2.4。
- `. += _kern_offset;` 之后的所有段才带偏移（`AT(ADDR(...) - _kern_offset)` 表示"执行地址在高处、装载地址在低处"）。两个地址必须分开的理由：引导器只按装载地址把字节写进物理内存，而分页开启后 CPU 按执行地址取指；`AT()` 就是链接期把同一个段的两个地址钉在一起的方式。
- 于是这段 `unpaged` 区域是"分页开启前执行、分页开启后不再需要"的代码与数据（它的执行地址等于物理地址，靠恒等映射就能访问）：`kinfo` 已在 `kmain` 开头拷进 `.usermapped` 副本，活跃的 `pagedir[]`/静态页表池在高半 paged `pg_utils.o` 副本里，段内留下的是 unpaged 副本。C 在 `kmain` 后段（`system_init` 之后、`bsp_finish_booting` 之前——精确锚点见 §2.4.2 的"还引导段"行）把它 `add_memmap` 归还给通用分配器，且归还发生在 `kernel_may_alloc` 写 0 之前，不撞 §2.4 那组断言。

Rust 侧没有这样一段，这里要分清两件事：**链接布局**上生产内核镜像（两条 UEFI 产线 + U-Boot 接收镜像 + riscv64 直核生产镜像）的所有段都在高半虚拟地址（C 的 `unpaged` 段是唯一"执行地址 = 物理地址"的例外；QEMU 直核测试内核按 VMA=LMA 平铺，见 02 篇的 4.5 节，不在这条陈述内）；

**实际执行地址**上，两条 UEFI 链的 boot-shim 建好根、开分页后跳进独立 `kernel.elf` 的高半入口，内核从第一条指令起就在高半（§3.6）；riscv64 的自引导链则起步于物理视图（medany——RISC-V 默认的 PC 相对寻址代码模型，代码按"装在哪就按哪解释地址"的方式编译——的平移视图，见 02 篇的 4.5 节），高半地址要等开分页之后才被解引用。

**判据结论**：当前所有生产链接布局（两条 UEFI 链、U-Boot 接收镜像、riscv64 生产直核镜像，均为高半 VMA）与 VMA=LMA 的 QEMU 测试内核都不存在"分页开启前执行、之后可归还"的**内核段**。内联形态已退役，不在本条判据范围。正面写法：没有一段是"在链接布局上可单独界定、且分页后不再需要"的；`bootface` 代码虽然只在分页前执行，但它是内核镜像的一部分、没有独立链接边界，不能单独归还。因此这个字段对在 Rust 侧没有语义内容——它随后的处置（先写死为 `(0, 0)`、最终整个删除）都从这一句来。

这个字段对在 Rust 侧的处置走过两个阶段，最后停在**删除**：先是把 `(0, 0)` 写死并配一条硬断言（"必须为 0"），字段删除后离开 `KernelInfo`，wire 版本 7→8：本字段对释放 16 字节，同版本删除的 `free_upper_idx` 再释放 8 字节，合计 24 字节，envelope 从 184 缩到 160（该字段的删除论证见本节下文的过度回收风险表；`free_upper_idx` 的删除论证见 §3.4）。为什么"写死 + 断言"还不够、要连字段一起删：`bootstrap_start/len` 表达的其实是**一个布尔事实**（"存在分页开启前专用、之后可回收的段"——Rust 侧恒为否），却占两个整数、两个 builder 参数和 16 字节 wire；所有生产调用点都传 `(PhysBytes(0), 0)`，唯一的"读者"是那条断言自己。更重要的是保留字段就保留了一个误设的接口面——`validate` 的断言只能把非零值拦在 `kmain` 入口，拦不住"合法类型、错误语义"的值进入类型系统。误设的后果是本篇推演过的真实事故来源：

这不是纯假设：曾有一次把 `bootstrap_len` 填成 `kern_phys_base.0`。先把时刻说准：误设发生在 `build_kernel_info` 的调用点，那一步只是把 `[0, kern_phys_base)` **加进空闲清单**，不写任何内存；真正会覆盖这些区间的是之后的取页者（内核的 `boot_alloc`、以及 VM 接手后的分配器）。因此下表是**反事实推演**：它列的是"如果这块区间之后被分配出去，谁会先读到坏数据"，不是同一时刻四行同时发生。下表列的是那次误设会扫进的同一区间 `[0, kern_phys_base)` 里的四类内容（不是互斥分区）：

| 若 `bootstrap_len` 被设成 `kern_phys_base.0`（下表以 riscv64 链为例：`kern_phys_base = 0x80200000`；OpenSBI 行有地址锚点，其余三行是当时部署的观察，具体地址未记录） | 那块内存实际存着什么 | 误回收后先坏在哪（按"取页时谁还活着"读） |
|---|---|---|
| OpenSBI 区间 `[0x8000_0000, 0x8020_0000)` | SBI 运行时服务（console、timer、IPI）的代码与跳板 | 内核接手后仍会调 SBI；下一次 SBI 调用（早期控制台输出、定时器）进入被覆盖的地址。这是四行里唯一"取页时仍然活着"的读者 |
| DTB 所在区间 | 设备树 blob；boot-shim 用它建内存表、读 `/chosen/bootargs`，并把物理地址作为 `PlatformDescSource` 传给内核 | 内核在 kmain 之后解析 DTB 时读到被改写的属性，硬件发现失败（归 04 篇）；若 DTB 在取页前已被解析完，则这一行不构成先坏点 |
| U-Boot 镜像与 `BootFileTable` | 生产链上 `fatload` 预载的文件；表本身是 shim 镜像 `.bss` 的一部分（`build_shim_table` 构建），不是预载物 | **不构成先坏点**：shim 交出控制权后不再从表里取文件；这段区间被覆盖的后果只是"若回跳/重入 shim 会读到垃圾"，正常启动路径不会发生 |
| boot-shim 自身 | shim 的代码与栈（仅在 shim 镜像落在 `[0, kern_phys_base)` 的部署里） | **不构成先坏点**：取页发生在内核接手之后，那时 shim 已不再执行；直接取指失败只在回跳/重入时发生。若某次部署里 shim 仍在执行期间就被取页，才属于这一行的原描述 |

这张表里的每一行都落在"某块仍被使用的固件/引导代码内存被写进空闲清单"，而四者的共同点是：它们都不在 boot-shim 报告的可用内存里，而是被"从 0 开始、长度到内核物理基址"这个区间扫进去的。诚实边界：OpenSBI 行有 `[0x8000_0000, 0x8020_0000)` 这个地址锚点；DTB/U-Boot/shim 三行的具体物理地址当时没有记录，只能按"那次部署把三者放在内核物理基址以下"的观察来读，不要把它们当成可复算的地址证据。表中"不构成先坏点"的两行仍然留在表里，是因为它们说明误回收的**区间范围**；它们不承担"四类内容都会先坏"的结论。

这张表也解释了那次险情的机制：把"上界地址"当"区间长度"，`add_memmap(0, kern_phys_base)` 会把上表所列的全部固件结构扫进"可回收"———早期 `build_kernel_info` 的调用点（riscv64 链）曾有一条注释（字段删除时随参数一起删除），原文（源码里分两行，这里合并）把自己的旧取值记为险情来源：`the previous \`PhysBytes(0) + len = kern_phys_base.0\` was a dangerous over-reclaim that swept up OpenSBI firmware, DTB, U-Boot image, and the boot-shim itself`（x86_64 链上同一个误设的区间是 `[0, 0x200000)`，即 [`x86_64.ld`](os/kernel-image/x86_64.ld) 的 `KERNEL_PHYS_BASE`）；现仓在 `opensbi_helpers.rs` 的测试模块保留一句"结构上不可能"的回归说明；当前 `kmain` 的早期校验段已不再要求 `bootstrap_len == 0`（该字段已不存在），早期实现的取证注释仍提到该字段。那次险情靠人工改值解决；字段删除之后，"提供一个可误设的回收区间"这个接口面整体消失，险情在类型上不可复发。C 侧的归还机制照旧运转（`add_memmap` 对零长请求静默吸收的语义，2.4 已见），本篇不再为它携带任何 Rust 侧对应物。
#### 3.4.2 引导期页表页分配器的换手

§3.6 的五条生产腿与交接形态定下来之后，这一节看引导期页表页分配器怎么在这两个镜像之间换手。C 的**页表页**来源是 `alloc_pagetable` 的 6 页静态池，用完 panic（02 篇的 2.4 节）；`pg_alloc_page` 从可用内存记录的最高条目尾部取页，服务的是 4KB 被映射数据页，不是页表页（§2.4.2/02 篇的 2.4 节）。页表分配策略与"页表操作"写在同一份文件里，页表代码直接调用分配函数。

**把 C 的静态页表池原样搬到 64 位会怎样**（保留"6 页，用完 panic"）。`PG_PAGETABLES = 6` 是两级页表的预算：32 位下 `pg_map` 的一个 4 MB 目录项只需要一个页表页，6 页对常见的小范围映射够用；跨 4 MB 边界或分散映射会多占页，这一点在下面的 64 位推演里放大。4 级页表下一次 4KB 映射要穿过 PML4（x86-64 四级页表的顶层，§3.4.3 详述）→ PDPT → PD → PT 四级，其中下面三级都可能需要新建页表页；实际需要的页数随地址分布变化（映射分散时每个 PML4 槽都要一条新链）。固定 6 页按每个 PML4 槽最多 3 页算只够两个槽，第三个槽（第二次跨 PML4 边界）就会耗尽，表现为 `panic("no more pagetables")` 落在启动中途——而"6 页"这个数字在 64 位下没有任何依据。

Rust 侧把**页表页**分配抽出来（`pt_alloc.rs` 的 [`fn register`](<os/arch/src/arch/pt_alloc.rs:fn register>)、[`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>)）；4KB 数据页的运行期分配归 VM 物理分配器，不在这个槽里：`Paging`/`HugePages` 的实现只调用一个没有参数的函数指针槽，谁来提供页由"注册"决定。

```rust
// 代码引用（块内中文注释为译述；英文 SAFETY 注释有节略，原文见源码；source-symbol: register/alloc_pt_page），os/arch/src/arch/pt_alloc.rs。
type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;
...
/// Registered allocator, or 0 when unregistered.
static PT_ALLOC: AtomicUsize = AtomicUsize::new(0);
/// Registered free hook, or 0 when unregistered.
static PT_FREE: AtomicUsize = AtomicUsize::new(0);

pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    // 写一次：第二次注册是接线错误，release 也 fail-closed。
    PT_ALLOC
        .compare_exchange(0, alloc_fn as usize, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|_| panic!("pt_alloc::register called twice"));
}

pub fn is_registered() -> bool {
    PT_ALLOC.load(Ordering::Acquire) != 0
}

#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    let raw = PT_ALLOC.load(Ordering::Acquire);
    assert!(raw != 0, "pt_alloc::alloc_pt_page called before register");
    // SAFETY: `register()` stored a `PtAllocFn` value in this slot and the
    // slot is never replaced; `usize` and the function pointer have the same
    // width on every supported 64-bit target.
    let alloc_fn: PtAllocFn = unsafe { core::mem::transmute(raw) };
    alloc_fn()
}
```

boot 阶段的实现是一个 bump 分配器（`boot_alloc.rs` 的 [`BootAlloc::alloc`](os/kernel/src/boot_alloc.rs:fn alloc)）：

```rust
// 代码引用（块内中文注释为译述；英文原注释见源码；source-symbol: BootAlloc::alloc），os/kernel/src/boot_alloc.rs:fn alloc。
    pub fn alloc(&self) -> Result<(PhysBytes, VirBytes), PageTableError> {
        // 从本实例的原子状态重建共享的 BumpRegion 值，跑公共算法，再把游标写回。
        let mut region = BumpRegion::resumed(
            self.base.load(Ordering::Acquire),
            self.next.load(Ordering::Acquire),
            self.end.load(Ordering::Acquire),
        );
        let Some(pa) = region.alloc_pages(1) else {
            return Err(PageTableError::AllocationFailed);
        };
        self.next.store(region.next(), Ordering::Release);
        // Zero-fill the page. In test mode, skip the write (no real memory).
        #[cfg(not(test))]
        // SAFETY: pa is a valid, 4KB-aligned physical address returned by the
        // bump allocator. Under identity mapping (boot stage), VA=PA, so the
        // pointer is valid. The page is not aliased by any other live reference.
        unsafe { core::ptr::write_bytes(pa as *mut u64, 0, 512) };
        Ok((PhysBytes(pa), VirBytes(pa)))
    }
```

换手由一次初始化与三次注册组成（下表三行：第 1 行是初始化、不注册；第 2 行里两个二进制各注册一次，共两次；第 3 行一次），各有锚点：

| 时刻 | 谁写这个槽 | 代码 | 依据 |
|---|---|---|---|
| shim 进程内、进入内核镜像之前 | 初始化 bump 池（**不写注册槽**） | `boot_alloc::init_boot_pt_alloc(bump_base, bump_end);`（U-Boot 腿写成 `result.bump.start()`/`result.bump.end()`） | [`uefi_helpers.rs`](os/boot-shim/src/uefi_helpers.rs)、[`uboot_main.rs`](os/boot-shim/src/uboot_main.rs) |
| shim 进程内 Step 0 与**接收镜像**的 `resume_additive`（同一 helper `boot_validate_and_prepare`） | 只有"未注册"时才写——每个二进制各有一份 `PT_ALLOC` 静态（`AtomicUsize`，0 表示未注册），接收镜像的这份在它自己的 `.bss` 里首次注册。shim 侧不直接调 `register`，而是在 `boot_validate_and_prepare` 内经 `is_registered()` 守卫间接注册；两次注册不冲突：UEFI 腿写的是 boot-shim 的那份、接收镜像写的是自己的那份，互相独立 | `if !pt_alloc::is_registered() { pt_alloc::register(boot_alloc::boot_pt_alloc); }` | [`boot_validate_and_prepare`](os/kernel/src/lib.rs:fn boot_validate_and_prepare) |
| VM 初始化 | VM 在自己的地址空间里注册（并注册归还钩子） | `if !minix_arch::pt_alloc::is_registered() { minix_arch::pt_alloc::register(crate::alloc_page::vm_pt_alloc); }` 与 `if !minix_arch::pt_alloc::is_free_registered() { minix_arch::pt_alloc::register_free(crate::alloc_page::vm_pt_free); }` | [`vm_server.rs`](os/servers/vm/src/vm_server.rs) |

内核侧与 VM 侧的写者都用 `is_registered()`（或 `is_free_registered()`）先自检；shim 侧不直接调用 `register`，`init_boot_pt_alloc` 只设定 bump 区间，`boot_validate_and_prepare` 在 shim 进程里经 `is_registered()` 守卫间接注册。这个槽是“写一次”的，重写是 bug，因此没有“后注册者覆盖前者”的语义。VM 侧的分配函数把页表页从 VM 的物理内存分配器取出，归还函数把它还回同一个池。C 这里有两套池，不能混：内核自举页表的 6 页静态池（`i386/pg_utils.c` 的 `alloc_pagetable`，发完 panic、从不归还）与 VM 侧用户页表的池（`servers/vm/pagetable.c` 的 `pt_free` 把页还回 VM 物理分配器所用的同一池，经 `free_mem`）；Rust 的注册槽同时服务内核自举页表（`boot_pt_alloc`，经 `boot_validate_and_prepare` 守卫注册）与 VM 页表（`vm_pt_alloc`）；`register_free` 只对应 VM 侧池。两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`VmBootAllocator` 是 VM 侧的启动帧分配器：从每个池区间末端向下消耗，方向与从起点向上的 `BootAlloc` 相反）（`vm_handoff.rs` 的 [`fn build_vm_handoff`](<os/kernel/src/vm_handoff.rs:fn build_vm_handoff>) 调用的 [`fn classify`](<os/kernel/src/vm_handoff.rs:fn classify>)）；引导期建表之后的 Direct Map 覆盖建立则在 [`dm_coverage.rs`](os/kernel/src/dm_coverage.rs)：

```rust
// 节选（略去行号引用；kernel image 一行加了 A+D 之后的归属说明，模块行加了按腿归属的说明，C 对应关系一行按 §2.4.2 的指针改述，见上文），os/kernel/src/vm_handoff.rs 的扣除区间表头与 bump 行。
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base/kern_size`（UEFI 腿在 A+D 的 A 步已预订为 `LOADER_DATA`，本行对 free list 退化为空操作；自引导腿与 U-Boot shim 腿仍是实际扣除） |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs (deduction record) | `KernelInfo.boot_modules`
//!   — **every** blob including VM's own on the legs whose blobs live in the
//!   free list (self-boot / U-Boot); UEFI blobs are `LOADER_DATA`, so this row
//!   is a no-op there. On the self-boot / U-Boot legs VM's segment frames come
//!   from this free list before VM starts, so reclaiming its blob corrupts VM
//!   before entry. |
//!
//! Note the two module lists are different on purpose: the deduction record
//! keeps every blob, while the handoff module list (`VmBootHandoff.modules`,
//! built by [`build_vm_handoff`]) excludes VM as a Rust-side choice — C's
//! `kinfo.module_list[]` keeps VM and zeroes its entry after reclaim.
```

```rust
// 代码引用（节选，实际调用段），os/kernel/src/vm_handoff.rs:fn classify。
    // 〔编注〕五个扣除来源统一交给 for_each_live_bootstrap；bump 整段的 push 在
    // 该函数的 `if let Some((base, end)) = boot_bump` 分支里（以下为源码调用段）。
    for_each_live_bootstrap(
        kernel_info.kern_phys_base,
        kernel_info.kern_size,
        kernel_info.boot_modules,
        Some(root_paddr),
        boot_alloc_region(),
        &suffixes[..suffix_n],
        |base, len| record.push(base, len),
    );
```

表头那句注释给出口径："consumed pages and remainder are both kernel-reserved"——扣的是**整段**，不是已用部分。理由在本节末的反向推演里。表里还有一行与 C 的差异需要说明：C 在 VM 模块的字节被复制进地址空间之后把它回收（就是 2.4.2 表里的"还 VM 镜像"那一行），而**实现**把**所有**启动模块的字节区间都算作内核保留。表头把两份清单分开陈述：扣除记录含 VM 自己；交接模块清单（`VmBootHandoff.modules`）排除 VM 是 Rust 侧的选择（C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零，见 §2.4.2）。理由是启动顺序里的一个 Rust 侧前提：**模块字节留在 blob 原地，VM 启动那一刻才被搬进自己的地址空间**——C 侧不同，C 的注释写明回收发生在 "just copied into existence"（复制完成后），所以 C 能在复制后归还而后续装载只伤到无人再读的 blob 原件。Rust 侧复制被推迟到 VM 启动，清单就必须从任何搬运发生之前就把全部模块 blob（含 VM 自己的）划出：内核的 boot-exec——`kmain` 的引导执行阶段，逐个 `exec` 启动镜像里的服务进程——在 VM 服务起来之前只装载 VM 一个服务器（`EntrySpec::loaded`）；其余启动服务是 `EntrySpec::DEFERRED`，由 RS 运行期 EXEC 装载——它们的段帧同样取自这份自由清单。若按 C 的时点归还 VM 的 blob，后续装载与 EXEC 的帧分配可能盖掉尚待搬运的 VM 镜像字节。

**改为按"已用部分"扣除会怎样**（用 `boot_alloc_used_bytes()` 而不是 `boot_alloc_region()`）。设 bump 区间是 `[0x1E00000, 0x2200000)`（1024 页 = `0x400000` 字节，与 `prepare_boot(storage, 1024)` 的预算同量级），boot 阶段用掉前 100 页（`100 × 0x1000 = 0x64000` 字节），于是游标停在 `0x1E00000 + 0x64000 = 0x1E64000`，而 VM 的账上会剩下 `[0x1E64000, 0x2200000)` 可用。下一次 `map_huge` 分配中间页表页时（调用点可以是 `arch_boot_impl` 的 Step 2/2c 设备窗口，或 Step 4 建立 DM 覆盖）`BumpRegion::alloc_pages(1)` 从 `0x1E64000` 切走一页并把它写零（`write_bytes(pa as *mut u64, 0, 512)`）。这些调用点都在 VM 首次发布自由页之前，所以这次写零不会覆盖 VM 已分配的页；真正的问题是**账本边界**：按已用部分扣除把 bump 区间的剩余部分交给 VM，而 bootstrap 路径仍持有 `boot_alloc_region()`，在 `is_registered()` 为真（`PT_ALLOC != 0`）时仍可能从同一区间取页（例如未来新增的启动期消费者），两者对“这块内存归谁”没有共同的截止点。整段扣除是一个保守的截止点：bump 池整体不进入 VM 自由清单，代价是**VM 发布前未被接收侧取用的尾部页**（已被接收侧用作页表页的部分仍在使用；整段扣除只是不让 VM 碰这段区间），收益是不需要证明“所有 boot 消费者都在 VM 发布前结束”。
#### 3.4.3 `free_upper_idx` 与页目录槽位：字段、借用链与 64 位做法

页表页由谁发说完，剩下的是页目录槽位的账——C 的 `freepde_start` 是 02 篇的 4.3 节 那条借槽链的起点：`memory_init` 把它自增两次填进 `freepdes[]`，`createpde` 用这两个槽位在当前页表里开临时窗口，`virtual_copy_f` 借窗口做跨地址空间拷贝。这套机制的三个前提（完整陈述见 02 篇的 4.3 节）在 64 位侧以三种方式不再必要：① 内核不再借当前页表的槽位，Direct Map 窗口是固定虚拟基址到物理地址的线性映射，不需要“空槽”；② Direct Map 不依赖“当前 CR3/当前 satp 指向哪张根”这个可借用槽位——三架构都实现 `Paging::inherit_supervisor_half`，把源根的高半条目复制进新进程根：aarch64 的 L0 上半 256–511 项就是 TTBR1 区（[`arm64/paging.rs`](os/arch/src/arm64/paging.rs:fn inherit_supervisor_half)），x86-64 复制高半 PML4 槽位（[`x86_64/paging.rs`](os/arch/src/x86_64/paging.rs:fn inherit_supervisor_half)），riscv64 复制 Sv39 L2 的 256–511 项、satp 切换整根但每张进程根都携带内核半区（[`riscv64/paging.rs`](os/arch/src/riscv64/paging.rs:fn inherit_supervisor_half)）；③ 没有借槽动作，就没有归还义务——跨地址空间拷贝由内核用 Direct Map + 软件页表走查完成（`SYS_VIRCOPY`/`cross_space_copy`），不再借当前页表的槽位。

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

#### 3.5.1 取件抽象：一条普通函数，差异收在 `FileLoader`

本节起反复用"腿"：**一条腿 = 一个完整启动路径（一个引导面 + 它交付的内核镜像）**；腿、链、形态、交接路径的计数口径不同，对照表在 §3.6。

C 里"同一串函数名、两套实现"的机制见 §2.7：两种架构各自写 `head.S` 与 `pre_init.c`，靠命名与语义约定保持一致，编译期不会检查它们是否真的对应。

Rust 侧起初把固件相关的部分收进一个只有**一个方法**的 trait（`minix-boot::BootShim`）。它现在已不存在，理由是可核的三条事实：唯一实现是 U-Boot 腿的 `RiscvUbootBootShim`（在移除前的 一次提交 上 `grep -rn "impl BootShim" os/` 只有一处；当前树已零命中）；没有 `dyn BootShim`，`prepare_boot` 还是关联函数，不参与对象分派；测试接缝实际由 `FileLoader` 提供，不是它。单实现 trait 既不是接缝，也不是已经验证过的扩展点，留着只会让"换固件"的承诺看起来比证据强。

现在 U-Boot 腿的入口是一条普通函数：[`fn prepare_boot`](<os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot>)，由 [`fn uboot_main`](<os/boot-shim/src/uboot_main.rs:fn uboot_main>) 直接调用，签名是 `prepare_boot(&mut BootStorage, bump_pages) -> BootPrepareResult<'a>`。UEFI 生产入口 `prepare_and_enter_kernel_image` 仍是独立入口（§3.6）；两个直核面复用 opensbi_helpers 的库函数，同样不经过它。未来若真出现第二条固件腿，等第二个实现落地时再按两个真实实现提炼接口，而不是先猜一个。

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

```rust
// 代码引用（trait 级文档注释整段略去，块内不保留省略号），os/boot-shim/src/loader.rs:trait FileLoader。
pub trait FileLoader {
    /// Read a file by path. Returns `None` if the file does not exist or
    /// cannot be read.
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>>;

    /// Read a file that must exist. Default panics with a useful message.
    fn read_required<'a>(&'a self, path: &str) -> FileBytes<'a> {
        match self.read(path) {
            Some(d) => d,
            None => panic!("boot-shim: required file not found: {}", path),
        }
    }
}
```

`read_required` 是**内核 ELF 取件**的致命落点：它把"文件不存在"从 `None` 变成一个带路径的 panic。启动模块走 §4.3 的 `read + unwrap_or_else(panic!)`，是第二个致命落点，错误消息格式与 `read_required` 不同；两者都是 fail-closed，`read` 本身只返回 `Option`，不决定致命性。两个实现各自只提供 `read`：UEFI 走 `SimpleFileSystem`（并把正斜杠转成 UEFI 要求的分隔符），U-Boot 走预先落在 RAM 里的 `BootFileTable`。除此之外的装载逻辑（解析、段拷贝、模块页分配、结构体构造）在两个 boot-shim 实现族之间只有一份；两个直核面共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`，`bootface.rs`/`bootface_a64.rs` 只剩本架构常量、入口横幅与各自的 console（下文展开）。两个实现都在 `read` 里只处理字节来源，错误折成 `Option` 交给各自的调用方。

```rust
// 代码引用（保留读取路径；注释有删节；一处英文注释译为中文），os/boot-shim/src/uefi_helpers.rs。
impl FileLoader for UefiFileLoader {
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>> {
        let fs_proto = boot::get_image_file_system(boot::image_handle()).ok()?;
        let mut fs = FileSystem::new(fs_proto);
        let uefi_path = path.replace('/', "\\");
        let cs = uefi::CString16::try_from(uefi_path.as_str()).ok()?;
        // 〔编注〕固件协议返回新缓冲：这条腿拥有它。
        fs.read(cs.as_ref()).ok().map(FileBytes::Owned)
    }
}
```

U-Boot 侧的实现见 §4.3 的 `entry_path_eq` 与 `UbootFileLoader`；两条 `read` 的差别在字节来源，U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度（kernel 槽 2 MiB、普通模块槽 1 MiB、`mfs` 专用 16 MiB；超过槽容量时打印 `truncating` 后按槽容量写入长度，即**截断后继续**，不是停机），loader 的二次修剪对已在槽内的输入是 no-op；非 ELF 或畸形输入由 `elf_file_end` 返回 `None`，调用方保留整槽长度，不做修剪；`read_required` 与全部装载逻辑完全共用。范围要说准：这里的“只有一份”指**两个 boot-shim 实现族之间**；两个直核面不走 `FileLoader`，但共用 `bootface_common` 的 `place_modules` 与 `run_self_boot`；入口壳只负责常量、横幅与 console 选择。

`bootface_common` 不是只把函数搬了个家，它把两个直核面共同的模块放置约束集中到一处：`NAME_SLOT = 16`、`NAME_POOL` 是 12 个 16 B 槽，`place_modules` 检查条目数 ≥ 12（一条断言；有效条目不足时 fail-closed 返回 `None`）、每槽 basename 与 `NAME_POOL[i]` 相等、长度非零、源区间整段落在 memmap 的单个区间内、源区间不与 bump 池相交、目标页不落内核跨度；通过后才按槽分配 bump 页、拷贝、清页尾、写入名字池。另有编译期 `const _` 把 `NAME_POOL[i]` 与 `boot_shim::loader::MODULE_NAMES[i]` 逐字节比较，两个直核腿的名字顺序漂移会直接变成编译错误。`PlacementPool` 存在的理由不是打包参数：五个相邻的裸 `u64`（名字池地址、bump 起点、bump 终点、内核起点、内核终点）互换后仍能编译并产生“看似合理”的放置，结构体的命名字段把这类调用点错误挡在编译期。两条调用腿的差异只落在 `PlacementPool`（名字池地址、bump 起止、内核跨度）与 `alloc_pages`/`say`/`say_hex` 回调上：riscv64 用 `0x8200_0000..0x8400_0000`，aarch64 用 `0x4400_0000..0x4600_0000`，容量同为 32 MiB。模块放置之外的流程（DTB 校验、memmap 收集、表址闸、bump 取页、平台源、参数缓冲、`KernelInfo` 组装）也在同一份 `run_self_boot` 里，两壳不再各存一份顺序相同的代码。

去掉 trait 后，有代码或计数证据的收益有两条：

- **可测试性来自 `FileLoader`**（代码证据，测试形态成立）：`FileLoader` 是 trait——[`loader.rs`](os/boot-shim/src/loader.rs) 的 tests 里有内存表实现 `MockLoader`（`impl FileLoader for MockLoader`），涉及装载的单元测试都经由它喂文件字节。
- **条件编译的落点收敛**（按构建形态成立）：**固件选择**相关的 `cfg` 只出现在 [`lib.rs`](os/boot-shim/src/lib.rs) 的模块与再导出声明处（7 处 `#[cfg(feature ...)]` 属性：6 条 `uefi` 加 1 条 `opensbi`）。另有一组 `qemu-test-kernels` gate（10 处）在 `uefi_helpers.rs`，只门控测试内核辅助物（`UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`、`classify_map_bytes_into_test_tables`、`exit_boot_services_and_classify`）；生产构建不链接它们。因此准确说法是"固件分支不在业务逻辑里"，不是"整个 crate 只有 7 处 feature 属性"。

另外两条只算单腿事实或设计承诺，不并入上面的收益：

- **入口形状固定**（只在 U-Boot 生产腿成立）：`prepare_boot` 的 `storage`/`bump_pages` 语义与 `BootPrepareResult<'a>` 的返回形状由签名钉住；但"统一"的对象只有这一条腿，UEFI 与直核面各有自己的入口，不能把它说成跨固件统一。
- **换固件不动内核**（设计承诺，观察，尚无自动化反例）：新增一种固件形态，改动是 boot-shim 侧的新入口或共享库调用，不需要预先实现一个 trait；`kernel.elf` 不必重新构建（两者分属 `os/kernel-image` 与 `os/boot-shim` 两个产物，这条分界的后果在 §3.6 与 02 篇的 4.5 节）。边界要说准：boot-shim 屏蔽的是**固件差异**，板级差异仍落到内核侧的平台发现（[`minix-platform`](os/libs/minix-platform/src/) 的 `PlatformDesc` 与内核侧平台源解析），不能把它说成"新板子只需写 boot-shim"。

这里故意不列"类型安全"与"封装"——这两条常被记在 trait 头上，实际来自 feature 门控：`uefi_helpers` 与 `opensbi_helpers` 各由 [`lib.rs`](os/boot-shim/src/lib.rs) 的 `#[cfg(feature = ...)]` 门控，开启哪个 feature 就只有哪个固件类型存在，**写错固件名本来就编译不过**。UEFI 生产入口也没有借 trait 做抽象：[`fn main`](<os/boot-shim/src/main.rs:fn main>) 按具体类型写——`prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`。这不是把固件写死在产品里：boot-shim 这个可执行文件只在 UEFI 形态下构建（bin 的 `required-features = ["fw-uefi-image"]` 隐含 `uefi`），固件选择发生在"构建哪个二进制"，不是运行期分派；opensbi 形态的生产路径是 riscv64 U-Boot shim（§3.1.1/§3.6）的 `opensbi_helpers::prepare_boot`；`kernel-image` 自引导面是辅助/嵌入式形态，复用同一组库函数但不经这份 main.rs。

**OpenSBI 路径的两种角色要分清**（接线状态与覆盖脚本的权威表在 §3.1.1 接线状态表，这里只讲与引导入口有关的部分；本篇用“OpenSBI 路径”指经 `opensbi_helpers` 的腿，§4.4 的字段表里特指 U-Boot shim 腿，不把两个直核面算进来）：U-Boot 路径复用 boot-shim 库根的 `BootStorage`/`BootReport` 与 opensbi_helpers 的 `BootFileTable`、`bump_alloc_pages`，并调用 `prepare_boot`；`kernel-image` 的直核自举面是另一条机器状态，不经它。memmap 源统一下沉的裁决不合并引导形态层——独立 shim 与 bootface 自举对应不同机器状态，`prepare_boot` 只承担前者的固件差异。
#### 3.5.2 crate 依赖方向

C 的引导链同样不止一个二进制：BIOS 场景有 bootblock 与 boot2 两级引导器（[`boot2.c`](minix3/sys/arch/i386/stand/boot/boot2.c)），GRUB 场景由 multiboot 引导器代劳——引导器与内核不是链接成一个二进制，而是各自链接出独立的可执行文件，运行期只隔一条 multiboot 协议（寄存器递 `mbi` 指针）。内核侧的引导准备 [`head.S`](minix3/minix/kernel/arch/i386/head.S) 与 [`pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 则编在内核镜像之内，`pre_init` 填的 `kinfo` 本来就是内核自己的全局变量。所以 C 无需回答"引导层与内核谁依赖谁"：引导器与内核的边界由协议承载，协议两侧没有编译期关系。

Rust 侧的问题形态不同：boot-shim 独立成产物，却要与内核共享类型定义（`KernelInfo` 与交接的返回形状），这份复用由 crate 依赖表达——"谁依赖谁"第一次成为必须显式回答的构建问题，且答案必须单向：

```
boot-shim ──┬─→ minix-boot   （BootPrepareResult、BootStorage/BootReport、KernelInfo）
            ├─→ minix-arch   （仅 boot_dm_admissible_end；不调用 pt_alloc/Paging）
            └─→ minix-kernel （enter_kernel_image_v1、init_boot_pt_alloc）
minix-kernel ─┬─→ minix-arch
              ├─→ minix-boot
              └─→ minix-plat、minix-platform、minix-types

minix-boot ──→ minix-types

kernel-image ─┬─→ minix-kernel （arch_boot / arch_boot_resume_v1；直核面调 arch_boot）
              ├─→ minix-boot
              ├─→ minix-arch
              ├─→ minix-plat
              ┄→ boot-shim（仅 riscv64/aarch64 target；依赖的是库，不是引导器产物）
```

（图只画主边；条件依赖与 leaf crates 以下方四份依赖表为准，其中 `kernel-image ┄→ boot-shim` 仅 riscv64/aarch64 target 生效。`boot-shim → minix-kernel` 的调用面只有 `enter_kernel_image_v1` 与 `boot_alloc::init_boot_pt_alloc`；`arch_boot` 是 `kernel-image` 两个直核面的入口，不由 boot-shim 调用，图上的括号已按这个分工标注。两个名字看起来相近，但指的不是同一个东西：**boot-shim** 是可执行体 crate，**minix-boot** 是共享库 crate——标题里"内核不依赖 boot-shim"说的是不依赖前者这个*可执行体*；图中 `minix-kernel → minix-boot` 是对*库*的依赖，二者不矛盾。）

依据是四份依赖表（boot-shim / kernel / minix-boot / kernel-image 四个 manifest；命令：`sed -n '/^\[dependencies\]/,/^\[/p' os/kernel/Cargo.toml`（输出中无 `boot-shim` 也无 `uefi`，即证否定；全仓四 manifest 的依赖表见下方四行（`os/kernel` 的 `bitflags` 从略））。文中另提到的 `minix-elf` 是 boot-shim 的普通依赖，不单列成表）：

- `os/boot-shim/Cargo.toml` 的 `[dependencies]` 里有 `minix-types`、`minix-boot`、`minix-arch`、`minix-kernel`、`minix-plat`；`minix-elf`（共享的 ELF 解析器）；非可选的 `fdt`（U-Boot 腿解 DTB）；`uefi = "0.33"` 与 `uefi-raw = "0.9"` 挂在可选的 `dep:uefi`/`dep:uefi-raw` 上，由 `uefi` feature 一并打开。
- `os/kernel/Cargo.toml` 的依赖是 `minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`——**没有 boot-shim，也没有 uefi**。两个名字相近的 crate 分工不同：`minix-plat` 是板级/机器相关的实现层（早期控制台、中断控制器一类与具体硬件打交道的代码，[`early_console.rs`](os/plat/src/early_console.rs)），`minix-platform` 是平台描述符的解析与分发层（`os/libs/minix-platform/`，面向 ACPI/DTB 这类固件描述）。
- `os/libs/minix-boot/Cargo.toml` 只依赖 `minix-types` 与 `fdt`。
- `os/kernel-image/Cargo.toml` 依赖 `minix-kernel`、`minix-boot`、`minix-arch`、`minix-plat`、`minix-types`。Cargo 的 `[target.'cfg(target_arch = "…")'.dependencies]` 语法允许一条依赖只在指定 target 架构下生效，kernel-image 用它给 riscv64 与 aarch64 各加了两条：`boot-shim`（只开 `opensbi` feature，`uefi` 不开）与 `fdt`（解 DTB 用）——只有以这两个架构为 target 构建时它们才生效。
  - 符合本项目设计目标的方式（独立引导器 + 内核本体两个产物）里，"内核不依赖 boot-shim"对 `minix-kernel` 与 `boot-shim` 成立——引导器依赖内核、内核不回头依赖引导器。这一分工的接线状态见 §3.1.1 的接线状态表（两条 UEFI 产线已跳独立镜像；riscv64 的门链里，U-Boot shim 的六路标门 `test-riscv64-uboot-shim.sh` 注册在 `os/qemu-tests/run_all.sh`；kernel-image 的直核自引导门 `test-kernel-image-riscv64-bootface.sh` 是手工门，不在 `run_all.sh` 中——接线归属以该表为准）。
  - `kernel-image` 的情况不同：它是自引导镜像（引导面 `bootface` 长在自己体内，§3.1 例外条款），riscv64/aarch64 目标下按上面那两条 target 表反向依赖 boot-shim 的**库**（取件表、memmap 源等函数）——依赖的是库而不是引导器产物，单向边界不因此破例（道理见问题二）。这张 target 表就是终态：memmap 源下沉到 `minix-boot` 之后，反向依赖的仍是库、不是引导器产物，依赖方向没有变化。

这条单向边界回答的是"共享类型住哪"，与"哪种固件能编译"是两个独立的问题，别混在一起：

**问题一：共享类型为什么住 `minix-boot`**。真正**跨两侧**引用的是 `KernelInfo`（内核经 `arch_boot` 读它，直核面与接收侧构造它）与 `BootReport`（`minix-kernel` 的 `enter_kernel_image_v1`/`prepare_kernel_image_v1` 以 `&minix_boot::BootReport<'_>` 为参数）；`BootStorage`/`BootPrepareResult` 是引导侧的接口类型。放在同一库是为了让这些签名不把 boot-shim 可执行体或固件实现拖进内核依赖面，而不是因为“内核完全不引用引导侧类型”。若 `KernelInfo` 定义住在引导层，内核引用它就得依赖 boot-shim——反向依赖、且把固件实现拖进内核依赖面；若住在内核里，共享库 `minix-boot` 就得反向依赖 `minix-kernel`，与已存在的 `minix-kernel → minix-boot` 成环（boot-shim → minix-kernel 本身不是环——依赖图里本来就有，boot-shim 调 `enter_kernel_image_v1`）。所以这层定义下沉到不含 UEFI/SBI 固件实现、但共享参数归一化与 DTB 解析等实现的 `minix-boot`（依赖表是 `minix-types` 与 `fdt`），谁依赖它都不会被拖进固件代码。这条边界不是风格洁癖：`os/kernel/Cargo.toml` 的依赖表里没有 boot-shim、也没有 uefi，用上面那条 `sed` 就能一次验证。

**问题二（完整形态论证归 §3.1 例外条款与 §3.6；此处只给与依赖方向有关的结论）：riscv64/aarch64 上的引导面为什么长成"内核镜像自举"**。背景见 §3.1 表后的引用段：RISC-V 与 ARM 的 `-kernel` 极简装机（QEMU `-kernel` 模拟的是真实硬件的这种装载方式）下，下一级镜像直接进 RAM、固件跳转，固件与内核之间没有"装载与被装载"的分界，独立 boot-shim 没有存在位置。于是两个架构的引导准备各自由 `kernel-image` 自带的引导面承担——riscv64 是 `bootface`（[`bootface.rs`](os/kernel-image/src/bootface.rs)），aarch64 是 `bootface_a64`（[`bootface_a64.rs`](os/kernel-image/src/bootface_a64.rs)），流程由 `bootface_common::run_self_boot` 承载（DTB 解内存 → 取件表 → bump 池（bump 池 = 一段只前进不回收的线性分配器：从区间起点逐页发放，游标只增不减，耗尽即拒绝）→ 模块 → `KernelInfo` → 同一个 `arch_boot`）。两者都复用 boot-shim 的**库函数**，但不是独立引导器产物；这一形态的固件对话走 DTB 与 PSCI/DMB（DMB = 数据内存屏障）一类的裸协议，`kernel-image` 与 `minix-kernel` 两个 crate 都不依赖 `uefi`，单向边界原样成立。它的定位是**平行的装机面**：与 UEFI 独立 shim 共享同一套取件库；内核引导收敛层按腿分两处（直核 `arch_boot_finish`、跨镜像 `arch_boot_resume_v1`，最终同落 `jump_to_kmain`，见 §3.6/02 篇的 4.5 节）——统一发生在层内（类型、取件库、收敛层），引导形态层不造伪统一（memmap 源统一下沉的裁决，三形态对照见 §3.6）。

**如果让 `minix-kernel` 直接依赖 boot-shim 会怎样**（动机多半是"想复用 `BootPrepareResult` 的定义"）：复用类型走 `minix-boot` 即可，不必依赖引导层（问题一已答）。剩下的否决理由是本节标题本身：**内核不该认识固件形态**——屏蔽固件差异正是 boot-shim（或 bootface）存在的意义。"riscv64 编译不过"（UEFI 目标只有 x86_64/aarch64/i686，riscv64 暂不支持）不构成独立论据，换 opensbi feature 就能过；否决理由从头到尾只有依赖方向一条。
### 3.6 五条生产腿与发送侧（接缝定义）

本节用“五条生产腿”一个主计数；其余计法的换算表在节末。

C 里没有"交接"这个问题：内核是唯一的二进制，`pre_init` 返回的指针就是同一个地址空间里的地址，`head.S` 的 `call kmain` 是同一次调用。

（下文反复用到 AArch64 的 TTBR0/TTBR1：它们是两张页表基址寄存器，分别承载低半与高半地址的翻译表；切换地址空间只换 TTBR0，TTBR1 在上下文切换中不被替换。）

Rust 侧的收敛点分两处：直核家族收敛在 `arch_boot_finish`（`arch_boot_impl` 的调用在其体内；三个 cfg 分支的通用实现锚点见 [`fn arch_boot_impl`](<os/kernel/src/lib.rs:fn arch_boot_impl>)），跨镜像三腿的接收侧收敛在 `arch_boot_resume_v1` → `resume_additive`。三条对外入口分别是 `arch_boot`（直核/测试）、`enter_kernel_image_v1`（跨镜像 v1 发送，三条腿：两条 UEFI 链 + riscv64 U-Boot shim；它不在本镜像内收敛，终点是跳独立镜像的入口指令）与 `arch_boot_resume_v1`（接收）：

```rust
// 代码引用（保留签名与首段注释），os/kernel/src/lib.rs:fn arch_boot_finish（**直核家族**——arch_boot 三个 cfg 分支——共用的尾部；跨镜像的发送/接收两端都不经它：发送侧尾部是 `br {entry}` 跳独立镜像，接收侧走 `resume_additive`）。
fn arch_boot_finish<A, H>(kernel_info: &KernelInfo, root_page: PhysBytes) -> !
where
    A: BootArch,
    H: crate::boot::HigherHalf,
{
    // Fail-fast before any page table is built: a malformed handoff must not
    // reach the mapping machinery and only surface later at kmain. The
    // pre-table gate skips the boot_modules non-empty check because QEMU
    // bootstrap test kernels legally carry no init-server image.
    kernel_info.validate_before_tables();
    kernel_info.check_boot_postconditions();
    let kinfo = store_kernel_info(kernel_info);
    register_panic_diagnostic();
    arch_boot_impl::<A>(kinfo, root_page);
    // ……（省略 SAFETY 注释，见源文件）……
    unsafe { H::jump_to_kmain(kinfo) }
}
```

x86_64 的 `arch_boot` 只做诊断打印并调用 `arch_boot_finish::<X86_64BootArch, X86_64HigherHalf>`；`arch_boot_impl` 的调用在这个 helper 内，不在 `arch_boot` 的函数体里。

签名里的两个参数就是交接的全部内容：一份借用的启动信息（`&KernelInfo`，入口立即深拷）与一个根页表页的物理地址。返回值是 `!`——它不返回。

跨镜像交接的产线上多一层（x86_64、aarch64 与 riscv64 U-Boot shim 三条 v1 发送腿同走 `lib.rs` 的 [`fn enter_kernel_image_v1`](<os/kernel/src/lib.rs:fn enter_kernel_image_v1>)；TTBR0/TTBR1 是 aarch64 侧的细节）：内核以独立 ELF 的形式存在于高半（aarch64 侧经 TTBR1），boot-shim 先分配一块八页 arena 并把 v1 协议写进去，然后建表、开分页、把运行期字段补进 arena，最后**绝对跳转**到入口；内存图来源分两路：两条 UEFI 腿在 `ExitBootServices` 的配对循环里分类最终快照，riscv64 U-Boot shim 没有 EBS，直接从 DTB `/memory` 取表；入口寄存器只带 arena 基址；三条腿各自用哪个寄存器、x86_64 的 trampoline 如何转交，见 §4.1 的入口约定。

「虚拟入口」与「物理 arena 基址」这两个值在跳转那一跳上同处。此前它们是一对相邻的裸 `u64`（`-> (u64, u64)` 与 `jump_kernel_image_v1(entry: u64, arena_pa: u64)`），交换后仍能编译：跳转目标会变成 arena 的物理地址，分页刚开后第一次取指就故障，而 `scause`/`CR2` 指向的地址与真实原因无关。现在改由一个值对象按字段名承载。`PreparedJump`、`prepare_kernel_image_v1`、`jump_kernel_image_v1` 都是 crate 内私有实现，唯一公开入口是 `enter_kernel_image_v1`；它不是 wire 类型，只在本仓两处相邻调用之间传递。

```rust
// 代码引用（保留稳定的公共入口、窗口构造、建根调用与 aarch64 跳转臂；错误消息、SAFETY 注释与 x86/riscv 跳转臂有省略；`PreparedJump` 保留定义原文，略去 `#[cfg]` 与 `#[derive]` 两行），os/kernel/src/lib.rs:fn enter_kernel_image_v1 + fn prepare_kernel_image_v1 + fn jump_kernel_image_v1 + struct PreparedJump。
struct PreparedJump {
    /// Virtual entry of the standalone image's `_start`.
    pub entry: VirBytes,
    /// Physical base of the handoff arena (the `x0`/`rbx`/`a0` handoff value).
    pub arena_pa: PhysBytes,
}
pub fn enter_kernel_image_v1(
    arena_pa: PhysBytes,
    report: &minix_boot::BootReport<'_>,
    root_page: PhysBytes,
    shim_windows: &[PhysRange],
) -> ! {
    let jump = prepare_kernel_image_v1(arena_pa, report, root_page, shim_windows);
    jump_kernel_image_v1(jump)
}

fn prepare_kernel_image_v1(
    arena_pa: PhysBytes,
    report: &minix_boot::BootReport<'_>,
    root_page: PhysBytes,
    shim_windows: &[PhysRange],
) -> PreparedJump {
    let arena_pa = arena_pa.0;
    // ……（riscv64 cfg 别名、x86 cli、Step 0、policy 选择见源文件）……
    let (bump_base, bump_end) = boot_alloc::boot_alloc_region()
        .expect("enter_kernel_image_v1: bump region not registered");
    let mut windows = BootWindows::new();
    for range in shim_windows {
        windows.push(range.start().0, range.end().0);
    }
    push_common_boot_windows(
        &mut windows,
        report.kern_phys_base.0,
        report.kern_size,
        root_page.0,
        Some((bump_base, bump_end)),
        report.boot_modules,
        report.platform_sources,
    );
    windows.push(
        arena_pa,
        arena_pa
            .checked_add(ARENA_CAP as u64)
            .expect("enter_kernel_image_v1: arena overflows u64"),
    );
    // x86 的 AP 实模式跳板窗口；aarch64/riscv64 没有这个窗口。
    #[cfg(target_arch = "x86_64")]
    windows.push(0, DEV_PIECE);
    let windows = windows.check(support.phys_addr_limit());
    build_bootstrap_root_and_enable::<BootArchImpl>(root_page, &policy.kernel(), windows);
    let bump_next = boot_alloc::boot_alloc_next();
    // ……（arena 映射与 patch_runtime_fields_in 见源文件）……
    let entry = VirBytes(core::hint::black_box(report.kern_virt_base.0));
    PreparedJump {
        entry,
        arena_pa: PhysBytes(arena_pa),
    }
}

fn jump_kernel_image_v1(jump: PreparedJump) -> ! {
    let PreparedJump {
        entry: VirBytes(entry),
        arena_pa: PhysBytes(arena_pa),
    } = jump;
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "isb",
            "br {entry}",
            in("x0") arena_pa,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
    // ……（x86/riscv64 跳转臂见源文件）……
}
```

三处具体事实：

- 交接载荷带 `bump_next`。原因在 §4.6 与 `BootAlloc::resume` 的文档里：两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次。两条 UEFI 腿的 arena 由 UEFI `AllocatePages` 单独分配、不占 bump 池，因此没有"载荷页"要跳过；U-Boot shim 腿的 arena 由 `bump_alloc_pages` 从 U-Boot 可用内存中模块落点之后的区域另行分配（它与 `BootAlloc` 是两个独立游标，见 §4.2.1 的"勿混"说明）；其起点不低于 `bump_end`，因此在 1024 页 `boot_alloc_region()` 区间**之外**；它的身份随 startup 分区退场（§3.2.1），arena 的归属由下面的生命周期合同单独成立；这里的“same pool”指同一片 U-Boot 可用物理内存，不是同一个游标：shim 用 `bump_alloc_pages` 从模块落点之后发 arena，接收镜像的 `BootAlloc` 只续用页表子区间，两者游标独立。扣除表没有单独排除它，而是按生命周期合同处理：解码是 arena 的最后一次读，VM 自由页在那之后才发布，因此这些帧可以重新发给 VM（§4.6 的 poison 测试钉住这条时序）。
- `in("x0") arena_pa`：arena 基址绑在固定的第一参数寄存器上，而跳转目标放在任意寄存器。代码注释给出了理由——若在 asm 块里写隐式 `mov x0, ...`，寄存器分配器可能把 `entry` 也分配到 `x0`，两条约束顺序不定。
- 接收侧先把六个分区直接解码进 `store_kernel_info` 的六个 `.bss` 落地区（`resume_additive` 的 store 因源与目标同址成为 no-op），再只做 additive 阶段（接收壳的分派次序见 02 篇的 4.5 节，这里不重复）：

```rust
// 代码引用（略去首 3 行摘要与结尾 10 行——结尾那段不是设计史注记，而是 TTBR1 高半免疫 `switch_address_space` 的现时理由与加性序列枚举 `validate（内含 pt_alloc 注册）→ DM plan → record root → store → register_panic_diagnostic → establish DM → jump`（英文摘要把 register 与 validate 并列是分组说明，不是逐字次序）；保留段逐字；末行在源文件里与后续内容同行，此处截断至句号），
// os/kernel/src/lib.rs:fn resume_additive 的文档注释（调用序合并后，加性序列的
// 设计注记挂在通用体上；arch_boot_resume_high_half 退化为带路标的委托壳）。
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

引文里的 `establish the direct map` 就是 02 篇的 4.3 节 的 Step 4；本篇 §4.8 只保留启动期概念入口，完整机制归独立内存管理主题；这里只保留引文中的时序位置。`new_from_page` 的真实签名是 `new_from_page(root_page, channel: PteChannel)`：根的清零与后续 PTE 访问必须走调用方声明的写视图——bootstrap 建根用 `PteChannel::Identity`（Direct Map 尚未建立），运行期进程根用 `KernelDm`，VM 自有根用 `VmDm`。引文省略了该参数，但“清零活动根”的结论不变：三个通道只是同一根的不同写入口。

riscv64 上是**直核自引导形态**的出生地：内核镜像自己就是引导体——在没有 U-Boot 与文件系统的 `-kernel` 场景里，入口是 [`main.rs`](os/kernel-image/src/main.rs) 按架构分道的 `rust_image_main`，riscv64 的实现是 [`fn bootface`](<os/kernel-image/src/bootface.rs:fn bootface>)（aarch64 的对应面是 `bootface_a64.rs` 的 [`fn bootface_a64`](<os/kernel-image/src/bootface_a64.rs:fn bootface_a64>)，只在该架构构建时编译），从设备树解析内存、按 `BootFileTable` 取模块——模块来源只有表这一条，表的**地址**只有 DTB `/chosen` 的 `opensbi,boot-file-table` 属性一条通道（仓库内六个脚本——`test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-kernel-image-riscv64-bootface`、`test-riscv64-boot-full`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64`——都用 `fdtput` 注入表址）。此前的备用 `a2` 参数通道在启动裁决中删除（仓内从未有生产者：QEMU/OpenSBI 只约定 `a0`/`a1`）；表址先过 `in_memmap_range` 再过魔数，失败即 halt。细节在 `bootface.rs` 的入口段，本篇不展开。

U-Boot shim 腿自建表、不经过此通道——它把表与模块序列化进 arena，交给接收镜像的 `arch_boot_resume_v1`；调 `arch_boot` 的是两个直核面。U-Boot shim 路径落地后也补齐了**跨镜像跳转形态**的完整链：`riscv64-uboot-shim`（物理链接，VMA=LMA，自带 bootstrap 栈）吃下 U-Boot `bootelf -p` 的 `(extra_argc, extra_argv)` 参数通道，把内核 ELF 与 12 个引导模块准备成 `BootHandoffV1` arena，DTB 以 `PlatformDescSource` 指针随信封交付，交给配套特性 `fw-riscv64-uboot` 构建的 `kernel-image` 接收镜像走 `arch_boot_resume_v1`——接收侧与 aarch64 同构，复用同一个加性恢复序列，不重建根表。riscv64 的 `jump_to_kmain` 因此带一个执行位置探针：直核面仍在低地址恒等窗口里执行，medany 代码模型的 PC 相对 `la` 解析出的是装载地址，要加上镜像的链接↔装载偏移；接收侧已被 shim 跳到高半链接地址，`la` 解析的已是真值，偏移不得再加（两节分工：02 篇的 4.3 节 讲 C 侧的两份 `pagedir` 换根，本节讲 Rust 侧为何禁止重建活动根；不合并）——同一段 asm 用 `auipc` 读当前 PC 与 `kern_virt_base` 比较后分道。五条内存来源（两条 UEFI 链的分类结果、U-Boot shim、两个直核面）交付的形状统一为两张表，收敛点与各腿的产出方式见 §3.2；引导形态层本身不合并。

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
>
> “产线”是链的别称，不单独计数。两个“5”成员不同：架构×链五行含 aarch64 直核面、不含直载测试链；真机门链五条含直载测试链、不含 aarch64 直核面。§4.1 的阶段总览表只按 UEFI / 非 UEFI 两列讲阶段，不是独立计法。
### 3.7 所有权与校验门链（I3/I4）

I3 与 I4 在实现上是两条相邻的检查：**载荷先落到自己的存储，再发布；校验在消费（建表/映射/分配）之前完成**。注意“消费前”不等于“发布前”：直核面由 `arch_boot_finish` 先 `store_kernel_info`、再进 `arch_boot_impl` 的 `boot_validate_and_prepare`；v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`。这两组腿（直核面与 v1 接收面）的 store 与 boot_validate 次序相反。

所有权：`BootStorage` 拥有 U-Boot shim 腿上的六类载荷；UEFI 腿只拥有五类（五个 `fill_*` 入口；`fill_map_tables` 一次提交两表）；UEFI 腿的 source 分区由两个阶段直写 wire：分类器先写 UEFI 记录，`authorize_and_copy` 随后用 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录并经 `push_dtb_sources` 追加、二次 `finish_sources()`；storage 的 source 切片为空，不在这句的覆盖范围内；strict 审计按 authority 过滤，因此这批 DTB 记录不计入 UEFI 权威的覆盖；`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自在闭包返回时提交计数，半提交状态不可表示。`BootReport<'a>` 只是借用视图；跨镜像发送侧把它序列化进 wire，接收侧在自己的 `.bss` 重建切片。`'static` 只由 `report_static` 的 one-shot 静态存储支撑，不从堆上“泄漏”出来。UEFI 腿的 source 记录由分类器与 DTB claims 两阶段直写 wire，不经过 `fill_source_records`；因此“第二次 fill 会 panic”只对实际走 `fill_*` 的腿成立。

校验的逐腿顺序（§4.4.1 的权威表）：直核面 `validate_requiring_sources` → `validate_before_tables` → `check_boot_postconditions` → `store_kernel_info` → `boot_validate_and_prepare` → `kmain` 的 `validate`；v1 接收面 `validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate`，不经过 `validate_before_tables`。`check_boot_postconditions` 在 v10 之后只剩 `kern_virt_base > u32::MAX` 一条断言；镜像与模块、模块与模块的半开区间重叠在共享校验体 `validate_impl_inner` 里，三档都会跑。

四类错误各有落点：缺来源记录 → `FreeSpanWithoutMemorySource`；权威与记录不匹配 → “declared RAM authority … has no source records”；半提交 → 走 `fill_*` 的腿第二次调用 panic；跨镜像指针 → 接收侧读到零或 fault。

---

## 4. 实现详解

设计定了，代码仍要在两个镜像、五个入口和一次 `ExitBootServices` 之间落地。本章沿两条 UEFI 链的实际入口走一遍：`main` 怎样把页表页预算交给 `prepare_and_enter_kernel_image`，九个动作在哪一步读文件、哪一步退出固件服务、哪一步把分类计数提交，以及 `KernelInfo` 的两条产出入径和六个落地区怎样保证接收侧只碰自己的副本。

### 4.0 本章导读

#### 学习目标

读完本章能：

- 沿 `main` → `prepare_and_enter_kernel_image` → `enter_kernel_image_v1` → `arch_boot_resume_v1` 走完两条 UEFI 链的入口。
- 说出九动作表里读文件、EBS、提交分类计数与跳转各在哪一步。
- 指出 `KernelInfo` 两条产出入径、六个落地区与门链的实际执行序。

#### 前置概念

- 第 1 章的三份数据与 I1–I4。
- 第 2 章的 C 侧调用树：`pre_init` 的 `get_parameters` 与 `pg_*` 五步的接缝。
- 第 3 章的 wire v1、arena、五条生产腿与 `BootStorage` 的所有权规则。

#### 最小示例

```bash
# 在源码里定位本章的 Step 路标与接收端首行（§6.1 的复算命令）
$ grep -n 'boot_stage!\|### minix-rs kernel-image' os/kernel/src/lib.rs os/kernel-image/src/main.rs os/kernel-image/src/bootface*.rs
```

命中的 `boot_stage!` 是 Step 1/2 与 Step 4 的打印点；`### minix-rs kernel-image` 是接收端进入 Rust 面的第一行（§6.1 的路标表）。

#### 失败对照

如果把 `boot_validate_and_prepare` 内的 `pt_alloc::register` 挪到进入内核之后，Step 1/2 第一次 `map_huge` → `alloc_pt_page` 会断言 `pt_alloc::alloc_pt_page called before register`；`init_boot_pt_alloc` 只初始化 bump 区间，不写注册槽。`boot_alloc_region()` 为 `None` 时 `boot_validate_and_prepare` 直接停机。所以注册是进入内核的前置条件，不能从内存图推断。

#### 小结

- 入口骨架只有一句：`main` 把页表页预算交给 `prepare_and_enter_kernel_image`，后者不返回。
- 九个动作按“准备 → 退出固件服务 → 交接”三段排列；读文件在 EBS 之前，跳转在 EBS 之后。
- 两条 UEFI 链共用发送侧；接收端按架构从 `rbx`/`x0`/`a0` 取 arena 基址后汇入 `resume_additive`。
- `build_kernel_info` 只服务两个直核面；三条 v1 腿的接收侧从 wire 解码得到同一结构。
- 六个落地区把切片深拷进内核 `.bss`，此后只使用这份副本。

#### 术语

- **九个动作**：`prepare_and_enter_kernel_image` 内部按序执行的九步，产出归属见 §4.2 的表。
- **落地区**：内核 `.bss` 里的六个定长存储区，`store_kernel_info` 把 wire 解码后的切片拷进去。
- **门链**：`validate_requiring_sources`、`validate_before_tables`、`check_boot_postconditions`、`boot_validate_and_prepare`、`KernelInfo::validate` 组成的校验次序。
- **A+D**：先读 ELF 预订 LMA 跨度、再分配 root/bump、最后拷贝段的动作组合。

### 4.1 入口与调用链

UEFI 入口是固件加载 `.efi` 后调用的那个函数。它只做一件事：调用 `prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`，把后续工作交给这个函数。后者内部再分三段：准备（读内核 ELF 并预订其 LMA 跨度 → 分配根页表页与 bump 区 → 拷贝段与装载模块 → 平台源与参数 → 分配 arena 并序列化非内存图载荷）、退出固件服务（配对循环里分类最终内存图并调 `ExitBootServices`）、交接（提交分类计数、构造报告并复核、激活自举根、补运行期字段、跳独立镜像）。入口本身不返回。

本章的 Step 编号在这里立住（02 篇的 4.3 节 与 §6.1 共用）：Step 0 校验与分配器注册检查（`boot_alloc_region()` 为 `None` 即停机；没有从 memmap 切 256 页的兜底，理由见 §4.1 末段）；Step 1 恒等映射；Step 2 内核高半映射（2b 高半设备别名、2c 设备窗口页）；Step 3 开分页（`enable`）；Step 4 DM 覆盖（概念入口见 §4.8；完整布局/策略归独立内存管理主题）。

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

- `main.rs` 的 [`fn main`](<os/boot-shim/src/main.rs:fn main>) 传给 `prepare_and_enter_kernel_image` 的 `boot_shim::BOOT_PT_BUMP_PAGES` 是**页表页预算**（常量值 1024，定义在 `lib.rs`；1024 页 = 4MiB；两个直核面另用 `bootface_common` 的 `PT_BUMP_PAGES = 256`）。第一段注释把这份预算的构成列了出来：恒等映射、内核高地址映射、Direct Map 覆盖建立、引导身份窗口的逐页拆解（只拆 `load_elf_into` 的 ELF 段页与 64 MiB VM heap 窗口，不是把全部 boot 恒等窗口撤掉）、以及交接 arena 之后的页表链条——它们都从同一个 bump 池里取页。预算是"上限"而不是"必须用完"：`BootAlloc` 只在分配时推进游标。**1024 与 256 的差别来自两条路径各自要建的东西**：跨镜像腿要额外建 Direct Map 覆盖、ELF 段的逐页拆解与 arena 之后的页表链，三架构各自的中层表加起来需要更大余量；两个直核面走的是同一片注册给 `pt_alloc` 的 bump 池，覆盖量级是各架构页表根 + 低半恒等映射 + 高半映射 + 两个 DM 窗口（riscv64 为 Sv39 root，aarch64 为 TTBR0/TTBR1 两套根；窗口构造点见 §4.3 的 `BootWindows`，DM 建表同样从 `pt_alloc` 取页）——**DM 窗口也算在这 256 页里**，只是窗口数与逐页拆解面远小于跨镜像腿，所以 1 MiB 够用。两者没有共享常量，因为它们描述的不是同一个上界；实际消耗由 `boot_alloc_used_bytes()` 在交接时记为 `kernel_allocated_dynamic`（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff` 的 `kernel_allocated_dynamic` 赋值点），不靠预算反推。**未用部分不退回**——VM 交接记录按 `boot_alloc_region()` 把整段扣除（见 §3.4 的扣除表），用掉的与剩下的都算内核保留。（UEFI 路径下 bump 区本就不在 conventional 列表里，这条扣除对 free list 是空操作；扣整段防的是自引导等路径把 bump 区记进清单的情形。映射责任移交 VM 之后，`vmctl_set_addr_space` 不再向这个池取 kerninfo 注入页，预算只剩启动期本身的消耗。）
- 两条 UEFI 产线的入口是同一个 `prepare_and_enter_kernel_image`：arena 分配、载荷序列化、配对循环都在 EBS 之前，EBS 之后提交分类计数并复核、构造 `BootReport`、建根、补字段与跳转（发送侧不构造 `KernelInfo`；`KernelInfo` 由接收镜像解码后形成）。分界线之后的代码不再出现任何 `uefi::` 调用（`prepare_and_enter_kernel_image` 与 `ebs_pair` 两个模块都如此，后者的分类计数打印也在 EBS 成功分支改走裸串口）；路标走裸串口。
- 进入内核的分道只剩接收端的一处：x86_64 的 `rust_image_main` 拿到的是 `_start` trampoline 从 `rbx` 转进 `rdi` 的 arena 基址（x86_64 入口约定，见本节总览表），aarch64 从 `x0` 取；两者都调用 `arch_boot_resume_v1`，再从既有的自举根上做增量阶段（`rust_image_main` 的分道臂代码——含 riscv64 的两种交付形态——逐臂引在 02 篇的 4.5 节，本节只讲 boot-shim 侧的入口链）。内核不再有"链进 PE 低地址内联执行"的生产路径，VM 也就不再需要在进程页表里复现低地址窗口（`reserved_regions` 的恒等重放半随恒等窗口退场，DM 候选半保留，见 §3.2）。

**若把页表分配器的注册挪到进入内核之后会怎样**。先锚定视角：这里的"分配器"是**接收镜像自己链接的那份 `BootAlloc` 副本**（§3.2：两个镜像各链接一份）——shim 进程内 `init_boot_pt_alloc` 初始化过的是另一份，与本推演无关；接收副本在 resume 之前 `boot_alloc_region()` 为 `None`。在此视角下：`prepare_and_enter_kernel_image` 动作 1（root/bump 分配之后）那次 `init_boot_pt_alloc` 照常执行，但 `register` 实际发生在 `boot_validate_and_prepare`（由 `enter_kernel_image_v1` 调用，带 `is_registered()` 守卫），把它挪到 `enter_kernel_image_v1` 的跳转之后（接收侧即 `arch_boot_resume_v1` 的增量阶段之后；`arch_boot` 是直核/测试入口，不是 UEFI 链的接收点）。`boot_validate_and_prepare` 在 `boot_alloc_region()` 为 `None` 时**直接停机**（该兜底已不存在）：从 `memmap` 里挑第一个够大的区间来兜底，等于把内核镜像、已装载模块或 arena 所在的位置当成空闲起点，把活着的物理页再发一次。本段挪动的是 `register`，不是 bump 区间初始化：`require_boot_region` 仍会通过，故障落在 Step 1/2 第一次 `map_huge` → `alloc_pt_page`，断言消息是 `pt_alloc::alloc_pt_page called before register`。两个对象要分开：`require_boot_region` 看的是 `boot_alloc_region()`（bump 区间），`alloc_pt_page` 看的是 `PT_ALLOC != 0`（页表分配器是否已注册）。注册是进入内核的**前置条件**，不是可以从内存图推断的默认值。
### 4.2 UEFI 路径：九个动作与产出归属

两条 UEFI 产线（x86_64 与 aarch64）共用同一个入口：`os/boot-shim/src/uefi_helpers.rs` 的 [`fn prepare_and_enter_kernel_image`](<os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image>)。它返回 `!`：建根、开分页、跳独立镜像，不返回 boot-shim。U-Boot 腿的入口是 `opensbi_helpers::prepare_boot`；UEFI 侧走自己的入口，两者不共享 trait。

```rust
// 节选（保留顺序与关键约束；注释与排版有改写/删节；source-symbol: prepare_and_enter_kernel_image；未展开段：参数统计的 `println`、`system_table`/`boot_services`/`ops` 构造、arena 的 `assert_range_clear_of_kernel`、`NeedBiggerBuffer` 扩容断言分支、`dumpdtb_blob` 的 `#[cfg]` 构造、入口与首段 VMA 的 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, …)`），os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image。
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

    // 语义表住在 shim 自己的静态区（`.data`：`BootStorage::new()` 非全零；见 §3.2）；loader 直接写进它的模块槽。
    let storage = UEFI_BOOT_STORAGE.take();
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

先按执行序读；动作 ID 只是稳定标签，不承担顺序语义。下表按时间拍列出真实执行顺序，后面的表再按动作 ID 给约束。动作 2 拆成 2a（读/预订）与 2b（拷贝/模块）两拍，所以 10 拍对应 9 个动作；动作 2b 与 §4.1 的 Step 2b 是两套编号，不要混读。

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

三套编号要分清：本节的“动作 ID”、本节的“时间拍”、§4.2.2 的描述符判定五步；§4.1 的 Step 0–4 只作概览，不参与本节计数。

**按动作 ID 的约束**：

| 动作 ID（非时序） | 时间序 | 动作 | 约束 |
|---|---|---|---|
| 1 | 2 | 分配根页表页与 bump 区，`init_boot_pt_alloc` | 必须在 DM 可达上界之下；同一物理区间由 shim 建根与接收侧各自续用，两个 `BootAlloc` 实例不共享游标（交接以 `bump_base`/`bump_next`/`bump_end` 三个字段记录游标位置，见 §4.6）。按 A+D 的顺序，本步排在动作 2 的 ELF 预订**之后**（先订跨度、再订自举页，避免自举页落进镜像的拷贝目标） |
| 2 | 1（读/预订）/ 3（拷贝/模块） | 读 `kernel.elf`、预订其 LMA 跨度；拷贝段与加载 12 个模块在动作 1 之后（时间列的 3 是动作 2 的拷贝/模块段；动作 3 在时间 4） | 必须在 EBS 之前（依赖 `SimpleFileSystem`）；A+D 的顺序与 ELF `Vec` 例外见本节表后的 bullet |
| 3 | 4 | 定位平台描述符来源，归一化 `param_buf` | 必须在 EBS 之前（依赖 Configuration Table——固件在 EBS 前交给应用的描述符表，DTB/RSDP 指针挂在其中——与 `LoadOptions`——固件为本镜像保留的命令行字节串） |
| 4 | 5 | 分配八页 arena（`alloc_arena()` 经 UEFI `boot::allocate_pages(MaxAddress(bootstrap_alloc_limit()))`——该上界为无参函数返回的 DM 可达界；`bump_alloc_pages` 是 OpenSBI 侧的另一个分配器，勿混），`LOADER_DATA` | 整段落在恒等/DM 可达界之下；接收方经恒等窗口读它 |
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

**动作 6 的分流规则**（[`fn classify`](<os/boot-shim/src/ebs_map.rs:fn classify>) 与单条判定 [`fn adjudicate`](<os/boot-shim/src/ebs_map.rs:fn adjudicate>)）。判据不是"类型标签加 RAM 顶两遍扫描"，而是与 `adjudicate` 源码注释逐行对应的五步加两道内联闸：

| 步 | 判据 | 命中动作 | 未命中 |
|---|---|---|---|
| Step 1 | 类型已知（≤ `LAST_KNOWN`）、属性仅 `KNOWN` 位、无 `AMBIGUOUS`/`PROFILE_ONLY`、无 ISA 掩码（`ISA_VALID`/`ISA_MASK` 均不置位） | 继续 | 报错（`UnknownType`/`AmbiguousAttribute`/`ProfileAttribute`/`UnknownAttribute`/`IsaAttributes`） |
| Step 2 | 类型 ∈ {`UNACCEPTED`, `PAL_CODE`} | `RefuseToBoot`（先于一切后续步） | 继续 |
| 内联闸 | `checked_shape`：描述符结构可表示 | 继续 | 报错 |
| Step 3 | 类型 ∈ {`RESERVED`, `UNUSABLE`} → `Excluded`；∈ {`MMIO`, `MMIO_PORT_SPACE`} → `DevicePath` | 定档返回（先于属性判定） | 继续 |
| 内联闸 | `checked_len`：物理范围可表达 | 继续 | 报错 |
| Step 4 | 属性含 `TYPE_INDEPENDENT` | `Side` | 继续 |
| Step 5 | 属性 `WRITE_BACK == 0` → `Side`；否则按类型表：`CONVENTIONAL`→可分配、`LOADER_CODE`/`LOADER_DATA`/`BOOT_SERVICES_CODE`/`BOOT_SERVICES_DATA`/`ACPI_RECLAIM`→`ReservedMapped`、其余→`Side` | 定档返回 | —（类型表兜底） |

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

模块契约是本篇里最容易在改动时踩坏的东西：三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）的“顺序”部分没有编译期检查，只能靠装载顺序、运行期断言与测试兜住；两个直核腿另有 `NAME_POOL` 与 `MODULE_NAMES` 的编译期逐字节对账（本节末）。装载期防线是 A/B/C 三道（接收侧另有 D，共四道）：**A** 每个段的文件字节区间与该段及**所有**段的目标 LMA 区间不相交（`first_source_destination_overlap`）；**B** 各段的目标 LMA 区间两两不相交（`first_overlapping_pair`）；**C** 模块区间不落进内核物理跨度：两条 UEFI 腿用 `alloc_module_pages` 拷贝前拒绝 + `assert_modules_clear_of_kernel` 装载后复查；U-Boot shim 腿用编译期 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 与 root/bump 复查。文件字节区间彼此之间的重叠不查——只读拷贝无害。任一条命中即 halt。Rust 侧的 A/B 是 ELF 装载新增的两道防线（守内核自身 PT_LOAD 段，本节）；C/D 才是 C 的 `overlaps`（§2.5）两类检查的落点：C 守模块 vs 内核跨度（§4.2 的 `record_kernel_span`、`alloc_module_pages`、`assert_modules_clear_of_kernel`），D 在接收侧重查同一对关系并追加模块 vs 模块（`KernelInfo::validate` 的两段断言——内核跨度那段与 C 故意重复，因为接收侧不能信任发送侧已查；§4.4）。

```rust
// 代码引用（文档注释的说明段整段略去，块内不保留省略号），os/boot-shim/src/loader.rs 与 os/libs/minix-boot/src/kernel_info.rs。
pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";
pub const DUMPDTB_PATH: &str = "/dumpdtb.dtb";
pub const MODULES_DIR: &str = "/EFI/minix/modules";

// 〔编注〕minix-boot：共享的规范名字表；数组长度在编译期钉住计数。
pub const MODULE_NAMES: [&str; NR_BOOT_MODULES] = [
    "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm", "pfs", "mfs", "init",
];

// 〔编注〕boot-shim：只重导出同一份表，不另立第二份。
pub use minix_boot::MODULE_NAMES;
```

- 名字表有 12 项（复算：`sed -n '/pub const MODULE_NAMES/,/];/p' os/libs/minix-boot/src/kernel_info.rs | grep -o '"[a-z]*"' | wc -l` 输出 12），`NR_BOOT_MODULES = 12` 与表同住 `minix-boot`；boot-shim 与内核的 strict 门读同一份常量（设计项）。12 不是随手定的数：它等于 C `boot_image` 表的用户态段长度，少一项会有服务器没有启动名，多一项则没有对应的进程号可绑（C 侧 `NR_BOOT_PROCS` 与本篇的 `NR_BOOT_MODULES` 是同一张表的两个计数口径；每个名字绑哪个进程号，由表项自带的 `*_PROC_NR` 给出——[`kernel/table.c`](minix3/minix/kernel/table.c) 的 `{DS_PROC_NR, "ds"}` 等条目）。
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
### 4.4 `KernelInfo` 构造的两条产出入径

五条生产腿（两条 UEFI 链、U-Boot shim、两个直核面）在**数据形状**上收敛，但构造入口分两组（各腿如何产出两张表以 §3.2 为权威，本节只讲构造入口）：两个直核面共用 [`fn build_kernel_info`](<os/boot-shim/src/lib.rs:fn build_kernel_info>)（boot-shim 库根导出）；三条 v1 腿（两条 UEFI 链 + U-Boot shim）共用 `BootStorage`/`BootReport` 形状与 arena 协议，发送侧不构造 `KernelInfo`。这是“层内收敛原则”（§3.5 问题二：收敛数据形状，不合并引导形态层）的落点：每条路径仍各自产出自己的两张表（`memmap` 与 `reserved_regions`），共享的是形状而不是一个构造函数。

```rust
// 代码引用（略去函数文档注释与 `minix_boot::` 路径限定符；`minix_boot::RamAuthority` 保留以标出权威类型；代码内注释有删节与改述），os/boot-shim/src/lib.rs:fn build_kernel_info。
pub fn build_kernel_info(
    report: &minix_boot::BootReport<'static>,
    authority: minix_boot::RamAuthority,
) -> minix_boot::KernelInfo {
    minix_boot::KernelInfo {
        memmap: report.memmap,
        reserved_regions: report.reserved_regions,
        source_records: report.source_records,
        kern_virt_base: report.kern_virt_base,
        kern_phys_base: report.kern_phys_base,
        kern_size: report.kern_size,
        boot_modules: report.boot_modules,
        platform_sources: report.platform_sources,
        param_buf: report.param_buf,
        authority,
    }
}
```

`reserved_regions` 的取值按腿的细目以 §3.2 为权威（UEFI `ReservedMapped`、U-Boot/riscv 直核 DTB 保留段、aarch64 直核 `fw_end=0`）；本节只保留构造入口，不逐腿复述。

| 字段 | 谁填（直核 builder 视角；v1 腿不调用 builder，见末行） | 值从哪来（来源细目以 §3.2 为权威，本表只留构造入口） |
|---|---|---|
| `memmap` | builder（直核面调用）；v1 接收侧由 wire 解码后填同一结构 | 见 §3.2 的 `memmap` 行：UEFI 用配对循环最后一次成功 `GetMemoryMap` 的 conventional 列表；U-Boot 走 `argv[0]`（`bootelf -p` 参数通道的首个参数 = DTB 物理地址，由 shim 在 boot.scr 里传入；不是进程 argv），riscv64 直核走 `a1`，aarch64 直核从固定 `DTB_PA = DRAM_BASE` 读（无入参）；同一份解析在 minix-boot |
| `reserved_regions` | 调用方 | 见 §3.2 的 `reserved_regions` 行：UEFI 用 `ReservedMapped` 列表（`Excluded`/`DevicePath`/`Side` 不进），U-Boot/riscv 直核用 DTB 以 `KERN_PHYS_BASE` 裁剪的保留段，aarch64 直核用 `fw_end = 0` 关闭固件前缀裁剪 |
| `kern_virt_base` / `kern_phys_base` / `kern_size` | 装载函数 | ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)`；装载器另断言 `e_entry == min(vaddr)`（UEFI 与 U-Boot 两条 loader 各一处），直核面由 `KERN_*` 常量提供；三架构跨距均由 L9 对账（按架构选 `bootface.rs`/`bootface_a64.rs`） |
| `user_sp`（v10 起不再是字段） | 消费点直接取常量 | `minix_types::USER_STACK_TOP` 单一权威常量；`arch_boot` 的用户栈映射、`build_vm_handoff` 的 `user_sp`、`fill_kerninfo_page` 的 `kui_user_sp` 与 `GET_KINFO` 的 `user_sp` 都读它 |
| `kern_stack_top`（v10 起不再是字段） | 访问器派生 | `KernelInfo::kern_stack_top()` = `kern_virt_base + kern_size`——**镜像尾**（末段 memsz 端点，Rust 链接脚本在 `.bss` 末尾 `ALIGN(0x200000)` 收口后的地址；与 C 的 `_kern_size` 不可混用）。它与 `kernel_boot_stack_top` 的区别、收口填充与清零语义见下表后的 bullets |
| `syscall_entry`（v10 起不再是字段） | 消费点直接取函数 | 实际门地址只由各架构的 `syscall_entry_va()` 提供（C 的 `kinfo_t` 也没有这个字段） |
| `boot_modules` | 装载函数 | 按 `MODULE_NAMES` 顺序的 12 项 |
| `source_records` | 经 `BootReport` 提供（builder 参数只有 `authority`） | 直核面：构造前算好 DTB 来源记录放进 report；v1 发送腿不调用 builder，发送侧写 wire、接收侧解码落 `.bss`（§3.2 的展开与 §4.2 的写入段）。startup_objects 参数已退场（§3.2.1） |
| `authority` | 调用方经 builder 参数传入 | 两个直核面传 `RamAuthority::DTB`；v1 接收侧由 wire 的 `ram_authority` 解码写入，UEFI 腿的发送侧在 `Serializer::new` 里传 `InputObjectId::UefiMemoryMap`，接收侧解码为 `RamAuthority::UEFI`（§3.2 的绑定说明） |
| `platform_sources` | 调用方 | UEFI：Configuration Table 扫描结果（DTB/RSDP，带 `len`）；OpenSBI：`[DTB]` 或空；两个直核面：DTB（`with_optional_len(DTB, …)`，各一条） |
| `param_buf` | 调用方 | 各条路径的 normalizer 输出：UEFI 读 `LoadOptions`，OpenSBI/直核读 DTB `/chosen/bootargs` |

`kern_stack_top` 的三条展开（v10 起该值是访问器派生，不是字段，三条语义不变）：

- **两个对象**：`kern_stack_top` 是镜像尾（Rust 链接脚本 `ALIGN(0x200000)` 收口后的地址，2MiB 对齐）；`kernel_boot_stack_top` 是 64KiB 引导栈数组自身的顶，定在收口**之前**。C 的 `kernel.lds` 没有这道收口，`_kern_size` 算到 `.bss` 结束，两者不可混用。
- **收口填充**：数组顶按 16 字节对齐、镜像尾按 2MiB 对齐，二者相差 0 到 2MiB−16。`jump_to_kmain` 把 sp 置到镜像尾（[`higher_half.rs`](os/kernel/src/arch/riscv64/higher_half.rs:jump_to_kmain) 的 `mv sp, stack_top`），栈向下生长先穿过这段填充；填充属于末段 memsz（三个 `os/kernel-image/*.ld` 的收口注释都写明），已映射、可写。填充等于给引导栈一段额外余量，不改变安全性。
- **清零语义**：只对跨镜像腿成立（loader 按 `memsz` 清零）；两个直核面的 `_start` 只清到 `__bss_end`，收口填充不保证为零——栈是先写后读，安全性不依赖清零。

`boot_validate_and_prepare` 是这份结构在内核侧的入口校验（pre-table 与 postconditions 门已先读过部分字段），它在建表之前先做校验：

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
2. `validate_before_tables`（pre-table gate）：`kern_size != 0`、`memmap` 非空、`param_buf.len() <= 1024`——它**不查** `boot_modules` 非空（QEMU 测试内核合法携带空模块表）。这里只列与 boot 路径直接相关的项，**不是完整清单**：共享校验体 `validate_impl_inner` 还检查 `platform_sources` 的空址/kind/溢出、两表升序与两两不相交、非零长度与页对齐等，完整清单以源码为准。
3. `check_boot_postconditions`：镜像地址下限（`kern_virt_base > u32::MAX`）。v10（设计裁决）之后只剩一条可被剩余字段证伪的关系——`kern_virt_base` 必须大于 `u32::MAX`（4 GiB 门；这不是各架构的用户/内核分界，而是“内核不能在 4 GiB 以下”的保守下界）；“`syscall_entry` 落在镜像跨度内 / `kern_stack_top` 不越过跨度上界 / 用户栈不越入内核半区”三条读取派生字段的断言随字段一起删除（用派生值重算同一条等式只是重抄，不构成检查）。
4. `KernelInfo::validate`（`kmain` 入口）与生产腿的 `validate_requiring_sources`（strict 变体）：前者复核非地址不变量、检查生产内核的 `boot_modules` 非空，并复核内核镜像与各模块、模块与模块的半开区间不重叠（相切放行；跨度加法用 `checked_add`，防 release 回绕把判断变成恒真）；非 strict 在 `source_records` 非空时也查“来源记录覆盖投影内存图”（条件性检查，见 §3.2）。`ValidationLevel` 三档是 `BeforeTables`/`Standard`/`Strict`；`Strict` 不带载荷，`authority` 从 `KernelInfo.authority` 读取，`validate_requiring_sources()` 无参。后者按结构体自带的 `authority` 要求 `source_records` 覆盖投影内存图，并要求内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内（v10 之前这条由 `startup_objects` 的一条记录回答；该字段与分区已随设计裁决删除）。

#### 4.4.1 门链与逐腿执行序

腿间实际执行序要说清：直核面是 **strict（`validate_requiring_sources`，两个 `bootface` 在调 `arch_boot` 之前显式调用）→ pre-table（2）→ postconditions（3）→ `store_kernel_info`（`arch_boot_finish`，完整落一次）→ boot_validate（1，`arch_boot_impl` 开头）→ kmain validate**；v1 接收面是 **strict → postconditions（3）→ `resume_boot_pt_alloc` → boot_validate（1，`resume_additive` 内）→ … → `store_kernel_info` → `kmain` 的 `validate`**，且**不经过** pre-table（`resume_additive` 体内没有 `validate_before_tables`）。接收面的 3 有可见调用点：`arch_boot_resume_v1` 在 `kinfo.validate_requiring_sources()` 之后、`boot_alloc::resume_boot_pt_alloc` 之前调用 `kinfo.check_boot_postconditions()`。下表按门列 bug 类，不按时间排序。

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
| `KernelInfo::validate`（`kmain` 入口） | 复核 `kern_size`/`memmap`/`param_buf` 非地址不变量；`boot_modules` 非空（仅生产内核；QEMU 测试内核合法为空）；`source_records` 非空时另查来源覆盖（条件性，见 §3.2） | 同前几行，在更晚的入口再挡一次；模块装载失败 |
| 共享校验体（`BeforeTables`/`Standard`/`Strict` 都跑；`kmain` 入口再跑一次） | `memmap` 与 `reserved_regions` 不相交 | 保留区被当成空闲页发出 |
| 共享校验体（三档都跑；接收侧 strict 再复核一次） | 内核镜像与各模块、模块与模块按半开区间互不重叠（相切放行；跨度加法 `checked_add`） | 跨二进制载荷把模块放到内核或另一个模块身上（信任边界上的第二道复核） |
| `validate_requiring_sources`（生产腿 strict；三条 v1 接收腿与两个直核面都调用） | 按 `KernelInfo.authority` 查 `source_records` 覆盖投影内存图；内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内；按共享 `MODULE_NAMES` 的位置核对 `boot_modules[i].name` | 生产者漏写 provenance、声明了没有记录的权威、伪造了模块名，或内核跨度没有任何内存图记录解释 |
| 规划期（不是门） | `plan_kernel_mapping` 的首选/回退选择 | 镜像跨距不满足回退尺寸对齐 → 规划阶段给出可读错误 |

**"去掉 `bootstrap_len == 0` 断言"在当前代码里已无从谈起**：字段与断言已随字段删除（论证见 §3.4），这条反向推演保留的是它曾对应的真实险情。字段删除后，“提供一个可误设的回收区间”这个接口面整体消失，险情在类型上不可复发（完整故障链见 §3.4）。
### 4.5 `store_kernel_info` 的六个落地区

`arch_boot` 的早期动作是把按值传入的结构深拷进内核自己的 `.bss`（`lib.rs` 的 [`fn store_kernel_info`](<os/kernel/src/lib.rs:fn store_kernel_info>)），然后只使用这份副本。

```rust
// 节选（四个切片落地区收敛到 `land_slice`；保留 helper、四处调用、仍内联的模块名/param 两块与指针回填；开头的一次性守卫 assert 与相邻接续行有省略），os/kernel/src/lib.rs:fn land_slice + fn store_kernel_info。
fn land_slice<T: Copy>(src: &[T], dst: &mut [T], what: &str) {
    assert!(
        src.len() <= dst.len(),
        "store_kernel_info: {what} exceeds the .bss landing pad"
    );
    if src.as_ptr() != dst.as_ptr() {
        // SAFETY: `src.len() <= dst.len()`, the ranges are distinct unless the
        // pointer equality guard above skipped, and `ptr::copy` tolerates
        // partial overlap.
        unsafe {
            core::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), src.len());
        }
    }
}

pub fn store_kernel_info(kernel_info: &KernelInfo) -> &'static KernelInfo {
    unsafe {
        let store = &mut *crate::globals::RESERVED_REGION_STORE.get();
        let src = kernel_info.reserved_regions;
        land_slice(src, store, "reserved_regions");
        let mstore = &mut *crate::globals::MEMMAP_REGION_STORE.get();
        let msrc = kernel_info.memmap;
        land_slice(msrc, mstore, "memmap");
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
                // `copy_from_slice` requires non-overlapping source and
                // destination; use the overlap-safe `ptr::copy`.
                unsafe {
                    core::ptr::copy(bytes.as_ptr(), slot.as_mut_ptr(), n);
                }
                let name: &'static str = match core::str::from_utf8(&slot[..n]) {
                    Ok(s) => s,
                    Err(e) => core::str::from_utf8(&slot[..e.valid_up_to()]).unwrap_or("?"),
                };
                bstore[i] = minix_boot::BootModule {
                    name,
                    start: module.start,
                    len: module.len,
                };
            }
        }
        let pstore = &mut *crate::globals::PLATFORM_SOURCE_STORE.get();
        let psrc = kernel_info.platform_sources;
        land_slice(psrc, pstore, "platform_sources");
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
        let sstore = &mut *crate::globals::SOURCE_RECORD_STORE.get();
        let ssrc = kernel_info.source_records;
        land_slice(ssrc, sstore, "source_records");
        let mut k = *kernel_info;
        k.reserved_regions = core::slice::from_raw_parts(store.as_ptr(), src.len());
        k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
        k.boot_modules = core::slice::from_raw_parts(bstore.as_ptr(), bsrc.len());
        k.platform_sources = core::slice::from_raw_parts(pstore.as_ptr(), psrc.len());
        k.param_buf = core::slice::from_raw_parts(param.as_ptr(), csrc.len());
        k.source_records = core::slice::from_raw_parts(sstore.as_ptr(), ssrc.len());
        *KERNEL_INFO.get() = Some(k);
    }
    crate::kernel_info().expect("store_kernel_info: KERNEL_INFO just written")
}
```

**表名：《落地区与交接容量》**。六个落地区、一个名字池、一行 `KernelInfo.memmap` 切片视图与两个 `VmBootHandoff` 交接字段的容量与溢出行为；**这张表是全文交接容量的唯一权威**；§2.4.2.4 与 §3.2 引用本表并可在行内重复数字以服务上下文，但不另立权威口径：

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

`land_slice` 里的 `if src.as_ptr() != dst.as_ptr()` 不是防御性假设：生产路径有两个 `store_kernel_info` 调用点——`arch_boot_finish` 传原始 handoff（真正拷贝一次）；`resume_additive` 的 store 源已是前一步解码落进 store 的切片（同址，跳过）。`kmain` 不再调用 `store_kernel_info`：它通过 `kernel_info()` 读同一 `.bss` 槽；`store_kernel_info` 顶部的一次性守卫会让第二次调用直接 panic，不是同址 no-op。`kernel_info()` 不是第二通道：`GET_KINFO`/`GET_IMAGE`、`phys_ram_view`、`harden_kernel_text` 与 `kmain` 都还通过它读取同一 `.bss` 槽；两条取用路径指向同一存储，区别只在“刚存完的这次调用拿返回值、其余调用拿全局访问器”。`ptr::copy` 是为**部分重叠**保留（`copy_nonoverlapping` 对部分重叠是 UB）；精确同址由守卫跳过，不会调用到拷贝。

容量分两组说：下面三个容量按“有独立设计依据”列，后几个按“直接对齐既有契约”列。三个容量各有来由：`RESERVED_REGION_STORE_LEN = 256` 是真机记录（无归档日志，可靠性低于可复跑探针）的 127 条 reserved 翻倍后向上取整到 2 的幂（独立容量；与 VM 侧 `free_regions` 64 / `deducted` 47 不是同一常量——那两个是本表下方的交接字段行，分属"内核落地区"与"VM 交接"两层）；`MEMMAP_REGION_STORE_LEN = 128` 直接定居 globals.rs——C 形状账本模块（`memmap.rs` 的 add/cut + 空洞 + 高水位）删除后，交接扣减由 [`vm_handoff.rs`](os/kernel/src/vm_handoff.rs) 的 `FreeRangeSet`（有序不相交页对齐集合）承担（扣减换手后），行为契约由"与参考区间减法逐页等价"的对拍测试锁住（读数与归因限定见 §3.2 的"不做深拷…会怎样"，实现侧增量见本节末）；`BOOT_MODULE_NAME_LEN = 16` 与 `vm_handoff::copy_name` 的 16 字节 `proc_name` 字段同宽，落地不会比既有长度契约更紧。另外几个容量定案：`PLATFORM_SOURCE_STORE_LEN = 4` 是"一个 DTB/ACPI 源加一个回退"的真实上限再乘二；`PARAM_BUF_STORE_LEN = 1024` 直接对齐 C 的定长语义，复制前先清零，短缓冲留下的尾部因此是全零而不是上一轮内容；`SOURCE_RECORD_STORE_LEN` 直接取 wire 分区容量（296），与解码端同一口径；v10（设计裁决）删掉了 startup 落地区（原容量 20 的规范集合不再存在），所以本表的 startup 行只作删除记录，不再有现役第七行。`BOOT_MODULE_STORE = 12` 的来源是 `MODULE_NAMES` 的长度（见 §4.3），它不是独立容量。

**只做一半深拷会怎样**：`reserved_regions` 深拷进去，但 `memmap` 与 `boot_modules` 保持原指针。后果是 free list 退化成空记录，第一次物理页分配失败时，现场离"三个切片里有两个没落地"隔着整个分类阶段。机制与读数见 §3.2.4，证据判别结论见 §5.3；本节不重复。
### 4.6 arena 与分配器换手的落地

三处写者与各自的自守卫在 §3.4.2 已列出。这里补上"游标"的两种恢复方式与扣除表的读法。

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

扣除表的读法：`build_vm_handoff` 把"内核保留"与"引导已用"两类区间收进同一份记录（见 §3.4.2 引用的表），其中 bump 区是**整段**；`VmBootAllocator` 的已用后缀（该分配器从每个区间末端向下消耗，已用部分是连续尾段——表里第二类）是同段内的另一笔。两份记录随交接页一起交给 VM，VM 在消费侧校验 `LiveBootstrap ∩ VM-free = ∅`——也就是说，"内核记账对不对"这件事不是靠 kernel 单方面声明，而是在 VM 侧再复核一次。

**两个 bump 实例共享一套算法**。U-Boot shim 的固定池从 `MODULE_REGION_BASE` 起按"根页 → 1024 页页表子区间 → 模块页 → arena"的顺序发放；内核镜像的 `BootAlloc` 只续用其中的页表子区间。两个实例**不共享游标**——合并曾被试过：shim 的游标跨过子区间末尾（模块与 arena 在子区间之外），`BootAlloc::resume` 直接看到 `next > end`，QEMU U-Boot 门在 3/6 个路标处 panic，合并方案已回滚。共享的是纯值核心 `minix_boot::BumpRegion { start, next, end }`：`alloc_pages` 的边界检查、`used_bytes` 与一致性谓词都只有这一份。编译期断言固定常量：池基址与大小按 4 KiB 对齐、`KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`、根页 + 页表子区间 + `ARENA_CAP` 不超过池大小、`BUMP_END <= boot_dm_admissible_end()`；运行期断言实例区间：`prepare_boot` 分配后检查根页就是池首页、页表子区间紧接其后且不越池尾，`BootAlloc::resume`/`region` 用 `BumpRegion::is_consistent` 校验跨镜像三元组。编译期只断言常量，运行时输入的合法性不假装编译期已证。

**arena 生命周期合同**。`arch_boot_resume_v1` 的解码是 arena 的最后一次读取：六个载荷全部复制进内核 `.bss` store，`KernelInfo` 的切片指向这些 store，`arena_pa` 不保留。VM 自由页清单在 `kmain → build_vm_handoff` 才发布，严格晚于解码，于是 `LastRead(arena) ≺ Publish(VMFreePages)`——arena 页可以按生命周期回收，不做永久扣除。这条合同只对 arena 会落进 conventional 的腿有意义（U-Boot shim 与合成测试）；两条 UEFI 腿的 arena 是 `LOADER_DATA`→`ReservedMapped`，从不在 free list 里，回收与否对 VM 无影响。startup 分区的删除账见 §3.2.1；这里只需要结论：内核不再保留 startup-object 记录，也不再有它指向的 arena 身份。宿主测试 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 用真实序列化 arena 解码后整块 poison（写入哨兵值，模拟 arena 被复用），再跑与 VM 相同的 `classify_regions`：自由清单与解码值一致，落在 conventional 区间里的 arena 页被重新发给 VM，内核镜像仍被扣除。若未来有人在跳转后读 arena，必须同时移动解码边界或把 arena 加进扣除表，否则 VM 可能发到一份活的 wire 镜像。

**去掉内核侧 `is_registered()` 守卫会怎样**：让 `boot_validate_and_prepare` 无条件 `register(boot_alloc::boot_pt_alloc)`。`register` 用 `PT_ALLOC.compare_exchange(0, alloc_fn, AcqRel, Acquire)` 抢唯一写权；失败即 `panic!("pt_alloc::register called twice")`，在**两种构建档**都生效，不存在“release 下静默保留第一个注册者”的分支。因此第二次注册不会改写函数指针，也不会让 boot 继续：若第一个注册者是某个测试内核（它用了一个避开内核镜像的池），后续注册会在启动现场停机，而不是悄悄沿用测试池。`is_registered()` 守卫因此是所有构建档的硬前置条件，不再只是给测试内核让路的便利。
### 4.7 与 Minix3 C 的函数落点对照

> 本节按"函数→落点"排，C 侧符号的速查索引在 §7.5；Rust 侧落点以 §3/§4 的定义节为准。三者冲突时统一按：**源码 > 定义节（§3/§4） > 速查表（§7.5）**。

| C 函数（第 2 章） | Rust 落点 | 保留 / 改型 / 淘汰 / 新增 | 差异原因一句话 |
|---|---|---|---|
| [`multiboot_init`](minix3/minix/kernel/arch/i386/head.S:multiboot_init) | [`fn main`](<os/boot-shim/src/main.rs:fn main>) | 改型 | 引导器跳转到内核入口 → 固件跳转到 boot-shim 入口 |
| `pre_init(magic, ebx)` | `prepare_and_enter_kernel_image`（UEFI 两条路径）与 `opensbi_helpers::prepare_boot`（U-Boot 路径） | 改型 | `pre_init` 的八条语句（02 篇的 2.4 节 计数）被拆成固件入口与共享 builder |
| `get_parameters(ebx, cbi)` | `prepare_and_enter_kernel_image` + `ebs_pair::pair_and_classify` + `build_kernel_info`（仅直核面） | 改型 | 拷一份引导器结构体 → 调固件接口拿两份内存列表 |
| `mb_set_param(...)` | [`normalize_param_buf`](<os/libs/minix-boot/src/boot_params.rs:fn normalize_param_buf>) + [`param_buf`](os/libs/minix-boot/src/kernel_info.rs:struct KernelInfo) | 改型 | 统一分词/拒绝/整条丢弃规则（十条可观察分歧见 §3.2），ARCH 生成、BOARD 有来源才生成；UEFI 与 DTB 两源共用 |
| `add_memmap(cbi, addr, len)` | `ebs_pair::pair_and_classify` / `build_memmaps`（构建侧）；内核侧扣减为 [`FreeRangeSet`](os/kernel/src/vm_handoff.rs) 的区间集合（扣减换手后，C 形状账本模块删除） | 改型 | 构建侧改为配对循环分类；内核侧按有序不相交区间集合记账，行为由对拍测试锁等价 |
| `cut_memmap(cbi, start, end)` | [`FreeRangeSet::cut`](os/kernel/src/vm_handoff.rs)（`classify` 扣减步调用） | 改型 | 引导侧报告它认为可分配的 DRAM（UEFI 腿已把内核跨度预订为 `LOADER_DATA`，自举/U-Boot 腿仍在表内）；切除/扣除在内核侧按腿生效。C 的第三个参数是"结束地址"，Rust 集合的 `cut` 收 `PhysRange`（半开区间 `[start, end)`）——C 里 add/cut 参数形不一致（§2.4 那处 `alloc_lowest` 传错参数就出在这个差异上），Rust 用区间类型把端点和长度分开 |
| `alloc_lowest(cbi, len)` | 当前无对应物；低地址代码的拷入端已有（[`fn install_at`](<os/arch/src/x86_64/ap_early_entry.rs:fn install_at>)，地址由调用者给） | 暂未移植 | 唯一的消费者是 32 位 AP 启动跳板（SMP 归 SMP 篇（尚未生成））；`install_at` 的注释写明扫描空闲表找最低区间的动作归 SMP 篇的 `smp_init`，**重做时不得复刻 §2.4 那处"把长度当结束地址"的误用**（这条规则依赖的四个前提与各自的复查判据见 §2.4.2 的前提清单） |
| `overlaps(mod, n, cmp_mod)` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（内核段间）、C UEFI 腿 `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核；U-Boot 腿用编译期上界 + root/bump 复查）、D `KernelInfo::validate`（模块 × 模块 + 内核 × 模块） | 改型 | `overlaps` 的两类检查落到 C/D；A/B 是 ELF 装载新增的两道防线（§4.3） |
| `pg_clear()` | [`X86_64Paging::new_from_page`](os/arch/src/x86_64/paging.rs:fn new_from_page) 的清零 | 改型 | 根页由调用者提供 |
| `pg_identity(&kinfo)` | [`build_bootstrap_root_and_enable`](os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable) Step 1 | 改型 | 范围由调用方窗口表决定；Step 1 固定 2 MiB piece，Step 2 用 `kern_huge` |
| `pg_mapkernel()` | 同函数 Step 2 | 改型 | 返回值不再被使用 |
| `pg_load()` | [`X86_64Paging::enable`](os/arch/src/x86_64/paging.rs:fn enable) 的 `mov cr3` | 保留（同一动作） | 硬件动作不变 |
| `vm_enable_paging()` | 同函数 `enable` 的 `EFER.NXE` + `CR0` + `CR4` 段 | 改型 | 多了 NXE；少了 `PSE` |
| `pg_alloc_page(cbi)` | 无同名对应物：4KB 被映射数据页的运行期分配归 VM `phys_mem`；引导期页表页见 `alloc_pagetable` | 职责转移 | 从记录尾部取页 → 运行期由 VM 物理分配器承担 |
| `alloc_pagetable(&ph)` | [`fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>) + 三架构实现 | 改型 | 6 页静态池 → 注册式分配器 |
| `pg_map(phys, vaddr, vaddr_end, cbi)` | [`X86_64Paging::map`](os/arch/src/x86_64/paging.rs:fn map)（4KB 粒度） | 保留（语义） | 4KB 细粒度映射的用途不变 |

这张表里 `alloc_lowest` 一行标的是"暂未移植"，意思是**当前 64 位路径不需要按原样移植**：`alloc_lowest` 的服务对象（把 16 位启动代码放到 1MB 以下）属于 AP 启动，而 64 位侧的对应机制（AP 停车邮箱 + 启动记录）在 [`smp.rs`](os/kernel/src/smp.rs) 与 [`main.rs`](os/kernel-image/src/main.rs) 的 `park` 段里，不经过"从记录最低处分配"这条路径。若 SMP 篇的低地址分配需求最终出现，它应以新机制重新实现，而不是把 C 的 `alloc_lowest` 原样搬过来；这条未闭环项归 SMP 篇，不是当前的缺陷。

静态页表池为什么不能原样搬到 64 位，见 §3.4 的分配器换手推演；本节只保留 `alloc_pagetable` 的对照行，不重复那段推导。

### 4.8 交接后的 Direct Map（概念入口）

**Direct Map 是内核访问物理内存的一种映射方式**：内核为一定的物理地址范围建立固定的虚拟地址窗口，使内核能够通过该窗口访问对应的物理页。

Direct Map 覆盖建立发生在启动信息存储之后、进入 `kmain` 之前，但两条腿的链不同：直核面是 `store_kernel_info` → `arch_boot_impl`（`build_bootstrap_root_and_enable` → `establish_boot_dm`）→ `jump_to_kmain`；v1 接收面是 `store_kernel_info` → `resume_additive` 内的 `establish_boot_dm` → `jump_to_kmain`，**不调用** `arch_boot_impl`/`build_bootstrap_root_and_enable`（活动根只做加法，重建会清零活动根）。`establish_boot_dm` 在已经活动的 bootstrap 页表根上追加映射，不重新创建或替换根页表。

**能够通过 Direct Map 访问，不代表该物理页可以分配给 VM。** VM 的可分配集合还要考虑固件内存分类、Direct Map 的覆盖范围，以及启动期间已经占用的物理页。未被 Direct Map 覆盖的候选内存不能进入 VM 自由表，否则 VM 分配出的页面可能无法被内核正常访问。

Direct Map 的完整布局、覆盖策略及其在物理内存管理中的作用，属于独立的内存管理主题；本节只介绍它在启动交接中的位置与作用。

---

## 5. 测试：四条不变量各自的证据

四条不变量听起来都能用“测试通过”背书，但不同测试证明的东西并不一样：宿主测试证明形状与负控，真机门证明某条链能走到终点，二者都不能证明任意畸形输入会被拒绝。本章要解决的问题是把 I1–I4 拆成可核对的证据矩阵：每条不变量有哪些宿主证据、哪些真机证据、各自不证明什么，以及一条失败链应该用哪个路标定位。

### 5.0 本章导读

#### 学习目标

读完本章能：

- 按 I1–I4 说出每条不变量各自的宿主证据与真机证据。
- 说出每条证据“不证明什么”，以及宿主测试的边界。
- 用真机门脚本的关键路标判断一条链走到了哪一步。

#### 前置概念

- 第 1 章的 I1–I4 与五条生产腿。
- 第 2 章 §2.6 的模块装载三层检查。
- 第 3 章的 wire、`BootStorage` 所有权与 `SourceRecord`。
- 第 4 章的 `store_kernel_info` 六个落地区与门链。

#### 最小示例

```bash
$ cd os
# 宿主证据：交接库与内核 mock 形态（§5.2 的读数）
$ cargo test -p minix-boot --lib
$ cargo test -p minix-kernel --features mock --lib
```

预期分别全绿；§5.2 记录的读数是 188/0 与 866/0/8。

#### 失败对照

如果把 `KernelInfo::validate` 的负控当成“所有畸形布局都会被拒绝”的证据，就会高估覆盖：§5.1 的“不证明什么”列写明它只覆盖已构造的负例，任意畸形 wire 仍需负控扩展。所以证据矩阵按不变量分行，每行同时列出证明范围与不证明范围。

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

### 5.1 四条不变量的证据矩阵

四条不变量各有自己的证据层，不能互相借用。门覆盖不等于成功路径覆盖；逐行读“不证明什么”列，尤其 aarch64 直核只到 SMP 交付。

| 不变量 | 宿主证据 | 真机证据 | 不证明什么 |
|---|---|---|---|
| I1 值可跨镜像 | `handoff_v1` 的 encode/decode round-trip；`KernelInfo` 字段台账；`store_kernel_info` 的落地区测试 | `test-shim-bootmarks-x86.sh`（x86 v1 交接）、`test-riscv64-uboot-shim.sh` 6/6 | 不证明任意畸形 wire 都被拒绝；负控覆盖已知变体 |
| I2 来源可追溯 | `memmap_source` 的 `SourceRecord`/`audit_source_coverage` 正负控；DTB 收集器 40 槽上限 | riscv64/aarch64 直核的 DTB memmap 启动 | UEFI 腿的 source 由分类器直写 wire，宿主只测分类器 |
| I3 所有权明确 | `BootStorage` 容量/半提交/one-shot 测试；`BootReport` 生命周期编译检查 | 五条腿都有真机证据；其中四条有 §5.4 的启动门链，aarch64 直核由 `test-smp-aps-aarch64` 驱动到 SMP 交付；riscv64 `bootface` 与 aarch64 UEFI 的 DTB_CFGTABLE 形态只验证 fail-closed 错误路径，不证明走到 `kmain`；aarch64 直核只到 SMP 交付（部分覆盖）；走到 `kmain` 的证据覆盖 x86 UEFI 与 riscv64 U-Boot shim；aarch64 UEFI 的判据只到发送侧分类点，未证明 `kmain` | 不证明所有调用点都遵守 one-shot；真机门不覆盖所有失败形态 |
| I4 校验先于消费 | `KernelInfo::validate_*` 的 `should_panic` 负控；布局/跨度被测 | QEMU 各腿的启动门 | 不证明所有畸形布局；只覆盖已构造的负例；不证明校验先于消费——顺序由 §4.4.1 的调用序与代码走查保证 |

### 5.2 宿主测试的边界

宿主测试的边界：`cargo test -p minix-boot --lib` 188/0、`-p boot-shim --lib` 136/0、`-p minix-kernel --features mock --lib` 866/0/8、`-p minix-elf --lib` 27/0。真机门：`test-shim-bootmarks-x86.sh` 与 `test-riscv64-uboot-shim.sh` 为发送/接收全链；`test-kernel-image-riscv64-bootface` 直核；aarch64 直核由 `test-smp-aps-aarch64` 到 boot/SMP 标记（其后的用户链 Data Abort 是已登记的 A2 遗留，不作为本表的通过证据）。

### 5.3 机制 → 需要证明什么

下表与 §5.1 使用同一批宿主/真机证据，只按机制重排；它不是第二份独立证据，读证据去 §5.1 的对应行。

| 机制 | 需要证明 | 证据 | 不证明什么 |
|---|---|---|---|
| 深拷不变量（I1/I3） | 接收侧消费前，载荷已在私有存储 | 见 §5.1 的 I1、I3 行（同一批证据） | 不证明任意畸形 wire；负控只覆盖已知变体 |
| 来源覆盖（I2） | 每个投影字节有来源记录 | 见 §5.1 的 I2 行（同一批证据） | UEFI 腿 source 由分类器直写 wire，宿主只测分类器 |
| 校验先于消费（I4） | 建表/分配之前完成校验 | 见 §5.1 的 I4 行（同一批证据） | 不证明所有畸形布局；不证明顺序（顺序由 §4.4.1 的调用序与代码走查保证） |

**零化读数的结论**（证据在 §3.2.4）：候选①“页表不重放”与候选②“被当空闲复用清零”都**未排除**（探针结果未归档、不参与 PASS 判据；当前依据是源码注释与人工观察）；因此“入口尽早深拷”是预防性不变量，不是已证机制。真机门只证明五条链能走到各自的终点，不证明这条候选②。

### 5.4 真机门链

`DTB_CFGTABLE=1` 指该固件配置把机器 DTB 发布到 UEFI 配置表；AAVMF 的 DTB 页不是 `EfiACPIReclaimMemory`，所以这是“存在但不可用”的拒绝分支，不是“无 DTB 配置”。默认配置没有可用的 DTB，判据看 `memmap source` 行。

| 链 | 脚本 | 判据（关键路标） |
|---|---|---|
| x86_64 UEFI | `test-shim-bootmarks-x86.sh` | `kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、`resuming high-half boot`、`entering scheduler` |
| aarch64 UEFI | `test-shim-bootmarks-aarch64.sh` | 前两条固定标记 + 第三条变体：默认无 DTB 配置看 `memmap source` 行；`DTB_CFGTABLE=1` 的强制 DT 配置看 `DTB preflight failed`（存在但不可用）行。两者互斥，均须前两条固定标记同时出现 |
| riscv64 U-Boot 直载测试链 | `test-riscv64-uboot.sh` | `### TEST_RESULT: PASS hello-boot-riscv64 ###`；宿主 U-Boot 不兼容时 SKIP（exit 2） |
| riscv64 U-Boot shim | `test-riscv64-uboot-shim.sh` | 6/6：`uboot leg enter`、`table built`、`kernel + 12 modules loaded`、`[raw] entering kernel image v1`、`resuming high-half … riscv64`、`entering scheduler` |
| riscv64 直核 bootface | `test-kernel-image-riscv64-bootface.sh` | 表址行、`table holds 0x` + 十六进制条目数（脚本完整串为 `table holds 0x0000000000000000 entries`）、`module placement failed` 的行序（故意让模块定位失败以验错误路径） |

aarch64 直核面不在上表：它由 `test-smp-aps-aarch64.sh` 用 `-kernel` 驱动，判据是 SMP 交付（`ap-arrived`/`ap-irq-ready`），不是启动路标门；`test-sysboot` 是 x86 用户态 IPC 交换测试，也不是本表的启动门。

### 5.5 未闭环项交接

本节列出的范围内，没有已登记但未关闭的测试缺口；覆盖度限制已在 §5.1 的“不证明什么”列明（含 aarch64 直核只到 SMP 交付），按范围声明不计为缺口。aarch64 直核的用户链 Data Abort 是已登记的 A2 遗留：A2 指 aarch64 直核在 SMP 交付之后进入用户链时的 Data Abort，它是已登记的功能遗留，不是测试缺口，单列不计入本表。五条生产腿各有对应的真机证据；其中 aarch64 直核经 `test-smp-aps-aarch64` 到 SMP 交付，不属于 §5.4 的五条门链，按 §5.1 的“部分覆盖”读；不把 fail-closed 形态算成走到 `kmain`。已关闭的三项：C 侧段归属由 `check-layout.sh` 的 L10（`Makefile.inc` 改名名单 ↔ `kernel.lds` 收段）覆盖；riscv64 载体的 PC 高半判据已并入 `kmain_verify` 的 PASS 条件；生产 `kmain` 入口的 ABI 栈形已由三份 `jump_to_kmain` 的静态断言覆盖。§6.1 的“step1+2 之后没有发送侧路标”是已知诊断边界，冻结的裁决明确不为此增加打印，由 QEMU `-d int` 覆盖；§6.3 的窗口断言关闭记录指 `BOOT_IDENTITY_MAP_END` 的 Step 1 断言。

---

## 6. 诊断


本章回答"引导失败了怎么查"：6.1 讲 EBS 前后两条输出通道与里程碑路标的读法，6.2 是从静默到可读的 panic 路径，6.3 列典型失败模式与对应信号，6.4 给可复制命令。这一章按用途查，不按执行顺序读。

### 6.0 本章导读

#### 学习目标

读完本章能：

- 按 EBS 分界判断一条输出该走固件控制台还是裸串口。
- 用里程碑清单把 boot-shim 的动作切成十几段，定位串口停在哪一段。
- 按“看输出 → 看异常 → 看页表走查”三层抓现场。

#### 前置概念

- 第 1 章的 I1–I4 与五条生产腿。
- 第 3 章的 v1 腿与直核面之分。
- 第 4 章的入口链与 Step 0–4 编号。
- 第 5 章 §5.4 的五个真机门脚本与 §5.5 的已知诊断边界。

#### 最小示例

```bash
# 第一层：跑 x86_64 UEFI 交接门，脚本把串口写入日志并 grep 标记（§6.4）
$ bash os/qemu-tests/test-shim-bootmarks-x86.sh
```

预期关键行依次出现：`kernel loaded (entry staged)`、`12 boot modules loaded`、`[raw] entering kernel image v1`、`resuming high-half boot`、`entering scheduler`（§5.4 的判据列）。

#### 失败对照

如果在 panic handler 里调用 `format!` 或固件服务，第二次故障（分配失败、协议空指针）会在处理第一次故障的过程中发生——串口上得到的是 panic 处理自身的崩溃或什么都没有，原始 panic 的消息与位置被覆盖。所以 panic 路径只用固定栈缓冲与 `raw_serial_line`，不分配、不调固件服务。

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
| `kernel: arch_boot entered` | 内核侧第一个路标；`kernel: preA rdi=…` 是另一条临时取证打印（源码注释自标 `temporary`），位于 `kmain` 内，不在 `arch_boot` 返回路径上 | **仅 x86_64 测试内核（直核入口）**；x86_64 的 UEFI 生产接收侧走 `arch_boot_resume_v1`，不打印 | `os/kernel/src/lib.rs` 的 x86_64 `arch_boot` |
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

**删掉 `[raw] entering kernel image v1` 这一行会怎样**：发送侧在调用 `enter_kernel_image_v1` 之前打印 `[raw] arena=…` 与这一行；建根与开分页发生在调用内部。删掉这一行后，串口停点按三档读：`[raw] arena=…` 表示还没进调用；`kernel: v4 capability facts ok` 表示已进调用但 Step 1/2 映射尚未完成（建根前半）；`kernel: step1+2 mappings ok` 表示 Step 1/2 已过（建根后半、**跳转前**）。**`step1+2` 不是"已经在独立镜像里"的信号**：它由发送侧 shim 进程内的 `build_bootstrap_root_and_enable` 打印（不是 `[raw] entering` 那一行，后者由 `uefi_helpers.rs`/`uboot_main.rs` 在调用 `enter_kernel_image_v1` 之前打印），三条 v1 发送腿共用；要判断接收端是否已起，只能看它之后有没有 `###` 或第二组 `v0`。若串口最后一行是 `step1+2`，可能是“建根完成、还没执行到跳转”，也可能是“已经跳转、接收端没出声”——这一行本身区分不了这两种状态，`step1+2` 之后到 `jump_kernel_image_v1` 之间仍有代码。`kernel:` 前缀同样不代表已经在独立镜像里。第二档与第三档之间没有专用发送侧路标；这是已知诊断边界，已裁决不为此增加打印：CR3 切换后 shim 侧不可观测属于 PE/CR3 架构特性（三次尝试实证），A→C 间隙由 QEMU `-d int` 异常追踪覆盖，门链全过为实证背书（诊断边界与决策见 §5.5 未闭环项）。

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
| 引导器指令次序错（`load_mods` 在 `multiboot` 之后） | [`get_parameters`](minix3/minix/kernel/arch/i386/pre_init.c:get_parameters) 的 `assert(mbi->mi_mods_count > 0)`（同函数前面那条查 `HAS_MODS` 的会通过，理由见 2.1） | 未开串口调试（默认）：屏幕刷出 `Minix panic. System diagnostics buffer:` 与 `press any key to reboot`、等一个按键才复位；命令行里写了 `cttyline=`（`do_serial_debug` 为真）时走**互斥**的另一条通路——panic 由串口同步输出后立即复位，不刷屏、也不等按键。这条断言排在命令行解析之后，所以 `cttyline=` 已经生效；对照下一行那条在解析之前，串口一定不出声 | 核对 `boot.cfg.default` 的 `;` 分隔次序 |
| 参数压栈次序错（x86-32） | [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的 `assert(magic == MULTIBOOT_INFO_MAGIC)` | 同样刷屏等按键，但串口一定不出声——这条断言在命令行解析之前，`do_serial_debug` 还是 `.bss` 的零初值；报出的表达式里带着 `magic` 这个名字，而它当时装的是地址（值本身不打印，见 §2.2 的说明） | 核对 `head.S` 两条 `push` 的次序 |
| 模块缺失 | `loader.rs:load_boot_modules_into` | `boot-shim: required boot module 'X' not found at <路径>` | 核对盘上 12 个模块名（脚本的 `mdir` 前置检查会先报） |
| 内核 ELF 读不到 | `FileLoader::read_required` | `boot-shim: required file not found: /EFI/minix/kernel.elf` | 核对 ESP 的 `/EFI/minix/kernel.elf` 与固件是否挂载了该分区 |
| bump 区间耗尽 | `BootAlloc::alloc` 返回 `AllocationFailed` | 内核侧 `map_huge`/`boot_pt_alloc` 失败，或 `enter_kernel_image_v1: bump region not registered` | 复算 `prepare_and_enter_kernel_image(N)` 的页数预算与镜像规模 |
| v1 信封/载荷无效 | `arch_boot_resume_v1` 的 `decode_*` | `kernel-image: handoff v1 envelope invalid: <变体>`（或 memmap/reserved/modules/platform/param/source 六个分区中任一段的对应行） | 核对 shim 的序列化顺序与描述符长度；先看 `[raw] arena=… conv=… res=…` 是否已打印 |
| 发送侧走完、接收端未起（无声卡死） | `[raw] entering kernel image v1` 之后、接收侧 `### minix-rs kernel-image: resuming high-half boot…` 之前 | 串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根的后半段：Step 1/2 映射已过、Step 2b/2c 也已完成，卡在 Step 3 `enable` 或跳转），更早则是 `kernel: v4 capability facts ok` 或 `[raw] arena=…`——三档读法见 §6.1 末段；接收侧第二组 `v0` 不出现 | 按 §6.1 末段三档先定位发送侧停点；再按 §6.4 第三层在 `enter_kernel_image_v1` 或接收侧委托链下断点 |
| ~~`bootstrap_len` 非零~~ | （字段与断言均已不存在，该失败模式不可复发） | 历史消息曾是 `bootstrap_len must be 0 (Rust port has no unpaged section), got N`——现行代码 grep 不到它，排障不要按这条找 | — |
| boot-shim 自身镜像被装载在 4 GiB 之上 | 固件装载器（UEFI 无此保证：规范不约束 loader 镜像的物理落点） | boot identity 窗口的 VA 上限是 `BOOT_IDENTITY_MAP_END`；shim image 取自 `LoadedImage.info()`、stack 取自当前 SP（`shim_stack_window()`）并加入窗口表，Step 1 映射循环断言 `end <= BOOT_IDENTITY_MAP_END`（2 MiB 上取整后）——落在 4 GiB 之上会以可读消息 fail-closed，不再静默落到无映射区 | 已由窗口断言覆盖（见 §5.5 的关闭记录） |
| 快照位置错（自举页被报成可用） | `assert_bootstrap_outside_memmap` | `conventional memmap [a,b) overlaps bootstrap allocation [c,d)` | 核对快照调用点在三次分配（root/bump/arena）之后 |
| 大页阶梯无档可用 | 规划层 `plan_boot_pages`（两个调用点：直核 `arch_boot` 家族与 v1 发送侧 `enter_kernel_image_v1`——后者服务 x86_64、aarch64、riscv64 U-Boot shim 三条腿） | 两条消息前缀对应两族：`arch_boot: page-size policy: …`（直核）/ `enter_kernel_image_v1: page-size policy: …`（v1 发送三腿）；消息体按变体分两种：`NoKernelPageSize` 是 `no kernel page size satisfies the layout`，`FallbackUnsupported` 是 `fallback page size <N> is not supported`（后者表示回退档本身不被硬件支持） | 跑 `check-layout.sh` 的 L3/L4/L5 |
| 页表切换后取指故障 | `Paging::enable` 之后第一条指令 | QEMU `-d int` 日志里的异常记录；`CR2`/`EPC` 指向当前指令 | 核对恒等映射范围是否覆盖该地址（02 篇的 4.3 节） |
| 固件不设 `WRITE_BACK`，内核跨度被分类成 `Side` | `ebs_map` 的属性分类；随后 `KernelInfo::validate` 的 strict 门 | `declared RAM authority … has no source records` 或内核跨度不落在两张表的任一区间内，strict 门 fail-closed | 看分类计数 `conv/res` 与内核跨度是否在 `reserved_regions`；确认固件内存描述符的 WB 位（§4.2.2/§4.4） |
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
| `get_parameters` | `prepare_and_enter_kernel_image` + `ebs_pair` / `build_kernel_info` | §2.3 / §2.4 / §4.2–§4.4 |
| `add_memmap` / `cut_memmap` | 构建侧：`ebs_pair::pair_and_classify`（生产）/ `build_memmaps`（测试辅助）；内核侧：`vm_handoff::FreeRangeSet` 扣减（C 形状账本模块已不存在） | §2.4 / §4.7 |
| `alloc_lowest` | 暂未移植（AP 启动归 SMP 篇；低地址代码拷入端 `install_at` 已有） | §2.4.2 / §4.7 |
| `overlaps` | 防线 A/B/C/D：A `first_source_destination_overlap`（源 × 目标）、B `first_overlapping_pair`（段间目标 × 目标）、C `alloc_module_pages`/`assert_modules_clear_of_kernel`（模块 × 内核跨度，装载期）、D `KernelInfo::validate`（模块 × 内核跨度 + 模块 × 模块，接收侧复核；模块 × 内核与 C 故意重复，因为接收侧不能信任发送侧已查） | §2.5 / §4.7 / §4.3 |
| `pg_clear` / `pg_identity` / `pg_mapkernel` / `pg_load` / `vm_enable_paging` | Step 1–3、`new_from_page`、`enable` | §2.2 / §2.8 / §3.4.3 / §4.7 / 02 篇的 4.3 节 |
| `freepde_start` | 字段已退场（字段删除）；`GET_KINFO` 报告位固定填 0 | 02 篇的 4.3 节 / §3.4 |
| `alloc_pagetable` | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page`（注册式分配器） | 02 篇的 2.4 节 / §3.4 / §4.6 |
| `pg_alloc_page` | 无同名对应物（运行期 VM 物理分配器；引导期页表页见 `alloc_pagetable`） | §2.4.2 / §3.4 |
| `bootstrap_start` / `bootstrap_len` | 字段已退场（字段删除；C 侧回收机制照旧） | §2.3 / §3.4 |
| `mem_high_phys` | 无对应物（boot identity 窗口） | §2.4 / §2.4.2 / §3.2.2.1 |
| `kinfo`（全局） | `KernelInfo`（按值交接 + `.bss` 深拷） | §2.3 / §3.2 / §4.5 |
| `kmain` | `kmain`（高半执行） | §2.2 / §2.8 |
| `k_initial_stktop` | 角色对应（换栈目标），**不是同一对象**：`kern_stack_top` 是镜像尾，`kernel_boot_stack_top` 是栈数组自身的顶（sp 初值不取它，取镜像尾） | §2.2 / §2.8 |
