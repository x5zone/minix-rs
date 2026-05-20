# 24-client-alloc-lib: 其他服务器怎么用 VM 分配内存

> **分类**: VM客户端库
> **源码**: `minix3/minix/lib/libsys/alloc_util.c`, `vm_map_phys.c`, `vm_cache.c`
> **Rust 对应**: `minix_alloc` crate 设计
> **说明**: 驱动、服务器如何通过 IPC 向 VM 请求内存分配、物理映射、页缓存

---

## 1. 概述

### 1.1 谁需要 VM 分配内存

VM 不仅是用户进程的内存管理器，也是**整个系统中所有服务器和驱动的内存提供者**：

| 调用者 | 需求 | 使用的 VM 接口 |
|--------|------|---------------|
| 用户进程 | 堆扩展、文件映射 | `VM_BRK`, `VM_MMAP` |
| PM（进程管理器） | fork/exec 内存 | `VM_FORK`, `VM_EXIT` |
| VFS（文件系统） | 文件映射协作 | `VM_VFS_REPLY`, `VM_VFS_MMAP` |
| RS（重启服务器） | 服务更新 | `VM_RS_UPDATE`, `VM_RS_MEMCTL` |
| 驱动程序 | DMA 缓冲区、设备寄存器映射 | `VM_MAP_PHYS`, `alloc_contig` |
| 网络驱动 | 连续物理内存 | `alloc_contig` (MAP_PREALLOC|MAP_CONTIG) |
| 块设备驱动 | 页缓存 | `VM_MAPCACHEPAGE`, `VM_SETCACHEPAGE` |
| TTY 驱动 | 显存映射 | `VM_MAP_PHYS` |

**核心问题**：这些调用者不在 VM 进程内，无法直接调用 `alloc_mem()`。它们必须通过 IPC 向 VM 发送请求，VM 处理后返回结果。

### 1.2 客户端库的层次

```
┌─────────────────────────────────────────┐
│  应用层: 驱动/服务器代码                  │
│  buf = alloc_contig(4096, ...);          │
│  vaddr = vm_map_phys(SELF, phys, len);   │
├─────────────────────────────────────────┤
│  客户端库: libsys / minix_alloc crate    │
│  封装 IPC 消息构造和发送                   │
├─────────────────────────────────────────┤
│  IPC 层: _taskcall(VM_PROC_NR, ...)      │
│  同步 IPC: 发送请求 → 等待回复             │
├─────────────────────────────────────────┤
│  VM 服务端: do_mmap / do_map_phys / ...  │
│  处理请求 → 返回结果                       │
└─────────────────────────────────────────┘
```

### 1.3 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 22-vm-munmap | `VM_MAP_PHYS` / `VM_UNMAP_PHYS` 的服务端实现 |
| 23-vfs-interaction | `VM_MAPCACHEPAGE` / `VM_SETCACHEPAGE` 的 VFS 协作 |
| 21-vm-brk-complete | `VM_BRK` 的服务端实现 |
| 08 slab 分配器 | VM 自身使用 slab 分配内核堆 |

---

## 2. C 源码分析

### 2.1 alloc_contig / free_contig — 连续物理内存

```c
/* libsys/alloc_util.c */
void *alloc_contig(size_t len, int flags, phys_bytes *phys)
{
    void* buf;
    int mmapflags = MAP_PREALLOC|MAP_CONTIG|MAP_ANON;

    if(flags & AC_LOWER16M)  mmapflags |= MAP_LOWER16M;
    if(flags & AC_LOWER1M)   mmapflags |= MAP_LOWER1M;
    if(flags & AC_ALIGN64K)  mmapflags |= MAP_ALIGNMENT_64KB;

    /* 通过 mmap 系统调用向 VM 请求连续物理内存 */
    buf = sef_llvm_ac_mmap(0, len, PROT_READ|PROT_WRITE,
        mmapflags, -1, 0);
    if(buf == MAP_FAILED) return NULL;

    /* 获取物理地址（如果调用者需要） */
    if(phys != NULL && sys_umap(SELF, VM_D, (vir_bytes)buf, len, phys) != OK)
        panic("sys_umap_data_fb failed");

    return buf;
}

int free_contig(void *addr, size_t len)
{
    return sef_llvm_ac_munmap(addr, len);
}
```

