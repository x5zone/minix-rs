# 10-switch-to-user: 调度循环入口

> **分类**: Kernel 调度核心
> **源码**: `minix3/minix/kernel/proc.c:299-474`（switch_to_user）, `proc.c:176-229`（idle）
> **前置**: 08（bsp_finish_booting 调用 switch_to_user）+ 09（VM 启动协议完成，VM 可调度）
> **C 总行数**: ~180 行

---

## 1. 概述

### 1.1 内核的最后一道问题：接下来由谁占用处理器

一台机器上往往同时存在几十个"想要运行"的进程，但任何时刻每个 CPU 只能执行一条指令流。操作系统必须持续回答一个问题：**此刻这个 CPU 应该执行谁的代码？** 在 Minix3 中，回答这个问题的地方不是某个后台运行的"调度器线程"，而是一个普通函数——`switch_to_user()`。它的特别之处在于：**它从不把答案"返回"给调用者，而是直接把处理器交出去**。

为什么调度不做成一个独立线程？因为微内核的中断处理模型决定了这样最自然：当硬件中断、CPU 异常或系统调用把 CPU 带进内核时，内核完成手头的处理工作（投递消息、处理页错误、执行系统调用），接下来自然要面对"之前被中断的那个进程还能不能继续跑"的判断。与其让内核先退出、再由另一个线程重新调度，不如就在进入内核的同一个执行流里把下一步定好——`switch_to_user()` 就是这个"下一步"。

这个函数因此有了两个鲜明的性质：

1. **它是所有内核工作流的汇聚点**。无论内核因为什么被进入，离开内核的方式只有一种——经过这个函数选出的进程恢复运行。
2. **它自身永不返回**。C 源码在函数末尾写 `NOT_REACHABLE`（proc.c:473）来提醒读者；Rust 用返回类型 `!`（[lib.rs:2757](../../os/kernel/src/lib.rs)）让编译器强制检查。函数体是一个无限循环，唯一的"退出循环"方式是执行架构相关的特权返回指令（x86-64 的 `iretq`、ARM64 的 `eret`、RISC-V 的 `sret`），把 CPU 交给用户态——之后循环从下一次陷入内核处重新开始。

### 1.2 三条进入路径，一个出口

内核每次被进入，都带着"待处理的内核工作"；工作处理完毕后，控制流都汇入调度循环：

```
硬件中断 → interrupt() → 保存上下文 → switch_to_user()
CPU 异常 → exception() → 保存上下文 → switch_to_user()
系统调用 → sys_call()  → 保存上下文 → switch_to_user()
```

无论哪条路径进入，调度循环面对的决策完全相同——它不关心"内核为什么被进入"，只关心三件事：

1. **刚才被中断的进程还能继续运行吗？**（可运行性检查）
2. **有没有被延迟到调度时才处理的工作？**（杂项标志处理：延迟的系统调用、挂起的内核调用恢复、消息投递）
3. **该进程的时间片用完了吗？**（量子检查——用完了要通知调度服务器，而不是直接换人）

前两问的答案决定了"继续跑当前进程"还是"重新挑一个"；第三问即使在"继续跑"的情况下也要回答，因为时间片是给用户态调度器（SCHED 服务器）的信号，错过它就破坏了协作调度的协议。

### 1.3 控制流总览

C 用两个标号（`not_runnable_pick_new`、`check_misc_flags`）和三个 `goto` 把上述决策编织进一个函数体。Rust 把同样的控制流重排为一个外层循环加五个顺序阶段；两个 `goto not_runnable_pick_new` 变成 `continue`：

```
switch_to_user()（Rust 版本，永不返回）
  │
  ├─ 阶段 1：当前进程仍可运行？ ──是──→ 阶段 3
  │         （proc_ptr 快照 + is_runnable 判断）
  │                              否
  ├─ 阶段 2：not_runnable_pick_new     ↓
  │   ├─ PREEMPTED 进程按剩余量子重新入队（队头或队尾）
  │   ├─ 反复 pick_proc()，直到选出进程；队列全空则 idle() 停机等中断
  │   ├─ 更新 per-CPU 的 proc_ptr
  │   └─ switch_address_space()：安装该进程的页表根（必要时）
  │
  ├─ 阶段 3：process_misc_flags（check_misc_flags）
  │   while 循环按优先级处理 5 个杂项标志：
  │   KCALL_RESUME → kernel_call_resume
  │   DELIVERMSG   → delivermsg
  │   SC_DEFER     → arch_do_syscall
  │   SC_TRACE     → cause_sig(SIGTRAP)
  │   SC_ACTIVE    → 清除后跳出
  │   处理中进程不可运行 → 回到阶段 2（Rust: 返回 false → continue）
  │
  ├─ 阶段 4：check_quantum
  │   时间片耗尽 → proc_no_time()（通知调度服务器或重置量子）
  │   处理后不可运行 → 回到阶段 2（Rust: 返回 false → continue）
  │
  └─ 阶段 5：finish_and_restore（终局分派）
      ├─ context_stop(KERNEL)：内核执行时长记账 + TSC 基线推进
      ├─ 释放 BKL（大内核锁）——从此处直到下一次陷入内核
      ├─ FPU 所有权：非持有者置 CR0.TS（下次浮点指令陷入内核）
      ├─ 清除 MF_CONTEXT_SET
      └─ 从 cpu_context 重建 trap frame → restore_to_user() ← 永不返回
```

