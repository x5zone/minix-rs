# 00-vfs-overview: VFS 层架构概览 (fork 视角)

> **分类**: VFS 整体层级
> **说明**: 汇总 VFS 模块在 fork 流程中的全局概念、设计原则和跨组件约定

---

## 1. VFS 在 fork 中的角色

### 1.1 fork 流程概览

```
用户进程 fork()
    │
    ▼
PM (Process Manager)
    │
    ├── 分配子进程 slot
    ├── 调用 SYS_FORK → Kernel
    │
    ▼ 发送 VFS_PM_FORK 消息
┌──────────────────────────────┐
│       VFS (本阶段)            │
│  • 复制 fproc 结构体          │
│  • 递增 filp 引用计数         │
│  • 递增 vnode 引用计数        │
│  • 设置子进程 PID/endpoint    │
│  • 清除子进程标志              │
│  • 回复 PM                   │
└──────────────────────────────┘
    │
    ▼ 回复 VFS_PM_FORK_REPLY
PM 继续处理
    │
    ▼ 调用 VM
VM 复制地址空间
```

### 1.2 VFS 的职责

在 fork 流程中，VFS 负责：

1. **fproc 结构复制**: 将父进程的 `struct fproc` 复制到子进程槽
2. **文件描述符共享**: 递增所有打开文件的 `filp_count`，使父子进程共享文件表条目
3. **目录 vnode 复制**: 通过 `dup_vnode()` 递增根目录和工作目录的引用计数
4. **进程属性设置**: 设置子进程的 PID、endpoint，清除标志

### 1.3 与其他组件的协作

#### 1.3.1 与 PM 的协作

- **PM 职责**: 分配进程槽、发送 `VFS_PM_FORK` 消息
- **VFS 职责**: 复制文件系统相关状态、回复 `VFS_PM_FORK_REPLY`
- **协作方式**: PM 通过 IPC 消息通知 VFS，VFS 同步处理并回复

#### 1.3.2 与 Kernel 的协作

- **Kernel 先行**: 在 VFS 之前，Kernel 已通过 `do_fork()` 创建了子进程的内核结构
- **VFS 后续**: VFS 在 Kernel 处理完成后才收到 PM 的 fork 通知

#### 1.3.3 与 VM 的协作

- **无直接交互**: VFS 在 fork 流程中不与 VM 直接通信
- **间接关系**: VM 负责地址空间复制，VFS 负责文件状态复制

---

## 2. 核心数据结构

### 2.1 VFS 数据结构全景

VFS 围绕进程文件状态管理，核心数据结构形成如下层次：

```
fproc (进程文件状态)
  │
  ├── fp_filp[] ──→ filp (文件表条目，引用计数)
  │                    │
  │                    └── filp_vno ──→ vnode (虚拟节点，引用计数)
  │                                       │
  │                                       └── v_vmnt ──→ vmnt (挂载点)
  │
  ├── fp_rd ──→ vnode (根目录)
  └── fp_wd ──→ vnode (工作目录)
```

### 2.2 关键结构体

| 结构体 | 定义文件 | 说明 | fork 时的处理 |
|--------|----------|------|---------------|
| `struct fproc` | `fproc.h` | VFS 进程状态 | 整体复制，修正字段 |
| `struct filp` | `file.h` | 文件表条目 | 引用计数递增 |
| `struct vnode` | `vnode.h` | 虚拟节点 | 引用计数递增 (dup_vnode) |
| `struct vmnt` | `vmnt.h` | 挂载点 | 不直接涉及 |
| `struct worker_thread` | `threads.h` | 工作线程 | 不直接涉及 |
| `tll_t` | `tll.h` | 三级锁 | 不直接涉及 |

### 2.3 引用计数模型

fork 的核心语义是 **共享而非复制**：

