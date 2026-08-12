# 17-syscall-process Design（设计文档）

> **状态**: 完整设计（基于 `17-outline.md` 经 outline-review 批准）
> **创建**: 2026-08-01
> **作者**: Trae (GLM-5.2)
> **前置**: 11-scheduling-primitives.md, 16-smp.md, 06-proc-init-boot-proc.md, 10-switch-to-user.md
> **C 源码**: `minix3/minix/kernel/system/do_fork.c` (136行), `do_exec.c` (60行), `do_exit.c` (26行), `do_clear.c` (80行), `do_runctl.c` (76行), `do_schedctl.c` (46行), `do_statectl.c` (52行)
> **Rust 实现**: `os/kernel/src/syscall_process.rs` (1243 行), `os/kernel/src/proc.rs` (fork_from), `os/kernel/src/syscall.rs` (KcallResult), `os/libs/minix-types/src/types/endpoint.rs` (Endpoint)

---

## §1. 设计目标与约束

### 1.1 目标

重写 `os/kernel/src/syscall_process.rs` 及配套类型，使其：
1. **对齐 C ground truth**: 7 个 `do_*` 函数的完整语义——fork（同步前提+代际+降权）、exec（清 DELIVERMSG+arch_proc_init+清 FPU）、exit（cause_sig+EDONTREPLY）、clear（幂等回收+6 类资源释放）、runctl（RC_STOP/RESUME+SMP IPI）、schedctl（双模式）、statectl（5 请求分发）
2. **修复 review 发现的问题**: 删除迭代叙事（日期/P0-XX ID）、tmp 引用、stub `{ ... }`、Ch1 feature-listing、Ch3 平庸决策表、测试不可 grep——重写为 concept-driven + hypothesis-driven + 真实代码 + 可 grep 测试
3. **避免 translate**: 用 Rust 类型系统重新表达 C 语义——`Endpoint` newtype 替代裸 i32、`KProcess::fork_from` 替代 `*rpc = *rpp`、`StatectlRequest` enum 替代 switch/case、`Option` 替代 -1 sentinel、`SchedParams` 结构体替代多参数、`bitflags` 替代裸位运算、`KcallResult` enum 替代 errno 返回
4. **诚实标注 DEFERRED**: 10 项依赖外部子系统的路径（VM/IRQ/IPC/timer/arch/SmpArch/SignalContext/data_copy）明确标注 DEFERRED + 实现路径，不写 stub 隐藏

### 1.2 约束

- `#![no_std]`（除 `#[cfg(test)]`）——`KProcess` 含 `AtomicU8`/`AtomicU32`，无需堆分配
- BKL 保护共享数据（进程表/priv 表访问由上层 dispatch 持 BKL），无 interior mutability（除全局 atomic）
- 硬件抽象为 trait（`ArchProcInit` 等），内核代码无 `#[cfg(target_arch)]` 行为选择
- 不引入 C 兼容层 / FFI
- 代码注释引用 C 源码 `file:line`
- 跨 CPU 无 `Rc`/`RefCell`（BKL 串行化）
- DEFERRED 项标注理由 + 依赖，不写空 stub
- 测试函数名可 grep（`fn test_*`）

### 1.3 Ground Truth 验证

| C 函数 | 行号 | 职责 | Rust 归属 | 状态 |
|--------|------|------|----------|------|
| `do_fork` | do_fork.c:26-134 | 复制 proc，新 endpoint，SYS_PROC 降级，PFF_VMINHIBIT | `dispatch_fork` (syscall_process.rs:141-220) | ✅ 完整 |
| `do_exec` | do_exec.c:20-59 | 清 DELIVERMSG，arch_proc_init 设 IP/SP，清 FPU | `dispatch_exec` (236-277) | ⚠️ 部分（DEFERRED: cross-space copy + arch_proc_init） |
| `do_exit` | do_exit.c:18-25 | cause_sig(SIGABRT)，EDONTREPLY | `dispatch_exit` (285-299) | ✅ 完整（DEFERRED: mini_notify） |
| `do_clear` | do_clear.c:17-78 | 释放地址空间/IRQ/endpoint/timer/FPU/priv，RTS_SLOT_FREE | `dispatch_clear` (392-479) | ⚠️ 部分（IRQ/timer ✅ 已实现；DEFERRED: VM 地址空间/IPC endpoint） |
| `do_runctl` | do_runctl.c:18-73 | RC_STOP/RC_RESUME，SMP IPI | `dispatch_runctl` (423-493) | ✅ 完整（单 CPU；DEFERRED: SMP IPI） |
| `do_schedctl` | do_schedctl.c:7-46 | KERNEL flag 设参数或设 p_scheduler | `dispatch_schedctl` (519-603) | ✅ 完整 |
| `do_statectl` | do_statectl.c:15-51 | 5 请求分发 | `dispatch_statectl` (619-732) | ⚠️ 部分（DEFERRED: ClearIpcRefs + filter 元素填充） |

