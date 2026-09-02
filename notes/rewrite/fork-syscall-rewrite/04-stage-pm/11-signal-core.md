# 11 — 信号生成与分发核心：`do_kill`→`check_sig`→`sig_proc`→`sig_proc_exit`

本文讲清信号如何从 `kill(2)` 的 `pid` 四态选择（`>0`/`0`/`-1`/`<-1`）出发，经 `check_sig` 的全表逆序扫描与权限门（`SUPER_USER`/`real/eff` 四重、`INIT` 保护、`RS` 先杀、`VM` 跳过、`SIGS_IS_LETHAL` 的 `PRIV_PROC` 保护），由 `sig_proc` 的 9 判定链（`TRACE` 先行→`VFS|EVENT` 挂起→`PRIV_PROC` 消息化→`ignore`/`block`/`TRACE_STOPPED`→`caught`→`unpause`→`sig_send`→`terminate`）唯一投递，最终经 `sig_proc_exit` 的 `core_sset` 分支与 `process_ksig` 的 `EDEADEPT` 双检与 `SIGSNDELAY` 恢复，完成“生成→选择→投递→终止”的核心闭环。

前置阅读：03-mproc-table.md（`ProcTable` 扫描与 `pm_isokendpt`）、04-ipc-dispatch.md（`ReplyIntent` 与 `SUSPEND` 的自杀子类）、02-mproc-struct.md（`SignalState` 四位图 `ignored/caught/mask/pending` + `Lifecycle`）、09-pm-exit.md（`sig_proc_exit` 的 `exit_proc` 调用点）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `fork`/`exit`/`wait` 的生命周期与 `ProcTable` 三层身份、`WAITING` 位的 `WaitState`、`Lifecycle` 互斥的开发者。

> **本章不讲什么**：
> - `sigaction` 族安装语义（12-signal-handlers.md，`do_sigaction` 的 `SIG_IGN/DFL/CATCH` 三态、`sa_mask`/`sa_flags`）
> - 停止/延迟/恢复 `PROC_STOPPED`/`DELAY_CALL`/`UNPAUSED`（13-signal-flow.md，`stop_proc`/`try_resume_proc`/`unpause`/`restart_sigs`/`check_pending`）
> - `sys_kill`/`sys_sigsend` 内核侧（01-stage-kernel/19-syscall-signal.md，`kernel/system/do_kill.c`）
>
> 本章只回答一个问题：**为什么信号从 `kill` 的 `pid` 四态出发，必须经 `check_sig` 的逆序扫描与权限门，再由 `sig_proc` 的 9 判定链唯一投递，且 `process_ksig` 的 `EDEADEPT` 双检与 `SIGS_IS_LETHAL` 保护如何使“内核信号 vs 用户信号”可区分**。

### 1.1 为什么需要信号：异步事件的进程间通知

信号是**异步事件**的进程间通知（`kill` 的 `pid` 选择 + `signal` 的 `ignore/block/catch/terminate` 四处置），与 `exit` 的同步 `VFS_CALL` 解绑不同，信号可在任意时刻由 `kill(2)`（用户）、键盘 `SIGINT`、时钟 `SIGALRM`（内核 `sys_kill`）产生，`PM` 作为唯一信号权威（`process_ksig` 来自内核 `sys_kill`/`sys_getksig` 的 `SIGS_SIGNAL_RECEIVED`，`do_kill` 来自用户 `kill(2)`，`RS` 经 `do_srv_kill` 代内核杀系统进程），需在 `check_sig` 中统一选择目标集，再由 `sig_proc` 唯一投递。

### 1.2 为什么 `check_sig` 有四种 `pid` 语义：`>0`/`0`/`-1`/`<-1`

`kill(2)` 的 POSIX 四态（`signal.h:2` `kill(pid, sig)` 语义）在 `signal.c:601-604` 逐行过滤：

- `pid > 0`：精确 `pid == rmp_pid`（`601`）；
- `pid == 0`：同组（`602` `mp_procgrp != rmp_procgrp → continue`，`0` 的归一化在 `check_sig` 入口由 `mp_procgrp` 判定，与 `wait4` 的 `pidarg==0 → -procgrp` 同型但信号侧不归一为负，直接比较组）；
- `pid == -1`：全系统（除 `INIT_PID ≤1` 的 `INIT`/`idle`，`603` `pid <= INIT_PID → continue`）；
- `pid < -1`：指定组 `-pid`（`604` `procgrp != -pid → continue`）。

