# 08 · 启动完成翻转：从初始化态到运行态

> 路径与标注约定（`[ARCH: …]` 等记号的解码处）全局只说一次：见 [README.md](../README.md) 的「全局阅读约定」。

> **C 真值源**：`minix3/minix/kernel/main.c`（`bsp_finish_booting`/`kmain`/`cstart`/`announce`）、`minix3/minix/kernel/system.c`（`system_init`/`kernel_call_dispatch`）、`minix3/minix/kernel/proc.h`（RTS 位族与宏）、`minix3/minix/kernel/clock.c:boot_cpu_init_timer`、`minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init`、`minix3/minix/kernel/arch/i386/arch_system.c:fpu_init`/`cpu_identify`、`minix3/minix/kernel/arch/i386/pg_utils.c`（`add_memmap` 与 `kernel_may_alloc` 的五处断言）、`minix3/minix/kernel/arch/i386/pre_init.c`（`kernel_may_alloc` 定义）、`minix3/minix/include/minix/com.h`（`VM_PROC_NR`/`NR_SYS_CALLS`）、`minix3/minix/kernel/{config.h,glo.h}`、`minix3/minix/kernel/arch/i386/{acpi.c,oxpcie.c}`、`minix3/minix/kernel/system/do_umap_remote.c`。
> **Rust 实现**：`os/kernel/src/lib.rs`（`kmain` 的 Phase A-F 与 `bsp_finish_booting`）、`os/kernel/src/syscall.rs`（`enum Syscall` 与分派）、`os/kernel/src/memmap.rs`、`os/kernel/src/vm_handoff.rs`、`os/kernel/src/proc_table.rs`、`os/kernel/src/smp.rs`、`os/kernel/src/globals.rs`、`os/kernel/src/kerninfo.rs`、`os/kernel/src/irq_manager.rs`、`os/arch/src/x86_64/cpu_identity.rs`、`os/kernel/src/{clock,proc,trap_dispatch}.rs`。
> **目标读者**：熟悉操作系统基本概念（进程、调度、页表、中断）与 C 语言的开发者；Rust 一侧读到 enum/trait 与 `unsafe` 契约大意即可。
> **前置知识**：kmain 前半与 Phase 划分（03 篇）、定时器三段语义（05 篇）、进程表与 RTS 位族（06 篇）、Direct Map 与 memory_init（07 篇）——本篇从"表都建完了"接起。
> **教学目标**：①翻转的三个开关及其次序防线（§1.1）；②kmain 尾段的十二拍全景与交接时刻的进程图景（§1.2、§1.3）；③C 侧收尾序列：system_init 装配、kernel_may_alloc 窗口、bsp_finish_booting 逐拍、smp_init 位次（§2）；④Rust 的重排与消失：分发表编译期穷尽、Phase A–F、两个全局、bootstrap 内存归宿（§3）；⑤实现走查：classify、announce/kerninfo、Step -1..9（§4）；⑥测试与诚实披露（§5）、诊断（§6）。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；本篇无 `[ARCH: ]` 标注。
> **本篇主题**：内核从"初始化态"翻转为"运行态"的那一段——三个开关（内核分配窗口关死、第一批进程交给调度器、时间开始流动）在 C 与 Rust 各自怎么拨；"分发表建立"这个启动动作在 Rust 里的消失；bootstrap 内存两套归宿。
> **前置**：[03-kmain-cstart.md](03-kmain-cstart.md)（kmain 前半与 Phase 划分）、[05-clock-interrupt-init.md](05-clock-interrupt-init.md)（定时器三段语义与 pic_init）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（进程表、RTS 位族与 boot 循环）、[07-cross-space-init.md](07-cross-space-init.md)（Direct Map 与 memory_init）。
> **后续**：handoff 页的协议面与 VM 侧消费（`09-vm-boot-protocol`（VM 启动协议篇，尚未重生成））、调度循环本体（`10-switch-to-user`（切用户与调度循环篇，尚未重生成））、分派全景与 58 项全表（`13-syscall-dispatch`（系统调用分派篇，尚未重生成））、smp_init 本体与 AP 路径（`16-smp`（SMP 篇，尚未重生成））、tick 到达后的记账消费（`17-clock-timer`（时钟与定时器运行篇，尚未重生成））。

[03-kmain-cstart.md](03-kmain-cstart.md) 的结尾，C 的 `kmain` 还在 boot image 装载循环里逐项填表；[07-cross-space-init.md](07-cross-space-init.md) 的结尾，Direct Map 的两个窗口槽已经拨出。到这里，"建表"类的工作做完了：进程表填满、权限结构就位、内存记账就绪。剩下的不是再建一张表，而是把内核从"初始化代码在跑"翻转成"进程在跑"——这个翻转由 `minix3/minix/kernel/main.c:bsp_finish_booting` 这一个函数完成，它拨动三个互相独立的开关，然后一句 `switch_to_user()` 把 CPU 交出去，再也不回来。本篇走完这段收尾：C 怎么拨、Rust 把哪些拍重排了、哪些启动期的动作在 Rust 里换了形状或干脆消失。

---

## 1. 概述

### 1.1 翻转的三个开关

"初始化态"与"运行态"的分界不是一条语句，而是三个互相独立的开关：

1. **内核分配窗口**。启动期内核可以直接划拉物理内存（给页表页、给结构体），VM 起来之后这套"直接划拉"必须停止——否则两个分配器同时记账同一批页。C 用一个全局标志 `kernel_may_alloc` 表达这个窗口：定义即开（`minix3/minix/kernel/arch/i386/pre_init.c` 第33行附近的 `int kernel_may_alloc = 1;`，声明在 `minix3/minix/kernel/glo.h`），kmain 重申一次，`bsp_finish_booting` 末尾关死。
2. **进程交接**。boot 循环把全部 17 个条目都按住（`RTS_PROC_STOP`，见 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4），解除循环在 `bsp_finish_booting` 里把这位置从 12 个服务槽上摘掉——从"全停"到"分批放行"。
3. **时间起点**。硬件定时器从 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.6 起就一直没编程——因为那次编程含一次校准，要等进程上下文可用。`bsp_finish_booting` 里调用 `boot_cpu_init_timer`（`minix3/minix/kernel/clock.c:boot_cpu_init_timer`），时间从这一拍开始流动。

三个开关各自的"前一条成立依赖"画在一起是这样的：

```
        初始化态 ──────────────────────────────► 运行态

  [开关 1] 内核分配窗口
     定义即开（pre_init 自己要用）
        │ kmain 重申 = 1
        │ ……启动期直接分配……
        │ bootstrap 回收（一次 add_memmap，要求窗口开着）
        v
     kernel_may_alloc = 0  ◄── 依赖：此后不再有内核侧直接分配
                               （VM 将成为唯一的分配者）

  [开关 2] 进程交接
     boot 循环：17 项全部 RTS_PROC_STOP
        │ （06 篇）权限结构、页表记录就位
        v
     解除循环：0..11 号服务槽摘掉 PROC_STOP  ◄── 依赖：调度函数依赖
                               的 proc_ptr/bill_ptr 已指向 idle
        │
        v
     就绪队列非空（VM 第一个入队，§1.3）

  [开关 3] 时间起点
     cstart 以来定时器一直未编程
        │ 记账基线先复位（cycles_accounting_init）
        v
     boot_cpu_init_timer(system_hz)  ◄── 依赖：记账基线已复位；
        │                                  失败出口：panic
        v
     tick 开始到达（05 篇的放行门此时才真正有输入）
```

三条链里只有"时间起点"这条有失败出口。C 的写法是：

```c
// minix3/minix/kernel/main.c:bsp_finish_booting —— 逐字引用（定时器调用与 panic 分支）
  if (boot_cpu_init_timer(system_hz)) {
	  panic("FATAL : failed to initialize timer interrupts, "
			  "cannot continue without any clock source!");
  }
```

（`minix3/minix/kernel/main.c:bsp_finish_booting`，`boot_cpu_init_timer` 返回非零即 panic。）

`boot_cpu_init_timer` 返回非零只有一种情形：定时器编程或处理函数注册失败（`minix3/minix/kernel/clock.c:boot_cpu_init_timer` 的两个 `return -1`）。panic 的措辞把原因说在明处：没有时钟源的内核无法继续。另外两个开关没有这样的运行时守卫——`kernel_may_alloc = 0` 是一次普通赋值，解除循环是一次普通循环，它们不出错的方式只有一种：调用次序本身不出错。第 2 章会看到，C 的防线就是把次序写死在一条直线上。

把两个开关的次序反过来推演，坏点都能落到具体对象上。把 `kernel_may_alloc = 0` 挪到 `switch_to_user()` 之后：VM 是 boot image 里唯一立即可调度者（§1.3 论证），它一上场就可能发 `SYS_VMCTL` 请求做分配（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`），而内核此刻还在用"直接划拉物理内存"的旧模式记账——`pg_alloc_page` 从 `kinfo.memmap` 里挑页、把页直接从记账里划走（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`），VM 侧从自己的空闲表里挑页，两边同时分配同一批页，谁也不知道对方划走了哪些。把记账基线的复位（`cycles_accounting_init`，本篇 §2.3 的第7拍）挪到定时器之后：基线与定时器编程的先后被拆开——首个 tick 到达时 tick 处理要结算"被打断段"（`minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 读每 CPU 的 TSC 基线），若基线还没建立，第一段记账把整个 TSC 计数记到当前进程头上。

### 1.2 收尾全景：kmain 尾段与十二拍

从"表都建完了"到"CPU 交出去"，中间隔着哪些拍？看 `minix3/minix/kernel/main.c:kmain` 的尾段——从 boot image 循环结束到函数末尾的全部动作：

```c
// minix3/minix/kernel/main.c:kmain —— 逐字引用（尾段：boot image 循环结束到 NOT_REACHABLE）
  memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs));

#define IPCNAME(n) { \
	assert((n) >= 0 && (n) <= IPCNO_HIGHEST); \
	assert(!ipc_call_names[n]);	\
	ipc_call_names[n] = #n; \
}

  arch_post_init();

  IPCNAME(SEND);
  IPCNAME(RECEIVE);
  IPCNAME(SENDREC);
  IPCNAME(NOTIFY);
  IPCNAME(SENDNB);
  IPCNAME(SENDA);

  /* System and processes initialization */
  memory_init();
  DEBUGEXTRA(("system_init()... "));
  system_init();
  DEBUGEXTRA(("done\n"));

  /* The bootstrap phase is over, so we can add the physical
   * memory used for it to the free list.
   */
  add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len);

#ifdef CONFIG_SMP
  if (config_no_apic) {
	  DEBUGBASIC(("APIC disabled, disables SMP, using legacy PIC\n"));
	  smp_single_cpu_fallback();
  } else if (config_no_smp) {
	  DEBUGBASIC(("SMP disabled, using legacy PIC\n"));
	  smp_single_cpu_fallback();
  } else {
	  smp_init();
	  /*
	   * if smp_init() returns it means that it failed and we try to finish
	   * single CPU booting
	   */
	  bsp_finish_booting();
  }
#else
  /* 
   * if configured for a single CPU, we are already on the kernel stack which we
   * are going to use everytime we execute kernel code. We finish booting and we
   * never return here
   */
  bsp_finish_booting();
#endif

  NOT_REACHABLE;
}
```

（`minix3/minix/kernel/main.c:kmain` 尾段。boot image 循环的结束点、`system_init` 与 `add_memmap` 的调用时刻，§2.0 的时序图会标出各自位置。）

把这段与 `bsp_finish_booting` 的函数体合起来，收尾的全景是一张十二拍的表。拍名、C 里的位置、一句话职责、依赖的前拍、以及与前三篇的回指边界：

| 拍 | C 位置 | 职责 | 依赖 | 回指边界 |
|---|---|---|---|---|
| 1 cpu_identify | `bsp_finish_booting` 首个调用 | 探明 CPU 身份填表 | CPUID 可用（03 篇） | 本篇 §2.3 |
| 2 vm_running = 0 | krandom 之前 | 声明"VM 还没起来" | 无 | 本篇 §2.3 |
| 3 krandom 两行 | 紧随其后 | 装填随机数源指针与计数 | boot 参数 | 本篇 §2.3 只记拍 |
| 4 bill/proc_ptr → idle | announce 前 | 调度函数依赖 proc_ptr 指着东西 | 进程表可寻址 | 本篇 §2.3 |
| 5 announce | bill/proc 之后 | 打印启动横幅 | 串口可用 | 本篇 §4.3（Rust 对位） |
| 6 解除循环 | announce 后 | 12 个服务槽摘 PROC_STOP | 拍 4 | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.6 |
| 7 记账基线 | 解除循环后 | 复位每 CPU TSC 记账三变量 | 无硬依赖，但先于拍 8 | 本篇 §2.3 |
| 8 定时器 | 基线复位后 | 编程+注册+放行（一次调用） | 拍 7；进程上下文已可用 | [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.6/§3.7/§4.7 |
| 9 fpu_init | 定时器后 | 探针判 FPU 存在性并设 CR0/CR4 位 | CPUID（拍 1） | 本篇 §2.3 |
| 10 DEBUG FIXME 三查 | fpu_init 后 | 调试配置下打印警告 | 编译期开关 | 本篇 §2.3 只记拍 |
| 11 SMP 标志段 | 三查后 | 置 CPU 就绪位与 machine 字段 | SMP 分支结果 | 本篇 §2.4 |
| 12 关窗 | SMP 段后 | kernel_may_alloc = 0 | VM 即将上场 | 本篇 §2.2 |
| （终）切换 | 最后一拍 | switch_to_user，不返回 | 以上全部 | `10-switch-to-user`（切用户与调度循环篇，尚未重生成） |

这张表里有一个可以先记下、§2.3 展开的分布：十二拍加一个终拍里，真正改变控制流的只有五处（解除循环、记账基线、定时器、关窗、以及终拍的切换）；三拍是填身份或探测（cpu_identify、krandom、fpu 探针）；两拍是把状态写明白（DEBUG FIXME 三查——编译期开关下的纯打印；SMP 标志段——写就绪位与 machine 字段）；三拍是为切换做的准备（vm_running = 0、bill/proc_ptr 指向 idle、announce）。一个收尾函数里，"拨开关"的代码只占一小半，另一半在让开关"敢拨"。

### 1.3 交接时刻的进程图景：谁是唯一站着的人

解除循环跑完的那一瞬间，就绪队列里有谁？回答这个问题需要两段代码。第一段是解除用的宏——它不是普通清位，清位之后还跟着一个动作：

```c
/* minix3/minix/kernel/proc.h:RTS_UNSET —— 逐字引用（宏定义与其上的注释行） */
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
```

（`minix3/minix/kernel/proc.h:RTS_UNSET`。可运行的判据是 `minix3/minix/kernel/proc.h:proc_is_runnable`——`p_rts_flags == 0` 才算可运行，这是 proc.h 位族注释里的原话。）

第二段是 boot 循环里给各槽挂 inhibit 位的豁免逻辑：

```c
/* minix3/minix/kernel/main.c:bsp_finish_booting 所在文件 kmain 的 boot 循环末尾 —— 逐字引用 */
	/* Process isn't scheduled until VM has set up a pagetable for it. */
	if(rp->p_nr != VM_PROC_NR && rp->p_nr >= 0) {
		rp->p_rts_flags |= RTS_VMINHIBIT;
		rp->p_rts_flags |= RTS_BOOTINHIBIT;
	}

	rp->p_rts_flags |= RTS_PROC_STOP;
	rp->p_rts_flags &= ~RTS_SLOT_FREE;
```

（条件读法：`p_nr < 0` 的内核 task 不挂 inhibit；`p_nr == VM_PROC_NR` 的 VM 不挂 inhibit；其余用户进程两个都挂。`VM_PROC_NR` 的值是 8，注释写明身份：`minix3/minix/include/minix/com.h:VM_PROC_NR`，`/* memory server */`。本块在 `RTS_SLOT_FREE` 清位后截断，源码紧随的 `DEBUGEXTRA(("done\n"));` 调试行省去。）

两段合起来，boot 循环结束时四类槽位的状态与解除循环的后果：

| 槽类 | boot 循环置位 | 解除循环后 | 调度后果 |
|---|---|---|---|
| 任务槽（proc 号 -5..-1，即 ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL 五个——`minix3/minix/include/minix/com.h` 里 HARDWARE 是 KERNEL 的中断别名，不是独立槽） | 仅 `RTS_PROC_STOP`（[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.6） | 不在循环内，仍停 | 永不解除：内核 task 在调用点就地执行，不进队列（06 §1.4） |
| idle 槽（任务槽之一） | `RTS_PROC_STOP` | 仍停，且是 bill_ptr 的指向目标 | 第4拍让记账先有落点 |
| VM 槽（proc 号 8） | 仅 `RTS_PROC_STOP`（豁免了两个 inhibits） | 位清零 → `p_rts_flags == 0` → `enqueue` | **当场唯一入队者** |
| 其余服务槽（0..11 中除 8） | `RTS_PROC_STOP` + `RTS_VMINHIBIT` + `RTS_BOOTINHIBIT` | 只去掉 PROC_STOP，两个 inhibits 还在 | 仍不可运行，等 VM/RS 各自摘位 |

VM 之所以是"唯一站着的人"，推理链可以完全从代码里走出来：解除循环覆盖进程号 0..11（[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4 已指出循环变量经 `proc_addr` 折算的是进程号）；这 12 个槽里，11 个挂着 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`，清掉 `RTS_PROC_STOP` 之后 `p_rts_flags` 仍非零，`RTS_UNSET` 的入队条件不成立；VM 的两个 inhibits 在 boot 循环里就被豁免了，清掉 `RTS_PROC_STOP` 后它的 `p_rts_flags` 归零，`proc_is_runnable` 成立，`enqueue` 把它链进就绪队列。这不是"逐个唤醒"，而是"一次批量放行里只有 VM 立即生效"——其余 11 个槽的解除同样执行了，只是效果要等 VM 或 RS 发对应的清除请求才兑现。

