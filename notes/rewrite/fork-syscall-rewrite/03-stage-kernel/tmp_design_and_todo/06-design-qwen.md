# 06-proc-init-boot-proc 重新设计方案

> **状态**：设计完成，待评审  
> **目的**：独立设计 Rust 实现方案，解决 06-problem.md 中列举的所有问题  
> **设计原则**：从 OS 概念出发，不从 C 函数翻译；硬件语义完全封装在 arch 层；boot 阶段零堆分配

---

## 1. 设计哲学：从 OS 概念出发，不从 C 函数出发

### 1.1 核心问题：OS 在 boot 阶段需要回答什么？

不是"C 代码做了什么"，而是"OS 在启动进程时必须知道什么"：

1. **进程是谁？** → 进程号、端点、类型（内核任务/系统服务/用户进程）
2. **进程能做什么？** → 能力（系统调用权限、IPC 权限、硬件资源访问权限）
3. **进程如何开始执行？** → 初始执行上下文（入口点、栈、CPU 状态）
4. **进程为什么现在不能运行？** → RTS 标志（多原因位图）

这四个问题对应四个 OS 概念，**不是**四个 C 函数。

### 1.2 当前设计的根本错误

当前设计是 **C 函数的 1:1 翻译**：

| C 函数 | Rust trait | 错误本质 |
|--------|-----------|---------|
| `arch_proc_reset()` | `ArchProcReset` | 翻译函数名，不是 OS 概念 |
| `arch_proc_init()` | `ArchProcInit` | 翻译函数名，不是 OS 概念 |
| `arch_boot_proc()` | `BootProcArch` | 翻译函数名，不是 OS 概念 |

**违反最高原则**：
- ❌ 原则 2（不允许 translate）：trait 名直接对应 C 函数名
- ❌ 原则 1（不允许硬件语义泄漏）：`InitialRegState` 包含 `segment_selectors`（x86-64 特有）
- ❌ 原则 3（boot 阶段零堆）：`ProcessTable` 和 `PrivTable` 使用 `Box<[T]>`

### 1.3 正确的设计方向

**一个 trait，一个概念**：`ProcessBootArch` —— "架构特定的进程启动初始化"

**一个关联类型**：`ExecutionContext` —— "进程的初始执行上下文"（架构自定义，OS 层不透明）

**零硬件语义泄漏**：OS 层只看到 `CurrentArch::ExecutionContext`，不知道里面有什么字段。

---

## 2. 核心设计：单一 trait + 关联类型

### 2.1 trait 定义

```rust
// os/arch/src/arch/proc_arch.rs

/// 架构特定的进程启动初始化。
///
/// OS 概念：每个进程首次执行时，CPU 必须有确定的初始状态。
/// 这个状态是架构特定的（x86-64 有段选择子，ARM 有 PSR，RISC-V 有 sstatus），
/// 但 OS 层不关心具体细节——它只需要存储这个状态，在调度时应用。
///
/// 设计原则：
/// - 单一 trait：不是三个 trait 的继承链，而是一个 trait 代表一个概念
/// - 关联类型：`ExecutionContext` 是架构自定义的不透明类型
/// - 纯函数式：返回纯值，不修改进程结构体
pub trait ProcessBootArch {
    /// 进程的初始执行上下文（架构自定义）。
    ///
    /// 这个类型包含进程首次执行时 CPU 需要的所有初始状态：
    /// - x86-64: RFLAGS、段选择子、FPU 状态
    /// - aarch64: SPSR_EL1
    /// - riscv64: sstatus
    ///
    /// OS 层不访问其内部字段，只整体存储、整体应用。
    type ExecutionContext: ExecutionContextOps;

    /// 为指定进程创建初始执行上下文。
    ///
    /// # 参数
    /// - `is_kernel`: 是否为内核任务（影响特权级和中断状态）
    /// - `proc_nr`: 进程号（某些架构可能需要，如 x86-64 的 FPU 状态索引）
    ///
    /// # 返回
    /// 架构特定的初始执行上下文，OS 层整体存储到 KProcess。
    fn create_execution_context(is_kernel: bool, proc_nr: ProcNr) -> Self::ExecutionContext;

    /// 为 VM 进程加载 ELF 并创建执行上下文。
    ///
    /// 这是"开天辟地"问题的解决点：VM 是第一个用户进程，
    /// 它的地址空间必须由内核用 bootstrap 页表建立。
    ///
    /// # 参数
    /// - `module`: VM 的 boot module（ELF 镜像）
    /// - `kernel_info`: 内核启动信息（包含内存映射、用户栈顶等）
    /// - `paging`: 页表操作接口（用于映射 VM 的代码和栈）
    ///
    /// # 返回
    /// VM 的执行上下文（包含 ELF 入口点、栈指针等）。
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Self::ExecutionContext;
}

/// 执行上下文必须支持的操作。
///
/// 这是一个"最小接口"——OS 层只需要能够：
/// 1. 将执行上下文应用到 trap frame（调度时）
/// 2. 提取入口点和栈指针（用于调试和统计）
///
/// 具体字段由架构自定义，OS 层不访问。
pub trait ExecutionContextOps: Clone + Copy + 'static {
    /// 将执行上下文应用到 trap frame。
    ///
    /// 调度器在进程首次执行时调用此方法，将初始 CPU 状态写入 trap frame。
    /// 这是架构层内部事务——OS 层不知道 trap frame 的具体布局。
    fn apply_to_trap_frame(&self, frame: &mut TrapFrame);

    /// 提取入口点（虚拟地址）。
    ///
    /// 用于调试和统计，不参与实际调度。
    fn entry_point(&self) -> VirBytes;

    /// 提取栈指针（虚拟地址）。
    ///
    /// 用于调试和统计，不参与实际调度。
    fn stack_pointer(&self) -> VirBytes;
}
```