---

## 2. C 源码分析

### 2.1 switch_to_user() — 主循环

**源码**: `proc.c:299-474`

**阶段 1+2：进程选择**（proc.c:309-349）

```c
p = get_cpulocal_var(proc_ptr);
if (proc_is_runnable(p))
    goto check_misc_flags;

not_runnable_pick_new:
    if (proc_is_preempted(p)) {
        p->p_rts_flags &= ~RTS_PREEMPTED;
        if (proc_is_runnable(p)) {
            if (p->p_cpu_time_left)
                enqueue_head(p);
            else
                enqueue(p);
        }
    }
    while (!(p = pick_proc())) {
        idle();
    }
    get_cpulocal_var(proc_ptr) = p;
```

这段代码的细节比看上去多：

- **PREEMPTED 的两种下场**（proc.c:322-330）：被抢占的进程（`RTS_PREEMPTED` 置位）解除标志后，如果时间片还有剩，就从**队头**重新进入就绪队列——它还没用完自己的份额，应该先于同优先级的其他进程继续跑；如果时间片已经耗尽，就从**队尾**进入——它要排队重新领取时间片。这一区分是公平调度的关键。
- **不可运行就不入队**（proc.c:324）：解除 PREEMPTED 后进程可能因为其他 RTS 标志（比如正在等待消息）而仍然不可运行，此时直接跳过入队——抢占处理不能违背"可运行才入队"的基本不变量。
- **idle 不是函数调用那么简单**（proc.c:338-340）：`while (!(p = pick_proc())) idle();` 意味着队列全空时 CPU 反复进入停机等待，每次被中断唤醒后重新扫描队列。`idle()` 本身见 §2.2。

**阶段 3：地址空间切换**（proc.c:349）

```c
switch_address_space(p);
```

注意时机：切换发生在**选出进程之后、处理杂项标志之前**。这不是随意的安排——后续的 `delivermsg()` 要在目标进程的地址空间里拷贝消息，所以地址空间必须先就位。函数实现在 `klib.S:605-626`（i386 汇编），语义见 §2.3。

**阶段 4：杂项标志处理**（proc.c:351-415，`check_misc_flags:` 标号起）

```c
while (p->p_misc_flags &
    (MF_KCALL_RESUME | MF_DELIVERMSG |
     MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE)) {

    assert(proc_is_runnable(p));
    if (p->p_misc_flags & MF_KCALL_RESUME) {
        kernel_call_resume(p);
    }
    else if (p->p_misc_flags & MF_DELIVERMSG) {
        delivermsg(p);
    }
    else if (p->p_misc_flags & MF_SC_DEFER) {
        arch_do_syscall(p);
        /* MF_SIG_DELAY 时 sig_delay_done(p) —— proc.c:379-381 */
    }
    else if (p->p_misc_flags & MF_SC_TRACE) {
        if (!(p->p_misc_flags & MF_SC_ACTIVE))
            break;                      /* 非系统调用离开路径，不追踪 */
        p->p_misc_flags &= ~(MF_SC_TRACE | MF_SC_ACTIVE);
        cause_sig(proc_nr(p), SIGTRAP); /* 阻塞进程，等 PM 处理 */
    }
    else if (p->p_misc_flags & MF_SC_ACTIVE) {
        p->p_misc_flags &= ~MF_SC_ACTIVE;
        break;
    }

    if (!proc_is_runnable(p))
        goto not_runnable_pick_new;     /* proc.c:413-414 */
}
```

五个标志按固定优先级串行处理——`if-else` 链的排列顺序就是优先级顺序：内核调用恢复（KCALL_RESUME）最优先，其次是消息投递（DELIVERMSG）、延迟系统调用（SC_DEFER）、系统调用追踪（SC_TRACE），最后是系统调用活动标记（SC_ACTIVE）。`SC_TRACE` 与 `SC_ACTIVE` 的组合逻辑需要特别注意：`SC_TRACE` 只在"确实处于系统调用离开路径"（`SC_ACTIVE` 同时置位）时才触发追踪，否则直接跳出循环——这是为了让追踪事件严格对应"正在离开系统调用"的时刻。

每处理完一个标志都要重新检查可运行性（proc.c:413-414），因为处理过程本身可能让进程阻塞——比如 `delivermsg` 拷贝消息时目标缓冲区缺页，进程会被挂起等待 VM。

**阶段 5：时间片检查**（proc.c:421-428）

```c
if (!p->p_cpu_time_left)
    proc_no_time(p);
if (!proc_is_runnable(p))
    goto not_runnable_pick_new;
```

`proc_no_time()`（proc.c:1893-1910）按调度策略分两支：**用户态调度器管理的进程**（SCHED 服务器通过 SYS_SCHEDULE 接管）走 `notify_scheduler(p)` 向调度服务器发送 `SCHEDULING_NO_QUANTUM` 消息——注意 proc.c:1896 的注释明言"this dequeues the process"，这个调用同时把进程从就绪队列摘除；**内核直接管理的进程**只是把时间片重置为 `p_quantum_size_ms` 换算出的循环值，它们绕过调度，不需要通知任何人。时间片检查放在这个位置（而不是进入内核时）是有意的——proc.c:416-419 的注释解释了原因：只有在这里发送的"时间片用完"消息才不会与普通 IPC 竞争。

