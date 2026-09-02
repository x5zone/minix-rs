# 10 — PM 协议：`service_pm` 的 `VFS_PM_*` 12 请求与 `pm_fork` 的 `filp` 共享及 `pm_exit` 的 `free_proc` 级联

本文讲清 VFS 如何在 `VFS_PM_RQ_BASE 0x900` 的 `&~0x7f` 前缀守门、`VFS_PM_FORK/SRV_FORK` 的 `okendpt` 与 `PID_FREE` 双守门、`fproc[childno]=fproc[parentno]` 的 `fp_lock` 保留与 `fp_filp[i]→filp_count++` 共享与 `dup_vnode(fp_rd/wd)` 及 `FP_NOFLAGS` 重置的约束下，以 `service_pm` 的 `立即(7)→延期(4)→worker_start(PM_WORK)` 与 `service_pm_postponed` 的 `VFS_PM_EXEC/EXIT/DUMPCORE/UNPAUSE` 四分支及 `pm_fork` 的 `copy_fproc + SRV_FORK 的 setuid/setgid` 追加建立 `PM → VFS` 的非 `call_vec` 控制面，并以 `free_proc` 的 `close_fd×OPEN_MAX + put_vnode(rd/wd) + FP_EXITING 级联` 与 `pm_setuid/setgid/setgroups/setsid` 的 `okendpt→slot` 凭证注入为进程生命周期提供可逆回收。

前置阅读：`03-fproc-table.md`（`isokendpt` 三守卫与 `PID_FREE` 双哨兵）、`09-main-loop.md`（`Route::Pm` 的 `main:91` 守门与 `is_notify` 的 `FP_PENDING` 优先级）、`08-worker-thread.md`（`worker_start(...,NULL)` 的 `PM_WORK` 协程与 `can_start` 守门）。

