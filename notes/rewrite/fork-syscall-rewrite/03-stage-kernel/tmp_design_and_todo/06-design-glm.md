# 06-proc-init-boot-proc.md — Rust 重设计（GLM 独立版）

> **状态**：独立设计稿（多 AI bagging 阶段，未参考其他 AI 的设计文档）
> **范围**：仅 06 文档对应的 Rust 设计与实现（arch 层 + kernel 层的 proc init / boot proc 部分）
> **ground truth**：Minix3 源码 `minix3/minix/kernel/{proc.c,main.c,system.c,arch/i386/*,arch/earm/*}`
> **遵循原则**：review-rules 最高原则（OS 与硬件解耦、rewrite 而非 translate、boot 阶段不用堆）

---

## 0. 设计原则（强制）

本设计从以下 OS 概念出发，不从"C 函数对应关系"出发：

| 原则 | 含义 | 违反时的症状 |
|------|------|------------|
| **P1 OS 层只见 OS 概念** | kernel crate 的类型/字段/方法名都是 OS 语义（capability、startup、slot），不是硬件术语（segment、fpu、psw） | `segment_selectors` 出现在 arm64 代码 |
| **P2 arch 层 owns CPU state** | CPU 状态是 arch 内部事务，OS 只存储不透明值、arch 自己应用 | kernel 拆解 arch 返回值、`fpu_needs_zero` 流经 kernel |
| **P3 trait 用于真多态** | trait 仅当"≥2 个实现行为不同 + 被用作 bound"时使用；否则用具体类型 + 类型别名 | 3 个 trait 对应 3 个 C 函数、`load_vm_elf` 三架构完全相同 |
| **P4 boot 阶段零堆** | `init_proc_and_boot` 执行时无 `GlobalAlloc`，所有数据结构编译期固定大小 | `Box<[KProcess]>` / `Box<[KPriv]>` |
| **P5 能力是数据，不是裸参数** | 权限/能力打包为结构体/枚举，不用 6 个 `u16/u32/u64` 裸参数 | `configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` |
| **P6 FPU 是 arch 演进问题** | 不翻译 Minix3 的 fnsave/fxrstor 模型；x86-64 用 XSAVE、aarch64 用 CPACR_EL1.FPEN、riscv64 用 sstatus.FS | `fpu_needs_zero: bool` 字段泄漏到 OS 层 |

---

## 1. 问题诊断（简述）

详细问题清单见 [06-problem.md](./06-problem.md)。本设计直接修复以下 P0/P1：

| # | 问题 | 严重度 | 本设计的修复节 |
|---|------|-------|------------|
| #1 | `segment_selectors` / `fpu_needs_zero` 泄漏到 OS 层 | P0 | §3.1（关联类型 `StartupState`） |
| #8 | `fpu_needs_zero` 翻译 Minix3 fnsave 模型 | P0 | §3.3（FPU 完全下沉到 arch） |
| #9 | "init_regs 内部调用 reset" 因果链编造 | P1 | §3.2（单方法 `build_startup_state`） |
| #10 | 3 个 trait 翻译 3 个 C 函数 | P0 | §3.1（合并为单 trait `BootArch`） |
| #11 | `Box<[KProcess]>` / `Box<[KPriv]>` 用堆 | P0 | §4.1（固定大小数组 + const fn） |
| #12 | §3.4 翻译味 + 6 个裸参数 | P1 | §4.3（`ProcessCapability` + `CapabilityTemplate`） |

---

## 2. 新设计概览

### 2.1 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│  kernel 层（os/kernel/）— 只见 OS 概念                       │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ ProcessTable │  │  PrivTable   │  │     KProcess      │  │
│  │ [KProc; N]   │  │ [KPriv; N]   │  │ startup_state:    │  │
│  │ const fn new │  │ const fn new │  │   CurrentStartup  │  │
│  └──────────────┘  └──────────────┘  └───────────────────┘  │
│         │                  │                ▲                │
│         ▼                  ▼                │ 不透明值        │
│  ┌──────────────────────────────────────────┴──────────────┐ │
│  │  init_proc_and_boot()  ← 主流程（OS 概念编排）            │ │
│  └─────────────────────────────────────────────────────────┘ │
│                          ▲ opaque                            │
└──────────────────────────┼──────────────────────────────────┘
                           │
┌──────────────────────────┼──────────────────────────────────┐
│  arch 层（os/arch/）      │ owns CPU state                   │
│  ┌───────────────────────┴────────────────────────────────┐ │
│  │  trait BootArch  (唯一 trait，2 个方法)                 │ │
│  │  • type StartupState  (关联类型，arch 各自定义)         │ │
│  │  • type TrapFrame     (关联类型，arch 各自定义)         │ │
│  │  • build_startup_state(ProcKind, ProcNr, EntrySpec)    │ │
│  │      -> Self::StartupState                             │ │
│  │  • apply_to_trap_frame(&StartupState, &mut TrapFrame)  │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  free fn load_vm_elf<P: Paging>(...) -> VmLoadResult   │ │
│  │  （三架构共享，不在 trait 里——没有多态）                │ │
│  └────────────────────────────────────────────────────────┘ │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────┐       ┌──────────┐       ┌──────────┐         │
│  │ x86_64   │       │  arm64   │       │ riscv64  │         │
│  │ BootArch │       │ BootArch │       │ BootArch │         │
│  └──────────┘       └──────────┘       └──────────┘         │
│  Each arch owns (内部，OS 看不到):                           │
│  - StartupState 字段（psw/seg_sel/xsave_state/rip/rsp/...） │
│  - TrapFrame 布局                                           │
│  - FPU 初始化逻辑（XSAVE / CPACR_EL1 / sstatus.FS）        │
└────────────────────────────────────────────────────────────┘
```

### 2.2 与 C 函数的对应关系（不是"翻译"，是"职责重新分配"）

| C 函数 | OS 概念 | Rust 落地 |
|--------|--------|---------|
| `proc_init()` | "进程表初始化为全空槽" | `ProcessTable::new()` const 构造 |
| `arch_proc_reset()` | "为新进程构建初始 CPU 状态" | `BootArch::build_startup_state(ProcKind::KernelTask, ...)` |
| `arch_proc_init()` | "为用户进程构建带入口点的 CPU 状态" | `BootArch::build_startup_state(ProcKind::Vm, EntrySpec::Loaded{...})` |
| `arch_boot_proc()` | "加载 VM ELF + 构建启动状态" | free fn `load_vm_elf()` + `build_startup_state()` |
| `get_priv()` + 特权设置 | "为进程授予能力" | `PrivTable::grant_capability(nr, CapabilityTemplate::Vm)` |
| boot image 循环 | "按角色编排所有 boot 进程" | `init_proc_and_boot()` 主流程 |

**关键区别**：C 版的 3 个 arch 函数（reset/init/boot_proc）是**实现细节**，Rust 版不照搬这个划分。Rust 版的划分基于 OS 概念："构建状态"（`build_startup_state`）和"应用状态"（`apply_to_trap_frame`）是两个正交的 OS 操作。

---

## 3. arch 层设计

### 3.1 `BootArch` trait — 唯一的 arch 抽象

```rust
// os/arch/src/arch/boot.rs

use minix_types::VirBytes;
use minix_boot::{BootModule, KernelInfo};
use crate::paging::Paging;

