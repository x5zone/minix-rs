# 14 · 异常与中断：同步与异步事件的分流

> **C 真值源**：`minix3/minix/kernel/arch/i386/exception.c`（`ex_data`/`exception_handler`/`pagefault`/`inkernel_disaster`）、`minix3/minix/kernel/interrupt.c`（`put_irq_handler`/`rm_irq_handler`/`irq_handle`/`enable_irq`/`disable_irq`）、`minix3/minix/kernel/type.h`（`irq_hook_t`）、`minix3/minix/kernel/glo.h`（`catch_pagefaults`/`irq_actids`）、`minix3/minix/kernel/vm.h`（`PHYS_COPY_CATCH`）、`minix3/minix/kernel/config.h`（`NR_IRQ_HOOKS`）、`minix3/minix/kernel/arch/i386/protect.c`（`gate_table_exceptions`）、`minix3/minix/kernel/arch/i386/mpx.S`（`exception_entry`/`copr_not_available`/`hwint_master`/`hwint_slave`）、`minix3/minix/kernel/arch/i386/sconst.h`（`TEST_INT_IN_KERNEL`）、`minix3/minix/kernel/arch/i386/klib.S`（copy_msg 族契约注释与恢复标签）、`minix3/minix/include/arch/i386/include/interrupt.h`（`DEBUG_VECTOR`/`NR_IRQ_VECTORS`）、`minix3/minix/kernel/arch/i386/include/archconst.h`（`PAGE_FAULT_VECTOR`/`PROTECTION_VECTOR`）、`minix3/minix/kernel/system/do_irqctl.c`（`generic_handler`）、`minix3/minix/kernel/system.c`（`cause_sig`）、`minix3/minix/kernel/system/do_vmctl.c`（`VMCTL_CLEAR_PAGEFAULT`）、`minix3/minix/kernel/proc.c`（`copr_not_available_handler`）、`minix3/minix/kernel/arch/i386/memory.c`（VM 请求的按需页捕获区）。
> **Rust 实现**：`os/arch/src/arch/exception_dispatcher.rs`（`ExceptionClass`/`ExceptionSignal`/`ExceptionOutcome`/`ExceptionDispatcher`/`handle`/`classify_signal`）、`os/arch/src/arch/exception.rs`（`ExceptionArch`/`FaultContext`/`RecoveryPoint`/`FaultContextTracker`）、`os/arch/src/arch/trap_style.rs`（`TrapStyle`，[10-switch-to-user.md](10-switch-to-user.md) §1.2/§4.4 已立）、`os/kernel/src/page_fault.rs`（`set_pagefault_pending`/`clear_pagefault_pending`/`is_pagefault_pending`/`build_vm_pagefault_msg`/`kernel_mode_pagefault_panic_msg`）、`os/kernel/src/irq_manager.rs`（`IrqManager`/`IrqHookContext`/`IrqNotify`/`KernelNotifier`/`generic_notify_handler`/`dispatch_hardware_irq`）、`os/kernel/src/trap_dispatch.rs`（x86 异常分发体/`forward_pagefault_to_vm`/`exception_signal_to_nr`）、`os/kernel/src/syscall_signal.rs`（`cause_signal`）、`os/libs/minix-types/src/ipc/vm.rs`（`VM_PAGEFAULT`）、`os/plat/src/interrupt.rs`（`IrqAction`/`NR_IRQ_HOOKS`/`NR_IRQ_VECTORS`）。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:enum NAME`、`path:struct NAME`），不使用行号；逐字引用块的块级区间题注带行区间与复算命令；表内行号是当前代码快照，锚点仍以符号名为准。
> **本篇主题**：CPU 遇到的计划外事件怎么分流——异常（同步，当前指令触发）有归属，有转信号、转发 VM、内核自愈、停机四种归宿；中断（异步，设备触发）无归属，走 hook 链把通知交给注册过的驱动进程；以及 Rust 用枚举决策面、独立转发助手与索引化 hook 池怎么承接这套分流。
> **前置**：[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.3（`intr_init` 的十次寄存器写——控制器初始化在本篇之前完成）、§3.4（控制器的归宿是全局 `IRQ_MANAGER`）、[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1（IRQ 钩子池清零进了 `IrqManager::new` 构造——本篇讲注册与触发时刻）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（`CLEAR_PAGEFAULT`/MEMREQ 的消费端——本篇讲缺页转发的生产端识别半）、[10-switch-to-user.md](10-switch-to-user.md) §2.3（pick_proc 与 restore 的进出配对——本篇讲事件把进程停下的进入侧）、[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 与 §4.2（RTS 位变化的队列半——本篇讲 `RTS_PAGEFAULT`/`RTS_SIGNALED` 位为什么变）、[12-ipc-core.md](12-ipc-core.md) §2.1（int-33 的 IPC 分派——本篇讲异常与中断这两条别的进入通道）、[13-syscall-dispatch.md](13-syscall-dispatch.md)（syscall 指令陷入的传参与分派——本篇讲剩下两类事件：异常与设备中断）。
> **后续**：`19-syscall-signal` 方向（信号的用户态接口与投递，尚未重生成）、`24-cross-space-runtime` 方向（VMSUSPEND 恢复协议与跨空间拷贝运行时，尚未重生成）、`31-fpu-context-switching` 方向（FPU 保存与恢复本体，尚未重生成）、`17-clock-timer` 方向（时钟 handler 内部，尚未重生成）。
> **目标读者**：熟悉操作系统基本概念（进程、虚拟内存、特权级、陷入与中断）与 C 语言的开发者；Rust 一侧读到 `enum`/`match`/`Result` 大意即可。
> **前置知识**：[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.3/§3.4（控制器初始化与 `IRQ_MANAGER` 归宿）、[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1（IRQ 钩子池构造清零）、[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（`CLEAR_PAGEFAULT` 消费端）、[10-switch-to-user.md](10-switch-to-user.md) §1.2/§4.4（KTS 记录与 `TrapStyle` 承接）、[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2/§4.2（RTS 位变化的队列半）、[12-ipc-core.md](12-ipc-core.md) §2.1/§2.5（int-33 分派与 NOTIFY 旁路）、[13-syscall-dispatch.md](13-syscall-dispatch.md)（syscall 分派与五步周期）。
> **教学目标**：①能说出异常与中断的分野（同步有归属 vs 异步无归属）与异常四种归宿（转信号/转发 VM/内核自愈/停机；两个入口特例：伪 NMI 忽略、用户态 #NM 汇编拦截）；②能按判定次序复述 `exception_handler`（伪 NMI → 嵌套三段 → 缺页 → 来源）与 `pagefault` 的四层判定；③能复述 IRQ hook 链的注册/注销/触发/开关与 `irq_actids`「全零才放线」；④能说出 Rust 承接形状——`ExceptionSignal`/`ExceptionOutcome` 与 `IrqManager` 索引池承重，`ExceptionClass`（未接线）与 `FaultContext`（生产恒 `Normal`，服务测试与未来捕获窗）为待接线件（如实披露见 §3.1/§3.2）；⑤能说出 31 条测试清单的构成与哪一半没有宿主测试（其中 14 条当前不可执行，见 §5.3）。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构：决策/执行拆分、枚举化）；`ExceptionClass` 未接线与 `FaultContext` 生产恒 `Normal` 为如实披露的现状；无 `[ARCH: ...]` 标注对象。
> **编号说明**：引文内出现的 3.4–3.7 式节号（带 `§` 前缀）是源码注释按 `draft/14-exception-interrupt.md` 的节号所写——与本文现行节号不同，按注释原文保留；`R14-T*` 为本文 §5.4 自含的交接编号。

[13-syscall-dispatch.md](13-syscall-dispatch.md) 讲完了用户态主动陷入的两大通道：int-33 把消息原语送进 `do_ipc`，syscall 指令把内核调用送进 `kernel_call`。这两条通道有一个共同点——都是**代码自己要求**进内核的。本篇讲剩下的两类事件，它们的共同点是**没有人要求它们发生**：一类是异常，CPU 执行当前指令时撞上的意外（除以零、访问了没映射的地址、碰了非法指令）；另一类是中断，设备在任意时刻插进来的信号（时钟走了一拍、磁盘来了一块数据）。异常与中断共用一套陷入机制，但语义截然不同：异常有归属——它因当前进程的指令而起，内核可以把它折算成这个进程的信号；中断无归属——它因设备而起，谁在跑就打断谁，内核只能沿着注册好的 hook 链把通知交给等它的驱动进程。

本篇走完这套分流：C 的两个文件怎么分工（第 2 章）、Rust 用什么形状承接（第 3 章）、逐函数怎么落地（第 4 章）、靠什么测试钉住（第 5 章）、卡住时怎么查（第 6 章）。

---

## 1. 概述

### 1.1 异常与中断：同步与异步的两类计划外事件

异常与中断的分工，C 源码在文件头用三行话写完了：

```c
/* This file contains a simple exception handler.  Exceptions in user
 * processes are converted to signals. Exceptions in a kernel task cause
 * a panic.
 */
```

（区间复算：`sed -n '1,4p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 4。锚点：`minix3/minix/kernel/arch/i386/exception.c`。）

「用户进程里的异常转成信号，内核任务里的异常导致 panic」——这句注释把异常的处置原则定了调，但它只覆盖了异常这一半。补上另一半的是事件的**归属**问题：异常是同步的，它由当前正在执行的那条指令直接触发——除零的那条 `div`、缺页的那次访存——所以它有明确的归属者，内核可以对这个进程做点什么（发信号、通知 VM、停掉它）；中断是异步的，它由设备触发，到达时机与当前进程毫无关系——时钟中断到达时正在跑的可能是任何进程。归属的差异决定处置的差异：异常可以折算成归属者的信号（用户态非缺页异常如此；缺页转 VM、内核态异常自愈或停机——四种归宿见下图），因为它就是这个进程自己撞出来的；中断不能杀任何进程——设想中断可以杀进程：一个网络包到达，内核把「正在运行」的进程杀掉抵账，可设备驱动根本无从知道这个进程做错了什么，它甚至不知道这个进程的存在。杀进程的依据只能来自进程自己的行为，这正是异常与中断的分界线。

两类事件从哪里进来？异常门与中断门都登记进同一张 IDT——C 侧分两张门表装入：`gate_table_pic`（硬件中断门，`hwint00`-`hwint15`）与 `gate_table_exceptions`（异常门与用户陷入门，本段）。`minix3/minix/kernel/arch/i386/protect.c` 的异常门段：

```c
static struct gate_table_s gate_table_exceptions[] = {
	{ divide_error, DIVIDE_VECTOR, INTR_PRIVILEGE },
	{ single_step_exception, DEBUG_VECTOR, INTR_PRIVILEGE },
	{ nmi, NMI_VECTOR, INTR_PRIVILEGE },
	{ breakpoint_exception, BREAKPOINT_VECTOR, USER_PRIVILEGE },
	{ overflow, OVERFLOW_VECTOR, USER_PRIVILEGE },
	{ bounds_check, BOUNDS_VECTOR, INTR_PRIVILEGE },
	{ inval_opcode, INVAL_OP_VECTOR, INTR_PRIVILEGE },
	{ copr_not_available, COPROC_NOT_VECTOR, INTR_PRIVILEGE },
	{ double_fault, DOUBLE_FAULT_VECTOR, INTR_PRIVILEGE },
	{ copr_seg_overrun, COPROC_SEG_VECTOR, INTR_PRIVILEGE },
	{ inval_tss, INVAL_TSS_VECTOR, INTR_PRIVILEGE },
	{ segment_not_present, SEG_NOT_VECTOR, INTR_PRIVILEGE },
	{ stack_exception, STACK_FAULT_VECTOR, INTR_PRIVILEGE },
	{ general_protection, PROTECTION_VECTOR, INTR_PRIVILEGE },
	{ page_fault, PAGE_FAULT_VECTOR, INTR_PRIVILEGE },
	{ copr_error, COPROC_ERR_VECTOR, INTR_PRIVILEGE },
	{ alignment_check, ALIGNMENT_CHECK_VECTOR, INTR_PRIVILEGE },
	{ machine_check, MACHINE_CHECK_VECTOR, INTR_PRIVILEGE },
	{ simd_exception, SIMD_EXCEPTION_VECTOR, INTR_PRIVILEGE },
	{ ipc_entry_softint_orig, IPC_VECTOR_ORIG, USER_PRIVILEGE },
	{ kernel_call_entry_orig, KERN_CALL_VECTOR_ORIG, USER_PRIVILEGE },
	{ ipc_entry_softint_um, IPC_VECTOR_UM, USER_PRIVILEGE },
	{ kernel_call_entry_um, KERN_CALL_VECTOR_UM, USER_PRIVILEGE },
	{ NULL, 0, 0}
};
```

（区间复算：`sed -n '127,152p' minix3/minix/kernel/arch/i386/protect.c | wc -l` = 26。锚点：`minix3/minix/kernel/arch/i386/protect.c` 的 `gate_table_exceptions` 定义段。）

这张表登记 23 条门：前 19 条是 x86 的异常向量（除零 0 号、调试 1 号、NMI 2 号……SIMD 19 号），后 4 条是 int-32/33/34/35 四条用户态陷入指令门（13 篇的 syscall 通道与 12 篇的 IPC 通道）。异常向量与用户陷入指令共用 IDT 这个机制，但走到内核之后的去向完全不同：陷入指令门通向 `do_ipc`/`kernel_call`（12、13 两篇），异常门通向本篇的分流器。

把两类事件的完整去向画成一张机制图，这就是全篇的行进地图：

```
                CPU 执行流遇到计划外事件
                          │
        ┌─────────────────┴──────────────────────┐
        │ 异常（同步）                             │ 中断（异步）
        │ 当前指令的产物：除零/缺页/GP…            │ 设备信号：时钟/磁盘/网络…
        │ 有归属：因谁而起就停在谁头上，           │ 无归属：谁在跑就打断谁，
        │ 能折算成它的信号                         │ 只能通知注册过的驱动进程
        ▼                                         ▼
  protect.c 门表：19 条异常门，各指向             hwint00-15（mpx.S 汇编）
  mpx.S 里自己的汇编标签                            │ PIC_IRQ_HANDLER(irq)
        │                                          ▼
        │ 向量 7（#NM）的用户态在汇编层就分走：     irq_handle(irq)（interrupt.c）
        │ copr_not_available → 懒 FPU 恢复         │ 先屏蔽本线
        │ （31 方向）；内核态落嵌套入口             │ 遍历 hook 链：置 ID 位→调 handler
        ▼                                          │ handler 返回真则清位
  exception_entry → 判嵌套 →                       ▼
  exception_handler（exception.c）            全部清位才放开本线；最后 ack
        ├─ 向量 2 → 打印一行忽略
        ├─ 向量 14 → pagefault → 转发 VM
        ├─ 嵌套命中拷贝/FPU 函数区 → eip 重定向（自愈）
        ├─ 嵌套调试陷阱+追踪位 → 清位返回（忽略）
        ├─ 非嵌套+用户进程 → cause_sig → 信号管理器（19-syscall-signal 方向）
        └─ 其余（内核里的异常）→ inkernel_disaster + panic
```

图里异常那一侧画出的终点里，四个是异常的归宿：**转信号**（用户进程的异常，交给信号管理器）、**转发 VM**（缺页，交给管内存的服务器）、**内核自愈**（嵌套异常命中预登记的恢复区，折回一个失败标签接着跑）、**停机**（内核自己撞上异常，打印诊断信息后 panic）；另两个是「就当没发生」的忽略分支——伪 NMI 的打印返回与嵌套调试陷阱的清位返回（§2.2）。（本图按归宿归类，不表判定次序——判定次序见 §2.2。）中断那一侧只有一条路：hook 链通知。向量 7 在用户态的分支（懒 FPU 恢复）没有落进这四种归宿——它在汇编入口就被分走了，根本到不了 C 的分流器；这个细节在第 2 章展开。

本章收束一遍框架：计划外事件按归属分成两类——异常同步且有归属：转信号、转发 VM、自愈都以归属者为对象，停机则说明内核自己出了问题；中断异步且无归属，唯一动作是沿 hook 链通知等它的驱动。第 2 章按这份地图逐个函数看 C 怎么落实。

