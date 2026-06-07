# 03-vm-request: VMREQUEST 挂起与恢复机制

> **分类**: Kernel 运行时内存请求管理
> **源码**: `kernel/proc.h`(p_vmrequest 结构体), `kernel/proc.c`(vm_suspend), `kernel/system.c`(kernel_call_finish/resume, clear_memreq), `kernel/system/do_vmctl.c`(VMCTL_MEMREQ_GET/REPLY), `kernel/glo.h`(vmrequest 链表), `kernel/vm.h`(VMSUSPEND), `kernel/arch/i386/memory.c`(check_resumed_caller, vm_check_range, virtual_copy_f)
> **说明**: 当内核代进程执行的内存操作（跨地址空间拷贝、消息投递、地址范围检查）遇到缺页时，内核不能自行处理——必须挂起当前操作，将请求排队通知 VM，等 VM 处理缺页后恢复。本文档分析这个挂起-通知-恢复的完整机制。跨地址空间拷贝的缺页触发场景见 02-page-table-kernel。

---

## 1. 概述

### 1.1 VMREQUEST 是什么

Minix3 内核代用户态进程执行系统调用时，经常需要访问进程的虚拟地址空间——例如 `sys_copy` 拷贝数据、`sys_sigsend` 投递信号帧、`sys_vumap` 检查地址范围。这些操作可能遇到**缺页**：目标虚拟地址对应的物理页不在内存中。

内核没有自己的缺页处理程序——页表管理是 VM 进程的职责。因此，当缺页发生时，内核必须：

1. **挂起**当前操作，保存恢复所需的上下文
2. **通知** VM 进程有内存请求需要处理
3. **等待** VM 处理完毕后**恢复**操作

这个"挂起-通知-恢复"机制就是 VMREQUEST。

### 1.2 与 02 文档的关系

02-page-table-kernel 分析了跨地址空间拷贝的**触发场景**——`lin_lin_copy` 如何通过 `PHYS_COPY_CATCH` 捕获缺页，`virtual_copy_f` 如何返回 `VMSUSPEND`。本文档聚焦 VMREQUEST 的**管理机制**：

- 挂起信息如何保存在 `p_vmrequest` 中
- 挂起进程如何通过 `vmrequest` 全局链表排队
- 内核如何通过 `SYS_VMCTL` 的 `VMCTL_MEMREQ_GET/REPLY` 与 VM 交互
- 恢复时如何通过 `MF_KCALL_RESUME` 重试被中断的内核调用

### 1.3 核心概念

#### 1.3.1 VMSUSPEND 返回值

`VMSUSPEND`（-996）是内核内部的伪错误码，定义在 `kernel/vm.h:6`。它不是真正的错误，而是表示"操作因缺页挂起，需要等待 VM 处理后重试"。

```c
// kernel/vm.h:6
#define VMSUSPEND       (-996)
```

当内核调用返回 `VMSUSPEND` 时，调用者进程被标记为 `RTS_VMREQUEST`，不再参与调度，直到 VM 回复。

#### 1.3.2 三种挂起类型

VMREQUEST 有三种挂起类型（`kernel/proc.h:98-101`），对应三种被缺页中断的内核操作：

| 类型 | 值 | 含义 | 恢复行为 |
|------|---|------|---------|
| `VMSTYPE_KERNELCALL` | 1 | 内核调用（sys_copy 等）被缺页中断 | 设置 `MF_KCALL_RESUME`，调度时重试内核调用 |
| `VMSTYPE_DELIVERMSG` | 2 | 消息投递（copy_msg_to_user）被缺页中断 | VM 处理后直接解除 `RTS_VMREQUEST` |
| `VMSTYPE_MAP` | 3 | 预留类型，当前无代码设置 | 仅在 `do_vmctl.c:102` 有 case 分支 |

**注意**：`VMSTYPE_MAP`（3）虽然在 `do_vmctl.c` 的 `VMCTL_MEMREQ_REPLY` 中有处理分支，但搜索整个内核代码，没有任何调用者将 `type` 设为 `VMSTYPE_MAP`。这是一个预留但未使用的类型。

#### 1.3.3 vmrequest 全局链表

`vmrequest`（`kernel/glo.h:41`）是一个全局单链表头指针，指向第一个被挂起的进程。每个被挂起进程通过 `p_vmrequest.nextrequestor` 字段链接到下一个。

```
vmrequest → proc_A → proc_B → proc_C → NULL
```

当新进程被挂起时，它被插入链表头部（`proc.c:254-257`）。如果插入前链表为空，内核通过 `send_sig(VM_PROC_NR, SIGKMEM)` 通知 VM 有新的内存请求。

#### 1.3.4 RTS_VMREQUEST 调度标志

`RTS_VMREQUEST`（0x800，`proc.h:153`）是进程调度标志之一。设置此标志的进程不参与调度——它正在等待 VM 处理内存请求。

相关标志位：
- `RTS_VMREQUEST`（0x800）：内存请求发起者，等待 VM 回复
- `RTS_VMREQTARGET`（0x1000，`proc.h:154`）：内存请求目标——**已定义但从未使用**（搜索全代码无 `RTS_SET.*VMREQTARGET`）
- `RTS_PAGEFAULT`（0x400）：进程自身触发了缺页异常（与 VMREQUEST 不同，PAGEFAULT 是进程自己缺页，VMREQUEST 是内核代进程操作时缺页）

#### 1.3.5 MF_KCALL_RESUME 杂项标志

`MF_KCALL_RESUME`（0x008，`proc.h:237`）表示"内核调用被缺页中断，需要恢复"。当 VM 回复内存请求且挂起类型为 `VMSTYPE_KERNELCALL` 时，此标志被设置（`do_vmctl.c:95`）。

在 `switch_to_user()`（`proc.c:356-360`）中，调度器检查此标志，如果设置则调用 `kernel_call_resume()` 重新执行被中断的内核调用。

### 1.4 完整生命周期

一个 VMREQUEST 的完整生命周期：

```
1. 触发：内核代进程执行操作 → 缺页
   virtual_copy_f() → lin_lin_copy() 返回 EFAULT_SRC/DST
   或 delivermsg() → copy_msg_to_user() 失败
   或 vm_check_range() 主动请求 VM 检查

2. 挂起：vm_suspend() 保存上下文，排队通知
   设置 RTS_VMREQUEST → 进程不可调度
   填充 p_vmrequest 字段 → 保存缺页信息
   插入 vmrequest 链表 → send_sig(VM_PROC_NR, SIGKMEM)

3. 请求：VM 收到 SIGKMEM → 调用 sys_vmctl(VMCTL_MEMREQ_GET)
   内核遍历 vmrequest 链表 → 返回第一个请求的详情
   从链表中移除该请求

4. 处理：VM 处理缺页（映射物理页等）

5. 回复：VM 调用 sys_vmctl(VMCTL_MEMREQ_REPLY, result)
   内核设置 vmresult → 根据 type 设置恢复标志
   清除 RTS_VMREQUEST → 进程可再次调度

6. 恢复：调度器选中该进程
   switch_to_user() 检查 MF_KCALL_RESUME
   kernel_call_resume() → 重新执行内核调用
   check_resumed_caller() → 返回 VM 的检查结果
```

### 1.5 与缺页异常（PAGEFAULT）的区别

| 维度 | VMREQUEST | PAGEFAULT |
|------|-----------|-----------|
| 触发者 | 内核代进程操作时缺页 | 进程自身执行时缺页 |
| 触发路径 | `lin_lin_copy` / `copy_msg_to_user` 返回 EFAULT | CPU 缺页异常 → 内核异常处理程序 |
| 挂起对象 | 发起操作的进程（caller） | 缺页进程自身 |
| 通知 VM | `send_sig(SIGKMEM)` + `VMCTL_MEMREQ_GET` | `VM_PAGEFAULT` 通知消息 |
| 恢复方式 | `VMCTL_MEMREQ_REPLY` + 重试内核调用 | VM 映射页后清除 `RTS_PAGEFAULT` |
| 标志位 | `RTS_VMREQUEST` | `RTS_PAGEFAULT` |

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 常量定义

**VMSUSPEND 伪错误码**（`kernel/vm.h:6`）：
```c
#define VMSUSPEND       (-996)
#define EFAULT_SRC	(-995)    // 源地址缺页
#define EFAULT_DST	(-994)    // 目标地址缺页
```

**VMSTYPE 挂起类型**（`kernel/proc.h:98-101`）：
```c
#define VMSTYPE_SYS_NONE	0   // 无操作（未使用）
#define VMSTYPE_KERNELCALL	1   // 内核调用被中断
#define VMSTYPE_DELIVERMSG	2   // 消息投递被中断
#define VMSTYPE_MAP		3   // 预留，未使用
```

**VMPTYPE 请求参数类型**（`include/minix/vm.h:37-38`）：
```c
#define VMPTYPE_NONE		0
#define VMPTYPE_CHECK		1   // 请求 VM 检查地址范围
```

**VMCTL 命令**（`include/minix/com.h:395-398`）：
```c
#define VMCTL_CLEAR_PAGEFAULT	12   // 清除缺页标志
#define VMCTL_MEMREQ_GET 	14   // VM 获取下一个内存请求
#define VMCTL_MEMREQ_REPLY	15   // VM 回复内存请求结果
```

**SIGKMEM 信号**（`sys/sys/signal.h:271`）：
```c
#define SIGKMEM          71    /* kernel memory request pending */
```

**调度标志**（`kernel/proc.h:153-154`）：
```c
#define RTS_VMREQUEST	0x800	/* originator of vm memory request */
#define RTS_VMREQTARGET	0x1000	/* target of vm memory request (未使用) */
```

**杂项标志**（`kernel/proc.h:237`）：
```c
#define MF_KCALL_RESUME 0x008	/* processing a kernel call was interrupted */
```

#### 2.1.2 SVMCTL 消息字段

`VMCTL_MEMREQ_GET` 返回时使用的消息字段（`include/minix/com.h:373-379`）：

| 字段 | 消息成员 | 含义 |
|------|---------|------|
| 目标进程 | `SVMCTL_MRG_TARGET` (m2_i1) | 缺页地址所属的进程 endpoint |
| 缺页地址 | `SVMCTL_MRG_ADDR` (m2_i2) | 缺页的起始虚拟地址 |
| 范围长度 | `SVMCTL_MRG_LENGTH` (m2_i3) | 需要检查的内存范围长度 |
| 读写标志 | `SVMCTL_MRG_FLAG` (m2_s1) | 非零表示写访问 |
| 源进程 | `SVMCTL_MRG_EP2` (m2_l1) | 请求的源进程（MEMREQ_GET 中未设置） |
| 源地址 | `SVMCTL_MRG_ADDR2` (m2_l2) | 请求的源地址（MEMREQ_GET 中未设置） |
| 请求者 | `SVMCTL_MRG_REQUESTOR` (m2_p1) | 发起请求的进程 endpoint |

### 2.2 核心数据结构

#### 2.2.1 p_vmrequest — VM 挂起请求（`kernel/proc.h:87-124`）

```c
struct {
    struct proc  *nextrestart;      /* next in vmrestart chain (未使用) */
    struct proc  *nextrequestor;    /* next in vmrequest chain */
    int           type;             /* suspended operation: VMSTYPE_* */
    union ixfer_saved {
        message  reqmsg;            /* suspended request message */
    } saved;
    int           req_type;         /* request parameter type: VMPTYPE_* */
    endpoint_t    target;           /* target process endpoint */
    union ixfer_params {
        struct {
            vir_bytes  start, length;   /* memory range */
            u8_t       writeflag;       /* nonzero for write access */
        } check;
    } params;
    int           vmresult;         /* VM result when available */
} p_vmrequest;
```

**字段说明**：

