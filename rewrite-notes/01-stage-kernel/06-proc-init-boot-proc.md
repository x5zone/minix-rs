# 06 · 进程表与 boot 进程初始化：261 个槽、16 个状态位与 17 项清单

> **C 真值源**：`minix3/minix/kernel/proc.h`、`minix3/minix/kernel/proc.c`、`minix3/minix/kernel/priv.h`、`minix3/minix/kernel/table.c`、`minix3/minix/kernel/main.c`、`minix3/minix/kernel/glo.h`、`minix3/minix/kernel/type.h`、`minix3/minix/kernel/system.c`、`minix3/minix/kernel/system/do_vmctl.c`、`minix3/minix/kernel/system/do_privctl.c`、`minix3/minix/kernel/system/do_fork.c`、`minix3/minix/kernel/system/do_clear.c`、`minix3/minix/kernel/system/do_runctl.c`、`minix3/minix/kernel/smp.c`、`minix3/minix/kernel/arch/i386/protect.c`、`minix3/minix/kernel/arch/i386/arch_system.c`、`minix3/minix/kernel/arch/i386/memory.c`、`minix3/minix/kernel/arch/i386/exception.c`、`minix3/minix/kernel/arch/earm/protect.c`、`minix3/minix/include/minix/{const.h,param.h,com.h,priv.h,endpoint.h,sys_config.h}`、`minix3/minix/servers/vm/main.c`
> **Rust 实现**：`os/kernel/src/{lib.rs,proc.rs,proc_table.rs,kpriv.rs,capability.rs,globals.rs,misc.rs}`、`os/arch/src/arch/boot.rs`、`os/arch/src/{x86_64,arm64,riscv64}/boot.rs`、`os/arch/src/lib.rs`、`os/libs/minix-types/src/types/{endpoint.rs,com.rs,proc_nr.rs}`、`os/libs/minix-boot/src/kernel_info.rs`、`os/boot-shim/src/loader.rs`
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:struct Name`、`path:const NAME`、`path:MACRO`），不使用行号；引文块内部的坐标与历史标记（若源注释原文带——如 `Replaces the previous …`、阶段编号 `D-58`/`E1 slice N`、日期）按原文保留，属引文保真的一部分，与本文结论无关。
> **本篇主题**：内核的两张编译期定容表怎么存、`struct proc`/`struct priv` 的字段各自承担什么、`p_rts_flags` 的 16 个位各自由谁置位与解除、`p_misc_flags` 的 18 个位各自的位值与语义（置位与解除的调用面散布在各功能篇），以及编译期写死的 17 项 boot image 清单如何被逐项填进进程槽、其中 VM 又是如何被内核亲自装载。
> **前置**：[03-kmain-cstart.md](03-kmain-cstart.md)（`kmain` 的分段与 `proc_init` 在其中的位置）、[04-platform-discovery.md](04-platform-discovery.md)（平台描述符与 `KernelInfo` 的消费）、[05-clock-interrupt-init.md](05-clock-interrupt-init.md)（§1.4 把"进程表本身的初始化与 boot 进程装载"指给本篇）。
> **后续**：`08-system-init-boot-finish`（启动收尾与切用户篇，尚未重生成）接 `bsp_finish_booting` 的其余步骤与 `switch_to_user`；`07-cross-space-init`（跨空间访问初始化篇，尚未重生成）接页表根与 Direct Map；`10-smp-boot`（SMP 启动篇，尚未重生成）接 AP 侧的同一批抑制位；`11-scheduling-primitives`（调度原语篇，尚未重生成）接就绪队列与 `pick_proc`；`12-ipc-core`（IPC 核心篇，尚未重生成）接消息流程。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；FPU 处理由 32 位的 `fnsave`/`fxrstor` 保存模型改为三个目标架构各自的懒初始化模型（`[ARCH: K-4]`），riscv64 一侧整套为 minix-rs 新增（`[ARCH: K-2]`）。
>
> **目标读者**：熟悉操作系统基本概念（特权级、分页、进程切换、中断）与 C 语言的开发者；Rust 一侧读到 trait/泛型即可。不需要预先了解 Minix3 的实现——引用的 C 符号都有解码；也不需要先弄懂调度器算法——本篇只用"全零才可运行"这一条规则（§1.5 已声明）。
>
> **前置知识**：虚拟内存与中断的大意；上面"前置"一行里那三篇的结论（`kmain` 分段、平台描述符、时钟与中断）。读过"自己写 OS"类书籍的读者可跳过 §1.1 的开场；§1.3 的清单、§1.4.1 的三套编号映射是后文前提，不建议跳过。
>
> **教学目标**（读完本篇应能独立讲出）：① 三张表的容量与索引关系（261 进程槽 / 64 特权槽 / 16 位状态），以及"为什么是编译期定容静态数组"的三层约束（§1.2、§2.0）；② `struct proc` 与 `struct priv` 的字段分组及各自承担的语义（§2.1、§2.3）；③ `p_rts_flags` 各位的置位/解除方，以及 `p_misc_flags` 各位的名字与语义（§2.2、§2.1.6；MF 的调用面散布在各功能篇）；④ 17 项 boot image 清单的两类成员、三套编号的映射与逐项填充的动作序列（§1.3、§1.4.1、§2.4–§2.6）；⑤ VM 的 ELF 为什么由内核亲自装载、逐页装载与共享边界页的处理（§2.7、§4.2）；⑥ Rust 侧两张表的类型化重建（`ProcKind`/`EntrySpec`/`KProcess`/`KPriv`）与零堆启动的工程实现（§3.1–§3.12）；⑦ 四道抑制位的就位与解除责任（§2.5、§2.6、§3.9、§4.8）；⑧ 每条机制靠什么证据成立、哪些还没有测试或真机断言（§5 的证据总账）。

05 篇结束时，CPU 已经能接住异常，控制器处于已知的屏蔽态，定时器已经编程、处理函数已经注册、门也开了。能听见硬件说话，不等于有谁可以被打断去服务——那一刻进程表还是一片空白，`proc_ptr` 指向一个尚未初始化的槽，任何一次 tick 进来都找不到可以记账的对象。这一篇要处理的，就是把"内核眼中可被调度的执行流"这件事从数据结构到实例化流程完整读一遍：两张编译期定容的表、挂在槽上的两组状态位、一份编译期写死的清单，以及清单落进槽位的那一段代码。

---

## 1. 概述

### 1.1 05 篇交出的状态与还缺的那件东西

中断能被送达之后，处理一次时钟中断要做的第一件事是记账：这一段时间算给谁、下一个该跑谁。这两问都要求"谁"是内核里一个真实存在的对象，而 05 篇结束时它还不存在。

C 里这个对象就是 `struct proc` 的一个槽。它在 `minix3/minix/kernel/proc.h` 里被声明为包含寄存器保存区、状态位、优先级、消息缓冲区、信号位图等 40 个顶层成员的结构体：

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 节选程序头部的成员 */
struct proc {
  struct stackframe_s p_reg;	/* process' registers saved in stack frame */
  struct segframe p_seg;	/* segment descriptors */
  proc_nr_t p_nr;		/* number of this process (for fast access) */
  struct priv *p_priv;		/* system privileges structure */
  volatile u32_t p_rts_flags;	/* process is runnable only if zero */
  volatile u32_t p_misc_flags;	/* flags that do not suspend the process */
  ...
```

这两行注释各定下一件事：`p_rts_flags` 的注释把"可运行"定义成一个数值条件（"process is runnable only if zero"）；`p_priv` 是一个指向另一张表的指针，说明"能不能跑"与"能做什么"是两份数据。此外还有 34 个顶层成员在这段节选之外，§2.1 会逐个走完。（块内 `...` 处省略的正是这 34 个成员，本块为节选。）

表的存储位置在文件末尾：

```c
/* 逐字引用，minix3/minix/kernel/proc.h:EXTERN struct proc proc */
EXTERN struct proc proc[NR_TASKS + NR_PROCS];	/* process table */
```

`EXTERN` 在这份文件里是 `extern`、在 `table.c` 里被重定义成空串（`minix3/minix/kernel/table.c` 的文件头注释解释了这套写法），于是这一行在 `table.c` 里产出真正的存储、在其余文件里只是声明。也就是说，进程表不是"启动时创建"的，它是 BSS 里一段编译期就定好地址的内存，`kmain` 里那句 `proc_init()` 做的事情不是分配，而是把每个槽宣布成空槽。

这一步的位置在 `minix3/minix/kernel/main.c:kmain`：`cstart()` 之后、`IPCF_POOL_INIT()`（IPC 过滤池的初始化宏，紧随其后）与 boot 循环之前。

**把 `proc_init()` 排到 boot 循环之后会怎样**：`proc_addr(n)` 的算式会先把槽指针算出来（BSS 里地址是有效的），但槽里的 `p_endpoint` 还是 BSS 零——而 boot 循环第二行就是 `ip->endpoint = rp->p_endpoint;`，把 0 号端点的值回填进清单条目；`p_priv` 也是零，`get_priv` 之前的任何一次 `priv(rp)` 都会解引用空指针。清表语句的位置不是风格问题，它决定了后面几步读到的是什么。

### 1.2 三张表与它们各自的容量

内核在任意时刻要回答三个问题：这是谁、它能做什么、它现在能不能跑。

第一问的答案在进程表里，容量是 `NR_TASKS + NR_PROCS`，两个加数在 `minix3/minix/include/minix/sys_config.h` 与 `minix3/minix/include/minix/com.h` 里分别给出：5 个内核 task 与 256 个用户进程槽，合计 261。第二问的答案在特权表里，容量是 `NR_SYS_PROCS = 64`（同一个 `sys_config.h` 文件）。第三问的答案不是一张独立的表，而是每个进程槽里的 `p_rts_flags`——它自己就是一张位图。

三张表之间有两条固定关系，另有一条贯通全篇的判定关系（第三条）：

- 进程号到槽的定位是一次纯算术：`proc_addr(n)` 展开为 `(&(proc[NR_TASKS + (n)]))`（宏族定义在 `minix3/minix/kernel/proc.h:END_PROC_ADDR` 与它相邻的几行里）。进程号允许为负——内核 task 的进程号是 −5 到 −1——所以下标算式必须写成"加一个偏移"，而不是直接用进程号。
- 进程与特权槽是多对一：261 个进程槽对应 64 个特权槽。这不是容量不够的妥协，`minix3/minix/kernel/priv.h` 的文件头注释把省空间写成了设计目的（"all user processes share one structure … very space efficient"）。共享的那个槽由一条算式指定：

```c
/* 逐字引用，minix3/minix/include/minix/priv.h:USER_PRIV_ID 与同文件 static_priv_id */
#define static_priv_id(n)          (NR_TASKS + (n))
#define USER_PRIV_ID	static_priv_id(ROOT_USR_PROC_NR)
```

`ROOT_USR_PROC_NR` 是 `INIT_PROC_NR`（`minix3/minix/include/minix/com.h:ROOT_USR_PROC_NR`），值为 11；代入得到 `USER_PRIV_ID = 5 + 11 = 16`。而 16 恰好是静态段的最后一个槽——静态段的长度是 `NR_STATIC_PRIV_IDS = NR_BOOT_PROCS = 17`（`minix3/minix/include/minix/priv.h:NR_STATIC_PRIV_IDS`），槽号区间 `[0, 17)`。整张特权表另有 47 个动态槽 `[17, 64)`，留给运行期由 RS 创建的系统服务。

这三张表在同一段代码里被同一批语句操作，把它们放在一张图上（本篇机制图，无语言围栏块）：

```text
        进程表 proc[261]                       特权表 priv[64]
   下标 = NR_TASKS + p_nr                下标 = s_id（静态段 [0,17) / 动态段 [17,64)）
   ┌────┬────┬────┬──────────────┐       ┌──────────────┬──────────────┐
   │ -5 │ -4 │ -3 │ …  …  0 … 255│       │ 0..4 内核 task│ 5..16 用户   │
   │ASYN│IDLE│CLOK│  PM … INIT   │       │ 5     PM …    │  16 = 全体用户共享
   └─┬──┴─┬──┴─┬──┴──────┬───────┘       └──────┬───────┴──────┬───────┘
     │    │    │         │  p_priv ──────────────┘              │
     │    │    │         │  （多对一：261 个槽指向 64 个特权槽）  │
     └────┴────┴─────────┴──► p_rts_flags（16 位；全零 ⇔ 可运行）◄┘
```

图中第三条关系用的是 `p_rts_flags` 的判定宏，它把"可运行"写成一次相等比较：`rts_f_is_runnable(flg)` 的值就是 `((flg) == 0)`（`minix3/minix/kernel/proc.h:proc_is_runnable` 就是它的进程版包装）。

**若让每个进程槽各自持有一份特权结构会怎样**：`struct priv` 的 31 个字段、三个定长数组（`s_io_tab[64]`、`s_mem_tab[20]`、`s_irq_tab[16]`）乘 261 份，就是按进程数线性增长的 BSS；更实质的变化在掩码宽度上：`s_ipc_to` 按特权槽号置位（位宽 `BITMAP_CHUNKS(NR_SYS_PROCS)`，`minix3/minix/kernel/type.h:sys_map_t`），槽号空间一扩到 261，位图宽度、`has_pending` 的扫描上界（`minix3/minix/kernel/proc.c:has_pending`）与 `set_sys_bit` 的检查都要一起改。

### 1.3 boot image：一份编译期清单和它的两类成员

第一批进程没有创建者，所以它们不能"被创建"，只能"已经写在源码里"。这份清单在 `minix3/minix/kernel/table.c`：

```c
/* 逐字引用，minix3/minix/kernel/table.c:image —— 表头注释与 17 个条目 */
/* The system image table lists all programs that are part of the boot image. 
 * The order of the entries here MUST agree with the order of the programs
 * in the boot image and all kernel tasks must come first.
 * The order of the entries here matches the priority NOTIFY messages are
 * delivered to a given process. NOTIFY messages are always delivered with
 * the highest priority. DS must be the first system process in the list to
 * allow reliable asynchronous publishing of system events. RS comes right after
 * to prioritize ping messages periodically delivered to system processes.
 */

struct boot_image image[NR_BOOT_PROCS] = {
/* process nr, name */
{ASYNCM,        "asyncm"},
{IDLE,          "idle"  },
{CLOCK,         "clock" },
{SYSTEM,        "system"},
{HARDWARE,      "kernel"},
                      
{DS_PROC_NR,    "ds"    },
{RS_PROC_NR,    "rs"    },
                      
{PM_PROC_NR,    "pm"    },
{SCHED_PROC_NR, "sched" },
{VFS_PROC_NR,   "vfs"   },
{MEM_PROC_NR,   "memory"},
{TTY_PROC_NR,   "tty"   },
{MIB_PROC_NR,   "mib"   },
{VM_PROC_NR,    "vm"    },
{PFS_PROC_NR,   "pfs"   },
{MFS_PROC_NR,   "mfs"   },
{INIT_PROC_NR,  "init"  },
};
```

17 个条目，前 5 个是内核 task（进程号为负），后 12 个是用户态模块。两类成员的差别不在名字上，而在"代码内容是谁放进内存的"：内核 task 编在内核镜像里，用户模块是引导器按 `boot.cfg` 的 `module` 指令加载进来的独立文件。这决定了 boot 循环里必须有一个分支——`i >= NR_TASKS` 的条目才需要去 `kinfo.module_list` 取地址。

表的静态部分只写"谁、什么号、叫什么"三样。`struct boot_image` 的另外两个字段是空的：

```c
/* 逐字引用，minix3/minix/include/minix/type.h:struct boot_image */
struct boot_image {
  int proc_nr;                    	/* process number to use */
  char proc_name[PROC_NAME_LEN];        /* name in process table */
  endpoint_t endpoint;                  /* endpoint number when started */
  phys_bytes start_addr;		/* Where it's in memory */
  phys_bytes len;
};
```

`start_addr` 与 `len` 由 boot 循环从模块链表回填，`endpoint` 由 boot 循环从进程槽读出来再写回清单（`main.c:kmain` 的 `ip->endpoint = rp->p_endpoint;`）。方向是从槽到清单：清单本来是常量，却在启动期被改写成"这一次启动的实况"，而这份改写后的副本随后被交给 VM。

**把模块取址的下标漏掉 `NR_TASKS` 偏移会怎样**：第 j 个模块的地址被读成第 j+5 项、末 5 次读越过 12 项数组的末尾——这是"编号错位"族的一例，族里现象最完整的一条（端点互换 + VM `panic("no init_proc")`）见 §1.4.1 的反推。

### 1.4 本篇的终点状态：就位但暂停

boot 循环结束时，17 个条目的槽都已经写好字段，但没有一个可以运行。挡住它们的不是一件事，而是四件，分别由四个状态位表示：

| 位 | 挡住什么 | 由谁解除 |
|---|---|---|
| `RTS_NO_PRIV` | 还没有权限（`p_priv` 未分配或未填） | RS 发 `SYS_PRIV_ALLOW`（`minix3/minix/kernel/system/do_privctl.c:do_privctl`） |
| `RTS_VMINHIBIT` | 页表还没由 VM 建好 | VM 发 `VMCTL_SETADDRSPACE` 或 `VMCTL_VMINHIBIT_CLEAR`（`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl` 与 `minix3/minix/kernel/system/do_vmctl.c:do_vmctl`） |
| `RTS_BOOTINHIBIT` | VM 还没把它的镜像装好 | VM 发 `VMCTL_BOOTINHIBIT_CLEAR`（同上文件） |
| `RTS_PROC_STOP` | 内核还没到"可以开始调度"的时刻 | `bsp_finish_booting` 的解除循环（`minix3/minix/kernel/main.c:bsp_finish_booting`） |

四个位被置位的时机也不同：`NO_PRIV` 与 `NO_QUANTUM` 成对置给"不能立刻调度"的成员；`VMINHIBIT`/`BOOTINHIBIT` 置给除 VM 以外的所有用户进程；`PROC_STOP` 置给全部 17 项。§2.6 会把这几步逐条读出来。

解除循环本身很短，但它有一个边界：

```c
/* 逐字引用，minix3/minix/kernel/main.c:bsp_finish_booting —— 解除那一段 */
  for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) {
	RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
  }
```

上界是 `NR_BOOT_PROCS - NR_TASKS = 12`，而循环体里的 `proc_addr(i)` 用的是**用户进程号**而不是清单下标。这两个事实合起来说明：解除循环覆盖的是进程号 0..11 的 12 个用户进程，5 个内核 task 不在其中，`RTS_PROC_STOP` 对它们永不解除（`proc_init` 里也给每 CPU 的 IDLE 结构置过这一位，`minix3/minix/kernel/proc.c:proc_init`）。这一位对内核 task 保持置位不是遗漏：内核 task 的工作在调用点就地执行——到期的时钟定时器由 tick 处理里的 `tmrs_exptimers` 跑（`minix3/minix/kernel/clock.c:timer_int_handler`），内核调用由 `kernel_call` 在陷入路径上直接分派（`minix3/minix/kernel/system.c:kernel_call`）——它们的槽不需要被调度器选中，`PROC_STOP` 保持置位正是"不把它们放进就绪队列"的表达。

**把上界写成 `NR_BOOT_PROCS` 会怎样**：多出的五次迭代落在用户进程号 12..16（`proc_addr` 的槽 17..21）上——这些槽从未被 boot 循环碰过，`p_rts_flags` 仍是 `RTS_SLOT_FREE`；而 `RTS_UNSET(rp, RTS_PROC_STOP)` 只清 `PROC_STOP` 这一位（它们本来就没置它），`SLOT_FREE` 不动，清完仍非零、判定不可运行、不入队——单看这处越界，它恰好无害。上界真正的承重点**是一致性**：循环体按用户进程号寻址（`proc_addr(i)` 覆盖 0..11），上界的 12 与"12 个用户进程"同义；哪一天把坐标或计数换成别的（槽下标、条目数），"清错了位"或"对象错了"才会变成真故障。

### 1.4.1 boot image 序号、模块序号与 proc 号三者的映射

`image[]` 里的条目、引导器给的模块、以及 `com.h` 里的进程号，是三套互不相同的编号。以 `image[]` 的下标为准，前 5 项没有模块，后 12 项与模块链表一一对应，对应关系是位置相减：

| image 下标 | 名字 | 类别 | 模块下标 | proc 号 | 端点（gen 0） |
|---|---|---|---|---|---|
| 0 | asyncm | 内核 task | — | −5 | −5 |
| 1 | idle | 内核 task | — | −4 | −4 |
| 2 | clock | 内核 task | — | −3 | −3 |
| 3 | system | 内核 task | — | −2 | −2 |
| 4 | kernel | 内核 task | — | −1 | −1 |
| 5 | ds | 用户模块 | 0 | 6 | 6 |
| 6 | rs | 用户模块 | 1 | 2 | 2 |
| 7 | pm | 用户模块 | 2 | 0 | 0 |
| 8 | sched | 用户模块 | 3 | 4 | 4 |
| 9 | vfs | 用户模块 | 4 | 1 | 1 |
| 10 | memory | 用户模块 | 5 | 3 | 3 |
| 11 | tty | 用户模块 | 6 | 5 | 5 |
| 12 | mib | 用户模块 | 7 | 7 | 7 |
| 13 | vm | 用户模块 | 8 | 8 | 8 |
| 14 | pfs | 用户模块 | 9 | 9 | 9 |
| 15 | mfs | 用户模块 | 10 | 10 | 10 |
| 16 | init | 用户模块 | 11 | 11 | 11 |

两列对照立刻暴露一件事：模块下标与 proc 号没有算术关系。DS 是模块 0、proc 号 6；RS 是模块 1、proc 号 2；PM 是模块 2、proc 号 0。启动期把模块装进哪个槽，靠的是"按下标取模块、按 proc 号取槽"这两次独立的查表。

内核侧的对应物是两组常量：5 个内核 task 的名字与进程号一张表，12 个用户模块各自的 proc 号另一张表，后者与引导器摆放模块的顺序构成一份位置约定——长度由共享常量钉死，顺序靠人工对齐。

**把模块序号直接当 proc 号用会怎样**：`module_list[0]`（ds）会被装进 proc 号 0 的槽，也就是 PM 的槽；DS 与 PM 的端点互换。用户态代码按 `DS_PROC_NR` 寻址的调用全部落到 PM 的槽上，而 VM 启动时靠线性扫描 `kernel_boot_info.boot_procs[]` 按 `proc_nr` 找自己的条目（`minix3/minix/servers/vm/main.c:init_proc`），错位后它找不到 DS 的 boot 条目，直接 `panic("no init_proc")`。

### 1.5 本篇范围与可以安全跳过的内容

本篇的终点是一个可描述的状态：261 个槽全部有明确归属，17 个 boot 条目字段齐备，VM 的 ELF 已在 bootstrap 页表里，四道抑制位就位待解除。越过这个终点之后的事不属于本篇：

| 不在这里展开的内容 | 它属于 |
|---|---|
| 首次 `pick_proc` 的选择与 `switch_to_user` 的寄存器恢复 | `08-system-init-boot-finish`（启动收尾与切用户篇，尚未重生成）、`11-scheduling-primitives`（调度原语篇，尚未重生成） |
| 就绪队列的入队/出队规则与优先级队列 | `11-scheduling-primitives`（调度原语篇，尚未重生成） |
| 消息传递的匹配、阻塞与唤醒 | `12-ipc-core`（IPC 核心篇，尚未重生成） |
| `SYS_PRIVCTL` 的运行时语义与 RS 的授权流程 | 权限管控篇（尚未重生成） |
| AP 侧的进程可见性与 per-CPU 数据 | `10-smp-boot`（SMP 启动篇，尚未重生成） |
| FPU 寄存器逐位细节与懒陷阱的完整时序 | FPU 专题（尚未重生成）；本篇给到策略字段、归属与"门控发生在哪个域"（§3.6、§4.1），逐寄存器时序与完整流程归专题 |
| VM 侧的 `VMCTL` 协商协议 | `09-vm-boot-protocol`（VM 启动协议篇，尚未重生成） |
| 启动全流程的 A–F 阶段骨架（本篇对应其中"清表与装载"的那一段） | `00-kernel-overview`（内核总览篇，尚未重生成） |

两条可以安全跳过的说明：本篇**不需要**掌握三个架构的异常模型——`cpu_context` 的字段是架构私有的，内核层只认识"进程的 CPU 上下文"这个概念，看不到里面的 PSW/PSR/sstatus；本篇也**不需要**先弄懂调度器的队列算法——16 个状态位的判定只有一条规则（全零才可运行），队列动作只在这条规则发生跃迁时被触发。

**本章收束**：用 §1.2 的那张图再说一遍这一章。三张表各有容量与索引关系（261 个槽用 `NR_TASKS + p_nr` 定位，64 个特权槽按 s_id 索引且用户进程共享 16 号槽，16 位状态用"全零"定义可运行）；17 项清单分两类（5 个内核 task 与 12 个用户模块，模块下标与 proc 号无关）；四道抑制挡住四件不同的事，其中三道由用户态发来的内核调用解除、一道由内核自己在启动末刻解除。剩下的一整章要做的，就是把这三样东西在 C 源码里读到位。

---

## 2. C 源码分析

### 2.0 存储约束与容量：为什么是编译期定容的静态数组

两张表的容量与存储形态由三层约束逐级决定，这三层约束也构成后面每一个存储选择的前提。**第一层：模型本身编译期定容。** 内核里没有"表会增长"这个概念。下面五个容量常量都是编译期常量，前三个是字面量，后两个是由前几个常量算出的宏表达式：

| 常量 | 值 | 定义处 | 含义 |
|---|---|---|---|
| `NR_TASKS` | 5 | `minix3/minix/include/minix/com.h:NR_TASKS` | 内核 task 数（ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL，进程号 −5..−1） |
| `NR_PROCS` | 256 | `minix3/minix/include/minix/sys_config.h:_NR_PROCS`（经 `minix3/minix/include/minix/config.h:NR_PROCS`） | 用户进程槽上限（进程号 0..255） |
| `NR_SYS_PROCS` | 64 | 同一个 `sys_config.h` 的 `_NR_SYS_PROCS` | 特权槽上限 |
| `NR_BOOT_PROCS` | `NR_TASKS + LAST_SPECIAL_PROC_NR + 1` = 17（`LAST_SPECIAL_PROC_NR` 即 `INIT_PROC_NR`（11），定义在 `minix3/minix/include/minix/com.h`） | `minix3/minix/include/minix/param.h:NR_BOOT_PROCS` | boot image 条目数（5 task + 12 模块） |
| `NR_BOOT_MODULES` | `INIT_PROC_NR + 1` = 12 | `minix3/minix/include/minix/com.h:NR_BOOT_MODULES` | boot image 中用户态模块数 |

两个容量关系由编译期强制。`NR_BOOT_PROCS (17) ≤ NR_SYS_PROCS (64)` 写在 `minix3/minix/kernel/priv.h` 末尾：

```c
/* 逐字引用，minix3/minix/kernel/priv.h —— 末尾的编译期自检 */
#if (NR_BOOT_PROCS > NR_SYS_PROCS)
#error NR_SYS_PROCS must be larger than NR_BOOT_PROCS
#endif
```

它的含义是：boot image 的每个成员在启动期都要能分到一个特权槽，否则内核拒绝编译。进程表槽总数则由 `proc.h` 的数组声明本身给出：`NR_TASKS + NR_PROCS = 261`。

**第二层：内核全生命周期不使用堆分配器，而不只是 boot 期。** `minix3/minix/kernel/main.c:kmain` 里有一句 `kernel_may_alloc = 1;`，注释写着 "Kernel may use bits of main memory before VM is started"，`bsp_finish_booting` 末尾又把它置回 0（注释 "Kernel may no longer use bits of memory as VM will be running soon"）。这个开关管的是"内核能否直接使用主存中的空闲段"，它不提供任何分配器；C 内核里没有 `malloc`/`kmalloc`/`slab` 之类的调用，窗口内真正做分配的是一条按页从内存图里切走空闲段的路径：`pg_alloc_page`（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`，开头一句 `assert(kernel_may_alloc)`，被 `pg_map` 的分配分支调用）。`freepdes` 数组（`minix3/minix/kernel/arch/i386/memory.c:memory_init`）是另一件事——跨空间映射窗口借用的空闲页目录，见 `07-cross-space-init`（跨空间访问初始化篇，尚未重生成）。因此在这段窗口里唯一能用的存储是编译期预留的静态内存——`proc[]`、`priv[]` 都是 `EXTERN` 数组，直接落在 BSS。

**第三层：于是存储形态只能落成编译期定容的静态数组。** 前三层合起来把选择空间压到一处：容量是编译期常量、窗口内没有分配器、表要用到关机。任何对位实现都只能把这两张表放在编译期就确定地址的存储里，用定长数组表达；具体的类型与访问方式属于下一章（§3.1）。

需要说清边界：**`no_std` 不等于没有堆**。`no_std` 只表示不依赖标准库，`alloc` crate 在裸机环境里照样可用。"这里不能用堆"的判据是第二层——启动期没有全局分配器，而进程表在这个窗口里就必须可用。

这段因果链另有四条边界。**其一**，本节回答的只是"为什么是静态数组"——槽定位、状态位图、就绪队列链接、特权槽映射这些字段级存储决策在 §2.1–§2.3 展开，Rust 的类型与同步原语在 §3.1–§3.10 展开，本节不重复。**其二**，三层因果链只在 Minix3 与 minix-rs 的模型下成立，不是内核通用规律：进程表可以做成运行期可增长的结构（Linux 的进程描述符由 slab 分配，Redox 的进程对象装在可增长容器里，seL4 与 Minix3 一样编译期定容——这三句是设计空间的对照，具体形态不属本篇的核对范围）；把 `proc[]` 改成动态分配属加负式架构演进，要引入 id 分配器、退出时的资源回收与 OOM 处理，超出本篇的 Rewrite 范围。**其三**，定容是主动设计选择：没有 id 分配器要维护，内存占用编译期确定，不存在"进程表满"的运行期失败路径。**其四**，`proc[]` 的 261 槽是**全系统进程身份的刚性上限**——超过 261 的进程身份在内核里不存在，动态容器装不满它；静态数组的强制只落在内核 `proc[]` 与 VM 的进程表上（`os/servers/vm/src/vmproc/table.rs:struct VmProcTable`，两者建立时堆都未就绪），PM/VFS 等后启动服务此时堆已可用，它们沿用静态数组是继承 Minix3 的形态而不是"必须"。

**把两张表改成按需分配会怎样**：`proc_init` 执行时堆还不存在（第二层），第一句分配要么空指针要么直接 panic；即使把窗口推到 VM 启动之后，`proc_addr(n)` 的"一次乘加定位"会退化成一次查找，`isokprocn(n)` 这类边界宏需要一次容器查询，而唯一的失败返回位置——`sys_call` 的调用链——并没有为 ENOMEM 准备回传路径。静态数组把这三件事都消掉了。

### 2.1 `struct proc` 字段全集

`struct proc` 有 40 个顶层成员。按"谁读它、在什么时机写"分成六组，导航如下表；本节逐组展开。

| 组 | 成员 | 本节 | boot 期是否被写 |
|---|---|---|---|
| 身份 | `p_nr` `p_endpoint` `p_name` `p_magic` `p_priv` `p_found` | §2.1.1 | 清表写 3 个（`p_nr`/`p_endpoint`/`p_magic`；`p_rts_flags` 属生命周期组）；填充写名字与 `p_priv` 关联 |
| 调度与 CPU | `p_priority` `p_cpu_time_left` `p_quantum_size_ms` `p_scheduler` `p_cpu` `p_cpu_mask` `p_stale_tlb` | §2.1.2 | 写优先级、时间片、`p_scheduler`、`p_cpu_time_left` |
| IPC 链接与消息 | `p_nextready` `p_caller_q` `p_q_link` `p_getfrom_e` `p_sendto_e` `p_sendmsg` `p_delivermsg` `p_delivermsg_vir` | §2.1.3 | 不写（BSS 零 = 无消息） |
| 执行上下文与 VM 请求 | `p_reg` `p_seg` `p_pending` `p_defer` `p_vmrequest` | §2.1.4 | `arch_proc_reset` 写 `p_reg`/`p_seg`；`p_defer`/`p_vmrequest` 不写 |
| 记账与统计 | `p_accounting` `p_dequeued` `p_user_time` `p_sys_time` `p_virt_left` `p_prof_left` `p_cycles` `p_kcall_cycles` `p_kipc_cycles` `p_tick_cycles` `p_cpuavg` | §2.1.5 | 只写 `p_accounting` 六个子字段（`reset_proc_accounting`） |
| 生命周期控制 | `p_rts_flags` `p_misc_flags` | §2.1.6 | 全部 17 项都要定状态 |
| 条件编译（不分组） | 1 | 仅 `p_schedules`（`DEBUG_TRACE` 下的调度计数）——调试用途，本篇不展开其语义（Rust 对位见 §4.4） |

口径说明：40 这个数按"顶层成员"计——37 个具名声明 + 3 个匿名聚合类型（`p_accounting`、`p_vmrequest`、`p_defer`）；`p_cpu_mask`/`p_stale_tlb` 在 `CONFIG_SMP` 下、`p_schedules` 在 `DEBUG_TRACE` 下，按声明行一并计入。复算方式见 §6.2。

#### 2.1.1 身份组

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 身份组六行（多处拼接；块内 `...` 处为结构体中与这六行不相邻的其他成员） */
  proc_nr_t p_nr;		/* number of this process (for fast access) */
  struct priv *p_priv;		/* system privileges structure */
  ...
  char p_name[PROC_NAME_LEN];	/* name of the process, including \0 */
  endpoint_t p_endpoint;	/* endpoint number, generation-aware */
  ...
  int p_found;	/* consistency checking variables */
  int p_magic;		/* check validity of proc pointers */
```

