# 09-vm-boot-protocol: VM 启动后的内核-VM 协商协议

> **分类**: Kernel IPC 协议
> **源码**: `minix3/minix/kernel/system/do_vmctl.c`, `minix3/minix/kernel/arch/i386/arch_do_vmctl.c`
> **前置**: 08（bsp_finish_booting 完成，VM 已开始运行）
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
8. vm_running = 1（由 step3 的 SetAddrSpace 触发；详见 §3 决策"vm_running 置位时机"）
```

> **Step 8 说明**：Minix3 C 源码实际从未设 `vm_running = 1`（`main.c:47` 只设 0），这是 C 的 bug。Rust 在 `SetAddrSpace` 成功且 target 是 `VM_PROC_NR` 时调用 `set_vm_running(true)` 修正此遗漏。Step 8 与 Step 3 是同一 `SetAddrSpace` 调用的副作用，时序上 Step 3 完成后 `vm_running` 即为 1。

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
| `VMCTL_CLEAR_PAGEFAULT` | 33-36 | 清除进程的页错误标志 | `RTS_UNSET(PAGEFAULT)` |
| `VMCTL_MEMREQ_GET` | 37-79 | 获取下一个 VM 请求 | 读 `vmrequest` 链表 |
| `VMCTL_MEMREQ_REPLY` | 81-110 | VM 回复请求结果 | `RTS_UNSET(VMREQUEST)`, `MF_KCALL_RESUME` |
| `VMCTL_KERN_PHYSMAP` | 112-119 | 声明物理映射区 | 调用 `arch_phys_map()` |
| `VMCTL_KERN_MAP_REPLY` | 120-124 | VM 返回虚拟地址 | 调用 `arch_phys_map_reply()` |
| `VMCTL_VMINHIBIT_SET` | 125-136 | 设置 VMINHIBIT | `RTS_SET(VMINHIBIT)`, `MF_FLUSH_TLB` |
| `VMCTL_VMINHIBIT_CLEAR` | 137-161 | 清除 VMINHIBIT | `RTS_UNSET(VMINHIBIT)` |
| `VMCTL_CLEARMAPCACHE` | 162-165 | 清除映射缓存 | 调用 `mem_clear_mapcache()` |
| `VMCTL_BOOTINHIBIT_CLEAR` | 166-168 | 清除 BOOTINHIBIT | `RTS_UNSET(BOOTINHIBIT)` |

**未在 switch 中处理的子命令**：传递给 `arch_do_vmctl()` 处理（do_vmctl.c:172）。

### 2.2 arch_do_vmctl() — x86 架构特定 VMCTL

**源码**: `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:38-67`

| 子命令 | 行号 | 语义 |
|--------|------|------|
| `VMCTL_GET_PDBR` | 44-47 | 获取进程 CR3 值 |
| `VMCTL_SETADDRSPACE` | 48-50 | 设置进程 CR3 + 虚拟地址 |
| `VMCTL_FLUSHTLB` | 51-55 | 刷新 TLB（reload_cr3） |
| `VMCTL_I386_INVLPG` | 56-60 | 单页 TLB 失效 |

**setcr3() 辅助函数**（arch_do_vmctl.c:19-33）：

```c
static void setcr3(struct proc *p, u32_t cr3, u32_t *v)
{
    /* Set process CR3. */
    p->p_seg.p_cr3 = cr3;
    assert(p->p_seg.p_cr3);
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

**VMCTL_MEMREQ_GET**（do_vmctl.c:37-79）：

遍历 `vmrequest` 链表，找到第一个通过 IPC 过滤器的请求，返回请求信息：
- `SVMCTL_MRG_TARGET`: 请求目标端点
- `SVMCTL_MRG_ADDR`: 检查起始地址
- `SVMCTL_MRG_LENGTH`: 检查长度
- `SVMCTL_MRG_FLAG`: 写标志
- `SVMCTL_MRG_REQUESTOR`: 请求者端点

设置 `vmresult = VMSUSPEND`，从链表中移除该请求。无匹配请求时返回 `ENOENT`。

**VMCTL_MEMREQ_REPLY**（do_vmctl.c:81-110）：

VM 回复请求结果。根据 `VMSTYPE_*` 类型设置不同的恢复标志：
- `VMSTYPE_KERNELCALL`: 设置 `MF_KCALL_RESUME`
- `VMSTYPE_DELIVERMSG`: 断言 `MF_DELIVERMSG` 已设置
- `VMSTYPE_MAP`: 断言 `RTS_VMREQUEST` 已设置

然后清除 `RTS_VMREQUEST`，使进程可重新调度。

### 2.4 VMCTL_VMINHIBIT_SET/CLEAR — VM 抑制控制

**VMCTL_VMINHIBIT_SET**（do_vmctl.c:125-136）：
- SMP：如果进程在不同 CPU 上，发送 IPI `smp_schedule_vminhibit`
- 设置 `RTS_VMINHIBIT`，阻止进程调度
- SMP：设置 `MF_FLUSH_TLB`，标记需要 TLB 刷新

**VMCTL_VMINHIBIT_CLEAR**（do_vmctl.c:137-161）：
- 清除 `RTS_VMINHIBIT`，允许进程调度
- SMP：如果有 `MF_SENDA_VM_MISS`，尝试重新投递异步消息
- SMP：标记所有 CPU 的 stale TLB

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_address_space / 页表切换 | trait 抽象 vs 推迟实现 | **当前未抽象为独立 trait** | 08 早期版本曾定义 `PageTableSwitcher`，但因无实现且非当前 VMCTL 子命令入口，已移除；`SetAddrSpace` 分支直接在 `dispatch_vmctl` 中实现数据层（p_seg + VMINHIBIT + vm_running），`write_cr3` 部分待 ptproc 跟踪 + arch trait 非零页表构造器扩展后补齐 |
| KERN_PHYSMAP / KERN_MAP_REPLY | 保留 vs 删除 | **保留协议，64 位返回 ENOSYS** | 64 位 direct map 已就绪，协议层保留向后兼容；返回 ENOSYS 而非 noop，让调用方明确知道功能未启用 |
| VMINHIBIT_CLEAR | 逐进程 vs 批量 | **逐进程** | 保持 C 语义——VM 逐个调用 |
| vm_running 置位时机 | SETADDRSPACE 后 vs VMINHIBIT_CLEAR 后 | **SETADDRSPACE 后**（**修正 C bug**） | **C 源码 bug**：`vm_running` 只在 `main.c:47` 设为 0，从未设为 1（`rg "vm_running\s*=" minix3/minix/kernel/` 证实）。Rust 修正此遗漏：`SetAddrSpace` 成功且 target 是 `VM_PROC_NR` 时调用 `set_vm_running(true)`，让 `do_umap_remote`/`acpi`/`oxpcie` 等读取者看到 VM 已激活 |
| VMCTL 子命令分派 | match vs 函数指针数组 | **enum VmCtlParam + match** | 与 syscall.rs 设计一致，编译期穷尽 |
| MEMREQ_GET/REPLY | 直接操作 vmrequest 链表 vs VmRequestQueue | **VmRequestQueue 方法** | 复用 vm.rs 已有类型 |
| do_vmctl 代码归属 | vm.rs vs syscall.rs | **syscall.rs `dispatch_vmctl`** | 当前实现将 SYS_VMCTL 与其他内核调用统一在 `kernel_call_dispatch` 中分派 |
| `assert(RTS_ISSET(...))` 处理 | 保留 assert vs 错误返回 | **`if !is_set { return EINVAL }`**（**Rust 改进**） | C 的 `assert()` 在 debug 构建会 panic、生产构建是 noop；Rust 改为运行时错误返回更健壮，非 translate 设计。适用于 `CLEAR_PAGEFAULT`/`MEMREQ_REPLY`/`VMINHIBIT_CLEAR` 中的前置条件检查 |
| VmCtlResult/VmCtlError 类型化 | 裸 i32 返回 vs enum | **enum** | `VmCtlResult` 类型化 `VMSUSPEND=-996`；`VmCtlError` 精确分类错误（NoRequest/InvalidState/InvalidEndpoint）|

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

> 08 早期版本曾定义 `PageTableSwitcher` trait，意图将 `switch_address_space()` / `__switch_address_space()` 抽象为跨架构接口。但该 trait 没有实现，也未被任何代码使用，属于死代码。按 review 规则移除。`SetAddrSpace` 的数据层实现直接在 `dispatch_vmctl` 中完成（见 §4.3），`write_cr3` 部分待 ptproc 跟踪 + arch trait 非零页表构造器扩展后补齐。

### 4.3 VmCtlResult / VmCtlError 枚举

```rust
// os/kernel/src/vm.rs:700
/// VMCTL 返回值。C: do_vmctl 返回 int（OK/ENOENT/EINVAL/VMSUSPEND=−996/VMPTYPE_CHECK=1）
pub enum VmCtlResult {
    Ok(i32),      // OK=0, ENOENT=2, EINVAL=22, VMPTYPE_CHECK=1
    VmSuspend,    // VMSUSPEND=-996（类型化，非裸负数）
    BadParam,     // arch_do_vmctl default 分支 EINVAL
}

// os/kernel/src/vm.rs:713
/// VMCTL 错误分类（用于内部处理，不直接返回 VM）
pub enum VmCtlError {
    NoRequest,       // ENOENT: vmrequest 链表无匹配
    InvalidState,    // assert 失败 → EINVAL（Rust 改进：assert → 错误返回）
    InvalidEndpoint, // endpoint 不存在 → EINVAL
}
```

**设计要点**：
- `VmCtlResult::VmSuspend` 类型化 C 的 `VMSUSPEND=-996`，避免魔术负数
- `VmCtlError` 精确分类错误来源，便于上层 match 处理
- 错误码与 Minix3 errno 严格对应，不自创

### 4.4 dispatch_vmctl 实现

**位置**: `os/kernel/src/syscall.rs:665`（~200 行完整实现，由 `kernel_call_dispatch` 统一分派）

```rust
// os/kernel/src/syscall.rs:665
/// 处理 SYS_VMCTL 系统调用。
/// C: do_vmctl() — do_vmctl.c:17-173
fn dispatch_vmctl(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    // 1. 权限检查：caller 非 system process → EPERM（Rust 显式化，C 隐式）
    // 2. 解析 SVMCTL_WHO / SVMCTL_PARAM / SVMCTL_VALUE（提前读 m1 字段避免借用冲突）
    // 3. SVMCTL_WHO == SELF → caller endpoint
    // 4. isokendpt + proc_table.get_mut(target_nr)
    // 5. VmCtlParam::try_from(SVMCTL_PARAM) → BadParam 失败
    // 6. match VmCtlParam 分派到 13 个分支：
    //    - ClearPageFault: assert→EINVAL 检查 + RTS_UNSET(PAGEFAULT)
    //    - MemReqGet: VmRequestQueue::next_request → Ok(VMPTYPE_CHECK) / ENOENT
    //    - MemReqReply: assert→EINVAL + 设置 vmresult + MF_KCALL_RESUME + RTS_UNSET(VMREQUEST)
    //    - KernPhysMap / KernMapReply: 返回 ENOSYS（64 位 Direct Map 无需）
    //    - VmInhibitSet: RTS_SET(VMINHIBIT)（SMP IPI 待实现）
    //    - VmInhibitClear: assert→EINVAL + RTS_UNSET(VMINHIBIT)
    //    - ClearMapCache / BootInhibitClear: 直接 RTS_UNSET
    //    - GetPdbr / FlushTlb / InvlPg: 返回 ENOSYS（待 arch trait）
    //    - SetAddrSpace: 见 §4.5
}
```

> **代码归属说明**：本文档早期版本假设 `do_vmctl` 在 `vm.rs` 中实现；实际代码放在 `syscall.rs` 作为 `dispatch_vmctl`，与其他 SYS_* 调用统一在 `kernel_call_dispatch` 中分派。

### 4.5 SetAddrSpace 分支实现

**位置**: `os/kernel/src/syscall.rs:868-911`

对应 C 的 `setcr3()`（arch_do_vmctl.c:19-33）5 步时序：

```rust
VmCtlParam::SetAddrSpace => {
    // SVMCTL_PTROOT = m1_i3, SVMCTL_PTROOT_V = m1_p1
    let ptroot_phys = value_raw as u64;
    let ptroot_virt = unsafe { msg.m_u.m_m1.m1p1 };

    let target = proc_table.get_mut(target_nr);
    match target {
        Some(p) => {
            // Step 1-2 (C): p->p_seg.p_cr3 = cr3; p->p_seg.p_cr3_v = v;
            p.p_seg.phys_root = minix_types::PhysBytes(ptroot_phys);
            p.p_seg.virt_root = if ptroot_virt != 0 {
                Some(minix_types::VirBytes(ptroot_virt))
            } else { None };

            // Step 3 (C): write_cr3 — DEFERRED
            // 需要 ptproc 跟踪 + 非零页表构造器（Paging::new_from_page 会清零，不能用）
            // 单 CPU boot 时调度器在下次切换时通过 arch 上下文恢复路径切换 CR3

            // Step 4 (C): arch_enable_paging — 64 位 noop（分页在 boot 时已启用）

            // Step 5 (C): RTS_UNSET(p, RTS_VMINHIBIT)
            p.p_rts_flags.clear(crate::proc::RtsFlagsBits::VMINHIBIT);

            // C bug correction: vm_running = true when target is VM
            // C 源码从未设 vm_running=1（main.c:47 只设 0），Rust 修正此遗漏
            if p.p_nr == crate::proc::proc_nr::VM_PROC_NR {
                crate::set_vm_running(true);
            }

            VmCtlResult::Ok(0)
        }
        None => return KcallResult::Ok(EINVAL),
    }
}
```

**实现状态**：
| C setcr3 步骤 | Rust 实现 | 状态 |
|--------------|----------|------|
| Step 1-2: 设 p_cr3 + p_cr3_v | `p.p_seg.phys_root` + `virt_root` | ✅ 已实现 |
| Step 3: write_cr3 (ptproc) | — | ⚠️ DEFERRED（待 ptproc + arch trait） |
| Step 4: arch_enable_paging | noop（64 位 boot 时已启用） | ✅ 架构演进 |
| Step 5: RTS_UNSET(VMINHIBIT) | `p_rts_flags.clear(VMINHIBIT)` | ✅ 已实现 |
| C 遗漏: vm_running=1 | `set_vm_running(true)` | ✅ Rust 修正 C bug |

### 4.6 KERN_PHYSMAP / KERN_MAP_REPLY 的 64 位处理

在 64 位 + Direct Map 模型下，内核可以直接访问所有物理内存，因此：
- `VMCTL_KERN_PHYSMAP`: 返回 `ENOSYS`（Direct Map 下无需额外映射，调用方应感知功能未启用）
- `VMCTL_KERN_MAP_REPLY`: 返回 `ENOSYS`（无需记录虚拟地址映射）

协议层保留，注释标注"32 位遗留"。返回 `ENOSYS` 而非 noop，让调用方明确知道功能未启用，避免静默成功导致后续逻辑误判。

---

## 5. 测试

### 5.1 单元测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_vmctl_param_from_u32` | `vm.rs:1295` | 合法/非法 `VmCtlParam` 转换 |
| `test_vmctl_result_variants` | `vm.rs:1311` | `VmCtlResult` 与 C 错误码对应 |
| `test_vm_memreq_get_empty_queue` | `proc_table.rs:1136` | 空队列返回 ENOENT |
| `test_vm_memreq_get_dequeues_pending_request` | `proc_table.rs:1145` | MemReqGet 取出 pending 请求 |
| `test_vm_memreq_reply_completes_request` | `proc_table.rs:1191` | MemReqReply 完成请求并设置结果 |
| `test_vm_memreq_reply_invalid_state` | `proc_table.rs:1229` | 非法状态下回复返回错误 |

### 5.2 集成测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `vm_suspend_state_pending_to_fetched` | `vm.rs:990` | `VmSuspendState` Pending→Fetched |
| `vm_suspend_state_fetched_to_completed` | `vm.rs:999` | `VmSuspendState` Fetched→Completed |
| `vm_suspend_state_pending_is_initial` | `vm.rs:1115` | Pending 为初始状态 |
| `vm_suspend_state_fetched_after_memreq_get` | `vm.rs:1122` | MemReqGet 后转 Fetched |
| `kernel_call_resume_returns_ok_on_success` | `vm.rs:1211` | `kernel_call_resume` 成功路径 |
| `kernel_call_resume_returns_fault_on_failure` | `vm.rs:1224` | `kernel_call_resume` 失败路径 |

---

## 6. 参见

- [08-system-init-boot-finish](08-system-init-boot-finish.md) — bsp_finish_booting 启动调度循环
- [10-switch-to-user](10-switch-to-user.md) — switch_to_user 调度循环入口（调用 switch_address_space 的上层）
- [11-scheduling-primitives](11-scheduling-primitives.md) — VMINHIBIT 对调度的影响
- [13-syscall-dispatch](13-syscall-dispatch.md) — SYS_VMCTL 的分派路径

### 6.1 08 委派职责（待补）

08 文档 §676 委派给 09 的两个 `kinfo` 字段更新当前未在 09 实现：

- `kinfo.mmap_size`: VM direct map 窗口大小（用于 PM/VFS 等查询 VM 地址空间布局）
- `kinfo.mem_high_phys`: 系统最高物理地址（用于内核/VM 内存布局一致性）

**当前状态**: 这两个字段在 08 的 `bsp_finish_booting` 阶段未更新，09 的 `SetAddrSpace` 分支也未覆盖。

**原因**: `mmap_size` + `mem_high_phys` 的更新依赖 VM 启动后向内核回报地址空间布局（通过 `VMCTL_KERN_MAP_REPLY` 或独立机制），而 64 位 Direct Map 模型下 `KERN_PHYSMAP`/`KERN_MAP_REPLY` 返回 ENOSYS（见 §4.6），因此这两个字段的更新路径需要单独设计。

**待后续阶段补齐**: 待 VM direct map 的实际大小 + 最高物理地址在 boot 阶段确定后，在 `dispatch_vmctl` 中补充分支或通过独立 `SYS_GETINFO` 路径更新。本阶段标记为 DEFERRED。
