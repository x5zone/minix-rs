# 09 — 退出路径 `do_exit → exit_proc → exit_restart` 与僵尸收养链

本文讲清进程退出如何分两阶段（`exit_proc` 的 9 步在 `VFS_PM_EXIT` 投递前、`exit_restart` 的 5 步在 `VFS_PM_EXIT_REPLY` 到达后）完成资源解绑：从 `do_exit` 的 `PRIV_PROC→SIGKILL` 门、到 `exit_proc` 的 `sys_stop` 强制→`vm_willexit`→`VFS_PM_EXIT`→`PRIV_PROC` 直毁→`EXITING`→`zombify`→`disinherit` 收养 `INIT` 的 `NEW_PARENT` 记忆与 `SIGHUP`，再经 05 的 `handle_vfs_reply` 与 06 的 `publish_event` 衔接至 `exit_restart` 的 `sched_stop`→`sys_clear`→`vm_exit`→`TRACE_EXIT`→`cleanup`，以及 `zombify`/`check_parent` 的两级僵尸（`ZOMBIE` vs `TRACE_ZOMBIE`）如何与 10 的 `wait4` 形成生产–消费闭环。

前置阅读：03-mproc-table.md（`procs_in_use`/`pm_isokendpt`）、05-vfs-interaction.md（`tell_vfs` 三段式与 `handle_vfs_reply` 的 EXIT 提前 `return`）、06-event-subscription.md（`publish_event` 的 EXIT 事件与 `resume_event` 的 `exit_restart` 分支）、02-mproc-struct.md（`Lifecycle`/`Guardianship`/`BlockState`）、07-pm-fork.md（`procs_in_use++` 与 `get_free_pid` 的对应 `cleanup` 回收）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `fork` 的两阶段与 `VFS_CALL` 延续（07）、`VFS` 异步投递（05）与事件串行（06），知道 `Lifecycle` 互斥枚举与 `Guardianship` 双监护的开发者。

> **本章不讲什么**：
> - `wait4` 的回收细节（`10-pm-wait.md`，`tell_parent/tell_tracer/wait_test`）
> - `VFS_PM_EXIT` 的 `fproc` 释放细节（`05-stage-vfs`，`fproc` 的 `fd`/`cwd` 清理）
> - `vm_willexit`/`vm_exit` 的页表释放（`02-stage-vm`，`VM_WILLEXIT/VM_EXIT` 对端）
> - `TRACE_EXIT` 的 `ptrace(T_EXIT)` 细节（`18-trace.md`）
> - `SIGHUP` 的信号投递（`11-signal-core.md`，`check_sig` 广播）
>
> 本章只回答一个问题：**为什么退出必须分两阶段（`exit_proc` 与 `exit_restart` 以 `VFS_PM_EXIT` 为界），以及僵尸与收养如何使“进程已死但父未收”的中间态可观测**。

### 1.1 为什么退出是两阶段：多服务解绑的顺序敏感

进程是四表共享的资源容器（`proc` 可调度实体 + `vmproc` 地址空间 + `fproc` 打开文件 + `mproc` 身份），退出需按序解绑——`forkexit.c:311-315` 注释点明：

> *Tell the kernel the process is no longer runnable ... Then tell VFS ... and finally, clean up the process at the kernel. This order is important so that VFS can tell drivers to cancel requests such as copying to/ from the exiting process, before it is gone.*

单步解绑（`sys_clear` + `vm_exit` 立即）会使 VFS 在驱动仍拷贝至已消失 `proc` 时访问悬空 `proc`。因此 `exit_proc` 的 9 步在 `VFS_PM_EXIT` 投递前完成 `sys_stop`（停调度）与 `vm_willexit`（告 VM 将退出），`tell_vfs` 后 `EXITING` 标记并 `zombify`，`exit_restart` 的 5 步在 `VFS_PM_EXIT_REPLY` 到达后完成 `sched_stop`→`sys_clear`→`vm_exit`（`forkexit.c:418-457`），`VFS_PM_EXIT` 为两阶段的分界。

### 1.2 为什么系统服务直毁：`PRIV_PROC` 的死锁避免

`forkexit.c:361-369` 注释：

> *Destroy system processes without waiting for VFS. This is needed because the system process might be a block device driver that VFS is blocked waiting on.*

若 `exit_proc` 对 `PRIV_PROC` 仍等待 `VFS_PM_EXIT_REPLY`，`VFS` 正 `blocked waiting on` 该驱动（如块设备 `driver`），则 `PM` 等 `VFS`、`VFS` 等驱动、`驱动`即退出进程本身，死锁。因此 `PRIV_PROC` 在 `tell_vfs` 后立即 `sys_clear` 直毁（`361-369`），不等待 `VFS`，`!PRIV_PROC` 则在 `exit_restart` 的 `sched_stop` 后 `sys_clear`（`447-452`），二者互斥（`PRIV_PROC` 仅一次 `sys_clear`）。