`proc_id==-1 && SIGTERM` 先杀 `RS`（`588-589` `sys_kill(RS, SIGTERM)` 先行，使 `RS` 有机会清理服务，再逆序扫全表）与 `wait4` 的 `pidarg` 四态同源但信号侧多 `RS` 先杀的时序（`forkexit.c:588-589` 广播 `SIGTERM` 时 `RS` 先行）。

### 1.3 为什么系统进程受保护：`PRIV_PROC` 的三重

`PRIV_PROC` 为内核任务/驱动（`VFS`/`VM`/`INET`，`mproc.h:98` `0x02000`），用户态 `kill` 的致命信号若可任意杀系统服务则微内核可用性崩，三重保护：

- `SIGS_IS_LETHAL` 致命信号（`SIGKILL/TERM` 等，`sys/sigtype.h`）仅 `ksig==TRUE`（来自内核/RS）可杀 `PRIV_PROC`（`616-618` `!ksig && is_lethal && PRIV_PROC → EPERM`）；
- 广播 `SIGKILL` 时跳过 `PRIV_PROC`（`607-608` `proc_id==-1 && SIGKILL && PRIV_PROC → continue`）；
- `VM_PROC_NR` 恒跳过（`613` `VM_PROC_NR` 为 `com.h:61`，VM 与信号管理器页错误死锁，`610-613` 注释）。

`ksig==TRUE` 的 `do_srv_kill`（`RS` 代内核清理）与 `process_ksig`（内核 `SIGKILL`）为唯二可杀 `PRIV_PROC` 的路径，`do_kill` 的 `ksig==FALSE` 不可。

### 1.4 为什么 `sig_proc` 有 9 判定链：优先级显式化

`signal.c:411-539` 的 9 判定使"调试→挂起→系统→忽略→阻塞→追踪停止→捕获→默认忽略→终止"的优先级显式：

1. `TRACE` 先行（`411-422` `tracer!=NO_TRACER && signo!=SIGKILL → sigaddset(sigtrace) + TRACE_STOPPED→trace_stop`，调试器优先于 `block/ignore`）；
2. `VFS|EVENT` 挂起（`425-444` `sigaddset(pending/ksigpending)` + `!(PROC_STOPPED|DELAY_CALL) → stop_proc(FALSE)`，`VFS|EVENT` 时未决信号待 `VFS` 回复，`PROC_STOPPED` 兼作 `restart_sigs` 的重检标志）；
3. `PRIV_PROC` 系统信号（`448-480` `PM_PROC_NR` 跳过 + `!ksig → sys_kill` 转内核 + `SIGS_IS_STACKTRACE → sys_diagctl_stacktrace` + `!SIGS_IS_TERMINATION → asynsend(SIGS_SIGNAL_RECEIVED)` 消息化 vs `sig_proc_exit`）；
4. `badignore`（`483-485` `ksig && noign && (ignore||mask)`，`noign_sset` 为 `SIGILL/TRAP/...` 不可忽略，`main.c:140-141`）；
5. `ignore`（`487-489` `ignore → return`）；
6. `block`（`491-496` `mask → pending`）；
7. `TRACE_STOPPED`（`499-507` `TRACE_STOPPED && signo!=SIGKILL → pending`，`trace_stop` 的 `TRACE_STOPPED` 位使调试器混淆前不投递）；
8. `caught`（`509-531` `!badignore && is_caught → !unpause→pending else sig_send → kill`）；
9. `ign_sset` 默认忽略（`533-535` `ign_sset` 为 `SIGCHLD/WINCH/CONT/INFO`，`main.c:139`）；
10. `sig_proc_exit` 终止（`539`）。

`badignore` 的 `noign_sset` 覆盖（`main.c:140`）使 `SIGKILL` 9 的 `SIG_IGN` 安装在 `do_sigaction` 已 `return OK`（`signal.c:49`），但 `sig_proc` 仍可杀（`badignore` 使忽略失效）。

### 1.5 为什么 `process_ksig` 需 `EDEADEPT` 双检：`endpoint` 的 `EXITING` 窗口

