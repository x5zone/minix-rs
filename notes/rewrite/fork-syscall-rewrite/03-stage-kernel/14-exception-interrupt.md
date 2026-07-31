# 14-exception-interrupt: 异常与中断处理

> **源码**: `minix3/minix/kernel/arch/i386/exception.c`, `minix3/minix/kernel/interrupt.c`, `minix3/minix/kernel/arch/i386/hw_intr.h`
> **Rust 实现**: `os/arch/src/arch/exception.rs`, `os/arch/src/arch/exception_dispatcher.rs`, `os/kernel/src/irq_manager.rs`, `os/kernel/src/page_fault.rs`
> **前置**: 03/05（保护模式 + 中断控制器基建）、10（switch_to_user 枢纽）、11（RTS 状态机）、12（IPC mini_notify）、13（sys_call 入口与 m_type 路由）
> **下游**: 15（时钟中断与定时器）、16（SMP/BKL）

---

## Ch1: 概念

### 1.1 两类意外事件：异常 vs 中断

CPU 在执行指令流时，会被两类"意外事件"打断：

| 类型 | 触发者 | 与当前指令关系 | 可否 retry | OS 术语 |
|------|--------|--------------|-----------|---------|
| **异常（Exception）** | 指令本身 | 同步——执行某条指令时立即触发 | 部分可（缺页处理后重执行） | 异常/陷阱/故障 |
| **中断（Interrupt）** | 外部设备 | 异步——与当前指令无关，随时到达 | 不可（设备状态已变） | 硬件中断 |

本质区别：**异常是"我错了"——CPU 执行了有问题的指令；中断是"你有事"——设备通知 CPU 来处理**。这个区别决定了内核处理方式的分流：异常可能需要给进程发信号或转发给 VM 修页；中断只需要唤醒等待的进程。

承接 05：本章假设中断控制器（8259A/IOAPIC）已初始化、IDT/VBAR/stvec 已填好入口。本章回答"入口之后内核做什么"。

### 1.2 三条激活路径与统一出口

内核没有主循环，只有三种被激活的方式。无论哪条路径进入，最终都汇聚到同一个出口：

| 路径 | 入口 | 触发 | 处理 | 出口 |
|------|------|------|------|------|
| 硬件中断 | IDT[IRQ+32] → `irq_handle` | 外部设备异步 | 遍历 hook 链 → `mini_notify` 唤醒进程 | switch_to_user |
| CPU 异常 | IDT[vec] → `exception_handler` | 当前指令导致 | 用户态→cause_sig / 页错误→转发VM / 内核态→panic | switch_to_user |
| 系统调用 | INT 0x80/SYSCALL/SVC → `sys_call` | 用户进程主动 | kernel_call / do_ipc（详见 13） | switch_to_user |

**三条路径的出口都是 `switch_to_user()`**（详见 10）。这验证了 10 文档的结论：switch_to_user 是内核运行时的核心枢纽，它不关心"为什么进入内核"，只关心"接下来该运行谁"。

> **边界说明**：系统调用入口机制（INT 0x80/SYSENTER/SYSCALL/SVC/ecall 的选择与 CPU 特性检测）见附录 C；`m_type` 路由与 `KERNEL_CALL=0x600` 归一化见 13-syscall-dispatch。本章只覆盖"CPU 如何进入内核 + 进入后异常/中断如何分流"。

时钟中断是硬件中断的一个特例，但因其驱动调度的特殊地位单独成章——时钟中断处理函数内部、量子管理、同步闹钟、虚拟/性能定时器见 **15-clock-timer**。本章 §1.2 仅把时钟中断定位为"三条激活路径之一"。

### 1.3 异常帧：CPU 压了什么

异常发生时，CPU 自动把关键寄存器压入当前栈，形成**异常帧（exception frame）**：

| 字段 | 含义 | 压入者 |
|------|------|--------|
| `vector` | 异常向量号 | 汇编入口（mpx.S） |
| `errcode` | 错误码（无错误码的异常由汇编压入 0） | CPU 或汇编入口 |
| `eip`/`rip` | 异常发生时的指令指针 | CPU |
| `cs` | 异常发生时的代码段 | CPU |
| `eflags`/`rflags` | 异常发生时的标志寄存器 | CPU |
| `esp`/`rsp` | 异常发生时的栈指针（**嵌套异常时无效**） | CPU |
| `ss` | 异常发生时的栈段（**嵌套异常时无效**） | CPU |

**嵌套异常**是关键概念：当异常在内核态发生时（`is_nested=1`），CPU 不发生栈切换，因此**不会压入 `esp`/`ss`**——这两个字段是栈上的残留数据，不可使用。嵌套异常必须特殊处理（见 §2.2），否则会因无法恢复而 panic。

### 1.4 中断分流的三问框架（跨架构统一抽象）

不同架构的异常/中断机制表面差异很大，但 CPU 都需要回答三个问题：

| CPU 问题 | x86-64 | aarch64 | riscv64 |
|---------|--------|---------|---------|
| ① 当前特权级？ | ring 0/3（CS.DPL） | EL0/EL1 | S/U-mode |
| ② 异常入口在哪？ | IDT | VBAR_EL1 | stvec |
| ③ 内核栈在哪？ | TSS.sp0 | SP_EL1 | sscratch |
| 页错误地址？ | CR2 | FAR_EL1 | stval |
| 页错误向量？ | 14 | Prefetch/Data Abort | scause=12/13/15 |
| 系统调用入口？ | INT 0x80/SYSCALL | SVC | ecall |

