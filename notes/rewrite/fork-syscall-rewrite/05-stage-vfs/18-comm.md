# 18-comm: VFS 进程间通信

> 本文档分析 `minix3/minix/servers/vfs/comm.c` 中的进程间通信机制。

---

## 1. 概述

### 1.1 comm.c 的角色

`comm.c` 封装了 VFS 与其他服务器之间的通信，提供三类通信接口：

| 函数 | 通信对象 | 用途 |
|------|----------|------|
| `fs_sendrec()` | 文件系统服务器（MFS等） | 文件操作请求（open/read/write/close 等） |
| `drv_sendrec()` | 设备驱动 | 块设备 I/O 请求 |
| `vm_sendrec()` | VM 服务器 | 内存映射相关请求 |

VFS 作为中间层，用户进程的文件操作请求先到达 VFS，VFS 再通过 `comm.c` 的接口转发给底层 FS 或驱动。底层 FS 处理完成后回复 VFS，VFS 再回复用户进程。

与 fork 的间接关系：`pm_fork()` 本身不调用任何通信函数，但 fork 后子进程发起文件操作时，VFS 需要通过 `fs_sendrec()` 与底层 FS 通信。

---

## 2. VFS-FS 通信

### 2.1 fs_sendrec()

```c
int fs_sendrec(endpoint_t fs_e, message *reqmp)
{
    vmp = find_vmnt(fs_e);
    self->w_sendrec = reqmp;

    if (!(vmp->m_flags & VMNT_CALLBACK) &&
        vmp->m_comm.c_cur_reqs < vmp->m_comm.c_max_reqs) {
        r = sendmsg(vmp, vmp->m_fs_e, self);  /* 直接发送 */
    } else {
        r = queuemsg(vmp);                     /* 排队等待 */
    }

    worker_wait();  /* 让出执行权，等待回复 */
    return(reqmp->m_type);
}
```

`fs_sendrec()` 是 VFS 与底层 FS 通信的核心函数，采用**异步发送 + 同步等待**模式：

1. **判断是否可直接发送**：检查 `c_cur_reqs < c_max_reqs` 且没有回调挂起
2. **直接发送**：调用 `sendmsg()`，通过 `asynsend3()` 异步发送请求，`c_cur_reqs++`
3. **排队等待**：若 FS 已达最大并发数，调用 `queuemsg()` 将请求加入等待队列
4. **让出执行权**：`worker_wait()` 使当前工作线程休眠，直到收到回复

底层实现使用 `asynsend3(AMF_NOREPLY)` 异步发送，VFS 主循环在收到回复后唤醒对应的工作线程。

### 2.2 请求队列

当 FS 已达最大并发请求数时，`queuemsg()` 将请求加入 `c_req_queue` 等待队列：

```c
static int queuemsg(struct vmnt *vmp)
{
    struct worker_thread *wp;

    wp = vmp->m_comm.c_req_queue;
    while (wp->w_next != NULL)
        wp = wp->w_next;
    wp->w_next = self;  /* 尾部插入 */
    sending++;
    return(OK);
}
```

队列是工作线程的链表（`w_next` 串联），采用 FIFO 顺序。当 FS 完成一个请求后，`fs_sendmore()` 从队列头部取出下一个请求发送：

```c
void fs_sendmore(struct vmnt *vmp)
{
    if (vmp->m_comm.c_cur_reqs >= vmp->m_comm.c_max_reqs) return;
    worker = vmp->m_comm.c_req_queue;
    vmp->m_comm.c_req_queue = worker->w_next;  /* 头部移除 */
    sendmsg(vmp, vmp->m_fs_e, worker);
}
```

---

## 3. VFS-驱动通信

### 3.1 drv_sendrec()

```c
int drv_sendrec(endpoint_t drv_e, message *reqmp)
{
    dp = get_dmap_by_endpt(drv_e);
    lock_dmap(dp);
    dp->dmap_servicing = self->w_tid;
    self->w_task = drv_e;
    self->w_drv_sendrec = reqmp;

    r = asynsend3(drv_e, self->w_drv_sendrec, AMF_NOREPLY);
    worker_wait();  /* 等待驱动回复 */

    dp->dmap_servicing = INVALID_THREAD;
    unlock_dmap(dp);
    return(r);
}
```

`drv_sendrec()` 与 `fs_sendrec()` 的关键区别：

1. **无并发控制**：设备驱动同一时刻只处理一个请求（`dmap_servicing` 保证排他），不需要 `c_max_reqs`/`c_cur_reqs` 的并发计数
2. **dmap 锁**：使用 `dmap` 的互斥锁（`lock_dmap`）而非 `comm_t` 的请求队列
3. **仅用于块设备**：`/dev/tty` 重定向会被拒绝（返回 `EIO`）

### 3.2 bdev/cdev/sdev 通信

VFS 对不同设备类型提供独立的通信接口：

| 接口 | 文件 | 设备类型 | 通信方式 |
|------|------|----------|----------|
| `bdev_*()` | bdev.c | 块设备（磁盘） | `drv_sendrec()`，同步请求-回复 |
| `cdev_*()` | cdev.c | 字符设备（终端、串口） | `asynsend3()` + 回调，异步 |
| `sdev_*()` | sdev.c | Socket 设备 | 类似 cdev，异步 |

块设备使用同步的 `drv_sendrec()`，因为块设备 I/O 通常需要等待完成。字符设备使用异步通信，因为终端输入可能长时间阻塞。

