# 07 — PM 侧 `do_fork` 全链路

本文讲清 PM 的 `do_fork` 如何在一次 `PM_FORK` 系统调用中完成"进程身份的创建"：从容量与槽位预检、到 VM 的地址空间复制、到 PM 进程表项的显式构造、到 PID 分配、到 VFS 的文件描述符表投递、到 tracer 的 `SIGSTOP`，以及为什么 PM 在此投递后立即以 `SUSPEND` 让出延续——回复由 05 的 `handle_vfs_reply` 在 `VFS_PM_FORK_REPLY` 到达时异步完成。

前置阅读：03-mproc-table.md（`ProcTable` 槽位轮转 / `PidGenerator::get_free_pid`）、04-ipc-dispatch.md（`PmCall::Fork → ReplyLater` 契约）、05-vfs-interaction.md（`tell_vfs` 三段式与 `handle_vfs_reply` 的 FORK 双分支）、02-mproc-struct.md（`RemainingFlags::TAINTED` / `mpsigact` 独立表）、06-event-subscription.md（`mp_eventsub == NO_EVENTSUB` 不变量）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 PM 主循环分发与 VFS 异步延续（04/05），知道 `ProcTable` 的三层身份（slot/endpoint/pid）与 `BlockState` 的 `VFS_CALL` 挂起的开发者。

> **本章不讲什么**：
> - VM 侧页表与地址空间的实际复制（`02-stage-vm/18-vm-fork.md`，`VM_FORK` 对端）
> - VFS 侧 fd 表的实际复制（`05-stage-vfs`，`VFS_PM_FORK` 对端）
> - `sched_start_user` 的调度器内部（16-scheduling.md）
> - `sig_proc(SIGSTOP)` 的信号投递（11-signal-core.md）
> - `srv_fork` 的 `PRIV_PROC` 继承差异（08-pm-srv-fork.md）
>
> 本章只回答一个问题：**PM 如何在一次 `fork` 中，以不可回滚窗口为界，把"创建一个新进程"拆为同步可失败的前半与异步投递后 SUSPEND 的后半**。

### 1.1 为什么需要 fork：资源的隔离复制

进程是**资源容器**：地址空间（VM）、文件描述符表（VFS）、调度上下文（kernel / SCHED）、信号状态（PM）。`fork` 的本质是容器的**浅复制**——子进程获得父容器的大部分状态快照，但拥有独立的生命周期（独立 `slot` / `endpoint` / `pid`），后续任一方修改不再互相可见（写时复制在 VM 侧懒触发，本章只到 PM 的 `vm_fork` 调用点）。

与线程 `pthread_create` 的对比凸显设计取舍：线程共享地址空间与 fd 表，创建成本低但隔离弱；`fork` 共享代价高（VM 需复制页表、VFS 需复制 fd 引用计数），但隔离强——父进程可继续持有旧资源而不被子进程篡改。PM 选择 `fork` 作为进程家族的唯一创建原语（`07-pm-fork`）与 `srv_fork`（08，RS 专用）的双轨，正是隔离性要求的体现。

### 1.2 为什么是多服务协同：三表分离的一致性窗口

微内核把进程表的三份拷贝分离成三个权威：

- **PM/mproc**：身份、凭证、信号、调度器指针；
- **VM/vmproc**：页表、地址空间；
- **VFS/fproc**：fd 表、cwd、root。

三表以 `slot`（`0..NR_PROCS`）为坐标对齐，以 `endpoint`（`slot + generation`）为跨服务身份。三服务同时持有同一进程的槽位，因此一次 `fork` 必须三次会合——PM 先占槽，VM 复制地址空间，VFS 复制 fd 表。若任一服务失败，已占资源需回滚；若部分成功后服务崩溃，需由 RS 重启该服务并依 `endpoint` 重新同步（`01-stage-kernel` 的服务重启语义）。

### 1.3 为什么分两阶段：同步可失败的前半与异步 SUSPEND 的后半

`do_fork` 的注释（`forkexit.c:56-57`）点明前提：

> *If tables might fill up during FORK, don't even start since recovery half way through is such a nuisance.*

前半（容量检查→槽位轮转→`vm_fork`）是**同步可失败**的：任一步失败可直接 `return EAGAIN/ENOMEM` 回复父进程，无 side-effect 残留（`vm_fork` 失败前未 `procs_in_use++`，未 `*rmc=*rmp`，无需清理）。

后半在 `vm_fork` 成功后开启**不可回滚窗口**（`forkexit.c:82`）：

> *PM may not fail fork after call to vm_fork(), as VM calls sys_fork().*

VM 已调用内核 `sys_fork` 复制了 `proc` 与页表，PM 若此时 `EAGAIN` 会留下 VM 侧已复制但 PM 侧未占位的孤儿。Rust 侧的 `alloc_slot` 已在 `vm_fork` 前 `procs_in_use++`，因此 `vm_fork` 失败时需显式 `release_slot` 回滚——这是 C 与 Rust 顺序差异的补偿（见 §3.3/3.5）。

VFS 投递的异步性同样源于两阶段：`VFS_PM_FORK` 经 `asynsend3`（`tell_vfs` 的 `AMF_NOREPLY`）投递后 PM **不能阻塞等待**，否则与 05 的死锁论证同构（PM 等 VFS，VFS 某路径又需 PM，见 05 §1.2）。因此 PM 在 `tell_vfs` 后立即 `return SUSPEND`（`forkexit.c:139`），延续挂在**子进程**的 `VFS_CALL` 上，回复由 05 的 `handle_vfs_reply` 在 `VFS_PM_FORK_REPLY` 到达时异步完成（`sched_start_user` 成败双分支 + `reply(parent/child)`）。

