# 14-exception-interrupt: 异常与中断处理

> **源码**: `minix3/minix/kernel/arch/i386/exception.c`, `minix3/minix/kernel/interrupt.c`, `minix3/minix/kernel/clock.c:70-199`
> **前置**: 10（进程状态）, 11（IPC——中断通过 mini_notify 唤醒进程）

---

## Ch1: 概念

### 1.1 三种激活路径

内核没有主循环，只有三种被激活的方式：

| 路径 | 入口 | 触发 | 处理 | 出口 |
|------|------|------|------|------|
| 硬件中断 | IDT[IRQ+32] → irq_handle() | 外部设备异步 | 遍历 hook 链 → mini_notify → switch_to_user | 调度决策 |
| CPU 异常 | IDT[vec] → exception_handler() | 当前指令导致 | 用户态→cause_sig / 页错误→转发VM / 内核态→panic | 调度决策 |
| 系统调用 | INT 0x80 → sys_call | 用户进程主动 | kernel_call / do_ipc | 调度决策 |

**三条路径的出口都是同一个**——`switch_to_user()`。这验证了 09 文档的结论：switch_to_user 是内核运行时的核心枢纽。

### 1.2 页错误处理（最复杂的异常）

四层分解：

| 层次 | 问题 | C 机制 |
|------|------|--------|
| 状态定义 | 缺页进程的状态？ | RTS_PAGEFAULT + p_vmrequest |
| 状态转换 | 缺页如何触发/恢复？ | exception → mini_send(VM) → VMCTL_CLEAR_PAGEFAULT |
| 事件源 | 什么触发缺页？ | 用户态访问未映射页 / 内核态 cross_space_copy |
| 服务 | VM 如何处理？ | 分配物理页 → 映射 → 通知内核恢复进程 |

### 1.3 时钟中断（调度的驱动力）

```
时钟中断 → timer_int_handler()
  → 递减 p_cpu_time_left
  → 归零？
    → RTS_SET(NO_QUANTUM)
    → notify_scheduler(p)        ← 通知 PM 重新分配量子
    → switch_to_user → pick_proc ← 可能切换进程
  → 未归零？
    → 返回当前进程
```

### 1.4 中断到调度的完整路径

```
硬件中断 → IDT → BKL_LOCK → irq_handle → mini_notify(target)
  → 唤醒等待进程 (RTS_UNSET(RECEIVING))
  → switch_to_user → pick_proc → restore_user_context
```

---

## Ch2: C 源码分析

### 2.0 Claims-Evidence

| Claim | Evidence | Status |
|-------|----------|--------|
| x86 异常表有 20 项 | `exception.c:19-39` ex_data[] | ✅ verified |
| 页错误设置 RTS_PAGEFAULT | `exception.c:112` | ✅ verified |
| 页错误通过 mini_send 通知 VM | `exception.c:115-120` | ✅ verified |
| VM 页错误不可处理 → panic | `exception.c:100-110` | ✅ verified |
| 用户态异常 → cause_sig | `exception.c:258-260` | ✅ verified |
| 内核态异常 → panic | `exception.c:264` | ✅ verified |
| IRQ hook 链表管理 | `interrupt.c:29-73` | ✅ verified |
| irq_handle 遍历 hook 链 | `interrupt.c:116-160` | ✅ verified |
| timer_int_handler 递减量子 | `clock.c:70-199` | ✅ verified |

### 2.1 exception_handler

```c
// exception.c:180-286
void exception_handler(int is_nested, struct exception_frame * frame)
{
  struct ex_s *ep;
  struct proc *saved_proc;

  saved_proc = get_cpulocal_var(proc_ptr);
  ep = &ex_data[frame->vector];

  if (frame->vector == 2) return;  // spurious NMI

  // 嵌套异常特殊处理
  if (is_nested) {
    // copy_msg_to/from_user 中的页错误 → 跳转到恢复点
    // fxrstor 中的 FPU 异常 → 跳转到恢复点
    // DEBUG_VECTOR + TRACEBIT → 清除标志位
  }

  // 页错误单独处理
  if (frame->vector == PAGE_FAULT_VECTOR) {
    pagefault(saved_proc, frame, is_nested);
    return;
  }

  // 用户态异常 → 发送信号
  if (is_nested == 0 && !iskernelp(saved_proc)) {
    cause_sig(proc_nr(saved_proc), ep->signum);
    return;
  }

  // 内核态异常 → panic
  inkernel_disaster(saved_proc, frame, ep, is_nested);
}
```

