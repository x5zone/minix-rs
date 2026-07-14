# 06-proc-init-boot-proc.md — 独立设计（mini）

> **作者**：mini（独立设计，未参考 06-design-*.md 的其他 AI 答案）
> **状态**：初稿，待多 AI bagging
> **目标**：彻底重写 Ch3（设计决策）+ Ch4（实现详解），不参考任何既有答案，独立给出"OS 与硬件解耦、rewrite 而非 translate、boot 阶段无堆"的 Rust 设计。

---

## 〇、本设计的核心立场（写在最前面）

读完 `06-problem.md` 列出的 12 个问题、当前 Rust 实现、以及 Minix3 C 源码（`proc.c`、`main.c`、`protect.c`、`arch_system.c`），我对本模块的根本判断是：

> **当前实现是一份"对 Minix3 C 源码的逐函数 Rust 翻译 + 多余的 trait 包装 + 无意义的安全声明"**——它既没有捕捉到 Minix3 真实的设计语义，也没有利用 Rust 类型系统表达 OS 概念，更违反了项目自身的 [RECONSTRUCTION-PRINCIPLES.md](../../RECONSTRUCTION-PRINCIPLES.md)（"rewrite 不 translate"）和 [arch_mapping.md](../../arch_mapping.md)（"抽象机制而非描述硬件"）。

本设计因此采取**激进重写**的立场：

1. **删 3 个 trait，换 1 个 trait + 1 个类型别名**。当前 `ArchProcReset` / `ArchProcInit` / `BootProcArch` 三个 trait 全部是 1:1 翻译 C 函数，没有任何"行为不同的实现 + 用作 trait bound"——按 review-patterns 模式 25 全部不必要。
2. **删所有 `Box<[...]>`，换编译期固定大小数组**。当前 `ProcessTable::procs: Box<[KProcess]>` 和 `PrivTable::privs: Box<[KPriv]>` 在 boot 阶段无法分配（kernel 没有 `GlobalAlloc`），且 `KProcess` 全部字段都是 const-init 可行的——纯属用 Rust 时的过度设计。
3. **删 4 个 `initial_*` 字段，换 1 个 `boot_context: CurrentBootContext`**。`initial_pc/sp/ps_strings_reg/status` 把 arch 返回值拆成 4 个 OS 层字段，再让 kernel 通过 setter 写回——既破坏 §3.1 自称的"纯函数式"，又把 x86-64 特有的字段（`fpu_needs_zero` 实际被丢弃、`initial_status` 被 kernel 改 RFLAGS）泄漏到 OS 层。
4. **删 FPU/段选择子的所有 OS 层痕迹**。`fpu_needs_zero: bool` 是翻译 Minix3 `fnsave/fxrstor` 的过时模型，现代 x86-64 用 XSAVE 懒加载、aarch64/riscv64 用 CPACR_EL1.FPEN/sstatus.FS——OS 不应知道这些。"OS 类型 = 编译时定型的类型"原则（problem.md P1 原则）要求它们不出现。
5. **6 个裸参数 `configure_boot_priv(6 args)` 换为 1 个 `ProcessCategory` 枚举**。这是经典的"翻译 C 字段"反模式（模式 16），应该用 OS 概念（"这是 VM 进程" / "这是根系统服务"）驱动，而非 6 个 C 字段的镜像。
6. **删 3 个 arch 重复的 `load_vm_elf`，换 1 个 ELF 加载自由函数**。三架构的 ELF 加载代码 100% 相同（`grep -A 5 "fn load_vm_elf" os/arch/src/*/proc_arch.rs` 对比）——把它放在 trait 里纯属假抽象。
7. **KPriv 的 30+ 裸字段按 OS 语义重新分组成 5-6 个子结构**（capability / signals / io / mem / irq / grant），并增加 newtype（`TrapMask` / `SyscallBitmap` / `IpcBitmap`）替代裸 `u16`/`u32`/`u64`。

下面展开。

---

## 一、问题全景（独立于 problem.md 的视角）

### 1.1 problem.md 已列的 12 个问题（同意并采纳）

| # | 问题 | 严重度 | 本设计采纳的解决方向 |
|---|------|-------|-------------------|
| #1 | `InitialRegState` 含 `segment_selectors`/`fpu_needs_zero`（x86-64 泄漏） | P0 | 改用 `BootContext` 关联类型 + `apply_to_trap_frame` |
| #2 | `InitialRegs`/`VmLoadResult` 命名混淆 | P1 | 统一为 `BootEntry` + `ps_strings: VirBytes` |
| #3 | kernel setter 拆解 arch 返回值 | P1 | kernel 不拆解，整体存储 `boot_context` |
| #4 | `InitialRegs` 拆 3 字段 | P1 | 合并到 `BootContext` |
| #5 | `initial_status` 字段名泄漏 | P1 | 彻底下沉到 x86-64 `BootContext` 内部 |
| #6 | 文档 §3.2 自称"OS-semantic"但违反 | P2 | 文档重写时严格遵守 |
| #7 | `SegmentSelectors::default()` 哨兵值 | P2 | 该类型从 OS 层消失 |
| #8 | `fpu_needs_zero` 翻译 + 因果链编造 | P0 | OS 层不出现任何 FPU 概念 |
| #9 | "init_regs 内部调用 reset" 编造 | P0 | 合并为单 trait 方法，无顺序耦合 |
| #10 | 3 个 trait 全是 C 函数翻译 | P0 | 合并为 1 个 trait |
| #11 | `Box<[KProcess]>` 用堆但无 `GlobalAlloc` | P0 | 改 `[KProcess; N]` + `const fn new` |
| #12 | §3.4 全文 translate 味 | P1 | 重写为 OS 概念驱动 |

### 1.2 problem.md 漏掉的、或我独立发现的问题

| # | 问题 | 严重度 | 来源 |
|---|------|-------|------|
| **#13** | `p_ext_reg_state: ExtRegState` (576B) 在 `KProcess` 中 | P0 | grep `os/kernel/src/proc.rs:801` — 144KB 静态浪费，且 aarch64/riscv64 完全不需要 |
| **#14** | `KPriv` 30+ 裸字段（`s_k_call_mask: [u32; 2]`、`s_ipc_to: u64` 等） | P1 | 阅读 `os/kernel/src/kpriv.rs:130-180` 全部裸字段 |
| **#15** | `SYS_CALL_MASK_SIZE = 2` 是 BITMAP_CHUNKS 常量，但 hardcoded | P1 | `os/kernel/src/kpriv.rs:24` 注释 `BITMAP_CHUNKS(NR_SYS_CALLS) = 2` 但没引用宏 |
| **#16** | `set_boot_initial_reg_state` 接受 `_fpu_needs_zero: bool` 然后丢弃 | P0 | `os/kernel/src/proc.rs:1144` 显式 `let _ = _fpu_needs_zero;` |
| **#17** | `syscall_device.rs` 直接 `target.initial_status \|= X86_64_IOPL_BITS` 改 RFLAGS | P0 | `os/kernel/src/syscall_device.rs:484, 520, 523` — OS 层直接操作 x86-64 硬件标志 |
| **#18** | KProcess 字段 `p_sendmsg`/`p_delivermsg: Message`（32B × 2 × 256 = 16KB），但 `p_ext_reg_state` 576B 比 IPC 消息大 18 倍 | P1 | 计算 size，对比 IPC vs FPU 内存预算 |
| **#19** | `SchedFields::new()` 用 `AtomicI8::new(priority::USER_Q)` 但 `priority::USER_Q` 不是 const | P0 | 当前无法 `const fn new` 构造完整 KProcess（problem.md 11.8 确认） |
| **#20** | `#[cfg(feature = "mock")]` 在 `lib.rs:802` 走 `MockPaging` 加载 VM ELF，**但生产模式 PC=0/SP=0**——VM 实际跑不起来 | P0 | `os/kernel/src/lib.rs:822-826` 注释"DEFERRED"，boot 路径上 VM 永远是 PC=0 |
| **#21** | 文档 §4.6 漏讲 KProcess 30+ 字段和 KPriv 30+ 字段——这是项目最严重的设计文档缺失 | P0 | 阅读 `os/kernel/src/proc.rs` 全文 + `kpriv.rs` 全文 |
| **#22** | `idempotent_priv_id: u16` 命名反义：实际是 `static_priv_id` 翻译 C 宏 `static_priv_id(n) = NR_TASKS + n` | P2 | `os/kernel/src/kpriv.rs:91` 注释正确但函数名仍可商榷 |
| **#23** | 三个 arch `load_vm_elf` 100% 相同代码（约 100 行 × 3） | P0 | 横向对比 `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs` 三个文件 |
| **#24** | `mock::MockPaging` 在生产配置下被使用（`lib.rs:802` `#[cfg(feature = "mock")]` 实际是默认开启），所以"未实现"被静默掩盖 | P0 | `Cargo.toml` feature 状态 + `lib.rs:802` `#[cfg(feature = "mock")]` 块 |
| **#25** | `minix-elf::segment_iter` 解析 ELF 后用 `paging.map(vaddr, paddr=vaddr, flags)` 做 1:1 映射——但 `paddr=vaddr` 是用户态假设，内核物理地址与虚拟地址不同 | P1 | 阅读 `x86_64/proc_arch.rs:171-182` |

> **说明**：#13-#25 是我在阅读 problem.md 之后**独立发现**的问题（未参考其他 AI 答案）。其中 #16、#17、#20、#24 是"当前代码根本跑不通"的根因，#21 是"项目最严重的设计文档缺失"，#23 是"假抽象的典型例子"。

---

## 二、设计目标与最高原则

### 2.1 三大最高原则（继承 problem.md，强化表述）

#### 原则 A：OS 与硬件解耦（编译时定型）

```
判别口诀：看到这个字段，问"aarch64 编译时，这个字段有值吗？有意义吗？"
         → 没有 / 没意义 → 违规，必须通过关联类型/类型别名下沉
```

**实施**：
- 所有 arch-specific 类型用 `cfg` + 关联类型 / 类型别名
- OS 层不出现 `SegmentSelectors`、`fpu_needs_zero`、RFLAGS IOPL 位等
- x86-64 编译后只有 `X86_64BootContext`，aarch64 编译后只有 `Aarch64BootContext`——类型系统中没有"通用 BootContext"

#### 原则 B：Rewrite 而非 Translate（概念驱动）