### 1.3 为什么需要僵尸：`exit` 与 `wait4` 的生产–消费

`exit` 仅生产退出码（`mp_exitstatus`/`mp_sigstatus`，`mproc.h:25-26` `char`），`wait4` 才消费（`W_EXITCODE` + `rusage` 累计，`forkexit.c:712-723` `parent->mp_child_utime/stime +=`）。若 `exit` 直接 `cleanup`（`procs_in_use--` + `mp_pid=0`），`wait4` 的 `tell_parent` 无源。僵尸（`ZOMBIE` 0x04 / `TRACE_ZOMBIE` 0x10000，`mproc.h:88/101`）是"已死未收"的中间态，`procs_in_use` 仍计数，`mp_pid` 仍保留，`TOLD_PARENT`（`0x40`）避免二次通知（`forkexit.c:687-690`）。

`zombify` 的两级僵尸（`forkexit.c:593-624`）正是生产侧的细化：`tracer != NO_TRACER && tracer != parent → TRACE_ZOMBIE`（先通知 tracer 伪父），否则 `ZOMBIE`（通知真父），`!wait_test(tracer) → return` 否则 `tell_tracer`，`check_parent` 再对真父重试。

### 1.4 为什么收养：`INIT` 的 `NEW_PARENT` 记忆

进程退出时其子的 `mp_parent` 指向已死槽位（`proc_nr` 已 `EXITING`），`disinherit` 将其重赋 `INIT_PROC_NR 11`（`forkexit.c:396`，`main.c:194` `INIT` 父为自身，`INIT` 永活），若子正 `VFS_CALL` 则 `NEW_PARENT` 记忆（`402-403` `VFS_CALL → NEW_PARENT`），使 `handle_vfs_reply` 的 `VFS_PM_FORK_REPLY` 分支不误回已死父（`main.c:392-393` `!new_parent → reply(parent)`，05 `ipc/vfs.rs:298` `reply_to_new_parent` 抑制）。若子已 `ZOMBIE` 则 `check_parent(..., TRUE)` 立即对 `INIT` 重试 `tell_parent` 或 `SIGCHLD`（`406-407`），`SIGHUP` 在 `procgrp !=0`（会话 Leader，`298` `mp_pid == mp_procgrp`）时广播至进程组（`412` `check_sig(-procgrp, SIGHUP)`，`signal.c:568`）。

### 1.5 退出与事件的衔接：`publish_event` 串在两阶段之间

`handle_vfs_reply` 的 `VFS_PM_EXIT/CORE → publish_event`（`main.c:365`）串在 `exit_proc` 的 `tell_vfs` 与 `exit_restart` 之间，使 `EventRegistry::resume_event` 的 `exit_restart` 分支（06 §2.3 的 `resume_event` 尾部 `exit_restart` vs `restart_sigs`）与本章 `exit_restart` 同源：`publish_event` 的 `EXIT` 事件经串行投递至订阅者（如 `DS`），订阅者 `PROC_EVENT_REPLY` 后 `resume_event` 的 `exit_restart` 分支才 `sched_stop`→`sys_clear`→`vm_exit`（`forkexit.c:418-457`）。`TRACE_EXIT` 的 `ptrace(T_EXIT)` 唤醒在 `exit_restart` 末尾（`459-464` `reply(tracer, OK)`），`TOLD_PARENT→cleanup` 的 `procs_in_use--` 与 07 的 `++` 成对。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `mp_flags &= (IN_USE|VFS_CALL|...)`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `do_exit`。** Linux `do_exit` → `exit_state = EXIT_ZOMBIE` → `forget_original_parent` → `find_new_reaper`（`INIT` 或最近 `PR_SET_CHILD_SUBREAPER`）→ `exit_notify` → `schedule`，`wait4` 的 `do_wait` 消费 `EXIT_ZOMBIE→EXIT_DEAD`。Minix3 的 `zombify`→`check_parent`→`disinherit` 的 `INIT` 收养与 `find_new_reaper` 同源，但 Linux 以 `task_struct->real_parent` 单指针 + `children` 链表，Minix3 以 `mp_parent` 槽位 + 全表扫描（`for rmp=0..NR_PROCS`，`388`），Rust 侧 `Guardianship` 双监护使 `tracer` 伪父显式化（`mproc/guardianship.rs`）。

**FreeBSD `proc` 僵尸链表。** FreeBSD 以 `zombproc` 链表链死未收进程，Minix3 以 `ZOMBIE` 位 + `procs_in_use` 计数，Rust 侧 `Lifecycle::Zombie` 枚举使 `is_zombie()` 与 `is_exiting()` 互斥，`TOLD_PARENT` 避免二次通知（`forkexit.c:687-690` 的 `TOLD_PARENT` 位）。