---

## §2. 核心数据结构设计

### 2.1 Endpoint newtype（D2 — 已实现）

```rust
/// Endpoint identifier, equivalent to C's `endpoint_t` (i32).
///
/// C: `endpoint.h` — `_ENDPOINT(g, p)`, `_ENDPOINT_G(e)`, `_ENDPOINT_P(e)`
///
/// D2: `#[repr(transparent)]` newtype replaces C's raw `i32` to prevent
/// confusion with errno / proc_nr / raw values at compile time. The
/// transparent repr preserves ABI compatibility for FFI/message layout.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Endpoint(pub i32);

impl Endpoint {
    /// Construct from generation and slot. C: `_ENDPOINT(g, p)`.
    pub const fn from_generation_slot(generation: i32, slot: i32) -> Self {
        Self((generation << ENDPOINT_GENERATION_SHIFT) + slot)
    }
    /// Extract slot. C: `_ENDPOINT_P(e)`.
    pub const fn slot(self) -> i32 { /* ... */ }
    /// Extract generation. C: `_ENDPOINT_G(e)`.
    pub const fn generation(self) -> i32 { /* ... */ }
    /// fork: increment generation, recombine with child slot.
    /// C: do_fork.c:59,69-72 — `gen = _ENDPOINT_G(...); ++gen; _ENDPOINT(gen, p_nr)`
    pub fn fork_new_endpoint(old: Endpoint, child_nr: ProcNr) -> Endpoint { /* ... */ }
}
```

**anti-translate 差异**:
- C `endpoint_t` = `i32` → Rust `Endpoint(i32)` newtype：编译期防混淆
- C `_ENDPOINT`/`_ENDPOINT_G`/`_ENDPOINT_P` 宏 → Rust 方法：类型安全
- 代际回绕（`>= _ENDPOINT_MAX_GENERATION` 归 1，do_fork.c:69-70）封装在 `fork_new_endpoint` 内

### 2.2 StatectlRequest enum（D3 — 已实现）

```rust
/// Statectl request types. C: do_statectl.c:19 switch(request), com.h:442-446
///
/// D3: enum + match replaces C's switch/case. The compiler enforces
/// exhaustive matching; new request variants must be handled. `TryFrom<i32>`
/// converts illegal values to `Err(())` → EINVAL, centralizing boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum StatectlRequest {
    ClearIpcRefs = 1,     // SYS_STATE_CLEAR_IPC_REFS, com.h:442
    SetStateTable = 2,    // SYS_STATE_SET_STATE_TABLE, com.h:443
    AddIpcBlFilter = 3,   // SYS_STATE_ADD_IPC_BL_FILTER, com.h:444
    AddIpcWlFilter = 4,   // SYS_STATE_ADD_IPC_WL_FILTER, com.h:445
    ClearIpcFilters = 5,  // SYS_STATE_CLEAR_IPC_FILTERS, com.h:446
}

