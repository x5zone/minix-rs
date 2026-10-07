# Fork 系统调用纵向切片 — 项目概述

## 1. 项目目标

使用 Rust 语言重构复刻 Minix3 操作系统的 `fork()` 系统调用实现，作为OS探索的纵向切片，以供学习与验证。

### 核心要求
- **纵向切片**: 贯穿 PM → VM → Kernel → VFS → SCHED 五个服务层
- **源码级复刻**: 算法、数据结构、状态流转与 Minix3 源码保持一致
- **硬件全 Mock**: 所有硬件操作（MMU、寄存器、时钟、磁盘）使用 Mock 实现
- **纯软件逻辑**: 仅实现 OS 内部资源管理、引用计数、状态机等核心逻辑
- **约束**: no_std 环境，**仅支持 64 位系统**

### 实施策略
- **小步快跑**: 每次 100-300 行代码，逻辑与基建同步推进
- **逐层实现**: 先完成 PM 基础结构，再扩展 VM/VFS/Kernel

## 2. 核心约束

### 2.1 硬件 Mock 范围
| 硬件类型 | Mock 内容 | 实现重点 |
|---------|----------|---------|
| MMU/页表 | 页表创建、绑定、权限设置 | CoW 标记、读写权限判定逻辑 |
| 物理内存 | 内存分配/释放 | 物理块引用计数管理 |
| CPU 寄存器 | 上下文保存/恢复 | `ret_reg = 0` 伪造逻辑 |
| FPU | 浮点状态保存区 | 父子进程 FPU 状态隔离 |
| 时钟 | 系统滴答获取 | 进程启动时间、定时器管理 |
| 磁盘 IO | 文件读写、inode 操作 | filp/vnode 引用计数 |
| IPC 通信 | 跨服务消息发送/接收 | 消息协议、异步通知机制 |

### 2.2 Mock 边界详细规范

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

#### Mock 接口定义

所有硬件相关操作统一使用 Mock 实现：

```rust
// 内存分配Mock: 返回虚拟物理地址
type MockPhysAlloc = dyn FnMut() -> u64;

// 寄存器读写Mock
type MockRegAccess = dyn FnMut(Reg, u64) -> Result<u64, ()>;

// 时钟Mock: 返回当前滴答数
type MockClock = dyn FnMut() -> Clock;

// IPC通信Mock: 消息发送/接收
type MockIpcSend = dyn FnMut(Endpoint, &Message) -> Result<(), IpcError>;
```

### 2.3 禁止事项
- 不允许使用 Rust `Clone` trait 代替显式字段复制
- 不允许跳过任何 Minix3 源码中的状态检查或标志位处理
- 不允许合并或简化跨服务协调流程

## 3. 五层架构

```
┌─────────────────────────────────────────────────────────────┐
│  User Space                                                 │
│  fork() syscall ────────────────────────────────────────────┤
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  PM (Process Manager)                                       │
│  - 进程结构复制 (mproc)                                      │
│  - PID 分配与回收                                            │
│  - 父子进程关系建立                                          │
│  - 跨服务协调 (VM/VFS/SCHED)                                 │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│  VM (Virtual    │ │  Kernel         │ │  VFS (Virtual   │
│   Memory)       │ │  - PCB 克隆      │ │   File System)  │
│  - 地址空间克隆  │ │  - 上下文伪造    │ │  - 文件描述符复制│
│  - CoW 机制     │ │  - Endpoint 生成 │ │  - filp引用计数  │
│  - 物理块管理   │ │  - RTS 标志管理  │ │  - vnode引用计数 │
└─────────────────┘ └─────────────────┘ └─────────────────┘
                              │
                              ▼
              ┌─────────────────────────┐
              │  SCHED (Scheduler)      │
              │  - 优先级继承            │
              │  - 时间片继承            │
              │  - CPU 选择              │
              │  - 调度接管 (sys_schedctl)│
              └─────────────────────────┘
```

### 3.1 五服务协同流程

```
PM                    VM                    Kernel              VFS                 SCHED
│                     │                     │                   │                    │
│──VM_FORK───────────>│                     │                   │                    │
│                     │──SYS_FORK──────────>│                   │                    │
│                     │<──child_endpoint────│                   │                    │
│<──child_endpoint────│                     │                   │                    │
│                     │                     │                   │                    │
│──VFS_PM_FORK────────────────────────────────────────────────>│                    │
│<──VFS_PM_FORK_REPLY─────────────────────────────────────────│                    │
│                     │                     │                   │                    │
│──SCHEDULING_INHERIT─────────────────────────────────────────────────────────────>│
│<──OK (scheduler=SCHED_PROC_NR)──────────────────────────────────────────────────│
│                     │                     │                   │                    │
│  (唤醒父进程)        │                     │                   │                    │
```

### 关键约束

