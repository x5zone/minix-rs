# 06-proc-init-boot-proc 重新设计（Kimi）

> **状态**：设计方案（供多 AI bagging 后统一定稿）  
> **范围**：`os/arch/src/{arch,x86_64,arm64,riscv64}/proc_arch.rs`、`os/kernel/src/{proc,proc_table,kpriv,lib,syscall_device}.rs` 以及文档 `06-proc-init-boot-proc.md` 的第三章、第四章。  
> **前提**：本设计完全独立撰写，未阅读 `06-design-*.md` 中其他 AI 的输出。

---

## 1. 设计目标与核心原则

本轮重构不只是修 bug，而是把 06 模块从「翻译 Minix3 C 代码」改造成「用 Rust 类型系统重新表达同一套 OS 语义」。必须同时满足三条不可违背的原则：

| # | 原则 | 含义 | 反模式 |
|---|---|---|---|
| P1 | **硬件语义不泄漏到 OS 层** | 编译后只支持一种架构；跨架构 trait/结构体里不能出现「对当前架构无意义」的字段或方法。 | `InitialRegState::segment_selectors` 出现在 aarch64/riscv64 代码路径中。 |
| P2 | **rewrite，不要 translate** | API 命名、类型拆分、trait 边界从 OS 概念出发，不从 C 函数名出发。 | 三个 trait 分别叫 `ArchProcReset/ArchProcInit/BootProcArch`，方法名对应 `arch_proc_reset/arch_proc_init/arch_boot_proc`。 |
| P3 | **boot 阶段绝不用堆** | `init_proc_and_boot()` 执行时 VM 尚未启动，没有 `GlobalAlloc`；`ProcessTable`、`PrivTable` 必须是固定大小数组。 | `Box<[KProcess]>`、`Box<[KPriv]>`。 |

在这三条原则之上，补充两条工程原则：

- **P4：arch 层返回整体值，kernel 层整体接受。** kernel 不应该把 arch 返回的寄存器状态拆成 `status`、`fpu_needs_zero`、`pc`、`sp`、`ps_strings_reg` 再分别塞入 `KProcess`；它应该拿到一个不可拆的 arch 上下文对象，原样保存，调度时原样交给 arch 层应用。
- **P5：OS 类型名必须是 OS 概念名，不用硬件术语。** `SegmentSelectors`、`ps_strings_reg` 这类名字必须下沉到具体架构 crate，不能出现在跨架构接口。

---

## 2. 问题诊断（基于 06-problem.md 与源码核实）

### 2.1 硬件语义泄漏（P0）

当前 `os/arch/src/arch/proc_arch.rs:39-50` 定义：

```rust
pub struct InitialRegState {
    pub status: u64,
    pub segment_selectors: SegmentSelectors,  // x86-64 特有
    pub fpu_needs_zero: bool,                 // x86-64 语义
}
```

`SegmentSelectors` 直接编码 x86-64 GDT 选择子；aarch64/riscv64 实现被迫 `SegmentSelectors::default()` 并写注释「all zero」。更严重的是，`os/kernel/src/lib.rs:747-748` 只用 `status` 和 `fpu_needs_zero`，`segment_selectors` 在 kernel 层被完全丢弃。这证明该字段不是为了 OS 层，而是为了 arch 层自己，却通过 trait 返回值强加给所有架构。

### 2.2 Translate 味（P0/P1）

三个 trait 的命名与文档注释几乎逐字对应 C 函数：

| Rust trait | C 函数 | 当前问题 |
|---|---|---|
| `ArchProcReset` | `arch_proc_reset()` | trait 名 = C 函数名 |
| `ArchProcInit` | `arch_proc_init()` | 文档说「init 内部调用 reset」，但 Rust 实现里并不调用，这是编造的因果链 |
| `BootProcArch` | `arch_boot_proc()` | 单方法 trait，且方法名照搬 C |

`PrivTable::configure_boot_priv` 直接翻译 6 个 C 字段（`flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr`），属于典型的裸整数 translate。

### 2.3 Boot 阶段用堆（P0）

`os/kernel/src/proc_table.rs:118` 和 `os/kernel/src/kpriv.rs:221`：

```rust
procs: Box<[KProcess]>,
privs: Box<[KPriv]>,
```

构造时使用 `Vec::new().into_boxed_slice()`。Boot 阶段没有 `GlobalAlloc`（`extern crate alloc` 存在，但 boot 时未初始化分配器），这会导致链接或运行时 panic。

### 2.4 Kernel 层拆解 arch 返回值（P1）

```rust
let reg_state = CurrentBootProcArch::initial_reg_state(true, nr);
proc.set_boot_initial_reg_state(reg_state.status, reg_state.fpu_needs_zero);
```

`segment_selectors` 被丢弃，`fpu_needs_zero` 被收下又丢弃（`os/kernel/src/proc.rs:1144-1152` 中 `_fpu_needs_zero` 未使用）。arch 层设计为「返回纯值」，kernel 层却立刻拆解，破坏了分层语义。

### 2.5 FPU 与 IOPL 的硬件术语残留（P0/P1）