### 1.4 子进程的第一口身份：PID 与 slot 的正交

`slot` 是服务间坐标（轮转复用的 `next_child`），`PID` 是 POSIX 用户可见命名（`30000` 空间内全局唯一，`const.h:3` `NR_PIDS 30000`），`endpoint` 是跨服务身份（`slot + generation`，`kernel/system/do_fork.c:69-72` 在槽位复用时递增）。三者正交：

- `slot` 复用快（`find_free_slot` 轮转，`NR_PROCS=256`）；
- `PID` 复用慢（`get_free_pid` 扫描 `mp_pid`/`mp_procgrp` 双字段，`NR_PIDS` 30000）；
- `endpoint` 的 `generation` 由内核在 `sys_fork` 时对复用槽位递增，PM 只验证不生成（03 §2.1）。

`get_free_pid` 的 `next_pid` 轮转与 `mp_procgrp` 冲突扫描（`utility.c:34-74`）正是"命名 vs 坐标"正交的证据：`procgrp` 亦占用 PID 命名空间，因此 `get_free_pid` 需同时避免 `pid` 与 `procgrp` 碰撞。

### 1.5 fork 的延续：SUSPEND 挂在子进程

`do_fork` 的 `return SUSPEND`（`forkexit.c:139`）是 04 的 `ReplyIntent::ReplyLater` 的起源：`main.c:106` 的 `if (result != SUSPEND) reply` 不回复本次 `PM_FORK`，延续由 `handle_vfs_reply` 的 FORK 分支完成（05 §2.4/§4.3）：

- `sched_start_user` 成功 → `reply(child, OK)` + `reply(parent, child_pid)`（`!new_parent` 保护）；
- `sched_start_user` 失败 → `exit_proc(child, -1)` 拆解孤儿 + `reply(parent, -1)`。

延续挂在**子进程**（`tell_vfs(child_slot, VfsCall::Fork)` 的 `VFS_CALL` 置于子槽，`forkexit.c:130` 的 `rmc`），而非父进程——父进程在 `do_fork` 返回后即无 `VFS_CALL`，等待的是子进程的 VFS 往返。这是《UNIX fork 语义 vs 微内核实现》的典型错位：用户视角"父进程 fork"在内部实现为"子进程的 VFS 投递"。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `*rmc=*rmp`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `fork`/`clone`。** Linux 的 `clone(CLONE_VM|CLONE_FS|...)` 把"共享哪些资源"拆为位标志，`fork` 即 `clone(SIGCHLD)`。Minix3 的 `fork` 无此类细粒度标志，PM 的 `*rmc=*rmp` 实为 `CLONE_ALL`（除 `PRIV_PROC` 调度接管与 `IN_USE|TAINTED` 过滤外全继承）。Linux 之后引入 `clone3` 的 `struct clone_args` 进一步显式化参数；PM 的 Rust 改写 `Process::fork_from` 显式构造与 `clone_args` 同构（编译期穷尽字段），但共享策略保持 Minix3 的"全继承"以保兼容。

**Redox `Scheme` 的进程创建。** Redox 的进程创建与 `Scheme::open` 解耦：`exec` 由 `acquire` 新 Scheme 承载，fork 的资源复制更多在 `Scheme` 侧。PM 的三表协同与 Redox 解耦同向，但 Redox 因 `Scheme` 统一文件与进程抽象而可"先创 Scheme 再映射"，PM 需先占 `slot` 再调 VM（VM 需 `child_slot` 知道目标槽位）——顺序差异源于"谁持有目标槽位"的权威归属不同。

**seL4 的 `TCB` + `CNode` 手动装配。** seL4 无 `fork` 原语，创建新线程需手动 `retype` `TCB`、`CNode`、`VSpace` 并装配。Minix3 的 `fork` 原语一次性完成装配（PM 负责身份、VM 负责页表、VFS 负责 fd），`sys_fork` 的内核 `proc` 复制是 seL4 手动装配的自动化。Rust 侧 `handle_fork` 的跨服务编排（`vm_fork → copy_mproc → tell_vfs`）与 seL4 的手动装配同为"显式装配"，差异在于 Minix3 由 PM 统一编排而非调用者自行。

**结论（本章的设计基线）。** 把 C 的"半途检查 + 整槽复制 + 裸 `tell_vfs` + 隐式 SUSPEND"改写为"显式协调器 `handle_fork`（跨服务编排）+ 显式构造 `Process::fork_from`（字段级复制）+ 类型化投递 `VfsCall::Fork` + 显式延续 `ReplyLater`"。PM 先占槽→VM 先复制→VFS 后投递的顺序与 `forkexit.c:60-139` 逐行对齐，又因 Rust 显式构造而使 `make impossible to forget a field`（新增字段需更新 `fork_from`，编译器强制）。

### 1.7 小结

1. **为什么 fork**——资源容器的隔离复制，与线程的共享语义正交。
2. **为什么多服务**——三表分离需三次会合，PM 只负责身份与凭证，其余由 VM/VFS 完成。
3. **为什么两阶段**——`vm_fork` 前可失败回返，`vm_fork` 后不可失败（`sys_fork` 已复制内核 proc），VFS 投递异步 SUSPEND（`asynsend3`，05 死锁论证）。
4. **身份正交**——`slot` 轮转复用、`PID` 慢轮转全局唯一、`endpoint` 的 `generation` 由内核递增。
5. **延续位置**——`SUSPEND` 挂在**子进程**的 `VFS_CALL`，由 05 的 `VFS_PM_FORK_REPLY` 异步双回复。

