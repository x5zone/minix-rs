# 24-cross-space-runtime: 跨地址空间运行时

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/include/minix/syslib.h` (`sys_datacopy` 宏), `minix3/minix/kernel/system/do_copy.c` (`do_copy` 处理 `SYS_VIRCOPY`/`SYS_PHYSCOPY`), `minix3/minix/kernel/arch/i386/memory.c` (`data_copy` / `data_copy_vmcheck` / `virtual_copy_f` 实现), `minix3/minix/kernel/proto.h` (`virtual_copy` / `virtual_copy_vmcheck` 宏), `minix3/minix/kernel/proc.c` (`vm_suspend`), `minix3/minix/kernel/system.c` (`kernel_call_resume`), `minix3/minix/kernel/vm.h` (`VMSUSPEND`/`EFAULT_SRC`/`EFAULT_DST`), `minix3/minix/kernel/proc.h` (`p_vmrequest` / `RTS_VMREQUEST` / `MF_KCALL_RESUME`)
> **前置**: 18（vircopy/safecopy 系统调用分派）, 09（VM 启动协议——VMCTL_MEMREQ 接口）, 15（SMP——BKL）
> **关联 Rust**: `os/kernel/src/cross_space.rs` (`data_copy_vmcheck`), `os/kernel/src/vm.rs` (`cross_space_copy` / `AddressRef` / `CrossSpaceResult` / `VmCopyContext`), `os/kernel/src/proc.rs` (`suspend_for_vm_with_copy` / `VmSuspendContext`), `os/kernel/src/syscall_copy.rs` (`dispatch_vircopy` / `virtual_copy_vmcheck`), `os/kernel/src/pte_walk.rs` (PTE walk)
>
> **注意**: Minix3 中**不存在** `do_datacopy.c` 文件，也不存在 `SYS_DATACOPY` 内核调用号。`sys_datacopy` 是用户空间库宏（`syslib.h:129`），展开为 `sys_vircopy(p1, v1, p2, v2, len, 0)`，最终发送 `SYS_VIRCOPY` 内核调用，由 `do_copy.c` 中的 `do_copy()` 处理。

---

## Ch1: 概念

**核心问题**: 内核代用户态进程执行系统调用时，如果操作的目标地址在调用方地址空间中**尚未映射**（缺页），内核该如何恢复？

这看似简单——普通进程遇到缺页只需陷入内核由 VM 处理。但**内核代行**场景打破了这个对称性：

- 内核本身**没有缺页处理程序**（设计如此——内核最小化原则）
- 内核正在代行某进程的系统调用（如 SIGSEND 拷贝 sigframe 到用户栈），此时"缺页"发生在内核上下文
- 内核不能自行映射用户页（这是 VM 的职责），也不能直接返回 EFAULT（页可能只是 lazy，VM 能修复）

**VMREQUEST 机制**就是为这个场景设计的"挂起-通知-恢复"协议：

```
内核代行 → 缺页 → 挂起调用方（RTS_VMREQUEST）→ 通知 VM（SIGKMEM）
       → VM 处理缺页 → VM 回复（VMCTL_MEMREQ_REPLY）
       → 内核恢复调用方 → 重试代行操作