**调用链**：

```
驱动代码: buf = alloc_contig(4096, 0, &phys)
  │
  ▼
sef_llvm_ac_mmap(0, 4096, PROT_RW, MAP_PREALLOC|MAP_CONTIG|MAP_ANON, -1, 0)
  │
  ▼  IPC: VM_MMAP
  │
VM do_mmap():
  ├── MAP_ANON → 匿名映射
  ├── MAP_CONTIG → mem_type_anon_contig
  ├── MAP_PREALLOC → MF_PREALLOC → map_handle_memory() 预分配
  └── 返回虚拟地址

驱动代码: sys_umap(SELF, VM_D, buf, 4096, &phys)
  │
  ▼  内核系统调用: 查询虚拟地址对应的物理地址
  │
内核: 返回物理地址
```

**关键标志**：

| 标志 | 含义 | VM 处理 |
|------|------|--------|
| `MAP_PREALLOC` | 立即分配物理页 | `map_handle_memory()` 预分配所有页 |
| `MAP_CONTIG` | 物理连续 | `mem_type_anon_contig` 一次性分配连续物理块 |
| `MAP_ANON` | 匿名内存 | 不关联文件 |
| `MAP_LOWER16M` | 16MB 以下 | ISA DMA 限制 |
| `MAP_LOWER1M` | 1MB 以下 | 传统 DMA 限制 |
| `MAP_ALIGNMENT_64KB` | 64KB 对齐 | 物理地址 64KB 对齐 |

### 2.2 vm_map_phys / vm_unmap_phys — 物理内存映射

```c
/* libsys/vm_map_phys.c */
void *vm_map_phys(endpoint_t who, void *phaddr, size_t len)
{
    message m;
    memset(&m, 0, sizeof(m));
    m.m_lsys_vm_map_phys.ep = who;
    m.m_lsys_vm_map_phys.phaddr = (phys_bytes)phaddr;
    m.m_lsys_vm_map_phys.len = len;

    /* 同步 IPC: 发送 VM_MAP_PHYS 请求 */
    r = _taskcall(VM_PROC_NR, VM_MAP_PHYS, &m);
    if (r != OK) return MAP_FAILED;

    return m.m_lsys_vm_map_phys.reply;
}

int vm_unmap_phys(endpoint_t who, void *vaddr, size_t len)
{
    message m;
    memset(&m, 0, sizeof(m));
    m.m_lsys_vm_unmap_phys.ep = who;
    m.m_lsys_vm_unmap_phys.vaddr = vaddr;

    /* 同步 IPC: 发送 VM_UNMAP_PHYS 请求 */
    r = _taskcall(VM_PROC_NR, VM_UNMAP_PHYS, &m);
    return r;
}
```

**典型使用场景**：

```
驱动: 映射设备寄存器
  vaddr = vm_map_phys(SELF, 0xFE000000, 0x1000);
  *(volatile u32*)vaddr = 0x1234;   /* 通过虚拟地址访问设备寄存器 */

驱动: 映射 DMA 缓冲区给设备
  buf = alloc_contig(4096, 0, &phys);
  /* phys 是物理地址，设备用 phys 访问 */
  /* buf 是虚拟地址，驱动用 buf 访问 */
```

### 2.3 vm_map_cacheblock / vm_set_cacheblock — 页缓存

```c
/* libsys/vm_cache.c */
void *vm_map_cacheblock(dev_t dev, off_t dev_offset,
    ino_t ino, off_t ino_offset, u32_t *flags, int blocksize)
{
    message m;
    /* 构造 VM_MAPCACHEPAGE 请求 */
    vm_cachecall(&m, VM_MAPCACHEPAGE, NULL, dev, dev_offset,
        ino, ino_offset, flags, blocksize, 0);
    return m.m_vmmcp_reply.addr;
}

int vm_set_cacheblock(void *block, dev_t dev, off_t dev_offset,
    ino_t ino, off_t ino_offset, u32_t *flags, int blocksize, int setflags)
{
    message m;
    return vm_cachecall(&m, VM_SETCACHEPAGE, block, dev, dev_offset,
        ino, ino_offset, flags, blocksize, setflags);
}

int vm_forget_cacheblock(dev_t dev, off_t dev_offset, int blocksize)
{
    message m;
    return vm_cachecall(&m, VM_FORGETCACHEPAGE, NULL, dev, dev_offset,
        VMC_NO_INODE, 0, 0, blocksize, 0);
}

int vm_clear_cache(dev_t dev)
{
    message m;
    m.m_vmmcp.dev = dev;
    return _taskcall(VM_PROC_NR, VM_CLEARCACHE, &m);
}
```