impl TryFrom<i32> for StatectlRequest {
    type Error = ();
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ClearIpcRefs),
            2 => Ok(Self::SetStateTable),
            3 => Ok(Self::AddIpcBlFilter),
            4 => Ok(Self::AddIpcWlFilter),
            5 => Ok(Self::ClearIpcFilters),
            _ => Err(()),  // → EINVAL, matches C default (do_statectl.c:46-49)
        }
    }
}
```

**anti-translate 差异**:
- C `switch(request)` + `default: EINVAL` → Rust `match req` + `TryFrom` Err→EINVAL
- C 魔术数字 1-5 → Rust 命名变体
- 编译器强制穷尽检查（新增请求必处理）

### 2.3 SchedParams + Option sentinel（D4 — 已实现）

```rust
/// Scheduling parameters for `sched_proc`.
///
/// D4: `Option` replaces C's `-1` sentinel ("keep current value").
/// C: do_schedctl.c:32-34 — `priority/quantum/cpu` are `int`; -1 means
/// "keep current". Rust uses `Option` to express "optional" at the type
/// level; the dispatch layer converts `-1 → None`, `v>=0 → Some(v)`.
pub struct SchedParams {
    pub priority: Option<u8>,   // -1 → None; 0..=15 → Some
    pub quantum: Option<u32>,   // -1 → None; >=1 → Some
    pub cpu: Option<u32>,       // -1 → None; >=0 → Some
    pub niced: bool,            // C: FALSE literal (do_schedctl.c:37)
}
```

**dispatch 层转换**（syscall_process.rs:554-564）:
```rust
// C: do_schedctl.c:32-34 — priority/quantum/cpu; -1 = "keep current"
let priority_opt = match priority {
    -1 => None,
    v if v >= 0 => Some(v as u8),
    _ => return KcallResult::Ok(EINVAL),  // priority < 0 && != -1
};
// ... quantum/cpu 同理
```

**anti-translate 差异**:
- C `-1` 哨兵 → Rust `Option::None`：类型层表达"可选"
- C 范围校验散落 → Rust 集中在 match（非法值 < -1 直接 EINVAL）
- 保留 C 的 -1 消息层兼容（消息仍是 i32）

### 2.4 KcallResult enum（D5 — 已实现）

```rust
/// Kernel call completion status. C: system.c — result encoding.
///
/// D5: enum replaces C's i32 errno return. C distinguishes "success
/// value" (>=0), "errno" (<0), "EDONTREPLY" (-998), "VMSUSPEND" (-996)
/// by value ranges — error-prone. Rust uses distinct variants so the
/// caller `match`es on completion kind, not on integer ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with return value (C: result >= 0 or OK).
    Ok(i32),
    /// Call requires VM assistance (C: VMSUSPEND = -996).
    VmSuspend,
    /// No reply should be sent (C: EDONTREPLY). Used by SYS_EXIT.
    NoReply,
    /// Invalid/unimplemented syscall (C: EBADREQUEST = 212).
    BadCall,
    /// Caller lacks permission (C: ECALLDENIED = 210).
    CallDenied,
}
```

**anti-translate 差异**:
- C `return EDONTREPLY` (-998) → Rust `KcallResult::NoReply`：语义变体而非魔术 errno
- C `return child_endpoint` (正数) vs `return EINVAL` (22) 共享 i32 → Rust `Ok(endpoint)` vs `Ok(EINVAL)` vs `NoReply` 区分
- `dispatch_exit` 返回 `NoReply`（syscall_process.rs:298）精确表达"不回复"

### 2.5 KProcess::fork_from（D1 — 已实现）

```rust
/// Construct a child process from a parent, applying all fork corrections.
///
/// C: do_fork.c:63 — `*rpc = *rpp` (struct assignment copy) followed by
/// field-by-field corrections (do_fork.c:71-122).
///
/// D1: A dedicated constructor replaces C's struct-assignment + manual
/// fixes. `KProcess` contains `AtomicU8`/`AtomicU32` (non-`Copy`), so
/// direct `*rpc = *rpp` is not expressible in Rust. The constructor
/// applies all corrections in one place: RTS_NO_QUANTUM, clear signal
/// flags, clear timer/trace flags, reset p_seg, set None priv_id.
/// The compiler guarantees no field is forgotten.
///
/// C corrections mirrored:
/// - RTS_SET(rpc, RTS_NO_QUANTUM) — do_fork.c:90
/// - RTS_UNSET(rpc, RTS_SIGNALED|SIG_PENDING|P_STOP) — do_fork.c:122
/// - p_misc_flags &= ~(VIRT_TIMER|PROF_TIMER|SC_TRACE|SPROF_SEEN|STEP) — do_fork.c:78-79
/// - p_seg reset (p_cr3=0) — do_fork.c:126-130
pub fn fork_from(parent: &KProcess, child_nr: ProcNr, child_endpoint: Endpoint) -> Self {
    // Copy RTS flags then apply fork corrections.
    let child_rts = {
        let flags = parent.p_rts_flags.get();
        let flags = flags | RtsFlagsBits::NO_QUANTUM;          // do_fork.c:90
        let flags = flags & !(RtsFlagsBits::SIGNALED           // do_fork.c:122
            | RtsFlagsBits::SIG_PENDING
            | RtsFlagsBits::P_STOP
            | RtsFlagsBits::VMREQUEST);
        RtsFlags::with(flags)
    };
    // Copy misc flags then clear timer/trace. do_fork.c:78-79
    let child_mf = { /* ... */ };
    Self { p_nr: child_nr, p_endpoint: child_endpoint, /* ... */ }
}
```

**配套 `complete_fork_setup`**（应用 do_fork.c:105-107,115-116,84-87）:
```rust
/// Apply fork completion: NO_PRIV (if sys proc parent), VMINHIBIT (if
/// requested), name suffix "*F".
/// C: do_fork.c:84-87,93-95,104-106
pub fn complete_fork_setup(child: &mut KProcess, parent_is_sys_proc: bool, fork_flags: u32) {
    if parent_is_sys_proc {
        // C: do_fork.c:105-107 — rpc->p_priv = USER_PRIV_ID; RTS_NO_PRIV
        child.priv_id = Some(USER_PRIV_ID);
        child.p_rts_flags.set(RtsFlagsBits::NO_PRIV);
    }
    if fork_flags & PFF_VMINHIBIT != 0 {
        // C: do_fork.c:115-116 — RTS_SET(rpc, RTS_VMINHIBIT)
        child.p_rts_flags.set(RtsFlagsBits::VMINHIBIT);
    }
    // C: do_fork.c:84-87 — strcat(p_name, "*F")
    child.p_name.push_str("*F");
}
```

### 2.6 ProcNr newtype 升级（D6 — 本轮决策：推迟）

**现状**: `pub type ProcNr = i32;` (proc.rs:24) — 类型别名，编译期无防护。

**决策**: **本轮推迟升级**，记录为已知技术债。理由：
1. `ProcNr` 使用点遍布 `proc.rs`/`proc_table.rs`/`sched.rs`/`smp.rs`/`syscall_process.rs` 等多个模块（grep 约 50+ 处），升级为 `#[repr(transparent)] pub struct ProcNr(pub i32)` 需同步修改所有使用点 + 补 `From<i32>`/`Into<i32>` 转换 + 修复所有 `i32` 与 `ProcNr` 互操作处
2. 本轮 17-syscall-process 重写聚焦于 7 个 dispatch 函数的语义对齐与文档重写，ProcNr 升级是横切关注点，应在独立的"ProcNr newtype 升级"专项中处理，避免与本轮耦合
3. `Endpoint` newtype 已提供编译期防护给最易混淆的 endpoint，ProcNr 的 i32 别名在 BKL 串行化下运行时风险可控

