# Fork 系统调用纵向切片重构计划 — Part 1：总体架构与基础阶段

> **范围**: 总体架构 + 阶段 1~3（MProc 结构体、do_fork 前半部分、PID 生成器）
> **状态**: 阶段 1~2 已完成，阶段 3 待实现

---

## 总体架构

### Minix3 多进程表架构

> **重要**: Minix3 采用分布式进程表设计，共有 **4 份进程表**，分别由不同组件管理：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Minix3 进程表分布                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  ┌─────────┐│
│  │    Kernel       │  │       PM        │  │       VM        │  │   VFS   ││
│  │ proc[NR_TASKS+  │  │ mproc[NR_PROCS] │  │vmproc[NR_PROCS] │  │fproc[]  ││
│  │   NR_PROCS]     │  │                 │  │                 │  │         ││
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  └────┬────┘│
│           │                    │                    │                │      │
│           │   endpoint         │   endpoint         │   endpoint     │      │
│           │   proc_nr          │   mp_endpoint      │   vm_endpoint  │      │
│           │                    │   mp_pid           │                │      │
│           ▼                    ▼                    ▼                ▼      │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                    通过 endpoint / proc_nr 关联                          ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

### 各进程表职责

| 进程表 | C 源文件 | 核心职责 | 关键字段 |
|--------|---------|----------|----------|
| **Kernel/proc** | `kernel/proc.h` | 调度、IPC、寄存器 | `p_reg`, `p_rts_flags`, `p_priority`, `p_endpoint` |
| **PM/mproc** | `servers/pm/mproc.h` | 进程管理、信号、权限 | `mp_pid`, `mp_parent`, `mp_flags`, `mp_realuid` |
| **VM/vmproc** | `servers/vm/vmproc.h` | 虚拟内存、页表 | `vm_pt`, `vm_regions_avl`, `vm_total` |
| **VFS/fproc** | `servers/vfs/fproc.h` | 文件描述符、目录 | `fp_filp`, `fp_wd`, `fp_rd`, `fp_tty` |

### 进程表索引强对齐约束

> ⚠️ **关键约束**: Minix3 的四份进程表之间存在**物理级的索引对齐**关系。

在 Minix3 中，PM 的 `mproc[5]`、VM 的 `vmproc[5]`、VFS 的 `fproc[5]`、Kernel 的 `proc[5]` 
描述的是**同一个进程**的不同侧面。这个索引（槽位号）是跨服务的"物理标识符"。

#### 优势

| 优势 | 说明 |
|------|------|
| **Endpoint 瞬间转换** | 内核收到消息后，只需 `index = endpoint & mask` 即可定位，无需 Hash 表、无需搜索、无需锁 |
| **同步逻辑简化** | PM 告诉 VFS "5 号槽位 fork 了，新进程在 8 号槽位"，VFS 直接去 8 号槽位初始化即可 |
| **隐式共识** | 各服务共享同一套"物理地址空间（槽位号）"，成为跨服务的隐式协议 |

#### 劣势

| 劣势 | 说明 |
|------|------|
| **扩展性极差** | 若 PM 支持 1024 进程，但 VFS 只编译了 256，整个系统崩溃。所有服务必须为 `NR_PROCS` 重新编译 |
| **资源浪费** | 纯计算进程不打开文件，仍需在 VFS 的 `fproc` 表占坑位 |
| **容错风险** | 若 index 对齐出错（PM 释放 8 号槽位但 VFS 未收到通知），会发生严重"身份错乱" |

#### Rust 重写策略

**当前阶段采用"复刻派"**，理由：
1. 我们复刻的是 Minix3，index 对齐是其 IPC 协议的一部分
2. 在 `no_std` 且无分配器的内核早期，`HashMap<Pid, SlotIndex>` 会引入巨大复杂度

