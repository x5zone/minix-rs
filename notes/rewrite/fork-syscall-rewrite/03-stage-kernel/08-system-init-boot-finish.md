# 08-system-init-boot-finish: 系统调用初始化与启动完成

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/system.c:168-289`, `minix3/minix/kernel/arch/i386/pg_utils.c:86-121`, `minix3/minix/kernel/main.c:38-110`
> **说明**: kmain 的最后三步——系统调用注册、bootstrap 内存回收、启动完成——把内核从"初始化态"带入"运行态"

---

## 1. 概述

### 1.1 核心问题

kmain 的最后三步（T8-T10）完成内核从"初始化态"到"运行态"的转换。在此之前（T0-T7），内核完成了硬件发现、页表建立、进程表初始化、特权结构分配——但这些进程全部处于 `RTS_PROC_STOP` 状态，无人可运行。T8-T10 做三件事：

1. **注册系统调用处理函数**（system_init）——让进程的 `SYS_*` 请求有对应的处理入口
2. **回收 bootstrap 内存**（add_memmap）——boot 阶段临时占用的物理内存归还给系统
3. **启动调度循环**（bsp_finish_booting）——解除 boot 进程的 PROC_STOP，启动时钟，调用 `switch_to_user()` 永不返回

### 1.2 三个子阶段的时序关系

```
T8: system_init()
 │  注册 call_vec[] → IRQ hook 池初始化 → alarm timer 初始化
 │
T9: add_memmap(bootstrap)
 │  bootstrap 物理内存回收 → kernel_may_alloc 即将关闭
 │
T10: bsp_finish_booting()
 │  vm_running=0 → announce → RTS_PROC_STOP 解除 →
 │  时钟启动 → FPU 初始化 → kernel_may_alloc=0 →
 │  switch_to_user() ← 永不返回
```

**关键不变量**：T8 之前，`call_vec[]` 全为 NULL，任何系统调用都会 panic；T10 之前，所有 boot 进程被 `RTS_PROC_STOP` 阻止运行；T10 之后，内核进入调度循环，不再返回 kmain。

### 1.3 与前后文档的关系

| 前置 | 本文档 | 后续 |
|------|--------|------|
| 06: ptproc 已设置、freepdes 已分配 | T8-T10: 内核初始化完成 | 08: VM 启动后的内核-VM 协商协议 |

> 注：同目录下 06 文档实际文件名为 `07-cross-space-init.md`，文中简写为“06 文档”。

06 完成后，进程表和特权结构已就绪，但进程不可运行（PROC_STOP），系统调用未注册。本文档覆盖从"所有基础设施就绪"到"调度循环启动"的过渡。

### 1.4 行为规则

1. **system_init 必须在 bsp_finish_booting 之前完成**——否则进程发出系统调用时 `call_vec[N]` 为 NULL，内核 panic
2. **add_memmap 必须在 kernel_may_alloc=0 之前完成**——add_memmap 断言 `kernel_may_alloc` 为真
3. **bsp_finish_booting 是 kmain 的最后一步**——调用 `switch_to_user()` 后永不返回

---

## 2. C 源码分析

### 2.1 相关定义（常量、宏）

**系统调用号定义**（`minix3/minix/include/minix/com.h:207-270`）：

| 常量 | 值 | 处理函数 | 类别 |
|------|-----|---------|------|
| SYS_FORK | KERNEL_CALL+0 | do_fork | 进程管理 |
| SYS_EXEC | KERNEL_CALL+1 | do_exec | 进程管理 |
| SYS_CLEAR | KERNEL_CALL+2 | do_clear | 进程管理 |
| SYS_SCHEDULE | KERNEL_CALL+3 | do_schedule | 调度 |
| SYS_PRIVCTL | KERNEL_CALL+4 | do_privctl | 进程管理 |
| SYS_TRACE | KERNEL_CALL+5 | do_trace | 进程管理 |
| SYS_KILL | KERNEL_CALL+6 | do_kill | 信号 |
| SYS_GETKSIG | KERNEL_CALL+7 | do_getksig | 信号 |
| SYS_ENDKSIG | KERNEL_CALL+8 | do_endksig | 信号 |
| SYS_SIGSEND | KERNEL_CALL+9 | do_sigsend | 信号 |
| SYS_SIGRETURN | KERNEL_CALL+10 | do_sigreturn | 信号 |
| SYS_MEMSET | KERNEL_CALL+13 | do_memset | 内存 |
| SYS_UMAP | KERNEL_CALL+14 | do_umap | 拷贝 |
| SYS_VIRCOPY | KERNEL_CALL+15 | do_vircopy | 拷贝 |
| SYS_PHYSCOPY | KERNEL_CALL+16 | do_copy | 拷贝 |
| SYS_UMAP_REMOTE | KERNEL_CALL+17 | do_umap_remote | 拷贝 |
| SYS_VUMAP | KERNEL_CALL+18 | do_vumap | 拷贝 |
| SYS_IRQCTL | KERNEL_CALL+19 | do_irqctl | 设备 I/O |
| SYS_DEVIO | KERNEL_CALL+21 | do_devio | 设备 I/O (x86) |
| SYS_SDEVIO | KERNEL_CALL+22 | do_sdevio | 设备 I/O (x86) |
| SYS_VDEVIO | KERNEL_CALL+23 | do_vdevio | 设备 I/O (x86) |
| SYS_SETALARM | KERNEL_CALL+24 | do_setalarm | 时钟 |
| SYS_TIMES | KERNEL_CALL+25 | do_times | 时钟 |
| SYS_GETINFO | KERNEL_CALL+26 | do_getinfo | 系统控制 |
| SYS_ABORT | KERNEL_CALL+27 | do_abort | 系统控制 |
| SYS_IOPENABLE | KERNEL_CALL+28 | do_iopenable | 设备 I/O (x86) |
| SYS_SAFECOPYFROM | KERNEL_CALL+31 | do_safecopy_from | 拷贝 |
| SYS_SAFECOPYTO | KERNEL_CALL+32 | do_safecopy_to | 拷贝 |
| SYS_VSAFECOPY | KERNEL_CALL+33 | do_vsafecopy | 拷贝 |
| SYS_SETGRANT | KERNEL_CALL+34 | do_setgrant | 进程管理 |
| SYS_READBIOS | KERNEL_CALL+35 | do_readbios | 设备 I/O (x86) |
| SYS_SPROF | KERNEL_CALL+36 | do_sprofile | 性能 |
| SYS_STIME | KERNEL_CALL+39 | do_stime | 时钟 |
| SYS_SETTIME | KERNEL_CALL+40 | do_settime | 时钟 |
| SYS_VMCTL | KERNEL_CALL+43 | do_vmctl | 内存 |
| SYS_DIAGCTL | KERNEL_CALL+44 | do_diagctl | 系统控制 |
| SYS_VTIMER | KERNEL_CALL+45 | do_vtimer | 时钟 |
| SYS_RUNCTL | KERNEL_CALL+46 | do_runctl | 进程管理 |
| SYS_GETMCONTEXT | KERNEL_CALL+50 | do_getmcontext | 机器状态 |
| SYS_SETMCONTEXT | KERNEL_CALL+51 | do_setmcontext | 机器状态 |
| SYS_UPDATE | KERNEL_CALL+52 | do_update | 进程管理 |
| SYS_EXIT | KERNEL_CALL+53 | do_exit | 进程管理 |
| SYS_SCHEDCTL | KERNEL_CALL+54 | do_schedctl | 调度 |
| SYS_STATECTL | KERNEL_CALL+55 | do_statectl | 进程管理 |
| SYS_SAFEMEMSET | KERNEL_CALL+56 | do_safememset | 拷贝 |
| SYS_PADCONF | KERNEL_CALL+57 | do_padconf | ARM |

**NR_SYS_CALLS = 58**（`com.h:270`）

**map() 宏**（`system.c:54-57`）：
```c
#define map(call_nr, handler)                   \
    {   int call_index = call_nr-KERNEL_CALL;   \
        assert(call_index >= 0 && call_index < NR_SYS_CALLS); \
        call_vec[call_index] = (handler); }
