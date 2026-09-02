# 18 — 调试：`do_trace` 的 `T_*` 全族与 `trace_stop` 的 `W_STOPCODE` 暂停

本文讲清调试如何在 PM 侧以“`T_OK` 的子进程自声明与 `T_ATTACH` 的主动附着（`SUPER_USER` 三重 + `PRIV_PROC` 双向禁）+ `TRACE_STOPPED` 独立于 `PROC_STOPPED` + `mp_sigtrace` 位图缓冲的 `check_sig` 全量重放 + `T_EXIT` 的 `TRACE_EXIT` 哨兵与 `VFS|EVENT` 分叉 + `T_DETACH` 的 `sigtrace→check_sig` 全量 + `T_RESUME` 的 `sigtrace→OK` 短路 + `trace_stop` 的 `sys_trace(T_STOP)` + `W_STOPCODE` 的 `0x7f` 截断”为完整链路，使 `ptrace` 的 16 命令在 `wait4` 的 `TRACE_STOPPED` 分支与 `tracer_died` 的 `TRACE_EXIT` 消费中可区分。

前置阅读：03-mproc-table.md（`find_proc`/`pm_isokendpt` 的 `ESRCH` 双路径与 `who_p` 显式 `caller`）、10-pm-wait.md（`wait_test` 的 `TRACE_STOPPED` 分支 `wait4` 的 `tracer` 伪父 `tell_tracer` 与 `W_STOPCODE` 的 `0x7f` 截断）、11-signal-core.md（`sig_proc` 的 `TRACE` 先行 `sigtrace` 位图与 `VFS|EVENT` 挂起的 `PROC_STOPPED` 双用途）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `guardianship` 的 `Normal/Traced` 双态（02）、`wait_test` 的 `TRACE_STOPPED` 分支与 `W_STOPCODE` 的 `0x7f` 截断（10）、`sig_proc` 的 `TRACE` 先行 `sigtrace` 位图（11）的开发者。