**统一抽象先行**：OS 策略（分流、信号映射、页错误转发、IRQ hook 管理）在所有架构上相同，只有"读取 CR2/FAR/stval""判断用户态""修改 rip 返回"等硬件操作不同。因此 Rust 把硬件操作抽象为 trait（§3.3），OS 策略写成架构无关代码（§3.2）。这避免了 `#[cfg(target_arch)]` 在策略代码中散落。

### 1.5 页错误的特殊性

页错误（x86 向量 14）是最复杂的异常，**不走通用的"发信号"路径**，而有独立处理：

四层分解：

| 层次 | 问题 | C 机制 |
|------|------|--------|
| 状态定义 | 缺页进程的状态？ | RTS_PAGEFAULT + p_vmrequest |
| 状态转换 | 缺页如何触发/恢复？ | exception → mini_send(VM) → VMCTL_CLEAR_PAGEFAULT |
| 事件源 | 什么触发缺页？ | 用户态访问未映射页 / 内核态 cross_space_copy |
| 服务 | VM 如何处理？ | 分配物理页 → 映射 → 通知内核恢复进程 |

**VM 自身不能缺页**：如果 VM 进程触发页错误，内核直接 panic。这是设计上的必然——VM 负责为所有进程管理内存，如果 VM 自身需要缺页处理，就形成了循环依赖（谁来处理 VM 的缺页？）。详见 §2.3。

---

## Ch2: C 源码分析

### 2.0 Claims-Evidence

| Claim | Evidence | Status |
|-------|----------|--------|
| x86 异常表有 20 项 | `exception.c:19-39` ex_data[] | ✅ verified |
| 异常分流主干五分支 | `exception.c:180-283` exception_handler | ✅ verified |
| 页错误设置 RTS_PAGEFAULT | `exception.c:116` | ✅ verified |
| 页错误通过 mini_send 通知 VM | `exception.c:124-125` | ✅ verified |
| VM 页错误不可处理 → panic | `exception.c:110` | ✅ verified |
| 用户态异常 → cause_sig | `exception.c:275` | ✅ verified |
| 内核态异常 → inkernel_disaster | `exception.c:280` | ✅ verified |
| 嵌套恢复用 eip 地址范围比较 | `exception.c:66-70, 206-229` | ✅ verified |
| IRQ hook 链表管理 | `interrupt.c:29-176` | ✅ verified |
| put_irq_handler unmask 条件 | `interrupt.c:65` `&= ~id` | ✅ verified |
| **timer_int_handler 不递减 quantum** | `clock.c:70-173` 无 p_cpu_time_left | ✅ verified（纠正旧 doc 错误） |

### 2.1 exception_handler 分流主干

```c
// exception.c:180-283
void exception_handler(int is_nested, struct exception_frame * frame)
{
  saved_proc = get_cpulocal_var(proc_ptr);
  ep = &ex_data[frame->vector];

  if (frame->vector == 2) return;              // ① spurious NMI

  if (is_nested) { ... 嵌套特殊处理 ... }       // ② 见 §2.2

  if (frame->vector == PAGE_FAULT_VECTOR) {    // ③ 页错误单独处理
    pagefault(saved_proc, frame, is_nested);
    return;
  }

  if (is_nested == 0 && !iskernelp(saved_proc)) {  // ④ 用户态 → 信号
    cause_sig(proc_nr(saved_proc), ep->signum);
    return;
  }

  inkernel_disaster(saved_proc, frame, ep, is_nested);  // ⑤ 内核态 → panic
}
```

五个分支按优先级排列：NMI 忽略 → 嵌套特殊 → 页错误 → 用户态信号 → 内核态 panic。这个分流顺序是 Rust `ExceptionDispatcher::handle` 的直接对应（§4.2）。

### 2.2 嵌套异常的四种恢复点

C 用**指令地址范围比较**判断嵌套异常发生在哪个"可恢复操作"中，并跳到对应的恢复点：

| 可恢复操作 | C 判断条件 | 恢复点 | 处理 |
|-----------|-----------|--------|------|
| copy_msg_to/from_user | `eip ∈ [copy_msg_to_user, __copy_msg_to_user_end]` 等 | `__user_copy_msg_pointer_failure` | 页错误/保护错误→跳恢复点 |
| fxrstor/frstor（FPU 恢复） | `eip ∈ [fxrstor, __fxrstor_end]` 等 | `__frstor_failure` | 任何异常→跳恢复点 |
| phys_copy | `eip ∈ (phys_copy, phys_copy_fault)` | `phys_copy_fault_in_kernel` | 内核态→改 eip；用户态→改 p_reg.pc |
| phys_memset | `eip ∈ (phys_memset, memset_fault)` | `memset_fault_in_kernel` | 同上 |

此外有一个特殊调试例：`DEBUG_VECTOR + TRACEBIT + KTS_NONE` 时清除 TF 标志位返回（`exception.c:232-250`），因为被 traced 的进程经 sysenter/syscall 入核时 TF 未清。

**C 的地址范围比较的缺陷**：依赖符号地址、跨架构符号名不同、不类型安全。Rust 用 `FaultContext` enum 替代（§3.4）。

### 2.3 pagefault 转发 VM

```c
// exception.c:49-130
static void pagefault(struct proc *pr, struct exception_frame *frame, int is_nested)
{
  reg_t pagefaultcr2 = read_cr2();

  // 内核态 phys_copy/memset 中的页错误 → 特殊恢复（见 §2.2）
  if ((is_nested || iskernelp(pr)) && catch_pagefaults &&
      (in_physcopy || in_memset)) { ... return; }

  // 嵌套页错误（非 physcopy）→ panic
  if (is_nested) { inkernel_disaster(pr, frame, NULL, is_nested); }

  // VM 进程页错误 → panic（循环依赖）
  if (pr->p_endpoint == VM_PROC_NR) { panic("pagefault in VM"); }

  // 阻塞进程直到缺页处理完
  RTS_SET(pr, RTS_PAGEFAULT);

  // 构造 VM_PAGEFAULT 消息通知 VM
  m_pagefault.m_source = pr->p_endpoint;
  m_pagefault.m_type   = VM_PAGEFAULT;
  m_pagefault.VPF_ADDR = pagefaultcr2;
  m_pagefault.VPF_FLAGS = frame->errcode;
  mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL);
}
```

