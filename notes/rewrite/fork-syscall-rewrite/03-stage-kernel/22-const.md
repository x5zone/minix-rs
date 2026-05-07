# 22-const - 常量定义

> 本文档分析 `minix3/minix/kernel/const.h`，讲解常量定义。

---

## 1. 概述

`const.h` 定义了内核使用的核心常量和宏，涵盖端点验证、位操作、内存布局等关键功能。

**核心作用**：
- **端点验证**：`isokendpt`、`okendpt` 宏验证进程端点有效性
- **位操作**：`get_sys_bit`、`set_sys_bit` 等宏操作位图
- **内存布局**：`USR_DATATOP`、`USR_STACKTOP` 定义用户空间边界

**设计原则**：
- **编译期计算**：使用宏实现零运行时开销
- **类型安全**：通过 `_Static_assert` 验证常量值
- **可配置性**：关键常量可通过编译选项调整

### 1.1 内核常量

内核常量定义了系统运行的基础参数和边界值。

**常量分类**：

| 类别 | 示例 | 作用 |
|------|------|------|
| **进程限制** | `NR_PROCS`、`NR_TASKS` | 定义进程表大小 |
| **内存布局** | `USR_DATATOP`、`USR_STACKTOP` | 定义用户空间边界 |
| **端点特殊值** | `ANY`、`NONE`、`SELF` | IPC 特殊标识 |
| **错误码** | `OK`、`EBADREQUEST` | 系统调用返回值 |
| **标志位** | `RTS_NO_QUANTUM`、`RTS_NO_MIN` | 进程调度状态 |

### 1.2 与 fork 的关系

fork 系统调用使用以下常量：

| 常量 | 用途 |
|------|------|
| `isokendpt` | 验证父进程端点有效性 |
| `okendpt` | 验证并获取父进程槽位号 |
| `PRIO_PROC` | 设置子进程初始优先级 |
| `RTS_NO_QUANTUM` | 子进程初始调度状态（无时间片） |

---

## 2. C 源码分析

本节分析 `minix3/minix/kernel/const.h` 中的常量定义，包括：

- **端点验证宏**：`isokendpt`、`okendpt`
- **虚拟复制常量**：`_SRC_`、`_DST_`
- **系统位操作宏**：`get_sys_bit`、`set_sys_bit`、`unset_sys_bit`
- **消息常量**：`END_OF_KMESS`
- **用户空间限制**：`USR_DATATOP`、`USR_STACKTOP`

### 2.1 端点验证宏

端点验证宏用于将 endpoint 转换为进程号并验证其有效性。

**宏定义**：

```c
/* Translate an endpoint number to a process number, return success. */
#ifndef isokendpt
#define isokendpt(e,p) isokendpt_d((e),(p),0)
#define okendpt(e,p)   isokendpt_d((e),(p),1)
#endif
```

**两个宏的关系**：

| 宏 | 调用参数 | 含义 |
|---|---------|------|
| `isokendpt(e, p)` | `isokendpt_d(e, p, 0)` | 宽松模式，允许无效端点 |
| `okendpt(e, p)` | `isokendpt_d(e, p, 1)` | 严格模式，无效时触发 panic |

#### 2.1.1 isokendpt 宏

`isokendpt` 是宽松模式的端点验证宏，允许无效端点。

**定义链**：

```c
#define isokendpt(e,p) isokendpt_d((e),(p),0)
#define isokendpt_d(e, p, f) isokendpt_f((e), (p), (f))
```

**核心实现** (`proc.c`)：

```c
int isokendpt_f(endpoint_t e, int *p, int fatalflag) {
    *p = _ENDPOINT_P(e);  // 提取 slot
    ok = 0;
    if(isokprocn(*p) && !isemptyn(*p) && proc_addr(*p)->p_endpoint == e)
        ok = 1;
    if(!ok && fatalflag)
        panic("invalid endpoint: %d", e);
    return ok;
}
```

**返回值**：
- `1`：端点有效
- `0`：端点无效（不 panic）

##### 2.1.1.1 宏展开

以 `isokendpt(endpoint, &proc_nr)` 为例：

**展开过程**：

```
isokendpt(endpoint, &proc_nr)
    │
    ▼
isokendpt_d(endpoint, &proc_nr, 0)
    │
    ▼
isokendpt_f(endpoint, &proc_nr, 0)
```

**参数传递**：

| 参数 | 值 | 含义 |
|------|---|------|
| `e` | `endpoint` | 要验证的端点值 |
| `p` | `&proc_nr` | 输出参数，存储提取的进程号 |
| `fatalflag` | `0` | 宽松模式，无效时不 panic |

##### 2.1.1.2 端点有效性检查

`isokendpt_f` 函数通过三重检查验证端点有效性：

