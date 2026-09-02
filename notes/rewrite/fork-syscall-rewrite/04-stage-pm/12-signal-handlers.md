# 12 — 信号处理器的安装与掩码语义

本文讲清信号处理器如何在 PM 侧被“安装”、如何被“屏蔽”以及最终如何被“投递到用户栈”：`do_sigaction` 的 `SIG_IGN/DFL/CATCH` 三态为何要以不同方式联动四张位图（`ignored/caught/pending/kernel_pending`）、`do_sigprocmask` 的四种 `how` 为何只有两种会触发 `check_pending`、为什么 `KILL/STOP` 必须在五处同时剥离、`sigsuspend` 与 `sigreturn` 为何是一对以 `SUSPEND` 为桥梁的原子操作，以及 `sig_send` 如何把 PM 的位图语义翻译成内核的 `sigmsg` 栈帧并在 `EFAULT/ENOMEM` 与 `WAITING|SIGSUSPENDED` 之间分岔。

前置阅读：11-signal-core.md（`sig_proc` 的 9 判定链与 `ignored/caught/mask/pending` 的投递语义、`check_sig` 的权限门）、02-mproc-struct.md（`SignalState` 四位图与 `SigAction` 堆分配、`BlockState::stopped/unpaused/suspended`）、04-ipc-dispatch.md（`ReplyIntent::ReplyLater` 的 `SUSPEND 语义契约`，`plan.md §7.3` 的三子类）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `sig_proc` 的 `ignore→block→caught→terminate` 优先级（11 的 9 链）、`ProcTable` 的逆序扫描与 `SUSPEND` 自杀语义（04）的开发者；知道 `SigSet = u64` 的 `1<<(n-1)` 位语义与 `_NSIG=64` 边界。

> **本章不讲什么**：
> - 信号的生成与广播（`check_sig` 的四态 `pid`、`process_ksig` 的 `EDEADEPT` 双检、`sig_proc_exit` 的 `core_sset`）—— `11-signal-core.md`
> - 停止/延迟/恢复的 `PROC_STOPPED`/`DELAY_CALL`/`UNPAUSED`/`restart_sigs`（`stop_proc`/`try_resume_proc`/`unpause`/`check_pending`/`restart_sigs` 的详述）—— `13-signal-flow.md`；本章只到它们的**调用点**
> - 内核 `sigframe` 的栈推送与 `sigcontext` 恢复（`sys_sigsend`/`sys_sigreturn` 的内核侧实现）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md`
>
> 本章只回答一个问题：**PM 如何为“将来某个信号到达时进程该做什么”建立契约，并以掩码控制该契约何时生效，最终把契约翻译成内核可执行的栈帧**。

### 1.1 为什么信号需要“安装语义”：位图是契约的物化

信号的四处置（`ignore` / `block` / `catch` / `terminate`）在 `11-signal-core.md` 的 `sig_proc` 9 链中已有全序：`badignore → ignore → block → TRACE_STOPPED → caught → terminate`。但这些判定依赖的位图（`mp_ignore` / `mp_catch` / `mp_sigmask` / `mp_sigpending`）从何而来？答案是**安装**：用户通过 `sigaction(2)` 把“我对信号 N 的态度”写进 PM 的位图，PM 记住它，下次 `sig_proc` 读取它。

没有安装语义，`sig_proc` 的优先级无从判定——`catch` 位为空则永远走不到 `sig_send`，`ignore` 位为空则 `SIGCHLD` 无法默认忽略（`main.c:139` 的 `ign_sset` 只是初始默认，`sigaction` 可改写）。因此 `do_sigaction` 不是孤立的系统调用，而是 `sig_proc` 投递语义的**生产者**；二者在 PM 进程表的同一 `SignalState` 上形成“生产—消费”闭环：

```
sigaction(2)  ── do_sigaction ──►  SignalState { ignored, caught, mask, pending }
                                              │
kill(2) ── check_sig ── sig_proc ─────────────┘
                              │
                              └──► sig_send ── sys_sigsend ──► kernel sigframe
