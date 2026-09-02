# 08 — worker 池：`NR_WTHREADS=9` 真实线程到请求槽状态机的 `pending/busy/block_all` 与 `w_fp` 双向绑定

本文讲清 VFS 如何在 `NR_WTHREADS 9` 固定线程、`pending` 全局排队计数、`busy` 已绑定计数、`block_all` 初始化门控、`FP_PENDING/FP_PM_WORK` 的待处理标记、`FP_BLOCKED_ON_*` 的七态阻塞、以及 `w_fp ↔ fp_worker` 双向绑定的约束下，以 `worker_init` 的 `mthread_create×9 + cond/mutex` 初始化与 `worker_start` 的 `pending/active/normal/PM` 四象限判定及 `worker_allow` 的门控开关建立 `fproc → worker` 的可抢占分发，并以 `suspend/resume/wait/signal/stop` 的 `self/fp/err_code` 三件套保存与 `thread_cleanup` 的 `VMNT_CALLBACK` 回收为阻塞恢复提供可逆上下文。

前置阅读：`02-fproc-struct.md`（`FProc.flags: PENDING/PM_WORK` 与 `BlockedOn` 七态）、`03-fproc-table.md`（`FProcTable::is_ok_endpoint` 三守卫与 `PID_FREE` 双哨兵）、`07-tll-lock.md`（`tll_lock` 的 `EBUSY → tll_append → worker_wait` 排队）。

> 本章不讲什么：
> - `fproc` 各字段含义与 `fp_blocked_on` 七态的 `BlockedOn` 载荷—— `02-fproc-struct.md`
> - `fproc[NR_PROCS]` 的表验证与槽位复用—— `03-fproc-table.md`
> - `tll` 三级锁的 `READ/READSER/WRITE` 与 `write/serial` 双队列偏序—— `07-tll-lock.md`
> - 主循环五路分发与 `call_vec[64]` 的调用路由—— `09-main-loop.md`
> - `service_pm` 的 12 个 `VFS_PM_*` 请求内容—— `10-pm-protocol.md`
> - `pipe`/`select`/`cdev`/`sdev` 的 `SUSPEND→revive` 三恢复路径—— `17-pipe.md/23-select.md/21-cdev.md/22-sdev.md`
> - 内核 `sys_datacopy` 与 `mthread` 的 KMP 实现—— `99-global-concepts.md` + `../01-stage-kernel/16-smp.md`

---

## 1 概念

### 1.1 为什么只有 VFS 需要并发

Minix3 的 9 个系统服务中仅 VFS 使用 `mthread`——`worker.c:5` 的 `worker_main` 线程入口与 `threads.h:5` 的 `thread_t/mutex_t/cond_t` 宏映射是 VFS 独有的。原因不在“文件系统更复杂”，而在“语义必须阻塞”：

- `pipe.c:pipe_suspend` 的管道读在空管时必须等待写端到达；
- `select.c:select_callback` 的 `select` 必须等待任一描述符就绪或超时；
- `cdev.c:cdev_io` 与 `sdev.c:sdev_sendrec` 的驱动 `sendrec` 必须等待驱动回复；
- `comm.c:fs_sendrec` 的 FS `sendrec` 必须等待文件服务器回复且 `m_comm.c_cur_reqs` 限制并发；
- `lock.c:lock_op` 的 `F_SETLKW` 必须等待锁可用。

若 VFS 为单线程事件循环（与 PM/VM 同型），任一 `SUSPEND` 将阻塞整个文件服务——`init(1)` 的 `open("/etc/rc")` 因 `MFS` 拥塞而使 `sh(1)` 的 `read(STDIN)` 无法响应。真实线程将“阻塞的副作用”隔离到单个 `w_fp` 绑定：`worker_suspend` 保存 `self/fp/err_code` 三件套，`worker_sleep` 在 `w_event` 条件变量上等待，主线程 `get_work` 仍可在 `reviving` 非零时优先执行 `unblock` 复活路径（`main.c:590-596` 的 `reviving != 0 → for fp: REVIVED → unblock`）。

此“隔离优于串行”的权衡在 plan §4 `ARCH A-1` 定性：minix-rs 将 9 真实线程消除为**请求槽状态机**（`WorkerState::Idle/Busy/WaitingForFs/Suspended`），阻塞在 Rust 以 `ReplyIntent::ReplyLater` 的显式状态而非 `cond_wait` 的隐式调度显式——语义保留，线程消失。

### 1.2 两级并发：进程级 pending 与线程级 busy

`worker.c:10-11` 的 `pending: unsigned int`（标 `FP_PENDING` 的进程数）与 `busy: unsigned int`（`w_fp != NULL` 的线程数）在 `worker_may_do_pending: 147-156` 的三条件中正交：

```c
return (pending > 0 && worker_available() > 1 && !block_all);
```

- `pending > 0` ——有排队工作可做；
- `worker_available() > 1` ——`NR_WTHREADS - busy > 1`，**至少保留 1 个 spare 线程**不用于 `pending`（`147-151` 注释 *the spare thread is never used for pending work*，`331-339` 的 `needed = use_spare ? 1 : 2` 同理）；
- `!block_all` ——`worker_allow(FALSE)` 门控关闭时禁止消费排队（`162-183` 的初始化门控）。

此“至少留一”的不变量是死锁避免的静态策略：同步回调（`handle_work:179` 的 `use_spare=TRUE`）与 `FS` 自身回调的 `VMNT_CALLBACK` 检查（`main.c:160` `if (m_flags & CALLBACK) → EAGAIN`）可能在 `busy == 8` 时仍需一个线程，二者合证 `spare` 的必需性。`pending` 计数的错误将使 `worker_allow(TRUE)` 的 `for (rfp: PENDING) assign` 漏唤醒，`busy` 计数的错误将使 `worker_available() == 0` 时 `EAGAIN` 误判。

