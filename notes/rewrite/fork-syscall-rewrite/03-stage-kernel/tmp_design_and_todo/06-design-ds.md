# 06-design-ds: 进程表初始化与 VM ELF 加载 —— Rust 重设计

> **状态**: 设计稿（多 AI bagging 阶段）
> **作者**: DeepSeek
> **目标**: 解决 [06-problem.md](./06-problem.md) 中列出的全部 P0/P1/P2 问题，提供可落地的 Rust 架构设计。
> **范围**: §3（Rust 设计决策）和 §4（实现详解）的完全重写。

---

## 设计原则（从问题中提炼）

在开始设计之前，先明确三条不可违反的原则：

### 原则 1：OS 层不出现硬件语义

编译完成后，代码只支持一种架构。任何在 aarch64/riscv64 编译时"无意义"的字段/类型/方法名，都不应存在于该架构的编译产物中。

**检测方法**：看到这个字段，问"aarch64 编译时，这个字段有值吗？有意义吗？"→ 没有/没意义 → 违规。

### 原则 2：Rewrite，不是 Translate

Rust 版从 OS 概念出发设计 API，不从"对应哪个 C 函数"出发。方法名、类型名、参数组织方式都反映 OS 概念，而非 C 实现细节。

**检测方法**：看到这个 API，问"它对应哪个 C 函数？"→ 如果这是第一步想到的 → 违规。

### 原则 3：Boot 阶段绝不允许堆

`init_proc_and_boot()` 执行时 VM 还没启动，没有堆可用。所有 boot 阶段数据结构必须是编译期固定大小（数组、`[T; N]`）。

---

## 1. 总体架构

### 1.1 分层：kernel 层 + arch 层

```
┌──────────────────────────────────────────────────────────────┐
│  kernel 层（os/kernel/）                                      │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────┐                  │
│  │  ProcessTable    │  │   PrivTable      │                  │
│  │  [KProcess; N]   │  │   [KPriv; N]     │                  │
│  │  (固定数组)      │  │   (固定数组)     │                  │
│  └──────────────────┘  └──────────────────┘                  │
│           │                      │                            │
│           ▼                      ▼                            │
│  ┌───────────────────────────────────────────────────────┐    │
│  │           init_proc_and_boot()  主流程                │    │
│  └───────────────────────────────────────────────────────┘    │
│                         │                                     │
│                         │ 使用 CurrentRegs 类型别名           │
│                         ▼                                     │
│              ┌──────────────────┐                             │
│              │   KProcess       │                             │
│              │   .boot_regs:    │                             │
│              │     CurrentRegs  │  ← 单一字段，不透明          │
│              └──────────────────┘                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                          │
                          │ CurrentRegs = X86_64Regs | AArch64Regs | Riscv64Regs
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  arch 层（os/arch/）                                            │
│                                                                 │
│  ┌──────────────────────────────────────────────────────┐      │
│  │  pub trait ArchProcess {                             │      │
│  │      type Regs: BootRegs;                            │      │
│  │      fn boot_regs(is_kernel: bool) -> Self::Regs;    │      │
│  │  }                                                   │      │
│  └──────────────────────────────────────────────────────┘      │
│           │                  │                  │               │
│           ▼                  ▼                  ▼               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │ X86_64Regs   │  │ AArch64Regs  │  │ Riscv64Regs  │          │
│  │ - rflags     │  │ - spsr       │  │ - sstatus    │          │
│  │ - cs,ds,...  │  │ - pc,sp      │  │ - pc,sp      │          │
│  │ - pc,sp      │  │ - x0         │  │ - a0         │          │
│  │ - rbx        │  │              │  │              │          │
│  │ - xsave_cfg  │  │              │  │              │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│                                                                 │
│  ┌──────────────────────────────────────────────────────┐      │
│  │  pub fn load_vm_elf<P: Paging>(...) -> VmLoadResult  │      │
│  │  (纯函数，不在 trait 中 — 三架构完全相同)            │      │
│  └──────────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 设计决策：一个 trait + 关联类型 + 一个纯函数

| 当前设计（问题版） | 新设计 | 理由 |
|---|---|---|
| 3 个 trait：`ArchProcReset`, `ArchProcInit`, `BootProcArch` | 1 个 trait：`ArchProcess` | 当前 3 个 trait 完全对应 3 个 C 函数（translate 味），且 `init_regs` 和 `load_vm_elf` 几乎没有架构差异 |
| `InitialRegState` 含 `segment_selectors`（所有架构可见） | 各 arch 自定义 `Regs` 类型（关联类型），字段仅对该架构有意义 | 硬件语义不下沉到 OS 层 |
| `InitialRegs` 独立类型，kernel 拆解后分别存储 | `Regs` 类型自带 `set_entry()` 方法，kernel 整体存储 | 纯函数式：arch 返回值不被 kernel 拆解 |
| `Box<[KProcess]>` + `Box<[KPriv]>` | `[KProcess; PROC_TABLE_SIZE]` + `[KPriv; NR_SYS_PROCS]` | 无堆，编译期固定大小 |
| `load_vm_elf` 在 trait 中 | `pub fn load_vm_elf()` 纯函数 | 三架构实现完全相同，不需要 trait 派发 |

### 1.3 与 C 函数的对应关系（概念层面，非逐行翻译）

| OS 概念 | C 实现（仅供参考） | Rust 新设计 |
|---|---|---|
| 新建进程的初始寄存器状态 | `arch_proc_reset(rp)` 直接写 `p_reg` | `ArchProcess::boot_regs()` 返回 `Self::Regs` |
| 设置进程的入口点 | `arch_proc_init()` 写 `p_reg.pc/sp/bx` | `Regs::set_entry(pc, sp, ps_strings)` |
| 加载 VM 的 ELF 到 bootstrap 页表 | `arch_boot_proc()` 内联 ELF 加载 | `load_vm_elf()` 纯函数 |
| 清空进程表 + 特权表 | `proc_init()` 循环 | `ProcessTable::new()` + `PrivTable::new()` |
| 为 boot 进程分配特权 | `get_priv()` + 内联设置 | `PrivTable::assign()` + `PrivTable::set_capability()` |
| 遍历 boot image 初始化 | `main.c` 循环 | `init_proc_and_boot()` 主流程 |

---

## 2. arch 层设计

### 2.1 核心 trait：`ArchProcess`

```rust
// os/arch/src/arch/proc_arch.rs