**检查流程**：

```c
*p = _ENDPOINT_P(e);  // 步骤1: 提取 slot 号
ok = 0;
if(isokprocn(*p) && !isemptyn(*p) && proc_addr(*p)->p_endpoint == e)
    ok = 1;           // 三重检查全部通过
```

**三重检查详解**：

| 检查 | 宏/函数 | 含义 | 实现原理 |
|------|---------|------|----------|
| **范围检查** | `isokprocn(n)` | slot 号在有效范围内 | `(unsigned)(n + NR_TASKS) < NR_PROCS + NR_TASKS` |
| **占用检查** | `isemptyn(n)` | slot 已被进程占用 | `proc_addr(n)->p_rts_flags != RTS_SLOT_FREE` |
| **匹配检查** | `p_endpoint == e` | 端点值完全匹配 | 比较 endpoint 的 generation 部分 |

**为什么需要三重检查**：

1. **范围检查**：防止数组越界访问 `proc_addr(n)` 宏会计算 `&proc[NR_TASKS + n]`，若 `n` 超出范围会导致越界

2. **占用检查**：排除空槽位进程退出后，其 slot 被标记为 `RTS_SLOT_FREE`，但 slot 号本身仍有效

3. **匹配检查**：验证 generation 端点包含 generation 号，用于检测"进程已退出，新进程复用同一 slot"的情况

**generation 机制示意**：

```
endpoint = (generation << SHIFT) + slot

进程 A (slot=5, generation=0): endpoint = 5
进程 A 退出
进程 B (slot=5, generation=1): endpoint = 5 + GENERATION_SIZE

旧端点 5 发来消息 → isokendpt(5, &p) 失败
  因为 proc_addr(5)->p_endpoint = 5 + GENERATION_SIZE ≠ 5
```

**辅助宏定义** (`proc.h`)：

```c
#define proc_addr(n)    (&(proc[NR_TASKS + (n)]))
#define isokprocn(n)    ((unsigned) ((n) + NR_TASKS) < NR_PROCS + NR_TASKS)
#define isemptyn(n)     isemptyp(proc_addr(n))
#define isemptyp(p)     ((p)->p_rts_flags == RTS_SLOT_FREE)
```

**端点提取宏** (`endpoint.h`)：

```c
#define _ENDPOINT_P(e) \
    ((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

##### 2.1.1.3 fork 时的使用

在 `do_fork` 系统调用处理中，`isokendpt` 用于验证父进程端点：

**使用场景** (`do_fork.c`)：

```c
int do_fork(struct proc * caller, message * m_ptr)
{
    int p_proc;  // 父进程的 slot 号
    
    // 验证父进程端点有效性
    if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
        return EINVAL;
    
    rpp = proc_addr(p_proc);  // 获取父进程指针
    // ...
}
```

**验证流程**：

```
PM 调用 SYS_FORK
       │
       ▼
消息参数: m_lsys_krn_sys_fork.endpt = 父进程端点
       │
       ▼
isokendpt(endpt, &p_proc)
       │
       ├── 成功: p_proc = 父进程 slot 号
       │         继续执行 fork
       │
       └── 失败: 返回 EINVAL
                 端点无效（进程已退出？）
```

**为什么需要验证**：

1. **安全边界**：PM 传入的端点可能被篡改或已失效
2. **防御性编程**：内核不信任用户空间传入的参数
3. **进程生命周期**：端点可能指向已退出的进程

**验证后使用**：

```c
rpp = proc_addr(p_proc);           // 获取父进程指针
rpc = proc_addr(child_slot);       // 获取子进程指针
if (isemptyp(rpp) || !isemptyp(rpc)) 
    return EINVAL;                 // 二次验证：父进程存在，子进程为空
```

#### 2.1.2 okendpt 宏

`okendpt` 是严格模式的端点验证宏，无效端点会触发 panic。

**定义** (`const.h`)：

```c
#define okendpt(e,p)   isokendpt_d((e),(p),1)
```

**调用链**：

```
okendpt(endpoint, &proc_nr)
       │
       ▼
isokendpt_d(endpoint, &proc_nr, 1)
       │
       ▼
isokendpt_f(endpoint, &proc_nr, 1)  // fatalflag = 1
       │
       ├── 成功: 返回 1
       │
       └── 失败: panic("invalid endpoint: %d", endpoint)
