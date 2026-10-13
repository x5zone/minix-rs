# 07 · 跨空间初始化：Direct Map 取代 32 位临时窗口

> **C 真值源**：`minix3/minix/kernel/main.c`（`arch_post_init`/`memory_init` 的调用位置）、`minix3/minix/kernel/arch/i386/protect.c:arch_post_init`、`minix3/minix/kernel/arch/i386/memory.c:memory_init/createpde/lin_lin_copy/vm_memset/mem_clear_mapcache`、`minix3/minix/kernel/arch/i386/pg_utils.c:pg_info`、`minix3/minix/kernel/system/do_vmctl.c`（`VMCTL_CLEARMAPCACHE` 分支）、`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`、`minix3/minix/kernel/arch/i386/klib.S`（含 `ENTRY(__switch_address_space)` 入口）。
> **Rust 模块**：`os/kernel/src/dm_coverage.rs`（`establish_boot_dm` 与四个候选源函数）、`os/arch/src/arch/direct_map.rs`（`DirectMapArch` trait 与三架构实现）、`os/arch/src/arch/dm_coverage.rs`（`DmRange`/`plan_range_units`/`establish_dm_range`）、`os/kernel/src/lib.rs`（`init_post_and_memory`/`switch_address_space`/`arch_boot_impl`）、`os/arch/src/arch/paging.rs`（`map_kernel`/`PageFlags`）。
> **前置**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)（boot 期建表，§2.6）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（进程槽与 boot→running 转换，§3.12）。
> **说明**：本篇讲一件事的两代解法——内核没有自己的页表，却要读写别的地址空间里的内存。32 位 C 的答案是"在当前页表里借一个 4 MB 窗口"（记录 → 分配 → 使用 → 清理四步）；64 位 Rust 的答案是"划一段常驻线性窗口，翻译退化为一次加法"，以及随之而来的双视图地址空间与运行期窗口覆盖模型。**与 01 篇的分界**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 已从 boot 建表角度讲过 `createpde` 借槽位窗口与 `freepde_start` 的语义（含偏移一位后果）、§3.8/§3.9 讲过页表页分配器换手与 `free_upper_idx` 字段保留；本篇从跨空间访问机制的角度接管同一段代码的完整生命周期，引用不重复。

## 1. 概述

### 1.1 CPU 只认一张页表：跨空间访问的真实困难

MMU 做虚拟地址翻译时只看一个东西：当前装在页表根寄存器里的那张页表。x86-32 上是 CR3，翻译过程不问"发起访问的代码来自哪个特权级"——CPL0 的内核代码和 CPL3 的用户代码走的是同一条翻译路径，特权级只影响权限检查（U/S 位那一半），不影响"这个 VA 有没有映射"这个问题本身。

第 2 章会给它的代码证据；先把事实摆出来：**内核没有自己的页表**。C 的调度器不为内核任务换页表根——`minix3/minix/kernel/arch/i386/klib.S:__switch_address_space` 的第一件事就是检查目标进程的 `p_cr3` 是否为 0，为 0（内核任务，没有自己的页表）就直接返回，CR3 保持原值。也就是说，内核代码永远运行在"某一张已经被装上的页表"里，这张表通常属于某个用户进程。`minix3/minix/kernel/cpulocals.h:ptproc` 的模块注释把这件事说得很直白：

```c
/*
 * 逐字引用，minix3/minix/kernel/cpulocals.h（ptproc 字段的模块注释）。
 */
/*
 * which processpage tables are loaded right now. We need to know this because
 * some processes are loaded in each process pagetables and don't have their own
 * pagetables. Therefore we cannot use the proc_ptr pointer
 */
	struct proc * ptproc;
```

注释里 "processpage" 连写是源码原文。这段注释给了两个信息：ptproc 记的是"当前装着谁的页表"；它必须与 proc_ptr（当前运行的进程）分开，因为两者可以不是同一个对象——运行着的进程可以没有自己的页表。

在这前提下，"内核要访问一段内存"按翻译是否现成拆成三种情况：

| 内核要访问什么 | 谁的表在翻译 | 翻译是否现成 |
|---|---|---|
| 内核自己的代码与数据 | 当前这张（任何一张都行） | 现成——每张进程页表都映射内核段 |
| 当前表主人的用户内存 | 当前这张 | 现成——该表的 USER 项，CPL0 可访问 |
| 别的进程的用户内存、或裸物理地址 | 当前这张 | **不现成**——翻译记录在别那张表里，或者根本只存在物理地址 |

