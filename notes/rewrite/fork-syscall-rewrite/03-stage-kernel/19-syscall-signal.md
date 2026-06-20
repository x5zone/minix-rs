# 19-syscall-signal: 信号系统

> **分类**: 系统调用服务
> **源码**: `minix3/minix/kernel/system/do_kill.c`, `do_getksig.c`, `do_endksig.c`, `do_sigsend.c`, `do_sigreturn.c`
> **前置**: 16（进程管理——进程状态）, 10（调度——RTS_SIGNALED/RTS_SIG_PENDING）
> **C 总行数**: ~390 行

---

## Ch1: 概念

**核心问题**: 内核如何管理信号的发送、投递和返回？

Minix3 信号系统分为两条路径：

1. **内核信号路径**（KILL/GETKSIG/ENDKSIG）：内核内部产生信号，信号管理器（通常是 PM）通过轮询获取
2. **POSIX 信号路径**（SIGSEND/SIGRETURN）：信号管理器直接操作进程的栈帧来安装信号处理器

### 1.1 内核信号路径

```
cause_sig(proc_nr, sig_nr)     ← 内核内部调用（如时钟、异常）
  → p_pending |= sig_mask(sig_nr)
  → RTS_SET(SIGNALED)
  → 通知信号管理器

SYS_GETKSIG                   ← 信号管理器轮询
  → 扫描所有进程，找 RTS_SIGNALED 且 s_sig_mgr == caller 的
  → 返回 endpoint + pending 位图
  → RTS_UNSET(SIGNALED), RTS_SET(SIG_PENDING)

SYS_ENDKSIG                   ← 信号管理器处理完毕
  → RTS_UNSET(SIG_PENDING)    ← 如果没有新的 RTS_SIGNALED
```

**关键语义**：
- `RTS_SIGNALED`：进程有未处理的内核信号
- `RTS_SIG_PENDING`：信号管理器正在处理信号，进程不可调度
- `p_pending`：挂起信号位图（`sigset_t`）
- `s_sig_mgr`：每个进程的信号管理器 endpoint（存储在 `KPriv` 中，通过 `ProcessTable::sig_mgr()` 访问）

### 1.2 POSIX 信号路径

```
SYS_SIGSEND                   ← 信号管理器安装信号处理器
  → 从用户空间拷贝 sigmsg 结构
  → 在用户栈上构建 sigframe_sigcontext
  → 保存当前寄存器到 sigcontext
  → 修改进程寄存器：SP→sigframe, PC→sighandler
  → 清除 MF_FPU_INITIALIZED

SYS_SIGRETURN                 ← 信号处理器返回
  → 从用户栈拷贝 sigcontext
  → 恢复寄存器（保留系统标志位）
  → 恢复 FPU 状态
```

**SIGSEND 的关键约束**：
- 拷贝 sigframe 到用户栈可能触发 VMSUSPEND
- 在拷贝完成之前，**不能修改进程寄存器**（否则 VMSUSPEND 恢复后会重复修改）
- 因此寄存器修改必须在最后一次 data_copy_vmcheck 之后

### 1.3 cause_sig()

`cause_sig()` 不是系统调用，而是内核内部函数，被时钟中断、异常处理等调用：

1. 设置 `p_pending |= sig_mask(sig_nr)`
2. 设置 `RTS_SIGNALED`
3. 如果信号管理器正在 RECEIVE 等待，则唤醒它

---

## Ch2: C 源码分析

### do_kill.c (41 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 22-41 | `do_kill()` | 验证 endpoint + sig_nr → cause_sig |

### do_getksig.c (43 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-43 | `do_getksig()` | 扫描 BEG_USER_ADDR→END_PROC_ADDR，找 RTS_SIGNALED 且 s_sig_mgr == caller |

### do_endksig.c (41 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 19-41 | `do_endksig()` | 验证 s_sig_mgr → 检查 RTS_SIG_PENDING → 如无新信号则清除 |