两个反向假设都有具体坏点。若删掉 boot 循环里对 VM 的豁免（让 VM 也挂 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`）：解除循环后队列空，`switch_to_user` 无进程可交，VM 要等某个中断路径顺手摘位才能上场——启动路径多出一个"谁第一个碰 VM"的隐性依赖，而且首个 tick 到达时队列里没有消费者。若把解除循环的覆盖范围扩到任务槽：任务槽的 `RTS_PROC_STOP` 是"不进就绪队列"的表达（06 §1.4 的"内核 task 永不解除"），把它摘掉等于把五个永不该被调度的槽推进队列，`pick_proc` 可能在 idle 之前选中一个没有执行上下文的任务——内核 task 的"函数体在调用点执行"这个执行模型（06 §1.4）依赖的是它们永远不被调度这个前提。

### 1.4 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| 定时器"编程、注册、放行"三段语义与 pic_init 的 8259A 分工 | 只落"第8拍这个时点 + panic 分支" | [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.6/§3.7/§4.7 |
| RTS 位族语义、boot 循环逐项填充 | 回指；本篇补调度后果（§1.3） | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4/§2.6 |
| memory_init/freepdes 拨槽 | 只回指调用时刻（§2.0 图中标注） | [07-cross-space-init.md](07-cross-space-init.md) §2.2 |
| add_memmap/cut_memmap 机制与 LIMIT 截断 | 只讲"此刻回收"的时点语义 | [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4 |
| 各 dispatch handler 本体与 58 项全表 | 只讲"表何时可用" | `13-syscall-dispatch`（系统调用分派篇，尚未重生成） |
| switch_to_user 与调度循环本体 | 指向 | `10-switch-to-user`（切用户与调度循环篇，尚未重生成） |
| tick 到达后的记账消费 | 指向 | `17-clock-timer`（时钟与定时器运行篇，尚未重生成） |
| smp_init 本体与 AP 启动路径 | 指向 | `16-smp`（SMP 篇，尚未重生成） |
| VM 侧对 handoff 页的消费 | 指向 | `09-vm-boot-protocol`（VM 启动协议篇，尚未重生成） |
| shutdown 路径（prepare_shutdown/minix_shutdown） | 不讲（无蓝图指派，归属后续系统管理篇方向） | `minix3/minix/kernel/main.c:kmain` 同文件的尾部函数 |

认知减负声明：本章不必深究 `kernel_call_finish` 里 VMSUSPEND 的挂起-恢复协议（分派篇与跨空间运行篇的辖区），也不必深究 alarm 链的运行期行为（`17-clock-timer`（时钟与定时器运行篇，尚未重生成））——本篇对分发表只要求"表已可用"这一时刻，对定时器只要求"这一拍之后 tick 会来"。

把第 1 章的图按同一框架收一次：三个开关各有一条依赖链——分配窗口的关闭依赖"此后不再有内核侧直接分配"，进程交接依赖"调度函数的指针有着落"，时间起点依赖"记账基线已复位"且是三者中唯一带失败出口的。第 2 章按 C 的调用序把每一拍落在代码上。

### 1.5 本篇的 C 侧覆盖边界

本篇的 C 侧对照物是 Minix3 的两个内核架构实现（i386 与 earm）。Rust 侧多数动作有 C 逐条可对照（§2 走查、§3.6 处置表逐行对账），少数几种形态在 C 树里没有对应物：

| Rust 侧分支 / 机制 | C 对照物 | 判定 |
|---|---|---|
| 三个开关、十二拍、SMP 三分支（§1–§2） | i386 `main.c` | **有 C 可对照**（§3.6 逐行对账） |
| A2 扣减账（`vm_handoff::classify`，§3.5、§4.2） | i386 `kmain` 尾段的一次 `add_memmap` | 有 C 可对照（改型，对照面是"一条语句"的语义） |
| `vm_running` 的补写（`set_vm_running`，§3.3） | **无**——C 从不置 1（§2.3 的"单向状态"） | **未覆盖（无 C 源）**（增量修正） |
| 分派 enum 穷尽性与 const 断言（§3.1） | i386 `call_vec` + `map` 宏（声称的防线） | 有对照面（对照物是 C"声称却不复现"的防线，§2.1） |
| B.5 kerninfo 页的独立页形态（§4.3） | i386 `.usermapped` 段 + `kuserinfo` 字段拷贝 | 部分对照（投影语义同、承载形态新） |
| aarch64 / riscv64 的 FP 策略与懒陷阱（§4.6 Step 7） | **无**——C 内核没有这两个架构 | **未覆盖（无 C 源）** |

"未覆盖"的含义：这些分支的正确性依据不是 C 对照，而是设计契约（§3）与测试（§5）。本篇对它们的陈述一律写作"无对应物"，不写作"与 C 一致"——这条纪律同时适用于后续修订中的新增段落。

---

## 2. C 源码分析

### 2.0 本章地图：kmain 尾段的时序

第 1.2 节引过的尾段代码，调用关系画成一条直线是这样的（`b.` 前缀表示 `bsp_finish_booting` 内的拍，编号沿用 §1.2 的十二拍表）：

```
 kmain 尾段（main.c）                          bsp_finish_booting（main.c）
 ─────────────────────                        ────────────────────────────
 boot image 循环结束
   │
   ├─ memcpy: kinfo.boot_procs 回写
   ├─ arch_post_init()          ──► 07 篇（保护与 DM 前置）
   ├─ IPCNAME × 6               （登记 6 个 IPC 调用名）
   ├─ memory_init()             ──► 07 篇 §2.2（拨两个窗口槽）
   ├─ system_init()             ──► §2.1（分发表装配）
   ├─ add_memmap(bootstrap)     ──► §2.2（回收引导段）
   ├─ SMP 三分支                ──► §2.4（降级或 smp_init）
   │     │
   │     v
   └─ bsp_finish_booting() ──┐
                             v
   b.1  cpu_identify         （填身份）
   b.2  vm_running = 0       （切换准备）
   b.3  krandom 两行          （填身份）
   b.4  bill/proc_ptr→idle   （切换准备）
   b.5  announce             （切换准备）
   b.6  解除循环              （控制流：开关 2）──► 06 篇（boot 循环）
   b.7  cycles_accounting_init（控制流：记账基线）
   b.8  boot_cpu_init_timer  （控制流：开关 3）──► 05 篇（定时器三段）
   b.9  fpu_init             （探针）          ──► §2.3
   b.10 DEBUG FIXME 三查     （声明状态）
   b.11 SMP 标志段           （声明状态）      ──► §2.4
   b.12 kernel_may_alloc = 0 （控制流：开关 1）──► §2.2
   （终）switch_to_user + NOT_REACHABLE        ──► 10 篇方向
```

这张图上有两处属于本章、其余属于前篇或第 3 章：`system_init` 与 `add_memmap` 的回收时点是 §2.1 与 §2.2 的本体；`bsp_finish_booting` 的逐拍走查在 §2.3，SMP 分支在 §2.4。定时器的编程本体、boot 循环的填充、Direct Map 的建立分别归 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.6、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.6、[07-cross-space-init.md](07-cross-space-init.md) §2.2，这里只标时点。

还有一个可以就地确认的形状问题：这条直线没有"阶段"结构。C 的 `kmain` 是一条带两个分支（`CONFIG_SMP` 内的三分支与 `#else` 的单核直通）的直线，从 `memcpy` 到 `NOT_REACHABLE` 顺序执行，没有任何阶段标记；"阶段"是 Rust 侧重组代码时引入的词汇（第 3 章 §3.2 会回到这一点）。

### 2.1 system_init：三段装配与 map 宏的编译期把戏

一张 58 槽的函数指针表，为什么在启动末端才装配？先把声明与宏读出来——它们是同一件事的两半：

```c
// minix3/minix/kernel/system.c —— 逐字引用（call_vec 声明及其注释、map 宏定义）
/* Declaration of the call vector that defines the mapping of system calls
 * to handler functions. The vector is initialized in sys_init() with map(),
 * which makes sure the system call numbers are ok. No space is allocated,
 * because the dummy is declared extern. If an illegal call is given, the
 * array size will be negative and this won't compile.
 */
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);

#define map(call_nr, handler) 					\
    {	int call_index = call_nr-KERNEL_CALL; 				\
    	assert(call_index >= 0 && call_index < NR_SYS_CALLS);			\
    call_vec[call_index] = (handler)  ; }
```

（`minix3/minix/kernel/system.c` 的 `call_vec` 声明与 `map` 宏。注释里写的 `sys_init()` 是函数名的笔误——装配函数的实际名字是本节标题里的 `system_init`，注释按原文引用未改。`NR_SYS_CALLS` 的值是 58：`minix3/minix/include/minix/com.h:NR_SYS_CALLS`，`/* number of kernel calls */`。`KERNEL_CALL` 是内核调用的编号基底，同为 com.h 里的常量。）

`system_init` 的函数体分三段。前两段清零两个数组，第三段逐组注册：

```c
// minix3/minix/kernel/system.c:system_init —— 逐字引用（前两段与第三段的 NULL 循环）
void system_init(void)
{
  register struct priv *sp;
  int i;

  /* Initialize IRQ handler hooks. Mark all hooks available. */
  for (i=0; i<NR_IRQ_HOOKS; i++) {
      irq_hooks[i].proc_nr_e = NONE;
  }

  /* Initialize all alarm timers for all processes. */
  for (sp=BEG_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
    tmr_inittimer(&(sp->s_alarm_timer));
  }

  /* Initialize the call vector to a safe default handler. Some system calls
   * may be disabled or nonexistant. Then explicitly map known calls to their
   * handler functions. This is done with a macro that gives a compile error
   * if an illegal call number is used. The ordering is not important here.
   */
  for (i=0; i<NR_SYS_CALLS; i++) {
      call_vec[i] = NULL;
  }
```

（`minix3/minix/kernel/system.c:system_init` 的前三段。第一段清 IRQ 钩子池，`NR_IRQ_HOOKS` 在 `minix3/minix/kernel/config.h` 里按构建形态取 16（PIC）或 64（APIC）两个分支值；第二段逐特权结构初始化报警定时器；第三段把 58 个槽全部置 NULL——注释末句顺手声明了一条契约：注册次序无关紧要。）

第三段是逐组注册。每组以一条组名注释开头，组内每行一条 `map`：

```c
// minix3/minix/kernel/system.c:system_init —— 节选（第三段开头；块内已含 Process management
// 全组与 Signal handling 的组注释行，省略号处为其余各注册组：设备 I/O、内存管理、拷贝
//（含 safe memset 小节）、时钟、系统控制、性能剖析、arm 专属、i386 专属、
// 机器状态切换与调度，全部同形，至函数结尾）
  /* Process management. */
  map(SYS_FORK, do_fork); 		/* a process forked a new process */
  map(SYS_EXEC, do_exec);		/* update process after execute */
  map(SYS_CLEAR, do_clear);		/* clean up after process exit */
  map(SYS_EXIT, do_exit);		/* a system process wants to exit */
  map(SYS_PRIVCTL, do_privctl);		/* system privileges control */
  map(SYS_TRACE, do_trace);		/* request a trace operation */
  map(SYS_SETGRANT, do_setgrant);	/* get/set own parameters */
  map(SYS_RUNCTL, do_runctl);		/* set/clear stop flag of a process */
  map(SYS_UPDATE, do_update);		/* update a process into another */
  map(SYS_STATECTL, do_statectl);	/* let a process control its state */

  /* Signal handling. */
  ...
```

把第三段的组注释逐个数出来（源文本口径，含全部条件编译分支；复算命令：`awk 'NR>=185 && NR<=270' minix3/minix/kernel/system.c | grep -c 'map(SYS_'`，输出 46）：

| 注册组（注释原文） | 条数 | 备注 |
|---|---|---|
| Process management | 10 | |
| Signal handling | 5 | |
| Device I/O | 1 + 2 条 i386 专属 | `SYS_IRQCTL` 无条件；`SYS_DEVIO`/`SYS_VDEVIO` 在 `#if defined(__i386__)` 内 |
| Memory management | 2 | |
| Copying（含 safe memset 小节） | 8 + 1 | |
| Clock functionality | 5 | |
| System control | 3 | |
| Profiling | 1 | |
| arm-specific | 1 | `SYS_PADCONF`，`#if defined(__arm__)` |
| i386-specific | 3 | `SYS_READBIOS`/`SYS_IOPENABLE`/`SYS_SDEVIO` |
| Machine state switching | 2 | |
| Scheduling | 2 | |
| 合计 | 46 | 其中无条件 40 条；i386 构建多 5 条、arm 构建多 1 条 |

现在回答"为什么在启动末端才装配"与"怎么保证没人填错格子"。装配时点的理由写在调用位置上：`system_init` 排在 `memory_init()` 之后、`add_memmap` 之前（§2.0 图）——它装配的是纯软件结构（三个数组/链），不依赖任何更早的硬件步骤，而它自己也没有任何后来者依赖：分派函数 `kernel_call_dispatch` 在启动完成后才会被调用（`minix3/minix/kernel/system.c:kernel_call_dispatch`，本篇只取"表何时可用"这个时点，分派全景归 `13-syscall-dispatch`（系统调用分派篇，尚未重生成））。填错格子的防线有两层，机制不同：

- **运行时 assert**：宏内第一行 `assert(call_index >= 0 && call_index < NR_SYS_CALLS)`。它在 `NDEBUG` 构建下整体消失——不是恒久防线。
- **声明注释声称的编译期防线**：声明注释原话 "If an illegal call is given, the array size will be negative and this won't compile."——这条断言对**现行宏形态不复现**。宏把 `call_nr-KERNEL_CALL` 先存进局部变量 `call_index`，再经 `call_vec[call_index]` 访问数组——编译器看到的是一次运行期下标，不做拒绝。本文作者用仓库环境的 gcc 做过最小实验：把 `map(SYS_FOO, ...)`（`SYS_FOO` 偏移 70，越出 58）按宏形态编译，`-c`、`-Wall`、`-O2 -Wall` 三种配置全部干净通过；把越界值直接写成常量下标 `call_vec[SYS_FOO-KERNEL_CALL]` 的旧式形态同样通过。注释描述的"数组长度变负、编译不通过"应是更早宏形状的遗留——按现码，越界注册唯一的守卫是 DEBUG 构建的 assert，release 构建下它是一次静默的越界写。Rust 侧（§3.1）的编译期守卫因此不是"同一防线的重表达"，而是把 C 声明里**声称却没有兑现**的那层防线真正做实。

还有一处只读代码才能发现的事实：`kernel_call_dispatch` 对"表在、槽空"有专门的分支——调用号合法但 `call_vec[call_nr]` 为 NULL 时，打印 `Unused kernel call` 并返回 `EBADREQUEST`（`minix3/minix/kernel/system.c:kernel_call_dispatch` 的 else 分支）。也就是说，NULL 槽不是错误状态，而是注册模型的一部分：58 个槽里只有 46 个被源文本注册（含条件分支），剩下的就是那些 `Unused`。NULL 循环（第三段开头的 `call_vec[i] = NULL`）先于全部注册执行——若没有它，合法范围内的未注册槽因 `call_vec` 是 static 数组、缺省落在 `.bss`（初值 0，等价 NULL）而看不出差异——显式清零把这条"空槽=0"的契约从存储事实升级为源文本里的声明面。

反向推演两个替换。若 `call_vec` 改成运行时 `malloc` 后填充：静态数组的边界信息从类型里消失，越界注册连"注释声称的那层防线"都失去作用对象，退化成运行期踩内存，且 no-alloc 的启动环境里 `malloc` 本身无处安放。若改成"每处理器一张表"：`system_init` 的一次装配语义被破坏——每个 CPU 都得装配一遍，或引入复制同步；C 选择全局一张表，前提是内核调用只在 BKL（大内核锁）持有时分派，全局表不会有并发装配问题（分派路径的锁纪律归 `13-syscall-dispatch`（系统调用分派篇，尚未重生成）展开）。

### 2.2 分配窗口：kernel_may_alloc 的开、用、关

这个标志的一生有三个时刻。定义时刻在 pre_init——注意初值：

```c
// minix3/minix/kernel/arch/i386/pre_init.c —— 逐字引用（定义及其上的注释行）
/* Kernel may use memory */
int kernel_may_alloc = 1;
```

（`minix3/minix/kernel/arch/i386/pre_init.c` 的定义。`minix3/minix/kernel/glo.h` 里有同名的 `EXTERN int kernel_may_alloc;` 声明。定义点初值就是 1，不是 kmain 里才打开——pre_init 自己先于 kmain 执行，它的建表调用必须看到真值。）

使用时刻散布在页表工具族里。全 i386 目录检索这个名字，命中的消费点只有五处 assert，全在 `pg_utils.c`（复算命令：`grep -n 'kernel_may_alloc' minix3/minix/kernel/arch/i386/pg_utils.c`，输出 :42/:74/:102/:143/:274 五行）：

| 消费点 | 所在函数 | 守的是什么 |
|---|---|---|
| :42 | `minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap` | 切除区间不改记账 |
| :74 | `minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest` | 从记账里挑最低帧 |
| :102 | `minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap` | 往记账里登记新区间 |
| :143 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page` | 给页表页分帧 |
| :274 | `minix3/minix/kernel/arch/i386/pg_utils.c:pg_map` | 建映射时分帧 |

五个函数合起来就是"内核直接划拉物理内存"的全部动作面：登记、切除、挑帧、建映射。窗口机制守的就是这一族操作。

关窗时刻在 `bsp_finish_booting` 倒数第二拍（§2.3 的第12拍，`main.c:105`）。而在关窗之前还有一次特殊的"用"——bootstrap 回收，它本身就是一次 `add_memmap`：

```c
// minix3/minix/kernel/main.c:kmain —— 逐字引用（回收注释与调用）
  /* The bootstrap phase is over, so we can add the physical
   * memory used for it to the free list.
   */
  add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len);
```

（`minix3/minix/kernel/main.c:kmain` 尾段。add_memmap 的机制本体——LIMIT 截断、页对齐、空槽扫描——在 [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4 已逐字教学，这里只落时点。）

窗口的三个时刻排成一条线：

```
 定义 = 1（pre_init.c:33）
    │  pre_init 的建表调用（要看到真值）
    │  kmain 重申 = 1（main.c:142，含注释 "Kernel may use bits of main memory before VM is started"）
    │  ……页表工具族五处 assert 都在这段窗口内跑……
    │  bootstrap 回收（main.c:301，一次 add_memmap）
    v
 关窗 = 0（bsp_finish_booting 第12拍，main.c:105）
    │  此后 pg_utils 全族的动作都会在 DEBUG 构建下断言失败
    v
 switch_to_user（VM 上场，成为唯一分配者）
```

两个只读代码才能发现的事实。第一，`kernel_may_alloc` 没有"读取后分支"的消费者——五处全是 assert，没有一处 `if (kernel_may_alloc) ... else ...`。标志的强制力因此是"开发期断言 + 次序纪律"：DEBUG 构建下越窗操作当场断言，release 构建下 assert 消失、什么都不拦，正确性完全靠调用序里"回收先于关窗"这句话。第二，"回收先于关窗"不只是习惯，而是硬因果——回收本身是一次 `add_memmap`（`main.c:301`），它体内的 assert（`pg_utils.c:102`）要求窗口开着；若把回收挪到关窗之后，DEBUG 构建直接断言失败，release 构建则把引导段静默记进 free list——而此刻 VM 可能已经启动并在用同一份记账，两个分配器同时记账同一批页（§1.1 的推演在这里的落点）。

### 2.3 bsp_finish_booting 逐拍：十二拍里每一拍在干什么

`minix3/minix/kernel/main.c:bsp_finish_booting` 是本篇的主走查对象。全函数分三段逐字读。

第一段（第1-5拍：探身份与切换准备——给调度函数落点）：

```c
// minix3/minix/kernel/main.c:bsp_finish_booting —— 逐字引用（函数头到 announce）
void bsp_finish_booting(void)
{
  int i;
#if SPROFILE
  sprofiling = 0;      /* we're not profiling until instructed to */
#endif /* SPROFILE */

  cpu_identify();

  vm_running = 0;
  krandom.random_sources = RANDOM_SOURCES;
  krandom.random_elements = RANDOM_ELEMENTS;

  /* MINIX is now ready. All boot image processes are on the ready queue.
   * Return to the assembly code to start running the current process. 
   */
  
  /* it should point somewhere */
  get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc);
  get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
  announce();				/* print MINIX startup banner */
```

（`minix3/minix/kernel/main.c:bsp_finish_booting` 前 21 行。`cpu_identify` 的 i386 实现在 `minix3/minix/kernel/arch/i386/arch_system.c:cpu_identify`——CPUID 叶 0/1 填每 CPU 的身份表。第4拍的注释只有一句 "it should point somewhere"——它陈述的依赖在 boot 循环里也出现过一次（[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §2.6 的"scheduling functions depend on proc_ptr pointing somewhere"）：调度函数解引用 proc_ptr，它不能是空。bill_ptr 与 proc_ptr 都指向 idle，记账与"当前在跑谁"两个问题暂时都有落点。第5拍 announce 的函数体在 §4.3 对位时逐字引。）

第二段（第6-9拍：两个开关与其准备）：

```c
// minix3/minix/kernel/main.c:bsp_finish_booting —— 逐字引用（解除循环到 fpu_init）
  /*
   * we have access to the cpu local run queue, only now schedule the processes.
   * We ignore the slots for the former kernel tasks
   */
  for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) {
	RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
  }
  /*
   * Enable timer interrupts and clock task on the boot CPU.  First reset the
   * CPU accounting values, as the timer initialization (indirectly) uses them.
   */
  cycles_accounting_init();

  if (boot_cpu_init_timer(system_hz)) {
	  panic("FATAL : failed to initialize timer interrupts, "
			  "cannot continue without any clock source!");
  }

  fpu_init();
```

（`minix3/minix/kernel/main.c:bsp_finish_booting` 中段。第7拍在先的理由写在注释前两行里："First reset the CPU accounting values, as the timer initialization (indirectly) uses them."——这只是理由的一半，另一半在读侧，本节末尾的反向推演补全。`cycles_accounting_init` 的函数体很短，值得整段读：）

```c
// minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init —— 逐字引用（全函数）
void cycles_accounting_init(void)
{
#ifdef CONFIG_SMP
	unsigned cpu = cpuid;
#endif

	read_tsc_64(get_cpu_var_ptr(cpu, tsc_ctr_switch));

	get_cpu_var(cpu, cpu_last_tsc) = 0;
	get_cpu_var(cpu, cpu_last_idle) = 0;
}
```

（`minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init`。每 CPU 三个变量：`tsc_ctr_switch` 读入当前 TSC 计数作"切换点"基线，`cpu_last_tsc` 与 `cpu_last_idle` 清零——记账从现在起算，不把 boot 期积累的计数带进第一个进程。）

第三段（第10-12拍与切换）：

```c
// minix3/minix/kernel/main.c:bsp_finish_booting —— 逐字引用（FIXME 三查到函数尾）
/* Warnings for sanity checks that take time. These warnings are printed
 * so it's a clear warning no full release should be done with them
 * enabled.
 */