```

**与 isokendpt 的唯一区别**：`fatalflag = 1`

##### 2.1.2.1 与 isokendpt 的区别

两个宏的核心区别在于对无效端点的处理方式：

| 特性 | `isokendpt` | `okendpt` |
|------|-------------|-----------|
| **fatalflag** | `0` | `1` |
| **无效端点时** | 返回 `0`，继续执行 | 调用 `panic()`，系统停止 |
| **使用场景** | 可恢复错误，返回错误码 | 不可恢复错误，断言失败 |
| **典型调用者** | 系统调用处理 | 内核内部关键路径 |

**选择原则**：

- **isokendpt**：用户空间传入的参数，可能无效是正常情况
- **okendpt**：内核内部逻辑，端点无效表示严重 bug

**代码对比**：

```c
// isokendpt: 可恢复
if(!isokendpt(endpoint, &proc_nr))
    return EINVAL;  // 返回错误，让调用者处理

// okendpt: 不可恢复
if(!okendpt(endpoint, &proc_nr))
    // 永远不会执行到这里
    // 因为 okendpt 失败时会 panic
```

##### 2.1.2.2 严格模式

严格模式通过 `fatalflag = 1` 触发，无效端点会导致系统 panic。

**panic 触发逻辑** (`proc.c`)：

```c
int isokendpt_f(endpoint_t e, int *p, int fatalflag) {
    *p = _ENDPOINT_P(e);
    ok = 0;
    if(isokprocn(*p) && !isemptyn(*p) && proc_addr(*p)->p_endpoint == e)
        ok = 1;
    if(!ok && fatalflag)
        panic("invalid endpoint: %d", e);  // 严格模式：系统崩溃
    return ok;
}
```

**严格模式的设计意图**：

1. **断言语义**：端点无效 = 内核 bug，不应继续执行
2. **快速失败**：问题立即暴露，避免后续难以调试的错误
3. **开发辅助**：panic 信息包含无效端点值，便于定位问题

**典型使用场景**：

```c
// 内核内部获取进程指针，端点必须有效
if(!okendpt(endpoint, &proc_nr))
    ;  // 永远不会执行
rp = proc_addr(proc_nr);  // 安全使用
```

**何时使用严格模式**：

| 场景 | 推荐宏 |
|------|--------|
| 系统调用入口，验证用户参数 | `isokendpt` |
| IPC 消息中的端点 | `isokendpt` |
| 内核内部已知有效的端点 | `okendpt` |
| 进程表遍历中的端点 | `okendpt` |

### 2.2 虚拟复制常量

虚拟复制常量用于 `sys_copy` 系统调用，区分源和目标地址。

**定义** (`const.h`)：

```c
#define _SRC_   0
#define _DST_   1
```

**设计意图**：使用数组索引而非布尔标志，简化代码逻辑。

#### 2.2.1 _SRC_ 常量

`_SRC_` (值为 0) 表示源地址，用于数组索引。

**使用场景** (`do_copy.c`)：

```c
vir_addr[_SRC_].proc_nr_e = m_ptr->m_lsys_krn_sys_copy.src_endpt;
vir_addr[_SRC_].offset = m_ptr->m_lsys_krn_sys_copy.src_addr;
```

**数组索引模式**：

```c
struct vir_addr {
    endpoint_t proc_nr_e;  // 进程端点
    vir_bytes offset;      // 虚拟地址偏移
} vir_addr[2];             // [0]=源, [1]=目标
```

**优势**：源和目标使用相同的数据结构和处理逻辑，减少代码重复。

#### 2.2.2 _DST_ 常量

`_DST_` (值为 1) 表示目标地址，用于数组索引。

**使用场景** (`do_copy.c`)：

```c
vir_addr[_DST_].proc_nr_e = m_ptr->m_lsys_krn_sys_copy.dst_endpt;
vir_addr[_DST_].offset = m_ptr->m_lsys_krn_sys_copy.dst_addr;
```

**验证循环示例**：

```c
/* Now do some checks for both the source and destination virtual address.
 * This is done once for _SRC_, then once for _DST_.
 */
for (i = _SRC_; i <= _DST_; i++) {
    // 统一处理源和目标
    if (vir_addr[i].proc_nr_e == SELF)
        vir_addr[i].proc_nr_e = caller_ptr->p_endpoint;
}
```

**设计模式**：通过循环统一处理源和目标，避免重复代码。

### 2.3 系统位操作宏

系统位操作宏用于操作进程权限位图、信号位图等内核位图结构。

**核心宏定义** (`const.h`)：

```c
#define get_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit) ))
#define get_sys_bits(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) )
#define set_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit) ))
#define unset_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) &= ~(1 << CHUNK_OFFSET(bit) ))
```

**辅助宏** (`bitmap.h`)：

```c
#define BITCHUNK_BITS       (sizeof(bitchunk_t) * 8)  // 通常为 32
#define MAP_CHUNK(map,bit)  (map)[((bit)/BITCHUNK_BITS)]
#define CHUNK_OFFSET(bit)   ((bit)%BITCHUNK_BITS)
```

**位图结构**：

```c
typedef struct {
    bitchunk_t chunk[BITMAP_CHUNKS(NR_PROCS)];  // 位数组
} sys_map_t;
```

#### 2.3.1 get_sys_bit 宏

`get_sys_bit` 测试位图中某一位是否被设置。

**定义**：

```c
#define get_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit) ))
```

**计算过程**：

```
位号 bit = 35 (假设)
BITCHUNK_BITS = 32

