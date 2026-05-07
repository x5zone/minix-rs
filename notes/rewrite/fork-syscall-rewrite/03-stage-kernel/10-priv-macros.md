# 10-priv-macros - 特权访问宏

> 本文档分析 `minix3/minix/kernel/priv.h` 第 61-105 行，讲解特权访问宏的定义。

---

## 1. 概述

特权访问宏（Privilege Access Macros）是 Minix3 内核中用于访问和管理特权结构体的一组宏定义。这些宏提供了便捷的方式来获取特权表项、检查权限以及管理特权相关操作。

### 1.1 特权表访问

特权表（`priv` 数组）存储了系统中所有进程（包括系统进程和用户进程）的特权信息。特权访问宏提供了多种方式来访问这个表：

```
┌─────────────────────────────────────────────────────────────┐
│                    特权表访问方式                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 通过索引访问                                            │
│     priv_addr(i) -> 获取第 i 个特权表项的指针               │
│                                                             │
│  2. 通过进程号访问                                          │
│     priv(rp) -> 获取进程 rp 的特权结构体指针                │
│                                                             │
│  3. 通过进程号获取 ID                                       │
│     nr_to_id(nr) -> 获取进程号 nr 对应的特权 ID            │
│                                                             │
│  4. 通过 ID 获取进程号                                      │
│     id_to_nr(id) -> 获取特权 ID 对应的进程号               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 与 fork 的关系

在 `do_fork()` 系统调用中，特权访问宏被广泛用于检查和处理父进程和子进程的特权：

```c
// do_fork.c 中使用特权访问宏

// 1. 获取父进程的特权指针
struct priv *parent_priv = priv(rpp);

// 2. 检查父进程是否为系统进程
if (parent_priv->s_flags & SYS_PROC) {
    // 父进程是系统进程，子进程需要降级
    
    // 3. 获取用户特权结构体指针
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    
    // 4. 设置禁止运行标志
    rpc->p_rts_flags |= RTS_NO_PRIV;
}

// 5. 检查父进程是否有特定权限
if (may_send_to(rpp, target_nr)) {
    // 父进程可以向 target_nr 发送消息
}

// 6. 获取子进程的特权 ID
sys_id_t child_priv_id = nr_to_id(rpc->p_nr);
```

特权访问宏在 fork 时的使用流程：

```
┌─────────────────────────────────────────────────────────────┐
│               fork 时特权访问宏的使用流程                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  do_fork()                                                  │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ priv(rpp)       │ 获取父进程特权指针                     │
│  │ 检查父进程特权   │                                        │
│  └─────────────────┘                                        │
│     │                                                       │
│     ▼                                                       │
│  ┌─────────────────┐                                        │
│  │ s_flags & SYS_PROC    │ 检查是否为系统进程                │
│  └─────────────────┘                                        │
│     │                                                       │
│     ├─是──────────────────────────────────────────┐          │
│     │                                            │          │
│     ▼                                            ▼          │
│  ┌──────────────┐                        ┌────────────────┐ │
│  │ 用户进程      │                        │ priv_addr()    │ │
│  │ 继承父特权    │                        │ 获取用户特权结构体│ │
│  └──────────────┘                        └────────────────┘ │
│     │                                            │          │
│     ▼                                            ▼          │
│  ┌─────────────────┐                        ┌────────────────┐│
│  │ priv(rpc) =     │                        │ rpc->p_priv =   ││
│  │ priv(rpp)       │                        │ 用户特权指针    ││
│  └─────────────────┘                        └────────────────┘│
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 常量定义

特权相关常量定义了特权访问机制中的关键数值，包括栈保护字、特权表地址等。这些常量为内核提供了安全检查和边界保护的基础。

### 2.1 STACK_GUARD

`STACK_GUARD` 定义了特权结构体中栈保护字的魔术值，用于检测内核任务栈溢出。

```c
/* Guard word for task stacks. */
#define STACK_GUARD	((reg_t) (sizeof(reg_t) == 2 ? 0xBEEF : 0xDEADBEEF))
```

该常量根据 `reg_t` 类型的大小自动选择合适的魔术值：
- 16位系统（`sizeof(reg_t) == 2`）：使用 `0xBEEF`
- 32/64位系统：使用 `0xDEADBEEF`

#### 2.1.1 栈保护字

栈保护字（Stack Guard Word）存储在 `priv` 结构体的 `s_stack_guard` 字段中：

```c
struct priv {
  // ... 其他字段 ...
  reg_t *s_stack_guard;		/* stack guard word for kernel tasks */
  // ...
};
```

**作用机制：**

1. **初始化**：内核任务创建时，`s_stack_guard` 指向栈底附近的特定位置，并初始化为 `STACK_GUARD` 值

2. **溢出检测**：在任务切换或系统调用返回时，内核检查 `*s_stack_guard` 是否仍等于 `STACK_GUARD`

3. **错误处理**：如果值被修改，说明发生了栈溢出，内核触发 panic 或采取恢复措施

#### 2.1.2 魔术值

`0xDEADBEEF` 是一个在计算机科学中广泛使用的魔术值（Magic Number），具有以下特点：

**选择原因：**

1. **易于识别**：在十六进制表示中非常显眼，便于在调试器中识别
2. **低概率冲突**：这个特定值在正常程序执行中极少出现
3. **历史传统**：从早期 IBM 系统开始就被用作调试标记

**字面含义：**
- `DEAD`：表示内存区域"死亡"或不可访问
- `BEEF`： beef 的英文单词，暗示"内容"或"实质"

**16位变体 `0xBEEF`：**
- 在16位系统中，`0xDEADBEEF` 超出单字范围
- 使用 `0xBEEF` 作为等效魔术值，保持可识别性

---

## 3. 特权表地址宏

特权表地址宏（Privilege Table Address Macros）定义了特权表（`priv` 数组）的地址范围和分区边界。这些宏提供了一种类型安全的方式来访问特权表的不同区域，包括静态特权槽和动态特权槽。

### 3.1 特权表地址空间布局

特权表在内存中的布局如下图所示：

```
┌─────────────────────────────────────────────────────────────────┐
│                     特权表地址空间布局                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  低地址                                                          │
│                                                                 │
│  ┌─────────────────┐ ← BEG_PRIV_ADDR (BEG_STATIC_PRIV_ADDR)      │
│  │  静态特权槽 0   │     priv[0] - 内核任务 (IDLE)                 │
│  ├─────────────────┤                                             │
│  │  静态特权槽 1   │     priv[1] - 系统进程 1                     │
│  ├─────────────────┤                                             │
│  │       ...       │                                             │
│  ├─────────────────┤                                             │
│  │  静态特权槽 N-1 │     priv[NR_STATIC_PRIV_IDS-1]               │
│  └─────────────────┘ ← END_STATIC_PRIV_ADDR (BEG_DYN_PRIV_ADDR) │
│                                                                 │
│  ─────────────────── 静态/动态边界 ──────────────────────────────│
│                                                                 │
│  ┌─────────────────┐                                             │
│  │  动态特权槽 0   │     priv[NR_STATIC_PRIV_IDS]                 │
│  ├─────────────────┤                                             │
│  │  动态特权槽 1   │                                             │
│  ├─────────────────┤                                             │
│  │       ...       │                                             │
│  ├─────────────────┤                                             │
│  │  动态特权槽 M-1 │     priv[NR_SYS_PROCS-1]                     │
│  └─────────────────┘ ← END_PRIV_ADDR (END_DYN_PRIV_ADDR)          │
│                                                                 │
│  高地址                                                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

关键常量说明：
- NR_STATIC_PRIV_IDS: 静态特权槽数量（系统进程 + 内核任务）
- NR_DYN_PRIV_IDS: 动态特权槽数量（NR_SYS_PROCS - NR_STATIC_PRIV_IDS）
- NR_SYS_PROCS: 系统总特权槽数量（静态 + 动态）
```

### 3.2 地址宏的分类

特权表地址宏可以分为以下三类：

