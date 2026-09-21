# servers/pm/glo.h 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/glo.h`
> **核心功能**: PM（进程管理器）全局变量声明
> **所属模块**: PM（Process Manager）

---

## 一、文件概述

### 1.1 功能说明（是什么）

`glo.h` 集中声明 PM 所有全局变量，使用 `EXTERN` 宏统一管理变量的定义和声明。这是 PM 模块的全局状态定义文件。

**生活类比**：想象一个学校的公告栏：
- 公告栏上贴着各种通知（全局变量）。
- 所有人都能看到公告栏的内容（全局访问）。
- 只有管理员能修改公告栏（受控修改）。
- `glo.h` 就是 PM 这个"学校"的"公告栏"。

### 1.2 设计原因（为什么）

**为什么需要集中声明全局变量？**

1. **单一定义规则**：C 语言要求变量只能定义一次，但可以声明多次。
2. **避免重复**：不需要在每个 `.c` 文件中重复声明。
3. **统一管理**：所有全局变量在一处声明，便于维护。
4. **编译优化**：编译器可以更好地优化全局变量的访问。

**EXTERN 宏机制**：

```
EXTERN 宏工作原理:
┌─────────────────────────────────────────────────────────────┐
│ 在 table.c 中:                                               │
│   #define _TABLE                                             │
│   #include "glo.h"  → 展开为: int procs_in_use; (定义)       │
├─────────────────────────────────────────────────────────────┤
│ 在其他文件中:                                                 │
│   #include "glo.h"  → 展开为: extern int procs_in_use; (声明)│
└─────────────────────────────────────────────────────────────┘
```

### 1.3 应用场景（什么情景使用）

| 场景 | 说明 |
|------|------|
| 添加全局变量 | 在 `glo.h` 中添加 `EXTERN` 声明 |
| 访问全局变量 | 包含 `glo.h` 后直接使用 |
| 初始化变量 | 在 `table.c` 中初始化 |

---

## 二、逐行详细讲解

### 2.1 EXTERN 宏定义

```c
/* EXTERN should be extern except in table.c */
#ifdef _TABLE
#undef EXTERN
#define EXTERN
#endif
```

**逐行解释**：

- **第1行**：`/* EXTERN should be extern except in table.c */`
  - 注释翻译：EXTERN 应该是 extern，除了在 table.c 中。
  - 设计思路：解释 EXTERN 宏的行为。

- **第2行**：`#ifdef _TABLE`
  - 检查是否定义了 `_TABLE` 宏。
  - 只有 `table.c` 会定义这个宏。

- **第3行**：`#undef EXTERN`
  - 取消已有的 EXTERN 定义。

- **第4行**：`#define EXTERN`
  - 将 EXTERN 定义为空。
  - 这样 `EXTERN int x;` 就变成 `int x;`（定义）。

- **第5行**：`#endif`
  - 条件编译结束。

**EXTERN 宏的定义位置**：

EXTERN 宏在 `minix/const.h` 中定义：

```c
#define EXTERN extern
```

**完整工作流程**：