（这六个成员在 `struct proc` 里并不相邻：`p_nr`/`p_priv` 在头部，`p_name`/`p_endpoint` 在消息缓冲之前，`p_found`/`p_magic` 在末尾。）

`proc_nr_t` 是 `int`（`minix3/minix/kernel/type.h:proc_nr_t`）。`p_nr` 的值域是 −5..255，它同时是"下标算式的输入"与"进程身份的短标识"。`p_name` 是 16 字节的数组（`PROC_NAME_LEN`，`minix3/minix/include/minix/type.h:PROC_NAME_LEN`）。**boot 期只有两类成员拿到名字**：内核 task 由 boot 循环直接写（`minix3/minix/kernel/main.c:kmain` 里那句 `if(i < NR_TASKS) strlcpy(rp->p_name, ip->proc_name, ...)`）；VM 由 `arch_boot_proc` 末尾那次 `arch_proc_init` 写（`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc`，该调用点整体位于 `if(rp->p_nr == VM_PROC_NR)` 分支内；函数体见 `minix3/minix/kernel/arch/i386/memory.c:arch_proc_init`，它的前两句是 `arch_proc_reset(pr)` 与 `strlcpy(pr->p_name, name, ...)`）。**其余 11 个用户模块在 boot 期没有名字**——它们各自启动后经 `exec` 路径拿名字（`minix3/minix/kernel/system/do_exec.c:do_exec` 里同一支 `arch_proc_init`）。

`p_magic` 是完整性哨兵：`PMAGIC` 定义在 `minix3/minix/include/minix/const.h:PMAGIC`，值为 `0xC0FFEE1`，判定宏是 `proc_ptr_ok(p)`。它的读者是调试与内部校验路径。

`p_endpoint` 是"槽号 + 代"的打包值，§2.8 展开。

#### 2.1.2 调度与 CPU 组

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 调度组（含一个 CONFIG_SMP 块） */
  char p_priority;		/* current process priority */
  u64_t p_cpu_time_left;	/* time left to use the cpu */
  unsigned p_quantum_size_ms;	/* assigned time quantum in ms
				   FIXME remove this */
  struct proc *p_scheduler;	/* who should get out of quantum msg */
  unsigned p_cpu;		/* what CPU is the process running on */
#ifdef CONFIG_SMP
  bitchunk_t p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]; /* what CPUs is the
							    process allowed to
							    run on */
  bitchunk_t p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]; /* On which cpu are
				possibly stale entries from this process and has
				to be fresed the next kernel touches this
				processes memory
				 */
#endif
```

`p_priority` 的值域由调度器的优先级队列数决定：`TASK_Q = 0` 是最高、`MIN_USER_Q = NR_SCHED_QUEUES - 1 = 15` 是最低（`minix3/minix/include/minix/config.h:NR_SCHED_QUEUES`）。`kmain` 的 boot 循环只给 VM 与 RS 写优先级（`SRV_Q = USER_Q`，值为 7），其余槽的 `p_priority` 是清表时写的 0。

`p_scheduler` 指向该进程的用户态调度器；清表时写成 `NULL`，判定宏 `proc_kernel_scheduler(p)` 把 `NULL` 与"指向自己"都当作"由内核默认策略调度"（`minix3/minix/kernel/proc.h:proc_kernel_scheduler`）。

`p_cpu_mask` 与 `p_stale_tlb` 在 32 位架构（i386/ARM32）下是位图数组，前者按注释是"允许在哪些 CPU 上运行"的白名单，后者记录"哪些 CPU 上可能有该进程的陈旧 TLB 项"。这两个字段在当前 C 树里的读者很少：`p_cpu_mask` 全树只有声明与 `do_update` 槽交换时的一次整块拷贝，启动路径不写它、调度路径不读它；`p_stale_tlb` 的写入点在 `do_vmctl` 的 `VMINHIBIT` 解除分支（`bits_fill`）。`p_cpu` 记录最近一次被分配到哪个 CPU。

#### 2.1.3 IPC 链接与消息组

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— IPC 链接与消息组 */
  struct proc *p_nextready;	/* pointer to next ready process */
  struct proc *p_caller_q;	/* head of list of procs wishing to send */
  struct proc *p_q_link;	/* link to next proc wishing to send */
  endpoint_t p_getfrom_e;	/* from whom does process want to receive? */
  endpoint_t p_sendto_e;	/* to whom does process want to send? */
  ...
  message p_sendmsg;		/* Message from this process if SENDING */
  message p_delivermsg;		/* Message for this process if MF_DELIVERMSG */
  vir_bytes p_delivermsg_vir;	/* Virtual addr this proc wants message at */
```

（块内省略号处为 `p_pending`、`p_name`、`p_endpoint` 三个成员：`p_pending` 在 §2.1.4，后两个在 §2.1.1。）

这一组里有三个 `struct proc *` 指针，其中两个组成链表：`p_nextready` 挂在同优先级的就绪队列上，`p_caller_q`/`p_q_link` 组成"谁在等我收消息"的等待队列——队头在收方，链节在发方。消息缓冲是内嵌的两个 `message` 结构体（发出与待投递各一份）加一个虚拟地址字段。这一组在 boot 期完全不被写：槽是零值意味着"不在任何队列上、没有消息"。

`p_caller_q` 与 `p_q_link` 的分工决定了"从等待队列里摘一个进程"要改哪个槽：摘节点要改前驱的链节或队头，`clear_ipc`（`minix3/minix/kernel/system.c:clear_ipc`）就是逐链走一遍。

#### 2.1.4 执行上下文与 VM 请求组

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 上下文组开头两行 */
  struct stackframe_s p_reg;	/* process' registers saved in stack frame */
  struct segframe p_seg;	/* segment descriptors */
