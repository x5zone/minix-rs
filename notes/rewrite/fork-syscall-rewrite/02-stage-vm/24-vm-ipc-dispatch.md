# 24-vm-ipc-dispatch: VM 的 IPC 请求分发

> **分类**: VM服务端
> **源码**: `minix3/minix/servers/vm/main.c` (CALLMAP + 主循环), `minix3/minix/include/minix/com.h` (请求码)
> **Rust 对应**: `os/servers/vm/src/ipc/dispatcher.rs`, `os/libs/minix-types/src/ipc/vm.rs`
> **说明**: VM 如何接收 IPC 消息、验证权限、分发到处理函数、返回结果

---

## 1. 概述

### 1.1 VM 的双重身份

VM 在 Minix3 微内核中既是**内存管理服务器**（接收 IPC 请求），也是**异步回调处理器**（接收 VFS 回复）。所有外部交互都通过 IPC 完成：

```
┌──────────────────────────────────────────────────────┐
│                    调用者                              │
│  PM / VFS / RS / 驱动 / 用户进程 / 内核(pagefault)   │
└──────────────┬───────────────────────────────────────┘
               │  同步 IPC (ipc_sendrec)
               ▼
┌──────────────────────────────────────────────────────┐
│              VM 主循环                                │
│  1. ipc_receive(ANY) 接收消息                         │
│  2. CALLNUMBER(m_type) → 请求码索引                   │
│  3. acl_check() 权限验证                              │
│  4. vm_calls[c].vmc_func(&msg) 分发处理               │
│  5. result != SUSPEND → ipc_reply()                  │
└──────────────────────────────────────────────────────┘
```

### 1.2 两种特殊消息

| 消息类型 | 来源 | 处理方式 | 是否回复 |
|---------|------|---------|---------|
| `VM_PAGEFAULT` | 内核 | 直接调用 `do_pagefaults()`，不经过 CALLMAP | 不回复（内核通过 `sys_vmctl` 解除阻塞） |
| `VM_VFS_REPLY` | VFS | 经过 CALLMAP → `do_vfs_reply()`，处理异步回调 | 不回复（VFS 不等待回复） |

### 1.3 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 03-acl | ACL 权限检查机制 |
| 18-vm-mmap | `VM_MMAP` / `VM_VFS_MMAP` 的服务端实现 |
| 19-vm-munmap | `VM_MUNMAP` / `VM_UNMAP_PHYS` / `VM_SHM_UNMAP` 的服务端实现 |
| 17-vm-brk | `VM_BRK` 的服务端实现 |
| 20-vm-exit | `VM_EXIT` / `VM_WILLEXIT` 的服务端实现 |
| 22-vm-queries | `VM_INFO` / `VM_GETPHYS` / `VM_GETREF` / `VM_GETRUSAGE` 的服务端实现 |
| 23-vfs-interaction | `VM_VFS_REPLY` 的异步回调机制 |
| 25-page-cache | `VM_MAPCACHEPAGE` 等缓存操作的服务端实现 |

---

## 2. C 源码分析

### 2.1 请求码定义

Minix3 在 `com.h` 中定义所有 VM 请求码，基址为 `0xC00`：