```
fork 前:
  父进程 fproc
    fp_filp[0] ──→ filp (filp_count=1)
    fp_filp[1] ──→ filp (filp_count=1)
    fp_rd ──→ vnode (v_ref_count=1)
    fp_wd ──→ vnode (v_ref_count=1)

fork 后:
  父进程 fproc                   子进程 fproc
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (filp_count=2)
    fp_filp[1] ──┐                 fp_filp[1] ──┘  → filp (filp_count=2)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode (v_ref_count=2)
    fp_wd ───────┐                 fp_wd ───────┘   → vnode (v_ref_count=2)
    fp_lock (自己的)               fp_lock (自己的，被保留)
    fp_pid = parent_pid            fp_pid = cpid
    fp_flags = (原值)              fp_flags = FP_NOFLAGS
```

**POSIX 语义**:
- 父子进程共享文件偏移量 (`filp_pos`)——这是 fork 的核心语义
- `close()` 只递减 `filp_count`，到 0 时才真正释放
- 文件锁 (`flock`) 不被继承（POSIX 规定）
- `select()` 状态不被继承

---

## 3. VFS 源码组织

### 3.1 目录结构

```
minix3/minix/servers/vfs/
├── fproc.h          ← VFS 进程结构体 (01-03)
├── file.h           ← filp 文件表 (04)
├── vnode.h          ← vnode 虚拟节点 (05)
├── vmnt.h           ← vmnt 挂载点 (06)
├── tll.h            ← 三级锁 (07)
├── threads.h        ← 工作线程结构 (08)
├── const.h          ← 常量定义 (09)
├── type.h           ← 类型定义 (09)
├── glo.h            ← 全局变量 (09)
├── fs.h             ← 主头文件 (09)
├── proto.h          ← 函数原型 (09)
├── main.c           ← 主循环与消息分发 (10-11, 16)
├── misc.c           ← pm_fork/pm_exit 实现 (12-15, 19)
├── filedes.c        ← 文件描述符管理 (17)
├── comm.c           ← 进程间通信 (18)
├── worker.c         ← 工作线程管理
├── lock.c           ← 文件锁
├── vnode.c          ← vnode 操作
├── vmnt.c           ← 挂载点操作
├── tll.c            ← 三级锁实现
├── path.c           ← 路径解析
├── open.c           ← open 系统调用
├── read.c           ← read 系统调用
├── write.c          ← write 系统调用
├── pipe.c           ← 管道操作
├── mount.c          ← 挂载操作
├── exec.c           ← exec 系统调用
├── select.c         ← select 系统调用
├── protect.c        ← 权限管理
├── request.c        ← FS 请求封装
├── socket.c         ← socket 操作
├── stadir.c         ← stat 目录操作
├── device.c         ← 设备管理
├── dmap.c           ← 设备映射
├── bdev.c           ← 块设备
├── cdev.c           ← 字符设备
├── sdev.c           ← socket 设备
├── smap.c           ← socket 映射
├── link.c           ← 链接操作
├── coredump.c       ← 核心转储
├── time.c           ← 时间操作
├── utility.c        ← 工具函数
├── table.c          ← 调度表
└── gcov.c           ← 代码覆盖
```

### 3.2 文档与源码对应

| 文档 | 对应源文件 | 覆盖内容 |
|------|-----------|---------|
| 01-fproc-struct | `fproc.h` | FProc 基本字段 |
| 02-fproc-flags | `fproc.h`, `const.h` | 进程标志与阻塞状态 |
| 03-fproc-cred | `fproc.h` | UID/GID/umask 凭证 |
| 04-filp-struct | `file.h` | Filp 文件表条目 |
| 05-vnode-struct | `vnode.h`, `vnode.c` | VNode 虚拟节点 |
| 06-vmnt-struct | `vmnt.h` | VMnt 挂载点 |
| 07-tll-lock | `tll.h`, `tll.c` | 三级锁机制 |
| 08-worker-thread | `threads.h`, `worker.c` | 工作线程框架 |
| 09-globals-const | `glo.h`, `const.h`, `type.h`, `fs.h`, `proto.h` | 全局变量与常量 |
| 10-main-loop | `main.c` | 主循环与消息分发 |
| 11-service-pm | `main.c` | PM 消息处理与 fork 路由 |
| 12-pm-fork-copy | `misc.c` | pm_fork: fproc 复制 |
| 13-pm-fork-filp | `misc.c` | pm_fork: filp 引用计数 |
| 14-pm-fork-vnode | `misc.c`, `vnode.c` | pm_fork: vnode 引用计数 |
| 15-pm-fork-flags | `misc.c` | pm_fork: 标志与 PID/endpoint |
| 16-pm-fork-reply | `main.c` | VFS-PM fork 回复协议 |
| 17-filedes | `filedes.c` | 文件描述符管理 (close 路径) |
| 18-comm | `comm.c` | VFS 进程间通信 |
| 19-pm-exit | `misc.c` | pm_exit (fork 的逆操作) |