### 2.2 pagefault

```c
// exception.c:49-131
static void pagefault(struct proc *pr, struct exception_frame *frame, int is_nested)
{
  reg_t pagefaultcr2 = read_cr2();

  // 内核态 physcopy/memset 中的页错误 → 特殊恢复
  if ((is_nested || iskernelp(pr)) && catch_pagefaults &&
      (in_physcopy || in_memset)) {
    // 设置恢复地址
    return;
  }

  // 嵌套页错误（非 physcopy）→ panic
  if (is_nested) {
    inkernel_disaster(pr, frame, NULL, is_nested);
  }

  // VM 进程页错误 → panic
  if (pr->p_endpoint == VM_PROC_NR) {
    panic("pagefault in VM");
  }

  // 设置 RTS_PAGEFAULT
  RTS_SET(pr, RTS_PAGEFAULT);

  // 通过 mini_send 通知 VM
  m_pagefault.m_source = pr->p_endpoint;
  m_pagefault.m_type = VM_PAGEFAULT;
  m_pagefault.VPF_ADDR = pagefaultcr2;
  m_pagefault.VPF_FLAGS = frame->errcode;
  mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL);
}
```

### 2.3 x86 异常表

| 向量号 | 异常名 | 信号 | 最低处理器 |
|--------|--------|------|-----------|
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

C 源码: `exception.c:19-39`

### 2.4 IRQ hook 管理

```c
// interrupt.c:29-73 — put_irq_handler()
void put_irq_handler(irq_hook_t* hook, int irq, const irq_handler_t handler)
{
  // 遍历链表找到尾部，收集已用 ID 位图
  // 分配最低未用 ID
  // 追加到链表尾部
  // 如果是第一个 handler → unmask IRQ
}

// interrupt.c:75-106 — rm_irq_handler()
void rm_irq_handler(const irq_hook_t* hook)
{
  // 从链表中移除
  // 清除 actid 位
  // 如果链表空 → mask IRQ
  // 如果链表非空且无活跃 handler → unmask IRQ
}

// interrupt.c:116-160 — irq_handle()
void irq_handle(int irq)
{
  hw_intr_mask(irq);        // 先屏蔽
  hook = irq_handlers[irq];
  if (hook == NULL) return; // spurious

  while (hook != NULL) {
    irq_actids[irq] |= hook->id;
    if ((*hook->handler)(hook))
      irq_actids[hook->irq] &= ~hook->id;
    hook = hook->next;
  }

  if (irq_actids[irq] == 0)
    hw_intr_unmask(irq);    // 全部完成 → 解除屏蔽

  hw_intr_ack(irq);         // 发送 EOI
}
```

### 2.5 timer_int_handler

```c
// clock.c:70-199
int timer_int_handler(void)
{
  // 递增 uptime / realtime
  p->p_user_time++;
  if (!(priv(p)->s_flags & BILLABLE))
    billp->p_sys_time++;

  // 递减虚拟定时器
  if ((p->p_misc_flags & MF_VIRT_TIMER) && (p->p_virt_left > 0))
    p->p_virt_left--;
  if ((p->p_misc_flags & MF_PROF_TIMER) && (p->p_prof_left > 0))
    p->p_prof_left--;

  // 检查定时器到期
  vtimer_check(p);
  if (p != billp) vtimer_check(billp);

  // 更新负载平均
  load_update();

  // 检查同步闹钟定时器
  if (clock_timers != NULL && tmr_has_expired(...))
    tmrs_exptimers(...);

  arch_timer_int_handler();
  return(1);  // 重新启用中断
}
```

### 2.6 架构差异

| 维度 | x86 | ARM | RISC-V |
|------|-----|-----|--------|
| 异常入口 | IDT | Exception Vector Table | stvec |
| 页错误向量 | 14 | Prefetch/Data Abort | scause=12/13/15 |
| 系统调用入口 | INT 0x80 / SYSENTER | SVC | ecall |
| 时钟中断 | IRQ 0 (PIT/HPET) | Timer IRQ | stimer interrupt |
| CR2 寄存器 | 页错误地址 | FAR | stval |

