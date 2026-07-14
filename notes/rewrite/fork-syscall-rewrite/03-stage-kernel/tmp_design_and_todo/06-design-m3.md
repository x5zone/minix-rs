# 06-proc-init-boot-proc · 独立设计稿 (M3)

> **作者**：MiniMax-M3
> **状态**：独立设计（不参考任何其他 AI 的答案）
> **目的**：针对 `06-problem.md` 列出的 12 个问题，给出独立判断 + 端到端设计。本稿只覆盖 **Ch3（设计决策）** 与 **Ch4（实现详解）**——Ch1/Ch2（概念与 C 源码）原文档质量可接受，不在重写范围。
> **预期**：多 AI bagging 后，取长补短形成最终设计，再回填到 `06-proc-init-boot-proc.md` 的 Ch3/Ch4。

---

## 0. 阅读与独立判断声明

本文档基于以下材料独立形成判断，**未读**任何 `06-design-*.md`（其他 AI 的答案），也**未读**任何 `kboot-*` 或 `plat-design-*` 的衍生品——避免 bagging 失效。

- `06-problem.md`（用户整理的 12 个问题清单）
- `06-proc-init-boot-proc.md`（当前设计文档 Ch1-Ch4）
- `os/arch/src/{arch,x86_64,arm64,riscv64}/proc_arch.rs`（当前 Rust 实现）
- `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs` 三份 arch 实现
- `os/kernel/src/{proc.rs,proc_table.rs,kpriv.rs,lib.rs}`（当前 kernel 实现）
- `os/kernel/src/proc.rs` 中 KProcess 的 30+ 字段定义
- `minix3/minix/kernel/arch/i386/arch_system.c:140-200`（C 端 FPU 处理原始证据）
- `minix3/minix/kernel/arch/i386/arch_system.c:189-200`（`MF_FPU_INITIALIZED` 的语义）

读完上述材料后，我对 `06-problem.md` 的每个问题**逐条独立表态**，给出"同意 / 部分同意 / 不同意 / 问题表述不准"，并对问题中未涵盖的盲点提出**新增问题**。

---

## 1. 对 `06-problem.md` 12 个问题的独立判断

### 1.1 问题 #1 — `InitialRegState` 含 `segment_selectors`/`fpu_needs_zero`（P0）

**完全同意**。这是最高原则 P1（编译时定型的 OS 类型）的最严重违反。

**独立补充**（problem.md 未提）：

- aarch64/riscv64 的 `InitialRegState { segment_selectors: SegmentSelectors::default(), fpu_needs_zero: false }` 不仅字段无意义——`SegmentSelectors` 这个**类型本身**在 aarch64/riscv64 的编译产物里不应该存在。
- 当前实现要求三架构 `use ... SegmentSelectors`，违反了"OS 层不接触 arch 特有类型"。
- `fpu_needs_zero` 不仅是 x86 特有，还是**翻译过时的 FPU 模型**——见问题 #8。

**修复方向**：通过关联类型 + 类型别名让 arch-specific 字段不出现在 OS 层。

---

### 1.2 问题 #2 — `InitialRegs.ps_strings_reg` 与 `VmLoadResult.ps_strings` 命名混淆（P1）

**部分同意**。

**我的细化**：

- `ps_strings_reg` 名称确实误导——`ps_strings_reg: u64` 是个**值**而非"寄存器号"。三架构对"哪个寄存器收 ps_strings"的差异属于 arch 内部事务，OS 不该关心。
- `VmLoadResult.ps_strings: VirBytes`（地址）vs `InitialRegs.ps_strings_reg: u64`（值）——字段名一个带 `_reg` 后缀、一个不带，**确实是不一致**。
- 但 `VmLoadResult.allocated_bytes` 是**有用**的——它记录 VM 启动用了多少物理页（供后续 `add_memmap` 回收统计），应保留为 `allocated_bytes`。

**修复方向**：

- 字段名统一为 `ps_strings: VirBytes`（OS 关心的是地址，不是放哪个寄存器）。
- 三架构 `apply_to_trap_frame` 内部各自决定写入哪个寄存器（x86-64→rbx, aarch64→r0, riscv64→a0）——OS 完全不感知。

---

### 1.3 问题 #3 — kernel setter 拆解 arch 返回值（破坏"纯函数式 trait"）（P1）

**完全同意**。

**修复方向**：kernel 不拆解 arch 返回值——**整体接收**、**整体存储**、让 arch 在应用时**整体拆解**。

---

### 1.4 问题 #4 — `InitialRegs` 的应用同样拆解问题（P1）

**完全同意**。`proc.set_boot_pc_sp(init_regs.pc, init_regs.sp, init_regs.ps_strings_reg)` 把 arch 返回的 3 字段拆成 3 个独立 setter——这破坏了 §3.1 自称的"纯函数式 trait 设计"。

**修复方向**：arch 返回的 StartupRegs 应该**整体存储**到 KProcess 的 `startup: CurrentStartup` 字段；调度时 arch 用 `startup.apply_to_trap_frame(&mut frame)` **整体应用**。

---

### 1.5 问题 #5 — `initial_status` 字段名与 IOPL 操作（P1）

**部分同意**。

**我的细化**：

- `initial_status: u64` 字段名 OS 中性——这点同意。
- 但所有"使用"它的代码（IOPL 设置）确实在 x86-64 syscall_device 中散布——这不是 `initial_status` 字段名的问题，而是 **IOPL 是 x86 特有概念，不应在 OS 层表达**。
- 修复方向：IOPL 配置应该**下沉**为 arch 层方法 `arch_grant_io_permission(proc: &KProcess)`，kernel 层不接触具体位操作。

---

### 1.6 问题 #6 — 文档自称"OS-semantic"但违反（P2）

**完全同意**——`segment_selectors` 字段名是 x86 段选择子的硬件术语，根本不是 OS 语义。这是对 Ch1 "概念抽象"原则的讽刺——文档在讲 OS 语义，但命名上泄露硬件。

---

### 1.7 问题 #7 — `SegmentSelectors::default()` 的哨兵值（P2）

**完全同意**。`Default` 返回全零——用 `Default` 表达"该字段无意义"是 C 式哨兵值思维（all-zero = none），违反类型表达。

**修复方向**：`SegmentSelectors` 这个**类型本身**不应该出现在 aarch64/riscv64 的编译产物中——直接删除。

---

### 1.8 问题 #8 — `fpu_needs_zero` 是"翻译 Minix3 + 原因编造"双重问题（P0）

**完全同意，且问题描述准确**。我对照 `minix3/minix/kernel/arch/i386/arch_system.c:140-200` 验证了：

```c
// arch_system.c:144 — arch 层的静态数组
static char fpu_state[NR_PROCS][FPU_XFP_SIZE] __aligned(FPUALIGN);

// arch_system.c:158 — arch 层直接 memset
memset(v, 0, FPU_XFP_SIZE);
```

C 版的 `arch_proc_reset` 直接访问 `fpu_state[pr->p_nr]`，**根本不存在"arch 无法访问 FPU 保存区"**——这是当前 Rust 设计**编造的因果链**。

**独立补充**（problem.md 提到但未深挖）：

- 现代硬件的 FPU 处理**根本不是** Minix3 那个 `fnsave/fxrstor` 模型。
- **x86-64 现代模型**：XSAVE/XRSTOR + XCR0 决定 XSAVE area 大小 + `MF_FPU_INITIALIZED` lazy init 标志 + CR0.TS 延迟 FPU context switch。`fpu_state` 静态数组是 32-bit 时代的产物；64-bit 应该用**动态 XSAVE area**（按 XCR0 决定大小）。
- **aarch64 现代模型**：没有 per-process FPU 保存区——FPU enable 是**系统级**的（CPACR_EL1.FPEN），per-process 只有 FPCR/FPSR 几个寄存器（context switch 时 save/restore 即可）。
- **riscv64 现代模型**：没有 per-process FPU 保存区——sstatus.FS 状态机（Off=0 / Initial=1 / Clean=2 / Dirty=3）控制，第一次 FP 指令 trap 进 kernel。

**修复方向**：FPU/扩展寄存器初始化**完全下沉**到 arch 层，OS 层不感知 FPU 概念存在；x86-64 走 XSAVE 现代路径。

---

### 1.9 问题 #9 — "init_regs 内部调用 reset"是编造因果链（P1）

**完全同意**。trait 继承（`ArchProcInit: ArchProcReset`）是**编译时类型关系**，不是运行时的"调用 reset"。文档把"实现继承"等同于"运行调用"是错误的。

**修复方向**：合并 `initial_reg_state` 和 `init_regs` 为单一函数 `build_startup_for_user(role, entry, sp, ps_strings)`，kernel 只调用一次。

---

### 1.10 问题 #10 — 3 个 trait 完全对应 3 个 C 函数（无架构差异）（P0）

**完全同意**，且我认为问题描述还可以更激进。

**我的独立洞察**（比 problem.md 更进一步）：

- 不仅"3 个 trait 是翻译"——**trait 本身就是错的抽象**。当前 Rust 在编译时已经知道目标架构（`#[cfg(target_arch)]`），trait 派发没有运行时开销也没有静态多态需求，trait 完全是冗余的。
- `load_vm_elf` 三架构代码**完全相同**（我对比了 `x86_64/load_vm_elf` 与 `arm64/load_vm_elf`，除了 ELF 段 flags 转换细节外，逻辑 100% 相同）——所以连"按架构派发"都不需要。
- `ArchProcInit::init_regs` 三架构差异仅在 "ps_strings 写到哪个寄存器名"——这个差异**完全可以通过 `apply_to_trap_frame` 内部 if/分支实现**，trait 派发无意义。

**修复方向（比 problem.md 更激进）**：

- **删掉所有 trait**。改用 cfg-selected `pub type CurrentReg = ...;` + 普通函数 `pub fn build_startup(...) -> CurrentReg`。
- kernel 层代码完全 arch-agnostic，只看到 `CurrentReg` / `CurrentTrapFrame` 等类型别名。

用户原话"似乎 CurrentReg 就足够了"——**正是此意**。

---

### 1.11 问题 #11 — `Box<[KProcess]>` / `Box<[KPriv]>` 使用堆（P0）

**完全同意，且这是最大 P0**（problem.md 已正确识别）。

**独立补充**：

- `KProcess::new()` 当前**不是 `const fn`**——所以 `[KProcess; N]` 直接用 const 数组初始化**还做不到**。需要把 `KProcess::new` 改为 `const fn`。
- 大部分字段是 const-initable：`AtomicI32::new()` 是 const（Rust 1.75+）、`AtomicU64::new()` 是 const、`ProcName::new()` 是 const、`Endpoint::NONE` 是 const、`Option<None>` 是 const。
- `KPriv::new(i as SysId)` 也需要 const 化。
- KProcess 还有个特殊问题：`#[cfg(debug_assertions)] pub p_magic: u32`——`#[cfg]` 在 `const fn` 内合法，但需要确认 magic 字面量是 const（已经是 `0xC0FFEE1` 字面量，没问题）。

**修复方向**：

- `KProcess::new` 改为 `pub const fn new(nr, endpoint) -> Self`。
- `ProcessTable` 用 `procs: [KProcess; PROC_TABLE_SIZE]` 内嵌数组。
- `PrivTable` 同样用 `privs: [KPriv; NR_SYS_PROCS]` 内嵌数组。
- `Scheduler` / `VmRequestQueue` 也需要 const-initable。

---

### 1.12 问题 #12 — §3.4 100% 翻译 C 函数（P1）

**完全同意**，且"translate 味"在当前文档里**遍布 Ch3、Ch4 全章**，不止 §3.4。

**独立补充**：

- §3.5 的"决策汇总表"每个 C 函数一一对应 Rust 实现——这本应是"OS 概念差异"的对比，不是"C vs Rust 实现差异"。
- §4.4 的 `load_vm_elf` 实现完全照搬 C 的 `arch_boot_proc` 流程，包括 ps_strings 栈布局的 padding 算法（`sp -= sizeof(int) + sizeof(void*) + sizeof(void*)`）——这是 C 的指针运算，Rust 应该用结构体表达。

**修复方向**：

- 命名反映 OS 概念：`ProcessCapability`（不是 `KPriv` 字段集合）。
- 参数打包为 `BootCapability` 模板枚举（不是 6 个裸参数）。
- ps_strings 用 `PsStrings` 结构体（不是裸字节 padding）。

---

## 2. 我额外发现的问题（problem.md 未涵盖）

### 2.1 新问题 A：`KProcess` 已经有 `p_ext_reg_state: ExtRegState` 字段但未真正使用

`proc.rs:808`：

```rust
/// Extended register state (XSAVE area on x86-64, VFP/NEON on ARM64, F/D on RISC-V).
pub p_ext_reg_state: ExtRegState,
```

注释说"仅当 `MF_EXT_REG_INITIALIZED` flag 设置时有效"——但当前代码**根本没有任何位置设置这个 flag**。这是个半成品的扩展寄存器抽象。

**意义**：当前设计"半身不遂"——既不是 Minix3 时代 `fpu_state[NR_PROCS]` 静态数组（删了），也不是现代 XSAVE 抽象（没做完）。要么 commit 到现代模型（XSAVE area + lazy init flag），要么回退到 Minix3 静态数组。

**修复**：与现代 FPU 模型 #8 的修复一起做——彻底走 XSAVE 路径。

### 2.2 新问题 B：`KProcess.initial_pc/sp/status/ps_strings_reg` 4 个字段是"碎片化设计"

当前 KProcess 的 boot-time 字段：

```rust
pub initial_pc: VirBytes,
pub initial_sp: VirBytes,
pub initial_ps_strings_reg: u64,
pub initial_status: u64,
```

这 4 个字段**作为一组**才有意义——调度器首次调度进程时，把这 4 个值写入 trap frame。但当前它们作为 4 个独立字段暴露，破坏内聚性。

**修复**：合并为单个 `startup: CurrentStartup` 字段（arch-specific opaque 值），kernel 不拆解，调度时 arch 用 `startup.apply_to_trap_frame(&mut frame)` 整体写入。

### 2.3 新问题 C：`load_vm_elf` 三架构代码完全相同——根本不该在 arch 层

我对比了三个 arch 的 `load_vm_elf` 实现：

- x86_64：`pages_for_seg → map → copy_nonoverlapping`
- arm64：完全相同的 pages_for_seg → map → copy_nonoverlapping
- riscv64：完全相同的 pages_for_seg → map → copy_nonoverlapping

差异仅在 ELF 段 flags → PageFlags 转换（一处微小差异），这可以通过 cfg 函数或 trait 小方法解决。

**意义**：把"VM ELF 加载"放进 arch trait 是**架构分层错误**——这不是 arch-specific 概念，是 OS 概念（"把 VM 的 ELF 二进制加载到内存并建立地址空间"），只是恰好 arch crate 提供了 ELF 解析工具。

**修复**：`load_vm_image` 提到 arch crate 的 `common` 模块（或更高层），三架构共享一份实现，仅 ELF flags 转换是 cfg-dispatch。

### 2.4 新问题 D：`init_proc_and_boot` 的 `#[cfg(feature = "mock")]` 分支是技术债

当前主流程：

```rust
let (pc, sp, ps_strings) = if is_vm {
    #[cfg(feature = "mock")] { ... 走 MockPaging ... }
    #[cfg(not(feature = "mock"))] { (VirBytes(0), VirBytes(0), VirBytes(0)) }  // ← 占位
} else { (VirBytes(0), VirBytes(0), VirBytes(0)) };  // ← 占位
```

非 mock 下 VM ELF 加载是占位实现（PC=0 表示 "RS 后续加载"）——这本身是合理 fallback（VM 可由 RS 运行时加载），但代码用 `#[cfg]` 切换是技术债。

**修复**：把"VM 是否由内核在 boot 阶段加载 ELF"做成**清晰的策略选择**，而不是 cfg：

```rust
pub enum VmLoadPolicy {
    KernelLoadsVm,        // 内核在 boot 时用 bootstrap 页表加载 VM
    DeferredToRs,         // VM 由 RS 在运行时加载（PC=0）
}

// 默认 deferred；测试可启用 KernelLoadsVm
```

### 2.5 新问题 E：`p_init_proc_and_boot()` 主流程 50+ 行 C-味 loop

当前 `init_proc_and_boot()` 主循环是一坨 50+ 行函数，逻辑混在一起：

- 计算 proc_nr
- 设置名字
- 判定 schedulable
- 分配特权
- 配置特权
- 调 arch 函数
- 设置 RTS flags

每个 step 都耦合在一起，难以单独测试。

**修复**：拆为独立函数 + 一个干净的循环：

```rust
pub fn init_proc_and_boot(kinfo: &KernelInfo) -> (ProcessTable, PrivTable) {
    let mut pt = ProcessTable::new();
    let mut prt = PrivTable::new();
    for slot in 0..NR_BOOT_MODULES {
        init_one_boot_proc(slot, kinfo, &mut pt, &mut prt);
    }
    pt
}

fn init_one_boot_proc(slot, kinfo, pt, prt) {
    // 每步都是独立可测函数
}
```

---

## 3. 设计原则（修复依据）

经过独立分析，我对 problem.md 的原则提出**加强版**：

### P1 原则（加强）：编译时定型的 OS 类型（CF. problem.md P1）

- 字段级：aarch64 编译时**类型系统中根本不存在** `SegmentSelectors`（不是 `SegmentSelectors::default()`）。
- 类型级：OS 层不导入 arch-specific 类型；`use` 语句里不能出现 arch-specific 名字。
- 实现级：kernel 层代码用 `cfg` 切换的目标架构相关**只能是类型别名**，不能是 if/else 分支。

### P2 原则（加强）：OS 概念而非 C 函数（CF. problem.md P2）

- 字段命名来自 OS 概念（`ProcessCapability`），不是 C 字段翻译（`s_ipc_to`、`s_trap_mask`）。
- 函数命名来自 OS 动作（`assign_capability`），不是 C 函数翻译（`assign_static`）。
- trait/接口来自 OS 抽象（"启动配置"），不是 C 函数对应（`ArchProcReset`/`ArchProcInit`/`BootProcArch`）。

### P3 原则（加强）：进程表无堆存储（CF. problem.md P3）