### 2.2 为什么是单一 trait，不是三个 trait？

**当前设计的三个 trait**：

```rust
pub trait ArchProcReset { ... }      // 对应 arch_proc_reset()
pub trait ArchProcInit: ArchProcReset { ... }  // 对应 arch_proc_init()
pub trait BootProcArch: ArchProcInit { ... }   // 对应 arch_boot_proc()
```

**问题**：
1. ❌ 翻译 C 函数名，不是 OS 概念
2. ❌ 继承链表达的是"C 函数的调用关系"，不是"OS 概念的层次关系"
3. ❌ 三个 trait 返回三个不同的结构体（`InitialRegState`、`InitialRegs`、`VmLoadResult`），kernel 层需要分别处理

**正确设计**：

```rust
pub trait ProcessBootArch {
    type ExecutionContext: ExecutionContextOps;
    fn create_execution_context(...) -> Self::ExecutionContext;
    fn load_vm_elf<P: Paging>(...) -> Self::ExecutionContext;
}
```

**好处**：
1. ✅ 一个 trait 代表一个 OS 概念："架构特定的进程启动初始化"
2. ✅ 关联类型 `ExecutionContext` 是架构自定义的不透明类型
3. ✅ 两个方法代表两种场景："普通进程"和"VM 进程"（VM 需要加载 ELF）
4. ✅ kernel 层只处理一种类型：`CurrentArch::ExecutionContext`

### 2.3 为什么是关联类型，不是通用结构体？

**当前设计的通用结构体**：

```rust
pub struct InitialRegState {
    pub status: u64,
    pub segment_selectors: SegmentSelectors,  // ❌ x86-64 特有
    pub fpu_needs_zero: bool,                  // ❌ x86-64 特有
}
```

**问题**：
1. ❌ `segment_selectors` 在 aarch64/riscv64 永远全零——"非法状态可表达"
2. ❌ `fpu_needs_zero` 在 aarch64/riscv64 永远 false——"非法状态可表达"
3. ❌ 所有架构被迫 import `SegmentSelectors` 类型——硬件语义泄漏到 OS 层

**正确设计**：

```rust
// arch 层：每个架构定义自己的 ExecutionContext
pub struct X86_64ExecutionContext {
    pub rflags: u64,
    pub segment_selectors: SegmentSelectors,  // ✅ x86-64 内部类型
    pub fpu_state: FpuState,                   // ✅ x86-64 内部类型
}

pub struct AArch64ExecutionContext {
    pub spsr_el1: u64,  // ✅ 没有 segment_selectors
}

pub struct Riscv64ExecutionContext {
    pub sstatus: u64,   // ✅ 没有 segment_selectors
}

// OS 层：只看到关联类型别名
type CurrentExecutionContext = <CurrentArch as ProcessBootArch>::ExecutionContext;
```

**好处**：
1. ✅ aarch64 编译时，类型系统中根本不存在 `SegmentSelectors`
2. ✅ 每个架构只包含自己需要的字段——"非法状态不可表达"
3. ✅ OS 层不访问内部字段——硬件语义完全封装

---

## 3. 类型设计：架构特定的执行上下文

### 3.1 x86-64 执行上下文