下一章逐行分析 C 的 `do_fork`；第 3 章给出 Rust 的显式构造与协调器。

---

## 2 C 源码分析

### 2.1 容量检查：`procs_in_use` 与 `LAST_FEW`（forkexit.c:32/60-65）

```c
#define LAST_FEW  2                      // forkexit.c:32

rmp = mp;                                // forkexit.c:59 当前进程
if ((procs_in_use == NR_PROCS) ||        // forkexit.c:60-65
    (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    printf("PM: warning, process table is full!\n");
    return(EAGAIN);                       // sys/errno.h:35
}
```

`procs_in_use` 为 `glo.h:9` 全局计数（`mproc[NR_PROCS]` 中 `IN_USE` 槽数）；`LAST_FEW=2` 为预留给 superuser 的最后槽位（`00` 与 `NR_PROCS-1` 等效边界），`mp_effuid` 即 `privilege.credentials.effective`（`mproc.h:42`），`0` 为 superuser。三层判定：全满（`==NR_PROCS`）→ 全拒；近满（`>=NR_PROCS-2`）且非 root → 拒；其余可分配。`EAGAIN`（`try again`）与 Rust `ForkError::TableFull/ReservedForRoot → EAGAIN` 同映射（`mproc/fork.rs:88-94`）。

### 2.2 槽位轮转：`next_child` 与双 `panic` 守卫（forkexit.c:51/68-75）

```c
static unsigned int next_child = 0;      // forkexit.c:51 文件级轮转指针
int n = 0;
do {
    next_child = (next_child+1) % NR_PROCS; // forkexit.c:69 先递增后检查
    n++;
} while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
if(n > NR_PROCS) panic("do_fork can't find child slot");                         // forkexit.c:72-73
if(next_child >= NR_PROCS || (mproc[next_child].mp_flags & IN_USE))
    panic("do_fork finds wrong child slot: %d", next_child);                     // forkexit.c:74-75
```

轮转语义为"下一次从 `next_child+1` 开始找空槽"（与 `03-mproc-table.md` 的 `find_free_slot` 同序，先递增）；`n` 计数至 `NR_PROCS+1` 时 `panic`——此为不可达守卫（容量检查已保证存在空槽）；第二 `panic` 为"找到的槽位仍 `IN_USE`"的二次守卫（`next_child >= NR_PROCS` 实为类型守卫，`unsigned` 永非负，但 `NR_PROCS` 越界检查与 `pm_isokendpt` 的 `slot >= NR_PROCS → EINVAL` 同源）。

### 2.3 `vm_fork` 同步段与不可失败窗口（forkexit.c:78-82）

```c
if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;                             // forkexit.c:78-80 同步失败即返
}
/* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */ // forkexit.c:82
```

`vm_fork` 在 `minix/vm.h` 原型为 `int vm_fork(endpoint_t, int slot, endpoint_t *)`（`m1_i1/i2` 载荷，`VM_FORK 0xC01`），VM 侧经 `sys_fork`（`kernel/system/do_fork.c:69-72`）复制 `proc` 与页表并生成新 `endpoint`（`slot + generation`）。`s` 已是 `errno`（`EAGAIN` 表满或 `ENOMEM` 内存不足），PM 直接返 `s`；成功后即进入不可失败窗口，后续 `EAGAIN` 不再合法（Rust 侧 `alloc_slot` 的 `procs_in_use++` 已在 `vm_fork` 前，需在失败时回滚，见 §3.3）。

### 2.4 槽位占位与全量复制：`procs_in_use++` → `*rmc=*rmp` → 子资源重整（forkexit.c:84-116）

```c
rmc = &mproc[next_child];                 // forkexit.c:84
procs_in_use++;                           // forkexit.c:86 先占计数
*rmc = *rmp;                              // forkexit.c:87 整槽复制（15 字段 + 56 字节 union）
rmc->mp_sigact = mpsigact[next_child];    // forkexit.c:88 重指 sigact 指针（外置二维 `mpsigact[NR_PROCS][NSIG]`，占 80% per-process 状态）
memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child])); // forkexit.c:89
rmc->mp_parent = who_p;                   // forkexit.c:90 父槽位（`glo.h:16` `who_p`）
if (!(rmc->mp_trace_flags & TO_TRACEFORK)) { // forkexit.c:91-95
    rmc->mp_tracer = NO_TRACER;           // 0，无追踪
    rmc->mp_trace_flags = 0;
    (void) sigemptyset(&rmc->mp_sigtrace);
}

if (rmc->mp_flags & PRIV_PROC) {          // forkexit.c:100-103 PRIV_PROC 父的特权接管
    assert(rmc->mp_scheduler == NONE);    // 系统进程的 scheduler 必为 NONE（启动期 `NONE`，见 01）
    rmc->mp_scheduler = SCHED_PROC_NR;    // 4，SCHED 服务
}

/* Inherit only these flags. */           // forkexit.c:105-106
rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
rmc->mp_child_utime = 0;                  // forkexit.c:107-114 子资源清零
rmc->mp_child_stime = 0;
rmc->mp_exitstatus = 0;
rmc->mp_sigstatus = 0;
rmc->mp_endpoint = child_ep;              // VM 返回的 endpoint（generation 已递增）
for (i = 0; i < NR_ITIMERS; i++) rmc->mp_interval[i] = 0;
rmc->mp_started = getticks();             // 启动时间，`ps(1)` 用

assert(rmc->mp_eventsub == NO_EVENTSUB);  // forkexit.c:116 新进程无事件订阅游标（06 不变量）
```

逐行要点：