- `fpu_needs_zero` 翻译 Minix3 的 `fnsave/fxrstor` 模型，未考虑 x86-64 XSAVE、aarch64 CPACR_EL1.FPEN、riscv64 sstatus.FS。
- `syscall_device.rs:484-523` 直接操作 `initial_status` 的 `X86_64_IOPL_BITS`，把 x86-64 RFLAGS 位操作放在 kernel 层。

---

## 3. 新架构总览

把「boot 一个进程」抽象成三个 OS 概念，而不是三个 C 函数：

1. **初始执行上下文（Initial Execution Context）**：一个进程第一次被调度时，CPU 需要的全部寄存器状态。对 OS 层是不透明整体；arch 层知道内部字段。
2. **引导特权画像（Boot Privilege Profile）**：boot 阶段为某类进程预定义的「能做什么」模板（Idle、KernelTask、VM、RootService、Service、User）。
3. **引导镜像条目（Boot Image Entry）**：内核编译时自带的进程清单，描述每个 boot 进程的名字、进程号、特权画像。

新的层次关系：

```
kernel layer
│   ProcessTable ── fixed [KProcess; N]
│   PrivTable    ── fixed [KPriv; M]
│   KProcess     ── stores <CurrentBootProcArch as BootProcArch>::Context
│   init_proc_and_boot()
│
└── arch trait (cross-arch interface)
        BootProcArch {
            type Context: BootContext;
            fn base_context(is_kernel, proc_nr) -> Context;
            fn with_user_entry(base, UserEntry) -> Context;
            fn apply_context(&Context, &mut TrapFrame);
            fn enable_user_io(&mut Context);
            fn load_vm_elf<P: Paging>(...) -> Result<VmBootImage, VmLoadError>;
        }
        BootContext {
            type TrapFrame;
            fn apply_to_frame(&self, &mut TrapFrame);
        }
        UserEntry { pc, sp, ps_strings }
        VmBootImage { entry: UserEntry, allocated_bytes }
        
    └── x86_64 impl
            X86_64BootContext { status, segment_selectors, user_entry }
            X86_64SegmentSelectors { cs, ds, ss, es, fs, gs }
            FPU/XSAVE init inside apply_to_frame
    └── aarch64 impl
            AArch64BootContext { status, user_entry }
            no segment selectors, lazy VFP init inside apply_to_frame
    └── riscv64 impl
            Riscv64BootContext { status, user_entry }
            no segment selectors, lazy F/D init inside apply_to_frame
```

关键变化：

- **一个 trait 替代三个 trait**：`BootProcArch` 足以表达 boot 阶段所有 arch 相关行为。
- **关联类型隔离硬件字段**：`Context` 由具体架构定义，aarch64/riscv64 的类型系统中不存在 `SegmentSelectors`。
- **kernel 层不拆字段**：`KProcess` 只存一个 `initial_context` 字段，调度时整体应用。
- **boot 阶段零堆**：`ProcessTable`、`PrivTable` 改用 `[KProcess; N]`、`[KPriv; M]`。
- **特权画像抽象**：`configure_boot_priv` 的 6 个裸参数被一个 `BootProfile` 枚举替代。

---

## 4. Arch 层 redesign

### 4.1 新的 trait 定义

文件：`os/arch/src/arch/proc_arch.rs`

```rust
//! Process boot-time architecture abstraction.
//!
//! Provides a single, concept-driven trait `BootProcArch` that answers:
//!   1. What is the initial CPU context for a fresh process slot?
//!   2. How does that context change when the process gets a user entry point?
//!   3. How is the VM ELF loaded into the bootstrap address space?
//!   4. How is the context applied to a trap frame on first schedule?
//!
//! All hardware-specific fields (segment selectors, FPU model, IOPL bits)
//! live in the per-arch `Context` type, never in this cross-arch interface.

use core::fmt::Debug;
use minix_types::{VirBytes, PhysBytes};
use minix_boot::{KernelInfo, BootModule};
use crate::paging::Paging;

/// Architecture-independent description of a user process entry point.
///
/// The kernel knows the virtual addresses of the entry point, stack top,
/// and ps_strings block. It does *not* know which physical register holds
/// ps_strings — that is decided by the arch layer inside `with_user_entry`.
#[derive(Debug, Clone, Copy)]
pub struct UserEntry {
    /// Program counter (entry point).
    pub pc: VirBytes,
    /// Stack pointer.
    pub sp: VirBytes,
    /// Address of the ps_strings structure on the user stack.
    pub ps_strings: VirBytes,
}

/// Result of loading the VM ELF into the bootstrap page table.
#[derive(Debug, Clone, Copy)]
pub struct VmBootImage {
    /// Entry point + stack + ps_strings for the VM process.
    pub entry: UserEntry,
    /// Total bytes mapped for VM in the bootstrap address space.
    pub allocated_bytes: usize,
}

/// Architecture-specific initial execution context.
///
/// The kernel stores this context in `KProcess::initial_context` and later
/// passes it to `BootProcArch::apply_context` when the process is scheduled
/// for the first time. The kernel never inspects the interior.
pub trait BootContext: Copy + Debug {
    type TrapFrame;

    /// Apply this context to a trap frame.
    ///
    /// This is the only arch-specific point that knows how to write the
    /// initial register values into the scheduler's saved exception frame.
    /// FPU/extended-register initialization is performed here, not exposed
    /// as a separate flag to the kernel.
    fn apply_to_frame(&self, frame: &mut Self::TrapFrame);
}

/// Architecture abstraction for boot-time process initialization.
///
/// A single trait replaces the previous `ArchProcReset`, `ArchProcInit`,
/// and `BootProcArch`. The split was driven by C function boundaries;
/// the unified trait is driven by the OS concept "how this architecture
/// boots a process".
pub trait BootProcArch {
    /// Architecture-specific initial context type.
    type Context: BootContext;

    /// Create the base context for a fresh process slot.
    ///
    /// `is_kernel` is true for kernel tasks (`p_nr < 0`), false for user
    /// processes. The returned context has no user entry point; call
    /// `with_user_entry` to add one.
    fn base_context(is_kernel: bool, proc_nr: i32) -> Self::Context;

    /// Augment a base context with a user entry point.
    fn with_user_entry(base: Self::Context, entry: UserEntry) -> Self::Context;

    /// Apply a context to a trap frame.
    fn apply_context(ctx: &Self::Context, frame: &mut <Self::Context as BootContext>::TrapFrame);

    /// Enable user-mode I/O access for the process, if the architecture
    /// supports such a concept. Default implementation is a no-op.
    ///
    /// On x86-64 this sets IOPL=3 in the saved RFLAGS. On aarch64/riscv64
    /// there is no direct equivalent, so the syscall dispatch should return
    /// ENOSYS instead of calling this.
    fn enable_user_io(ctx: &mut Self::Context) {
        let _ = ctx;
    }

    /// Load the VM ELF binary into the bootstrap page table.
    ///
    /// Returns the user entry point and memory statistics. On ELF parse
    /// failure, returns `Err(VmLoadError)` instead of a zeroed result.
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Result<VmBootImage, VmLoadError>;
}

/// Error type for VM ELF loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLoadError {
    InvalidElf,
    NoLoadableSegments,
    MappingFailed,
}
```