### 1.3 初始化门控：`block_all` 与 `worker_allow` 的两阶段

`main.c:501-523` 的 `do_init_root` 在挂载根文件系统期间执行：

```c
worker_allow(FALSE);  // 507: 关门——新请求全部 pending
mount_pfs();          // 510: 管道 FS
mount_fs(..., MFS);   // 516: 根 FS
worker_allow(TRUE);   // 522: 开门——drain pending
```

`worker_allow:162-185` 的开关语义在关闭时仅 `block_all = !allow`（`170` 单赋值），开启时则 `if (!may_do_pending) return` 的短路 + `for (rfp: FP_PENDING) { clear PENDING; pending--; assign; if (!may_do_pending) return; }` 的排队释放。`block_all` 置位期间 `worker_try_activate:349` 的 `!block_all || use_spare` 使普通请求 `→ pending` 而回调请求 `→ spare` 直通。

门控的“关闭仅标记、开启才释放”与 `sef_cb_init_fresh:415-436` 的 `VFS_PM_INIT` 握手后 `worker_init` 同型：`worker_init:27-56` 仅创建线程与 `pending=0/busy=0/block_all=FALSE` 零化，`worker_yield:435` 的 `mthread_yield_all + self=NULL` 使主线程交出 `self` 全局。

### 1.4 双向绑定：`w_fp ↔ fp_worker` 的可抢占关联

`threads.h:27` 的 `w_fp: fproc*` 与 `fproc.h:86` 的 `fp_worker: worker_thread*` 构成**双向指针绑定**——`worker_assign:138` 的 `rfp->fp_worker = worker; worker->w_fp = rfp; busy++` 与 `worker_main:283-286` 的 `fp->fp_worker = NULL; self->w_fp = NULL; busy--` 在 `busy` 计数两侧原子化。绑定失败的后果在 `worker_suspend:482` 的 `assert(self->w_fp == fp && fp->fp_worker == self)` 与 `worker_set_proc:598-606` 的 *incredibly ugly* 注释中显式：`reboot` 路径的 `fp == rfp ? return : panic(target not idle) → fp->fp_worker=NULL → fp=rfp → self->w_fp=rfp` 违反线程模型，仅 `reboot` 允许。

双向绑定的收益是“进程可用性”查询的 O(1)：`worker_can_start:295-325` 的 `is_pending/is_active/has_normal_work → !pending && !active → 可新起； has_normal_work → 不可多加； is_pending → 可加正常工作（待执行前）； else → 不可（PM 活跃）` 在 `main.c:104` 的 `ds_event` 分发中守门——`DS` 永不对 VFS 发 `VFS` 调用且无 `PM` 延期，故 `worker_can_start` 对 `DS` 安全（`main.c:102` 注释）。

### 1.5 挂起的协程语义：`suspend/resume` 的三件套与 `wait/signal` 的睡眠

`worker_suspend:474-487` 的三件套（`self/fp/err_code`）保存与 `worker_resume:493-504` 的 `self = org_self; fp = self->w_fp; err_code = w_err_code` 恢复构成**协程上下文**：`lock_proc:528-541` 的 `mutex_trylock → 成功直返；失败 → suspend → mutex_lock → resume` 将 `fp_lock` 的互斥等待建模为协程挂起，而非条件变量的显式 `wait`。`worker_wait:510-520` 的 `suspend; sleep; resume; assert(w_next==NULL)` 则将 `tll_append:48-71` 的 `worker_wait` 阻塞显式为 `sleep + signal` 的条件变量等待——`tll.c:271` 的 `worker_wait` 在 `tll_lock` 层等待 `READSER→WRITE` 升级或 `WRITE` 独占，`pipe.c:suspend` 在 `pipe` 层等待读端/写端到达，二者合流于同一 `w_event` 队列（`tll.c:278` 的 `t_write/serial` 双队列头唤醒即信号此队列）。

`worker_sleep:443-453` 的 `mutex_lock(w_event_mutex) → cond_wait → mutex_unlock → self=worker` 与 `worker_wake:459-468` 的 `mutex_lock → cond_signal → mutex_unlock` 则将 `sleep` 的“交出 `self` 全局”与 `wake` 的“置位计数”分化——`worker_yield:435` 的 `self=NULL` 同为交出。

### 1.6 与其他 OS 的并发对照

- **Linux** 以 `workqueue`（`system_wq` 的 `worker_pool { nr_workers, nr_idle, worklist }`）+ `wait_queue_head_t` 的完成队列组织延迟工作，`VFS` 的 `pending` 排队在 Linux 以 `work_struct.pending` 的 `list_add_tail(worklist)` 近似；`block_all` 门控在 Linux 以 `freeze_workqueues_begin → flush_workqueue` 的冻结/释放近似；`VFS` 的 `NR_WTHREADS 9` 固定上界在 Linux 以 `WQ_UNBOUND` 的 `max_active` 动态上界替代。`Linux` 的 `spare` 在 `workqueue` 以 `rescuer_thread` 的救援线程显式。
- **Redox** 的 `Scheme` 以 `async/await` 的 `Future` + `executor::spawn` 的任务队列组织阻塞，`VFS` 的 `w_fp` 绑定在 Redox 以 `SchemeId → Future` 的 `Arc<Mutex<FileDescription>>` 的 `open → scheme` 异步调度替代线程绑定；`worker_suspend` 的 `self/fp/err_code` 三件套在 Redox 以 `Future::poll(cx: Waker)` 的上下文保存替代；`worker_allow` 的门控在 Redox 以 `executor.block_on(root_mount)` 的 `async` 异步门控替代。`Redox` 无 `NR_WTHREADS` 固定上界，`executor` 的 `num_threads` 动态扩容。
- **seL4** 无 `worker`，`VFS` 的 `mthread` 多线程在 seL4 以 `seL4_Notify` 的被动服务器（`seL4_ReplyRecv` 循环）的单线程事件循环 + `seL4_Word badge` 的端点多路复用显式；`VFS` 的 `pending` 排队在 seL4 以 `endpoint queue` 的内核队列显式；`VFS` 的 `spare` 在 seL4 以 `vka_cspace_alloc` 的空闲 `cslot` 显式。