/// 进程角色（OS 概念，不是硬件概念）。
///
/// 替代当前代码里的 `is_kernel: bool` + 隐式的 `is_vm` / `is_root_sys` 判断。
/// arch 层根据角色决定初始 PSW/PSR/sstatus、段选择子、FPU 初始化策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcKind {
    /// 内核任务（CLOCK/SYSTEM/IDLE/KERNEL）：运行在内核态，无 ELF
    KernelTask,
    /// VM 进程：运行在用户态，boot 期加载 ELF
    Vm,
    /// 根系统服务（RS）：运行在用户态，boot 期不加载 ELF（RS 自己加载）
    RootService,
    /// 其他系统服务：boot 期不加载 ELF，由 RS 在运行时加载
    UserService,
    /// 用户进程：boot 期不存在，由 fork/exec 创建
    UserProcess,
}

/// 入口点规格（OS 概念：进程的"第一条指令"在哪里）。
///
/// 替代当前代码里的 `pc: VirBytes, sp: VirBytes, ps_strings: VirBytes` 三参数。
/// `None` 表示"暂未确定"（kernel task 无入口点；非 VM 用户进程延后加载）。
#[derive(Debug, Clone, Copy, Default)]
pub struct EntrySpec {
    /// 入口点 PC。`None` = 尚未加载 ELF（kernel task 或延后加载的用户进程）。
    pub pc: Option<VirBytes>,
    /// 初始 SP。`None` = 尚未设置栈。
    pub sp: Option<VirBytes>,
    /// ps_strings 地址（x86-64:rbx, aarch64:r0, riscv64:a0）。
    /// `None` = 无 ps_strings（kernel task）。
    pub ps_strings: Option<VirBytes>,
}

impl EntrySpec {
    /// 内核任务：无入口点（调度器会用默认入口）。
    pub const KERNEL_TASK: Self = Self { pc: None, sp: None, ps_strings: None };

    /// 延后加载的用户进程：boot 期 PC/SP 全零，由 RS 后续设置。
    pub const DEFERRED: Self = Self { pc: None, sp: None, ps_strings: None };

    /// VM 进程：ELF 已加载，入口点和栈已知。
    pub const fn loaded(pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        Self { pc: Some(pc), sp: Some(sp), ps_strings: Some(ps_strings) }
    }
}

/// VM ELF 加载结果（arch 无关，因为加载逻辑本身 arch 无关）。
#[derive(Debug, Clone, Copy)]
pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

/// arch 层 boot 操作的唯一抽象。
///
/// # OS 概念
///
/// "启动一个进程"在 OS 层面是两个正交操作：
/// 1. **构建初始 CPU 状态**（boot 期，进程尚未运行）
/// 2. **应用状态到 trap frame**（首次调度时，进程即将运行）
///
/// 这两个操作之间，状态以**不透明值**的形式存储在 `KProcess` 中。
/// kernel 层不 inspect 这个值的内部结构——它是 arch 的私有数据。
///
/// # 为什么是 trait
///
/// - 3 个架构的 `StartupState` 字段布局完全不同（x86-64 有段选择子+XSAVE area，
///   aarch64 有 CPACR_EL1 配置，riscv64 有 sstatus.FS）
/// - `build_startup_state` 和 `apply_to_trap_frame` 的实现行为真的不同
/// - kernel 层通过 `CurrentBootArch` 类型别名静态派发，零运行时开销
///
/// # 为什么只有一个 trait（不是三个）
///
/// C 版的 `arch_proc_reset` / `arch_proc_init` / `arch_boot_proc` 是**实现细节**，
/// 不是 OS 概念。Rust 版用 `build_startup_state`（覆盖 reset+init）+
/// free fn `load_vm_elf`（覆盖 boot_proc 的 ELF 加载部分）替代。
pub trait BootArch {
    /// arch 私有的"进程初始 CPU 状态"。
    ///
    /// kernel 层只存储这个值、把它传给 `apply_to_trap_frame`，
    /// **从不读写其字段**。
    ///
    /// # 各架构内部字段（仅作说明，kernel 层不应依赖）
    ///
    /// - x86_64: `psw`, `cs/ds/ss/es/fs/gs`, `rip`, `rsp`, `rbx`,
    ///   `fpu_init: X86FpuInitPolicy`（XSAVE area 清零策略）
    /// - aarch64: `psr`, `pc`, `sp`, `r0`, `fpu_trap: bool`（CPACR_EL1.FPEN 配置）
    /// - riscv64: `sstatus`, `sepc`, `sp`, `a0`, `fs_state: u8`（sstatus.FS 初始值）
    type StartupState: Copy + core::fmt::Debug + Default;

    /// arch 的 trap frame 类型。
    ///
    /// 通常就是 `stackframe_s` 的 Rust 对应物。kernel 层通过类型别名
    /// `CurrentTrapFrame` 引用，不 inspect 其字段。
    type TrapFrame;

    /// 为新进程构建初始 CPU 状态。
    ///
    /// # OS 概念
    ///
    /// "我有一个进程，角色是 `kind`，进程号是 `proc_nr`，入口点是 `entry`。
    ///  给我它运行起来所需的全部 CPU 状态。"
    ///
    /// # arch 层的内部职责
    ///
    /// 1. 根据 `kind` 选择初始 PSW/PSR/sstatus（内核态 vs 用户态）
    /// 2. x86-64：设置段选择子（USER_CS_SELECTOR/USER_DS_SELECTOR）
    /// 3. x86-64：根据 `kind` 决定 XSAVE area 初始化策略
    ///    - `KernelTask`：不初始化（内核任务用内核 FPU 上下文）
    ///    - `Vm` / `UserService` / `UserProcess`：标记"首次使用时初始化"
    ///      （现代 x86-64 用 XSAVE 的 lazy 模式，不是 Minix3 的 memset 清零）
    /// 4. aarch64：配置 CPACR_EL1.FPEN（EL0/EL1 都允许 FP，lazy trap）
    /// 5. riscv64：设置 sstatus.FS = Initial（首次 FP 指令 trap）
    /// 6. 根据 `entry.pc/sp/ps_strings` 设置 rip/rsp/rbx（或 pc/sp/r0、sepc/sp/a0）
    ///
    /// # 为什么 `entry` 是 `EntrySpec` 而非 3 个 `Option<u64>`
    ///
    /// 打包为结构体让"入口点"成为一个 OS 概念，而不是 3 个散落的参数。
    /// `EntrySpec::KERNEL_TASK` / `EntrySpec::DEFERRED` / `EntrySpec::loaded(...)` 
    /// 让调用点表达意图而非填参数。
    fn build_startup_state(
        kind: ProcKind,
        proc_nr: ProcNr,
        entry: EntrySpec,
    ) -> Self::StartupState;