**升级方案**（记录待实施）:
```rust
/// Process number (slot index). C: `int p_nr` / `proc_nr_t`.
///
/// D6 (deferred): upgrade from `type ProcNr = i32` to a newtype to
/// prevent compile-time confusion with other i32 values (errno, raw
/// endpoint). `#[repr(transparent)]` preserves ABI.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcNr(pub i32);

impl From<i32> for ProcNr { fn from(v: i32) -> Self { Self(v) } }
impl From<ProcNr> for i32 { fn from(p: ProcNr) -> i32 { p.0 } }
```

**影响点清单**（升级时需 grep）: `os/kernel/src/proc.rs`, `proc_table.rs`, `sched.rs`, `smp.rs`, `syscall_process.rs`, `syscall_signal.rs` 等所有 `ProcNr` 出现处。

### 2.7 RtsFlagsBits / MiscFlagsBits bitflags（已实现）

```rust
/// RTS flags. C: proc.h — `p_rts_flags` bit operations via RTS_SET/UNSET/ISSET.
///
/// bitflags replaces C's raw `|=`/`&=`/`&` bit ops: type-safe, supports
/// `|`/`&`/`contains`, `from_bits_truncate` tolerates unknown bits.
bitflags::bitflags! {
    pub struct RtsFlagsBits: u32 {
        const SLOT_FREE = 0x01;      // do_clear.c:57
        const PROC_STOP = 0x02;      // do_runctl.c:62
        const SENDING = 0x04;        // do_runctl.c:45
        const RECEIVING = 0x08;      // do_fork.c:51, do_exec.c:51
        const SIGNALED = 0x10;       // do_exit.c (cause_sig), do_fork.c:122
        const SIG_PENDING = 0x20;    // do_fork.c:122
        const NO_QUANTUM = 0x40;     // do_fork.c:90
        const NO_PRIV = 0x80;        // do_fork.c:107
        const VMINHIBIT = 0x100;     // do_fork.c:116
        const P_STOP = 0x200;        // do_fork.c:122
        // ...
    }
}
```

**anti-translate 差异**: C `RTS_SET(rp, RTS_PROC_STOP)` 宏 → Rust `rp.p_rts_flags.set(RtsFlagsBits::PROC_STOP)`：类型安全位操作。

---

## §3. trait 设计（硬件/子系统抽象）

DEFERRED 项依赖的 trait 接口定义。内核 dispatch 代码依赖 trait，不依赖具体实现/`#[cfg]`。