```
编译流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. table.c 编译:                                             │
│    #define _TABLE                                            │
│    #include <minix/const.h>  → EXTERN = extern               │
│    #include "glo.h"                                          │
│      → #ifdef _TABLE (true)                                  │
│      → #undef EXTERN                                         │
│      → #define EXTERN (空)                                   │
│      → EXTERN int x; → int x; (定义，分配内存)               │
├─────────────────────────────────────────────────────────────┤
│ 2. main.c 编译:                                              │
│    #include <minix/const.h>  → EXTERN = extern               │
│    #include "glo.h"                                          │
│      → #ifdef _TABLE (false)                                 │
│      → EXTERN 保持 extern                                    │
│      → EXTERN int x; → extern int x; (声明，不分配内存)      │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.2 核心全局变量

```c
/* Global variables. */
EXTERN struct mproc *mp;	/* ptr to 'mproc' slot of current process */
EXTERN int procs_in_use;	/* how many processes are marked as IN_USE */
EXTERN char monitor_params[MULTIBOOT_PARAM_BUF_SIZE];
```

**逐行解释**：

- **第1行**：`/* Global variables. */`
  - 注释说明以下是全局变量。

- **第2行**：`EXTERN struct mproc *mp;`
  - 声明 `mp` 为指向 `struct mproc` 的指针。
  - 注释翻译：指向当前进程的 mproc 槽位的指针。
  - **内存大小**：4 字节（32 位）或 8 字节（64 位）。
  - **存储位置**：`.bss` 或 `.data` 段（全局/静态存储区）。

- **第3行**：`EXTERN int procs_in_use;`
  - 声明 `procs_in_use` 为整型变量。
  - 注释翻译：有多少进程被标记为 IN_USE。
  - **内存大小**：4 字节。
  - **存储位置**：`.bss` 或 `.data` 段。

- **第4行**：`EXTERN char monitor_params[MULTIBOOT_PARAM_BUF_SIZE];`
  - 声明 `monitor_params` 为字符数组。
  - 存储启动监视器传递的参数。
  - **内存大小**：`MULTIBOOT_PARAM_BUF_SIZE` 字节。
  - **存储位置**：`.bss` 段。

**内存布局图**：

```
全局变量内存布局:
┌─────────────────────────────────────────────────────────────┐
│ 地址         变量名           大小        内容               │
├─────────────────────────────────────────────────────────────┤
│ 0x1000      mp               4/8 字节    指针值             │
│ 0x1008      procs_in_use     4 字节      整数值             │
│ 0x100C      monitor_params   4096 字节   字符串数组         │
└─────────────────────────────────────────────────────────────┘

mp 指针指向:
┌─────────────────────────────────────────────────────────────┐
│ mp → mproc[slot]                                            │
│       ├── mp_exitstatus                                      │
│       ├── mp_sigstatus                                       │
│       ├── mp_pid                                             │
│       └── ...                                                │
└─────────────────────────────────────────────────────────────┘
```

**mp 变量的重要性**：

```
mp 变量使用场景:
┌─────────────────────────────────────────────────────────────┐
│ 系统调用处理时:                                              │
│                                                              │
│ int do_fork(void) {                                          │
│     struct mproc *rmp = mp;  // 获取调用者进程               │
│     // rmp->mp_pid 就是调用者的 PID                          │
│     // rmp->mp_uid 就是调用者的 UID                          │
│ }                                                            │
├─────────────────────────────────────────────────────────────┤
│ 为什么需要 mp:                                               │
│ - 快速访问当前进程信息                                       │
│ - 避免每次查找进程表                                         │
│ - 系统调用处理的上下文                                       │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.3 uname 信息

```c
/* Misc.c */
extern struct utsname uts_val;	/* uname info */
```

**逐行解释**：

- **第1行**：`/* Misc.c */`
  - 注释说明此变量在 `misc.c` 中定义。

- **第2行**：`extern struct utsname uts_val;`
  - 声明 `uts_val` 为 `struct utsname` 类型。
  - 注释翻译：uname 信息。
  - 使用 `extern` 而不是 `EXTERN`，说明这个变量总是声明。

**utsname 结构定义**：

```c
struct utsname {
    char sysname[65];    // 操作系统名称 (如 "Minix")
    char nodename[65];   // 网络节点名称
    char release[65];    // 操作系统版本 (如 "3.3.0")
    char version[65];    // 版本详细信息
    char machine[65];    // 硬件架构 (如 "i386")
};
```

**内存布局**：