```

**IRQ hook 池**：`NR_IRQ_HOOKS = 64`（`glo.h`）

**4GB 截断常量**（`pg_utils.c:88`）：`#define LIMIT 0xFFFFF000`

### 2.2 核心数据结构

**call_vec**（`system.c:52`）：
```c
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);
```
- 函数指针数组，下标 = `syscall_number - KERNEL_CALL`
- 初始化为 NULL，system_init() 中逐个 map
- kernel_call_dispatch() 通过 `call_vec[call_nr]` 分派

**irq_hooks[]**（`glo.h`）：
```c
struct irq_hook {
    int proc_nr_e;       /* -1 = NONE = 空槽 */
    /* ... 其他字段 */
} irq_hooks[NR_IRQ_HOOKS];
```
- 固定大小池，`proc_nr_e == NONE` 表示可用

**s_alarm_timer**（`priv.h`）：
- 每个 `struct priv` 包含一个 `minix_timer_t s_alarm_timer`
- system_init() 遍历所有 priv 结构初始化定时器

**kernel_may_alloc**（`glo.h`）：
- 全局标志，kmain 开始时设为 1
- bsp_finish_booting() 中设为 0
- add_memmap() 断言此标志为真

**vm_running**（`glo.h`）：
- 全局标志，bsp_finish_booting() 中设为 0
- do_vmctl 的多个子命令检查此标志

### 2.3 关键函数分析

#### system_init()（`system.c:168-289`）

**三步初始化**：

1. **IRQ hook 池清零**（L178-180）：遍历 `irq_hooks[0..NR_IRQ_HOOKS-1]`，设 `proc_nr_e = NONE`
2. **Alarm timer 初始化**（L182-184）：遍历 `BEG_PRIV_ADDR..END_PRIV_ADDR`，对每个 priv 调用 `tmr_inittimer()`
3. **call_vec 注册**（L186-278）：先全部置 NULL，然后逐个 `map(SYS_*, do_*)` 注册

**map() 宏的安全保证**：编译期 assert 确保系统调用号在 `[0, NR_SYS_CALLS)` 范围内。如果有人用了非法调用号，编译失败。

**条件编译**：
- `SYS_DEVIO`/`SYS_SDEVIO`/`SYS_VDEVIO`/`SYS_IOPENABLE`/`SYS_READBIOS` 仅 `__i386__`
- `SYS_PADCONF` 仅 `__arm__`
- 64 位（x86_64/aarch64/riscv64）这些调用不注册

#### add_memmap()（`pg_utils.c:86-121`）