    /// 把启动状态应用到 trap frame。
    ///
    /// # OS 概念
    ///
    /// "这个进程要第一次运行了，把它的初始 CPU 状态写进 trap frame。"
    ///
    /// # 调用时机
    ///
    /// 由调度器在首次调度该进程时调用。boot 期构建的 `StartupState` 
    /// 一直存在 `KProcess` 里，直到这一刻才被应用。
    ///
    /// # arch 层的内部职责
    ///
    /// 1. 写入 PSW/PSR/sstatus 到 trap frame 的状态寄存器字段
    /// 2. x86-64：写入段选择子到 CS/DS/SS/ES/FS/GS 字段
    /// 3. 写入 PC/SP/ps_strings 寄存器
    /// 4. x86-64：如果 `StartupState` 标记了"XSAVE area 需初始化"，
    ///    在 trap frame 的 FPU 保存区执行 `xsave` 初始化序列
    ///    （设置 XCOMP_BV、清零 legacy area）
    /// 5. aarch64：如果 `StartupState` 标记了"FPU 需 trap"，
    ///    配置 CPACR_EL1.FPEN=0b01（EL0 enable, EL1 trap）
    /// 6. riscv64：写入 sstatus.FS = Initial
    fn apply_to_trap_frame(
        state: &Self::StartupState,
        frame: &mut Self::TrapFrame,
    );
}

/// 加载 VM ELF 到 bootstrap 页表。
///
/// # 为什么是 free function 而非 trait 方法
///
/// 三架构的 `load_vm_elf` 实现**完全相同**（已 grep 验证：
/// `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs` 的 `load_vm_elf` 
/// 代码逐行一致，只有模块注释不同）。把它放进 trait 是假多态。
///
/// 这个函数只依赖 `Paging` trait（arch 无关的页表抽象），
/// 所以它是 arch crate 里的共享代码，不是任何 `BootArch` 实现的一部分。
///
/// # OS 概念
///
/// "读 ELF → 映射段 → 设置栈 → 返回入口点"。这是 ELF 加载的通用流程，
/// 与 CPU 架构无关。arch 差异只在"入口点写到哪个寄存器"——那由
/// `build_startup_state` 处理。
///
/// # C 对应
///
/// `libexec_load_elf()` + `arch_boot_proc()` 的 ELF 加载部分。
/// Rust 用 `minix_elf` crate 替代 `libexec`，用 `Paging::map()` 替代 `pg_map`。
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmLoadResult {
    // ... 共享实现（从当前三份重复代码合并）...
    // 详见 §5.4
    todo!("merge from current x86_64/arm64/riscv64 implementations")
}
```

### 3.2 为什么合并 3 个 trait 为 1 个

**当前设计的 3 个 trait**：

```rust
pub trait ArchProcReset  { fn initial_reg_state(...) -> InitialRegState; }
pub trait ArchProcInit: ArchProcReset { fn init_regs(...) -> InitialRegs; }
pub trait BootProcArch: ArchProcInit { fn load_vm_elf<P: Paging>(...) -> VmLoadResult; }
```

**问题**：
1. `ArchProcInit::init_regs` 三架构实现**几乎相同**（只是把 `pc/sp/ps_strings` 装进 `InitialRegs`）——假多态
2. `BootProcArch::load_vm_elf` 三架构实现**完全相同**——假多态
3. trait 继承链"reset → init → boot"翻译自 C 函数调用链，不是 OS 概念
4. 文档 §3.3 自称"init 内部调用 reset"是因果链编造（trait 继承是编译时类型关系，不是运行时调用）

**新设计的 1 个 trait**：

```rust
pub trait BootArch {
    type StartupState;
    type TrapFrame;
    fn build_startup_state(kind, proc_nr, entry) -> Self::StartupState;
    fn apply_to_trap_frame(state, frame);
}
// + free fn load_vm_elf<P: Paging>(...)
```

**为什么这样分**：
- `build_startup_state` 真有架构差异（PSW 常量、段选择子、FPU 策略都不同）→ 进 trait
- `apply_to_trap_frame` 真有架构差异（写不同寄存器）→ 进 trait
- `load_vm_elf` 无架构差异（只依赖 `Paging`）→ free function

**trait bound 使用**：kernel 层会有 `T: BootArch` 的 bound（用于 mock 测试），满足"trait 必须被用作 bound"的要求。

### 3.3 FPU 处理：arch 内部消化，OS 不接触

**当前问题**：`fpu_needs_zero: bool` 出现在 `InitialRegState` 里，流经 kernel 层（`set_boot_initial_reg_state(status, fpu_needs_zero)`），但 kernel 层收下后丢弃（`let _ = _fpu_needs_zero`）。这是 arch → kernel → arch 的死循环。

**新设计**：FPU 完全是 `StartupState` 的内部字段，kernel 层看不到。

#### x86_64 的 FPU 策略（现代 XSAVE 模型，不是 Minix3 fnsave）

```rust
// os/arch/src/x86_64/boot.rs

/// x86-64 FPU 初始化策略（arch 内部，OS 看不到）
#[derive(Debug, Clone, Copy)]
enum X86FpuInitPolicy {
    /// 内核任务：复用内核 FPU 上下文，不初始化
    KernelTask,
    /// 用户进程：首次使用时初始化 XSAVE area
    /// （现代 x86-64 用 lazy XSAVE，不是 Minix3 的 memset 清零）
    LazyUserInit,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StartupState {
    psw: u64,
    cs: u64, ds: u64, ss: u64, es: u64, fs: u64, gs: u64,
    rip: u64, rsp: u64, rbx: u64,
    fpu_policy: X86FpuInitPolicy,
}

impl BootArch for X86_64BootArch {
    type StartupState = StartupState;
    type TrapFrame = TrapFrame;

    fn build_startup_state(kind: ProcKind, proc_nr: ProcNr, entry: EntrySpec) -> Self::StartupState {
        let (psw, fpu_policy) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSW, X86FpuInitPolicy::KernelTask),
            _ => (INIT_PSW, X86FpuInitPolicy::LazyUserInit),
        };
        StartupState {
            psw,
            cs: USER_CS_SELECTOR, ds: USER_DS_SELECTOR, ss: USER_DS_SELECTOR,
            es: USER_DS_SELECTOR, fs: USER_DS_SELECTOR, gs: USER_DS_SELECTOR,
            rip: entry.pc.map(|v| v.0).unwrap_or(0),
            rsp: entry.sp.map(|v| v.0).unwrap_or(0),
            rbx: entry.ps_strings.map(|v| v.0).unwrap_or(0),
            fpu_policy,
        }
    }

    fn apply_to_trap_frame(state: &Self::StartupState, frame: &mut Self::TrapFrame) {
        frame.rflags = state.psw;
        frame.cs = state.cs; frame.ds = state.ds; frame.ss = state.ss;
        frame.es = state.es; frame.fs = state.fs; frame.gs = state.gs;
        frame.rip = state.rip; frame.rsp = state.rsp; frame.rbx = state.rbx;
        match state.fpu_policy {
            X86FpuInitPolicy::KernelTask => { /* 不动 FPU */ }
            X86FpuInitPolicy::LazyUserInit => {
                // 现代 x86-64：设置 XSAVE area 的 XSTATE_BV=0，
                // 让首次 FP 指令触发 #NM，由 trap handler 执行 xrstor 初始化。
                // 不是 Minix3 的 memset(fpu_state[nr], 0, FPU_XFP_SIZE)。
                frame.xsave_header.xstate_bv = 0;
            }
        }
    }
}
```

#### aarch64 的 FPU 策略（CPACR_EL1.FPEN）

```rust
// os/arch/src/arm64/boot.rs

#[derive(Debug, Clone, Copy, Default)]
pub struct StartupState {
    psr: u64,
    pc: u64, sp: u64, r0: u64,
    /// CPACR_EL1.FPEN 配置：true=EL0/EL1 都允许 FP（lazy trap 由 OS 控制）
    fpu_enable_el0: bool,
}

