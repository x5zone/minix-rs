# 09 — 主循环与分发：`main` 的 `reviving` 优先与 `TRNS_GET_ID` 路由及 `call_vec[64]` 的枚举分发

本文讲清 VFS 如何在 `reviving` 复活计数、`TRNS_GET_ID` 的 `0xFFFF` 线程 id 编码、`VFS_TRANSID` 的 `is_fs_transid` 守门、`who_e == PM_PROC_NR` 的 PM 守门、`is_notify` 的三通知守门、`who_p < 0` 的 task 忽略、`IS_BDEV/CDEV/SDEV_RS` 的驱动守门、以及 `call_vec[64]` 的 `VFS_BASE` 索引与 `SUSPEND` 延迟回复的约束下，以 `get_work` 的 `reviving → ANY 接收` 与 `main` 的 `transid → PM → notify → task → BDEV/CDEV/SDEV → syscall` 八路优先级及 `do_work` 的 `VFS_CALL → call_vec → reply/SUSPEND` 三分支建立 `Message → Route → Worker` 的可逆分发，并以 `reply/replycode` 的 `ipc_sendnb` 非阻塞与 `unblock` 的 `FP_REVIVED` 清除为阻塞恢复提供可观测路径。

前置阅读：`01-vfs-init-main.md`（`sef_cb_init_fresh` 的 `worker_allow` 门控与 `do_init_root` 时序）、`08-worker-thread.md`（`WorkerPool` 的 `may_do_pending` 与 `w_fp↔fp_worker` 双向绑定）、`02-fproc-struct.md`（`BlockedOn` 七态与 `reviving` 语义）。