```

### 1.1 跨地址空间拷贝的三阶段抽象

无论 C 还是 Rust，跨地址空间拷贝本质都是三个阶段：

| 阶段 | 职责 | 失败处理 |
|------|------|---------|
| **1. 地址解析** | 源/目标虚拟地址 → 物理地址 | 缺页 → 触发 VMREQUEST |
| **2. 物理拷贝** | 在内核地址空间拷贝字节 | 物理内存错误 → EFAULT |
| **3. 结果回报** | 返回 OK / EFAULT / VMSUSPEND | VMSUSPEND → 调用方挂起 |

**阶段 1 的失败**是 VMREQUEST 的触发源。C 用 `virtual_copy_f()`（memory.c）做阶段 1+2，Rust 用 `cross_space_copy()`（vm.rs）做同样的事，区别在于阶段 1 的实现机制：

- **C**：`createpde()` 临时映射源/目标页表项到内核地址空间（`freepdes[]` 数组 + `MEMORY_MUTEX` 互斥保护）
- **Rust**：利用 64-bit 地址空间的 **Direct Map**（`KERNEL_DIRECT_MAP_BASE + PA → KV`）直接访问任意物理页，无需临时映射

### 1.2 VMREQUEST 机制

VMREQUEST 是 Minix3 微内核的核心恢复机制，解决"内核代行时缺页"问题。它的关键设计是**把缺页处理委托给 VM 服务器**——内核不自行映射页，而是：

1. **挂起调用方**：设置 `RTS_VMREQUEST` 标志，进程不可调度
2. **保存上下文**：在 `p_vmrequest`（C）/ `p_vm_suspend`（Rust）中记录请求类型、目标端点、内存范围
3. **链入全局队列**：`vmrequest` 单链表，新请求插入头部
4. **通知 VM**：`send_sig(VM_PROC_NR, SIGKMEM)` 发送内核信号
5. **VM 处理**：VM 收到 SIGKMEM 后调用 `sys_vmctl(VMCTL_MEMREQ_GET)` 取请求，处理后调用 `sys_vmctl(VMCTL_MEMREQ_REPLY, result)` 回复
6. **内核恢复**：清除 `RTS_VMREQUEST`，设置 `MF_KCALL_RESUME`，调度时 `kernel_call_resume()` 重试

### 1.3 VMSUSPEND 语义

`VMSUSPEND`（-996）是内核内部的**伪错误码**（`kernel/vm.h:6`），不是真正的错误，而是"操作因缺页挂起，需 VM 协助"的信号：

```c
#define VMSUSPEND       (-996)
#define EFAULT_SRC      (-995)    // 源地址缺页（不可恢复）
#define EFAULT_DST      (-994)    // 目标地址缺页（不可恢复）
```

**关键区分**：
- `VMSUSPEND` = "页可能 lazy，VM 能修复" → 挂起并等待 VM
- `EFAULT_SRC/DST` = "地址非法" → 直接返回 EFAULT，不挂起

C 把这三种返回值混在 `int` 里，调用方必须用 `if (r == VMSUSPEND)` 区分。Rust 用 `CrossSpaceResult` enum 在类型层分离：

```rust
pub enum CrossSpaceResult {
    Completed(Result<(), VmCopyError>),   // C: OK 或 EFAULT_SRC/DST
    Suspended(VmFaultType),                // C: VMSUSPEND + 故障方向
}
```

### 1.4 三种挂起类型

VMREQUEST 支持三种挂起场景（`proc.h:98-101`）：

| 类型 | 值 | 含义 | 恢复行为 |
|------|---|------|---------|
| `VMSTYPE_KERNELCALL` | 1 | 内核调用被缺页中断 | 设置 `MF_KCALL_RESUME`，调度时 `kernel_call_resume()` 重试 |
| `VMSTYPE_DELIVERMSG` | 2 | 消息投递被缺页中断 | VM 处理后直接清除 `RTS_VMREQUEST` |
| `VMSTYPE_MAP` | 3 | 预留类型 | 仅 `do_vmctl.c:102` 有 case 分支，当前无代码设置 |

本文档聚焦 `VMSTYPE_KERNELCALL`（跨地址空间拷贝场景）；DELIVERMSG 在 12-ipc-core.md 覆盖，MAP 当前未使用。

### 1.5 与 PAGEFAULT 的区别

| 维度 | VMREQUEST | PAGEFAULT |
|------|-----------|-----------|
| 触发者 | 内核代进程操作时缺页 | 进程自身执行时缺页 |
| 通知 VM | `send_sig(SIGKMEM)` + `VMCTL_MEMREQ_GET` | `VM_PAGEFAULT` 通知消息 |
| 恢复方式 | `VMCTL_MEMREQ_REPLY` + 重试内核调用 | VM 映射页后清除 `RTS_PAGEFAULT` |
| 标志位 | `RTS_VMREQUEST` (0x800) | `RTS_PAGEFAULT` (0x400) |
| 谁的"错" | 内核代行时遇到的页不在内存 | 进程自身访问未映射页 |

**关键洞察**：两者都是"页不在内存 → 通知 VM → VM 修复 → 恢复"，但**触发上下文不同**——VMREQUEST 是内核代行，PAGEFAULT 是用户态执行。这决定了通知机制（内核信号 vs VM_PAGEFAULT 消息）和恢复路径（kernel_call_resume vs 直接调度）的差异。

### 1.6 本章不讲什么

- vircopy/safecopy 系统调用分派见 [18-syscall-copy.md](18-syscall-copy.md)
- VM 启动协议 + VMCTL_MEMREQ 接口见 [09-vm-boot-protocol.md](09-vm-boot-protocol.md)
- SIGSEND 使用 `data_copy_vmcheck` 见 [19-syscall-signal.md](19-syscall-signal.md)
- Direct Map + PageTableRef 设计见 02-page-table-kernel.md（其他 stage）
- IPC 消息投递的 VMSUSPEND（DELIVERMSG）见 [12-ipc-core.md](12-ipc-core.md)

---

## Ch2: C 源码分析

### 2.1 syslib.h — sys_datacopy 宏

| 行号 | 定义 | 说明 |
|------|------|------|
| 128 | `/* Shorthands for sys_vircopy() and sys_physcopy() system calls. */` | 注释 |
| 129 | `#define sys_datacopy(p1, v1, p2, v2, len) sys_vircopy(p1, v1, p2, v2, len, 0)` | 用户空间简化接口，flags=0 |
| 130 | `#define sys_datacopy_try(p1, v1, p2, v2, len) sys_vircopy(p1, v1, p2, v2, len, CP_FLAG_TRY)` | 带重试的版本 |
| 131 | `int sys_vircopy(endpoint_t src_proc, vir_bytes src_v, ...)` | `sys_vircopy` 函数声明 |