```

`p_reg` 是寄存器保存区，`p_seg` 是段/页表描述区。这两个字段是架构私有内容的落点，`arch_proc_reset` 负责在清表时把它们写成已知值：清空寄存器、按进程类型写初值 PSW（内核 task 用 `INIT_TASK_PSW`、用户进程用 `INIT_PSW`）、写六个段选择子、给用户进程分配 FPU 状态缓冲（`minix3/minix/kernel/arch/i386/arch_system.c:arch_proc_reset`）。

FPU 缓冲在 32 位架构下是一个架构级池加一个指针：

```c
/* 逐字引用，minix3/minix/kernel/arch/i386/arch_system.c —— FPU 池与取址 */
static char fpu_state[NR_PROCS][FPU_XFP_SIZE] __aligned(FPUALIGN);
...
	if(pr->p_nr >= 0) {
		v = fpu_state[pr->p_nr];
```

（节选，`minix3/minix/kernel/arch/i386/arch_system.c`——块内 `...` 处省略 `arch_proc_reset` 的函数头与函数间的语句。）

注意这里的条件是 `p_nr >= 0`：内核 task（负进程号）拿不到缓冲，`p_seg.fpu_state` 保持 `NULL`。这条细节在 §3.6 的 FPU 策略对位里会用到。

组内还有一个匿名聚合 `p_vmrequest`，它承载"内核在等 VM 处理一次内存请求"所需的现场（请求类型、目标、参数、结果、两条链的链接），有效性由 `RTS_VMREQUEST` 指示——`p_vmrequest` 的注释把这条约定写得很直白（"Until it has asked what needs to be done and fixed it, save necessary state here"）。

`p_pending` 是内核信号位图，`p_defer` 保存被推迟的 `do_ipc` 参数。两者在 boot 期都不写。

#### 2.1.5 记账与统计组

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 记账块与时间字段 */
  struct {
	u64_t enter_queue;	/* time when enqueued (cycles) */
	u64_t time_in_queue;	/* time spent in queue */
	unsigned long dequeues;
	unsigned long ipc_sync;
	unsigned long ipc_async;
	unsigned long preempted;
  } p_accounting;

  clock_t p_dequeued;		/* uptime at which process was last dequeued */
  clock_t p_user_time;		/* user time in ticks */
  clock_t p_sys_time;		/* sys time in ticks */
  clock_t p_virt_left;		/* number of ticks left on virtual timer */
  clock_t p_prof_left;		/* number of ticks left on profile timer */
  u64_t p_cycles;		/* how many cycles did the process use */
  u64_t p_kcall_cycles;		/* kernel cycles caused by this proc (kcall) */
  u64_t p_kipc_cycles;		/* cycles caused by this proc (ipc) */
  u64_t p_tick_cycles;		/* cycles accumulated for up to a clock tick */
  struct cpuavg p_cpuavg;	/* running CPU average, for ps(1) */
```

这一组在 boot 期只被 `reset_proc_accounting` 写：

```c
/* 逐字引用，minix3/minix/kernel/proc.c:reset_proc_accounting —— 完整函数体 */
void reset_proc_accounting(struct proc *p)
{
  p->p_accounting.preempted = 0;
  p->p_accounting.ipc_sync  = 0;
  p->p_accounting.ipc_async = 0;
  p->p_accounting.dequeues  = 0;
  p->p_accounting.time_in_queue = 0;
  p->p_accounting.enter_queue = 0;
}
```

它清的是六个子字段：三个计数器（`preempted`/`ipc_sync`/`ipc_async`）加出队次数 `dequeues`，以及"入队时刻"与"在队列里待了多久"两个时间累加量。其余统计字段留给运行期。

#### 2.1.6 生命周期控制组：RTS 与 MF 两个位图

```c
/* 逐字引用，minix3/minix/kernel/proc.h:struct proc —— 生命周期组两行 */
  volatile u32_t p_rts_flags;	/* process is runnable only if zero */
  volatile u32_t p_misc_flags;	/* flags that do not suspend the process */
```

两个字段都是 32 位，语义分工写在注释里：`p_rts_flags`（runtime status）决定"能不能跑"，`p_misc_flags`（miscellaneous）不影响挂起状态、只改变行为细节。

**`p_rts_flags` 的 16 个位**是 §2.2 的主题。全部定义如下（表内的 16 个位在 §2.2 逐位展开）：

| 位值 | 位名 | 位值 | 位名 |
|---|---|---|---|
| `0x00001` | `RTS_SLOT_FREE` | `0x01000` | `RTS_VMREQTARGET` |
| `0x00002` | `RTS_PROC_STOP` | `0x04000` | `RTS_PREEMPTED` |
| `0x00004` | `RTS_SENDING` | `0x08000` | `RTS_NO_QUANTUM` |
| `0x00008` | `RTS_RECEIVING` | `0x10000` | `RTS_BOOTINHIBIT` |
| `0x00010` | `RTS_SIGNALED` | | |
| `0x00020` | `RTS_SIG_PENDING` | | |
| `0x00040` | `RTS_P_STOP` | | |
| `0x00080` | `RTS_NO_PRIV` | | |
| `0x00100` | `RTS_NO_ENDPOINT` | | |
| `0x00200` | `RTS_VMINHIBIT` | | |
| `0x00400` | `RTS_PAGEFAULT` | | |
| `0x00800` | `RTS_VMREQUEST` | | |

16 个位，位值不连续：`0x02000` 没有被任何位占用，`RTS_PREEMPTED` 直接取 `0x04000`。数这个数的时候口径要写在前面——`proc.h` 里以 `#define RTS_` 开头的行有 20 行，其中 4 行是操作宏（`RTS_ISSET`/`RTS_SET`/`RTS_UNSET`/`RTS_SETFLAGS`），剩下 16 行是位定义。

**`p_misc_flags` 的 18 个位**全表如下。第三列给的是行尾注释表达的语义；置位与解除的调用点分布在各功能篇的路径上——IPC 与信号、VM 协议、时钟与统计、SMP、FPU 与系统调用跟踪各有一批——本篇只固定位的名字、值与语义：

| 位值 | 位名 | 语义（行尾注释原文要点） |
|---|---|---|
| `0x00001` | `MF_REPLY_PEND` | 对一个 `IPC_REQUEST` 的应答尚未发出 |
| `0x00002` | `MF_VIRT_TIMER` | 进程虚拟定时器在走 |
| `0x00004` | `MF_PROF_TIMER` | 档案（profiling）定时器在走 |
| `0x00008` | `MF_KCALL_RESUME` | 内核调用被中断（等 VM 或长拷贝被抢占），需要恢复执行 |
| `0x00040` | `MF_DELIVERMSG` | 运行前先替它把消息拷进去 |
| `0x00080` | `MF_SIG_DELAY` | 等它不再处于发送状态时再送信号 |
| `0x00100` | `MF_SC_ACTIVE` | 系统调用跟踪：正在一次系统调用中 |
| `0x00200` | `MF_SC_DEFER` | 系统调用跟踪：被推迟的系统调用 |
| `0x00400` | `MF_SC_TRACE` | 系统调用跟踪：触发跟踪事件 |
| `0x01000` | `MF_FPU_INITIALIZED` | 进程已用过数学协处理器，FPU 寄存器有意义 |
| `0x02000` | `MF_SENDING_FROM_KERNEL` | 这条消息是内核替该进程发出的 |
| `0x04000` | `MF_CONTEXT_SET` | 不要动上下文 |
| `0x08000` | `MF_SPROF_SEEN` | profiling 已经记录过该进程 |
| `0x10000` | `MF_FLUSH_TLB` | 让它再运行前必须刷 TLB（当前只对 SMP 有意义） |
| `0x20000` | `MF_SENDA_VM_MISS` | 异步接收落空是因为 VM 正在改发送方的地址空间 |
| `0x40000` | `MF_STEP` | 单步执行该进程 |
| `0x80000` | `MF_MSGFAILED`（注释列其中一处语义由调用点推得） | 消息投递失败 |
| `0x100000` | `MF_NICED` | 用户降低了该进程的最高优先级 |

18 个位，位值同样不连续：`0x00010`、`0x00020` 与 `0x00800` 三个位置未被占用。位宽的上限是 21 位（最高位 `0x100000`），离 32 位还有余量。

这 18 个位里，boot 期唯一会被写的是 `MF_FPU_INITIALIZED` 的反向操作——`do_clear` 在回收一个槽时把它清掉（`minix3/minix/kernel/system/do_clear.c:do_clear`）：

```c
/* 逐字引用，minix3/minix/kernel/system/do_clear.c:do_clear —— 回收那一段 */
  /* Make sure that the exiting process is no longer scheduled,
   * and mark slot as FREE. Also mark saved fpu contents as not significant.
   */
  RTS_SETFLAGS(rc, RTS_SLOT_FREE);
  
  /* release FPU */
  release_fpu(rc);
  rc->p_misc_flags &= ~MF_FPU_INITIALIZED;
```

`RTS_SETFLAGS` 与另外两个宏不同：它不做"跃迁判定"，而是"若原来可运行先出队、再把整个值覆盖成给定值"。两条注释与三行代码合起来说明回收的终态：`p_rts_flags` 恰好等于 `RTS_SLOT_FREE`（判空宏 `isemptyp` 的判据），`MF_FPU_INITIALIZED` 被清掉意味着"下一个占用该槽的进程不会继承前任的 FPU 状态"。

**让 `p_misc_flags` 也参与"能不能跑"的判定会怎样**（例如把 `MF_SIG_DELAY` 当成暂停条件）：`RTS_SET`/`RTS_UNSET` 的跃迁判定只看 `p_rts_flags`，两处判定若各看一份位，同一个进程可能"按 RTS 可运行、按 MF 不可运行"，`pick_proc` 选中它、`switch_to_user` 又发现不该跑，返回路径没有为这种二次否决准备分支；这也是 `do_runctl` 在收到 `RC_DELAY` 请求时选择"置 `MF_SIG_DELAY` 并返回 EBUSY"而不是"置 RTS 位"的原因（`minix3/minix/kernel/system/do_runctl.c:do_runctl`）。

### 2.2 `p_rts_flags` 位表：16 位逐位

16 个位的定义在 `proc.h` 里是连续的一段：

```c
/* 逐字引用，minix3/minix/kernel/proc.h —— 16 个 RTS 位定义（位值不连续） */
/* Bits for the runtime flags. A process is runnable iff p_rts_flags == 0. */
#define RTS_SLOT_FREE	0x01	/* process slot is free */
#define RTS_PROC_STOP	0x02	/* process has been stopped */
#define RTS_SENDING	0x04	/* process blocked trying to send */
#define RTS_RECEIVING	0x08	/* process blocked trying to receive */
#define RTS_SIGNALED	0x10	/* set when new kernel signal arrives */
#define RTS_SIG_PENDING	0x20	/* unready while signal being processed */
#define RTS_P_STOP	0x40	/* set when process is being traced */
#define RTS_NO_PRIV	0x80	/* keep forked system process from running */
#define RTS_NO_ENDPOINT	0x100	/* process cannot send or receive messages */
#define RTS_VMINHIBIT	0x200	/* not scheduled until pagetable set by VM */
#define RTS_PAGEFAULT	0x400	/* process has unhandled pagefault */
#define RTS_VMREQUEST	0x800	/* originator of vm memory request */
#define RTS_VMREQTARGET	0x1000	/* target of vm memory request */
#define RTS_PREEMPTED	0x4000	/* this process was preempted by a higher
				   priority process and we should pick a new one
				   to run. Processes with this flag should be
				   returned to the front of their current
				   priority queue if they are still runnable
				   before we pick a new one
				 */
#define RTS_NO_QUANTUM	0x8000	/* process ran out of its quantum and we should
				   pick a new one. Process was dequeued and
				   should be enqueued at the end of some run
				   queue again */
#define RTS_BOOTINHIBIT	0x10000	/* not ready until VM has made it */

/* A process is runnable iff p_rts_flags == 0. */
#define rts_f_is_runnable(flg)	((flg) == 0)
```

逐位看，能发现三件事。第一，位的编号从 `0x01` 到 `0x10000`、中间空掉 `0x2000`，最高位落在 17 号位，说明这份位图还留有很大余量。第二，注释里出现频率最高的词是三类：`not scheduled`/`unready`/`blocked`（不可调度的原因）、`set when`（谁触发）、`should be`（解除后该怎么办）。第三，只有 `RTS_PREEMPTED` 与 `RTS_NO_QUANTUM` 两条注释谈到了"重新入队的顺序"（队首/队尾），其余位在语义上都是"解除即可继续"。

#### 2.2.1 三个操作宏

位的读写在 `proc.h` 里全部经过四个宏，其中三个会顺带改就绪队列：

```c
/* 逐字引用，minix3/minix/kernel/proc.h —— RTS_ISSET / RTS_SET / RTS_UNSET / RTS_SETFLAGS */
#define RTS_ISSET(rp, f) (((rp)->p_rts_flags & (f)) == (f))


/* Set flag and dequeue if the process was runnable. */
#define RTS_SET(rp, f)							\
	do {								\
		const int rts = (rp)->p_rts_flags;			\
		(rp)->p_rts_flags |= (f);				\
		if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) {	\
			dequeue(rp);					\
		}							\
	} while(0)

/* Clear flag and enqueue if the process was not runnable but is now. */
#define RTS_UNSET(rp, f) 						\
	do {								\
		int rts;						\
		rts = (rp)->p_rts_flags;				\
		(rp)->p_rts_flags &= ~(f);				\
		if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) {	\
			enqueue(rp);					\
		}							\
	} while(0)

/* Set flags to this value. */
#define RTS_SETFLAGS(rp, f)					\
	do {								\
		if(proc_is_runnable(rp) && (f)) { dequeue(rp); }		\
		(rp)->p_rts_flags = (f);				\
	} while(0)
```

三段宏体各有分工。`RTS_SET` 先读旧值再置位，只有"旧值可运行、新值不可运行"时才出队——这一点保证了对已经阻塞的进程再置一个位不会把它从队列上摘第二次。`RTS_UNSET` 是对称的：旧值不可运行、新值变成零才入队。`RTS_SETFLAGS` 不做跃迁判定：若原来可运行且目标值非零就先出队，然后把整个值覆盖成 `f`；它的唯一调用点是 `do_clear` 的槽回收，因为那里要的不是"清一个位"而是"把状态改成恰好等于 `RTS_SLOT_FREE`"。

三个宏的入队/出队动作把"状态位"和"就绪队列"绑在一起，这就是为什么 §1.2 的图里 `p_rts_flags` 那条线上会分出一条到队列的箭头。"全零才可运行"是判定，"跃迁才动队列"是维护规则。

#### 2.2.2 逐位的置位与解除

下表把 16 位逐一列出。第三、四列给置位与解除的调用点；第五列说明解除后会发生什么（是否立刻入队）；最后一列给出该位的存在理由所在的调用链。表内的置位/解除两列给的是 `文件:函数` 坐标：启动期四位（`RTS_NO_PRIV`、`RTS_VMINHIBIT`、`RTS_BOOTINHIBIT`、`RTS_PROC_STOP`）的置位语句在本篇 §2.6 的 boot 循环里有逐字引用，`RTS_PROC_STOP` 的解除循环引在 §1.4，四位的 Rust 对位在 §3.9；其余十二位的置位与解除语句**主要**分布在表内点出的 IPC（`mini_send`/`mini_receive`/`clear_ipc`）、信号（`cause_sig`）、VM（`do_vmctl`）、调度（`enqueue`/`switch_to_user`/`sched_proc`）与跟踪（`do_trace`）函数体里，另有清表与回收路径（`proc_init`/`do_clear`）与退出路径（`clear_endpoint`/`do_exec`）若干处，本篇只给坐标，不逐行展开（列举口径：各列给主要调用点，非穷举——漏列的多为 CONFIG_SMP 分支与次要解除路径，可按函数名全树检索复核）。

| 位 | 位名 | 谁置位 | 谁解除 | 解除后的动作 | 读者/判据 |
|---|---|---|---|---|---|
| `0x00001` | `RTS_SLOT_FREE` | `minix3/minix/kernel/proc.c:proc_init`（清表写整值）、`minix3/minix/kernel/system/do_clear.c:do_clear`（`RTS_SETFLAGS`） | `minix3/minix/kernel/main.c:kmain`（boot 循环末 `&= ~`） | 出队（`RTS_SETFLAGS` 路径）；boot 路径不触发跃迁 | `isemptyp`（`p_rts_flags == RTS_SLOT_FREE`） |
| `0x00002` | `RTS_PROC_STOP` | `main.c` 的 `kmain`（全部 17 项）、`minix3/minix/kernel/proc.c:proc_init`（各 CPU 的 idle 槽）、`minix3/minix/kernel/system/do_runctl.c:do_runctl`（`RC_STOP`）、`minix3/minix/kernel/smp.c:smp_sched_handler`（跨 CPU 停）与 `smp.c:smp_schedule_stop_proc` | `main.c` 的 `bsp_finish_booting`（进程号 0..11）、`do_runctl`（`RC_RESUME`）、`smp.c`（迁移收尾）、`minix3/minix/kernel/arch/i386/arch_system.c:save_fpu`（CONFIG_SMP） | `RTS_UNSET` 命中跃迁 → 入队 | `do_sigsend.c`、`smp.c` 的断言 |
| `0x00004` | `RTS_SENDING` | `minix3/minix/kernel/proc.c` 的 `mini_send`（目标不可收时） | 同文件的投递分支、`minix3/minix/kernel/system.c:clear_ipc` | 入队条件成立才入队（多数路径还有其他位在） | `ipc.h:WILLRECEIVE`、`P_BLOCKEDON` |
| `0x00008` | `RTS_RECEIVING` | `minix3/minix/kernel/proc.c` 的 `mini_receive` | 同文件的 `mini_send` 命中收方、`minix3/minix/kernel/system.c:clear_ipc`、`system/do_exec.c` | 同上 | `P_BLOCKEDON`、`WILLRECEIVE` |
| `0x00010` | `RTS_SIGNALED` | `system.c:cause_sig` | `system/do_getksig.c`（PM 取走）、`system/do_fork.c`（子进程不继承） | 另一个位（`SIG_PENDING`）还在，通常不入队 | `do_getksig.c`、`do_endksig.c` |
| `0x00020` | `RTS_SIG_PENDING` | `system.c:cause_sig` | `system/do_endksig.c`（信号处理完毕）、`do_fork.c` | 命中跃迁 → 入队 | `do_endksig.c`、`do_update.c` |
| `0x00040` | `RTS_P_STOP` | `system/do_trace.c`（开始跟踪） | `do_trace.c`（三处恢复路径）、`do_fork.c` | 命中跃迁 → 入队 | `do_trace.c`、`do_fork.c` |
| `0x00080` | `RTS_NO_PRIV` | `main.c:kmain`（非可调度的 boot 成员）、`system/do_privctl.c:do_privctl`（收回）、`system/do_fork.c`（子进程先无权限） | `do_privctl.c`（`SYS_PRIV_ALLOW`）、`system.c:cause_sig`（切到备用信号管理器） | 命中跃迁 → 入队 | `do_privctl.c` 的 `EPERM` 检查、`do_update.c` |
| `0x00100` | `RTS_NO_ENDPOINT` | `minix3/minix/kernel/system.c:clear_endpoint`（进程退出） | 无具名解除点；`do_clear` 的 `RTS_SETFLAGS` 整值覆盖会清掉它（见下） | —— | `proc.c` 里 `mini_send`/`mini_receive` 的收件人检查 |
| `0x00200` | `RTS_VMINHIBIT` | `main.c` 的 `kmain`（除 VM 的全部用户进程）、`system/do_fork.c`、`do_vmctl.c`（`VMCTL_VMINHIBIT_SET`）、`smp.c` | `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl`（`VMCTL_SETADDRSPACE`）、`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`（`VMCTL_VMINHIBIT_CLEAR`） | 命中跃迁 → 入队 | `proc.c:has_pending`（异步投递跳过）、`memory.c` 的断言 |
| `0x00400` | `RTS_PAGEFAULT` | `arch/i386/exception.c`、`arch/earm/exception.c`（页错误处理入口） | `system/do_vmctl.c`（`VMCTL_CLEAR_PAGEFAULT`） | 命中跃迁 → 入队 | `do_vmctl.c` 的断言 |
| `0x00800` | `RTS_VMREQUEST` | `proc.c:vm_suspend`（发起内存请求） | `system/do_vmctl.c`（`VMCTL_MEMREQ_REPLY`）、`system.c:clear_memreq`（进程退出） | 命中跃迁 → 入队 | `do_vmctl.c` 的 `assert(RTS_ISSET(...))`、`kernel_call_resume` |
| `0x01000` | `RTS_VMREQTARGET` | **本仓检索范围内无置位点** | 同左 | —— | `debug.c` 的打印、`servers/mib/proc.c` 的读点 |
| `0x04000` | `RTS_PREEMPTED` | `minix3/minix/kernel/proc.c` 的 `enqueue` 抢占分支、`minix3/minix/kernel/smp.c:smp_ipi_sched_handler` 后处理 | `minix3/minix/kernel/proc.c:switch_to_user` 的 `not_runnable_pick_new` 段 | 出队；若仍可运行则按 `p_cpu_time_left` 决定插队首还是队尾 | `proc_is_preempted` |
| `0x08000` | `RTS_NO_QUANTUM` | `main.c:kmain`（与 `NO_PRIV` 成对）、`system/do_fork.c`、`system.c:sched_proc`、`proc.c:notify_scheduler` | `system.c:sched_proc`（末尾统一清） | 命中跃迁 → 入队 | `proc_no_quantum` |
| `0x10000` | `RTS_BOOTINHIBIT` | `main.c:kmain`（除 VM 的全部用户进程） | `system/do_vmctl.c`（`VMCTL_BOOTINHIBIT_CLEAR`） | 命中跃迁 → 入队 | 与 `VMINHIBIT` 同批检查 |

表里有两行属于"定义与使用不对称"的实例，单独说明如下。

`RTS_VMREQTARGET` 在整个 C 树里只有三处出现：定义（`proc.h`）、`debug.c` 的打印、以及 `minix3/minix/servers/mib/proc.c` 里的一次读取。没有 `RTS_SET(p, RTS_VMREQTARGET)` 这样的置位语句。也就是说这个位在当前代码里是一个"有名字、有打印、有人读，但没人置"的位——读它的人永远读到零。给出这个结论的检索必须覆盖空白变体，因为 `proc.h` 里 `#define RTS_VMREQTARGET` 后面是制表符而不是空格。下面三条按检索输出的实际顺序列出（`^I` 表示制表符）：

```text
minix3/minix/kernel/debug.c:156:^IFLAG(RTS_VMREQTARGET);
minix3/minix/kernel/proc.h:154:#define RTS_VMREQTARGET^I0x1000^I/* target of vm memory request */
minix3/minix/servers/mib/proc.c:358:^I^I    RTS_ISSET(kp, RTS_VMREQTARGET))
```

`RTS_NO_ENDPOINT` 是另一种不对称：它有置位点（`clear_endpoint`），却没有以 `RTS_UNSET(..., RTS_NO_ENDPOINT)` 形式出现的具名解除点。这个位的语义是"该槽不能再收发消息"，而槽的复用并不因此被堵死：`do_clear` 在 `clear_endpoint` 之后执行 `RTS_SETFLAGS(rc, RTS_SLOT_FREE)`（§2.1.6 已引），这个宏是整值覆盖而不是按位置位，`NO_ENDPOINT` 随整个 `p_rts_flags` 一起被覆盖成 `RTS_SLOT_FREE`；下一个占用该槽的进程从自己的权限授予与端点初始化路径重新获得收发能力。`proc.c` 里另留了一句注释 `/* XXX: RTS_NO_ENDPOINT should be removed */`（`minix3/minix/kernel/proc.c` 的 `mini_receive` 附近的收件人检查处），说明这个位本身被上游标记为待清理。本篇按"检索范围内未命中具名解除点"陈述，不推断它的历史意图。

Rust 侧的对应路径是 `os/kernel/src/syscall_process.rs:dispatch_clear` 与 `os/kernel/src/syscall.rs:clear_endpoint`：置位照做；回收那一步调的是 `rts_setflags(target_nr, RtsFlagsBits::SLOT_FREE)`（`os/kernel/src/proc_table.rs:ProcessTable::rts_setflags`——整值覆盖，其文档注释明写自己是 C 的 `RTS_SETFLAGS` 的对位），`NO_ENDPOINT` 随整个标志字一起被覆盖成精确空槽态。这条对位有回归测试固定：`syscall_process.rs` 的 `test` 断言回收后 `is_empty` 为真（`is_empty` 即"恰好等于 `RTS_SLOT_FREE`"的精确判据，与 C 的 `isemptyp` 同形）。

#### 2.2.3 位与位之间：为什么不是一位一件事

16 个位里，只有 `RTS_PREEMPTED` 和 `RTS_NO_QUANTUM` 的注释提到了解除后的入队位置（队首 / 队尾）。这不是注释的偶然：`switch_to_user` 里对 `RTS_PREEMPTED` 的处理是"清掉、若仍可运行则按剩余时间决定 `enqueue_head` 还是 `enqueue`"（`minix3/minix/kernel/proc.c:switch_to_user`），而 `RTS_NO_QUANTUM` 的清位统一发生在 `sched_proc` 末尾（`minix3/minix/kernel/system.c:sched_proc`）。

其余位的共同点是：它们只表达"现在不能跑的原因"，不表达"恢复后插哪儿"。这就解释了 §1.2 那条"全零才可运行"的判定为什么够用——队列位置这件事没有编码进位里，而是编码进"清位那一步由谁做"。

**去掉 `RTS_SET` 里的旧值读取会怎样**（写成"置位并出队"）：对 SENDREC 的阻塞路径来说，一个进程可能同时持 `RTS_SENDING` 与 `RTS_RECEIVING`（`proc.h:P_BLOCKEDON` 的注释解释了为什么两个位可以同时置着），第二次置位时它已经在队列外，`dequeue` 会把它的 `p_nextready` 接到别人身上或清成空——而它的 `p_nextready` 此刻是上一次入队时的残留值。对象落到具体字段：一个新入队进程的 `p_nextready` 被改写。

**去掉 `RTS_UNSET` 的入队条件会怎样**（改成"清位后若值为零就入队"）：`clear_ipc` 对每个相关进程清 `RTS_SENDING`/`RTS_RECEIVING`（`minix3/minix/kernel/system.c:clear_ipc`），其中可能有本来就跑着、这两个位根本没置的进程；它们的清位不改变值，判定却成立，于是同一个槽被 `enqueue` 两次。就绪队列是单链表，同一节点出现两次意味着链尾指向自己，`pick_proc` 遍历时不会终止。

### 2.3 `struct priv`：31 个字段与权限模板族

特权结构的声明如下（逐字引用，31 个字段）：

```c
/* 逐字引用，minix3/minix/kernel/priv.h:struct priv —— 完整结构体 */
struct priv {
  proc_nr_t s_proc_nr;		/* number of associated process */
  sys_id_t s_id;		/* index of this system structure */
  short s_flags;		/* PREEMTIBLE, BILLABLE, etc. */
  int s_init_flags;             /* initialization flags given to the process. */

  /* Asynchronous sends */
  vir_bytes s_asyntab;		/* addr. of table in process' address space */
  size_t s_asynsize;		/* number of elements in table. 0 when not in
				 * use
				 */
  endpoint_t s_asynendpoint;    /* the endpoint the asyn table belongs to. */

  short s_trap_mask;		/* allowed system call traps */
  sys_map_t s_ipc_to;		/* allowed destination processes */

  /* allowed kernel calls */
  bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE];

  endpoint_t s_sig_mgr;		/* signal manager for system signals */
  endpoint_t s_bak_sig_mgr;	/* backup signal manager for system signals */
  sys_map_t s_notify_pending;  	/* bit map with pending notifications */
  sys_map_t s_asyn_pending;	/* bit map with pending asyn messages */
  irq_id_t s_int_pending;	/* pending hardware interrupts */
  sigset_t s_sig_pending;	/* pending signals */
  ipc_filter_t *s_ipcf;         /* ipc filter (NULL when no filter is set) */

  minix_timer_t s_alarm_timer;	/* synchronous alarm timer */
  reg_t *s_stack_guard;		/* stack guard word for kernel tasks */

  char s_diag_sig;		/* send a SIGKMESS when diagnostics arrive? */

  int s_nr_io_range;		/* allowed I/O ports */
  struct io_range s_io_tab[NR_IO_RANGE];
  int s_nr_mem_range;		/* allowed memory ranges */
  struct minix_mem_range s_mem_tab[NR_MEM_RANGE];
  int s_nr_irq;			/* allowed IRQ lines */
  int s_irq_tab[NR_IRQ];
  vir_bytes s_grant_table;	/* grant table address of process, or 0 */
  int s_grant_entries;		/* no. of entries, or 0 */
  endpoint_t s_grant_endpoint;  /* the endpoint the grant table belongs to */
  vir_bytes s_state_table;	/* state table address of process, or 0 */
  int s_state_entries;		/* no. of entries, or 0 */
};
```

按职责分组（本节只展开与会话/授权相关的那几组；其余组的静态含义按分组名自明，运行期语义归权限管控篇）：

| 组 | 字段 | 组内条数 |
|---|---|---|
| 身份与角色 | `s_proc_nr` `s_id` `s_flags` `s_init_flags` | 4 |
| 异步发送表挂载点 | `s_asyntab` `s_asynsize` `s_asynendpoint` | 3 |
| IPC 三层掩码 | `s_trap_mask` `s_ipc_to` `s_k_call_mask` | 3 |
| 信号簿记 | `s_sig_mgr` `s_bak_sig_mgr` `s_notify_pending` `s_asyn_pending` `s_int_pending` `s_sig_pending` | 6 |
| I/O 与 IRQ 白名单 | `s_nr_io_range` `s_io_tab` `s_nr_irq` `s_irq_tab` | 4 |
| 内存白名单与过滤 | `s_nr_mem_range` `s_mem_tab` `s_ipcf` `s_stack_guard` `s_diag_sig` | 5 |
| 运行时挂载点 | `s_alarm_timer` `s_grant_table` `s_grant_entries` `s_grant_endpoint` `s_state_table` `s_state_entries` | 6 |

（4 + 3 + 3 + 6 + 4 + 5 + 6 = 31，与结构体声明一一对应。复算见 §6.2。）

`sys_id_t` 是 `short`（`minix3/minix/kernel/type.h:sys_id_t`），`sys_map_t` 是两个 32 位块组成的位图（`BITMAP_CHUNKS(NR_SYS_PROCS)` = 2）。三个内嵌数组的容量常量分别是 `NR_IO_RANGE = 64`、`NR_MEM_RANGE = 20`、`NR_IRQ = 16`（`minix3/minix/include/minix/config.h`）；`SYS_CALL_MASK_SIZE` 由 `NR_SYS_CALLS = 58` 向上取整到 2 个 32 位块（`minix3/minix/include/minix/com.h:NR_SYS_CALLS` 与 `minix3/minix/include/minix/com.h:SYS_CALL_MASK_SIZE`）。

#### 2.3.1 静态段、动态段与 `get_priv`

特权槽分两段，边界写在地址宏里（`minix3/minix/kernel/priv.h`）：`BEG_STATIC_PRIV_ADDR` 到 `END_STATIC_PRIV_ADDR` 是 `NR_STATIC_PRIV_IDS` 个静态槽，之后到 `END_PRIV_ADDR` 是动态槽。分配函数有两条路径：

```c
/* 逐字引用，minix3/minix/kernel/system.c:get_priv —— 完整函数体 */
int get_priv(
  register struct proc *rc,		/* new (child) process pointer */
  int priv_id				/* privilege id */
)
{
  register struct priv *sp;                 /* privilege structure */

  if(priv_id == NULL_PRIV_ID) {             /* allocate slot dynamically */
      for (sp = BEG_DYN_PRIV_ADDR; sp < END_DYN_PRIV_ADDR; ++sp)
          if (sp->s_proc_nr == NONE) break;
      if (sp >= END_DYN_PRIV_ADDR) return(ENOSPC);
  }
  else {                                    /* allocate slot from id */
      if(!is_static_priv_id(priv_id)) {
          return EINVAL;                    /* invalid static priv id */
      }
      if(priv[priv_id].s_proc_nr != NONE) {
          return EBUSY;                     /* slot already in use */
      }
      sp = &priv[priv_id];
  }
  rc->p_priv = sp;			    /* assign new slot */
  rc->p_priv->s_proc_nr = proc_nr(rc);	    /* set association */

  return(OK);
}
```

两条路径的差别不只是"线性扫描 vs 直接取址"。静态路径用调用者给的 id 作为槽号，因此 id 与进程号之间必须有一个固定映射（`static_priv_id(n) = NR_TASKS + n`）；动态路径从 `BEG_DYN_PRIV_ADDR` 起线性找第一个 `s_proc_nr == NONE` 的槽，耗尽返回 `ENOSPC`。两条路径失败时返回的错误码不同——`EINVAL`（id 越界）、`EBUSY`（静态槽已被占）、`ENOSPC`（动态段满）——这三个错误码在启动期各有对应的现实：看它们就大致能判断是"算错了 id"、"重复分配"还是"槽不够"。

函数最后两行是"绑定"：进程槽的 `p_priv` 指向特权槽，特权槽的 `s_proc_nr` 指回进程号。两个方向都要写，因为 `priv(rp)`（经 `p_priv`）与 `id_to_nr(id)`（经 `s_proc_nr`）是两个不同方向的查询。

#### 2.3.2 权限模板族

`kmain` 的 boot 循环给三类"立即可调度"的成员填权限，填的值来自一组预定义常量。常量的定义在 `minix3/minix/include/minix/priv.h`：

```c
/* 逐字引用，minix3/minix/include/minix/priv.h —— 旗标组合与三层掩码模板（按原文顺序节选；个别注释跨行处以行内 `...` 缩短） */
/* Allowed targets. */
#define NO_M      (-1)              /* no targets allowed */
#define ALL_M     (-2)              /* all targets allowed */

/* Allowed calls. */
#define NO_C      (-1)              /* no calls allowed */
#define ALL_C     (-2)              /* all calls allowed */
#define NULL_C    (-3)              /* null call entry */

...
/* privilege flags */
#define IDL_F     (SYS_PROC | BILLABLE) /* idle task is not preemptible as we ... */
#define TSK_F     (SYS_PROC)                       /* other kernel tasks */
#define SRV_F     (SYS_PROC | PREEMPTIBLE)         /* system services */
#define DSRV_F    (SRV_F | DYN_PRIV_ID)            /* dynamic system services */
#define RSYS_F    (SRV_F | ROOT_SYS_PROC)          /* root sys proc */
#define VM_F      (SYS_PROC | VM_SYS_PROC)         /* vm */
#define USR_F     (BILLABLE | PREEMPTIBLE)         /* user processes */
#define IMM_F     (ROOT_SYS_PROC | VM_SYS_PROC | PREEMPTIBLE) /* immutable */

...
/* allowed traps */
#define CSK_T     (1 << RECEIVE)                   /* clock and system */
#define TSK_T     0                                /* other kernel tasks */
#define SRV_T     (~0)                             /* system services */
#define DSRV_T    (~0)                             /* dynamic system services */
#define USR_T     (1 << SENDREC)                   /* user processes */

/* allowed targets */
#define TSK_M     NO_M                             /* all kernel tasks */
#define SRV_M     ALL_M                            /* system services */
#define DSRV_M    ALL_M                            /* dynamic system services */
#define USR_M     ALL_M                            /* user processes */

/* allowed kernel calls */
#define TSK_KC    NO_C                             /* all kernel tasks */
#define SRV_KC    ALL_C                            /* dynamic system services */
#define DSRV_KC   ALL_C                            /* default sys proc */
#define USR_KC    NO_C                             /* user processes */

...
/* signal manager */
#define SRV_SM    ROOT_SYS_PROC_NR                 /* system services */

...
/* scheduling priority queue. */
#define SRV_Q     USER_Q                           /* system services */

/* scheduling quantum. */
#define SRV_QT    USER_QUANTUM                     /* system services */

...
```

（节选，按原文顺序逐行复现。块内省略号处从上到下依次省去：`Default privilege settings` 块注释、init flags 一组（`TSK_I`/`SRV_I`/`DSRV_I`/`USR_I`）、vm calls 一组（`SRV_VC`/`DSRV_VC`/`USR_VC`）、scheduler 一组（`SRV_SCH`/`DSRV_SCH`/`USR_SCH`）与末尾的 `DSRV_CPU`；`IDL_F` 的注释原文是一段多行块注释（解释 idle 为何不可抢占），此处以省略号截断，其余行连注释按原文。）

模板里几个组合值的展开如下（`NR_TASKS` 的角色在 §2.3.1 的 `static_priv_id` 算式里已经出现过；`SELF` 是 `minix3/minix/include/minix/endpoint.h:SELF` 定义的"本进程"端点，值为 `_ENDPOINT_SLOT_TOP - 3`，在 boot 循环里只出现一次——VM 那一支的 `priv(rp)->s_sig_mgr = SELF`（`minix3/minix/kernel/main.c:kmain`），即 VM 自己给自己当信号管理器，而 RS 那一支填的是 `SRV_SM = ROOT_SYS_PROC_NR`）：

| 常量 | 位组合展开 | 说明 |
|---|---|---|
| `IDL_F` | `SYS_PROC \| BILLABLE` | idle task：是系统进程、要记账，但**不**可抢占 |
| `TSK_F` | `SYS_PROC` | 其余内核 task：只有系统进程位 |
| `VM_F` | `SYS_PROC \| VM_SYS_PROC` | VM：系统进程 + VM 角色位 |
| `RSYS_F` | `SYS_PROC \| PREEMPTIBLE \| ROOT_SYS_PROC` | RS：系统进程 + 可抢占 + 根服务角色位 |
| `TSK_T` | `0` | 内核 task 不允许任何 IPC trap |
| `CSK_T` | `1 << RECEIVE` | 只允许 RECEIVE——一个"只能收"的端点 |
| `SRV_T` | `~0` | 系统服务允许全部 trap |
| `TSK_M` | `NO_M = (-1)` | 内核 task 不能主动发给别人 |
| `SRV_M` | `ALL_M = (-2)` | 系统服务可以发给任意目标 |
| `TSK_KC` | `NO_C = (-1)` | 内核 task 不能调内核调用 |
| `SRV_KC` | `ALL_C = (-2)` | 系统服务可以调任意内核调用 |

`NO_M`/`ALL_M`/`NO_C`/`ALL_C` 是四个负数哨兵，它们不会出现在掩码里——boot 循环对它们做的是分支判断，而不是赋值：

```c
/* 逐字引用，minix3/minix/kernel/main.c:kmain —— 填 IPC 目标掩码与内核调用掩码 */
            /* Fill in target mask. */
            memset(&map, 0, sizeof(map));

            if (ipc_to_m == ALL_M) {
                for(j = 0; j < NR_SYS_PROCS; j++)
                    set_sys_bit(map, j);
            }

            fill_sendto_mask(rp, &map);

            /* Fill in kernel call mask. */
            for(j = 0; j < SYS_CALL_MASK_SIZE; j++) {
                priv(rp)->s_k_call_mask[j] = (kcalls == NO_C ? 0 : (~0));
            }
```

两层处理方式不同：IPC 目标掩码先把 `ALL_M` 展开成"64 个位全置"的局部位图，再交给 `fill_sendto_mask` 逐位落地；内核调用掩码只有两种取值，直接按 `NO_C` 判断写 0 或全 1。`SYS_CALL_MASK_SIZE` 个块（2 个）在这里被整体写成同样的值。

#### 2.3.3 `fill_sendto_mask`：自位恒清与双向授权

`fill_sendto_mask` 逐位走 `set_sendto_bit` 或 `unset_sendto_bit`，而 `set_sendto_bit` 里有两个守卫：

```c
/* 逐字引用，minix3/minix/kernel/system.c:set_sendto_bit —— 完整函数体 */
void set_sendto_bit(const struct proc *rp, int id)
{
/* Allow a process to send messages to the process(es) associated with the
 * system privilege structure with the given ID.
 */

  /* Disallow the process from sending to a process privilege structure with no
   * associated process, and disallow the process from sending to itself.
   */
  if (id_to_nr(id) == NONE || priv_id(rp) == id) {
	unset_sys_bit(priv(rp)->s_ipc_to, id);
	return;
  }

  set_sys_bit(priv(rp)->s_ipc_to, id);

  /* The process that this process can now send to, must be able to reply (or
   * vice versa). Therefore, its send mask should be updated as well. Ignore
   * receivers that don't support traps other than RECEIVE, they can't reply
   * or send messages anyway.
   */
  if (priv_addr(id)->s_trap_mask & ~((1 << RECEIVE)))
      set_sys_bit(priv_addr(id)->s_ipc_to, priv_id(rp));
}
```

两个守卫的后果不是"拒绝"，而是"把本方这一位清掉"——请求授予与执行清除共用同一条返回路径。第一个守卫针对"对端槽还没有关联进程"：`id_to_nr(id)` 即 `priv_addr(id)->s_proc_nr`，未关联时为 `NONE`；此时如果置位，等于给未来的某个占用者预授权。第二个守卫针对自位，因为自发送没有意义。

授予成功之后还有一步互惠：给对端的掩码里也置上本方的特权槽号，这样一对进程才能双向通话。例外是"对端不允许任何 RECEIVE 之外的 trap"的端点（比如 `CSK_T` 的 CLOCK/SYSTEM），它不会回发，所以跳过这一步。

`fill_sendto_mask` 把这件事对 64 个槽逐个做一遍：

```c
/* 逐字引用，minix3/minix/kernel/system.c:fill_sendto_mask —— 完整函数体 */
void fill_sendto_mask(const struct proc *rp, sys_map_t *map)
{
  int i;

  for (i=0; i < NR_SYS_PROCS; i++) {
  	if (get_sys_bit(*map, i))
  		set_sendto_bit(rp, i);
  	else
  		unset_sendto_bit(rp, i);
  }
}
```

把待填的位图交给它，得到的是"C 的终态"：`ALL_M` 展开成全 1 的位图，经过 fill 后本方自位被清掉、对端未关联的位被清掉、对端是纯收端点的位被置上但不互惠。`unset_sendto_bit` 那一侧还负责对称性：清本方位的同时清对端的反向位（`minix3/minix/kernel/system.c:unset_sendto_bit`）。

**把 `fill_sendto_mask` 换成整体赋值会怎样**（`priv(rp)->s_ipc_to = *map;`）：两个后果分别落到不同的对象上：本方掩码里自家槽号那一位置上了，掩码检查类宏 `may_send_to` 会让"自己给自己发消息"通过检查；更实质的是清位方向——`map` 里被清掉的位对应的对端，它们的反向位不会同步清除，而 `may_send_to` 是单向查询，于是出现"我不能再发给你、你却还能发给我"的单向授权，掩码对称这条不变量（`set_sendto_bit` 的注释称之为"must be able to reply"）失效。

### 2.4 boot image 静态表与顺序语义

§1.3 已经引用过 `table.c` 的 17 项清单与它的表头注释。那一大段注释里包含三条可以逐条核对的声明：

1. "The order of the entries here MUST agree with the order of the programs in the boot image"——数组顺序必须与引导器摆放模块的顺序一致。
2. "all kernel tasks must come first"——前 5 项必须是内核 task。
3. "The order of the entries here matches the priority NOTIFY messages are delivered to a given process. … DS must be the first system process in the list to allow reliable asynchronous publishing of system events. RS comes right after to prioritize ping messages periodically delivered to system processes."——顺序与 NOTIFY 的投递优先级对应，DS 必须是第一个系统服务。

前两条在代码里能看到明确的消费点。第一条的消费点是 boot 循环的模块下标：`kinfo.module_list[i - NR_TASKS]` 这一句隐含了"第 i 个清单条目对应第 i − 5 个模块"。第二条的消费点是同一个循环里的分支：前面 5 项只写名字，从第 6 项起才去模块链表取地址。

第三条需要把 NOTIFY 的投递路径读一遍才能判断。通知在收不到时被记成位图：

```c
/* 逐字引用，minix3/minix/kernel/proc.c:mini_notify —— 位图记账那一段 */
  /* Destination is not ready to receive the notification. Add it to the 
   * bit map with pending notifications. Note the indirectness: the privilege id
   * instead of the process number is used in the pending bit map.
   */ 
  src_id = priv(caller_ptr)->s_id;
  set_sys_bit(priv(dst_ptr)->s_notify_pending, src_id); 
```

源码注释把关键处写明白了：位图的下标是**特权槽号**，不是进程号。而指定的收方在扫描这张位图时取的是最低的一个置位：

```c
/* 逐字引用，minix3/minix/kernel/proc.c:has_pending —— ANY 分支的最低位置扫描 */
	/* Find a source with a pending message */
	for (src_id = 0; src_id < NR_SYS_PROCS; src_id += BITCHUNK_BITS) {
		if (get_sys_bits(*map, src_id) != 0) {
			while (!get_sys_bit(*map, src_id)) src_id++;
			goto quit_search;
		}
	}
```

（块内省略了 `CONFIG_SMP` 的替代分支与实际的两层循环结构；非 SMP 编译下就是"找到第一个非零块、在该块里找到第一个置位"。）

于是一个可以核对的结论是：通知投递的先后由**特权槽号的序**决定。而特权槽号对静态段来说是 `NR_TASKS + proc_nr`——RS 的槽号是 7、DS 的槽号是 11，二者之间是 MEM（8）、SCHED（9）、TTY（10）；PM（5）与 VFS（6）在 RS 之前。本篇在 C 内核的 NOTIFY 投递路径上未检出按 `image[]` 下标排序的代码；表头注释声明的那条优先级语义，其载体是清单顺序本身，而清单顺序在代码里的消费者是 boot 循环（第 1、2 条）与 VM 的启动遍历（下面这段）：

```c
/* 逐字引用，minix3/minix/servers/vm/main.c:init_vm —— 按清单顺序为各 boot 进程建页表 */
	/* Give these processes their own page table. */
	for (ip = &kernel_boot_info.boot_procs[0];
		ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
		struct vmproc *vmp;

		if(ip->proc_nr < 0) continue;

		assert(ip->start_addr);

		/* VM has already been set up by the kernel and pt_init().
		 * Any other boot process is already in memory and is set up
		 * here.
		 */
		if(ip->proc_nr == VM_PROC_NR) continue;

		vmp = init_proc(ip->proc_nr);

		exec_bootproc(vmp, ip);

		/* Free the file blob */
		assert(!(ip->start_addr % VM_PAGE_SIZE));
		ip->len = roundup(ip->len, VM_PAGE_SIZE);
		free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
	}
```

这段循环解释了"顺序"的第二个后果：VM 启动之后按 `kinfo.boot_procs[]` 的顺序给每个 boot 进程建自己的页表并装载 ELF，DS 排在第一个、RS 第二个。注意它按 `proc_nr` 跳过负值（内核 task）与 VM 自己，其余每个条目走一次 `init_proc`（线性扫描匹配 `proc_nr`）+ `exec_bootproc` + 归还文件块。

**把 `image[]` 里 DS 与 RS 两行调换会怎样**：`kinfo.boot_procs[]` 的顺序随之改变，VM 的遍历先给 RS 建页表；`init_proc` 的线性扫描不受影响（按 `proc_nr` 匹配），受影响的是启动期"谁先具备运行条件"的时序——这类改动的可观察差别不在单条分支上，而在多个服务竞争同一批资源的先后。

### 2.5 `proc_init`：261 槽的清表流程

清表函数的全文如下：

```c
/* 逐字引用，minix3/minix/kernel/proc.c:proc_init —— 完整函数体 */
void proc_init(void)
{
	struct proc * rp;
	struct priv *sp;
	int i;

	/* Clear the process table. Announce each slot as empty and set up
	 * mappings for proc_addr() and proc_nr() macros. Do the same for the
	 * table with privilege structures for the system processes. 
	 */
	for (rp = BEG_PROC_ADDR, i = -NR_TASKS; rp < END_PROC_ADDR; ++rp, ++i) {
		rp->p_rts_flags = RTS_SLOT_FREE;/* initialize free slot */
		rp->p_magic = PMAGIC;
		rp->p_nr = i;			/* proc number from ptr */
		rp->p_endpoint = _ENDPOINT(0, rp->p_nr); /* generation no. 0 */
		rp->p_scheduler = NULL;		/* no user space scheduler */
		rp->p_priority = 0;		/* no priority */
		rp->p_quantum_size_ms = 0;	/* no quantum size */

		/* arch-specific initialization */
		arch_proc_reset(rp);
	}
	for (sp = BEG_PRIV_ADDR, i = 0; sp < END_PRIV_ADDR; ++sp, ++i) {
		sp->s_proc_nr = NONE;		/* initialize as free */
		sp->s_id = (sys_id_t) i;	/* priv structure index */
		ppriv_addr[i] = sp;		/* priv ptr from number */
		sp->s_sig_mgr = NONE;		/* clear signal managers */
		sp->s_bak_sig_mgr = NONE;
	}

	idle_priv.s_flags = IDL_F;
	/* initialize IDLE structures for every CPU */
	for (i = 0; i < CONFIG_MAX_CPUS; i++) {
		struct proc * ip = get_cpu_var_ptr(i, idle_proc);
		ip->p_endpoint = IDLE;
		ip->p_priv = &idle_priv;
		/* must not let idle ever get scheduled */
		ip->p_rts_flags |= RTS_PROC_STOP;
		set_idle_name(ip->p_name, i);
	}
}
```

第一个循环写 7 个字段加一次架构复位。逐条看：

- `p_rts_flags = RTS_SLOT_FREE`（不是 0）——判空宏族 `isemptyp(p)`/`isemptyn(n)` 的判据是 `(p)->p_rts_flags == RTS_SLOT_FREE`（`minix3/minix/kernel/proc.h:isemptyn`），所以"空槽"必须是这个精确值，而不是"没有任何位"。
- `p_magic = PMAGIC`——把哨兵写好，此后 `proc_ptr_ok(p)` 才有意义。
- `p_nr = i`，而 `i` 的初值是 `-NR_TASKS` 且与指针同步自增——这一步把"槽下标 ↔ 进程号"的换算固化进数据本身。之后 `proc_nr(rp)` 就是读这个字段，而不是每次重新算下标（`minix3/minix/kernel/proc.h:proc_nr`）。
- `p_endpoint = _ENDPOINT(0, p_nr)`——generation 从 0 开始。
- `p_scheduler = NULL`、`p_priority = 0`、`p_quantum_size_ms = 0`——清表不赋有效调度参数，`p_priority = 0` 恰好是最高的 `TASK_Q`，但此刻 `p_scheduler` 是 `NULL`（内核默认策略），且所有槽都不可运行，所以没有调度后果。

`arch_proc_reset` 是架构层的复位：清寄存器、按"是否内核 task"写初值 PSW、写六个段选择子、给用户进程取 FPU 缓冲（§2.1.4 已引）。**它是这张表里唯一与硬件相关的部分**；把清寄存器、写状态字、给 FPU 缓冲这三件事收进一个架构方法，是 §3.5 那条设计决策的起点。

第二个循环写特权表的 5 项：`s_proc_nr = NONE`（未关联）、`s_id = i`（槽号即下标）、`ppriv_addr[i] = sp`（桥接表的指针）、`s_sig_mgr = NONE` 与 `s_bak_sig_mgr = NONE` 两个信号管理器。**注意这里没有清 `s_flags`**——BSS 零就是"没有任何能力位"，这是刻意的：启动期不触碰的字段保持零，含义是"未授权"。§2.3 的 31 字段表里 I/O、内存、运行时三组的字段全部不在清表写列表内，它们对未被授权的进程就是零值。

第三个循环给每个 CPU 的 idle 结构写 4 项（注意：`proc[]` 里另有一个 idle 条目——清单第 2 项、被 boot 循环填名字与权限模板——那份只是表内的一个身份槽；真正被调度器当兜底选中的是这里写的每 CPU `idle_proc`）：端点固定为 `IDLE`（宏值为 −4）、`p_priv` 指向文件级静态 `idle_priv`、`p_rts_flags |= RTS_PROC_STOP`、名字由 `set_idle_name` 按 CPU 编号生成。idle 结构**不在 `proc[]` 数组里**——它是 `__cpu_local_vars` 的一部分，所以它不参与进程号索引，也不需要 `p_nr`。

**用一次 `memset` 代替清表循环会怎样**：表面上"清零"更彻底，实际把三个判据同时破坏：`p_rts_flags` 变成 0 而不是 `RTS_SLOT_FREE`，`isemptyp` 恒假，于是 `clear_endpoint` 开头的 `if(isemptyp(rc)) panic("clear_proc: empty process: %d", rc->p_endpoint);`（`minix3/minix/kernel/system.c:clear_endpoint`）在第一次回收槽时触发 `panic`；`p_magic` 变成 0，`proc_ptr_ok` 从第一次校验起失败；`p_endpoint` 变成 0，而 0 既是 `_ENDPOINT(0, 0)`（PM 的端点）也是一个合法数值，故障表现是"所有未初始化槽都指向 PM"而不是立刻报错——最难查的一类。

### 2.6 boot 循环：逐项填充的 12 个动作

`kmain` 的 boot 循环是整个实例化过程的主体。按循环体内可分辨的动作块计数（口径：语句级动作；`schedulable_proc` 分支内部的四组赋值合并为 1 个动作），共 12 个：

| # | 动作 | 只对哪类成员 |
|---|---|---|
| 1 | `ip = &image[i]; rp = proc_addr(ip->proc_nr);` 取条目与目标槽 | 全部 |
| 2 | `ip->endpoint = rp->p_endpoint;` 从槽回填端点 | 全部 |
| 3 | `rp->p_cpu_time_left = 0;` 清时间片余量 | 全部 |
| 4 | `i < NR_TASKS`：`strlcpy(rp->p_name, ip->proc_name, ...)` | 内核 task |
| 5 | `i >= NR_TASKS`：从 `kinfo.module_list[i - NR_TASKS]` 回填 `start_addr`/`len` | 用户模块 |
| 6 | `reset_proc_accounting(rp)` | 全部 |
| 7 | 判 `schedulable_proc`，真则 `get_priv` + 填权限模板 + `fill_sendto_mask` + 填调用掩码 | 可调度者 |
| 8 | 假则 `RTS_SET(rp, RTS_NO_PRIV \| RTS_NO_QUANTUM)` | 其余 |
| 9 | `arch_boot_proc(ip, rp)`（VM 的 ELF 装载在这里） | 全部（内部对负号返回） |
| 10 | `if(!get_cpulocal_var(proc_ptr)) get_cpulocal_var(proc_ptr) = rp;` | 全部（只生效一次） |
| 11 | 非 VM 的用户进程：`\|= RTS_VMINHIBIT` 与 `\|= RTS_BOOTINHIBIT` | 用户进程（除 VM） |
| 12 | `\|= RTS_PROC_STOP` 与 `&= ~RTS_SLOT_FREE` | 全部 |

可调度性判定与三条分支的原文：

```c
/* 逐字引用，minix3/minix/kernel/main.c:kmain —— 可调度性判定与特权授予开头 */
	rp->p_cpu_time_left = 0;
	if(i < NR_TASKS)			/* name (tasks only) */
		strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name));

	if(i >= NR_TASKS) {
		/* Remember this so it can be passed to VM */
		multiboot_module_t *mb_mod = &kinfo.module_list[i - NR_TASKS];
		ip->start_addr = mb_mod->mod_start;
		ip->len = mb_mod->mod_end - mb_mod->mod_start;
	}
	
	reset_proc_accounting(rp);

	/* See if this process is immediately schedulable.
	 * In that case, set its privileges now and allow it to run.
	 * Only kernel tasks and the root system process get to run immediately.
	 * All the other system processes are inhibited from running by the
	 * RTS_NO_PRIV flag. They can only be scheduled once the root system
	 * process has set their privileges.
	 */
	proc_nr = proc_nr(rp);
	schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
		proc_nr == VM_PROC_NR);
	if(schedulable_proc) {
	    /* Assign privilege structure. Force a static privilege id. */
            (void) get_priv(rp, static_priv_id(proc_nr));
```

三个条件对应三类成员：`iskerneln(proc_nr)` 是"进程号小于零"（`minix3/minix/kernel/proc.h:iskerneln`），覆盖 5 个内核 task；`isrootsysn(proc_nr)` 是"进程号等于 `ROOT_SYS_PROC_NR`"（同文件的 `isrootsysn`，值为 RS 的 2）；第三个条件是 `proc_nr == VM_PROC_NR`（值为 8）。把这三类放进同一分支是有依赖顺序的：内核 task 的代码编在内核镜像里，不依赖任何装载；RS 由 VM 装载但它要早期运行以建立服务；VM 是唯一由内核亲自装载的进程，也是其他所有用户进程的页表建立者。其余 10 个用户模块（12 项减去当场授予的 RS 与 VM——RS 同轮拿到权限与优先级）在这一轮只被登记，等 VM 起来后由 VM 的遍历装载（§2.4）。

分支内部按成员类型填不同的模板（§2.3.2 的表），并调用 `fill_sendto_mask` 与内核调用掩码填充（§2.3.3 已逐行引用）。注意 `get_priv` 的返回值被强转成 `(void)`——启动期没有为"静态槽被占"或"id 越界"准备处理路径，C 选择不检查；这份信任的前提是"清表刚做完、没有任何槽被占"。

分支的另一半是**抑制**：

```c
/* 逐字引用，minix3/minix/kernel/main.c:kmain —— 不可调度者与循环尾部 */
	else {
	    /* Don't let the process run for now. */
            RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM);
	}

	/* Arch-specific state initialization. */
	arch_boot_proc(ip, rp);

	/* scheduling functions depend on proc_ptr pointing somewhere. */
	if(!get_cpulocal_var(proc_ptr))
		get_cpulocal_var(proc_ptr) = rp;

	/* Process isn't scheduled until VM has set up a pagetable for it. */
	if(rp->p_nr != VM_PROC_NR && rp->p_nr >= 0) {
		rp->p_rts_flags |= RTS_VMINHIBIT;
		rp->p_rts_flags |= RTS_BOOTINHIBIT;
	}

	rp->p_rts_flags |= RTS_PROC_STOP;
	rp->p_rts_flags &= ~RTS_SLOT_FREE;
	DEBUGEXTRA(("done\n"));
  }
```

尾部三行都用裸位运算而不是宏。`RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 用的是直接的 `|=` 而不是 `RTS_SET` 宏——此时进程已经不可运行（要么刚被 `RTS_SET` 置了两位，要么是内核 task（`RTS_SLOT_FREE` 尚未清，两项都不可运行）），`RTS_SET` 里的跃迁判定不会触发出队，两者等价；用裸 `|=` 表达的是"这一步不做队列动作"。`RTS_PROC_STOP` 与清 `RTS_SLOT_FREE` 同样用裸操作，理由相同。而 `proc_ptr` 那一句是"第一次赋值后不再改"——它给调度函数留了一个非空指针，具体指向谁不重要（第一个条目），因为此刻没有进程可运行，`pick_proc` 会走 idle 兜底。

**把表内条目的顺序倒过来会怎样**：表头第 2 条写着 `all kernel tasks must come first`，而 `i < NR_TASKS` 这个分支正是靠它把前 5 项按内核 task 处理（写名字、不回填模块地址）；顺序一乱，内核 task 会被当成用户模块去 `kinfo.module_list` 里取地址——错位的机制与 §1.4.1 里"按下标取进程号"那条同型。（循环方向本身倒过来不坏：每个动作只依赖条目自身，唯一随方向变的是 `proc_ptr` 兜底指向谁，而它本来就"指向谁不重要"。）

**把"双闸"（对用户进程同时置 `RTS_VMINHIBIT` 与 `RTS_BOOTINHIBIT` 的那一步）移出循环会怎样**（对全部 17 项统一置位）：VM 的 `p_nr` 等于 `VM_PROC_NR`，它也会被挂上双闸；而解除 `RTS_VMINHIBIT` 的两条路径一条是 VM 自己发 `VMCTL_SETADDRSPACE`（`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl`），另一条是 VM 发 `VMCTL_VMINHIBIT_CLEAR`（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`）——两条都以"VM 已在运行"为前提。于是没有任何进程能解除 VM 的抑制位，`pick_proc` 永远选不到 VM，`switch_to_user` 之后系统停在 idle 循环里，唯一的现象是"启动到此为止、无输出"。

**把 `RTS_VMINHIBIT` 与 `RTS_BOOTINHIBIT` 合成一位会怎样**：两个位的置位集合不同——boot 循环给除 VM 外的全部用户进程置两位；`do_fork` 只在 `PFF_VMINHIBIT` 标志下给子进程置 `VMINHIBIT`（`minix3/minix/kernel/system/do_fork.c:do_fork`），`VMCTL_VMINHIBIT_SET` 与 `smp.c` 的跨 CPU 路径也只置 `VMINHIBIT`。解除路径也不同——`VMINHIBIT` 有两条（`VMCTL_SETADDRSPACE` 在 VM 装好地址空间时顺带解除、`VMCTL_VMINHIBIT_CLEAR` 显式解除），`BOOTINHIBIT` 只有一条（`VMCTL_BOOTINHIBIT_CLEAR`，VM 装载完镜像时发出）。合成一位之后，解除集变成两条路径的并集，"谁先到谁清"：boot 进程会在页表刚装好、镜像还没装载时就被放行，第一次调度就在空地址空间上取指；反过来，fork 出的子进程只等"地址空间就绪"这一件事，它的解除者只来自 `VMINHIBIT` 那两条路径——如果合并后保留下来的解除路径是 boot 期专用的 `VMCTL_BOOTINHIBIT_CLEAR`，这类进程就再也没有解除者，永远停在 `pick_proc` 之外。两个位分开，表达的是"页表就绪"与"镜像就绪"两件事各有一位、各自有各自的解除者。

### 2.7 `arch_boot_proc` 与 VM ELF 装载

`arch_boot_proc` 是 boot 循环动作 9 对应的函数。它以"一个提前返回加一个分支"划分三类成员：

```c
/* 逐字引用，minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc —— 完整函数体 */
void arch_boot_proc(struct boot_image *ip, struct proc *rp)
{
	multiboot_module_t *mod;
	struct ps_strings *psp;
	char *sp;

	if(rp->p_nr < 0) return;

	mod = bootmod(rp->p_nr);

	/* Important special case: we put VM in the bootstrap pagetable
	 * so it can run.
	 */

	if(rp->p_nr == VM_PROC_NR) {
		struct exec_info execi;

		memset(&execi, 0, sizeof(execi));

		/* exec parameters */
		execi.stack_high = kinfo.user_sp;
		execi.stack_size = 64 * 1024;	/* not too crazy as it must be preallocated */
		execi.proc_e = ip->endpoint;
		execi.hdr = (char *) mod->mod_start; /* phys mem direct */
		execi.filesize = execi.hdr_len = mod->mod_end - mod->mod_start;
		strlcpy(execi.progname, ip->proc_name, sizeof(execi.progname));
		execi.frame_len = 0;

		/* callbacks for use in the kernel */
		execi.copymem = libexec_copy_memcpy;
		execi.clearmem = libexec_clear_memset;
		execi.allocmem_prealloc_junk = libexec_pg_alloc;
		execi.allocmem_prealloc_cleared = libexec_pg_alloc;
		execi.allocmem_ondemand = libexec_pg_alloc;
		execi.clearproc = NULL;

		/* parse VM ELF binary and alloc/map it into bootstrap pagetable */
		if(libexec_load_elf(&execi) != OK)
			panic("VM loading failed");

		/* Setup a ps_strings struct on the stack, pointing to the
		 * following argv, envp. */
		sp = (char *)execi.stack_high;
		sp -= sizeof(struct ps_strings);
		psp = (struct ps_strings *) sp;

		/* Take the stack pointer down three words to give startup code
		 * something to use as "argc", "argv" and "envp".
		 */
		sp -= (sizeof(void *) + sizeof(void *) + sizeof(int));

		// linear address space, so it is available.
		psp->ps_argvstr = (char **)(sp + sizeof(int));
		psp->ps_nargvstr = 0;
		psp->ps_envstr = psp->ps_argvstr + sizeof(void *);
		psp->ps_nenvstr = 0;

		arch_proc_init(rp, execi.pc, (vir_bytes)sp,
			execi.stack_high - sizeof(struct ps_strings),
			ip->proc_name);

		/* Free VM blob that was just copied into existence. */
		add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
		mod->mod_end = mod->mod_start = 0;

		/* Remember them */
		kinfo.vm_allocated_bytes = alloc_for_vm;
	}
}
```

两个分类点把成员分成三类：开头的 `if(rp->p_nr < 0) return;` 让内核 task 直接返回；`p_nr >= 0` 但不是 VM 的用户模块，取到 `mod` 之后不进 VM 分支、不做任何事就走到函数末尾——它们的 ELF 由 VM 在启动后装载；只有 VM 走完整段。

Rust 侧同一处的角色注释把这两类写成"其镜像由 **VM** 在运行期装载（C 的 boot 侧只把 VM 装进自举页表，其余启动镜像的页表由 VM 建；不是 RS 自己）"（`os/arch/src/arch/boot.rs:enum ProcKind` 的 `RootService`/`UserService` 两个变体），与 C 里实际装载它们的是 VM（§2.4 的 `init_vm` 遍历）一致；仓内"给非 VM 装载"的入口是 `load_process_elf`（`os/arch/src/arch/boot.rs:fn load_process_elf`，与 `load_vm_elf` 共用 `load_elf_into`），它目前的调用者只有测试内核（`os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs`），生产启动路径尚未接上——其余十个模块在 Rust 侧的装载归属按现状记在 `ProcKind` 的角色注释里。

对 VM 这一支，逐段读：

1. **`execi.hdr = (char *) mod->mod_start`**：ELF 头直接指向模块的物理地址。此刻页表是 bootstrap 那份，模块区间在恒等映射下可被内核直接读到。注释里那句 `phys mem direct` 就是这件事。
2. **六个回调全部指向内核自己的实现**：`copymem`/`clearmem` 用 `memcpy`/`memset`，三个 `allocmem_*` 共用 `libexec_pg_alloc`。后者是"边分配边映射"的落点：

```c
/* 逐字引用，minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc —— 完整函数体 */
static int libexec_pg_alloc(struct exec_info *execi, vir_bytes vaddr, size_t len)
{
        pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo);
  	pg_load();
        memset((char *) vaddr, 0, len);
	alloc_for_vm += len;
        return OK;
}
```

`pg_map` 的 `PG_ALLOCATEME` 标志要求这段映射顺带分配物理帧，`pg_load()` 把刚动过的页目录项装进硬件，`memset` 清零（覆盖 `.bss`），`alloc_for_vm` 记账。这条路径是 `kernel_may_alloc` 窗口必须打开的**理由**（`03-kmain-cstart.md` §2.1 已从窗口那一侧讲过同一件事）：内核在 VM 运行之前直接使用主存里的空闲段，用的就是这一段代码。

3. **栈顶三步减法**：`sp` 从 `kinfo.user_sp` 开始，先退 `sizeof(struct ps_strings)` 放描述符，再退三个字给 `argc`/`argv`/`envp`。`ps_strings` 的四个字段按地址关系指回去：`ps_argvstr` 指向 `sp + sizeof(int)`（跳过 `argc`），`ps_envstr` 指向 `ps_argvstr` 之后一个字。三字布局的口径是"一个 `int` 加两个指针"，`sizeof(int)` 在此处恰好是一个指针宽度的一半；64 位 LP64 下这个口径会带来出生栈指针的对齐问题，本篇 §4.2 会对照。
4. **`arch_proc_init`** 写入口三要素：清一次上下文、写名字、再写 `pc`/`sp`/`bx`（`minix3/minix/kernel/arch/i386/memory.c:arch_proc_init`）。`ps_str` 落在 `bx` 上是 32 位 x86 的寄存器约定。
5. **回收模块物理内存**：`add_memmap` 把模块区间还给空闲清单，随后把 `mod_start`/`mod_end` 置 0。两行必须成对：回收之后若还留着地址，后续任何按 `ip->start_addr` 再回收一次的路径会重复归还同一区间。

五段里最容易出事的是第 3 段（栈顶的三字布局）：它是唯一没有 64 位 C 真源可抄的一段——`ARGC_ARGV_ENVP` 的注释写明上游只有 i386/ARM32 的 `protect.c`（那两种位宽下 `int` 与指针同宽，两种写法重合），并且记录了本端在这一段出过一次对齐错误（旧值 `2*usize + i32` 让出生 sp 落在 4-mod-8 上，§4.2 会引这段注释）。**这一处的"坏样"要说清**：按源码注释自己的记录，它**当前没有可观察症状**——消费端只读 `ps_strings` 描述符（字段序与写入逐字对位），`argc` 槽那一个字本身没有读者，所以旧写法被记成"对现行行为惰性"；代价是潜伏的：将来若有人把 `argc` 槽当整字读，非 8 字节对齐就会当场暴露。其余四段都能在 C 里找到逐行对位：ELF 头的物理直读、三个 `allocmem_*` 共用的 `pg_map` 路径、`arch_proc_init` 的入口三要素、以及 `add_memmap` 加置零这一对。

**删掉 `add_memmap` 那一对句子里的前者会怎样**（只置 0）：模块原本占的那段物理内存在启动早期被 `cut_memmap` 切走过，此后再也没有归还路径，空闲清单里永远少一整段；表现是"可分配内存比物理内存少了一块（正好是 VM 镜像大小）"。反过来，只保留 `add_memmap` 而删掉置 0：`mod_start`/`mod_end` 仍指向同一段，`kinfo.boot_procs` 的这份副本会被 VM 读到，VM 的遍历里有 `assert(ip->start_addr);` 与随后的 `free_mem(...)`——同一块内存被归还两次，空闲清单出现重叠区间。

### 2.8 endpoint、generation 与槽位常量体系

endpoint 是"槽号 + 代"的打包值。三个宏给出打包与拆包：

```c
/* 逐字引用，minix3/minix/include/minix/endpoint.h —— 打包与拆包三宏 */
#define _ENDPOINT(g, p) \
	((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)
#define _ENDPOINT_P(e) \
	((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

`_ENDPOINT_GENERATION_SHIFT` 是 15、`_ENDPOINT_GENERATION_SIZE` 是 `1 << 15`（同一个头文件），`MAX_NR_TASKS` 是 1023（`minix3/minix/include/minix/com.h:MAX_NR_TASKS`）。拆包先加 1023 再取低位/右移，最后减回去——这一手让负的槽号也能落在同一段地址里（本篇绘制的位域图）：

```text
   endpoint（32 位）＝ [ 代（高位） | 槽（低 15 位） ]
   槽的编码：负数域 -1023..-1 与正数域 0..255 共用低 15 位
   例：_ENDPOINT(0, 6)  = 6      （DS，0 号代）
        _ENDPOINT(0, -3) = -3     （CLOCK）
   特殊值（同一头文件）：
       _ENDPOINT_MAX_GENERATION = INT_MAX/2^15 - 1
       槽号上限 _ENDPOINT_SLOT_TOP = 2^15 - 1023
```

`proc_init` 写的是 0 号代（`_ENDPOINT(0, p_nr)`）。每次槽被复用时代号递增，回绕时跳过 0：

```c
/* 逐字引用，minix3/minix/kernel/system/do_fork.c:do_fork —— 代号递增那几行 */
  gen = _ENDPOINT_G(rpc->p_endpoint);
#if defined(__i386__)
  old_fpu_save_area_p = rpc->p_seg.fpu_state;
#endif
  *rpc = *rpp;				/* copy 'proc' struct */
...
  if(++gen >= _ENDPOINT_MAX_GENERATION)	/* increase generation */
	gen = 1;			/* generation number wraparound */
  rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;	/* this was obliterated by copy */
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);	/* new endpoint of slot */
```

（节选，`minix3/minix/kernel/system/do_fork.c:do_fork`——块内 `...` 处为 i386 分支的 FPU 缓冲指针保存/恢复与 `memcpy` 两段，属 §3.13 的主题。）

代号校验的读点在 `isokendpt`：拆出槽号、检查范围、再确认"该槽此刻的 `p_endpoint` 与传入值相等"（`minix3/minix/kernel/proc.c` 的 `isokendpt_f`，经 `isokendpt` 宏包装）。第三个条件正是代号在起作用：槽已复用但代号不同时，这个比较失败。

负端点的取值与名字在 `com.h` 里集中定义：`ASYNCM = -5`、`IDLE = -4`、`CLOCK = -3`、`SYSTEM = -2`、`KERNEL = -1`，其中 `HARDWARE` 就是 `KERNEL` 的别名。`NR_TASKS` 是 5，因此负端点恰好占满 `[-NR_TASKS, 0)` 这一段。

槽位定位的宏族把"数组下标"与"进程号"之间的算术写死在一处：

| 宏 | 展开 | 用途 |
|---|---|---|
| `BEG_PROC_ADDR` / `END_PROC_ADDR` | `&proc[0]` / `&proc[NR_TASKS + NR_PROCS]` | 遍历整表 |
| `BEG_USER_ADDR` | `&proc[NR_TASKS]` | 遍历/划分用户段 |
| `proc_addr(n)` | `(&(proc[NR_TASKS + (n)]))` | 进程号 → 槽指针 |
| `proc_nr(p)` | `((p)->p_nr)` | 槽指针 → 进程号 |
| `isokprocn(n)` | `((unsigned) ((n) + NR_TASKS) < NR_PROCS + NR_TASKS)` | 一次判断排除两端越界 |
| `isemptyn(n)` / `isemptyp(p)` | `isemptyp(proc_addr(n))` / `((p)->p_rts_flags == RTS_SLOT_FREE)` | 判空 |
| `iskernelp(p)` / `iskerneln(n)` | `((p) < BEG_USER_ADDR)` / `((n) < 0)` | 内核 task 判定 |
| `isuserp(p)` / `isusern(n)` | `isusern((p) >= BEG_USER_ADDR)` / `((n) >= 0)` | 用户进程判定 |
| `isrootsysn(n)` | `((n) == ROOT_SYS_PROC_NR)` | 根系统进程判定 |

`isokprocn` 的写法把两端边界压进一次比较：`(n) + NR_TASKS` 先把负数域平移到 0 起点，再转成无符号与上界比较——`n = -6` 时左边是 `-1` 的无符号形式（极大值），比较不成立；`n = 256` 时左边是 261，不小于 261，同样不成立。一次算术同时挡住两端。

**让端点代号恒为 0 会怎样**（去掉 `do_fork` 里的递增）：`isokendpt` 的第三个条件退化成"槽号对得上就算合法"，于是持有旧 endpoint 的进程发来的消息会落到槽的新占用者身上：一个已经退出的进程的端点号，在槽被复用后完全相同，收件人检查与代号比较都会通过。这类错误的可见形态是"消息被投给了错误的进程"，且只在槽复用之后出现——正是"回绕时 `gen = 1` 而不是 0"这句代码要防的点（代号 0 是 `proc_init` 给全新槽的初值，回绕若落到 0，"全新槽"与"刚复用过的槽"无法区分）。

### 2.9 本章归纳

本章读到的状态变化收成两张表。清表之后：

| 对象 | 状态 |
|---|---|
| 261 个进程槽 | `p_rts_flags == RTS_SLOT_FREE`、`p_magic == PMAGIC`、`p_nr` 与 `p_endpoint`（0 号代）已定、`p_scheduler == NULL`、`p_priority == 0`、`p_quantum_size_ms == 0`、架构上下文为初值；其余字段为 BSS 零 |
| 64 个特权槽 | `s_proc_nr == NONE`、`s_id == 槽号`、`ppriv_addr[]` 就位、两个信号管理器为 `NONE`；其余字段为零 |
| 每 CPU 的 idle 槽 | 端点 `IDLE`、`p_priv` 指向静态 `idle_priv`、`RTS_PROC_STOP` 置位 |

填充之后（17 项 boot 成员）：

| 对象 | 状态 |
|---|---|
| 内核 task（5 项） | 名字已写、权限模板已填（idle 用 `IDL_F`，其余 `TSK_F`；CLOCK/SYSTEM 的 trap 掩码为 `CSK_T`）、`RTS_PROC_STOP` 置位、`SLOT_FREE` 已清 |
| 可调度用户进程（VM、RS） | 权限模板已填（VM `VM_F`、RS `RSYS_F`）、优先级与时间片已给、`RTS_PROC_STOP` 置位。**名字**：VM 已写（`arch_proc_init` 在 `arch_boot_proc` 的 VM 分支里调用）、RS 未写（RS 属"其余 11 个"，名字在它自己 exec 时写——§2.1.1）；**双闸**：RS 带 `VMINHIBIT|BOOTINHIBIT`（§1.4），VM 不带；VM 的 ELF 已在 bootstrap 页表里 |
| 其余用户模块（10 项） | 名字与模块地址已登记、`RTS_NO_PRIV \| RTS_NO_QUANTUM` 置位、双闸置位、`RTS_PROC_STOP` 置位；ELF 与页表等 VM |
| 全部 17 项 | `SLOT_FREE` 已清；`p_cpu_time_left == 0`；记账六项已归零 |

两表的差别在分工：清表只写"这个槽存在"，填充才写"这个槽是谁、能做什么"。而"现在能不能跑"由 16 个位共同表达，其中 4 个位在填充阶段被置上、等待不同角色的解除者。

**本章交代给下一章的东西**：两张静态数组的存储形态与容量（§2.0）与状态位图的字段形态（§2.1.6、§2.2）、字段语义（§2.1–§2.3）、清单与顺序（§2.4）、清表与填充的动作序列（§2.5–§2.6）、VM 装载（§2.7）、身份数值（§2.8）。下一章回答的是同一批问题的另一种表达方式，以及这些负担里哪些在新环境下不再存在。

---

## 3. Rust 设计决策

本章 14 节按"两条硬约束（零堆、锁由 BKL 提供 → 表的存储与同步形态）+ 逐对象决策"组织：3.1–3.5 是两张表与清单的类型化重建，3.6–3.9 是执行环境相关决策（FPU、装载、全局存储、抑制位），3.10–3.12 是特权结构与主流程形状，3.13 补 fork 继承，3.14 是去向总表。

### 3.1 两张表的存储：编译期定容 + 索引化访问（→§2.0、§2.1、§2.5）

C 的负担很具体：`proc[]` 与 `priv[]` 是 BSS 里的定容数组（§2.0 第一、二层），`proc_addr(n)` 返回裸指针且不做边界检查（§2.8 的宏族），`proc_init` 靠一个运行期循环把 261 个槽逐字段写成已知值（§2.5）。Rust 侧要回答三个问题：这份存储放在哪、指针换成什么、清表循环去哪了。

存储的位置是 `os/kernel/src/globals.rs` 的两个 `static`：

```rust
/// Global process table — C's `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`.
///
/// # Storage (06-proc-init-boot-proc.md §3.1)
///
/// `SyncUnsafeCell` is the Rust 2024 translation of C's BSS `EXTERN` array:
/// compile-time-fixed address, zero heap, zero runtime overhead, with explicit
/// `Sync` (BKL guards all access). The `#![no_std]` kernel has no allocator at
/// boot time, so `Box<[KProcess]>` is forbidden here.
///
/// # SAFETY
///
/// All access requires the Big Kernel Lock (BKL). The BKL serializes all
/// kernel code, so at most one CPU mutates `PROC_TABLE` at a time. Boot-time
/// init (single-threaded, before BKL exists) is also safe.
pub(crate) static PROC_TABLE: SyncUnsafeCell<crate::proc_table::ProcessTable> = SyncUnsafeCell::new(crate::proc_table::ProcessTable::new());
```

（逐字引用，`os/kernel/src/globals.rs:static PROC_TABLE`；紧随其后的 `PRIV_TABLE` 是同一段注释结构、内容为"Same storage / safety model as `PROC_TABLE`"。）

三个要素：类型是 `SyncUnsafeCell<T>` 而不是 `Mutex<T>`，初值是 `ProcessTable::new()`（一个 `const fn`），内部是定长数组：

```rust
/// # Storage (06-proc-init-boot-proc.md §3.1)
///
/// `procs` is a fixed-size array `[KProcess; PROC_TABLE_SIZE]`, NOT a
/// `Box<[KProcess]>`. This eliminates heap allocation in the boot phase
/// (`#![no_std]` + no allocator yet) and gives a compile-time-fixed address
/// (matching C's `EXTERN struct proc proc[NR_TASKS + NR_PROCS]` in BSS).
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
```

（节选，`os/kernel/src/proc_table.rs:struct ProcessTable`——结构体还有两个字段 `sched: [Scheduler; MAX_CPUS]` 与 `vm_request_queue`，与本节无关，此处省略。）

那个 `const fn` 就是"C 的清表循环去哪了"的答案：

```rust
    pub const fn new() -> Self {
        let mut procs = [const { KProcess::new_zeroed() }; PROC_TABLE_SIZE];
        let mut i = 0;
        while i < PROC_TABLE_SIZE {
            let nr = ProcNr(i as i32 - NR_TASKS as i32);
            procs[i].p_nr = nr;
            procs[i].p_endpoint = Endpoint::from_generation_slot(0, nr.0);
            i += 1;
        }
```

（节选，`os/kernel/src/proc_table.rs:ProcessTable::new`——块内省略了其后对 idle 槽的三处赋值（端点、`RTS_PROC_STOP`、名字）与一次下标计算（`let idle_idx`），以及结构体构造；行内注释为原文。）

逐行对照 §2.5 的 C 循环：`i` 从 0 走到 260，进程号按 `i - NR_TASKS` 得到（C 那边是计数器从 `-NR_TASKS` 起步，同一个算式）；`p_endpoint` 用 `from_generation_slot(0, nr)`，与 `_ENDPOINT(0, p_nr)` 同形。差别在于这段循环在**编译期**执行：产物是 `PROC_TABLE` 这个 `static` 的初值，运行期没有对应的清表调用。语义上等价——每次上电都从这份初值开始——所以"清表"这件事在 Rust 里退化成了"初始化一份静态数据"。

访问路径的第二个变化是把"校验"与"访问"合成一次：

```rust
/// C: `isokprocn(n)` + `proc_addr(n)` combined — the C versions are separate
/// (isokprocn validates, proc_addr accesses without validation).
/// Rust unifies both into one function that returns `Option<usize>`.
///
/// Formula: `index = nr + NR_TASKS` (same as C's `proc_addr` offset).
/// Kernel tasks (nr < 0) map to indices 0..NR_TASKS-1.
/// User processes (nr >= 0) map to indices NR_TASKS..NR_TASKS+NR_PROCS-1.
pub(crate) const fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    let offset = nr.0 as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}
```

（逐字引用，`os/kernel/src/proc_table.rs:fn nr_to_idx`；其上的文档注释里另有两行属性与一条借用说明，与本节的判据无关，未录入。）

C 那边 `isokprocn` 只是一个可以被忘记调用的判定宏，`proc_addr` 本身不检查；Rust 把两者并成 `nr_to_idx(nr) -> Option<usize>`，越界与负溢出都返回 `None`。上层访问器里 `get`/`get_mut`/`is_valid_nr`/`is_empty` 都从它出发（`is_empty` 经 `get` 间接经过它），所以"忘记校验"这件事在类型上不可能发生——要拿到 `&KProcess` 就必须先经过 `Option` 的解包。`is_kernel` 是例外：它直接读符号位 `nr.0 < 0`（`os/kernel/src/proc_table.rs:ProcessTable::is_kernel`），与 C 的 `iskerneln(n)` 同形，不需要下标也不需要 `nr_to_idx`。

取舍如下：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `static SyncUnsafeCell<ProcessTable>` + 定长数组 | `Box<[KProcess]>`；`Vec<KProcess>`；`OnceLock<...>` 惰性初始化 | 每个访问点自己举证"持有 BKL 或处于单线程启动"；`Sync` 只保证能放进 `static`，不保证并发安全 |
| `const fn` 编译期初始化 | 运行期 `proc_init` 循环 | 初值必须在 `const` 可求值范围内（`bitflags::bits()` 不可用，见 §3.8，故用裸位值） |
| `nr_to_idx -> Option<usize>` | 裸指针 + `isokprocn` 前置检查 | 每个访问点多一次 `Option` 处理 |

**把 `get`/`get_mut` 换成返回裸指针 `*mut KProcess` 会怎样**：§4.4 要讲的 IPC 队列（同一进程可能同时是"某队列的节点"与"另一队列的队头"）看似需要两个同时有效的可变引用——但那个需求已由 §4.5 的 `get2_mut` 用一次安全拆分覆盖；整体换裸指针真正的代价在别处：`nr_to_idx` 的边界校验会从每个访问点撤走，越界从"返回 `None`"退化成"未定义的内存访问"。（加一把 `Mutex` 的形态也不成立：BKL 已串行化整个内核，它只多一次锁获取与第二套纪律；而启动期初始化发生在 BKL 建立之前——`kmain` 里 `BKL_LOCK()` 与 boot 循环的先后见 `03-kmain-cstart.md`，那时没有任何锁可用。）

### 3.2 权限模板与能力授予（→§2.3、§2.6）

C 的负担是"6 个字段逐个赋值 + 一层位图填充"（§2.3.2、§2.3.3）：`s_flags`、`s_init_flags`、`s_trap_mask`、`s_ipc_to`、`s_k_call_mask`、`s_sig_mgr` 六项每次都要给对，漏一项不会编译报错；而 `ipc_to` 还要再经过 `fill_sendto_mask` 才能落到终态。Rust 把"选角色"与"填字段"合成一次调用，模板只描述角色：

```rust
/// Capability template — "starter kit" of capabilities for a category of process.
///
/// Replaces the previous two-step `assign_static` +
/// `configure_boot_priv` boot flow (06-proc-init-boot-proc.md §3.2). Picking
/// a template once is correct by construction; you cannot forget to
/// set the IPC mask or to deny kernel calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTemplate {
    /// Idle kernel task: runs only when nothing else can.
    /// C: `IDL_F` flag set; `ipc_to = NO_M` (cannot SEND to other endpoints),
    /// `trap_mask = TSK_T = 0` (no IPC traps), no kcalls.
    Idle,
    /// Ordinary kernel task (CLOCK / SYSTEM / KERNEL / ASYNCM).
    /// C: `TSK_F` flag set; `ipc_to = NO_M` (cannot SEND to other endpoints).
    /// ...
    KernelTask,
    /// VM server. C: `VM_F`; ALL_M IPC targets, ALL_C kernel calls.
    Vm,
    /// Root system service (RS). C: `RSYS_F`; ALL_M / ALL_C.
    RootService,
    /// Process that has no privilege yet (slot reserved, waiting for
    /// RS to grant capabilities at runtime). C: no flags; NO_M / NO_C.
    Deferred,
}
```

（节选，`os/kernel/src/capability.rs:enum CapabilityTemplate`；块内两处 `...` 分别省略 `KernelTask` 的第三行注释（关于 CLOCK/SYSTEM 的 `CSK_T` 例外由授予函数补）与 `Deferred` 之前的空行。五个变体齐全。）

五个变体对照 §2.3.2 的常量族：

| 变体 | C 对应 | `s_flags` | trap 掩码 | IPC 目标 | 内核调用 | 用在哪 |
|---|---|---|---|---|---|---|
| `Idle` | `IDL_F` | `SYS_PROC \| BILLABLE` | `TSK_T = 0` | `NO_M` | `NO_C` | 每 CPU 的 idle 槽 |
| `KernelTask` | `TSK_F` | `SYS_PROC` | `TSK_T`（CLOCK/SYSTEM 例外为 `CSK_T`） | `NO_M` | `NO_C` | 其余 4 个内核 task |
| `Vm` | `VM_F` | `SYS_PROC \| VM_SYS_PROC` | `SRV_T = ~0` | `ALL_M` | `ALL_C` | VM |
| `RootService` | `RSYS_F` | `SYS_PROC \| PREEMPTIBLE \| ROOT_SYS_PROC` | `SRV_T` | `ALL_M` | `ALL_C` | RS |
| `Deferred` | 无旗标 | 空 | 无 trap | `NO_M` | `NO_C` | 等 RS 授权的用户模块 |

模板是角色级抽象，实例级例外留在授予函数里。`KernelTask` 的注释把这条分工写明："`trap_mask()` here returns the generic role default (`NONE`); the CLOCK/SYSTEM `CSK_T` exception is applied by `PrivTable::grant_capability` where the `proc_nr` is known"。授予函数的实现：

```rust
        // Start from the role default, then apply per-process exceptions
        // (mirrors C's `main.c:218-219` ternary). The CLOCK/SYSTEM exception
        // cannot live in `CapabilityTemplate::trap_mask()` because templates
        // are role-level abstractions (proc_nr is instance-level).
        let mut trap_mask = template.trap_mask();
        if matches!(template, crate::capability::CapabilityTemplate::KernelTask) {
            // Newtype comparison (no raw-slot deconstruction): same
            // `(proc_nr == CLOCK || proc_nr == SYSTEM)` test as C main.c:218-219.
            if proc_nr == crate::proc::proc_nr::CLOCK
                || proc_nr == crate::proc::proc_nr::SYSTEM
            {
                trap_mask = TrapMask::RECEIVE;
            }
        }
