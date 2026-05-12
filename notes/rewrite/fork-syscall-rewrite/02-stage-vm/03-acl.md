# 03-acl: VM 访问控制列表 (ACL)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/acl.c`  
> **说明**: 控制进程可以调用哪些 VM 系统调用

---

## 1. 概述

### 1.1 ACL 的作用

**ACL (Access Control List)** 是 VM (Virtual Memory) 服务器的内部权限控制机制，用于限制进程可以调用哪些 VM 系统调用。

**为什么 VM 需要权限控制？**

1. **安全隔离**：防止普通进程滥用 VM 功能（如直接操作其他进程的内存）
2. **最小权限原则**：每个进程只能访问其需要的 VM 功能
3. **系统进程特殊处理**：RS (Reincarnation Server) 等系统进程需要更多权限来管理服务

### 1.2 与 Minix3 的对应关系

**C 源码位置**：[acl.c](minix3/minix/servers/vm/acl.c)

**核心数据结构**：
```c
// acl.c
static bitchunk_t acl_mask[NR_SYS_PROCS][VM_CALL_MASK_SIZE];  // 权限位图
static bitchunk_t acl_inuse[BITMAP_CHUNKS(NR_SYS_PROCS)];      // 使用状态

#define NO_ACL       -1   // 无 ACL，临时状态，暂时允许所有调用
#define USER_ACL      0   // 普通用户进程共享的 ACL
#define FIRST_SYS_ACL 1   // 系统进程 ACL 起始索引
```

