# 10 — 等待与回收：`do_wait4` 的三环与 `tell_parent` 的 `TOLD_PARENT`

> **Rust 实现**: `os/servers/pm/src/wait.rs`（`do_wait4` 主扫描与三环）、`os/servers/pm/src/exit.rs`（`wait_test`/`check_parent`/`tell_parent`/`tell_tracer`/`cleanup`/`zombify`）、`os/servers/pm/src/mproc/{wait,lifecycle,guardianship,table}.rs`（`WaitState`/`WaitTarget`/`Lifecycle` 与槽位回收）、`os/servers/pm/src/ipc/{calls,dispatcher,decode}.rs`（`PmCall::Wait4` 与回复意图）

本文讲清 `do_wait4` 如何在一次 `PM_WAIT4` 中完成"父进程对子进程死亡的等待与回收"：从 `pidarg` 的四态归一化（`>0`/`0→-procgrp`/`-1`/`<-1`）与 `WNOHANG`/`ECHILD`，到全表扫描的三条件过滤（`IN_USE|TOLD_PARENT`/`parent||tracer`/`ZOMBIE` 伪父）与三环（`TRACE_ZOMBIE→tell_tracer`/`TRACE_STOPPED→W_STOPCODE`/`ZOMBIE→tell_parent`），再到 `wait_test` 的 `WAITING∧right_child` 双条件与 `tell_parent` 的 `sys_datacopy(rusage)`+`W_EXITCODE`+`WAITING`清+`ZOMBIE→TOLD_PARENT`+`child_utime/stime` 累计，以及 `tracer` 伪父的 `TRACE_ZOMBIE→ZOMBIE` 转换与 `cleanup` 的 `procs_in_use--`（与 07 的 `++` 成对）。

前置阅读：03-mproc-table.md（`ProcTable`/`WaitState`/`Pid`/`procgrp`）、09-pm-exit.md（`zombify` 两级僵尸 `ZOMBIE`/`TRACE_ZOMBIE` 与 `cleanup` 的 `procs_in_use--`）、02-mproc-struct.md（`Lifecycle` 互斥枚举 + `Guardianship` 双监护）、06-event-subscription.md（`VFS|EVENT` 时不 `cleanup` 的 `try_cleanup` 守卫）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `exit` 的两阶段与僵尸生产（09）、`ProcTable` 三层身份与 `Lifecycle` 互斥、`WAITING` 位的 `WaitState` 的开发者。

> **本章不讲什么**：
> - 僵尸产生 `zombify` 的 `TRACE_ZOMBIE` vs `ZOMBIE` 互斥（09-pm-exit.md §2.6，已在 09 落地）
> - `trace_stop` 的 `TRACE_STOPPED` 语义（18-trace.md）
> - `sig_proc(SIGCHLD)` 的发送（11-signal-core.md，`check_parent` 的 `else → sig_proc` 分支）
> - `WNOHANG` 以外的 `options` 位（`WUNTRACED` 等归 18-trace.md，`minix3/minix/servers/pm/forkexit.c:do_wait4` 仅识别 `WNOHANG`）
>
> 本章只回答一个问题：**为什么 `wait4` 必须在一次全表扫描中区分“追踪僵尸→追踪停止→普通僵尸”三环，且 `tell_parent` 的 `TOLD_PARENT` 与 `VFS|EVENT` 守卫如何使“已收割不再被等待”与“仍阻塞不回收”同时成立**。

### 1.1 为什么需要 wait：`exit` 仅生产，`wait` 才消费

`exit`（09）的 `zombify` 仅生产 `ZOMBIE`/`TRACE_ZOMBIE`（`minix3/minix/servers/pm/forkexit.c:zombify`），`procs_in_use` 仍计数，`mp_pid` 仍保留，若直接 `cleanup` 则 `W_EXITCODE` 与 `rusage` 无源可交付。`wait4`（本章）经 `tell_parent` 消费：`sys_datacopy(rusage)` 跨地址空间拷贝 `ru_utime/ru_stime`（`minix3/minix/servers/pm/utility.c:set_rusage_times` 仅填这两个成员，其余 `ru_` 字段保持 `memset(0)` 的零值；`minix3/minix/servers/pm/forkexit.c:tell_parent` 在拷贝失败时 `reply(errno)` + `return FALSE`，使 `try_cleanup` 清零、不 `cleanup`，子进程留待重试），`W_EXITCODE`（`minix3/sys/sys/wait.h:66`，`exitstatus/sigstatus` 为 `char`）写 `parent->mp_reply`，`reply(parent, pid)` 唤醒父进程，`WAITING` 清，`ZOMBIE→TOLD_PARENT`（`minix3/minix/servers/pm/forkexit.c:tell_parent`），`child_utime/stime` 累计至父，`TOLD_PARENT` 避免二次通知（`forkexit.c:tell_parent` 对二次通知 `panic`）。

