# do_times.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_times.c`

**总行数**: 46 行

**作用**: 实现 `SYS_TIMES` 系统调用，获取进程和系统时间统计

---

## 一、文件概述

### 1.1 是什么（What）

`do_times.c` 实现了 MINIX3 的**时间统计系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_TIMES` | 获取进程时间统计和系统时间 |

**核心功能**：
- 获取进程的用户态运行时间
- 获取进程的内核态运行时间
- 获取系统启动以来的单调时间
- 获取实时时钟值
- 获取系统启动时间（Unix 时间戳）

### 1.2 为什么需要（Why）

**设计原因**：

在操作系统中，时间统计对于以下场景至关重要：

1. **进程记账**：记录进程使用的 CPU 时间
2. **性能分析**：分析程序在用户态和内核态的时间分布
3. **资源限制**：限制进程的 CPU 使用时间
4. **时间同步**：获取系统时间用于网络协议

**用户时间 vs 系统时间**：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  进程执行时间分布                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户时间 (p_user_time):                                                │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  进程在用户态执行应用程序代码的时间                                │  │
│  │  - 执行用户程序指令                                               │  │
│  │  - 计算密集型操作                                                 │  │
│  │  - 用户态库函数调用                                               │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  系统时间 (p_sys_time):                                                │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  进程在内核态执行系统调用的时间                                    │  │
│  │  - 文件 I/O 操作                                                  │  │
│  │  - 网络通信                                                       │  │
│  │  - 内存管理                                                       │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.3 使用场景（When）

| 场景 | 使用的时间类型 | 说明 |
|------|---------------|------|
| `times()` 系统调用 | 用户时间 + 系统时间 | POSIX 进程时间统计 |
| `getrusage()` | 用户时间 + 系统时间 | 资源使用统计 |
| `ps` 命令 | 用户时间 + 系统时间 | 显示进程 CPU 使用 |
| `top` 命令 | 用户时间 + 系统时间 | 实时监控 |
| NTP 时间同步 | 实时时钟 | 网络时间协议 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-13 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_TIMES
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_times.endpt		(get info for this process)
 *   m_krn_lsys_sys_times.user_time	(return values ...)
 *   m_krn_lsys_sys_times.system_time
 *   m_krn_lsys_sys_times.boot_time
 *   m_krn_lsys_sys_times.boot_ticks
 *   m_krn_lsys_sys_times.real_ticks
 */
```

**翻译**：
```
本文件实现的内核调用：
  m_type: SYS_TIMES

此内核调用的参数：
  m_lsys_krn_sys_times.endpt       - 获取此进程的信息
  m_krn_lsys_sys_times.user_time   - 返回值：用户时间
  m_krn_lsys_sys_times.system_time - 返回值：系统时间
  m_krn_lsys_sys_times.boot_time   - 返回值：启动时间
  m_krn_lsys_sys_times.boot_ticks  - 返回值：启动以来的 ticks
  m_krn_lsys_sys_times.real_ticks  - 返回值：实时时钟 ticks
```

**参数详解**：

| 字段 | 方向 | 类型 | 含义 |
|------|------|------|------|
| `endpt` | 输入 | `endpoint_t` | 目标进程端点 |
| `user_time` | 输出 | `clock_t` | 用户态运行时间（ticks） |
| `system_time` | 输出 | `clock_t` | 内核态运行时间（ticks） |
| `boot_ticks` | 输出 | `clock_t` | 单调时钟（ticks） |
| `real_ticks` | 输出 | `clock_t` | 实时时钟（ticks） |
| `boot_time` | 输出 | `time_t` | 系统启动时间（Unix 时间戳） |

### 2.2 头文件包含（第 15-21 行）

```c
#include "kernel/system.h"

#include <minix/endpoint.h>

#if USE_TIMES
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架、`struct proc` 定义、时钟函数声明 |
| `<minix/endpoint.h>` | 端点类型定义（`endpoint_t`、`SELF`、`NONE`） |

**条件编译**：`USE_TIMES` 控制是否编译此功能。

### 2.3 do_times 函数签名（第 23-27 行）

```c
/*===========================================================================*
 *				do_times				     *
 *===========================================================================*/
int do_times(struct proc * caller, message * m_ptr)
```

**参数**：
- `caller` - 调用者进程指针
- `m_ptr` - 消息指针，包含请求参数和返回值

**返回值**：
- `OK` - 操作成功（此函数总是成功）

### 2.4 函数注释（第 28 行）

```c
/* Handle sys_times().  Retrieve the accounting information. */
```

**翻译**：`Handle sys_times(). Retrieve the accounting information.` = "处理 sys_times()。获取记账信息。"

**设计原因**：
- "记账"（accounting）是操作系统术语
- 指记录进程资源使用情况
- 主要用于计费、审计、性能分析

### 2.5 局部变量声明（第 29-32 行）

```c
  register const struct proc *rp;
  int proc_nr;
  endpoint_t e_proc_nr;