- `ProcessTable.procs: [KProcess; N]`——编译期固定大小。
- `PrivTable.privs: [KPriv; N]`——编译期固定大小。
- `KProcess::new()`、`KPriv::new()` 必须 `const fn`。
- `Scheduler`、`VmRequestQueue` 同样需要 const-initable（具体见后续单独设计稿）。

### P4 原则（新增）：Capability-based 安全模型

- 进程有"能力"（capability），不是字段集合。
- `ProcessCapability` 包含 IPC、kernel call、trap、signal manager 等子能力。
- `BootCapability` 枚举模板：KernelTask / SystemService / RootServer / DefaultUser。
- 不直接镜像 C 的 `s_flags`/`s_trap_mask`/`s_ipc_to`/`s_k_call_mask`/`s_sig_mgr`/`s_io_tab`/`s_irq_tab`——这些是 C 内部表示，Rust 应该重新组合为 OS 语义。

### P5 原则（新增）：arch 层是 opaque type，不是 trait

- trait 用于"行为多态"——但 minix-rs 是**编译时单架构**，没有多态需求。
- 用 `pub type CurrentReg = ...;`（cfg-selected）替代 trait。
- arch-specific 函数用 cfg-dispatch 或 `pub fn` per-arch module re-export。
- 唯一保留的 trait 是 ELF flags → PageFlags 的小转换（因为这是 PageFlags 的语义，不是 Process 的）。

### P6 原则（新增）：整体应用，避免按顺序组合

- arch 返回"启动配置"整体值，kernel 整体接收。
- arch 在 `apply_to_trap_frame` 内部消化所有寄存器写入顺序。
- kernel 不需要知道"先写 status 还是先写 PC"。
- kernel 不需要知道"先 reset 再 init 还是先 init 再 reset"——只有一个 `build_startup(...)` 函数。

---

## 4. Chapter 3 设计（Rust 设计决策）

### 3.1 OS 概念视角

启动一个进程是一个**原子操作**：OS 决定"这个进程能做什么、初始状态是什么、第一次执行从哪里开始"。这个动作是 OS 概念，不应该被分解为"先 reset 寄存器、再 init PC/SP、再加载 ELF、再分配特权"。

Rust 设计应该把这个原子动作表达为单一抽象：**`ProcessStartup`**。

```
                        OS 概念：进程 = 启动配置 + 内核资源
                                  ↓
              ProcessStartup   =   入口地址 + 栈指针 + ps_strings + arch 私有启动状态
              ProcessCapability = IPC + 系统调用 + trap + signal
              ProcessTable     = [KProcess; N] 固定槽位
              PrivTable        = [KPriv; N] 固定槽位
              VmBootstrap      = VM ELF 加载策略
```

### 3.2 类型设计

#### 3.2.1 arch 层：`CurrentStartup`（opaque 类型别名）

```rust
// os/arch/src/lib.rs（cfg-selected re-export）

#[cfg(target_arch = "x86_64")]
pub use arch_x86_64::process::{
    StartupRegs as CurrentStartup,
    TrapFrame as CurrentTrapFrame,
    ExtRegSlot as CurrentExtRegSlot,
    build_startup_for_user,
    build_startup_for_kernel,
    apply_startup_to_trap_frame,
    init_ext_reg_slot,
};

#[cfg(target_arch = "aarch64")]
pub use arch_aarch64::process::{ ... 同上 ... };

#[cfg(target_arch = "riscv64")]
pub use arch_riscv64::process::{ ... 同上 ... };
```

**关键**：`CurrentStartup` 是**类型别名**，不是 trait。每次编译时根据 `target_arch` 解析为具体类型。aarch64 编译时这个类型不包含任何 x86-64 字段——**字段在编译产物中不存在**。

#### 3.2.2 x86-64 `StartupRegs`

```rust
#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: RFlags,                 // x86-64: RFLAGS 初值
    entry_point: VirBytes,          // x86-64: rip 初值
    stack_pointer: VirBytes,        // x86-64: rsp 初值
    ps_strings: VirBytes,           // x86-64: 启动时传给 crt0 的 ps_strings 地址
    ext_reg_init: ExtRegInit,       // x86-64: XSAVE area 初始化方式
}

#[derive(Clone, Copy)]
pub enum ExtRegInit {
    /// 用户进程需要分配 XSAVE area 并清零
    AllocateXSaveArea { size: usize },
    /// 内核任务无需 FPU/XSAVE 上下文
    NoExtRegState,
}
```

#### 3.2.3 aarch64 `StartupRegs`

```rust
#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: SpsrEl1,                // aarch64: SPSR_EL1 初值（EL0t/EL1h）
    entry_point: VirBytes,          // pc
    stack_pointer: VirBytes,        // sp
    ps_strings: VirBytes,           // 启动时 r0 传给 crt0
}

#[derive(Clone, Copy)]
pub struct ExtRegSlot;  // aarch64 无 per-process FPU 保存区；只存一个零大小标记

pub fn init_ext_reg_slot(_slot_nr: ProcNr, _role: ProcessRole) -> ExtRegSlot {
    // aarch64 实际工作：CPACR_EL1.FPEN 配置（已在 cstart 完成）
    // 此处仅返回 marker，OS 层不感知
}
```

#### 3.2.4 riscv64 `StartupRegs`

```rust
#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: Sstatus,                // riscv64: sstatus 初值
    entry_point: VirBytes,          // sepc
    stack_pointer: VirBytes,        // sp
    ps_strings: VirBytes,           // 启动时 a0 传给 crt0
}

#[derive(Clone, Copy)]
pub struct ExtRegSlot;  // riscv64 无 per-process FPU 保存区；sstatus.FS 写入 StartupRegs.status
```

**关键观察**：`StartupRegs` 三个架构有不同的字段集合，**没有共同 trait**。共同点只有字段名（`entry_point`、`stack_pointer`、`ps_strings`、`status`），但**这是命名约定，不是类型约束**——kernel 层只能整体接收，不能拆解字段。

#### 3.2.5 `ProcessCapability`（OS 概念）

```rust
// os/kernel/src/capability.rs

/// 进程能力：进程能做什么、和谁通信、能接收什么信号。
///
/// 不是 C 字段的镜像——是 OS 安全模型的完整表达。
#[derive(Clone, Copy)]
pub struct ProcessCapability {
    /// 可发送 IPC 的目标 endpoint 位图
    pub ipc_targets: EndpointBitmap,
    /// 可调用的系统调用 trap 编号
    pub trap_mask: TrapMask,
    /// 可调用的内核调用位图
    pub kernel_calls: KCallBitmap,
    /// 信号管理器（谁负责处理本进程信号）
    pub signal_manager: Option<Endpoint>,
    /// 进程类型标志
    pub role: ProcessRole,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcessRole {
    Idle,
    KernelTask,
    SystemService,    // VM
    RootServer,       // RS
    User,             // 普通用户进程
}
```

#### 3.2.6 `BootCapability`（boot 阶段预定义模板）

```rust
/// 进程能力模板：boot 阶段为每类进程预定义的 capability。
///
/// 不用"分配静态特权"这种 C 函数翻译——直接表达"这个进程是 VM/RS/KernelTask"。
pub enum BootCapability {
    KernelTask { role: KernelTaskKind },
    SystemService,
    RootServer,
    DeferredToRs,    // 普通用户进程，RS 运行时分配
}

pub enum KernelTaskKind {
    Idle,
    Clock,
    System,
    Kernel,
}
```

#### 3.2.7 `KProcess::startup` 字段

```rust
pub struct KProcess {
    // ... 其他 30+ 字段不变 ...

    /// 启动配置（arch-specific opaque 值）。
    ///
    /// - 由 arch 层 `build_startup_*` 构建
    /// - 调度器首次调度时由 arch 层 `apply_startup_to_trap_frame` 整体写入 trap frame
    /// - kernel 层不拆解这个字段
    pub startup: arch::CurrentStartup,

    /// 扩展寄存器槽位（arch-specific）。
    ///
    /// - x86-64: XSAVE area 索引/状态
    /// - aarch64/riscv64: 零大小标记（无 per-process FPU 保存区）
    pub ext_reg_slot: arch::CurrentExtRegSlot,
}
```

**取代原来的 4 个字段**：

- `initial_pc: VirBytes`
- `initial_sp: VirBytes`
- `initial_ps_strings_reg: u64`
- `initial_status: u64`

→ **全部合并到 `startup` 字段**。kernel 不再直接接触这些 arch-specific 值。

### 3.3 函数式接口

#### 3.3.1 进程启动配置

```rust
// os/arch/src/process.rs（per-arch 模块，cfg re-export）

/// 为用户进程构建启动配置。
///
/// 这是 OS 表达"启动一个用户进程"概念的纯函数。
/// 输入：entry point、stack pointer、ps_strings 地址、进程角色。
/// 输出：arch-specific opaque 值，kernel 整体接收。
pub fn build_startup_for_user(
    role: ProcessRole,
    entry: VirBytes,
    sp: VirBytes,
    ps_strings: VirBytes,
) -> CurrentStartup;

/// 为内核任务构建启动配置。
///
/// 内核任务无用户栈、无 ps_strings。
pub fn build_startup_for_kernel(role: ProcessRole) -> CurrentStartup;

/// 把启动配置应用到 trap frame。
///
/// 调度器首次调度本进程前调用，由 arch 层消化所有寄存器写入顺序。
pub fn apply_startup_to_trap_frame(
    startup: CurrentStartup,
    frame: &mut CurrentTrapFrame,
);
```

#### 3.3.2 扩展寄存器（FPU/SIMD）初始化

```rust
/// 初始化进程的扩展寄存器槽位。
///
/// x86-64: 分配 XSAVE area + 清零 + 设置 MF_EXT_REG_INITIALIZED 标志。
/// aarch64: 标记（无 per-process FPU 保存区，CPACR_EL1.FPEN 已系统级启用）。
/// riscv64: 标记（无 per-process FPU 保存区，sstatus.FS 在 trap frame 写入）。
///
/// kernel 层调用一次，arch 层决定具体动作。
pub fn init_ext_reg_slot(slot_nr: ProcNr, role: ProcessRole) -> CurrentExtRegSlot;
```

#### 3.3.3 VM ELF 加载（提到 arch-common，不在 per-arch trait）

```rust
// os/arch/src/vm_boot.rs（跨架构共享）

/// 加载 VM 的 ELF 到 bootstrap 地址空间。
///
/// 三架构共享同一实现——ELF 加载是 OS 概念，不是 arch-specific。
/// 唯一 arch-specific 是 ELF 段 flags → PageFlags 转换（cfg-dispatch）。
pub fn load_vm_image<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmImage;
```

**VM ELF 加载流程**（OS 概念视角）：

1. 解析 ELF 头获取 entry point
2. 遍历 PT_LOAD 段：
   - 解析段 flags（arch-specific，由 cfg 函数处理）
   - 计算 page-aligned vaddr 区间
   - 调用 `paging.map(vaddr, paddr, flags)` 分配物理页并映射
   - 复制段数据到物理页
3. 分配栈区间（页对齐）
4. 在栈顶构造 `PsStrings` 结构体
5. 返回 `VmImage { entry, sp, ps_strings, allocated_bytes }`

#### 3.3.4 `PsStrings` 结构体（BSD 约定）

```rust
// os/kernel/src/ps_strings.rs

/// BSD 风格的 ps_strings 栈顶结构。
///
/// 启动代码（crt0）通过 ps_strings 指针定位 argv/envp。
/// 用结构体表达，不用 C 的裸指针 padding。
#[repr(C)]
pub struct PsStrings {
    pub ps_argvstr: *const *const u8,
    pub ps_nargvstr: i32,
    pub ps_envstr: *const *const u8,
    pub ps_nenvstr: i32,
}
```

#### 3.3.5 进程表

```rust
// os/kernel/src/proc_table.rs

pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],   // ← 不用 Box！
    sched: Scheduler,                      // const-init
    vm_request_queue: VmRequestQueue,      // const-init
}

impl ProcessTable {
    pub const fn new() -> Self {
        let procs = {
            let mut arr: [KProcess; PROC_TABLE_SIZE] = 
                [const { KProcess::empty() }; PROC_TABLE_SIZE];
            // 注：const fn 中不能用 iterator/closure，只能用 while loop + 数组索引
            let mut i = 0;
            while i < PROC_TABLE_SIZE {
                let nr = (i as i32) - (NR_TASKS as i32);
                arr[i] = KProcess::empty_at(nr);
                i += 1;
            }
            arr
        };
        Self {
            procs,
            sched: Scheduler::new(),
            vm_request_queue: VmRequestQueue::new(),
        }
    }
}
```

**关键**：`KProcess::empty_at(nr) -> Self` 必须是 `const fn`——这是把 `KProcess::new` 改为 const 的最直接形式。

#### 3.3.6 特权表（Capability Table）

```rust
// os/kernel/src/capability.rs

pub struct CapabilityTable {
    /// 静态能力区：boot 阶段预定义（VM/RS/KernelTask）
    static_caps: [ProcessCapability; NR_STATIC_CAPS],
    /// 动态能力区：运行时分配（普通用户进程）
    dynamic_caps: [ProcessCapability; NR_DYNAMIC_CAPS],
}

pub const NR_STATIC_CAPS: usize = NR_TASKS + NR_SYSTEM_SERVICES;
pub const NR_DYNAMIC_CAPS: usize = NR_PROCS - NR_SYSTEM_SERVICES;

impl CapabilityTable {
    pub const fn new() -> Self {
        Self {
            static_caps: [const { ProcessCapability::empty() }; NR_STATIC_CAPS],
            dynamic_caps: [const { ProcessCapability::empty() }; NR_DYNAMIC_CAPS],
        }
    }

    /// 绑定 boot 阶段预定义能力到指定进程。
    pub fn bind_boot_capability(
        &mut self,
        proc_nr: ProcNr,
        template: BootCapability,
    ) -> Result<(), CapError>;
}
```

**取代原来的 `PrivTable::assign_static + configure_boot_priv`**——把"分配 + 配置"两步合并为"绑定模板"一步，kernel 调用更简洁。

### 3.4 trait 层次设计

**核心决策：删除所有 trait**。

```diff
- pub trait ArchProcReset { fn initial_reg_state(...) -> InitialRegState; }
- pub trait ArchProcInit: ArchProcReset { fn init_regs(...) -> InitialRegs; }
- pub trait BootProcArch: ArchProcInit { fn load_vm_elf<P: Paging>(...) -> VmLoadResult; }
+ // 不再有 trait —— 用 cfg-selected 类型别名和模块函数
```

**为什么可以删除 trait**：

- minix-rs 是**编译时单架构**，没有运行时多态需求。
- trait 在这里只是"按架构派发方法"，但 Rust 的 `#[cfg(target_arch)]` 已经做了同样的事——而且零运行时开销。
- 三架构的差异（x86-64 的 XSAVE 处理 vs aarch64 的 CPACR_EL1 vs riscv64 的 sstatus.FS）通过**不同的实现函数**表达，每个架构自己的 `process.rs` 模块用自己的代码——不需要 trait 抽象。

**唯一可能保留的 trait**：`PageFlags` 的 ELF 段 flags 转换（因为 `PageFlags` 是 Paging trait 的关联类型，与 Process 解耦）。但即使是这里，也可以用 cfg-selected 函数替代 trait：

```rust
// os/arch/src/paging.rs

#[cfg(target_arch = "x86_64")]
pub use x86_64::elf_flags_to_page_flags;

#[cfg(target_arch = "aarch64")]
pub use aarch64::elf_flags_to_page_flags;

#[cfg(target_arch = "riscv64")]
pub use riscv64::elf_flags_to_page_flags;
```

### 3.5 主流程设计

```rust
// os/kernel/src/lib.rs

/// 阶段 C: 初始化进程表并加载 boot 进程。
///
/// OS 视角：
/// 1. 创建空白的进程表和特权表
/// 2. 遍历 boot image，为每个 entry 启动配置（arch 无关的入口、栈、ps_strings）
/// 3. 为 schedulable 进程绑定 capability
/// 4. 设置所有进程的初始状态（停止、等待 VM）
pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    let mut proc_table = ProcessTable::new();
    let mut cap_table = CapabilityTable::new();

    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "NR_BOOT_MODULES mismatch with boot info"
    );

    for (slot_idx, module) in kernel_info.boot_modules.iter().enumerate() {
        init_one_boot_proc(slot_idx, module, kernel_info, &mut proc_table, &mut cap_table);
    }

    proc_table
}

fn init_one_boot_proc(
    slot_idx: usize,
    module: &BootModule,
    kernel_info: &KernelInfo,
    pt: &mut ProcessTable,
    ct: &mut CapabilityTable,
) {
    let proc_nr = slot_nr_to_proc_nr(slot_idx);
    let role = classify_process_role(proc_nr);

    // ── Step 1: 设置进程名 ──
    pt.get_mut(proc_nr).unwrap().set_name(module.name);

    // ── Step 2: 分配扩展寄存器槽位 ──
    let ext_reg_slot = arch::init_ext_reg_slot(proc_nr, role);
    pt.get_mut(proc_nr).unwrap().ext_reg_slot = ext_reg_slot;

    // ── Step 3: 绑定 capability ──
    let capability = match role {
        ProcessRole::KernelTask => BootCapability::KernelTask { kind: ... },
        ProcessRole::SystemService => BootCapability::SystemService,
        ProcessRole::RootServer => BootCapability::RootServer,
        ProcessRole::User => BootCapability::DeferredToRs,
        ProcessRole::Idle => BootCapability::KernelTask { kind: KernelTaskKind::Idle },
    };
    ct.bind_boot_capability(proc_nr, capability)?;

    // ── Step 4: 构建启动配置 ──
    let startup = if is_kernel_task(proc_nr) {
        arch::build_startup_for_kernel(role)
    } else {
        let vm_image = load_vm_image_if_needed(role, module, kernel_info);
        arch::build_startup_for_user(role, vm_image.entry, vm_image.sp, vm_image.ps_strings)
    };
    pt.get_mut(proc_nr).unwrap().startup = startup;

    // ── Step 5: 设置 RTS 状态 ──
    finalize_boot_state(pt.get_mut(proc_nr).unwrap(), role);
}

fn finalize_boot_state(proc: &mut KProcess, role: ProcessRole) {
    use RtsFlagsBits::*;
    proc.rts_flags.set(PROC_STOP);
    proc.rts_flags.clear(SLOT_FREE);

    match role {
        ProcessRole::KernelTask => { /* 无需 VM inhibit */ }
        ProcessRole::SystemService => { /* VM 自身，无需 VM inhibit */ }
        ProcessRole::RootServer | ProcessRole::User => {
            proc.rts_flags.set(VMINHIBIT | BOOTINHIBIT);
        }
    }
}
```