#if DEBUG_SCHED_CHECK
  FIXME("DEBUG_SCHED_CHECK enabled");
#endif
#if DEBUG_VMASSERT
  FIXME("DEBUG_VMASSERT enabled");
#endif
#if DEBUG_PROC_CHECK
  FIXME("PROC check enabled");
#endif

#ifdef CONFIG_SMP
  cpu_set_flag(bsp_cpu_id, CPU_IS_READY);
  machine.processors_count = ncpus;
  machine.bsp_id = bsp_cpu_id;
#else
  machine.processors_count = 1;
  machine.bsp_id = 0;
#endif
  

  /* Kernel may no longer use bits of memory as VM will be running soon */
  kernel_may_alloc = 0;

  switch_to_user();
  NOT_REACHABLE;
}
```

（`minix3/minix/kernel/main.c:bsp_finish_booting` 末段。三查的注释把性质说得很直白：这些检查耗时，打印警告是为了让"带着它们出正式版"变得显眼——三拍都是编译期开关下的打印，不改任何状态。第11拍在 SMP 构建下置 BSP 就绪标志并回写 machine 字段，非 SMP 构建写死单核值。第12拍的注释与赋值就是 §2.2 的关窗时刻。）

fpu_init 是十二拍里唯一带"探测"性质的拍，函数体单独读：

```c
// minix3/minix/kernel/arch/i386/arch_system.c:fpu_init —— 逐字引用（探针与 CR0/CR4 设置）
void fpu_init(void)
{
	unsigned short cw, sw;

	fninit();
	sw = fnstsw();
	fnstcw(&cw);

	if((sw & 0xff) == 0 &&
	   (cw & 0x103f) == 0x3f) {
		/* We have some sort of FPU, but don't check exact model.
		 * Set CR0_NE and CR0_MP to handle fpu exceptions
		 * in native mode. */
		write_cr0(read_cr0() | CR0_MP_NE);
		get_cpulocal_var(fpu_presence) = 1;
		if(_cpufeature(_CPUF_I386_FXSR)) {
			u32_t cr4 = read_cr4() | CR4_OSFXSR; /* Enable FXSR. */

			/* OSXMMEXCPT if supported
			 * FXSR feature can be available without SSE
			 */
			if(_cpufeature(_CPUF_I386_SSE))
				cr4 |= CR4_OSXMMEXCPT; 

			write_cr4(cr4);
			osfxsr_feature = 1;
		} else {
			osfxsr_feature = 0;
		}
	} else {
		/* No FPU presents. */
		get_cpulocal_var(fpu_presence) = 0;
                osfxsr_feature = 0;
                return;
        }
}
```

（`minix3/minix/kernel/arch/i386/arch_system.c:fpu_init`。探针的原理是回读比对：`fninit` 把 x87 复位到约定状态——控制字低 6 位全 1（就近舍入、全精度、全部异常屏蔽）、状态字清零；随后 `fnstsw`/`fnstcw` 把两个寄存器读回来，若状态字低 8 位为 0 且控制字低 6 位仍是全 1（比对值 `0x3f`），说明芯片真的执行了 `fninit`，判"存在某种 FPU"。掩码 `0x103f` 里多出来的 `0x1000` 位**参与**判定——它在掩码内，而比对值 `0x3f` 的该位为 0，因此回读控制字的 bit12 必须为 0 才判"通过"（x87 控制字该位是预留位，约定态为 0；三位读者独立命中过这里的一处旧表述）。判 existence 之后按 CPUID 特征位补 CR4 的 OSFXSR/OSXMMEXCPT，注释特意提醒"FXSR 可与 SSE 分离存在"。）

十二拍对照表（拍号沿用 §1.2）：

| 拍 | 类别 | C 位置 | 读法要点 |
|---|---|---|---|
| 1 cpu_identify | 填身份 | 第一句 | CPUID 叶 0 定厂商、叶 1 定 family/model/stepping（`minix3/minix/kernel/arch/i386/arch_system.c:cpu_identify`） |
| 2 vm_running = 0 | 切换准备 | krandom 之前 | 见下面的读者表 |
| 3 krandom 两行 | 填身份 | 紧随 | 装随机数源指针与元素计数 |
| 4 bill/proc_ptr → idle | 切换准备 | announce 前 | 注释 "it should point somewhere" |
| 5 announce | 切换准备 | 第4拍后 | 版本串来自编译期宏 |
| 6 解除循环 | 控制流 | 中段 | §1.3 已展开调度后果 |
| 7 记账基线 | 控制流 | 解除后 | 先于拍 8，注释给一半理由 |
| 8 定时器 | 控制流 | 基线后 | 唯一带失败出口的拍 |
| 9 fpu_init | 填身份/探针 | 定时器后 | 回读比对，见上 |
| 10 FIXME 三查 | 声明状态 | 末段 | 纯打印，编译期开关 |
| 11 SMP 标志段 | 声明状态 | 三查后 | 就绪位与 machine 字段 |
| 12 关窗 | 控制流 | 倒数第二 | §2.2 的关窗时刻 |
| 终 切换 | 控制流 | 最后 | `switch_to_user(); NOT_REACHABLE;` |

`vm_running` 的五个读者（复算命令：`grep -rn 'vm_running' minix3/minix --include='*.c' --include='*.h' | grep -v earm`，输出 7 行：1 处写、5 处读、1 处声明）：

| 读者 | 位置 | 读它决定什么 |
|---|---|---|
| `do_umap_remote` | `minix3/minix/kernel/system/do_umap_remote.c` | safecopy 路径：`vm_running` 为真时才用 `vm_lookup_range` 查目标地址空间连续性——VM 起来前不查 |
| `oxpcie_putc` | `minix3/minix/kernel/arch/i386/oxpcie.c` | PCIe 串口的直写路径只在 `vm_running` 为真且 `oxpcie_vaddr` 已设（VM 已接管映射）时可用（oxpcie.c:52） |
| `oxpcie_in` | 同上文件 | 同上，读方向的映射决策 |
| `acpi_phys2vir` | `minix3/minix/kernel/arch/i386/acpi.c` | `!vm_running` 时物理地址直接当虚拟地址用，否则 panic |
| `acpi_phys_copy` | 同上文件 | 同上，直读硬件的守卫 |

这些读者全是"VM 起来前后行为不同"的分叉点。而本篇能确认的事实是：**`vm_running` 在全树只有一个写点（`bsp_finish_booting` 第2拍），写的是 0，从来没有任何代码把它写成 1**。五个读者读到的永远是 0——它表达的不是"VM 活着"，而是"VM 还没起来"这个单向状态，且连"起来"都不记录。这个缺口在第 3 章会作为一处修正的对照物。

三个"次序为什么不能换"的推演落到具体对象上。记账基线（拍 7）与定时器（拍 8）：注释给了写侧的一半理由（"timer initialization (indirectly) uses them"），另一半在读侧——首个 tick 到达时，`minix3/minix/kernel/arch/i386/arch_clock.c:context_stop` 用 `tsc_ctr_switch` 结算"被打断段"：若基线还是 0，第一段记账把整个 TSC 计数（从机器上电累计）全部记到当前进程头上，`cpu_last_tsc` 的差分失去意义。关窗（拍 12）与切换（终拍）：§2.2 已推演。解除（拍 6）与切换（终拍）：§1.3 已推演——队列里没有 VM 时 `switch_to_user` 无进程可交。探针（拍 9）若删掉、换成"i386 都有 FPU"的硬编码：486SX 与 386 时代的无 FPU 处理器会拿到错误的存在标志，后续上下文保存/恢复路径（`minix3/minix/kernel/arch/i386/arch_system.c:save_local_fpu` 一族）会去写不存在的寄存器；探针是硬件代际的遗物——它防的那批机器早已不在主流，但它在 C 里是真实存在的防御。

### 2.4 smp_init 的位次与单核降级

§1.2 的尾段引用里有三分支，这里把分支结构单独读。第一个分支的两个降级开关在 `cstart` 里从启动参数读来：

```c
// minix3/minix/kernel/main.c:cstart —— 逐字引用（no_apic 参数解析，USE_APIC 构建下）
#ifdef USE_APIC
  value = env_get("no_apic");
  if(value)
	config_no_apic = atoi(value);
  else
	config_no_apic = 1;
```

```c
// minix3/minix/kernel/main.c:cstart —— 逐字引用（no_apic 蕴含 no_smp 与 no_smp 解析）
#ifdef CONFIG_SMP
  if (config_no_apic)
	  config_no_smp = 1;
  value = env_get("no_smp");
  if(value)
	config_no_smp = atoi(value);
  else
	config_no_smp = 0;
```

（`minix3/minix/kernel/main.c:cstart` 的两段参数解析。第一段里 `no_apic` 缺省的 `else` 把 `config_no_apic` 置 1——**默认构建命中单核降级**（§2.4 三分支表的第一支）。第二段的蕴含行（`config_no_apic` 非零把 `config_no_smp` 预置 1）随后被 `no_smp` 解析在两条路径上改写（有参数取参数值、无参数置 0），读这两段要连读——`config_no_smp` 的独立效力只在 `no_apic=0` 时才被消费；判定主线是 `kmain` 的三分支（先判 `config_no_apic`）。另有 `USE_APIC` 未定义的构建形态：`config_no_apic` 保持声明默认、参数解析整段不出现在源里。）

三分支的对照：

| 分支 | 条件 | 动作 | 后续 |
|---|---|---|---|
| 1 | `config_no_apic` | `smp_single_cpu_fallback()` 后落入收尾 | 打印 "APIC disabled, disables SMP, using legacy PIC" |
| 2 | `config_no_smp` | `smp_single_cpu_fallback()` 后落入收尾 | 打印 "SMP disabled, using legacy PIC" |
| 3 | 两者皆非 | `smp_init()`，返回即失败 | 注释原话："if smp_init() returns it means that it failed and we try to finish single CPU booting"——失败与成功汇入同一个调用 `bsp_finish_booting()` |

第三分支读起来最不像"失败处理"：`smp_init()` 返回本身就意味着失败（成功路径不返回，AP 由它拉起），失败后不做任何补救、直接调用 `bsp_finish_booting()` 继续单核收尾。非 SMP 构建（`#else` 分支）的注释补充了另一半图景："we are already on the kernel stack which we are going to use everytime we execute kernel code"——单核形态下收尾就发生在目标栈上，收完不再返回。

所以 SMP 在这条直线上的位置是**收尾的前置增强，不是平行路径**：三条分支最后都汇到同一个 `bsp_finish_booting()`，降级后系统行为与未配置 SMP 完全一致。两个反向假设都站不住。若 `smp_init` 失败走 panic 而非降级：单核 QEMU（`-smp 1`）与 APIC 禁用这类合法配置全部无法启动——它们不是故障，是配置。若降级发生在 `bsp_finish_booting` 之后（先收尾再降级）：就绪队列已放行、定时器已编程，此时再改 CPU 拓扑意味着已入队的进程面对一个被回退的机器——降级返回的设计让"smp 失败"与"smp 未配置"汇入同一条收尾线，收尾代码对两者无感知。

### 2.5 本章归纳：收尾序列总表

把 §2.1-§2.4 的全部动作按执行序串起来。每行给位置、依赖、反序或缺失的后果、以及第 3 章接手该行的小节：

| # | 动作 | 位置 | 依赖前拍 | 若缺失或反序 | 第 3 章接手节 |
|---|---|---|---|---|---|
| 1 | boot_procs 回写 | kmain 尾段首句 | boot 循环 | boot image 布局无人记录 | [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（boot 表装配） |
| 2 | arch_post_init | 随后 | 保护结构（03 篇） | DM 前置缺失 | [07-cross-space-init.md](07-cross-space-init.md) §2.1 |
| 3 | IPCNAME × 6 | arch_post_init 之后 | 无 | IPC 调用名表缺 6 项 | §3.1（名称随 enum 取得） |
| 4 | memory_init | 随后 | arch_post_init | 窗口槽未拨 | [07-cross-space-init.md](07-cross-space-init.md) §2.2 |
| 5 | system_init | memory_init 后 | 无硬依赖 | 分发表不可用 | §3.1 |
| 6 | bootstrap 回收 | system_init 后 | 窗口开着 | 引导段泄漏或双记账 | §3.2/§3.5 |
| 7 | SMP 三分支 | 回收后 | 拓扑探测 | AP 不上线或单核降级 | §3.2 |
| 8 | bsp 十二拍 + 切换 | 分支汇合点 | 以上全部 | 见 §2.3/§2.4 逐拍推演 | §3.3/§3.4 |

数一遍这张表里的次序约束：回收先于关窗（§2.2 的 assert 因果）、记账基线先于定时器（§2.3 的读侧推演）、解除先于切换（§1.3）、SMP 分支先于收尾（§2.4）。这些约束不靠任何运行时检查——五处 assert 里只有回收那一处会在反序时触发，其余约束连 assert 都没有。C 的防线是布局本身：次序写在一条直线里，读代码的人按顺序读，写代码的人按顺序写。第 3 章的每个决策都从这张表的某一行出发。

---

## 3. Rust 设计决策

### 3.1 分发表的归宿：运行时装配 → 编译期穷尽（→§2.1）

C 的 `system_init` 在启动末端装配一张 58 槽函数指针表，声明注释声称用 `map` 宏的两层防线保证没人填错格子（§2.1 的核实结论：该防线对现行宏形态**不复现**；Rust 的回答是把防线做实——见本节的 const 断言与分派穷尽性）。Rust 对这张表的回答是：表不再在启动期"装配"——它变成一个枚举加一个 match，在编译期就把分派形状定死。模块注释开头就把对位关系声明出来：

```rust
// os/kernel/src/syscall.rs —— 逐字引用（模块注释头部）
//! Kernel system call dispatch — enum + match replaces C's call_vec[].
//!
//! # Minix3 C Source Mapping
//!
//! - `system.c:168-278` — system_init(): IRQ hook init + alarm timer init + call_vec registration
//! - `system.c:103-116` — kernel_call_dispatch(): call_vec[call_nr] dispatch
//! - `system.c:58-90` — kernel_call_finish(): VMSUSPEND handling + result copy
//! - `com.h:207-267` — SYS_* constant definitions
//! - `com.h:270` — NR_SYS_CALLS = 58
//!
//! # Design Decisions (08-system-init-boot-finish.md §3)
//!
//! - **D1**: `enum Syscall + match` replaces C's `call_vec[]` function pointer array.
//!   Benefits: type safety, compile-time exhaustiveness check, no function pointers.
//! - **D2**: `const assert` replaces C's `map()` macro runtime assert.
//! - **D6**: Architecture-specific syscalls dispatch via `ArchSyscall` trait
//!   with default `BadCall` implementations. Each arch overrides only the
//!   methods it supports (ZST impls). `CurrentArchSyscall` type alias selects
//!   the implementation via a single `#[cfg(target_arch)]` (one location),
//!   avoiding scattered `#[cfg]` behavior selection in the dispatch match.
```

（`os/kernel/src/syscall.rs` 模块注释。一段登记：注释给架构专属这条决策的编号是 D6，而 `os/kernel/src/lib.rs` 的 bsp 文档注释里 D6 指的是 vm_running——两个 D6 在源码注释里撞号；本篇按内容指认这条决策、不沿用注释编号，撞号在此登记。另注：注释的三处 C 坐标均有漂移——`system.c:168-278` 比实际函数体（168-270）多几行，`system.c:103-116`（kernel_call_dispatch 实际起于 :95）与 `system.c:58-90`（kernel_call_finish 实际 :59-93）同族，均按原文引用未改。）

枚举本体长这样——头部、空洞、尾段各看一段：

```rust
// os/kernel/src/syscall.rs:enum Syscall —— 逐字引用（枚举声明到第一处空洞注释）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    Clear = 2,
    Schedule = 3,
    Privctl = 4,
    Trace = 5,
    Kill = 6,
    Getksig = 7,
    Endksig = 8,
    Sigsend = 9,
    Sigreturn = 10,
    // 11-12: WONTFIX — reserved/unused (no SYS_* in com.h, no map() in system.c)
```

```rust
// os/kernel/src/syscall.rs:enum Syscall —— 逐字引用（尾段三处空洞注释到枚举收尾）
    Sprof = 36,
    // 37-38: WONTFIX — reserved/unused
    Stime = 39,
    Settime = 40,
    // 41-42: WONTFIX — reserved/unused
    Vmctl = 43,
    Diagctl = 44,
    Vtimer = 45,
    Runctl = 46,
    // 47-49: WONTFIX — reserved/unused
    Getmcontext = 50,
    Setmcontext = 51,
    Update = 52,
    Exit = 53,
    Schedctl = 54,
    Statectl = 55,
    Safememset = 56,
    Padconf = 57,
}
```

（`os/kernel/src/syscall.rs:enum Syscall`。判别值就是 C 的 `SYS_*` 相对 `KERNEL_CALL` 的偏移——`Fork = 0` 对 `SYS_FORK (KERNEL_CALL + 0)`，`Padconf = 57` 对 `SYS_PADCONF (KERNEL_CALL + 57)`。两段之间的变体（13..36）省略未引，空洞注释共有六段：11-12、20、29-30、37-38、41-42、47-49——12 个编号，每段注释写明理由：com.h 没有 `SYS_*` 定义、system.c 没有对应的 `map`。）

判别值越界的守卫不在运行期：

```rust
// os/kernel/src/syscall.rs —— 逐字引用（编译期断言块）
/// Compile-time verification that all syscall enum values are within [0, NR_SYS_CALLS).
///
/// C: map() macro's assert(call_index >= 0 && call_index < NR_SYS_CALLS)
/// Design decision D2: const assert replaces C's runtime assert in map() macro.
const _: () = {
    // R-16 (2026-08-12): SAFETY: `Syscall` is `#[repr(u16)]` (see enum decl
    // above), so every discriminant is stored as a u16 and `as u16` is a
    // lossless no-op that cannot truncate.
    assert!(Syscall::Fork as u16 == 0);
    assert!(Syscall::Padconf as u16 == 57);
    assert!((Syscall::Padconf as u16) < (NR_SYS_CALLS as u16));
};
```

（`os/kernel/src/syscall.rs` 的 const 断言。块内 `R-16` 是仓库内部记录编号，按原文引用。三个断言钉住的是**端点**：判别值从 0 开始、到 57 结束、最后一个变体落在 58 以内；**追加在 Padconf 之后、越出 58 的新变体不会触发这三条断言**——挡住它的是分派 `match` 的穷尽性与 `try_from` 的显式编号表（两条防线各管一段：本块钉端点、穷尽性钉全集）。）

`system_init` 三段在 Rust 里的去向各不相同。IRQ 钩子池清零进了 `IrqManager::new` 的构造（`os/kernel/src/irq_manager.rs:impl IrqManager::new`，构造时把钩子槽初始化为 NONE——与 C 的 `proc_nr_e = NONE` 循环同义），初始化时机在 Phase B（05 篇的中断管理器辖区）；报警定时器不逐特权结构初始化，改成内嵌在时钟状态里的侵入式有序链（[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §4.2 的 ClockState 结构）；注册动作本身消失——Phase E 在 `kmain` 里只剩一段注释：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（Phase E 注释块）
    // Phase E: (no boot-time action) — C `system_init()` registered the
    // syscall table (system.c:168-278 call_vec[]); in Rust the `enum
    // Syscall` + match dispatch IS the table, and the per-call work runs
    // when syscalls arrive. IrqManager (IRQ hook pool) and alarm-timer
    // initialization happen in Phase B / ClockState instead.
    // See 08-system-init-boot-finish.md §4.4
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 Phase E 注释。C 的实调用在这里是一段纯注释——没有函数、没有空语句，"阶段存在"只靠注释与前后的 boot_stage 路标维持。§4.1 的走查会从地图角度回到这个对照。）

两种装配思想的差别可以用一张表说清：

| 维度 | C：运行时装配 | Rust：编译期穷尽 |
|---|---|---|
| 表的实体 | `call_vec[58]` 函数指针数组，boot 时填 | `enum Syscall` + match，链接时定形 |
| 空洞的表达 | NULL 槽 + `Unused kernel call` 分支 | 六段显式 WONTFIX 注释，try_from 返回 Err |
| 越界注册的防线 | map 宏 assert（仅 DEBUG；声明注释声称的编译期防线在现宏形态不复现，§2.1 实验） | const 断言 + match 穷尽性检查（恒久，无构建开关） |
| "次序无关" | 注释声明（"The ordering is not important here"） | match 无次序语义——结构上不存在次序 |
| 架构专属调用 | `#if defined(__i386__)` 决定是否注册 | 枚举无条件含全部变体，不支持的架构走 `ArchSyscall` 默认实现返回 BadCall → `EBADREQUEST` |