**核心函数**：
| C 函数 | 源码 | 作用 |
|-------|------|------|
| `acl_init()` | [acl.c:21](minix3/minix/servers/vm/acl.c#L21) | 初始化 ACL 数据结构 |
| `acl_check()` | [acl.c:37](minix3/minix/servers/vm/acl.c#L37) | 检查进程是否有权限执行某个 VM 调用 |
| `acl_set()` | [acl.c:70](minix3/minix/servers/vm/acl.c#L70) | 为进程设置 ACL |
| `acl_fork()` | [acl.c:110](minix3/minix/servers/vm/acl.c#L110) | fork 时处理 ACL 继承 |
| `acl_clear()` | [acl.c:120](minix3/minix/servers/vm/acl.c#L120) | 进程退出时清理 ACL |

**与 vmproc 的关系**：
```c
// vmproc.h
struct vmproc {
    ...
    int vm_acl;  // ACL 索引：NO_ACL(-1), USER_ACL(0), 或系统 ACL(1+)
    ...
};
```

### 1.3 三种 ACL 状态

Minix3 的 `vm_acl` 字段有三种取值，对应三种语义不同的状态：

| `vm_acl` 值 | 名称 | 本质 | 语义 |
|-------------|------|------|------|
| `-1` (NO_ACL) | 未初始化 | 生命周期状态 | 进程尚未被 RS 接管，暂时允许所有调用 |
| `0` (USER_ACL) | 默认权限 | 权限策略 | 所有普通用户进程共享相同的权限配置 |
| `1~63` | 系统权限 | 权限策略 | 系统服务拥有独立的权限位图 |

**核心洞察**：`NO_ACL` 不是权限策略，而是"尚未进入权限系统"的生命周期状态。

### 1.4 权限继承规则

**fork 时的行为**：
- **普通进程** (`USER_ACL`)：子进程继承父进程的 `USER_ACL`
- **系统进程** (有独立 ACL，`1~63`)：子进程获得 `NO_ACL`，需要 RS 重新设置权限

**设计理由**：
- 普通进程保持简单，自动继承权限
- 系统进程 fork 后进入临时状态，需要显式授权，防止权限扩散

---

## 2. C 源码分析

### 2.1 VM 调用号定义

ACL 检查的是 VM 调用权限，调用号是 ACL 位图的索引。

**Minix3 定义位置**: [com.h:627](minix3/minix/include/minix/com.h#L627)

```c
#define VM_RQ_BASE      0xC00    // VM 调用号基址

// --- PM 调用 ---
#define VM_EXIT         (VM_RQ_BASE+0)   // 进程退出
#define VM_FORK         (VM_RQ_BASE+1)   // 进程 fork
#define VM_BRK          (VM_RQ_BASE+2)   // 堆调整
#define VM_EXEC_NEWMEM  (VM_RQ_BASE+3)   // exec 新内存
#define VM_WILLEXIT     (VM_RQ_BASE+5)   // 即将退出

// --- 通用调用 ---
#define VM_MMAP         (VM_RQ_BASE+10)  // mmap
#define VM_MUNMAP       (VM_RQ_BASE+17)  // munmap
#define VM_MAP_PHYS     (VM_RQ_BASE+15)  // 映射物理内存
#define VM_UNMAP_PHYS   (VM_RQ_BASE+16)  // 解除物理映射

// --- RS 调用 ---
#define VM_RS_SET_PRIV  (VM_RQ_BASE+37)  // RS 设置权限
#define VM_RS_PREPARE   (VM_RQ_BASE+48)  // RS 准备服务

// --- 特殊 ---
#define VM_PAGEFAULT    (VM_RQ_BASE+0xFF) // 缺页异常（内核发送，不经过 ACL）
```

**调用来源分类**

| 来源 | 典型调用 | 说明 |
|------|---------|------|
| PM | `VM_FORK`, `VM_EXIT`, `VM_BRK` | 进程管理相关 |
| 用户进程 | `VM_MMAP`, `VM_MUNMAP` | 内存映射系统调用 |
| 系统服务 | `VM_MAP_PHYS`, `VM_RS_PREPARE` | RS、驱动等特权操作 |
| 内核 | `VM_PAGEFAULT` | 缺页异常，不经过 IPC，不经过 ACL |

**ACL 位偏移**: `acl_check(vmp, call)` 中的 `call` 参数是调用号相对于 `VM_RQ_BASE` 的偏移量（0 开始），用于索引位图。例如 `VM_MMAP` 的偏移量为 10，对应位图的第 10 位。

**VM_PAGEFAULT 不在 ACL 范围内**: `VM_PAGEFAULT` 的偏移量为 0xFF = 255，远超 ACL 位图的 64 位范围（`NR_VM_CALLS = 49`）。这是合理的，因为缺页异常由内核直接发送，不经过 IPC 请求通道，不需要 ACL 检查。

> Rust 调用号常量定义在 `minix-types/src/ipc/vm.rs`，与 C 定义一一对应。详见 [§3.3](#33-aclmask-位标志)。

### 2.2 ACL 表结构

**核心数据结构**

```c
// acl.c
#define NO_ACL		 -1       // 无 ACL，所有调用暂时允许
#define USER_ACL	  0       // 普通用户进程共享的 ACL
#define FIRST_SYS_ACL  1       // 系统进程 ACL 起始索引

static bitchunk_t acl_mask[NR_SYS_PROCS][VM_CALL_MASK_SIZE];  // 权限位图表
static bitchunk_t acl_inuse[BITMAP_CHUNKS(NR_SYS_PROCS)];      // ACL 使用状态位图
```

**结构说明**

| 数据结构 | 类型 | 说明 |
|---------|------|------|
| `acl_mask[][]` | `bitchunk_t[64][2]` | 二维数组，每个 ACL 索引对应一个位图，表示允许的 VM 调用 |
| `acl_inuse[]` | `bitchunk_t[2]` | 位图，标记哪些系统 ACL 槽位已被占用 |
| `vm_acl` | `int` | 每个进程一个，指向 `acl_mask` 的索引 |

**ACL 索引类型**

```
vm_acl 值      含义                              使用者
─────────────────────────────────────────────────────────────────
-1 (NO_ACL)    临时状态，尚未设置 ACL，暂时允许    新创建的系统进程、
               （会打印警告）                      系统进程 fork 后的子进程
 0 (USER_ACL)   普通用户进程共享 ACL                所有普通用户进程
 1~63           系统进程独立 ACL                    RS、DS、VM 等系统服务
```

**权限位图布局**

```
acl_mask[acl_index] = [chunk0, chunk1]  // 64 位权限掩码
                      │      │
                      │      └─ 位 32-63: VM_CALL_32 ~ VM_CALL_63
                      └─ 位 0-31: VM_CALL_0 ~ VM_CALL_31

位值: 1 = 允许调用, 0 = 拒绝调用
```

**与 vmproc 的关系**

```c
struct vmproc {
    ...
    int vm_acl;        // 指向 acl_mask[] 的索引
    ...
};

// 访问权限时的查找流程:
// 1. 通过 endpoint 找到 vmproc
// 2. 读取 vmproc->vm_acl 获取 ACL 索引
// 3. 用 call 号查询 acl_mask[vm_acl][call/32] 的对应位
```

### 2.3 权限检查

#### 2.3.1 acl_check - 检查调用权限

**函数签名**

```c
// [acl.c:37](minix3/minix/servers/vm/acl.c#L37)
int acl_check(struct vmproc *vmp, int call);
```

**参数说明**

| 参数 | 类型 | 说明 |
|-----|------|------|
| `vmp` | `struct vmproc *` | 发起调用的进程 |
| `call` | `int` | VM 调用号（从 0 开始） |

**返回值**

| 返回值 | 含义 |
|-------|------|
| `OK` (0) | 允许调用 |
| `EPERM` | 无权限，拒绝调用 |

**检查流程**

```c
int acl_check(struct vmproc *vmp, int call) {
    // 1. VM 进程自身调用总是允许
    if (vmp->vm_endpoint == VM_PROC_NR)
        return OK;

    // 2. NO_ACL: 暂时允许所有调用（兼容行为）
    if (vmp->vm_acl == NO_ACL) {
        // RS 启动时可能需要调用 VM_BRK
        if (vmp->vm_endpoint == RS_PROC_NR)
            return OK;
        
        printf("VM: calling process %u has no ACL!\n", vmp->vm_endpoint);
        return OK;  // 暂时允许，但打印警告
    }

    // 3. 检查权限位图
    if (!GET_BIT(acl_mask[vmp->vm_acl], call))
        return EPERM;  // 无权限

    return OK;  // 有权限
}
```

**检查规则详解**

| 条件 | 处理 | 说明 |
|-----|------|------|
| VM 进程自身 | 直接允许 | VM 内部调用不需要检查 |
| `NO_ACL` | 允许但警告 | 兼容旧代码，RS 特殊处理 |
| `USER_ACL` | 检查位图 | 普通进程共享的权限 |
| 系统 ACL | 检查位图 | 系统进程独立权限 |

#### 2.3.2 调用点分析

**ACL 检查的唯一位置**

在 Minix3 中，`acl_check` 只在 VM 主消息循环中被调用：

```c
// main.c: VM 主循环
while (TRUE) {
    // 1. 接收消息
    sef_receive_status(ANY, &msg, &rcv_sts);
    who_e = msg.m_source;
    
    // 2. 验证调用者 endpoint
    vm_isokendpt(who_e, &caller_slot);
    
    // 3. 解析调用号
    type = msg.m_type;
    c = CALLNUMBER(type);  // 将消息类型转换为调用号
    
    // 4. 检查调用是否有效
    if (c < 0 || !vm_calls[c].vmc_func) {
        result = ENOSYS;
    } else {
        // 5. 检查 ACL 权限（唯一检查点）
        if (acl_check(&vmproc[caller_slot], c) != OK) {
            printf("VM: unauthorized %s by %d\n",
                   vm_calls[c].vmc_name, who_e);
            result = EPERM;
        } else {
            // 6. 执行实际调用
            result = vm_calls[c].vmc_func(&msg);
        }
    }
}
```

**特殊调用处理（不经过 ACL 检查）**

| 调用类型 | 处理方式 | 说明 |
|---------|---------|------|
| `VM_PAGEFAULT` | `do_pagefaults()` | 缺页异常，内核直接发送，偏移量 0xFF 超出位图范围 |
| 无效调用号 | 返回 `ENOSYS` | 调用号越界或未注册 |

### 2.4 fork 时的 ACL 继承

#### 2.4.1 acl_fork - 复制权限

**函数签名**

```c
// [acl.c:110-114](minix3/minix/servers/vm/acl.c#L110-L114)
void acl_fork(struct vmproc *vmp);
```

**参数说明**

| 参数 | 类型 | 说明 |
|-----|------|------|
| `vmp` | `struct vmproc *` | 子进程的 vmproc（fork 后已初始化） |

**继承规则**

```c
void acl_fork(struct vmproc *vmp) {
    if (vmp->vm_acl != USER_ACL)
        vmp->vm_acl = NO_ACL;
}
```

| 父进程 ACL | 子进程 ACL | 说明 |
|-----------|-----------|------|
| `USER_ACL` (0) | `USER_ACL` (0) | 普通进程继承共享 ACL |
| `NO_ACL` (-1) | `NO_ACL` (-1) | 保持无 ACL 状态 |
| 系统 ACL (1+) | `NO_ACL` (-1) | 系统进程子进程需重新授权 |

**设计原理**

1. **普通进程简化**：用户进程 fork 时自动继承 `USER_ACL`，无需 RS 介入
2. **系统进程隔离**：系统进程（如 RS、DS）的子进程不继承特权，防止权限扩散
3. **RS 重新授权**：系统进程的子进程需要 RS 显式调用 `acl_set()` 设置权限

**调用时机**

```c
// fork.c: do_fork 函数中
int do_fork(message *msg) {
    // ... 创建子进程 ...
    
    // 复制父进程的 vmproc 数据
    memcpy(vmc, vmp, sizeof(struct vmproc));
    
    // 调整子进程特定的字段
    vmc->vm_endpoint = child_ep;
    vmc->vm_flags &= VMF_INUSE;
    
    // 处理 ACL 继承
    acl_fork(vmc);  // 根据父进程 ACL 决定子进程权限
    
    // ... 其他初始化 ...
}
```

#### 2.4.2 权限传播

**进程退出时的权限清理**

```c
// [acl.c:120-128](minix3/minix/servers/vm/acl.c#L120-L128)
void acl_clear(struct vmproc *vmp) {
    if (vmp->vm_acl != NO_ACL) {
        // 系统 ACL 需要释放槽位
        if (vmp->vm_acl != USER_ACL)
            UNSET_BIT(acl_inuse, vmp->vm_acl);

        // 重置为 NO_ACL
        vmp->vm_acl = NO_ACL;
    }
}
```

**完整生命周期示例**

```
普通进程生命周期:
  init → USER_ACL ──fork──► USER_ACL ──exit──► 槽位释放
                          子进程

系统进程生命周期:
  RS创建 → 系统ACL#1 ──fork──► NO_ACL ──RS授权──► 系统ACL#2 ──exit──► 槽位释放
                              子进程
```

---

## 3. Rust 设计

### 3.1 设计原则

1. **语义显式化**：用类型表达状态，不用 magic number
2. **共享是语义**：共享通过 enum variant 表达，而非全局数组索引
3. **最小改动**：保留 Minix3 行为，不改变启动流程和 RS 交互
4. **类型安全**：非法状态不可表达

### 3.2 AclState 枚举

Minix3 的 `vm_acl` 字段用 `i32` 表示三种语义不同的状态，Rust 使用 enum 显式建模：

```rust
/// ACL 状态，表达进程的权限配置。
///
/// 对应 Minix3 的三种状态：
/// - `Uninitialized` → `NO_ACL (-1)`
/// - `Default` → `USER_ACL (0)`
/// - `System(mask)` → 系统进程 ACL 槽位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AclState {
    /// 未初始化状态。进程尚未被 RS 接管，暂时允许所有调用。
    ///
    /// 这是生命周期状态，不是权限策略。
    /// Minix3: `vm_acl == NO_ACL`
    Uninitialized,

    /// 默认权限。所有普通用户进程共享相同的权限配置。
    ///
    /// Minix3: `vm_acl == USER_ACL`
    Default,

    /// 系统进程权限。系统服务拥有独立的权限位图。
    ///
    /// Minix3: `vm_acl >= FIRST_SYS_ACL`
    System(AclMask),
}
```

**与 Minix3 的映射**

| Minix3 | Rust | 本质 |
|--------|------|------|
| `vm_acl == -1` (NO_ACL) | `AclState::Uninitialized` | 生命周期状态 |
| `vm_acl == 0` (USER_ACL) | `AclState::Default` | 权限策略（共享） |
| `vm_acl >= 1` | `AclState::System(mask)` | 权限策略（独占） |

**为什么 Default 不带数据？**

1. **不可变**：Default ACL 是系统常量，不应被修改
2. **类型安全**：`AclState::Default` 不可能携带非法值
3. **语义清晰**：Default 和 `System(DEFAULT_MASK)` 是不同状态——前者是"所有普通进程共享的默认配置"，后者是"恰好拥有默认权限的系统进程"

**为什么保留 Uninitialized？**

1. **Minix3 兼容**：启动流程依赖此状态
2. **RS 交互**：RS 在服务启动时才分配 ACL
3. **渐进式重构**：先显式建模，再考虑消灭

### 3.3 AclMask 位标志

Minix3 使用 `bitchunk_t acl_mask[NR_SYS_PROCS][VM_CALL_MASK_SIZE]` 全局数组存储权限位图。Rust 使用 `bitflags` 将权限内联到 `AclState::System(mask)` 中，消除了全局数组。

```rust
use minix_types::{
    VM_RQ_BASE, VM_EXIT, VM_FORK, VM_BRK, VM_EXEC_NEWMEM, VM_WILLEXIT,
    VM_MMAP, VM_ADDDMA, VM_DELDMA, VM_GETDMA, VM_MAP_PHYS, VM_UNMAP_PHYS,
    VM_MUNMAP, VM_MAPCACHEPAGE, VM_SETCACHEPAGE, VM_FORGETCACHEPAGE,
    VM_CLEARCACHE, VM_VFS_REPLY, VM_REMAP, VM_SHM_UNMAP, VM_GETPHYS,
    VM_GETREF, VM_RS_SET_PRIV, VM_INFO, VM_RS_UPDATE, VM_RS_MEMCTL,
    VM_REMAP_RO, VM_PROCCTL, VM_VFS_MMAP, VM_GETRUSAGE, VM_RS_PREPARE,
};

bitflags::bitflags! {
    /// VM 调用权限位图。
    ///
    /// 每一位对应一个 VM 调用号，1 表示允许，0 表示禁止。
    /// 位偏移量 = 调用号 - VM_RQ_BASE
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct AclMask: u64 {
        const VM_EXIT = 1 << (VM_EXIT - VM_RQ_BASE);
        const VM_FORK = 1 << (VM_FORK - VM_RQ_BASE);
        const VM_BRK = 1 << (VM_BRK - VM_RQ_BASE);
        const VM_EXEC_NEWMEM = 1 << (VM_EXEC_NEWMEM - VM_RQ_BASE);
        const VM_WILLEXIT = 1 << (VM_WILLEXIT - VM_RQ_BASE);
        const VM_MMAP = 1 << (VM_MMAP - VM_RQ_BASE);
        const VM_ADDDMA = 1 << (VM_ADDDMA - VM_RQ_BASE);
        const VM_DELDMA = 1 << (VM_DELDMA - VM_RQ_BASE);
        const VM_GETDMA = 1 << (VM_GETDMA - VM_RQ_BASE);
        const VM_MAP_PHYS = 1 << (VM_MAP_PHYS - VM_RQ_BASE);
        const VM_UNMAP_PHYS = 1 << (VM_UNMAP_PHYS - VM_RQ_BASE);
        const VM_MUNMAP = 1 << (VM_MUNMAP - VM_RQ_BASE);
        const VM_MAPCACHEPAGE = 1 << (VM_MAPCACHEPAGE - VM_RQ_BASE);
        const VM_SETCACHEPAGE = 1 << (VM_SETCACHEPAGE - VM_RQ_BASE);
        const VM_FORGETCACHEPAGE = 1 << (VM_FORGETCACHEPAGE - VM_RQ_BASE);
        const VM_CLEARCACHE = 1 << (VM_CLEARCACHE - VM_RQ_BASE);
        const VM_VFS_REPLY = 1 << (VM_VFS_REPLY - VM_RQ_BASE);
        const VM_REMAP = 1 << (VM_REMAP - VM_RQ_BASE);
        const VM_SHM_UNMAP = 1 << (VM_SHM_UNMAP - VM_RQ_BASE);
        const VM_GETPHYS = 1 << (VM_GETPHYS - VM_RQ_BASE);
        const VM_GETREF = 1 << (VM_GETREF - VM_RQ_BASE);
        const VM_RS_SET_PRIV = 1 << (VM_RS_SET_PRIV - VM_RQ_BASE);
        const VM_INFO = 1 << (VM_INFO - VM_RQ_BASE);
        const VM_RS_UPDATE = 1 << (VM_RS_UPDATE - VM_RQ_BASE);
        const VM_RS_MEMCTL = 1 << (VM_RS_MEMCTL - VM_RQ_BASE);
        const VM_REMAP_RO = 1 << (VM_REMAP_RO - VM_RQ_BASE);
        const VM_PROCCTL = 1 << (VM_PROCCTL - VM_RQ_BASE);
        const VM_VFS_MMAP = 1 << (VM_VFS_MMAP - VM_RQ_BASE);
        const VM_GETRUSAGE = 1 << (VM_GETRUSAGE - VM_RQ_BASE);
        const VM_RS_PREPARE = 1 << (VM_RS_PREPARE - VM_RQ_BASE);
    }
}
```

**设计要点**：

- `VM_EXIT` 等调用号**只在 `minix-types` 中定义一次**
- `AclMask` 通过 `1 << (VM_EXIT - VM_RQ_BASE)` 计算位偏移
- 消除了重复定义的不一致风险
- `u64` 足以覆盖所有 ACL 范围内的调用号（`NR_VM_CALLS = 49`）

**VM_PAGEFAULT 不在 AclMask 中**：其偏移量 0xFF = 255 超出 u64 的 64 位宽度（`1 << 255` 无法用 u64 表示），且缺页异常不经过 ACL 检查。

**预定义权限集合**

```rust
impl AclMask {
    /// 默认用户权限：允许普通用户进程的所有调用。
    ///
    /// 不包含特权操作（如 VM_MAP_PHYS、VM_RS_PREPARE）。
    pub(crate) const DEFAULT: Self = Self::from_bits_truncate(
        Self::VM_EXIT.bits() |
        Self::VM_FORK.bits() |
        Self::VM_BRK.bits() |
        Self::VM_EXEC_NEWMEM.bits() |
        Self::VM_WILLEXIT.bits() |
        Self::VM_MMAP.bits() |
        Self::VM_MUNMAP.bits()
    );
}
```

> `Uninitialized` 是特殊生命周期状态，`acl_check()` 直接放行不走位图，因此不需要 `ALL_ALLOWED` 常量。`mask()` 方法返回 `Option<AclMask>`，`Uninitialized` 返回 `None`：

```rust
    pub(crate) fn mask(&self) -> Option<AclMask> {
        match self {
            AclState::Uninitialized => None,
            AclState::Default => Some(AclMask::DEFAULT),
            AclState::System(m) => Some(*m),
        }
    }
```

### 3.4 Minix3 函数映射总表

Minix3 的 `acl.c` 定义了 5 个函数，Rust 实现的对应关系如下：

| Minix3 函数 | Rust 方法 | 说明 |
|------------|----------|------|
| `acl_init()` | `VmProc::vacant()` | 不需要独立函数。`VmProc::vacant()` 将 `vm_acl` 初始化为 `AclState::Uninitialized`，等价于 Minix3 的 `vmproc[i].vm_acl = NO_ACL`。全局 `acl_mask` 和 `acl_inuse` 数组在 Rust 中不存在（权限内联于 `AclState::System(AclMask)`），因此无需初始化。 |
| `acl_check(vmp, call)` | `AclState::acl_check(&self, proc, call)` | 语义完全一致。见 3.5 节。 |
| `acl_set(vmp, mask, sys_proc)` | `AclState::acl_set(sys_proc, mask)` | 语义一致，但无需槽位管理。见 3.6 节。 |
| `acl_fork(vmp)` | `AclState::acl_fork(&self)` | 语义完全一致。见 3.7 节。 |
| `acl_clear(vmp)` | `AclState::acl_clear(&self)` | 语义简化：无需释放槽位。见 3.8 节。 |

### 3.5 权限检查 — `acl_check`

```rust
impl AclState {
    /// Check whether a process is allowed to make a certain (zero-based) call.
    ///
    /// Corresponds to Minix3's `acl_check()`.
    /// Returns `Ok(())` if allowed, `Err(VmError::PermissionDenied)` if not.
    pub(crate) fn acl_check(&self, proc: &ActiveProc<'_>, call: u32) -> Result<(), VmError> {
        if proc.endpoint() == Endpoint::VM {
            return Ok(());
        }

        match self {
            AclState::Uninitialized => {
                if proc.endpoint() != Endpoint::RS {
                    // TODO: Minix3 prints "VM: calling process %u has no ACL!"
                }
                Ok(())
            }
            AclState::Default => {
                let call_flag = AclMask::from_bits_truncate(1u64 << call);
                if AclMask::DEFAULT.contains(call_flag) {
                    Ok(())
                } else {
                    Err(VmError::PermissionDenied)
                }
            }
            AclState::System(mask) => {
                let call_flag = AclMask::from_bits_truncate(1u64 << call);
                if mask.contains(call_flag) {
                    Ok(())
                } else {
                    Err(VmError::PermissionDenied)
                }
            }
        }
    }
}
```

**与 Minix3 的检查流程对比**

| 步骤 | Minix3 C | Rust |
|-----|----------|------|
| VM 进程检查 | `vmp->vm_endpoint == VM_PROC_NR` | `proc.endpoint() == Endpoint::VM` |
| NO_ACL 处理 | 打印警告，允许调用 | `Uninitialized` → 允许（TODO: 日志） |
| USER_ACL 检查 | `GET_BIT(acl_mask[0], call)` | `DEFAULT.contains(from_bits_truncate(1 << call))` |
| 系统 ACL 检查 | `GET_BIT(acl_mask[vm_acl], call)` | `mask.contains(from_bits_truncate(1 << call))` |
| 返回值 | `OK` / `EPERM` | `Ok(())` / `Err(VmError::PermissionDenied)` |

### 3.6 权限设置 — `acl_set`

```c
// [acl.c:70-101](minix3/minix/servers/vm/acl.c#L70-L101)
void acl_set(struct vmproc *vmp, bitchunk_t *mask, int sys_proc);
```

```rust
impl AclState {
    /// Assign a call mask to a process.
    ///
    /// Corresponds to Minix3's `acl_set()`.
    /// - User processes (`sys_proc == false`) get `Default` (shared user ACL).
    /// - System processes (`sys_proc == true`) get `System(mask)`.
    ///
    /// Unlike Minix3, there is no shared slot table. Each `System(AclMask)`
    /// carries its own mask, so slot allocation is unnecessary.
    pub(crate) fn acl_set(sys_proc: bool, mask: Option<AclMask>) -> Self {
        if sys_proc {
            match mask {
                Some(m) => AclState::System(m),
                None => {
                    // Minix3: "WARNING: inheriting uninitialized ACL mask"
                    // In our design, no shared slots to inherit from.
                    AclState::System(AclMask::empty())
                }
            }
        } else {
            AclState::Default
        }
    }
}
```

**与 Minix3 的 `acl_set` 对比**

| 方面 | Minix3 C | Rust |
|-----|----------|------|
| 槽位分配 | 遍历 `acl_inuse` 找空闲槽位 | 不需要，`System(AclMask)` 内联权限 |
| 槽位耗尽 | `printf("VM: no ACL entries available!")` 并返回 | 不可能发生，无槽位上限 |
| 用户进程 | 固定使用 `USER_ACL` (slot 0) | 固定返回 `Default` |
| 系统 + 有 mask | 分配槽位 + `memcpy` mask | `System(mask)` |
| 系统 + 无 mask | 分配槽位 + 继承已有 mask | `System(AclMask::empty())` + 警告 |
| 先清旧 ACL | 调用 `acl_clear()` | 调用方负责先 `acl_clear()` |

**设计差异说明**

Minix3 的 `acl_set` 内部调用 `acl_clear` 先清除旧 ACL。Rust 版本将 `acl_set` 设计为返回新 `AclState` 的纯函数，由调用方负责先调用 `acl_clear()`。这样设计的原因：

1. `AclState` 是值类型（enum），赋值即替换，不存在"忘记清除"的风险
2. 调用方可以灵活组合：`proc.set_acl(old.acl_clear()); proc.set_acl(AclState::acl_set(...))`
3. 避免在构造函数中隐含副作用

### 3.7 Fork 行为 — `acl_fork`

```rust
impl AclState {
    /// A process has forked. User processes inherit their parent's ACL.
    /// System processes do not inherit an ACL.
    ///
    /// Corresponds to Minix3's `acl_fork()`.
    pub(crate) fn acl_fork(&self) -> Self {
        match self {
            AclState::Uninitialized => AclState::Uninitialized,
            AclState::Default => AclState::Default,
            AclState::System(_) => {
                // 系统进程的子进程不继承特权
                AclState::Uninitialized
            }
        }
    }
}
```

**继承规则对比**

| 父进程 ACL | 子进程 ACL | Minix3 原始 | Rust |
|-----------|-----------|------------|------|
| USER_ACL (0) | USER_ACL (0) | `if (vmp->vm_acl != USER_ACL) vmp->vm_acl = NO_ACL;` | `Default → Default` |
| NO_ACL (-1) | NO_ACL (-1) | 同上（`!= USER_ACL` 但已是 NO_ACL） | `Uninitialized → Uninitialized` |
| 系统 ACL (1+) | NO_ACL (-1) | 同上 | `System(_) → Uninitialized` |

**与 Minix3 的设计差异**

Minix3 的 `acl_fork(vmp)` 只接收子进程参数——因为 fork 时子进程通过 `memcpy` 已复制了父进程的 `vm_acl`，`acl_fork` 只需检查并修正。Rust 的 typestate 设计中不存在 `memcpy` 继承，子进程的 ACL 需要显式设置，因此 `acl_fork()` 返回子进程应获得的 ACL 状态。

### 3.8 ACL 清除 — `acl_clear`

```rust
impl AclState {
    /// A process has exited. Mark it as having no ACL.
    ///
    /// Corresponds to Minix3's `acl_clear()`.
    /// Unlike Minix3, there is no shared slot table to free,
    /// so simply returning `Uninitialized` is sufficient.
    pub(crate) fn acl_clear(&self) -> Self {
        AclState::Uninitialized
    }
}
```

**与 Minix3 的 `acl_clear` 对比**

| 方面 | Minix3 C | Rust |
|-----|----------|------|
| 清除 ACL 值 | `vmp->vm_acl = NO_ACL` | 返回 `Uninitialized` |
| 释放系统槽位 | `UNSET_BIT(acl_inuse, vmp->vm_acl)` | 不需要，无共享槽位表 |
| 用户槽位释放 | 不释放（`USER_ACL` 永久占用） | 不适用 |
| NO_ACL 时 | 跳过（`if (vmp->vm_acl != NO_ACL)`） | 无条件返回 `Uninitialized`（幂等） |

**设计差异说明**

Minix3 的 `acl_clear` 需要释放 `acl_inuse` 中的槽位引用计数，因为多个进程可能共享同一个系统 ACL 槽位。Rust 设计中 `System(AclMask)` 是值类型，权限内联，不存在共享槽位，因此无需释放。`acl_clear` 简化为无条件返回 `Uninitialized`。

Minix3 中 `acl_clear` 在 `acl_set` 内部被调用（先清后设），也在进程退出时被调用。Rust 中：
- 进程退出时：`VmProc::clear()` 将 `vm_acl` 设为 `AclState::Uninitialized`，等价于 `acl_clear`
- `acl_set` 前：调用方负责先 `acl_clear()`（见 3.6 节）

### 3.9 与旧设计的对比

| 方面 | 旧实现 (AclIndex + AclManager) | 新实现 (AclState + AclMask) |
|------|------|------|
| ACL 索引 | `AclIndex(i32)` + magic number | `AclState` enum |
| 权限位图 | `acl_mask[][]` 全局数组 | `AclMask` bitflags 内联 |
| 槽位管理 | `acl_inuse` 位图 + `AclManager` | 不需要（权限内联） |
| VmProc.acl 大小 | 4 bytes (i32) | ~16 bytes (enum + u64) |
| 全局 ACL 表 | 64 × 2 × 4 = 512 bytes | 0 |
| in_use 位图 | 8 bytes | 0 |
| 权限检查 | `manager.check(&proc, call)` | `proc.acl().acl_check(&proc, call)` |
| Fork | `manager.fork(&parent, &mut child)` | `parent.acl().acl_fork()` |

**API 变化**

```rust
// 旧
let idx = proc.acl().get();
if idx == NO_ACL { ... }
if idx == USER_ACL { ... }
manager.check(&proc, call)

// 新
match proc.acl() {
    AclState::Uninitialized => { ... }
    AclState::Default => { ... }
    AclState::System(mask) => { ... }
}
proc.acl().acl_check(&proc, call)
```

---

## 4. 安全分析

### 4.1 权限最小化

**为什么需要限制 VM 调用？**

VM（虚拟内存管理器）是 Minix3 的核心服务，负责管理所有进程的内存。如果任意进程都能调用任意 VM 功能，会带来严重的安全风险：

| VM 调用 | 风险 | 需要限制的进程 |
|--------|------|---------------|
| `VM_MAP_PHYS` | 直接映射物理内存，可绕过内存保护 | 仅限系统进程 |
| `VM_UNMAP_PHYS` | 解映射物理内存，可能导致系统崩溃 | 仅限系统进程 |
| `VM_RS_PREPARE` | RS 专用，用于服务重启准备 | 仅限 RS |
| `VM_MMAP` / `VM_MUNMAP` | 任意映射/释放内存 | 普通进程只能操作自己的内存 |

**最小权限原则**

```
设计原则：每个进程只能访问完成其任务所必需的最小资源集合。

普通进程 (AclState::Default)
  ├─ VM_EXIT: 允许
  ├─ VM_FORK: 允许
  ├─ VM_BRK: 允许
  ├─ VM_EXEC_NEWMEM: 允许
  ├─ VM_WILLEXIT: 允许
  ├─ VM_MMAP: 允许
  ├─ VM_MUNMAP: 允许
  └─ VM_MAP_PHYS: 禁止 ← 特权操作

系统进程 (AclState::System(mask))
  └─ 根据需要配置，可包含特权操作
```

### 4.2 沙箱机制

**ACL 如何支持进程隔离**

```
┌─────────────────────────────────────────────────────────────┐
│                        用户进程 A                            │
│              ┌─────────────────────────┐                     │
│              │ 只能调用允许的 VM 操作   │                     │
│              │ (Default: 基本内存管理)  │                     │
│              └─────────────────────────┘                     │
└───────────────────────────┬─────────────────────────────────┘
                            │ IPC
┌───────────────────────────▼─────────────────────────────────┐
│                      VM 服务器                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ ACL 检查: AclState::check()                           │  │
│  │ - Uninitialized: 允许所有（过渡态）                     │  │
│  │ - Default: 只允许基本操作                               │  │
│  │ - System(mask): 按位图检查                              │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

**沙箱逃逸防护**

```
攻击尝试 1: 通过 VM_MAP_PHYS 映射内核内存
  ACL 检查: AclState::Default 不包含 VM_MAP_PHYS → Err(())
  防护成功

攻击尝试 2: fork 后提升权限
  ACL 继承: AclState::System(_).acl_fork() → Uninitialized
  结果: 子进程不继承特权，需 RS 重新授权
  防护成功

攻击尝试 3: 构造非法 AclState
  类型系统: enum 不存在非法值，编译期保证
  防护成功
```

---

## 5. 测试

### 5.1 权限检查测试

```rust
#[test]
fn test_acl_check_uninitialized() {
    let state = AclState::Uninitialized;
    let proc = get_active_proc(UserSlot::new(20));

    // Uninitialized 允许所有调用
    assert!(state.acl_check(&proc, 0).is_ok());
    assert!(state.acl_check(&proc, 5).is_ok());
    assert!(state.acl_check(&proc, 100).is_ok());
}

#[test]
fn test_acl_check_vm_proc() {
    let state = AclState::Default;
    let mut proc = get_active_proc(UserSlot::new(21));
    proc.set_endpoint(Endpoint::VM);

    // VM 进程总是允许
    assert!(state.acl_check(&proc, 0).is_ok());
    assert!(state.acl_check(&proc, 100).is_ok());
}

#[test]
fn test_acl_check_default() {
    let state = AclState::Default;
    let proc = get_active_proc(UserSlot::new(22));

    // Default 允许基本调用
    assert!(state.acl_check(&proc, VM_EXIT - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_FORK - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_BRK - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_MMAP - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_MUNMAP - VM_RQ_BASE).is_ok());

    // Default 禁止特权调用
    assert!(state.acl_check(&proc, VM_MAP_PHYS - VM_RQ_BASE).is_err());
    assert!(state.acl_check(&proc, VM_RS_PREPARE - VM_RQ_BASE).is_err());
}

#[test]
fn test_acl_check_system() {
    let mask = AclMask::VM_MMAP | AclMask::VM_MAP_PHYS | AclMask::VM_RS_PREPARE;
    let state = AclState::System(mask);
    let proc = get_active_proc(UserSlot::new(23));

    // 授权的调用
    assert!(state.acl_check(&proc, VM_MMAP - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_MAP_PHYS - VM_RQ_BASE).is_ok());
    assert!(state.acl_check(&proc, VM_RS_PREPARE - VM_RQ_BASE).is_ok());

    // 未授权的调用
    assert!(state.acl_check(&proc, VM_EXIT - VM_RQ_BASE).is_err());
    assert!(state.acl_check(&proc, VM_FORK - VM_RQ_BASE).is_err());
}
```

### 5.2 Fork 继承测试

```rust
#[test]
fn test_acl_fork_default() {
    let state = AclState::Default;
    assert_eq!(state.acl_fork(), AclState::Default);
}

#[test]
fn test_acl_fork_uninitialized() {
    let state = AclState::Uninitialized;
    assert_eq!(state.acl_fork(), AclState::Uninitialized);
}

#[test]
fn test_acl_fork_system() {
    let mask = AclMask::VM_MMAP | AclMask::VM_MAP_PHYS;
    let state = AclState::System(mask);
    // 系统进程的子进程不继承特权
    assert_eq!(state.acl_fork(), AclState::Uninitialized);
}
```

### 5.3 AclMask 测试

```rust
#[test]
fn test_acl_mask_default() {
    let mask = AclMask::DEFAULT;
    assert!(mask.contains(AclMask::VM_EXIT));
    assert!(mask.contains(AclMask::VM_FORK));
    assert!(mask.contains(AclMask::VM_BRK));
    assert!(mask.contains(AclMask::VM_MMAP));
    assert!(mask.contains(AclMask::VM_MUNMAP));
    assert!(!mask.contains(AclMask::VM_MAP_PHYS));
    assert!(!mask.contains(AclMask::VM_RS_PREPARE));
}

#[test]
fn test_acl_state_mask() {
    assert_eq!(AclState::Uninitialized.mask(), None);
    assert_eq!(AclState::Default.mask(), Some(AclMask::DEFAULT));
    let custom = AclMask::VM_MMAP | AclMask::VM_BRK;
    assert_eq!(AclState::System(custom).mask(), Some(custom));
}
```

---

## 6. 参见

- [01-vmproc-struct.md](01-vmproc-struct.md) - vm_acl 字段
- [17-vm-fork.md](17-vm-fork.md) - fork 时的 ACL 处理

---

*分类: VM私有*