**功能**：将 bootstrap 阶段占用的物理内存区域添加到 `kinfo.memmap[]` 供 VM 管理。

**关键逻辑**：
1. **4GB 截断**（L89-92）：`addr > LIMIT` 直接返回；`addr + len > LIMIT` 截断 len
2. **页对齐**（L97-98）：base 向上对齐，len 向下对齐到 PAGE_SIZE
3. **断言 kernel_may_alloc**（L102）：确保在内核分配窗口内调用
4. **查找空槽**（L106-121）：线性扫描 `memmap[]`，找到第一个 `mm_length == 0` 的槽
5. **更新 mem_high_phys**（L123-125）：跟踪最高物理地址

**32 位遗留**：`LIMIT = 0xFFFFF000`（4GB-4KB）是 Minix3 32 位地址空间限制。64 位下不需要此截断。

#### bsp_finish_booting()（`main.c:38-110`）

**BSP 启动完成序列**：

| 步骤 | 代码 | 说明 |
|------|------|------|
| 1 | `cpu_identify()` | CPU 特性识别 |
| 2 | `vm_running = 0` | VM 尚未运行 |
| 3 | `krandom` 初始化 | 随机数源配置 |
| 4 | `bill_ptr = proc_ptr = idle_proc` | 初始计费/当前进程指向 IDLE |
| 5 | `announce()` | 打印 MINIX 启动横幅 |
| 6 | `RTS_UNSET(proc_addr(i), RTS_PROC_STOP)` | 解除 boot 进程的停止标志（i=0..NR_BOOT_PROCS-NR_TASKS-1） |
| 7 | `cycles_accounting_init()` | CPU 计账初始化 |
| 8 | `boot_cpu_init_timer(system_hz)` | 启动 100Hz 时钟 |
| 9 | `fpu_init()` | FPU 初始化 |
| 10 | `cpu_set_flag(bsp_cpu_id, CPU_IS_READY)` | SMP: 标记 BSP 就绪 |
| 11 | `kernel_may_alloc = 0` | 关闭内核分配窗口 |
| 12 | `switch_to_user()` | 永不返回，进入调度循环 |

**步骤 6 的范围**：只解除 `i < NR_BOOT_PROCS - NR_TASKS` 的进程（即用户态 boot 进程），内核任务（IDLE/CLOCK/SYSTEM/ASYNCM）已经在更早阶段启动。

**步骤 8 的失败处理**：如果时钟初始化失败，直接 panic——没有时钟源，内核无法调度。

**步骤 11 的含义**：`kernel_may_alloc = 0` 后，内核不能再直接分配物理内存。所有内存管理交给 VM。

#### kernel_call_dispatch()（`system.c:103-116`）

**系统调用分派**：
```c
call_nr = msg->m_type - KERNEL_CALL;
if (call_nr < 0 || call_nr >= NR_SYS_CALLS) return EBADCALL;
result = call_vec[call_nr](caller, msg);
```

**VMSUSPEND 处理**（`kernel_call_finish()`，L58-90）：
- 如果 handler 返回 `VMSUSPEND`，保存请求消息到 `p_vmrequest.saved.reqmsg`，设置 `MF_KCALL_RESUME`
- 否则，将结果拷贝回用户空间

### 2.4 调用关系

```
kmain()
 ├── system_init()                    ← T8
 │    ├── irq_hooks[] 初始化
 │    ├── tmr_inittimer() × N privs
 │    └── map(SYS_*, do_*) × 50+      ← call_vec 注册
 ├── add_memmap(&kinfo, bootstrap)    ← T9
 └── bsp_finish_booting()             ← T10
      ├── cpu_identify()
      ├── vm_running = 0
      ├── announce()
      ├── RTS_UNSET × (NR_BOOT_PROCS - NR_TASKS)
      ├── cycles_accounting_init()
      ├── boot_cpu_init_timer(100)
      ├── fpu_init()
      ├── kernel_may_alloc = 0
      └── switch_to_user()            ← 永不返回
```

### 2.5 设计要点

1. **map() 宏的编译期安全**：非法调用号 → assert 失败 → 编译错误。这是 C 的"穷尽检查"替代方案。
2. **kernel_may_alloc 窗口**：kmain 开始时为 1，bsp_finish_booting 最后设为 0。这个窗口保证内核在 VM 接管前可以分配内存。
3. **vm_running 的初始值**：设为 0（而非 1），因为此时 VM 尚未启动。do_vmctl 的子命令通过此标志判断 VM 是否可用。
4. **boot 进程分两批启动**：内核任务在 proc_init 阶段就启动；用户态 boot 进程在 bsp_finish_booting 中解除 PROC_STOP。

---

