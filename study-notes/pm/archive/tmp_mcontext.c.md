# servers/pm/mcontext.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/mcontext.c`
> **核心功能**: 机器上下文系统调用

---

## 文件概述

这个文件实现了机器上下文（machine context）相关的系统调用，用于保存和恢复进程的 CPU 执行状态。这是实现用户级线程（如 POSIX ucontext）的基础设施。

**核心概念**:
- **机器上下文**: CPU 寄存器状态（通用寄存器、程序计数器、栈指针等）
- **上下文切换**: 保存当前执行状态并恢复另一个状态
- **用户级线程**: 不需要内核参与的线程切换机制

---

## 头文件包含

```c
#include "pm.h"
#include <minix/callnr.h>
#include <minix/endpoint.h>
#include <minix/com.h>
#include <minix/vm.h>
#include "mproc.h"
```

**逐行讲解**:

| 行号 | 头文件 | 作用 | 关键内容 |
|------|--------|------|----------|
| 1 | `"pm.h"` | PM 模块主头文件 | PM 公共定义 |
| 2 | `<minix/callnr.h>` | 系统调用号定义 | `PM_GETMCONTEXT`, `PM_SETMCONTEXT` |
| 3 | `<minix/endpoint.h>` | 端点类型定义 | `endpoint_t`, `who_e` |
| 4 | `<minix/com.h>` | 通信相关定义 | 进程号常量 |
| 5 | `<minix/vm.h>` | 虚拟内存定义 | VM 相关接口 |
| 6 | `"mproc.h"` | 进程控制块定义 | `struct mproc`, `mproc[]` |

**设计原因**: 这个文件非常精简，因为实际的上下文操作由内核完成。PM 只作为系统调用的转发层。

---

## do_setmcontext 函数

```c
/*===========================================================================*
 *				do_setmcontext				     *
 *===========================================================================*/
int
do_setmcontext(void)
{
  return sys_setmcontext(who_e, m_in.m_lc_pm_mcontext.ctx);
}
```

**逐行讲解**:

**第 10 行**: 函数头注释
- 标准的 Minix 函数分隔注释

**第 11-15 行**: `int do_setmcontext(void)`
- **返回类型**: `int` - 系统调用返回值（OK 或错误码）
- **参数**: 无参数（通过全局变量 `m_in` 获取输入）

**第 14 行**: `return sys_setmcontext(who_e, m_in.m_lc_pm_mcontext.ctx);`

**参数解析**:

| 参数 | 含义 | 来源 |
|------|------|------|
| `who_e` | 调用进程的端点号 | 全局变量，PM 主循环设置 |
| `m_in.m_lc_pm_mcontext.ctx` | 用户空间的 mcontext 结构指针 | 系统调用参数 |

**`m_in` 结构**: 
- 全局消息缓冲区，存储用户进程发送的系统调用参数
- `m_lc_pm_mcontext` 是消息联合体中的一个字段，专门用于 mcontext 系统调用
- `ctx` 是 `vir_bytes` 类型（虚拟地址）

**内存图示**:
```
用户进程空间:
┌─────────────────────────────────────┐
│ mcontext_t 结构                      │
│ ├── mc_flags                         │
│ ├── 通用寄存器 (eax, ebx, ecx...)    │
│ ├── 程序计数器 (eip)                 │
│ ├── 栈指针 (esp, ebp)               │
│ └── FPU 状态 (可选)                  │
└─────────────────────────────────────┘
         ↑
         │ m_in.m_lc_pm_mcontext.ctx 指向这里
         
PM 进程空间:
┌─────────────────────────────────────┐
│ who_e = 调用者端点号                 │
│ m_in.m_lc_pm_mcontext.ctx = 地址    │
└─────────────────────────────────────┘
         │
         │ sys_setmcontext() 转发
         ▼
内核空间:
┌─────────────────────────────────────┐
│ do_setmcontext() 实际执行            │
│ 从用户空间复制 mcontext 结构         │
│ 恢复进程的寄存器状态                  │
└─────────────────────────────────────┘
```

**设计原因**:
1. **微内核架构**: PM 不直接操作 CPU 寄存器，通过内核调用完成
2. **安全隔离**: 只有内核可以修改进程的执行状态
3. **转发模式**: PM 作为系统服务，转发请求到内核

---

## do_getmcontext 函数

```c
/*===========================================================================*
 *				do_getmcontext				     *
 *===========================================================================*/
int
do_getmcontext(void)
{
  return sys_getmcontext(who_e, m_in.m_lc_pm_mcontext.ctx);
}
```

**逐行讲解**:

**第 20-25 行**: `int do_getmcontext(void)`
- **功能**: 获取当前进程的机器上下文
- **返回值**: `OK` 成功，或错误码

**第 24 行**: `return sys_getmcontext(who_e, m_in.m_lc_pm_mcontext.ctx);`

