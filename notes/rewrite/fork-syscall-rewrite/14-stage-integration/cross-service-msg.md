# 跨服务消息协议

> 本文档详细说明 fork 系统调用涉及的所有跨服务消息协议，包括消息格式、字段定义和流程说明。

---

## 1. 消息格式

### 1.1 Minix3 消息结构

```c
// Minix3 通用消息结构
typedef struct {
    int m_source;           // 发送者 endpoint
    int m_type;             // 消息类型
    union {
        // m1: 两个 int + 两个指针
        struct { int m1i1, m1i2; char *m1p1, *m1p2; } m_m1;
        // m2: 三个 int + 一个指针 + 一个 long
        struct { int m2i1, m2i2, m2i3; long m2l1; char *m2p1; } m_m2;
        // m3: 两个 int + 一个 off_t + 一个指针 + 一个 char[14]
        struct { int m3i1, m3i2; off_t m3l1; char *m3p1; char m3ca1[14]; } m_m3;
        // m4: 四个 long
        struct { long m4l1, m4l2, m4l3, m4l4; } m_m4;
        // m5: 一个 char + 五个 int
        struct { char m5c1; int m5i1, m5i2, m5i3, m5i4, m5i5; } m_m5;
        // m6: 六个 int
        struct { int m6i1, m6i2, m6i3, m6i4, m5i5, m6i6; } m_m6;
        // m7: 五个 int
        struct { int m7i1, m7i2, m7i3, m7i4, m7i5; } m_m7;
    };
} message;
```

---

## 2. PM → VM: VM_FORK

### 2.1 消息类型

```c
#define VM_FORK  (VM_BASE + 1)  // VM 基础消息号 + 1
```

### 2.2 消息字段

| 字段 | 类型 | 含义 | 方向 |
|------|------|------|------|
| `VMF_ENDPOINT` | `m1_i1` | 父进程 endpoint | 输入 |
| `VMF_SLOTNO` | `m1_i2` | 子进程槽号 | 输入 |
| `VMF_CHILD_ENDPOINT` | `m1_i3` | 子进程 endpoint（输出） | 输出 |

### 2.3 C 代码示例

```c
// PM 发送 VM_FORK
message m;
m.m_type = VM_FORK;
m.VMF_ENDPOINT = parent_endpoint;    // 父进程 endpoint
m.VMF_SLOTNO = child_slot;           // 子进程槽号

// 发送消息给 VM
ipc_sendrec(VM_PROC_NR, &m);

// 接收回复
child_endpoint = m.VMF_CHILD_ENDPOINT;  // VM 返回的子进程 endpoint
```

### 2.4 Rust 结构定义

```rust
pub struct VmForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: usize,
}

pub struct VmForkResponse {
    pub child_endpoint: Endpoint,
}
```

---

## 3. VM → Kernel: SYS_FORK

### 3.1 消息类型

```c
#define SYS_FORK  (KERNEL_CALL + 1)  // 内核调用基础号 + 1
```

### 3.2 消息字段

**输入（VM → Kernel）**:

| 字段 | 类型 | 含义 |
|------|------|------|
| `m_lsys_krn_sys_fork.endpt` | `int` | 父进程 endpoint |
| `m_lsys_krn_sys_fork.slot` | `int` | 子进程槽号 |
| `m_lsys_krn_sys_fork.flags` | `int` | 标志位（PFF_VMINHIBIT = 0x01）|

**输出（Kernel → VM）**:

| 字段 | 类型 | 含义 |
|------|------|------|
| `m_krn_lsys_sys_fork.endpt` | `int` | 子进程的新 endpoint |
| `m_krn_lsys_sys_fork.msgaddr` | `vir_bytes` | 子进程的消息缓冲区虚拟地址 |

### 3.3 标志位定义

```c
#define PFF_VMINHIBIT  0x01  // 设置 RTS_VMINHIBIT，等待 VM 设置页表
```

### 3.4 C 代码示例