impl BootArch for AArch64BootArch {
    type StartupState = StartupState;
    type TrapFrame = TrapFrame;

    fn build_startup_state(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::StartupState {
        let (psr, fpu_enable_el0) = match kind {
            ProcKind::KernelTask => (INIT_TASK_PSR, false), // 内核任务用 EL1h，FP 不 trap
            _ => (INIT_PSR, true), // 用户进程：EL0 FP 允许，首次使用 lazy init
        };
        StartupState {
            psr, pc: entry.pc.map(|v| v.0).unwrap_or(0),
            sp: entry.sp.map(|v| v.0).unwrap_or(0),
            r0: entry.ps_strings.map(|v| v.0).unwrap_or(0),
            fpu_enable_el0,
        }
    }

    fn apply_to_trap_frame(state: &Self::StartupState, frame: &mut Self::TrapFrame) {
        frame.spsr = state.psr;
        frame.pc = state.pc; frame.sp = state.sp; frame.regs[0] = state.r0;
        // CPACR_EL1.FPEN 配置在 trap return 时由 EL1 → EL0 切换代码处理
        if state.fpu_enable_el0 {
            frame.cpacr_el1 |= 0x3 << 20; // FPEN=0b11
        }
    }
}
```

#### riscv64 的 FPU 策略（sstatus.FS）

```rust
// os/arch/src/riscv64/boot.rs

const SSTATUS_FS_INITIAL: u64 = 0x1 << 13; // FS=0b01

#[derive(Debug, Clone, Copy, Default)]
pub struct StartupState {
    sstatus: u64,
    sepc: u64, sp: u64, a0: u64,
}

impl BootArch for Riscv64BootArch {
    type StartupState = StartupState;
    type TrapFrame = TrapFrame;

    fn build_startup_state(kind: ProcKind, _proc_nr: ProcNr, entry: EntrySpec) -> Self::StartupState {
        let sstatus = match kind {
            ProcKind::KernelTask => INIT_TASK_SSTATUS, // SPP=1, FS=Off
            _ => INIT_SSTATUS | SSTATUS_FS_INITIAL,    // SPP=0, SPIE=1, FS=Initial
        };
        StartupState {
            sstatus, sepc: entry.pc.map(|v| v.0).unwrap_or(0),
            sp: entry.sp.map(|v| v.0).unwrap_or(0),
            a0: entry.ps_strings.map(|v| v.0).unwrap_or(0),
        }
    }

    fn apply_to_trap_frame(state: &Self::StartupState, frame: &mut Self::TrapFrame) {
        frame.sstatus = state.sstatus;
        frame.sepc = state.sepc; frame.sp = state.sp; frame.a0 = state.a0;
    }
}
```

### 3.4 类型别名（kernel 层入口）

```rust
// os/arch/src/lib.rs

#[cfg(target_arch = "x86_64")]
pub type CurrentBootArch = crate::x86_64::boot::X86_64BootArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentBootArch = crate::arm64::boot::AArch64BootArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentBootArch = crate::riscv64::boot::Riscv64BootArch;

/// kernel 层存储这个类型，从不 inspect 其字段。
pub type CurrentStartupState = <CurrentBootArch as BootArch>::StartupState;
pub type CurrentTrapFrame = <CurrentBootArch as BootArch>::TrapFrame;

#[cfg(all(feature = "mock", not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
pub type CurrentBootArch = crate::mock::MockBootArch;
```

---

## 4. kernel 层设计

### 4.1 `ProcessTable` / `PrivTable` — 固定大小数组，零堆

```rust
// os/kernel/src/proc_table.rs

use crate::proc::{KProcess, ProcNr, ...};

pub const NR_TASKS: usize = minix_types::NR_TASKS;
pub const NR_PROCS: usize = 256;
pub const NR_SYS_PROCS: usize = 64;
pub const PROC_TABLE_SIZE: usize = NR_TASKS + NR_PROCS;

/// 内核进程表。
///
/// # 存储模型
///
/// 编译期固定大小数组 `[KProcess; PROC_TABLE_SIZE]`，**不使用堆**。
/// boot 阶段 VM 尚未启动，无 `GlobalAlloc`，所有数据结构必须 const-constructible。
///
/// # C 对应
///
/// C 版 `struct proc proc[NR_TASKS + NR_PROCS]` 是全局静态数组（kernel/proc.c:119）。
/// Rust 版用 `ProcessTable` 结构体内嵌数组，由 `const fn new()` 初始化。
///
/// # SMP 安全
///
/// 所有方法要求调用方持有 BKL（见模块文档）。
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: crate::vm::VmRequestQueue,
}

impl ProcessTable {
    /// 构造全空槽的进程表。
    ///
    /// `KProcess::empty()` 必须是 `const fn`——所有字段都是
    /// `Copy` + const-constructible（`AtomicU32::new(0)` 在 Rust 1.75+ 是 const）。
    pub const fn new() -> Self {
        Self {
            procs: [const { KProcess::empty() }; PROC_TABLE_SIZE],
            sched: Scheduler::new(),
            vm_request_queue: crate::vm::VmRequestQueue::new(),
        }
    }

    pub fn get(&self, nr: ProcNr) -> Option<&KProcess> { ... }
    pub fn get_mut(&mut self, nr: ProcNr) -> Option<&mut KProcess> { ... }
    // ... 其他方法不变 ...
}
```

```rust
// os/kernel/src/kpriv.rs

pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],  // 固定大小，无堆
}

impl PrivTable {
    pub const fn new() -> Self {
        Self {
            privs: [const { KPriv::empty(0) }; NR_SYS_PROCS],
            // 注：每个 slot 的 s_id 需要在 new() 后用循环设置，
            // 因为 const fn 不能用 enumerate()。见下方 init()。
        }
    }

    /// boot 期调用一次，为每个 slot 设置正确的 s_id。
    pub fn init(&mut self) {
        for (i, priv_) in self.privs.iter_mut().enumerate() {
            priv_.s_id = i as SysId;
        }
    }
}
```

**`KProcess::empty()` 的 const 可行性**（已验证当前字段）：

| 字段 | 类型 | const 可行 |
|------|------|----------|
| `p_nr: ProcNr` (i32) | Copy | ✅ |
| `p_endpoint: Endpoint` | Copy | ✅ |
| `p_seg: ProcessSegments` | Copy + Default | ✅ |
| `priv_id: Option<PrivId>` | Copy | ✅ |
| `p_rts_flags: RtsFlags` (AtomicU32) | `AtomicU32::new(0)` const since 1.75 | ✅ |
| `p_misc_flags: MiscFlags` | 同上 | ✅ |
| `p_sched: SchedFields` | 全 AtomicU32 | ✅ |
| `p_accounting/p_time/p_cycles/p_cpuavg` | 全 Atomic | ✅ |
| `p_name: ProcName` | `[u8; 16]` + `const fn new()` | ✅ |
| `p_sendmsg/p_delivermsg: Message` | `#[derive(Copy, Default)]` | ✅ |
| `startup_state: CurrentStartupState` | `#[derive(Default)]` | ✅（见 §4.2） |

### 4.2 `KProcess` — 用 `startup_state` 替代 4 个 `initial_*` 字段