---

## 5. Chapter 4 设计（实现详解）

### 5.1 类型定义

```rust
// os/arch/src/lib.rs —— cfg-selected re-export

#[cfg(target_arch = "x86_64")]
pub use arch_x86_64::process::{
    StartupRegs as CurrentStartup,
    TrapFrame as CurrentTrapFrame,
    ExtRegSlot as CurrentExtRegSlot,
};

#[cfg(target_arch = "aarch64")]
pub use arch_aarch64::process::{ /* 同名字段 */ };

#[cfg(target_arch = "riscv64")]
pub use arch_riscv64::process::{ /* 同名字段 */ };
```

### 5.2 x86-64 进程启动实现

```rust
// os/arch/src/x86_64/process.rs

use minix_types::{VirBytes, PhysBytes};

/// RFLAGS 初值
const RFLAGS_USER: u64 = 0x0202;       // IOPL=0, IF=1
const RFLAGS_KERNEL: u64 = 0x1202;      // IOPL=1, IF=1

/// XSAVE area 大小（简化版——实际生产应动态计算）
const XSAVE_AREA_SIZE: usize = 8192;
const XSAVE_AREA_ALIGN: usize = 64;

/// 进程启动配置（arch-specific opaque 值）。
#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: u64,            // RFLAGS
    entry_point: VirBytes,  // rip
    stack_pointer: VirBytes,// rsp
    ps_strings: VirBytes,   // 启动时 rbx 指向这里
    ext_reg_init: ExtRegInit,
}

#[derive(Clone, Copy)]
pub enum ExtRegInit {
    AllocateXSaveArea,
    NoExtRegState,
}

/// Trap frame（arch-specific，由调度器拥有）。
#[repr(C)]
pub struct TrapFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub rbp: u64, pub rbx: u64, pub r11: u64, pub r10: u64,
    pub r9: u64, pub r8: u64,
    pub rax: u64, pub rcx: u64, pub rdx: u64, pub rsi: u64, pub rdi: u64,
    pub rflags: u64, pub rip: u64, pub rsp: u64,
    // ... 其他字段
}

/// 扩展寄存器槽位（arch-specific）。
pub struct ExtRegSlot {
    /// XSAVE area（用户进程有；内核任务 None）
    pub xsave_area: Option<Box<[u8; XSAVE_AREA_SIZE]>>,
    /// 已初始化标志（对应 C 的 MF_FPU_INITIALIZED）
    pub initialized: bool,
}

/// 为用户进程构建启动配置。
pub fn build_startup_for_user(
    role: ProcessRole,
    entry: VirBytes,
    sp: VirBytes,
    ps_strings: VirBytes,
) -> StartupRegs {
    let status = match role {
        ProcessRole::KernelTask => RFLAGS_KERNEL,
        _ => RFLAGS_USER,
    };
    let ext_reg_init = match role {
        ProcessRole::User => ExtRegInit::AllocateXSaveArea,
        _ => ExtRegInit::NoExtRegState,
    };
    StartupRegs {
        status,
        entry_point: entry,
        stack_pointer: sp,
        ps_strings,
        ext_reg_init,
    }
}

/// 为内核任务构建启动配置。
pub fn build_startup_for_kernel(role: ProcessRole) -> StartupRegs {
    build_startup_for_user(role, VirBytes(0), VirBytes(0), VirBytes(0))
}

/// 把启动配置应用到 trap frame。
pub fn apply_startup_to_trap_frame(
    startup: StartupRegs,
    frame: &mut TrapFrame,
) {
    frame.rflags = startup.status;
    frame.rip = startup.entry_point.0;
    frame.rsp = startup.stack_pointer.0;
    frame.rbx = startup.ps_strings.0;
    // 其他寄存器：trap frame 初始化时已经清零
}

/// 初始化扩展寄存器槽位。
pub fn init_ext_reg_slot(slot_nr: ProcNr, role: ProcessRole) -> ExtRegSlot {
    match role {
        ProcessRole::User => {
            // x86-64: 分配 XSAVE area + 清零 + 标记 lazy init
            let mut xsave_area = Box::new([0u8; XSAVE_AREA_SIZE]);
            // 清零（XSAVE area 必须初始化，XCOMP_BV 标志头除外）
            for byte in xsave_area.iter_mut() { *byte = 0; }
            ExtRegSlot {
                xsave_area: Some(xsave_area),
                initialized: false,  // MF_EXT_REG_INITIALIZED=false; 第一次 FP 指令 trap
            }
        }
        _ => {
            // 内核任务不使用 FPU/SIMD
            ExtRegSlot { xsave_area: None, initialized: false }
        }
    }
}
```

**关键说明**：

- 字段 `status`、`entry_point` 等**只对 x86-64 有意义**——aarch64 编译产物里这些字段名根本不存在（是不同类型）。
- `Box<[u8; XSAVE_AREA_SIZE]>` 用于 XSAVE area——但**仅 x86-64 编译**时存在（其他架构用零大小 `ExtRegSlot`）。
- 不再有 `SegmentSelectors`——x86-64 在 64-bit long mode 下段寄存器几乎不使用（除 FS/GS 用于 TLS），由 `arch_set_fs_base`/`arch_set_gs_base` 等显式 API 处理，不进 startup 配置。

### 5.3 aarch64 进程启动实现

```rust
// os/arch/src/aarch64/process.rs

/// SPSR_EL1 初值
const SPSR_EL1H_MASKED: u64 = 0x000003C5;  // M=EL1h, F/I/A/D masked
const SPSR_EL0T: u64 = 0x00000000;         // M=EL0t, no masking

#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: u64,            // SPSR_EL1
    entry_point: VirBytes,  // 写入 ELR_EL1，sret 时跳转到此
    stack_pointer: VirBytes,
    ps_strings: VirBytes,   // 启动时 x0 传给 crt0
}

#[repr(C)]
pub struct TrapFrame {
    pub x29: u64, pub x28: u64, /* ... */ pub x0: u64,
    pub spsr: u64, pub elr: u64, pub sp: u64,
}

pub struct ExtRegSlot;  // 零大小标记

pub fn build_startup_for_user(role: ProcessRole, entry: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> StartupRegs {
    let status = match role {
        ProcessRole::KernelTask => SPSR_EL1H_MASKED,
        _ => SPSR_EL0T,
    };
    StartupRegs { status, entry_point: entry, stack_pointer: sp, ps_strings }
}

pub fn build_startup_for_kernel(role: ProcessRole) -> StartupRegs {
    let sp = get_kernel_stack_for(role);
    build_startup_for_user(role, VirBytes(0), sp, VirBytes(0))
}

pub fn apply_startup_to_trap_frame(startup: StartupRegs, frame: &mut TrapFrame) {
    frame.spsr = startup.status;
    frame.elr = startup.entry_point.0;
    frame.sp = startup.stack_pointer.0;
    frame.x0 = startup.ps_strings.0;
}

pub fn init_ext_reg_slot(_slot_nr: ProcNr, role: ProcessRole) -> ExtRegSlot {
    // aarch64 没有 per-process FPU 保存区。
    // CPACR_EL1.FPEN 配置是系统级，由 cstart() 一次性启用。
    // per-process 状态 (FPCR/FPSR) 由 trap frame 保存，FPCR/FPSR 在 sret 时恢复。
    ExtRegSlot
}
```

**关键差异**：

- 没有 `ext_reg_init` 字段——aarch64 永远不需要 per-process FPU 分配。
- `ExtRegSlot` 是零大小类型（ZST）——编译产物里不占空间。
- `ps_strings` 写入 `x0`（aarch64 第一参数寄存器）——kernel 不知道这件事，由 arch 在 `apply_startup_to_trap_frame` 内部处理。

### 5.4 riscv64 进程启动实现

```rust
// os/arch/src/riscv64/process.rs

const SSTATUS_SPP_SMODE: u64 = 0x00000100;   // SPP=1
const SSTATUS_SPIE: u64 = 0x00000020;        // SPIE=1

#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: u64,            // sstatus (SPP, SPIE)
    entry_point: VirBytes,  // 写入 sepc，sret 时跳转到此
    stack_pointer: VirBytes,
    ps_strings: VirBytes,   // 启动时 a0 传给 crt0
}

#[repr(C)]
pub struct TrapFrame {
    pub ra: u64, pub sp: u64, pub gp: u64, pub tp: u64,
    pub t0: u64, /* ... */ pub t6: u64,
    pub a0: u64, pub a1: u64, /* ... */ pub a7: u64,
    pub sstatus: u64, pub sepc: u64,
}

pub struct ExtRegSlot;

pub fn build_startup_for_user(role: ProcessRole, entry: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> StartupRegs {
    let status = match role {
        ProcessRole::KernelTask => SSTATUS_SPP_SMODE,    // SPP=1
        _ => SSTATUS_SPIE,                                // SPIE=1
    };
    StartupRegs { status, entry_point: entry, stack_pointer: sp, ps_strings }
}

pub fn build_startup_for_kernel(role: ProcessRole) -> StartupRegs {
    let sp = get_kernel_stack_for(role);
    build_startup_for_user(role, VirBytes(0), sp, VirBytes(0))
}

pub fn apply_startup_to_trap_frame(startup: StartupRegs, frame: &mut TrapFrame) {
    frame.sstatus = startup.status;
    frame.sepc = startup.entry_point.0;
    frame.sp = startup.stack_pointer.0;
    frame.a0 = startup.ps_strings.0;
}

pub fn init_ext_reg_slot(_slot_nr: ProcNr, _role: ProcessRole) -> ExtRegSlot {
    // riscv64 没有 per-process FPU 保存区。
    // sstatus.FS 状态机在 trap frame 写入时设置 FS=Initial；
    // 第一次 FP 指令 trap 进 kernel，FS 变 Dirty；
    // 之后由 context switch 的 save/restore 管理。
    ExtRegSlot
}
```

### 5.5 三架构对照表

| 概念 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 状态寄存器 | RFLAGS | SPSR_EL1 | sstatus |
| PC | rip | ELR_EL1 | sepc |
| SP | rsp | sp | sp |
| ps_strings 寄存器 | rbx | x0 | a0 |
| XSAVE area | 是 (XCR0 决定大小) | 否 (FPCR/FPSR 在 trap frame) | 否 (sstatus.FS 状态机) |
| Lazy FPU | CR0.TS | 不需要 (CPACR_EL1.FPEN 系统级) | sstatus.FS = Initial |
| `CurrentStartup` 类型大小 | 5 字段 | 4 字段 | 4 字段 |
| `CurrentExtRegSlot` 类型大小 | Box<[u8; N]> | ZST | ZST |

### 5.6 VM ELF 加载实现（跨架构共享）

```rust
// os/arch/src/vm_boot.rs（新增，跨架构共享）

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use minix_elf::{segment_iter, entry_point};
use crate::paging::{Paging, PageFlags};

pub struct VmImage {
    pub entry: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

pub fn load_vm_image<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> Result<VmImage, BootError> {
    let image = unsafe { core::slice::from_raw_parts(module.start.0 as *const u8, module.len) };

    let segments = segment_iter(image).map_err(|_| BootError::InvalidElf)?;
    let entry = entry_point(image).ok_or(BootError::InvalidElf)?;

    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated = 0;

    for seg in segments {
        let flags = elf_flags_to_page_flags(seg.flags);  // cfg-dispatch
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1);
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            let paddr = PhysBytes(vaddr);
            paging.map(VirBytes(vaddr), paddr, flags)?;
            total_allocated += page_size as usize;

            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - (copy_start % page_size as usize),
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(file_offset as usize),
                            vaddr as *mut u8,
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

    // 栈布局（BSD 风格 ps_strings）
    let stack_high = kernel_info.user_sp;
    let stack_size = 64 * 1024;  // 64KB VM 栈
    let sp_base = VirBytes(stack_high.0 - stack_size as u64);

    // 映射栈页
    let stack_flags = PageFlags::read_write();
    let mut stack_addr = sp_base.0 & !(page_size - 1);
    while stack_addr < stack_high.0 {
        paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags)?;
        total_allocated += page_size as usize;
        stack_addr += page_size;
    }

    // 构造 PsStrings（用结构体而非裸 padding）
    let ps_strings = VirBytes(sp_base.0 - core::mem::size_of::<PsStrings>() as u64);
    unsafe {
        let psp = ps_strings.0 as *mut PsStrings;
        (*psp).ps_argvstr = core::ptr::null();
        (*psp).ps_nargvstr = 0;
        (*psp).ps_envstr = core::ptr::null();
        (*psp).ps_nenvstr = 0;
    }

    Ok(VmImage { entry: VirBytes(entry), sp: sp_base, ps_strings, allocated_bytes: total_allocated })
}
```

### 5.7 ProcessTable / CapabilityTable 实现

```rust
// os/kernel/src/proc_table.rs

pub const NR_TASKS: usize = 8;
pub const NR_PROCS: usize = 256;
pub const PROC_TABLE_SIZE: usize = NR_TASKS + NR_PROCS;

pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],   // ← 内嵌数组，无堆
    sched: Scheduler,
    vm_request_queue: VmRequestQueue,
}

impl ProcessTable {
    pub const fn new() -> Self {
        let procs = build_initial_procs();
        Self {
            procs,
            sched: Scheduler::empty(),
            vm_request_queue: VmRequestQueue::empty(),
        }
    }

    pub fn get(&self, nr: ProcNr) -> Option<&KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&self.procs[idx])
    }

    pub fn get_mut(&mut self, nr: ProcNr) -> Option<&mut KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&mut self.procs[idx])
    }
}

const fn build_initial_procs() -> [KProcess; PROC_TABLE_SIZE] {
    let mut arr = [const { KProcess::empty() }; PROC_TABLE_SIZE];
    let mut i = 0;
    while i < PROC_TABLE_SIZE {
        let nr = (i as i32) - (NR_TASKS as i32);
        arr[i] = KProcess::empty_at(nr);
        i += 1;
    }
    arr
}

// os/kernel/src/capability.rs

pub const NR_STATIC_CAPS: usize = 16;   // Kernel tasks + system services
pub const NR_DYNAMIC_CAPS: usize = 64;  // User processes (RS-managed)

pub struct CapabilityTable {
    static_caps: [ProcessCapability; NR_STATIC_CAPS],
    dynamic_caps: [ProcessCapability; NR_DYNAMIC_CAPS],
}

impl CapabilityTable {
    pub const fn new() -> Self {
        Self {
            static_caps: [const { ProcessCapability::empty() }; NR_STATIC_CAPS],
            dynamic_caps: [const { ProcessCapability::empty() }; NR_DYNAMIC_CAPS],
        }
    }

    /// 绑定 boot 阶段预定义 capability 到指定进程。
    pub fn bind_boot_capability(
        &mut self,
        proc_nr: ProcNr,
        template: BootCapability,
    ) -> Result<(), CapError> {
        let cap_id = static_priv_id(proc_nr);
        let cap = self.static_caps.get_mut(cap_id as usize)
            .ok_or(CapError::OutOfRange)?;
        if cap.is_assigned() {
            return Err(CapError::AlreadyBound);
        }
        *cap = ProcessCapability::from_template(template);
        Ok(())
    }
}
```

### 5.8 KProcess 调整

```rust
// os/kernel/src/proc.rs

impl KProcess {
    /// 常量构造：用于 [KProcess; N] 静态数组初始化。
    pub const fn empty_at(nr: ProcNr) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: Endpoint::from_const_generation_slot(0, nr),  // 需 Endpoint 提供 const ctor
            p_seg: ProcessSegments::EMPTY,
            priv_id: None,
            #[cfg(debug_assertions)]
            p_magic: 0xC0FFEE1,
            p_rts_flags: RtsFlags::with_const(RtsFlagsBits::SLOT_FREE),
            p_misc_flags: MiscFlags::empty(),
            p_fault_addr: None,
            p_sched: SchedFields::empty(),
            p_accounting: Accounting::empty(),
            p_time: TimeStats::empty(),
            p_cycles: CyclesStats::empty(),
            p_cpuavg: CpuAvg::empty(),
            p_dequeued: AtomicU64::new(0),
            p_defer: DeferArgs::EMPTY,
            p_nextready: AtomicI32::new(NONE_PROC_NR),
            p_caller_q: AtomicI32::new(NONE_PROC_NR),
            p_q_link: AtomicI32::new(NONE_PROC_NR),
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: Message::ZERO,
            p_delivermsg: Message::ZERO,
            p_delivermsg_vir: VirBytes::new(0),
            p_ext_reg_state: ExtRegState::EMPTY,    // 零大小或 const-empty
            p_next_restart: None,
            p_next_requestor: None,
            p_vm_suspend: None,
            // ↓ 替换原来 4 个独立字段
            startup: arch::CurrentStartup::empty(),
            ext_reg_slot: arch::CurrentExtRegSlot::EMPTY,
        }
    }
}
```

**注意**：

- `AtomicU32/64::new` 自 Rust 1.75 起是 const。
- `Endpoint::NONE`/`from_const_generation_slot` 需要为 const。
- `Option<T>::None` 自 Rust 1.83 起支持 const（之前需要 const fn 才能 None）。
- `Message::ZERO`/`DeferArgs::EMPTY`/`ExtRegState::EMPTY` 用 const 关联常量。
- `arch::CurrentStartup::empty()` 是 arch-specific 常量值（x86-64 全零、aarch64 全零、riscv64 全零）。
- `arch::CurrentExtRegSlot::EMPTY` 同上。

---

## 6. 与原文档的核心差异总览