**页缓存操作**：

| 操作 | IPC 请求 | 说明 |
|------|---------|------|
| 映射缓存块 | `VM_MAPCACHEPAGE` | 获取一个缓存页的虚拟地址 |
| 设置缓存块 | `VM_SETCACHEPAGE` | 将数据注册到缓存 |
| 忘记缓存块 | `VM_FORGETCACHEPAGE` | 使缓存项失效 |
| 清除设备缓存 | `VM_CLEARCACHE` | 清除某设备的所有缓存 |

### 2.4 VM 的完整 IPC 接口一览

从 `com.h` 和 `main.c` 的 CALLMAP 整理：

| 请求码 | 名称 | 调用者 | 功能 |
|--------|------|--------|------|
| 0xC00 | `VM_EXIT` | PM | 进程退出 |
| 0xC01 | `VM_FORK` | PM | 进程 fork |
| 0xC02 | `VM_BRK` | 用户/PM | 堆调整 |
| 0xC05 | `VM_WILLEXIT` | PM | 进程即将退出 |
| 0xC0A | `VM_MMAP` | 用户/驱动 | 内存映射 |
| 0xC0C | `VM_ADDDMA` | — | DMA 添加 |
| 0xC0D | `VM_DELDMA` | — | DMA 删除 |
| 0xC0E | `VM_GETDMA` | — | DMA 查询 |
| 0xC0F | `VM_MAP_PHYS` | 驱动 | 物理内存映射 |
| 0xC10 | `VM_UNMAP_PHYS` | 驱动 | 取消物理映射 |
| 0xC11 | `VM_MUNMAP` | 用户 | 取消映射 |
| 0xC1A | `VM_MAPCACHEPAGE` | VFS/驱动 | 映射缓存页 |
| 0xC1B | `VM_SETCACHEPAGE` | VFS/驱动 | 设置缓存页 |
| 0xC1C | `VM_FORGETCACHEPAGE` | VFS/驱动 | 忘记缓存页 |
| 0xC1D | `VM_CLEARCACHE` | VFS/驱动 | 清除设备缓存 |
| 0xC1E | `VM_VFS_REPLY` | VFS | VFS 回复 |
| 0xC21 | `VM_REMAP` | — | 重新映射 |
| 0xC22 | `VM_SHM_UNMAP` | 用户 | 共享内存取消映射 |
| 0xC23 | `VM_GETPHYS` | — | 获取物理地址 |
| 0xC24 | `VM_GETREF` | — | 获取引用计数 |
| 0xC25 | `VM_RS_SET_PRIV` | RS | RS 设置权限 |
| 0xC28 | `VM_INFO` | — | VM 信息查询 |
| 0xC29 | `VM_RS_UPDATE` | RS | RS 更新 |
| 0xC2A | `VM_RS_MEMCTL` | RS | RS 内存控制 |
| 0xC2C | `VM_REMAP_RO` | — | 只读重新映射 |
| 0xC2D | `VM_PROCCTL` | — | 进程控制 |
| 0xC2E | `VM_VFS_MMAP` | VFS | VFS mmap |
| 0xC2F | `VM_GETRUSAGE` | — | 获取资源使用 |
| 0xC30 | `VM_RS_PREPARE` | RS | RS 准备 |
| 0xCFF | `VM_PAGEFAULT` | 内核 | 缺页异常 |

### 2.5 _taskcall — 同步 IPC 的底层

```c
/* 所有客户端库函数最终都调用 _taskcall */
int _taskcall(endpoint_t who, int callnr, message *msg)
{
    msg->m_type = callnr;
    return ipc_sendrec(who, msg);  /* 同步发送+接收 */
}
```

