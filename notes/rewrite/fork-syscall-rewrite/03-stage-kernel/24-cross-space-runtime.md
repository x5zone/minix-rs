# 24-cross-space-runtime: 跨地址空间运行时

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/include/minix/syslib.h` (`sys_datacopy` 宏), `minix3/minix/kernel/system/do_copy.c` (`do_copy` 处理 `SYS_VIRCOPY`/`SYS_PHYSCOPY`), `minix3/minix/kernel/arch/i386/memory.c` (`data_copy_vmcheck`), `do_memset.c` (已在 17 中覆盖)
> **前置**: 17（跨进程拷贝——vircopy/safecopy）, 15（SMP——BKL）
> **C 总行数**: ~200 行（do_copy.c 为主）
>
> **注意**: Minix3 中**不存在** `do_datacopy.c` 文件，也不存在 `SYS_DATACOPY` 内核调用号。`sys_datacopy` 是用户空间库宏（`syslib.h:129`），展开为 `sys_vircopy(p1, v1, p2, v2, len, 0)`，最终发送 `SYS_VIRCOPY` 内核调用，由 `do_copy.c` 中的 `do_copy()` 处理。

---

## Ch1: 概念

**核心问题**: 内核如何为系统进程提供跨地址空间的数据传输运行时？

跨地址空间运行时是 vircopy/safecopy 的高层封装，提供：

1. **sys_datacopy 宏**：用户空间库宏（`syslib.h:129`），展开为 `sys_vircopy(p1, v1, p2, v2, len, 0)`，最终发送 `SYS_VIRCOPY` 内核调用。无独立 `SYS_DATACOPY` 内核调用号。
2. **SYS_MEMSET**：跨进程内存填充（已在 17 中覆盖）
3. **data_copy / data_copy_vmcheck**：内核内部跨进程拷贝辅助函数（定义在 `arch/*/memory.c`）

### 1.1 sys_datacopy vs sys_vircopy

| 特性 | sys_vircopy | sys_datacopy |
|------|-------------|--------------|
| 本质 | 内核调用 (`SYS_VIRCOPY=15`) | 用户空间宏，展开为 `sys_vircopy(..., 0)` |
| flags 参数 | 显式传入（0 或 `CP_FLAG_TRY`） | 硬编码为 0 |
| 调用者 | PM/VFS/RS/VM | VM（简化接口） |
| 地址类型 | vir_addr (seg+addr) | endpoint + virtual address |

### 1.2 data_copy_vmcheck()

`data_copy_vmcheck()` 是内核内部函数（定义在 `arch/i386/memory.c:690`、`arch/earm/memory.c:595`），被 SIGSEND、GETINFO、DIAGCTL 等使用：

1. 如果 src/dst 是 SELF，替换为调用者 endpoint
2. 调用 `virtual_copy_vmcheck()` 执行实际拷贝
3. 如果 VM 暂停进程（VMSUSPEND），返回 EFAULT 并设置 VM 相关标志

### 1.3 VMSUSPEND 语义

当拷贝操作触发缺页时：
1. 进程进入 VMSUSPEND 状态（`RTS_VMSUSPEND`）
2. VM 被通知处理缺页
3. 缺页处理完成后，进程恢复并重试拷贝
4. **关键约束**：在 VMSUSPEND 之前修改的寄存器必须可恢复

---

## Ch2: C 源码分析

### syslib.h — sys_datacopy 宏

| 行号 | 定义 | 说明 |
|------|------|------|
| 129 | `#define sys_datacopy(p1, v1, p2, v2, len) sys_vircopy(p1, v1, p2, v2, len, 0)` | 用户空间简化接口，flags=0 |
| 130 | `#define sys_datacopy_try(p1, v1, p2, v2, len) sys_vircopy(p1, v1, p2, v2, len, CP_FLAG_TRY)` | 带重试的版本 |

> `sys_datacopy` 不是独立的内核调用。它展开为 `sys_vircopy`，发送 `SYS_VIRCOPY` 内核调用，由 `do_copy.c` 中的 `do_copy()` 处理。

### do_copy.c — SYS_VIRCOPY / SYS_PHYSCOPY 处理

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 1-16 | 头注释 | 声明处理 `SYS_VIRCOPY` 和 `SYS_PHYSCOPY` |
| 28-100 | `do_copy()` | 解析 src/dst segment+addr → virtual_copy |

### arch/i386/memory.c — data_copy_vmcheck

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 688-730 | `data_copy_vmcheck()` | SELF 替换 → virtual_copy_vmcheck → VMSUSPEND 处理 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | data_copy_vmcheck | 保留 vs 重构 | **保留接口** | VMSUSPEND 语义复杂 |
| D2 | VMSUSPEND 处理 | 返回码 vs Result 类型 | **`CopyResult` enum** | 区分 OK/EFAULT/VMSUSPEND |
| D3 | Direct Map 使用 | 逐页映射 vs 整段映射 | **整段映射** | 64-bit 地址空间有 Direct Map |
| D4 | 拷贝缓冲区 | 栈分配 vs 内核堆 | **栈分配（小量）/ 内核堆（大量）** | no_std 限制 |

---

## Ch4: 实现要点

### 4.1 CopyResult enum

```rust
/// Result of a cross-address-space copy operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// Copy completed successfully.
    Ok,
    /// Copy failed: bad address or access violation.
    Fault,
    /// Copy suspended: process is waiting for VM to handle page fault.
    VmSuspend,
}
```

### 4.2 data_copy_vmcheck

```rust
/// Kernel-internal cross-process copy with VM check.
/// C: `data_copy_vmcheck()` — arch/i386/memory.c:690
pub fn data_copy_vmcheck(
    caller: &mut KProcess,
    src_endpt: Endpoint,
    src_addr: u64,
    dst_endpt: Endpoint,
    dst_addr: u64,
    bytes: usize,
) -> CopyResult {
    // SELF replacement
    // virtual_copy_vmcheck → Direct Map copy
    // On page fault: set RTS_VMSUSPEND, return VmSuspend
}
```

### 4.3 与 SIGSEND 的交互

SIGSEND 使用 `data_copy_vmcheck` 拷贝 sigframe 到用户栈。如果返回 VmSuspend：
- **不能修改进程寄存器**（否则恢复后会重复修改）
- 等待 VM 处理缺页后重试

---

## 测试

- 单元：CopyResult enum
- 单元：SELF 替换逻辑
- 单元：零字节拷贝返回 Ok
- 单元：地址溢出检查

---

## 补充：VMREQUEST 挂起与恢复机制详细分析

> 来源：tmp-03-vm-request.md

### VMREQUEST 核心概念

当内核代用户态进程执行系统调用时，可能遇到**缺页**——目标虚拟地址对应的物理页不在内存中。内核没有自己的缺页处理程序，必须挂起操作、通知 VM、等待恢复。这个"挂起-通知-恢复"机制就是 VMREQUEST。

### VMSUSPEND 返回值

`VMSUSPEND`（-996）是内核内部的伪错误码，定义在 `kernel/vm.h:6`。它不是真正的错误，而是表示"操作因缺页挂起，需要等待 VM 处理后重试"。

```c
#define VMSUSPEND       (-996)
#define EFAULT_SRC      (-995)    // 源地址缺页
#define EFAULT_DST      (-994)    // 目标地址缺页
```

### 三种挂起类型

| 类型 | 值 | 含义 | 恢复行为 |
|------|---|------|---------|
| `VMSTYPE_KERNELCALL` | 1 | 内核调用被缺页中断 | 设置 `MF_KCALL_RESUME`，调度时重试内核调用 |
| `VMSTYPE_DELIVERMSG` | 2 | 消息投递被缺页中断 | VM 处理后直接解除 `RTS_VMREQUEST` |
| `VMSTYPE_MAP` | 3 | 预留类型，当前无代码设置 | 仅在 `do_vmctl.c:102` 有 case 分支 |

### vmrequest 全局链表

`vmrequest` 是全局单链表头指针，指向第一个被挂起的进程。每个被挂起进程通过 `p_vmrequest.nextrequestor` 字段链接。当新进程被挂起时插入链表头部。如果插入前链表为空，内核通过 `send_sig(VM_PROC_NR, SIGKMEM)` 通知 VM。

```
vmrequest → proc_A → proc_B → proc_C → NULL
```

### p_vmrequest 结构体

定义在 `kernel/proc.h:87-124`：

| 字段 | 用途 |
|------|------|
| `nextrequestor` | vmrequest 链表指针 |
| `type` | 被中断的操作类型（VMSTYPE_*） |
| `saved.reqmsg` | 被中断的内核调用消息 |
| `req_type` | 请求参数类型（VMPTYPE_*） |
| `target` | 目标进程 endpoint |
| `params.check.start/length/writeflag` | 内存范围检查参数 |
| `vmresult` | VM 处理结果 |

### VMCTL_MEMREQ_GET/REPLY 消息字段

| 字段 | 消息成员 | 含义 |
|------|---------|------|
| 目标进程 | `SVMCTL_MRG_TARGET` | 缺页地址所属的进程 endpoint |
| 缺页地址 | `SVMCTL_MRG_ADDR` | 缺页的起始虚拟地址 |
| 范围长度 | `SVMCTL_MRG_LENGTH` | 需要检查的内存范围长度 |
| 读写标志 | `SVMCTL_MRG_FLAG` | 非零表示写访问 |
| 请求者 | `SVMCTL_MRG_REQUESTOR` | 发起请求的进程 endpoint |

### VMREQUEST 完整生命周期

```
1. 触发：内核代进程执行操作 → 缺页
2. 挂起：vm_suspend() 保存上下文，排队通知
   设置 RTS_VMREQUEST → 进程不可调度
   填充 p_vmrequest 字段 → 保存缺页信息
   插入 vmrequest 链表 → send_sig(VM_PROC_NR, SIGKMEM)
3. 请求：VM 收到 SIGKMEM → 调用 sys_vmctl(VMCTL_MEMREQ_GET)
4. 处理：VM 处理缺页（映射物理页等）
5. 回复：VM 调用 sys_vmctl(VMCTL_MEMREQ_REPLY, result)
   内核设置 vmresult → 根据 type 设置恢复标志
   清除 RTS_VMREQUEST → 进程可再次调度
6. 恢复：调度器选中该进程
   switch_to_user() 检查 MF_KCALL_RESUME
   kernel_call_resume() → 重新执行内核调用
```

### VMREQUEST 与 PAGEFAULT 的区别

| 维度 | VMREQUEST | PAGEFAULT |
|------|-----------|-----------|
| 触发者 | 内核代进程操作时缺页 | 进程自身执行时缺页 |
| 通知 VM | `send_sig(SIGKMEM)` + `VMCTL_MEMREQ_GET` | `VM_PAGEFAULT` 通知消息 |
| 恢复方式 | `VMCTL_MEMREQ_REPLY` + 重试内核调用 | VM 映射页后清除 `RTS_PAGEFAULT` |
| 标志位 | `RTS_VMREQUEST` | `RTS_PAGEFAULT` |

### 相关常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `RTS_VMREQUEST` | 0x800 | 内存请求发起者，等待 VM 回复 |
| `RTS_VMREQTARGET` | 0x1000 | 已定义但从未使用 |
| `MF_KCALL_RESUME` | 0x008 | 内核调用被缺页中断，需要恢复 |
| `SIGKMEM` | 71 | 内核内存请求待处理信号 |

---

## 参见

- [18-syscall-copy.md](18-syscall-copy.md) — vircopy/safecopy 底层实现
- [19-syscall-signal.md](19-syscall-signal.md) — SIGSEND 使用 data_copy_vmcheck