### 1.2 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| 信号的用户态接口（sigreturn、handler 跳板、投递时序） | 只讲到 `cause_sig` 置位并通知信号管理器为止 | `19-syscall-signal` 方向（信号接口篇，尚未重生成） |
| VMSUSPEND 的恢复协议（VM 服务完缺页后进程怎么活回来） | 只讲转发消息的生产端；消费端回指 | [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3、`24-cross-space-runtime` 方向（尚未重生成） |
| FPU 保存与恢复本体（FXSAVE/FXRSTOR、所有权交接） | 只讲 #NM 这个入口位的判定 | `31-fpu-context-switching` 方向（尚未重生成） |
| 时钟 handler 内部（报警链、uptime） | 只讲 IRQ hook 链把它通知到 | `17-clock-timer` 方向（尚未重生成） |
| 中断控制器的初始化（8259A/APIC/GIC/PLIC 编程） | 已讲完，本篇从「线已就绪」起步 | [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.3、[同篇](05-clock-interrupt-init.md) §3.4 |
| trap 入口的寄存器传参（int-33/syscall 的 r1/r2/r3 怎么来） | 已讲完，本篇只讲异常帧与向量 | [13-syscall-dispatch.md](13-syscall-dispatch.md) §2.0 |
| IPC 过滤的决策面 | 不讲 | `23-ipc-filter`（过滤决策篇，尚未重生成） |

认知减负声明：本章不必深究信号的用户态接口（`19-syscall-signal` 方向）、VMSUSPEND 恢复协议（`24-cross-space-runtime` 方向）、FPU 保存细节（`31-fpu-context-switching` 方向）、时钟 handler 内部（`17-clock-timer` 方向）、控制器初始化（05 篇已讲）——本篇保证「事件进来 → 判定归属 → 落到归宿」这条分流线索读完整。

### 1.5 C 侧覆盖边界（本篇叙述面逐项对账）

本篇的 C 对照源在架构上只有两半可用，逐项列清（判据：`minix3/minix/kernel/arch/` 实查 = {i386, earm}）：

| 叙述面 | C 对应物 | 处置 |
|---|---|---|
| `ex_data`/`exception_handler`/`pagefault`/`inkernel_disaster` | x86：`i386/exception.c`；**ARM32：`earm/exception.c` 同型**（8 向量表、`catch_pagefaults`+地址窗口、`inkernel_disaster`） | 正文按 x86 逐段走查；ARM32 差异（向量表内容、`saved_lr` 对位）不展开 |
| IRQ hook 链（注册/注销/触发/开关） | `minix3/minix/kernel/interrupt.c`——架构无关，i386 与 earm 共用（线数各异：i386 16/64、earm 125） | 直接对照（架构无关面） |
| 陷入入口汇编（`exception_entry`/`hwint_master`/`hwint_slave`/`copr_not_available`） | x86：`i386/mpx.S`；ARM32：`earm/mpx.S`（同型入口） | 对照 x86；ARM 汇编差异不展开 |
| 三架构统一分发面（Rust `ExceptionDispatcher`/`TrapStyle` 判定上移） | **无 C 对应物**（C 侧无 riscv 实现；aarch64 无 64 位 C 对照） | 新增形状：判定上移（§3.3 表行 14、§4.1）与 claim/complete 显式化（§4.3）——均属无 C 对位面，不写「与 C 一致」 |
| `FaultContext` 枚举替换地址窗口 | 窗口机制 x86 与 ARM32 都有；**枚举化本身无 C 对位** | §3.2 如实披露（生产恒 `Normal`） |
| 按需页捕获窗（`phys_copy_fault` 家族） | x86 `i386/klib.S`、ARM32 `earm/phys_copy.S` | 本篇只对照 x86 四标签；ARM 侧不展开 |

纪律：**无 C 对应物的分支一律写「无 C 对位」**，不写「与 C 一致」；§5.4 反转表不对无 C 源分支标「已覆盖」。

---

## 2. C 源码分析

### 2.0 本章地图：两个文件、两种事件

异常与中断的 C 实现住在两个文件里：`minix3/minix/kernel/arch/i386/exception.c`（386 行）住着异常分流的全套——分类表、分流主干、缺页转发、内核灾难兜底；`minix3/minix/kernel/interrupt.c`（177 行）住着中断的 hook 链管理——注册、注销、触发、开关。这个文件划分不是随手为之：异常处理必须读 x86 的异常帧与 CR2 寄存器，天生贴架构（所以住在 `arch/i386/` 下）；hook 链管理只操作链表与位图，哪个架构都一样（所以住在 `kernel/` 顶层，x86 与 ARM 共用）。异常处理里唯一的架构无关件——`cause_sig`——也被抽到了 `minix3/minix/kernel/system.c`。

画出两个文件的函数关系：

```
  exception.c（arch/i386，异常一侧）
    exception_handler(is_nested, frame) ── 分流主干，入口来自 mpx.S 的 exception_entry
      ├─ ex_data[20] ──────────── 向量→信号的分类表（数据，不是函数）
      ├─ pagefault(pr, frame, is_nested) ── 向量 14 专用：缺页转发 VM
      │    ├─ catch_pagefaults + in_physcopy/in_memset ── 物理拷贝捕获窗（开关+地址窗）
      │    └─ mini_send(pr, VM_PROC_NR, ...) ── 转发动作
      ├─ cause_sig(proc_nr, signum) ── 转信号动作（函数体在 system.c）
      └─ inkernel_disaster(...) ── 兜底：打印现场 + panic
  interrupt.c（kernel 顶层，中断一侧）
    put_irq_handler(hook, irq, handler) ── 注册：追加 hook 链 + 分配 ID 位 + 放开本线
    rm_irq_handler(hook) ── 注销：摘链 + 视情况屏蔽
    irq_handle(irq) ── 触发：屏蔽 → 遍历链 → 条件放开 → ack
    enable_irq / disable_irq(hook) ── 按 hook 的 ID 位开关整条线
```

锚点：`minix3/minix/kernel/arch/i386/exception.c:exception_handler`、`:pagefault`、`:inkernel_disaster`、`minix3/minix/kernel/interrupt.c:put_irq_handler`、`:rm_irq_handler`、`:irq_handle`、`:enable_irq`、`:disable_irq`、`minix3/minix/kernel/system.c:cause_sig`。

### 2.1 ex_data 分类表：19 种异常到信号的映射（主走查一）

分流器做的第一件事是查表。表与表项的类型一起定义在 `exception.c` 的头部：

```c
struct ex_s {
	char *msg;
	int signum;
	int minprocessor;
};

static struct ex_s ex_data[] = {
	{ "Divide error", SIGFPE, 86 },
	{ "Debug exception", SIGTRAP, 86 },
	{ "Nonmaskable interrupt", SIGBUS, 86 },
	{ "Breakpoint", SIGEMT, 86 },
	{ "Overflow", SIGFPE, 86 },
	{ "Bounds check", SIGFPE, 186 },
	{ "Invalid opcode", SIGILL, 186 },
	{ "Coprocessor not available", SIGFPE, 186 },
	{ "Double fault", SIGBUS, 286 },
	{ "Coprocessor segment overrun", SIGSEGV, 286 },
	{ "Invalid TSS", SIGSEGV, 286 },
	{ "Segment not present", SIGSEGV, 286 },
	{ "Stack exception", SIGSEGV, 286 },	/* STACK_FAULT already used */
	{ "General protection", SIGSEGV, 286 },
	{ "Page fault", SIGSEGV, 386 },		/* not close */
	{ NULL, SIGILL, 0 },			/* probably software trap */
	{ "Coprocessor error", SIGFPE, 386 },
	{ "Alignment check", SIGBUS, 386 },
	{ "Machine check", SIGBUS, 386 },
	{ "SIMD exception", SIGFPE, 386 },
};
```

（区间复算：`sed -n '13,40p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 28。锚点：`minix3/minix/kernel/arch/i386/exception.c` 的 `struct ex_s` 与 `ex_data` 定义段。）

每个表项三个字段：`msg` 是异常名（打印用），`signum` 是这条异常折算成的信号号，`minprocessor` 标记这种异常从哪一代 x86 处理器开始存在。数组下标就是向量号——0 号下标是除零，14 号下标是缺页，一一对齐。把 20 个表项逐项拆开：

| 向量 | msg | signum | minprocessor | 语义 |
|---|---|---|---|---|
| 0 | Divide error | SIGFPE | 86 | 除法出错 |
| 1 | Debug exception | SIGTRAP | 86 | 调试陷阱（单步/断点命中） |
| 2 | Nonmaskable interrupt | SIGBUS | 86 | 不可屏蔽中断 |
| 3 | Breakpoint | SIGEMT | 86 | 断点指令（int 3） |
| 4 | Overflow | SIGFPE | 86 | into 检出的溢出 |
| 5 | Bounds check | SIGFPE | 186 | bound 指令越界 |
| 6 | Invalid opcode | SIGILL | 186 | 非法指令 |
| 7 | Coprocessor not available | SIGFPE | 186 | 协处理器不可用（#NM） |
| 8 | Double fault | SIGBUS | 286 | 双重错误 |
| 9 | Coprocessor segment overrun | SIGSEGV | 286 | 协处理器段越界 |
| 10 | Invalid TSS | SIGSEGV | 286 | TSS 无效 |
| 11 | Segment not present | SIGSEGV | 286 | 段不存在 |
| 12 | Stack exception | SIGSEGV | 286 | 栈异常 |
| 13 | General protection | SIGSEGV | 286 | 通用保护错 |
| 14 | Page fault | SIGSEGV | 386 | 缺页 |
| 15 | NULL | SIGILL | 0 | Intel 保留向量占位 |
| 16 | Coprocessor error | SIGFPE | 386 | x87 协处理器错误 |
| 17 | Alignment check | SIGBUS | 386 | 对齐检查 |
| 18 | Machine check | SIGBUS | 386 | 机器检查 |
| 19 | SIMD exception | SIGFPE | 386 | SIMD 浮点异常 |

（逐行复算：`sed -n '20,39p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 20。锚点：`minix3/minix/kernel/arch/i386/exception.c` 的 `ex_data` 定义段。）

这张表有三处值得停下来核对的地方，每处都能从源码行上直接指出。

第一，**除零映射的是 SIGFPE，不是 SIGILL**。整数除零被折进「浮点异常」看起来错位，但这是 Unix 的历史约定：SIGFPE 的名字来自浮点运算异常，而它的语义早已扩成「算术运算异常」——除零、溢出（向量 4）、边界越界（向量 5）全部落在 SIGFPE，向量 7 的协处理器不可用、向量 16 的 x87 错误与向量 19 的 SIMD 浮点异常也是。六种异常共用一个信号号，用户态收到 SIGFPE 时需要另查原因。

第二，**minprocessor 列不是运行时筛选条件**。86/186/286/386 四个值记录每种异常「从哪代处理器开始存在」——bound 指令与非法指令异常从 80186 开始，双重错误从 80286 开始，缺页从 80386 开始。整个文件里对这一列的唯一提及是 `ex_s` 结构体的字段声明本身（`exception.c:16`），表初始化按位置写字面量，没有任何代码读它：现代 CPU 全部支持这 19 种异常，这一列是历史注解，不是筛选逻辑。

第三，**向量 15 的 NULL 槽是占位，不是表尾**。它住在表中间（后面还有 4 个表项），作用是把数组下标与向量号的对齐维持下去：15 号向量是 Intel 保留槽，现实中没有对应的异常，但数组必须为它留出位置。`inkernel_disaster` 里那句 `if (ep->msg == NULL) printf("Intel-reserved exception %d")` 就是给这个槽准备的打印分支。它的 `signum` 直接写成 SIGILL——假如一个软件陷阱真的落进这个向量，用户态进程收到的是 SIGILL，这个值来自表项自己，不需要任何特殊分支。

反过来推演这张表的结构：假如把 NULL 槽删掉、把表压成 19 项「紧凑」的样子，会发生什么？数组下标从此不再等于向量号——16 号向量（协处理器错误）会查到原 17 号的「Alignment check/SIGBUS」，17 号查到原 18 号的「Machine check/SIGBUS」——信号值凑巧还对，条目名已经错位，18 号「机器检查」的进程会收到 SIMD 的 SIGFPE；更糟的是 19 号向量查到的是数组界外，读到什么全凭内存内容。下标即向量的对齐关系是这张表唯一的索引协议，占位槽是协议的一部分，删不得。

### 2.2 exception_handler：分流主干（主走查二）

分流主干全函数如下（104 行，全函数逐字）：

```c
void exception_handler(int is_nested, struct exception_frame * frame)
{
/* An exception or unexpected interrupt has occurred. */
  register struct ex_s *ep;
  struct proc *saved_proc;

  /* Save proc_ptr, because it may be changed by debug statements. */
  saved_proc = get_cpulocal_var(proc_ptr);
  
  ep = &ex_data[frame->vector];

  if (frame->vector == 2) {		/* spurious NMI on some machines */
	printf("got spurious NMI\n");
	return;
  }

  /*
   * handle special cases for nested problems as they might be tricky or filter
   * them out quickly if the traps are not nested
   */
  if (is_nested) {
	/*
	 * if a problem occured while copying a message from userspace because
	 * of a wrong pointer supplied by userland, handle it the only way we
	 * can handle it ...
	 */
	if (((void*)frame->eip >= (void*)copy_msg_to_user &&
			(void*)frame->eip <= (void*)__copy_msg_to_user_end) ||
			((void*)frame->eip >= (void*)copy_msg_from_user &&
			(void*)frame->eip <= (void*)__copy_msg_from_user_end)) {
		switch(frame->vector) {
		/* these error are expected */
		case PAGE_FAULT_VECTOR:
		case PROTECTION_VECTOR:
			frame->eip = (reg_t) __user_copy_msg_pointer_failure;
			return;
		default:
			panic("Copy involving a user pointer failed unexpectedly!");
		}
	}

	/* Pass any error resulting from restoring FPU state, as a FPU
	 * exception to the process.
	 */
	if (((void*)frame->eip >= (void*)fxrstor &&
			(void *)frame->eip <= (void*)__fxrstor_end) ||
			((void*)frame->eip >= (void*)frstor &&
			(void *)frame->eip <= (void*)__frstor_end)) {
		frame->eip = (reg_t) __frstor_failure;
		return;
	}

  	if(frame->vector == DEBUG_VECTOR
		&& (saved_proc->p_reg.psw & TRACEBIT)
		&& (saved_proc->p_seg.p_kern_trap_style == KTS_NONE)) {
		/* Getting a debug trap in the kernel is legitimate
		 * if a traced process entered the kernel using sysenter
		 * or syscall; the trap flag is not cleared then.
		 *
		 * It triggers on the first kernel entry so the trap
		 * style is still KTS_NONE.
		 */

		frame->eflags &= ~TRACEBIT;

		return;

		/* If control passes, this case is not recognized as legitimate
		 * and we panic later on after all.
		 */
	}
  }

  if(frame->vector == PAGE_FAULT_VECTOR) {
	pagefault(saved_proc, frame, is_nested);
	return;
  }

  /* If an exception occurs while running a process, the is_nested variable
   * will be zero. Exceptions in interrupt handlers or system traps will make
   * is_nested non-zero.
   */
  if (is_nested == 0 && ! iskernelp(saved_proc)) {
#if 0
	{

  		printf(
  "vec_nr= %d, trap_errno= 0x%lx, eip= 0x%lx, cs= 0x%x, eflags= 0x%lx\n",
			frame->vector, (unsigned long)frame->errcode,
			(unsigned long)frame->eip, frame->cs,
			(unsigned long)frame->eflags);
		proc_stacktrace(saved_proc);
	}

#endif
	cause_sig(proc_nr(saved_proc), ep->signum);
	return;
  }

  /* Exception in system code. This is not supposed to happen. */
  inkernel_disaster(saved_proc, frame, ep, is_nested);

  panic("return from inkernel_disaster");
}
```

（区间复算：`sed -n '180,283p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 104。锚点：`minix3/minix/kernel/arch/i386/exception.c:exception_handler`、`:ex_data`、`:pagefault`、`:inkernel_disaster`。）

函数的判定次序是分层的，从上到下依次是：先看事件本身有没有特殊身份（伪 NMI）、再看有没有预登记的恢复区（嵌套分支的三段）、再按向量分流（缺页）、最后按来源分流（用户进程转信号，其余兜底）。（`is_nested` 的判定来源在节末的汇编段展开；这里先用它的语义。）把这条次序画成分流路径图：

```
exception_handler(is_nested, frame)
  │ saved_proc = proc_ptr          ← 记下被打断的进程（后面全靠它）
  │ ep = &ex_data[vector]          ← 查分类表
  ├─ vector == 2 ？ ── 是 → printf("got spurious NMI") → return
  ├─ is_nested ？ ── 是（内核执行中又撞异常）：
  │    ├─ eip 落在 copy_msg_to_user/from_user 区？
  │    │    └─ 向量 14/13 → eip = __user_copy_msg_pointer_failure → return
  │    │       其他向量 → panic（拷贝函数区不该有别的异常）
  │    ├─ eip 落在 fxrstor/frstor 区？ ── 是 → eip = __frstor_failure → return
  │    └─ 向量 1 + 保存的 PSW 有追踪位 + 进入样式 KTS_NONE？
  │         └─ 是 → 清帧里的追踪位 → return
  ├─ vector == 14 ？ ── 是 → pagefault(saved_proc, frame, is_nested) → return
  ├─ is_nested == 0 且 !iskernelp(saved_proc)？ ── 是（用户进程）
  │    └─ cause_sig(proc_nr(saved_proc), ep->signum) → return
  └─ 其余 → inkernel_disaster(saved_proc, frame, ep, is_nested) → panic
```

逐段解读这份主干。开头两行是簿记：`saved_proc` 抓住 CPU 本地变量的 `proc_ptr`——注释特意说明「因为调试语句可能改它」，后面所有按归属处置的动作都要用这个指针；`ep` 指向分类表项，信号号从这里取。

第一段，伪 NMI：向量 2 在某些机器上会冒出没有真实来源的 NMI，C 的处置是打印一行然后返回——不消费表项、不看归属、不 panic，就当没发生过。这条分支放在最前面，因为它不需要任何上下文。

第二段是整个函数里最重的部分：`is_nested` 为真，说明**内核正在执行时又撞上了一个异常**。这三种情况各防一件事。判定用的第三条件里那个 `KTS_NONE` 是进程的内核陷入样式记录（`os/arch/src/arch/trap_style.rs:enum TrapStyle`；[10-switch-to-user.md](10-switch-to-user.md) §1.2/§4.4 的对位——进程还没真正在内核里记过账时它保持 KTS_NONE）。

第一种，拷贝函数区命中。`copy_msg_to_user`/`copy_msg_from_user` 是内核代用户进程搬消息的两个函数——[12-ipc-core.md](12-ipc-core.md) §2.3 的发送侧拷入（落 `p_sendmsg`）与直接交付（落 `p_delivermsg`）、[同篇](12-ipc-core.md) §2.7 的交付拷出、13 篇的内核调用拷入/回写，全都经由它们（`minix3/minix/kernel/proc.c` 的 `mini_send` 与 `delivermsg`、`minix3/minix/kernel/system.c:kernel_call`）。它们的契约写在 `klib.S` 的注释里：

```c
 * This function from the callers point of view either succeeds or returns an
 * error which gives the caller a chance to respond accordingly. In fact it
 * either succeeds or if it generates a pagefault, general protection or other
 * exception, the trap handler has to redirect the execution to
 * __user_copy_msg_pointer_failure where the error is reported to the caller
 * without resolving the pagefault. It is not kernel's problem to deal with
 * wrong pointers from userspace and the caller should return an error to
 * userspace as if wrong values or request were passed to the kernel
```

（区间复算：`sed -n '227,234p' minix3/minix/kernel/arch/i386/klib.S | wc -l` = 8。锚点：`minix3/minix/kernel/arch/i386/klib.S:copy_msg_to_user`、`:copy_msg_from_user`。）

契约的意思是：用户给了一个坏指针，拷贝函数不负责解决——陷入的异常处理程序要把执行流折到 `__user_copy_msg_pointer_failure`，让拷贝以「失败」收场，由调用方把错误还给用户。分流器照契约办事：eip 落在这两个函数的地址区间内且异常是缺页或保护错（向量 14/13），就把帧里的 eip 改写成恢复标签的地址后返回——中断返回时 CPU 从恢复标签继续执行，拷贝函数以失败身份退出。恢复标签本身只有两条指令：

```asm
ENTRY(__user_copy_msg_pointer_failure)
	movl	$-1, %eax
	ret
```

（区间复算：`sed -n '342,344p' minix3/minix/kernel/arch/i386/klib.S | wc -l` = 3。锚点：`minix3/minix/kernel/arch/i386/klib.S:__user_copy_msg_pointer_failure`。）

`movl $-1, %eax; ret`——拷贝函数返回 -1，调用方拿到失败码，进程不死、内核不死。区间判定只认这两个向量是有意收窄的：`switch` 的 default 臂直接 panic——拷贝函数区的代码路径固定，撞上缺页与保护错之外的异常说明内核自己出了问题，这不属于「替用户兜底」的范畴。这个 eip 重定向的消费点在 [12-ipc-core.md](12-ipc-core.md) §2.3 闭合：交付拷贝路径把「拷贝失败」当作一次普通的 EFAULT 处理，进程收到错误码继续活着。

第二种，FPU 状态恢复区命中。`fxrstor`/`frstor` 是恢复浮点状态的两个函数，进程的保存浮点状态坏了会让它们撞异常。处置与拷贝区同形：eip 折到 `__frstor_failure`，它返回 1 表示失败：

```asm
ENTRY(__frstor_failure)
	mov	$1, %eax
	ret
```

（区间复算：`sed -n '515,517p' minix3/minix/kernel/arch/i386/klib.S | wc -l` = 3。锚点：`minix3/minix/kernel/arch/i386/klib.S:__frstor_failure`。）

注释把这层意图写明了——「Pass any error resulting from restoring FPU state, as a FPU exception to the process」：恢复浮点状态的失败要变成**这个进程的**浮点异常，而不是内核的灾难。FPU 状态的保存与恢复本体归 `31-fpu-context-switching` 方向（尚未重生成），本篇只立这个入口位。

第三种，入口单步。三个条件同时成立才算合法：向量是 1（调试陷阱）、保存的 PSW 里追踪位开着、进程的内核陷入样式还是 KTS_NONE（13 篇的 TrapStyle 对位——进程还没真正在内核里记过账）。注释解释了场景：被追踪的进程用 sysenter/syscall 进入内核时硬件不自动清追踪位，于是在内核执行的第一条指令上撞出单步陷阱。处置是只把帧里的追踪位清掉然后返回——CPU 中断返回时带着清过位的标志继续跑，进程不死。三个条件缺一不可：不满足任何一条的内核内调试陷阱都落到最后兜底。

两种 eip 重定向防的是同一件事的两个变体：**内核不能因为用户的错而死**。反过来推演：假如不做重定向，用户进程递来一个坏指针，内核代它拷贝时撞上缺页，嵌套异常一路落进 `inkernel_disaster`，整台机器 panic——死因是一个用户态进程传了个野指针。重定向把「用户的错」折回给用户（拷贝失败、进程拿到 EFAULT），把「内核的错」（拷贝区撞上非预期向量）留给 panic。分界线画在「谁导致的异常」上。

第三段，缺页专用通道：向量 14 不走信号、不走兜底，整只进 `pagefault`——它有自己的一套判定，下一节展开。

第四段，按来源分流。到这里还没出去的异常，要么是嵌套的（无论被打断的是谁），要么是非嵌套但发生在内核进程里。判定 `is_nested == 0 && !iskernelp(saved_proc)` 为真——异常发生在一个用户进程的用户态执行中——就调 `cause_sig(proc_nr(saved_proc), ep->signum)`：信号号从分类表来，归属者从 saved_proc 来。`cause_sig` 的函数头注释把它的语义定完整了：

```c
/*===========================================================================*
 *				cause_sig				     *
 *===========================================================================*/
void cause_sig(proc_nr_t proc_nr, int sig_nr)
{
/* A system process wants to send signal 'sig_nr' to process 'proc_nr'.
 * Examples are:
 *  - HARDWARE wanting to cause a SIGSEGV after a CPU exception
 *  - TTY wanting to cause SIGINT upon getting a DEL
 *  - FS wanting to cause SIGPIPE for a broken pipe
 * Signals are handled by sending a message to the signal manager assigned to
 * the process. This function handles the signals and makes sure the signal
 * manager gets them by sending a notification. The process being signaled
 * is blocked while the signal manager has not finished all signals for it.
 * Race conditions between calls to this function and the system calls that
 * process pending kernel signals cannot exist. Signal related functions are
 * only called when a user process causes a CPU exception and from the kernel
 * process level, which runs to completion.
 */
```

（区间复算：`sed -n '386,404p' minix3/minix/kernel/system.c | wc -l` = 19。锚点：`minix3/minix/kernel/system.c:cause_sig`。）

函数体做的事：查出进程的信号管理器（通常是 PM），把信号记入进程的待决信号集，置 `RTS_SIGNALED | RTS_SIG_PENDING` 位，向信号管理器发 SIGKSIG 通知。进程在信号管理器处理完之前停在 `RTS_SIGNALED` 上（[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 讲这个位怎么把它摘出就绪队列）。信号的用户态投递与 handler 跳板归 `19-syscall-signal` 方向（尚未重生成），本篇只立「异常路径在这里转信号」这个调用点。

最后一段是兜底：走到这里的异常发生在内核自己的执行中（嵌套、或内核进程、或不满足任何恢复条件），`inkernel_disaster` 打印诊断信息后 panic：

```c
static void inkernel_disaster(struct proc *saved_proc,
	struct exception_frame * frame, struct ex_s *ep,
	int is_nested)
{
#if USE_SYSDEBUG
  if(ep) {
	if (ep->msg == NULL)
		printf("\nIntel-reserved exception %d\n", frame->vector);
	  else
		printf("\n%s\n", ep->msg);
  }

  printf("cpu %d is_nested = %d ", cpuid, is_nested);

  printf("vec_nr= %d, trap_errno= 0x%x, eip= 0x%x, "
	"cs= 0x%x, eflags= 0x%x trap_esp 0x%08x\n",
	frame->vector, frame->errcode, frame->eip,
	frame->cs, frame->eflags, frame);
  printf("KERNEL registers :\n");
#define REG(n) (((u32_t *)frame)[-n])
  printf(
		  "\t%%eax 0x%08x %%ebx 0x%08x %%ecx 0x%08x %%edx 0x%08x\n"
		  "\t%%esp 0x%08x %%ebp 0x%08x %%esi 0x%08x %%edi 0x%08x\n",
		  REG(1), REG(2), REG(3), REG(4),
		  REG(5), REG(6), REG(7), REG(8));

  { 
  	reg_t k_ebp = REG(6);
  	printf("KERNEL stacktrace, starting with ebp = 0x%lx:\n", k_ebp);
  	proc_stacktrace_execute(proc_addr(SYSTEM), k_ebp, frame->eip);
  }

  if (saved_proc) {
	  printf("scheduled was: process %d (%s), ", saved_proc->p_endpoint, saved_proc->p_name);
	  printf("pc = 0x%x\n", (unsigned) saved_proc->p_reg.pc);
	  proc_stacktrace(saved_proc);

	  panic("Unhandled kernel exception");
  }

  /* in an early stage of boot process we don't have processes yet */
  panic("exception in kernel while booting, no saved_proc yet");
#endif /* USE_SYSDEBUG */
}
```

（区间复算：`sed -n '132,175p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 44。锚点：`minix3/minix/kernel/arch/i386/exception.c` 的 `inkernel_disaster` 定义段。）

它打印四样东西：分类表项的异常名（NULL 槽打「Intel-reserved exception」加向量号）、帧的关键字段（向量号、错误码、eip、CS、标志）、内核寄存器组、以 SYSTEM 的栈为起点的内核栈回溯；若 `saved_proc` 非空还打印被调度进程的名字与栈回溯。打印完两条路都是 panic：有 saved_proc 走「Unhandled kernel exception」，没有（启动早期还没有进程）走「exception in kernel while booting」。整个 `#if USE_SYSDEBUG` 包裹意味着关掉这个开关（默认构建开启：`kernel/Makefile` 带 `-DUSE_SYSDEBUG=1`）时整个函数体——打印与两条 panic——都被裁空；内核异常的收场由分流器最后那句 `panic("return from inkernel_disaster")` 兜住——按类型这函数不该返回，返回即异常。

还剩一个入口问题：`is_nested` 这个参数是谁判定的？答案在汇编。`mpx.S` 的异常公共入口：

```asm
exception_entry:
	/*
	 * check if it is a nested trap by comparing the saved code segment
	 * descriptor with the kernel CS first
	 */
	TEST_INT_IN_KERNEL(12, exception_entry_nested)

exception_entry_from_user:
	SAVE_PROCESS_CTX(8, KTS_INT_HARD)

	/* stop user process cycles */
	push	%ebp
	/* for stack trace clear %ebp */
	movl	$0, %ebp
	call	_C_LABEL(context_stop)
	add	$4, %esp

	/*
	 * push a pointer to the interrupt state pushed by the cpu and the
	 * vector number pushed by the vector handler just before calling
	 * exception_entry and call the exception handler.
	 */
	push	%esp
	push	$0	/* it's not a nested exception */
	call 	_C_LABEL(exception_handler)

	jmp	_C_LABEL(switch_to_user)

exception_entry_nested:

	pusha
	mov	%esp, %eax
	add	$(8 * 4), %eax
	push	%eax
	pushl	$1			/* it's a nested exception */
	call	_C_LABEL(exception_handler)
	add	$8, %esp
	popa

	/* clear the error code and the exception number */
	add	$8, %esp
	/* resume execution at the point of exception */
	iret
```

（区间复算：`sed -n '347,389p' minix3/minix/kernel/arch/i386/mpx.S | wc -l` = 43。锚点：`minix3/minix/kernel/arch/i386/mpx.S:exception_entry`、`:exception_entry_nested`。）

嵌套判定就是开头那个宏展开的比较：

```c
#define TEST_INT_IN_KERNEL(displ, label)	\
	cmpl	$KERN_CS_SELECTOR, displ(%esp)	;\
	je	label				;
```

（区间复算：`sed -n '19,21p' minix3/minix/kernel/arch/i386/sconst.h | wc -l` = 3。锚点：`minix3/minix/kernel/arch/i386/sconst.h:TEST_INT_IN_KERNEL`。）

把异常帧里 CPU 压栈的段选择子与内核段选择子比较：相等说明被打断的是内核自己（内核里又撞了异常，嵌套），不等说明被打断的是用户进程。用户来源走 `exception_entry_from_user`：保存进程上下文、停用户进程的记账、压参数（帧指针 + 非嵌套标志 0）调 `exception_handler`，回来直接 `switch_to_user`——异常可能把进程停了（转信号），所以出去时重走调度。内核来源走 `exception_entry_nested`：只在栈上做个轻框架，压参数（帧指针 + 嵌套标志 1）调完处理函数后 `iret` 原地恢复——嵌套异常的三种合法结局（重定向、清追踪位、忽略）都是「就当没打断过」，被打断的内核执行流原样继续。

用户来源那条路上还有个本篇开头提过的分支：向量 7（#NM，协处理器不可用）的异常门在 `protect.c` 门表里指向的标签不是公共入口，而是 `mpx.S` 的 `copr_not_available`。这个标签自己比较内核 CS：用户来源直接 `SAVE_PROCESS_CTX` 后调 `copr_not_available_handler`（`minix3/minix/kernel/proc.c:copr_not_available_handler`，懒 FPU 恢复的本体，归 `31-fpu-context-switching` 方向）——向量 7 的用户态异常在汇编层就消费掉了，从不进 C 的分流器；内核来源压上向量号后跳 `exception_entry_nested`，作为嵌套异常处理。

### 2.3 pagefault：页错误转发 VM

缺页有独立的处理函数。全函数如下（82 行，全函数逐字）：

```c
static void pagefault( struct proc *pr,
			struct exception_frame * frame,
			int is_nested)
{
	int in_physcopy = 0, in_memset = 0;

	reg_t pagefaultcr2;
	message m_pagefault;
	int err;

	pagefaultcr2 = read_cr2();

#if 0
	printf("kernel: pagefault in pr %d, addr 0x%lx, his cr3 0x%lx, actual cr3 0x%lx\n",
		pr->p_endpoint, pagefaultcr2, pr->p_seg.p_cr3, read_cr3());
#endif

	in_physcopy = (frame->eip > (vir_bytes) phys_copy) &&
	   (frame->eip < (vir_bytes) phys_copy_fault);

	in_memset = (frame->eip > (vir_bytes) phys_memset) &&
	   (frame->eip < (vir_bytes) memset_fault);

	if((is_nested || iskernelp(pr)) &&
		catch_pagefaults && (in_physcopy || in_memset)) {
#if 0
		printf("pf caught! addr 0x%lx\n", pagefaultcr2);
#endif
		if (is_nested) {
			if(in_physcopy) {
				assert(!in_memset);
				frame->eip = (reg_t) phys_copy_fault_in_kernel;
			} else {
				frame->eip = (reg_t) memset_fault_in_kernel;
			}
		}
		else {
			pr->p_reg.pc = (reg_t) phys_copy_fault;
			pr->p_reg.retreg = pagefaultcr2;
		}
	
		return;
	}

	if(is_nested) {
		printf("pagefault in kernel at pc 0x%lx address 0x%lx\n",
			frame->eip, pagefaultcr2);
		inkernel_disaster(pr, frame, NULL, is_nested);
	}

	/* VM can't handle page faults. */
	if(pr->p_endpoint == VM_PROC_NR) {
		/* Page fault we can't / don't want to
		 * handle.
		 */
		printf("pagefault for VM on CPU %d, "
			"pc = 0x%x, addr = 0x%x, flags = 0x%x, is_nested %d\n",
			cpuid, pr->p_reg.pc, pagefaultcr2, frame->errcode,
			is_nested);
		proc_stacktrace(pr);
		printf("pc of pagefault: 0x%lx\n", frame->eip);
		panic("pagefault in VM");

		return;
	}

	/* Don't schedule this process until pagefault is handled. */
	RTS_SET(pr, RTS_PAGEFAULT);

	/* tell Vm about the pagefault */
	m_pagefault.m_source = pr->p_endpoint;
	m_pagefault.m_type   = VM_PAGEFAULT;
	m_pagefault.VPF_ADDR = pagefaultcr2;
	m_pagefault.VPF_FLAGS = frame->errcode;

	if ((err = mini_send(pr, VM_PROC_NR,
					&m_pagefault, FROM_KERNEL))) {
		panic("WARNING: pagefault: mini_send returned %d\n", err);
	}

	return;
}
```

（区间复算：`sed -n '49,130p' minix3/minix/kernel/arch/i386/exception.c | wc -l` = 82。锚点：`minix3/minix/kernel/arch/i386/exception.c:pagefault`、`:read_cr2`。）

函数按四层判定往下走，每层的条件与结局都不同。

第一层，先取故障地址：`read_cr2()` 读 CR2 寄存器——x86 在缺页时把出错的虚拟地址放进这个寄存器，它是后续一切判定的原料。开头那段 `#if 0` 的调试打印（打印进程端点、故障地址、它该有的 CR3 与实际 CR3）被注释留着，说明「地址对了但地址空间装错」是这类故障的常见怀疑对象。

第二层，捕获窗判定。两个布尔先把 eip 的位置钉出来：`in_physcopy` 是 eip 落在 `phys_copy` 与 `phys_copy_fault` 之间，`in_memset` 是落在 `phys_memset` 与 `memset_fault` 之间。然后复合条件 `(is_nested || iskernelp(pr)) && catch_pagefaults && (in_physcopy || in_memset)` 命中时走捕获路。三个条件的角色各不相同：`catch_pagefaults` 是计数变量（声明在 `minix3/minix/kernel/glo.h`，非零即开），`PHYS_COPY_CATCH` 宏把它 ++/--（可嵌套）：

```c
#define PHYS_COPY_CATCH(src, dst, size, a) {	\
	catch_pagefaults++;			\
	a = phys_copy(src, dst, size);		\
	catch_pagefaults--;			\
	}
```

（区间复算：`sed -n '10,14p' minix3/minix/kernel/vm.h | wc -l` = 5。锚点：`minix3/minix/kernel/vm.h:PHYS_COPY_CATCH`。）

`minix3/minix/kernel/arch/i386/memory.c` 的 VM 请求处理段则用直接赋值（进区前先断言为 0，再置 1；退出置 0）。首项 `(is_nested || iskernelp(pr))` 是资格条件——两项合起来圈定「内核侧执行流」：嵌套（无论被打断的是谁）或非嵌套但进程本身是内核进程。开关开着 + eip 落在拷贝/清零函数区 = 这是一次**替 VM 干活时撞上的缺页**——源页或目标页还没按需装入，而这正是 VM 想要的效果（它就是要内核在拷贝中把页碰进来）。结局是自愈而非转发：非嵌套时把进程的 pc 折到 `phys_copy_fault`、返回寄存器塞入 CR2 故障地址——`phys_copy` 以「返回第一个没拷成的地址」的方式结束；嵌套时把帧 eip 折到 `phys_copy_fault_in_kernel`。这两个标签与 `memset_fault`/`memset_fault_in_kernel` 都在 `minix3/minix/kernel/arch/i386/klib.S` 里（phys_copy 一对与 memset 一对各成一对，两对相隔百余行）。这条捕获路与 12 篇 §2.3 的内核副本锚是一对：消息拷贝把内容冻进内核槽靠的是「拷贝必成」，而拷贝必成靠的是 VM 先把双方页备齐、备齐动作本身用这条捕获路完成。

第三层，嵌套且没开捕获：内核自己（不在拷贝函数区）撞了缺页，直接 `inkernel_disaster`。注意这里传的分类表项是 NULL——缺页不走 `ex_data` 的信号号，`inkernel_disaster` 里 `ep` 为空时跳过异常名打印。

第四层，按归属分流。先拦 VM 自己：`pr->p_endpoint == VM_PROC_NR` 时打印帧摘要、栈回溯后 `panic("pagefault in VM")`。管内存的服务器自己缺页，转发给 VM 等于让它排队等自己服务——这个死结必须当场掐断。然后是正常路：用户进程缺页。三步动作连成一串：置 `RTS_PAGEFAULT`（注释「Don't schedule this process until pagefault is handled」——这个位让调度器不再选它，[11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2 的队列半）；构造 `m_pagefault` 消息（`m_type = VM_PAGEFAULT`、`VPF_ADDR = CR2`、`VPF_FLAGS = 错误码`，来源是故障进程自己的端点）；`mini_send` 直接发给 VM，失败即 panic。函数返回后回到分流器、回到 `switch_to_user`——故障进程停在 `RTS_PAGEFAULT` 上，VM 收到消息去服务缺页，服务完通过 `VMCTL_CLEAR_PAGEFAULT` 把位清掉（消费端在 [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3）：

```c
	case VMCTL_CLEAR_PAGEFAULT:
		assert(RTS_ISSET(p,RTS_PAGEFAULT));
		RTS_UNSET(p, RTS_PAGEFAULT);
		return OK;
```

（区间复算：`sed -n '33,36p' minix3/minix/kernel/system/do_vmctl.c | wc -l` = 4。锚点：`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`。）

断言先验位一定开着再清——生产端与消费端靠这一位配对。

缺页为什么必须转发而不是折信号？反过来推演：假如用户进程缺页直接 `cause_sig(SIGSEGV)`，第一个死的是写时复制的页。进程 fork 后第一次写共享页、第一次访问刚换入的页、第一次触碰尚未装入的页——这些全是合法操作，合法操作的下一步是 VM 补页让进程继续；折成 SIGSEGV 则这些进程全被误杀，连「进程永远无法合法地触发页入」这句话都成立——因为触发页入的入口本身就是这条转发路。SIGSEGV 只该给真正的非法访问，而「合法还是非法」的判定权在 VM 手里，不在异常处理器的手里。

### 2.4 interrupt.c：IRQ hook 链

中断侧的数据结构先看。hook 的类型定义在 `minix3/minix/kernel/type.h`：

```c
typedef struct irq_hook {
  struct irq_hook *next;		/* next hook in chain */
  int (*handler)(struct irq_hook *);	/* interrupt handler */
  int irq;				/* IRQ vector number */ 
  int id;				/* id of this hook */
  endpoint_t proc_nr_e;			/* (endpoint) NONE if not in use */
  irq_id_t notify_id;			/* id to return on interrupt */
  irq_policy_t policy;			/* bit mask for policy */
} irq_hook_t;
```

（区间复算：`sed -n '18,26p' minix3/minix/kernel/type.h | wc -l` = 9。锚点：`minix3/minix/kernel/type.h:irq_hook_t`。）

七个字段：`next` 串链，`handler` 是 C 函数指针，`irq` 记本 hook 挂在哪条线，`id` 是本 hook 在这条线上的身份位（一位，不是编号），`proc_nr_e` 记属主进程端点，`notify_id` 是通知时带给属主的位号，`policy` 是策略位。每条 IRQ 线一个链，链上的每个 hook 属于一个进程。注册函数：

```c
void put_irq_handler( irq_hook_t* hook, int irq,
  const irq_handler_t handler)
{
  int id;
  irq_hook_t **line;
  unsigned long bitmap;

  if( irq < 0 || irq >= NR_IRQ_VECTORS )
	panic("invalid call to put_irq_handler: %d",  irq);

  line = &irq_handlers[irq];

  bitmap = 0;
  while ( *line != NULL ) {
	if(hook == *line) return; /* extra initialization */
	bitmap |= (*line)->id;	/* mark ids in use */
	line = &(*line)->next;
  }

  /* find the lowest id not in use */
  for (id = 1; id != 0; id <<= 1)
  	if (!(bitmap & id)) break;

  if(id == 0)
  	panic("Too many handlers for irq: %d",  irq);
  
  hook->next = NULL;
  hook->handler = handler;
  hook->irq = irq;
  hook->id = id;
  *line = hook;

  /* And as last enable the irq at the hardware.
   *
   * Internal this activates the line or source of the given interrupt.
   */
  if((irq_actids[hook->irq] &= ~hook->id) == 0) {
	  hw_intr_used(irq);
	  hw_intr_unmask(hook->irq);
  }
}
```

（区间复算：`sed -n '29,69p' minix3/minix/kernel/interrupt.c | wc -l` = 41。锚点：`minix3/minix/kernel/interrupt.c:put_irq_handler`、`:irq_handlers`、`:hw_intr_unmask`。）

注册做四件事。第一，范围检查，越界直接 panic。第二，沿链走一遍：若这个 hook 已在链上，直接返回（注释「extra initialization」——重复注册是无害的幂等操作）；否则把链上所有 hook 的 `id` 位或进 bitmap，得到「这条线已被占用的位集合」。第三，从位 1 开始找最低的未占用位分给新 hook；32 位全满则 panic「Too many handlers」——一条线最多 32 个 hook，上限来自 `id` 的位宽。第四，新 hook 尾插进链，然后放开中断线：`irq_actids[irq] &= ~hook->id` 清掉（新 hook 的）活跃位后，若整条线的活跃位归零就 `hw_intr_used` + `hw_intr_unmask`——**注册动作本身就放线**，不是等第一次中断才放。`irq_actids` 是每条线的活跃位图（`minix3/minix/kernel/glo.h`），它记「哪些 hook 还没干完活」。

注销半：`rm_irq_handler` 按 `id` 从链上摘除并处理线状态——摘后线空则屏蔽并标记停用，否则若活跃位归零则放开：

```c
/* Unregister an interrupt handler.  */
void rm_irq_handler( const irq_hook_t* hook ) {
  const int irq = hook->irq; 
  const int id = hook->id;
  irq_hook_t **line;

  if( irq < 0 || irq >= NR_IRQ_VECTORS ) 
	panic("invalid call to rm_irq_handler: %d",  irq);

  /* remove the hook  */
  line = &irq_handlers[irq];
  while( (*line) != NULL ) {
	if((*line)->id == id) {
		(*line) = (*line)->next;
		if (irq_actids[irq] & id)
			irq_actids[irq] &= ~id;
    	}
    	else {
		line = &(*line)->next;
    	}
  }

  /* Disable the irq if there are no other handlers registered.
   * If the irq is shared, reenable it if there is no active handler.
   */
  if (irq_handlers[irq] == NULL) {
	hw_intr_mask(irq);
	hw_intr_not_used(irq);
  }
  else if (irq_actids[irq] == 0) {
	hw_intr_unmask(irq);
  }
}
```

（区间复算：`sed -n '74,106p' minix3/minix/kernel/interrupt.c | wc -l` = 33。锚点：`minix3/minix/kernel/interrupt.c:rm_irq_handler`。）

摘链按 `id`（不是按指针）匹配；「进程死时会自动摘钩」（上文 `generic_handler` 引文注释）背后就是这一半——Rust 侧对位是 `IrqManager::remove_hook`（§3.3 表、§4.3）。

触发从哪来？中断的入口在汇编——`hwint00`-`hwint07` 与 `hwint08`-`hwint15` 分别是 `hwint_master` 与 `hwint_slave` 两组宏的展开（后者尾部多一条从 PIC 应答），两条路径（打断用户态/空闲态与打断内核态）都汇进 `PIC_IRQ_HANDLER` → `irq_handle`；§4.3 的 BKL「继承或自旋」对位就对着这两条路径：

```c
#define hwint_master(irq) \
	TEST_INT_IN_KERNEL(4, 0f)					;\
									\
	SAVE_PROCESS_CTX(0, KTS_INT_HARD)				;\
	push	%ebp							;\
	movl	$0, %ebp	/* for stack trace */			;\
	call	_C_LABEL(context_stop)					;\
	add	$4, %esp						;\
	PIC_IRQ_HANDLER(irq)						;\
	movb	$END_OF_INT, %al					;\
	outb	$INT_CTL	/* reenable interrupts in master pic */	;\
	jmp	_C_LABEL(switch_to_user)				;\
									\
0:									\
	pusha								;\
	call	_C_LABEL(context_stop_idle)				;\
	PIC_IRQ_HANDLER(irq)						;\
	movb	$END_OF_INT, %al					;\
	outb	$INT_CTL	/* reenable interrupts in master pic */	;\
	CLEAR_IF(10*4(%esp))						;\
	popa								;\
	iret								;

```

（区间复算：`sed -n '74,96p' minix3/minix/kernel/arch/i386/mpx.S | wc -l` = 23。锚点：`minix3/minix/kernel/arch/i386/mpx.S` 的 `hwint_master` 宏。）

触发函数：

```c
void irq_handle(int irq)
{
  irq_hook_t * hook;

  /* here we need not to get this IRQ until all the handlers had a say */
  assert(irq >= 0 && irq < NR_IRQ_VECTORS);
  hw_intr_mask(irq);
  hook = irq_handlers[irq];

  /* Check for spurious interrupts. */
  if(hook == NULL) {
      static int nspurious[NR_IRQ_VECTORS], report_interval = 100;
      nspurious[irq]++;
      if(nspurious[irq] == 1 || !(nspurious[irq] % report_interval)) {
      	printf("irq_handle: spurious irq %d (count: %d); keeping masked\n",
		irq, nspurious[irq]);
	if(report_interval < INT_MAX/2)
		report_interval *= 2;
      }
      return;
  }

  /* Call list of handlers for an IRQ. */
  while( hook != NULL ) {
    /* For each handler in the list, mark it active by setting its ID bit,
     * call the function, and unmark it if the function returns true.
     */
    irq_actids[irq] |= hook->id;
    
    /* Call the hooked function. */
    if( (*hook->handler)(hook) )
      irq_actids[hook->irq] &= ~hook->id;
    
    /* Next hooked function. */
    hook = hook->next;
  }

  /* reenable the IRQ only if there is no active handler */
  if (irq_actids[irq] == 0)
	  hw_intr_unmask(irq);

  hw_intr_ack(irq);
}
```

（区间复算：`sed -n '116,158p' minix3/minix/kernel/interrupt.c | wc -l` = 43。锚点：`minix3/minix/kernel/interrupt.c:irq_handle`。）

触发的次序是硬约束：**先屏蔽本线**（`hw_intr_mask`）再干活——注释「here we need not to get this IRQ until all the handlers had a say」：这条线的后续中断要等链上每个 handler 都表过态。屏蔽之后查链头：链是空的，说明没有驱动注册这条线却来了中断——伪中断。处置不是 panic 而是计数：静态数组按线计数，第 1 次必打、之后每逢**当前**报告间隔的倍数打印一次，每次打印后间隔翻倍（初始 100、上限 INT_MAX/2；该间隔是**跨线共享**的单一静态变量，不是每线一份），并保持屏蔽直接返回——伪中断是线一级的噪声（未注册的线也会因控制器毛刺响铃），不是内核出错的证据，不值得反复打扰、也不能无声，更不该停机。链非空则遍历：每个 hook 先把它的 `id` 位置进 `irq_actids[irq]`（先标记「它还没干完」），再调 handler，返回真（干完了）就清掉自己的位。遍历完，**全部**活跃位归零才 `hw_intr_unmask` 放线——任何一个 handler 说「没干完」（返回假），这条线就保持屏蔽，直到它在别处被 `enable_irq` 放开。最后 `hw_intr_ack` 回应控制器。

遍历里「先置位再调用再按返回值清位」的次序值得单独看一眼。为什么先置位？设想反过来：先调 handler 再置位——handler 执行期间另一个 CPU 看到这条线的活跃位是 0，可能误判「没人干活」；而先置位的写法保证 handler 从被调用那一刻起就在位图上有名字。为什么按返回值清位而不是无条件清？返回假的 handler 在说「活没干完，别放线」——比如驱动还要等硬件状态，需要屏蔽整条线直到它显式 `enable_irq`。位图把「链上谁还没表态」记成机器可判的状态，放线判定 `irq_actids[irq] == 0` 因此是一行比较而不是一次遍历。

hook 链为什么是多 hook 设计？驱动的通知语义在 `generic_handler`（SYS_IRQCTL 注册的通用 handler）里：

```c
static int generic_handler(irq_hook_t * hook)
{
/* This function handles hardware interrupt in a simple and generic way. All
 * interrupts are transformed into messages to a driver. The IRQ line will be
 * reenabled if the policy says so.
 */
  int proc_nr;

  /* As a side-effect, the interrupt handler gathers random information by 
   * timestamping the interrupt events. This is used for /dev/random.
   */
  get_randomness(&krandom, hook->irq);

  /* Check if the handler is still alive.
   * If it's dead, this should never happen, as processes that die 
   * automatically get their interrupt hooks unhooked.
   */
  if(!isokendpt(hook->proc_nr_e, &proc_nr))
     panic("invalid interrupt handler: %d", hook->proc_nr_e);

  /* Add a bit for this interrupt to the process' pending interrupts. When 
   * sending the notification message, this bit map will be magically set
   * as an argument. 
   */
  priv(proc_addr(proc_nr))->s_int_pending |= (1 << hook->notify_id);

  /* Build notification message and return. */
  mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);
  return(hook->policy & IRQ_REENABLE);
}

#endif /* USE_IRQCTL */
```

（区间复算：`sed -n '143,174p' minix3/minix/kernel/system/do_irqctl.c | wc -l` = 32。锚点：`minix3/minix/kernel/system/do_irqctl.c:generic_handler`。块尾孤立的 `#endif /* USE_IRQCTL */` 是上文 `#if USE_IRQCTL` 的收尾——该 handler 随此开关编入。）

它把硬件中断变换成给属主进程的通知：先查属主端点还活着（死了 panic——注释说进程死时会自动摘钩，端点失效说明摘钩机制出了漏洞），然后把 `notify_id` 位置进属主的 `s_int_pending`，最后 `mini_notify(HARDWARE, 端点)` 发通知。注意这是 NOTIFY（[12-ipc-core.md](12-ipc-core.md) §2.5 的旁路语义：目标没在收也不等，通知记进位图等它来收），不是 SEND——中断上下文里发送者不能停，NOTIFY 的永不停恰好是中断需要的。返回值取 `hook->policy & IRQ_REENABLE`：策略开着就返回真（干完了、可放线），没开就返回假（保持屏蔽）。

开关函数一对，按 hook 的 ID 位操作整条线：

```c
void enable_irq(const irq_hook_t *hook)
{
  if((irq_actids[hook->irq] &= ~hook->id) == 0) {
    hw_intr_unmask(hook->irq);
  }
}

/* Return true if the interrupt was enabled before call.  */
int disable_irq(const irq_hook_t *hook)
{
  if(irq_actids[hook->irq] & hook->id)  /* already disabled */
    return 0;
  irq_actids[hook->irq] |= hook->id;
  hw_intr_mask(hook->irq);
  return TRUE;
}
```

（区间复算：`sed -n '161,176p' minix3/minix/kernel/interrupt.c | wc -l` = 16。锚点：`minix3/minix/kernel/interrupt.c:enable_irq`、`:disable_irq`。）

`enable_irq` 清自己的活跃位，全线归零才放线；`disable_irq` 置自己的活跃位并屏蔽整线，返回 TRUE 表示「是这次 newly disabled 的」。位的语义与 `irq_handle` 一致：活跃位是「还有 hook 没干完」，全零才能放。

反向推演单 hook 假设：假如一条线只许挂一个 hook，第二个注册同一 IRQ 线的驱动会被拒——而 x86 的线数（`minix3/minix/include/arch/i386/include/interrupt.h` 的 `NR_IRQ_VECTORS`，无 APIC 16 / 有 APIC 64 两档）在设备多于一打的机器上根本不够分（16 档尤甚）。hook 链把「一条线」与「一个属主」解耦：链上每个 hook 有自己的 ID 位、自己的属主、自己的策略，共享一条线的两个驱动各自被通知、各自表态，位图裁决全线的开与关。池子的大小同样有来源：`minix3/minix/kernel/config.h` 按 APIC 与否给两档：

```c
 */
#ifndef USE_APIC
#define NR_IRQ_HOOKS	  16		/* number of interrupt hooks */
#else
#define NR_IRQ_HOOKS	  64		/* number of interrupt hooks */
```

（区间复算：`sed -n '57,61p' minix3/minix/kernel/config.h | wc -l` = 5。锚点：`minix3/minix/kernel/config.h` 的 `NR_IRQ_HOOKS` 定义对。块首孤立的 ` */` 是上文注释收尾。）

### 2.5 本章归纳：异常/中断分流总表

把第 2 章的全部分流路收进一张表。判定是**层级**的：先判事件本身（伪 NMI）、再判恢复区（嵌套分支）、再判向量（缺页）、最后判来源（用户进程还是内核）；中断侧没有层级，只有一条链路。

| # | 事件 | 判定条件 | 归宿 | 走查节 |
|---|---|---|---|---|
| 1 | 异常·伪 NMI | 向量 2 | 打印一行，忽略 | §2.2 |
| 2 | 异常·内核代用户拷贝撞页/保护错 | 嵌套 + eip 在 copy_msg_to_user/from_user 区 + 向量 14/13 | eip 折 `__user_copy_msg_pointer_failure`，拷贝以失败返回 | §2.2 |
| 3 | 异常·FPU 状态恢复撞异常 | 嵌套 + eip 在 fxrstor/frstor 区 | eip 折 `__frstor_failure`，折成进程的 FPU 异常 | §2.2 |
| 4 | 异常·入口单步 | 嵌套 + 向量 1 + PSW 追踪位 + KTS_NONE | 清帧内追踪位，继续跑 | §2.2 |
| 5 | 异常·替 VM 拷贝撞缺页 | `catch_pagefaults` 开 + eip 在 phys_copy/memset 区 | 非嵌套：pc 折 `phys_copy_fault` + 返回 CR2；嵌套：折 `*_in_kernel` 标签 | §2.3 |
| 6 | 异常·内核撞缺页（非捕获） | 嵌套且不在捕获窗，或 VM 自己 | 前者 `inkernel_disaster`；后者打印后 panic | §2.3 |
| 7 | 异常·用户进程缺页 | 向量 14 + 非嵌套 + 非 VM | 置 `RTS_PAGEFAULT` + `VM_PAGEFAULT` 消息发给 VM | §2.3 |
| 8 | 异常·用户进程其他异常 | 非嵌套 + 非内核进程 | `cause_sig`，信号号查 `ex_data` | §2.2 |
| 9 | 异常·其余（内核执行中） | 以上全部未命中 | `inkernel_disaster` + panic | §2.2 |
| 10 | 中断 | hwint 汇编入口 | `irq_handle`：屏蔽 → hook 链逐个置位/调用/按返回清位 → 全零放线 → ack | §2.4 |
| 11 | 中断·伪中断 | 链头为空 | 按线计数，第 1 次与倍频次打印，保持屏蔽 | §2.4 |

（行数复算：本表 11 个数据行，行号即第一列的 #1-#11。表的层级次序对应 `exception_handler` 的源码次序——表行 2-4 在 `is_nested` 分支内，行 5-7 在 `pagefault` 内，行 8-9 在主干尾部。）

---
## 3. Rust 设计决策

### 3.1 异常分流的结构化：ExceptionClass/ExceptionSignal/ExceptionOutcome（→§2.1/§2.2）

C 里真实存在的语义与负担是：`ex_data` 表把「异常分类」与「信号映射」压在同一张表里（§2.1），而 `exception_handler` 把「决定怎么处置」与「执行处置」揉在同一个函数体里——转信号是直接调 `cause_sig`，转发是直接调 `pagefault`，兜底是直接调 `inkernel_disaster`（§2.2 的路径图）。Rust 把这两层都拆开：分类、信号、处置结果各自成枚举，分发器只产出决策，执行交给调用方。三个枚举与分发器的定义：

```rust
/// Exception classification for dispatch.
///
/// C: ex_data[] — exception.c:19-39
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionClass {
    NonMaskableInterrupt,
    PageFault,
    Debug,
    Signal(ExceptionSignal),
}

/// Signal type derived from exception classification.
///
/// Subset of POSIX signals that x86 exceptions map to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionSignal {
    Fpe,
    Ill,
    Segv,
    Bus,
    Emt,
    Trap,
}

/// Outcome of exception dispatch.
///
/// Replaces C's mix of returns, panics, and direct function calls
/// with a structured enum that the caller can pattern-match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionOutcome {
    SpuriousNmi,
    /// User-mode #NM (vector 7): the process touched FPU state while
    /// CR0.TS was set. The lazy-FPU restore stage acts on this outcome
    /// (save previous fpu_owner, restore current process, clts).
    /// C: copr_not_available_handler() — proc.c:1922-1958
    FpuTrap,
    Signal(ExceptionSignal),
    ForwardToVm(VmPagefaultIn),
    RedirectToRecovery(RecoveryPoint),
    PhysCopyFault { fault_addr: VirBytes },
    VmPageFault,
    KernelPanic(InterruptVector),
    ClearTrapFlag,
}
```

（区间复算：`sed -n '26,69p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 44。锚点：`os/arch/src/arch/exception_dispatcher.rs:enum ExceptionClass`、`:enum ExceptionSignal`、`:enum ExceptionOutcome`。）

`ExceptionSignal` 六个变体恰好是 §2.1 表格 signum 列出现过的全部信号——C 的表里有六种取值（SIGFPE/SIGTRAP/SIGBUS/SIGEMT/SIGILL/SIGSEGV），Rust 用同名的六个变体承接。`ExceptionOutcome` 九个变体把 §2.2/§2.3 路径图的每个终点各占一个：`Signal`（转信号）、`ForwardToVm`（转发 VM）、`RedirectToRecovery`/`PhysCopyFault`（内核自愈的两个形状）、`ClearTrapFlag` 与 `SpuriousNmi`（两个忽略形：嵌套调试清位、伪 NMI 打印）、`FpuTrap`（懒 FPU 入口）、`VmPageFault` 与 `KernelPanic`（停机的两个形状）。分发器结构体 `ExceptionDispatcher` 的入口签名（全函数体在 §4.1 逐字展示）：

```rust
    pub fn handle(
        frame: &mut EA::Frame,
        is_nested: bool,
        is_vm: bool,
        fault_ctx: FaultContext,
        is_traced: bool,
        kern_trap_style: TrapStyle,
    ) -> ExceptionOutcome {
```

（签名示意：`sed -n '93,100p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 8。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn handle`、`:struct ExceptionDispatcher`。）

「决策与执行分离」的代价与收益都摆在签名上。收益：`handle` 是纯决策——它只读帧、只产出裁决（重定向、清位以结果枚举携带，落帧在执行臂）但不发信号、不发消息、不 panic，于是全部九条路径都能在宿主测试里用假帧驱动（第 5 章）；C 的 `exception_handler` 想测「转信号」就得连着 `cause_sig` 与信号管理器一起跑。代价：调用方要接住九个变体并各自执行——x86 的执行体在 `os/kernel/src/trap_dispatch.rs` 的分发体里，`match` 九臂一条不少（§4.2）。

取舍表：

| 选项 | 内容 | 否掉什么 | 代价 |
|---|---|---|---|
| 三枚举 + `ExceptionOutcome`（采纳） | 分类/信号/结果分层，`handle` 只出决策 | C 的表+直呼副作用一体写法 | 调用方九臂 `match`；决策与执行隔了一层间接 |
| 单表直译 `ex_data` | Rust 数组照抄 20 项三元组 | —— | 新增异常分类要改表结构；`msg` 文案列与 `minprocessor` 列在 Rust 里无消费者，抄过来即死数据 |
| `handle` 直接执行副作用 | 照抄 C 的函数形状 | —— | 宿主测试必须连带信号/IPC 全套；九条路径里只有少数可测 |

关于 `ExceptionClass` 要如实记一笔现状：这个枚举声明了「事件分类」的词表（NMI/缺页/调试/信号类），也随 `os/arch/src/lib.rs` 转出，但当前 `handle` 的分支按向量号与 `is_user` 直接判定，没有任何代码消费 `ExceptionClass`——词表在、接线未完成。它的去向属于后续代码卓越轮的判断（补接线或删除），本篇按现状记录，不预写结论。

反向推演单表直译：假如照抄 C 的数组，向量→信号的映射会再次变成「数据」而非「代码」——编译期无法穷尽检查，向量 15 的占位槽、越界向量的兜底都得靠运行时约定；`classify_signal` 用 `match` 写成代码后，漏一个向量是编译错（穷尽性检查），对账靠测试逐值钉死（§5.2 的 `classify_all_vectors`）。C 的表在 C 里是唯一合理形状（向量→信号的映射只写一遍、由运行时查）；Rust 的 match 在 Rust 里是唯一合理形状（类型系统接管穷尽性）——两种形状各自的代价在自己的语言里最小。

### 3.2 page_fault 的 Rust 对位（→§2.3）

C 里真实存在的负担是：`pagefault` 尾部把三件不同性质的事压在八行里——置 `RTS_PAGEFAULT` 位（进程状态机）、构造消息（协议格式）、`mini_send`（IPC 动作），三者共用局部变量 `m_pagefault`（§2.3 逐字）。Rust 拆成三个独立助手，各自可测：置位与地址记录的原语归 `os/kernel/src/page_fault.rs:set_pagefault_pending`，消息构造归 `:fn build_vm_pagefault_msg`，发送归 trap 分发体里的 `os/kernel/src/trap_dispatch.rs:fn forward_pagefault_to_vm`（§4.2 逐段走查）。生产路径只串接后两个；置位一步走的是带出队半的 `rts_set` 包装层，`set_pagefault_pending` 的去向见本节末的披露。拆分之后还有一层与 C 的结构性差异要交代：C 的转发条件里有 `catch_pagefaults` 计数窗与两个地址窗口（`in_physcopy`/`in_memset`，§2.3 第二层），Rust 用显式的上下文枚举替代这对开关+窗口：

```rust
/// Fault recovery context for nested exceptions.
///
/// Replaces C's address-range comparison (`frame->eip > phys_copy && ...`)
/// with an explicit context enum. Set before entering recoverable
/// operations, cleared on exit.
///
/// C: catch_pagefaults + address range comparison — exception.c:59-73
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultContext {
    Normal,
    PhysCopy,
    Memset,
    UserCopyMsg,
    FpuRestore,
}

/// Fault recovery points for nested exception handling.
///
/// C: phys_copy_fault_in_kernel, memset_fault_in_kernel,
///    __user_copy_msg_pointer_failure, __frstor_failure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPoint {
    PhysCopyFaultInKernel,
    MemsetFaultInKernel,
    UserCopyMsgFailure,
    FpuRestoreFailure,
}
```

（区间复算：`sed -n '101,127p' os/arch/src/arch/exception.rs | wc -l` = 27。锚点：`os/arch/src/arch/exception.rs:enum FaultContext`、`:enum RecoveryPoint`。）

`FaultContext` 五个变体对应 C 里四个上下文类别（共六段地址区间：copy_msg 两个、fxrstor/frstor 两个、phys_copy 一个、phys_memset 一个）加正常态：`PhysCopy`/`Memset` 对应 §2.3 第二层的两个窗口，`UserCopyMsg`/`FpuRestore` 对应 §2.2 嵌套分支的前两种重定向；`RecoveryPoint` 四个变体对应四个恢复标签（`__user_copy_msg_pointer_failure`/`__frstor_failure`/`phys_copy_fault_in_kernel`/`memset_fault_in_kernel`）。判定方式从「eip 落在哪个地址区间」换成「内核当前在哪个上下文里执行」：C 靠比较 eip 与函数地址，Rust 靠进入可恢复操作前显式登记上下文（`os/arch/src/arch/exception.rs:struct FaultContextTracker` 的 `enter`/`leave`）。上下文枚举比地址窗口多两样东西：它与架构无关（三架构里 aarch64/riscv64 在 C 侧没有可比的地址窗口——本仓 C 只有 x86 与 ARM32 两半，见 §1.5），且它在编译期可穷尽——五个变体各有去处，`recovery_point()` 把前四个映到恢复点、`Normal` 映到空：

```rust
    pub fn recovery_point(self) -> Option<RecoveryPoint> {
        match self {
            FaultContext::PhysCopy => Some(RecoveryPoint::PhysCopyFaultInKernel),
            FaultContext::Memset => Some(RecoveryPoint::MemsetFaultInKernel),
            FaultContext::UserCopyMsg => Some(RecoveryPoint::UserCopyMsgFailure),
            FaultContext::FpuRestore => Some(RecoveryPoint::FpuRestoreFailure),
            FaultContext::Normal => None,
        }
    }
}
```

（区间复算：`sed -n '137,146p' os/arch/src/arch/exception.rs | wc -l` = 10。锚点：`os/arch/src/arch/exception.rs` 的 `FaultContext::recovery_point` 实现。）

这里必须如实记一笔与 C 的差距：本内核的 x86 分发体调用 `handle` 时恒传 `FaultContext::Normal`——因为这套内核的用户拷贝路径改成了「先验证后拷贝」（调用方缓冲先过范围检查，坏指针在拷贝之前就被拒绝），可恢复的拷贝撞页在构造上不会发生，`FaultContext` 在生产路径上没有登记者。C 的捕获窗（§2.3 第二层）与嵌套拷贝重定向（§2.2 第二段）在这套 Rust 内核里对应的不是「同机制的另一种写法」，而是「被验证优先策略取代的机制」——`os/kernel/src/trap_dispatch.rs` 的分发体把 `RedirectToRecovery`/`PhysCopyFault` 两个结果实现为 panic，理由写在 acting 臂：这两个结果若出现，说明验证被绕过，是内核自身的不变量被破坏。C 的按需页捕获（`phys_copy_fault` 家族）属于 `24-cross-space-runtime` 方向（尚未重生成）的跨空间拷贝运行时，届时若引入捕获窗，`FaultContext` 的登记点就是它的落位处。置位侧同理：`forward_pagefault_to_vm` 用带出队半的 `rts_set`（[11-scheduling-primitives.md](11-scheduling-primitives.md) §4.2 的包装层）置 `RTS_PAGEFAULT`——只置标志不摘队列会让进程「标志已停、队列还在」，调度器会选到不该跑的进程；`set_pagefault_pending` 作为无队列半的原语，生产路径不经它，宿主测试与 qemu 测试内核（`os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-faultloop/`）用它构造状态。

转发协议的消费端闭环与 C 相同：`RTS_PAGEFAULT` 置位、`VM_PAGEFAULT` 消息、VM 服务后经 `VMCTL_CLEAR_PAGEFAULT` 清位——[09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3 的消费半与本篇的生产半在同一对位上配对。Rust 侧补了一处 C 没有的记录：`set_pagefault_pending` 把故障地址存进进程表的 `p_fault_addr` 字段供事后诊断——C 的故障地址只随消息旅行，进程表里不留底。

取舍表：

| 选项 | 内容 | 否掉什么 | 代价 |
|---|---|---|---|
| 三助手拆分（采纳） | 置位/构造消息/发送各自独立、可单测 | C 的八行混合体 | 三个可测原语 + 一个生产串接函数（`forward_pagefault_to_vm` 内串 `rts_set`/直写地址/构造/发送四步）；调用方负责串接 |
| `FaultContext` 枚举（采纳） | 显式上下文登记 | C 的计数窗+地址窗口比较 | 生产路径暂无登记者（验证优先取代），枚举当前只服务测试与未来捕获窗 |
| 照抄捕获窗 | eip 与 `phys_copy` 比地址 | —— | 三架构没有统一的「函数地址布局」可比；aarch64/riscv64 无现成对位 |

反向推演上下文枚举：假如保留 C 的地址窗口比较并移植到 Rust，x86 侧要维护 `phys_copy` 等函数的地址常量，aarch64/riscv64 侧要各自发明等价锚点——同一语义三种实现，且地址布局随链接脚本漂移，比较是脆的；换成显式登记后，语义只剩一份（枚举），架构差异退回「登记点放哪里」。反过来假如只留地址比较不留枚举，宿主测试无法构造「嵌套拷贝撞页」的场景——测试里没有真实的函数地址可比较，而枚举一个参数就能喂进去（§5.1 的 `nested_phys_copy` 测试正靠这个）。

### 3.3 处置表（→第 2 章各节）

C 的异常/中断面逐项去向如下。每行给出验证方式——测试名见第 5 章，走查节见第 4 章：

| C 语义（真值源） | 处置 | Rust 落点 | 验证 |
|---|---|---|---|
| `ex_data[20]` 向量→信号表（§2.1） | 改型 | `ExceptionSignal` 六变体 + `classify_signal` 的穷尽 match | `classify_all_vectors` |
| `ex_data` 的 `msg` 诊断文案列 | 淘汰 | 无对位——Rust 的诊断文案在执行臂现场构造 | §6.1 |
| `ex_data` 的 `minprocessor` 列 | 淘汰 | 无对位——C 运行时也从不读取（§2.1） | §6.2 复算 |
| 伪 NMI 打印返回（§2.2） | 保留 | `ExceptionOutcome::SpuriousNmi` 变体 | `spurious_nmi` |
| copy_msg 区 eip 重定向（§2.2） | 改型 | `FaultContext::UserCopyMsg` + `RecoveryPoint::UserCopyMsgFailure`；生产臂验证优先，恢复结果按不变量破坏 panic | `nested_user_copy_msg` |
| fxrstor 区 eip 重定向（§2.2） | 改型 | `FaultContext::FpuRestore` + `RecoveryPoint::FpuRestoreFailure` | `nested_fpu_restore` |
| 入口单步清追踪位（§2.2） | 保留 | `ClearTrapFlag` 变体 + `TrapStyle::NoEntry` 判据 | `nested_debug_*` 三测试 |
| 缺页转发 VM（§2.3） | 拆分 | `handle_page_fault`（决策）+ `page_fault.rs` 助手与 `forward_pagefault_to_vm`（执行） | `page_fault` 组 8 测试 |
| `catch_pagefaults` 计数窗+地址窗（§2.3） | 改型 | `FaultContext` 枚举 + `FaultContextTracker`（生产路径恒 `Normal`，§3.2 如实披露） | `nested_phys_copy` |
| VM 自身缺页 panic（§2.3） | 保留 | `VmPageFault` 变体 | `vm_page_fault` |
| `RTS_PAGEFAULT`+`VM_PAGEFAULT`+`mini_send`（§2.3） | 拆分 | `forward_pagefault_to_vm`（内部四步：`rts_set` 出队半置位 + `p_fault_addr` 直写 + `build_vm_pagefault_msg` 构造 + 引擎发送）；`set_pagefault_pending` 为原语，生产不经它（§3.2 披露） | `test_build_vm_pagefault_msg_*`、`test_vmctl_clear_page_fault_requeues_target` |
| `cause_sig` 调用点（§2.2） | 保留 | `Signal` 变体 → `os/kernel/src/syscall_signal.rs:fn cause_signal` | `user_divide_error` |
| `inkernel_disaster`+panic（§2.2） | 改型 | `KernelPanic(vector)` 变体，现场打印移入执行臂 | `kernel_panic` |
| 用户态 #NM 汇编拦截（§2.2） | 改型 | `FpuTrap` 变体——判定上移进分发器；懒恢复执行在执行臂（CLTS+保存旧属主+恢复本进程+属主交接，§4.2 执行臂表）；FPU 保存恢复本体的完整叙事归 `31-fpu-context-switching` 方向（尚未重生成） | `user_nm_fpu_trap` |
| `put_irq_handler` 链表+ID 位（§2.4） | 改型 | `IrqManager` 钩子池：`Option<usize>` 索引链 + 最低未用位分配 | `multiple_hooks_same_irq` |
| `rm_irq_handler` 摘链+条件屏蔽（§2.4） | 改型 | `IrqManager::remove_hook`（池内摘链 + 条件放线） | `remove_hook` |
| `irq_handle` 屏蔽→遍历→放线→ack（§2.4） | 保留 | `dispatch_claimed`（claim→mask→链→条件放线→complete） | `dispatch_claims_before_mask_and_completes_last` |
| 伪中断计数打印（§2.4） | 改型 | `IrqError::Spurious` 返回给调用方，计数交调用点 | `spurious_irq` |
| `generic_handler` 置位+通知（§2.4） | 保留 | `generic_notify_handler` + `KernelNotifier` | `generic_notify_handler_sends_notification` |
| `enable_irq`/`disable_irq`（§2.4） | 保留 | `IrqManager::enable_irq`/`disable_irq`（actids 位+条件放线） | `enable_disable_irq` |

（行数复算：本表 20 行数据行，与第 1 列列出的真值源一一对应；四档口径——**保留**=语义与可见动作对位（决策/执行拆分不改类别）、**改型**=语义保留而形状重写、**淘汰**=无对位、**拆分**=一个 C 概念落成多个 Rust 符号/阶段；全篇分布为保留 7、改型 9、淘汰 2、拆分 2。锚点：`os/kernel/src/syscall_signal.rs:fn cause_signal`、`os/plat/src/interrupt.rs:enum IrqAction`。）

---

## 4. 实现详解

### 4.1 ExceptionDispatcher 走查（→§3.1）

§3.1 的决策落成四个方法：入口 `handle`、嵌套子路径 `handle_nested`、缺页子路径 `handle_page_fault`、信号分类 `classify_signal`。入口全函数如下（40 行，逐字）：

```rust
    pub fn handle(
        frame: &mut EA::Frame,
        is_nested: bool,
        is_vm: bool,
        fault_ctx: FaultContext,
        is_traced: bool,
        kern_trap_style: TrapStyle,
    ) -> ExceptionOutcome {
        let vector = EA::vector(frame);
        let is_user = EA::is_user_mode(frame);

        if vector.get() == 2 {
            return ExceptionOutcome::SpuriousNmi;
        }

        if is_nested {
            return Self::handle_nested(frame, vector, fault_ctx, is_traced, kern_trap_style);
        }

        if vector.get() == 14 {
            return Self::handle_page_fault(frame, is_nested, is_vm, fault_ctx);
        }

        // User-mode #NM (vector 7) is the lazy-FPU trap: the process touched
        // FPU state while CR0.TS is set. C intercepts user #NM in the assembly
        // entry (mpx.S copr_not_available) and performs the lazy FPU restore
        // in copr_not_available_handler() (proc.c:1922); the Rust dispatcher
        // returns FpuTrap for the FPU-restore stage to implement (see
        // 31-fpu-context-switching.md §4.4). Kernel-mode #NM already fell
        // through to handle_nested() above, matching C's exception_entry_nested.
        if vector.get() == 7 && is_user {
            return ExceptionOutcome::FpuTrap;
        }

        if is_user {
            return Self::classify_signal(vector);
        }

        ExceptionOutcome::KernelPanic(vector)
    }
```

（区间复算：`sed -n '93,132p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 40。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn handle`、`:struct ExceptionDispatcher`。）

下面按分支次序走一遍，与 C 的次序逐点对位。

`handle` 的第一段与 C 相同：向量 2 返回 `SpuriousNmi`（C 打印一行后返回；Rust 返回决策，打印在执行臂——§3.3 记为「保留」：语义与可见动作同，仅落点移到执行臂）。第二段对应 C 的 `is_nested` 大分支，整只交给 `handle_nested`。第三段对应 C 的向量 14 分流。之后是 Rust 与 C 的第一处次序差异：`vector == 7 && is_user` 返回 `FpuTrap`。C 里这个判定根本不存在——用户态 #NM 在汇编层就被 `copr_not_available` 标签消费（§2.2 末段），用户态来源的向量 7 从不进 `exception_handler`；Rust 把它显式收进分发器，因为三架构的陷入入口已统一成一个分发面（[10-switch-to-user.md](10-switch-to-user.md) 的 `TrapStyle` 所在的同一层），x86 特有的「汇编层拦截」没有位置，判定只能上移。这一上移的语义后果：C 的分流器对向量 7 只可能见到内核态来源（嵌套入口压上向量号进来），Rust 的分流器两种来源都可能见到，所以判定写成 `vector == 7 && is_user`——用户态给 `FpuTrap`，内核态落进 `handle_nested` 无恢复区时照常 panic（§5.1 的 `nested_nm_panics` 测试钉这条）。

第四段是来源判定：`is_user` 直接查帧的特权位（`os/arch/src/arch/exception.rs:trait ExceptionArch::is_user_mode`，x86 实现比对 CS 低两位），对应 C 的 `is_nested == 0 && !iskernelp(saved_proc)`——C 查进程表的进程类型，Rust 查帧的特权级。非嵌套异常下两者同判：异常发生时 CPU 在用户态当且仅当被打断的是用户进程在用户态的执行。最后其余全部 `KernelPanic(vector)`，对应 C 的 `inkernel_disaster`+panic。

嵌套子路径全函数（35 行，逐字）：

```rust
    fn handle_nested(
        frame: &mut EA::Frame,
        vector: InterruptVector,
        fault_ctx: FaultContext,
        is_traced: bool,
        kern_trap_style: TrapStyle,
    ) -> ExceptionOutcome {
        if fault_ctx == FaultContext::UserCopyMsg {
            let is_pf_or_gpf = vector.get() == 14 || vector.get() == 13;
            if is_pf_or_gpf {
                return ExceptionOutcome::RedirectToRecovery(
                    RecoveryPoint::UserCopyMsgFailure,
                );
            }
        }

        if fault_ctx == FaultContext::FpuRestore {
            return ExceptionOutcome::RedirectToRecovery(
                RecoveryPoint::FpuRestoreFailure,
            );
        }

        // C: exception.c:231-234 — debug exception in kernel is legitimate
        // only if the process is traced and entered via SYSENTER/SYSCALL
        // (trap style is still KTS_NONE at the first kernel entry).
        if vector.get() == 1 && is_traced && kern_trap_style == TrapStyle::NoEntry {
            return ExceptionOutcome::ClearTrapFlag;
        }

        if vector.get() == 14 {
            return Self::handle_page_fault(frame, true, false, fault_ctx);
        }

        ExceptionOutcome::KernelPanic(vector)
    }
```

（区间复算：`sed -n '137,171p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 35。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn handle_nested`。）

四层判定的次序与 C 的嵌套分支逐段对位：拷贝上下文命中且向量是 14/13（缺页/保护错）→ `RedirectToRecovery(UserCopyMsgFailure)`，对应 C 的 eip 折 `__user_copy_msg_pointer_failure`；FPU 恢复上下文 → `RedirectToRecovery(FpuRestoreFailure)`，对应 `__frstor_failure`；向量 1 + 追踪位 + `TrapStyle::NoEntry` → `ClearTrapFlag`，对应 C 的入口单步清位。两个 C 有而 Rust 收窄的点如实指出：其一，C 对拷贝区的非缺页/非保护错向量 panic（`switch` 的 default 臂），Rust 的判定写成「上下文命中且向量是 14/13」——其他向量会穿过这条不命中，落到后面的缺页判定或最终 `KernelPanic`，终点同为停机，中间少一道显式的「拷贝区不该有别的异常」断言；其二，C 的三个判定读的是帧内 eip，Rust 读的是调用方传入的 `fault_ctx`——判定的真值来源从「地址」换成「登记」（§3.2 已述其取舍）。第四层把缺页转给 `handle_page_fault`（嵌套标志保持），最后兜底 `KernelPanic`。

缺页子路径全函数（40 行，逐字）：

```rust
    fn handle_page_fault(
        frame: &mut EA::Frame,
        is_nested: bool,
        is_vm: bool,
        fault_ctx: FaultContext,
    ) -> ExceptionOutcome {
        let fault_addr = EA::page_fault_address();
        let is_write = EA::is_write_fault(frame);

        // C also checks `catch_pagefaults` counter before recovering;
        // omitted here because FaultContext enum precisely tracks the
        // recoverable context (see §3.6 in 14-exception-interrupt.md).
        // C: catch_pagefaults && (in_physcopy || in_memset) — exception.c:62-73
        if fault_ctx == FaultContext::PhysCopy || fault_ctx == FaultContext::Memset {
            if is_nested {
                let recovery = if fault_ctx == FaultContext::PhysCopy {
                    RecoveryPoint::PhysCopyFaultInKernel
                } else {
                    RecoveryPoint::MemsetFaultInKernel
                };
                return ExceptionOutcome::RedirectToRecovery(recovery);
            } else {
                return ExceptionOutcome::PhysCopyFault { fault_addr };
            }
        }

        if is_nested {
            return ExceptionOutcome::KernelPanic(InterruptVector::new(14));
        }

        if is_vm {
            return ExceptionOutcome::VmPageFault;
        }

        ExceptionOutcome::ForwardToVm(VmPagefaultIn {
            endpoint: minix_types::Endpoint::NONE,
            vaddr: fault_addr,
            write: is_write,
        })
    }
```

（区间复算：`sed -n '176,215p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 40。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn handle_page_fault`。）

与 C 的 `pagefault`（§2.3）逐层对位：第一层 `FaultContext::PhysCopy`/`Memset` 命中对应 C 的捕获窗——嵌套给 `RedirectToRecovery`（对应折 `*_in_kernel` 标签），非嵌套给 `PhysCopyFault{fault_addr}`（对应 pc 折 `phys_copy_fault` + 返回寄存器装 CR2；`set_return_value` 是 `ExceptionArch` 为这个结局准备的写返回寄存器方法，`os/arch/src/arch/exception.rs:trait ExceptionArch::set_return_value`）。注释里那句「C also checks `catch_pagefaults` counter before recovering; omitted here」与本篇 §3.2 的披露一致：开关被上下文枚举吸收，本内核生产路径不设登记者。第二层嵌套且无捕获上下文 → `KernelPanic`，对应 C 的 `inkernel_disaster`。第三层 `is_vm` → `VmPageFault`，对应 C 的 `panic("pagefault in VM")` 前的判定。最后正常路：`ForwardToVm(VmPagefaultIn{...})`——注意枚举里的 `endpoint` 字段填的是占位值，执行臂不消费它——真实端点在 `forward_pagefault_to_vm` 内部按进程号重取（§4.2），C 的对应字段在构造消息时直接取 `pr->p_endpoint`；内核侧该字段只被填成 `NONE` 且无消费者（VM 侧 `decode_message` 另有真实写入——`endpoint: msg.m_source`），是内核侧留白（待清理或接线，R14-T06）；`write` 位取自错误码（C 把整个错误码装进 `VPF_FLAGS` 交给 VM 解读，Rust 在判定侧就拆出写位，两种粒度都在协议里）。

信号分类全函数（25 行，逐字）：

```rust
    fn classify_signal(vector: InterruptVector) -> ExceptionOutcome {
        use ExceptionSignal::*;
        let sig = match vector.get() {
            0 => Fpe,
            1 => Trap,
            3 => Emt,
            4 => Fpe,
            5 => Fpe,
            6 => Ill,
            7 => Fpe,
            8 => Bus,
            9 => Segv,
            10 => Segv,
            11 => Segv,
            12 => Segv,
            13 => Segv,
            15 => Ill,
            16 => Fpe,
            17 => Bus,
            18 => Bus,
            19 => Fpe,
            _ => Segv,
        };
        ExceptionOutcome::Signal(sig)
    }
```

（区间复算：`sed -n '220,244p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 25。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn classify_signal`。）

18 条向量臂加一条兜底（20 项 C 表 − 向量 2/14），与 §2.1 的表逐行对账：0/4/5/7/16/19 → Fpe 六条（除零、溢出、越界、#NM、x87 错误、SIMD），1 → Trap，3 → Emt（断点），6 → Ill，8 → Bus，9-13 → Segv 五条，15 → Ill（NULL 槽的 SIGILL），17/18 → Bus。向量 2 与 14 不需要臂——前者在入口就被 `SpuriousNmi` 截下，后者整只进 `handle_page_fault`，这与 C 的控制流相同（C 的表虽有 2/14 号表项，分流器在这两个向量上也到不了 `cause_sig`）。`_ => Segv` 兜底臂覆盖的是表外向量——它不影响合法向量的对账，但它意味着「未知向量按段错误上报」是这段代码的选择而非必然；`classify_all_vectors` 测试（§5.2）把 18 个可分类向量逐值钉死，兜底臂不在其列；向量 7 的 `Fpe` 臂与 C 的 `ex_data[7]` 对位保留——生产路径不可达（用户态被 `FpuTrap` 截下、内核态落 `KernelPanic`），由该测试直接调用覆盖。

### 4.2 page_fault 与执行臂走查（→§3.1/§3.2）

§3.2 的三助手拆分，先看置位与查询这对。全函数（4 行 + 14 行，逐字）：

```rust
pub fn set_pagefault_pending(proc: &mut KProcess, fault_addr: u64) {
    proc.p_rts_flags.set(RtsFlagsBits::PAGEFAULT);
    proc.p_fault_addr = Some(fault_addr);
}
```

（区间复算：`sed -n '70,73p' os/kernel/src/page_fault.rs | wc -l` = 4。锚点：`os/kernel/src/page_fault.rs:fn set_pagefault_pending`、`:fn clear_pagefault_pending`、`:fn is_pagefault_pending`。）

```rust
pub fn clear_pagefault_pending(proc: &mut KProcess) -> bool {
    let was_set = proc.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT);
    proc.p_rts_flags.clear(RtsFlagsBits::PAGEFAULT);
    proc.p_fault_addr = None;
    was_set
}

/// Check whether a process has an unhandled page fault.
///
/// Used by the scheduler (per-CPU scheduling queue) to skip
/// faulting processes until VM clears the flag.
pub fn is_pagefault_pending(proc: &KProcess) -> bool {
    proc.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT)
}
```

（区间复算：`sed -n '84,97p' os/kernel/src/page_fault.rs | wc -l` = 14。锚点：`os/kernel/src/page_fault.rs:fn clear_pagefault_pending`、`:fn is_pagefault_pending`。）

`set_pagefault_pending` 对应 C 的 `RTS_SET(pr, RTS_PAGEFAULT)`（§2.3 第四层第一步）的原语面，外加 `p_fault_addr` 记录——C 没有的诊断底账（§3.2 已述）。它是无队列半的原语：生产置位在 `forward_pagefault_to_vm` 里走带出队半的 `rts_set`（见下文对它的走查）。`clear_pagefault_pending` 的返回值承接 C 消费端的断言语义：C 的 `VMCTL_CLEAR_PAGEFAULT` 先 `assert(RTS_ISSET(...))` 再清（§2.3 末的引文），Rust 把「位没开」折成返回值 false；生产清位在 VMCTL 臂里做同样判定（`vmctl_clear_page_fault` 的 `was_set` → EINVAL）后走带入队半的 `rts_unset`。`is_pagefault_pending` 是就绪判定的查询原语面（调度器侧跳过缺页进程的判据，11 篇 §2.2）。**现状披露**：这三个助手当前都没有生产调用者，留给宿主测试与 qemu 测试内核构造状态。

消息构造助手（25 行，逐字）：

```rust
pub fn build_vm_pagefault_msg(
    proc_endpoint: Endpoint,
    fault_addr: u64,
    error_code: u32,
) -> Message {
    // SAFETY: `Message` is a `#[repr(C)]` union; zeroing is the
    // documented "fresh message" pattern in Minix's kernel.
    let mut msg = Message {
        // C: exception.c:119 — `m_pagefault.m_source = pr->p_endpoint`
        m_source: proc_endpoint,
        // C: exception.c:120 — `m_pagefault.m_type = VM_PAGEFAULT`
        //
        // VM_PAGEFAULT is a u32 in minix-types; m_type is i32. The cast is
        // safe because VM_PAGEFAULT (0xCFF = 3327) fits in i32's positive
        // range. VM's main loop checks `m_type == VM_PAGEFAULT` (vm_server.rs:436)
        // — without this assignment, m_type stays 0 (from Message::default)
        // and VM cannot dispatch the page-fault request.
        m_type: VM_PAGEFAULT as i32,
        ..Default::default()
    };
    // C: exception.c:121-122 — `VPF_ADDR = pagefaultcr2; VPF_FLAGS = frame->errcode`
    msg.m_u.m_vm_pagefault.vpf_addr = fault_addr;
    msg.m_u.m_vm_pagefault.vpf_flags = error_code;
    msg
}
```

（区间复算：`sed -n '200,224p' os/kernel/src/page_fault.rs | wc -l` = 25。锚点：`os/kernel/src/page_fault.rs:fn build_vm_pagefault_msg`、`:const VM_PAGEFAULT`。）

四个字段与 C 的四行赋值逐行对位：`m_source` ← 故障进程端点、`m_type` ← `VM_PAGEFAULT`、`vpf_addr` ← 故障地址、`vpf_flags` ← 错误码（§2.3 第四层第二步的 C 逐字）。`m_type` 的 `as i32` 转换在注释里自带安全性论证：`VM_PAGEFAULT` 的值是 `VM_RQ_BASE + 0xFF`（`os/libs/minix-types/src/ipc/vm.rs:const VM_PAGEFAULT`），手算 `0xC00 + 0xFF = 0xCFF = 3327`，落在 i32 正数区间。注释还点破了这一位的要害：VM 的主循环按这个值分派，m_type 若停在默认值 0，消息会被无声丢弃——发送成功与请求被服务是两回事。

执行臂在 trap 分发体里。转发臂的收尾段（19 行，逐字）：

```rust
            // C: pagefault() tail — RTS_PAGEFAULT, VM_PAGEFAULT message,
            // FROM_KERNEL mini_send (exception.c:112-129). Send errors
            // panic here (C: panic "WARNING: pagefault: mini_send
            // returned %d").
            if let Err(e) = forward_pagefault_to_vm(
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
                cur_nr,
                pf.vaddr.0,
                frame.errcode as u32,
            ) {
                panic!("pagefault: mini_send returned {e:?}");
            }
            // C: pagefault() returns → mpx.S `jmp switch_to_user`. The
            // process stays non-runnable (RTS_PAGEFAULT) until VM's
            // SYS_VMCTL ClearPageFault re-enqueues it (do_vmctl.c:35);
            // the scheduling loop picks whoever is runnable next.
            crate::scheduler_loop(crate::current_cpu_id())
        }
```

（区间复算：`sed -n '585,603p' os/kernel/src/trap_dispatch.rs | wc -l` = 19。锚点：`os/kernel/src/trap_dispatch.rs:fn x86_trap_dispatch_body`、`:fn forward_pagefault_to_vm`。）

转发臂的头半段在收尾段之前做两件事（正文叙述，不另贴码）：把故障现场从帧存回进程表的 `cpu_context`（VM 服务缺页期间进程停在 `RTS_PAGEFAULT` 上，恢复时要对着这份挂起上下文重执行故障指令），并登记 `TrapStyle::FullContext` 返回样式（[10-switch-to-user.md](10-switch-to-user.md) §1.2/§4.4 的进入/返回配对——缺了它，转发后重新入队的进程在下次分派时撞「返回样式未知」）。收尾段调 `forward_pagefault_to_vm`，失败即 panic（对应 C 的 `panic("WARNING: pagefault: mini_send returned %d")`）；成功则进调度循环——故障进程停在 `RTS_PAGEFAULT` 上不会被选中，调度器去挑别人，对应 C 的 `pagefault` 返回后 `jmp switch_to_user`。信号臂的尾段与转发臂同形（13 行，逐字）：

```rust
            // C: cause_sig(proc_nr(saved_proc), ep->signum) —
            // exception.c:276. RTS_SIGNALED parks the process until its
            // signal manager resolves the delivery (kernel-side behavior
            // is complete at cause_sig).
            let sig_nr = exception_signal_to_nr(sig);
            crate::syscall_signal::cause_signal(
                cur_nr,
                sig_nr,
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
            );
            // C: cause_sig returns → mpx.S `jmp switch_to_user`.
            crate::scheduler_loop(crate::current_cpu_id())
```

（区间复算：`sed -n '640,652p' os/kernel/src/trap_dispatch.rs | wc -l` = 13。锚点：`os/kernel/src/trap_dispatch.rs:fn exception_signal_to_nr`、`os/kernel/src/syscall_signal.rs:fn cause_signal`。）

`exception_signal_to_nr` 把 §4.1 分类出的 `ExceptionSignal` 六变体映到内核信号号（Fpe→SIGFPE、Ill→SIGILL、Segv→SIGSEGV、Bus→SIGBUS、Emt→SIGEMT、Trap→SIGTRAP），`cause_signal` 是 Rust 信号路径的统一入口（[12-ipc-core.md](12-ipc-core.md) 已引），置位后同样进调度循环——进程停在 `RTS_SIGNALED` 上等信号管理器。

`forward_pagefault_to_vm` 的头段（20 行，逐字）：

```rust
    proc_table.rts_set(cur_nr, RtsFlagsBits::PAGEFAULT);
    // Rust-side diagnostic record (the field `set_pagefault_pending`
    // maintains; C keeps no per-process fault address — the address
    // travels in the message alone).
    if let Some(p) = proc_table.get_mut(cur_nr) {
        p.p_fault_addr = Some(fault_addr);
    }
    let src_endpoint = proc_table
        .get(cur_nr)
        .map(|p| p.p_endpoint)
        .unwrap_or(minix_types::Endpoint::NONE);
    let msg = crate::page_fault::build_vm_pagefault_msg(src_endpoint, fault_addr, error_code);
    // Send target: the VM process's own endpoint (C: mini_send(pr,
    // VM_PROC_NR, ...) resolves the slot through the table; a dead VM
    // slot is the EDEADSRCDST path).
    let dst_endpoint = proc_table
        .get(crate::proc::proc_nr::VM_PROC_NR)
        .map(|p| p.p_endpoint)
        .ok_or(crate::ipc::IpcError::DeadSrcDst)?;
    let mut engine = IpcEngine::new(proc_table.procs_slice_mut(), priv_table, &KernelUserCopy);
```

（区间复算：`sed -n '756,775p' os/kernel/src/trap_dispatch.rs | wc -l` = 20。锚点：`os/kernel/src/trap_dispatch.rs:fn forward_pagefault_to_vm`。）

头段里 `p_fault_addr` 的直写与引文注释所说 `set_pagefault_pending` 维护的是同一字段——生产走直写、原语留给测试（§3.2 披露）。这里有两个 C 没有的动作值得停下。第一，置位走的是 `rts_set` 而不是上面那对裸助手：`rts_set` 带出队半——C 的 `RTS_SET` 宏族在置位的同时处理就绪队列摘除（[11-scheduling-primitives.md](11-scheduling-primitives.md) §4.2 的包装层）；只调 `set_pagefault_pending` 会留下「标志已停、队列还在」的中间态，调度器会选到不该跑的进程（源码 doc 注释点名了这条死路：`pick_proc` 取队头时不做可运行复查）。第二，发送后收集引擎的全部唤醒目标并逐个入队（头段之后的尾段做这件事）：`mini_send` 若目标 VM 正停在 RECEIVE 上，引擎侧只清它的 `RTS_RECEIVING`，入就绪队列是调用方的责任——漏了这半，VM 被解锁却永不入队，调度器无人可选，整台机器 idle 停机，而故障进程还停在 `RTS_PAGEFAULT` 上等一个永远不会到来的服务。C 的 `mini_send` 把队列半包在原语里，Rust 的 IPC 引擎把它拆出来交调用方——这条拆分的走查位在 [12-ipc-core.md](12-ipc-core.md) §4.3，本篇只负责把「调用方必须补队」这个义务钉在转发路径上。

**执行臂总表**：§3.1 的「调用方要接住九个变体」在 x86 执行体（`os/kernel/src/trap_dispatch.rs` 的分发体）里的逐臂落点——转发与信号两臂已在上文走查，其余七臂一览：

| 变体 | 执行动作 | 锚点（同文件 `trap_dispatch.rs`） | 钉住 |
|---|---|---|---|
| `SpuriousNmi` | 控制台打印一行 | SpuriousNmi 臂 | `spurious_nmi`（决策半） |
| `ClearTrapFlag` | 清帧内追踪位（`rflags &= !TRACEBIT`） | ClearTrapFlag 臂 | `nested_debug_clear_tf`（决策半） |
| `RedirectToRecovery` | panic——验证优先下出现即不变量破坏 | RedirectToRecovery 臂 | `nested_user_copy_msg`/`nested_phys_copy`（决策半） |
| `PhysCopyFault` | panic——Direct Map 窗口校验漏过 | PhysCopyFault 臂 | `nested_phys_copy`（决策半） |
| `KernelPanic` | 控制台帧转储（vector/err/rip/cs/rflags/rsp/ss）后 panic | KernelPanic 臂 | `kernel_panic`/`nested_nm_panics`（决策半） |
| `VmPageFault` | 控制台转储（cpu/rip/cr2/err）后 `panic!("pagefault in VM")` | VmPageFault 臂 | `vm_page_fault`（决策半） |
| `ForwardToVm` | 存回故障现场 + `TrapStyle::FullContext` + 转发 + 进调度循环 | 上文走查 | `user_page_fault`（决策半）+ 转发组 |
| `Signal` | 控制台转储 + 存回现场 + `TrapStyle::FullContext` + `cause_signal` + 进调度循环 | 上文走查 | `user_divide_error`（决策半） |
| `FpuTrap` | CLTS + 保存旧属主 FP 状态 + 恢复本进程 + 属主交接（落穿后 iretq 重执行） | FpuTrap 臂 | `user_nm_fpu_trap`（决策半） |

（行数复算：本表 9 个数据行，与 `ExceptionOutcome` 九变体一一对应；执行体三架构各一份，本篇走查 x86 一份。锚点：`os/kernel/src/trap_dispatch.rs:fn x86_trap_dispatch_body`。）

### 4.3 IRQ hook 管理的 Rust 对位（→§2.4）

§3.3 处置表里 hook 链各行的 Rust 落点集中在这里。C 的 hook 链是「进程自己 `malloc` 一个 hook、内核串指针」的形状；Rust 的 `IrqManager` 把链收进一个固定池。结构体与槽类型（33 行，逐字）：

```rust
/// An IRQ hook slot in the global hook pool.
struct IrqHookSlot {
    next: Option<usize>,
    handler: IrqHandler,
    irq: IrqVector,
    id: IrqId,
    proc_endpoint: Endpoint,
    notify_id: IrqNotifyId,
    policy: IrqPolicy,
}

/// Error type for IRQ operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqError {
    InvalidIrq,
    NoFreeSlots,
    Spurious(IrqVector),
    NotOwner,
}

/// Architecture-independent IRQ hook chain manager.
///
/// Manages registration, removal, and dispatch of IRQ handlers.
/// Uses a fixed-size hook pool and per-vector chain heads.
///
/// C: interrupt.c — put_irq_handler(), rm_irq_handler(), irq_handle()
pub struct IrqManager<IC: InterruptRouter + PerCpuInterruptUnit> {
    hooks: [Option<IrqHookSlot>; NR_IRQ_HOOKS],
    handlers: [Option<usize>; NR_IRQ_VECTORS],
    actids: [IrqIdBitmap; NR_IRQ_VECTORS],
    irq_use: u64,
    controller: IC,
}
```

（区间复算：`sed -n '294,326p' os/kernel/src/irq_manager.rs | wc -l` = 33。锚点：`os/kernel/src/irq_manager.rs:struct IrqHookSlot`、`:struct IrqManager`、`:enum IrqError`。）

形状对位：`hooks` 池对应 C 的全局 `irq_hook_t` 数组池（`NR_IRQ_HOOKS` 个槽），`handlers` 数组对应 C 的 `irq_handlers[NR_IRQ_VECTORS]` 链头表，`actids` 对应 C 的 `irq_actids`，`controller` 收编 C 散在各处的 `hw_intr_*` 调用（05 篇 §3.3 把控制器拆成 `InterruptRouter` 与 `PerCpuInterruptUnit` 两个 trait，本篇的消费端就是它）。与 C 的第一处形状差异在链节：C 的 `next` 是裸指针，Rust 是 `Option<usize>` 池内索引——所有权归池，链只是索引关系，没有悬垂与释放问题。第二处差异在错误处理：C 的越界检查与单线 32 位 ID 耗尽是 panic；Rust 把越界改 `InvalidIrq`、新增池槽耗尽的 `NoFreeSlots`（两者为返回值，调用方决定怎么办），单线 ID 耗尽仍与 C 同源 panic（`too many handlers`，见注册段引文）。

注册路径的核心段（42 行，逐字）：

```rust
        // Allocate the lowest unused bit ID for this IRQ chain.
        let mut bitmap: IrqIdBitmap = 0;
        let mut cur = self.handlers[irq_idx];
        while let Some(idx) = cur {
            let slot = self.hooks[idx].as_ref().unwrap();
            bitmap |= slot.id.0;
            cur = slot.next;
        }

        let mut id = 1u32;
        while id != 0 && (bitmap & id) != 0 {
            id <<= 1;
        }
        if id == 0 {
            panic!("too many handlers for IRQ {}", irq.get());
        }

        self.hooks[slot_idx] = Some(IrqHookSlot {
            next: None,
            handler,
            irq,
            id: IrqId(id),
            proc_endpoint,
            notify_id,
            policy,
        });

        self.append_to_chain(irq_idx, slot_idx);

        self.irq_use |= 1u64 << irq_idx;

        // C: interrupt.c:65 — `(irq_actids[irq] &= ~hook->id) == 0`.
        // Clear this hook's active bit (no-op for a brand-new hook, but
        // faithful to C and safe if a slot is ever reused), then unmask
        // the IRQ only if NO handler on this vector is still active.
        self.actids[irq_idx] &= !id;
        if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        Ok(IrqId(id))
    }
```

（区间复算：`sed -n '400,441p' os/kernel/src/irq_manager.rs | wc -l` = 42。锚点：`os/kernel/src/irq_manager.rs:impl IrqManager::register_hook_at_slot`、`:fn append_to_chain`。）

与 C 的 `put_irq_handler`（§2.4）四件事逐项对位：找最低未用位 ID 的循环与 C 的 `for (id = 1; id != 0; id <<= 1)` 同构（位全满 panic，文案同源「too many handlers」）；尾插由 `append_to_chain` 完成（C 在遍历链时顺手记尾，Rust 拆成独立方法，遍历两遍换一处职责分立）；`actids &= !id` 后条件放线与 C 的 `if((irq_actids[...] &= ~hook->id) == 0)` 一字不差地对应——新 hook 活跃位本来是零，这次清位是幂等防御，真正起作用的是「全零才 unmask」的判定。Rust 侧多出的 `irq_use` 位图是本篇范围外的记账（[05-clock-interrupt-init.md](05-clock-interrupt-init.md) §3.4 的控制器归宿），不计入对位。注销半对位在 `IrqManager::remove_hook`（池内摘链 + 条件放线），对应 §2.4 的 `rm_irq_handler`；§5.1 的 `remove_hook` 测试钉住「注销后派发变 `Spurious`」。开关一对 `enable_irq`/`disable_irq` 与注册/注销共用同一张位图：清自己的位、全线归零才放线（`enable_irq`）；置位并屏蔽整线、返回「此前是否已屏蔽」（`disable_irq`）——`enable_disable_irq` 测试钉住往返。

触发路径全函数（67 行，逐字）：

```rust
    pub fn dispatch_claimed(
        &mut self,
        irq: IrqVector,
        claimed: Option<u32>,
        notifier: &mut dyn IrqNotify,
    ) -> Result<(), IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return Err(IrqError::InvalidIrq);
        }

        self.controller.mask(irq);

        let mut slot_idx = self.handlers[irq_idx];
        if slot_idx.is_none() {
            return Err(IrqError::Spurious(irq));
        }

        while let Some(idx) = slot_idx {
            // Copy the slot fields by value (all Copy) so we don't hold a
            // borrow on `self.hooks` while calling the handler. The handler
            // receives `&mut IrqHookContext` which only borrows `notifier`.
            let (handler, slot_irq, slot_id, slot_proc_ep, slot_notify_id, slot_policy, next) = {
                let slot = self.hooks[idx].as_ref().unwrap();
                (
                    slot.handler,
                    slot.irq,
                    slot.id,
                    slot.proc_endpoint,
                    slot.notify_id,
                    slot.policy,
                    slot.next,
                )
            };

            self.actids[irq_idx] |= slot_id.0;

            // C: do_irqctl.c:154 — `get_randomness(&krandom, hook->irq)`.
            // Called once per hook invocation (matching C's generic_handler).
            // Currently a no-op stub (see krandom.rs §Design Decisions D3).
            crate::krandom::get_randomness(slot_irq.get() as i32);

            let mut ctx = IrqHookContext {
                irq: slot_irq,
                id: slot_id,
                proc_endpoint: slot_proc_ep,
                notify_id: slot_notify_id,
                policy: slot_policy,
                notifier,
            };

            let action = handler(&mut ctx);
            if action == IrqAction::Completed {
                self.actids[irq_idx] &= !slot_id.0;
            }

            slot_idx = next;
        }

        if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        self.controller.complete(claimed);

        Ok(())
    }
