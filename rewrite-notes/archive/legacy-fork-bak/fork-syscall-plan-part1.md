# Fork 系统调用纵向切片重构计划 — Part 1：总体架构与基础阶段

> **范围**: 总体架构 + 阶段 1~3（MProc 结构体、do_fork 前半部分、PID 生成器）
> **状态**: 阶段 1~4 已完成

---

## 总体架构

### Minix3 多进程表架构

Minix3 采用分布式进程表设计，共有 4 份进程表：

| 服务 | 进程表 | C 源文件 | Rust crate |
|------|--------|---------|------------|
| PM | `mproc` | `servers/pm/mproc.h` | `minix-pm` |
| VM | `vmproc` | `servers/vm/vmproc.h` | `minix-vm` |
| VFS | `fproc` | `servers/vfs/fproc.h` | `minix-vfs` |
| Kernel | `proc` | `kernel/proc.h` | `minix-kernel` |

四份进程表通过 `endpoint` 关联：

```
endpoint = _ENDPOINT(generation, slot)
         = (generation << 15) | slot
```

### Fork 的四服务协作

```
PM                    VM                    Kernel              VFS
│                     │                     │                   │
│──VM_FORK───────────>│                     │                   │
│                     │──SYS_FORK──────────>│                   │
│                     │<──child_endpoint────│                   │
│<──child_endpoint────│                     │                   │
│                     │                     │                   │
│──VFS_PM_FORK────────────────────────────────────────────────>│
│<──VFS_PM_FORK_REPLY─────────────────────────────────────────│
│                     │                     │                   │
│  (唤醒父进程)        │                     │                   │
```

### 关键约束

1. **PM 调用 vm_fork 后不能失败**：VM 已经调用了 sys_fork() 创建了内核进程
2. **Endpoint generation**：每次槽位重用时 generation 递增，防止旧消息误投递
3. **SUSPEND 机制**：PM 返回 SUSPEND 给内核，等待 VFS 回复后才唤醒父进程

---

## 第一阶段：MProc 结构体重构

**状态**: ✅ 已完成

**目标**: 将 C 的 `mproc` 结构体重构为 Rust 的分层 `Process` 结构体

### 1.1 C 源码分析

**文件**: `minix3/minix/servers/pm/mproc.h`

C 的 `mproc` 是一个扁平结构体，所有字段平铺：

```c
struct mproc {
  pid_t mp_pid;              /* 进程 ID */
  endpoint_t mp_endpoint;    /* 内核 endpoint */
  pid_t mp_procgrp;          /* 进程组 ID */
  uid_t mp_realuid;          /* 真实 UID */
  uid_t mp_effuid;           /* 有效 UID */
  // ... 30+ 字段平铺
};
```

### 1.2 Rust 分层设计

```rust
pub struct Process {
    pub identity: ProcessIdentity,   // 身份信息
    pub state: ProcessState,         // 状态机
    pub resources: ProcessResources, // 资源
    pub ipc: ProcessIpc,             // IPC 上下文
}
```

### 1.3 已实现文件

| 文件 | 内容 |
|------|------|
| `pm/src/mproc/mproc.rs` | Process 结构体定义 |
| `pm/src/mproc/lifecycle.rs` | 生命周期状态机 |
| `pm/src/mproc/block.rs` | 阻塞状态 |
| `pm/src/mproc/wait.rs` | 等待状态 |
| `pm/src/mproc/guardianship.rs` | 监护关系 |
| `pm/src/mproc/trace.rs` | 追踪状态 |
| `pm/src/mproc/signal.rs` | 信号状态 |
| `pm/src/mproc/credentials.rs` | 权限凭证 |

---

## 第二阶段：do_fork 前半部分

**状态**: ✅ 已完成

**目标**: 实现参数检查与槽位分配

### 2.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 第 60~82 行

```c
int do_fork(void) {
  register struct mproc *rmp;
  register struct mproc *rmc;
  static unsigned int next_child = 0;
  int n = 0;

  rmp = mp;

  // 1. 检查进程表是否已满
  if ((procs_in_use == NR_PROCS) ||
      (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    return(EAGAIN);
  }

  // 2. 查找空闲槽位
  do {
    next_child = (next_child+1) % NR_PROCS;
    n++;
  } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
}
```

### 2.2 Rust 实现

**文件**: `os/servers/pm/src/mproc/fork.rs`

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

---

## 第三阶段：PID 生成器

**状态**: ✅ 已完成

**目标**: 实现 `get_free_pid()` 的 Rust 版本

### 3.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c`

```c
pid_t get_free_pid() {
  static pid_t next_pid = INIT_PID + 1;
  register struct mproc *rmp;
  int t;

  do {
    t = 0;
    next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
    for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
      if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
        t = 1;
        break;
      }
  } while (t);

  return(next_pid);
}
```

### 3.2 Rust 实现

**文件**: `os/servers/pm/src/mproc/pid_gen.rs`

- 单调递增 + 冲突检测策略
- O(1) 期望，O(N) 最坏
- 检查 PID 与进程组 ID 冲突

---

## 第四阶段：进程结构复制与初始化

**状态**: ✅ 已完成

**目标**: 实现 `Process::fork_from()` — 显式构造子进程

### 4.1 核心实现

**文件**: `os/servers/pm/src/mproc/fork.rs`

```rust
impl Process {
    pub fn fork_from(
        parent: &Process, child_index: usize,
        child_pid: Pid, child_endpoint: Endpoint, parent_index: usize,
    ) -> Self {
        // 1. IDENTITY：继承 + 覆盖
        // 2. STATE：重置 + 关系
        // 3. RESOURCES：混合策略（权限继承，统计清零）
        // 4. IPC：默认
    }
}
```

### 4.2 关键修正

| 字段 | 之前（错误） | 之后（正确） |
|------|-------------|-------------|
| `identity.id.index` | `parent.identity.id.index` | `ProcIndex::new(child_index)` |
| `resources.started` | `parent.resources.started` | `getticks()` |
| `resources.intervals` | `parent.resources.intervals` | `[0; NR_ITIMERS]` |
| `resources.flags` | `parent.resources.flags` | 只保留 TAINTED |
| `resources.scheduler` | 直接继承 | 特权进程 → `Endpoint::RS` |