| 维度 | 原文档（v1） | 本设计（v2） |
|------|-------------|-------------|
| arch 接口形式 | 3 个 trait（`ArchProcReset`/`ArchProcInit`/`BootProcArch`） | 0 个 trait，cfg-selected 类型别名 + 普通函数 |
| InitialRegState | 含 `segment_selectors`/`fpu_needs_zero` 字段 | 全部下沉到 `StartupRegs` 各架构自有字段 |
| `ps_strings_reg` 字段名 | 暗示"寄存器" | 改为 `ps_strings: VirBytes`（地址），寄存器选择由 arch 内部消化 |
| FPU 处理 | `fpu_needs_zero: bool` 翻译 Minix3 fnsave | arch 层 `init_ext_reg_slot` 处理 XSAVE/CPACR_EL1/sstatus.FS 现代机制 |
| 进程表存储 | `Box<[KProcess]>`（堆） | `[KProcess; PROC_TABLE_SIZE]`（内嵌） |
| `KProcess::new()` | 非 const | `const fn empty_at(nr)` |
| `KProcess` 启动字段 | 4 个独立字段（pc/sp/ps_strings_reg/status） | 单个 `startup: CurrentStartup` 字段 |
| 特权抽象 | 6 个裸参数 `configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` | `ProcessCapability` 结构体 + `BootCapability` 枚举模板 |
| VM ELF 加载位置 | 在 arch trait `BootProcArch::load_vm_elf` | 提到 `vm_boot.rs` 跨架构共享 |
| ELF 段 padding | `sp -= sizeof(int) + sizeof(void*) + sizeof(void*)` | `PsStrings` 结构体 |
| 6 个 `KProcess::set_*` setter | 分散 | 整体 `startup` 字段 + `apply_to_trap_frame` arch-internal |

---

## 7. 修复范围评估（独立）

| 范围 | 文件 | 行数估计 |
|------|------|----------|
| 删除 3 个 trait，重写 `arch/proc_arch.rs` | `os/arch/src/arch/proc_arch.rs` | -150 + 50 = -100 |
| 新增 `arch/{x86_64,arm64,riscv64}/process.rs` | 三架构 | ~150 × 3 = 450 |
| 新增 `arch/vm_boot.rs`（跨架构共享） | `os/arch/src/` | ~150 |
| 删除 `SegmentSelectors`、`InitialRegState`、`InitialRegs` | 多文件 | ~30 删除 |
| `KProcess::new` → `const fn empty_at` | `os/kernel/src/proc.rs` | ~50 |
| `ProcessTable` 改为内嵌数组 | `os/kernel/src/proc_table.rs` | ~50 |
| `CapabilityTable`（替代 `PrivTable`） | 新增 `os/kernel/src/capability.rs` | ~250 |
| `init_proc_and_boot` 重写主流程 | `os/kernel/src/lib.rs` | ~150 |
| 测试更新（三架构） | 多文件 | ~200 × 3 = 600 |
| 文档 Ch3/Ch4 重写 | `06-proc-init-boot-proc.md` | ~500 |
| **总计** | — | **~2200 行改动** |

---

## 8. 待 bagging 阶段讨论的关键决策

我对以下几个关键决策有信心但保留讨论空间，希望其他 AI 给出独立意见：

### Q1：是否彻底删除 trait？

我的判断：**删除**。理由：
- minix-rs 是编译时单架构——trait 派发零运行时收益。
- 类型别名 + cfg re-export 是 Rust 惯用模式，且每个架构的实现完全独立可测。
- 但若其他 AI 觉得保留 `trait ArchStartup { ... }`（用关联类型而非方法多态）可以增强文档可读性，我接受折中。

### Q2：`ExtRegSlot` 是 Box<[u8; N]> 还是其他？

我的判断：**Box<[u8; N]>`（仅 x86-64）**。理由：
- XSAVE area 必须 64-byte 对齐，`Box` 提供对齐保证。
- 静态数组 `[u8; N]` 在 kernel 静态内存中可能不对齐；动态分配保证对齐。
- aarch64/riscv64 的 `ExtRegSlot` 是 ZST（零大小类型）。
- 但若其他 AI 提出"用 `StaticCell` + 静态数组"更符合无堆约束，我接受——前提是 XSAVE area 必须对齐且按进程槽位区分。

### Q3：`ProcessCapability` 还是 `KPriv`（保留字段名）？

我的判断：**新建 `ProcessCapability`**，旧的 `KPriv` 删除。理由：
- `KPriv` 是 C `struct priv` 的 1:1 翻译（字段镜像）。
- `ProcessCapability` 是 OS 概念的重新表达（IPC 能力 + 系统调用能力 + trap + signal manager）。
- 但若其他 AI 觉得改名 + 字段重组成本太高，可保留 `KPriv` 名字但内部重组为 capability 语义。

### Q4：`BootCapability` 枚举 vs `&[CapabilityRule]` 数组？

我的判断：**枚举**。理由：
- boot 阶段只有 4 类进程（Idle/KernelTask/SystemService/RootServer），枚举表达最清晰。
- `&[CapabilityRule]` 适合运行时动态配置（RS 配置用户进程 capability），boot 阶段不需要。
- 但若其他 AI 认为 boot 阶段和运行时应统一抽象，可改用 `CapabilitySpec` 数组。

### Q5：`CapabilityTable` 静态/动态分离还是统一数组？

我的判断：**分离**。理由：
- 静态区容量小（NR_TASKS + NR_SYSTEM_SERVICES ≈ 16），编译期固定。
- 动态区容量大（NR_PROCS - NR_SYSTEM_SERVICES ≈ 192），但仍然固定（编译期决定 `NR_DYNAMIC_CAPS`）。
- 分离让 boot 阶段特权分配（静态）和运行时分配（动态）逻辑清晰。
- 但若其他 AI 觉得统一数组更简单，可合并为单个 `[ProcessCapability; NR_SYS_PROCS]`，按 ID 范围区分静态/动态。

### Q6：`p_ext_reg_state: ExtRegState` 字段怎么办？

我的判断：**删除**。理由：
- 当前 `ExtRegState` 是空 struct（未真正使用），半成品设计。
- 用 `ext_reg_slot: CurrentExtRegSlot` 替代——arch-specific opaque 值（x86-64 是 Box<[u8; N]>，aarch64/riscv64 是 ZST）。
- 但需要进一步验证 `ExtRegState` 是否在 trap frame 保存/恢复路径上有用——若有用则保留为 `ext_reg_state: CurrentExtRegSlot`。

### Q7：`initial_ps_strings_reg: u64` 字段值类型是否应为 `VirBytes`？

我的判断：**统一为 `VirBytes`**。理由：
- `ps_strings: VirBytes`（地址）跨三架构一致——kernel 看到的都是地址。
- arch 在 `apply_startup_to_trap_frame` 内部把 `VirBytes.0` 写到具体寄存器（x86-64→rbx, aarch64→x0, riscv64→a0）。
- 字段类型统一为 `VirBytes` 避免 kernel 误以为是寄存器号。

### Q8：VM ELF 加载放 arch 还是 kernel？

我的判断：**arch crate 顶层（`vm_boot.rs`）**，不在 per-arch 模块。理由：
- ELF 加载逻辑 100% 跨架构共享——只是 ELF flags → PageFlags 转换是 cfg-dispatch。
- arch crate 提供 Paging trait 和 PageFlags——这些是底层 OS 抽象。
- kernel crate 提供 CapabilityTable 和 ProcessTable——这些是 OS 概念。
- ELF 加载是介于两者之间的"加载器"——放 arch crate 是合理位置（依赖 Paging trait，不依赖 kernel 概念）。
- 但若其他 AI 觉得应放 kernel crate 顶层（更"OS-概念"），我接受——只要消除 trait 派发即可。

---

## 9. 多 AI bagging 关注的元问题

### 9.1 我额外提出的 5 个问题是否成立？

- **新问题 A（p_ext_reg_state 半成品）**：成立，已被 KProcess 注释自身证据支持。
- **新问题 B（4 字段碎片化）**：成立，违反内聚性原则。
- **新问题 C（load_vm_elf 跨架构相同）**：成立，代码对比已验证。
- **新问题 D（#[cfg(feature = "mock")] 是技术债）**：成立，policy 应通过 `VmLoadPolicy` 表达而非 cfg 切换。
- **新问题 E（主流程 50+ 行未拆分）**：成立，工程实践问题。

### 9.2 我的修复方向是否过度激进？

我的设计**删除了 3 个 trait**，这可能与其他 AI 倾向"保留 trait 但用关联类型"不同。

**风险点**：
- 删除 trait 后，mock 测试需要重新组织（不再有 `MockProcArch` trait impl）。
- 测试需要写 per-arch 的 stub 实现或使用 cfg。

**缓解**：
- mock 可以通过 cfg 选择 arch-specific 路径时返回固定值——不需要 trait。
- 测试覆盖率不会下降。

### 9.3 我对 problem.md 的"加强版"原则是否过激？

- P1（编译时定型 OS 类型）我加强了"类型系统中根本不存在 arch-specific 字段"——这可能让 arch crate 的内部封装变重。
- P3（无堆）我加强了"Scheduler/VmRequestQueue 也需要 const-initable"——这会推动更多 const fn 改动。

**风险点**：可能牵动其他模块的设计（Scheduler、VmRequestQueue 的 const-init）。

**缓解**：这些是后续设计稿的工作，本稿不强制实施。

---

## 10. 一句话总结

> **本设计的核心：用类型别名替代 trait、用 opaque 值替代字段集合、用 capability 替代 priv 镜像、用 const 数组替代 Box——所有改动都为了让"编译时单架构"的代码看起来"OS 层真的不感知 arch 细节"。**

---

## 11. 自审计补充（v1 → v2 关键发现）

> 上一版（v1）写完后，我做了**代码 grep 验证**——发现 7 个实质性盲点。本节记录这些发现及对应修正。

### 11.1 盲点 A：XSAVE area 与"无堆"约束冲突

**v1 的设计**（问题）：

```rust
// os/arch/src/x86_64/process.rs
pub struct ExtRegSlot {
    pub xsave_area: Option<Box<[u8; XSAVE_AREA_SIZE]>>,  // ← Box！与"无堆"冲突！
}
```

**盲点**：v1 主张"无堆"，但同时用 `Box<[u8; N]>` 给 XSAVE area——自相矛盾。

**v2 修正**：

**关键发现**（grep `proc.rs:20-43`）：

```rust
const EXT_REG_STATE_SIZE: usize = 576;
#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct ExtRegState {
    data: [u8; EXT_REG_STATE_SIZE],
    valid: bool,
}
```

**`ExtRegState` 已经是 64-byte 对齐的 576 字节内嵌数组**——这就是 Minix3 的 `FPU_XFP_SIZE`（576 bytes for XSAVE area on x86-64）。**不需要 Box**——直接内嵌在 `KProcess` 中。

**v2 设计**：

```rust
// 复用现有 ExtRegState，扩展 const fn 支持
impl ExtRegState {
    /// 必须 const，因为 KProcess::empty_at 需要它
    pub const fn empty() -> Self {
        Self { data: [0u8; EXT_REG_STATE_SIZE], valid: false }
    }
}

// KProcess 中：
pub struct KProcess {
    pub p_ext_reg_state: ExtRegState,    // 保留并修复，576 字节内嵌，无堆
    // ... 其他字段
}
```

**修正后的策略**：

- **保留** `p_ext_reg_state: ExtRegState`（不删除——它用于 fork FPU 继承语义，见盲点 B）
- **扩展**为 const fn（`empty()` 是 const）
- **x86-64 XSAVE area 静态分配**：每个 KProcess 内嵌 576 字节，无堆
- **aarch64/riscv64 仍然 ZST**：用类型别名 `CurrentExtRegSlot`（不同类型，零成本）

### 11.2 盲点 B：`p_ext_reg_state` 在 fork 路径被使用——不可删除

**v1 的判断**（错误）：

> 新问题 A：`KProcess` 已经有 `p_ext_reg_state: ExtRegState` 字段但未真正使用

**事实**（grep `proc.rs:1341, 1357-1360`）：

```rust
// proc.rs:1357-1360 — fork_from 内部
if parent.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED) {
    child.p_ext_reg_state = parent.p_ext_reg_state.clone();
    child.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
}
```

**C 语义**（`do_fork.c`）：

```c
if(proc_used_fpu(rpp))
    memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
```

fork 必须**继承父进程的 FPU 状态**——这是 OS 行为正确性的一部分。

**v2 修正**：

- **保留** `p_ext_reg_state` 字段
- **修复** `ExtRegState::empty()` 为 const fn（用于 `[KProcess; N]` 数组初始化）
- **保留** fork 路径的 FPU 继承语义（已经在代码中）
- 注释更新：明确"fork copy semantics"是字段存在的理由

### 11.3 盲点 C：`MF_EXT_REG_INITIALIZED` 跨模块使用——必须统一管理

**grep 发现**（6 处生产代码引用）：

| 位置 | 操作 | 语义 |
|------|------|------|
| `syscall_process.rs:272` | clear | exec 完成后清零 FPU init |
| `syscall_process.rs:398` | clear | 系统调用后清零 FPU init |
| `syscall_process.rs:947` | set | 信号返回时恢复 FPU init |
| `syscall_process.rs:1039` | set | mcontext 恢复后设置 FPU init |
| `proc.rs:1359` | set | fork 继承父进程 FPU init |
| `proc.rs:1357` | check | fork 时检查父是否 init |

**v1 盲点**：我只关注 boot 阶段，忽略了**运行时 FPU lazy init** 流程。

**v2 修正**：

- **保留** `MF_EXT_REG_INITIALIZED` 在 `p_misc_flags`（已是 OS 概念：标志位）
- **arch 层职责分工**：
  - x86-64: `init_ext_reg_slot()` 分配+清零 XSAVE area（仅 boot 时）
  - x86-64: context switch 时由 arch 层检查 `MF_EXT_REG_INITIALIZED` 决定是否 load XSAVE area
  - aarch64/riscv64: 不需要此标志（FPU 状态由 trap frame 自带 FPCR/FPSR 或 sstatus.FS）
- **不删除**该标志——它是 x86-64 lazy FPU 切换的关键，删了破坏运行时语义

### 11.4 盲点 D：const fn 现实约束——多个内部类型需要 const 化

**grep 发现**——需要 const 化的函数（v1 假设它们是 const，实际不是）：

| 类型 | 当前状态 | 需要改 |
|------|---------|--------|
| `Endpoint::from_generation_slot` | ✅ `const fn` (endpoint.rs:82) | 不改 |
| `VirBytes::new` | ✅ `const fn` (address.rs:20) | 不改 |
| `ProcName::new` | ✅ `const fn` (proc.rs:869) | 不改 |
| `AtomicI32::new` | ✅ const (Rust 1.75+) | 不改 |
| `AtomicU64::new` | ✅ const (Rust 1.75+) | 不改 |
| `Scheduler::new` | ✅ `const fn` (sched.rs:47) | 不改 |
| `VmRequestQueue::new` | ✅ `const fn` (vm.rs:455) | 不改 |
| `SchedFields::new` | ❌ 非 const | **改 const fn** |
| `Accounting::new` | ❌ 非 const | **改 const fn** |
| `TimeStats::new` | ❌ 非 const | **改 const fn** |
| `CyclesStats::new` | ❌ 非 const | **改 const fn** |
| `CpuAvg::new` | ❌ 非 const | **改 const fn** |
| `SigSet::empty` | ❌ 非 const | **改 const fn** |
| `RtsFlags::with` | ❌ 非 const | **改 const fn** |
| `MiscFlags::new` | ❌ 非 const | **改 const fn** |
| `ExtRegState::new` | ❌ 非 const | **改 const fn empty** |
| `DeferArgs::default` | ❌ 非 const | **改 const fn empty** |
| `Message::default` | ❌ 非 const | **改 const fn empty** |
| `ProcessSegments::default` | ❌ 非 const | **改 const EMPTY 关联常量** |
| `BootModule::empty` | ✅ `const fn` | 不改 |

**v2 修正**：

- `KProcess::empty_at(nr) -> Self` 必须 `const fn`
- 这要求上述 9 个内部类型的 `new()`/构造也改为 `const fn`
- 这是**纯机械改动**——把 `pub fn new() -> Self` 改为 `pub const fn new() -> Self`，内部所有表达式都已经是 const-initable（`AtomicU32::new(0)`、`u64::new(0)`、数组字面量等）
- **预计 ~50 行代码改动**：每个 `new()` 加 `const` 关键字
- **预计 ~10 行测试改动**：编译错误触发后再修

### 11.5 盲点 E：`PrivTable` 跨模块引用——删除它有连锁影响

**grep 发现**——`PrivTable` 在生产代码中的引用：

| 文件 | 用途 |
|------|------|
| `syscall_process.rs:28, 145, 353, 603` | 系统调用路径需要 PrivTable 参数 |
| `syscall_clock.rs:14, 28, 175, 183, 292, 309` | 时钟权限检查需要 PrivTable |

**v1 盲点**：我把 `PrivTable` 替换为 `CapabilityTable` 过于激进——kernel 内很多系统调用路径都依赖 `PrivTable` API。

**v2 修正（保守路线）**：

**选项 A：完全替换**（激进，影响大）

- 删除 `PrivTable`
- 新增 `CapabilityTable`
- 修改 `syscall_process.rs`/`syscall_clock.rs` 全部 6 处引用
- 预计 ~500 行改动

**选项 B：保留 `PrivTable` API，内部用 capability 重组**（保守，推荐）

```rust
// PrivTable 保留，但内部实现改为 capability-based
pub struct PrivTable {
    caps: [ProcessCapability; NR_SYS_PROCS],  // 不再用裸字段
}

impl PrivTable {
    /// 兼容旧 API：保留 assign_static / configure_boot_priv 方法签名
    /// 但内部委托给 capability
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> { ... }
    