MAP_CHUNK(map, 35)  →  map.chunk[35/32] = map.chunk[1]
CHUNK_OFFSET(35)    →  35 % 32 = 3
1 << 3              →  0b1000

结果: map.chunk[1] & 0b1000  →  非零表示位35已设置
```

##### 2.3.1.1 位测试

位测试用于检查位图中某一位的状态，常用于权限检查和信号检测。

**使用示例**：

```c
// 检查进程是否有发送消息的权限
if (get_sys_bit(priv(caller_ptr)->s_ipc_to, proc_nr(dst_ptr))) {
    // 允许发送
} else {
    // 权限不足
    return EPERM;
}
```

**返回值**：

| 表达式结果 | 含义 |
|-----------|------|
| 非零 | 位已设置（有权限/有信号） |
| 零 | 位未设置（无权限/无信号） |

**典型场景**：

- **IPC 权限检查**：`s_ipc_to` 位图控制可发送消息的目标
- **信号检测**：`s_sig_pending` 位图记录待处理信号
- **异步消息**：`s_asyn_pending` 位图记录待发送的异步通知

##### 2.3.1.2 MAP_CHUNK 宏

`MAP_CHUNK` 宏定位包含目标位的 chunk（字）。

**定义**：

```c
#define MAP_CHUNK(map,bit)  (map)[((bit)/BITCHUNK_BITS)]
```

**作用**：将位号转换为数组索引，定位到包含该位的 `bitchunk_t` 元素。

**内存布局**：

```
位号范围        chunk 索引
[0, 31]    →   chunk[0]
[32, 63]   →   chunk[1]
[64, 95]   →   chunk[2]
...

例如：bit=50
  50 / 32 = 1  →  chunk[1]
  chunk[1] 包含位 32-63
```

**与 CHUNK_OFFSET 配合**：

```c
// 完整的位定位
chunk = MAP_CHUNK(map, bit);      // 哪个字
offset = CHUNK_OFFSET(bit);       // 字内哪一位
mask = 1 << offset;               // 位掩码
```

#### 2.3.2 get_sys_bits 宏

`get_sys_bits` 返回包含目标位的整个 chunk，用于批量检查。

**定义**：

```c
#define get_sys_bits(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) )
```

**与 get_sys_bit 的区别**：

| 宏 | 返回值 | 用途 |
|---|--------|------|
| `get_sys_bit(map, bit)` | 单个位的状态（0或非0） | 检查特定位 |
| `get_sys_bits(map, bit)` | 整个 chunk（32位） | 批量检查多个位 |

**使用场景**：

```c
// 检查是否有任何待处理信号
bitchunk_t pending = get_sys_bits(priv(p)->s_sig_pending, 0);
if (pending != 0) {
    // 有信号待处理
}
```

#### 2.3.3 set_sys_bit 宏

`set_sys_bit` 设置位图中的某一位为 1。

**定义**：

```c
#define set_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit) ))
```

**操作分解**：

```c
// 展开后的等价操作
MAP_CHUNK((map).chunk, bit)          // 定位 chunk
|= (1 << CHUNK_OFFSET(bit))          // 设置对应位
```

##### 2.3.3.1 位设置

位设置用于标记权限、信号或状态。

**使用示例**：

```c
// 设置进程的 IPC 发送权限
set_sys_bit(priv(p)->s_ipc_to, target_proc_nr);

// 标记待处理信号
set_sys_bit(priv(p)->s_sig_pending, sig_nr);

// 标记异步通知待发送
set_sys_bit(priv(p)->s_asyn_pending, src_proc_nr);
```

**原子性注意**：这些宏本身不保证原子性，多核环境需要额外同步。

##### 2.3.3.2 CHUNK_OFFSET 宏

`CHUNK_OFFSET` 计算位在 chunk 内的偏移量。

**定义**：

```c
#define CHUNK_OFFSET(bit)   ((bit)%BITCHUNK_BITS)
```

**作用**：计算位在 `bitchunk_t`（32位字）内的位置，用于生成位掩码。

**计算示例**：

```
bit = 35
BITCHUNK_BITS = 32

CHUNK_OFFSET(35) = 35 % 32 = 3

