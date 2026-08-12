# 24-cross-space-runtime Outline v1

> 本文件是 24-cross-space-runtime.md 的结构契约（Gate H.6 依据），基于 C 源码独立推导。
> 非持久化 ground truth；每轮 review 重新评估，保留历史版本不覆盖。

---

## Ch1: 概念（跨地址空间运行时）

### 教学目标
- 从 CPU 视角回答"内核如何代用户态进程完成跨地址空间的数据传输"
- 引入 VMREQUEST 机制作为微内核的核心抽象：内核代行时缺页 → 挂起 → 通知 VM → 恢复重试
- 为后续 Ch3 设计决策提供概念基础（Direct Map vs 临时 PDE / CrossSpaceResult vs C int）

### 知识点覆盖
1. **跨地址空间拷贝的三阶段抽象**（统一抽象先行）
   - 阶段 1：地址解析（VA → PA）
   - 阶段 2：物理内存拷贝
   - 阶段 3：失败处理（缺页 → VMSUSPEND / 错误地址 → EFAULT）
2. **VMREQUEST 机制**（核心概念，从 WHY 出发）
   - 为什么需要：内核代行系统调用时遇到缺页，无法自行处理（内核无缺页 handler）
   - 三步循环：挂起 → 通知 VM → 恢复重试
   - 与 PAGEFAULT 的区别：内核代行 vs 用户态执行
3. **VMSUSPEND 语义**
   - 不是错误码，是"需 VM 协助"信号
   - 触发 `vm_suspend()`：保存请求消息、设置 RTS_VMREQUEST、链入 vmrequest 队列、SIGKMEM 通知 VM
   - VM 通过 VMCTL_MEMREQ_GET/REPLY 完成 handshake
4. **三种挂起类型**（VMSTYPE_KERNELCALL / DELIVERMSG / MAP）
5. **本章不讲什么**：vircopy/safecopy 系统调用分派见 18-syscall-copy.md；VM 启动协议见 09-vm-boot-protocol.md；SIGSEND 使用见 19-syscall-signal.md

### 核心概念清单（按引入顺序）
| 顺序 | 概念 | 依赖 | 教学要点 |
|------|------|------|---------|
| 1 | 跨地址空间拷贝 | 无 | 三阶段抽象：解析→拷贝→失败处理 |
| 2 | VMREQUEST 机制 | 跨地址空间拷贝 | 内核代行缺页 → 挂起 → VM 协助 → 恢复 |
| 3 | VMSUSPEND | VMREQUEST 机制 | "需 VM 协助"信号（非错误码） |
| 4 | vmrequest 链表 | VMREQUEST 机制 | 全局单链表 + SIGKMEM 通知 |
| 5 | 挂起类型 | VMREQUEST 机制 | KERNELCALL / DELIVERMSG / MAP 三类 |
| 6 | Direct Map | 跨地址空间拷贝 | 阶段 1+2 的 Rust 实现机制 |
| 7 | CrossSpaceResult | VMSUSPEND | Rust 类型层区分"完成"vs"挂起" |

### 双向闭环
| 机制 | 触发（进入） | 恢复（返回） |
|------|-------------|-------------|
| VMREQUEST | 内核代行 → 缺页 → `vm_suspend()` | VM 回复 → 清 RTS_VMREQUEST → `kernel_call_resume()` |
| data_copy_vmcheck | 调用方传入 caller + src/dst | 返回 Ok / Fault / VmSuspend |

---

## Ch2: C 源码分析

### 知识点覆盖
1. **文件清单与职责**：
   - `include/minix/syslib.h:128-131` — `sys_datacopy` / `sys_datacopy_try` 宏
   - `include/minix/com.h:223-224, 313` — `SYS_VIRCOPY=15` / `SYS_PHYSCOPY=16` / `CP_FLAG_TRY=0x01`
   - `kernel/vm.h:6-8` — `VMSUSPEND=-996` / `EFAULT_SRC=-995` / `EFAULT_DST=-994`
   - `kernel/system/do_copy.c:1-90` — `do_copy()` 处理 SYS_VIRCOPY/SYS_PHYSCOPY
   - `kernel/arch/i386/memory.c:592-666, 671, 690-705` — `virtual_copy_f()` + `data_copy()` + `data_copy_vmcheck()`（earm 对应在 `arch/earm/memory.c:497+`）
   - `kernel/proto.h:182-187` — `virtual_copy` / `virtual_copy_vmcheck` 宏（展开为 `virtual_copy_f` 调用）
   - `kernel/proc.h:87-124` — `p_vmrequest` 结构 + `VMSTYPE_*` 常量
   - `kernel/proc.h:153-154, 237` — `RTS_VMREQUEST` / `RTS_VMREQTARGET` / `MF_KCALL_RESUME`
   - `kernel/proc.c:234-258` — `vm_suspend()` 实现；`kernel/system.c:612` — `kernel_call_resume()` 实现