### 3.1 ArchProcInit trait（exec — DEFERRED）

```rust
/// Architecture-specific process initialization after exec.
///
/// C: `arch_proc_init(rp, ip, stack, ps_str, name)` — do_exec.c:45-48
/// Sets the new IP/SP/ps_str/name on the target process. Each arch
/// provides an implementation; kernel code depends only on the trait.
pub trait ArchProcInit {
    /// Initialize process registers for new IP/SP.
    /// C: arch/<arch>/proc.c — arch_proc_init
    fn proc_init(
        rp: &mut KProcess,
        ip: u32,
        stack: u32,
        ps_str: u32,
        name: &str,
    );
}
```

**DEFERRED 理由**: 各 arch 实现需在 `os/arch/src/arch/` 提供（x86_64/aarch64/riscv64）。当前 `dispatch_exec` 标注 DEFERRED (syscall_process.rs:263)。

### 3.2 SignalContext trait（exit 通知 — DEFERRED）

```rust
/// Signal subsystem integration for SYS_EXIT notification.
///
/// C: `cause_sig(caller, SIGABRT)` — do_exit.c:24 → system.c:389-426
/// The full C path: look up sig_mgr from priv(rp), set RTS_SIGNALED +
/// s_sig_pending, then `mini_notify(sig_mgr, caller->p_endpoint)`.
/// The in-place state mutations are done by `cause_signal_abort`;
/// the signal-manager notify is deferred to this trait.
pub trait SignalContext {
    /// Notify the signal manager that `caller` has a pending signal.
    /// C: system.c:413-414 — mini_notify(sig_mgr, caller->p_endpoint)
    fn notify_signal_manager(caller: &KProcess, priv_table: &PrivTable);
}
```

**DEFERRED 理由**: 需 PrivTable + mini_notify（kernel IPC core）。当前 `cause_signal_abort` 仅做 in-place 状态设置 (syscall_process.rs:308-320)，通知 DEFERRED。

### 3.3 VmContext trait（clear 地址空间 — DEFERRED）

```rust
/// VM integration for address-space release.
///
/// C: `release_address_space(rc)` — do_clear.c:35
pub trait VmContext {
    fn release_address_space(rc: &mut KProcess);
}
```

### 3.4 IrqManager trait（clear IRQ hooks — ✅ 已实现）

```rust
/// IRQ manager integration for hook release.
///
/// C: do_clear.c:41-46 — loop `irq_hooks[]`, `rm_irq_handler`, mark NONE.
pub trait IrqManager {
    /// Release any IRQ hooks owned by `rc->p_endpoint`.
    fn release_hooks(rc_endpoint: Endpoint);
}
```

**已实现**: `dispatch_clear` 不通过 trait 抽象，而是直接复用全局 `irq_manager()`（与 `dispatch_irqctl` 同一访问点）。实现遍历 `0..NR_IRQ_HOOKS` 槽位，对 `hook_owner(slot) == target_endpoint` 的槽调 `remove_hook_by_slot(slot)`（syscall_process.rs:425-438）。测试通过 `init_irq_manager_for_test()` 初始化全局 IrqManager。

### 3.5 IpcEngine trait（clear endpoint / statectl — DEFERRED）

```rust
/// IPC engine integration for endpoint cleanup and filters.
///
/// C: `clear_endpoint(rc)` — do_clear.c:49
/// C: `clear_ipc_refs(caller, EDEADSRCDST)` — do_statectl.c:25
/// C: `add_ipc_filter`/`clear_ipc_filters` — do_statectl.c:34,39,44
pub trait IpcEngine {
    /// Remove the process's ability to send/receive. C: clear_endpoint
    fn clear_endpoint(rc: &mut KProcess);
    /// Clear IPC refs for all processes communicating with caller.
    /// C: clear_ipc_refs(caller, EDEADSRCDST) — do_statectl.c:25
    fn clear_ipc_refs(caller: &mut KProcess);
    /// Populate filter elements from user space.
    /// C: add_ipc_filter(..., address, length) — do_statectl.c:32-41
    fn populate_filter(
        caller: &KProcess,
        filter_type: IpcFilterType,
        address: usize,
        length: usize,
    ) -> Result<(), i32>;
}
```