### 1.2 为什么 `pidarg` 有四态：`>0`/`0→-procgrp`/`-1`/`<-1`

`wait4` 的首参 `pid`（`m_in.m_lc_pm_wait4.pid`，`minix3/minix/servers/pm/forkexit.c:do_wait4`）四态在入口归一：

```c
if (pidarg == 0) pidarg = -mp->mp_procgrp; // pidarg < 0 ==> proc grp
```

- `pidarg > 0`：等待 `pid == pidarg` 的精确子；
- `pidarg == 0`：归一为 `-procgrp`，等待同组任一子（`-pidarg == procgrp`）；
- `pidarg == -1`：等待任一子；
- `pidarg < -1`：等待组 `-pidarg` 任一子。

`0` 归一化使会话/组语义与 `getset.c` 的 `mp_procgrp`（`minix3/minix/servers/pm/mproc.h:mp_procgrp`，`pid == procgrp` 即会话 Leader）统一，`WaitTarget` 枚举（`os/servers/pm/src/mproc/wait.rs:enum WaitTarget` 的 `AnyChild`/`SpecificChild`/`Group`）使 `is_waiting_for` 的 `match` 穷尽且 `0` 归一化显式化。

### 1.3 为什么是 `SUSPEND` 而非同步 `reply`：`WAITING` 的异步唤醒

`wait4` 的满足条件可能未来才出现（子尚未退出），`do_wait4` 置 `WAITING` + `mp_wpid/mp_waddr` 后 `SUSPEND`（三字段同生同灭，`os/servers/pm/src/mproc/wait.rs:struct WaitState`），由 `zombify/check_parent` 的 `tell_parent` 异步 `reply(parent, pid)` 唤醒，与 `do_fork` 的 `SUSPEND`（07，`VFS_PM_FORK`）同源（04 `ReplyLater`）但唤醒源为 `exit` 而非 `VFS`。

`WNOHANG`（`minix3/sys/sys/wait.h:79`，值 `0x00000001`）与 `ECHILD`（`minix3/sys/sys/errno.h:51`，值 `10`）为同步返路径：`children>0` 且 `WNOHANG → 0`（父不等待），`children==0`（三条件过滤后无 `acceptable child`）→ `ECHILD`。

### 1.4 为什么需要 `tracer` 伪父：`TRACE_ZOMBIE` 与 `W_STOPCODE`

`ptrace` 使 `tracer` 成为"伪父"（`mp_tracer == who_p`，`minix3/minix/servers/pm/mproc.h:mp_tracer`，`NO_TRACER` 为 `0`，见 `minix3/minix/servers/pm/const.h:11`），`wait4` 的首环先扫描 `TRACE_ZOMBIE`/`TRACE_STOPPED`：`TRACE_ZOMBIE` 经 `tell_tracer` 转 `ZOMBIE` 后再对真父可见；`TRACE_STOPPED` 经 `W_STOPCODE(i)`（`minix3/sys/sys/wait.h:67`）直接返 `pid`——扫描 `1.._NSIG`（`_NSIG` 为 64，见 `minix3/sys/sys/signal.h:45`）用 `sigismember(sigtrace, i)` 找最低待报告信号位并 `sigdelset` 消费之，`mp_reply` 存停止码而非退出码（`minix3/minix/servers/pm/forkexit.c:do_wait4`）。因此 `wait4` 先服务追踪僵尸/停止，再服务普通僵尸；这条顺序与 `zombify` 的 `TRACE_ZOMBIE → check_parent` 串联同源。

### 1.5 为什么 `rusage` 需 `sys_datacopy`：跨地址空间拷贝

`wait4` 的 `addr` 为用户虚地址（`m_lc_pm_wait4.addr`，`minix3/minix/servers/pm/forkexit.c:do_wait4`），`PM` 需经 `sys_datacopy(SELF, &r_usage, parent_ep, addr)` 跨地址空间拷贝（`minix3/minix/kernel/system/do_datacopy.c`）；失败 `reply(parent, errno)` + `return FALSE` 使 `try_cleanup` 清零、不 `cleanup`（`check_parent` 的 `try_cleanup && !(VFS|EVENT) → cleanup` 禁止在 `VFS|EVENT` 时回收）。`set_rusage_times` 仅填 `ru_utime/ru_stime`（`minix3/minix/servers/pm/utility.c:set_rusage_times`，其余 `ru_` 字段保持 `tell_parent` 中 `memset(0)` 的零值）。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `for (rp=0; rp<NR_PROCS; rp++)`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `wait4`/`waitpid`。** Linux `wait4` 的 `pid` 四态（`>0` 精确/`0` 同组/`-1` 任一/`<-1` 组）与 `WNOHANG`/`WUNTRACED`/`WCONTINUED` + `rusage` 累计 + `__WNOTHREAD` 与 Minix3 的 `pidarg` 四态 + `WNOHANG` + `ECHILD` 同源，但 Linux 以 `task_struct->children` 链表 + `exit_state` 枚举，Minix3 以 `mproc[NR_PROCS]` 全表扫描 + `ZOMBIE` 位；Rust 侧 `WaitTarget` 枚举 + `WaitState` + `Lifecycle` 使 `pidarg` 过滤与 `WAITING` 位显式化。