> 本章不讲什么：
> - `service_pm` 的 12 个 `VFS_PM_*` 请求内容—— `10-pm-protocol.md`（`VFS_PM_FORK` 为例的次主线在 10）
> - `comm.c` 的 `m_comm` 队列与 `send_work` 的 `VMNT_CALLBACK` 细节—— `11-fs-comm.md`
> - `bdev/cdev/sdev_reply` 的驱动回复内部—— `20-bdev.md/21-cdev.md/22-sdev.md`（每路 `reply` 后 `worker_signal`）
> - `select` 定时器 `expire_timers` 的时钟管理—— `23-select.md`（`CLOCK` notify 的消费方）
> - `do_pending_pipe` 的 pipe 重入与 `lock_filp` 时序—— `17-pipe.md`（`unblock` 的 `worker_start(...,do_pending_pipe)` 分支）
> - `DS` 事件 `ds_event` 的 `map_service` 细节—— `19-device-map.md`
> - 内核 `sef_receive/ipc_sendnb/sys_kill` 的 IPC 原语—— `99-global-concepts.md` + `../01-stage-kernel/12-ipc-core.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 08 的请求槽状态机（`pending/busy/allow` 三计数与 `SuspendToken`）与 02 的 `FP_REVIVED`/`FP_BLOCKED_ON_PIPE` 语义，能 `grep "reviving\|TRNS_GET_ID\|call_vec" minix3/minix/servers/vfs/main.c` 的开发者。

### 1.1 心脏的职责：阻塞不应传染

VFS 的主循环（`main.c:68-139` `while(TRUE) { yield; send_work; get_work; dispatch }`）是 VFS 的**心脏**——唯一从内核收消息并决定“谁去做”的线程。与 PM 的 `get_work` 只做 `sef_receive(ANY)` 不同，VFS 的 `get_work:580` 必须在收新消息**之前**先看 `reviving != 0`：若管道或锁的 `FP_REVIVED` 进程在等待复活，直接 `unblock` 恢复其请求，不收新消息。`reviving` 的 `for (rp: PID_FREE && REVIVED → unblock)` 优先于 `sef_receive(ANY)` 的 `for(;;) sef_receive` 的**复活优先**，使 `write` 的 `SUSPEND` 在 `read` 到达后由 `revive` 立即复活，而非等下一轮 `main` 循环才收 `read`。

此优先级的代价是 `get_work` 的 `TRUE/FALSE` 双返回：`TRUE` 表示“`m_in` 已填新消息，主循环继续分发”；`FALSE` 表示“已 `worker_start(...,do_pending_pipe)` 启动复活工作，无新 `m_in` 可分发，`main` 直接 `continue`”。`FALSE` 的“无消息但有工作产生”在 08 的 `worker_allow` 门控中亦有同型：`pending` 的消费在 `worker_get_work` 与 `worker_allow` 两处，但 09 的 `reviving` 仅 `get_work` 一处。

### 1.2 八路优先：为什么顺序不能换

`main:80-138` 的八路优先级（`transid → PM → notify → task → BDEV → CDEV → SDEV → syscall`）以 `if/else if` 的短路固化，交换任意两路即错：

1. **`IS_VFS_FS_TRANSID(transid)` → `do_reply`**（80-90，最高）——FS 的异步回复以 `m_type` 的高 16 位编码 `transid`（`vfsif.h:79` `TRNS_GET_ID` 的 `&0xFFFF`），低 8 位为 `REPLY` 状态。`VFS_TRANSID` 偏移后的 `thread_t` 回指 `workers[]` 的槽位（`worker_get(transid-VFS_TRANSID)`），`m_type = TRNS_DEL_ID` stripping 后 `*w_sendrec = m_in` + `c_cur_reqs--` + `worker_signal`。若此路后于 PM，将误把 FS 的 `VFS_TRANSID+tid` 当 PM 消息处理（`PM_PROC_NR` 的 `1` endpoint 恰为 `workers[0]` 的 `tid` 偏移，歧义）。

2. **`who_e == PM_PROC_NR` → `service_pm`**（91-93）——PM 是唯一可对 VFS 发 `VFS_PM_*` 的端点（`com.h:513` `VFS_PM_RQ_BASE 0x900`），其消息永不经 `call_vec`，且 `service_pm` 的 `VFS_PM_UNPAUSE` 等延期请求在 `worker_start(..., NULL)` 后即 `return`（不经后续 `handle_work`）。若此路后于 `syscall`，PM 的 `VFS_PM_FORK` 将被 `is_notify` 误判为 notify（`call_nr` 的 `NOTIFY_MESSAGE` 范围恰与 `VFS_PM_*` 部分重叠的风险受 `IS_VFS_FS_TRANSID` 优先保护）。

3. **`is_notify(call_nr)` → `DS/KERNEL/CLOCK`**（95-117）——`is_notify` 的 `(a-NOTIFY) <0x100`（`com.h:93`）捕获所有 `notify`，`who_e` 再分流：`DS_PROC_NR==6` 的 `ds_event` 需 `worker_can_start(fp)` 守门（08 的可加性）；`KERNEL==-1` 的 `mthread_stacktraces`；`CLOCK==-3` 的 `expire_timers(timestamp)`。若此路后于 `task ignore`，任务的 `notify` 将被 `who_p<0` 误判为 ignore。

4. **`who_p < 0` → `continue` ignore**（118-124）——`_ENDPOINT_P` 负值即任务（kernel/drivers 的伪 endpoint），VFS 对任务消息只收 `notify`（上路），普通消息直接忽略（告警 `ignoring message from`）。此路必须在 `BDEV/CDEV/SDEV` 之前，否则驱动的 `sendrec` 回复（正 endpoint）将被误忽略。

5. **`IS_BDEV/CDEV/SDEV_RS(call_nr)` → 三驱动回复**（126-134）——`RS_BASE` 的 `&~0x7f` 前缀匹配（`com.h:923/967/1041`），每路 `*_reply` 的 `w_task` 校验 + `c_cur_reqs--` + `worker_signal` 与 `do_reply:209` 同型，但对端是 `dmap/smap` 的 driver endpoint（19 的设备表），非 FS 的 `vmnt`。

6. **else `handle_work(do_work)` → 正常 syscall**（135-138）——`do_work:283` 的 `IS_VFS_CALL → call_index = job_call_nr-VFS_BASE → call_vec[index] ? call : ENOSYS` + `error != SUSPEND → reply`。`handle_work:146` 的 `FP_SRV_PROC` 分支在 `vmnt.m_flags & CALLBACK → EAGAIN` 或 `worker_available()==0 → EAGAIN` 或 `→ m_flags|=CALLBACK|FORCEROOTBSF` 的死锁防护后，以 `use_spare=TRUE` 调 `worker_start`。

优先级的不变量是 `transid` 的 `~0xff` 掩码与 `VFS_PM_*` 的 `0x900/0xA00` 范围与 `is_notify` 的 `0x100` 窗口的**编码不重叠**——`TRNS_GET_ID` 的 `0xFFFF` 提取在 `0x100-0x3FF` 的 `VFS_CALL` 范围外编码 `VFS_TRANSID` 的 `0x1000+` 偏移，使 FS 回复在 `m_type` 域内可区分。

### 1.3 `call_vec[64]`：函数指针表的类型化枚举演进

Minix3 的 `table.c:17` 的 `int (*const call_vec[NR_VFS_CALLS])(void) = { CALL(VFS_OPEN)=do_open, … }` 以 `CALL(n)=[n-VFS_BASE]` 的指定初始化将 64 调用号映射到 handler，调用点 `call_vec[call_index]` 的 `index <64 && !=NULL ? call : ENOSYS` 在 `do_work:283` 的 `switch` 外实现 `O(1)` 分发。问题有三：`int(*)(void)` 的 `void` 使 `m_in/m_out` 经全局 `err_code/m_in` 隐式传递；`NULL` 的哨兵在 `Enable syscall stats` 的 `calls_stats` 埋点中需 `!=NULL` 守门；`VFS_RMDIR→do_unlink` 的别名（`table.c:36` `CALL(VFS_RMDIR)=do_unlink`）在 C 中无类型区分。

`ARCH A-2` 的 minix-rs 演进是 `VfsCallNum` 的 64 变体枚举 + `CallTable::dispatch` 的 `match` 穷尽：`VFS_BASE+0x00..0x3F` 的 64 变体在 `call_table.rs:28` 的 `repr(u32)` 枚举中显式，`from_raw(raw)->Option<VfsCallNum>` 的 `raw-VFS_BASE` 检查替代 `call_index<64`，`dispatch(call)->Result` 的 `match` 使 `VFS_RMDIR` 的别名在语义层可区分（`Rmdir` 变体可独立匹配）而非复用 `do_unlink` 的函数指针别名。

### 1.4 回复的双路径：`reply` 的 `ipc_sendnb` 与 `SUSPEND` 的延迟

`do_work:297` 的 `if (error != SUSPEND) reply(&job_m_out, fp->fp_endpoint, error)` 使 `SUSPEND` 的三恢复路径（`17-pipe.md` 的 `revive`、`23-select.md` 的 `select_return`、`21-cdev.md/22-sdev.md` 的 `*_reply`）不回复；`reply:638` 的 `m_out->m_type=result; ipc_sendnb(whom,m_out)` 非阻塞发送（`KERNEL` 的 `sendnb` 不等待接收方 `receive`），失败仅 `printf+stacktrace`（`644`）。`replycode:655` 的 `memset+reply` 在 `handle_work:160` 的 `CALLBACK/SUSPEND` 路径的 `EAGAIN` 注入中复用。

`ARCH A-5` 的演进是 `ReplyIntent::Reply(i32)/ReplyLater/NoReply` 的显式契约：`SUSPEND` 在 Rust 以 `ReplyLater` 的 `must-be-revived` 状态显式，`reviving` 的 `for REVIVED→unblock` 优先路径成为该状态的消费方（17/23 的 revive 路径再转 `worker_start(...,do_pending_pipe)`）。

### 1.5 全局的聚合：`glo.h` 的 `fp/m_in/self/call_nr` 宏与 `VfsState`

C 的 `glo.h:26-32` 的 `who_p/who_e/call_nr/job_m_in/job_call_nr` 宏以 `fp - fproc` 的指针算术与 `self != NULL ? fp->fp_endpoint : m_in.m_source` 的 TLS 分叉封装“当前谁在调用”。`fp` 的 `EXTERN struct fproc *fp` 与 `self` 的 `EXTERN worker_thread *self` 的全局 TLS 在 `worker_main:248` 的 `fp=self->w_fp` 写入与 `worker_yield:437` 的 `self=NULL` 交出间摆动。`reviving` 的 `16` 与 `susp_count` 的 `14` 的全局计数在 `pipe.c:revive` 的 `REVIVING` 置位中递增，于 `unblock:958` 的 `~REVIVED` 清除中递减。

`ARCH A-4` 的演进是 `VfsState { fproc_table, worker_pool, call_table, reviving, current_message, current_fp_slot, boot_phase … }` 的聚合：`fp/m_in/self` 的 TLS 在 Rust 以 `VfsState::current_fp_slot: Option<UserSlot>` 与 `current_message: Message` 的显式状态替代；`who_p/who_e/call_nr` 的宏在 Rust 以 `VfsState::caller_slot() / caller_endpoint() / call_nr()` 的方法显式，借用规则替代 `fp_lock` 的互斥。

### 1.6 与其他 OS 的主循环对照

- **Linux** `workqueue` + `VFS`：`workqueue` 的 `worker_thread` 在 `pool.worklist` 上 `while (!list_empty(worklist)) { work = list_first → process_one_work }` 轮询；`VFS` 的 `do_reply` 的 `c_cur_reqs--` 在 Linux 以 `sb->s_active--` 的 `mount` 引用计数近似；`Linux` 的 `call_vec` 在 `file_operations` 的 `read/write/llseek` 等 `op` 指针表中以 `inode->i_fop->read_iter` 的虚表分发替代 `VFS` 的平表 `call_vec`。
- **Redox** `Scheme` 事件循环：`Redox` 的 `scheme.handle(Request::Call(SchemeCall { number, arg }))` 以 `match number { 0x100 => read, 0x101 => write }` 的 `match` 枚举（与 `CallTable::dispatch` 同型）；`Redox` 的 `transid` 在 `Scheme` 的 `HandleId` 的 `u64` 句柄直接索引 `Slab`，`VFS` 的 `TRNS_GET_ID` 的高 16 位编码在 `Redox` 以 `HandleId::from_raw` 的 `usize` 索引显式；`Redox` 的 `SUSPEND` 在 `Scheme::call` 的 `Future::Pending` + `Waker` 存储，`VFS` 的 `SUSPEND` 在 `ReplyIntent::ReplyLater` + `reviving` 优先的显式计数。
- **seL4** `seL4_ReplyRecv` 单线程被动服务器：`seL4_Recv(endpoint, &badge) → badge==PM? → handle_pm : badge==FS? → handle_fs : call_handler[call]` 的 `badge` 多路复用在 `VFS` 以 `m_in.m_source` 的 endpoint 解引 + `m_type` 的编码前缀多路复用近似；`seL4` 的 `reply` 的 `seL4_Reply` 原语在 `VFS` 以 `ipc_sendnb` 的非阻塞发送近似（`seL4_Reply` 的阻塞在 `VFS` 以 `sendnb` 的 `printf` 降级）。

共同约束是“单线程事件泵的优先级”。Minix3 的选择是以 `reviving` 的复活优先 + `transid` 的 `VFS_TRANSID` 偏移路由 + `PM` 的同步屏障 + `is_notify` 的任务伪装防护 + `RS` 前缀的驱动 `RS_BASE` 分流 + `call_vec` 的平表 `O(1)` 的八路短路链路使 `SUSPEND` 的延迟回复在 `reviving` 的 `unblock` 复活中可观测。

### 1.7 小结

主循环是 `VFS` 的单线程事件泵，`reviving` 的复活优先于 `ANY` 接收使管道恢复不饥饿，`TRNS_GET_ID` 的高 16 位 `VFS_TRANSID` 路由使多 worker 的 FS 异步回复可定位，`PM` 的 `VFS_PM_*` 守门使控制面与数据面分离，`is_notify` 的三通知与 `who_p<0` 的 task 忽略使驱动 `notify` 与 `notify` 歧义消解，`IS_BDEV/CDEV/SDEV_RS` 的 `RS_BASE` 前缀使块/字符/套接字驱动各回各家，`handle_work(do_work)` 的 `VFS_CALL→call_vec→reply/SUSPEND` 使 64 调用的 POSIX 语义在 worker 的可睡眠锁中可执行。下一节以 `main.c:54-973` 全文与 `table.c` 的 64 调用为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `glo.h:24-44` 的五宏与六全局

`glo.h:25` 的 `m_in: message` 输入，`13` 的 `fp: fproc*` 当前进程，`16` 的 `reviving/susp_count` 复活/挂起计数，`34` 的 `self: worker_thread*` TLS，`41` 的 `err_code: int` 暂存。宏：`glo.h:26 who_p (fp-fproc)` 槽索引、`27 fproc_addr(e) (&fproc[_ENDPOINT_P(e)])` 地址、`28 who_e (self?fp->endpoint:m_source)` 真实来源、`29 call_nr (m_in.m_type)` 调用号、`30 job_m_in (self->w_m_in)` worker 输入、`32 job_call_nr (w_m_in.m_type)` worker 调用号。`who_e` 的 `self != NULL` 分叉使 `service_pm` 的 `who_e==PM_PROC_NR` 在 worker 上下文与主循环上下文均可判。

### 2.2 `table.c:17-82` 的 `call_vec[64]` 平表

`table.c:15` 的 `CALL(n)=[n-VFS_BASE]` 指定初始化将 64 调用的 `VFS_READ..VFS_SHUTDOWN`（`callnr.h:72-136`）映射到 `do_read..do_shutdown` 64 handler：文件族 `READ/WRITE/LSEEK/OPEN/CREAT/CLOSE/PIPE2/GETDENTS`（`15/16/17`）、名字族 `LINK/UNLINK/RENAME/SYMLINK/READLINK/MKDIR/MKNOD/CHDIR/FCHDIR/CHROOT`（`13/27/28`）、元数据族 `STAT/FSTAT/LSTAT/CHMOD/FCHMOD/CHOWN/FCHOWN/UMASK/ACCESS/TRUNCATE/FTRUNCATE/UTIMENS`（`28/29/31`）、挂载族 `MOUNT/UMOUNT/STATVFS1/FSTATVFS1/GETVFSSTAT`（`18/28`）、控制族 `IOCTL/FCNTL/SELECT/SYNC/FSYNC/VMCALL/COPYFD/MAPDRIVER/GETSYSINFO/SVRCTL/GCOV_FLUSH/GETRUSAGE`（`19/23/30/31`）、套接字族 `SOCKET..SHUTDOWN`（`24`）的 6 族。`CALL(VFS_RMDIR)=do_unlink` 的别名与 `VFS_SENDMSG/RECVMSG→do_sockmsg` 的复用在平表内显式为不同索引同函数指针。

### 2.3 `main:54-141` 的八路主循环

`main.c:64 sef_local_startup` 后 `66 printf("Started VFS: %d worker thread(s)", NR_WTHREADS)`、`69 while(TRUE) { worker_yield; send_work; if(!get_work) continue; … }` 的三阶段：`70 worker_yield` 交出 TLS、`72 send_work` 的 `m_comm.c_cur_reqs` 恢复、`77 get_work` 的 `TRUE/FALSE` 双返回。`80-90` 的 `TRNS_GET_ID → IS_VFS_FS_TRANSID → worker_get(transid-VFS_TRANSID) → NULL/false→spurious → TRNS_DEL_ID → do_reply` 的 `+thread` 偏移路由与 `91-93` 的 `PM → service_pm` 同级。

### 2.4 `get_work:580-633` 的 `reviving` 优先与 `ANY` 接收

`get_work:590 if(reviving !=0) { for(rp: PID_FREE && REVIVED → return unblock(rp)) ; panic }` 的复活优先 + `599 for(;;) { sef_receive(ANY,&m_in); proc_p=_ENDPOINT_P(m_source); fp=proc_p<0||>=NR?NULL:&fproc[proc_p]; if(fp && endpoint==NONE → continue ignore NONE) ; if(fp && endpoint != who_e → panic inconsistent) ; return TRUE }` 的端点一致性校验（`fp->endpoint == who_e` 的双版本 `fproc[who_p]` vs `fp` 的互证）。

### 2.5 `unblock:921-973` 的 `PIPE→worker_start(do_pending_pipe)` 分叉

`unblock:931 blocked_on = rfp->fp_blocked_on; 934 m_in.m_source=endpoint; 936 switch(PIPE → m_type=fp_pipe.callnr+readwrite fields ; FLOCK → VFS_FCNTL+fctl fields ; default panic) ; 957 rfp->blocked_on=NONE; flags&=~REVIVED; reviving--; assert >=0; 965 if(PIPE) { worker_start(rfp,do_pending_pipe,&m_in,FALSE); return FALSE } ; 971 fp=rfp; return TRUE` 的“管道重入用 `do_pending_pipe` 独立路径（`main.c:216` 的 `lock_filp+rw_pipe+EPIPE+SIGPIPE`），锁用 `return TRUE` 复用原请求（`main.c:971 fp=rfp` 的 `fp` 全局指向复活进程）”。

### 2.6 `handle_work:146-181` 的 `FP_SRV_PROC` 死锁防护

`handle_work:156 if(FP_SRV_PROC) { vmp=find_vmnt(proc_e); if(vmp && CALLBACK→EAGAIN; if(available==0→EAGAIN; vmp->flags|=CALLBACK; if(MOUNTING→FORCEROOTBSF); } use_spare=TRUE; worker_start(fp,func,&m_in,use_spare)` 的回调 guard 与 `do_reply:191 find_vmnt → CALLBACK 清除`（`main.c:558 thread_cleanup` 的 `flags&=~CALLBACK` 与之互证）的往返。

### 2.7 `do_reply:187-211` 的 `w_task` 校验与 `c_cur_reqs--`

`do_reply:191 find_vmnt(who_e) → NULL&&!VM → panic ; 194 w_task != who_e → printf expected/not ; 202 w_sendrec==NULL → late reply ignored ; 206 *w_sendrec=m_in; 207 w_sendrec=NULL; 208 w_task=NONE; 209 if(vmp) c_cur_reqs--; 210 worker_signal` 的“请求/回复配对的 `w_sendrec` 持有度与 `c_cur_reqs` 窗口计数”双守门。

### 2.8 `do_work:263-298` 的 `VFS_CALL → call_vec → reply/SUSPEND`

`do_work:268 if(pid==PID_FREE→return drop vanished); 275 memset(job_m_out,0); 283 if(IS_VFS_CALL(job_call_nr)) { index=job_nr-VFS_BASE; if(index<NR && call_vec[index]!=NULL) error=call(); else ENOSYS } else ENOSYS; 297 if(error != SUSPEND) reply(&job_m_out,endpoint,error)` 的 `vanished 进程丢弃` + `VFS_CALL 范围守门` + `NULL handler→ENOSYS` + `SUSPEND 分叉`。

### 2.9 `reply:638-663` 的 `ipc_sendnb` 非阻塞

`reply:643 m_out->m_type=result; 644 ipc_sendnb(whom,m_out); 645-649 if(!OK) printf+stacktrace` 的非阻塞 + `replycode:655 memset+reply` 的 `EAGAIN` 注入复用（`handle_work:160` 的 `replycode(proc_e,EAGAIN)`）。

### 2.10 `TRNS_GET_ID/TRNS_DEL_ID/VFS_TRANSID` 的编码

`vfsif.h:79 TRNS_GET_ID(t) ((t)&0xFFFF)` 的高 16 位 `transid` 提取，`81 TRNS_DEL_ID(t) ((short)(t)>>16)` 的 `m_type` 高位 stripping，`com.h:911 VFS_TRANSID (TRANSACTION_BASE+1)` 的 `thread_t` 偏移，`912 IS_VFS_FS_TRANSID(type) ((type&~0xff)==TRANSACTION_BASE)` 的 `~0xff` 前缀匹配，三者的 `thread_t` 往返在 `main:82 worker_get(transid-VFS_TRANSID)` 处闭合。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `main.c:80` 的 `transid = TRNS_GET_ID(m_in.m_type)` 与 `call_vec[call_index]()` 的裸指针，而是吸收 Redox/Linux 的类型化分发后做取舍。以下决策对应 `.design/09-design.v1.md` D1-D6。

### D1 八路优先显式：`Route` 枚举的优先级路由

- **C**：`if/else if` 的隐式优先级链（80-135），`reviving` 在 `get_work:590` 内而非 `main`，`TRNS_GET_ID` 的 `0xFFFF` 掩码在 `main:80` 局部。
- **Rust**：`Route` 枚举（`Revived { slot } / FsReply { wp } / Pm / Notify { source } / TaskIgnored / Bdev / Cdev / Sdev / Syscall { call }`）+ `VfsState::route_message(&Message) -> Route` 的 `match` 穷尽；`get_work` 的 `reviving` 优先在 `VfsState::poll_next() -> PollResult` 的 `if reviving>0 { find REVIVED→Unblock → Queued } else { receive }` 显式，使 `main` 的 `while let PollResult::Ready(msg) = poll_next() { route → handle }` 的优先级可审计。
- **为什么**：`if/else if` 的顺序依赖在 Rust 以枚举的 `priority()` 排序函数显式，`reviving` 的 `return unblock` 的 `FALSE→continue` 在 Rust 以 `PollResult::Revived` 的非消息分支显式。

### D2 函数指针表 → 类型化枚举分发（ARCH A-2）

- **C**：`int(*call_vec[64])(void)` 的 64 函数指针 + `CALL(VFS_RMDIR)=do_unlink` 别名复用（`table.c:36`）。
- **Rust**：`VfsCallNum` 的 64 变体 `enum`（`call_table.rs:28` `repr(u32)`，`VFS_BASE+0x00..0x3F`）+ `CallTable { handlers: [Option<VfsCallNum>;64] }` 的 `lookup(raw)->Option<VfsCallNum>` + `CallResolver` trait 的 `resolve(raw)->Option<VfsCallNum>` + `TransIdCodec` trait 的 `encode/decode/is_fs_transid`；`CallResolver` 的 `CallTable`（查表）与 `NullResolver`（永 `None`）双实现在 `call_table.rs:339/350` 行为不同（`VFS_OPEN → Some(Open) vs None`）；`TransIdCodec` 的 `VfsTransIdCodec`（`base 0xB00`）与 `TestTransIdCodec { base: 0xC00 }` 在 `main_loop.rs:85` 行为不同（`0xB01 → Vfs true / Test false`）；`Rmdir` 与 `Unlink` 的别名在语义层分 `Rmdir` 变体独立，handler 内 `match` 可 `Rmdir => do_unlink()` 复用但调用点可区分。
- **为什么**：`NULL` 的哨兵在 Rust 以 `Option` 的 `None→ENOSYS` 显式，`call_index<64` 的边界在 Rust 以 `raw.checked_sub(VFS_BASE) → index → Option` 的 `Result` 显式；`calls_stats` 的埋点在 Rust 以 `#[cfg(feature="syscall_stats")]` 的 `AtomicUsize` 计数显式；`resolve` 的 trait 抽象使 08 的 `Revived` 优先与 09 的 `FsReply` 路由的 `call_vec` 查询在测试中可 `NullResolver` 的拒绝样本覆盖。
- **备选**：保留 `fn()` 指针表；否决——`void` 的隐式 `m_in` 依赖在 `CallResolver::resolve` 的 `&self` 显式上下文替代。

### D3 TransId 类型化（A-2 的子决策）

- **C**：`TRNS_GET_ID(t)&0xFFFF` / `VFS_TRANSID+tid` 的裸 `u16` 编码（`vfsif.h:79/81` / `com.h:911`）。
- **Rust**：`TransId { slot: usize }` 的 newtype + `TransIdCodec` trait 的 `encode(slot)->u32` / `decode(raw)->Option<usize>` / `is_fs_transid(raw)->bool` + `VfsTransIdCodec`（`TRANSACTION_BASE 0x1000 + slot`）与 `TestTransIdCodec { base }` 双实现；`Route::FsReply` 的 `wp` 在 `codec.decode(TRNS_GET_ID(raw))` 的 `Option` 守门后取得，避免 `worker_get(NONE) → spurious` 的 `printf` 分支在 Rust 以 `Err(Spurious)` 的 `log::warn` 显式。
- **为什么**：`~0xff` 的 `TRANSACTION_BASE` 掩码在 trait 的 `is_fs_transid` 方法内可测试（`VFS_BASE 0x100` 与 `TRANSACTION_BASE 0x1000` 的编码不重叠在 `is_fs_transid(raw:0x100)->false` 的样本中显式）。

### D4 回复语义显式（ARCH A-5）

- **C**：`return SUSPEND` 的 `SUSPEND` 哨兵（`main.c:297` `error != SUSPEND → reply`）+ `reviving` 的 `for REVIVED→unblock` 复活 + `pipe/select/cdev/sdev` 的 `SUSPEND→revive` 三路径。
- **Rust**：`ReplyIntent::Reply(i32)` / `ReplyLater` / `NoReply` 的 `A-5` 枚举（`main_loop.rs:118` 已落地）+ `ReplySink` trait 的 `send(endpoint, code)` + `PendingSink`（`ReplyLater` 的 `reviving++` 与 `unblock` 的 `reviving--` 往返在 `VfsState::enqueue_revive(slot)` 的 `flags|=REVIVED` 显式）。
- **为什么**：`SUSPEND` 的哨兵在 C 以 `int` 的 `0xFFFF` 保留值隐式，Rust 以 `ReplyLater` 的 `must_be_revived` 状态不与 `0..0x3FF` 的成功码混淆；`ipc_sendnb` 的失败 `printf` 在 Rust 以 `ReplySink::send` 的 `Result<(),SendError>` 的 `log::error` 显式。

### D5 全局聚合（ARCH A-4）

- **C**：`glo.h:13 fp` / `16 reviving` / `25 m_in` / `34 self` / `41 err_code` 的 5+ 全局 TLS。
- **Rust**：`VfsState { fproc_table, worker_pool, call_table, reviving, current_message, current_fp_slot }` 的聚合（本篇新增 `reviving` 的 `usize` 与 `current_message` 的 `Message` 显式；`who_p/who_e/call_nr` 的宏在 `VfsState::caller_slot()/caller_endpoint()/call_nr()` 的方法显式，`fp` 的全局在 Rust 以 `&mut FProc` 的借用替代 `fp_lock` 的互斥）。
- **为什么**：全局的隐式 TLS 在 08 的单线程事件循环假设下以 `VfsState` 的显式聚合替代，阶段间 `reviving` 的 `unblock` 复活与 `get_work` 的 `reviving` 优先在 `VfsState::poll_next` 的借用中可审计。

### D6 复活入口的二路分叉

- **C**：`unblock:957 blocked_on==PIPE→worker_start(...,do_pending_pipe,FALSE)→FALSE` vs `FLOCK→fp=rfp→TRUE`。
- **Rust**：`UnblockOutcome::QueuedPipe` / `RevivedLock` 的枚举 + `VfsState::unblock(slot)->Result<UnblockOutcome, UnblockError>` 的 `match blocked_on { Pipe(fb) => QueuedPipe{ fb } , Flock(fb) => RevivedLock{ fb } }`；`QueuedPipe` 的 `worker_start(...,DoPendingPipe)` 与 `RevivedLock` 的 `current_fp_slot=slot` 分化在 `poll_next` 的 `Reviving → match unblock → QueuedPipe{continue}|RevivedLock{route Syscall}` 显式。
- **为什么**：`return FALSE` 的“无消息但有工作”与 `return TRUE` 的“复用 `fp` 的原请求”在 C 以布尔隐式，Rust 以 `QueuedPipe` 的 `WorkerPool::assign` 与 `RevivedLock` 的 `Route::Syscall` 分流显式。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-2 函数指针表 → 枚举分发 | `CallTable` + `VfsCallNum` + `CallResolver`(`CallTable` vs `NullResolver`) + `TransIdCodec`(`Vfs` 0xB00 vs `Test` 0xC00) | `call_table.rs:CallResolver` + `main_loop.rs:TransIdCodec` + 本文档 D2 + 09 正文 1.3 |
| A-4 全局 → VfsState 聚合 | `VfsState { reviving, current_message, current_fp_slot }` | `main_loop.rs:VfsState` + 本文档 D5 + 09 正文 1.5 |
| A-5 SUSPEND→ReplyIntent | `ReplyIntent::ReplyLater` + `reviving` 往返 | `main_loop.rs:ReplyIntent` + 本文档 D4 + 09 正文 1.4 |
| A-8 64 位 | `VFS_BASE 0x100` 的 `u32` 枚举 | `call_table.rs:VfsCallNum` + 本文档 D2 + 09 正文 2.2 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── main_loop.rs        — VfsState{ fproc_table, worker_pool, call_table, reviving, current_message, current_fp_slot, boot_phase… } + Route/TransIdCodec(Vfs 0xB00 vs Test 0xC00)/reply/unblock/poll_next
├── call_table.rs       — VfsCallNum(64)/CallTable[64]/CallResolver(CallTable vs NullResolver)
├── worker.rs           — WorkerPool 的 may_do_pending 与 w_fp 互证（08）
└── fproc.rs            — FpFlags::REVIVED 与 BlockedOn::Pipe/Flock 互证（02）
```

### 4.2 `call_table.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `VFS_BASE 0x100` | `callnr.h:68` | `call_table.rs:VFS_BASE: u32 =0x100` | `repr(u32)` 枚举基址 |
| `NR_VFS_CALLS 64` | `callnr.h:137` | `NR_VFS_CALLS: usize =64` | `handlers:[Option<VfsCallNum>;64]` 长度 |
| `call_vec[64]` | `table.c:17` | `CallTable { handlers:[Option<VfsCallNum>;64] }` | `lookup(raw)->Option<VfsCallNum>` 的 `raw-VFS_BASE` 检查 |
| `is_notify` | `com.h:93` | `main_loop.rs:NotifyKind::is_notify(raw)` | `(raw-NOTIFY)<0x100` 前缀匹配 |

### 4.3 `main_loop.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `main:54` 八路循环 | `main.c:54-141` | `VfsState::poll_next() -> PollResult` + `route_message()->Route` | `Revived优先 → transid → PM → notify → task → BDEV/CDEV/SDEV → syscall` |
| `get_work:580` | `main.c:580-633` | `poll_next()` 的 `reviving !=0 → find REVIVED→unblock` 优先 + `receive(ANY)` 的端点一致性校验 | `Endpoint::NONE 的 continue ignore` 与 `endpoint != who_e → Panic` 互证 |
| `unblock:921` | `main.c:921-973` | `unblock(slot)->UnblockOutcome` | `Pipe→QueuedPipe(worker_start do_pending_pipe) / Flock→RevivedLock(fp=slot)` |
| `do_reply:187` | `main.c:187-211` | `handle_fs_reply(wp, msg)` | `w_task==who_e` 校验 + `w_sendrec==NULL→LateIgnored` + `c_cur_reqs--` + `signal` |
| `do_work:263` | `main.c:263-298` | `call_table.dispatch(call)->ReplyIntent` | `IS_VFS_CALL → index<64 && Some(call) → dispatch → ENOSYS` 的 `Option` 守门 |
| `reply:638` | `main.c:638-650` | `ReplySink::send(endpoint, code)` | `m_type=result; sendnb → Err(log)` |
| `TRNS_GET_ID` | `vfsif.h:79` | `TransIdCodec::decode(raw&0xFFFF) → Option<usize>` | `~0xff` 的 `TRANSACTION_BASE` 掩码在 `is_fs_transid` 显式 |

### 4.4 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 复活优先 `reviving>0 → unblock` | `poll_next` | `if reviving>0 { for REVIVED→unblock }` | `main.c:590` |
| 编码不重叠 `IS_VFS_FS_TRANSID & IS_VFS_CALL==∅` | `TransIdCodec::is_fs_transid` | `(raw&~0xff)==TRANSACTION_BASE` vs `(raw&~0xff)==VFS_BASE` | `com.h:912` vs `callnr.h:70` |
| PM 永不经 call_vec | `route_message` | `who_e==PM_PROC_NR → Route::Pm` 的 `call_vec` 短路 | `main.c:91` |
| 任务只收 notify | `route_message` | `is_notify` 分流前 `who_p<0 → Ignored` | `main.c:118` |
| 别名 `Rmdir→do_unlink` 可区分 | `VfsCallNum::Rmdir` | `match Rmdir => do_unlink()` 的语义别名 | `table.c:36` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **112 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 = 102 → `cargo test` 实测 112 的计口径以 112 为准，含新增 trait 契约测试；`minix-types` 108 独立）。
> 本章直接影响 `23 → 35` 项新增（`poll_reviving/unblock_pipe/unblock_flock/route_transid/route_pm/route_notify/route_task/route_bdev_cdev_sdev/route_syscall/call_table_lookup/transid_codec_two_impls/reply_intent/revive计数/call_resolver`），`minix-vfs --lib` 总计 96 → 112。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_poll_reviving_priority` | `main.c:590-596` | `reviving>0 → unblock` 优先于 `receive` | `main_loop.rs` |
| `test_unblock_pipe_queued` | `main.c:965-967` | `PIPE → worker_start(DoPendingPipe) → QueuedPipe` | `main_loop.rs` |
| `test_unblock_flock_revived` | `main.c:970-973` | `FLOCK → fp=rfp → RevivedLock` | `main_loop.rs` |
| `test_route_fs_reply` | `main.c:80-90` | `TRNS_GET_ID & IS_VFS_FS_TRANSID → FsReply` | `main_loop.rs` |
| `test_route_pm` | `main.c:91-93` | `PM_PROC_NR → Pm` | `main_loop.rs` |
| `test_route_notify_ds` | `main.c:98-105` | `is_notify && DS_PROC_NR → Notify(Ds)` 的 `can_start` 守门 | `main_loop.rs` |
| `test_route_task_ignored` | `main.c:118-124` | `who_p<0 → Ignored` | `main_loop.rs` |
| `test_route_bdev_cdev_sdev` | `main.c:126-134` | `IS_BDEV/CDEV/SDEV_RS → Bdev/Cdev/Sdev` | `main_loop.rs` |
| `test_route_syscall` | `main.c:135-138` | `else → Syscall(call)` 的 `handle_work` 委派 | `main_loop.rs` |
| `test_call_table_lookup` | `table.c:17` | `VFS_READ→Some(Read), Rmdir→Some(Rmdir)`, `0→None, 0x200→None` | `call_table.rs` |
| `test_call_resolver_two_impls` | `table.c:17` | `CallResolver` 的 `CallTable(Some) vs NullResolver(None)` | `call_table.rs` |
| `test_transid_codec_roundtrip` | `vfsif.h:79/81` | `encode(slot) → decode → Some(slot)` 的 `&0xFFFF` 往返 | `main_loop.rs` |
| `test_transid_codec_two_impls_differ` | `com.h:911` | `Vfs(0xB01) vs Test(0xC01)` 的 `is_fs_transid` 行为差异 | `main_loop.rs` |
| `test_transid_not_fs_for_vfs_call` | `com.h:912` | `VFS_READ(0x100) is_fs_transid==false` 的编码不重叠 | `main_loop.rs` |
| `test_reply_intent_variants` | `main.c:297` | `Reply/ReplyLater/NoReply` 的 `!=` 可区分 | `main_loop.rs` |
| `test_reviving_counter` | `main.c:958-960` | `unblock → reviving--` 的 `assert >=0` | `main_loop.rs` |
| `test_call_resolver_trait_two_impls` | `call_table.rs:339` | `CallResolver` trait `dyn` 分发（`Table vs Null`） | `main_loop.rs` |

测试策略：`poll_next` 的 `reviving` 以 `reviving=1,FP_REVIVED 置位 → poll→Revived` 的优先样本覆盖；`route` 的八路以 `FsReply→Pm→Notify→Task→Bdev/Cdev/Sdev→Syscall` 的 6 样本覆盖；`call_table` 以 `Read/Close/0/0x200` 的 `Some/None` 两样本覆盖；`transid` 以 `encode(7)→decode 7` 的往返及 `Vfs vs Test` 双实现差异样本覆盖；`CallResolver` 以 `CallTable(Some) vs Null(None)` 的 `dyn` 行为差异样本覆盖。

---

## 6 过渡

本篇在 `08-worker-thread` 的 `WorkerPool::may_do_pending` 与 `w_fp` 可抢占之后，主循环以 `reviving` 的复活优先 + `transid` 的线程路由 + `PM` 的控制面守门 + `is_notify` 的三通知分流 + `IS_BDEV/CDEV/SDEV_RS` 的驱动分流 + `call_vec` 的 `VFS_CALL→dispatch→reply/SUSPEND` 建立 `Message → Route → Worker` 的可逆分发，是 08 的并发边界之后、10 的 `service_pm` 延期之前、11 的 `m_comm` 队列之前、17/23 的 `revive` 消费之前的“运行时心脏”。

```
08-worker-thread: worker[9] 的 pending/busy/allow + w_fp 双向绑定 + suspend/resume 的协程  （worker_allow 门控）
  │
  └─► 本章: main( reviving优先→transid→PM→notify→task→BDEV/CDEV/SDEV→syscall ) + get_work( ANY接收 + 端点校验 ) + unblock( PIPE→Queued vs FLOCK→Revived ) + do_reply/do_work/reply  （poll_next 的 Revived优先与 TRNS_GET_ID 路由及 call_vec 枚举分发）
         │
         ├─► 10-pm-protocol: service_pm 的 12 个 VFS_PM_* 的立即 vs 延期 vs worker_start(PM_WORK)  （PM 路由的消费方）
         ├─► 11-fs-comm: comm.c 的 m_comm 队列与 send_work 的 c_cur_reqs 窗口  （transid 路由的对端）
         ├─► 17-pipe: pipe 的 suspend/revive 的 REVIVED 置位与 unblock 的 QueuedPipe 分支  （reviving 的生产方）
         └─► 23-select: select 的 CLOCK notify → expire_timers  （notify 中 CLOCK 分支的消费方）
```

`call_vec` 的 64 调用在 `call_table.rs` 的 `VfsCallNum` 枚举中可 `match` 穷尽，为 14~31 的各 syscall handler 提供 `Lookup` 不变量；`SUSPEND` 的 `ReplyLater` 在 17/23 的 `reviving` 生产中复活用 `poll_next` 的 `reviving>0` 优先收敛。阅读顺序提示：若关心“控制面如何与数据面分离”，下一站 `10-pm-protocol.md`（`PM` 路由的 12 请求）；若关心“数据面如何与 FS 对端同步”，下一站 `11-fs-comm.md`（`transid` 路由的 `m_comm` 队列）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/main.c:54-141`（`main` 八路循环 `worker_yield+send_work+get_work+transid/PM/notify/task/BDEV/CDEV/SDEV/syscall`）、`580-633`（`get_work: reviving优先+ANY接收+端点校验`）、`263-298`（`do_work: VFS_CALL→call_vec→reply/SUSPEND`）、`187-211`（`do_reply: w_task校验+w_sendrec→c_cur_reqs--`）、`146-181`（`handle_work: CALLBACK+available→EAGAIN→use_spare`）、`638-663`（`reply/replycode: ipc_sendnb`）、`921-973`（`unblock: PIPE→QueuedPipe vs FLOCK→RevivedLock`）、`minix3/minix/servers/vfs/table.c:17-82`（`call_vec[64]` 的 `CALL(VFS_*)` 指定初始化）、`minix3/minix/servers/vfs/glo.h:24-44`（`m_in/fp/self/reviving/susp_count/err_code/who_p/who_e/call_nr/job_m_in`）、`minix3/minix/include/minix/callnr.h:68-138`（`VFS_BASE 0x100 / NR_VFS_CALLS 64 / VFS_* 64 常量`）、`minix3/minix/include/minix/com.h:91-93`（`is_notify`）、`911-912`（`VFS_TRANSID/IS_VFS_FS_TRANSID`）、`minix3/minix/include/minix/vfsif.h:79-81`（`TRNS_GET_ID/TRNS_DEL_ID`）
- 阶段文档：`01-vfs-init-main.md`（`sef_cb_init_fresh` 的 `worker_allow` 门控）、`08-worker-thread.md`（`WorkerPool` 的 `may_do_pending` 与 `steal_context`）、`02-fproc-struct.md`（`BlockedOn::Pipe/Flock` 与 `FpFlags::REVIVED`）、`10-pm-protocol.md`（`service_pm` 的 12 请求的立即 vs 延期）、`11-fs-comm.md`（`m_comm` 队列的 `c_cur_reqs` 窗口）、`99-global-concepts.md`（`VFS_BASE/NR_VFS_CALLS/TRNS` 术语）
- Rust 实现：`os/servers/vfs/src/main_loop.rs:1`（`VfsState{ reviving, current_message, current_fp_slot }` + `Route / TransIdCodec / ReplyIntent / poll_next / route_message / unblock`）、`os/servers/vfs/src/call_table.rs:1`（`VfsCallNum(64)/CallTable[64]/CallResolver/TransIdCodec`）、`os/servers/vfs/src/worker.rs:1`（`WorkerPool` 的 `may_do_pending` 与 reviving 的复活优先互证）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/12-ipc-core.md`（`sef_receive/ipc_sendnb` 的阻塞语义）
