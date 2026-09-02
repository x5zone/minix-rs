# 10 — 等待与回收：`do_wait4` 的三环与 `tell_parent` 的 `TOLD_PARENT`

本文讲清 `do_wait4` 如何在一次 `PM_WAIT4` 中完成"父进程对子进程死亡的等待与回收"：从 `pidarg` 的四态归一化（`>0`/`0→-procgrp`/`-1`/`<-1`）与 `WNOHANG`/`ECHILD`，到全表扫描的三条件过滤（`IN_USE|TOLD_PARENT`/`parent||tracer`/`ZOMBIE` 伪父）与三环（`TRACE_ZOMBIE→tell_tracer`/`TRACE_STOPPED→W_STOPCODE`/`ZOMBIE→tell_parent`），再到 `wait_test` 的 `WAITING∧right_child` 双条件与 `tell_parent` 的 `sys_datacopy(rusage)`+`W_EXITCODE`+`WAITING`清+`ZOMBIE→TOLD_PARENT`+`child_utime/stime` 累计，以及 `tracer` 伪父的 `TRACE_ZOMBIE→ZOMBIE` 转换与 `cleanup` 的 `procs_in_use--`（与 07 的 `++` 成对）。

前置阅读：03-mproc-table.md（`ProcTable`/`WaitState`/`Pid`/`procgrp`）、09-pm-exit.md（`zombify` 两级僵尸 `ZOMBIE`/`TRACE_ZOMBIE` 与 `cleanup` 的 `procs_in_use--`）、02-mproc-struct.md（`Lifecycle` 互斥枚举 + `Guardianship` 双监护）、06-event-subscription.md（`VFS|EVENT` 时不 `cleanup` 的 `try_cleanup` 守卫）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `exit` 的两阶段与僵尸生产（09）、`ProcTable` 三层身份与 `Lifecycle` 互斥、 `WAITING` 位的 `WaitState` 的开发者。

> **本章不讲什么**：
> - 僵尸产生 `zombify` 的 `TRACE_ZOMBIE` vs `ZOMBIE` 互斥（09-pm-exit.md §2.6，已在 09 落地）
> - `trace_stop` 的 `TRACE_STOPPED` 语义（18-trace.md，`trace.c:276` 的 `TRACE_STOPPED` 位）
> - `sig_proc(SIGCHLD)` 的发送（11-signal-core.md，`signal.c:384`，`check_parent` 的 `else → sig_proc` 分支）
> - `WNOHANG` 以外的 `options` 位（`WUNTRACED` 等归 18-trace.md，`forkexit.c:553` 仅 `WNOHANG`）
>
> 本章只回答一个问题：**为什么 `wait4` 必须在一次全表扫描中区分“追踪僵尸→追踪停止→普通僵尸”三环，且 `tell_parent` 的 `TOLD_PARENT` 与 `VFS|EVENT` 守卫如何使“已收割不再被等待”与“仍阻塞不回收”同时成立**。

### 1.1 为什么需要 wait：`exit` 仅生产，`wait` 才消费

`exit`（09）的 `zombify` 仅生产 `ZOMBIE`/`TRACE_ZOMBIE`（`forkexit.c:593-624`），`procs_in_use` 仍计数，`mp_pid` 仍保留，`W_EXITCODE` 与 `rusage` 无源若直接 `cleanup`。`wait4`（本章）经 `tell_parent` 消费：`sys_datacopy(rusage)` 跨地址空间拷贝 `ru_utime/ru_stime`（`utility.c:92` 仅两字段，其余 `TODO` 保留 `0`，`forkexit.c:694-708` 失败 `reply(errno)` + `return FALSE` 使 `try_cleanup` 清零，不 `cleanup`），`W_EXITCODE`（`wait.h:63` `W_EXITCODE` 宏，`exitstatus/sigstatus` 的 `char` 截断）填 `parent->mp_reply`，`reply(parent, pid)` 唤醒，`WAITING` 清，`ZOMBIE→TOLD_PARENT`（`forkexit.c:715-717`），`child_utime/stime` 累计至父（`722-723`），`TOLD_PARENT` 避免二次通知（`687-690` `TOLD_PARENT` 防重 `panic`）。

### 1.2 为什么 `pidarg` 有四态：`>0`/`0→-procgrp`/`-1`/`<-1`