> **建议**: 将 `SlotIndex` 封装为独立类型（而非 `usize`），通过类型系统提醒：
> 这个 Index 不是普通数字，而是跨服务的"物理标识符"。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotIndex(pub usize);
// NOTE: This index MUST be synchronized with VFS/VM/Kernel due to Minix3's design.
```

### Endpoint 与 Generation

Minix3 的 Endpoint 格式（源码：`minix/include/minix/endpoint.h`）：

```c
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT(g, p) ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

```
endpoint = (generation << 15) + proc_nr

┌────────────────────────────────┬───────────────────────┐
│         高 17 位               │       低 15 位         │
│        generation              │      proc_nr          │
│    （代数，防止过时消息）        │   （进程槽位号）        │
└────────────────────────────────┴───────────────────────┘
```

**Generation 的维护**：
- 嵌入在 `endpoint` 中，**不需要单独存储**
- 每次槽位释放时 generation +1
- 作用：防止"过时的消息发给新进程"

### Fork 调用完整流程（跨服务器协调）

基于 `minix3/minix/servers/pm/forkexit.c` 的 `do_fork()` 函数：

```
用户进程 fork()
    │
    ▼ 系统调用
┌─────────────────────────────────────────┐
│  PM (Process Manager)                   │
│  ┌─────────────┐  ┌─────────────┐      │
│  │ do_fork()   │  │ get_free_pid│      │
│  │ mproc 表管理│  │ PID生成器   │      │
│  └──────┬──────┘  └─────────────┘      │
│         │                              │
│         │ ① 检查进程表是否已满            │
│         │ ② 查找空闲 mproc 槽位          │
│         │ ③ 调用 vm_fork()              │
│         │ ④ 复制父进程 mproc             │
│         │ ⑤ 设置子进程特有字段            │
│         │ ⑥ 分配 PID                    │
│         │ ⑦ 通知 VFS                    │
│         │ ⑧ 返回 SUSPEND               │
│         │                              │
└─────────┼───────────────────────────────┘
          │ IPC: vm_fork()
          ▼
┌─────────────────────────────────────────┐
│  VM (Virtual Memory)                    │
│  ┌─────────────┐                        │
│  │ vm_fork()   │                        │
│  │ vmproc 表管理│                       │
│  └──────┬──────┘                        │
│         │                              │
│         │ A. 分配 vmproc 槽位            │
│         │ B. 复制父进程地址空间          │
│         │ C. 调用 sys_fork() 创建内核进程│
│         │ D. 生成新 endpoint             │
│         │ E. 返回 endpoint 给 PM         │
│         │                              │
└─────────┼───────────────────────────────┘
          │ IPC: VFS_PM_FORK
          ▼
┌─────────────────────────────────────────┐
│  VFS (Virtual File System)              │
│  ┌─────────────┐                        │
│  │ fproc 表管理│                        │
│  └──────┬──────┘                        │
│         │                              │
│         │ a. 分配 fproc 槽位             │
│         │ b. 复制文件描述符表            │
│         │ c. 复制工作目录                │
│         │ d. 回复 PM                     │
│         │                              │
└─────────┼───────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────┐
│  Kernel (由 VM 的 sys_fork 触发)         │
│  ┌─────────────┐                        │
│  │ sys_fork()  │                        │
│  │ proc 表管理 │                        │
│  └─────────────┘                        │
│                                         │
│  I. 分配 proc 槽位                       │
│  II. 初始化调度状态                      │
│  III. 设置 endpoint                      │
└─────────────────────────────────────────┘
```

**关键约束**：PM 调用 `vm_fork()` 后不能再失败，因为 VM 内部已经调用了 `sys_fork()` 创建了内核进程。

```c
/* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */
```

