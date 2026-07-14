# 06 设计：进程初始化与启动架构（重设计）

> **作者**: Agent Seed（多 AI bagging，独立设计，未参考其他 `06-design-*.md`）
> **范围**: 对应 `06-proc-init-boot-proc.md` 的 Rust 实现重设计。
> **目标原则**（硬约束，见 `06-problem.md` §9）:
> 1. **硬件语义零泄漏到 OS 层** — 类型系统在编译时定型，非当前架构字段/类型根本不存在。
> 2. **Rewrite，不是 Translate** — 从 OS 概念出发定义接口，不与 C 函数签名一一对应。
> 3. **boot 阶段零堆** — `init_proc_and_boot()` 执行时 VM 尚未启动，所有数据结构编译时固定大小。
> 4. **最小 trait 数** — 一个 arch trait 足够表达"启动一个进程"这件事；额外功能用普通模块函数。

---

## 1. 问题根因分析（Root Cause）

### 1.1 当前设计的三个泄漏点

| 泄漏点 | 原因 | 后果 |
|-------|-----|-----|
| `InitialRegState.segment_selectors` | 共享结构体包含 x86-64 特有字段 | `arm64/riscv64` 代码里出现 `SegmentSelectors::default()`（哨兵零值）——违反原则 1 |
| `InitialRegState.fpu_needs_zero: bool` | 把 C 的"清零 FPU"实现细节当概念 | 现代 x86-64 用 XSAVE 的 lazy init；`aarch64/riscv64` 永远 `false`——违反原则 2 |
| `ArchProcReset` / `ArchProcInit` / `BootProcArch` 三个 trait | 翻译 C 的三个同名函数 | 每个 trait 都是"中间值"，kernel 被迫按顺序拼接——违反原则 2 |

### 1.2 kernel 层问题：四个裸字段 + `Box<[T]>`

```rust
// ❌ 当前 KProcess: 四个分别存储的"初始值"字段
pub initial_pc: VirBytes,
pub initial_sp: VirBytes,
pub initial_ps_strings_reg: u64,  // u64 裸整数，语义泄漏（某寄存器）
pub initial_status: u64,           // u64 裸整数，x86 特有注释（IOPL）

// ❌ ProcessTable/PrivTable: Vec→Box，boot 阶段无分配器就会 panic
procs: Box<[KProcess]>,
privs: Box<[KPriv]>,
```

---

## 2. 重新设计：OS 语义层（Kernel 层）

### 2.1 核心思路：`BootConfig` 是 OS 层对"启动一个进程"的唯一抽象

**kernel 层**只关心一个进程启动时需要：

```text
启动一个进程 =  入口点(PC)
              + 栈顶(SP)
              + 一段"由 arch 层来解释"的不透明状态（含状态寄存器、FPU 策略、段寄存器……）
              + 一组特权属性（s_flags / trap_mask / ipc_to / k_call_mask）
```

Kernel 层**不**关心 `segment_selectors`、`fpu_needs_zero`、`XSAVE` vs `lazy FPEN`，这些都下沉到 arch 层。

### 2.2 `KProcess` 重设计：`CurrentBootConfig` 替代四个裸字段

```rust
// ✅ os/kernel/src/proc.rs

use os_arch::proc_boot::{CurrentBootConfig, CurrentBootArch};

/// 内核层的进程描述。
///
/// 设计要点：
/// - 所有"启动时需要保留"的架构相关状态，整体存储在 `boot_cfg` 中。
/// - kernel 层**从不**拆解 `boot_cfg` 的字段；只在写入 trap frame 时调用
///   `CurrentBootArch::apply_to_trap_frame(&self.boot_cfg, &mut frame)`。
/// - `initial_pc` / `initial_sp` / `initial_ps_strings` 是通用虚拟地址，三架构语义
///   一致（"入口点"、"栈顶"、"ps_strings 地址"），不命名为"寄存器"。
pub struct KProcess {
    pub p_nr: ProcNr,
    pub p_endpoint: Endpoint,
    pub p_name: ProcName,
    pub p_rts_flags: RtsFlags,
    pub p_misc_flags: MiscFlags,
    pub p_priv: Option<PrivId>,

    /// 启动配置（入口 + 栈 + ps_strings + arch 不透明状态）。
    pub boot_cfg: CurrentBootConfig,
    // ... 其他调度 / IPC / VM 状态 ...
}

impl KProcess {
    /// 构建一个空 slot（proc_init 阶段，尚未进入 boot 循环）。
    ///
    /// 只调用 `CurrentBootArch::empty_slot_cfg()` — 由 arch 层决定"空 slot"
    /// 的状态寄存器是什么（kernel 完全不关心具体数值）。
    pub const fn new_empty(nr: ProcNr, ep: Endpoint) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: ep,
            p_name: ProcName::new(),
            p_rts_flags: RtsFlags::SLOT_FREE,
            p_misc_flags: MiscFlags::empty(),
            p_priv: None,
            boot_cfg: CurrentBootConfig::empty_slot(),
        }
    }

    /// boot 循环：为可调度进程填入完整启动配置。
    ///
    /// `pc` / `sp` / `ps_strings` 来自 ELF 解析结果（kernel 层计算）；
    /// `arch_state` 来自 `CurrentBootArch::build_state(...)`（arch 层计算）。
    pub fn set_boot_config(
        &mut self,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        arch_state: <CurrentBootArch as ArchBootProc>::ArchState,
    ) {
        self.boot_cfg = CurrentBootConfig::new(pc, sp, ps_strings, arch_state);
    }
}
```