| 字段 | 用途 | 设置时机 |
|------|------|---------|
| `nextrestart` | vmrestart 链表指针 | **从未使用**——搜索全代码无赋值 |
| `nextrequestor` | vmrequest 链表指针 | `vm_suspend()` 中设置，指向原链表头 |
| `type` | 被中断的操作类型 | `vm_suspend()` 中设置，值为 `VMSTYPE_KERNELCALL` 或 `VMSTYPE_DELIVERMSG` |
| `saved.reqmsg` | 被中断的内核调用消息 | `kernel_call_finish()` 中保存，`kernel_call_resume()` 中恢复 |
| `req_type` | 请求参数类型 | `vm_suspend()` 中设为 `VMPTYPE_CHECK` |
| `target` | 缺页地址所属进程 | `vm_suspend()` 中设置 |
| `params.check` | 缺页地址范围和方向 | `vm_suspend()` 中设置 |
| `vmresult` | VM 处理结果 | `VMCTL_MEMREQ_GET` 时设为 `VMSUSPEND`，`VMCTL_MEMREQ_REPLY` 时设为 VM 的返回值 |

**未使用字段**：`nextrestart` 在代码中从未被赋值或读取，是遗留字段。

#### 2.2.2 vmrequest 全局链表（`kernel/glo.h:41`）

```c
EXTERN struct proc *vmrequest;  /* first process on vmrequest queue */
```

这是一个内核全局变量，指向 vmrequest 链表的第一个进程。链表通过 `p_vmrequest.nextrequestor` 串联。

**链表操作**：
- **插入**（`vm_suspend()`，`proc.c:254-257`）：新进程插入链表头部。如果插入前链表为空，发送 `SIGKMEM` 通知 VM。
- **移除**（`VMCTL_MEMREQ_GET`，`do_vmctl.c:74`）：从链表中摘除第一个匹配 IPC 过滤器的请求。
- **清理**（`clear_memreq()`，`system.c:488-503`）：进程退出时从链表中移除。
- **迁移**（`do_update()`，`do_update.c:320-339`）：进程更新时替换链表中的指针。

### 2.3 关键函数分析

#### 2.3.1 vm_suspend() — 挂起调用者（`kernel/proc.c:234-257`）

```c
void vm_suspend(struct proc *caller, const struct proc *target,
        const vir_bytes linaddr, const vir_bytes len, const int type,
        const int writeflag)
{
    assert(!RTS_ISSET(caller, RTS_VMREQUEST));
    assert(!RTS_ISSET(target, RTS_VMREQUEST));

    RTS_SET(caller, RTS_VMREQUEST);

    caller->p_vmrequest.req_type = VMPTYPE_CHECK;
    caller->p_vmrequest.target = target->p_endpoint;
    caller->p_vmrequest.params.check.start = linaddr;
    caller->p_vmrequest.params.check.length = len;
    caller->p_vmrequest.params.check.writeflag = writeflag;
    caller->p_vmrequest.type = type;

    if(!(caller->p_vmrequest.nextrequestor = vmrequest))
        if(OK != send_sig(VM_PROC_NR, SIGKMEM))
            panic("send_sig failed");
    vmrequest = caller;
}
```

**行为要点**：

1. **断言**：caller 和 target 都不能已经在 vmrequest 链表中——一个进程同一时刻只能有一个挂起的内存请求
2. **设置 RTS_VMREQUEST**：进程不可调度
3. **填充请求参数**：`req_type` 固定为 `VMPTYPE_CHECK`，`type` 由调用者指定（`VMSTYPE_KERNELCALL` 或 `VMSTYPE_DELIVERMSG`）
4. **链表插入**：将 caller 插入 vmrequest 链表头部。关键细节——`if(!(caller->p_vmrequest.nextrequestor = vmrequest))` 的含义是：如果原链表为空（`vmrequest == NULL`），则 `nextrequestor` 被赋值为 NULL（条件为真），发送 `SIGKMEM` 通知 VM。如果原链表非空，`nextrequestor` 指向原链表头，不发送信号（VM 已经知道有请求待处理）
5. **更新链表头**：`vmrequest = caller`

**调用者**（x86 架构）：

| 调用位置 | 传入 type | 场景 |
|---------|----------|------|
| `memory.c:444` vm_check_range | `VMSTYPE_KERNELCALL` | 地址范围检查缺页 |
| `memory.c:566` vm_memset | `VMSTYPE_KERNELCALL` | 物理内存填充缺页 |
| `memory.c:661` virtual_copy_f | `VMSTYPE_KERNELCALL` | 跨地址空间拷贝缺页 |
| `proc.c:281` delivermsg | `VMSTYPE_DELIVERMSG` | 消息投递缺页 |

#### 2.3.2 kernel_call_finish() — 内核调用完成处理（`kernel/system.c:59-98`）

```c
static void kernel_call_finish(struct proc * caller, message *msg, int result)
{
    if(result == VMSUSPEND) {
        assert(RTS_ISSET(caller, RTS_VMREQUEST));
        assert(caller->p_vmrequest.type == VMSTYPE_KERNELCALL);
        caller->p_vmrequest.saved.reqmsg = *msg;
        caller->p_misc_flags |= MF_KCALL_RESUME;
    } else {
        caller->p_vmrequest.saved.reqmsg.m_source = NONE;
        if (result != EDONTREPLY) {
            msg->m_source = SYSTEM;
            msg->m_type = result;
            if (copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)) {
                printf("WARNING wrong user pointer ...\n");
                cause_sig(proc_nr(caller), SIGSEGV);
            }
        }
    }
}
```

**行为要点**：

1. **VMSUSPEND 路径**：保存原始请求消息到 `p_vmrequest.saved.reqmsg`，设置 `MF_KCALL_RESUME` 标志。进程已被 `vm_suspend()` 标记为 `RTS_VMREQUEST`，不会返回用户态
2. **正常完成路径**：清除保存的消息（`m_source = NONE`），将结果拷贝回用户态。如果拷贝失败，发送 SIGSEGV

#### 2.3.3 VMCTL_MEMREQ_GET — VM 获取内存请求（`kernel/system/do_vmctl.c:37-79`）

```c
case VMCTL_MEMREQ_GET:
    for (rpp = &vmrequest; *rpp != NULL;
        rpp = &(*rpp)->p_vmrequest.nextrequestor) {
        rp = *rpp;
        assert(RTS_ISSET(rp, RTS_VMREQUEST));
        okendpt(rp->p_vmrequest.target, &proc_nr);
        target = proc_addr(proc_nr);

        if (!allow_ipc_filtered_memreq(rp, target))
            continue;

        if (rp->p_vmrequest.req_type != VMPTYPE_CHECK)
            panic("VMREQUEST wrong type");

        m_ptr->SVMCTL_MRG_TARGET   = rp->p_vmrequest.target;
        m_ptr->SVMCTL_MRG_ADDR     = rp->p_vmrequest.params.check.start;
        m_ptr->SVMCTL_MRG_LENGTH   = rp->p_vmrequest.params.check.length;
        m_ptr->SVMCTL_MRG_FLAG     = rp->p_vmrequest.params.check.writeflag;
        m_ptr->SVMCTL_MRG_REQUESTOR = (void *) rp->p_endpoint;

        rp->p_vmrequest.vmresult = VMSUSPEND;
        *rpp = rp->p_vmrequest.nextrequestor;
        return rp->p_vmrequest.req_type;
    }
    return ENOENT;
```

**行为要点**：

1. **遍历链表**：从头到尾遍历 vmrequest 链表，寻找第一个通过 IPC 过滤器的请求
2. **IPC 过滤器**：`allow_ipc_filtered_memreq()` 检查 VM 是否允许接收来自该请求者/目标的请求。VM 在 update 操作期间可能设置 IPC 过滤器，阻止某些请求
3. **返回请求详情**：将缺页信息填入回复消息
4. **设置 vmresult**：`VMSUSPEND` 表示"请求已被取出但尚未处理"
5. **从链表移除**：`*rpp = rp->p_vmrequest.nextrequestor` 将该请求从链表中摘除
6. **返回值**：`VMPTYPE_CHECK`（1）表示这是一个地址检查请求，`ENOENT` 表示没有待处理请求

#### 2.3.4 VMCTL_MEMREQ_REPLY — VM 回复内存请求（`kernel/system/do_vmctl.c:81-109`）

```c
case VMCTL_MEMREQ_REPLY:
    assert(RTS_ISSET(p, RTS_VMREQUEST));
    assert(p->p_vmrequest.vmresult == VMSUSPEND);
    okendpt(p->p_vmrequest.target, &proc_nr);
    target = proc_addr(proc_nr);
    p->p_vmrequest.vmresult = m_ptr->SVMCTL_VALUE;
    assert(p->p_vmrequest.vmresult != VMSUSPEND);

    switch(p->p_vmrequest.type) {
    case VMSTYPE_KERNELCALL:
        p->p_misc_flags |= MF_KCALL_RESUME;
        break;
    case VMSTYPE_DELIVERMSG:
        assert(p->p_misc_flags & MF_DELIVERMSG);
        assert(p == target);
        assert(RTS_ISSET(p, RTS_VMREQUEST));
        break;
    case VMSTYPE_MAP:
        assert(RTS_ISSET(p, RTS_VMREQUEST));
        break;
    default:
        panic("strange request type: %d", p->p_vmrequest.type);
    }

    RTS_UNSET(p, RTS_VMREQUEST);
    return OK;
```

**行为要点**：

1. **断言**：进程必须在 VMREQUEST 状态，且 vmresult 必须为 VMSUSPEND（表示请求已被取出但未回复）
2. **设置 vmresult**：VM 的处理结果（OK 或 EFAULT 等），不能为 VMSUSPEND
3. **按类型设置恢复标志**：
   - `VMSTYPE_KERNELCALL`：设置 `MF_KCALL_RESUME`，调度时重试内核调用
   - `VMSTYPE_DELIVERMSG`：仅断言检查，不设额外标志——下次调度时 `delivermsg()` 会重试
   - `VMSTYPE_MAP`：仅断言检查
4. **清除 RTS_VMREQUEST**：进程可再次调度

#### 2.3.5 kernel_call_resume() — 恢复被中断的内核调用（`kernel/system.c:612-636`）

```c
void kernel_call_resume(struct proc *caller)
{
    int result;
    assert(!RTS_ISSET(caller, RTS_VMREQUEST));
    assert(caller->p_vmrequest.saved.reqmsg.m_source == caller->p_endpoint);

    result = kernel_call_dispatch(caller, &caller->p_vmrequest.saved.reqmsg);
    caller->p_misc_flags &= ~MF_KCALL_RESUME;
    kernel_call_finish(caller, &caller->p_vmrequest.saved.reqmsg, result);
}
```

**行为要点**：

1. **断言**：进程已不在 VMREQUEST 状态（`RTS_VMREQUEST` 已在 `VMCTL_MEMREQ_REPLY` 中清除）
2. **重新执行**：用保存的原始请求消息重新调用 `kernel_call_dispatch()`
3. **清除 MF_KCALL_RESUME**：防止下次调度时再次恢复
4. **完成处理**：调用 `kernel_call_finish()` 处理结果——如果再次 VMSUSPEND，则再次挂起

**重试语义**：内核调用可能再次缺页（例如拷贝跨多个页面），此时 `kernel_call_finish()` 会再次保存消息、设置 `MF_KCALL_RESUME`，形成"挂起→恢复→重试→可能再挂起"的循环。

#### 2.3.6 check_resumed_caller() — 检查恢复结果（`kernel/arch/i386/memory.c:135-145`）

```c
static int check_resumed_caller(struct proc *caller)
{
    if (caller && (caller->p_misc_flags & MF_KCALL_RESUME)) {
        assert(caller->p_vmrequest.vmresult != VMSUSPEND);
        return caller->p_vmrequest.vmresult;
    }
    return OK;
}
```

**行为要点**：

1. 如果调用者带有 `MF_KCALL_RESUME` 标志（从 VMSUSPEND 恢复），直接返回 VM 的检查结果
2. 如果 VM 返回 OK，表示缺页已处理，操作可以继续
3. 如果 VM 返回 EFAULT，表示地址无效，操作应失败

**调用位置**：`virtual_copy_f()`（`memory.c:643`）和 `vm_check_range()`（`memory.c:440`）在操作开始时调用此函数，检查上次 VMSUSPEND 的结果。

#### 2.3.7 vm_check_range() — 主动请求 VM 检查地址范围（`kernel/arch/i386/memory.c:427-447`）