```rust
// os/kernel/src/proc.rs

use minix_arch::CurrentStartupState;

pub struct KProcess {
    // ... 现有字段保留 ...

    // ── 删除以下 4 个字段 ──
    // pub initial_pc: VirBytes,
    // pub initial_sp: VirBytes,
    // pub initial_ps_strings_reg: u64,
    // pub initial_status: u64,

    // ── 替换为单个不透明字段 ──
    /// 进程的初始 CPU 状态（arch 私有，kernel 不 inspect）。
    ///
    /// 由 `BootArch::build_startup_state()` 在 boot 期构建，
    /// 由 `BootArch::apply_to_trap_frame()` 在首次调度时应用。
    ///
    /// 替代旧设计的 `initial_pc/initial_sp/initial_status/initial_ps_strings_reg`
    /// 4 个拆解字段——那些字段破坏了"arch 返回纯值"的原则（§3.1）。
    pub startup_state: CurrentStartupState,
}

impl KProcess {
    /// 构造空槽进程（const fn，用于 `ProcessTable::new()`）。
    pub const fn empty() -> Self {
        Self {
            // ... 现有字段用 const 初始化 ...
            startup_state: CurrentStartupState::DEFAULT,  // 见下方说明
        }
    }

    /// 设置进程名（保留，对应 C 的 `strlcpy(rp->p_name, ...)`）。
    pub fn set_name(&mut self, name: &str) {
        self.p_name = ProcName::from_str(name);
    }

    /// 存储由 `BootArch::build_startup_state()` 构建的状态。
    ///
    /// **不拆解**——整体存储。这是对旧设计 `set_boot_initial_reg_state(status, fpu_needs_zero)`
    /// + `set_boot_pc_sp(pc, sp, ps_strings_reg)` 两个拆解式 setter 的替代。
    pub fn set_startup_state(&mut self, state: CurrentStartupState) {
        self.startup_state = state;
    }
}
```

**`CurrentStartupState::DEFAULT` 的来源**：

`BootArch::StartupState` 需要 `Default` trait。各架构的 `Default` 实现返回"全零"状态（用于空槽）：

```rust
// x86_64/boot.rs
impl Default for StartupState {
    fn default() -> Self {
        Self { psw: 0, cs: 0, ds: 0, ss: 0, es: 0, fs: 0, gs: 0,
               rip: 0, rsp: 0, rbx: 0, fpu_policy: X86FpuInitPolicy::KernelTask }
    }
}

// 为 const 上下文提供常量
impl StartupState {
    pub const DEFAULT: Self = Self { psw: 0, cs: 0, ds: 0, ss: 0, es: 0, fs: 0, gs: 0,
                                      rip: 0, rsp: 0, rbx: 0,
                                      fpu_policy: X86FpuInitPolicy::KernelTask };
}
```

### 4.3 `ProcessCapability` + `CapabilityTemplate` — 替代 6 个裸参数