**阶段 6：终局分派**（proc.c:437-474）

```c
p = arch_finish_switch_to_user();
assert(p->p_cpu_time_left);

context_stop(proc_addr(KERNEL));

/* FPU 所有权检查 */
if (get_cpulocal_var(fpu_owner) != p)
    enable_fpu_exception();
else
    disable_fpu_exception();

p->p_misc_flags &= ~MF_CONTEXT_SET;

#ifdef CONFIG_SMP
/* MF_FLUSH_TLB 刷新 —— 仅 SMP（proc.c:458-464）*/
#endif
restart_local_timer();

restore_user_context(p);  /* 不返回 */
NOT_REACHABLE;
```

各步骤的含义：

- `arch_finish_switch_to_user()`（arch_system.c:495-513）做两件事：把当前进程指针存到内核栈顶（汇编恢复路径要用），以及**把保存的 PSW 或上 IF_MASK**（arch_system.c:512）——确保恢复出的用户上下文开着中断。
- `context_stop(proc_addr(KERNEL))`（arch_clock.c:208-349）是记账函数：把"从上次切换到现在"的 TSC 差值记到 KERNEL 伪进程头上（这就是本次内核执行的时长），并推进 per-CPU 的 TSC 基线。在 SMP 配置下它还兼任 **BKL 释放点**——arch_clock.c:226-233 的 `must_bkl_unlock` 分支在统计完 KERNEL 时长后解锁大内核锁，因为接下来要么进入用户态（BKL 应当空闲），要么进入 idle 停机（同样不能持锁）。这是 C 代码里唯一一处"内核即将把 CPU 让出去"的语义边界。
- FPU 所有权是惰式 FPU 切换的核心：只有当前持有 FPU 的进程才能无陷阱地执行浮点指令；其他进程的浮点指令会触发 #NM（x86）陷入内核，由内核保存/恢复 FPU 状态后再放行。
- 清除 `MF_CONTEXT_SET`：该标志表示"寄存器保存区已由显式写入（如 SYS_TRACE 的 T_SETUSER）更新"。分派完成后上下文已经消费，下次内核进入必须重新保存。
- `restore_user_context(p)` 恢复全部寄存器并执行特权返回指令。C 在其后写 `NOT_REACHABLE`（proc.c:473）——这只是给人看的注释；Rust 的 `!` 类型让编译器做同样的检查。

### 2.2 idle() — CPU 的休眠姿态

**源码**: `proc.c:176-229`

```c
static void idle(void) {
    struct proc * p;

    p = get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
    if (priv(p)->s_flags & BILLABLE)
        get_cpulocal_var(bill_ptr) = p;

    switch_address_space_idle();          /* 仅 CONFIG_SMP：切到 VM 的地址空间 */

#ifdef CONFIG_SMP
    get_cpulocal_var(cpu_is_idle) = 1;
    if (cpuid != bsp_cpu_id)
        stop_local_timer();               /* AP 停表：时间在 BSP 上记 */
    else
#endif
    {
        restart_local_timer();            /* BSP：保证停机后定时器仍会响 */
    }

    context_stop(proc_addr(KERNEL));      /* 开始统计 idle 时长 */
#if !SPROFILE
    halt_cpu();                           /* STI + HLT，等下一次中断 */
#else
    /* sprofiling 模式改为轮询 idle_interrupted —— proc.c:211-229 */
#endif
}
```

idle 的每一步都有明确目的：

1. **把 IDLE 伪进程登记为"当前进程"**（proc.c:185-187）。这不是形式主义：`context_stop` 只对"当前进程"记账，CPU 空转的时长要记到某个名字下，IDLE 就是那个名字。IDLE 的特权结构带 `BILLABLE` 标志（priv.h:36 `IDL_F = SYS_PROC | BILLABLE`），所以 `bill_ptr` 也指向它。
2. **`switch_address_space_idle()` 只在 SMP 下有内容**（proc.c:160-170）：切到 VM 的地址空间，赌 VM 的页表把内核映射好，这样中断到来时内核能直接跑。单 CPU 构建里这一步是空的——内核地址空间本来就活跃。
3. **BSP 保持定时器运转**（proc.c:198-204）：停机期间唯一的唤醒源就是时钟中断（或设备中断），BSP 的定时器必须活着；AP 的时间统计都在 BSP 上做，所以 AP 可以停掉本地定时器省电。
4. **`context_stop(KERNEL)` 先于停机执行**：从上次切换到现在的时长记为内核执行时间；之后 CPU 停着的时间会在下一次 `context_stop`（唤醒后）被记为 idle 时间——两次记账共用同一个 TSC 基线。
5. **`halt_cpu()`**（klib.S:407-414）是 `sti; hlt` 两连：先开中断再停机。顺序不能反——带中断屏蔽停机等于永远睡死。`hlt` 被一个中断唤醒后，中断处理函数返回到 `hlt` 的下一条指令，函数返回，调用方（调度循环）重新尝试 `pick_proc()`。
6. **idle 结束时不做任何统计**（proc.c:221-222 注释）：中断返回后内核还要处理很多工作才回到调度循环，此时才由下一次 `context_stop` 把"idle + 后续内核工作"一起结算——idle 的时长精确值在那时才被划分出来。