**关键点**：`sys_datacopy` 不是独立的内核调用。它展开为 `sys_vircopy`，发送 `SYS_VIRCOPY` 内核调用（`com.h:223`，`KERNEL_CALL + 15`），由 `do_copy.c` 中的 `do_copy()` 处理。

### 2.2 com.h — 内核调用号与标志

| 行号 | 定义 | 值 | 说明 |
|------|------|----|------|
| 223 | `#define SYS_VIRCOPY (KERNEL_CALL + 15)` | 15 | `sys_vircopy()` 入口 |
| 224 | `#define SYS_PHYSCOPY (KERNEL_CALL + 16)` | 16 | `sys_physcopy()` 入口 |
| 313 | `#define CP_FLAG_TRY 0x01` | 1 | 不透明映射（VFS 专用 try-copy） |

### 2.3 vm.h — VMSUSPEND 与 EFAULT_SRC/DST

```c
// kernel/vm.h:6-8
#define VMSUSPEND       (-996)
#define EFAULT_SRC      (-995)    // 源地址缺页（不可恢复）
#define EFAULT_DST      (-994)    // 目标地址缺页（不可恢复）
```

这三个负数是内核内部伪错误码，**不对外暴露**（用户空间看到的是 EFAULT=14）。它们仅在 `virtual_copy_f()` / `virtual_copy_vmcheck()` 返回路径中使用，调用方用 `if (r == VMSUSPEND)` 区分"挂起"vs"错误"。

### 2.4 do_copy.c — SYS_VIRCOPY / SYS_PHYSCOPY 处理

`do_copy()`（`do_copy.c:22-90`）是 `SYS_VIRCOPY` / `SYS_PHYSCOPY` 的内核侧处理函数。算法步骤：

1. **解析消息**（L50-56）：从 `m_lsys_krn_sys_copy` 提取 src/dst endpoint + addr + bytes
2. **SELF 替换**（L64-65）：`vir_addr[i].proc_nr_e == SELF` → `caller->p_endpoint`
3. **端点校验**（L67-70）：`isokendpt()` 验证 src/dst endpoint 有效
4. **溢出检查**（L77）：`bytes != (phys_bytes)(vir_bytes) bytes` → `E2BIG`（16-bit vir_bytes 截断保护，64-bit 下不再需要）
5. **分支**（L80-89）：
   - `CP_FLAG_TRY`：调用 `virtual_copy()`，EFAULT_SRC/DST 直接返回（VFS 专用，不挂起）
   - 默认：调用 `virtual_copy_vmcheck(caller, ...)`，可能返回 VMSUSPEND

```c
// do_copy.c:80-89
if(m_ptr->m_lsys_krn_sys_copy.flags & CP_FLAG_TRY) {
    int r;
    assert(caller->p_endpoint == VFS_PROC_NR);
    r = virtual_copy(&vir_addr[_SRC_], &vir_addr[_DST_], bytes);
    if(r == EFAULT_SRC || r == EFAULT_DST) return r = EFAULT;
    return r;
} else {
    return( virtual_copy_vmcheck(caller, &vir_addr[_SRC_],
                      &vir_addr[_DST_], bytes) );
}
```

### 2.5 arch/i386/memory.c — data_copy / data_copy_vmcheck

`data_copy()` 和 `data_copy_vmcheck()`（`memory.c:671-705`）是内核内部辅助函数，被 SIGSEND、GETINFO、DIAGCTL 等使用。它们是 `virtual_copy` / `virtual_copy_vmcheck` 的薄包装，接受 endpoint + addr 参数而非 `vir_addr` 结构：

```c
// memory.c:690-705
int data_copy_vmcheck(struct proc * caller,
    const endpoint_t from_proc, const vir_bytes from_addr,
    const endpoint_t to_proc, const vir_bytes to_addr,
    size_t bytes)
{
  struct vir_addr src, dst;
  src.offset = from_addr;
  dst.offset = to_addr;
  src.proc_nr_e = from_proc;
  dst.proc_nr_e = to_proc;
  assert(src.proc_nr_e != NONE);
  assert(dst.proc_nr_e != NONE);
  return virtual_copy_vmcheck(caller, &src, &dst, bytes);
}
```

**关键点**：`data_copy_vmcheck` 接受 `caller` 参数，因为 `virtual_copy_vmcheck` 需要在缺页时设置 `caller->p_rts_flags |= RTS_VMREQUEST`（通过 `vm_suspend()`）。

### 2.6 proto.h + arch memory.c — virtual_copy_vmcheck 宏 / virtual_copy_f 实现

**关键事实**：Minix3 中**不存在**公共的 `kernel/memory.c` 文件。`virtual_copy_vmcheck` 是 `proto.h:184-185` 定义的**宏**，展开为 `virtual_copy_f(caller, src, dst, bytes, 1)`；`virtual_copy_f` 是**架构相关**实现，分别位于 `arch/i386/memory.c:592-666` 和 `arch/earm/memory.c:497+`。