`wait4` 的首参 `pid`（`m_in.m_lc_pm_wait4.pid`，`forkexit.c:490`）四态在 `493` 归一化：

```c
if (pidarg == 0) pidarg = -mp->mp_procgrp; // pidarg < 0 ==> proc grp（493 注释）
```

- `pidarg > 0`：等待 `pid == pidarg` 的精确子（`507`）；
- `pidarg == 0`：归一为 `-procgrp`，等待同组任一子（`508` `-pidarg == procgrp`）；
- `pidarg == -1`：等待任一子（`502-503` `parent==who_p` 任一）；
- `pidarg < -1`：等待组 `-pidarg` 任一子（`508`）。

`0` 归一化使会话/组语义与 `getset.c` 的 `mp_procgrp`（`mproc.h:30`，`pid == procgrp` 即会话 Leader，`A-13`）统一，`WaitTarget` 枚举（`mproc/wait.rs:30` `AnyChild/SpecificChild/Group`）使 `is_waiting_for` 的 `match` 穷尽且 `0` 归一化显式化。

### 1.3 为什么是 `SUSPEND` 而非同步 `reply`：`WAITING` 的异步唤醒

`wait4` 的满足条件可能未来才出现（子尚未退出），`do_wait4` 置 `WAITING` + `mp_wpid/mp_waddr` 后 `SUSPEND`（`556-559` `WAITING|mp_wpid|mp_waddr` 同生同灭，`mproc/wait.rs:12` `WaitState`），由 `zombify/check_parent` 的 `tell_parent` 异步 `reply(parent, pid)` 唤醒（`714`），与 `do_fork` 的 `SUSPEND`（07，`VFS_PM_FORK`）同源（04 `ReplyLater`）但唤醒源为 `exit` 而非 `VFS`。

`WNOHANG`（`wait.h:10` `WNOHANG 0x01`）与 `ECHILD`（`sys/errno.h:10` `ECHILD 10`）为同步返路径：`children>0` 且 `WNOHANG → 0`（`553-554` 父不等待），`children==0`（无 `acceptable child`，`502-508` 三条件过滤后 `children==0`）→ `ECHILD`（`560-562`）。

### 1.4 为什么需要 `tracer` 伪父：`TRACE_ZOMBIE` 与 `W_STOPCODE`

`ptrace` 使 `tracer` 成为"伪父"（`mp_tracer == who_p`，`mproc.h:34` `NO_TRACER 0`），`wait4` 的首环先扫描 `TRACE_ZOMBIE`/`TRACE_STOPPED`（`512-534`），`TRACE_ZOMBIE` 经 `tell_tracer` 转 `ZOMBIE` 后再对真父可见（`752-753` `TRACE_ZOMBIE→ZOMBIE`），`TRACE_STOPPED` 经 `W_STOPCODE(i)` 直接返 `pid`（`530-531` `wait.h:32` `W_STOPCODE` 宏，`sigismember(sigtrace, i)` 扫描 `1.._NSIG`，`W_STOPCODE` 的 `status` 为 `W_STOPCODE`，非 `W_EXITCODE`，`forkexit.c:530` 存 `mp_reply`）——`wait4` 因此先服务追踪僵尸/停止，再服务普通僵尸（`537-545`），与 `zombify` 的 `TRACE_ZOMBIE → check_parent` 串联（`623`）。

### 1.5 为什么 `rusage` 需 `sys_datacopy`：跨地址空间拷贝

`wait4` 的 `addr` 为用户虚地址（`m_lc_pm_wait4.addr`，`forkexit.c:492`），`PM` 需经 `sys_datacopy(SELF, &r_usage, parent_ep, addr)` 跨地址空间拷贝（`703-704` `kernel/system/do_datacopy.c`），失败 `reply(parent, errno)` + `return FALSE` 使 `try_cleanup` 清零，不 `cleanup`（`705-707`，`check_parent` 的 `try_cleanup && !(VFS|EVENT) → cleanup` 禁止在 `VFS|EVENT` 时回收，`658`），`set_rusage_times` 仅填 `ru_utime/ru_stime`（`utility.c:92`，其余 `ru_` 字段 `TODO` 保留 `0`，`forkexit.c:699` `memset(0)` 后 `set_rusage_times`）。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `for (rp=0..NR_PROCS)`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `wait4`/`waitpid`。** Linux `wait4` 的 `pid` 四态（`>0` 精确/`0` 同组/`-1` 任一/`<-1` 组）与 `WNOHANG/WUNTRACED/WCONTINUED` + `rusage` 累计 + `__WNOTHREAD` 与 Minix3 的 `pidarg` 四态 + `WNOHANG` + `ECHILD` 同源，但 Linux 以 `task_struct->children` 链表 + `exit_state` 枚举，Minix3 以 `mproc[NR_PROCS]` 全表扫描 + `ZOMBIE` 位，Rust 侧 `WaitTarget` 枚举 + `WaitState` + `Lifecycle` 使 `pidarg` 过滤与 `WAITING` 位显式化（`mproc/wait.rs:12` 同生同灭）。