### 2.3 `ProcessTable` / `PrivTable`：编译期固定大小数组

boot 阶段**没有 allocator**。`ProcessTable::new()` 不允许 `Vec::collect().into_boxed_slice()`。

```rust
// ✅ os/kernel/src/proc_table.rs  —  #![no_std]，无 `alloc::*`

pub const NR_TASKS: usize = minix_types::NR_TASKS;
pub const NR_PROCS: usize = 256;
pub const NR_SYS_PROCS: usize = 64;
pub const PROC_TABLE_SIZE: usize = NR_TASKS + NR_PROCS;

pub struct ProcessTable {
    /// 固定大小数组，内嵌在 `ProcessTable` 中。boot 阶段无堆。
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: crate::vm::VmRequestQueue,
}

impl ProcessTable {
    /// const fn 保证在编译期/静态中可构造。
    /// 用 `[const { KProcess::new_empty(...) }; N]` 语法。
    pub const fn new() -> Self {
        // 第 0 个 slot 是 IDLE（proc_nr = -NR_TASKS），其他 slot 连续编号。
        let mut procs = [const { KProcess::new_empty_uninit() }; PROC_TABLE_SIZE];
        let mut i = 0;
        while i < PROC_TABLE_SIZE {
            let nr = (i as i32) - (NR_TASKS as i32);
            // 用结构体字段直接赋值替代 new_empty() 非 const 的问题：
            // 见下方 "const 构造"小节。
            procs[i].p_nr = nr;
            procs[i].p_endpoint = Endpoint::from_generation_slot_const(0, nr);
            procs[i].boot_cfg = CurrentBootConfig::empty_slot();
            // ... RTS 等也用类似方式 ...
            i += 1;
        }
        Self { procs, sched: Scheduler::new(), vm_request_queue: VmRequestQueue::new() }
    }
    // ... get/get_mut/is_valid_nr ...
}
```

**关于 `const { KProcess::new_empty() }`**: `KProcess` 的字段必须全部 `const` 可构造。`KProcess::new_empty_uninit()` 是一个**仅做字段置零**的 `const fn`，与 `new_empty()` 的语义一致但不做任何 trait 调用。这是对设计的一个**实现级约束**，不影响抽象。

`PrivTable` 同理：

```rust
// ✅ os/kernel/src/kpriv.rs

pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_uninit() }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].s_id = i as SysId;
            privs[i].s_proc_nr = None;
            i += 1;
        }
        Self { privs }
    }
    // ... get/get_mut/assign_static/dynamic_alloc ...
}
```

### 2.4 特权配置：用 builder 替代裸参数列表

`06-problem.md` §12 指出当前实现里 `configure_boot_priv()` 接收 6 个裸参数——这是典型的 translate 痕迹。重设计为 builder / 配置结构体：

```rust
// ✅ os/kernel/src/boot_priv.rs

/// 一个 boot 阶段进程的完整特权配置（对应 C 的 `s_flags` / `s_trap_mask`
/// / `s_ipc_to` / `s_k_call_mask` / `s_sig_mgr` 的集合）。
///
/// 设计原则：
/// - 只暴露"这是一个什么类型进程"的 API，不暴露"哪个 bit 是什么"。
/// - bit 布局在 `KPriv` 内部定义，此处不关心。
pub enum BootPrivTemplate {
    /// 内核 task：允许 I/O，有限 trap。
    KernelTask,
    /// RS 进程：可向所有系统进程发 IPC。
    RootSystem,
    /// VM 进程：独占 MMU 操作权限。
    Vm,
    /// 普通系统进程（由 RS 在运行时补齐）。
    /// boot 阶段先标记为"有 slot 但无权限"。
    SystemService,
}

impl BootPrivTemplate {
    /// 把模板应用到一个 `KPriv`。kernel 层唯一入口。
    pub fn apply(&self, priv_: &mut KPriv) {
        match self {
            BootPrivTemplate::KernelTask => {
                priv_.set_flags(KPrivFlags::from_bits_truncate(0x01)); // IOPL 等下沉到 KPriv
                priv_.set_trap_mask(TrapMask::kernel_default());
                priv_.set_ipc_to(IpcBitmap::all_system_procs());
                priv_.set_k_call_mask(KCallBitmap::kernel_default());
            }
            BootPrivTemplate::Vm => { /* ... 同 C 版 VM 的位布局 ... */ }
            // ... 其他模板 ...
        }
    }
}
```

这把"翻译 C 函数签名 + 裸整数 bit mask"变成"应用一个模板"——**rewrite 风格**。

---

## 3. 重新设计：Arch 抽象层（一个 trait）

### 3.1 唯一 arch trait：`ArchBootProc`