```

位图是契约的物化形式——`ignored` 记录“我不想收到它”，`caught` 记录“我想自己处理它”，`mask` 记录“我现在不想被它打断”，`pending` 记录“它已到但被我暂时拦住”。四张位图的组合决定信号最终走 `ignore` 还是 `caught` 还是 `blocked` 还是 `terminate`。

### 1.2 为什么 `sigaction` 要区分“忽略”与“捕获”：不对称的联动

`signal.c:67-78` 的三态分支并不对称——这不是风格问题，而是 `sig_proc` 语义的必然：

* `SIG_IGN`（`67-71`）：`sigaddset(ignore)` + `sigdelset(pending/ksigpending/catch)`。忽略意味着“此后该信号永远不应再出现”，因此不仅要记 `ignore`，还要**同步清理已挂起的同信号**（否则 `check_pending` 会在下一次 `UNBLOCK` 时重投一个已被声明忽略的信号），并清 `catch`（忽略与捕获互斥）。
* `SIG_DFL`（`72-74`）：`sigdelset(ignore/catch)`。默认处置不清理 `pending`——信号已挂起但处置回归默认，等待下次 `sig_proc` 按 `ign_sset` / `core_sset` 重新判定（`11 §1.4` 的 `ign_sset` 默认忽略 vs `terminate`）。
* `catch`（`75-77`）：`sigdelset(ignore) + sigaddset(catch)`。捕获与忽略互斥，需清 `ignore` 再置 `catch`，不对 `pending` 动手——已挂起的信号正等着 `sig_send` 建立 handler。

三态共享的收敛点是 `79-84` 的统一落盘：`sa_handler / sa_mask / sa_flags / sigreturn`，其中 `sa_mask` 需**剥离 `KILL/STOP`**（`80-81`），否则 `sig_send` 在 `800-803` 叠加 `sa_mask` 时会把不可屏蔽信号意外屏蔽。

不对称的联动使 `SignalState::install` 不能是“先清后置”的简单覆盖——必须是按 `handler` 分支的原子四位图事务。

### 1.3 为什么 `KILL/STOP` 不可捕获/忽略/屏蔽：三重封堵

`SIGKILL=9` / `SIGSTOP=17`（`sys/signal.h:62/70`）的不可阻断性是 POSIX 的进程管理不变量——内核必须保证“有一个信号可无条件杀死/停止任何进程”。PM 在三层同时封堵：

1. **安装层**：`do_sigaction` 的 `49` 行 `sig_nr == SIGKILL → return OK` 早返——`SIGKILL` 不可重装处理器，直接成功返回（`SIG_IGN/DFL` 的三态分支根本不执行）；`SIGSTOP` 未在 `49` 早返但在 `72-78` 的 `SIG_DFL` 融入默认语义，且 `sa_mask` 剥离阻止它进入 `catch` 的阻塞掩码。
2. **掩码层**：`sa_mask` 的 `sigdelset(KILL/STOP)`（`80-81`）与 `do_sigprocmask` 的三处剥离（`124-125 BLOCK` / `141-142 SETMASK` / `166-167 sigsuspend` / `186-187 sigreturn`）——任何把 `KILL/STOP` 写入阻塞集的尝试都在入口被剥离。
3. **投递层**：`sig_proc` 的 `411` 行 `signo != SIGKILL` 条件使 `TRACE` 拦截跳过 `KILL`，`499` 行的 `TRACE_STOPPED` 同样跳过 `KILL`——`KILL` 绕过所有可拦截路径直达 `sig_proc_exit`。

三重封堵使不可阻断性在“安装→掩码→投递”全链路无泄漏——Rust 侧收敛为 `SigSet::without_unkillable` 一处方法（§3.3），消除五处分散的 `sigdelset`。

### 1.4 为什么 `sigprocmask` 有四种 `how`：叠加、清除、覆盖、只读

`signal.c:122-152` 的四 `how` 对应四种掩码演化（`sys/signal.h:174-180`）：

| `how` | 值 | 语义 | 剥离 `KILL/STOP` | `check_pending` |
|-------|----|------|-----------------|-----------------|
| `SIG_BLOCK` | 1 | `mask \|= set` 叠加 | 是（`124-125`） | 否（`129` 后无重检） |
| `SIG_UNBLOCK` | 2 | `mask &= ~set` 清除 | 否（`133` 不剥离） | 是（`137`） |
| `SIG_SETMASK` | 3 | `mask = set` 覆盖 | 是（`141-142`） | 是（`144`） |
| `SIG_INQUIRE` |10 | 只读旧 `mask` | —（无写入） | 否 |

两处微妙差异：

* `BLOCK` 与 `UNBLOCK` 对 `KILL/STOP` 的处理不同——`BLOCK` 剥离后 `sigaddset` 循环（`124-129`）防止“屏蔽不可屏蔽”，`UNBLOCK` 的 `132-136` 循环不剥离（`sigdelset` 语义：即使 `set` 含 `KILL`，`mask` 原本就不含 `KILL`，清一次无害，且 `sigdelset` 的边界检查在 Minix 系统内为 `_SYSTEM` 宏的 `__sigdelset` 无检查版）。
* `UNBLOCK` 与 `SETMASK` 后必须 `check_pending(mp)`（`137/144`），而 `BLOCK` 与 `INQUIRE` 不需要——只有解除阻塞才可能使 `pending` 中的信号变为可投递；新增阻塞不会产生新的可投递信号。

因此 `SigMaskOp` 枚举需携带“是否触发 `check_pending`”的语义（§3.4 的 `Option<Pending>`）。

### 1.5 为什么 `sigsuspend` 与 `sigreturn` 成对：原子等待与上下文恢复

`sigsuspend`（`signal.c:157-171`）与 `sigreturn`（`173-192`）是信号掩码语义的一对**原子桥**：

* `sigsuspend`：`mask2 = mask` 保存旧掩码（`164`）→ `mask = set` 覆盖新掩码并剥离 `KILL/STOP`（`165-167`）→ 置 `SIGSUSPENDED`（`168`）→ `check_pending`（`169`）→ `return SUSPEND`（`170`，`04` 的 `ReplyLater`）。调用者在新掩码下睡眠，直到被信号唤醒——掩码切换与睡眠在 PM 侧原子完成（无竞争窗口）。
* `sigreturn`：`mask = set` 恢复掩码并剥离（`185-187`）→ `sys_sigreturn(who_e, ctx)`（`189`）使内核的 `sigcontext` 与用户栈同步 → `check_pending`（`190`）→ `return r`（`191`）。它是 `sig_send` 的 `sigmsg` 推送的逆操作——handler 执行完后把 `sm_mask`（`signal.c:793/795` 的 `mask` 或 `mask2`）还原。

两者以 `check_pending` 为衔接——`sigsuspend` 进入睡眠前检查已解除阻塞的 `pending`（若有则立即 `sig_proc→sig_send` 在同 `check_pending` 链内完成，不必等新信号）；`sigreturn` 恢复掩码后同样重检（handler 期间可能又有 `pending` 积累）。

`sigsuspend` 的 `SUSPEND` 是 `04` 的 `ReplyLater` 中**唯一不经 VFS、纯由信号驱动唤醒**的来源（`plan.md §7.3` 的 `ReplyLater` 自杀子类之外）。

### 1.6 为什么 `sig_send` 是“位图→栈帧”的翻译器

`signal.c:776-855` 的 `sig_send` 是 PM 位图语义与内核栈帧语义的**交汇点**。PM 侧的 `caught` 位决定“该进 `sig_send`”，`sig_send` 把 `SignalState` 翻译成 `struct sigmsg`（`type.h:73`）：

```c
struct sigmsg {
    sigset_t sm_mask;        // handler 期间的掩码（mask 或 mask2）
    int      sm_signo;        // 信号编号
    vir_bytes sm_sighandler; // handler 地址（mp_sigact[signo].sa_handler）
    vir_bytes sm_sigreturn;  // __sigreturn trampoline
};
```

翻译含四步掩码演化（`792-813`）：

1. `sm_mask` 取 `mask` 还是 `mask2`——若进程 `SIGSUSPENDED` 则取 `mask2`（`792-795`），否则取 `mask`（`795`）。这是 `1.5` 原子桥的兑现：`sigsuspend` 期间捕获信号的 handler 应在“进入 `sigsuspend` 前的掩码”下执行，`sigreturn` 恢复的也是该掩码。
2. 叠加 `sa_mask`（`800-803`）：`for i=1.._NSIG if sa_mask has i → sigaddset(sm_mask, i)`——handler 声明的“执行期间额外屏蔽集”。
3. `SA_NODEFER 0x10`（`805-808`）：若置位则 `sigdelset(sm_mask, signo)`——抵消第 2 步对当前信号自身的屏蔽（默认 `sigaddset(sm_mask, signo)` 防止 handler 重入，`NODEFER` 允许重入）。
4. `SA_RESETHAND 0x04`（`810-813`）：若置位则 `sigdelset(catch, signo) + handler = SIG_DFL`——handler 一次性，触发后回归默认（`SIG_DFL`）。

`sys_sigsend(rmp->mp_endpoint, &sigmsg)` 的三分支（`818-829`）是翻译的**边界**：`EFAULT/ENOMEM → false`（用户栈不足，`sig_proc` 的 `printf+killing` 兜底），`OK → true`（后续 `WAITING|SIGSUSPENDED → EINTR+try_resume` vs `UNPAUSED` 断言的二分，`832-851`），其他 `→ panic`（PM/内核不一致）。

### 1.7 与其他 OS 状态机的对照

Rust 改写不是照抄 `for i=1.._NSIG if sigismember` 循环，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `sigaction` / `sigprocmask` / `sigsuspend`。** Linux 的 `sigaction` 同样三态（`SIG_DFL=0` / `SIG_IGN=1` / handler）+ `sa_mask` / `sa_flags`（`SA_NODEFER` / `SA_RESETHAND` / `SA_RESTART` / `SA_NOCLDSTOP` / `SA_SIGINFO`，`sys/signal.h:150-162`），`sigprocmask` 同样四 `how`（`BLOCK/UNBLOCK/SETMASK`，`signal.h:174-179`），`sigsuspend` 同样“原子换掩码+睡眠”（`sigsuspend(2)`）。与 Minix3 的差异在**排队**：Linux `sigqueue` 的 `siginfo` 使实时信号可排队（`signal_struct.shared_pending` 为队列），Minix3 位图使信号**不排队**（同一信号多次到达在 `pending` 中挤压为 1 位）——位图是常数时间 `sigismember`（`1<<(n-1)`）与 `#![no_std]` 无堆分配的取舍，队列是 `siginfo` 携带与实时性的取舍。Minix3 选择位图恰因 `PM` 单线程无堆且 `_NSIG=64` 可用 `u64` 一次表达。

Linux 的 `SA_RESTART`（`0x0002`）在 Minix3 未实现（`11` 的 `unpause` 经 VFS `EINTR` 而非内核自动重启），`SA_NOCLDSTOP/WAIT` 仅对 `SIGCHLD` 有效（`signal.h:156-157`），Minix3 仅需 `SA_NODEFER/RESETHAND` 两位即覆盖 `sig_send` 的全部行为。

**Redox `SigAction` + `SigQueue`。** Redox 以 `SigAction { handler: SigHandler, mask: SigSet, flags: SigActionFlags }`（`common/src/signal.rs`）+ `SigQueue(VecDeque<Signal>)`（`kernel/src/signal.rs`）建模，`SigHandler` 同样三态（`Default / Ignore / Handler(addr)`），`SigSet` 同样 `u64` 位图（`common/src/signal.rs` `1<<(sig-1)`）。差异同上：Redox 的 `SigQueue` 为 per-process 队列（`sigqueue` 可排队），Minix3 为位图；Redox 队列的 `SigActionFlags` 含 `SA_NODEFER/RESETHAND` 与 Minix3 一致，`sys_sigsend` 的内核侧同样“把 `SigAction` 压入用户栈”但 Redox 经 `Scheme` 的 `sigreturn` trampoline，Minix3 经 `sys_sigreturn` 的 `sigmsg.ctx`。

Redox 的位图查询同样 `sigismember` 常数时间，但队列使 `kill -USR1` 连发三次可区分三次（`SIGRTMIN..MAX` 实时区），Minix3 三次挤压为一次——这是调度语义的差异，不影响 `install` / `mask_op` / `sigsuspend` 的结构。