> **本章不讲什么**：
> - 内核 `sys_trace` 实现（`T_STOP/GET/SET` 寄存器/内存拷贝）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel` 的 `kernel/system/do_trace.c`
> - `tracer_died` 的 `TRACER_DEATH` 消费（`TRACED` 转 `Zombie→ToldParent` 的 `forkexit.c:760-795`）—— `09-pm-exit.md`
> - `wait4` 的 `tracer` 分支 `tell_tracer`（`W_STOPCODE` 的 `0x7f` 截断与 `wait_test` 的 `TRACE_STOPPED` 分支）—— `10-pm-wait.md` 已覆盖
>
> 本章只回答一个问题：**PM 如何为 `ptrace` 的 16 命令在 `T_OK/ATTACH` 的双入口与 `TRACE_STOPPED` 独立暂停与 `mp_sigtrace` 位图缓冲的 `check_sig` 全量重放与 `W_STOPCODE` 的 `wait4` 通知中建立 `Traced` 状态机**。

### 1.1 为什么 `ptrace` 需要 `T_OK` 与 `T_ATTACH` 双入口：子进程自声明 vs 调试器主动附着

`T_OK` 的子进程自声明（`fork` 后 `exec` 前 `ptrace(T_OK)` 的 `mp->mp_tracer = mp->mp_parent` 自挂 `trace.c:58`）与 `T_ATTACH` 的调试器主动附着（`find_proc(pid)` 的 `SUPER_USER` 三重 `trace.c:67-71` + `PRIV_PROC` 双向禁 `74-78` + `self/PM/VM` 禁 `81-82` + `already traced→EBUSY` `85` + `TO_NOEXEC` `88` + `SIGSTOP` 暂停 `90`）双入口使“子进程自愿被调试”与“调试器强行附着已运行进程”在 PM 侧以 `T_OK` 的 `EBUSY` 门（`56` `tracer!=NO_TRACER→EBUSY`）与 `T_ATTACH` 的 `SUPER_USER` 三重对偶——`T_OK` 的子进程自挂无需 `SUPER_USER` 判据（`fork` 的 `parent` 即 `tracer` 的信任链），`T_ATTACH` 的主动附着需 `SUPER_USER` 三重（`eff!=SUPER_USER && (eff!=child_eff || effgid!=child_effgid || child_eff!=child_real || child_effgid!=child_realgid)→EPERM`）。

### 1.2 为什么 `TRACE_STOPPED` 独立于 `PROC_STOPPED`：调试暂停 vs `VFS` 异步暂停

`TRACE_STOPPED 0x80`（`mproc.h:93`）的“调试器暂停”与 `PROC_STOPPED 0x08`（`mproc.h:89`）的“`VFS` 异步暂停”双轨（`trace.rs:TraceState { stopped, exit_pending }` 双 bool）：`trace_stop` 置 `TRACE_STOPPED`（`trace.c:266`）后 `wait_test` 的 `W_STOPCODE` 通知 `tracer` 的 `wait4`（`trace.c:273` `W_STOPCODE(signo)` 的 `0x7f` 截断，`wait.h: W_STOPCODE`），而 `PROC_STOPPED` 的 `restart_sigs` 重检 `pending`（`13` 的 `check_pending` 的 `VFS→break`）不经 `W_STOPCODE`——`TRACE_STOPPED` 的 `wait4` 通知经 `wait` 的 `tell_tracer` 的 `W_STOPCODE` 的 `0x7f` 截断与 `PROC_STOPPED` 的 `restart_sigs` 的 `pending` 重检双轨分叉。

### 1.3 为什么 `mp_sigtrace` 位图是 `tracer` 的 `pending` 缓冲

`sig_proc:417` `sigaddset(sigtrace, signo)` 的 `TRACE` 先行缓冲（`11` 的 `TRACE` 先行 `sigtrace` 位图）+ `T_DETACH:197-201` 的 `sigtrace→check_sig` 全量重放（`for i if sigtrace→check_sig` 的 `check_sig` 全量重放）+ `T_RESUME:231-236` 的 `sigtrace` 全量 `OK` 短路（`sigismember(sigtrace) → OK` 的 `T_RESUME` 假成功，使调试器 `wait4` 的 `W_STOPCODE` 后 `SIGTRAP` 投递延迟）——`mp_sigtrace` 是 `tracer` 的 `pending` 缓冲，`T_DETACH` 的全量重放使 `DETACH` 后 `sigtrace` 的 `pending` 信号不丢失，`T_RESUME` 的假成功使 `tracer` 的 `wait4` 后 `SIGTRAP` 延迟至 `sigtrace` 清空。

### 1.4 为什么 `T_EXIT` 需 `TRACE_EXIT` 哨兵与 `VFS|EVENT` 分叉

`T_EXIT:147-155` `TRACE_EXIT` 置位（`147`）+ `VFS|EVENT→save exitstatus`（`150-151` `VFS|EVENT→save it`）否则 `exit_proc`（`153-154` `exit_proc(child, data, FALSE)`）+ `SUSPEND`（`159` `04` 的 `ReplyLater` 的 `T_EXIT` 子类）的 `VFS` 未决时 `save` 而非直接 `exit`（`handle_vfs_reply` 的 `publish_event` 前 `TRACE_EXIT` 已判，`restart_sigs` 的 `TRACE_EXIT→exit_proc` 优先于 `PROC_STOPPED` 重检 `13` 的 `TraceExitState`）。

### 1.5 为什么 `T_DETACH` 的 `data` 信号需 `sig_proc` 再投

`T_DETACH:204-206` `data>0 → sig_proc(child, data, TRUE)` 的 `TRUE` 使 `TRACED` 进程的 `SIGKILL` 可经 `sig_proc` 的 `TRACE_STOPPED` 跳过而 `SIGSTOP` 的 `data` 信号在 `check_pending` 后 `~TRACE_STOPPED` 的 `child` 上再投（`210-214` 的 `~TRACE_STOPPED` + `flags=0` + `check_pending` 使 `pending` 的 `SIGSTOP` 在 `DETACH` 后 `check_pending` 的 `VFS→break` 前投）。

### 1.6 为什么 `T_GETRANGE/T_SETRANGE` 的 `TS_INS/DATA` 校验

`173` `pr_space != TS_INS && TS_DATA → EINVAL` + `174` `pr_size 0或>LONG_MAX→EINVAL` 的 `ptrace_range { pr_space, pr_addr, pr_ptr, pr_size }` 范围校验，`sys_vircopy` 的 `child→who_e` 双向拷贝（`177-183` `GET: child→who_e` vs `SET: who_e→child`）使调试器可批量读/写 `INS/DATA` 段。

### 1.7 与其他 OS 的对照

Rust 改写不是照抄 `trace.c:56` 的 `mp->mp_tracer != NO_TRACER → EBUSY`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `ptrace(PTRACE_ATTACH/DETACH/TRACEME)` 的 `may_ptrace_attach`。** Linux `ptrace_attach` 的 `may_ptrace_attach` 的 `SUPER_USER` 三重 + `PRIV_PROC` 双向禁 `capable(CAP_SYS_PTRACE)` + `already traced→EBUSY` + `SIGSTOP` 暂停与 Minix3 `T_ATTACH` 的 `67-91` 同谓词（`SUPER_USER` 三重 + `PRIV_PROC` 双向禁 + `already traced→EBUSY` + `SIGSTOP`），`WSTOPPED` 的 `WIFSTOPPED`/`WSTOPSIG` 的 `W_STOPCODE` `0x7f` 截断与 `trace_stop:273` 同 `0x7f` 截断，`PtracePeekData/PokeData` 的 `sys_ptrace` 透传与 `trace.c:244-249` 同 `sys_trace` 透传。

**Redox `ptrace` 缺省。** Redox 无 `ptrace`（`Scheme` 的 `handle` 替代，`kernel/ptrace` 未实现），调试经 `Scheme` 的 `handle` 的 `RwLock` 与 `TCB` 的 `suspend` 显式（`kernel/scheme: handle` 的 `park`），`mp_sigtrace` 位图缓冲在 Redox 以 `Scheme` 的 `pending` 位图替代。

**`seL4` 无 `ptrace` 的 `TCB` 显式 `suspend`。** `seL4` 无 `ptrace`（`seL4_TCB_Suspend` + `seL4_TCB_Resume` 的显式 `suspend` 与 `seL4_TCB_ReadRegisters` 的寄存器拷贝替代 `ptrace`，`seL4_TCB_Suspend` 先于 `ReadRegisters` 的时序与 `trace_stop` 的 `sys_trace(T_STOP)` 先于 `TRACE_STOPPED` 置位同序）。

**结论（本章的设计基线）。** 把 C 的“`mp->mp_tracer != NO_TRACER → EBUSY` 哨兵 + `SUPER_USER` 三重与 `PRIV_PROC` 双向禁散落 + `TRACE_STOPPED` 位与 `PROC_STOPPED` 双轨 + `mp_sigtrace` 位图缓冲的 `check_sig` 全量重放 + `W_STOPCODE` 的 `0x7f` 截断裸宏”改写为“`GuardianState::try_set_tracer` + `AttachPolicy::may_attach` 一处谓词 + `TraceState { stopped, exit_pending }` 双 bool + `TraceDetach::replay_sigtrace` + `WaitCode::stop` 枚举的 `0x7f` 截断”——与 Linux `ptrace` 的 `may_ptrace_attach` + `WIFSTOPPED` 同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `TraceOptions` 显式携带 `TO_*`。

### 1.8 小结

1. **为什么双入口**——`T_OK` 子进程自挂 `parent` 与 `T_ATTACH` 调试器主动附着的 `SUPER_USER` 三重对偶。
2. **为什么独立暂停**——`TRACE_STOPPED` 的调试暂停与 `PROC_STOPPED` 的 `VFS` 异步暂停双轨。
3. **为什么位图缓冲**——`sigtrace` 位图使 `TRACE` 先行的信号在 `DETACH` 后 `check_sig` 全量重放与 `T_RESUME` 的假成功不丢失。
4. **为什么哨兵**——`T_EXIT` 的 `TRACE_EXIT` 使 `VFS` 未决时 `save` 而非直接 `exit`。
5. **为什么再投**——`T_DETACH` 的 `data>0→sig_proc` 使 `DETACH` 后的 `SIGSTOP` 在 `check_pending` 后再投。
6. **为什么范围校验**——`TS_INS/DATA` 与 `LONG_MAX` 上界使 `GETRANGE/SETRANGE` 的 `sys_vircopy` 双向安全。

下一章逐行分析 C 的 `do_trace` 全族与 `trace_stop`；第 3 章给出 Rust 的 `T_*` 枚举与 `TraceStop::stop`。

---

## 2 C 源码分析

### 2.1 `do_trace` 序言（`trace.c:42-54`）

```c
int do_trace(void)
{ // 42  ptrace 的 PM 侧入口（16 命令的 T_* 全族）
  register struct mproc *child; struct ptrace_range pr; int i, r, req; // 44-46
  req = m_in.m_lc_pm_ptrace.req; // 48  req（m_lc_pm_ptrace.req，ipc.h:469 `req`）
```

`48` 行 `req` 取 `m_in.m_lc_pm_ptrace.req`（`ipc.h:469` `mess_lc_pm_ptrace { pid, req, addr, data }` 的 `req`），`54` 行 `switch(req)` 的 `T_OK/ATTACH` 优先（`50-52` 注释 `T_OK 为子 fork 后 exec 前` vs `T_ATTACH 为调试器主动`）。

### 2.2 `T_OK`（`trace.c:55-60`）

```c
  case T_OK: // 55  enable tracing by parent for this proc
    if (mp->mp_tracer != NO_TRACER) return(EBUSY); // 56  已被追踪→EBUSY
    mp->mp_tracer = mp->mp_parent; // 58  tracer=parent（mp->mp_parent）
    mp->mp_reply.m_pm_lc_ptrace.data = 0; // 59  reply.data=0
    return(OK); // 60
```

`56` 行 `tracer!=NO_TRACER→EBUSY` 守卫与 `85` 行 `already traced→EBUSY` 同哨兵（`NO_TRACER 0` 的 `PM` 槽位哨兵在 Rust 以 `Option` 的 `None` 收敛，`02` 的 `NO_TRACER_INDEX 0` 已论证），`58` 行 `tracer=parent` 的自挂使 `fork` 后 `exec` 前的 `T_OK` 无 `SUPER_USER` 判据（信任链为 `parent` 即 `tracer`）。

### 2.3 `T_ATTACH`（`trace.c:62-93`）

```c
  case T_ATTACH: // 62  attach to an existing process
    if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH); // 63  find_proc→ESRCH
    if (child->mp_flags & EXITING) return(ESRCH); // 64  EXITING→ESRCH
    if (mp->mp_effuid != SUPER_USER && // 67  eff!=SUPER_USER
        (mp->mp_effuid != child->mp_effuid || // 68  eff!=child_eff
         mp->mp_effgid != child->mp_effgid || // 69  effgid!=child_effgid
         child->mp_effuid != child->mp_realuid || // 70  child eff!=real
         child->mp_effgid != child->mp_realgid)) return(EPERM); // 71  child effgid!=realgid → EPERM
    if (mp->mp_effuid != SUPER_USER && (child->mp_flags & PRIV_PROC)) return(EPERM); // 74-75  eff!=SUPER_USER && child PRIV_PROC→EPERM
    if (mp->mp_flags & PRIV_PROC) return(EPERM); // 78  caller PRIV_PROC→EPERM（系统服务禁追）
    if (child == mp || child->mp_endpoint == PM_PROC_NR || // 81  self/PM/VM→EPERM
        child->mp_endpoint == VM_PROC_NR) return(EPERM); // 82
    if (child->mp_tracer != NO_TRACER) return(EBUSY); // 85  已被追踪→EBUSY
    child->mp_tracer = who_p; // 87  tracer=who_p（调试器槽位）
    child->mp_trace_flags = TO_NOEXEC; // 88  flags=TO_NOEXEC（exec 前置信号可配）
    sig_proc(child, SIGSTOP, TRUE /*trace*/, FALSE /* ksig */); // 90  SIGSTOP 暂停（TRUE 使 TRACE 先行）
    mp->mp_reply.m_pm_lc_ptrace.data = 0; // 92  reply 0
    return(OK); // 93