```rust
// ✅ os/arch/src/arch/proc_boot.rs  (替代旧的 proc_arch.rs)

//! 启动一个进程的架构抽象。
//!
//! # 设计
//! - **一个 trait**：启动一个进程只有一个 OS 概念（"把 CPU 设置成某个初始状态"）。
//! - **关联类型 `ArchState`**：编译时定型。非当前架构的字段在类型系统中不存在。
//! - **Kernel 不拆解 `ArchState`**：kernel 只把它当作 opaque blob 整体存储/整体应用。
//! - **纯函数**：`build_state` 不修改进程（没有 `&mut KProcess` 参数）；
//!   对进程的修改在 `apply_to_trap_frame` 里进行（且参数类型是 arch-specific
//!   `TrapFrame`，由 arch crate 定义）。

use minix_types::VirBytes;

/// 架构层启动进程的抽象。**一个 crate 只实现一次**（通过 `cfg(target_arch)`
/// 或条件编译选择具体实现）。
pub trait ArchBootProc {
    /// 由架构层自定义的"启动状态"。
    ///
    /// 约束：
    /// - **必须 `const Default`（或等价 `const fn empty()`）**，
    ///   因为 kernel 层在 `ProcessTable::new()` 里需要编译期初始化。
    /// - `Copy + 'static`：作为 `KProcess.boot_cfg` 的字段内嵌存储。
    type ArchState: Copy + 'static;

    /// 为一个"空 slot"构造初始状态（`proc_init()` 阶段调用）。
    /// 等价于 C 的 `arch_proc_reset()` 语义。
    fn empty_slot_state() -> Self::ArchState;

    /// 为一个即将启动的进程构造完整启动状态（boot 循环中调用）。
    ///
    /// `is_kernel`: 是否为内核 task（`p_nr < 0`）。
    /// `proc_nr`: 进程号（x86-64 上决定 FPU 静态区索引；其他架构忽略）。
    ///
    /// 等价于 C 的 `arch_proc_init()` + `arch_proc_reset()` 的组合语义。
    /// 调用方**不再需要记住调用顺序**——这个函数内部按正确顺序组装。
    fn build_boot_state(is_kernel: bool, proc_nr: i32) -> Self::ArchState;

    /// 把启动状态写入 trap frame。**仅在上下文切换到该进程第一次运行前调用一次**。
    ///
    /// 参数 `frame` 的类型是 arch-specific（每个 crate 自己定义），kernel 层
    /// 通过 `type CurrentTrapFrame = ...` 转发它。
    fn apply_to_trap_frame(
        state: &Self::ArchState,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        frame: &mut CurrentTrapFrame,
    );

    /// 为用户进程的 FPU / 扩展寄存器做"初始可用"标记。
    ///
    /// # 设计说明
    ///
    /// 不同架构的 FPU 策略截然不同：
    /// - x86-64: XSAVE area 分配 + `MF_FPU_INITIALIZED` 标志 + CR0.TS lazy。
    /// - aarch64: CPACR_EL1.FPEN = trap on EL0 access（在第一次 FP 指令时真正分配）。
    /// - riscv64: sstatus.FS = Initial（第一次 FP 指令 trap）。
    ///
    /// kernel 层**不关心这些差异**。只需要一个语义："这个进程可以使用浮点了"。
    fn init_fpu_capability(proc: &mut KProcess);
}

/// 当前架构下的启动配置（OS 层类型，不包含 arch 字段）。
///
/// 这是 kernel 层唯一能"看到"的 arch 相关类型——它把 `ArchState` 作为
/// 泛型参数嵌入，使得 `boot_cfg.segment_selectors` 在 `arm64/riscv64` 上
/// **编译期就不存在**。
#[derive(Clone, Copy)]
pub struct BootConfig<A: ArchBootProc> {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub arch_state: A::ArchState,
}

impl<A: ArchBootProc> BootConfig<A> {
    pub fn new(pc: VirBytes, sp: VirBytes, ps_strings: VirBytes, arch_state: A::ArchState) -> Self {
        Self { pc, sp, ps_strings, arch_state }
    }

    /// 空 slot 用的默认值。`ArchState::empty_slot_state()` 保证 const 可构造。
    pub const fn empty_slot() -> Self {
        Self {
            pc: VirBytes(0),
            sp: VirBytes(0),
            ps_strings: VirBytes(0),
            arch_state: A::empty_slot_state(),
        }
    }
}

/// 对外暴露的"当前架构"具体类型别名。
/// kernel 层只使用这两个类型名：
///
/// ```rust
/// use os_arch::proc_boot::{CurrentBootConfig, CurrentBootArch};
/// ```
pub type CurrentBootConfig = BootConfig<CurrentBootArch>;

// `CurrentBootArch` 由 `os/arch/src/lib.rs` 通过 `cfg(target_arch)` 选择：
//   pub type CurrentBootArch = X86_64BootProc;   // x86_64
//   pub type CurrentBootArch = Aarch64BootProc;   // aarch64
//   pub type CurrentBootArch = Riscv64BootProc;   // riscv64
//
// 以及：
//   pub type CurrentTrapFrame = X86_64TrapFrame;  // 等
```

### 3.2 x86-64 具体实现：`ArchState` 里包含所有 x86-64 特有字段

```rust
// ✅ os/arch/src/x86_64/proc_boot.rs