### 4.2 当前架构类型别名

文件：`os/arch/src/lib.rs`

保持 `CurrentBootProcArch` 别名机制，指向新的单 trait：

```rust
#[cfg(feature = "mock")]
pub type CurrentBootProcArch = crate::arch::proc_arch::MockProcArch;

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentBootProcArch = crate::x86_64::proc_arch::X86_64ProcArch;

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentBootProcArch = crate::arm64::proc_arch::AArch64ProcArch;

#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentBootProcArch = crate::riscv64::proc_arch::Riscv64ProcArch;

/// Convenience alias for the current arch's boot context type.
pub type CurrentBootContext = <CurrentBootProcArch as BootProcArch>::Context;
```

### 4.3 x86-64 实现

文件：`os/arch/src/x86_64/proc_arch.rs`

```rust
use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    BootProcArch, BootContext, UserEntry, VmBootImage, VmLoadError,
};
use crate::paging::{Paging, PageFlags};

/// x86-64 segment selectors. This type is private to the x86-64 crate;
/// it never appears in the cross-arch `BootProcArch` interface.
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64SegmentSelectors {
    pub cs: u64,
    pub ds: u64,
    pub ss: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

/// x86-64 initial execution context.
#[derive(Debug, Clone, Copy)]
pub struct X86_64BootContext {
    pub status: u64,                              // RFLAGS
    pub segment_selectors: X86_64SegmentSelectors,
    pub user_entry: Option<UserEntry>,
}

impl BootContext for X86_64BootContext {
    type TrapFrame = crate::x86_64::trap::TrapFrame; // to be added

    fn apply_to_frame(&self, frame: &mut Self::TrapFrame) {
        // 1. General status and segment selectors.
        frame.rflags = self.status;
        frame.cs = self.segment_selectors.cs;
        // ... ds/ss/etc as appropriate for the trap frame layout

        // 2. User entry point, if any.
        if let Some(entry) = self.user_entry {
            frame.rip = entry.pc.0;
            frame.rsp = entry.sp.0;
            frame.rbx = entry.ps_strings.0;
        }

        // 3. FPU/extended register state.
        //    Modern x86-64 uses XSAVE. For a brand-new user process we
        //    initialize the XSAVE area to zero and set xstate_bv=0.
        //    Kernel tasks do not need FPU state.
        //    (Implementation detail: this calls into the arch FPU helper.)
    }
}

pub struct X86_64ProcArch;

impl BootProcArch for X86_64ProcArch {
    type Context = X86_64BootContext;

    fn base_context(is_kernel: bool, _proc_nr: i32) -> Self::Context {
        let status = if is_kernel { INIT_TASK_PSW } else { INIT_PSW };
        Self::Context {
            status,
            segment_selectors: X86_64SegmentSelectors {
                cs: USER_CS_SELECTOR,
                ds: USER_DS_SELECTOR,
                ss: USER_DS_SELECTOR,
                es: USER_DS_SELECTOR,
                fs: USER_DS_SELECTOR,
                gs: USER_DS_SELECTOR,
            },
            user_entry: None,
        }
    }

    fn with_user_entry(base: Self::Context, entry: UserEntry) -> Self::Context {
        Self::Context { user_entry: Some(entry), ..base }
    }

    fn apply_context(ctx: &Self::Context, frame: &mut <Self::Context as BootContext>::TrapFrame) {
        ctx.apply_to_frame(frame);
    }

    fn enable_user_io(ctx: &mut Self::Context) {
        ctx.status |= X86_64_IOPL_BITS; // 0x3000
    }

    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Result<VmBootImage, VmLoadError> {
        // Common ELF-loading helper defined below.
        load_vm_elf_common(module, kernel_info, paging)
    }
}
```

