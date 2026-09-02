# 11 — FS 通信原语：`m_comm` 的 `c_max_reqs/c_cur_reqs/c_req_queue` 窗口与 `VFS_TRANSID` 的 `TRNS_ADD_ID` 路由

本文讲清 VFS 如何在 `c_max_reqs` 的每 FS 并发窗口、`c_cur_reqs` 的在途计数、`c_req_queue` 的 `w_next` 单链表、`sending` 的全局排队计数、`VMNT_CALLBACK` 的挂起抑制、`VFS_TRANSID 0xB01` 的 `TRNS_ADD_ID/TRNS_GET_ID/TRNS_DEL_ID` 编码、`asynsend3(AMF_NOREPLY)` 的异步投递与 `worker_wait` 的同步等待的约束下，以 `fs_sendrec` 的 `CALLBACK/窗口` 二守门与 `sendmsg` 的 `c_cur_reqs++ + TRNS_ADD_ID(w_tid+VFS_TRANSID) + w_task=dst + asynsend3` 及 `queuemsg` 的尾插 `sending++` 及 `fs_sendmore` 的头部移除 `sending--` 及 `fs_cancel` 的 `while(queue) stop` 清空建立 `Worker → FS/VM/Driver` 的窗口化异步通信，并以 `drv_sendrec` 的 `CTTY_ENDPT→EIO` 与 `dmap_servicing` 排他及 `vm_sendrec` 的 `NULL vmp` 无窗口直通为块驱动与 VM 通信提供可观测分流。

前置阅读：`06-vmnt-table.md`（`Vmnt.m_comm: comm_t` 的三字段与 `VMNT_CALLBACK` 标志）、`09-main-loop.md`（`Route::FsReply` 的 `TRNS_GET_ID` 解码与 `do_reply` 的 `c_cur_reqs--`）、`08-worker-thread.md`（`WorkerPool::wait/signal` 的 `w_event` 队列与 `SuspendToken`）、`07-tll-lock.md`（`tll_lock` 的 `EBUSY→wait` 与 `VFS` 通信的 `worker_wait` 同源）。