```

| 变量 | 类型 | 大小 | 存储位置 | 用途 |
|------|------|------|---------|------|
| `rp` | `const struct proc *` | 8 字节 | 寄存器/栈 | 指向目标进程结构的指针 |
| `proc_nr` | `int` | 4 字节 | 栈 | 进程槽号 |
| `e_proc_nr` | `endpoint_t` | 4 字节 | 栈 | 进程端点 |

**`register` 关键字**：
- 提示编译器将变量放入寄存器
- 现代编译器通常忽略此提示
- 保留是为了历史兼容性

### 2.6 注释说明（第 34-38 行）

```c
  /* Insert the times needed by the SYS_TIMES kernel call in the message. 
   * The clock's interrupt handler may run to update the user or system time
   * while in this code, but that cannot do any harm.
   */
```

**翻译**：
```
将 SYS_TIMES 内核调用所需的时间插入消息中。
时钟中断处理程序可能在此代码执行期间运行并更新用户或系统时间，
但这不会造成任何危害。
```

**设计原因**：
- 时间统计是原子递增的
- 即使被中断打断，数据仍然一致
- 不需要加锁保护

### 2.7 确定目标进程（第 39-41 行）

```c
  e_proc_nr = (m_ptr->m_lsys_krn_sys_times.endpt == SELF) ?
      caller->p_endpoint : m_ptr->m_lsys_krn_sys_times.endpt;
```

**SELF 处理**：
- 如果 `endpt == SELF`，使用调用者进程
- 否则使用指定的进程端点

**示例**：
```
调用者: PM 进程 (endpoint = 10)
请求: endpt = SELF
结果: e_proc_nr = 10 (PM 自己)

请求: endpt = 100
结果: e_proc_nr = 100 (指定进程)
```

### 2.8 获取进程时间统计（第 42-45 行）

```c
  if(e_proc_nr != NONE && isokendpt(e_proc_nr, &proc_nr)) {
      rp = proc_addr(proc_nr);
      m_ptr->m_krn_lsys_sys_times.user_time   = rp->p_user_time;
      m_ptr->m_krn_lsys_sys_times.system_time = rp->p_sys_time;
  }
```

**逐行解析**：

| 行号 | 代码 | 说明 |
|------|------|------|
| 42 | `if(e_proc_nr != NONE && isokendpt(...))` | 检查端点有效性 |
| 43 | `rp = proc_addr(proc_nr)` | 获取进程结构指针 |
| 44 | `m_ptr->...user_time = rp->p_user_time` | 返回用户时间 |
| 45 | `m_ptr->...system_time = rp->p_sys_time` | 返回系统时间 |

**进程结构中的时间字段**（定义在 `kernel/proc.h`）：
```c
clock_t p_user_time;    /* 用户态时间（ticks） */
clock_t p_sys_time;     /* 内核态时间（ticks） */
```

**时间更新机制**（在 `kernel/clock.c` 的时钟中断处理中）：
```c
p->p_user_time++;                    // 每次时钟中断增加用户时间
if (!(priv(p)->s_flags & BILLABLE)) {
    billp->p_sys_time++;             // 非计费进程增加系统时间
}
```

### 2.9 获取系统时间（第 46-48 行）

```c
  m_ptr->m_krn_lsys_sys_times.boot_ticks = get_monotonic();
  m_ptr->m_krn_lsys_sys_times.real_ticks = get_realtime();
  m_ptr->m_krn_lsys_sys_times.boot_time = get_boottime();
```

**三种时间的区别**：

| 函数 | 返回值 | 含义 |
|------|--------|------|
| `get_monotonic()` | 单调时钟 | 系统启动以来的 ticks，不受时间调整影响 |
| `get_realtime()` | 实时时钟 | 可调整的墙上时钟 ticks |
| `get_boottime()` | 启动时间 | 系统启动时的 Unix 时间戳 |

**函数实现**（在 `kernel/clock.c`）：
```c
clock_t get_monotonic(void) {
    return(kclockinfo.uptime);      // 单调递增
}

clock_t get_realtime(void) {
    return(kclockinfo.realtime);    // 可被 set_realtime() 修改
}

time_t get_boottime(void) {
    return(kclockinfo.boottime);    // 可被 set_boottime() 修改
}
```

### 2.10 返回成功（第 49 行）

```c
  return(OK);
```

**设计原因**：
- 此函数不会失败
- 总是返回 `OK`
- 即使进程不存在，也只是不返回时间统计

### 2.11 条件编译结束（第 51 行）

```c
#endif /* USE_TIMES */
```

---

## 三、时间系统架构

### 3.1 时间数据结构

```
┌─────────────────────────────────────────────────────────────────────────┐
│  kclockinfo 结构（kernel/clock.c）                                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  struct {                                                               │
│      clock_t uptime;      // 单调时钟：系统启动以来的 ticks            │
│      clock_t realtime;    // 实时时钟：可调整的墙上时钟 ticks          │
│      time_t boottime;     // 启动时间：Unix 时间戳                     │
│      int hz;              // 时钟频率（通常 100 Hz）                   │
│  } kclockinfo;                                                          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 时间关系

