# 30-kernel-profile: 内核统计 profiling

> **分类**: 性能分析基础设施
> **源码**: `minix3/minix/kernel/profile.c` (157 行，全部 `#if SPROFILE`)
> **关联 Rust**: `os/kernel/src/misc.rs`（sample collection + dispatch_profile）, `os/kernel/src/clock.rs`（init/stop/ack_profile_clock 接口）, `os/arch/src/x86_64/clock.rs`（x86_64 impl）, `os/arch/src/arm64/clock.rs`（aarch64 impl）, `os/arch/src/riscv64/clock.rs`（riscv64 impl）
> **前置**: [15-clock-timer.md](15-clock-timer.md), [25-misc-unported.md](25-misc-unported.md), [26-watchdog.md](26-watchdog.md)
> **C 总行数**: 157 行

---

## Ch1: 概念

**核心问题**: 内核运行时，CPU 时间花在哪些进程上？哪些代码路径是热点？用户态 profiling 工具（如 `perf`）可以采样用户态执行，但内核态执行需要内核自身参与采样。内核如何采样自身执行，为性能优化提供数据？

这是内核自省的元问题。CPU 提供了时钟中断和性能计数器，内核利用这些硬件机制周期性记录"当前正在执行什么"，从而统计出 CPU 时间分布。

### 1.1 统计 profiling 的机制

统计 profiling 基于采样：不记录每条指令的执行，而是周期性采样"当前状态"，用统计方法推断整体分布。

**四个核心组件**:

| 组件 | C 实现 | 用途 |
|------|--------|------|
| 采样时钟 | `init_profile_clock(freq)` | 独立于调度时钟的专用时钟，按指定频率触发中断 |
| 采样 handler | `profile_clock_handler` | 中断时记录当前进程 + PC，存入 buffer |
| 分类统计 | `profile_sample` | 区分 idle / system / user 样本 |
| NMI profiling | `nmi_sprofile_handler` | 用不可屏蔽中断采样，即使内核关中断也能触发 |

**采样分类逻辑**（minix3/minix/kernel/profile.c:75-110）：
- `IDLE` 进程 → `sprof_info.idle_samples++`（CPU 空闲）
- `KERNEL` 或 `SYS_PROC` runnable → `sprof_info.system_samples++`（内核/系统进程）
- 其他 → `sprof_info.user_samples++`（用户进程）
- 每次都 `sprof_info.total_samples++`

### 1.2 SPROFILE 条件编译

整个 minix3/minix/kernel/profile.c 用 `#if SPROFILE` 包裹。`SPROFILE` 宏未启用时，文件编译为空——这是 Minix3 的特性开关机制，让 profiling 代码完全从生产构建中移除。

### 1.3 与 25-misc-unported.md 的关系

- [25-misc-unported.md](25-misc-unported.md) 文档化 `do_sprofile` 系统调用（用户态接口）——用户态通过 `SYS_SPROF` 启动/停止 profiling
- 本文档化 `profile.c` 内核侧（采样收集）——内核如何在 profiling 启用时收集样本
- 两者是"接口/实现"配对关系

### 1.4 redox 对照

- **redox**: 无内核 profiling——外部工具（`perf`）通过硬件性能计数器采样，内核不参与
- **Minix3**: 内核内 profiling + SPROFILE 条件编译——内核主动采样，数据存入内核 buffer
- **minix-rs**: trait 接口（`ClockArch::init_profile_clock` / `stop_profile_clock` / `ack_profile_clock`）+ 样本收集已实现（`profile_sample` / `sprof_save_sample` / `sprof_save_proc` / `profile_clock_handler`），使用 `static mut` buffer（BKL 保护）

### 1.5 本章不讲什么

- `do_sprofile` 系统调用（见 [25-misc-unported.md](25-misc-unported.md) §2.5）
- NMI watchdog 机制（见 [26-watchdog.md](26-watchdog.md)）
- 调度时钟机制（见 [15-clock-timer.md](15-clock-timer.md)）

---

## Ch2: C 源码分析

### 2.1 文件清单

| 文件 | 行数 | 核心内容 |
|------|------|---------|
| minix3/minix/kernel/profile.c | 157 | 7 函数 + 1 全局数组，全部 `#if SPROFILE` |

### 2.2 全局状态

```c
char sprof_sample_buffer[SAMPLE_BUFFER_SIZE];    // 采样缓冲区
static irq_hook_t profile_clock_hook;             // IRQ hook
```