```
判别口诀：看到这个 API，问"它对应哪个 C 函数？"——如果答案是第一步想到的，就是 translate
         应该问"在 OS 概念上，这是做什么的？"
```

**实施**：
- 命名来自 OS 概念：`Capability`（不是 `s_flags` 字段）、`ProcessCategory`（不是 `priv_flag_set`）、`BootContext`（不是 `InitialRegState`）
- 6 个裸参数打包为枚举或子结构
- 不再有"对应 C: get_priv(rp, static_priv_id(proc_nr))"这种注释

#### 原则 C：Boot 阶段无堆

```
判别口诀：在 init_proc_and_boot() 执行路径上 grep "Box\|Vec\|String" → 任何结果 = P0
```

**实施**：
- `ProcessTable: [KProcess; PROC_TABLE_SIZE]` 编译期固定大小
- `PrivTable: [KPriv; NR_SYS_PROCS]` 编译期固定大小
- 所有字段 const-fn 可构造
- 整个 boot 路径只使用栈/全局静态内存

### 2.2 Trait 最小化原则

**判定标准**（来自 review-patterns 模式 25）：
- ≥2 个**行为不同**的实现 + 被用作 trait bound → ✅ 保留 trait
- 任一不满足 → ❌ 删除 trait

**应用**：
| 候选 | 行为不同实现? | 用作 bound? | 判定 |
|------|------------|-----------|------|
| `ArchProcessInit`（合并的 trait）| ✅（3 个 arch）| ✅（`CurrentArch: ArchProcessInit` 隐式 bound）| ✅ 保留 |
| `ArchProcReset`（独立）| ❌（仅 status 常量不同，trait 方法签名相同）| ❌ | ❌ 删除 |
| `ArchProcInit`（独立）| ❌（仅寄存器名不同，已被 BootContext 吸收）| ❌ | ❌ 删除 |
| `BootProcArch::load_vm_elf` | ❌（三 arch 100% 相同）| ❌ | ❌ 改为自由函数 |

### 2.3 机制抽象原则

来自 [arch_mapping.md](../../arch_mapping.md)：

```
我们不描述硬件，我们只抽象机制。
OS 需要硬件提供什么机制，抽象出 trait，然后硬件实现这些 trait。
```

**本模块的机制抽象**：
| 机制 | Trait | OS 概念 |
|------|-------|--------|
| 创建新进程的初始执行上下文 | `ArchProcessInit` | `BootContext` |
| 应用执行上下文到 trap frame | 同上（trait 方法）| `apply_to_trap_frame` |
| 物理页映射 | `Paging` (已存在) | `paging.map` |
| ELF 段解析 | 无 trait，使用 `minix-elf` 库 | `load_elf_into_paging` |

**不抽象的"机制"**（避免假抽象）：
- ❌ 不为"privilege 操作"专门抽 trait——`PrivTable` 是数据 + 内置方法，不是 mechanism
- ❌ 不为"进程表查找"专门抽 trait——`&[KProcess]` 索引即可

---

## 三、模块结构（新）

### 3.1 文件布局

```
os/arch/src/arch/
├── proc_init.rs              # trait ArchProcessInit + type alias CurrentBootContext
└── proc_init/
    ├── x86_64.rs             # struct X86_64BootContext + impl
    ├── aarch64.rs            # struct Aarch64BootContext + impl
    └── riscv64.rs            # struct Riscv64BootContext + impl

os/kernel/src/
├── proc.rs                   # struct KProcess (含 boot_context: CurrentBootContext)
├── proc_table.rs             # struct ProcessTable { procs: [KProcess; N] }
├── kpriv.rs                  # struct KPriv (按 capability/signals/io/mem/irq 分组)
│                             # struct PrivTable { privs: [KPriv; N] }
├── elf_boot.rs               # pub fn load_elf_into_paging<P: Paging>(...) -> Result<BootEntry>
└── lib.rs                    # pub fn init_proc_and_boot() (主流程)
```

### 3.2 依赖关系

```
                       minix-types
                           │
                           ▼
   os/arch/src/arch/proc_init.rs  ◄──── os/kernel/src/lib.rs
           │                                    │
           ▼                                    ▼
os/arch/src/arch/proc_init/x86_64.rs     os/kernel/src/proc.rs
                                          os/kernel/src/kpriv.rs
                                          os/kernel/src/elf_boot.rs
                                          os/kernel/src/proc_table.rs
                                          os/kernel/src/lib.rs
                                          （init_proc_and_boot）
```

**关键约束**：`os/arch` 不依赖 `os/kernel`（保持分层；arch trait 永远不需要 kernel 类型）。`os/kernel` 依赖 `os/arch`（通过 `CurrentBootContext` 类型别名）。

---

## 四、核心类型定义

### 4.1 `BootContext`：arch-specific 透明值

```rust
// os/arch/src/arch/proc_init.rs

use minix_types::VirBytes;

/// 进程启动的"完整执行上下文"——arch-specific 透明值。
///
/// **OS 概念**：新进程在第一次被调度前，需要的所有硬件相关状态。
/// **OS 不关心**这个值的内部字段——只看 trait 方法。
///
/// **编译时定型**：
/// - x86-64 编译后 `CurrentBootContext == X86_64BootContext`
/// - aarch64 编译后 `CurrentBootContext == Aarch64BootContext`
/// - riscv64 编译后 `CurrentBootContext == Riscv64BootContext`
///
/// 关键性质：`Copy + 'static`，可存入 `KProcess` 而无堆分配。
pub trait ArchProcessInit: Copy + 'static {
    /// arch-specific 上下文类型，由各架构在 `proc_init/<arch>.rs` 中定义。
    type Context: Copy + 'static;

    /// 构造新进程的启动上下文。
    ///
    /// # Arguments
    /// - `category`: 进程类别（kernel task / system service / user），OS 概念
    /// - `entry`: 进程的入口点（PC/SP/ps_strings），None 表示 arch 用默认值
    ///   （如 kernel task 启动后跳到 arch-specific idle loop）
    fn make_context(category: ProcessCategory, entry: Option<BootEntry>) -> Self::Context;

    /// 将启动上下文应用到 trap frame。
    ///
    /// arch 层负责所有硬件相关写入：RFLAGS/SP 选择子/XSAVE area/CPACR_EL1/sstatus.FS。
    /// OS 层只传入上下文 + trap frame，不关心内部步骤。
    fn apply_to_trap_frame(ctx: &Self::Context, frame: &mut TrapFrame);
}

/// 当前 arch 的启动上下文类型——编译时定型，OS 代码直接用这个别名。
#[cfg(target_arch = "x86_64")]
pub type CurrentBootContext = x86_64::X86_64BootContext;

#[cfg(target_arch = "aarch64")]
pub type CurrentBootContext = aarch64::Aarch64BootContext;

#[cfg(target_arch = "riscv64")]
pub type CurrentBootContext = riscv64::Riscv64BootContext;
```

**为什么不需要 `ArchProcReset` / `ArchProcInit` / `BootProcArch` 三个 trait**：

| 旧 trait | 旧职责 | 新对应 |
|---------|-------|--------|
| `ArchProcReset::initial_reg_state` | 返回 status + 段选择子 + fpu_needs_zero | `ArchProcessInit::make_context` 一并返回（arch 内部消化 FPU/段）|
| `ArchProcInit::init_regs` | 返回 PC/SP/ps_strings_reg | 同一接口，entry: Option<BootEntry> |
| `BootProcArch::load_vm_elf` | ELF 加载 | **删除**，改为 `os/kernel/src/elf_boot.rs` 自由函数 |

**用户期待的"纯 OS code"（来自 problem.md #10.2）**正是这个：kernel 层只有 `CurrentBootContext`（一个具体类型别名），没有 `T: ArchProcXxx` bound。

### 4.2 `ProcessCategory`：OS 概念驱动

```rust
// os/arch/src/arch/proc_init.rs (与 BootContext 同模块，OS 概念)

/// 进程类别——OS 概念，arch 层根据它选择不同的启动策略。
///
/// **不**对应 C 的 `priv_flag_set`（那是 C 字段的翻译）。
/// **对应** OS 概念："这是哪种进程"——arch 决定该用哪种状态寄存器/段选择子/特权级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessCategory {
    /// 内核任务（如 CLOCK/SYSTEM/KERNEL/IDLE）。
    /// 运行在最高特权级，无 IPC 能力，无 FPU 用户态。
    KernelTask { is_idle: bool },
    /// 系统服务（如 VM/RS）。
    /// 运行在最高特权级，有完整 IPC 能力，按需 FPU 懒加载。
    SystemService,
    /// 用户进程（INIT 及其后代）。
    /// 运行在最低特权级，IPC 能力受限，FPU 懒加载。
    User,
}

/// 进程的入口点（PC/SP/ps_strings）。
///
/// OS 概念：进程从哪里开始执行、用什么栈。
/// arch 层决定这些值具体写入哪些寄存器（x86-64: rbx 是 ps_strings，
/// aarch64: r0 是 ps_strings 等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootEntry {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
}
```

### 4.3 `KProcess::boot_context` 字段

```rust
// os/kernel/src/proc.rs (字段定义)

// 旧代码（4 个分离字段）:
// pub initial_pc: VirBytes,
// pub initial_sp: VirBytes,
// pub initial_ps_strings_reg: u64,
// pub initial_status: u64,