**FreeBSD `wait6`。** FreeBSD `wait6` 的 `WRUSAGE` 与 `WTRAPPED` 与 Minix3 的 `set_rusage_times` 仅两字段 + `TRACE_STOPPED→W_STOPCODE` 同源，Minix3 的 `rusage` 仅 `ru_utime/ru_stime`（`utility.c:92` `TODO`）为简化。

**Redox `waitpid` 的 `Scheme` 阻塞。** Redox 以 `Scheme` 的 `FileDescription` 阻塞 + `waitpid` 的 `WAITING` 位，Minix3 以 `PM` 的 `WAITING` 位 + `mp_wpid/mp_waddr`，二者同为"父进程 `WAITING` 位 + 子 `ZOMBIE` 位"的双位协同，Rust 侧 `WaitState::is_waiting_for` 的 `WAITING && right_child` 双条件与 `forkexit.c:587` 同形。

**`seL4` 无 `wait`。** `seL4` 无 `wait` 原语，`TCB` 手动回收，Minix3 的僵尸链表（`ZOMBIE` 位 + `procs_in_use` 计数）与 `seL4` 手动回收同为显式解绑，`cleanup` 的 `procs_in_use--` 与 07 的 `++` 成对。

**结论（本章的设计基线）。** 把 `do_wait4` 的 `pidarg` 四态 + 全表扫描三条件 + 三环（`TRACE_ZOMBIE`→`TRACE_STOPPED`→`ZOMBIE`）+ `wait_test` 双条件 + `tell_parent` 的 `sys_datacopy`+`TOLD_PARENT` + `tracer` 伪父转换，改写为"类型化 `WaitTarget` 枚举 + 显式扫描器 + `WaitState` 同生同灭 + `Lifecycle` 互斥 + `ProcTable::release_slot` 显式回收"。

### 1.7 小结

1. **为什么 wait**——`exit` 生产 `ZOMBIE`/`TRACE_ZOMBIE`，`wait4` 经 `tell_parent/tell_tracer` 消费 + `cleanup` 回收 `procs_in_use`，`TOLD_PARENT` 防重。
2. **为什么四态**——`pidarg` 的 `>0`/`0→-procgrp`/`-1`/`<-1` 与 `WaitTarget` 枚举穷尽，`Group` 的负值编码使 `is_waiting_for` 的 `match` 显式化。
3. **为什么 `SUSPEND`**——`WAITING` 置位后 `SUSPEND`，由 `check_parent` 的 `tell_parent` 异步唤醒，与 `fork` 的 `SUSPEND` 同源但唤醒源为 `exit`。
4. **为什么 `tracer` 伪父**——`TRACE_ZOMBIE` 先通知 `tracer` 伪父，经 `tell_tracer` 转 `ZOMBIE` 再对真父可见；`TRACE_STOPPED` 经 `W_STOPCODE` 同步返。
5. **为什么 `rusage` 需 `sys_datacopy`**——用户虚地址跨地址空间拷贝，失败不 `cleanup`，`set_rusage_times` 仅两字段。

下一章逐行分析 C 的 `do_wait4`/`wait_test`/`tell_parent`/`tell_tracer`；第 3 章给出 Rust 的显式扫描器与状态机。

---

## 2 C 源码分析

### 2.1 `do_wait4` 序言：`pidarg` 四态归一化（forkexit.c:489-493）