---

## Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| D1: 异常处理 | 全局函数 vs trait | `trait ExceptionHandler` | 架构抽象 |
| D2: 异常表 | 硬编码 vs 数据驱动 | `const EXCEPTION_TABLE` | 编译期生成，与 C 一致 |
| D3: IRQ hook 链 | 指针链表 vs 索引链表 | 索引链表（`Option<usize>`） | 无指针，固定池分配 |
| D4: IrqAction | int 返回值 vs enum | `enum IrqAction { Completed, Pending }` | 类型安全 |
| D5: IrqManager | 全局 vs 泛型 | `IrqManager<IC: InterruptController>` | 注入架构依赖 |
| D6: 页错误 | 全局函数 vs 结构化 | `PageFaultInfo` + `trait PageFaultHandler` | 分离信息与处理 |
| D7: 时钟中断 | 全局 vs trait | `trait TimerHandler` | 架构抽象 |

### D3: 索引链表替代指针链表

C 使用 `irq_hook_t*` 指针链表。Rust 使用固定大小的 `hooks` 池 + `Option<usize>` 索引：

- 无动态分配（`no_std`）
- 无裸指针
- 编译期确定最大 hook 数量

### D5: IrqManager 泛型

IRQ 管理逻辑（注册/移除/遍历）在所有架构上相同。唯一差异是硬件操作（mask/unmask/eoi），通过 `InterruptController` trait 注入。

### D6: 页错误结构化

C 的 `pagefault()` 是一个大函数，混合了多种情况。Rust 将页错误信息提取为 `PageFaultInfo`，处理逻辑通过 `trait PageFaultHandler` 抽象：

```rust
pub struct PageFaultInfo {
    pub fault_addr: u64,    // CR2 / FAR / stval
    pub error_code: u32,    // x86 errcode
    pub is_write: bool,
    pub is_user: bool,
}

pub trait PageFaultHandler {
    fn handle_user_pagefault(&mut self, proc: &mut KProcess, info: &PageFaultInfo);
    fn handle_kernel_pagefault(&mut self, info: &PageFaultInfo) -> bool;
}
```

---

## Ch4: 实现

### 4.1 IrqManager

`os/kernel/src/irq_manager.rs` 提供完整的 IRQ hook 管理：

- `register_hook()`: 注册 handler，分配 ID，追加到链表
- `remove_hook()`: 移除 handler，清理链表
- `dispatch()`: 遍历 handler 链，跟踪 actid
- `enable_irq()` / `disable_irq()`: 控制 IRQ 屏蔽

### 4.2 ExceptionInfo

```rust
/// CPU 异常信息。
pub struct ExceptionInfo {
    /// 异常向量号。
    pub vector: u32,
    /// 错误码（如有）。
    pub error_code: u32,
    /// 故障指令地址。
    pub fault_addr: u64,
    /// 是否嵌套异常（内核态触发）。
    pub is_nested: bool,
}

/// 异常到信号的映射。
pub struct ExceptionMapping {
    pub description: &'static str,
    pub signal: u32,
}
```

### 4.3 PageFaultInfo

```rust
/// 页错误信息。
pub struct PageFaultInfo {
    /// 故障虚拟地址（CR2 / FAR / stval）。
    pub fault_addr: u64,
    /// 错误码。
    pub error_code: u32,
    /// 是否写操作。
    pub is_write: bool,
    /// 是否用户态触发。
    pub is_user: bool,
}
```

### 4.4 exception_handler 流程

```rust
pub fn handle_exception<EH: ExceptionHandler>(
    handler: &mut EH,
    proc: &mut KProcess,
    info: &ExceptionInfo,
) -> ExceptionAction {
    // 1. 特殊向量处理（NMI 等）
    if info.vector == 2 {
        return ExceptionAction::Ignore;
    }

    // 2. 嵌套异常特殊处理
    if info.is_nested {
        // copy_msg 中的页错误 → 跳转恢复点
        // FPU 恢复中的异常 → 跳转恢复点
        // DEBUG + TRACEBIT → 清除标志
    }

    // 3. 页错误单独处理
    if info.vector == PAGE_FAULT_VECTOR {
        return handler.handle_pagefault(proc, &PageFaultInfo::from(info));
    }

    // 4. 用户态异常 → 发送信号
    if !info.is_nested && !proc.is_kernel() {
        let mapping = &EXCEPTION_TABLE[info.vector as usize];
        return ExceptionAction::Signal(mapping.signal);
    }

    // 5. 内核态异常 → panic
    ExceptionAction::Panic
}
```