共同约束是“阻塞不得传染”。Minix3 的选择是以 9 固定真实线程 + `pending/busy/block_all` 全局三计数 + `w_fp ↔ fp_worker` 双向绑定的**线程隔离**显式阻塞边界，该设计在 minix-rs 以**请求槽状态机**消除真实线程但保留“进程级 `pending` 与线程级 `busy`”的双计数不变量。

### 1.7 小结

worker 池是 `VFS` 的并发边界：9 固定真实线程以 `pending`（排队）、`busy`（已绑）、`block_all`（门控）的三计数决定 `may_do_pending` 的消费时机，以 `w_fp` 的 `busy++` 分配与 `NULL` 归还界定生命周期，以 `is_pending/is_active/has_normal|PM_work` 的四象限决定 `worker_start` 的 `try_activate` vs `pending` 分叉，以 `suspend/resume` 的 `self/fp/err_code` 协程与 `wait/signal` 的 `cond_wait/signal` 睡眠实现可逆阻塞。下一节以 `worker.c:27-607` 全文为主线逐段核对。

---

## 2 C 源码分析

### 2.1 类型与常量（`threads.h:1-38` / `const.h:9` / `glo.h:34/37`）

`threads.h:4` 的 `thread_t/mutex_t/cond_t/attr_t` 映射 `mthread_*` 四宏（`5-8` `mutex_init/destroy/lock/trylock/unlock` + `10-19` `cond_init/destroy/wait/signal`）使 `worker.c` 的 `mutex_init(&wp->w_event_mutex)` 与 `cond_wait(&w_event, &w_event_mutex)` 在编译期等价 `mthread` 调用。`threads.h:23` 的 `struct worker_thread { w_tid, w_event_mutex, w_event, w_fp, w_m_in, w_m_out, w_err_code, w_sendrec, w_drv_sendrec, w_task, w_dmap, w_next }` 的 12 字段中 `w_next` 为 `tll.c:tll_append` 的双队列链指针（`tll.c:35` `w_next` 尾插），`w_dmap` 为 `bdev/cdev` 的设备映射缓存（`bdev.c:cdev.c` 的 `dmap` 域），`w_sendrec` 与 `w_drv_sendrec` 为 `FS` 与驱动两套 `sendrec` 缓存（`worker_stop:539-548` 的两分支 `if (w_drv_sendrec) … else if (w_sendrec) … else panic`）。`glo.h:34` 的 `EXTERN worker_thread *self` 为线程局部（TLS）指针（`worker_main:243` `self = (worker_thread *)arg` 存入，`worker_yield:437` `self=NULL` 交出），`37` 的 `workers[NR_WTHREADS]` 为全局固定槽数组（`const.h:9` `NR_WTHREADS 9` 与 `glo.h:37` `workers[NR_WTHREADS]` 同界）。

### 2.2 全局计数（`worker.c:9-12`）

`worker.c:10` 的 `pending: unsigned int`（`FP_PENDING` 进程数）、`11` 的 `busy: unsigned int`（`w_fp != NULL` 线程数）、`12` 的 `block_all: int`（`worker_allow(FALSE)` 置位）在 `worker_init:38-40` 的 `pending=0; busy=0; block_all=FALSE` 零化与 `worker_yield` 的 `self=NULL` 交出同为 `BSS` 零化契约。`TH_STACKSIZE` 的 `28 KiB（非 magic）/40 KiB（coverage）/64 KiB（minix_magic）` 三档（`15-20`）使 `NR_WTHREADS=9` 的线程栈合计 `252 KiB`，与 `fproc[256]` 的 `1.1 MiB` 在 `BSS` 总量中可预算。

### 2.3 `worker_init` 批量创建（`worker.c:27-58`）

`worker.c:33` 的 `mthread_attr_init(&tattr)` + `35` `setstacksize(TH_STACKSIZE)` 模板、`42-54` 的 `for (i=0..9: w_fp=NULL; w_next=NULL; w_task=NONE; mutex_init; cond_init; mthread_create(worker_main, wp))` 8 步、`57` 的 `worker_yield()` 交出构成创建时序。`33/35` 的 `panic("failed…")` 在 `sef_cb_init_fresh:444-445` 的 `worker_init` 调用中守门——启动失败即 `panic` 重启（`RS` 受控重启语义），与 `sys_hz` 的 `panic` 同型。

### 2.4 `worker_cleanup` 逆向拆除（`worker.c:63-104`）

`worker.c:73` 的 `assert(worker_idle())`（`pending==0 && busy==0`）为活更新前置，`76-82` 的 `for (i: assert(w_fp==NULL); worker_wake)` 使 `worker_main:222` 的 `worker_sleep → self->w_fp == NULL → return FALSE → thread exit` 路径触发，`85` 的 `worker_yield` 使退出线程 `join` 前调度，`88-96` 的 `mthread_join + cond_destroy + mutex_destroy` 逆序拆除（与 `init` 的 `create` 顺序相反），`100` 的 `mthread_attr_destroy` 收尾，`103` 的 `memset(workers, 0)` 清零。`sef_cb_lu_prepare:312-324` 的 `!worker_idle → ENOTREADY` 与 `worker_cleanup` 在活更新的 `REQUEST_FREE/PROTOCOL_FREE` 状态中调用（`main.c:312-324`），与 `312-324` 的 `if (!worker_idle) break→ENOTREADY` 的“非空闲则阻塞更新”守门。

### 2.5 `worker_idle / worker_available` 读视图（`worker.c:109-234`）