```c
pidarg  = m_in.m_lc_pm_wait4.pid;         // 490 1st param
options = m_in.m_lc_pm_wait4.options;     // 491 3rd param
addr    = m_in.m_lc_pm_wait4.addr;        // 492 4th param（vir_bytes，06 的 VirBytes）
if (pidarg == 0) pidarg = -mp->mp_procgrp; // 493 0 → -procgrp（mproc.h:30 procgrp，会话/组归一）
```

`m_lc_pm_wait4` 为 `ipc.h:445` `mess_lc_pm_wait4 { pid, options, addr }`（`pid` `pid_t` + `options` `int` + `addr` `vir_bytes`），`pidarg==0` 的归一使 `wait(0)` 等同 `waitpid(-procgrp)`（等待同组任一子，`getset.c` 进程组语义，`A-13`）。

### 2.2 `do_wait4` 主扫描：三条件过滤与 `children` 计数（forkexit.c:500-510）

```c
children = 0;
for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++) { // 501 全表扫描 NR_PROCS 256
    if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue; // 502 IN_USE 且非 TOLD_PARENT（已收割不再被等待）
    if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;   // 503 非子非追踪（parent/tracer 双监护，mproc.h:33-34）
    if (rp->mp_parent != who_p && (rp->mp_flags & ZOMBIE)) continue;   // 504 僵尸但父非 who_p 的非追踪僵尸不计（tracer 伪父的 ZOMBIE 不归真父）
    if (pidarg  > 0 && pidarg != rp->mp_pid) continue;                // 507 pid 精确过滤
    if (pidarg < -1 && -pidarg != rp->mp_procgrp) continue;           // 508 组过滤（-pidarg == procgrp）
    children++;                       // 510 acceptable child
```

`502` 的 `IN_USE|TOLD_PARENT != IN_USE` 使 `TOLD_PARENT` 已收割不再被等待（`Lifecycle::ToldParent` 互斥）；`503` 的 `parent||tracer` 双监护与 `mproc/guardianship.rs` 的 `Guardianship` 对应（`A-12`）；`504` 的 `ZOMBIE` 但父非 `who_p` 的非追踪僵尸不计，使 `TRACE_ZOMBIE` 的 `ZOMBIE` 转 `true` 后才对真父可见（`752-753`）。

### 2.3 `do_wait4` 三环：`TRACE_ZOMBIE`→`TRACE_STOPPED`→`ZOMBIE`（forkexit.c:512-547）

```c
if (rp->mp_tracer == who_p) {         // 512 tracer == who_p 首环
    if (rp->mp_flags & TRACE_ZOMBIE) { // 513 追踪僵尸
        tell_tracer(rp);              // 515 732-754 TRACE_ZOMBIE→ZOMBIE + reply(tracer, pid) + WAITING清
        check_parent(rp, TRUE /*try_cleanup*/); // 516 true → try_cleanup && !(VFS|EVENT) → cleanup
        return(SUSPEND);              // 517 tell_parent 异步唤醒 tracer，不直接 reply（SUSPEND 后 tell_tracer 已 reply）
    }
    if (rp->mp_flags & TRACE_STOPPED) { // 519 追踪停止
        for (i = 1; i < _NSIG; i++) { // 523 扫描 _NSIG 32（signal.h:20）
            if (sigismember(&rp->mp_sigtrace, i)) { // 524 sigtrace 位图（mproc.h:59）
                sigdelset(&rp->mp_sigtrace, i); // 527 清位
                mp->mp_reply.m_pm_lc_wait4.status = W_STOPCODE(i); // 529-530 wait.h:32 W_STOPCODE 宏
                return(rp->mp_pid);   // 531 同步返 pid（非 SUSPEND，W_STOPCODE 直接 reply）
            }
        }
    }
}

if (rp->mp_parent == who_p) {         // 537 真父环
    if (rp->mp_flags & ZOMBIE) {      // 538 普通僵尸
        waited_for = tell_parent(rp, addr); // 540 670-726 sys_datacopy + W_EXITCODE + WAITING清 + ZOMBIE→TOLD_PARENT
        if (waited_for && !(rp->mp_flags & (VFS_CALL | EVENT_CALL))) // 542-543 VFS|EVENT 时不 cleanup（06 resume 待串行完成，try_cleanup 守卫）
            cleanup(rp);              // 544 release_slot → procs_in_use--
        return(SUSPEND);              // 545 tell_parent 已 reply(parent, pid)，SUSPEND 使 main.c:106 不二次 reply
    }
}
```