### do_sigsend.c (166 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 25-166 | `do_sigsend()` | 拷贝 sigmsg → 构建 sigframe → 拷贝到用户栈 → 修改寄存器 |
| 49-55 | sigmsg 拷贝 | data_copy_vmcheck |
| 57-58 | 计算用户栈指针 | arch_get_sp |
| 60-100 | 构建 sigcontext | 保存寄存器（x86/arm 分别处理） |
| 120-125 | 拷贝 sigframe 到用户栈 | data_copy_vmcheck（可能 VMSUSPEND） |
| 130-145 | 修改进程寄存器 | **必须在拷贝之后** |

### do_sigreturn.c (98 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-98 | `do_sigreturn()` | 拷贝 sigcontext → 恢复寄存器 → 恢复 FPU |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 信号位图 | `u64` vs `sigset_t` | **`u64`** | Minix3 `_NSIG = 64`，一个 u64 刚好 |
| D2 | sigcontext 保存/恢复 | 直接操作寄存器 vs trait 抽象 | **`trait SignalContext`** | 多架构支持（x86/arm 寄存器完全不同） |
| D3 | SIGSEND 寄存器修改时序 | 保留 C 的"拷贝后修改"约束 | **保留** | VMSUSPEND 语义要求 |
| D4 | cause_sig | 全局函数 vs KProcess 方法 | **KProcess 方法** | 封装 p_pending + RTS 操作 |
| D5 | GETKSIG 进程扫描 | 线性扫描 vs 信号队列 | **线性扫描** | 与 C 一致，进程数少（<128） |
| D6 | SIGSEND/SIGRETURN 架构相关代码 | `#[cfg(target_arch)]` vs trait | **trait** | 遵循硬件抽象原则 |

---

## Ch4: 实现要点

### 4.1 核心类型

```rust
/// 信号编号。C: `_NSIG = 64`
pub const NSIG: usize = 64;

/// 信号位图。C: `sigset_t` = `u64`（64 个信号）
pub type SigSet = u64;

impl KProcess {
    /// 向进程发送信号。C: `cause_sig(proc_nr, sig_nr)`
    pub fn cause_signal(&mut self, sig_nr: u32) {
        if sig_nr as usize >= NSIG { return; }
        self.p_pending |= 1u64 << (sig_nr - 1);
        self.p_rts_flags.set(RtsFlagsBits::SIGNALED);
    }
}
```

### 4.2 trait SignalContext

```rust
/// 信号上下文操作（架构相关）。
pub trait SignalContext {
    /// 保存当前寄存器到 sigcontext 并构建 sigframe。
    fn build_sigframe(proc: &mut KProcess, smsg: &SigMsg) -> Result<SigFrame, i32>;
    /// 从 sigcontext 恢复寄存器。
    fn restore_sigcontext(proc: &mut KProcess, sc: &SigContext) -> Result<(), i32>;
}
```

### 4.3 SIGSEND 时序约束

```rust
// 1. 拷贝 sigmsg（可能 VMSUSPEND）
let smsg = data_copy_vmcheck(...)?;
// 2. 构建 sigframe
let frame = build_sigframe(proc, &smsg);
// 3. 拷贝 sigframe 到用户栈（可能 VMSUSPEND）
data_copy_vmcheck(..., &frame, ...)?;
// 4. 修改寄存器（必须在此之后！）
proc.p_reg.sp = frame_pointer;
proc.p_reg.pc = sighandler;
```

---

## 测试

- 单元：cause_signal 设置 p_pending 和 RTS_SIGNALED
- 单元：getksig 扫描找到 RTS_SIGNALED 进程
- 单元：endksig 清除 RTS_SIG_PENDING（无新信号时）
- 单元：endksig 保留 RTS_SIG_PENDING（有新信号时）
- 单元：sig_nr >= _NSIG 返回 EINVAL
- 单元：kernel endpoint 返回 EPERM

---

## 补充：信号处理详细分析

> 来源：tmp-15-syscall-exit-signal.md

