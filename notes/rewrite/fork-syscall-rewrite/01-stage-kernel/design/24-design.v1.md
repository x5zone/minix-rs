# 24-cross-space-runtime Design v1

> 本文件是 24-cross-space-runtime.md 关联 Rust 代码的设计契约（Gate H.1-H.5 依据）。
> 基于 C 源码 + Rust 当前代码状态独立推导。

---

## Ch1: 设计决策

### D1. 地址解析机制：Direct Map 替代 C 临时 PDE 映射

**C 方案**：`virtual_copy_f()` 调用 `createpde()` 临时映射源/目标页表项到内核地址空间，执行 `lin_lin_copy()`，然后解除临时映射。涉及 `freepdes[]` 全局数组 + `MEMORY_MUTEX` 互斥保护。

**Rust 方案**：利用 64-bit 地址空间的 Direct Map（`KERNEL_DIRECT_MAP_BASE + PA → KV`）直接访问任意物理页。PTE walk 通过 Direct Map 读取页表项，无需临时映射。

**理由**：
1. 64-bit 地址空间天然支持大段 Direct Map，无需 PDE 复用
2. 消除 `freepdes[]` 全局状态 + `MEMORY_MUTEX` 互斥（SMP 简化）
3. 消除 `createpde` 的副作用（修改页表 → TLB flush）
4. redox 对照：redox 也使用类似 Direct Map（`PHYS_OFFSET`）访问物理内存

### D2. 返回值类型：CrossSpaceResult enum 替代 C int

**C 方案**：`virtual_copy_vmcheck()` 返回 `int`，可能是 `OK(0)` / `EFAULT(14)` / `EFAULT_SRC(-995)` / `EFAULT_DST(-994)` / `VMSUSPEND(-996)`。调用方用 `if (r == VMSUSPEND)` 区分"挂起"vs"错误"。

**Rust 方案**：
```rust
pub enum CrossSpaceResult {
    Completed(Result<(), VmCopyError>),
    Suspended(VmFaultType),
}
```

**理由**：
1. 类型层区分"完成"vs"挂起"——`Completed` 包含 `Result`，`Suspended` 包含 `VmFaultType`
2. 避免 C 的 `if (r == VMSUSPEND)` 误判（如 `r == EFAULT_SRC` 与 `r == VMSUSPEND` 都是负数）
3. `VmFaultType::Src/Dst` 替代 C 的 `EFAULT_SRC/EFAULT_DST`，支持 `match` 穷尽检查

### D3. 地址抽象：AddressRef enum

**C 方案**：`struct vir_addr { endpoint_t proc_nr_e; vir_bytes offset; }`——通过 `proc_nr_e == NONE` / `SELF` / 实际 endpoint 区分地址类型，物理地址用 `struct phys_addr { phys_bytes phys; }`。

**Rust 方案**：
```rust
pub enum AddressRef {
    Process { endpoint: Endpoint, offset: VirBytes },
    Physical(PhysBytes),
}
```

**理由**：
1. enum 天然区分 Process vs Physical，无需 sentinel 值
2. `endpoint: Endpoint` newtype 防止 int 误用
3. SELF 替换在 `data_copy_vmcheck` 入口统一处理，AddressRef 不含 SELF

### D4. caller 参数：显式 `&mut KProcess`

**C 方案**：`data_copy_vmcheck(struct proc * caller, ...)` —— caller 用于设置 `RTS_VMSUSPEND`。

**Rust 方案**：`data_copy_vmcheck(caller: &mut KProcess, src: &KProcess, src_addr: VirBytes, dst: &KProcess, dst_addr: VirBytes, bytes: usize) -> CopyResult`

**理由**：
1. VMSUSPEND 语义要求设置 caller 的 `RTS_VMREQUEST` + `p_vmrequest` 字段——必须 `&mut`
2. C 用裸指针，Rust 用 `&mut` 显式表达可变借用
3. src/dst 用 `&KProcess` 不可变借用（只需读 `p_seg.phys_root`）

### D5. VmCopyContext 独立 struct