## 3. Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 | 来源 |
|---|------|------|------|------|------|
| D1 | call_vec 表达 | `[Option<fn>; 58]` vs `enum Syscall + match` | **match** | 类型安全 + 编译期穷尽检查 + 无函数指针数组 | m3 + kimi 一致 |
| D2 | map() 宏替代 | `const _: () = assert!(...)` vs 运行时 assert | **const assert** | 编译期检查，与 C 的 map() 宏安全级别等价 | kimi |
| D3 | IRQ hook 池 | `Vec<Option<IrqHook>>` vs `[Option<IrqHook>; 64]` | **`[Option; NR_IRQ_HOOKS]`** | 保持 C 池语义，O(1) 索引，无堆分配 | 已实现 (irq_manager.rs) |
| D4 | Alarm timer | 每个 priv 一个 timer struct | **保持** | per-priv 而非全局，与 C 语义一致 | m3 |
| D5 | add_memmap 4GB 截断 | 保留 vs 删除 | **删除** | 64 位不需要 4GB 限制，Direct Map 可访问全部物理内存 | 5/5 AI 一致 |
| D6 | vm_running 表达 | `AtomicBool` vs `CpuLocal<bool>` | **当前：全局 `AtomicBool`** | 单 BSP 启动阶段用全局原子过渡；SMP 就绪后移入 `SmpState.cpu_locals[cpu].vm_running` | ds + glm |
| D7 | switch_to_user | 普通函数 vs 发散函数 | **`-> !`** | 类型系统表达永不返回 | m3 + kimi 一致 |
| D8 | kernel_may_alloc | `AtomicBool` vs 编译期保证 | **运行时 AtomicBool** | C 的运行时标志无法完全消除（add_memmap 依赖它） | m3 |
| D9 | 条件编译 syscall | `#[cfg(target_arch)]` vs trait 分发 | **match + 架构无关默认** | 不注册的 syscall 在 match 中返回 EBADCALL，无需 cfg | qwen |

**D1 详细论证**：C 用函数指针数组 `call_vec[]` 做分派。Rust 用 `enum Syscall` + `match` 有三个优势：
1. **穷尽检查**：新增 syscall 时，match 未覆盖则编译失败——等价于 C 的 map() 宏 assert
2. **无函数指针**：避免间接调用的缓存不友好和安全隐患
3. **类型安全**：`enum Syscall` 的变体携带语义，而非裸整数

**D5 详细论证**：C 的 `add_memmap()` 有 `LIMIT = 0xFFFFF000` 截断，因为 32 位 Minix3 无法处理 >4GB 物理地址。64 位下 Direct Map 可以映射全部物理内存，此截断无意义。但 `add_memmap()` 本身仍需保留——bootstrap 内存回收是必要的。

**D9 详细论证**：C 用 `#if defined(__i386__)` 条件编译决定是否注册 `SYS_DEVIO` 等 x86 专用调用。Rust 不用 `#[cfg(target_arch)]` 选择行为（硬件抽象原则），而是让所有架构共享同一个 `enum Syscall` 定义，架构不支持的 syscall 在 match 分支中返回 `EBADCALL`。这避免了条件编译导致的代码路径分裂。

---

## 4. 实现详解

### 4.1 Syscall 枚举

```rust
// os/kernel/src/syscall.rs

/// Kernel system call number.
///
/// C: `SYS_*` constants in minix/com.h:207-270
/// C: `NR_SYS_CALLS = 58` in minix/com.h:270
///
/// Design decision D1: enum + match replaces C's call_vec[] function pointer array.
/// Design decision D9: architecture-specific syscalls return EBADCALL on
/// unsupported platforms rather than being conditionally compiled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    Clear = 2,
    Schedule = 3,
    Privctl = 4,
    Trace = 5,
    Kill = 6,
    Getksig = 7,
    Endksig = 8,
    Sigsend = 9,
    Sigreturn = 10,
    // 11-12: unused
    Memset = 13,
    Umap = 14,
    Vircopy = 15,
    Physcopy = 16,
    UmapRemote = 17,
    Vumap = 18,
    Irqctl = 19,
    // 20: unused
    Devio = 21,
    Sdevio = 22,
    Vdevio = 23,
    Setalarm = 24,
    Times = 25,
    Getinfo = 26,
    Abort = 27,
    Iopenable = 28,
    // 29-30: unused
    SafecopyFrom = 31,
    SafecopyTo = 32,
    Vsafecopy = 33,
    Setgrant = 34,
    Readbios = 35,
    Sprof = 36,
    // 37-38: unused
    Stime = 39,
    Settime = 40,
    // 41-42: unused
    Vmctl = 43,
    Diagctl = 44,
    Vtimer = 45,
    Runctl = 46,
    // 47-49: unused
    Getmcontext = 50,
    Setmcontext = 51,
    Update = 52,
    Exit = 53,
    Schedctl = 54,
    Statectl = 55,
    Safememset = 56,
    Padconf = 57,
}

/// Total number of kernel system calls.
/// C: NR_SYS_CALLS = 58
pub const NR_SYS_CALLS: usize = 58;

impl TryFrom<u16> for Syscall {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Fork),
            1 => Ok(Syscall::Exec),
            2 => Ok(Syscall::Clear),
            3 => Ok(Syscall::Schedule),
            4 => Ok(Syscall::Privctl),
            5 => Ok(Syscall::Trace),
            6 => Ok(Syscall::Kill),
            7 => Ok(Syscall::Getksig),
            8 => Ok(Syscall::Endksig),
            9 => Ok(Syscall::Sigsend),
            10 => Ok(Syscall::Sigreturn),
            13 => Ok(Syscall::Memset),
            14 => Ok(Syscall::Umap),
            15 => Ok(Syscall::Vircopy),
            16 => Ok(Syscall::Physcopy),
            17 => Ok(Syscall::UmapRemote),
            18 => Ok(Syscall::Vumap),
            19 => Ok(Syscall::Irqctl),
            21 => Ok(Syscall::Devio),
            22 => Ok(Syscall::Sdevio),
            23 => Ok(Syscall::Vdevio),
            24 => Ok(Syscall::Setalarm),
            25 => Ok(Syscall::Times),
            26 => Ok(Syscall::Getinfo),
            27 => Ok(Syscall::Abort),
            28 => Ok(Syscall::Iopenable),
            31 => Ok(Syscall::SafecopyFrom),
            32 => Ok(Syscall::SafecopyTo),
            33 => Ok(Syscall::Vsafecopy),
            34 => Ok(Syscall::Setgrant),
            35 => Ok(Syscall::Readbios),
            36 => Ok(Syscall::Sprof),
            39 => Ok(Syscall::Stime),
            40 => Ok(Syscall::Settime),
            43 => Ok(Syscall::Vmctl),
            44 => Ok(Syscall::Diagctl),
            45 => Ok(Syscall::Vtimer),
            46 => Ok(Syscall::Runctl),
            50 => Ok(Syscall::Getmcontext),
            51 => Ok(Syscall::Setmcontext),
            52 => Ok(Syscall::Update),
            53 => Ok(Syscall::Exit),
            54 => Ok(Syscall::Schedctl),
            55 => Ok(Syscall::Statectl),
            56 => Ok(Syscall::Safememset),
            57 => Ok(Syscall::Padconf),
            _ => Err(()),
        }
    }
}
```