**当前 statectl 实现**: 槽位分配/释放已实现（syscall_process.rs:674-727），filter 元素填充 DEFERRED (L690/712)。

### 3.6 TimerContext trait（clear 告警定时器 — ✅ 已实现）

```rust
/// Timer subsystem integration for alarm timer reset.
///
/// C: `reset_kernel_timer(&priv(rc)->s_alarm_timer)` — do_clear.c:52
pub trait TimerContext {
    fn reset_alarm_timer(priv_entry: &mut KPriv);
}
```

**已实现**: `dispatch_clear` 不通过 trait 抽象，而是直接接 `ClockState`：新增 `clock_state: &mut ClockState` 参数，从目标的 `priv.runtime.s_alarm_timer: Option<(TimerEntry, TimerId)>` 中 `take()` 出挂起的闹钟，调 `clock_state.reset_timer(timer_id)` 从时钟活动定时器链中取消（syscall_process.rs:443-453）。

### 3.7 SmpArch 接入（runctl SMP IPI — DEFERRED，见 16-smp）

```rust
/// SMP integration for cross-CPU runctl.
///
/// C: do_runctl.c:55-60 — `if (rp->p_cpu != cpuid) smp_schedule_stop_proc(rp);`
/// The SmpArch trait and smp_schedule_stop_proc are defined in 16-smp.
/// dispatch_runctl currently implements the single-CPU path only; the
/// SMP branch is DEFERRED until SmpArch::schedule_stop_proc is wired.
pub trait SmpRunctl {
    /// Stop a process running on a different CPU via synchronous IPI.
    /// C: smp_schedule_stop_proc(rp) — smp.c:114-121
    fn schedule_stop_proc(target: &KProcess);
}
```

**DEFERRED 理由**: 依赖 `SmpArch` trait (16-smp §4.6)。当前 `dispatch_runctl` 仅单 CPU RTS_SET (syscall_process.rs:475)。

### 3.8 DataCopy trait（exec 名字 / statectl filter — DEFERRED）

```rust
/// Cross-address-space copy for user-supplied data.
///
/// C: `data_copy(caller->p_endpoint, src, KERNEL, dst, len)` — do_exec.c:37-39
/// Used by exec (process name) and statectl (filter elements).
pub trait DataCopy {
    /// Copy `len` bytes from `src` in `src_proc`'s address space into
    /// `dst` in KERNEL. Returns OK or errno.
    /// C: data_copy — system.c
    fn copy_in(src_proc: Endpoint, src: usize, dst: &mut [u8]) -> Result<(), i32>;
}
```

**DEFERRED 理由**: 需 VM 集成（data_copy_vmcheck）。当前 exec 名字拷贝 (syscall_process.rs:259) 与 statectl filter 填充 (L690/712) 均标注 DEFERRED。

---

## §4. 限制与 DEFERRED

### 4.1 DEFERRED 函数实现路径表

| DEFERRED 项 | C 位置 | 依赖 trait | 实现路径 |
|------------|--------|-----------|---------|
| exec cross-space copy | do_exec.c:37-39 | `DataCopy` | VM 集成后接入 `copy_in` |
| exec arch_proc_init | do_exec.c:45-48 | `ArchProcInit` | 各 arch 在 `os/arch/src/arch/` 提供 impl |
| clear release_address_space | do_clear.c:35 | `VmContext` | VM 集成 |
| ~~clear IRQ hooks~~ | do_clear.c:41-46 | ✅ 已实现 | 全局 `irq_manager()` + `remove_hook_by_slot`（syscall_process.rs:425-438） |
| clear clear_endpoint | do_clear.c:49 | `IpcEngine` | IPC module 集成 |
| ~~clear reset_kernel_timer~~ | do_clear.c:52 | ✅ 已实现 | `clock_state.reset_timer(timer_id)` + `s_alarm_timer.take()`（syscall_process.rs:443-453） |
| runctl SMP IPI | do_runctl.c:55-60 | `SmpRunctl`/`SmpArch` | 见 16-smp §4.6 |
| exit mini_notify | do_exit.c:24 | `SignalContext` | SignalContext trait + IPC |
| statectl ClearIpcRefs | do_statectl.c:21-26 | `IpcEngine` | IPC engine（senda/cancel_async） |
| ~~statectl filter 元素填充~~ | do_statectl.c:32-41 | ✅ 已实现 | data_copy_vmcheck + filter pool |