**Redox `Scheme` 资源释放。** Redox 以 `FileDescription` 的 `close` 经 `Scheme::close` 释放 `fproc` 类资源，Minix3 以 `VFS_PM_EXIT` 经 `tell_vfs` 异步释放 `fproc`（`05-stage-vfs`），二者同为"VFS 侧 `fproc` 清理异步于 PM 侧 `mproc` 标记"，Rust 侧 `VfsCall::Exit` + `EventRegistry::publish` 同 `Scheme::close` 的异步。

**`seL4` `TCB` 回收。** `seL4` 需手动 `retype` `TCB`/`CNode` 回收，Minix3 的两阶段 `sys_clear`/`vm_exit` 在 `exit_proc`（`PRIV_PROC` 直毁）与 `exit_restart`（`!PRIV_PROC`）的互斥分属，与 `seL4` 手动回收同为显式解绑，Rust 侧 `ExitOrchestrator::first_half`/`second_half` 使两阶段显式化。

**结论（本章的设计基线）。** 把 `do_exit` 的 `PRIV_PROC→SIGKILL` 门、`exit_proc` 的 9 步（含 `VFS_CALL` 保留与 `EXITING` 进入）、`exit_restart` 的 5 步（`sched_stop`→`sys_clear`→`vm_exit`）、`zombify` 的两级僵尸与 `disinherit` 的 `INIT` 收养+`NEW_PARENT`，改写为"显式协调器 `handle_exit`→`exit_proc`→`zombify`→`disinherit` + 状态机 `Lifecycle`/`Guardianship`/`BlockState` + 事件 `publish` + `VFS_CALL` 延续"。

### 1.7 小结

1. **为什么两阶段**——`sys_stop`→`vm_willexit`→`VFS_PM_EXIT`→`EXITING`→`publish_event`→`exit_restart` 的 `sched_stop`→`sys_clear`→`vm_exit`，`VFS_PM_EXIT` 为分界，`VFS` 需先取消驱动拷贝再 `proc` 消失。
2. **为什么直毁**——`PRIV_PROC` 块设备驱动若等待 `VFS` 则死锁，`exit_proc` 立即 `sys_clear`，`!PRIV_PROC` 在 `exit_restart` 再 `sys_clear`，二者互斥。
3. **为什么僵尸**——`exit` 生产 `ZOMBIE`/`TRACE_ZOMBIE`，`wait4` 经 `tell_parent/tell_tracer` 消费，`TOLD_PARENT` 防重，`procs_in_use` 仍计数。
4. **为什么收养**——`disinherit` 将 `mp_parent==exiting` 重赋 `INIT`，`VFS_CALL→NEW_PARENT` 记忆使 `VFS_PM_FORK_REPLY` 不误回已死父，`ZOMBIE→check_parent` 对 `INIT` 重试。
5. **为什么事件衔接**——`handle_vfs_reply` 的 `publish_event` 串在两阶段之间，`resume_event` 的 `exit_restart` 分支与本章 `exit_restart` 同源。

下一章逐行分析 C 的 `do_exit`/`exit_proc`/`exit_restart`/`zombify`；第 3 章给出 Rust 的显式协调器与状态机。

---

## 2 C 源码分析

### 2.1 `do_exit` 序言：`PRIV_PROC → SIGKILL` 门与 `SUSPEND`（forkexit.c:245-262）

```c
if(mp->mp_flags & PRIV_PROC) {            // forkexit.c:253 PRIV_PROC 0x02000（mproc.h:98）
    printf("PM: system process %d (%s) tries to exit(), sending SIGKILL\n",
        mp->mp_endpoint, mp->mp_name);    // 254-255
    sys_kill(mp->mp_endpoint, SIGKILL);   // 256 sys_kill 9（signal.h:57 SIGKILL 9）
} else {
    exit_proc(mp, m_in.m_lc_pm_exit.status, FALSE /*dump_core*/); // 259 m_lc_pm_exit.status 来自 ipc.h:1812 mess_pm_lc_exit
}
return(SUSPEND);                          // 261 can't communicate from beyond the grave（main.c:106 ReplyLater 的永不回复子类，dispatcher.rs:45 NoReply）
```

`PRIV_PROC` 系统服务禁止 `exit`（应由 `SEF` 重启），`sys_kill(SIGKILL)` 9 为 `signal.c:384` `sig_proc` 的 `SIGS_IS_LETHAL` 快捷路径，11 章详述；`!PRIV_PROC` 才 `exit_proc`，恒 `SUSPEND`（`com.h:1151 -998`，`dispatcher.rs:45` `ReplyLater` 的 `NoReply` 子类——`do_exit` 永不回复，`do_fork` 的 `SUSPEND` 由 `handle_vfs_reply` 异步回复，`do_exit` 无后续回复者）。

