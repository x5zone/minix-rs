# 99-global-concepts: VFS 全局概念暂存区

> **分类**: Global 层级 ⚠️
> **说明**: VFS 全局概念汇总，不只是 VFS 视角，后续将迁移到系统级文档
> **⚠️ 注意**: 本文档内容属于系统全局，不应局限于 VFS 视角

---

## 1. 引用计数模型

### 1.1 VFS 引用计数体系

VFS 维护三类引用计数，分别管理不同层次的资源生命周期：

| 引用计数 | 所属结构 | 管理的资源 | 递增操作 | 递减操作 |
|----------|----------|------------|----------|----------|
| `filp_count` | `filp` | 文件表条目 | fork（`filp_count++`）、dup | close（`filp_count--`） |
| `v_ref_count` | `vnode` | VFS 层 vnode | `dup_vnode()` | `put_vnode()` |
| `v_fs_count` | `vnode` | 底层 FS 层 inode | `req_getnode()` | `req_putnode()`（v_ref_count 归零时） |

层次关系：

```
fproc.fp_filp[i] ──→ filp (filp_count) ──→ vnode (v_ref_count) ──→ FS inode (v_fs_count)
fproc.fp_rd/fp_wd ─────────────────────→ vnode (v_ref_count) ──→ FS inode (v_fs_count)
```

- `filp_count`：多少个 `fp_filp[]` 条目指向此 filp
- `v_ref_count`：多少个 filp + 多少个 `fp_rd`/`fp_wd` 指向此 vnode
- `v_fs_count`：VFS 向底层 FS 报告的引用数量

### 1.2 引用计数不变量

引用计数系统必须维护以下不变量：

1. **`filp_count > 0` ⟹ `filp_vno` 指向有效 vnode**：filp 存活时，其指向的 vnode 必须也存活。`close_filp()` 在 `filp_count` 降到 0 时才调用 `put_vnode()`，保证此不变量。

2. **`v_ref_count > 0` ⟹ vnode slot 被占用**：`v_ref_count == 0` 表示 vnode slot 空闲，可被新打开的文件复用。`put_vnode()` 在 `v_ref_count` 降到 0 时重置 vnode 并回收 slot。

3. **`v_fs_count >= v_ref_count` 的累积性**：`v_fs_count` 不与 `v_ref_count` 同步递减，而是在 `v_ref_count` 归零时一次性递减。为防止溢出，设置 256 阈值触发 `vnode_clean_refs()` 同步。

4. **fork 后 `filp_count` 和 `v_ref_count` 的对称性**：fork 递增的每个计数，exit 都必须递减。如果 exit 后计数不为 0，说明存在引用泄漏。

### 1.3 跨服务器引用计数

`v_ref_count`（VFS 层）和 `v_fs_count`（底层 FS 层）是独立的：

- **`v_ref_count`** 由 VFS 自行管理，`dup_vnode()`/`put_vnode()` 直接操作
- **`v_fs_count`** 通过 IPC 与底层 FS 同步，`req_getnode()`/`req_putnode()` 跨服务器通信

两者不同步递减的原因是性能：每次 `put_vnode()` 都调用 `req_putnode()` 会导致频繁的跨服务器 IPC。Minix3 的策略是**延迟同步**：`v_fs_count` 只在 `v_ref_count` 归零时才递减，减少 IPC 次数。

底层 FS 维护自己的 inode 引用计数，与 VFS 的 `v_fs_count` 对应。当 `v_fs_count` 降到 0 时，底层 FS 可以释放 inode（如果 FS 层的引用计数也降到 0）。

---

## 2. 进程生命周期与文件状态

### 2.1 文件状态的生命周期

文件引用计数随进程生命周期的变化：

| 事件 | filp_count | v_ref_count | 说明 |
|------|------------|-------------|------|
| open | 0→1 | 不变（vnode 已有引用） | 新建 filp，绑定到已有 vnode |
| fork | N→N+1 | 不变 | 子进程共享 filp，filp_count 递增 |
| dup | N→N+1 | 不变 | 同一进程内两个 fd 共享 filp |
| close | N→N-1 | 不变（N>1） | filp_count 递减，文件不关闭 |
| close | 1→0 | M→M-1 | 最后一个引用关闭，put_vnode |
| exit | 遍历 close | 遍历 put_vnode | 批量释放所有文件引用 |

关键观察：**fork 只递增 `filp_count`，不递增 `v_ref_count`**。因为 filp 是 vnode 的"代理"，fork 增加的是代理的引用者数量，而非代理指向的目标的引用数量。

### 2.2 目录状态的生命周期

目录 vnode 引用计数随进程生命周期的变化：

| 事件 | v_ref_count | 说明 |
|------|-------------|------|
| chdir | old: M→M-1, new: N→N+1 | `put_vnode(old_wd)` + `dup_vnode(new_wd)` |
| fork | N→N+1 | `dup_vnode(fp_rd)` + `dup_vnode(fp_wd)` |
| exit | N→N-1 | `put_vnode(fp_rd)` + `put_vnode(fp_wd)` |

与文件引用的关键区别：**目录 vnode 直接在 `fproc` 中引用（`fp_rd`/`fp_wd`），不经过 `filp` 中间层**。因此 fork 时直接递增 `v_ref_count`（`dup_vnode()`），而非 `filp_count`。

---

## 3. 多服务器协作模型

### 3.1 PM-VFS-Kernel 协作

fork 流程中三个服务器的职责划分：