### 2.3 周边协作函数速览

| 函数 | C 位置 | 语义 |
|------|--------|------|
| `pick_proc()` | proc.c:1785-1813 | 从本地 CPU 的 16 级优先级队列中取最高优先级的队头进程；若其特权结构带 `BILLABLE` 则更新 `bill_ptr`（proc.c:1808-1809）——系统时间要记到这个进程头上 |
| `__switch_address_space()` | klib.S:605-626（i386） | 三种情形：新 CR3 为 0（内核任务）→ 不动；新 CR3 等于当前 CR3 → 不动（省一次无谓的 TLB 清空，klib.S:614-620）；否则写入 CR3 并把 ptproc 指向该进程（klib.S:621-624）。**注意等值情形连 ptproc 都不更新**——`je 0f` 同时跳过寄存器写入和指针存储 |
| `context_stop()` | arch_clock.c:208-349 | TSC 差值记账 + 基线推进 + 量子扣减（`p_endpoint >= 0` 才扣，arch_clock.c:314）；SMP 下含 BKL 解锁分支（arch_clock.c:226-233） |
| `restart_local_timer()` | arch_clock.c:168-175 | 重启 LAPIC 定时器；`if (lapic_addr)` 表明无 LAPIC 时是空操作 |
| `halt_cpu()` | klib.S:407-414 | `sti; hlt` |
| `enable_fpu_exception()` | exception.c:375-380（i386） | CR0 已含 TS 则跳过，否则置 TS——下次浮点指令触发 #NM |
| `disable_fpu_exception()` | exception.c:382-385（i386） | `clts`——允许浮点指令 |

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_to_user 返回类型 | 返回值（C 风格） vs 发散类型 | **`-> !`** | C 的 `NOT_REACHABLE` 只是注释；Rust 用类型系统表达"永不返回"，编译器强制所有路径发散（[lib.rs:2757](../../os/kernel/src/lib.rs)） |
| BKL 释放时机 | 函数顶部释放（被否方案） vs 全程持有至记账点 | **全程持有，在 `finish_and_restore`/`idle` 的记账步之后释放** | 杂项标志阶段会改动共享内核状态（`process_misc_flags` 文档明言运行于 BKL 之下，proc_table.rs:781-783；`arch_do_syscall` 重新分发 IPC 依赖同一前提），顶部释放会打开数据竞争窗口。C 的释放点正是 `context_stop(KERNEL)` 内部（arch_clock.c:226-233），Rust 对齐同一位置。[ARCH: 释放点与 C 的 context_stop 对齐；单 CPU 构建，SMP 场景待重新验证] |
| 首次分派 | boot 时一次性预写所有 trap frame vs 每次分派前从 `cpu_context` 重建 | **每次分派重建**（`finish_and_restore` 第 7 步） | C 的 `p_reg` 同时是初值和保存值，Rust 的 `cpu_context` 继承同一角色；预写方案需要 boot 路径增设一次性钩子，且与未来的 trap-entry 保存路径无法统一（doc 06 §3.12 对此有对应修订） |
| proc_ptr 存储 | 全局变量 vs CpuLocal | **`CpuLocal.proc_ptr: Option<ProcNr>`**（smp.rs:139） | SMP 安全，每 CPU 独立；`Option` 表达"当前可能无进程" |
| 杂项标志循环 | while + if-else 链 vs match | **while + if-else 链**（proc_table.rs:764） | if-else 链的排列即优先级顺序，与 C 的 proc.c:360-407 一一对应；match 需要先做优先级提取，反而模糊语义 |
| restore 的抽象 | 直接内联汇编 vs trait | **`TrapReturnArch` trait**（trap_return.rs:72） | 与 `TrapEntryArch`（进入路径）对称，构成"进入/返回"一对抽象；OS 层只依赖 trait，架构差异（iretq/eret/sret）完全下沉到 arch crate |
| TrapReturnArch 的 Frame 来源 | 借用 `ExceptionArch::Frame` vs 独立关联类型 | **独立关联类型** | 借用式会强迫 mock 实现补齐 ExceptionArch 的 8 个无关方法；x86-64 实现直接复用 `X86_64ExceptionFrame`，实践中无重复 |
| 地址空间切换 | 新增 `Paging::switch_root` vs 复用 `TlbArch::set_active_root` | **复用**（tlb_arch.rs:135） | 该方法已为 VMCTL_SETADDRSPACE 而生（dispatch_vmctl 调用，syscall.rs:2036-2043）；调度器增加第二个调用方不构成"新抽象"的理由，新增 trait 反而制造单方法冗余 |
| CR3 相等比较 | 读硬件寄存器（C 做法） vs 软件镜像 | **软件镜像 `CURRENT_ROOT_PHYS`**（lib.rs:2265） | trait 边界内没有"读 CR3"的开口（`TlbArch` 只提供写入）；镜像由 `set_active_root_tracked`（lib.rs:2324）在每次硬件写入后同步更新，语义等价且便于测试 |
| gotos | 保留标号语义 vs 循环重构 | **外层 loop + continue** | 两个 `goto not_runnable_pick_new` 都是从循环体深处跳回"重新选进程"，与 `continue` 语义精确对应；标号在 Rust 中表达为循环边界，控制流等价且无 unsafe |
| idle 的停机原语 | 复用 `SmpArch::halt_cpu` vs 新增 `idle_halt` | **新增 `idle_halt`**（arch/smp.rs:84） | 语义不同：`halt_cpu` 服务于 IPI 停机（中断上下文，IF 已由入口路径管理），`idle_halt` 是"先开中断再停机"（klib.S:407-414 的 `sti; hlt`）——x86 上顺序错了就永远睡死，不能共用 |

