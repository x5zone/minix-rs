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
- **Rust 实现（P9-4）**：Step 3 的 `write_cr3` 由 `TlbArch::set_active_root`（[os/arch/src/arch/tlb_arch.rs:135](file:///home/xzhao/github/minix-rs/os/arch/src/arch/tlb_arch.rs)）完成；ptproc 跟踪用内核全局 `CURRENT_PTPROC_NR: AtomicI32`（[os/kernel/src/lib.rs:1976](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）+ `current_ptproc_nr()` 访问器（[os/kernel/src/lib.rs:1994](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)），以 proc-nr 比较替代 C 的指针同一性比较（详见 §4.8）

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
| switch_address_space / 页表切换 | trait 抽象 vs 推迟实现 | **不抽象独立 trait，复用 `TlbArch::set_active_root`** | 08 早期版本曾定义 `PageTableSwitcher`，但属死代码已移除；`SetAddrSpace` 数据层在 `dispatch_vmctl` 实现，Step 3 的 `write_cr3` 经 `TlbArch::set_active_root` 完成（P9-4，见 §4.8），无需"非零页表构造器"——该 trait 方法直接写 CR3/TTBR0/satp 不触碰页表内容 |
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

> 08 早期版本曾定义 `PageTableSwitcher` trait，意图将 `switch_address_space()` / `__switch_address_space()` 抽象为跨架构接口。但该 trait 没有实现，也未被任何代码使用，属于死代码。按 review 规则移除。`SetAddrSpace` 的数据层实现直接在 `dispatch_vmctl` 中完成（见 §4.5）；Step 3 的 `write_cr3` 经 `TlbArch::set_active_root` 完成（P9-4，见 §4.7/§4.8），不再需要独立的页表切换 trait。

> **为何不需要"非零页表构造器"**：原延期理由曾担忧 `Paging::new_from_page` 会清零页表内容，无法用于构造活跃页表。`set_active_root` 的设计绕开了这一顾虑——它是 `TlbArch` trait 上的低层关联函数，只负责写 CR3/TTBR0/satp 根寄存器（并按架构需要 flush TLB），完全不触碰页表 PTE 内容。页表构造仍是 VM 的职责（VM 在用户态建好页表后把物理根传给内核），内核只负责"安装根 + flush"。

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
    //    - GetPdbr / FlushTlb / InvlPg: ✅ 已实现（FIX-24, Phase 5）经 TlbArch trait 三架构
    //    - SetAddrSpace: 见 §4.5
}
```

> **代码归属说明**：本文档早期版本假设 `do_vmctl` 在 `vm.rs` 中实现；实际代码放在 `syscall.rs` 作为 `dispatch_vmctl`，与其他 SYS_* 调用统一在 `kernel_call_dispatch` 中分派。

### 4.5 SetAddrSpace 分支实现

**位置**: `os/kernel/src/syscall.rs:1967`

对应 C 的 `setcr3()`（arch_do_vmctl.c:19-33）5 步时序：

```rust
// os/kernel/src/syscall.rs:1967
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

            // Step 3 (C): if (p == ptproc) write_cr3(p->p_seg.p_cr3);
            // P9-4: 经 TlbArch::set_active_root 完成，proc-nr 比较替代指针同一性
            // （proc-nrs 唯一标识进程表槽位，一一对应无别名，详见 §4.8）
            if crate::current_ptproc_nr() == Some(p.p_nr) {
                unsafe {
                    minix_arch::CurrentTlbArch::set_active_root(
                        minix_types::PhysBytes(ptroot_phys),
                    );
                }
            }

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
| Step 3: write_cr3 (ptproc) | `TlbArch::set_active_root` + `current_ptproc_nr()` 比较 | ✅ 已实现（P9-4，见 §4.8） |
| Step 4: arch_enable_paging | noop（64 位 boot 时已启用） | ✅ 架构演进 |
| Step 5: RTS_UNSET(VMINHIBIT) | `p_rts_flags.clear(VMINHIBIT)` | ✅ 已实现 |
| C 遗漏: vm_running=1 | `set_vm_running(true)` | ✅ Rust 修正 C bug |

### 4.6 KERN_PHYSMAP / KERN_MAP_REPLY 的 64 位处理