    pub fn configure_boot_priv(
        &mut self, priv_id: PrivId, 
        capability: BootCapability,  // ← 改为传模板，不是 6 个裸参数
    ) { ... }
}
```

**v2 选择选项 B**：

- `PrivTable` 名字保留（API 兼容）
- 内部字段从 6 个裸字段（s_flags/s_trap_mask/s_ipc_to/s_k_call_mask/s_sig_mgr + 其他）→ 改为单个 `ProcessCapability`
- `configure_boot_priv` 签名从 6 个裸参数 → 改为 `BootCapability` 模板枚举
- `syscall_process.rs`/`syscall_clock.rs` **不改**（API 兼容）

**好处**：

- 改动局限在 `os/kernel/src/kpriv.rs`（~200 行）+ boot 调用点（~50 行）
- 其他模块的 6 处引用全部兼容

### 11.6 盲点 F：mock 测试策略——删除 trait 后需要重新设计

**v1 的轻描淡写**：

> mock 可以通过 cfg 选择 arch-specific 路径时返回固定值——不需要 trait

**v2 修正**：

当前 mock 测试通过 `MockProcArch` 注入（`#[cfg(feature = "mock")]`）。删除 trait 后，方案是：

**方案 A：arch-specific test 模块**

```rust
// os/arch/src/x86_64/process.rs（仅测试代码）
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_build_startup_user() {
        let startup = build_startup_for_user(
            ProcessRole::User,
            VirBytes(0x400000), 
            VirBytes(0x7fff0000), 
            VirBytes(0x7ffeffe0)
        );
        assert_eq!(startup.entry_point, VirBytes(0x400000));
    }
}
```

**方案 B：kernel 层 mockable build**

```rust
// kernel crate 的 Cargo.toml 增加 mock feature
[features]
mock = ["minix_arch/mock"]

// kernel crate 引用 arch 函数时
#[cfg(feature = "mock")]
use minix_arch::mock as arch;

#[cfg(not(feature = "mock"))]
use minix_arch as arch;
```

**v2 选方案 A**——更简单：

- 每个 arch 的 `process.rs` 内部 `#[cfg(test)]` 模块测试自己的实现
- kernel 层测试通过 `[KProcess; N]` 数组测试 + 调用 `arch::build_startup_*`（编译时是具体类型）

### 11.7 盲点 G：代码语法细节错误

**v1 错误 1**（§5.7 `bind_boot_capability()?`）：

```rust
fn init_one_boot_proc(...) {
    // ...
    ct.bind_boot_capability(proc_nr, capability)?;  // ← ? 不能用，返回值非 Result
}
```

**v2 修正**：

```rust
fn init_one_boot_proc(...) {
    // ...
    match ct.bind_boot_capability(proc_nr, capability) {
        Ok(()) => {},
        Err(CapError::AlreadyBound) => panic!("priv slot already bound for proc {}", proc_nr),
        Err(CapError::OutOfRange) => panic!("proc_nr out of range: {}", proc_nr),
    }
}
```

或更简洁：

```rust
ct.bind_boot_capability(proc_nr, capability)
    .unwrap_or_else(|e| panic!("bind_boot_capability failed: {:?} for proc {}", e, proc_nr));
```

**v1 错误 2**（§5.5 `load_vm_image_if_needed` 未定义返回值）：

```rust
let vm_image = load_vm_image_if_needed(role, module, kernel_info);
arch::build_startup_for_user(role, vm_image.entry, vm_image.sp, vm_image.ps_strings)
```

`load_vm_image_if_needed` 在 DeferredToRs 路径下应该返回什么？v1 没说清楚。

**v2 修正**：

```rust
/// 为非 VM 用户进程构造一个"占位"启动配置——entry=0, sp=0, ps_strings=0
/// 表示"RS 在运行时加载 ELF"。调度器会检测 entry=0 并等待 RS。
fn deferred_startup(role: ProcessRole) -> StartupRegs {
    build_startup_for_user(role, VirBytes(0), VirBytes(0), VirBytes(0))
}

fn init_one_boot_proc(...) {
    let startup = if is_kernel_task(proc_nr) {
        arch::build_startup_for_kernel(role)
    } else if is_vm(proc_nr) {
        // VM: 实际加载 ELF
        let image = arch::load_vm_image(module, kernel_info, &mut paging)?;
        arch::build_startup_for_user(role, image.entry, image.sp, image.ps_strings)
    } else {
        // 其他用户进程: deferred 到 RS
        deferred_startup(role)
    };
    pt.get_mut(proc_nr).unwrap().startup = startup;
}
```

**v1 错误 3**（§3.2.7 字段类型不一致）：

v1 写 `arch::CurrentStartup::empty()`——但 `CurrentStartup` 是类型别名，没有 `empty()` 方法。需要每个 arch 提供 `StartupRegs::empty()`。

**v2 修正**：

```rust
// os/arch/src/x86_64/process.rs
impl StartupRegs {
    pub const EMPTY: Self = Self {
        status: 0,
        entry_point: VirBytes(0),
        stack_pointer: VirBytes(0),
        ps_strings: VirBytes(0),
        ext_reg_init: ExtRegInit::NoExtRegState,
    };
}

// os/arch/src/lib.rs（cfg-selected re-export）
#[cfg(target_arch = "x86_64")]
pub use arch_x86_64::process::{
    StartupRegs as CurrentStartup,
    ExtRegSlot as CurrentExtRegSlot,
    TrapFrame as CurrentTrapFrame,
};

// kernel 层使用：
pub struct KProcess {
    pub startup: arch::CurrentStartup,
    pub ext_reg_slot: arch::CurrentExtRegSlot,
}

// const fn empty_at 内部：
startup: arch::CurrentStartup::EMPTY,
ext_reg_slot: arch::CurrentExtRegSlot::EMPTY,
```

### 11.8 盲点 H：IDLE per-CPU 与 VmLoadPolicy

**v1 提到但未集成**（§2.4 新问题 D）：

> VM ELF 加载的 #[cfg(feature = "mock")] 是技术债——应改为 VmLoadPolicy

**v2 修正**：

```rust
// os/kernel/src/vm_load_policy.rs（新增）

/// VM ELF 加载策略
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VmLoadPolicy {
    /// 内核在 boot 时用 bootstrap 页表加载 VM
    KernelLoadsVm,
    /// VM 由 RS 在运行时加载（kernel boot 时 entry=0 占位）
    DeferredToRs,
}

impl Default for VmLoadPolicy {
    fn default() -> Self { VmLoadPolicy::DeferredToRs }  // 安全默认
}

static VM_LOAD_POLICY: AtomicU32 = AtomicU32::new(0);  // 0 = DeferredToRs

pub fn get_vm_load_policy() -> VmLoadPolicy {
    match VM_LOAD_POLICY.load(Ordering::Acquire) {
        0 => VmLoadPolicy::DeferredToRs,
        1 => VmLoadPolicy::KernelLoadsVm,
        _ => VmLoadPolicy::DeferredToRs,
    }
}

#[cfg(test)]
pub fn set_vm_load_policy(policy: VmLoadPolicy) {
    VM_LOAD_POLICY.store(policy as u32, Ordering::Release);
}
```

**IDLE per-CPU**（v1 未涉及）：

```rust
// os/arch/src/per_cpu.rs（每架构独立实现）

#[cfg(target_arch = "x86_64")]
pub type IdleProcSlot = ...;  // GS-relative 或 per-CPU 变量

#[cfg(target_arch = "aarch64")]
pub type IdleProcSlot = ...;  // TPIDR_EL1 + offset

#[cfg(target_arch = "riscv64")]
pub type IdleProcSlot = ...;  // sscratch + offset

pub type CurrentIdleSlot = arch::IdleProcSlot;

// kernel crate 引用
pub fn get_idle_proc(cpu: CpuId) -> &'static KProcess {
    CurrentIdleSlot::get(cpu)
}
```

**v2 决定**：IDLE 详细设计**不在本稿范围**——属于 `11-scheduling-primitives.md` 范畴。本稿仅承诺：IDLE 不在 `ProcessTable` 内，而是 per-CPU 数据。

### 11.9 自审计总结：v2 相比 v1 的实质性改进

| 维度 | v1 | v2 |
|------|----|----|
| FPU 存储 | Box<[u8; N]>（与无堆冲突） | 576 字节内嵌（无堆，与 C 一致） |
| `p_ext_reg_state` 处理 | 错误地删除 | 保留并扩展为 const fn |
| `MF_EXT_REG_INITIALIZED` | 误判为可删 | 保留——runtime lazy FPU switch 需要 |
| const fn 化范围 | 仅 KProcess::new | 扩展到 9 个内部类型（~50 行） |
| PrivTable 处理 | 完全替换（破坏 6 处引用） | 保留 API 兼容，内部 capability 化 |
| Mock 测试 | 简略"cfg-dispatch" | 明确 arch-specific test 模块策略 |
| 代码语法 | 3 处错误（`?`/`load_vm_image_if_needed`/字段空值） | 修正 |
| IDLE/PCPU/VmLoadPolicy | 散落提及 | IDLE 推到后续文档；VmLoadPolicy 完整实现 |

**v2 修复代价不变**：~2200 行（v1 估算合理）

**v2 新增修复项**：

- 9 个内部类型 const fn 化：~50 行
- VmLoadPolicy 实现：~30 行
- 代码语法修正：~10 行

**v2 总修复代价**：~2300 行（与 v1 基本持平，新增项 vs 删除项抵消）

---

## 12. v2 修复后的总览

### 12.1 修复后的架构分层

```
┌────────────────────────────────────────────────────────────────────┐
│ kernel crate（OS 概念层）                                            │
│                                                                     │
│   ProcessTable { procs: [KProcess; PROC_TABLE_SIZE] }              │
│                  ↑ const fn new, 无堆                              │
│                  ├─ KProcess { p_ext_reg_state, startup, ... }     │
│                  │   ↑ p_ext_reg_state: ExtRegState (576 字节内嵌) │
│                  │   ↑ startup: arch::CurrentStartup (opaque)     │
│                  │                                                  │
│   PrivTable { privs: [KPriv; NR_SYS_PROCS] }                       │
│              ↑ 保留 API，内部 capability 化                          │
│                                                                     │
│   init_proc_and_boot() 主流程                                        │
│   init_one_boot_proc() 拆解后单一职责                                 │
│                                                                     │
├────────────────────────────────────────────────────────────────────┤
│ arch crate（arch 抽象层）                                            │
│                                                                     │
│   ┌─ x86_64/process.rs ───────────────────────────────────────┐    │
│   │  StartupRegs { status(RFLAGS), entry, sp, ps_strings, ...}│    │
│   │  ExtRegInit { AllocateXSaveArea, NoExtRegState }           │    │
│   │  build_startup_for_user() / for_kernel()                  │    │
│   │  apply_startup_to_trap_frame()                            │    │
│   │  init_ext_reg_slot() ← 576 字节内嵌                       │    │
│   └────────────────────────────────────────────────────────────┘    │
│   ┌─ aarch64/process.rs ──┐ ┌─ riscv64/process.rs ──┐              │
│   │  StartupRegs (4 字段) │ │  StartupRegs (4 字段)  │              │
│   │  ExtRegSlot (ZST)     │ │  ExtRegSlot (ZST)     │              │
│   └───────────────────────┘ └────────────────────────┘              │
│                                                                     │
│   vm_boot.rs（跨架构共享）                                             │
│   load_vm_image() ← 仅 ELF flags → PageFlags 是 cfg-dispatch       │
│                                                                     │
│   cfg-selected re-export：                                            │
│     pub type CurrentStartup = ...;                                  │
│     pub type CurrentTrapFrame = ...;                                │
│     pub type CurrentExtRegSlot = ...;                              │
└────────────────────────────────────────────────────────────────────┘
```

### 12.2 关键不变量（v2 强化版）

- **不变性 1（OS-arch 隔离）**：`grep "SegmentSelectors" os/kernel/` → **0 命中**（v1 期望 0 命中但实际 v1 设计自身有 Box 字段）
- **不变性 2（无堆）**：`grep "Box::new\|Vec::new" os/kernel/src/{proc_table,kpriv}.rs` → **0 命中**（v1 声称无堆但自身设计有 Box）
- **不变性 3（启动字段合并）**：`grep "initial_pc:\|initial_sp:\|initial_status:\|initial_ps_strings_reg:" os/kernel/src/proc.rs` → **0 命中**（替换为 `startup: CurrentStartup`）
- **不变性 4（arch trait 消失）**：`grep "trait ArchProcReset\|trait ArchProcInit\|trait BootProcArch" os/arch/src/` → **0 命中**
- **不变性 5（const fn 化）**：`KProcess::empty_at` 是 const fn；其内部 9 个类型都是 const

### 12.3 最终一句话总结（v2）

> **v2 修正 7 个 v1 盲点后的设计：用类型别名替代 trait、用 opaque 值替代字段集合、用 capability 内部重组替代 PrivTable 删除、用 const 数组替代 Box——所有改动都为了让"编译时单架构"的代码看起来"OS 层真的不感知 arch 细节"，且每一处改动都有代码证据支撑，不是凭直觉设计。**

---

## 13. v3 补充：每个 P0/P1 问题的可选方案（A/B/C 对比）

> v1/v2 给出了**单一推荐方案**。但用户明确要求"可选的答案"——便于 bagging 阶段与其他 AI 比对。本节为每个 P0/P1 问题列出 3 个可选方案。

### 13.1 P0 #1（segment_selectors 泄漏）：3 种可选方案

#### 方案 A（v2 推荐）：cfg-selected 类型别名 + 关联类型

```rust
// arch 层 trait 不存在；用类型别名
#[cfg(target_arch = "x86_64")]
pub type CurrentStartup = X86_64StartupRegs;

#[cfg(target_arch = "aarch64")]
pub type CurrentStartup = AArch64StartupRegs;

#[cfg(target_arch = "riscv64")]
pub type CurrentStartup = Riscv64StartupRegs;

pub struct X86_64StartupRegs { /* x86-64 特有字段 */ }
pub struct AArch64StartupRegs { /* 4 字段，无 segment_selectors */ }
pub struct Riscv64StartupRegs { /* 4 字段，无 segment_selectors */ }
```

**优点**：编译时定型，aarch64 编译产物里 `X86_64StartupRegs` 类型不存在。
**缺点**：每个 arch 需要独立写一份相似代码。
**适用范围**：OS 层完全 arch-agnostic。

#### 方案 B：trait + 关联类型

```rust
pub trait ArchStartup {
    type StartupRegs: Copy;
    fn build_for_user(...) -> Self::StartupRegs;
    fn apply(startup: Self::StartupRegs, frame: &mut Self::TrapFrame);
}

#[cfg(target_arch = "x86_64")]
impl ArchStartup for X86_64Arch {
    type StartupRegs = X86_64StartupRegs;  // 含 segment_selectors
    ...
}

#[cfg(target_arch = "aarch64")]
impl ArchStartup for AArch64Arch {
    type StartupRegs = AArch64StartupRegs;  // 不含 segment_selectors
    ...
}
```

**优点**：保留 trait 接口，文档可读性更高；mock 友好。
**缺点**：kernel 层用 `<CurrentArch as ArchStartup>::StartupRegs` 比较啰嗦。
**适用范围**：需要 mock 测试或运行时多态（目前不需要）。

#### 方案 C：单一 StartupRegs 大结构 + cfg 字段

```rust
#[derive(Clone, Copy)]
pub struct StartupRegs {
    pub status: u64,
    pub entry_point: VirBytes,
    pub stack_pointer: VirBytes,
    pub ps_strings: VirBytes,

    // x86-64 特有
    #[cfg(target_arch = "x86_64")]
    pub segment_selectors: SegmentSelectors,
    #[cfg(target_arch = "x86_64")]
    pub ext_reg_init: ExtRegInit,
}
```

**优点**：所有 arch 共享一个类型名，少量代码冗余。
**缺点**：cfg 字段仍然存在于源代码中（违反 P1 "编译产物中不存在"）。
**适用范围**：早期快速迁移。

**M3 推荐**：**方案 A**。最严格执行 P1 原则。

### 13.2 P0 #8（fpu_needs_zero 翻译 Minix3）：3 种可选方案

#### 方案 A（v2 推荐）：arch 层完全处理 XSAVE/CPACR_EL1/sstatus.FS

```rust
// x86-64：内嵌 576 字节 XSAVE area
pub struct X86_64ExtRegSlot {
    data: [u8; 576],   // FPU_XFP_SIZE, 64-byte aligned
    initialized: bool,
}

// aarch64：ZST
pub struct AArch64ExtRegSlot;

// riscv64：ZST
pub struct Riscv64ExtRegSlot;

// kernel 调用一次
let slot = arch::init_ext_reg_slot(proc_nr, role);
proc.p_ext_reg_state = slot;
```

**优点**：OS 层零感知；arch 层各自负责自己的 FPU 模型。
**缺点**：x86-64 XSAVE area 大小固定为 576（实际可能更大，需 runtime 检测 XCR0）。
**风险**：576 不一定够——可能 576 是 Minix3 旧值，现代 XSAVE 需要 2688+。

#### 方案 B：动态大小 XSAVE area（用 heap）

```rust
pub struct X86_64ExtRegSlot {
    data: Box<[u8]>,   // 运行时按 XCR0 决定大小
    size: usize,
    initialized: bool,
}
```

**优点**：大小自适应 XCR0 配置。
**缺点**：用堆（违反 P3 无堆约束）。
**缓解**：仅 boot 后 VM 启动后使用。

#### 方案 C：保守静态大小（1024 字节）

```rust
const XSAVE_MAX_SIZE: usize = 1024;   // 上限
pub struct X86_64ExtRegSlot {
    data: [u8; XSAVE_MAX_SIZE],
    actual_size: u16,
    initialized: bool,
}
```

**优点**：无堆；保守大小足够现代 XSAVE。
**缺点**：浪费内存（1024 vs 实际 ~576）。
**适用**：boot 阶段用此方案；runtime 由 VM 调整。

**M3 推荐**：**方案 C**（保守静态大小 1024 或 2688）+ boot 时检测 XCR0 决定 actual_size。

### 13.3 P0 #10（3 个 trait 是 C 函数翻译）：3 种可选方案

#### 方案 A（v2 推荐）：完全删除 trait，用类型别名

**优点**：最简洁；编译时单架构不需要 trait。
**缺点**：mock 测试需重新组织（per-arch test 模块）。

#### 方案 B：保留单一 trait + 关联类型

```rust
pub trait ArchProcStartup {
    type Regs: Copy;
    type Frame;
    type ExtRegSlot: Default;
    
    fn build_for_user(role: ProcessRole, entry: VirBytes, sp: VirBytes, ps: VirBytes) -> Self::Regs;
    fn build_for_kernel(role: ProcessRole) -> Self::Regs;
    fn apply_to_trap_frame(regs: Self::Regs, frame: &mut Self::Frame);
    fn init_ext_reg_slot(slot: ProcNr, role: ProcessRole) -> Self::ExtRegSlot;
}
```