```c
// VM 发送 SYS_FORK
message m;
m.m_type = SYS_FORK;
m.m_lsys_krn_sys_fork.endpt = parent_endpoint;
m.m_lsys_krn_sys_fork.slot = child_slot;
m.m_lsys_krn_sys_fork.flags = PFF_VMINHIBIT;  // 请求 VM 抑制

// 内核调用
kernel_call(&m);

// 接收回复
new_endpoint = m.m_krn_lsys_sys_fork.endpt;
msg_addr = m.m_krn_lsys_sys_fork.msgaddr;
```

### 3.5 Rust 结构定义

```rust
pub struct SysForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: usize,
    pub flags: SysForkFlags,
}

bitflags::bitflags! {
    pub struct SysForkFlags: u32 {
        const VMINHIBIT = 0x01;
    }
}

pub struct SysForkResponse {
    pub child_endpoint: Endpoint,
    pub msg_addr: VirBytes,
}
```

---

## 4. PM → VFS: VFS_PM_FORK

### 4.1 消息类型

```c
#define VFS_PM_FORK  (VFS_PM_BASE + 1)  // VFS-PM 基础消息号 + 1
```

### 4.2 消息字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `VFS_PM_ENDPT` | `m7_i1` | 子进程 endpoint |
| `VFS_PM_PENDPT` | `m7_i2` | 父进程 endpoint |
| `VFS_PM_CPID` | `m7_i3` | 子进程 PID |
| `VFS_PM_REUID` | `m7_i4` | 真实 uid（普通 fork 为 -1）|
| `VFS_PM_REGID` | `m7_i5` | 真实 gid（普通 fork 为 -1）|

### 4.3 C 代码示例

```c
// PM 发送 VFS_PM_FORK
message m;
m.m_type = VFS_PM_FORK;
m.VFS_PM_ENDPT = child_endpoint;    // 子进程 endpoint
m.VFS_PM_PENDPT = parent_endpoint;  // 父进程 endpoint
m.VFS_PM_CPID = child_pid;          // 子进程 PID
m.VFS_PM_REUID = -1;                // 普通 fork
m.VFS_PM_REGID = -1;                // 普通 fork

// 异步发送给 VFS
asynsend(VFS_PROC_NR, &m);
```

### 4.4 Rust 结构定义

```rust
pub struct VfsPmForkRequest {
    pub child_endpoint: Endpoint,
    pub parent_endpoint: Endpoint,
    pub child_pid: Pid,
    pub real_uid: Option<Uid>,  // None 表示 -1
    pub real_gid: Option<Gid>,  // None 表示 -1
}
```

---

## 5. VFS → PM: VFS_PM_FORK_REPLY

### 5.1 消息类型

```c
#define VFS_PM_FORK_REPLY  (VFS_PM_BASE + 2)
```

### 5.2 消息字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `VFS_PM_ENDPT` | `m7_i1` | 子进程 endpoint（确认）|

### 5.3 C 代码示例

```c
// VFS 发送回复
message m;
m.m_type = VFS_PM_FORK_REPLY;
m.VFS_PM_ENDPT = child_endpoint;

asynsend(PM_PROC_NR, &m);
```

---

## 6. 完整调用链