```

（节选，`os/kernel/src/kpriv.rs:PrivTable::grant_capability`——块内省略了函数开头的静态槽分配、中段的六个字段赋值与末尾的 `fill_sendto_mask` 调用。）

末尾那一步是 C 的终态来源：

```rust
        // D-58 (C main.c:244): route the seed template mask through the
        // whole-table fill. The fill guards clear the process's own bit
        // (C boot terminal state: a process never holds its own IPC bit —
        // self-send is meaningless and the reciprocity invariant is
        // maintained symmetrically across the table). The template itself
        // stays correct-by-construction (ALL for services); the fill
        // produces C's terminal state.
        self.fill_sendto_mask(priv_id, template.ipc_mask());
```

（逐字引用，`os/kernel/src/kpriv.rs:PrivTable::grant_capability` 末段；块内 `D-58` 是仓库内部阶段编号，按原文引用。）

模板给出的是意图（服务类角色是 `ALL`，含自位），`fill_sendto_mask` 把它走成 C 的终态（自位被守卫清掉）。这条分工不能省：若把模板掩码直接赋给 `s_ipc_to`，自位就留在掩码里（§2.3.3 的反向推演已经算过后果）。

失败类型也参与了这次设计。C 侧 `get_priv` 的静态路径返回 `EINVAL`/`EBUSY`（§2.3.1），动态路径返回 `ENOSPC`；Rust 早期版本把所有失败折成一个 `Option<PrivId>`，于是"静态槽被占"（编译期不变量被破坏，是 bug）与"动态段满"（可重试）不可区分。现在是一个枚举：

```rust
/// Errors returned by [`PrivTable::grant_capability`] (06-proc-init-boot-proc.md §3.2).
///
/// Replaces the previous `Option<PrivId>` return, which conflated
/// "out of slots" with "slot already occupied" and gave callers no way
/// to distinguish transient (retry-able) from permanent (bug) failures.
```

（逐字引用，`os/kernel/src/capability.rs:enum CapabilityError` 的文档注释。）

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 模板 + 一次 `grant_capability` | C 的两步（`assign_static` + `configure_boot_priv`） | 模板之外的定制要显式走 `configure_boot_priv`；模板集合需要随角色增加而维护 |
| 实例例外（CLOCK/SYSTEM）留在授予函数 | 为 `KernelTask` 加两个变体 | 授予函数里多一个 `if`，但模板仍是 5 个 |
| `CapabilityError` 枚举 | `Option<PrivId>` | 调用点要多写一行错误文案 |

**若省掉 `fill_sendto_mask` 会怎样**：终态就不是 C 的终态——自位留在 `s_ipc_to` 里、掩码表里残留上一次填充留下的单向授权；完整的失败链见 §2.3.3 的反推（本处只做对位提醒）。

### 3.3 RTS/MF 位图的类型化（→§2.2、§2.1.6）

C 的负担是裸位常量加一个 `u32` 字段（§2.1.6）：`0x00004` 这类字面值散落在各调用点，写错一个位号不会编译报错；`RTS_SET(rp, f)` 宏的 `f` 参数是 `int`，把 `MF_*` 的值传进去同样合法。Rust 侧把 16 位与 18 位各自做成类型：

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RtsFlagsBits: u32 {
        const SLOT_FREE = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
        const SIGNALED = 0x10;
        const SIG_PENDING = 0x20;
        const P_STOP = 0x40;
        const NO_PRIV = 0x80;
        const NO_ENDPOINT = 0x100;
        const VMINHIBIT = 0x200;
        const PAGEFAULT = 0x400;
        const VMREQUEST = 0x800;
        const VMREQTARGET = 0x1000;
        const PREEMPTED = 0x4000;
        const NO_QUANTUM = 0x8000;
        const BOOTINHIBIT = 0x10000;
    }
}
```