/// 架构特定的进程初始化。
///
/// 定义了"为进程准备初始寄存器状态"这一 OS 概念。
/// 只有一个 trait 方法，因为从 OS 概念看，"初始化一个进程的寄存器状态"
/// 是一个原子操作——不需要拆成 reset/init/boot 三步。
pub trait ArchProcess {
    /// 该架构的寄存器状态类型。
    /// 编译时由 cfg 确定：x86-64 = X86_64Regs, aarch64 = AArch64Regs, riscv64 = Riscv64Regs。
    type Regs: BootRegs;

    /// 为新进程构建初始寄存器状态。
    ///
    /// 返回的状态包含：状态寄存器初值、段选择子（x86-64）、
    /// FPU 初始化配置等。所有硬件细节封装在返回的 Regs 值中。
    ///
    /// # 参数
    /// * `is_kernel` - true: 内核 task（p_nr < 0），false: 用户进程
    fn boot_regs(is_kernel: bool) -> Self::Regs;
}
```

### 2.2 `BootRegs` trait：寄存器状态的操作接口

```rust
/// 寄存器状态可以执行的操作。
///
/// 各 arch 的 Regs 类型实现此 trait，kernel 层通过此 trait
/// 操作寄存器状态而不需要知道具体类型。
pub trait BootRegs: Sized {
    /// 设置进程的入口点（PC、SP、ps_strings 地址）。
    /// 返回更新后的状态（builder 模式）。
    fn with_entry(self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self;

    /// 读取 PC（入口点）。
    fn pc(&self) -> VirBytes;

    /// 读取 SP（栈指针）。
    fn sp(&self) -> VirBytes;

    /// 读取状态寄存器值（RFLAGS/SPSR/sstatus）。
    fn status(&self) -> u64;

    /// 读取 ps_strings 寄存器值。
    fn ps_strings_reg(&self) -> u64;
}
```

**设计理由**：`BootRegs` trait 提供 kernel 层需要的四个只读访问器（pc, sp, status, ps_strings_reg）。kernel 层通过这四个方法读取 arch 状态，写入 trap frame。不需要知道 segment_selectors、FPU 配置等 arch 内部细节。

### 2.3 x86-64 实现

```rust
// os/arch/src/x86_64/proc_arch.rs

/// x86-64 初始寄存器状态。
///
/// 包含 RFLAGS、段选择子、XSAVE 配置等 x86-64 特有硬件状态。
/// aarch64/riscv64 编译时此类型完全不存在。
#[derive(Debug, Clone, Copy)]
pub struct X86_64Regs {
    /// RFLAGS 初值（IOPL + IF + bit1）
    rflags: u64,
    /// 段选择子（平坦内存模型，USER_CS/USER_DS）
    cs: u16,
    ds: u16,
    ss: u16,
    es: u16,
    fs: u16,
    gs: u16,
    /// RIP（入口点）
    rip: u64,
    /// RSP（栈指针）
    rsp: u64,
    /// RBX（ps_strings 指针）
    rbx: u64,
    /// 是否需要在首次调度时初始化 XSAVE area
    xsave_needs_init: bool,
}

impl X86_64Regs {
    const INIT_TASK_RFLAGS: u64 = 0x1202; // IOPL=1, IF=1, bit1=1
    const INIT_USER_RFLAGS: u64 = 0x0202; // IOPL=0, IF=1, bit1=1
    const USER_CS: u16 = 0x1B;
    const USER_DS: u16 = 0x23;
}

impl ArchProcess for X86_64ProcArch {
    type Regs = X86_64Regs;