2. **核心函数行为契约**（5 个，列在 Ch2 末尾，Gate B 依据）
3. **VMREQUEST 完整生命周期**：触发→挂起→请求→处理→回复→恢复（6 步）
4. **CP_FLAG_TRY 语义**：VFS 专用 try-copy 路径，EFAULT_SRC/DST 直接返回，不挂起

---

## Ch3: Rust 设计决策

### 决策表（≥ 7 个 hypothesis-driven 决策）
| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 地址解析机制 | C 临时 PDE 映射 vs Direct Map | **Direct Map** | 64-bit 地址空间天然支持；消除 `createpde` 副作用 |
| D2 | 返回值类型 | C int (OK/EFAULT/VMSUSPEND 混合) vs enum | **`CrossSpaceResult` enum** | 类型层区分"完成"vs"挂起"，避免 C 的 `if (r == VMSUSPEND)` 误判 |
| D3 | 地址抽象 | C `vir_addr` struct vs Rust enum | **`AddressRef` enum** | `Process{endpoint, offset}` / `Physical(paddr)` 两类，类型层区分 |
| D4 | caller 参数 | 隐藏 vs 显式传入 | **显式 `caller: &mut KProcess`** | VMSUSPEND 需设置 caller 的 RTS_VMREQUEST，无法隐藏 |
| D5 | VmCopyContext | C `p_vmrequest` 内嵌 union vs Rust 独立 struct | **`VmCopyContext` 独立 struct** | Rust 所有权清晰；`fault_type` 字段区分 Src/Dst |
| D6 | vmrequest 链表 | C 全局指针 + `nextrequestor` vs Rust `VmRequestQueue` | **`VmRequestQueue`** | 封装链表操作 + 显式 insert/remove API；BKL 保护注释 |
| D7 | dispatch_datacopy | 保留死代码 vs 删除 | **删除** | Minix3 无 SYS_DATACOPY 调用号；已被 dispatch_vircopy 替代 |
| D8 | CopyResult 重复 | 保留两个 enum vs 统一 | **统一到 CrossSpaceResult** | DRY；删除 cross_space::CopyResult |
| D9 | PTE walk 架构 | `#[cfg(target_arch)]` vs trait 静态分派 | **trait `PagingArch` 静态分派** | 消除条件编译；x86_64/aarch64/riscv64 各自实现 |

---

## Ch4: 实现要点

### 知识点覆盖
1. **跨地址空间拷贝原语**：`cross_space_copy` / `cross_space_memset`
2. **Direct Map 机制**：`DirectMapArch::kernel_phys_to_virt` + PTE walk
3. **`data_copy_vmcheck` 实现**：caller + src/dst + VMSUSPEND 设置
4. **`VmCopyContext` + `VmRequestQueue`**：挂起状态管理
5. **VMCTL_MEMREQ_GET/REPLY 处理**：VM handshake
6. **`kernel_call_resume`**：恢复路径
7. **未实现部分**（诚实标注 DEFERRED）：aarch64/riscv64 PTE walk stub

---

## Ch5: 测试要点

### 知识点覆盖
1. L1 对偶测试：`test_data_copy_vmcheck_parity_with_c`（与 C 行为对照）
2. L2 契约测试：`test_cross_space_copy_contract`（trait 契约）
3. 边界测试：零字节 / 地址溢出 / SELF→caller 替换
4. 状态测试：VmSuspend 后 caller RTS_VMREQUEST 已设置
5. 恢复测试：VmRequestQueue insert/remove 顺序

---

## Ch6: 参见

- [18-syscall-copy.md](18-syscall-copy.md) — vircopy/safecopy 系统调用分派
- [19-syscall-signal.md](19-syscall-signal.md) — SIGSEND 使用 data_copy_vmcheck
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) — VM 启动协议（VMCTL_MEMREQ 接口）
- [02-page-table-kernel.md](02-page-table-kernel.md)（其他 stage） — Direct Map + PageTableRef 设计

---

## 知识点覆盖矩阵

| C 源码元素 | Ch1 概念 | Ch2 分析 | Ch3 设计 | Ch4 实现 | Ch5 测试 |
|-----------|---------|---------|---------|---------|---------|
| sys_datacopy 宏 | ✅ | ✅ | — | — | — |
| do_copy() | — | ✅ | — | — | — |
| data_copy_vmcheck() | ✅ | ✅ | ✅ D4 | ✅ | ✅ |
| virtual_copy_vmcheck() | — | ✅ | ✅ D1 | ✅ | ✅ |
| VMSUSPEND | ✅ | ✅ | ✅ D2 | ✅ | ✅ |
| p_vmrequest | ✅ | ✅ | ✅ D5 | ✅ | ✅ |
| vmrequest 链表 | ✅ | ✅ | ✅ D6 | ✅ | ✅ |
| VMCTL_MEMREQ_GET/REPLY | ✅ | ✅ | — | ✅ | — |
| kernel_call_resume() | ✅ | ✅ | — | ✅ | ✅ |
| Direct Map (Rust) | ✅ | — | ✅ D1 | ✅ | ✅ |
| cross_space_copy (Rust) | — | — | ✅ D2,D3 | ✅ | ✅ |