选择这条路的取舍：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| enum + match（编译期穷尽） | 运行时注册表（模仿 C 的函数指针数组） | 分派点集中一处，新增调用改枚举加臂；失去 C 那种"模块自注册"的扩展形状 |
| const 断言 | 只靠 try_from 的运行期拒绝 | 三个断言要随枚举演化手工维护 |
| 架构变体共存 + trait 默认拒绝 | `#[cfg(target_arch)]` 裁剪变体集合 | 枚举里有本架构用不到的变体 |

最后一条的否决理由在语义上：若让 aarch64 的枚举裁掉 `Devio` 变体，调用号 21 在 aarch64 就从"拒绝"（表在、槽空、返回 `EBADREQUEST`，§2.1 的 `Unused kernel call` 分支语义）变成"非法"（调用号根本不存在，try_from 直接 Err）——C 的语义是前者。还有一个只读代码才能发现的事实：枚举显式留出六段空洞并写明理由（com.h 无 `SYS_*`、system.c 无 `map`），空洞不是遗漏，是契约的一部分——对应 C 那边"46 条注册、58 个槽、NULL 槽返回 EBADREQUEST"的模型（§2.1 的计数）。

反向推演：若保留运行时注册表模仿 C——穷尽性检查消失，漏注册退化成运行期 NULL（C 靠 NULL 循环兜底；Rust 的 no_std 启动期无堆，静态注册又退回 C 的形状）；若用 `#[cfg]` 裁剪变体——上面已经推过，"拒绝"变"非法"。

### 3.2 收尾骨架的重排：Phase A-F 与 C 直线（→§2.0、§2.2）

C 的 `kmain` 尾段是一条从 boot_procs 回写到 `NOT_REACHABLE` 的直线，次序约束全靠布局本身（§2.0、§2.5）。Rust 把这条直线重组为 Phase A-F，重组不是平移——有三处重排，每处都有机制理由。先看地图（`os/kernel/src/lib.rs` 的 `kmain` 生产版的 Phase 结构，§4.1 逐段走）：

| Phase | 内容 | 与 C 的对应 | 重排？ |
|---|---|---|---|
| A | 校验 kinfo、开分配窗口 | kmain 前段 + 开窗（`main.c:142` 的对位） | 开窗后移到 A.1b |
| A.2 | FREE_MEMMAP 装配 + boot 模块区间切除 | pre_init 的 add/cut 序列 | — |
| A.5 | 平台描述符 | cstart 的环境解析侧 | — |
| B | 保护 + 时钟 + 中断（软件半） | cstart 的三行调用（03/05 篇） | — |
| B.5 | kerninfo 页构建、映射、发布 | C 无此拍 | **增补** |
| C | 进程表 + boot 装载 | proc_init + boot 循环（06 篇） | — |
| C.5/C.6 | 收尾杂项 | 散在 boot 循环后 | — |
| D | init_post_and_memory | arch_post_init + memory_init | — |
| E | （无启动动作） | system_init | **消失**（§3.1） |
| F | 守卫回收 + init_smp_state + BKL + smp_init + bsp | bootstrap 回收 + SMP 分支 + bsp_finish_booting | **重排**（回收、BKL） |

第一处重排：开窗（C `main.c:142` 的对位）从 kmain 前段后移到 Phase A.1b——先完成 kinfo 校验与控制台就绪，再打开分配窗口；初值语义与更早阶段的防御见 §3.3。

第二处重排：bootstrap 回收从"kmain 尾"挪到 Phase F，且恒被守卫跳过。Phase F 的头注释把理由写全了：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（Phase F 头注释与守卫条件）
    // Phase F: add_memmap + bsp_finish_booting
    // C: add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)
    // D5: 4GB truncation removed for 64-bit.
    // Reclaim the bootstrap (unpaged kernel) physical memory region.
    //
    // In Minix3 C this reclaims the small identity-mapped unpaged section
    // (start..end linker symbols in kernel.lds). In the Rust port this is
    // always a no-op because the kernel is higher-half from instruction #1 —
    // boot-shim passes `bootstrap_start=PhysBytes(0), bootstrap_len=0` and
    // the guard skips reclaim. See TODO-01-1 in 01-boot-shim-bootstrap.md
    // §X for the over-reclaim bug that motivated this.
    if kernel_info.bootstrap_len() > 0 {
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 Phase F 头。块内 `D5`/`TODO-01-1` 是仓库内部编号，按原文引用；`01-boot-shim-bootstrap.md` 是引导篇的文档名。C 回收的是小段恒等映射的未分页区，Rust 从第一条指令起就是高半镜像，引导段长度恒为 0，守卫恒不进入。）

第三处重排：BKL（大内核锁）的取得点从 cstart 之后（C 的 `main.c:149`，kmain 前段、先于 boot 循环）后移到 `smp_init` 之前，一次取得、故意泄漏：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（BKL 取得注释与调用）
    // C main.c:149 parity — the BKL is acquired ONCE, early in main, before
    // smp_start_aps: the AP bring-up path (wait_for_aps) does its
    // UNLOCK→wait→LOCK dance against this acquisition, and every AP that
    // enters the kernel expects the held-BKL convention. The guard leaks
    // deliberately (transfer) — the BKL stays held until switch_to_user
    // releases it.
    crate::smp::bkl_lock().transfer();
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 BKL 段。注释里的 UNLOCK→wait→LOCK "舞蹈"指 AP 等待路径的三步——等 AP 时放锁、等待、重新取锁。）

除三处重排之外，B.5 的 kerninfo 页是 C 没有的增补——C 的用户态投影是 `.usermapped` 段加结构体字段拷贝，Rust 改成一张独立内核信息页（§4.3 展开两种投影）。

取舍表：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 回收保留调用形状、恒被守卫跳过 | 直接删掉回收分支 | 每次启动多一次长度判断；换来 boot-shim 回退时的自动恢复 |
| BKL 一次取得 + transfer 泄漏 | 在 bsp_finish_booting 里二次取锁（C 形状） | 静态检查看不出"谁负责放"——靠 Step 8.5 的断言兜底（§3.4） |
| kerninfo 页增补 | 维持 C 的 `.usermapped` 段 | VM 接管地址空间后要复制映射（kerninfo 模块注释自认的后续工作） |

第一行需要单独说清"守卫不是死代码"。`bootstrap_len > 0` 分支是活的验收面：boot-shim 若回退成"报告引导段"的形状，分支立即激活、回收真实发生；分支体对 `add_memmap` 的 Result 做了两种错误的分类处置（调试构建 panic、release 构建带注释的显式丢弃——`let _ = e;`，不是日志；`os/kernel/src/lib.rs` 的 `kmain` 的 Phase F 匹配段，§4.5 逐行走）。把它当废代码删掉，等于把这个验收面拆了。

反向推演集中在 BKL。若取得点留在 C 的位置（cstart 之后立刻取）：`init_smp_state` 与 `smp_init` 之间的拓扑装配就运行在"锁已持有"的状态下，而 AP 等待路径的第一步是 UNLOCK——等于启动中途放一次锁再取回来；Rust 把取得点放在舞蹈之前一次完成，`transfer` 泄漏守卫是"持有权转移"的类型化表达（§4.5 展开 transfer 的机制）。若回收分支不保留：见上面的取舍表第一行。

### 3.3 两个全局的保留与修正（→§2.2、§2.3）

C 的收尾段有两个全局标志——`kernel_may_alloc` 的开关窗口（§2.2）与 `vm_running` 的"从不置 1"（§2.3）。Rust 保留两者的全局语义，但各做了一处修正，也各自欠下一条诚实披露。先看存储本体：

```rust
// os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC —— 逐字引用（文档注释与定义）
/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
pub(crate) static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);
```

（`os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC`。静态初值是 **false**——与 C 的"定义即 1"（§2.2 的 `int kernel_may_alloc = 1;`）相反。）

窗口的翻转序列：`os/kernel/src/lib.rs` 的 `kmain` 的 Phase A.1b 执行 `KERNEL_MAY_ALLOC.store(true, Ordering::Release)`（开窗，对位 C 的 kmain 重申），`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 8 执行 `store(false, ...)`（关窗，对位 §2.3 第12拍）。初值语义的差异是刻意的：Rust 的启动序列比 C 多出 `arch_boot` 等更早的阶段（[02-higher-half-kernel.md](02-higher-half-kernel.md) 的启动前段），"窗口默认关"把"开窗"从存储默认升级成显式事件；当前全树没有任何"读取后分支"的消费者（§3.6 的"零运行时消费点"），这条初值目前的可观测差异为零，与 C 的差别是纯声明面的——保留它是为将来出现早阶段消费者时的安全默认。

vm_running 的保留与修正写在 Step 1 的注释里：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 1 注释与写入）
    // Step 1: vm_running = 0
    // C: vm_running = 0 — main.c:47 (declared glo.h:74)
    // Rust: global `VM_RUNNING: AtomicBool` (definition below, L2121).
    // C keeps vm_running as a plain global (glo.h:74) — the Rust mirror is
    // a global AtomicBool with identical semantics. A future per-CPU split
    // would diverge from C and needs an [ARCH] marker if ever proposed.
    VM_RUNNING.store(false, Ordering::Release);
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 1。两处按原文引用的过时内容：`definition below, L2121` 的行号指向已经漂移——`VM_RUNNING` 实际定义在 `os/kernel/src/globals.rs`，lib.rs 只有 use；注释说的"全局 AtomicBool、语义同 C"与实际代码一致。）

修正发生在 VMCTL 路径上。C 的缺口是 §2.3 确认的"从不置 1"；Rust 补上了那笔写入：

```rust
// os/kernel/src/lib.rs:fn set_vm_running —— 逐字引用（文档注释与函数体）
/// Set the `vm_running` flag.
///
/// Called by `dispatch_vmctl(VMCTL_SETADDRSPACE)` when VM completes its
/// address-space switch. C: this write is missing in Minix3 (see
/// `VM_RUNNING` doc comment for details) — Rust corrects the omission.
///
/// # Concurrency
///
/// BKL must be held by the caller (single-writer guarantee). The `Release`
/// ordering ensures the flag is visible to other CPUs after the address-space
/// switch is complete.
pub fn set_vm_running(v: bool) {
    VM_RUNNING.store(v, Ordering::Release);
}
```

（`os/kernel/src/lib.rs:fn set_vm_running`，读侧访问器是 `os/kernel/src/lib.rs:fn vm_running`。注释自称"Rust corrects the omission"。）

两条诚实披露对应 C 侧的两个观察。披露一：`os/kernel/src/lib.rs:fn bsp_finish_booting` 的文档注释里有一行写 "vm_running is CpuLocal for SMP correctness"——与 Step 1 注释（全局 AtomicBool、per-CPU 分裂才需要 [ARCH] 标注）矛盾，现码是全局，按现码写，文档注释那行登记为偏离。披露二：Rust 的 `kernel_may_alloc` **没有运行时消费点**。复算命令：

```bash
grep -rn 'KERNEL_MAY_ALLOC\|kernel_may_alloc()' os/kernel/src --include='*.rs'
```

预期关键行：命中六行——use 导入一行（lib.rs 顶部）、定义（globals.rs 的 static）、两处 store（lib.rs 的开窗与 Step 8）、访问器定义及其体内的 load（lib.rs 的 `fn kernel_may_alloc` 两行）；访问器没有任何调用者。C 的五处 assert 强制力（§2.2 的消费点表）在 Rust 侧没有对应物，只有 `os/kernel/src/memmap.rs:fn add_memmap` 与 `:fn cut_memmap` 的文档注释各保留一句"C: assert(kernel_may_alloc) in pg_utils.c"——强制力从运行时断言降级为文档约定。"零消费点"是现状事实，不是设计主张——本篇照实登记，不改代码。

两条决策的取舍：

| 全局 | 选什么 | 否掉什么 | 代价 |
|---|---|---|---|
| KERNEL_MAY_ALLOC | AtomicBool，初值 false，显式开门 | 照搬 C 的"定义即 1" | 更早阶段触碰分配路径拿到保守答案——启动窗口行为与 C 有差别 |
| VM_RUNNING | 保留全局 AtomicBool + 补 set_vm_running | 迁成 per-CPU（文档注释那行 CpuLocal 说法） | C 的五个读者语义不漂移；补写让"VM 已起来"成为可观测状态 |

反向推演两则。若照搬 C 的"定义即 1"：kmain 之前误触的分配路径拿到"允许"——Rust 的启动序列里 arch_boot 阶段比 C 的任何对应物都早，false 初值把"窗口默认关"变成安全默认。若把 vm_running 迁成 per-CPU：C 的五个读者（§2.3 的读者表）读的是"这台机器上 VM 起没起"的全局事实，按 CPU 分裂后每个 CPU 各答各的，`do_umap_remote` 在不同 CPU 上可能读到不同答案——语义漂移必须以 [ARCH] 标注并同步改全部读者，注释原文也正是这么写的。

### 3.4 bsp_finish_booting 的形状：发散、参数化、前置钩子（→§2.3）

C 的 `bsp_finish_booting` 以 `switch_to_user(); NOT_REACHABLE;` 收尾，"不返回"只靠一条注释宏表达（§2.3 第三段的引用）。Rust 把这句话变成类型：函数返回 `!`。整个函数的文档注释与签名：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（文档注释主体）
/// BSP finish booting — the last step of kmain.
///
/// C: bsp_finish_booting() in main.c:38-97
///
/// Transitions the kernel from "initialization" to "running" state:
/// 1. Set vm_running = false (VM not yet started)
/// 2. Unset RTS_PROC_STOP on boot processes
/// 3. Initialize clock timer
/// 4. Initialize FPU
/// 5. Set kernel_may_alloc = false
/// 6. Call switch_to_user() — never returns
///
/// Design decision D7 (08 §3): returns `!` to express never-returning in the type system.
/// Design decision D6 (08 §3): vm_running is CpuLocal for SMP correctness.
/// Design decision D8 (08 §3): kernel_may_alloc uses AtomicBool.
///
/// `proc_table` is the `ProcessTable` built by `init_proc_and_boot` and
/// threaded through Phase D→F. Step 2 (bill_ptr/proc_ptr=IDLE) and
/// step 4 (RTS_PROC_STOP unset) operate on it.
```

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（cfg 属性、签名与首个 use）
#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut crate::proc_table::ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::{ProcNr, RtsFlagsBits, proc_nr};
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting`。两处登记：其一，文档注释有一处按原文引用的过时内容——`main.c:38-97` 的区间比实际函数（38-109）短；三行 Design decision 编号里 D6 那行的"CpuLocal"说法与现码矛盾（§3.3 的披露一），D7/D8 与现码一致。其二，这一大段 `///` 注释在源码里**没有空行分隔地续入了下一函数的注释**（"Program the boot clock source…"起，整块由 rustdoc 归属给 `os/kernel/src/lib.rs:fn boot_init_timer`），`fn bsp_finish_booting` 现码没有自己的文档注释——引文逐字为真，"它属于 bsp"这个归属是粘连前的历史形状，如实登记。签名两个参数的分工写在注释末段：`&mut ProcessTable` 服务 Step 2 与 Step 4，`&mut SmpState` 服务 Step 5 与 Step 7——签名即依赖清单，函数体不读参数之外的任何表。mock 构建下整个函数不编译，§5.2 的测试模式由此而来。）

发散类型的第一个后果是调用点形态：`kmain` 的 Phase F 末尾直接以 `bsp_finish_booting(...)` 作为尾表达式，不需要 `NOT_REACHABLE` 之类的哨兵——`!` 能赋给任何返回类型，编译器替注释宏做了全部工作。第二个后果是可测试性：发散函数不能整体调用（调用即不返回），副作用必须可拆出来单独驱动——§5.2 的两条测试就是这个模式的实例。

Rust 还给函数体加了 C 没有的两拍。第一拍在最前：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step -1）
    // Step -1: register the kernel panic diagnostic (D-48, C
    // utility.c:22-50) — as early as the diagnostic context (EarlyConsole,
    // SMP_STATE, util_stacktrace) is usable. Panics before this point fall
    // back to minix-rt's stage-1 sink path (message only).
    register_panic_diagnostic();
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step -1，注册函数是 `os/kernel/src/lib.rs:fn register_panic_diagnostic`。块内 `D-48` 是仓库内部编号，按原文引用。C 的 panic 处理器常驻内核（utility.c），Rust 的 panic 处理在 minix-rt 运行支持里、栈走查依赖 EarlyConsole 与 SMP 状态——这些要等启动推进到一定程度才可用，所以钩子有"注册时点"问题，C 没有。）

第二拍在 Step 8 与 Step 9 之间：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 8.5）
    // Step 8.5: BKL held (S-5 realignment).
    // C: BKL_LOCK() — main.c:149, early in main(). The acquisition moved to
    // kmain before smp_init (this function's only caller): wait_for_aps's
    // UNLOCK→wait→LOCK dance ends with the BKL held, so re-acquiring here
    // (the pre-S-5 single-CPU arrangement) would deadlock the non-reentrant
    // CAS lock. Debug-pinned so a future second caller cannot skip the
    // acquisition silently.
    debug_assert!(smp::bkl_is_locked(), "bsp_finish_booting requires the BKL (kmain acquires it before smp_init)");
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 8.5。块内 `S-5` 是仓库内部编号，按原文引用。断言钉的不是"要不要取锁"而是"锁必须已经在手上"——取锁动作在 `kmain` 里完成（§3.2 的第三处重排），这里只验证。）

记账落点那一步（对位 C 的第4拍）的账记在进程表上，`os/kernel/src/proc.rs` 的方法文档注释把对位关系写了出来：

```rust
// os/kernel/src/proc.rs（Accounting 的记账目标方法）—— 逐字引用（文档注释头部）
    /// Mark this process as the current billable target (IDLE-side).
    ///
    /// Called from `bsp_finish_booting` step 2 (Doc 08 §3) to indicate
    /// that until a real user process runs, accumulated time should be
    /// billed to the IDLE kernel task.
    ///
    /// C: `get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc)` —
    /// main.c:50.
```

（`os/kernel/src/proc.rs` 的 Accounting 方法注释。按原文引用的坐标 `main.c:50` 已漂移——C 的两行赋值实际在 `main.c` 第56-57行（§2.3 第一段的引用），登记。）

取舍表：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| `-> !` 发散类型 | 注释宏 NOT_REACHABLE | mock 构建下函数不编译，测试要走副作用直驱（§5.2） |
| 参数化（表与 SMP 状态显式传入） | 函数体直读全局 | 调用链要多穿两个参数 |
| Step -1 前置钩子注册 | 依赖 minix-rt 的 stage-1 消息兜底 | 注册之前的 panic 只有消息没有栈（§6.1 的诊断分界） |
| Step 8.5 断言锁在手上 | 在此处二次取锁（C 形状） | release 构建下断言消失，锁约定靠纪律 |

反向推演两则。若 Step 8.5 保留取锁（注释里"single-CPU arrangement"的旧形状）：`kmain` 已持有的非重入 CAS 锁被二次获取，BSP 死锁在启动末端——注释以条件式给出这个后果（"would deadlock the non-reentrant CAS lock"），对齐后只留断言。若 panic 钩子注册早于 EarlyConsole 可用：钩子往还没初始化的串口上写，启动失败时诊断输出本身成为第二故障源——所以注释把时点约束写成"as early as the diagnostic context is usable"。Step -1 排在 Step 0（CPU 身份探测）之前也是同一个道理：探测自身可能触发诊断，钩子必须先在位。

### 3.5 bootstrap 内存的最终归宿：一条 add_memmap vs 一本扣减账（→§2.2）

C 的 bootstrap 回收是 kmain 尾段里一条 `add_memmap` 语句——把引导段登记回 free list 就完事（§2.2）。Rust 没有引导段可还（高半镜像，§3.2 的恒 no-op 守卫），但 bootstrap 消耗掉的资源——页表层级、ELF 装载帧、栈帧——照样要从 VM 的空闲内存里扣除。这笔账的记法换了一个位置：不在 kmain 尾部，而在 A2 分类点，也就是 VM 的启动协议页构建时。模块注释开篇把结构说全：