**与 setmcontext 的对比**:

| 操作 | 数据流向 | 用途 |
|------|----------|------|
| `getmcontext` | 进程寄存器 → 用户空间 | 保存当前执行点 |
| `setmcontext` | 用户空间 → 进程寄存器 | 恢复执行点 |

**应用场景**:

1. **用户级线程切换**:
```c
// 线程 A 让出 CPU
getmcontext(&ctx_a);      // 保存 A 的上下文
setmcontext(&ctx_b);      // 恢复 B 的上下文

// 线程 B 执行...

// 线程 B 让出 CPU
getmcontext(&ctx_b);      // 保存 B 的上下文
setmcontext(&ctx_a);      // 恢复 A 的上下文
```

2. **协程实现**:
```c
// 协程 yield
getmcontext(&current_ctx);
setmcontext(&scheduler_ctx);
```

3. **异常处理（类似 setjmp/longjmp）**:
```c
if (getmcontext(&error_ctx) == 0) {
    // 正常执行路径
    risky_operation();
} else {
    // 从 setmcontext 返回，错误处理
    handle_error();
}
```

---

## 内核实现分析

`sys_setmcontext` 和 `sys_getmcontext` 的实现在：
- 库函数: `minix3/minix/lib/libsys/sys_mcontext.c`
- 内核处理: `minix3/minix/kernel/system/do_mcontext.c`

**库函数实现**（简化版）:
```c
int sys_getmcontext(endpoint_t proc, vir_bytes mcp) {
    message m;
    m.m_lsys_krn_sys_getmcontext.endpt = proc;
    m.m_lsys_krn_sys_getmcontext.ctx_ptr = mcp;
    return _kernel_call(SYS_GETMCONTEXT, &m);
}

int sys_setmcontext(endpoint_t proc, vir_bytes mcp) {
    message m;
    m.m_lsys_krn_sys_setmcontext.endpt = proc;
    m.m_lsys_krn_sys_setmcontext.ctx_ptr = mcp;
    return _kernel_call(SYS_SETMCONTEXT, &m);
}
```

**内核处理流程**（`do_getmcontext`）:
```
1. 验证端点号有效性
2. 获取进程控制块指针
3. 从用户空间复制 mcontext 结构
4. 填充寄存器值（通用寄存器、PC、SP）
5. 保存 FPU 状态（如果使用过）
6. 将 mcontext 结构复制回用户空间
```

**`mcontext_t` 结构**（i386 架构）:
```c
typedef struct {
    unsigned int mc_flags;        // 标志位
    unsigned int mc_gs;           // 段寄存器
    unsigned int mc_fs;
    unsigned int mc_es;
    unsigned int mc_ds;
    unsigned int mc_edi;          // 通用寄存器
    unsigned int mc_esi;
    unsigned int mc_ebp;
    unsigned int mc_isp;
    unsigned int mc_ebx;
    unsigned int mc_edx;
    unsigned int mc_ecx;
    unsigned int mc_eax;
    unsigned int mc_trapno;       // 陷阱号
    unsigned int mc_err;
    unsigned int mc_eip;          // 程序计数器
    unsigned int mc_cs;           // 代码段
    unsigned int mc_eflags;       // 标志寄存器
    unsigned int mc_esp;          // 栈指针
    unsigned int mc_ss;           // 栈段
    // FPU 状态...
} mcontext_t;
```

**内存布局**（32 位系统）:
```
mcontext_t 结构 (约 100+ 字节):
┌────────────────────────────────────┐
│ mc_flags: 4 字节                   │
│ mc_gs ~ mc_ds: 16 字节             │
│ mc_edi ~ mc_eax: 28 字节           │
│ mc_trapno, mc_err: 8 字节          │
│ mc_eip: 4 字节 (程序计数器)         │
│ mc_cs: 4 字节                      │
│ mc_eflags: 4 字节                  │
│ mc_esp: 4 字节 (栈指针)            │
│ mc_ss: 4 字节                      │
│ FPU 状态: 512 字节 (可选)          │
└────────────────────────────────────┘
```

---

## 设计原因

### 1. 为什么 PM 只是转发？

**微内核原则**: 
- PM 是用户空间进程，没有权限直接访问 CPU 寄存器
- 只有内核可以操作进程的执行状态
- PM 作为系统调用的入口点，负责权限检查和转发

### 2. 为什么需要 mcontext？

**用户级线程支持**:
- 传统线程需要内核参与切换（系统调用开销）
- 用户级线程在用户空间切换，无需内核参与
- `getmcontext`/`setmcontext` 提供了底层原语

**与 setjmp/longjmp 的区别**:
| 特性 | setjmp/longjmp | getmcontext/setmcontext |
|------|----------------|-------------------------|
| 信号掩码 | 不保存 | 可选保存 |
| 栈信息 | 不完整 | 完整保存 |
| FPU 状态 | 不保存 | 保存 |
| 用途 | 异常处理 | 线程切换 |