`worker.c:113` 的 `pending==0 && busy==0` 空闲与 `228-233` 的 `NR_WTHREADS - busy` 可用数构成读视图——`worker_available() > 1` 的“至少留一”在 `may_do_pending:156` 与 `can_start` 的调用点（`main.c:104` 的 `ds_event` 与 `handle_work:165` 的 `EAGAIN`）守门。

### 2.6 `worker_assign / worker_try_activate / worker_start` 分发核（`worker.c:119-426`）

- `worker_assign:119-142` 的 `for (i: w_fp==NULL → break) → rfp->fp_worker=worker; worker->w_fp=rfp; busy++ → worker_wake` 的 O(9) 扫描与 `busy++` 计数，使 `busy` 的递增与绑定的 `w_fp` 原子化。
- `worker_try_activate:331-355` 的 `needed = use_spare ? 1 : 2`（`342`）与 `needed <= available && (!block_all || use_spare) → assign else PENDING++` 的“`spare` 直通 vs `pending` 排队”分化，使 `block_all` 门控期间 `use_spare=TRUE` 的回调仍可直通（`main.c:177` 的 `handle_work` 的回调路径 `use_spare=TRUE`）。
- `worker_start:360-426` 的 `is_pm_work = (func==NULL)` 双轨 + `is_pending/is_active/has_normal|PM_work` 四象限 + `if (pending||active) { if (both) panic; if (!pm && has_normal → panic two calls; if (pm && has_pm → panic two PM) } else if (has_normal||has_pm → panic admin error)` 的 6 守卫 + `if (!pm) fp_msg=m_ptr, fp_func=func else fp_pm_msg=m_ptr, flags|=PM_WORK` 的两存储 + `if (!pending && !active) try_activate` 的“仅新绑定才调度”构成 `fork` 的 `VFS_PM_FORK`（非 `PM_WORK`）与 `VFS_PM_EXEC/EXIT/DUMPCORE/UNPAUSE`（`PM_WORK`）的双轨分化——后者在 `worker_main:270-276` 的 `if (FP_PM_WORK) { w_m_in=fp_pm_msg; service_pm_postponed; flags&=~PM_WORK }` 路径消费。

### 2.7 `worker_can_start` 可加性检查（`worker.c:295-326`）

`295-326` 的 `is_pending/is_active/has_normal_work → !pending&&!active→TRUE; has_normal→FALSE; is_pending→TRUE（待执行前可加正常工作）; else→FALSE（PM 活跃不可加正常工作）` 在 `main.c:104` 的 `if (worker_can_start(fp)) handle_work(ds_event)` 的 `DS` 事件守门——注释 *DS is not supposed to issue calls or be postponed PM target* 使该检查对 `DS` 充分。

### 2.8 `worker_get_work / worker_main` 工作循环（`worker.c:192-290`）

- `worker_get_work:192-223` 的 `assert(self->w_fp==NULL)` → `if (may_do_pending) for (rfp: PENDING → w_fp=rfp, fp_worker=self, busy++, clear PENDING, pending-- → TRUE)` → `worker_sleep → (w_fp != NULL)` 的“`pending` 优先 vs `sleep` 等待”双路径，使 `pending` 的消费在 `worker_get_work` 层（`worker` 线程视角）与 `worker_allow:176-185` 层（主线程视角）同型。
- `worker_main:239-290` 的 `self=arg → while (get_work) { fp=self->w_fp; assert(fp_worker==self); lock_proc; if (fp_func) { w_m_in=fp_msg; err_code=OK; fp_func(); fp_func=NULL } ; if (FP_PM_WORK) { w_m_in=fp_pm_msg; service_pm_postponed; flags&=~PM_WORK }; thread_cleanup; unlock_proc; fp_worker=NULL; self->w_fp=NULL; busy-- }` 的“正常工作先、PM 延期后、清 `CALLBACK`、解绑”时序，使 `do_work:263-298` 的 `fp_pid==PID_FREE → drop` 与 `thread_cleanup:558-575` 的 `VMNT_CALLBACK` 清除在解绑前原子化。

### 2.9 睡眠与协程：`worker_yield/sleep/wake/suspend/resume/wait/signal/stop`（`worker.c:431-550`）

| 函数 | 行号 | 语义 |
|------|------|------|
| `worker_yield` | 431-438 | `mthread_yield_all; self=NULL` 主线程交出 |
| `worker_sleep` | 443-453 | `lock(w_event_mutex) → cond_wait → unlock → self=worker` 线程休眠 |
| `worker_wake` | 459-468 | `lock → cond_signal → unlock` 线程唤醒 |
| `worker_suspend` | 474-487 | `assert(w_fp==fp && fp_worker==self) → save err_code → return self` 协程保存 |
| `worker_resume` | 493-504 | `assert(self) → self=org_self; fp=w_fp; err_code=w_err_code` 协程恢复 |
| `worker_wait` | 510-520 | `suspend → sleep → resume; assert(w_next==NULL)` tll 等待 |
| `worker_signal` | 526-530 | `assert(worker) → wake` tll 唤醒 |
| `worker_stop` | 535-550 | `if (w_drv_sendrec) m_type=EIO, clear else if (w_sendrec) m_type=EIO, clear else panic → wake` 驱动/FS 两套 `sendrec` 的 `EIO` 注入 |
| `worker_stop_by_endpt` | 555-567 | `if NONE return; for (workers: w_fp && w_task==ep → stop)` 端点级联 |
| `worker_get` | 572-581 | `for (tid==tid → return)` tid 反查 |
| `worker_set_proc` | 586-607 | `fp==rfp→noop; assert(target idle: fp_worker==NULL) → fp->worker=NULL → fp=rfp → self->w_fp=rfp → fp_worker=self` reboot 上下文偷换 |