```

（区间复算：`sed -n '548,614p' os/kernel/src/irq_manager.rs | wc -l` = 67。锚点：`os/kernel/src/irq_manager.rs:impl IrqManager::dispatch_claimed`、`:fn dispatch`。）

与 C 的 `irq_handle`（§2.4）逐段对位：先 `mask`（对应 `hw_intr_mask`）；链头空返回 `IrqError::Spurious`（对应伪中断分支——C 在里面计数打印，Rust 把计数交还调用方，§3.3 的「改型」）；遍历链，每个 hook 先置 ID 位（`self.actids[irq_idx] |= slot_id.0`，与 C 的次序论证一致：先给名字再调用）、采一次随机数时间戳（对应 C `generic_handler` 开头的 `get_randomness`；**现状披露**：Rust 侧该调用从 handler 内上移到派发循环、对每个 hook 生效；引文注释所称「no-op stub」已过期——`krandom.rs` 的 `get_randomness` 现为真实 TSC 采样实现（`[ARCH: deviation]` 登记在案），源码注释待同步（R14-T04））、构造 `IrqHookContext` 调 handler、返回 `Completed` 才清位；全部归零才 `unmask`；最后 `complete(claimed)` 回应控制器。次序上比 C 多出的一步是开头的 `claim`：C 的 i386 路径里「认领中断」发生在汇编入口（8259A 的级联应答在 `hwint` 尾部统一做），GIC/PLIC 一类控制器把认领做成一次寄存器读，读到的值就是「本次中断的身份」，完成（EOI）必须写回同一个身份——claim 与 complete 的成对关系是三架构统一到这里之后显式化的：x86 的认领是无操作的空转（没有认领寄存器，EOI 就是全部握手），ARM64/RISC-V 的认领读决定 EOI 写什么，跳过认领会让完成写回复位值 0，中断永不退役、反复重入。`dispatch_claimed` 把「调用方已认领」作为独立入口暴露，就是给 ARM64/RISC-V 那种「要先读身份才知道路由到哪条链」的入口路径用的（入口先 `controller_claim` 拿身份、再带着身份走完整握手）。

通知路径。`KernelNotifier` 的核心段（15 行，逐字）：

```rust
            // C: do_irqctl.c:167 — `priv(proc_addr(proc_nr))->s_int_pending
            //                        |= (1 << hook->notify_id)`
            priv_.signals.s_int_pending |= 1u32 << notify_id.get();
        }

        // C: do_irqctl.c:170 — `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e)`
        //
        // Deliver the notification. `HARDWARE = KERNEL = -1` (proc.rs:80).
        // C's `mini_notify` returns `void` (not a status code); `generic_handler`
        // does not check any return value because there is none to check.
        // The notification is best-effort: if the destination is not currently
        // RECEIVE-ing, the `s_int_pending` bit set above will be delivered on
        // its next RECEIVE. We ignore the Rust `IpcOutcome` to match C's
        // void-return semantics.
        let _ = crate::ipc::kernel_mini_notify(crate::proc::proc_nr::KERNEL, dst);