### Rust Crate 架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Rust Crate 架构                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  ┌─────────┐│
│  │    minix-pm     │  │    minix-vm     │  │    minix-vfs    │  │ kernel  ││
│  │                 │  │                 │  │                 │  │         ││
│  │  mproc (私有)   │  │  vmproc (私有)  │  │  fproc (私有)   │  │proc(私有)││
│  │  Process        │  │  VmProcess      │  │  FileProcess    │  │KProcess ││
│  │  ProcTable      │  │  VmProcTable    │  │  FProcTable     │  │KProcTab ││
│  │  PmContext      │  │                 │  │                 │  │         ││
│  │  PidGenerator   │  │                 │  │                 │  │         ││
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  └────┬────┘│
│           │                    │                    │                │      │
│           │   Endpoint         │   Endpoint         │   Endpoint     │      │
│           │   Pid              │   Pid              │   Pid          │      │
│           │   (来自 minix-types)                    │                │      │
│           ▼                    ▼                    ▼                ▼      │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                    minix-types (核心协议层)                              ││
│  │                    Endpoint, Pid, ProcIndex, Uid, Gid...               ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

### 目录结构（当前已实现）

```
os/
├── libs/
│   └── minix-types/           # 核心协议类型
│       ├── src/
│       │   ├── lib.rs         # 只导出核心类型
│       │   ├── types/
│       │   │   └── pid.rs     # Pid, Endpoint, ProcIndex, NR_PROCS, LAST_FEW
│       │   └── ipc/
│       │       └── message.rs # Message
│       └── README.md
│
└── servers/
    └── pm/                    # PM 服务（lib + bin）
        ├── Cargo.toml
        ├── src/
        │   ├── lib.rs         # 导出 mproc 模块
        │   ├── main.rs        # PM 主循环
        │   ├── mproc/         # PM 私有进程表
        │   │   ├── mod.rs     # 模块定义与导出
        │   │   ├── mproc.rs   # Process 结构体（分层设计）
        │   │   ├── table.rs   # ProcTable + Endpoint/Generation
        │   │   ├── context.rs # PmContext
        │   │   ├── lifecycle.rs # 生命周期状态机
        │   │   ├── block.rs   # 阻塞状态
        │   │   ├── wait.rs    # 等待状态
        │   │   ├── guardianship.rs # 监护关系
        │   │   ├── trace.rs   # 追踪状态
        │   │   ├── signal.rs  # 信号处理状态
        │   │   ├── credentials.rs # 凭证
        │   │   └── fork.rs    # fork 实现
        │   ├── fork.rs        # fork 系统调用入口（占位）
        │   ├── exec.rs        # exec（占位）
        │   ├── exit.rs        # exit（占位）
        │   ├── signal.rs      # signal（占位）
        │   └── wait.rs        # wait（占位）
        └── README.md
```

---

## 第一阶段：MProc 结构体与进程表基础

**状态**: ✅ 已完成

**目标**: 建立 PM 进程管理的数据基础

### 1.1 C 源码分析

**文件**: `minix3/minix/servers/pm/mproc.h`

**`struct mproc` 完整字段**（fork 必需部分标注 ★）：