位 35 在 chunk[1] 的第 3 位（从 0 开始计数）
```

**位掩码生成**：

```c
// bit = 35 时
mask = 1 << CHUNK_OFFSET(35)  =  1 << 3  =  0b00001000
```

#### 2.3.4 unset_sys_bit 宏

`unset_sys_bit` 清除位图中的某一位（设为 0）。

**定义**：

```c
#define unset_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) &= ~(1 << CHUNK_OFFSET(bit)) )
```

**操作分解**：

```c
// 展开后的等价操作
MAP_CHUNK((map).chunk, bit)           // 定位 chunk
&= ~(1 << CHUNK_OFFSET(bit))          // 清除对应位
```

**与 set_sys_bit 对比**：

| 操作 | 运算符 | 掩码 |
|------|--------|------|
| 设置位 | `\|=` | `1 << offset` |
| 清除位 | `&= ~` | `~(1 << offset)` |

##### 2.3.4.1 位清除

位清除用于取消权限、清除信号或重置状态。

**使用示例**：

```c
// 清除进程的 IPC 发送权限
unset_sys_bit(priv(p)->s_ipc_to, target_proc_nr);

// 清除已处理的信号
unset_sys_bit(priv(p)->s_sig_pending, sig_nr);

// 清除已发送的异步通知
unset_sys_bit(priv(p)->s_asyn_pending, src_proc_nr);
```

**在 fork 中的应用**：

```c
// 子进程继承父进程的权限位图
// 但需要清除一些不需要的状态
rpc->s_ipc_to = rpp->s_ipc_to;    // 继承 IPC 权限
// 清除异步消息状态（子进程不继承）
```

### 2.4 消息常量

消息常量用于内核日志输出机制。

**定义** (`const.h`)：

```c
/* for kputc() */
#define END_OF_KMESS    0
```

**设计意图**：使用特殊字符标记内核消息结束，触发输出驱动刷新。

#### 2.4.1 END_OF_KMESS

`END_OF_KMESS` (值为 0) 是内核消息的结束标记。

**为什么是 0**：

- ASCII 码 0 是空字符 (NUL)，不会出现在正常文本中
- 作为特殊标记，不会与实际输出内容冲突
- 便于检测：`if (c != END_OF_KMESS)` 表示正常字符

**内核消息缓冲区**：

```c
struct kmessages {
    int kmess_size;              // 当前消息长度
    char kmess_buf[KMESS_BUF_SIZE];  // 消息缓冲区
};
```

#### 2.4.2 kputc 使用

`kputc` 函数使用 `END_OF_KMESS` 标记消息结束并通知输出驱动。

**实现** (`utility.c`)：

```c
void kputc(int c)
{
/* Accumulate a single character for a kernel message. Send a notification
 * to the output drivers if an END_OF_KMESS is encountered.
 */
  if (c != END_OF_KMESS) {
      // 正常字符：追加到缓冲区
      int maxblpos = sizeof(kmess.kmess_buf) - 2;
      if (kmess.kmess_size < maxblpos) {
          kmess.kmess_buf[kmess.kmess_size++] = c;
      }
  } else {
      // 结束标记：通知输出驱动
      kmess.kmess_buf[kmess.kmess_size] = 0;  // NUL 终止
      if (kmess.kmess_size > 0) {
          mini_notify(proc_addr(SYSTEM), LOG_PROC_NR);
      }
  }
}
```

**工作流程**：

```
内核调用 printf/kprintf
       │
       ▼
逐字符调用 kputc(c)
       │
       ├── c != 0: 追加到 kmess_buf
       │
       └── c == 0: 发送通知给 LOG 进程
                   LOG 进程读取并输出消息
```

**使用示例** (`do_diagctl.c`)：

```c
// 用户空间诊断输出
for(i = 0; i < len; i++)
    kputc(mybuf[i]);
kputc(END_OF_KMESS);  // 触发输出
```

### 2.5 用户空间限制

用户空间限制定义了用户进程的地址空间边界。

**定义** (`const.h`)：

```c
/* User limits. */
#ifndef USR_DATATOP
#ifndef _MINIX_MAGIC
#define USR_DATATOP 0xF0000000
#else
#define USR_DATATOP 0xE0000000
#endif
#endif

#ifndef USR_STACKTOP
#define USR_STACKTOP USR_DATATOP
#endif

#ifndef USR_DATATOP_COMPACT
#define USR_DATATOP_COMPACT USR_DATATOP
#endif

