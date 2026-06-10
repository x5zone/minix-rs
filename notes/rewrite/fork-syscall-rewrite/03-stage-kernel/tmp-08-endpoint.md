# 08-endpoint: Endpoint 进程标识与 Generation 机制

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.c` endpoint 部分(~100行), `minix3/minix/include/minix/endpoint.h`
> **说明**: endpoint_lookup / isokendpt_f / generation 递增——解决 slot 重用导致的"错把 B 当成 A"问题

---

## 1. 概述

### 1.1 概念定义/作用

**Endpoint** 是 Minix3 中进程的通信标识符，用于 IPC 消息传递时标识发送方和接收方。每个进程有一个 endpoint 值，其他进程通过此值与它通信。

Endpoint 解决的核心问题是**进程槽位重用（slot reuse）**。进程表是固定大小的数组，一个进程退出后，其槽位可以被新进程占用。如果仅用槽位号（`p_nr`）标识进程，那么旧进程退出、新进程复用同一槽位后，原本发给旧进程的消息可能被新进程错误接收——"错把 B 当成 A"。

Endpoint 通过在槽位号上叠加 **generation 号**来解决此问题。每次槽位被复用时 generation 递增，使得旧 endpoint 值永远无法匹配新进程。内核在验证 endpoint 时同时检查槽位号和 generation 号，确保消息只能送达当前占用该槽位的进程。

### 1.2 与 Minix3 的对应关系

Endpoint 机制的核心实现分布在以下文件：

| 功能 | 文件 | 关键符号 |
|------|------|---------|
| Endpoint 布局定义 | `minix3/minix/include/minix/endpoint.h` | `_ENDPOINT`, `_ENDPOINT_G`, `_ENDPOINT_P` |
| 特殊 endpoint 值 | `minix3/minix/include/minix/endpoint.h` | `ANY`, `NONE`, `SELF` |
| Endpoint 验证 | `minix3/minix/kernel/proc.c:1830` | `isokendpt_f()` |
| Endpoint 查找 | `minix3/minix/kernel/proc.c:1818` | `endpoint_lookup()` |
| Generation 递增 | `minix3/minix/kernel/system/do_fork.c:59-72` | fork 时 generation++ |
| Endpoint 初始化 | `minix3/minix/kernel/proc.c:133` | `proc_init()` 中 `_ENDPOINT(0, p_nr)` |
| 验证宏包装 | `minix3/minix/kernel/const.h:12-13` | `isokendpt()`, `okendpt()` |

### 1.3 关键状态/机制说明

**Endpoint 编码布局**：一个 `endpoint_t`（int 类型）被分为两个字段：

```
|  generation (高 bits)  |  process slot (低 15 bits)  |
```

- **Generation**：占用高位，每次槽位重用时递增 1。初始值为 0
- **Process slot**：占用低 15 位（`_ENDPOINT_GENERATION_SHIFT = 15`），存储进程槽位号 `p_nr`

关键设计：当 generation 为 0 时，endpoint 值等于进程槽位号。这使得内核任务的 endpoint（如 `KERNEL=-1`, `CLOCK=-3`）可以直接用负数硬编码，无需计算。

**三个特殊 endpoint**：`ANY`、`NONE`、`SELF` 不对应任何真实进程，用于 IPC 语义：

- `ANY`：接收时表示"从任何进程接收"
- `NONE`：表示"无进程"（如初始化值）
- `SELF`：表示"自身进程"

这三个值被放在 slot 号空间的高端（`_ENDPOINT_SLOT_TOP` 附近），确保不与任何真实进程的 slot 号冲突。

**Generation 递增时机**：仅在 `do_fork()` 中，子进程复用进程表槽位时递增 generation。递增后若超过 `_ENDPOINT_MAX_GENERATION` 则回绕到 1（不回绕到 0，避免与初始值混淆）。

### 1.4 行为规则

1. **Endpoint 唯一性**：同一时刻，系统中不存在两个具有相同 endpoint 的活跃进程
2. **Generation 单调递增**：槽位每次被复用，generation 至少递增 1（回绕除外）
3. **Generation 0 特殊性**：generation 为 0 时 endpoint 等于 slot 号，用于硬编码的内核任务 endpoint
4. **验证三重条件**：`isokendpt_f()` 验证 endpoint 时同时检查：槽位号合法、槽位非空、进程的 `p_endpoint` 与传入 endpoint 匹配
5. **特殊值不可验证**：`ANY`/`NONE`/`SELF` 不通过 `isokendpt()` 验证——它们不是真实进程的 endpoint
6. **fatal 模式**：`okendpt()` 宏在验证失败时触发 panic，`isokendpt()` 仅返回 0

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 Endpoint 编码常量

定义于 `minix3/minix/include/minix/endpoint.h:45-57`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `_ENDPOINT_GENERATION_SHIFT` | 15 | generation 字段在 endpoint 中的位移量 |
| `_ENDPOINT_GENERATION_SIZE` | `1 << 15` = 32768 | generation 字段的大小 |
| `_ENDPOINT_MAX_GENERATION` | `INT_MAX / 32768 - 1` ≈ 65535 | generation 最大值（回绕前） |
| `_ENDPOINT_SLOT_TOP` | `32768 - MAX_NR_TASKS` = 31745 | slot 号空间上界 |
| `MAX_NR_PROCS` | `_ENDPOINT_SLOT_TOP - 3` ≈ 31742 | endpoint 布局允许的最大进程数 |

#### 2.1.2 特殊 Endpoint 值

| 常量 | 计算方式 | 实际值 | 含义 |
|------|---------|--------|------|
| `ANY` | `_ENDPOINT_SLOT_TOP - 1` | 31744 | 接收时匹配任何发送方 |
| `NONE` | `_ENDPOINT_SLOT_TOP - 2` | 31743 | 表示"无进程" |
| `SELF` | `_ENDPOINT_SLOT_TOP - 3` | 31742 | 表示"自身进程" |

#### 2.1.3 IPC 过滤器特殊 Endpoint

定义于 `minix3/minix/include/minix/ipc_filter.h:10-12`：

| 常量 | 计算方式 | 含义 |
|------|---------|------|
| `ANY_USR` | `_ENDPOINT(1, _ENDPOINT_P(ANY))` | 匹配任何用户进程 |
| `ANY_SYS` | `_ENDPOINT(2, _ENDPOINT_P(ANY))` | 匹配任何系统进程 |
| `ANY_TSK` | `_ENDPOINT(3, _ENDPOINT_P(ANY))` | 匹配任何内核任务 |

这些值通过在 ANY 的 slot 号上叠加不同的 generation 号实现，仅用于 IPC 过滤器，不用于普通 IPC。

### 2.2 核心数据结构

#### 2.2.1 Endpoint 编码格式

一个 `endpoint_t`（typedef int）的位布局：

```
31                              15                              0
┌───────────────────────────────┬───────────────────────────────┐
│       generation number       │       process slot number     │
│        (高 17 bits)           │        (低 15 bits)           │
└───────────────────────────────┴───────────────────────────────┘
```

- 低 15 位：进程槽位号 `p_nr`（范围 [-MAX_NR_TASKS, MAX_NR_PROCS)）
- 高位：generation 号（范围 [0, _ENDPOINT_MAX_GENERATION)）

对于内核任务（`p_nr` 为负数），slot 字段存储的是 `p_nr + MAX_NR_TASKS` 的低 15 位，使得负数也能正确编码。

#### 2.2.2 proc 结构中的 Endpoint 字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `p_endpoint` | `endpoint_t` | 进程当前 endpoint（含 generation） |
| `p_nr` | `proc_nr_t` | 进程槽位号（不含 generation，生命周期不变） |

`p_nr` 在进程生命周期内不变，`p_endpoint` 在槽位被复用时更新（generation 递增）。

### 2.3 关键函数分析

#### 2.3.1 _ENDPOINT(g, p)——构造 Endpoint

`minix3/minix/include/minix/endpoint.h:65-66`

```c
#define _ENDPOINT(g, p) \
    ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

