# 11-service-pm: PM 消息处理与 fork 路由

> 本文档分析 `minix3/minix/servers/vfs/main.c` 中的 `service_pm()` 和 `service_pm_postponed()` 函数。

---

## 1. 概述

### 1.1 PM-VFS 通信模型

- PM 与 VFS 之间的 IPC 通信模型：PM 和 VFS 是 Minix3 微内核架构中的两个用户态服务器，通过内核的 IPC 机制进行同步消息传递。PM 在处理进程生命周期事件（fork、exec、exit、setuid 等）时，需要通知 VFS 同步更新其私有的 fproc 表。通信流程为：PM 调用 `ipc_send(VFS_PROC_NR, &m)` 发送请求消息，VFS 主循环通过 `sef_receive(ANY, &m_in)` 接收，检测到 `who_e == PM_PROC_NR` 后调用 `service_pm()` 处理，处理完成后通过 `ipc_send(PM_PROC_NR, &m_out)` 发送回复。
- PM 是 VFS 的"客户端"——PM 不发起文件系统操作，而是通知 VFS 进程生命周期事件。这种关系是单向的：PM → VFS。当用户进程调用 fork/exec/exit 等系统调用时，请求先到达 PM，PM 处理完自己的 mproc 表更新后，再通知 VFS 同步更新 fproc 表。VFS 不主动向 PM 发起请求（除了初始化阶段的同步），只在收到 PM 消息后被动处理并回复。
- `VFS_PM_FORK` 与 `VFS_PM_SRV_FORK` 的区别：`VFS_PM_FORK` 用于普通用户进程的 fork，子进程继承父进程的 fproc 但凭证（UID/GID）不变；`VFS_PM_SRV_FORK` 用于系统服务进程的 fork（如 RS 重启服务时创建子进程），除了继承 fproc 外，还需要额外设置子进程的凭证为指定的 reuid/regid（通过 `pm_setuid()` 和 `pm_setgid()`），且回复消息类型为 `VFS_PM_SRV_FORK_REPLY` 而非 `VFS_PM_FORK_REPLY`。两者都调用同一个 `pm_fork()` 函数，区别仅在后续的凭证设置和回复类型。

---

## 2. service_pm() 函数

### 2.1 函数入口