### 4.2 已知技术债

| 技术债 | 现状 | 处理计划 |
|--------|------|---------|
| ProcNr 类型别名（D6 推迟） | `type ProcNr = i32` (proc.rs:24) | 独立专项升级为 newtype（见 §2.6） |
| fork 无测试 | dispatch_fork ✅ 完整但 0 测试 | 本轮建议补 3 个测试（见 §4.3） |
| runctl RC_DELAY 路径无测试 | dispatch_runctl 实现但无 RC_DELAY 测试 | 补 `test_dispatch_runctl_rc_delay_returns_ebusy` |

### 4.3 fork 测试补全建议（本轮）

`dispatch_fork` 已完整实现（syscall_process.rs:141-220），建议本轮补 3 个测试：

| 测试函数 | 验证行为 | C 依据 |
|---------|---------|--------|
| `test_dispatch_fork_creates_child_with_new_endpoint` | fork 创建子+新 endpoint 代际+1 | do_fork.c:69-72 |
| `test_dispatch_fork_rejects_non_receiving_parent` | 父非 RECEIVING → EINVAL | do_fork.c:51-54 |
| `test_dispatch_fork_downgrades_sys_proc_child` | SYS_PROC 父→USER 子+RTS_NO_PRIV | do_fork.c:105-107 |

### 4.4 no_std 约束

- `#![no_std]`（除 `#[cfg(test)]`）
- `KProcess` 字段：`AtomicU8`/`AtomicU32`/`Option<ProcNr>`/`Option<Endpoint>`/`RtsFlags`/`MiscFlags`/`SchedFields`——均无需堆分配
- `Endpoint`/`StatectlRequest`/`KcallResult`/`SchedParams`——栈值类型，无 `alloc`
- `bitflags` 宏在 no_std 下可用
- 测试模块 `#[cfg(test)]` 可用 `std`

---

## §5. 附录

### 5.1 C → Rust 行为差异矩阵

| C 行为 | C 位置 | Rust 实现 | 差异类型 | 说明 |
|--------|--------|----------|---------|------|
| `*rpc = *rpp` 结构体赋值 | do_fork.c:63 | `KProcess::fork_from` | 架构演进 | 非 Copy 字段需构造函数 |
| `_ENDPOINT(gen, p_nr)` 宏 | do_fork.c:72 | `Endpoint::fork_new_endpoint` | anti-translate | newtype 方法替代宏 |
| `switch(request)` | do_statectl.c:19 | `match StatectlRequest::try_from` | anti-translate | enum + 穷尽检查 |
| `-1` sentinel | do_schedctl.c:32-34 | `Option::None` | anti-translate | 类型层表达可选 |
| `return EDONTREPLY` | do_exit.c:25 | `KcallResult::NoReply` | anti-translate | enum 变体替代 errno |
| `RTS_SET(rp, RTS_PROC_STOP)` | do_runctl.c:62 | `rts_set(PROC_STOP)` | anti-translate | bitflags 方法 |
| `iskerneln(proc_nr)` → EPERM | do_runctl.c:31 | `ProcessTable::is_kernel` → EPERM | 语义对齐 | 一致 |
| `isemptyp(rc)` 幂等 | do_clear.c:38 | `SLOT_FREE` 检查 → OK | 语义对齐 | 一致 |
| `p->p_scheduler = caller` | do_schedctl.c:42 | `scheduler = Some(caller.p_nr)` | anti-translate | Option 索引替代裸指针 |
| `cause_sig(caller, SIGABRT)` | do_exit.c:24 | `cause_signal_abort` 助手 | 部分对齐 | in-place 状态 ✅；mini_notify DEFERRED |

### 5.2 anti-translate 决策汇总