**`seL4` 无信号。** `seL4` 无信号原语，以 `Notification` / `Endpoint` + `reply capability` 替代异步事件，`PM` 的 `sig_send` 9 链 + `VFS|EVENT` 挂起 + `PROC_STOPPED` 重检与 `seL4` 的显式 `Notification` 无法直接对照——`seL4` 的可靠性不靠“不可阻断信号”而靠 capability 显式撤销，Minix3 的 `KILL/STOP` 三重封堵即是在无 capability 世界中对“可无条件终止”的建模。

**结论（本章的设计基线）。** 把 C 的“`sys_datacopy` 裸指针 + 五处分散的 `sigdelset(KILL/STOP)` + 四 `how` 的 `switch` + `mask2` 隐式配对 + `sigmsg` 四步掩码”改写为“`SigActionReq { Option<SigAction> }` + `without_unkillable` 一处方法 + `SigMaskOp` 枚举 + `prepare_suspend`/`restore` 配对 + `prepare_sigmsg` 四步显式化 + `KernelSigSend/SigReturn` trait”——与 Linux/Redox 的 `SigAction` 三态 + `SA_*` 语义同源，又因 PM 单线程无堆而保持位图与同步 `sys_sig*` 调用。

### 1.8 小结

1. **为什么三态不对称**——`SIG_IGN` 同步清 `pending/ksigpending/catch`，`SIG_DFL` 保留 `pending`，`catch` 置 `caught`；位图联动的差异直接决定 `sig_proc` 的优先级。
2. **为什么 `KILL/STOP` 三重封堵**——安装层早返+剥离、掩码层剥离、投递层跳过，Rust 侧一处 `without_unkillable` 方法锁定。
3. **为什么四 `how`**——`BLOCK` 叠加不重检、`UNBLOCK/SETMASK` 解除阻塞需 `check_pending`、`INQUIRE` 只读；`UNBLOCK` 不剥离 `KILL/STOP` 的差异与 `BLOCK/SETMASK` 区分刻意。
4. **为什么 `suspend/return` 成对**——`mask2` 保存旧掩码在 `sigsuspend` 原子换掩码后 `SUSPEND` 睡眠，`sigreturn` 以 `set` 恢复掩码后 `sys_sigreturn` 同步内核栈，任缺 `check_pending` 则 `pending` 永不投递。
5. **为什么 `sig_send` 是翻译器**——`mask/mask2` 分支 + `sa_mask` 叠加 + `SA_NODEFER/RESETHAND` 四步把 `SignalState` 翻为 `sigmsg`，`sys_sigsend` 的 `EFAULT/ENOMEM→kill` vs `OK→EINTR/UNPAUSED` 使 PM 的位图语义与内核的栈帧语义可区分。
6. **为什么位图而非队列**——`#![no_std]` 无堆 + `PM` 单线程 + `_NSIG=64` 可 `u64` 常数时间 `sigismember`；代价是同信号多次到达挤压为 1 位，不影响 `install`/`mask_op`/`suspend` 结构。

下一章逐行分析 C 的 `do_sigaction`/`do_sigpending`/`do_sigprocmask`/`do_sigsuspend`/`do_sigreturn`/`sig_send`；第 3 章给出 Rust 的 `SignalState::install`/`apply_mask_op`/`without_unkillable` 与 `SigMaskOp`/`KernelSig*`。

---

## 2 C 源码分析

### 2.1 `do_sigaction`（`signal.c:40-86`）

```c
int do_sigaction(void)
{
  int r, sig_nr;
  struct sigaction svec;
  struct sigaction *svp;

  assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL))); // 46

  sig_nr = m_in.m_lc_pm_sig.nr;          // 48  m_lc_pm_sig { pid,nr,act,oact,ret } ipc.h:532
  if (sig_nr == SIGKILL) return(OK);     // 49  9 不可重装，直接成功（POSIX 不变量）
  if (sig_nr < 1 || sig_nr >= _NSIG) return(EINVAL); // 50  _NSIG 64 (signal.h:45)

  svp = &mp->mp_sigact[sig_nr];          // 52  mpsigact[NR_PROCS][_NSIG] 行指针（mproc.h:22）
  if (m_in.m_lc_pm_sig.oact != 0) {       // 53  oact 非空则回写旧动作
    r = sys_datacopy(PM_PROC_NR,(vir_bytes) svp, who_e,
        m_in.m_lc_pm_sig.oact, (phys_bytes) sizeof(svec));
    if (r != OK) return(r);              // 56  EFAULT 等透传
  }

  if (m_in.m_lc_pm_sig.act == 0)          // 59  act==0 只读旧动作
    return(OK);                          // 60

  /* Read in the sigaction structure. */
  r = sys_datacopy(who_e, m_in.m_lc_pm_sig.act, PM_PROC_NR, (vir_bytes) &svec,
      (phys_bytes) sizeof(svec));        // 63-64 读新 svec
  if (r != OK) return(r);                // 65

  if (svec.sa_handler == SIG_IGN) {       // 67  SIG_IGN 1
    sigaddset(&mp->mp_ignore, sig_nr);   // 68
    sigdelset(&mp->mp_sigpending, sig_nr);// 69  同步清理 pending
    sigdelset(&mp->mp_ksigpending, sig_nr);//70  双清 kernel_pending
    sigdelset(&mp->mp_catch, sig_nr);    // 71  忽略与捕获互斥
  } else if (svec.sa_handler == SIG_DFL) {//72  SIG_DFL 0
    sigdelset(&mp->mp_ignore, sig_nr);   // 73
    sigdelset(&mp->mp_catch, sig_nr);    // 74  保留 pending（不对称）
  } else {                                // 75  用户 handler（地址 !=0/1）
    sigdelset(&mp->mp_ignore, sig_nr);   // 76
    sigaddset(&mp->mp_catch, sig_nr);    // 77  置捕获
  }
  mp->mp_sigact[sig_nr].sa_handler = svec.sa_handler; // 79  落盘
  sigdelset(&svec.sa_mask, SIGKILL);     // 80  sa_mask 剥离 KILL/STOP
  sigdelset(&svec.sa_mask, SIGSTOP);     // 81
  mp->mp_sigact[sig_nr].sa_mask = svec.sa_mask;       // 82
  mp->mp_sigact[sig_nr].sa_flags = svec.sa_flags;     // 83
  mp->mp_sigreturn = m_in.m_lc_pm_sig.ret;            // 84  trampoline 地址
  return(OK);                                         // 85
}
```

五段式，顺序不可调换：`SIGKILL` 早返 → `_NSIG` 边界 → `oact` 回写 → `act==0` 只读 → `sys_datacopy` 读新 → 三态位图联动 → 落盘+剥离。`68-71` 的 `SIG_IGN` 四联动含 `pending` 双清是 `1.2` 不对称的直接证据；`72-74` 的 `DFL` 不清 `pending` 与其互补。

### 2.2 `do_sigpending`（`signal.c:88-97`）

```c
int do_sigpending(void)
{
  assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL))); // 93

  mp->mp_reply.m_pm_lc_sigset.set = mp->mp_sigpending; // 95  只读 pending（不含 ksigpending/mask）
  return OK;                                            // 96
}
```

只读快照：把 `mp_sigpending` 写入回复消息的 `m_pm_lc_sigset.set`（`ipc.h:1760` `mess_pm_lc_sigset { sigset_t set }`，`_ASSERT_MSG_SIZE` 56B），由 `reply()` 发回调用者（`main.c:106`）。不改任何位图，不触发 `check_pending`。前置断言与 `do_sigaction` 相同——`PROC_STOPPED|VFS_CALL|UNPAUSED|EVENT_CALL` 时不允许 `sigpending`（进程已挂起在内核/事件侧，`sigpending` 的快照无意义）。

### 2.3 `do_sigprocmask`（`signal.c:99-155`）