`process_ksig`（`294-378`，内核 `sys_kill`/`sys_getksig` 的 `SIGS_SIGNAL_RECEIVED` 回调）的 `pm_isokendpt → EDEADEPT`（`300-303`）+ `IN_USE|EXITING != IN_USE → EDEADEPT`（`305-309`）双重，使 `endpoint` 在信号投递前已 `EXITING` 或 `IN_USE` 失效时 `EDEADEPT`（`process is gone`），`mp = mproc[0]` 伪装 `PM` 为信号源（`312` `mp_procgrp = 0` 恢复，`334-335`）与 `SIGVTALRM` 的 `check_vtimer` 重启（`326-328`，`alarm.c:344`）。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `for (rmp=NR_PROCS-1; rmp>=0; rmp--)`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `kill`/`tkill`/`tgkill`。** Linux `kill` 的 `pid` 四态（`>0` 精确/`0` 同组/`-1` 全系统/`<-1` 指定组）+ `permission`（`CAP_KILL`/`same uid` 的 `real/eff` 四重）+ `signal_struct` 的 `shared_pending`/`blocked`/`ignored` 与 Minix3 的 `pid` 四态 + `SUPER_USER`/`real/eff` 四重 + `ignored/caught/mask/pending` 同源，但 Linux 以 `task_struct->children` 链表 + `signal_struct` 共享，Minix3 以 `mproc[NR_PROCS]` 全表逆序扫描 + `PRIV_PROC` 保护，Rust 侧 `SignalTarget` 枚举 + `Credentials::can_signal` + `SignalState` 四位图使 `pid` 过滤与 `permission` 显式化。

Linux 的 `kill` 还区分 `tgkill`（线程组）与 `tkill`（单线程），而 Minix3 无线程（`NR_PROCS` 进程即线程），`sig_proc` 的 `TRACE` 先行与 Linux 的 `ptrace` 信号拦截同源（`PTRACE_O_TRACESYSGOOD`），但 Minix3 以 `sigtrace` 位图 + `TRACE_STOPPED` 显式化，Linux 以 `task->ptrace` 标志。

**Redox `SigQueue`。** Redox 以 `SigQueue` + `Scheme` 信号分发，Minix3 以 `pending/ksigpending` 双位图（`mproc.h:57-58`）+ `SIGS_IS_LETHAL` 保护，`ksig` 的 `kernel_pending` 位使内核信号与用户信号可区分（`add_pending(signo, true)`）。Redox 的 `SigQueue` 为 per-process 队列（`Vec<Signal>`），Minix3 为位图（`u64`），位图使 `sigismember` 为常数时间（`1<<(n-1)`），队列使信号可排队（`sigqueue` 的 `siginfo`），Minix3 选择位图以保 `PM` 单线程无堆分配（`#![no_std]`）。

**`seL4` 无信号。** `seL4` 无信号原语，以 `Notification` 替代，Minix3 的 `sig_proc` 9 链 + `VFS|EVENT` 挂起 + `PROC_STOPPED` 兼作 `restart_sigs` 重检标志，与 `seL4` 的显式 `Notification` 同为异步事件，但 Minix3 以 `PM` 集中分发。`seL4` 的 `Notification` 为显式 capability（`seL4_Signal`），Minix3 的 `sig_proc` 为隐式 `pending` 位图 + `PROC_STOPPED` 标志，Rust 侧 `SignalState` 使隐式位图显式化（`SigSet` + `suspended`）。

**FreeBSD `kqueue` 信号过滤。** FreeBSD 以 `kqueue` + `EVFILT_SIGNAL` 使信号可经 `kevent` 轮询，Minix3 以 `sig_proc` 的 `VFS|EVENT` 挂起 + `PROC_STOPPED` 使信号在 `VFS` 回复后重检（`restart_sigs`），二者同为"信号在 `VFS` 期间挂起"，FreeBSD 以 `kqueue` 显式注册，Minix3 以 `PROC_STOPPED` 隐式重检。

**结论（本章的设计基线）。** 把 `check_sig` 的 `pid` 四态逆序扫描 + 权限门 + `SIGS_IS_LETHAL` 保护、`sig_proc` 的 9 判定链、`process_ksig` 的 `EDEADEPT` 双检、`sig_proc_exit` 的 `core_sset` 分支，改写为"类型化 `SignalTarget` 枚举 + 显式 `SignalOutcome` + `SignalState` 四位图 + `SignalClass` 宏类型化 + `ProcTable::signal_targets` 迭代器"。

#### 1.6.1 信号的四处置与 `PM` 的权威

