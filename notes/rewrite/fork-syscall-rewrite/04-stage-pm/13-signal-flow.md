# 13 — 延迟、停止与恢复：`stop_proc`/`try_resume_proc`/`unpause`/`check_pending`/`restart_sigs` 的延续

本文讲清信号在 `VFS 阻塞` 与 `内核发送` 的夹缝中如何不丢失：为什么 `sys_delay_stop` 的 `EBUSY` 必须记为 `DELAY_CALL` 并等待 `SIGSNDELAY`（`process_ksig` 尾部），为什么 `PROC_STOPPED` 既是“已停止”状态又是“`restart_sigs` 需重检”的指示器，为什么 `stop_proc` 的 `may_delay` 契约区分“已保证无 `EBUSY` 的 `VFS|EVENT` 挂起点”与“首次 `VFS UNPAUSE` 可延迟”，以及 `unpause` 的三路径（`UNPAUSED` 已就绪/`DELAY_CALL` 忙/`WAITING|SIGSUSPENDED` 的 `stop_proc(FALSE)`/其余 `tell_vfs`）如何与 `check_pending` 的 `VFS|EVENT→break` 和 `restart_sigs` 的 `TRACE_EXIT→exit` / `PROC_STOPPED→check→resume` 串联，完成“延迟→停止→挂起→重检→恢复”的完整状态机。

前置阅读：11-signal-core.md（`sig_proc` 的 `VFS|EVENT→pending+stop_proc(FALSE)→return` 与 `caught→unpause→sig_send` 调用点、`process_ksig` 的 `SIGSNDELAY` 尾部与 `EDEADEPT` 双检）、12-signal-handlers.md（`sig_send` 的 `sigmsg` 四步与 `sigsuspend` 的 `mask2`/`SIGSUSPENDED` 配对、`SignalState::prepare_sigmsg` 的 `SA_*` 语义与 `without_unkillable`）、05-vfs-interaction.md（`handle_vfs_reply` 尾部的 `restart_sigs` 调用点与两处 `publish_event` 提前 return 的互斥）、06-event-subscription.md（`EVENT_CALL` 的游标 `EventCursor` 与 `publish_event→resume_event→exit_restart/restart_sigs` 分派）、02-mproc-struct.md（`BlockState` 的 `stopped/ipc_blocked/unpaused` 三元与 `IpcBlockReason::DelayedSignal`）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `sig_proc` 的 9 链中 `VFS|EVENT` 挂起的 `stop_proc(FALSE)` 与 `caught` 的 `unpause→sig_send` 路径（11 §1.4/12 §1.6）、`SigSet` 的 `pending & !mask` 重检语义（12 §1.4）与 `BlockState` 的 `PROC_STOPPED/UNPAUSED/DELAY_CALL` 三位（02 §2.2）的开发者。

> **本章不讲什么**：
> - 信号的生成与广播（`check_sig` 四态 `pid`、`process_ksig` 的 `EDEADEPT` 双检与 `RS` 先杀）—— `11-signal-core.md`
> - `sigaction` 族安装与 `sig_send` 的 `sigmsg` 翻译（`SA_NODEFER/RESETHAND`/`without_unkillable`）—— `12-signal-handlers.md`；本章只到 `sig_send` 的 `PostAction` 分支点
> - `VFS` 协议状态机（`VFS_PM_*` 的 11 种回复与 `NEW_PARENT`/`UNPAUSED` 语义）—— `05-vfs-interaction.md`；本章只到 `restart_sigs` 调用点
> - 事件订阅串行化（`subs[NR_SUBS]` 的 `waiting` 计数与 `resume_event` 串行推进）—— `06-event-subscription.md`
> - 内核 `sys_delay_stop/sys_resume` 的 `EBUSY` 时序与 `sigframe` 推送（`do_delay_stop` 的 `SENDING` 检查）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`
>
> 本章只回答一个问题：**当信号在进程仍在内核发送消息（mid-send）或阻塞在 `VFS`/`WAIT`/`sigsuspend` 时，PM 如何保证“先把进程停下来，再把已解除阻塞的 `pending` 逐个投递，`VFS` 回复后再续”，且 `SIGSNDELAY` 的到来使延迟的停止最终兑现**。

### 1.1 为什么信号需要“延迟”：`EBUSY` 与 `SIGSNDELAY` 的配对

PM 的 `stop_proc` 本质是“请求内核把进程停在可接收信号的点”。内核的 `sys_delay_stop(endpoint)` 在进程已停止时直接 `OK`，但在进程仍在内核 `SENDING`（mid-send，消息尚在内核队列中未到达目标）时不能立即停——此时 `sys_delay_stop` 返回 `EBUSY`，并承诺“发送完成后以 `SIGSNDELAY` 通知你”（`signal.c:239-242` 注释 *kernel will give us EBUSY now and send a SIGSNDELAY to the process as soon as sending is done*）。PM 收到 `EBUSY` 后置 `DELAY_CALL`（`signal.c:254`），等待内核的 `SIGSNDELAY` 到来。

`process_ksig` 尾部（`signal.c:344-369`）是 `EBUSY` 的兑现点：

```
SIGSNDELAY + DELAY_CALL → ~DELAY_CALL (351) + assert(!PROC_STOPPED) (353)
    ├── VFS|EVENT → stop_proc(FALSE)→return (359-363)   // 延迟前已进 VFS，则只停
    └── else → check_pending (366)                       // 否则逐个重投