**同步 vs 异步**：
- 客户端库使用**同步 IPC**（`ipc_sendrec`）：发送请求后阻塞等待回复
- VM 内部的 VFS 请求使用**异步 IPC**（`asynsend3`）：发送后不等待

这意味着客户端调用 `vm_map_phys()` 时，调用者会阻塞直到 VM 处理完毕。对于驱动来说这是可接受的——驱动需要映射结果才能继续工作。

---

## 3. Rust 设计：minix_alloc crate

### 3.1 设计目标

将 Minix3 的 `libsys` 客户端库重新设计为 Rust crate `minix_alloc`，提供类型安全的内存分配接口。

### 3.2 crate 结构

```
minix_alloc/
├── Cargo.toml
├── src/
│   ├── lib.rs          # 公共接口
│   ├── contig.rs       # alloc_contig / free_contig
│   ├── map_phys.rs     # vm_map_phys / vm_unmap_phys
│   ├── cache.rs        # vm_map_cacheblock / vm_set_cacheblock
│   ├── mmap.rs         # mmap / munmap
│   ├── ipc.rs          # IPC 底层封装
│   └── error.rs        # 错误类型
```

### 3.3 公共接口

```rust
//! minix_alloc: VM 客户端内存分配库
//!
//! 提供其他服务器和驱动向 VM 请求内存分配的类型安全接口。

pub mod contig;
pub mod map_phys;
pub mod cache;
pub mod mmap;

pub use error::AllocError;

/// VM 服务端的 endpoint
const VM_PROC_NR: i32 = 8;

/// 页大小
const PAGE_SIZE: usize = 4096;
```

### 3.4 contig.rs — 连续物理内存分配

```rust
use core::ptr::NonNull;

/// 连续内存分配标志
#[derive(Debug, Clone, Copy, Default)]
pub struct ContigFlags(u32);

impl ContigFlags {
    pub const LOWER16M: Self = Self(0x01);
    pub const LOWER1M: Self = Self(0x02);
    pub const ALIGN64K: Self = Self(0x04);

    pub fn empty() -> Self { Self(0) }
}

/// 分配连续物理内存
///
/// 对应 Minix3 的 `alloc_contig()`。
/// 返回 (虚拟地址, 物理地址) 对。
pub fn alloc_contig(
    len: usize,
    flags: ContigFlags,
) -> Result<(NonNull<u8>, u64), AllocError> {
    if len == 0 {
        return Err(AllocError::InvalidLength);
    }

    let mut mmap_flags = MmapFlags::PREALLOC | MmapFlags::CONTIG | MmapFlags::ANON;

    if flags.contains(ContigFlags::LOWER16M) {
        mmap_flags |= MmapFlags::LOWER16M;
    }
    if flags.contains(ContigFlags::LOWER1M) {
        mmap_flags |= MmapFlags::LOWER1M;
    }
    if flags.contains(ContigFlags::ALIGN64K) {
        mmap_flags |= MmapFlags::ALIGN64K;
    }

    let vaddr = ipc_mmap(0, len, ProtFlags::READ | ProtFlags::WRITE,
        mmap_flags, -1, 0)?;

    let phys = sys_umap(vaddr, len)?;

    Ok((vaddr, phys))
}

/// 释放连续物理内存
pub fn free_contig(addr: NonNull<u8>, len: usize) -> Result<(), AllocError> {
    ipc_munmap(addr, len)
}
```

### 3.5 map_phys.rs — 物理内存映射

```rust
use core::ptr::NonNull;

/// 将物理地址映射到调用者的地址空间
///
/// 对应 Minix3 的 `vm_map_phys()`。
pub fn map_phys(
    phys_addr: u64,
    len: usize,
) -> Result<NonNull<u8>, AllocError> {
    map_phys_for(Endpoint::SELF, phys_addr, len)
}

/// 将物理地址映射到指定进程的地址空间
pub fn map_phys_for(
    target: Endpoint,
    phys_addr: u64,
    len: usize,
) -> Result<NonNull<u8>, AllocError> {
    if len == 0 {
        return Err(AllocError::InvalidLength);
    }

    let reply = ipc_vm_map_phys(target, phys_addr, len)?;

    NonNull::new(reply as *mut u8).ok_or(AllocError::MapFailed)
}

/// 取消物理内存映射
pub fn unmap_phys(vaddr: NonNull<u8>) -> Result<(), AllocError> {
    unmap_phys_for(Endpoint::SELF, vaddr)
}

/// 取消指定进程的物理内存映射
pub fn unmap_phys_for(
    target: Endpoint,
    vaddr: NonNull<u8>,
) -> Result<(), AllocError> {
    ipc_vm_unmap_phys(target, vaddr)
}
```