### 2.2 `exit_proc` 首段：`dump_core` 双抑制→`procgrp` 记忆→`ALARM_ON`→`sys_times` 记账（forkexit.c:267-310）

```c
if (dump_core && rmp->mp_realuid != rmp->mp_effuid) dump_core = FALSE; // 285-286 setuid 不 dump
if (dump_core && (rmp->mp_flags & PRIV_PROC)) dump_core = FALSE;       // 291-292 PRIV_PROC 不 dump（VFS 侧 fproc 已直毁）
proc_nr = (int)(rmp - mproc);             // 294 槽号（rmp-mproc 指针差，03 table.rs 的 UserSlot::new(slot)）
proc_nr_e = rmp->mp_endpoint;             // 295 endpoint（跨服务身份，02）
procgrp = (rmp->mp_pid == mp->mp_procgrp) ? mp->mp_procgrp : 0; // 298 会话 Leader 的 procgrp 记忆（getset.c 会话语义，A-13）
if (rmp->mp_flags & ALARM_ON) set_alarm(rmp, (clock_t)0); // 301 熄灭定时器（alarm.c:344 set_alarm 0，14 章）
if((r=sys_times(proc_nr_e, &user_time, &sys_time, NULL, NULL)) != OK) // 306-307 sys_times（kernel/system/do_times.c）
    panic("exit_proc: sys_times failed: %d", r);
rmp->mp_child_utime += user_time;         // 308-309 累计至 dead child（POSIX 禁在 wait 前回父，forkexit.c:303-305 注释）
rmp->mp_child_stime += sys_time;
```

`dump_core` 双门与 `getset.c` 的 `TAINTED`（`mproc.h:103`）语义同源（setuid 进程不 dump，防信息泄漏）；`procgrp` 记忆为 `412` 的 `SIGHUP` 广播前置（`check_sig(-procgrp, SIGHUP)`，`signal.c:568`）。

### 2.3 `exit_proc` 中段：`PROC_STOPPED` 强制→`vm_willexit`→`INIT/VFS` 特例→`VFS_PM_EXIT` 投递→`PRIV_PROC` 直毁（forkexit.c:311-369）

```c
/* 311-330 PROC_STOPPED 强制（sys_stop） */
if (!(rmp->mp_flags & PROC_STOPPED)) {    // 326
    if ((r = sys_stop(proc_nr_e)) != OK) panic("sys_stop failed: %d", r); // 327-328 sys_stop 9（kernel/system/do_stop.c）
    rmp->mp_flags |= PROC_STOPPED;        // 329
}                                         // 330 326 注释 TODO: make kernel discard delayed calls upon forced stops

if((r=vm_willexit(proc_nr_e)) != OK) {    // 332-334 vm_willexit（vm.h，02-stage-vm/18 对端，告 VM 该进程将退出）
    panic("exit_proc: vm_willexit failed: %d", r);
}

if (proc_nr_e == INIT_PROC_NR) {          // 336-341 INIT 特例（INIT_PROC_NR 11，com.h:72）
    printf("PM: INIT died with exit status %d; showing stacktrace\n", exit_status);
    sys_diagctl_stacktrace(proc_nr_e);    // 339 kernel diagctl 栈回溯
    return;                               // 340 INIT 退出不走 VFS（VFS 侧无 INIT fproc）
}
if (proc_nr_e == VFS_PROC_NR) {           // 342-345 VFS 特例（VFS_PROC_NR 1）
    panic("exit_proc: VFS died: %d", r);  // 344 VFS 死亡即 panic（VFS 为 PM 的 fproc 权威，PM 无法独立）
}

memset(&m, 0, sizeof(m));                 // 350
m.m_type = dump_core ? VFS_PM_DUMPCORE : VFS_PM_EXIT; // 351 com.h:524-525 0x904/0x905
m.VFS_PM_ENDPT = rmp->mp_endpoint;        // 352 m7i1 退出进程 endpoint（05 §2.1）
if (dump_core) {                          // 354-357
    m.VFS_PM_TERM_SIG = rmp->mp_sigstatus; // 355 m7i2 终止信号
    m.VFS_PM_PATH = rmp->mp_name;         // 356 m7p1 core 路径（proc 名字）
}
tell_vfs(rmp, &m);                        // 359 VFS_CALL 置退出进程（utility.c:123-139 三段式，05 §2.2）

if (rmp->mp_flags & PRIV_PROC) {          // 361-369 PRIV_PROC 直毁（块设备死锁避免）
    if((r= sys_clear(rmp->mp_endpoint)) != OK) // 367 sys_clear（kernel/system/do_clear.c，01-stage-kernel）
        panic("exit_proc: sys_clear failed: %d", r);
}
```