```rust
// os/arch/src/x86_64/proc_arch.rs

/// x86-64 进程的初始执行上下文。
///
/// 包含进程首次执行时 CPU 需要的所有初始状态：
/// - RFLAGS（状态寄存器，包含 IOPL、IF 等）
/// - 段选择子（CS/DS/SS/ES/FS/GS）
/// - FPU 状态（是否需要清零）
///
/// 这些是 x86-64 架构的内部细节，OS 层不访问。
#[derive(Debug, Clone, Copy)]
pub struct X86_64ExecutionContext {
    /// RFLAGS 初始值。
    /// - 内核任务：IOPL=1, IF=1, bit1=1 (0x1202)
    /// - 用户进程：IOPL=0, IF=1, bit1=1 (0x0202)
    rflags: u64,

    /// 段选择子（Ring 3）。
    /// CS=0x1B, DS/SS/ES/FS/GS=0x23
    segment_selectors: SegmentSelectors,

    /// FPU 状态是否需要清零。
    /// 用户进程=true（首次执行前清零 FPU 保存区）
    /// 内核任务=false（内核任务共享内核 FPU 状态）
    fpu_needs_zero: bool,

    /// 程序计数器（入口点）。
    entry_pc: VirBytes,

    /// 栈指针。
    entry_sp: VirBytes,

    /// ps_strings 地址（放在 rbx 寄存器）。
    ps_strings_addr: VirBytes,
}

/// x86-64 段选择子（架构内部类型，不暴露给 OS 层）。
#[derive(Debug, Clone, Copy)]
pub struct SegmentSelectors {
    pub cs: u64,
    pub ds: u64,
    pub ss: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

impl ExecutionContextOps for X86_64ExecutionContext {
    fn apply_to_trap_frame(&self, frame: &mut TrapFrame) {
        // 将执行上下文写入 trap frame。
        // 这是 x86-64 架构层内部事务：
        // - 设置 RFLAGS
        // - 设置段选择子
        // - 如果需要，清零 FPU 保存区
        // - 设置 PC/SP/ps_strings 寄存器
        frame.rflags = self.rflags;
        frame.cs = self.segment_selectors.cs;
        frame.ds = self.segment_selectors.ds;
        // ... 其他段选择子
        frame.rip = self.entry_pc.0;
        frame.rsp = self.entry_sp.0;
        frame.rbx = self.ps_strings_addr.0;  // ps_strings 放在 rbx

        if self.fpu_needs_zero {
            // 清零 FPU 保存区（x86-64 特有）
            frame.zero_fpu_state();
        }
    }

    fn entry_point(&self) -> VirBytes {
        self.entry_pc
    }

    fn stack_pointer(&self) -> VirBytes {
        self.entry_sp
    }
}

impl ProcessBootArch for X86_64ProcArch {
    type ExecutionContext = X86_64ExecutionContext;

    fn create_execution_context(is_kernel: bool, proc_nr: ProcNr) -> Self::ExecutionContext {
        let rflags = if is_kernel {
            0x1202  // INIT_TASK_PSW: IOPL=1, IF=1, bit1=1
        } else {
            0x0202  // INIT_PSW: IOPL=0, IF=1, bit1=1
        };

        let segment_selectors = SegmentSelectors {
            cs: 0x1B,  // USER_CS_SELECTOR (Ring 3)
            ds: 0x23,  // USER_DS_SELECTOR
            ss: 0x23,
            es: 0x23,
            fs: 0x23,
            gs: 0x23,
        };

        let fpu_needs_zero = !is_kernel;

        X86_64ExecutionContext {
            rflags,
            segment_selectors,
            fpu_needs_zero,
            entry_pc: VirBytes(0),      // 内核任务：入口点在内核代码中
            entry_sp: VirBytes(0),      // 内核任务：栈指针在内核栈中
            ps_strings_addr: VirBytes(0), // 内核任务：无 ps_strings
        }
    }

    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Self::ExecutionContext {
        // 加载 VM ELF 到 bootstrap 页表。
        // 返回 VM 的执行上下文（包含入口点、栈指针等）。

        // ... ELF 解析和映射逻辑（与当前实现相同）...

        let entry_pc = VirBytes(entry);
        let entry_sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);
        let ps_strings_addr = VirBytes(entry_sp.0 - 32);

        X86_64ExecutionContext {
            rflags: 0x0202,  // 用户进程
            segment_selectors: SegmentSelectors { /* ... */ },
            fpu_needs_zero: true,
            entry_pc,
            entry_sp,
            ps_strings_addr,
        }
    }
}
```

### 3.2 aarch64 执行上下文