**C 方案**：`p_vmrequest` 内嵌 union，含 `saved.reqmsg` / `params.check.{start,length,writeflag}` / `vmresult`。

**Rust 方案**：
```rust
pub struct VmCopyContext {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: VmFaultType,
}
```

**理由**：
1. Rust 所有权清晰，VmCopyContext 可独立传递/存储
2. `fault_type` 字段替代 C 的 `EFAULT_SRC/EFAULT_DST` 隐式区分
3. 未来可扩展为 `VmSuspendContext`（含 KERNELCALL/DELIVERMSG/MAP 三类）

### D6. VmRequestQueue 封装链表

**C 方案**：全局 `struct proc * vmrequest` 指针 + `p_vmrequest.nextrequestor` 字段链接。无封装。

**Rust 方案**：
```rust
pub struct VmRequestQueue {
    head: Option<Endpoint>,  // 链表头
    // BKL 保护：所有操作需 caller 持有 BKL
}
```

**理由**：
1. 封装 insert/remove API，避免裸指针操作
2. `Option<Endpoint>` 替代裸指针，避免 unsafe
3. BKL 保护注释：所有方法标注 `// SAFETY: Caller must hold BKL`

### D7. 删除 dispatch_datacopy 死代码

**C 方案**：无 `SYS_DATACOPY` 调用号；`sys_datacopy` 是用户空间宏，展开为 `sys_vircopy(..., 0)`，内核侧由 `do_copy()` 处理 `SYS_VIRCOPY`。

**Rust 当前**：`cross_space::dispatch_datacopy` 是死代码，含 placeholder 逻辑（`bytes = m1.m1p1` 错误）。

**决策**：**删除** `dispatch_datacopy`。`SYS_VIRCOPY` 已由 `syscall_copy.rs::dispatch_vircopy` 处理，无需重复。

**理由**：
1. Minix3 无 `SYS_DATACOPY` 调用号——保留 dispatch 函数违反 C 对齐
2. placeholder 逻辑（`bytes = m1.m1p1`）是 bug，可能误导后续维护者
3. 死代码 + 错误 placeholder = 设计债务，删除是最彻底修复

### D8. 统一 CopyResult 到 CrossSpaceResult

**Rust 当前**：`cross_space::CopyResult`（Ok/Fault/VmSuspend）vs `vm::CrossSpaceResult`（Completed(Ok/Err)/Suspended）。两者语义重叠。

**决策**：**删除** `cross_space::CopyResult`，统一使用 `vm::CrossSpaceResult`。`data_copy_vmcheck` 返回 `CrossSpaceResult`，调用方通过 `match` 处理三种情况。

**理由**：
1. DRY：单一类型表达"跨地址空间拷贝结果"
2. `CrossSpaceResult::Suspended(VmFaultType)` 比 `CopyResult::VmSuspend` 信息更丰富（含 Src/Dst）
3. `data_copy_vmcheck` 内部已委托给 `cross_space_copy` 返回 `CrossSpaceResult`——统一类型消除无意义的转换

### D9. PTE walk trait 静态分派

**C 方案**：`#if defined(__i386__)` / `#elif defined(__arm__)` 条件编译选择架构特定代码。

**Rust 方案**：trait `PagingArch` 静态分派，x86_64/aarch64/riscv64 各自实现。

**理由**：
1. 消除 `#[cfg(target_arch)]` 分散条件编译（Pattern #14）
2. trait bound 让上层代码泛型化
3. 编译期单态化，零运行时开销

---

## Ch2: Minix3 对齐矩阵