```

（区间复算：`sed -n '168,182p' os/kernel/src/irq_manager.rs | wc -l` = 15。锚点：`os/kernel/src/irq_manager.rs:impl IrqNotify for KernelNotifier::notify_hardware`、`:fn generic_notify_handler`。块尾孤立的 `}` 是上文 if 块收尾。）

与 C 的 `generic_handler`（§2.4 逐字）逐行对位：先置属主的 `s_int_pending` 位（对应 `priv(proc_addr(proc_nr))->s_int_pending |= (1 << hook->notify_id)`），再发通知（对应 `mini_notify(HARDWARE, hook->proc_nr_e)`——`KERNEL` 槽就是 C 的 `HARDWARE` 伪端点）。C 的端点存活检查（`isokendpt` 失败 panic）在头半段——端点解析不到活进程就 panic，文案与 C 同源。通知之后的队半（这段之后的三行，正文叙述）：引擎只清 `RTS_RECEIVING`，停在 RECEIVE 上的驱动进程要靠调用方入就绪队列——与 §4.2 转发路径的「调用方补队」是同一条纪律，漏了它，被中断通知唤醒的服务器永远等不到调度。

通用 handler 本体（11 行，逐字）：

```rust
fn generic_notify_handler(ctx: &mut IrqHookContext) -> IrqAction {
    // C: do_irqctl.c:167-170 — set pending bit + deliver notification.
    ctx.notifier.notify_hardware(ctx.proc_endpoint, ctx.notify_id);

    // C: do_irqctl.c:171 — `return(hook->policy & IRQ_REENABLE)`.
    if ctx.policy.contains(IrqPolicy::REENABLE) {
        IrqAction::Completed
    } else {
        IrqAction::NotCompleted
    }
}
```

（区间复算：`sed -n '864,874p' os/kernel/src/irq_manager.rs | wc -l` = 11。锚点：`os/kernel/src/irq_manager.rs:fn generic_notify_handler`。）

与 C 的返回值约定对位：C 返回 `hook->policy & IRQ_REENABLE`（真=干完），Rust 返回 `IrqAction::Completed`/`NotCompleted` 两个变体——int 返回值换成枚举，「返回真的含义」从约定变成类型。通知通过 `IrqHookContext` 里的 `notifier` trait 对象发出（§3.1 的执行注入同思路）：handler 不摸全局表，测试用假 notifier 记录调用（§5.2 的 `generic_notify_handler_sends_notification`）。

入口包装 `dispatch_hardware_irq`（节选；注释有删节——删节处为 BKL「继承或自旋」的选取规则与被打断段记账语义的两段注释）：

```rust
pub fn dispatch_hardware_irq(irq: IrqVector) -> Result<(), IrqError> {
    let mut notifier = KernelNotifier;
    let acquired = crate::smp::bkl_lock_or_inherit();
    let section = unsafe { crate::smp::BklSection::assume_held() };
    if acquired {
        crate::account_interrupt_stop(&section);
    }
    let mgr = crate::irq_manager_with(&section);
    let result = mgr.dispatch(irq, &mut notifier);
    if acquired {
        crate::smp::bkl_unlock();
    }
    result
}
```

（锚点：`os/kernel/src/irq_manager.rs:fn dispatch_hardware_irq`。）

BKL 的语义对应 C 的两条汇编路径（§2.4 引文的 `hwint_master` 宏）：打断用户态或空闲态时 BKL 空闲，这里自旋取得（C 的用户路径在陷入时取 BKL）；打断内核态时 BKL 已被被打断的执行流持有，这里「继承」而非重取（C 的内核路径根本不取锁——非重入锁上再取就是死锁）。`acquired` 只在真取得时为真，解锁也只在真取得时做——继承来的锁归原来的持有者。`account_interrupt_stop` 是被打断段记账的收尾（仅真取得时记，继承场景的那段留给出去侧的站点），属 10 篇的记账线，本篇只立调用点。

---
## 5. 测试要点

### 5.1 清单与读数（→第 4 章）

本篇三个 Rust 文件各带一组宿主单元测试。直接相关项按下表枚举（测试名的口径：`#[test]` 函数名）：

