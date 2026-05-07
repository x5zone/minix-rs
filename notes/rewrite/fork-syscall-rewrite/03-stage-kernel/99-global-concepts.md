# 99-global-concepts: 系统全局概念

> **分类**: Global 层级 ⚠️  
> **说明**: 系统全局概念汇总，后续将迁移到系统级文档  
> **⚠️ 注意**: 本文档内容属于系统全局，不应局限于 Kernel 视角

---

## 1. 进程标识系统

### 1.1 endpoint_t - 进程端点

#### 1.1.1 定义位置

**文件**: `minix3/minix/include/minix/endpoint.h`

```c
typedef int endpoint_t;  // 端点是一个整数
```

#### 1.1.2 结构

endpoint 采用 **slot + generation** 的编码方式：

```c
#define _ENDPOINT_GENERATION_SHIFT  15

#define _ENDPOINT(g, p) \
    ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)
#define _ENDPOINT_P(e) \
    ((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

**编码原理**:
- 高位：代数（generation），防止 slot 重用时的混淆
- 低位：进程槽位号（slot），标识进程在进程表中的位置

**示例**:
```
endpoint = _ENDPOINT(3, 5)  // generation=3, slot=5
```

#### 1.1.3 特殊端点

```c
#define ANY     ((endpoint_t) (_ENDPOINT_SLOT_TOP - 1))  // 任意进程
#define NONE    ((endpoint_t) (_ENDPOINT_SLOT_TOP - 2))  // 无进程
#define SELF    ((endpoint_t) (_ENDPOINT_SLOT_TOP - 3))  // 自己
```

**使用场景**:
- `ANY`: `receive(ANY, &msg)` 接收任意进程的消息
- `NONE`: 初始化时表示无效端点
- `SELF`: 向自己发送消息（特殊用途）

### 1.2 Endpoint 编解码

#### 1.2.1 _ENDPOINT_P - 提取 slot

`_ENDPOINT_P` 宏从 endpoint 中提取进程槽位号。

**宏定义**：

```c
#define _ENDPOINT_P(e) \
    ((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

**提取步骤**：

```
endpoint 值
    │
    ▼
1. (e) + MAX_NR_TASKS     // 偏移，使负数 slot 变为非负数
    │
    ▼
2. & 0x7FFF               // 掩码，提取低 15 位（slot 部分）
    │
    ▼
3. - MAX_NR_TASKS         // 减去偏移，恢复原始 slot 值
    │
    ▼
slot 号（可能为负，表示 kernel task）
```

**示例**：

```c
// 假设 MAX_NR_TASKS = 16, _ENDPOINT_GENERATION_SHIFT = 15

endpoint = _ENDPOINT(3, 5)   // generation=3, slot=5
// endpoint = (3 << 15) + 5 = 98309

slot = _ENDPOINT_P(98309)
// = ((98309 + 16) & 0x7FFF) - 16
// = (98325 & 0x7FFF) - 16
// = 21 - 16
// = 5  ✓

// 负数 slot（kernel task）
endpoint = _ENDPOINT(0, -5)  // generation=0, slot=-5
// endpoint = -5

slot = _ENDPOINT_P(-5)
// = ((-5 + 16) & 0x7FFF) - 16
// = (11 & 0x7FFF) - 16
// = 11 - 16
// = -5  ✓
```

#### 1.2.2 _ENDPOINT_G - 提取 generation

`_ENDPOINT_G` 宏从 endpoint 中提取代数（generation）。

**宏定义**：

```c
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)
```

**提取步骤**：

```
endpoint 值
    │
    ▼
1. (e) + MAX_NR_TASKS     // 偏移，使负数 slot 变为非负数
    │
    ▼
2. >> 15                  // 右移 15 位，提取高 17 位（generation 部分）
    │
    ▼
generation 号
```

**示例**：

```c
// 假设 MAX_NR_TASKS = 16, _ENDPOINT_GENERATION_SHIFT = 15

endpoint = _ENDPOINT(3, 5)   // generation=3, slot=5
// endpoint = (3 << 15) + 5 = 98309

gen = _ENDPOINT_G(98309)
// = (98309 + 16) >> 15
// = 98325 >> 15
// = 3  ✓

// 负数 slot（kernel task）
endpoint = _ENDPOINT(2, -5)  // generation=2, slot=-5
// endpoint = (2 << 15) + (-5) = 65531

gen = _ENDPOINT_G(65531)
// = (65531 + 16) >> 15
// = 65547 >> 15
// = 2  ✓
```

#### 1.2.3 _ENDPOINT - 构造 endpoint

`_ENDPOINT` 宏将 generation 和 slot 组合成 endpoint。

**宏定义**：

```c
#define _ENDPOINT(g, p) \
    ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

**构造步骤**：

```
generation (g) + slot (p)
    │
    ▼
1. (g) << 15              // 将 generation 左移 15 位到高位
    │
    ▼
2. + (p)                  // 加上 slot 值
    │
    ▼
endpoint 值
```

**示例**：

```c
// 假设 _ENDPOINT_GENERATION_SHIFT = 15

// 用户进程
endpoint = _ENDPOINT(3, 5)   // generation=3, slot=5
// = (3 << 15) + 5
// = 98304 + 5
// = 98309

// Kernel task（slot 为负数）
endpoint = _ENDPOINT(0, -5)  // generation=0, slot=-5
// = (0 << 15) + (-5)
// = -5

// 验证：generation=0 时，endpoint = slot
endpoint = _ENDPOINT(0, 10)  // generation=0, slot=10
// = 10  // 直接等于 slot 号
```

**设计优势**：当 generation=0 时，endpoint 直接等于 slot 号，便于调试和硬编码。

### 1.3 使用场景

#### 1.3.1 IPC 路由

endpoint 是 MINIX IPC 的核心标识符，用于消息路由。

**IPC 流程**：

```
发送进程                          内核                          接收进程
    │                              │                              │
    │ send(endpoint, &msg)         │                              │
    │ ───────────────────────────► │                              │
    │                              │ 1. 从 endpoint 提取 slot     │
    │                              │ 2. 验证 endpoint 有效性      │
    │                              │ 3. 查找目标进程              │
    │                              │ 4. 复制消息到目标缓冲区      │
    │                              │ ────────────────────────────►│
    │                              │                              │ 接收消息
```

**关键函数**：

```c
// 从 endpoint 查找进程
struct proc *endpoint_proc(endpoint_t ep) {
    int slot = _ENDPOINT_P(ep);
    if (slot < -NR_TASKS || slot >= NR_PROCS)
        return NULL;
    return &proc[slot + NR_TASKS];
}

// IPC 发送
int ipc_send(endpoint_t dest, message *msg) {
    struct proc *target = endpoint_proc(dest);
    if (!target) return ESRCH;
    // 复制消息...
}
```

**特殊端点处理**：

| 端点 | 处理方式 |
|------|----------|
| `ANY` | 接收任意进程的消息 |
| `SELF` | 向自己发送（特殊用途） |
| `NONE` | 表示无效或未设置 |

#### 1.3.2 进程查找

通过 endpoint 查找进程需要两步：验证有效性 + 获取进程指针。

**查找流程**：

```c
// 1. 验证 endpoint 并提取进程号
int proc_nr;
if (!isokendpt(endpoint, &proc_nr)) {
    return EINVAL;  // 无效 endpoint
}

// 2. 获取进程结构指针
struct proc *p = proc_addr(proc_nr);
```

**isokendpt 函数**：

```c
int isokendpt(endpoint_t ep, int *proc_nr) {
    *proc_nr = _ENDPOINT_P(ep);  // 提取 slot
    
    // 检查范围
    if (*proc_nr < -NR_TASKS || *proc_nr >= NR_PROCS)
        return 0;  // 超出范围
    
    // 检查 generation 匹配
    struct proc *p = proc_addr(*proc_nr);
    if (p->p_endpoint != ep)
        return 0;  // generation 不匹配（进程已退出重用）
    
    return 1;  // 有效
}
```

**proc_addr 宏**：

```c
#define proc_addr(p) (&proc[(p) + NR_TASKS])
```

**为什么需要 +NR_TASKS**：
- 进程表数组从 0 开始
- slot 为负数表示 kernel task（如 -5）
- 加上 NR_TASKS 后变成非负索引（如 -5 + 16 = 11）

---

## 2. 进程号系统

### 2.1 proc_nr_t - 进程号

```c
typedef int proc_nr_t;  // 进程表条目号
```

**范围**:
- 负数：内核任务 (kernel tasks)
- 非负数：用户进程

### 2.2 进程号与端点的关系

进程号（`proc_nr_t`）和端点（`endpoint_t`）都用于标识进程，但有重要区别。

**核心区别**：

| 特性 | 进程号 (`proc_nr_t`) | 端点 (`endpoint_t`) |
|------|---------------------|---------------------|
| **作用域** | 内核内部 | 全局（跨进程通信） |
| **唯一性** | 槽位唯一 | 全局唯一（含 generation） |
| **稳定性** | 槽位重用时复用 | generation 递增，永不重复 |
| **用途** | 进程表索引 | IPC 标识符 |

**关系公式**：

```
endpoint = (generation << 15) + proc_nr
proc_nr = endpoint & 0x7FFF  // 当 generation = 0 时
```

**示例**：

```c
// 进程 A 占用 slot 5
进程 A: proc_nr = 5, endpoint = 5 (generation=0)

// 进程 A 退出，进程 B 重用 slot 5
进程 B: proc_nr = 5, endpoint = 32773 (generation=1)
//       endpoint = (1 << 15) + 5 = 32773

// 此时旧的 endpoint=5 已失效
// isokendpt(5, ...) 会失败（generation 不匹配）
```

**设计目的**：generation 确保即使槽位被重用，旧的 endpoint 也不会错误地指向新进程，避免 IPC 混淆。

---

## 3. 位图操作

### 3.1 sys_map_t - 系统位图

```c
typedef struct {
  bitchunk_t chunk[BITMAP_CHUNKS(NR_SYS_PROCS)];
} sys_map_t;
```

### 3.2 位操作宏

```c
#define get_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit) ))
#define set_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit) ))
#define unset_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) &= ~(1 << CHUNK_OFFSET(bit) ))
```

---

## 4. 消息系统

### 4.1 message 结构

message 是 MINIX IPC 的核心数据结构，用于进程间通信。

**结构定义**：

```c
typedef struct noxfer_message {
    endpoint_t m_source;  // 发送者端点
    int m_type;           // 消息类型
    union {
        mess_u8   m_u8;   // 字节数组
        mess_u16  m_u16;  // 短整型数组
        mess_u32  m_u32;  // 整型数组
        mess_u64  m_u64;  // 长整型数组
        
        mess_1    m_m1;   // 通用类型 1
        mess_2    m_m2;   // 通用类型 2
        mess_3    m_m3;   // 通用类型 3
        mess_4    m_m4;   // 通用类型 4
        mess_7    m_m7;   // 通用类型 7
        mess_9    m_m9;   // 通用类型 9
        mess_10   m_m10;  // 通用类型 10
        
        // 特定子系统消息类型...
        mess_ds_req    m_ds_req;     // Data Store 请求
        mess_fs_vfs_*  m_fs_vfs_*;   // 文件系统消息
        // ...
    } m_u;
} message;
```

**设计要点**：

| 特性 | 说明 |
|------|------|
| **固定大小** | 64 字节，适合寄存器传递 |
| **联合体** | 多种消息布局共享内存 |
| **m_source** | 自动填充发送者端点 |
| **m_type** | 标识消息类型（如 IPC 请求/响应） |

**通用消息类型示例**：

```c
// mess_1: 整数 + 指针组合
typedef struct {
    uint64_t m1ull1;
    int m1i1, m1i2, m1i3;
    char *m1p1, *m1p2, *m1p3, *m1p4;
} mess_1;
```

### 4.2 消息传递

MINIX 使用同步消息传递机制，支持三种基本操作。

**基本原语**：

| 原语 | 阻塞性 | 说明 |
|------|--------|------|
| `send(dest, &msg)` | 阻塞 | 发送消息，等待接收方接收 |
| `receive(src, &msg)` | 阻塞 | 接收消息，等待发送方发送 |
| `sendrec(dest, &msg)` | 阻塞 | 发送并等待回复 |

**非阻塞变体**：

| 原语 | 说明 |
|------|------|
| `nb_send(dest, &msg)` | 非阻塞发送，失败立即返回 |
| `nb_receive(src, &msg)` | 非阻塞接收，无消息立即返回 |

**消息传递流程**：

```
发送进程                          内核                          接收进程
    │                              │                              │
    │ send(dest, &msg)             │                              │
    │ ───────────────────────────► │                              │
    │                              │ 1. 检查目标是否在等待         │
    │                              │ 2. 复制消息到目标缓冲区       │
    │                              │ 3. 唤醒目标进程               │
    │                              │ ────────────────────────────►│
    │                              │                              │ receive() 返回
    │ 阻塞中...                    │                              │
    │ ◄─────────────────────────── │                              │
    │ send() 返回                  │                              │
```

**关键特性**：
- **同步**：发送方阻塞直到接收方接收
- **复制**：消息内容从发送方复制到接收方
- **原子**：消息传递是原子操作

---

## 5. 待迁移内容

> 本节记录后续需要迁移到系统级文档的内容。

### 5.1 IPC 机制

> **待迁移**：详细的 IPC 机制（如 notify、interrupt 消息、endpoint 管理）应迁移到独立的 `ipc.md` 文档中。本节仅保留与 fork 直接相关的 endpoint 和 message 基础概念。

### 5.2 信号机制

> **待迁移**：信号机制（如信号发送、信号处理、信号掩码）应迁移到独立的 `signal.md` 文档中。fork 中子进程继承父进程信号设置的行为由 PM 模块处理。

### 5.3 调度机制

> **待迁移**：调度机制（如优先级、时间片、调度队列）应迁移到独立的 `scheduler.md` 文档中。fork 中子进程继承父进程优先级的行为由内核调度器处理。

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) - Kernel 整体架构概览
- [系统核心概念](../../concepts/README.md) - 全局概念文档
- [Endpoint 协议](../../concepts/endpoint.md) - 进程标识协议