#ifndef USR_STACKTOP_COMPACT
#define USR_STACKTOP_COMPACT 0x50000000
#endif
```

**地址空间布局**：

```
高地址
┌─────────────────────┐
│   内核空间          │  0xFFFFFFFF - USR_DATATOP
├─────────────────────┤ ← USR_DATATOP (0xF0000000)
│   用户栈区          │  向下增长
│         ↓           │
│                     │
│         ↑           │
│   用户数据/堆区     │  向上增长
├─────────────────────┤
│   用户代码段        │
└─────────────────────┘
低地址
```

#### 2.5.1 USR_DATATOP

`USR_DATATOP` 定义用户数据段的最高地址，是用户空间的上边界。

**定义**：

```c
#define USR_DATATOP 0xF0000000
```

**作用**：
- 用户空间和内核空间的分界线
- 用户进程不能访问高于此地址的内存
- 内核通过此值验证用户空间地址有效性

##### 2.5.1.1 默认值

默认值 `0xF0000000` (约 3.75 GB) 的选择原因：

**32位地址空间划分**：

```
地址范围              大小        用途
0x00000000-0xEFFFFFFF  ~3.75 GB   用户空间
0xF0000000-0xFFFFFFFF  ~256 MB    内核空间
```

**设计考量**：

1. **内核空间大小**：256 MB 足够内核代码、数据结构和驱动
2. **用户空间大小**：约 3.75 GB，满足大多数应用需求
3. **对齐友好**：`0xF0000000` 是 256 MB 对齐，便于内存管理

**条件编译变体**：

```c
#ifndef _MINIX_MAGIC
#define USR_DATATOP 0xF0000000   // 标准配置
#else
#define USR_DATATOP 0xE0000000   // 特殊配置，内核空间更大
#endif
```

##### 2.5.1.2 数据段顶部

用户数据段顶部定义了用户进程可访问的最高虚拟地址。

**含义**：

1. **数据段上界**：全局变量、静态变量、堆分配的内存不能超过此地址
2. **栈区起点**：栈从此地址向下增长
3. **权限边界**：高于此地址的内存只有内核可以访问

**在 fork 中的应用**：

```c
// VM 在创建子进程地址空间时使用此值
// 确保子进程的地址空间布局与父进程一致
child_vm->data_top = parent_vm->data_top;  // 通常为 USR_DATATOP
```

**地址验证**：

```c
// 内核验证用户空间地址
if (user_addr >= USR_DATATOP) {
    return EFAULT;  // 地址超出用户空间
}
```

#### 2.5.2 USR_STACKTOP

`USR_STACKTOP` 定义用户栈的最高地址。

**定义**：

```c
#ifndef USR_STACKTOP
#define USR_STACKTOP USR_DATATOP
#endif
```

**默认行为**：栈顶部等于数据段顶部，栈从最高地址向下增长。

##### 2.5.2.1 默认值

`USR_STACKTOP` 默认等于 `USR_DATATOP`，形成经典的地址空间布局。

**经典布局**：

```
USR_DATATOP ─────────┬─────────── USR_STACKTOP
                     │
                     │  栈区（向下增长）
                     │       ↓
                     │
                     │       ↑
                     │  堆区（向上增长）
                     │
─────────────────────┴─────────── 代码段起点
```

**为什么栈和数据段顶部相同**：

1. **简化设计**：栈和堆共享同一区域，中间是可用空间
2. **动态分配**：栈向下增长，堆向上增长，自动利用可用空间
3. **溢出检测**：栈和堆相撞时触发内存不足错误

##### 2.5.2.2 栈顶部

用户栈顶部是栈指针的初始值，也是栈增长的起点。

**含义**：

1. **栈指针初始值**：进程启动时，SP 寄存器指向此地址
2. **栈增长方向**：栈从此地址向下（低地址）增长
3. **函数调用栈帧**：每次函数调用从栈顶分配空间

**在 fork 中的应用**：

```c
// 子进程继承父进程的栈布局
// SP 指向与父进程相同的栈位置
rpc->p_reg.sp = rpp->p_reg.sp;  // 复制栈指针
```

**栈初始化**：

```c
// exec 时设置初始栈
sp = USR_STACKTOP;
sp -= argc * sizeof(char*);     // 参数数组
sp -= envc * sizeof(char*);     // 环境变量数组
```

#### 2.5.3 USR_DATATOP_COMPACT

`USR_DATATOP_COMPACT` 是紧凑模式下的数据段顶部。

**定义**：

```c
#ifndef USR_DATATOP_COMPACT
#define USR_DATATOP_COMPACT USR_DATATOP
#endif
```

**默认行为**：紧凑模式的数据段顶部等于标准模式。

**紧凑模式用途**：
- 用于内存受限的嵌入式系统
- 减少用户空间大小，留更多空间给内核

#### 2.5.4 USR_STACKTOP_COMPACT

`USR_STACKTOP_COMPACT` 是紧凑模式下的栈顶部。

**定义**：

```c
#ifndef USR_STACKTOP_COMPACT
#define USR_STACKTOP_COMPACT 0x50000000
#endif
```

**与标准模式的对比**：

| 模式 | 栈顶部 | 用户空间大小 |
|------|--------|-------------|
| 标准 | `0xF0000000` | ~3.75 GB |
| 紧凑 | `0x50000000` | ~1.25 GB |

##### 2.5.4.1 紧凑模式

紧凑模式的栈顶部 `0x50000000` (约 1.25 GB) 用于内存受限场景。

**紧凑模式布局**：

```
0x50000000 ─────────── 栈顶部（紧凑模式）
            │
            │  用户空间 (~1.25 GB)
            │