`worker_suspend` 的 `w_err_code = err_code` 与 `worker_resume` 的 `err_code = w_err_code` 的全局 `err_code`（`glo.h:41`）往返在 `lock_proc:538-541` 的 `suspend → mutex_lock → resume` 路径中显式——`glo.h:41` 的 `EXTERN int err_code` 为调用返回值暂存，其值在挂起期间由 `w_err_code` 托管。

### 2.10 `thread_cleanup` 回收（`main.c:558-575`）

`main.c:562-565` 的 `check_filp/vnode/vmnt_locks_by_me`（`LOCK_DEBUG`）与 `568-574` 的 `if (FP_SRV_PROC) { find_vmnt(fp_endpoint) → flags&=~VMNT_CALLBACK }` 的回调标志回收，使 `handle_work:170` 的 `VMNT_CALLBACK` 置位在 `worker_main:279` 的 `thread_cleanup` 解绑前清除。`LOCK_DEBUG` 计数在 `fproc.h:72-79` 的 `fp_vp/vmnt_rdlocks` 与之互证。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `worker.c:139` 的 `w_fp = rfp` 裸指针赋值与 `cond_wait` 隐式调度，而是吸收 Redox/Linux 的异步执行模型后做取舍。以下决策对应 `.design/08-design.v1.md` D1-D6。

### D1 执行模型：真实线程 → 请求槽状态机

- **C**：`mthread_create(worker_main) ×9` 真实线程（`worker.c:52`）+ `w_event_mutex/cond` 线程内同步（`threads.h:25`）。
- **Rust**：`WorkerPool { slots: [WorkerSlot; 9], pending: usize, busy: usize, allow: bool }` 的 9 固定请求槽；`WorkerSlot { state: WorkerState::Idle/Busy { slot, func }/WaitingForFs { task } /Suspended { saved } }` 的显式状态机；`busy = slots.iter().filter(|s| !Idle).count()` 的派生计数；`NR_WTHREADS 9` 常量保留。
- **为什么**：真实线程的 `TH_STACKSIZE 28 KiB ×9 = 252 KiB` 栈在单线程事件循环（`ARCH A-1`）下为冗余；VFS 作为用户态服务器与 `PM` 同为单线程事件循环时，`ReplyIntent::ReplyLater`（`main_loop.rs:118`）的 `SUSPEND` 建模已在 `17-pipe/23-select` 层显式，`worker` 的 `Busy` 仅为“已绑定但未回复”的 `w_fp` 语义，无需 `mthread` 调度器。
- **备选**：保留 `std::thread::spawn×9` 真实线程；否决——`os` 的 `#![no_std]` 约束与 `WASM` 可移植性要求无 `std::thread`，且 `fproc` 的 `Rc` 引用在 `Send` 约束下需 `Arc+Mutex` 的 `SMP` 开销。

### D2 全局计数聚合：`pending/busy/block_all` → `WorkerPool` 字段

- **C**：`static pending/busy/block_all` 三 BSS 全局（`worker.c:10-12`）在 `glo.h` 无声明，仅 `worker.c` 可见。
- **Rust**：`WorkerPool { pending: usize, busy: usize, allow: bool }` 的聚合字段（`allow = !block_all` 正逻辑反转，`allow==true` 即 `block_all==FALSE`），`ARCH A-4` 的 `glo.h → VfsState` 聚合在 `08` 层为 `WorkerPool` 子聚合；`busy` 的递增在 `WorkerPool::assign` 层与 `w_fp` 绑定原子化，递减在 `WorkerPool::release` 层与 `w_fp=NULL` 原子化。
- **为什么**：三全局的分散使 `worker_allow:170` 的 `block_all = !allow` 需跨 `worker.c` 隐式状态，Rust 将“门控开关、排队数、绑定数”收敛为 `WorkerPool` 不变量：`pending == fproc.iter().filter(|fp| flags.contains(PENDING)).count()` 的派生校验，可 `debug_assert` 互证。

### D3 四象限分发：`worker_start` 的 `pending/active/normal/PM` 显式

- **C**：`worker_start:371-425` 的 `is_pending/is_active/has_normal/has_pm` 四布尔 + 4 `panic` 守卫 + `if (!pm) fp_msg/func else fp_pm_msg/PM_WORK` 两存储 + `if (!pending&&!active) try_activate` 单调度。
- **Rust**：`WorkerPool::start(slot, func: WorkerFunc, msg: Message, use_spare: bool, fproc: &mut FProcTable) -> Result<(), WorkerError>` 的 `WorkerFunc::DoWork/DoPendingPipe/DsEvent/PmReboot/VmProcCtl` 枚举（`WorkerFunc` 以枚举代 `void (*func)(void)` 函数指针，`pm_reboot` 的 `NULL` 判别在 Rust 以 `WorkerFunc::PmReboot` 显式）+ `StartError::AlreadyPendingAndActive/PendingNormalExists/ActiveNormalExists/ActivePmExists` 四错误；成功路径以 `assign_or_mark_pending` 的 `needs_spare(allow, use_spare)` 分化。
- **为什么**：`void (*func)(void)` 的 `NULL` 即 `PM_WORK` 在 C 以指针相等性隐式，Rust 以枚举使 `func==NULL` 的 `PM_WORK` 与 `func!=NULL` 的 `DoWork` 的 match 穷尽；`panic` 的 4 守卫在 Rust 以 `Result::Err` 的 `debug_assert` 可测试。

### D4 可加性守门：`worker_can_start` 的 `FP_PENDING` 优先级

- **C**：`worker_can_start:295-325` 的 `!pending&&!active → 可； has_normal→否； is_pending→可（待执行前可加）； else→否` 的 `PM` 活跃拒绝。
- **Rust**：`WorkerPool::can_start(slot: UserSlot, fproc: &FProc) -> bool` 的同语义 `match (pending, active, has_normal)` 穷尽；`DS` 事件的 `can_start` 守门在 `main_loop.rs:dispatch` 层以 `pool.can_start(slot)` 的显式调用保留。
- **为什么**：`PM_WORK` 的 `fp_flags & PM_WORK` 与 `fp_func != NULL` 的正常工作分属两存储（`fp_msg` vs `fp_pm_msg`），`can_start` 的“`pending` 但无正常工作可加”使 `PM_WORK` 的 `postponed` 队列可与正常工作的 `DoWork` 在 `worker_main:260-276` 的“正常先、PM 后”时序中串行。