```c
// proto.h:182-187
#define virtual_copy(src, dst, bytes) \
            virtual_copy_f(NULL, src, dst, bytes, 0)
#define virtual_copy_vmcheck(caller, src, dst, bytes) \
            virtual_copy_f(caller, src, dst, bytes, 1)
int virtual_copy_f(struct proc * caller, struct vir_addr *src,
    struct vir_addr *dst, vir_bytes bytes, int vmcheck);
```

`virtual_copy_f()`（`arch/i386/memory.c:592-666`）算法：

1. **零字节检查**（L608）：`bytes <= 0` → `EDOM`
2. **端点解析**（L614-630）：`vir_addr[i].proc_nr_e` → `proc_addr()`，`NONE` → `NULL`（物理地址）
3. **调用 `lin_lin_copy()`**（L635-636）：实际拷贝原语，内部使用 `createpde()` 临时 PDE 映射
4. **失败处理**（L637-663）：
   - `lin_lin_copy` 返回 `EFAULT_SRC/DST` → 根据 `vmcheck` 标志分支
   - `vmcheck=0`（即 `virtual_copy`）：直接返回 `EFAULT_SRC/DST`
   - `vmcheck=1`（即 `virtual_copy_vmcheck`）：调用 `vm_suspend(caller, target, lin, bytes, VMSTYPE_KERNELCALL, writeflag)`，返回 `VMSUSPEND`

**`lin_lin_copy()` 与临时 PDE 映射机制**（C 特有，arch-specific）：`lin_lin_copy()` 调用 `createpde()` 复用 `freepdes[]` 数组中的空闲 PDE 槽位，临时映射源/目标页表。拷贝完成后释放 PDE。涉及 `MEMORY_MUTEX` 互斥保护（SMP 安全）。

### 2.7 proc.h — p_vmrequest 结构

`p_vmrequest`（`proc.h:95-124`，匿名 struct 起始 + 尾部声明）是 VMREQUEST 的状态存储：

| 字段 | 类型 | 用途 |
|------|------|------|
| `nextrequestor` | `struct proc *` | vmrequest 链表指针 |
| `type` | `int` | 挂起类型（VMSTYPE_*） |
| `saved.reqmsg` | `message` | 被中断的内核调用消息 |
| `req_type` | `int` | 请求参数类型（VMPTYPE_*） |
| `target` | `endpoint_t` | 目标进程 endpoint |
| `params.check.start` | `vir_bytes` | 缺页起始地址 |
| `params.check.length` | `vir_bytes` | 内存范围长度 |
| `params.check.writeflag` | `u8_t` | 非零表示写访问 |
| `vmresult` | `int` | VM 处理结果 |

**相关 RTS/MF 标志**（`proc.h:153-237`）：

| 标志 | 值 | 含义 |
|------|----|------|
| `RTS_VMREQUEST` | 0x800 | 内存请求发起者，等待 VM 回复 |
| `RTS_VMREQTARGET` | 0x1000 | 仅 `debug.c:156` FLAG 名表（状态转储）引用，运行路径不使用 |
| `MF_KCALL_RESUME` | 0x008 | 内核调用被缺页中断，需要恢复 |

### 2.8 proc.c + system.c — vm_suspend / kernel_call_resume

`vm_suspend()`（`proc.c:234-258`）是 VMREQUEST 挂起的核心函数：

1. `assert(!RTS_ISSET(caller, RTS_VMREQUEST))`（L241）——调用方必须未挂起
2. 设置 `RTS_VMREQUEST`（L244，进程不可调度）
3. 填充 `p_vmrequest.{type, target, params.check.{start, length, writeflag}, req_type}`（L246-251）
4. 插入 `vmrequest` 全局链表头部（L254：`caller->p_vmrequest.nextrequestor = vmrequest; vmrequest = caller`）
5. 如果插入前链表为空（L254-257），`send_sig(VM_PROC_NR, SIGKMEM)` 通知 VM

`kernel_call_resume()`（定义在 `system.c:612`，从 `proc.c:361` 调度路径调用）是恢复路径：

1. 检查 `MF_KCALL_RESUME` 标志
2. 重新执行被中断的内核调用（使用 `saved.reqmsg`）
3. 清除 `MF_KCALL_RESUME`

### 2.9 核心函数行为契约（Gate B 依据）