信号的四处置（`ignore`/`block`/`catch`/`terminate`）在 `sig_proc` 的 9 链中按优先级显式化：`ignore`（`487-489`）直接丢弃，`block`（`491-496`）入 `pending` 待 `sigprocmask` 解阻塞后 `check_pending` 重投，`catch`（`509-531`）经 `unpause`→`sig_send` 建立 `sigframe`，`terminate`（`539`）经 `sig_proc_exit` 进入 `exit_proc`。`PM` 作为唯一权威，需在 `sig_proc` 中统一处理 `VFS|EVENT` 挂起（`425-444`）与 `PRIV_PROC` 系统信号消息化（`448-480`），使 `VFS` 侧 `fproc` 的阻塞调用可被 `VFS_PM_UNPAUSE` 中断（`signal.c:719-770` `unpause` 经 `tell_vfs`）。

#### 1.6.2 `SIGS_IS_LETHAL` 与 `core_sset` 的正交

`SIGS_IS_LETHAL`（`sys/sigtype.h`，`SIGKILL/TERM` 等致命）与 `core_sset`（`main.c:137-138`，`SIGQUIT/ILL/...` 需 `core`）正交：前者决定 `PRIV_PROC` 保护（`616-618`），后者决定 `sig_proc_exit` 的 `dump_core` 真值（`552`）。`SIGKILL` 9 既是 `is_lethal` 又是 `core_sset` 外（`core_sset` 不含 `SIGKILL`），因此 `sig_proc_exit` 的 `SIGKILL` 不 `dump_core`（`558` `exit_proc(..., FALSE)`），`SIGQUIT` 3 既是 `core_sset` 又非 `is_lethal`（`SIGQUIT` 可被 `PRIV_PROC` 的 `!ksig` 转 `sys_kill`），二者正交使 `PRIV_PROC` 的 `SIGQUIT` 经 `sys_kill` 转内核而非 `sig_proc_exit`。

### 1.7 小结

1. **为什么四态**——`kill` 的 `pid` 四态 + `RS` 先杀 `SIGTERM` 时序，使 `check_sig` 的逆序扫描与 `SignalTarget` 枚举穷尽。
2. **为什么系统保护**——`PRIV_PROC` 的 `SIGS_IS_LETHAL` 仅 `ksig` 可杀，广播 `SIGKILL` 跳 `PRIV_PROC`，`VM` 恒跳过。
3. **为什么 9 链**——`TRACE`→`VFS|EVENT`→`PRIV_PROC`→`badignore`→`ignore`→`block`→`TRACE_STOPPED`→`caught`→`terminate` 的优先级显式化。
4. **为什么双检**——`process_ksig` 的 `pm_isokendpt` + `IN_USE|EXITING` 双重 `EDEADEPT`，`endpoint` 的 `EXITING` 窗口。
5. **为什么 `core_sset` 分支**——`sig_proc_exit` 的 `core_sset` 决定 `dump_core` 真值，`PRIV_PROC` 不 `dump`（`exit_proc` 双门）。

下一章逐行分析 C 的 `check_sig`/`sig_proc`/`process_ksig`/`sig_proc_exit`；第 3 章给出 Rust 的 `SignalTarget`/`SignalState`/`SignalClass`。

---

## 2 C 源码分析

### 2.1 `do_kill`/`do_srv_kill` 序言：`ksig` 真假

```c
int do_kill(void) { return check_sig(m_in.m_lc_pm_sig.pid, m_in.m_lc_pm_sig.nr, FALSE); } // 197-202 FALSE → ksig==FALSE，PRIV_PROC 不可杀
int do_srv_kill(void) { // 204-221
    if (mp->mp_endpoint != RS_PROC_NR) return EPERM; // 212-213 RS 门（com.h:62, Endpoint::RS）
    return check_sig(m_in.m_rs_pm_srv_kill.pid, m_in.m_rs_pm_srv_kill.nr, TRUE); // 219-220 TRUE → ksig==TRUE，PRIV_PROC 可杀
}
```

`m_lc_pm_sig` 为 `ipc.h:1812` `mess_lc_pm_sig { pid, nr }`（`pid` `pid_t` + `nr` `int`），`m_rs_pm_srv_kill` 为 `ipc.h:2566` `mess_rs_pm_srv_kill { pid, nr }`（`RS` 专用，`com.h:62`），`ksig` 真假决定 `SIGS_IS_LETHAL` 的 `PRIV_PROC` 保护（`616-618`）。

### 2.2 `check_sig` 首段：`INVAL`/`INIT` 保护/`RS` 先杀