### 3. 为什么 FPU 状态单独处理？

**性能优化**:
- FPU 状态很大（512 字节）
- 不是所有进程都使用 FPU
- 只在使用过 FPU 时才保存/恢复

---

## 要点总结

1. **转发模式**: PM 的 mcontext 系统调用只是转发到内核，实际操作由内核完成
2. **用户级线程**: 这两个系统调用是用户级线程库的基础原语
3. **完整状态**: mcontext 包含完整的 CPU 执行状态，包括 FPU

---

## 灾难预演

**场景 1**: 如果 `setmcontext` 设置了无效的 `eip` 值
```
后果: 进程恢复执行时跳转到无效地址
      触发段错误 (SIGSEGV)
      进程崩溃
```

**场景 2**: 如果 `setmcontext` 设置了无效的 `esp` 值
```
后果: 栈指针指向无效内存
      函数调用时写入失败
      触发段错误或数据损坏
```

**场景 3**: 如果不保存 FPU 状态
```
后果: 线程切换后 FPU 寄存器内容错误
      浮点运算结果不正确
      科学计算程序崩溃
```

---

## 互动自测

1. **问题**: `getmcontext` 和 `setmcontext` 为什么必须在内核实现？
   **答案**: 因为它们需要直接访问进程的寄存器状态，包括程序计数器、栈指针等。用户空间程序没有权限直接操作这些硬件资源。

2. **问题**: 这两个系统调用与 `setjmp`/`longjmp` 有什么区别？
   **答案**: `mcontext` 系统调用保存完整的机器状态，包括 FPU 状态，适合用户级线程切换。`setjmp`/`longjmp` 只保存少量寄存器，主要用于异常处理。

3. **问题**: 为什么 PM 不直接处理 mcontext 操作？
   **答案**: PM 是用户空间进程，没有权限访问硬件寄存器。微内核架构要求只有内核才能操作进程的执行状态。

---

## Rust 实现对比

```rust
#![no_std]

use core::mem::size_of;

pub type Endpoint = i32;
pub type VirBytes = usize;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MContext {
    pub mc_flags: u32,
    pub mc_gs: u32,
    pub mc_fs: u32,
    pub mc_es: u32,
    pub mc_ds: u32,
    pub mc_edi: u32,
    pub mc_esi: u32,
    pub mc_ebp: u32,
    pub mc_isp: u32,
    pub mc_ebx: u32,
    pub mc_edx: u32,
    pub mc_ecx: u32,
    pub mc_eax: u32,
    pub mc_trapno: u32,
    pub mc_err: u32,
    pub mc_eip: u32,
    pub mc_cs: u32,
    pub mc_eflags: u32,
    pub mc_esp: u32,
    pub mc_ss: u32,
    pub fpregs: [u8; 512],
}

pub const MC_FPU_SAVED: u32 = 0x01;

#[derive(Debug)]
pub enum MContextError {
    InvalidEndpoint,
    PermissionDenied,
    CopyFailed,
}

pub struct PmContext;

impl PmContext {
    pub fn do_setmcontext(who_e: Endpoint, ctx_ptr: VirBytes) -> Result<(), MContextError> {
        sys_setmcontext(who_e, ctx_ptr)
    }

    pub fn do_getmcontext(who_e: Endpoint, ctx_ptr: VirBytes) -> Result<(), MContextError> {
        sys_getmcontext(who_e, ctx_ptr)
    }
}

fn sys_setmcontext(proc: Endpoint, mcp: VirBytes) -> Result<(), MContextError> {
    // 实际实现会调用内核
    // 这里展示接口设计
    let ctx = unsafe { &*(mcp as *const MContext) };
    
    if ctx.mc_flags & MC_FPU_SAVED != 0 {
        // 恢复 FPU 状态
    }
    
    Ok(())
}

fn sys_getmcontext(proc: Endpoint, mcp: VirBytes) -> Result<(), MContextError> {
    // 实际实现会调用内核
    let ctx = unsafe { &mut *(mcp as *mut MContext) };
    
    ctx.mc_flags = 0;
    // 填充寄存器值...
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcontext_size() {
        assert!(size_of::<MContext>() >= 100);
    }
}
```

**Rust 优势**:
1. **类型安全**: `MContext` 结构体明确表示上下文布局
2. **Result 类型**: 强制处理错误，不会忘记检查返回值
3. **repr(C)**: 确保与 C ABI 兼容
4. **常量标志**: `MC_FPU_SAVED` 使用常量而非魔术数字

**unsafe 说明**:
- `sys_setmcontext` 和 `sys_getmcontext` 中的指针解引用需要 `unsafe`
- 这是因为我们直接访问用户空间内存
- 在实际内核实现中，需要使用 `copy_from_user`/`copy_to_user` 等安全函数