```rust
// os/kernel/src/vm_handoff.rs —— 节选（模块注释主体；两处省略号各代设计文档的内部节号记号，其余逐字）
//! VM boot handoff construction — A2 post-bootstrap classification.
//!
//! At the classification point (all bootstrap allocations complete: DM
//! coverage page-table pages, PT hierarchy, ELF segments, stacks, handoff
//! frame) the kernel finalizes VM's free list from the **full** memmap by
//! cutting `LiveBootstrap(t_classify)` — bootstrap-used (① PT hierarchy,
//! ② ELF backing, ③ stack/handoff frames) ∪ kernel-reserved (kernel image,
//! boot bump region, reserved module blobs). The deduction record itself
//! is handed over in the same page, so VM can verify
//! `LiveBootstrap ∩ VM-free = ∅` at the consumer boundary
//! (`07-paging_init_design` …, Proof 2) instead of trusting the
//! kernel's bookkeeping.
//!
//! # Deduction ranges are the record
//!
//! Every range is an enumerable choke point (… provenance audit):
//!
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base()/kern_size()` |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs | `KernelInfo.boot_modules()` minus the VM
//!   module (reclaimed after ELF copy, C: protect.c:450-451) |
//!
//! The free list is then clipped to VM's Direct Map window
//! (`VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap`).
//!
//! All storage is fixed-size — boot is zero-heap.
```

（`os/kernel/src/vm_handoff.rs` 模块注释。这段注释的表里有一行与现码不符：`reserved module blobs` 写"minus the VM module"，而函数体（下面引用）对全部 boot 模块 blob 做全量扣减、含 VM 自己——按现码写，登记为注释偏离。`07-paging_init_design` 是设计文档名，其中反复出现的 6.0-A2 是那份文档的内部节号，按原文引用。）

扣减的五个来源（按模块注释的表逐源落实，最后一源按现码修正）：内核镜像、VM 的自根页、boot bump 区间（**全量**——注释原话"consumed pages and remainder are both kernel-reserved"，没用完的尾页也算内核保留）、bootstrap 已用帧（每个池区间从末端往下走，已用部分是连续后缀）、全部 boot 模块 blob——含 VM 自己。最后一源为什么必须含 VM 自己，函数体里的取证注释给出过完整的因果链，引它的现文（省去一行以仓库内部编号开头的裁决行，该行说明的裁决内容即后随注释本身）：

```rust
// os/kernel/src/vm_handoff.rs:fn classify —— 节选（VM blob 全量扣减的取证注释与循环；
// 省略号处为注释首行，行首是仓库内部编号，其陈述的裁决即"全量保留"）
    // Kernel-reserved: every boot module blob INCLUDING VM's own.
    // ...
    // 但 kernel 的 boot-exec 在 VM 启动前逐台装载服务器 0..7——它们的段
    // 帧从本 free list 取，VM blob 被取走即 VM 镜像启动前被覆写（gh53/
    // 58：VM text 页非法指令→SIGSEGV-for-itself，p_fault_addr=None 非
    // PF）。625KB 的让利不值得换启动前腐坏：全量保留。
    assert!(
        vm_module_idx < kernel_info.boot_modules().len(),
        "vm_handoff: VM module index {} out of bounds",
        vm_module_idx
    );
    for module in kernel_info.boot_modules().iter() {
        record.push(module.start.0, module.len as u64);
    }
```

（`os/kernel/src/vm_handoff.rs:fn classify`。这段注释记录的因果链：早期版本回收 VM 的 blob 给 exec 腾帧，而内核的 boot-exec 在 VM 启动前装载前八个服务器、从同一张空闲表取帧——VM 的 text 页被取走覆写，VM 首跑即非法指令，页表完好无缺。故障面在"谁先拿帧"这种时序耦合上，注释里的 `gh53/58` 是当时的问题编号，按原文引用。）

扣减与发放的交付形状是"账随页走"：A2 空闲表与扣减记录按构造不相交——它们是同一批 `cut_memmap` 的两端（记录端被切走、幸存端留下来），两者连同启动表一起装进 handoff 页，VM 在自己的入口可以核对 `LiveBootstrap ∩ VM-free = ∅`，不必信任内核的记账。发布点是一个写一次的静态加配对序：

```rust
// os/kernel/src/vm_handoff.rs:fn build_vm_handoff —— 节选（A2 空闲表发布段）
    // Publish the A2 free list for post-first-load frame consumers (see
    // [`VM_PMM_FREE`]) — write-once at the classification point, before
    // any multi-image loader runs.
    {
        // SAFETY: write-once-then-read contract (see `VM_PMM_FREE`).
        let slot = unsafe { &mut *VM_PMM_FREE.get() };
        for (i, region) in classification.free_regions.iter().enumerate() {
            slot[i] = *region;
        }
        VM_PMM_FREE_COUNT.store(classification.free_region_count, Ordering::Release);
    }
```

（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff` 的发布段。计数用 `Ordering::Release` 发布，读侧 `os/kernel/src/vm_handoff.rs:static VM_PMM_FREE` 的读取函数以 `Ordering::Acquire` 配对——写一次、读多次的发布对。）

取舍表：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| A2 分类点集中记账（Phase C 末一次） | 沿 C 形状在 kmain 尾还账 | 记账逻辑集中在分类函数，与启动协议页同点交付 |
| 扣减账随页交付（可验证凭证） | 只发空闲表（消费者信任内核） | handoff 页要多装一份记录数组 |
| VM blob 全量扣减 | 回收 VM blob 给 exec 腾帧 | 625KB 物理内存以内核保留名义锁死到 VM 接管 |

反向推演两则。若恢复回收 VM blob 的形状：boot-exec 装服务器取走 VM 的 text 帧，VM 首跑非法指令而页表完好——取证注释的原话形状，故障在时序耦合上，与"谁分配正确"无关。若只发空闲表不发扣减账：VM 无法验证收到的帧不含 bootstrap 脚印，"凭证"退化成"信任"——同一页里两份数据的构造性不相交是验证之所以可能的前提。

### 3.6 处置表：C 收尾段的每件东西去了哪（→第 2 章各节）

第 3 章的决策到这里齐了，把第 2 章见过的每件东西逐项对账。去向分五档：保留同位（形状相同）、改型（语义保留、机制换）、增补（C 没有）、移位（存活但换到启动的另一阶段）、消失（无对应物）：

| C 项 | C 锚点 | 去向 | Rust 锚点 |
|---|---|---|---|
| IRQ 钩子池清零 | `minix3/minix/kernel/system.c:system_init` 第一段 | 改型：构造时清零 | `os/kernel/src/irq_manager.rs:impl IrqManager::new` |
| 报警定时器逐 priv 初始化 | 同上第二段 | 改型：内嵌侵入式链 | `os/kernel/src/clock.rs` 的时钟状态（05 篇 §4.2） |
| call_vec 注册 | 同上第三段 | 消失：enum + match 即表 | `os/kernel/src/syscall.rs:enum Syscall` |
| map 宏双层防线 | `minix3/minix/kernel/system.c:map` | 改型：const 断言 | `os/kernel/src/syscall.rs` 的 const 断言块 |
| "表在槽空 → EBADREQUEST" | `minix3/minix/kernel/system.c:kernel_call_dispatch` | 保留同位：WONTFIX 空洞 → BadCall | `os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner` |
| kernel_may_alloc 五处 assert | `minix3/minix/kernel/arch/i386/pg_utils.c` 五函数 | 改型：零运行时消费点，两条文档注释 | `os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC` |
| vm_running 写 0、从不置 1 | `minix3/minix/kernel/main.c:bsp_finish_booting` 第2拍 | 保留同位 + 增补 set_vm_running | `os/kernel/src/lib.rs:fn set_vm_running` |
| bootstrap 回收一条语句 | `minix3/minix/kernel/main.c:kmain` 尾段 | 改型：A2 分类点扣减账 | `os/kernel/src/vm_handoff.rs:fn classify` |
| cpu_identify | `minix3/minix/kernel/arch/i386/arch_system.c:cpu_identify` | 改型：arch 探针 + 内核侧表，顺带修一处 C 的 model 解码错误 | `os/kernel/src/smp.rs:fn cpu_identify`、`os/arch/src/x86_64/cpu_identity.rs:fn decode_signature` |
| vm_running = 0 | 同上第2拍 | 保留同位 | `os/kernel/src/lib.rs:fn bsp_finish_booting` Step 1 |
| krandom 两行 | 同上第3拍 | 移位：归入启动更早阶段的随机源语境 | （06 篇 Phase C.6 语境） |
| bill/proc_ptr → idle | 同上第4拍 | 改型：记账标志位 + 调度侧消费 | `os/kernel/src/proc_table.rs:fn set_bill_to_idle` |
| announce | 同上第5拍 | 改型：一行横幅 + kerninfo 页 | 同上 Step 3、`os/kernel/src/kerninfo.rs:fn init_kerninfo` |
| 解除循环 | 同上第6拍 | 保留同位：rts_unset 自动入队 | 同上 Step 4 |
| cycles_accounting_init | `minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init` | 改型：note_context_switch 记基线 | 同上 Step 5 |
| boot_cpu_init_timer + panic | `minix3/minix/kernel/clock.c:boot_cpu_init_timer` | 保留同位：三段序独立成函数 | `os/kernel/src/lib.rs:fn boot_init_timer` |
| fpu_init 探针 | `minix3/minix/kernel/arch/i386/arch_system.c:fpu_init` | 改型：存在性直置 true（探针删除） | 同上 Step 7 |
| DEBUG FIXME 三查 | 同上第10拍 | 消失：无对位 | — |
| SMP 标志段 | 同上第11拍 | 改型：SmpState 就绪态 + 拓扑查询 | `os/kernel/src/lib.rs:fn init_smp_state` |
| 关窗 | 同上第12拍 | 保留同位：Step 8 | 同上 Step 8 |
| smp 三分支与降级 | `minix3/minix/kernel/main.c:kmain` SMP 段 | 改型：降级语义内嵌进 smp_init | `os/kernel/src/smp.rs:fn smp_init` 的调用约定 |
| switch + NOT_REACHABLE | 同上终拍 | 改型：`!` 类型吸收哨兵 | `os/kernel/src/lib.rs:fn switch_to_user` |
| 4GB 截断（LIMIT） | `minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap` | 消失：随 64 位地址空间失去对象；捎带裁掉 mmap_size/mem_high_phys 两个 C 字段（os/kernel/src/memmap.rs 模块注释的核销记录） | `os/kernel/src/memmap.rs:fn add_memmap` |

数一遍消失项：整行消失的两个——注册动作（enum 取代）、DEBUG FIXME 三查（无对位）；4GB 截断是第三个，它随 64 位失去对象，两个 C 字段以设计 no-op 的形式留在 Rust 侧（表中同标"消失"，"半个"的说法即指这层区别）。其余全部保留或改型。这张表的反向读法是恢复实验：对每个消失项问"恢复它需要哪些配套"——恢复注册表需要堆或静态注册机构，还得给穷尽性检查找替代品；恢复三查需要三套调试检查的实现加编译开关；恢复截断需要回答"64 位下截到哪里"——每个都比删掉它时的理由更重。

---

## 4. 实现详解

### 4.1 kmain 的 Phase A-F 骨架与全链时序（→§3.2）