在 64 位 + Direct Map 模型下，内核可以直接访问所有物理内存，因此：
- `VMCTL_KERN_PHYSMAP`: 返回 `ENOSYS`（Direct Map 下无需额外映射，调用方应感知功能未启用）
- `VMCTL_KERN_MAP_REPLY`: 返回 `ENOSYS`（无需记录虚拟地址映射）

协议层保留，注释标注"32 位遗留"。返回 `ENOSYS` 而非 noop，让调用方明确知道功能未启用，避免静默成功导致后续逻辑误判。

### 4.7 GetPdbr / FlushTlb / InvlPg / ClearMapCache 实现（FIX-24, Phase 5）

C 由 `arch_do_vmctl()` (arch_do_vmctl.c:38-65) 处理的 3 个 arch-specific 子命令 + 1 个 32-bit-only 子命令，现已在 `dispatch_vmctl` 中实现（[os/kernel/src/syscall.rs:1200-1481](file:///home/xzhao/github/minix-rs/os/kernel/src/syscall.rs)）。

**TlbArch trait 抽象**（[os/arch/src/arch/tlb_arch.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arch/tlb_arch.rs)）：

```rust
pub trait TlbArch {
    /// Flush all non-global TLB entries on the current CPU.
    /// C: write_cr3(read_cr3()) (x86) / tlbi alle1is (aarch64) / sfence.vma zero, zero (riscv64)
    unsafe fn flush_all();

    /// Flush the TLB entry for a single virtual address on the current CPU.
    /// C: invlpg(addr) (x86) / tlbi vaae1is, <va> (aarch64) / sfence.vma <va>, zero (riscv64)
    unsafe fn flush_addr(vaddr: VirBytes);

    /// Install a new page-table root on the current CPU (P9-4).
    /// C: write_cr3(p->p_seg.p_cr3) inside setcr3() — arch_do_vmctl.c:31
    /// 与 flush_all（重载当前 root）不同：本方法装入一个新 root，切换地址空间。
    unsafe fn set_active_root(phys_root: PhysBytes);
}
```

**三架构实现**：

| 架构 | 文件 | flush_all | flush_addr | set_active_root (P9-4) |
|------|------|-----------|------------|------------------------|
| x86_64 | `os/arch/src/x86_64/tlb.rs` | `mov cr3, {cr3}` (CR3 重载) | `invlpg [addr]` | `mov cr3, {phys_root}`（CR3 写隐式 flush 非全局 TLB） |
| aarch64 | `os/arch/src/arm64/tlb.rs` | `tlbi alle1is` + `isb` | `tlbi vaae1is, {va>>12}` + `isb` | `msr TTBR0_EL1, {root}` + `tlbi alle1is` + `isb`（TTBR0 写不刷 TLB，需显式 `tlbi`） |
| riscv64 | `os/arch/src/riscv64/tlb.rs` | `sfence.vma zero, zero` | `sfence.vma {va}, zero` | `csrw satp, (SV39<<60)\|(root>>12)` + `sfence.vma zero, zero`（satp 写不刷 TLB） |
| Mock | `os/arch/src/arch/tlb_arch.rs` | no-op | no-op | no-op |

**dispatch_vmctl 子命令实现**：

| 子命令 | C 位置 | Rust 实现 | 状态 |
|--------|--------|----------|------|
| `GetPdbr` | arch_do_vmctl.c:38-40 | 读 `p.p_seg.phys_root.0 as i32` | ✅ 已实现 |
| `FlushTlb` | arch_do_vmctl.c:42-44 | `unsafe { CurrentTlbArch::flush_all(); }` | ✅ 已实现 |
| `InvlPg` | arch_do_vmctl.c:52-54 | `unsafe { CurrentTlbArch::flush_addr(VirBytes(value_raw as u64)); }` | ✅ 已实现 |
| `ClearMapCache` | do_vmctl.c:161-164 | `VmCtlResult::Ok(0)` — **WONTFIX**（64-bit Direct Map 无 cache table） | ✅ 已实现（no-op） |

**设计决策**：
- **为何 `TlbArch` 是关联函数而非实例方法**: TLB flush 操作当前 CPU 的 TLB，是全局资源不绑定具体 `Paging` 实例。C 的 `write_cr3`/`invlpg` 也是 free function
- **为何 `unsafe`**: 需要 paging 已启用 + SMP 下 cross-CPU shootdown 由 caller 负责
- **为何 `ClearMapCache` 标 WONTFIX 而非 DEFERRED**: `mem_clear_mapcache()` 是 32-bit Direct Map cache table 的清理，64-bit 用静态偏移 Direct Map 覆盖全部物理内存，无 cache table 可清。永久不实现，返回 Ok(0) 匹配 C 在无 cache table 架构上的 no-op 行为
- **为何 `set_active_root` 与 `flush_all` 分离**: `flush_all` 重载当前 root（仅刷 TLB，不切地址空间）；`set_active_root` 装入新 root（切地址空间 + 按架构刷 TLB）。两者语义不同，C 也分开（`write_cr3(read_cr3())` vs `write_cr3(new_cr3)`）

### 4.8 ptproc 跟踪与 set_active_root（P9-4）

C 的 `setcr3()` 用指针同一性 `if (p == get_cpulocal_var(ptproc))` 判断目标进程是否是当前页表进程（arch_do_vmctl.c:31）。Rust 移植需解决两个问题：如何跟踪 ptproc、如何写硬件 root 寄存器。P9-4 同时落地两者。

#### 4.8.1 CURRENT_PTPROC_NR 内核全局

**位置**: [os/kernel/src/lib.rs:1976](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)

```rust
// os/kernel/src/lib.rs:1976
static CURRENT_PTPROC_NR: AtomicI32 = AtomicI32::new(i32::MIN);

/// Sentinel value indicating CURRENT_PTPROC_NR has not been initialized.
/// Distinct from any valid proc-nr (user procs ≥ 0, kernel tasks in -NR_TASKS..=-1).
const PTPROC_UNSET: i32 = i32::MIN;  // lib.rs:1981

/// Read the proc-nr of the current ptproc. Returns None if not yet set.
pub fn current_ptproc_nr() -> Option<crate::proc::ProcNr> {  // lib.rs:1994
    let v = CURRENT_PTPROC_NR.load(Ordering::Acquire);
    if v == PTPROC_UNSET { None } else { Some(crate::proc::ProcNr(v)) }
}

/// Set the current ptproc proc-nr. Called once during init_post_and_memory.
pub fn set_current_ptproc_nr(nr: crate::proc::ProcNr) {  // lib.rs:2015
    CURRENT_PTPROC_NR.store(nr.0, Ordering::Release);
}
```

**初始化**：`init_post_and_memory` 在调用 arch 层 `CurrentPostInitArch::set_ptproc` 之后，调用 `set_current_ptproc_nr(VM_PROC_NR)`（[os/kernel/src/lib.rs:1005](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。这镜像 C 的 `get_cpulocal_var(ptproc) = vm`（`arch_post_init()`，protect.c:372）。

**为何用 proc-nr 比较而非指针同一性**：C 比较 `struct proc *` 指针，Rust 用 `ProcNr`（i32 进程表索引）。两者等价——proc-nrs 唯一标识 `ProcessTable` 中的进程槽位，一一对应无别名（同一 proc-nr 永远映射到同一 `KProcess`）。proc-nr 比较还避免了裸指针的不安全性，与 [16-smp.md §D8](16-smp.md) 的 per-CPU 索引设计一致（`proc_ptr`/`fpu_owner` 均用 `Option<ProcNr>`）。

**Sentinel 设计**：`PTPROC_UNSET = i32::MIN` 表示"尚未初始化"，与所有合法 proc-nr 不冲突（用户进程 ≥ 0，内核任务在 `-NR_TASKS..=-1` 小负数区间）。`current_ptproc_nr()` 返回 `Option<ProcNr>`——`None` 表示 boot 早期尚未设过。

#### 4.8.2 TlbArch::set_active_root 三架构汇编

`set_active_root` 的完整定义见 [os/arch/src/arch/tlb_arch.rs:135](file:///home/xzhao/github/minix-rs/os/arch/src/arch/tlb_arch.rs)。三架构实现在 §4.7 表中列出，关键差异：

- **x86-64**（[os/arch/src/x86_64/tlb.rs:51](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/tlb.rs)）：`mov cr3, {phys_root}`。写 CR3 隐式 flush 所有非全局 TLB 条目（Intel SDM Vol 3 §4.10.4.1），无需额外 invalidate 指令。
- **aarch64**（[os/arch/src/arm64/tlb.rs:61](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/tlb.rs)）：`msr TTBR0_EL1, {root}` + `tlbi alle1is` + `isb`。ARM64 写 TTBR0 **不**隐式刷 TLB（ARM ARM D5.4.5），必须显式 `tlbi alle1is` 清除旧 root 的过期翻译，`isb` 同步上下文。
- **riscv64**（[os/arch/src/riscv64/tlb.rs:70](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/tlb.rs)）：`csrw satp, (SV39_MODE << 60) | (phys_root >> 12)` + `sfence.vma zero, zero`。RISC-V 写 satp **不**隐式刷 TLB（Priv ISA §4.2.1），需 `sfence.vma`。satp 编码为 `[MODE(1)=8] [ASID(16)=0] [PPN(44)]`，故 `phys_root >> 12` 丢弃页内偏移。

#### 4.8.3 并发模型

**BKL 保护**：所有写者（`init_post_and_memory`、`dispatch_vmctl(SetAddrSpace)` 当 target 是 ptproc 时）持有 BKL；读者（`dispatch_vmctl(SetAddrSpace)` 的比较）也持有 BKL——系统调用总是在到达分派器前获取 BKL。

**单写者原则**：`ptproc` 仅在 boot 期间设一次（`init_post_and_memory` 设为 `VM_PROC_NR`），正常运行不修改（对齐 C 行为——`arch_post_init()` 是 C 中唯一写者）。

**内存序**：`set_current_ptproc_nr` 用 `Release`，`current_ptproc_nr` 用 `Acquire`——保证 boot 期 ptproc 安装完成后其他 CPU 能观察到该值。即便无 BKL 时读到陈旧值也是安全的：最坏后果是跳过一次 CR3 reload，下次上下文切换会修正。

### 4.9 VM ELF 加载 at boot（P9-5 / FIX-24）

C 的 `arch_boot_proc()` 在 boot 期间把 VM ELF 段映射进 bootstrap 页表（protect.c:388 x86 / protect.c:115 ARM）。Rust 移植早期将非 mock 路径标为 `EntrySpec::DEFERRED`（PC=0），靠 RS 在运行时加载 VM ELF——这是不正确的，因为 VM 是 ptproc，必须在 boot 后立即可运行以服务其他 boot 进程的 VMCTL/PRIVCTL 系统调用。P9-5 落地真实 boot 期 VM ELF 加载。

#### 4.9.1 CURRENT_ROOT_PHYS 内核全局

**位置**: [os/kernel/src/lib.rs:2043](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)

```rust
// os/kernel/src/lib.rs:2043
static CURRENT_ROOT_PHYS: AtomicU64 = AtomicU64::new(ROOT_PHYS_UNSET);

/// Sentinel: u64::MAX. Distinct from any 4KB-aligned physical address.
const ROOT_PHYS_UNSET: u64 = u64::MAX;  // lib.rs:2047

/// Read the bootstrap page-table root physical address.
/// Returns None before arch_boot_impl has run.
pub fn current_root_phys() -> Option<minix_types::PhysBytes> {  // lib.rs:2061
    let v = CURRENT_ROOT_PHYS.load(Ordering::Acquire);
    if v == ROOT_PHYS_UNSET { None } else { Some(minix_types::PhysBytes(v)) }
}

/// Record the bootstrap root. Called from arch_boot_impl after enable().
pub fn set_current_root_phys(phys: minix_types::PhysBytes) {  // lib.rs:2080
    CURRENT_ROOT_PHYS.store(phys.0, Ordering::Release);
}
```

**初始化**：`arch_boot_impl` 在 `Paging::enable()` 成功后立即调 `set_current_root_phys(root_page)`（[os/kernel/src/lib.rs:265](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。注意传的是 `root_page` 参数（raw physical address）而非 `enable()` 的返回值——某些架构的 `enable()` 返回 satp 编码值（riscv64），不是 raw physical address。

**Sentinel 设计**：`ROOT_PHYS_UNSET = u64::MAX` 不是 4KB 对齐（低 12 位非零），永远不可能与真实页表根物理地址冲突。

**与 CURRENT_PTPROC_NR 的关系**：两者都跟踪 boot 期安装的"当前页表"状态，但语义层次不同：
- `CURRENT_ROOT_PHYS`：硬件层 — 当前装入 CR3/TTBR0/satp 的物理根地址。在 boot 期就是 bootstrap 根。
- `CURRENT_PTPROC_NR`：OS 层 — 当前作为 ptproc 的进程编号。`init_post_and_memory` 把它设为 `VM_PROC_NR`，因为 VM 接管了 bootstrap 根作为自己的初始根。

SMP 迁移时两者都需要变为 per-CPU `CpuLocal` 字段（详见 [16-smp.md §D8](16-smp.md)）。

#### 4.9.2 Paging::from_active_root trait 方法

**位置**: [os/arch/src/arch/paging.rs:202](file:///home/xzhao/github/minix-rs/os/arch/src/arch/paging.rs)

```rust
/// Wrap an already-active page table root without modifying it.
fn from_active_root(root_phys: PhysBytes) -> Self;
```

与 `new_from_page`（zero-fill 根页）不同，`from_active_root` 假设根页表已初始化并装入 MMU，仅创建 `Paging` handle 用于 `map`/`remap`/`query`。三架构实现都是简单的 `Self { root_paddr: root_phys.0 }`（[x86_64/paging.rs:370](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/paging.rs) / [arm64/paging.rs:413](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/paging.rs) / [riscv64/paging.rs:424](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/paging.rs)）。

#### 4.9.3 init_proc_and_boot 非 mock 路径

**位置**: [os/kernel/src/lib.rs:907-958](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)

```rust
// FIX-24 (Phase 9): Real VM ELF loading at boot.
#[cfg(not(feature = "mock"))]
{
    use minix_arch::paging::Paging as _;
    use minix_arch::CurrentPaging;

    let root_phys = current_root_phys()
        .expect("init_proc_and_boot: bootstrap root not set");
    let mut paging = CurrentPaging::from_active_root(root_phys);
    let vm_result = load_vm_elf(module, kernel_info, &mut paging)
        .expect("load_vm_elf: VM ELF is required at boot");

    // Reclaim VM module physical memory (undoes Phase A.2 cut_memmap).
    // C: protect.c:450-451 — mod->mod_start = mod_end = 0.
    unsafe {
        let mmap = &mut *FREE_MEMMAP.get();
        let _ = memmap::add_memmap(mmap, module.start.0, module.len as u64);
    }

    // Record VM's page-table root in p_seg so init_post_and_memory can
    // install VM as ptproc (set_ptproc + set_current_ptproc_nr).
    proc.p_seg.phys_root = root_phys;
    proc.p_seg.virt_root = Some(VirBytes(root_phys.0)); // VA=PA in bootstrap

    EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings)
}
```

**关键设计**：
1. **复用 bootstrap 页表**：不在 boot 期为 VM 单独构造页表，而是把 ELF 段映射进 `arch_boot_impl` 创建并已激活的 bootstrap 页表。VM 接管这个根作为自己的初始根。VMCTL SetAddrSpace 后续会替换为 VM 自建的页表（经 `TlbArch::set_active_root` 写硬件）。
2. **identity mapping**：`load_vm_elf` 用 VA=PA 1:1 映射段（[arch/boot.rs:287-323](file:///home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs)），与 bootstrap 页表的低地址 identity mapping 一致。
3. **p_seg 同步**：VM 的 `p_seg.phys_root`/`virt_root` 记录为 bootstrap 根，使 `init_post_and_memory` 能从 `proc_table.get(VM_PROC_NR).p_seg` 读出根地址并交给 `CurrentPostInitArch::set_ptproc`。
4. **module 内存回收**：ELF 段复制进页表后立即 `add_memmap` 回收 module 物理内存，与 mock 路径和 C 行为一致（`protect.c:450-451`）。

#### 4.9.4 测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_root_phys_unset_returns_none_before_boot` | `lib.rs:2953` | boot 前 `current_root_phys()` 返回 None |
| `test_root_phys_set_returns_recorded_value` | `lib.rs:2963` | `set_current_root_phys` 后能读回 |
| `test_root_phys_set_is_idempotent` | `lib.rs:2974` | 重复 set 不破坏状态 |
| `test_root_phys_sentinel_distinct_from_valid_addresses` | `lib.rs:2986` | `ROOT_PHYS_UNSET` 与合法地址不冲突 |
| `test_from_active_root_round_trip_root_paddr` | `lib.rs:3003` | `from_active_root` 后 `root_paddr()` 一致 |
| `test_from_active_root_does_not_allocate_via_new_mock_path` | `lib.rs:3021` | `from_active_root` 不走 `new_mock` 分配路径 |

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