- `procs_in_use++` 在 `*rmc=*rmp` 之前（`86`），`get_free_pid` 在 `*rmc=*rmp` 之后（`119`）——顺序与 `mproc` 全量复制的覆盖时序相关，`get_free_pid` 扫描 `mp_pid`/`procgrp` 需在子进程槽已占但 `mp_pid` 旧值未覆盖前完成？实则 `get_free_pid` 扫描包含子进程新占槽的旧 `mp_pid`（此时仍为父的 `pid`），若该旧值恰为 `next_pid` 会导致误判冲突；但 `next_pid` 轮转算法在冲突时 `next_pid++`，最终仍会跳过该旧值——顺序差异不影响可观测行为（Rust 侧显式构造无此整拷贝副产物）。
- `mpsigact` 外置：`mproc.h:22` 的 `mpsigact[NR_PROCS][_NSIG]`（`_NSIG=32`）占 80% per-process 状态，PM 为避免 `MIB` 拉取而外置（`forkexit.c:88-89` 重指 + `memcpy`），Rust 侧 `SignalState::actions: [SigAction; 32]` 按槽位索引（`mproc/signal.rs`）。
- `PRIV_PROC` 接管：仅当父为 `PRIV_PROC`（`mproc.h:98` `0x02000`）且 `mp_scheduler==NONE` 时，子进程转为 `User` 并 `scheduler=SCHED`（`forkexit.c:101` `SCHED_PROC_NR 4`），对应 Rust `Privilege::Kernel → User(root)`（`mproc/fork.rs:277-291`），`PRIV_PROC` 不继承（`106` 的 `IN_USE|DELAY_CALL|TAINTED` 未含 `PRIV_PROC`）。
- `DELAY_CALL` 的继承在 C 为 `106` 的掩码产物，但 Rust 侧刻意不继承（`mproc/fork.rs:307` 注释：mid-send 进程不可执行 `fork`，`DELAY_CALL` 继承为 whole-copy 副产物）。

### 2.5 PID 分配：`get_free_pid` 双字段扫描（utility.c:34-74）

```c
pid_t get_free_pid(void) {                // utility.c:34
    static pid_t next_pid = INIT_PID + 1; // utility.c:36 1+1=2 起
    register struct mproc *rmp;
    int t;
    do {
        t = 0;
        next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1); // utility.c:43 1..30000 轮转
        for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
            if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) { // utility.c:45 双字段冲突
                t = 1; break;
            }
    } while (t);                          // utility.c:49
    return(next_pid);                     // utility.c:50
}
```

`NR_PIDS=30000`（`const.h:3`），`NO_PID=0`（`const.h:8`），`INIT_PID=1`（`const.h:9`）；`next_pid` 轮转时跳过 `0/1`，双字段扫描保证 `pid` 与 `procgrp`（进程组亦占用 PID 命名空间）均不碰撞；复杂度期望 `O(1)`（冲突率约 `NR_PROCS/NR_PIDS ≈ 0.8%`），最坏 `O(NR_PROCS)`。

`forkexit.c:119-120` 的 `new_pid = get_free_pid(); rmc->mp_pid = new_pid` 将全局唯一命名注入子进程（Rust `PidGenerator::get_free_pid` 同算法，`mproc/pid_gen.rs:32`，`Cell<Pid>` 单线程安全）。

### 2.6 VFS 投递：`VFS_PM_FORK` 的 `tell_vfs(rmc)`（forkexit.c:122-130）

```c
memset(&m, 0, sizeof(m));                 // forkexit.c:122
m.m_type = VFS_PM_FORK;                   // forkexit.c:123 0x907（com.h:527, RQ_BASE 0x900+7）
m.VFS_PM_ENDPT = rmc->mp_endpoint;        // forkexit.c:124 m7i1 子 endpoint（寻址键，05 §2.1）
m.VFS_PM_PENDPT = rmp->mp_endpoint;       // forkexit.c:125 m7i2 父 endpoint
m.VFS_PM_CPID = rmc->mp_pid;              // forkexit.c:126 m7i3 子 PID
m.VFS_PM_REUID = -1;                      // forkexit.c:127 m7i4 -1 哨兵
m.VFS_PM_REGID = -1;                      // forkexit.c:128 m7i5 -1 哨兵（com.h:301-302 "Not used by VFS_PM_FORK"）
tell_vfs(rmc, &m);                        // forkexit.c:130 ① not-idle ② asynsend3(VFS, AMF_NOREPLY) ③ VFS_CALL 置于子进程（utility.c:123-139，05 §2.2 三段式）
```

`REUID/REGID = -1` 为显式哨兵，`VFS_PM_SRV_FORK`（`0x908`）才填真实 `reuid/regid`（08 差异）；`tell_vfs(rmc)` 的 `rmp` 参数即子进程 `rmc`，`VFS_CALL` 置于子槽（`utility.c:138` `rmp->mp_flags |= VFS_CALL`），延续由 05 的 `handle_vfs_reply` 的 FORK 分支消费（`sched_start_user` 成败双分支）。

### 2.7 tracer 信号：`sig_proc(SIGSTOP)`（forkexit.c:133-134）

```c
if (rmc->mp_tracer != NO_TRACER)          // forkexit.c:133 mproc.h:34 NO_TRACER 0
    sig_proc(rmc, SIGSTOP, TRUE /*trace*/, FALSE /* ksig */); // signal.c:384
```

`NO_TRACER` 0 与 `NO_PID` 同值但语义正交（`tracer` 为槽位索引，`pid` 为命名）；`SIGSTOP` 19（`signal.h:63`）以 `trace=true` 投递，11 章详述，本章只到调用点（Rust `unimplemented!("DEFERRED: sig_proc SIGSTOP — 见 11-signal-core.md")`）。