- `service_pm()` 的调用点在 [main.c:93](minix3/minix/servers/vfs/main.c#L93)：主循环中 `get_work()` 返回后，检查 `who_e == PM_PROC_NR`，若消息来自 PM 则直接调用 `service_pm()` 并 `continue`，不再走 `call_vec[]` 分发路径。`service_pm()` 在主线程中执行，因此不能阻塞——任何可能阻塞的操作必须推迟到工作线程中处理。
- 函数签名 `static void service_pm(void)`（[main.c:764](minix3/minix/servers/vfs/main.c#L764)）：`static` 表示仅在本文件可见，`void` 参数表示不需要显式传参——函数通过全局变量 `m_in`、`call_nr`、`who_e` 等访问当前消息和发送者信息，通过全局 `m_out` 构造回复。无返回值——处理结果通过回复消息告知 PM。

### 2.2 消息接收

- PM 消息的接收并非在 `service_pm()` 内部进行——消息已在主循环的 `get_work()` 中通过 `sef_receive(ANY, &m_in)` 接收完毕。`service_pm()` 被调用时，`m_in` 中已经包含了 PM 发来的消息。`service_pm()` 直接从全局 `m_in` 和 `call_nr` 宏（展开为 `m_in.m_type`）读取消息类型和字段，无需再次接收。
- 消息类型通过 `call_nr` 宏（即 `m_in.m_type`）解析。`service_pm()` 使用 `switch (call_nr)` 对消息类型进行分发，每个 `case` 对应一种 PM 消息类型：`VFS_PM_SETUID`、`VFS_PM_SETGID`、`VFS_PM_SETSID`、`VFS_PM_EXEC`、`VFS_PM_EXIT`、`VFS_PM_DUMPCORE`、`VFS_PM_UNPAUSE`、`VFS_PM_FORK`、`VFS_PM_SRV_FORK`、`VFS_PM_SETGROUPS`、`VFS_PM_REBOOT`。未知类型走 `default` 分支，打印错误信息并返回。

### 2.3 消息类型分发

- `VFS_PM_FORK` 处理分支（[main.c:850-875](minix3/minix/servers/vfs/main.c#L850)）：从消息中提取三个字段——`pproc_e = m_in.VFS_PM_PENDPT`（父进程 endpoint）、`proc_e = m_in.VFS_PM_ENDPT`（子进程 endpoint）、`child_pid = m_in.VFS_PM_CPID`（子进程 PID），然后调用 `pm_fork(pproc_e, proc_e, child_pid)` 执行 fproc 复制和引用计数递增，最后设置 `m_out.m_type = VFS_PM_FORK_REPLY` 和 `m_out.VFS_PM_ENDPT = proc_e`。
- `VFS_PM_SRV_FORK` 与 `VFS_PM_FORK` 共用同一个 `case` 分支（[main.c:850-851](minix3/minix/servers/vfs/main.c#L850)），因为两者都调用 `pm_fork()`。区别在于：SRV_FORK 额外从消息中提取 `reuid = m_in.VFS_PM_REUID` 和 `regid = m_in.VFS_PM_REGID`，在 `pm_fork()` 之后调用 `pm_setuid(proc_e, reuid, reuid)` 和 `pm_setgid(proc_e, regid, regid)` 设置子进程的凭证，并将回复类型改为 `VFS_PM_SRV_FORK_REPLY`。
- `VFS_PM_EXIT` 处理分支（[main.c:829-849](minix3/minix/servers/vfs/main.c#L829)）：与 `VFS_PM_EXEC`、`VFS_PM_DUMPCORE`、`VFS_PM_UNPAUSE` 共用推迟处理路径。从消息中提取 `proc_e = m_in.VFS_PM_ENDPT`，验证 endpoint 有效性后，调用 `worker_start(rfp, NULL, &m_in, FALSE)` 启动工作线程处理。注意 `func` 参数为 NULL——工作线程会检查目标进程的 `FP_PENDING` 标志，在 `service_pm_postponed()` 中根据 `job_call_nr` 分发到具体的 `pm_exit()` 处理。
- `VFS_PM_SETUID` 处理分支（[main.c:784-798](minix3/minix/servers/vfs/main.c#L784)）：从消息中提取 `proc_e = m_in.VFS_PM_ENDPT`、`euid = m_in.VFS_PM_EID`、`ruid = m_in.VFS_PM_RID`，直接调用 `pm_setuid(proc_e, euid, ruid)` 修改目标进程的有效 UID 和真实 UID。设置回复 `m_out.m_type = VFS_PM_SETUID_REPLY` 和 `m_out.VFS_PM_ENDPT = proc_e`。这是立即处理的消息——不需要推迟到工作线程，因为 `pm_setuid()` 只修改 fproc 的凭证字段，不涉及阻塞操作。
- `VFS_PM_SETGID` 处理分支（[main.c:800-814](minix3/minix/servers/vfs/main.c#L800)）：与 SETUID 结构相同，从消息中提取 `proc_e = m_in.VFS_PM_ENDPT`、`egid = m_in.VFS_PM_EID`、`rgid = m_in.VFS_PM_RID`，调用 `pm_setgid(proc_e, egid, rgid)` 修改目标进程的有效 GID 和真实 GID。设置回复 `m_out.m_type = VFS_PM_SETGID_REPLY` 和 `m_out.VFS_PM_ENDPT = proc_e`。同样是立即处理，不涉及阻塞操作。
- `VFS_PM_SETSID` 处理分支（[main.c:816-826](minix3/minix/servers/vfs/main.c#L816)）：从消息中提取 `proc_e = m_in.VFS_PM_ENDPT`，调用 `pm_setsid(proc_e)` 将目标进程设为会话领导进程。`pm_setsid()` 会清除进程的控制终端（`fp_tty = NO_DEV`）并设置 `FP_SESLDR` 标志。设置回复 `m_out.m_type = VFS_PM_SETSID_REPLY` 和 `m_out.VFS_PM_ENDPT = proc_e`。立即处理，不涉及阻塞操作。
- 其他 PM 消息类型：`VFS_PM_SETGROUPS`（[main.c:876-891](minix3/minix/servers/vfs/main.c#L876)）——设置补充组列表，从消息提取 `proc_e`、`group_no`、`group_addr`，调用 `pm_setgroups()`，立即处理并回复；`VFS_PM_REBOOT`（[main.c:893-904](minix3/minix/servers/vfs/main.c#L893)）——重启请求，通过 `worker_start(fproc_addr(PM_PROC_NR), pm_reboot, &m_in, FALSE)` 在独立工作线程中处理，因为 `pm_reboot()` 可能涉及阻塞的文件系统同步操作。

---

## 3. VFS_PM_FORK 消息处理

### 3.1 消息字段

- VFS_PM_FORK 消息的字段：
  - `m_in.VFS_PM_PENDPT` (`pproc_e`)：父进程的 endpoint，VFS 通过 `okendpt()` 验证其有效性并定位父进程的 fproc 槽位
  - `m_in.VFS_PM_ENDPT` (`proc_e`)：子进程的 endpoint，VFS 通过 `_ENDPOINT_P()` 提取子进程槽位号（注意：此时子进程的 fproc 中 `fp_endpoint` 尚未设置，不能用 `okendpt()` 验证）
  - `m_in.VFS_PM_CPID` (`child_pid`)：子进程的 PID，由 PM 分配，VFS 将其设置到子进程 fproc 的 `fp_pid` 字段

### 3.2 调用 pm_fork()

- `pm_fork(pproc_e, proc_e, child_pid)` 调用（详见 [12-pm-fork-copy](12-pm-fork-copy.md)）：这是 VFS fork 的核心函数，执行以下操作：① 验证父进程 endpoint 并定位 fproc；② 提取子进程槽位号，断言其 `fp_pid == PID_FREE`；③ 整体复制父进程 fproc 到子进程槽位（保留子进程自己的 `fp_lock`）；④ 遍历子进程的 `fp_filp[]`，对每个非 NULL 的 filp 递增 `filp_count`；⑤ 设置子进程的 `fp_pid` 和 `fp_endpoint`；⑥ 清除子进程标志 `fp_flags = FP_NOFLAGS`；⑦ 对 `fp_rd` 和 `fp_wd` 调用 `dup_vnode()` 递增 vnode 引用计数。

### 3.3 构造回复

- `m_out.m_type = VFS_PM_FORK_REPLY`——设置回复消息类型为 `VFS_PM_FORK_REPLY`，告知 PM fork 操作已完成。回复类型与请求类型一一对应：`VFS_PM_FORK` → `VFS_PM_FORK_REPLY`，`VFS_PM_SRV_FORK` → `VFS_PM_SRV_FORK_REPLY`。PM 通过回复类型区分不同的 fork 结果。
- 回复消息的字段：`m_out.VFS_PM_ENDPT = proc_e`——包含子进程的 endpoint，PM 用此字段确认 VFS 已正确处理了该子进程的 fork。fork 回复非常简洁，不包含错误码——因为 `pm_fork()` 不返回错误（内部使用 `okendpt()` 和 `assert()` 确保正确性，失败则 panic）。这与 exec/exit 等操作的回复不同，后者包含 `VFS_PM_STATUS` 字段表示操作结果。

---

## 4. VFS_PM_SRV_FORK 消息处理

### 4.1 与 VFS_PM_FORK 的区别

- 服务进程 fork (`VFS_PM_SRV_FORK`) 的特殊之处：普通 fork 的子进程继承父进程的全部凭证（UID/GID），而服务进程 fork 的子进程需要使用 PM 指定的凭证。这是因为系统服务（如 RS 管理的服务）可能需要以不同的用户身份运行子进程。SRV_FORK 消息额外包含 `VFS_PM_REUID` 和 `VFS_PM_REGID` 字段，指定子进程应使用的真实/有效 UID 和 GID。
- 系统服务 fork 后的额外初始化：`pm_fork()` 完成基本的 fproc 复制后，SRV_FORK 路径还会调用 `pm_setuid(proc_e, reuid, reuid)` 和 `pm_setgid(proc_e, regid, regid)` 设置子进程的凭证。注意两个参数相同（real = effective），这意味着服务进程的子进程不使用 saved-set-UID/GID 机制。此外，服务进程的子进程可能需要设置 `FP_SRV_PROC` 标志（由 `pm_fork()` 从父进程继承），使其在 VFS 中被识别为服务进程，从而在向 FS 发送请求时获得特殊处理（如回调机制）。

### 4.2 处理流程

- `VFS_PM_SRV_FORK` 的处理逻辑（[main.c:850-875](minix3/minix/servers/vfs/main.c#L850)）：与 `VFS_PM_FORK` 共用 `case` 分支。提取 `pproc_e`、`proc_e`、`child_pid` 后调用 `pm_fork()`，然后检查 `call_nr == VFS_PM_SRV_FORK`，若是则额外提取 `reuid` 和 `regid`，调用 `pm_setuid()` 和 `pm_setgid()` 设置凭证，并将 `m_out.m_type` 改为 `VFS_PM_SRV_FORK_REPLY`。
- `m_out.m_type = VFS_PM_SRV_FORK_REPLY`——使用不同的回复类型，使 PM 能区分普通 fork 和服务 fork 的回复。在代码中，先设置 `m_out.m_type = VFS_PM_FORK_REPLY`（默认值），然后 `if (call_nr == VFS_PM_SRV_FORK)` 时覆盖为 `VFS_PM_SRV_FORK_REPLY`。这种"先设默认、再按需覆盖"的模式简化了代码——大部分逻辑是共享的，只在需要时分支。

### 4.3 pm_fork() 的共用

- `VFS_PM_FORK` 和 `VFS_PM_SRV_FORK` 都调用同一个 `pm_fork()` 函数。`pm_fork()` 只负责 fproc 的复制和引用计数递增，不涉及凭证设置——凭证的差异化处理在 `service_pm()` 中完成。这种设计遵循了"核心逻辑统一，差异逻辑外置"的原则：`pm_fork()` 是纯粹的 fproc 复制操作，与 fork 类型无关；凭证设置是 PM 侧的业务逻辑，由 `service_pm()` 根据消息类型决定是否执行。
- 两种 fork 的区别总结：① 回复消息类型不同（`VFS_PM_FORK_REPLY` vs `VFS_PM_SRV_FORK_REPLY`）；② SRV_FORK 额外调用 `pm_setuid()` 和 `pm_setgid()` 设置凭证；③ SRV_FORK 消息额外包含 `VFS_PM_REUID` 和 `VFS_PM_REGID` 字段。除此之外，两者完全一致——都调用 `pm_fork()`，都设置 `m_out.VFS_PM_ENDPT = proc_e`，都通过 `ipc_send(PM_PROC_NR, &m_out)` 发送回复。

---

## 5. service_pm_postponed() 函数

### 5.1 延迟处理机制

- `service_pm_postponed()`（[main.c:668-759](minix3/minix/servers/vfs/main.c#L668)）在工作线程上下文中执行延迟的 PM 消息处理。它通过 `job_call_nr`（从推迟时保存的消息中提取）分发到具体的处理函数：`VFS_PM_EXEC` → `pm_exec()`、`VFS_PM_EXIT` → `pm_exit()`、`VFS_PM_DUMPCORE` → `pm_dumpcore()`、`VFS_PM_UNPAUSE` → `unpause()`。每个分支构造对应的回复消息并通过 `ipc_send(PM_PROC_NR, &m_out)` 发送。
- 需要延迟处理的原因：`service_pm()` 在主线程中执行，不能阻塞。但 `VFS_PM_EXEC`、`VFS_PM_EXIT`、`VFS_PM_DUMPCORE`、`VFS_PM_UNPAUSE` 这些操作可能涉及阻塞的文件系统操作（如关闭文件时需要向 FS 发送请求并等待回复）。更重要的是，这些操作的目标进程可能正忙——有工作线程在处理其系统调用。PM 请求必须与目标进程的当前操作串行化，否则会导致 fproc 状态不一致。因此，`service_pm()` 将这些请求推迟：通过 `worker_start(rfp, NULL, &m_in, FALSE)` 启动关联到目标进程的工作线程，当目标进程的当前操作完成后，工作线程再执行推迟的 PM 请求。

### 5.2 延迟处理的消息类型

- 需要延迟处理的 PM 消息类型：`VFS_PM_EXEC`、`VFS_PM_EXIT`、`VFS_PM_DUMPCORE`、`VFS_PM_UNPAUSE`。这四种消息的共同特点是：① 操作的目标进程可能正忙（有工作线程在处理其请求）；② 操作本身可能涉及阻塞的文件系统调用（如 `pm_exit()` 需要关闭所有打开的文件，`pm_exec()` 需要执行二进制文件）。相比之下，`VFS_PM_FORK`、`VFS_PM_SETUID`、`VFS_PM_SETGID`、`VFS_PM_SETSID`、`VFS_PM_SETGROUPS` 可以立即处理，因为它们只修改 fproc 的内存字段，不涉及阻塞操作。
- 延迟队列的管理：VFS 没有显式的"延迟队列"数据结构。推迟机制通过 fproc 的 `FP_PENDING` 标志和工作线程的 `worker_start()` 实现。当 `service_pm()` 收到需要推迟的 PM 消息时，调用 `worker_start(rfp, NULL, &m_in, FALSE)` 将消息保存到目标进程关联的工作线程中。如果目标进程正忙（已有工作线程），`worker_start()` 会设置 `FP_PENDING` 标志并将消息暂存。当目标进程的当前操作完成后，`send_work()` 检查 `FP_PENDING` 标志，启动工作线程执行 `service_pm_postponed()`。

### 5.3 回复发送

- 延迟处理的回复发送：`service_pm_postponed()` 在工作线程中执行完毕后，通过 `ipc_send(PM_PROC_NR, &m_out)` 发送回复。与立即处理的回复不同，延迟处理的回复不在 `service_pm()` 中发送，而是在工作线程的上下文中发送。这意味着 PM 可能需要等待较长时间才能收到回复——从 VFS 收到消息到工作线程完成处理，中间可能经过目标进程的当前操作完成、文件系统请求回复等多个步骤。PM 在等待回复期间阻塞在 `ipc_receive()` 上。
- 发送失败时的 panic 处理：`service_pm()` 中使用 `ipc_sendnb(PM_PROC_NR, &m_out)` 发送回复，若返回值不为 `OK`，则调用 `panic("VFS: service_pm: ipc_send failed")` 直接 panic。这是因为 PM 和 VFS 之间的通信是同步的——PM 在发送请求后阻塞等待回复，如果 VFS 无法发送回复，PM 将永久阻塞，整个系统的进程管理将瘫痪。因此，发送失败是不可恢复的致命错误，panic 是唯一合理的选择。

---

## 6. PM 消息格式

### 6.1 VFS_PM_FORK 请求

- PM → VFS 方向的 fork 消息格式（`VFS_PM_FORK`）：`m_type = VFS_PM_FORK`，`VFS_PM_PENDPT` = 父进程 endpoint，`VFS_PM_ENDPT` = 子进程 endpoint，`VFS_PM_CPID` = 子进程 PID。对于 `VFS_PM_SRV_FORK`，额外包含 `VFS_PM_REUID` = 子进程真实/有效 UID，`VFS_PM_REGID` = 子进程真实/有效 GID。
- 消息字段的命名约定：PM → VFS 消息使用 `VFS_PM_` 前缀，字段名遵循 `<服务>_<方向>_<含义>` 模式。`PENDPT` = Parent Endpoint（父进程端点），`ENDPT` = Endpoint（目标进程端点），`CPID` = Child PID（子进程 PID），`EID` = Effective ID（有效 ID），`RID` = Real ID（真实 ID），`REUID`/`REGID` = Requested Effective UID/GID（请求的有效 UID/GID，仅 SRV_FORK 使用）。

### 6.2 VFS_PM_FORK_REPLY 回复

- VFS → PM 方向的 fork 回复消息格式：`m_type = VFS_PM_FORK_REPLY`，`VFS_PM_ENDPT = proc_e`（子进程 endpoint）。回复非常简洁，仅包含子进程的 endpoint 作为确认。不包含错误码——`pm_fork()` 内部使用 `okendpt()` 和 `assert()` 确保正确性，失败则 panic，不会返回错误。
- 回复中包含的信息：仅 `VFS_PM_ENDPT`（子进程 endpoint）。PM 用此字段确认 VFS 已正确处理了该子进程的 fork。与 exec/exit 回复不同，fork 回复不包含 `VFS_PM_STATUS` 状态码，因为 fork 操作在 VFS 侧不可能失败——只要 PM 发送的 endpoint 有效，`pm_fork()` 就一定能成功完成。

### 6.3 VFS_PM_SRV_FORK_REPLY 回复

- SRV_FORK 回复与普通 FORK 回复的区别：仅 `m_type` 不同——`VFS_PM_SRV_FORK_REPLY` vs `VFS_PM_FORK_REPLY`。两者的 `VFS_PM_ENDPT` 字段完全相同（都是子进程 endpoint）。PM 需要区分回复类型是因为 SRV_FORK 的调用者（如 RS）可能需要特殊处理——例如等待服务进程的凭证设置完成后再继续。

### 6.4 pm_setuid() 函数

- `pm_setuid(proc_e, euid, ruid)` 函数（[protect.c:90](minix3/minix/servers/vfs/protect.c#L90)）：通过 `okendpt(proc_e, &slot)` 验证 endpoint 有效性并获取槽位号，然后设置 `fproc[slot].fp_effuid = euid` 和 `fproc[slot].fp_realuid = ruid`。函数不返回错误——如果 `okendpt()` 失败则 `panic`。在 SRV_FORK 中，`euid == ruid`（两个参数相同），因为服务进程不需要 saved-set-UID 机制。

### 6.5 pm_setgid() 函数

- `pm_setgid(proc_e, egid, rgid)` 函数（[protect.c:119](minix3/minix/servers/vfs/protect.c#L119)）：与 `pm_setuid()` 结构相同，通过 `okendpt()` 验证 endpoint 后设置 `fproc[slot].fp_effgid = egid` 和 `fproc[slot].fp_realgid = rgid`。在 SRV_FORK 中，`egid == rgid`，原因与 `pm_setuid()` 相同。
- `pm_setsid(proc_e)` 函数（[protect.c:144](minix3/minix/servers/vfs/protect.c#L144)）：通过 `okendpt()` 验证 endpoint 后，设置 `fproc[slot].fp_flags |= FP_SESLDR`（标记为会话领导进程）并清除 `fproc[slot].fp_tty = NO_DEV`（断开与原控制终端的关联）。POSIX 规定 `setsid()` 创建新会话，调用进程成为会话领导，且不再有控制终端。
- `pm_setgroups(proc_e, ngroups, group_addr)` 函数（[protect.c:161](minix3/minix/servers/vfs/protect.c#L161)）：通过 `okendpt()` 验证 endpoint 后，设置 `fproc[slot].fp_ngroups = ngroups`，然后通过 `sys_datacopy()` 从用户空间（`group_addr` 指向的内存地址）复制 `ngroups` 个 GID 到 `fproc[slot].fp_sgroups[]`。如果 `ngroups == 0`，则不执行复制操作，仅清零计数。

---

## 7. 错误处理

### 7.1 ipc_send 失败

- `panic("service_pm: ipc_send failed: %d", r)`——当 `ipc_sendnb(PM_PROC_NR, &m_out)` 返回非 `OK` 值时触发。`r` 是 IPC 发送的错误码，可能的原因包括：PM 进程已崩溃（endpoint 无效）、内核 IPC 机制内部错误。panic 是唯一合理的选择，因为 PM 在等待回复，VFS 无法告知 PM 操作结果，系统将陷入死锁。
- 回复失败需要 panic 的原因：PM 和 VFS 之间的通信是同步请求-回复模型——PM 发送请求后阻塞在 `ipc_receive()` 上等待回复。如果 VFS 无法发送回复，PM 将永久阻塞，导致整个系统的进程管理功能瘫痪（无法 fork、exec、exit）。没有其他恢复路径——不可能重试（PM 已经在阻塞等待），也不可能通知其他进程（没有备用通信渠道）。因此 panic 是唯一安全的选择。

### 7.2 pm_fork 内部错误

- `pm_fork()` 中的断言和 panic 条件：① `okendpt(pproc_e, &parent_slot)` 失败时 panic——父进程 endpoint 无效，说明 PM 发送了错误的数据；② `assert(rfp->fp_pid == PID_FREE)`——子进程槽位必须空闲，否则说明 PM 和 VFS 的进程表状态不一致；③ `assert(rfp->fp_endpoint == NONE)`——子进程槽位的 endpoint 必须为 NONE，与 `fp_pid == PID_FREE` 构成双重检查。这些断言保护了 PM-VFS 之间的一致性不变量——如果不变量被破坏，继续运行只会导致更严重的数据损坏。
- 子进程 slot 非空闲时的 panic：`pm_fork()` 中通过 `_ENDPOINT_P(proc_e)` 提取子进程槽位号后，使用 `assert(rfp->fp_pid == PID_FREE)` 断言子进程槽位必须空闲。如果断言失败，说明 PM 分配的子进程槽位已被占用——这是 PM 和 VFS 之间状态不一致的致命错误，唯一的恢复方式是 panic。这种不一致可能发生在：PM 在未通知 VFS 的情况下重用了槽位，或 VFS 在处理 exit 时未正确释放槽位。

### 7.3 pm_exec 错误处理

- `pm_exec()` 函数（[exec.c](minix3/minix/servers/vfs/exec.c)）：处理 `VFS_PM_EXEC` 消息，执行进程的 exec 操作。主要步骤：① 关闭设置了 FD_CLOEXEC 的文件描述符（遍历 `fp_filp[]`，检查 `fp_cloexec_set` 位图）；② 从可执行文件路径加载新程序映像；③ 设置进程名 `fp_name`；④ 重置信号处理。`pm_exec()` 需要在工作线程中执行，因为加载可执行文件涉及向 FS 发送请求并等待回复。

### 7.4 pm_exit 处理

- `pm_exit()` 函数（[main.c:604](minix3/minix/servers/vfs/main.c#L604)）：处理 `VFS_PM_EXIT` 消息，清理退出进程的 VFS 资源。主要步骤：① 遍历 `fp_filp[]`，对每个非 NULL 的 filp 调用 `close_fp()` 关闭文件（递减 `filp_count`，若为 0 则释放 filp 和 vnode）；② 释放 `fp_rd` 和 `fp_wd` 的 vnode 引用（`put_vnode()`）；③ 清空 `fp_filp[]` 和 `fp_cloexec_set`；④ 释放该进程持有的所有文件锁；⑤ 将 `fp_pid` 设为 `PID_FREE`，`fp_endpoint` 设为 `NONE`，标记槽位为未使用。

### 7.5 pm_dumpcore 处理

- `pm_dumpcore()` 函数（[exec.c](minix3/minix/servers/vfs/exec.c)）：处理 `VFS_PM_DUMPCORE` 消息，在进程崩溃时生成核心转储文件。与 `pm_exit()` 类似，需要关闭文件描述符和释放资源，但额外需要创建核心转储文件并写入内存映像。核心转储文件的创建涉及路径解析和文件写入操作，因此必须在工作线程中执行。

---

## 8. C 源码

**文件**: `minix3/minix/servers/vfs/main.c` (service_pm 关键部分)

```c
static void service_pm(void)
{
  // ... 接收 PM 消息 ...
  case VFS_PM_FORK:
  case VFS_PM_SRV_FORK:
    // ... 解析消息字段 ...
    pm_fork(pproc_e, proc_e, child_pid);
    m_out.m_type = VFS_PM_FORK_REPLY;
    // ...
    if (call_nr == VFS_PM_SRV_FORK) {
      m_out.m_type = VFS_PM_SRV_FORK_REPLY;
    }
    // ... 发送回复 ...
}
```

- `okendpt(endpoint, &slot)` 验证（[utility.c](minix3/minix/servers/vfs/utility.c)）：检查 endpoint 是否有效并返回对应的槽位号。验证逻辑：① 从 endpoint 提取槽位号 `p = _ENDPOINT_P(endpoint)`；② 检查 `p >= 0 && p < NR_PROCS`；③ 检查 `fproc[p].fp_endpoint == endpoint`（确保 endpoint 的 generation 号匹配）。如果验证失败返回 `EINVAL`，成功则设置 `*slot = p` 并返回 `OK`。所有 PM 消息处理函数都通过 `okendpt()` 验证 endpoint，防止使用过期或无效的进程标识。