```c
int do_sigprocmask(void)
{
  sigset_t set;
  int i;

  assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL))); //117

  set = m_in.m_lc_pm_sigset.set;                     //119  传入集（libc 实际掩码，非指针）
  mp->mp_reply.m_pm_lc_sigset.set = mp->mp_sigmask;  //120  先回旧 mask（lib 拷贝到用户指针）

  switch (m_in.m_lc_pm_sigset.how) {                 //122  how 来自同一个 mess_lc_pm_sigset
      case SIG_BLOCK:                                //123  1
    sigdelset(&set, SIGKILL);                        //124  剥离 KILL/STOP
    sigdelset(&set, SIGSTOP);                        //125
    for (i = 1; i < _NSIG; i++) {                    //126
        if (sigismember(&set, i))                    //127
            sigaddset(&mp->mp_sigmask, i);           //128  叠加
    }
    break;                                           //130

      case SIG_UNBLOCK:                              //132  2
    for (i = 1; i < _NSIG; i++) {                    //133
        if (sigismember(&set, i))                    //134
            sigdelset(&mp->mp_sigmask, i);           //135  全量清除（不剥离，见 1.4）
    }
    check_pending(mp);                               //137  解除阻塞→重检 pending
    break;                                           //138

      case SIG_SETMASK:                              //140  3
    sigdelset(&set, SIGKILL);                        //141  剥离
    sigdelset(&set, SIGSTOP);                        //142
    mp->mp_sigmask = set;                            //143  覆盖
    check_pending(mp);                               //144
    break;                                           //145

      case SIG_INQUIRE:                              //147  10 (minix 私有，signal.h:179)
    break;                                           //148  只读，无写入无重检

      default:                                       //150
    return(EINVAL);                                  //151  非法 how
    break;                                           //152  unreachable
  }
  return OK;                                         //154
}
```

`119` 的 `set` 是 libc 已拷贝的实际掩码（`signal.c:104-112` 注释"passes actual mask ... to save a copy"），`how` 为 `1/2/3/10`（`signal.h:174-179`），`120` 的旧 `sigmask` 写入 `mp_reply` 由 `reply()` 带回。`BLOCK` 的 `sigaddset` 循环（`126-129`）与 `UNBLOCK` 的 `sigdelset` 循环（`133-136`）逐位等价 `__sig*set` 的位操作，但 Minix 系统内宏为 `minix/include/lib.h` 的 `__sigaddset` 无检查版（`sys/signal.h:106-112` 的 `_KERNEL` 分支）。

### 2.4 `do_sigsuspend`（`signal.c:157-171`）

```c
int do_sigsuspend(void)
{
  assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL))); //162

  mp->mp_sigmask2 = mp->mp_sigmask;    /* save the old mask */            //164
  mp->mp_sigmask = m_in.m_lc_pm_sigset.set;                                //165
  sigdelset(&mp->mp_sigmask, SIGKILL);                                      //166
  sigdelset(&mp->mp_sigmask, SIGSTOP);                                      //167
  mp->mp_flags |= SIGSUSPENDED;                                             //168
  check_pending(mp);                                                        //169
  return(SUSPEND);                                                          //170  04 的 ReplyLater
}
```

六步原子：保存 `mask→mask2`（`164`）→ 覆盖 `mask`（`165`）→ 剥离 `KILL/STOP`（`166-167`）→ 置 `SIGSUSPENDED`（`168`）→ `check_pending`（`169`）→ `SUSPEND`（`170`，`main.c:106` 的 `ReplyLater`，`04` 的 `SUSPEND` 显式化的 `sigsuspend` 子类）。`165` 的 `m_lc_pm_sigset.set` 是调用者期望的新掩码（libc 的 `sigsuspend(&mask)` 传入）；`mask2` 仅 `sigsuspend` 路径的保存，`sigreturn` 不依赖它（`185` 的 `mask=set` 由调用方显式传）。

### 2.5 `do_sigreturn`（`signal.c:173-192`）

```c
int do_sigreturn(void)
{
  int r;

  assert(!(mp->mp_flags & (PROC_STOPPED | VFS_CALL | UNPAUSED | EVENT_CALL))); //183

  mp->mp_sigmask = m_in.m_lc_pm_sigset.set;                    //185  恢复
  sigdelset(&mp->mp_sigmask, SIGKILL);                         //186
  sigdelset(&mp->mp_sigmask, SIGSTOP);                         //187

  r = sys_sigreturn(who_e, (struct sigmsg *)m_in.m_lc_pm_sigset.ctx); //189  ctx 来自 sigsend 的 sigcontext
  check_pending(mp);                                           //190  掩码恢复后重检
  return(r);                                                   //191  OK 或 EFAULT 等
}
```

`185-187` 的恢复必须在 `189` 之前——即使 `sys_sigreturn` 失败（`EFAULT` 页错误，`sigmsg` 的栈帧不在用户栈），掩码已在 `185` 改写；`190` 的 `check_pending` 无论 `189` 成功与否都执行。`ctx` 为 `m_lc_pm_sigset.ctx`（`ipc.h:545` `vir_bytes ctx`，`who_e` 的 `sigmsg.ctx` 指向的 `sigcontext`，内核侧 `sys_sigreturn` 恢复寄存器）。

### 2.6 `sig_send`（`signal.c:776-855`）

```c
static int
sig_send(
    struct mproc *rmp,        /* what process to spawn a signal handler in */
    int signo            /* signal to send to process (1 to _NSIG-1) */
)
{
  struct sigmsg sigmsg;
  int i, r, sigflags, slot;

  assert(rmp->mp_flags & PROC_STOPPED);          //787  前置：必须已 stopped

  sigflags = rmp->mp_sigact[signo].sa_flags;     //789  SA_* 标志
  slot = (int) (rmp - mproc);                    //790  槽索引

  if (rmp->mp_flags & SIGSUSPENDED)              //792
    sigmsg.sm_mask = rmp->mp_sigmask2;           //793  sigsuspend 期间用 mask2
  else                                           //794
    sigmsg.sm_mask = rmp->mp_sigmask;            //795  否则用 mask
  sigmsg.sm_signo = signo;                       //796
  sigmsg.sm_sighandler =                         //797  handler 地址（0=DFL/1=IGN 不会到此）
    (vir_bytes) rmp->mp_sigact[signo].sa_handler;//798
  sigmsg.sm_sigreturn = rmp->mp_sigreturn;       //799  __sigreturn trampoline
  for (i = 1; i < _NSIG; i++) {                  //800  叠加 sa_mask
    if (sigismember(&rmp->mp_sigact[signo].sa_mask, i))
        sigaddset(&sigmsg.sm_mask, i);           //802
  }

  if (sigflags & SA_NODEFER)                     //805  0x10（signal.h:153）
    sigdelset(&sigmsg.sm_mask, signo);           //806  抵消当前信号的屏蔽（允许重入）
  else                                           //807
    sigaddset(&sigmsg.sm_mask, signo);           //808  默认屏蔽当前信号

  if (sigflags & SA_RESETHAND) {                 //810  0x04（signal.h:152）
    sigdelset(&rmp->mp_catch, signo);            //811  catch→DFL
    rmp->mp_sigact[signo].sa_handler = SIG_DFL;  //812
  }
  sigdelset(&rmp->mp_sigpending, signo);         //814  从 pending（与 kernel_pending）移除
  sigdelset(&rmp->mp_ksigpending, signo);        //815

  /* Ask the kernel to deliver the signal */
  r = sys_sigsend(rmp->mp_endpoint, &sigmsg);    //818
  /* sys_sigsend can fail legitimately with EFAULT or ENOMEM if the process
   * memory can't accommodate the signal handler.  The target process will be
   * killed in that case, so do not bother interrupting or resuming it.
   */
  if(r == EFAULT || r == ENOMEM) {               //823
    return(FALSE);                               //824  false→sig_proc 的 printf+killing
  }
  /* Other errors are unexpected pm/kernel discrepancies. */
  if (r != OK) {                                 //827
    panic("sys_sigsend failed: %d", r);          //828
  }

  /* Was the process suspended in PM? Then interrupt the blocking call. */
  if (rmp->mp_flags & (WAITING | SIGSUSPENDED)) { //832  PM 侧睡眠的两种
    rmp->mp_flags &= ~(WAITING | SIGSUSPENDED);  //833  清标志
    reply(slot, EINTR);                          //835  中断阻塞调用（wait4/sigsuspend）
    assert(!(rmp->mp_flags & UNPAUSED));         //840  UNPAUSED 不该置位（VFS 未介入）
    try_resume_proc(rmp);                        //842  尝试 resume（若无 VFS/EVENT 则 resume）
    assert(!(rmp->mp_flags & PROC_STOPPED));     //844  resume 后不该再 stopped
  } else {                                       //845  非 PM 睡眠（正被 VFS 解暂停或 normal）
    /* If the process was not suspended in PM, VFS must first have
     * confirmed that it has tried to unsuspend any blocking call.
     */
    assert(rmp->mp_flags & UNPAUSED);            //851  必须已 UNPAUSED（VFS 确认）
  }

  return(TRUE);                                  //854  成功
}
```