1. **PM 调用 vm_fork 后不能失败**：VM 已经调用了 sys_fork() 创建了内核进程
2. **Endpoint generation**：每次槽位重用时 generation 递增，防止旧消息误投递
3. **SUSPEND 机制**：PM 返回 SUSPEND 给内核，等待 VFS 回复后才唤醒父进程

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
    ├── 发送 SCHEDULING_INHERIT → SCHED
    │       │
    │       ▼
    │   SCHED: do_start_scheduling()  [sched/schedule.c]
    │       ├── 继承父进程 priority/time_slice
    │       ├── pick_cpu() 选择 CPU
    │       ├── sys_schedctl() 接管调度
    │       └── schedule_process() 设置内核调度参数
    │
    └── 返回 SUSPEND（等待 VFS 回复后唤醒父进程）
```

### 3.2 各层职责

| 层级 | 核心结构体 | 主要职责 |
|-----|-----------|---------|
| PM | `mproc` | 进程身份、状态、资源、IPC 管理；fork 主状态机 |
| VM | `vmproc` | 虚拟内存区域、物理块、CoW 引用计数、页表绑定 |
| Kernel | `proc` | 调度上下文、寄存器保存、Endpoint 生成、RTS 标志 |
| VFS | `fproc` | 文件描述符表、filp/vnode 引用计数、工作目录 |
| SCHED | `schedproc` | 优先级、时间片、CPU 选择、调度接管 |

## 4. 关键算法清单

### 4.1 必须实现的硬核逻辑
- [ ] **vm_region 遍历**: 遍历父进程所有内存区域进行 CoW 复制
- [ ] **filp 引用计数**: fork 时递增，close 时递减，零时释放
- [ ] **vnode 引用计数**: 根目录/工作目录的引用管理
- [ ] **PCB 上下文伪造**: 子进程 `ret_reg = 0`，父进程返回子 PID
- [ ] **CoW 引用计数算法**: `refcount==1` 可写，`refcount>=2` 只读
- [ ] **Endpoint 生成**: `endpoint = (generation << 15) | slot`

### 4.2 状态机关键点
- **PM do_fork**: 槽位分配 → VM_FORK → 结构复制 → VFS通知 → SCHED通知 → SUSPEND
- **VM fork**: 页表创建 → 区域遍历 → CoW 标记 → 页表绑定
- **Kernel fork**: PCB 克隆 → 上下文伪造 → RTS 标志设置 → 调度准备
- **VFS fork**: filp 引用递增 → vnode 引用递增 → 异步回复
- **SCHED fork**: priority/time_slice 继承 → CPU 选择 → 调度接管 → 参数下发

## 5. 文档地图

### 00-master-plan/ 目录结构（平铺）

| 文件 | 内容说明 |
|-----|---------|
| `01-project-overview.md` | 本文件：项目概述、目标、约束、架构 |
| `02-architecture-analysis.md` | 四层架构深度分析、数据流、交互协议 |
| `03-minix3-source-audit.md` | Minix3 源码审计：关键函数、结构体、算法 |
| `04-implementation-roadmap.md` | 实现路线图：6 阶段规划、里程碑、验收标准 |
| `05-mock-strategy.md` | Mock 策略：各层 Mock 接口定义、使用规范 |
| `06-phase1-pm-guide.md` | 阶段1：PM 层实现指南 |
| `07-phase2-vm-guide.md` | 阶段2：VM 层实现指南 |
| `08-phase3-kernel-guide.md` | 阶段3：Kernel 层实现指南 |
| `09-phase4-vfs-guide.md` | 阶段4：VFS 层实现指南 |
| `10-phase5-statemachine-guide.md` | 阶段5：PM 状态机与跨服务协调指南 |
| `11-verification-checklist.md` | 验证清单：100+ 检查项、测试用例 |

## 6. 参考资源

### Minix3 源码位置
- PM: `minix/servers/pm/forkexit.c` — `do_fork()`
- VM: `minix/servers/vm/fork.c`, `region.c`, `phys.c`
- Kernel: `minix/kernel/system/do_fork.c`
- VFS: `minix/servers/vfs/misc.c` — `pm_fork()`

### 本地备份
- 原始规划文档: `fork-syscall-rewrite-bak/`

## 7. 当前进度

| 阶段 | 状态 | 说明 |
|-----|------|------|
| 阶段1: PM 基础结构 | ✅ 已完成 | `Process::fork_from()`, PID 生成, Endpoint 管理 |
| 阶段2: VM CoW | ⏳ 待实现 | 地址空间克隆, 物理块引用计数 |
| 阶段3: Kernel PCB | ⏳ 待实现 | 上下文伪造, Endpoint 生成, RTS 标志 |
| 阶段4: VFS fd | ⏳ 待实现 | filp/vnode 引用计数 |
| 阶段5: SCHED 调度 | ⏳ 待实现 | 优先级/时间片继承, CPU 选择, 调度接管 |
| 阶段6: PM 状态机 | ⏳ 待实现 | 跨服务协调, 失败回滚 |
| 阶段7: 集成测试 | ⏳ 待实现 | 端到端验证 |

---

> **下一步**: 阅读 `02-architecture-analysis.md` 了解四层架构详细设计