第 3.2 节定了"三处重排"的决策，本节把生产版 `kmain` 的骨架与 C 直线的对齐关系铺开。Phase 清单写在函数的文档注释里：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（Phase 清单文档注释与生产版签名）
/// Phase A (this function): Entry — validate kinfo, allow kernel alloc
/// Phase B: cstart — prot_init + clock + intr (software only; D-59) + arch_init
/// Phase C: proc_init + arch_boot_proc
/// Phase D: arch_post_init + memory_init
/// Phase E: (no boot-time action — C `system_init` became the `enum Syscall`
///          dispatch; per-call work runs when syscalls arrive)
/// Phase F: bsp_finish_booting (timer program+register+gate, D-59) + switch_to_user
///
/// C: main.c:115-147
#[cfg(all(not(feature = "mock"), not(feature = "qemu_test")))]
pub fn kmain(kernel_info: &KernelInfo) -> ! {
```

（`os/kernel/src/lib.rs` 的 `kmain` 生产版的文档注释头。块内两处 `D-59` 是仓库内部编号，按原文引用。同名函数还有一个 `#[cfg(feature = "qemu_test")]` 的裸函数包装（`os/kernel/src/lib.rs` 里 `pub extern "C" fn kmain`），它只转发寄存器现场给校验函数，不在本篇走查范围。另一处登记：块内 `/// C: main.c:115-147` 是**源注释原文**——按 C 源核对，`kmain` 起于 115 而远长于 147 行，本篇正文用到的分拍坐标（142/149/301/311）均在其中；源注释的区间待回修，此处按原文引用并登记。）

C 直线与 Rust Phase 的全链对齐（三处重排用 ◄ 标出，与 §3.2 的表一一对应）：

```
 C kmain（一条直线）                      Rust kmain（Phase A-F）
 ────────────────────                    ─────────────────────────
 kmain 入口（kinfo 落地）        ──────►  Phase A    validate + console
 kernel_may_alloc = 1     ◄─重排1─       Phase A.1b  开窗（store true）
 pre_init 的建表序列      （前移）       Phase A.2   FREE_MEMMAP + 模块切除
 cstart：prot_init/clock/intr   ──────►  Phase B     保护+时钟+中断（软件半）
 cstart 内 kinfo/kuserinfo 填充 ◄─增补─  Phase B.5   kerninfo 页
 proc_init + boot 循环          ──────►  Phase C     进程表 + boot 装载
 boot 循环后的杂项              ──────►  Phase C.5/C.6
 arch_post_init + memory_init   ──────►  Phase D     init_post_and_memory
 system_init                    ──────►  Phase E     （纯注释，§3.1）
 bootstrap 回收          ◄─重排2─       Phase F 前段  守卫回收（恒跳过）
 BKL_LOCK（尾段之后）     ◄─重排3─       Phase F      init_smp_state → bkl transfer
 SMP 三分支                     ──────►  Phase F      smp_init（降级内嵌）
 bsp_finish_booting             ──────►  Phase F 末   bsp_finish_booting(proc, smp)
```

三处重排的机制理由都在第 3 章：重排 1（开窗后移）与更早阶段的安全默认，见 §3.3；重排 2（回收挪到 Phase F 且恒跳过）见 §3.2；重排 3（BKL 后移到 smp_init 前）见 §3.2。B.5 的增补见 §3.2 与 §4.3。

还有一个只在实现层看得清的形状：Phase E 在源码里是纯注释——没有函数调用、没有空语句，"阶段存在"只靠注释与前后两条 `boot_stage!("kernel: kmain ... ok\n")` 串口路标维持（§3.1 引过注释原文）。这与 C 形成最直接的对照：C 的 `system_init` 是一次真实调用（§2.1 的三段装配），Rust 的同位置什么也不执行——"表可用"的时刻从启动期搬到了链接期。

### 4.2 A2 分类点走查：`vm_handoff::classify`（→§3.5）

第 3.5 节定了"五源扣减账"的决策，本节把 `os/kernel/src/vm_handoff.rs:fn classify` 逐段走完。先看两个数据结构——扣减记录与它的收集器：

```rust
// os/kernel/src/vm_handoff.rs:struct Classification 与 :struct DeductionCollector —— 逐字引用
/// A2 classification output — the free list and the deduction record.
///
/// The record is what the kernel cut from the memmap; the free list is
/// what survived (clipped to VM's Direct Map window). The two are disjoint
/// by construction (`cut_memmap` on the recorded ranges).
pub struct Classification {
    /// Surviving free ranges, page-aligned, DM-window-clipped.
    pub free_regions: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS],
    pub free_region_count: usize,
    /// `LiveBootstrap(t_classify)` — the ranges cut from the memmap.
    pub deducted: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    pub deducted_count: usize,
}

/// Fixed-capacity collector for deduction ranges (fail-fast on overflow).
struct DeductionCollector {
    ranges: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    count: usize,
}

impl DeductionCollector {
    const fn new() -> Self {
        Self {
            ranges: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_DEDUCTED],
            count: 0,
        }
    }

    /// Records one page-rounded deduction range.
    ///
    /// `start` must be page-aligned (every choke-point source is); `len`
    /// is rounded up to the page the way [`cut_memmap`] rounds its cuts,
    /// so the record is exactly the range removed from the memmap.
    fn push(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        assert!(
            start & (PAGE_SIZE - 1) == 0,
            "vm_handoff: deduction start 0x{start:x} must be page-aligned"
        );
        let size = len.div_ceil(PAGE_SIZE) * PAGE_SIZE;
        assert!(
            self.count < VM_BOOT_HANDOFF_MAX_DEDUCTED,
            "vm_handoff: deduction record overflow — LiveBootstrap is not enumerable within capacity"
        );
        self.ranges[self.count] = HandoffMemRegion { base: start, size };
        self.count += 1;
    }
}
```

（`os/kernel/src/vm_handoff.rs:struct Classification` 与 `:struct DeductionCollector`。push 的两个断言就是"可枚举"的执行形态：起点必须页对齐（每个扣减源都是页对齐的 choke point），容量溢出立即 panic 而不是静默丢弃——账少记一笔，VM 就会把一段内核保留的内存当空闲。）

函数本体按执行序走。第一段：memmap 快照与前三源：

```rust
// os/kernel/src/vm_handoff.rs:fn classify —— 节选（文档注释、签名、快照、前三源 push；两处省略号各代设计文档的内部节号记号，其余逐字）
/// Runs the A2 classification.
///
/// `vm_module_idx` is the boot-module index of VM itself — its blob was
/// reclaimed after the ELF copy (C: protect.c:450-451 `add_memmap` +
/// `mod_start = mod_end = 0`), so it stays free and is not deducted.
///
/// The UEFI boot path allocates the root page and bump region as
/// `LOADER_DATA`, which never appears in the conventional memmap — cutting
/// them is a no-op there. The no-shim fallback path registers its bump
/// region *from* the memmap, and the cut is what keeps it out of VM's free
/// list. Either way the whole registered range is deducted (kernel-reserved
/// includes the bump remainder, …).
pub fn classify(
    kernel_info: &KernelInfo,
    root_paddr: PhysBytes,
    vm_alloc: &VmBootAllocator,
    vm_module_idx: usize,
) -> Classification {
    // Snapshot the full memmap into a local working copy (C: pre_init
    // operates on kinfo's own memmap copy). MAXMEMMAP bounds it; the
    // boot-shim cannot report more.
    let mut mmap = [MemMapEntry::default(); MAXMEMMAP];
    for (i, region) in kernel_info.memmap().iter().enumerate() {
        assert!(
            i < MAXMEMMAP,
            "vm_handoff: boot-shim memmap exceeds MAXMEMMAP"
        );
        if region.len > 0 {
            mmap[i] = MemMapEntry {
                base: region.base.0,
                length: region.len as u64,
            };
        }
    }

    // ── Build the LiveBootstrap(t_classify) record ──
    let mut record = DeductionCollector::new();

    // Kernel-reserved: kernel image (C: pre_init.c:196-199 treats the
    // kernel as an extra module and cuts it the same way).
    record.push(kernel_info.kern_phys_base().0, kernel_info.kern_size());

    // Kernel-reserved: VM's self root page (A1 identity — the handoff value
    // itself, … "root page（handoff 值本身）").
    record.push(root_paddr.0, PAGE_SIZE);

    // Kernel-reserved: the boot bump region, whole range.
    if let Some((base, end)) = boot_alloc_region() {
        record.push(base, end.saturating_sub(base));
    }
```

（`os/kernel/src/vm_handoff.rs:fn classify`。文档注释第一段有一处按原文引用的过时陈述："its blob was reclaimed … so it stays free and is not deducted"与函数体对全部模块 blob 全量扣减的现行为矛盾（§3.5 已登记，此处是同族第二处）——注释描述的是回收 VM blob 的形状，现码不是。文档注释第二段就是两种启动形态的说明：UEFI 路径的根页与 bump 区间是 LOADER_DATA、根本不在 conventional memmap 里，切它是 no-op；无 shim 回退路径的 bump 区间才真正从 memmap 切掉——同一段代码服务两种形态，靠 cut 的幂等性自然吸收差异。）

第二段：bootstrap 已用帧的后缀扣减（第四源；第五源即 §3.5 引过的 VM blob 段）：

```rust
// os/kernel/src/vm_handoff.rs:fn classify —— 逐字引用（已用帧后缀）
    // Bootstrap-used ②③: frames handed out by the VM bootstrap allocator.
    // The allocator walks each pool region from `end` down to `start`, so
    // the consumed part of every touched region is a contiguous suffix:
    // regions before `idx` are fully consumed, region `idx` contributes
    // `[cursor, end)`, later regions nothing.
    let pool = vm_alloc.regions();
    let idx = vm_alloc.current_region_idx();
    let cursor = vm_alloc.cursor().0;
    for i in 0..pool.len().min(idx + 1) {
        let region = pool[i];
        let used_start = if i < idx {
            region.start.0
        } else {
            cursor.max(region.start.0)
        };
        if used_start < region.end.0 {
            record.push(used_start, region.end.0 - used_start);
        }
    }
```

（`os/kernel/src/vm_handoff.rs:fn classify`。后缀形状来自分配器的分配方向：从区间末端往下走，所以每个被碰过的区间，已用部分是连续后缀——游标之前的区间全消耗、游标所在区间出一段、之后的没动过。）

第三段：整批切与窗口裁剪：

```rust
// os/kernel/src/vm_handoff.rs:fn classify —— 节选（cut 循环与 DM 窗口裁剪；一处省略号代设计文档的内部节号记号，其余逐字）
    // ── Cut the record out of the memmap (same-family exclusion as
    //    `VmBootRegion::select_multi` — frame.rs invariant 1) ──
    for range in &record.ranges[..record.count] {
        // Every recorded range is page-aligned by construction.
        cut_memmap(&mut mmap, range.base, range.size)
            .expect("vm_handoff: cut_memmap failed — memmap slot exhaustion");
    }

    // ── Clip survivors to VM's Direct Map window ──
    // VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap
    // (…): a region the VM DM window cannot represent cannot be
    // accessed through vm_phys_to_virt, so it must not enter the free list.
    let window_end = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;
    let mut free_regions = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS];
    let mut free_count = 0usize;
    for entry in mmap.iter().take(MAXMEMMAP) {
        if entry.is_empty() {
            continue;
        }
        let base = entry.base;
        let end = entry.base.saturating_add(entry.length);
        let clipped_end = end.min(window_end);
        if base >= clipped_end {
            continue; // entirely outside the window
        }
        assert!(
            free_count < VM_BOOT_HANDOFF_MAX_REGIONS,
            "vm_handoff: free-region list overflow — raise VM_BOOT_HANDOFF_MAX_REGIONS"
        );
        free_regions[free_count] = HandoffMemRegion {
            base,
            size: clipped_end - base,
        };
        free_count += 1;
    }
```

（`os/kernel/src/vm_handoff.rs:fn classify`。切是"先记满再切"：循环里只有 `cut_memmap` 加 expect，没有对 memmap 的查询——记录阶段与动作阶段解耦，本节末段的反向推演就落在这一点上。裁剪把幸存区按 VM 的 Direct Map 窗口端点截断：窗口表示不了的物理区间翻译不成 VM 可用的虚拟地址，进了空闲表也是死帧。）

第四段：可见性打印（省去其上四行说明打印动机的注释，那段注释含仓库内部编号；打印本体逐字）：

```rust
// os/kernel/src/vm_handoff.rs:fn classify —— 逐字引用（幸存区可见性打印）
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str("kernel: vm_handoff free n=");
        Console::write_hex(free_count as u64);
        Console::write_str(" deducted=");
        Console::write_hex(record.count as u64);
        Console::write_str("\n");
        for r in free_regions.iter().take(free_count).take(8) {
            Console::write_str("  base=");
            Console::write_hex(r.base);
            Console::write_str(" size=");
            Console::write_hex(r.size);
            Console::write_str("\n");
        }
    }
```

（`os/kernel/src/vm_handoff.rs:fn classify` 的打印块。串口上出现 `kernel: vm_handoff free n=… deducted=…` 加至多 8 条 `base=… size=…`——§6.1 的诊断第 2 项靠这行读数分辨"快照本来就小"还是"扣减切多了"。）

手算一遍扣减序列，数据取自测试 `test_classification_deducts_all_record_sources`（§5.2 细读）：memmap 只有一条 `[1M, 64M)`；根页在 2M；bump 区间 `[4M, 5M)`；已用帧后缀 `[6M, 8M)`；内核镜像 `[128K, 1M)`；一个保留模块 blob `[3M, 3.5M)`。扣减记录五条：`[128K, 1M)`（与 memmap 只在 1M 边界相接）、`[2M, 2M+4K)`（根页，按页取整）、`[4M, 5M)`、`[6M, 8M)`、`[3M, 3.5M)`。切完幸存五段：`[1M, 2M)`、`[2M+4K, 3M)`、`[3.5M, 4M)`、`[5M, 6M)`、`[8M, 64M)`——与测试断言的期望序列一致。内核镜像那笔的边界行为：起点在 memmap 之前的扣减不是错误，cut 对不重叠的部分无操作（[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4 的 cut 语义）。

调用点在 `os/kernel/src/lib.rs` 的 `init_proc_and_boot`（Phase C 末构建 handoff 页的那次调用，§3.5 的发布段同点）。

反向推演：若扣减不整批 cut、而逐条即时 cut——中间态的 memmap 既是后续源的查询输入又是被改对象，扣减顺序就成了正确性的一部分（先切某条可能改变后续源要查询的边界）；先记满再切，记录与动作解耦，五个源的 push 顺序只影响记录顺序、不影响幸存集。

### 4.3 announce 与 kerninfo：kinfo 的用户态投影（→§2.3、§3.2）

C 的第5拍 announce 打印启动横幅，函数体很短：

```c
// minix3/minix/kernel/main.c:announce —— 逐字引用（全函数）
static void announce(void)
{
  /* Display the MINIX startup banner. */
  printf("\nMINIX %s. "
#ifdef PAE
"(PAE) "
#endif
#ifdef _VCS_REVISION
	"(" _VCS_REVISION ")\n"
#endif
      "Copyright 2016, Vrije Universiteit, Amsterdam, The Netherlands\n",
      OS_RELEASE);
  printf("MINIX is open source software, see http://www.minix3.org\n");
}
```

（`minix3/minix/kernel/main.c:announce`。版本串来自编译期宏：release 与 version 的填充在 `minix3/minix/kernel/main.c:cstart` 的 kinfo 填充段——`strlcpy(kinfo.release, OS_RELEASE, ...)` 与 `strlcpy(kinfo.version, OS_VERSION, ...)` 两行。）

Rust 侧的对位是一行横幅（Step 3），横幅之外用户态第一次拿内核信息靠的是 kerninfo 页：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 3）
    // Step 3: announce() — print MINIX banner
    // C: announce() in main.c:60 — printf("MINIX %s ...\n", OS_RELEASE)
    // Rust: print to EarlyConsole so QEMU serial captures it.
    use crate::kmess::{EarlyConsole, KernelConsole as Console};
    Console::write_str("\nMINIX-RS 0.1.0 (rust rewrite) — scheduling live\n");
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 3。按原文引用的坐标 `main.c:60` 已漂移——announce 的调用行在 `main.c` 第58行，登记。）

```rust
// os/kernel/src/kerninfo.rs:const KERNINFO_USER_VA 与 :const KUSERINFO_OFFSET —— 逐字引用
/// Fixed user-space VA of the kernel information page.
///
/// Placement policy, same constraints as `minix_types::VM_BOOT_HANDOFF_VA`:
/// above the 4 GiB identity window (a VA below it would alias the boot
/// identity mapping instead of getting fresh PTEs), below every supported
/// architecture's user-VA limit (Sv39 caps user VA at 2^38), and far from
/// the handoff page at 0x1_0000_0000 so the two never share a PD entry.
/// Consumers never hardcode this value — they receive it from the
/// `MINIX_KERNINFO` call — so moving it later is a one-line change.
pub const KERNINFO_USER_VA: u64 = 0x2_0000_0000;

/// Offset of the `kuserinfo` sub-structure inside the page.
///
/// C keeps `minix_kerninfo` and `kuserinfo` as separate `.usermapped`
/// symbols; the pointer relation is all that matters, so both live on one
/// page here and the pointer is `KERNINFO_USER_VA + KUSERINFO_OFFSET`.
const KUSERINFO_OFFSET: usize = 0x800;
```

（`os/libs/minix-types/src/types/boot.rs:const KERNINFO_USER_VA` 与 `os/kernel/src/kerninfo.rs:const KUSERINFO_OFFSET`。前者是单一权威定义，内核 `os/kernel/src/kerninfo.rs` 只以 `pub use minix_types::KERNINFO_USER_VA` 再导出。第二条注释的原话就是 §3.2 增补决策的落点：C 的两个分离符号（`minix_kerninfo` 与 `kuserinfo`，都在 `.usermapped` 段）在 Rust 并到一页，保留的只有指针关系——`KERNINFO_USER_VA + KUSERINFO_OFFSET`。）

页的构建时机在 Phase B.5，注释把次序理由写全：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（Phase B.5 注释与调用）
    // Phase B.5: kernel information page — build, user-map, publish.
    // From here on the MINIX_KERNINFO IPC call (= 6) answers OK with the
    // page address instead of EBADCALL (C: proc.c:685-693, publication at
    // memory.c:913). Runs before proc_init so boot processes are born with
    // the page already queryable, mirroring the C order where the cstart
    // kuserinfo fill (main.c:438-440) precedes proc_init.
    kerninfo::init_kerninfo(kernel_info);
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 Phase B.5。调用入口是 `os/kernel/src/kerninfo.rs:fn init_kerninfo`。`MINIX_KERNINFO` 的调用号是 6——C 侧同号调用在 proc.c，页未就绪时返回 EBADCALL。）

两种投影的分界：C 是"结构体字段拷贝 + 固定 `.usermapped` 段布局"——cstart 填字段，段获得用户映射后发布地址；Rust 是"单页镜像 + 调用号投递"——整页写好、映射进当前启动根、通过 `MINIX_KERNINFO` 调用把地址交给询问者。C 的 cstart 填充先于 proc_init，Rust 的 B.5 先于 Phase C——两边的次序是同构的。

反向推演：kerninfo 若排在 proc_init 之后——boot 进程出生时页表里没有这张页的映射，出生后的首次 `MINIX_KERNINFO` 询问落在缺页上；C 的次序（cstart 填充先于 proc_init）防的是同一件事，两边的约束同构。

### 4.4 system_init 的 Rust 表达：Phase E 的空与 enum 的满（→§3.1）

第 3.1 节定了"编译期穷尽"的决策，本节把"表何时可用"的实现面走完。分派入口先看形状——BKL 的取得、见证、内部分派、记账标记、转移：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch —— 逐字引用（外层分派全函数）
pub fn kernel_call_dispatch(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // Acquire BKL — C: BKL_LOCK() in mpx.S kernel_call_entry_common
    // R-03: Keep the guard alive and derive a BklSection witness for
    // compile-time BKL proof on global accessor calls (irq_manager_with, etc.).
    // R-05/B1: BklGuard is RAII (Drop releases BKL); transfer() hands
    // ownership to the ambient held-BKL scope because BKL must stay held
    // until kernel_call_finish() releases it.
    let bkl_guard = crate::smp::bkl_lock();
    let result = {
        let bkl_section = bkl_guard.section();
        kernel_call_dispatch_inner(caller_nr, proc_table, msg, priv_table, clock_state, &bkl_section)
    };
    // D-9 (C system.c:160) — the kernel call is in flight for `caller`:
    // attribute kernel time to it at the next context_stop. C sets the
    // marker after dispatch, before finish, unconditionally (a failed
    // call's kernel work is still the caller's); kernel_call_resume does
    // not re-set it.
    crate::set_kbill_kcall_with(caller_nr, &bkl_guard.section());
    // BKL is NOT released here — transfer() suppressed the guard's Drop.
    // BKL is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    bkl_guard.transfer();
    result
}
```

（`os/kernel/src/syscall.rs:fn kernel_call_dispatch`。块内 `R-03`/`R-05`/`B1`/`D-9` 是仓库内部编号，按原文引用。本篇与 `13-syscall-dispatch`（系统调用分派篇，尚未重生成）的分界在：这里只看"表已可用"之后分派怎么消费表，handler 本体与 58 项全表归那边。transfer 的"不是释放"语义 §4.5 展开。）

归一化是消费表前的第一道工序：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner —— 逐字引用（调用号归一化）
    // Base normalization (C mpx.S `subl $KERNEL_CALL` parity): libc issues
    // ABSOLUTE call numbers (com.h `SYS_*` = KERNEL_CALL + offset — the
    // E2/E6 wrapper precedent), while the dispatch tables index by the
    // RELATIVE offset. Host tests feed relative numbers directly; both
    // shapes normalize here — anything below the base passes through.
    let call_nr = if msg.m_type >= minix_types::KERNEL_CALL {
        (msg.m_type - minix_types::KERNEL_CALL) as u16
    } else {
        msg.m_type as u16
    };

    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };
```

（`os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner`。一处注释偏离要登记：注释说归一化与"C mpx.S 的 `subl $KERNEL_CALL`"对位，但在本仓的 Minix3 树里，这个减法发生在 C 代码里——`minix3/minix/kernel/system.c:kernel_call_dispatch` 的 `call_nr = msg->m_type - KERNEL_CALL;` 一句，arch 目录的 .S 文件里没有 KERNEL_CALL 字样（复算命令：`grep -rn KERNEL_CALL minix3/minix/kernel/arch/`，零命中）。语义对位仍然成立：两边都在范围检查前把调用号归一到相对偏移。）

归一化接受两种调用号形状：绝对号（libc 形状，`KERNEL_CALL` 加偏移）与相对号（宿主测试直喂），低于基底的值直通——注释原话"anything below the base passes through"。这是宿主测试与生产共用一条分派路径的接口决定。

权限查表与 match 骨架（节选）：

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner —— 逐字引用（权限查表）+ 节选（match 头尾）
    let call_denied = proc_table
        .get(caller_nr)
        .and_then(|c| c.priv_id)
        .and_then(|id| priv_table.get(id))
        .is_none_or(|caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
    if call_denied {
        return KcallResult::CallDenied;
    }
```

```rust
// os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner —— 节选（match 骨架；
// 省略号处为其余同形分派臂，首臂与架构臂、尾臂逐字保留）
    match syscall {
        Syscall::Fork => dispatch_fork(caller_nr, proc_table, msg, priv_table),
        Syscall::Exec => dispatch_exec(caller_nr, proc_table, msg),
        Syscall::Clear => dispatch_clear(caller_nr, proc_table, msg, priv_table, clock_state),
        ...
        // D6: x86-specific syscalls — return BadCall on other architectures.
        Syscall::Devio => CurrentArchSyscall::dispatch_devio(caller_nr, proc_table, msg, priv_table),
        Syscall::Sdevio => CurrentArchSyscall::dispatch_sdevio(caller_nr, proc_table, msg, priv_table),
        // D6: VDEVIO is also x86-specific (system.c:215-216: #if defined(__i386__))
        Syscall::Vdevio => CurrentArchSyscall::dispatch_vdevio(caller_nr, proc_table, msg, priv_table),
        ...
        // D6: ARM-specific — return BadCall on other architectures.
        Syscall::Padconf => CurrentArchSyscall::dispatch_padconf(caller_nr, proc_table, msg),
    }
}
```

（`os/kernel/src/syscall.rs:fn kernel_call_dispatch_inner` 的权限查表与 match 节选。权限查表是 Option 链：无特权记录直接拒绝，有则过过滤器——C 对位的 `GET_BIT(priv(caller)->s_k_call_mask, call_nr)` 在 system.c 的分派函数里。架构专属臂走 `CurrentArchSyscall` 的默认实现返回 BadCall——§3.1 的决策；块内 `D6` 是注释原稿对这条决策的编号（§3.1 开头的撞号登记即指此处）。块内坐标 `system.c:215-216` 按原文引用——核对实源，该 `#if` 现在 :213（同族坐标漂移，登记）。）

三段装配的 Rust 落点收拢成一张表（与 §2.1 的三段对照）：

| C 三段（system_init） | Rust 落点 | "表可用"时刻 |
|---|---|---|
| IRQ 钩子池清零 | `IrqManager::new` 构造（Phase B） | Phase B 末 |
| 报警定时器逐 priv 初始化 | ClockState 内嵌链（05 篇） | Phase B 末 |
| call_vec 清零 + 46 条 map | 无动作（enum + match 即表） | 链接期 |

反向推演两则（`#[cfg]` 裁剪变体让"拒绝"变"非法"见 §3.1；归一化只收绝对号让宿主测试被迫造偏移、测试与生产分叉见本节上文的 try_from 走查）——实现面的对应物是：try_from 的 Err 与 match 的穷尽性是仅有的两道入口守卫，二者都由编译器背书。

### 4.5 Phase F 前置：守卫回收与 SMP 接线（→§3.2、§3.5）

第 3.2 节的重排决策落在 Phase F 前段的三段实现上。第一段是那条恒不进入却写了约五十行的回收分支——守卫体内的调用与 Result 分类处置：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（守卫体内的 add_memmap 调用与 Result 处置）
        // SAFETY: Boot is single-threaded; FREE_MEMMAP is only accessed here
        // during boot. kernel_may_alloc is true at this point.
        let add_memmap_result = unsafe {
            let mmap = &mut *FREE_MEMMAP.get();
            memmap::add_memmap(
                mmap,
                kernel_info.bootstrap_start().0,
                kernel_info.bootstrap_len(),
            )
        };
        // Pattern §31 (返回值完整性): C add_memmap returns void; errors are
        // fatal (panic) in C. Rust explicitly handles the Result. Two failure modes:
        //   - `Err(MemMapError::ZeroLength)`: bootstrap_len is not
        //     page-aligned and rounds down to zero. The default
        //     kernel_info contains zero — handled by the outer
        //     `if bootstrap_len > 0` guard. Reaching here means a
        //     boot-shim bug; we keep the boot going (log + carry on)
        //     because losing the bootstrap reclaim is recoverable
        //     (less free memory, not a crash).
        //   - `Err(MemMapError::NoSlots)`: MAXMEMMAP entries are all
        //     in use. Indicates a memory map corruption or a missing
        //     boot-shim cleanup; surface via kernel log so the issue
        //     is visible during debugging. The default behaviour
        //     (carry on) matches the previous `let _ = ...`.
        match add_memmap_result {
            Ok(_slot_idx) => {
                // Slot was claimed; no action needed. We use the
                // underscore prefix to make the intent explicit:
                // "we don't need the index, but the success case
                // is meaningful".
            }
            Err(e) => {
                // Pattern §31 compliant: handle every Result variant.
                // Use `debug_assert!` in release-mode-noop + log
                // approach: the boot should succeed even on failure,
                // but the failure must be observable.
                #[cfg(debug_assertions)]
                panic!("add_memmap failed: {:?} — boot-shim has a bug", e);
                #[cfg(not(debug_assertions))]
                {
                    // Release builds: carry on. The kernel still
                    // boots; the bootstrap memory is simply not
                    // reclaimed (a small leak of the boot-shim
                    // region, bounded by `bootstrap_len`).
                    let _ = e; // explicit discard with intent
                }
            }
        }
    }
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 Phase F 守卫体。块内 `Pattern §31` 是内部设计模式的引用记号，按原文引用。C 的 `add_memmap` 返回 void、错误即 panic（§2.2 的窗口表）；Rust 把两种失败分开：ZeroLength 意味着 boot-shim 报了未对齐的引导段（可恢复，少一块内存不是崩溃），NoSlots 意味着记账表满（内存图损坏或 shim 漏清理，要可见）。）

第二段是 BKL 守卫上的 `transfer`。它不是释放：

```rust
// os/kernel/src/smp.rs:impl BklGuard::transfer —— 逐字引用（文档注释与函数体）
    /// B1 (todo §1): this replaces the former `core::mem::forget(guard)`
    /// idiom at every cross-function transfer point. Mechanics are
    /// identical (ManuallyDrop suppresses Drop), but the transfer is now a
    /// named, greppable API on the guard's own type instead of a raw
    /// `forget` scattered across call sites — "who intentionally keeps the
    /// BKL held" is one search away, and a future RAII-native redesign has
    /// exactly one suppression point to replace.
    pub fn transfer(self) {
        // `mem::forget` suppresses `Drop` (which would `bkl_unlock`), so the
        // guard leaks and the BKL simply stays held. This is the ONE
        // deliberate `forget` in the entire codebase (B1): every
        // cross-function BKL transfer goes through here instead of a
        // scattered `core::mem::forget` at call sites.
        core::mem::forget(self);
    }
```

（`os/kernel/src/smp.rs` 的 `BklGuard::transfer`。块内 `B1` 是仓库内部编号，按原文引用。transfer 吞掉 RAII 守卫的 Drop 责任——BKL 的下一次释放点在 `kernel_call_finish` 或 `switch_to_user` 侧（§4.4 引的注释里列了这两个释放点），这是"持有权在函数间转移"的类型化表达。锁状态的检查函数是 `os/kernel/src/smp.rs:fn bkl_is_locked`。一处按原文引用的过时断言要登记：注释说这是"全库唯一一个故意的 `forget`、所有跨函数转移都经此处"——现码不成立，`os/kernel/src/trap_dispatch.rs` 的三个调度再入路径（`reenter_scheduler` 与两个架构的 `resched_thunk`）都是 `bkl_lock()` 后裸 `mem::forget(guard)` 的旧形状，另有三个 qemu 测试内核同形；"唯一性"只在 kernel 生产路径的启动链上成立，登记不展开。）

第三段是 smp_init 的调用——C 形状的降级语义被内嵌：

```rust
// os/kernel/src/lib.rs:fn kmain —— 逐字引用（smp_init 调用注释与尾表达式）
    // C main.c:311 parity — full SMP bring-up (smp_start_aps + wait_for_APs
    // inside). C: "if smp_init() returns it means that it failed and we try
    // to finish single CPU booting" — our smp_init embeds the same degrade-
    // and-continue semantics (per-AP timeout skip + tolerant wait), so
    // falling through to bsp_finish_booting is the C-shaped flow. Returns
    // with the BKL held (wait_for_aps re-acquired it after the dance).
    crate::smp::smp_init();

    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        let smp_state = crate::smp_state_boot_unchecked();
        let proc_table = crate::proc_table_boot_unchecked();
        bsp_finish_booting(proc_table, smp_state)
    }
}
```

（`os/kernel/src/lib.rs` 的 `kmain` 的 Phase F 尾段。拓扑装配在 `os/kernel/src/lib.rs:fn init_smp_state`，它查平台描述符里的 CPU 拓扑而不是硬编码单核；`os/kernel/src/smp.rs:fn smp_init` 内嵌逐 AP 超时跳过与宽容等待——"smp_init 返回即失败"的 C 语义（§2.4）在这里变成"个别 AP 失败不阻断收尾"。）

第一段为什么值得五十行：分支恒不进入（§3.2），但分支体是 boot-shim 的验收面——shim 回退成"报告引导段"时它激活，两种失败各有处置策略：debug 构建 panic；release 构建是带注释的显式丢弃（`let _ = e;`）——丢弃按设计是静默的，可观测性由 debug 构建的 panic 与注释本身承担。对照物是 `os/kernel/src/memmap.rs:fn add_memmap` 文档注释里的核销记录——C 的 `mmap_size`/`mem_high_phys` 两个字段在 Rust 是设计 no-op：

```rust
// os/kernel/src/memmap.rs:fn add_memmap（模块文档注释的核销段）—— 逐字引用
/// **D-34 resolution (design no-op)**: both C consumers are absorbed by
/// replacement designs in Rust —
/// - `mmap_size` (count of used slots): `FREE_MEMMAP` scans empty slots
///   implicitly (`memmap.rs` scan loop), so a separate counter would be a
///   second source of truth;
/// - `mem_high_phys`: the frame allocator (`frame.rs`) iterates regions
///   directly instead of a high-water scan, and the VM consumer asserts
///   `!free_regions.is_empty()` instead of `mmap_size > 0`
///   (`os/servers/vm/src/boot.rs:141-144`).
///
///   No Rust code reads either value — maintaining them would be write-only
///   state. Verified 2026-09-06; see todo.md D-34 and 08-system-init-boot-finish.md §4.5.
```

（`os/kernel/src/memmap.rs` 的 add_memmap 文档注释核销段。块内 `D-34` 是仓库内部编号，按原文引用；注释尾部的 "§4.5" 指向即本节，落位一致；注释内引用的 `os/servers/vm/src/boot.rs:141-144` 行号已漂移（`!free_regions.is_empty()` 断言现在 :171-174 附近），按原文引用未改。两个 C 字段的职责被替代设计吸收：占用槽计数由空槽扫描隐式给出，最高物理地址由帧分配器直迭代替代。）

反向推演：若回收分支去掉 Result 处置、恢复裸丢弃——NoSlots（memmap 碎片化）在 release 下静默，启动后"少了一块内存"无从对账；处置策略的注释原话把这条论证写成了"failure must be observable"。

### 4.6 bsp_finish_booting 逐步走查：Step -1 到 9（→§3.3、§3.4）

第 3.3/§3.4 的决策（两个全局、发散与参数化、两拍增删）落到函数体的逐步执行上。先给 C 十二拍与 Rust Step 的对照总表，再逐拍走。

| C 拍 | C 锚点 | Rust Step | Rust 锚点 | 差注 |
|---|---|---|---|---|
| （无） | — | Step -1 panic 钩子 | `os/kernel/src/lib.rs:fn register_panic_diagnostic` | Rust 特有（§3.4） |
| 1 cpu_identify | `minix3/minix/kernel/arch/i386/arch_system.c:cpu_identify` | Step 0 | `os/kernel/src/smp.rs:fn cpu_identify` | 探针进 arch crate，表进 smp.rs；顺带修一处 C 的解码错误 |
| 2 vm_running = 0 | `minix3/minix/kernel/main.c:bsp_finish_booting` 第2拍 | Step 1 | Step 1（§3.3 已引） | 语义同位 |
| 3 krandom 两行 | 同上第3拍 | （无独立步） | — | 移到启动更早阶段（06 篇 Phase C.6 语境） |
| 4 bill/proc_ptr → idle | 同上第4拍 | Step 2 | `os/kernel/src/proc_table.rs:fn set_bill_to_idle` | 记账标志位；proc_ptr 侧归调度循环 |
| 5 announce | 同上第5拍 | Step 3 | Step 3（§4.3 已引） | 一行横幅 |
| 6 解除循环 | 同上第6拍 | Step 4 | Step 4（本节） | rts_unset 自动入队 |
| 7 记账基线 | `minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init` | Step 5 | Step 5（本节） | note_context_switch 记基线 |
| 8 定时器 | `minix3/minix/kernel/clock.c:boot_cpu_init_timer` | Step 6 | `os/kernel/src/lib.rs:fn boot_init_timer` | 三段序独立成函数（定时器三段的本体在 05 篇） |
| 9 fpu_init | `minix3/minix/kernel/arch/i386/arch_system.c:fpu_init` | Step 7 | Step 7（本节） | 探针删除，直置 true |
| 10 FIXME 三查 | 同上第10拍 | （无） | — | 消失（§3.6） |
| 11 SMP 标志段 | 同上第11拍 | （前置） | `os/kernel/src/lib.rs:fn init_smp_state` | 移到 smp_init 前（§3.2 重排） |
| 12 关窗 | 同上第12拍 | Step 8 | Step 8（§3.3 已引） | 同位 |
| （无） | — | Step 8.5 BKL 断言 | Step 8.5（§3.4 已引） | Rust 特有 |
| 终 switch + NOT_REACHABLE | 同上终拍 | Step 9 | `os/kernel/src/lib.rs:fn switch_to_user` | `!` 吸收哨兵 |

Step 0：CPU 身份探测。Rust 把寄存器读取放进 arch crate、把表留在内核侧，分界写在 `os/kernel/src/smp.rs` 的段注释里：

```rust
// os/kernel/src/smp.rs —— 逐字引用（CPU 身份表段注释）
// ── CPU identity table (D-53: 08-system-init-boot-finish.md §4.6) ──
//
// C's split: `cpu_identify()` (arch code, arch_system.c:212 i386 / :85 earm)
// fills the kernel global `cpu_info[CONFIG_MAX_CPUS]` (glo.h). The register
// read lives in the arch crate (`CurrentCpuIdentity` probe); the table lives
// here because `CONFIG_MAX_CPUS` is kernel configuration and `GET_CPUINFO`
// (misc.rs) is the kernel-ABI consumer.
```

（`os/kernel/src/smp.rs` 的段注释。块内 `D-53` 是仓库内部编号，按原文引用；它指名的 §4.6 即本节，落位一致。）

x86 探针里有一处对 C 的行为修正——CPUID 叶 1 的 model 解码。Rust 侧的文档注释把两边的差异写成了对照：

```rust
// os/arch/src/x86_64/cpu_identity.rs:fn decode_signature —— 逐字引用（文档注释与函数体）
/// Decode the CPUID leaf-1 EAX signature into `(family, model, stepping)`.
///
/// Per SDM (CPUID leaf 1, EAX): extended family [27:20] joins when the base
/// family is 0xF; extended model [19:16] joins when the *family* is 0xF or
/// 0x6.
///
/// MINIX3 BUG: C (arch_system.c:239) conditions the extended-model merge on
/// the *base model* (`model == 0xf || model == 0x6`), truncating the model
/// for every family-6 CPU whose base model is neither 0xF nor 0x6 — i.e.
/// all modern Intel cores from Penryn on (Nehalem 0x106E0 reports 0xE
/// instead of 0x1E; Skylake 0x406E9 reports 0xE instead of 0x4E). The
/// condition is harmless on C's era hardware (family-0xF Netburst sets the
/// extended-model bits only with base model 6) but breaks exactly the
/// modern CPUs minix-rs targets. // Rust fix: condition on family.
fn decode_signature(eax: u32) -> (u8, u8, u8) {
    let mut family = ((eax >> 8) & 0xF) as u8;
    if family == 0xF {
        family = family.saturating_add(((eax >> 20) & 0xFF) as u8);
    }
    let mut model = ((eax >> 4) & 0xF) as u8;
    if family == 0xF || family == 0x6 {
        model += (((eax >> 16) & 0xF) << 4) as u8;
    }
    (family, model, (eax & 0xF) as u8)
}
```

（`os/arch/src/x86_64/cpu_identity.rs:fn decode_signature`。C 侧的写法在 `minix3/minix/kernel/arch/i386/arch_system.c:cpu_identify`：合并条件写在了 base model 上，而 SDM 的条件是 family——family-6 的现代处理器（Nehalem、Skylake 等）的 model 高位全被截掉。C 时代硬件上无害、目标硬件上必错；这是本篇唯一一处 Rust 明确改变**内核语义值**（身份表字段值）的地方；面向用户的可观测差异另有 §4.3 的横幅措辞改写。四条解码测试钉住它（§5.1）。）

Step 2：记账落点。Rust 不是指针赋值：

```rust
// os/kernel/src/proc_table.rs:fn set_bill_to_idle —— 逐字引用（文档注释与函数体）
    /// Set the BSP's bill pointer to the IDLE kernel task.
    ///
    /// C: `bsp_finish_booting` step 2 — `get_cpulocal_var(bill_ptr) =
    /// get_cpulocal_var_ptr(idle_proc)` (main.c:50).
    ///
    /// We do not have a live `SmpState` plumbed here yet; this records the
    /// intent in `ProcessTable` so the scheduler side can read it. The real
    /// per-CPU write lives in `SmpState.cpu_locals[0].set_running(IDLE)`
    /// (see smp.rs:127) and is performed by the scheduler init path.
    pub fn set_bill_to_idle(&mut self) {
        use crate::proc::proc_nr;
        if let Some(idle) = self.get_mut(proc_nr::IDLE) {
            // Bill to IDLE so that time spent in this pre-scheduling phase
            // is accounted to the kernel, not to any user process. Matches
            // C's `bill_ptr = idle_proc` semantic.
            idle.p_accounting.bill_to_idle();
        }
    }
```

（`os/kernel/src/proc_table.rs:fn set_bill_to_idle`。按原文引用的坐标 `main.c:50` 已漂移——C 的两行在 56-57（§3.4 已登记同族坐标），注释内 `(see smp.rs:127)` 的行号也已漂移（`set_running` 实际在 :226）。C 的 per-CPU `bill_ptr` 指针在 Rust 是记账状态位，由调度循环侧的记账消费读取；两者在"启动期时间记到内核头上"这一点上同义，形状不同。proc_ptr 那一半的落点：注释里指名的 `set_running(IDLE)` 在生产代码里**没有调用者**——调度循环入口（`os/kernel/src/lib.rs` 的 `scheduler_loop` 前段）直接写 CpuLocal 的 `proc_ptr` 字段，"由调度器初始化路径承担"这半句属实，机制名是注释的过时形状，登记。）

Step 3 见 §4.3。Step 4：解除循环：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 4）
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

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 4。上界算术与 C 相同；`os/kernel/src/proc_table.rs` 的 rts_unset 带自动入队语义——§1.3 的"VM 当场唯一入队者"在 Rust 重放时成立的是同一条推理链：只有 VM 槽在清位后全零。）

Step 5：TSC 基线：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 5 与延迟置位注释）
    // Step 5: cycles_accounting_init() — set BSP TSC baseline.
    // C: cycles_accounting_init() — proc.c (resets per-CPU cycle counters).
    // Sets the BSP's TSC baseline so the first context switch has a
    // correct reference point. `note_context_switch(tsc)` records
    // `cpu_last_tsc = tsc` and `cpu_last_idle = tsc` in the BSP's
    // per-CPU state.
    // The TSC is the cycle counter (rdtsc on x86-64, CNTPCT_EL0 on aarch64,
    // mtime on riscv64). See `clock::read_tsc` for the per-arch wrapper.
    let tsc = crate::clock::read_tsc();
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.note_context_switch(tsc);
    }
    // Note: `tsc_ctr_switch` is set inside `note_context_switch` only if
    // we extend the API. For now the field starts at 0; the next quantum
    // check will see `cpu_last_tsc = tsc` (a non-zero value) and reset
    // `tsc_ctr_switch` on the first context switch. This matches the
    // deferred-but-functional behavior: the BSP gets a clean TSC baseline.
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 5。两处登记：其一，注释说 C 的 `cycles_accounting_init` 在 proc.c，实际定义在 `minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init`（§2.3 已逐字引）；注释还说 `note_context_switch` 记 "cpu_last_tsc = tsc and cpu_last_idle = tsc"——后半现码不成立。其二，紧跟其后的 Note 段注释声称 "`tsc_ctr_switch` is set inside `note_context_switch` only if we extend the API"、字段延迟到首次上下文切换才由量子检查复位（"deferred-but-functional"）——这整段是过时叙事：现码 `os/kernel/src/smp.rs` 的 `note_context_switch` **直接写** `tsc_ctr_switch = tsc; cpu_last_tsc = tsc;` 两个字段（不写 cpu_last_idle），smp.rs 自己的单元测试以非零常数直喂并断言 `tsc_ctr_switch` 落位。所以在"建立切换基线"这一层，Rust 与 C 同构；现存的残差只有 `cpu_last_idle`——C 在此拍清零它，Rust 的现码不写这个字段。按现码读，本拍的语义是"三个记账量里的两个当场建基线"。）

Step 6：定时器。C 的第8拍在 Rust 是一次独立函数调用：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 6 调用点）
    // S-8 (2026-09-14): the three-part sequence moved to the standalone
    // `boot_init_timer` (same order, D-59 contract intact) so the
    // test-timer-irq bring-up kernel reuses the production wiring instead
    // of duplicating it.
    boot_init_timer();
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 6。块内 `S-8` 是仓库内部编号，按原文引用。独立函数的文档注释把"为什么是一个函数"写成次序契约：）

```rust
// os/kernel/src/lib.rs:fn boot_init_timer —— 逐字引用（文档注释主体）
/// Program the boot clock source, register the clock IRQ hook, and open the
/// timer gate — the C `boot_cpu_init_timer(system_hz)` three-part sequence
/// (clock.c:294), extracted from `bsp_finish_booting` Step 6 so the S-8 L3
/// bring-up test (`test-timer-irq`) reuses the production wiring verbatim.
///
/// Order contract (D-59, 2026-09-09 — the reason this is one function):
/// program the source, register the handler, open the gate. Reordering any
/// two steps leaves a live, unhandled interrupt source across boot.
///
/// C: boot_cpu_init_timer — clock.c:294 (init_local_timer +
///    register_local_timer_handler); intr_init/apic gates.
```

（`os/kernel/src/lib.rs:fn boot_init_timer` 的文档注释。块内 `S-8`/`D-59` 是仓库内部编号、日期按原文。三段序（编程、注册、放行）的本体教学在 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §4.7，含 x86 侧 pic_init 收尾的重映射理由；本篇只落两件事：调用时点在 Step 5 之后（因果见本节末尾），以及一处指向登记——`boot_init_timer` 定义前的一段注释写 "See 08-system-init-boot-finish.md §4.5"，那个指向实落在本节（Step 6 的定时器位置），编号沿注释原稿。）

Step 7：FPU 存在性。C 的探针（§2.3 的回读比对）在 Rust 删除，直置 true。裁决注释引它的现文（省去一行以仓库内部编号开头的裁决行，该行陈述的裁决即后随注释本身）：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 节选（Step 7 注释与写入；
// 注释中段省去一行裁决行首，其余逐字）
    // Step 7: fpu_init() — set BSP FPU presence.
    // C: fpu_init() — arch-specific (arch_system.c, i386/earm).
    // This is a global "is FPU present" probe that updates
    // `cpulocals.fpu_presence`. In Rust, `CpuLocal::fpu_presence` is
    // already a `bool` field (see smp.rs:134). All three target
    // architectures (x86-64, aarch64, riscv64) have FPUs, so we set
    // the BSP's fpu_presence to `true`. The per-process FPU init is
    // handled by `CpuContextArch::build_cpu_context` via the per-arch FPU
    // strategy field (`fpu_policy` on x86_64, `fpu_enable_el0` on aarch64,
    // `sstatus.FS` on riscv64 — see 06-proc-init-boot-proc.md §4.1)
    // (already built for every boot process — see init_proc_and_boot).
    //
    // ...
    // aarch64 的 CPACR_EL1.FPEN 懒陷阱门控由**首次 finish_and_restore 的
    // disable()** 建立（任何用户首条 FP 指令之前），`FpuArch::init()`
    // （0b01）保持未接线——firmware 复位值 0b00 期间的窗口内内核 -neon
    // 无 EL1 FP 使用，安全；真机接线实验（a64-fix-7）伴随早期 PM↔VFS
    // 握手墙复发（pm fb201 洪流），不冒险。x86_64/riscv64 的 init 语义
    // 同样由各自启动路径的既有策略位承担。
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.fpu_presence = true;
    }
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 7。块内 `a64-fix-7`/`pm fb201` 是仓库内部实验与问题记号，按原文引用；注释内 `(see smp.rs:134)` 的行号已漂移（`fpu_presence` 字段实际在 smp.rs:175），按原文引用未改。探针删除的理由：三个目标架构的 FPU 是架构保证（§2.3 末尾的"硬件代际遗物"判断在此兑现），aarch64 的 FP 门控由首次 FP 使用时的懒陷阱建立、不依赖这拍。）

Step 8 与 8.5 见 §3.3 与 §3.4 的引用。Step 9：

```rust
// os/kernel/src/lib.rs:fn bsp_finish_booting —— 逐字引用（Step 9 与函数尾）
    // Step 9: switch_to_user() — never returns
    // C: switch_to_user(); NOT_REACHABLE;
    // Rust: Divergent function, type `-> !`
    // Covered in detail in 10-switch-to-user.md
    //
    // Suppress unused-idle warning: IDLE slot was used by step 2.
    let _ = proc_nr::IDLE;
    switch_to_user()
}
```

（`os/kernel/src/lib.rs:fn bsp_finish_booting` 的 Step 9。`os/kernel/src/lib.rs:fn switch_to_user` 是薄入口——打一条 "entering scheduler" 串口路标后进调度循环，循环本体归 `10-switch-to-user`（切用户与调度循环篇，尚未重生成）。注释里的 "Covered in detail in 10-switch-to-user.md" 是篇题指向，按原文引用。）

次序约束在 Rust 侧的保留方式与 C 不同：C 靠直线布局，Rust 的 Step 5→6 依赖一条真实的中断时序——放行段（enable_timer_irq）在注册与编程之后当拍生效，首个 tick 最早在 Step 6 执行中就可能到达，所以基线（Step 5）必须已经就位；Step 8→9 依赖发散类型（关窗之后才允许切换）。两个反向推演都是 C 推演的 Rust 重放：Step 5 挪到 Step 6 之后——首个 tick 的结算读到零基线（§2.3 的"整个 TSC 计入当前进程"重演，且 Rust 侧的 tick 到达窗口更早，因为放行当拍生效）；Step 4 若包含任务槽——rts_unset 的自动入队会把没有执行上下文的任务槽真挂上就绪队列（§1.3 的重放）。

---

## 5. 测试要点

### 5.1 测试清单与读数（→第 4 章）

与本篇相关的测试分五组，分布在五个文件。先给两套计数口径——它们数的东西不同，正文分开报：

口径一（源码计数，含 helper 与全部测试模块）：

```bash
grep -c 'fn test_' os/kernel/src/syscall.rs os/kernel/src/memmap.rs \
  os/kernel/src/vm_handoff.rs os/kernel/src/lib.rs os/arch/src/x86_64/cpu_identity.rs