use minix_types::VirBytes;
use crate::proc_boot::{ArchBootProc, CurrentTrapFrame, X86_64TrapFrame, KProcess};

/// x86-64 启动状态。**只在 x86-64 编译单元里出现**。
/// aarch64/riscv64 的 `ArchState` 结构完全不同，且不会出现 `segment_selectors` 字段。
#[derive(Debug, Clone, Copy)]
pub struct X86_64BootState {
    pub rflags: u64,            // 状态寄存器（RFLAGS）
    pub segment_selectors: X86_64SegSelectors, // x86-64 特有
    pub fpu_init_strategy: X86_64FpuStrategy,  // FPU 策略（非 bool）
}

#[derive(Debug, Clone, Copy)]
pub struct X86_64SegSelectors {
    pub cs: u16,  // 使用 u16 而非 u64——段选择子就是 16 位
    pub ds_ss_es_fs_gs: u16,  // 平坦模型下其余一致，合并一个字段
}

/// FPU 初始化策略。不是"需要清零吗"，而是"用哪种方式把 FPU 带到可用状态"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X86_64FpuStrategy {
    /// 内核 task：不使用 FPU，无需初始化。
    Unused,
    /// 用户进程：分配 XSAVE area（lazy init），设置 `MF_FPU_INITIALIZED`。
    /// `XSAVE` 上下文在第一次 FP 指令 trap 时真正保存。
    XsaveLazy,
}

pub struct X86_64BootProc;

impl ArchBootProc for X86_64BootProc {
    type ArchState = X86_64BootState;

    fn empty_slot_state() -> Self::ArchState {
        X86_64BootState {
            rflags: 0,
            segment_selectors: X86_64SegSelectors {
                cs: USER_CS_SELECTOR,
                ds_ss_es_fs_gs: USER_DS_SELECTOR,
            },
            fpu_init_strategy: X86_64FpuStrategy::Unused,
        }
    }

    fn build_boot_state(is_kernel: bool, _proc_nr: i32) -> Self::ArchState {
        X86_64BootState {
            rflags: if is_kernel { INIT_TASK_PSW } else { INIT_PSW },
            segment_selectors: X86_64SegSelectors {
                cs: USER_CS_SELECTOR,
                ds_ss_es_fs_gs: USER_DS_SELECTOR,
            },
            fpu_init_strategy: if is_kernel {
                X86_64FpuStrategy::Unused
            } else {
                X86_64FpuStrategy::XsaveLazy
            },
        }
    }

    fn apply_to_trap_frame(
        state: &Self::ArchState,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        frame: &mut X86_64TrapFrame,
    ) {
        frame.rip = pc.0;
        frame.rsp = sp.0;
        frame.rbx = ps_strings.0;
        frame.rflags = state.rflags;
        frame.cs = state.segment_selectors.cs as u64;
        frame.ss = state.segment_selectors.ds_ss_es_fs_gs as u64;
        // 其他段寄存器在 trap frame 写入时由其他路径处理；
        // 此处只关心"启动第一次运行"需要的寄存器。
    }

    fn init_fpu_capability(proc: &mut KProcess) {
        // x86-64: 为用户进程分配 XSAVE area；设置 `MF_FPU_INITIALIZED`；
        // 保持 CR0.TS 以便第一次 FP 指令触发 trap 然后 lazy save。
        // kernel task 不使用 FPU，不做任何事。
        if !proc.is_kernel_task() {
            proc.misc_flags.set_fpu_initialized();
            // 真正的 XSAVE area 分配在 arch 层的静态区域（类似 C 版的
            // `static char fpu_state[NR_PROCS][FPU_XFP_SIZE]`），由 arch 层
            // 内部管理，不暴露给 kernel 层。
        }
    }
}
```

**设计要点**：
- `segment_selectors` 现在**只存在于 `X86_64BootState`**。`aarch64/riscv64` 上这个字段不存在。
- `fpu_needs_zero: bool` 变成 `X86_64FpuStrategy::XsaveLazy`——表达"做什么"而非"要不要清零"。aarch64/riscv64 用自己的策略类型（见下）。
- `apply_to_trap_frame` 接收 x86-64 专属的 `X86_64TrapFrame`，内部按正确顺序写入所有寄存器——调用方不再需要"先 reset 再 init"这种顺序意识。

### 3.3 aarch64 实现

```rust
// ✅ os/arch/src/arm64/proc_boot.rs

use minix_types::VirBytes;
use crate::proc_boot::{ArchBootProc, Aarch64TrapFrame, KProcess};

/// aarch64 启动状态。**无 segment_selectors / fpu_needs_zero**。
#[derive(Debug, Clone, Copy)]
pub struct Aarch64BootState {
    pub spsr_el1: u64,  // 状态寄存器（EL1h / EL0t）
    pub fpu_strategy: Aarch64FpuStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aarch64FpuStrategy {
    Unused,
    /// CPACR_EL1.FPEN = 0b01 （EL0 访问 FP 会 trap 到 EL1，lazy 分配）。
    FpenTrapOnEl0Access,
}

pub struct Aarch64BootProc;

impl ArchBootProc for Aarch64BootProc {
    type ArchState = Aarch64BootState;