```c
if (signo < 0 || signo >= _NSIG) return(EINVAL); // 582 _NSIG 32（signal.h:20），-1 探测语义在 632 跳过 sig_proc
if (proc_id == INIT_PID && signo == SIGKILL) return(EINVAL); // 585 INIT_PID 1 保护（const.h:9），INIT 不可 SIGKILL
if (proc_id == -1 && signo == SIGTERM) sys_kill(RS_PROC_NR, signo); // 588-589 全系统 SIGTERM 先杀 RS（RS 清理服务）
```

`signo` 越界 `EINVAL`（`sys/errno.h:22`），`INIT_PID + SIGKILL → EINVAL` 使 `INIT` 永活（`main.c:194` `INIT` 父为自身，`exit.c:336` `INIT` 死亡仅 `stacktrace`），`RS` 先杀时序在 `handle_kill` 首行显式 `if target==All && signo==SIGTERM { sig_proc(RS) }`（D7）。

### 2.3 `check_sig` 主扫描：逆序 `NR_PROCS-1..0` + `count` + `SUSPEND` 自杀

```c
count = 0; error_code = ESRCH; // 595-596 ESRCH 3（sys/errno.h:3，no such process）
for (rmp = &mproc[NR_PROCS-1]; rmp >= &mproc[0]; rmp--) { // 597 逆序（forkexit.c 的 pid 魔数使系统进程在末尾，先杀系统进程后杀用户，591-593 注释）
    if (!(rmp->mp_flags & IN_USE)) continue; // 598
    if (proc_id > 0 && proc_id != rmp->mp_pid) continue; // 601
    if (proc_id == 0 && mp->mp_procgrp != rmp->mp_procgrp) continue; // 602
    if (proc_id == -1 && rmp->mp_pid <= INIT_PID) continue; // 603 跳过 INIT/idle
    if (proc_id < -1 && rmp->mp_procgrp != -proc_id) continue; // 604
    if (proc_id == -1 && signo == SIGKILL && (rmp->mp_flags & PRIV_PROC)) continue; // 607-608 广播 SIGKILL 跳 PRIV_PROC
    if (rmp->mp_endpoint == VM_PROC_NR) continue; // 613 VM 恒跳过（610-613 注释：VM 与信号管理器页错误死锁）
    if (!ksig && SIGS_IS_LETHAL(signo) && (rmp->mp_flags & PRIV_PROC)) { error_code = EPERM; continue; } // 616-618
    if (mp->mp_effuid != SUPER_USER && mp->mp_realuid != rmp->mp_realuid && mp->mp_effuid != rmp->mp_realuid && mp->mp_realuid != rmp->mp_effuid && mp->mp_effuid != rmp->mp_effuid) { error_code = EPERM; continue; } // 622-628 四重
    count++; // 631
    if (signo == 0 || (rmp->mp_flags & EXITING)) continue; // 632 0 探测或已退出则计数但不 sig_proc
    sig_proc(rmp, signo, TRUE /*trace*/, ksig); // 638
    if (proc_id > 0) break; // 640 精确单播
}
if ((mp->mp_flags & (IN_USE | EXITING)) != IN_USE) return(SUSPEND); // 644 自杀 EXITING 置位时 SUSPEND（main.c:106 ReplyLater 的自杀子类）
return(count > 0 ? OK : error_code); // 645 0 → OK（SUSPEND 已返），否则 ESRCH/EPERM 末次
```

`count` 计数 + `error_code` 末次保留（`595-596` `ESRCH` 初始，若权限 `EPERM` 则覆盖末次为 `EPERM`，`645` `count>0 ? OK : error_code`），`SUSPEND` 当 `mp` 自杀（`IN_USE|EXITING != IN_USE`，`644`，`04` 的 `ReplyLater` 自杀子类，与 `fork` 的 `SUSPEND` 同源但语义为"自杀不回"）。

### 2.4 `sig_proc` 9 判定链（`384-540`）