| 宏类型 | 宏名称 | 说明 |
|--------|--------|------|
| **整体边界** | `BEG_PRIV_ADDR` / `END_PRIV_ADDR` | 整个特权表的起止地址 |
| **静态区域** | `BEG_STATIC_PRIV_ADDR` / `END_STATIC_PRIV_ADDR` | 静态特权槽的范围 |
| **动态区域** | `BEG_DYN_PRIV_ADDR` / `END_DYN_PRIV_ADDR` | 动态特权槽的范围 |

### 3.1 BEG_PRIV_ADDR

`BEG_PRIV_ADDR` 宏定义了特权表的起始地址，即第一个特权槽（`priv[0]`）的地址。

**宏定义（来自 minix/kernel/priv.h:72）：**

```c
#define BEG_PRIV_ADDR              (&priv[0])
```

**定义解析：**

| 组成部分 | 说明 |
|---------|------|
| `&priv[0]` | 取特权表数组 `priv` 的第0个元素的地址 |
| 返回值类型 | `struct priv *`（指向特权结构体的指针） |

**使用示例：**

```c
// 遍历所有特权槽
struct priv *priv_slot = BEG_PRIV_ADDR;
for (int i = 0; i < NR_SYS_PROCS; i++, priv_slot++) {
    // 处理每个特权槽
    if (priv_slot->s_proc_nr != NO_PROC_NR) {
        // 该槽已被占用
    }
}

// 计算特权表大小
size_t priv_table_size = (size_t)(END_PRIV_ADDR - BEG_PRIV_ADDR);
```

**与 BEG_STATIC_PRIV_ADDR 的关系：**

```
BEG_PRIV_ADDR = BEG_STATIC_PRIV_ADDR
                ↓
          静态区域起始地址
          (priv[0])
```

#### 3.1.1 特权表起始地址的含义

**内存布局中的位置：**

```
内存低地址 ────────────────────────►

     ┌─────────────────────────┐ ← BEG_PRIV_ADDR
     │                         │       priv[0]
     │    静态特权槽区域        │
     │   (内核任务 + 系统进程)   │
     │                         │
     ├─────────────────────────┤ ← END_STATIC_PRIV_ADDR
     │                         │
     │    动态特权槽区域        │
     │      (用户进程)          │
     │                         │
     └─────────────────────────┘ ← END_PRIV_ADDR
                                        priv[NR_SYS_PROCS]

◄────────────────────────────────────  内存高地址
```

**关键意义：**

1. **基准点（Anchor Point）**
   - 所有特权表地址计算都以此为基准
   - `priv[i]` 的地址可以通过 `BEG_PRIV_ADDR + i` 计算

2. **边界检查（Bounds Checking）**
   - 任何有效的特权指针 `p` 必须满足：`BEG_PRIV_ADDR <= p < END_PRIV_ADDR`
   - 静态特权检查：`BEG_STATIC_PRIV_ADDR <= p < END_STATIC_PRIV_ADDR`

3. **与特权 ID 的映射（ID Mapping）**
   - 特权 ID 就是相对于 `BEG_PRIV_ADDR` 的偏移量
   - `priv_id = priv_ptr - BEG_PRIV_ADDR`

### 3.2 END_PRIV_ADDR

`END_PRIV_ADDR` 宏定义了特权表的结束地址，即最后一个有效特权槽之后的位置（`priv[NR_SYS_PROCS]`）。

**宏定义（来自 minix/kernel/priv.h:73）：**

```c
#define END_PRIV_ADDR              (&priv[NR_SYS_PROCS])
```

**定义解析：**

| 组成部分 | 说明 |
|---------|------|
| `&priv[NR_SYS_PROCS]` | 取特权表数组 `priv` 的第 `NR_SYS_PROCS` 个元素的地址 |
| 返回值类型 | `struct priv *`（指向特权结构体的指针） |
| 语义 | 尾后指针（one-past-the-end pointer），类似 C++ STL 中的 `end()` |

**关键特性：**

```
内存布局：

    ┌───────────────────┐         ┌───────────────────┐
    │   priv[0]         │         │ priv[NR_SYS_PROCS]│
    │   (第一个槽)       │   ...   │   (尾后位置)      │
    └───────────────────┘         └───────────────────┘
            ▲                                 ▲
            │                                 │
      BEG_PRIV_ADDR                     END_PRIV_ADDR
      (有效范围起点)                     (有效范围终点之后)
```

**使用示例：**

```c
// 1. 计算特权表中的槽数量
int num_priv_slots = END_PRIV_ADDR - BEG_PRIV_ADDR;  // = NR_SYS_PROCS

// 2. 遍历所有有效特权槽（使用索引）
for (int i = 0; i < (END_PRIV_ADDR - BEG_PRIV_ADDR); i++) {
    struct priv *sp = BEG_PRIV_ADDR + i;
    // 处理 sp
}

// 3. 指针范围检查
bool is_valid_priv_ptr(struct priv *sp) {
    return sp >= BEG_PRIV_ADDR && sp < END_PRIV_ADDR;
}

// 4. 计算静态/动态区域大小
int static_slots = END_STATIC_PRIV_ADDR - BEG_STATIC_PRIV_ADDR;
int dynamic_slots = END_DYN_PRIV_ADDR - BEG_DYN_PRIV_ADDR;
```

**与 BEG_PRIV_ADDR 的关系：**

```
BEG_PRIV_ADDR                    END_PRIV_ADDR
      │                                │
      ▼                                ▼
┌──────────────────────────────────────────┐
│                                          │
│   有效特权槽区域 (NR_SYS_PROCS 个槽)       │
│                                          │
└──────────────────────────────────────────┘

有效槽数量 = END_PRIV_ADDR - BEG_PRIV_ADDR = NR_SYS_PROCS
```

**安全性考虑：**

1. **尾后指针的合法性**：`END_PRIV_ADDR` 指向数组尾后位置，根据 C 语言标准，这个位置的指针可以进行比较运算（如 `<`、`>=`），但不能解引用（`*` 或 `->`）

2. **边界检查的必要性**：所有特权指针在使用前都应验证是否在 `[BEG_PRIV_ADDR, END_PRIV_ADDR)` 范围内

3. **与 NR_SYS_PROCS 的同步**：如果 `NR_SYS_PROCS` 配置改变，必须重新编译以确保 `END_PRIV_ADDR` 正确

### 3.3 BEG_STATIC_PRIV_ADDR

`BEG_STATIC_PRIV_ADDR` 宏定义了静态特权槽区域的起始地址。静态特权槽是为内核任务和系统进程预留的特权表项，在系统启动时分配，运行时不会改变。

**宏定义（来自 minix/kernel/priv.h:74）：**

```c
#define BEG_STATIC_PRIV_ADDR       BEG_PRIV_ADDR
```

**定义解析：**

| 组成部分 | 说明 |
|---------|------|
| `BEG_PRIV_ADDR` | 即 `&priv[0]`，特权表的起始地址 |
| 等价定义 | `BEG_STATIC_PRIV_ADDR = &priv[0]` |

**关键特性：**

```
静态特权槽区域布局：

    BEG_STATIC_PRIV_ADDR (BEG_PRIV_ADDR)
              │
              ▼
    ┌─────────────────┐ priv[0]: 内核任务 (IDLE/clock/task)
    │  静态特权槽 0   │       s_id = 0
    ├─────────────────┤
    │  静态特权槽 1   │ priv[1]: 系统进程 (如 PM)
    │                 │       s_id = 1
    ├─────────────────┤
    │       ...       │ ...
    ├─────────────────┤
    │  静态特权槽 N-1 │ priv[NR_STATIC_PRIV_IDS-1]
    │                 │       s_id = NR_STATIC_PRIV_IDS-1
    └─────────────────┘
              │
              ▼
    END_STATIC_PRIV_ADDR (BEG_DYN_PRIV_ADDR)

其中 N = NR_STATIC_PRIV_IDS
```

**使用示例：**