```

`67-71` 行 `SUPER_USER` 三重、`74-78` 行 `PRIV_PROC` 双向禁、`81-82` 行 `self/PM/VM→EPERM` + `85` 行 `already traced→EBUSY` + `88` 行 `TO_NOEXEC` + `90` 行 `SIGSTOP` 暂停—— `T_ATTACH` 的 7 守卫在 `may_attach` 一处谓词（`D2`）。

### 2.4 `T_STOP`（`trace.c:95-99`）

```c
  case T_STOP: // 95  stop the process（未暴露）
    return(EINVAL); // 99  未暴露，kill 的 SIGSTOP 替代（96-98 注释）
```

`99` 行 `EINVAL` 未暴露（`95-98` 注释 `its effect can be achieved better by sending the traced process a signal with kill(2)`，`kill` 的 `SIGSTOP` 经 `check_sig` 的 `PROC_STOPPED` 双用途替代 `T_STOP`）。

### 2.5 `T_READB_INS/T_WRITEB_INS`（`trace.c:101-134`）

```c
  case T_READB_INS: // 101  special hack for reading text segments
    if (mp->mp_effuid != SUPER_USER) return(EPERM); // 102  SUPER_USER 门
    if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH); // 103
    if (child->mp_flags & EXITING) return(ESRCH); // 104
    r = sys_trace(req, child->mp_endpoint, m_in.m_lc_pm_ptrace.addr, &m_in.m_lc_pm_ptrace.data); // 106-107  sys_trace 透传
    if (r != OK) return(r); // 108
    mp->mp_reply.m_pm_lc_ptrace.data = m_in.m_lc_pm_ptrace.data; // 110  reply.data 回填
    return(OK); // 111