`PROC_STOPPED` 强制的 `326` 注释 `TODO: make kernel discard delayed calls` 与 `main.c:80-82` 的 `EXITING` 丢弃前置（退出中进程的延迟调用 `continue`）呼应，`DELAY_CALL`（`mproc.h:102`）的 `SIGSNDELAY` 语义归 13；`INIT`/`VFS` 特例的 `return`/`panic` 为服务树根的生死契约（`INIT` 永活，`VFS` 与 PM 互生）。

### 2.4 `exit_proc` 尾段：`EXITING` 保留位→`zombify`→`disinherit` 收养与 `SIGHUP`（forkexit.c:371-413）

```c
rmp->mp_flags &= (IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED); // 374 仅 5 位保留
rmp->mp_flags |= EXITING;                 // 375 进入 EXITING 中间态（mproc.h:91 0x20）
rmp->mp_exitstatus = (char) exit_status;  // 379 mp_exitstatus char 截断（mproc.h:25）
if (!dump_core) zombify(rmp);             // 384-385 非 core 立即僵尸化（core 路径推迟至 exit_restart 444-445）
for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) { // 388 disinherit 全表扫描
    if (!(rmp->mp_flags & IN_USE)) continue; // 389
    if (rmp->mp_tracer == proc_nr) {      // 390-393 tracer 死亡
        tracer_died(rmp);                 // 392 tracer_died: NO_TRACER 恢复 + !EXITING→SIGKILL 级联 / TRACE_ZOMBIE→ZOMBIE + check_parent
    }
    if (rmp->mp_parent == proc_nr) {      // 394-408 亲子收养
        rmp->mp_parent = INIT_PROC_NR;    // 396 INIT 收养（main.c:194 INIT 父为自身）
        if (rmp->mp_flags & VFS_CALL)     // 402-403 VFS_CALL → NEW_PARENT 记忆（mproc.h:96 0x00800，05 take_vfs_call 载荷）
            rmp->mp_flags |= NEW_PARENT;
        if (rmp->mp_flags & ZOMBIE)       // 406-407 已僵尸则立即对 INIT 重试 check_parent
            check_parent(rmp, TRUE /*try_cleanup*/); // 407 check_parent: wait_test→tell_parent→cleanup vs SIGCHLD
    }
}
if (procgrp != 0) check_sig(-procgrp, SIGHUP, FALSE /* ksig */); // 412 会话 Leader 的 hangup（signal.c:568 check_sig，-procgrp 为进程组广播，SIGHUP 1）
```

逐行要点：

- `374` 保留位 `IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED`（`IN_USE` 仍计数 `procs_in_use`，`VFS_CALL` 保留至 `handle_vfs_reply` 后、`PRIV_PROC` 保留至 `exit_restart` 的 `sys_clear` 分支、`TRACE_EXIT` 保留至 `exit_restart` 末尾 `reply(tracer,OK)`、`PROC_STOPPED` 保留至 `sched_stop` 前）；
- `zombify` 仅非 core 立即（`384-385`），core 推迟至 `exit_restart` `444-445` 的 `!(TRACE_ZOMBIE|ZOMBIE|TOLD_PARENT) → zombify`（`VFS_PM_DUMPCORE` 的 `fproc` 需先 `core` 完成）；
- `disinherit` 先 `tracer_died` 后 `parent`（`390-393` 在 `394-408` 前），`NEW_PARENT` 仅在 `VFS_CALL` 时记忆（`402-403`），与 05 的 `take_vfs_call` 清位成对（`handle_vfs_reply` 的 `VFS_PM_FORK_REPLY` 分支 `new_parent` 抑制 `reply(parent)`）；
- `SIGHUP` 仅在 `procgrp !=0`（会话 Leader，`298` 记忆）时广播（`412`，`signal.c:568` `check_sig`，`-procgrp` 为组广播，`SIGHUP 1`）。

### 2.5 `exit_restart`（forkexit.c:418-469）

