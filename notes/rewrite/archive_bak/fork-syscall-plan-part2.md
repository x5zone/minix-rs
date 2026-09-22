# Fork 系统调用纵向切片重构计划 — Part 2：PM 核心实现与 IPC 消息

> **范围**: 阶段 5~6（IPC 消息格式、PM→VM/VFS 协调）
> **前置**: 完成 [Part 1](fork-syscall-plan-part1.md) 中的阶段 1~4

---

## 第五阶段：IPC 消息格式定义

**状态**: ❌ 待实现

**目标**: 定义 fork 流程中所有跨服务 IPC 消息的 Rust 类型

### 5.1 Minix3 消息格式分析

#### PM → VM: VM_FORK

**文件**: `minix3/minix/include/minix/com.h`

```c
#define VM_FORK         (VM_RQ_BASE+1)
#  define VMF_ENDPOINT       m1_i1    /* 父进程 endpoint（输入） */
#  define VMF_SLOTNO         m1_i2    /* 子进程槽号（输入） */
#  define VMF_CHILD_ENDPOINT m1_i3    /* 子进程 endpoint（输出） */
```

#### VM → Kernel: SYS_FORK

**文件**: `minix3/minix/include/minix/com.h`

```c
/* 输入字段 */
m_lsys_krn_sys_fork.endpt    /* 父进程 endpoint */
m_lsys_krn_sys_fork.slot     /* 子进程槽号 */
m_lsys_krn_sys_fork.flags    /* fork 标志 (PFF_VMINHIBIT=0x01) */

/* 输出字段 */
m_krn_lsys_sys_fork.endpt    /* 子进程的新 endpoint */
m_krn_lsys_sys_fork.msgaddr  /* 子进程的消息缓冲区虚拟地址 */
```

#### PM → VFS: VFS_PM_FORK

```c
#define VFS_PM_FORK      (VFS_PM_RQ_BASE + 7)
#  define VFS_PM_ENDPT   m7_i1    /* 子进程 endpoint */
#  define VFS_PM_PENDPT  m7_i2    /* 父进程 endpoint */
#  define VFS_PM_CPID    m7_i3    /* 子进程 PID */
#  define VFS_PM_REUID   m7_i4    /* 真实 uid (-1 for regular fork) */
#  define VFS_PM_REGID   m7_i5    /* 真实 gid (-1 for regular fork) */
```

#### VFS → PM: VFS_PM_FORK_REPLY

```c
#define VFS_PM_FORK_REPLY     (VFS_PM_RS_BASE + 7)
#  define VFS_PM_ENDPT   /* 子进程 endpoint（确认） */
```

### 5.2 Rust 实现

**文件**: `os/libs/minix-types/src/ipc/messages.rs`（新建）

```rust
/// VM_FORK 请求消息
#[repr(C)]
pub struct VmForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: i32,
}

/// VM_FORK 响应消息
#[repr(C)]
pub struct VmForkResponse {
    pub child_endpoint: Endpoint,
}

/// SYS_FORK 请求消息
#[repr(C)]
pub struct SysForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: i32,
    pub flags: u32,
}

/// SYS_FORK 响应消息
#[repr(C)]
pub struct SysForkResponse {
    pub child_endpoint: Endpoint,
    pub msg_addr: VirBytes,
}

/// VFS_PM_FORK 请求消息
#[repr(C)]
pub struct VfsPmForkRequest {
    pub child_endpoint: Endpoint,
    pub parent_endpoint: Endpoint,
    pub child_pid: Pid,
    pub real_uid: i32,
    pub real_gid: i32,
}

/// VFS_PM_FORK 响应消息
#[repr(C)]
pub struct VfsPmForkResponse {
    pub child_endpoint: Endpoint,
}

/// PFF_VMINHIBIT 标志
pub const PFF_VMINHIBIT: u32 = 0x01;
```

### 5.3 验证目标

- [ ] 消息格式与 Minix3 C 代码一致
- [ ] 内存布局与 C 的 `m1`/`m7` 消息格式兼容
- [ ] 所有消息类型都有对应的 Rust 类型

---

## 第六阶段：PM→VM/VFS 协调

**状态**: ❌ 待实现

**目标**: 实现 PM 的完整 fork 流程，包括调用 VM 和通知 VFS

### 6.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 完整流程

```c
int do_fork(void) {
  // ① 参数检查
  // ② 查找空闲槽位
  // ③ 调用 VM fork
  if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;
  }
  // ④ 获取子进程槽位指针
  // ⑤ 复制父进程 mproc 到子进程
  // ⑥ 恢复 sigact 指针
  // ⑦ 设置父子关系
  // ⑧ 清除追踪器
  // ⑨ 特权进程处理
  // ⑩ 继承/重置标志位和统计信息
  // ⑪ 分配 PID
  // ⑫ 通知 VFS
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_FORK;
  m.VFS_PM_ENDPT = rmc->mp_endpoint;
  m.VFS_PM_PENDPT = rmp->mp_endpoint;
  m.VFS_PM_CPID = rmc->mp_pid;
  m.VFS_PM_REUID = -1;
  m.VFS_PM_REGID = -1;
  tell_vfs(rmc, &m);

  // ⑬ 如果有追踪器，发送 SIGSTOP
  if (rmc->mp_tracer != NO_TRACER)
    sig_proc(rmc, SIGSTOP, TRUE, FALSE);

  // ⑭ 返回 SUSPEND
  return SUSPEND;
}
```

### 6.2 Rust 实现

**文件**: `os/servers/pm/src/mproc/fork.rs`

```rust
/// SUSPEND 返回值
pub const SUSPEND: i32 = -1;

impl<'a> PmContext<'a> {
    /// fork 系统调用（完整流程）
    pub fn do_fork(&mut self) -> Result<i32, ForkError> {
        // 阶段 1: 检查和槽位分配（可以失败）
        let prepare = self.do_fork_prepare()?;

        // 阶段 2: VM fork（可以失败，但失败后需回滚槽位）
        let vm_result = self.call_vm_fork(
            self.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.table.release_slot(prepare.child_index);
            e
        })?;

        // 阶段 3: 进程结构初始化（不能失败！）
        self.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            vm_result.child_endpoint,
        );

        // 阶段 4: 通知 VFS（异步，不等待）
        self.notify_vfs_fork(
            vm_result.child_endpoint,
            self.current_proc().identity.endpoint,
            prepare.child_pid,
        );

        // 阶段 5: 返回 SUSPEND
        Ok(SUSPEND)
    }
}
```

### 6.3 验证目标

- [ ] PM 在 vm_fork 失败时正确回滚槽位
- [ ] PM 在 vm_fork 成功后不失败
- [ ] VFS 通知消息格式正确
- [ ] 返回 SUSPEND 正确

### 6.4 tell_vfs 实现分析

**文件**: `minix3/minix/servers/pm/utility.c`

```c
void tell_vfs(rmp, m_ptr)
struct mproc *rmp;
message *m_ptr;
{
  int r;
  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
    panic("tell_vfs: not idle: %d", m_ptr->m_type);

  r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
  if (r != OK)
    panic("unable to send to VFS: %d", r);

  rmp->mp_flags |= VFS_CALL;
}
```

关键点：
- 使用 `asynsend3`（异步发送，不等待回复）
- 设置 `VFS_CALL` 标志防止重复发送
- 如果进程已经有 `VFS_CALL` 或 `EVENT_CALL`，则 panic