`sprof_info`（在别处定义）包含统计字段：
- `mem_used`: buffer 已用字节数（-1 表示满）
- `idle_samples` / `system_samples` / `user_samples` / `total_samples`: 分类计数
- `sprof_mem_size`: buffer 总大小

### 2.3 时钟初始化与停止

**`init_profile_clock(freq)`**（minix3/minix/kernel/profile.c:27-37）：
1. 调用 `arch_init_profile_clock(freq)` 初始化架构专用时钟
2. 若返回 IRQ 号 ≥ 0，注册 `profile_clock_handler` 为 IRQ handler
3. `enable_irq` 启用中断

**`stop_profile_clock()`**（minix3/minix/kernel/profile.c:42-49）：
1. 调用 `arch_stop_profile_clock()` 停止架构专用时钟
2. `disable_irq` + `rm_irq_handler` 注销 handler

### 2.4 样本收集

**`sprof_save_sample(p, pc)`**（minix3/minix/kernel/profile.c:51-61）：
```c
struct sprof_sample *s = (struct sprof_sample *)(sprof_sample_buffer + sprof_info.mem_used);
s->proc = p->p_endpoint;
s->pc = pc;
sprof_info.mem_used += sizeof(struct sprof_sample);
```
将 endpoint + PC 写入 buffer，前进 `mem_used` 指针。

**`sprof_save_proc(p)`**（minix3/minix/kernel/profile.c:63-73）：
```c
struct sprof_proc *s = (struct sprof_proc *)(sprof_sample_buffer + sprof_info.mem_used);
s->proc = p->p_endpoint;
strcpy(s->name, p->p_name);
sprof_info.mem_used += sizeof(struct sprof_proc);
```
首次见到某进程时保存 endpoint + name（用于后续解析）。

### 2.5 主采样逻辑

**`profile_sample(p, pc)`**（minix3/minix/kernel/profile.c:75-110）：

```c
// 未启用或 buffer 满 → 返回
if (!sprofiling || sprof_info.mem_used == -1) return;

// buffer 空间不足 → 标记满
if (sprof_info.mem_used + ... > sprof_mem_size) {
    sprof_info.mem_used = -1;
    return;
}

// 分类采样
if (p->p_endpoint == IDLE)
    sprof_info.idle_samples++;
else if (p->p_endpoint == KERNEL || (priv(p)->s_flags & SYS_PROC && proc_is_runnable(p))) {
    // 首次见到的系统进程 → 保存进程信息
    if (!(p->p_misc_flags & MF_SPROF_SEEN)) {
        p->p_misc_flags |= MF_SPROF_SEEN;
        sprof_save_proc(p);
    }
    sprof_save_sample(p, pc);
    sprof_info.system_samples++;
} else {
    sprof_info.user_samples++;
}
sprof_info.total_samples++;
```

**关键设计**:
- `MF_SPROF_SEEN` 标志避免重复保存同一进程信息
- 只对 system 进程保存 PC 样本（user 进程只计数）
- buffer 满后设置 `mem_used = -1` 停止采样

### 2.6 中断 handler

**`profile_clock_handler(hook)`**（minix3/minix/kernel/profile.c:115-126）：
```c
struct proc *p = get_cpulocal_var(proc_ptr);   // 当前进程
profile_sample(p, (void *)p->p_reg.pc);         // 采样
arch_ack_profile_clock();                        // ACK 中断
return 1;                                        // 重新启用中断
```

### 2.7 NMI profiling handler

**`nmi_sprofile_handler(frame)`**（minix3/minix/kernel/profile.c:128-155）：

NMI 版本与时钟版本的区别：
- NMI 即使在内核关中断时也能触发
- 检查 `nmi_in_kernel(frame)` 区分内核态/用户态中断
- 内核态中断时，若 IDLE 调度则记 idle，否则采样 KERNEL 进程
- 用户态中断时，采样当前进程

> **关联**: NMI 机制详见 [26-watchdog.md](26-watchdog.md)。

---

## Ch3: 设计决策

### 3.1 D1: profile 时钟接口保留（trait 抽象）

**C 行为**: `init_profile_clock(freq)` + `stop_profile_clock()` + `arch_init_profile_clock` / `arch_stop_profile_clock`。

**Rust 64-bit 决策**: `ClockArch` trait 保留 `init_profile_clock` / `stop_profile_clock` 方法。