```c
int vm_check_range(struct proc *caller, struct proc *target,
    vir_bytes vir_addr, size_t bytes, int writeflag)
{
    int r;
    if ((caller->p_misc_flags & MF_KCALL_RESUME) &&
            (r = caller->p_vmrequest.vmresult) != OK)
        return r;

    vm_suspend(caller, target, vir_addr, bytes, VMSTYPE_KERNELCALL,
        writeflag);
    return VMSUSPEND;
}
```

**行为要点**：

1. **首次调用**：直接调用 `vm_suspend()`，返回 `VMSUSPEND`
2. **恢复后重试**：如果 `MF_KCALL_RESUME` 已设置且 VM 返回错误，直接返回该错误；如果 VM 返回 OK，继续执行 `vm_suspend()` 再次检查（但此时 VM 应该已经映射了页面，不会再次 VMSUSPEND）

**设计意图**：`vm_check_range()` 是"先检查再操作"模式——在执行拷贝之前先确保地址范围合法。这与 `virtual_copy_f()` 的"操作时捕获缺页"模式不同。

#### 2.3.8 delivermsg() — 消息投递与 VMSUSPEND（`kernel/proc.c:265-297`）

```c
static void delivermsg(struct proc *rp)
{
    assert(!RTS_ISSET(rp, RTS_VMREQUEST));
    assert(rp->p_misc_flags & MF_DELIVERMSG);
    assert(rp->p_delivermsg.m_source != NONE);

    if (copy_msg_to_user(&rp->p_delivermsg,
                            (message *) rp->p_delivermsg_vir)) {
        if(rp->p_misc_flags & MF_MSGFAILED) {
            printf("WARNING wrong user pointer ...\n");
            cause_sig(rp->p_nr, SIGSEGV);
        } else {
            vm_suspend(rp, rp, rp->p_delivermsg_vir,
                sizeof(message), VMSTYPE_DELIVERMSG, 1);
            rp->p_misc_flags |= MF_MSGFAILED;
        }
    } else {
        rp->p_delivermsg.m_source = NONE;
        rp->p_misc_flags &= ~(MF_DELIVERMSG|MF_MSGFAILED);
        if(!(rp->p_misc_flags & MF_CONTEXT_SET)) {
            rp->p_reg.retreg = OK;
        }
    }
}
```

**行为要点**：

1. **首次失败**：`copy_msg_to_user()` 失败，调用 `vm_suspend()` 挂起，设置 `MF_MSGFAILED`
2. **二次失败**：如果 `MF_MSGFAILED` 已设置且再次失败，发送 SIGSEGV——说明不是缺页而是非法指针
3. **成功**：清除投递标志

**与 VMSTYPE_KERNELCALL 的区别**：`VMSTYPE_DELIVERMSG` 恢复时不设置 `MF_KCALL_RESUME`，而是依赖 `MF_DELIVERMSG` 标志——`switch_to_user()` 在处理 `MF_DELIVERMSG` 时会再次调用 `delivermsg()` 重试。

#### 2.3.9 clear_memreq() — 清理进程的内存请求（`kernel/system.c:488-503`）

```c
static void clear_memreq(struct proc *rp)
{
    struct proc **rpp;
    if (!RTS_ISSET(rp, RTS_VMREQUEST))
        return;

    for (rpp = &vmrequest; *rpp != NULL;
       rpp = &(*rpp)->p_vmrequest.nextrequestor) {
        if (*rpp == rp) {
            *rpp = rp->p_vmrequest.nextrequestor;
            break;
        }
    }
    RTS_UNSET(rp, RTS_VMREQUEST);
}
```

**行为要点**：进程退出时（`clear_endpoint()` 调用链），需要从 vmrequest 链表中移除该进程，并清除 `RTS_VMREQUEST` 标志。不通知 VM——VM 下次 `VMCTL_MEMREQ_GET` 时自然不会再看到该进程。

#### 2.3.10 switch_to_user() 中的恢复检查（`kernel/proc.c:356-360`）

```c
while (p->p_misc_flags &
    (MF_KCALL_RESUME | MF_DELIVERMSG |
     MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE)) {

    if (p->p_misc_flags & MF_KCALL_RESUME) {
        kernel_call_resume(p);
    }
    else if (p->p_misc_flags & MF_DELIVERMSG) {
        delivermsg(p);
    }
    // ... 其他标志处理 ...

    if (!proc_is_runnable(p))
        goto not_runnable_pick_new;
}
```

**行为要点**：调度器选中进程后，在返回用户态之前检查杂项标志。`MF_KCALL_RESUME` 优先于 `MF_DELIVERMSG`。每次处理后检查进程是否仍然可运行——`kernel_call_resume()` 可能再次 VMSUSPEND。

### 2.4 调用关系分析

#### 2.4.1 VMREQUEST 触发路径

```
用户态进程 → 内核调用
  │
  ├─ SYS_COPY/SYS_SAFECOPY → virtual_copy_vmcheck() → virtual_copy_f()
  │   ├─ lin_lin_copy() 缺页 → EFAULT_SRC/DST
  │   └─ vm_suspend(caller, target, ..., VMSTYPE_KERNELCALL, ...)
  │       → return VMSUSPEND
  │       → kernel_call_finish() 保存消息, 设置 MF_KCALL_RESUME
  │
  ├─ SYS_VMCTL(VMCTL_MEMREQ_GET) → VM 取请求
  │   → 遍历 vmrequest 链表 → 返回请求详情
  │   → 从链表移除
  │
  ├─ VM 处理缺页 → sys_vmctl(VMCTL_MEMREQ_REPLY, result)
  │   → 设置 vmresult → 设置 MF_KCALL_RESUME → 清除 RTS_VMREQUEST
  │
  └─ 调度器选中进程 → switch_to_user()
      → MF_KCALL_RESUME → kernel_call_resume()
      → 重新执行内核调用 → check_resumed_caller() 返回 VM 结果
```

#### 2.4.2 DELIVERMSG 触发路径

```
调度器选中进程 → switch_to_user()
  → MF_DELIVERMSG → delivermsg()
    → copy_msg_to_user() 失败
    → vm_suspend(rp, rp, ..., VMSTYPE_DELIVERMSG, ...)
    → 进程不可调度

VM 回复 → VMCTL_MEMREQ_REPLY
  → type == VMSTYPE_DELIVERMSG → 不设 MF_KCALL_RESUME
  → 清除 RTS_VMREQUEST → 进程可调度

调度器再次选中 → switch_to_user()
  → MF_DELIVERMSG 仍设置 → delivermsg() 重试
  → 成功：清除 MF_DELIVERMSG
  → 再次失败 + MF_MSGFAILED：SIGSEGV
```

#### 2.4.3 vm_check_range 触发路径

```
内核调用 (sys_vumap, sys_sigsend 等)
  → vm_check_range(caller, target, addr, len, writeflag)
    → 首次：vm_suspend() → return VMSUSPEND
    → 恢复后：check_resumed_caller() → 返回 VM 结果
```

### 2.5 设计要点与特殊处理

#### 2.5.1 IPC 过滤器与内存请求

`allow_ipc_filtered_memreq()`（`system.c:879-916`）在 `VMCTL_MEMREQ_GET` 中过滤请求。当 VM 正在执行 update 操作时，它可能设置了 IPC 过滤器，只允许与特定进程通信。此时内核跳过被过滤的请求，返回下一个允许的请求。

如果 VM 清除了 IPC 过滤器（`system.c:768-770`），且 vmrequest 链表非空，内核主动发送 `SIGKMEM` 通知 VM——因为之前被过滤的请求现在可以被处理了。

#### 2.5.2 进程更新时的链表迁移

`do_update()`（`do_update.c:323-339`）在进程更新（live update）时，需要将 vmrequest 链表中的进程指针从旧进程替换为新进程。如果只有一个进程在链表中，替换指针；如果两个都在，不需要操作（相对位置不变）。

#### 2.5.3 MF_KCALL_RESUME 的双重作用

`MF_KCALL_RESUME` 有两个使用场景：

1. **VMSUSPEND 恢复**：`VMCTL_MEMREQ_REPLY` 设置，`kernel_call_resume()` 使用
2. **长时间拷贝被抢占**：`kernel_call_finish()` 注释提到"a long running copy was preempted"——当内核调用执行时间过长被时钟中断抢占时，也通过此标志恢复

在 `check_resumed_caller()` 中，两种场景统一处理：如果 `MF_KCALL_RESUME` 已设置，检查 `vmresult`。

#### 2.5.4 vmresult 的三态语义

`vmresult` 字段有三个语义状态：

| vmresult 值 | 含义 | 设置时机 |
|-------------|------|---------|
| 初始值（0） | 未使用 | 进程初始化 |
| VMSUSPEND (-996) | 请求已被 VM 取出，等待回复 | `VMCTL_MEMREQ_GET` |
| OK/EFAULT 等 | VM 的处理结果 | `VMCTL_MEMREQ_REPLY` |

注意：`VMCTL_MEMREQ_REPLY` 中断言 `vmresult != VMSUSPEND`，确保不会将 VMSUSPEND 作为最终结果。

#### 2.5.5 SIGKMEM 的发送时机

`SIGKMEM` 仅在 vmrequest 链表**从空变为非空**时发送（`vm_suspend()` 中 `if(!(nextrequestor = vmrequest))`）。后续的挂起操作只将进程加入链表，不重复发送信号——VM 在处理完当前请求后会循环调用 `VMCTL_MEMREQ_GET` 直到返回 `ENOENT`。

VM 端的 `do_memory()`（`pagefaults.c:294`）使用 `while(1)` 循环不断获取请求，直到 `sys_vmctl_get_memreq()` 返回非正值（`ENOENT` 或错误）。

#### 2.5.6 未使用的字段和类型

| 符号 | 位置 | 状态 |
|------|------|------|
| `nextrestart` | `proc.h:96` | 已声明，从未赋值或读取 |
| `VMSTYPE_SYS_NONE` | `proc.h:98` | 已定义，从未使用 |
| `VMSTYPE_MAP` | `proc.h:101` | 已定义，有 case 分支但无代码设置此类型 |
| `RTS_VMREQTARGET` | `proc.h:154` | 已定义，从未 `RTS_SET`/`RTS_UNSET` |
| `VMPTYPE_NONE` | `include/minix/vm.h:37` | 已定义，从未使用 |

这些未使用的符号在 minix-rs 中不需要实现，但应在文档中注明它们在 Minix3 中的状态。

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"，每个决策追溯 Ch1&Ch2 的依据。
> 共享约束：00-kernel-overview.md §1.5（BKL + SMP + 中断，`Rc`/`RefCell` 限制，硬件操作必须 trait 抽象）。
> 前置文档：02-page-table-kernel.md §3.7（VMSUSPEND 语义保留，实现重新表达）已定义 `VmCopyError`/`VmFaultType`/`VmRequest`，本节在此基础上扩展。

### 3.1 VmSuspendType 枚举替代 VMSTYPE_* 宏

**决策**：用 `VmSuspendType` 枚举替代 Minix3 的 `VMSTYPE_KERNELCALL`/`VMSTYPE_DELIVERMSG`/`VMSTYPE_MAP` 宏，不实现 `VMSTYPE_SYS_NONE`（从未使用）和 `VMSTYPE_MAP`（有 case 分支但无代码设置此类型）。