    fn boot_regs(is_kernel: bool) -> X86_64Regs {
        X86_64Regs {
            rflags: if is_kernel { Self::INIT_TASK_RFLAGS } else { Self::INIT_USER_RFLAGS },
            cs: Self::USER_CS,
            ds: Self::USER_DS,
            ss: Self::USER_DS,
            es: Self::USER_DS,
            fs: Self::USER_DS,
            gs: Self::USER_DS,
            rip: 0,
            rsp: 0,
            rbx: 0,
            xsave_needs_init: !is_kernel, // 用户进程需要 XSAVE area 初始化
        }
    }
}

impl BootRegs for X86_64Regs {
    fn with_entry(mut self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        self.rip = pc.0;
        self.rsp = sp.0;
        self.rbx = ps_strings.0;
        self
    }

    fn pc(&self) -> VirBytes { VirBytes(self.rip) }
    fn sp(&self) -> VirBytes { VirBytes(self.rsp) }
    fn status(&self) -> u64 { self.rflags }
    fn ps_strings_reg(&self) -> u64 { self.rbx }
}
```

### 2.4 AArch64 实现

```rust
// os/arch/src/arm64/proc_arch.rs

/// AArch64 初始寄存器状态。
///
/// 无段选择子，无 XSAVE——SPSR_EL1 控制异常级别和中断屏蔽。
#[derive(Debug, Clone, Copy)]
pub struct AArch64Regs {
    /// SPSR_EL1 初值（EL0t 或 EL1h）
    spsr: u64,
    /// ELR_EL1（入口点）
    elr: u64,
    /// SP_EL0（栈指针）
    sp: u64,
    /// X0（ps_strings 指针，启动代码约定）
    x0: u64,
}

impl AArch64Regs {
    const INIT_TASK_SPSR: u64 = 0x000003C5; // EL1h, F/I/A/D masked
    const INIT_USER_SPSR: u64 = 0x00000000; // EL0t
}

impl ArchProcess for AArch64ProcArch {
    type Regs = AArch64Regs;

    fn boot_regs(is_kernel: bool) -> AArch64Regs {
        AArch64Regs {
            spsr: if is_kernel { Self::INIT_TASK_SPSR } else { Self::INIT_USER_SPSR },
            elr: 0,
            sp: 0,
            x0: 0,
        }
    }
}

impl BootRegs for AArch64Regs {
    fn with_entry(mut self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        self.elr = pc.0;
        self.sp = sp.0;
        self.x0 = ps_strings.0;
        self
    }

    fn pc(&self) -> VirBytes { VirBytes(self.elr) }
    fn sp(&self) -> VirBytes { VirBytes(self.sp) }
    fn status(&self) -> u64 { self.spsr }
    fn ps_strings_reg(&self) -> u64 { self.x0 }
}
```

### 2.5 RISC-V 实现

```rust
// os/arch/src/riscv64/proc_arch.rs

/// RISC-V 初始寄存器状态。
///
/// 无段选择子——sstatus.SPP 控制特权级，sstatus.SPIE 控制中断。
#[derive(Debug, Clone, Copy)]
pub struct Riscv64Regs {
    /// sstatus 初值（SPP=1/SPIE=0 或 SPP=0/SPIE=1）
    sstatus: u64,
    /// sepc（入口点）
    sepc: u64,
    /// x2（栈指针）
    sp: u64,
    /// a0（ps_strings 指针，参数寄存器）
    a0: u64,
}

impl Riscv64Regs {
    const INIT_TASK_SSTATUS: u64 = 0x00000100; // SPP=1 (S-mode), SPIE=0
    const INIT_USER_SSTATUS: u64 = 0x00000020; // SPP=0 (U-mode), SPIE=1
}

impl ArchProcess for Riscv64ProcArch {
    type Regs = Riscv64Regs;

    fn boot_regs(is_kernel: bool) -> Riscv64Regs {
        Riscv64Regs {
            sstatus: if is_kernel { Self::INIT_TASK_SSTATUS } else { Self::INIT_USER_SSTATUS },
            sepc: 0,
            sp: 0,
            a0: 0,
        }
    }
}

impl BootRegs for Riscv64Regs {
    fn with_entry(mut self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        self.sepc = pc.0;
        self.sp = sp.0;
        self.a0 = ps_strings.0;
        self
    }