| 函数 | C 行为 | Rust 行为 | 差异类型 |
|------|--------|----------|---------|
| `data_copy_vmcheck` | 接受 `caller*` + endpoint + addr，返回 int (OK/EFAULT/VMSUSPEND) | 接受 `caller: &mut KProcess` + `src/dst: AddressRef` + `proc_cr3` 闭包，返回 CrossSpaceResult | 设计决策 D2/D3/D4 |
| `virtual_copy_vmcheck` | `createpde` 临时 PDE + `lin_lin_copy` | Direct Map + `copy_nonoverlapping` | 架构演进 D1 |
| `vm_suspend` | 修改 caller->p_vmrequest + RTS_VMREQUEST + 链表 + send_sig | `caller.suspend_for_vm_with_copy()` 设置 `p_vm_suspend: Option<VmSuspendContext>` + RTS_VMREQUEST | 设计决策 D5/D6 |
| `do_copy` | 解析消息 + SELF 替换 + isokendpt + 溢出检查 + 分支 | `dispatch_vircopy` 在 syscall_copy.rs 中实现 | 已在 18-syscall-copy.md 覆盖 |
| `kernel_call_resume` | 重新执行 saved.reqmsg | 简单版已实现（vm.rs:896，读 VM 结果 + 清 MF_KCALL_RESUME）并接入 `process_misc_flags`（proc_table.rs KCALL_RESUME 分支）；完整重派发延迟至 `switch_to_user`（见 10-switch-to-user.md §4.2） | 部分实现 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 地址解析机制 | C 临时 PDE 映射 vs Direct Map | **Direct Map** | 64-bit 地址空间天然支持大段 Direct Map，消除 `createpde` 副作用 + `freepdes[]` 全局状态 + `MEMORY_MUTEX` 互斥。redox 也使用类似 Direct Map（`PHYS_OFFSET`）访问物理内存 |
| D2 | 返回值类型 | C int (OK/EFAULT/VMSUSPEND 混合) vs enum | **`CrossSpaceResult` enum** | 类型层区分"完成"vs"挂起"——`Completed(Result<(), VmCopyError>)` vs `Suspended(VmFaultType)`。避免 C 的 `if (r == VMSUSPEND)` 误判（如 `r == EFAULT_SRC` 与 `r == VMSUSPEND` 都是负数） |
| D3 | 地址抽象 | C `vir_addr` struct + sentinel vs Rust enum | **`AddressRef` enum** | `Process{endpoint, offset}` / `Physical(paddr)` 两类，类型层区分。`Endpoint` newtype 防止 int 误用。SELF 替换在 `data_copy_vmcheck` 入口统一处理 |
| D4 | caller 参数 | 隐藏 vs 显式 `&mut KProcess` | **显式 `caller: &mut KProcess`** | VMSUSPEND 语义要求设置 caller 的 `RTS_VMREQUEST` + `p_vm_suspend`——必须 `&mut`。C 用裸指针，Rust 用 `&mut` 显式表达可变借用，编译期保证唯一性 |
| D5 | 挂起上下文 | C `p_vmrequest` 内嵌 union vs Rust 独立 struct | **`VmCopyContext` 独立 struct** | Rust 所有权清晰，可独立传递/存储。`fault_type: VmFaultType` 字段替代 C 的 `EFAULT_SRC/EFAULT_DST` 隐式区分 |
| D6 | vmrequest 链表 | C 全局指针 + nextrequestor vs Rust `VmRequestQueue` | **`VmRequestQueue`** | 封装链表操作 + 显式 insert/remove API。`Option<Endpoint>` 替代裸指针避免 unsafe。BKL 保护注释：所有方法标注 `// SAFETY: Caller must hold BKL` |
| D7 | dispatch_datacopy | 保留死代码 vs 删除 | **删除** | Minix3 无 `SYS_DATACOPY` 调用号；`sys_datacopy` 是用户空间宏，展开为 `sys_vircopy(..., 0)`，内核侧由 `dispatch_vircopy` 处理。死代码 + placeholder bug = 设计债务 |
| D8 | CopyResult 重复 | 保留两个 enum vs 统一 | **统一到 `CrossSpaceResult`** | DRY：`cross_space::CopyResult` 与 `vm::CrossSpaceResult` 语义重叠。`CrossSpaceResult::Suspended(VmFaultType)` 比 `CopyResult::VmSuspend` 信息更丰富（含 Src/Dst）。删除 `CopyResult` 消除无意义转换 |
| D9 | PTE walk 架构分派 | `#[cfg(target_arch)]` vs trait 静态分派 | **trait 静态分派** | 消除条件编译分散（Pattern #14）；trait bound 让上层代码泛型化；编译期单态化零开销。x86_64/aarch64/riscv64 各自实现 `PagingArch` |

---

## Ch4: 实现详解

### 4.1 cross_space_copy — 跨地址空间拷贝原语

`cross_space_copy<D: DirectMapArch>()`（`os/kernel/src/vm.rs:327-370`）是跨地址空间拷贝的核心原语，对应 C 的 `virtual_copy_f()`：

```rust
pub fn cross_space_copy<D: DirectMapArch>(
    src: &AddressRef,
    dst: &AddressRef,
    bytes: usize,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult
```

**算法**（三阶段，对齐 Ch1 §1.1）：