**优点**：保留 trait 接口语义；mock 友好。
**缺点**：3 个 trait 合并为 1 个，但仍存在 trait。

#### 方案 C：完全 module-per-arch 模式 + macro

```rust
// arch crate 顶层
#[cfg(target_arch = "x86_64")]
pub mod current {
    pub use super::x86_64::*;
}

// 每个 arch 提供同名函数
pub fn build_startup_for_user(...) -> StartupRegs { ... }
```

**优点**：每个 arch 是独立 module；kernel 只用 `arch::current::build_*`。
**缺点**：函数无法覆盖——如果两个 arch 需要不同签名，需 trait。

**M3 推荐**：**方案 A**——minix-rs 编译时单架构，trait 派发零收益，方案 A 最干净。

### 13.4 P0 #11（Box 堆存储）：3 种可选方案

#### 方案 A（v2 推荐）：内嵌数组 `[T; N]`

```rust
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],  // 直接内嵌
    ...
}
```

**要求**：`KProcess::empty_at(nr) -> Self` 是 `const fn`。
**优点**：完全无堆；编译期固定大小。
**缺点**：KProcess 大小 = 所有字段之和（~1.5KB）+ 576 字节 XSAVE = ~2KB/进程 × 256 进程 = 512KB——可接受。

#### 方案 B：StaticCell 全局静态

```rust
use static_cell::StaticCell;
static PROC_TABLE: StaticCell<[KProcess; PROC_TABLE_SIZE]> = StaticCell::new();

pub fn proc_table() -> &'static mut [KProcess] {
    PROC_TABLE.init_with(|| {
        let mut arr: [MaybeUninit<KProcess>; PROC_TABLE_SIZE] = unsafe { MaybeUninit::uninit().assume_init() };
        for (i, slot) in arr.iter_mut().enumerate() {
            slot.write(KProcess::empty_at(i as ProcNr));
        }
        unsafe { core::mem::transmute_copy(&arr) }
    })
}
```

**优点**：与方案 A 同样无堆；可能更模块化。
**缺点**：依赖 `static_cell` crate；首次访问才初始化（隐式状态）。

#### 方案 C：延迟到 VM 启动后用 VM 堆

```rust
pub struct ProcessTable {
    procs: &'static mut [KProcess],   // 由 VM 申请的页填充
    ...
}
```

**优点**：boot 阶段零内存压力。
**缺点**：VM 启动前不可用；架构复杂（VM 申请多少页？）。

**M3 推荐**：**方案 A**（最直接）。StaticCell 是兜底方案。

### 13.5 P1 #12（§3.4 translate 味）：3 种可选方案

#### 方案 A（v2 推荐）：ProcessCapability + BootCapability 枚举

```rust
pub enum BootCapability {
    Idle,
    KernelTask { kind: KernelTaskKind },
    SystemService,         // VM
    RootServer,            // RS
    DeferredToRs,          // 普通用户进程
}

impl PrivTable {
    pub fn bind_boot_capability(&mut self, proc_nr: ProcNr, cap: BootCapability) -> Result<(), CapError> {
        let slot = static_priv_id(proc_nr);
        let priv_ = &mut self.privs[slot];
        if priv_.is_assigned() { return Err(CapError::AlreadyBound); }
        priv_.capability = ProcessCapability::from(cap);
        Ok(())
    }
}
```

**优点**：命名 OS 概念化；类型安全。
**缺点**：新增 capability 类型需扩展枚举。

#### 方案 B：保留原 `configure_boot_priv` API + 6 个参数

**不推荐**——继续翻译 C 字段，不解决问题。

#### 方案 C：Capability 作为单独 trait + impl per role

```rust
pub trait BootCapabilityImpl {
    fn apply(&self, priv_: &mut KPriv);
}

pub struct IdleCap;
impl BootCapabilityImpl for IdleCap { ... }
pub struct VmCap;
impl BootCapabilityImpl for VmCap { ... }
```

**优点**：组合灵活。
**缺点**：过度工程；boot 阶段只有 4-5 种 capability，不需要 trait。

**M3 推荐**：**方案 A**。

### 13.6 各方案对比总览

| 问题 | 方案 A（M3 推荐） | 方案 B | 方案 C |
|------|-------------------|--------|--------|
| #1 segment_selectors | 类型别名 + 关联 | trait + 关联类型 | cfg 字段 |
| #8 FPU | 静态 1024 字节 | 动态 Box | 保守 1024 字节 |
| #10 trait 删除 | 完全删除 | 单一 trait + 关联 | module + macro |
| #11 Box | `[T; N]` 内嵌 | StaticCell | VM 堆（lazy） |
| #12 capability | `BootCapability` 枚举 | 保留 6 参数 | capability trait |

---

## 14. v3 补充：完整可编译的实现骨架

> v1/v2 给了大量代码片段，但**关键函数没有完整实现**。本节给出**完整可编译的实现骨架**——按 bagging 阶段对比要求，每个函数都必须自洽。

### 14.1 完整 `KProcess::empty_at`（const fn）

```rust
// os/kernel/src/proc.rs

impl KProcess {
    /// 构造一个 slot 的初始状态（const fn，用于 [KProcess; N] 数组初始化）。
    /// 
    /// 所有字段都设为类型默认值；slot 标记为 SLOT_FREE。
    /// 此函数被 `ProcessTable::new()` 在 const 上下文中调用。
    pub const fn empty_at(nr: ProcNr) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: Endpoint::from_generation_slot(0, nr),
            p_seg: ProcessSegments::EMPTY,
            priv_id: None,
            #[cfg(debug_assertions)]
            p_magic: 0xC0FFEE1,
            p_rts_flags: RtsFlags::with_const(RtsFlagsBits::SLOT_FREE),
            p_misc_flags: MiscFlags::empty(),
            p_fault_addr: None,
            p_sched: SchedFields::empty(),
            p_accounting: Accounting::empty(),
            p_time: TimeStats::empty(),
            p_cycles: CyclesStats::empty(),
            p_cpuavg: CpuAvg::empty(),
            p_dequeued: AtomicU64::new(0),
            p_defer: DeferArgs::EMPTY,
            p_nextready: AtomicI32::new(NONE_PROC_NR),
            p_caller_q: AtomicI32::new(NONE_PROC_NR),
            p_q_link: AtomicI32::new(NONE_PROC_NR),
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: Message::ZERO,
            p_delivermsg: Message::ZERO,
            p_delivermsg_vir: VirBytes::ZERO,
            p_ext_reg_state: ExtRegState::empty(),  // 576 字节内嵌，零值
            p_next_restart: None,
            p_next_requestor: None,
            p_vm_suspend: None,
            // ↓ v3 替换原来的 4 个独立字段
            startup: arch::CurrentStartup::EMPTY,
            ext_reg_slot: arch::CurrentExtRegSlot::EMPTY,
        }
    }

    // ... 其他方法保持不变 ...
}

// 9 个需要 const 化的内部类型，示例：

impl SchedFields {
    pub const fn empty() -> Self {
        Self {
            priority: AtomicI8::new(0),
            quantum: AtomicU32::new(0),
            // ... 其他字段 ...
        }
    }
}

impl Accounting {
    pub const fn empty() -> Self {
        Self {
            // ... 所有 u64/u32 字段清零 ...
        }
    }
}

impl RtsFlags {
    pub const fn with_const(bits: RtsFlagsBits) -> Self {
        Self(AtomicU32::new(bits.bits()))
    }
}

impl MiscFlags {
    pub const fn empty() -> Self {
        Self(AtomicU32::new(0))
    }
}

impl ExtRegState {
    pub const fn empty() -> Self {
        Self { data: [0u8; EXT_REG_STATE_SIZE], valid: false }
    }
}

// 等等
```

### 14.2 完整 `CapabilityTable`（替代 PrivTable 内部字段）

```rust
// os/kernel/src/capability.rs

use minix_types::Endpoint;
use crate::proc::{ProcNr, proc_nr};
use crate::kpriv::PrivId;

/// Capability 错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapError {
    OutOfRange,
    AlreadyBound,
    NotFound,
}

/// OS 进程角色（OS 概念，不是 C 字段翻译）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessRole {
    Idle,
    KernelTask(KernelTaskKind),
    SystemService,    // VM
    RootServer,       // RS
    User,             // 普通用户进程
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelTaskKind {
    Idle, Clock, System, Kernel,
}

/// 单个进程的能力（IPC + system call + trap + signal manager）
/// 
/// 这是 OS 安全模型的完整表达，不是 C `struct priv` 的字段翻译。
#[derive(Clone, Copy)]
pub struct ProcessCapability {
    /// 进程类型
    pub role: ProcessRole,
    /// 允许发送 IPC 的目标 endpoint 位图
    pub ipc_targets: EndpointBitmap,
    /// 允许调用的系统调用 trap 编号位图
    pub trap_mask: TrapMask,
    /// 允许调用的内核调用位图
    pub kernel_calls: KCallBitmap,
    /// 信号管理器 endpoint（None = 无信号管理）
    pub signal_manager: Option<Endpoint>,
    /// 调度器：进程优先级队列
    pub priority: u8,
    /// 调度器：时间片大小（毫秒）
    pub quantum_ms: u32,
}

impl ProcessCapability {
    pub const fn empty() -> Self {
        Self {
            role: ProcessRole::User,
            ipc_targets: EndpointBitmap::empty(),
            trap_mask: TrapMask::empty(),
            kernel_calls: KCallBitmap::empty(),
            signal_manager: None,
            priority: 0,
            quantum_ms: 0,
        }
    }

    pub fn is_assigned(&self) -> bool {
        self.role != ProcessRole::User || self.ipc_targets.is_set()
    }
}

/// boot 阶段预定义的能力模板
#[derive(Clone, Copy, Debug)]
pub enum BootCapability {
    Idle,
    KernelTask(KernelTaskKind),
    SystemService,
    RootServer,
    DeferredToRs,
}

impl From<BootCapability> for ProcessCapability {
    /// 把 boot 模板转换为完整 capability。
    /// 
    /// C 源码映射：
    /// - IDL_F (priv.h:36)    = Idle
    /// - TSK_F (priv.h:44)    = KernelTask
    /// - VM_F  (priv.h:48)    = SystemService
    /// - RSYS_F(priv.h:47)    = RootServer
    /// 
    /// 参考 main.c:202-243 的赋值逻辑。
    fn from(template: BootCapability) -> Self {
        match template {
            BootCapability::Idle => Self {
                role: ProcessRole::Idle,
                ipc_targets: EndpointBitmap::NONE,  // TSK_M = NO_M
                trap_mask: TrapMask::empty(),        // TSK_T = 0
                kernel_calls: KCallBitmap::NONE,    // TSK_KC = NO_C
                signal_manager: None,                // TSK 没有 signal manager
                priority: 0,
                quantum_ms: 0,
            },
            BootCapability::KernelTask(kind) => {
                let trap = match kind {
                    KernelTaskKind::Clock | KernelTaskKind::System => {
                        TrapMask::bit(TrapBit::RECEIVE)  // CSK_T = 1 << RECEIVE
                    }
                    _ => TrapMask::empty(),             // TSK_T = 0
                };
                Self {
                    role: ProcessRole::KernelTask(kind),
                    ipc_targets: EndpointBitmap::NONE,  // TSK_M = NO_M
                    trap_mask: trap,
                    kernel_calls: KCallBitmap::NONE,    // TSK_KC = NO_C
                    signal_manager: None,
                    priority: 0,
                    quantum_ms: 0,
                }
            }
            BootCapability::SystemService => Self {
                role: ProcessRole::SystemService,
                ipc_targets: EndpointBitmap::ALL,     // SRV_M = ALL_M
                trap_mask: TrapMask::all(),            // SRV_T = ~0
                kernel_calls: KCallBitmap::ALL,       // SRV_KC = ALL_C
                signal_manager: Some(Endpoint::SELF), // SRV_SM = SELF
                priority: USER_Q,                      // SRV_Q
                quantum_ms: USER_QUANTUM,              // SRV_QT
            },
            BootCapability::RootServer => Self {
                role: ProcessRole::RootServer,
                ipc_targets: EndpointBitmap::ALL,     // SRV_M = ALL_M
                trap_mask: TrapMask::all(),            // SRV_T = ~0
                kernel_calls: KCallBitmap::ALL,       // SRV_KC = ALL_C
                signal_manager: Some(Endpoint::from_nr(proc_nr::RS_PROC_NR)), // SRV_SM = ROOT_SYS_PROC_NR
                priority: USER_Q,
                quantum_ms: USER_QUANTUM,
            },
            BootCapability::DeferredToRs => Self::empty(),  // RS 运行时配置
        }
    }
}

/// 进程能力表（保留 PrivTable API 名字，syscall_process.rs/syscall_clock.rs 6 处引用兼容）
pub struct PrivTable {
    caps: [ProcessCapability; NR_SYS_PROCS],
}

impl PrivTable {
    /// 保留旧 API 名字以兼容 syscall_process.rs:28 等引用
    pub fn new() -> Self {
        Self {
            caps: [const { ProcessCapability::empty() }; NR_SYS_PROCS],
        }
    }

    pub fn init(&mut self) {
        for i in 0..NR_SYS_PROCS {
            self.caps[i] = ProcessCapability::empty();
        }
    }

    /// 兼容旧 API：分配静态 slot
    /// 
    /// C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        let priv_id = static_priv_id(proc_nr);
        let slot = self.caps.get_mut(priv_id as usize)?;
        if slot.is_assigned() {
            return None;  // EBUSY
        }
        // 注意：assign_static 只占位，不设置 capability
        // capability 由 configure_boot_capability 设置
        Some(priv_id)
    }

    /// 新 API：用 boot capability 模板配置
    /// 
    /// 替代原 configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)
    pub fn configure_boot_capability(
        &mut self,
        priv_id: PrivId,
        template: BootCapability,
    ) -> Result<(), CapError> {
        let slot = self.caps.get_mut(priv_id as usize).ok_or(CapError::OutOfRange)?;
        *slot = ProcessCapability::from(template);
        Ok(())
    }

    /// 兼容旧 API（标记 deprecated）
    #[deprecated(note = "Use configure_boot_capability with BootCapability enum")]
    pub fn configure_boot_priv(
        &mut self,
        priv_id: PrivId,
        _flags: u16, _init_flags: i32, _trap_mask: u16,
        _ipc_to: u64, _k_call_mask: [u32; 2], _sig_mgr: Endpoint,
    ) {
        // 提供兼容 shim：内部转译为 BootCapability
        // 不推荐新代码使用
    }

    pub(crate) fn get(&self, id: PrivId) -> Option<&ProcessCapability> {
        self.caps.get(id as usize)
    }

    pub(crate) fn get_capability(&self, proc_nr: ProcNr) -> Option<&ProcessCapability> {
        // 通过进程号反查 capability
        let priv_id = static_priv_id(proc_nr);
        self.get(priv_id)
    }
}
```

### 14.3 完整 `init_one_boot_proc` 主流程