### 2.8 同步/异步边界与顺序敏感（forkexit.c:82/139）

```c
/* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */ // 82
...
return SUSPEND;                            // 139 com.h:1151 -998，main.c:106 的 ReplyLater
```

`vm_fork` 前可 `EAGAIN`，后不可；`procs_in_use++` 在 `vm_fork` 成功后（C `86` 在 `vm_fork` 后），Rust `alloc_slot` 的 `++` 在 `vm_fork` 前（需在 `vm_fork` 失败时 `release_slot` 回滚，见 §3.3）；`get_free_pid` 在 `*rmc=*rmp` 之后（C `119` 在 `116` 断言后），Rust 保持同序；`tell_vfs` 在 `get_free_pid` 之后（C `130` 在 `120` 后）；`sig_proc` 在 `tell_vfs` 之后（C `133` 在 `130` 后），`return SUSPEND` 终局（C `139`，05 的 `handle_vfs_reply` 在 `VFS_PM_FORK_REPLY` 到达时双回复）。

### 2.9 对端视角（vm_fork / VFS_PM_FORK）

- **VM 侧**：`VM_FORK 0xC01`（`com.h:602`），`vm_fork(rmp->endpoint, child_slot, &child_ep)` → `sys_fork`（`kernel/system/do_fork.c:69-72` `gen = proc[slot].p_endpoint + 0x8000` 等代数递增），对端实现 `02-stage-vm/18-vm-fork.md`，本章只写 PM 侧调用点。
- **VFS 侧**：`VFS_PM_FORK 0x907`（`com.h:527`），`tell_vfs` 后 VFS 异步复制 `fproc` 的 fd 表，回复 `VFS_PM_FORK_REPLY 0x987`（`com.h:540`），PM 侧 `handle_vfs_reply` 的 FORK 分支（05 §2.4/§4.3）完成 `sched_start_user` 与 `reply`，本章只写投递。

### 2.10 不变式分类

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `panic("can't find child slot")` | `forkexit.c:72-73` | `n > NR_PROCS`（全表扫描仍 `IN_USE`） | 不可达（容量检查已保证） |
| `panic("finds wrong child slot")` | `forkexit.c:74-75` | `next_child >= NR_PROCS \|\| IN_USE`（找到的槽仍占用） | 不可达（`IN_USE` 扫描已保证） |
| `panic("asynsend failed")` | `event.c:104-106` 间接（`tell_vfs` 的 `asynsend3` 失败） | 内核异步表溢出或 VFS 死亡 | 不可恢复（`ASYN_NR` 上界被突破） |
| `assert(mp_eventsub == NO_EVENTSUB)` | `forkexit.c:116` | 新子进程仍挂事件游标 | 不可恢复（06 不变量） |
| `EAGAIN` | `forkexit.c:64` | `procs_in_use` 全满或近满非 root | 可恢复（父进程可重试） |
| `SUSPEND` | `forkexit.c:139` | 投递后延续由 VFS 完成 | 异步契约（`ReplyLater`） |

---

## 3 Rust 设计决策

Rust 改写遵循"语义重写（Rewrite）而非翻译（translate）"：保留 C 的外部行为与不变量，但用显式构造与跨服务编排替代整槽复制与裸 `tell_vfs`。以下决策对应设计契约 `.design/07-design.v1.md` 的 D1–D8。

### D1：容量检查收敛到 `ProcTable::can_alloc_for_user`（ARCH A-3）

`ProcTable::can_alloc_for_user(is_root)`（`mproc/table.rs:114`，`NR_PROCS - LAST_FEW` 阈值与 `is_root` 由 `Credentials::is_superuser` 即 `effuid==0` 判定，`Cell` 单线程）消除 `forkexit.c:60-65` 的分散阈值算术；`handle_fork` 不再重复 `EAGAIN` 逻辑，直接 `if !can_alloc { return Err(EAGAIN) }`，与 `PmContext::do_fork_prepare` 单一真相。

### D2：槽位轮转收敛到 `ProcTable::alloc_slot`（ARCH A-2/A-3）

`alloc_slot`（`mproc/table.rs:138`，`next_child` `Cell` 先递增后检查，与 `forkexit.c:69` 同序）在 `vm_fork` 前占位，满表 `None → EAGAIN`（`panic` 不可达路径在 Rust 侧为 `Option`）；`handle_fork` 不再手写 `next_child` 循环，直接 `alloc_slot().ok_or(ProcTableFull)`。

### D3：`vm_fork` 同步调用的 `VmError` 回滚（ARCH A-4）

`VmForkIn { parent, child_slot } → Result<Endpoint, VmError>`（`minix-types/src/ipc/vm.rs`，跨服务契约）与 `mproc/fork.rs:78` 的 `vm_fork` 对齐，但 `alloc_slot` 的 `procs_in_use++` 在 `vm_fork` 前（C `86` 在后），失败时需 `release_slot(child_slot)` 回滚以保 `procs_in_use` 不变量；成功后 `child_ep` 需满足 `slot == child_slot`（`forkexit.c:75` 第二守卫）。

### D4：PID 分配收敛到 `PidGenerator`（ARCH A-11）

`PidGenerator::get_free_pid(&ProcTable)`（`mproc/pid_gen.rs:32`，`NR_PIDS=30000` 上界回绕 + `pid`/`procgrp` 双字段冲突，`Cell<Pid>` 单线程）与 `utility.c:34-74` 逐行对齐；调用点保持 `*rmc=*rmp` 与子资源清零之后、`tell_vfs` 之前（`forkexit.c:119` 同序）。

### D5：`Process::fork_from` 显式构造（ARCH A-1/A-2）