**功能**：将 generation 号 `g` 和进程槽位号 `p` 组合成一个 endpoint 值。

**行为**：将 generation 左移 15 位后加上 slot 号。当 `g=0` 时，结果等于 `p` 本身，确保硬编码的内核任务 endpoint 正确。

**使用场景**：
- `proc_init()` 中：`_ENDPOINT(0, p_nr)` 初始化进程 endpoint
- `do_fork()` 中：`_ENDPOINT(gen, p_nr)` 为子进程创建新 endpoint

#### 2.3.2 _ENDPOINT_G(e)——提取 Generation 号

`minix3/minix/include/minix/endpoint.h:67`

```c
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)
```

**功能**：从 endpoint 值中提取 generation 号。

**行为**：先将 endpoint 加上 `MAX_NR_TASKS`（处理负数 slot 号的偏移），再右移 15 位得到 generation。

**注意**：加 `MAX_NR_TASKS` 是因为内核任务的 slot 号为负数，直接右移会丢失符号位。加上偏移后所有 slot 号变为非负数，右移结果正确。

#### 2.3.3 _ENDPOINT_P(e)——提取进程槽位号

`minix3/minix/include/minix/endpoint.h:68-69`

```c
#define _ENDPOINT_P(e) \
    ((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

**功能**：从 endpoint 值中提取进程槽位号。

**行为**：
1. 加上 `MAX_NR_TASKS` 偏移
2. 与 `0x7FFF`（低 15 位掩码）做与运算，提取 slot 部分
3. 减去 `MAX_NR_TASKS` 恢复原始 slot 号（可能为负数）

#### 2.3.4 isokendpt_f()——Endpoint 验证

`minix3/minix/kernel/proc.c:1830-1858`

```c
int isokendpt_f(endpoint_t e, int *p, const int fatalflag)
```

**功能**：验证 endpoint 是否对应一个当前活跃的进程，并返回其进程号。

**行为**：
1. 调用 `_ENDPOINT_P(e)` 提取进程号到 `*p`
2. 三重验证：
   - `isokprocn(*p)`：进程号在合法范围内
   - `!isemptyn(*p)`：进程槽位非空（`p_rts_flags != RTS_SLOT_FREE`）
   - `proc_addr(*p)->p_endpoint == e`：进程当前 endpoint 与传入值匹配（generation 一致）
3. 三重验证全部通过返回 1（成功），否则返回 0
4. 若 `fatalflag` 非零且验证失败，调用 `panic()` 终止系统

**Generation 验证的关键性**：第三步 `p_endpoint == e` 是防止"错把 B 当成 A"的核心。即使 slot 号合法且非空，如果 generation 不匹配（旧进程已退出、新进程已占用），验证仍然失败。

**两个调用接口**：

| 宏 | fatalflag | 行为 |
|----|-----------|------|
| `isokendpt(e, p)` | 0 | 验证失败返回 0，不 panic |
| `okendpt(e, p)` | 1 | 验证失败触发 panic |

#### 2.3.5 endpoint_lookup()——Endpoint 查找

`minix3/minix/kernel/proc.c:1818-1825`

```c
struct proc *endpoint_lookup(endpoint_t e)
```

**功能**：通过 endpoint 查找对应的进程控制块指针。

**行为**：
1. 调用 `isokendpt(e, &n)` 验证 endpoint 并获取进程号
2. 若验证失败返回 NULL
3. 若验证成功返回 `proc_addr(n)`

**与 `proc_addr()` 的区别**：`proc_addr(n)` 仅按进程号索引，不验证 endpoint 有效性；`endpoint_lookup(e)` 先验证再索引，安全但较慢。

#### 2.3.6 Generation 递增（do_fork 中）

`minix3/minix/kernel/system/do_fork.c:59-72`

```c
gen = _ENDPOINT_G(rpc->p_endpoint);
// ... copy parent proc struct to child ...
if(++gen >= _ENDPOINT_MAX_GENERATION)
    gen = 1;
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
```

**功能**：fork 子进程时递增槽位的 generation 号。

**行为**：
1. 提取子进程槽位当前的 generation 号
2. 复制父进程的 proc 结构到子进程（`*rpc = *rpp`）
3. generation 递增 1，若超过 `_ENDPOINT_MAX_GENERATION` 则回绕到 1
4. 用新 generation 和子进程 slot 号构造新 endpoint
5. 恢复子进程的 `p_nr`（被复制覆盖了）

**回绕到 1 而非 0**：generation 0 有特殊含义（endpoint 等于 slot 号），回绕时跳过 0 避免与初始状态混淆。

### 2.4 调用关系/调用点分析

#### 2.4.1 Endpoint 验证调用链

```
IPC 消息传递 (mini_send / mini_receive / mini_notify)
  └─ isokendpt(e, &p) 或 okendpt(e, &p)
       └─ isokendpt_f(e, &p, fatalflag)
            ├─ _ENDPOINT_P(e)         // 提取 slot 号
            ├─ isokprocn(p)           // 验证 slot 号范围
            ├─ isemptyn(p)            // 验证 slot 非空
            └─ proc_addr(p)->p_endpoint == e  // 验证 generation