```c
EXTERN struct mproc {
  char mp_exitstatus;          // ★ 退出状态
  char mp_sigstatus;           // ★ 信号状态
  char mp_eventsub;            // 事件订阅者
  pid_t mp_pid;                // ★ 进程 ID
  endpoint_t mp_endpoint;      // ★ 内核端点
  pid_t mp_procgrp;            // ★ 进程组 ID
  pid_t mp_wpid;               // 等待的 PID
  vir_bytes mp_waddr;          // rusage 地址
  int mp_parent;               // ★ 父进程索引
  int mp_tracer;               // ★ 追踪者索引

  clock_t mp_child_utime;      // ★ 子进程用户时间累计
  clock_t mp_child_stime;      // ★ 子进程系统时间累计

  uid_t mp_realuid;            // ★ 真实 UID
  uid_t mp_effuid;             // ★ 有效 UID（权限检查用）
  uid_t mp_svuid;              // 保存的 UID
  gid_t mp_realgid;            // 真实 GID
  gid_t mp_effgid;             // 有效 GID
  gid_t mp_svgid;              // 保存的 GID

  int mp_ngroups;              // 补充组数
  gid_t mp_sgroups[NGROUPS_MAX]; // 补充组

  sigset_t mp_ignore;          // 忽略的信号
  sigset_t mp_catch;           // 捕获的信号
  sigset_t mp_sigmask;         // 信号掩码
  sigset_t mp_sigmask2;        // 保存的信号掩码
  sigset_t mp_sigpending;      // 待处理信号
  sigset_t mp_ksigpending;     // 内核待处理信号
  sigset_t mp_sigtrace;        // 追踪信号
  ixfer_sigaction *mp_sigact;  // ★ 信号处理函数（外部数组）
  vir_bytes mp_sigreturn;      // sigreturn 地址
  minix_timer_t mp_timer;      // 定时器
  clock_t mp_interval[NR_ITIMERS]; // ★ 间隔定时器
  clock_t mp_started;          // ★ 启动时间

  unsigned mp_flags;           // ★ 状态标志
  unsigned mp_trace_flags;     // ★ 追踪标志
  message mp_reply;            // 回复消息

  vir_bytes mp_frame_addr;     // 栈帧地址
  size_t mp_frame_len;         // 栈帧长度

  signed int mp_nice;          // ★ nice 值
  endpoint_t mp_scheduler;     // ★ 调度器端点

  char mp_name[PROC_NAME_LEN]; // ★ 进程名
  int mp_magic;                // 魔数校验
} mproc[NR_PROCS];
```

**标志位定义**（fork 必需标注 ★）：

```c
#define IN_USE        0x00001  // ★ 槽位已使用
#define WAITING       0x00002  // 父进程在等待
#define ZOMBIE        0x00004  // 僵尸状态
#define PROC_STOPPED  0x00008  // 进程已停止
#define ALARM_ON      0x00010  // 定时器已启动
#define EXITING       0x00020  // 正在退出
#define TOLD_PARENT   0x00040  // 已通知父进程
#define TRACE_STOPPED 0x00080  // 追踪停止
#define SIGSUSPENDED  0x00100  // 信号挂起
#define VFS_CALL      0x00400  // 等待 VFS
#define NEW_PARENT    0x00800  // ★ 父进程已变更
#define UNPAUSED      0x01000  // VFS 已回复
#define PRIV_PROC     0x02000  // ★ 特权进程
#define PARTIAL_EXEC  0x04000  // 部分执行
#define TRACE_EXIT    0x08000  // 追踪者强制退出
#define TRACE_ZOMBIE  0x10000  // 追踪僵尸
#define DELAY_CALL    0x20000  // ★ 延迟调用
#define TAINTED       0x40000  // ★ 污染标记
#define EVENT_CALL    0x80000  // 事件订阅等待
```

**常量定义**（`minix3/minix/servers/pm/const.h`）：

```c
#define NR_PIDS       30000    // PID 最大值
#define NO_PID        0        // 无效 PID
#define INIT_PID      1        // init 进程 PID
#define NO_TRACER     0        // 无追踪者
#define NR_ITIMERS    3        // 间隔定时器数量
```

### 1.2 Rust 实现（已完成）

**Process 结构体分层设计**：

```rust
// os/servers/pm/src/mproc/mproc.rs
pub struct Process {
    pub identity: ProcessIdentity,   // 身份：PID, Endpoint, procgrp, name
    pub state: ProcessState,         // 状态机：lifecycle, block, wait, guardianship, trace
    pub resources: ProcessResources, // 资源：privilege, signals, timers, nice, scheduler
    pub ipc: ProcessIpc,             // IPC：reply, event_subscriber, frame
}
```

**Minix3 字段 → Rust 分层映射**：

| Minix3 字段 | Rust 字段 | 层 |
|------------|----------|-----|
| `mp_pid` | `identity.id.pid` | 身份 |
| `mp_endpoint` | `identity.endpoint` | 身份 |
| `mp_procgrp` | `identity.procgrp` | 身份 |
| `mp_name` | `identity.name` | 身份 |
| `mp_parent` | `state.guardianship.parent` | 状态 |
| `mp_tracer` | `state.guardianship.tracer` | 状态 |
| `mp_flags` (IN_USE等) | `state.lifecycle` | 状态 |
| `mp_trace_flags` | `state.trace` | 状态 |
| `mp_realuid/mp_effuid` | `resources.privilege` | 资源 |
| `mp_nice` | `resources.nice` | 资源 |
| `mp_scheduler` | `resources.scheduler` | 资源 |
| `mp_reply` | `ipc.reply` | IPC |

