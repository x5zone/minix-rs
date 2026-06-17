# 08-vm-boot-protocol: VM 启动后的内核-VM 协商协议

> **分类**: Kernel IPC 协议
> **源码**: `minix3/minix/kernel/system/do_vmctl.c`, `minix3/minix/kernel/arch/i386/arch_do_vmctl.c`
> **前置**: 07（bsp_finish_booting 完成，VM 已开始运行）
> **C 总行数**: ~230 行

---

## 1. 概述

### 1.1 核心问题

VM 是页表的所有者，但它刚启动时只有 kernel 给的 bootstrap 页表。内核和 VM 之间如何协商，让系统进入"运行时可用"状态？

内核通过 `SYS_VMCTL` 系统调用接收 VM 的请求。VM 在启动过程中依次发送多个 `VMCTL_*` 子命令，完成地址空间切换、物理映射声明、VMINHIBIT 清除等操作。

### 1.2 协商时序

```
1. switch_to_user() → pick_proc() → 选 VM（唯一无 VMINHIBIT 的进程）
2. VM 用户态代码执行 init_page_table() → map_kernel()
   → 建立 kernel direct map（KERNEL_DIRECT_MAP_BASE, U/S=0, G=1）
3. VM → SYS_VMCTL(VMCTL_SETADDRSPACE): 切换 CR3 到 VM 的真实页表
4. VM → SYS_VMCTL(VMCTL_KERN_PHYSMAP): 内核声明需映射的物理区
5. VM → SYS_VMCTL(VMCTL_KERN_MAP_REPLY): VM 返回虚拟地址
6. VM 为 PM/VFS/RS 等创建页表
7. VM → SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR): 解除所有进程的 VMINHIBIT
8. vm_running = 1
```

### 1.3 双视图地址空间模型

| 属性 | VM direct map | Kernel direct map |
|------|--------------|-------------------|
| 虚拟地址基址 | `0x0000_0000_8000_0000` | `0xFFFF_8000_0000_0000` |
| U/S 位 | 1（用户态可访问） | 0（仅内核态可访问） |
| G 位 | 0 | 1（CR3 切换不刷新 TLB） |
| 建立者 | Kernel（arch_boot_proc） | VM（`map_kernel()`） |
| 建立时机 | VM 启动前（T5） | VM 启动后（T11） |

**关键洞察**：这不是"两份映射"，而是"同一物理内存在不同特权级下的两个必要窗口"。x86-64 的 U/S 位不可能同时为 0 和 1，因此两个窗口是硬件的必然要求。

---

## 2. C 源码分析

### 2.1 do_vmctl() — 通用 VMCTL 分派

**源码**: `minix3/minix/kernel/system/do_vmctl.c:17-173`

`do_vmctl()` 是 `SYS_VMCTL` 系统调用的入口函数。它接收 VM 发来的消息，根据 `SVMCTL_PARAM` 字段分派到不同的处理逻辑。

> **Rust 实现位置**：当前实现为 `os/kernel/src/syscall.rs` 中的 `dispatch_vmctl()`，由 `kernel_call_dispatch()` 统一分派，而不是独立的 `vm.rs::do_vmctl()` 函数。

**子命令清单**：

| 子命令 | 行号 | 语义 | 设置的 RTS/MF 标志 |
|--------|------|------|-------------------|
| `VMCTL_CLEAR_PAGEFAULT` | 32-35 | 清除进程的页错误标志 | `RTS_UNSET(PAGEFAULT)` |
| `VMCTL_MEMREQ_GET` | 36-72 | 获取下一个 VM 请求 | 读 `vmrequest` 链表 |
| `VMCTL_MEMREQ_REPLY` | 73-104 | VM 回复请求结果 | `RTS_UNSET(VMREQUEST)`, `MF_KCALL_RESUME` |
| `VMCTL_KERN_PHYSMAP` | 105-112 | 声明物理映射区 | 调用 `arch_phys_map()` |
| `VMCTL_KERN_MAP_REPLY` | 113-118 | VM 返回虚拟地址 | 调用 `arch_phys_map_reply()` |
| `VMCTL_VMINHIBIT_SET` | 119-131 | 设置 VMINHIBIT | `RTS_SET(VMINHIBIT)`, `MF_FLUSH_TLB` |
| `VMCTL_VMINHIBIT_CLEAR` | 132-160 | 清除 VMINHIBIT | `RTS_UNSET(VMINHIBIT)` |
| `VMCTL_CLEARMAPCACHE` | 161-164 | 清除映射缓存 | 调用 `mem_clear_mapcache()` |
| `VMCTL_BOOTINHIBIT_CLEAR` | 165-167 | 清除 BOOTINHIBIT | `RTS_UNSET(BOOTINHIBIT)` |