（逐字引用，`os/kernel/src/proc.rs:bitflags RtsFlagsBits`。）

位值与 C 逐个相同，包括"跳过 `0x2000`"这件事——`PREEMPTED` 仍然是 `0x4000`。`MiscFlagsBits` 同样是 18 个位、值一一对应，唯一的改名是 `MF_FPU_INITIALIZED` → `EXT_REG_INITIALIZED`（值仍为 `0x1000`）：常量行本身没有注释，名字即说明——三个目标架构上这个位表达的是"扩展寄存器状态已初始化"，不专指 FPU 设备。

字段本身是原子包装：

```rust
/// Runtime status flags (wraps atomic operations).
///
/// Uses `RtsFlagsBits` bitflags internally for type-safe flag values.
/// The atomic wrapper allows lock-free reads from interrupt context
/// (BKL may not be held during timer interrupts).
pub struct RtsFlags(AtomicU32);
```

（逐字引用，`os/kernel/src/proc.rs:struct RtsFlags` 与其文档注释。）

原子内层的选择由读者决定：判定"可运行"的调用点在中断上下文里（tick 处理要保持对 `proc_ptr` 的读），那里不能假装持有 BKL，所以读必须是无锁的。写入侧仍要求 BKL——`os/kernel/src/proc_table.rs` 的模块注释把这条纪律写在文件开头（"All access to `ProcessTable` must be performed while holding the Big Kernel Lock"）。

操作宏的对位是两个方法，语义与 §2.2.1 的三个宏一致：

```rust
    pub fn rts_set(&mut self, nr: ProcNr, flags: RtsFlagsBits) {
        let was_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
        if let Some(p) = self.get_mut(nr) {
            p.p_rts_flags.set(flags);
        }
        let is_runnable = self.get(nr).is_some_and(|p| p.is_runnable());
        if was_runnable && !is_runnable
            && self.is_in_scheduler(nr) {
                // C uses `get_cpu_var(rp->p_cpu, run_q_head)` — the process's
                // assigned CPU determines which per-CPU queue to dequeue from.
                let cpu_id = self.get(nr)
                    .map(|p| CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire)))
                    .unwrap_or(CpuId::BSP);
                self.sched_dequeue(nr, cpu_id);
            }
    }
```

（节选，`os/kernel/src/proc_table.rs:ProcessTable::rts_set`——`rts_unset` 是它的镜像：`was_runnable && !is_runnable` 换成 `!was_runnable && is_runnable`，`sched_dequeue` 换成 `sched_enqueue`。）

三段结构与 `RTS_SET` 相同：读旧值、改位、按跃迁决定队列动作。多出来的一处是 `is_in_scheduler` 判定，它对应 C 里"进程是否真的在队上"这件事——C 的 `dequeue` 对不在队上的进程会做一次链表遍历后无果而终，Rust 选择先问一句。

还有一处与 §2.8 的宏族直接对应：身份判定不再比较地址，而是读字段。

```rust
    /// Check if this is a kernel task (p_nr < 0).
    /// C: iskernelp(p) = ((p) < BEG_USER_ADDR) — but Rust uses p_nr field
    /// instead of address comparison (06-proc-init-boot-proc.md §3.3).
    pub fn is_kernel_task(&self) -> bool {
        self.p_nr.0 < 0
    }
```

（逐字引用，`os/kernel/src/proc.rs:KProcess::is_kernel_task` 与其文档注释。）

C 的 `iskernelp` 比较的是"槽指针是否落在 `BEG_USER_ADDR` 之前"——一个依赖数组布局的判据（§2.8 的宏表）。Rust 侧改成读 `p_nr` 的符号，把"内核 task"这件事从"数组里的位置"换成了"身份字段的性质"：结构体搬家、数组扩容、槽被单独传给别的函数，判定都不变。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 两套 `bitflags!` 类型 | 裸 `u32` + 位常量 | 需要 `#[allow]`/转换点把历史常量暴露给过渡期代码（`mod rts`/`mod mf` 里的 `u32` 常量） |
| `AtomicU32` 内层 | 普通 `u32` + 约定"读时持锁" | 写入仍是"改整个字"，读改写序列必须持 BKL |
| 身份判定读 `p_nr` | 比较槽指针 | `p_nr` 必须始终与槽位置一致（`proc_init` 对位与 fork 的 `p_nr` 重写都要保证） |

**把两套位合成同一个字（需 `u64`：RTS 最高位 17、MF 最高位 21）会怎样**：`p_rts_flags` 的一处写入会与 `p_misc_flags` 的写入竞争同一个字（SMP 下两个 CPU 各改一半时经典 read-modify-write 丢失），而 `p_rts_flags` 的读在中断上下文、`p_misc_flags` 的读在内核调用路径，两者的同步要求不同——合在一起只能按更严的那个来。分成两个字段、两个原子字之后，`rts_set` 与 `misc_flags` 的更新互不干扰。

### 3.4 boot image 的类型重建：`ProcKind` + `EntrySpec`（→§2.4、§2.6、§2.7）

C 的负担有两层。第一层是角色：`arch_boot_proc` 与其他调用点靠"`p_nr < 0`"、"`p_nr == VM_PROC_NR`"、"`isrootsysn`"三种临时判断区分成员类别（§2.6、§2.7）。第二层是入口状态：未装载的进程用"PC/SP/ps_strings 三个零值"表示，VM 装载失败时也只能返回零（§2.7 的 `panic` 路径之外没有任何区分手段）。Rust 侧把这两层都变成类型：

```rust
pub enum ProcKind {
    /// Kernel task (CLOCK / SYSTEM / IDLE / KERNEL): runs in kernel mode,
    /// has no ELF entry point.
    KernelTask,
    /// VM process: runs in user mode, ELF is loaded during boot.
    Vm,
    /// Root system service (RS): runs in user mode; its image is loaded by
    /// **VM** at runtime (C boot-side loads only VM into the bootstrap
    /// pagetable and VM then builds each other boot image's page table;
    /// not by RS itself).
    RootService,
    /// Other system service: runs in user mode; its image is loaded by **VM**
    /// at runtime (same VM-driven boot-image walk as `RootService`).
    UserService,
    /// User process: created by fork/exec, never appears during boot.
    UserProcess,
}
```

（逐字引用，`os/arch/src/arch/boot.rs:enum ProcKind`。）

五个变体里只有前四个出现在启动期。映射规则由内核层写在一个 `if` 链里，注释把三条对应关系逐条列出：

```rust
        // Process kind mapping (06-proc-init-boot-proc.md §3.4):
        //   VM_PROC_NR    → ProcKind::Vm
        //   RS_PROC_NR    → ProcKind::RootService
        //   other user    → ProcKind::UserService (RS will load ELF later)
        let proc_kind = if is_vm {
            ProcKind::Vm
        } else if is_root_sys {
            ProcKind::RootService
        } else {
            ProcKind::UserService
        };
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 中的映射段。）

入口状态是第二个类型：

```rust
/// Entry-point specification (OS concept — replaces the previous
/// `pc: VirBytes, sp: VirBytes, ps_strings: VirBytes` triple, which was
/// null-padded to zero when no ELF had been loaded).
///
/// `None` means "not yet known" (kernel task has no entry point; user
/// process not yet loaded by RS).
#[derive(Debug, Clone, Copy, Default)]
pub struct EntrySpec {
    /// Entry-point PC. `None` ⇔ not yet loaded.
    pub pc: Option<VirBytes>,
    /// Initial SP. `None` ⇔ not yet known.
    pub sp: Option<VirBytes>,
    /// ps_strings address (x86-64: rbx, aarch64: r0, riscv64: a0).
    /// `None` ⇔ no ps_strings (kernel task).
    pub ps_strings: Option<VirBytes>,
}

impl EntrySpec {
    /// Kernel task: no entry point.
    pub const KERNEL_TASK: Self = Self { pc: None, sp: None, ps_strings: None };

    /// Deferred user process: PC/SP not yet known, will be set by RS.
    pub const DEFERRED: Self = Self { pc: None, sp: None, ps_strings: None };

    /// ELF already loaded — entry + stack + ps_strings all known.
    pub const fn loaded(pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        Self { pc: Some(pc), sp: Some(sp), ps_strings: Some(ps_strings) }
    }
}
```

（逐字引用，`os/arch/src/arch/boot.rs:struct EntrySpec` 与其 `impl` 的前三个构造点。）

`KERNEL_TASK` 与 `DEFERRED` 的**值完全相同**（三个 `None`），区分它们的是调用者的意图：前者进 `build_cpu_context` 表示"这个角色没有入口"，后者表示"入口还不知道，等 RS"。两个常量并存的意义在于"以后要不要填"这件事留在类型里——如果只有一个 `NONE` 常量，arch 层无法区分"永远不会有入口"与"暂时没有"，第一次调度时该不该报错也就无从判断。

内核侧的清单常量与 §1.4.1 的三套编号对应：

```rust
pub const KERNEL_TASKS: &[(&str, ProcNr); NR_TASKS] = &[
    ("asyncm", ProcNr(-5)),  // ASYNCM — async message completion notifications
    ("idle",   ProcNr(-4)),  // IDLE — runs when no other process can
    ("clock",  ProcNr(-3)),  // CLOCK — alarms and clock functions
    ("system", ProcNr(-2)),  // SYSTEM — system functionality requests
    ("kernel", ProcNr(-1)),  // KERNEL/HARDWARE — pseudo-process for IPC/scheduling
];
```

（逐字引用，`os/kernel/src/proc.rs:const KERNEL_TASKS`。）

用户模块那一侧是 `BOOT_MODULE_PROC_NRS`（12 项，内容见 §1.4.1 的表），它的类型 `&[ProcNr; NR_BOOT_MODULES]` 把长度钉在共享常量上：引导器的 `MODULE_NAMES` 长度断言与它共用同一个 `NR_BOOT_MODULES`，任何一侧改了数量都会在两侧编译报错，只有顺序是手工维持的约定。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `ProcKind` 五变体 | `is_kernel: bool` + 两个 `is_vm`/`is_root_sys` 布尔 | 新增角色要在枚举与 arch 层各加一处分支 |
| `EntrySpec` 三个 `Option` | 三元素零值约定 | 每个消费者要处理 `None`；`KERNEL_TASK`/`DEFERRED` 同值需靠调用点区分 |
| 模块号常量数组 + 类型定长 | 运行期按名字查表 | 顺序仍要人工对齐（类型管不住顺序） |

**若不引入 `ProcKind` 而传三个布尔（`is_kernel`/`is_vm`/`is_root_sys`）会怎样**：八个组合里"`is_kernel` 与 `is_vm` 同时为真"这类非法组合在类型上合法，arch 层只能靠 `assert` 兜底；枚举把非法组合从可表达集合里删掉。（`EntrySpec` 换回三个裸值则与 §3.7 的返回类型问题同型：`KERNEL_TASK` 与"装载失败"无法区分。）

### 3.5 CPU 上下文：三个 C 函数收成一个 trait 与一个自由函数（→§2.7、§2.1.4）

C 的负担是同一件事被拆在三个函数里：`arch_proc_reset`（清上下文，§2.5 的清表循环调用它）、`arch_proc_init`（把入口三要素写进寄存器，§2.7 的 VM 装载末尾调用它）、`arch_boot_proc`（先判角色、再装 ELF）。其中 ELF 装载那一半的 i386 与 earm 两份实现逐段相同——`libexec_load_elf` 的调用参数、`ps_strings` 的三字布局、回收模块内存的两行，只有 `struct ps_strings` 的字段类型因 32 位而异。Rust 侧按"是不是真差异"切线：上下文部分收进 trait，装载部分降为自由函数。

```rust
//! # Why a single trait (instead of the previous 3)
//!
//! The C side has three functions — `arch_proc_reset`, `arch_proc_init`,
//! `arch_boot_proc` — but those are implementation detail, not OS
//! concepts. The Rust side collapses the three into one
//! `build_cpu_context` (covers reset + init) and one free function
//! `load_vm_elf` (covers the ELF-loading half of `arch_boot_proc`).
//! See `06-proc-init-boot-proc.md` §3.5 for the full rationale.
```

（逐字引用，`os/arch/src/arch/boot.rs` 模块文档的"三函数收拢"段。）

自由函数的理由写在它的文档里：

```rust
/// This is a **free function** rather than a trait method because the
/// three architecture implementations are byte-for-byte identical
/// (verified against … 的旧三份实现 prior to refactor — see
/// `06-proc-init-boot-proc.md` §3.5). Putting it in a trait would be
/// false polymorphism.
```

（节选：第二段括号注里那处路径被换成了省略号——注释原文指向的三个 `proc_arch.rs` 已在这次重构中删除，仓内只剩 `os/arch/src/arch/mod.rs` 的一行记录说明它们的职责被 `boot.rs` 接走；其余各行按原文。`os/arch/src/arch/boot.rs:fn load_vm_elf` 的文档注释。）

判据是"三份实现逐字相同"：差异不存在时把它放进 trait 只会制造一个假接口——每加一个架构都要抄一份同样的代码，任何一份改动都不是接口变化而是漂移。trait 保留给真有差异的部分：初值状态字、段选择子、FPU 策略、用户态 I/O 的编码方式。

trait 的名字也按同一条判据改过：

```rust
// ── CurrentCpuContextArch / CpuContext / TrapFrame type aliases ──
//
// Replaces the old `CurrentBootProcArch` (see 06-proc-init-boot-proc.md §3.5
// for the renaming rationale — the abstraction is "process's CPU
// context", not just the boot phase).
```

（逐字引用，`os/arch/src/lib.rs` 的类型别名段。）

内核层的两个入口方法对应 C 的两半：

```rust
    /// Set the architecture-private CPU context.
    ///
    /// The kernel layer never inspects `cpu_context`'s fields; it just
    /// stores the value and hands it to
    /// `CurrentCpuContextArch::apply_to_trap_frame` at first dispatch.
    /// Caller builds the value via
    /// `CurrentCpuContextArch::build_cpu_context(kind, proc_nr, entry)`.
    ///
    /// Replaces the previous `set_boot_initial_reg_state` +
    /// `set_boot_pc_sp` pair (06-proc-init-boot-proc.md §3.5).
    ///
    /// ...：与 C 的 `arch_proc_reset`（arch_system.c:181，
    /// boot 与 exec 两条路都经它）对位——装完整新上下文的同时必须种
    /// `KTS_FULLCONTEXT`，否则首次调度在返回门撞上
    /// `panic("no entry trap style known")`（arch_system.c:597-598）。
    pub fn set_boot_cpu_context(&mut self, cpu_context: CurrentCpuContext) {
        self.cpu_context = cpu_context;
        self.trap_style = minix_arch::TrapStyle::FullContext;
    }
```

（节选（注释有一处删节）：`os/kernel/src/proc.rs:KProcess::set_boot_cpu_context` 的文档注释与函数体——块内 `...` 处省略注释原文里的一处仓库内部编号前缀；`arch_system.c:181` 与 `arch_system.c:597-598` 是注释原文里的坐标，按原文引用。）

第二条入口是那个把硬件概念挡在架构层内的例子：

```rust
    /// This sinks the x86-64 hardware concept (RFLAGS.IOPL) entirely
    /// into the arch layer — the kernel layer only knows about the
    /// OS concept "enable user I/O". Other architectures are no-ops.
    /// Replaces the previous `target.initial_status |= X86_64_IOPL_BITS`
    /// direct write in `syscall_device.rs` (06-proc-init-boot-proc.md §3.5).
    pub fn enable_user_io(&mut self) {
        <CurrentCpuContextArch as CpuContextArch>::enable_user_io(&mut self.cpu_context);
    }
```

（逐字引用，`os/kernel/src/proc.rs:KProcess::enable_user_io` 与其文档注释。）

C 里 `enable_iop` 做的是 `pp->p_reg.psw |= 0x3000`（`0x3000` 是 RFLAGS 的 IOPL 两位），这是一条硬件细节，却写在通用调用路径上。Rust 把它折进 `CpuContextArch::enable_user_io`：内核层说"允许用户态 I/O"，x86-64 实现写 `psw |= 0x3000`，aarch64 与 riscv64 是空实现。三个架构的差异被限制在一个方法体里。

还有一条与 `const fn` 初始化相关的约束：上下文类型必须 `Copy + Default`，否则 §3.1 的 `[KProcess; PROC_TABLE_SIZE]` 预填不成立。构造期两次调用 `build_cpu_context` 的形状是这样：

```rust
        let cpu_context = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
            proc_kind,
            nr,
            entry,
        );
        proc.set_boot_cpu_context(cpu_context);
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 的用户模块循环段。）

三个参数就是"角色、进程号、入口"——没有裸地址、没有位定义。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 单一 `CpuContextArch` + 一个自由函数 | 三 trait 分层（reset/init/boot 各一个） | 新增角色时枚举与实现要同步；`load_vm_elf` 的参数表较长 |
| `CpuContext` 为不透明类型 | 内核层可见 `initial_pc`/`initial_sp`/`initial_status`/`p_ext_reg_state` 四字段 | 内核层想看某平台的上下文内容要经 trait 方法暴露 |
| `build_cpu_context(kind, nr, entry)` | 传裸寄存器值 | trait 参数里必须能表达"角色"（故有 §3.4 的枚举） |

**把 `CpuContext` 的字段暴露给内核层会怎样**：内核就要认识 PSW/PSR/sstatus 三种状态字的位定义，03 篇刚划定"抽象语义、不描述机制"的那条边界立刻被穿透。（`load_vm_elf` 做成 trait 方法也不成立：三份相同代码会让"真正该改的那一处"难以辨认，漏改的一份只在对应架构的启动日志里显形。）

### 3.6 FPU 策略：不翻译 32 位的保存模型（→§2.1.4）

C 的负担是 32 位架构下的 FPU 处理：架构级池 `fpu_state[NR_PROCS]` 加每进程一个 `p_seg.fpu_state` 指针，取值条件还带着"只给 `p_nr >= 0` 的进程分配"这一条（§2.1.4 已引）。Rust 侧的三个目标架构都用懒初始化：x86-64 走 XSAVE 家族、aarch64 用 `CPACR_EL1.FPEN`、riscv64 用 `sstatus.FS`。策略因此不是一个可以照抄的字段，而是一个"架构自己知道、内核层不读"的字段。

```rust
//! # FPU handling
//!
//! Modern 64-bit architectures use lazy FPU initialisation
//! (XSAVE / CPACR_EL1.FPEN / sstatus.FS); they do NOT use the legacy
//! `fnsave`/`fxrstor` model that Minix3 32-bit code translates through.
//! FPU policy is a `CpuContext` field that the kernel layer never reads.
```

（逐字引用，`os/arch/src/arch/boot.rs` 模块文档的 FPU 小节。）

三个架构的策略字段形状不同，各自表达"FP 指令第一次执行时会不会陷入"：

| 架构 | 策略字段 | 取值含义 |
|---|---|---|
| x86-64 | `fpu_policy`（`X86FpuInitPolicy` 枚举） | `KernelTask` 与 `LazyUserInit` 两种初值；后者让首个 FP 指令触发 `#NM` 后由内核建立状态 |
| aarch64 | `fpu_enable_el0: bool` | 已是继承用的策略标记，不再驱动硬件寄存器：per-dispatch 的 `CPACR_EL1.FPEN` 写已移除，CPACR 由内核 `finish_and_restore` 的 FPU 归属逻辑写（owner==picked 则使能，否则关闭）；内核 task 为 `false` |
| riscv64 | `sstatus`（含 `FS` 字段） | 懒陷阱由 `FS = Off` 门控：非持有者置 `Off`，首个 FP 指令因 `Off` 陷入；`Initial`/`Clean`/`Dirty` 都不陷入（`INIT_USER_SSTATUS = 0x20`、`INIT_TASK_SSTATUS = 0x100` 的 `FS` 域都是 `Off`） |

（逐字引用其文档注释：aarch64 的 `fpu_enable_el0` 字段注释写明"Kernel tasks have this `false`; user processes have this `true`. Arch-internal: the kernel layer never reads this"，另一处 `apply_to_trap_frame` 的注释写明"`fpu_enable_el0` 标志保留（fork 继承语义），不再驱动硬件寄存器"；riscv64 的 `INIT_USER_SSTATUS` 注释按其原文指向本篇 §3.4——那里的 `ProcKind` 把内核 task 与用户进程分成两个角色，两个初值常量正是这两个角色的取值，命名区分的是这两个变体；模块文档则写明"**only `Off(0b00)` traps**；`Initial`/`Clean`/`Dirty` 都不陷入"，并注明早先版本误以为 `Initial` 会陷入、使轮转分支成为不可达死代码。）

内存形态也同步改了：C 是"指针 + 架构级池"，Rust 是"每槽内嵌的定长缓冲"：

```rust
    /// FPU / extended-register state buffer.
    ///
    /// C: `p_seg.fpu_state` — a `char *` pointer (i386 archtypes.h:35) into
    /// the arch-level pool `fpu_state[NR_PROCS][FPU_XFP_SIZE]`
    /// (arch_system.c:144, NULL for kernel tasks). Rust embeds the buffer
    /// per-process instead of pointer + pool.
    ///
    /// ...
    /// - mock (test):     0 bytes (ZST — no actual state saved)
    /// - x86-64 (FXSAVE): 512 bytes
    /// - aarch64 (FPSIMD): 528 bytes
    /// - riscv64 (F/D):   264 bytes
    pub fpu_state: CurrentFpuState,
```

（节选，`os/kernel/src/proc.rs:KProcess::fpu_state` 的文档注释——块内两处 `...` 分别省略该注释中段（保存/恢复时机的说明）与末段；四条尺寸行按原文。）

内嵌的代价是每个槽都占这块空间（内核 task 的槽也占），换来的是没有池、没有指针、没有"指针为空时的特判"。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 三架构各自的懒初始化策略字段 | 照抄 `fnsave`/`fxrstor` 路径 | 策略语义分散在三份架构代码里，逐寄存器细节要看各架构文档 |
| 每槽内嵌 FPU 缓冲 | 架构级池 + 指针 | 表体积变大；测试构建用零尺寸类型（ZST）抵消 |

**照抄 `fnsave`/`fxrstor` 会怎样**：这两个指令在 x86-64 上仍可执行，但 `fnsave` 只保存 x87 状态（108 字节格式），SSE/AVX 寄存器不在其中；两个进程切换后向量寄存器互相污染，表现是"同一个用户进程的浮点结果在别的进程运行过之后变化"。

本节涉及的三架构懒陷阱门控的逐寄存器时序属 FPU 专题文档（尚未重生成）；真机上"首次用户 FP 指令前的门控建立"这一步在本篇按策略字段的归属陈述，未做真机验证 `[待验证]`。

### 3.7 VM ELF 装载的错误模型（→§2.7）

C 的负担是一句 `if(libexec_load_elf(&execi) != OK) panic("VM loading failed");`（§2.7 已引）：全部失败原因合成一个 `panic`，而在此之前"装载器内部失败"与"没有装载"之间的区分只存在于函数返回值里，一旦返回值被丢成零 PC 就再也看不出来。Rust 侧把失败分类、把"要不要停机"的决定交给调用者。

```rust
/// Errors from `load_vm_elf`.
///
/// Replaces the previous "return zero PC and let the caller figure it
/// out" silent failure (see `06-proc-init-boot-proc.md` §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLoadError {
    /// ELF header / magic invalid, or no PT_LOAD segments.
    InvalidElf,
    /// `VmBootAllocator` ran out of frames for the VM image.
    OutOfMemory,
    /// Pages could not be mapped into the bootstrap page table.
    MappingFailed,
    /// The module blob physically lands inside its own segment VA pages
    /// (image PA ∈ segment VA range). Boot-identity eviction would
    /// destroy not-yet-copied bytes; a real UEFI layout never produces
    /// this overlap, so it is rejected explicitly rather than read
    /// through a stale mapping (NK4-A first-light, 2026-09-21).
    ImageInsideSegments,
}
```

（逐字引用，`os/arch/src/arch/boot.rs:enum VmLoadError`——末一变体的注释里含仓库内部阶段编号与日期，按原文保留。）

四类失败对应四个不同的现场：格式不对（模块文件不是 ELF 或没有可装载段）、内存不够（bootstrap 分配器取不到帧）、映射失败（页表装不下去）、以及一种布局拒绝——镜像的物理区间落在它自己要映射的虚拟段范围内，此时驱逐恒等映射会把还没复制完的镜像字节抹掉，函数宁可返回错误也不读陈旧映射。最后一类在真实的 UEFI 布局里不会出现，属于防御性分支。

调用者只写一次"必须成功"：

```rust
                let vm_result = load_vm_elf(
                    module,
                    kernel_info,
                    &mut paging,
                    &mut vm_alloc,
                    &access,
                )
                .unwrap_or_else(|e| {
                    panic!("load_vm_elf: VM ELF is required at boot: {e:?}")
                });
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 的生产分支。）

`panic` 文案把 `e` 打出来，所以现场里既有"必须成功"这条策略，也有具体是哪一类失败。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 装载器返回 `Result` | 装载器内部 `panic` | 调用者要处理错误（这里选择 panic，但决定在调用点） |
| 四类错误分开 | 单个 `Err(())` | 枚举要随新失败模式增长 |

**若保持"返回零 PC"会怎样**：`EntrySpec::loaded(0, sp, ps)` 与 `KERNEL_TASK` 的差别只剩 `sp`/`ps` 两个 `Some`；VM 第一次被调度时从地址 0 取指，故障以取指异常出现，诊断时看不到任何与 ELF 有关的现场。

### 3.8 零堆启动的工程实现（→§2.0）

C 的负担是 §2.0 的第二层：启动窗口内没有分配器，所有全局结构必须编译期定容。Rust 侧把这条约束变成三件工程事实：全局表用 `SyncUnsafeCell`、初始化用 `const fn`、临时集合用固定数组。

```rust
#[repr(transparent)]
pub(crate) struct SyncUnsafeCell<T: ?Sized> {
    value: core::cell::UnsafeCell<T>,
}
```

（逐字引用，`os/kernel/src/globals.rs` 里 `SyncUnsafeCell` 的结构体定义；同文件 `os/kernel/src/globals.rs:static PROC_TABLE` 是它的使用点之一——在它之前还有 `FREE_MEMMAP`、`KERNEL_INFO`、`RESERVED_REGION_STORE`、`MEMMAP_REGION_STORE`、`BOOT_MODULE_STORE`、`BOOT_MODULE_NAME_STORE` 六个同类型的 `static`。）

它的文档注释把三条理由写给读者：`UnsafeCell` 的 `!Sync` 一并被消掉、从而能放进 `static`；内层可变性仍要求调用者履行安全契约；`get()` 依旧 `unsafe`。也就是说这个包装类型只解决"能不能放在 `static` 里"，不解决并发——并发由 BKL（或启动期的单线程）承担，每一处解引用都要自己举证。

能放进 `static` 的类型还受一层白名单约束：`SyncUnsafeCell<T>` 的 `Sync` 只在 `T: BklProtected` 时成立，而 `BklProtected` 是一个 sealed 标记 trait，实现只在一个宏调用里列出（`os/kernel/src/globals.rs` 的 `bkl_protected_impls!`，两处调用，其中一处带 `#[cfg(target_arch = "x86_64")]`）——`RefCell`/`Rc`/`Cell` 这类类型因此无法借 `SyncUnsafeCell` 混进 `static`，给新的全局类型开洞必须显式改那处清单。