```c
// 1. 检查特权指针是否在静态区域
bool is_static_priv(struct priv *sp) {
    return sp >= BEG_STATIC_PRIV_ADDR && sp < END_STATIC_PRIV_ADDR;
}

// 2. 遍历所有静态特权槽
struct priv *sp;
for (sp = BEG_STATIC_PRIV_ADDR; sp < END_STATIC_PRIV_ADDR; sp++) {
    // 处理静态特权槽 sp
    if (sp->s_proc_nr != NO_PROC_NR) {
        printf("Static slot %d: process %d\n", 
               sp - BEG_STATIC_PRIV_ADDR, sp->s_proc_nr);
    }
}

// 3. 计算静态区域大小
int num_static_slots = END_STATIC_PRIV_ADDR - BEG_STATIC_PRIV_ADDR;
// 等价于: NR_STATIC_PRIV_IDS

// 4. 获取静态区域起始的特权 ID
sys_id_t first_static_id = BEG_STATIC_PRIV_ADDR - BEG_PRIV_ADDR;  // = 0
```

#### 3.3.1 静态特权槽

**定义：**

静态特权槽（Static Privilege Slots）是特权表中为内核任务和系统进程预留的固定位置。这些槽位具有以下特点：

1. **预分配**：在编译时确定数量，由 `NR_STATIC_PRIV_IDS` 定义
2. **持久性**：系统运行期间始终存在，不会被回收
3. **专用性**：每个静态槽对应一个特定的内核任务或系统进程
4. **索引固定**：槽位索引与进程身份绑定，不会改变

**静态特权槽分配示例：**

```
NR_STATIC_PRIV_IDS = 9 (包含内核任务和基本系统进程)

静态特权槽分配表：
┌─────────┬─────────────┬──────────────────┬────────────────┐
│ 槽索引  │   进程号     │     进程名        │     说明       │
├─────────┼─────────────┼──────────────────┼────────────────┤
│    0    │   KERNEL    │   kernel task    │  内核调度任务   │
│    1    │   CLOCK     │   clock task     │  时钟任务       │
│    2    │   SYSTEM    │   system task    │  系统任务       │
│    3    │   HARDWARE  │   hardware driver│  硬件抽象驱动   │
│    4    │   PM_PROC_NR│   PM (进程管理)  │  进程管理器     │
│    5    │   FS_PROC_NR│   FS (文件系统)  │  文件系统       │
│    6    │   RS_PROC_NR│   RS (重启动)    │  重启动服务     │
│    7    │   DS_PROC_NR│   DS (数据存储)  │  数据存储服务   │
│    8    │   VM_PROC_NR│   VM (虚拟内存)  │  虚拟内存管理器 │
└─────────┴─────────────┴──────────────────┴────────────────┘
```

#### 3.3.2 系统进程特权

**系统进程使用静态特权槽的机制：**

系统进程（如 PM、FS、RS 等）在 Minix3 启动时通过引导镜像（boot image）加载，它们的特权槽在编译时就已确定：

```
系统启动时的特权分配流程：

1. 引导镜像定义 (image.h)
   ┌─────────────────────────────────────┐
   │  struct boot_image {                │
   │      char *name;        // 进程名    │
   │      int proc_nr;       // 进程号    │
   │      int priv_id;       // 特权ID   │
   │      ...                            │
   │  };                                 │
   └─────────────────────────────────────┘

2. 启动时初始化 (main.c)
   main()
       │
       ├── kinfo_init()          // 初始化内核信息
       │
       ├── vm_init()             // 初始化VM（此时priv表已初始化）
       │
       └── init_boot_image()     // 从引导镜像初始化进程
               │
               ├── 遍历引导镜像中的每个进程
               │
               ├── 调用 alloc_priv_id() 获取特权ID
               │   └── 对于系统进程，返回静态分配的ID
               │
               ├── 调用 proc_init() 初始化进程结构
               │   └── 设置 p_priv = priv_addr(priv_id)
               │
               └── 复制特权信息到 priv[priv_id]
```

**系统进程的特权特征：**

| 特征 | 说明 | 在静态特权槽中的体现 |
|------|------|---------------------|
| **固定身份** | 进程号（proc_nr）在编译时确定 | 槽索引与进程号一一对应 |
| **持久存在** | 进程生命周期与系统相同 | 槽位不会被回收或重新分配 |
| **完整权限** | 拥有几乎所有特权标志 | `s_flags` 设置为 `SYS_PROC` 等 |
| **IPC 能力** | 可向任何进程发送消息 | `s_ipc_to` 掩码允许所有目标 |
| **内核调用** | 可执行所有内核调用 | `s_k_call_mask` 全置位 |

### 3.4 END_STATIC_PRIV_ADDR

**定义**（`kernel/priv.h:75`）：

```c
#define END_STATIC_PRIV_ADDR       (BEG_STATIC_PRIV_ADDR + NR_STATIC_PRIV_IDS)
```

`END_STATIC_PRIV_ADDR` 指向静态特权区域的末尾（past-the-end），即最后一个静态特权槽之后的位置。它等于 `BEG_STATIC_PRIV_ADDR` 加上静态特权槽数量 `NR_STATIC_PRIV_IDS`（即 `NR_BOOT_PROCS`）。

**在特权表中的位置：**

```
priv[] 特权表：
┌─────────────────────┐ ◄── BEG_PRIV_ADDR (= BEG_STATIC_PRIV_ADDR)
│  priv[0]            │     静态特权槽 0
├─────────────────────┤
│  priv[1]            │     静态特权槽 1
├─────────────────────┤
│       ...           │
├─────────────────────┤
│  priv[N-1]          │     静态特权槽 N-1 (N = NR_STATIC_PRIV_IDS)
└─────────────────────┘ ◄── END_STATIC_PRIV_ADDR
│  priv[N]            │     动态特权槽开始
├─────────────────────┤
│       ...           │
└─────────────────────┘ ◄── END_PRIV_ADDR
```

**关键性质：**

1. **静态区域的边界**：`END_STATIC_PRIV_ADDR` 划分了静态特权槽与动态特权槽的边界。任何 `sp < END_STATIC_PRIV_ADDR && sp >= BEG_STATIC_PRIV_ADDR` 的指针都属于静态区域
2. **等于动态区域起始**：由于 `BEG_DYN_PRIV_ADDR` 定义为 `END_STATIC_PRIV_ADDR`，它同时标记了动态特权区域的起点
3. **与 `is_static_priv_id` 的对应**：在 `include/minix/priv.h:11` 中，`is_static_priv_id(id)` 检查 `id >= 0 && id < NR_STATIC_PRIV_IDS`，与此宏的语义一致——特权 ID 小于 `NR_STATIC_PRIV_IDS` 的即为静态槽

**使用场景：**

- 遍历所有静态特权槽：`for (sp = BEG_STATIC_PRIV_ADDR; sp < END_STATIC_PRIV_ADDR; sp++)`
- 判断特权结构是否属于静态区域：`sp >= BEG_STATIC_PRIV_ADDR && sp < END_STATIC_PRIV_ADDR`
- 计算静态槽数量：`END_STATIC_PRIV_ADDR - BEG_STATIC_PRIV_ADDR == NR_STATIC_PRIV_IDS`

### 3.5 BEG_DYN_PRIV_ADDR

**定义**（`kernel/priv.h:76`）：

```c
#define BEG_DYN_PRIV_ADDR          END_STATIC_PRIV_ADDR
```

`BEG_DYN_PRIV_ADDR` 指向动态特权区域的起始位置。由于它直接定义为 `END_STATIC_PRIV_ADDR`，静态特权区域的末尾即是动态特权区域的起点，两者在内存中紧密衔接，没有间隔。

**在特权表中的位置：**

```
priv[] 特权表：
                         ┌── BEG_STATIC_PRIV_ADDR (= BEG_PRIV_ADDR)
  ┌─────────────────────┐
  │  静态特权槽 0..N-1  │  N = NR_STATIC_PRIV_IDS
  └─────────────────────┘ ◄── END_STATIC_PRIV_ADDR = BEG_DYN_PRIV_ADDR
  ┌─────────────────────┐
  │  动态特权槽 N       │     第一个动态槽
  ├─────────────────────┤
  │       ...           │
  └─────────────────────┘ ◄── END_DYN_PRIV_ADDR (= END_PRIV_ADDR)
```

