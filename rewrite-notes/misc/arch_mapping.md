# 架构机制映射表：从硬件描述到机制抽象

> **核心思想**：抽象机制，而不是描述硬件。机制应该是 trait，不同架构实现这个 trait。

---

## 一、为什么需要机制抽象？

### 1.1 传统 C 风格的问题

在传统的操作系统代码中（如 Minix 3），我们经常看到这样的命名：

```c
init_TSS();        // 初始化任务状态段
load_cr3();        // 加载页表基址
cli() / sti();     // 关中断 / 开中断
set_ldt();         // 设置局部描述符表
```

**问题**：
- 这些名字直接描述了硬件细节（TSS、CR3、LDT）
- 代码锁死在特定架构（x86）上
- 移植到其他架构需要重写所有代码

### 1.2 机制抽象的优势

如果我们用机制命名，代码的生命力会完全不同：

| 硬件描述 (C 风格) | 机制抽象 (Rust Trait 风格) | 机制的本质逻辑 |
|-------------------|----------------------------|----------------|
| `init_TSS()` | `impl PrivilegeStackSwitcher for Arch` | 处理从 Ring3 进入 Ring0 时内核栈的来源 |
| `load_cr3()` | `impl AddressSpaceSwitcher for Arch` | 切换内存视图，即切换页表根路径 |
| `cli() / sti()` | `impl InterruptController for Arch` | 保证当前代码序列的原子性（不可抢占性） |
| `set_ldt()` | `impl ThreadLocalStorage for Arch` | 为每个线程提供独立的内存上下文 |

**优势**：
- **命名即正义**：名字描述的是"做什么"，而不是"怎么做"
- **架构无关**：trait 定义机制，不同架构实现细节
- **易于理解**：新人不需要了解 TSS 就能理解"特权级栈切换"

---

## 二、为什么必须是静态分发？

在内核最底层的汇编胶水层，**动态分发（`dyn Trait` / 虚函数表）是绝对的禁忌**：

### 2.1 引导阶段限制

在 MMU 开启前或堆栈初始化前，根本没有运行环境去解析虚表（vtable）。

### 2.2 性能开销

内核底层的切换是按"时钟周期"计费的：
- 一次间接跳转（Indirect Branch）不仅慢
- 还会破坏 CPU 的分支预测器
- 甚至引入 Spectre 漏洞

### 2.3 内联优化

静态分发允许 Rust 编译器将不同架构的实现直接内联到调用处：

```rust
// 编译前
arch_switch_context(from, to);

// 编译后（x86_64）
asm!("mov {0}, rsp", "mov rsp, {1}", ...);

// 编译后（RISC-V）
asm!("csrrw sp, sscratch, sp", ...);
```

这样，`arch_switch_context` 在编译后就直接是几条汇编指令，没有任何函数调用开销。

---

## 三、如何在 Rust 中实现机制抽象？

### 3.1 定义机制 Trait

```rust
/// 定义内核必须具备的"机制"
pub trait ArchPrivilegeManager {
    type Context; // 硬件相关的上下文结构
    
    /// 机制：进入特权模式前准备好内核栈
    fn prepare_kernel_stack(&self, stack_top: VAddr);
    
    /// 机制：原子性地保存并切换上下文
    unsafe fn switch_context(from: *mut Self::Context, to: *const Self::Context);
}
```

### 3.2 不同架构的实现

```rust
// x86_64 的实现
struct X86_64;
impl ArchPrivilegeManager for X86_64 {
    type Context = X86Context;
    
    fn prepare_kernel_stack(&self, stack_top: VAddr) {
        // 这里才是处理 TSS 的地方，被封装在机制之后
        TSS.set_rsp0(stack_top);
    }
    
    unsafe fn switch_context(from: *mut Self::Context, to: *const Self::Context) {
        asm!(
            "pushad",
            "mov {0}, esp",
            "mov esp, {1}",
            "popad",
            in(reg) from,
            in(reg) to,
        );
    }
}

// RISC-V 的实现
struct RiscV64;
impl ArchPrivilegeManager for RiscV64 {
    type Context = RiscVContext;
    
    fn prepare_kernel_stack(&self, stack_top: VAddr) {
        // RISC-V 使用 sscratch 寄存器，不需要提前设置
        // 在中断入口时手动交换
    }
    
    unsafe fn switch_context(from: *mut Self::Context, to: *const Self::Context) {
        asm!(
            "csrrw sp, sscratch, sp",  // 交换用户栈和内核栈
            "sd ra, 0(sp)",
            // ... 保存其他寄存器
        );
    }
}
```