`const fn` 初始化的限制也在这一层暴露。`KProcess::new_zeroed` 的注释写明其中一个字段为什么用裸位值：

```rust
    /// overwrites them per-slot. `p_rts_flags` is set to `SLOT_FREE` (0x01)
    /// using the raw bit value because `RtsFlagsBits::SLOT_FREE.bits()` is
    /// not `const` (bitflags limitation).
    ///
    /// See `06-proc-init-boot-proc.md` §3.8 (zero-heap `static mut` storage).
    pub const fn new_zeroed() -> Self {
```

（节选，`os/kernel/src/proc.rs:KProcess::new_zeroed` 的文档注释；`proc_table.rs` 里另有一个文件级常量 `PROC_STOP_BITS: u32 = 0x02`，注释同样说明它是"为了 `const fn` 初始化"。）

启动期需要临时集合的地方也走固定数组。VM 引导区选择要排除内核与所有 boot 模块占用的物理区间，这个排除集在零堆下是这样表达的：

```rust
                let mut exclusions = [MemoryRegion {
                    base: PhysBytes(0),
                    len: 0,
                }; crate::proc::NR_BOOT_MODULES + 1];
                let mut n_excl = 0usize;
                push_exclusion(
                    &mut exclusions,
                    &mut n_excl,
                    kernel_info.kern_phys_base(),
                    kernel_info.kern_size() as usize,
                );
                for m in kernel_info.boot_modules() {
                    push_exclusion(&mut exclusions, &mut n_excl, m.start, m.len);
                }
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 生产分支的排除集段。）

数组长度是 `NR_BOOT_MODULES + 1`：1 个内核镜像区间加 12 个模块区间。`push_exclusion` 在越界时 `assert`——不是"丢弃多的条目"，因为排除集少一项会让 VM 的段帧与某个已占用区间重叠（§4.7 会讲后果）。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `SyncUnsafeCell<T>` + `const fn` 初值 | `static mut`（Rust 2024 下不允许安全引用）、`OnceLock` | 每处 `get()` 需自行举证并发前提 |
| 固定数组表达临时集合 | `Vec`（需要分配器） | 容量上限必须能从常量推导（这里是 `NR_BOOT_MODULES + 1`） |
| 裸位值出现在 `const fn` 里 | 统一的 `bitflags` 取值 | 两处裸值常量需与 `bitflags` 定义保持同值（有单测覆盖） |

**把排除集换成 `Vec` 会怎样**：`init_proc_and_boot` 会在 `kernel_may_alloc` 窗口内调用分配器，而窗口的语义是"内核可以直接使用空闲物理内存"、不是"堆可用"——第一句分配就会撞上不存在的基础设施。（惰性初始化形态同样不成立：`proc_table_boot_unchecked` 一类 boot 访问器以"BKL 尚未建立、单线程"为调用前提，`OnceLock` 把"表何时存在"推到运行期、要面对两个 CPU 同时首次访问。）

### 3.9 四个抑制位的表达与解除责任（→§2.5、§2.6）

C 的负担是四位分散在四处置位、又由四类不同角色解除（§1.4 的关节表）：boot 循环置两位、`do_vmctl` 解两位、`do_privctl` 解一位、`bsp_finish_booting` 解一位，每个位"谁有权解除"这件事只体现在调用点的位置与权限检查里。Rust 侧保持位的语义与值不变，把"置位时机"与"解除责任"留在同样的调用点上，并用 `rts_unset` 的自动入队把"解除"与"进队列"绑在一次动作里。

置位侧与 C 的对应（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot`）：

```rust
        } else {
            // Don't let the process run for now.
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }
```

```rust
        // VM inhibit: all user processes except VM must wait for VM to
        // create their page tables.
        // C: main.c:267-270
        if nr != proc_nr::VM_PROC_NR {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot`。）

解除侧在启动末刻：

```rust
    // Step 4: Unset RTS_PROC_STOP on boot processes
    // C: for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++)
    //       RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
    // Rust: ProcessTable::rts_unset auto-enqueues a newly-runnable process
    // (see `ProcessTable::rts_unset` in proc_table.rs). Iterate from 0 (first user boot module) up to
    // but excluding kernel tasks (which were marked PROC_STOP during
    // init_proc_and_boot and must stay stopped — they're invoked lazily).
    // C also skips kernel tasks (the loop is `for i in 0..NR_BOOT_PROCS-NR_TASKS`).
    for nr in 0..(ProcNr(crate::proc::NR_BOOT_PROCS as i32)
        - ProcNr(crate::proc_table::NR_TASKS as i32)).0
    {
        proc_table.rts_unset(ProcNr(nr), RtsFlagsBits::PROC_STOP);
    }
```

（逐字引用，`os/kernel/src/lib.rs:bsp_finish_booting` 的解除循环。）

上界与 C 相同（`NR_BOOT_PROCS - NR_TASKS = 12`），迭代的是进程号 0..11。注释把"内核 task 保持停止、被惰性调用"这条语义写在了代码旁边，这与 §1.4 的边界推演是同一条。

四位各自的状态在 Rust 侧汇总如下（"解除者"一列与 §1.4 的表一致，实现位置是 Rust 的）：

| 位 | 置位点（Rust） | 解除点（Rust） | 解除后 |
|---|---|---|---|
| `NO_PRIV` | `init_proc_and_boot` 的 else 分支 | 权限路径清位（`priv_id` 关联成功时不再置位；运行期由 RS 的调用清） | `rts_unset` 命中跃迁则入队 |
| `VMINHIBIT` | 同上的双闸 | 运行期 VM 的 `VMCTL` 路径 | 同上 |
| `BOOTINHIBIT` | 同上的双闸 | 运行期 VM 的 `VMCTL` 路径 | 同上 |
| `PROC_STOP` | `init_proc_and_boot` 每个成员各置一次 | `bsp_finish_booting` 的解除循环 | 入队（本节的循环即首次入队） |

`RTS_NO_ENDPOINT` 不在本篇的启动路径（四道抑制位）之内——它的置位与回收走进程退出路径（`clear_endpoint` 与 `do_clear` 的对位见 §2.2.2），运行期语义在 `12-ipc-core` 与进程生命周期篇里读。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 位的语义与值保持 C 原样 | 把 `PROC_STOP` 改成一个"boot 阶段"全局开关 | 每 CPU 的 idle 槽仍需要自己的 `PROC_STOP`；全局开关无法表达 per-进程状态 |
| `rts_unset` 自带入队 | 标志操作与队列操作分开 API | 拆分会让"清位但忘了入队"成为可写出的 bug（本节末的反推给出后果） |
| 解除循环迭代进程号 | 迭代"清单下标再换算进程号" | 两者在 boot 段恰好重合，但语义上 C 迭代的是进程号；沿用同形写法让边界推演可以逐字对照 |

**把 `rts_unset` 的自动入队拆掉会怎样**：`bsp_finish_booting` 清掉 12 个进程的 `PROC_STOP` 之后队列一个进程都没有，`pick_proc` 恒空、CPU 停在 idle 直到下一次 tick——而 tick 处理本身又要选进程；运行期的同款缺失表现为"被重新调度的进程再也收不到时间片"。

### 3.10 `KPriv` 8 子结构（→§2.3）

C 的负担是 31 个字段平铺在一个结构体里，唯一的组织是字段顺序与注释分组（§2.3 的表）。启动期只写其中 4 个（清表）或 6 个（模板），运行期由 `do_privctl` 改另外一批，两种读写时机混在同一个类型里。Rust 按"读写时机"切成 8 个子结构：

```rust
// ── KPriv 8 substructures (06-proc-init-boot-proc.md §3.10) ─────────────────────
//
// KPriv is split into 8 substructures by responsibility: PrivIdentity
// (s_proc_nr, s_id), PrivFlags (s_flags), PrivInit (s_init_flags),
// PrivSignals, PrivIpc, PrivIo, PrivMem, PrivRuntime. Each is its own
// `Default`/`const fn new()`-constructible type so the kernel layer can
// pre-build `PrivTable` via `[KPriv; NR_SYS_PROCS]` instead of heap-allocating.
//
// Field name C-prefix `s_` is preserved for traceability to Minix3's `struct priv`.
```

（逐字引用，`os/kernel/src/kpriv.rs` 的 8 子结构段头注释。）

C 的 31 个字段逐个落到：

| 子结构 | 字段 | C 对应 |
|---|---|---|
| `PrivIdentity` | `s_proc_nr`、`s_id` | 身份两字段 |
| `PrivFlags` | `s_flags` | 角色旗标（含 4 个角色位与 `CHECK_*` 位） |
| `PrivInit` | `s_init_flags` | 初始化旗标 |
| `PrivSignals` | `s_sig_mgr`、`s_bak_sig_mgr`、`s_notify_pending`、`s_asyn_pending`、`s_int_pending`、`s_sig_pending`、`s_asyntab`、`s_asynsize`、`s_asynendpoint` | 信号簿记 6 项 + 异步发送表挂载点 3 项 |
| `PrivIpc` | `s_trap_mask`、`s_ipc_to`、`s_k_call_mask` | 三层掩码 |
| `PrivIo` | `s_nr_io_range`、`s_io_tab`、`s_nr_irq`、`s_irq_tab` | I/O 与 IRQ 白名单 4 项 |
| `PrivMem` | `s_nr_mem_range`、`s_mem_tab`、`s_ipcf`、`s_stack_guard`、`s_diag_sig` | 内存白名单与过滤 5 项 |
| `PrivRuntime` | `s_alarm_timer`、`s_grant_*` 三项、`s_state_*` 两项 | 运行时挂载点 6 项 |

分组与 §2.3 的七组不是一一对应（下文的"若按 §2.3 的七组原样切"指 §2.3 那七组；Rust 侧另按 wire 边界重构）：C 的"异步发送表挂载点"三字段并进了 `PrivSignals`（它们只在异步投递路径上与信号簿记一起被读），"身份与角色"拆成 4 个子结构里的三个（把"绑定"与"能力"分开，见下）。

拆分的收益在 `const fn` 初始化上可见：

```rust
    /// Const-constructible zeroed KPriv with `s_id = id` (for `const fn`
    /// `PrivTable::new()` / `static mut` init).
    ///
    /// See `06-proc-init-boot-proc.md` §3.2 / §3.8.
    pub const fn new_zeroed(id: SysId) -> Self {
        Self {
            identity: PrivIdentity {
                s_proc_nr: None,
                s_id: id,
            },
```

（节选，`os/kernel/src/kpriv.rs:KPriv::new_zeroed`——块内在 `s_id` 赋值后省略其余 7 个子结构的初值。）

`PrivTable::new` 只碰 `identity.s_id`：

```rust
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_zeroed(0) }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].identity.s_id = i as SysId;
            i += 1;
        }
        Self { privs }
    }
```

（逐字引用，`os/kernel/src/kpriv.rs:PrivTable::new`。）

与 §2.5 的 C 循环对照：C 那边写 `s_proc_nr`、`s_id`、`ppriv_addr[i]`、两个信号管理器，Rust 这边 `s_proc_nr` 的初值由 `KPriv::new_zeroed` 给出、`ppriv_addr` 这张桥接表不存在（直接按索引取；C 侧那张表在 §2.5 的清表循环里被逐槽填写，声明是 `EXTERN struct priv *ppriv_addr[NR_SYS_PROCS]`，取址入口是 `priv_addr(i)` 宏，两者同在 `minix3/minix/kernel/priv.h`）、两个信号管理器由 `KPriv::new` 的默认值给出。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 8 子结构 | 31 字段平铺；§2.3 的七组分法 | 字段路径变长（`priv.flags.s_flags`）；用户态 wire 结构要按同一顺序维护 |
| 每子结构独立 `const fn` 构造 | 整体 `new_zeroed` 一个巨型字面量 | 子结构数量与 wire 顺序耦合（见下） |
| 保留 `s_` 前缀 | 去掉前缀改成 Rust 命名 | 前缀是"与 C 字段对应"的显式标记 |

**按 §2.3 的七组原样切会怎样**：`s_proc_nr`（分配一次）与 `s_flags`（运行期可改）落在同一组，`const fn` 预填无法只给身份赋初值；更要紧的是 wire 边界——内核与 RS 两份手工同序的结构（`os/servers/rs/src/privilege.rs`）随分组一起改，改漏一个字段就是 `data_copy` 的静默错位。

### 3.11 调度字段与统计字段（→§2.1.2、§2.1.5）

C 的负担是三件：优先级字段是 `char` 且 `sched_proc` 用 `-1` 当"保持当前"的哨兵；CPU 亲和掩码是 `bitchunk_t` 数组；统计字段按读者分成三块但没有类型边界（§2.1.2、§2.1.5）。Rust 逐个改型：

```rust
/// Priority newtype (wraps validity check).
///
/// Valid range: `TASK_Q(0)` to `MIN_USER_Q(15)`.
/// Design decision §3.3 (11-scheduling-primitives.md): internal type is `u8`.
/// The special value `-1` in Minix3's `sched_proc()` means "keep current priority"
/// and is NOT a valid `Priority` — it is a parameter sentinel expressed via
/// `Option<u8>` / `Option<Priority>` in `sched_proc` (§3.8), not encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);
```

（逐字引用，`os/kernel/src/proc.rs:struct Priority` 与其文档注释；注释里引用的 `11-scheduling-primitives.md` 是尚未重生成的篇目，其节号属该篇自己的编号。derive 行给出的是全序比较能力——优先级要能比大小。）

哨兵从"值域里的一个负数"变成"参数类型上的 `Option`"：`-1` 不再是合法优先级，值域用 `u8` 表达，构造器做范围检查。CPU 亲和掩码是一个单字位图：

```rust
/// CPU affinity bitmap (06-proc-init-boot-proc.md §3.11).
///
/// C: `p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]` — proc.h:37.
///
/// Bit `i` set ⇔ process may run on CPU `i`. Default = all bits set
/// (any CPU). The scheduler must check `allows(cpu)` before enqueuing
/// into a per-CPU ready queue.
///
/// Stored as `[u64; 1]` (32 bits used, 64-bit chunk for alignment).
/// `MAX_CPUS = 32` fits in a single `u64`.
pub struct CpuMask {
```

（节选，`os/kernel/src/proc.rs:struct CpuMask` 的文档注释——块内在 `pub struct CpuMask {` 后省略其字段声明：实际字段是 `bits: u64`，`MAX_CPUS = 32` 个位放在一个 64 位字里。）

"默认值"要分两层看：类型自己的 `Default` 派生给出的是 `bits: 0`（空掩码，不允许任何 CPU）；"全部允许"来自构造路径——`SchedFields::new()` 显式调用 `CpuMask::all()`（`os/kernel/src/proc.rs:SchedFields::new`，`all()` 把低 `MAX_CPUS` 位全置）。而 C 的 `p_cpu_mask` 初值是 BSS 零、且启动路径不写它——同一个字段在两边的初值语义并不相同（Rust 的槽初值经 `SchedFields::new` 是"全允许"，C 的零值在当前树里没有读者去解读）。这个差异不改变现行为，因为两边都没有调度器读它；差异要留给将来真正启用该白名单的改动来处理。

调度字段收进一个子结构：

```rust
pub struct SchedFields {
    pub priority: AtomicU8,
    pub quantum: Quantum,
    pub cpu: AtomicU32,
    /// CPU affinity bitmap (06-proc-init-boot-proc.md §3.11).
    /// Default = all CPUs allowed. Scheduler checks `allows(cpu)` before
    /// enqueuing into a per-CPU ready queue.
    pub cpu_mask: CpuMask,
    /// User-space scheduler process number.
    ...
}
```

（节选，`os/kernel/src/proc.rs:struct SchedFields`——块内 `...` 处省略 `p_scheduler` 字段及其两行注释。）

C 的 `p_priority`、`p_quantum_size_ms`、`p_cpu`、`p_cpu_mask`、`p_scheduler` 五项在这里合为一体；`p_cpu_time_left` 与时间片尺寸一起进了 `Quantum`（字段是 `cpu_time_left: AtomicU64` 与 `size_ms: AtomicU32`）。两个方法的对应关系要分开说：`Quantum::consume` 是 tick 路径上的递减，生产调用点在 `os/kernel/src/clock.rs:decrement_quantum_in_with_delta`，对位 C 的 `p_cpu_time_left -= tsc_delta` 与 `if (!p->p_cpu_time_left) proc_no_time(p)`（`minix3/minix/kernel/proc.c:switch_to_user`）；`Quantum::allocate` 没有生产调用点——`sched_proc` 里"改时间片就重算余量"那一步直接写 `cpu_time_left`（`os/kernel/src/sched.rs:sched_proc`），`allocate` 只在测试里用。`proc_no_time` 判的不是零：它检查 `!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE`，按角色决定是通知用户态调度器还是就地续一个时间片（`minix3/minix/kernel/proc.c:proc_no_time`）。

统计侧按"给谁看"分成三块：`Accounting`（六个量，交给用户态调度器：`enter_queue`/`time_in_queue`/`dequeues`/`ipc_sync`/`ipc_async`/`preempted`）、`TimeStats`（ticks 记账：`user_time`/`sys_time`/`virt_left`/`prof_left`）、`CyclesStats`（cycles 记账：`total`/`kcall`/`kipc`/`tick`）。分组与 C 的三个注释分组一致，区别是它们成了独立类型。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `Priority(u8)` + `Option` 哨兵 | `i8`/`char` + `-1` 哨兵 | 与 C 的 wire 边界要多一次转换（`do_schedctl` 的入参） |
| `CpuMask { bits: u64 }`，构造路径给"全允许" | 32 位块数组 + BSS 零 | 默认值语义与 C 不同（C 的零值在当前树里没有读者；启动期两边都不可观察） |
| 三个统计子结构 | 一个大的统计块 | 访问路径变长 |

**若 `Priority` 内部保留 `i8` 并允许负值会怎样**：`sched_proc(priority = -1)` 的"保持"语义与"优先级为负"在类型上不可区分（C 的 `priority < TASK_Q` 检查把 `-1` 放行，恰好正确但靠调用者守约）；`Option` 之后"没给就保持"成为分支形状，值域检查只剩一条。

### 3.12 boot→running 的转换形状（→§2.5、§2.6、§2.7）

C 的负担是这条链散在六个调用里：`proc_init()` → boot 循环 → `arch_post_init()` → `memory_init()` → `system_init()` → `bsp_finish_booting()`（§2.5–§2.7 与 `03-kmain-cstart.md` §2.1 已给）。Rust 收成两个函数加一个收尾：`init_proc_and_boot`（建表 + 填充 + 装载）、`init_post_and_memory`（页表根的确认）、`bsp_finish_booting`（解除 + 切用户）。其中 C 的 `arch_post_init` 与 `memory_init` 的**大部分内容被淘汰**，理由是一条架构事实：Direct Map 是常驻映射，不需要临时窗口。

```rust
/// 1. Assert VM's page-table root is valid (established in Phase C).
/// 2. Confirm VM is the kernel-level ptproc (the `set_current_ptproc_nr`
///    write happens in `switch_address_space`, and the CpuLocal slot is
///    installed in `init_smp_state` — this step records nothing itself) so
///    `dispatch_vmctl(SetAddrSpace)` Step 3 can decide whether to reload
///    the hardware root register (C: `setcr3()`, arch_do_vmctl.c:19-33; the
///    `p == ptproc` check is at :25).
/// 3. Assert the VM Direct Map base is configured.
///
/// The C behavior this replaces — `arch_post_init`'s `ptproc = VM` + `pg_info`
/// and `memory_init`'s freepdes allocation — is intentionally absent:
/// Direct Map is a permanent mapping, so no temporary-window allocation or
/// borrowed page-directory registration is needed (ARCH: Direct Map, see
/// 07-cross-space-init.md §3.5).
```

（逐字引用，`os/kernel/src/lib.rs:init_post_and_memory` 的文档注释。注释里 `arch_do_vmctl.c:19-33` 的坐标是原文，`07-cross-space-init.md` 是尚未重生成的篇目。注释第 2 条与函数体有偏离：函数体里只有三条断言——两条读 VM 槽的根字段、一条读 Direct Map 基址，`set_current_ptproc_nr` 的生产调用点在 `switch_address_space`（`os/kernel/src/lib.rs:fn switch_address_space`），这里做的是"根已就绪"的确认。）

前两条断言读的是 VM 槽的两个页表根字段：

```rust
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
```

（逐字引用，`os/kernel/src/lib.rs:init_post_and_memory`。）

这两个值不是 VM 自己填的，而是 `init_proc_and_boot` 在装载 ELF 时写下的——它与 C 的差别在于"C 的 `arch_post_init` 只需把 VM 记为 ptproc，不需要确认根地址，因为 `pg_info` 现取；Rust 侧根地址在装载阶段就落在槽里，这里做的是确认"。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `init_proc_and_boot` 一次做完建表+填充+装载 | 按 C 拆成三个函数（`proc_init`/boot 循环/`arch_boot_proc`） | 单函数较长（§4.7 逐步走查）；好处是"表从哪来、谁填的、谁装的"在一个函数里可读 |
| 保留 `init_post_and_memory` 作就绪确认 | 照抄 `arch_post_init` + `memory_init`（含 freepdes 分配） | 需要断言 VM 的根字段已被填（否则确认退化成空操作） |
| `bsp_finish_booting` 返回 `!` | C 的 `NOT_REACHABLE` 宏 | 调用点不能有后置代码 |

**删掉 `init_post_and_memory` 的断言会怎样**：一个 `phys_root == 0` 的槽会带着空根继续启动，直到第一次切地址空间时由 `switch_address_space` 把它写进页表根寄存器——此后每次运行都从一个空根翻译地址，现象是取指立即失败而不是切换前被拦住。断言是唯一能在切换发生前拦住它的地方——`phys_root` 为 0 在 boot 期不会立刻出错（`p_seg` 只是一块内存），错误要等到第一次切页表才显形。

### 3.13 fork 与 CPU 上下文的继承（→§2.1.4、§2.2）

C 的负担在 `do_fork` 的开头几行（§2.8 已引）：`*rpc = *rpp;` 把父进程的整个 `struct proc` 复制给子进程，随后逐个修补 `p_nr`、`p_endpoint`、`retreg`；i386 上还要先把父进程的 FPU 状态保存、把子进程原来的缓冲指针存下来、复制完再把指针恢复并按需 `memcpy` 状态。这种方法的前提是"结构体的每一半都适合复制"，而不是"每一半都应该继承"。Rust 侧把继承做成按语义逐项决策，其中最典型的一项是 FPU 策略：

```rust
        // See 06-proc-init-boot-proc.md §3.14.
        if parent.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED) {
            <CurrentCpuContextArch as CpuContextArch>::inherit_fpu_state(
                &mut child.cpu_context,
                &parent.cpu_context,
            );
            child.fpu_state = parent.fpu_state;
            child.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
        }
```

（逐字引用，`os/kernel/src/proc.rs` 的 fork 继承分支；块内源码注释里的 `§3.14` 指向本文档旧编号——fork 一节现为 §3.13，源码注释待同步，登记 `SRC-10`。）

条件读的是父进程的"扩展寄存器已初始化"位（C 的 `MF_FPU_INITIALIZED`）：父进程用过 FP，子进程才继承——继承分两层，条件内先调 `inherit_fpu_state` 拿策略，再把 `fpu_state` 缓冲整块复制并把子进程的 `EXT_REG_INITIALIZED` 位置上；否则子进程保持"未初始化"，第一次 FP 指令走懒陷阱。三个架构的实现注释把"为什么要继承"写成了"不继承会怎样"：