> 本章不讲什么：
> - `syscall` 服务面 `open/read/write` 等 64 调用—— `14~31`（`call_vec` 的消费方，09 的 `Syscall` 分支）
> - `m_comm` 队列与 `send_work` 的 `VMNT_CALLBACK` 及 `c_cur_reqs` 窗口—— `11-fs-comm.md`（`transid` 路由的对端）
> - `bdev/cdev/sdev_reply` 的驱动 `RS` 前缀与 `worker_signal` —— `20/21/22`（`09` 的 `Bdev/Cdev/Sdev` 分支消费方）
> - `select` 的 `CLOCK` 定时器与 `expire_timers` —— `23-select.md`（`NOTIFY` 的 `CLOCK` 分支）
> - 文件描述符的 `get_fd/close_fd` 单步与 `invalidate_filp` —— `14-filedes.md`（`free_proc` 的 `close_fd` 循环单步）
> - `dup_vnode/put_vnode` 的 `v_ref_count/v_fs_count` 双层及 `clean_refs` 的 `256` 阈值—— `05-vnode-table.md`（`pm_fork` 的 `dup` 与 `pm_exit` 的 `put` 的对端）
> - 内核 `sys_datacopy_wrapper` 的跨进程拷贝—— `99-global-concepts.md` + `../01-stage-kernel/18-syscall-copy.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 09 的 `Route::Pm` 八路优先级（`PM_PROC_NR==0` 的 `service_pm()` 短路）与 08 的 `worker_start(...,NULL)` 的 `PM_WORK` 延期（`FP_PM_WORK` 的待执行标记），能 `grep "VFS_PM_FORK\|pm_fork\|free_proc" minix3/minix/servers/vfs/misc.c` 的开发者。

### 1.1 为什么 PM 是进程生命周期的权威

Minix3 的 `PM` 是 `mproc` 的权威（`../04-stage-pm/00-pm-overview.md`），`VFS` 是 `fproc` 的权威。`fork/exit/exec/setuid` 的用户态发起在 `PM` 侧完成 `kernel` 的 `proc` 分配与 `mproc` 创建后，`PM` **必须**以 `VFS_PM_FORK/EXIT/EXEC…` 的 `ipc_send(VFS_PROC_NR, &m_out)` 通知 `VFS`，使 `fproc[childno]` 与 `mproc[childno]` 的 `endpoint` 对齐。`PM` 与 `VFS` 的 `NR_PROCS 256` 同界（`fproc.h:11` *must be the same as in the kernel*）使 `childno = _ENDPOINT_P(cproc)` 的槽号在两侧一致。

与 `call_vec[64]` 的用户进程 `VFS_BASE 0x100` 不同，`VFS_PM_RQ_BASE 0x900` 的 `&~0x7f` 前缀（`com.h:516`）将 `PM → VFS` 的 12 请求与 `VFS → FS` 的 `REQ_*` 的 `0x00` 前缀及 `VFS → VM` 的 `VM_*` 前缀在 `m_type` 域内不重叠，使 `main:91` 的 `who_e==PM` 守门可区分 `PM` 的控制面与 `FS` 的 `transid` 回复（`09` 的 `FsReply` 优先于 `Pm` 的 8 路排序依赖此不重叠）。

### 1.2 立即、延期、worker_start：三级调度

`service_pm:783` 的 `switch(call_nr)` 将 12 请求按“是否可阻塞目标进程”分三调度：

- **立即**（7 路：`SETUID/SETGID/SETGROUPS/SETSID/FORK/SRV_FORK/SETGROUPS`）：`okendpt → tfp->fields` 直接写 `fproc`，`ipc_send(PM, REPLY)` 同步回复。`FORK` 的 `pm_fork` 在此路（`864 pm_fork` 后 `m_type=FORK_REPLY` → `ipc_send`），因 `fork` 的 `copy_fproc` 只涉及内存拷贝，无 `SUSPEND` 风险。
- **延期**（4 路：`EXEC/EXIT/DUMPCORE/UNPAUSE`）：`isokendpt → slot → rfp=&fproc[slot] → worker_start(rfp,NULL,&m_in,FALSE)` 的 `PM_WORK` 标记（`worker.c:418 flags|=FP_PM_WORK`），`return` 不回复；`worker_main:270` 的 `FP_PM_WORK → w_m_in=fp_pm_msg → service_pm_postponed → flags&=~PM_WORK` 后 `ipc_send(PM, REPLY)`。延期缘于目标进程可能正 `Busy`（`08` 的 `can_start` 守门：`has_normal_work → FALSE`），`PM_WORK` 的队列使 `exec` 与 `read` 串行。
- **独立 worker**（1 路：`REBOOT`）：`worker_start(fproc_addr(PM_PROC_NR), pm_reboot, …)` 的 `PM_PROC_NR==0` 关联（`PM` 在 VFS 视角永 `idle`，注释 *PM is always idle*），`pm_reboot` 的 `do_sync + free_proc×256 + unmount_all` 独立于任何目标进程。

此“立即 vs 延期 vs 独立”与 09 的 `SUSPEND→ReplyLater` 的 `reviving` 优先同型：`PM_WORK` 的 `FP_PENDING` 排队在 `08` 的 `pending/busy/block_all` 三计数可观测，`service_pm_postponed` 的 `switch(job_call_nr)` 的 4 分支在 `17/23/21/22` 的 `revive` 消费前不新增 `reviving`。

### 1.3 fork 次主线：`pm_fork` 的四步共享

`pm_fork:577` 的 `VFS_PM_FORK` 是 `fork` 在 `VFS` 侧的次主线（plan §1.3），在 `service_pm` 的 `VFS_PM_FORK` 分支内 `864 pm_fork(pproc,cproc,cpid)` 后 `REPLY`：

1. **槽位守门**：`okendpt(pproc, &parentno)`（`592`）的 `isokendpt` 三守卫 + `childno=_ENDPOINT_P(cproc)` 的 `0≤childno<NR_PROCS` 范围守门（`599` `panic bogus child`）+ `fproc[childno].pid==PID_FREE` 的空闲双哨兵（`601` `panic in-use child`）。
2. **整表拷贝但锁保留**：`c_fp_lock = fproc[childno].fp_lock; fproc[childno]=fproc[parentno]; fp_lock=c_fp_lock`（`606`）的 `fp_lock` 属于槽位而非进程——`pm_fork` 的整表 `=` 覆盖后子进程仍用自己的 `fp_lock`（08 的 `ARCH A-6` 的槽位锁在 `fork` 的 `=` 中复用）。
3. **FD 共享**：`for(i=0; i<OPEN_MAX; i++) if(cp->fp_filp[i]) cp->fp_filp[i]->filp_count++`（`617`）的 `filp_count` 递增在 `04-filp-table.md` 的 `FsfFlags::PENDING` 可观测，`cloexec_set` 的位图在 `14-filedes.md` 的 `FD_CLOEXEC` 可观测。
4. **目录共享与标志重置**：`cp->fp_pid=cpid; cp->fp_endpoint=cproc;`（`620`）的 `PID_FREE→cpid` 的存活标记 + `dup_vnode(cp->fp_rd/wd)` 的 `v_ref_count++`（`632` `05` 的 `VnodeTable::dup`）+ `cp->fp_flags=FP_NOFLAGS`（`629`）的 `REVIVED/PENDING/PM_WORK/SESLDR` 清零。

`SRV_FORK` 的 `pm_fork` 后 `869 pm_setuid(cproc,reuid,reuid); pm_setgid(cproc,regid,regid)` 的追加使服务进程的 `uid/gid` 在 `fork` 后立即注入，避免服务进程继承 `PM` 的 `0` 身份。

### 1.4 exit 的级联回收：`free_proc` 的 `FP_EXITING` 分水岭

`pm_exit:713` 的 `free_proc(FP_EXITING)` 在 `service_pm_postponed:708` 的 `VFS_PM_EXIT` 分支内执行。`free_proc:639` 以 `flags & FP_EXITING` 分两阶段：

- **前 `FP_EXITING`（任何释放）**：`if(endpoint==NONE→panic already free)` + `if(is_blocked→unpause)` 的 `pipe` 解挂（`17` 的 `susp_count→reviving`）+ `for(i: close_fd(i,FALSE))` 的 `filp_count--` 与 `FD_CLOEXEC` 清除（`14` 的 `close_fd` 单步）+ `put_vnode(rd/wd)` 的 `v_ref_count--` 慢路径可能 `req_putnode`（`05` 的 `Vnode` 双层）。
- **后 `FP_EXITING`（仅 `exit`）**：`flags|=EXITING` + `unsuspend_by_endpt`（`17`）+ `dmap_unmap_by_endpt` / `smap_unmap_by_endpt`（`19`）+ `worker_stop_by_endpt`（`08`）+ `vmnt_unmap_by_endpt`（`06` 的 `m_fs_e==ep → mark_free → fs_cancel → invalidate_filp → put_vnode(mounted_on)`）+ `SESLDR && tty!=0 → revoke tty` 的 `for(rfp: tty==dev → tty=0; for(f: S_ISCHR && sdev==dev → cdev_close → FILP_CLOSED)` 的 tty 撤销级联。

`flags|=FP_EXITING` 的置位使 `pm_reboot` 的 `for(rfp: endpoint!=NONE → worker_set_proc(rfp)→free_proc(0))` 的 `FREE` vs `EXITING` 分化在 `free_proc` 内可区分：`reboot` 的第一轮 `free_proc(0)` 仅前阶段（保留 `vmnt_unmap` 给第二轮的 `unmount_all` 后），第二轮 `free_proc(0)` 的 `endpoint!=NONE` 再释放 FS 的 `vmnt`。

### 1.5 凭证的单写：`setuid/setgid/setgroups/setsid`

`pm_setuid:764` 的 `okendpt→slot → tfp->effuid=euid; realuid=ruid` 的 `uid_t` 双写与 `pm_setgid:726` 的 `effgid/rgid` 双写在 `02-fproc-struct.md` 的 `real_uid/eff_uid` 五字段可观测；`pm_setgroups:743` 的 `ngroups*sizeof(gid_t) ≤ sizeof(sgroups)` 的 `panic too much` 守门 + `sys_datacopy_wrapper(who_e, groups, SELF, rfp->sgroups, len)` 的跨进程拷贝在 `99` 的 `sys_datacopy_wrapper` 可观测；`pm_setsid:779` 的 `okendpt→flags|=SESLDR; tty=0` 的 `SESLDR` 会话首领与控台分离在 `02` 的 `SESLDR 0x004` 位可观测。

### 1.6 与其他 OS 的进程生命周期对照

- **Linux** `copy_process`：`copy_files(clone_flags & CLONE_FILES ? dup_fd : copy_fd)` 的 `CLONE_FILES` 标志决定 `filp` 的共享 vs 拷贝，`copy_fs` 的 `fs->users++` 使 `pwd/root` 的 `dentry` 共享，`copy_creds` 的 `prepare_creds → commit_creds` 的 RCU 凭证原子切换。`VFS` 的 `pm_fork` 的无 `CLONE_FILES` 分支（恒共享）与 `dup_vnode` 的 `users++` 同型，但 Linux 的 `clone` 标志在 `VFS` 以 `SRV_FORK` 的追加 `setuid/setgid` 分化。
- **Redox** `Scheme::fork`：`Redox` 的 `Scheme` 以 `Arc<RwLock<OpenFileDescription>>` 的 `open` 描述的 `Arc` 共享实现 `fork` 的 `fd` 共享，`dup_vnode` 的 `v_ref_count` 在 `Redox` 以 `Arc::clone` 的强引用计数显式；`pm_fork` 的 `fp_lock` 保留在 `Redox` 以 `RwLock::new(())` 的每进程新锁显式（锁属于槽位而非 `Arc`）。
- **seL4** `TCB` 派生：`seL4_TCB_Configure` 的 `cspace/vspace/ipc_buffer` 三配置使 `fork` 在 `seL4` 以 `Untyped_Retype` 的 `TCB` + `CNode` + `VSpace` 显式创建，`VFS` 的 `fproc[childno]=fproc[parentno]` 的整表拷贝在 `seL4` 以 `seL4_CNode_Copy` 的 `cap` 拷贝显式；`VFS` 的 `PID_FREE` 双哨兵在 `seL4` 以 `seL4_CapNull` 的空能力显式。

共同约束是“`fork` 的 `bss` 零初始化不应污染子进程的已分配身份”。Minix3 以 `c_fp_lock` 暂存 + 整表 `=` + 恢复的 `fp_lock` 保留使 `child` 的 `fp_lock` 不随 `parent` 的 `fp_lock` 状态污染，与 `Redox` 的每进程新 `RwLock` 同型但以 slot 复用实现。

### 1.7 小结

`PM` 协议是 `VFS` 的非 `call_vec` 控制面：`VFS_PM_RQ_BASE 0x900` 的 `&~0x7f` 前缀使 12 请求在 `m_type` 域内可区分，`service_pm` 的 `立即(7)→延期(4)→独立(1)` 的三级调度使 `PM_WORK` 的 `FP_PENDING` 排队在 08 的三计数可观测，`pm_fork` 的 `okendpt/PID_FREE` 双守门 + `fp_lock` 保留 + `filp_count++` + `dup_vnode(rd/wd)` + `FP_NOFLAGS` 的四步共享使 `fork` 的 `fd` 与目录的 `COW` 前共享可观测，`free_proc` 的 `close_fd×256 + put_vnode + EXITING 级联` 使 `exit` 的 `tty` 撤销与 `vmnt` 解绑可逆。下一节以 `com.h:513-584` 的 12 请求与 `main.c:668-920` / `misc.c:577-1010` 全文为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `com.h:513-584` 的 12+11 请求/回复族

`com.h:513 VFS_PM_RQ_BASE 0x900` 与 `514 VFS_PM_RS_BASE 0x980` 的 `0x80` 偏移使 `R→S` 的 `+0x80` 变换在 `service_pm:795 m_type=SETUID_REPLY` 可验证；`520-531` 的 `INIT(0)+SETUID(1)+SETGID(2)+SETSID(3)+EXIT(4)+DUMPCORE(5)+EXEC(6)+FORK(7)+SRV_FORK(8)+UNPAUSE(9)+REBOOT(10)+SETGROUPS(11)` 的 12 枚举与 `534-544` 的 `11` 个 `*_REPLY` 在 `service_pm:783 switch(call_nr)` 的 `case VFS_PM_*` 全覆盖；`547 VFS_PM_ENDPT m7_i1` 的 `endpoint` 与 `550 VFS_PM_SLOT m7_i2` 的槽号及 `551 VFS_PM_PID m7_i3` 的 `pid` 在 `01` 的 `VFS_PM_INIT` 握手已述，新增 `554 VFS_PM_EID/RID m7_i2/3` 的 `setuid/gid` 双 id、`558 VFS_PM_GROUP_NO/ADDR m7_i2/m7_p1` 的 `setgroups` 长度与指针、`562 VFS_PM_PATH/PATH_LEN m7_p1/m7_i2` 的 `exec/dumpcore` 路径、`577 VFS_PM_PENDPT/CPID/REUID/REGID m7_i2/3/4/5` 的 `fork` 四元组、`583 VFS_PM_TERM_SIG m7_i2` 的 `dumpcore` 信号在 `misc.c:903` 的 `csig==0→panic` 守门中可验证。

### 2.2 `service_pm:764-915` 的立即三调

`service_pm:777 fproc *rfp,slot; memset(m_out,0); switch(call_nr)` 的 `m_out` 清零与 `slave` 变量声明。`784 SETUID: proc_e=m_in.VFS_PM_ENDPT; euid=m_in.VFS_PM_EID; ruid=m_in.VFS_PM_RID; pm_setuid(proc_e,euid,ruid); m_type=SETUID_REPLY` 的 `okendpt→slot→tfp→eff/real` 双写。`800 SETGID` 同型。`816 SETSID: pm_setsid(proc_e); REPLY` 的 `SESLDR|tty=0`。`828 POSTPONED 4 路：VFS_PM_EXEC/EXIT/DUMPCORE/UNPAUSE 的 isokendpt→slot→rfp→worker_start(rfp,NULL,&m_in,FALSE)→return` 的 `PM_WORK` 延期 + 早期 `PM → service_pm:91` 的 `return` 不回复。`850 FORK/SRV_FORK: pproc=m_in.VFS_PM_PENDPT; cproc=m_in.VFS_PM_ENDPT; cpid=m_in.VFS_PM_CPID; pm_fork(pproc,cproc,cpid); REPLY; if(SRV_FORK) { m_type=SRV_FORK_REPLY; pm_setuid(cproc,reuid,reuid); pm_setgid(cproc,regid,regid); }` 的 `SRV_FORK` 追加。`876 SETGROUPS: group_no=m_in.VFS_PM_GROUP_NO; groups=m_in.VFS_PM_GROUP_ADDR; pm_setgroups(proc_e,group_no,groups); REPLY` 的 `m_p1` 指针跨进程拷贝。`893 REBOOT: worker_start(fproc_addr(PM_PROC_NR), pm_reboot, &m_in,FALSE)→return` 的 `PM_PROC_NR` 关联 `idle` 假设（注释 *PM is always idle*）。

### 2.3 `service_pm_postponed:668-763` 的四分支

`service_pm_postponed:677 memset(m_out,0); switch(job_call_nr)` 的 `job_call_nr` 为 `self->w_m_in.m_type` 的 `PM_WORK` 存储（`worker.c:271 w_m_in=fp_pm_msg`）。`680 EXEC: proc_e=job_m_in.VFS_PM_ENDPT; exec_path=m7_p1; len=m7_i2; frame=m7_p2; ps_str=m7_i5; assert(proc_e==fp->endpoint); r=pm_exec(path,len,frame,len,&pc,&newsp,&ps_str); m_type=EXEC_REPLY; ENDPT=proc_e; PC=newsp; STATUS=r; NEWPS_STR=ps_str` 的 `pc/newsp` 回带（`25-exec.md` 的 `minix_get_user_sp` 对端）。`703 EXIT: proc_e=ENDPT; assert(==fp); pm_exit(); REPLY dummy ENDPT` 的 `free_proc(FP_EXITING)`级联。`716 DUMPCORE: proc_e=ENDPT; csig=TERM_SIG; core_path=PATH; if(csig==0) panic; assert(==fp); pm_dumpcore(csig,core_path); REPLY CORE` 的 `0→panic` 的“无信号 core 不支持”。`740 UNPAUSE: proc_e=ENDPT; assert(==fp); unpause(); REPLY UNPAUSE` 的 `FP_BLOCKED_ON_NONE` 解挂（`17` 的 `pipe` 与 `23` 的 `select` 两路）。

### 2.4 `pm_fork:577-634` 的四步共享

`pm_fork:592 okendpt(pproc, &parentno)` 的 `isokendpt` 三守卫（`03`）+ `598 childno=_ENDPOINT_P(cproc); if(<0||>=NR)→panic bogus; if(pid!=PID_FREE)→panic in-use` 的 `PID_FREE` 双哨兵 + `606 c_fp_lock=fp_lock; fproc[childno]=fproc[parentno]; fp_lock=c_fp_lock` 的锁保留 + `616 for(OPEN_MAX) if(filp) count++` 的 `filp` 共享（`04` 的 `FilpTable::incr_ref`）+ `620 pid=cpid; endpoint=cproc` + `632 dup_vnode(rd/wd)` 的 `vnode` 共享（`05` 的 `VnodeTable::dup`）+ `629 flags=NOFLAGS` 的 `SESLDR/PENDING/PM_WORK` 清零。

### 2.5 `free_proc:639-708` 的 `FP_EXITING` 分水岭

`free_proc:647 endpoint==NONE→panic already free` + `650 is_blocked→unpause` 的 `FP_BLOCKED_ON_*` 解挂 + `654 for(OPEN_MAX) close_fd(f,FALSE)` 的 `filp_count--` + `659 put_vnode(rd/wd)` 的 `v_ref_count--` + `663 if(!(flags&EXITING)) return` 的 `FP_EXITING` 分水岭 + `665 flags|=EXITING` + `672 unsuspend_by_endpt` + `673 dmap_unmap` + `674 smap_unmap` + `676 worker_stop_by_endpt` + `677 vmnt_unmap_by_endpt` 的 `m_fs_e==ep→mark_free` + `683 SESLDR&&tty!=0 → for(rfp: tty==dev→0; for(f: S_ISCHR&&sdev==dev→cdev_close→FILP_CLOSED))` 的 tty 撤销级联 + `705 endpoint=NONE; pid=PID_FREE; flags=NOFLAGS` 的空闲化。

### 2.6 `pm_set*` 的 `okendpt→slot→*` 单写

`pm_setuid:725 okendpt→slot→tfp->eff= euid; real=ruid`；`pm_setgid:726` 同 `egid/rgid`；`pm_setgroups:743` 的 `ngroups*sizeof(gid) ≤ sizeof(sgroups)` 的 `panic too much` + `sys_datacopy_wrapper(who_e, groups, SELF, sgroups, len)` + `ngroups=len`；`pm_setsid:784 okendpt→slot→flags|=SESLDR; tty=0`。

### 2.7 `pm_reboot/pm_dumpcore` 的 `do_sync` 与 `free_proc` 循环

`pm_reboot:503 do_sync + for(i 0..NR: lock_proc(rfp); endpoint!=NONE&&find_vmnt==NULL → worker_set_proc→free_proc(0)→restore; unlock) + do_sync + unmount_all(0) + for(i … endpoint!=NONE → free_proc(0)) + do_sync + unmount_all(1) + ipc_send(PM,REBOOT_REPLY)` 的 `FREE` vs `EXITING` 双轮释放；`pm_dumpcore:903 if(is_blocked→unpause) + snprintf(core.%d) + common_open(O_WRONLY|CREAT|TRUNC) + sys_datacopy(PM→VFS proc_name) + get_filp(write)→write_elf_core_file→unlock→free_proc(EXITING)` 的 `core` 解挂与 `EXITING` 释放（`26-coredump.md` 的 `write_elf_core_file` 调用点）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `misc.c:606` 的 `fproc[childno]=fproc[parentno]` 与 `worker.c:418` 的 `flags|=PM_WORK`，而是吸收 Redox/Linux 的进程凭证与 `fd` 共享模型后做取舍。以下决策对应 `.design/10-design.v1.md` D1-D6。

### D1 `VFS_PM_*` 类型化：`PmRequest` 枚举的 `decode`

- **C**：`m_type==VFS_PM_SETUID + m7_i2/m7_i3 的 EID/RID` 的散落 `VFS_PM_EID/RID` 宏（`com.h:554`），`switch(call_nr)` 的 `case VFS_PM_*` 12 分支在 `service_pm:783` 与 `service_pm_postponed:679` 两处（立即 7 + 延期 4 + 独立 1 的割裂）。
- **Rust**：`PmRequest` 的 12 变体 `enum`（`Fork{SrvFork, pproc,cproc,cpid,reuid,regid} / Exit{ep} / Exec{ep,path,len,…} / SetUid{ep,euid,ruid} / SetGid / SetGroups{ep,ngroups,addr} / SetSid{ep} / Reboot / Unpause{ep} / DumpCore{ep,sig,path} / SetGroups`）+ `PmRequest::decode(msg: &Message) -> Result<Self, PmError>` 的 `IS_VFS_PM_RQ` 前缀守门（`&~0x7f==0x900`）+ `VFS_PM_EID/RID` 的 `decode` 集中解析；`PmResponse` 的 11 变体 `REPLY`（`0x980` 前缀）对称。
- **为什么**：`VFS_PM_EID` 的 `m7_i2` 在 `SETUID` 专属与 `FORK` 的 `VFS_PM_PENDPT` 复用 `m7_i2` 的 `alias` 在 C 以宏重名隐式，Rust 以 `Fork { pproc }` vs `SetUid { euid }` 的字段名显式使 `m7_i2` 的复用不可误用；`12 枚举`的 `match` 穷尽使 `VFS_PM_REBOOT` 的独立 `PM_PROC_NR` 关联在 `Route::Pm` 的 `Reboot` 分支可审计。
- **备选**：保留 `VFS_PM_EID` 宏的 `Message` 直读；否决——`m7_i2` 的复用在 `FORK` 与 `SETUID` 的错误解码在类型层面不可捕获。

### D2 `pm_fork` 的锁保留与 `filp` 共享：`copy_fproc` 的 `FpLockToken` 显式

- **C**：`c_fp_lock = fproc[childno].fp_lock; fproc[childno]=fproc[parentno]; fp_lock=c_fp_lock` 的 `mutex_t` 暂存（`misc.c:606`），`for(OPEN_MAX) if(filp) filp_count++` 的裸 `++`（`617`）。
- **Rust**：`fn copy_fproc(parent: &FProc, child: &mut FProc, filp_table: &mut FilpTable) -> CopyOutcome` 的 `FpLockToken`（`child` 的 `fp_lock` 属于槽位，整表 `=` 后以 `Token` 恢复，使 `child` 的 `fp_lock` 不随 `parent` 的 `trylock` 状态污染）+ `filp_table.incr_ref(filp_id) -> Result<FilpId, FilpError>` 的 `Result` 守门（`04` 的 `FsfFlags::count` 在 `FilpTable::alloc_free` 的 `count==0` 哨兵中可测试）。
- **为什么**：`mutex_t` 的 `c_fp_lock` 暂存在 Rust 以 `FpLockToken` 的所有权转移显式，避免 `=` 的 `Copy` 语义在 `FpLock` 的 `!Copy` 约束下静默错拷；`filp_count++` 的裸 `++` 在 Rust 以 `incr_ref` 的 `checked_add` 显式溢出检查（`NR_FILPS 1024` 的 `count==0` 空闲哨兵在 `incr_ref` 的 `count==0→Some` 守门）。
- **备选**：整表 `clone_from`；否决——`fp_lock` 的 `!Clone` 使 `clone` 必须 `omit`，类型层面不可 `Copy`。

### D3 `free_proc` 级联：`FreeKind` 枚举的 `FP_EXITING` 分水岭

- **C**：`free_proc(flags)` 的 `flags & FP_EXITING` 分水岭（`misc.c:663`），`unsuspend + dmap/smap_unmap + worker_stop + vmnt_unmap + SESLDR tty revoke` 的 5 级联在 `if (flags&EXITING)` 块内。
- **Rust**：`enum FreeKind { Free, Exiting }` + `fn free_proc(fproc: &mut FProc, kind: FreeKind, ctx: &mut FreeCtx) -> FreeOutcome` 的 `FreeCtx { dmap: &mut DmapTable, smap: &mut SmapTable, worker: &mut WorkerPool, vmnt: &mut VmntTable, tty: &mut TtyRevoker }` 依赖注入；`Free` 仅 `close_fd×256 + put_vnode`，`Exiting` 追加 `unsuspend+dmap+smap+worker+vmnt+tty` 的 5 级联在 `match kind` 穷尽。
- **为什么**：`int flags` 的位集在 Rust 以 `FreeKind` 的穷尽 `match` 使 `pm_reboot` 的双轮 `Free(0)` vs `Exiting` 在 `free_proc` 内可区分；`for(rfp: tty==dev)` 的 `SESLDR` 撤销在 Rust 以 `TtyRevoker::revoke(dev) -> usize` 的 `revoked` 计数可测试。
- **备选**：保留 `flags: u32`；否决——`FP_EXITING` 的 `0x20` 位在 `02` 的 `SESLDR 0x004` 位集中易混淆，枚举的 `match` 使 `FREE` 的 `close_fd` 循环不遗漏 `put_vnode`。

### D4 凭证单写：`okendpt→slot` 的 `Result` 显式

- **C**：`pm_setuid:731 okendpt(proc_e,&slot); tfp=&fproc[slot]; tfp->eff=euid; real=ruid` 的 `panic` 守门（`utility.c:119` 的 `fatal=1`）。
- **Rust**：`FProcTable::ok_endpoint(ep) -> Result<UserSlot, FprocError>` 的 `EDEADEPT→PM 侧 ESRCH` 映射（`03` 的 `FprocError::BadEndpoint.to_errno()`）+ `fn set_uid(fproc: &mut FProc, euid: Uid, ruid: Uid) -> Result<(), FprocError>` 的 `Uid` newtype 守门（`02` 的 `Uid(u32)` 使 `egid` 误传 `Uid` 处编译期失败）。
- **为什么**：`okendpt` 的 `panic` 在 `PM` 控制面是“不可恢复”语义，Rust 以 `Result` 的 `Err(BadEndpoint)` 使 `service_pm` 的 `PM→REPLY` 路径可 `match Err → log::warn + return` 的 `continue`（不 `panic` 整个 VFS 服务，`RS` 的受控重启在 `VfsState` 聚合中可测试）。
- **备选**：`unwrap_or_else(panic)` 的 `ok_endpoint` 保留；否决——`PM` 的 `VFS_PM_SETGROUPS` 的 `groups` 指针跨进程拷贝失败（`sys_datacopy`）应为 `EFAULT` 而非 `panic`。

### D5 `service_pm` 三调度：`PmHandler` trait 的 `Immediate vs Postponed vs Reboot`

- **C**：`service_pm:783 switch` 的 12 分支在 `case SETUID: ipc_send(REPLY)` 的立即与 `case EXEC: worker_start(NULL)→return` 的延期与 `case REBOOT: worker_start(PM_PROC_NR)→return` 的独立在同一 `switch` 内割裂。
- **Rust**：`trait PmHandler { fn handle_immediate(&mut self, req: PmRequest) -> Option<PmResponse>; fn handle_postponed(&mut self, req: PmRequest) -> PmResponse; fn handle_reboot(&mut self, req: PmRequest) -> PmResponse; }` 的 `ImmediateHandler`（`SetUid/Gid/SetGroups/SetSid/Fork/SrvFork` 的 `Result` 显式）与 `PostponedHandler`（`Exec/Exit/DumpCore/Unpause` 的 `WorkerPool::start(...,PM_WORK)`）双实现；`RebootHandler` 的 `PM_PROC_NR` 关联在 `WorkerPool::steal_context` 的 `target idle` 守卫可测试（`08` 的 `TargetNotIdle`）。
- **为什么**：`NULL` 的 `PM_WORK` 标记在 Rust 以 `PostponedHandler` 的 `WorkerFunc::PmPostponed` 显式枚举（`08` 的 `is_pm_work` 分化），`REBOOT` 的 `PM_PROC_NR` 关联在 `VfsState::current_fp_slot == Some(PM_SLOT)` 的 `Option` 守门使 `PM` 永 `idle` 的假设可 `debug_assert`。

### D6 测试与 `fproc_light` 缺口

- **C**：`misc.c:55 SI_PROC_TAB` 的 `fproc_light` 三字段投影（`01` 的 `fproc_light` 3×256 探测）。
- **Rust**：`FprocLight { tty, blocked_on, task }` 的 `#[cfg(feature="fproc_light")]` 缺口在 `FProcTable::snapshot_light` 的 `Vec<FprocLight>` 快照可测试（`03` 的 `ARCH A-7`）。
- **缺口**：`MIB` 拉取的 `sys_datacopy_wrapper` 的两段拷贝在 `99` 的 `CopyToUser` trait 占位；`pm_setgroups` 的 `groups` 指针跨进程拷贝在 `99` 的 `GrantReader` 占位。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-3 `fp_blocked_on` union → 枚举 | `BlockedOn::Pipe/Flock/Select/Cdev/Sdev` 五载荷 | `fproc.rs:BlockedOn` + 本文档 D2/D3 + 10 正文 1.3/1.4 |
| A-4 `glo.h` 全局 → `VfsState` 聚合 | `VfsState { fproc_table, reviving, pending }` + `FreeCtx` 注入 | `main_loop.rs:VfsState` + 本文档 D3 + 10 正文 1.5 |
| A-7 `fproc_light` 缺口 | `FprocLight` 的 `#[cfg(feature)]` 快照 | `fproc.rs:FprocLight` + 本文档 D6 + 10 正文 1.6 |
| A-8 64 位 | `Pid/Uid/Gid/Endpoint` 的 `minix-types` newtype | `minix-types:Pid` + 本文档 D4 + 10 正文 1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── ipc/dispatcher.rs       — PmRequest(12)/PmResponse(11)/PmError/decode + PmHandler trait (Immediate/Postponed/Reboot) + handle_fork/handle_exit/handle_setuid/setgid/setgroups/setsid/reboot/unpause/dumpcore + CopyOutcome/FreeKind/FreeCtx
├── fproc.rs                — FProc { pid/endpoint/flags/PENDING/PM_WORK/SESLDR/REVIVED, BlockedOn::Pipe/Flock, uid/gid/sgroups } + FProcTable::ok_endpoint
├── main_loop.rs            — VfsState { fproc_table/worker_pool/reviving } 的 enqueue_revive/unblock/ poll_next 的 reviving 优先（09）
├── worker.rs               — WorkerPool::start(...,PM_WORK) 的挂起与 steal_context 的 PM 关联（08）
└── filp.rs / vnode.rs      — FilpTable::incr_ref / VnodeTable::dup/put 的 ref 递增（04/05，free_proc 的 close_fd/put_vnode 对端）
```

### 4.2 `dispatcher.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `VFS_PM_RQ/RS_BASE` | `com.h:513-514` | `dispatcher.rs:PmRequest::BASE 0x900` | `&~0x7f==0x900` 的 `IS_VFS_PM_RQ` 前缀守门 |
| 12 请求 | `com.h:520-531` | `PmRequest::decode(msg)->Result<Self, PmError>` | `VFS_PM_FORK(0x907)+SRV_FORK(0x908)+EXIT(0x904)+EXEC(0x906)+SETUID(0x901)+…` 12 变体 |
| `service_pm` 立即 | `main.c:783` | `PmHandler::handle_immediate(req)->Option<PmResponse>` | `pm_setuid/gid/sid/groups + pm_fork + Fork/SrvFork` 的 `okendpt→slot` 守门 |
| `service_pm_postponed` 延期 | `main.c:668` | `PmHandler::handle_postponed(req)->PmResponse` | `Exec/Exit/DumpCore/Unpause` 的 `WorkerPool::start(...,PmPostponed)` |
| `pm_fork` 四步 | `misc.c:577` | `handle_fork(parent, child, pid, filp_table, vnode_table)->Result<CopyOutcome, ForkError>` | `okendpt+PID_FREE→copy_fproc+fp_lock Token→filp_count++→dup_vnode→FP_NOFLAGS` |
| `free_proc` 级联 | `misc.c:639` | `free_proc(slot, kind:FreeKind, ctx:FreeCtx)->FreeOutcome` | `Free( close_fd×256 + put_vnode ) vs Exiting(+unsuspend+dmap+smap+worker+vmnt+tty)` |
| `pm_setuid/gid` | `misc.c:764/726` | `handle_setuid(ep,euid,ruid)` | `okendpt→slot→eff/real` 双写 |
| `pm_setgroups` | `misc.c:743` | `handle_setgroups(ep, ngroups, groups)` | `ngroups*sizeof(gid) ≤ sizeof(sgroups) → datacopy` |
| `pm_setsid` | `misc.c:779` | `handle_setsid(ep)` | `flags\|=SESLDR; tty=0` |
| `pm_reboot` | `misc.c:503` | `handle_reboot(&mut VfsState)` | `do_sync + for(proc: endpoint!=NONE→free_proc(Free)) + unmount_all(0) + for(Free)+unmount_all(1)` |

### 4.3 `free_proc` 的 `FreeCtx` 注入

`FreeCtx { dmap: &mut DmapTable, smap: &mut SmapTable, worker: &mut WorkerPool, vmnt: &mut VmntTable, filp_table: &mut FilpTable, vnode_table: &mut VnodeTable }` 的 `&mut` 注入使 `dmap_unmap_by_endpt`（`19`）、`smap_unmap`（`19`）、`worker_stop_by_endpt`（`08`）、`vmnt_unmap_by_endpt`（`06`）、`close_fd`（`14`）、`put_vnode`（`05`）的 6 对端在 `free_proc` 内可 `mock` 注入测试（`MockDmap` 的 `unmap→0` vs `RealDmap` 的 `find→mark_free` 行为差异在 `PmHandler` 双实现中可测）。

### 4.4 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 前缀不重叠 `IS_VFS_PM_RQ & IS_VFS_CALL==∅` | `PmRequest::decode` | `&~0x7f==0x900` vs `&~0xff==0x100` | `com.h:516` vs `callnr.h:70` |
| 槽位双守门 `okendpt + PID_FREE` | `handle_fork` | `okendpt(pproc) → parentno` + `child pid==PID_FREE` | `misc.c:592/601` |
| 锁保留 `fp_lock ∈ slot` | `copy_fproc` | `c_token=child.lock; child=parent; child.lock=token` | `misc.c:606` |
| FD 共享 `filp_count>0` | `handle_fork` | `for(OPEN_MAX) if(filp) incr_ref` 的 `checked_add` | `misc.c:617` |
| 标志清零 `FP_NOFLAGS` | `handle_fork` | `flags=NOFLAGS` 的 `SESLDR→0` | `misc.c:629` |
| 级联分水岭 `Exiting` | `free_proc` | `match kind { Free→close+put; Exiting→+5级联 }` | `misc.c:663` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **124 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 + `ipc/dispatcher` 16 = 118 → `cargo test` 实测 124；`minix-types` 108 独立）。
> 本章直接影响 `4 → 16` 项新增（`fork_ok/fork_in-use/fork_bogus/srv_fork_uid/fork_lock_preserved/filp_shared/exit_free/exit_exiting/setsid/setuid/setgid/setgroups/reboot/pm_request_decode/pm_handler_two_impls`），`minix-vfs --lib` 总计 112 → 124。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_handle_fork_success` | `misc.c:592-629` | `okendpt+PID_FREE→copy + NOFLAGS` | `dispatcher.rs` |
| `test_handle_fork_bogus_child` | `misc.c:599-602` | `childno<0\|>=NR → Err(BogusChild)` | `dispatcher.rs` |
| `test_handle_fork_in_use` | `misc.c:601-602` | `pid!=PID_FREE → Err(InUse)` | `dispatcher.rs` |
| `test_handle_srv_fork_sets_ids` | `misc.c:868-870` | `SRV_FORK → fork + setuid + setgid` | `dispatcher.rs` |
| `test_copy_fproc_lock_preserved` | `misc.c:606` | `c_lock token 恢复` 的 `slot 锁保留` | `dispatcher.rs` |
| `test_copy_fproc_filp_shared` | `misc.c:617` | `filp_count++` 的 `incr_ref` | `dispatcher.rs` |
| `test_handle_exit_free` | `misc.c:647-660` | `endpoint!=NONE 且 !EXITING → close+put` | `dispatcher.rs` |
| `test_handle_exit_exiting` | `misc.c:663-708` | `EXITING → +5 级联（unsuspend/dmap/smap/worker/vmnt/tty）` | `dispatcher.rs` |
| `test_handle_setuid` | `misc.c:764-774` | `eff/real 双写` | `dispatcher.rs` |
| `test_handle_setgid` | `misc.c:726-736` | `eff/real gid 双写` | `dispatcher.rs` |
| `test_handle_setgroups` | `misc.c:743-756` | `ngroups*sizeof ≤ sizeof` 守门与 `datacopy` | `dispatcher.rs` |
| `test_handle_setsid` | `misc.c:784-791` | `SESLDR\|=1; tty=0` | `dispatcher.rs` |
| `test_pm_request_decode` | `com.h:516` | `IS_VFS_PM_RQ(0x907)→Some(Fork)` | `dispatcher.rs` |
| `test_pm_handler_two_impls` | `dispatcher.rs:1` | `PmHandler` trait `VfsPmHandler` vs `MockPmHandler` 的 `Fork Ok vs Err` 行为差异 | `dispatcher.rs` |

测试策略：`handle_fork` 的 `okendpt+PID_FREE` 以 `parent 0→child 1 Ok` 与 `child 1 已占用 → Err` 两样本覆盖；`SRV_FORK` 以 `fork 后 uid/gid 双写` 的追加样本覆盖；`free_proc` 的 `Free vs Exiting` 以 `flags & EXITING` 的两样本覆盖；`PmRequest::decode` 的 `0x907→Fork` 与 `0x100→None` 的前缀守门两样本覆盖；`PmHandler` 以 `Vfs(Ok) vs Mock(Err)` 的 `dyn` 行为差异样本覆盖。

---

## 6 过渡

本篇在 `09-main-loop` 的 `Route::Pm` 的 `main:91` 守门之后，`service_pm` 的 `立即/延期/独立` 三调度将 `PM → VFS` 的 12 请求分流：`FORK/SRV_FORK` 的 `copy_fproc` 共享在 `04/05` 的 `filp_count++` 与 `dup_vnode` 可观测，`EXIT` 的 `free_proc` 级联在 `14` 的 `close_fd` 与 `06` 的 `vmnt_unmap` 可观测，`EXEC` 的 `pm_exec` 在 `25-exec.md` 的 `Get_read_vp` 可观测。

```
09-main-loop: main( reviving→transid→PM→notify→task→BDEV/CDEV/SDEV→syscall )  （Route::Pm 守门）
  │
  └─► 本章: service_pm( 立即 SETUID/GID/SID/GROUPS/FORK/SRV_FORK ) + worker_start(PM_WORK) 的 EXEC/EXIT/DUMPCORE/UNPAUSE 延期 + Reboot 独立  （PmRequest 12 变体与 FreeKind::Exiting 的 5 级联）
         │
         ├─► 04-filp-table: filp[1024] 的 incr_ref 与 close_fd 单步  （fork 共享与 exit 释放的 Filp 对端）
         ├─► 05-vnode-table: vnode 的 dup_vnode 与 put_vnode 的双层计数  （fork 的 rd/wd 共享与 exit 的 put 对端）
         ├─► 06-vmnt-table: vmnt_unmap_by_endpt 的 m_fs_e 失效  （exit 的 FS 退出的 vmnt 回收）
         ├─► 08-worker-thread: worker_start(...,PM_WORK) 的挂起与 FreeCtx 的 worker_stop  （PM_WORK 的生产与 exit 的 worker 消费）
         ├─► 14-filedes: close_fd 的 filp_count-- 与 FD_CLOEXEC 清除  （free_proc 的 close 循环单步）
         ├─► 18-mount: unmount_all 的 vmnt 批量失效  （reboot 的 unmount 阶段）
         ├─► 25-exec: pm_exec 的进程映像替换与 VFS → VM 的 memmap  （EXEC 延期的消费方）
         └─► 26-coredump: pm_dumpcore 的 write_elf_core_file  （DUMPCORE 延期的消费方）
```

`VFS_PM_FORK` 的 `copy_fproc` 共享在 `04/05` 的 `filp_count++` 与 `dup_vnode` 可观测，为 `10` 的次主线 `fork` 的后续 `14` 的 `close_fd` 释放提供 `count>1` 的共享可观测；`free_proc` 的 `EXITING` 级联在 `06` 的 `vmnt_unmap` 与 `19` 的 `dmap_unmap` 可观测，为 `99` 的 `NR_PROCS` 常量与 `Endpoint` 术语提供 `PID_FREE` 双哨兵的回收可观测。阅读顺序提示：若关心“`fd` 如何被共享与释放”，下一站 `04-filp-table.md` + `14-filedes.md`；若关心“`fork` 后 `exec` 如何替换映像”，下一站 `25-exec.md`。

---

## 7 参见

- C 源：`minix3/minix/include/minix/com.h:513-584`（`VFS_PM_RQ/RS_BASE` 前缀与 12 请求/11 回复及 `VFS_PM_*` 的 `m7_i*/m7_p*` 宏）、`minix3/minix/servers/vfs/main.c:668-763`（`service_pm_postponed` 的 `EXEC/EXIT/DUMPCORE/UNPAUSE` 四分支）、`764-915`（`service_pm` 的 `SETUID/GID/SID/GROUPS/FORK/SRV_FORK/REBOOT` 立即与 `FORK` 的 `SRV` 追加）、`minix3/minix/servers/vfs/misc.c:503-572`（`pm_reboot` 的 `do_sync×3` 与 `free_proc` 双轮）、`577-634`（`pm_fork` 四步）、`639-720`（`free_proc/ pm_exit` 级联）、`726-792`（`pm_setgid/setgroups/setuid/setsid` 凭证）、`903-943`（`pm_dumpcore` 的 `unpause→open→write_elf→free_proc`）、`minix3/minix/servers/vfs/fproc.h:91-98`（`FP_*` 标志）、`minix3/minix/servers/vfs/glo.h:26-28`（`fproc_addr/who_p`）
- 阶段文档：`03-fproc-table.md`（`isokendpt` 三守卫与 `PID_FREE` 双哨兵）、`09-main-loop.md`（`Route::Pm` 的 `main:91` 守门与 `PROM` 前缀不重叠）、`08-worker-thread.md`（`WorkerPool::start(...,PM_WORK)` 的 `FP_PENDING` 排队与 `steal_context`）、`04-filp-table.md`（`FilpTable::incr_ref` 的 `count==0` 哨兵）、`05-vnode-table.md`（`VnodeTable::dup/put` 的 `v_ref_count/v_fs_count` 双层）、`14-filedes.md`（`close_fd` 的单步）、`99-global-concepts.md`（`VFS_PM_*` 术语与 `NR_PROCS` 常量）
- Rust 实现：`os/servers/vfs/src/ipc/dispatcher.rs:1`（`PmRequest(12)/PmResponse(11)/PmHandler(Vfs vs Mock)/handle_fork/handle_exit/handle_setuid/free_proc`）、`os/servers/vfs/src/main_loop.rs:1`（`PmMessageType` 的 `Unknown` 兜底 + `VfsState::handle_pm_fork` 的 `PID_FREE` 守门）、`os/servers/vfs/src/fproc.rs:1`（`FProcTable::ok_endpoint` 与 `FpFlags::NOFLAGS/SESLDR/REVIVED`）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../04-stage-pm/05-vfs-interaction.md`（`PM` 侧 `tell_vfs` 的 `VFS_PM_*` 发送方状态机）、`../01-stage-kernel/18-syscall-copy.md`（`sys_datacopy_wrapper` 的跨进程拷贝）