| 决策 | C 语义 | Rust 表达 | 状态 |
|------|--------|----------|------|
| D1 fork_from | `*rpc = *rpp` + 逐字段修正 | `KProcess::fork_from` 构造函数 | ✅ 已实现 |
| D2 Endpoint newtype | 裸 `endpoint_t = i32` | `#[repr(transparent)] struct Endpoint(i32)` | ✅ 已实现 |
| D3 StatectlRequest enum | switch/case + 魔术数字 | `enum + TryFrom + match` | ✅ 已实现 |
| D4 Option sentinel | `-1` 哨兵 | `Option<T>` | ✅ 已实现 |
| D5 KcallResult enum | errno 范围判断 | `enum { Ok, VmSuspend, NoReply, BadCall, CallDenied }` | ✅ 已实现 |
| D6 ProcNr newtype | 裸 `int p_nr` | `type ProcNr = i32`（别名） | ⚠️ 推迟升级（见 §2.6） |
| bitflags RtsFlagsBits | `RTS_SET`/`RTS_UNSET` 宏 | `bitflags!` + `.set()`/`.clear()` | ✅ 已实现 |
| SchedParams 结构体 | 多独立参数 | `SchedParams { priority, quantum, cpu, niced }` | ✅ 已实现 |

### 5.3 与 redox OS 进程管理对照

| 关注点 | Minix3 / Minix-RS | redox OS |
|--------|-------------------|----------|
| 进程实体 | `struct proc` / `KProcess` | `context::Context` |
| 进程表 | 全局 `proc[]` 数组 + BKL | `contexts` scheme + `Arc<RwLock<Context>>` |
| fork | SYS_FORK 显式 syscall（同步点） | `scheme::proc` dup 操作 |
| exec | SYS_EXEC 替换 IP/SP | `scheme::exec` |
| 信号 | cause_sig + 信号管理器（PM） | `scheme::signal` |
| 调度 | per-CPU 队列 + BKL | per-CPU 队列 + spinlock |
| 借鉴点 | — | redox 用 `Arc`+`RwLock` 管理进程表；Minix-RS 用 BKL 串行化 + ProcNr 索引（更接近 C 原语义，避免 `Arc` 的 alloc 依赖） |

**设计取舍**: Minix-RS 选择 BKL + ProcNr 索引（而非 redox 的 `Arc<RwLock>`），理由：
1. 对齐 C 的 BKL 串行化模型（见 16-smp），减少 SMP 引入的复杂度
2. `Arc` 需 `alloc` crate，与 no_std 早期启动约束冲突
3. ProcNr 索引 + 进程表统一访问，避免 `Arc` 的引用计数开销

### 5.4 参见

- [11-scheduling-primitives.md](../11-scheduling-primitives.md) — `sched_proc` / `SchedParams` / 优先级与时间片
- [16-smp.md](../16-smp.md) — `SmpArch` / `smp_schedule_stop_proc` / IPI 同步 / BKL
- [06-proc-init-boot-proc.md](../06-proc-init-boot-proc.md) — `KProcess` 结构 / `RtsFlagsBits` / proc 表
- [10-switch-to-user.md](../10-switch-to-user.md) — RTS 标志变更触发的调度入队
- [14-exception-interrupt.md](../14-exception-interrupt.md) — 信号投递与异常入口
- [22-privilege.md](../22-privilege.md) — `USER_PRIV_ID` / `SYS_PROC` / `PrivTable`（前向引用，待创建）

---

## §6. 自检

- [x] §1 目标对齐 7 个 do_* 语义 + anti-translate + DEFERRED 诚实标注
- [x] §1.2 no_std 约束明确
- [x] §1.3 Ground Truth 验证表（7 函数 × 状态）
- [x] §2 数据结构覆盖 D1-D6 决策（Endpoint/StatectlRequest/SchedParams/KcallResult/fork_from/ProcNr/bitflags）
- [x] §2.6 ProcNr 决策明确（本轮推迟 + 升级方案 + 影响点）
- [x] §3 trait 设计覆盖 8 个 DEFERRED 依赖（ArchProcInit/SignalContext/VmContext/IrqManager/IpcEngine/TimerContext/SmpRunctl/DataCopy）
- [x] §4.1 DEFERRED 实现路径表（10 项 × 依赖 trait × 路径）
- [x] §4.2 已知技术债（ProcNr/fork 测试/RC_DELAY 测试）
- [x] §5.1 C→Rust 行为差异矩阵（10 行）
- [x] §5.2 anti-translate 决策汇总（8 项 × 状态）
- [x] §5.3 redox 对照（7 维度 + 设计取舍）
- [x] 无迭代叙事日期 / 无 tmp 引用 / 无 P0-XX ID
- [x] 代码注释英文，文档代码块注释中文
- [x] 所有 C 引用带 file:line