### 4.4 aarch64 实现

文件：`os/arch/src/arm64/proc_arch.rs`

```rust
/// AArch64 initial execution context.
/// Note: no segment selectors, no FPU zero flag.
#[derive(Debug, Clone, Copy)]
pub struct AArch64BootContext {
    pub status: u64, // SPSR_EL1 value
    pub user_entry: Option<UserEntry>,
}

impl BootContext for AArch64BootContext {
    type TrapFrame = crate::arm64::trap::TrapFrame;

    fn apply_to_frame(&self, frame: &mut Self::TrapFrame) {
        frame.spsr_el1 = self.status;
        if let Some(entry) = self.user_entry {
            frame.elr_el1 = entry.pc.0;
            frame.sp_el0 = entry.sp.0;
            frame.x[0] = entry.ps_strings.0; // r0 / x0
        }
        // FPU/NEON: CPACR_EL1.FPEN is set globally; per-process VFP state
        // is lazily initialized on first FP access trap.
    }
}

impl BootProcArch for AArch64ProcArch {
    type Context = AArch64BootContext;

    fn base_context(is_kernel: bool, _proc_nr: i32) -> Self::Context {
        let status = if is_kernel { INIT_TASK_PSR } else { INIT_PSR };
        Self::Context { status, user_entry: None }
    }

    fn with_user_entry(base: Self::Context, entry: UserEntry) -> Self::Context {
        Self::Context { user_entry: Some(entry), ..base }
    }

    fn apply_context(ctx: &Self::Context, frame: &mut <Self::Context as BootContext>::TrapFrame) {
        ctx.apply_to_frame(frame);
    }

    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Result<VmBootImage, VmLoadError> {
        load_vm_elf_common(module, kernel_info, paging)
    }
}
```

### 4.5 riscv64 实现

与 aarch64 类似，context 中无段选择子、无 FPU flag。

```rust
/// RISC-V 64-bit initial execution context.
#[derive(Debug, Clone, Copy)]
pub struct Riscv64BootContext {
    pub status: u64, // sstatus
    pub user_entry: Option<UserEntry>,
}

impl BootContext for Riscv64BootContext {
    type TrapFrame = crate::riscv64::trap::TrapFrame;

    fn apply_to_frame(&self, frame: &mut Self::TrapFrame) {
        frame.sstatus = self.status;
        if let Some(entry) = self.user_entry {
            frame.sepc = entry.pc.0;
            frame.sp = entry.sp.0;
            frame.x[10] = entry.ps_strings.0; // a0
        }
        // F/D extension: sstatus.FS set to Initial; lazily zeroed on first use.
    }
}
```

### 4.6 共享的 ELF 加载逻辑

三架构的 `load_vm_elf` 目前几乎完全相同，仅 `elf_flags_to_page_flags` 有细微差别。应提取公共实现：

```rust
// os/arch/src/arch/proc_arch.rs (or 新增 os/arch/src/arch/vm_elf.rs)

pub fn load_vm_elf_common<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> Result<VmBootImage, VmLoadError> {
    let image = unsafe {
        core::slice::from_raw_parts(module.start.0 as *const u8, module.len)
    };

    let iter = minix_elf::segment_iter(image).map_err(|_| VmLoadError::InvalidElf)?;
    let entry = minix_elf::entry_point(image).ok_or(VmLoadError::InvalidElf)?;

    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated: usize = 0;

    for seg in iter {
        let flags = P::elf_segment_to_flags(seg.flags);
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1);
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            // TODO: 真正的 bootstrap 物理页分配应来自 boot bump allocator，
            // 而不是 1:1 identity。当前实现保留 1:1 作为过渡，直到 boot allocator
            // 接入 load_vm_elf。
            let paddr = PhysBytes(vaddr);
            paging.map(VirBytes(vaddr), paddr, flags)
                .map_err(|_| VmLoadError::MappingFailed)?;
            total_allocated += page_size as usize;

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
                            dst_ptr,
                            copy_len,
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
        let paddr = PhysBytes(stack_addr); // TODO: 同上
        paging.map(VirBytes(stack_addr), paddr, stack_flags)
            .map_err(|_| VmLoadError::MappingFailed)?;
        total_allocated += page_size as usize;
        stack_addr += page_size;
    }

    let ps_strings = VirBytes(sp.0 - 32);

    Ok(VmBootImage {
        entry: UserEntry {
            pc: VirBytes(entry),
            sp,
            ps_strings,
        },
        allocated_bytes: total_allocated,
    })
}
```

`Paging` trait 新增一个关联函数把 ELF segment flags 转成 `PageFlags`：

```rust
pub trait Paging {
    // ... existing methods ...
    fn elf_segment_to_flags(elf_flags: u32) -> PageFlags;
}
```

这样三架构 ELF 加载代码只剩 flag 转换不同。