```c
/* com.h */
#define VM_RQ_BASE       0xC00

#define VM_EXIT          (VM_RQ_BASE+0)    /* PM → VM */
#define VM_FORK          (VM_RQ_BASE+1)    /* PM → VM */
#define VM_BRK           (VM_RQ_BASE+2)    /* 用户/PM → VM */
#define VM_EXEC_NEWMEM   (VM_RQ_BASE+3)    /* PM → VM */
#define VM_WILLEXIT      (VM_RQ_BASE+5)    /* PM → VM */
#define VM_MMAP          (VM_RQ_BASE+10)   /* 用户/驱动 → VM */
#define VM_ADDDMA        (VM_RQ_BASE+12)   /* — */
#define VM_DELDMA        (VM_RQ_BASE+13)   /* — */
#define VM_GETDMA        (VM_RQ_BASE+14)   /* — */
#define VM_MAP_PHYS      (VM_RQ_BASE+15)   /* 驱动 → VM */
#define VM_UNMAP_PHYS    (VM_RQ_BASE+16)   /* 驱动 → VM */
#define VM_MUNMAP        (VM_RQ_BASE+17)   /* 用户 → VM */
#define VM_MAPCACHEPAGE  (VM_RQ_BASE+26)   /* VFS/驱动 → VM */
#define VM_SETCACHEPAGE  (VM_RQ_BASE+27)   /* VFS/驱动 → VM */
#define VM_FORGETCACHEPAGE (VM_RQ_BASE+28) /* VFS/驱动 → VM */
#define VM_CLEARCACHE    (VM_RQ_BASE+29)   /* VFS/驱动 → VM */
#define VM_VFS_REPLY     (VM_RQ_BASE+30)   /* VFS → VM */
#define VM_REMAP         (VM_RQ_BASE+33)   /* — */
#define VM_SHM_UNMAP     (VM_RQ_BASE+34)   /* 用户 → VM */
#define VM_GETPHYS       (VM_RQ_BASE+35)   /* — */
#define VM_GETREF        (VM_RQ_BASE+36)   /* — */
#define VM_RS_SET_PRIV   (VM_RQ_BASE+37)   /* RS → VM */
#define VM_INFO          (VM_RQ_BASE+40)   /* — */
#define VM_RS_UPDATE     (VM_RQ_BASE+41)   /* RS → VM */
#define VM_RS_MEMCTL     (VM_RQ_BASE+42)   /* RS → VM */
#define VM_REMAP_RO      (VM_RQ_BASE+44)   /* — */
#define VM_PROCCTL       (VM_RQ_BASE+45)   /* — */
#define VM_VFS_MMAP      (VM_RQ_BASE+46)   /* VFS → VM */
#define VM_GETRUSAGE     (VM_RQ_BASE+47)   /* — */
#define VM_RS_PREPARE    (VM_RQ_BASE+48)   /* RS → VM */
#define VM_PAGEFAULT     (VM_RQ_BASE+0xff) /* 内核 → VM */
```

**`VM_BASIC_CALLS`**：用户进程默认可调用的请求子集（`com.h:778`）：
```c
#define VM_BASIC_CALLS \
    VM_BRK, VM_MMAP, VM_MUNMAP, VM_MAP_PHYS, VM_UNMAP_PHYS, VM_INFO, \
    VM_GETRUSAGE
```

### 2.2 CALLMAP 分发表

Minix3 使用函数指针数组 `vm_calls[]` 作为分发表（`main.c:48-51`）：

```c
static struct {
    int (*vmc_func)(message *);    /* Call handles message. */
    const char *vmc_name;          /* Human-readable string. */
} vm_calls[NR_VM_CALLS];
```

初始化时通过 `CALLMAP` 宏填充（`main.c:523-575`）：

```c
/* Basic VM calls. */
CALLMAP(VM_MMAP, do_mmap);
CALLMAP(VM_MUNMAP, do_munmap);
CALLMAP(VM_MAP_PHYS, do_map_phys);
CALLMAP(VM_UNMAP_PHYS, do_munmap);   /* 复用 do_munmap */

/* Calls from PM. */
CALLMAP(VM_EXIT, do_exit);
CALLMAP(VM_FORK, do_fork);
CALLMAP(VM_BRK, do_brk);
CALLMAP(VM_WILLEXIT, do_willexit);

CALLMAP(VM_PROCCTL, do_procctl_notrans);

/* Calls from VFS. */
CALLMAP(VM_VFS_REPLY, do_vfs_reply);
CALLMAP(VM_VFS_MMAP, do_vfs_mmap);

/* Calls from RS. */
CALLMAP(VM_RS_SET_PRIV, do_rs_set_priv);
CALLMAP(VM_RS_PREPARE, do_rs_prepare);
CALLMAP(VM_RS_UPDATE, do_rs_update);
CALLMAP(VM_RS_MEMCTL, do_rs_memctl);

/* Generic calls. */
CALLMAP(VM_REMAP, do_remap);
CALLMAP(VM_REMAP_RO, do_remap);      /* 复用 do_remap */
CALLMAP(VM_GETPHYS, do_get_phys);
CALLMAP(VM_SHM_UNMAP, do_munmap);    /* 复用 do_munmap */
CALLMAP(VM_GETREF, do_get_refcount);
CALLMAP(VM_INFO, do_info);

/* Cache blocks. */
CALLMAP(VM_MAPCACHEPAGE, do_mapcache);
CALLMAP(VM_SETCACHEPAGE, do_setcache);
CALLMAP(VM_FORGETCACHEPAGE, do_forgetcache);
CALLMAP(VM_CLEARCACHE, do_clearcache);

/* getrusage */
CALLMAP(VM_GETRUSAGE, do_getrusage);
```