### 4.5 pagefault 流程

```rust
pub fn handle_user_pagefault(
    procs: &mut [KProcess],
    vm_endpoint: Endpoint,
    proc_nr: ProcNr,
    info: &PageFaultInfo,
) -> IpcResult<()> {
    let proc = &mut procs[nr_to_idx(proc_nr).unwrap()];

    // VM 进程页错误 → panic
    if proc.p_endpoint == vm_endpoint {
        panic!("pagefault in VM");
    }

    // 设置 RTS_PAGEFAULT
    proc.p_rts_flags.set(RtsFlagsBits::PAGEFAULT);

    // 构造页错误消息
    let msg = Message {
        m_source: proc.p_endpoint,
        m_type: VM_PAGEFAULT,
        ..Default::default()
    };

    // 通过 mini_send 通知 VM
    IpcEngine::send(procs, proc_nr, vm_endpoint, &msg, SendFlags::FROM_KERNEL)
}
```

### 4.6 timer_int_handler 流程

```rust
pub fn timer_tick<IC: InterruptController>(
    irq_mgr: &mut IrqManager<IC>,
    procs: &mut [KProcess],
    current: ProcNr,
) -> TimerAction {
    let proc = &mut procs[nr_to_idx(current).unwrap()];

    // 递增用户时间
    proc.p_user_time += 1;

    // 递减虚拟定时器
    // ...

    // 检查量子是否用完
    if proc.p_cpu_time_left > 0 {
        proc.p_cpu_time_left -= 1;
    }
    if proc.p_cpu_time_left == 0 {
        proc.p_rts_flags.set(RtsFlagsBits::NO_QUANTUM);
        return TimerAction::Reschedule;
    }

    TimerAction::Continue
}
```

---

## Ch5: 测试

### 5.1 异常处理

| 测试 | 验证 |
|------|------|
| NMI 向量 → Ignore | 向量 2 被忽略 |
| 用户态 GP → SIGSEGV | 向量 13 映射到 SIGSEGV |
| 用户态页错误 → 通知 VM | RTS_PAGEFAULT 设置 + mini_send |
| VM 进程页错误 → panic | 不允许 VM 缺页 |
| 内核态异常 → Panic | 非嵌套内核异常触发 panic |

### 5.2 IRQ 管理

| 测试 | 验证 |
|------|------|
| register_hook → ID 分配 | 最低未用位分配 |
| register_hook → 链表追加 | 多 handler 正确排序 |
| remove_hook → 链表更新 | 正确移除中间/头部/尾部 |
| dispatch → 遍历所有 handler | 每个 handler 被调用 |
| spurious IRQ → 错误 | 无 handler 的 IRQ 返回 Spurious |
| actid 跟踪 | Pending handler 保持 actid |

### 5.3 时钟中断

| 测试 | 验证 |
|------|------|
| 量子递减 | 每次递减 1 |
| 量子用完 → Reschedule | NO_QUANTUM 标志设置 |
| 量子未用完 → Continue | 继续当前进程 |
| 用户/系统时间统计 | 正确计费 |

---

## 6. 补充：x86 保护模式基础设施

> 来源：tmp-04-protection.md

### 6.1 GDT 布局

Minix3 的 GDT 采用固定索引布局（archconst.h:14-21）：

| 索引 | 选择符 | 用途 | DPL |
|------|--------|------|-----|
| 0 | 0x00 | 空描述符（硬件要求） | — |
| 1 | 0x08 (`KERN_CS_SELECTOR`) | 内核代码段 | 0 |
| 2 | 0x10 (`KERN_DS_SELECTOR`) | 内核数据段 | 0 |
| 3 | 0x1B (`USER_CS_SELECTOR`) | 用户代码段 | 3 |
| 4 | 0x23 (`USER_DS_SELECTOR`) | 用户数据段 | 3 |
| 5 | 0x28 (`LDT_SELECTOR`) | 不可用的 LDT | 0 |
| 6+ | `TSS_SELECTOR(cpu)` | 每 CPU 一个 TSS | 0 |