```

`102/114` 行 `SUPER_USER` 门使仅 `root` 可读写文本段，`106-107/129-130` 行 `sys_trace(req, endpoint, addr, &data)` 透传 `T_OK/ATTACH` 外的 `DATA/INS/USER` 等。

### 2.6 `do_trace` 后半守卫（`trace.c:140-143`）

```c
  if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH); // 140  find_proc→ESRCH
  if (child->mp_flags & EXITING) return(ESRCH); // 141  EXITING→ESRCH
  if (child->mp_tracer != who_p) return(ESRCH); // 142  tracer!=who_p→ESRCH（非 tracer 的调试器）
  if (!(child->mp_flags & TRACE_STOPPED)) return(EBUSY); // 143  !TRACE_STOPPED→EBUSY（未暂停）
```

`140-143` 行 `find_proc→ESRCH` + `EXITING→ESRCH` + `tracer!=who_p→ESRCH` + `!TRACE_STOPPED→EBUSY` 守卫为 `T_EXIT` 等 7 命令的 `TRACE_STOPPED` 前置守卫。

### 2.7 `T_EXIT`（`trace.c:146-159`）

```c
  case T_EXIT: // 146  exit
    child->mp_flags |= TRACE_EXIT; // 147  TRACE_EXIT 置位（mproc.h:100 TRACE_EXIT 0x8000）
    if (child->mp_flags & (VFS_CALL | EVENT_CALL)) // 150  VFS|EVENT→save
      child->mp_exitstatus = m_in.m_lc_pm_ptrace.data; // 151  save it（mp_exitstatus）
    else
      exit_proc(child, m_in.m_lc_pm_ptrace.data, FALSE /*dump_core*/); // 153-154  直接 exit
    return(SUSPEND); // 159  SUSPEND（04 的 ReplyLater 的 T_EXIT 子类）
```

`147` 行 `TRACE_EXIT` 置位与 `VFS|EVENT→save`（`150-151`）否则 `exit_proc`（`153-154`）的 `VFS` 未决时 `save` 而非直接 `exit`，`159` 行 `SUSPEND`（`04` 的 `ReplyLater` 的 `T_EXIT` 子类，`handle_vfs_reply` 的 `publish_event` 前 `TRACE_EXIT` 已判）。

### 2.8 `T_SETOPT`（`trace.c:161-165`）

```c
  case T_SETOPT: // 161  set trace options
    child->mp_trace_flags = m_in.m_lc_pm_ptrace.data; // 162  trace_flags=data（TO_* 位直接存）
    mp->mp_reply.m_pm_lc_ptrace.data = 0; // 164
    return(OK); // 165
```

`162` 行 `trace_flags=data` 的 `TO_*` 位直接存（`TO_NOEXEC 0x1/ALTEXEC 0x2/TRACEFORK 0x1` 的 `TraceOptions` 一处结构，`A-12`）。

### 2.9 `T_GETRANGE/T_SETRANGE`（`trace.c:167-188`）

```c
  case T_GETRANGE:
  case T_SETRANGE: // 167-168  get/set range of values
    r = sys_datacopy(who_e, m_in.m_lc_pm_ptrace.addr, SELF, (vir_bytes)&pr, (phys_bytes)sizeof(pr)); // 169-170  取 ptrace_range
    if (r != OK) return(r); // 171
    if (pr.pr_space != TS_INS && pr.pr_space != TS_DATA) return(EINVAL); // 173  TS_INS 0/TS_DATA 1 校验
    if (pr.pr_size == 0 || pr.pr_size > LONG_MAX) return(EINVAL); // 174  size 0或>LONG_MAX→EINVAL
    if (req == T_GETRANGE) // 176
      r = sys_vircopy(child->mp_endpoint, (vir_bytes) pr.pr_addr, who_e, (vir_bytes) pr.pr_ptr, (phys_bytes) pr.pr_size, 0); // 177-179  GET: child→who_e
    else // 180
      r = sys_vircopy(who_e, (vir_bytes) pr.pr_ptr, child->mp_endpoint, (vir_bytes) pr.pr_addr, (phys_bytes) pr.pr_size, 0); // 181-183  SET: who_e→child
    if (r != OK) return(r); // 185
    mp->mp_reply.m_pm_lc_ptrace.data = 0; // 187
    return(OK); // 188