**关键性质：**

1. **与静态区域无缝衔接**：`BEG_DYN_PRIV_ADDR == END_STATIC_PRIV_ADDR`，特权表在内存中连续排列，静态槽结束后紧接着就是动态槽
2. **动态分配的起点**：当 `priv_id == NULL_PRIV_ID` 时，内核从此位置开始线性扫描，寻找空闲的动态槽（见 `system.c:285`）
3. **静态与动态的边界**：任何 `sp >= BEG_DYN_PRIV_ADDR && sp < END_DYN_PRIV_ADDR` 的指针都属于动态区域

#### 3.5.1 动态特权槽

**定义：**

动态特权槽（Dynamic Privilege Slots）是特权表中为运行时启动的系统服务预留的可分配位置。与静态槽不同，动态槽在系统启动时为空，仅在需要时分配给新的系统进程。

**关键特点：**

1. **按需分配**：系统运行期间，当新系统进程需要特权结构时，内核在动态区域中线性扫描寻找空闲槽（`s_proc_nr == NONE`），见 `system.c:284-287`
2. **数量有限**：动态槽总数为 `NR_SYS_PROCS - NR_STATIC_PRIV_IDS`，分配完返回 `ENOSPC`
3. **可回收**：当系统进程终止时，其动态槽被释放（`s_proc_nr` 重置为 `NONE`），可供后续分配
4. **身份由 PM 决定**：PM 通过 `SYS_PRIVCTL` 系统调用请求内核分配特权，若调用方未指定静态 ID（`DYN_PRIV_ID` 标志），内核即分配动态槽（`do_privctl.c:91`）

**与静态特权槽的对比：**

| 特征 | 静态特权槽 | 动态特权槽 |
|------|-----------|-----------|
| 分配时机 | 编译时/启动时 | 运行时按需 |
| 对应进程 | 内核任务 + 启动服务 | 后续启动的系统服务 |
| 槽索引 | 固定，与进程号绑定 | 动态，首次可用 |
| 回收 | 不回收 | 进程终止后可回收 |
| ID 标识 | `0 <= id < NR_STATIC_PRIV_IDS` | `NR_STATIC_PRIV_IDS <= id < NR_SYS_PROCS` |

**分配流程（`system.c:278-302`）：**

```
get_priv(rp, priv_id)
    │
    ├── priv_id == NULL_PRIV_ID ?
    │       │
    │       ├── 是 → 线性扫描动态区域
    │       │       for sp in BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR
    │       │           if sp->s_proc_nr == NONE → 找到空闲槽
    │       │       找不到 → ENOSPC
    │       │
    │       └── 否 → 分配指定静态槽
    │               检查 is_static_priv_id(priv_id)
    │               检查 priv[priv_id].s_proc_nr == NONE
    │
    └── 分配成功 → 设置 rp->p_priv = sp
                    设置 sp->s_proc_nr = proc_nr(rp)
```

### 3.6 END_DYN_PRIV_ADDR

**定义**（`kernel/priv.h:77`）：

```c
#define END_DYN_PRIV_ADDR          END_PRIV_ADDR
```

`END_DYN_PRIV_ADDR` 指向整个特权表的末尾（past-the-end）。由于动态特权区域从静态区域末尾一直延伸到特权表末尾，它等于 `END_PRIV_ADDR`，即 `&priv[NR_SYS_PROCS]`。

**在特权表中的位置：**

```
priv[] 特权表（共 NR_SYS_PROCS = 64 项）：
┌─────────────────────┐ ◄── BEG_PRIV_ADDR (= &priv[0])
│  priv[0]            │     静态特权槽 0
├─────────────────────┤
│       ...           │     静态特权槽 1..N-1
├─────────────────────┤
│  priv[N-1]          │     静态特权槽 N-1 (N = NR_STATIC_PRIV_IDS)
├─────────────────────┤ ◄── END_STATIC_PRIV_ADDR = BEG_DYN_PRIV_ADDR
│  priv[N]            │     动态特权槽 0
├─────────────────────┤
│       ...           │     动态特权槽 1..M-1
├─────────────────────┤
│  priv[NR_SYS_PROCS-1]│    动态特权槽 M-1
└─────────────────────┘ ◄── END_DYN_PRIV_ADDR = END_PRIV_ADDR
```

**关键性质：**

1. **整个特权表的边界**：`END_DYN_PRIV_ADDR == END_PRIV_ADDR`，动态区域覆盖了静态区域之后的所有剩余空间
2. **动态槽数量**：`END_DYN_PRIV_ADDR - BEG_DYN_PRIV_ADDR == NR_SYS_PROCS - NR_STATIC_PRIV_IDS`，即总系统进程数减去静态槽数
3. **扫描上界**：`get_priv()` 函数在动态区域扫描时，以此作为循环上界（`system.c:285`）

---

## 4. 特权访问宏

本节分析 `priv_addr`、`priv_id` 和 `priv` 三个核心访问宏。它们构成了从特权 ID → 特权指针 → 特权字段的双向访问路径，是特权系统的基础操作接口。

### 4.1 priv_addr 宏

**定义**（`kernel/priv.h:79`）：

```c
#define priv_addr(i)      (ppriv_addr)[(i)]
```

`priv_addr(i)` 通过特权 ID `i` 索引 `ppriv_addr` 指针数组，返回指向 `priv[i]` 的指针。它将整数索引转换为 `struct priv *` 指针。

**为什么不直接用 `&priv[i]`？** C 语言中 `&priv[i]` 需要计算 `priv + i * sizeof(struct priv)`，涉及乘法运算。而 `ppriv_addr[i]` 只需要一次数组索引（基地址 + 偏移），避免了乘法，是典型的空间换时间优化。

#### 4.1.1 特权 ID 到特权指针

从特权 ID 获取特权指针的过程：

```
特权 ID (sys_id_t) ──→ ppriv_addr[id] ──→ struct priv *
     3                  ppriv_addr[3]      &priv[3]
```

这是特权系统中最基础的查找操作。内核在多处使用此转换：
- `get_priv()` 中通过 `priv_id` 获取目标槽指针
- `system_init()` 中遍历初始化特权结构的定时器
- fork 时为子进程分配特权后，通过 `priv_addr(priv_id)` 访问新分配的特权结构

#### 4.1.2 ppriv_addr 数组

`ppriv_addr` 是一个 `struct priv *` 指针数组，声明为 `ppriv_addr[NR_SYS_PROCS]`。每个元素 `ppriv_addr[i]` 指向 `priv[i]`，在系统初始化时设置。

**作用：**

1. **加速索引**：避免 `&priv[i]` 的乘法开销，直接通过指针数组索引获取地址
2. **统一接口**：`priv_addr(i)` 宏封装了此数组访问，对外提供统一的"ID → 指针"转换
3. **可扩展性**：若未来特权表不再是简单数组（如分段存储），只需修改 `ppriv_addr` 的初始化逻辑，无需修改 `priv_addr` 宏

### 4.2 priv_id 宏

**定义**（`kernel/priv.h:80`）：

```c
#define priv_id(rp)	  ((rp)->p_priv->s_id)
```

`priv_id(rp)` 从进程结构体出发，通过 `p_priv` 指针访问特权结构，再读取 `s_id` 字段，获取该进程的特权 ID。这是 `priv_addr(i)` 的逆操作——从进程获取其特权索引。

#### 4.2.1 获取进程特权 ID

获取进程特权 ID 的路径：

```
struct proc *rp ──→ rp->p_priv ──→ p_priv->s_id
       │               │                │
       进程结构    特权结构指针      特权 ID (sys_id_t)
```

`priv_id()` 在 IPC 权限检查中广泛使用。例如 `nr_to_id(nr)` 内部通过 `priv(proc_addr(nr))->s_id` 获取进程号对应的特权 ID，而 `may_send_to()` 又通过此 ID 在位图中查找权限。

### 4.3 priv 宏

**定义**（`kernel/priv.h:81`）：

```c
#define priv(rp)	  ((rp)->p_priv)
```