**关键观察**：
- `VM_UNMAP_PHYS`、`VM_SHM_UNMAP` 都复用 `do_munmap`，但传入参数不同
- `VM_REMAP_RO` 复用 `do_remap`，通过只读标志区分
- `VM_PAGEFAULT` 不在 CALLMAP 中，主循环单独处理
- `VM_VFS_REPLY` 在 CALLMAP 中但返回 `SUSPEND`（不回复调用者）

### 2.3 主循环的消息处理

```c
/* main.c:130-184, 简化 */
who_e = msg.m_source;
vm_isokendpt(who_e, &caller_slot);

type = msg.m_type;
c = CALLNUMBER(type);    /* 将 0xC0A → 10 (0-based 索引) */
result = ENOSYS;

if(msg.m_source == VFS_PROC_NR && IS_VFS_FS_TRANSID(transid)) {
    /* VFS 带事务 ID 的请求 → do_procctl */
    result = do_procctl(&msg, transid);
} else if(msg.m_type == RS_INIT && msg.m_source == RS_PROC_NR) {
    /* RS 初始化请求 */
    result = do_sef_init_request(&msg);
    result = SUSPEND;  /* 不回复 RS */
} else if(msg.m_type == VM_PAGEFAULT) {
    /* 缺页异常：验证来自内核 */
    if(!IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)) {
        printf("VM: process %d faked VM_PAGEFAULT message!\n", msg.m_source);
    }
    do_pagefaults(&msg);
    continue;  /* 不回复，内核通过 sys_vmctl 解除阻塞 */
} else if(c < 0 || !vm_calls[c].vmc_func) {
    /* 无效请求码 → ENOSYS */
} else {
    /* 正常请求：ACL 检查 + 分发 */
    if(acl_check(&vmproc[caller_slot], c) != OK) {
        printf("VM: unauthorized %s by %d\n", vm_calls[c].vmc_name, who_e);
    } else {
        result = vm_calls[c].vmc_func(&msg);
    }
}

/* SUSPEND 抑制回复，其他都回复 */
if(result != SUSPEND) {
    msg.m_type = result;
    ipc_reply(who_e, &msg);
}
```

**SUSPEND 语义**：处理函数返回 `SUSPEND` 表示"稍后回复"——VM 不立即发送回复，而是等异步操作（如 VFS I/O）完成后由回调函数发送回复。这用于文件映射的缺页处理。

### 2.4 客户端-服务端对应关系