```

`169-170` 行 `sys_datacopy` 取 `pr`（`ptrace_range { pr_space, pr_addr, pr_ptr, pr_size }`），`173` 行 `TS_INS/DATA` 校验与 `174` 行 `pr_size` 上界 `LONG_MAX` 一处校验（`D5` `PtraceRange::validate`），`177-183` 行 `sys_vircopy` 双向。

### 2.10 `T_DETACH`（`trace.c:190-215`）

```c
  case T_DETACH: // 190  detach from traced process
    if (m_in.m_lc_pm_ptrace.data < 0 || m_in.m_lc_pm_ptrace.data >= _NSIG) return(EINVAL); // 191-192  data<0||>=64→EINVAL
    child->mp_tracer = NO_TRACER; // 194  tracer=NO_TRACER（清哨兵）
    for (i = 1; i < _NSIG; i++) { // 197  sigtrace→check_sig 全量重放
      if (sigismember(&child->mp_sigtrace, i)) {
        sigdelset(&child->mp_sigtrace, i); // 199
        check_sig(child->mp_pid, i, FALSE /* ksig */); // 200  ksig==FALSE
      }
    }
    if (m_in.m_lc_pm_ptrace.data > 0) { // 204  data>0→sig_proc
      sig_proc(child, m_in.m_lc_pm_ptrace.data, TRUE /*trace*/, FALSE /* ksig */); // 205-206
    }
    child->mp_flags &= ~TRACE_STOPPED; // 210  ~TRACE_STOPPED
    child->mp_trace_flags = 0; // 211  flags=0
    check_pending(child); // 213  pending 重检（13 的 PROC_STOPPED 双用途）
    break; // 215
```

`191-192` 行 `data<0||>=_NSIG→EINVAL` 的 `data` 信号边界，`194` 行 `tracer=NO_TRACER` 清哨兵后 `197-201` 行 `sigtrace→check_sig` 全量重放，`204-206` 行 `data>0→sig_proc` 的 `TRUE` 使 `TRACED` 进程的 `SIGKILL` 可经 `TRACE_STOPPED` 跳过，`210-211` 行 `~TRACE_STOPPED`/`flags=0` 清暂停与 `213` 行 `check_pending` 的 `PROC_STOPPED` 双用途重检。

### 2.11 `T_RESUME/T_STEP/T_SYSCALL`（`trace.c:217-242`）

```c
  case T_RESUME:
  case T_STEP:
  case T_SYSCALL: // 217-219  resume execution
    if (m_in.m_lc_pm_ptrace.data < 0 || m_in.m_lc_pm_ptrace.data >= _NSIG) return(EINVAL); // 220-221  data 边界
    if (m_in.m_lc_pm_ptrace.data > 0) { // 223  data>0→sig_proc
      sig_proc(child, m_in.m_lc_pm_ptrace.data, FALSE /*trace*/, FALSE /* ksig */); // 224-226  FALSE 使 TRACE_STOPPED 跳过
    }
    for (i = 1; i < _NSIG; i++) { // 231  sigtrace 全量 OK 短路
      if (sigismember(&child->mp_sigtrace, i)) {
        mp->mp_reply.m_pm_lc_ptrace.data = 0; // 233
        return(OK); // 234  假成功（sigtrace 非空时 feign resumption）
      }
    }
    child->mp_flags &= ~TRACE_STOPPED; // 238  ~TRACE_STOPPED
    check_pending(child); // 240  pending 重检
    break; // 242
```

`220-221` 行 `data` 边界与 `T_DETACH:191-192` 同谓词，`223-226` 行 `data>0→sig_proc` 的 `FALSE` 使 `TRACE_STOPPED` 跳过，`231-236` 行 `sigtrace` 全量 `OK` 短路（`if sigtrace→OK` 的 `T_RESUME` 假成功），`238` 行 `~TRACE_STOPPED` + `240` 行 `check_pending` 的 `PROC_STOPPED` 双用途重检。

### 2.12 `sys_trace` 透传（`trace.c:244-249`）

```c
  r = sys_trace(req, child->mp_endpoint, m_in.m_lc_pm_ptrace.addr, &m_in.m_lc_pm_ptrace.data); // 244  sys_trace 透传 T_OK/ATTACH 外的 DATA/INS/USER 等
  if (r != OK) return(r); // 246
  mp->mp_reply.m_pm_lc_ptrace.data = m_in.m_lc_pm_ptrace.data; // 248  reply.data 回填
  return(OK); // 249