```
用户进程: fork()
    │
    ▼
PM: do_fork()                        [forkexit.c]
    ├── ① 检查进程表
    ├── ② 查找空闲槽位
    ├── ③ vm_fork() ──── IPC ────→ VM: do_fork()          [vm/fork.c]
    │   │                                ├── 验证 endpoint/slot
    │   │                                ├── *vmc = *vmp (复制 vmproc)
    │   │                                ├── pt_new() (新页表)
    │   │                                ├── map_proc_copy() (CoW)
    │   │                                │   └── map_copy_region()
    │   │                                │       └── pb_reference() (refcount++)
    │   │                                ├── sys_fork() ──→ Kernel: do_fork()  [do_fork.c]
    │   │                                │                    ├── *rpc = *rpp (复制 PCB)
    │   │                                │                    ├── generation++, 新 endpoint
    │   │                                │                    ├── retreg = 0 (子返回 0)
    │   │                                │                    ├── RTS_NO_QUANTUM
    │   │                                │                    ├── RTS_VMINHIBIT
    │   │                                │                    └── 特权降级
    │   │                                ├── pt_bind() (绑定页表, 清 RTS_VMINHIBIT)
    │   │                                └── 返回 child_endpoint
    │   └── ← child_endpoint
    ├── ④ *rmc = *rmp (复制 mproc)
    ├── ⑤ 修复 sigact 指针
    ├── ⑥ 设置父进程关系
    ├── ⑦ 清除追踪器
    ├── ⑧ 特权进程 scheduler
    ├── ⑨ 标志位过滤 (只保留 TAINTED)
    ├── ⑩ 重置统计/定时器
    ├── ⑪ get_free_pid()
    ├── ⑫ tell_vfs(VFS_PM_FORK) ──→ VFS: pm_fork()        [vfs/misc.c]
    │   │                                ├── fproc[child] = fproc[parent]
    │   │                                ├── filp_count++ (所有打开的 fd)
    │   │                                ├── fp_pid = cpid
    │   │                                ├── fp_flags = FP_NOFLAGS
    │   │                                ├── dup_vnode(fp_rd)
    │   │                                ├── dup_vnode(fp_wd)
    │   │                                └── 回复 VFS_PM_FORK_REPLY
    ├── ⑬ 追踪器 SIGSTOP
    └── ⑭ return SUSPEND
```

---

## 7. 消息时序图

```
PM                    VM                  Kernel               VFS
 │                     │                     │                   │
 │─── VM_FORK ────────>│                     │                   │
 │                     │─── SYS_FORK ───────>│                   │
 │                     │<── endpoint ────────│                   │
 │                     │                     │                   │
 │                     │─── pt_bind() ──────>│                   │
 │                     │                     │                   │
 │<── child_endpoint ──│                     │                   │
 │                     │                     │                   │
 │─── VFS_PM_FORK ──────────────────────────────────────────────>│
 │                     │                     │                   │
 │                     │                     │<── VFS_PM_FORK_REPLY
 │                     │                     │                   │
 │<── SUSPEND ─────────┼─────────────────────┼───────────────────┤
```

---

## 8. 错误处理

### 8.1 VM_FORK 错误

| 错误码 | 原因 | PM 处理 |
|--------|------|---------|
| `EINVAL` | 无效 endpoint/slot | 清理并返回错误 |
| `ENOMEM` | 内存不足 | 清理并返回错误 |

### 8.2 SYS_FORK 错误

| 错误码 | 原因 | VM 处理 |
|--------|------|---------|
| `EINVAL` | 无效参数 | 回滚并返回错误 |
| `EBUSY` | 槽位被占用 | panic（不应该发生）|

### 8.3 VFS_PM_FORK 错误

VFS_PM_FORK 是异步消息，错误通过 VFS_PM_FORK_REPLY 返回或记录在日志中。

---

## 9. 超时处理

### 9.1 同步调用超时

```rust
// VM_FORK 和 SYS_FORK 是同步调用
const FORK_TIMEOUT_MS: u64 = 5000;  // 5 秒超时

match ipc_sendrec_timeout(VM_PROC_NR, &mut msg, FORK_TIMEOUT_MS) {
    Ok(()) => { /* 成功 */ }
    Err(Timeout) => {
        // 超时处理：可能 VM 卡死
        panic!("VM_FORK timeout");
    }
}
```

### 9.2 异步调用超时

```rust
// VFS_PM_FORK 是异步调用，需要等待回复
const VFS_REPLY_TIMEOUT_MS: u64 = 5000;

// PM 需要设置状态机等待 VFS_PM_FORK_REPLY
pm_state.awaiting_vfs_reply = Some(child_slot);

// 超时后未收到回复，记录警告
```