| 客户端调用 | IPC 请求 | 服务端处理函数 | 返回值 |
|-----------|---------|-------------|--------|
| `alloc_contig()` | `VM_MMAP` | `do_mmap()` | 虚拟地址 |
| `free_contig()` | `VM_MUNMAP` | `do_munmap()` | OK/错误 |
| `vm_map_phys()` | `VM_MAP_PHYS` | `do_map_phys()` | 虚拟地址 |
| `vm_unmap_phys()` | `VM_UNMAP_PHYS` | `do_munmap()` | OK/错误 |
| `vm_map_cacheblock()` | `VM_MAPCACHEPAGE` | `do_mapcache()` | 缓存块地址 |
| `vm_set_cacheblock()` | `VM_SETCACHEPAGE` | `do_setcache()` | OK/错误 |
| `vm_forget_cacheblock()` | `VM_FORGETCACHEPAGE` | `do_forgetcache()` | OK/错误 |
| `vm_clear_cache()` | `VM_CLEARCACHE` | `do_clearcache()` | OK/错误 |
| `brk()` | `VM_BRK` | `do_brk()` | 新 brk 地址 |
| `mmap()` | `VM_MMAP` | `do_mmap()` | 映射地址 |
| `munmap()` | `VM_MUNMAP` | `do_munmap()` | OK/错误 |

**注意**：客户端库函数（如 `alloc_contig`、`vm_map_phys`）是 `libsys` 中的 IPC 封装，不在 VM 服务端 rewrite 范围内。它们使用同步 IPC（`_taskcall` → `ipc_sendrec`），调用者阻塞直到 VM 处理完毕。

---

## 3. Rust 设计决策

### 3.1 类型安全分发替代函数指针数组

**C 方案**：`vm_calls[]` 函数指针数组 + `CALLNUMBER` 索引 + `vmc_func(msg)` 调用

**Rust 方案**：`MessageDispatcher` 结构体 + `match msg_type` 枚举分发 + 类型化的 `VmXxxIn`/`VmReply`

**理由**：
- C 的函数指针数组在编译时无法验证参数类型——所有处理函数都接收 `message*`，返回 `int`
- Rust 的 `match` 分发让每个处理函数接收**类型化的请求**（`VmMmapIn`、`VmMapPhysIn`），返回**类型化的结果**（`VmReply`）
- 编译器保证所有请求码都有对应的 `match` 分支（配合 `VmError::NotImplemented` 占位）

### 3.2 VmReply 统一返回类型

**C 方案**：处理函数返回 `int`（errno 值或 `OK` 或 `SUSPEND`），结果写入 `msg.m_type`

**Rust 方案**：`VmReply` 枚举，每个变体携带类型化的输出数据

**理由**：
- C 的 `message` 结构体是"万能容器"——所有请求和回复共享同一内存布局，字段含义由 `m_type` 决定
- Rust 的 `VmReply` 让每个回复类型在编译时确定，避免字段误用
- `VmReply::Suspend` 替代 C 的 `SUSPEND` 魔术返回值
- `VmReply::Error(VmError)` 替代 C 的 errno 返回

### 3.3 ACL 检查的位置

**C 方案**：主循环中 `acl_check()` 在 `vmc_func` 调用前执行

**Rust 方案**：ACL 检查在 `MessageDispatcher` 之外、IPC 接收层执行（参见 [03-acl.md](03-acl.md)）

**理由**：ACL 是横切关注点，不应与业务分发混在一起。Rust 的 `AclMask` bitflags + `AclState` 三态枚举比 C 的 `vm_acl` 整数更类型安全。

### 3.4 缓存操作的分发

`VM_MAPCACHEPAGE` / `VM_SETCACHEPAGE` / `VM_FORGETCACHEPAGE` / `VM_CLEARCACHE` 在 C 中通过 CALLMAP 分发到 `do_mapcache` 等函数。在 Rust 中，这些请求的 IPC 解码和分发框架已就位（`AclMask` 包含对应位），但服务端处理函数尚未实现。

---

## 4. Rust 实现详解

### 4.1 请求码定义（minix-types）

请求码定义在 `minix-types/src/ipc/vm.rs`，与 C 的 `com.h` 一一对应：

```rust
pub const VM_RQ_BASE: u32 = 0xC00;
pub const VM_EXIT: u32 = VM_RQ_BASE + 0;
pub const VM_FORK: u32 = VM_RQ_BASE + 1;
pub const VM_BRK: u32 = VM_RQ_BASE + 2;
// ... 所有请求码 ...
pub const VM_PAGEFAULT: u32 = VM_RQ_BASE + 0xff;
```