**FreeBSD `wait6`。** FreeBSD `wait6` 的 `WRUSAGE` 与 `WTRAPPED` 与 Minix3 的 `set_rusage_times` 仅两字段 + `TRACE_STOPPED→W_STOPCODE` 同源，Minix3 的 `rusage` 仅 `ru_utime/ru_stime` 为简化。

**Redox `waitpid` 的 `Scheme` 阻塞。** Redox 以 `Scheme` 的 `FileDescription` 阻塞 + `waitpid` 的 `WAITING` 位，Minix3 以 `PM` 的 `WAITING` 位 + `mp_wpid/mp_waddr`，二者同为"父进程 `WAITING` 位 + 子 `ZOMBIE` 位"的双位协同；Rust 侧 `WaitState::is_waiting_for` 的 `WAITING && right_child` 双条件与 `minix3/minix/servers/pm/forkexit.c:wait_test` 同形。

**`seL4` 无 `wait`。** `seL4` 无 `wait` 原语，`TCB` 手动回收，Minix3 的僵尸链表（`ZOMBIE` 位 + `procs_in_use` 计数）与 `seL4` 手动回收同为显式解绑，`cleanup` 的 `procs_in_use--` 与 07 的 `++` 成对。

**本章的设计基线**：把 `do_wait4` 的 `pidarg` 四态 + 全表扫描三条件 + 三环（`TRACE_ZOMBIE`→`TRACE_STOPPED`→`ZOMBIE`）+ `wait_test` 双条件 + `tell_parent` 的 `sys_datacopy`+`TOLD_PARENT` + `tracer` 伪父转换，改写为"类型化 `WaitTarget` 枚举 + 显式扫描器 + `WaitState` 同生同灭 + `Lifecycle` 互斥 + `ProcTable::release_slot` 显式回收"。

### 1.7 小结

1. **为什么 wait**——`exit` 生产 `ZOMBIE`/`TRACE_ZOMBIE`，`wait4` 经 `tell_parent/tell_tracer` 消费 + `cleanup` 回收 `procs_in_use`，`TOLD_PARENT` 防重。
2. **为什么四态**——`pidarg` 的 `>0`/`0→-procgrp`/`-1`/`<-1` 与 `WaitTarget` 枚举穷尽，`Group` 的负值编码使 `is_waiting_for` 的 `match` 显式化。
3. **为什么 `SUSPEND`**——`WAITING` 置位后 `SUSPEND`，由 `check_parent` 的 `tell_parent` 异步唤醒，与 `fork` 的 `SUSPEND` 同源但唤醒源为 `exit`。
4. **为什么 `tracer` 伪父**——`TRACE_ZOMBIE` 先通知 `tracer` 伪父，经 `tell_tracer` 转 `ZOMBIE` 再对真父可见；`TRACE_STOPPED` 经 `W_STOPCODE` 同步返。
5. **为什么 `rusage` 需 `sys_datacopy`**——用户虚地址跨地址空间拷贝，失败不 `cleanup`，`set_rusage_times` 仅两字段。

下一章逐行分析 C 的 `do_wait4`/`wait_test`/`tell_parent`/`tell_tracer`；第 3 章给出 Rust 的显式扫描器与状态机。

---

## 2 C 源码分析

### 2.1 `do_wait4` 序言：`pidarg` 四态归一化（minix3/minix/servers/pm/forkexit.c:do_wait4）

```c
pidarg  = m_in.m_lc_pm_wait4.pid;          // 1st param
options = m_in.m_lc_pm_wait4.options;      // 3rd param
addr    = m_in.m_lc_pm_wait4.addr;         // 4th param（vir_bytes，06 的 VirBytes）
if (pidarg == 0) pidarg = -mp->mp_procgrp; // 0 → -procgrp（mproc.h:mp_procgrp，会话/组归一）
```

`m_lc_pm_wait4` 为 `minix3/minix/include/minix/ipc.h:mess_lc_pm_wait4`（`pid_t pid` + `int options` + `vir_bytes addr`）；`pidarg==0` 的归一使 `wait(0)` 等同 `waitpid(-procgrp)`（等待同组任一子，`getset.c` 进程组语义）。

### 2.2 `do_wait4` 主扫描：三条件过滤与 `children` 计数（minix3/minix/servers/pm/forkexit.c:do_wait4）