`792-799` 的 `sm_mask` 分支 + `800-808` 的 `sa_mask`/`SA_NODEFER` + `810-813` 的 `RESETHAND` + `814-815` 的 pending 清理 + `818-828` 的 `sys_sigsend` 三分支 + `832-851` 的 `WAITING|SIGSUSPENDED` vs `UNPAUSED` 二分，六段式不可调换——掩码演化在 `818` 之前，`RESETHAND` 的 `catch` 清理在 `818` 之后对后续信号生效但不影响本次投递，`pending` 清理在 `818` 之前使失败路径不残留 `pending`。

### 2.7 消息与类型（`ipc.h:532-551` / `type.h:73` / `sys/signal.h:97-180`）

* `mess_lc_pm_sig { pid_t pid; int nr; vir_bytes act; vir_bytes oact; vir_bytes ret }`（`ipc.h:532-540` `act/oact` 为 `vir_bytes` 指针，`ret` 为 `sigreturn` trampoline，`_ASSERT 56B`）—— `do_sigaction` 的三指针消息。
* `mess_lc_pm_sigset { int how; vir_bytes ctx; sigset_t set }`（`ipc.h:542-550` `how/set/ctx` 三字段，`_ASSERT 56B`）—— `do_sigprocmask/sigsuspend/sigreturn` 复用同一联合体成员（`how` 仅 `procmask` 用，`ctx` 仅 `sigreturn` 用，`set` 三者共用）。
* `struct sigmsg { sigset_t sm_mask; int sm_signo; vir_bytes sm_sighandler; vir_bytes sm_sigreturn }`（`type.h:73`）—— `sig_send` 的内核投递载荷。
* `SIG_DFL 0` / `SIG_IGN 1`（`signal.h:97-99`，`sa_handler` 的 `0/1` 特化）、`_NSIG 64`（`signal.h:45`）、`SA_NODEFER 0x10` / `SA_RESETHAND 0x04` / `SA_RESTART 0x02` / `SA_ONSTACK 0x01`（`signal.h:150-153`，本档仅前两者）、`SIG_BLOCK 1` / `UNBLOCK 2` / `SETMASK 3` / `INQUIRE 10`（`signal.h:174-180`，`10` 为 Minix 私有）、`SIGKILL 9` / `SIGSTOP 17`（`signal.h:62/70`）、`EINTR 4`（`errno.h:4`）、`EFAULT 14` / `ENOMEM 12`（`errno.h`）。

### 2.8 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `SIGKILL→OK` | `signal.c:49` | `SIGKILL` 不可重装 | 不变量（早返） |
| `_NSIG` 越界 `EINVAL` | `signal.c:50` / `123` | `signo<1 \|\| >=64` | 可恢复（`EINVAL`） |
| 掩码三剥离（`sa_mask`/`BLOCK`/`SETMASK`/`suspend`/`sigreturn` 五处） | `80-81/124-125/141-142/166-167/186-187` | `KILL/STOP` 写入 `sa_mask` 或 `mask` | 不变量（剥离） |
| `SIGSUSPENDED` 仅 `sigsuspend` 置位 | `signal.c:168` | `sigsuspend` 独有 | 不变量（布尔） |
| `sig_send` 前置 `PROC_STOPPED` | `signal.c:787` | 未 stopped 即 `sig_send` | 不可恢复（`assert`） |
| `EFAULT/ENOMEM→false` | `signal.c:823` | 用户栈不足 | 可恢复（`sig_proc` 的 `killing` 分支） |
| `panic(sys_sigsend ≠OK/EFAULT/ENOMEM)` | `signal.c:828` | PM/内核不一致 | 不可恢复 |
| `WAITING\|SIGSUSPENDED→EINTR+try_resume` | `signal.c:832-844` | PM 侧睡眠被信号中断 | 不变量（`EINTR` 语义） |
| `else→UNPAUSED` | `signal.c:845-851` | VFS 已解暂停 | 不变量（`UNPAUSED` 断言） |
| `SIGSUSPENDED` 的 `mask2` 分支 | `signal.c:792-795` | `sigsuspend` 期间捕获 | 不变量（`sm_mask` 为 `mask2`） |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `SigHandler` 三态 + `SigMaskOp` 枚举 + `without_unkillable` 一处方法 + `KernelSig` trait” 的 8 决策，保留 C 的 `sigaction` 三态位图联动与 `sig_send` 四步掩码，但以类型系统使 `KILL/STOP` 剥离与 `sigsuspend` 配对显式化。以下决策对应设计契约 `.design/12-design.v1.md` 的 D1–D8（正式文档不引用中间产物，见 `00-pm-overview.md` 的 Hidden Folder 约定）。

### D1：`handle_sigaction` 的 `SIGKILL` 保护与 `oact/act` 解耦

- **C**：`49` 的 `SIGKILL→OK` 早返与 `50` 的 `_NSIG` 边界纯整数比较，`53-60` 的 `oact/act` 以 `vir_bytes==0` 裸比较决定是否 `sys_datacopy`。
- **Rust**：`handle_sigaction(table, caller, req) -> Result<Option<SigAction>, SigActionError>`，其中 `req: SigActionReq { signo: i32, act: Option<SigAction>, need_oact: bool, sigreturn: VirBytes }`。`act==None` 对应 C 的 `act==0` 只读旧动作，`need_oact` 对应 `oact!=0` 需回写，`SIGKILL` 早返 `Ok(None)`（`Ok` 表示“安装成功”，`None` 表示“无新动作写入”），`_NSIG` 越界 `Err(InvalidSignal)`。
- **为什么**：`vir_bytes` 的 `0` 空指针与有效指针在 C 用整数比较，Rust 用 `Option` 在类型层表达“是否有效”；`sys_datacopy` 的指针有效性由 `minix-sys` 的 `UserMemory` trait 在真实路径落地，本档以 `Option<SigAction>` 显式化“指针是否有效”避免裸比较（与 `02-stage-vm` 的 `UserMemory` 抽象同型）。
- **替代方案**：保持 `vir_bytes act/oact` 裸字段。否决——裸 `0` 比较使 `oact` 回写与 `act` 读取的“是否有效”隐式，易漏 `act==0` 早返后仍读 `svec` 的 use-after-copy。

### D2：`SIG_IGN/DFL/CATCH` 三态的位图原子性

- **C**：`67-78` 的三分支各维护 `ignored/catch/pending/ksigpending` 的不同子集，`SIG_IGN` 需四联动（`68-71`），`DFL` 两清（`73-74`），`catch` 置位（`76-77`）。
- **Rust**：`SignalState::install(signo, handler, mask, flags, sigreturn)` 原子事务：`SIG_IGN: ignored|=bit; pending&=!bit; kernel_pending&=!bit; caught&=!bit`，`SIG_DFL: ignored&=!bit; caught&=!bit`，`catch: ignored&=!bit; caught|=bit`，后 `actions[signo] = SigAction{ handler: handler.0, mask: mask.without_unkillable(), flags }`。
- **为什么**：`SIG_IGN` 的 `pending` 双清（`69-70`）若遗漏则 `check_pending` 在后续 `UNBLOCK` 时重投已被声明忽略的信号——位图原子性保证“忽略即清理已挂起”。`DFL` 不清 `pending` 的不对称（`72-74` 无 `sigdelset(pending)`）由 Rust 同分支差异显式保留。
- **行为契约**：`pending` 清理含 `kernel_pending`（`69-70` 双清），`catch` 清理（`71`）与 `ignored` 清理（`76`）互斥分支，与 `67-78` 同序。

### D3：`sa_mask` 的 `KILL/STOP` 剥离一处方法化

