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

**采样分类逻辑**（minix3/minix/kernel/profile.c:profile_sample）：
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

**`init_profile_clock(freq)`**（minix3/minix/kernel/profile.c:init_profile_clock）：
1. 调用 `arch_init_profile_clock(freq)` 初始化架构专用时钟
2. 若返回 IRQ 号 ≥ 0，注册 `profile_clock_handler` 为 IRQ handler
3. `enable_irq` 启用中断

**`stop_profile_clock()`**（minix3/minix/kernel/profile.c:stop_profile_clock）：
1. 调用 `arch_stop_profile_clock()` 停止架构专用时钟
2. `disable_irq` + `rm_irq_handler` 注销 handler

### 2.4 样本收集

**`sprof_save_sample(p, pc)`**（minix3/minix/kernel/profile.c:sprof_save_sample）：
```c
struct sprof_sample *s = (struct sprof_sample *)(sprof_sample_buffer + sprof_info.mem_used);
s->proc = p->p_endpoint;
s->pc = pc;
sprof_info.mem_used += sizeof(struct sprof_sample);
```
将 endpoint + PC 写入 buffer，前进 `mem_used` 指针。

**`sprof_save_proc(p)`**（minix3/minix/kernel/profile.c:sprof_save_proc）：
```c
struct sprof_proc *s = (struct sprof_proc *)(sprof_sample_buffer + sprof_info.mem_used);
s->proc = p->p_endpoint;
strcpy(s->name, p->p_name);
sprof_info.mem_used += sizeof(struct sprof_proc);
```
首次见到某进程时保存 endpoint + name（用于后续解析）。

### 2.5 主采样逻辑

**`profile_sample(p, pc)`**（minix3/minix/kernel/profile.c:profile_sample）：

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

**`profile_clock_handler(hook)`**（minix3/minix/kernel/profile.c:profile_clock_handler）：
```c
struct proc *p = get_cpulocal_var(proc_ptr);   // 当前进程
profile_sample(p, (void *)p->p_reg.pc);         // 采样
arch_ack_profile_clock();                        // ACK 中断
return 1;                                        // 重新启用中断
```

### 2.7 NMI profiling handler

**`nmi_sprofile_handler(frame)`**（minix3/minix/kernel/profile.c:nmi_sprofile_handler）：

NMI 版本与时钟版本的区别：
- NMI 即使在内核关中断时也能触发
- 检查 `nmi_in_kernel(frame)` 区分内核态/用户态中断
- 内核态中断时，若 IDLE 调度则记 idle，否则采样 KERNEL 进程
- 用户态中断时，采样当前进程

> **关联**: NMI 机制详见 [26-watchdog.md](26-watchdog.md)。

---

## Ch3: 设计决策

### 3.1 D1: profile 时钟接口与中断接线（trait 抽象 + hook 链）

**C 行为**: `init_profile_clock(freq)` + `stop_profile_clock()` + `arch_init_profile_clock` / `arch_stop_profile_clock`；IRQ 号拿到后 `put_irq_handler` 挂采样 handler，停止时摘除。

**Rust 64-bit 决策**: `ClockArch` trait 保留 `init_profile_clock` / `stop_profile_clock` 方法承担编程半边；hook 的挂载与摘除在 kernel 层补齐，落在 `IrqManager` 的 hook 链上。