**关键不变量**：① VM 缺页必 panic；② 转发 VM 前先设 RTS_PAGEFAULT 阻塞，防止进程被重新调度；③ 用 `mini_send` 而非 `send`，因为内核态不能阻塞等待。

### 2.4 ex_data[] 异常→信号映射

| 向量 | 异常名 | 信号 | 最低处理器 |
|------|--------|------|-----------|
| 0 | Divide error | SIGFPE | 86 |
| 1 | Debug exception | SIGTRAP | 86 |
| 2 | Nonmaskable interrupt | SIGBUS | 86 |
| 3 | Breakpoint | SIGEMT | 86 |
| 4 | Overflow | SIGFPE | 86 |
| 5 | Bounds check | SIGFPE | 186 |
| 6 | Invalid opcode | SIGILL | 186 |
| 7 | Coprocessor not available | SIGFPE | 186 |
| 8 | Double fault | SIGBUS | 286 |
| 9 | Coprocessor segment overrun | SIGSEGV | 286 |
| 10 | Invalid TSS | SIGSEGV | 286 |
| 11 | Segment not present | SIGSEGV | 286 |
| 12 | Stack exception | SIGSEGV | 286 |
| 13 | General protection | SIGSEGV | 286 |
| 14 | Page fault | SIGSEGV | 386 |
| 15 | (reserved) | SIGILL | 0 |
| 16 | Coprocessor error | SIGFPE | 386 |
| 17 | Alignment check | SIGBUS | 386 |
| 18 | Machine check | SIGBUS | 386 |
| 19 | SIMD exception | SIGFPE | 386 |

C 源码: `exception.c:19-39`。页错误虽映射 SIGSEGV，但走独立路径（§2.3），不经过 cause_sig。

### 2.5 IRQ hook 链管理

Minix3 允许同一 IRQ 线被多个驱动共享。每个驱动注册一个 `irq_hook_t`，链表组织：

```c
// interrupt.c:29-69 — put_irq_handler()
void put_irq_handler(irq_hook_t* hook, int irq, const irq_handler_t handler)
{
  // 遍历链表找到尾部，收集已用 ID 位图
  // 分配最低未用位作为 id (1, 2, 4, 8, ...)
  // 追加到链表尾部
  // 清除本 hook 的 actid 位；若整个 actids[irq]==0 → unmask
  if((irq_actids[hook->irq] &= ~hook->id) == 0) {  // interrupt.c:65
    hw_intr_used(irq); hw_intr_unmask(hook->irq);
  }
}

// interrupt.c:116-158 — irq_handle()
void irq_handle(int irq)
{
  hw_intr_mask(irq);                       // 先屏蔽
  hook = irq_handlers[irq];
  if (hook == NULL) return;                // spurious：保持屏蔽
  while (hook != NULL) {
    irq_actids[irq] |= hook->id;           // 标记活跃
    if ((*hook->handler)(hook))            // 返回非零=完成
      irq_actids[hook->irq] &= ~hook->id;  // 清除活跃
    hook = hook->next;
  }
  if (irq_actids[irq] == 0) hw_intr_unmask(irq);  // 全部完成 → 解除屏蔽
  hw_intr_ack(irq);                        // EOI
}
```

**id 位掩码机制**：每个 hook 分配一个 2 的幂次 id（1/2/4/8...），用于在 `irq_actids[]` 中标记该 hook 是否活跃（未完成）。只有当某 IRQ 的所有 hook 都完成（actids==0）才 unmask。这保证共享 IRQ 中某个 handler 未完成时不丢失中断。

### 2.6 中断控制器抽象（hw_intr 宏）

Minix3 通过宏抽象中断控制器操作（`hw_intr.h`），支持 8259A PIC 和 IOAPIC：

| 宏 | 8259A PIC | IOAPIC |
|----|-----------|--------|
| `hw_intr_mask(irq)` | `irq_8259_mask` | `ioapic_mask_irq` |
| `hw_intr_unmask(irq)` | `irq_8259_unmask` | `ioapic_unmask_irq` |
| `hw_intr_ack(irq)` | `irq_8259_eoi` | `ioapic_eoi` |
| `hw_intr_used`/`not_used` | 空 | 设置 IRQ 路由 |

8259A 的 `hw_intr_used` 为空：8259A 是固定 16 个 IRQ，无需动态配置路由；IOAPIC 需显式设置 IRQ 路由到哪个 CPU。这是**策略差异**（路由配置），Rust 通过 `InterruptController` trait 注入（§3.7）。

### 2.7 三架构差异表

| 维度 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 异常入口 | IDT | VBAR_EL1 | stvec |
| 页错误向量 | 14 | Prefetch/Data Abort | scause=12/13/15 |
| 系统调用入口 | INT 0x80/SYSCALL | SVC | ecall |
| 故障地址寄存器 | CR2 | FAR_EL1 | stval |
| 错误码 | errcode（CPU 压入） | ESR_EL1.ISS | stval（复用） |
| 当前特权级 | CS.DPL | SPSR_EL1.M | sstatus.SPP |

统一抽象见 §1.4 三问框架。各架构如何回答三问的具体实现见 `os/arch/src/{x86_64,aarch64,riscv64}/`。

---

## Ch3: Rust 设计决策

### 3.1 设计目标与约束