> 本章不讲什么：
> - `req_*` 的 `REQ_LOOKUP/READ/CREATE…` 的 35 协议面包装—— `12-request-wrappers.md`（`request.c` 全文件，`vfsif.h:41-73`）
> - 路径解析的 `lookup/advance` 与挂载点穿越—— `13-path-lookup.md`（`fs_sendrec` 的调用方）
> - `open/read/write` 的 `get_fd` 与 `filp` 锁—— `15/16`（`fs_sendrec` 的调用方）
> - `bdev/cdev/sdev_reply` 的驱动回复内部与 `grant` 机制—— `20/21/22`（`09` 的 `Bdev/Cdev/Sdev` 分支消费方，仅本章 `drv_sendrec` 的块驱动直通作对照）
> - `vm_vfs_procctl_handlemem` 的 `VM_PROCCTL` 细节与 `VMPCTL_WHO/PARAM` —— 本章公开 `vm_sendrec` 的 `NULL vmp` 直通，`VMPPARAM_HANDLEMEM` 的 `02-stage-vm/20` 交叉已标注移交
> - 内核 `asynsend3/sys_datacopy` 的 IPC 原语—— `99-global-concepts.md` + `../01-stage-kernel/12-ipc-core.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 06 的 `Vmnt{m_fs_e,m_comm,m_flags}` 与 09 的 `FsReply` 路由（`TRNS_GET_ID` 的高 16 位剥离与 `do_reply` 的 `c_cur_reqs--`），能 `grep "c_max_reqs|c_cur_reqs|c_req_queue|sending|VMNT_CALLBACK" minix3/minix/servers/vfs/comm.c` 的开发者。

### 1.1 为什么需要窗口化异步

VFS 有 9 个 worker 可并发发起文件操作，底层 FS（如 MFS）的并发能力由 `c_max_reqs` 声明（`request.c` 的 `RES_THREADED` 与 `mount` 时的 `VFS_FS_MOUNT_REPLY` 的 `max_reqs` 回带，`06` 的 `m_fs_flags` 可观测）。若 VFS 的 9 路并发直通 `asynsend3` 无窗口，`MFS` 的 `c_max_reqs=1` 的串行 FS 将被 9 路并发淹没，回复顺序与 `w_next` 链表顺序不一致，`VFS_TRANSID` 的线程路由将与 `c_req_queue` 的 FIFO 语义冲突。此窗口化与 Linux 的 `request_queue` 的 `queue_depth` 及 Redox 的 `Scheme` 的 `max_requests` 直通可对照，但 VFS 选择“**每挂载点窗口**”（`m_comm` 嵌入 `vmnt`，非全局单队列），使 `/` 的 `MFS` 窗口与 `/mnt/usb` 的 `vfat` 窗口的 `c_cur_reqs` 计数隔离。

### 1.2 三字段窗口：`max/cur/queue`

`type.h` 的 `comm_t { c_max_reqs; c_cur_reqs; c_req_queue; }`（`comm.c:41` `vmp->m_comm` 嵌入）在 `sendmsg:19 c_cur_reqs++` 投递与 `do_reply:209 c_cur_reqs--` 回收间维护 `0 ≤ cur ≤ max` 的窗口不变量。`max` 在 `mount` 时由 FS 声明（`m` 的 `RES_THREADED` 位决定 `max>1`），`cur` 在 `sendmsg` 的 `c_cur_reqs++` 与 `do_reply` 的 `c_cur_reqs--` 的配对中可观测，`queue` 在 `queuemsg:233 while(w_next)→tail→append` 的 `O(n)` 尾插与 `fs_sendmore:79 head→` `c_req_queue = worker->w_next` 的 `O(1)` 头部移除中可观测。`sending` 的全局计数（`comm.c:17` `EXTERN int sending`）统计 `queue` 中等待的请求数，`send_work:43 for(vmnt 0..NR_MNTS) fs_sendmore` 的全局扫描在 `sending==0` 时 `return` 短路（`comm.c:42 if(sending==0) return`）。

此“每 FS 窗口 + 全局 `sending` 扫表”与 Redox 的 `Scheme` 的每 scheme 独立 `VecDeque<Request>` + 全局 `executor::run_until_stalled` 的 `for(scheme) poll` 扫表同型：VFS 的 `for(vmp) fs_sendmore` 的 `send_work:44` 循环即 `executor` 的 `poll` 循环的 `FS` 侧同型。

### 1.3 `VFS_TRANSID` 的线程路由

`asynsend3` 的 `AMF_NOREPLY` 使 `fs_sendrec` 的投递与 `do_reply` 的回收解耦，`VFS` 需将异步回复路由回准确 worker。`comm.c:20 transid = w_tid + VFS_TRANSID` 的 `VFS_TRANSACTION_BASE 0xB00` 前缀（`com.h:909`）与 `21 w_sendrec->m_type = TRNS_ADD_ID(m_type, transid)` 的 `((t<<16)|(id&0xFFFF))` 高位编码（`vfsif.h:80`）及 `main:88 m_type = TRNS_DEL_ID(m_type)` 的 `((short)(t>>16))` 高位剥离（`vfsif.h:81`）及 `80 transid=TRNS_GET_ID(m_type)` 的 `&0xFFFF` 低位提取（`vfsif.h:79`）的 `TRNS_ADD/GET/DEL` 三宏在 `sendmsg` 与 `do_reply` 间闭环：`sendmsg` 的 `transid+VFS_TRANSID` 写入 `m_type` 低 16，`main` 的 `transid = TRNS_GET_ID(m_in.m_type)` 提取低 16 后 `IS_VFS_FS_TRANSID(transid)` 的 `&~0xff==0xB00` 守门区分 `FS` 回复与 `PM` 的 `0x900` / `VFS_CALL` 的 `0x100` 前缀不重叠（`09` 的 `FsReply > Pm` 优先级依赖此不重叠）。

### 1.4 三类 `sendrec` 的分化

`comm.c` 的三 `sendrec` 按对端能力分化：

- `fs_sendrec:134` 的 `find_vmnt(fs_e)→NULL→EIO` 守门与 `fp_endpoint==fs_e → EDEADLK` 的自 `FS` 调用守门（`143` `if(fs_e==fp->endpoint) return EDEADLK`）后 `!(CALLBACK)&&cur<max → sendmsg else queuemsg` 的窗口守门（`149`），`VMNT_CALLBACK` 的挂起抑制使 `handle_work:160` 的 `m_flags&CALLBACK→EAGAIN` 在单次 `fs_sendrec` 内不新增回调重入。
- `drv_sendrec:89` 的 `CTTY_ENDPT→EIO`（`99`）与 `get_dmap_by_endpt→NULL→panic` 的 `dmap` 守门后 `lock_dmap→dmap_servicing!=INVALID→panic` 的排他守门（`109`）的 `dmap_servicing = w_tid` 单服务语义，使块驱动的并发窗口恒为 1（与 `comm_t` 的 `max_reqs` 窗口的“可配置并发”对立）。
- `vm_sendrec:173` 的 `sendmsg(NULL, VM_PROC_NR, self)` 的 `vmp==NULL` 时 `sendmsg:19 if(vmp) c_cur_reqs++` 的 `NULL` 守门跳过窗口计数，使 `VM` 的 `PROCCTL` 等请求不被 `c_max_reqs` 限流。

### 1.5 阻塞的可逆：`worker_wait` 的 `w_next` 链表与 `fs_cancel` 的 `while(stop)` 清空

`fs_sendrec:160 worker_wait` 的 `w_event` 队列（`08` 的 `SuspendToken`）使投递后 worker 让出，`do_reply:210 worker_signal` 唤醒。`w_next` 的单链表在 `queuemsg:240 sending++` 的尾插与 `fs_sendmore:81 sending--` 的头部移除间维护 `sending` 的全局计数与 `queue` 的 `w_next==NULL` 尾不变式。`fs_cancel:50 while(queue!=NULL) { queue = worker->w_next; sending--; worker_stop(worker); }` 的 `while(head)` 遍历在 `vmnt_unmap_by_endpt:180` 的 `FS` 崩溃回收中使 `queue` 长度归 0 且每项 `worker_stop` 的 `w_sendrec->m_type=EIO` 的 `EIO` 注入在 `worker_stop:539` 的 `w_drv_sendrec/w_sendrec` 双守门可观测。

### 1.6 与其他 OS 的窗口化对照

- **Linux** `blk-mq` 的 `request_queue`：`Linux` 的 `queue_depth` 的 `tag_set.queue_depth` 与 `nr_requests` 的 `queue->nr_requests` 窗口及 `blk_mq_get_tag` 的 `s_bitmap` 队列及 `blk_mq_put_tag` 的 `wake_up` 同型；`VFS` 的 `c_max_reqs` 的 `queue_depth` 与 `c_cur_reqs` 的 `nr_requests` 及 `queuemsg` 的 `w_next` 尾插及 `fs_sendmore` 的头部移除及 `worker_signal` 的 `wake_up` 同型。`Linux` 的每 `request_queue` 独立窗口在 `VFS` 以每 `vmnt` 独立 `comm_t` 实现。
- **Redox** `Scheme` 的 `RequestQueue`：`Redox` 的 `Scheme::handle(Request{ id, cmd })` 的 `id` 回显与 `Response{ id, result }` 的 `id` 路由在 `VFS` 以 `TRNS_ADD_ID(id)` 的 `m_type` 高位回显及 `TRNS_GET_ID(id)` 的低位路由同型；`Redox` 的 `RequestQueue` 的 `VecDeque<Request>` 的 `push_back`/`pop_front` 在 `VFS` 以 `c_req_queue` 的 `w_next` 尾插/`pop_front` 同型；`Redox` 的 `scheme.max_requests` 的 `c_max_reqs` 窗口在 `VFS` 以 `vmnt.m_comm.c_max_reqs` 实现。
- **seL4** `seL4_Send/Recv` 的端点队列：`seL4` 的 `seL4_ReplyRecv` 的端点 `queue` 的 `tcbQueue` 单链表及 `seL4_NBSend` 的 `asynsend` 在 `VFS` 以 `c_req_queue` 的 `w_next` 单链表及 `asynsend3(AMF_NOREPLY)` 的 `non-blocking` 同型；`seL4` 的 `badge` 的 `endpoint` 多路复用在 `VFS` 以 `m_type` 的高位 `transid` 编码的多路复用同型。

共同约束是“窗口隔离与路由可测试”。Minix3 以 `comm_t` 的每挂载窗口与 `TRNS_ADD/GET/DEL` 的高位编码使 `vmnt_unmap_by_endpt` 的 `fs_cancel` 清空不影响 `VM` 的 `NULL vmp` 直通。

### 1.7 小结

`m_comm` 是 `vmnt` 的发送窗口：`max` 声明FS并发能力，`cur` 在 `sendmsg`/`do_reply` 间配对增减维持 `cur ≤ max`，`queue` 在 `queuemsg` 的 `O(n)` 尾插与 `fs_sendmore` 的头部移除间以 `w_next` 链表实现 `sending` 的全局排队计数，`CALLBACK` 的挂起抑制使回调重入在单次 `sendrec` 内不新增窗口，`VFS_TRANSID` 的 `TRNS_ADD/GET/DEL` 高位编码使 `9 worker` 的异步回复在 `main` 的 `FsReply` 优先可路由，`drv/vm_sendrec` 的 `dmap` 排他与 `NULL vmp` 直通使块驱动与 VM 的窗口可分流。下一节以 `comm.c:11-244` 全文与 `request.h` 的 `node_details` 及 `com.h:909` 的 `VFS_TRANSID` 为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `type.h:comm_t` 三字段（`comm.c:41` 嵌入）

`type.h: (comm_t { c_max_reqs: int; c_cur_reqs: int; c_req_queue: worker_thread* })` 的 `c_max_reqs` 在 `mount` 时由 `FS` 的 `RES_THREADED` 位决定（`06` 的 `m_fs_flags`），`c_cur_reqs` 在 `sendmsg:19 c_cur_reqs++` 与 `do_reply:209 c_cur_reqs--` 的配对中可观测，`c_req_queue` 在 `queuemsg:229 if(queue==NULL) queue=self else while(w_next)→append` 的 `sending++` 尾插中可观测。

### 2.2 `sendmsg:11-32` 的 `c_cur_reqs++ + TRNS_ADD_ID + asynsend3`

`sendmsg:19 if(vmp) c_cur_reqs++` 的 `vmp==NULL` 守门使 `vm_sendrec:183 sendmsg(NULL,VM)` 不增 `cur`，`20 transid = w_tid + VFS_TRANSID` 的 `VFS_TRANSACTION_BASE 0xB00` 前缀（`com.h:909`）与 `21 m_type = TRNS_ADD_ID(m_type,transid)` 的 `((t<<16)|(id&0xFFFF))` 高位编码（`vfsif.h:80`）及 `22 w_task=dst` 的 `task` 绑定，`23 asynsend3(dst,w_sendrec,AMF_NOREPLY)` 的 `non-blocking` 在 `23 r=asynsend3` 的 `printf+stacktrace` 的 `return(r)` 守门中可观测。

### 2.3 `send_work:37-45` 的全局扫表

`send_work:42 if(sending==0) return` 的 `sending` 短路（`comm.c:17` `glo.h:17 EXTERN int sending`）与 `43 for(vmp=vmnt; vmp<vmnt+NR_MNTS; vmp++) fs_sendmore(vmp)` 的 `NR_MNTS 8` 固定扫表，使 `vmnt[8]` 的每挂载窗口在 `main:72 send_work` 的 `worker_yield` 后可观测（`09` 的 `main` 八路循环的 `send_work` 调用点）。

### 2.4 `fs_cancel:50-61` 的 `while(queue) stop` 清空

`fs_cancel:55 while((worker=vmp->m_comm.c_req_queue)!=NULL) { c_req_queue=worker->w_next; worker->w_next=NULL; sending--; worker_stop(worker); }` 的 `while(head)` 遍历在 `vmnt_unmap_by_endpt:180` 的 `FS` 崩溃回收中使 `queue` 长度归 0 且每项 `worker_stop:539` 的 `w_sendrec->m_type=EIO` 的 `EIO` 注入在 `08` 的 `SuspendToken` 可观测。

### 2.5 `fs_sendmore:66-84` 的窗口守门与头部移除

`fs_sendmore:71 if(m_fs_e==NONE) return` 的 `NO_DEV` 空闲 `vmnt`（`06` 的 `m_dev==NO_DEV`）守门与 `72 if(queue==NULL) return` 的 `NULL` 队列守门与 `74 if(cur>=max) return` 的窗口满守门与 `76 if(VMNT_CALLBACK) return` 的回调抑制守门，`79 c_req_queue=worker->w_next; 80 worker->w_next=NULL; 81 sending--; assert(sending>=0); 83 sendmsg(vmp, m_fs_e, worker)` 的 `pop_front` 的 `head→w_next` 更新与 `sending--` 的全局递减配对 `queuemsg` 的 `sending++`。

### 2.6 `drv_sendrec:89-129` 的 `CTTY→EIO` 与 `dmap` 排他

`drv_sendrec:99 if(drv_e==CTTY_ENDPT) return EIO` 的 `/dev/tty` 非块设备守门（`const.h:52 CTTY_ENDPT==VFS_PROC_NR`）与 `104 get_dmap_by_endpt(drv_e)→NULL→panic invalid` 的 `dmap` 守门后 `107 lock_dmap(dp); 108 if(servicing!=INVALID) panic inconsistency; 110 servicing=w_tid` 的排他守门（`19` 的 `DmapEntry::servicing`），`111 w_task=drv_e; 112 w_drv_sendrec=reqmp` 的 `w_drv_sendrec` 绑定与 `114 asynsend3(AMF_NOREPLY)→OK→worker_wait()` 的让出及 `118 printf→w_drv_sendrec=NULL` 的 `EIO` 注入分支，`124 assert(w_drv_sendrec==NULL); 125 servicing=INVALID; 126 w_task=NONE; 127 unlock_dmap` 的配对清理。

### 2.7 `fs_sendrec:134-168` 的 `CALLBACK/窗口` 二守门

`fs_sendrec:139 find_vmnt(fs_e)→NULL→EIO` 的 `m_fs_e==fs_e && m_dev!=NO_DEV` 命中守门（`06`）与 `143 if(fs_e==fp->fp_endpoint) return EDEADLK` 的自 `FS` 调用守门（`19` 的 `handle_work:156 FP_SRV_PROC` 的 `CALLBACK→EAGAIN` 与之互证的同一 `FS` 回调死锁防护）及 `148 w_sendrec=reqmp` 的 `assert(w_sendrec==NULL)` 前置（`135`），`149 if(!(CALLBACK) && cur<max → sendmsg else queuemsg)` 的二守门与 `160 worker_wait` 的让出及 `164 r=reqmp->m_type; 165 if(ERESTART) r=EIO` 的 `ERESTART` 抑制（`ERESTART` 为 `VFS` 内部 `SUSPEND→revive` 的 `reviving` 优先路径的哨兵，不应透传至 `read/write` 的 `ReplyIntent`）。

### 2.8 `vm_sendrec:173-194` 的 `NULL vmp` 直通

`vm_sendrec:183 sendmsg(NULL,VM_PROC_NR,self)` 的 `vmp==NULL` 时 `sendmsg:19 if(vmp) c_cur_reqs++` 的 `NULL` 守门跳过窗口计数，使 `VM` 的 `PROCCTL` 等请求不被 `c_max_reqs` 限流，`184 w_next=NULL` 的尾不变式与 `189 worker_wait` 的让出及 `193 return(reqmp->m_type)` 的 `ERESTART` 不抑制（`VM` 的 `ERESTART` 不经 `VFS` 的 `SUSPEND` 路径）。

### 2.9 `vm_vfs_procctl_handlemem:199-218` 的 `!self→EFAULT`

`vm_vfs_procctl_handlemem:206 if(!self) return EFAULT` 的 `main` 线程不可挂起守门（`09` 的 `main:68` `self==NULL` 的主循环与 `worker_main:248 fp=self->w_fp` 的 `self != NULL` 的 `worker` 上下文分化）与 `210 m_type=VM_PROCCTL; 211 VMPCTL_WHO=ep; 212 PARAM=VMPPARAM_HANDLEMEM; 213 M1=mem; 214 LEN=len; 215 FLAGS=flags` 的 `VM_PROCCTL` 帧封装及 `217 vm_sendrec(&m)` 的 `vm_sendrec` 复用。

### 2.10 `queuemsg:223-244` 的尾插 `sending++`

`queuemsg:229 if(queue==NULL) queue=self else { queue=c_req_queue; while(w_next) queue=w_next; queue->w_next=self; }` 的 `NULL→head vs while→tail` 分化与 `240 w_next=NULL` 的尾不变式及 `241 sending++` 的全局递增配对 `fs_sendmore:81 sending--`。

### 2.11 `request.h` 的 `node_details/lookup_res` 与 `com.h:909` 的 `VFS_TRANSID`

`request.h:12 node_details { fs_e,inode_nr,fmode,fsize,uid,gid,dev }` 的 `REQ_NEWNODE` 响应与 `25 lookup_res { fs_e,inode_nr,fmode,fsize,uid,gid,dev,char_processed,symloop }` 的 `REQ_LOOKUP` 响应在 `12-request-wrappers.md` 的 `req_lookup` 的 `lookup_res` 对端可观测，`com.h:909 VFS_TRANSACTION_BASE 0xB00` 的 `0xB00` 前缀使 `VFS_TRANSID 0xB01` 的 `+1` 偏移在 `sendmsg:20 transid=w_tid+0xB01` 可观测。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `comm.c:19` 的 `c_cur_reqs++` 与 `w_next` 裸链表，而是吸收 Redox/Linux 的队列模型后做取舍。以下决策对应 `.design/11-design.v1.md` D1-D6。

### D1 每挂载窗口与全局 `sending`：`FsComm` 的 `VecDeque` 窗口

- **C**：`comm_t { cur/max/queue }` 的 `queue` 为 `w_next` 单链表尾插 `O(n)`（`queuemsg:233 while(w_next)`），`sending` 为 `int` 全局。
- **Rust**：`FsComm { max_reqs: usize, cur_reqs: usize, queue: VecDeque<SlotId> }` 的 `VecDeque` 尾插 `push_back` `O(1)` + 头部移除 `pop_front` `O(1)` + `len()` 的 `cur ≤ max` 窗口守门；`GlobalComm { vmnts: [FsComm;8], sending: usize }` 的 `sending == sum(queue.len())` 不变量在 `debug_assert` 可观测。
- **为什么**：`w_next` 裸链表的 `O(n)` 尾插在 `NR_WTHREADS 9` 的小上界可接受，但 `VecDeque` 的 `len` 可观测使 `send_work` 的 `sending==0 → return` 短路在 Rust 以 `global.is_empty()` 显式。
- **备选**：保留 `w_next` 链表；否决——`VecDeque` 的 `len` 可观测使 `fs_cancel` 的 `while(queue) pop_front` 的 `sending--` 配对在单元测试可计数。

### D2 `VFS_TRANSID` 类型化：`TransId` 的 `TRNS_ADD/GET/DEL` 封装

- **C**：`TRNS_ADD_ID(t,id) ((t<<16)|(id&0xFFFF))` 的裸宏（`vfsif.h:80`）在 `sendmsg:21` 的 `m_type = TRNS_ADD_ID(m_type, transid)` 与 `main:88` 的 `TRNS_DEL_ID` 散落。
- **Rust**：`TransId { raw: u32 }` 的 newtype + `TransId::encode(slot: SlotId) -> u32` 的 `VFS_TRANSID 0xB01 + slot` + `decode(raw)->Option<SlotId>` 的 `IS_VFS_FS_TRANSID` 前缀守门（`&~0xff==0xB00`）+ `strip(m_type)->u32` 的 `>>16` 高位剥离；`TransIdCodec` trait 的 `Vfs(0xB00)` 与 `Test(0xC00)` 双实现在 `09` 已落地，本章的 `sendmsg` 复用 `TransId::encode` 的 `TRNS_ADD_ID` 封装。
- **为什么**：`0xFFFF` 掩码的散落在 Rust 以 `TransId` 的 `newtype` 使 `m_type` 的低 16 `transid` 与高 16 `call_nr` 不混淆。

### D3 三 `sendrec` 分化：`FsTransport` trait 的 `QueuePolicy`

- **C**：`fs_sendrec` 的 `CALLBACK/窗口` 二守门 vs `drv_sendrec` 的 `CTTY→EIO + dmap排他` vs `vm_sendrec` 的 `NULL vmp` 直通三函数分立。
- **Rust**：`trait FsTransport { fn send_fs(&mut self, vmp: VmntId, req: Message) -> Result<CommId, CommError>; fn send_drv(&mut self, drv: Endpoint, req: Message) -> Result<CommId, CommError>; fn send_vm(&mut self, req: Message) -> Result<CommId, CommError>; }` 的 `BlockingTransport`（`worker_wait` 真等待）与 `MockTransport`（`Vec<Message>` 记录不等待）双实现；`QueuePolicy` 的 `FifoPolicy`（`queuemsg` 尾插）与 `LifoPolicy`（`push_front`）双实现在 `FsComm::queue_policy` 可测试（`sending++` 的 `while` 清空顺序在 `LIFO` 时逆序）。
- **为什么**：`drv_sendrec` 的 `dmap_servicing` 排他在 Rust 以 `DmapTable::try_lock(drv) -> Result<Token, DmapError>` 的 `Token` 所有权显式，`VM` 的 `NULL vmp` 直通在 `send_vm` 的 `Option<VmntId>::None` 分支显式。
- **备选**：保留三函数；否决——`FsTransport` 的 `trait` 使 `fs_sendrec` 的窗口守门在测试中以 `MockTransport` 的 `cur_reqs` 超限样本覆盖。

### D4 `VMNT_CALLBACK` 抑制：`VmntFlags::CALLBACK` 的 `try_send` 守门

- **C**：`fs_sendrec:149 if(flags & CALLBACK) → queuemsg` 的抑制与 `handle_work:160 if(CALLBACK) → EAGAIN` 的死锁防护重复。
- **Rust**：`FsComm::can_send(flags: VmntFlags, cur: usize, max: usize) -> Result<(), CommError>` 的 `if(flags.contains(CALLBACK)) → Err(Callback)` 守门在 `send_fs` 的 `?` 传播，使抑制在 `FsComm` 层可测试（`CALLBACK→Err` 的 `is_err()` 样本）。
- **为什么**：`CALLBACK` 的 `EAGAIN` 在 `handle_work` 与 `fs_sendrec` 两处重复，Rust 以 `can_send` 的单一守门消除重复。

### D5 `ERESTART` 抑制与 `EDEADLK` 自调用守门

- **C**：`fs_sendrec:165 if(r==ERESTART) r=EIO` 的内部哨兵抑制与 `143 if(fs_e==fp->endpoint) return EDEADLK` 的自 `FS` 调用守门。
- **Rust**：`CommError::Restart` 的 `to_errno() → EIO` 映射（`11` 的 `VmError::Restart→EIO` 同型）与 `CommError::Deadlock` 的 `to_errno() → EDEADLK` 使 `ERESTART` 的抑制在 `fs_sendrec` 的 `match reqmp.m_type { ERESTART => EIO }` 显式。

### D6 测试与 `fproc_light` 缺口

- **C**：`misc.c:55 SI_PROC_TAB` 的 `fproc_light` 三字段在 `do_getsysinfo` 的 `sys_datacopy` 可观测。
- **Rust**：`FsComm` 的 `max_reqs` 窗口在 `FsComm::new(max)` 的 `max==1` 的 `MFS` 串行样本与 `max==4` 的并发样本的 `can_send` 可测试；`sending` 的全局 `0→queue→sendmore→0` 循环在 `GlobalComm::send_work` 的 `for(vmnt) fs_sendmore` 可测试（`sending==0→return` 短路）。
- **缺口**：`vm_vfs_procctl_handlemem` 的 `VM_PROCCTL` 帧封装在 `fs_comm.rs:vm_procctl_handlemem` 的 `MessageM7` `VMPPARAM_HANDLEMEM` 占位；`m_comm` 的 `c_max_reqs` 的 `mount` 时 `RES_THREADED` 声明在 `06` 的 `m_fs_flags` 可观测。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-4 全局聚合 | `GlobalComm { sending, vmnts: [FsComm;8] }` | `fs_comm.rs:GlobalComm` + 本文档 D1 + 11 正文 1.5 |
| A-6 锁降级 | `FsComm.queue: VecDeque` 替代 `w_next` 链表 | `fs_comm.rs:FsComm.queue` + 本文档 D1 + 11 正文 1.5 |
| A-8 64 位 | `TransId(u32)` 的 `VFS_TRANSID` `u32` | `fs_comm.rs:TransId` + 本文档 D2 + 11 正文 1.3 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── fs_comm.rs          — GlobalComm{ vmnts:[FsComm;8], sending:usize } + FsComm{ max/cur/queue:VecDeque } + TransId/TRNS_ADD/GET/DEL + FsTransport trait (Blocking vs Mock + Fifo vs Lifo) + vm_procctl_handlemem
├── vmnt.rs             — Vmnt.m_comm: FsComm 嵌入 + VmntFlags::CALLBACK/MOUNTING + m_fs_e/m_dev 双哨兵（06）
├── worker.rs           — WorkerPool::wait/signal 的 w_event 队列与 sQueue 的 sending 互证（08）
├── main_loop.rs        — TransIdCodec(Vfs 0xB00) 与 do_reply 的 c_cur_reqs-- 对端（09）
└── fproc.rs            — FpFlags::REVIVED 与 reviving 的 unblock 对端（02）
```