### 3.6 cache.rs — 页缓存操作

```rust
/// 缓存块信息
pub struct CacheBlock {
    pub addr: *mut u8,
    pub dev: u64,
    pub dev_offset: u64,
    pub ino: u64,
    pub ino_offset: u64,
    pub flags: u32,
}

/// 映射一个缓存块
pub fn map_cache_block(
    dev: u64,
    dev_offset: u64,
    ino: u64,
    ino_offset: u64,
    flags: &mut u32,
    block_size: usize,
) -> Result<*mut u8, AllocError> {
    if block_size % PAGE_SIZE != 0 {
        return Err(AllocError::NotAligned);
    }

    ipc_vm_map_cache_page(dev, dev_offset, ino, ino_offset, flags, block_size)
}

/// 注册一个缓存块
pub fn set_cache_block(
    block_addr: *mut u8,
    dev: u64,
    dev_offset: u64,
    ino: u64,
    ino_offset: u64,
    flags: &mut u32,
    block_size: usize,
    set_flags: u32,
) -> Result<(), AllocError> {
    ipc_vm_set_cache_page(block_addr, dev, dev_offset, ino, ino_offset,
        flags, block_size, set_flags)
}

/// 使一个缓存块失效
pub fn forget_cache_block(
    dev: u64,
    dev_offset: u64,
    block_size: usize,
) -> Result<(), AllocError> {
    ipc_vm_forget_cache_page(dev, dev_offset, block_size)
}

/// 清除某设备的所有缓存
pub fn clear_cache(dev: u64) -> Result<(), AllocError> {
    ipc_vm_clear_cache(dev)
}
```

### 3.7 error.rs — 错误类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    OutOfMemory,
    InvalidLength,
    NotAligned,
    PermissionDenied,
    MapFailed,
    UnmapFailed,
    IpcError(i32),
    NotFound,
}
```

### 3.8 ipc.rs — IPC 底层封装

```rust
use minix_ipc::{Message, taskcall};

const VM_PROC_NR: i32 = 8;

const VM_MMAP: i32 = 0xC0A;
const VM_MUNMAP: i32 = 0xC11;
const VM_MAP_PHYS: i32 = 0xC0F;
const VM_UNMAP_PHYS: i32 = 0xC10;
const VM_MAPCACHEPAGE: i32 = 0xC1A;
const VM_SETCACHEPAGE: i32 = 0xC1B;
const VM_FORGETCACHEPAGE: i32 = 0xC1C;
const VM_CLEARCACHE: i32 = 0xC1D;

pub(crate) fn ipc_mmap(
    addr: usize, len: usize, prot: u32, flags: u32,
    fd: i32, offset: i64,
) -> Result<NonNull<u8>, AllocError> {
    let mut msg = Message::zeroed();
    msg.m_type = VM_MMAP;
    msg.m_mmap.addr = addr;
    msg.m_mmap.len = len;
    msg.m_mmap.prot = prot;
    msg.m_mmap.flags = flags;
    msg.m_mmap.fd = fd;
    msg.m_mmap.offset = offset;

    let r = taskcall(VM_PROC_NR, &mut msg);
    if r != 0 {
        return Err(AllocError::IpcError(r));
    }

    NonNull::new(msg.m_mmap.retaddr as *mut u8)
        .ok_or(AllocError::MapFailed)
}

pub(crate) fn ipc_vm_map_phys(
    target: Endpoint, phys_addr: u64, len: usize,
) -> Result<usize, AllocError> {
    let mut msg = Message::zeroed();
    msg.m_type = VM_MAP_PHYS;
    msg.m_lsys_vm_map_phys.ep = target.as_i32();
    msg.m_lsys_vm_map_phys.phaddr = phys_addr;
    msg.m_lsys_vm_map_phys.len = len;

    let r = taskcall(VM_PROC_NR, &mut msg);
    if r != 0 {
        return Err(AllocError::IpcError(r));
    }

    Ok(msg.m_lsys_vm_map_phys.reply as usize)
}