### 3.3 编译期架构选择

```rust
// 根据编译目标，自动选择架构实现
#[cfg(target_arch = "x86_64")]
pub type CurrentArch = x86::X86Manager;

#[cfg(target_arch = "riscv64")]
pub type CurrentArch = riscv::RiscvManager;

#[cfg(target_arch = "aarch64")]
pub type CurrentArch = arm::Aarch64Manager;

// 统一的机制调用，没有任何 runtime 开销
fn handle_trap() {
    CurrentArch::save_context(); // 静态分发，直接内联
}
```

---

## 四、核心机制架构映射表

这是**架构师视角**下的"终极版图"。这张表揭示了一个深刻的真相：**虽然硬件的命令字（Instruction）千差万别，但人类对操作系统的诉求（Mechanism）在过去四十年里几乎没变过。**

### 4.1 特权级与上下文管理

| 机制 (Mechanism) | 机制的本质目的 | x86_64 实现 | AArch64 (ARMv8) 实现 | RISC-V 实现 |
|------------------|----------------|-------------|----------------------|-------------|
| **Privilege Transition Stack** | 确保从低特权级进入内核时，有一个可信的栈 | **TSS.RSP0** (硬件自动加载) | **SP_EL1** (硬件自动切换) | **sscratch** (手动交换 SP) |
| **Exception Vectoring** | 硬件出事或系统调用时，代码往哪跳？ | **IDT** (中断描述符表) | **VBAR_EL1** (向量基址) | **stvec** (向量基址) |
| **Execution State Context** | 进程"活"着的全部物理证据（寄存器） | **GPRs + RFLAGS** | **X0-X30 + PSTATE** | **X1-X31 + sstatus** |
| **System Call Entry** | 用户态请求内核服务的受控通道 | **SYSCALL / SYSRET** | **SVC** 指令 | **ecall** 指令 |

### 4.2 内存管理

| 机制 (Mechanism) | 机制的本质目的 | x86_64 实现 | AArch64 (ARMv8) 实现 | RISC-V 实现 |
|------------------|----------------|-------------|----------------------|-------------|
| **Address Space Root** | 定义当前进程能看到哪块内存 | **CR3** 寄存器 | **TTBR0_EL1** 寄存器 | **satp** 寄存器 |
| **Page Table Entry Flags** | 页的权限和属性（读/写/执行/缓存） | **PTE** (Present, RW, US, etc.) | **Block/Page Descriptors** | **PTE** (R, W, X, etc.) |
| **TLB Management** | 地址转换缓存失效 | **invlpg / cr3 reload** | **TLBI** 指令 | **sfence.vma** 指令 |

### 4.3 中断与异常

| 机制 (Mechanism) | 机制的本质目的 | x86_64 实现 | AArch64 (ARMv8) 实现 | RISC-V 实现 |
|------------------|----------------|-------------|----------------------|-------------|
| **Interrupt Enable/Disable** | 保证当前代码序列的原子性 | **cli / sti** (EFLAGS.IF) | **msr DAIF** (掩码) | **csrrc/csrri sstatus** (SIE) |
| **Interrupt Controller** | 管理外部中断源 | **APIC / IOAPIC** | **GIC** (Generic Interrupt Controller) | **PLIC** (Platform Level Interrupt Controller) |
| **Timer Interrupt** | 定时器中断，用于调度 | **LAPIC Timer** | **Generic Timer** | **mtime / mtimecmp** |
| **Nested Interrupt Handling** | 中断嵌套的栈管理 | **k_reenter** 计数器 | **SP_EL0 / SP_EL1** 切换 | **sscratch** 检查 |

### 4.4 线程与同步