| 文件 | 测试（逐条） | 钉住什么 | 走查 |
|---|---|---|---|
| `os/arch/src/arch/exception_dispatcher.rs` | `spurious_nmi` | 向量 2 → `SpuriousNmi` | §4.1 |
|  | `user_divide_error` | 用户态向量 0 → `Signal(Fpe)` | §4.1 |
|  | `user_page_fault` | 用户态向量 14 → `ForwardToVm` | §4.1 |
|  | `user_nm_fpu_trap` | 用户态向量 7 → `FpuTrap` | §4.1 |
|  | `nested_nm_panics` | 内核态 #NM 无恢复区 → `KernelPanic` | §4.1 |
|  | `vm_page_fault` | VM 自身缺页 → `VmPageFault` | §4.1 |
|  | `kernel_panic` | 内核态向量 13 → `KernelPanic` | §4.1 |
|  | `nested_user_copy_msg` | 拷贝区+向量 14 → `UserCopyMsgFailure` | §4.1 |
|  | `nested_phys_copy` | `PhysCopy` 上下文 → `PhysCopyFaultInKernel` | §4.1 |
|  | `nested_fpu_restore` | `FpuRestore` 上下文 → `FpuRestoreFailure` | §4.1 |
|  | `nested_debug_clear_tf` | 三条件齐 → `ClearTrapFlag` | §4.1 |
|  | `nested_debug_not_traced` | 缺追踪位 → `KernelPanic` | §4.1 |
|  | `nested_debug_traced_not_kts_none` | 样式非 NoEntry → `KernelPanic` | §4.1 |
|  | `classify_all_vectors` | 18 向量→信号逐值对账 | §4.1 |
| `os/kernel/src/page_fault.rs` | `test_set_pagefault_pending_sets_flag_and_addr` | 置位+地址记录 | §4.2 |
|  | `test_clear_pagefault_pending_clears_flag_and_addr` | 清位+首清返回 true | §4.2 |
|  | `test_is_pagefault_pending_default_false` | 新进程无该位 | §4.2 |
|  | `test_build_vm_pagefault_msg_populates_fields` | 消息四字段 | §4.2 |
|  | `test_kernel_mode_pagefault_panic_msg_includes_endpoint_and_addr` | 内核缺页 panic 文案 | §4.2 |
|  | `test_pagefault_pending_roundtrip_with_clear` | 置→查→清→查 | §4.2 |
|  | `test_pagefault_pending_idempotent_set` | 重复置位幂等、地址后写者胜 | §4.2 |
|  | `test_pagefault_skips_kernel_proc_for_set` | 内核进程不该走置位（文案契约） | §4.2 |
| `os/kernel/src/irq_manager.rs` | `register_and_dispatch` | 注册+派发+完成基本环 | §4.3 |
|  | `dispatch_claims_before_mask_and_completes_last` | claim→mask→链→unmask→complete 全序 | §4.3 |
|  | `spurious_irq` | 无注册线 → `Spurious` | §4.3 |
|  | `multiple_hooks_same_irq` | 同线双 hook 异 ID | §4.3 |
|  | `not_completed_keeps_active` | 返回未完成则活跃位保持 | §4.3 |
|  | `remove_hook` | 注销后派发变 `Spurious` | §4.3 |
|  | `enable_disable_irq` | 清位放线/置位屏蔽的往返 | §4.3 |
|  | `generic_notify_handler_sends_notification` | 通用 handler 真的发通知 | §4.3 |
|  | `generic_notify_handler_no_reenable_keeps_active` | 无 REENABLE 位保持 | §4.3 |