**依据**：§2.1.1 分析了 `VMSTYPE_*` 宏定义（`proc.h:98-101`）。§2.5.6 确认 `VMSTYPE_SYS_NONE` 和 `VMSTYPE_MAP` 从未使用。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendType {
    KernelCall,
    DeliverMsg,
}
```

**与 Minix3 的对照**：

| Minix3 | minix-rs | 说明 |
|--------|---------|------|
| `VMSTYPE_KERNELCALL` (1) | `VmSuspendType::KernelCall` | 内核调用被中断 |
| `VMSTYPE_DELIVERMSG` (2) | `VmSuspendType::DeliverMsg` | 消息投递被中断 |
| `VMSTYPE_MAP` (3) | 不实现 | 有 case 分支但无代码设置此类型（§2.5.6） |
| `VMSTYPE_SYS_NONE` (0) | 不实现 | 从未使用（§2.5.6） |

**为什么不实现 VMSTYPE_MAP**：虽然 `VMCTL_MEMREQ_REPLY` 处理中有 `case VMSTYPE_MAP` 分支（`do_vmctl.c:102-103`），但搜索整个内核代码，没有任何地方调用 `vm_suspend(..., VMSTYPE_MAP, ...)`。这是一个预留但从未使用的类型。Rust 枚举不需要预留变体——如果将来需要，添加变体是编译器强制检查的破坏性变更，不会遗漏处理。

### 3.2 VmSuspendContext 替代 p_vmrequest 匿名结构体

**决策**：用 `VmSuspendContext` 结构体替代 Minix3 的 `p_vmrequest` 匿名结构体，将 C 的"全字段平铺+哨兵值"模式改为 Rust 的"Option + 枚举 + 类型状态"模式。

**依据**：§2.2.1 分析了 `p_vmrequest` 的全部字段。§2.3.1 分析了 `vm_suspend()` 如何填充这些字段。§2.3.4 分析了 `VMCTL_MEMREQ_REPLY` 如何读取 `vmresult` 并根据 `type` 分派恢复逻辑。

**C 结构体的问题**：

1. **`vmresult` 哨兵值**：`0` = 未处理，`VMSUSPEND` = 已取出待回复，其他 = VM 回复结果。三态语义隐含在整数值中，编译器无法检查。
2. **`type` 与 `saved`/`params` 的隐式关联**：`type == VMSTYPE_KERNELCALL` 时 `saved.reqmsg` 有效，`type == VMSTYPE_DELIVERMSG` 时 `saved.reqmsg` 也有效但含义不同。字段有效性依赖运行时 `type` 值。
3. **`nextrequestor` 裸指针**：链表操作通过 `*proc` 指针，无所有权语义。
4. **`nextrestart` 从未使用**：已声明但从未赋值或读取（§2.5.6）。

**Rust 设计**：

```rust
pub struct VmSuspendContext {
    pub suspend_type: VmSuspendType,
    pub target: Endpoint,
    pub check_params: VmCheckParams,
    pub state: VmSuspendState,
    pub saved_msg: Option<Message>,
}
```

**字段映射**：

| p_vmrequest 字段 | VmSuspendContext 字段 | 变化说明 |
|-----------------|----------------------|---------|
| `type` (int) | `suspend_type: VmSuspendType` | 枚举替代 int 宏 |
| `target` (endpoint_t) | `target: Endpoint` | 类型不变 |
| `params.check.start/length/writeflag` | `check_params: VmCheckParams` | 提取为独立结构体 |
| `vmresult` (int) | `state: VmSuspendState` | 枚举替代三态哨兵值（见 §3.3） |
| `saved.reqmsg` (message) | `saved_msg: Option<Message>` | `Some` = 有保存的消息，`None` = 无 |
| `req_type` (int) | 消除 | 固定为 `VMPTYPE_CHECK`（§2.5.6 确认 `VMPTYPE_NONE` 从未使用） |
| `nextrequestor` (*proc) | 移到 `VmRequestQueue`（见 §3.4） | 链表管理独立于进程上下文 |
| `nextrestart` (*proc) | 不实现 | 从未使用（§2.5.6） |

**VmCheckParams 提取**：

```rust
#[derive(Debug, Clone, Copy)]
pub struct VmCheckParams {
    pub start: VirBytes,
    pub length: VirBytes,
    pub write_flag: bool,
}
```

Minix3 的 `params.check.writeflag` 是 `u8_t`（`0` = 读，非零 = 写）。Rust 用 `bool` 表达——`false` = 读，`true` = 写。消除"非零即写"的隐式约定。

**saved_msg 的 Option 语义**：`VMSTYPE_KERNELCALL` 时 `saved.reqmsg` 保存被中断的系统调用消息（`kernel_call_finish` 中填充）。`VMSTYPE_DELIVERMSG` 时 `saved.reqmsg` 保存投递中的消息（`delivermsg` 中填充）。两者都需要保存消息，但 `Option<Message>` 允许表达"尚未保存"的状态——在 `vm_suspend` 调用之前，`saved_msg` 为 `None`。

**为什么 req_type 消除**：§2.1.1 分析了 `VMPTYPE_CHECK` 和 `VMPTYPE_NONE`。§2.5.6 确认 `VMPTYPE_NONE` 从未使用，`vm_suspend()` 中硬编码 `caller->p_vmrequest.req_type = VMPTYPE_CHECK`。既然只有一个有效值，该字段无信息量，消除。

### 3.3 VmSuspendState 枚举替代 vmresult 三态哨兵值

**决策**：用 `VmSuspendState` 枚举替代 Minix3 的 `vmresult` 字段的三态哨兵值模式。

**依据**：§2.3.3 分析了 `VMCTL_MEMREQ_GET` 将 `vmresult` 设为 `VMSUSPEND`（标记"已取出待回复"）。§2.3.4 分析了 `VMCTL_MEMREQ_REPLY` 断言 `vmresult == VMSUSPEND` 后写入 VM 的回复值。§2.3.6 分析了 `check_resumed_caller()` 通过 `vmresult` 判断 VM 的处理结果。

**Minix3 的 vmresult 三态**：

| vmresult 值 | 含义 | 设置时机 |
|------------|------|---------|
| `0` | 初始/未处理 | `vm_suspend()` 中初始化 |
| `VMSUSPEND` (-996) | 已取出待回复 | `VMCTL_MEMREQ_GET` 中设置 |
| 其他值 | VM 回复结果 | `VMCTL_MEMREQ_REPLY` 中设置 |

**Rust 设计**：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendState {
    Pending,
    Fetched,
    Completed(VmCheckResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCheckResult {
    Ok,
    Fault,
}
```

**状态转换**：

```
Pending ──VMCTL_MEMREQ_GET──▶ Fetched ──VMCTL_MEMREQ_REPLY──▶ Completed(Ok/Fault)
```

**与 Minix3 的对照**：

| vmresult 值 | VmSuspendState | 说明 |
|------------|---------------|------|
| `0` | `Pending` | 初始状态，等待 VM 获取 |
| `VMSUSPEND` (-996) | `Fetched` | VM 已获取请求，等待回复 |
| `OK` (0) | `Completed(VmCheckResult::Ok)` | VM 确认地址有效 |
| `EFAULT` 等 | `Completed(VmCheckResult::Fault)` | VM 确认地址无效 |

**为什么不用 `Option<Result<(), VmError>>`**：02-page-table-kernel.md §4.3b 的 `VmRequest.fault_type: Option<VmFaultType>` 使用了 `Option` 模式。但 `vmresult` 有三个状态（未处理/已取出/已完成），`Option` 只能表达两个。`Option<Result<...>>` 可以表达"无/成功/失败"三态，但无法表达"已取出"这个中间状态——而 `VMCTL_MEMREQ_GET` 设置 `vmresult = VMSUSPEND` 正是标记"请求已被 VM 取走"的关键步骤，用于防止重复获取。

**VmCheckResult 的设计**：Minix3 的 `vmresult` 在 `VMCTL_MEMREQ_REPLY` 后可以是任意 errno（`OK`/`EFAULT`/`ENOMEM` 等）。但 `check_resumed_caller()`（`memory.c:135-145`）只检查 `vmresult != OK`——它不关心具体错误类型，只关心"成功还是失败"。因此 `VmCheckResult` 简化为 `Ok`/`Fault` 两个变体。如果将来需要区分具体错误码，可以扩展 `Fault(VmFaultError)` 变体。

### 3.4 VmRequestQueue 替代裸指针链表

**决策**：用 `VmRequestQueue` 结构体替代 Minix3 的 `vmrequest` 裸指针链表，使用 `ProcNr` 索引替代 `*proc` 指针。

**依据**：§2.2.2 分析了 `vmrequest` 全局链表头（`glo.h:41`）。§2.3.1 分析了 `vm_suspend()` 的链表插入逻辑——头插法，插入时判断链表是否从空变为非空以决定是否发送 `SIGKMEM`。§2.3.3 分析了 `VMCTL_MEMREQ_GET` 的链表遍历和节点移除逻辑。

**Minix3 的链表问题**：

1. **裸指针**：`nextrequestor` 是 `struct proc *`，直接指向进程表中的进程。进程更新（live update）时需要遍历链表替换指针（§2.5.2）。
2. **全局可变状态**：`vmrequest` 是 `EXTERN struct proc *`，任何内核代码都可以修改。
3. **无所有权语义**：链表节点的生命周期与进程表绑定，但没有类型系统保证。

**Rust 设计**：

```rust
pub struct VmRequestQueue {
    head: Option<ProcNr>,
}
```

**为什么用 ProcNr 索引而非裸指针**：内核进程表是固定大小的数组（`proc[NR_PROCS + NR_TASKS]`），进程通过 `ProcNr`（槽位索引）访问。使用索引替代指针有以下好处：

1. **避免指针失效**：进程更新（live update）时，新旧进程在同一槽位，索引自动指向新进程，无需遍历链表替换指针（§2.5.2 的 `do_update` 链表迁移逻辑可以消除）。
2. **与现有代码一致**：`KProcess.p_nextready`/`p_caller_q`/`p_q_link` 已经使用 `Option<ProcNr>`（`proc.rs:445-453`）。
3. **BKL 下的安全**：索引访问进程表需要 BKL 保护，但索引本身是 `Copy` 类型，不存在 `&mut` 借用冲突。

**链表节点**：`next_requestor` 字段从 `p_vmrequest` 移到 `KProcess` 中（与 `p_nextready`/`p_caller_q` 同级），因为链表管理是进程调度层面的操作，不属于挂起上下文。

```rust
pub struct KProcess {
    // ... existing fields ...
    pub p_nextready: Option<ProcNr>,
    pub p_caller_q: Option<ProcNr>,
    pub p_q_link: Option<ProcNr>,
    pub p_next_requestor: Option<ProcNr>,  // 新增
    pub p_vm_suspend: Option<VmSuspendContext>,  // 新增
}
```

**关键方法**：

```rust
impl VmRequestQueue {
    pub fn enqueue(&mut self, proc_nr: ProcNr, procs: &mut [KProcess]) -> bool {
        let proc = &mut procs[proc_nr as usize];
        proc.p_next_requestor = self.head;
        let was_empty = self.head.is_none();
        self.head = Some(proc_nr);
        was_empty
    }

    pub fn dequeue_filtered<F>(
        &mut self,
        procs: &[KProcess],
        filter: F,
    ) -> Option<ProcNr>
    where
        F: Fn(&KProcess, &KProcess) -> bool,
    {
        let mut prev: Option<&mut Option<ProcNr>> = None;
        // ... 遍历链表，找到第一个通过过滤器的节点 ...
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }
}
```

**`enqueue` 返回 `bool`**：对应 Minix3 的 `if(!(caller->p_vmrequest.nextrequestor = vmrequest))` 判断——链表从空变为非空时返回 `true`，调用者据此决定是否发送 `SIGKMEM`。

**`dequeue_filtered`**：对应 `VMCTL_MEMREQ_GET` 中的 `allow_ipc_filtered_memreq` 过滤逻辑（§2.5.1）。VM 可能设置了 IPC 过滤器，只允许与特定进程通信。内核跳过被过滤的请求，返回第一个允许的请求。

**BKL 安全分析**（参考 00-kernel-overview §1.5）：

| 操作 | BKL 状态 | 安全 |
|------|---------|------|
| `enqueue`（`vm_suspend` 中调用） | 系统调用处理全程持有 BKL | BKL 保护 |
| `dequeue_filtered`（`VMCTL_MEMREQ_GET` 中调用） | 系统调用处理全程持有 BKL | BKL 保护 |
| 链表遍历 | 系统调用处理全程持有 BKL | BKL 保护 |
| IPC 过滤器清除后发送 SIGKMEM（`system.c:768`） | BKL 持有 | BKL 保护 |