| Minix3 概念 | design 中对应 | code 中对应 | 缺失位置 |
|------------|--------------|------------|---------|
| `sys_datacopy` 宏 | —（用户空间宏，非内核） | — | 无需 |
| `do_copy()` | —（18-syscall-copy.md 覆盖） | `syscall_copy.rs::dispatch_vircopy` | 无需 |
| `data_copy_vmcheck()` | D4 | `cross_space.rs::data_copy_vmcheck` | ✅ |
| `virtual_copy_vmcheck()` | D1+D2 | `syscall_copy.rs::virtual_copy_vmcheck` + `vm.rs::cross_space_copy` | ✅ |
| `VMSUSPEND` | D2 `Suspended` variant | `CrossSpaceResult::Suspended` | ✅ |
| `p_vmrequest` | D5 `VmCopyContext` | `vm.rs::VmCopyContext` | ✅ |
| `vmrequest` 链表 | D6 `VmRequestQueue` | （DEFERRED 实现） | ⚠️ 待实现 |
| `vm_suspend()` | D4+D5 | （DEFERRED 实现） | ⚠️ 待实现 |
| `kernel_call_resume()` | D4 恢复路径 | （DEFERRED 实现） | ⚠️ 待实现 |

---

## Ch3: Rust 类型清单

| 类型 | 位置 | 职责 |
|------|------|------|
| `AddressRef` | `vm.rs` | 地址抽象（Process/Physical） |
| `CrossSpaceResult` | `vm.rs` | 拷贝结果（Completed/Suspended） |
| `VmCopyError` | `vm.rs` | 错误类型（SrcFault/DstFault/InvalidAddr/...） |
| `VmFaultType` | `vm.rs` | 缺页方向（Src/Dst） |
| `VmCopyContext` | `vm.rs` | 挂起上下文（src/dst/bytes/fault_type） |
| `PageTableRef` | `vm.rs` | 页表根引用（cr3） |
| `DirectMapArch` trait | `minix_arch::direct_map` | Direct Map 抽象（PA↔KV） |
| `cross_space_copy` | `vm.rs` | 拷贝原语（泛型 + D: DirectMapArch） |
| `cross_space_memset` | `vm.rs` | 填充原语 |
| `data_copy_vmcheck` | `cross_space.rs` | 内核内部入口（含 caller） |
| `VmRequestQueue` | （待实现） | 挂起队列 |

---

## Ch4: 限制与已知缺口

| 缺口 | 严重度 | 理由 | 计划 |
|------|--------|------|------|
| `VmRequestQueue` 实现 | P1 DEFERRED | 当前无 SIGKMEM 触发路径 | 随 SIGSEND 实现落地 |
| `vm_suspend()` 实现 | P1 DEFERRED | 依赖 VmRequestQueue | 同上 |
| `kernel_call_resume()` 实现 | P1 DEFERRED | 依赖 vm_suspend | 同上 |
| aarch64/riscv64 PTE walk | P2 DEFERRED | x86_64 优先 | 随架构实现推进 |
| `VmSuspendContext`（KERNELCALL/DELIVERMSG/MAP 三类） | P2 DEFERRED | 当前仅 copy 场景 | 随 IPC/message deliver 推进 |

---

## 附录 A: 与 redox 对照

| 维度 | Minix3 | redox | minix-rs 选择 |
|------|--------|-------|--------------|
| 跨地址空间拷贝 | VMREQUEST + 临时 PDE | scheme-based + 临时映射 | Direct Map + CrossSpaceResult |
| 缺页处理 | 内核挂起 + VM 协助 | 用户态 scheme 直接处理 | 沿用 Minix3 VMREQUEST（微内核架构对齐） |
| 返回值 | int (OK/EFAULT/VMSUSPEND) | Result + 特定 Error | `CrossSpaceResult` enum（类型层区分） |
| 地址抽象 | vir_addr struct | scheme token + offset | `AddressRef` enum（Process/Physical） |
| 链表 | 裸指针 + nextrequestor | 无全局链表（per-scheme） | `VmRequestQueue` 封装 |

**关键差异**：redox 的 scheme 模型把"地址空间"抽象为 scheme token，每个 scheme 自管理内存映射；Minix3 的 VMREQUEST 是"内核代行时缺页"的恢复机制。两者解决不同问题——redox 是"谁拥有内存"，Minix3 是"内核代行时如何恢复"。minix-rs 沿用 Minix3 VMREQUEST 因为微内核架构要求内核最小化，不能自行处理缺页。