---

## 5. Kernel 层 redesign

### 5.1 KProcess 字段精简

文件：`os/kernel/src/proc.rs`

删除这四个字段：

```rust
// 删除
pub initial_pc: VirBytes,
pub initial_sp: VirBytes,
pub initial_ps_strings_reg: u64,
pub initial_status: u64,
```

替换为一个整体 arch 上下文：

```rust
use minix_arch::CurrentBootContext;

pub struct KProcess {
    // ... existing fields ...

    /// Initial execution context for this process.
    /// Set during boot (or fork/exec reset) by the arch layer.
    /// Consumed by the scheduler when the process is first dispatched.
    pub initial_context: CurrentBootContext,
}
```

对应的 setter：

```rust
impl KProcess {
    /// Set the architecture-specific initial context.
    pub fn set_boot_context(&mut self, ctx: CurrentBootContext) {
        self.initial_context = ctx;
    }
}
```

调度器首次调度进程时：

```rust
// 在 scheduler/context-switch 路径中
let ctx = &proc.initial_context;
CurrentBootProcArch::apply_context(ctx, &mut trap_frame);
```

### 5.2 消除 boot 阶段的堆分配

#### ProcessTable

文件：`os/kernel/src/proc_table.rs`

```rust
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: VmRequestQueue,
}

impl ProcessTable {
    pub fn new() -> Self {
        // 先放置一个 const-constructible 的占位数组。
        let mut procs = [const { KProcess::new_empty() }; PROC_TABLE_SIZE];

        for i in 0..PROC_TABLE_SIZE {
            let nr = (i as ProcNr) - (NR_TASKS as ProcNr);
            let endpoint = Endpoint::from_generation_slot(0, nr);
            procs[i] = KProcess::new(nr, endpoint);
        }

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

前提：`KProcess::new_empty()` 必须是 `const fn`，所有字段的默认值都必须是 const-constructible。当前代码中需要调整的主要是：

- `Message::default()` 改为 `const fn default()`。
- `VmSuspendContext` 的默认值为 `None`，`None::<T>` 在 const 中可用只要 `T: Sized`。
- `ProcessSegments::default()` 改为 `const fn default()`。
- 各 `Atomic*::new(0)` 已经 const stable。

如果某些字段暂时无法 const 初始化，可用 `MaybeUninit` 替代（详见 §8.2）。

#### PrivTable

文件：`os/kernel/src/kpriv.rs`

```rust
pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    pub fn new() -> Self {
        let mut privs = [const { KPriv::new_empty() }; NR_SYS_PROCS];
        for i in 0..NR_SYS_PROCS {
            privs[i] = KPriv::new(i as SysId);
        }
        Self { privs }
    }
}
```

同样要求 `KPriv::new_empty()` 为 `const fn`。

### 5.3 特权画像：替代 `configure_boot_priv`

文件：`os/kernel/src/kpriv.rs`

#### 新类型：IpcBitmap、KernelCallMask

把裸 `u64` 和 `[u32; 2]` 包成语义类型：

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcBitmap(u64);

impl IpcBitmap {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(!0);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KernelCallMask([u32; SYS_CALL_MASK_SIZE]);

impl KernelCallMask {
    pub const NONE: Self = Self([0; SYS_CALL_MASK_SIZE]);
    pub const ALL: Self = Self([0xFFFF_FFFF; SYS_CALL_MASK_SIZE]);
}
```

#### 新类型：ProcessCapability

```rust
/// A process's boot-time privilege set, expressed as OS concepts.
#[derive(Debug, Clone, Copy)]
pub struct ProcessCapability {
    pub flags: PrivFlagsBits,
    pub trap_mask: u16,
    pub ipc_targets: IpcBitmap,
    pub kernel_calls: KernelCallMask,
}

impl ProcessCapability {
    pub const IDLE: Self = Self {
        flags: priv_flag_set::IDL_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::NONE,
        kernel_calls: KernelCallMask::NONE,
    };

    pub const KERNEL_TASK: Self = Self {
        flags: priv_flag_set::TSK_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::NONE,
        kernel_calls: KernelCallMask::NONE,
    };

    pub const VM: Self = Self {
        flags: priv_flag_set::VM_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::ALL,
        kernel_calls: KernelCallMask::ALL,
    };

    pub const ROOT_SERVICE: Self = Self {
        flags: priv_flag_set::RSYS_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::ALL,
        kernel_calls: KernelCallMask::ALL,
    };

    pub const SERVICE: Self = Self {
        flags: priv_flag_set::SRV_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::ALL,
        kernel_calls: KernelCallMask::ALL,
    };

    pub const USER: Self = Self {
        flags: priv_flag_set::USR_F,
        trap_mask: 0,
        ipc_targets: IpcBitmap::NONE,
        kernel_calls: KernelCallMask::NONE,
    };
}
```

#### 新枚举：BootProfile