### D5 挂起与睡眠：`mutex/cond` → `WorkerState` 显式

- **C**：`worker_sleep:443` 的 `cond_wait(w_event, w_event_mutex)` 与 `worker_suspend:485` 的 `w_err_code=err_code` 三件套保存。
- **Rust**：`WorkerState::Suspended { saved_err: i32, saved_slot: UserSlot }` 的显式状态；`WorkerPool::suspend(slot) -> SuspendToken` 的 `Token { slot, saved_err }` + `pool.resume(token)` 的 `Token` 消耗；`pool.wait(slot)` 的 `suspend → WaitingForFs { task }` 状态迁；`pool.signal(slot)` 的 `Suspended → Busy` 迁；`pool.yield_now()` 的 `allow` 交出为 `noop`（单线程事件循环下 `mthread_yield_all` 无语义）。
- **为什么**：`mthread` 的 `cond` 在 `#![no_std]` 的单线程事件循环下以 `ReplyIntent::ReplyLater` 的状态机已在 `main_loop` 层替代；`err_code` 的 `w_err_code` 托管在 Rust 以 `SuspendToken { err }` 的所有权转移显式，避免 `glo.h:41` 全局 `err_code` 的隐式 TLS。

### D6 生命周期：`stop_by_endpt / set_proc / get` 的显式与 `#[cfg]` 隔离