| 机制 (Mechanism) | 机制的本质目的 | x86_64 实现 | AArch64 (ARMv8) 实现 | RISC-V 实现 |
|------------------|----------------|-------------|----------------------|-------------|
| **Thread Local Base** | 给每个线程一个私有的"储物柜" | **FS / GS** 段寄存器 | **TPIDR_EL0** 寄存器 | **tp** (x4) 寄存器 |
| **Atomic Operations** | 无锁数据结构的基础 | **LOCK** 前缀 + CMPXCHG | **LDXR / STXR** (LL/SC) | **amo*** 指令 |
| **Memory Barriers** | 保证内存操作的顺序性 | **mfence / sfence / lfence** | **dmb / dsb / isb** | **fence** 指令 |
| **Spinlock Implementation** | 自旋锁的底层实现 | **xchg** 或 **cmpxchg** | **LDAXR / STLXR** | **amoswap** |

### 4.5 性能与调试

| 机制 (Mechanism) | 机制的本质目的 | x86_64 实现 | AArch64 (ARMv8) 实现 | RISC-V 实现 |
|------------------|----------------|-------------|----------------------|-------------|
| **Performance Counters** | 性能监控（周期、缓存未命中等） | **MSR** 寄存器 (PERFCTR) | **PMU** (Performance Monitors) | **hpmcounter*** 寄存器 |
| **Breakpoints** | 软件断点支持 | **INT3** 指令 / **DR0-DR3** | **BRK** 指令 | **ebreak** 指令 |
| **Watchpoints** | 硬件数据断点 | **DR0-DR3** + **DR7** | **DBGBCR / DBGBVR** | **Trigger Module** |

---

## 五、架构差异深度分析

### 5.1 特权级栈切换：TSS vs sscratch

这是最能体现架构哲学差异的例子：

#### x86_64：管家式服务

```asm
; 用户态 → 内核态（硬件自动完成）
; 1. CPU 从 TSS.RSP0 读取内核栈指针
; 2. CPU 自动压入 SS, RSP, RFLAGS, CS, RIP
; 3. CPU 切换到内核栈
; 4. CPU 跳转到 IDT 中的处理程序

; 软件只需要提前设置 TSS
mov [tss + TSS_RSP0], kernel_stack_top
```

**特点**：
- 硬件做了很多工作
- 需要配置复杂的 TSS 结构
- 性能好（硬件优化）
- 但灵活性差

#### RISC-V：极简主义

```asm
; 用户态 → 内核态（软件手动完成）
; 1. CPU 跳转到 stvec
; 2. 软件手动交换 SP 和 sscratch
csrrw sp, sscratch, sp  ; sp = sscratch, sscratch = sp
                        ; 现在 sp 是内核栈，sscratch 是用户栈

; 3. 软件手动保存寄存器
sd ra, 0(sp)
sd s0, 8(sp)
; ...
```

**特点**：
- 硬件几乎不做任何事
- 软件完全控制
- 灵活性高
- 但需要更多代码

#### AArch64：中间路线

```asm
; 用户态 → 内核态（硬件部分自动）
; 1. CPU 自动切换到 SP_EL1（内核栈）
; 2. CPU 自动保存部分状态到 SPSR_EL1, ELR_EL1
; 3. CPU 跳转到 VBAR_EL1 + offset

; 软件只需要保存通用寄存器
stp x0, x1, [sp, #-16]!
stp x2, x3, [sp, #-16]!
; ...
```

**特点**：
- 硬件和软件分工明确
- 确定的 EL（Exception Level）分层
- 平衡了性能和灵活性

### 5.2 中断使能/禁用：EFLAGS.IF vs DAIF vs sstatus.SIE

这是内核死锁的常见罪魁祸首：

#### x86_64：简单直接

```asm
cli                    ; 清除 EFLAGS.IF，禁用中断
; ... 临界区代码 ...
sti                    ; 设置 EFLAGS.IF，启用中断

; 问题：无法禁用 NMI（不可屏蔽中断）
```

#### AArch64：掩码控制

```asm
msr DAIFSet, #2        ; 设置 I 位，禁用 IRQ
; ... 临界区代码 ...
msr DAIFClr, #2        ; 清除 I 位，启用 IRQ

; 优势：可以单独控制 D (Debug), A (SError), I (IRQ), F (FIQ)
```

#### RISC-V：CSR 位操作

```asm
csrrc sstatus, sstatus, 2  ; 清除 SIE 位，禁用中断
; ... 临界区代码 ...
csrrs sstatus, sstatus, 2  ; 设置 SIE 位，启用中断

; 优势：可以原子地读取-修改-写入
```

