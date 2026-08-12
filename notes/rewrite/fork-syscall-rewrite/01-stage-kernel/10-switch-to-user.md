# 10-switch-to-user: 调度循环入口

> **分类**: Kernel 调度核心
> **源码**: `minix3/minix/kernel/proc.c:299-474`（switch_to_user）, `proc.c:176-229`（idle）
> **前置**: 08（bsp_finish_booting 调用 switch_to_user）+ 09（VM 启动协议完成，VM 可调度）
> **C 总行数**: ~180 行

---

## 1. 概述

### 1.1 核心问题

`switch_to_user()` 是内核运行时的核心枢纽。所有三条激活路径（硬件中断、CPU 异常、系统调用）最终都汇聚于此函数。它负责：

1. **进程可运行性检查**：当前进程是否仍可运行？
2. **Misc 标志处理**：是否有延迟操作需要执行？
3. **时间片管理**：进程是否用完了时间片？
4. **地址空间切换**：是否需要切换 CR3？
5. **上下文恢复**：恢复用户态寄存器，iret 返回用户态

### 1.2 三条激活路径

```
硬件中断 → interrupt() → 保存上下文 → switch_to_user()
CPU 异常 → exception() → 保存上下文 → switch_to_user()
系统调用 → sys_call()  → 保存上下文 → switch_to_user()
```

无论哪条路径进入，`switch_to_user()` 的逻辑完全相同——它不关心"为什么进入内核"，只关心"接下来该运行谁"。

### 1.3 控制流图

```
switch_to_user()
  │
  ├─ 当前进程可运行？ ──── 是 ──→ check_misc_flags
  │                              │
  │                              否
  │                              ↓
  ├─ not_runnable_pick_new:
  │   ├─ 处理 PREEMPTED 标志
  │   ├─ 重新入队（enqueue_head 或 enqueue）
  │   └─ while (!pick_proc()) idle()
  │
  ├─ 更新 proc_ptr = p
  ├─ switch_address_space(p)
  │
  ├─ check_misc_flags:
  │   while (MF_KCALL_RESUME | MF_DELIVERMSG | MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE)
  │       ├─ MF_KCALL_RESUME → kernel_call_resume(p)
  │       ├─ MF_DELIVERMSG   → delivermsg(p)
  │       ├─ MF_SC_DEFER     → arch_do_syscall(p)
  │       ├─ MF_SC_TRACE     → cause_sig(SIGTRAP)
  │       └─ MF_SC_ACTIVE    → 清除，break
  │       └─ 进程不可运行？ → goto not_runnable_pick_new
  │
  ├─ 时间片检查：!p_cpu_time_left → proc_no_time(p)
  ├─ 进程仍可运行？否 → goto not_runnable_pick_new
  ├─ arch_finish_switch_to_user()
  ├─ FPU 所有权检查
  ├─ 清除 MF_CONTEXT_SET
  ├─ SMP TLB 刷新
  ├─ restart_local_timer()
  └─ restore_user_context(p)  ← 不返回
```

---

## 2. C 源码分析

### 2.1 switch_to_user() — 主循环

**源码**: `proc.c:299-474`

**阶段 1：进程选择**（proc.c:309-344）

```c
p = get_cpulocal_var(proc_ptr);
if (proc_is_runnable(p))
    goto check_misc_flags;

not_runnable_pick_new:
    if (proc_is_preempted(p)) {
        p->p_rts_flags &= ~RTS_PREEMPTED;
        if (proc_is_runnable(p)) {
            if (p->p_cpu_time_left) enqueue_head(p);
            else enqueue(p);
        }
    }
    while (!(p = pick_proc())) { idle(); }
    get_cpulocal_var(proc_ptr) = p;
```

**关键语义**：
- 如果当前进程仍可运行，直接跳到 misc 标志处理
- PREEMPTED 进程根据剩余时间片决定入队头还是队尾
- 无可运行进程时进入 idle 循环

**阶段 2：地址空间切换**（proc.c:349）

```c
switch_address_space(p);
```

在选好进程后、处理 misc 标志前切换地址空间。这确保后续的 `delivermsg()` 等操作在正确的地址空间中执行。