**未在 switch 中处理的子命令**：传递给 `arch_do_vmctl()` 处理（do_vmctl.c:172）。

### 2.2 arch_do_vmctl() — x86 架构特定 VMCTL

**源码**: `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:38-65`

| 子命令 | 行号 | 语义 |
|--------|------|------|
| `VMCTL_GET_PDBR` | 50-52 | 获取进程 CR3 值 |
| `VMCTL_SETADDRSPACE` | 53-55 | 设置进程 CR3 + 虚拟地址 |
| `VMCTL_FLUSHTLB` | 56-59 | 刷新 TLB（reload_cr3） |
| `VMCTL_I386_INVLPG` | 60-63 | 单页 TLB 失效 |

**setcr3() 辅助函数**（arch_do_vmctl.c:18-28）：

```c
static void setcr3(struct proc *p, u32_t cr3, u32_t *v)
{
    p->p_seg.p_cr3 = cr3;
    p->p_seg.p_cr3_v = v;
    if(p == get_cpulocal_var(ptproc)) {
        write_cr3(p->p_seg.p_cr3);
    }
    if(p->p_nr == VM_PROC_NR) {
        if (arch_enable_paging(p) != OK)
            panic("arch_enable_paging failed");
    }
    RTS_UNSET(p, RTS_VMINHIBIT);
}
```

**关键观察**：
- `VMCTL_SETADDRSPACE` 同时清除 `RTS_VMINHIBIT`——VM 设置完页表后进程立即可调度
- 如果设置的是当前运行进程（ptproc）的 CR3，立即刷新硬件 CR3
- 如果设置的是 VM 进程的 CR3，调用 `arch_enable_paging()` 启用分页

### 2.3 VMCTL_MEMREQ_GET/REPLY — VM 请求获取与回复

**VMCTL_MEMREQ_GET**（do_vmctl.c:36-72）：

遍历 `vmrequest` 链表，找到第一个通过 IPC 过滤器的请求，返回请求信息：
- `SVMCTL_MRG_TARGET`: 请求目标端点
- `SVMCTL_MRG_ADDR`: 检查起始地址
- `SVMCTL_MRG_LENGTH`: 检查长度
- `SVMCTL_MRG_FLAG`: 写标志
- `SVMCTL_MRG_REQUESTOR`: 请求者端点

设置 `vmresult = VMSUSPEND`，从链表中移除该请求。

**VMCTL_MEMREQ_REPLY**（do_vmctl.c:73-104）：

VM 回复请求结果。根据 `VMSTYPE_*` 类型设置不同的恢复标志：
- `VMSTYPE_KERNELCALL`: 设置 `MF_KCALL_RESUME`
- `VMSTYPE_DELIVERMSG`: 断言 `MF_DELIVERMSG` 已设置
- `VMSTYPE_MAP`: 断言 `RTS_VMREQUEST` 已设置

然后清除 `RTS_VMREQUEST`，使进程可重新调度。

### 2.4 VMCTL_VMINHIBIT_SET/CLEAR — VM 抑制控制

**VMCTL_VMINHIBIT_SET**（do_vmctl.c:119-131）：
- SMP：如果进程在不同 CPU 上，发送 IPI `smp_schedule_vminhibit`
- 设置 `RTS_VMINHIBIT`，阻止进程调度
- SMP：设置 `MF_FLUSH_TLB`，标记需要 TLB 刷新

**VMCTL_VMINHIBIT_CLEAR**（do_vmctl.c:132-160）：
- 清除 `RTS_VMINHIBIT`，允许进程调度
- SMP：如果有 `MF_SENDA_VM_MISS`，尝试重新投递异步消息
- SMP：标记所有 CPU 的 stale TLB

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_address_space / 页表切换 | trait 抽象 vs 推迟实现 | **当前未抽象为独立 trait** | 08 早期版本曾定义 `PageTableSwitcher`，但因无实现且非当前 VMCTL 子命令入口，已在修复中移除；地址空间切换随 09/15 调度阶段统一实现 |
| KERN_PHYSMAP / KERN_MAP_REPLY | 保留 vs 删除 | **保留协议，64 位实现为 noop** | 64 位 direct map 已就绪，但协议层保留向后兼容 |
| VMINHIBIT_CLEAR | 逐进程 vs 批量 | **批量** | 保持 C 语义——VM 逐个调用 |
| vm_running 置位时机 | SETADDRSPACE 后 vs VMINHIBIT_CLEAR 后 | **SETADDRSPACE 后** | 与 C 一致 |
| VMCTL 子命令分派 | match vs 函数指针数组 | **enum VmCtlParam + match** | 与 syscall.rs 设计一致，编译期穷尽 |
| MEMREQ_GET/REPLY | 直接操作 vmrequest 链表 vs VmRequestQueue | **VmRequestQueue 方法** | 复用 vm.rs 已有类型 |
| do_vmctl 代码归属 | vm.rs vs syscall.rs | **syscall.rs `dispatch_vmctl`** | 当前实现将 SYS_VMCTL 与其他内核调用统一在 `kernel_call_dispatch` 中分派 |