- **no_std**：arch + kernel 全 `no_std`（test 除外）
- **硬件全 trait**：异常帧/故障地址寄存器/中断控制器操作全部 trait，OS 策略不出现 `#[cfg(target_arch)]` 选行为
- **SMP/BKL**：IrqManager 在 BKL 下访问；ExceptionDispatcher 无可变全局状态（BKL 由汇编 trap 入口获取，见 16）
- **不 translate**：用 enum/newtype/match 重新表达，禁止 1:1 翻译 C 的函数指针数组/裸 int 返回/地址范围比较

### 3.2 决策 D1：异常分流主干（多方案优中选优）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 保留 C 式全局函数** | `fn handle_exception(info) -> int` | 与 C 1:1 | 裸 int 返回失类型安全、无法表达"转发 VM""跳恢复点"等不同动作 |
| **B. ExceptionDispatcher<EA> 返回 ExceptionOutcome enum** | 分流逻辑是 arch 无关结构体，泛型注入 EA: ExceptionArch，返回 enum 区分七种结果 | 分流与执行分离、enum 穷尽、可单测 | 调用方需 match outcome |
| **C. 直接在 dispatcher 内 cause_sig/mini_send** | 与 C 一样在分流时直接执行副作用 | 与 C 一致 | dispatcher 依赖 IPC engine/proc table，循环依赖、不可单测 |

**选定 B**：`ExceptionDispatcher` 只"决定做什么"，返回 `ExceptionOutcome`；实际发信号/转发 VM/panic 由调用方执行。分流逻辑放 `arch/src/arch/exception_dispatcher.rs`（arch crate 含 arch 无关抽象，依赖 `ExceptionArch` trait）。

### 3.3 决策 D2：异常帧抽象（ExceptionArch trait）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 共用 struct ExceptionFrame** | 三架构共用一个 struct 含所有字段 | 简单 | aarch64/riscv64 字段浪费、x86 的 esp/ss 在嵌套时无效却仍存在 |
| **B. associated type Frame + ExceptionArch trait** | 每架构自定义 Frame，trait 提取共性方法 | 无浪费、类型安全 | trait 略复杂 |

**选定 B**：`ExceptionArch` trait（`os/arch/src/arch/exception.rs`）。关键方法 `page_fault_address()` 是静态的（无 `&frame`）——因为 x86 的故障地址在 CR2 寄存器而非 frame 中，ARM/RISC-V 在 FAR/stval 寄存器。

### 3.4 决策 D3：嵌套恢复点（FaultContext + RecoveryPoint enum）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 保留 C 地址范围比较** | `frame.eip > phys_copy && frame.eip < phys_copy_fault` | 与 C 1:1 | 需符号地址比较、跨架构符号名不同、不类型安全 |
| **B. FaultContext enum + RecoveryPoint enum** | 进入可恢复操作前 set context，异常时查表跳恢复点 | 类型安全、可穷尽、无符号比较 | 需在 phys_copy/memset/fxrstor/copy_msg 入口设 context |
| **C. closure + catch_unwind** | 用 Rust panic 机制恢复 | idiomatic | panic 跨 FFI/汇编不安全，内核禁 panic unwind |

**选定 B**：`FaultContext`（Normal/PhysCopy/Memset/UserCopyMsg/FpuRestore）在进入可恢复操作前设置，`RecoveryPoint`（PhysCopyFaultInKernel 等）由 `recovery_point()` 映射。这彻底替代了 C 的地址范围比较，是**架构演进**（非 translate）。

### 3.5 决策 D4：异常→信号映射（ExceptionSignal enum + match）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. const EXCEPTION_TABLE: [ExceptionMapping; 20]** | 数组 + `signal: u32` 裸数值 | 紧凑 | 裸 u32 失类型安全、数值易错（SIGFPE=8 vs SIGTRAP=5 易混） |
| **B. ExceptionSignal enum + classify_signal match** | enum Fpe/Ill/Segv/Bus/Emt/Trap + match 穷尽 | 类型安全、编译器穷尽检查、无重复 | match 略长 |

**选定 B**：`ExceptionSignal` enum（`exception_dispatcher.rs`）。signal 数值只在最终投递信号时映射，分流阶段用类型安全的 enum。旧 doc 描述的 `EXCEPTION_TABLE` const 已在本次重写中删除（与 `classify_signal` 重复且类型更弱）。

### 3.6 决策 D5：IRQ hook 链（索引链表 + IrqAction enum）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 指针链表（如 C）** | `Box<IrqHook>` + `next: Option<Box<IrqHook>>` | 动态 | no_std 堆分配、裸指针 |
| **B. 索引链表 + 固定池** | `[Option<IrqHookSlot>; NR_IRQ_HOOKS]` + `Option<usize>` | 无堆、固定池、编译期上限 | 池满需处理 |
| **C. Vec** | 动态数组 | 简单 | no_std 需 alloc、无固定上限 |

**选定 B**：`IrqManager<IC>` 用固定池 + `Option<usize>` 索引链表。`handler: fn(...)` 而非 `Box<dyn Fn>`——handler 是静态函数指针，无需堆。`IrqAction` enum（Completed/NotCompleted）替代 C 的 int 返回约定。

### 3.7 决策 D6：IrqManager 位置与泛型

`IrqManager` 放 `kernel/src/irq_manager.rs` 而非 `arch/`——它是 OS 策略（hook 注册/分发），不是 CPU ISA 机制。唯一硬件依赖（mask/unmask/eoi）通过 `IC: InterruptController` trait 注入。这符合"机制 vs 策略分离"：trait 定义硬件机制，IrqManager 实现 OS 策略。