```

`244` 行 `sys_trace(req, endpoint, addr, &data)` 透传 `T_OK/ATTACH` 外的 `DATA/INS/USER` 等。

### 2.13 `trace_stop`（`trace.c:256-276`）

```c
void trace_stop(register struct mproc *rmp, int signo)
{ // 256  A traced process got a signal so stop it.
  register struct mproc *rpmp = mproc + rmp->mp_tracer; // 260  rpmp = tracer 的 mproc
  int r; // 261
  r = sys_trace(T_STOP, rmp->mp_endpoint, 0L, (long *) 0); // 263  sys_trace(T_STOP) 先于置位
  if (r != OK) panic("sys_trace failed: %d", r); // 264
  rmp->mp_flags |= TRACE_STOPPED; // 266  TRACE_STOPPED 置位（mproc.h:93）
  if (wait_test(rpmp, rmp)) { // 267  wait_test(rpmp,rmp) → ~sigtrace(signo) + ~WAITING + W_STOPCODE + reply(tracer, pid)
    sigdelset(&rmp->mp_sigtrace, signo); // 270  ~sigtrace(signo)（sigtrace 位图清当前信号）
    rpmp->mp_flags &= ~WAITING; // 272  ~WAITING（parent 不再等待）
    rpmp->mp_reply.m_pm_lc_wait4.status = W_STOPCODE(signo); // 273  W_STOPCODE(signo) 的 0x7f 截断（wait.h: W_STOPCODE）
    reply(rmp->mp_tracer, rmp->mp_pid); // 274  reply(tracer, pid)（wait4 的 tracer 分支 tell_tracer 的 W_STOPCODE 的 0x7f 截断同位）
  }
}
```

`263` 行 `sys_trace(T_STOP)` 先于 `266` 行 `TRACE_STOPPED` 置位（`T_STOP` 的 `sys_trace` 先于置位与 `trace.c:263` 先于 `266` 同序，`D8` `TraceStop::stop` 同序），`267-275` 行 `wait_test` 的 `W_STOPCODE` 通知 `tracer` 的 `wait4`（`wait.h: W_STOPCODE` 的 `0x7f` 截断与 `wait.rs: W_STOPCODE` 同位）。

### 2.14 消息与类型（`sys/ptrace.h:226` `T_OK 0` 等 `T_*` + `sys/wait.h: W_STOPCODE` + `sys/signal.h: _NSIG 64`）

- `T_OK 0` (`PT_TRACE_ME`) / `T_ATTACH 9` (`PT_ATTACH`) / `T_STOP` 等 `T_*`（`ptrace.h:226-234` `T_OK/ATTACH/STOP/READB_INS/WRITEB_INS/EXIT/SETOPT/GETRANGE/SETRANGE/DETACH/RESUME/STEP/SYSCALL`，`sys/ptrace.h:226` `T_OK 0` 等）、`W_STOPCODE(signo)` (`wait.h: W_STOPCODE` 的 `((signo)<<8 | 0x7f)` 截断，`trace.c:273`）、`_NSIG 64`（`sys/signal.h:45`）、`TRACE_STOPPED 0x80`（`mproc.h:93`）、`TO_NOEXEC/ALTEXEC`（`sys/ptrace.h: TO_NOEXEC 0x1/ALTEXEC 0x2`）。

### 2.15 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `T_OK` 的 `tracer==NO_TRACER` 守卫 | `trace.c:56` `tracer!=NO_TRACER→EBUSY` | 已被追踪→EBUSY | 可恢复 `EBUSY` |
| `T_ATTACH` 的 `SUPER_USER` 三重 | `trace.c:67-71` | `eff!=SUPER_USER && (eff!=child_eff || ...)` → `EPERM` | 可恢复 `EPERM` |
| `T_EXIT` 的 `VFS|EVENT→save` | `trace.c:150-151` | `VFS|EVENT→save exitstatus` | 不变量 |
| `T_DETACH` 的 `sigtrace→check_sig` 全量重放 | `trace.c:197-201` | `sigtrace→check_sig` 全量 | 不变量 |
| `TRACE_STOPPED` 的 `sys_trace(T_STOP)` 先于置位 | `trace.c:263` 先于 `266` | `T_STOP` 先于置位 | 不变量（时序） |
| `W_STOPCODE` 的 `0x7f` 截断 | `wait.h: W_STOPCODE` | `W_STOPCODE(signo)` 的 `0x7f` 截断 | 不变量 |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `T_*` 枚举 + `may_attach` 一处谓词 + `TraceState { stopped, exit_pending }` 双 bool + `replay_sigtrace` + `WaitCode::stop` 枚举的 `0x7f` 截断”的 8 决策，保留 C 的 `T_OK/ATTACH` 双入口与 `TRACE_STOPPED` 独立暂停，但以类型系统使 `T_OK` 的 `EBUSY` 守卫与 `T_ATTACH` 的 `SUPER_USER` 三重显式化。以下决策对应设计契约 `.design/18-design.v1.md` 的 D1–D8。

### D1：`T_OK` 的 `tracer!=NO_TRACER→EBUSY` 收敛到 `try_set_tracer`（ARCH A-12）

- **C**：`56` `tracer!=NO_TRACER→EBUSY` else `tracer=parent`。
- **Rust**：`fn try_set_tracer(&mut self, parent: UserSlot) -> Result<(), TraceError>`（`if tracer.is_some()→EBUSY` else `Traced { parent, tracer: parent }`，`A-12`）。

### D2：`T_ATTACH` 的三重判据收敛到 `may_attach`（ARCH A-12）

- **C**：`67-91` 7 守卫。
- **Rust**：`fn may_attach(caller: &Credentials, target: &Process) -> Result<(), TraceError>`（`SUPER_USER` 三重 + `PRIV_PROC` 双向禁 + `self/PM/VM→EPERM` + `already traced→EBUSY` 一处谓词）。

### D3：`T_EXIT` 的 `TRACE_EXIT` 哨兵收敛到 `TraceExitState`（ARCH A-12）

- **C**：`147` 置位 + `150-151` `VFS|EVENT→save` 否则 `exit_proc`。
- **Rust**：`enum TraceExitState { Pending { status }, Immediate { status } }` + `fn set_trace_exit(table, child, status)`。

### D4：`T_SETOPT` 收敛到 `set_trace_options`（ARCH A-12）

- **C**：`162` `trace_flags=data`。
- **Rust**：`fn set_trace_options(&mut self, bits: u32)`（`TraceOptions::from_bits_truncate`）。

### D5：`T_GETRANGE/T_SETRANGE` 的 `TS_INS/DATA` 校验收敛到 `PtraceRange`（ARCH A-11）

- **C**：`173` `TS_INS/DATA→EINVAL` + `174` `size 0||>LONG_MAX→EINVAL` + `177-183` `sys_vircopy` 双向。
- **Rust**：`struct PtraceRange { space: TsSpace, addr, ptr, size }` + `enum TsSpace { Ins, Data }` + `RangeCtl::vircopy` trait。

### D6：`T_DETACH` 的 `sigtrace→check_sig` 全量重放收敛到 `replay_sigtrace`（ARCH A-12）

- **C**：`197-201` `for i if sigtrace→check_sig` + `204-206` `data>0→sig_proc`。
- **Rust**：`fn replay_sigtrace(table, child)` 全量重放。

### D7：`T_RESUME/T_STEP/T_SYSCALL` 的 `sigtrace` 短路收敛到 `maybe_short_circuit`（ARCH A-12）

- **C**：`231-236` `if sigtrace→OK` 假成功。
- **Rust**：`fn maybe_short_circuit(sigtrace: SigSet) -> bool`。

### D8：`trace_stop` 的 `W_STOPCODE` 收敛到 `WaitCode::stop`（ARCH A-2）

- **C**：`273` `W_STOPCODE(signo)` 的 `0x7f` 截断。
- **Rust**：`enum WaitCode { Stop(i32) }` + `fn stop_code(signo: i32) -> i32`。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-12 双监护外 `Traced` | `Guardianship::Traced` + `try_set_tracer`（D1） | `mproc/guardianship.rs` + 本文档 §3.1 + 计划 §4 |
| A-2 位→枚举 | `T_*` 枚举（D1/D2） | `trace.rs` + 本文档 §3.1/3.2 + 计划 §4 |
| A-3 全局→显式 | `TraceCtl::trace` 显式 `caller: UserSlot`（D1/D2） | `trace.rs` 注释 + 本文档 §3.1/3.2 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   ├── trace.rs         — TraceState { stopped, exit_pending } + TraceFlags 已在 guardianship.rs 的 TraceOptions
│   └── guardianship.rs  — Guardianship::try_set_tracer + set_trace_options + clear_tracer
├── trace.rs             — do_trace(table, caller, req, &mut dyn TraceCtl) -> Result<ReplyIntent, TraceError>（T_* 全族 16 命令的 match 分派 + TraceStop 的 W_STOPCODE） + trace_stop(table, child, signo, &mut dyn TraceCtl, &mut dyn WaitReply)
└── ipc/
    └── mod.rs           — （无新增，trace 的 sys 调用经 trace.rs 的 trait 注入）
```