```

#### 2.4.2 Endpoint 生命周期

```
进程创建 (proc_init)
  └─ p_endpoint = _ENDPOINT(0, p_nr)    // generation = 0

进程 fork (do_fork)
  └─ gen = _ENDPOINT_G(p_endpoint) + 1  // generation 递增
  └─ p_endpoint = _ENDPOINT(gen, p_nr)  // 新 endpoint

进程退出 (do_clear)
  └─ p_rts_flags = RTS_SLOT_FREE        // slot 标记空闲
  └─ p_endpoint 不重置                  // 保留旧值供验证失败

新进程复用 slot
  └─ gen = _ENDPOINT_G(p_endpoint) + 1  // generation 再次递增
  └─ p_endpoint = _ENDPOINT(gen, p_nr)  // 新 endpoint
```

#### 2.4.3 isokendpt / okendpt 使用统计

| 使用场景 | 调用接口 | 失败处理 |
|---------|---------|---------|
| IPC 消息发送/接收 | `isokendpt()` | 返回错误码 |
| 内核调用参数验证 | `okendpt()` | panic |
| endpoint_lookup | `isokendpt()` | 返回 NULL |
| fork 父进程验证 | `isokendpt()` | 返回 EINVAL |

### 2.5 设计要点/特殊处理

#### 2.5.1 Generation 0 的特殊含义

当 generation 为 0 时，`_ENDPOINT(0, p_nr) == p_nr`。这个设计使得内核任务的 endpoint 可以直接用负数硬编码在 `com.h` 中：

```c
#define CLOCK   ((endpoint_t) -3)   // _ENDPOINT(0, -3) == -3
#define SYSTEM  ((endpoint_t) -2)   // _ENDPOINT(0, -2) == -2
#define KERNEL  ((endpoint_t) -1)   // _ENDPOINT(0, -1) == -1
```

内核任务永远不会退出和重启，其 generation 始终为 0，endpoint 值永远等于 slot 号。这简化了内核代码中对这些任务的引用。

#### 2.5.2 15 位位移的权衡

`_ENDPOINT_GENERATION_SHIFT = 15` 决定了 slot 号和 generation 号的位数分配：

- **Slot 号空间**：2^15 = 32768 个值，减去 MAX_NR_TASKS(1023) 和 3 个特殊值，最多支持约 31742 个进程
- **Generation 空间**：约 65535 次重用

这个分配是进程数量和重用次数之间的权衡。增大位移可以支持更多 generation（更安全的重用），但减少最大进程数。当前配置下 `NR_PROCS=256` 远小于 `MAX_NR_PROCS≈31742`，有充足的扩展空间。

#### 2.5.3 负数 Slot 号的处理

内核任务的 slot 号为负数（-5 ~ -1），直接存储在 endpoint 的低 15 位中会导致符号扩展问题。`_ENDPOINT_G` 和 `_ENDPOINT_P` 宏通过加减 `MAX_NR_TASKS` 偏移来处理：

- 编码时：`_ENDPOINT(g, p) = (g << 15) + p`，负数 `p` 的补码表示自然编码
- 解码时：先加 `MAX_NR_TASKS` 使所有值非负，提取后再减回来

#### 2.5.4 特殊 Endpoint 的位置选择

`ANY`、`NONE`、`SELF` 被放在 slot 号空间的高端（`_ENDPOINT_SLOT_TOP - 1/2/3`），确保：

1. 不与任何真实进程的 slot 号冲突（真实 slot 号范围 [-MAX_NR_TASKS, MAX_NR_PROCS)）
2. Generation 为 0，使得这些值在 endpoint 空间中是固定的
3. 可以通过 `_ENDPOINT_P()` 提取其 slot 部分，但 `isokprocn()` 验证会失败

#### 2.5.5 验证失败不重置 endpoint

进程退出时（`do_clear()`），`p_endpoint` 不被重置为初始值。这意味着旧 endpoint 值保留在 slot 中，直到新进程复用该 slot 并设置新 endpoint。这是安全的，因为：

1. 退出进程的 `p_rts_flags = RTS_SLOT_FREE`，`isemptyn()` 检查会失败
2. 即使有人用旧 endpoint 尝试通信，`isokendpt_f()` 的三重验证中第二步就会失败

#### 2.5.6 IPC 过滤器中的 Generation 复用

`ANY_USR`、`ANY_SYS`、`ANY_TSK` 利用 generation 字段来区分"任何用户进程"、"任何系统进程"、"任何内核任务"。它们使用 ANY 的 slot 号但不同的 generation 号（1、2、3），这是对 endpoint 编码的创造性复用，但仅限于 IPC 过滤器内部，不影响普通 IPC 语义。