> 设计决策 D1：enum + match 替代 C 的 `call_vec[]` 函数指针数组。新增 syscall 时 match 未覆盖则编译失败，等价于 C 的 map() 宏 assert。

### 4.2 系统调用分派

```rust
// os/kernel/src/syscall.rs (continued)

use crate::proc::KProcess;
use crate::kpriv::PrivTable;
use crate::proc_table::ProcessTable;
use crate::clock::ClockState;
use minix_types::Message;

/// Result of a kernel call dispatch.
/// C: EBADCALL, VMSUSPEND, EDONTREPLY in minix/errno.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with return value.
    Ok(i32),
    /// Call requires VM assistance (VMSUSPEND = -996 in C).
    VmSuspend,
    /// No reply should be sent (EDONTREPLY).
    NoReply,
    /// Invalid/unimplemented syscall number.
    BadCall,
    /// Caller does not have permission for this call.
    /// C: `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:107
    CallDenied,
}

/// Dispatch a kernel system call.
///
/// C: kernel_call_dispatch() in system.c:103-116
/// C: kernel_call_finish() in system.c:58-90
///
/// Design decision D1: match replaces call_vec[] dispatch.
/// Design decision D9: arch-specific syscalls return BadCall on unsupported
/// platforms instead of being conditionally compiled out.
///
/// # BKL
///
/// Acquires the Big Kernel Lock on entry. BKL is released later in
/// `kernel_call_finish()` or `switch_to_user()`, matching C's pattern where
/// the trap entry holds the lock across dispatch + finish.
pub fn kernel_call_dispatch(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    proc_table: &mut ProcessTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    let _ = crate::smp::bkl_lock(); // C: BKL_LOCK() in mpx.S trap entry

    let call_nr = msg.m_type as u16;
    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))`
    let denied = match caller.priv_id {
        Some(id) => match priv_table.get(id) {
            Some(priv) => !kcall_filter_check(priv, call_nr as u32),
            None => true,
        },
        None => true,
    };
    if denied {
        return KcallResult::CallDenied;
    }

    match syscall {
        Syscall::Fork => crate::syscall_process::dispatch_fork(caller, msg, proc_table, priv_table),
        Syscall::Exec => crate::syscall_process::dispatch_exec(caller, msg, proc_table),
        Syscall::Clear => crate::syscall_process::dispatch_clear(caller, msg, proc_table, priv_table),
        Syscall::Exit => crate::syscall_process::dispatch_exit(caller, msg),
        Syscall::Schedule => dispatch_schedule(caller, msg, proc_table),
        Syscall::Privctl => dispatch_privctl(caller, msg),
        Syscall::Trace => dispatch_trace(caller, msg, proc_table),
        Syscall::Kill => dispatch_kill(caller, msg, proc_table, priv_table),
        Syscall::Getksig => dispatch_getksig(caller, msg, proc_table, priv_table),
        Syscall::Endksig => dispatch_endksig(caller, msg, proc_table, priv_table),
        Syscall::Sigsend => dispatch_sigsend(caller, msg, proc_table),
        Syscall::Sigreturn => dispatch_sigreturn(caller, msg, proc_table),
        Syscall::Memset => dispatch_memset(caller, msg, proc_table),
        Syscall::Umap => dispatch_umap(caller, msg, proc_table),
        Syscall::Vircopy => dispatch_vircopy(caller, msg, proc_table),
        Syscall::Physcopy => dispatch_physcopy(caller, msg, proc_table),
        Syscall::UmapRemote => dispatch_umap_remote(caller, msg, proc_table),
        Syscall::Vumap => dispatch_vumap(caller, msg, proc_table),
        Syscall::Irqctl => dispatch_irqctl(caller, msg),
        // D9: x86-specific syscalls — BadCall on unsupported arch.
        Syscall::Devio => dispatch_arch_devio(caller, msg, priv_table),
        Syscall::Sdevio => dispatch_arch_sdevio(caller, msg, priv_table, proc_table),
        Syscall::Vdevio => dispatch_arch_vdevio(caller, msg),
        Syscall::Setalarm => dispatch_setalarm(caller, msg, priv_table, clock_state),
        Syscall::Times => dispatch_times(caller, msg, proc_table),
        Syscall::Getinfo => dispatch_getinfo(caller, msg, priv_table, proc_table),
        Syscall::Abort => dispatch_abort(caller, msg),
        Syscall::Iopenable => dispatch_arch_iopenable(caller, msg, proc_table),
        Syscall::SafecopyFrom => dispatch_safecopy_from(caller, msg, proc_table),
        Syscall::SafecopyTo => dispatch_safecopy_to(caller, msg, proc_table),
        Syscall::Vsafecopy => dispatch_vsafecopy(caller, msg),
        Syscall::Setgrant => dispatch_setgrant(caller, msg, priv_table),
        Syscall::Readbios => dispatch_arch_readbios(caller, msg),
        Syscall::Sprof => dispatch_sprofile(caller, msg, proc_table),
        Syscall::Stime => dispatch_stime(caller, msg, clock_state),
        Syscall::Settime => dispatch_settime(caller, msg, clock_state),
        Syscall::Vmctl => dispatch_vmctl(caller, msg, proc_table),
        Syscall::Diagctl => dispatch_diagctl(caller, msg, priv_table),
        Syscall::Vtimer => dispatch_vtimer(caller, msg, priv_table, proc_table),
        Syscall::Runctl => dispatch_runctl(caller, msg, proc_table),
        Syscall::Getmcontext => dispatch_getmcontext(caller, msg, proc_table),
        Syscall::Setmcontext => dispatch_setmcontext(caller, msg, proc_table),
        Syscall::Update => dispatch_update(caller, msg, proc_table, priv_table),
        Syscall::Schedctl => dispatch_schedctl(caller, msg, proc_table),
        Syscall::Statectl => dispatch_statectl(caller, msg, priv_table),
        Syscall::Safememset => dispatch_safememset(caller, msg, proc_table, priv_table),
        // D9: ARM-specific — BadCall on non-ARM.
        Syscall::Padconf => dispatch_arch_padconf(caller, msg),
    }
}
```