**阶段 3：Misc 标志处理**（proc.c:350-414，`check_misc_flags:` 标签起）

按优先级处理 5 种 misc 标志：

| 优先级 | 标志 | 操作 | 行号 |
|--------|------|------|------|
| 1 | `MF_KCALL_RESUME` | `kernel_call_resume(p)` | 359-361 |
| 2 | `MF_DELIVERMSG` | `delivermsg(p)` | 362-366 |
| 3 | `MF_SC_DEFER` | `arch_do_syscall(p)` | 367-381 |
| 4 | `MF_SC_TRACE` | `cause_sig(SIGTRAP)` | 382-398 |
| 5 | `MF_SC_ACTIVE` | 清除标志，break | 399-406 |

**循环条件**（proc.c:354-356）：`while (p->p_misc_flags & (MF_KCALL_RESUME | MF_DELIVERMSG | MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE))`

每次处理完一个标志后检查进程是否仍可运行，不可运行则 `goto not_runnable_pick_new`。

**阶段 4：时间片检查**（proc.c:421-422）

```c
if (!p->p_cpu_time_left)
    proc_no_time(p);
```

`proc_no_time()` 向调度服务器发送消息通知时间片用完，但不清除进程的可运行状态——调度服务器稍后会通过 `SYS_SCHEDULE` 重新设置时间片。

**阶段 5：上下文恢复**（proc.c:437-474）

```c
p = arch_finish_switch_to_user();
assert(p->p_cpu_time_left);
context_stop(proc_addr(KERNEL));
// FPU 所有权检查
// 清除 MF_CONTEXT_SET
// SMP TLB 刷新
restart_local_timer();
restore_user_context(p);  // 不返回
```

### 2.2 idle() — CPU 空闲

**源码**: `proc.c:176-229`

```c
static void idle(void) {
    p = get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
    if (priv(p)->s_flags & BILLABLE)
        get_cpulocal_var(bill_ptr) = p;
    switch_address_space_idle();
    // SMP: cpu_is_idle=1; AP: stop_local_timer(); BSP: restart_local_timer();
    context_stop(proc_addr(KERNEL));  // 开始统计 idle 时间
    halt_cpu();  // STI + HLT（或 sprofiling 模式下轮询 idle_interrupted）
    // idle 结束的统计不在此时做——中断返回后内核处理很多工作才回到这里
}
```

**关键语义**：
- 设置当前进程为 idle 进程（用于时间统计）
- 切换到 idle 地址空间
- BSP 保持定时器运行（用于时钟中断唤醒），AP 停止定时器
- `context_stop(KERNEL)` 开始 idle 时间统计（与 switch_to_user 中的 `context_stop` 配对）
- `halt_cpu()` 执行 STI + HLT，等待下一个中断

---

## 3. Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_to_user 返回 | 不返回（C 风格） vs 返回 Result | **不返回（! 类型）** | C 的 `restore_user_context` 不返回，Rust 用 `!` 表达 |
| 当前实现状态 | 完整调度循环 vs 占位 stub | **占位 stub** | 完整调度循环依赖 10/11/13 等后续文档；当前 `lib.rs::switch_to_user()` 仅释放 BKL 后 `loop { spin_loop() }` |
| proc_ptr | 全局变量 vs CpuLocal | **CpuLocal\<Option\<ProcNr\>\>** | SMP 安全，每 CPU 独立（设计目标） |
| idle | 内联 vs 独立方法 | **独立方法** | 与 C 一致（设计目标） |
| Misc 标志循环 | while + if-else chain vs match | **while + if-else chain** | 当前 `ProcessTable::process_misc_flags()` 按 C 的优先级链处理 |
| restore_user_context | 内联汇编 vs trait | **抽象为 `TrapReturnArch` trait（评估结论，待落地）** | 09 早期版本曾定义 `ContextRestore` 死代码已移除；2026-07-31 doc 03 §4.3 评估结论：trait IS warranted（与 `TrapEntryArch` 对称、arch-agnostic、职责正交），拟议签名 `trait TrapReturnArch: ExceptionArch { type RegisterFile; unsafe fn restore_to_user(frame: &Self::Frame, regs: &Self::RegisterFile) -> !; }`。trait 定义 + asm impl 待本文 return-path 落地时加入 `os/arch/src/arch/trap_return.rs` |
| 地址空间切换时机 | 选进程后立即 vs 恢复上下文前 | **选进程后立即** | 与 C 一致（设计目标） |