```rust
// os/arch/src/arm64/proc_arch.rs

/// AArch64 进程的初始执行上下文。
///
/// 包含进程首次执行时 CPU 需要的所有初始状态：
/// - SPSR_EL1（保存的程序状态寄存器）
///
/// 注意：AArch64 没有段选择子，没有 FPU 初始化需求（lazy init）。
#[derive(Debug, Clone, Copy)]
pub struct AArch64ExecutionContext {
    /// SPSR_EL1 初始值。
    /// - 内核任务：EL1h, IRQ/FIQ masked (0x3C5)
    /// - 用户进程：EL0t (0x0)
    spsr_el1: u64,

    /// 程序计数器（入口点）。
    entry_pc: VirBytes,

    /// 栈指针。
    entry_sp: VirBytes,

    /// ps_strings 地址（放在 r0 寄存器）。
    ps_strings_addr: VirBytes,
}

impl ExecutionContextOps for AArch64ExecutionContext {
    fn apply_to_trap_frame(&self, frame: &mut TrapFrame) {
        // AArch64 架构层内部事务：
        // - 设置 SPSR_EL1
        // - 设置 PC/SP/ps_strings 寄存器
        frame.spsr_el1 = self.spsr_el1;
        frame.pc = self.entry_pc.0;
        frame.sp = self.entry_sp.0;
        frame.r0 = self.ps_strings_addr.0;  // ps_strings 放在 r0
    }

    fn entry_point(&self) -> VirBytes {
        self.entry_pc
    }

    fn stack_pointer(&self) -> VirBytes {
        self.entry_sp
    }
}

impl ProcessBootArch for AArch64ProcArch {
    type ExecutionContext = AArch64ExecutionContext;

    fn create_execution_context(is_kernel: bool, proc_nr: ProcNr) -> Self::ExecutionContext {
        let spsr_el1 = if is_kernel {
            0x3C5  // EL1h, F/I/A/D masked
        } else {
            0x0    // EL0t
        };

        AArch64ExecutionContext {
            spsr_el1,
            entry_pc: VirBytes(0),
            entry_sp: VirBytes(0),
            ps_strings_addr: VirBytes(0),
        }
    }

    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Self::ExecutionContext {
        // ... ELF 解析和映射逻辑 ...

        AArch64ExecutionContext {
            spsr_el1: 0x0,  // 用户进程
            entry_pc: VirBytes(entry),
            entry_sp: VirBytes(stack_high.0 - VM_STACK_SIZE as u64),
            ps_strings_addr: VirBytes(entry_sp.0 - 32),
        }
    }
}
```

### 3.3 riscv64 执行上下文

```rust
// os/arch/src/riscv64/proc_arch.rs

/// RISC-V 64-bit 进程的初始执行上下文。
///
/// 包含进程首次执行时 CPU 需要的所有初始状态：
/// - sstatus（Supervisor Status Register）
///
/// 注意：RISC-V 没有段选择子，没有 FPU 初始化需求（lazy init）。
#[derive(Debug, Clone, Copy)]
pub struct Riscv64ExecutionContext {
    /// sstatus 初始值。
    /// - 内核任务：SPP=1 (S-mode), SPIE=0 (interrupts disabled)
    /// - 用户进程：SPP=0 (U-mode), SPIE=1 (interrupts enabled)
    sstatus: u64,

    /// 程序计数器（入口点，通过 sepc 设置）。
    entry_pc: VirBytes,

    /// 栈指针。
    entry_sp: VirBytes,

    /// ps_strings 地址（放在 a0 寄存器）。
    ps_strings_addr: VirBytes,
}

impl ExecutionContextOps for Riscv64ExecutionContext {
    fn apply_to_trap_frame(&self, frame: &mut TrapFrame) {
        // RISC-V 架构层内部事务：
        // - 设置 sstatus
        // - 设置 sepc (PC)
        // - 设置 sp
        // - 设置 a0 (ps_strings)
        frame.sstatus = self.sstatus;
        frame.sepc = self.entry_pc.0;
        frame.sp = self.entry_sp.0;
        frame.a0 = self.ps_strings_addr.0;  // ps_strings 放在 a0
    }

    fn entry_point(&self) -> VirBytes {
        self.entry_pc
    }

    fn stack_pointer(&self) -> VirBytes {
        self.entry_sp
    }
}

impl ProcessBootArch for Riscv64ProcArch {
    type ExecutionContext = Riscv64ExecutionContext;

    fn create_execution_context(is_kernel: bool, proc_nr: ProcNr) -> Self::ExecutionContext {
        let sstatus = if is_kernel {
            0x100  // SPP=1, SPIE=0
        } else {
            0x20   // SPP=0, SPIE=1
        };

        Riscv64ExecutionContext {
            sstatus,
            entry_pc: VirBytes(0),
            entry_sp: VirBytes(0),
            ps_strings_addr: VirBytes(0),
        }
    }

    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> Self::ExecutionContext {
        // ... ELF 解析和映射逻辑 ...

        Riscv64ExecutionContext {
            sstatus: 0x20,  // 用户进程
            entry_pc: VirBytes(entry),
            entry_sp: VirBytes(stack_high.0 - VM_STACK_SIZE as u64),
            ps_strings_addr: VirBytes(entry_sp.0 - 32),
        }
    }
}
```