```c
slot = (int)(rmp - mproc); // 406
if ((rmp->mp_flags & (IN_USE | EXITING)) != IN_USE) panic("PM: signal %d sent to exiting process %d\n", signo, slot); // 407-409
if (trace == TRUE && rmp->mp_tracer != NO_TRACER && signo != SIGKILL) { // 411-422 TRACE 先行
    sigaddset(&rmp->mp_sigtrace, signo); // 417
    if (!(rmp->mp_flags & TRACE_STOPPED)) trace_stop(rmp, signo); // 419-420 a signal causes it to stop（trace.c:256）
    return;
}
if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) { // 425-444 VFS|EVENT 挂起
    sigaddset(&rmp->mp_sigpending, signo); // 426
    if(ksig) sigaddset(&rmp->mp_ksigpending, signo); // 427-428
    if (!(rmp->mp_flags & (PROC_STOPPED | DELAY_CALL))) stop_proc(rmp, FALSE); // 437-442 stop_proc(FALSE) → PROC_STOPPED
    return;
}
if(rmp->mp_flags & PRIV_PROC) { // 448-480 PRIV_PROC 系统信号
    if(rmp->mp_endpoint == PM_PROC_NR) return; // 450-452 PM 自身跳过（广播时）
    if(!ksig) { sys_kill(rmp->mp_endpoint, signo); return; } // 458-461 !ksig → sys_kill 转内核
    if(SIGS_IS_STACKTRACE(signo)) sys_diagctl_stacktrace(rmp->mp_endpoint); // 464-466
    if(!SIGS_IS_TERMINATION(signo)) { // 468-474 非终止 → asynsend(SIGS_SIGNAL_RECEIVED)
        message m; m.m_type = SIGS_SIGNAL_RECEIVED; m.m_pm_lsys_sigs_signal.num = signo; asynsend3(rmp->mp_endpoint, &m, AMF_NOREPLY);
    } else { // 475-478 终止 → sig_proc_exit
        sig_proc_exit(rmp, signo);
    }
    return;
}
badignore = ksig && sigismember(&noign_sset, signo) && (sigismember(&rmp->mp_ignore, signo) || sigismember(&rmp->mp_sigmask, signo)); // 483-485 noign_sset 为 SIGILL/TRAP/...（main.c:140-141）
if (!badignore && sigismember(&rmp->mp_ignore, signo)) return; // 487-489 ignore → return
if (!badignore && sigismember(&rmp->mp_sigmask, signo)) { // 491-496 block → pending
    sigaddset(&rmp->mp_sigpending, signo); if(ksig) sigaddset(&rmp->mp_ksigpending, signo); return;
}
if ((rmp->mp_flags & TRACE_STOPPED) && signo != SIGKILL) { // 499-507 TRACE_STOPPED → pending
    sigaddset(&rmp->mp_sigpending, signo); if(ksig) sigaddset(&rmp->mp_ksigpending, signo); return;
}
if (!badignore && sigismember(&rmp->mp_catch, signo)) { // 509-531 caught
    if (!unpause(rmp)) { // 514-520 !unpause → pending
        sigaddset(&rmp->mp_sigpending, signo); if(ksig) sigaddset(&rmp->mp_ksigpending, signo); return;
    }
    if (sig_send(rmp, signo)) return; // 526-527 sig_send 成功 → return
    printf("PM: %d can't catch signal %d - killing\n", rmp->mp_pid, signo); // 530-531 失败则杀
} else if (!badignore && sigismember(&ign_sset, signo)) return; // 533-535 默认忽略（ign_sset 为 SIGCHLD/WINCH/CONT/INFO，main.c:139）
sig_proc_exit(rmp, signo); // 539 终止
```

`badignore` 的 `noign_sset` 覆盖（`483-485`，`SIGKILL 9` 的 `SIG_IGN` 安装在 `do_sigaction` 已 `return OK`（`49`），但 `sig_proc` 仍可杀）与 `VFS|EVENT` 挂起的 `stop_proc`（`425-444`）为 13 的 `restart_sigs` 前置。

### 2.5 `sig_proc_exit`（`546-563`）

```c
rmp->mp_sigstatus = (char) signo; // 551 char 截断（mproc.h:26）
if (sigismember(&core_sset, signo)) { // 552 core_sset 为 SIGQUIT/ILL/TRAP/...（main.c:137-138）
    if(!(rmp->mp_flags & PRIV_PROC)) { printf("PM: coredump signal %d for %d / %s\n", signo, rmp->mp_pid, rmp->mp_name); sys_diagctl_stacktrace(rmp->mp_endpoint); } // 553-557 PRIV_PROC 不 dump
    exit_proc(rmp, 0, TRUE /*dump_core*/); // 558 dump_core 真（forkexit.c:350 VFS_PM_DUMPCORE）
} else {
    exit_proc(rmp, 0, FALSE /*dump_core*/); // 561
}
```

`mp_sigstatus` 的 `char` 截断与 `mproc/lifecycle.rs:27` `sig_status: i8` 对应，`core_sset` 决定 `dump_core` 真值（`exit.rs:285-292` 双门）。

### 2.6 `process_ksig`（`294-378`）