```c
children = 0;
for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++) { // 全表扫描 NR_PROCS 256
    if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue; // IN_USE 且非 TOLD_PARENT（已收割不再被等待）
    if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;   // 非子非追踪（parent/tracer 双监护）
    if (rp->mp_parent != who_p && (rp->mp_flags & ZOMBIE)) continue;   // 僵尸但父非 who_p 的非追踪僵尸不计
    if (pidarg  > 0 && pidarg != rp->mp_pid) continue;                // pid 精确过滤
    if (pidarg < -1 && -pidarg != rp->mp_procgrp) continue;           // 组过滤（-pidarg == procgrp）
    children++;                       // acceptable child
```

第一条过滤使 `TOLD_PARENT`（已收割）不再被等待（对应 `Lifecycle::ToldParent`）；第二条的 `parent||tracer` 双监护与 `mproc/guardianship.rs` 的 `Guardianship` 对应；第三条使"父非调用者的僵尸"不被计入——这正是 `TRACE_ZOMBIE` 必须先转成 `ZOMBIE` 才对真父可见的原因（见 `minix3/minix/servers/pm/forkexit.c:tell_tracer`）。

### 2.3 `do_wait4` 三环：`TRACE_ZOMBIE`→`TRACE_STOPPED`→`ZOMBIE`（minix3/minix/servers/pm/forkexit.c:do_wait4）

```c
if (rp->mp_tracer == who_p) {         // tracer == who_p 首环
    if (rp->mp_flags & TRACE_ZOMBIE) { // 追踪僵尸
        tell_tracer(rp);              // TRACE_ZOMBIE→ZOMBIE + reply(tracer, pid) + WAITING 清
        check_parent(rp, TRUE /*try_cleanup*/); // true → try_cleanup && !(VFS|EVENT) → cleanup
        return(SUSPEND);              // tell_tracer 已 reply，SUSPEND 使主循环不二次 reply
    }
    if (rp->mp_flags & TRACE_STOPPED) { // 追踪停止
        for (i = 1; i < _NSIG; i++) { // 扫描 _NSIG（=64，sys/signal.h:45）
            if (sigismember(&rp->mp_sigtrace, i)) { // sigtrace 位图（mproc.h:mp_sigtrace）
                sigdelset(&rp->mp_sigtrace, i); // 清位（消费该停止事件）
                mp->mp_reply.m_pm_lc_wait4.status = W_STOPCODE(i); // wait.h:67
                return(rp->mp_pid);   // 同步返 pid（非 SUSPEND，主循环直接 reply）
            }
        }
    }
}

if (rp->mp_parent == who_p) {         // 真父环
    if (rp->mp_flags & ZOMBIE) {      // 普通僵尸
        waited_for = tell_parent(rp, addr); // sys_datacopy + W_EXITCODE + WAITING 清 + ZOMBIE→TOLD_PARENT
        if (waited_for && !(rp->mp_flags & (VFS_CALL | EVENT_CALL))) // VFS|EVENT 时不 cleanup（06 待串行完成）
            cleanup(rp);              // release_slot → procs_in_use--
        return(SUSPEND);              // tell_parent 已 reply(parent, pid)
    }
}
```

**交付对象的决定规则（本篇最容易被忽略的不变量）**：三环判定写在**每个子进程的循环体内**，任一环命中即 `return`。因此一次 `wait4` 只交付一个子进程，而交付对象是**表序最先命中者**——与它属于哪一环无关。当低表序的普通僵尸与高表序的追踪僵尸并存时，C 交付前者；把三环改写成"先全局扫追踪僵尸、再全局扫普通僵尸"会改变这个对象，属于语义偏移。

### 2.4 `do_wait4` 尾段：`WNOHANG` / `ECHILD`（minix3/minix/servers/pm/forkexit.c:do_wait4）

```c
if (children > 0) {                   // 至少一个子满足 pidarg，但未退出
    if (options & WNOHANG) {          // WNOHANG 0x00000001（wait.h:79）
        return(0);                    // 父不等待，同步返 0
    }
    mp->mp_flags |= WAITING;          // WAITING 置位（mproc.h:87）
    mp->mp_wpid = (pid_t) pidarg;     // 保存 pidarg 供 wait_test
    mp->mp_waddr = addr;              // 保存 rusage addr 供 tell_parent
    return(SUSPEND);                  // 不 reply，由未来 tell_parent 唤醒
} else {
    return(ECHILD);                   // 无子（errno.h:51，值 10）
}
```

`WNOHANG` 与 `SUSPEND` 互斥：`children>0` 且 `WNOHANG` 同步返 0；`children==0`（无 acceptable child）同步返 `ECHILD`。

### 2.5 `wait_test`（minix3/minix/servers/pm/forkexit.c:wait_test）

```c
pidarg = rmp->mp_wpid;                // who's being waited for?
parent_waiting = rmp->mp_flags & WAITING;
right_child =                         // child meets one of the 3 tests?
    (pidarg == -1 || pidarg == child->mp_pid ||
     -pidarg == child->mp_procgrp);
return (parent_waiting && right_child);
```