`core::array::from_fn` 的反面——`mproc/fork.rs:249` 的 `Process::fork_from(parent, child_idx, child_pid, child_ep, parent_idx)` 将 `*rmc=*rmp` 的 15 字段整拷贝改为 9 步显式构造（身份/亲缘/`Privilege` 接管/`RemainingFlags` 仅 `TAINTED`/`BlockState::default`/`SignalState` 克隆/`intervals` 清零/`started=getticks()`/`Ipc::default`），编译期穷尽新字段（`make impossible to forget a field`）；`DELAY_CALL` 不继承（`mproc/fork.rs:307` 论证 mid-send 进程不可 `fork`）且 `mp_eventsub` 断言由 `BlockState::default` 保证（06 不变量）。

### D6：`tell_vfs` 的 `VFS_CALL` 置于子进程（ARCH A-4/A-6）

`VfsCall::Fork { child, parent, child_pid }`（`minix-types/src/ipc/vfs.rs:347` 的 `m7i1/m7i2/m7i3/-1/-1` 哨兵与 `com.h:547-583` 对齐）经 `tell_vfs(child_slot, Fork, transport)` 投递（`ipc/vfs.rs:182` 三段式：not-idle→`send(VFS)`→`VFS_CALL{reply_to_new_parent:false}`），`VFS_CALL` 置于子槽（`forkexit.c:130` 的 `rmc`），延续由 05 的 `handle_vfs_reply` 异步双回复（`reply(child,OK)`+`reply(parent,child_pid)` 且 `NEW_PARENT` 抑制）。

### D7：tracer `SIGSTOP` 的 DEFERRED 标注

`if child_tracer.is_some() { sig_proc(child, SIGSTOP, traced=true) }`（`forkexit.c:133-134` 的 `signal.c:384`）在 `signal.rs` 落地前为 `unimplemented!("DEFERRED: sig_proc SIGSTOP — 见 11-signal-core.md")`，与 05 的 `VmFork` 占位同惯例；`NoTracer` 为主路径，`TO_TRACEFORK` 清零分支由 `TraceState::default` 覆盖（`mproc/fork.rs:91-95` 的 `if (trace_flags & TO_TRACEFORK) { .. } else { tracer=NO_TRACER }` 当前简化为恒 `default`，属 P2 与 C 差异）。

### D8：`SUSPEND` → `ReplyLater` 与 05 的 FORK 双分支闭环（ARCH A-6）

`PmCall::Fork → ReplyLater`（`ipc/calls.rs:211`）使 `main.c:106` 的 `result != SUSPEND → reply` 不回复本次 `PM_FORK`；`handle_vfs_reply` 的 FORK 分支（`ipc/vfs.rs:269`）`sched_start_user` 成功→`reply(child,OK)`+`reply(parent,child_pid)`，失败→`exit_proc(child,-1)`+`reply(parent,-1)`，`new_parent` 保护与 `forkexit.c:392-393` 同逻辑（本章发起侧 `SUSPEND`，消费侧归 05，跨文档链路在 §6 过渡闭环）。

---

## 4 实现详解

### 4.1 跨服务编排层（`os/servers/pm/src/fork.rs`）

`handle_fork(table, parent_ep, transport) -> Result<Pid, ForkCoordError>`（`fork.rs:22`，`PmError → EAGAIN/ENOMEM/ENOSYS` 映射见 `fork.rs:197`）8 步：

1. `find_parent_slot`（`fork.rs:73` 扫描 `endpoint==parent_ep && IN_USE`，`table.c:23` 的 `call_vec` 前置 `pm_isokendpt` 已保证父进程 `IN_USE`，此处二次校验为防御）；
2. `table.alloc_slot().ok_or(ProcTableFull)?`（D2）；
3. `child_pid = table.pid_generator.get_free_pid(table)`（D4，调用点在 `vm_fork` 之后以对齐 C `119`）；
4. `child_ep = Endpoint::from_generation_slot(1, child_slot)`（`kernel/system/do_fork.c:69-72` 的代数递增在 Rust 侧简化为 `gen=1`，内核真实代数由 VM 侧回写，本章为协调器占位）；
5. `send_vm_fork(VmForkIn{parent_ep, child_slot})?`（D3，失败时 `release_slot` 回滚）；
6. `copy_mproc(table, parent_slot, child_slot, child_pid, child_ep)`（D5，`mproc/fork.rs:101` 的 `Process::fork_from`）；
7. `tell_vfs(table, child_slot, VfsCall::Fork{child,parent,child_pid}, transport)?`（D6，`VFS_CALL` 置于子槽）；
8. `send_kernel_request(KernelRequest::Fork{...})?`（`01-stage-kernel`，`sys_fork` 的 `proc` 复制）；
9. `if child_tracer.is_some() { sig_proc(...) }`（D7，DEFERRED）→ `Ok(child_pid)`（调用方 `dispatcher` 映射 `ReplyLater`）。

> **与 `mproc/fork.rs` 的职责正交**：`mproc/fork.rs` 的 `PmContext::do_fork_prepare` / `Process::fork_from` 只负责表层预检与显式构造（不触 `transport`），`fork.rs` 的 `handle_fork` 负责跨服务编排（触 `transport`），两者通过 `ProcTable` 共享状态（`cell.rs:23` 的 `Cell` 在单线程下安全）。

### 4.2 进程复制层（`os/servers/pm/src/mproc/fork.rs`）