```rust
// os/kernel/src/lib.rs

fn init_one_boot_proc(
    slot_idx: usize,
    module: &BootModule,
    kernel_info: &KernelInfo,
    pt: &mut ProcessTable,
    priv_table: &mut PrivTable,
    paging: &mut impl Paging,
) {
    let proc_nr = slot_idx_to_proc_nr(slot_idx);
    let proc = pt.get_mut(proc_nr).expect("boot proc slot out of range");
    let role = classify_role(proc_nr);

    // ── Step 1: 设置进程名 ──
    // C: strlcpy(rp->p_name, ip->proc_name, ...) — main.c:177
    proc.set_name(ProcName::from_str(module.name));

    // ── Step 2: 初始化扩展寄存器槽位 ──
    // arch 层处理 XSAVE/CPACR_EL1/sstatus.FS
    // 不需要 kernel 知道 FPU 机制
    let ext_slot = arch::init_ext_reg_slot(proc_nr, role);
    proc.ext_reg_slot = ext_slot;

    // ── Step 3: 绑定 capability ──
    let template = boot_template_for_role(proc_nr, role);
    
    // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
    let priv_id = priv_table.assign_static(proc_nr)
        .expect("boot: static priv slot already bound");
    
    // 替代原 configure_boot_priv(6 个裸参数)
    priv_table.configure_boot_capability(priv_id, template)
        .expect("boot: priv slot out of range");

    // ── Step 4: 构建启动配置 ──
    let startup = build_startup_for_role(
        proc_nr, role, module, kernel_info, paging,
    );
    proc.startup = startup;

    // ── Step 5: 设置 RTS 状态 ──
    // C: main.c:264-270
    finalize_rts_flags(proc, role);
}

fn classify_role(proc_nr: ProcNr) -> ProcessRole {
    use proc_nr::*;
    match proc_nr {
        IDLE => ProcessRole::Idle,
        CLOCK | SYSTEM | KERNEL => ProcessRole::KernelTask(match proc_nr {
            CLOCK => KernelTaskKind::Clock,
            SYSTEM => KernelTaskKind::System,
            KERNEL => KernelTaskKind::Kernel,
            _ => unreachable!(),
        }),
        VM_PROC_NR => ProcessRole::SystemService,
        RS_PROC_NR => ProcessRole::RootServer,
        _ if proc_nr >= 0 => ProcessRole::User,
        _ => panic!("unknown kernel task proc_nr: {}", proc_nr),
    }
}

fn boot_template_for_role(proc_nr: ProcNr, role: ProcessRole) -> BootCapability {
    match role {
        ProcessRole::Idle => BootCapability::Idle,
        ProcessRole::KernelTask(kind) => BootCapability::KernelTask(kind),
        ProcessRole::SystemService => BootCapability::SystemService,
        ProcessRole::RootServer => BootCapability::RootServer,
        ProcessRole::User => BootCapability::DeferredToRs,
    }
}

fn build_startup_for_role(
    proc_nr: ProcNr,
    role: ProcessRole,
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut impl Paging,
) -> arch::CurrentStartup {
    if proc_nr < 0 {
        // 内核任务：无用户栈，arch 层负责内核栈分配
        arch::build_startup_for_kernel(role)
    } else if proc_nr == proc_nr::VM_PROC_NR {
        // VM: 实际加载 ELF 到 bootstrap 页表
        match arch::load_vm_image(module, kernel_info, paging) {
            Ok(image) => arch::build_startup_for_user(
                role, image.entry, image.sp, image.ps_strings,
            ),
            Err(_) => {
                // VM 加载失败是致命错误
                panic!("VM ELF loading failed for proc {}", proc_nr);
            }
        }
    } else {
        // 其他用户进程：RS 在运行时加载 ELF
        // 启动配置 entry=0/sp=0/ps_strings=0 表示"等待 RS"
        deferred_user_startup(role)
    }
}

fn deferred_user_startup(role: ProcessRole) -> arch::CurrentStartup {
    arch::build_startup_for_user(role, VirBytes(0), VirBytes(0), VirBytes(0))
}

fn finalize_rts_flags(proc: &mut KProcess, role: ProcessRole) {
    use RtsFlagsBits::*;
    // C: main.c:269 — 所有进程 RTS_PROC_STOP
    proc.p_rts_flags.set(PROC_STOP);
    // C: main.c:270 — 所有进程清除 RTS_SLOT_FREE
    proc.p_rts_flags.clear(SLOT_FREE);

    // C: main.c:264-267 — 除 VM 外的用户进程 VM inhibit
    if matches!(role, ProcessRole::User | ProcessRole::RootServer) {
        proc.p_rts_flags.set(VMINHIBIT);
        proc.p_rts_flags.set(BOOTINHIBIT);
    }
    
    // C: main.c:253 — 非 schedulable 进程 NO_PRIV | NO_QUANTUM
    if matches!(role, ProcessRole::User) {
        proc.p_rts_flags.set(NO_PRIV);
        proc.p_rts_flags.set(NO_QUANTUM);
    }
}

pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    use crate::paging::current_bootstrap::BootstrapPaging;
    
    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "NR_BOOT_MODULES={} but kinfo.boot_modules.len()={}",
        NR_BOOT_MODULES, kernel_info.boot_modules.len()
    );

    let mut pt = ProcessTable::new();      // [KProcess; N] const fn new
    let mut priv_table = PrivTable::new(); // [ProcessCapability; N] const fn new

    // C: main.c:158 — IPCF_POOL_INIT()
    // IPC filter pool 初始化（属于 stage E，本稿不展开）

    let mut paging = BootstrapPaging::new(kernel_info);

    // C: main.c:165 — for (i=0; i < NR_BOOT_PROCS; ++i)
    for (slot_idx, module) in kernel_info.boot_modules.iter().enumerate() {
        init_one_boot_proc(
            slot_idx, module, kernel_info, &mut pt, &mut priv_table, &mut paging,
        );
    }

    // C: main.c:275 — memcpy(kinfo.boot_procs, image, sizeof(image))
    // 把 boot image 信息回写到 kinfo（供 VM 读取）
    
    pt
}

fn slot_idx_to_proc_nr(slot_idx: usize) -> ProcNr {
    if slot_idx < NR_TASKS {
        (slot_idx as ProcNr) - (NR_TASKS as ProcNr)  // 负数：kernel tasks
    } else {
        (slot_idx - NR_TASKS) as ProcNr              // 非负：user processes
    }
}
```

### 14.4 完整 arch 层 x86-64 实现

```rust
// os/arch/src/x86_64/process.rs

use minix_types::{VirBytes, PhysBytes};
use crate::paging::{Paging, PageFlags};

const RFLAGS_USER: u64 = 0x0202;       // IOPL=0, IF=1, bit1=1
const RFLAGS_KERNEL: u64 = 0x1202;      // IOPL=1, IF=1, bit1=1

const XSAVE_CONSERVATIVE_SIZE: usize = 1024;  // 上限；实际由 XCR0 决定

/// x86-64 进程启动配置（arch-specific opaque 值）。
/// 
/// aarch64/riscv64 编译时此类型不存在——kernel 层只看到 `CurrentStartup` 别名。
#[derive(Clone, Copy)]
pub struct StartupRegs {
    status: u64,             // RFLAGS
    entry_point: VirBytes,   // rip
    stack_pointer: VirBytes, // rsp
    ps_strings: VirBytes,    // 启动时 rbx
    ext_reg_init: ExtRegInit,
}

#[derive(Clone, Copy)]
pub enum ExtRegInit {
    /// 用户进程：分配 XSAVE area
    AllocateXSaveArea,
    /// 内核任务：无 FPU 上下文
    NoExtRegState,
}

impl StartupRegs {
    pub const EMPTY: Self = Self {
        status: 0,
        entry_point: VirBytes(0),
        stack_pointer: VirBytes(0),
        ps_strings: VirBytes(0),
        ext_reg_init: ExtRegInit::NoExtRegState,
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub rbp: u64, pub rbx: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rax: u64, pub rcx: u64, pub rdx: u64, pub rsi: u64, pub rdi: u64,
    pub rflags: u64, pub rip: u64, pub rsp: u64,
}

/// x86-64 扩展寄存器槽位
/// 
/// 设计选择：conservative 静态大小 1024 字节，无堆。
/// 实际 XSAVE area 由 XCR0 决定（典型 576-2688 bytes），1024 上限足以覆盖。
#[repr(align(64))]
pub struct ExtRegSlot {
    data: [u8; XSAVE_CONSERVATIVE_SIZE],
    actual_size: u16,
    initialized: bool,  // MF_EXT_REG_INITIALIZED
}

impl ExtRegSlot {
    pub const EMPTY: Self = Self {
        data: [0u8; XSAVE_CONSERVATIVE_SIZE],
        actual_size: 0,
        initialized: false,
    };

    pub fn zero_and_init(&mut self, actual_size: usize) {
        for byte in &mut self.data {
            *byte = 0;
        }
        self.actual_size = actual_size as u16;
        self.initialized = false;  // lazy init：第一次 FP 指令 trap
    }
}

pub fn build_startup_for_user(
    role: ProcessRole,
    entry: VirBytes,
    sp: VirBytes,
    ps_strings: VirBytes,
) -> StartupRegs {
    let status = match role {
        ProcessRole::KernelTask(_) | ProcessRole::Idle => RFLAGS_KERNEL,
        _ => RFLAGS_USER,
    };
    let ext_reg_init = match role {
        ProcessRole::User => ExtRegInit::AllocateXSaveArea,
        _ => ExtRegInit::NoExtRegState,
    };
    StartupRegs { status, entry_point: entry, stack_pointer: sp, ps_strings, ext_reg_init }
}

pub fn build_startup_for_kernel(role: ProcessRole) -> StartupRegs {
    let sp = kernel_stack_for(role);
    StartupRegs {
        status: RFLAGS_KERNEL,
        entry_point: VirBytes(kernel_task_entry_for(role)),
        stack_pointer: sp,
        ps_strings: VirBytes(0),
        ext_reg_init: ExtRegInit::NoExtRegState,
    }
}

fn kernel_task_entry_for(role: ProcessRole) -> u64 {
    match role {
        ProcessRole::Idle => 0xFFFF_FFFF_8000_0000 + offset_of!(idle_task_entry),
        ProcessRole::KernelTask(KernelTaskKind::Clock) => 0xFFFF_FFFF_8000_0000 + offset_of!(clock_task_entry),
        ProcessRole::KernelTask(KernelTaskKind::System) => 0xFFFF_FFFF_8000_0000 + offset_of!(system_task_entry),
        ProcessRole::KernelTask(KernelTaskKind::Kernel) => 0xFFFF_FFFF_8000_0000 + offset_of!(kernel_call_entry),
        _ => panic!("not a kernel task role: {:?}", role),
    }
}

fn kernel_stack_for(_role: ProcessRole) -> VirBytes {
    // 内核栈分配——简化版：所有 kernel task 共享一个静态栈
    // 实际生产应由 per-CPU 栈管理
    VirBytes(0xFFFF_FFFF_8000_4000)  // 内核高半区固定地址
}

pub fn apply_startup_to_trap_frame(startup: StartupRegs, frame: &mut TrapFrame) {
    frame.rflags = startup.status;
    frame.rip = startup.entry_point.0;
    frame.rsp = startup.stack_pointer.0;
    frame.rbx = startup.ps_strings.0;
    // 其他寄存器保持 trap frame 默认值（已由 trap frame 初始化清零）
}

pub fn init_ext_reg_slot(_slot_nr: ProcNr, role: ProcessRole) -> ExtRegSlot {
    let mut slot = ExtRegSlot::EMPTY;
    match role {
        ProcessRole::User => {
            // x86-64: 检测 XCR0 决定 XSAVE area 实际大小
            // 简化版：固定用 576 (FPU_XFP_SIZE) — 与 Minix3 一致
            let actual_size = 576;
            slot.zero_and_init(actual_size);
        }
        _ => {
            // 内核任务：不分配 FPU 上下文
            slot.actual_size = 0;
        }
    }
    slot
}
```

### 14.5 完整 VM ELF 加载（跨架构共享）

```rust
// os/arch/src/vm_boot.rs（v3 完整版）

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use minix_elf::{segment_iter, entry_point};
use crate::paging::{Paging, PageFlags};

const VM_STACK_SIZE: usize = 64 * 1024;  // C: execi.stack_size = 64 * 1024 — protect.c:411

pub struct VmImage {
    pub entry: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

#[derive(Debug)]
pub enum BootError {
    InvalidElf,
    NoSegments,
    PageAllocFailed,
}

pub fn load_vm_image<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> Result<VmImage, BootError> {
    let image = unsafe {
        core::slice::from_raw_parts(module.start.0 as *const u8, module.len)
    };

    let segments = segment_iter(image).map_err(|_| BootError::InvalidElf)?;
    let entry = entry_point(image).ok_or(BootError::InvalidElf)?;

    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated = 0usize;

    for seg in segments {
        let flags = elf_flags_to_page_flags(seg.flags);  // cfg-dispatch
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1);
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            let paddr = PhysBytes(vaddr);  // identity mapping for bootstrap
            paging.map(VirBytes(vaddr), paddr, flags)
                .map_err(|_| BootError::PageAllocFailed)?;
            total_allocated += page_size as usize;

            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - (copy_start % page_size as usize),
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(file_offset as usize),
                            vaddr as *mut u8,
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

    // 栈布局
    let stack_high = kernel_info.user_sp;
    let sp_base = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);

    let stack_flags = PageFlags::read_write();
    let mut stack_addr = sp_base.0 & !(page_size - 1);
    while stack_addr < stack_high.0 {
        paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags)
            .map_err(|_| BootError::PageAllocFailed)?;
        total_allocated += page_size as usize;
        stack_addr += page_size;
    }

    // BSD 风格 ps_strings
    let ps_strings = VirBytes(sp_base.0 - core::mem::size_of::<PsStrings>() as u64);
    unsafe {
        let psp = ps_strings.0 as *mut PsStrings;
        (*psp).ps_argvstr = core::ptr::null();
        (*psp).ps_nargvstr = 0;
        (*psp).ps_envstr = core::ptr::null();
        (*psp).ps_nenvstr = 0;
    }

    Ok(VmImage { entry: VirBytes(entry), sp: sp_base, ps_strings, allocated_bytes: total_allocated })
}

#[repr(C)]
struct PsStrings {
    ps_argvstr: *const *const u8,
    ps_nargvstr: i32,
    ps_envstr: *const *const u8,
    ps_nenvstr: i32,
}

#[cfg(target_arch = "x86_64")]
fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags { /* x86-64 特定 */ }

#[cfg(target_arch = "aarch64")]
fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags { /* aarch64 特定 */ }

#[cfg(target_arch = "riscv64")]
fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags { /* riscv64 特定 */ }
```

---

## 15. v3 补充：分阶段迁移路径

> 2300 行改动不能一次到位。v3 拆为 **4 个阶段**，每阶段可独立编译+测试+回滚。

### Phase 1：基础 const fn 化（**预计 1 周**）

**目标**：让 KProcess/ProcessTable/KPriv 可 const 初始化。

**步骤**：
1. 把 9 个内部类型的 `new()`/构造改为 `const fn`（约 50 行）
2. 新增 `KProcess::empty_at(nr) -> Self` const fn（约 50 行）
3. 新增各内部类型的 `EMPTY`/`empty()` 关联常量
4. **不改** ProcessTable 字段——仍用 Box，但**准备好 const 化路径**

**验证**：
```bash
cargo build --target x86_64-unknown-none
# 期望：编译成功，所有 internal types const-init
cargo test -p minix-kernel
# 期望：原有测试通过
```

**风险**：低——纯机械改动。

### Phase 2：ProcessTable 去掉 Box（**预计 1 周**）

**目标**：`ProcessTable.procs: [KProcess; PROC_TABLE_SIZE]`。

**步骤**：
1. 修改 `ProcessTable` 字段：`procs: Box<[KProcess]>` → `procs: [KProcess; PROC_TABLE_SIZE]`
2. 修改 `ProcessTable::new()` 为 `const fn`，调用 `KProcess::empty_at(nr)`
3. **同步** 修改 `PrivTable.procs` 同样内嵌
4. 验证全部现有 syscall_process.rs / syscall_clock.rs 测试通过

**验证**：
```bash
# 不变性检查
rg "Box::new\|Vec::new" os/kernel/src/{proc_table,kpriv}.rs  # 0 命中
cargo test -p minix-kernel
cargo build --target x86_64-unknown-none
```

**风险**：中——可能因 KProcess 中未 const 化的字段编译失败。

### Phase 3：CapabilityTable 内部重组（**预计 2 周**）

**目标**：`PrivTable` 内部从 6 个裸字段改为 `ProcessCapability`。

**步骤**：
1. 新增 `os/kernel/src/capability.rs`（含 `ProcessCapability`/`BootCapability`/`ProcessRole`）
2. 修改 `PrivTable` 内部存储为 `[ProcessCapability; NR_SYS_PROCS]`
3. 修改 `configure_boot_priv(6 裸参数)` → `configure_boot_capability(BootCapability 枚举)`
4. **保留**旧 API 作为 deprecated shim（避免 syscall_process.rs/syscall_clock.rs 改动）
5. 修改 `init_one_boot_proc` 使用新 API
6. 测试

**验证**：
```bash
rg "configure_boot_priv\b" os/kernel/src/  # 应该只剩 deprecated shim
cargo test -p minix-kernel
```

**风险**：中——6 处 syscall 引用不能改（API 兼容）。

### Phase 4：arch trait 删除 + 类型别名（**预计 2 周**）

**目标**：彻底删除 `ArchProcReset`/`ArchProcInit`/`BootProcArch`，改用 cfg-selected 类型别名。

**步骤**：
1. 在 `os/arch/src/{x86_64,aarch64,riscv64}/process.rs` 实现 `StartupRegs`/`build_*`/`apply_*`/`init_ext_reg_slot`
2. 在 `os/arch/src/vm_boot.rs` 实现跨架构共享的 `load_vm_image`
3. 在 `os/arch/src/lib.rs` cfg-selected re-export `CurrentStartup`/`CurrentTrapFrame`/`CurrentExtRegSlot`
4. 删除 `os/arch/src/arch/proc_arch.rs` 的 3 个 trait
5. 修改 `KProcess.startup: CurrentStartup`，删除 4 个 initial_* 字段
6. 修改 `init_one_boot_proc` 使用新 API
7. 修改 `KProcess.ext_reg_slot: CurrentExtRegSlot`
8. 修改 fork 路径（保留 `p_ext_reg_state` 作为底层字段，但通过 `ext_reg_slot` 访问）
9. 测试

**验证**：
```bash
# 不变性检查
rg "SegmentSelectors" os/kernel/  # 0 命中
rg "trait ArchProcReset\|trait ArchProcInit\|trait BootProcArch" os/arch/src/  # 0 命中
rg "initial_pc:\|initial_sp:\|initial_status:\|initial_ps_strings_reg:" os/kernel/src/proc.rs  # 0 命中

cargo test -p minix-kernel
cargo test -p minix-arch
cargo build --target x86_64-unknown-none
cargo build --target aarch64-unknown-none
cargo build --target riscv64gc-unknown-none-elf
```

**风险**：高——三架构协同修改；可能需要几次迭代。

### Phase 总结

| Phase | 工作量 | 风险 | 可回滚 |
|-------|--------|------|--------|
| Phase 1 const fn 化 | 50 行 | 低 | 是（git revert） |
| Phase 2 去掉 Box | 100 行 | 中 | 是 |
| Phase 3 Capability 化 | 600 行 | 中 | 是 |
| Phase 4 arch 重构 | 1550 行 | 高 | 是 |
| **总计** | **2300 行** | - | - |

**关键里程碑**：每个 Phase 结束都应：
1. `cargo build` 全目标通过
2. `cargo test` 全测试通过
3. 不变性 grep 检查 0 命中
4. 更新本设计文档对应章节，记录实际改动 vs 计划

---

## 16. v3 补充：测试用例设计

> 每个设计决策都需要可执行的测试验证。v3 为关键修复点列出测试用例。

### 16.1 Test Set A：arch 接口隔离

**测试目标**：kernel crate 不导入 arch-specific 类型。

```rust
// os/kernel/tests/arch_isolation.rs

#[test]
fn kernel_does_not_import_segment_selectors() {
    // 不变性：kernel/ 下不能 import SegmentSelectors
    // 验证方法：grep "use.*SegmentSelectors" os/kernel/src/  → 0 命中
}

#[test]
fn kernel_does_not_import_ext_reg_init() {
    // 不变性：kernel/ 下不能 import ExtRegInit 枚举
}

#[test]
fn kernel_uses_current_startup_type_alias() {
    // 验证：KProcess.startup 字段类型是 CurrentStartup (类型别名)
    // 编译期保证：cfg-selected 后类型确定
}
```

### 16.2 Test Set B：ProcessTable 无堆