### 3.1 为什么 BKL 必须持有到记账点才释放

这是本次实现中最重要的一次决策，值得单独说明。

直觉上"调度循环只读共享状态，进循环前就可以释放 BKL"——这个论证对"选进程"阶段成立，对"杂项标志"阶段不成立：`process_misc_flags` 会调用 `ipc::delivermsg`（真正拷贝消息）、`vm::kernel_call_resume`（读取 VM 结果）、`arch_do_syscall`（完整重新分发 IPC）——这些操作触碰进程表、特权表和 IPC 引擎的共享数据结构，proc_table.rs:781-783 的文档明确写了"Called from `process_misc_flags` which runs under BKL (held by `switch_to_user`)"。如果 BKL 在进循环前就释放了，这个承诺就是空头支票：SMP 下另一个 CPU 可以同时闯进来修改同一张进程表。

对照 C 的实际释放点可以确认正确答案：C 的 BKL 解锁发生在 `context_stop(KERNEL)` 内部（arch_clock.c:226-233），即**所有内核工作完成之后、CPU 即将让出之前**。Rust 现在把释放点放在 `finish_and_restore` 的记账步之后和 `idle` 的停机之前——与 C 语义逐点对齐。

单 CPU 构建下这个决策没有可观察的行为差异（BKL 总是无竞争地获取），但类型和文档上的承诺必须与意图一致：这段代码的每一行都应该按"SMP 正确"的标准书写，等 SMP 真正落地时不需要回头补课。

### 3.2 为什么"首次分派"被合并进"每次分派"

一个看似自然的方案是在进入调度循环前，由 boot 路径遍历进程表，把每个 boot 进程的 `cpu_context` 预写到一份 trap frame 上。这个方案的问题在于它制造了一条概念裂缝——trap frame 有了两个来源（boot 预写 vs 运行时保存），而第二个来源当时还不存在（trap entry 的保存路径属于 14 号文档）。

C 的模型没有这道裂缝：`p_reg` 既是进程出生时的初值，也是每次陷入时被覆盖写的保存区，恢复时一律从它读。Rust 的 `cpu_context` 是 `p_reg` 的直接对应物（arch 私有、不透明、长期存储在 `KProcess` 里——doc 06 §3.5 的设计），所以正确的做法就是让**每次分派**都从 `cpu_context` 重建 trap frame：第一次运行时它是初值，之后的运行中它被 trap entry 保存路径持续刷新。一条路径，两种身份，与 C 完全同构。doc 06 §3.12 的伪代码与叙述同步按此模型修订。

---

## 4. Rust 实现

### 4.1 主循环

**位置**: [lib.rs:2757](../../os/kernel/src/lib.rs)（`fn switch_to_user() -> !`），配套五个阶段函数（lib.rs:2324-2755）

主循环本身很薄——五个阶段各是一个命名函数，循环体只做调度与串联：

```rust
fn switch_to_user() -> ! {
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let bsp = smp.bsp_cpu_id();

    // proc_ptr = IDLE（对应 C main.c:54 的 per-CPU 一半）
    if let Some(local) = smp.cpu_local_mut(bsp) {
        local.proc_ptr = Some(proc_nr::IDLE);
    }

    loop {
        let mut current = smp.cpu_local(bsp).and_then(|l| l.proc_ptr);

        let need_pick = current.is_none_or(|nr| {
            !table.get(nr).is_some_and(|p| p.is_runnable())
        });
        if need_pick {
            if let Some(cur) = current {
                requeue_if_preempted(table, cur);       // proc.c:321-330
            }
            let picked = loop {
                if let Some(p) = pick_and_bill(table, smp, priv_table) {
                    break p;
                }
                idle(table, smp, priv_table);            // proc.c:338-340
            };
            if let Some(local) = smp.cpu_local_mut(bsp) {
                local.proc_ptr = Some(picked);           // proc.c:343
            }
            switch_address_space(table, picked);         // proc.c:349
        }
        let picked = current.expect("scheduler loop: proc_ptr seeded or picked above");

        if !table.process_misc_flags(picked, &crate::ipc::KernelUserCopy, priv_table) {
            continue;                                    // proc.c:413-414
        }
        if !table.check_quantum(picked) {
            continue;                                    // proc.c:427-428
        }
        finish_and_restore(table, smp, picked);          // 永不返回
    }
}
```

阶段函数与 C 语义的对应：