---

## 4. kernel 层设计：整体存储，整体应用

### 4.1 KProcess 字段设计

```rust
// os/kernel/src/proc.rs

pub struct KProcess {
    // ... 其他字段 ...

    /// 进程的初始执行上下文（架构特定）。
    ///
    /// 进程首次执行时，调度器调用 `apply_to_trap_frame()` 将此上下文写入 trap frame。
    /// 这是架构层返回的不透明类型——kernel 层不访问其内部字段。
    ///
    /// 设计原则：
    /// - 整体存储：arch 层返回完整的 ExecutionContext，kernel 层整体接受
    /// - 整体应用：调度时整体应用到 trap frame，不拆解
    /// - 类型安全：`CurrentArch::ExecutionContext` 是编译时定型的类型
    pub execution_context: Option<CurrentArch::ExecutionContext>,
}
```

**为什么是 `Option<ExecutionContext>`？**

- 内核任务：`create_execution_context()` 返回的上下文，`entry_pc/entry_sp` 为零（内核任务的入口点在编译时确定）
- 用户进程：`create_execution_context()` 返回的上下文，`entry_pc/entry_sp` 为零；`load_vm_elf()` 返回的上下文包含真实入口点
- 进程回收时：重置为 `None`，表示"无有效执行上下文"

### 4.2 kernel 层调用逻辑

```rust
// os/kernel/src/lib.rs

pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();

    // ... boot image 循环 ...

    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr = /* 计算进程号 */;
        let proc = proc_table.get_mut(nr).unwrap();

        let is_kernel = nr < 0;
        let is_vm = nr == proc_nr::VM_PROC_NR;

        // 1. 创建执行上下文（架构特定）
        let exec_ctx = if is_vm {
            // VM 进程：加载 ELF 并创建执行上下文
            CurrentArch::load_vm_elf(module, kernel_info, &mut paging)
        } else {
            // 普通进程：创建默认执行上下文
            CurrentArch::create_execution_context(is_kernel, nr)
        };

        // 2. 整体存储到 KProcess
        proc.execution_context = Some(exec_ctx);

        // 3. 设置特权（能力模型，见下文）
        if schedulable {
            let priv_id = priv_table.assign_static(nr).unwrap();
            priv_table.configure_boot_priv(priv_id, capability);
        }

        // 4. 设置 RTS 标志
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    proc_table
}
```

**关键改进**：
1. ✅ kernel 层不拆解 `ExecutionContext`——整体存储
2. ✅ kernel 层不访问 `ExecutionContext` 内部字段——不透明类型
3. ✅ 调度时整体应用：`proc.execution_context.apply_to_trap_frame(frame)`

---

## 5. 能力模型：替代 6 个裸参数

### 5.1 当前设计的问题

```rust
pub fn configure_boot_priv(
    &mut self,
    priv_id: PrivId,
    flags: u16,           // ❌ 裸整数
    init_flags: i32,      // ❌ 裸整数
    trap_mask: u16,       // ❌ 裸整数
    ipc_to: u64,          // ❌ 裸整数
    k_call_mask: [u32; 2], // ❌ 裸整数数组
    sig_mgr: Endpoint,    // ❌ 裸端点
) { ... }
```

**问题**：
1. ❌ 7 个裸参数——"翻译 C 字段"，不是 OS 概念
2. ❌ 调用方需要知道每个参数的含义——"过程式"，不是"声明式"
3. ❌ 容易出错——参数顺序、类型、含义都需要记忆

### 5.2 正确设计：能力模型