```
uts_val 结构 (共 325 字节):
┌─────────────────────────────────────────────────────────────┐
│ 偏移    字段名          大小        示例值                   │
├─────────────────────────────────────────────────────────────┤
│ 0      sysname         65 字节    "Minix"                   │
│ 65     nodename        65 字节    "localhost"               │
│ 130    release         65 字节    "3.3.0"                   │
│ 195    version         65 字节    "revision 1234"           │
│ 260    machine         65 字节    "i386"                    │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.4 消息和调用信息

```c
/* The parameters of the call are kept here. */
EXTERN message m_in;		/* the incoming message itself is kept here. */
EXTERN int who_p, who_e;	/* caller's proc number, endpoint */
EXTERN int call_nr;		/* system call number */
```

**逐行解释**：

- **第1行**：`/* The parameters of the call are kept here. */`
  - 注释翻译：调用的参数保存在这里。

- **第2行**：`EXTERN message m_in;`
  - 声明 `m_in` 为 `message` 类型。
  - 注释翻译：输入消息本身保存在这里。
  - **内存大小**：约 64 字节（取决于消息类型）。
  - **存储位置**：`.bss` 段。

- **第3行**：`EXTERN int who_p, who_e;`
  - 声明 `who_p` 和 `who_e` 为整型变量。
  - 注释翻译：调用者的进程编号和端点。
  - `who_p`：进程槽位编号（0 到 NR_PROCS-1）。
  - `who_e`：进程端点 ID（唯一标识符）。

- **第4行**：`EXTERN int call_nr;`
  - 声明 `call_nr` 为整型变量。
  - 注释翻译：系统调用号。
  - 用于标识是哪个系统调用。

**消息处理流程**：

```
系统调用处理流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 用户进程调用系统调用:                                      │
│    pid = fork();                                             │
├─────────────────────────────────────────────────────────────┤
│ 2. 内核接收请求，转发给 PM:                                   │
│    消息类型 = SYS_FORK                                       │
│    调用者端点 = 用户进程端点                                  │
├─────────────────────────────────────────────────────────────┤
│ 3. PM 主循环接收消息:                                         │
│    receive(ANY, &m_in);                                      │
│    who_e = m_in.m_source;                                    │
│    who_p = _ENDPOINT_P(who_e);                               │
│    call_nr = m_in.m_type;                                    │
│    mp = &mproc[who_p];                                       │
├─────────────────────────────────────────────────────────────┤
│ 4. 调用对应的处理函数:                                        │
│    result = call_vec[call_nr]();                             │
└─────────────────────────────────────────────────────────────┘
```

**变量关系图**：

```
消息处理变量关系:
┌─────────────────────────────────────────────────────────────┐
│ m_in (message)                                               │
│   ├── m_source → who_e (端点)                               │
│   ├── m_type   → call_nr (系统调用号)                       │
│   └── m_u      → 系统调用参数                               │
│                                                              │
│ who_e (端点)                                                 │
│   └── 通过转换 → who_p (槽位编号)                           │
│                                                              │
│ who_p (槽位编号)                                             │
│   └── 索引 → mp = &mproc[who_p]                             │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.5 系统调用向量

```c
extern int (* const call_vec[])(void);
```

**逐行解释**：

- **声明**：`extern int (* const call_vec[])(void);`
  - `call_vec`：系统调用向量名称。
  - `int (*)(void)`：函数指针类型，指向返回 int 的无参函数。
  - `const`：指针本身是常量，不能修改。
  - `[]`：数组，大小未指定。

**声明拆解**：

```
声明拆解:
┌─────────────────────────────────────────────────────────────┐
│ extern                    - 外部链接                         │
│ int                       - 返回类型                         │
│ (* const call_vec[])      - 常量函数指针数组                 │
│ (void)                    - 无参数                           │
└─────────────────────────────────────────────────────────────┘

等价于:
typedef int (*syscall_handler_t)(void);
extern const syscall_handler_t call_vec[];
```

**call_vec 定义（在 table.c 中）**：

```c
int (* const call_vec[])(void) = {
    no_sys,        /* 0 = unused      */
    do_exit,       /* 1 = exit        */
    do_fork,       /* 2 = fork        */
    do_read,       /* 3 = read        */
    do_write,      /* 4 = write       */
    // ...
};
```

**使用方式**：

```c
// 在主循环中
int result;
result = call_vec[call_nr]();  // 调用对应的处理函数
```

---

### 2.6 信号集合

```c
EXTERN sigset_t core_sset;	/* which signals cause core images */
EXTERN sigset_t ign_sset;	/* which signals are by default ignored */
EXTERN sigset_t noign_sset;	/* which signals cannot be ignored */
```

**逐行解释**：

- **第1行**：`EXTERN sigset_t core_sset;`
  - 声明 `core_sset` 为信号集类型。
  - 注释翻译：哪些信号会导致核心转储（core dump）。
  - 包含：`SIGQUIT`、`SIGILL`、`SIGABRT`、`SIGFPE`、`SIGSEGV` 等。

- **第2行**：`EXTERN sigset_t ign_sset;`
  - 声明 `ign_sset` 为信号集类型。
  - 注释翻译：哪些信号默认被忽略。
  - 包含：`SIGCHLD`、`SIGURG`、`SIGWINCH` 等。