`TRACE_ZOMBIE` 优先于 `TRACE_STOPPED` 优先于 `ZOMBIE` 的顺序与 `zombify` 的 `TRACE_ZOMBIE → check_parent` 串联（`623`）同序；`W_STOPCODE` 同步返 `pid`（`531` 非 `SUSPEND`，`wait.h:32`），`ZOMBIE` 经 `tell_parent` 的 `SUSPEND` 异步唤醒（`545`）。

### 2.4 `do_wait4` 尾段：`WNOHANG` / `ECHILD`（forkexit.c:550-563）

```c
if (children > 0) {                   // 551 至少一子满足 pidarg，但未退出
    if (options & WNOHANG) {          // 553 WNOHANG 0x01（wait.h:10）
        return(0);                    // 554 父不等待，同步返 0
    }
    mp->mp_flags |= WAITING;          // 556 WAITING 置位（mproc.h:87 0x00002，WaitState::waiting）
    mp->mp_wpid = (pid_t) pidarg;     // 557 保存 pidarg 供 wait_test
    mp->mp_waddr = addr;              // 558 保存 rusage addr 供 tell_parent
    return(SUSPEND);                  // 559 SUSPEND 使 main.c:106 不 reply，由未来 tell_parent 唤醒
} else {
    return(ECHILD);                   // 562 sys/errno.h:10 ECHILD 10（no child）
}
```

`WNOHANG` 与 `SUSPEND` 的互斥：`children>0` 且 `WNOHANG → 0` 同步返，`children==0`（无 acceptable child，`502-508` 三条件过滤后 `children==0`）→ `ECHILD` 同步返。

### 2.5 `wait_test`（forkexit.c:569-588）

```c
pidarg = rmp->mp_wpid;                // 581
parent_waiting = rmp->mp_flags & WAITING; // 582
right_child = (pidarg == -1 || pidarg == child->mp_pid || // 583-585
    -pidarg == child->mp_procgrp);
return (parent_waiting && right_child); // 587 WAITING && right_child 双条件
```

`right_child` 的 `-pidarg == procgrp` 与 `WaitTarget::Group` 的 `is_waiting_for` 同形（`mproc/wait.rs:67`），`WAITING` 与 `right_child` 双条件与 `WaitState::is_waiting_for` 的 `WAITING && right_child` 同形。

### 2.6 `tell_parent`（forkexit.c:670-726）

```c
mp_parent= child->mp_parent;          // 684
if (mp_parent <= 0) panic("tell_parent: bad value in mp_parent: %d", mp_parent); // 685-686
if(!(child->mp_flags & ZOMBIE)) panic("tell_parent: child not a zombie"); // 687-688
if(child->mp_flags & TOLD_PARENT) panic("tell_parent: telling parent again"); // 689-690 TOLD_PARENT 防重
parent = &mproc[mp_parent];
if (addr) {                           // 694 addr 非 0 则 rusage 拷贝
    memset(&r_usage, 0, sizeof(r_usage)); // 699
    set_rusage_times(&r_usage, child->mp_child_utime, child->mp_child_stime); // 700-701 utility.c:92 仅 ru_utime/ru_stime
    if ((r = sys_datacopy(SELF, (vir_bytes)&r_usage, parent->mp_endpoint, addr, sizeof(r_usage))) != OK) { // 703-704
        reply(child->mp_parent, r);   // 705 拷贝失败 reply(parent, errno)
        return FALSE;                 // 707 FALSE 使 try_cleanup 清零，不 cleanup
    }
}
parent->mp_reply.m_pm_lc_wait4.status = W_EXITCODE(child->mp_exitstatus, child->mp_sigstatus); // 712-713 wait.h:63
reply(child->mp_parent, child->mp_pid); // 714 reply(parent, pid) 唤醒（main.c:250-270）
parent->mp_flags &= ~WAITING;         // 715 WAITING 清
child->mp_flags &= ~ZOMBIE;           // 716 ZOMBIE 清
child->mp_flags |= TOLD_PARENT;       // 717 TOLD_PARENT 置位（避免二次通知，mproc/lifecycle.rs:27）
parent->mp_child_utime += child->mp_child_utime; // 722-723 累计至父（POSIX 累计在 wait 前不回父，forkexit.c:303-305）
parent->mp_child_stime += child->mp_child_stime;
return TRUE;                          // 725 TRUE 使 check_parent 的 try_cleanup 有效
```