### 4.2 MessageDispatcher（dispatcher.rs）

`MessageDispatcher` 是无状态的分发器，每个 `dispatch_xxx` 方法接收类型化请求，调用对应处理函数，将结果映射为 `VmReply`：

```rust
pub(crate) struct MessageDispatcher;

impl MessageDispatcher {
    // fork — C returns EINVAL on vm_isokendpt failure (fork.c:44)
    pub(crate) fn dispatch_fork(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmForkIn,
    ) -> VmReply { ... }

    // mmap — 文件映射可能返回 Suspended
    pub(crate) fn dispatch_mmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMmapIn,
    ) -> VmReply {
        match mmap::handle_mmap(table, page_alloc, frames, &request) {
            Ok(mmap::MmapResult::Complete(response)) =>
                VmReply::Mmap(VmMmapOut { ret_addr: response.mapped_addr }),
            Ok(mmap::MmapResult::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(mmap_error_to_vm_error(e)),
        }
    }

    // map_phys — 纯同步，不会 Suspended
    pub(crate) fn dispatch_map_phys(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMapPhysIn,
    ) -> VmReply {
        match map_phys::handle_map_phys(
            table, page_alloc, frames,
            request.caller, request.target,
            request.phys_addr, request.length,
        ) {
            Ok(virt_addr) => VmReply::MapPhys(VmMapPhysOut { virt_addr }),
            Err(e) => VmReply::Error(map_phys_error_to_vm_error(e)),
        }
    }

    // unmap_phys / shm_unmap — 复用 munmap，与 C 一致
    pub(crate) fn dispatch_unmap_phys(...) -> VmReply {
        let req = munmap::MunmapRequest {
            endpoint: request.target,
            addr: request.vaddr,
            length: VirBytes(0),
            lookup_region_length: true,  // 长度从区域查找
        };
        match munmap::handle_munmap(table, page_alloc, frames, &req) { ... }
    }

    // pagefault / exec_newmem — 占位
    pub(crate) fn dispatch_pagefault(...) -> VmReply {
        VmReply::Error(VmError::NotImplemented)
    }
}
```

**与 C 的关键对应**：

| C 机制 | Rust 对应 |
|--------|----------|
| `vm_calls[c].vmc_func(&msg)` | `MessageDispatcher::dispatch_xxx(table, alloc, frames, request)` |
| `result = ENOSYS` | `VmReply::Error(VmError::NotImplemented)` |
| `result = SUSPEND` | `VmReply::Suspend` |
| `msg.m_type = result; ipc_reply()` | `VmReply` 枚举变体 |
| `VM_UNMAP_PHYS → do_munmap` | `dispatch_unmap_phys → munmap::handle_munmap` |
| `VM_SHM_UNMAP → do_munmap` | `dispatch_shm_unmap → munmap::handle_munmap` |

### 4.3 错误映射

每个服务模块定义自己的错误枚举，`dispatcher.rs` 负责映射为统一的 `VmError`：

```rust
fn mmap_error_to_vm_error(e: mmap::MmapError) -> VmError {
    match e {
        mmap::MmapError::ProcessNotFound => VmError::InvalidEndpoint,
        mmap::MmapError::OutOfMemory => VmError::OutOfMemory,
        mmap::MmapError::PermissionDenied => VmError::PermissionDenied,
        // ...
    }
}

fn fork_error_to_vm_error(e: fork::ForkError) -> VmError {
    match e {
        fork::ForkError::InvalidEndpoint => VmError::InvalidProcess,
        fork::ForkError::InvalidSlot => VmError::InvalidProcess,
        fork::ForkError::SlotInUse => VmError::SlotInUse,
        fork::ForkError::NoMemory => VmError::OutOfMemory,
        // ...
    }
}
```