```c
if(pm_isokendpt(proc_nr_e, &proc_nr) != OK) { printf("PM: process_ksig: %d?? not ok\n", proc_nr_e); return EDEADEPT; } // 300-303
rmp = &mproc[proc_nr];
if ((rmp->mp_flags & (IN_USE | EXITING)) != IN_USE) return EDEADEPT; // 305-309
proc_id = rmp->mp_pid; // 311
mp = &mproc[0]; mp->mp_procgrp = rmp->mp_procgrp; // 312-313 伪装 PM 为信号源（A-3）
switch (signo) { // 320-332
    case SIGINT: case SIGQUIT: case SIGWINCH: case SIGINFO: id = 0; break; // 321-325 组广播
    case SIGVTALRM: case SIGPROF: check_vtimer(proc_nr, signo); /* fall-through */ // 326-328
    default: id = proc_id; break; // 330-332 精确
}
check_sig(id, signo, TRUE /* ksig */); // 334 TRUE 使 PRIV_PROC 可杀
mp->mp_procgrp = 0; // 335 恢复
if (signo == SIGSNDELAY && (rmp->mp_flags & DELAY_CALL)) { // 344-369 DELAY_CALL 恢复
    rmp->mp_flags &= ~DELAY_CALL; // 351
    if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) { stop_proc(rmp, FALSE); return OK; } // 359-362 VFS|EVENT → stop
    check_pending(rmp); // 366
}
if ((mproc[proc_nr].mp_flags & (IN_USE | EXITING)) == IN_USE) return OK; // 372-373 仍存活 → OK
else return EDEADEPT; // 376 process is gone
```

`EDEADEPT` 双重（`300` `pm_isokendpt` + `305` `IN_USE|EXITING`），`mp = mproc[0]` 伪装（`312`）与 `SIGVTALRM` 的 `check_vtimer` 重启（`326-328`，`alarm.c:344`）。

### 2.7 对端与消息格式（`sys_kill`/`SIGS_SIGNAL_RECEIVED`）

- **内核侧**：`sys_kill`/`sys_sigsend`/`sys_getksig`（`01-stage-kernel/19-syscall-signal.md`，`kernel/system/do_kill.c`）、`SIGS_SIGNAL_RECEIVED 0`（`com.h:597` `COMMON_RQ_BASE+0`）的系统进程消息化（`com.h:597` + `ipc.h:2566` `mess_pm_lsys_sigs_signal`）、`SIGS_IS_LETHAL/STACKTRACE/TERMINATION` 宏（`sys/sigtype.h`，`signal.c:464-476` 三分支）
- **信号集合**：`core_sset`（`SIGQUIT/ILL/TRAP/ABRT/EMT/FPE/BUS/SEGV`，`main.c:137-138`）、`ign_sset`（`SIGCHLD/WINCH/CONT/INFO`，`main.c:139`）、`noign_sset`（`SIGILL/TRAP/EMT/FPE/BUS/SEGV`，`main.c:140-141`）、`_NSIG 32`（`signal.h:20`）→ `SigSet` 64 位扩展（`A-11`）

### 2.8 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `INIT_PID + SIGKILL → EINVAL` | `signal.c:585` | `INIT` 永活 | 可恢复（`EINVAL`） |
| `RS → EPERM` | `signal.c:212-213` | 非 `RS` 的 `srv_kill` | 可恢复（`EPERM`） |
| `SIGS_IS_LETHAL` 的 `PRIV_PROC` 保护 | `signal.c:616-618` | `!ksig && lethal && PRIV_PROC → EPERM` | 可恢复 |
| `VM_PROC_NR` 跳过 | `signal.c:613` | `VM` 与信号管理器死锁 | 设计约束 |
| `VFS|EVENT` 挂起 | `signal.c:425-444` | `VFS|EVENT` 时 `pending` + `stop_proc` | 不变式（13 的 `restart_sigs` 重检） |
| `TRACE` 先行 | `signal.c:411-422` | `tracer != NO_TRACER && signo != SIGKILL` | 调试器优先 |
| `badignore` 的 `noign_sset` 覆盖 | `signal.c:483-485` | `ksig && noign && (ignore||mask)` | 致命不可忽略 |

---

## 3 Rust 设计决策