**已实现**:
- os/kernel/src/clock.rs:fn init_profile_clock: arch 编程成功后调 `register_profile_hook`
- os/kernel/src/clock.rs:fn stop_profile_clock: 先停 arch 时钟源再 `unregister_profile_hook`
- os/kernel/src/clock.rs:fn register_profile_hook / fn unregister_profile_hook: hook 生命周期（ID 存 `PROFILE_HOOK_ID`）
- os/plat/src/x86_64/interrupt.rs:const PROFILE_CLOCK_IRQ: `IrqVector::new(8)`（RTC → IOAPIC 输入 8）
- os/plat/src/arm64/interrupt.rs:const PROFILE_CLOCK_IRQ 与 os/plat/src/riscv64/interrupt.rs:const PROFILE_CLOCK_IRQ: 伪向量 0——两架构 arch 实现返回 Unsupported，该身份永远不会被注册或投递
- os/arch/src/x86_64/clock.rs:fn init_profile_clock / fn stop_profile_clock / fn ack_profile_clock: RTC 编程与 register C 应答
- os/arch/src/arm64/clock.rs:fn init_profile_clock 与 os/arch/src/riscv64/clock.rs:fn init_profile_clock: 返回 `Err(ProfileClockError::Unsupported)`

**理由**: 接口轻量，trait 抽象符合 HW 抽象原则。编程（arch）与注册（kernel hook 链）的分离保持了 C 的分层——C 也是 arch 出 IRQ 号、profile.c 出 hook；hook 链复用 `IrqManager` 现成的 mask/unmask/EOI 机制，采样函数本身不接触控制器。`do_sprofile` 系统调用已调用此接口（见 os/kernel/src/misc.rs:fn dispatch_profile）。

### 3.2 D2: 样本收集实现

**C 行为**: `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler` 实现采样收集。

**Rust 64-bit 决策**: 实现样本收集，使用 `static mut` buffer + BKL 保护。

**已实现**:
- os/kernel/src/misc.rs: `SprofSample` / `SprofProc` 结构体 + `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler` / `is_sys_proc_runnable`；接线侧 `profile_clock_hook` / `profile_clock_tick` / `stash_profile_pc` / `PROFILE_TRAP_PC`（见 §4.3）
- `profile_sample(proc, pc, priv_table)` 接收 `&KProcess` + PC + `&PrivTable`，分类为 idle/system/user
- `profile_clock_handler(proc, pc, priv_table)` 调用 `profile_sample` 后 `ack_profile_clock()`
- 使用 `addr_of_mut!` 避免 Rust 2024 `static_mut_refs` 问题

**设计选择**:
1. `static mut` buffer（BKL 保护）而非堆分配——`no_std` 内核中断上下文不能堆分配
2. PC 作为参数传入（非从 `p_reg` 读取）——Rust trap frame 在栈上，不在进程结构体中
3. `PrivTable` 作为参数传入——避免全局可变状态访问
4. 采样进入 `IrqManager` 的 hook 链（`profile_clock_hook`），PC 经单发槽 `PROFILE_TRAP_PC` 从 trap 入口传递——trap 入口只负责暂存 `frame.rip`，控制器 mask/unmask/EOI 全部复用派发机制；完整叙述见 §4.3

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

### 4.3 do_sprofile 调用与中断接线

PROF_START 与 PROF_STOP 的控制面在 os/kernel/src/misc.rs:fn dispatch_profile：START 校验 endpoint 与 `intr_type`（PROF_NMI 返回 ENOSYS，见 §4.4）后调 `init_profile_clock`；STOP 停钟后通过 `data_copy_vmcheck` 把 `SPROF_INFO` 与样本 buffer 拷回用户进程。

数据面的最后一环——时钟中断真的产生样本——由三段接力完成。C 里这是 profile.c:27-49 的一次 `put_irq_handler`；Rust 拆开是因为它的 IRQ 路径在入口处不保存进程上下文，PC 需要一条自己的通道。

**第一段：注册。** C 在 `arch_init_profile_clock` 返回 IRQ 号后把 handler 挂上 CMOS_CLOCK_IRQ（profile.c:34）。Rust 侧 os/kernel/src/clock.rs:fn register_profile_hook 做同一件事：arch 编程返回 `Ok` 后，把 os/kernel/src/misc.rs:fn profile_clock_hook 挂到 `minix_plat::PROFILE_CLOCK_IRQ` 上——x86-64 这是 8，RTC 周期中断经 IOAPIC 的输入线；aarch64/riscv64 的 arch 实现返回 Unsupported，这条注册路径根本不会执行。hook ID 记在 `PROFILE_HOOK_ID`，PROF_STOP 时 os/kernel/src/clock.rs:fn unregister_profile_hook 用它摘除，对应 profile.c:47-48 的 `disable_irq` + `rm_irq_handler`——`IrqManager::remove_hook` 在链空时重新 mask 控制器线，disable 那半边被它覆盖。