`right_child` 的三态（任意/精确 pid/进程组）与 `WaitTarget` 枚举一一对应；`check_parent` 与 `zombify` 都靠它决定"交付还是只发 `SIGCHLD`"——父在等**别的**子进程时不得交付（`WaitState::is_waiting_for` 是它在 Rust 侧的对应）。

### 2.6 `tell_parent`（minix3/minix/servers/pm/forkexit.c:tell_parent）

```c
mp_parent = child->mp_parent;
if (mp_parent <= 0) panic("tell_parent: bad value in mp_parent: %d", mp_parent);
if (!(child->mp_flags & ZOMBIE)) panic("tell_parent: child not a zombie");
if (child->mp_flags & TOLD_PARENT) panic("tell_parent: telling parent again"); // 防重
parent = &mproc[mp_parent];

if (addr) {                           // addr 非 0 才做 rusage 拷贝（waitpid(pid, NULL, 0) 合法）
    memset(&r_usage, 0, sizeof(r_usage));
    set_rusage_times(&r_usage, child->mp_child_utime, child->mp_child_stime); // utility.c:set_rusage_times
    if ((r = sys_datacopy(SELF, (vir_bytes)&r_usage, parent->mp_endpoint, addr,
                          sizeof(r_usage))) != OK) {
        reply(child->mp_parent, r);   // 拷贝失败：reply(parent, errno)
        return FALSE;                 // FALSE 使 check_parent 的 try_cleanup 清零，不 cleanup
    }
}
parent->mp_reply.m_pm_lc_wait4.status =
    W_EXITCODE(child->mp_exitstatus, child->mp_sigstatus); // wait.h:66
reply(child->mp_parent, child->mp_pid); // reply(parent, pid) 唤醒
parent->mp_flags &= ~WAITING;         // WAITING 清
child->mp_flags &= ~ZOMBIE;           // ZOMBIE 清
child->mp_flags |= TOLD_PARENT;       // TOLD_PARENT 置位（避免二次通知）
parent->mp_child_utime += child->mp_child_utime; // 累计至父（POSIX：wait 前不归父）
parent->mp_child_stime += child->mp_child_stime;
return TRUE;                          // TRUE 使 check_parent 的 try_cleanup 有效
```

`sys_datacopy` 失败时子进程保持 `ZOMBIE`（可重试），`VFS|EVENT` 时不 `cleanup`。

`W_EXITCODE` 的两个入参都是 `char`（`minix3/minix/servers/pm/mproc.h:mp_exitstatus`/`:mp_sigstatus`）：整型提升按**符号扩展**，`main.c` 的 `mp_sigstatus |= WCOREFLAG` 使带 core 的终止信号在 wire 上取负值（如 `0o200|6` 即 `-122`）。`tell_parent` 不做掩码，`tell_tracer` 显式 `& 0377`——两处不同，Rust 侧逐位照搬（`os/servers/pm/src/exit.rs`）。

### 2.7 `tell_tracer`（minix3/minix/servers/pm/forkexit.c:tell_tracer）

```c
mp_tracer = child->mp_tracer;
if (mp_tracer <= 0) panic("tell_tracer: bad value in mp_tracer: %d", mp_tracer);
if (!(child->mp_flags & TRACE_ZOMBIE)) panic("tell_tracer: child not a zombie");
tracer = &mproc[mp_tracer];

/* TODO: rusage support */

tracer->mp_reply.m_pm_lc_wait4.status =
    W_EXITCODE(child->mp_exitstatus, (child->mp_sigstatus & 0377)); // 截断到 8 位
reply(child->mp_tracer, child->mp_pid);
tracer->mp_flags &= ~WAITING;
child->mp_flags &= ~TRACE_ZOMBIE;
child->mp_flags |= ZOMBIE;            // TRACE_ZOMBIE→ZOMBIE（伪父转真父僵尸，与 09 的 zombify 对偶）
```

这里的 `& 0377` 是 `tell_tracer` 自身的位宽收敛（`mp_sigstatus` 为 `char`，截断后按无符号 8 位参与 `W_EXITCODE`）；`tell_parent` 不做这一步，两者对 `sigstatus` 的处理并不相同。`TRACE_ZOMBIE→ZOMBIE` 之后，子进程才在 `check_parent` 中对真父可见。

### 2.8 `set_rusage_times`（minix3/minix/servers/pm/utility.c:set_rusage_times）

```c
void set_rusage_times(struct rusage *r_usage, clock_t user_time, clock_t sys_time)
{
	u64_t usec;

	usec = user_time * 1000000 / sys_hz();
	r_usage->ru_utime.tv_sec  = usec / 1000000;
	r_usage->ru_utime.tv_usec = usec % 1000000;

	usec = sys_time * 1000000 / sys_hz();
	r_usage->ru_stime.tv_sec  = usec / 1000000;
	r_usage->ru_stime.tv_usec = usec % 1000000;
}
```