- **第3行**：`EXTERN sigset_t noign_sset;`
  - 声明 `noign_sset` 为信号集类型。
  - 注释翻译：哪些信号不能被忽略。
  - 包含：`SIGKILL`、`SIGSTOP`。

**sigset_t 类型**：

```c
// sigset_t 通常定义为位掩码
typedef struct {
    unsigned long __val[128 / sizeof(long)];
} sigset_t;

// 或者简化为
typedef unsigned long sigset_t;  // 64 位系统上可表示 64 个信号
```

**信号分类表**：

```
信号分类:
┌─────────────────────────────────────────────────────────────┐
│ 信号编号  信号名    默认行为      能否忽略   能否捕获        │
├─────────────────────────────────────────────────────────────┤
│ 1        SIGHUP    终止          是         是              │
│ 2        SIGINT    终止          是         是              │
│ 3        SIGQUIT   终止+core     是         是              │
│ 4        SIGILL    终止+core     是         是              │
│ 5        SIGTRAP   终止+core     是         是              │
│ 6        SIGABRT   终止+core     是         是              │
│ 7        SIGBUS    终止+core     是         是              │
│ 8        SIGFPE    终止+core     是         是              │
│ 9        SIGKILL   终止          否         否              │
│ 10       SIGUSR1   终止          是         是              │
│ 11       SIGSEGV   终止+core     是         是              │
│ 12       SIGUSR2   终止          是         是              │
│ 13       SIGPIPE   终止          是         是              │
│ 14       SIGALRM   终止          是         是              │
│ 15       SIGTERM   终止          是         是              │
│ 17       SIGCHLD   忽略          是         是              │
│ 18       SIGCONT   继续          是         是              │
│ 19       SIGSTOP   停止          否         否              │
│ 20       SIGTSTP   停止          是         是              │
└─────────────────────────────────────────────────────────────┘

core_sset = {SIGQUIT, SIGILL, SIGABRT, SIGBUS, SIGFPE, SIGSEGV, ...}
ign_sset  = {SIGCHLD, SIGURG, SIGWINCH, ...}
noign_sset = {SIGKILL, SIGSTOP}
```

---

### 2.7 系统频率和标志

```c
EXTERN u32_t system_hz;		/* System clock frequency. */
EXTERN int abort_flag;
```

**逐行解释**：

- **第1行**：`EXTERN u32_t system_hz;`
  - 声明 `system_hz` 为 32 位无符号整数。
  - 注释翻译：系统时钟频率。
  - 典型值：100 Hz（每秒 100 个时钟滴答）。
  - 用于时间计算：秒数 × system_hz = 滴答数。

- **第2行**：`EXTERN int abort_flag;`
  - 声明 `abort_flag` 为整型变量。
  - 系统中止标志。
  - 非零值表示系统正在中止。

**system_hz 使用示例**：

```c
// 将秒转换为滴答数
clock_t seconds_to_ticks(int seconds) {
    return seconds * system_hz;
}

// 将滴答数转换为秒
int ticks_to_seconds(clock_t ticks) {
    return ticks / system_hz;
}
```

---

### 2.8 机器信息

```c
EXTERN struct machine machine;		/* machine info */
#ifdef CONFIG_SMP
EXTERN int cpu_proc[CONFIG_MAX_CPUS];
#endif
```

**逐行解释**：

- **第1行**：`EXTERN struct machine machine;`
  - 声明 `machine` 为 `struct machine` 类型。
  - 注释翻译：机器信息。
  - 存储硬件相关信息。

- **第2-4行**：SMP 支持
  - `#ifdef CONFIG_SMP`：条件编译，仅在 SMP 配置时编译。
  - `EXTERN int cpu_proc[CONFIG_MAX_CPUS];`：CPU 进程映射数组。
  - 记录每个 CPU 当前运行的进程。

**machine 结构定义**：

```c
struct machine {
    unsigned processor;   // 处理器类型
    int vdu_ega;          // EGA 显示支持
    int vdu_vga;          // VGA 显示支持
    int pc_at;            // AT 兼容
    int ps_mca;           // MCA 总线
    int pc_xt;            // XT 兼容
    int ready;            // 就绪标志
    int fpu;              // 浮点单元支持
};
```