**第二段：PC 的传递。** C 的 handler 从 `p->p_reg.pc` 读被中断上下文的 PC，那是汇编入口早已存进进程上下文的值。Rust 的 IRQ 路径不把 trap frame 存进进程上下文，PC 只活在栈上的 `TrapFrame` 里，而 hook 链的 fn 指针签名又传不进 frame。解法是单发槽 `PROFILE_TRAP_PC`（misc.rs）：trap 入口（os/kernel/src/trap_dispatch.rs 的 IRQ 分支）发现来的是 profile clock 线且 `SPROFILING` 为真时，把 `frame.rip` 写进槽；hook 在链内用 `swap(0)` 一次性取走。单发槽够用的原因有三：这条线只投递到一个 CPU；派发机制在 hook 链运行期间 mask 着它；整条链跑在被中断上下文持有的 BKL 下——第二次触发不可能在第一次被消费之前插入。

**第三段：采样与应答。** hook 借 `BklSection::assume_held` 取回进程表、特权表与当前进程（与 `clock_irq_handler` 同一模式），把（当前进程，PC）交给参数化的 os/kernel/src/misc.rs:fn profile_clock_tick。tick 在有当前进程且 PC 非零时走 `profile_clock_handler` 采样（内部含 RTC register C 的读取应答）；否则只应答。RTC 的中断在 register C 被读之前不会再次产生，所以哪怕这次没东西可采，应答也不能省——profile.c:123 的 `arch_ack_profile_clock()` 对每次 tick 无条件执行，就是这个语义。

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

### 5.3 中断接线测试

os/kernel/src/misc.rs tests 模块中 3 个测试覆盖 tick 核心路径（`profile_clock_tick` 是 `profile_clock_hook` 剥离全局状态后的可宿主测试核心）：

| 测试名 | 覆盖路径 |
|--------|---------|
| `test_profile_clock_tick_samples_stashed_pc` | 有当前进程 + 有 PC → 采样，PC 逐字进入样本记录 |
| `test_profile_clock_tick_without_pc_only_acks` | PC 为 0 → 不构造样本，只应答时钟线 |
| `test_profile_clock_tick_without_current_process_only_acks` | 无当前进程 → 不采样，只应答时钟线 |

plat 三架构各 1 个 pin 测试钉住 IRQ 身份：os/plat/src/x86_64/interrupt.rs:fn test_profile_clock_irq_is_cmos_irq8（= 8）与 os/plat/src/arm64/interrupt.rs:fn test_profile_clock_irq_is_unreachable_pseudo / os/plat/src/riscv64/interrupt.rs:fn test_profile_clock_irq_is_unreachable_pseudo（= 0，永不注册）。

宿主测试覆盖不到的部分是 hook 在真实 `IrqManager` 链上的运转与 trap 入口分支——hook 的 fn 指针签名读的是全局状态，无法注入测试替身；这两段需要 QEMU 载体让 RTC 中断真实打进来才能验证，归入验收阶梯的 qemu-tests 一并考虑。

---

## Ch6: 跨文档引用

### 6.1 前序引用

- [15-clock-timer.md](15-clock-timer.md): `ClockArch` trait 定义（profile 时钟接口所在）
- [25-misc-unported.md](25-misc-unported.md) §2.5: `do_sprofile` 系统调用（用户态接口）
- [26-watchdog.md](26-watchdog.md): NMI 机制 + `nmi_sprofile_handler`