---

## 4. pm_fork 完整流程

```
PM 发送 VFS_PM_FORK 消息
    │ {parent_ep, child_ep, child_pid}
    │
    ▼
VFS main loop (main.c)
    │ get_work() → service_pm()
    │
    ▼
service_pm() (main.c:764)
    │ 解析消息字段
    │ 区分 VFS_PM_FORK / VFS_PM_SRV_FORK
    │
    ▼
pm_fork(pproc, cproc, cpid) (misc.c:577)
    │
    ├── 1. okendpt() 验证父进程 endpoint
    ├── 2. _ENDPOINT_P() 提取子进程 slot
    ├── 3. assert 子进程 slot 空闲
    ├── 4. 保存子进程 fp_lock
    ├── 5. 整体复制 fproc[parent] → fproc[child]
    ├── 6. 恢复子进程 fp_lock
    ├── 7. 遍历 fp_filp[]，递增 filp_count
    ├── 8. 设置 cp->fp_pid = cpid
    ├── 9. 设置 cp->fp_endpoint = cproc
    ├── 10. cp->fp_flags = FP_NOFLAGS
    ├── 11. dup_vnode(cp->fp_rd)
    └── 12. dup_vnode(cp->fp_wd)
    │
    ▼
service_pm() 继续
    │ 构造 VFS_PM_FORK_REPLY 消息
    │ ipc_send(PM_PROC_NR, &m_out)
    │
    ▼
PM 收到回复，继续 fork 流程
```

---

## 5. VFS 多线程模型

VFS 是 Minix3 中唯一使用多线程的服务器进程，这对 fork 处理有重要影响：

- **工作线程池**: `NR_WTHREADS` (9) 个工作线程并发处理请求
- **fp_lock 保留**: fork 复制 fproc 时必须保留子进程 slot 自己的 mutex，因为其他线程可能正在操作该 slot
- **锁机制**: vnode 和 vmnt 使用三级锁 (TLL) 保护共享状态
- **阻塞状态**: fproc 记录了进程在 VFS 中的阻塞原因

---

## 6. 与 Kernel 阶段的对比

| 维度 | Kernel (03-stage) | VFS (04-stage) |
|------|-------------------|----------------|
| 核心结构 | `struct proc` | `struct fproc` |
| 复制方式 | 整体复制 + 修正 | 整体复制 + 修正 |
| 引用计数 | 无（独占） | filp_count / v_ref_count（共享） |
| 标识 | endpoint + proc_nr | endpoint + pid |
| 锁 | 自旋锁 / 中断禁用 | mutex (mthread) |
| 并发模型 | 单核中断驱动 | 多线程 |
| fork 触发 | PM 发送 SYS_FORK | PM 发送 VFS_PM_FORK |
| fork 回复 | 同步返回 endpoint | 异步 ipc_send 回复 |

---

## 7. 参见

- [03-stage-kernel/00-kernel-overview.md](../03-stage-kernel/00-kernel-overview.md) - Kernel 整体架构概览
- [99-global-concepts.md](99-global-concepts.md) - VFS 全局概念暂存区