```rust
    /// x86-64: the child inherits the parent's `X86FpuInitPolicy` so that
    /// a forked user process keeps `LazyUserInit` (rather than silently
    /// dropping back to `KernelTask`). C: `memcpy(rpc->p_seg.fpu_state,
```

（逐字引用，`os/arch/src/x86_64/boot.rs:CpuContextArch::inherit_fpu_state` 的文档注释开头三行。）

aarch64 那一份的说法相同：子进程继承 `fpu_enable_el0`，否则"静默退回内核 task 的陷阱模式"；riscv64 那一份继承 `sstatus` 整个值，从而保住 `FS` 字段（`Off`/`Initial`/`Clean`/`Dirty` 四态）。三份 `inherit_fpu_state` 实现继承的都是**策略**；寄存器缓冲另由分支里的 `child.fpu_state = parent.fpu_state` 整块复制，二者共用一个条件（父进程的"已初始化"位）。也就是说被继承的是**策略 + 条件化的寄存器缓冲**——缓冲内容只在父进程用过 FP 时才有意义，与 C 只在 `proc_used_fpu` 下做 `memcpy` 是同一条判据。

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 逐项继承（策略 + 条件化的状态） | 整块复制后修补 | 每加一个上下文相关字段都要在 fork 路径上做一次决策（漏掉一项的后果可见：子进程缺少该字段的初值） |
| 继承条件是父进程的"已初始化"位 | 无条件继承 | 未用过 FP 的父进程的子进程保持懒初始化，与 C 的行为一致 |

**子进程不继承 FPU 策略会怎样**：用户进程 fork 出的子进程第一次执行 FP 指令时被判成"内核 task 不许用 FP"，走一段只有子进程到得了的异常路径；三个架构的注释点名的正是这一条。

除 FPU 之外，`fork_from` 还有一张逐字段的继承与修正清单——`p_rts_flags`（复制后 `NO_QUANTUM` 置位、`SIGNALED`/`SIG_PENDING`/`P_STOP` 清掉）、`p_misc_flags`（复制后清 `VIRT_TIMER`/`PROF_TIMER`/`SC_TRACE`/`SPROF_SEEN`/`STEP`）、`p_time`、`p_pending`、以及队列链接各自的处置——它写在 `os/kernel/src/proc.rs:KProcess::fork_from` 的文档注释里，本节只展开 FPU 一项；进程侧的 fork 全流程见 `04-stage-pm/07-pm-fork.md`。

### 3.14 C→Rust 去向表汇总（章末总表；表内 fork 行对应 §3.13）

| 对象 | C 形态 | Rust 形态 | 档位 | 依据节 |
|---|---|---|---|---|
| 进程表 | `EXTERN struct proc proc[]`（BSS） | `static PROC_TABLE: SyncUnsafeCell<ProcessTable>`，内部 `[KProcess; 261]` | Refactor | §3.1、§4.3 |
| 特权表 | `EXTERN struct priv priv[]` + `ppriv_addr[]` 桥接表 | `static PRIV_TABLE`，内部 `[KPriv; 64]`，无桥接表 | Refactor | §3.1、§3.10 |
| 槽定位 | `proc_addr(n)` 裸指针 + `isokprocn` 前置检查 | `nr_to_idx(nr) -> Option<usize>` | Refactor | §3.1 |
| 清表 | `proc_init` 运行期循环 | `const fn new()` 编译期初值 | Refactor | §3.1 |
| IPC 队列链接 | `struct proc *` | `Option<ProcNr>` 槽索引 | Rewrite | §3.1、§4.4 |
| 权限授予 | `get_priv` + 六字段赋值 + `fill_sendto_mask` | `CapabilityTemplate` + `grant_capability` + `fill_sendto_mask` | Refactor | §3.2 |
| 权限失败码 | `EINVAL`/`EBUSY`/`ENOSPC` 返回 int | `CapabilityError` 枚举 | Refactor | §3.2 |
| 状态位 | 16 个 `#define` + `u32` 字段 + 三个宏 | `RtsFlagsBits` + `RtsFlags(AtomicU32)` + `rts_set`/`rts_unset` | Rewrite | §3.3 |
| 杂项位 | 18 个 `#define`（含 `MF_FPU_INITIALIZED`） | `MiscFlagsBits`（`EXT_REG_INITIALIZED`） | Refactor | §3.3 |
| 内核 task 判定 | `iskernelp(p)` 比较槽指针 | `KProcess::is_kernel_task` 读 `p_nr` 符号 | Refactor | §3.3 |
| 角色区分 | `p_nr < 0` / `== VM_PROC_NR` / `isrootsysn` 临时判断 | `ProcKind` 枚举 | Refactor | §3.4 |
| 入口状态 | 三个零值约定 | `EntrySpec` 三个 `Option` | Refactor | §3.4 |
| 清单常量 | `table.c` 的 `image[]`（含名字） | `KERNEL_TASKS` + `BOOT_MODULE_PROC_NRS`（名字在模块与清单两处） | Rewrite | §3.4 |
| 上下文初始化 | `arch_proc_reset` + `arch_proc_init` | `CpuContextArch::build_cpu_context` | Refactor | §3.5 |
| ELF 装载 | 各架构各一份 `arch_boot_proc` 的装载半 | 共享自由函数 `load_vm_elf` | Refactor | §3.5、§3.7 |
| 用户态 I/O 使能 | `p_reg.psw \|= 0x3000` 写在通用路径 | `CpuContextArch::enable_user_io` | Refactor | §3.5 |
| FPU 保存模型 | 池 + 指针 + `fnsave` 路径 | 每槽内嵌缓冲 + 三架构懒初始化策略 | `[ARCH: K-4]` | §3.6 |
| 装载失败 | 内部 `panic("VM loading failed")` | `Result<_, VmLoadError>` + 调用点 `panic` | Refactor | §3.7 |
| 全局存储 | `EXTERN` 数组 | `SyncUnsafeCell` + `const fn` 初值 | Refactor | §3.8 |
| 高半区/DM 相关初始化 | `arch_post_init` + `memory_init` 的 freepdes | `init_post_and_memory` 的三条就绪断言（ptproc 记录实际在 `switch_address_space`） | `[ARCH: Direct Map]` | §3.12 |
| fork 上下文继承 | `*rpc = *rpp` 整块复制 + 定点修补 | 按语义重建 + `inherit_fpu_state` 策略继承 | Rewrite | §3.13 |
| 初始栈额度 | 64 KiB + VM 自扩 | 一次性 256 KiB 固定额度（无自扩） | `[ARCH: K-5]` | §4.2 |

档位沿用项目的三档术语：`[ARCH]` 表示外部可观察行为改变；`Rewrite` 与 `Refactor` 的划界是**逐项判断**（各行的理由在"依据节"所列的对应小节，不另立一条机器可判定的统一规则）。**本表是权威**：§4.9 的档位列以本表为准（重复行只从简引用）；§3.10 的选型表不承担档位标注。


---

## 4. 实现详解

本章按"先 arch 层机制、再 kernel 层对象、最后主流程串起"的顺序走一遍，每一节回指第 3 章的决策编号。

### 4.1 arch 层：`CpuContextArch` 的三架构实现与 per-arch FPU 策略字段（→3.5、3.6）

三个架构的 `build_cpu_context` 形状一致，差异集中在"按 `ProcKind` 选哪组初值"。x86-64：

```rust
    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let (psw, fpu_policy) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSW, X86FpuInitPolicy::KernelTask),
            _ => (INIT_PSW, X86FpuInitPolicy::LazyUserInit),
        };
        X86_64CpuContext {
            psw,
            cs: USER_CS_SELECTOR,
            ds: USER_DS_SELECTOR,
            ss: USER_DS_SELECTOR,
            es: USER_DS_SELECTOR,
            fs: USER_DS_SELECTOR,
            gs: USER_DS_SELECTOR,
            rip: entry.pc.map(|v| v.0).unwrap_or(0),
            rsp: entry.sp.map(|v| v.0).unwrap_or(0),
            rbx: entry.ps_strings.map(|v| v.0).unwrap_or(0),
            fpu_policy,
            gp_regs: [0; X86_64CpuContext::GP_REGS_LEN],
        }
    }
```

（逐字引用，`os/arch/src/x86_64/boot.rs` 里 `CpuContextArch for X86_64CpuContextArch` 的 `build_cpu_context`；接口本身的定义见 `os/arch/src/arch/boot.rs:trait CpuContextArch`。）

三个事实从这段里读出来。第一，角色只影响两个值：初值 PSW 与 FPU 策略；其余字段（六个段选择子、零寄存器）对所有角色相同。第二，`entry` 的三个 `Option` 在这里被 `unwrap_or(0)` 收成零——`EntrySpec` 的"未装载"语义在 arch 层落到"寄存器写 0"，而 `KERNEL_TASK` 与 `DEFERRED` 的区分由调用者在更早的地方使用。第三，`fpu_policy` 被存进上下文但 `apply_to_trap_frame` 不读它（那段代码里有一行 `let _ = ctx.fpu_policy;`，注释说明懒初始化的实际动作发生在第一次 `#NM` 陷入、由内核侧完成）。

aarch64 与 riscv64 的同名方法：

（逐字引用，`os/arch/src/arm64/boot.rs` 的同名方法。）

```rust
    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let (psr, fpu_enable_el0) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSR, false),
            _ => (INIT_PSR, true),
        };
```

```rust
    fn build_cpu_context(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext {
        let sstatus = match kind {
            ProcKind::KernelTask => INIT_TASK_SSTATUS,
            _ => INIT_USER_SSTATUS,
        };
```

（两条均为逐字引用，分别取自 `os/arch/src/arm64/boot.rs` 与 `os/arch/src/riscv64/boot.rs`。）

riscv64 的两个初值常量带一条命名注释，指向本篇 §3.4 的角色划分（`ProcKind` 把内核 task 与用户进程分成两类）："Named `INIT_USER_SSTATUS` per 06-proc-init-boot-proc.md §3.4 to distinguish the user-process variant from the kernel-task variant"（`os/arch/src/riscv64/boot.rs:const INIT_USER_SSTATUS` 的文档注释）。两条值各自只置一个位：内核 task 是 `SPP=1`（`sret` 回 S 态），用户进程是 `SPIE=1`（`sret` 后开中断）；两者的 `FS` 域都是 `Off`——懒陷阱的门控正落在这个域上，首个 FP 指令因 `Off` 陷入。

三个架构的差异面因此可以列成一张固定的表：

| 差异面 | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 内核 task 初值状态字 | `INIT_TASK_PSW` | `INIT_TASK_PSR` | `INIT_TASK_SSTATUS` |
| 用户进程初值状态字 | `INIT_PSW` | `INIT_PSR` | `INIT_USER_SSTATUS` |
| 段选择子 | 六个（CS/DS/SS/ES/FS/GS） | 无 | 无 |
| FPU 策略字段 | `fpu_policy`（枚举两值） | `fpu_enable_el0`（继承标记，不驱动 CPACR） | `sstatus` 内的 `FS` 域（门控用 `Off`） |
| ps_strings 承载寄存器 | `rbx` | `r0` | `a0` |
| `enable_user_io` | 写 `psw \|= 0x3000` | 空实现 | 空实现 |
| `inherit_fpu_state` | 继承策略枚举 | 继承 `fpu_enable_el0` | 继承 `sstatus` 整值 |

内核层看不到这张表里的任何一项：它只调用 `build_cpu_context(kind, nr, entry)`、`set_boot_cpu_context` 与 `enable_user_io`。表内三条 FPU 门控（x86-64 的 `#NM` 路径、aarch64 的 CPACR 归属、riscv64 的 `FS=Off` 门控）在本篇按源码与注释陈述，真机行为未核实 `[待验证]`。

**把初值 PSW 提到内核层会怎样**：三个架构的状态字位定义（x86-64 的 `IF`、aarch64 的 `DAIF`、riscv64 的 `SPP/SPIE`）会与内核层字段类型混在一起，或者内核层退化成"写一个魔数"——两种情况都破坏 03 篇划定的边界（内核层只认识 OS 概念）。

### 4.2 `load_vm_elf`：逐页装载与共享边界页（→3.5、3.7）

装载的主体是 `load_elf_into`（`load_vm_elf` 与"给非 VM 装载"的 `load_process_elf` 共用它，差别只在传给它的根页表是哪一个）。逐段 ELF 的页循环里，一页的处理是四步，注释把它们编号写在代码里：

```rust
            // ① Destination frame. A page that is ALREADY mapped with
            //    USER permission is a shared boundary page: linkers
            //    (lld's separate-code layout) start a later PT_LOAD
            //    inside the previous segment's last page. C parity: the
            //    C loader maps such a page once and both segments' bytes
            //    land in the same frame (libexec pg_map). Copy into the
            //    existing frame; only a fresh page gets a new frame +
            //    full zero + map. A mapped page WITHOUT USER is not a
            //    linker artifact at all — it is boot-identity residue
            //    on the adopted bootstrap root (see eviction below).
            let v = VirBytes(vaddr);
            if let Some((_pa, f)) = paging.query(v) {
                if !f.contains(crate::paging::PageFlags::USER_ACCESSIBLE) {
                    ...
```

（节选，`os/arch/src/arch/boot.rs:fn load_elf_into` 的页循环开头——块内在 `if !f.contains(...)` 的分支体开头处省略，该分支体在下方单独引用。）

"页已经存在"有两种来源，处理方式完全不同：

| 已存在页的性质 | 来源 | 处理 |
|---|---|---|
| 有 USER 权限 | 链接器把后一个 `PT_LOAD` 的起点放进前一个段的最后一页（共享边界页） | 复用该页的帧：拷文件字节；`.bss` 部分只清"属于本段的那一段"，不清页内属于前一段的字节 |
| 无 USER 权限 | 引导期恒等映射留下的监督者大页 | 驱逐：先拆大页到 4 KiB、开放中间层用户可见、解除映射，再走"新页"路径 |

驱逐那一段有条件保护：

```rust
                    if vaddr < img_hi && img_lo < vaddr + page_size {
                        // Image blob physically INSIDE a page we would
                        // unmap → its remaining bytes would fault
                        // mid-copy (or silently rebind through the DM
                        // under a different identity). Never produced by
                        // a real UEFI layout; refuse explicitly.
                        return Err(VmLoadError::ImageInsideSegments);
                    }
```

（逐字引用，`os/arch/src/arch/boot.rs:fn load_elf_into` 的驱逐保护段；其中 `img_lo`/`img_hi` 是模块镜像的物理区间上下界。）

这正是 §3.7 那个 `ImageInsideSegments` 变体的产生处。保护的动因可以从两行代码读出：本函数从 `module.start` 指向的物理内存直接读 ELF 字节（"模块在恒等映射下可被内核直读"，§2.7），驱逐会解除这段恒等映射；若模块自身的物理区间落在"要解除的那一页"里，剩余字节在复制中途消失。真机布局不产生这种重叠（该前提未在本篇核实 `[待验证]`），所以这是拒绝而不是兼容处理。

新页路径的四步在注释里编号为 ①a–①d：从 `VmBootAllocator` 取一个物理帧、把帧映射到内核可寻址地址后整帧清零（覆盖未对齐的首尾页与 `.bss` 尾）、再安装用户映射（VA → 分配的 PA，不做 `paddr == vaddr` 假设）。之后是 ② 拷文件字节与 ③ 共享页的 `.bss` 局部清零：

```rust
            // ② File-backed bytes for this page: the segment's file slice
            //    [page_off, page_off + file_bytes_here) lands at
            //    dst[seg_start_in_page..]. saturating_sub: bss-only pages
            //    (page_off ≥ filesz) simply have no file part.
```

（逐字引用，`os/arch/src/arch/boot.rs:fn load_elf_into` 的 ② 段注释。）

段起始落在页中间时的偏移由 `seg_start_in_page` 承载，它是"链接器产物"与"页对齐假设"之间的桥：C 的 `pg_map` 对 `vaddr % I386_PAGE_SIZE` 有断言（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_map`），Rust 的装载器显式接受段起点非页对齐，代价就是要处理共享页。注释把这条差异写明了（"this loader deliberately accepts mid-page segment starts and maps each page exactly once"）。

段与栈之间还有一步：**装载器尾的指令缓存同步**（源码在该处标注 `[ARCH: cache-sync]`）。页循环里对可执行段逐页记账"被写入的内核可寻址帧区间"，循环结束后一次性调用 `CurrentCacheSyncArch::sync_icache_range`（`os/arch/src/arch/boot.rs:fn load_elf_into` 的页循环记账与尾段调用；注释说明只记整帧、过度失效无害）。需要这一步的是 aarch64 与 riscv64——它们的 I-cache 与这些数据写入之间不保证一致，必须显式维护；x86_64 的 trait 实现是空操作。两条宿主测试把它钉成回归读数：`load_vm_elf_syncs_icache_over_exec_segment_frames`（可执行段恰好同步一次）与 `load_vm_elf_skips_icache_sync_for_non_exec_image`（非可执行段不同步）。

段装完之后是栈与 `ps_strings`：

```rust
    // Map the user stack: [stack_high - VM_STACK_SIZE, stack_high).
    // C kernel seeds 64 KiB and lets VM self-grow (reservedqueue.mappedin);
    // we lack the self-grow, so the eager runway IS the whole allowance.
```

（节选（注释有删节）：`os/arch/src/arch/boot.rs:fn load_elf_into` 的栈段注释开头；被删去的是原文末行括号里的一处仓库内部编号引用。）

C 的内核先给 64 KiB 栈、之后由 VM 自己按需扩张（`reservedqueue.mappedin` 机制）；本端还没有 VM 自扩栈，所以把初始栈放大到 256 KiB 作为整段额度（`VM_STACK_SIZE` 常量的注释给了实测数值与留量）。栈帧同样从 bootstrap 分配器取，且沿用"先选物理帧、再映射到虚拟地址"的方向（与 §4.2 前半的 ①a–①d 同向）。

`ps_strings` 的三字布局在 Rust 侧有一处与 C 不同的对齐：

```rust
    /// The three words below ps_strings — argc, argv, envp — each one
    /// full pointer word (SD-17 对齐轮 2026-10-06：旧值 `2*usize + i32`＝
    /// 20B，argc 槽只有 4B 且令出生 sp 落在 4-mod-8 非全字对齐上；exec 路径
    /// minix-sys/stack.rs 早已是 WORD 槽。上游只有 i386/ARM32 的
    /// protect.c（int 与指针同宽故两形重合），64 位无 C 真源可抄——统一
    /// 取 LP64 全字，与 exec 路径同形，出生 sp 回到 8 字节对齐。消费端
    /// startup-minix.c 只读 ps_strings 描述符（字段序与下方写入逐字
    /// 对位），argc 槽字本身无读者，改动对现行行为惰性）。
    const ARGC_ARGV_ENVP: u64 = 3 * core::mem::size_of::<usize>() as u64;
```

（逐字引用，`os/arch/src/arch/boot.rs:fn load_elf_into` 的 `ARGC_ARGV_ENVP` 常量与它的文档注释；注释内含仓库内部编号与日期，按原文保留。）

C 的 `sp -= (sizeof(void *) + sizeof(void *) + sizeof(int))` 在 32 位上是 12 字节（三个 4 字节字，与"三个指针"重合），在 64 位上是 20 字节——`argc` 槽只有 4 字节，出生 sp 落在 4 字节边界。Rust 统一成三个整字（24 字节），出生 sp 回到 8 字节对齐。

**按"C 的断言"要求段起点页对齐会怎样**：链接器用 `lld` 的 separate-code 布局时后一个 `PT_LOAD` 会落在前一页中间，装载直接失败；若对"已存在的页"不做权限判别地复用，恒等映射的监督者页会成为用户段页，用户取指在 CPL3 下撞页错误（`#PF err=0x15` 是记录在案的现象形态）。

### 4.3 kernel 层：`ProcessTable` 与 `PrivTable` 的落地（→3.1、3.8）

两张表的全局访问器分成两组：启动期用的 `proc_table_boot_unchecked` 与持锁用的 `proc_table`：

```rust
    // SAFETY: boot is single-threaded before BKL exists.
    let proc_table = unsafe { crate::proc_table_boot_unchecked() };
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 开头；块上方是 C 的 `EXTERN struct proc proc[]` / `EXTERN struct priv priv[]` 对位注释。）

两个访问器的名字把前提写在类型层之外：`*_unchecked` 表示"没有 BKL 见证"，`*_with(_section: &BklSection)` 形态表示"调用者持有 BKL"。这套约定使"在启动期用锁版本"或"在运行期用 unchecked 版本"成为可被 review 抓住的写法差异。

访问方法的形状与 C 的宏族对照如下（§2.8 的宏表）：

| C 宏 | Rust 方法 | 差异 |
|---|---|---|
| `proc_addr(n)` | `ProcessTable::get(nr) -> Option<&KProcess>` / `get_mut` | 越界返回 `None` |
| `proc_nr(p)` | `KProcess::p_nr` 字段 | 直接读字段 |
| `isokprocn(n)` | `ProcessTable::is_valid_nr(nr)` | 内部走 `nr_to_idx` |
| `isemptyn(n)` / `isemptyp(p)` | `ProcessTable::is_empty(nr)` | 判据同 C：`SLOT_FREE` 精确相等 |
| `iskerneln(n)` | `ProcessTable::is_kernel(nr)` / `KProcess::is_kernel_task` | 符号判定（§3.3） |
| `for (p=BEG_PROC_ADDR; p<END_PROC_ADDR; p++)` | `ProcessTable::iter()` / `iter_mut()` | 迭代整表；测试断言条数等于 261 |
| `priv_addr(i)` | `PrivTable::get(id) -> Option<&KPriv>` | 越界返回 `None` |
| `priv(rp)` / `priv_id(rp)` | `KProcess::priv_id: Option<PrivId>` + `PrivTable::get` | 索引 + 查表，无桥接表 |

`ProcessTable::new` 里另外两个字段与本节无关但影响布局：`sched: [Scheduler; MAX_CPUS]` 是 per-CPU 就绪队列（C 的 `run_q_head[]`/`run_q_tail[]` 在 `__cpu_local_vars` 里），`vm_request_queue` 是 VM 请求队列的封装（C 的全局 `vmrequest` 链头）。两者都定容。

**`iter()` 不校验、按序号当进程号用会怎样**：表的物理下标 0 对应进程号 −5，序号与进程号整体错位；C 的遍历用指针自增、进程号从字段读，不存在这个陷阱——§5.3 的 `test_process_table_const_init_per_slot_nr` 逐槽核对 `p_nr == i - NR_TASKS` 正是为此。

### 4.4 kernel 层：`KProcess` 的字段分组（→3.11、3.3）

（本节除字段分组外，还含槽复用与槽交换两组对象级内容——它们把分组结论延伸到槽的生命周期，C 侧对位是 `adjust_proc_slot`。）

`KProcess` 的字段与 §2.1 的六组对照，多数同名保留，重要差异列在下面：

| C 字段（组） | Rust 对位 | 差异说明 |
|---|---|---|
| `p_nr`、`p_endpoint`、`p_name`、`p_magic` | 同名字段；`p_magic` 仅 `#[cfg(debug_assertions)]` | 哨兵只在调试构建存在 |
| `p_priv` | `priv_id: Option<PrivId>` | 指针 → 索引（§3.1） |
| `p_rts_flags`、`p_misc_flags` | `RtsFlags`、`MiscFlags`（原子包装） | §3.3 |
| `p_priority`、`p_quantum_size_ms`、`p_cpu_time_left`、`p_cpu`、`p_cpu_mask`、`p_scheduler` | `p_sched: SchedFields` + `Quantum` | §3.11，六个字段收成一个子结构 |
| `p_reg`、`p_seg` | `cpu_context: CurrentCpuContext` + `p_seg: ProcessSegments` | 寄存器区交给架构私有上下文；`p_seg` 只留页表根两个字段 |
| `p_pending`、`p_defer` | 同名字段（`SigSet`、`DeferArgs`） | `SigSet` 是 128 位（C 的 `sigset_t` 是 4×32 位） |
| `p_nextready`、`p_caller_q`、`p_q_link` | `p_nextready: AtomicI32`、`caller_q_head/tail: Option<ProcNr>`、`send_q_link: Option<ProcNr>` | 指针 → 索引；队列尾 O(1)（C 走链到尾） |
| `p_sendmsg`、`p_delivermsg`、`p_delivermsg_vir` | 同名字段 | 无差异 |
| `p_vmrequest`（总是内嵌 + `RTS_VMREQUEST` 指示有效） | `p_vm_suspend: Option<VmSuspendContext>` | 有效性写进类型；不变式 `RTS_VMREQUEST ⇔ is_some()` |
| `p_accounting` | `p_accounting: Accounting` | 六个子字段同名 |
| `p_user_time`、`p_sys_time`、`p_virt_left`、`p_prof_left` | `p_time: TimeStats` | 收成一个子结构（`p_dequeued` 不在其中，Rust 侧是独立字段） |
| `p_cycles`、`p_kcall_cycles`、`p_kipc_cycles`、`p_tick_cycles` | `p_cycles: CyclesStats` | 同上 |
| `p_cpuavg` | `p_cpuavg: CpuAvg` | 同名字段 |
| `p_found` | 无对位 | C 用它标记"本轮扫描已访问"，Rust 的调试检查改为局部位集（`os/kernel/src/debug.rs` 的 `found` 数组），不动进程表 |
| `p_stale_tlb` | 无对位 | 32 位架构专用；Rust 的 `p_seg` 只有 `phys_root`/`virt_root`，TLB 陈旧记账尚无归属（C 的写点在 `do_vmctl` 的 `VMINHIBIT` 解除分支、消费点在 CONFIG_SMP 下的 `lin_lin_copy` 一类路径），是否需要等价机制归 `10-smp-boot`（SMP 启动篇，尚未重生成） |
| `p_schedules`（`DEBUG_TRACE`） | 无对位 | 调试构建才有的计数 |
| （新增） | `p_fault_addr: Option<u64>`、`trap_style`、`fpu_state` | 诊断记录、进入内核的方式、FPU 缓冲 |

两个"结构体层面的保护"也在这里：占用的槽不能靠 Rust 的隐式析构销毁。

```rust
    fn drop(&mut self) {
        if self.slot_is_occupied() {
            panic!(
                "BUG: occupied KProcess slot #{} dropped without explicit destruction",
                self.p_nr.0
            );
        }
    }
```

（逐字引用，`os/kernel/src/proc.rs:impl Drop for KProcess`。）

这条 `Drop` 的逻辑与 C 的"槽只能由 `do_clear` 回收"是同一个不变量：处于占用状态的槽被析构，意味着 IPC 队列、定时器链、调度链上的其他槽仍指向它——C 里这不会有任何提示，Rust 里它会 `panic`。`p_nr` 出现在 panic 文案里，让现场能直接指出是哪一个槽。

`Drop` 只 panic、不做清理，这一点决定了整张表的复用模型：槽是常驻存储，"占用"与"空闲"是一次状态转换（`p_rts_flags` 从 `RTS_SLOT_FREE` 变成别的值、再回到它），而不是"构造对象 / 销毁对象"。这个模型成立的前提是 `KProcess` 不持有需要隐式释放的资源——FPU 缓冲是内嵌的定长数组，队列关系是槽号索引（`Option<ProcNr>`）而不是堆上节点，页表根由显式路径释放（`os/kernel/src/syscall.rs:release_address_space`），所以整槽复用不会漏掉析构动作。live update 的槽交换是这条模型的一个推论：`ProcessTable::swap_slots` 用 `core::mem::swap` 交换两个槽的全部内容，随后把"必须留在原槽"的身份字段按槽恢复——端点、进程号、`priv_id`、调度器、CPU、CPU 掩码与调用者队列头尾（`os/kernel/src/misc.rs` 的 `dispatch_update`，对位 C 的 `adjust_proc_slot`）；不恢复这些字段，交换就会把身份连同内容一起搬走。

**让 `KProcess` 实现 `Clone`/`Copy` 会怎样**：`*rpc = *rpp` 那种整块复制就有了 Rust 形态，而 fork 路径上"复制后要修哪些字段"的清单会退化成同样易漏的人工步骤（§3.13 讨论过的失败形态）；更重要的是队列链接会被复制——两个槽的 `caller_q_head` 指向同一个后继，链表出现分叉。

### 4.5 能力授予与 IPC 掩码的落地（→3.2、3.9）

两条授予路径对应 boot 循环的两类可调度成员。内核 task：

```rust
        let template = if nr == proc_nr::IDLE {
            crate::capability::CapabilityTemplate::Idle
        } else {
            crate::capability::CapabilityTemplate::KernelTask
        };
        let priv_id = priv_table.grant_capability(nr, template)
            .expect("grant_capability: kernel task priv slot occupied");
        // E1 slice 5: link the priv slot back into the process — C's
        // `get_priv(rp, id)` sets rp->p_priv (proc.h); without this the
        // process's priv_id stays None and every kernel call is denied
        // ECALLDENIED (observed live in test-user-trap).
        proc.priv_id = Some(priv_id);
```

（逐字引用，`os/kernel/src/lib.rs:init_proc_and_boot` 的内核 task 循环；注释里的切片编号是仓库内部编号，按原文保留。）

用户模块那一侧的模板选择与同一个回链动作（`is_vm` 取 `Vm`、否则 `RootService`）。注意回链那一步：C 里 `get_priv` 一个函数同时做"分配槽"与"写 `rc->p_priv`"，Rust 拆成两步——`grant_capability` 填槽内字段并返回槽号，调用者再把槽号写进进程。注释记录了漏掉第二步的现场：进程的权限槽存在但进程找不到它，所有内核调用被拒（`ECALLDENIED`）。

`fill_sendto_mask` 在授予函数内部被调用一次（§3.2 末段已引），它与 C 的逐位路径一致：

```rust
    pub fn fill_sendto_mask(&mut self, rp: PrivId, map: IpcMask) {
        for idx in 0..NR_SYS_PROCS {
            if map.has_bit(idx) {
                self.set_sendto_bit(rp, idx as PrivId);
            } else {
                self.unset_sendto_bit(rp, idx as PrivId);
            }
        }
    }
```

（逐字引用，`os/kernel/src/kpriv.rs:PrivTable::fill_sendto_mask`。）

而 `set_sendto_bit` 的两个守卫与互惠置位在 Rust 侧需要一次借用拆分——同一次操作要同时改两个槽：

```rust
        // C: system.c:321-328 — grant own bit, then the reciprocal bit
        // unless the target is RECEIVE-only. Read the target's trap mask
        // before taking the two mutable borrows.
        let target_can_reply = self
            .get(id)
            .map(|p| p.ipc.s_trap_mask.allows_more_than_receive())
            .unwrap_or(false);
        let (rp_slot, id_slot) = self.get2_mut(rp, id);
```

（逐字引用，`os/kernel/src/kpriv.rs:PrivTable::set_sendto_bit`。）

"先读后借"的顺序不是风格：`get2_mut` 通过一次 `split_at_mut` 同时给出两个可变引用，而在它之前必须先把"对端是否只允许 RECEIVE"这个只读查询做完——否则两个可变引用期间还要再借一次表。C 那边用的是裸指针，没有这层约束；这是 Rust 借用规则在实现层留下的一处可见痕迹。

**把 `set_sendto_bit` 改成两步借用会怎样**（"先改自己、释放借用、再改对端"）：两步之间表处于"单向授权"的中间态；如果第一步之后发生任何提前返回（例如对端索引越界），不变量就停在中间态。`get2_mut` 的写法把两步放在一次借用内，中间态不可观察。

### 4.6 kernel 层：`KPriv` 8 子结构与 wire 边界（→3.10）

`KPriv` 的子结构顺序与 wire 结构 `PrivUpdateRequest` 的顺序是同一个：

```rust
/// `#[repr(C)]` ensures deterministic layout for cross-space `data_copy`.
/// The user-space RS (Reincarnation Server) fills this struct and passes
/// its address via `m_lsys_krn_sys_privctl.arg_ptr`.
///
/// Field order mirrors the KPriv 8-substructure sequence
/// (06-proc-init-boot-proc.md §3.10), minus the kernel-internal
/// `PrivRuntime`: identity → flags → init → signal managers → IPC masks →
/// I/O then IRQ → memory (each resource group lists its count before the
/// table, matching `PrivIo`/`PrivMem`). The RS-side fill struct `Privilege`
/// (`os/servers/rs/src/privilege.rs`) lists its fields in the same order,
```

（逐字引用，`os/kernel/src/kpriv.rs:struct PrivUpdateRequest` 的文档注释。）

三处顺序一致是手工维持的约定：内核的 8 子结构、wire 结构、RS 侧的填充结构。wire 结构少一个 `PrivRuntime`（内核内部状态不出现在用户态请求里），因此"字段数相同"不成立，"相对顺序相同"是判据。

能力类型直接存在 `KPriv` 里，wire 边界做宽度转换：

```rust
//! `KPriv` stores these types directly (06-proc-init-boot-proc.md §4.6,
//! 8-substructure layout): `PrivFlags::s_flags: ProcessCapability`,
//! `PrivIpc::s_trap_mask: TrapMask`, `PrivIpc::s_ipc_to: IpcMask`,
//! `PrivIpc::s_k_call_mask: KCallMask`. The wire boundary keeps the raw
//! Minix3 widths (`PrivUpdateRequest.s_flags: u16`, `.s_trap_mask: u16`,
//! `.s_ipc_to: u64`, `.s_k_call_mask: [u32; SYS_CALL_MASK_SIZE]`) and
//! converts via each type's `from_wire` / `to_wire` codec.
```

（逐字引用，`os/kernel/src/capability.rs` 的模块文档"Storage in KPriv"段。）

`ProcessCapability` 的位定义分两层：低 11 位是 C 的 `s_flags` 位（`minix3/minix/include/minix/const.h` 的 11 个：`PREEMPTIBLE` `0x002`、`BILLABLE` `0x004`、`DYN_PRIV_ID` `0x008`、`SYS_PROC` `0x010`、`CHECK_IO_PORT` `0x020`、`CHECK_IRQ` `0x040`、`CHECK_MEM` `0x080`、`ROOT_SYS_PROC` `0x100`、`VM_SYS_PROC` `0x200`、`LU_SYS_PROC` `0x400`、`RST_SYS_PROC` `0x800`），高位是三个 Rust 扩展位（`KILL`/`SIGS_SYS`/`OWN_ID`），后者"在 16 位 wire 范围之上、从不上 wire"——`to_wire` 的实现是 `(self.bits() & 0xFFFF) as u16`。

| wire 字段 | 宽度 | 内核侧类型 |
|---|---|---|
| `s_flags` | `u16` | `ProcessCapability`（`u32`，扩展位被 `to_wire` 截掉） |
| `s_trap_mask` | `u16` | `TrapMask` |
| `s_ipc_to` | `u64` | `IpcMask`（`NR_SYS_PROCS = 64` 位，单字） |
| `s_k_call_mask` | `[u32; 2]` | `KCallMask` |

**让 Rust 扩展位也上 wire 会怎样**（"统一宽度更简单"）：`s_flags` 的宽度从 16 位变成 32 位，RS 侧结构与内核侧结构的尺寸不再一致；`data_copy` 按内核侧尺寸从用户地址拷贝时会读到结构之外的字节，或者写回时覆盖用户地址空间里相邻的数据。

### 4.7 主流程：`init_proc_and_boot` 的逐步走查（→3.12、3.4）

函数体的实际顺序与 §2.6 的 12 动作对位如下（Rust 侧多了几件 C 没有的工作，一并列出）：

| # | C 动作（§2.6） | Rust 位置 |
|---|---|---|
| — | 模块数量断言（Rust 为编译期 `assert_eq!`；C 侧同类检查是运行期 `panic("expecting %d boot processes/modules...")`——形态不同、意图对位） | `assert_eq!(kernel_info.boot_modules().len(), NR_BOOT_MODULES, ...)` |
| 1–3 | 取条目与槽、回填端点、清时间片 | `KERNEL_TASKS`/`BOOT_MODULE_PROC_NRS` 两轮循环的头部 |
| 4–5 | 写名字 / 回填模块地址 | 内核 task 轮写的是常量名；用户模块轮写 `module.name`，地址信息由 `KernelInfo` 携带、不在进程表 |
| 6 | 记账清零 | `reset_proc_accounting` 的对位（清零在 `new_zeroed` 的初值里完成） |
| 7 | 判可调度 + 授予权限 | 用户模块轮的 `schedulable` 分支（§4.5） |
| 8 | 否则置 `NO_PRIV \| NO_QUANTUM` | 同上的 `else` 分支（§3.9） |
| 9 | `arch_boot_proc` | VM 分支：`load_vm_elf` + 交接页 + 根地址登记（见下） |
| 10 | `proc_ptr` 兜底（循环内第一次遇到空指针时填） | Rust 侧没有逐次兜底：记账半边由 `set_bill_to_idle` 清 idle 槽的 `enter_queue`，当前指针在调度循环入口 `scheduler_loop` 种入 |
| 11 | 双闸 | 用户模块轮尾部（§3.9） |
| 12 | `PROC_STOP` + 清 `SLOT_FREE` | 用户模块轮尾部 |
| — | （C 无）VM 引导区选择与排除集 | `VmBootRegion::select_multi` + `push_exclusion`（§3.8） |
| — | （C 无）堆窗口驱逐 | 装载完成后对 `VM_HEAP_BASE..+VM_HEAP_SIZE` 逐页拆大页、开放用户走查、解除映射 |
| — | （C 无）交接页写入 | `build_vm_handoff` + `core::ptr::write(hv, handoff)`；随后登记 `p_seg.phys_root`/`virt_root`。交接页内容=VM 启动所需的自由内存清单与自举记录（字段构成与 VM 侧消费属 `09-vm-boot-protocol`，见 §1.5 的边界表） |

VM 分支比 C 多出的三件事都源自同一件事：C 的 VM 在运行后会自己安装页表，而本端把 bootstrap 根直接交给 VM 并让它采纳（"the bootstrap root IS VM's initial and permanent root"）。因此需要在装载后立刻清理"VM 运行期会用到、但此刻被引导恒等映射占着"的虚拟地址区间（堆窗口），并把根的地址写进进程槽供 §3.12 的确认使用。

堆窗口驱逐的形状：

```rust
                {
                    let heap_lo = <minix_arch::CurrentDirectMap as minix_arch::arch::direct_map::DirectMapArch>::VM_HEAP_BASE;
                    let heap_end = heap_lo
                        + <minix_arch::CurrentDirectMap as minix_arch::arch::direct_map::DirectMapArch>::VM_HEAP_SIZE;
                    let page = CurrentPaging::PAGE_SIZE as u64;
                    let mut hva = heap_lo;
                    while hva < heap_end {
                        let v = VirBytes(hva);
                        if paging.query(v).is_some() {
                            paging.split_huge(v).unwrap_or_else(|e| panic!(
                                "heap-window eviction: split_huge failed ({e:?}) — \
                                 arm64/riscv64 must implement it before their first light"
                            ));
```

（节选，`os/kernel/src/lib.rs:init_proc_and_boot` 的生产分支——块内在 `split_huge` 的 panic 文案之后省略 `grant_user_walk` 与 `unmap` 两个调用。）

驱逐必须发生在 `load_vm_elf` **返回之后**：装载期间模块镜像靠恒等映射直读，驱逐提前会让复制中途的字节消失。源码注释把这条次序依赖写在了原地。

**排除集只排除内核镜像会怎样**（不排除 boot 模块）：`VmBootRegion::select_multi` 会把某个 boot 模块占用的物理区间当作"空闲"选进 VM 的引导区，随后 VM 的 ELF 段帧可能分配在该区间上；模块回收（`add_memmap`）又把它标回空闲——同一段物理内存同时属于"VM 的段页"与"空闲清单"，下一次分配写进去就是覆盖正在使用的页。排除集长度 `NR_BOOT_MODULES + 1` 正是"内核 1 段 + 模块 12 段"。

### 4.8 boot→running：`bsp_finish_booting` 的解除与交接（→3.9）

Rust 侧这段的步骤序（与 C 的 `bsp_finish_booting` 对照）：

| C 步骤 | Rust 步骤 | 差异 |
|---|---|---|
| `cpu_identify()` | `cpu_identify()` | 读 CPU 身份写全局表 |
| `vm_running = 0` | `VM_RUNNING.store(false, ...)` | 普通全局 → `AtomicBool` |
| `bill_ptr = proc_ptr = idle_proc` | `proc_table.set_bill_to_idle()` + `scheduler_loop` 入口的 `proc_ptr = IDLE` | 记账半边在 `set_bill_to_idle`；当前指针在调度循环入口种入 |
| `announce()` | 打印一行 banner 到早期控制台 | 输出通道不同，语义相同 |
| 解除循环（进程号 0..11） | 同一循环（§3.9 已引） | 上界与迭代对象一致 |
| `cycles_accounting_init()` | `note_context_switch(tsc)` | 给 BSP 记一个计时基线 |
| `boot_cpu_init_timer(system_hz)` | `boot_init_timer()` | 05 篇已详述（编程 → 注册 → 开门三段） |
| `fpu_init()` | `bsp_local.fpu_presence = true` | 三架构均有 FPU，逐进程策略在上下文里 |
| `machine.processors_count` / `bsp_id` | per-CPU 状态里的对应字段 | 见 `10-smp-boot`（SMP 启动篇，尚未重生成） |
| `kernel_may_alloc = 0` | `KERNEL_MAY_ALLOC.store(false, ...)` | 关掉内核直接使用主存的窗口 |
| `BKL_LOCK()`（在 `main()` 早期，不在本函数内） | `debug_assert!(smp::bkl_is_locked())` | 获取点移到 `kmain`（`wait_for_aps` 的 UNLOCK→wait→LOCK 收尾时已在手），这里只留断言钉住前提 |
| `switch_to_user()` + `NOT_REACHABLE` | `switch_to_user()`，函数类型为 `!` | 永不返回写进类型 |

其中"解除循环"与"关窗口"的次序是本篇的收尾问题。C 的次序是：解除 → 计时基线 → 定时器 → FPU → 关窗口 → 切用户。Rust 保持同序。

BKL 这一项是两边的另一处差别：C 在 `bsp_finish_booting` 内没有锁动作，Rust 在切用户前留一条 `debug_assert!(smp::bkl_is_locked())`（`os/kernel/src/lib.rs:bsp_finish_booting`），把"进入 `switch_to_user` 时 BKL 已在手"这个前提写成可执行的检查；BKL 跨 `switch_to_user` 的归属与释放时点归 `08-system-init-boot-finish`（启动收尾与切用户篇，尚未重生成）与 `10-smp-boot`（SMP 启动篇，尚未重生成）。

**把"关窗口"提到解除循环之前会怎样**：解除循环清掉 12 个进程的 `PROC_STOP`，但**其中只有 VM 因此变为可运行**（RS 仍带双闸、其余十个还带 `NO_PRIV|NO_QUANTUM` 与双闸——见 §1.4 的位表），所以"开始跑用户态"的是 VM。窗口的关闭点必须落在**最后一次内核侧直接分配**之后：窗口一旦早于任何还会走 `pg_alloc_page` 的步骤（例如之后新增的某步），第一次需要直接分配的内核路径就会撞上"内核不得直接分配"的断言；本篇把它定在切用户之前，为的是不再依赖对"还有没有分配"的逐处论证——用户态在 `switch_to_user` 之后才开始，两步骤序都改变不了它的页错误现场，这一点如实说明（本条是设计约束，不是已观察故障）。

### 4.9 与 Minix3 C 的对照

| C 定义 | Rust 对位 | 档位 | 依据节 |
|---|---|---|---|
| `struct proc`（40 顶层成员） | `KProcess` | Rewrite/Refactor | §2.1、§4.4 |
| `EXTERN struct proc proc[261]` | `static PROC_TABLE` + `ProcessTable` | Refactor | §2.0、§3.1、§4.3 |
| `struct priv`（31 字段） | `KPriv` 8 子结构 | Refactor | §2.3、§3.10、§4.6 |
| `EXTERN struct priv priv[64]` + `ppriv_addr[]` | `static PRIV_TABLE`（无桥接表） | Refactor | §2.3、§3.1、§3.10 |
| `proc_addr`/`isokprocn`/`iskerneln` 等宏族 | `nr_to_idx` + 访问方法 | Refactor | §2.8、§3.1、§4.3 |
| `proc_init` | `ProcessTable::new`（`const fn`） | Refactor | §2.5、§3.1 |
| 16 个 `RTS_*` + 三个操作宏 | `RtsFlagsBits` + `RtsFlags` + `rts_set`/`rts_unset` | Rewrite | §2.2、§3.3 |
| 18 个 `MF_*` | `MiscFlagsBits` | Refactor | §2.1.6、§3.3 |
| `struct boot_image image[17]` | `KERNEL_TASKS` + `BOOT_MODULE_PROC_NRS` | Rewrite | §1.4.1、§2.4、§3.4 |
| boot 循环（12 动作） | `init_proc_and_boot` | Rewrite | §2.6、§4.7 |
| `get_priv` + 六字段赋值 | `grant_capability` + `CapabilityTemplate` | Refactor | §2.3、§3.2、§4.5 |
| `set_sendto_bit`/`unset_sendto_bit`/`fill_sendto_mask` | 同名方法 | Refactor | §2.3.3、§3.2、§4.5 |
| `arch_proc_reset`/`arch_proc_init`/`arch_boot_proc` | `CpuContextArch::build_cpu_context` + `load_vm_elf` | Refactor | §2.7、§3.5、§4.1、§4.2 |
| `p_seg.fpu_state` 池 + `fnsave` 路径 | 内嵌 `fpu_state` + 三架构懒初始化策略 | `[ARCH: K-4]` | §2.1.4、§3.6 |
| `arch_post_init` + `memory_init`（freepdes） | `init_post_and_memory`（三条就绪断言；ptproc 记录在 `switch_address_space`） | `[ARCH: Direct Map]` | §3.12 |
| 初始栈：C 给 64 KiB + VM 自扩（`reservedqueue.mappedin`） | 一次性 256 KiB 固定额度（无自扩） | `[ARCH: K-5]`（外部可观察的额度变化） | §4.2 |
| `bsp_finish_booting` | 同名函数，返回 `!` | Rewrite | §3.9、§4.8 |
| `_ENDPOINT`/`_ENDPOINT_P`/`_ENDPOINT_G` | `Endpoint::from_generation_slot`/`slot`/`generation` | Rewrite | §2.8 |

表中的档位沿用 §3.14 的去向总表（该表逐行给出依据；本表从简，冲突时以 §3.14 为准）。

---

## 5. 测试要点

### 5.1 宿主测试读数与口径

本篇涉及的四个 kernel 文件与四个 arch 文件里的宿主测试条数（口径：源码里 `#[test]` 的出现次数，含模块内全部测试；复算命令见 §6.2 里标号 8 的那条）：

| 文件 | `#[test]` 条数 | 主要覆盖 |
|---|---|---|
| `os/kernel/src/proc.rs` | 43 | 位图类型、优先级/时间片、CPU 掩码、名称、fork 继承分支 |
| `os/kernel/src/proc_table.rs` | 41 | 表的构造、访问原语、RTS 跃迁、VM 请求队列、misc 位清理 |
| `os/kernel/src/kpriv.rs` | 39 | 特权表构造、槽释放、`grant_capability`、`set/fill` 掩码 |
| `os/kernel/src/capability.rs` | 15 | 能力位组合、模板与 wire 编解码 |
| `os/arch/src/arch/boot.rs` | 14 | `load_vm_elf` 的解析、布局、错误路径、尾指令缓存同步 |
| `os/arch/src/x86_64/boot.rs` | 14 | 初值 PSW 两态、FPU 策略继承、栈回溯 |
| `os/arch/src/arm64/boot.rs` | 5 | 初值状态字与 `fpu_enable_el0` 继承 |
| `os/arch/src/riscv64/boot.rs` | 4 | `sstatus` 两态与整值继承 |

三架构把 §4.1 差异面表的对应行固定成回归读数：`kernel_task_uses_init_task_psw` 与 `user_process_uses_init_psw`（x86-64 的两个初值状态字）、`inherit_fpu_state_propagates_lazy_user_policy`（x86-64）、`inherit_fpu_state_propagates_fpu_enable_el0`（aarch64）、`inherit_fpu_state_copies_sstatus`（riscv64）。

这些读数跑在 `feature = "mock"` 的构建下（`os/kernel/Cargo.toml` 的 `default = ["mock"]` 就是宿主测试形态，生产形态走 `--no-default-features`）：`os/kernel/src/lib.rs` 里成对的 `#[cfg(not(feature = "mock"))]` 与 `#[cfg(all(test, feature = "mock"))]` 分支用替身顶替真实的物理内存访问与空闲内存图，所以"测试通过"与"真机行为"之间隔着这一层替身。

宿主测试能证明的部分：表的每槽初值、访问原语的边界、位图的取值与跃迁判定、掩码的自位清除与互惠、ELF 装载器的解析与错误分类。宿主测试不能证明的部分：真实页表的映射后果（`paging.query` 在测试里是 mock）、真机上的首次调度与用户态取指、以及"启动期没有堆分配"这条约束本身（它靠代码审查与构建配置保证，不靠单测）。

### 5.2 机制 → 需要证明什么（反转核对表）

本节把顺序倒过来核对一遍：**从机制出发**，逐条问"要证明它成立需要什么可观测断言、现有测试是否覆盖"。这张表是"覆盖证据"（§5.1 读数、§5.3 测试清单）的补集视角——只从测试出发的清单里，**没有任何测试覆盖的机制不会出现**；反转核对专门把它们找出来（01 篇的五层结构中的"机制→断言"一层，在本篇的落地）。

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 证据类型 | 覆盖状态 | TODO |
|---|---|---|---|---|---|
| M1 | 容量与槽定位（§1.2/§2.0/§2.8） | 261 槽可遍历；下标 = `NR_TASKS + p_nr` 逐槽成立；边界宏两端正确 | 宿主 | **已覆盖**（`test_iter_counts` / `test_process_table_const_init_per_slot_nr` / `test_is_valid_nr_boundaries`） | — |
| M2 | 三套编号映射（§1.3/§1.4.1/§2.4） | `KERNEL_TASKS` 与 `BOOT_MODULE_PROC_NRS` 的 12 项顺序与 `image[]` 的模块序一致（位置约定） | 宿主（常量对账） | **未覆盖**：现有测试只查槽内 `p_nr`，不查两张常量表与清单的顺序一致 | **R06-T06** |
| M3 | 四道抑制位（§1.4/§2.6） | boot 循环后各 boot 成员的位集与 C 表一致（谁挂双闸、谁挂 `NO_PRIV\|NO_QUANTUM`） | 真机 | 部分（`test_process_table_idle` 查 idle 的 `PROC_STOP`；全表位集靠真机载体） | R06-T01（已登记） |
| M4 | 位与队列的绑定（§2.2.1） | `RTS_SET`/`RTS_UNSET` 只在跃迁时动队列；`SETFLAGS` 整值覆盖 | 宿主 | **已覆盖**（`test_rts_set_unset` 组） | — |
| M5 | 位值表（§2.1.6/§2.2） | RTS 16 位与 MF 18 位的位值与 C 完全相同 | 宿主（常量对账） | 部分：类型常量保证定义；**逐位值无独立测试** | **R06-T07** |
| M6 | 特权表与模板族（§2.3/§2.6） | 64 槽与静态/动态分段；模板位组合与 C 一致；`fill` 的自位清除与互惠 | 宿主 | **已覆盖**（kpriv 组 + `set/fill` 组） | — |
| M7 | VM ELF 装载（§2.7/§4.2） | 无效 ELF/OOM 返回错误；栈与 `ps_strings` 布置；`.bss` 清零；逆序分配；icache 同步恰好一次 | 宿主 | **已覆盖**（`load_vm_elf_*` 7 条 + `load_process_elf_*` 2 条；`os/arch/src/arch/boot.rs` 共 14 条） | — |
| M8 | 入口上下文初值（§3.5/§4.1） | 内核 task 与用户进程的初值状态字两态；段选择子；FPU 策略继承 | 宿主 | **已覆盖**（x86_64 两态 + 三架构继承各一） | — |
| M9 | `init_post_and_memory` 的就绪断言（§3.12/§4.7） | `phys_root == 0` 的槽在切地址空间前被断言拦住 | 宿主或真机 | **未覆盖** | **R06-T08** |
| M10 | 零堆约束（§2.0/§3.8） | 生产构建不链接 `alloc` | 构建期 | 无机器门 | R06-T02（已登记） |
| M11 | boot 循环 12 动作与主流程（§2.6/§4.7） | 12 动作逐一生效、`init_proc_and_boot` 端到端 | 真机 | 载体存在、通过状态未复核 | R06-T01（已登记） |
| M12 | FPU 懒陷阱门控（§3.6/§4.1） | 三架构门控的真机行为 | 真机 | 无真机回归 | R06-T03（已登记） |
| M13 | `ImageInsideSegments` 前提（§4.2） | "真机布局不产生该重叠"的观测 | 真机 | 按源码陈述 `[待验证]` | R06-T04（已登记） |
| M14 | `iter()` 序号语义（§4.3） | 序号不得当进程号用 | 宿主 | 部分（`test_iter_counts`） | R06-T05（已登记） |

**反转核对判定（三选一）：(b) 有遗漏 → 已执行本节改造**——新增 3 条覆盖不足的机制（M2、M9 未覆盖；M5 逐位对账缺失 → R06-T06/T07/T08，登记进 §5.4 的缺口表）；本表与测试清单（§5.3）构成"机制视角 + 测试视角"两面。

**自检（第二重收益）**：M1–M14 全部写得出可观测断言，无"写不出"的机制。

### 5.3 进程表访问原语与表的构造测试（→4.3、4.4）

`proc_table.rs` 的测试按被验证的对象分几类，逐条列名如下（测试名逐字取自源码）：

| 测试名 | 验证的 C 不变量 |
|---|---|
| `test_process_table_new` | `get()` 的边界（`ProcNr(0)`、`ProcNr(-1)`、`ProcNr(255)` 有值，`ProcNr(256)` 为 `None`） |
| `test_process_table_const_init_per_slot_nr` | 每槽 `p_nr == i - NR_TASKS`、`p_endpoint != NONE`（对位 `proc_init` 的两行） |
| `test_process_table_const_init_idle_name` | idle 槽的名字是 C 的拼写（`set_idle_name` 的产物，`GET_PROC` 会逐字导出） |
| `test_process_table_idle` | idle 槽的 `PROC_STOP` 已置位 |
| `test_is_valid_nr` / `test_is_valid_nr_boundaries` | `isokprocn` 的两端边界（`ProcNr(-6)` 非法、`ProcNr(-5)` 合法、`ProcNr(255)` 合法、`ProcNr(256)` 非法） |
| `test_is_kernel` / `test_is_kernel_task` | `iskerneln` 与 `iskernelp` 的符号判定 |
| `test_proc_nr_roundtrip` | 进程号 → 下标 → 进程号的往返 |
| `test_is_empty_new_table` / `test_is_empty_after_alloc` | `isemptyp` 的判据（`SLOT_FREE` 精确相等） |
| `test_iter_counts` | 遍历条数等于 261 |
| `test_rts_set_unset` / `test_rts_set_unset_multiple_flags` / `test_rts_set_idempotent` | 三个宏的跃迁语义与幂等性 |
| `test_process_misc_flags_*`（多条） | `MF_*` 位的清理与 `MF_SIG_DELAY` 等分支 |
| `test_fork_integration_flow` / `test_fork_child_rts_corrections` | fork 后子进程的 RTS 位修补 |
| `test_set_bill_to_idle` | `bsp_finish_booting` 第 2 步的对位 |
| `test_vm_memreq_*`（多条） | VM 请求队列的入队/出队/重锚 |

`kpriv.rs` 一侧对应特权表与掩码：

| 测试名 | 验证的 C 不变量 |
|---|---|
| `test_priv_table_const_init_sets_per_slot_s_id` | 每槽 `s_id == i`、`s_proc_nr == None`（对位 §2.5 第二个循环） |
| `test_priv_table_new` | 槽数 64、`get(64)` 返回 `None` |
| `test_priv_flag_set_idl` / `_usr_no_sys_proc` / `_vm` | `IDL_F`/`USR_F`/`VM_F` 的位组合与 `priv.h` 常量一致 |
| `test_kpriv_new` / `test_kpriv_is_sys_proc` / `test_kpriv_flag_predicates` | 旗标判定谓词 |
| `test_drop_unassigned_slot_is_normal` / `test_slot_is_occupied_agrees_with_binding` | 槽所有权不变量（`Drop` 判据） |
| `grant_capability` 一组（`kpriv.rs` 内的 `mod tests` 段） | 五种模板的字段与掩码结果、自位清除 |
| `set/fill_sendto_mask` 一组 | 守卫分支、互惠置位、"自位必须保持清除"的断言 |

其中与 §2.3.3 直接对应的是自位断言：`assert!(!mask.may_send_to(...), "self bit must stay clear")` 一类的断言在多处出现，它们把"`fill_sendto_mask` 是终态来源"这件事固定成可回归的读数。

`arch/boot.rs` 的测试覆盖装载器：

| 测试名 | 验证的内容 |
|---|---|
| `load_vm_elf_invalid_elf_returns_err` | `InvalidElf` 路径 |
| `load_vm_elf_out_of_memory_returns_err` | `OutOfMemory` 路径 |
| `load_vm_elf_places_stack_and_ps_strings_like_c` | 栈与 `ps_strings` 的布置与 C 对位 |
| `load_vm_elf_zeroes_bss_and_copies_filesz` | `.bss` 清零与文件段拷贝的分界 |
| `load_vm_elf_allocates_strictly_descending_across_regions` | 跨引导区的分配次序 |
| `load_vm_elf_syncs_icache_over_exec_segment_frames` | 可执行段的帧区间恰好同步一次（`[ARCH: cache-sync]`） |
| `load_vm_elf_skips_icache_sync_for_non_exec_image` | 非可执行镜像不同步 |

### 5.4 测试缺口与诚实披露

| 缺口 | 现状 | 为什么留 | 解除条件 |
|---|---|---|---|
| 真机启动链 | **载体已核实**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/`（x86_64 UEFI `.efi` 测试内核），由 `os/qemu-tests/run_all.sh` 驱动（`run_test "test-proc-init" x86_64 ...`）；按载体头部注释覆盖 7 点——内核 task 初始化、`SLOT_FREE`/`PROC_STOP` 位、VM 的 RTS 位、非 VM 用户的 `VMINHIBIT`+`BOOTINHIBIT`、`VM p_seg` 可达、`init_post_and_memory` 完成性、自举根与 VM Direct Map 的哨兵检查。**本轮未实跑**（文档侧不跑测试验证——按源码边界，见本节末） | 原缺口是「未核实载体名称与覆盖范围」，本轮按源码清点解除 | 载体演进时同步核对覆盖声明 |
| "启动期零堆分配"这条约束 | 无机器门 | 它由构建配置（生产构建不链接 `alloc`）与代码审查保证 | 加一条构建期或链接期的检查 |
| `Slice`/`iter` 的遍历与进程号换算的一致性 | 部分被 `test_process_table_const_init_per_slot_nr` 覆盖 | `iter()` 的序号语义只在测试里用到 | 若运行期出现"按序号当进程号"的用法，补一条断言 |
| `RTS_VMREQTARGET` 的定义/使用不对称 | 无测试 | 该位"有名字、有打印、有人读，但没人置"（§2.2.2），测试无处下断言 | 若上游语义变化（置位点出现或该位被删除），同步更新 §2.2.2 |
| `RTS_NO_ENDPOINT` 无具名解除点 | **已覆盖**：回收终态由 `dispatch_clear` 的回归测试断言（§2.2.2 末段） | 无具名 `RTS_UNSET` 解除点属上游形态（`XXX` 注释标待清理） | 上游删除该位时同步 §2.2.2 |
| 三架构 FPU 懒陷阱时序 | 无真机回归（本篇未核实）`[待验证]` | 属 FPU 专题与 SMP/调度篇的交叉 | 见 FPU 专题（尚未重生成） |
| `BOOT_MODULE_PROC_NRS` ↔ `image[]` 模块序（M2） | 未覆盖（常量对账缺口，反转核对发现） | 位置约定目前只靠人工对齐 | 加一条常量顺序对账断言（R06-T06） |
| RTS/MF 位值逐位对账（M5） | 部分覆盖（类型常量保证定义，无逐位测试） | 位值在类型定义处集中，漂移面小 | 加构建期 const 对账或宿主测试（R06-T07） |
| `init_post_and_memory` 根就绪断言的负控（M9） | 未覆盖（反转核对发现） | 断言在真机载体上被动执行 | 负控测试或真机检查点（R06-T08） |

**诚实披露**：本篇的 §3.6 与 §4.1 涉及的三架构 FPU 时序、以及 §4.2 的 `ImageInsideSegments` 触发条件，均按源码与注释陈述（§5.2 的反转核对表给出机制视角的覆盖全貌）；真机行为未在本篇核实，逐项已标 `[待验证]`。真机链的载体名称与覆盖范围本轮已按源码清点（上表第一行），但其**通过状态**未在本篇复核——文档侧不跑测试验证（源码边界）。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认动作 | 期望读数 |
|---|---|---|---|
| 启动后没有任何进程被调度，串口停在 banner 之后 | `RTS_PROC_STOP` 的解除循环上界、`RTS_VMINHIBIT` 是否误挂到 VM | 读解除循环的上界表达式；读双闸的分支条件 | 上界是 `NR_BOOT_PROCS - NR_TASKS`；双闸条件是 `nr != VM_PROC_NR` |
| 某个服务第一次内核调用被拒（`ECALLDENIED` 一类的拒绝） | `priv_id` 是否回链、模板是否授予 | 查授予路径里有没有 `proc.priv_id = Some(priv_id)` | 两条授予路径各有一处 |
| 用户进程第一次取指异常（地址 0 或监督者页） | `EntrySpec` 是否得到 `loaded`（装载是否静默失败）、恒等映射残留是否驱逐 | 查 `load_vm_elf` 的返回是否为 `Err`；查驱逐分支的判别条件 | 返回类型是 `Result`；驱逐判别是"页存在但无 USER 权限" |
| 可用内存比预期少一块（约等于某个模块大小） | 模块物理内存的回收遗漏 | 查 `load_vm_elf` 之后是否有把模块区间还给空闲清单的动作；查 `mod_start`/`mod_end` 是否置 0 | 两件事成对出现（C 是 `add_memmap` + 置 0） |
| 掩码检查出现"我能收你的、不能发给你" | `fill_sendto_mask` 是否被整体赋值替代 | 查 `fill_sendto_mask` 的实现是否逐位走 `set`/`unset` | 逐位路径（`for idx in 0..NR_SYS_PROCS`） |
| VM 永远不被选中 | `RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 是否被挂到 VM、`p_seg.phys_root` 是否为 0 | 读双闸条件与 `init_post_and_memory` 的断言 | 双闸排除 VM；断言要求根非 0 且 virt 根存在 |

### 6.2 可复制命令与预期输出

以下命令都在仓库根目录执行，每条都已实跑并给出预期输出的关键行。第一条到第七条是 C 侧的读数，第八条是 Rust 侧的读数。

```bash
# 1) 四个容量常量现算（口径：宏展开式，不抄表）
python3 -c "NR_TASKS=5; LAST=11; print('NR_BOOT_PROCS', NR_TASKS+LAST+1, 'NR_BOOT_MODULES', LAST+1, 'USER_PRIV_ID', NR_TASKS+11, 'PROC_TABLE_SIZE', NR_TASKS+256)"
# 预期输出：NR_BOOT_PROCS 17 NR_BOOT_MODULES 12 USER_PRIV_ID 16 PROC_TABLE_SIZE 261

# 2) 位定义条数（口径：以 #define 开头、名字以 0x 值结尾的行）
grep -c '^#define RTS_[A-Z_]*[[:space:]]*0x' minix3/minix/kernel/proc.h
grep -c '^#define MF_[A-Z_]*[[:space:]]*0x' minix3/minix/kernel/proc.h
# 预期输出：16 与 18；操作宏另算（grep -c '^#define RTS_[A-Z]*(' → 4）

# 3) 结构体成员计数（口径：顶层声明行；proc 含 2 个匿名聚合，priv 无匿名）
awk '/^struct proc \{/,/^\};/' minix3/minix/kernel/proc.h | grep -cE '^  [a-zA-Z_].*;|^  struct \{$'
awk '/^struct priv \{/,/^\};/' minix3/minix/kernel/priv.h | grep -cE '^  [a-zA-Z_].*;'
# 预期输出：40（37 具名 + 3 个匿名聚合类型）与 31

# 4) boot image 条目数
awk '/struct boot_image image/,/^\};/' minix3/minix/kernel/table.c | grep -cE '^\{'
# 预期输出：17

# 5) VMREQTARGET 的全部出现（口径：全 C 树，形态为 定义/打印/读取）
grep -rn "RTS_VMREQTARGET" minix3/
# 预期输出：3 行——proc.h 的 #define、debug.c 的 FLAG 打印、servers/mib/proc.c 的 RTS_ISSET

# 6) 解除循环与填充循环的边界（口径：main.c 里出现 NR_BOOT_PROCS 的全部行）
grep -n "NR_BOOT_PROCS" minix3/minix/kernel/main.c
# 预期输出：2 行——两处 for 的上界表达式（`NR_BOOT_PROCS - NR_TASKS` 与 `NR_BOOT_PROCS`）。
#   注意 kinfo.boot_procs 是字段名而不是这个宏，所以断言与拷贝那两行不出现。

# 7) 清理与填充的调用点
grep -n "proc_init()\|IPCF_POOL_INIT" minix3/minix/kernel/main.c
# 预期输出：2 行（proc_init 与 IPCF_POOL_INIT 相邻）

# 8) Rust 侧：表容量与宿主测试条数
grep -n "NR_PROCS: usize\|NR_SYS_PROCS: usize\|PROC_TABLE_SIZE: usize" os/kernel/src/proc_table.rs
for f in proc proc_table kpriv capability; do printf "%s.rs %s\n" $f "$(grep -c '#\[test\]' os/kernel/src/$f.rs)"; done
for f in arch/boot x86_64/boot arm64/boot riscv64/boot; do printf "os/arch/src/%s.rs %s\n" $f "$(grep -c '#\[test\]' os/arch/src/$f.rs)"; done
# 预期输出：256 / 64 / PROC_TABLE_SIZE = NR_TASKS + NR_PROCS；测试条数 43 / 41 / 39 / 15 与 14 / 14 / 5 / 4
```

---

## 7. 参见

前五篇与后续篇：

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 启动信息与内存记账（`KernelInfo` 的来源；`cut_memmap` 的用法与模块区间的归属变更）
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 高半地址空间与恒等映射（VM 装载期间"物理地址直读"的前提）
- [03-kmain-cstart.md](03-kmain-cstart.md) — `kmain` 的分段与 `cstart` 的调用序列（`proc_init` 在其中的位置；`kernel_may_alloc` 窗口）
- [04-platform-discovery.md](04-platform-discovery.md) — 平台描述符与 `KernelInfo` 的消费（`kinfo.module_list` 的上游）
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断初始化（§1.4 把进程表初始化指给本篇；本篇 §4.8 的定时器一步是它的续接）
- 后续：`08-system-init-boot-finish`（启动收尾与切用户篇，尚未重生成）、`07-cross-space-init`（跨空间访问初始化篇，尚未重生成）、`09-vm-boot-protocol`（VM 启动协议篇，尚未重生成）、`10-smp-boot`（SMP 启动篇，尚未重生成）、`11-scheduling-primitives`（调度原语篇，尚未重生成）、`12-ipc-core`（IPC 核心篇，尚未重生成）、权限管控篇（尚未重生成）、FPU 专题（尚未重生成）

C 源码：

- `minix3/minix/kernel/proc.h` — `struct proc`、16 个 `RTS_*` 位、三个操作宏、`MF_*` 位、槽位宏族
- `minix3/minix/kernel/proc.c` — `proc_init`、`set_idle_name`、`has_pending`、`mini_notify`、`mini_send`/`mini_receive`、`switch_to_user`、`isokendpt`、`notify_scheduler`、`proc_no_time`、`reset_proc_accounting`
- `minix3/minix/kernel/priv.h`、`minix3/minix/include/minix/priv.h` — `struct priv`、槽段地址宏、模板常量族
- `minix3/minix/kernel/table.c` — `image[]` 与顺序契约的头注
- `minix3/minix/kernel/main.c` — `kmain`（`NR_BOOT_MODULES` 断言与 boot 循环）、`bsp_finish_booting`
- `minix3/minix/kernel/system.c` — `get_priv`、`set_sendto_bit`、`unset_sendto_bit`、`fill_sendto_mask`、`clear_ipc`、`clear_endpoint`、`cause_sig`、`sched_proc`
- `minix3/minix/kernel/system/{do_vmctl.c,do_privctl.c,do_fork.c,do_clear.c,do_runctl.c}` — 四道抑制位的解除/置位与槽回收
- `minix3/minix/kernel/smp.c` — 跨 CPU 的 `PROC_STOP`/`VMINHIBIT` 置位与 `PREEMPTED`
- `minix3/minix/kernel/arch/i386/{protect.c,arch_system.c,memory.c,exception.c}` — `arch_boot_proc`、`libexec_pg_alloc`、`arch_proc_reset`、`arch_proc_init`、页错误入口
- `minix3/minix/kernel/arch/earm/{protect.c,exception.c}` — ARM-32 架构下的同一格
- `minix3/minix/include/minix/{const.h,param.h,com.h,endpoint.h,sys_config.h,type.h}` — 常量、容量、endpoint 宏、`struct boot_image`
- `minix3/minix/servers/vm/main.c` — `init_proc` 与 `init_vm` 的两处清单遍历

Rust 源码：

- `os/kernel/src/lib.rs` — `init_proc_and_boot`、`init_post_and_memory`、`bsp_finish_booting`、`boot_init_timer`、`push_exclusion`、`SyncUnsafeCell` 访问器
- `os/kernel/src/proc.rs` — `KProcess`、两套位定义与包装、`Priority`/`Quantum`/`CpuMask`/`SchedFields`、`KERNEL_TASKS`、`BOOT_MODULE_PROC_NRS`、fork 继承分支
- `os/kernel/src/proc_table.rs` — `ProcessTable`、`nr_to_idx`、访问原语、`rts_set`/`rts_unset`、宿主测试
- `os/kernel/src/kpriv.rs` — `KPriv` 8 子结构、`PrivTable`、`grant_capability`、`set/fill_sendto_mask`、`PrivUpdateRequest`
- `os/kernel/src/capability.rs` — `ProcessCapability`、`CapabilityTemplate`、`TrapMask`/`IpcMask`/`KCallMask`、`CapabilityError`
- `os/kernel/src/globals.rs` — `SyncUnsafeCell`、`PROC_TABLE`、`PRIV_TABLE`
- `os/arch/src/arch/boot.rs` — `ProcKind`、`EntrySpec`、`CpuContextArch`、`load_vm_elf`、`load_elf_into`、`VmLoadError`
- `os/arch/src/{x86_64,arm64,riscv64}/boot.rs` — 三架构的 `CpuContextArch` 实现与 FPU 策略字段
- `os/libs/minix-types/src/types/{endpoint.rs,com.rs,proc_nr.rs}` — `Endpoint`、`NR_TASKS`、`MAX_NR_TASKS`、`ProcNr`
- `os/libs/minix-boot/src/kernel_info.rs`、`os/boot-shim/src/loader.rs` — `NR_BOOT_MODULES` 与 `MODULE_NAMES` 的共享契约