pub(crate) fn ipc_vm_unmap_phys(
    target: Endpoint, vaddr: NonNull<u8>,
) -> Result<(), AllocError> {
    let mut msg = Message::zeroed();
    msg.m_type = VM_UNMAP_PHYS;
    msg.m_lsys_vm_unmap_phys.ep = target.as_i32();
    msg.m_lsys_vm_unmap_phys.vaddr = vaddr.as_ptr() as usize;

    let r = taskcall(VM_PROC_NR, &mut msg);
    if r != 0 {
        return Err(AllocError::IpcError(r));
    }

    Ok(())
}
```

---

## 4. VM 服务端的客户端请求处理

### 4.1 现有 Rust 代码状态

| 组件 | 现有状态 | 需要的操作 |
|------|---------|-----------|
| `VmPageAllocator` | ✅ 已实现 | 客户端请求的物理页分配后端 |
| `PhysAllocator` trait | ✅ 已实现 | 物理内存分配接口 |
| `ReservedRegion` | ❌ 已删除 | 已删除，Direct Map 下不需要 |
| `CriticalPool` | ✅ 已实现 | 紧急内存池 |
| `do_mmap()` | ❌ 不存在 | 需要实现 |
| `do_map_phys()` | ❌ 不存在 | 22-vm-munmap 中有设计 |
| `do_munmap()` | ❌ 不存在 | 22-vm-munmap 中有设计 |
| `do_mapcache()` | ❌ 不存在 | 需要实现 |
| IPC 分发 | ❌ 不存在 | 需要实现主循环的消息分发 |

### 4.2 VM 主循环的消息分发

```rust
/// VM 主循环：接收消息，分发到对应处理函数
pub fn vm_main_loop(table: &mut VmProcTable, page_alloc: &mut VmPageAllocator) {
    loop {
        let msg = ipc_receive(ANY);

        let result = match msg.m_type {
            VM_BRK => do_brk(&msg, table, page_alloc),
            VM_MMAP => do_mmap(&msg, table, page_alloc),
            VM_MUNMAP | VM_UNMAP_PHYS | VM_SHM_UNMAP => do_munmap(&msg, table, page_alloc),
            VM_MAP_PHYS => do_map_phys(&msg, table),
            VM_EXIT => do_exit(&msg, table, page_alloc),
            VM_FORK => do_fork(&msg, table, page_alloc),
            VM_VFS_REPLY => do_vfs_reply(&msg, table),
            VM_MAPCACHEPAGE => do_mapcache(&msg, table),
            VM_SETCACHEPAGE => do_setcache(&msg, table),
            VM_FORGETCACHEPAGE => do_forgetcache(&msg, table),
            VM_CLEARCACHE => do_clearcache(&msg, table),
            VM_INFO => do_info(&msg, table),
            VM_GETPHYS => do_get_phys(&msg, table),
            VM_GETREF => do_get_refcount(&msg, table),
            VM_PAGEFAULT => do_pagefaults(&msg, table, page_alloc),
            _ => EINVAL,
        };

        if result != SUSPEND {
            ipc_reply(msg.m_source, &msg);
        }
    }
}
```

### 4.3 客户端请求与服务端处理的对应

| 客户端调用 | IPC 请求 | 服务端处理 | 返回值 |
|-----------|---------|-----------|--------|
| `alloc_contig()` | `VM_MMAP` | `do_mmap()` → `mmap_region()` | 虚拟地址 |
| `free_contig()` | `VM_MUNMAP` | `do_munmap()` → `unmap_range()` | OK/错误 |
| `vm_map_phys()` | `VM_MAP_PHYS` | `do_map_phys()` → `map_page_region()` | 虚拟地址 |
| `vm_unmap_phys()` | `VM_UNMAP_PHYS` | `do_munmap()` → `unmap_range()` | OK/错误 |
| `vm_map_cacheblock()` | `VM_MAPCACHEPAGE` | `do_mapcache()` | 缓存块地址 |
| `vm_set_cacheblock()` | `VM_SETCACHEPAGE` | `do_setcache()` | OK/错误 |
| `vm_forget_cacheblock()` | `VM_FORGETCACHEPAGE` | `do_forgetcache()` | OK/错误 |
| `vm_clear_cache()` | `VM_CLEARCACHE` | `do_clearcache()` | OK/错误 |

---

## 5. 客户端库的 RAII 封装

### 5.1 问题：C 风格的手动资源管理

Minix3 的客户端库是 C 风格的：分配和释放是独立调用，调用者必须确保释放。

```c
/* C: 手动管理 */
void *buf = alloc_contig(4096, 0, &phys);
/* ... 使用 buf ... */
free_contig(buf, 4096);  /* 必须手动释放！忘记就泄漏 */
```

### 5.2 Rust 方案：RAII 封装

```rust
/// 连续物理内存区域
///
/// RAII 封装：drop 时自动释放
pub struct ContigMem {
    vaddr: NonNull<u8>,
    phys: u64,
    len: usize,
}

