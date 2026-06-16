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
| goto 模拟 | loop + continue/break vs 状态机 | **loop + continue/break** | C 的 goto 在此函数中形成 2 个跳转目标，loop 更清晰 |
| proc_ptr | 全局变量 vs CpuLocal | **CpuLocal\<Option\<ProcNr\>\>** | SMP 安全，每 CPU 独立 |
| idle | 内联 vs 独立函数 | **独立方法** | 与 C 一致，且 idle 有独立的地址空间切换逻辑 |
| Misc 标志循环 | while + if-else chain vs match | **while + match** | Rust match 穷尽性检查，但需要处理优先级（按位检查） |
| restore_user_context | 内联汇编 vs trait | **trait ContextRestore** | 多架构支持，x86 iret vs aarch64 eret |
| 地址空间切换时机 | 选进程后立即 vs 恢复上下文前 | **选进程后立即** | 与 C 一致，delivermsg 需要正确地址空间 |

---

## 4. 实现要点

### 4.1 SwitchToUserFlow 枚举

```rust
/// switch_to_user 的控制流状态。
///
/// C 使用 goto 在多个检查点之间跳转。
/// Rust 使用枚举状态 + loop 模拟相同控制流。
enum SwitchFlow {
    /// 检查当前进程是否可运行
    CheckCurrent,
    /// 当前进程不可运行，选择新进程
    PickNew,
    /// 处理 misc 标志
    CheckMiscFlags,
    /// 检查时间片
    CheckQuantum,
    /// 恢复用户态上下文
    RestoreContext,
}
```

### 4.2 trait ContextRestore

```rust
/// 用户态上下文恢复抽象。
/// C: restore_user_context() — 各架构汇编实现
pub trait ContextRestore {
    /// 恢复进程的用户态上下文并切换到用户态。
    /// 此函数不返回（! 类型）。
    ///
    /// C: restore_user_context(p) — mpx.S (x86), mpx.S (ARM)
    fn restore(proc: &KProcess) -> !;
}
```

### 4.3 switch_to_user 主体

```rust
/// 调度循环入口：选择下一个可运行进程并切换到用户态。
///
/// C: switch_to_user() — proc.c:299-477
///
/// 此函数不返回。它恢复用户态上下文后直接切换到用户态执行。
/// 下一次内核入口（中断/异常/系统调用）会重新调用此函数。
pub fn switch_to_user<C: ContextRestore, P: PageTableSwitcher>(
    cpu_local: &mut CpuLocal,
    proc_table: &mut ProcessTable,
    pt_switcher: &P,
) -> ! {
    let mut state = SwitchFlow::CheckCurrent;
    loop {
        match state {
            SwitchFlow::CheckCurrent => { /* ... */ }
            SwitchFlow::PickNew => { /* ... */ }
            SwitchFlow::CheckMiscFlags => { /* ... */ }
            SwitchFlow::CheckQuantum => { /* ... */ }
            SwitchFlow::RestoreContext => {
                return C::restore(proc);
            }
        }
    }
}
```

### 4.4 Misc 标志处理

```rust
/// 处理进程的 misc 标志。
///
/// C: check_misc_flags 循环 — proc.c:351-405
fn process_misc_flags(
    proc: &mut KProcess,
    proc_table: &mut ProcessTable,
) -> bool {
    // 返回 true 表示进程仍可运行，false 表示不可运行
    loop {
        let flags = proc.p_misc_flags;
        let interesting = flags.intersects(
            MiscFlagsBits::KCALL_RESUME
            | MiscFlagsBits::DELIVERMSG
            | MiscFlagsBits::SC_DEFER
            | MiscFlagsBits::SC_TRACE
            | MiscFlagsBits::SC_ACTIVE
        );
        if !interesting { break; }

        if flags.contains(MiscFlagsBits::KCALL_RESUME) {
            kernel_call_resume(proc);
        } else if flags.contains(MiscFlagsBits::DELIVERMSG) {
            delivermsg(proc);
        } else if flags.contains(MiscFlagsBits::SC_DEFER) {
            arch_do_syscall(proc);
        } else if flags.contains(MiscFlagsBits::SC_TRACE) {
            if !flags.contains(MiscFlagsBits::SC_ACTIVE) { break; }
            cause_sig(proc.p_nr, Signal::SIGTRAP);
        } else if flags.contains(MiscFlagsBits::SC_ACTIVE) {
            proc.p_misc_flags.clear(MiscFlagsBits::SC_ACTIVE);
            break;
        }

        if !proc.is_runnable() { return false; }
    }
    true
}
```

---

## 5. 测试

### 5.1 单元测试

| 测试 | 验证内容 |
|------|---------|
| `test_switch_flow_current_runnable` | 当前进程可运行时跳过 pick_proc |
| `test_switch_flow_preempted_enqueue_head` | PREEMPTED + 有时间片 → enqueue_head |
| `test_switch_flow_preempted_enqueue_tail` | PREEMPTED + 无时间片 → enqueue |
| `test_switch_flow_idle_loop` | 无可运行进程时进入 idle |
| `test_misc_flags_kcall_resume` | MF_KCALL_RESUME 调用 kernel_call_resume |
| `test_misc_flags_delivermsg` | MF_DELIVERMSG 调用 delivermsg |
| `test_misc_flags_sc_defer` | MF_SC_DEFER 执行延迟系统调用 |
| `test_misc_flags_unrunnable` | misc 处理后不可运行 → 重新选择 |
| `test_quantum_check` | 无时间片时调用 proc_no_time |
| `test_context_set_cleared` | 恢复前清除 MF_CONTEXT_SET |

### 5.2 集成测试

| 测试 | 验证内容 |
|------|---------|
| `test_full_switch_to_user_cycle` | 完整的调度循环周期 |

---

## 6. 参见

- [08-vm-boot-protocol](08-vm-boot-protocol.md) — switch_address_space 和 VMCTL_SETADDRSPACE
- [10-scheduling-primitives](10-scheduling-primitives.md) — enqueue/dequeue/pick_proc
- [11-ipc-core](11-ipc-core.md) — delivermsg 和 IPC 投递
- [13-exception-interrupt](13-exception-interrupt.md) — 三条激活路径