0x00000000 ─┴────────── 代码段起点

0x50000000 - 0xFFFFFFFF  内核空间 (~2.75 GB)
```

**使用场景**：

1. **嵌入式系统**：内存有限，需要更大的内核空间
2. **服务器场景**：内核需要管理大量资源，需要更大的内核地址空间
3. **特殊硬件**：某些硬件映射需要高地址区域

**选择 0x50000000 的原因**：

- 1.25 GB 用户空间对大多数应用足够
- 2.75 GB 内核空间可容纳大型驱动和数据结构
- 地址对齐，便于内存管理

---

## 3. Rust 设计决策

本节讨论如何用 Rust 实现常量定义，重点关注类型安全和编译期保证。

### 3.1 常量定义

Rust 提供多种常量定义方式，各有适用场景。

**常量定义方式**：

| 方式 | 关键字 | 特点 |
|------|--------|------|
| 编译期常量 | `const` | 内联，无地址 |
| 静态变量 | `static` | 有固定地址，可变需 `mut` |
| 关联常量 | `impl` 块中 | 与类型关联 |

**Rust 实现示例**：

```rust
// 编译期常量
pub const USR_DATATOP: usize = 0xF000_0000;
pub const USR_STACKTOP: usize = USR_DATATOP;
pub const END_OF_KMESS: u8 = 0;

// 带类型的端点
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Endpoint(i32);

impl Endpoint {
    pub const ANY: Self = Self(-1);
    pub const NONE: Self = Self(-2);
    pub const SELF: Self = Self(-3);
}
```

**类型安全优势**：

```rust
// C: 端点是 int，可以传任意整数
int endpoint = 12345;  // 可能无效

// Rust: 端点是强类型
let endpoint = Endpoint::from_raw(12345)?;  // 必须验证
```

### 3.2 位操作

Rust 的位操作比 C 更安全，但仍需注意边界条件。

**安全性改进**：

```rust
// C: 宏展开可能有副作用
#define set_sys_bit(map,bit) \
    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit)) )
// 问题: bit 被求值两次

// Rust: 函数保证单次求值
pub fn set_sys_bit(map: &mut SysMap, bit: usize) {
    let chunk_idx = bit / BITCHUNK_BITS;
    let offset = bit % BITCHUNK_BITS;
    map.chunk[chunk_idx] |= 1 << offset;
}
```

**边界检查**：

```rust
pub fn set_sys_bit_checked(map: &mut SysMap, bit: usize) -> Result<(), Error> {
    if bit >= NR_PROCS {
        return Err(Error::BitOutOfRange);
    }
    let chunk_idx = bit / BITCHUNK_BITS;
    let offset = bit % BITCHUNK_BITS;
    map.chunk[chunk_idx] |= 1 << offset;
    Ok(())
}
```

**使用 trait 抽象**：

```rust
pub trait Bitmap {
    fn get_bit(&self, bit: usize) -> bool;
    fn set_bit(&mut self, bit: usize);
    fn clear_bit(&mut self, bit: usize);
}

impl Bitmap for SysMap {
    fn get_bit(&self, bit: usize) -> bool {
        (self.chunk[bit / 32] & (1 << (bit % 32))) != 0
    }
    // ...
}
```

### 3.3 条件编译

Rust 使用 `cfg` 属性处理条件编译，比 C 的 `#ifdef` 更清晰。

**C 的条件编译**：

```c
#ifndef USR_DATATOP
#ifndef _MINIX_MAGIC
#define USR_DATATOP 0xF0000000
#else
#define USR_DATATOP 0xE0000000
#endif
#endif
```

**Rust 的条件编译**：

```rust
#[cfg(not(feature = "compact"))]
pub const USR_DATATOP: usize = 0xF000_0000;

#[cfg(feature = "compact")]
pub const USR_DATATOP: usize = 0xE000_0000;

// 或使用 cfg_attr
pub const USR_DATATOP: usize = if cfg!(feature = "compact") {
    0xE000_0000
} else {
    0xF000_0000
};
```

**Cargo.toml 配置**：

```toml
[features]
default = []
compact = []
```

**优势**：