---

## 4. 实现要点

### 4.1 当前 switch_to_user 占位实现

**位置**: `os/kernel/src/lib.rs:1508-...`（`fn switch_to_user() -> !`）

```rust
// os/kernel/src/lib.rs

/// Entry point for the scheduling loop.
///
/// C: switch_to_user() in proc.c
/// Returns `!` — never returns to caller.
///
/// Full implementation covered in 10-switch-to-user.md.
///
/// # BKL (Big Kernel Lock)
///
/// In C, the BKL is released in `restore_user_context()` (the last thing
/// before returning to user mode). In Rust, we release the BKL at the
/// top of `switch_to_user()` before the scheduling loop. This is safe
/// because:
///
/// 1. The scheduling loop itself does not modify shared kernel state
///    (it only reads per-CPU state and picks a process).
/// 2. If a process needs kernel service (syscall, exception), the
///    entry point re-acquires the BKL before touching shared state.
/// 3. This matches C's pattern: BKL is released before the context
///    switch and re-acquired on the next kernel entry.
fn switch_to_user() -> ! {
    // Release BKL before entering the scheduling loop.
    // C: BKL is released implicitly by restore_user_context() which
    // does not return. In Rust, we release explicitly before the loop.
    smp::bkl_unlock();

    // First-dispatch hook: apply each boot process's `cpu_context`
    // to its trap frame once, before the scheduling loop picks the
    // first runnable process. The arch layer owns the trap-frame
    // layout; the kernel only hands it the opaque `CpuContext` built
    // during `init_proc_and_boot`.
    //
    // C: this work is folded into `arch_boot_proc()` + the first
    // `restore_user_context()` in Minix3. Splitting it here keeps the
    // arch trait's `apply_to_trap_frame` as the single sink for
    // initial-register writes (arch returns pure value).
    //
    // SAFETY: boot is single-threaded (BKL just released, but no other
    // CPU is up yet on single-CPU configs). On SMP this must move
    // inside the per-CPU dispatch path.
    unsafe {
        apply_boot_cpu_contexts();
    }

    // Placeholder — full scheduler loop implemented in 10-switch-to-user.md
    loop {
        core::hint::spin_loop();
    }
}
```

**设计要点**：
- **BKL 顶部释放**：C 在 `restore_user_context()` 隐式释放，Rust 在函数顶部显式释放（设计决策，见上方安全论证）
- **首次分发拆分**：C 折叠在 `arch_boot_proc()` + 首次 `restore_user_context()` 中；Rust 拆分为独立的 `apply_boot_cpu_contexts()` 步骤，使 arch trait 的 `apply_to_trap_frame` 成为初始寄存器写入的唯一入口（arch 返回纯值，不写硬件）
- **`-> !` 类型**：Rust 用类型系统表达"永不返回"，替代 C 的 `NOT_REACHABLE`

> 实现状态：当前 `switch_to_user()` 是占位 stub。完整调度循环（选进程、`process_misc_flags`、地址空间切换、`restore_user_context`）依赖 11/13/14 等文档的调度/IPC/异常机制完成后才能落地。

### 4.2 process_misc_flags 实现

**位置**: `os/kernel/src/proc_table.rs:833-926`（`pub fn process_misc_flags`）