- **C**：`worker_stop:535-550` 的两套 `w_sendrec/w_drv_sendrec` 的 `EIO` 注入与 `worker_set_proc:586-607` 的 *incredibly ugly* 上下文偷换。
- **Rust**：`WorkerPool::stop_by_endpoint(ep: Endpoint)` 的 `for (slot: w_fp && w_task==ep → inject Eio + wake)` 与 `WorkerPool::steal_context(from, to)` 的 `debug_assert!(target.is_idle()) → transfer` 的显式 `steal`；`worker_get(tid)` 的 `tid` 反查在 Rust 以 `WorkerPool::find_by_slot(slot)` 的 `Slot` 索引替代 `thread_t` 比较（`Invalid thread` 哨兵 `const.h:30` 的 `((thread_t)-1)` 在 Rust 以 `Option<Slot>` 的 `None` 替代）。
- **为什么**：`w_sendrec` 的 `EIO` 注入本质是“等待 FS/驱动回复的请求在进程/FS 退出时快速失败”，Rust 以 `WorkerSlot { waiting: Option<TaskId>, reply: Option<Reply> }` 的 `Option::take` 显式；`set_proc` 的偷换仅 `pm_reboot` 路径（`main.c:901`）使用，Rust 以 `#[cfg(feature = "reboot_steal")]` 的可选隔离标注 `unsafe` 语义。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 执行模型：真实线程 → 请求槽状态机 | `WorkerPool.slots: [WorkerSlot; 9]` 固定槽 | `worker.rs:WorkerPool.slots` + 本文档 D1 + 08 正文 1.1 |
| A-4 全局聚合：`pending/busy/block_all` → `WorkerPool` 聚合 | `WorkerPool { pending, busy, allow }` | `worker.rs:WorkerPool.pending` + 本文档 D2 + 08 正文 2.2 |
| A-6 锁降级：`fp_lock/w_event_mutex` → 状态机 | `WorkerState::Suspended` + `SuspendToken` | `worker.rs:SuspendToken` + 本文档 D5 + 08 正文 2.9 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── worker.rs           — WorkerPool/WorkerSlot/WorkerState/WorkerFunc/WorkerError/SuspendToken/allow/assign/may_do_pending/can_start/start/suspend/resume/wait/signal/stop/steal
├── fproc.rs            — FProc { flags: PENDING/PM_WORK } 互引 pending 计数
├── main_loop.rs        — VfsState { worker_pool, boot_phase, accept_requests } 互引 allow 门控
└── tll.rs              — Tll::try_lock 的 Busy → pool.wait 互引
```

### 4.2 `worker.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `NR_WTHREADS 9` | `const.h:9` | `worker.rs:NR_WTHREADS: usize = 9` | 固定 9 槽 |
| `workers[9]` | `glo.h:37` + `worker.c:27` | `WorkerPool.slots: [WorkerSlot; 9]` | 构造即空闲 |
| `pending/busy/block_all` | `worker.c:10-12` | `WorkerPool { pending, busy, allow }` | 零化 + 单调计数 |
| `worker_init` | `worker.c:27` | `WorkerPool::new()` | `pending=0, busy=0, allow=true, slots=[Idle;9]` |
| `worker_cleanup` | `worker.c:63` | `WorkerPool::cleanup(&mut self) -> Result` | `assert_idle → drain drain → zero` |
| `worker_idle` | `worker.c:109` | `WorkerPool::is_idle() -> bool` | `pending==0 && busy==0` |
| `worker_available` | `worker.c:228` | `WorkerPool::available() -> usize` | `NR_WTHREADS - busy` |
| `worker_may_do_pending` | `worker.c:147` | `WorkerPool::may_do_pending() -> bool` | `pending>0 && available>1 && allow` |
| `worker_allow` | `worker.c:162` | `WorkerPool::set_allow(bool)` | `allow=val; if may → drain pending` |
| `worker_assign` | `worker.c:119` | `WorkerPool::assign(slot) -> Option<usize>` | `找 Idle → Busy{slot} → busy++` |
| `worker_can_start` | `worker.c:295` | `WorkerPool::can_start(fp) -> bool` | `!pending&&!active→T; has_normal→F; pending→T; else→F` |
| `worker_try_activate` | `worker.c:331` | `WorkerPool::try_activate(slot, use_spare) -> Activate` | `needed<=avail && (allow\|\|spare) → assign else PENDING++` |
| `worker_start` | `worker.c:360` | `WorkerPool::start(slot, func, msg, spare, &mut FProc) -> Result` | 四象限 `panic→Err` + 双存储 + `!pending&&!active→try_activate` |
| `worker_yield` | `worker.c:431` | `WorkerPool::yield_now(&mut self)` | `noop`（单线程下 `mthread_yield_all` 无语义） |
| `worker_suspend` | `worker.c:474` | `WorkerPool::suspend(slot, err) -> SuspendToken` | `save err, state=Suspended` |
| `worker_resume` | `worker.c:493` | `WorkerPool::resume(token)` | `restore err, state=Busy` |
| `worker_wait` | `worker.c:510` | `WorkerPool::wait(slot) -> SuspendToken` | `suspend → sleep` 合成 |
| `worker_signal` | `worker.c:526` | `WorkerPool::signal(token)` | `wake Waiting` |
| `worker_stop` | `worker.c:535` | `WorkerPool::stop(slot)` | `EIO 注入 + wake` |
| `worker_stop_by_endpt` | `worker.c:555` | `WorkerPool::stop_by_endpoint(ep)` | `for slot: task==ep → stop` |
| `worker_get` | `worker.c:572` | `WorkerPool::find_by_slot(slot)` | `Slot` 索引替代 `tid` |
| `worker_set_proc` | `worker.c:586` | `WorkerPool::steal_context(from, to)` | `assert(target idle) → transfer w_fp` |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空闲 `pending==0 && busy==0` | `WorkerPool::is_idle` | `pending==0 && busy==0` | `worker.c:113` |
| 可用 `NR - busy` | `available` | `NR_WTHREADS - busy` | `worker.c:233` |
| 至少留一 `available>1` | `may_do_pending` | `available>1` | `worker.c:156` |
| 门控 `!allow → 仅 spare` | `try_activate` | `!block_all \|\| use_spare` | `worker.c:349` |
| 双向绑定 `w_fp ↔ fp_worker` | `assign/release` | `w_fp==slot && fp_worker==slot` | `worker.c:138` |
| 协程保存 `err_code` 往返 | `suspend/resume` | `saved_err ↔ err` | `worker.c:485/504` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **96 passed / 0 failed**（`fproc` 13 + `main_loop` 23 + `worker` 19 + `call_table` 6 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 = 88 → `cargo test` 实测 96 的计口径以 `96 passed` 为准；`minix-types` 108 独立）。
> 本章直接影响 `8 → 19` 项新增（`is_idle/available/may_pending/allow/assign/can_start/start_pm/start_both_pending/suspend_resume/wait_signal/stop/stop_by_ep/steal/selector/cleanup/yield/error/slot`），`minix-vfs --lib` 总计 77 → 96（实测 96，doc 计数以 `cargo test` 输出为准）。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_worker_pool_new_is_idle` | `worker.c:27-40` | `pending 0 busy 0 allow true` 的零化 | `worker.rs` |
| `test_worker_available` | `worker.c:228-233` | `NR 9 - busy` 可用数 | `worker.rs` |
| `test_worker_may_do_pending_spare` | `worker.c:147-156` | `pending>0 && avail>1 && allow` 的 spare 保留 | `worker.rs` |
| `test_worker_allow_drains` | `worker.c:162-185` | `FALSE→PENDING` 排队、`TRUE→assign` 释放 | `worker.rs` |
| `test_worker_assign_busy` | `worker.c:119-142` | `w_fp==NULL→Busy + busy++` 绑定 | `worker.rs` |
| `test_worker_can_start_pending` | `worker.c:295-325` | `!pending&&!active→T; has_normal→F; pending→T` 的三路 | `worker.rs` |
| `test_worker_start_pm_vs_normal` | `worker.c:360-426` | `func NULL→PM_WORK` vs `func Some→fp_func` 双存储 + `already pending\|active` 的 Err | `worker.rs` |
| `test_worker_start_both_pending_and_active` | `worker.c:382-384` | `pending&&active → BothPendingAndActive` 的 `panic→Err` | `worker.rs` |
| `test_worker_suspend_resume_token` | `worker.c:474-504` | `save err → restore(err)` 的协程往返 | `worker.rs` |
| `test_worker_wait_is_suspend_plus_sleep` | `worker.c:510-520` | `wait → Suspended → signal → Busy` 状转 | `worker.rs` |
| `test_worker_stop_injects_eio` | `worker.c:535-550` | `stop → EIO + wake` 的两套 `sendrec` 守卫 | `worker.rs` |
| `test_worker_stop_by_endpoint` | `worker.c:555-567` | `task==ep → stop` 的端点级联 | `worker.rs` |
| `test_worker_steal_context` | `worker.c:586-607` | `from→to` 的 `w_fp` 偷换的 `target idle` 守卫 | `worker.rs` |
| `test_slot_selector_two_impls` | `worker.c:128/331` | `FirstFit` 线性 vs `RoundRobin` 轮转的 `SlotSelector` 双实现差异 | `worker.rs` |
| `test_worker_cleanup_requires_idle` | `worker.c:63-104` | `!idle → Err` 与 `idle → Ok` 的活更新守门 | `worker.rs` |
| `test_worker_yield_is_noop` | `worker.c:431-438` | `yield_now` 在单线程事件循环下 `noop` | `worker.rs` |
| `test_worker_trait_has_two_impls` | `worker.c:128` | `SlotSelector` trait 双实现 `dyn` 分发 | `worker.rs` |
| `test_worker_error_to_errno` | `worker.c:382` | `WorkerError → EINVAL/EBUSY/ENOSPC` 的 `to_errno` 映射 | `worker.rs` |
| `test_worker_slot_new_and_release` | `worker.c:45/283` | `WorkerSlot::new → bind → release` 的 `Idle↔Busy` 往返 | `worker.rs` |

测试策略：`WorkerPool` 的 `idle` 以 `new → idle` 零化样本覆盖；`may_do_pending` 以 `pending=1,busy=8→不可` 与 `busy=7→可` 的 spare 边界两样本覆盖；`allow` 以 `FALSE 时 dispatch→PENDING` 与 `TRUE 时 drain→Busy` 两样本覆盖；`start` 以 `pending+has_normal → Err` 与 `!pending&&!active → Ok assign` 两样本覆盖；`suspend/resume` 以 `Token` 的 `err` 往返样本覆盖；`stop` 以 `Task==NONE 时不注入` 的 `None` 守卫样本覆盖；`SlotSelector` 以 `FirstFit` 最低空闲 vs `RoundRobin` 轮转的 `0→3` 差异样本覆盖。

---

## 6 过渡

本篇在 `main.c:445` 的 `sef_cb_init_fresh` 单点（`worker_init` 的零化）之后、主循环 `get_work` 的 `reviving != 0 → unblock` 优先路径之前，是 `07` 的 `tll` 等待队列之后、`09` 的消息分发之前的“可抢占分发”前提。

```
07-tll-lock: tll 的 t_readonly-- + write优先选头 + UPGR/PEND 正交  （tll_wait 的 worker_wait 等待）
  │
  └─► 本章: worker[9] 的 pending/busy/block_all 三计数 + w_fp ↔ fp_worker 双向绑定 + suspend/resume/wait/signal/stop 的协程与睡眠  （worker_init 的 Pool::new + worker_allow 的门控开关）
         │
         ├─► 09-main-loop: get_work 的 reviving 优先 vs ANY 接收 + who_p<0 的 task 忽略 + IS_BDEV/CDEV/SDEV 回复  （依赖 worker 的 may_do_pending 门控）
         └─► 10-pm-protocol: service_pm 的 12 个 VFS_PM_* 的 worker_start 双轨  （VFS_PM_FORK 的非 PM_WORK 与 VFS_PM_EXEC 的 PM_WORK 分化）