1. **类型检查**：条件编译的代码仍需通过类型检查
2. **IDE 支持**：rust-analyzer 可以分析所有配置
3. **文档生成**：`cargo doc` 可以生成所有配置的文档

---

## 4. 实现

本节给出常量和位操作的 Rust 实现代码。

### 4.1 常量定义

```rust
//! 内核常量定义
//! 对应 minix3/minix/kernel/const.h

/// 用户空间限制
#[cfg(not(feature = "compact"))]
pub const USR_DATATOP: usize = 0xF000_0000;

#[cfg(feature = "compact")]
pub const USR_DATATOP: usize = 0xE000_0000;

/// 用户栈顶部（默认等于数据段顶部）
pub const USR_STACKTOP: usize = USR_DATATOP;

/// 紧凑模式栈顶部
#[cfg(feature = "compact")]
pub const USR_STACKTOP_COMPACT: usize = 0x5000_0000;

/// 内核消息结束标记
pub const END_OF_KMESS: u8 = 0;

/// 虚拟复制方向
pub const SRC: usize = 0;
pub const DST: usize = 1;

/// 每个 chunk 的位数（32位）
pub const BITCHUNK_BITS: usize = 32;
```

### 4.2 位操作函数

```rust
//! 位操作函数
//! 对应 minix3/minix/kernel/const.h 中的位操作宏

use super::{BITCHUNK_BITS, NR_PROCS};

/// 系统位图
#[derive(Clone, Default)]
pub struct SysMap {
    pub chunks: [u32; (NR_PROCS + 31) / 32],
}

impl SysMap {
    pub fn new() -> Self {
        Self { chunks: [0; (NR_PROCS + 31) / 32] }
    }

    /// 获取位图中某一位的状态
    pub fn get_bit(&self, bit: usize) -> bool {
        let chunk_idx = bit / BITCHUNK_BITS;
        let offset = bit % BITCHUNK_BITS;
        (self.chunks[chunk_idx] & (1 << offset)) != 0
    }

    /// 获取包含目标位的整个 chunk
    pub fn get_bits(&self, bit: usize) -> u32 {
        self.chunks[bit / BITCHUNK_BITS]
    }

    /// 设置位图中的某一位
    pub fn set_bit(&mut self, bit: usize) {
        let chunk_idx = bit / BITCHUNK_BITS;
        let offset = bit % BITCHUNK_BITS;
        self.chunks[chunk_idx] |= 1 << offset;
    }

    /// 清除位图中的某一位
    pub fn clear_bit(&mut self, bit: usize) {
        let chunk_idx = bit / BITCHUNK_BITS;
        let offset = bit % BITCHUNK_BITS;
        self.chunks[chunk_idx] &= !(1 << offset);
    }
}

/// 计算 chunk 索引
#[inline]
pub const fn map_chunk(bit: usize) -> usize {
    bit / BITCHUNK_BITS
}

/// 计算位在 chunk 内的偏移
#[inline]
pub const fn chunk_offset(bit: usize) -> usize {
    bit % BITCHUNK_BITS
}
```

### 4.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usr_datatop() {
        assert_eq!(USR_DATATOP, 0xF000_0000);
        assert_eq!(USR_STACKTOP, USR_DATATOP);
    }

    #[test]
    fn test_sys_map_basic() {
        let mut map = SysMap::new();
        
        // 初始状态：所有位为 0
        assert!(!map.get_bit(0));
        assert!(!map.get_bit(31));
        assert!(!map.get_bit(32));
        
        // 设置位
        map.set_bit(5);
        assert!(map.get_bit(5));
        assert!(!map.get_bit(4));
        assert!(!map.get_bit(6));
        
        // 清除位
        map.clear_bit(5);
        assert!(!map.get_bit(5));
    }

    #[test]
    fn test_sys_map_chunk_boundary() {
        let mut map = SysMap::new();
        
        // 测试 chunk 边界（位 31 和 32）
        map.set_bit(31);
        map.set_bit(32);
        assert!(map.get_bit(31));
        assert!(map.get_bit(32));
        
        // 验证它们在不同的 chunk
        assert_eq!(map_chunk(31), 0);
        assert_eq!(map_chunk(32), 1);
    }

    #[test]
    fn test_chunk_offset() {
        assert_eq!(chunk_offset(0), 0);
        assert_eq!(chunk_offset(31), 31);
        assert_eq!(chunk_offset(32), 0);
        assert_eq!(chunk_offset(35), 3);
    }

    #[test]
    fn test_end_of_kmess() {
        assert_eq!(END_OF_KMESS, 0);
    }
}
```

---

## 5. 参见

- [20-endpoint](20-endpoint.md) - 端点机制
- [21-type](21-type.md) - 基本类型定义
- [15-do-fork-validate](15-do-fork-validate.md) - fork 参数验证