`sys_datacopy` 失败 `return FALSE` 使 `check_parent` 的 `try_cleanup` 清零（`652-653`），`VFS|EVENT` 时不 `cleanup`（`658`），`W_EXITCODE` 的 `exitstatus/sigstatus` 为 `char` 截断（`mproc.h:25-26`）。

### 2.7 `tell_tracer`（forkexit.c:732-754）

```c
mp_tracer = child->mp_tracer;         // 739
if (mp_tracer <= 0) panic("tell_tracer: bad value in mp_tracer: %d", mp_tracer); // 740-741
if(!(child->mp_flags & TRACE_ZOMBIE)) panic("tell_tracer: child not a zombie"); // 742-743
tracer = &mproc[mp_tracer];
tracer->mp_reply.m_pm_lc_wait4.status = W_EXITCODE(child->mp_exitstatus, (child->mp_sigstatus & 0377)); // 748-749 sigstatus & 0377 截断（wait.h:32）
reply(child->mp_tracer, child->mp_pid); // 750
tracer->mp_flags &= ~WAITING;         // 751
child->mp_flags &= ~TRACE_ZOMBIE;     // 752
child->mp_flags |= ZOMBIE;            // 753 TRACE_ZOMBIE→ZOMBIE（伪父转真父僵尸，09 zombify 对偶）
```

`sigstatus & 0377` 截断与 `wait.h:32` `W_STOPCODE` 同源，`TRACE_ZOMBIE→ZOMBIE` 使 `check_parent` 后对真父可见（`752-753`）。

### 2.8 `set_rusage_times`（utility.c:92-106）

```c
r_usage->ru_utime = child_utime;      // 92 仅 ru_utime/ru_stime
r_usage->ru_stime = child_stime;      // 92
// 其余 ru_ 字段 TODO 保留 0（utility.c:92 注释）
```

`forkexit.c:699` `memset(0)` 后 `set_rusage_times`，其余 `ru_` 字段 `TODO` 保留 `0`（`utility.c:92` 注释"TODO: support other fields"）。

### 2.9 `cleanup`（forkexit.c:795-806）

```c
rmp->mp_pid = 0;                      // 801
rmp->mp_flags = 0;                    // 802 Lifecycle::Unused + BlockState::default
rmp->mp_child_utime = 0;              // 803
rmp->mp_child_stime = 0;              // 804
procs_in_use--;                       // 805 与 07 procs_in_use++ 成对（table.rs:172 release_slot）
```

`procs_in_use--` 与 07 `alloc_slot` 的 `++` 成对，`Lifecycle::Unused` 使 `find_free_slot` 可复用。

### 2.10 主循环与 `zombify` 衔接（main.c:80-82/502）

`main.c:80-82` `EXITING` 丢弃（退出中进程的延迟调用 `continue`）+ `do_wait4` 的 `IN_USE|TOLD_PARENT != IN_USE` 过滤（`502` 已收割不再被等待）+ `zombify` 的 `check_parent(FALSE)` 对 `WAITING` 父的 `tell_parent` 唤醒（`623`），`TOLD_PARENT→cleanup` 的 `procs_in_use--` 使 `fork` 的容量检查可见。

---

## 3 Rust 设计决策

Rust 改写遵循"显式扫描器 + `WaitState` 同生同灭 + `Lifecycle` 互斥 + `TOLD_PARENT` 防重"的 8 决策，保留 C 的三环顺序与 `VFS|EVENT` 守卫，但用类型系统使 `pidarg` 四态与 `WAITING` 位穷尽。以下决策对应 `.design/10-design.v1.md` 的 D1–D8。

### D1：`pidarg` 四态收敛到 `WaitTarget` 枚举（ARCH A-2）

`WaitTarget::{AnyChild, SpecificChild(Pid), Group(Pid)}`（`mproc/wait.rs:30`）+ `from_pidarg(pidarg, caller_procgrp)` 归一化（`pidarg==0 → Group(-procgrp)`，`forkexit.c:493`），`is_waiting_for` 的 `match` 穷尽。

### D2：`WAITING` 位 + `mp_wpid/mp_waddr` 收敛到 `WaitState`（ARCH A-2）

