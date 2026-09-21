# MINIX3 IPC 详细文档

## 目录

1. [IPC 头文件总结](#一ipc-头文件总结)
   - [sys/sys/ipc.h](#1-syssysipch)
   - [kernel/ipc.h](#2-kernelipch)
   - [kernel/ipc_filter.h](#3-kernelipc_filterh)
2. [IPC 常量定义](#二ipc-常量定义)
   - [include/minix/ipcconst.h](#1-includeminixipcconsth)
   - [include/minix/com.h](#2-includeminixcomh)
3. [IPC 数据结构](#三ipc-数据结构)
   - [include/minix/ipc.h](#1-includeminixipch)
   - [include/minix/type.h](#2-includeminixtypeh)
   - [include/minix/const.h](#3-includeminixconsth)
4. [用户态 IPC 实现](#四用户态-ipc-实现)
   - [lib/libc/arch/i386/sys/_ipc.S](#1-liblibcarchi386sys_ipcs)
   - [lib/libc/sys/syscall.c](#2-liblibcsyssyscallc)
5. [总结与关系图](#五总结与关系图)

---

> **注意**: 进程管理相关内容（proc.h, sched_proc 等）已迁移至 [process/process-detail.md](../process/process-detail.md)。
> 
> 系统调用相关内容（system.c, system.h 等）已迁移至 [syscall/syscall-detail.md](../syscall/syscall-detail.md)。
> 
> 中断与时钟相关内容（clock.c, interrupt.c 等）已迁移至 [interrupt/interrupt-detail.md](../interrupt/interrupt-detail.md)。

---

# 一、IPC 头文件总结

## 1. sys/sys/ipc.h

**文件位置**: `/minix3/sys/sys/ipc.h`

**作用**: 定义 System V IPC 的权限结构和常量

### 核心结构体

```c
struct ipc_perm {
    uid_t       uid;    /* 当前用户ID */
    gid_t       gid;    /* 当前组ID */
    uid_t       cuid;   /* 创建者用户ID */
    gid_t       cgid;   /* 创建者组ID */
    mode_t      mode;   /* 权限位 */
    unsigned short _seq; /* 序列号（防旧引用复活）*/
    key_t       _key;   /* 用户指定的键 */
};
```

### 权限标志位

| 常量 | 值（八进制） | 作用 |
|------|--------------|------|
| `IPC_R` | 000400 | 读取权限 |
| `IPC_W` | 000200 | 写入权限 |
| `IPC_M` | 010000 | 修改控制信息权限 |

### 创建标志位

| 常量 | 值 | 作用 |
|------|-----|------|
| `IPC_CREAT` | 001000 | key 不存在时创建 |
| `IPC_EXCL` | 002000 | key 已存在则失败 |
| `IPC_NOWAIT` | 004000 | 不阻塞，立即返回 |

### 控制命令

| 常量 | 值 | 作用 |
|------|-----|------|
| `IPC_PRIVATE` | (key_t)0 | 私有键，创建独立 IPC |
| `IPC_RMID` | 0 | 删除 IPC 对象 |
| `IPC_SET` | 1 | 设置属性 |
| `IPC_STAT` | 2 | 获取属性 |

### ID 转换

```
IPC ID = (序列号 << 16) | 索引

IXSEQ_TO_IPCID(ix, perm)  → 索引+序列号 → 完整ID
IPCID_TO_IX(id)           → 完整ID → 索引
IPCID_TO_SEQ(id)          → 完整ID → 序列号
```

### 关键概念

- **序列号防复活**: 删除 IPC 后序列号递增，旧的 ID 引用自动失效
- **ftok()**: 从文件路径生成唯一 key

---

## 2. kernel/ipc.h

**文件位置**: `minix3/minix/kernel/ipc.h`

**作用**: 定义 MINIX 内核 IPC 的核心常量和宏，是 `proc.c` 中 IPC 实现的基础

### 核心标志位

| 标志 | 值 | 作用 |
|------|-----|------|
| `NON_BLOCKING` | 0x0080 | 非阻塞模式（目标未就绪立即返回） |
| `FROM_KERNEL` | 0x0100 | 消息来自内核（代理进程发送） |

### FROM_KERNEL 标志详解

**两种语义**：

| 场景 | 含义 | 示例 |
|------|------|------|
| **内核代理发送** | 内核代替进程发送消息 | `sys_call()` 中内核代替调用者发送 |
| **内核直接发送** | 内核作为发送者发送消息 | `exception.c` 中内核发送异常消息 |

**代码示例**：

```c
// 场景1：内核代理发送 - proc.c:863
result = mini_send(caller_ptr, src_dst_e, m_ptr, FROM_KERNEL);

// 场景2：内核直接发送 - exception.c:143
m_ptr->m_type = SYN_ALARM;
asynsend(proc_e, m_ptr);  // 内核直接发送，不设置 FROM_KERNEL
```

### WILLRECEIVE 宏

判断目标进程是否准备好接收消息：

```c
#define WILLRECEIVE(src_e,dst_ptr,m_src_v,m_src_p) \
    ((RTS_ISSET(dst_ptr, RTS_RECEIVING) && \
    !RTS_ISSET(dst_ptr, RTS_SENDING)) && \
    CANRECEIVE(dst_ptr->p_getfrom_e,src_e,dst_ptr,m_src_v,m_src_p))
```

**检查条件**：
1. 目标正在接收（`RTS_RECEIVING`）
2. 目标不在发送（`!RTS_SENDING`）
3. 发送者匹配（`CANRECEIVE`）

### CANRECEIVE 宏

判断接收者是否愿意接收来自特定发送者的消息：

```c
#define CANRECEIVE(receive_e,src_e,dst_ptr,m_src_v,m_src_p) \
    (((receive_e) == ANY || (receive_e) == (src_e)) && \
    (priv(dst_ptr)->s_ipcf == NULL || \
    allow_ipc_filtered_msg(dst_ptr,src_e,m_src_v,m_src_p)))
```

**检查条件**：
1. 接收者期望的发送者匹配（`ANY` 或特定端点）
2. 消息过滤器允许（如果设置了过滤器）

### IPC 状态码宏

```c
#define IPC_STATUS_GET(p)       ((p)->p_reg.IPC_STATUS_REG)
#define IPC_STATUS_CLEAR(p)     ((p)->p_reg.IPC_STATUS_REG = 0)
#define IPC_STATUS_ADD(p, m)    do { \
        if(!((p)->p_misc_flags & MF_REPLY_PEND)) { \
            (p)->p_reg.IPC_STATUS_REG |= (m); \
        } \
    } while(0)
```

### MF_REPLY_PEND 与原子性问题

**核心问题**：SENDREC 不是上下文原子的！

```c
/*
 * XXX: SENDREC is not currently atomic for user processes. 
 * A process can return from SENDREC in a different context 
 * when a Posix signal handler gets executed.
 */
```

**时间线**：

```
T1: A 执行 sendrec(B)
    └─► 设置 MF_REPLY_PEND
    └─► mini_send(A → B)
    └─► A 阻塞在接收

T2: B 发送回复
    └─► 设置 MF_DELIVERMSG
    └─► ⚠️ 还没设置返回值！

T3: 信号到达 A
    └─► 保存原始 p_reg 到用户栈
    └─► 修改 p_reg 为信号处理函数

T4: 调度器选择 A 运行
    └─► delivermsg() 设置 p_reg.retreg = OK
    └─► ⚠️ 此时 p_reg 是信号处理函数的上下文！

T5: sigreturn
    └─► 从用户栈恢复原始 p_reg
    └─► ⚠️ 覆盖 T4 设置的返回值！
```

**解决方案**：

```c
// 当前方案：SENDREC 期间不修改 p_reg
if (!(p->p_misc_flags & MF_REPLY_PEND)) {
    p->p_reg.IPC_STATUS_REG |= m;
}

// 更好的方案：将 IPC 状态保存在内核
struct ipc_state {
    int return_value;      // 保存在内核，不会被 sigreturn 覆盖
    endpoint_t reply_from;
};
```

### 与其他文件的关系

| 文件 | 层面 |
|------|------|
| `include/minix/ipcconst.h` | 用户态 IPC 系统调用号 |
| `include/minix/ipc.h` | 用户态消息结构体 |
| **`kernel/ipc.h`** | **内核态 IPC 宏定义** |
| `kernel/proc.c` | 内核态 IPC 实现 |

---

## 3. kernel/ipc_filter.h

**文件位置**: `/minix3/minix/kernel/ipc_filter.h`

**作用**: 让进程选择性接收/拒绝某些消息

### 过滤器类型

| 类型 | 值 | 含义 |
|------|-----|------|
| `IPCF_NONE` | 0 | 无过滤器（接收所有） |
| `IPCF_BLACKLIST` | 1 | 黑名单（拒绝名单中的） |
| `IPCF_WHITELIST` | 2 | 白名单（只接收名单中的） |

### 进程类型判断

| 宏 | 判断 |
|----|------|
| `IPCF_IS_USR_EP(EP)` | 用户进程？ |
| `IPCF_IS_SYS_EP(EP)` | 系统服务？ |
| `IPCF_IS_TSK_EP(EP)` | 内核任务？ |

### 过滤器结构体

```c
struct ipc_filter_s {
    int type;              // NONE/BLACKLIST/WHITELIST
    int num_elements;     // 过滤规则数量
    int flags;            // 标志位
    struct ipc_filter_s *next;   // 链表指针
    ipc_filter_el_t elements[];  // 过滤元素数组
};
```

### 匹配标志

| 标志 | 含义 |
|------|------|
| `IPCF_MATCH_M_TYPE` | 按消息类型匹配 |
| `IPCF_MATCH_M_SOURCE` | 按发送者匹配 |

### 过滤器池

```
IPCF_POOL_SIZE = 2 * NR_SYS_PROCS

预分配固定数量过滤器，避免运行时动态分配
```

---

## 4. include/minix/ipc_filter.h

**文件位置**: `minix3/minix/include/minix/ipc_filter.h`

**作用**: 定义用户态 IPC 过滤器接口，是 `kernel/ipc_filter.h` 的用户空间对应头文件

### 特殊消息来源

```c
#define ANY_USR  _ENDPOINT(1, _ENDPOINT_P(ANY))  // 任意用户进程
#define ANY_SYS  _ENDPOINT(2, _ENDPOINT_P(ANY))  // 任意系统服务
#define ANY_TSK  _ENDPOINT(3, _ENDPOINT_P(ANY))  // 任意内核任务
```

**设计目的**：在过滤器中支持"任意某类进程"的匹配，而不是具体的端点号。

### 过滤器常量

```c
#define IPCF_MAX_ELEMENTS  (NR_SYS_PROCS * 2)  // 每个过滤器最多元素数
```

### 过滤器元素标志

| 标志 | 值 | 含义 |
|------|-----|------|
| `IPCF_MATCH_M_SOURCE` | 0x1 | 按发送者匹配 |
| `IPCF_MATCH_M_TYPE` | 0x2 | 按消息类型匹配 |
| `IPCF_EL_BLACKLIST` | 0x4 | 黑名单元素 |
| `IPCF_EL_WHITELIST` | 0x8 | 白名单元素 |

### 过滤器元素结构体

```c
struct ipc_filter_el_s {
    int flags;           // 匹配标志
    endpoint_t m_source; // 发送者端点（或 ANY_USR/SYS/TSK）
    int m_type;          // 消息类型
};
typedef struct ipc_filter_el_s ipc_filter_el_t;
```

**内存布局**：
```
┌─────────────────────────────────────────────────────┐
│  flags (4B)  │  m_source (4B)  │  m_type (4B)      │
│  匹配规则    │  发送者端点     │  消息类型          │
└─────────────────────────────────────────────────────┘
         总计: 12 bytes
```

### 两文件关系

| 文件 | 层面 | 内容 |
|------|------|------|
| `include/minix/ipc_filter.h` | 用户态接口 | 过滤器元素定义、常量 |
| `kernel/ipc_filter.h` | 内核态实现 | 过滤器结构体、池管理、匹配宏 |

### 使用示例

```c
// 只接收来自 PM 的 PM_EXIT 消息
ipc_filter_el_t el = {
    .flags = IPCF_MATCH_M_SOURCE | IPCF_MATCH_M_TYPE,
    .m_source = PM_PROC_NR,
    .m_type = PM_EXIT
};

// 拒绝所有来自用户进程的消息
ipc_filter_el_t el = {
    .flags = IPCF_MATCH_M_SOURCE | IPCF_EL_BLACKLIST,
    .m_source = ANY_USR,
    .m_type = 0  // 不匹配消息类型
};
```

---

### 现代硬件与 Rust 重构建议

#### 1. 类型安全的过滤器

**C 语言问题**：`flags` 字段使用位运算，编译期无法检查组合合法性。

```c
// C: 运行时才能发现错误
el.flags = IPCF_EL_BLACKLIST | IPCF_EL_WHITELIST;  // 矛盾！
```

**Rust 重构**：使用枚举和类型状态模式。

```rust
#[derive(Clone, Copy, Debug)]
pub enum FilterAction {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug)]
pub enum SourceMatcher {
    AnyUser,      // ANY_USR
    AnySystem,    // ANY_SYS
    AnyTask,      // ANY_TSK
    Specific(Endpoint),
}

#[derive(Clone, Copy, Debug)]
pub struct FilterElement {
    pub action: FilterAction,
    pub source: Option<SourceMatcher>,
    pub msg_type: Option<MessageType>,
}

impl FilterElement {
    pub fn matches(&self, msg: &Message) -> bool {
        let source_match = match self.source {
            None => true,
            Some(SourceMatcher::AnyUser) => msg.source.is_user_process(),
            Some(SourceMatcher::AnySystem) => msg.source.is_system_service(),
            Some(SourceMatcher::AnyTask) => msg.source.is_kernel_task(),
            Some(SourceMatcher::Specific(ep)) => msg.source == ep,
        };
        
        let type_match = match self.msg_type {
            None => true,
            Some(t) => msg.m_type == t,
        };
        
        source_match && type_match
    }
}
```

#### 2. 过滤器池的 Arena 分配器

**C 语言问题**：全局静态数组，大小固定，无法动态扩展。

```c
EXTERN ipc_filter_t ipc_filter_pool[IPCF_POOL_SIZE];
```

**Rust 重构**：使用 Arena 分配器或 `Box`。

```rust
use std::collections::VecDeque;

pub struct FilterPool {
    filters: Vec<Option<Filter>>,
    free_list: VecDeque<usize>,
}

impl FilterPool {
    pub fn new(capacity: usize) -> Self {
        let mut free_list = VecDeque::with_capacity(capacity);
        for i in 0..capacity {
            free_list.push_back(i);
        }
        Self {
            filters: vec![None; capacity],
            free_list,
        }
    }
    
    pub fn allocate(&mut self, filter_type: FilterType) -> Option<FilterRef> {
        self.free_list.pop_front().map(|idx| {
            self.filters[idx] = Some(Filter::new(filter_type));
            FilterRef(idx)
        })
    }
    
    pub fn deallocate(&mut self, idx: FilterRef) {
        self.filters[idx.0] = None;
        self.free_list.push_back(idx.0);
    }
}
```

#### 3. 过滤器链的安全遍历

**C 语言问题**：链表遍历需要手动检查 NULL。

```c
struct ipc_filter_s *f = p->p_ipcf;
while (f != NULL) {
    // 处理过滤器
    f = f->next;
}
```

**Rust 重构**：使用迭代器。

```rust
impl Process {
    pub fn filter_chain(&self) -> impl Iterator<Item = &Filter> {
        std::iter::successors(
            self.ipc_filter.as_ref(),
            |f| f.next.as_ref()
        )
    }
}

// 使用
for filter in process.filter_chain() {
    if filter.matches(&msg) {
        return filter.action;
    }
}
```

#### 4. 现代 64 位硬件优化

| 方面 | C 原实现 | Rust 重构 |
|------|----------|-----------|
| **过滤器元素大小** | 12 字节（未对齐） | 16 字节（对齐到缓存行） |
| **匹配算法** | 线性扫描 | 可考虑 BTreeMap 或哈希表 |
| **缓存友好** | 分散在内存 | 可使用 `Vec` 连续存储 |

```rust
// 缓存友好的过滤器存储
#[repr(align(64))]
pub struct CacheAlignedFilter {
    pub filter: Filter,
    _padding: [u8; 64 - std::mem::size_of::<Filter>()],
}
```

#### 5. 错误处理

**C 语言问题**：过滤器设置失败时返回错误码，容易忘记检查。

```c
int r = set_filter(&el);
// 忘记检查 r...
```

**Rust 重构**：使用 `Result` 强制处理错误。

```rust
pub fn set_filter(&mut self, element: FilterElement) -> Result<(), FilterError> {
    if self.elements.len() >= MAX_ELEMENTS {
        return Err(FilterError::TooManyElements);
    }
    self.elements.push(element);
    Ok(())
}

// 调用者必须处理错误
process.set_filter(element)?;  // 自动传播错误
```

---

### 三文件对比

| 方面 | sys/sys/ipc.h | kernel/ipc.h | kernel/ipc_filter.h |
|------|---------------|--------------|---------------------|
| **作用** | 权限管理（谁能访问） | IPC 宏定义（内核态） | 消息过滤（收什么消息） |
| **层面** | 系统级 | 内核级 | 进程级 |
| **检查时机** | IPC 对象创建/访问时 | 消息发送/接收时 | 消息到达时 |
| **默认值** | 允许访问 | 无过滤器 | 允许接收 |

### 完整流程

```
┌─────────────────────────────────────────────────────────┐
│                    IPC 消息传递流程                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  发送方                                                  │
│    │                                                   │
│    ▼                                                   │
│  ┌─────────────────┐                                   │
│  │ 检查 ipc_perm   │ ← sys/sys/ipc.h: 权限检查         │
│  │ (uid/gid/mode) │                                   │
│  └────────┬────────┘                                   │
│           │ 允许                                         │
│           ▼                                            │
│  ┌─────────────────┐                                   │
│  │ WILLRECEIVE?    │ ← kernel/ipc.h: 接收就绪检查      │
│  │ CANRECEIVE?     │                                   │
│  └────────┬────────┘                                   │
│           │ 就绪                                         │
│           ▼                                            │
│  ┌─────────────────┐                                   │
│  │ 消息发送到目标  │                                   │
│  └────────┬────────┘                                   │
│           │                                            │
│           ▼                                            │
│  接收方                                                  │
│    │                                                   │
│    ▼                                                   │
│  ┌─────────────────┐                                   │
│  │ 检查 ipc_filter │ ← kernel/ipc_filter.h: 消息过滤   │
│  │ (白名单/黑名单) │                                   │
│  └────────┬────────┘                                   │
│           │ 允许                                         │
│           ▼                                            │
│  ┌─────────────────┐                                   │
│  │ 交付给进程     │                                   │
│  └─────────────────┘                                   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

---

# 二、IPC 常量定义

## 1. include/minix/ipcconst.h

**文件位置**: `minix3/minix/include/minix/ipcconst.h`

**作用**: 定义 MINIX IPC 系统调用号和常量

### 核心系统调用号

| 调用 | 编号 | 阻塞？ | 说明 |
|------|------|--------|------|
| SEND | 1 | ✅ | 阻塞发送，等待对方接收 |
| RECEIVE | 2 | ✅ | 阻塞接收，等待消息 |
| SENDREC | 3 | ✅ | 原子操作：发送+等待回复 |
| NOTIFY | 4 | ❌ | 异步通知（仅传递端点号） |
| SENDNB | 5 | ❌ | 非阻塞发送（尝试型，失败即返回） |
| KERNINFO | 6 | ❌ | 获取内核信息 |
| SENDA | 16 | ❌ | 异步发送（排队型，内核稍后投递） |

### SENDNB vs SENDA 区别

| 特性 | SENDNB | SENDA |
|------|--------|-------|
| **行为** | 试发，不行就放弃 | 放入队列，稍后投递 |
| **排队** | ❌ 不排队 | ✅ 排队 |
| **失败** | 立即返回错误 | 自动重试 |

**记忆口诀**：
- SENDNB = "试试能不能发"
- SENDA = "放着慢慢发"

### 消息大小

MINIX 消息结构体固定 **56 字节**

### 状态码结构（32位）

```
┌───────────────┬─────────────────┬─────────────┐
│   保留(14位)  │   Flags (12位)  │  调用号(6位)│
│               │  Bit17: FROM_   │  1=SEND     │
│               │   KERNEL        │  2=RECEIVE  │
│               │                 │  3=SENDREC  │
│               │                 │  4=NOTIFY   │
│               │                 │  5=SENDNB   │
│               │                 │  16=SENDA   │
└───────────────┴─────────────────┴─────────────┘
```

### 与其他 IPC 文件的关系

| 文件 | 层面 |
|------|------|
| `sys/sys/ipc.h` | System V IPC 权限（msgget/semget） |
| `kernel/ipc_filter.h` | 进程级消息过滤 |
| **`minix/include/minix/ipcconst.h`** | **MINIX 核心 IPC 原语** |

**一句话概括**：ipcconst.h 定义了 MINIX 特有的轻量级 IPC 原语（SEND/RECEIVE/NOTIFY），比传统 System V 更简洁高效。

---

## 2. include/minix/com.h

**文件位置**: `minix3/minix/include/minix/com.h`

**作用**: 定义 Minix3 所有系统进程的消息类型编号，是 IPC 通信的"消息字典"

### 核心概念：消息类型编号空间

Minix3 为每个子系统分配 256 个消息号（0x00-0xFF），避免不同服务的消息类型冲突。

### 消息类型编号总览

| 编号范围 | 子系统 | 说明 |
|----------|--------|------|
| 0x000-0x0FF | PM | 进程管理 |
| 0x100-0x1FF | VFS | 虚拟文件系统 |
| 0x200-0x2FF | DL | 数据链路层/网络驱动 |
| 0x300-0x3FF | BUS | 总线控制器 (PCI/I2C) |
| 0x400-0x4FF | CDEV | 字符设备 |
| 0x500-0x5FF | BDEV | 块设备 |
| 0x600-0x6FF | KERNEL_CALL | 内核系统调用 |
| 0x700-0x7FF | RS | 重生服务 |

### 重要消息类型

#### 进程管理 (PM)
| 消息 | 值 | 说明 |
|------|-----|------|
| `PM_FORK` | 0x01 | 创建子进程 |
| `PM_EXIT` | 0x02 | 进程退出 |
| `PM_EXEC` | 0x04 | 执行新程序 |
| `PM_SETUID` | 0x09 | 设置用户ID |
| `PM_SETGID` | 0x0A | 设置组ID |

#### 虚拟文件系统 (VFS)
| 消息 | 值 | 说明 |
|------|-----|------|
| `VFS_READ` | 0x101 | 读取文件 |
| `VFS_WRITE` | 0x102 | 写入文件 |
| `VFS_OPEN` | 0x103 | 打开文件 |
| `VFS_CLOSE` | 0x104 | 关闭文件 |
| `VFS_STAT` | 0x105 | 获取文件状态 |

#### 内核调用 (KERNEL_CALL)
| 消息 | 值 | 说明 |
|------|-----|------|
| `SYS_FORK` | 0x601 | 内核 fork |
| `SYS_EXEC` | 0x602 | 内核 exec |
| `SYS_EXIT` | 0x603 | 内核 exit |
| `SYS_NICE` | 0x604 | 设置优先级 |
| `SYS_PRIVCTL` | 0x605 | 特权控制 |

### 进程端点号定义

| 进程 | 端点号 | 说明 |
|------|--------|------|
| `PM_PROC_NR` | 0 | 进程管理器 |
| `VFS_PROC_NR` | 1 | 虚拟文件系统 |
| `RS_PROC_NR` | 2 | 重生服务 |
| `VM_PROC_NR` | 3 | 虚拟内存管理器 |
| `LOG_PROC_NR` | 4 | 日志服务 |
| `TTY_PROC_NR` | 5 | 终端驱动 |
| `DS_PROC_NR` | 6 | 数据存储服务 |
| `INIT_PROC_NR` | 7 | 初始化进程 |

### 特殊端点号

| 端点号 | 说明 |
|--------|------|
| `ANY` | -1 | 接收来自任何进程的消息 |
| `NONE` | -2 | 无特定端点 |
| `SELF` | -3 | 自己 |
| `SENDALL` | -4 | 广播给所有进程 |

---

# 三、IPC 数据结构

## 1. include/minix/ipc.h

**文件位置**: `minix3/minix/include/minix/ipc.h`

**作用**: 定义 Minix3 所有 IPC 消息的结构体格式，是进程间通信的核心数据结构

### 核心概念：固定大小消息

Minix3 的 IPC 消息大小固定为 **64 字节**，设计原因：
- 内存池管理简单（所有消息用同一内存池）
- 零拷贝：直接传递消息指针，不需要复制
- 无碎片：不会产生内存碎片
- 快速分配/释放：O(1) 时间复杂度

### 基础类型定义

```c
typedef int endpoint_t;      // 进程端点号 (4 bytes)
typedef size_t vir_bytes;    // 虚拟地址字节数 (8 bytes)
typedef unsigned cp_grant_id_t;  // 授权ID (4 bytes)
```

### 通用消息结构 (mess_1 到 mess_10)

Minix3 早期设计的通用消息格式，每种有不同的字段组合：

| 结构 | 字段类型 | 用途 |
|------|----------|------|
| `mess_1` | 3个int + 4个指针 + 1个64位 | 通用参数传递 |
| `mess_2` | 2个int + 2个long + 1个指针 + sigset_t | 带信号集 |
| `mess_3` | 2个int + 1个指针 + 1个char* | 带字符数组 |
| `mess_4` | 1个long + 5个long | 大量长整型 |
| `mess_7` | 5个int + 2个指针 | 通用消息 |
| `mess_9` | 5个long + 4个short + 2个64位 | 大量数值 |
| `mess_10` | 4个int + 3个long + 1个64位 | 混合类型 |

### 特定服务消息分类

#### VFS 相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_vfs_fs_readwrite` | 文件读写 |
| `mess_vfs_fs_getdents` | 读取目录 |
| `mess_vfs_fs_lookup` | 路径查找 |
| `mess_vfs_fs_create` | 创建文件 |
| `mess_vfs_fs_mknod` | 创建特殊文件 |
| `mess_vfs_fs_mkdir` | 创建目录 |
| `mess_vfs_fs_link` | 创建硬链接 |
| `mess_vfs_fs_slink` | 创建符号链接 |
| `mess_vfs_fs_unlink` | 删除文件 |
| `mess_vfs_fs_rename` | 重命名 |
| `mess_vfs_fs_stat` | 获取文件状态 |
| `mess_vfs_fs_chmod` | 修改权限 |
| `mess_vfs_fs_chown` | 修改所有者 |
| `mess_vfs_fs_flush` | 刷新缓冲区 |
| `mess_vfs_fs_ftrunc` | 截断文件 |

#### VM (虚拟内存) 相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_vm_vfs_mmap` | 内存映射文件 |
| `mess_vmmcp` | 虚拟内存页面复制 |
| `mess_vmmcp_reply` | vmmcp 响应 |

#### Socket/网络相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_vfs_lsockdriver_socket` | 创建 socket |
| `mess_vfs_lsockdriver_sendrecv` | 发送/接收数据 |
| `mess_vfs_lsockdriver_addr` | 地址绑定/获取 |
| `mess_vfs_lsockdriver_getset` | 获取/设置选项 |
| `mess_vfs_lsockdriver_select` | socket select |

#### 字符设备相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_vfs_lchardriver_readwrite` | 读写字符设备 |
| `mess_vfs_lchardriver_openclose` | 打开/关闭设备 |
| `mess_vfs_lchardriver_select` | 设备 select |

#### PM (进程管理) 相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_rs_pm_exec_restart` | 执行重启 |
| `mess_rs_pm_srv_kill` | 终止服务进程 |
| `mess_rs_req` | RS 请求 |
| `mess_rs_update` | RS 状态更新 |

#### 信号相关消息

| 消息名 | 用途 |
|--------|------|
| `mess_sigcalls` | 信号系统调用 (SIGKILL, SIGSEND 等) |

### 核心结构体：message

```c
typedef struct noxfer_message {
    endpoint_t m_source;    // 发送者进程号 (4 bytes)
    int m_type;             // 消息类型 (4 bytes)
    union {
        // 56 字节的消息数据
        mess_u8        m_u8;
        mess_1         m_m1;
        mess_vfs_readwrite m_vfs_readwrite;
        // ... 更多类型
        u8_t size[56];     // 原始字节数组
    };
} message __ALIGNED(16);
```

**消息布局**:
```
┌────────────────────────────────────────┐
│  m_source    │ m_type    │   union{}   │
│   4 bytes    │  4 bytes  │   56 bytes  │
└────────────────────────────────────────┘
         8 bytes            56 bytes
              总计: 64 bytes
```

### 编译时断言

```c
typedef int _ASSERT_message[sizeof(message) == 64 ? 1 : -1];
```

确保 message 结构体大小正好是 64 字节，保证二进制兼容性。

### 异步消息结构体

```c
typedef struct asynmsg {
    unsigned flags;      // 标志位 (AMF_EMPTY/VALID/DONE/NOTIFY)
    endpoint_t dst;    // 目标端点
    int result;         // 结果
    message msg;        // 消息内容
} asynmsg_t;
```

**异步标志**:
| 标志 | 含义 |
|------|------|
| `AMF_EMPTY` | 槽位未使用 |
| `AMF_VALID` | 消息有效 |
| `AMF_DONE` | 内核已处理 |
| `AMF_NOTIFY` | 完成后通知 |
| `AMF_NOREPLY` | 无需回复 |

### IPC 函数向量表

```c
struct minix_ipcvecs {
    int (*send)(endpoint_t dest, message *m_ptr);
    int (*receive)(endpoint_t src, message *m_ptr, int *st);
    int (*sendrec)(endpoint_t src_dest, message *m_ptr);
    int (*sendnb)(endpoint_t dest, message *m_ptr);
    int (*notify)(endpoint_t dest);
    int (*do_kernel_call)(message *m_ptr);
    int (*senda)(asynmsg_t *table, size_t count);
};
```

这是运行时绑定的函数指针表，允许灵活替换 IPC 实现。

### IPC 系统调用 API

| 函数 | 作用 | 特点 |
|------|------|------|
| `ipc_send()` | 同步发送 | 阻塞等待接收 |
| `ipc_receive()` | 同步接收 | 阻塞等待发送 |
| `ipc_sendrec()` | 发送并接收 | 发送后等待回复 |
| `ipc_sendnb()` | 非阻塞发送 | 立即返回 |
| `ipc_notify()` | 轻量通知 | 只传信号，无数据 |
| `ipc_senda()` | 异步批量发送 | 批量非阻塞 |

### 理论关联：Minix3 微内核架构

```
┌─────────────────────────────────────────────────────────┐
│           Minix3 微内核消息传递                          │
│                                                         │
│   ┌──────────┐         ┌──────────┐                    │
│   │  进程A   │  消息    │  进程B   │                    │
│   │          │─────────▶│          │                    │
│   └──────────┘         └──────────┘                    │
│                                                         │
│   用户空间服务进程之间通过 IPC 通信                       │
│   内核只负责消息传递，不执行业务逻辑                      │
│                                                         │
│   消息大小固定 64 字节，确保高效传递                     │
└─────────────────────────────────────────────────────────┘
```

---

## 2. include/minix/type.h

**文件位置**: `minix3/minix/include/minix/type.h`

**作用**: 定义 Minix3 的基本数据类型和结构体

### 核心类型分类

#### 1. 进程和端点类型

| 类型 | 定义 | 说明 |
|------|------|------|
| `endpoint_t` | `int` | 进程端点号（带版本号）|
| `proc_nr_t` | `int` | 进程槽位编号（0-1023）|

#### 2. 地址类型

| 类型 | 定义 | 说明 |
|------|------|------|
| `vir_bytes` | `size_t` | 虚拟地址字节数 |
| `vir_clicks` | `size_t` | 虚拟地址 CLICK 数 |
| `phys_bytes` | `u64_t` | 物理地址字节数 |
| `phys_clicks` | `u64_t` | 物理地址 CLICK 数 |

#### 3. I/O 向量

```c
typedef struct iov_grant_iter_s {
    cp_grant_id_t igj_vir;    // 授权 ID
    endpoint_t igj_who;       // 目标进程
    size_t igj_offset;        // 偏移量
} iov_grant_iter_t;
```

#### 4. 信号相关

```c
typedef struct __siginfo {
    int si_signo;      // 信号编号
    int si_code;       // 信号来源
    pid_t si_pid;      // 发送者 PID
    uid_t si_uid;      // 发送者 UID
    // ... 更多字段
} siginfo_t;
```

#### 5. 负载和时钟

| 结构体 | 用途 |
|--------|------|
| `cpuavg` | 单进程 CPU 利用率（衰减平均值）|
| `loadinfo` | 系统负载历史（150 个采样点，15 分钟）|
| `kclockinfo` | 系统时钟（启动时间、运行时间、频率）|

#### 6. 机器和资源结构体

| 结构体 | 用途 |
|--------|------|
| `machine` | 机器硬件信息（CPU 数量、APIC、ACPI）|
| `io_range` | I/O 端口范围授权 |
| `minix_mem_range` | 物理内存范围授权 |
| `boot_image` | 系统启动时加载的进程列表 |
| `memory` | 物理内存块描述 |

#### 7. 内核数据结构体

| 结构体 | 用途 |
|--------|------|
| `kmessages` | 内核日志环形缓冲区 |
| `k_randomness` | 内核随机数池（16 源 × 64 元素）|
| `kuserinfo` | 暴露给用户程序的信息（栈指针）|
| `minix_kerninfo` | 内核总信息结构体（所有指针）|

#### 8. ABI 兼容性

- **KERNINFO_MAGIC** = 0xfc3b84bf：魔数，验证结构体有效性
- **ABI 限制**：minix_kerninfo 的布局不能随便改，否则破坏用户程序

**一句话概括**：type.h 是 Minix3 的"基本数据类型字典"，定义了进程标识、地址类型、I/O 向量、信号、负载、时钟、资源管理等核心结构体。

---

## 3. include/minix/const.h

**文件位置**: `minix3/minix/include/minix/const.h`

**作用**: 定义 Minix3 系统使用的各种常量

### 编译相关常量

| 常量 | 值 | 含义 |
|------|------|------|
| `UNUSED` | 宏 | 抑制未使用参数警告 |
| `EXTERN` | extern | 外部变量声明 |
| `TRUE` | 1 | 真 |
| `FALSE` | 0 | 假 |
| `SUPER_USER` | ((uid_t) 0) | 超级用户 uid = 0 |

### 向量和 I/O 常量

| 常量 | 值 | 含义 |
|------|------|------|
| `SCPVEC_NR` | 64 | 安全复制最大条目数 |
| `MAPVEC_NR` | 64 | 虚拟映射最大条目数 |
| `NR_IOREQS` | 64 | I/O 请求最大条目数 |
| `VUA_READ` | 0x01 | 虚拟内存读权限 |
| `VUA_WRITE` | 0x02 | 虚拟内存写权限 |

### 内存常量

| 常量 | 值 | 含义 |
|------|------|------|
| `CLICK_SIZE` | 4096 | 内存分配单元（4KB 页）|
| `CLICK_SHIFT` | 12 | log2(4096) = 12 |
| `SEGMENT_TYPE` | 0xFF00 | 段类型掩码（高8位）|
| `SEGMENT_INDEX` | 0x00FF | 段索引掩码（低8位）|
| `VM_D` | 0x1001 | VM 虚拟地址 |
| `VM_GRANT` | 0x1003 | VM 授权内存 |

### Click 对齐宏

```c
#define CLICK_FLOOR(n)  (((n) / CLICK_SIZE) * CLICK_SIZE)  // 向下对齐
#define CLICK_CEIL(n)   CLICK_FLOOR((n) + CLICK_SIZE-1)    // 向上对齐
```

### 文件 Inode 模式位

| 常量 | 值 | 含义 |
|------|------|------|
| `I_TYPE` | 0170000 | 文件类型掩码 |
| `I_REGULAR` | 0100000 | 普通文件 |
| `I_DIRECTORY` | 0040000 | 目录 |
| `I_BLOCK_SPECIAL` | 0060000 | 块设备 |
| `I_CHAR_SPECIAL` | 0020000 | 字符设备 |
| `I_SYMBOLIC_LINK` | 0120000 | 符号链接 |
| `I_NAMED_PIPE` | 0010000 | 管道/FIFO |
| `I_SET_UID_BIT` | 0004000 | SUID 位 |
| `I_SET_GID_BIT` | 0002000 | SGID 位 |
| `I_SET_STCKY_BIT` | 0001000 | Sticky 位 |

### 限制值

| 常量 | 值 | 含义 |
|------|------|------|
| `MAX_INODE_NR` | 037777777777 | 最大 inode 编号 |
| `MAX_FILE_POS` | 0x7FFFFFFF | 最大文件偏移（2GB-1）|
| `MAX_SYM_LOOPS` | 8 | 符号链接最大递归次数 |
| `NO_BLOCK` | 0 | 无块编号 |
| `NO_ENTRY` | 0 | 无目录项 |
| `INVAL_UID` | -1 | 无效 uid |
| `INVAL_GID` | -1 | 无效 gid |

### 进程特权标志

| 常量 | 值 | 含义 |
|------|------|------|
| `PREEMPTIBLE` | 0x002 | 可抢占 |
| `BILLABLE` | 0x004 | 可计费 |
| `DYN_PRIV_ID` | 0x008 | 动态特权 ID |
| `SYS_PROC` | 0x010 | 系统进程 |
| `CHECK_IO_PORT` | 0x020 | 检查 I/O 端口 |
| `CHECK_IRQ` | 0x040 | 检查 IRQ |
| `CHECK_MEM` | 0x080 | 检查内存映射 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程 |
| `VM_SYS_PROC` | 0x200 | VM 系统进程 |
| `LU_SYS_PROC` | 0x400 | 实时更新系统进程 |
| `RST_SYS_PROC` | 0x800 | 重启系统进程 |

### 启动和调试常量

| 常量 | 值 | 含义 |
|------|------|------|
| `VERBOSEBOOT_QUIET` | 0 | 安静启动 |
| `VERBOSEBOOT_BASIC` | 1 | 基本启动信息 |
| `VERBOSEBOOT_EXTRA` | 2 | 额外启动信息 |
| `VERBOSEBOOT_MAX` | 3 | 最大启动信息 |
| `PMAGIC` | 0xC0FFEE1 | 进程结构魔数 |

### CPU 特性标志

| 常量 | 值 | 含义 |
|------|------|------|
| `MKF_I386_INTEL_SYSENTER` | 1<<0 | Intel SYSENTER 支持 |
| `MKF_I386_AMD_SYSCALL` | 1<<1 | AMD SYSCALL 支持 |
| `MINIX_CPUSTATES` | 5 | CPU 状态数量 |

### 网络常量

| 常量 | 值 | 含义 |
|------|------|------|
| `NDEV_ETH_PACKET_MIN` | 60 | 最小以太网帧（字节）|
| `NDEV_ETH_PACKET_MAX` | 1514 | 最大以太网帧（字节）|
| `NDEV_ETH_PACKET_TAG` | 4 | VLAN 标签大小 |
| `NDEV_ETH_PACKET_CRC` | 4 | CRC 校验大小 |

**一句话概括**：const.h 是 Minix3 的"常量百科全书"，定义了布尔值、内存页大小、文件类型、权限位、进程特权、启动选项、网络参数等各种系统常量。

---

# 四、用户态 IPC 实现

## 1. lib/libc/arch/i386/sys/_ipc.S

**文件位置**: `minix3/minix/lib/libc/arch/i386/sys/_ipc.S`

**作用**: 用户态 IPC 系统调用的汇编入口，是用户程序进入内核进行消息传递的桥梁。

**核心思想**: 通过软件中断 `int $33` 从用户态切换到内核态，实现进程间通信。

### 核心函数

#### 六个 IPC 入口函数

| 函数名 | 作用 | 参数 | 特点 |
|--------|------|------|------|
| `_ipc_send_intr` | 同步发送 | dest, msg | 阻塞直到接收者就绪 |
| `_ipc_receive_intr` | 同步接收 | src, msg | 阻塞直到发送者就绪 |
| `_ipc_sendrec_intr` | 发送并接收 | dest, msg | RPC 核心，原子操作 |
| `_ipc_notify_intr` | 异步通知 | dest | 非阻塞，只传信号 |
| `_ipc_sendnb_intr` | 非阻塞发送 | dest, msg | 立即返回 |
| `_ipc_senda_intr` | 批量异步发送 | table, count | 批量非阻塞 |

#### 统一调用约定

所有函数遵循相同的寄存器约定：

| 寄存器 | 用途 | 说明 |
|--------|------|------|
| `%eax` | 目标/源端点 | 第一个参数 |
| `%ebx` | 消息指针/表指针 | callee-saved，必须保存 |
| `%ecx` | 调用类型 | SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA |
| `%eax` | 返回值 | 函数返回结果 |

### 关键机制

#### 软件中断 `int $33`

```asm
movl    SRC_DST(%ebp), %eax    ; 目标端点
movl    MESSAGE(%ebp), %ebx    ; 消息指针
movl    $SENDREC, %ecx         ; 调用类型
int     $IPCVEC_INTR           ; 触发中断 33
```

**CPU 自动完成**：
1. 从 TSS.RSP0 读取内核栈指针
2. 压入 SS, ESP, EFLAGS, CS, EIP
3. 切换到内核栈
4. 跳转到 IDT[33] 处理程序

#### 栈帧布局

```
高地址
        │
        │  [调用者的栈帧]
        │
        ├─────────────────┐
        │  返回地址        │  ← 4(%ebp)
        ├─────────────────┤
        │  保存的 %ebp     │  ← 0(%ebp)，ebp 指向这里
        ├─────────────────┤
        │  保存的 %ebx     │  ← -4(%ebp)，callee-saved
        ├─────────────────┤
        │  [局部变量]      │
        └─────────────────┘
低地址

参数访问：
  SRC_DST  = 8(%ebp)   ; 第一个参数
  MESSAGE  = 12(%ebp)  ; 第二个参数
```

#### callee-saved 寄存器

**必须保存的寄存器** (`push %ebx` / `pop %ebx`)：
- `%ebx`：调用者可能存放重要数据

**可以破坏的寄存器** (caller-saved)：
- `%eax`：用于返回值
- `%ecx`：用于调用类型

### 设计决策

#### 为什么用 `int $33` 而不是 `syscall`？

| 方式 | 优点 | 缺点 | Minix3 选择 |
|------|------|------|-------------|
| `int $33` | 兼容所有 x86 | 较慢（100-200 周期） | ✅ 使用 |
| `syscall` | 快（30-50 周期） | 需要现代 CPU | ❌ 未使用 |

**原因**：
- Minix3 是教学系统，兼容性优先
- 代码简单，易于理解
- 性能不是首要目标

**2026 年现状**：`syscall` 已成为 x86-64 标准，Intel 和 AMD 都支持。

#### 为什么分离 IPC 和普通系统调用？

| 中断号 | 用途 |
|--------|------|
| 32 | 普通系统调用（kernel call） |
| 33 | IPC 系统调用（消息传递） |
| 34-35 | 用户映射优化版本 |

**好处**：
- IPC 是高频操作，单独优化路径
- 减少分支判断，提高性能

#### 为什么用寄存器传递参数？

```asm
; 寄存器传递（当前实现）
movl    %eax, dest      ; 1 条指令
movl    %ebx, msg       ; 1 条指令
int     $33             ; 触发中断

; 栈传递（假设）
pushl   msg             ; 内存写入
pushl   dest            ; 内存写入
int     $33
addl    $8, %esp        ; 清理栈
```

**寄存器传递优势**：
- 更快（无需内存访问）
- 更简单（无需栈操作）
- 内核可以直接读取寄存器

### 与内核的协作

#### 完整调用链

```
用户态程序
    │
    ▼
_ipc_sendrec_intr()  ← _ipc.S（本文件）
    │
    │  int $33
    ▼
mpx.S（内核入口）
    │
    ▼
do_ipc()（内核处理）
    │
    ▼
mini_send/mini_receive/mini_notify  ← proc.c
    │
    ▼
消息传递到目标进程
```

#### 内核返回路径

1. 内核修改消息内容（写入回复）
2. 内核恢复用户态寄存器
3. `iret` 返回用户态
4. 用户态从 `%eax` 读取返回值
5. 用户态从消息结构读取回复数据

---

## 2. lib/libc/sys/syscall.c

**文件位置**: `minix3/minix/lib/libc/sys/syscall.c`

**作用**: 用户态系统调用的包装函数，提供高层抽象，隐藏 IPC 细节。

**核心思想**: 将系统调用号封装到消息中，处理错误码转换，提供 C 语言友好的接口。

### 代码结构

```c
#include <sys/cdefs.h>
#include <lib.h>
#include "namespace.h"

#ifdef __weak_alias
__weak_alias(syscall, _syscall)
#endif

int _syscall(endpoint_t who, int syscallnr, message *msgptr)
{
  int status;

  msgptr->m_type = syscallnr;           // 1. 设置系统调用号
  status = ipc_sendrec(who, msgptr);    // 2. 发送并等待回复
  if (status != 0) {
    /* IPC 本身失败 */
    msgptr->m_type = status;
  }
  if (msgptr->m_type < 0) {             // 3. 检查返回值
    errno = -msgptr->m_type;            // 负数 = 错误码
    return(-1);
  }
  return(msgptr->m_type);               // 非负数 = 成功结果
}
```

### 核心机制

#### 1. m_type 的双重语义

| 阶段 | m_type 的值 | 含义 |
|------|-------------|------|
| 发送前 | `syscallnr` (正数) | 系统调用号（告诉服务器做什么） |
| 返回后 | `>= 0` 或 `< 0` | 返回值（服务器告诉结果） |

**设计意图**：复用同一个字段，减少消息大小，避免额外的内存拷贝。

**问题**：语义混淆，可读性差，容易出错。

**Rust 改进**：使用类型状态模式，分离 `RequestMessage` 和 `ResponseMessage`。

#### 2. 弱别名机制

```c
#ifdef __weak_alias
__weak_alias(syscall, _syscall)
#endif
```

**作用**：
- 创建弱符号 `syscall`，它是 `_syscall` 的别名
- 用户代码可以覆盖 `syscall` 实现
- 如果没有覆盖，使用 libc 提供的默认实现

**现代编译器支持**：
- GCC/Clang: `__attribute__((weak, alias("_syscall")))`
- MSVC: 使用 `#pragma comment(linker, "/alternatename:...")`

#### 3. 错误码转换

```c
if (msgptr->m_type < 0) {
    errno = -msgptr->m_type;    // 负数错误码 → 正数 errno
    return(-1);                  // 返回 -1 表示失败
}
return(msgptr->m_type);          // 非负数直接返回
```

**符合 POSIX 惯例**：
- 成功：返回非负值（文件描述符、字节数等）
- 失败：返回 -1，错误码存入 `errno`

### 调用流程

```
用户程序
    │
    │  syscall(VFS, VFS_OPEN, &msg)
    ▼
_syscall()  ← syscall.c（本文件）
    │
    │  1. msg.m_type = VFS_OPEN
    │  2. ipc_sendrec(VFS, &msg)
    ▼
_ipc_sendrec_intr()  ← _ipc.S
    │
    │  int $33
    ▼
内核处理...
    │
    ▼
返回用户态
    │
    │  3. 检查 msg.m_type
    ▼
返回结果给调用者
```

### 设计决策

#### 为什么需要包装函数？

| 层级 | 文件 | 职责 |
|------|------|------|
| 底层 | `_ipc.S` | 汇编入口，寄存器操作，中断触发 |
| 中层 | `syscall.c` | 错误处理，语义封装，POSIX 兼容 |
| 高层 | `open.c`, `read.c` | 具体系统调用，参数构造 |

**分层好处**：
- 职责分离，代码清晰
- 底层可以替换（如从 `int $33` 改为 `syscall`）
- 高层保持一致接口

#### 为什么 m_type 要复用？

**C 语言的角度**：
- 减少消息结构体大小（缓存友好）
- 减少内存拷贝（零拷贝优化）

**Rust 的角度**：
- 这是设计缺陷，应该用类型系统保证语义单一
- `PhantomData<State>` 可以在编译期区分状态，零运行时开销

### 与 _ipc.S 的关系

```
syscall.c          _ipc.S
    │                │
    │  调用          │
    ▼                ▼
ipc_sendrec()  →  _ipc_sendrec_intr()
                      │
                      │  int $33
                      ▼
                    内核态
```

**关键区别**：
- `syscall.c`：C 语言层，处理高层逻辑
- `_ipc.S`：汇编层，处理底层寄存器和中断

---

### 性能分析

#### IPC 操作开销分解

| 阶段 | 时钟周期 | 说明 |
|------|----------|------|
| 用户态准备 | 10-20 | 设置寄存器、栈帧 |
| `int $33` 切换 | 100-200 | 硬件保存状态、切换栈 |
| 内核处理 | 200-800 | 消息拷贝、进程调度 |
| 返回用户态 | 50-100 | `iret` 恢复状态 |
| **总计** | **450-1200** | 完整 IPC 操作 |

#### 优化方向

1. **使用 `syscall` 替代 `int $33`**：减少 50-100 周期
2. **批量操作 `senda`**：分摊切换开销
3. **共享内存**：避免消息拷贝
4. **异步 `notify`**：最小开销（~100 周期）

### Rust 实现对比

#### C 版本（Minix3）

```asm
ENTRY(_ipc_sendrec_intr)
    push    %ebp
    movl    %esp, %ebp
    push    %ebx
    movl    SRC_DST(%ebp), %eax
    movl    MESSAGE(%ebp), %ebx
    movl    $SENDREC, %ecx
    int     $IPCVEC_INTR
    pop     %ebx
    pop     %ebp
    ret
```

#### Rust 版本（改进）

```rust
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
enum IpcCall {
    Send = 1,
    Receive = 2,
    SendRec = 3,
    Notify = 4,
    SendNb = 5,
    SendA = 16,
}

#[inline(always)]
unsafe fn ipc_syscall(
    call: IpcCall,
    endpoint: Endpoint,
    msg: *mut Message,
) -> IpcResult<()> {
    let result: i32;
    
    asm!(
        "int $33",
        in("eax") endpoint.0,
        in("ebx") msg as usize,
        in("ecx") call as i32,
        lateout("eax") result,
    );
    
    if result == 0 {
        Ok(())
    } else {
        Err(IpcError::from(result))
    }
}
```

**改进点**：
- 类型安全：`Endpoint` 类型防止无效端点
- 错误处理：`Result` 强制处理错误
- 零成本抽象：内联后性能相同

### 要点总结

#### 核心知识点

1. **软件中断机制**：`int $33` 是用户态进入内核态的桥梁
2. **寄存器约定**：`%eax` = 端点，`%ebx` = 消息，`%ecx` = 调用类型
3. **callee-saved 寄存器**：`%ebx` 必须保存/恢复
4. **六种 IPC 类型**：send/receive/sendrec/notify/sendnb/senda

#### 设计意图

1. **简单优先**：`int $33` 兼容性好，代码简单
2. **约定明确**：固定寄存器分配，减少歧义
3. **语义分离**：不同 IPC 类型有不同入口
4. **批量优化**：`senda` 分摊切换开销

#### 与内核的协作

1. **mpx.S**：接收中断，保存上下文
2. **proc.c**：实现 IPC 逻辑（send/receive/notify）
3. **返回路径**：内核修改消息，用户态读取回复

---

# 五、总结与关系图

## 文件关系图

```
┌─────────────────────────────────────────────────────────────────┐
│                     MINIX3 IPC 文件关系                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  用户态                                                          │
│    │                                                            │
│    ├─► lib/libc/arch/i386/sys/_ipc.S ──► int $33 ──┐           │
│    │                                                │           │
│    └─► lib/libc/sys/syscall.c ──► _syscall() ──────┤           │
│                                                     ▼           │
│  内核态                                          mpx.S          │
│                                                     │           │
│                                                     ▼           │
│                                               do_ipc()          │
│                                                     │           │
│    ┌────────────────────────────────────────────────┘           │
│    │                                                           │
│    ├─► kernel/proc.c ──► mini_send/mini_receive/mini_notify    │
│    │                                                           │
│    ├─► kernel/proc.h ──► struct proc, RTS flags                │
│    │                                                           │
│    ├─► kernel/ipc_filter.h ──► 消息过滤                        │
│    │                                                           │
│    └─► kernel/system.c ──► 系统调用分发                        │
│                                                                 │
│  头文件                                                          │
│    │                                                            │
│    ├─► include/minix/ipc.h ──► message 结构体 (64字节)         │
│    │                                                            │
│    ├─► include/minix/ipcconst.h ──► SEND/RECEIVE/NOTIFY 常量   │
│    │                                                            │
│    ├─► include/minix/com.h ──► 消息类型编号                    │
│    │                                                            │
│    ├─► include/minix/type.h ──► 基本数据类型                   │
│    │                                                            │
│    ├─► include/minix/const.h ──► 系统常量                      │
│    │                                                            │
│    └─► sys/sys/ipc.h ──► System V IPC 权限                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## IPC 调用流程

```
┌─────────────────────────────────────────────────────────────────┐
│                     IPC 调用完整流程                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  用户程序                                                        │
│    │                                                            │
│    │  1. 调用 ipc_sendrec(dest, &msg)                          │
│    ▼                                                            │
│  _ipc_sendrec_intr()  [lib/libc/arch/i386/sys/_ipc.S]          │
│    │                                                            │
│    │  2. 设置寄存器                                            │
│    │     %eax = dest                                           │
│    │     %ebx = &msg                                           │
│    │     %ecx = SENDREC                                        │
│    ▼                                                            │
│  int $33  ─────────────────────────────────────────────┐       │
│                                                        │       │
│  3. CPU 自动切换：                                      │       │
│     - 保存用户态上下文 (SS, ESP, EFLAGS, CS, EIP)      │       │
│     - 切换到内核栈                                    │       │
│     - 跳转到 IDT[33] 处理程序                         │       │
│                                                        │       │
│  mpx.S  [kernel/arch/i386/mpx.S] ◄─────────────────────┘       │
│    │                                                            │
│    │  4. 保存寄存器，调用 do_ipc()                             │
│    ▼                                                            │
│  do_ipc()  [kernel/proc.c]                                     │
│    │                                                            │
│    │  5. 根据 %ecx 分发到 mini_send/mini_receive               │
│    ▼                                                            │
│  mini_send()  [kernel/proc.c]                                  │
│    │                                                            │
│    │  6. 检查接收者状态                                        │
│    │     - 如果接收者在等待：直接传递消息                      │
│    │     - 否则：加入发送者队列，阻塞当前进程                  │
│    ▼                                                            │
│  调度器                                                          │
│    │                                                            │
│    │  7. 选择下一个运行的进程                                  │
│    ▼                                                            │
│  pick_proc()  [kernel/proc.c]                                  │
│    │                                                            │
│    │  8. 从就绪队列选择最高优先级进程                          │
│    ▼                                                            │
│  恢复上下文                                                      │
│    │                                                            │
│    │  9. iret 返回用户态                                       │
│    ▼                                                            │
│  用户程序（继续执行）                                            │
│    │                                                            │
│    │  10. 检查返回值，读取回复消息                             │
│    ▼                                                            │
│  完成！                                                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 要点速查表

| 概念 | 文件 | 关键内容 |
|------|------|----------|
| **消息大小** | ipc.h | 固定 64 字节 |
| **系统调用号** | ipcconst.h | SEND=1, RECEIVE=2, SENDREC=3, NOTIFY=4 |
| **进程状态** | proc.h | RTS flags，0=可运行 |
| **用户态入口** | _ipc.S | int $33，%eax/%ebx/%ecx 传参 |
| **权限检查** | ipc.h | uid/gid/mode |
| **消息过滤** | ipc_filter.h | 黑名单/白名单 |
| **消息类型** | com.h | 0x000-0x7FF 按子系统分配 |
| **系统调用包装** | syscall.c | m_type 双重语义，弱别名，错误码转换 |

---

**文档版本**: 2026-03-30
**涵盖文件**: IPC 核心头文件、用户态 IPC 实现
**相关文档**: 
- [进程管理详细文档](../process/process-detail.md)
- [系统调用详细文档](../syscall/syscall-detail.md)
- [中断与时钟详细文档](../interrupt/interrupt-detail.md)