**SYSENTER/SYSEXIT 约束**：Intel SYSENTER 指令要求 CS 选择符紧接 SS 选择符（偏移 +8），因此内核代码段（索引1）和数据段（索引2）必须相邻，用户代码段（索引3）和数据段（索引4）也必须相邻。

### 6.2 特权级

Minix3 仅使用两级特权（archconst.h:33-34）：

| 级别 | 常量 | 值 | 使用者 |
|------|------|----|--------|
| Ring 0 | `INTR_PRIVILEGE` | 0 | 内核 + 中断处理程序 |
| Ring 3 | `USER_PRIVILEGE` | 3 | 服务进程 + 用户进程 |

### 6.3 系统调用入口机制

Minix3 支持三种用户态→内核态的系统调用入口，通过 CPU 特性检测选择（protect.c:323-324）：

| 机制 | CPU 特性标志 | IDT 向量 | 入口函数 |
|------|-------------|---------|---------|
| `int` 指令（原始） | 始终可用 | `KERN_CALL_VECTOR_ORIG`(32) | `kernel_call_entry_orig` |
| `int` 指令（user-mapped） | 始终可用 | `KERN_CALL_VECTOR_UM`(34) | `kernel_call_entry_um` |
| SYSENTER（Intel） | `MKF_I386_INTEL_SYSENTER` | MSR | `ipc_entry_sysenter` |
| SYSCALL（AMD） | `MKF_I386_AMD_SYSCALL` | MSR | `ipc_entry_syscall_cpuN` |

### 6.4 GDT/IDT/TSS 尺寸与索引常量

| 常量 | 值 | 含义 |
|------|----|------|
| `IDT_SIZE` | 256 | IDT 最大条目数 |
| `KERN_CS_INDEX` | 1 | 内核代码段在 GDT 中的索引 |
| `KERN_DS_INDEX` | 2 | 内核数据段在 GDT 中的索引 |
| `USER_CS_INDEX` | 3 | 用户代码段在 GDT 中的索引 |
| `USER_DS_INDEX` | 4 | 用户数据段在 GDT 中的索引 |
| `TSS_INDEX(cpu)` | `6 + cpu` | 每 CPU 的 TSS 索引 |
| `GDT_SIZE` | `6 + CONFIG_MAX_CPUS` | GDT 总条目数 |

### 6.5 prot_init() 初始化流程

`prot_init()` 在 `cstart()` 中被调用，完成：
1. 清零 GDT/IDT → 设置描述符指针 → 初始化 TSS → 填充 GDT 条目 → 加载选择符 → 重建页表
2. 段描述符基地址/界限：内核和用户段的基地址均为 0，界限均为 4GB（平坦内存模型）
3. IDT 门描述符特权：硬件中断和 CPU 异常使用 `INTR_PRIVILEGE`(0)，breakpoint/overflow/IPC/系统调用向量使用 `USER_PRIVILEGE`(3)
4. TSS 初始化：每 CPU 一个 TSS，`ss0:sp0` 指向该 CPU 的内核栈顶

### 6.6 VM 二进制加载的特殊性