```

预期输出：`syscall.rs` 44、`memmap.rs` 16、`vm_handoff.rs` 7、`lib.rs` 56、`cpu_identity.rs` 4（每个文件一行"文件:计数"）。

口径二（宿主实跑，mock 构建收集到的用例数与通过数）：

```bash
cargo test -p minix-kernel --features mock --lib
cargo test -p minix-kernel --features mock --lib vm_handoff
```

预期关键行：全量为 `running 840 tests` 与 `test result: ok. 832 passed; 0 failed; 8 ignored`；过滤后为 `running 7 tests` 与 `test result: ok. 7 passed`。两口径的差异来自模块组合：`grep -c` 数的是源文件里的测试函数定义（lib.rs 的 56 个定义经模块树展开后与其他模块的测试一起收集），cargo 的 840 是整个 crate 收集到的全部用例（复核：`smp.rs:1952` 的 `test_clear_idle_flag` 系成稿后新增，读数随之上移 1）。

五组的钉点清单：

| 组 | 测试名 | 钉什么 |
|---|---|---|
| syscall（44 条中本篇相关的 5 条） | `test_syscall_try_from_valid` | 判别值 0/1/53/57 各自映射到正确变体 |
| | `test_syscall_try_from_invalid` | 空洞编号 11 与越界 58/255 返回 Err |
| | `test_kernel_call_dispatch_bad_call` | 非法调用号 → BadCall；测试自证 BKL 由 dispatch 取得、须手动释放 |
| | `test_kernel_call_dispatch_call_denied_no_priv` | 无特权记录的调用者 → CallDenied |
| | `test_kernel_call_dispatch_sets_kbill_marker` | 分派后、finish 前无条件置 kbill 标记（被拒的调用也算调用者的内核工作） |
| memmap（16 条） | add_memmap/cut_memmap 的对齐、切除、补回语义 | [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4 已列，本篇不重复 |
| vm_handoff（7 条） | `test_classification_deducts_all_record_sources` | 五源全部进扣减记录（§4.2 手算的数据源） |
| | `test_classification_free_list_is_exact_complement` | 空闲表是 memmap 减扣减账的精确补（§5.2 细读） |
| | `test_classification_clips_to_dm_window` | 跨窗口端的区域被裁剪不丢弃；PA 0 的根页切掉首帧 |
| | `test_classification_drops_out_of_window_region` | 窗口外的区域不进空闲表 |
| | `test_boot_image_table_layout` | 5 任务 + 12 模块恰填 C 的 boot_image 表；名字截断同 strlcpy |
| | `test_identity_windows_clip_sort_merge` 与 `…_empty_reserved_still_has_mmio` | 恒等窗口的构建合同（归 09 篇方向） |
| lib.rs bsp（2 条） | `test_bsp_finish_booting_step_5_7_side_effects` | Step 5/7 的副作用直驱（§5.2 细读） |
| | `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | 单核构建只有 BSP 槽被初始化，AP 槽保持默认值 |
| cpu_identity（4 条） | `test_decode_signature_family6_extended_model` 等 4 条 | model 解码修复的四个代际（§4.6 的 MINIX3 BUG 对照） |