// 新代码（1 个透明上下文）:
/// 进程的启动上下文，arch-specific 透明值。
///
/// **何时设置**：`init_proc_and_boot()` 创建进程时设置一次。
/// **何时读取**：调度器第一次切换到该进程时（设置 trap frame）。
/// **OS 不解释内部字段**——只整体存储、整体应用。
pub boot_context: CurrentBootContext,
```

**为什么是 `CurrentBootContext`（具体类型别名）而不是泛型**：
- 整个 kernel crate 编译时只有一个 arch——`CurrentBootContext` 是具体类型，无泛型开销
- 不需要 `T: ArchProcessInit` bound（`CurrentBootContext` 已经是具体类型）
- arch 切换通过 `cfg` 实现，零运行时成本（符合 [arch_mapping.md §2](../../arch_mapping.md) 的"静态分发"要求）

### 4.4 旧 4 个字段处理

| 旧字段 | 新处理 |
|-------|-------|
| `initial_pc` | 进入 `CurrentBootContext` 内部（具体哪个字段由 arch 决定）|
| `initial_sp` | 同上 |
| `initial_ps_strings_reg` | 同上（命名也消失——OS 层不叫它"寄存器"）|
| `initial_status` | 同上，且 **x86-64 不再被 kernel 改 RFLAGS**（IOPL 操作下沉到 arch）|

**RFLAGS IOPL 操作的迁移**：

旧代码 (`os/kernel/src/syscall_device.rs:484-523`)：
```rust
target.initial_status |= X86_64_IOPL_BITS;  // 改 OS 层字段
```

新方案（方案 A：彻底下沉）：
```rust
// syscall_device.rs 调用 arch trait：
X86_64Arch::enable_iopl_for_process(&target);
// 内部修改 target.boot_context 内部字段（如果 arch 选择这样）
// 或：修改时直接写 trap frame 寄存器
```

新方案（方案 B：用 typed API 包装）：
```rust
// kernel 写：
let mut ctx = target.boot_context;
X86_64Arch::set_iopl(&mut ctx, IoplLevel::Ring3);
target.boot_context = ctx;
```

> **取舍**：本设计选择**方案 A**（彻底下沉到 arch trait 方法），因为 IOPL 是 x86-64 特有的概念，OS 层不应知道"RFLAGS 有 IOPL 位"。

### 4.5 `p_ext_reg_state: ExtRegState` 删除

旧代码（`os/kernel/src/proc.rs:801`）：
```rust
pub p_ext_reg_state: ExtRegState,  // 576 字节/进程 × 256 = 144KB
```

**删除理由**：
1. **aarch64/riscv64 完全不需要**：这两个架构没有"进程 FPU 保存区"概念
2. **x86-64 也不需要"创建时清零"**：XSAVE 懒加载，CR0.TS 机制
3. **576 字节是历史包袱**：Minix3 的 `FPU_XFP_SIZE` 对应 `fxsave` 区域（512 字节），modern XSAVE 区域大小由 XCR0 决定（最大 ~2.5KB）
4. **OS 类型 = 编译时定型原则**：aarch64 编译后 `ExtRegState` 类型不应存在

**新方案**：
- x86-64: `X86_64BootContext` 内部有 `xsave_area: MaybeUninit<[u8; MAX_XSAVE_SIZE]>` 字段
- aarch64: `Aarch64BootContext` 不含 FPU 字段
- riscv64: 同 aarch64
- `MF_EXT_REG_INITIALIZED` 标志下沉到 x86-64 `BootContext` 内部

### 4.6 `KPriv` 重组（按 OS 语义分组）

旧代码（`os/kernel/src/kpriv.rs:130-180`）—— 30+ 裸字段：
```rust
pub(crate) struct KPriv {
    pub(crate) s_proc_nr: Option<ProcNr>,
    pub(crate) s_id: SysId,
    pub(crate) s_flags: PrivFlagsBits,  // raw u16
    pub(crate) s_init_flags: i32,
    pub(crate) s_asyntab: u64,
    pub(crate) s_asynsize: usize,
    pub(crate) s_asynendpoint: Endpoint,
    pub(crate) s_trap_mask: u16,        // raw u16
    pub(crate) s_ipc_to: u64,           // raw u64
    pub(crate) s_k_call_mask: [u32; 2], // raw [u32; 2]
    pub(crate) s_sig_mgr: Endpoint,
    pub(crate) s_bak_sig_mgr: Endpoint,
    pub(crate) s_notify_pending: u64,
    pub(crate) s_asyn_pending: u64,
    pub(crate) s_int_pending: u32,
    pub(crate) s_sig_pending: SigSet,
    pub(crate) s_ipcf: Option<usize>,
    pub(crate) s_alarm_timer: Option<crate::clock::TimerEntry>,
    pub(crate) s_stack_guard: Option<usize>,
    pub(crate) s_diag_sig: bool,
    pub(crate) s_nr_io_range: i32,
    pub(crate) s_io_tab: [IoRange; NR_IO_RANGE],
    pub(crate) s_nr_mem_range: i32,
    pub(crate) s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub(crate) s_nr_irq: i32,
    pub(crate) s_irq_tab: [i32; NR_IRQ],
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}
```

**新设计**（按 OS 语义分组）：

```rust
// os/kernel/src/kpriv.rs

use minix_types::Endpoint;
use crate::proc::{ProcNr, SigSet};
use crate::clock::TimerEntry;

/// 系统进程的能力——OS 概念。
///
/// **对应 C**：`s_flags` + `s_ipc_to` + `s_k_call_mask` + `s_trap_mask` + `s_sig_mgr`。
/// **OS 视角**：一个进程能做什么（什么系统调用、什么 IPC 目标、什么异常）。
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    /// 进程类别（kernel task / system service / user）—— 决定 flags 默认值
    pub category: ProcessCategory,
    /// 进程可以调用的系统调用位图。
    /// x86-64 NR_SYS_CALLS = 64，故用 [u32; 2]（实际可由 BITMAP_CHUNKS 宏计算）。
    pub syscalls: SyscallBitmap,
    /// 进程可以发送 IPC 的目标位图。
    pub ipc_targets: IpcBitmap,
    /// 进程可以调用的 kernel call 位图。
    pub k_calls: KCallBitmap,
    /// 进程可以接收的异常位图。
    pub trap_mask: TrapMask,
    /// 信号管理员。
    pub signal_manager: SignalManager,
}

/// 系统进程的信号状态。
#[derive(Debug, Clone, Copy)]
pub struct SignalState {
    pub manager: SignalManager,
    pub bak_manager: SignalManager,
    pub notify_pending: u64,
    pub asyn_pending: u64,
    pub int_pending: u32,
    pub sig_pending: SigSet,
    pub alarm_timer: Option<TimerEntry>,
}

/// 系统进程的 I/O 端口访问权限。
#[derive(Clone, Copy)]
pub struct IoAccess {
    pub ranges: [IoRange; NR_IO_RANGE],
    pub nr_ranges: i32,
}

/// 系统进程的内存范围访问权限。
#[derive(Clone, Copy)]
pub struct MemAccess {
    pub ranges: [MemRange; NR_MEM_RANGE],
    pub nr_ranges: i32,
}

/// 系统进程的中断权限。
#[derive(Clone, Copy)]
pub struct IrqAccess {
    pub tabs: [i32; NR_IRQ],
    pub nr_irq: i32,
}

/// 系统进程的 grant table / state table 引用。
#[derive(Debug, Clone, Copy, Default)]
pub struct TableRef {
    pub table: usize,
    pub entries: i32,
    pub endpoint: Endpoint,
}

/// 异步通知表。
#[derive(Debug, Clone, Copy, Default)]
pub struct AsyncTable {
    pub table: u64,
    pub size: usize,
    pub endpoint: Endpoint,
}

/// 系统进程结构（KPriv）。
#[derive(Clone, Copy)]
pub struct KPriv {
    /// OS 识别：哪个 proc 拥有这个 priv 槽
    pub owner: Option<ProcNr>,
    /// Priv 槽的系统 ID（debugging 用）
    pub id: SysId,
    /// 启动时的 init_flags
    pub init_flags: i32,
    /// 进程能力
    pub capability: Capability,
    /// 信号状态
    pub signals: SignalState,
    /// I/O 访问
    pub io: IoAccess,
    /// 内存范围访问
    pub mem: MemAccess,
    /// 中断权限
    pub irq: IrqAccess,
    /// Grant table 引用
    pub grant: TableRef,
    /// State table 引用
    pub state: TableRef,
    /// 异步通知表
    pub async_table: AsyncTable,
    /// IPC filter 引用（运行时使用）
    pub ipc_filter: Option<usize>,
    /// 栈保护
    pub stack_guard: Option<usize>,
    /// Diagnostic signal 标志
    pub diag_sig: bool,
}
```

**优势**：
- 按 OS 语义分组（capability / signals / io / mem / irq / grant）
- 6 个裸参数（`s_flags`/`s_trap_mask`/`s_ipc_to`/`s_k_call_mask`/`s_sig_mgr`/`s_init_flags`）打包到 `Capability` 单参数
- newtype：`SyscallBitmap`/`IpcBitmap`/`KCallBitmap`/`TrapMask`/`SignalManager` 替代裸类型
- `AsyncTable` 独立成块（之前散在 3 个字段）

### 4.7 `PrivTable::configure_for()`：按类别而非 6 裸参数

旧代码（`os/kernel/src/kpriv.rs:298-`）：
```rust
pub fn configure_boot_priv(
    &mut self, priv_id: PrivId, flags: PrivFlagsBits,
    init_flags: i32, trap_mask: u16, ipc_to: u64,
    k_call_mask: [u32; 2], sig_mgr: Endpoint,
) { ... }
```

新设计：
```rust
// os/kernel/src/kpriv.rs

impl PrivTable {
    /// 根据进程类别配置 capability。
    ///
    /// **OS 概念**：这是 VM 进程 / 根系统服务 / 用户进程 / IDLE 任务……所以它有这些能力。
    /// **不**对应 C 的 6 字段赋值（problem.md #12 修复）。
    pub fn configure_for(&mut self, priv_id: PrivId, category: ProcessCategory, signal_manager: SignalManager) {
        let priv_ = &mut self.privs[priv_id as usize];
        let cap = &mut priv_.capability;
        cap.category = category;

        // 根据类别设置能力默认值
        match category {
            ProcessCategory::KernelTask { is_idle } => {
                cap.syscalls = SyscallBitmap::empty();
                cap.ipc_targets = IpcBitmap::empty();
                cap.k_calls = KCallBitmap::empty();
                cap.trap_mask = if is_idle { TrapMask::empty() } else { TrapMask::kernel_task_default() };
            }
            ProcessCategory::SystemService => {
                cap.syscalls = SyscallBitmap::all();
                cap.ipc_targets = IpcBitmap::all();
                cap.k_calls = KCallBitmap::all();
                cap.trap_mask = TrapMask::service_default();
            }
            ProcessCategory::User => {
                cap.syscalls = SyscallBitmap::empty();
                cap.ipc_targets = IpcBitmap::empty();
                cap.k_calls = KCallBitmap::empty();
                cap.trap_mask = TrapMask::empty();
            }
        }
        priv_.signals.manager = signal_manager;
    }
}
```

**调用方**（`init_proc_and_boot`）：
```rust
// 旧：priv_table.configure_boot_priv(priv_id, priv_flag_set::VM_F, 0, 0, IPC_TO_ALL, K_CALL_MASK_ALL, ...);
// 新：
priv_table.configure_for(priv_id, ProcessCategory::SystemService, SignalManager::self_ref(nr));
```

**优势**：
- 6 裸参数 → 1 个 `ProcessCategory` 枚举
- 不再有"对应 C: main.c:202-243"的翻译味
- 用户能直接读懂"这是 system service"——而不需要查 `priv_flag_set` 是 `VM_F` 还是 `RSYS_F`

### 4.8 newtype 包装

```rust
// os/kernel/src/types.rs (新文件) 或 kpriv.rs 顶部