`ru_` 的其余成员（`ru_maxrss`、`ru_minflt` 等）保持调用方 `memset(0)` 后的零值——`tell_parent` 只在需要时才构造并拷贝整个 `struct rusage`。

### 2.9 `cleanup`（minix3/minix/servers/pm/forkexit.c:cleanup）

```c
rmp->mp_pid = 0;
rmp->mp_flags = 0;                    // Lifecycle::Unused + 默认阻塞态
rmp->mp_child_utime = 0;
rmp->mp_child_stime = 0;
procs_in_use--;                       // 与 07 alloc_slot 的 ++ 成对（table.rs:fn release_slot）
```

`procs_in_use--` 与 `alloc_slot` 的 `++` 成对，槽位随之可被 `find_free_slot` 复用。

### 2.10 `check_parent`：生产侧与消费侧的接缝（minix3/minix/servers/pm/forkexit.c:check_parent）

```c
if (p_mp->mp_flags & EXITING) {
	/* 子进程的父已退出：不做，改判给 INIT 后重查 */
}
else if (wait_test(p_mp, child)) {
	if (!tell_parent(child, p_mp->mp_waddr))
		try_cleanup = FALSE;          /* 子仍是僵尸 */
	if (try_cleanup && !(child->mp_flags & (VFS_CALL | EVENT_CALL)))
		cleanup(child);
}
else {
	sig_proc(p_mp, SIGCHLD, TRUE /*trace*/, FALSE /*ksig*/); // 父没在等这个子 → 只发信号
}
```

`zombify` 在子进程退出时调用它（`check_parent(rmp, FALSE)`），因此这里同时决定"立刻交付"与"只发 `SIGCHLD`"两条路——`wait_test` 的 `right_child` 就是这两条路的分界。

---

## 3 Rust 设计决策

Rust 改写遵循"显式扫描器 + `WaitState` 同生同灭 + `Lifecycle` 互斥 + `TOLD_PARENT` 防重"的 8 条决策，保留 C 的三环顺序、表序优先与 `VFS|EVENT` 守卫，但用类型系统使 `pidarg` 四态与 `WAITING` 位穷尽。

### D1：`pidarg` 四态收敛到 `WaitTarget` 枚举

`WaitTarget::{AnyChild, SpecificChild(Pid), Group(Pid)}`（`os/servers/pm/src/mproc/wait.rs:enum WaitTarget`）+ `from_pidarg(pidarg, caller_procgrp)` 归一化（`pidarg==0 → Group(-procgrp)`，对应 `minix3/minix/servers/pm/forkexit.c:do_wait4` 的入口归一），`is_waiting_for` 的 `match` 穷尽。

### D2：`WAITING` 位 + `mp_wpid/mp_waddr` 收敛到 `WaitState`

`WaitState { waiting: bool, target: WaitTarget, rusage_addr: VirBytes }`（`os/servers/pm/src/mproc/wait.rs:struct WaitState`）三字段同生同灭：`WAITING` 置位即 `WaitState { waiting: true, target: from_pidarg(...), rusage_addr }`，`tell_parent`/`tell_tracer` 后 `waiting = false`。

### D3：`do_wait4` 主扫描收敛到单趟扫描器（表序优先）

`children` 计数 + `for` 三条件过滤 + **每个子进程内按 `TRACE_ZOMBIE → TRACE_STOPPED → ZOMBIE` 判定、命中即返回**——扫描器必须保持"表序最先命中者被交付"，禁止拆成多趟遍历；`WNOHANG` 与 `ECHILD` 分支与 C 尾段同形。实现见 `os/servers/pm/src/wait.rs:fn do_wait4`。

### D4：`wait_test` 收敛到 `WaitState::is_waiting_for`

`os/servers/pm/src/exit.rs:fn wait_test` 调用 `WaitState::is_waiting_for(child_pid, child_procgrp)`（`os/servers/pm/src/mproc/wait.rs:impl WaitState::is_waiting_for`），即 `WAITING && right_child` 双条件；`right_child` 的 `Group` 分支为 `child_procgrp == -pgrp`，与 C 的 `-pidarg == child->mp_procgrp` 同形。

### D5：`tell_parent` 的 `sys_datacopy(rusage)` + `W_EXITCODE` + `WAITING` 清 + `ZOMBIE→TOLD_PARENT`

`os/servers/pm/src/exit.rs:fn tell_parent`：`addr != 0` 才拷贝 rusage（`KernelGateway::copy_to_user`），失败 `reply(errno)` + 返回 `false`；随后 `reply(parent, pid)`、清 `WAITING`、`Lifecycle::Zombie → Lifecycle::ToldParent`、子时间并入父桶。C 的三处 `panic` 在 Rust 侧由边界守卫与状态枚举承担。

### D6：`tell_tracer` 的 `TRACE_ZOMBIE→ZOMBIE` 伪父转换