impl ContigMem {
    pub fn alloc(len: usize, flags: ContigFlags) -> Result<Self, AllocError> {
        let (vaddr, phys) = contig::alloc_contig(len, flags)?;
        Ok(Self { vaddr, phys, len })
    }

    pub fn vaddr(&self) -> *mut u8 { self.vaddr.as_ptr() }
    pub fn phys(&self) -> u64 { self.phys }
    pub fn len(&self) -> usize { self.len }
}

impl Drop for ContigMem {
    fn drop(&mut self) {
        let _ = contig::free_contig(self.vaddr, self.len);
    }
}

/// 物理内存映射区域
pub struct PhysMapping {
    vaddr: NonNull<u8>,
    len: usize,
}

impl PhysMapping {
    pub fn map(phys_addr: u64, len: usize) -> Result<Self, AllocError> {
        let vaddr = map_phys::map_phys(phys_addr, len)?;
        Ok(Self { vaddr, len })
    }

    pub fn vaddr(&self) -> *mut u8 { self.vaddr.as_ptr() }
}

impl Drop for PhysMapping {
    fn drop(&mut self) {
        let _ = map_phys::unmap_phys(self.vaddr);
    }
}
```

**RAII 的优势**：

```rust
fn driver_init() -> Result<(), AllocError> {
    let dma_buf = ContigMem::alloc(4096, ContigFlags::empty())?;
    let regs = PhysMapping::map(0xFE000000, 0x1000)?;

    // 使用 dma_buf 和 regs...
    write_reg(&regs, 0x1234);

    Ok(())
    // dma_buf 和 regs 自动 drop，无需手动释放
}
```

### 5.3 安全的 DMA 缓冲区

```rust
/// DMA 缓冲区：连续物理内存 + 虚拟地址 + 物理地址
pub struct DmaBuffer {
    inner: ContigMem,
}

impl DmaBuffer {
    pub fn alloc(len: usize) -> Result<Self, AllocError> {
        let inner = ContigMem::alloc(len, ContigFlags::empty())?;
        Ok(Self { inner })
    }