/// 异常掩码（OS 概念：进程能接收哪些异常）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrapMask(u16);

/// 系统调用位图（OS 概念：进程能调用哪些 syscall）。
#[derive(Debug, Clone, Copy)]
pub struct SyscallBitmap([u32; SYS_CALL_MASK_SIZE]);

/// IPC 目标位图（OS 概念：进程能给谁发 IPC）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcBitmap(u64);

/// Kernel call 位图（OS 概念：进程能调用哪些 kernel call）。
#[derive(Debug, Clone, Copy)]
pub struct KCallBitmap([u32; SYS_CALL_MASK_SIZE]);

/// 信号管理员（OS 概念：进程信号由谁处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalManager(Endpoint);
```

> **C 来源**（`minix3/minix/include/minix/priv.h`）保留：
> - `s_k_call_mask: sys_map_t` = `BITMAP_CHUNKS(NR_SYS_CALLS)` = `[u32; 2]`
> - `s_ipc_to: sys_map_t` = 64 位 bitmap
> - `s_trap_mask: u16` (TrapHandler bitmap)
> - `s_sig_mgr: endpoint_t` (Endpoint)

但 OS 层不直接用 `u16` / `[u32; 2]`——用 newtype 表达语义。

### 4.9 ELF 加载：自由函数，非 trait

```rust
// os/kernel/src/elf_boot.rs (新文件)

use minix_types::{VirBytes, PhysBytes};
use minix_arch::Paging;
use minix_elf;

/// ELF 加载结果（OS 概念：进程从哪里开始执行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfBootResult {
    pub entry: BootEntry,
    pub allocated_bytes: usize,
}

/// 将 ELF 二进制加载到 paging 提供的虚拟地址空间。
///
/// **OS 概念**：把 ELF 段映射到虚拟地址、复制段数据、设置栈。
/// **arch 中立**：用 `Paging` trait 做实际的物理页分配/映射。
///
/// 三架构 100% 共享——不放在 trait 里。
///
/// # Arguments
/// - `image`: ELF 镜像字节流（从 boot module 读取）
/// - `user_sp`: 用户栈的最高虚拟地址（来自 KernelInfo.user_sp）
/// - `stack_size`: 栈大小（默认 64KB）
/// - `paging`: paging 实现，由 caller 决定是 MockPaging / 真实分页
///
/// # Returns
/// - `Ok(ElfBootResult)`: 加载成功
/// - `Err(ElfError)`: 加载失败（无效 ELF 等）
pub fn load_elf_into_paging<P: Paging>(
    image: &[u8],
    user_sp: VirBytes,
    stack_size: usize,
    paging: &mut P,
) -> Result<ElfBootResult, ElfError> {
    // 1. 解析 ELF header
    let entry_point = minix_elf::entry_point(image).ok_or(ElfError::InvalidElf)?;
    let iter = minix_elf::segment_iter(image).ok_or(ElfError::InvalidElf)?;

    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated: usize = 0;

    // 2. 映射每个 PT_LOAD 段
    for seg in iter {
        let flags = elf_flags_to_page_flags(seg.flags);
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1);
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            // 1:1 物理映射（boot 阶段 paddr = vaddr）
            // SAFETY: 物理地址是 vaddr 的镜像，由 paging.map 负责分配
            let paddr = PhysBytes(vaddr);
            paging.map(VirBytes(vaddr), paddr, flags)
                .map_err(ElfError::MapFailed)?;
            total_allocated += page_size as usize;

            // 复制段数据
            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - (copy_start % page_size as usize),
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    let src_offset = file_offset as usize;
                    // SAFETY: vaddr 已通过 paging.map 映射；复制长度在 [0, page_size]
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            image.as_ptr().add(src_offset),
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

    // 3. 设置用户栈
    let stack_high = user_sp;
    let sp = VirBytes(stack_high.0 - stack_size as u64);
    let stack_flags = PageFlags::read_write();
    let mut stack_addr = sp.0 & !(page_size - 1);
    while stack_addr < stack_high.0 {
        paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags)
            .map_err(ElfError::MapFailed)?;
        total_allocated += page_size as usize;
        stack_addr += page_size;
    }

    // 4. 设置 ps_strings
    let ps_strings = VirBytes(sp.0 - 32);

    Ok(ElfBootResult {
        entry: BootEntry { pc: VirBytes(entry_point), sp, ps_strings },
        allocated_bytes: total_allocated,
    })
}
```

**为什么是自由函数而非 trait 方法**：
- 三架构 ELF 加载代码 100% 相同（problem.md #23 / 我的 #23）
- `Paging` trait 已经提供了"分配物理页 + 映射"机制，arch 不需要再做
- 把 ELF 加载放在 kernel crate（不是 arch crate）——因为它本来就是 OS 概念（"加载进程镜像"）

### 4.10 `ProcessTable` / `PrivTable` 无堆版本

```rust
// os/kernel/src/proc_table.rs

use crate::proc::{KProcess, ProcNr, ...};

pub const PROC_TABLE_SIZE: usize = NR_TASKS + NR_PROCS;  // = 261

/// 进程表（编译期固定大小，no_std 兼容）。
#[repr(C)]
pub struct ProcessTable {
    pub procs: [KProcess; PROC_TABLE_SIZE],
    pub sched: Scheduler,
    pub vm_request_queue: VmRequestQueue,
}

impl ProcessTable {
    /// 编译期可构造的 new。
    pub const fn new() -> Self {
        // KProcess 字段全部 const-init 可行
        // （验证：所有 Atomic* 类型支持 const new；所有 [T; N] 数组支持 const 构造）
        Self {
            procs: [const { KProcess::new_empty() }; PROC_TABLE_SIZE],
            sched: Scheduler::new(),
            vm_request_queue: VmRequestQueue::new(),
        }
    }
}
```

```rust
// os/kernel/src/kpriv.rs

pub const NR_SYS_PROCS: usize = 64;

pub struct PrivTable {
    pub privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    pub const fn new() -> Self {
        Self {
            privs: [const { KPriv::new_empty() }; NR_SYS_PROCS],
        }
    }
}
```

**const-init 可行性验证**（独立计算）：

| 字段 | 类型 | const-init? | 说明 |
|------|------|------------|------|
| `p_rts_flags` | `RtsFlags(AtomicU32)` | ✅ | `AtomicU32::new(0)` 是 const (Rust 1.75+) |
| `p_misc_flags` | `MiscFlags(AtomicU32)` | ✅ | 同上 |
| `p_sched` | `SchedFields { priority: AtomicI8, ... }` | ✅ | `AtomicI8::new(0)` 是 const |
| `p_nextready/caller_q/q_link` | `AtomicI32` | ✅ | const |
| `p_getfrom_e/p_sendto_e` | `Endpoint` | ✅ | `Endpoint` 是 `repr(transparent) struct { u32 }`，支持 const |
| `p_pending` | `SigSet(u64)` | ✅ | `SigSet(0)` 是 const |
| `p_name` | `ProcName { data: [u8; 16] }` | ✅ | `ProcName::new()` 是 const |
| `p_sendmsg/p_delivermsg` | `Message` | ✅ | `Message` derive Copy + Default |
| `p_ext_reg_state` | `ExtRegState` | — | **删除**（设计变更）|
| `p_vm_suspend` | `Option<VmSuspendContext>` | ✅ | `None` 是 const |
| `p_next_restart/next_requestor` | `Option<ProcNr>` | ✅ | `None` 是 const |
| `boot_context` | `CurrentBootContext` (cfg-selected) | ✅ | 各 arch 的 BootContext 提供 `const EMPTY` |
| `p_fault_addr` | `Option<...>` | ✅ | `None` 是 const |
| `p_defer` | `DeferArgs` | ✅ | 如果是 POD，const |
| `p_dequeued` | `AtomicU64` | ✅ | const |
| `p_seg` | `ProcessSegments` | ⚠️ | 需检查（如果含 Atomic 字段就 OK）|
| `p_endpoint` | `Endpoint` | ✅ | const |

**结论**：所有字段 const-init 可行（前提：`p_ext_reg_state` 删除；`p_seg` 检查后调整）。

> **解决 `priority::USER_Q` 非 const 问题**（我的 #19）：删除 `SchedFields::new()` 中的 `AtomicI8::new(priority::USER_Q)`，改为 `AtomicI8::new(0)` + 后续 `KProcess::set_priority(USER_Q)` 显式调用。`priority` 初始化为 0（=NONE），OS 知道 NONE 表示未初始化。

---

## 五、`init_proc_and_boot` 主流程

### 5.1 重写后的流程

```rust
// os/kernel/src/lib.rs

pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> ProcessTable {
    use minix_arch::proc_init::{CurrentBootContext, ProcessCategory, BootEntry};
    use crate::proc_table::ProcessTable;
    use crate::kpriv::{PrivTable, ProcessCategory, SignalManager};
    use crate::elf_boot::load_elf_into_paging;
    use crate::paging::mock::MockPaging;  // 或真实 paging，由 cfg 决定

    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();

    // Step 1: 初始化所有进程槽为 SLOT_FREE
    // （ProcessTable::new() 已完成）

    // Step 2: 初始化所有 priv 槽为空
    // （PrivTable::new() 已完成）

    // Step 3: 遍历 boot image（kernel tasks + user modules）
    for boot_entry in enumerate_boot_image(kernel_info) {
        let nr = boot_entry.proc_nr;
        let proc = proc_table.get_mut(nr).expect("valid proc nr");

        // Step 3a: 进程名
        proc.set_name(boot_entry.name);

        // Step 3b: 特权分配（按 ProcessCategory）
        if boot_entry.schedulable {
            let priv_id = priv_table.assign_static(nr)
                .expect("static priv slot occupied");
            priv_table.configure_for(
                priv_id,
                boot_entry.category,  // ProcessCategory 枚举
                SignalManager::self_ref(nr),
            );
        } else {
            proc.set_rts_flags(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        // Step 3c: Boot context（arch-specific 透明值）
        let entry = if boot_entry.is_vm {
            // Step 3c-VM: 加载 VM ELF
            #[cfg(feature = "mock")]
            let mut paging = MockPaging::new();
            #[cfg(not(feature = "mock"))]
            let mut paging = create_vm_bootstrap_paging();

            let result = load_elf_into_paging(
                boot_entry.elf_image,
                kernel_info.user_sp,
                VM_STACK_SIZE,
                &mut paging,
            ).expect("VM ELF load failed");

            result.entry
        } else {
            BootEntry { pc: VirBytes(0), sp: VirBytes(0), ps_strings: VirBytes(0) }
        };

        let ctx = CurrentBootContext::make_context(boot_entry.category, Some(entry));
        proc.set_boot_context(ctx);  // 单个 setter，存储整体

        // Step 3d: VM inhibit
        if !boot_entry.is_vm && !is_kernel_task(nr) {
            proc.set_rts_flags(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }

        // Step 3e: 启动状态
        proc.set_rts_flags(RtsFlagsBits::PROC_STOP);
        proc.clear_rts_flags(RtsFlagsBits::SLOT_FREE);
    }

    proc_table
}
```

**对比旧版（`os/kernel/src/lib.rs:680-840`）**：

| 旧版 | 新版 |
|------|------|
| `reg_state = CurrentBootProcArch::initial_reg_state(...)`<br>`proc.set_boot_initial_reg_state(reg_state.status, reg_state.fpu_needs_zero)` | `ctx = CurrentBootContext::make_context(category, entry)`<br>`proc.set_boot_context(ctx)` |
| `init_regs = CurrentBootProcArch::init_regs(...)`<br>`proc.set_boot_pc_sp(init_regs.pc, init_regs.sp, init_regs.ps_strings_reg)` | （合并到 `make_context`）|
| `if is_vm { CurrentBootProcArch::load_vm_elf(...) }` | `if is_vm { load_elf_into_paging(...) }`（自由函数）|
| `priv_table.configure_boot_priv(priv_id, flags, 0, 0, IPC_TO_ALL, K_CALL_MASK_ALL, ...)` (6 裸参数) | `priv_table.configure_for(priv_id, ProcessCategory::SystemService, ...)` (1 枚举) |

**行数变化**：旧 165 行（`lib.rs:680-840`）→ 新 ~40 行。简化源于：
- 不再有 `set_boot_initial_reg_state` + `set_boot_pc_sp` 两个 setter
- 不再有 `fpu_needs_zero` 分支
- 6 裸参数打包为 1 枚举
- `CurrentBootContext::make_context` 一次完成

---

## 六、arch 层 trait 实现

### 6.1 x86-64 实现

```rust
// os/arch/src/arch/proc_init/x86_64.rs

use minix_types::VirBytes;
use crate::paging::PageFlags;
use super::{ArchProcessInit, BootContext, ProcessCategory, BootEntry};
use crate::trap::TrapFrame;

/// x86-64 启动上下文（arch-specific 透明值）。
///
/// OS 层不解释内部字段——只整体存储到 KProcess.boot_context，整体应用到 trap frame。
#[derive(Debug, Clone, Copy, Default)]
pub struct X86_64BootContext {
    /// RFLAGS 初值（IOPL/IF 等位）
    pub rflags: Rflags,
    /// 段选择子（CS/DS/SS/ES/FS/GS）
    pub segments: SegmentSelectors,
    /// 入口点（PC/SP/ps_strings）
    pub entry: Option<BootEntry>,
    /// XSAVE area 懒加载标志
    pub xsave_initialized: bool,
}

/// x86-64 RFLAGS 包装（OS 概念：进程初始标志位）。
#[derive(Debug, Clone, Copy, Default)]
pub struct Rflags(u64);

impl Rflags {
    pub const KERNEL_TASK: Self = Self(0x1202);  // IOPL=1, IF=1, bit1=1
    pub const USER: Self = Self(0x0202);        // IOPL=0, IF=1, bit1=1
}

/// x86-64 段选择子（arch-internal，不出现在 OS 层）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentSelectors {
    pub cs: u16,
    pub ds: u16,
    pub ss: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
}

impl ArchProcessInit for X86_64BootContext {
    type Context = X86_64BootContext;

    fn make_context(category: ProcessCategory, entry: Option<BootEntry>) -> Self::Context {
        let rflags = match category {
            ProcessCategory::KernelTask { is_idle: _ } => Rflags::KERNEL_TASK,
            ProcessCategory::SystemService | ProcessCategory::User => Rflags::USER,
        };
        let segments = match category {
            ProcessCategory::KernelTask { is_idle: _ } => SegmentSelectors::kernel_default(),
            _ => SegmentSelectors::user_default(),
        };
        Self {
            rflags,
            segments,
            entry,
            xsave_initialized: false,  // 懒加载——first FPU use 才分配
        }
    }

    fn apply_to_trap_frame(ctx: &Self::Context, frame: &mut TrapFrame) {
        frame.rflags = ctx.rflags.0;
        frame.cs = ctx.segments.cs;
        frame.ds = ctx.segments.ds;
        frame.ss = ctx.segments.ss;
        frame.es = ctx.segments.es;
        frame.fs = ctx.segments.fs;
        frame.gs = ctx.segments.gs;
        if let Some(entry) = ctx.entry {
            frame.rip = entry.pc.0;
            frame.rsp = entry.sp.0;
            frame.rbx = entry.ps_strings.0;  // x86-64: ps_strings in rbx
        }
        // XSAVE area 懒加载——CR0.TS 保持，第一次 FP 指令触发 #NM
    }
}
```

### 6.2 aarch64 实现（不出现 SegmentSelectors/FPU）

```rust
// os/arch/src/arch/proc_init/aarch64.rs

use super::{ArchProcessInit, BootContext, ProcessCategory, BootEntry};
use crate::trap::TrapFrame;

#[derive(Debug, Clone, Copy, Default)]
pub struct Aarch64BootContext {
    pub spsr: Spsr,
    pub entry: Option<BootEntry>,
    pub cpacr_fpen: CpacrFpen,  // CPACR_EL1.FPEN 字段
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Spsr(u64);

impl Spsr {
    pub const KERNEL_TASK: Self = Self(0x000003C5);  // M=EL1h, F=1, I=1, A=1, D=1
    pub const USER: Self = Self(0x00000000);         // M=EL0t
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpacrFpen(u8);

impl ArchProcessInit for Aarch64BootContext {
    type Context = Aarch64BootContext;

    fn make_context(category: ProcessCategory, entry: Option<BootEntry>) -> Self::Context {
        let spsr = match category {
            ProcessCategory::KernelTask { .. } => Spsr::KERNEL_TASK,
            _ => Spsr::USER,
        };
        let cpacr_fpen = match category {
            ProcessCategory::KernelTask { .. } => CpacrFpen(0b00),  // EL0/EL1 trap
            _ => CpacrFpen(0b01),  // EL0 enable, EL1 trap
        };
        Self { spsr, entry, cpacr_fpen }
    }

    fn apply_to_trap_frame(ctx: &Self::Context, frame: &mut TrapFrame) {
        frame.spsr = ctx.spsr.0;
        frame.cpacr_el1 = ctx.cpacr_fpen.0 as u64;
        if let Some(entry) = ctx.entry {
            frame.elr = entry.pc.0;
            frame.sp = entry.sp.0;
            frame.r0 = entry.ps_strings.0;  // aarch64: ps_strings in r0
        }
        // 不需要 FPU 初始化——CPACR_EL1.FPEN 已在 cpacr_el1 字段设置
    }
}
```

### 6.3 riscv64 实现（不出现 SegmentSelectors/FPU）

```rust
// os/arch/src/arch/proc_init/riscv64.rs

use super::{ArchProcessInit, BootContext, ProcessCategory, BootEntry};
use crate::trap::TrapFrame;

#[derive(Debug, Clone, Copy, Default)]
pub struct Riscv64BootContext {
    pub sstatus: Sstatus,
    pub entry: Option<BootEntry>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Sstatus(u64);

impl Sstatus {
    pub const KERNEL_TASK: Self = Self(0x100);  // SPP=1
    pub const USER: Self = Self(0x20);          // SPIE=1
}

impl ArchProcessInit for Riscv64BootContext {
    type Context = Riscv64BootContext;

    fn make_context(category: ProcessCategory, entry: Option<BootEntry>) -> Self::Context {
        let sstatus = match category {
            ProcessCategory::KernelTask { .. } => Sstatus::KERNEL_TASK,
            _ => Sstatus::USER,
        };
        Self { sstatus, entry }
    }

    fn apply_to_trap_frame(ctx: &Self::Context, frame: &mut TrapFrame) {
        frame.sstatus = ctx.sstatus.0;
        if let Some(entry) = ctx.entry {
            frame.sepc = entry.pc.0;
            frame.sp = entry.sp.0;
            frame.a0 = entry.ps_strings.0;  // riscv64: ps_strings in a0
        }
        // sstatus.FS = Initial (第一次 FP 指令会 trap)
    }
}
```

**关键观察**：
- 三个 arch 的 `make_context` / `apply_to_trap_frame` **逻辑结构 100% 相同**，仅字段名/常量值不同
- 这恰好证明：3 个旧 trait 是过度设计——1 个 trait 足够
- aarch64/riscv64 编译后**根本不出现** `xsave_initialized`、`segments` 等 x86-64 字段（关联类型/类型别名机制保证）

---

## 七、修复覆盖问题清单

下表确认本设计对 problem.md #1-#12 + 我独立的 #13-#25 全部问题的覆盖：

| 问题 | 本设计的解决 | 验证方法 |
|------|------------|---------|
| #1 `segment_selectors` 泄漏 | `X86_64BootContext` 内部字段，OS 层只见 `CurrentBootContext` | `grep "SegmentSelectors" os/kernel/` 应当 0 命中 |
| #2 `ps_strings_reg` 命名混淆 | 重命名为 `ps_strings: VirBytes` | `grep "ps_strings_reg" os/` 应当 0 命中 |
| #3 kernel setter 拆解 arch 返回值 | 单一 `set_boot_context(ctx)` 整体存储 | `grep "set_boot_initial\|set_boot_pc" os/` 应当 0 命中 |
| #4 `InitialRegs` 拆 3 字段 | 合并到 `BootContext` | 旧字段 `initial_pc/sp/ps_strings_reg/status` 全部删除 |
| #5 `initial_status` 泄漏 | 字段下沉到 `X86_64BootContext.rflags`，kernel 不再直接修改 | `grep "initial_status" os/kernel/src/syscall_device.rs` 应当改为 `X86_64Arch::enable_iopl_for_process` |
| #6 文档 §3.2 自相矛盾 | 文档 §3 重写时严格遵守原则 A | 见 §10 重写要点 |
| #7 `SegmentSelectors::default()` 哨兵 | 类型从 OS 层消失 | `grep "SegmentSelectors" os/kernel/` 应当 0 命中 |
| #8 `fpu_needs_zero` 翻译 | OS 层不出现任何 FPU 概念 | `grep "fpu_needs_zero\|FPU\|fpu_" os/kernel/src/proc.rs` 应当 0 命中 |
| #9 "init_regs 内部调用 reset" 编造 | 单 trait 方法，无顺序耦合 | trait 方法签名无 `&ArchProcReset` 依赖 |
| #10 3 个 trait 全是 C 函数翻译 | 合并为 1 个 `ArchProcessInit` | `grep "trait ArchProc" os/arch/src/` 应当 1 个 trait |
| #11 `Box<[KProcess]>` 用堆 | `[KProcess; PROC_TABLE_SIZE]` | `grep "Box<\[KProcess\]\|Box<\[KPriv\]" os/kernel/` 应当 0 命中 |
| #12 §3.4 全文 translate 味 | 重写为 OS 概念驱动 | 文档 Ch3 §3 整节重写 |
| #13 `p_ext_reg_state` 144KB 浪费 | 字段从 KProcess 删除 | `grep "p_ext_reg_state\|ExtRegState" os/kernel/src/proc.rs` 应当 0 命中 |
| #14 KPriv 30+ 裸字段 | 重组为 5-6 个子结构 | 新结构定义见 §4.6 |
| #15 `SYS_CALL_MASK_SIZE = 2` 硬编码 | 引用 `minix_types::BITMAP_CHUNKS` 宏 | `grep "SYS_CALL_MASK_SIZE = 2" os/kernel/src/kpriv.rs` 应当改为 `= BITMAP_CHUNKS(NR_SYS_CALLS)` |
| #16 `set_boot_initial_reg_state` 接受 `_fpu_needs_zero` 然后丢弃 | setter 整体接收 `CurrentBootContext`，不拆解 | `grep "_fpu_needs_zero" os/` 应当 0 命中 |
| #17 `syscall_device.rs` 直接改 RFLAGS IOPL | 改为调用 `X86_64Arch::enable_iopl_for_process(&mut ctx)` | `grep "initial_status \|= X86_64_IOPL" os/` 应当 0 命中 |
| #18 `p_ext_reg_state` 576B 比 IPC 大 18 倍 | 删除后：所有 arch 静态内存预算 = 32B × 2 (IPC) | 计算 size |
| #19 `SchedFields::new()` 调 `priority::USER_Q` 非 const | 改为 `AtomicI8::new(0)` + 显式 `set_priority(USER_Q)` | grep 验证 |
| #20 `#[cfg(feature = "mock")]` 在生产模式走 PC=0 | 增加 `create_vm_bootstrap_paging()` 工厂，删除 mock 分支 | grep 验证 |
| #21 文档 §4.6 漏讲 KProcess/KPriv 字段 | Ch4 §4 整章重写，新增 §4.7-KProcess-Fields 和 §4.8-KPriv-Fields | 文档 grep 验证 |
| #22 `idempotent_priv_id` 命名反义 | 改回 `static_priv_id` | grep 验证 |
| #23 三 arch `load_vm_elf` 100% 相同 | 改自由函数 `load_elf_into_paging` | `grep "fn load_vm_elf" os/arch/src/` 应当 0 命中 |
| #24 `mock::MockPaging` 在生产模式静默掩盖 | 工厂函数 `create_vm_bootstrap_paging()` 显式选择 | grep 验证 |
| #25 ELF `paddr = vaddr` 是用户态假设 | 文档 §4.x 显式标注"boot 阶段 1:1 映射是临时方案" | 文档 grep 验证 |

---

## 八、测试设计

### 8.1 L1 对偶测试（与 Minix3 行为一致）

```rust
// os/arch/src/arch/proc_init/x86_64.rs 的 tests 段

#[test]
fn l1_parity_kernel_task_rflags() {
    // C: INIT_TASK_PSW = 0x1200 (32-bit), minix-rs: 0x1202 (64-bit, bit1=1)
    let ctx = X86_64BootContext::make_context(
        ProcessCategory::KernelTask { is_idle: false },
        None,
    );
    assert_eq!(ctx.rflags.0, 0x1202);
}

#[test]
fn l1_parity_user_process_rflags() {
    // C: INIT_PSW = 0x0200 (32-bit), minix-rs: 0x0202 (64-bit)
    let ctx = X86_64BootContext::make_context(
        ProcessCategory::User,
        None,
    );
    assert_eq!(ctx.rflags.0, 0x0202);
}

#[test]
fn l1_parity_aarch64_user_spsr() {
    // C: INIT_PSR = 0x50 (32-bit ARM), minix-rs: 0x0 (64-bit, M=EL0t)
    let ctx = Aarch64BootContext::make_context(ProcessCategory::User, None);
    assert_eq!(ctx.spsr.0, 0x0);
}

#[test]
fn l1_parity_riscv64_user_sstatus() {
    // minix-rs 设计：SPIE=1
    let ctx = Riscv64BootContext::make_context(ProcessCategory::User, None);
    assert_eq!(ctx.sstatus.0, 0x20);
}
```

### 8.2 L2 契约测试（trait 契约）

```rust
// os/arch/src/arch/proc_init.rs (在 #[cfg(test)] mod tests 段)

#[test]
fn l2_parity_compile_time_arch_selection() {
    // 验证：x86-64 编译时 CurrentBootContext == X86_64BootContext
    // 验证：aarch64 编译时不存在 X86_64BootContext 引用
    #[cfg(target_arch = "x86_64")]
    {
        let _: X86_64BootContext = CurrentBootContext::default();
    }
    #[cfg(target_arch = "aarch64")]
    {
        let _: Aarch64BootContext = CurrentBootContext::default();
    }
    #[cfg(target_arch = "riscv64")]
    {
        let _: Riscv64BootContext = CurrentBootContext::default();
    }
}

#[test]
fn l2_no_x86_leak_in_non_x86() {
    // 验证：aarch64 编译时不存在 SegmentSelectors
    #[cfg(target_arch = "aarch64")]
    {
        // 编译期检查：以下行编译失败
        // let _: SegmentSelectors = todo!();  // 取消注释会编译错误
    }
}
```

### 8.3 L2 进程表/特权表契约测试

```rust
// os/kernel/src/proc_table.rs (tests 段)

#[test]
fn l2_proc_table_const_new() {
    // 验证：ProcessTable::new() 是 const fn
    const _TABLE: ProcessTable = ProcessTable::new();
    // 编译期就构造好，no_std 兼容
}

#[test]
fn l2_proc_table_size() {
    use core::mem::size_of;
    let proc_size = size_of::<KProcess>();
    let table_size = size_of::<ProcessTable>();
    // KProcess ~256-512 bytes, ProcessTable = KProcess * 261 + sched + queue
    // 无 Box 指针（8 字节），节省 8 字节
    assert!(table_size < 200_000, "ProcessTable should be < 200KB, got {}B", table_size);
}

#[test]
fn l2_priv_table_size() {
    use core::mem::size_of;
    let priv_size = size_of::<KPriv>();
    let table_size = size_of::<PrivTable>();
    // KPriv 重构后 < 4KB
    assert!(priv_size < 4096, "KPriv should be < 4KB, got {}B", priv_size);
    assert!(table_size < 256_000, "PrivTable should be < 256KB, got {}B", table_size);
}
```

### 8.4 L3 doctest（公共 API 示例）

```rust
// os/arch/src/arch/proc_init.rs 顶部 trait 定义

/// 构造新进程的启动上下文。
///
/// # Example
/// ```
/// use minix_arch::proc_init::{ArchProcessInit, CurrentBootContext, ProcessCategory, BootEntry};
/// use minix_types::VirBytes;
///
/// let entry = BootEntry {
///     pc: VirBytes(0x400_000),
///     sp: VirBytes(0x7fff_0000),
///     ps_strings: VirBytes(0x7fff_0000 - 32),
/// };
/// let ctx = CurrentBootContext::make_context(ProcessCategory::User, Some(entry));
/// // ctx 现在是 arch-specific 透明值，可存储到 KProcess.boot_context
/// ```
pub trait ArchProcessInit: Copy + 'static { ... }
```

### 8.5 测试覆盖矩阵

| 维度 | 测试类型 | 测试函数数 | 覆盖目标 |
|------|---------|----------|---------|
| arch PSW/PSR/sstatus 初值 | L1 对偶 | 6（3 arch × 2 category）| 与 Minix3 行为一致 |
| ELF 加载边界 | L1 对偶 | 4 | 空 ELF / 无效 ELF / 段越界 / 正常 |
| ProcessTable 容量 | L2 契约 | 2 | const-fn 构造 + 内存预算 |
| PrivTable 容量 | L2 契约 | 2 | const-fn 构造 + 内存预算 |
| 编译期 arch 选择 | L2 契约 | 1 | `CurrentBootContext` 编译期定型 |
| Capability 类别默认 | L2 契约 | 3 | KernelTask / SystemService / User |
| 公共 API 用法 | L3 doctest | 3 | make_context / apply_to_trap_frame / configure_for |
| **合计** | — | **21** | — |

---

## 九、迁移计划（具体文件改动清单）

### 9.1 删除的文件

| 路径 | 原因 |
|------|------|
| `os/arch/src/arch/proc_arch.rs` | 旧 3-trait 文件，被 `proc_init.rs` 替代 |
| `os/arch/src/x86_64/proc_arch.rs` | 旧 x86-64 impl，被 `proc_init/x86_64.rs` 替代 |
| `os/arch/src/arm64/proc_arch.rs` | 旧 aarch64 impl，被 `proc_init/aarch64.rs` 替代 |
| `os/arch/src/riscv64/proc_arch.rs` | 旧 riscv64 impl，被 `proc_init/riscv64.rs` 替代 |

### 9.2 新增的文件

| 路径 | 内容 |
|------|------|
| `os/arch/src/arch/proc_init.rs` | trait + type alias |
| `os/arch/src/arch/proc_init/x86_64.rs` | x86-64 impl |
| `os/arch/src/arch/proc_init/aarch64.rs` | aarch64 impl |
| `os/arch/src/arch/proc_init/riscv64.rs` | riscv64 impl |
| `os/kernel/src/elf_boot.rs` | ELF 加载自由函数 |
| `os/kernel/src/types.rs` | TrapMask / SyscallBitmap / IpcBitmap / KCallBitmap newtype |

### 9.3 修改的文件

| 路径 | 改动 |
|------|------|
| `os/kernel/src/proc.rs` | 删 `initial_pc/sp/ps_strings_reg/status`/`p_ext_reg_state`；加 `boot_context: CurrentBootContext`；加 `set_boot_context` setter；删 `set_boot_initial_reg_state`/`set_boot_pc_sp` |
| `os/kernel/src/proc_table.rs` | `procs: Box<[KProcess]>` → `procs: [KProcess; PROC_TABLE_SIZE]`；`new()` 改 `const fn` |
| `os/kernel/src/kpriv.rs` | 30+ 裸字段重组为 6 子结构（capability/signals/io/mem/irq/grant）；`privs: Box<[KPriv]>` → `privs: [KPriv; NR_SYS_PROCS]`；`new()` 改 `const fn`；`configure_boot_priv(6 args)` → `configure_for(category)` |
| `os/kernel/src/lib.rs` | `init_proc_and_boot()` 重写为 §5.1 流程（~40 行 vs 旧 165 行） |
| `os/kernel/src/syscall_device.rs` | `target.initial_status \|= X86_64_IOPL_BITS` → `X86_64Arch::enable_iopl_for_process(&mut ctx)` |
| `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/06-proc-init-boot-proc.md` | 整章重写：Ch3 改为 OS 概念驱动，Ch4 改为按数据类型组织，新增 §4.7 KProcess-Fields 和 §4.8 KPriv-Fields |

### 9.4 风险评估

| 改动 | 风险 | 缓解 |
|------|------|------|
| 删 4 个 `initial_*` 字段 | 调用方未更新 | grep 全 kernel 确认所有 `initial_*` 引用；先重写 lib.rs 调用，再删字段 |
| 删 `p_ext_reg_state` 字段 | fork 逻辑依赖（`fork_from` 中复制） | 检查 `fork_from` 是否还在用，更新为新机制 |
| 改 `KPriv` 字段 | 大量代码访问裸字段 | 增量迁移：先加新结构，旧字段保留为 wrapper |
| `const fn new` 全 KProcess 构造 | `priority::USER_Q` 等非 const | 改为 `AtomicI8::new(0)` + 显式 set_priority |
| 删 `mock` 分支 | 当前 `cargo build --release` 用 mock 加载 VM | 加 `create_vm_bootstrap_paging()` 工厂（先 stub） |

### 9.5 实施顺序（避免编译破坏）

1. **Phase 1（基础设施）**：新增 `os/arch/src/arch/proc_init.rs` + `proc_init/<arch>.rs`，**保留旧 trait 不删**（双套并行）
2. **Phase 2（数据迁移）**：修改 `KProcess` 加 `boot_context: CurrentBootContext`（与 `initial_*` 并存）；新增 `KPriv` 子结构（与裸字段并存）
3. **Phase 3（代码迁移）**：修改 `init_proc_and_boot` 调用新 API；修改 `syscall_device` 走新 arch 方法
4. **Phase 4（清理）**：删除旧 4 个 `initial_*` 字段、删除 `p_ext_reg_state`、删除旧 3 个 trait、删除旧 `load_vm_elf` 重复实现、删除 `Box<[...]>`
5. **Phase 5（测试）**：补 L1/L2/L3 测试，验证 const-fn 构造、arch-specific 字段消失、ProcessTable 大小

---

## 十、文档 Ch3+Ch4 重写要点

### 10.1 Ch3（设计决策）—— 概念驱动，不提 C 函数

**原文档 §3 的核心问题**：每段都"对应 C 函数名"——典型的实现驱动 + translate 味（problem.md #12）。

**重写后的 Ch3 结构**：

| 章节 | 核心命题 | 回答的问题 |
|------|---------|----------|
| §3.1 | 启动进程是 OS 关心的 3 个抽象问题 | 启动一个进程需要什么？ |
| §3.2 | BootContext 抽象：arch-specific 透明值 | 为什么 OS 不直接看到 x86-64 段选择子？ |
| §3.3 | ProcessCategory：OS 概念 | 为什么不需要 6 字段配置？ |
| §3.4 | Capability：进程能力模型 | 进程能做什么（vs 进程是什么）？ |
| §3.5 | ELF 加载为什么不是 arch trait | 三架构 100% 相同 |
| §3.6 | 无堆原则：编译期固定大小 | 为什么不用 Box/Vec？ |
| §3.7 | 单一 trait 原则 | 为什么不是 3 个 trait？ |

**反 translate 措辞要求**：
- ❌ "对应 C 的 `arch_proc_reset()`"  → ✅ "构造新进程的启动上下文"
- ❌ "对应 C 的 `get_priv(rp, static_priv_id(proc_nr))`"  → ✅ "分配系统进程的能力槽位"
- ❌ "C 版的 `arch_proc_init` 内部调用 `arch_proc_reset`"  → 整段删除（problem.md #9 编造）

### 10.2 Ch4（实现详解）—— 按数据类型组织

**原文档 §4 的核心问题**：
- §4.6 漏讲 KProcess/KPriv 字段（problem.md #21）
- §4 全按"步骤"组织，与"数据结构"脱节

**重写后的 Ch4 结构**：

| 章节 | 内容 | 解决 |
|------|------|------|
| §4.1 | `init_proc_and_boot()` 主流程（5 步） | 重写为 ~40 行伪代码 |
| §4.2 | `CurrentBootContext` 定义 + 编译期选择 | 解释 type alias + cfg |
| §4.3 | `ArchProcessInit::make_context` 三架构 | 并列展示 3 个 impl |
| §4.4 | `ArchProcessInit::apply_to_trap_frame` | 解释 arch 内部消化 FPU/段 |
| §4.5 | `load_elf_into_paging()` 自由函数 | 解释为什么不是 trait |
| §4.6 | `PrivTable::configure_for()` | 用 ProcessCategory 枚举驱动 |
| §4.7 | KProcess 字段完整列表（30+ 字段） | **新增**——按分组（identity / sched / IPC / signals / VM / boot） |
| §4.8 | KPriv 字段完整列表（30+ 字段） | **新增**——按子结构（capability / signals / io / mem / irq / grant） |
| §4.9 | ProcessTable 内存布局 | `procs: [KProcess; N]` 编译期大小 |
| §4.10 | 测试设计 | L1/L2/L3 各列举 |

### 10.3 §4.7 KProcess 字段详解（新增）

```markdown
### 4.7 KProcess 字段详解

KProcess 是 OS 中描述进程的核心数据结构，约 30 字段，按职责分为 6 组：

| 分组 | 字段 | 类型 | 用途 | C 来源 |
|------|------|------|------|-------|
| **identity** | `p_nr` | `ProcNr` | 进程号（-NR_TASKS..NR_PROCS-1）| `proc_nr` |
| | `p_endpoint` | `Endpoint` | IPC 端点 | `endpoint` |
| | `priv_id` | `Option<PrivId>` | 关联的 KPriv 槽 | `p_priv` |
| | `p_name` | `ProcName` | 进程名（16B 固定）| `p_name` |
| **状态** | `p_rts_flags` | `RtsFlags` (Atomic) | 调度状态（SLOT_FREE/SENDING/...）| `p_rts_flags` |
| | `p_misc_flags` | `MiscFlags` (Atomic) | 杂项标志（DELIVERMSG/...）| `p_misc_flags` |
| | `p_fault_addr` | `Option<...>` | 待处理页错误地址 | `p_fault_addr` |
| **调度** | `p_sched` | `SchedFields` | 优先级/时间片/CPU 亲和性 | `p_sched` |
| | `p_accounting` | `Accounting` | 记账 | `p_accounting` |
| | `p_time` | `TimeStats` | 用户/系统时间 | `p_user_time/p_sys_time` |
| | `p_cycles` | `CyclesStats` | 周期计数 | `p_cycles` |
| | `p_cpuavg` | `CpuAvg` | CPU 平均利用率 | `p_cpuavg` |
| | `p_dequeued` | `AtomicU64` | 出队时间戳 | `p_dequeued` |
| | `p_defer` | `DeferArgs` | 延迟参数（VM fault 处理）| `p_defer` |
| **IPC** | `p_nextready` | `AtomicI32` | 就绪队列 | `p_nextready` |
| | `p_caller_q` | `AtomicI32` | 调用者队列 | `p_caller_q` |
| | `p_q_link` | `AtomicI32` | 队列链接 | `p_q_link` |
| | `p_getfrom_e` | `Endpoint` | RECEIVE 源 | `p_getfrom_e` |
| | `p_sendto_e` | `Endpoint` | SEND 目标 | `p_sendto_e` |
| | `p_pending` | `SigSet` | 待处理信号 | `p_pending` |
| | `p_sendmsg` | `Message` | 发送消息 | `p_sendmsg` |
| | `p_delivermsg` | `Message` | 待投递消息 | `p_delivermsg` |
| | `p_delivermsg_vir` | `VirBytes` | 投递消息虚拟地址 | `p_delivermsg_vir` |
| **VM** | `p_seg` | `ProcessSegments` | 段/页表信息 | `p_seg` |
| | `p_next_restart` | `Option<ProcNr>` | VM restart 链 | `p_vmrequest.nextrestart` |
| | `p_next_requestor` | `Option<ProcNr>` | VM request 链 | `p_vmrequest.nextrequestor` |
| | `p_vm_suspend` | `Option<VmSuspendContext>` | VM 挂起上下文 | `p_vmrequest` |
| **Boot** | `boot_context` | `CurrentBootContext` | 启动上下文（arch 透明）| `p_reg` 初值 |

**字段删除记录**：
- `p_ext_reg_state: ExtRegState` (576B) → **删除**（x86-64 内部消化，aarch64/riscv64 不需要）
- `initial_pc/sp/ps_strings_reg/status` (4 字段) → 合并为 `boot_context`
```

### 10.4 §4.8 KPriv 字段详解（新增）

```markdown
### 4.8 KPriv 字段详解

KPriv 是 OS 中描述系统进程能力的核心数据结构，按 OS 语义分为 6 子结构：

| 子结构 | 字段 | OS 概念 | C 来源 |
|--------|------|---------|-------|
| **owner** | `owner: Option<ProcNr>` | 哪个进程拥有此 priv 槽 | `s_proc_nr` |
| | `id: SysId` | Priv 槽 ID（debugging）| `s_id` |
| | `init_flags: i32` | 初始化标志 | `s_init_flags` |
| **capability** | `category: ProcessCategory` | 进程类别 | `s_flags` 部分 |
| | `syscalls: SyscallBitmap` | 可调用 syscall | `s_ipc_to` + ipc_filter |
| | `ipc_targets: IpcBitmap` | 可发送 IPC 目标 | `s_ipc_to` |
| | `k_calls: KCallBitmap` | 可调用 kernel call | `s_k_call_mask` |
| | `trap_mask: TrapMask` | 可接收异常 | `s_trap_mask` |
| | `signal_manager: SignalManager` | 信号管理员 | `s_sig_mgr` |
| **signals** | `manager` / `bak_manager` | 主/备信号管理员 | `s_sig_mgr` / `s_bak_sig_mgr` |
| | `notify_pending: u64` | 待通知 | `s_notify_pending` |
| | `asyn_pending: u64` | 待异步 | `s_asyn_pending` |
| | `int_pending: u32` | 待中断 | `s_int_pending` |
| | `sig_pending: SigSet` | 待信号 | `s_sig_pending` |
| | `alarm_timer: Option<TimerEntry>` | 闹钟 | `s_alarm_timer` |
| **io** | `ranges: [IoRange; NR_IO_RANGE]` | I/O 端口范围 | `s_io_tab` |
| | `nr_ranges: i32` | 有效范围数 | `s_nr_io_range` |
| **mem** | `ranges: [MemRange; NR_MEM_RANGE]` | 内存访问范围 | `s_mem_tab` |
| | `nr_ranges: i32` | 有效范围数 | `s_nr_mem_range` |
| **irq** | `tabs: [i32; NR_IRQ]` | 中断位图 | `s_irq_tab` |
| | `nr_irq: i32` | 有效中断数 | `s_nr_irq` |
| **grant** | `table: usize` | grant table 地址 | `s_grant_table` |
| | `entries: i32` | grant table 项数 | `s_grant_entries` |
| | `endpoint: Endpoint` | grant 端点 | `s_grant_endpoint` |
| **state** | `table: usize` | state table 地址 | `s_state_table` |
| | `entries: i32` | state table 项数 | `s_state_entries` |
| **async** | `table: u64` | 异步通知表 | `s_asyntab` |
| | `size: usize` | 表大小 | `s_asynsize` |
| | `endpoint: Endpoint` | 异步端点 | `s_asynendpoint` |
| **其他** | `ipc_filter: Option<usize>` | IPC filter 引用 | `s_ipcf` |
| | `stack_guard: Option<usize>` | 栈保护 | `s_stack_guard` |
| | `diag_sig: bool` | 诊断信号 | `s_diag_sig` |

**字段分组收益**：
- 6 裸参数 `configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` 简化为 1 个 `configure_for(category, signal_manager)`
- Newtype（`TrapMask`/`SyscallBitmap`/`IpcBitmap`）防止裸 `u16`/`u64` 误用
- 6 个子结构对应 OS 关心的 6 个维度，认知负担 < 30 个裸字段
```

---

## 十一、本设计的局限与待多 AI bagging 时澄清

### 11.1 我自己仍有疑问的点

1. **`boot_context: CurrentBootContext` 在 `KProcess` 中的位置**：放在 §4.7 "Boot" 组还是单独一组？当前选择"Boot"组，但语义上它更接近"identity"——因为每个进程只有一个。
2. **`p_seg: ProcessSegments`**：未深入分析。它包含什么？是否有 arch 字段？可能也违反"OS 与硬件解耦"原则。
3. **`p_defer: DeferArgs`**：未深入分析。VM fault 处理的延迟参数——可能是 arch-specific。
4. **`SchedFields` 中 `scheduler: Option<...>` 字段**：未深入分析。可能是调度器特定数据。
5. **`Capability` 是否应包含 `priority` 和 `quantum_size_ms`**：C 版的 `s_flags` + `p_priority` + `p_quantum_size_ms` 在 main.c:178-248 中是组合赋值的，本设计把它们放在 `SchedFields` 而非 `Capability`——但是否合理？
6. **FPU/XSAVE 懒加载机制**：x86-64 用 `CR0.TS` 标志实现，第一次 FP 指令触发 #NM 异常。本设计只标注"懒加载"，但没设计 `CR0.TS` 触发时的处理流程。
7. **`MF_EXT_REG_INITIALIZED` 的位置**：删除 `p_ext_reg_state` 后，这个标志位放到 `X86_64BootContext` 内部——但 fork 时如何复制？需要子进程继承"已初始化"标志。
8. **bsp_finish_booting() 与 init_proc_and_boot() 的关系**：C 版在 `kmain` 中调用 `proc_init()` + boot image 循环 + `arch_post_init()` + `memory_init()` + `system_init()` + `bsp_finish_booting()`，本设计只覆盖了 `proc_init` + boot image 循环——后续如何衔接？

### 11.2 期望从其他 AI 答案中获取的补充

- **方案 A vs B vs C 的取舍**（problem.md #10.3）：我选了"1 个 trait + 1 个类型别名"（方案 B），但其他 AI 可能选不同方案——bagging 时比较
- **Capability 是否过度抽象**：C 版的 `priv` 概念接近 capability，但我没看到 Minix3 源码中有 capability 模型术语——是否过度？
- **ProcessCategory 的 3 变体是否足够**：C 版有 IDLE/TSK/SRV/DSRV/RSYS/VM/LU/RST 8 个 flag 组合，我合并为 3 个 category——是否丢信息？
- **X86_64BootContext 是否应再拆为 x86-64 子模块**：本设计放 1 文件，但 x86-64 的 XSAVE area + segment selectors + RFLAGS 已经有 100+ 行，是否该拆？

### 11.3 与其他 AI 答案潜在冲突的预判

| 议题 | 我的立场 | 可能冲突 |
|------|---------|---------|
| trait 数量 | 1 个 `ArchProcessInit` | 可能有人选 2 个（reset + init） |
| 类型别名 vs 关联类型 | 类型别名 `CurrentBootContext` | 可能有人选关联类型 `<CurrentArch as ...>::Context` |
| `BootContext` 是否 arch 内字段可见 | 全部 `pub` 但 OS 不用 | 可能有人设 `pub(crate)` 限制可见性 |
| `p_ext_reg_state` 字段 | 彻底删除 | 可能有人保留（"fork 时复制"是真实需求）|
| KPriv 子结构粒度 | 6 个（capability/signals/io/mem/irq/grant）| 可能有人 4-5 个 |
| `configure_for(category)` vs `configure_for(category, custom_overrides)` | 仅 `category`（默认值）| 可能有人加 overrides 应对特殊情况 |
| ELF 加载位置 | 自由函数 in `os/kernel/src/elf_boot.rs` | 可能有人放 `os/libs/minix-elf-loader` crate |

---

## 附录 A：本设计与 problem.md 三大原则的对齐

| 原则 | 本设计落实情况 |
|------|--------------|
| **原则 1：不允许硬件语义泄露到 OS 层** | ✅ `CurrentBootContext` 是 type alias（编译时定型）；`SegmentSelectors`/`fpu_needs_zero` 从 OS 层消失；x86-64 RFLAGS IOPL 操作下沉到 `X86_64Arch::enable_iopl_for_process`；aarch64/riscv64 编译时不存在 x86-64 字段 |
| **原则 2：不允许 translate，必须 rewrite** | ✅ `ProcessCategory`/`Capability`/`BootContext` 全部是 OS 概念；6 裸参数 → 1 枚举；命名来自 OS 而非 C；不再有"对应 C 函数"注释 |
| **原则 3：boot 阶段无堆** | ✅ `ProcessTable: [KProcess; PROC_TABLE_SIZE]` + `PrivTable: [KPriv; NR_SYS_PROCS]`；`new()` 改 `const fn`；`p_ext_reg_state` 删除（节省 144KB）|

## 附录 B：本设计与 review-rules 模式的对齐

| 模式 | 描述 | 本设计落实 |
|------|------|----------|
| 14 | 硬件未抽象为 trait | ✅ `Paging`/`ArchProcessInit` 全部 trait 化 |
| 16 | 裸整数表达语义 | ✅ `TrapMask`/`SyscallBitmap`/`IpcBitmap` newtype 包装 |
| 17 | C 式空指针/哨兵值 | ✅ 删除 `SegmentSelectors::default()` 哨兵 |
| 18 | unsafe 滥用 | ⚠️ ELF 加载仍有 `unsafe`（paging.map 后 copy），但加 SAFETY 注释 |
| 21 | 硬件语义泄漏 | ✅ x86-64 特有字段全在 `X86_64BootContext` 内部 |
| 22 | no_std 违规 | ✅ 全部用 `[T; N]` 替代 `Box<[T]>` |
| 25 | 不必要的 trait | ✅ 3 个 trait → 1 个；`load_vm_elf` 改自由函数 |
| 30 | 外部知识误导 | ✅ 删除 "fpu_needs_zero 因 arch 无法访问 FPU 保存区" 的编造 |
| 31 | 通用接口含上下文特定元素 | ✅ `CurrentBootContext` 编译时定型，跨 arch 不共享 |
| 32 | 外部调用返回值被无说明忽略 | ⚠️ ELF 加载的错误处理需补充 |
| 33 | 资源获取后无释放路径 | ✅ ProcessTable 静态分配，无释放问题 |
| 34 | 注释理由虚假 | ✅ 删除所有"对应 C 函数"的解释，改为 OS 概念 |
| 51 | 实现驱动概念章 | ✅ Ch3 §3 全部概念驱动，删除 "对应 C 函数" 模式 |

---

**最后：本设计是独立完成的，未参考 06-design-ds/glm/kimi/m3/qwen/seed 中任何其他 AI 的答案。bagging 时请对比方案差异。**