Rust 改写遵循"显式 `SignalTarget` 枚举 + `SignalState` 四位图 + `SignalClass` 类型化 + `ProcTable::signal_targets` 迭代器"的 8 决策，保留 C 的 `pid` 四态逆序扫描与 `sig_proc` 9 链，但用类型系统使 `SIGS_IS_LETHAL` 保护与 `VFS|EVENT` 挂起显式化。以下决策对应 `.design/11-design.v1.md` 的 D1–D8。

### D1：`SignalTarget` 枚举

`SignalTarget::{One(Pid), ProcessGroup(Pid), All, SystemAll}` + `from_pid` 归一化（`0 → ProcessGroup(caller_procgrp)`），`A-2` 位→枚举。

### D2：`Credentials::can_signal` 封装四重匹配

`SUPER_USER(0)` 或 `real/eff` 四重（`signal.c:622-628`），`A-11` `Uid` 64 位映射；`!ksig && is_lethal && PRIV_PROC → EPERM` 单独分支。

### D3：`SignalDisposition` 9 链枚举

`Traced`/`PendingVfs`/`SystemMessage`/`Terminate`/`Ignored`/`Blocked`/`TraceStopped`/`Caught`/`DefaultIgnore`，`A-2` 优先级显式化。

### D4：`SigSet = u64` 位图

`1<<(signo-1)`，`_NSIG 64`，`core/ign/noign` 三集合为 `SigSet` 常量（`init.rs:98-123` 的 `CORE_SIGSET` 等）。

### D5：`process_ksig` 双重过滤

`ProcTable::pm_isokendpt` + `Lifecycle::is_exiting` 双检（`A-3` 伪装 → `SignalContext` 显式传参）。

### D6：`sig_proc_exit` 的 `core_sset` 分支

`is_core_dump(signo)` + `exit_proc(dump_core)`，`dump_core` 双门（`realuid != effuid` 与 `PRIV_PROC`）。

### D7：`check_sig` 计数与 `SUSPEND`

`SignalOutcome { Sent(usize), NoPerm }` + `ReplyIntent`（`count>0 ? OK : error_code`，`SUSPEND` 当 `mp` 自杀）。

### D8：`SignalClass` 枚举

`is_lethal/is_stacktrace/is_termination` 三方法（`sys/sigtype.h`）。

---

## 4 实现详解

### 4.1 `os/servers/pm/src/signal.rs`

`handle_kill`/`check_sig` 逆序扫描 + `sig_proc` 9 链 + `sig_proc_exit` + `process_ksig` 双检 + `SIGVTALRM` 重启 + `SIGSNDELAY` 恢复。

### 4.2 `os/servers/pm/src/mproc/signal.rs`

`SignalState` 四位图 + `SigAction` + `CORE/IGN/NOIGN` 常量。

### 4.3 `os/servers/pm/src/mproc/table.rs`

`ProcTable::signal_targets` 迭代器。

### 4.4 `os/servers/pm/src/ipc/{dispatcher,init}.rs`

`PmCall::Kill=11`/`SrvKill=42` + `ReplyIntent`。

### 4.5 不变量表（表：C 锚点 → Rust 表达 → 检测 panic/Reply）

---

## 5 测试矩阵

### 5.1 `mproc/signal.rs`（位图操作）

- `test_signal_state_default`：`SignalState::default` → `!has_pending`
- `test_is_blocked/ignored/caught`：`SigSet` 位操作
- `test_add_pending`：`pending`/`kernel_pending` 双位图

### 5.2 `signal.rs`（`check_sig` 四态与 `sig_proc` 9 链）

- `test_kill_all` / `test_kill_eperm` 等

---

## 6 过渡

本篇在主循环 `PM_KILL`/`PM_SRV_KILL` 与 `process_ksig` 的 `SIGS_SIGNAL_RECEIVED` 之间的位置；`sig_proc` 的 `unpause`→`sig_send`→`sig_proc_exit` 为 13 的 `unpause`/`sig_send`/`restart_sigs` 前置。

## 7 参见

- C 源：`minix3/minix/servers/pm/signal.c:197-646`（`do_kill`/`check_sig`/`sig_proc` 等）
- 设计契约：`.design/11-design.v1.md`（D1–D8）、`.design/11-outline.v1.md` 等
- PM 阶段文档：03-mproc-table.md（`ProcTable`）、04-ipc-dispatch.md（`ReplyIntent`）、02-mproc-struct.md（`SignalState`）、09-pm-exit.md（`sig_proc_exit`）、13-signal-flow.md（`stop_proc` 等）
- 内核接口：01-stage-kernel/19-syscall-signal.md（`sys_kill` 等）