**`VmError::InvalidEndpoint` vs `VmError::InvalidProcess`**：C 中 `vm_isokendpt` 失败在不同场景返回不同 errno：
- 大多数服务（fork/brk/exit/map_phys/rs/query）返回 `EINVAL` → Rust 映射为 `VmError::InvalidProcess`
- `do_mmap` 的 third-party 映射返回 `ESRCH`（mmap.c:216）→ Rust 映射为 `VmError::InvalidEndpoint`
- `do_getrusage` 返回 `ESRCH`（utility.c:442）→ Rust 映射为 `VmError::InvalidEndpoint`

**与 C 的对应**：C 中处理函数直接返回 errno（`EINVAL`、`ENOMEM` 等），Rust 通过 `VmError` 枚举替代，`VmError::to_errno()` 在 IPC 回复编码时转换为 errno。

### 4.4 ACL 权限检查

`AclMask` bitflags 覆盖所有 VM 请求码（`acl.rs`）：

```rust
bitflags::bitflags! {
    pub(crate) struct AclMask: u64 {
        const VM_MMAP = 1 << (VM_MMAP - VM_RQ_BASE);
        const VM_MAP_PHYS = 1 << (VM_MAP_PHYS - VM_RQ_BASE);
        const VM_MAPCACHEPAGE = 1 << (VM_MAPCACHEPAGE - VM_RQ_BASE);
        // ... 所有请求码 ...
    }
}
```

**与 C 的对应**：C 用 `vm_acl` 整数 + `vm_acl_bitmap[]` 位图，Rust 用 `AclState` 三态枚举（`Uninitialized`/`Default`/`System(AclMask)`）。

### 4.5 完整的请求分发状态

> **状态说明**：
> - "已实现" = dispatch_by_number 已连接 + handler 逻辑完整
> - "已连接/部分实现" = dispatch_by_number 已连接 M1/M2 解码，但 handler 内部有 stub（如 RS_PREPARE 返回 NotImplemented）
> - "占位" = dispatch_by_number 已连接但 handler 返回 NotImplemented
> - "TODO" = dispatch_by_number 未连接，走默认 NotImplemented 分支