> 设计决策 D1：match 替代 call_vec[]。D9：架构专用 syscall 在不支持平台返回 BadCall。当前实现已将具体子系统调用（fork/exec/clear 等）委托到 `syscall_process` 等子模块，而非返回 BadCall 的占位符。

### 4.3 编译期断言

```rust
// os/kernel/src/syscall.rs (continued)

/// Compile-time verification that all syscall numbers in the enum
/// are within the valid range [0, NR_SYS_CALLS).
///
/// C: map() macro's assert(call_index >= 0 && call_index < NR_SYS_CALLS)
/// Design decision D2: const assert replaces C's runtime assert in map() macro.
const _: () = {
    let _ = Syscall::Fork as u16;      // 0
    let _ = Syscall::Padconf as u16;   // 57
    assert!(Syscall::Fork as u16 == 0);
    assert!(Syscall::Padconf as u16 == 57);
    assert!(Syscall::Padconf as u16 < NR_SYS_CALLS as u16);
};
```

### 4.4 system_init 的 Rust 表达

C 中的 `system_init()` 做三件事：清零 `irq_hooks[]`、初始化每个 `priv` 的 alarm timer、用 `map()` 宏填充 `call_vec[]`。在 Rust 中，这三件事被分解到构造函数和类型系统里，**没有独立的 `system_init` 函数**：

1. **IRQ hook 池**：`IrqManager::new()` 在构造时将所有 hook 设为 `None`。
2. **Alarm timer**：`KPriv::new()` 在构造时清零 `s_alarm_timer`。
3. **Call vector**：被 `enum Syscall` + `match` 替代，编译期 const assert（D2）替代 `map()` 宏的运行时 assert。

`kmain` 中 Phase E（原 `system_init()` 阶段）因此只有一行注释标记该阶段，没有函数调用：

```rust
// os/kernel/src/lib.rs (kmain Phase E)
// Phase E: system_init equivalent — IrqManager/KPriv/Syscall already constructed.
```

> 设计决策 D1/D2：Rust 的 enum + match + const assert 替代 C 的 call_vec[] + map() 宏。`system_init` 的三步初始化由构造函数和类型系统隐式完成。

### 4.5 add_memmap Rust 实现