`arch_boot_proc()` 中，VM 进程的加载路径与其他 boot 进程完全不同：
- **VM**：在自举页表中通过 `libexec_load_elf()` 解析 ELF 并映射
- **其他进程**：仅设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT` 等待 VM 为其创建页表

原因：VM 是第一个运行的用户态进程，它负责为其他所有进程管理内存。在 VM 运行之前，没有服务进程能处理内存分配请求。

---

## 7. 补充：异常与中断详细分析

> 来源：tmp-05-exception-interrupt.md

### 7.1 异常帧（exception_frame）

当异常发生时，CPU 自动将关键寄存器压入当前栈（arch_proto.h:72-80）：

| 字段 | 含义 | 压入者 |
|------|------|--------|
| `vector` | 异常向量号 | 汇编入口（mpx.S） |
| `errcode` | 错误码（无错误码的异常由汇编压入 0） | CPU 或汇编入口 |
| `eip` | 异常发生时的指令指针 | CPU |
| `cs` | 异常发生时的代码段 | CPU |
| `eflags` | 异常发生时的标志寄存器 | CPU |
| `esp` | 异常发生时的栈指针（嵌套异常时无效） | CPU |
| `ss` | 异常发生时的栈段（嵌套异常时无效） | CPU |

**嵌套异常**：当异常在内核态发生时（`is_nested=1`），CPU 不会压入 `esp`/`ss`（因为不发生栈切换），此时这两个字段的值是栈上的残留数据，不可使用。

### 7.2 异常分类表（ex_data[]）

Minix3 将 20 个 x86 异常向量映射到 POSIX 信号（exception.c:19-39）：

| 向量 | 异常名称 | 信号 | 最低处理器 |
|------|---------|------|-----------|
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
| 16 | Coprocessor error | SIGFPE | 386 |
| 17 | Alignment check | SIGBUS | 386 |
| 18 | Machine check | SIGBUS | 386 |
| 19 | SIMD exception | SIGFPE | 386 |

**页错误（向量 14）是特殊异常**：它不走通用的"发信号"路径，而是有独立的 `pagefault()` 函数处理。

### 7.3 IRQ 钩子链（irq_hook_t）

Minix3 允许同一 IRQ 线被多个驱动共享。每个驱动注册一个 `irq_hook_t` 钩子：

| 字段 | 类型 | 含义 |
|------|------|------|
| `next` | `struct irq_hook *` | 链表下一项 |
| `handler` | `int (*)(struct irq_hook *)` | 中断处理回调 |
| `irq` | `int` | IRQ 向量号 |
| `id` | `int` | 唯一标识（位掩码，用于 `irq_actids`） |
| `proc_nr_e` | `endpoint_t` | 注册进程的端点 |
| `notify_id` | `irq_id_t` | 通知标识 |
| `policy` | `irq_policy_t` | 策略位掩码（如 `IRQ_REENABLE`） |

**id 分配策略**：`put_irq_handler()` 为每个钩子分配最小的未使用位（1, 2, 4, 8, ...），用于在 `irq_actids[]` 中标记该钩子是否活跃。

### 7.4 中断控制器抽象（hw_intr 宏）

Minix3 通过宏抽象中断控制器操作（hw_intr.h），支持 8259A PIC 和 IOAPIC 两种硬件：

| 宏 | 8259A PIC 实现 | IOAPIC 实现 |
|----|---------------|-------------|
| `hw_intr_mask(irq)` | `irq_8259_mask(irq)` | `ioapic_mask_irq(irq)` |
| `hw_intr_unmask(irq)` | `irq_8259_unmask(irq)` | `ioapic_unmask_irq(irq)` |
| `hw_intr_ack(irq)` | `irq_8259_eoi(irq)` | `ioapic_eoi(irq)` |

8259A 的 `hw_intr_used`/`hw_intr_not_used` 为空：因为 8259A 是固定 16 个 IRQ，无需动态配置路由。IOAPIC 需要显式设置 IRQ 路由到哪个 CPU。

### 7.5 页错误转发机制详细流程

1. CPU 将错误地址写入 CR2 寄存器
2. 内核读取 CR2，构造 `VM_PAGEFAULT` 消息
3. 通过 `mini_send()` 将消息发送给 VM 进程
4. 设置 `RTS_PAGEFAULT` 阻塞当前进程
5. VM 处理缺页后通过 `SYS_VMCTL` 清除 `RTS_PAGEFAULT`，恢复进程

**VM 自身不能缺页**：如果 VM 进程触发页错误，内核直接 panic。这是设计上的必然——VM 负责为所有进程管理内存，如果 VM 自身需要缺页处理，就形成了循环依赖。

---

## 参考文献

1. `minix3/minix/kernel/arch/i386/exception.c:19-286` — 异常表、pagefault、exception_handler
2. `minix3/minix/kernel/interrupt.c:29-177` — put_irq_handler、rm_irq_handler、irq_handle
3. `minix3/minix/kernel/clock.c:70-199` — timer_int_handler
4. `os/kernel/src/irq_manager.rs` — Rust IRQ 管理实现
5. `minix3/minix/kernel/interrupt.h` — irq_hook_t 定义