    fn empty_slot_state() -> Self::ArchState {
        Aarch64BootState { spsr_el1: 0, fpu_strategy: Aarch64FpuStrategy::Unused }
    }

    fn build_boot_state(is_kernel: bool, _proc_nr: i32) -> Self::ArchState {
        Aarch64BootState {
            spsr_el1: if is_kernel { INIT_TASK_SPSR_EL1H } else { INIT_SPSR_EL0T },
            fpu_strategy: if is_kernel {
                Aarch64FpuStrategy::Unused
            } else {
                Aarch64FpuStrategy::FpenTrapOnEl0Access
            },
        }
    }

    fn apply_to_trap_frame(
        state: &Self::ArchState,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        frame: &mut Aarch64TrapFrame,
    ) {
        frame.elr_el1 = pc.0;
        frame.sp_el0 = sp.0;
        frame.regs[0] = ps_strings.0;  // aarch64 约定 ps_strings 放在 r0
        frame.spsr_el1 = state.spsr_el1;
    }

    fn init_fpu_capability(proc: &mut KProcess) {
        if !proc.is_kernel_task() {
            // CPACR_EL1.FPEN = 0b01（EL0 访问 FP 触发 trap）
            // 真正的 save/restore 在 trap handler 中做。
            // kernel 层看不到 CPACR 寄存器。
        }
    }
}
```

### 3.4 riscv64 实现

```rust
// ✅ os/arch/src/riscv64/proc_boot.rs
// 与 aarch64 对称。ArchState 包含 sstatus + FpuStrategy（FS=Initial）。
// 代码结构同上，略。
```

### 3.5 `load_vm_elf` 移出 trait：它是纯 OS 功能，与架构无关

`06-problem.md` §10 指出：`BootProcArch::load_vm_elf` 的三个 arch 实现**完全相同**——解析 ELF、分配物理页、通过 `Paging` trait 映射。这是**架构无关**的逻辑，不该放在 arch trait 里。

**重设计**：改为 arch crate 的一个**普通模块函数**（因为它依赖 arch 的 `Paging` 实现），但它是**所有架构共享的代码**——放在 `os/arch/src/arch/vm_elf_loader.rs`，不与 `ArchBootProc` trait 绑定。

```rust
// ✅ os/arch/src/arch/vm_elf_loader.rs

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use crate::paging::Paging;

pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

/// 把 VM 的 ELF 镜像加载到 bootstrap 页表。
///
/// 架构无关：只依赖 `Paging` trait。x86-64 / aarch64 / riscv64 共用同一份代码。
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmLoadResult {
    // 解析 ELF（minix-elf crate）→ 遍历 PT_LOAD → pg_map(paging trait) → 建立栈 → 返回入口/栈/ps_strings
    // （与当前 x86_64 实现的主体逻辑一致，只是不放在 trait 里，也不在 per-arch 文件里）
    //
    // 关键差异：当前代码里 `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs`
    // 每个文件都复制了一份这段逻辑 —— 这是 translate 导致的冗余。
    // 新设计把它整合到一个文件。
    //
    // VM 的用户栈大小、ps_strings 布局，都是 OS 级常量，与 arch 无关。
    // 见 `06-proc-init-boot-proc.md` §1.5 的设计说明（bootstrap 页表 + VM 映射）。
    let _ = (module, kernel_info, paging); // placeholder
    VmLoadResult { pc: VirBytes(0), sp: VirBytes(0), ps_strings: VirBytes(0), allocated_bytes: 0 }
}
```

---

## 4. Kernel 层的调用方：`init_proc_and_boot()`（单一入口）

当前实现把 proc_init + boot 循环分散在多个文件。新设计提供**一个内核级入口函数**，清晰呈现 OS 概念级顺序：

```rust
// ✅ os/kernel/src/boot.rs  (新增)

use crate::proc_table::ProcessTable;
use crate::kpriv::PrivTable;
use crate::boot_priv::BootPrivTemplate;
use os_arch::proc_boot::{CurrentBootArch, ArchBootProc};
use os_arch::vm_elf_loader::load_vm_elf;
use minix_boot::{BootInfo, BootModule, KernelInfo};

/// 内核启动阶段的"进程表初始化 + boot image 循环"唯一入口。
///
/// 概念顺序（与 C 的 `proc_init() → main.c boot loop` 一致）：
///   1. 清空进程表（slot = SLOT_FREE，arch 层空状态）。
///   2. 清空特权表。
///   3. 遍历 boot image，为每个进程：
///        a. 决定 schedulable → 分配静态特权 + 应用模板。
///        b. VM 进程：加载 ELF 到 bootstrap 页表。
///        c. 组装 `BootConfig`（pc/sp/ps_strings + arch_state）。
///        d. 设置 RTS flags（PROC_STOP, 清除 SLOT_FREE）。
pub fn init_proc_and_boot(
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
    boot_info: &BootInfo,
    bootstrap_paging: &mut impl Paging,
) {
    // --- 3.a VM 加载（架构无关 ELF loader） ---
    let vm_module = boot_info.modules.iter().find(|m| m.is_vm);
    let vm_load = vm_module.map(|m| load_vm_elf(m, &boot_info.kernel_info, bootstrap_paging));

    // --- 3.b 遍历 boot image (概念级循环，不翻译 C 函数) ---
    for (idx, entry) in boot_info.boot_image.iter().enumerate() {
        let nr = entry.proc_nr;
        let proc = proc_table.get_mut(nr).expect("boot image proc_nr out of range");

        let is_kernel = nr < 0;

        // 特权模板（OS 概念级 API，不暴露 bit）
        let template = match () {
            _ if is_kernel => BootPrivTemplate::KernelTask,
            _ if nr == proc_nr::VM => BootPrivTemplate::Vm,
            _ if nr == proc_nr::RS => BootPrivTemplate::RootSystem,
            _ => BootPrivTemplate::SystemService,
        };
        if let Some(priv_id) = priv_table.assign_static(nr) {
            if let Some(priv_) = priv_table.get_mut(priv_id) {
                template.apply(priv_);
                proc.p_priv = Some(priv_id);
            }
        }

        // 启动配置（arch 组装状态；kernel 只做整体存储）
        let arch_state = CurrentBootArch::build_boot_state(is_kernel, nr);
        let (pc, sp, ps_strings) = if nr == proc_nr::VM {
            if let Some(r) = &vm_load { (r.pc, r.sp, r.ps_strings) } else { (VirBytes(0), VirBytes(0), VirBytes(0)) }
        } else {
            (entry.pc, entry.sp, VirBytes(0))
        };
        proc.set_boot_config(pc, sp, ps_strings, arch_state);

        // FPU: arch 层内部处理
        if !is_kernel {
            CurrentBootArch::init_fpu_capability(proc);
        }

        // RTS flags（OS 语义位图；不暴露 bit layout）
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        if !is_kernel && nr != proc_nr::VM {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT);
            proc.p_rts_flags.set(RtsFlagsBits::BOOTINHIBIT);
        }

        // 进程名（来自 boot image；内核 task 硬编码）
        if let Some(name) = entry.name {
            proc.p_name = ProcName::from_str(name);
        }

        let _ = idx;
    }
}
```

---

## 5. 调度时的 trap frame 写入：kernel 层只有"整体应用"

```rust
// os/kernel/src/context_switch.rs  (示意)