第 2 章会看到，`createpde` 的函数体开头恰好就是这张表的前两行：目标进程是当前表主人（`pr == ptproc`）或内核进程（`iskernelp`）时，它把调用者给的地址原样返回，一个条目都不写。跨空间访问的困难只属于第三行，而且困难与"内核态还是用户态"无关——就算把整套机制放到 CPL0 之外去想，"当前页表解释不了目标 VA"这句话也一个字不用改。反过来，若设想"内核靠特权级绕过翻译"：绕过翻译的唯一开关是 `CR0.PG = 0`，而内核自己的高地址映射恰在同一条翻译路径上，关掉它之后第一条取指就找不到下一句指令（[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 给过同族推演，对象这里是"特权级假设"本身）。

### 1.2 C 的 32 位解法：临时窗口的一生

32 位世界的约束很紧：虚拟地址空间 4 GB，页目录 1024 个槽位，内核映射占掉高地址的一截之后，剩下的空闲槽位是稀缺资源。C 的解法是：**在当前生效的页表里，临时占用一个空闲槽位，把目标地址对应的页目录项写进去**——对方地址空间里那个 4 MB 块的翻译，就这样出现在当前地址空间里；传输完成后条目作废，槽位留给下一次借用。

这套机制的一生分四步，每步有一个 C 函数：

```
              一次跨地址空间访问（拷贝 / 清零 / 读写）

   [记录] boot 收尾，一次            [分配] boot 收尾，一次
   arch_post_init                   memory_init
   ptproc = VM                      freepdes[0..1] =
   p_cr3 / p_cr3_v 写进 VM 槽         freepde_start 起的两个槽位号
        |                                |
        +---------------+----------------+
                        |
                        v
              [使用] 每次跨空间访问
              createpde(目标, 地址, 借几号槽)
              把目标的 4MB 窗口写进当前表
              条目变了就 reload_cr3
                        |
                        v
              [清理] VM 显式请求（SYS_VMCTL 子命令）
              mem_clear_mapcache：窗口条目写 0
```

图上有两处先记下、第 2 章展开：第一，四步里只有"使用"是每次访问都要走的，"记录"和"分配"在 boot 收尾各做一次，槽位是**一次拨给、终身复用**的；第二，"清理"不是内核自己找时机做，而是 VM 通过一个系统调用子命令显式请求——VM 知道什么时候页目录的归属变过，内核不知道。

这个四步结构与 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 的分工在这里说清楚：01 篇从 boot 建表的角度讲过 `pg_mapkernel` 为什么返回"第一个空闲槽位号"、返回值偏移一位会砸中内核映射的最后一块（那篇 §2.6 的"偏移一位的后果"一段），也贴过 `memory_init` 与 `createpde` 的关键片段；本篇不重复那些，从"这套机制作为一个整体怎么活、怎么死"的角度，把四步各自的函数完整走一遍。

### 1.3 64 位的方向性区别：从"每次开工的机制"到"常驻的常数加法"

64 位地址空间把约束松开了：虚拟地址远远多于物理地址，OS 可以划一段固定虚拟基址，把全部（或一大段）物理内存按 `VA = PA + 基址` 线性别名进去。这个做法的名字叫 Direct Map。先给它定性（Rust 侧的模块文档开头就是这么写的，出处见第 3 章）：它是**软件设计模式，不是硬件机制**——硬件提供的仍然只是 MMU（页表、TLB、PTE 位），Direct Map 是内核使用这套 MMU 的方式：建立一条固定的物理到虚拟线性偏移。Redox 的内核 direct map、Linux 的线性映射区都是同构做法（`rewrite-notes/02-stage-vm/07-pagetable-struct.md` §1.4 有外部对照，此处引用不重复）。

拿它对住第 1.2 节的四步机制，方向性区别是：

| 维度 | C 临时窗口 | Direct Map |
|---|---|---|
| 窗口数量 | 2 个槽位，全系统共用 | 每张进程页表里各有一份完整窗口 |
| 生命周期 | boot 拨槽、按次借用、显式清理 | boot 期建一次，之后永不改内容 |
| 每次访问的动作 | 写 PDE → 旧值比较 → 可能刷 TLB → 截断传输块 → 算窗口内地址 | 一次加法 |
| 需要的状态 | ptproc、freepdes[]、条目旧值 | 无——两个常量 |

第三行是关键：Direct Map 省掉的不是一次函数调用，而是一整类状态。上面那张表里的每一行状态（哪个槽借给了谁、这次写没写目录、要不要刷 TLB）在常数加法的世界里都没有对应物。哪些 C 函数因此整体消失、哪些以别的形态活下来，是第 3 章的素材；窗口本身怎么在 boot 期建起来，是第 4 章的素材。

### 1.4 ptproc 的两个用途与它的去留

第 1.1 节的 ptproc 在临时窗口机制里出现过两次，两次的身份不同：

- **用途一（窗口侧）**：`createpde` 要把目标的页目录项写进"当前生效的那张表"，写谁？写 ptproc 指的那张——ptproc 的 `p_cr3_v` 就是写入目标；同时 `pr == ptproc` 又是"目标已可见"的特判条件。窗口机制死后，这个用途没有对应物。
- **用途二（切表侧）**：`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3` 在 VM 通过 `VMCTL_SETADDRSPACE` 给进程换页表时，要判断"要换上去的表是不是已经装着的这张"，是就不必真写 CR3：

```c
/*
 * 节选（保留 ptproc 判据；省略号处为 VM_PROC_NR 分支——arch_enable_paging 调用——
 * 与 RTS_VMINHIBIT 的解除），minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3。
 */
static void setcr3(struct proc *p, u32_t cr3, u32_t *v)
{
	/* Set process CR3. */
	p->p_seg.p_cr3 = cr3;
	assert(p->p_seg.p_cr3);
	p->p_seg.p_cr3_v = v; 
	if(p == get_cpulocal_var(ptproc)) {
		write_cr3(p->p_seg.p_cr3);
	}
	...
```

（`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`，VM 换表请求的落点。）

这个判据与窗口无关：只要"换页表"这件事还存在（VM 运行期一直在给进程建表、换表），"目标是否就是当前在位的那张表"就需要一个记录来回答。所以用途二活了下来。

两用途的去向表：

| ptproc 用途 | 消费点 | 去向 |
|---|---|---|
| 窗口的写入目标与"已可见"判据 | `minix3/minix/kernel/arch/i386/memory.c:createpde` | 随临时窗口机制废弃 |
| 换表时"是否立即重载"的判据 | `arch_do_vmctl.c:setcr3` | 存活——Rust 侧落到 per-CPU 的 ptproc 记录（第 3 章 §3.5、第 4 章 §4.2） |

还有一个反向假设要处理：把 ptproc 换成 proc_ptr 行不行？不行，而且 `minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy` 的入口断言就是证据——它要求 `read_cr3() == ptproc->p_cr3`（当前硬件寄存器装的就是 ptproc 的表）。这个等式由 `minix3/minix/kernel/arch/i386/klib.S:__switch_address_space` 维持：每次真实切表（`mov %eax, %cr3`）之后跟着一条 `mov %edx, (%eax)`，把新表主人写进 ptproc。proc_ptr 记的是"正在跑谁"，与"表是谁的"是两个问题：发起跨空间拷贝的系统调用由调用者触发，但表可以是任何一张——把判据换成 proc_ptr，`setcr3` 的"目标已在家"判定和 `lin_lin_copy` 的入口等式都会失去维护者。

### 1.5 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| `createpde` 借槽位窗口的 boot 建表背景、`freepde_start` 偏移一位后果 | 引用不重复 | [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 |
| `free_upper_idx` 字段为何保留 | 引用不重复 | [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §3.9 |
| `init_post_and_memory` 的三条断言（boot→running 侧） | §4.2 讲机制，转换形状引 06 | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §3.12 |
| 页表结构本体（`pt_t`、层级、宏族、VM 侧 Direct Map 消费） | 不讲 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md` §1.4、`08-pagetable-ops.md` |
| 跨空间拷贝的系统调用消费面（`sys_vircopy` 一族谁在调） | 只讲到 `lin_lin_copy` 为止 | `22-syscall-copy`（跨空间拷贝调用面，尚未重生成） |
| 页错误挂起与恢复协议（`vm_suspend` 之后的事） | 一行指向 | `29-cross-space-runtime`（VMSUSPEND 协议，尚未重生成） |
| AP（应用处理器）侧的 ptproc 安装路径 | 一段带过 | `10-smp-boot`（SMP 启动，尚未重生成） |

认知减负声明：x86 分页的 U/S 位、PWT/PCD 位这些硬件细节，本篇只用其中一条语义（每个 VA 翻译的 U/S 取值来自它自己的页表项，不存在"一条翻译同时是两种特权"），位表本身不展开——它在 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6.1 已经逐位列过；TLB 的替换与预热策略属于 MMU 通识加 `rewrite-notes/02-stage-vm/07-pagetable-struct.md` 的辖区，本篇不碰。

把第 1 章的图按同一框架收一次：四步机制里，"记录"与"分配"各跑一次、供给全程；"使用"每次访问都改当前表并可能刷 TLB；"清理"等 VM 的显式指令。第 2 章按这个顺序走函数。

## 2. C 源码分析

### 2.0 本章地图：一条四步生命周期

四个阶段的函数与触发点全部在 C 主流程的两个位置上：

| 阶段 | 函数 | 触发点 | 频率 |
|---|---|---|---|
| 记录 | `minix3/minix/kernel/arch/i386/protect.c:arch_post_init` | `minix3/minix/kernel/main.c:283`（boot image 循环之后） | boot 一次 |
| 分配 | `minix3/minix/kernel/arch/i386/memory.c:memory_init` | `minix3/minix/kernel/main.c:293`（紧随其后） | boot 一次 |
| 使用 | `minix3/minix/kernel/arch/i386/memory.c:createpde`（被 `lin_lin_copy`、`vm_memset` 调用） | 每次跨空间访问 | 每次，循环内反复 |
| 清理 | `minix3/minix/kernel/arch/i386/memory.c:mem_clear_mapcache` | `minix3/minix/kernel/system/do_vmctl.c:162` 的 `VMCTL_CLEARMAPCACHE` 分支 | VM 显式请求 |

清理的发起者要单独确认一遍。`minix3/minix/include/minix/com.h:408` 定义 `VMCTL_CLEARMAPCACHE = 32`；内核侧分支：

```c
/*
 * 逐字引用，minix3/minix/kernel/system/do_vmctl.c（VMCTL_CLEARMAPCACHE 分支）。
 */
	case VMCTL_CLEARMAPCACHE:
		/* VM says: forget about old mappings we have cached. */
		mem_clear_mapcache();
		return OK;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 `VMCTL_CLEARMAPCACHE` 分支。）

注释原文只有一个主语：VM says。同一分支上文（`VMCTL_VMINHIBIT` 的处理段）还有一条更长的注释解释为什么必须由 VM 来说："We don't know whether kernel has the changed mapping installed to access userspace memory. And if so, on what CPU. More over we don't know what mapping has changed and how and therefore we must invalidate all mappings we have anywhere."（引文至该句为止；源注释末尾还有一句 "Next time we map memory, we map it fresh."）——内核没有足够信息判定"哪些窗口缓存还可信"，只有改动了页目录归属的 VM 知道。这一条同时回答了"为什么清理不由内核自动做"：自动清理的时机无从判定。

下面四节按"记录 → 分配 → 使用 → 清理"走函数；走完后 §2.6 合成一张总表。

### 2.1 记录：`arch_post_init` 把"表是谁的"和"表在哪"写下来

`minix3/minix/kernel/arch/i386/protect.c:arch_post_init`（全函数，protect.c:370–377 共 8 行）：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/protect.c:arch_post_init。
 */
void arch_post_init(void)
{
  /* Let memory mapping code know what's going on at bootstrap time */
  struct proc *vm;
  vm = proc_addr(VM_PROC_NR);
  get_cpulocal_var(ptproc) = vm;
  pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
}
```

逐行解码：

- `vm = proc_addr(VM_PROC_NR)`：按进程号取 VM 的槽位地址。此刻 boot image 循环（`main.c:165-272`）已经跑完，VM 槽的调度与权限字段已就位。
- `get_cpulocal_var(ptproc) = vm`：把本 CPU 的 ptproc 记为 VM。为什么是 VM？因为刚建好并装上的 bootstrap 页目录**就是** VM 的初始页表——第 1.1 节"表是谁的"这个问题，boot 收尾时的答案只有一个：VM。
- `pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v)`：把页目录的地址写进 VM 槽的 `p_seg`，一次写两个值。

第二个值的来历在 `minix3/minix/kernel/arch/i386/pg_utils.c:pg_info`（全函数，5 行）：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_info。
 */
void pg_info(reg_t *pagedir_ph, u32_t **pagedir_v)
{
	*pagedir_ph = vir2phys(pagedir);
	*pagedir_v = pagedir;
}
```

`pagedir` 是 `pg_utils.c:19` 的文件级静态数组（`static u32_t pagedir[1024] __aligned(4096)`）——boot 期 `pg_clear`/`pg_identity`/`pg_mapkernel` 写的就是它（建表五步见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6）。`pg_info` 交出的两个值指向**同一个对象**：`p_cr3` 是它的物理地址（给 MMU 与 `write_cr3` 用），`p_cr3_v` 是它的虚拟地址（给内核直接写条目用）。内核能解引用 `p_cr3_v`，靠的不是什么特殊通道，而是第 1.1 节摆过的事实——内核段在每张表里都有映射，内核静态数组的地址在任何一张在位的表里都可访问。

**不看代码想不到的一条**：`arch_post_init` 全程不分配、不建表，它做的只是"记名字"——但内存管理代码此后的一切跨空间访问都以这两个记录为前提：`createpde` 每次写入的目标是 `ptproc->p_seg.p_cr3_v`，`mem_clear_mapcache` 清的也是它。

如果反过来想：若只记物理地址、不记虚拟地址——`createpde` 与 `mem_clear_mapcache` 都要直接写页目录条目，而写条目需要可解引用的虚拟地址；开分页之后物理地址对 CPU 不可直接寻址，机制在它最需要的那个动作上失效。`p_cr3` 与 `p_cr3_v` 成对存在，是"MMU 用物理、内核用虚拟"这个分工在数据结构上的直接投影。

### 2.2 分配：`memory_init` 拨出两个窗口槽

先看槽位的账面。`minix3/minix/kernel/arch/i386/memory.c` 头部：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/memory.c:29-31。
 */
static int nfreepdes = 0;
#define MAXFREEPDES	2
static int freepdes[MAXFREEPDES];
```

三个文件级静态：已用数、容量 2、槽位数组。`minix3/minix/kernel/arch/i386/memory.c:memory_init`（全函数，11 行）：

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

逐行解码：

- `assert(nfreepdes == 0)`：入口断言。`memory_init` 在 `main.c:293` 只被调用一次，这条断言把"分配只发生一次"从约定升级成可检查的约束——第二次调用直接启动失败。
- 两条自增赋值：把 `kinfo.freepde_start` 起的两个连续槽位号抄进 `freepdes[0]`、`freepdes[1]`。`freepde_start` 的值来自 boot 期 `pg_mapkernel()` 的返回值——"内核映射占完之后第一个空闲的槽位号"（语义与偏移一位后果见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6，此处引用）。
- 三条出口断言：`freepde_start < 1024` 保证没拨出页目录之外；`nfreepdes == 2` 与容量声明对账；`nfreepdes <= MAXFREEPDES` 防数组越界。

容量为什么恰好是 2？看消费者怎么用：`minix3/minix/kernel/arch/i386/memory.c:189-190`，`lin_lin_copy` 每轮循环固定传 `free_pde_idx = 0`（源方向）与 `1`（目的方向）；`vm_memset` 只用 0 号。两个并发方向的跨空间访问各占一槽，2 正好是"一次拷贝"的需求量，一个都不多余。

**不看代码想不到的一条**：这次"分配"拨的不是内存，是**页目录槽位的编号**——`freepdes[]` 里存的只是两个 0–1023 的整数，真正的内存（4 MB 窗口背后的物理页）要到每次"使用"时才由写入的 PDE 值临时指定。分配动作与内存无关，这是"窗口"这个词在实现层的真实含义。

如果反过来想：若把容量扩到 4（`MAXFREEPDES = 4`）并拨四个槽——代码里没有任何消费者使用 2、3 号下标（两处调用点的 `free_pde_idx` 都是字面量 0 或 1），多拨的两个槽永远空转；而 `assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES)` 的上界压力反而翻倍。反过来若容量为 1——`lin_lin_copy` 的目的方向没有槽可用，跨进程拷贝退化成"只能单向"。2 是消费者结构决定的数，不是拍出来的。

### 2.3 使用：`createpde` 的一次完整判定

本篇的主走查函数。`minix3/minix/kernel/arch/i386/memory.c:createpde`（全函数逐字引用，61 行，含注释）：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/memory.c:createpde。
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
	phys_bytes offset;
	int pde;

	assert(free_pde_idx >= 0 && free_pde_idx < nfreepdes);
	pde = freepdes[free_pde_idx];
	assert(pde >= 0 && pde < 1024);

	if(pr && ((pr == get_cpulocal_var(ptproc)) || iskernelp(pr))) {
		/* Process memory is requested, and
		 * it's a process that is already in current page table, or
		 * the kernel, which is always there.
		 * Therefore linaddr is valid directly, with the requested
		 * size.
		 */
		return linaddr;
	}

	if(pr) {
		/* Requested address is in a process that is not currently
		 * accessible directly. Grab the PDE entry of that process'
		 * page table that corresponds to the requested address.
		 */
		assert(pr->p_seg.p_cr3_v);
		pdeval = pr->p_seg.p_cr3_v[I386_VM_PDE(linaddr)];
	} else {
		/* Requested address is physical. Make up the PDE entry. */
		pdeval = (linaddr & I386_VM_ADDR_MASK_4MB) | 
			I386_VM_BIGPAGE | I386_VM_PRESENT | 
			I386_VM_WRITE | I386_VM_USER;
	}

	/* Write the pde value that we need into a pde that the kernel
	 * can access, into the currently loaded page table so it becomes
	 * visible.
	 */
	assert(get_cpulocal_var(ptproc)->p_seg.p_cr3_v);
	if(get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] != pdeval) {
		get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] = pdeval;
		*changed = 1;
	}

	/* Memory is now available, but only the 4MB window of virtual
	 * address space that we have mapped; calculate how much of
	 * the requested range is visible and return that in *bytes,
	 * if that is less than the requested range.
	 */
	offset = linaddr & I386_VM_OFFSET_MASK_4MB; /* Offset in 4MB window. */
	*bytes = MIN(*bytes, I386_BIG_PAGE_SIZE - offset); 

	/* Return the linear address of the start of the new mapping. */
	return I386_BIG_PAGE_SIZE*pde + offset;
}
```

逐段解码。

**入口三条**（两个断言 + 一次取槽）：`free_pde_idx` 必须落在已拨槽位数之内，取出的槽号必须在页目录范围内。它检查的是"下标合法"，不检查"槽位当前是否空闲"——空闲性由调用者纪律保证（第 2.4 节的两个调用点各占一槽，互不重叠）。

**特判分支**（第 1.1 节对照表的前两行落地）：目标是 ptproc 本人，或 `iskernelp(pr)` 为真的内核进程，函数**原样返回 `linaddr`，一个条目都不写**。`iskernelp` 的定义在 `minix3/minix/kernel/proc.h:275`：

```c
/*
 * 逐字引用，minix3/minix/kernel/proc.h:275。
 */
#define iskernelp(p)	  ((p) < BEG_USER_ADDR)
```

比较的是**进程槽指针**与进程表的分段地址：内核 task 占据进程表的低段槽位，它们的代码与数据在内核段里，而内核段在每张表里都有映射——所以"内核进程的地址"在当前表里永远现成。三个分支的判据全部落在"翻译是否现成"上，与特权级无关。

**借值分支与造值分支**（第三行的两条路）：目标是离位进程时，`pdeval = pr->p_seg.p_cr3_v[I386_VM_PDE(linaddr)]`——从**对方的页目录**里把对应槽位的现成条目抄出来（能这么做，是因为 `p_cr3_v` 和 §2.1 一样是内核可解引用的虚拟地址；`I386_VM_PDE(v)` 即 `v >> 22`，`minix3/minix/include/arch/i386/include/vm.h:62`）。目标是物理地址时没有现成条目可抄，按 4 MB 大页的格式**现造**一个：物理基址（按 4 MB 对齐截断）拼上 `BIGPAGE | PRESENT | WRITE | USER`。

**写入与 changed**（机制的心脏）：把 `pdeval` 写进**当前表**（`get_cpulocal_var(ptproc)->p_cr3_v`）的 `freepdes[free_pde_idx]` 号槽。写入前先比较旧值——只有真的变了才置 `*changed = 1`；调用方见到 changed 才刷 TLB（第 2.4 节）。同内容重写不刷新，这条比较是窗口机制唯一的 TLB 开销闸门。

**截断与返回**：算出目标在 4 MB 块内的偏移 `offset`，把 `*bytes` 截断到本窗口内可见的剩余量，返回窗口内的起始线性地址 `I386_BIG_PAGE_SIZE*pde + offset`。调用方拿到的可能只是一次部分可见，剩下的字节等下一轮循环再借窗。

用一组数走一遍（借的是 `freepdes[0]` 号槽；为让数字可读，**假设这个槽的槽位号是 0**——实际值来自 `kinfo.freepde_start`，在内核映射之后（01 篇 §2.6），返回地址的整体高度随之不同，算式形状不变）：

| 步 | 算式 | 结果 |
|---|---|---|
| 找对方的 PDE 下标 | `0x01234567 >> 22` | `4` |
| 偏移 | `0x01234567 & 0x003FFFFF` | `0x234567` |
| 本轮可见量 | `0x400000 - 0x234567` | `0x1CBA99`（约 1.85 MB，16 MB 请求被截到不足两成） |
| 返回地址 | `4 MB × 0 + 0x234567` | `0x234567` |

也就是说调用方这一轮只能在 `0x234567` 起拷 `0x1CBA99` 字节，下一轮带着剩余字节数重来——10 MB 的请求约三轮、16 MB 的请求四轮，每轮两次 `createpde` 加一次可能的 `reload_cr3`。

**不看代码想不到的一条**：特判分支返回的 `linaddr` 是**对方进程的 VA 原样**——一个看起来"属于别人地址空间"的地址被直接当返回值用出去。它能成立，恰因为对方就是当前页表的主人；这条返回值路径就是第 1.1 节"困难只属于第三行"的代码形态。

如果反过来想：把写入前的旧值比较删掉、每次都置 `changed`——后果不在正确性而在性能结构：`lin_lin_copy` 每轮循环（`memory.c:192`）与 `vm_memset` 每轮（`memory.c:559`）都会执行 `reload_cr3()`，把整张 TLB 里所有非全局项反复作废；跨进程拷贝的热循环因此每次迭代多一次全量 TLB 冷启动。反过来，若既不比较也不清理地复用窗口（见 §2.5 的反向）才是正确性问题。