### 3.8 决策 D7：页错误转发（ExceptionOutcome::ForwardToVm）

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. trait PageFaultHandler** | `trait { handle_user_pagefault; handle_kernel_pagefault }` | 抽象 | 无 ≥2 行为不同的实现，违反"trait 必要性"（模式 25） |
| **B. ExceptionOutcome::ForwardToVm(VmPagefaultIn)** | 分流返回 enum，调用方投递 | 分流与投递分离、可单测 | 调用方需接 page_fault.rs |
| **C. dispatcher 内直接 mini_send** | 在 dispatcher 内发 IPC | 与 C 一致 | dispatcher 依赖 IPC engine，循环依赖、不可单测 |

**选定 B**：dispatcher 只决定"转发 VM"，实际 RTS_PAGEFAULT 设置 + 消息构造在 `kernel/src/page_fault.rs` 完成（`set_pagefault_pending` + `build_vm_pagefault_msg`），`mini_send` 由汇编 trap 入口执行。旧 doc 虚构的 `trait PageFaultHandler` 不存在，已从设计删除。

### 3.9 决策 D8：timer 不在本章

| 方案 | 描述 | 优 | 劣 |
|------|------|----|----|
| **A. 保留 timer 实现** | 如旧 doc §1.3/§4.6 | 一章全 | 与 15 重复、事实错误（timer_int_handler 不递减 quantum） |
| **B. 删除 timer 实现，仅 §1.2 一句话定位** | timer 归 15 | 边界清晰、消除重复与错误 | 读者需跳 15 |

**选定 B**：`timer_int_handler` 内部、量子管理、同步闹钟、虚拟/性能定时器全部归 **15-clock-timer**。旧 doc 的 `timer_tick` 函数（虚构，代码中不存在）已从设计删除。`timer_int_handler` 实际只做时间记账（`clock.c:70-173`），quantum 递减/NO_QUANTUM 由 switch_to_user（10）与调度原语（11）处理。

---

## Ch4: 实现

### 4.1 ExceptionArch trait + 三架构 impl

`os/arch/src/arch/exception.rs`：

```rust
pub trait ExceptionArch {
    type Frame;
    fn vector(frame: &Self::Frame) -> InterruptVector;
    fn error_code(frame: &Self::Frame) -> u64;
    fn instruction_pointer(frame: &Self::Frame) -> VirBytes;
    fn is_user_mode(frame: &Self::Frame) -> bool;
    fn page_fault_address() -> VirBytes;   // CR2/FAR_EL1/stval，静态读寄存器
    fn is_write_fault(frame: &Self::Frame) -> bool;
    fn set_instruction_pointer(frame: &mut Self::Frame, ip: VirBytes);
    fn set_return_value(frame: &mut Self::Frame, value: u64);
}
```

三架构 impl：x86_64（`os/arch/src/x86_64/exception.rs`，已实现，6 个测试通过）、aarch64、riscv64。`set_instruction_pointer`/`set_return_value` 用于嵌套恢复时重定向执行到恢复点。

同文件还定义 `FaultContext`/`RecoveryPoint` enum 与 `FaultContextTracker`（per-CPU 上下文跟踪，替代 C 的 `catch_pagefaults` 全局标志 + 地址范围比较）。

### 4.2 ExceptionDispatcher 分流

`os/arch/src/arch/exception_dispatcher.rs`：

```rust
pub struct ExceptionDispatcher<EA: ExceptionArch> { .. }

impl<EA: ExceptionArch> ExceptionDispatcher<EA> {
    pub fn handle(frame: &mut EA::Frame, is_nested: bool, is_vm: bool,
                  fault_ctx: FaultContext, is_traced: bool,
                  kern_trap_style: KernTrapStyle) -> ExceptionOutcome
    {
        // ① spurious NMI
        if vector.get() == 2 { return ExceptionOutcome::SpuriousNmi; }
        // ② 嵌套异常特殊处理（§4.3）
        if is_nested { return Self::handle_nested(..); }
        // ③ 页错误
        if vector.get() == 14 { return Self::handle_page_fault(..); }
        // ④ 用户态 → 信号
        if is_user { return Self::classify_signal(vector); }
        // ⑤ 内核态 → panic
        ExceptionOutcome::KernelPanic(vector)
    }
}
```

对应 C `exception_handler` 五分支（§2.1）。`is_traced`/`kern_trap_style` 用于嵌套调试异常的特殊判断（C `exception.c:232-250`）。

### 4.3 FaultContext / RecoveryPoint / ExceptionOutcome

```rust
pub enum FaultContext { Normal, PhysCopy, Memset, UserCopyMsg, FpuRestore }
pub enum RecoveryPoint {
    PhysCopyFaultInKernel, MemsetFaultInKernel, UserCopyMsgFailure, FpuRestoreFailure,
}
impl FaultContext {
    pub fn recovery_point(self) -> Option<RecoveryPoint> { .. }
}

pub enum ExceptionOutcome {
    SpuriousNmi,
    Signal(ExceptionSignal),
    ForwardToVm(VmPagefaultIn),       // 页错误转发 VM
    RedirectToRecovery(RecoveryPoint),// 嵌套恢复
    PhysCopyFault { fault_addr: VirBytes },
    VmPageFault,                       // VM 自身缺页 → panic
    KernelPanic(InterruptVector),
    ClearTrapFlag,                     // DEBUG+TRACEBIT+KTS_NONE
}
```

`ExceptionOutcome` 的七个变体完整覆盖 C `exception_handler` + `pagefault` 的所有出口路径。调用方 match 此 enum 执行实际副作用（发信号/转发 VM/panic）。

### 4.4 IrqManager<IC>

`os/kernel/src/irq_manager.rs` 提供 IRQ hook 管理：