use os_arch::proc_boot::{CurrentBootArch, ArchBootProc, CurrentTrapFrame};
use crate::proc::KProcess;

/// 第一次调度到 `proc` 时，把它的启动配置写入 trap frame。
/// 后续上下文切换走正常 save/restore 路径（非本文范围）。
pub fn setup_first_run_trap_frame(proc: &KProcess, frame: &mut CurrentTrapFrame) {
    // 🔑 关键：kernel 层没有拆解字段，只调用一次 arch 层 API。
    // `CurrentBootArch::apply_to_trap_frame` 内部决定"写入哪些寄存器、什么顺序"。
    CurrentBootArch::apply_to_trap_frame(
        &proc.boot_cfg.arch_state,
        proc.boot_cfg.pc,
        proc.boot_cfg.sp,
        proc.boot_cfg.ps_strings,
        frame,
    );
}
```

---

## 6. 迁移路径（Refactor Roadmap）

### 6.1 文件变更清单

| 旧文件 | 操作 | 新文件 | 说明 |
|-------|-----|-----|-----|
| `os/arch/src/arch/proc_arch.rs` | 删除 | `os/arch/src/arch/proc_boot.rs` | 一个 trait：`ArchBootProc` + `BootConfig` 泛型 |
| `os/arch/src/x86_64/proc_arch.rs` | 删除 | `os/arch/src/x86_64/proc_boot.rs` | `X86_64BootState` + `X86_64BootProc` impl |
| `os/arch/src/arm64/proc_arch.rs` | 删除 | `os/arch/src/arm64/proc_boot.rs` | `Aarch64BootState` + impl |
| `os/arch/src/riscv64/proc_arch.rs` | 删除 | `os/arch/src/riscv64/proc_boot.rs` | `Riscv64BootState` + impl |
| （三文件重复代码） | 新 | `os/arch/src/arch/vm_elf_loader.rs` | `load_vm_elf()` 普通函数，三架构共享 |
| `os/kernel/src/proc.rs` | 修改 | | 四裸字段 → `boot_cfg: CurrentBootConfig` |
| `os/kernel/src/proc_table.rs` | 修改 | | `Box<[KProcess]>` → `[KProcess; N]`；`const fn new()` |
| `os/kernel/src/kpriv.rs` | 修改 | | `Box<[KPriv]>` → `[KPriv; N]`；`const fn new()` |
| （新） | 新 | `os/kernel/src/boot_priv.rs` | `BootPrivTemplate` 模板 API |
| （新） | 新 | `os/kernel/src/boot.rs` | `init_proc_and_boot()` 统一入口 |
| `os/kernel/src/lib.rs` | 修改 | | 删除旧的逐字段调用；改走 `init_proc_and_boot()` |

### 6.2 迁移顺序（后一步依赖前一步完成）

1. **Step 1（安全，可独立合并）**：把 `ProcessTable`/`PrivTable` 的 `Box<[T]>` 改成 `[T; N]` + `const fn`。不改变任何抽象。
2. **Step 2（安全）**：把 `ArchProcReset::initial_reg_state` 的返回值从 `InitialRegState` 改成 arch-specific 关联类型（初步引入关联类型机制）。
3. **Step 3（破坏性）**：把 `ArchProcReset + ArchProcInit + BootProcArch` 三 trait 合并为 `ArchBootProc`。
4. **Step 4（破坏性）**：把 `KProcess` 的四个裸字段替换为 `boot_cfg: CurrentBootConfig`。
5. **Step 5（安全）**：把 `load_vm_elf` 从三文件移到 `vm_elf_loader.rs`。
6. **Step 6（文档）**：重写 `06-proc-init-boot-proc.md` 的 §3 和 §4。

每一步都跑 `cargo test --package os-kernel` + `cargo test --package os-arch`（在有 mock 的情况下）。

---

## 7. 验证不变量（Verification Invariants）

这些是"完成重设计后必须成立的属性"，用于其他 AI 或 review 阶段做 bagging 比对。

### 7.1 `grep -n "SegmentSelectors\|fpu_needs_zero\|InitialRegState"` 验证

| 文件 | 期望 grep 结果 |
|-----|-------------|
| `os/arch/src/x86_64/proc_boot.rs` | `X86_64SegSelectors` 存在 ✓ |
| `os/arch/src/arm64/proc_boot.rs` | **零匹配** ✗→✓（迁移前应当匹配不到） |
| `os/arch/src/riscv64/proc_boot.rs` | **零匹配** |
| `os/kernel/src/**` | **零匹配**（kernel 层没有任何 x86 特有字段名） |

### 7.2 `grep -n "ArchProcReset\|ArchProcInit\|BootProcArch"` 验证

所有旧 trait 名在整个 `os/` 目录下**零匹配**（除可能在历史 commit 中）。唯一的 arch trait 是 `ArchBootProc`。

### 7.3 `alloc::` 在 boot 阶段不可达

```text
grep -n "use alloc::\|use std::\|Box::\|Vec::\|format\!" os/kernel/src/boot.rs
=> 零匹配
```

`ProcessTable::new()` 与 `PrivTable::new()` 必须是 `const fn`（编译期构造）。

### 7.4 `CurrentBootConfig` 类型别名在 kernel 层唯一可见

```rust
// os/kernel/src/**/*.rs 中 arch 相关使用应当只出现这两个名字：
//   CurrentBootConfig
//   CurrentBootArch
// 不出现 X86_64BootState / Aarch64BootState / SegmentSelectors 等 arch 具体名。
```

---

## 8. FAQ / 设计权衡

### Q1：为什么 `BootConfig` 不把 `arch_state` 也包成 OS 语义字段？

**答**：我们做不到——`arch_state` 的**内容**就是 arch 相关的（段选择子、RFLAGS、XSAVE 策略）。把它们命名为更"OS 语义"的名字只会制造"看起来通用但实际上只在一个架构上有意义"的伪抽象。**正确的做法是让这些字段在编译时在非目标架构上直接不存在**——这正是关联类型 `ArchState` 提供的保证。

### Q2：为什么 trait 数不是 0？为什么不直接用裸 `cfg` 条件编译？

**答**：`trait + 关联类型` 提供的是**类型系统的契约**——它强制每个架构都实现同一组方法。裸 `cfg` 会导致"某个架构漏了一个方法"直到编译到那个架构时才发现。trait 在编译 host 架构时就把契约检查了。此外，`#[cfg(feature = "mock")]` 下可以提供 `MockBootProc` 实现用于单元测试，这也是裸 `cfg` 做不到的。

