# 10-main-loop: VFS 主循环与消息分发

> 本文档分析 `minix3/minix/servers/vfs/main.c` 中的主循环、消息接收和请求分发机制。

---

## 1. 概述

### 1.1 VFS 主循环的角色

- VFS 主循环是消息驱动的——遵循"接收请求→处理→回复"的三阶段模型。主循环通过 `sef_receive(ANY, &m_in)` 阻塞等待来自任意进程的 IPC 消息，根据消息类型分发到对应的处理函数，处理完成后通过 `ipc_sendnb()` 发送回复。这一模型与 Minix3 所有用户态服务器一致：服务器不主动发起操作，只被动响应消息。
- 与 Kernel 主循环的关键区别：Kernel 是单核中断驱动的——通过硬件中断触发处理，不存在阻塞等待；VFS 是多线程的——主线程负责接收消息，工作线程负责处理，阻塞 I/O 操作（如管道读、设备 I/O、文件系统请求）只阻塞当前工作线程，不影响其他线程。VFS 是 Minix3 中唯一使用 mthread 多线程库的服务器。
- 主循环入口点是 `main()` 函数（[main.c:54](minix3/minix/servers/vfs/main.c#L54)）。`main()` 首先调用 `sef_local_startup()` 完成 SEF 初始化，然后进入 `while (TRUE)` 无限循环。循环体依次执行 `worker_yield()` → `send_work()` → `get_work()` → 消息分发，永不返回。

---

## 2. main() 函数

### 2.1 SEF 初始化

- `sef_local_startup()`（[main.c:374](minix3/minix/servers/vfs/main.c#L374)）是 VFS 的 SEF (System Event Framework) 初始化入口。SEF 是 Minix3 的服务生命周期管理框架，负责处理服务的启动、重启和在线更新 (live update)。`sef_local_startup()` 注册了 VFS 特有的回调函数后，调用 `sef_startup()` 将控制权交给 SEF 框架，SEF 框架再回调 `sef_cb_init_fresh()` 完成真正的初始化。
- VFS 注册了两个 SEF 初始化回调：`sef_cb_init_fresh()`（[main.c:393](minix3/minix/servers/vfs/main.c#L393)）用于全新启动时初始化——从 PM 接收引导进程信息、初始化 fproc 表、创建工作线程、挂载根文件系统；`sef_cb_init_lu()`（[main.c:352](minix3/minix/servers/vfs/main.c#L352)）用于在线更新 (live update) 时初始化——执行默认状态转移，并根据更新前的准备状态决定是否重建工作线程。两者区别在于：fresh 是从零构建，lu 是在已有状态上恢复。

### 2.2 根文件系统初始化

- `do_init_root()`（[main.c:501](minix3/minix/servers/vfs/main.c#L501)）在工作线程上下文中执行根文件系统初始化。它先调用 `worker_allow(FALSE)` 禁止其他请求进入，然后依次挂载 Pipe 文件系统 (`mount_pfs()`) 和根文件系统 (`mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, ...)` )，最后调用 `worker_allow(TRUE)` 恢复请求处理。根文件系统使用 MFS (Minix File System) 作为文件系统类型，从引导映像中的 RAM 磁盘挂载。
- VFS 启动时挂载根文件系统的完整路径：`sef_cb_init_fresh()` 完成基本初始化后，通过 `worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)` 在一个工作线程中启动根文件系统挂载。`do_init_root()` 先挂载 PFS（Pipe File System），再调用 `mount_fs()` 将 MFS 挂载到 "/" 路径。`mount_fs()` 会向 MFS 进程发送 `VFS_FS_MOUNT` 消息，MFS 返回根 vnode 后，VFS 设置 `fp_rd` 和 `fp_wd` 指向根目录 vnode。整个挂载过程在工作线程中执行，避免阻塞主循环。

### 2.3 主循环结构

- 主循环的三阶段结构（[main.c:69-139](minix3/minix/servers/vfs/main.c#L69)）：
  1. **`worker_yield()`** — 让其他线程先运行，确保工作线程有机会完成之前的请求
  2. **`send_work()`** — 将待处理的 PM 推迟请求分发给空闲工作线程
  3. **`get_work()`** — 接收新消息，返回 TRUE 表示有新消息需处理，FALSE 表示已生成其他线程活动（如唤醒阻塞进程），需继续循环

  获取消息后，主循环根据消息来源分发：FS 回复 → `do_reply()`；PM 消息 → `service_pm()`；通知 → 各通知处理函数；设备回复 → `bdev_reply()/cdev_reply()/sdev_reply()`；普通系统调用 → `handle_work(do_work)`

- `worker_yield()` 在每次循环开始时调用，主动让出主线程的执行权，使工作线程有机会完成之前的请求并发送回复。这是协作式多线程的关键点——VFS 使用 mthread 用户态线程库，线程切换需要显式让出。如果不调用 `worker_yield()`，主线程可能独占 CPU，导致工作线程饥饿。
- `send_work()` 在 `worker_yield()` 之后调用，负责将待处理的 PM 推迟请求分发给空闲工作线程。当 PM 消息到达时，如果目标进程正忙（有工作线程在处理其请求），VFS 不能立即处理，而是将请求标记为"推迟"（设置 `FP_PENDING` 标志）。`send_work()` 检查是否有推迟的 PM 请求，若目标进程已空闲，则启动工作线程处理该推迟请求。这确保了 PM 请求与目标进程的其他操作串行化。

---

## 3. get_work() 函数

### 3.1 消息接收

- `get_work()`（[main.c:580](minix3/minix/servers/vfs/main.c#L580)）负责接收 IPC 消息。它首先检查 `reviving` 计数器——如果有被唤醒的阻塞进程（如管道读恢复），则调用 `unblock()` 恢复该进程的请求并返回。若无阻塞进程需要恢复，则调用 `sef_receive(ANY, &m_in)` 阻塞等待来自任意进程的消息。收到消息后，通过 `_ENDPOINT_P(m_in.m_source)` 提取发送者的进程槽位号，设置全局 `fp` 指针指向对应的 fproc，并进行一致性检查（验证存储的 endpoint 与消息来源是否匹配）。
- PM 消息的特殊处理：在主循环中，`get_work()` 返回后，主循环检查 `who_e == PM_PROC_NR`，若消息来自 PM，则直接调用 `service_pm()` 处理并 `continue`，不走 `call_vec[]` 分发路径。原因是 PM 消息不是用户进程的系统调用，而是 PM 通知 VFS 关于进程生命周期事件（fork、exec、exit、setuid 等）。PM 消息需要特殊处理——部分可以立即处理（如 setuid、setgid），部分需要推迟到目标进程空闲时处理（如 exec、exit），部分需要在工作线程中处理（如 reboot）。

### 3.2 事务 ID 处理

- `TRNS_GET_ID(m_in.m_type)` 用于从消息类型字段中提取事务 ID。在 VFS 与文件系统服务（如 MFS）的通信中，VFS 向 FS 发送请求后，FS 的回复消息的 `m_type` 字段中编码了事务 ID。事务 ID 的编码方式是将工作线程的线程 ID 加上 `VFS_TRANSID` 偏移后嵌入 `m_type` 的高位。通过 `IS_VFS_FS_TRANSID(transid)` 检查是否为 FS 回复消息，若是则通过 `worker_get((thread_t) transid - VFS_TRANSID)` 定位对应的工作线程。
- 事务 ID 的核心作用是请求/回复匹配。VFS 有多个工作线程，每个线程可能同时向不同的 FS 发送请求。当 FS 回复到达时，VFS 需要知道这条回复对应哪个工作线程的哪个请求。事务 ID 将线程 ID 编码进消息类型，使主循环能精确定位等待该回复的工作线程，将回复数据复制到工作线程的 `w_sendrec` 中，并通过 `worker_signal()` 唤醒该线程。没有事务 ID，VFS 无法在多线程环境下正确匹配异步回复。

### 3.3 工作线程分配

- `get_work()` 返回 FALSE 时的含义：主循环中 `if (!get_work()) continue;` 表示当前没有新消息需要主循环处理，因为 `get_work()` 已经生成了其他线程活动。具体场景是 `unblock()` 恢复了一个阻塞在管道上的进程——此时 `unblock()` 调用 `worker_start()` 启动工作线程处理 `do_pending_pipe()`，返回 FALSE 告诉主循环不需要再处理这条消息。另一种场景是 `unblock()` 恢复了文件锁请求，此时返回 TRUE，主循环继续处理该请求（通过 `call_vec[]` 分发）。

---

## 4. 消息分发

### 4.1 call_vec[] 调度表

- `call_vec[NR_VFS_CALLS]` 是系统调用号到处理函数的映射表。它是一个函数指针数组，索引为 `call_nr - VFS_BASE`，值为对应处理函数的指针。在 `do_work()` 中，通过 `call_index = job_call_nr - VFS_BASE` 计算索引，然后调用 `(*call_vec[call_index])()` 执行具体处理函数。若 `call_vec[call_index]` 为 NULL 或索引越界，返回 `ENOSYS`（系统调用未实现）。这种设计将系统调用分发从 switch-case 变为 O(1) 的数组查找。
- `call_vec[]` 定义在 [table.c](minix3/minix/servers/vfs/table.c) 中，使用 `CALL(n)` 宏进行指定初始化：`#define CALL(n) [((n) - VFS_BASE)]`，将系统调用号 `n` 映射到数组索引 `n - VFS_BASE`。数组包含约 60 个条目，覆盖文件 I/O（read/write/open/close）、目录操作（mkdir/rmdir/chdir）、文件系统管理（mount/umount/sync）、网络 socket 操作（socket/bind/connect 等）等所有 VFS 系统调用。
- fork 相关的调用号映射：fork 系统调用不经过 `call_vec[]`，而是通过 PM 消息路径处理。PM 发送 `VFS_PM_FORK` 或 `VFS_PM_SRV_FORK` 消息给 VFS，由 `service_pm()` 中的 `case VFS_PM_FORK` / `case VFS_PM_SRV_FORK` 分支处理，直接调用 `pm_fork()`。与 fork 间接相关的 `call_vec[]` 条目包括：`VFS_OPEN → do_open`（子进程继承的文件描述符）、`VFS_CLOSE → do_close`（引用计数递减）、`VFS_PIPE2 → do_pipe2`（管道创建）、`VFS_FCNTL → do_fcntl`（FD_CLOEXEC 设置）。

### 4.2 PM 消息的分流

- PM 消息不走 `call_vec[]` 的原因：PM 消息是通知型消息，不是用户进程的系统调用。`call_vec[]` 的索引基于 `VFS_BASE` 偏移的系统调用号，而 PM 消息使用独立的 `VFS_PM_*` 编号空间。更重要的是，PM 消息涉及目标进程的生命周期管理，需要与目标进程当前的操作串行化——如果目标进程正有工作线程在处理系统调用，PM 消息必须推迟到该系统调用完成后再处理。这种推迟机制在 `call_vec[]` 的简单分发模型中无法实现，因此 PM 消息需要独立的 `service_pm()` 处理路径。
- `service_pm()`（[main.c:764](minix3/minix/servers/vfs/main.c#L764)）独立处理 PM 消息，采用三种策略：**立即处理**——`VFS_PM_SETUID`、`VFS_PM_SETGID`、`VFS_PM_SETSID`、`VFS_PM_SETGROUPS` 以及 `VFS_PM_FORK`/`VFS_PM_SRV_FORK` 在主线程中直接处理并同步回复 PM；**推迟处理**——`VFS_PM_EXEC`、`VFS_PM_EXIT`、`VFS_PM_DUMPCORE`、`VFS_PM_UNPAUSE` 通过 `worker_start()` 启动工作线程处理（因为目标进程可能正忙），这些请求在工作线程中通过 `service_pm_postponed()` 执行；**工作线程处理**——`VFS_PM_REBOOT` 在独立工作线程中执行 `pm_reboot()`。

### 4.3 do_reply() 函数

- 回复消息的发送逻辑：`do_reply()`（[main.c:187](minix3/minix/servers/vfs/main.c#L187)）处理来自文件系统服务的回复。它首先通过 `find_vmnt(who_e)` 找到发送回复的 FS 对应的 vmnt 结构，然后验证回复的发送者是否是工作线程等待的 FS（`wp->w_task != who_e` 检查），将收到的消息复制到工作线程的 `w_sendrec` 中，清空 `w_sendrec` 和 `w_task`，递减 vmnt 的当前请求数 `c_cur_reqs`，最后通过 `worker_signal(wp)` 唤醒等待该回复的工作线程。若发送者不匹配或 `w_sendrec` 已为 NULL，则忽略该回复。
- 工作线程的回复发送：`do_work()` 完成系统调用处理后，调用 `reply(&job_m_out, fp->fp_endpoint, error)` 发送回复。`reply()` 函数（[main.c:638](minix3/minix/servers/vfs/main.c#L638)）将结果码设置到 `m_out->m_type`，然后通过 `ipc_sendnb(whom, m_out)` 非阻塞发送。`ipc_sendnb` 不阻塞发送者——如果接收方无法立即接收，消息被丢弃（但 VFS 通常能保证用户进程在 wait 状态）。`job_m_out` 是工作线程的线程本地输出消息缓冲区，与 `job_m_in` 配对使用。

---

## 5. handle_work() 框架

### 5.1 工作处理函数

- `handle_work(void (*func)(void))`（[main.c:146](minix3/minix/servers/vfs/main.c#L146)）是工作分发的桥梁函数。它处理两种场景：普通系统调用和来自 FS 服务的回调。对于普通系统调用，`func` 参数为 `do_work`，直接调用 `worker_start(fp, func, &m_in, FALSE)` 启动工作线程。对于来自 FS 服务的回调（`fp->fp_flags & FP_SRV_PROC`），需要额外处理：检查该 vmnt 是否已有回调在进行（`VMNT_CALLBACK`），若是则返回 `EAGAIN`；检查是否有可用工作线程，若没有也返回 `EAGAIN`；设置 `VMNT_CALLBACK` 标志，并使用备用线程（`use_spare = TRUE`）处理。
- 工作线程上下文中执行具体处理函数的机制：`worker_start()` 将 `func` 和 `&m_in` 保存到工作线程的 `w_func` 和 `w_sendrec` 字段中，然后唤醒该工作线程。工作线程被唤醒后，在 `do_work()` 的线程主循环中，先将 `m_in` 复制到线程本地的 `job_m_in`，提取 `job_call_nr`，然后通过 `call_vec[job_call_nr - VFS_BASE]()` 调用具体的系统调用处理函数。处理完成后，若结果不是 `SUSPEND`，则调用 `reply()` 发送回复。整个执行过程在工作线程的栈上完成，不阻塞主线程。

---

## 6. VFS 启动流程

### 6.1 sef_local_startup()

- SEF 初始化步骤（[main.c:374-388](minix3/minix/servers/vfs/main.c#L374)）：`sef_local_startup()` 依次注册以下回调：① `sef_setcb_init_fresh(sef_cb_init_fresh)` — 全新启动回调；② `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)` — 重启回调（使用默认的有状态重启行为）；③ `sef_setcb_init_lu(sef_cb_init_lu)` — 在线更新回调；④ `sef_setcb_lu_prepare(sef_cb_lu_prepare)` — 在线更新准备回调（检查工作线程是否空闲）；⑤ `sef_setcb_lu_state_changed(sef_cb_lu_state_changed)` — 在线更新状态变更回调（失败时重建工作线程）；⑥ `sef_setcb_lu_state_isvalid(sef_cb_lu_state_isvalid_standard)` — 标准状态验证。注册完成后调用 `sef_startup()` 启动 SEF 框架。
- VFS 注册的 SEF 事件回调及其职责：
  | 回调 | 函数 | 职责 |
  |------|------|------|
  | init_fresh | `sef_cb_init_fresh()` | 全新启动：初始化 fproc 表、从 PM 接收引导进程、创建工作线程、挂载根文件系统 |
  | init_restart | `SEF_CB_INIT_RESTART_STATEFUL` | 崩溃重启：保留状态的有状态重启（默认行为） |
  | init_lu | `sef_cb_init_lu()` | 在线更新：执行状态转移，根据准备状态重建工作线程 |
  | lu_prepare | `sef_cb_lu_prepare()` | 更新准备：检查工作线程是否空闲，若不空闲则阻止更新 |
  | lu_state_changed | `sef_cb_lu_state_changed()` | 状态变更：更新失败回滚时重建工作线程 |
  | lu_state_isvalid | 标准验证 | 验证更新状态是否有效 |

### 6.2 sef_cb_init_fresh()

- 全新启动时的初始化（[main.c:393-496](minix3/minix/servers/vfs/main.c#L393)）：`sef_cb_init_fresh()` 执行以下步骤：① 设置 `self = NULL` 和 `verbose = 0`；② 初始化 fproc 表——将所有槽位的 `fp_endpoint` 设为 `NONE`，`fp_pid` 设为 `PID_FREE`；③ 从 PM 接收引导进程信息——循环调用 `sef_receive(PM_PROC_NR, &mess)`，直到收到 `VFS_PM_ENDPT == NONE` 的终止消息，为每个引导进程设置 `fp_pid`、`fp_endpoint`、凭证和 umask；④ 同步回复 PM 确认初始化完成；⑤ 订阅设备驱动事件；⑥ 调用 `worker_init()` 创建工作线程；⑦ 初始化全局锁 `bsf_lock`；⑧ 初始化设备表 `dmap` 和 socket 表 `smap`；⑨ 映射引导映像中的服务；⑩ 初始化所有进程的 mutex、文件描述符表和目录指针；⑪ 初始化 vnode、vmnt、select、filp 结构；⑫ 启动工作线程挂载根文件系统。
- fproc 表的初始化分两个阶段：**第一阶段**（在从 PM 接收引导进程之前）——遍历 `fproc[0]` 到 `fproc[NR_PROCS-1]`，将每个槽位的 `fp_endpoint` 设为 `NONE`，`fp_pid` 设为 `PID_FREE`，标记为未使用；**第二阶段**（在所有其他初始化完成后）——再次遍历 fproc 表，为每个槽位初始化 `fp_lock` mutex、设置 `fp_worker = NULL`、将 `fp_filp[0..OPEN_MAX-1]` 全部设为 NULL、将 `fp_rd` 和 `fp_wd` 设为 NULL。两阶段设计的原因是：第一阶段只需标记槽位为空，使后续的 `okendpt()` 检查能正确工作；第二阶段在 vnode/vmnt/filp 结构初始化之后进行，确保目录指针的 NULL 值不会与未初始化的 vnode 混淆。

---

## 7. 与 fork 的关系

### 7.1 fork 消息的接收路径

```
PM 发送 VFS_PM_FORK 消息
    │
    ▼
VFS main loop
    │ get_work()
    │ 检测到消息来自 PM_PROC_NR
    │
    ▼
service_pm()
    │ 解析 fork 消息
    │ 调用 pm_fork()
```

### 7.2 fork 回复的发送路径

```
pm_fork() 完成
    │
    ▼
service_pm() 继续
    │ 构造回复消息
    │ ipc_send(PM_PROC_NR, &m_out)
```