```rust
// os/kernel/src/proc_table.rs

/// 处理进程的 misc 标志。
///
/// C: check_misc_flags 循环 — proc.c:350-414
///
/// 返回 true 表示进程仍可运行，false 表示不可运行（需重新选择）。
///
/// In C, each branch calls a handler function (kernel_call_resume,
/// delivermsg, arch_do_syscall) which clears the corresponding flag.
/// In Rust:
/// - `DELIVERMSG` (FIX-20): wired to `crate::ipc::delivermsg`
/// - `KCALL_RESUME` (FIX-21): wired to `crate::vm::kernel_call_resume`
///   (simple version — clears flag + reads VM result; full re-dispatch
///   is done by `switch_to_user` which has access to priv_table +
///   clock_state + proc_table)
/// - `SC_DEFER` (FIX-21): wired to `self.arch_do_syscall()`
/// - `SC_TRACE` / `SC_ACTIVE`: still TODO (future phase)
pub fn process_misc_flags(
    &mut self,
    nr: ProcNr,
    user_copy: &dyn crate::ipc::UserCopy,
    priv_table: &mut crate::kpriv::PrivTable,
) -> bool {
    let interesting_flags = MiscFlagsBits::KCALL_RESUME
        | MiscFlagsBits::DELIVERMSG
        | MiscFlagsBits::SC_DEFER
        | MiscFlagsBits::SC_TRACE
        | MiscFlagsBits::SC_ACTIVE;

    loop {
        let flags = self.get(nr).map_or(MiscFlagsBits::empty(), |p| p.p_misc_flags.get());
        if !flags.intersects(interesting_flags) {
            break;
        }

        // 按优先级处理（与 C 的 if-else chain 一致）
        if flags.contains(MiscFlagsBits::KCALL_RESUME) {
            // C: kernel_call_resume(p) — system.c:612-638.
            // FIX-21 (Phase 1C): wired to crate::vm::kernel_call_resume
            // (simple version — reads VM result + clears MF_KCALL_RESUME).
            let result = match self.get_mut(nr) {
                Some(p) => crate::vm::kernel_call_resume(p),
                None => break,
            };
            let _ = result; // TODO: route VmCheckResult in switch_to_user
        } else if flags.contains(MiscFlagsBits::DELIVERMSG) {
            // C: delivermsg(p) — proc.c:263-294.
            // FIX-20 (Phase 1B): wired to crate::ipc::delivermsg.
            let result = match self.get_mut(nr) {
                Some(p) => crate::ipc::delivermsg(p, user_copy),
                None => break,
            };
            match result {
                crate::ipc::DeliverResult::Delivered => { /* continue loop */ }
                crate::ipc::DeliverResult::PageFault => {
                    // TODO: vm_suspend(VMS_PAGEFAULT) — future phase
                    break;
                }
                crate::ipc::DeliverResult::Segfault => {
                    // TODO: cause_sig(SIGSEGV) — future phase
                    break;
                }
            }
        } else if flags.contains(MiscFlagsBits::SC_DEFER) {
            // C: arch_do_syscall(p) — arch_system.c:485 (i386) / 141 (earm)
            // FIX-21 (Phase 1C): wired to self.arch_do_syscall().
            let _ = self.arch_do_syscall(nr, priv_table);
        } else if flags.contains(MiscFlagsBits::SC_TRACE) {
            if !flags.contains(MiscFlagsBits::SC_ACTIVE) {
                break;
            }
            self.get_mut(nr).map(|p| {
                p.p_misc_flags.clear(MiscFlagsBits::SC_TRACE | MiscFlagsBits::SC_ACTIVE);
            });
            // TODO: wire cause_sig() from signal module
            break;
        } else if flags.contains(MiscFlagsBits::SC_ACTIVE) {
            self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::SC_ACTIVE));
            break;
        }

        // 检查进程是否仍可运行
        if !self.get(nr).map_or(false, |p| p.is_runnable()) {
            return false;
        }
    }
    true
}
```