1. **阶段 1（地址解析）**：`resolve_physical::<D>(src, &proc_cr3)` — 通过 `proc_cr3` 闭包获取源进程的 CR3，调用 `lookup_in_table::<D>(cr3, offset)` 执行 PTE walk（x86_64: PML4→PDPT→PD→PT）。如果 PTE 不存在，返回 `ResolveError::PageFault` → `CrossSpaceResult::Suspended(VmFaultType::Src)`
2. **阶段 2（物理拷贝）**：`D::kernel_phys_to_virt(src_phys)` 将物理地址转为内核虚拟地址，`core::ptr::copy_nonoverlapping` 执行拷贝
3. **阶段 3（结果回报）**：返回 `CrossSpaceResult::Completed(Ok(()))` / `Completed(Err(VmCopyError::*))` / `Suspended(VmFaultType::*)`

**关键差异**（vs C `virtual_copy_f`）：
- C 用 `createpde` 临时映射 PDE，Rust 用 Direct Map 直接访问
- C 用 `lin_lin_copy` 拷贝，Rust 用 `copy_nonoverlapping`（语义等价，Rust 类型安全）
- C 返回 int（VMSUSPEND/EFAULT_SRC/DST 混合），Rust 返回 `CrossSpaceResult` enum

### 4.2 data_copy_vmcheck — 内核内部入口

`data_copy_vmcheck()`（`os/kernel/src/cross_space.rs:118-179`）是内核内部跨地址空间拷贝的入口，对应 C 的 `data_copy_vmcheck()`（`arch/i386/memory.c:690-705`）：

```rust
pub fn data_copy_vmcheck(
    caller: &mut KProcess,
    src: AddressRef,
    dst: AddressRef,
    bytes: usize,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult
```

**算法**：

1. 调用 `cross_space_copy::<CurrentDirectMap>(&src, &dst, bytes, &proc_cr3)` 执行三阶段拷贝（阶段 1 地址解析由 PTE walk 完成，`proc_cr3` 闭包解析 Endpoint → CR3/TTBR0）
2. **VMSUSPEND 处理**（关键差异，D4）：如果返回 `Suspended(fault_type)`：
   - 构造 `VmCopyContext::new(src, dst, bytes, fault_type)` 保存挂起上下文（D5）
   - 构造 `VmCheckParams { start, length, write_flag }` 记录缺页信息（`start` = 故障侧地址，`length` = 完整 `bytes`，`write_flag` = Dst→true/Src→false）
   - 调用 `caller.suspend_for_vm_with_copy(KernelCall, target, params, None, copy_ctx)` 设置 `RTS_VMREQUEST` + `p_vm_suspend`（对应 C 的 `vm_suspend()`）

**调用方职责**（D3 反译差异）：`AddressRef` 与 `proc_cr3` 闭包由**调用方**构造（如 `dispatch_copy`，见 §4.5）——C 的 `data_copy_vmcheck` 内部用 `isokendpt()` + `proc_addr()` 解析 endpoint（memory.c:690-705），Rust 改为参数传入，避免同时借用多个 `KProcess` 的生命周期冲突（Anti-translate note，cross_space.rs:100-106）。

**关键不变量**：返回 `Suspended(_)` 时，调用方**不得**修改进程寄存器（如 SIGSEND 的 sigframe 设置），因为进程将被恢复并以原寄存器状态重试拷贝。

**与 C 的关键差异**：
- C 的 `data_copy_vmcheck` 只返回 `VMSUSPEND`，调用方负责调用 `vm_suspend()`
- Rust 的 `data_copy_vmcheck` **内联** VMSUSPEND 处理——直接调用 `caller.suspend_for_vm_with_copy()`
- 理由：Rust 借用检查器要求 `&mut KProcess`，把 `vm_suspend` 逻辑内联到 `data_copy_vmcheck` 让借用关系更显式

### 4.3 suspend_for_vm_with_copy — VMSUSPEND 状态设置

`KProcess::suspend_for_vm_with_copy()`（`os/kernel/src/proc.rs:1417-1438`）对应 C 的 `vm_suspend()`：

```rust
pub fn suspend_for_vm_with_copy(
    &mut self,
    suspend_type: VmSuspendType,
    target: Endpoint,
    params: VmCheckParams,
    saved_msg: Option<Message>,
    copy_ctx: VmCopyContext,
) {
    debug_assert!(!self.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    debug_assert!(self.p_vm_suspend.is_none());
    self.p_vm_suspend = Some(VmSuspendContext {
        suspend_type, target, check_params: params,
        state: VmSuspendState::Pending,
        saved_msg, copy_context: Some(copy_ctx),
    });
    self.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
}
```

**与 C `vm_suspend()` 的差异**：
- C 修改 `caller->p_vmrequest` 内嵌 union + `nextrequestor` 指针 + `vmrequest` 全局链表
- Rust 设置 `p_vm_suspend: Option<VmSuspendContext>`（D5）+ `RTS_VMREQUEST`
- **当前状态**：`VmRequestQueue` 已实现（vm.rs:534，`enqueue`@577 / `dequeue_filtered`@594 / `enqueue_and_notify`@646 / `remove`@663 + 测试），但 `suspend_for_vm_with_copy` 尚未调用 `enqueue_and_notify`——挂起进程不会进入队列，也不会向 VM 发送 SIGKMEM（见 §4.6）