```

`tll` 的 `EBUSY → tll_append → worker_wait` 阻塞在 `07` 显式，本章的 `worker_suspend` 在 `tll` 层等待 `READSER→WRITE` 升级；`pipe` 的 `suspend` 在 `17` 显式，本章的 `worker_wait` 在 `pipe` 层等待读端/写端到达——二者合流于同一 `w_event` 队列（`tll.c:278` 的 `write/serial` 双队列头唤醒即信号此队列）。

阅读顺序提示：若关心“分发如何被门控”，下一站 `09-main-loop.md`（`get_work` 的 `reviving` 优先与 `handle_work` 的 `use_spare`）；若关心“分发如何与 PM 延期交互”，下一站 `10-pm-protocol.md`（`worker_start` 的 `PM_WORK` 与 `service_pm_postponed` 的消费）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/threads.h:1-38`（`worker_thread` 12 字段 / `mthread` 宏映射 / `INVALID_THREAD`）、`minix3/minix/servers/vfs/worker.c:1-607`（`pending/busy/block_all` 三全局 / `worker_init/cleanup/idle/available/may_do_pending/allow/assign/get_work/main/yield/sleep/wake/suspend/resume/wait/signal/stop/stop_by_endpt/get/set_proc` 22 函数 + `thread_cleanup`）、`minix3/minix/servers/vfs/glo.h:22/34/37/41`（`fp/self/workers/err_code/ROOT_FS_E` / `who_p/fproc_addr/who_e/job_m_in` 宏）、`minix3/minix/servers/vfs/const.h:9`（`NR_WTHREADS 9` / `FP_BLOCKED_ON_*`）、`minix3/minix/servers/vfs/main.c:68-137`（`worker_yield + send_work + get_work + IS_VFS_FS_TRANSID→do_reply + PM→service_pm + notify + IS_BDEV/CDEV/SDEV + handle_work` 五路分发）、`501-523`（`do_init_root` 的 `worker_allow(FALSE/TRUE)` 两阶段）、`558-575`（`thread_cleanup` 的 `VMNT_CALLBACK` 回收）、`528-553`（`lock_proc/unlock_proc` 的 `suspend→lock→resume` 协程）
- 阶段文档：`02-fproc-struct.md`（`BlockedOn` 七态与 `FProc.flags: PENDING/PM_WORK`）、`03-fproc-table.md`（`isokendpt` 三守卫与 `PID_FREE/NONE` 双哨兵）、`07-tll-lock.md`（`tll` 的 `lock → EBUSY → append → wait` 排队）、`09-main-loop.md`（`get_work` 的 `reviving` 优先与 `do_work` 的 `SUSPEND`）、`10-pm-protocol.md`（`service_pm` 的 `worker_start(..., NULL)` 的 `PM_WORK` 延期）、`99-global-concepts.md`（`NR_WTHREADS` 常量与 `WorkerState` 术语）
- Rust 实现：`os/servers/vfs/src/worker.rs:1`（`WorkerPool/WorkerSlot/WorkerState/WorkerFunc/SuspendToken`）、`os/servers/vfs/src/fproc.rs:1`（`FProc.flags` 的 `PENDING/PM_WORK` 二位 + `BlockedOn` 枚举）、`os/servers/vfs/src/main_loop.rs:1`（`VfsState.worker_pool` 聚合与 `BootPhase::Mounting` 的 `allow` 门控）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`RS` 的 `boot image` 与 `NR_PROCS` 同界）、`../01-stage-kernel/16-smp.md`（`mthread` 的 `BKL` 假设与 `!Send` 单线程事件循环）