`priv(rp)` 是最基础的特权访问宏，返回进程 `rp` 的特权结构指针 `p_priv`。它是 `priv_id()` 的底层依赖，也是所有访问特权字段的入口。

**访问链示意：**

```
priv(rp)                → rp->p_priv
priv(rp)->s_flags       → 特权标志
priv(rp)->s_ipc_to      → IPC 目标掩码
priv(rp)->s_k_call_mask → 内核调用掩码
priv(rp)->s_id          → 特权 ID（等同于 priv_id(rp)）
```

#### 4.3.1 获取进程特权指针

每个进程的 `p_priv` 字段指向其特权结构。对于系统进程，`p_priv` 指向 `priv[]` 表中对应的槽位；对于用户进程，`p_priv` 统一指向 `priv[USER_PRIV_ID]`——所有用户进程共享同一个特权结构。

```
进程类型与 p_priv 指向：
┌─────────────────┐     ┌──────────────────┐
│ 系统进程 (PM)   │────→│ priv[PM_PRIV_ID] │   独占特权槽
├─────────────────┤     ├──────────────────┤
│ 系统进程 (FS)   │────→│ priv[FS_PRIV_ID] │   独占特权槽
├─────────────────┤     ├──────────────────┤
│ 用户进程 A      │──┐  │       ...        │
├─────────────────┤  │  ├──────────────────┤
│ 用户进程 B      │──┼─→│ priv[USER_PRIV_ID]│  共享特权槽
└─────────────────┘  │  └──────────────────┘
                     └──────────────────────┘
```

#### 4.3.2 fork 时的使用

在 `do_fork()` 中，`priv()` 宏用于检查父进程是否为系统进程，从而决定子进程的特权处理策略：

```c
// do_fork 中的特权检查（简化）
if (priv(rpp)->s_flags & SYS_PROC) {
    // 父进程是系统进程 → 子进程降级为用户进程
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;  // 等待 PM 设置特权
} else {
    // 父进程是用户进程 → 子进程同样使用 USER_PRIV_ID
    rpc->p_priv = priv_addr(USER_PRIV_ID);
}
```

**要点：**

- fork 时通过 `priv(rpp)->s_flags & SYS_PROC` 判断父进程特权级别
- 系统进程的子进程不能继承系统特权（安全原则），降级为用户特权
- 设置 `RTS_NO_PRIV` 标志后，子进程不会立即运行，需等待 PM 通过 `SYS_PRIVCTL` 分配特权

---

## 5. ID 与进程号转换宏

本节分析 `id_to_nr` 和 `nr_to_id` 两个转换宏。它们在特权 ID（`sys_id_t`）与进程号（`proc_nr_t`）之间建立双向映射，是 IPC 权限检查的关键桥梁。

### 5.1 id_to_nr 宏

**定义**（`kernel/priv.h:83`）：

```c
#define id_to_nr(id)	priv_addr(id)->s_proc_nr
```

`id_to_nr(id)` 通过特权 ID 获取关联的进程号。它先调用 `priv_addr(id)` 得到特权结构指针，再读取 `s_proc_nr` 字段。

#### 5.1.1 特权 ID 到进程号

转换路径：

```
sys_id_t (id) ──→ priv_addr(id) ──→ s_proc_nr ──→ proc_nr_t
     3            &priv[3]            -5 (PM)       PM_PROC_NR
```

此转换在 IPC 中用于将位图中的特权 ID 还原为进程号。例如，当遍历 `s_ipc_to` 位图找到允许发送的目标特权 ID 后，需要调用 `id_to_nr()` 获取目标进程号。

### 5.2 nr_to_id 宏

**定义**（`kernel/priv.h:84`）：

```c
#define nr_to_id(nr)    priv(proc_addr(nr))->s_id
```

`nr_to_id(nr)` 通过进程号获取特权 ID。它先调用 `proc_addr(nr)` 得到进程结构指针，再通过 `priv()` 宏访问特权结构的 `s_id` 字段。

#### 5.2.1 进程号到特权 ID

转换路径：

```
proc_nr_t (nr) ──→ proc_addr(nr) ──→ priv(rp) ──→ s_id ──→ sys_id_t
    PM_PROC_NR (-5)   &proc[PM]      &priv[PM]    4          PM 的特权 ID
```

此转换是 `id_to_nr` 的逆操作，在 IPC 权限检查中至关重要。`may_send_to()` 宏内部调用 `nr_to_id(nr)` 将目标进程号转换为特权 ID，然后在 `s_ipc_to` 位图中检查该 ID 对应的位。

**注意：对于用户进程**，由于所有用户进程共享 `priv[USER_PRIV_ID]`，`nr_to_id()` 对任意用户进程号都返回 `USER_PRIV_ID`。

---

## 6. 发送权限检查宏

本节分析 `may_send_to` 和 `may_asynsend_to` 两个权限检查宏。它们基于 `s_ipc_to` 位图实现 IPC 发送权限控制，是 Minix3 微内核安全模型的核心。

### 6.1 may_send_to 宏

**定义**（`kernel/priv.h:86`）：

```c
#define may_send_to(rp, nr) (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))
```

`may_send_to(rp, nr)` 检查进程 `rp` 是否允许向进程号为 `nr` 的目标发送同步消息。它将目标进程号转换为特权 ID，然后在发送者的 `s_ipc_to` 位图中检查对应位是否置位。

#### 6.1.1 检查发送权限

权限检查流程：

```
may_send_to(caller_ptr, dst_nr)
    │
    ├── 1. nr_to_id(dst_nr)    → 获取目标的特权 ID
    │      priv(proc_addr(dst_nr))->s_id
    │
    ├── 2. priv(caller_ptr)    → 获取发送者的特权结构
    │      caller_ptr->p_priv
    │
    └── 3. get_sys_bit(s_ipc_to, target_id)  → 检查位图
           s_ipc_to 位图中 target_id 位是否置位
               │
               ├── 置位 → 允许发送 (返回非零)
               └── 未置位 → ECALLDENIED
```

在 `proc.c:536` 中，`send()` 函数在发送前调用此宏：若返回假，则直接返回 `ECALLDENIED`，拒绝发送。

#### 6.1.2 s_ipc_to 掩码

`s_ipc_to` 是 `sys_map_t` 类型的位图，声明在 `struct priv` 中。位图中的每一位对应一个特权 ID，若置位则表示允许向该特权 ID 关联的进程发送 IPC 消息。

**位图结构：**

```
s_ipc_to (sys_map_t)：
  位 [0]     → 特权 ID 0 (KERNEL)     → 允许/禁止
  位 [1]     → 特权 ID 1 (CLOCK)      → 允许/禁止
  ...
  位 [NR_SYS_PROCS-1] → 特权 ID N-1   → 允许/禁止
```

**典型配置：**

| 进程类型 | s_ipc_to | 说明 |
|---------|----------|------|
| 内核任务 | 全置位 | 可向任何进程发送 |
| 系统服务 (PM/FS) | 选择性置位 | 仅允许向特定服务发送 |
| 用户进程 | 仅 PM/FS 置位 | 只能向 PM 和 FS 发送 |

**底层位操作**：`get_sys_bit()` 定义在 `include/minix/bitmap.h:16`，展开为 `MAP_CHUNK(map,bit) & (1 << CHUNK_OFFSET(bit))`，即先定位到正确的 `bitchunk_t` 字，再测试对应的位。

### 6.2 may_asynsend_to 宏

**定义**（`kernel/priv.h:87`）：

```c
#define may_asynsend_to(rp, nr) (may_send_to(rp, nr) || (rp)->p_nr == nr)
```

`may_asynsend_to(rp, nr)` 在 `may_send_to` 的基础上增加了一个条件：若发送者的进程号 `rp->p_nr` 等于目标进程号 `nr`，也允许异步发送。这意味着**进程总是可以向自己发送异步消息**。

#### 6.2.1 异步发送权限

异步发送权限检查比同步发送更宽松，额外允许自发送：

