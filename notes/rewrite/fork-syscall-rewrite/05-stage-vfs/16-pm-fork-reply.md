# 16-pm-fork-reply: VFS-PM fork 回复协议

> 本文档分析 `minix3/minix/servers/vfs/main.c` 中 `service_pm()` 发送 fork 回复的协议。

---

## 1. 概述

### 1.1 回复协议的角色

`pm_fork()` 处理完成后，VFS 必须回复 PM。这是 PM→VFS 请求-回复协议的一部分：

- PM 发送 `VFS_PM_FORK` 请求给 VFS
- VFS 处理完成后，发送 `VFS_PM_FORK_REPLY` 回复给 PM
- PM 收到回复后才继续 fork 流程

回复使用 `ipc_send()` 而非 `ipc_sendrec()`，因为这是**异步回复**——VFS 不需要等待 PM 的再次回复。PM 在发送 `VFS_PM_FORK` 后阻塞等待回复，`ipc_send()` 将回复投递到 PM 的消息队列，PM 被唤醒后继续执行。

---

## 2. VFS_PM_FORK 回复

### 2.1 回复消息构造

```c
pm_fork(pproc_e, proc_e, child_pid);
m_out.m_type = VFS_PM_FORK_REPLY;
m_out.VFS_PM_ENDPT = proc_e;
```

回复消息包含两个字段：

| 字段 | 值 | 说明 |
|------|----|------|
| `m_type` | `VFS_PM_FORK_REPLY` | 消息类型，PM 据此识别这是 fork 回复 |
| `VFS_PM_ENDPT` | `proc_e` | 子进程的 endpoint，PM 用于后续操作 |

注意：回复消息不包含错误码。`pm_fork()` 的返回类型是 `void`，不会失败——如果参数无效，它会直接 `panic()`。

### 2.2 回复发送

```c
r = ipc_send(PM_PROC_NR, &m_out);
if (r != OK)
    panic("service_pm: ipc_send failed: %d", r);
```

`ipc_send()` 是非阻塞的发送操作，将消息投递到 PM 的消息队列后立即返回。

与 `service_pm_postponed()` 中的 `ipc_send` 的区别：两者使用相同的发送机制，但执行上下文不同。`service_pm()` 在主线程中执行，处理不需要阻塞的 PM 请求（如 fork）；`service_pm_postponed()` 在工作线程中执行，处理可能需要阻塞的 PM 请求（如 exit、exec）。

### 2.3 PM 端的接收

PM 在主循环中接收 `VFS_PM_FORK_REPLY`：

```c
/* pm/main.c */
case VFS_PM_FORK_REPLY:
    /* Schedule the newly created process ... */
    r = OK;
    if (rmp->mp_scheduler != KERNEL && rmp->mp_scheduler != NONE) {
        r = sched_start_user(rmp->mp_scheduler, rmp);
    }
```

PM 收到回复后的后续操作：
1. **调度子进程**：调用 `sched_start_user()` 让调度器将子进程加入可运行队列
2. **处理调度失败**：如果调度失败，PM 需要回滚 fork（清理子进程的 Kernel/VM/VFS 状态）
3. **回复用户进程**：fork 系统调用返回子进程的 PID（给父进程）或 0（给子进程）

PM 等待 VFS 回复是 fork 流程的同步点——PM 必须确认 VFS 已完成 fproc 复制后，才能继续后续操作。

---

## 3. VFS_PM_SRV_FORK 回复

### 3.1 回复类型区别

```c
m_out.m_type = VFS_PM_FORK_REPLY;

if (call_nr == VFS_PM_SRV_FORK) {
    m_out.m_type = VFS_PM_SRV_FORK_REPLY;
    pm_setuid(proc_e, reuid, reuid);
    pm_setgid(proc_e, regid, regid);
}
```

PM 通过 `m_type` 区分普通 fork 和服务 fork 的回复：

| 回复类型 | 请求类型 | 说明 |
|----------|----------|------|
| `VFS_PM_FORK_REPLY` | `VFS_PM_FORK` | 普通用户进程 fork |
| `VFS_PM_SRV_FORK_REPLY` | `VFS_PM_SRV_FORK` | 系统服务进程 fork |