- `register_hook()`：分配最低未用位 ID，追加链表，unmask（对齐 C `interrupt.c:65` 的 `&= ~id` 语义——先清位再查全部）
- `remove_hook()`：移除链表节点，清理 actid，空链表则 mask
- `dispatch(irq, notifier)`：mask → 遍历 handler 链跟踪 actid → 全部完成则 unmask → EOI
- `enable_irq()`/`disable_irq()`：单 hook 粒度使能控制

#### Handler 签名与通知链路（D9）

C 的 `generic_handler(irq_hook_t *hook)` 接收 hook 指针，handler 内部直接调用全局 `mini_notify`。Rust 重写为类型安全的 `IrqHookContext` + `IrqNotify` trait 注入：

```rust
/// IRQ handler 函数指针类型。
/// C: `int (*handler)(irq_hook_t *)` — glo.h:46
pub type IrqHandler = for<'a> fn(ctx: &'a mut IrqHookContext<'a>) -> IrqAction;

/// handler 上下文，携带 slot 信息 + 通知器引用。
pub struct IrqHookContext<'a> {
    pub irq: IrqVector,
    pub id: IrqId,
    pub proc_endpoint: Endpoint,     // C: hook->proc_nr_e
    pub notify_id: IrqNotifyId,      // C: hook->notify_id
    pub policy: IrqPolicy,           // C: hook->policy
    pub notifier: &'a mut dyn IrqNotify,
}

/// 硬件通知投递 trait（生产用 KernelNotifier，测试用 MockNotifier）。
pub trait IrqNotify {
    fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId);
}
```

`generic_notify_handler` 通过 `ctx.notifier.notify_hardware()` 复现 C 的 `priv(rp)->s_int_pending |= (1 << notify_id); mini_notify(HARDWARE, proc_endpoint)` 两侧副作用（do_irqctl.c:167-170）。`IrqNotify` trait 使通知逻辑可 mock——handler 可在不依赖全局态的测试环境中验证。

`dispatch` 方法接收 `&mut dyn IrqNotify`，为链上每个 handler 构造 `IrqHookContext` 并调用。handler 签名从 `fn(IrqVector, IrqId) -> IrqAction` 改为 `fn(&mut IrqHookContext) -> IrqAction`，让 handler 获得完整 slot 信息（`proc_endpoint`/`notify_id`/`policy`）和通知能力，而非仅 irq+id。

#### IrqManager 全局化（D10）

`IrqManager<CurrentInterruptController>` 作为全局 `static mut IRQ_MANAGER: Option<...>` 存于 `kernel/src/lib.rs`，与 `PROC_TABLE`/`PRIV_TABLE` 同模式（BKL 保护 + `unsafe fn irq_manager()` 访问器）。在 `init_clock_and_interrupts` 中构造 IC 后移入全局。

Trap 入口路径通过 `dispatch_hardware_irq(irq: IrqVector)` 进入 dispatch：

```rust
pub fn dispatch_hardware_irq(irq: IrqVector) -> Result<(), IrqError> {
    let mut notifier = KernelNotifier;
    unsafe { crate::irq_manager() }.dispatch(irq, &mut notifier)
}
```

`KernelNotifier` 实现 `IrqNotify`：先在 `priv_table` 中置 `s_int_pending` 位，再调 `kernel_mini_notify(KERNEL, dst)` 投递通知。`kernel_mini_notify` 是 `mini_notify_core` 的全局表封装，与 syscall 路径共用同一通知逻辑（单一真相）。

#### KernelNotifier 与 C `generic_handler` 的两处已知缺口

C `generic_handler`（do_irqctl.c:145-172）在投递通知前还有两步副作用，Rust 当前未完整复现，记录如下：

| C 行 | C 代码 | Rust 实现 | 缺口类型 | 处理计划 |
|------|--------|----------|---------|---------|
| do_irqctl.c:154 | `get_randomness(&krandom, hook->irq)` | ❌ 未实现（`KernelNotifier::notify_hardware` 仅以注释标注） | 已知缺口 | 推迟到独立 `krandom` 子系统落地（minix3 `krandom` 是 `/dev/random` 的熵池，跨越 kernel/PM/VFS 多服务，不属于本章异常/中断分流职责）。Rust 实现后，`KernelNotifier` 在 `notify_hardware` 入口处调用 `krandom::add_interrupt(irq)`。 |
| do_irqctl.c:160-161 | `if(!isokendpt(hook->proc_nr_e, &proc_nr)) panic("invalid interrupt handler: %d", hook->proc_nr_e)` | ✅ 语义对齐（panic 等价物：`unwrap_or_else(\|\| panic!("invalid interrupt handler: endpoint={:?}", dst))`，irq_manager.rs:149-151） | 无缺口（仅实现形式差异） | C 用 `isokendpt` 验证 endpoint→proc_nr 映射；Rust 用 `proc_table.iter().find(\|p\| p.p_endpoint == dst)` 等价查找，找不到则 panic，diagnostic 与 C 同义。Rust 额外校验 `priv_id` 存在性 + `priv_table.get_mut(priv_id)` 成功（C 隐含 `priv(proc_addr(proc_nr))` 不返回 NULL，未显式检查）。 |

**`isokendpt` 语义说明**：C 的 `isokendpt(endpoint, &proc_nr)` 是 `endpoint` → `proc_nr` 的双向校验宏（同时检查 endpoint 合法性并输出 proc_nr）。Rust 无需此宏，因为：(1) `Endpoint` 是新类型（`pub struct Endpoint(i32)`），类型系统已隔离裸 i32；(2) `proc_table.iter().find(|p| p.p_endpoint == dst)` 完成相同查找；(3) 找不到时 panic 与 C 的 `panic` 同义。Rust 的额外 `priv_id`/`priv_table` 检查是 C 隐含假设的显式化（C 假设 `priv(proc_addr(proc_nr))` 不返回 NULL，Rust 不做此假设）。