```
┌─────────────────────────────────────────────────────────────────────────┐
│  时间计算关系                                                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  当前 Unix 时间 = boottime + realtime / hz                              │
│                                                                         │
│  示例：                                                                 │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  boottime = 1704067200  (2024-01-01 00:00:00 UTC)                │  │
│  │  realtime = 360000 ticks                                        │  │
│  │  hz = 100                                                        │  │
│  │                                                                  │  │
│  │  当前时间 = 1704067200 + 360000/100                              │  │
│  │           = 1704067200 + 3600                                    │  │
│  │           = 1704070800  (2024-01-01 01:00:00 UTC)                │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.3 时钟中断流程

```
时钟中断发生
    │
    ▼
timer_int_handler() [kernel/clock.c]
    │
    ├── 更新系统时间
    │   ├── kclockinfo.uptime++        // 单调时钟
    │   └── kclockinfo.realtime++      // 实时时钟
    │
    ├── 更新进程时间统计
    │   ├── p->p_user_time++           // 当前进程用户时间
    │   └── billp->p_sys_time++        // 计费进程系统时间
    │
    ├── 更新虚拟定时器
    │   ├── p->p_virt_left--           // VT_VIRTUAL
    │   └── p->p_prof_left--           // VT_PROF
    │
    └── 检查定时器到期
        └── vtimer_check()
```

---

## 四、与 POSIX times() 的关系

### 4.1 POSIX times() 系统调用

```c
#include <sys/times.h>

clock_t times(struct tms *buf);
```

**struct tms 结构**：
```c
struct tms {
    clock_t tms_utime;   // 用户态 CPU 时间
    clock_t tms_stime;   // 内核态 CPU 时间
    clock_t tms_cutime;  // 已终止子进程的用户时间
    clock_t tms_cstime;  // 已终止子进程的系统时间
};
```

### 4.2 MINIX3 实现链路

```
用户程序
    │
    ▼
times() [libc]
    │
    ▼
SYS_TIMES 系统调用
    │
    ▼
do_times() [kernel/system/do_times.c]
    │
    ├── 返回 p_user_time → tms_utime
    └── 返回 p_sys_time → tms_stime
```

---

## 五、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 时钟精度 | tick 级别（10ms） | 纳秒级高精度定时器 |
| 时间源 | PIT/i8254 | TSC/HPET/ACPI Timer |
| 多核支持 | 全局时钟 | per-CPU 时钟 |
| 电源管理 | 固定频率 | tickless 内核 |

---

## 六、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy, Default)]
pub struct TimesInfo {
    pub user_time: u64,
    pub system_time: u64,
    pub boot_ticks: u64,
    pub real_ticks: u64,
    pub boot_time: i64,
}

pub fn do_times(caller: &Proc, m_ptr: &mut Message) -> Result<(), ()> {
    let e_proc_nr = if m_ptr.endpt == Endpoint::SELF {
        caller.endpoint
    } else {
        m_ptr.endpt
    };

    if e_proc_nr != Endpoint::NONE {
        if let Some(rp) = Proc::from_endpoint(e_proc_nr) {
            m_ptr.user_time = rp.user_time;
            m_ptr.system_time = rp.sys_time;
        }
    }

    m_ptr.boot_ticks = get_monotonic();
    m_ptr.real_ticks = get_realtime();
    m_ptr.boot_time = get_boottime();

    Ok(())
}

fn get_monotonic() -> u64 {
    KCLOCKINFO.uptime
}

fn get_realtime() -> u64 {
    KCLOCKINFO.realtime
}

fn get_boottime() -> i64 {
    KCLOCKINFO.boottime
}
```

---

## 七、要点总结

### 核心知识点

1. **三种时间类型**：
   - 单调时钟（monotonic）：系统启动以来，不受调整影响
   - 实时时钟（realtime）：可调整的墙上时钟
   - 启动时间（boottime）：Unix 时间戳

2. **进程时间统计**：
   - 用户时间：进程在用户态执行的时间
   - 系统时间：进程在内核态执行的时间
   - 每次时钟中断更新

3. **无锁设计**：
   - 时间统计是原子递增
   - 不需要加锁保护
   - 中断安全

---

## 八、灾难预演

### 场景 1：如果时间统计不准确

```
后果：
1. 进程记账错误
2. 性能分析数据无效
3. CPU 使用率显示异常
```

### 场景 2：如果 get_monotonic() 返回错误值

```
后果：
1. 定时器计算错误
2. 超时判断失效
3. 系统调度异常
```

### 场景 3：如果进程端点无效

```
后果：
1. isokendpt() 检查失败
2. 不返回进程时间统计
3. 但系统时间仍然返回
```

---

## 九、互动自测

1. **问题**：用户时间和系统时间有什么区别？
   **答案**：用户时间是进程在用户态执行的时间，系统时间是进程在内核态（系统调用）执行的时间。

2. **问题**：get_monotonic() 和 get_realtime() 有什么区别？
   **答案**：get_monotonic() 返回单调递增的时间，不受时间调整影响；get_realtime() 返回可调整的实时时钟。

3. **问题**：为什么 do_times() 不需要加锁？
   **答案**：时间统计是原子递增操作，即使被中断打断，数据仍然一致，不需要锁保护。

---

*讲解者：Minix-rs 学习助手*