```rust
/// Predefined privilege profile for a boot-time process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootProfile {
    Idle,
    KernelTask,
    Vm,
    RootService,
    Service,
    User,
}

impl BootProfile {
    pub fn capability(self) -> ProcessCapability {
        match self {
            Self::Idle => ProcessCapability::IDLE,
            Self::KernelTask => ProcessCapability::KERNEL_TASK,
            Self::Vm => ProcessCapability::VM,
            Self::RootService => ProcessCapability::ROOT_SERVICE,
            Self::Service => ProcessCapability::SERVICE,
            Self::User => ProcessCapability::USER,
        }
    }

    /// True if this profile should be made schedulable during boot.
    pub fn schedulable_at_boot(self) -> bool {
        matches!(self, Self::Idle | Self::Vm | Self::RootService)
    }

    /// Map a kernel task number to its profile.
    pub fn for_kernel_task(nr: ProcNr) -> Self {
        if nr == proc_nr::IDLE { Self::Idle } else { Self::KernelTask }
    }

    /// Map a user boot module number to its profile.
    pub fn for_user_module(nr: ProcNr) -> Self {
        match nr {
            proc_nr::VM_PROC_NR => Self::Vm,
            proc_nr::RS_PROC_NR => Self::RootService,
            _ => Self::User,
        }
    }
}
```

#### PrivTable 新方法

```rust
impl PrivTable {
    /// Grant a boot-time privilege profile to a process.
    ///
    /// Returns the assigned privilege id, or `None` if the slot is already
    /// occupied or out of range.
    pub fn grant_boot_profile(
        &mut self,
        proc_nr: ProcNr,
        profile: BootProfile,
        sig_mgr: Endpoint,
    ) -> Option<PrivId> {
        let priv_id = if proc_nr < 0 {
            (NR_TASKS as i32 + proc_nr) as PrivId
        } else {
            (NR_TASKS as PrivId + proc_nr as PrivId) as PrivId
        };

        let priv_ = self.get_mut(priv_id)?;
        if priv_.s_proc_nr.is_some() {
            return None;
        }

        let cap = profile.capability();
        priv_.s_proc_nr = Some(proc_nr);
        priv_.s_flags = cap.flags;
        priv_.s_trap_mask = cap.trap_mask;
        priv_.s_ipc_to = cap.ipc_targets.0;
        priv_.s_k_call_mask = cap.kernel_calls.0;
        priv_.s_sig_mgr = sig_mgr;

        Some(priv_id)
    }
}
```

### 5.4 init_proc_and_boot 新流程

文件：`os/kernel/src/lib.rs`

```rust
pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    use minix_arch::{BootProcArch, CurrentBootProcArch, UserEntry};
    use crate::proc_table::ProcessTable;
    use crate::kpriv::{PrivTable, BootProfile};
    use crate::proc::{proc_nr, KERNEL_TASKS, BOOT_MODULE_PROC_NRS, NR_BOOT_MODULES};

    // Step 1: heap-free tables.
    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();

    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES,
        kernel_info.boot_modules.len()
    );

    // Step 2: kernel tasks.
    for &(name, nr) in KERNEL_TASKS.iter() {
        let proc = proc_table.get_mut(nr).expect("kernel task slot missing");
        proc.set_boot_name(name);

        let profile = BootProfile::for_kernel_task(nr);
        let sig_mgr = Endpoint::NONE;
        priv_table.grant_boot_profile(nr, profile, sig_mgr)
            .expect("kernel task priv slot occupied");

        let ctx = CurrentBootProcArch::base_context(true, nr);
        proc.set_boot_context(ctx);

        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 3: user boot modules.
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr = BOOT_MODULE_PROC_NRS[i];
        let proc = proc_table.get_mut(nr).expect("user module slot missing");
        proc.set_boot_name(module.name);

        let profile = BootProfile::for_user_module(nr);

        if profile.schedulable_at_boot() {
            let sig_mgr = Endpoint::from_generation_slot(0, nr);
            priv_table.grant_boot_profile(nr, profile, sig_mgr)
                .expect("user module priv slot occupied");
        } else {
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        let base_ctx = CurrentBootProcArch::base_context(false, nr);

        let entry = if profile == BootProfile::Vm {
            #[cfg(feature = "mock")]
            {
                use minix_arch::paging::mock::MockPaging;
                let mut paging = MockPaging::new_from_page(PhysBytes(0));
                let image = CurrentBootProcArch::load_vm_elf(module, kernel_info, &mut paging)
                    .unwrap_or_else(|_| VmBootImage {
                        entry: UserEntry {
                            pc: VirBytes(0),
                            sp: VirBytes(0),
                            ps_strings: VirBytes(0),
                        },
                        allocated_bytes: 0,
                    });
                image.entry
            }
            #[cfg(not(feature = "mock"))]
            {
                // TODO(P0): 接入真正的 bootstrap page table 与物理页分配器。
                // 当前返回零值占位，RS 将在 userspace bring-up 阶段重新加载 VM ELF。
                let _ = module;
                UserEntry {
                    pc: VirBytes(0),
                    sp: VirBytes(0),
                    ps_strings: VirBytes(0),
                }
            }
        } else {
            UserEntry {
                pc: VirBytes(0),
                sp: VirBytes(0),
                ps_strings: VirBytes(0),
            }
        };

        let ctx = CurrentBootProcArch::with_user_entry(base_ctx, entry);
        proc.set_boot_context(ctx);

        if profile != BootProfile::Vm {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    proc_table
}
```

### 5.5 IOPL 下沉到 arch 层