- **C**：`80-81/124-125/141-142/166-167/186-187` 五处 `sigdelset(KILL/STOP)` 分散，新增 handler 时易漏一处即 P0。
- **Rust**：`SigSet::without_unkillable(self) -> SigSet { self & !UNKILLABLE_MASK }`，其中 `UNKILLABLE_MASK = (1u64<<(SIGKILL-1)) | (1u64<<(SIGSTOP-1))`（`SIGKILL=9→bit8, SIGSTOP=17→bit16`，`u64 0x10100`），所有安装/掩码入口统一 `mask = mask.without_unkillable()`。
- **为什么**：POSIX 不可阻断信号的不变量应在类型层一处锁定；方法化使“`BLOCK/SETMASK/suspend/sigreturn/sa_mask` 五处语义同一”在 `SigSet` 层保证。
- **替代方案**：每处手写两行 `sigdelset`。否决——已在 11 的 `without_unkillable` 提出，12 复用同一常量。

### D4：`do_sigprocmask` 的四 `how` 枚举化

- **C**：`122` 的 `switch(how)` 四分支，`BLOCK` 剥离后 `sigaddset` 循环（`124-129`），`UNBLOCK` 不剥离 `sigdelset` 循环（`132-136`）+ `check_pending`，`SETMASK` 剥离覆盖（`141-144`）+ `check_pending`，`INQUIRE` 空操作，`default→EINVAL`。
- **Rust**：`SigMaskOp::{Block, Unblock, SetMask, Inquire}` + `TryFrom<i32>`（`1→Block,2→Unblock,3→SetMask,10→Inquire` else `InvalidHow`），`SignalState::apply_mask_op(op, set) -> MaskOpEffect { Unchanged, Changed { needs_check: bool } }`，其中 `Block: self.mask |= set.without_unkillable()`，`Unblock: for bit in set { mask&=!bit } + needs_check(true)`，`SetMask: self.mask = set.without_unkillable() + needs_check(true)`，`Inquire: Unchanged`。
- **为什么**：`BLOCK` 不 `check_pending` 而 `UNBLOCK/SETMASK` 必须重检的差异（`137/144` vs `129`）在 `MaskOpEffect::needs_check` 显式化，调用方 `if effect.needs_check { check_pending }` 消除 `switch` 分支的隐式时序。
- **行为契约**：`KILL/STOP` 在 `Block/SetMask` 剥离而 `Unblock` 不剥离（`124-125` vs `133` 差异），`Inquire` 不改 `mask`（`147-148`），`EINVAL` 仅 `default` 分支（`150-151`）。

### D5：`do_sigsuspend` 的 `mask2` 保存与 `SUSPEND`

- **C**：`164` `mask2 = mask` 保存 + `165` 覆盖 + `166-167` 剥离 + `168` 置位 `SIGSUSPENDED` + `169` `check_pending` + `170` `SUSPEND`。
- **Rust**：`SignalState::prepare_suspend(new_mask) { self.mask_saved = self.mask; self.mask = new_mask.without_unkillable(); self.suspended = true }` + `handle_sigsuspend(table, caller, new_mask) -> ReplyIntent::ReplyLater`（保存后 `check_pending`，若有 `pending` 则 `sig_proc→sig_send` 在同 `check_pending` 链内完成，无则保持 `SUSPEND`）。
- **为什么**：`mask2` 与 `suspended` 必须同生同灭——`792-795` 的 `sm_mask` 分支依赖此配对，`sigreturn` 的恢复不依赖 `mask2`（`185` 由调用方显式传 `set`），分离使两者的不对称在类型层显式。

### D6：`do_sigreturn` 的掩码恢复与 `sys_sigreturn` 抽象

- **C**：`185` `mask = set + 剥离` 先于 `189` `sys_sigreturn`，`190` `check_pending` 无论成功与否都执行。
- **Rust**：`handle_sigreturn(table, caller, set, ctx, kernel: &mut dyn KernelSig) -> Result<(), SigReturnError>`，其中 `KernelSig::sigreturn(endpoint, ctx) -> Result<(), i32>` 抽象 `sys_sigreturn`（`A-3` 硬件抽象：PM 的 OS 层不暴露 `sigcontext` 寄存器布局），顺序 `restore_mask → sigreturn → check_pending` 与 `185-190` 紧邻不变。

### D7：`sig_send` 的 `sigmsg` 构造与 `SA_*` 四步

- **C**：`792-813` 的 `sigmsg` 四步掩码演化 + `810-813` 的 `RESETHAND` 位图清理 + `814-815` 的 `pending` 清理。
- **Rust**：`SignalState::prepare_sigmsg(signo, &self) -> SigMsg { mask: if suspended { mask_saved } else { mask }, signo, handler: actions[signo].handler, sigreturn }` + 叠加 `sa_mask.without_unkillable()` + `SA_NODEFER` 抵消（`mask & !bit(signo)` vs `mask|bit(signo)`）+ `SA_RESETHAND` 的 `caught&=!bit + handler=SIG_DFL(0)`，`pending&=!bit; kernel_pending&=!bit` 在 `sys_sigsend` 之前（与 `814-815` 同序）。
- **为什么**：`mask2` 分支（`792-795`）使 `sigsuspend` 期间捕获的 `sm_mask` 为进入前的掩码（POSIX：handler 结束后 `sigreturn` 恢复的 `mask` 为 `sm_mask`），`SA_NODEFER/RESETHAND` 的位操作与 `actions` 状态机联动在 `prepare_sigmsg` 一处显式。

### D8：`sig_send` 的 `sys_sigsend` 三分支与 `WAITING|SIGSUSPENDED` 后分岔

- **C**：`818` `sys_sigsend` → `823` `EFAULT/ENOMEM→false` vs `827` `panic` vs `832-851` 的 `WAITING|SIGSUSPENDED→EINTR+try_resume` vs `UNPAUSED` 二分。
- **Rust**：`KernelSig::sigsend(endpoint, &SigMsg) -> Result<(), SigSendError>`，其中 `SigSendError::FaultOrNoMem` 映射 `EFAULT/ENOMEM`（`false` 路径，`sig_proc` 的 `printf+killing` 由 11 的 `sig_proc` 兜底），`SigSendError::Unexpected(i32)` 触发 `panic`（与 `828` 同语义），`Ok` 后 `PostAction::{InterruptedWait(EINTR), AwaitVfsUnpause}` 枚举（`832` 分支 `reply(EINTR) + try_resume`，`845` 分支 `assert(unpaused)`）。
- **行为契约**：`EFAULT/ENOMEM→false`（`823-824`）与 `panic(828)` 的错误分层、`832` `WAITING|SIGSUSPENDED` 与 `845` `else` 互斥、`835` `reply(slot, EINTR)` 的 `slot = target - mproc`（`790`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-2 flag→枚举 | `SigMaskOp` 四 how + `SigHandler` 三态（D1/D4） | `signal_handlers.rs` + 本文档 §3.1/3.4 + `plan.md §4` |
| A-3 全局→显式 | `KernelSig` trait 抽象 `sys_sigsend/sigreturn`（D6/D8） | `signal_handlers.rs` 注释 + 本文档 §3.6/3.8 + `plan.md §4` |
| A-6 SUSPEND 显式化 | `sigsuspend→ReplyLater`（D5）+ `WAITING→EINTR`（D8） | `init.rs run_once` + 本文档 §3.5/3.8 + `plan.md §7.3` |
| A-11 64位 | `SigSet=u64` + `without_unkillable`（D3） | `mproc/signal.rs` + 本文档 §3.3 + `plan.md §4` |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   ├── signal.rs        — SignalState::install/apply_mask_op/prepare_suspend/prepare_sigmsg/without_unkillable + SigHandler/SigMaskOp/UNKILLABLE_MASK
│   └── block.rs         — BlockState::suspended（SIGSUSPENDED 布尔，与 02 的 stopped/unpaused 区分）
├── signal_handlers.rs   — handle_sigaction/sigpending/sigprocmask/sigsuspend/sigreturn/sig_send（D1-D8）
└── ipc/
    └── calls.rs         — PmCall::SigAction/SigPending/... 分发（值 47 以内，callnr.h）
```

### 4.2 `mproc/signal.rs`：位图与安装语义

```rust
pub const UNKILLABLE_MASK: SigSet = (1u64 << (SIGKILL-1)) | (1u64 << (SIGSTOP-1)); // 0x10100

impl SigSetExt for SigSet {
    fn without_unkillable(self) -> SigSet { self & !UNKILLABLE_MASK }
}

pub enum SigHandler { Default, Ignore, Catch(VirBytes) } // 0/1/addr
pub enum SigMaskOp { Block, Unblock, SetMask, Inquire } // 1/2/3/10