### 4.2 `fs_comm.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `comm_t` | `type.h:comm_t` | `fs_comm.rs:FsComm { max_reqs, cur_reqs, queue:VecDeque<SlotId> }` | `max` 声明并发窗口，`cur` 配对增减 |
| `sending` | `glo.h:17` | `GlobalComm { sending }` | `sending==sum(queue.len())` 不变量 |
| `VFS_TRANSACTION_BASE` | `com.h:909` | `fs_comm.rs:TRANSACTION_BASE:u32=0xB00` | `~0xff` 前缀守门 |
| `VFS_TRANSID` | `com.h:911` | `TransId::VFS_TRANSID:u32=0xB01` | `w_tid+VFS_TRANSID` 编码 |
| `TRNS_ADD/GET/DEL` | `vfsif.h:79-81` | `TransId::add/get/del` | `((t<<16)|(id&0xFFFF))` 高位编码 |
| `sendmsg` | `comm.c:11` | `GlobalComm::sendmsg(vmp, dst, req) ->Result<TransId, CommError>` | `c_cur_reqs++ + transid+VFS_TRANSID + w_task=dst + asynsend3` |
| `queuemsg` | `comm.c:223` | `FsComm::enqueue(slot) -> CommId` | `queue.push_back(slot); sending++` |
| `fs_sendmore` | `comm.c:66` | `GlobalComm::fs_sendmore(vmp) ->Option<SlotId>` | `cur<max && !CALLBACK && queue非空 → pop_front + sendmsg` |
| `send_work` | `comm.c:37` | `GlobalComm::send_work() ->usize` | `if sending==0 return 0; for(vmnt) fs_sendmore→sendmsg 计数` |
| `fs_cancel` | `comm.c:50` | `GlobalComm::fs_cancel(vmp) ->usize` | `while(queue) { pop; sending--; stop(EIO) }` 计数返回 |
| `fs_sendrec` | `comm.c:134` | `FsTransport::send_fs(vmp, req) ->Result<Reply,CommError>` | `CALLBACK→Err + cur<max→sendmsg else enqueue + wait` |
| `drv_sendrec` | `comm.c:89` | `FsTransport::send_drv(drv, req)` | `CTTY→EIO + dmap排他 + w_task=drv + asynsend→wait` |
| `vm_sendrec` | `comm.c:173` | `FsTransport::send_vm(req)` | `sendmsg(NULL,VM)` 的 `vmp==None` 直通 |
| `vm_vfs_procctl_handlemem` | `comm.c:199` | `FsTransport::procctl_handlemem(ep,mem,len,flags)` | `!self→EFAULT + VM_PROCCTL帧 + vm_sendrec` |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 窗口 `cur ≤ max` | `FsComm::can_send` | `cur < max → Ok else Queue` | `comm.c:74` |
| 空 `queue==None → cur==0` | `FsComm::is_idle` | `queue.is_empty() && cur==0` | `comm.c:72` |
| 全局 `sending==sum(queue.len())` | `GlobalComm` | `enqueue→sending++ / dequeue→sending--` | `comm.c:241/81` |
| 高位编码 `TRNS_ADD→GET→DEL` 往返 | `TransId` | `del(add(t,id))==t` | `vfsif.h:79-81` |
| 回调抑制 `CALLBACK→Queue` | `FsComm::can_send` | `flags.contains(CALLBACK)→Err` | `comm.c:76` |
| 尾不变式 `w_next==NULL` | `FsComm::queue` | `VecDeque` 的 `push_back` 尾为 `None` | `comm.c:240` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **143 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 + `ipc/dispatcher` 16 + `fs_comm` 19 = 137 → `cargo test` 实测 143；`minix-types` 108 独立）。
> 本章直接影响 `19` 项新增（`transid_add_get_del/queue_tail/queue_head/sendmsg_cur/send_work_scan/fs_cancel_while/fs_sendrec_window/drv_ctty/drv_dmap/vm_null/ere_restart/edeadlk/callback/sending_zero/transid_two_impls/queue_two_impls/fs_transport_two_impls/vm_procctl`），`minix-vfs --lib` 总计 124 → 143。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_transid_add_get_del` | `vfsif.h:79-81` | `add(t,id)→get→id, del→t` 高位往返 | `fs_comm.rs` |
| `test_transid_two_impls` | `com.h:909` | `VfsTransIdCodec` 0xB00 vs `Test 0xC00` 的 `is_fs` 行为差异 | `fs_comm.rs` |
| `test_fs_comm_window` | `comm.c:74` | `cur<max→Ok, cur==max→Queue` 的 `can_send` | `fs_comm.rs` |
| `test_queue_tail_head` | `comm.c:229/79` | `enqueue tail→push_back, dequeue head→pop_front` 的 FIFO | `fs_comm.rs` |
| `test_sendmsg_cur_inc` | `comm.c:19` | `sendmsg → cur++` 的配对 | `fs_comm.rs` |
| `test_send_work_scan` | `comm.c:43` | `sending==0→0, sending>0→for(vmnt) fs_sendmore` 的扫表 | `fs_comm.rs` |
| `test_fs_cancel_while` | `comm.c:55` | `while(queue) pop→sending--→stop(EIO)` 的 `sending--` 计数 | `fs_comm.rs` |
| `test_fs_sendrec_window` | `comm.c:149` | `!CALLBACK && cur<max→sendmsg else enqueue` 二守门 | `fs_comm.rs` |
| `test_drv_ctty` | `comm.c:99` | `CTTY_ENDPT→EIO` | `fs_comm.rs` |
| `test_drv_dmap_busy` | `comm.c:108` | `dmap_servicing!=INVALID→panic→Err(Busy)` 的 `TryLock` | `fs_comm.rs` |
| `test_vm_null` | `comm.c:183` | `vmp==NULL→cur不增` 的 `NULL` 直通 | `fs_comm.rs` |
| `test_ere_restart` | `comm.c:165` | `ERESTART→EIO` 的抑制 | `fs_comm.rs` |
| `test_edeadlk` | `comm.c:143` | `fs_e==fp_endpoint→EDEADLK` | `fs_comm.rs` |
| `test_callback` | `comm.c:76` | `CALLBACK→Err(Callback)` | `fs_comm.rs` |
| `test_sending_zero` | `comm.c:42` | `sending==0→send_work==0` 的短路 | `fs_comm.rs` |
| `test_transid_two_impls_11` | `com.h:909` | `TransIdCodec` trait `Vfs vs Test` 的 `encode 0xB01 vs 0xC01` 差异 | `fs_comm.rs` |
| `test_queue_two_impls` | `comm.c:223` | `FifoQueue` tail vs `LifoQueue` head 的 `push` 行为差异 | `fs_comm.rs` |
| `test_fs_transport_two_impls` | `comm.c:134` | `FsTransport` trait `Blocking vs Mock` 的 `sent_fs` 记录差异 | `fs_comm.rs` |
| `test_vm_procctl_handlemem` | `comm.c:199` | `!self→EFAULT` 的主线程守门 vs 有 `SlotId` 时 `TransId` 编码 | `fs_comm.rs` |

测试策略：`TransId` 的 `ADD/GET/DEL` 以 `add(0x1234,0xB01)→get 0xB01 & del 0x1234` 往返样本覆盖；`FsComm` 的窗口以 `max=1 cur=1→Queue` 与 `max=4 cur=1→Send` 两样本覆盖；`queue` 以 `enqueue 1,2 → dequeue 1` 的 FIFO 样本覆盖；`sendmsg` 以 `cur 0→1` 的递增样本覆盖；`fs_cancel` 以 `queue 2→while pop 2→sending 0` 的清空样本覆盖；`TransId` 与 `Queue` 的双 trait 以 `Vfs vs Test` 的 `0xB01 vs 0xC01` 与 `Fifo head vs Lifo tail` 的 `dyn` 行为差异样本覆盖。

---

## 6 过渡

本篇在 `09-main-loop` 的 `Route::FsReply` 的 `TRNS_GET_ID` 解码与 `do_reply` 的 `c_cur_reqs--` 对端之前，`fs_sendrec` 的 `c_cur_reqs++` 投递与 `m_comm` 的 `c_max_reqs` 窗口及 `VFS_TRANSID` 的高位编码及 `sending` 的全局排队计数及 `VMNT_CALLBACK` 的挂起抑制建立 `Worker→FS/VM/Driver` 的窗口化异步通信，是 09 的 `FsReply` 优先可路由之前、12 的 `req_*` 包装的对端之前、14~31 的 `fs_sendrec` 调用方之前、17/23 的 `revive` 消费之前的“请求队列”前提。

```
09-main-loop: main( FsReply>PM>notify>task>BDEV/CDEV/SDEV>syscall ) + do_reply(c_cur_reqs--)  （TRNS_GET_ID 路由）
  │
  └─► 本章: m_comm{ max/cur/queue:VecDeque + sending } + sendmsg(c_cur++ + TRNS_ADD_ID + w_task + asynsend) + queuemsg(push_back) + fs_sendmore(pop_front) + fs_cancel(while stop) + drv/vm_sendrec  （c_max/c_cur/queue 的 VecDeque 窗口与 VFS_TRANSID 的高位编码）
         │
         ├─► 12-request-wrappers: request.c 的 req_lookup 等 35 REQ_* 的 FS 端包装  （本章 fs_sendrec 的 REQ 包装对端）
         ├─► 14~31: open/read/write 等的 req_* 调用  （本章 fs_sendrec 的调用方）
         ├─► 17-pipe: pipe 的 suspend 的 reviving 生产方  （本章 fs_sendrec 的 SUSPEND 消费者）
         └─► 23-select: select 的 CLOCK notify 定时器  （本章 VMNT_CALLBACK 抑制的另一消费方）