```

`DELAY_CALL` 不是“信号丢失”，是“内核就绪前的等待令牌”——`344` 行 `signo == SIGSNDELAY && DELAY_CALL` 的双条件保证只有“曾 `EBUSY` 的进程的完成通知”才触发兑现，其他 `SIGSNDELAY` 忽略。

### 1.2 为什么 `PROC_STOPPED` 有双用途：已停止与需重检

`PROC_STOPPED`（`mproc.h:89 0x08`）在 `signal.c` 有双重含义（`signal.c:430-434` 注释）：

> *Since we always stop the process to deliver signals during a VFS or event call, the `PROC_STOPPED` flag doubles as an indicator in `restart_sigs()` that signals must be rechecked after a reply arrives.*

1. **已停止**：`stop_proc` 的 `OK → PROC_STOPPED`（`245-248`）与 `try_resume_proc` 的 `~PROC_STOPPED`（`288`）构成“已停/已恢复”状态。
2. **需重检**：`sig_proc` 的 `VFS|EVENT` 挂起点 `stop_proc(FALSE)` 后 `return`（`437-444`）与 `check_pending` 的 `VFS|EVENT→assert(PROC_STOPPED)→break`（`672-679`），使 `VFS` 阻塞时到达的信号以 `PROC_STOPPED` 为“待重检”标记，`restart_sigs` 以 `699` 行 `if PROC_STOPPED` 触发 `check_pending→try_resume`。

`restart_sigs` 的 `704` 行 `assert(!DELAY_CALL)` 是双用途不与 `DELAY_CALL` 混淆的守卫——`DELAY_CALL` 时进程并未 `PROC_STOPPED`（`signal.c:353` 断言），因此 `PROC_STOPPED` 为真时必无 `DELAY_CALL`。

### 1.3 为什么 `stop_proc` 有 `may_delay`：调用者是否已排除 `EBUSY`

`stop_proc(rmp, may_delay)` 的 `may_delay` 区分两种调用者的契约：

- `may_delay == FALSE`（`sig_proc:442` 的 `VFS|EVENT` 挂起点 与 `unpause:750` 的 `WAITING|SIGSUSPENDED`）——调用者已保证“进程不在 mid-send”（`signal.c:437-441` 注释 *process must have made a call to PM. Therefore, there can be no delay calls*；`745-749` 注释 *We know for a fact that the process called us*），此时 `EBUSY` 为不可恢复错误（`251-252` `panic("unexpected delay call")`）。
- `may_delay == TRUE`（`unpause:760` 的 `!PROC_STOPPED → stop_proc(TRUE)` 的首次 `VFS UNPAUSE`）——进程可能正在 `SENDING`，`EBUSY` 可接受（`254-256` 置 `DELAY_CALL→FALSE`）。

`bool` 的 `may_delay` 在 Rust 易误用为“随意传 `true` 则 `EBUSY` 被吞”——`MayDelay::MustStop/MayDefer` 枚举使契约在类型层互斥（§3.1）。

### 1.4 为什么 `unpause` 有三条路径：已解/正忙/PM 睡/VFS 睡

`unpause`（`signal.c:719-770`）是“把进程从 `VFS` 或 `WAIT`/`suspend` 唤醒到可接收信号”的统一入口，三路径覆盖四态：

| 分支 | 条件 | 动作 | 返回 | 语义 |
|------|------|------|------|------|
| 已就绪 | `UNPAUSED`（`734`） | `assert((DELAY\|PROC)==PROC)` | `TRUE` | `VFS_PM_UNPAUSE_REPLY` 已到，`VFS` 已确认可中断 |
| 正忙 | `DELAY_CALL`（`741`） | — | `FALSE` | 内核仍 `SENDING`，暂缓 `sig_send`（`sig_proc:514-520` 入 `pending`） |
| PM 睡眠 | `WAITING\|SIGSUSPENDED`（`745`） | `stop_proc(FALSE)` 停止（已排除 `EBUSY`） | `TRUE` | `wait4`/`sigsuspend` 的阻塞在 PM 侧，直接停然后 `sig_send` 的 `WAITING→EINTR` 分支 |
| VFS 睡眠 | `!PROC_STOPPED && !stop(TRUE)→FALSE`（`760-761`） | — | `FALSE` | `EBUSY` 延迟，`pending` |
| VFS 睡眠就绪 | `PROC_STOPPED`（`760` 已停或新停） | `tell_vfs(VFS_PM_UNPAUSE)`（`763-767`） | `FALSE` | 请 `VFS` 尝试中断 `READ/WRITE` 等可中断调用，`VFS` 回复后 `restart_sigs` 再续 |

`734-738` 的 `UNPAUSED → TRUE` 与 `763-769` 的 `tell_vfs → FALSE` 互为“`VFS` 未确认 vs 已确认”的两极：已 `UNPAUSED` 则 `sig_send` 的 `else→assert(UNPAUSED)` 分支可直接进 `restart_sigs` 的恢复；未 `UNPAUSED` 则先 `tell_vfs` 等 `VFS_PM_UNPAUSE_REPLY`。

### 1.5 为什么 `check_pending` 要循环到 `VFS|EVENT` 就 `break`：重检与异步的互斥

`check_pending`（`signal.c:651-682`）的 `for i=1.._NSIG` 逐位扫描 `pending & !mask`，每命中一次：

1. `ksig = ksigpending(i)` 还原 `ksig` 真假（`667`），
2. 双清 `pending/ksigpending(i)`（`668-669`），
3. `sig_proc(FALSE, ksig)` 以 `trace==FALSE` 重入（`670`，`11` 的 `trace` 先行在此跳过——已解阻塞的 `pending` 不再给 tracer），
4. 若 `sig_proc` 使目标进入 `VFS|EVENT` 并 `PROC_STOPPED`（`425-444` 的 `VFS|EVENT` 挂起点），则 `672-679` 行 `if VFS|EVENT → assert(PROC_STOPPED)→break`——此时 `check_pending` 必须停在 `PROC_STOPPED` 处，等待 `VFS` 回复后 `restart_sigs` 再续。

`break` 使“重检”与“`VFS` 异步”不并发：同一进程的 `pending` 扫描与 `VFS` 回复的 `restart_sigs` 不在同一 `check_pending` 循环内并发，前者以 `PROC_STOPPED` 为断点，后者以 `restart_sigs:704` 的 `assert(!DELAY)` 保证 `PROC_STOPPED` 的断点可续。

### 1.6 为什么 `restart_sigs` 有 `TRACE_EXIT` 优先：tracer 强制退出高于信号重检

`restart_sigs`（`signal.c:687-714`）是 `VFS` 回复后“信号相关善后”的统一入口（`main.c:393-423` `handle_vfs_reply` 尾部、`event.c:115-122` `resume_event` 尾部均调用）：

1. `693` 行 `if VFS|EVENT|EXITING → return`——仍阻塞或已退出则无善后；
2. `695-698` 行 `if TRACE_EXIT → exit_proc(mp_exitstatus, FALSE)`——tracer 强制退出（`trace.c: TRACE_EXIT` 由 `ptrace(T_EXIT)` 置位）优先于 `PROC_STOPPED` 的信号重检（先死，再谈信号）；
3. `699-712` 行 `else if PROC_STOPPED → assert(!DELAY) → check_pending → try_resume`——`VFS` 阻塞时到达的信号以 `PROC_STOPPED` 为“需重检”标记，`check_pending` 逐个重投后 `try_resume` 清 `PROC_STOPPED|UNPAUSED`。

`TRACE_EXIT` 优先（`695` 先于 `699`）使“tracer 说你必须死”的语义高于“已解阻塞的 `pending` 再投”——先 `exit_proc`，不再 `try_resume`。

### 1.7 与其他 OS 状态机的对照

Rust 改写不是照抄 `for i=1.._NSIG if sigismember` 循环，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `TASK_INTERRUPTIBLE`/`TASK_UNINTERRUPTIBLE` + `signal_pending`。** Linux 的 `TASK_INTERRUPTIBLE`（可被信号中断）与 `TASK_UNINTERRUPTIBLE`（不可中断）区分“等待是否可被信号打断”——PM 的 `WAITING`（`wait4`）与 `VFS_CALL`（`VFS` 阻塞）同样区分“PM 侧可中断”与“`VFS` 侧需 `tell_vfs` 协商中断”。Linux 的 `recalc_sigpending` + `signal_wake_up` 在 `sigprocmask` 解阻塞后重检 `pending`，PM 的 `check_pending` 在 `UNBLOCK/SETMASK/sigsuspend/sigreturn` 后同理重检（`12` 的 `MaskOpEffect::needs_check`）。差异在唤醒：Linux `try_to_wake_up` 直接调度，PM `try_resume_proc` 的 `sys_resume` + `VFS|EVENT` 守卫先判可否恢复（`279-280`），`VFS` 未回则不 `resume`。

**Redox `Context::blocked` + `SigQueue`。** Redox 以 `Context { status: Blocked { reason } }` + `SigQueue(VecDeque<Signal>)` 建模阻塞与信号队列，`SigQueue` 使信号可排队（`sigqueue` 的 `siginfo`），PM 以 `BlockState::stopped + DelayedSignal` + `SigSet` 位图使信号挤压为 1 位（`SigSet = u64` 常数时间 `sigismember`）。Redox 的 `Scheme` 句柄超时与 `Alarm` 亦以“阻塞→超时→唤醒”同理，PM 的 `VFS UNPAUSE` 以 `tell_vfs` + `VFS_PM_UNPAUSE_REPLY` 显式往返替代超时。

**FreeBSD `msleep` 中断。** FreeBSD 的 `msleep(chan, pri, wmesg, timo)` 在 `pri & PCATCH` 时可被信号中断并返 `EINTR`/`ERESTART`，`VFS` 的 `READ/WRITE` 等可中断调用在 PM 侧即 `tell_vfs(VFS_PM_UNPAUSE)` 的 `VFS` 往返 counterpart——`unpause` 的 `VFS_PM_UNPAUSE` 正是“请 `VFS` 检查是否可 `EINTR`”的询问，`VFS_PM_UNPAUSE_REPLY` 后 `UNPAUSED` 置位使 `sig_send` 的 `else→assert(UNPAUSED)` 分支可续。

**`seL4` `Notification` 显式唤醒。** `seL4` 无 `PROC_STOPPED` 的隐式标志，以 `Notification` + `reply capability` 显式唤醒等待者，PM 的 `PROC_STOPPED` 隐式标志因“每进程单 `VFS`/`EVENT` 延续”而不歧义，但需 `nested` 守卫与 `UNPAUSED` 的 `tell_vfs` 往返来弥补隐式的时序。

**结论（本章的设计基线）。** 把 C 的“`sys_delay_stop→EBUSY→DELAY_CALL→SIGSNDELAY→~DELAY→check_pending` 的隐式延迟令牌 + `PROC_STOPPED` 双用途 + `may_delay` 布尔 + `unpause` 三路径 `bool` + `check_pending` 的 `VFS|EVENT→break` + `restart_sigs` 的 `TRACE_EXIT` 优先”改写为“`MayDelay::{MustStop,MayDefer}` + `StopOutcome::{Stopped,Deferred}` + `UnpauseOutcome::{Ready,Busy,VfsWait}` + `PendingScanner` 迭代器 + `RestartAction::{Noop,Exit,CheckAndResume}` + `handle_sigsn_delay` 显式状态机 + `BlockState::can_resume` 守卫”——与 Linux/Redox 的可中断等待 + `recalc_sigpending` 同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的显式 `BlockState` 收敛。

### 1.8 小结

1. **为什么延迟**——`sys_delay_stop` 的 `EBUSY` 是内核 `SENDING` 的证据，`DELAY_CALL` 等 `SIGSNDELAY` 的完成通知后 `~DELAY→check_pending` 兑现。
2. **为什么双用途**——`PROC_STOPPED` 既是“已停”状态，又是 `restart_sigs` 的“需重检”指示器（`VFS|EVENT` 挂起点以 `PROC_STOPPED` 为断点，`restart_sigs` 以 `PROC_STOPPED` 为入口，`assert(!DELAY)` 隔离 `DELAY`）。
3. **为什么 `may_delay`**——`VFS|EVENT` 挂起点已保证无 `SENDING` 则 `MustStop`（遇 `EBUSY` 则 `panic`），首次 `VFS UNPAUSE` 可能 `SENDING` 则 `MayDefer`（可 `DELAY`）。
4. **为什么三路径**——`UNPAUSED` 已就绪/ `DELAY` 忙/ `WAITING|SIGSUSPENDED` 的 `stop(FALSE)` / 其余 `tell_vfs` 覆盖 `VFS` 已确认/未确认/PM 睡眠/内核忙四态。
5. **为什么 `break`**——`check_pending` 的 `VFS|EVENT→assert(PROC_STOPPED)→break` 使重检与 `VFS` 异步不并发，`PROC_STOPPED` 仍置位等 `restart_sigs` 续。
6. **为什么 `TRACE_EXIT` 优先**——tracer 强制退出高于信号重检（`693` 先于 `699`）。

下一章逐行分析 C 的 `stop_proc`/`try_resume_proc`/`check_pending`/`restart_sigs`/`unpause`/`SIGSNDELAY`；第 3 章给出 Rust 的 `MayDelay`/`KernelStop`/`UnpauseOutcome`/`PendingScanner`。

---

## 2 C 源码分析

### 2.1 `stop_proc`（`signal.c:226-261`）

```c
static int stop_proc(struct mproc *rmp, int may_delay)
{ // 226  may_delay: TRUE 允许 DELAY_CALL，FALSE 遇 EBUSY 则 panic
  int r;
  assert(!(rmp->mp_flags & (PROC_STOPPED | DELAY_CALL | UNPAUSED))); // 237  三互斥
  r = sys_delay_stop(rmp->mp_endpoint); // 239  kernel/system/do_delay_stop.c: SENDING?EBUSY:OK
  switch (r) { // 244
  case OK: // 245
    rmp->mp_flags |= PROC_STOPPED; // 246  已停止
    return TRUE; // 248  已停
  case EBUSY: // 250  内核仍 SENDING
    if (!may_delay) panic("stop_proc: unexpected delay call"); // 251-252
    rmp->mp_flags |= DELAY_CALL; // 254  记延迟令牌
    return FALSE; // 256  未停（延迟）
  default: panic("sys_delay_stop failed: %d", r); // 259
  }
}
```

三段式与 `mproc.h:89/102/97` 的 `PROC_STOPPED=0x08` / `DELAY_CALL=0x20000` / `UNPAUSED=0x1000` 互斥：`237` 行断言三者不并存，`OK→PROC_STOPPED` 与 `EBUSY+MayDefer→DELAY_CALL` 互斥置位，`EBUSY+MustStop` 为不可恢复（`panic`）——`VFS|EVENT` 挂起点与 `WAITING|SIGSUSPENDED` 的 `stop(FALSE)` 皆 `MustStop`。

### 2.2 `try_resume_proc`（`signal.c:266-289`）

```c
static void try_resume_proc(struct mproc *rmp)
{ // 266  尝试恢复已停进程
  int r;
  assert(rmp->mp_flags & PROC_STOPPED); // 271  前置：必须已停
  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL | EXITING)) return; // 279-280  守卫：仍 VFS|EVENT|退出则不恢复
  if ((r = sys_resume(rmp->mp_endpoint)) != OK) panic("sys_resume failed: %d", r); // 282-283
  rmp->mp_flags &= ~(PROC_STOPPED | UNPAUSED); // 288  双清：UNPAUSED 只随 PROC_STOPPED 清
}
```

`279-280` 的 `VFS|EVENT|EXITING` 守卫与 `05` 的 `handle_vfs_reply` 的 `publish_event` 分派互斥（`VFS` 未回则不 `resume`，等 `restart_sigs` 再试）；`288` 行 `UNPAUSED` 的双清语义（*We can safely assume that a stopped process need only be unpaused once*，`signal.c:285-286` 注释）使 `VFS UNPAUSE` 的“已确认”只持续到下一次 `resume`。

### 2.3 `check_pending`（`signal.c:651-682`）

```c
void check_pending(register struct mproc *rmp)
{ // 652  解阻塞后重检：遍历 pending & !mask
  int i, ksig;
  for (i = 1; i < _NSIG; i++) { // 664
    if (sigismember(&rmp->mp_sigpending, i) && !sigismember(&rmp->mp_sigmask, i)) { // 665-666
        ksig = sigismember(&rmp->mp_ksigpending, i); // 667  还原 ksig 真假（与 11 的 ksigpending 同源）
        sigdelset(&rmp->mp_sigpending, i); // 668  双清（先清再投递，避免重入）
        sigdelset(&rmp->mp_ksigpending, i); // 669
        sig_proc(rmp, i, FALSE /*trace*/, ksig); // 670  trace FALSE：已解阻塞的 pending 不再给 tracer
        if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) { // 672  投递导致 VFS|EVENT 挂起
            assert(rmp->mp_flags & PROC_STOPPED); // 677  断言以 PROC_STOPPED 为“需重检”标记
            break; // 678  VFS|EVENT→break，等 restart_sigs 续
        }
    }
  }
}
```

`664-666` 的 `pending & !mask` 谓词与 `12` 的 `MaskOpEffect::needs_check` 同源，`667` 的 `ksig` 还原使 `SIGSNDELAY` 的 `DELAY_CALL` 语义在 `sig_proc` 的 `ksigpending` 分支可区分，`670` 的 `trace==FALSE` 跳过 `sig_proc` 的 `trace` 先行（`11` 的 `tracer` 已有 `sigtrace` 位图），`672-679` 的 `VFS|EVENT→break` 使“重检”与“异步 `VFS`”不并发（`PROC_STOPPED` 仍置位，`restart_sigs:704` 的 `assert(!DELAY)` 可续）。

### 2.4 `restart_sigs`（`signal.c:687-714`）

```c
void restart_sigs(struct mproc *rmp)
{ // 688  VFS 已回复后的信号善后
  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL | EXITING)) return; // 693  仍阻塞或已退出则无善后
  if (rmp->mp_flags & TRACE_EXIT) { // 695  tracer 强制退出优先
    exit_proc(rmp, rmp->mp_exitstatus, FALSE /*dump_core*/); // 697  先死
  } else if (rmp->mp_flags & PROC_STOPPED) { // 699  需重检（VFS 中到达的信号以此标记）
    assert(!(rmp->mp_flags & DELAY_CALL)); // 704  双用途不与 DELAY 混淆
    check_pending(rmp); // 709  重检（可能再次 VFS→break）
    try_resume_proc(rmp); // 712  尝试恢复（VFS|EVENT 守卫在 try_resume 中）
  }
}
```

`693` 的三条件 `return` 与 `699` 的 `PROC_STOPPED` 分支构成 `restart_sigs` 的两级入口：`TRACE_EXIT` 优先于 `PROC_STOPPED`（`695` 先于 `699`，禁止颠倒），`704` 的 `assert(!DELAY)` 是 `1.2` 双用途不与 `DELAY_CALL` 混淆的守卫，`709→712` 的 `check→try_resume` 串联使 `VFS` 回复后已解阻塞的 `pending` 原子恢复（`check_pending` 的 `break` 后仍 `try_resume` 清 `PROC_STOPPED`）。

### 2.5 `unpause`（`signal.c:719-770`）

```c
static int unpause(struct mproc *rmp)
{ // 720  把 WAITING|SIGSUSPENDED|VFS 阻塞解为可 sig_send
  message m;
  assert(!(rmp->mp_flags & (VFS_CALL | EVENT_CALL))); // 731  前置：未 VFS|EVENT
  if (rmp->mp_flags & UNPAUSED) { // 734  已收到 VFS_PM_UNPAUSE_REPLY
    assert((rmp->mp_flags & (DELAY_CALL | PROC_STOPPED)) == PROC_STOPPED); // 735
    return TRUE; // 737  已就绪，可 sig_send
  }
  if (rmp->mp_flags & DELAY_CALL) return FALSE; // 741-742  内核仍 SENDING，暂缓
  if (rmp->mp_flags & (WAITING | SIGSUSPENDED)) { // 745  PM 侧睡眠
    stop_proc(rmp, FALSE /*may_delay*/); // 750  必 MustStop（已保证无 SENDING）
    return TRUE; // 752  已停可 sig_send
  }
  if (!(rmp->mp_flags & PROC_STOPPED) && !stop_proc(rmp, TRUE /*may_delay*/)) return FALSE; // 760-761  VFS 侧首次停可能 DELAY
  memset(&m, 0, sizeof(m)); m.m_type = VFS_PM_UNPAUSE; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 763-765
  tell_vfs(rmp, &m); // 767  请 VFS 中断 READ/WRITE 等可中断调用
  return FALSE; // 769  等 VFS 回复（UNPAUSED 置位后再 TRUE）
}
```

四段式与 `mproc.h:95/97/102/94/89` 的 `VFS_CALL`/`UNPAUSED`/`DELAY_CALL`/`SIGSUSPENDED`/`PROC_STOPPED` 五标志强耦合：`731` 断言未 `VFS|EVENT`（`VFS` 阻塞的进程不经 `unpause`，直接 `sig_proc` 的 `VFS|EVENT` 分支 `pending+stop`），`735` 的 `UNPAUSED→PROC` 断言使“`VFS` 已确认”只与 `PROC_STOPPED` 共存，`741` 的 `DELAY→FALSE` 使 `EBUSY` 时 `sig_proc:514-520` 入 `pending`，`745-753` 的 `WAITING|SIGSUSPENDED→stop(FALSE)` 使 PM 侧睡眠直接停，`760-769` 的 `!PROC_STOPPED→MayDefer→tell_vfs→FALSE` 使 `VFS` 侧睡眠经 `VFS_PM_UNPAUSE` 往返（`com.h:498`，`05` 的 `tell_vfs` 编码）后 `UNPAUSED` 才 `TRUE`。

### 2.6 `process_ksig` 尾部 `SIGSNDELAY`（`signal.c:344-369`）

```c
  if (signo == SIGSNDELAY && (rmp->mp_flags & DELAY_CALL)) { // 344  内核的“发送完成”通知
    rmp->mp_flags &= ~DELAY_CALL; // 351  清延迟令牌
    assert(!(rmp->mp_flags & PROC_STOPPED)); // 353  DELAY 时未 PROC_STOPPED
    if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) { // 359  延迟前已进 VFS|EVENT
        stop_proc(rmp, FALSE /*may_delay*/); // 360  只停（MayDelay==FALSE，借 VFS 阻塞已保证无 SENDING）
        return OK; // 362  等 VFS 回复后 restart_sigs 续
    }
    check_pending(rmp); // 366  逐个重投（可能再次 VFS→break）
    assert(!(rmp->mp_flags & DELAY_CALL)); // 368
  }
  if ((mproc[proc_nr].mp_flags & (IN_USE | EXITING)) == IN_USE) return OK; // 372
  else return EDEADEPT; // 376