### Q3：为什么 `load_vm_elf` 不是 `ArchBootProc` 的关联函数？

**答**：它在三架构上**实现完全相同**（解析 ELF + `Paging` trait 映射）。把它放进 trait 会给每个架构制造"空泛的 impl 负担"——这就是 `06-problem.md` §10 指出的"不必要的 trait 抽象"。改成普通函数后，代码量减少 2/3（三实现 → 一实现），同时保留 `P: Paging` 的泛型参数，让 mock 测试仍然可用。

### Q4：`empty_slot_state()` 为什么要求能 `const` 构造？

**答**：`ProcessTable::new()` 必须是 `const fn`（或至少在 boot 阶段无堆）。如果 `ArchState` 不能 `const` 初始化，就无法在 `[KProcess; N]` 的 `const { ... }` 重复表达式里构造。这是一个**实现级硬约束**，不是设计偏好。**如果某个架构的 `ArchState` 无法 `const`（因为它需要运行时计算），解决方案**：

- 用 `MaybeUninit<KProcess>` 数组 + 运行时 `ManuallyDrop` 初始化；或
- 用 `ArchState = MaybeUninit<RealArchState>`，在 `build_boot_state` 时真正初始化。

**推荐**：前者（`MaybeUninit` 数组 + 运行时初始化），对 kernel 层干净；但仍需保证**不触发 `alloc`**。