文件：`os/kernel/src/syscall_device.rs`

删除直接操作 `initial_status` 的代码，改为调用 arch trait：

```rust
pub fn dispatch_iopenable(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    // ... endpoint validation ...

    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    if let Some(target) = proc_table.get_mut(target_nr) {
        CurrentBootProcArch::enable_user_io(&mut target.initial_context);
    }

    KcallResult::Ok(0)
}
```

x86-64 的 `enable_user_io` 实现设置 `status |= 0x3000`；aarch64/riscv64 使用默认 no-op。syscall 表层面仍然只在 x86-64 暴露 `SYS_IOPENABLE`（其他架构返回 `ENOSYS`），但关键状态修改不再散落到 kernel 层的硬件位操作中。

---

## 6. FPU / 扩展寄存器处理

### 6.1 原则

- **kernel 层不持有 FPU 状态初始化的语义**。`fpu_needs_zero` 字段彻底删除。
- arch 层在 `BootContext::apply_to_frame` 中根据进程类型和硬件能力决定如何初始化扩展寄存器。

### 6.2 各架构策略

| 架构 | 首次调度时的行为 | 说明 |
|---|---|---|
| x86-64 | 用户进程：分配/清零 XSAVE 区域（x87/SSE/AVX 状态全零），`xstate_bv = 0`；设置 `CR0.TS`/`XCR0` 由 arch FPU 助手管理。 | 替代旧的 `fnsave/fxrstor` 清零模型。 |
| aarch64 | 用户进程：无需立即清零；`CPACR_EL1.FPEN` 全局启用后，首次 FP/NEON 指令触发陷阱，陷阱处理程序清零 VFP 上下文。 | lazy VFP，与当前实现一致。 |
| riscv64 | 用户进程：设置 `sstatus.FS = Initial`；首次 F/D 指令触发非法指令陷阱，陷阱处理程序清零 F 寄存器。 | lazy F/D。 |
| 全部 | 内核任务：不初始化 FPU 状态；内核代码不使用 FP/SIMD，或只在显式保存/恢复的小窗口使用。 | 与 Minix3 一致。 |

---

## 7. 文档重写指引

文档 `06-proc-init-boot-proc.md` 的第三章、第四章需要随代码一起重写，核心方向：

### 7.1 第三章：从 C 函数对应表 → OS 概念决策

删除以下翻译味内容：

- 「`ArchProcReset` 对应 `arch_proc_reset()`」这类表格。
- 「`configure_boot_priv` 对应 main.c:202-243」的表述。
- 「init 内部调用 reset」等编造因果链。

改为：

- §3.1 解释「初始执行上下文」概念：为什么 arch 层返回一个整体对象，kernel 层不拆字段。
- §3.2 解释「关联类型」如何隔离硬件字段，aarch64/riscv64 看不到 `SegmentSelectors`。
- §3.3 解释「BootProcArch 单 trait」的边界：一个架构如何 boot 一个进程。
- §3.4 解释「BootProfile」：为什么 boot 阶段特权是预定义模板，而不是 6 个裸参数。
- §3.5 解释「boot 阶段零堆」：`ProcessTable`/`PrivTable` 为什么必须是固定数组。

### 7.2 第四章：按数据结构 + 流程组织

- §4.0 实现地图：数据结构与主流程。
- §4.1 `BootProcArch` / `BootContext` / `UserEntry` / `VmBootImage` 类型定义。
- §4.2 三架构 `Context` 实现对照表（按 CPU 四个问题组织，不是按 C 函数）。
- §4.3 `ProcessTable::new()` 与 `PrivTable::new()` 的固定数组初始化。
- §4.4 `init_proc_and_boot()` 主流程（Step 1/2/3）。
- §4.5 `KProcess` 字段变更：`initial_context` 替代四个分散字段。
- §4.6 调度器如何应用 `initial_context`。
- §4.7 FPU 初始化策略。
- §4.8 IOPL 下沉到 arch 层。

---

## 8. 实现顺序与迁移计划

### 8.1 推荐实施顺序

1. **新增类型与 trait**：在 `os/arch/src/arch/proc_arch.rs` 定义 `UserEntry`、`VmBootImage`、`VmLoadError`、`BootContext`、`BootProcArch`。
2. **x86-64 实现**：先改 x86-64（参考旧 `X86_64ProcArch` 的常量），确保 `X86_64BootContext` 含 `segment_selectors` 但不出现在跨架构接口。
3. **aarch64/riscv64 实现**：它们的新 context 类型不含段选择子，删除旧 `SegmentSelectors` 导入。
4. **删除旧 trait**：移除 `ArchProcReset`、`ArchProcInit`、旧 `BootProcArch`、旧 `InitialRegState`、`InitialRegs`、`VmLoadResult`。
5. **KProcess 改字段**：删除四个旧字段，新增 `initial_context: CurrentBootContext`，改 setter 为 `set_boot_context`。
6. **改 `init_proc_and_boot`**：使用新的 `BootProcArch` API 和 `UserEntry`。
7. **改 `syscall_device.rs` IOPL**：调用 `CurrentBootProcArch::enable_user_io`。
8. **改 `ProcessTable`/`PrivTable`**：从 `Box<[T]>` 改为 `[T; N]`，添加 `new_empty()` const fn。
9. **改 `kpriv.rs`**：新增 `IpcBitmap`、`KernelCallMask`、`ProcessCapability`、`BootProfile`，替换 `configure_boot_priv`。
10. **文档重写**：按 §7 重写 Ch3/Ch4。