| 服务器 | 职责 | 数据结构 |
|--------|------|----------|
| **PM** | 分配 PID、协调 fork 流程、管理进程树 | `mproc[]` |
| **Kernel** | 创建内核进程结构、分配 endpoint | `proc[]` |
| **VFS** | 复制文件状态、管理文件描述符和目录 | `fproc[]`、`filp[]`、`vnode[]` |

协作模式：PM 是 fork 的**协调者**，按顺序调用 Kernel 和 VFS：

1. PM → Kernel：`sys_fork()` 创建内核进程
2. PM → VFS：`VFS_PM_FORK` 复制文件状态
3. PM → VM：复制内存映射

每个服务器独立管理自己的进程表，通过 endpoint 和 slot 号保持一致。

### 3.2 VFS-FS 协作

VFS 与底层 FS 的协作遵循**透明性原则**：底层 FS 不知道也不需要知道 fork 的发生。

- **fork 时不通信**：`pm_fork()` 不调用 `fs_sendrec()`，所有操作在 VFS 内部完成
- **close 时才通信**：`v_ref_count` 降到 0 时，`put_vnode()` 通过 `req_putnode()` 通知底层 FS
- **FS 视角**：底层 FS 只看到 inode 的打开/关闭事件，不关心是哪个进程触发的

这种设计使得 VFS 可以独立管理文件描述符的共享语义，无需与底层 FS 协调。

### 3.3 消息流

fork 流程中的完整消息流：

```
User → PM:    fork() 系统调用
PM → Kernel:  sys_fork(parent, child_slot)     创建内核进程
Kernel → PM:  返回子进程 endpoint
PM → VFS:     VFS_PM_FORK(parent_e, child_e, child_pid)
VFS:          pm_fork() — 复制 fproc、递增引用计数
VFS → PM:     VFS_PM_FORK_REPLY(child_endpoint)
PM → VM:      vm_fork(parent, child)            复制内存映射
VM → PM:      返回结果
PM → User:    返回 child_pid（父进程）/ 0（子进程）
```

关键观察：PM 是消息流的中心，按顺序与 Kernel、VFS、VM 通信。VFS 的处理是同步的（不阻塞），PM 等待 VFS 回复后才继续。

---

## 4. POSIX fork 语义

### 4.1 必须继承的属性

POSIX 规定 fork 后子进程必须继承的属性，以及各属性的实现者：

| 属性 | 实现者 | 继承方式 |
|------|--------|----------|
| 打开文件描述符（共享偏移量） | VFS | 整体复制 `fp_filp[]`，`filp_count++` |
| FD_CLOEXEC 标志 | VFS | 整体复制 `fp_cloexec_set` |
| 工作目录、根目录 | VFS | 整体复制 `fp_rd`/`fp_wd`，`dup_vnode()` |
| 文件模式创建掩码 (umask) | VFS | 整体复制 `fp_umask` |
| 实际/有效 UID/GID | VFS | 整体复制 `fp_realuid`/`fp_effuid` 等 |
| 补充组 | VFS | 整体复制 `fp_sgroups[]`/`fp_ngroups` |
| 控制终端 | VFS | 整体复制 `fp_tty` |
| 信号掩码 | PM | PM 复制 `mp_sigmask` |
| 进程组 ID | PM | PM 复制 `mp_procgrp` |
| 会话 ID | PM | PM 复制会话成员关系 |

### 4.2 不继承的属性

POSIX 规定 fork 后子进程不继承的属性：

| 属性 | 原因 | VFS 处理 |
|------|------|----------|
| 文件锁 (flock/F_SETLK) | 锁属于特定进程 | `close_fd()` 释放锁 |
| select/poll 状态 | 等待属于特定进程 | 不复制等待状态 |
| 异步 I/O 操作 | 操作属于特定进程 | 不复制 AIO 上下文 |
| 定时器 (alarm) | 定时器属于特定进程 | PM 不复制 `mp_interval` |
| 信号待处理集 | 信号属于特定进程 | PM 清除 `mp_sigpending` |
| 退出状态 | 子进程未退出 | `fp_flags = FP_NOFLAGS`（清除 `FP_EXITING`） |

### 4.3 VFS 的实现

VFS 通过"整体复制 + 选择性修正"实现 POSIX fork 语义：

1. **整体复制** `fproc[child] = fproc[parent]`：继承所有属性（文件描述符、目录、凭证、umask 等）
2. **递增引用计数**：`filp_count++`（文件）、`dup_vnode()`（目录）
3. **修正标识字段**：`fp_pid`、`fp_endpoint`、`fp_flags`
4. **不继承的属性自动清除**：`fp_flags = FP_NOFLAGS` 清除 `FP_EXITING`、`FP_PENDING` 等

VFS 负责文件相关的 POSIX 语义（文件描述符、目录、凭证），PM 负责进程相关的语义（信号、进程组、定时器），Kernel 负责调度相关的语义（优先级、CPU 亲和性）。

---

## 5. 待迁移内容

> 本节记录后续需要迁移到系统级文档的内容。

### 5.1 引用计数模型

> **待迁移**: VFS 引用计数模型应迁移到独立的全局文档，因为引用计数是跨子系统的重要概念。

### 5.2 POSIX 语义规范

> **待迁移**: POSIX fork 语义规范应迁移到独立文档，因为涉及 PM、Kernel、VFS、VM 多个子系统。

### 5.3 多服务器消息协议

> **待迁移**: PM-VFS-Kernel 消息协议应迁移到系统级 IPC 文档。

---

## 6. 参见

- [00-vfs-overview.md](00-vfs-overview.md) - VFS 整体架构概览
- [03-stage-kernel/99-global-concepts.md](../03-stage-kernel/99-global-concepts.md) - Kernel 全局概念