服务 fork 的额外操作是设置凭证（`pm_setuid`/`pm_setgid`），因为服务进程可能需要特定的 UID/GID（如 `root` 权限），而不是继承父进程的凭证。

### 3.2 回复内容

SRV_FORK 回复与普通 FORK 回复的字段差异：

| 字段 | FORK 回复 | SRV_FORK 回复 |
|------|-----------|---------------|
| `m_type` | `VFS_PM_FORK_REPLY` | `VFS_PM_SRV_FORK_REPLY` |
| `VFS_PM_ENDPT` | 子进程 endpoint | 子进程 endpoint |

两者的消息字段相同，区别仅在于 `m_type`。但 SRV_FORK 在构造回复前执行了额外的凭证设置：

```c
pm_setuid(proc_e, reuid, reuid);  /* 设置实际/有效 UID */
pm_setgid(proc_e, regid, regid);  /* 设置实际/有效 GID */
```

这些凭证信息来自 PM 发送的请求消息（`VFS_PM_REUID`/`VFS_PM_REGID`），由 RS（Reincarnation Server）指定。服务进程的凭证不继承自父进程，而是由 RS 根据服务配置决定。

---

## 4. 回复时序

### 4.1 同步 vs 异步

VFS 的 fork 处理是**同步的**——`pm_fork()` 不阻塞，所有操作（复制 fproc、递增引用计数、设置标识、dup_vnode）都是纯内存操作，不涉及 IPC 或 I/O。

`service_pm()` 在 `pm_fork()` 完成后立即构造回复并发送：

```c
pm_fork(pproc_e, proc_e, child_pid);    /* 同步处理，不阻塞 */
m_out.m_type = VFS_PM_FORK_REPLY;       /* 立即构造回复 */
m_out.VFS_PM_ENDPT = proc_e;
/* ... */
r = ipc_send(PM_PROC_NR, &m_out);       /* 非阻塞发送 */
```

`ipc_send()` 是非阻塞的——它将消息投递到 PM 的消息队列后立即返回。PM 必须已经准备好接收（即 PM 在发送 `VFS_PM_FORK` 后阻塞在 `receive` 上），否则发送会失败并触发 panic。

### 4.2 错误处理

```c
r = ipc_send(PM_PROC_NR, &m_out);
if (r != OK)
    panic("service_pm: ipc_send failed: %d", r);
```

回复失败时 VFS 直接 `panic()`。原因是：PM 在发送 `VFS_PM_FORK` 后阻塞等待回复，如果 VFS 的回复无法送达，PM 将**无限等待**，导致整个系统的 fork 功能瘫痪。

这是一个不可恢复的错误——PM 的 fork 流程已经部分完成（Kernel 已创建子进程、VM 已复制内存），无法回滚。因此 panic 是唯一合理的选择，表明系统状态已不一致。

---

## 5. 完整消息时序图

```
PM                          VFS
 │                           │
 │ ── VFS_PM_FORK ─────────►│ {pproc_e, proc_e, child_pid}
 │                           │
 │                           ├── service_pm() 接收消息
 │                           ├── pm_fork() 处理
 │                           │   ├── 复制 fproc
 │                           │   ├── 递增 filp_count
 │                           │   ├── 设置 pid/endpoint
 │                           │   ├── 清除 flags
 │                           │   └── dup_vnode(rd/wd)
 │                           │
 │                           ├── 构造回复 m_out
 │◄── VFS_PM_FORK_REPLY ────│
 │                           │
 ├── 继续处理 fork           │
 │   (调用 VM 等)            │
```

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/main.c` (service_pm fork 回复部分)

```c
case VFS_PM_FORK:
case VFS_PM_SRV_FORK:
    // ... 解析消息 ...
    pm_fork(pproc_e, proc_e, child_pid);
    m_out.m_type = VFS_PM_FORK_REPLY;
    // ...
    if (call_nr == VFS_PM_SRV_FORK) {
        m_out.m_type = VFS_PM_SRV_FORK_REPLY;
    }
    // ... ipc_send ...
```