（行数复算：`grep -c '#\[test\]' os/arch/src/arch/exception_dispatcher.rs` = 14、`grep -c '#\[test\]' os/kernel/src/page_fault.rs` = 8、`grep -c '#\[test\]' os/kernel/src/irq_manager.rs` = 9，三数相加 31，与上表 31 行一致。锚点：三个文件的 `mod tests`。**可执行性**：`exception_dispatcher.rs` 的 14 条当前不可宿主执行（§5.3 其一）——可执行 17 条；本表「钉住什么」列为应钉住的口径，不可执行项不计入覆盖读数。）

读数（全套件口径，实跑；读数随测试集增长，以现跑为准）：单元测试 `cd os && cargo test -p minix-kernel --features mock` → **832 passed / 0 failed / 8 ignored**（840 running）；集成测试 `os/kernel/tests/boot_integration.rs` → 3 passed / 0 failed / 3 ignored；doc-test 1 ignored。本篇直接相关项的过滤读数：`cargo test -p minix-kernel --features mock page_fault` → 14 passed / 0 failed（8 条 `page_fault::tests` + 4 条 `ipc::tests::user_copy_range_*page_fault*`（完整名含匹配段：`unmapped_page_is_page_fault`/`second_page_unmapped_is_page_fault`/`supervisor_page_is_page_fault`/`write_to_readonly_page_is_page_fault`）+ `test_deliver_message_first_page_fault` + `test_vmctl_clear_page_fault_requeues_target`，过滤口径按完整测试名含 page_fault）；`cargo test -p minix-kernel --features mock irq` → 24 passed / 0 failed / 1 ignored（ignored 的一条见 §5.3）。过滤读数只当冒烟，覆盖口径一律按上表 31 条与全套件数。