```

`m_comm` 的 `c_max_reqs` 在 `mount` 时由 `FS` 的 `RES_THREADED` 位声明，为 `06` 的 `m_fs_flags` 可观测；`sending` 的全局 `0→queue→sendmore→0` 循环在 `09` 的 `send_work` 扫表中可观测。阅读顺序提示：若关心“`REQ_*` 如何被包装为 `fs_sendrec`”，下一站 `12-request-wrappers.md`；若关心“`open` 如何触发 `REQ_CREATE`”，下一站 `15-open-close.md`。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/comm.c:11-244`（`sendmsg:11` 的 `c_cur_reqs++ + TRNS_ADD_ID + asynsend3`、`send_work:37` 的 `sending==0` 短路与 `NR_MNTS` 扫表、`fs_cancel:50` 的 `while(queue) stop`、`fs_sendmore:66` 的 `窗口+CALLBACK` 守门与 `pop_front + sendmsg`、`drv_sendrec:89` 的 `CTTY→EIO` 与 `dmap_servicing` 排他、`fs_sendrec:134` 的 `CALLBACK/窗口` 二守门与 `ERESTART→EIO`、`vm_sendrec:173` 的 `NULL vmp` 直通、`vm_vfs_procctl_handlemem:199` 的 `!self→EFAULT`、`queuemsg:223` 的 `sending++` 尾插）、`minix3/minix/servers/vfs/type.h:comm_t`（`c_max_reqs/c_cur_reqs/c_req_queue` 三字段）、`minix3/minix/servers/vfs/vmnt.h:7-21`（`vmnt.m_comm` 嵌入与 `VMNT_CALLBACK 02`）、`minix3/minix/include/minix/com.h:909-912`（`VFS_TRANSACTION_BASE 0xB00 / VFS_TRANSID 0xB01 / IS_VFS_FS_TRANSID ~0xff`）、`minix3/minix/include/minix/vfsif.h:79-81`（`TRNS_GET_ID/ADD/DEL` 的 `&0xFFFF/<<16/>>16`）
- 阶段文档：`06-vmnt-table.md`（`Vmnt.m_comm: FsComm` 嵌入与 `VMNT_CALLBACK` 标志）、`09-main-loop.md`（`Route::FsReply` 的 `TRNS_GET_ID` 解码与 `do_reply` 的 `c_cur_reqs--`）、`08-worker-thread.md`（`WorkerPool::wait/signal` 的 `w_event` 队列与 `SuspendToken`）、`07-tll-lock.md`（`tll_lock` 的 `EBUSY→wait` 与 `VFS` 通信的 `worker_wait` 同源）、`12-request-wrappers.md`（`request.c` 的 `REQ_*` 包装与 `node_details`）、`99-global-concepts.md`（`comm_t` 术语与 `sending` 计数）
- Rust 实现：`os/servers/vfs/src/fs_comm.rs:1`（`GlobalComm{ vmnts:[FsComm;8], sending }` + `FsComm{ max/cur/queue:VecDeque }` + `TransId/TRNS_ADD/GET/DEL + FsTransport trait (Blocking vs Mock + Fifo vs Lifo)`）、`os/servers/vfs/src/vmnt.rs:1`（`Vmnt.m_comm: FsComm` 嵌入）、`os/servers/vfs/src/main_loop.rs:1`（`TransIdCodec` 的 `Vfs 0xB00` vs `Test 0xC00` 对端）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/12-ipc-core.md`（`asynsend3(AMF_NOREPLY)` 的异步投递）