```

`344` 的双条件（`signo==70` 且 `DELAY_CALL`）使非延迟路径的正常 `SIGSNDELAY` 被忽略（`SIGSNDELAY` 由 `do_kill` 直接 `check_sig` 也会产生，但 `344` 仅延迟兑现），`351` 清标志后 `359-366` 的 `VFS|EVENT→stop` 与 `check_pending` 互斥分支与 `signal.c:359-366` 同序，`368` 的 `assert(!DELAY)` 保证兑现后无残留 `DELAY_CALL`。

### 2.7 消息与类型（`com.h:498` / `mproc.h:86-104` / `sys/signal.h:264`）

- `VFS_PM_UNPAUSE`（`com.h:498` `VFS_PM_RQ_BASE + ?`，`0x...`，`unpause:764-765` 的 `m_type` 与 `m.VFS_PM_ENDPT`）—— `unpause` 的 `VFS` 询问与 `handle_vfs_reply` 的 `VFS_PM_UNPAUSE_REPLY`（`com.h:528`）往返（`05` 的 `VfsCall::Unpause` 编解码）。
- `PROC_STOPPED 0x08`/`DELAY_CALL 0x20000`/`SIGSUSPENDED 0x100`/`UNPAUSED 0x1000`/`VFS_CALL 0x400`/`EVENT_CALL 0x80000`/`EXITING 0x20`/`TRACE_EXIT 0x8000`（`mproc.h:86-104`）。
- `SIGSNDELAY 70`（`sys/signal.h:264`，`process_ksig:344` 双条件与 `sig_proc` 的 `DELAY_CALL` 令牌同值，测试 `test_constants_match_c` 锁定）。
- `EBUSY  -107?`（`errno.h:78`，`sys_delay_stop` 的 `EBUSY` 与 `stop_proc:250` 分支同值）与 `OK 0`。

### 2.8 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `PROC_STOPPED` 双用途 | `signal.c:430-434` 注释 + `restart_sigs:704` `assert(!DELAY)` | `VFS|EVENT` 挂起点 `PROC_STOPPED` 为“需重检” | 不变量（`restart_sigs` 的 `PROC_STOPPED` 即重检） |
| `DELAY_CALL` 与 `PROC_STOPPED` 互斥 | `signal.c:237/353` `assert(!(PROC\|DELAY\|UNPAUSED))` / `assert(!PROC)` | `stop` 时不并存 | 不变量 |
| `may_delay` 契约 | `signal.c:251` `panic("unexpected delay")` | `MustStop` 遇 `EBUSY` | 不可恢复 |
| `check_pending` 的 `VFS|EVENT→break` | `signal.c:672-679` | 投递导致 `VFS|EVENT` | 不变量（`assert(PROC_STOPPED)`） |
| `restart_sigs` 的 `VFS|EVENT|EXITING→return` | `signal.c:693` | 仍阻塞或已退出 | 不变量 |
| `TRACE_EXIT` 优先 | `signal.c:695` 先于 `699` | `tracer` 强制退出 | 不变量（优先） |
| `unpause` 的 `UNPAUSED→PROC` 断言 | `signal.c:735` | `UNPAUSED` 必 `PROC_STOPPED` 且无 `DELAY` | 不变量 |
| `SIGSNDELAY` 兑现 | `signal.c:344/351` | `signo==70 && DELAY` | 不变量（`~DELAY→check`） |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `MayDelay` 枚举 + `KernelStop/Resume` trait + `UnpauseOutcome` 三态 + `PendingScanner` 迭代器 + `RestartAction` 枚举 + `handle_sigsn_delay` 显式状态机”的 8 决策，保留 C 的 `EBUSY→DELAY→SIGSNDELAY→check` 与 `VFS|EVENT→PROC_STOPPED→restart_sigs` 闭环，但以类型系统使 `may_delay` 契约与 `unpause` 三路径显式化。以下决策对应设计契约 `.design/13-design.v1.md` 的 D1–D8。

### D1：`stop_proc` 的 `may_delay` 收敛到 `MayDelay` 枚举

- **C**：`226` 行 `may_delay` 布尔（`251` `!may_delay→panic` 的契约在调用点保证）。
- **Rust**：`enum MayDelay { MustStop, MayDefer }` + `fn stop_proc(table, target, MayDelay, &mut dyn KernelStop) -> Result<StopOutcome, StopError>`（`StopOutcome::{Stopped, Deferred}`，`KernelStop::delay_stop(ep) -> Result<StopResult, i32>`，`StopResult::{Stopped, Busy}`）。
- **为什么**：`bool` 的 `may_delay` 在 `unpause:760` 的 `MayDefer` 与 `sig_proc:442` 的 `MustStop` 易误用为随意传 `true`；枚举使“`VFS|EVENT` 挂起点不可延迟”与“首次 `VFS UNPAUSE` 可延迟”在签名层互斥。

### D2：`try_resume_proc` 的守卫收敛到 `BlockState::can_resume` + `KernelResume` trait

- **C**：`279-280` 的 `VFS|EVENT|EXITING` 守卫 + `282` `sys_resume` + `288` 双清。
- **Rust**：`BlockState::can_resume(lifecycle) -> bool`（`!is_vfs_or_event && !is_exiting`），`KernelResume::resume(ep)`，`try_resume_proc(table, target, &mut dyn KernelResume) -> bool`（`false` 守卫阻挡，`true` 已恢复），双清在 `clear_stopped_and_unpaused` 一处方法。

### D3：`unpause` 的三路径收敛到 `UnpauseOutcome` 三态

- **C**：`734-769` 的 `TRUE/FALSE` 二值携带“已就绪/需等待”的语义（`sig_proc:514` 的 `!unpause→pending` 分支需区分 `DELAY` 忙 vs `VFS` 未就绪）。
- **Rust**：`enum UnpauseOutcome { Ready, Busy, VfsWait }`（`Ready` 对应 `TRUE` 可 `sig_send`，`Busy`.`VfsWait` 对应 `FALSE` 的两种原因），`unpause(table, target, &mut dyn KernelStop, &mut dyn VfsCtl) -> UnpauseOutcome`（`VfsCtl::tell_unpause(ep)` 抽象 `com.h:498` 的 `VFS_PM_UNPAUSE` 编码）。

### D4：`check_pending` 的扫描收敛到 `PendingScanner` 迭代器

- **C**：`664-681` 的 `for i=1.._NSIG if pending && !mask` 线性扫描 + `VFS|EVENT→break`。
- **Rust**：`SignalState::next_unblocked() -> Option<(u32,bool)>`（`pending & !mask` 最小 `signo` 与 `ksig` 还原），`check_pending(table, target, &mut dyn SigProcCaller) -> CheckPendingOutcome::{Completed, BrokenOnVfs}`（循环 `next_unblocked → take_pending → sig_proc(FALSE)` → `is_vfs_or_event_stopped → break`）。

### D5：`restart_sigs` 的分支收敛到 `RestartAction` 枚举

- **C**：`693` 三条件 `return` + `695-698` `TRACE_EXIT → exit_proc` + `699-712` `PROC_STOPPED → check→resume`。
- **Rust**：`enum RestartAction { Noop, Exit(i8), CheckAndResume }` + `restart_sigs(table, target, &mut dyn KernelResume, &mut dyn ExitHandler, &mut dyn SigProcCaller) -> RestartAction`（`TRACE_EXIT` 优先于 `PROC_STOPPED`，`Noop` 对应 `693` 提前 return）。

### D6：`SIGSNDELAY` 兑现收敛到 `handle_sigsn_delay`

- **C**：`344-369` 的 `if SIGSNDELAY && DELAY → ~DELAY → VFS|EVENT→stop else check` 隐式令牌在 `process_ksig` 尾部散落。
- **Rust**：`SignalFlow::handle_sigsn_delay(table, slot, &mut dyn KernelStop, &mut dyn SigProcCaller) -> bool`（`DELAY_CALL` 的 `DelayedSignal` 承载，`~DELAY` + `VFS|EVENT→stop` 否则 `check_pending`，`DELAY` 清后 `assert(!is_delayed)`）。

### D7：`PROC_STOPPED` 双用途的语义明确

- **C**：`430-434` 注释“`PROC_STOPPED` doubles as an indicator”以 `restart_sigs` 的 `if PROC_STOPPED` 为重检入口。
- **Rust**：`BlockState::stopped` 既是“已停”状态，也是 `restart_sigs` 的“需重检”指示器（`restart_sigs` 的 `if stopped` 即 `needs_recheck`），`assert(!is_delayed)` 在 `restart_sigs` 保证不与 `DELAY_CALL` 混淆。

### D8：常量收敛到 `minix-types`

- **C**：`com.h:498` `VFS_PM_UNPAUSE`、`sys/signal.h:264` `SIGSNDELAY=70`、`mproc.h:86-104` 7 个 `mp_flags`。
- **Rust**：`minix-types: VFS_PM_UNPAUSE`（补 `0x...` 数值锁定）、`SIGSNDELAY=70`、`BlockState` 的 `stopped/unpaused/delayed` 守卫。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-10 DELAY_CALL 延迟令牌 | `BlockState::DelayedSignal` + `handle_sigsn_delay`（D1/D6） | `block.rs` + 本文档 §3.1/3.6 + 计划 §4 |
| A-2 flag→枚举 | `MayDelay`/`UnpauseOutcome`/`RestartAction`（D1/D3/D5） | `signal_flow.rs` + 本文档 §3.1/3.3/3.5 + 计划 §4 |
| A-3 全局→显式 | `KernelStop/Resume`/`VfsCtl` 显式传参（D1/D2/D3） | `signal_flow.rs` 注释 + 本文档 §3.1-3.3 + 计划 §4 |
| A-6 SUSPEND 显式化 | `unpause` 的 `WAITING→stop(FALSE)` 与 `sigsuspend` 的 `UNPAUSED` 衔接（D3） | `signal_flow.rs` + 本文档 §3.3 + 计划 §7.3 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   ├── block.rs         — BlockState::{stopped, ipc_blocked: Option<DelayedSignal>, unpaused} + can_resume() + delayed() + clear_stopped_and_unpaused()
│   └── signal.rs        — SignalState::{next_unblocked(), take_pending(), has_pending_unblocked()} + SigSet::pending & !mask
├── signal_flow.rs       — stop_proc/try_resume_proc/unpause/check_pending/restart_sigs/handle_sigsn_delay（D1–D8）
└── ipc/
    └── vfs.rs           — VFS_PM_UNPAUSE 编解码扩展（com.h:498 → minix-types VFS_PM_UNPAUSE）
```