impl SignalState {
    pub fn install(&mut self, signo: u32, h: SigHandler, mask: SigSet, flags: i32, sigreturn: VirBytes)
    pub fn apply_mask_op(&mut self, op: SigMaskOp, set: SigSet) -> MaskOpEffect
    pub fn prepare_suspend(&mut self, new_mask: SigSet)
    pub fn restore_for_sigreturn(&mut self, set: SigSet)
    pub fn prepare_sigmsg(&mut self, signo: u32) -> SigMsg // 含 SA_* 四步（需 &mut 因 RESETHAND 改 caught）
    pub fn pending_snapshot(&self) -> SigSet { self.pending } // 只读 pending（sigpending 用）
}
```

`install` 的四位图原子性：`SIG_IGN: ignored|=b; pending&=!b; kernel_pending&=!b; caught&=!b`，`SIG_DFL: ignored&=!b; caught&=!b`，`Catch: ignored&=!b; caught|=b`。`pending` 的 `kernel_pending` 双清由 `mproc/signal.rs:138-167` 的 `add_pending` 对称保证。

`apply_mask_op` 的 `needs_check`：`Block|Inquire → false`，`Unblock|SetMask → true`（调用方 `if needs_check { check_pending }`），与 `signal.c:129/137/144` 同条件。

`prepare_sigmsg` 的 `suspended` 分支：`sm_mask = if suspended { mask_saved } else { mask }`（`792-795`），后叠加 `sa_mask`（`800-803`）与 `SA_*`（`805-813`），再 `pending&=!b; kernel_pending&=!b`（`814-815`）。`RESETHAND` 时 `actions[signo].handler=0`（`SIG_DFL`）。

### 4.3 `signal_handlers.rs`：IPC handler 层

```rust
pub struct SigActionReq { pub signo: i32, pub act: Option<SigAction>, pub need_oact: bool, pub sigreturn: VirBytes }
pub enum SigActionError { InvalidSignal, Fault }
pub enum SigMaskError { InvalidHow }
pub enum SigReturnError { Fault(i32) }
pub trait KernelSig { fn sigsend(&mut self, ep: Endpoint, msg: &SigMsg) -> Result<(), SigSendError>; fn sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32>; }
pub enum SigSendError { FaultOrNoMem, Unexpected(i32) }
pub enum PostAction { InterruptedWait, AwaitVfsUnpause }

pub fn handle_sigaction(table: &mut ProcTable, caller: UserSlot, req: SigActionReq) -> Result<Option<SigAction>, SigActionError>
pub fn handle_sigpending(table: &ProcTable, caller: UserSlot) -> SigSet
pub fn handle_sigprocmask(table: &mut ProcTable, caller: UserSlot, how: i32, set: SigSet) -> Result<(SigSet, MaskOpEffect), SigMaskError>
pub fn handle_sigsuspend(table: &mut ProcTable, caller: UserSlot, set: SigSet) -> ReplyIntent // always ReplyLater
pub fn handle_sigreturn(table: &mut ProcTable, caller: UserSlot, set: SigSet, ctx: VirBytes, k: &mut dyn KernelSig) -> Result<(), SigReturnError>
pub fn sig_send(table: &mut ProcTable, target: UserSlot, signo: i32, k: &mut dyn KernelSig) -> Result<PostAction, SigSendError>
```

* `handle_sigaction`：`signo==SIGKILL→Ok(None)`（`49` 早返）→ `_NSIG` 越界 `Err(InvalidSignal)` → `need_oact` 时 `old = actions[signo]` 克隆 → `act==None → Ok(old)` 只读 → 否则 `install(signo, ...)` + `Ok(old)`。
* `handle_sigpending`：`Ok(table[caller].pending)` 纯只读。
* `handle_sigprocmask`：旧 `mask` 快照 → `SigMaskOp::try_from(how)` → `apply_mask_op` → 调用方据 `needs_check` 决定 `check_pending`（本档只到 `Effect`，13 的 `check_pending` 在 `run_once` 层触发）。
* `handle_sigsuspend`：`prepare_suspend(set)` → `ReplyLater`（`SUSPEND` 显式化，`04` 的 `ReplyLater` 自杀子类之外）。
* `handle_sigreturn`：`restore_for_sigreturn(set)`（`without_unkillable` 在内）→ `k.sigreturn(ep, ctx)` → `check_pending`（无论成功与否，失败码透传）。
* `sig_send`：`assert!(block.stopped)`（`787`）→ `prepare_sigmsg`（含四步）→ `k.sigsend` 三分支（`FaultOrNoMem→Err(FaultOrNoMem)`，`Unexpected→panic`）→ `PostAction` 二分（`WAITING||suspended → InterruptedWait` 且 `reply(EINTR)` 由调用方 `signal.rs` 的 `sig_proc` 路径完成，本档只返枚举；`else → AwaitVfsUnpause + debug_assert!(unpaused)`）。

### 4.4 `os/libs/minix-types/src/ipc/message.rs`：消息联合体对齐

`mess_lc_pm_sig { pid: i32, nr: i32, act: u64, oact: u64, ret: u64 }`（`act/oact/ret` 为 `VirBytes`，`ipc.h:532-540` 的 `vir_bytes act/oact/ret`），`mess_lc_pm_sigset { how: i32, ctx: u64, set: SigSet }`（`ipc.h:542-549` 的 `how/set/ctx`），`mess_sigcalls { map: SigSet, endpt: i32, sig: i32, sigctx: u64 }`（`ipc.h:1912-1922`，`sys_sigsend/sigreturn` 共用） + `SigMsg { sm_mask, sm_signo, sighandler, sigreturn }`（`type.h:73`），各 `_ASSERT_MSG_SIZE 56B`，与 `MessageUnion` 新增 `m_lc_pm_sig` / `m_lc_pm_sigset` / `m_sigcalls` 成员。

常量 `SIGKILL/SIGSTOP/_NSIG/SA_NODEFER/SA_RESETHAND/SIG_BLOCK/UNBLOCK/SETMASK/INQUIRE` 收敛到 `minix-types`（单一真相，`sys/signal.h:97-180` 数值锁定，由 `test_signal_constants_match_signal_h` 守卫）。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `SIGKILL` 不可重装 | `signal.c:49` | `signo==SIGKILL→Ok(None)` 早返 | `debug_assert` 不可达 `install(KILL)` |
| 2 | `KILL/STOP` 不可屏蔽 | `80-81/124-125/141-142/166-167/186-187` | `without_unkillable` 一处方法 | `test_without_unkillable_removes_kill_stop` |
| 3 | `SIG_IGN` 四联动 | `68-71` | `install(Ignore)` 原子四清 | `test_sigaction_ignore_clears_pending_and_catch` |
| 4 | `UNBLOCK/SETMASK` 后重检 | `137/144` | `MaskOpEffect::needs_check` | `test_sigprocmask_unblock_triggers_check` |
| 5 | `suspend` 的 `mask2` 配对 | `164/792-795` | `prepare_suspend` + `prepare_sigmsg` 的 `suspended` 分支 | `test_sigsuspend_saves_mask2_and_sigsend_uses_it` |
| 6 | `sig_send` 前置 `PROC_STOPPED` | `787` | `assert!(block.stopped)` | `test_sig_send_requires_stopped` |
| 7 | `EFAULT/ENOMEM→false` | `823` | `SigSendError::FaultOrNoMem` | `test_sig_send_fault_returns_false` |
| 8 | `WAITING\|SIGSUSPENDED→EINTR` | `832-844` | `PostAction::InterruptedWait` | `test_sig_send_waiting_returns_eintr` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-02 为 **116 passed / 0 failed**（原 91 + 本档新增 ~25：`mproc/signal.rs` 8 + `signal_handlers.rs` 17）。`cargo test -p minix-types --lib` 66 passed（新增 `test_signal_constants` 等）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `mproc/signal.rs`（位图与安装语义）

- `test_without_unkillable_removes_kill_stop`：`SigSet` 的 `without_unkillable` 剥离 `KILL(9)/STOP(17)` 两位（`80-81` 五处语义一处锁定）
- `test_sigaction_ignore_clears_pending_and_catch`：`SIG_IGN` 的 `pending/kernel_pending/catch` 三清（`68-71`）
- `test_sigaction_dfl_keeps_pending`：`SIG_DFL` 保留 `pending`（`72-74` 不对称）
- `test_sigaction_catch_sets_caught`：`catch` 置 `caught` 清 `ignored`（`75-77`）
- `test_apply_mask_op_block_no_check` / `test_apply_mask_op_unblock_needs_check` / `test_apply_mask_op_setmask` / `test_apply_mask_op_inquire`：四 `how` 的 `needs_check` 差异（`129/137/144/148`）与 `KILL/STOP` 剥离差异
- `test_prepare_suspend_saves_mask2`：`mask→mask2` 保存 + `SIGSUSPENDED` 置位（`164/168`）
- `test_prepare_sigmsg_uses_mask2_when_suspended`：`sigsuspend` 期间 `sm_mask = mask2`（`792-795`）
- `test_prepare_sigmsg_sa_nodefer_and_resethand`：`SA_NODEFER` 抵消与 `RESETHAND` 清 `caught`（`805-813`）
- `test_sig_handler_default/ignore/catch`：`SigHandler` 三态与 `SigAction` 堆分配

### 5.2 `signal_handlers.rs`（IPC handler 层）

- `test_sigaction_kill_returns_ok`：`sig_nr==SIGKILL → Ok(None)`（`49` 早返）
- `test_sigaction_invalid_signal`：`signo<1||>=64 → EINVAL`（`50`）
- `test_sigaction_oact_only_reads`：`act==None → Ok(old)` 只读（`59-60`）
- `test_sigaction_mask_strips_unkillable`：`sa_mask` 含 `KILL/STOP` 被剥离（`80-81`）
- `test_sigpending_snapshot`：`sigpending` 只读 `pending`（`95` 不含 `ksigpending/mask`）
- `test_sigprocmask_block_strips` / `test_sigprocmask_unblock_does_not_strip`：`BLOCK` 剥离 `KILL/STOP` 而 `UNBLOCK` 不剥离（`124-125` vs `133`）
- `test_sigprocmask_invalid_how`：非法 `how → EINVAL`（`150-151`）
- `test_sigsuspend_returns_reply_later`：`sigsuspend` 恒 `ReplyLater`（`170 SUSPEND`）
- `test_sigsuspend_preserves_kill_stop`：`sigsuspend` 的新 `mask` 经 `without_unkillable`（`166-167`）
- `test_sigreturn_restores_and_calls_kernel`：`sigreturn` 的 `mask=set→sigreturn→check_pending` 三步序（`185-190`）
- `test_sig_send_requires_stopped`：`sig_send` 前置 `PROC_STOPPED` 断言（`787`）
- `test_sig_send_fault_returns_false`：`EFAULT/ENOMEM → Err(FaultOrNoMem)`（`823`）
- `test_sig_send_unexpected_panics`：其他错误 `panic`（`828`，`#[should_panic]`）
- `test_sig_send_waiting_vs_unpaused`：`WAITING|SIGSUSPENDED → InterruptedWait` vs `else→AwaitVfsUnpause`（`832-851`）