**设计要点**：
- **bitflags 类型安全**：C 用裸 `u32` 位掩码，Rust 用 `MiscFlagsBits` bitflags，`contains`/`intersects`/`clear` 操作类型安全
- **if-else chain 保留 C 优先级语义**：C 的 if-else chain 表达了 5 个标志的优先级顺序（KCALL_RESUME > DELIVERMSG > SC_DEFER > SC_TRACE > SC_ACTIVE），Rust 保留此结构以匹配 C 语义
- **bool 返回值**：C 用 `goto not_runnable_pick_new` 跳转，Rust 用 `bool` 返回值让 caller 决定是否重新选进程（类型安全改进）
- **`user_copy` + `priv_table` 参数注入**：`process_misc_flags` 接受 `&dyn UserCopy` + `&mut PrivTable` 参数（FIX-20/21），避免在循环内构造完整 `IpcEngine`（`IpcEngine` 需要 `priv_table` + `procs` 切片，而 `ProcessTable` 已持有这些）
- **KCALL_RESUME 分裂设计**（FIX-21 设计偏差）：C 的 `kernel_call_resume` 在 misc_flags 循环内做完整重新分发。Rust 分为两步：`vm::kernel_call_resume`（简单版）在循环内清标志 + 读 VM 结果；`syscall::kernel_call_resume`（完整版）由 `switch_to_user` 调用做重新分发。原因是 Rust 借用检查器：`process_misc_flags` 持有 `&mut self`，无法同时传 `self` 作为 `proc_table` 给 `syscall::kernel_call_resume`
- **`arch_do_syscall` 非 trait 方法**（FIX-21）：C 的 `arch_do_syscall` 是 arch-specific（i386 用 `p_defer`，ARM 用 `p_reg`），Rust 统一为 `p_defer` struct，消除 arch 差异。放入 `TrapEntryArch` trait 会产生 3 个相同实现，违反"≥2 行为不同实现"规则。实现为 `ProcessTable` 方法

> **接入状态**（FIX-21, Phase 1C, 2026-08-12）：
> - `KCALL_RESUME` 分支已接入 `crate::vm::kernel_call_resume`（[vm.rs:886](file:///home/xzhao/github/minix-rs/os/kernel/src/vm.rs)），清标志 + 读 VM 结果。完整重新分发由 `switch_to_user` 调用 `syscall::kernel_call_resume`（[syscall.rs:1645](file:///home/xzhao/github/minix-rs/os/kernel/src/syscall.rs)）
> - `SC_DEFER` 分支已接入 `self.arch_do_syscall()`（[proc_table.rs:963](file:///home/xzhao/github/minix-rs/os/kernel/src/proc_table.rs)），清标志 + 重新分发 IPC
> - `SC_TRACE` / `SC_ACTIVE` 仍为 TODO（future phase — 依赖 signal module）
> - `vm_suspend(VMS_PAGEFAULT)` 和 `cause_sig(SIGSEGV)` 路由由 caller（`switch_to_user`）实现，依赖 future phase

---

## 5. 测试

### 5.1 单元测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_process_misc_flags_empty_returns_true` | `proc_table.rs` | 无 misc 标志时返回 true |
| `test_process_misc_flags_clears_kcall_resume` | `proc_table.rs` | MF_KCALL_RESUME 经 `vm::kernel_call_resume` 清除（FIX-21）；需 Completed VmSuspendContext |
| `test_process_misc_flags_clears_delivermsg` | `proc_table.rs` | MF_DELIVERMSG 经 `ipc::delivermsg` 拷贝成功后清除（FIX-20）；MF_MSGFAILED 也被清 |
| `test_process_misc_flags_clears_sc_defer` | `proc_table.rs` | MF_SC_DEFER 经 `arch_do_syscall` 清除（FIX-21）；p_defer.r1 = SEND |
| `test_process_misc_flags_unrunnable_returns_false` | `proc_table.rs` | misc 处理后不可运行 → 返回 false |
| `test_bsp_finish_booting_*` | `lib.rs` | `switch_to_user()` 在 bsp_finish_booting 中被调用且不返回 |

### 5.2 集成测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `boot_simulation_full_flow` | `tests/boot_integration.rs` | 启动流程集成验证 |

> 注：`switch_to_user()` 当前为占位 stub，完整调度循环的单元/集成测试需在 10/11/13 等机制落地后补充。

---

## 6. 参见

- [08-system-init-boot-finish](08-system-init-boot-finish.md) — bsp_finish_booting 调用 switch_to_user（D7 设计决策来源）
- [09-vm-boot-protocol](09-vm-boot-protocol.md) — switch_address_space 和 VMCTL_SETADDRSPACE
- [11-scheduling-primitives](11-scheduling-primitives.md) — enqueue/dequeue/pick_proc
- [12-ipc-core](12-ipc-core.md) — delivermsg 和 IPC 投递
- [14-exception-interrupt](14-exception-interrupt.md) — 三条激活路径