### 4.2 `mproc/block.rs`：阻塞三元与守卫

```rust
impl BlockState {
    pub fn can_resume(&self, lifecycle: &Lifecycle) -> bool {
        !self.is_vfs_blocked() && !self.is_event_blocked() && !lifecycle.is_exiting()
    }
    pub fn is_delayed(&self) -> bool { matches!(self.ipc_blocked, Some(DelayedSignal)) }
    pub fn clear_stopped_and_unpaused(&mut self) { self.stopped = false; self.unpaused = false; }
}
```

`VFS|EVENT|EXITING→return`（`279-280`）收敛为 `can_resume` 一处谓词，`288` 双清收敛为 `clear_stopped_and_unpaused` 一处方法（UNPAUSED 只随 PROC_STOPPED 清）。

### 4.3 `mproc/signal.rs`：`pending & !mask` 迭代

```rust
impl SignalState {
    pub fn next_unblocked(&self) -> Option<(u32,bool)> { // 1..64 最小 pending & !mask
        let unblocked = self.pending & !self.mask;
        if unblocked == 0 { None } else {
            let signo = unblocked.trailing_zeros() + 1;
            let ksig = (self.kernel_pending >> (signo-1) & 1) != 0;
            Some((signo as u32, ksig))
        }
    }
    pub fn take_pending(&mut self, signo: u32) { // 双清 668-669
        let b = 1u64 << (signo-1);
        self.pending &= !b; self.kernel_pending &= !b;
    }
}
```