```rust
// os/kernel/src/capability.rs

use bitflags::bitflags;
use minix_types::Endpoint;

/// Newtype：陷阱掩码（哪些 trap 允许）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrapMask(pub u16);

impl TrapMask {
    pub const NONE: Self = Self(0);
    /// 内核任务允许的 trap（CLOCK/SYSTEM 用 CSK_T，其他用 TSK_T）
    pub const KERNEL_TASK: Self = Self(0x4);  // 示例值，实际从 C 头文件映射
    pub const SERVICE: Self = Self(0x6);       // SRV_T
}

/// Newtype：IPC 目标位图（64 位，每位对应一个 sys proc）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcBitmap(pub u64);

impl IpcBitmap {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(!0);
}

/// Newtype：内核调用掩码（64 位，分两个 u32）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KCallBitmap(pub [u32; 2]);

impl KCallBitmap {
    pub const NONE: Self = Self([0; 2]);
    pub const ALL: Self = Self([0xFFFF_FFFF; 2]);
}

/// 进程能力（OS 概念：进程被允许做什么）。
///
/// 替代旧设计的 6 个裸参数 `configure_boot_priv(flags, init_flags, trap_mask,
/// ipc_to, k_call_mask, sig_mgr)`。那些参数直接翻译 C 字段，缺乏类型安全
/// 和 OS 语义。
///
/// # 与 C `struct priv` 的关系
///
/// `ProcessCapability` 是 `KPriv` 中"权限相关字段"的视图，不是完整替代。
/// `KPriv` 还包含运行时状态（`s_notify_pending`、`s_alarm_timer` 等），
/// 这些不属于"能力"概念。
#[derive(Debug, Clone, Copy)]
pub struct ProcessCapability {
    pub flags: PrivFlagsBits,
    pub trap_mask: TrapMask,
    pub ipc_targets: IpcBitmap,
    pub kernel_calls: KCallBitmap,
    pub signal_manager: Endpoint,
}

/// Boot 期预定义的能力模板（OS 概念：按角色授予能力）。
///
/// 替代旧设计在 `init_proc_and_boot()` 里散落的 `if is_vm { ... } else if
/// is_root_sys { ... }` 分支。每个变体对应一种 OS 角色，封装该角色的
/// 完整能力定义。
///
/// # 与 C 版 `main.c:178-248` 的关系
///
/// C 版在 boot image 循环里用 `if/else if` 分支设置不同进程类型的特权。
/// Rust 版把每种类型的配置提取为 `CapabilityTemplate` 变体，让"角色 → 能力"
/// 的映射成为显式数据，而非散落的控制流。
#[derive(Debug, Clone, Copy)]
pub enum CapabilityTemplate {
    /// IDLE 任务：可计费，无 IPC，无内核调用
    Idle,
    /// 内核任务（CLOCK/SYSTEM/KERNEL）：sys_proc，有限 trap
    KernelTask {
        trap_mask: TrapMask,
    },
    /// VM 进程：sys_proc + VM_SYS_PROC，全部 IPC + 全部内核调用
    Vm,
    /// 根系统服务（RS）：sys_proc + ROOT_SYS_PROC，全部 IPC + 全部内核调用
    RootService,
    /// 其他系统服务：sys_proc + PREEMPTIBLE，由 RS 在运行时授予具体能力
    /// （boot 期不授予，进程处于 `RTS_NO_PRIV` 状态）
    Deferred,
}

impl CapabilityTemplate {
    /// 把模板转换为具体能力。
    ///
    /// 这是"角色 → 能力"映射的单一来源。`init_proc_and_boot()` 调用此方法，
    /// 不再在主流程里散落 `if is_vm { flags = VM_F; ... }` 分支。
    pub fn build(self, self_endpoint: Endpoint) -> ProcessCapability {
        match self {
            Self::Idle => ProcessCapability {
                flags: priv_flag_set::IDL_F,
                trap_mask: TrapMask::NONE,
                ipc_targets: IpcBitmap::NONE,
                kernel_calls: KCallBitmap::NONE,
                signal_manager: Endpoint::NONE,
            },
            Self::KernelTask { trap_mask } => ProcessCapability {
                flags: priv_flag_set::TSK_F,
                trap_mask,
                ipc_targets: IpcBitmap::NONE,
                kernel_calls: KCallBitmap::NONE,
                signal_manager: Endpoint::NONE,
            },
            Self::Vm => ProcessCapability {
                flags: priv_flag_set::VM_F,
                trap_mask: TrapMask::SERVICE,
                ipc_targets: IpcBitmap::ALL,
                kernel_calls: KCallBitmap::ALL,
                signal_manager: self_endpoint,  // VM 自管信号
            },
            Self::RootService => ProcessCapability {
                flags: priv_flag_set::RSYS_F,
                trap_mask: TrapMask::SERVICE,
                ipc_targets: IpcBitmap::ALL,
                kernel_calls: KCallBitmap::ALL,
                signal_manager: self_endpoint,
            },
            Self::Deferred => ProcessCapability {
                flags: PrivFlagsBits::empty(),  // 运行时由 RS 设置
                trap_mask: TrapMask::NONE,
                ipc_targets: IpcBitmap::NONE,
                kernel_calls: KCallBitmap::NONE,
                signal_manager: Endpoint::NONE,
            },
        }
    }
}
```

### 4.4 `PrivTable` — 用 `grant_capability` 替代 `assign_static` + `configure_boot_priv`

```rust
// os/kernel/src/kpriv.rs

impl PrivTable {
    /// 为进程授予能力（OS 概念：分配 slot + 写入能力）。
    ///
    /// 替代旧设计的两步操作：
    /// 1. `assign_static(proc_nr) -> Option<PrivId>`
    /// 2. `configure_boot_priv(priv_id, flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)`
    ///
    /// 旧设计的两步分离导致调用方需要记顺序、需要传 6 个裸参数。
    /// 新设计用 `CapabilityTemplate` 封装角色，一步完成"分配 + 配置"。
    ///
    /// # C 对应
    ///
    /// `get_priv(rp, static_priv_id(proc_nr))` + `main.c:178-248` 的特权设置。
    ///
    /// # 返回值
    ///
    /// `Ok(PrivId)` 成功；`Err(CapabilityError)` 失败（slot 占用 / 越界）。
    pub fn grant_capability(
        &mut self,
        proc_nr: ProcNr,
        template: CapabilityTemplate,
    ) -> Result<PrivId, CapabilityError> {
        let priv_id = static_priv_id(proc_nr);
        let priv_ = self.get_mut(priv_id).ok_or(CapabilityError::InvalidPrivId)?;
        if priv_.s_proc_nr.is_some() {
            return Err(CapabilityError::SlotOccupied);
        }

        let self_endpoint = Endpoint::from_generation_slot(0, proc_nr);
        let cap = template.build(self_endpoint);

        priv_.s_proc_nr = Some(proc_nr);
        priv_.s_flags = cap.flags;
        priv_.s_trap_mask = cap.trap_mask.0;
        priv_.s_ipc_to = cap.ipc_targets.0;
        priv_.s_k_call_mask = cap.kernel_calls.0;
        priv_.s_sig_mgr = cap.signal_manager;
        Ok(priv_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    InvalidPrivId,
    SlotOccupied,
}
```

---

## 5. 主流程：`init_proc_and_boot()`

### 5.1 重写后的主流程（OS 概念编排）

```rust
// os/kernel/src/lib.rs

#[cfg(not(feature = "mock"))]
pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    use minix_arch::{BootArch, CurrentBootArch, load_vm_elf};
    use crate::capability::{CapabilityTemplate, TrapMask};
    use crate::proc::{ProcKind, EntrySpec, proc_nr, KERNEL_TASKS, BOOT_MODULE_PROC_NRS};

    // ── Step 1: 构造空进程表 + 空特权表（const fn，零堆）──
    // OS 概念："所有进程槽初始化为空，等待填充"
    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();
    priv_table.init();

    // ── Step 2: 校验 boot module 数量 ──
    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES,
        kernel_info.boot_modules.len()
    );

    // ── Step 3: 初始化内核任务（编译期固定列表）──
    // OS 概念："内核任务是内核的一部分，不来自 boot modules"
    for &(name, nr) in KERNEL_TASKS.iter() {
        let Some(proc) = proc_table.get_mut(nr) else { continue };

        proc.set_name(name);

        // 授予能力：IDLE 用 Idle 模板，其他用 KernelTask 模板
        let template = if nr == proc_nr::IDLE {
            CapabilityTemplate::Idle
        } else {
            CapabilityTemplate::KernelTask {
                trap_mask: if nr == proc_nr::CLOCK || nr == proc_nr::SYSTEM {
                    TrapMask::KERNEL_TASK  // CSK_T
                } else {
                    TrapMask::KERNEL_TASK  // TSK_T（实际值需映射 C 头文件）
                },
            }
        };
        priv_table.grant_capability(nr, template)
            .expect("kernel task priv slot occupied");

        // 构建初始 CPU 状态（arch 内部处理 PSW/段选择子/FPU 策略）
        let state = CurrentBootArch::build_startup_state(
            ProcKind::KernelTask,
            nr,
            EntrySpec::KERNEL_TASK,
        );
        proc.set_startup_state(state);

        // 内核任务启动时处于 STOPPED 状态
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // ── Step 4: 初始化用户空间 boot modules ──
    // OS 概念："从 boot info 读取模块列表，按角色编排"
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr: ProcNr = BOOT_MODULE_PROC_NRS[i];
        let Some(proc) = proc_table.get_mut(nr) else { continue };

        proc.set_name(module.name);

        let is_vm = nr == proc_nr::VM_PROC_NR;
        let is_root_sys = nr == proc_nr::RS_PROC_NR;

        // ── 4a: 授予能力（按角色选模板）──
        let (template, kind, entry) = if is_vm {
            // VM：加载 ELF，构建带入口点的状态
            let vm_result = load_vm_elf(module, kernel_info, &mut CurrentPaging::new());
            (
                CapabilityTemplate::Vm,
                ProcKind::Vm,
                EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings),
            )
        } else if is_root_sys {
            // RS：boot 期不加载 ELF，延后到 RS 自己启动
            (CapabilityTemplate::RootService, ProcKind::RootService, EntrySpec::DEFERRED)
        } else {
            // 其他系统服务：延后加载
            (CapabilityTemplate::Deferred, ProcKind::UserService, EntrySpec::DEFERRED)
        };

        if matches!(template, CapabilityTemplate::Deferred) {
            // 无特权进程：标记不可调度，等 RS 后续授予
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        } else {
            priv_table.grant_capability(nr, template)
                .expect("static priv slot occupied");
        }

        // ── 4b: 构建初始 CPU 状态 ──
        let state = CurrentBootArch::build_startup_state(kind, nr, entry);
        proc.set_startup_state(state);

        // ── 4c: 设置运行时标志 ──
        // 非 VM 用户进程需等 VM 创建页表
        if nr != proc_nr::VM_PROC_NR {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    proc_table
}
```

### 5.2 与旧主流程的对比

| 旧设计 | 新设计 | 改进点 |
|--------|--------|--------|
| `ProcessTable::new()` 用 `Box<[KProcess]>` | `const fn new()` 用 `[KProcess; N]` | 零堆，boot 期可用 |
| `priv_table.assign_static(nr)` + `configure_boot_priv(id, flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` | `priv_table.grant_capability(nr, template)` | 一步完成，6 个裸参数打包为模板 |
| `CurrentBootProcArch::initial_reg_state(is_kernel, nr)` + `proc.set_boot_initial_reg_state(status, fpu_needs_zero)` | `CurrentBootArch::build_startup_state(kind, nr, entry)` + `proc.set_startup_state(state)` | 不拆解，整体存储；FPU 不泄漏 |
| `CurrentBootProcArch::init_regs(false, nr, pc, sp, ps_strings)` + `proc.set_boot_pc_sp(pc, sp, ps_strings_reg)` | 合并进 `build_startup_state`（`EntrySpec::loaded(pc, sp, ps_strings)`） | 消除"先 reset 再 init"的脆弱协议 |
| `if is_vm { ... } else if is_root_sys { ... }` 散落分支 | `CapabilityTemplate` 枚举 + `grant_capability` | 角色映射成为显式数据 |
| `CurrentBootProcArch::load_vm_elf(module, kinfo, paging)` | `minix_arch::load_vm_elf(module, kinfo, paging)`（free fn） | 不再是 trait 方法，消除假多态 |

### 5.3 调度器的应用步骤（首次调度时）

```rust
// os/kernel/src/sched.rs（伪代码）

fn dispatch_first_run(proc: &KProcess) {
    // arch 层把 startup_state 写入 trap frame
    // kernel 层不关心具体写了哪些寄存器
    CurrentBootArch::apply_to_trap_frame(&proc.startup_state, &mut current_trap_frame);
    // ... 然后执行 iret/eret/sret ...
}
```

这是 `startup_state` 的**唯一消费者**。boot 期构建，调度期应用，中间 kernel 层从不 inspect。

### 5.4 `load_vm_elf` 共享实现（从三份重复代码合并）

```rust
// os/arch/src/arch/boot.rs

pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmLoadResult {
    const VM_STACK_SIZE: usize = 64 * 1024;

    let image = unsafe {
        core::slice::from_raw_parts(module.start.0 as *const u8, module.len)
    };

    let iter = match minix_elf::segment_iter(image) {
        Ok(it) => it,
        Err(_) => {
            let stack_high = kernel_info.user_sp;
            let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);
            return VmLoadResult {
                pc: VirBytes(0), sp,
                ps_strings: VirBytes(sp.0 - 32),
                allocated_bytes: 0,
            };
        }
    };

    let entry = minix_elf::entry_point(image).unwrap_or(0);
    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated: usize = 0;

    for seg in iter {
        let flags = elf_flags_to_page_flags(seg.flags);
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1);
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            let paddr = PhysBytes(vaddr);
            if paging.map(VirBytes(vaddr), paddr, flags).is_ok() {
                total_allocated += page_size as usize;
            }
            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - (copy_start % page_size as usize),
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    let src_offset = file_offset as usize;
                    let dst_ptr = vaddr as *mut u8;
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(src_offset),
                            dst_ptr, copy_len,
                        );
                    }
                    file_offset += copy_len as u64;
                    file_remaining -= copy_len as u64;
                }
            }
            vaddr += page_size;
        }
    }

    let stack_high = kernel_info.user_sp;
    let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);
    let stack_flags = PageFlags::read_write();
    let mut stack_addr = sp.0 & !(page_size - 1);
    while stack_addr < stack_high.0 {
        if paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags).is_ok() {
            total_allocated += page_size as usize;
        }
        stack_addr += page_size;
    }

    let ps_strings = VirBytes(sp.0 - 32);
    VmLoadResult {
        pc: VirBytes(entry), sp, ps_strings,
        allocated_bytes: total_allocated,
    }
}

fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags {
    let mut flags = PageFlags::PRESENT | PageFlags::USER_ACCESSIBLE;
    if elf_flags & 0x2 != 0 { flags |= PageFlags::WRITABLE; }
    if elf_flags & 0x1 != 0 { flags |= PageFlags::EXECUTABLE; }
    flags
}
```

---

## 6. 迁移路径

### 6.1 文件改动清单

| 文件 | 改动类型 | 估计行数 |
|------|--------|--------|
| `os/arch/src/arch/proc_arch.rs` | **删除**（替换为 `boot.rs`） | -230 |
| `os/arch/src/arch/boot.rs` | **新建**（`BootArch` trait + `load_vm_elf` + `ProcKind`/`EntrySpec`） | +200 |
| `os/arch/src/x86_64/proc_arch.rs` | **删除**（替换为 `boot.rs`） | -260 |
| `os/arch/src/x86_64/boot.rs` | **新建**（`X86_64BootArch` + `StartupState` + FPU 策略） | +120 |
| `os/arch/src/arm64/proc_arch.rs` | **删除** | -240 |
| `os/arch/src/arm64/boot.rs` | **新建** | +80 |
| `os/arch/src/riscv64/proc_arch.rs` | **删除** | -220 |
| `os/arch/src/riscv64/boot.rs` | **新建** | +80 |
| `os/arch/src/lib.rs` | 更新类型别名（`CurrentBootArch`/`CurrentStartupState`/`CurrentTrapFrame`） | ~20 |
| `os/kernel/src/proc_table.rs` | `Box<[KProcess]>` → `[KProcess; N]` + `const fn new()` | ~30 |
| `os/kernel/src/kpriv.rs` | `Box<[KPriv]>` → `[KPriv; N]` + `const fn new()` + `grant_capability` | ~80 |
| `os/kernel/src/capability.rs` | **新建**（`ProcessCapability` + `CapabilityTemplate` + Newtypes） | +150 |
| `os/kernel/src/proc.rs` | 删除 4 个 `initial_*` 字段 + 2 个 setter，加 `startup_state` 字段 | ~40 |
| `os/kernel/src/lib.rs` | 重写 `init_proc_and_boot()` | ~100 |
| `os/kernel/src/syscall_device.rs` | IOPL 操作下沉到 arch（见 §6.2） | ~30 |

**总计**：约 +1000 / -950 行（净增 ~50 行，但消除了三份重复的 `load_vm_elf` 和三份被迫写 `default()` 的 `initial_reg_state`）。

### 6.2 IOPL 操作下沉（问题 #5 的修复）

**当前问题**：`os/kernel/src/syscall_device.rs` 多处直接操作 `proc.initial_status` 的 IOPL 位（`0x3000` mask）。这是 x86-64 硬件语义泄漏到 kernel 层。

**修复方向**：在 `BootArch` trait 增加一个方法（或单独的 `ArchCpuState` trait）：

```rust
// os/arch/src/arch/boot.rs（扩展）

pub trait BootArch {
    // ... 现有方法 ...

    /// 为进程启用 I/O 特权（x86-64: 设置 RFLAGS.IOPL=3；其他架构: no-op）。
    ///
    /// OS 概念："让这个进程能直接访问 I/O 端口"。
    /// arch 层内部决定怎么实现——x86-64 改 IOPL 位，aarch64 配置 PSTATE.PAN，
    /// riscv64 配置 sstatus.SUM。
    fn enable_io_privilege(state: &mut Self::StartupState);

    /// 查询进程是否拥有 I/O 特权。
    fn has_io_privilege(state: &Self::StartupState) -> bool;
}

// x86_64 实现
impl BootArch for X86_64BootArch {
    fn enable_io_privilege(state: &mut Self::StartupState) {
        state.psw |= 0x3000;  // IOPL=3
    }
    fn has_io_privilege(state: &Self::StartupState) -> bool {
        (state.psw & 0x3000) == 0x3000
    }
}