`VmRequestQueue` 的所有操作都在 BKL 保护下执行，不需要额外的同步机制。但 `VmRequestQueue` 本身不应使用 `Rc`/`RefCell`——因为 BKL 释放窗口内可能有中断处理代码访问进程表（虽然不访问 vmrequest 链表，但保持类型层面的约束更安全）。

### 3.5 p_vm_suspend: Option<VmSuspendContext> 替代 p_vmrequest 内嵌结构体

**决策**：将 `p_vmrequest` 从内嵌结构体改为 `Option<VmSuspendContext>`，`None` 表示进程没有挂起的内存请求，`Some` 表示有。

**依据**：§2.3.1 分析了 `vm_suspend()` 的 `assert(!RTS_ISSET(caller, RTS_VMREQUEST))` 断言——确保进程不会重复挂起。§2.3.4 分析了 `VMCTL_MEMREQ_REPLY` 的 `RTS_UNSET(p, RTS_VMREQUEST)` ——恢复后清除标志。

**Minix3 的问题**：`p_vmrequest` 始终存在于 `struct proc` 中，即使进程没有挂起的内存请求。字段的有效性由 `RTS_VMREQUEST` 标志位控制——标志位为 0 时字段无效，但编译器无法检查。

**Rust 设计**：

```rust
pub struct KProcess {
    // ...
    pub p_vm_suspend: Option<VmSuspendContext>,
}
```

**与 RTS_VMREQUEST 的关系**：Minix3 用 `RTS_VMREQUEST` 标志位同时表达两个语义：(1) 进程因内存请求挂起，不参与调度；(2) `p_vmrequest` 字段有效。Rust 中 `p_vm_suspend: Some(...)` 隐含了语义 (2)，但语义 (1) 仍需要调度标志——因为调度器检查的是标志位，不是 `p_vm_suspend`。

**设计选择**：保留 `RTS_VMREQUEST` 作为 `RtsFlags` 中的调度标志（`rts::VMREQUEST = 0x800`），但添加不变量约束：

> **不变量**：`p_rts_flags.is_set(VMREQUEST) <==> p_vm_suspend.is_some()`

这个不变量在 `vm_suspend`（设置标志 + 填充上下文）和 `vmctl_memreq_reply`（清除标志 + 读取结果）中维护。如果使用类型状态模式，可以将两者合并为一个枚举，但会与 `RtsFlags` 的位操作模式冲突（调度器需要批量检查多个标志位）。

**为什么不用类型状态模式**：Minix3 的 `RTS_VMREQUEST` 与其他 RTS 标志（`SENDING`/`RECEIVING`/`PAGEFAULT` 等）是位运算关系——调度器通过 `p_rts_flags == 0` 判断进程可运行。类型状态模式要求将进程状态建模为互斥的枚举变体，但 Minix3 的 RTS 标志可以同时设置多个（例如 `RTS_VMREQUEST | RTS_PAGEFAULT` 不可能出现，但 `RTS_SENDING | RTS_SIGNALED` 可以）。因此保持位标志模式，用不变量约束保证一致性。

### 3.6 MF_KCALL_RESUME 的 Rust 表达

**决策**：保留 `MF_KCALL_RESUME` 作为 `MiscFlags` 中的标志位，不使用类型状态模式。但将"内核调用被中断"的语义通过 `VmSuspendType::KernelCall` 枚举显式表达。

**依据**：§2.3.4 分析了 `VMCTL_MEMREQ_REPLY` 中 `VMSTYPE_KERNELCALL` 分支设置 `MF_KCALL_RESUME`。§2.3.5 分析了 `kernel_call_resume()` 检查 `MF_KCALL_RESUME` 并重试内核调用。§2.3.10 分析了 `switch_to_user()` 中 `MF_KCALL_RESUME` 触发 `kernel_call_resume()`。

**Minix3 的 MF_KCALL_RESUME 双重作用**（§2.5.3）：

1. **恢复路径选择**：`switch_to_user()` 检查此标志，决定调用 `kernel_call_resume()` 还是正常返回用户态。
2. **上下文保存标记**：`kernel_call_finish()` 设置此标志，表示系统调用消息已保存到 `p_vmrequest.saved.reqmsg`。

**Rust 中的表达**：

```rust
pub mod mf {
    pub const KCALL_RESUME: u32 = 0x008;
    // ... 其他标志 ...
}
```

**为什么保留标志位而非类型状态**：与 §3.5 相同的理由——`MiscFlags` 是位运算模式，`MF_KCALL_RESUME` 可能与其他杂项标志同时设置（例如 `MF_KCALL_RESUME | MF_SC_ACTIVE`）。类型状态模式要求互斥变体，不适合。

**但语义通过枚举强化**：`VmSuspendType::KernelCall` 枚举变体显式表达了"内核调用被中断"的语义。当 `VMCTL_MEMREQ_REPLY` 收到 `VmSuspendType::KernelCall` 时，设置 `MF_KCALL_RESUME`。这比 Minix3 的 `switch(p->p_vmrequest.type) { case VMSTYPE_KERNELCALL: ... }` 更清晰——枚举变体名即文档。

**MF_DELIVERMSG 的保留**：`MF_DELIVERMSG`（`0x040`）已在 `proc.rs:107` 中定义。`VMSTYPE_DELIVERMSG` 恢复时依赖此标志重试消息投递（§2.3.10），语义不变。

### 3.7 VMCTL_MEMREQ_GET/REPLY 的 Rust 接口设计

**决策**：将 `VMCTL_MEMREQ_GET` 和 `VMCTL_MEMREQ_REPLY` 设计为 `VmRequestHandler` trait 的方法，而非直接函数调用。

**依据**：§2.3.3 分析了 `VMCTL_MEMREQ_GET` 遍历链表、过滤请求、返回请求详情。§2.3.4 分析了 `VMCTL_MEMREQ_REPLY` 设置结果、恢复进程。§1.5.3（00-kernel-overview）要求"所有硬件操作必须通过 trait"。

**但 VMCTL 不是硬件操作**：`VMCTL_MEMREQ_GET/REPLY` 是内核与 VM 之间的 IPC 协议，不涉及硬件寄存器或页表位操作。它更接近"内核内部状态管理"而非"硬件抽象"。

**设计选择**：使用普通方法而非 trait，因为：

1. **单一实现者**：只有内核自身实现此逻辑，不需要多态。
2. **无硬件依赖**：不涉及 `#[cfg(target_arch)]` 行为选择。
3. **与现有模式一致**：`kernel/src/vm.rs` 的 `VmRequestHandler` 也是具体结构体而非 trait。

```rust
pub struct VmRequestHandler;

impl VmRequestHandler {
    pub fn memreq_get(
        queue: &mut VmRequestQueue,
        procs: &[KProcess],
        msg: &mut Message,
    ) -> Result<VmCheckParams, VmCtlError> {
        let proc_nr = queue.dequeue_filtered(procs, |requestor, target| {
            allow_ipc_filtered_memreq(requestor, target)
        });

        match proc_nr {
            Some(nr) => {
                let proc = &procs[nr as usize];
                let ctx = proc.p_vm_suspend.as_ref().expect("VMREQUEST set but no context");
                // 填充消息字段
                // 设置 state = Fetched
                Ok(ctx.check_params)
            }
            None => Err(VmCtlError::NoRequest),
        }
    }

    pub fn memreq_reply(
        proc: &mut KProcess,
        result: VmCheckResult,
    ) -> Result<(), VmCtlError> {
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::NoRequest)?;
        assert_eq!(ctx.state, VmSuspendState::Fetched);

        ctx.state = VmSuspendState::Completed(result);

        match ctx.suspend_type {
            VmSuspendType::KernelCall => {
                proc.p_misc_flags.set(mf::KCALL_RESUME);
            }
            VmSuspendType::DeliverMsg => {
                assert!(proc.p_misc_flags.is_set(mf::DELIVERMSG));
            }
        }

        proc.p_rts_flags.clear(rts::VMREQUEST);
        Ok(())
    }
}
```

**VmCtlError**：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCtlError {
    NoRequest,
    InvalidState,
    InvalidEndpoint,
}
```

**与 Minix3 的对照**：

| Minix3 返回值 | Rust 等价 | 说明 |
|-------------|----------|------|
| `VMPTYPE_CHECK` | `Ok(VmCheckParams)` | 成功获取请求 |
| `ENOENT` | `Err(VmCtlError::NoRequest)` | 链表为空或全部被过滤 |
| `OK` (REPLY) | `Ok(())` | 成功回复 |
| `panic` | `Err(VmCtlError::InvalidState)` | 断言失败改为错误返回 |

**断言改错误返回**：Minix3 的 `VMCTL_MEMREQ_REPLY` 中有多个 `assert`（`RTS_ISSET(p, RTS_VMREQUEST)`、`vmresult == VMSUSPEND`、`MF_DELIVERMSG` 等）。Rust 中这些断言改为 `Result` 错误返回——在内核中 panic 是最后的手段，错误返回允许上层决定如何处理（日志+忽略、或主动 panic）。

### 3.8 与 02-page-table-kernel.md 的 VmRequest 关系

**决策**：02 文档的 `VmRequest`（§4.3b）记录跨地址空间操作的挂起上下文（源/目标地址、字节数、缺页方向），03 文档的 `VmSuspendContext` 记录更完整的挂起上下文（挂起类型、目标进程、检查参数、状态、保存消息）。两者合并为统一的 `VmSuspendContext`，02 文档的 `VmRequest` 字段作为 `VmSuspendContext` 的子集。

**依据**：§2.2.1 分析了 `p_vmrequest` 的完整字段。02-page-table-kernel.md §4.3b 的 `VmRequest` 只覆盖了跨地址空间拷贝场景（`src`/`dst`/`bytes`/`fault_type`），未覆盖消息投递场景（`saved_msg`/`suspend_type`）和地址范围检查场景（`check_params`）。

**合并方案**：

02 文档的 `VmRequest` 字段（跨地址空间拷贝）：

```rust
// 02-page-table-kernel.md §4.3b — 当前定义
pub struct VmRequest {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: Option<VmFaultType>,
}
```

03 文档的 `VmSuspendContext` 字段（完整挂起上下文）：

```rust
// 03-vm-request.md §3.2 — 本节定义
pub struct VmSuspendContext {
    pub suspend_type: VmSuspendType,
    pub target: Endpoint,
    pub check_params: VmCheckParams,
    pub state: VmSuspendState,
    pub saved_msg: Option<Message>,
}
```

**合并后**：

```rust
pub struct VmSuspendContext {
    pub suspend_type: VmSuspendType,
    pub target: Endpoint,
    pub check_params: VmCheckParams,
    pub state: VmSuspendState,
    pub saved_msg: Option<Message>,
    pub copy_context: Option<VmCopyContext>,
}

pub struct VmCopyContext {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: VmFaultType,
}
```

**为什么 `copy_context` 是 `Option`**：只有 `VMSTYPE_KERNELCALL` 场景下跨地址空间拷贝需要 `VmCopyContext`。`VMSTYPE_DELIVERMSG` 场景下不需要（消息投递的"拷贝"语义不同——它投递一条消息到进程的接收缓冲区，不是跨地址空间拷贝）。`Option` 允许表达"无拷贝上下文"的状态。

**02 文档的 VmRequest 迁移**：`kernel/src/vm.rs` 中的 `VmRequest` 结构体已重命名为 `VmCopyContext` 并移入 `VmSuspendContext`。`VmCopyError::Suspended` 变体已移除——它混淆了"操作挂起"和"地址错误"两个不同语义。现在使用 `CrossSpaceResult` 类型：`CrossSpaceResult::Suspended(VmFaultType)` 表示操作因缺页挂起（对应 C 的 `VMSUSPEND`），`CrossSpaceResult::Completed(Err(VmCopyError))` 表示地址解析错误（对应 C 的 `EFAULT_SRC/DST`）。

### 3.9 vm_suspend 的 Rust 表达

**决策**：将 `vm_suspend()` 拆分为"构造挂起上下文"和"入队+通知"两步，而非 Minix3 的单一函数。

**依据**：§2.3.1 分析了 `vm_suspend()` 的完整逻辑——设置 RTS_VMREQUEST、填充 p_vmrequest、链表插入、发送 SIGKMEM。§2.3.7 分析了 `vm_check_range()` 调用 `vm_suspend()` 的场景。

**Minix3 的问题**：`vm_suspend()` 同时做了三件事：(1) 修改进程状态（设置 RTS_VMREQUEST），(2) 填充挂起上下文，(3) 管理全局链表。这些职责耦合在一起。

**Rust 设计**：

```rust
impl KProcess {
    pub fn suspend_for_vm(
        &mut self,
        suspend_type: VmSuspendType,
        target: Endpoint,
        params: VmCheckParams,
        saved_msg: Option<Message>,
    ) {
        assert!(!self.p_rts_flags.is_set(rts::VMREQUEST));
        assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
        });

        self.p_rts_flags.set(rts::VMREQUEST);
    }
}