```
may_asynsend_to(rp, nr)
    │
    ├── may_send_to(rp, nr)   → 位图中允许？
    │       │
    │       ├── 是 → 允许
    │       └── 否 → 继续检查
    │
    └── rp->p_nr == nr        → 目标是自己？
            │
            ├── 是 → 允许（自通知）
            └── 否 → ECALLDENIED
```

**自发送的设计意图**：进程给自己发异步通知是一种常见模式，例如时钟中断处理函数向所属进程发送通知。允许自发送避免了在 `s_ipc_to` 位图中为自身额外置位的需要。此逻辑在 `proc.c:1266`（异步发送入口）和 `proc.c:1413`（异步消息投递）中使用。

---

## 7. 特权表声明

本节分析 `priv[]` 和 `ppriv_addr[]` 两个全局数组的声明。它们是特权系统的核心数据结构，前者存储所有特权结构，后者提供快速索引指针。

### 7.1 priv 数组声明

**声明**（`kernel/priv.h:94`）：

```c
EXTERN struct priv priv[NR_SYS_PROCS];		/* system properties table */
```

`priv[]` 是全局特权结构数组，大小为 `NR_SYS_PROCS`（默认 64，定义在 `include/minix/sys_config.h:9`）。数组中的每个元素是一个 `struct priv`，存储一个系统进程的特权信息。`EXTERN` 宏确保在头文件中为 `extern` 声明，在定义文件中为实际定义。

#### 7.1.1 特权表大小

特权表大小由 `NR_SYS_PROCS` 决定，默认值为 64（`_NR_SYS_PROCS`，定义在 `sys_config.h:9`）。该值必须大于等于 `NR_BOOT_PROCS`（启动进程数），否则触发编译时错误。

**大小计算：**

```
特权表总大小 = NR_SYS_PROCS × sizeof(struct priv)
静态槽大小  = NR_STATIC_PRIV_IDS × sizeof(struct priv) = NR_BOOT_PROCS × sizeof(struct priv)
动态槽大小  = (NR_SYS_PROCS - NR_BOOT_PROCS) × sizeof(struct priv)
```

`NR_BOOT_PROCS` 的计算（`include/minix/param.h:9`）：

```c
#define NR_BOOT_PROCS   (NR_TASKS + LAST_SPECIAL_PROC_NR + 1)
```

即内核任务数加上特殊系统进程数加一。

### 7.2 ppriv_addr 数组声明

**声明**（`kernel/priv.h:95`）：

```c
EXTERN struct priv *ppriv_addr[NR_SYS_PROCS];	/* direct slot pointers */
```

`ppriv_addr[]` 是全局指针数组，每个元素 `ppriv_addr[i]` 存储指向 `priv[i]` 的指针。数组大小与 `priv[]` 相同，均为 `NR_SYS_PROCS`。

#### 7.2.1 直接槽指针

直接槽指针提供了一种从特权 ID 直接获取特权结构指针的机制。在系统初始化时，每个 `ppriv_addr[i]` 被设置为 `&priv[i]`。此后，`priv_addr(i)` 宏只需一次数组索引即可完成 ID → 指针的转换。

```
初始化：
ppriv_addr[0] = &priv[0]
ppriv_addr[1] = &priv[1]
...
ppriv_addr[NR_SYS_PROCS-1] = &priv[NR_SYS_PROCS-1]

使用：
priv_addr(3) → ppriv_addr[3] → &priv[3]
```

#### 7.2.2 性能优化

**对比：直接索引 vs 指针数组**

```c
// 方式 1：直接数组索引（需要乘法）
struct priv *sp = &priv[i];
// 编译为: base_addr + i * sizeof(struct priv)
// sizeof(struct priv) 通常 > 100 字节，乘法开销不可忽略

// 方式 2：指针数组索引（仅需加法）
struct priv *sp = ppriv_addr[i];
// 编译为: ppriv_addr_base + i * sizeof(struct priv *)
// sizeof(struct priv *) = 4/8 字节，乘法可优化为移位
```

在 Minix3 内核中，`priv_addr()` 被频繁调用（IPC 路径、权限检查、定时器遍历等），指针数组的优化具有实际意义。注释（`priv.h:91-93`）也明确说明了这一点：

> The pointers allow faster access because now a process entry can be found by indexing the psys_addr array, while accessing an element i requires a multiplication with sizeof(struct sys) to determine the address.

---

## 8. 编译时检查

本节分析特权表大小的编译时安全检查。Minix3 使用 `#error` 预处理指令确保 `NR_SYS_PROCS` 足够容纳所有启动进程，防止运行时特权槽溢出。

### 8.1 NR_BOOT_PROCS 检查

**代码**（`kernel/priv.h:101-103`）：

```c
#if (NR_BOOT_PROCS > NR_SYS_PROCS)
#error NR_SYS_PROCS must be larger than NR_BOOT_PROCS
#endif
```

此检查确保特权表 `priv[NR_SYS_PROCS]` 的容量足以容纳所有启动进程。`NR_BOOT_PROCS` 是引导镜像中的进程数（包括内核任务和基本系统服务），而 `NR_SYS_PROCS` 是特权表的总大小。

#### 8.1.1 启动进程数量限制

**限制的含义：**

- `NR_BOOT_PROCS`（默认约 9-12）由引导镜像配置决定，代表系统启动时必须分配静态特权槽的进程数
- `NR_SYS_PROCS`（默认 64）是特权表的硬上限，同时容纳静态槽和动态槽
- 若 `NR_BOOT_PROCS > NR_SYS_PROCS`，启动进程都无法全部获得特权槽，系统无法正常引导

**隐含约束：**

```
NR_BOOT_PROCS ≤ NR_SYS_PROCS
     │              │
     │              └── 特权表总容量 (64)
     └── 静态槽数量
                      │
     动态槽数量 = NR_SYS_PROCS - NR_BOOT_PROCS
     (供运行时新系统服务使用)
```

若动态槽数量为 0（即 `NR_BOOT_PROCS == NR_SYS_PROCS`），系统将无法在运行时启动任何新的系统服务，`get_priv()` 在动态区域扫描时会返回 `ENOSPC`。

#### 8.1.2 编译时错误

**编译时错误的作用：**

`#error` 是 C 预处理器指令，在编译阶段即产生错误并终止编译。这比运行时检查更安全——如果特权表配置不一致，错误会在编译时暴露，而不是在系统运行时因特权槽越界导致未定义行为。

**在 Rust 中的对应方式：**

Rust 没有 `#error` 的直接等价，但可以通过以下方式实现编译时检查：

```rust
// 方式 1：const assert（Rust 1.73+）
const _: () = assert!(NR_BOOT_PROCS <= NR_SYS_PROCS);

// 方式 2：类型级别断言
const fn check_boot_procs() {
    if NR_BOOT_PROCS > NR_SYS_PROCS {
        panic!("NR_SYS_PROCS must be >= NR_BOOT_PROCS");
    }
}
const _: () = check_boot_procs();
```

---

## 9. Rust 设计决策

本节讨论如何用 Rust 的类型系统重新表达特权访问宏。核心思路：将 C 宏的隐式约定转化为 Rust 的显式类型约束，在编译时而非运行时捕获越界和类型混淆错误。

**C 宏的问题：**

| C 宏 | 问题 |
|------|------|
| `priv_addr(i)` | `i` 无类型约束，可传入任意整数 |
| `priv_id(rp)` | 依赖 `rp->p_priv` 非空，无编译时保证 |
| `priv(rp)` | 返回裸指针，无生命周期 |
| `id_to_nr(id)` | 空槽返回 `NONE`，但无法区分"空"与"进程号=NONE" |
| `may_send_to(rp, nr)` | 位图越界为 UB |

**Rust 设计原则：**

1. `SysId` 和 `ProcNr` 使用 newtype 区分，避免混淆
2. 特权表通过 `PrivTable` 封装，所有访问经过边界检查
3. `KProcess` 的 `p_priv` 使用 `Option<&Priv>` 表达"可能无特权"
4. 权限检查返回 `bool`，位图越界返回 `false` 而非 UB

### 9.1 特权表结构

**C 设计回顾：**