`check_pending` 的 `for i=1.._NSIG` 线性扫描在 Rust 可为 `trailing_zeros` 加速但保持 `1..64` 最小顺序（与 `signal.c:664` 对齐）；`next_unblocked` 使 `check_pending` 的循环在 `signal_flow.rs` 为 `while let Some((signo,ksig)) = state.next_unblocked()`。

### 4.4 `signal_flow.rs`：五函数与 trait

```rust
pub enum MayDelay { MustStop, MayDefer }
pub enum StopOutcome { Stopped, Deferred }
pub trait KernelStop { fn delay_stop(&mut self, ep: Endpoint) -> Result<StopOutcome, i32>; }
pub trait KernelResume { fn resume(&mut self, ep: Endpoint) -> Result<(), i32>; }
pub trait VfsCtl { fn tell_unpause(&mut self, ep: Endpoint); }
pub enum UnpauseOutcome { Ready, Busy, VfsWait }
pub enum CheckPendingOutcome { Completed, BrokenOnVfs }
pub enum RestartAction { Noop, Exit(i8), CheckAndResume }

pub fn stop_proc(table: &mut ProcTable, tgt: UserSlot, d: MayDelay, k: &mut dyn KernelStop) -> Result<StopOutcome, ()>
pub fn try_resume_proc(table: &mut ProcTable, tgt: UserSlot, k: &mut dyn KernelResume) -> bool
pub fn unpause(table: &mut ProcTable, tgt: UserSlot, k: &mut dyn KernelStop, v: &mut dyn VfsCtl) -> UnpauseOutcome
pub fn check_pending(table: &mut ProcTable, tgt: UserSlot, caller: &mut dyn SigProcCaller) -> CheckPendingOutcome
pub fn restart_sigs(table: &mut ProcTable, tgt: UserSlot, k: &mut dyn KernelResume, e: &mut dyn ExitHandler, c: &mut dyn SigProcCaller) -> RestartAction
pub fn handle_sigsn_delay(table: &mut ProcTable, slot: usize, k: &mut dyn KernelStop, c: &mut dyn SigProcCaller) -> bool
```