### 5.2 关键测试细读：扣减账互补与发散函数的可测性（→§4.2、§4.6）

第一条细读是扣减账的构造性验证——空闲表必须精确等于 memmap 减去扣减账：

```rust
// os/kernel/src/vm_handoff.rs（tests 模块）—— 逐字引用（exact_complement 测试）
    /// Per-span layout where every source occupies its own memmap span:
    /// the free list is exactly the untouched spans.
    #[test]
    fn test_classification_free_list_is_exact_complement() {
        let memmap = [
            mem_region(0x10_0000, 0x10_0000), // [1M, 2M)  → free
            mem_region(0x20_0000, 0x10_0000), // [2M, 3M)  → kernel image
            mem_region(0x30_0000, 0x10_0000), // [3M, 4M)  → bump region
            mem_region(0x40_0000, 0x10_0000), // [4M, 5M)  → used frames
            mem_region(0x50_0000, 0x10_0000), // [5M, 6M)  → reserved module
            mem_region(0x60_0000, 0x10_0000), // [6M, 7M)  → free
        ];
        let root = PhysBytes(0x1_f0_0000); // outside memmap (UEFI LOADER_DATA case)
        let c = classify_raw(
            &memmap,
            root,
            Some((0x30_0000, 0x40_0000)),
            &[(0x40_0000, 0x50_0000)],
            &[(0x20_0000, 0x10_0000), (0x50_0000, 0x10_0000)],
        );

        assert_eq!(
            sorted_free(&c),
            vec![(0x10_0000, 0x20_0000), (0x60_0000, 0x70_0000)]
        );
        assert_eq!(c.deducted_count, 5, "root + bump + used + kernel + module");
        assert_disjoint(&c);
    }
```

（`os/kernel/src/vm_handoff.rs` tests 模块。布局故意让每个源独占一条 memmap 区间——空闲表就应该是没被碰过的两条 `[1M, 2M)` 与 `[6M, 7M)`；根页放在 memmap 之外，对应 §4.2 说的 UEFI LOADER_DATA 形态（切它是 no-op，但记录里要有这一笔，所以 `deducted_count` 是 5 不是 4）。`assert_disjoint` 钉的是"账与表按构造不相交"。）

改错哪一行会变红：若 `classify` 的切循环漏切某一源（比如模块 blob 那一循环被去掉），被漏的区间会出现在 `sorted_free` 的期望里、`deducted_count` 也会从 5 变 4——两处断言同时红。若只删掉 `assert_disjoint`，"切两次同一区间"这类重复扣减（表端正确、账端重复）不再被拦。

第二条细读是发散函数的可测性——`bsp_finish_booting` 在宿主测试里不能整体调用（`-> !`，且 mock 构建下整个函数不编译），测的是从函数体里抽出的副作用动作：

```rust
// os/kernel/src/lib.rs（tests 模块）—— 逐字引用（Step 5/7 副作用直驱测试）
    /// Verify the bsp_finish_booting Step 5/7 side effects: cycles accounting
    /// init and fpu_presence are set on the BSP's CpuLocal. Since
    /// bsp_finish_booting is divergent (-> !, calls switch_to_user), we test
    /// the side-effect operations directly.
    #[test]
    fn test_bsp_finish_booting_step_5_7_side_effects() {
        use crate::smp::SmpState;
        use crate::clock::read_tsc;

        // Simulate Step 5 + Step 7 of bsp_finish_booting.
        let mut smp = SmpState::new_single_cpu();
        let tsc = read_tsc();
        let bsp = smp.bsp_cpu_id();
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.note_context_switch(tsc);
        }
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.fpu_presence = true;
        }

        // Verify side effects.
        let bsp_local = smp.cpu_local(bsp).unwrap();
        assert_eq!(bsp_local.cpu_last_tsc, tsc, "cycles_accounting_init must set cpu_last_tsc");
        assert_eq!(bsp_local.cpu_last_idle, tsc, "cycles_accounting_init must set cpu_last_idle");
        assert!(bsp_local.fpu_presence, "fpu_init must set fpu_presence = true");
    }
```

（`os/kernel/src/lib.rs` tests 模块。测试的文档注释把模式说破：发散函数不能整体调用，就直接驱动同一段副作用——构造 SmpState、调用 Step 5/7 的等价动作、断言基线与 fpu 位。测试钉的是副作用动作本身的语义；Step 顺序与调用关系由走查（§4.6）与真机启动覆盖，不在这条测试的射程里。）

改错哪一行会变红，要先说清这条测试的一个边界：宿主测试里 `crate::clock::read_tsc()` 走 `#[cfg(test)]` 分支恒返回 0（`os/kernel/src/clock.rs` 的 read_tsc 测试桩），所以前两条记账断言（`cpu_last_tsc == tsc` 与 `cpu_last_idle == tsc`）在宿主构建下是 `0 == 0` 的恒真式——它们记录了断言意图，但在宿主门没有判别力；fpu 断言有效。记账写入的真实判别覆盖在 `os/kernel/src/smp.rs` 的 `test_cpu_local_note_context_switch`：以非零常数直喂 `note_context_switch`，断言 `tsc_ctr_switch` 与 `cpu_last_tsc` 两个字段落位（§4.6 的现码行为由它钉住）。若 `note_context_switch` 不再写 `tsc_ctr_switch`，那条 smp 单元测试红；本测试能红的只有 fpu 一条。

### 5.3 测试边界与诚实披露（→第 3 章、第 4 章）

没有测试钉住的面，本篇逐项列出。复算命令与预期一并给出——"没有"是可复核的结论：

```bash
grep -rn 'smp_single_cpu_fallback\|single_cpu_fallback' os/kernel/src --include='*.rs' | grep -i 'test'
grep -rn 'announce\|MINIX-RS 0.1.0' os/kernel/src --include='*.rs' | grep -i 'assert\|test'
grep -rn 'KERNEL_MAY_ALLOC' os/kernel/src --include='*.rs'
```

预期关键行：第一条与第二条零命中（smp 降级分支与横幅内容没有断言）；第三条命中只有定义、两处 store 与一处访问器（§3.3 的读数），没有任何测试模块出现。逐项披露：

| 没有测试的面 | 现状 | 与正文的关联 |
|---|---|---|
| smp 降级分支（fallback 与逐 AP 超时） | 分支本体无宿主测试（超时常量与单核 `wait_for_aps` 另有两条宿主测试，降级分支不在其中）；行为只能真机观察 | §2.4/§4.5 的降级语义只有读码一个证据源 |
| announce/横幅内容 | 无断言 | §4.3 的一行横幅没有合同 |
| panic 钩子注册时点 | 无测试；Step -1 之前的 panic 行为归 minix-rt | §3.4/§6.1 的诊断分界靠读码 |
| KERNEL_MAY_ALLOC 翻转序列 | 无测试钉住 | 与 §3.3 的"零消费点"披露互为表里——状态无读者，测试也无从断言行为 |
| kerninfo 页内容与 C 布局的逐字节对齐 | kerninfo 自己的测试模块钉 magic/偏移合同，不钉与 C 的逐字节一致 | §4.3 的"指针关系保留"是读码结论 |

真机门与宿主门的分界也要说清：定时器三段（Step 6）的真机行为——tick 真的到达、放行门真的打开——归 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §5.2 的 QEMU 载体叙事，宿主测试只覆盖"生产接线被复用"这一层（Step 6 注释里 `test-timer-irq` 复用 `boot_init_timer` 的原话）；本篇的 Step 5/7 副作用测试属于宿主门。

### 5.4 机制 → 需要证明什么（反转核对表）

（同系列先例：从**机制**出发列"要证明它成立需要什么断言"；下表"测试覆盖状态"与 §1.5 的"C 侧边界"是两个维度，读时按列名区分。）

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 测试覆盖状态 | 现有证据 / TODO |
|---|---|---|---|---|
| M1 | 三个开关的次序防线（§1.1、§2.2、§2.3） | 关窗先于切换；基线先于定时器；解除先于切换 | 部分覆盖 | 真机启动链全链（能到 `entering scheduler` 即十二拍全过）；**反序负向无载体** → **R08-T01** |
| M2 | A2 扣减账与空闲表的构造性不相交（§3.5、§4.2） | 全源扣减、重叠即 panic、页对齐 | 已覆盖 | 宿主 `vm_handoff` 七条（含 `test_classification_deducts_all_record_sources`，§5.2） |
| M3 | KERNEL_MAY_ALLOC 翻转序列（§2.2、§3.3） | 开窗/关窗发生且仅发生（除"零消费点"披露） | 未覆盖 | §5.3 已披露无测试钉住 → **R08-T02** |
| M4 | smp 降级分支（§2.4、§4.5） | fallback 与逐 AP 超时的行为 | 未覆盖 | §5.3 已披露（分支本体无宿主测试）→ **R08-T03** |
| M5 | kerninfo 页与 C 布局的对齐（§4.3） | magic/偏移合同；与 C 逐字节一致 | 部分覆盖 | 宿主钉 magic/偏移；**与 C 逐字节对齐无载体** → **R08-T04** |
| M6 | panic 钩子注册时点（§3.4、§6.1） | Step -1 之前/之后的 panic 行为分界 | 未覆盖 | §5.3 已披露（归 minix-rt 通道）→ 登记（跨篇） |
| M7 | announce 横幅（§4.3） | —— | 无合同 | §5.3 已披露（一行横幅无断言）→ 登记 |
| M8 | Step 5/7 副作用（§4.6） | 记账基线复位、`fpu_presence` 置位 | 已覆盖 | 宿主两条（§5.3 末句） |
| M9 | Phase E 的"无动作"（§3.2、§4.4） | 分发表在链接期就位（无启动动作） | 已覆盖（结构式） | enum + match 的穷尽性由编译器背书（§3.1） |

（§5.3 的披露行分别对到本表 M3/M4/M6/M7 的断言化；两类表互为反转面，任一表更新须同步另一表。）

**新 TODO（对源码/测试 workflow 的需求输入）**：

| ID | 内容 | 分流 |
|---|---|---|
| R08-T01 | 三个开关反序的负向载体（构造"关窗晚于切换"的启动序） | **测试侧** |
| R08-T02 | KERNEL_MAY_ALLOC 翻转序列的断言载体 | **测试侧** |
| R08-T03 | smp 降级分支的载体（宿主 fallback 场景或真机超时） | **测试侧** |
| R08-T04 | kerninfo 页与 C 逐字节对齐的核对载体 | **测试侧** |

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认命令与预期关键行 |
|---|---|---|
| 启动后 tick 不来，或冻结在调度循环入口 | Step 6 的三段序或 x86 的 pic_init 缺席 | 串口读数分两层：出现 `entering scheduler` 说明 Step 9 已达、tick 不来是 Step 6 的门问题；连它都没有则是 Step 8.5/9 之前的挂死。检查接线序：`grep -n 'boot_init_timer\|enable_timer_irq\|pic_init' os/kernel/src/lib.rs`——注意可读性边界：前几条命中是 kmain 与函数注释里的历史字样，"编程"与"注册"两步的调用（`init_timer`/`register_hook`）不含这些关键词、grep 不可见；**可见的次序证据**只有放行（`enable_timer_irq` 调用行）先于 pic_init（x86 尾调用行）这一对，编程与注册两步要靠 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §4.7 的走查核对 |
| VM 首跑非法指令、页表完好 | A2 扣减漏了 VM blob（回收形状复发）或 A2 空闲表重复发放 | 串口找 `kernel: vm_handoff free n=` 行，核对 deducted 计数是否覆盖全部模块 blob；跑 `cargo test -p minix-kernel --features mock --lib vm_handoff`，预期 7 passed——`test_classification_deducts_all_record_sources` 单独拦"漏源" |
| panic 只有消息、没有栈走查 | panic 发生在 Step -1（诊断钩子注册）之前的阶段 | 看串口最后的 boot_stage 路标：最后一条若是 `kernel: entering validate` 或 `kernel: kmain A.1b console+kinfo ok` 一类，panic 落在钩子注册前，走 minix-rt 的 stage-1 通道（只有消息）；若路标已到 Phase F 之后仍无栈，才是钩子本身的故障 |
| 启动后内存对不上、或用户态查询内核内存布局多出内核镜像 | memmap 快照与扣减口径不一致 | 串口 `kernel: vm_handoff free n=` 的 base/size 明细与 boot-shim 上报的 memmap 对账；工具侧用 [07-cross-space-init.md](07-cross-space-init.md) §6 的 Direct Map 覆盖检查命令回指核对窗口边界 |

第一行的两层读数展开说：串口路标是启动进度的唯一观测面，`entering scheduler` 由 `os/kernel/src/lib.rs:fn switch_to_user` 打印——它出现意味着十二拍全部走完、CPU 已进调度循环，此刻 tick 不来只可能出在 Step 6 的放行链（编程了没注册、注册了没放行、x86 上 PIC 重映射缺席）；它不出现则挂在更早的拍，从最后一条路标定位。第二行的机理在 §3.5：回收 VM blob 的形状会让 boot-exec 在 VM 启动前覆写 VM 的 text 页——页表完好是判别特征，故障面在"谁先拿帧"。

### 6.2 手算与常量核对命令

想核对正文的任一断言，按断言跑对应命令：

```bash
# 三个常量（§2.1/§1.3）
grep -n 'define NR_TASKS\|define VM_PROC_NR\|define NR_SYS_CALLS' minix3/minix/include/minix/com.h
# 预期：NR_TASKS 5、VM_PROC_NR 8（注释 memory server）、NR_SYS_CALLS 58

# map 注册条数（§2.1，源文本口径含全部条件分支）
awk 'NR>=185 && NR<=270' minix3/minix/kernel/system.c | grep -c 'map(SYS_'
# 预期：46

# kernel_may_alloc 的 C 消费点（§2.2，五处 assert）
grep -n 'kernel_may_alloc' minix3/minix/kernel/arch/i386/pg_utils.c
# 预期：42、74、102、143、274 五行

# vm_running 的全部读写点（§2.3，一写五读）
grep -rn 'vm_running' minix3/minix --include='*.c' --include='*.h' | grep -v earm
# 预期：7 行——main.c 写 0 一处，do_umap_remote/oxpcie/acpi 读五处，glo.h 声明一处

# KERNEL_MAY_ALLOC 的 Rust 消费面（§3.3，零运行时消费点）
grep -rn 'KERNEL_MAY_ALLOC\|kernel_may_alloc()' os/kernel/src --include='*.rs'
# 预期：六行——use 导入、定义、两处 store、访问器定义与体内 load；无调用者

# 测试计数（§5.1 口径一）
grep -c 'fn test_' os/kernel/src/syscall.rs os/kernel/src/memmap.rs \
  os/kernel/src/vm_handoff.rs os/kernel/src/lib.rs os/arch/src/x86_64/cpu_identity.rs
# 预期：44、16、7、56、4
```

VM 入队推理链（§1.3）的核对命令单独一条——确认 12 个服务槽里只有 VM 不挂 inhibit：

```bash
grep -n 'VM_PROC_NR\|RTS_VMINHIBIT\|RTS_BOOTINHIBIT' minix3/minix/kernel/main.c
# 预期：豁免条件一行（p_nr != VM_PROC_NR && p_nr >= 0）、两个置位各一行
```

---

## 7. 参见

- [03-kmain-cstart.md](03-kmain-cstart.md)（Phase 划分与 kmain 前半）、[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.6/§3.7/§4.7（定时器三段与 pic_init）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4/§2.6（RTS 位族与 boot 循环）、[07-cross-space-init.md](07-cross-space-init.md) §2.2（memory_init/freepdes）、[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §2.4/§2.5（add_memmap/cut_memmap 机制与引导段语义）
- `09-vm-boot-protocol`（handoff 页协议面与五步协商，尚未重生成）、`10-switch-to-user`（调度循环本体，尚未重生成）、`13-syscall-dispatch`（分派全景与 58 项全表，尚未重生成）、`16-smp`（smp_init 本体与 AP 路径，尚未重生成）、`17-clock-timer`（tick 后的记账消费，尚未重生成）、`31-fpu-context-switching`（FPU 上下文，尚未重生成）
- C 源码：`minix3/minix/kernel/main.c:bsp_finish_booting`、`minix3/minix/kernel/main.c:kmain`、`minix3/minix/kernel/main.c:announce`、`minix3/minix/kernel/system.c:system_init`、`minix3/minix/kernel/arch/i386/arch_system.c:fpu_init`、`minix3/minix/kernel/arch/i386/arch_clock.c:cycles_accounting_init`、`minix3/minix/kernel/clock.c:boot_cpu_init_timer`
- Rust：`os/kernel/src/lib.rs` 的 `kmain`、`os/kernel/src/lib.rs:fn bsp_finish_booting`、`os/kernel/src/lib.rs:fn boot_init_timer`、`os/kernel/src/lib.rs:fn switch_to_user`、`os/kernel/src/syscall.rs`、`os/kernel/src/memmap.rs`、`os/kernel/src/vm_handoff.rs`、`os/kernel/src/kerninfo.rs`、`os/kernel/src/smp.rs`