| 阶段函数 | C 对应 | 关键语义 |
|----------|--------|---------|
| `pick_and_bill`（lib.rs:2346） | proc.c:1785-1813 | 选队头进程；BILLABLE 则更新 `bill_ptr`（经 `kpriv::is_billable`，kpriv.rs:180） |
| `requeue_if_preempted`（lib.rs:2392） | proc.c:322-330 | 用裸 `p_rts_flags.clear` 而非 `rts_unset`——后者的自动入队只到队尾，无法表达"有量子进队头、无量子进队尾"的区分（调度语义见 §2.1） |
| `switch_address_space`（lib.rs:2558） | klib.S:605-626 | 三情形：根为 0 不动；根等于镜像值不动（**且不更新 ptproc**——对齐 C 的 `je 0f` 同时跳过两件事）；否则 `set_active_root_tracked` + 更新 ptproc 镜像 |
| `process_misc_flags`（proc_table.rs:764） | proc.c:351-415 | if-else 链保持 5 标志优先级；返回 bool 表达 C 的 `goto not_runnable_pick_new` |
| `check_quantum`（proc_table.rs:939） | proc.c:421-428 | 时间片耗尽走 `sched_proc_no_time`（proc_table.rs:592，对应 `proc_no_time` 的两支策略）；随后复查可运行性 |
| `finish_and_restore`（lib.rs:2624） | proc.c:437-474 | 见 §4.2 |

### 4.2 终局分派：finish_and_restore

```rust
fn finish_and_restore(
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    picked: crate::proc::ProcNr,
) -> ! {
    // debug_assert(quantum > 0) —— C:438 的 assert
    // context_stop(KERNEL)：内核执行时长记账 + TSC 基线推进
    let _exhausted = crate::clock::decrement_quantum_in(smp, kernel, tsc);
    crate::smp::bkl_unlock();                          // C: arch_clock.c:226-233 的释放点

    // FPU 所有权（C:443-446）
    if fpu_owner != Some(picked) { fpu.disable(); } else { fpu.enable(); }

    // 清除 MF_CONTEXT_SET（C:451）
    // restart_local_timer —— 当前时钟源自动重装，空操作（见 §4.4）

    // 从 cpu_context 重建 trap frame，然后离开
    let mut frame = <CurrentCpuContextArch as CpuContextArch>::TrapFrame::default();
    <CurrentCpuContextArch as CpuContextArch>::apply_to_trap_frame(&ctx, &mut frame);
    unsafe { CurrentTrapReturnArch::restore_to_user(&frame, &ctx) }   // 永不返回
}
```

（节选自 lib.rs:2624-2704，完整实现含每个步骤的 C 行号引用。）

`decrement_quantum_in`（clock.rs:439，`pub(crate)`）是 C `context_stop` 的 Rust 对应物中"量子扣减"的部分：它推进 per-CPU 的 TSC 基线、对 `p_endpoint >= 0` 的进程扣减量子；对 KERNEL/IDLE 这类端点为负的伪进程只推进基线不扣量子——与 arch_clock.c:314 的豁免逻辑一致。函数接受注入的 `&mut SmpState` 而不读全局，因此调用方可以传入自己已借用的状态而不产生两个可变别名。

### 4.3 idle

**位置**: [lib.rs:2453](../../os/kernel/src/lib.rs)（对应 proc.c:175-229）

实现顺序与 C 逐步对应：登记 IDLE 为当前进程并（因 IDLE 的特权带 BILLABLE）更新 `bill_ptr` → 置 `cpu_is_idle = 1` → `restart_local_timer()` → `decrement_quantum_in(KERNEL)` 记账 → **释放 BKL** → `CurrentSmpArch::idle_halt()` → 返回后**重新获取 BKL**。

`idle_halt` 是本次新增的 trait 方法（arch/smp.rs:84，x86-64 实现在 x86_64/smp.rs:194）：x86-64 为 `sti; hlt`，ARM64 为 `msr daifclr, #2` + `wfi`，RISC-V 为 `csrs sstatus, SIE` + `wfi`。它与既有 `SmpArch::halt_cpu`（IPI 停机路径用的裸 `hlt`/`wfi`）刻意分开——两者的中断使能语义不同，详见 §3 决策表最后一行。

单 CPU 构建下省略的部分与 C 的单 CPU 构建省略的部分一一对应：`switch_address_space_idle()` 只在 CONFIG_SMP 下有内容（proc.c:160-170）；AP 停定时器分支同理；`sprofiling` 轮询变体（proc.c:211-229）默认构建不编译。

### 4.4 restart_local_timer 为何是空操作

C 的 `restart_local_timer()`（arch_clock.c:168-175）在**没有 LAPIC**时是空操作（`if (lapic_addr)` 包裹）。单 CPU 的 Rust 构建用周期性时钟源（x86-64 的 PIT、ARM64 的通用定时器比较器、RISC-V 的 CLINT 比较器），这些硬件自动重装、无需软件重新武装——所以 Rust 版本（lib.rs:2526）的精确对等物就是空操作。当 LAPIC 一次性定时器成为时钟源时（bsp_finish_booting Step 6 已记录该延迟），这个函数就是重新武装的钩子位。

### 4.5 与 C 的差异总表