C 中 `priv[]` 是固定大小数组，`ppriv_addr[]` 是指针数组用于加速索引。两者作为全局变量直接访问。

**Rust 设计：**

```rust
/// 特权表
///
/// 封装 `priv[]` 和 `ppriv_addr[]`，提供类型安全的访问接口。
/// 不再暴露裸指针，所有访问经过边界检查。
pub struct PrivTable {
    /// 特权结构数组（对应 C 的 priv[NR_SYS_PROCS]）
    slots: [Priv; NR_SYS_PROCS],
}
```

**关键决策：**

1. **取消 `ppriv_addr[]`**：Rust 中 `&slots[i]` 的索引开销远小于 C（编译器可优化为直接偏移），指针数组优化在 Rust 中不再必要
2. **封装而非全局**：`PrivTable` 作为内核结构的字段，通过 `&self` / `&mut self` 控制访问，而非 `EXTERN` 全局变量
3. **静态/动态分区用方法表达**：`static_slots()` / `dynamic_slots()` 返回切片迭代器，替代 C 的地址范围宏

### 9.2 静态与动态特权

**C 中的静态/动态分区：**

C 通过 `BEG_STATIC_PRIV_ADDR` / `END_STATIC_PRIV_ADDR` 等地址范围宏划分静态与动态区域，用指针算术遍历。

**Rust 中的设计：**

```rust
impl PrivTable {
    /// 静态特权槽切片（对应 BEG_STATIC_PRIV_ADDR..END_STATIC_PRIV_ADDR）
    pub fn static_slots(&self) -> &[Priv] {
        &self.slots[..NR_BOOT_PROCS]
    }

    /// 动态特权槽切片（对应 BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR）
    pub fn dynamic_slots(&self) -> &[Priv] {
        &self.slots[NR_BOOT_PROCS..]
    }

    /// 动态区域分配（对应 get_priv 中 priv_id==NULL_PRIV_ID 的分支）
    pub fn alloc_dynamic(&mut self, proc_nr: ProcNr) -> Result<SysId, Errno> {
        for (i, slot) in self.dynamic_slots_mut().iter_mut().enumerate() {
            if slot.s_proc_nr == PROC_NR_NONE {
                let id = SysId(NR_BOOT_PROCS as u8 + i as u8);
                slot.s_proc_nr = proc_nr;
                return Ok(id);
            }
        }
        Err(Errno::ENOSPC)
    }
}
```

**改进点：**

1. 用切片迭代替代指针算术，消除越界风险
2. `alloc_dynamic` 封装了线性扫描逻辑，返回 `Result` 而非裸 `ENOSPC` 错误码
3. `is_static_priv_id(id)` 可通过 `id.0 < NR_BOOT_PROCS` 直接判断

### 9.3 权限检查

**C 中的权限检查问题：**

```c
#define may_send_to(rp, nr) (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))
```

- `nr_to_id(nr)` 可能返回越界索引，`get_sys_bit` 的位图越界为 UB
- `priv(rp)` 解引用裸指针，`p_priv` 可能为空
- 返回 `int`，调用方可能误用为错误码

**Rust 设计：**

```rust
impl PrivTable {
    /// 检查发送权限（对应 may_send_to）
    ///
    /// 返回 `false` 的情况：
    /// - 位图中对应位未置位（无权限）
    /// - 特权 ID 越界（安全降级，而非 UB）
    /// - 进程无特权结构（用户进程的共享特权）
    pub fn may_send_to(&self, caller: &KProcess, target_nr: ProcNr) -> bool {
        let caller_priv = match caller.priv_ref() {
            Some(p) => p,
            None => return false,
        };
        let target_id = match self.nr_to_id(target_nr) {
            Some(id) => id,
            None => return false,
        };
        caller_priv.s_ipc_to.get(target_id.0 as usize)
    }

    /// 检查异步发送权限（对应 may_asynsend_to）
    pub fn may_asynsend_to(&self, caller: &KProcess, target_nr: ProcNr) -> bool {
        self.may_send_to(caller, target_nr) || caller.p_nr == target_nr
    }
}
```

**安全性提升：**

1. **无 UB**：位图 `get()` 方法内建越界检查，越界返回 `false` 而非 UB
2. **显式空检查**：`priv_ref()` 返回 `Option<&Priv>`，强制处理无特权的情况
3. **返回 `bool`**：不与错误码混淆，语义清晰
4. **利用已有 `Bitmap` 类型**：`s_ipc_to` 使用 `minix_types::Bitmap`，位操作已充分测试

---

## 10. 实现

本节给出特权表和访问方法的 Rust 实现。代码位于 `os/kernel/src/proc/priv_table.rs`（新增模块）。

### 10.1 特权表定义