### 1.3 验证目标

- [x] `Process` 结构体可正确定义并实例化（在 PM crate 中）
- [x] PM 进程表可安全访问（单线程 Cell 模式）
- [x] 单元测试：进程槽位分配与释放
- [x] minix-types 只包含核心协议类型（Endpoint, Pid 等）

---

## 第二阶段：do_fork 核心逻辑（上）— 参数检查与槽位分配

**状态**: ✅ 已完成

**目标**: 重写 fork 前半部分 - 参数检查与槽位分配

### 2.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 函数

```c
int do_fork(void) {
  register struct mproc *rmp;   // 父进程指针
  register struct mproc *rmc;   // 子进程指针
  static unsigned int next_child = 0;
  int i, n = 0, s;
  endpoint_t child_ep;
  message m;

  rmp = mp;  // 当前进程（宏：#define mp (&mproc[who_p])）

  // ① 检查进程表是否已满
  if ((procs_in_use == NR_PROCS) ||
      (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    printf("PM: warning, process table is full!\n");
    return(EAGAIN);
  }

  // ② 查找空闲槽位（轮询算法）
  do {
    next_child = (next_child+1) % NR_PROCS;
    n++;
  } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);

  if(n > NR_PROCS)
    panic("do_fork can't find child slot");

  // ③ 调用 VM fork（不可逆点！）
  if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;
  }
  /* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */

  // ④~⑪ 见 Part2 文档
}
```

**关键常量**：

```c
// forkexit.c 中定义
#define LAST_FEW  2  // 保留给超级用户的槽位数

// 注意：这与 minix-types 中的 LAST_FEW = 5 不同
// 原始 C 代码中 LAST_FEW = 2，需要确认哪个值是正确的
```

### 2.2 Rust 实现（已完成）

**ProcTable** (`os/servers/pm/src/mproc/table.rs`)：

```rust
pub struct ProcTable {
    pub procs: [Process; NR_PROCS],
    pub procs_in_use: Cell<usize>,
    pub next_child: Cell<usize>,
}
```

**PmContext** (`os/servers/pm/src/mproc/context.rs`)：

```rust
pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,
    pub current: usize,
}
```

**do_fork_prepare** (`os/servers/pm/src/mproc/fork.rs`)：

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork_prepare(&mut self) -> Result<ForkResult, ForkError> {
        if self.table.is_full() { return Err(ForkError::TableFull); }
        if !self.can_alloc() { return Err(ForkError::ReservedForRoot); }
        let child_index = self.table.alloc_slot().ok_or(ForkError::TableFull)?;
        let child_pid = self.generate_child_pid();
        let child_endpoint = ProcTable::calculate_endpoint(child_index);
        Ok(ForkResult { child_index, child_pid, child_endpoint })
    }
}
```

### 2.3 已知偏差与待修复项

| 偏差 | 说明 | 修复方案 |
|------|------|---------|
| **LAST_FEW 值不一致** | C 源码 `forkexit.c` 中 `#define LAST_FEW 2`，但 `minix-types` 中定义为 5 | 需确认 Minix3 实际值，可能 C 代码中是局部覆盖 |
| **PID 生成未实现** | 当前 `generate_child_pid()` 是简化实现，无冲突检测 | 阶段 3 实现 |
| **vm_fork 未调用** | 当前未实现 VM 交互 | 阶段 5 实现 |
| **endpoint 由 PM 计算** | Minix3 中 endpoint 由 VM/Kernel 生成返回，当前由 PM 本地计算 | 阶段 5 修正 |

### 2.4 验证目标