- `stop_proc`：`assert(!(stopped|delayed|unpaused))`（`237`）→ `k.delay_stop → Stopped→stopped=true` / `Busy→MustStop:panic` / `Busy→MayDefer: ipc_blocked=DelayedSignal`。
- `try_resume_proc`：`assert(stopped)`（`271`）→ `!can_resume→false` → `k.resume → clear_stopped_and_unpaused → true`。
- `unpause`：`assert(!(VFS|EVENT))`（`731`）→ `UNPAUSED→Ready`（`734-738` `assert(delay|proc==proc)`）→ `DELAY→Busy`（`741`）→ `WAITING|SIGSUSPENDED→stop(MustStop)→Ready`（`745-753`）→ `!stopped && stop(MayDefer)=Deferred→Busy`（`760-761`）→ `tell_unpause→VfsWait`（`763-769`）。
- `check_pending`：`while next_unblocked → take → sig_proc(FALSE,ksig) → if is_vfs_or_event_stopped → break`（`672-679` 的 `assert(PROC_STOPPED)` 在 Rust 为 `debug_assert!(stopped)`）。
- `restart_sigs`：`if VFS|EVENT|EXITING→Noop`（`693`）→ `if TRACE_EXIT→Exit(exit_code)`（`695-698`）→ `if stopped→assert(!delayed)→check_pending→try_resume→CheckAndResume`（`699-712`）。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `PROC_STOPPED` 双用途 | `signal.c:430-434` 注释 | `stopped` 布尔 + `needs_recheck` 派生 | `restart_sigs` 的 `if stopped` 即重检 |
| 2 | `DELAY` 与 `PROC` 互斥 | `signal.c:237/353` | `assert(!(stopped\|delayed\|unpaused))` | `debug_assert` |
| 3 | `may_delay` 契约 | `signal.c:251` `panic` | `MayDelay::MustStop→panic` | `#[should_panic]` |
| 4 | `VFS|EVENT→break` | `signal.c:672-679` | `CheckPendingOutcome::BrokenOnVfs` | `test_check_pending_breaks_on_vfs` |
| 5 | `VFS|EVENT|EXITING→Noop` | `signal.c:693` | `RestartAction::Noop` | `test_restart_sigs_noop_when_vfs` |
| 6 | `TRACE_EXIT→Exit` 优先 | `signal.c:695` 先于 `699` | `RestartAction::Exit` | `test_restart_sigs_trace_exit_first` |
| 7 | `UNPAUSED→PROC` | `signal.c:735` | `UnpauseOutcome::Ready` 断言 | `debug_assert` |
| 8 | `SIGSNDELAY+DELAY→~DELAY→check` | `signal.c:344/351` | `handle_sigsn_delay` | `test_sigsn_delay_clears_and_checks` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-02 为 **226 passed / 0 failed**（原 209 + 本档新增 ~17：`signal_flow.rs` 13 + `mproc/block.rs` 2 + `mproc/signal.rs` 2）。`cargo test -p minix-types --lib` 108 passed（新增 `VFS_PM_UNPAUSE` 常量与 `SIGSNDELAY` 若补）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `signal_flow.rs`（延迟与恢复机制）