    fn pc(&self) -> VirBytes { VirBytes(self.sepc) }
    fn sp(&self) -> VirBytes { VirBytes(self.sp) }
    fn status(&self) -> u64 { self.sstatus }
    fn ps_strings_reg(&self) -> u64 { self.a0 }
}
```

### 2.6 类型别名：CurrentRegs

```rust
// os/arch/src/arch/proc_arch.rs

/// 当前编译目标架构的寄存器状态类型。
/// kernel 层通过此别名使用，不知道具体是哪个架构。
#[cfg(target_arch = "x86_64")]
pub type CurrentRegs = crate::x86_64::proc_arch::X86_64Regs;

#[cfg(target_arch = "aarch64")]
pub type CurrentRegs = crate::arm64::proc_arch::AArch64Regs;

#[cfg(target_arch = "riscv64")]
pub type CurrentRegs = crate::riscv64::proc_arch::Riscv64Regs;

/// 当前编译目标架构的 ArchProcess 实现。
#[cfg(target_arch = "x86_64")]
pub type CurrentArch = crate::x86_64::proc_arch::X86_64ProcArch;

#[cfg(target_arch = "aarch64")]
pub type CurrentArch = crate::arm64::proc_arch::AArch64ProcArch;

#[cfg(target_arch = "riscv64")]
pub type CurrentArch = crate::riscv64::proc_arch::Riscv64ProcArch;
```

### 2.7 `load_vm_elf`：纯函数（不在 trait 中）

```rust
// os/arch/src/arch/elf_loader.rs  (或 os/arch/src/arch/proc_arch.rs)

/// 加载 VM ELF 到 bootstrap 页表。
///
/// 三架构完全相同——不需要 trait 派发。使用 `Paging` trait 抽象页表操作。
///
/// 返回结果：入口点、栈指针、ps_strings 地址、已分配字节数。
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmLoadResult {
    // 1. 解析 ELF 头
    // 2. 遍历 PT_LOAD 段：分配物理页 + 映射 + 复制段数据
    // 3. 在栈顶设置 ps_strings 结构
    // 4. 返回 VmLoadResult
    // （实现与当前三架构的 load_vm_elf 完全相同，略）
    todo!()
}

/// VM ELF 加载结果。
pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}
```

### 2.8 Mock 支持（测试用）

```rust
// os/arch/src/arch/proc_arch.rs

#[cfg(any(test, feature = "mock"))]
pub struct MockRegs {
    pub status: u64,
    pub pc: u64,
    pub sp: u64,
    pub ps_strings_reg: u64,
}

#[cfg(any(test, feature = "mock"))]
impl BootRegs for MockRegs {
    fn with_entry(mut self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        self.pc = pc.0;
        self.sp = sp.0;
        self.ps_strings_reg = ps_strings.0;
        self
    }
    fn pc(&self) -> VirBytes { VirBytes(self.pc) }
    fn sp(&self) -> VirBytes { VirBytes(self.sp) }
    fn status(&self) -> u64 { self.status }
    fn ps_strings_reg(&self) -> u64 { self.ps_strings_reg }
}

#[cfg(any(test, feature = "mock"))]
pub struct MockArch;

#[cfg(any(test, feature = "mock"))]
impl ArchProcess for MockArch {
    type Regs = MockRegs;
    fn boot_regs(_is_kernel: bool) -> MockRegs {
        MockRegs { status: 0, pc: 0, sp: 0, ps_strings_reg: 0 }
    }
}
```

---

## 3. kernel 层设计

### 3.1 存储：固定数组，零堆

```rust
// os/kernel/src/proc_table.rs

/// 进程表。编译期固定大小，不使用堆。
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: VmRequestQueue,
}

impl ProcessTable {
    /// 创建空的进程表。所有 slot 标记为 SLOT_FREE。
    pub fn new() -> Self {
        let mut procs = [KProcess::new_empty(); PROC_TABLE_SIZE];
        // 初始化每个 slot 的 p_nr 和 p_endpoint
        for (i, proc) in procs.iter_mut().enumerate() {
            let nr = (i as ProcNr) - (NR_TASKS as ProcNr);
            proc.p_nr = nr;
            proc.p_endpoint = Endpoint::from_generation_slot(0, nr);
            proc.p_rts_flags.set(RtsFlagsBits::SLOT_FREE);
            #[cfg(debug_assertions)]
            { proc.p_magic = 0xC0FFEE1; }
        }
        // IDLE 进程特殊处理
        let idle_idx = nr_to_idx(proc_nr::IDLE).unwrap();
        procs[idle_idx].p_endpoint = Endpoint::from_generation_slot(0, proc_nr::IDLE);
        procs[idle_idx].p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        procs[idle_idx].p_name = ProcName::from_str("IDLE");

        Self {
            procs,
            sched: Scheduler::new(),
            vm_request_queue: VmRequestQueue::new(),
        }
    }
}
```

```rust
// os/kernel/src/kpriv.rs