impl VmRequestQueue {
    pub fn enqueue_and_notify(
        &mut self,
        proc_nr: ProcNr,
        procs: &mut [KProcess],
        send_sig: &mut dyn FnMut() -> Result<(), KernelError>,
    ) -> Result<(), KernelError> {
        let was_empty = self.enqueue(proc_nr, procs);
        if was_empty {
            send_sig()?;
        }
        Ok(())
    }
}
```

**调用点示例**（`vm_check_range` 的 Rust 等价）：

```rust
pub fn vm_check_range(
    caller: &mut KProcess,
    target: Endpoint,
    start: VirBytes,
    length: VirBytes,
    write_flag: bool,
    queue: &mut VmRequestQueue,
    proc_nr: ProcNr,
    procs: &mut [KProcess],
    send_sig: &mut dyn FnMut() -> Result<(), KernelError>,
) -> Result<(), VmCopyError> {
    if caller.p_misc_flags.is_set(mf::KCALL_RESUME) {
        return check_resumed_caller(caller);
    }

    // 先查后操作（02-page-table-kernel §3.3）
    // ... lookup_in_table 检查地址范围 ...

    // 地址范围有缺页 → 挂起
    caller.suspend_for_vm(
        VmSuspendType::KernelCall,
        target,
        VmCheckParams { start, length, write_flag },
        None,
    );
    queue.enqueue_and_notify(proc_nr, procs, send_sig)?;
    CrossSpaceResult::Suspended(fault_type)
}
```

**为什么拆分**：

1. **职责分离**：`suspend_for_vm` 只修改进程自身状态，`enqueue_and_notify` 只管理全局链表和通知。Minix3 的 `vm_suspend()` 两者混在一起。
2. **可测试性**：`suspend_for_vm` 可以在测试中单独调用，不需要全局链表。
3. **send_sig 注入**：`send_sig` 通过参数注入而非全局函数调用，允许测试时 mock。

**send_sig 的 trait 注入 vs FnMut**：使用 `FnMut` 闭包而非 trait，因为发送信号是一次性操作，不需要多态。如果将来需要更复杂的信号发送策略（如批量发送、延迟发送），可以改为 trait。

### 3.10 clear_memreq 的 Rust 表达

**决策**：将 `clear_memreq()` 实现为 `KProcess` 的方法，清理挂起上下文并恢复进程。

**依据**：§2.3.9 分析了 `clear_memreq()` 的逻辑——从 vmrequest 链表中移除进程、清除 `RTS_VMREQUEST`。它在进程退出时（`clear_endpoint()` 调用链）调用，确保不会留下悬挂的内存请求。

**注意**：Minix3 的 `clear_memreq()` **不清除** `MF_KCALL_RESUME`（grep `system.c` 确认无此操作），也**不重置** `p_vmrequest.type`（`VMSTYPE_SYS_NONE` 在内核中从未被赋值使用）。`MF_KCALL_RESUME` 仅在 `kernel_call_resume()` (system.c:635) 中被清除。Rust 实现忠实复现此行为。

**Rust 设计**：

```rust
impl KProcess {
    pub fn clear_vm_suspend(&mut self) {
        self.p_vm_suspend = None;
        self.p_rts_flags.clear(rts::VMREQUEST);
        // 注意：不清除 MF_KCALL_RESUME，与 Minix3 clear_memreq 行为一致
        // MF_KCALL_RESUME 仅在 kernel_call_resume() 中清除
    }
}
```

**与 Minix3 的对照**：Minix3 的 `clear_memreq` 只做两件事：(1) 从 vmrequest 链表中移除进程 (2) `RTS_UNSET(rp, RTS_VMREQUEST)`。Rust 将 `p_vm_suspend` 设为 `None` 替代了 Minix3 中 `p_vmrequest` 各字段的"隐式废弃"——更清晰，编译器保证后续不会误读已清理的上下文。

**从 VmRequestQueue 中移除**：`clear_memreq()` 只清理进程自身状态，不从链表中移除。Minix3 中也是如此——`clear_memreq` 在进程退出时调用，此时进程已经不在 vmrequest 链表中（因为 `VMCTL_MEMREQ_REPLY` 已经将进程从链表中移除，或者进程从未被加入链表）。如果进程确实在链表中（极端情况），需要额外处理。Rust 中可以在 `clear_vm_suspend` 中添加 `p_next_requestor = None` 以确保一致性。

### 3.11 do_update 链表迁移的简化

**决策**：由于使用 `ProcNr` 索引替代裸指针（§3.4），进程更新（live update）时的链表迁移逻辑可以消除。

**依据**：§2.5.2 分析了 `do_update()`（`do_update.c:323-339`）在进程更新时需要将 vmrequest 链表中的 `*proc` 指针从旧进程替换为新进程。这是因为 Minix3 使用裸指针链接——旧进程的 `struct proc` 被新进程替换后，链表中的指针需要更新。

**Rust 中为什么不需要**：`VmRequestQueue` 使用 `ProcNr` 索引。进程更新时，新进程占据同一槽位，索引自动指向新进程。链表中的 `Option<ProcNr>` 值不需要修改。

**但需要更新 `VmSuspendContext`**：如果旧进程有挂起的内存请求（`p_vm_suspend.is_some()`），新进程需要继承这个上下文。这属于 `do_update` 的进程结构复制逻辑，不属于 vmrequest 链表管理。

### 3.12 CrossSpaceResult 与 VmSuspendContext 的关系

**决策**：`CrossSpaceResult::Suspended` 是跨地址空间操作的**挂起信号**，`VmSuspendContext` 是进程的**挂起状态上下文**。两者是不同层面的概念，不应合并。

**依据**：§2.3.7 分析了 `vm_check_range()` 返回 `VMSUSPEND` 给调用者。§2.3.1 分析了 `vm_suspend()` 同时设置 `RTS_VMREQUEST` 和填充 `p_vmrequest`。02-page-table-kernel.md §3.7 定义了 `CrossSpaceResult`。

**数据流**：

```
cross_space_copy / vm_check_range
    → 发现缺页
    → 返回 CrossSpaceResult::Suspended(VmFaultType) 给调用者
    → 调用者（kernel_call_dispatch）收到 Suspended
    → 调用 suspend_for_vm() 填充 VmSuspendContext
    → 调用 kernel_call_finish() 保存消息
    → 进程被挂起，等待 VM 处理