`WaitState { waiting: bool, target: WaitTarget, rusage_addr: VirBytes }`（`mproc/wait.rs:12`）同生同灭（`waiting` 与 `target`/`rusage_addr` 同 `Default`），`WAITING` 置位即 `WaitState { waiting:true, target: from_pidarg(...), rusage_addr }`，`tell_parent` 后 `waiting=false`。

### D3：`do_wait4` 主扫描收敛到 `WaitScanner`（ARCH A-2/A-12）

`children=0` + `for rp=0..NR_PROCS` 三条件过滤 + `children++` + 三环 `TRACE_ZOMBIE→tell_tracer`/`TRACE_STOPPED→W_STOPCODE`/`ZOMBIE→tell_parent`（`forkexit.c:500-548`），`WNOHANG` 与 `ECHILD` 分支与 `553-562` 同。

### D4：`wait_test` 收敛到 `WaitState::is_waiting_for`（ARCH A-2）

`mproc/wait.rs:67` `WAITING && right_child` 双条件，`right_child` 的 `Group` 即 `child.procgrp == -pgrp`（`forkexit.c:585`）。

### D5：`tell_parent` 的 `sys_datacopy(rusage)` + `W_EXITCODE` + `WAITING`清 + `ZOMBIE→TOLD_PARENT`

`wait.rs: tell_parent` 的 `sys_datacopy` 占位（`minix-types` 的 `sys_datacopy` 封装，`utility.c:92` 的 `set_rusage_times` 仅两字段，失败 `reply(errno)` + `return false`）+ `Lifecycle::ToldParent`（`A-2`）。

### D6：`tell_tracer` 的 `TRACE_ZOMBIE→ZOMBIE` 伪父转换（ARCH A-12）

`Lifecycle::TraceZombie → Zombie` 枚举转换，`W_EXITCODE` 的 `sigstatus & 0377` 截断。

### D7：`cleanup` 收敛到 `ProcTable::release_slot`（ARCH A-2）

`mproc/table.rs:172` `Process::default()` + `procs_in_use--`（与 07 `++` 成对），`try_cleanup && !(VFS|EVENT)` 守卫由 `BlockState::ipc_blocked` 判断。

### D8：`set_rusage_times` 仅 `ru_utime/ru_stime`（ARCH A-11）

`types/clock.rs` `Clock` 占位，`W_EXITCODE` 的 `rusage` 拷贝失败不 `cleanup`。

---

## 4 实现详解

### 4.1 `os/servers/pm/src/wait.rs`

`handle_wait4(caller, pidarg, options, rusage_addr, table, transport) -> WaitOutcome`（`do_wait4` 主扫描 + 三环 + `WNOHANG`/`ECHILD`）+ `wait_test` + `tell_parent` + `tell_tracer` + `cleanup` + `set_rusage_times` 的 `rusage` 填充（`W_EXITCODE`/`W_STOPCODE` 宏 + `sys_datacopy` 占位）。

### 4.2 `os/servers/pm/src/mproc/wait.rs`

`WaitState` + `WaitTarget` 枚举 + `is_waiting_for` + `rusage_addr`（`mproc.h:31-32` `mp_wpid/mp_waddr`）。

### 4.3 `os/servers/pm/src/mproc/{lifecycle,guardianship}.rs`

`Lifecycle::Zombie/TraceZombie/ToldParent` 互斥 + `Guardianship::tracer` 伪父（`A-12`）。

### 4.4 `os/servers/pm/src/ipc/{dispatcher,init}.rs`

`PmCall::Wait4 = 3` + `ReplyIntent`（`do_wait4` 的 `SUSPEND` 异步由 `tell_parent` 唤醒 vs `W_STOPCODE` 同步返 `pid` vs `0`/`ECHILD` 同步返）。

### 4.5 不变量表（表：C 锚点 → Rust 表达 → 检测 panic/Reply/SUSPEND）