**已实现**:
- os/kernel/src/clock.rs:293: `pub fn init_profile_clock(hz: u32) -> Result<(), minix_arch::clock::ProfileClockError>`
- os/kernel/src/clock.rs:315: `pub fn stop_profile_clock()`
- os/arch/src/x86_64/clock.rs:114: `fn init_profile_clock(&mut self, hz: u32) -> Result<(), ProfileClockError>`
- os/arch/src/x86_64/clock.rs:165: `fn stop_profile_clock(&mut self)`
- os/arch/src/arm64/clock.rs:89: aarch64 impl

**理由**: 接口轻量，trait 抽象符合 HW 抽象原则；保留接口为未来实现预留；`do_sprofile` 系统调用已调用此接口（见 os/kernel/src/misc.rs:1957）。

### 3.2 D2: 样本收集实现

**C 行为**: `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler` 实现采样收集。

**Rust 64-bit 决策**: 实现样本收集，使用 `static mut` buffer + BKL 保护。

**已实现**:
- os/kernel/src/misc.rs: `SprofSample` / `SprofProc` 结构体 + `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler` / `is_sys_proc_runnable`
- `profile_sample(proc, pc, priv_table)` 接收 `&KProcess` + PC + `&PrivTable`，分类为 idle/system/user
- `profile_clock_handler(proc, pc, priv_table)` 调用 `profile_sample` 后 `ack_profile_clock()`
- 使用 `addr_of_mut!` 避免 Rust 2024 `static_mut_refs` 问题

**设计选择**:
1. `static mut` buffer（BKL 保护）而非堆分配——`no_std` 内核中断上下文不能堆分配
2. PC 作为参数传入（非从 `p_reg` 读取）——Rust trap frame 在栈上，不在进程结构体中
3. `PrivTable` 作为参数传入——避免全局可变状态访问
4. `profile_clock_handler` 是公开函数，trap entry path 检测到 profile clock IRQ 时直接调用

### 3.3 D3: NMI profiling 不实现（WONTFIX）

**ARCH: WONTFIX** — 见 [26-watchdog.md](26-watchdog.md)

**C 行为**: `nmi_sprofile_handler` 用 NMI 采样。

**Rust 64-bit 决策**: 不实现。

**理由**: 64-bit 无 NMI 子系统（见 [26-watchdog.md](26-watchdog.md) §1.2）。

### 3.4 D4: `sprof_sample_buffer` 保留（缩减大小）

**C 行为**: `char sprof_sample_buffer[SAMPLE_BUFFER_SIZE]` 全局数组（64 MB）。

**Rust 64-bit 决策**: 保留为 `static mut SPROF_SAMPLE_BUFFER: [u8; 256 * 1024]`。

**理由**: 256 KB 足够典型 profiling 会话（~10K samples + proc records）；64 MB BSS 在 `no_std` 内核中过大。使用 `static mut` + BKL 保护（中断上下文不能堆分配）。`dispatch_profile` (PROF_STOP) 通过 `data_copy_vmcheck` 将 buffer 内容拷贝到用户空间。

---

## Ch4: Rust 实现

### 4.1 profile 时钟接口

os/kernel/src/clock.rs：

```rust
/// C: `init_profile_clock()` — arch/i386/arch_clock.c
pub fn init_profile_clock(hz: u32) -> Result<(), minix_arch::clock::ProfileClockError> {
    // ...
    clock_arch.init_profile_clock(hz)
}

/// C: `stop_profile_clock()` — arch/i386/arch_clock.c
pub fn stop_profile_clock() {
    // ...
    clock_arch.stop_profile_clock();
}

/// C: `arch_ack_profile_clock()` — profile.c:123
pub fn ack_profile_clock() {
    // ...
    clock_arch.ack_profile_clock();
}
```

x86_64 impl（os/arch/src/x86_64/clock.rs）配置 RTC 定时器 + 读 Register C ack。

aarch64 impl（os/arch/src/arm64/clock.rs）返回 `Err(ProfileClockError::Unsupported)`（PMU 未集成）。

riscv64 impl（os/arch/src/riscv64/clock.rs）返回 `Err(ProfileClockError::Unsupported)`（无独立 profiling 定时器）。

### 4.2 样本收集

os/kernel/src/misc.rs：