### 4.2 `mproc/trace.rs` & `guardianship.rs`：`TraceState` 三元

```rust
pub struct TraceState { pub stopped: bool, pub exit_pending: bool } // TRACE_STOPPED/TRACE_EXIT
impl Guardianship {
    pub fn try_set_tracer(&mut self, parent: UserSlot) -> Result<(), TraceError> // T_OK
    pub fn set_trace_options(&mut self, bits: u32) // T_SETOPT
    pub fn clear_tracer(&mut self) // T_DETACH
}
```

### 4.3 `trace.rs`：`T_*` 全族分派

```rust
pub enum PtraceReq { Ok, Attach { pid }, Stop, ReadIns { pid, addr }, WriteIns { pid, addr, data }, Exit { pid, status }, SetOpt { pid, flags }, GetRange { pid, range }, SetRange { pid, range }, Detach { pid, sig }, Resume { pid, sig }, Step { pid, sig }, Syscall { pid, sig } }
pub trait TraceCtl { fn trace(&mut self, req: i32, ep: Endpoint, addr: VirBytes, data: &mut u64) -> i32; fn vircopy(&mut self, from: Endpoint, from_addr, to: Endpoint, to_addr, size) -> i32; }
pub fn do_trace(table: &mut ProcTable, caller: UserSlot, req: PtraceReq, ctl: &mut dyn TraceCtl) -> Result<ReplyIntent, TraceError>
pub fn trace_stop(table: &mut ProcTable, child: UserSlot, signo: i32, ctl: &mut dyn TraceCtl, wait: &mut dyn WaitReply) // sys_trace(T_STOP) + TRACE_STOPPED + W_STOPCODE
```

### 4.4 `os/libs/minix-types/src/ipc/trace.rs`（或 `pm.rs` 扩展）：`PtraceReq` 常量与 `TsSpace` 枚举

```rust
pub const T_OK: i32 = 0; pub const T_ATTACH: i32 = 9; // ptrace.h:226-234
pub enum TsSpace { Ins = 0, Data = 1 }
pub struct PtraceRange { pub space: TsSpace, pub addr: VirBytes, pub ptr: VirBytes, pub size: usize }
```

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `T_OK` 的 `tracer==NO_TRACER` 守卫 | `trace.c:56` | `try_set_tracer` | `test_t_ok_ebusy` |
| 2 | `T_ATTACH` 的 `SUPER_USER` 三重 | `trace.c:67-71` | `may_attach` | `test_t_attach_perm` |
| 3 | `T_EXIT` 的 `VFS|EVENT→save` | `trace.c:150-151` | `TraceExitState::Pending` | `test_t_exit_save` |
| 4 | `T_DETACH` 的 `sigtrace→check_sig` 全量 | `trace.c:197-201` | `replay_sigtrace` | `test_t_detach_replay` |
| 5 | `TRACE_STOPPED` 的 `sys_trace(T_STOP)` 先于置位 | `trace.c:263` 先于 `266` | `TraceStop::stop` | `test_trace_stop_wait` |
| 6 | `W_STOPCODE` 的 `0x7f` 截断 | `wait.h: W_STOPCODE` | `WaitCode::stop` | `test_w_stopcode` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **295 passed / 0 failed**（原 285 + 本档新增 ~10：`trace.rs` 8 + `mproc/trace.rs` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `trace.rs`（`T_*` 全族与 `trace_stop`）