---

## 4. VFS-VM 通信

### 4.1 vm_sendrec()

```c
int vm_sendrec(message *reqmp)
{
    self->w_sendrec = reqmp;
    r = sendmsg(NULL, VM_PROC_NR, self);  /* vmp 为 NULL，无并发计数 */
    worker_wait();
    return(reqmp->m_type);
}
```

`vm_sendrec()` 是最简单的通信函数，直接发送给 VM，无需并发控制（`sendmsg` 的 `vmp` 参数为 NULL，不递增 `c_cur_reqs`）。

VFS 与 VM 的通信场景包括：
- `vm_vfs_procctl_handlemem()` — 进程退出时通知 VM 释放内存映射
- `vm_mmap()` / `vm_munmap()` — 内存映射/取消映射
- `vm_set_priv()` — 设置进程特权级

与 fork 的关系：fork 本身不触发 VFS-VM 通信。但 fork 后子进程 exit 时，VFS 通过 `vm_vfs_procctl_handlemem()` 通知 VM 释放子进程的内存映射。

---

## 5. 通信与并发控制

### 5.1 comm_t 结构体

```c
typedef struct {
    int c_max_reqs;                     /* FS 可同时处理的最大请求数 */
    int c_cur_reqs;                     /* FS 当前正在处理的请求数 */
    struct worker_thread *c_req_queue;  /* 等待发送的请求队列 */
} comm_t;
```

`comm_t` 是挂载点（`vmnt`）的通信状态，嵌入在 `vmnt.m_comm` 中。每个挂载点有独立的 `comm_t`，实现**按 FS 实例的并发控制**：

- **`c_max_reqs`**：由底层 FS 在挂载时通过 `VFS_MOUNT_REPLY` 消息告知 VFS。MFS 默认值为 `1`（串行处理），其他 FS 可能支持更高的并发度。
- **`c_cur_reqs`**：`sendmsg()` 递增，收到回复时递减。当 `c_cur_reqs >= c_max_reqs` 时，新请求必须排队。
- **`c_req_queue`**：工作线程链表，FIFO 顺序。

### 5.2 请求队列管理

请求队列的完整生命周期：

1. **入队**（`queuemsg()`）：工作线程发现 `c_cur_reqs >= c_max_reqs`，将自身加入 `c_req_queue` 尾部，`sending++`
2. **等待**：工作线程调用 `worker_wait()` 让出执行权，进入休眠
3. **出队**（`fs_sendmore()`）：收到回复后 `c_cur_reqs--`，检查队列是否有等待者，取出头部发送
4. **取消**（`fs_cancel()`）：FS 卸载时，遍历队列取消所有挂起请求

`sending` 全局变量跟踪当前等待发送的请求总数，VFS 主循环通过 `if (sending > 0) send_work()` 主动尝试发送排队的请求。

---

## 6. 通信与 fork

### 6.1 pm_fork 不涉及直接通信

`pm_fork()` 的所有操作都是 VFS 内部的数据结构操作，不调用任何通信函数：

- `fproc[childno] = fproc[parentno]` — 内存复制
- `filp_count++` — 引用计数递增
- `dup_vnode()` — 引用计数递增
- `fp_pid`/`fp_endpoint`/`fp_flags` 修正 — 字段赋值

fork 不需要通知底层 FS，因为底层 FS 管理 inode 而非文件描述符。fork 增加的是 VFS 层的引用计数（`filp_count`、`v_ref_count`），底层 FS 的引用计数（`v_fs_count`）不变。

### 6.2 fork 后的间接通信

fork 后子进程发起文件操作时，VFS 通过 `fs_sendrec()` 与底层 FS 通信。关键点：**底层 FS 不知道也不需要知道 fork 的发生**。

底层 FS 只关心 inode 操作（读、写、创建、删除），不关心哪个进程在操作。VFS 的 vnode 层已经屏蔽了进程差异——子进程和父进程通过共享的 `filp` 访问同一个 vnode，底层 FS 看到的是对同一个 inode 的操作请求，无法区分来自父进程还是子进程。

这也是 Minix3 微内核架构的优势：服务器之间通过 IPC 通信，各自维护独立的状态，fork 只影响 VFS 的 `fproc`/`filp`/`vnode` 状态，不需要跨服务器协调。

### 6.3 put_vnode 时的通信

`put_vnode()` 在 `v_ref_count` 降到 0 时调用 `req_putnode()`，这是 VFS 通知底层 FS 释放 inode 的通信：

```c
r = req_putnode(vp->v_fs_e, vp->v_inode_nr, vp->v_fs_count);
```

`req_putnode()` 内部调用 `fs_sendrec()` 发送 `REQ_PUTNODE` 请求给底层 FS。底层 FS 收到后递减 inode 的引用计数，如果也降到 0 则释放 inode。

fork 后的 put_vnode 通信链路：

```
父进程 exit → put_vnode(fp_rd) → v_ref_count: 2→1（不通信）
子进程 exit → put_vnode(fp_rd) → v_ref_count: 1→0 → req_putnode()（通信）
```

---

## 7. C 源码

**文件**: `minix3/minix/servers/vfs/type.h` (comm_t)

```c
typedef struct {
  int c_max_reqs;	/* Max requests an FS can handle simultaneously */
  int c_cur_reqs;	/* Number of requests the FS is currently handling */
  struct worker_thread *c_req_queue;/* Queue of procs waiting to send a message */
} comm_t;
```