| # | C 行为 | Rust 行为 | 差异类型 | 依据 |
|---|--------|----------|---------|------|
| 1 | `NOT_REACHABLE` 注释（proc.c:473） | `-> !` 返回类型 | 类型演进 | D10-1 |
| 2 | BKL 在 `context_stop(KERNEL)` 内释放（arch_clock.c:226-233） | 在 `finish_and_restore` 记账后 / `idle` 停机前显式释放——同一语义位置 | 结构对齐（[ARCH] 标注） | §3.1 |
| 3 | `p_reg` 既是初值也是保存值 | `cpu_context` 同构；每次分派重建 trap frame | 结构对齐 | §3.2 |
| 4 | `get_cpulocal_var(proc_ptr)` | `CpuLocal.proc_ptr: Option<ProcNr>`（smp.rs:139） | 类型安全 | smp.rs:202 |
| 5 | 读硬件 CR3 判断等值（klib.S:618） | 读 `CURRENT_ROOT_PHYS` 软件镜像（lib.rs:2265） | 机制等价（镜像由 lib.rs:2324 同步） | §3 决策表 |
| 6 | `while` + `goto` 控制流 | 外层 `loop` + `continue` | 控制流演进（语义等价） | lib.rs:2776（循环）；continue 位于 lib.rs:2820/2829 |
| 7 | `restore_user_context` 汇编（mpx.S） | `TrapReturnArch::restore_to_user`（三架构实现，trap_return.rs） | 抽象演进（trait 下沉 arch 层） | §3 决策表 |
| 8 | `context_stop` 记 `kernel_ticks[cpu]` / `p_cycles`（arch_clock.c:231-232） | `decrement_quantum_in` 只推进基线 + 扣量子，per-CPU 内核时长统计未接线 | **已知缺口** | `idle` 实现内 TODO(P2) 注释；随 15 号文档的记账路径落地 |
| 9 | `kernel_call_resume(p)` 在杂项循环内完成完整重新分发（system.c:612-638） | 循环内调用简单版（读 VM 结果 + 清标志，vm.rs:896）；完整重分发被借用模型阻塞（`process_misc_flags` 持有 `&mut self`，无法同时把 `self` 作为 `proc_table` 传给 `syscall::kernel_call_resume`，syscall.rs:2662） | **已知缺口**（FIX-21 设计偏差，杂项标志接口阶段已记录） | proc_table.rs:788-792 注释 |
| 10 | `arch_do_syscall` 读 `p_defer` 后完整重执行系统调用 | `ProcessTable::arch_do_syscall`（proc_table.rs:892）已完成标志清除 + IPC 重分发；SEND/SENDREC 的消息体重读（p_defer.r3 用户指针）依赖系统调用追踪路径 | **已知缺口**（同上，依赖后续阶段） | proc_table.rs:887-893 注释 |
| 11 | `SC_TRACE`/`SC_ACTIVE` → `cause_sig(SIGTRAP)` | `process_misc_flags` 内清标志但 `cause_sig` 未接线（依赖信号模块） | **已知缺口** | proc_table.rs:842 注释 |
| 12 | `arch_finish_switch_to_user` 的"内核栈顶存进程指针"（arch_system.c:507） | 无对应——Rust 的恢复路径以值传递 frame/寄存器，不依赖栈布局 | 结构演进 | trap_return.rs 模块文档 |
| 13 | IF_MASK 或入 PSW（arch_system.c:512） | 下沉到 `TrapReturnArch` 契约：x86-64 实现对 RFLAGS 或 IF_MASK（x86_64/trap_return.rs:37-38），ARM64 清 SPSR 屏蔽位，RISC-V 置 SPIE | 位置迁移（语义保持：恢复出的用户上下文开中断） | trap_return.rs 契约第 2 条 |
| 14 | `sprofiling` 轮询变体（proc.c:211-229） | 未实现（统计剖析子系统延后） | 范围声明 | §4.3 |
| 15 | idle 后中断返回处的实时统计（proc.c:221-222 注释） | 同 C——结束统计由下一次 `context_stop` 统一结算 | 对齐（无差异） | lib.rs:2508 |

**差异 8-11 是有意保留的诚实缺口**：它们依赖的子系统（记账、系统调用追踪、信号模块、syscall 完整重分发的借用重构）属于后续文档的范围；每处都在代码注释中带 file:line 的 TODO 指向，不静默。

### 4.6 BKL 与中断的协作全景

调度循环把 BKL 的生命周期与"内核态"严格绑定：进入内核（trap 入口 / `kernel_call_dispatch` / `dispatch_ipc_entry`）获取，离开内核（终局分派、idle 停机）释放。图示：

```
            BKL 状态
              │
用户态 ──trap──→ [获取] ──→ 内核工作 ──→ finish_and_restore ──→ [释放] ──→ 用户态
   ↑                                                                    │
   └────────────────────────────────────────────────────────────────────┘
              │
就绪队列空 ──→ [获取]（循环已持有）──→ idle 记账 ──→ [释放] ──→ idle_halt 停机
   ↑                                                  │
   └────── 中断唤醒：trap 入口重新 [获取] ←────────────┘
```

这与 C 的行为逐点一致：C 的中断入口 `BKL_LOCK()`（mpx.S），C 的 `context_stop(KERNEL)` 解锁（arch_clock.c:226-233）。