- `test_t_ok_ebusy`：`T_OK` 的 `EBUSY` 门（`trace.c:56`）
- `test_t_attach_perm`：`T_ATTACH` 的 `SUPER_USER` 三重/`PRIV_PROC` 双向禁/`already traced→EBUSY`（`67-85`）
- `test_t_exit_save`：`VFS|EVENT→save` vs `exit_proc`（`150-151`）
- `test_t_detach_replay`：`sigtrace→check_sig` 全量（`197-201`）
- `test_trace_stop_wait`：`sys_trace(T_STOP)` + `TRACE_STOPPED` + `W_STOPCODE` + `wait_test`（`263/266/273`）

### 5.2 `mproc/trace.rs` 与 `mproc/guardianship.rs`（三元）

- `test_trace_state_default`：`TraceState { stopped, exit_pending }` 默认 `false`
- `test_try_set_tracer`：`try_set_tracer` 的 `EBUSY` 门

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `T_OK 0/T_ATTACH 9`（`ptrace.h:226`）、`W_STOPCODE` 的 `0x7f` 截断（`wait.h`）

测试策略：`TraceCtl` 的 `trace/vircopy` 均 `TestTraceCtl` mock 可注入 `OK/EPERM/ESRCH` 与 `data` 回填；`W_STOPCODE` 的 `0x7f` 截断以 `WaitCode::stop` 纯函数验；`T_EXIT` 的 `VFS|EVENT` 分叉以 `ProcTable` 的 `VFS_CALL` 置位验。

---

## 6 过渡

本篇在 `do_trace` 的 `T_*` 全族与 `trace_stop` 的 `W_STOPCODE` 之间，是 `10` 的 `wait4` 的 `tracer` 分支与 `09` 的 `tracer_died` 的 `TRACE_EXIT` 消费方衔接；`T_RESUME` 的 `check_pending` 为 `13` 的 `PROC_STOPPED` 重检前置：

```
10-pm-wait.md（wait4 的 tracer 分支 tell_tracer 的 W_STOPCODE 的 0x7f 截断）
  │
  └─► 本章（do_trace 的 T_OK/ATTACH 自声明/主动附着 + T_EXIT 的 TRACE_EXIT 哨兵 + T_DETACH 的 sigtrace 重放 + trace_stop 的 W_STOPCODE 暂停）
         │
         ├─► 09-pm-exit.md（tracer_died 的 TRACER_DEATH 消费，TRACED 转 Zombie→ToldParent）
         └─► 13-signal-flow.md（T_RESUME 的 check_pending 的 PROC_STOPPED 双用途重检）
```

`T_RESUME` 的 `check_pending` 为 `13` 的 `PROC_STOPPED` 双用途重检前置（`T_RESUME` 的 `~TRACE_STOPPED` 后 `check_pending` 的 `VFS→break` 前 `~TRACE_STOPPED` 的 `child` 上再投）。

阅读顺序提示：若想先理解“`tracer_died` 的 `TRACER_DEATH` 消费”，下一站 `09-pm-exit.md` 的 `forkexit.c:760-795` 的 `TRACED` 转 `Zombie→ToldParent`；若想理解“`wait4` 的 `tracer` 分支 `tell_tracer`”，下一站 `10-pm-wait.md` 的 `wait_test` 的 `TRACE_STOPPED` 分支。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/trace.c` 全文（`42-250` `do_trace` 等）、`minix3/minix/servers/pm/mproc.h:93`（`TRACE_STOPPED`）、`minix3/sys/sys/ptrace.h:226`（`T_OK 0` 等 `T_*`）、`minix3/sys/sys/wait.h: W_STOPCODE`（`0x7f` 截断）、`minix3/sys/sys/signal.h: _NSIG 64`
- PM 阶段文档：03-mproc-table.md（`find_proc`/`pm_isokendpt`）、10-pm-wait.md（`wait_test` 的 `TRACE_STOPPED` 分支与 `W_STOPCODE`）、11-signal-core.md（`sig_proc` 的 `TRACE` 先行 `sigtrace` 位图）、09-pm-exit.md（`tracer_died` 的 `TRACE_EXIT` 消费）、02-mproc-struct.md（`TraceState` 三元）
- 阶段内顺序：11 → 12 → 13 → **本章（18）** → 10（`wait4` 的 `tracer` 分支消费 `trace_stop` 的 `W_STOPCODE`）→ 09（`tracer_died` 的 `TRACE_EXIT` 消费）→ 02（`TraceState` 三元）
- OS 模式参考：Linux `ptrace(PTRACE_ATTACH)` 的 `may_ptrace_attach` + `WIFSTOPPED`（`kernel/ptrace.c`）、Redox `ptrace` 缺省（`Scheme` 的 `handle`）、`seL4` `TCB` 显式 `suspend`（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/trace.rs`（`TraceState { stopped, exit_pending }`）、`os/servers/pm/src/mproc/guardianship.rs`（`Guardianship::try_set_tracer`）、`os/servers/pm/src/trace.rs`（`PtraceReq` + `do_trace` 全族 + `trace_stop` 的 `W_STOPCODE`）