```

`CrossSpaceResult::Suspended` 告诉调用者"操作因缺页挂起"，调用者据此执行挂起逻辑（构造 VmSuspendContext、保存消息、不回复用户进程）。`VmSuspendContext` 记录挂起的详细信息，供 VM 查询和恢复时使用。

**为什么用 `CrossSpaceResult` 而非 `Result<(), VmCopyError>`**：在 C 中，`VMSUSPEND`(-996) 和 `EFAULT_SRC/DST`(-995/-994) 是同一返回值空间的不同值，但语义完全不同——`VMSUSPEND` 触发 `vm_suspend()` 挂起流程，`EFAULT_SRC/DST` 触发正常错误返回。将它们放在同一个 `VmCopyError` 枚举中混淆了"需要恢复的正常流程"和"地址错误"。`CrossSpaceResult` 将两者分离：`Suspended` 表示"需要恢复"，`Completed(Err)` 表示"地址错误"。

### 3.13 不需要的 Minix3 符号

以下 Minix3 符号在 minix-rs 中不需要实现：

| 符号 | 原因 | 依据 |
|------|------|------|
| `VMSTYPE_SYS_NONE` (0) | 从未使用 | §2.5.6 |
| `VMSTYPE_MAP` (3) | 有 case 分支但无代码设置此类型 | §2.5.6 |
| `VMPTYPE_NONE` (0) | 从未使用 | §2.5.6 |
| `p_vmrequest.nextrestart` | 从未赋值或读取 | §2.5.6 |
| `RTS_VMREQTARGET` (0x1000) | 从未 `RTS_SET`/`RTS_UNSET` | §2.5.6 |
| `vmrequest` 裸指针链表 | 用 `VmRequestQueue` + `ProcNr` 索引替代 | §3.4 |
| `p_vmrequest` 内嵌结构体 | 用 `Option<VmSuspendContext>` 替代 | §3.5 |
| `vmresult` 三态哨兵值 | 用 `VmSuspendState` 枚举替代 | §3.3 |
| `do_update` 链表迁移 | `ProcNr` 索引自动指向新进程 | §3.11 |

---

## 4. 实现详解

> 本章解释"如何实现"，每个实现对应 Ch3 的设计决策。代码位于 `os/kernel/src/vm.rs`（与 02 文档共享模块）和 `os/kernel/src/proc.rs`（KProcess 扩展）。

### 4.1 VmSuspendType — 挂起类型枚举

> 设计决策：§3.1（VmSuspendType 枚举替代 VMSTYPE_* 宏）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendType {
    KernelCall,
    DeliverMsg,
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|---------|
| 判断挂起类型 | `switch(p->p_vmrequest.type) { case VMSTYPE_KERNELCALL: ... }` | `match ctx.suspend_type { VmSuspendType::KernelCall => ... }` |
| 设置挂起类型 | `caller->p_vmrequest.type = VMSTYPE_KERNELCALL` | `VmSuspendContext { suspend_type: VmSuspendType::KernelCall, ... }` |
| 未覆盖的类型 | `default: panic("strange request type")` | 编译器强制穷尽匹配，无需 default |

**编译时穷尽检查**：Minix3 的 `switch(p->p_vmrequest.type)` 需要 `default: panic()` 分支处理未知的 `type` 值。Rust 的 `match` 对枚举强制穷尽——如果将来添加 `VmSuspendType` 变体，所有 match 点编译报错，不会遗漏。

### 4.2 VmCheckParams — 地址检查参数

> 设计决策：§3.2（VmCheckParams 提取为独立结构体）

```rust
#[derive(Debug, Clone, Copy)]
pub struct VmCheckParams {
    pub start: VirBytes,
    pub length: VirBytes,
    pub write_flag: bool,
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|---------|
| 设置检查参数 | `caller->p_vmrequest.params.check.start = linaddr; ...` | `VmCheckParams { start, length, write_flag }` |
| 读取检查参数 | `m_ptr->SVMCTL_MRG_ADDR = rp->p_vmrequest.params.check.start; ...` | `ctx.check_params.start` |
| 写标志判断 | `rp->p_vmrequest.params.check.writeflag` (u8, 0/非0) | `ctx.check_params.write_flag` (bool) |

**write_flag: bool 替代 u8**：Minix3 的 `writeflag` 是 `u8_t`，`0` = 读，非零 = 写。Rust 用 `bool` 消除"非零即写"的隐式约定——`false` = 读，`true` = 写，不存在歧义值。

### 4.3 VmSuspendState — 挂起状态枚举

> 设计决策：§3.3（VmSuspendState 枚举替代 vmresult 三态哨兵值）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendState {
    Pending,
    Fetched,
    Completed(VmCheckResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCheckResult {
    Ok,
    Fault,
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (vmresult int) | minix-rs (VmSuspendState) |
|------|----------------------|--------------------------|
| 初始化 | `vmresult = 0`（隐式，vm_suspend 未显式设置） | `state: VmSuspendState::Pending`（显式构造） |
| 标记已取出 | `rp->p_vmrequest.vmresult = VMSUSPEND` | `ctx.state = VmSuspendState::Fetched` |
| 检查已取出 | `assert(vmresult == VMSUSPEND)` | `assert_eq!(ctx.state, VmSuspendState::Fetched)` |
| 写入结果 | `p->p_vmrequest.vmresult = m_ptr->SVMCTL_VALUE` | `ctx.state = VmSuspendState::Completed(result)` |
| 读取结果 | `if (vmresult != OK) return EFAULT` | `match ctx.state { Completed(VmCheckResult::Fault) => ... }` |
| 非法状态 | `vmresult` 可被设为任意 int | `VmSuspendState` 只有三个合法变体 |

**状态机不变量**：

```
Pending ──memreq_get──▶ Fetched ──memreq_reply──▶ Completed(Ok/Fault)
```

- `Pending → Fetched`：`VMCTL_MEMREQ_GET` 取出请求时转换
- `Fetched → Completed`：`VMCTL_MEMREQ_REPLY` 写入 VM 结果时转换
- 不允许 `Pending → Completed`（跳过 GET 直接 REPLY）
- 不允许反向转换

### 4.4 VmSuspendContext — 挂起上下文结构体

> 设计决策：§3.2（VmSuspendContext 替代 p_vmrequest）、§3.8（与 02 文档 VmRequest 合并）

```rust
pub struct VmSuspendContext {
    pub suspend_type: VmSuspendType,
    pub target: Endpoint,
    pub check_params: VmCheckParams,
    pub state: VmSuspendState,
    pub saved_msg: Option<Message>,
    pub copy_context: Option<VmCopyContext>,
}
```

**VmCopyContext**（从 02 文档 `VmRequest` 迁移，代码定义在 `os/kernel/src/vm.rs`，设计见 02-page-table-kernel.md §4.3b）：

```rust
pub struct VmCopyContext {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: VmFaultType,
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (p_vmrequest) | minix-rs (VmSuspendContext) |
|------|---------------------|---------------------------|
| 判断有无挂起请求 | `RTS_ISSET(p, RTS_VMREQUEST)` | `p_vm_suspend.is_some()` + `RTS_VMREQUEST` 不变量 |
| 读取挂起类型 | `p->p_vmrequest.type` (int) | `ctx.suspend_type` (VmSuspendType) |
| 读取目标进程 | `p->p_vmrequest.target` (endpoint_t) | `ctx.target` (Endpoint) |
| 读取检查参数 | `p->p_vmrequest.params.check.*` | `ctx.check_params.*` |
| 读取 VM 结果 | `p->p_vmrequest.vmresult` (int) | `ctx.state` (VmSuspendState) |
| 读取保存的消息 | `p->p_vmrequest.saved.reqmsg` | `ctx.saved_msg` (Option\<Message\>) |
| 读取拷贝上下文 | 无（隐含在 vm_check_range 调用链中） | `ctx.copy_context` (Option\<VmCopyContext\>) |
| 请求参数类型 | `p->p_vmrequest.req_type` (int, 固定 VMPTYPE_CHECK) | 消除（§3.2） |
| 链表下一节点 | `p->p_vmrequest.nextrequestor` (*proc) | `p_next_requestor` (Option\<ProcNr\>，在 KProcess 上) |
| 未使用字段 | `p->p_vmrequest.nextrestart` (*proc) | 不实现（§2.5.6） |

**copy_context 的 Option 语义**：

| suspend_type | copy_context | 场景 |
|-------------|-------------|------|
| `KernelCall` | `Some(VmCopyContext)` | 跨地址空间拷贝/清零/检查被中断 |
| `KernelCall` | `None` | vm_check_range 被中断（不需要恢复拷贝，只需重试检查） |
| `DeliverMsg` | `None` | 消息投递被中断（不涉及跨地址空间拷贝） |

**构造示例**（vm_check_range 场景）：

```rust
let ctx = VmSuspendContext {
    suspend_type: VmSuspendType::KernelCall,
    target,
    check_params: VmCheckParams { start, length, write_flag },
    state: VmSuspendState::Pending,
    saved_msg: None,
    copy_context: None,
};
```

**构造示例**（cross_space_copy 场景）：

```rust
let ctx = VmSuspendContext {
    suspend_type: VmSuspendType::KernelCall,
    target,
    check_params: VmCheckParams { start, length, write_flag },
    state: VmSuspendState::Pending,
    saved_msg: None,
    copy_context: Some(VmCopyContext {
        src, dst, bytes, fault_type,
    }),
};
```

**构造示例**（delivermsg 场景）：

```rust
let ctx = VmSuspendContext {
    suspend_type: VmSuspendType::DeliverMsg,
    target: rp.p_endpoint,
    check_params: VmCheckParams {
        start: rp.p_delivermsg_vir,
        length: VirBytes(core::mem::size_of::<Message>() as u64),
        write_flag: true,
    },
    state: VmSuspendState::Pending,
    saved_msg: Some(rp.p_delivermsg),
    copy_context: None,
};
```

### 4.5 VmRequestQueue — 挂起请求队列

> 设计决策：§3.4（VmRequestQueue 替代裸指针链表）

```rust
pub struct VmRequestQueue {
    head: Option<ProcNr>,
}

impl VmRequestQueue {
    pub const fn new() -> Self {
        Self { head: None }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    pub fn enqueue(&mut self, proc_nr: ProcNr, procs: &mut [KProcess]) -> bool {
        let proc = &mut procs[proc_nr as usize];
        proc.p_next_requestor = self.head;
        let was_empty = self.head.is_none();
        self.head = Some(proc_nr);
        was_empty
    }

    pub fn dequeue_filtered<F>(
        &mut self,
        procs: &[KProcess],
        mut filter: F,
    ) -> Option<ProcNr>
    where
        F: FnMut(&KProcess, &KProcess) -> bool,
    {
        let mut prev_link: *mut Option<ProcNr> = &mut self.head;
        let mut current = self.head;

        while let Some(nr) = current {
            let proc = &procs[nr as usize];
            let target_nr = proc.p_vm_suspend.as_ref()
                .and_then(|ctx| {
                    // 需要通过 target endpoint 查找 ProcNr
                    // 实际实现中需要进程表查找
                    None
                });
            // 简化：假设 filter 接受 requestor 和 target 的 ProcNr
            // 实际实现需要 resolve target endpoint → ProcNr
            let passed = true; // placeholder

            if passed {
                // 从链表中移除
                unsafe {
                    *prev_link = proc.p_next_requestor;
                }
                let removed_proc = &mut procs[nr as usize];
                removed_proc.p_next_requestor = None;
                return Some(nr);
            }

            prev_link = &mut procs[nr as usize].p_next_requestor;
            current = proc.p_next_requestor;
        }

        None
    }

    pub fn enqueue_and_notify(
        &mut self,
        proc_nr: ProcNr,
        procs: &mut [KProcess],
        send_sig: &mut dyn FnMut() -> Result<(), ()>,
    ) -> Result<(), ()> {
        let was_empty = self.enqueue(proc_nr, procs);
        if was_empty {
            send_sig()?;
        }
        Ok(())
    }
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (vmrequest 链表) | minix-rs (VmRequestQueue) |
|------|------------------------|--------------------------|
| 链表头 | `EXTERN struct proc *vmrequest` | `VmRequestQueue { head: Option<ProcNr> }` |
| 头插法入队 | `caller->p_vmrequest.nextrequestor = vmrequest; vmrequest = caller;` | `queue.enqueue(proc_nr, procs)` |
| 判断空→发信号 | `if(!(nextrequestor = vmrequest)) send_sig(VM_PROC_NR, SIGKMEM)` | `enqueue` 返回 `was_empty`，调用者决定发信号 |
| 遍历+过滤+移除 | `for(rpp = &vmrequest; *rpp != NULL; rpp = &(*rpp)->p_vmrequest.nextrequestor)` | `dequeue_filtered(procs, filter)` |
| 节点链接 | `*proc` 指针 | `Option<ProcNr>` 索引 |

**dequeue_filtered 的 unsafe 使用**：链表遍历中需要修改前驱节点的 `p_next_requestor` 字段，而同时读取当前节点的 `p_next_requestor`。由于 `procs` 是 `&[KProcess]`（不可变切片），修改链接需要 `UnsafeCell` 或 unsafe。实际实现中应将 `procs` 参数改为 `&mut [KProcess]`，消除 unsafe：

```rust
pub fn dequeue_filtered<F>(
    &mut self,
    procs: &mut [KProcess],
    mut filter: F,
) -> Option<ProcNr>
where
    F: FnMut(&KProcess, &KProcess) -> bool,
{
    let mut current = self.head;
    let mut prev_nr: Option<ProcNr> = None;

    while let Some(nr) = current {
        let proc = &procs[nr as usize];
        let next = proc.p_next_requestor;
        let passed = filter(proc, proc); // 简化，实际需 target

        if passed {
            // 从链表中移除当前节点
            if let Some(pnr) = prev_nr {
                procs[pnr as usize].p_next_requestor = next;
            } else {
                self.head = next;
            }
            procs[nr as usize].p_next_requestor = None;
            return Some(nr);
        }

        prev_nr = Some(nr);
        current = next;
    }

    None
}
```

**BKL 安全**：`VmRequestQueue` 的所有方法要求调用者持有 BKL。方法本身不包含同步原语——BKL 由调用者保证。这与 Minix3 的设计一致：`vmrequest` 链表操作在系统调用处理路径中执行，全程持有 BKL。

### 4.6 VmRequestHandler — VMCTL 请求处理

> 设计决策：§3.7（VMCTL_MEMREQ_GET/REPLY 的 Rust 接口设计）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCtlError {
    NoRequest,
    InvalidState,
    InvalidEndpoint,
}

pub struct VmRequestHandler;

impl VmRequestHandler {
    pub fn memreq_get(
        queue: &mut VmRequestQueue,
        procs: &mut [KProcess],
        endpoint_lookup: impl Fn(Endpoint) -> Option<ProcNr>,
    ) -> Result<(ProcNr, VmCheckParams), VmCtlError> {
        let proc_nr = queue.dequeue_filtered(procs, |requestor, _target| {
            // 对应 Minix3 的 allow_ipc_filtered_memreq
            // VM 设置 IPC 过滤器时，只允许与特定进程通信
            // 当前简化：总是允许
            true
        }).ok_or(VmCtlError::NoRequest)?;

        let proc = &mut procs[proc_nr as usize];
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::InvalidState)?;

        if ctx.state != VmSuspendState::Pending {
            return Err(VmCtlError::InvalidState);
        }

        ctx.state = VmSuspendState::Fetched;
        let params = ctx.check_params;

        Ok((proc_nr, params))
    }

    pub fn memreq_reply(
        proc: &mut KProcess,
        result: VmCheckResult,
    ) -> Result<(), VmCtlError> {
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::NoRequest)?;

        if ctx.state != VmSuspendState::Fetched {
            return Err(VmCtlError::InvalidState);
        }

        ctx.state = VmSuspendState::Completed(result);

        match ctx.suspend_type {
            VmSuspendType::KernelCall => {
                proc.p_misc_flags.set(mf::KCALL_RESUME);
            }
            VmSuspendType::DeliverMsg => {
                if !proc.p_misc_flags.is_set(mf::DELIVERMSG) {
                    return Err(VmCtlError::InvalidState);
                }
            }
        }

        proc.p_rts_flags.clear(rts::VMREQUEST);
        Ok(())
    }
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 (do_vmctl.c) | minix-rs (VmRequestHandler) |
|------|---------------------|---------------------------|
| 获取请求 | `case VMCTL_MEMREQ_GET:` 遍历链表 | `memreq_get()` → `dequeue_filtered()` |
| 返回无请求 | `return ENOENT` | `Err(VmCtlError::NoRequest)` |
| 标记已取出 | `rp->p_vmrequest.vmresult = VMSUSPEND` | `ctx.state = VmSuspendState::Fetched` |
| 回复结果 | `case VMCTL_MEMREQ_REPLY:` 设置 vmresult | `memreq_reply()` → `ctx.state = Completed(result)` |
| 断言已取出 | `assert(vmresult == VMSUSPEND)` | `if ctx.state != Fetched { return Err(InvalidState) }` |
| 设置恢复标志 | `p->p_misc_flags \|= MF_KCALL_RESUME` | `proc.p_misc_flags.set(mf::KCALL_RESUME)` |
| 断言投递标志 | `assert(p->p_misc_flags & MF_DELIVERMSG)` | `if !is_set(DELIVERMSG) { return Err(InvalidState) }` |
| 清除挂起标志 | `RTS_UNSET(p, RTS_VMREQUEST)` | `proc.p_rts_flags.clear(rts::VMREQUEST)` |
| 未知类型 panic | `default: panic("strange request type")` | 编译器穷尽检查，不可能到达 |

**断言改错误返回**：Minix3 中 `assert(vmresult == VMSUSPEND)` 和 `assert(p->p_misc_flags & MF_DELIVERMSG)` 在条件不满足时 panic。Rust 中改为 `Result` 错误返回——调用者可以选择 panic（`unwrap()`）或优雅处理。内核中 panic 是最后手段，错误返回更安全。

### 4.7 KProcess 扩展 — 挂起与恢复方法

> 设计决策：§3.5（Option\<VmSuspendContext\>）、§3.9（vm_suspend 拆分）、§3.10（clear_memreq）

在 `os/kernel/src/proc.rs` 中扩展 `KProcess`：

```rust
impl KProcess {
    pub fn suspend_for_vm(
        &mut self,
        suspend_type: VmSuspendType,
        target: Endpoint,
        params: VmCheckParams,
        saved_msg: Option<Message>,
    ) {
        debug_assert!(!self.p_rts_flags.is_set(rts::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: None,
        });

        self.p_rts_flags.set(rts::VMREQUEST);
    }

    pub fn suspend_for_vm_with_copy(
        &mut self,
        suspend_type: VmSuspendType,
        target: Endpoint,
        params: VmCheckParams,
        saved_msg: Option<Message>,
        copy_ctx: VmCopyContext,
    ) {
        debug_assert!(!self.p_rts_flags.is_set(rts::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: Some(copy_ctx),
        });

        self.p_rts_flags.set(rts::VMREQUEST);
    }

    pub fn clear_vm_suspend(&mut self) {
        self.p_vm_suspend = None;
        self.p_rts_flags.clear(rts::VMREQUEST);
    }

    pub fn is_vm_suspended(&self) -> bool {
        self.p_rts_flags.is_set(rts::VMREQUEST)
    }

    pub fn vm_suspend_context(&self) -> Option<&VmSuspendContext> {
        self.p_vm_suspend.as_ref()
    }

    pub fn vm_suspend_context_mut(&mut self) -> Option<&mut VmSuspendContext> {
        self.p_vm_suspend.as_mut()
    }
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|---------|
| 挂起进程 | `vm_suspend(caller, target, linaddr, len, type, writeflag)` | `caller.suspend_for_vm(type, target, params, msg)` |
| 清理挂起 | `clear_memreq(rp)` | `rp.clear_vm_suspend()` |
| 判断是否挂起 | `RTS_ISSET(p, RTS_VMREQUEST)` | `rp.is_vm_suspended()` |
| 读取上下文 | `p->p_vmrequest.*` | `rp.vm_suspend_context().*.check_params.*` |
| 不变量检查 | `assert(!RTS_ISSET(caller, RTS_VMREQUEST))` | `debug_assert!(self.p_vm_suspend.is_none())` |

**debug_assert vs assert**：`suspend_for_vm` 中的断言使用 `debug_assert!`——在 release 构建中不检查。理由：这些不变量由 `RTS_VMREQUEST` 标志位保证，而标志位的正确性由调用链保证（`vm_suspend` 只在 `RTS_VMREQUEST` 未设置时调用）。如果需要更强的保证，可以改为 `assert!`。

**两个 suspend 方法**：`suspend_for_vm` 用于 `vm_check_range` 场景（无拷贝上下文），`suspend_for_vm_with_copy` 用于 `cross_space_copy` 场景（有拷贝上下文）。这对应 Minix3 中 `vm_suspend` 的不同调用点——`vm_check_range` 不保存拷贝参数，`virtual_copy_f` 通过 `VmCopyContext` 保存。

### 4.8 与现有 vm.rs 的整合

> 设计决策：§3.8（与 02 文档 VmRequest 合并）

**当前 `kernel/src/vm.rs` 的结构**（02 文档定义）：

```rust
// 已有
pub struct PageTableRef { ... }
pub enum AddressRef { ... }
pub enum VmCopyError { ... }
pub enum VmFaultType { ... }
pub struct VmRequest { ... }  // → 重命名为 VmCopyContext
pub fn cross_space_copy<D: DirectMapArch>(...) { ... }
```

**整合方案**：

1. **`VmRequest` → `VmCopyContext`**：重命名，字段不变（`src`/`dst`/`bytes`/`fault_type`）
2. **新增类型**：`VmSuspendType`、`VmCheckParams`、`VmSuspendState`、`VmCheckResult`、`VmSuspendContext`、`VmRequestQueue`、`VmCtlError`、`VmRequestHandler`
3. **`VmCopyError::Suspended` 已移除**：改用 `CrossSpaceResult::Suspended(VmFaultType)` 表示操作挂起，与 `VmCopyError`（地址错误）分离
4. **`VmFaultType` 保留**：它区分缺页方向（源/目标），用于 `VmCopyContext`

**整合后的 `vm.rs` 结构**：

```rust
// === 02-page-table-kernel 定义 ===
pub struct PageTableRef { ... }
pub enum AddressRef { ... }
pub enum VmCopyError {
    SrcPageFault,
    DstPageFault,
    InvalidAddress,
    Suspended,          // 保留：跨地址空间操作错误返回
    PermissionDenied,
    UnknownEndpoint,
}
pub enum VmFaultType { ... }
pub struct VmCopyContext { ... }  // 原 VmRequest，重命名
pub fn cross_space_copy<D: DirectMapArch>(...) { ... }

// === 03-vm-request 定义 ===
pub enum VmSuspendType { ... }
pub struct VmCheckParams { ... }
pub enum VmSuspendState { ... }
pub enum VmCheckResult { ... }
pub struct VmSuspendContext { ... }
pub struct VmRequestQueue { ... }
pub enum VmCtlError { ... }
pub struct VmRequestHandler;
```

**KProcess 新增字段**（`proc.rs`）：

```rust
pub struct KProcess {
    // ... existing fields ...

    // VM request queue link (对应 Minix3 p_vmrequest.nextrequestor)
    pub p_next_requestor: Option<ProcNr>,

    // VM suspend context (对应 Minix3 p_vmrequest，但用 Option 替代内嵌)
    pub p_vm_suspend: Option<VmSuspendContext>,
}
```

**不变量约束**（文档化，不由类型系统强制）：

> `p_rts_flags.is_set(VMREQUEST) <==> p_vm_suspend.is_some()`

### 4.9 恢复路径的 Rust 实现

> 设计决策：§3.6（MF_KCALL_RESUME 保留标志位）、§3.9（vm_suspend 拆分）

**VMSTYPE_KERNELCALL 恢复路径**：

```rust
pub fn kernel_call_resume(caller: &mut KProcess) -> VmCheckResult {
    debug_assert!(caller.p_misc_flags.is_set(mf::KCALL_RESUME));

    let ctx = caller.p_vm_suspend.as_ref()
        .expect("MF_KCALL_RESUME set but no VmSuspendContext");

    match ctx.state {
        VmSuspendState::Completed(result) => {
            caller.p_misc_flags.clear(mf::KCALL_RESUME);
            // result == Ok: VM 确认地址有效，重试内核调用
            // result == Fault: VM 确认地址无效，返回 EFAULT 给用户进程
            result
        }
        _ => panic!("kernel_call_resume with non-completed state"),
    }
}
```

**VMSTYPE_DELIVERMSG 恢复路径**：

```rust
// 在 switch_to_user 中检查 MF_DELIVERMSG
pub fn try_deliver_message(rp: &mut KProcess) -> Result<bool, ()> {
    if !rp.p_misc_flags.is_set(mf::DELIVERMSG) {
        return Ok(false);
    }

    // 重试消息投递
    // delivermsg(rp) → 可能再次 VMSUSPEND
    Ok(true)
}
```

**C 行为 vs Rust 行为对照**：

| 恢复路径 | Minix3 | minix-rs |
|---------|--------|---------|
| KernelCall 恢复 | `switch_to_user` 检查 `MF_KCALL_RESUME` → `kernel_call_resume()` | 同，但 `kernel_call_resume` 读取 `VmSuspendContext.state` |
| KernelCall 重试 | `kernel_call_resume` 从 `p_vmrequest.saved.reqmsg` 恢复消息 | 从 `ctx.saved_msg` 恢复消息 |
| KernelCall 结果 | `check_resumed_caller()` 检查 `vmresult` | `kernel_call_resume` 匹配 `VmSuspendState::Completed(result)` |
| DeliverMsg 恢复 | `switch_to_user` 检查 `MF_DELIVERMSG` → `delivermsg()` | 同，`try_deliver_message` |
| DeliverMsg 重试 | `delivermsg` 从 `p_delivermsg` 重新投递 | 同，从 `p_delivermsg` 重新投递 |

## 5. 测试要点

> 对应 Ch3 设计决策和 Ch4 实现的测试覆盖。测试代码位于 `os/kernel/src/vm.rs` 和 `os/kernel/src/proc.rs` 的 `#[cfg(test)] mod tests` 中。

### 5.1 单元测试

| 测试 | 覆盖的决策 | 验证内容 |
|------|----------|---------|
| `VmSuspendType` 穷尽匹配 | §3.1 | 枚举变体覆盖所有合法类型 |
| `VmSuspendState` 状态转换 | §3.3 | Pending→Fetched→Completed，非法转换被拒绝 |
| `VmCheckParams` 构造 | §3.2 | write_flag: bool 消除 u8 歧义 |
| `VmRequestQueue::enqueue` | §3.4 | 头插法、was_empty 返回值 |
| `VmRequestQueue::dequeue_filtered` | §3.4 | 过滤逻辑、空链表返回 None |
| `VmRequestHandler::memreq_get` | §3.7 | Pending→Fetched 转换、NoRequest 错误 |
| `VmRequestHandler::memreq_reply` | §3.7 | Fetched→Completed 转换、InvalidState 错误、标志位设置 |
| `KProcess::suspend_for_vm` | §3.5, §3.9 | RTS_VMREQUEST 设置、VmSuspendContext 填充 |
| `KProcess::clear_vm_suspend` | §3.10 | RTS_VMREQUEST 清除、p_vm_suspend = None |
| RTS_VMREQUEST 不变量 | §3.5 | `is_set(VMREQUEST) <==> p_vm_suspend.is_some()` |

### 5.2 集成测试

| 测试 | 覆盖的路径 | 验证内容 |
|------|----------|---------|
| vm_check_range 挂起 | §2.3.7 | 缺页→suspend_for_vm→enqueue→SIGKMEM |
| VMCTL_MEMREQ_GET 循环 | §2.3.3 | VM 循环获取直到 NoRequest |
| VMCTL_MEMREQ_REPLY 恢复 | §2.3.4 | VM 回复→MF_KCALL_RESUME→kernel_call_resume |
| delivermsg 挂起与恢复 | §2.3.8 | 消息投递缺页→DeliverMsg→MF_DELIVERMSG→重试 |
| IPC 过滤器 | §2.5.1 | VM 设置过滤器→跳过被过滤请求→清除过滤器后重试 |

---

## 6. 参见

- [02-page-table-kernel.md](02-page-table-kernel.md) — 跨地址空间拷贝的缺页触发场景，`VmCopyContext` 的原始定义
- [00-kernel-overview.md](00-kernel-overview.md) §1.5 — 内核执行模型约束（BKL + SMP + 中断）
- [06-proc-struct.md](06-proc-struct.md) — `struct proc` 完整字段分析
- [12-syscall-dispatch.md](12-syscall-dispatch.md) — `kernel_call_dispatch`/`kernel_call_finish`/`kernel_call_resume` 调用链