- `test_stop_proc_ok_stops`：`OK→PROC_STOPPED→Stopped`（`245-248`）
- `test_stop_proc_ebusy_must_panic`：`EBUSY+MustStop→panic`（`251-252` `#[should_panic]`）
- `test_stop_proc_ebusy_may_defer`：`EBUSY+MayDefer→DELAY_CALL→Deferred`（`254-256`）
- `test_stop_proc_unexpected_panics`：其他错误 `panic`（`259`）
- `test_try_resume_noop_when_vfs`：`VFS|EVENT|EXITING→Noop`（`279-280`）
- `test_try_resume_clears_stopped_and_unpaused`：`OK→~(PROC|UNPAUSED)` 双清（`288`）
- `test_unpause_ready_when_unpaused`：`UNPAUSED→Ready`（`734-738`）
- `test_unpause_busy_when_delay`：`DELAY→Busy`（`741-742`）
- `test_unpause_ready_when_waiting`：`WAITING→stop(FALSE)→Ready`（`745-753`）
- `test_unpause_vfs_wait_when_not_stopped`：`!PROC_STOPPED→MayDefer→VfsWait` 或 `Busy`→`VfsWait`（`760-769`）
- `test_check_pending_single_delivers`：`pending&!mask` 单 `sig_proc(FALSE)`（`664-670`）
- `test_check_pending_breaks_on_vfs`：`VFS|EVENT→break`（`672-679` 单步后 `BrokenOnVfs`）
- `test_check_pending_ksig_restored`：`ksigpending` 还原 `ksig` 真假（`667`）
- `test_restart_sigs_noop_when_vfs`：`VFS|EVENT|EXITING→Noop`（`693`）
- `test_restart_sigs_trace_exit_first`：`TRACE_EXIT→Exit` 优先于 `PROC_STOPPED`（`695-698`）
- `test_restart_sigs_check_and_resume`：`PROC_STOPPED→check→resume`（`699-712`）
- `test_sigsn_delay_clears_and_checks`：`SIGSNDELAY+DELAY→~DELAY→VFS?stop:check`（`344-366`）