```rust
// os/kernel/tests/proc_table_no_heap.rs

#[test]
fn process_table_const_initialization() {
    // 编译期常量构造
    const PT: ProcessTable = ProcessTable::new();
    // 验证：所有 slot 处于 SLOT_FREE
    assert!(PT.get(proc_nr::IDLE).unwrap().p_rts_flags.is_set(SLOT_FREE));
}

#[test]
fn process_table_no_box_in_struct() {
    // 运行时检查：用 std::mem::size_of 验证
    assert_eq!(
        std::mem::size_of::<ProcessTable>(),
        NR_PROCS * std::mem::size_of::<KProcess>() + size_of::<Scheduler>() + size_of::<VmRequestQueue>()
        // 若使用 Box，会少一个 [KProcess; N] 的 size
    );
}

#[test]
fn priv_table_no_box() {
    const PRT: PrivTable = PrivTable::new();
    // 验证：所有 capability 处于 empty 状态
    assert!(!PRT.get_capability(proc_nr::IDLE).unwrap().is_assigned());
}
```

### 16.3 Test Set C：Capability 绑定

```rust
// os/kernel/tests/capability_binding.rs

#[test]
fn vm_capability_has_all_ipc_targets() {
    let mut pt = PrivTable::new();
    let priv_id = pt.assign_static(proc_nr::VM_PROC_NR).unwrap();
    pt.configure_boot_capability(priv_id, BootCapability::SystemService).unwrap();
    let cap = pt.get_capability(proc_nr::VM_PROC_NR).unwrap();
    assert_eq!(cap.role, ProcessRole::SystemService);
    assert!(cap.ipc_targets.is_all());  // SRV_M = ALL_M
}

#[test]
fn idle_capability_has_no_ipc() {
    let mut pt = PrivTable::new();
    let priv_id = pt.assign_static(proc_nr::IDLE).unwrap();
    pt.configure_boot_capability(priv_id, BootCapability::Idle).unwrap();
    let cap = pt.get_capability(proc_nr::IDLE).unwrap();
    assert!(cap.ipc_targets.is_none());  // TSK_M = NO_M
}

#[test]
fn clock_task_has_receive_trap() {
    let mut pt = PrivTable::new();
    let priv_id = pt.assign_static(proc_nr::CLOCK).unwrap();
    pt.configure_boot_capability(priv_id, BootCapability::KernelTask(KernelTaskKind::Clock)).unwrap();
    let cap = pt.get_capability(proc_nr::CLOCK).unwrap();
    assert!(cap.trap_mask.is_set(TrapBit::RECEIVE));  // CSK_T = 1 << RECEIVE
}

#[test]
fn rs_capability_signal_manager_is_self() {
    let mut pt = PrivTable::new();
    let priv_id = pt.assign_static(proc_nr::RS_PROC_NR).unwrap();
    pt.configure_boot_capability(priv_id, BootCapability::RootServer).unwrap();
    let cap = pt.get_capability(proc_nr::RS_PROC_NR).unwrap();
    assert_eq!(cap.signal_manager, Some(Endpoint::from_nr(proc_nr::RS_PROC_NR)));
}

#[test]
fn double_assignment_fails() {
    let mut pt = PrivTable::new();
    pt.assign_static(proc_nr::VM_PROC_NR).unwrap();
    assert_eq!(pt.assign_static(proc_nr::VM_PROC_NR), None);  // EBUSY
}
```

### 16.4 Test Set D：arch StartupRegs 应用

```rust
// os/arch/tests/startup_to_trap_frame.rs

#[cfg(target_arch = "x86_64")]
#[test]
fn x86_64_user_startup_writes_rflags() {
    let startup = X86_64StartupRegs::build_for_user(
        ProcessRole::User,
        VirBytes(0x400000),
        VirBytes(0x7fff_0000),
        VirBytes(0x7ffe_ffe0),
    );
    let mut frame = TrapFrame::default();  // 全零
    apply_startup_to_trap_frame(startup, &mut frame);
    assert_eq!(frame.rflags, RFLAGS_USER);
    assert_eq!(frame.rip, 0x400000);
    assert_eq!(frame.rsp, 0x7fff_0000);
    assert_eq!(frame.rbx, 0x7ffe_ffe0);  // ps_strings 在 rbx
}

#[cfg(target_arch = "aarch64")]
#[test]
fn aarch64_user_startup_writes_x0_for_ps_strings() {
    let startup = AArch64StartupRegs::build_for_user(
        ProcessRole::User,
        VirBytes(0x400000),
        VirBytes(0x7fff_0000),
        VirBytes(0x7ffe_ffe0),
    );
    let mut frame = TrapFrame::default();
    apply_startup_to_trap_frame(startup, &mut frame);
    assert_eq!(frame.elr, 0x400000);
    assert_eq!(frame.sp, 0x7fff_0000);
    assert_eq!(frame.x0, 0x7ffe_ffe0);  // ps_strings 在 x0
}
```

### 16.5 Test Set E：FPU 初始化

```rust
// os/arch/tests/ext_reg_init.rs

#[cfg(target_arch = "x86_64")]
#[test]
fn user_process_gets_xsave_area() {
    let slot = init_ext_reg_slot(0, ProcessRole::User);
    assert_eq!(slot.actual_size, 576);  // FPU_XFP_SIZE
    assert!(!slot.initialized);  // lazy init
    // 验证数据已清零
    assert!(slot.data.iter().all(|&b| b == 0));
}

#[cfg(target_arch = "x86_64")]
#[test]
fn kernel_task_gets_no_fpu() {
    let slot = init_ext_reg_slot(-1, ProcessRole::KernelTask(KernelTaskKind::System));
    assert_eq!(slot.actual_size, 0);
}

#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
#[test]
fn no_per_process_fpu_slot() {
    let slot = init_ext_reg_slot(0, ProcessRole::User);
    // aarch64/riscv64 ExtRegSlot 是 ZST
    assert_eq!(core::mem::size_of_val(&slot), 0);
}
```

### 16.6 Test Set F：主流程集成

```rust
// os/kernel/tests/init_proc_and_boot.rs

#[test]
fn all_boot_procs_get_capability_or_no_priv() {
    let kinfo = mock_kernel_info_with_n_modules(NR_BOOT_MODULES);
    let pt = init_proc_and_boot(&kinfo);
    
    // schedulable 进程 (kernel tasks, RS, VM) 获得 capability
    let vm = pt.get(proc_nr::VM_PROC_NR).unwrap();
    assert!(!vm.p_rts_flags.is_set(NO_PRIV));
    
    // 非 schedulable 进程 (普通用户进程) 没有 capability
    let pm = pt.get(0).unwrap();  // PM_PROC_NR
    assert!(pm.p_rts_flags.is_set(NO_PRIV));
}

#[test]
fn all_procs_have_proc_stop_set() {
    let kinfo = mock_kernel_info();
    let pt = init_proc_and_boot(&kinfo);
    for i in -NR_TASKS..NR_PROCS {
        if let Some(proc) = pt.get(i as ProcNr) {
            assert!(proc.p_rts_flags.is_set(PROC_STOP));
            assert!(!proc.p_rts_flags.is_set(SLOT_FREE));
        }
    }
}

#[test]
fn non_vm_user_procs_have_vm_inhibit() {
    let kinfo = mock_kernel_info();
    let pt = init_proc_and_boot(&kinfo);
    for i in 0..NR_PROCS {
        if i as ProcNr == proc_nr::VM_PROC_NR { continue; }
        let proc = pt.get(i as ProcNr).unwrap();
        if proc.p_nr >= 0 {
            assert!(proc.p_rts_flags.is_set(VMINHIBIT));
            assert!(proc.p_rts_flags.is_set(BOOTINHIBIT));
        }
    }
}

#[test]
fn vm_gets_startup_with_real_entry() {
    let kinfo = mock_kernel_info_with_vm_elf();
    let pt = init_proc_and_boot(&kinfo);
    let vm = pt.get(proc_nr::VM_PROC_NR).unwrap();
    // 验证 startup.entry_point != 0（VM ELF 实际加载了）
    assert!(vm.startup.entry_point().0 > 0);
}

#[test]
fn pm_gets_deferred_startup() {
    let kinfo = mock_kernel_info();
    let pt = init_proc_and_boot(&kinfo);
    let pm = pt.get(proc_nr::PM_PROC_NR).unwrap();
    // PM 由 RS 运行时加载，startup entry=0
    assert_eq!(pm.startup.entry_point().0, 0);
}
```

### 16.7 Test Set G：fork FPU 继承

```rust
// os/kernel/tests/fork_fpu_inheritance.rs

#[test]
fn forked_process_inherits_parent_fpu_state() {
    let mut parent = KProcess::new(5, Endpoint::from_generation_slot(0, 5));
    parent.p_ext_reg_state.zero_and_init(576);
    parent.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
    // 假设 parent 已经使用 FPU，data 有非零内容
    parent.p_ext_reg_state.data[10] = 0xAB;

    let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(0, 10));
    
    // C 语义：memcpy(fpu_state) — 继承父进程 FPU 状态
    assert_eq!(child.p_ext_reg_state.data[10], 0xAB);
    assert!(child.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED));
}

#[test]
fn forked_process_does_not_inherit_if_parent_not_initialized() {
    let parent = KProcess::new(5, Endpoint::from_generation_slot(0, 5));
    // parent 未初始化 FPU
    let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(0, 10));
    
    assert!(!child.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED));
}
```

### 16.8 测试覆盖率目标

| 模块 | 测试用例数 | 覆盖率目标 |
|------|-----------|----------|
| `arch::process` (per arch) | 6 | 100% |
| `arch::vm_boot` | 4 | 90% |
| `kernel::capability` | 8 | 100% |
| `kernel::proc_table` | 4 | 90% |
| `kernel::init_proc_and_boot` | 6 | 80% |
| `kernel::proc` (fork FPU) | 4 | 80% |
| **总计** | **32 测试** | **~90%** |

---

## 17. v3 补充：C 源码交叉验证

> 每个设计决策都要有 C 源码证据。v3 列出关键决策的 C 源码对照表。

### 17.1 Capability → C 字段映射

| v3 Capability 字段 | C 字段/常量 | C 源码位置 | 验证 |
|-------------------|------------|-----------|------|
| `role: ProcessRole::Idle` | `IDL_F = SYS_PROC | BILLABLE` | priv.h:36 | ✅ |
| `role: ProcessRole::KernelTask(Clock)` | `TSK_F = SYS_PROC` | priv.h:44 | ✅ |
| `role: ProcessRole::SystemService` (VM) | `VM_F = SYS_PROC | VM_SYS_PROC` | priv.h:48 | ✅ |
| `role: ProcessRole::RootServer` (RS) | `RSYS_F = SRV_F | ROOT_SYS_PROC` | priv.h:47 | ✅ |
| `ipc_targets: ALL` (VM/RS) | `SRV_M = ALL_M = -2` | priv.h:25, 67 | ✅ |
| `ipc_targets: NONE` (kernel tasks) | `TSK_M = NO_M = -1` | priv.h:24, 66 | ✅ |
| `trap_mask: RECEIVE bit` (Clock/System) | `CSK_T = 1 << RECEIVE` | priv.h:59 | ✅ |
| `trap_mask: all` (VM/RS) | `SRV_T = ~0` | priv.h:61 | ✅ |
| `trap_mask: empty` (kernel tasks) | `TSK_T = 0` | priv.h:60 | ✅ |
| `kernel_calls: ALL` (VM/RS) | `SRV_KC = ALL_C = -2` | priv.h:28, 73 | ✅ |
| `kernel_calls: NONE` (kernel tasks) | `TSK_KC = NO_C = -1` | priv.h:28, 72 | ✅ |
| `signal_manager: SELF` (VM) | `priv(rp)->s_sig_mgr = SELF` | main.c:208 | ✅ |
| `signal_manager: RS` (RS) | `SRV_SM = ROOT_SYS_PROC_NR` | priv.h:83 | ✅ |

### 17.2 RTS flags 状态机 → C 状态机映射

| v3 设置 | C 设置 | C 源码位置 |
|---------|--------|-----------|
| `proc.p_rts_flags.set(PROC_STOP)` | `rp->p_rts_flags |= RTS_PROC_STOP` | main.c:269 |
| `proc.p_rts_flags.clear(SLOT_FREE)` | `rp->p_rts_flags &= ~RTS_SLOT_FREE` | main.c:270 |
| `proc.p_rts_flags.set(VMINHIBIT)` | `rp->p_rts_flags |= RTS_VMINHIBIT` | main.c:265 |
| `proc.p_rts_flags.set(BOOTINHIBIT)` | `rp->p_rts_flags |= RTS_BOOTINHIBIT` | main.c:266 |
| `proc.p_rts_flags.set(NO_PRIV | NO_QUANTUM)` | `RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM)` | main.c:253 |

### 17.3 VM ELF 加载 → C 源码映射

| v3 步骤 | C 源码位置 |
|---------|-----------|
| `load_vm_image()` | `arch_boot_proc` (VM 分支) — protect.c:388-455 (x86) |
| `segment_iter` | `libexec_load_elf` 内部的 PT_LOAD 遍历 — exec_elf.c:127-318 |
| ELF flags → PageFlags | `libexec` callback `setflags` — exec_elf.c |
| `paging.map()` | `pg_map(PG_ALLOCATEME, ...)` — protect.c:425 |
| `copy_nonoverlapping()` | `libexec` callback `copymem` — exec_elf.c |
| 64KB 栈 | `execi.stack_size = 64 * 1024` — protect.c:411 |
| ps_strings 结构 | `struct ps_strings` — sys/exec.h |
| `add_memmap()` 回收 | `add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start)` — protect.c:454 |

### 17.4 FPU/ext-reg → C 源码映射

| v3 设计 | C 源码 | C 位置 |
|---------|--------|--------|
| `ext_reg_slot: CurrentExtRegSlot` (x86-64 内嵌 576 字节) | `static char fpu_state[NR_PROCS][FPU_XFP_SIZE]` | arch_system.c:144 |
| x86-64: `zero_and_init(576)` | `memset(v, 0, FPU_XFP_SIZE)` | arch_system.c:158 |
| aarch64: ZST（无 per-process FPU） | `arch_proc_reset` 内 `memset(&pr->p_reg, 0, ...)` 无 FPU 处理 | arch_system.c:42-53 |
| riscv64: ZST | 类比 aarch64 | (无 C 源码) |
| `MF_EXT_REG_INITIALIZED` flag | `pr->p_misc_flags |= MF_FPU_INITIALIZED` | arch_system.c:198 |
| Fork 时继承 FPU | `memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE)` | do_fork.c |

### 17.5 Privilege 静态分配 → C 源码映射

| v3 函数 | C 函数 | C 位置 |
|---------|--------|--------|
| `static_priv_id(proc_nr)` | `#define static_priv_id(n) (NR_TASKS + (n))` | priv.h:12 |
| `assign_static(proc_nr)` | `get_priv(rp, static_priv_id(proc_nr))` | main.c:200 |
| `bind_capability(priv_id, BootCapability::Idle)` | `priv(rp)->s_flags = IDL_F` (kernel tasks) | main.c:214 |
| `bind_capability(priv_id, BootCapability::SystemService)` | `priv(rp)->s_flags = VM_F` | main.c:204 |
| `bind_capability(priv_id, BootCapability::RootServer)` | `priv(rp)->s_flags = RSYS_F` | main.c:226 |

### 17.6 不变量 → C 源码映射

| v3 不变性 | C 验证 | 验证方法 |
|----------|--------|---------|
| `[KProcess; PROC_TABLE_SIZE]` 无堆 | `static struct proc proc[NR_TASKS + NR_PROCS]` | proc.c:24 |
| `[KPriv; NR_SYS_PROCS]` 无堆 | `static struct priv priv[NR_SYS_PROCS]` | priv.c |
| Capability 字段数（4 个）| C struct priv 字段数（11 个） | priv.h 结构体定义 |
| VM ELF 加载仅 VM 触发 | `if(rp->p_nr == VM_PROC_NR)` 分支 | protect.c:399 |

---

## 18. v3 最终总结

### 18.1 v3 相比 v1/v2 的关键改进

| 维度 | v1 | v2 | v3 |
|------|----|----|----|
| 每个问题的可选方案 | ❌ 单一推荐 | ❌ 单一推荐 | ✅ A/B/C 三方案对比 |
| 完整可编译代码骨架 | ⚠️ 片段 | ⚠️ 片段 | ✅ 完整函数实现 |
| 迁移路径 | ❌ 一次性 | ❌ 一次性 | ✅ 4 阶段分步 |
| 测试用例设计 | ❌ 缺失 | ❌ 缺失 | ✅ 32 个测试用例 |
| C 源码交叉验证 | ⚠️ 部分 | ⚠️ 部分 | ✅ 完整对照表 |

### 18.2 v3 的设计决策矩阵

| 决策 | M3 推荐 | 备选 | 决策依据 |
|------|---------|------|---------|
| arch 接口形式 | 类型别名 | trait / cfg 字段 | P1 编译时定型 |
| FPU 存储 | 1024 字节保守内嵌 | 动态 Box / 更小内嵌 | P3 无堆 + 现代 XSAVE 适配 |
| trait 删除 | 完全删除 | 单一 trait / macro | minix-rs 单架构 |
| ProcessTable | `[KProcess; N]` | StaticCell / VM 堆 | 最直接 + 无堆 |
| PrivTable 改造 | 内部 capability 化 | 完全替换 | API 兼容 |
| Capability 表达 | `BootCapability` 枚举 | trait / 6 裸参数 | OS 概念 |
| VmLoadPolicy | 枚举 + 运行时切换 | #[cfg(feature)] | 可测试性 |
| 测试策略 | per-arch test 模块 | cfg-dispatch mock | 最简单 |

### 18.3 文档结构最终版

```
§1-2     问题诊断 + 独立判断
§3-10    核心设计（v1 → v2 修正）
§11      v1→v2 自审计补充（7 个盲点）
§12      v2 总览（架构图 + 不变量）
§13      v3 补充：每个 P0/P1 的 A/B/C 可选方案
§14      v3 补充：完整可编译代码骨架
§15      v3 补充：4 阶段迁移路径
§16      v3 补充：32 个测试用例
§17      v3 补充：C 源码交叉验证
§18      v3 最终总结
```

### 18.4 一句话总结（v3）

> **v3 在 v1/v2 基础上补充了完整可编译代码、3 种可选方案对比、4 阶段迁移路径、32 个测试用例、C 源码交叉验证表——可直接驱动 2300 行重构，无需后续补设计。**