### 4.4 VmSuspendContext — 挂起状态

`VmSuspendContext`（`os/kernel/src/vm.rs:494-522`）是 VMREQUEST 的状态存储，替代 C 的 `p_vmrequest`：

| Rust 字段 | C 对应 | 类型差异 |
|----------|--------|---------|
| `suspend_type: VmSuspendType` | `p_vmrequest.type` (int) | enum 替代 int（D5） |
| `target: Endpoint` | `p_vmrequest.target` (endpoint_t) | newtype 替代 int |
| `check_params: VmCheckParams` | `p_vmrequest.params.check` | 独立 struct + `bool` 替代 `u8_t` |
| `state: VmSuspendState` | `p_vmrequest.vmresult` (int) | enum 状态机替代三态 sentinel |
| `saved_msg: Option<Message>` | `p_vmrequest.saved.reqmsg` | Option 替代 always-embedded |
| `copy_context: Option<VmCopyContext>` | （C 无对应） | Rust 新增，用于 cross_space_copy 恢复 |

### 4.5 dispatch_vircopy — 系统调用分派

`dispatch_vircopy()`（`os/kernel/src/syscall_copy.rs:220-226`）处理 `SYS_VIRCOPY` / `SYS_PHYSCOPY` 系统调用，对应 C 的 `do_copy()`。它是薄包装，委托给 `dispatch_copy()`（syscall_copy.rs:242-359）。详见 [18-syscall-copy.md](18-syscall-copy.md)。

**当前状态**（`dispatch_copy` 内部分支，对齐 C `do_copy.c:80-89`）：
- **CP_FLAG_TRY 路径**（VFS 专用）：直接调用 `cross_space_copy::<CurrentDirectMap>`，`Suspended(_)` → `EFAULT`（不挂起）
- **默认路径**：调用 `data_copy_vmcheck(caller, src, dst, bytes, proc_cr3)`（cross_space.rs:118）执行带 VM 检查的拷贝，`Suspended(_)` → `KcallResult::VmSuspend`（挂起调用方，等待 VM 修复后重试）

### 4.6 已知缺口（诚实标注 DEFERRED）

| 缺口 | 严重度 | 理由 | 计划 |
|------|--------|------|------|
| 挂起路径未接入 `VmRequestQueue` + SIGKMEM 通知 | P1 DEFERRED | `VmRequestQueue` 已实现（vm.rs:534，`enqueue`@577 / `dequeue_filtered`@594 / `enqueue_and_notify`@646 / `remove`@663 + 测试），但 `suspend_for_vm_with_copy`（proc.rs:1417）只设置 `RTS_VMREQUEST` + `p_vm_suspend`，未调用 `enqueue_and_notify`——无 SIGKMEM 触发路径 | 随 SIGSEND 实现落地 |
| `kernel_call_resume()` 完整重派发 | P1 DEFERRED | 简单版（vm.rs:896）已接入 `process_misc_flags`（proc_table.rs KCALL_RESUME 分支，FIX-21）；完整重派发（重新执行 `saved.reqmsg`）延迟至 `switch_to_user`（见 10-switch-to-user.md §4.2） | 随调度器集成推进 |
| ~~aarch64/riscv64 PTE walk~~ | ~~P2 DEFERRED~~ | ✅ **已实现（2026-08-14 核实）**：三架构 `PteWalkArch` 完整实现——x86_64/paging.rs、arm64/paging.rs:310（`walk_translate` 4 级）、riscv64/paging.rs:334（Sv39 3 级） | 已解决 |
| `VmSuspendContext` 完整三类（KERNELCALL/DELIVERMSG/MAP） | P2 DEFERRED | 当前仅 `KernelCall` + `DeliverMsg`，`Map` 未使用 | 随 IPC/message deliver 推进 |
| 部分拷贝进度报告 | P2 WONTFIX | 代码注释已定案（cross_space.rs:139-145）：`cross_space_copy` 重试幂等（写后写同内容安全），`VmCheckParams.length` 用完整 `bytes` 是安全的；内核内部拷贝（sigframe/getinfo/diagctl）均为小拷贝，额外 VM fault 处理成本可接受 | WONTFIX（不实现） |

---

## Ch5: 测试要点

### 5.1 测试矩阵