### 8.2 若 `const fn new_empty()` 受阻的 fallback

如果某些子结构（如 `Message`）的 const default 暂时不可行，可用 `MaybeUninit` 构造数组：

```rust
pub fn new() -> Self {
    let mut procs: [MaybeUninit<KProcess>; PROC_TABLE_SIZE] =
        [const { MaybeUninit::uninit() }; PROC_TABLE_SIZE];
    for i in 0..PROC_TABLE_SIZE {
        let nr = (i as ProcNr) - (NR_TASKS as ProcNr);
        let endpoint = Endpoint::from_generation_slot(0, nr);
        procs[i].write(KProcess::new(nr, endpoint));
    }
    // SAFETY: all elements initialized above.
    let procs = unsafe { core::mem::transmute::<_, [KProcess; PROC_TABLE_SIZE]>(procs) };
    // ... special IDLE handling ...
}
```

但首选方案仍是让 `KProcess::new_empty` const，因为它让代码更干净、类型系统能检查未初始化风险。

### 8.3 测试策略

- **单元测试**：
  - `X86_64ProcArch::base_context(true, _)` 返回 `status == INIT_TASK_PSW`。
  - `X86_64ProcArch::base_context(false, _)` 返回 `user_entry == None`。
  - `with_user_entry` 后 `user_entry` 被正确设置。
  - mock arch 的 `Context` 能 round-trip `UserEntry`。
- **契约测试**：每个 arch 的 `BootContext::apply_to_frame` 将 `UserEntry` 写入正确的 trap frame 字段（x86-64: rip/rsp/rbx；aarch64: elr_el1/sp_el0/x0；riscv64: sepc/sp/a0）。
- **集成测试**：`init_proc_and_boot` 在 mock 下能构造 `ProcessTable`，VM 进程获得 `Vm` profile，非 VM 用户进程获得 `NO_PRIV|NO_QUANTUM`。
- **L1 对偶测试**：对照 Minix3 C 的 boot image 表，验证每个 boot 进程的 `s_flags`、`s_ipc_to`、`s_k_call_mask` 与 C 一致。

---

## 9. 与其他模块的边界

- **07-cross-space-init.md**：`init_post_and_memory` 仍接收 `&ProcessTable`，从 VM 的 `initial_context` 中读取 PC/SP（如有需要）。无接口破坏。
- **08-system-init-boot-finish.md**：`bsp_finish_booting` 解除 `RTS_PROC_STOP` 等，逻辑不变。
- **11-scheduling-primitives.md / 10-switch-to-user.md**：调度器/上下文切换路径需要调用 `CurrentBootProcArch::apply_context(&proc.initial_context, &mut trap_frame)` 来初始化 trap frame。
- **20-syscall-device.md**：`SYS_IOPENABLE` 改为调用 `CurrentBootProcArch::enable_user_io`。

---

## 10. 待决策事项

1. **物理页分配接入 `load_vm_elf`**：当前设计保留 1:1 映射作为占位。真正的 bootstrap 页表需要 boot bump allocator 传入 `load_vm_elf`。应在哪个 commit 接入？建议作为 06 重构完成后的下一个 TODO(P0)。
2. **`TrapFrame` 的位置**：arch crate 需要定义 `TrapFrame`。若当前无此类型，需要新增 `os/arch/src/{x86_64,arm64,riscv64}/trap.rs`。
3. **`Message` const default**：确认是否可改；若不能，采用 `MaybeUninit` fallback。
4. **是否保留 `BootProfile::Service`**：当前 boot image 中只有 VM/RS 是 schedulable service，其余用户模块为 `User`。`Service` 保留给后续 RS 动态配置或驱动启动时使用。
5. **mock arch 的 `Context`**：mock `BootContext` 用最小字段实现（`status`、`user_entry`），确保测试不依赖真实硬件。

---

## 11. 验收标准

重构完成后，以下检查必须全部通过：

- [ ] `rg "SegmentSelectors" os/kernel/src os/arch/src/arch/` 返回 0 处（仅允许在 `os/arch/src/x86_64/` 出现）。
- [ ] `rg "Box<\[KProcess\]>" os/kernel/src/` 与 `rg "Box<\[KPriv\]>" os/kernel/src/` 返回 0 处。
- [ ] `rg "ArchProcReset|ArchProcInit" os/arch/src os/kernel/src/` 返回 0 处（被单 trait 替代）。
- [ ] `rg "fpu_needs_zero" os/` 返回 0 处。
- [ ] `rg "configure_boot_priv" os/kernel/src/` 返回 0 处。
- [ ] `rg "initial_pc|initial_sp|initial_ps_strings_reg|initial_status" os/kernel/src/proc.rs` 返回 0 处（替换为 `initial_context`）。
- [ ] `cargo test --features mock -p minix-arch` 通过。
- [ ] `cargo test --features mock -p minix-kernel` 通过。
- [ ] 文档 Ch3/Ch4 不再以「对应 C 函数」作为组织主线。