- [x] 进程表满时返回 `EAGAIN`
- [x] 非 root 用户在 LAST_FEW 保留区间返回 `EAGAIN`
- [x] 能正确找到并分配空闲槽位
- [x] 单元测试：边界条件（满表、只剩一个槽位等）

---

## 第三阶段：PID 生成器

**状态**: ❌ 待实现

**目标**: 重写 `get_free_pid` 函数

### 3.1 C 源码分析

**文件**: `minix3/minix/servers/pm/utility.c` (第 32-52 行)

```c
pid_t get_free_pid()
{
  static pid_t next_pid = INIT_PID + 1;  // 下一个 PID，初始值为 2
  register struct mproc *rmp;
  int t;  // 冲突标记：0 表示 PID 空闲

  do {
    t = 0;
    next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
    // 遍历所有进程检查 PID 冲突
    for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
      if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
        t = 1;
        break;
      }
  } while (t);

  return(next_pid);
}
```

**关键常量** (`minix3/minix/servers/pm/const.h`)：

```c
#define NR_PIDS    30000    // PID 范围：0 ~ NR_PIDS-1
#define INIT_PID   1        // init 进程 PID
```

**冲突检测的必要性**：

```c
if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid)
```

- `mp_procgrp` 是进程组 ID，通常等于进程组组长的 PID
- **PID 不能与任何进程的 `mp_pid` 或 `mp_procgrp` 冲突**

### 3.2 技术方案

详见 [fork-rewr-03.md](fork-rewr-03.md)，推荐方案二（位图）。

**需要添加的常量** (`os/libs/minix-types/src/types/pid.rs`)：

```rust
/// PID 最大值
pub const NR_PIDS: Pid = 30000;

/// init 进程 PID
pub const INIT_PID: Pid = 1;

/// 无效 PID
pub const NO_PID: Pid = 0;
```

**PidGenerator 设计**（推荐位图方案）：

```rust
pub struct PidGenerator {
    next_pid: Cell<Pid>,
    pid_bitmap: [Cell<u64>; (NR_PIDS as usize + 1 + 63) / 64],  // ~4.7 KB
    procgrp_bitmap: [Cell<u64>; (NR_PIDS as usize + 1 + 63) / 64], // ~4.7 KB
}
```

### 3.3 验证目标

- [ ] PID 唯一性保证（不与 `mp_pid` 和 `mp_procgrp` 冲突）
- [ ] PID 循环复用（达到 NR_PIDS 后回到 INIT_PID+1）
- [ ] PID 释放后可重新分配
- [ ] 单元测试：边界条件（PID 耗尽、procgrp 冲突等）

---

## 分阶段实施路线图

| 阶段 | 内容 | 状态 | 代码量 | 关键依赖 |
|------|------|------|--------|---------|
| **1** | MProc 结构体与进程表 | ✅ 完成 | ~800 行 | minix-types |
| **2** | do_fork 前半部分 | ✅ 完成 | ~200 行 | 阶段 1 |
| **3** | PID 生成器 | ❌ 待实现 | ~150 行 | 阶段 1 |
| **4** | do_fork 后半部分 | ❌ 待实现 | ~200 行 | 阶段 2+3 |
| **5** | VM Fork (Mock) | ❌ 待实现 | ~150 行 | 阶段 4 |
| **6** | VFS 通知与 SUSPEND | ❌ 待实现 | ~150 行 | 阶段 5 |
| **7** | do_exit 实现 | ❌ 待实现 | ~300 行 | 阶段 6 |
| **8** | do_wait4 实现 | ❌ 待实现 | ~250 行 | 阶段 7 |
| **9** | do_srv_fork 实现 | ❌ 待实现 | ~100 行 | 阶段 6 |
| **10** | 集成测试 | ❌ 待实现 | ~200 行 | 全部 |

> 阶段 4~10 详见 [Part 2](fork-syscall-plan-part2.md)
> 附录与参考详见 [Part 3](fork-syscall-plan-part3.md)