`os/servers/pm/src/exit.rs:fn tell_tracer`：`W_EXITCODE(ec, sig_status & 0377)`、`reply(tracer, pid)`、清 tracer 的 `WAITING`、`Lifecycle::TraceZombie → Lifecycle::Zombie`。

### D7：`cleanup` 收敛到 `ProcTable::release_slot`

`os/servers/pm/src/exit.rs:fn cleanup` → `os/servers/pm/src/mproc/table.rs:fn release_slot`（槽位重置 + `procs_in_use--`，与 `alloc_slot` 成对）；`try_cleanup && !(VFS|EVENT)` 守卫由 `is_vfs_or_event_blocked` 判断。

### D8：`set_rusage_times` 仅 `ru_utime/ru_stime`

`os/servers/pm/src/exit.rs:fn tell_parent` 内联构造 144 字节 `rusage`：ticks 经 `table.system_hz` 换算成 `ru_utime/ru_stime` 的 sec/usec 四字段，其余保持 0。

---

## 4 实现详解

### 4.1 `os/servers/pm/src/wait.rs`

`do_wait4(caller, pidarg, options, rusage_addr, table, transport, kern) -> ReplyIntent`：入口归一 `pidarg==0 → -procgrp`，单趟扫描收集候选（三条件过滤 + `pidarg` 匹配）并计数 `children`，随后按表序逐候选判定三环，尾段返回 `ReplyIntent::Reply(0)`（`WNOHANG`）、`ReplyIntent::ReplyLater`（置 `WAITING`）或 `ReplyIntent::Reply(-ECHILD)`。TRACE_STOPPED 环扫描 `SignalState::trace_mask` 取最低待报告信号位并消费（对应 `sigismember`+`sigdelset`），回复载荷 `W_STOPCODE(i)`、返回 pid；`sigtrace` 为空时与 C 一致落出该环继续 ZOMBIE 环，不虚构停止码。

### 4.2 `os/servers/pm/src/exit.rs`

`wait_test`（调用 `WaitState::is_waiting_for`）、`check_parent`（`wait_test` 真 → `tell_parent` + 守卫 `cleanup`；假 → `sig_proc(SIGCHLD)`）、`tell_parent`、`tell_tracer`、`cleanup`（→ `release_slot`）、`zombify` 与 `tracer_died` 都在此文件——它们是 09 的生产侧与本章消费侧共用的接缝。

### 4.3 `os/servers/pm/src/mproc/{wait,lifecycle,guardianship,table}.rs`

`WaitState` + `WaitTarget` 枚举 + `is_waiting_for` + `rusage_addr`（对应 `mp_wpid/mp_waddr`）；`Lifecycle::{Zombie, TraceZombie, ToldParent}` 互斥；`Guardianship::{Normal, Traced}` 表达 `parent`/`tracer` 双监护；`release_slot`/`alloc_slot` 维护 `procs_in_use`。

### 4.4 `os/servers/pm/src/ipc/{calls,dispatcher,decode}.rs`

`PmCall::Wait4 = 3` + `ReplyIntent::{Reply, ReplyLater, NoReply}`：`W_STOPCODE` 环与尾段同步返（`Reply`），三环的 `SUSPEND` 异步由 `tell_parent`/`tell_tracer` 唤醒（`ReplyLater`）。回复载荷的 wire 契约：wait status 的"最后一跳"是载荷而非 `m_type`——C 在三处把状态写入 `mp_reply.m_pm_lc_wait4.status`（TRACE_STOPPED 环、`tell_parent`、`tell_tracer`），`m_type` 是子 pid（tag），载荷才是 status（body）。Rust 侧 `minix-types` 的 `MessPmLcWait4 { status: i32 }`（对应 `minix3/minix/include/minix/ipc.h:mess_pm_lc_wait4`，56 字节）与 `m_u.m_pm_lc_wait4` arm 承载它；端到端断言见 `os/servers/pm/tests/run_once_integration.rs:wait4_zombie_replies_pid_tag_with_status_payload`。

### 4.5 不变量表（C 锚点 → Rust 表达 → 检测点）

| # | 不变量 | C 锚点 | Rust 表达 |
|---|--------|--------|-----------|
| 1 | `pidarg==0 → -procgrp` | `minix3/minix/servers/pm/forkexit.c:do_wait4` | `WaitTarget::from_pidarg` |
| 2 | `IN_USE|TOLD_PARENT != IN_USE` 过滤 | `forkexit.c:do_wait4` | `Lifecycle::ToldParent` 不计 |
| 3 | 表序优先（单次只交付一个） | `forkexit.c:do_wait4` | `wait.rs:do_wait4` 单趟扫描，命中即返回 |
| 4 | `WNOHANG → 0` | `forkexit.c:do_wait4` | `ReplyIntent::Reply(0)` |
| 5 | `ECHILD` | `forkexit.c:do_wait4` | `ReplyIntent::Reply(-ECHILD)` |
| 6 | `TOLD_PARENT` 防重 | `forkexit.c:tell_parent` | `Lifecycle::ToldParent` 与 `release_slot` 唯一入口 |
| 7 | `VFS|EVENT` 不 `cleanup` | `forkexit.c:check_parent` | `!is_vfs_or_event_blocked` 守卫 |