    /// 驱动用虚拟地址读写缓冲区
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.inner.vaddr().cast(), self.inner.len()) }
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.inner.vaddr().cast(), self.inner.len()) }
    }

    /// 设备用物理地址进行 DMA 传输
    pub fn phys_addr(&self) -> u64 {
        self.inner.phys()
    }
}
```

---

## 6. 实现清单

### 6.1 需要新增的 crate

| crate | 内容 | 优先级 |
|-------|------|--------|
| `minix_alloc` | 客户端内存分配库 | 🔴 P0 |

### 6.2 minix_alloc crate 的模块

| 模块 | 内容 | 优先级 |
|------|------|--------|
| `contig.rs` | `alloc_contig`, `free_contig`, `ContigMem`, `DmaBuffer` | 🔴 P0 |
| `map_phys.rs` | `vm_map_phys`, `vm_unmap_phys`, `PhysMapping` | 🔴 P0 |
| `cache.rs` | `map_cache_block`, `set_cache_block`, `forget_cache_block`, `clear_cache` | 🟡 P1 |
| `mmap.rs` | `mmap`, `munmap` | 🟡 P1 |
| `ipc.rs` | IPC 底层封装 | 🔴 P0 |
| `error.rs` | `AllocError` 错误类型 | 🔴 P0 |

### 6.3 VM 服务端需要实现的请求处理

| 函数 | 对应请求 | 优先级 |
|------|---------|--------|
| `do_mmap()` | `VM_MMAP` | 🔴 P0 |
| `do_mapcache()` | `VM_MAPCACHEPAGE` | 🟡 P1 |
| `do_setcache()` | `VM_SETCACHEPAGE` | 🟡 P1 |
| `do_forgetcache()` | `VM_FORGETCACHEPAGE` | 🟡 P1 |
| `do_clearcache()` | `VM_CLEARCACHE` | 🟡 P1 |
| `do_info()` | `VM_INFO` | 🟢 P2 |
| `do_get_phys()` | `VM_GETPHYS` | 🟢 P2 |
| `do_get_refcount()` | `VM_GETREF` | 🟢 P2 |

### 6.4 测试计划

| 测试 | 描述 |
|------|------|
| `test_alloc_contig_basic` | 基本连续内存分配 |
| `test_alloc_contig_dma_flags` | DMA 限制标志 |
| `test_free_contig` | 释放连续内存 |
| `test_contig_mem_raii` | RAII 自动释放 |
| `test_map_phys_basic` | 基本物理映射 |
| `test_unmap_phys` | 取消物理映射 |
| `test_phys_mapping_raii` | RAII 自动取消映射 |
| `test_dma_buffer` | DMA 缓冲区虚拟/物理地址 |
| `test_cache_block` | 缓存块映射和设置 |
| `test_ipc_error_handling` | IPC 错误处理 |

---

## 7. 客户端库与 VM 内部的对比

### 7.1 两类内存分配的对比

| 方面 | VM 内部分配 | 客户端请求分配 |
|------|-----------|--------------|
| 调用者 | VM 自身 | 其他服务器/驱动 |
| 接口 | `alloc_mem()` / `free_mem()` | IPC: `VM_MMAP` / `VM_MAP_PHYS` |
| 物理页来源 | `PhysAllocator` | 同一个 `PhysAllocator` |
| 虚拟地址 | VM 自身地址空间 | 调用者进程的地址空间 |
| 同步/异步 | 同步（函数调用） | 同步 IPC（阻塞等待） |
| 错误处理 | 返回值 | IPC 返回码 |

### 7.2 物理页的统一管理

所有物理页——无论是 VM 内部分配还是客户端请求分配——都来自同一个 `PhysAllocator`：

```
PhysAllocator (bitmap/buddy/segment_tree)
  │
  ├── VM 内部使用:
  │    ├── 页表页 (pt_alloc)
  │    ├── PhysBlock 结构 (SLABALLOC)
  │    └── VirRegion 结构 (SLABALLOC)
  │
  └── 客户端请求:
       ├── alloc_contig → MAP_PREALLOC|MAP_CONTIG → 一次性分配连续页
       ├── vm_map_phys → VR_DIRECT → 不分配物理页（映射已有物理地址）
       ├── brk 扩展 → 延迟分配 → 缺页时分配
       └── 文件映射 → 缺页时分配或从缓存获取
```

**关键约束**：物理内存是有限的全局资源。VM 必须在所有请求之间公平分配，避免某个客户端耗尽所有物理页。

### 7.3 客户端库的设计哲学

Minix3 的客户端库遵循**最小特权原则**：

1. **权限检查**：`VM_MAP_PHYS` 需要 `map_perm_check()`，只有授权的驱动可以映射物理地址
2. **地址空间隔离**：每个进程只能操作自己的地址空间（除非有特权）
3. **资源限制**：VM 跟踪每个进程的 `vm_total`，防止过度分配
4. **同步阻塞**：客户端调用阻塞直到完成，避免异步带来的复杂性

Rust 的 `minix_alloc` crate 在此基础上增加**编译时安全**：
- RAII 封装确保资源释放
- 类型系统区分虚拟地址和物理地址
- `NonNull` 确保非空指针
- `Result` 确保错误处理