---

## 5. 测试

### 5.1 单元测试

调度循环本身发散（`-> !`），无法"调用后断言"；测试策略是**分层注入**：各阶段函数接受注入的 `ProcessTable`/`SmpState`/`PrivTable`，可独立驱动并断言；主循环用一个端到端测试驱动到 mock 恢复点——mock 的 panic（"MockTrapReturn::restore_to_user"）就是"五阶段全部走通"的成功信号（mock 的设计意图见 arch/trap_return.rs:113-141）。

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_requeue_preempted_with_quantum_reenters_queue_head` | [lib.rs:3492](../../os/kernel/src/lib.rs) | PREEMPTED + 有量子 → 队头重入（C: proc.c:324-327） |
| `test_requeue_preempted_without_quantum_reenters_queue_tail` | [lib.rs:3515](../../os/kernel/src/lib.rs) | PREEMPTED + 无量子 → 队尾重入（C: proc.c:327-328） |
| `test_requeue_preempted_unrunnable_not_enqueued` | [lib.rs:3536](../../os/kernel/src/lib.rs) | 解除 PREEMPTED 后仍不可运行 → 不入队（C: proc.c:324 守卫） |
| `test_requeue_not_preempted_is_noop` | [lib.rs:3555](../../os/kernel/src/lib.rs) | 无 PREEMPTED → 整个重入队块跳过（C: proc.c:322 守卫） |
| `test_pick_and_bill_sets_bill_ptr_for_billable_process` | [lib.rs:3572](../../os/kernel/src/lib.rs) | BILLABLE 进程被选中 → `bill_ptr` 更新（C: proc.c:1808-1809） |
| `test_pick_and_bill_empty_queues_returns_none` | [lib.rs:3588](../../os/kernel/src/lib.rs) | 队列全空 → `None`（调用方进入 idle，C: proc.c:338） |
| `test_switch_address_space_kernel_task_is_noop` | [lib.rs:3606](../../os/kernel/src/lib.rs) | 内核任务（根为 0）→ 根与 ptproc 均不动（C: klib.S:610-612） |
| `test_switch_address_space_installs_root_and_tracks_ptproc` | [lib.rs:3621](../../os/kernel/src/lib.rs) | 正常切换 → 镜像更新 + ptproc 登记（C: klib.S:621-624） |
| `test_switch_address_space_same_root_skips_ptproc_update` | [lib.rs:3638](../../os/kernel/src/lib.rs) | 等根切换 → 根不动 **且 ptproc 不更新**（C 的 `je 0f` 同时跳过两件事，klib.S:618-620） |
| `test_idle_marks_cpu_idle_and_bills_idle_proc` | [lib.rs:3667](../../os/kernel/src/lib.rs) | idle 登记 IDLE 为当前进程 + 置 `cpu_is_idle`（C: proc.c:185-187, 192） |
| `test_finish_and_restore_reaches_mock_restore` | [lib.rs:3695](../../os/kernel/src/lib.rs) | 终局分派走到 mock 恢复点（记账/FPU/清标志/重建 frame 全部完成） |
| `test_switch_to_user_full_loop_dispatches_first_runnable_process` | [lib.rs:3714](../../os/kernel/src/lib.rs) | 端到端：IDLE 种子 → 重选 → 装地址空间 → 杂项 → 量子 → 分派发散 |
| `test_process_misc_flags_*`（5 个） | proc_table.rs:1455-1543 | 杂项标志各分支（FIX-20/21 阶段已有，保持不变） |

mock 恢复点的 panic 消息包含 frame 值（arch/trap_return.rs:134-141），意外触发时可直接诊断到"哪个进程被分派、寄存器是什么"——这在裸机上只表现为挂死的行为，在测试里成为可读的失败信息。

### 5.2 回归

mock 全套单元测试 573 项通过（2026-09-04，`cargo test -p minix-kernel --features mock`）；`cargo clippy -p minix-arch --features mock` 与 `-p minix-kernel --features mock` 对本次新增代码零告警。

---

## 6. 参见

- [08-system-init-boot-finish](08-system-init-boot-finish.md) — bsp_finish_booting 调用 switch_to_user（D7 设计决策来源）
- [09-vm-boot-protocol](09-vm-boot-protocol.md) — switch_address_space 与 VMCTL_SETADDRSPACE 共用 `TlbArch::set_active_root` + ptproc 镜像
- [11-scheduling-primitives](11-scheduling-primitives.md) — enqueue/dequeue/pick_proc/proc_no_time 的完整语义
- [12-ipc-core](12-ipc-core.md) — delivermsg 和 IPC 投递（杂项标志阶段的执行体）
- [13-syscall-dispatch](13-syscall-dispatch.md) — kernel_call_dispatch/finish 的 BKL 生命周期（与循环的协作见 §4.6）
- [14-exception-interrupt](14-exception-interrupt.md) — 三条激活路径的入口侧
- [15-clock-timer](15-clock-timer.md) — context_stop 记账与 quantum 扣减的完整模型（差异 8 的落地处）
- [31-fpu-context-switching](31-fpu-context-switching.md) — FPU 所有权与惰式切换