**SMP 配置说明**：

```
SMP (对称多处理):
┌─────────────────────────────────────────────────────────────┐
│ cpu_proc 数组:                                               │
│                                                              │
│ cpu_proc[0] = 进程 A 的槽位编号  (CPU 0 运行进程 A)          │
│ cpu_proc[1] = 进程 B 的槽位编号  (CPU 1 运行进程 B)          │
│ cpu_proc[2] = -1                 (CPU 2 空闲)               │
│ cpu_proc[3] = 进程 C 的槽位编号  (CPU 3 运行进程 C)          │
└─────────────────────────────────────────────────────────────┘
```

---

## 三、理论关联

### 3.1 全局变量与进程管理

PM 的全局变量支持进程管理的核心功能：

```
全局变量与进程管理:
┌─────────────────────────────────────────────────────────────┐
│ 进程创建 (fork):                                             │
│ - mp: 获取父进程信息                                         │
│ - procs_in_use: 增加进程计数                                 │
│ - call_vec[SYS_FORK]: 调用 do_fork()                        │
├─────────────────────────────────────────────────────────────┤
│ 进程退出 (exit):                                             │
│ - mp: 获取退出进程信息                                       │
│ - procs_in_use: 减少进程计数                                 │
│ - call_vec[SYS_EXIT]: 调用 do_exit()                        │
├─────────────────────────────────────────────────────────────┤
│ 信号处理:                                                    │
│ - core_sset: 判断是否生成 core dump                          │
│ - ign_sset: 判断默认行为                                     │
│ - noign_sset: 判断能否忽略                                   │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 微内核架构

PM 作为用户态服务，通过全局变量维护状态：

```
微内核架构中的全局变量:
┌─────────────────────────────────────────────────────────────┐
│ 内核空间:                                                    │
│ - 内核进程表 (kproc)                                         │
│ - 中断处理                                                   │
│ - 调度器                                                     │
├─────────────────────────────────────────────────────────────┤
│ 用户空间 (PM):                                               │
│ - mproc[NR_PROCS]: PM 进程表                                 │
│ - mp: 当前进程指针                                           │
│ - m_in: 消息缓冲区                                           │
│ - call_vec: 系统调用分发表                                   │
├─────────────────────────────────────────────────────────────┤
│ 用户空间 (VFS):                                              │
│ - vfs 进程表                                                 │
│ - 文件描述符表                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## 四、Rust 实现与对比

### 4.1 全局变量封装

**C 语言版本**：
```c
EXTERN struct mproc *mp;
EXTERN int procs_in_use;
EXTERN message m_in;
EXTERN int who_p, who_e;
EXTERN int call_nr;
```

**Rust 版本**：
```rust
#![no_std]

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

pub type Pid = i32;
pub type Endpoint = i32;

#[derive(Debug, Clone, Copy)]
pub struct Message {
    pub m_source: Endpoint,
    pub m_type: i32,
    pub m_u: MessageData,
}

#[derive(Debug, Clone, Copy)]
pub union MessageData {
    pub m_m1: M1,
    pub m_m2: M2,
    pub m_m3: M3,
}

#[derive(Debug, Clone, Copy)]
pub struct M1 {
    pub m1i1: i32,
    pub m1i2: i32,
    pub m1i3: i32,
    pub m1p1: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct M2 {
    pub m2i1: i32,
    pub m2i2: i32,
    pub m2l1: i64,
    pub m2l2: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct M3 {
    pub m3i1: i32,
    pub m3i2: i32,
    pub m3ca1: [u8; 14],
}

pub struct MProc;

pub struct PmGlobals {
    pub mp: Option<*mut MProc>,
    pub procs_in_use: AtomicI32,
    pub m_in: Message,
    pub who_p: i32,
    pub who_e: i32,
    pub call_nr: i32,
    pub system_hz: AtomicU32,
    pub abort_flag: AtomicI32,
}

impl PmGlobals {
    pub const fn new() -> Self {
        Self {
            mp: None,
            procs_in_use: AtomicI32::new(0),
            m_in: Message {
                m_source: 0,
                m_type: 0,
                m_u: MessageData { m_m1: M1 { m1i1: 0, m1i2: 0, m1i3: 0, m1p1: 0 } },
            },
            who_p: 0,
            who_e: 0,
            call_nr: 0,
            system_hz: AtomicU32::new(100),
            abort_flag: AtomicI32::new(0),
        }
    }

    pub fn inc_procs(&self) {
        self.procs_in_use.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dec_procs(&self) {
        self.procs_in_use.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn is_aborted(&self) -> bool {
        self.abort_flag.load(Ordering::SeqCst) != 0
    }

    pub fn set_abort(&self) {
        self.abort_flag.store(1, Ordering::SeqCst);
    }

    pub fn get_hz(&self) -> u32 {
        self.system_hz.load(Ordering::SeqCst)
    }
}

pub static mut PM_GLOBALS: PmGlobals = PmGlobals::new();
```