### 5.2 关键测试细读（→§4.1/§4.2/§4.3）

细读一：18 向量→信号的对账表测试。全函数（30 行，逐字）：

```rust
    #[test]
    fn classify_all_vectors() {
        let expected: &[(u8, ExceptionSignal)] = &[
            (0, ExceptionSignal::Fpe),
            (1, ExceptionSignal::Trap),
            (3, ExceptionSignal::Emt),
            (4, ExceptionSignal::Fpe),
            (5, ExceptionSignal::Fpe),
            (6, ExceptionSignal::Ill),
            (7, ExceptionSignal::Fpe),
            (8, ExceptionSignal::Bus),
            (9, ExceptionSignal::Segv),
            (10, ExceptionSignal::Segv),
            (11, ExceptionSignal::Segv),
            (12, ExceptionSignal::Segv),
            (13, ExceptionSignal::Segv),
            (15, ExceptionSignal::Ill),
            (16, ExceptionSignal::Fpe),
            (17, ExceptionSignal::Bus),
            (18, ExceptionSignal::Bus),
            (19, ExceptionSignal::Fpe),
        ];
        for &(v, expected_sig) in expected {
            let outcome = ExceptionDispatcher::<MockFrame>::classify_signal(
                InterruptVector::new(v),
            );
            assert_eq!(outcome, ExceptionOutcome::Signal(expected_sig),
                "vector {} signal mismatch", v);
        }
    }
```