```c
if((r = sched_stop(rmp->mp_scheduler, rmp->mp_endpoint)) != OK) { // 425 sched_stop（schedule.c:55，16 章，失敗仅 printf 432-433）
    printf("PM: The scheduler did not want to give up scheduling %s, ret=%d.\n", rmp->mp_name, r);
}
rmp->mp_scheduler = NONE;                 // 441 NONE 0（com.h:61，01-stage-kernel）
if (!(rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE | TOLD_PARENT))) // 444-445 core 路径首次僵尸化（!dump_core 已 zombify，core 首次）
    zombify(rmp);
if (!(rmp->mp_flags & PRIV_PROC)) {       // 447-452 !PRIV_PROC → sys_clear（与 exit_proc 361-369 互斥，PRIV_PROC 仅一次 sys_clear）
    if((r=sys_clear(rmp->mp_endpoint)) != OK) panic("exit_restart: sys_clear failed: %d", r);
}
if((r=vm_exit(rmp->mp_endpoint)) != OK) { // 455-457 vm_exit（02-stage-vm，释放页表，panic 守卫）
    panic("exit_restart: vm_exit failed: %d", r);
}
if (rmp->mp_flags & TRACE_EXIT) {         // 459-464 TRACE_EXIT → reply(tracer, OK)（mproc.h:100 0x08000，trace.c:276，18 章）
    mproc[rmp->mp_tracer].mp_reply.m_pm_lc_ptrace.data = 0;
    reply(rmp->mp_tracer, OK);
}
if (rmp->mp_flags & TOLD_PARENT)          // 467-468 已收割则 cleanup（forkexit.c:795-806 procs_in_use-- + mp_pid=0）
    cleanup(rmp);
```

`441` `scheduler=NONE` 在 `sched_stop` 后（`sched_stop` 前 `scheduler` 仍为原 `SCHED`/`NONE`，`sched_stop` 需原 `scheduler` 值）；`TOLD_PARENT` 再 `cleanup`（`467-468`）与 10 的 `tell_parent` 的 `TOLD_PARENT` 避免二次通知（`forkexit.c:687-690`）呼应，`procs_in_use--` 与 07 的 `++` 成对。

### 2.6 `zombify` / `check_parent` / `tracer_died` / `cleanup`（forkexit.c:593-624/626-665/759-790/795-806）

`zombify`（`593-624`）`TRACE_ZOMBIE|ZOMBIE` 互斥 `panic`（`603-604`）+ `tracer != NO_TRACER && tracer != parent → TRACE_ZOMBIE` 否则 `ZOMBIE`（`607-620`）+ `!wait_test(tracer) → return` 否则 `tell_tracer`（`613-616`，`wait_test` 为 `wait.rs` `WaitState::is_waiting`）+ `check_parent(FALSE)`（`623`）。

`check_parent`（`626-665`）`p_mp = mproc[child->mp_parent]` → `EXITING → /*do nothing*/`（`646-650` `child of dead parent` 空窗）→ `wait_test → tell_parent` + `try_cleanup && !(VFS_CALL|EVENT_CALL) → cleanup`（`651-660`，`VFS_CALL|EVENT_CALL` 仍阻塞时不 `cleanup`，06 的 `resume_event` 待串行完成） 否则 `sig_proc(SIGCHLD)`（`663`，`signal.c:384`，11 章）。

`tracer_died`（`759-790`）`mp_tracer = NO_TRACER` → `mp_flags &= ~TRACE_EXIT`（`768-769`）→ `!EXITING → SIGKILL` 级联（`775-777` `sig_proc(SIGKILL)`，`11`）→ `TRACE_ZOMBIE → ZOMBIE + check_parent(TRUE)`（`784-788`）。

`cleanup`（`795-806`）`mp_pid=0` + `mp_flags=0` + `child_utime/stime=0` + `procs_in_use--`（`801-806`，`table.rs:release_slot` 对应 `fork` 的 `alloc_slot` 的 `++`）。

### 2.7 主循环与 VFS 回复的衔接（main.c:80-82/365）

`main.c:80-82` `EXITING` 丢弃（`if (mp->mp_flags & EXITING) continue`，退出中进程的延迟调用 `DELay_CALL` 直接丢弃，`forkexit.c:326` 注释 `TODO`）；`main.c:365` `publish_event` 的 `EXIT/CORE → publish_event` 提前 `return` 不走尾部 `restart_sigs`（06 §2.7），`forkexit.c:116` 的 `mp_eventsub == NO_EVENTSUB` 断言（`fork`/`srv_fork` 子无游标，本章 `exit_restart` 的 `TOLD_PARENT→cleanup` 后 `procs_in_use--` 使 `find_free_slot` 可复用）。

### 2.8 对端视角

- **VM 侧**：`vm_willexit`（`forkexit.c:332`，`02-stage-vm` 告 VM 将退出）与 `vm_exit`（`455`，释放页表），本章只写 PM 侧 `tell_vfs` 前后 `vm_*` 调用点。
- **VFS 侧**：`VFS_PM_EXIT 0x904`/`DUMPCORE 0x905`（`com.h:524-525`）的 `fproc` 释放，`VFS_PM_EXIT_REPLY 0x984`/`CORE_REPLY 0x985`（`com.h:537-538`）经 `handle_vfs_reply` 触发 `publish_event`（`main.c:365`），本章只写 `tell_vfs` 投递（`350-359`）。