| 测试名 | 类型 | 覆盖点 | C 对齐 |
|--------|------|--------|--------|
| `test_cross_space_result_variants` | L1 | CrossSpaceResult 三变体互不相等 | C: OK/EFAULT/VMSUSPEND 互不相等 |
| `test_completed_ok_matches_c_ok` | L1 | `Completed(Ok(()))` 对应 C `return OK` | memory.c:665（virtual_copy_f 成功路径 `return OK`） |
| `test_suspended_src_matches_c_vmsuspend_src` | L1 | `Suspended(VmFaultType::Src)` 对应 C `VMSUSPEND` + EFAULT_SRC | memory.c virtual_copy_f |
| `test_suspended_dst_matches_c_vmsuspend_dst` | L1 | `Suspended(VmFaultType::Dst)` 对应 C `VMSUSPEND` + EFAULT_DST | memory.c virtual_copy_f |
| `test_cross_space_result_match_exhaustive` | L2 | match 穷尽性（新增 variant 时编译失败） | — |
| `test_vm_copy_context_construction` | L1 | VmCopyContext 保存 src/dst/bytes/fault_type | C: `p_vmrequest.params.check` |

### 5.2 待补充测试（随 DEFERRED 项落地）

| 测试名 | 类型 | 覆盖点 |
|--------|------|--------|
| `test_data_copy_vmcheck_parity_with_c` | L1 | data_copy_vmcheck 与 C 行为对照（OK/Fault/Suspend） |
| `test_data_copy_vmcheck_sets_rts_vmrequest` | L1 | VMSUSPEND 后 caller.p_rts_flags 含 VMREQUEST |
| `test_data_copy_vmcheck_preserves_copy_context` | L1 | VMSUSPEND 后 caller.p_vm_suspend.copy_context 字段正确 |
| `test_zero_byte_copy_returns_ok` | 边界 | 零字节拷贝返回 Completed(Ok) |
| `test_self_replacement_at_dispatcher` | L1 | SELF → caller.p_endpoint 替换 |

**已落地（从待补充移出）**：

| 测试名 | 位置 | 覆盖点 |
|--------|------|--------|
| `vm_request_queue_new_is_empty`（vm.rs:1100）/ `vm_request_queue_enqueue_returns_was_empty`（:1106）/ `vm_request_queue_dequeue_filtered_empty`（:1118）/ `vm_request_queue_dequeue_filtered_returns_first_match`（:1126）/ `vm_request_queue_remove`（:1136）/ `memreq_get_dequeues_and_sets_fetched`（:1368） | vm.rs（`mod tests`，无 `test_` 前缀） | VmRequestQueue 队列操作（D6 已实现） |
| `kernel_call_resume_returns_ok_on_success` / `kernel_call_resume_returns_fault_on_failure` | vm.rs:1303/1316 | kernel_call_resume 简单版（读 VM 结果 + 清标志） |
| `check_resumed_caller_returns_ok_when_no_resume` / `check_resumed_caller_returns_result_when_resumed` | vm.rs:1328/1334 | check_resumed_caller（memory.c:135-145 对应） |

---

## Ch6: 参见

- [18-syscall-copy.md](18-syscall-copy.md) — vircopy/safecopy 系统调用分派（`dispatch_vircopy`）
- [19-syscall-signal.md](19-syscall-signal.md) — SIGSEND 使用 `data_copy_vmcheck`
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) — VM 启动协议 + VMCTL_MEMREQ_GET/REPLY 接口
- [12-ipc-core.md](12-ipc-core.md) — IPC 消息投递的 VMSUSPEND（VMSTYPE_DELIVERMSG）
- [15-clock-timer.md](15-clock-timer.md) — SMP BKL（`suspend_for_vm_with_copy` 需 BKL 保护）
- 02-page-table-kernel.md（其他 stage）— Direct Map + `PageTableRef` + `AddressRef` + `CrossSpaceResult` 设计

---

## 附录 A: 与 redox 对照

| 维度 | Minix3 | redox | minix-rs 选择 |
|------|--------|-------|--------------|
| 跨地址空间拷贝 | VMREQUEST + 临时 PDE (`createpde`) | scheme-based + 临时映射 | Direct Map + `CrossSpaceResult` |
| 缺页处理 | 内核挂起 + VM 协助 | 用户态 scheme 直接处理 | 沿用 Minix3 VMREQUEST（微内核架构对齐） |
| 返回值 | `int` (OK/EFAULT/VMSUSPEND) | `Result + 特定 Error` | `CrossSpaceResult` enum（类型层区分完成 vs 挂起） |
| 地址抽象 | `vir_addr` struct + sentinel | scheme token + offset | `AddressRef` enum（Process/Physical） |
| 挂起队列 | 裸指针 + `nextrequestor` | 无全局链表（per-scheme） | `VmRequestQueue` 封装已实现（vm.rs:534 + 测试）；挂起路径入队接线待实现（§4.6 首行） |
| 内核代行缺页 | VMREQUEST 机制 | scheme 自处理（内核不代行） | 沿用 VMREQUEST（minix-rs 内核仍代行系统调用） |

**关键差异**：redox 的 scheme 模型把"地址空间"抽象为 scheme token，每个 scheme 自管理内存映射；Minix3 的 VMREQUEST 是"内核代行时缺页"的恢复机制。两者解决不同问题——redox 是"谁拥有内存"，Minix3 是"内核代行时如何恢复"。minix-rs 沿用 Minix3 VMREQUEST 因为微内核架构要求内核最小化，不能自行处理缺页。