| # | 不变量 | C 锚点 | Rust 表达 |
|---|--------|--------|-----------|
| 1 | `pidarg==0 → -procgrp` | `forkexit.c:493` | `WaitTarget::from_pidarg` |
| 2 | `IN_USE|TOLD_PARENT != IN_USE` 过滤 | `forkexit.c:502` | `Lifecycle::ToldParent` 不计 |
| 3 | `TRACE_ZOMBIE` 优先 | `forkexit.c:512-517` | `handle_wait4` 三环顺序 |
| 4 | `WNOHANG → 0` | `forkexit.c:553-554` | `WaitOutcome::WouldBlock` |
| 5 | `ECHILD` | `forkexit.c:560-562` | `WaitOutcome::NotChild` |
| 6 | `TOLD_PARENT` 防重 | `forkexit.c:689-690` | `Lifecycle::ToldParent` 互斥 `panic` |
| 7 | `VFS|EVENT` 不 `cleanup` | `forkexit.c:658` | `!is_vfs_or_event_blocked` 守卫 |

---

## 5 测试矩阵

### 5.1 `mproc/wait.rs`（`WaitState`）

- `test_default_not_waiting`：`WaitState::default` → `!waiting`
- `test_waiting_for_any_child`：`AnyChild` 匹配任一 `pid`
- `test_waiting_for_specific_child`：`SpecificChild(1234)` 精确匹配
- `test_waiting_for_group`：`Group(-100)` 匹配 `procgrp==100`

### 5.2 `wait.rs`（`handle_wait4` 三环与 `WNOHANG`/`ECHILD`）

- `test_wait4_trace_zombie`：`TRACE_ZOMBIE`→`tell_tracer`+`SUSPEND`（`TRACE_ZOMBIE→ZOMBIE` 转换）
- `test_wait4_zombie`：`ZOMBIE`→`tell_parent`+`SUSPEND`（`W_EXITCODE` + `TOLD_PARENT`）
- `test_wait4_wnohang`：`children>0` 且 `WNOHANG → 0` 同步返
- `test_wait4_echild`：`children==0` → `ECHILD`
- `test_wait4_suspend`：`children>0` 且 `!WNOHANG` → `WAITING` 置位 + `SUSPEND`
- `test_wait_target_from_pidarg`：`0→Group(-procgrp)` 归一化
- `test_tell_parent_rusage`：`sys_datacopy` 失败→`return false` 不 `TOLD_PARENT`
- `test_cleanup_releases_slot`：`TOLD_PARENT→cleanup` 的 `procs_in_use--`

共 **8** 项，与 doc 矩阵一一对应；`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **172 passed**（含 `mproc/wait.rs` 4 + `wait.rs` 1 基线内），本章新增 6 项后将至 **178/108**。

---

## 6 过渡

本篇在主循环 `PM_WAIT4` 与 `zombify` 的消费侧之间的位置；`procs_in_use` 的 `--` 与 07 的 `++` 成对，`TOLD_PARENT` 为 `cleanup` 的唯一入口，`W_STOPCODE` 的 `TRACE_STOPPED` 语义为 18-trace.md 的 `ptrace` 停止等待前置——`wait4` 的 `WAITING` 置位使 `check_parent` 的 `SIGCHLD` 路径（11）与 `wait4` 的同步唤醒正交。

## 7 参见

- C 源：`minix3/minix/servers/pm/forkexit.c:471-807`（`do_wait4`/`wait_test`/`tell_parent`/`tell_tracer`/`cleanup`）、`minix3/minix/servers/pm/utility.c:92-106`（`set_rusage_times`）、`minix3/minix/servers/pm/mproc.h:86-92`（`WAITING`/`ZOMBIE`/`TOLD_PARENT`）、`minix3/minix/include/sys/wait.h:32/63`（`W_STOPCODE`/`W_EXITCODE`）
- 设计契约：`.design/10-design.v1.md`（D1–D8 与行为契约表）、`.design/10-outline.v1.md`、`.design/10-outline-review.v1.md`
- PM 阶段文档：03-mproc-table.md（`procs_in_use`/`WaitState`）、09-pm-exit.md（`zombify` 两级僵尸与 `cleanup`）、02-mproc-struct.md（`Lifecycle`/`Guardianship`）、06-event-subscription.md（`VFS|EVENT` 时不 `cleanup` 的 `try_cleanup` 守卫）、18-trace.md（`TRACE_STOPPED`）、11-signal-core.md（`W_STOPCODE` 的信号语义）、05-stage-vfs（`VFS_CALL` 守卫）
- 对端实现：02-stage-vm（`vm_getrusage`）、05-stage-vfs（`VFS_CALL` 守卫）
- 内核接口：01-stage-kernel（`sys_datacopy` 跨地址空间拷贝）