```rust
// os/kernel/src/kpriv.rs

/// 进程的能力（capability）。
///
/// OS 概念：进程能做什么？
/// - 哪些系统调用？
/// - 哪些 IPC 目标？
/// - 哪些硬件资源？
///
/// 这是一个"声明式"模型——调用方描述"进程能做什么"，不是"如何设置特权字段"。
#[derive(Debug, Clone, Copy)]
pub struct ProcessCapability {
    /// 进程类型（决定基本权限）。
    pub proc_type: ProcessType,

    /// 允许的系统调用（位图）。
    pub syscalls: SyscallBitmap,

    /// 允许的 IPC 目标（位图）。
    pub ipc_targets: IpcBitmap,

    /// 允许的硬件资源（架构特定）。
    pub hw_resources: HwResources,
}

/// 进程类型（决定基本权限）。
#[derive(Debug, Clone, Copy)]
pub enum ProcessType {
    /// 内核任务（IDLE、CLOCK、SYSTEM 等）。
    /// - 无系统调用权限
    /// - 无 IPC 权限
    /// - 运行在 Ring 0 / EL1
    KernelTask { is_idle: bool },

    /// 系统服务（VM、RS、PM、VFS 等）。
    /// - 完整系统调用权限
    /// - 完整 IPC 权限
    /// - 运行在 Ring 3 / EL0
    SystemService { is_root: bool, is_vm: bool },

    /// 普通用户进程。
    /// - 运行时由 RS 配置权限
    /// - 运行在 Ring 3 / EL0
    UserProcess,
}

/// 系统调用位图。
#[derive(Debug, Clone, Copy)]
pub struct SyscallBitmap {
    bits: [u32; 2],  // 64 位，支持 64 个系统调用
}

impl SyscallBitmap {
    pub const NONE: Self = Self { bits: [0; 2] };
    pub const ALL: Self = Self { bits: [0xFFFF_FFFF; 2] };
}

/// IPC 目标位图。
#[derive(Debug, Clone, Copy)]
pub struct IpcBitmap {
    bits: u64,  // 64 位，支持 64 个目标
}

impl IpcBitmap {
    pub const NONE: Self = Self { bits: 0 };
    pub const ALL: Self = Self { bits: 0xFFFF_FFFF_FFFF_FFFF };
}

/// 硬件资源（架构特定）。
#[derive(Debug, Clone, Copy)]
pub struct HwResources {
    /// I/O 端口范围（x86-64 特有，其他架构为空）。
    pub io_ranges: [IoRange; NR_IO_RANGE],
    pub io_range_count: usize,

    /// IRQ 范围。
    pub irq_ranges: [i32; NR_IRQ],
    pub irq_range_count: usize,

    /// 内存范围。
    pub mem_ranges: [MemRange; NR_MEM_RANGE],
    pub mem_range_count: usize,
}

impl Default for HwResources {
    fn default() -> Self {
        Self {
            io_ranges: [IoRange::new(); NR_IO_RANGE],
            io_range_count: 0,
            irq_ranges: [0; NR_IRQ],
            irq_range_count: 0,
            mem_ranges: [MemRange::new(); NR_MEM_RANGE],
            mem_range_count: 0,
        }
    }
}
```

### 5.3 使用示例

```rust
// os/kernel/src/lib.rs

// 内核任务
let capability = ProcessCapability {
    proc_type: ProcessType::KernelTask { is_idle: (nr == proc_nr::IDLE) },
    syscalls: SyscallBitmap::NONE,
    ipc_targets: IpcBitmap::NONE,
    hw_resources: HwResources::default(),
};

// VM 进程
let capability = ProcessCapability {
    proc_type: ProcessType::SystemService { is_root: false, is_vm: true },
    syscalls: SyscallBitmap::ALL,
    ipc_targets: IpcBitmap::ALL,
    hw_resources: HwResources::default(),
};

// RS 进程
let capability = ProcessCapability {
    proc_type: ProcessType::SystemService { is_root: true, is_vm: false },
    syscalls: SyscallBitmap::ALL,
    ipc_targets: IpcBitmap::ALL,
    hw_resources: HwResources::default(),
};

// 应用能力
priv_table.configure_boot_priv(priv_id, capability);
```

**好处**：
1. ✅ 声明式——调用方描述"进程能做什么"
2. ✅ 类型安全——`ProcessType` 是枚举，不是裸整数
3. ✅ 易于扩展——新增权限只需添加字段
4. ✅ 易于测试——可以断言"VM 的能力是什么"

---

## 6. 零堆分配：boot 阶段的数据结构

### 6.1 当前设计的问题

```rust
pub struct ProcessTable {
    procs: Box<[KProcess]>,  // ❌ 使用堆
}

pub struct PrivTable {
    privs: Box<[KPriv]>,     // ❌ 使用堆
}
```

**问题**：
- ❌ boot 阶段没有堆（VM 还没启动）
- ❌ `Box<[T]>` 需要 `GlobalAlloc`，但 boot 阶段没有 allocator