| 请求码 | Rust 处理函数 | dispatcher 方法 | 状态 |
|--------|-------------|----------------|------|
| `VM_FORK` | `fork::do_fork` | `dispatch_fork` | 已实现 |
| `VM_BRK` | `brk::handle_brk` | `dispatch_brk` | 已实现 |
| `VM_MMAP` | `mmap::handle_mmap` | `dispatch_mmap` | 已实现 |
| `VM_VFS_MMAP` | `mmap::handle_vfs_mmap` | `dispatch_vfs_mmap` | 已实现 (mmap.rs:273) |
| `VM_MUNMAP` | `munmap::handle_munmap` | `dispatch_munmap` | 已实现 |
| `VM_MAP_PHYS` | `map_phys::handle_map_phys` | `dispatch_map_phys` | 已实现 |
| `VM_UNMAP_PHYS` | `munmap::handle_munmap` | `dispatch_unmap_phys` | 占位（NotImplemented） |
| `VM_SHM_UNMAP` | `munmap::handle_munmap` | `dispatch_shm_unmap` | 已连接/部分实现（fail-closed 校验 + NotImplemented，见 S-19-FU） |
| `VM_EXIT` | `exit::handle_vm_exit` | `dispatch_exit` | 已实现 |
| `VM_WILLEXIT` | `exit::handle_vm_willexit` | `dispatch_willexit` | 已实现 |
| `VM_RS_SET_PRIV` | `rs::handle_rs_set_priv` | `dispatch_rs_set_priv` | 已连接/部分实现 |
| `VM_RS_PREPARE` | `rs::handle_rs_prepare` | `dispatch_rs_prepare` | 已连接/部分实现 |
| `VM_RS_UPDATE` | `rs::handle_rs_update` | `dispatch_rs_update` | 已连接/部分实现 |
| `VM_RS_MEMCTL` | `rs::handle_rs_memctl` | `dispatch_rs_memctl` | 已连接/部分实现 |
| `VM_INFO` | `query::handle_info` | `dispatch_info` | 已连接/部分实现 |
| `VM_GETPHYS` | `query::handle_get_phys` | `dispatch_get_phys` | 已连接/部分实现 |
| `VM_GETREF` | `query::handle_get_refcount` | `dispatch_get_refcount` | 已连接/部分实现 |
| `VM_GETRUSAGE` | `query::handle_getrusage` | `dispatch_getrusage` | 已连接/部分实现 |
| `VM_PAGEFAULT` | — | `dispatch_pagefault` | 占位（VmServer 主循环处理） |
| `VM_EXEC_NEWMEM` | — | `dispatch_exec_newmem` | 占位（NotImplemented） |
| `VM_MAPCACHEPAGE` | `page_cache::handle_mapcache` | `dispatch_mapcache` | 已连接/部分实现 |
| `VM_SETCACHEPAGE` | `page_cache::handle_setcache` | `dispatch_setcache` | 已连接/部分实现 |
| `VM_FORGETCACHEPAGE` | `page_cache::handle_forgetcache` | `dispatch_forgetcache` | 已实现 (dispatcher.rs:231) |
| `VM_CLEARCACHE` | `page_cache::handle_clearcache` | `dispatch_clearcache` | 已实现 (dispatcher.rs:241) |
| `VM_VFS_REPLY` | `vfs_queue::handle_reply` | `dispatch_vfs_reply` | 已连接/部分实现（fail-closed 校验 + NotImplemented，见 S-19-FU） |
| `VM_REMAP` | — | `dispatch_remap` | 已连接/部分实现（fail-closed 校验 + NotImplemented，见 S-19-FU） |
| `VM_REMAP_RO` | — | `dispatch_remap_ro` | 已连接/部分实现（fail-closed 校验 + NotImplemented，见 S-19-FU） |
| `VM_PROCCTL` | — | `dispatch_procctl` | 已连接/部分实现（fail-closed 校验 + NotImplemented，见 S-19-FU） |
| `VM_ADDDMA` | — | — | TODO |
| `VM_DELDMA` | — | — | TODO |
| `VM_GETDMA` | — | — | TODO |

### 4.6 VmServer 主循环

`VmServer` 的主循环（`vm_server.rs`）负责 IPC 收发和 VFS 回复的特殊处理：

```rust
pub fn main_loop(&mut self) {
    loop {
        // IPC 收发通过 IpcTransport 策略 trait (os/servers/vm/src/ipc/transport.rs)
        // 实现 (2026-06-13 修复 IPC stub). 旧实现是直接 panic 的 stub.
        let (msg, _sts) = ipc_receive().expect("IPC transport must be wired");
        // IpcTransport 抽象说明:
        //   - 生产: KernelIpcTransport::receive (依赖 kernel IPC core)
        //   - 测试: TestIpcTransport::receive (队列式 mock, 单元测试驱动主循环)
        // 旧 panic 字符串已被清晰 "wiring pending kernel IPC core" 错误替代.

        // 1. VM_PAGEFAULT: 内核专用，不经过 dispatcher
        // 2. VM_VFS_REPLY: 异步回调，不经过 dispatcher
        // 3. 其他请求: MessageDispatcher 分发

        let reply = match msg.m_type {
            VM_PAGEFAULT => { /* handle_pagefault */ }
            VM_VFS_REPLY => { /* vfs_queue.handle_reply */ }
            _ => MessageDispatcher::dispatch_xxx(...)
        };

        if reply != VmReply::Suspend {
            ipc_send(msg.m_source, &reply).expect("IPC send must succeed");
        }
    }
}
```

**与 C 主循环的对应**：