---

## 六、Rust 实现示例

### 6.1 中断保护（InterruptGuard）

```rust
/// 自动管理中断使能状态的 RAII 守卫
pub struct InterruptGuard {
    was_enabled: bool,
}

impl InterruptGuard {
    /// 禁用中断并返回守卫
    pub fn new() -> Self {
        let was_enabled = CurrentArch::interrupts_enabled();
        CurrentArch::disable_interrupts();
        Self { was_enabled }
    }
}

impl Drop for InterruptGuard {
    /// 守卫离开作用域时自动恢复中断状态
    fn drop(&mut self) {
        if self.was_enabled {
            CurrentArch::enable_interrupts();
        }
    }
}

// 使用示例
fn critical_section() {
    let _guard = InterruptGuard::new();
    // 这里中断被禁用
    do_something_critical();
    // _guard 离开作用域，自动恢复中断状态
}
```

### 6.2 上下文切换抽象

```rust
pub trait ContextSwitcher {
    type Context;
    
    /// 保存当前上下文
    unsafe fn save(ctx: &mut Self::Context);
    
    /// 恢复目标上下文
    unsafe fn restore(ctx: &Self::Context);
    
    /// 切换上下文（原子操作）
    unsafe fn switch(from: &mut Self::Context, to: &Self::Context);
}

// x86_64 实现
impl ContextSwitcher for X86_64 {
    type Context = X86Context;
    
    unsafe fn save(ctx: &mut Self::Context) {
        asm!(
            "pushad",
            "mov {0}, esp",
            out(reg) ctx.esp,
        );
    }
    
    unsafe fn restore(ctx: &Self::Context) {
        asm!(
            "mov esp, {0}",
            "popad",
            in(reg) ctx.esp,
        );
    }
    
    unsafe fn switch(from: &mut Self::Context, to: &Self::Context) {
        Self::save(from);
        Self::restore(to);
    }
}
```

---

## 七、读 .S 文件的策略

### 7.1 剥离汇编

只保留最精简的、Rust 无法表达的汇编（如修改 `cr3` 或 `sret`）。

### 7.2 裸函数（Naked Functions）

利用 Rust 的 `#[naked]` 属性，直接在 Rust 里写汇编：

```rust
#[naked]
pub unsafe extern "C" fn trap_entry() {
    asm!(
        // 保存上下文
        "push rax",
        "push rbx",
        // ...
        
        // 调用 Rust 处理函数
        "call {handler}",
        
        // 恢复上下文
        "pop rbx",
        "pop rax",
        "iretq",
        
        handler = sym handle_trap,
        options(noreturn),
    );
}
```

### 7.3 统一入口

不管什么架构，汇编只负责把寄存器"dump"到内存，然后立刻跳进一个统一命名的 Rust 函数：

```rust
// 所有架构的统一入口
fn handle_trap(ctx: &mut Context) {
    match ctx.trap_type {
        TrapType::Interrupt => handle_interrupt(ctx),
        TrapType::Syscall => handle_syscall(ctx),
        TrapType::PageFault => handle_page_fault(ctx),
        // ...
    }
}
```

---

## 八、总结

### 8.1 核心价值

这张架构映射表的价值在于：

1. **强制解耦**：代码不再绑定到特定硬件
2. **识别代差**：一眼看出不同架构的设计哲学
3. **教学价值**：帮助新人理解操作系统本质
4. **移植指南**：为移植到新架构提供路线图

### 8.2 设计哲学

- **x86**：管家式服务，硬件帮你做很多，但配置复杂
- **ARM**：中间路线，硬件和软件分工明确
- **RISC-V**：极简主义，硬件几乎不做，软件完全控制

### 8.3 Rust 的优势

- **零成本抽象**：trait + 静态分发 = 无运行时开销
- **类型安全**：编译期捕获错误
- **内联优化**：编译后直接是汇编指令

---

## 九、下一步

1. **补全表格**：继续添加更多机制（如虚拟化、安全扩展等）
2. **实现代码**：为每个机制编写 Rust trait 和不同架构的实现
3. **文档化**：为每个机制编写详细的设计文档
4. **测试**：编写跨架构的测试用例

---

**如果你把这个表格做完了，这就是一份超越了 Minix 的、通用的现代内核设计指南。**