/// 特权表。编译期固定大小，不使用堆。
pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    pub fn new() -> Self {
        let mut privs = [KPriv::new_empty(); NR_SYS_PROCS];
        for (i, priv_) in privs.iter_mut().enumerate() {
            priv_.s_id = i as SysId;
        }
        Self { privs }
    }
}
```

**关键变更**：`KProcess` 和 `KPriv` 需要提供 `const fn new_empty() -> Self` 用于数组初始化。当前所有字段都是简单类型（整数、Atomic、Option、数组），完全可行。

### 3.2 KProcess 的 boot_regs 字段

```rust
// os/kernel/src/proc.rs

use minix_arch::arch::CurrentRegs;

pub struct KProcess {
    // ... existing fields unchanged ...

    /// 初始寄存器状态（arch-specific，不透明）。
    ///
    /// 由 ArchProcess::boot_regs() 构建，通过 BootRegs trait 方法读取。
    /// 调度器首次调度此进程时，从此字段读取 PC/SP/status 写入 trap frame。
    ///
    /// 命名：boot_regs 而非 initial_regs 或 startup_regs ——
    /// 强调这是 boot 阶段设置的初始值，区别于运行时上下文切换保存的寄存器。
    pub boot_regs: CurrentRegs,
}
```

**关键变更**：
- 4 个独立字段（`initial_pc`, `initial_sp`, `initial_ps_strings_reg`, `initial_status`）→ 1 个字段 `boot_regs: CurrentRegs`
- kernel 层通过 `proc.boot_regs.pc()` 等方法读取，不需要知道内部结构
- `CurrentRegs` 在 aarch64 编译时不含 `SegmentSelectors`，在 riscv64 编译时不含 `XSAVE` 配置

### 3.3 特权分配 API：OS 概念命名

```rust
// os/kernel/src/kpriv.rs

/// 进程的能力（capability）——它能做什么。
///
/// 这是 OS 概念，不是 C 的 priv 结构体镜像。
/// 在 boot 阶段，能力由进程类型预定义；运行时由 RS 动态配置。
pub struct Capability {
    pub flags: PrivFlagsBits,
    pub trap_mask: u16,
    pub ipc_to: u64,
    pub k_call_mask: [u32; 2],
    pub sig_mgr: Endpoint,
}

/// Boot 阶段的预定义能力模板。
///
/// 每种进程类型有固定的能力集合，在编译时确定。
pub enum BootCapability {
    /// 内核任务（CLOCK, SYSTEM, KERNEL, ASYNCM）
    KernelTask {
        flags: PrivFlagsBits,
    },
    /// VM 进程
    Vm,
    /// Root System 进程（RS）
    RootSys,
}

impl BootCapability {
    /// 展开为具体的能力配置。
    pub fn expand(self, proc_nr: ProcNr) -> Capability {
        match self {
            BootCapability::KernelTask { flags } => Capability {
                flags,
                trap_mask: 0,          // TSK_T or CSK_T
                ipc_to: 0,             // TSK_M = NO_M
                k_call_mask: [0; 2],   // TSK_KC = NO_C
                sig_mgr: Endpoint::NONE,
            },
            BootCapability::Vm => Capability {
                flags: PrivFlagsBits::VM_F,
                trap_mask: 0,          // SRV_T
                ipc_to: !0,            // SRV_M = ALL_M
                k_call_mask: [!0; 2],  // SRV_KC = ALL_C
                sig_mgr: Endpoint::from_generation_slot(0, proc_nr),
            },
            BootCapability::RootSys => Capability {
                flags: PrivFlagsBits::RSYS_F,
                trap_mask: 0,          // SRV_T
                ipc_to: !0,            // SRV_M = ALL_M
                k_call_mask: [!0; 2],  // SRV_KC = ALL_C
                sig_mgr: Endpoint::from_generation_slot(0, proc_nr),
            },
        }
    }
}

impl PrivTable {
    /// 为进程分配特权 slot。
    ///
    /// 建立 proc_nr ↔ priv_id 的双向关联。
    /// 返回 priv_id 供后续 set_capability 使用。
    pub fn assign(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        let priv_id = static_priv_id(proc_nr);
        let priv_ = self.get_mut(priv_id)?;
        if priv_.s_proc_nr.is_some() {
            return None; // slot 已被占用
        }
        priv_.s_proc_nr = Some(proc_nr);
        Some(priv_id)
    }

    /// 设置进程的能力。
    ///
    /// 将 Capability 写入对应的特权 slot。
    pub fn set_capability(&mut self, priv_id: PrivId, cap: Capability) {
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.s_flags = cap.flags;
            priv_.s_trap_mask = cap.trap_mask;
            priv_.s_ipc_to = cap.ipc_to;
            priv_.s_k_call_mask = cap.k_call_mask;
            priv_.s_sig_mgr = cap.sig_mgr;
        }
    }
}
```

**关键变更**：
- `assign_static` → `assign`（去掉 translate 味的 "static"）
- `configure_boot_priv(6 个裸参数)` → `set_capability(Capability)`（打包为 OS 概念类型）
- 新增 `Capability` 和 `BootCapability` 类型，让能力配置的类型一目了然

### 3.4 主流程：`init_proc_and_boot()`

```rust
// os/kernel/src/lib.rs

fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use minix_arch::arch::{ArchProcess, CurrentArch, CurrentRegs, BootRegs};
    use minix_arch::elf_loader::load_vm_elf;

    // Step 1: 初始化空表
    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();

    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES, kernel_info.boot_modules.len()
    );

    // Step 2: 遍历 boot image
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr = if i < NR_TASKS {
            (i as ProcNr) - (NR_TASKS as ProcNr)
        } else {
            (i - NR_TASKS) as ProcNr
        };

        let proc = proc_table.get_mut(nr)
            .expect("boot proc: invalid process number");

        proc.set_boot_name(module.name);

        let is_kernel = nr < 0;
        let is_vm = nr == proc_nr::VM_PROC_NR;
        let is_root_sys = nr == proc_nr::RS_PROC_NR;
        let schedulable = is_kernel || is_root_sys || is_vm;

        // Step 2a: 特权分配
        if schedulable {
            let priv_id = priv_table.assign(nr)
                .expect("assign: static priv slot occupied");

            let boot_cap = if is_vm {
                BootCapability::Vm
            } else if is_kernel {
                let flags = if nr == proc_nr::IDLE {
                    PrivFlagsBits::IDL_F
                } else {
                    PrivFlagsBits::TSK_F
                };
                BootCapability::KernelTask { flags }
            } else {
                BootCapability::RootSys
            };

            priv_table.set_capability(priv_id, boot_cap.expand(nr));
        } else {
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        // Step 2b: 构建初始寄存器状态
        // 这是唯一与 arch 交互的点——一次调用，返回完整状态
        let mut boot_regs = CurrentArch::boot_regs(is_kernel);

        // Step 2c: 用户进程设置入口点
        if !is_kernel {
            let (pc, sp, ps_strings) = if is_vm {
                // VM: 加载 ELF 到 bootstrap 页表
                #[cfg(not(feature = "mock"))]
                {
                    // 生产环境：需要真实的 bootstrap 页表
                    // 当前 deferred，等 VM bootstrap 页表支持就绪后启用
                    (VirBytes(0), VirBytes(0), VirBytes(0))
                }
                #[cfg(feature = "mock")]
                {
                    use minix_arch::paging::mock::MockPaging;
                    let mut paging = MockPaging::new();
                    let result = load_vm_elf(module, kernel_info, &mut paging);
                    (result.pc, result.sp, result.ps_strings)
                }
            } else {
                // 其他用户进程：入口点由 RS 在运行时设置
                (VirBytes(0), VirBytes(0), VirBytes(0))
            };

            boot_regs = boot_regs.with_entry(pc, sp, ps_strings);
        }

        // 整体存储——不拆解 arch 返回值
        proc.boot_regs = boot_regs;

        // Step 2d: VM inhibit（非 VM 用户进程等待 VM 创建页表）
        if nr != proc_nr::VM_PROC_NR && nr >= 0 {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }

        // Step 2e: 所有进程进入停止状态，清除 SLOT_FREE
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }
}
```

**关键变更**：
- arch 层只调用一次：`CurrentArch::boot_regs(is_kernel)` 返回完整状态
- kernel 层整体存储：`proc.boot_regs = boot_regs`（不拆解字段）
- 入口点设置通过 builder 模式：`boot_regs.with_entry(pc, sp, ps_strings)`
- `load_vm_elf` 是纯函数调用，不与 trait 耦合
- 特权分配通过 `BootCapability` 枚举，消除 6 个裸参数

### 3.5 KProcess setter 简化

```rust
// os/kernel/src/proc.rs

impl KProcess {
    // 删除：set_boot_initial_reg_state(status, fpu_needs_zero)
    // 删除：set_boot_pc_sp(pc, sp, ps_strings_reg)
    // 原因：arch 返回值现在整体存储，不需要拆解 setter

    /// 设置进程名。
    pub fn set_boot_name(&mut self, name: &str) {
        self.p_name = ProcName::from_str(name);
    }
}
```

### 3.6 调度器使用 boot_regs

调度器首次调度进程时，从 `proc.boot_regs` 读取初始状态写入 trap frame：

```rust
// os/kernel/src/sched.rs (示意)