### 4.5 classify_signal 信号映射

```rust
pub enum ExceptionSignal { Fpe, Ill, Segv, Bus, Emt, Trap }

fn classify_signal(vector: InterruptVector) -> ExceptionOutcome {
    let sig = match vector.get() {
        0 => Fpe, 1 => Trap, 3 => Emt, 4 => Fpe, 5 => Fpe,
        6 => Ill, 7 => Fpe, 8 => Bus, 9 => Segv, 10 => Segv,
        11 => Segv, 12 => Segv, 13 => Segv, 15 => Ill,
        16 => Fpe, 17 => Bus, 18 => Bus, 19 => Fpe,
        _ => Segv,
    };
    ExceptionOutcome::Signal(sig)
}
```

与 C `ex_data[]`（§2.4）一一对应。match 的穷尽检查保证新增向量不会遗漏。signal 数值（SIGFPE=8 等）仅在最终投递时映射。

### 4.6 页错误转发路径

```
ExceptionDispatcher::handle_page_fault
  → ExceptionOutcome::ForwardToVm(VmPagefaultIn { endpoint, vaddr, write })
  → 调用方（trap 入口）匹配此分支：
      set_pagefault_pending(proc, fault_addr)   // RTS_PAGEFAULT（page_fault.rs）
      build_vm_pagefault_msg(...)                // 构造 VM_PAGEFAULT 消息
      mini_send(VM_PROC_NR, ...)                 // 由 trap 入口执行
  → ExceptionOutcome::VmPageFault 分支 → panic（VM 自身缺页）
```

`page_fault.rs` 把 C 的"设标志 + 发消息"拆成独立可测的 helper：`set_pagefault_pending`、`build_vm_pagefault_msg`、`is_pagefault_pending`、`clear_pagefault_pending`（VM 通过 SYS_VMCTL 调用）。

### 4.7 与 C 步骤数差异说明

| C 步骤（§2） | Rust 实现（§4） | 差异类型 | 说明 |
|-------------|----------------|---------|------|
| §2.1 五分支分流 | §4.2 handle 五分支 | 一致 | — |
| §2.2 地址范围比较 4 恢复点 | §4.3 FaultContext+RecoveryPoint enum | 架构演进 | 类型安全替代符号地址比较 |
| §2.3 pagefault 大函数 | §4.6 拆分 set_pagefault_pending + build_msg + 转发 | 设计决策 | 分流与投递分离，可单测 |
| §2.4 ex_data[] 数组 | §4.5 classify_signal match | 设计决策 | enum 替代裸 u32 |
| §2.5 指针链表 | §4.4 索引链表固定池 | 设计决策 | no_std 无堆 |
| §2.6 hw_intr 宏 | InterruptController trait（minix_plat） | 设计决策 | trait 替代宏 |
| C timer_int_handler 递减 quantum | ❌ Rust 不在本章实现 | 已知缺口（实为 C 也不在 timer_int_handler） | quantum 归 10/11/15 |
| do_irqctl.c:154 `get_randomness(&krandom, hook->irq)` | ❌ `KernelNotifier` 未调用 | 已知缺口 | 推迟到独立 `krandom` 子系统（跨 kernel/PM/VFS，非本章职责）。详见 §4.4 缺口表。 |
| do_irqctl.c:160-161 `isokendpt` + `panic` | ✅ `unwrap_or_else(\|\| panic!)` 语义对齐 | 无缺口（形式差异） | Rust 用 `iter().find` 替代 `isokendpt` 宏；额外显式校验 `priv_id`/`priv_table`（C 隐含假设）。详见 §4.4 缺口表。 |

### 4.8 Return path（TrapReturnArch，待落地）

本文档覆盖异常/中断的**进入**路径（`ExceptionArch` 解析 frame + `ExceptionDispatcher` 分流 + `IrqManager` dispatch）。**返回用户态**路径（`iretq`/`eret`/`sret` + GP 寄存器恢复）由 `TrapReturnArch` trait 抽象，trait 定义与 asm 实现待 doc 10 `switch_to_user` 完整调度循环落地时加入 `os/arch/src/arch/trap_return.rs`。