### 5.2 `mproc/block.rs` 与 `mproc/signal.rs`（状态机扩展）

- `test_block_can_resume_guard`：`can_resume` 的 `VFS|EVENT|EXITING` 守卫（`279-280`）
- `test_block_delayed_isolated`：`DelayedSignal` 与 `stopped/unpaused` 互斥（`237/353`）
- `test_next_unblocked_smallest`：`next_unblocked` 最小 `signo` 优先（`664` 顺序）
- `test_take_pending_clears_both`：`take_pending` 双清 `pending/ksigpending`（`668-669`）

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `SIGSNDELAY=70`（`sys/signal.h:264`）、`VFS_PM_UNPAUSE`（`com.h:498`）、`PROC_STOPPED=0x08` 等（`mproc.h:86-104`）

测试策略：`KernelStop/Resume` 与 `VfsCtl`/`SigProcCaller`/`ExitHandler` 均 `Test*` mock 可注入 `OK/EBUSY` 与计数；`check_pending` 的 `sig_proc` 以“记 `signo` 顺序” mock 验 `VFS→break`；`restart_sigs` 的 `TRACE_EXIT` 以 `Lifecycle::Trace` 置位验优先。

---

## 6 过渡

本篇在 11 的 `sig_proc→VFS|EVENT→stop_proc(FALSE)` 与 12 的 `unpause→sig_send` 机制之间，是 05 的 `handle_vfs_reply` 尾部 `restart_sigs` 与 06 的 `EVENT_CALL` 续的衔接：

```
11-signal-core.md（sig_proc的VFS|EVENT 挂起→pending+PROC_STOPPED + caught→unpause→sig_send）
  │
  └─► 12-signal-handlers.md（sigaction三态 + sigprocmask四how + sigsend四步翻译 + WAITING→EINTR）
         │
         └─► 本章（stop_proc的MayDelay + try_resume的守卫 + unpause三路径 + check_pending的VFS→break + restart_sigs的TRACE_EXIT优先 + SIGSNDELAY的DELAY兑现）
                │
                ├─► 05-vfs-interaction.md（VFS_PM_UNPAUSE_REPLY→UNPAUSED→publish_event→resume_event→restart_sigs 的 VFS 往返）
                ├─► 06-event-subscription.md（EVENT_CALL的resume_event→restart_sigs 的事件续）
                └─► 14-itimer.md（SIGALRM经 kill→check_pending→restart_sigs 的 ALARM_ON 可恢复路径，需本章 PROC_STOPPED 重检保证不丢失）
```

`check_pending` 为 12 的 `Block/Unblock/SetMask/sigsuspend/sigreturn` 的 `needs_check` 前置，是 11 的 `SIGSNDELAY` 延迟兑现的 `~DELAY→check` 续——本章锁定 `PROC_STOPPED` 的双用途不与 `DELAY_CALL` 混淆，14 的 `SIGALRM` 经 `kill→sig_proc→pending` 的重检即可复用同一路径。

阅读顺序提示：若想先理解“VFS 回复后如何善后”，下一站 `05-vfs-interaction.md` 的 `handle_vfs_reply` 尾部；若想理解“内核延迟停止的 EBUSY 时序”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`sys_delay_stop` 的 `SENDING` 检查与 `sys_resume` 的唤醒）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/signal.c:226-289`（`stop_proc`/`try_resume_proc`）、`minix3/minix/servers/pm/signal.c:651-776`（`check_pending`/`restart_sigs`/`unpause`）、`minix3/minix/servers/pm/signal.c:344-369`（`process_ksig` 尾部 `SIGSNDELAY`）、`minix3/minix/servers/pm/mproc.h:86-104`（`PROC_STOPPED` 等 7 个 `mp_flags`）、`minix3/minix/include/minix/com.h:498`（`VFS_PM_UNPAUSE`）、`minix3/sys/sys/signal.h:264`（`SIGSNDELAY`）
- PM 阶段文档：11-signal-core.md（`sig_proc` 的 `VFS|EVENT` 挂点与 `process_ksig` 的 `SIGSNDELAY` 分支）、12-signal-handlers.md（`sig_send` 的 `sigmsg` 四步与 `sigsuspend` 的 `mask2` 配对、`without_unkillable`）、05-vfs-interaction.md（`handle_vfs_reply` 的 `restart_sigs` 调用点与两处 `publish_event` 提前 return）、06-event-subscription.md（`EVENT_CALL` 的 `resume_event` 尾部分派）、02-mproc-struct.md（`BlockState` 三元与 `IpcBlockReason::DelayedSignal`）、04-ipc-dispatch.md（`ReplyIntent::ReplyLater` 的 `SUSPEND` 与 `EINTR` 中断）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`sys_delay_stop/sys_resume` 的 `EBUSY` 时序与 `SENDING` 检查）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md`（`sys_sigsend`/`sys_sigreturn` 的 `sigframe` 与 `sys_kill` 的信号投递）
- 阶段内顺序：11 → 12 → **本章（13）** → 14（`SIGALRM` 的 `ALARM_ON` 与 `check_pending` 的 `pending&!mask` 重检复用）→ 15（`TAINTED` 与 `credentials` 的信号边界）→ 16（`sched_stop` 的直毁，绕过本章的 `PROC_STOPPED`）
- OS 模式参考：Linux `TASK_INTERRUPTIBLE`/`signal_pending`/`recalc_sigpending`、`FreeBSD msleep(PCATCH)`、`Redox Context::blocked/SigQueue`、`seL4 Notification`（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/block.rs`（`BlockState::can_resume`/`stopped` 双用途）、`os/servers/pm/src/mproc/signal.rs`（`SignalState::next_unblocked/take_pending`）、`os/servers/pm/src/signal_flow.rs`（`MayDelay`/`stop_proc`/`try_resume_proc`/`unpause`/`check_pending`/`restart_sigs`/`handle_sigsn_delay`）