---

## 4. 实现要点

### 4.1 VmCtlParam 枚举

```rust
/// VMCTL 子命令参数。
/// C: SVMCTL_PARAM 字段，minix/com.h VMCTL_* 定义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VmCtlParam {
    ClearPageFault,     // VMCTL_CLEAR_PAGEFAULT
    MemReqGet,          // VMCTL_MEMREQ_GET
    MemReqReply,        // VMCTL_MEMREQ_REPLY
    KernPhysMap,        // VMCTL_KERN_PHYSMAP
    KernMapReply,       // VMCTL_KERN_MAP_REPLY
    VmInhibitSet,       // VMCTL_VMINHIBIT_SET
    VmInhibitClear,     // VMCTL_VMINHIBIT_CLEAR
    ClearMapCache,      // VMCTL_CLEARMAPCACHE
    BootInhibitClear,   // VMCTL_BOOTINHIBIT_CLEAR
    // arch-specific
    GetPdbr,            // VMCTL_GET_PDBR (x86)
    SetAddrSpace,       // VMCTL_SETADDRSPACE (x86)
    FlushTlb,           // VMCTL_FLUSHTLB (x86)
    InvlPg,             // VMCTL_I386_INVLPG (x86)
}
```

### 4.2 页表切换抽象（已移除）

> 08 早期版本曾定义 `PageTableSwitcher` trait，意图将 `switch_address_space()` / `__switch_address_space()` 抽象为跨架构接口。但该 trait 没有实现，也未被任何代码使用，属于死代码。按 review 规则移除，地址空间切换将随调度阶段（09/15）统一实现。

### 4.3 do_vmctl 分派

```rust
// os/kernel/src/syscall.rs

/// 处理 SYS_VMCTL 系统调用。
/// C: do_vmctl() — do_vmctl.c:17-173
///
/// 当前实现位于 syscall.rs，由 `kernel_call_dispatch` 统一分派。
fn dispatch_vmctl(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    // 1. 权限检查：仅 system process 可调用。
    // 2. 解析 SVMCTL_WHO / SVMCTL_PARAM / SVMCTL_VALUE。
    // 3. match VmCtlParam 分派到 ClearPageFault / MemReqGet /
    //    MemReqReply / VmInhibitSet / VmInhibitClear / BootInhibitClear 等。
    // 4. 架构特定命令（GetPdbr / SetAddrSpace / FlushTlb / InvlPg）
    //    当前返回 ENOSYS，待后续调度/页表阶段实现。
}
```

> 代码归属修正：本文档早期版本假设 `do_vmctl` 在 `vm.rs` 中实现；实际代码将其放在 `syscall.rs` 作为 `dispatch_vmctl`，与其他 SYS_* 调用统一分派。

### 4.4 KERN_PHYSMAP / KERN_MAP_REPLY 的 64 位处理

在 64 位 + Direct Map 模型下，内核可以直接访问所有物理内存，因此：
- `VMCTL_KERN_PHYSMAP`: 返回物理地址本身（Direct Map 下无需额外映射）
- `VMCTL_KERN_MAP_REPLY`: noop（无需记录虚拟地址映射）

但协议层保留，注释标注"32 位遗留"。

---

## 5. 测试

### 5.1 单元测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_vmctl_param_from_u32` | `vm.rs:1295` | 合法/非法 `VmCtlParam` 转换 |
| `test_vmctl_result_variants` | `vm.rs:1311` | `VmCtlResult` 与 C 错误码对应 |
| `test_vm_memreq_get_empty_queue` | `proc_table.rs:1071` | 空队列返回 ENOENT |
| `test_vm_memreq_get_dequeues_pending_request` | `proc_table.rs:1080` | MemReqGet 取出 pending 请求 |
| `test_vm_memreq_reply_completes_request` | `proc_table.rs:1126` | MemReqReply 完成请求并设置结果 |
| `test_vm_memreq_reply_invalid_state` | `proc_table.rs:1164` | 非法状态下回复返回错误 |

### 5.2 集成测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_vm_suspend_state_*` | `vm.rs` | `VmSuspendState` 状态机转换 |
| `kernel_call_resume_*` | `vm.rs` | `kernel_call_resume` 行为 |

---

## 6. 参见

- [07-system-init-boot-finish](07-system-init-boot-finish.md) — bsp_finish_booting 启动调度循环
- [10-scheduling-primitives](10-scheduling-primitives.md) — VMINHIBIT 对调度的影响
- [12-syscall-dispatch](12-syscall-dispatch.md) — SYS_VMCTL 的分派路径