### 2.9 不变式分类

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `PRIV_PROC` 直毁 vs 等待的死锁避免 | `forkexit.c:361-369` 注释 | `PRIV_PROC` 块设备驱动 `VFS` 正等待 | 设计约束（P1 未标注即偏差） |
| `EXITING` 丢弃 | `main.c:80-82` | 退出中 `DELAY_CALL` 残留 | 可恢复（`continue`） |
| `VFS_CALL` 保留至 `handle_vfs_reply` | `forkexit.c:374` + `main.c:328` | `EXITING` 与 `VFS_CALL` 同时置位 | 不变式（05 尾部 `IN_USE|EXITING==IN_USE` 不走） |
| `ZOMBIE|TRACE_ZOMBIE` 互斥 `panic` | `forkexit.c:603-604` | 已僵尸再 `zombify` | 不可恢复 |
| `TOLD_PARENT` 防重 | `forkexit.c:687-690` | `tell_parent` 二次通知 | 不可恢复（`panic` 守卫） |
| `NEW_PARENT` 跨 `fork→handle_vfs_reply` | `forkexit.c:402-403` | `VFS_CALL` 时 `NEW_PARENT` 记忆 | 不变式（05 抑制 `reply(parent)`） |
| `cleanup` `procs_in_use--` | `forkexit.c:805` | `TOLD_PARENT` 再 `cleanup` | `procs_in_use` 与 `fork` `++` 成对 |

---

## 3 Rust 设计决策

Rust 改写遵循"显式协调器 + 状态机枚举 + 双监护 + 事件发布"的 8 决策，保留 C 的 9+5 步时序，但用类型系统使 `EXITING/ZOMBIE/TRACE_ZOMBIE/TOLD_PARENT` 互斥与 `NEW_PARENT` 载荷显式化。以下决策对应 `.design/09-design.v1.md` 的 D1–D8。

### D1：`do_exit` 的 `PRIV_PROC → SIGKILL` 门

`Privilege::is_kernel()` 首检 `sig_proc(SIGKILL)`（`forkexit.c:253-256`），`handle_exit` 返 `NoReply`（`dispatcher.rs:45` `SUSPEND` 的永不回复子类，`main.c:106` 的 `ReplyLater` 区分 `do_fork` 的异步回复 vs `do_exit` 的永不回复）。

### D2：`exit_proc` 的 9 步

`ExitOrchestrator::first_half` 9 步与 `forkexit.c:267-413` 同序同条件，`dump_core` 双抑制（`realuid != effuid` 与 `PRIV_PROC` 各 `FALSE`）、`procgrp` 记忆（`mp_pid == procgrp → procgrp else 0`）、`ALARM_ON → timer=None`、`sys_times → child_utime/stime +=`（`Clock` 占位）、`PROC_STOPPED` 强制→`BlockState::stopped=true`、`vm_willexit` 占位、`INIT/VFS` 特例、`VfsCall::Exit/DumpCore`+`tell_vfs`、`PRIV_PROC→sys_clear`、 `EXITING` 保留位、`zombify`、`disinherit`、`SIGHUP`。

### D3：`Lifecycle` 互斥枚举（`A-2`）

`mproc/lifecycle.rs:27` `Lifecycle::{Unused,Running,Exiting,Zombie,TraceZombie,ToldParent}` 互斥（`mproc.h:91-101` 位→枚举），`Exiting` 载荷 `exit_code: i8, sig_status: i8` 对应 `mp_exitstatus/sigstatus` 的 `char` 截断（`forkexit.c:379`）。

### D4：`Guardianship` 双监护 + `NEW_PARENT` 载荷（`A-12`）

`mproc/guardianship.rs:26` 双监护 + `mproc/block.rs:52` `VfsCall{reply_to_new_parent}` 载荷，`disinherit` 的 `INIT` 收养与 `NEW_PARENT` 记忆成对（`forkexit.c:402-403` 与 05 `take_vfs_call` 清位）。

### D5：`zombify` 两级僵尸（`A-2`）

`Lifecycle::TraceZombie vs Zombie` 枚举分支（`forkexit.c:607-620`），`wait_test(tracer)` 决定 `tell_tracer` 或 `return`。

### D6：`check_parent` 的 `wait_test → tell_parent → cleanup` vs `SIGCHLD`

`WaitState` 与 `signal::sig_proc` 占位（11），`TOLD_PARENT` 由 `Lifecycle::ToldParent` 保证。

### D7：`disinherit` 的 `INIT` 收养与 `SIGHUP`

`INIT_PROC_NR 11` 收养，`NEW_PARENT` 仅 `VFS_CALL` 时记忆，`SIGHUP` 经 `check_sig` 广播（`signal.c:568`）。

### D8：`exit_restart` 的 5 步（`A-3`）