### 2.4 两个消费现场：`lin_lin_copy` 与 `vm_memset`

`minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy`（节选；第一段为入口断言组，第二段为循环核心；省略号处为 `RTS_SLOT_FREE`/`RTS_VMINHIBIT` 的断言组与 `CONFIG_SMP` 的 stale-TLB 位处理）：

```c
/*
 * 节选，minix3/minix/kernel/arch/i386/memory.c:lin_lin_copy。
 * 三处省略号：① read_cr3 断言之后的 procslot 取值与 RTS_SLOT_FREE/RTS_VMINHIBIT
 * 断言组；② while 循环内、4MB 建窗之前的 CONFIG_SMP stale-TLB 位处理；
 * ③ PHYS_COPY_CATCH 之后的页错误归属判定（EFAULT_SRC/EFAULT_DST）。
 */
static int lin_lin_copy(struct proc *srcproc, vir_bytes srclinaddr, 
	struct proc *dstproc, vir_bytes dstlinaddr, vir_bytes bytes)
{
	u32_t addr;
	proc_nr_t procslot;

	assert(get_cpulocal_var(ptproc));
	assert(get_cpulocal_var(proc_ptr));
	assert(read_cr3() == get_cpulocal_var(ptproc)->p_seg.p_cr3);
	...
	while(bytes > 0) {
		phys_bytes srcptr, dstptr;
		vir_bytes chunk = bytes;
		int changed = 0;
		...
		/* Set up 4MB ranges. */
		srcptr = createpde(srcproc, srclinaddr, &chunk, 0, &changed);
		dstptr = createpde(dstproc, dstlinaddr, &chunk, 1, &changed);
		if(changed)
			reload_cr3(); 

		/* Check for overflow. */
		if (srcptr + chunk < srcptr) return EFAULT_SRC;
		if (dstptr + chunk < dstptr) return EFAULT_DST;

		/* Copy pages. */
		PHYS_COPY_CATCH(srcptr, dstptr, chunk, addr);
		...
```

三个读点：

- 入口断言 `read_cr3() == ptproc->p_cr3`：整条机制只在"硬件寄存器里装的正是 ptproc 的表"时合法。这个等式的维持者是 `minix3/minix/kernel/arch/i386/klib.S:__switch_address_space`——真实切表后更新 ptproc（第 1.4 节已引）。也就是说，跨空间拷贝永远发生在"当前表 = ptproc 表"的上下文里，`createpde` 写 `ptproc->p_cr3_v` 才等价于写"当前生效的表"。
- 每轮两借：src 固定 0 号槽、dst 固定 1 号槽；两方向互不踩踏。
- `changed` 才 `reload_cr3()`：刷新是按需的。`minix3/minix/kernel/arch/i386/mpx.S:reload_cr3`（其上注释横幅与之同名）的实现共六条指令、三个动作——读 CR3、原样写回、返回；写回同值即触发一次全量 TLB 失效（全局页除外）。

清零路径 `minix3/minix/kernel/arch/i386/memory.c:vm_memset`（节选；省略号处为入口的 `check_resumed_caller`、端点解析与模式扩展）：

```c
/*
 * 节选，minix3/minix/kernel/arch/i386/memory.c:vm_memset。
 * 省略号：whoptr 为 NULL 时的 panic 路径，以及循环之后的收尾断言；
 * 之前的入口检查（check_resumed_caller、端点解析）与 pattern 扩展亦省略。
 */
	while (left > 0) {
		new_cr3 = 0;
		chunk = left;
		ptr = createpde(whoptr, cur_ph, &chunk, 0, &new_cr3);

		if (new_cr3)
			reload_cr3();

		/* If a page fault happens, pfa is non-null */
		if ((pfa = phys_memset(ptr, pattern, chunk))) {

			/* If a process pagefaults, VM may help out */
			if (whoptr) {
				vm_suspend(caller, whoptr, ph, count,
						   VMSTYPE_KERNELCALL, 1);
				assert(catch_pagefaults);
				catch_pagefaults = 0;
				return VMSUSPEND;
			}
			...
```

与拷贝路径的两个差别：只借 0 号槽（单方向）；页错误不走 `EFAULT` 返回而是转 `vm_suspend`，把"目标页还没落位"的现场交给 VM 处理并返回 `VMSUSPEND`——挂起之后如何恢复是另一套协议，归 `29-cross-space-runtime`（VMSUSPEND 协议，尚未重生成），本篇止步于此。

**不看代码想不到的一条**：`lin_lin_copy` 的源与目的若有一个本来就是当前表主人（比如把 A 进程的数据拷给正在当 ptproc 的 VM），对应方向的 `createpde` 走特判直接返回，`changed` 保持另一个方向的状态——两槽设计下"借窗"与"免借"可以按方向独立发生，调用方代码完全一样。

如果反过来想：src 与 dst 共用同一个槽（都传 0）——第二轮 `createpde(dstproc, ...)` 会把 dst 的窗口写进同一个槽位，把 src 刚建立的映射覆盖掉；此后 `srcptr` 指向的翻译已不存在，拷贝实际发生的是"把 dst 的内容拷到 dst"，静默产错。`free_pde_idx` 参数与两个字面量取值（`memory.c:189-190`）就是这条禁令的执行形式。

### 2.5 清理：`mem_clear_mapcache` 与它的触发面

`minix3/minix/kernel/arch/i386/memory.c:mem_clear_mapcache`（全函数，13 行）：

```c
/*
 * 逐字引用，minix3/minix/kernel/arch/i386/memory.c:mem_clear_mapcache。
 */
void mem_clear_mapcache(void)
{
	int i;
	for(i = 0; i < nfreepdes; i++) {
		struct proc *ptproc = get_cpulocal_var(ptproc);
		int pde = freepdes[i];
		u32_t *ptv;
		assert(ptproc);
		ptv = ptproc->p_seg.p_cr3_v;
		assert(ptv);
		ptv[pde] = 0;
	}
}
```

函数体只有一件事：把 `freepdes[0]`、`freepdes[1]` 两个槽位在 ptproc 页目录里的条目写 0。三个读点：

- 它**不刷 TLB**。写 0 只改了内存里的目录条目；已经缓存在 TLB 里的旧翻译不受影响。安全性由 §2.3 的旧值比较间接保证：条目清零后，下一次借窗写入的 `pdeval` 几乎必然与 0 不同，`changed` 置位、`reload_cr3` 执行，连同旧翻译一起失效。
- 它**不清 `nfreepdes`**。槽位编号的账（§2.2 拨的两条记录）与窗口条目是两份独立状态；清的是后者。若连账一起清，`createpde` 入口的 `assert(free_pde_idx < nfreepdes)` 从此必然失败——清理动作会把使用机制一起废除。
- 触发面在 VM 侧。VM 的包装函数 `minix3/minix/servers/vm/pagetable.c:pt_clearmapcache`（全函数，pagetable.c:751–759 共 9 行）把注释的意图说全了：

```c
/*
 * 逐字引用，minix3/minix/servers/vm/pagetable.c:pt_clearmapcache。
 */
void pt_clearmapcache(void)
{
	/* Make sure kernel will invalidate tlb when using current
	 * pagetable (i.e. vm's) to make new mappings before new cr3
	 * is loaded.
	 */
	if(sys_vmctl(SELF, VMCTL_CLEARMAPCACHE, 0) != OK)
		panic("VMCTL_CLEARMAPCACHE failed");
}
```

VM 在五个位置调用它：`servers/vm/pagetable.c:118`（`pt_assert` 的自检前）、`servers/vm/main.c:212`（live update 恢复时重绑页表后）、`servers/vm/main.c:719`（live update 换绑新旧两个 `vmproc` 后）、`servers/vm/main.c:749`（信号处理里补 spare 页之后）、`servers/vm/pagefaults.c:153`（页错误处理路径）。五个位置的共同点：页目录的归属或内容刚被 VM 大规模改过，内核窗口里缓存的映射整体不可信。

**不看代码想不到的一条**：清理的"作废"是三级的——条目写 0（本函数）、TLB 失效（推迟到下次借窗的 changed）、语义信任（由 VM 宣告）。内核自己永远不会主动走到这条路上：它不知道 VM 何时换了页目录，`do_vmctl.c` 的注释原文（§2.0 已引）把这条知识边界写成了注释。

如果反过来想：假如没有这个清理、窗口条目也不清零——live update 把进程槽与页目录换绑之后，窗口里缓存的可能是一条指向**已被复用的目录页**的旧翻译；下一次跨空间访问若旧值比较碰巧相等（同槽恰有同值），`changed` 不置位、TLB 不刷，内核把 A 进程的内容当 B 的读写，没有任何报错。清理 + 旧值比较两道闸合起来，把"缓存窗口"的信任窗口收窄到两次 VM 通告之间。

### 2.6 本章归纳

四步总表（每行给回指）：

| 阶段 | 函数（锚点） | 触发 | 改了什么状态 | 64 位侧的接手节 |
|---|---|---|---|---|
| 记录 | `minix3/minix/kernel/arch/i386/protect.c:arch_post_init` | boot 一次 | ptproc = VM；`p_cr3`/`p_cr3_v` 写入 VM 槽 | §3.5（记录点后移）、§3.6 |
| 分配 | `minix3/minix/kernel/arch/i386/memory.c:memory_init` | boot 一次 | `freepdes[0..1]` = 两个槽位号 | §3.1、§3.6 |
| 使用 | `minix3/minix/kernel/arch/i386/memory.c:createpde` | 每次访问 | 当前表的窗口槽条目；`changed` → TLB | §3.1（常数加法取代） |
| 清理 | `minix3/minix/kernel/arch/i386/memory.c:mem_clear_mapcache` | VM 请求 | 窗口条目写 0（不动 TLB、不动槽位账） | §3.6（随机制废弃） |