| C 路径 | Rust 路径 |
|--------|----------|
| `VM_PAGEFAULT` → `do_pagefaults()` → `continue` | `VM_PAGEFAULT` → `handle_pagefault()` → 不回复 |
| `VM_VFS_REPLY` → `do_vfs_reply()` → `SUSPEND` | `VM_VFS_REPLY` → `vfs_queue.handle_reply()` → 不回复 |
| `vm_calls[c].vmc_func(&msg)` | `MessageDispatcher::dispatch_xxx()` |
| `acl_check()` → `ENOSYS` | `AclState::acl_check()` → `VmError::PermissionDenied` |
| `result != SUSPEND → ipc_reply()` | `reply != VmReply::Suspend → ipc_reply()` |

---

## 5. 缓存操作的 IPC 接口

> 缓存操作（`VM_MAPCACHEPAGE` 等）的服务端处理函数尚未实现。
> 本节记录 C 源码中的接口定义，为后续实现提供参考。

### 5.1 C 源码中的缓存 IPC 接口

| 请求码 | 客户端函数 | 服务端处理 | 功能 |
|--------|-----------|-----------|------|
| `VM_MAPCACHEPAGE` | `vm_map_cacheblock()` | `do_mapcache()` | 获取缓存页的虚拟地址 |
| `VM_SETCACHEPAGE` | `vm_set_cacheblock()` | `do_setcache()` | 将数据注册到缓存 |
| `VM_FORGETCACHEPAGE` | `vm_forget_cacheblock()` | `do_forgetcache()` | 使缓存项失效 |
| `VM_CLEARCACHE` | `vm_clear_cache()` | `do_clearcache()` | 清除某设备的所有缓存 |

### 5.2 Rust 中的已有基础设施

- `PageCache`（`page_cache.rs`）：`find_by_inode`/`find_by_device`/`insert`/`remove`/`increase_refcount`/`decrease_refcount`
- `CacheMemory` memtype（`memtype.rs`）：骨架实现
- `AclMask::VM_MAPCACHEPAGE` / `VM_SETCACHEPAGE` / `VM_FORGETCACHEPAGE` / `VM_CLEARCACHE`：权限位已定义

### 5.3 实现路径

缓存操作的实现需要：
1. 在 `dispatcher.rs` 中添加 `dispatch_mapcache`/`dispatch_setcache`/`dispatch_forgetcache`/`dispatch_clearcache` 方法
2. 在 `minix-types/src/ipc/vm.rs` 中添加 `VmMapcacheIn`/`VmSetcacheIn` 等请求类型
3. 实现服务端处理函数，调用 `PageCache` 和 `CacheMemory`

---

## 6. 测试要点

### 6.1 分发正确性

- 每个 `dispatch_xxx` 方法调用正确的处理函数
- 错误映射正确：每个服务错误枚举变体映射到正确的 `VmError`
- `VmReply::Suspend` 仅在 `mmap` 文件映射和 `rs_update` 路径返回

### 6.2 复用验证

- `VM_UNMAP_PHYS` 和 `VM_SHM_UNMAP` 复用 `munmap::handle_munmap`，但参数不同（`lookup_region_length: true`）
- `VM_MUNMAP` 使用 `lookup_region_length: false`

### 6.3 权限检查

- 未授权请求返回 `VmError::PermissionDenied`
- `AclState::Uninitialized` 进程使用默认权限（`VM_BASIC_CALLS`）

---

## 7. 参见

| 文档 | 内容 |
|------|------|
| [03-acl.md](03-acl.md) | ACL 权限检查机制 |
| [18-vm-mmap.md](18-vm-mmap.md) | `VM_MMAP` / `VM_VFS_MMAP` 实现 |
| [19-vm-munmap.md](19-vm-munmap.md) | `VM_MUNMAP` / `VM_UNMAP_PHYS` / `VM_SHM_UNMAP` 实现 |
| [22-vm-queries.md](22-vm-queries.md) | `VM_INFO` / `VM_GETPHYS` / `VM_GETREF` / `VM_GETRUSAGE` 实现 |
| [23-vfs-interaction.md](23-vfs-interaction.md) | `VM_VFS_REPLY` 异步回调机制 |
| [25-page-cache.md](25-page-cache.md) | 页缓存数据结构和缓存 IPC 处理 |