// aarch64/riscv64 实现：no-op 或对应机制
```

`syscall_device.rs` 改为调用 `CurrentBootArch::enable_io_privilege(&mut proc.startup_state)`，不再直接操作 `initial_status` 字段。

---

## 7. 未决问题（需多 AI bagging 讨论）

### 7.1 `StartupState` 的 `Default` 实现是否合适？

我用 `Default` 返回"全零状态"用于空槽。但"全零 PSW"在 x86-64 上是非法的（bit 1 必须为 1）。替代方案：
- (a) `Default` 返回全零，但加 debug_assert 在 `apply_to_trap_frame` 时检查"非空槽"
- (b) 用 `Option<CurrentStartupState>`，`None` 表示空槽
- (c) 用 `MaybeUninit<CurrentStartupState>` + 标志位

**我的倾向**：(a) 最简单，`Default` 仅用于 `ProcessTable::new()` 的初始填充，真实进程都会调用 `set_startup_state` 覆盖。

### 7.2 `ProcKind` 和 `CapabilityTemplate` 是否冗余？

`ProcKind` 有 5 个变体，`CapabilityTemplate` 有 5 个变体，看起来一一对应。是否应该合并？

**我的倾向**：保持分离。`ProcKind` 是给 arch 看的（决定 PSW/FPU 策略），`CapabilityTemplate` 是给 kernel 看的（决定 IPC/syscall 权限）。两者关注点不同：
- `ProcKind::KernelTask` 和 `CapabilityTemplate::KernelTask { trap_mask }` 不同——前者是"运行在内核态"，后者是"sys_proc 标志 + 有限 trap"
- 未来可能有 `ProcKind::KernelTask` 但 `CapabilityTemplate::Idle` 的情况（IDLE 就是）

### 7.3 `load_vm_elf` 放在 arch crate 还是 kernel crate？

它是 arch 无关的，只依赖 `Paging` trait。可以放在：
- (a) `os/arch/src/arch/boot.rs`（我的选择）——因为它是 boot 期 arch 操作的一部分
- (b) `os/kernel/src/boot.rs`——因为它是 arch 无关的纯 OS 逻辑

**我的倾向**：(a)。虽然逻辑 arch 无关，但它操作物理内存（`PhysBytes`、`paging.map`），这些是 arch crate 的概念。kernel crate 应该只看 OS 抽象。

### 7.4 `TrapFrame` 关联类型是否需要？

我把 `TrapFrame` 作为 `BootArch` 的关联类型，但 kernel 层目前没有明确的 `TrapFrame` 类型——当前代码用 `p_reg`（stackframe_s）嵌入 KProcess。

**待确认**：
- `TrapFrame` 是否应该是 `KProcess` 的字段（替代 `p_reg`）？
- 还是 `apply_to_trap_frame` 应该直接接收 `&mut KProcess`，让 arch 自己找 `p_reg`？

**我的倾向**：后者。`apply_to_trap_frame(&state, &mut KProcess)` 让 arch 内部访问 `proc.p_reg`（或未来的 `proc.trap_frame`）。这样 `TrapFrame` 不需要作为关联类型暴露。

### 7.5 mock 测试如何处理？

当前 `MockProcArch` 实现了 3 个 trait。新设计只有 1 个 trait，mock 更简单：

```rust
#[cfg(feature = "mock")]
pub struct MockBootArch;

#[cfg(feature = "mock")]
impl BootArch for MockBootArch {
    type StartupState = MockStartupState;
    type TrapFrame = MockTrapFrame;

    fn build_startup_state(kind: ProcKind, _nr: ProcNr, entry: EntrySpec) -> Self::StartupState {
        MockStartupState { kind, entry }
    }

    fn apply_to_trap_frame(state: &Self::StartupState, frame: &mut Self::TrapFrame) {
        frame.kind = state.kind;
        frame.entry = state.entry;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MockStartupState {
    kind: ProcKind,
    entry: EntrySpec,
}
```

---

## 8. 与 review-rules 的对照

| review-rules | 本设计的遵守方式 |
|-------------|---------------|
| 模式 14（硬件未抽象为 trait） | `BootArch` trait + 关联类型，`StartupState` 是 arch 私有 |
| 模式 21（硬件语义泄漏到 OS 层） | `segment_selectors`/`fpu_needs_zero` 不再出现在 kernel crate |
| 模式 25（不必要的 trait 抽象） | 3 个 trait → 1 个 trait；`load_vm_elf` 改为 free function |
| 模式 22（no_std 违规） | `ProcessTable`/`PrivTable` 用固定数组，零堆 |
| 模式 16（裸整数表达语义） | `TrapMask`/`IpcBitmap`/`KCallBitmap` Newtype + `CapabilityTemplate` 枚举 |
| 模式 17（C 式哨兵值） | `SegmentSelectors::default()` 全零消除；`StartupState::Default` 仅用于空槽 |
| 模式 48（因果链编造） | "init_regs 内部调用 reset" 消除（合并为单方法） |
| 模式 31（通用接口含上下文特定元素） | `InitialRegState` 拆解为各架构独立的 `StartupState` |
| 最高原则（OS 与硬件无关） | kernel crate 只见 `CurrentStartupState`（不透明别名），不见任何硬件字段 |
| 最高原则（rewrite 而非 translate） | 从"OS 概念：构建状态/应用状态/授予能力"出发，不翻译 C 函数签名 |

---

## 9. 总结

本设计的核心改动：

1. **arch 层**：3 个 trait → 1 个 `BootArch` trait + 1 个 free fn `load_vm_elf`。`StartupState` 是 arch 私有的关联类型，kernel 层不 inspect。FPU 完全下沉到 arch 内部，采用现代硬件的 lazy 初始化模型（XSAVE / CPACR_EL1.FPEN / sstatus.FS），不翻译 Minix3 的 fnsave 模型。

2. **kernel 层**：
   - `ProcessTable`/`PrivTable` 用 `[T; N]` 固定数组 + `const fn new()`，零堆，boot 期可用
   - `KProcess` 用单个 `startup_state: CurrentStartupState` 字段替代 4 个 `initial_*` 字段，消除 arch → kernel → arch 的死循环
   - `PrivTable::grant_capability(nr, template)` 替代 `assign_static` + `configure_boot_priv` 6 裸参数
   - `CapabilityTemplate` 枚举把"角色 → 能力"映射变成显式数据，替代散落的 `if is_vm { ... } else if is_root_sys { ... }` 分支

3. **OS 概念重新分配**：
   - "构建状态"（`build_startup_state`）和"应用状态"（`apply_to_trap_frame`）是两个正交的 OS 操作，不照搬 C 的 reset/init/boot_proc 三函数划分
   - "授予能力"（`grant_capability`）是 OS 概念，不翻译 C 的 `get_priv + 字段赋值`
   - "入口点"（`EntrySpec`）是 OS 概念，不是 3 个散落的 `u64` 参数

4. **架构演进**：FPU 处理采用现代硬件模型，不翻译 Minix3 的 fnsave/fxrstor。x86-64 用 XSAVE lazy 模式，aarch64 用 CPACR_EL1.FPEN，riscv64 用 sstatus.FS。这些差异完全封装在 arch 层内部，kernel 层看不到。

这个设计是独立完成的，未参考其他 AI 的设计文档。待 bagging 阶段汇总后，可以取长补短，形成最终设计。