这套机制的成立条件，[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 归纳过三条（当前表里有连续空槽、窗口只在当前 CR3 下有效、借出者记录并保证归还）；本章走查补上第四条：**TLB 一致性由使用侧的旧值比较保证**——它分布在 `createpde` 的比较与 `lin_lin_copy`/`vm_memset` 的 `changed` 消费两处，清理函数反而不管。四个条件分布四个函数，单看任何一个函数都凑不齐全图；这是"跨函数隐性契约"的具体形状，也是第 3 章衡量"哪些可以死"时的底账：常数加法要取代的不是某个函数，是这四条契约的全体。

## 3. Rust 设计决策

第 1 章结尾说过，常数加法取代的是四条跨函数契约的全体。本章按决策逐条展开；每个决策的第一句都回到第 2 章里 C 侧真实存在的语义或负担。

### 3.1 决策一：用常数加法取代借窗口的循环（→§2.3、§2.4）

C 侧的负担（§2.3、§2.4 已走查）：每次跨空间访问要走"写 PDE → 旧值比较 → 按需刷 TLB → 截断传输块 → 算窗口内地址"，10 MB 拷贝循环约三轮，每轮两次借窗；机制的存在还要求 §2.6 的四条隐性契约同时在位。Rust 的选择：跨空间访问的地址换算退化为 `VA = 基址 + PA` 的一次加法——窗口在 boot 期一次建成、内容永不改变，于是"借"这个动作整个消失，四条契约随之失去存在理由：不需要空槽（窗口不是临时的）、不需要"只在当前 CR3 有效"（每张表都自带全量窗口）、不需要借出者记账（没有账可记）、不需要旧值比较（内容永不变，TLB 永不因窗口失效）。

随之而来的是另一个子决策：**不为死掉的机制立替身抽象**。C 有 `arch_post_init`/`memory_init` 两个阶段函数，Rust 的对应物是一个确认函数（§3.5）；设计过程中明确否决了为它们各造一个 trait（`PostInitArch`/`MemoryInitArch` 这类形状）——`os/kernel/src/lib.rs:init_post_and_memory` 的 Step 2 行内注释把这条否决写成了正文："It is NOT part of the createpde temporary-window mechanism (that arch layer is gone — `PostInitArch`/`MemoryInitArch` were superseded by Direct Map)"。否决的理由：阶段 D 在各架构上只剩几条断言，没有任何架构差异可抽象；为一个确认步骤立 trait，是用抽象层假装还存在一个机制。

| 取舍 | 选了什么 | 否掉了什么 | 代价 |
|---|---|---|---|
| 地址换算 | 常数加法（trait default 方法，§3.3） | 保留借窗机制的 64 位版 | 每张进程页表多出两个窗口的映射（§3.4 由 `map_kernel` 复刻承担） |
| 临时窗口机制 | 整体废弃（含清理协议） | 兼容层（保留 `createpde` 式 API） | 调用面（`22-syscall-copy`，尚未重生成）按新原语重写，无迁移期 |
| 阶段函数替身 | 不引入 `PostInitArch`/`MemoryInitArch` | 为确认步骤立 trait | 无——确认逻辑本就无架构差异 |

如果反过来想：64 位上保留窗口机制——每张进程页表都得预留两个窗口槽位（否则切表后窗口失效），`map_kernel` 的复刻清单里多一类"内容会变的条目"，而"内容会变的条目"与 §3.4 要建立的"内核窗口 PTE 永不改"不变量直接冲突；同时每次跨空间访问仍要一次全量 TLB 刷新，§2.3 手算过的三轮循环一次不少。常数加法不是把 C 函数改写一遍，是让它服务的循环不再需要存在。

### 3.2 决策二：双视图地址空间——U/S 位一次一值逼出两个窗口（→§2.3）

C 侧的语义（§2.3 特判分支）：内核在 CPL0 上读当前表主人的 USER 页不需要任何额外机制。64 位侧把"谁需要通过窗口读物理内存"数了一遍，答案是两个特权级：内核（CPL0，读任意物理帧）与 VM 服务器（CPL3，它自己走查页表、清零页、维护分配器元数据——VM 是用户态进程，这正是 Minix3 的设计）。而 x86 分页的 U/S 位对**每一条 VA 翻译**取一个值：一条翻译要么 U=1（用户态可访问），要么 U=0（仅监督态），不存在"内核走它是 U=0、VM 走它是 U=1"的同一张翻译。

于是单窗口方案的两个方向各被一条 ISA 语义否掉：

- 单窗口 U=1：用户态的任何代码都能经窗口直读全部物理内存——内核镜像、别的进程的页表、别的进程的用户数据，全部暴露。
- 单窗口 U=0：VM 每次"读自己管理的页表页"都要陷入内核代劳——VM 作为页表管理器的存在方式被拆掉。

所以是两个窗口，各带一套权限：**VM Direct Map**（user 可读写，落在用户虚拟区）服务 CPL3 的 VM；**Kernel Direct Map**（supervisor 可读写）服务内核。权限构造函数把这两套要求写成了两个值（`os/arch/src/arch/paging.rs:PageFlags`，节选）：

```rust
    // 节选，os/arch/src/arch/paging.rs:PageFlags 的两个构造函数。
    // （文档注释与函数体均按原文；省略号处为 read_only/kernel_read_only 等
    //   其余构造函数。）
    /// User-space read-write page: PTF_PRESENT | PTF_USER | PTF_WRITE.
    /// Corresponds to Minix3 `PTF_PRESENT|PTF_USER|PTF_WRITE`.
    pub const fn read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }
    ...
    /// Kernel read-write page (global): PTF_PRESENT | PTF_WRITE | PTF_GLOBAL.
    pub const fn kernel_read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::GLOBAL.bits()
        )
    }
```

（`os/arch/src/arch/paging.rs:PageFlags:read_write` 与 `:kernel_read_write`，节选。VM 窗口用前者：USER 位置位；内核窗口用后者：无 USER 位、带 GLOBAL 位。一处省略说明更正：`kernel_read_write` 的源注释只有第一行，"Corresponds to Minix3" 那行是 `read_write` 才有的。）

GLOBAL 位是这条决策里的另一半：它让内核窗口的 TLB 项跨 CR3 切换存活。C 侧对应物是 `vm_enable_paging` 里的 PGE 语义（`minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`，[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6 讲过置位次序，此处引用）。差别在于范围：C 的全局页语义没有"窗口"可依附，Rust 把"内核映射永不改"收成一个不变量写进了 `map_kernel` 的文档注释——内核 direct map 的 PTE 建成后不再修改，这正是 G=1 安全的前提。

**窗口放在哪**：x86-64 上内核镜像链接着内核半区基址 `0xFFFF_8000_0000_0000`（PML4[256] 槽），而镜像的映射公式是 `VA = kern_virt_base + offset`，**不**遵循 DM 的 `VA = DM基址 + PA`。若把 Kernel DM 也放进 PML4[256]，两条公式在同一条翻译上相撞：DM 建立时的叶安装会与镜像自身的映射冲突。所以内核窗口挪到下一个顶层槽：`0xFFFF_8080_0000_0000` 即 PML4[257]。这里有一处旧稿地址需要裁定：早期文档写过 Kernel DM 基址 `0xFFFF_8000_0000_0000`——那正是内核镜像自己的槽，按 `os/arch/src/arch/direct_map.rs` 的现值为准（x86-64 与 aarch64 均为 `0xFFFF_8080_0000_0000`；riscv64 因 Sv39 的地址宽度不同，见 §4.1，且它的旧值错法与 x86-64 同构——`direct_map.rs` 的 riscv64 文档注释明写旧基址 `0xFFFF_FC00_0000_0000` 的 39 位载荷恰好解码到镜像所在的 VPN[2]=256 槽）。

如果反过来想（三问按节逐条）：

- 若把两个窗口合成一个、U=1：用户态直读全部物理内存，第 3.1 节"内核窗口 PTE 永不改"的不变量还在，但内核完整性没了——任何 CPL3 代码可以把内核数据窗口当输入。
- 若把两个窗口合成一个、U=0：VM 的每次页表走查、每次清零都变成一次系统调用，VM 服务器退化成内核的壳。
- 若 Kernel DM 与镜像共占 PML4[256]：`establish_boot_dm` 向该槽装叶时与镜像映射冲突，或 DM 覆盖静默改写镜像翻译——两种结局都由 §4.3 的 `AlreadyMapped` 拒启或第一次取指失败呈现。
- 若窗口都放进用户区（省掉内核半区槽位）：U/S 判定要求内核窗口 U=0，而用户区段目录项在 VM 自己建的表里可被 VM 改写——信任边界依赖的是"槽位在内核半区"这一位置事实，不是权限位协商。

还有一处时序事实要按现码说清（`os/kernel/src/lib.rs` 里 `init_post_and_memory` 的 Step 3 注释写"窗口在 Phase C 建立"——该注释与现码有偏离，现码实际是 boot Step 4 建立、Phase C 只记录根地址；偏离的完整登记在 §4.2）：两个窗口都建立在 bootstrap 根上，VM 在运行期认领这张根作为自己的页表，所以"VM 的初始页表里有 VM 窗口"这句话成立，但建立动作不发生在 Phase C。

### 3.3 决策三：`DirectMapArch` 只抽象布局差，不抽象机制（→§3.1、§3.2）

C 侧的对应物（§2.6）：临时窗口机制没有任何"每架构一份"的形状——i386 与 earm 各有一份实现文件，但那是两份完整机制，不是"一份机制 + 每架构参数"。64 位侧剩下的架构差异被数到只剩一组常量：窗口基址（两个）与窗口尺寸（一个）、外加随之而来的堆区基址与尺寸。决策三：这组差异收进一个 trait，方法只留"由常量派生"的换算；大页能力（页尺寸、PTE 位、CPUID 探测）归 `HugePages`，覆盖建立（怎么把窗口写进页表）归 `DmCoverageArch`——三件事三个 trait，谁也不越界（`os/arch/src/arch/direct_map.rs:DirectMapArch`，`[ARCH: A-10]`：C 的双宏族变为按架构的 trait 实现）。

```rust
// 签名示意，os/arch/src/arch/direct_map.rs:DirectMapArch（文档注释有删节；
// 方法体在 §4.1 逐字给出）。
pub trait DirectMapArch {
    const VM_DIRECT_MAP_BASE: u64;
    const KERNEL_DIRECT_MAP_BASE: u64;
    const VM_HEAP_BASE: u64;
    const VM_DIRECT_MAP_SIZE: u64;
    const VM_HEAP_SIZE: u64;

    fn vm_phys_to_virt(phys: PhysBytes) -> VirBytes { ... }
    fn kernel_phys_to_virt(phys: PhysBytes) -> VirBytes { ... }
    fn virt_to_phys(virt: VirBytes) -> PhysBytes { ... }
}
```

三个换算方法里，反向的那个要逐行看——`os/arch/src/arch/direct_map.rs:DirectMapArch:virt_to_phys`（逐字引用）：

```rust
    fn virt_to_phys(virt: VirBytes) -> PhysBytes {
        if virt.get() >= Self::KERNEL_DIRECT_MAP_BASE {
            PhysBytes::new(virt.get() - Self::KERNEL_DIRECT_MAP_BASE)
        } else {
            PhysBytes::new(virt.get() - Self::VM_DIRECT_MAP_BASE)
        }
    }
```

判定用一个阈值比较完成：VA 高于等于内核窗口基址就走内核分支，否则走 VM 分支。两个读点：第一，这里**没有**独立的 `is_kernel_va` 谓词——"是不是内核 VA"与"减哪个基址"是同一次比较；落点要按数值算、不能按"属于内核"算：内核镜像与栈地址 `0xFFFF_8000_…` **低于**内核窗口基址 `0xFFFF_8080_…`，它们并不落进第一个分支，而是掉进 VM 分支——§3.2 末尾的基址裁定正是为了避免这类地址被窗口错误接住。第二，判别顺序是载荷正确的关键：内核栈地址（`0xFFFF_8000_…`）低于内核窗口基址，落进 else 的 VM 分支后减出的是垃圾"物理值"——这正是协调记录 E8 那次 `rep movsb` `#GP` 的机制；若再把比较反过来（先判"落在 VM 窗口区间"），一个高于 VM 窗口上限、又低于内核 DM 基址的地址会两头都不命中而减出垃圾物理值。这不是理论边界——协调记录（`rewrite-notes/coordination/edge_todo.md`，其中编号 E8 的条目）里有一轮真机取证：一处内核侧代码把 higher-half 内核栈地址误走了 DM 换算，得到非规范别名，`rep movsb` 直接 `#GP`。

如果反过来想：把"换算函数"升格成"每架构一套换算机制"（比如让某架构用寄存器基址、另一架构用查表）——trait 的常量形状就装不下了，方法得拆进各架构 impl；而现码三架构的换算**就是同一个减法**，唯一的架构差异是常量本身。抽象层与差异的真实大小对齐，是这个 trait 形状的判据：常量能表达的差异不造方法，方法能表达的不立机制。

### 3.4 决策四：`map_kernel` 复刻内核映射，内核无常驻页表（→§2.6）

C 侧的对照（§2.6 终点）：C 里"每张进程页表都映射内核段"由 VM 的 `pt_mapkernel` 完成，且 VM 还要从 `kernel_boot_info.freepde_start` 起预留一批页目录槽给内核做窗口（`minix3/minix/servers/vm/pagetable.c:freepde`，供 `pt_allocate_kernel_mapped_pagetables` 使用）——预留的正是第 2 章那套借窗机制的 VM 侧账目。Rust 侧的对应动作是 `os/arch/src/arch/paging.rs:map_kernel`（对应 C 的 `pt_mapkernel`）：给每张新建的进程页表复刻四段映射——内核 text、内核 data、**内核 DM 窗口**、恒等运行窗口。窗口段的循环是三行：

```rust
    // 节选，os/arch/src/arch/paging.rs:map_kernel 的 DM 窗口段
    // （省略号处为 text 段、data 段与恒等运行窗口段）。
    for i in 0..dm_pages {
        let vaddr = VirBytes(dm_vbase + i as u64 * page_size);
        let paddr = PhysBytes(i as u64 * page_size);
        pt.map(vaddr, paddr, PageFlags::kernel_read_write())?;
    }
    ...
```

（`os/arch/src/arch/paging.rs:map_kernel`。`VA = dm_vbase + i·页`、`PA = i·页`——DM 公式的逐页展开；flags 用 §3.2 的 `kernel_read_write()`，GLOBAL 位随每张表生效。）

两个设计点：

- **窗口覆盖量不再是常量**。`dm_pages` 从交接凭证（`VmBootHandoff` 的 `kern_dm_pages` 字段）来，其值由 `os/kernel/src/dm_coverage.rs:kernel_dm_pa_end` 现算：把与 `establish_boot_dm` 完全相同的四个候选源函数再迭代一遍，取最高页对齐终点（函数体走查在 §4.4）。这样保证"VM 复刻的窗口"与"内核 boot 期建成的窗口"覆盖同一片 PA——切换到任何一张 VM 建的表之后，内核经 DM 窗口能访问的物理范围不缩水。
- **内核自己仍无常驻页表**。boot 期那张 bootstrap 根被 VM 认领后永久存在（没有 destroy 路径，VM 进程槽的 `phys_root` 一直指着它）；内核的运行始终踩在"当前进程的表"上，与 C 同构——变的只是"表里预映射了什么"：从"内核段 + 两个借窗槽"变成"内核映射 + 全量 DM 窗口"。

如果反过来想：`dm_pages` 用一个写死的小值（历史上的实现真用过 4 页哨兵）——第一次切到 VM 建的进程表后，内核经窗口访问 16 KiB 以上的任何物理地址都是 not-present；`kernel_dm_pa_end` 的文档注释记录了这个前科（"the previous hardcoded 4-page sentinel left every address above 16 KiB not-present after the first CR3 switch"，逐字见 §4.4）。反方向（把窗口扩到整个物理地址空间的理论上限）则让每张进程页表背起用不到的映射量，表本身可能大到建不出来。覆盖量跟着"内核实际会访问的 PA"走，是两侧共用候选源的理由。

### 3.5 决策五：阶段 D 从两个函数缩成三条断言（→§2.1、§2.2）

C 侧的现状（§2.1、§2.2）：Phase D 位置上是两个函数——`arch_post_init`（记录 ptproc 与页目录地址）与 `memory_init`（拨两个窗口槽）。Rust 侧把这个时点保留下来，但内容缩成一个确认函数 `os/kernel/src/lib.rs:init_post_and_memory`（对应 C 两个函数的名字合在了一起）。§1.4 的两用途判定在这里兑现：

- **记录动作移走**。ptproc 的 Rust 记录点是 `os/kernel/src/lib.rs:switch_address_space`——每次真实的根切换都伴随一次 `set_current_ptproc_nr`（下引）；AP 侧则在各自启动尾部直接装 VM（`os/kernel/src/smp.rs` 的 AP 尾段，注释自称 "C arch_post_init parity"）。记录点跟着"硬件根真的变了"走，而不是跟着 boot 时序走。
- **确认动作留下**。三条断言拦 boot 布局错误：VM 根的物理地址非零、VM 根的虚拟映射存在、VM 窗口基址已配置。

确认函数的函数体（`os/kernel/src/lib.rs:init_post_and_memory`，逐字引用；函数文档注释省略——其中含本文档名的引用与仓库内部编号，见块后说明）：

```rust
#[cfg(not(feature = "mock"))]
pub fn init_post_and_memory(proc_table: &crate::proc_table::ProcessTable) {
    use minix_arch::{CurrentDirectMap, DirectMapArch};

    // Step 1: Assert VM's page-table root is valid.
    //
    // The root was installed in Phase C (`init_proc_and_boot`): for the
    // non-mock path, `p_seg.phys_root` records the bootstrap root and
    // `p_seg.virt_root` its identity-mapped VA (lib.rs:960-963). Both must
    // be present before VM can be trusted as the page-table process.
    let vm_proc = proc_table.get(crate::proc::proc_nr::VM_PROC_NR)
        .expect("VM process must be initialized before init_post_and_memory");
    assert!(
        vm_proc.p_seg.phys_root.0 != 0,
        "VM page-table root (phys) must be valid after stage C"
    );
    assert!(
        vm_proc.p_seg.virt_root.is_some(),
        "VM page-table root (virt) must be kernel-mapped after stage C"
    );

    // Step 2: Record VM as the kernel-level ptproc.
    //
    // C: get_cpulocal_var(ptproc) = vm — protect.c:372 (x86) / protect.c:99 (ARM)
    //
    // This is the kernel-level counterpart of C's per-CPU `ptproc`: it lets
    // `dispatch_vmctl(SetAddrSpace)` Step 3 decide whether to reload the
    // hardware root register when the target process is the current ptproc.
    // It is NOT part of the createpde temporary-window mechanism (that arch
    // layer is gone — `PostInitArch`/`MemoryInitArch` were superseded by
    // Direct Map), so it survives Direct Map (07-cross-space-init.md §1.4, P9-4).
    //
    // SAFETY: BKL is held during boot, only this CPU accesses the global.
    // S-6.1 D-40: the slot lives in CpuLocal — installed for the BSP in
    // `init_smp_state` (the SMP state must exist first; C: arch_post_init
    // per-CPU parity).

    // Step 3: Assert the VM Direct Map base is configured.
    //
    // The VM Direct Map window is established in Phase C when the kernel
    // builds VM's initial page table (07-cross-space-init.md §3.2). Its
    // base is a per-architecture compile-time constant; this check is a
    // documented-contract assertion that fails fast at boot if a future
    // architecture ever configures a zero base.
    assert!(
        CurrentDirectMap::VM_DIRECT_MAP_BASE != 0,
        "VM direct map base must be configured"
    );
}
```

（块内注释含仓库内部编号 `P9-4`、`S-6.1 D-40`、对本篇旧编号 `§3.2` 的引用与两处 C 坐标，均按原文引用。一处坐标偏移照抄未改：注释写 `protect.c:372 (x86) / protect.c:99 (ARM)`，而 `get_cpulocal_var(ptproc) = vm` 的赋值行实际在 protect.c:375 / earm protect.c:102——372/99 是函数体首行注释的位置。）

逐段读，并把两处**注释与函数体的偏离**点出来：

- Step 1 是两条断言，读 VM 槽的 `p_seg.phys_root` 与 `p_seg.virt_root`（这两个字段由 Phase C 的 `init_proc_and_boot` 在装载 VM ELF 时写入）。注释与函数体一致，但注释里的坐标 `(lib.rs:960-963)` 已陈旧——这两个字段的实际写入点在 `lib.rs:1997/2000`（`init_proc_and_boot` 内），960-963 现在是 kmain 的 rdmsr 探针；与 Step 2/3 的偏离同款登记。
- Step 2 的注释标题是"Record VM as the kernel-level ptproc"，但**注释下面没有一行代码**——函数体在这里什么都没做。这不是引用错误：`set_current_ptproc_nr` 的生产调用点全部在 `switch_address_space`（下引），[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §3.12 已核实并按"记录在 switch_address_space"的口径改正；本篇沿用同一口径，引用这条注释时注明偏离。
- Step 3 是第三条断言 `VM_DIRECT_MAP_BASE != 0`。注释自称 documented-contract assertion——它拦的不是现码的 bug，而是"未来某架构把基址配成零"的契约违约。注释里"窗口在 Phase C 建立"的说法与现码有偏离（§3.2 末已记，§4.2 的时序图给准确位置）。

记录点的真实形状（`os/kernel/src/lib.rs:switch_address_space`，节选；省略号处为按索引取槽与根为零时的提前返回）：

```rust
    ...
    let live_root = unsafe { minix_arch::CurrentTlbArch::get_active_root() };
    if live_root == root {
        // already active — skip the TLB flush (klib.S:618-620).
        ...
        set_this_cpu_root_phys(root);
        crate::set_current_ptproc_nr(nr);
        return;
    }
    set_active_root_tracked(root);
    crate::set_current_ptproc_nr(nr); // klib.S:622-624
```

（`os/kernel/src/lib.rs:switch_address_space`。两条路径——"根已在位，跳过重载"与"真实切换"——都以 `set_current_ptproc_nr` 收尾，对应 C `setcr3` 的 `p == ptproc` 判据与 `klib.S` 的 `__switch_address_space` 入口（汇编入口在真实切表之后把第二个参数指向的槽写为新表主人：`mov 8(%esp), %eax; mov %edx, (%eax)`，即 `*__ptproc = p`——klib.S:622-624）。消费点在 VM 换表请求：`os/kernel/src/syscall.rs` 的 SetAddrSpace 分支以 `current_ptproc_nr() == Some(p.p_nr)` 决定是否只更新软件镜像而跳过硬件重载——C `arch_do_vmctl.c:setcr3` 的对位。）

如果反过来想，两个方向各有一条：

- 把 `set_current_ptproc_nr` 放回 `init_post_and_memory`（让注释成真）：boot 期只有 BSP 在跑，写的是 BSP 的 per-CPU 槽；AP 各自的槽仍要靠 AP 尾段安装——同一份"记录"出现两个写入者、两个时机，而 `switch_address_space` 每次切换仍要再写一次。记录点放回 boot 只会多出一个冗余写入点，不会少掉切换点的写入。
- 删掉三条断言：VM 根无效或窗口基址为零的 boot 布局错误，要到第一次跨空间访问（或 VM 首次自走页表）才以 `#PF` 的形式出现，现场从"Phase D 的断言行号 + panic 文本"变成"运行期 CR2 值 + 调用栈回溯"。保留一个 boot 期确认点的价值就是把这类错误的取证面压回启动阶段。

### 3.6 废弃清单与存活项（→第 2 章各节）

把第 2 章出现过的每个机制逐项对账（处置档位沿用本项目"废弃 / 改型 / 存活"三档）：

| C 侧机制 | C 锚点 | 处置 | 现码落点 / 理由 |
|---|---|---|---|
| `freepdes[]`/`MAXFREEPDES`/`nfreepdes` | `memory.c:29-31` | 废弃 | 窗口临时性消失，无槽可拨 |
| `memory_init` | `memory.c:707` | 废弃 | 无分配动作可做；函数名并入确认函数（§3.5） |
| `createpde` | `memory.c:69` | 废弃 | 常数加法取代（§3.1、§3.3 的三个换算方法） |
| `mem_clear_mapcache` + `VMCTL_CLEARMAPCACHE` | `memory.c:35`、`do_vmctl.c:162` | 废弃 | 窗口内容永不改变，无缓存可清 |
| `pg_info` 的 `p_cr3_v` 半（内核可写视图） | `pg_utils.c:312` | 废弃 | 内核写 PTE 走恒等写通道与 DM，不再依赖"目录的 VA 记录" |
| `pg_info` 的 `p_cr3` 半（物理根） | 同上 | 改型 | 成为 `p_seg.phys_root`（Phase C 写入，§3.5 Step 1 断言的对象）；`virt_root` 是它的伴生确认字段 |
| ptproc 的窗口用途 | `cpulocals.h:ptproc` | 废弃 | 无窗口可写、无"已可见"特判需求 |
| ptproc 的切表判据用途 | `arch_do_vmctl.c:setcr3` | 存活 | per-CPU `CpuLocal.ptproc`（§3.5；AP 尾段同装） |
| 4 MB `BIGPAGE` 窗口概念 | `vm.h:I386_BIG_PAGE_SIZE` | 改型 | 粒度选择归 `plan_range_units` 的 1 GiB/2 MiB/4 KiB 贪心（§4.3） |
| `kinfo.freepde_start` | `minix3/minix/include/minix/param.h:27` | 改型 | 协议字段 `KernelInfo.free_upper_idx`（`Option<usize>`）保留为 boot-shim 镜像契约；内核侧消费者只剩 GET_KINFO 的报告（`os/kernel/src/misc.rs`）——字段语义与保留判据见 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §3.9 |

表里有两行"改型"要多说一句。`p_cr3` → `phys_root` 不只是改名：C 的记录动作在 Phase D（`arch_post_init`），Rust 的写入点在 Phase C（装载 VM ELF 时），消费点从"内核写窗口条目"变成"调度器切根 + Phase D 断言"。`freepde_start` → `free_upper_idx` 则是**语义孤儿**：机制死了，字段因为跨镜像协议（boot-shim 与内核对同一份 `KernelInfo` 布局的承诺）而保留——这也是全表里唯一"内核侧无消费者仍然存活"的行。

如果反过来想（恢复实验）：恢复表中任何一行，都要连着恢复它的配套组件——恢复 `mem_clear_mapcache` 需要同时恢复窗口槽记录、`VMCTL_CLEARMAPCACHE` 子命令与 VM 侧的四个调用点；恢复 ptproc 的窗口用途需要 `p_cr3_v` 一类的"内核可写目录视图"回到每张进程页表上。反方向的推演确认了这张表不是"删了一些函数"，而是一套机制的四个环节一起退场；单独复活任何一环，缺了其余三环都无法工作。

## 4. 实现详解

### 4.1 `DirectMapArch` 的接口与三架构地址表（→§3.3）

§3.3 给了 trait 的骨架，本节把它落成可查的表。x86-64 的实现整块逐字引用（常量、编译期断言与其上的布局论证注释都在，`os/arch/src/arch/direct_map.rs:X86_64DirectMap`）：

```rust
/// x86-64 Direct Map address space layout.
///
/// VM direct map sits at 2GB in user space; kernel direct map occupies the
/// PML4 slot immediately after the kernel image's slot: the kernel image is
/// linked at the kernel-half base `0xFFFF_8000_0000_0000` (PML4[256], see
/// `os/kernel/src/arch/x86_64/link.ld`), so the DM window must NOT share
/// that slot — boot maps the image there with `VA = kern_virt_base + offset`
/// translations that do not follow DM semantics (`VA = DM base + PA`).
/// `0xFFFF_8080_0000_0000` is PML4[257], a slot the bootstrap root never
/// touches outside DM establishment (07-paging_init_design §6.1).
#[derive(Default)]
pub struct X86_64DirectMap;

impl DirectMapArch for X86_64DirectMap {
    const VM_DIRECT_MAP_BASE: u64 = 0x0000_0000_8000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8080_0000_0000;
    const VM_HEAP_BASE: u64 = 0x0000_0000_C000_0000;
    const VM_DIRECT_MAP_SIZE: u64 = 0x0000_0000_4000_0000; // 1 GiB
    const VM_HEAP_SIZE: u64 = 64 * 1024 * 1024;
}

const _: () = assert!(
    <X86_64DirectMap as DirectMapArch>::VM_HEAP_BASE
        == <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_BASE
            + <X86_64DirectMap as DirectMapArch>::VM_DIRECT_MAP_SIZE,
    "x86-64: VM_HEAP_BASE must immediately follow the VM direct map window"
);
```

（块内注释末尾的 `07-paging_init_design §6.1` 是仓库内部设计文档引用，按原文保留；该文档属 02-stage 的设计底稿，本篇不直接引用其内容。）

三架构的完整地址表（常量值全部取自 `os/arch/src/arch/direct_map.rs` 的三个 impl 块，复算命令见 §6.2）：

| 架构 | VM_DM_BASE | VM_DM_SIZE | VM_HEAP_BASE | KERNEL_DM_BASE | 内核窗口顶层槽 | U/S |
|---|---|---|---|---|---|---|
| x86-64 | `0x0000_0000_8000_0000` | 1 GiB | `0x0000_0000_C000_0000` | `0xFFFF_8080_0000_0000` | PML4[257] | VM 窗 U=1 / 内核窗 U=0 |
| aarch64 | `0x0000_1000_0000_0000` | 2 GiB | `0x0000_1000_8000_0000` | `0xFFFF_8080_0000_0000` | L0[257] | 同上 |
| riscv64 | `0x0000_0010_0000_0000` | 16 GiB | `0x0000_0014_0000_0000` | `0xFFFF_FFC0_4000_0000` | Sv39 VPN[2]=257 | 同上 |

三个尺寸各有一句平台理由，写在各自 impl 块或 `dm_coverage` 模块注释里：x86-64 的 1 GiB 让 VM 窗口恰好落在用户区 2 GiB 处（`[2 GiB, 3 GiB)`），VM 堆紧随其后（编译期断言盯住 `VM_HEAP_BASE == VM_DM_BASE + VM_DM_SIZE`）；aarch64 的 2 GiB 是因为 QEMU virt 的 RAM 基址在 1 GiB——1 GiB 窗口会把整个 RAM 排除在 DM 可表示之外，VM 的物理内存管理就没有可管理的对象；riscv64 的 16 GiB 来自 Sv39 内核半区 256 GiB 的布局（内核 DM 基址在半区顶端之下 16 GiB），`os/kernel/src/dm_coverage.rs` 的模块注释顺手把这条上限讲成了窗口的 PA 边界：当前平台的 RAM 远低于它。

手算一遍（用第 4.5 节会再遇到的一个真实 PA）：`PA = 0x1DE0_E000`，x86-64 上——

- 内核别名：`0xFFFF_8080_0000_0000 + 0x1DE0_E000 = 0xFFFF_8080_1DE0_E000`；
- VM 别名：`0x0000_0000_8000_0000 + 0x1DE0_E000 = 0x9DE0_E000`，且 `0x1DE0_E000 < 0x4000_0000`（1 GiB），在 VM 窗口可表示范围内。

riscv64 的基址错法单独记一行，但要先把数字算对：旧值 `0xFFFF_FC00_0000_0000` 的 39 位载荷是 `0x0`，顶层下标 VPN[2] = **0**——这个值是非规范的、不落在任何合法 Sv39 槽上，与 x86-64 旧值"撞镜像所在的 256 号槽"并不是同一类错法。`os/arch/src/arch/direct_map.rs` 的 riscv64 文档注释把两件事写在了一起（"non-canonical"与"payload decodes to VPN[2] = 256"），这两个说法按算术不能同时成立，属**注释自身待回修**的一处；本篇按算术为准登记。现值 `0xFFFF_FFC0_4000_0000` 的 VPN[2] = 257，与另两个架构的"窗口不与镜像共槽"判据对齐。

如果反过来想：删掉那条 `VM_HEAP_BASE == VM_DM_BASE + VM_DM_SIZE` 编译期断言——两个常量可以各自漂移，VM 把堆页当 DM 页（或反之）的错误要到运行期的地址解算才暴露；`direct_map.rs` 里的一条注释记录了这个前科：窗口尺寸曾经硬编码在 VM 服务器本地，在大窗口架构上悄悄漂移，后来才收编进 trait。

### 4.2 阶段 D 入口与全链时序（→§3.5）

§3.5 已把 `init_post_and_memory` 的函数体逐段读过；本节补它的调用位置与整条时间线。调用位置在 `os/kernel/src/lib.rs` 的 kmain 内（其前方的 Phase D 注释块自报家门）：`init_post_and_memory(proc_table);`——其上一行的注释自报家门："Phase D: arch_post_init + memory_init → Direct Map readiness check"，并给了两侧 C 坐标（`protect.c:370`、`memory.c:707`，与第 2 章的锚点一致）。

从 boot-shim 交出到 VM 建起第一张进程表，两个窗口与 ptproc 的全部时间点如下（对照 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §3.12 的转换形状）：

```
boot-shim → arch_boot(kernel_info, root_page)
  Step 0    校验 KernelInfo、登记分配器
  Step 1–3  建 bootstrap 根（恒等 + 内核高半）并开启分页
  Step 4    establish_boot_dm —— 两个窗口装进 bootstrap 根
            validate_bootstrap_tree：根页与 bump 区必须在
            min(IDENTITY_MAP_END, VM 窗口) 之内
  → jump_to_kmain（高半区）
kmain
  Phase A/B  校验、保护结构、时钟、中断、arch 层初始化
  Phase C    init_proc_and_boot：
             · VM ELF 逐页装进 bootstrap 根（paging.map）
             · 写 VmBootHandoff 交接帧（kern_dm_pages = kernel_dm_pa_end）
             · proc.p_seg.phys_root / virt_root 记根地址
  Phase D    init_post_and_memory —— 三条断言（§3.5）
  Phase F    bsp_finish_booting → switch_to_user
首次调度     switch_address_space(VM)：
             live root == VM 根 → 跳过重载；set_current_ptproc_nr(VM)
             （BSP 的 ptproc 记录在此刻落地，不在 Phase D）
VM 运行期    init_page_table → map_kernel 复刻四段映射（含内核 DM 窗口）
             → 之后每个进程的页表都带同样的窗口
```

三个时序事实按这张图钉住：

- **窗口先于一切消费者**。`establish_boot_dm` 在 Step 4、kmain 之前跑完；Phase C 装载 VM ELF 用的 `paging.map`、VM 运行后的页表走查，用的都是已经建好的窗口。`arch_boot_impl` 里 Step 4 的注释把次序约束写成一行："Must run after `enable()` … and before any VM physical access through the windows"。
- **BSP 的 ptproc 落在首次调度**。§3.5 说过 Step 2 注释与函数体偏离——真正的落点是图上"首次调度"那一拍；AP 侧则在各自启动尾部直接装 VM（`os/kernel/src/smp.rs` 的 AP 尾段）。换句话说，"ptproc = VM"这个 boot 收尾状态，在 Rust 里由两条路径分别达成：BSP 经首次切换、AP 经启动尾段。
- **Phase C 只记根、不建窗口**。`os/kernel/src/lib.rs` 里 Step 3 的注释写"窗口在 Phase C 建立"（`init_post_and_memory` 逐字引用块里那段），按图上的真实位置，这句话与现码有偏离：窗口在 Step 4 建立且先于 kmain，Phase C 做的是把根地址记进 VM 槽、写交接帧。引用该注释时以此为准。

如果反过来想：把 `init_post_and_memory` 挪到 Phase F（`bsp_finish_booting` 之后）——此时 VM 已经过首次调度开始运行，"VM 根无效"这类错误的现场从 Phase D 的断言栈（panic 文本直接给出根字段名）变成 VM 用户态首跑的异常，中间多出一个已部分初始化的调度器要排查。确认步骤放在被确认对象投入使用之前，是它仅存的时序价值，也是"三条断言"替代"两个初始化函数"后保留 Phase D 时点的全部理由。

**窗口建立这一拍也有同款反推**：把 `establish_boot_dm` 从 Step 4 挪到 Phase C 之后——Phase C 装载 VM ELF 的 `paging.map` 正是窗口的第一个消费者（§3.2 末尾已按现码说明：装页表走的通道依赖窗口已在位）。窗口未建时，`paging.map` 对窗口区虚拟地址的第一次写入命中的是一个尚未建立的翻译：bootstrap 根里那个顶层槽是空的，故障以启动期缺页的形式落在建表循环里；此刻 VM 尚未运行，§2.4 那条"页错误交给 VM 处理"的 `vm_suspend` 协议不存在可调用的对端，引导只能中止。也就是说，窗口建立的位置错后不会产生更隐蔽的静默错误，而是把"Step 4 的次序断言"变成"建表循环里的缺页现场"——两处都拦得住，但后者的故障点在建表逻辑内部、与真正的布局错误混在一起，取证面更大。这正是 Step 4 注释把次序约束写成一行（"before any VM physical access through the windows"）的理由。

### 4.3 覆盖建立的三件套：`DmRange`、`plan_range_units`、`DmCoverageArch`（→§3.1、§3.3）

窗口"覆盖跟随资源区间，而不是覆盖 `[0, max_phys)`"——这句判断（`os/arch/src/arch/dm_coverage.rs` 模块注释的小节标题原文）由三个组件执行。第一个是范围类型及其裁剪（`os/arch/src/arch/dm_coverage.rs:DmRange:clipped_to`，逐字引用）：

```rust
    /// Clip the range to `[0, pa_limit)`. Returns `None` if nothing remains.
    ///
    /// Used to constrain coverage to a window's representable PA span
    /// (`VM_DIRECT_MAP_SIZE` for the VM DM window): a conventional region
    /// extending past the window is partially covered — the remainder stays
    /// outside DM representability and is excluded from VM PMM eligibility
    /// by resource qualification.
    pub fn clipped_to(self, pa_limit: u64) -> Option<DmRange> {
        if self.base >= pa_limit {
            return None;
        }
        let end = core::cmp::min(self.base.checked_add(self.len)?, pa_limit);
        Some(DmRange { base: self.base, len: end - self.base })
    }
```

一个越过窗口上限的常规内存区会被**部分**覆盖：窗口内的部分拿到映射，窗口外的部分留在可表示范围之外，并因此被 VM 物理内存管理的资格过滤排除——映射覆盖量与资源资格在这里是同一条边界。

第二个是贪心的粒度选择（`os/arch/src/arch/dm_coverage.rs:plan_range_units`，逐字引用）：

```rust
pub fn plan_range_units<F: FnMut(u64, usize) -> Result<(), PageTableError>>(
    range: DmRange,
    one_gb: bool,
    page_size: u64,
    mut emit: F,
) -> Result<(), PageTableError> {
    debug_assert!(
        range.base.is_multiple_of(page_size) && range.len.is_multiple_of(page_size),
        "DM candidate range must be page-aligned"
    );
    const TWO_MB: u64 = 1 << 21;
    const ONE_GB: u64 = 1 << 30;
    let mut cur = range.base;
    let end = range.base + range.len;
    while cur < end {
        let remaining = end - cur;
        if one_gb && cur.is_multiple_of(ONE_GB) && remaining >= ONE_GB {
            emit(cur, ONE_GB as usize)?;
            cur += ONE_GB;
        } else if cur.is_multiple_of(TWO_MB) && remaining >= TWO_MB {
            emit(cur, TWO_MB as usize)?;
            cur += TWO_MB;
        } else {
            emit(cur, page_size as usize)?;
            cur += page_size;
        }
    }
    Ok(())
}
```

每个发出的 `(物理地址, 尺寸)` 单元都整体落在候选区间内——大页叶只有在"对齐且剩余量足够"时才发出。"洞不被吞"的执行点就在这个循环里：一个不属于任何候选的洞（例如 x86 legacy 的 `[0xA0000, 0x100000)`），在 1 GiB、2 MiB、4 KiB 任何粒度上都不会成为某个单元的一部分。手排一次：候选 `[0, 0x40_0000)`（2 MiB 对齐、4 MiB 长）在 `one_gb = false` 时发出 `(0x0, 2 MiB)`、`(0x20_0000, 2 MiB)` 两个 2 MiB 叶——零 4 KiB 叶。

第三个是按架构装叶的 trait 与驱动（`os/arch/src/arch/dm_coverage.rs:establish_dm_range`，逐字引用）：

```rust
pub fn establish_dm_range<P: DmCoverageArch>(
    root: PhysBytes,
    range: DmRange,
    window_pa_limit: u64,
    va_base: u64,
    flags: PageFlags,
) -> Result<(), PageTableError> {
    let Some(clipped) = range.clipped_to(window_pa_limit) else {
        return Ok(());
    };
    plan_range_units(clipped, P::supports_1gb_page(), 4096, |pa, size| {
        // SAFETY: boot context per `dm_install_leaf` contract — paging is
        // enabled on `root`, all table pages satisfy the allocation bound,
        // and the caller guarantees single-threaded boot.
        unsafe { P::dm_install_leaf(root, VirBytes(va_base + pa), PhysBytes(pa), size, flags) }
    })
}
```

`DmCoverageArch` 的实现者（x86-64 侧 `os/arch/src/x86_64/paging.rs:X86_64DmCoverage`）要把每个单元写到 bootstrap 根的表树里，且**不得使用正在建立的窗口本身**——所有表访问走恒等写通道（`VA = PA`），这条自举约束写在 trait 的合同注释里。x86-64 实现里最重的一段是 VM 窗口与恒等映射的重叠善后（`os/arch/src/x86_64/paging.rs:X86_64DmCoverage::dm_install_leaf` 的拆分分支，节选；省略号处为对齐检查、PML4 层处理与 1 GiB 整叶替换分支；块内注释含仓库内部评审记录的编号标记，按原文引用）：

```rust
        // Split path (the x86-64 norm — the legacy hole [0xA0000, 0x100000)
        // fails the resource-containment condition): demote a pre-existing
        // 1 GiB leaf into an EMPTY lower table. The identity translation
        // for VA [2 GiB, 3 GiB) is wholesale superseded by DM semantics;
        // the replacement table keeps only what subsequent allowed units
        // re-map, so a reserved hole is never swallowed (Gate 8 Case E,
        // mechanism level).
        let pd = if huge3 {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; replacing the identity 1 GiB
            // leaf with a table pointer (no PS).
            unsafe {
                write_entry(
                    pdpt_ptr,
                    i3,
                    phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                )
            };
            ...
```

三个读点：**为什么有拆分**——x86-64 的 VM 窗口 `[2 GiB, 3 GiB)` 与恒等映射的 `PDPTE[2]` 是同一个槽，恒等建表在这里写的是一条 1 GiB 大页叶；**为什么换"空表"**——恒等翻译在这个 VA 段被整体退役，新表只保留后续合法 DM 单元写进去的内容，恒等时代的保留区翻译不会借拆分之机回到窗口里；**为什么中间层要带 U 位**——U/S 位跨层级做与运算，恒等建起的中间层是监督态，若不补 USER，VM 窗口叶上的 U=1 走查时仍会被上层抵消（实现里 `branch_flags` 的注释原话："U/S is ANDed across levels"）。

如果反过来想：贪心选择只查对齐不查"剩余量足够"（删掉 `remaining >= TWO_MB` 那半个条件）——候选 `[0x40_0000, 0x50_0000)`（起点 2 MiB 对齐、长度只有 1 MiB）会发出一条 2 MiB 叶，把 `[0x50_0000, 0x60_0000)` 这段候选之外的物理区间一并映射进去，"覆盖跟随资源"的边界被冲开一 MiB。真实的洞形状（x86 PC 的常规 `[0, 0xA0000)` + 洞 `[0xA0000, 0x100000)` + 常规 `[0x100000, …)`，`os/arch/src/arch/dm_coverage.rs` 的测试 `test_legacy_hole_never_swallowed` 用的正是这组数）在正确实现下每条单元都保持精确边界。拆分分支若拷贝恒等叶的映射而不是发空表——保留区翻译整体搬进新表，"洞不被吞"以另一种方式失败。两个方向都收敛到同一条不变量：窗口里只该有候选资源自己。

### 4.4 `establish_boot_dm`：安装次序、验证与调用点（→§3.2、§3.4）

四个候选源怎么变成两个窗口，主体在 `os/kernel/src/dm_coverage.rs:establish_boot_dm`（逐字引用到第四源循环为止；省略号处为一段 `#[cfg(not(feature = "mock"))]` 的调试探针残迹——打印 bump 区间与裁剪清单的循环体已被清空但结构保留，块注释里记着它的由来与一次多核早期启动的实测记录名；函数尾部的 `validate_bootstrap_tree(root);` 调用随后单独逐字）：

```rust
pub fn establish_boot_dm(kernel_info: &KernelInfo, root: PhysBytes) {
    let kernel_flags = PageFlags::kernel_read_write();
    let vm_flags = PageFlags::read_write();
    let kernel_va = CurrentDirectMap::KERNEL_DIRECT_MAP_BASE;
    let vm_va = CurrentDirectMap::VM_DIRECT_MAP_BASE;
    let vm_pa_limit = CurrentDirectMap::VM_DIRECT_MAP_SIZE;

    // Kernel DM first (§6.1 constraint ①), full PA span.
    for r in memmap_candidates(kernel_info) {
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window establishment failed");
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window establishment failed");
    }

    // VM DM second, PA span clipped to the window; candidates beyond the
    // window stay outside DM representability (resource qualification
    // excludes them from VM PMM — §6.1 资格过滤).
    for r in memmap_candidates(kernel_info) {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window establishment failed");
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window establishment failed");
    }
    for r in boot_module_candidates(kernel_info, root) {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window module coverage failed");
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window module coverage failed");
    }

    // Source 4: reserved/occupied regions enter the KERNEL DM only.
    // The VM DM deliberately stays on sources 1–3 — VM PMM resource
    // qualification must keep treating non-conventional memory as
    // unrepresentable (§6.1 资格过滤); the kernel is the only accessor of
    // its own image/stack pages through the DM alias.
    for r in reserved_regions_candidates(kernel_info, root) {
        establish_dm_range::<CurrentDmCoverage>(root, r, u64::MAX, kernel_va, kernel_flags)
            .expect("boot DM: kernel window reserved coverage failed");
    }
    ...
```

（块内注释含仓库内部设计文档节号与圈号标记，按原文引用；一处内部记录名在引用时省去。）

安装次序的三个不变量，每个都对应一段代码：

- **内核窗先行，全 PA 跨度**（前两个循环，`window_pa_limit = u64::MAX`）：恒等映射退役后，内核对 PA 的访问要立即有内核通道可走；这条次序约束在 `os/kernel/src/dm_coverage.rs` 模块注释里以"Kernel DM is established first"起头。
- **VM 窗按 `VM_DIRECT_MAP_SIZE` 裁剪**（第三、四个循环）：`vm_pa_limit` 传给 §4.3 的 `clipped_to`，窗外部分不映射、也不进 VM 的资源资格。
- **第四源只进内核窗**（最后的循环）：保留区（内核镜像与栈所在的非常规内存）的 DM 别名只有内核会用；VM 的资格过滤语义保持"只认常规内存"，不受第四源影响。注释里 "the kernel is the only accessor" 说的就是这条边界。

四个 `.expect` 各自的价值在 §4.3 的合同注释里：叶冲突（`AlreadyMapped`）在单候选趟过新窗口的正常布局下不该发生，发生即 boot 布局 bug，拒启。

尾部的验证（`os/kernel/src/dm_coverage.rs:validate_bootstrap_tree`，逐字引用）：

```rust
fn validate_bootstrap_tree(root: PhysBytes) {
    let bound = core::cmp::min(IDENTITY_MAP_END, CurrentDirectMap::VM_DIRECT_MAP_SIZE);
    assert!(
        root.0.checked_add(PAGE_SIZE).is_some_and(|end| end <= bound),
        "boot DM: bootstrap root {:#x} outside DM-admissible bound {:#x}",
        root.0,
        bound
    );
    if let Some((base, end)) = boot_alloc::boot_alloc_region() {
        assert!(
            end <= bound,
            "boot DM: boot bump region [{base:#x}, {end:#x}) outside DM-admissible bound {bound:#x}"
        );
    }
}
```

自举树的两个成员（根页与 bump 区）必须同时满足两个可达性：恒等写通道够得着（`PA < IDENTITY_MAP_END`，`os/kernel/src/lib.rs` 的 `IDENTITY_MAP_END = 0x1_0000_0000`）且 VM 窗口表示得了（VM 自走页表要经窗口读写根与页表页）。验证界取两者的 `min`；boot-shim 侧在分配时执行同一条界，内核侧在建窗后复核——两道检查独立存在，谁也不信任谁的执行。

调用点的现算口径（`grep -n 'establish_boot_dm' os/kernel/src/dm_coverage.rs os/kernel/src/lib.rs` 的输出见 §6.2）：

| 调用点 | 上下文 |
|---|---|
| `os/kernel/src/lib.rs:567`（`arch_boot_impl` Step 4） | 生产主路径：x86-64 与 riscv64 的 `arch_boot`、aarch64 直启路径、宿主 mock 测试都汇到这里 |
| `os/kernel/src/lib.rs:384`（`arch_boot_resume_high_half`） | 生产 aarch64 内核镜像高半启动路径：根由 boot-shim 建好并已在用，这里把 DM **增量**装上去（调用点上方注释原话："`establish_boot_dm` wraps `root_page` and only writes new PTEs"） |
| `os/kernel/src/dm_coverage.rs:747`（测试 `test_establish_boot_dm_reserved_is_kernel_window_only` 内） | 宿主测试 |

`establish_boot_dm` 的函数文档注释写"Called once from `arch_boot_impl`"——这句在第二条生产路径加入后已过时，以现码的两条生产路径为准。

窗口覆盖量还派生出交接侧的一个字段：`os/kernel/src/dm_coverage.rs:kernel_dm_pa_end`（逐字引用；函数文档注释含仓库内部记录名与一句关键前科，注释在此省略、要点在块后引用）：

```rust
pub fn kernel_dm_pa_end(kernel_info: &KernelInfo, root: PhysBytes) -> u64 {
    let mut max_end = 0u64;
    let mut note = |r: DmRange| {
        max_end = max_end.max(r.base.saturating_add(r.len));
    };
    for r in memmap_candidates(kernel_info) {
        note(r);
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        note(r);
    }
    for r in boot_module_candidates(kernel_info, root) {
        note(r);
    }
    for r in reserved_regions_candidates(kernel_info, root) {
        note(r);
    }
    max_end.div_ceil(PAGE_SIZE) * PAGE_SIZE
}
```

四个候选源再迭代一遍，取最高终点向上页对齐。它的消费者是交接帧：`os/kernel/src/vm_handoff.rs` 里 `kern_dm_pages` 字段的赋值正是 `kernel_dm_pa_end(...) / PAGE_SIZE`，字段注释的要点原文是"base AND coverage must equal what `establish_boot_dm` installed on the bootstrap root (same candidate union …)"。函数文档注释里记着这条链的前科：这个值曾用一个 4 页的哨兵顶替，原文是 "the previous hardcoded 4-page sentinel left every address above 16 KiB not-present after the first CR3 switch"——VM 按 `map_kernel` 复刻的窗口一旦比内核实际建的窄，切表动作本身就是内核窗口的截断。

如果反过来想：保留区若进 VM 窗口——VM 的物理内存资格以候选源为语义边界，非常规内存从此"可表示"，分配器可能把内核镜像或栈所在的物理页发给请求者；安装次序若 VM 先内核后——恒等映射退役发生时内核通道尚未就位，退役瞬间对退役区 PA 的任何内核访问悬空。两条反向都撞在"谁先可用"这同一面墙上，方向不同而已。

### 4.5 四个候选源与下溢修复（→§4.4）

候选源函数都在 `os/kernel/src/dm_coverage.rs`，签名一致（返回候选区间的迭代器），过滤规则一致（与更早的源重叠即整段丢弃——并集语义；二次安装同一叶槽会触发 §4.3 的 `AlreadyMapped` 拒启）。逐个过：

- **源 1：常规内存**（`memmap_candidates`）。直接映射 `KernelInfo.memmap()`——boot-shim 已经做过类型过滤，这里不再筛。
- **源 2：自举页表树**（`bootstrap_tree_candidates`，全函数逐字引用）：

```rust
fn bootstrap_tree_candidates(kernel_info: &KernelInfo, root: PhysBytes) -> [Option<DmRange>; 2] {
    let in_memmap =
        |r: &DmRange| memmap_candidates(kernel_info).any(|m| r.base < m.base + m.len && m.base < r.base + r.len);

    let root_range = DmRange::new(root.0, PAGE_SIZE);
    let root_end = root.0 + PAGE_SIZE;
    let bump_range = boot_alloc::boot_alloc_region().and_then(|(base, end)| {
        if root.0 >= base && root.0 < end {
            // Root inside the bump: front-trim past the root page. The trim
            // is guarded (start < end) so an empty remainder stays `None`
            // instead of wrapping into a bogus giant range.
            let start = core::cmp::max(base, root_end);
            (start < end).then(|| DmRange::new(start, end - start))
        } else {
            // Disjoint allocations: the whole bump range needs coverage.
            Some(DmRange::new(base, end - base))
        }
    });

    let mut out = [None, None];
    if !in_memmap(&root_range) {
        out[0] = Some(root_range);
    }
    if let Some(r) = bump_range.filter(|r| !in_memmap(r)) {
        out[1] = Some(r);
    }
    out
}
```

根页与 bump 区的关系只有两种真实形状：根从 bump 里拨出（升序分配，根是第一页）→ 两候选重叠，bump 候选跳过根页向前裁剪，根叶只装一次；根独立分配且在 bump **上方**（UEFI 垫片的 `MaxAddress` 分配是降序的：根页先拨、落在空闲低内存顶部，bump 在它之下）→ 区间不相交，bump 全量保留。函数的文档注释还记了第三种形状：根页严格落在 bump 中段时区间要裂成两段，而 `DmRange` 槽对表达不了三段——代码选择"发出 `[base, root_base)`、放弃根以上余段并在注释里记形状"，因为没有现行分配器产生这种布局。

- **源 3：boot 模块 blob**（`boot_module_candidates`）。模块由垫片以装载器数据页分配装载，不在常规 memmap 快照里；VM 读模块镜像（boot 进程的 exec 路径）直接走 VM 窗口——一个没被覆盖的 blob 页就是窗口上的一个洞，第一次 ELF 魔数读取就在 CPL3 缺页。函数文档注释里记着一次真机取证：缺页地址 `0x9de0e000` 恰是 §4.1 手算里那个 blob 页的 VM 别名（`VM_DM_BASE + PA`）。
- **源 4：保留区**（`reserved_regions_candidates`）。垫片把 UEFI 内存图里"非常规、非 MMIO"的描述符归进 `reserved_regions`——内核镜像与栈的落点就在其中。没有源 4 时，这些页的恒等翻译活着（取指访存都走它），但 `kernel_phys_to_virt` 一类的 DM 别名访问会撞 not-present；一次多核早期启动的取证里，复现轮次的缺页地址全部落在"已装候选之间的缝隙"上，对应的正是这个缝隙形状。

函数文档注释里另有一条工程约束（原文引用，`os/kernel/src/dm_coverage.rs:boot_module_candidates` 文档注释；含仓库内部记录名 `fix16` 与日期，按原文）："Heap-free by necessity: this runs inside `arch_boot` before the kernel heap exists — a `collect` here is a 64-byte `handle_alloc_error` boot failure"。四个候选函数全部用迭代器 filter 合成，不收集、不排序、不分配。

**下溢修复**是源 2 的核心教学点。把这个比较去掉会怎样：前裁剪被无条件执行（`start = max(base, root_end)`，不看根是否在 bump 之内）；在"根在 bump 上方"的形状下，`start` 被推到根页之上、越过 `end`，`end - start` 以无符号算术回绕成巨数——候选变成一个"空转零叶"的假区间：`establish_dm_range` 收到它，`plan_range_units` 的 `while cur < end` 一次都不进，bump 区零映射。后果链（每环都可从码上指认）：运行期分配的页表页来自 bump 区 → 这些 PA 在内核窗口里没有别名 → 内核走查器第一次经窗口访问运行期页表页就缺页——一次真机取证记录的缺页地址 `0xffff8080_0dfb4ff8` 正是内核别名格式（`0xFFFF_8080_…`）加上一个 bump 区内的 PA（这条缺陷的根因记录见协调文件 `rewrite-notes/coordination/edge_todo.md`，编号 E8 的条目）。修复语义就是现在函数体里的那个分支：**仅当根落在 bump 内才裁剪，且裁剪带 `start < end` 守卫**；不相交时全量保留。`os/kernel/src/dm_coverage.rs` 的测试 `test_union_keeps_full_bump_when_root_above_bump` 用当次真机实测的真实地址（bump `[0x0dfaf000, 0x0dfef000)`、根页 `0x0e789000`）把这条语义钉成回归（§5.2 细读）。

如果反过来想，两条：

- 把修复写成"回绕时钳回 `base`"而不是"不相交时全量保留"——bump 的中后段仍然没有候选，洞缩小但还在，走查缺页从必现变偶发（取决于分配器把页表页落在 bump 的哪一段），取证难度反而上升。
- 候选汇总若用 `Vec` 收集——发生在内核堆存在之前，`handle_alloc_error` 的 boot 失败（前科见上文引用的文档注释）；迭代器 filter 不只是风格，是这个执行点的生存条件。

## 5. 测试要点

### 5.1 宿主测试读数与口径

覆盖建立的测试分布在两侧，各 10 条（读数为实跑输出，复算命令见 §6.2）：

| 侧 | 测试 | 钉住的形状 |
|---|---|---|
| kernel（`os/kernel/src/dm_coverage.rs` tests 模块） | `test_union_drops_in_memmap_bump` | bump 落在常规内存内时被源 1 吸收，源 2 让位（并集语义） |
| 同上 | `test_union_keeps_full_bump_when_root_above_bump` | 根在 bump 上方时 bump 全量保留（下溢回归，§5.2） |
| 同上 | `test_boot_module_candidates_union` | 窗口外模块保留、与候选重叠的模块丢弃 |
| 同上 | `test_kernel_dm_pa_end_covers_all_sources` | 覆盖量跟随四源并集的最高终点 |
| 同上 | `test_reserved_regions_candidates_union` | 缝隙形保留区保留、与常规内存或模块重叠的整段丢弃 |
| 同上 | `test_reserved_kernel_dm_installs_gap_leaf` | 保留区叶真的装进内核窗、不双装 |
| 同上 | `test_establish_boot_dm_reserved_is_kernel_window_only` | 端到端：第四源只在内核窗、覆盖量跟随（§5.2） |
| 同上 | `test_boot_dm_x86_64_style_params` / `..._aarch64_...` / `..._riscv64_...` | 三组平台形参数走完 `arch_boot_impl` 不 panic |
| arch（`os/arch/src/arch/dm_coverage.rs` tests 模块） | `test_two_mb_region_uses_two_mb_units`、`test_one_gb_selection`、`test_unaligned_fragments_degrade_to_4k` | 粒度贪心的三档 |
| 同上 | `test_window_clipping` | 窗口上限裁剪 |
| 同上 | `test_bootstrap_tree_fragment_coverage` | 槽对的两个 `Option` 语义 |
| 同上 | `test_legacy_hole_never_swallowed` | 真实 x86 布局的洞保持精确边界 |
| 同上 | `test_establish_conflict_propagates` 等 4 条 driver 组 | 驱动层的 VA 偏移、冲突传播、窗口上限 |

两侧分工：arch 侧只看"给一个范围怎么发出叶"，kernel 侧只看"给一份 KernelInfo 该挑哪些范围"。分界线是 `DmCoverageArch` 的实现者——mock 注册表（宿主）或真表走查（真机）。

### 5.2 三条关键测试钉住的形状

第一条，下溢回归（`os/kernel/src/dm_coverage.rs:test_union_keeps_full_bump_when_root_above_bump`，节选；省略号处为 `KernelInfo` 的字段构造；测试文档注释与断言消息含一次真机实测的记录名与地址，按原文保留）：

```rust
        // Real test-user-trap numbers: bump [0x0dfaf000, 0x0dfef000) (64
        // pages), root page 0x0e789000 — above the bump end, outside the
        // 128 MiB memmap.
        boot_alloc::init_boot_pt_alloc(0x0dfa_f000, 0x0dfe_f000);
        let cands = bootstrap_tree_candidates(&info, PhysBytes(0x0e78_9000));
        assert_eq!(cands[0], Some(DmRange::new(0x0e78_9000, PAGE_SIZE)));
        assert_eq!(
            cands[1],
            Some(DmRange::new(0x0dfa_f000, 0x0dfe_f000 - 0x0dfa_f000)),
            "disjoint bump must be covered in full, not trimmed/underflowed"
        );
        ...
```

数字是真机当次的实数：bump 64 页、根页在 bump 终点之上、128 MiB 常规内存之外。第二个断言的消息原文把修复语义写全了——"disjoint bump must be covered in full, not trimmed/underflowed"；把 §4.5 否决的无条件 `max(base, root_end)` 写法代回去，`cands[1]` 变成回绕后的巨区间或 `None`，这条断言变红。

第二条，第四源的窗口归属端到端守卫（`os/kernel/src/dm_coverage.rs:test_establish_boot_dm_reserved_is_kernel_window_only`，节选；省略号处为数据构造与 `establish_boot_dm` 调用；块内注释含仓库内部记录名，按原文引用）：

```rust
        let leaves = minix_arch::arch::dm_coverage::mock::mock_dm_leaves();
        assert!(
            leaves.contains_key(&(CurrentDirectMap::KERNEL_DIRECT_MAP_BASE + 0x1f00_0000)),
            "reserved gap range must be installed in the kernel window"
        );
        assert!(
            !leaves.contains_key(&(CurrentDirectMap::VM_DIRECT_MAP_BASE + 0x1f00_0000)),
            "source 4 must stay out of the VM window — non-conventional memory \
             has to remain unrepresentable to VM PMM qualification"
        );
        assert!(
            kernel_dm_pa_end(&info, root) >= 0x1f00_0000 + 0x4000,
            "kernel_dm_pa_end must follow source 4 so the handoff's kern_dm_pages \
             covers the installed kernel-window leaf set"
        );
        ...
```

测试自己的文档注释说明了它为什么存在：候选级的测试看不出"第四源被整体挪进 VM 窗"或"覆盖量计算漏掉第四源"这类端到端偏移——两者在那之前都没有测试能拦。三条断言正好一一对应三个不许发生的偏移。

第三条，覆盖量的算术（`os/kernel/src/dm_coverage.rs:test_kernel_dm_pa_end_covers_all_sources` 的核心两行，节选）：

```rust
        boot_alloc::init_boot_pt_alloc(0x2000, 0x100_0000); // inside memmap → dropped
        assert_eq!(kernel_dm_pa_end(&info, PhysBytes(0x1000)), 0x1de5_0000,
            "module end 0x1de0e000+0x42000=0x1de50000 is the highest candidate");
```

bump 落在常规内存内被丢弃后，最高候选只剩模块 blob：终点 `0x1de0e000 + 0x42000 = 0x1de50000`，页对齐后原样返回。这个数就是交接帧里 `kern_dm_pages × 4096` 的值——测试把 §4.4 的派生链钉成了等式。

### 5.3 测试边界与结构性消失的错误类别

mock 门怎么接：`os/arch/src/arch/direct_map.rs` 之外的 `MockDmCoverage`（`os/arch/src/arch/dm_coverage.rs` 的 mock 模块）把每个叶记进进程全局注册表而不碰真表；kernel 包对 `minix-arch` 的常规依赖是 `default-features = false`（真实现），`mock` feature 才追加 `runtime-window`（`os/kernel/Cargo.toml` 的 feature 接线，注释里写明"生产形态 = `--no-default-features`"）。测试用 `lock_boot_globals` 串行化——boot 分配区与叶注册表都是进程全局，清理由锁的获取方负责。

宿主能证什么、真机才能证什么：候选选择、裁剪、粒度、覆盖量算术、窗口归属在宿主全可证（注册表就是叶集合的真值）；真表走查、恒等写通道、CPUID 的 1 GiB 探测、拆分分支在真机上的 TLB 后果，宿主证不了——那半边由 `os/qemu-tests/` 的启动链测试（如 `test-user-trap.sh`、`test-rt-birth.sh` 一类全链载体）以"能不能活着跑到用户态"覆盖。

结构性消失的错误类别：临时窗口机制下的"借窗后忘刷 TLB""两个方向共用槽位""清理后槽位记录错乱"这类错误，在测试清单里没有对应物——不是漏测，是那些状态在常数加法世界里不存在，测试清单因此比 C 侧同类机制短。

诚实披露一条（`os/kernel/src/dm_coverage.rs` 模块注释自述）：源 4 与更早候选部分重叠的描述符被整段丢弃后，**不重叠的尾部**没有内核窗叶——垫片的一条 UEFI 描述符经常同时盖住一个模块 blob 和几页无关内存，丢弃整段保住了并集合法性，尾部成为已知残留缺口；模块注释原文自称 "tracked as an open item, not claimed to be closed"。补齐需要做候选间的区间减法（候选数有界、仍可零堆），属后续工作，本篇如实转记、不宣布已闭。

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状（CR2 或失败形状） | 怀疑对象 | 确认动作与预期关键行 |
|---|---|---|
| 缺页，CR2 = `0xFFFF_8080_…`（内核别名格式） | 内核窗候选缺口：源 2 下溢或源 4 缝隙 | 跑 §6.2 的 kernel 侧测试；`test_union_keeps_full_bump_when_root_above_bump ... ok` 与 `test_reserved_regions_candidates_union ... ok` 应在输出中；再对照 CR2 低 32 位减去 bump/候选区间找缺口 |
| 缺页，CR2 落在 VM 窗口区间 `[0x8000_0000, 0xC000_0000)`（x86-64） | VM 窗候选缺口：模块 blob 未覆盖（历史实测值 `0x9de0e000` 即 `VM_DM_BASE + 0x1de0e000`） | 检查 `boot_module_candidates` 的过滤条件与出事模块的 PA；对照 §4.5 源 3 的说明 |
| 第一次切到 VM 建的进程表后，内核经窗口访问失败 | 交接帧的 `kern_dm_pages` 派生被改窄 | `grep -n 'kern_dm_pages' os/kernel/src/vm_handoff.rs` 应命中 `kernel_dm_pa_end(...) / PAGE_SIZE`；改动后跑 `test_kernel_dm_pa_end_covers_all_sources` |
| VM 分配器拿到非常规内存的页 | 第四源被误装进 VM 窗 | 跑 `test_establish_boot_dm_reserved_is_kernel_window_only`；VM 窗断言（`source 4 must stay out of the VM window`）变红即拦住 |
| boot 即拒启，消息 `boot DM: ... failed` | 叶冲突 = 并集过滤被破坏，同叶双装 | 沿消息文本定位是哪个窗口的哪个源循环（四条 `.expect` 消息各标一个源），再查对应候选函数的过滤条件 |
| boot 拒启，消息 `outside DM-admissible bound` | 自举树成员越界（根页或 bump 高于 `min(IDENTITY_MAP_END, VM 窗口)`） | 读 panic 文本里的两个十六进制数（实际值与上界），回 boot-shim 的分配路径 |

### 6.2 手算与常量核对命令

本篇全部关键读数的复算命令（均已实跑，读数附后）：

```bash
# 1. 三架构 DM 常量现值（§4.1 表的来源）
grep -n 'const VM_DIRECT_MAP_BASE\|const KERNEL_DIRECT_MAP_BASE\|const VM_DIRECT_MAP_SIZE' \
  os/arch/src/arch/direct_map.rs
#   → 15 行：trait 声明 3 行 + 四个 impl（x86-64/aarch64/riscv64/Mock）各 3 行；
#     三个真架构的 KERNEL_DIRECT_MAP_BASE 依次为 0xFFFF_8080_0000_0000、
#     0xFFFF_8080_0000_0000、0xFFFF_FFC0_4000_0000（direct_map.rs:82/:117/:149）

# 2. establish_boot_dm 的定义与全部调用点（§4.4 的口径）
grep -n 'establish_boot_dm' os/kernel/src/dm_coverage.rs os/kernel/src/lib.rs
#   → 10 行命中：定义 dm_coverage.rs:86；生产调用 lib.rs:384、lib.rs:567；
#     测试调用 dm_coverage.rs:747；其余 6 行为注释与测试名

# 3. kernel 侧覆盖测试（§5.1 读数）
cd os && cargo test -p minix-kernel --features mock dm_coverage
#   → test result: ok. 10 passed; 0 failed; ...; 828 filtered out

# 4. arch 侧覆盖测试（§5.1 读数）
cd os && cargo test -p minix-arch --features runtime-window,x86_64 --lib dm_coverage
#   → test result: ok. 10 passed; 0 failed; ...; 242 filtered out

# 5. C 侧两个阶段函数的调用位置（§2.0 表的来源）
grep -n 'arch_post_init();\|memory_init();' minix3/minix/kernel/main.c
#   → 283:  arch_post_init();  /  293:  memory_init();

# 6. 候选源函数清单（§4.5 的对象面）
grep -n 'fn memmap_candidates\|fn bootstrap_tree_candidates\|fn boot_module_candidates\|fn reserved_regions_candidates' \
  os/kernel/src/dm_coverage.rs
#   → :233 boot_module / :290 reserved_regions / :308 memmap / :340 bootstrap_tree

# 7. 手算核对：PA 0x1de0e000 的两个别名（§4.1）
python3 -c "print(hex(0xFFFF_8080_0000_0000 + 0x1de0e000), hex(0x0000_0000_8000_0000 + 0x1de0e000))"
#   → 0xffff80801de0e000 0x9de0e000
```

两条手算的复核入口：§2.3 的窗口算式（`linaddr = 0x0123_4567` → PDE 下标 4、偏移 `0x234567`、可见量 `0x1CBA99`）可用同一解释器按 `0x01234567 >> 22`、`0x01234567 & 0x3FFFFF`、`0x400000 - 0x234567` 逐条重放；§4.3 的叶序列（候选 `[0, 0x40_0000)` → 两个 2 MiB 叶）由 `test_two_mb_region_uses_two_mb_units` 的同形状断言向量给出。

## 7. 参见

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.6（`createpde` 借槽位窗口与 `freepde_start` 语义、恒等/内核映射的建表五步）、§3.8（页表页分配器换手）、§3.9（`free_upper_idx` 字段保留）
- [02-higher-half-kernel.md](02-higher-half-kernel.md)（内核高半区与链接布局；顶层槽位冲突的同类处理见其 §3.2–§3.3 与 §4.1）
- [03-kmain-cstart.md](03-kmain-cstart.md)（Phase A–F 的划分与本篇 Phase D 的位置）
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §3.12（`init_post_and_memory` 的口径与注释偏离登记、boot→running 转换形状）
- `rewrite-notes/02-stage-vm/07-pagetable-struct.md` §1.4（Direct Map 常量偏移的 VM 侧视角与外部对照）、`08-pagetable-ops.md`（页表操作逐函数语义）
- `22-syscall-copy`（跨空间拷贝的调用面：谁调 `lin_lin_copy` 一族，尚未重生成）
- `29-cross-space-runtime`（VMSUSPEND 挂起-恢复协议：`vm_suspend` 之后的事，尚未重生成）
- `09-system-init-boot-finish`（`memory_init` 在启动收尾里的对应物与 DM 就绪检查的时序位置，尚未重生成）
- `10-smp-boot`（AP 侧的 ptproc 安装与启动尾段，尚未重生成）