```rust
/// C: `struct sprof_sample` — include/minix/profile.h:27-30
#[repr(C)]
pub struct SprofSample { pub proc: i32, pub pc: u64 }

/// C: `struct sprof_proc` — include/minix/profile.h:32-35
#[repr(C)]
pub struct SprofProc { pub proc: i32, pub name: [u8; PROC_NAME_LEN] }

/// C: `sprof_save_sample()` — profile.c:51-61
unsafe fn sprof_save_sample(endpoint: i32, pc: u64) { ... }

/// C: `sprof_save_proc()` — profile.c:63-73
unsafe fn sprof_save_proc(proc: &KProcess) { ... }

/// C: `profile_sample()` — profile.c:75-110
pub unsafe fn profile_sample(proc: &KProcess, pc: u64, priv_table: &PrivTable) { ... }

/// C: `profile_clock_handler()` — profile.c:115-126
pub unsafe fn profile_clock_handler(proc: &KProcess, pc: u64, priv_table: &PrivTable) {
    profile_sample(proc, pc, priv_table);
    crate::clock::ack_profile_clock();
}
```

#### C 语义对齐

| C 代码 | Rust 实现 | 备注 |
|--------|-----------|------|
| `p->p_reg.pc` | `pc: u64` 参数 | Rust trap frame 在栈上，PC 由 trap entry 传入 |
| `priv(p)->s_flags & SYS_PROC` | `priv_table.get(priv_id).is_sys_proc()` | `PrivTable` 作为参数传入 |
| `proc_is_runnable(p)` | `proc.is_runnable()` | KProcess 方法 |
| `p->p_misc_flags \|= MF_SPROF_SEEN` | `proc.p_misc_flags.set(MiscFlagsBits::SPROF_SEEN)` | 原子操作 |
| `sprof_info.mem_used == -1` | `SPROF_INFO.mem_used == -1` | buffer 满标记 |
| `sprof_sample_buffer` | `SPROF_SAMPLE_BUFFER` (256 KB) | 缩减大小，BKL 保护 |

#### C 代码 typo 对齐

C 源码 profile.c:84-86 空间检查中，`2*sizeof(struct sprof_sample)` 出现两次（第二次应为 `2*sizeof(struct sprof_proc)`）。Rust 实现复制了 C 的精确检查以保持语义对齐，并在注释中标注了 C 的 typo。

### 4.3 do_sprofile 调用

os/kernel/src/misc.rs 的 `dispatch_profile` 调用 `init_profile_clock` / `stop_profile_clock`，PROF_STOP 时通过 `data_copy_vmcheck` 将 `SPROF_INFO` + `SPROF_SAMPLE_BUFFER` 拷贝到用户空间。

### 4.4 WONTFIX

| C 符号 | C 位置 | 状态 | 理由 |
|--------|--------|------|------|
| `nmi_sprofile_handler` | profile.c:128 | ❌ WONTFIX | 64-bit 无 NMI 子系统（见 26） |

---

## Ch5: 测试

### 5.1 profile 时钟接口测试

`init_profile_clock` / `stop_profile_clock` / `ack_profile_clock` 接口已由 `dispatch_profile` 测试间接覆盖（见 [25-misc-unported.md](25-misc-unported.md) §5）。

### 5.2 样本收集测试

os/kernel/src/misc.rs tests 模块中 8 个测试覆盖 `profile_sample` 全部分类路径：

| 测试名 | 覆盖路径 | C 对齐 |
|--------|---------|--------|
| `test_profile_sample_noop_when_not_profiling` | `!sprofiling` → 早返回 | profile.c:80 |
| `test_profile_sample_noop_when_buffer_full` | `mem_used == -1` → 早返回 | profile.c:81 |
| `test_profile_sample_idle_increments_idle_samples` | IDLE endpoint → idle_samples++ | profile.c:93 |
| `test_profile_sample_kernel_endpoint_saves_system_sample` | KERNEL endpoint → system sample + proc record | profile.c:94 |
| `test_profile_sample_user_process_increments_user_samples` | 非 SYS_PROC → user_samples++ | profile.c:106 |
| `test_profile_sample_runnable_sys_proc_saves_sample_and_proc` | SYS_PROC + runnable → system sample + proc record | profile.c:95 |
| `test_profile_sample_second_sample_does_not_resave_proc` | MF_SPROF_SEEN gate → 第二次只写 sample | profile.c:97-100 |
| `test_profile_sample_buffer_full_marks_mem_used_minus1` | 空间不足 → mem_used = -1 | profile.c:84-89 |

---

## Ch6: 跨文档引用

### 6.1 前序引用

- [15-clock-timer.md](15-clock-timer.md): `ClockArch` trait 定义（profile 时钟接口所在）
- [25-misc-unported.md](25-misc-unported.md) §2.5: `do_sprofile` 系统调用（用户态接口）
- [26-watchdog.md](26-watchdog.md): NMI 机制 + `nmi_sprofile_handler`