fn setup_first_run(proc: &KProcess, frame: &mut TrapFrame) {
    use minix_arch::arch::BootRegs;

    frame.set_pc(proc.boot_regs.pc());
    frame.set_sp(proc.boot_regs.sp());
    frame.set_status(proc.boot_regs.status());
    frame.set_ps_strings_reg(proc.boot_regs.ps_strings_reg());

    // x86-64 特有：初始化 XSAVE area
    // aarch64/riscv64：无操作（编译时优化掉）
    #[cfg(target_arch = "x86_64")]
    {
        use minix_arch::x86_64::proc_arch::X86_64Regs;
        // 通过 trait 的关联类型无法直接访问 arch-specific 字段，
        // 但可以用 cfg 分支处理 arch-specific 逻辑。
        // 实际上，XSAVE 初始化应该由 arch 层提供 apply_to_trap_frame 方法处理。
        // 见 §3.7 的替代方案。
    }
}
```

**注意**：`boot_regs` 字段通过 `BootRegs` trait 的 4 个方法暴露了 kernel 需要的信息。但 x86-64 的 XSAVE 初始化、段选择子写入等 arch-specific 操作，需要额外的 arch 方法。详见 §3.7。

### 3.7 扩展点：`BootRegs` 的 `apply_to_trap_frame` 方法

如果 kernel 层不想在不同架构间写 `#[cfg]` 分支，可以让 `BootRegs` trait 包含一个 `apply_to_trap_frame` 方法：

```rust
// 方案 A：BootRegs 增加 apply_to_trap_frame
pub trait BootRegs: Sized {
    // ... 现有方法 ...

    /// 将寄存器状态应用到 trap frame。
    /// arch 层在内部处理所有硬件细节（段选择子、XSAVE、FPU 等）。
    fn apply_to(&self, frame: &mut TrapFrame);
}

// x86-64 实现
impl BootRegs for X86_64Regs {
    fn apply_to(&self, frame: &mut TrapFrame) {
        frame.rip = self.rip;
        frame.rsp = self.rsp;
        frame.rflags = self.rflags;
        frame.rbx = self.rbx;
        frame.cs = self.cs;
        frame.ds = self.ds;
        // ... 其他段选择子 ...
        if self.xsave_needs_init {
            frame.init_xsave_area();
        }
    }
}

// aarch64 实现
impl BootRegs for AArch64Regs {
    fn apply_to(&self, frame: &mut TrapFrame) {
        frame.elr = self.elr;
        frame.sp = self.sp;
        frame.spsr = self.spsr;
        frame.x0 = self.x0;
        // 无段选择子，无 XSAVE
    }
}
```

**推荐**：采用方案 A。让 `apply_to` 成为 `BootRegs` trait 的一部分，kernel 层只需调用 `proc.boot_regs.apply_to(&mut frame)`，完全不需要知道 arch-specific 细节。

---

## 4. 问题对照表

| 问题编号 | 严重度 | 描述 | 新设计如何解决 |
|---|---|---|---|
| #1 | P0 | `segment_selectors` 在 aarch64/riscv64 永远全零 | `CurrentRegs` 关联类型：aarch64 编译时 `AArch64Regs` 不包含 `SegmentSelectors` 字段 |
| #6 | P2 | 文档自称 "OS-semantic" 但 `segment_selectors` 用硬件术语 | 字段名统一为 arch 内部细节，kernel 层不接触 |
| #7 | P2 | `SegmentSelectors::default()` 全零表达"无意义" | 类型根本不存在于非 x86 架构，无需 default |
| #8 | P0 | `fpu_needs_zero` 翻译 Minix3 的 fnsave 模型 | `xsave_needs_init` 是 x86-64 Regs 内部字段，其他架构无此概念；`apply_to` 方法内部处理 |
| #9 | P1 | "init_regs 内部调用 reset" 是编造的因果链 | `boot_regs` 返回完整状态，`with_entry` 只是设置 PC/SP/ps_strings，无"内部调用"概念 |
| #10 | P0 | 3 个 trait 完全对应 3 个 C 函数 | 1 个 trait `ArchProcess` + 1 个纯函数 `load_vm_elf` |
| #11 | P0 | `Box<[KProcess]>` 和 `Box<[KPriv]>` 用堆 | `[KProcess; PROC_TABLE_SIZE]` 和 `[KPriv; NR_SYS_PROCS]` 固定数组 |
| #12 | P1 | `configure_boot_priv` 6 个裸参数，翻译 C 字段 | `set_capability(Capability)` + `BootCapability` 枚举，OS 概念命名 |
| #3 | P1 | kernel 层拆解 arch 返回值 | `proc.boot_regs = boot_regs` 整体存储 |
| #4 | P1 | `InitialRegs` 同样被拆解 | `boot_regs.with_entry()` builder 模式，整体返回 |
| #5 | P1 | `initial_status` 字段名泄漏 x86-64 语义 | kernel 层通过 `boot_regs.status()` 读取，不关心内部名称 |

---

## 5. 迁移步骤

### 5.1 阶段 1：数据结构无堆化（先修 #11）

1. `KProcess` 和 `KPriv` 添加 `const fn new_empty() -> Self`
2. `ProcessTable` 改为 `[KProcess; PROC_TABLE_SIZE]`
3. `PrivTable` 改为 `[KPriv; NR_SYS_PROCS]`
4. 删除所有 `use alloc::boxed::Box` 和 `use alloc::vec::Vec`
5. 验证：`cargo build --target x86_64-unknown-none` 不报链接错误