```rust
// os/kernel/src/memmap.rs

use minix_boot::KernelInfo;
use minix_types::PhysBytes;

/// Maximum number of memory map entries.
/// C: MAXMEMMAP in minix/com.h
pub const MAXMEMMAP: usize = 128;

/// Memory map entry.
/// C: struct memory_info in minix/type.h
#[derive(Debug, Clone, Copy)]
pub struct MemMapEntry {
    /// Physical base address (page-aligned).
    pub base: u64,
    /// Length in bytes (page-aligned).
    pub length: u64,
}

/// Const zero entry for static initialization.
pub const MEM_MAP_ENTRY_ZERO: MemMapEntry = MemMapEntry { base: 0, length: 0 };

impl Default for MemMapEntry {
    fn default() -> Self {
        MEM_MAP_ENTRY_ZERO
    }
}

impl MemMapEntry {
    /// Whether this entry is empty (available for use).
    /// C: mm_length == 0 check in add_memmap()
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// Add a physical memory region to the kernel's memory map.
///
/// C: add_memmap() in pg_utils.c:86-121
///
/// Design decision D5: 4GB truncation removed for 64-bit.
/// The C version truncates at LIMIT=0xFFFFF000 because 32-bit Minix3
/// cannot handle >4GB physical addresses. In 64-bit minix-rs, Direct Map
/// can access all physical memory, so this truncation is unnecessary.
///
/// # Arguments
///
/// * `mmap` - Memory map array to insert into
/// * `addr` - Physical base address of the region
/// * `len` - Length of the region in bytes
///
/// # Returns
///
/// The index of the new entry, or a `MemMapError` on failure.
///
/// # Safety Invariant
///
/// This function should only be called during boot (while `kernel_may_alloc`
/// is true). The caller is responsible for ensuring this invariant.
/// C: assert(kernel_may_alloc) in pg_utils.c:102
pub fn add_memmap(mmap: &mut [MemMapEntry; MAXMEMMAP], addr: u64, len: u64) -> Result<usize, MemMapError> {
    // C: page alignment (roundup/rounddown)
    let page_size = 4096u64;
    let aligned_base = (addr + page_size - 1) & !(page_size - 1);
    let aligned_end = (addr + len) & !(page_size - 1);
    let aligned_len = aligned_end.saturating_sub(aligned_base);

    if aligned_len == 0 {
        return Err(MemMapError::ZeroLength);
    }

    // C: linear scan for empty slot (mm_length == 0)
    for i in 0..MAXMEMMAP {
        if mmap[i].is_empty() {
            mmap[i] = MemMapEntry {
                base: aligned_base,
                length: aligned_len,
            };
            return Ok(i);
        }
    }

    Err(MemMapError::NoSlots)
}

/// Errors from add_memmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemMapError {
    /// After page alignment, the region has zero length.
    ZeroLength,
    /// No empty slots available in the memory map.
    NoSlots,
}
```

> 设计决策 D5：删除 4GB 截断。64 位 Direct Map 可访问全部物理内存。

### 4.6 bsp_finish_booting Rust 实现

```rust
// os/kernel/src/lib.rs

use core::sync::atomic::{AtomicBool, Ordering};

/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);

/// Global atomic mirror of C's `vm_running` flag.
/// C: vm_running in glo.h:37.
/// Set to false in bsp_finish_booting step 1.
/// Multi-CPU will move it into `SmpState.cpu_locals[cpu].vm_running`.
static VM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Read the `vm_running` flag. C: `vm_running` — glo.h:37.
pub fn vm_running() -> bool { VM_RUNNING.load(Ordering::Acquire) }

/// BSP finish booting — the last step of kmain.
///
/// C: bsp_finish_booting() in main.c:38-110
///
/// Takes `&mut ProcessTable` so step 2 (bill_ptr = IDLE) and step 4
/// (RTS_PROC_STOP unset for boot processes) can operate directly.
/// Takes `&mut SmpState` for per-CPU cycle accounting and BSP identification.
#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::RtsFlagsBits;

    // Step 1: vm_running = 0 — wired to a global atomic; per-CPU on SMP
    VM_RUNNING.store(false, Ordering::Release);

    // Step 2: bill_ptr = idle_proc — ProcessTable::set_bill_to_idle
    proc_table.set_bill_to_idle();

    // Step 3: announce() — EarlyConsole banner (visible in QEMU serial)
    use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
    Console::write_str("\nMINIX-RS 0.1.0 (rust rewrite) — scheduling live\n");

    // Step 4: RTS_PROC_STOP unset for boot processes (skip kernel tasks)
    for nr in 0..(crate::proc::NR_BOOT_PROCS as ProcNr
        - crate::proc_table::NR_TASKS as ProcNr)
    {
        proc_table.rts_unset(nr, RtsFlagsBits::PROC_STOP);
    }

    // Step 5: cycles_accounting_init() — set BSP TSC baseline
    let tsc = crate::clock::read_tsc();
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.note_context_switch(tsc);
    }

    // Step 6: boot_cpu_init_timer — start the periodic tick
    // C: boot_cpu_init_timer(system_hz) — clock.c:294.
    // (a) `init_local_timer(freq)` was already done in Phase B via
    //     `CurrentClockArch::init_timer(DEFAULT_HZ)`.
    // (b) Timer IRQ handler registration is deferred until the global
    //     IrqManager lands; for now `boot_init_timer` accepts a dummy
    //     handler that satisfies the ArchBoot trait signature.
    use minix_arch::CurrentClockArch;
    CurrentClockArch::init_timer(crate::clock::DEFAULT_HZ);
    use minix_arch::arch_boot::{boot_init_timer, CurrentArchBoot, TimerHandlerFn};
    extern "Rust" fn dummy_timer_handler(
        _irq: minix_plat::IrqVector,
        _id: minix_plat::IrqId,
    ) -> minix_plat::IrqAction {
        minix_plat::IrqAction::Completed
    }
    let _ = boot_init_timer::<CurrentArchBoot>(dummy_timer_handler);

    // Step 7: FPU presence probe
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.fpu_presence = true;
    }

    // Step 8: kernel_may_alloc = 0 — closing the boot-time alloc window
    KERNEL_MAY_ALLOC.store(false, Ordering::Release);

    // Step 8.5: acquire BKL before entering the scheduling loop
    // C: BKL is already held when coming from the trap entry; Rust acquires
    // it here because switch_to_user() releases it before returning to user.
    crate::smp::bkl_lock();

    // Step 9: switch_to_user() — never returns (=> !)
    switch_to_user()
}

/// Entry point for the scheduling loop. C: switch_to_user() in proc.c.
/// D7: returns `!` — never returns to caller.
fn switch_to_user() -> ! {
    // Release BKL before entering the scheduling loop.
    // C: BKL is released implicitly by restore_user_context() which
    // does not return. In Rust, we release explicitly before the loop.
    crate::smp::bkl_unlock();

    // Placeholder — full scheduler loop implemented in 10-switch-to-user.md.
    loop { core::hint::spin_loop(); }
}
```