### 6.2 正确设计：固定大小数组

```rust
// os/kernel/src/proc_table.rs

pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],  // ✅ 固定大小数组
    sched: Scheduler,
    vm_request_queue: crate::vm::VmRequestQueue,
}

impl ProcessTable {
    pub const fn new() -> Self {
        Self {
            procs: [const { KProcess::new_empty() }; PROC_TABLE_SIZE],
            sched: Scheduler::new(),
            vm_request_queue: crate::vm::VmRequestQueue::new(),
        }
    }
}

// os/kernel/src/kpriv.rs

pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],  // ✅ 固定大小数组
}

impl PrivTable {
    pub const fn new() -> Self {
        Self {
            privs: [const { KPriv::new_empty() }; NR_SYS_PROCS],
        }
    }
}
```

**好处**：
1. ✅ 零堆分配——编译时确定大小
2. ✅ 可以在 boot 阶段使用
3. ✅ 类型系统保证不会越界（通过 `get()` / `get_mut()` 方法）

---

## 7. 对比总结

### 7.1 trait 设计对比

| 方面 | 当前设计 | 新设计 |
|------|---------|--------|
| trait 数量 | 3 个（`ArchProcReset`、`ArchProcInit`、`BootProcArch`） | 1 个（`ProcessBootArch`） |
| trait 命名 | 对应 C 函数名 | 对应 OS 概念 |
| 返回类型 | 3 个通用结构体（`InitialRegState`、`InitialRegs`、`VmLoadResult`） | 1 个关联类型（`ExecutionContext`） |
| 硬件语义 | 泄漏到 OS 层（`segment_selectors`、`fpu_needs_zero`） | 完全封装在 arch 层 |
| kernel 层处理 | 拆解返回值，分别存储 | 整体存储，整体应用 |

### 7.2 类型设计对比

| 方面 | 当前设计 | 新设计 |
|------|---------|--------|
| `InitialRegState` | 通用结构体，包含 x86-64 特有字段 | 不存在——每个架构有自己的 `ExecutionContext` |
| `segment_selectors` | 在 aarch64/riscv64 永远全零 | 只存在于 `X86_64ExecutionContext` |
| `fpu_needs_zero` | 在 aarch64/riscv64 永远 false | 只存在于 `X86_64ExecutionContext` |
| OS 层可见性 | 访问内部字段（`status`、`segment_selectors`） | 不访问内部字段——不透明类型 |

### 7.3 特权设计对比

| 方面 | 当前设计 | 新设计 |
|------|---------|--------|
| 参数数量 | 7 个裸参数 | 1 个 `ProcessCapability` 结构体 |
| 参数类型 | 裸整数（`u16`、`i32`、`u64`） | 类型安全（`ProcessType`、`SyscallBitmap`、`IpcBitmap`） |
| 调用方式 | 过程式（"设置这个字段"） | 声明式（"进程能做什么"） |
| 易于理解 | 需要知道每个参数的含义 | 一目了然——"VM 是 SystemService，有所有权限" |

### 7.4 内存分配对比

| 方面 | 当前设计 | 新设计 |
|------|---------|--------|
| `ProcessTable` | `Box<[KProcess]>`（堆） | `[KProcess; PROC_TABLE_SIZE]`（栈/静态） |
| `PrivTable` | `Box<[KPriv]>`（堆） | `[KPriv; NR_SYS_PROCS]`（栈/静态） |
| boot 阶段可用 | ❌ 不可用（无堆） | ✅ 可用（零堆） |

---

## 8. 迁移路径

### 8.1 第一阶段：重构 trait 设计

1. 定义 `ProcessBootArch` trait 和 `ExecutionContextOps` trait
2. 为三个架构实现各自的 `ExecutionContext` 类型
3. 删除旧的三个 trait（`ArchProcReset`、`ArchProcInit`、`BootProcArch`）
4. 更新 kernel 层调用逻辑（整体存储 `ExecutionContext`）

### 8.2 第二阶段：重构特权模型

1. 定义 `ProcessCapability`、`ProcessType`、`SyscallBitmap`、`IpcBitmap`、`HwResources`
2. 更新 `PrivTable::configure_boot_priv()` 接受 `ProcessCapability`
3. 更新 `init_proc_and_boot()` 使用能力模型

### 8.3 第三阶段：消除堆分配

1. 将 `ProcessTable` 和 `PrivTable` 改为固定大小数组
2. 实现 `const fn new()` 构造函数
3. 更新所有使用 `Box<[T]>` 的代码

### 8.4 第四阶段：更新文档