---

## 5 测试矩阵

### 5.1 `mproc/wait.rs`（`WaitState`）

- `test_default_not_waiting`：`WaitState::default` → `!waiting`
- `test_waiting_for_any_child`：`AnyChild` 匹配任一 `pid`
- `test_waiting_for_specific_child`：`SpecificChild(1234)` 精确匹配
- `test_waiting_for_group`：`Group(-100)` 匹配 `procgrp==100`

### 5.2 `wait.rs` / `exit.rs`（`do_wait4` 三环与 `tell_parent` 族）

- `test_wait4_trace_stopped_reports_lowest_signal_and_consumes_bit`：TRACE_STOPPED 环取最低信号位、消费该位、载荷 `W_STOPCODE`
- `test_wait4_trace_stopped_empty_sigtrace_falls_through`：`sigtrace` 为空时落出该环，不虚构停止码
- `test_wait4_zombie_tell_parent`：`ZOMBIE`→`tell_parent`+`SUSPEND`（`TOLD_PARENT`）
- `test_wait4_zombie_status_carries_wcoreflag_byte`：`WCOREFLAG` 位型穿过载荷不变
- `test_wait4_table_order_beats_ring_order`：低表序普通僵尸优先于高表序追踪僵尸（表序优先）
- `test_wait4_wnohang`：`children>0` 且 `WNOHANG → 0` 同步返
- `test_wait4_echild_no_children`：`children==0` → `ECHILD`
- `test_wait4_suspend_when_child_running`：`children>0` 且 `!WNOHANG` → `WAITING` 置位 + `SUSPEND`
- `test_wait_target_from_pidarg_zero`：`0→Group(-procgrp)` 归一化
- `test_tell_parent_async_writes_status_payload`：异步路径 `reply(parent, pid)` + `W_EXITCODE` 载荷
- `test_tell_parent_delivers_rusage_via_datacopy`：rusage 经 `copy_to_user` 投递
- `test_cleanup_releases_slot`：`TOLD_PARENT→cleanup` 的 `procs_in_use--`
- `test_check_parent_waits_for_matching_child_only`：父在等别的子进程时只发 `SIGCHLD`，不交付

`exit.rs` 的 `test_check_parent_sends_sigchld_when_parent_not_waiting` 覆盖"父未等待"的 `SIGCHLD` 路径。

截至本篇复核时，`cargo test -p minix-pm --lib` 为 **426 passed / 0 failed**（复核命令见下）；本节所列 13 项均在其中。

---

## 6 过渡

本篇在主循环 `PM_WAIT4` 与 `zombify` 的消费侧之间的位置；`procs_in_use` 的 `--` 与 07 的 `++` 成对，`TOLD_PARENT` 为 `cleanup` 的唯一入口，`W_STOPCODE` 的 `TRACE_STOPPED` 语义为 18-trace.md 的 `ptrace` 停止等待前置——`wait4` 的 `WAITING` 置位使 `check_parent` 的 `SIGCHLD` 路径（11）与 `wait4` 的同步唤醒正交。

## 7 参见

- C 源：`minix3/minix/servers/pm/forkexit.c:do_wait4`（含 `wait_test`/`tell_parent`/`tell_tracer`/`check_parent`/`cleanup`）、`minix3/minix/servers/pm/utility.c:set_rusage_times`、`minix3/minix/servers/pm/mproc.h:IN_USE`（`WAITING`/`ZOMBIE`/`TOLD_PARENT`）、`minix3/sys/sys/wait.h:66`（`W_EXITCODE`）与 `:67`（`W_STOPCODE`）
- PM 阶段文档：03-mproc-table.md（`procs_in_use`/`WaitState`）、09-pm-exit.md（`zombify` 两级僵尸与 `cleanup`）、02-mproc-struct.md（`Lifecycle`/`Guardianship`）、06-event-subscription.md（`VFS|EVENT` 时不 `cleanup` 的 `try_cleanup` 守卫）、18-trace.md（`TRACE_STOPPED`）、11-signal-core.md（`SIGCHLD` 的 `check_parent` 路径）
- 对端实现：02-stage-vm（`vm_getrusage`）、05-stage-vfs（`VFS_CALL` 守卫）
- 内核接口：01-stage-kernel（`sys_datacopy` 跨地址空间拷贝）
- 复核命令：`docker run --rm --memory=2g -u $(id -u):$(id -g) -v "$PWD/os:/work" -w /work minix-ci:1.94 cargo test -j 1 -p minix-pm --lib`