**各步骤职责与当前语义**（截至 2026-06-17）：

| 步骤 | 当前语义 | 说明 |
|------|---------|------|
| 1 `vm_running = 0` | `VM_RUNNING: AtomicBool` 全局镜像置 false | `lib.rs::vm_running()` 暴露给 `do_vmctl` 等读者 |
| 2 `bill_ptr = idle_proc` | `ProcessTable::set_bill_to_idle()` | 将计费指针指向 idle 进程 |
| 3 `announce()` | `EarlyConsole::write_str` 打印 MINIX-RS banner | QEMU 串口可见 |
| 4 `RTS_UNSET` × (NR_BOOT_PROCS-NR_TASKS) | `for nr in 0..(NR_BOOT_PROCS - NR_TASKS)` | 复用 `rts_unset` 自动入队 |
| 5 `cycles_accounting_init` | 设置 BSP TSC baseline | 通过 `SmpState::cpu_local_mut(bsp)` |
| 6 `boot_cpu_init_timer` | 硬件 timer 已在 Phase B 初始化；BSP handler 注册占位 | 等待全局 IrqManager 完成后补齐 |
| 7 `fpu_init` | 标记 `fpu_presence = true` | per-CPU 字段在 `SmpState` 中 |
| 8 `kernel_may_alloc = 0` | `KERNEL_MAY_ALLOC.store(false)` | 关闭启动期直接分配窗口 |
| 8.5 BKL 获取 | `smp::bkl_lock()` | 进入调度循环前持有 BKL |
| 9 `switch_to_user()` | 释放 BKL 后进入占位循环 | 真实调度循环见 10-switch-to-user.md |

**关键变更**：函数签名从 `fn bsp_finish_booting()` 先后改为 `fn bsp_finish_booting(&mut ProcessTable)`，最终为 `fn bsp_finish_booting(&mut ProcessTable, &mut SmpState)`，因为步骤 2/4 需要修改进程表，步骤 5/7 需要访问 per-CPU 状态。

> 设计决策 D7：`bsp_finish_booting() -> !` 类型系统表达永不返回。D6：vm_running 当前用全局 `AtomicBool`，SMP 就绪后移入 `SmpState`。D8：`kernel_may_alloc` 用 `AtomicBool`。

---

## 5. 测试要点

### 5.1 单元测试

| 测试 | 覆盖的设计/实现 | 说明 |
|------|---------------|------|
| `test_syscall_try_from_valid` | D1: Syscall enum | 所有合法 syscall 号可转换为 enum 变体 |
| `test_syscall_try_from_invalid` | D1: Syscall enum | 非法 syscall 号返回 Err |
| `test_kernel_call_dispatch_bad_call` | D1: match dispatch | 非法 syscall 号返回 BadCall |
| `test_add_memmap_no_truncation` | D5: 删除 4GB 截断 | >4GB 地址不被截断 |
| `test_add_memmap_alignment` | Ch2: 页对齐 | 非 4KB 对齐的地址/长度被正确对齐 |
| `test_add_memmap_zero_length` | Ch2: 零长度检查 | 对齐后长度为 0 返回错误 |
| `test_add_memmap_no_slots` | Ch2: 槽位耗尽 | 所有槽位已满时返回错误 |

### 5.2 静态测试

| 测试 | 覆盖的设计/实现 | 说明 |
|------|---------------|------|
| 编译期穷尽检查 | D1/D2 | `Syscall` enum 新增变体而不补 `match` arm → 编译失败；`const` assert 保证所有变体值 < NR_SYS_CALLS |

### 5.3 集成测试

| 测试 | 覆盖的设计/实现 | 说明 |
|------|---------------|------|
| `test_bsp_finish_booting_step_5_7_side_effects` | bsp_finish_booting 步骤 5/6/7 | 验证 cycle accounting、timer init、FPU presence 已设置 |
| `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | bsp_finish_booting BSP 唯一性 | 验证仅 BSP 被初始化 |

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) — 内核整体架构
- [07-cross-space-init.md](07-cross-space-init.md) — 进程表初始化（前置）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) — VM 启动后的内核-VM 协商（后续）
- [10-switch-to-user.md](10-switch-to-user.md) — switch_to_user 详细实现
- [13-syscall-dispatch.md](13-syscall-dispatch.md) — 系统调用分派详细实现
- [14-exception-interrupt.md](14-exception-interrupt.md) — 异常与中断处理
- C 源码：`minix3/minix/kernel/system.c:168-289` — system_init()
- C 源码：`minix3/minix/kernel/main.c:38-110` — bsp_finish_booting()
- C 源码：`minix3/minix/kernel/arch/i386/pg_utils.c:86-121` — add_memmap()
- C 头文件：`minix3/minix/include/minix/com.h:207-270` — SYS_* 定义