```rust
//! os/kernel/src/proc/priv_table.rs
//!
//! 特权表与特权访问方法
//!
//! 对应 Minix3 kernel/priv.h 中的宏和数据结构。
//! 将 C 宏的隐式约定转化为 Rust 的显式类型约束。

use minix_types::{Bitmap, Endpoint, ProcNr as ProcNrType};
use core::sync::atomic::{AtomicU32, AtomicI32, Ordering};

/// 系统进程最大数量（对应 NR_SYS_PROCS = 64）
pub const NR_SYS_PROCS: usize = 64;

/// 启动进程数量（对应 NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1）
pub const NR_BOOT_PROCS: usize = 5 + 11 + 1; // = 17

/// 用户特权 ID（所有用户进程共享的特权槽索引）
pub const USER_PRIV_ID: u8 = 0;

/// 编译时检查：NR_BOOT_PROCS 不能超过 NR_SYS_PROCS
const _: () = assert!(NR_BOOT_PROCS <= NR_SYS_PROCS);

/// 特权 ID 新类型
///
/// 对应 C 的 `sys_id_t`，使用 newtype 防止与 `ProcNr` 混淆。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SysId(pub u8);

impl SysId {
    /// 是否为静态特权 ID（对应 is_static_priv_id）
    pub const fn is_static(self) -> bool {
        (self.0 as usize) < NR_BOOT_PROCS
    }

    /// 是否为有效的特权 ID
    pub const fn is_valid(self) -> bool {
        (self.0 as usize) < NR_SYS_PROCS
    }
}

/// 特权标志
#[derive(Debug, Clone, Copy, Default)]
pub struct PrivFlags(pub u16);

impl PrivFlags {
    pub const SYS_PROC: u16 = 0x001;
    pub const BILLABLE: u16 = 0x002;
    pub const PREEMPTIBLE: u16 = 0x004;
    pub const DYN_PRIV_ID: u16 = 0x010;

    pub fn contains(self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

/// 特权结构体
///
/// 对应 Minix3 的 `struct priv`。
/// 每个系统进程拥有独立的 Priv，所有用户进程共享 `USER_PRIV_ID` 对应的 Priv。
#[derive(Debug)]
pub struct Priv {
    /// 关联进程号（NONE 表示空闲）
    pub s_proc_nr: AtomicI32,
    /// 特权表索引
    pub s_id: SysId,
    /// 特权标志
    pub s_flags: PrivFlags,
    /// IPC 目标掩码（对应 s_ipc_to）
    pub s_ipc_to: Bitmap,
    /// 内核调用掩码（对应 s_k_call_mask）
    pub s_k_call_mask: Bitmap,
}

impl Priv {
    /// 创建空闲特权槽
    fn empty(id: SysId) -> Self {
        Self {
            s_proc_nr: AtomicI32::new(PROC_NR_NONE),
            s_id: id,
            s_flags: PrivFlags::default(),
            s_ipc_to: Bitmap::new(NR_SYS_PROCS),
            s_k_call_mask: Bitmap::new(NR_SYS_PROCS),
        }
    }

    /// 是否空闲（s_proc_nr == NONE）
    pub fn is_empty(&self) -> bool {
        self.s_proc_nr.load(Ordering::Acquire) == PROC_NR_NONE
    }

    /// 是否为系统进程
    pub fn is_sys_proc(&self) -> bool {
        self.s_flags.contains(PrivFlags::SYS_PROC)
    }
}

/// 进程号 NONE 常量
const PROC_NR_NONE: i32 = -1;

/// 特权表
///
/// 封装 `priv[]` 和 `ppriv_addr[]`，提供类型安全的访问接口。
/// 对应 C 中 BEG_PRIV_ADDR..END_PRIV_ADDR 的全局数组。
pub struct PrivTable {
    slots: [Priv; NR_SYS_PROCS],
}

impl PrivTable {
    /// 创建空的特权表
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|i| Priv::empty(SysId(i as u8))),
        }
    }

    /// 通过特权 ID 获取特权结构（对应 priv_addr(i)）
    ///
    /// 返回 None 表示 ID 越界。
    pub fn get(&self, id: SysId) -> Option<&Priv> {
        if id.is_valid() {
            Some(&self.slots[id.0 as usize])
        } else {
            None
        }
    }

    /// 通过特权 ID 获取特权结构可变引用
    pub fn get_mut(&mut self, id: SysId) -> Option<&mut Priv> {
        if id.is_valid() {
            Some(&mut self.slots[id.0 as usize])
        } else {
            None
        }
    }

    /// 静态特权槽切片（对应 BEG_STATIC_PRIV_ADDR..END_STATIC_PRIV_ADDR）
    pub fn static_slots(&self) -> &[Priv] {
        &self.slots[..NR_BOOT_PROCS]
    }

    /// 动态特权槽切片（对应 BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR）
    pub fn dynamic_slots(&self) -> &[Priv] {
        &self.slots[NR_BOOT_PROCS..]
    }

    /// 动态区域分配（对应 get_priv 中 priv_id==NULL_PRIV_ID 的分支）
    ///
    /// 在动态区域中线性扫描空闲槽，返回分配的 SysId。
    /// 若无空闲槽返回 ENOSPC。
    pub fn alloc_dynamic(&mut self, proc_nr: i32) -> Result<SysId, i32> {
        for (i, slot) in self.slots[NR_BOOT_PROCS..].iter_mut().enumerate() {
            if slot.is_empty() {
                let id = SysId((NR_BOOT_PROCS + i) as u8);
                slot.s_proc_nr.store(proc_nr, Ordering::Release);
                return Ok(id);
            }
        }
        Err(12) // ENOSPC
    }

    /// 静态区域分配（对应 get_priv 中指定 priv_id 的分支）
    pub fn alloc_static(&mut self, id: SysId, proc_nr: i32) -> Result<(), i32> {
        if !id.is_static() {
            return Err(22); // EINVAL
        }
        let slot = &mut self.slots[id.0 as usize];
        if !slot.is_empty() {
            return Err(16); // EBUSY
        }
        slot.s_proc_nr.store(proc_nr, Ordering::Release);
        Ok(())
    }

    /// 特权 ID → 进程号（对应 id_to_nr）
    pub fn id_to_nr(&self, id: SysId) -> Option<i32> {
        self.get(id).map(|p| p.s_proc_nr.load(Ordering::Acquire))
    }

    /// 进程号 → 特权 ID（对应 nr_to_id）
    ///
    /// 遍历特权表查找 s_proc_nr == nr 的槽。
    /// 注意：C 中通过 proc_addr + priv 间接访问，这里简化为线性查找。
    pub fn nr_to_id(&self, nr: i32) -> Option<SysId> {
        self.slots.iter().find(|p| {
            p.s_proc_nr.load(Ordering::Acquire) == nr
        }).map(|p| p.s_id)
    }

    /// 检查发送权限（对应 may_send_to）
    pub fn may_send_to(&self, caller_priv: &Priv, target_nr: i32) -> bool {
        let target_id = match self.nr_to_id(target_nr) {
            Some(id) => id,
            None => return false,
        };
        caller_priv.s_ipc_to.get(target_id.0 as usize)
    }

    /// 检查异步发送权限（对应 may_asynsend_to）
    pub fn may_asynsend_to(&self, caller_priv: &Priv, caller_nr: i32, target_nr: i32) -> bool {
        self.may_send_to(caller_priv, target_nr) || caller_nr == target_nr
    }

    /// 释放特权槽（进程终止时调用）
    pub fn free(&mut self, id: SysId) {
        if let Some(slot) = self.get_mut(id) {
            slot.s_proc_nr.store(PROC_NR_NONE, Ordering::Release);
            slot.s_flags = PrivFlags::default();
            slot.s_ipc_to.clear();
            slot.s_k_call_mask.clear();
        }
    }
}

impl Default for PrivTable {
    fn default() -> Self {
        Self::new()
    }
}
```

### 10.2 访问方法

特权访问方法已在 10.1 的 `PrivTable` 中实现。这里补充 `KProcess` 侧的便捷访问方法，对应 C 中 `priv(rp)` 和 `priv_id(rp)` 宏。

```rust
//! os/kernel/src/proc.rs 中 KProcess 的特权访问扩展

impl KProcess {
    /// 获取特权 ID（对应 priv_id(rp)）
    ///
    /// 返回 None 表示进程尚未分配特权。
    pub fn priv_id(&self) -> Option<SysId> {
        self.p_priv_id
    }

    /// 获取特权标志
    ///
    /// 便捷方法，避免调用方需要持有 PrivTable 引用。
    pub fn is_sys_proc(&self, table: &PrivTable) -> bool {
        self.priv_id()
            .and_then(|id| table.get(id))
            .map(|p| p.is_sys_proc())
            .unwrap_or(false)
    }

    /// fork 时的特权降级检查
    ///
    /// 对应 do_fork 中 `priv(rpp)->s_flags & SYS_PROC` 的检查。
    /// 若父进程是系统进程，子进程应降级为 USER_PRIV_ID。
    pub fn should_downgrade_on_fork(&self, table: &PrivTable) -> bool {
        self.is_sys_proc(table)
    }
}
```

**设计说明：**

- `KProcess` 存储 `p_priv_id: Option<SysId>` 而非 `p_priv: *mut Priv`，避免裸指针和生命周期问题
- 特权结构的访问必须通过 `PrivTable` 引用，由调用方保证表的生命周期
- `should_downgrade_on_fork()` 封装了 fork 特权降级判断，对应 C 中 `priv(rpp)->s_flags & SYS_PROC`

### 10.3 单元测试

单元测试已实现在 `os/kernel/src/priv_table.rs` 的 `tests` 模块中，共 18 个测试用例，全部通过。

**测试覆盖：**

| 测试类别 | 测试名 | 验证内容 |
|---------|--------|---------|
| **SysId** | `test_sys_id_is_static` | 静态/动态 ID 边界 |
| | `test_sys_id_is_valid` | 有效性检查 |
| **PrivTable 基础** | `test_priv_table_new_all_empty` | 初始状态全空 |
| | `test_priv_table_static_dynamic_split` | 静态/动态分区大小 |
| | `test_priv_table_get_by_id` | ID 索引访问 |
| **静态分配** | `test_alloc_static` | 正常分配 + 重复分配 EBUSY |
| | `test_alloc_static_invalid` | 动态 ID 禁止静态分配 EINVAL |
| **动态分配** | `test_alloc_dynamic` | 正常分配 + id_to_nr 一致性 |
| | `test_alloc_dynamic_exhausted` | 槽耗尽 ENOSPC |
| **ID 转换** | `test_id_to_nr_and_nr_to_id` | 双向转换正确性 |
| **权限检查** | `test_may_send_to` | 授权/未授权/不存在目标 |
| | `test_may_asynsend_to_self` | 自发送允许 |
| | `test_may_asynsend_to_not_self` | 非自发送需授权 |
| **释放** | `test_free_slot` | 释放后槽空闲 + 位图清零 |
| | `test_free_and_realloc` | 释放后可再分配 |
| **标志** | `test_priv_flags` | 标志 insert/remove/contains |
| | `test_priv_is_sys_proc` | SYS_PROC 判断 |
| **编译时** | `test_compile_time_check` | NR_BOOT_PROCS <= NR_SYS_PROCS |

---

## 11. 参见

- [09-priv-struct](09-priv-struct.md) - 特权结构体
- [18-do-fork-priv](18-do-fork-priv.md) - fork 特权处理