`Process::fork_from`（`mproc/fork.rs:249`，9 步）：`Identity`（`pid/endpoint/procgrp/name`）→ `State`（`Running`/`BlockState::default`/`WaitState::default`/`Normal{parent}`/`TraceState::default`）→ `Privilege` 接管（`Kernel → User(root)/SCHED`，`User → inherit`）→ `Resources`（`child_utime/stime=0`/`started=getticks()`/`intervals=0`/`scheduler`/`RemainingFlags::TAINTED`）→ `SignalState` 克隆（`actions`/`mask`）→ `Ipc::default`（`reply=None`/`event_subscriber=None`，06 不变量）。`TO_TRACEFORK` 分支当前恒清零（P2 差异，见 §5）。

### 4.3 容量与 PID 层（`os/servers/pm/src/mproc/{table, pid_gen}.rs`）

`ProcTable::can_alloc_for_user`（`table.rs:114`，`LAST_FEW=2`）、`find_free_slot`（`table.rs:138` 先递增后检查）、`PidGenerator::get_free_pid`（`pid_gen.rs:32`，`next_pid` 轮转 + 双字段冲突，`NR_PIDS` 上界回绕）——三者与 `utility.c:34-74` 逐行对齐，`get_free_pid` 的 `Cell` 在单线程下安全，禁止跨线程 `Atomic`。

### 4.4 协议层（`os/libs/minix-types/src/ipc/vfs.rs` + `vm.rs`）

- `VfsCall::Fork { child, parent, child_pid }`（`vfs.rs:377`）的 `encode`（`m7i1=child`/`m7i2=parent`/`m7i3=child_pid`/`m7i4=-1`/`m7i5=-1`）与 `com.h:577-580` 对齐，未用槽位 `-1` 显式哨兵；
- `VmForkIn { parent, slot }`（`vm.rs`）与 `VM_FORK 0xC01` 对齐，对端实现 `02-stage-vm/18-vm-fork.md`；
- `tell_vfs` 的 `VFS_CALL` 置位与 `handle_vfs_reply` 的 `take_vfs_call` 清位成对（05 §2.2/2.3）。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 |
|---|--------|--------|-----------|
| 1 | 满表/近满非 root → `EAGAIN` | `forkexit.c:60-65` | `can_alloc_for_user → Err(EAGAIN)` |
| 2 | 轮转先递增后检查 | `forkexit.c:69` | `find_free_slot` 的 `(next_child+1)%NR_PROCS` |
| 3 | `vm_fork` 前可 `EAGAIN`，后不可 | `forkexit.c:78/82` | `VmFork` 失败→`EAGAIN`，成功后 `alloc_slot` 的 `++` 已不可回滚（需 `release_slot` 补偿） |
| 4 | `procs_in_use++` 在 `*rmc=*rmp` 前 | `forkexit.c:86` | `alloc_slot` 的 `++` 在 `fork_from` 前（回滚补偿） |
| 5 | `mpsigact` 外置 | `forkexit.c:88-89` | `SignalState::actions` 按槽索引 |
| 6 | `PRIV_PROC` 不继承，仅 `TAINTED` | `forkexit.c:100-106` | `RemainingFlags::TAINTED` 过滤 + `Kernel→User(SCHED)` |
| 7 | 子资源清零 | `forkexit.c:107-114` | `child_utime=0`/`interval=0`/`started=getticks()` |
| 8 | `mp_eventsub == NO_EVENTSUB` | `forkexit.c:116` | `BlockState::default` + `Ipc::default`（`None`） |
| 9 | `VFS_CALL` 置于子进程 | `forkexit.c:130` `tell_vfs(rmc)` | `tell_vfs(child_slot, Fork)` |
| 10 | `return SUSPEND` | `forkexit.c:139` | `ReplyLater`（`PmCall::Fork`） |

---

## 5 测试矩阵

### 5.1 `mproc/fork.rs`（表层预检与显式构造，`PmContext::do_fork_prepare` / `Process::fork_from`）

- `test_fork_prepare`：空表 `do_fork_prepare → Ok`（`child_index < NR_PROCS && child_pid>0`）
- `test_fork_table_full`：全表 `do_fork_prepare → Err(TableFull)`（`EAGAIN`）
- `test_fork_reserved_for_root`：近满 `NR_PROCS-2` 且非 root → `Err(ReservedForRoot)`（`EAGAIN`）
- `test_fork_child_from_parent`：`fork_child_from_parent` 后 `child.is_in_use && child.pid==pid && parent==slot`
- `test_fork_error_to_errno`：`TableFull→EAGAIN`/`ReservedForRoot→EAGAIN`/`ResourceExhausted→ENOMEM` 映射
- `test_fork_child_index_correct`：`fork_from` 的 `index/pid/endpoint` 正确
- `test_fork_inherited_fields`：`procgrp/nice` 继承
- `test_fork_cleared_fields`：`child_utime/stime/intervals` 清零
- `test_fork_flags_inheritance`：仅 `TAINTED` 继承
- `test_fork_flags_no_tainted`：`ALARM_ON|PARTIAL_EXEC` → 空
- `test_fork_no_delay_call`：`DELAY_CALL` 不继承
- `test_fork_privilege_scheduler`：`Kernel→User(root)/SCHED`
- `test_fork_normal_scheduler`：`User→inherit`
- `test_fork_parent_relationship`：`parent==slot`
- `test_fork_ipc_reset`：`reply/event_subscriber` 清零（`BlockState::default`）

共 **15** 项（`mproc/fork.rs:322`）。

### 5.2 `fork.rs`（跨服务编排，`handle_fork`）