### 4.2 系统调用向量

**C 语言版本**：
```c
extern int (* const call_vec[])(void);
```

**Rust 版本**：
```rust
pub type SysCallHandler = fn(&mut PmGlobals) -> i32;

pub const CALL_VEC: &[SysCallHandler] = &[
    sys_no_sys,    // 0
    sys_exit,      // 1
    sys_fork,      // 2
    sys_read,      // 3
    sys_write,     // 4
];

fn sys_no_sys(_g: &mut PmGlobals) -> i32 { -ENOSYS }
fn sys_exit(g: &mut PmGlobals) -> i32 { 
    // 实现退出逻辑
    0 
}
fn sys_fork(g: &mut PmGlobals) -> i32 { 
    // 实现 fork 逻辑
    0 
}
fn sys_read(_g: &mut PmGlobals) -> i32 { -ENOSYS }
fn sys_write(_g: &mut PmGlobals) -> i32 { -ENOSYS }

pub fn dispatch_syscall(g: &mut PmGlobals) -> i32 {
    let call_nr = g.call_nr as usize;
    if call_nr < CALL_VEC.len() {
        CALL_VEC[call_nr](g)
    } else {
        -ENOSYS
    }
}

const ENOSYS: i32 = 38;
```

### 4.3 信号集合

**C 语言版本**：
```c
EXTERN sigset_t core_sset;
EXTERN sigset_t ign_sset;
EXTERN sigset_t noign_sset;
```

**Rust 版本**：
```rust
use bitflags::bitflags;

bitflags! {
    pub struct SigSet: u64 {
        const SIGHUP    = 1 << 0;
        const SIGINT    = 1 << 1;
        const SIGQUIT   = 1 << 2;
        const SIGILL    = 1 << 3;
        const SIGTRAP   = 1 << 4;
        const SIGABRT   = 1 << 5;
        const SIGBUS    = 1 << 6;
        const SIGFPE    = 1 << 7;
        const SIGKILL   = 1 << 8;
        const SIGUSR1   = 1 << 9;
        const SIGSEGV   = 1 << 10;
        const SIGUSR2   = 1 << 11;
        const SIGPIPE   = 1 << 12;
        const SIGALRM   = 1 << 13;
        const SIGTERM   = 1 << 14;
        const SIGCHLD   = 1 << 16;
        const SIGCONT   = 1 << 17;
        const SIGSTOP   = 1 << 18;
        const SIGTSTP   = 1 << 19;
    }
}

impl SigSet {
    pub const CORE_DUMP: SigSet = SigSet::from_bits_truncate(
        SigSet::SIGQUIT.bits | SigSet::SIGILL.bits | 
        SigSet::SIGABRT.bits | SigSet::SIGBUS.bits | 
        SigSet::SIGFPE.bits | SigSet::SIGSEGV.bits
    );
    
    pub const IGNORE: SigSet = SigSet::from_bits_truncate(
        SigSet::SIGCHLD.bits
    );
    
    pub const NO_IGNORE: SigSet = SigSet::from_bits_truncate(
        SigSet::SIGKILL.bits | SigSet::SIGSTOP.bits
    );
}

pub fn causes_core_dump(sig: SigSet) -> bool {
    sig.intersects(SigSet::CORE_DUMP)
}

pub fn can_ignore(sig: SigSet) -> bool {
    !sig.intersects(SigSet::NO_IGNORE)
}
```