### 信号管理器（signal manager）

每个进程有一个关联的信号管理器（`s_sig_mgr`），负责处理该进程的信号。用户进程的信号管理器是 PM，系统进程可以有自定义的信号管理器。当信号产生时，内核通知信号管理器，由信号管理器决定如何处理。

> **注意**：`s_sig_mgr` 存储在 `KPriv` 结构体中（C: `priv(rp)->s_sig_mgr`），而非 `KProcess`。
> Rust 实现中通过 `ProcessTable::sig_mgr(nr, &priv_table)` 便捷方法访问，该方法
> 自动处理 `SELF` → `p_endpoint` 的替换逻辑（C: system.c:399-400）。

### 致命信号的备份管理器

若进程是自己的信号管理器且收到致命信号，内核尝试将信号转发给备份信号管理器（`s_bak_sig_mgr`）。若无备份管理器，内核 panic。

### 信号相关消息字段

| 字段宏 | 含义 |
|--------|------|
| `m_sigcalls.endpt` | 目标进程 endpoint |
| `m_sigcalls.sig` | 信号编号 |
| `m_sigcalls.map` | 待处理信号位图 |
| `m_sigcalls.sigctx` | sigcontext 结构指针 |

### 信号相关特殊常量

| 常量 | 含义 |
|------|------|
| `SIGKSIG` | 内核通知信号管理器有新信号 |
| `SIGKSIGSM` | 信号管理器自身的信号通知 |
| `SIGSNDELAY` | 停止延迟结束信号 |
| `SC_MAGIC` | sigcontext 结构的魔数（校验完整性） |

### 信号处理帧（struct sigframe_sigcontext）

`sys_sigsend` 在目标进程用户栈上构建的帧结构：

| 字段 | 含义 |
|------|------|
| `sf_sc` | `struct sigcontext`——保存的寄存器上下文 |
| `sf_scp` | 指向 `sf_sc` 的指针 |
| `sf_fp` | 帧指针 |
| `sf_signum` | 信号编号 |
| `sf_ra` | 返回地址（原始 PC） |
| `sf_ra_sigreturn` | sigreturn 库函数地址 |

### 信号处理行为规则

1. **sys_exit 不回复**：`do_exit()` 返回 `EDONTREPLY`
2. **sys_exit 发送 SIGABRT**：系统进程退出通过向自身发送 SIGABRT 实现
3. **信号不可发给内核任务**：`do_kill()` 拒绝向内核任务发送信号
4. **信号去重**：`cause_sig()` 检查信号是否已在 `p_pending` 中，避免重复通知
5. **sys_sigsend 的幂等性**：信号处理帧复制可能因页缺失失败（VMSUSPEND），代码设计为可安全重入
6. **sys_sigreturn 验证 sc_magic**：恢复上下文前检查 `SC_MAGIC`，防止损坏的信号上下文
7. **sys_clear 释放全部资源**：清理地址空间、IRQ 钩子、端点、定时器、FPU、特权结构

### 信号处理函数列表

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 系统进程退出 | `do_exit()` | system/do_exit.c |
| 发送信号 | `do_kill()` | system/do_kill.c |
| 信号分发核心 | `cause_sig()` | system.c:389 |
| 信号延迟完成 | `sig_delay_done()` | system.c:454 |
| 获取待处理信号 | `do_getksig()` | system/do_getksig.c |
| 结束信号处理 | `do_endksig()` | system/do_endksig.c |
| 推送信号处理帧 | `do_sigsend()` | system/do_sigsend.c |
| 信号处理返回 | `do_sigreturn()` | system/do_sigreturn.c |
| 清理进程槽位 | `do_clear()` | system/do_clear.c |

---

## 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — RTS_SIGNALED/RTS_SIG_PENDING 定义
- [14-exception-interrupt.md](14-exception-interrupt.md) — exception_handler 调用 cause_sig
- [15-clock-timer.md](15-clock-timer.md) — vtimer_check 发送 SIGVTALRM/SIGPROF