### 5.2 阶段 2：arch 层重构（修 #1, #8, #9, #10）

1. 定义 `ArchProcess` trait（1 个方法）和 `BootRegs` trait（4 个方法）
2. 定义三架构各自的 `Regs` 类型
3. 实现三架构的 `ArchProcess` 和 `BootRegs`
4. 提取 `load_vm_elf` 为纯函数
5. 删除旧的 `ArchProcReset`, `ArchProcInit`, `BootProcArch` trait
6. 删除旧的 `InitialRegState`, `InitialRegs`, `SegmentSelectors`

### 5.3 阶段 3：kernel 层适配（修 #3, #4, #5, #12）

1. `KProcess` 用 `boot_regs: CurrentRegs` 替换 4 个独立字段
2. 删除 `set_boot_initial_reg_state` 和 `set_boot_pc_sp`
3. 重写 `init_proc_and_boot` 主流程
4. 引入 `Capability` 和 `BootCapability` 类型
5. 重命名 `assign_static` → `assign`，`configure_boot_priv` → `set_capability`
6. 调度器适配：通过 `boot_regs.pc()` 等方法读取

### 5.4 阶段 4：测试适配

1. Mock 类型：`MockRegs` + `MockArch`
2. 测试 `ProcessTable::new()` 所有 slot 为 SLOT_FREE
3. 测试 `boot_regs()` 返回正确的 status 值
4. 测试 `with_entry()` 设置正确的 PC/SP/ps_strings
5. 测试 `BootCapability` 展开正确的 Capability
6. 测试 `load_vm_elf` 正确解析 ELF 和分配页面

---

## 6. 设计备选方案

### 6.1 是否需要 trait？

**当前方案**：1 个 trait `ArchProcess` + 1 个 trait `BootRegs`。

**备选方案 B**：完全去掉 trait，只靠类型别名 + 同名方法（duck typing）。

```rust
// 无 trait 版本
pub type CurrentRegs = X86_64Regs;

impl X86_64Regs {
    pub fn new(is_kernel: bool) -> Self { ... }
    pub fn with_entry(self, pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self { ... }
    pub fn pc(&self) -> VirBytes { ... }
    // ...
}
```

**对比**：

| 方面 | 有 trait | 无 trait |
|---|---|---|
| Mock 测试 | trait 实现，简单 | `#[cfg(test)]` 类型替换，需要 cfg 分支 |
| 编译时检查 | trait bound 保证方法存在 | 类型别名不保证方法签名一致 |
| 文档清晰度 | trait 明确表达"需要实现哪些方法" | 需要读各 arch 的 impl 才能知道完整 API |
| 复杂度 | 多 2 个 trait 定义 | 更简洁 |

**推荐**：保留 trait。两个 trait 的开销很小（`BootRegs` 4 个方法，`ArchProcess` 1 个方法），但提供了编译时保证和 mock 测试的便利性。这不像当前 3 个 trait 的"翻译 C 函数"问题——这里的 trait 表达的是 OS 概念（"一个新进程需要初始寄存器状态"）。

### 6.2 `BootRegs` 是否需要 `apply_to` 方法？

**方案 A**（推荐）：`BootRegs` 包含 `apply_to(&self, frame: &mut TrapFrame)`。

**方案 B**：kernel 层通过 `pc()`/`sp()`/`status()`/`ps_strings_reg()` 读取，自己写入 trap frame。

**对比**：

| 方面 | 方案 A | 方案 B |
|---|---|---|
| kernel 层代码 | `proc.boot_regs.apply_to(&mut frame)` | 4 行逐个读取 + 写入 |
| arch-specific 逻辑 | 全部在 `apply_to` 内部 | kernel 需要 `#[cfg]` 处理 x86 段选择子 |
| trait 方法数 | 5 个 | 4 个 |

**推荐**：方案 A。`apply_to` 让 arch 层完全消化硬件细节，kernel 层不需要 `#[cfg]` 分支。

---

## 7. 附录：文件变更清单

| 文件 | 变更类型 | 估计行数 |
|---|---|---|
| `os/arch/src/arch/proc_arch.rs` | 重写 | ±150 |
| `os/arch/src/x86_64/proc_arch.rs` | 重写 | ±60 |
| `os/arch/src/arm64/proc_arch.rs` | 重写 | ±50 |
| `os/arch/src/riscv64/proc_arch.rs` | 重写 | ±50 |
| `os/arch/src/arch/elf_loader.rs` | 新增 | ±100 |
| `os/kernel/src/proc.rs` | 修改 | -40 |
| `os/kernel/src/proc_table.rs` | 修改 | ±30 |
| `os/kernel/src/kpriv.rs` | 修改 | ±80 |
| `os/kernel/src/lib.rs` | 修改 | ±50 |
| `06-proc-init-boot-proc.md` §3-§4 | 重写 | ±500 |
| **总计** | | **~1100 行** |