### 5.3 `minix-types`（消息与常量）

- `test_signal_constants_match_signal_h`：锁定 `SIGKILL=9/SIGSTOP=17/_NSIG=64/SA_NODEFER=0x10/SA_RESETHAND=0x04` 等于 `sys/signal.h:45/62/70/152-153`
- `test_mess_lc_pm_sig_roundtrip` / `test_mess_lc_pm_sigset_roundtrip` / `test_sigmsg_roundtrip`：消息编解码往返（`ipc.h:532-551` + `type.h:73`）

测试策略：位图语义在 `mproc/signal.rs` 纯逻辑层验证（无需 `ProcTable`）；IPC 语义在 `signal_handlers.rs` 经 `ProcTable` + mock `KernelSig` 验证；`sys_sigsend/sigreturn` 的三分支经 `TestKernelSig` 可注入错误码；`check_pending` 的触发由 `MaskOpEffect` 断言（本档只到 `needs_check`，13 再验证实际重投）。

---

## 6 过渡

本篇在 11 的 `sig_proc→unpause→sig_send` 的 handler 语义位置，是 13 的 `stop_proc`/`try_resume_proc`/`check_pending`/`restart_sigs` 的前置：

```
11-signal-core.md（check_sig的权限门 + sig_proc的9链：ignore→block→TRACE_STOPPED→caught→unpause→sig_send→terminate）
  │
  └─► 本章（sigaction的三态安装 + sigprocmask的四how + sigsuspend/return成对 + sig_send的sigmsg四步翻译 + sys_sigsend三分支 + EINTR/UNPAUSED二分）
         │
         └─► 13-signal-flow.md（check_pending的pending重投 + restart_sigs的PROC_STOPPED重检 + stop_proc的may_delay/DELAY_CALL + try_resume_proc + unpause的WAITING→stop + VFS UNPAUSE往返 + SIGSNDELAY延迟恢复）
               │
               └─► 14-itimer.md（alarm的SIGALRM经 kill→本章sigsuspend/sigreturn可被中断）
```

VFS 解暂停路径（`05-vfs-interaction.md` 的 `VFS_PM_UNPAUSE_REPLY → UNPAUSED → publish_event → resume_event → restart_sigs → check_pending → sig_send`）与本章的 `catch` 分支的 `unpause` 在 13 闭环——本章的 `sig_send` 已把 `sm_mask` 的 `mask2` 分支与 `SA_*` 语义锁定，13 再补 `VFS_CALL|EVENT_CALL` 挂起与 `PROC_STOPPED` 的延续。

阅读顺序提示：若想先理解“信号安装后如何被投递”，下一站 `13-signal-flow.md`；若想理解“信号处理器如何被内核执行”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md`（内核 `sys_sigsend` 的 `sigframe` 推送与 `sys_sigreturn` 的 `sigcontext` 恢复）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/signal.c:40-192`（`do_sigaction`/`do_sigpending`/`do_sigprocmask`/`do_sigsuspend`/`do_sigreturn`）、`minix3/minix/servers/pm/signal.c:776-855`（`sig_send`）、`minix3/minix/servers/pm/mproc.h:52-64`（信号位图与动作表）、`minix3/minix/include/minix/ipc.h:532-551`（`mess_lc_pm_sig`/`mess_lc_pm_sigset`）、`minix3/minix/include/minix/type.h:73`（`sigmsg`）、`minix3/sys/sys/signal.h:97-180`（`SIG_DFL/IGN`/`SA_*`/`_NSIG`/`SIGKILL/STOP`）
- PM 阶段文档：11-signal-core.md（`sig_proc` 的 `ignore/catch/mask/pending` 消费与 `unpause` 调用点）、02-mproc-struct.md（`SignalState` 四位图与 `SigAction` 堆分配）、04-ipc-dispatch.md（`ReplyIntent::ReplyLater` 的 `SUSPEND` 三子类与 `EINTR` 中断）、13-signal-flow.md（`check_pending`/`restart_sigs`/`stop_proc`/`try_resume_proc`/`unpause` 的停止/延迟/恢复，本文只到调用点）、01-pm-init-main.md（`core_sset`/`ign_sset`/`noign_sset` 的初始化）、09-pm-exit.md（`sig_proc_exit` 的 `exit_proc` 终止）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md`（`sys_sigsend`/`sys_sigreturn`/`sys_delay_stop`/`sys_resume`/`sigframe`/`sigcontext`）
- 阶段内顺序：11 → **本章（12）** → 13 → 14 → 15 → 16（`sched_stop` 的直毁与 `SIGKILL` 保护）→ 17（`exec` 的 `pending` 清理与 `caught` 重置）→ 18（`trace_stop` 的 `TRACE_STOPPED`）
- OS 模式参考：Linux `sigaction`/`sigqueue`/`sigprocmask`（`kernel/signal.c` 与 `include/uapi/asm-generic/signal.h`）、Redox `SigAction`/`SigQueue`（`common/src/signal.rs` 与 `kernel/src/signal.rs`）、`seL4 Notification`（用户态能力替代信号，见 `01-stage-kernel` 对比）
- Rust 实现：`os/servers/pm/src/mproc/signal.rs`（`SignalState` 位图与 `SigAction`）、`os/servers/pm/src/signal_handlers.rs`（5 个 handler + `sig_send` 翻译）、`os/libs/minix-types/src/ipc/message.rs`（消息联合体与 `SigMsg`）