1. 重写 §3（Rust 设计决策）——从 OS 概念出发
2. 重写 §4（实现详解）——反映新设计
3. 删除所有"C 函数对应"的表述——改为"OS 概念"

---

## 9. 关键设计决策

### 9.1 为什么是单一 trait，不是三个 trait？

**三个 trait 的问题**：
1. ❌ 翻译 C 函数名，不是 OS 概念
2. ❌ 继承链表达的是"C 函数的调用关系"
3. ❌ 返回三个不同的结构体，kernel 层需要分别处理

**单一 trait 的好处**：
1. ✅ 一个 trait 代表一个 OS 概念："架构特定的进程启动初始化"
2. ✅ 关联类型是架构自定义的不透明类型
3. ✅ 两个方法代表两种场景："普通进程"和"VM 进程"
4. ✅ kernel 层只处理一种类型

### 9.2 为什么是关联类型，不是通用结构体？

**通用结构体的问题**：
1. ❌ 包含架构特定字段（`segment_selectors`、`fpu_needs_zero`）
2. ❌ 其他架构被迫写"废话"（`SegmentSelectors::default()`）
3. ❌ 硬件语义泄漏到 OS 层

**关联类型的好处**：
1. ✅ 每个架构定义自己的类型
2. ✅ aarch64 编译时，类型系统中不存在 `SegmentSelectors`
3. ✅ OS 层不访问内部字段——硬件语义完全封装

### 9.3 为什么是能力模型，不是 7 个裸参数？

**7 个裸参数的问题**：
1. ❌ 翻译 C 字段，不是 OS 概念
2. ❌ 调用方需要知道每个参数的含义
3. ❌ 容易出错

**能力模型的好处**：
1. ✅ 声明式——"进程能做什么"
2. ✅ 类型安全——枚举和位图，不是裸整数
3. ✅ 易于理解和测试

### 9.4 为什么是固定大小数组，不是 `Box<[T]>`？

**`Box<[T]>` 的问题**：
1. ❌ boot 阶段没有堆
2. ❌ 需要 `GlobalAlloc`

**固定大小数组的好处**：
1. ✅ 零堆分配
2. ✅ 编译时确定大小
3. ✅ 可以在 boot 阶段使用

---

## 10. 待讨论的问题

### 10.1 `ExecutionContext` 是否需要 `apply_to_trap_frame()` 方法？

**选项 A**：需要（当前设计）
- ✅ 调度器可以直接调用
- ❌ `TrapFrame` 类型需要在 arch 层可见

**选项 B**：不需要，kernel 层提供 `apply_execution_context()` 方法
- ✅ arch 层不依赖 kernel 层类型
- ❌ kernel 层需要知道 `ExecutionContext` 的内部结构

**结论**：选项 A 更好——`TrapFrame` 是架构特定的类型，应该在 arch 层定义。

### 10.2 `ProcessCapability` 是否需要 `hw_resources` 字段？

**选项 A**：需要（当前设计）
- ✅ 统一模型——所有权限在一个结构体中
- ❌ x86-64 特有字段（`io_ranges`）在其他架构为空

**选项 B**：不需要，`hw_resources` 作为架构特定的扩展
- ✅ 其他架构不需要写"废话"
- ❌ 需要 trait 或泛型来抽象

**结论**：选项 A 更好——`HwResources::default()` 返回空数组，不是"废话"。

### 10.3 `KProcess::execution_context` 是否应该是 `Option`？

**选项 A**：是 `Option`（当前设计）
- ✅ 可以表示"无有效执行上下文"（进程回收时）
- ❌ 每次访问都需要 `unwrap()`

**选项 B**：不是 `Option`，使用"空"的 `ExecutionContext`
- ✅ 不需要 `unwrap()`
- ❌ 需要为每个架构实现 `Default`

**结论**：选项 A 更好——`Option` 明确表达"有/无"的语义。

---

## 11. 总结

本设计从 OS 概念出发，不从 C 函数翻译：

1. **单一 trait**：`ProcessBootArch` 代表"架构特定的进程启动初始化"
2. **关联类型**：`ExecutionContext` 是架构自定义的不透明类型
3. **零硬件语义泄漏**：OS 层不访问 `ExecutionContext` 内部字段
4. **能力模型**：`ProcessCapability` 替代 7 个裸参数
5. **零堆分配**：固定大小数组替代 `Box<[T]>`

这个设计满足所有 review-rules 的最高原则：
- ✅ 原则 1：不允许硬件语义泄漏到 OS 层
- ✅ 原则 2：不允许 translate，必须 rewrite
- ✅ 原则 3：boot 阶段零堆分配
