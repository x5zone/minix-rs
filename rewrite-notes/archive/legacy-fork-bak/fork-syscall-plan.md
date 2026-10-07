# Fork 系统调用纵向切片重构计划

> **目标**: 将 Minix3 的 fork 系统调用从 C 逐步重构为 Rust
> **策略**: 小步快跑，每次一个阶段，逻辑与基建同步推进
> **范围**: PM + VM + VFS + Kernel 四服务协同
> **原则**: 只有硬件允许 mock，OS 逻辑必须触及

---

## 文档索引

本文档已拆分为三个部分，便于维护和查阅：

| 文档 | 范围 | 阶段 |
|------|------|------|
| [Part 1](fork-syscall-plan-part1.md) | 总体架构 + 基础阶段 | 阶段 1~3 |
| [Part 2](fork-syscall-plan-part2.md) | PM 核心实现 | 阶段 4~6 |
| [Part 3](fork-syscall-plan-part3.md) | VM/VFS/Kernel 实现 | 阶段 7~10 |

---

## 核心设计原则

### 1. 纵向切片 = 四服务协同

Minix3 的 fork 不是 PM 的独角戏，而是 **4 个服务** 的协作：

```
用户进程 fork()
    │
    ▼
PM: do_fork()                         [forkexit.c]
    ├── 分配 mproc 槽位、PID
    ├── 发送 VM_FORK → VM
    │       │
    │       ▼
    │   VM: do_fork()                  [vm/fork.c]
    │       ├── 复制 vmproc 结构体
    │       ├── 创建新页表 (pt_new)
    │       ├── 复制内存区域 (COW)
    │       ├── 发送 SYS_FORK → Kernel
    │       │       │
    │       │       ▼
    │       │   Kernel: do_fork()      [kernel/system/do_fork.c]
    │       │       ├── 复制 proc 结构体
    │       │       ├── 递增 generation，生成新 endpoint
    │       │       ├── 设置 RTS_NO_QUANTUM + RTS_VMINHIBIT
    │       │       └── 返回子进程 endpoint
    │       │
    │       ├── pt_bind() 绑定页表
    │       └── 返回子进程 endpoint 给 PM
    │
    ├── 设置子进程 mproc 字段
    ├── 发送 VFS_PM_FORK → VFS
    │       │
    │       ▼
    │   VFS: pm_fork()                 [vfs/misc.c]
    │       ├── 复制 fproc 结构体
    │       ├── 增加 filp 引用计数
    │       ├── 增加 vnode 引用计数
    │       └── 回复 PM
    │
    └── 返回 SUSPEND（等待 VFS 回复后唤醒父进程）
```

### 2. Mock 边界

| 层次 | 内容 | Mock 策略 |
|------|------|----------|
| **PM** | mproc 结构体、PID 生成、进程状态 | ✅ 真实实现 |
| **VM** | vmproc 结构体、内存区域复制 | ✅ 真实实现 |
| **VFS** | fproc 结构体、文件描述符复制 | ✅ 真实实现 |
| **Kernel** | proc 结构体、endpoint generation | ✅ 真实实现 |
| **IPC 传输** | 消息格式、send/receive | ✅ 真实实现 |
| **硬件** | 时钟中断、页表硬件、FPU 上下文 | ❌ Mock |
| **物理内存** | 实际物理页分配/释放 | ❌ Mock（用 Vec 模拟） |
| **CPU 调度** | 实际上下文切换 | ❌ Mock |

### 3. 阶段总览

| 阶段 | 内容 | 涉及服务 | 状态 |
|------|------|----------|------|
| 1 | MProc 结构体重构 | PM | ✅ 已完成 |
| 2 | do_fork 前半部分（槽位分配） | PM | ✅ 已完成 |
| 3 | PID 生成器 | PM | ✅ 已完成 |
| 4 | 进程结构复制与初始化 | PM | ✅ 已完成 |
| 5 | IPC 消息格式定义 | PM/VM/VFS/Kernel | ❌ 待实现 |
| 6 | VM vmproc 结构体与 fork | VM | ❌ 待实现 |
| 7 | Kernel proc 结构体与 sys_fork | Kernel | ❌ 待实现 |
| 8 | VFS fproc 结构体与 pm_fork | VFS | ❌ 待实现 |
| 9 | 跨服务协调：PM→VM→Kernel→VFS | PM/VM/VFS/Kernel | ❌ 待实现 |
| 10 | 集成测试与验证 | 全部 | ❌ 待实现 |

---

## 已有实现状态

| 模块 | 实现程度 | 关键缺失 |
|------|----------|----------|
| **minix-types** | 90% | 缺少具体系统调用消息类型常量 |
| **minix-ipc** | 30% | SyscallNum 已定义，send/receive 是 todo!() |
| **PM/mproc** | 80% | Process 结构体、进程表、fork 核心逻辑已实现 |
| **PM 顶层** | 5% | fork/exec/exit/signal/wait 入口全是 todo!() |
| **VM 服务器** | 0% | 完全空壳，无 vmproc 结构体 |
| **VFS 服务器** | 0% | 完全空壳，无 fproc 结构体 |
| **Kernel** | 5% | KProcess 只有 endpoint，其余全是空壳 |