评估结论与拟议签名见 [03-kmain-cstart.md §4.3 TrapReturnArch 评估结论](03-kmain-cstart.md#trapreturnarch-评估结论)。本文档不重复展开，仅声明 trait 边界归属：`TrapReturnArch: ExceptionArch`（supertrait 复用 `Self::Frame`），新增 `type RegisterFile` 表示 GP 寄存器保存区（C: `p_reg`）。

---

## Ch5: 测试

### 5.1 异常分流测试

| 测试函数 | 验证 | 文件 |
|---------|------|------|
| `spurious_nmi` | 向量 2 → SpuriousNmi | exception_dispatcher.rs |
| `user_divide_error` | 用户态向量 0 → Signal(Fpe) | exception_dispatcher.rs |
| `user_page_fault` | 用户态向量 14 → ForwardToVm | exception_dispatcher.rs |
| `vm_page_fault` | VM 进程缺页 → VmPageFault | exception_dispatcher.rs |
| `kernel_panic` | 内核态异常 → KernelPanic | exception_dispatcher.rs |
| `classify_all_vectors` | 全 19 向量信号映射对齐 C ex_data[] | exception_dispatcher.rs |
| `exception_frame_size`/`vector_extraction`/`is_user_mode_*`/`is_write_fault_*` | x86_64 ExceptionArch impl | x86_64/exception.rs |

### 5.2 嵌套恢复测试

| 测试函数 | 验证 |
|---------|------|
| `nested_user_copy_msg` | copy_msg 中页错误 → RedirectToRecovery(UserCopyMsgFailure) |
| `nested_phys_copy` | phys_copy 中页错误 → RedirectToRecovery(PhysCopyFaultInKernel) |
| `nested_fpu_restore` | fxrstor 中异常 → RedirectToRecovery(FpuRestoreFailure) |
| `nested_debug_clear_tf` | DEBUG+TRACEBIT+KTS_NONE → ClearTrapFlag |
| `nested_debug_not_traced` | DEBUG 非 traced → KernelPanic |
| `nested_debug_traced_not_kts_none` | DEBUG traced 但非 KTS_NONE → KernelPanic |

### 5.3 IRQ 管理测试

| 测试函数 | 验证 |
|---------|------|
| `register_and_dispatch` | 注册后 dispatch 调用 handler + EOI |
| `spurious_irq` | 无 handler 的 IRQ → Spurious 错误 |
| `multiple_hooks_same_irq` | 共享 IRQ 多 handler 各分配唯一 id |
| `not_completed_keeps_active` | handler 返回 NotCompleted → actid 保持 |
| `remove_hook` | 移除后 dispatch → Spurious |
| `enable_disable_irq` | enable/disable 控制 actid 与 mask |

> timer 相关测试不在本章，见 15-clock-timer。

---

## 附录

### A. 三架构异常入口对照

详见 §1.4 三问框架表与 §2.7 差异表。各架构 ExceptionArch impl 见 `os/arch/src/{x86_64,aarch64,riscv64}/exception.rs`。

### B. 与 13/15/16 的边界

| 主题 | 归属 | 说明 |
|------|------|------|
| trap 入口汇编 + KERNEL_CALL=0x600 归一化 | 13-syscall-dispatch | m_type 路由 |
| timer_int_handler 内部 + 量子 + 闹钟 + virt/prof timer | 15-clock-timer | 时钟中断主体 |
| BKL 自旋锁实现 | 16-smp | bkl_lock/unlock |
| GDT/IDT/TSS 初始化 | 03/05 | 保护模式基建 |
| 系统调用入口机制（INT 0x80/SYSENTER/SYSCALL 选择） | 附录 C（本章）+ 03 | CPU 特性检测 |

### C. x86 保护模式基建摘要

> 来源：03-kmain-cstart、05-clock-interrupt-init。本附录仅摘要，细节见对应文档。

**GDT 布局**（`archconst.h:14-21`）：

| 索引 | 选择符 | 用途 | DPL |
|------|--------|------|-----|
| 1 | 0x08 (`KERN_CS_SELECTOR`) | 内核代码段 | 0 |
| 2 | 0x10 (`KERN_DS_SELECTOR`) | 内核数据段 | 0 |
| 3 | 0x1B (`USER_CS_SELECTOR`) | 用户代码段 | 3 |
| 4 | 0x23 (`USER_DS_SELECTOR`) | 用户数据段 | 3 |
| 6+ | `TSS_SELECTOR(cpu)` | 每 CPU 一个 TSS | 0 |

**SYSENTER/SYSEXIT 约束**：Intel SYSENTER 要求 CS 选择符紧接 SS 选择符（偏移 +8），因此内核 CS(1)/DS(2) 必须相邻，用户 CS(3)/DS(4) 也必须相邻。

**特权级**：Minix3 仅用两级（`archconst.h:33-34`）：Ring 0（`INTR_PRIVILEGE`，内核+中断处理）、Ring 3（`USER_PRIVILEGE`，服务进程+用户进程）。

**系统调用入口机制**（`protect.c:323-324` 通过 CPU 特性检测选择）：

| 机制 | CPU 特性标志 | 入口 |
|------|-------------|------|
| `int` 指令（原始） | 始终可用 | IDT 向量 32 |
| `int` 指令（user-mapped） | 始终可用 | IDT 向量 34 |
| SYSENTER（Intel） | `MKF_I386_INTEL_SYSENTER` | MSR |
| SYSCALL（AMD） | `MKF_I386_AMD_SYSCALL` | MSR |

64 位模式下 SYSENTER 被移除，仅 KTS_NONE/KTS_SYSCALL/KTS_INT_HARD 相关（见 `KernTrapStyle` enum）。m_type 路由与 KERNEL_CALL=0x600 归一化见 13。

**prot_init()**（在 `cstart()` 中调用）：清零 GDT/IDT → 设置描述符指针 → 初始化 TSS → 填充 GDT 条目 → 加载选择符 → 重建页表。段描述符基地址均为 0、界限 4GB（平坦内存模型）。IDT 门描述符特权：硬件中断和 CPU 异常用 `INTR_PRIVILEGE`(0)，breakpoint/overflow/IPC/系统调用向量用 `USER_PRIVILEGE`(3)。

---

## 参考文献

1. `minix3/minix/kernel/arch/i386/exception.c:19-283` — 异常表、pagefault、exception_handler
2. `minix3/minix/kernel/interrupt.c:29-176` — put_irq_handler、rm_irq_handler、irq_handle、enable/disable_irq
3. `minix3/minix/kernel/arch/i386/hw_intr.h` — 中断控制器抽象宏
4. `minix3/minix/kernel/clock.c:70-173` — timer_int_handler（仅时间记账，quantum 递减不在此）
5. `os/arch/src/arch/exception.rs` — ExceptionArch trait、FaultContext、RecoveryPoint
6. `os/arch/src/arch/exception_dispatcher.rs` — ExceptionDispatcher、ExceptionOutcome、ExceptionSignal
7. `os/kernel/src/irq_manager.rs` — IrqManager<IC>
8. `os/kernel/src/page_fault.rs` — 页错误转发 helper