（区间复算：`sed -n '417,446p' os/arch/src/arch/exception_dispatcher.rs | wc -l` = 30。锚点：`os/arch/src/arch/exception_dispatcher.rs:fn classify_all_vectors`。）

测试的形状就是 §3.3 处置表第一行的兑现：`expected` 数组把 18 对（向量，信号）逐值列出，逐对断言。这个形状与 C 的 `ex_data` 有一个刻意的差异——C 用数据表存映射、运行时查；Rust 的映射写在 `classify_signal` 的 match 臂里、测试再用一张数据表反向核对。两边各存一份、形状不同（数据表 vs 枚举 match），漂移会在测试点暴露：改了 match 漏改测试（或反之）即红。变红条件具体到值：`classify_signal` 的向量 5 臂若从 `Fpe` 改成 `Ill`，第 5 对断言失败并报出「vector 5 signal mismatch」。注意测试表只有 18 对——向量 2 与 14 不在表内（20 项 C 表 − 2），与 §4.1 的口径一致（这两个向量到不了分类）。

细读二：转发消息的四字段钉死。全函数（15 行，逐字）：

```rust
    #[test]
    fn test_build_vm_pagefault_msg_populates_fields() {
        // C: exception.c:119-122 — m_source / m_type / VPF_ADDR / VPF_FLAGS.
        let msg = build_vm_pagefault_msg(Endpoint(42), 0xcafe_f00d, 0x07);
        assert_eq!(msg.m_source, Endpoint(42));
        // m_type must be VM_PAGEFAULT — VM's main loop dispatches on this
        // exact value (vm_server.rs:436). A default m_type=0 would cause
        // VM to silently drop the page-fault request.
        assert_eq!(msg.m_type, VM_PAGEFAULT as i32);
        // SAFETY: `m_vm_pagefault` is the active union arm; we just wrote
        // it via `build_vm_pagefault_msg`.
        let pf = unsafe { msg.m_u.m_vm_pagefault };
        assert_eq!(pf.vpf_addr, 0xcafe_f00d);
        assert_eq!(pf.vpf_flags, 0x07);
    }
```

（区间复算：`sed -n '276,290p' os/kernel/src/page_fault.rs | wc -l` = 15。锚点：`os/kernel/src/page_fault.rs:fn test_build_vm_pagefault_msg_populates_fields`。）

四条断言与 §4.2 的四行 C 对位逐条咬合：`m_source` = 调用方给的端点、`m_type` = `VM_PAGEFAULT`（注释点出「默认值 0 会让 VM 无声丢弃」——§4.2 的要害句在测试里再钉一次）、`vpf_addr`/`vpf_flags` = 喂进去的两个值。`unsafe` 读联合体活跃臂的注释写明安全性依据（刚用 `build_vm_pagefault_msg` 写过这个臂）。变红条件：`build_vm_pagefault_msg` 漏掉任一字段赋值、`m_type` 换成别的常量、或 `VPF_ADDR`/`VPF_FLAGS` 两字段的填装互换。

细读三：通用 handler 的通知副作用。全函数（28 行，逐字）：

```rust
    #[test]
    fn generic_notify_handler_sends_notification() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        // Register via `irqctl_set_policy`, which installs `generic_notify_handler`.
        let proc_ep = Endpoint::KERNEL;
        let notify_id = IrqNotifyId(3);
        mgr.irqctl_set_policy(
            IrqVector::new(0),
            proc_ep,
            notify_id,
            IrqPolicy::REENABLE,
        ).expect("irqctl_set_policy should succeed");

        let mut notifier = MockNotifier::default();
        mgr.dispatch(IrqVector::new(0), &mut notifier).expect("dispatch should succeed");

        // The handler should have called notify_hardware exactly once with
        // the registered proc_endpoint and notify_id.
        assert_eq!(notifier.notify_log.len(), 1, "expected exactly one notify call");
        assert_eq!(notifier.notify_log[0].0, proc_ep, "notify dst endpoint mismatch");
        assert_eq!(notifier.notify_log[0].1, notify_id, "notify_id mismatch");

        // With REENABLE policy, the handler returns Completed → active bit cleared.
        assert_eq!(mgr.actids[0], 0, "REENABLE policy should clear active bit");
    }
```

（区间复算：`sed -n '1153,1180p' os/kernel/src/irq_manager.rs | wc -l` = 28。锚点：`os/kernel/src/irq_manager.rs:fn generic_notify_handler_sends_notification`。）

这条测试钉的是 §4.3 通知路径的副作用本体：经 `irqctl_set_policy` 注册（装入的就是 `generic_notify_handler`），派发一次，假 notifier 收到的调用恰好一次、目标端点与 `notify_id` 恰好是注册值，且 REENABLE 策略让活跃位清零。它存在的历史理由写在 doc 注释里（此处曾缺测试）——测试自己不依赖那段历史，钉住的是「注册→派发→通知→清位」这条链真的通。变红条件：`generic_notify_handler` 不再调 `notify_hardware`、通知目标换成别的端点、或 `REENABLE` 判定反向。

### 5.3 测试边界与诚实披露

三处边界如实列出，每处附复现命令。

其一，`os/arch` 的测试靶当前编译失败，`exception_dispatcher.rs` 的 14 条测试跑不起来。实跑证据：`cd os && cargo test -p minix-arch` 报 41 个编译错误，分布在多个模块的测试代码（`arch/boot.rs` 17、`x86_64/boot.rs` 6、`arch/dm_coverage.rs` 7、`arch/trap_return.rs` 7、`arch/frame.rs` 2、`x86_64/paging.rs` 2 等）：无前缀的 `Vec`/`vec!`（no_std 下须写 `alloc::vec::Vec`）、未导入的 `Box`、`paging::mock` 不可见，以及 `MockTrapReturn`——其定义被 `runtime-window` feature 门在测试构建之外（报错 note 即「configured out」）。编译在测得任何一条之前就停了。`exception_dispatcher.rs` 自己的 14 条测试不在这份错误清单里（它只用 `core` 类型），但同靶编译失败使它们全部无法执行。§5.1 表中该文件的 14 条应读作「已写、对得上源码、当前不可宿主执行」，修复方向是给涉事模块补 `alloc::` 前缀与缺失替身——本篇不动代码，只把现状钉在这。

其二，x86 分发体本身没有宿主测试直接调用。`os/kernel/src/trap_dispatch.rs` 的异常分发体是 `#[cfg(target_arch = "x86_64")]` 的真机路径（陷入存根调用它），宿主测试（mock 形态）不构建这条路径；它的行为由三部分间接覆盖：决策半被 `exception_dispatcher.rs` 的测试覆盖（§5.1）、执行半的关键收尾被 `page_fault` 与 `ipc` 的转发/拷贝测试覆盖（§5.1 过滤读数里的 6 条非 `page_fault::tests` 项）、真机侧由 qemu 回归钉住（设备 IRQ 交付链有 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq/` 测试内核：PIT → IOAPIC → 陷入存根 → `IrqManager::dispatch` → 时钟 handler → uptime 推进，PASS 判据是 uptime 到 5 拍）。本内核（页表式保护的三架构）没有 C 那对 `copy_msg` 函数区，嵌套拷贝重定向没有生产路径可测——`nested_user_copy_msg` 钉的是决策半的形状，不是可触发的行为（§3.2 的披露在测试侧的对应）。

其三，一条 IRQ 相关测试被忽略。`cargo test -p minix-kernel --features mock irq` 的读数里有 1 ignored：`clock::clock_irq_handler_tests::test_clock_hook_registered_in_manager`，忽略理由是它要经真实控制器放线（x86 需要 iopl 特权或 qemu 环境）。宿主能测的 hook 注册/派发逻辑由 `irq_manager.rs` 的 9 条覆盖；「注册动作经真控制器放线」这一半归真机门。

宿主与真机的分界一句话：宿主测的是「分派的决策面与 hook 池的状态机」（枚举、匹配、位图、协议字段），真机测的是「事件真的进来」（陷入、寄存器、控制器寄存器、VM 在场）。

### 5.4 覆盖状态反转表（→Ch4；宿主测试面的反面）

宿主测试覆盖「分派的决策面与 hook 池的状态机」，反转列「代码里存在但宿主测试钉不住/生产不可达」的面——每行给出真实性的另一侧：

| # | 面（Rust 侧） | 宿主测试覆盖 | 反转侧（生产/真机面） | 状态 |
|---|---|---|---|---|
| M1 | `handle`/`handle_nested`/`handle_page_fault` 决策面 | 14 条决策测试（当前不可执行，§5.3 其一） | 不可执行期间的守卫 = 源码逐字复核 + 对账 | 部分覆盖 |
| M2 | `RedirectToRecovery`/`PhysCopyFault` 执行臂 | 无（panic 臂不可测） | 生产路径不设登记者（验证优先）；出现即不变量破坏 | 未覆盖（无 C 源生产路径） |
| M3 | `FpuTrap` 执行臂（懒 FPU 恢复） | 无宿主测试 | 真机 #NM 路径；FPU 保存恢复本体归 31 方向 | 未覆盖（宿主） |
| M4 | 转发路径的「调用方补队」义务 | 转发组间接覆盖（VMCTL 清位测试） | 真机交付链（timer-irq 型测试内核） | 部分覆盖 |
| M5 | 设备 IRQ 经真控制器放线 | 1 条 ignored（`test_clock_hook_registered_in_manager`，要 iopl/qemu） | 真机门 | 未覆盖（宿主） |
| M6 | `krandom` 中断熵源 | 无 | TSC 采样实现（`[ARCH: deviation]`），源码注释待同步（R14-T04） | 未覆盖（宿主） |

交接编号（`R14-T*`，与源码/测试工作流的接口）：**R14-T01** minix-arch 测试靶编译修复（41 errors；修后 `exception_dispatcher` 14 条可执行）；**R14-T02** 源码注释节号时效（`exception_dispatcher.rs`/`irq_manager.rs` 的 3.4–3.7 式节号引用与现行节号不同步）；**R14-T03** `ExceptionClass` 接线或删除（代码卓越轮）；**R14-T04** `krandom` 注释同步（`irq_manager.rs` 的 no-op stub 注释已过期）；**R14-T05** `FaultContext` 登记者（24 方向引入捕获窗时的落位点）；**R14-T06** `VmPagefaultIn.endpoint` 内核侧死值清理或接线（内核构造只填 `NONE`；VM 解码侧另有写入，§4.1）。

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认命令与预期关键输出 |
|---|---|---|
| 控制台打出 `user exception: vector N ...`（Rust 文案）后用户进程死 | 用户异常转信号路径；N 是向量号，查 §2.1 表得信号 | `sed -n '604,620p' os/kernel/src/trap_dispatch.rs`（Rust 打印点）；`sed -n '20,39p' minix3/minix/kernel/arch/i386/exception.c`（C 表逐行在）——N=0/4/5/7/16/19 全是 SIGFPE，别急着怀疑浮点 |
| 内核 panic `kernel exception vector N at rip ...`（Rust 文案） | 内核执行流中的异常，走 `KernelPanic` 臂 | `sed -n '516,543p' os/kernel/src/trap_dispatch.rs`（打印臂）；向量含义查 §2.1 表；N=13（GP）优先怀疑内核代码碰了非法地址/特权指令 |
| panic `pagefault in VM`（Rust 与 C 同源文案） | VM 进程自己缺页（转发目标=自己的死结被掐断） | `sed -n '544,563p' os/kernel/src/trap_dispatch.rs`（打印含 cpu/rip/cr2/errcode）；C 对位 `sed -n '99,113p' minix3/minix/kernel/arch/i386/exception.c` |
| 用户进程长期不被调度，诊断显示 `RTS_PAGEFAULT` 位未清 | 消费半没跑：VM 服务完但 `VMCTL_CLEAR_PAGEFAULT` 未达，或转发未投出 | `sed -n '33,36p' minix3/minix/kernel/system/do_vmctl.c`（清位半）；`cargo test -p minix-kernel --features mock vmctl` → `test_vmctl_clear_page_fault_requeues_target ... ok` |
| 消息发出后 VM 全静默、机器 idle（故障进程仍停 PAGEFAULT） | 转发只清了 `RTS_RECEIVING` 没补入队（§4.2 的义务半） | `cargo test -p minix-kernel --features mock page_fault` → 14 passed；走查 `sed -n '757,800p' os/kernel/src/trap_dispatch.rs` 的唤醒收集段 |
| 设备中断后驱动再没被通知，控制台偶见 `irq_handle: spurious irq N`（C 文案；Rust 侧为 `IrqError::Spurious` 返回值，计数在调用点） | 该线没有注册 hook（或 hook 被误注销） | `grep -rn 'put_irq_handler' minix3/minix/kernel/ \| head`（C 注册点）；`grep -rn 'register_hook' os/kernel/src/ \| head`（Rust 注册点）；`cargo test -p minix-kernel --features mock spurious` → `spurious_irq ... ok` |
| ARM64/RISC-V 真机上同一中断反复重入、永不退役 | claim/complete 配对破坏：完成写回的不是认领读到的身份 | `cargo test -p minix-kernel --features mock dispatch_claims` → `dispatch_claims_before_mask_and_completes_last ... ok`（全序断言 claim→mask→链→unmask→complete）；走查 §4.3 |
| 进程收到 SIGILL 但代码没有非法指令 | NULL 槽向量 15（Intel 保留/软件陷阱落点）或其他映射 SIGILL 的向量（6） | §2.1 表的 NULL 槽行；对照 `classify_all_vectors` 的期望表 |

### 6.2 手算与常量核对命令

全部命令实跑过，括号内为关键输出。

```bash
# 1. ex_data 表项数（两种口径）
sed -n '20,39p' minix3/minix/kernel/arch/i386/exception.c | wc -l        # 20（表项总数）
grep -c '{ "' minix3/minix/kernel/arch/i386/exception.c                  # 19（有名字的表项；NULL 行不以 '{ "' 开头，不计入）
# 2. 文件行数
wc -l minix3/minix/kernel/arch/i386/exception.c minix3/minix/kernel/interrupt.c
# （386 / 177）
# 3. Rust 三文件行数
wc -l os/arch/src/arch/exception_dispatcher.rs os/kernel/src/page_fault.rs os/kernel/src/irq_manager.rs
# （447 / 356 / 1209）
# 4. VM_PAGEFAULT 的数值手算
grep -n 'pub const VM_RQ_BASE' os/libs/minix-types/src/ipc/vm.rs          # 0xC00
grep -n 'pub const VM_PAGEFAULT' os/libs/minix-types/src/ipc/vm.rs        # VM_RQ_BASE + 0xFF
# 0xC00 + 0xFF = 0xCFF = 3327（i32 正区间，m_type 转换安全）
# 5. 向量→信号对账（C 表 vs Rust 测试期望表，并排人工核对）
sed -n '20,39p' minix3/minix/kernel/arch/i386/exception.c
sed -n '419,438p' os/arch/src/arch/exception_dispatcher.rs
# 6. 全套件读数
cd os && cargo test -p minix-kernel --features mock   # 832 passed / 0 failed / 8 ignored
# 7. minix-arch 测试靶现状（预期编译失败，见 §5.3 其一）
cd os && cargo test -p minix-arch 2>&1 | tail -3       # error: could not compile `minix-arch` (lib test)
```

第 5 条的对账规则：C 表与 Rust 期望表按**向量号**核对（C 表 20 行里向量 2/14 两行在期望表缺席，不能按位置对齐）——SIGFPE/SIGTRAP/SIGBUS/SIGEMT/SIGILL/SIGSEGV 六类对应 Fpe/Trap/Bus/Emt/Ill/Segv 六个变体；C 表的 2/14 两行在 Rust 期望表里缺席（它们到不了分类，§4.1）。`minprocessor` 列无对账对象：Rust 侧整个淘汰（§3.3）。

常量速查：`PAGE_FAULT_VECTOR` = 14、`PROTECTION_VECTOR` = 13（`minix3/minix/kernel/arch/i386/include/archconst.h:PAGE_FAULT_VECTOR`、`:PROTECTION_VECTOR`）；`DEBUG_VECTOR` = 1、x86 的 `NR_IRQ_VECTORS` = 16（无 APIC）/ 64（APIC）（`minix3/minix/include/arch/i386/include/interrupt.h:DEBUG_VECTOR`、`:NR_IRQ_VECTORS`）；`NR_IRQ_HOOKS` 无 APIC 16 / 有 APIC 64（§2.4 引文）；Rust 侧 `NR_IRQ_VECTORS` = `NR_IRQ_HOOKS` = 64（`os/plat/src/interrupt.rs:const NR_IRQ_VECTORS`、`:const NR_IRQ_HOOKS`）——Rust 池比 C 的无 APIC 档大，比有 APIC 档持平，三架构共用一套上限。

---

## 7. 参见

- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) §2.3（`intr_init` 的十次寄存器写——控制器初始化）、§3.3（`InterruptRouter`/`PerCpuInterruptUnit` 两 trait 拆分）、§3.4（控制器的归宿是全局 `IRQ_MANAGER`）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1（IRQ 钩子池清零进 `IrqManager::new` 构造）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) §2.3（`CLEAR_PAGEFAULT`/MEMREQ 消费端——本篇转发路的生产端对侧）
- [10-switch-to-user.md](10-switch-to-user.md) §2.3（pick_proc 与 restore 的进出配对——事件把进程停下后，出去侧怎么选人）、§3.3（Stage 3a 重派——异常/中断之外的第三类循环事件）
- [11-scheduling-primitives.md](11-scheduling-primitives.md) §2.2（`RTS_PAGEFAULT`/`RTS_SIGNALED` 置位后的队列半）、§4.2（包装层与两分支——`rts_set` 带出队半的出处）
- [12-ipc-core.md](12-ipc-core.md) §2.3（mini_send 的内核副本与阻塞排队——`copy_msg_from_user` 的消费点）、§2.5（NOTIFY 的位图旁路——IRQ 通知的语义底座）、§2.7（delivermsg 延迟拷贝——`copy_msg_to_user` 的消费点）
- [13-syscall-dispatch.md](13-syscall-dispatch.md)（syscall 指令陷入的传参与 `TrapStyle`——本篇异常帧之外的另一类陷入）
- `19-syscall-signal` 方向（信号用户态接口，尚未重生成）；`24-cross-space-runtime` 方向（VMSUSPEND 恢复与跨空间拷贝、按需页捕获的归属，尚未重生成）；`31-fpu-context-switching` 方向（懒 FPU 恢复本体，尚未重生成）；`17-clock-timer` 方向（时钟 handler 内部，尚未重生成）；`23-ipc-filter`（IPC 过滤决策，尚未重生成）
