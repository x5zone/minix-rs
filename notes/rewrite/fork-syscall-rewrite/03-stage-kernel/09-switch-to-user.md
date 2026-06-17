# 09-switch-to-user: 调度循环入口

> **分类**: Kernel 调度核心
> **源码**: `minix3/minix/kernel/proc.c:299-477`（switch_to_user）, `proc.c:176-213`（idle）
> **前置**: 08（VM 启动协议完成，VM 可调度）
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

**源码**: `proc.c:299-477`

**阶段 1：进程选择**（proc.c:309-345）

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

**阶段 3：Misc 标志处理**（proc.c:351-405）

按优先级处理 5 种 misc 标志：

| 优先级 | 标志 | 操作 | 行号 |
|--------|------|------|------|
| 1 | `MF_KCALL_RESUME` | `kernel_call_resume(p)` | 356-358 |
| 2 | `MF_DELIVERMSG` | `delivermsg(p)` | 359-362 |
| 3 | `MF_SC_DEFER` | `arch_do_syscall(p)` | 363-377 |
| 4 | `MF_SC_TRACE` | `cause_sig(SIGTRAP)` | 378-393 |
| 5 | `MF_SC_ACTIVE` | 清除标志，break | 394-399 |

**循环条件**：`while (p->p_misc_flags & (MF_KCALL_RESUME | MF_DELIVERMSG | MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE))`

每次处理完一个标志后检查进程是否仍可运行，不可运行则 `goto not_runnable_pick_new`。

**阶段 4：时间片检查**（proc.c:418-424）

```c
if (!p->p_cpu_time_left)
    proc_no_time(p);
```

`proc_no_time()` 向调度服务器发送消息通知时间片用完，但不清除进程的可运行状态——调度服务器稍后会通过 `SYS_SCHEDULE` 重新设置时间片。

**阶段 5：上下文恢复**（proc.c:432-477）

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

**源码**: `proc.c:176-213`

```c
static void idle(void) {
    p = get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
    if (priv(p)->s_flags & BILLABLE)
        get_cpulocal_var(bill_ptr) = p;
    switch_address_space_idle();
    // BSP: restart_local_timer(); AP: stop_local_timer();
    halt_cpu();  // STI + HLT
}
```

**关键语义**：
- 设置当前进程为 idle 进程（用于时间统计）
- 切换到 idle 地址空间
- BSP 保持定时器运行（用于时钟中断唤醒），AP 停止定时器
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
| restore_user_context | 内联汇编 vs trait | **未抽象为 trait** | 09 早期版本曾定义 `ContextRestore`，但无实现且为死代码，已移除；恢复上下文将在完整调度循环中直接调用架构入口 |
| 地址空间切换时机 | 选进程后立即 vs 恢复上下文前 | **选进程后立即** | 与 C 一致（设计目标） |

---

## 4. 实现要点

### 4.1 当前 switch_to_user 占位实现

```rust
// os/kernel/src/lib.rs

/// Entry point for the scheduling loop.
///
/// C: switch_to_user() in proc.c
/// Design decision D7 (07 §3): returns `!` — never returns to caller.
///
/// Full implementation covered in 09-switch-to-user.md.
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
    crate::smp::bkl_unlock();

    // Placeholder — full scheduler loop implemented in 09-switch-to-user.md.
    loop { core::hint::spin_loop(); }
}
```

> 实现状态：当前 `switch_to_user()` 是占位 stub。完整调度循环（选进程、`process_misc_flags`、地址空间切换、`restore_user_context`）依赖 10/11/13 等文档的调度/IPC/异常机制完成后才能落地。

### 4.2 process_misc_flags 实现

```rust
// os/kernel/src/proc_table.rs

/// 处理进程的 misc 标志。
///
/// C: check_misc_flags 循环 — proc.c:351-405
///
/// 返回 true 表示进程仍可运行，false 表示不可运行（需重新选择）。
pub fn process_misc_flags(&mut self, nr: ProcNr) -> bool {
    let interesting_flags = MiscFlagsBits::KCALL_RESUME
        | MiscFlagsBits::DELIVERMSG
        | MiscFlagsBits::SC_DEFER
        | MiscFlagsBits::SC_TRACE
        | MiscFlagsBits::SC_ACTIVE;

    loop {
        let flags = self.get(nr).map_or(MiscFlagsBits::empty(), |p| p.p_misc_flags.get());
        if !flags.intersects(interesting_flags) { break; }

        if flags.contains(MiscFlagsBits::KCALL_RESUME) {
            // TODO: wire kernel_call_resume() from vm.rs
            self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::KCALL_RESUME));
        } else if flags.contains(MiscFlagsBits::DELIVERMSG) {
            // TODO: wire delivermsg() from ipc module
            self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG));
        } else if flags.contains(MiscFlagsBits::SC_DEFER) {
            // TODO: wire arch_do_syscall() from arch layer
            self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::SC_DEFER));
        } else if flags.contains(MiscFlagsBits::SC_TRACE) {
            if !flags.contains(MiscFlagsBits::SC_ACTIVE) { break; }
            self.get_mut(nr).map(|p| {
                p.p_misc_flags.clear(MiscFlagsBits::SC_TRACE | MiscFlagsBits::SC_ACTIVE);
            });
            // TODO: wire cause_sig() from signal module
            break;
        } else if flags.contains(MiscFlagsBits::SC_ACTIVE) {
            self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::SC_ACTIVE));
            break;
        }

        if !self.get(nr).map_or(false, |p| p.is_runnable()) {
            return false;
        }
    }
    true
}
```

> 当前 `process_misc_flags` 仅清除对应标志，尚未调用真正的 handler（`kernel_call_resume`、`delivermsg`、`arch_do_syscall`、`cause_sig`）。在 `switch_to_user` 完整实现前，这种占位行为可防止 misc 标志导致无限循环。

---

## 5. 测试

### 5.1 单元测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `test_process_misc_flags_empty_returns_true` | `proc_table.rs` | 无 misc 标志时返回 true |
| `test_process_misc_flags_clears_kcall_resume` | `proc_table.rs` | MF_KCALL_RESUME 被清除 |
| `test_process_misc_flags_clears_delivermsg` | `proc_table.rs` | MF_DELIVERMSG 被清除 |
| `test_process_misc_flags_unrunnable_returns_false` | `proc_table.rs` | misc 处理后不可运行 → 返回 false |
| `test_bsp_finish_booting_*` | `lib.rs` | `switch_to_user()` 在 bsp_finish_booting 中被调用且不返回 |

### 5.2 集成测试

| 测试 | 文件:行 | 验证内容 |
|------|--------|---------|
| `boot_simulation_full_flow` | `tests/boot_integration.rs` | 启动流程集成验证 |

> 注：`switch_to_user()` 当前为占位 stub，完整调度循环的单元/集成测试需在 10/11/13 等机制落地后补充。

---

## 6. 参见

- [08-vm-boot-protocol](08-vm-boot-protocol.md) — switch_address_space 和 VMCTL_SETADDRSPACE
- [10-scheduling-primitives](10-scheduling-primitives.md) — enqueue/dequeue/pick_proc
- [11-ipc-core](11-ipc-core.md) — delivermsg 和 IPC 投递
- [13-exception-interrupt](13-exception-interrupt.md) — 三条激活路径