`sched_stop → scheduler=NONE` → `zombify(core)` → `sys_clear(!PRIV_PROC)` → `vm_exit` → `TRACE_EXIT→reply(tracer,OK)` → `TOLD_PARENT→cleanup`，`procs_in_use--` 与 07 `++` 成对。

---

## 4 实现详解

### 4.1 `os/servers/pm/src/exit.rs`

`handle_exit(caller, status, table, transport, event) -> ReplyIntent` 首检 `PRIV_PROC→SIGKILL`，`exit_proc` 9 步（`ALARM_ON` 熄灭→`sys_times` 累计→`PROC_STOPPED` 强制→`vm_willexit`→`INIT/VFS` 特例→`VfsCall::Exit/DumpCore`+`tell_vfs`→`sys_clear` 直毁→`EXITING`+`zombify`→`disinherit`+`SIGHUP`），`exit_restart` 5 步（`sched_stop`→`NOME`→`zombify`→`sys_clear`→`vm_exit`→`TRACE_EXIT`→`cleanup`），`zombify`/`check_parent`/`tracer_died`/`disinherit`/`cleanup` 与 `SIGHUP`。

### 4.2 `os/servers/pm/src/mproc/{lifecycle,guardianship,block,wait}.rs`

`Lifecycle` 5 变体、`Guardianship` 双监护、`BlockState` 保留位、`WaitState` `WAITING`。

### 4.3 `os/servers/pm/src/ipc/{vfs,event}.rs`

`VfsCall::Exit/DumpCore` + `tell_vfs`（`VFS_CALL` 置退出进程）+ `EventRegistry::publish_event`（EXIT 事件，`main.c:365`）。

### 4.4 `os/servers/pm/src/signal.rs`

`sig_proc(SIGKILL/CHLD/HUP)` 占位（11）。

### 4.5 `os/servers/vm/src/vm.rs`

`VmWillexit/VmExit` 占位（02-stage-vm）。

### 4.6 不变量表（同 §2.9，Rust 表达→panic/Reply/SUSPEND）

---

## 5 测试矩阵

### 5.1 `exit.rs`（`do_exit`/`exit_proc`/`exit_restart`/`zombify`/`check_parent`/`disinherit`）

- `test_do_exit_priv_proc`：`PRIV_PROC→SIGKILL` vs `exit_proc`
- `test_exit_proc_dump_core_suppressed`：`realuid != effuid` 与 `PRIV_PROC` 双抑制
- `test_exit_proc_vfs_call`：`VFS_PM_EXIT` 投递且 `VFS_CALL` 保留
- `test_exit_priv_sys_clear`：`PRIV_PROC` 立即 `sys_clear`
- `test_exit_lifecycle_exiting`：`EXITING` 保留位
- `test_zombify_trace_zombie`：`TRACE_ZOMBIE` vs `ZOMBIE`
- `test_disinherit_new_parent`：`VFS_CALL→NEW_PARENT`
- `test_exit_restart_priv`：`PRIV_PROC` 不二次 `sys_clear` 等

### 5.2 测试总数声明（截至日期）

`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **166 passed**（含 07/08 的 `fork.rs` 7 + `mproc/fork.rs` 18），`exit.rs` 新增 10 项后将至 **176/108**。

---

## 6 过渡

本篇在主循环 `PM_EXIT` 与 `handle_vfs_reply` 的 `VFS_PM_EXIT/CORE → publish_event → exit_restart` 之间的位置；`disinherit` 的 `INIT` 收养为 10 的 `wait4` 的 `children` 扫描与 11 的 `SIGHUP` 广播的前置；`TOLD_PARENT→cleanup` 的 `procs_in_use--` 与 07 的 `++` 成对。

## 7 参见

- C 源：`minix3/minix/servers/pm/forkexit.c:242-469/590-807`（`do_exit`/`exit_proc`/`exit_restart`/`zombify`）、`minix3/minix/include/minix/com.h:524-525`（`VFS_PM_EXIT`）、`minix3/minix/servers/pm/main.c:365`（`publish_event`）
- 设计契约：`.design/09-design.v1.md`（D1–D8 与行为契约表）、`.design/09-outline.v1.md`、`.design/09-outline-review.v1.md`
- PM 阶段文档：03-mproc-table.md（`procs_in_use`）、05-vfs-interaction.md（`tell_vfs`）、06-event-subscription.md（`publish_event`）、02-mproc-struct.md（`Lifecycle`）、10-pm-wait.md（`wait_test`）
- 对端实现：02-stage-vm（`vm_willexit/vm_exit`）、05-stage-vfs（`VFS_PM_EXIT`）
- 内核接口：01-stage-kernel（`sys_stop/sys_clear/sys_times`）、16-scheduling.md（`sched_stop`）、18-trace.md（`TRACE_EXIT`）