- `test_find_parent_slot_success`：`find_parent_slot(EP 1,0) → Ok(0)`
- `test_find_parent_slot_not_found`：`find_parent_slot(EP 1,0) → Err(InvalidEndpoint)`
- `test_handle_fork_success`：`handle_fork(EP 1,0) → Ok(child_pid>0)`（含 `VFS_CALL` 置于子槽断言，`05` 的 `tell_vfs` 三段式）
- `test_handle_fork_parent_not_found`：`handle_fork(EP 1,0) → Err(InvalidEndpoint)`

共 **4** 项（`fork.rs:213`）。**与 `mproc/fork.rs` 的 15 项正交**：`fork.rs` 测跨服务编排（`transport` 参与），`mproc/fork.rs` 测表层与显式构造（无 `transport`）。

### 5.3 集成与跨文档

- `ipc/calls.rs:243` `test_dispatch_fork_is_reply_later`：`PmCall::Fork → ReplyLater`（`forkexit.c:139` `SUSPEND`）
- `init.rs:803` `test_run_once_fork_no_sync_reply`：`run_once(PM_FORK) → Handled` 且 `transport.sent().is_empty()`（`main.c:106` `SUSPEND` 不回复本消息）
- `ipc/vfs.rs:763` `test_fork_success` 系列（`handle_vfs_reply` 的 FORK 双分支，05 §5.2）：`sched_start_user` 成败 → `exit_proc` 或 `reply(parent/child)` 且 `NEW_PARENT` 抑制，`restart_sigs` 尾部

完整测试清单：`rg "^\s*fn test_" os/servers/pm/src/{fork,mproc/fork}.rs`（19） + `rg "fork" os/servers/pm/src/{init,ipc/{calls,vfs}}.rs`（3）— 本章直接相关 **22** 项；`cargo test -p minix-pm --lib` 截至 2026-09-02 为 **160 passed**（含 06 的 30），本章新增 `0`（`fork.rs` 已有 4）+ `mproc/fork.rs` 15 已在基线内，`cargo test -p minix-types` **108 passed** 不变。

---

## 6 过渡

`do_fork` 是主循环 `PM_FORK` 的 handler 与 `handle_vfs_reply` 的 `VFS_PM_FORK_REPLY` 发起方的合一：前者在 `PM_FORK` 到达时完成容量→槽位→VM→复制→PID→VFS 的 9 步并 `SUSPEND`，后者在 `VFS_PM_FORK_REPLY` 到达时完成 `sched_start_user` 与双回复（`reply(child,OK)`/`reply(parent,child_pid)`，`NEW_PARENT` 保护，05 §4.3）。

**下一入口**：

- **08-pm-srv-fork.md**——`do_srv_fork` 的 `PRIV_PROC` 继承差异（`IN_USE|PRIV_PROC|DELAY_CALL`）与 `VFS_PM_SRV_FORK` 的 `reuid/regid` 真实填充；
- **09-pm-exit.md**——`exit_proc` / `exit_restart` / `zombify` / `check_parent` / `disinherit`（`NEW_PARENT` 真实设置点 `forkexit.c:402-403`）与 `fork` 的 `procs_in_use` 计数形成生命周期闭环；
- **16-scheduling.md**——`sched_start_user` 的 SCHED 服务内部（`fork` 的双分支消费方）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/forkexit.c:32-140`（`do_fork`）、`minix3/minix/servers/pm/utility.c:34-74`（`get_free_pid`）、`minix3/minix/servers/pm/mproc.h:22/27/86-104`（`mpsigact`/`mp_eventsub`/`mp_flags`）、`minix3/minix/servers/pm/glo.h:9/51`（`procs_in_use`/`next_child`）、`minix3/minix/include/minix/com.h:527/540/547-583`（`VFS_PM_FORK` 字段）、`minix3/minix/include/minix/vm.h`（`vm_fork` 原型）、`minix3/minix/include/minix/callnr.h:14`（`PM_FORK 2`）、`minix3/minix/servers/pm/main.c:88-89/106`（`PROC_EVENT_REPLY`/`SUSPEND`）、`minix3/minix/servers/vm/fork.c`（VM 对端）、`minix3/minix/servers/vfs/main.c:395-410`（VFS 对端）
- 设计契约：`.design/07-design.v1.md`（D1–D8 与行为契约表）、`.design/07-outline.v1.md`、`.design/07-outline-review.v1.md`
- PM 阶段文档：03-mproc-table.md（`can_alloc`/`find_free_slot`/`get_free_pid`）、04-ipc-dispatch.md（`ReplyLater` 契约与 `PmCall::Fork` 分发）、05-vfs-interaction.md（`tell_vfs` 三段式与 `handle_vfs_reply` FORK 双分支）、02-mproc-struct.md（`RemainingFlags::TAINTED` / `mpsigact`）、06-event-subscription.md（`NO_EVENTSUB`）、16-scheduling.md（`sched_start_user`）、11-signal-core.md（`sig_proc` SIGSTOP）、08-pm-srv-fork.md（`PRIV_PROC` 差异）
- 对端实现：`02-stage-vm/18-vm-fork.md`（`vm_fork` 对端）、`05-stage-vfs`（`VFS_PM_FORK` 对端）
- 内核接口：`01-stage-kernel/06-proc-init-boot-proc.md`（`boot_image` 启动）、`01-stage-kernel/19-syscall-signal.md`（`sig_proc` 内核路径）、`01-stage-kernel`（`sys_fork` 代数递增）
- Rust 实现：`os/servers/pm/src/fork.rs`（`handle_fork` 协调器）、`os/servers/pm/src/mproc/fork.rs`（`PmContext::do_fork_prepare` / `Process::fork_from`）、`os/libs/minix-types/src/ipc/vfs.rs`（`VfsCall::Fork`）、`os/libs/minix-types/src/ipc/vm.rs`（`VmForkIn`）