### Q5：`ps_strings_reg: u64` 改成 `ps_strings: VirBytes` 之后，"存哪个寄存器"的信息去哪了？

**答**：这个信息属于 arch 层——`apply_to_trap_frame()` 里决定写入哪个寄存器（x86-64 → rbx；aarch64 → r0；riscv64 → a0）。kernel 层只需要知道"这是 ps_strings 的虚拟地址"——**不知道也不应该知道**它会被放进哪个寄存器。这正是"硬件语义零泄漏"的直接体现。

### Q6：trait bound 从哪里来？`ArchBootProc` 会被用作 `T: ArchBootProc` 吗？

**答**：在 kernel 层**不会**——kernel 层只用 `CurrentBootArch`（一个具体类型别名）。在测试中（`#[cfg(test)]` 或 `#[cfg(feature = "mock")]`），**会**——`MockBootProc` 用 `T: ArchBootProc` 做泛型测试。这就够了：**trait 存在的目的是"契约"（contract），不是"运行时分发"**。

---

## 9. 设计自检（Self-Check against 06-problem.md）

| 06-problem.md 列出的问题 | 本设计如何解决 | 节 |
|----------------------|-------------|----|
| #1：`segment_selectors` 在 aarch64/riscv64 永远全零 | `X86_64BootState` 独享字段；其他架构 `ArchState` 根本无此字段 | §3.1/3.2 |
| #2：`InitialRegState.fpu_needs_zero` 翻译 C 的 memset 思维 → 现代架构 lazy init | `X86_64FpuStrategy::XsaveLazy` / `Aarch64FpuStrategy::FpenTrapOnEl0Access` / `Riscv64FpuStrategy::FsInitial` | §3.2/3.3 |
| #3：kernel 层拆解 arch 返回值的逐字段存储 | `BootConfig` 整体存储 + `apply_to_trap_frame` 整体应用 | §2.2/§5 |
| #4：同样问题适用于 `InitialRegs` | `BootConfig` 的 `pc/sp/ps_strings` 直接替换 `InitialRegs` | §2.2 |
| #5：`initial_status` 字段 + 注释泄漏 x86-64 IOPL 到 kernel | `initial_status` 字段删除；IOPL 由 `X86_64BootState.rflags` 在 arch 层内部计算 | §2.2 + §3.2 |
| #6：文档自称 OS-semantic 但 `segment_selectors` 暴露硬件 | 新文档里 `BootConfig` 的描述是"整体启动配置"，不列举硬件字段；`ArchState` 明确标注为 arch-internal | 文档部分待迁移 |
| #7：`SegmentSelectors::default()` 零值哨兵 | 删除 `SegmentSelectors` 这个共享类型；x86-64 自用 `X86_64SegSelectors`，无 default 哨兵 | §3.2 |
| #8：FPU 翻译 C 的 memset，未考虑 XSAVE/CPACR/sstatus.FS | `init_fpu_capability()` + 各架构策略枚举；用户进程 lazy init | §3.1 (`init_fpu_capability`) |
| #9："init_regs 内部调用 reset"——编造的因果链 + 调用顺序意识 | `build_boot_state` 单一入口：顺序在 arch 层内部消化；kernel 层不了解"reset / init" | §3.2 `build_boot_state` |
| #10：三 trait 翻译三 C 函数 | 合并为 `ArchBootProc` 一个 trait；`load_vm_elf` 降为普通函数 | §3/§3.5 |
| #11：`ProcessTable`/`PrivTable` 用 `Box<[T]>` + `Vec::collect()` —— boot 阶段无 allocator | `[T; N]` 固定数组 + `const fn new()` | §2.3 |
| #12：`configure_boot_priv` 6 个裸参数 | `BootPrivTemplate` 模板 API，kernel 层"应用一个模板" | §2.4 |

---

## 10. 对文档 `06-proc-init-boot-proc.md` 的重写提示（给文档作者）

- **§3**（Rust 设计决策）：整节重写。改为：
  - §3.1 设计原则（零泄漏 / rewrite / 零堆）。
  - §3.2 `BootConfig` + `ArchBootProc` 的类型图（kernel 层只看 `CurrentBootConfig` 别名）。
  - §3.3 各架构 `ArchState` 的差异表（对比而非翻译 C 实现）。
- **§4**（实现详解）：与 §3 对齐。旧的 `InitialRegState` / `InitialRegs` / `ArchProcReset` / `ArchProcInit` 章节全部删除，替换为 `ArchBootProc` + `BootPrivTemplate` + `ProcessTable` 的说明。
- **§1.7**（三架构对照）：已经是"概念级对照"——不需要改。但要确保不再把"段选择子"说成是三架构都存在的东西。

---

> **最终判定**：本设计的核心变换是**"共享结构体 + 三 trait → 关联类型 + 单一 trait + 普通函数"**。它解决了 `06-problem.md` 中 12 个问题的根因，同时为测试保留了泛型约束的灵活性。代码量预计减少约 40%（三文件重复 `load_vm_elf` → 一文件；三 trait 定义 → 一个 trait），并在非目标架构上把硬件字段从"永远为零但存在"变成"根本不存在"。