### 4.4 Rust 优势分析

| 方面 | C 语言 | Rust |
|------|--------|------|
| 全局变量 | 散落在各处 | 封装在结构体中 |
| 线程安全 | 无保证 | `Atomic` 类型 |
| 空指针 | 可能崩溃 | `Option<T>` |
| 错误处理 | 错误码 | `Result<T, E>` |
| 类型安全 | 弱类型 | 强类型 |

---

## 五、要点总结

### 5.1 核心知识点

1. **EXTERN 宏机制**：在 `table.c` 中定义变量，其他文件中声明。

2. **核心全局变量**：
   - `mp`：当前进程指针
   - `procs_in_use`：进程计数
   - `m_in`：消息缓冲区
   - `call_vec`：系统调用分发表

3. **信号集合**：`core_sset`、`ign_sset`、`noign_sset` 用于信号处理。

### 5.2 设计亮点

- **集中管理**：所有全局变量在一处声明。
- **单一定义**：使用 EXTERN 宏确保变量只定义一次。
- **快速访问**：`mp` 指针避免频繁查找进程表。

---

## 六、灾难预演

### 6.1 如果删除 mp 变量

**后果**：系统调用处理变慢。

**现象**：
- 每次系统调用需要查找进程表。
- 性能下降 10-100 倍。
- 代码复杂度增加。

### 6.2 如果删除 call_vec

**后果**：系统调用无法分发。

**现象**：
- 无法处理任何系统调用。
- 进程无法创建、退出。
- 系统完全失效。

### 6.3 如果 EXTERN 宏定义错误

**后果**：链接错误。

**现象**：
- 如果所有文件都定义变量：多重定义错误。
- 如果所有文件都声明变量：未定义符号错误。
- 链接阶段失败。

---

## 七、互动自测

### 问题 1：EXTERN 宏的作用是什么？

<details>
<summary>点击查看答案</summary>

EXTERN 宏的作用是统一管理全局变量的定义和声明：

1. **在 table.c 中**：
   - 定义 `_TABLE` 宏。
   - EXTERN 展开为空。
   - `EXTERN int x;` 变成 `int x;`（定义）。

2. **在其他文件中**：
   - 不定义 `_TABLE` 宏。
   - EXTERN 保持 `extern`。
   - `EXTERN int x;` 变成 `extern int x;`（声明）。

**好处**：
- 确保变量只定义一次。
- 避免重复声明。
- 集中管理全局变量。
</details>

### 问题 2：mp 变量为什么重要？

<details>
<summary>点击查看答案</summary>

`mp` 变量的重要性：

1. **快速访问**：
   - 直接指向当前进程的 mproc 槽位。
   - 不需要每次查找进程表。

2. **上下文信息**：
   - 系统调用处理时，需要知道调用者是谁。
   - `mp->mp_pid` 是调用者的 PID。
   - `mp->mp_uid` 是调用者的 UID。

3. **权限检查**：
   - 检查调用者是否有权限执行操作。
   - 例如：`setuid` 需要检查调用者权限。

**示例**：
```c
int do_kill(void) {
    pid_t pid = m_in.m1_i1;  // 目标进程
    int sig = m_in.m1_i2;    // 信号
    
    // 检查权限
    if (mp->mp_uid != 0) {   // 非超级用户
        // 只能向自己的进程发送信号
    }
}
```
</details>

### 问题 3：call_vec 是如何工作的？

<details>
<summary>点击查看答案</summary>

`call_vec` 是系统调用分发表：

1. **结构**：
   - 函数指针数组。
   - 索引是系统调用号。
   - 值是对应的处理函数。

2. **定义**：
   ```c
   int (* const call_vec[])(void) = {
       [SYS_EXIT] = do_exit,
       [SYS_FORK] = do_fork,
       [SYS_EXEC] = do_exec,
       // ...
   };
   ```

3. **使用**：
   ```c
   // 主循环中
   int result = call_vec[call_nr]();
   ```

4. **工作流程**：
   - 用户调用 `fork()`。
   - 内核发送消息给 PM。
   - PM 接收消息，提取 `call_nr = SYS_FORK`。
   - 调用 `call_vec[SYS_FORK]()` 即 `do_fork()`。
   - 返回结果给用户。
</details>
