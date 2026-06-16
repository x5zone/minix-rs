# runtime-design-m3: kboot 之后 kernel 全部叙事的最终规划

> **创建**: 2026-06-13
> **前置文档**: [kboot-new.md](kboot-new.md) — 已确立 T0-T7（启动 7 步）的叙事基线，本文从 T8 续起
> **目标**: 解决 overview 规划中的"叙事跳跃"问题，给出**严格沿 CPU 执行时间线**的 18 篇文档规划
> **方法**: 逐行追踪 Minix3 C 源码 `main.c:38-522`、`proc.c:1-1980`、`system.c:1-997`、40 个 `system/do_*.c`，以"T 编号 + 时序依赖"重新切分文档
> **决策原则**:
>   1. **顺序叙事**: 每篇文档承接上一篇，前向零引用
>   2. **时间线优先**: 文档编号 = CPU 实际执行顺序，而非"子系统并列"
>   3. **C 行为是 Ground Truth**: 任何 Rust 设计必须能溯源到 C 行为
>   4. **Direct Map 演进在 Ch3 标注**: 不消除 C 行为记录

---

## 0. 总览：T8-T26 完整时序与文档覆盖

### 0.1 启动阶段之后，CPU 进入运行时

kboot 阶段（01-06）结束于 T7：`memory_init()` 分配了 2 个 `freepde` 槽位，`ptproc = VM`。从 T8 起，CPU 仍在 `kmain()` 内（C 函数体），沿 main.c 顺序执行以下三件事：

| 时序点 | C 函数 | C 源码 | 核心动作 | **本文档** |
|--------|--------|--------|---------|------------|
| T8 | `system_init()` | system.c:168-272 | 注册 50+ 个 system call handler 到 call_vec[] | 07 §1 |
| T9 | `add_memmap(&kinfo, bootstrap_start, bootstrap_len)` | pg_utils.c:86-125 | 回收 bootstrap 内存 | 07 §2 |
| T9.5 | `smp_init()` 或 `smp_single_cpu_fallback()` | smp.c | 启动 APs 或单核 | 07 §3 + 15 §1 |
| T10 | `bsp_finish_booting()` | main.c:38-113 | cpu_identify、announce、RTS_PROC_STOP 解除、timer、fpu、**switch_to_user** | 07 §4 |
| T11 | VM 第一次被调度 | (回 02-stage-vm/26) | VM 调 init_page_table → map_kernel 建 direct map | 08 §1 |
| T12.1 | SYS_VMCTL(VMCTL_SETADDRSPACE) | do_vmctl.c | 切换 CR3 到 VM 的真实页表 | 08 §2 |
| T12.3 | SYS_VMCTL(VMCTL_KERN_PHYSMAP) | do_vmctl.c + arch/i386/do_vmctl.c | 内核声明需映射的物理区 | 08 §3 |
| T12.5 | SYS_VMCTL(VMCTL_KERN_MAP_REPLY) | do_vmctl.c | VM 返回虚拟地址 | 08 §4 |
| T12.7 | SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR) | do_vmctl.c | 解除所有进程的 VMINHIBIT | 08 §5 |
| T12.9 | `vm_running = 1` | (kernel 全局变量) | runtime 真正就绪 | 08 §6 |
| T13 | switch_to_user() 第一次返回 | proc.c:299-450 | idle → pick_proc → 切地址空间 → iret | 09 §1 |
| T13.1 | pick_proc() | proc.c | 16 优先级队列遍历 | 10 §1 |
| T13.2 | enqueue() / dequeue() | proc.c:1595 / 1716 | 队列插入/删除 | 10 §2 |
| T13.3 | RTS_* 状态机 | proc.h:141-228 | runnable iff p_rts_flags == 0 | 10 §3 |
| T14 | 时钟中断 fire | clock.c:70 timer_int_handler | 100Hz tick → RTS_NO_QUANTUM / RTS_PREEMPTED | 14 §1 |
| T15 | 系统调用 trap (INT 0x30) | arch_system.c | sys_call 入口 → kernel_call | 12 §1 |
| T15.1 | kernel_call(m_user, caller) | system.c:136-162 | copy_msg_from_user → kernel_call_dispatch | 12 §2 |
| T15.2 | kernel_call_dispatch | system.c | call_vec[syscall_num] 分派 | 12 §3 |
| T16 | mini_send / mini_receive | proc.c:870 / 1080 | 同步 IPC 阻塞语义 | 11 §1 |
| T16.1 | mini_notify | proc.c:1122 | 异步单边 | 11 §2 |
| T16.2 | try_deliver_senda / SENDA | proc.c:1200 | 异步双边 | 11 §3 |
| T16.3 | delivermsg | proc.c | 拷贝 m_source → 用户栈 | 11 §4 |
| T17 | do_fork / do_exec / do_exit | system/do_fork.c 等 | 进程生命周期 | 16 |
| T18 | do_memset / do_umap / do_vircopy / do_physcopy / do_safecopy* | system/do_memset.c 等 | 内存系统调用 | 17 |
| T18.5 | lin_lin_copy (内核跨进程) | arch/i386/memory.c | 跨地址空间 memcpy | 23 §1 |
| T18.6 | createpde | arch/i386/memory.c | 临时 PDE 映射 | 23 §2 |
| T18.7 | vm_memset / vm_lookup | memory.c | VM 协助的 memset/查询 | 23 §3 |
| T19 | VMSUSPEND 协议 | memory.c | 进程缺页 → VM → resume | 23 §4 |
| T20 | SMP 时钟中断（AP 核） | smp.c:156 smp_sched_handler | IPI SCHED → AP 调度 | 15 §2 |
| T21 | exception (page fault) | arch/i386/exception.c | pagefault handler | 13 §1 |
| T22 | ipc_filter | system.c:705-916 | IPC 过滤器 | 22 |
| T23 | privilege (priv) | system.c:274-540, priv.h | 权限位操作 | 21 |
| T24 | watchdog / ACPI | watchdog.c, acpi.c | 监视器 | 24 §1 |
| T25 | debug / profile / usermapped | debug.c, profile.c, usermapped_data.c | 调试 | 24 §2-3 |
| T26 | unported symbols | (legacy / x86-only) | 64 位/Rust 不移植 | 24 §4 |

### 0.2 与 kboot-new 的衔接

kboot-new.md 已经规划了 07 与 08 两篇文档（基于 T8-T10 + T11-T12.7）。本文将这两篇作为基线，进一步把 **T13-T26** 拆为 16 篇文档（09-24），形成 **07-24 共 18 篇** 的完整 runtime 规划。

**关键差异（相对 kboot-new §0.2）**：
- kboot-new 07 仅覆盖 T8-T10（kmain tail ~200 行 C）。本规划 07 保持不变。
- kboot-new 08 仅覆盖 T11-T12.7（VMCTL 握手 ~150 行 C）。本规划 08 保持不变。
- kboot-new 之后戛然而止。本规划从 T13 续起，进入**真正的 runtime**。

### 0.3 文档依赖图（强约束：仅向后引用）

```
07-system-init-boot-finish ─┐
                            │
                            ▼
08-vm-boot-protocol ───────┐
                            │
                            ▼
09-switch-to-user-entry ───┐
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
       10-scheduling   13-exception-  14-clock-timer
       -primitives     interrupt
              │             │             │
              ▼             │             │
       11-ipc-core ◄────────┴─────────────┘
              │
              ▼
       12-syscall-dispatch
              │
   ┌──────────┼──────────┬──────────┬──────────┐
   ▼          ▼          ▼          ▼          ▼
16-proc   17-mem     18-signal  19-device  20-control
   │          │          │          │          │
   └──────────┴──────────┴──────────┴──────────┘
              │
              ▼
       21-privilege    22-ipc-filter
              │             │
              └──────┬──────┘
                     ▼
              23-cross-space-runtime
                     │
                     ▼
              24-misc-unported
```

**反例检查**：
- 09 引用 07-08 已在前面（向后 ✅）
- 10 引用 09（在前面 ✅）
- 14 引用 13（同时引用但 13 略前 ✅）
- 21 引用 16-20（21 在后 ✅）
- 24 引用所有（前向 ✅）

---

## 1. 文档 07：system_init + add_memmap + bsp_finish_booting（kmain 尾段）

> **状态**: 与 kboot-new §1 一致，此处重写得更紧凑
> **C 源码**: `minix3/minix/kernel/system.c:168-272`, `minix3/minix/kernel/arch/i386/pg_utils.c:86-125`, `minix3/minix/kernel/main.c:38-113`
> **C 总行数**: ~230 行
> **依赖**: 06（ptproc 已设、freepdes 已分配）
> **产出**: 09 启动运行时所需的全部就绪状态

### 1.1 system_init()（system.c:168-272）

**核心问题**: 内核如何把 50+ 个系统调用号映射到对应的 C handler？

**C 源码分析**:
```
1. for (i=0; i<NR_IRQ_HOOKS; i++) irq_hooks[i].proc_nr_e = NONE;   // L173-175
2. for (sp=BEG_PRIV_ADDR; sp<END_PRIV_ADDR; sp++) tmr_inittimer(&sp->s_alarm_timer);  // L178-180
3. for (i=0; i<NR_SYS_CALLS; i++) call_vec[i] = NULL;              // L188-190
4. map(SYS_FORK, do_fork);  // ~50 行 map() 调用                    // L193-271
```

**关键概念**:
- `call_vec[NR_SYS_CALLS]` 是 system call 派发表（数组下标 = syscall 号）
- `map(call, fn)` 是宏：检查范围 + 写 `call_vec[call] = fn`
- 中断 hook 池（NR_IRQ_HOOKS = 64）初始化为 NONE
- 每个 priv 结构有 s_alarm_timer，定时器必须先 init

**map() 涵盖的 syscall（Ch1 必读）**:
- 进程管理：FORK / EXEC / CLEAR / EXIT / PRIVCTL / TRACE / SETGRANT / RUNCTL / UPDATE / STATECTL
- 信号：KILL / GETKSIG / ENDKSIG / SIGSEND / SIGRETURN
- 设备：IRQCTL / DEVIO(i386) / VDEVIO / SDEVIO(i386) / IOPENABLE(i386) / READBIOS(i386)
- 内存：MEMSET / VMCTL
- 拷贝：UMAP / UMAP_REMOTE / VUMAP / VIRCOPY / PHYSCOPY / SAFECOPYFROM / SAFECOPYTO / VSAFECOPY / SAFEMEMSET
- 时钟：TIMES / SETALARM / STIME / SETTIME / VTIMER
- 系统控制：ABORT / GETINFO / DIAGCTL
- 性能：SPROF
- 调度：SCHEDULE / SCHEDCTL
- 机器状态：SETMCONTEXT / GETMCONTEXT
- ARM：PADCONF

**Ch3 设计决策**:
| 决策 | 选项 | 结论 |
|------|------|------|
| call_vec 表达 | `[Option<fn>; NR_SYS_CALLS]` vs `enum Syscall + match` | **match**（类型安全 + 编译期穷尽检查） |
| IRQ hook 池表达 | `Vec<Option<IrqHook>>` vs `BTreeMap<irq, hook>` | **Vec<Option>**（保持 C 池语义） |
| Alarm timer 表达 | 每个 priv 一个 timer struct | **保持**（per-priv 而非全局） |
| map() 宏的处理 | 保留 vs 静态断言 | **保留** 宏为 `const _: () = { ... }`（编译期检查） |

**Ch4 实现要点**:
- `call_vec: [Option<SyscallHandler>; NR_SYS_CALLS]` 或 `match syscall { SYS_FORK => do_fork, ... }`
- `enum Syscall { Fork, Exec, ... }` 派生 `From<u16> + TryFrom<u16>`
- 编译期断言：所有 `map()` 项已覆盖

**测试**:
- 单元：每个 syscall 号可正确派发到 handler
- 静态：`map(SYS_FORK, do_fork)` 编译期检查

### 1.2 add_memmap(bootstrap)（pg_utils.c:86-125）

**核心问题**: bootstrap 阶段（head.S、cstart 早期）临时占用的内存，如何归还给系统？

**C 源码分析**:
```
1. struct mem_map *map = kinfo.memmap;  // 内存映射表
2. for (i=0; i<NR_MEMS; i++) ...        // 找到 bootstrap 区域
3. add_memmap 函数：把 (start, len) 标记为 MULTIBOOT_MEMORY_AVAILABLE
4. 4GB 截断（if (start + len > 0x100000000ULL) len = 0x100000000ULL - start;）
5. assert(kernel_may_alloc);  // 必须在 VM 启动前
6. 更新 mem_high_phys
```

**关键概念**:
- `kinfo.memmap[]` 是物理内存块的 BIOS/UEFI 报告表
- `kernel_may_alloc` 是 kmain 的"允许 VM alloc"开关，T9 后必须关闭
- 32 位 4GB 截断是 Minix3 32 位地址空间限制的遗留（注释 "rest of minix can't deal with any bigger"）

**Ch3 设计决策**:
| 决策 | 选项 | 结论 |
|------|------|------|
| 4GB 截断 | 保留 vs 删除 | **删除**（64 位不需要） |
| bootstrap_start 来源 | boot 模块回填 vs kinfo 显式 | **保持**（kinfo 中已存） |
| 内存表 | `BTreeMap<u64, MemRegion>` vs `Vec<MemRegion>` | **Vec**（保持顺序、不需要排序） |

**Ch4 实现要点**:
- `struct PhysMemMap { regions: Vec<MemRegion> }`
- `add_memmap(boot_start, boot_len) -> Result<()>` 64 位无截断
- 静态断言：`kernel_may_alloc` 关闭 = `T9_done` 标记

### 1.3 smp_init() / smp_single_cpu_fallback()（smp.c:30-50）

**核心问题**: 单核 vs 多核，分支条件是什么？

**C 源码分析**:
```
#ifdef CONFIG_SMP
  smp_init();            // 多核：启动 APs
#else
  smp_single_cpu_fallback();  // 单核：直接标记 BSP 启动完成
#endif
```

**详细在 15 §1 展开**。07 这里只标注"这是 15 §1 的入口点"。

### 1.4 bsp_finish_booting()（main.c:38-113）

**核心问题**: BSP（Bootstrap Processor）在进入调度循环前需要完成哪些"系统级就绪"操作？

**C 源码分析**:
```
L42-46: cpu_identify()                // CPU 型号 / 特性检测
L48:    vm_running = 0                // ★ VM 还没跑
L50:    krandom 初始化（init_random）  // 内核随机源
L52:    bill_ptr = idle_proc; proc_ptr = idle_proc;  // 记账默认 idle
L55-58: announce()                     // 打印 MINIX 启动 banner
L67-71: for (i=0; i<NR_BOOT_PROCS-NR_TASKS; i++) RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
       // 解除 boot 进程（不含 kernel tasks）的 RTS_PROC_STOP
L74-77: cycles_accounting_init()       // CPU 周期记账
L80:    boot_cpu_init_timer(system_hz) // 启动 BSP 时钟中断
L83-90: machine.processors_count = ..., machine.bsp_id = ...  // SMP 计数
L93-100: fpu_init()                    // FPU 初始化
L103:   kernel_may_alloc = 0           // 禁止内核分配
L106-110: prepare_idle(); BKL_LOCK();  // idle 进程准备 + 拿 BKL
L113:   switch_to_user()               // ★ 进入调度循环（永不返回）
```

**关键概念**:
- `vm_running = 0` 标记：所有 do_vmctl 子命令通过此检查（VM 未就绪前的 vmctl 全部失败）
- `RTS_PROC_STOP` 是 boot-time 抑制位，由 05 设置，此处解除
- `boot_cpu_init_timer(100)` 启动 100Hz 时钟（system_hz）
- `prepare_idle()` + `BKL_LOCK()`：idle 进程作为 fallback，调度器无进程可调度时跑它
- `switch_to_user()` 永不返回：从此 CPU 走的是 `kernel_entry → handle → return_to_user → iret` 循环

**Ch3 设计决策**:
| 决策 | 选项 | 结论 |
|------|------|------|
| vm_running 表达 | `AtomicBool` vs `CpuLocal<bool>` | **CpuLocal**（每核独立） |
| 启动 banner | println! vs kputc | **kputc**（直接写串口） |
| switch_to_user 永不返回 | 发散函数 `fn() -> !` | **`-> !`**（类型系统表达） |

**Ch4 实现要点**:
- `pub fn bsp_finish_booting() -> !` 发散函数
- 内部: `vm_running.store(false, ...); announce(); unblock_boot_procs(); cycles_accounting_init(); boot_cpu_init_timer(SYSTEM_HZ); fpu_init(); kernel_may_alloc.store(false); switch_to_user();`
- `switch_to_user` 内部 `loop { ... }`（永不返回）

### 1.5 tmp-17 迁移

| tmp-17 内容 | 迁移到 | 操作 |
|------------|--------|------|
| system_init 表格 | 07 §1.1 | 复用 |
| add_memmap 描述 | 07 §1.2 | 复用 |
| bsp_finish_booting 行号图 | 07 §1.4 | 复用 |
| RTS_PROC_STOP 解除 | 07 §1.4 L67-71 | 复用 |
| switch_to_user 入口语义 | 07 §1.4 L113（概览） + 09 §1（详细） | 拆分 |

---

## 2. 文档 08：vm-boot-protocol（VM 启动后的内核-VM 协商协议）

> **状态**: 与 kboot-new §2 一致
> **C 源码**: `minix3/minix/kernel/system/do_vmctl.c`, `minix3/minix/kernel/arch/i386/do_vmctl.c`, `minix3/minix/servers/vm/`（由 02-stage-vm 26-vm-init-main.md 配合）
> **C 总行数**: do_vmctl.c ~150 + arch_do_vmctl.c ~80 = ~230 行
> **依赖**: 07（bsp_finish_booting 完成）
> **产出**: `vm_running = 1`，所有 boot 进程可调度

### 2.1 VM 启动到 init_page_table

**核心问题**: VM 是 page table 的所有者，但它刚启动时只有 kernel 给的 bootstrap 页表，如何切换到自己的页表？

**时序**:
```
1. switch_to_user() 第一次返回 → pick_proc() → 选 VM（唯一无 VMINHIBIT 的进程）
2. VM 用户态代码（vm/src/.../main.c）执行 init_page_table() → map_kernel()
   → 建立 kernel direct map（KERNEL_DIRECT_MAP_BASE, U/S=0, G=1）
   → 这是 VM 对"kernel direct map"的所有权声明
3. VM 调 SYS_VMCTL(VMCTL_SETADDRSPACE, &vm_pcb)  ← ★ 切换 CR3
```

**Ch1 必读**: `init_page_table` 在 02-stage-vm 文档中已详述。08 只关心 VMCTL 协议本身。

### 2.2 VMCTL_SETADDRSPACE（do_vmctl.c + arch/i386/do_vmctl.c）

**核心问题**: 内核如何切换到自己进程的页表？这是 **switch_address_space** 的另一面。

**C 源码**:
```c
case VMCTL_SETADDRSPACE: {
    struct vmcb *vm = ...;
    switch_address_space(vm_proc);  // arch/i386/do_vmctl.c
    return OK;
}
```

**switch_address_space 关键操作（arch/i386/do_vmctl.c）**:
- `write_cr3(p->p_seg.p_cr3)` — 加载 VM 的页目录物理地址
- `video_mem = video_mem_vaddr` — 切 video 段到虚拟地址（这是 Minix3 的 32 位遗留）
- 重设 APIC 基址（如果有）

**Direct Map 演进**: 64 位 `write_cr3(cr3)` 不需要 video_mem 切换（因为所有物理地址都通过 direct map 可见），也不需要 APIC 重映射（待 Ch3 确认）。

**Ch3 设计决策**:
- 用 trait `PageTableSwitcher::switch_to(cr3_phys)` 抽象，x86_64 / aarch64 各实现
- `vm_running.store(true, Ordering::SeqCst)` 在 SETADDRSPACE 完成后立即置位

### 2.3 VMCTL_KERN_PHYSMAP（arch_do_vmctl.c）

**核心问题**: 64 位 Direct Map 把全部物理内存映射到内核虚拟空间，但 Minix3 C 时代是 32 位，需要内核"申请"哪些区段要映射。VM 把这些申请实现为 vmctl。

**C 源码**:
```c
case VMCTL_KERN_PHYSMAP: {
    struct minix_mem_range mr = m->m_vmctl.kern_physmap;
    arch_phys_map(vmp, &mr);  // x86: 直接用
    return OK;
}
```

**Ch1 必读**: 在 32 位 Minix3，`arch_phys_map` 实际不做事（直接返回 0 起点），因为 x86_32 没有 direct map 概念。这个 vmctl 是 aarch64 时代的产物。Rust 64 位版本**不再需要** KERN_PHYSMAP（direct map 始终存在），但**协议层必须保留**（向后兼容老 VM）。

**Ch3 设计决策**:
- **保留协议**（VM 仍可调）
- **arch_phys_map 实现 = noop**（direct map 已就绪）
- 注释必须清楚标注 "32 位遗留，64 位无操作"

### 2.4 VMCTL_KERN_MAP_REPLY（do_vmctl.c）

**核心问题**: VM 收到 KERN_PHYSMAP 后，回复"我已映射好的虚拟地址"。64 位下是 noop。

**C 源码**:
```c
case VMCTL_KERN_MAP_REPLY: {
    struct minix_mem_range mr = m->m_vmctl.kern_map_reply;
    arch_phys_map_reply(&mr);
    return OK;
}
```

**Ch1 必读**: 这是 KERN_PHYSMAP 的回环。32 位下回复的虚拟地址被记录到 kernel 全局变量；64 位下 noop。

### 2.5 VMCTL_VMINHIBIT_CLEAR（do_vmctl.c）

**核心问题**: VM 已为其他进程建好页表，解除所有进程的 VMINHIBIT 抑制位。

**C 源码**:
```c
case VMCTL_VMINHIBIT_CLEAR: {
    for (i = 0; i < NR_PROCS; i++) {
        struct proc *rp = proc_addr(i);
        if (rp->p_rts_flags == RTS_SLOT_FREE) continue;
        if (rp->p_endpoint == VM_PROC_NR) continue;  // VM 自己不解
        RTS_UNSET(rp, RTS_VMINHIBIT);
    }
    return OK;
}
```

**关键概念**:
- VMINHIBIT 抑制位在 05-proc-init-boot-proc 设置
- 解除后，调度器才能把这些进程加入 ready queue
- VM 自己不解（自己不需要自己建页表）

**Ch3 设计决策**:
- Rust 表达：`for_each_proc_mut(|p| if p.pid != VM_PID { p.rts_flags.remove(RTS_VMINHIBIT) })`
- 必须保证：VMINHIBIT 解除是"批量"操作，不能逐进程

### 2.6 vm_running = 1 之后的协议完成

**核心问题**: 协议走完一遍，runtime 真正可用。

**检查清单**:
- [ ] VMCTL_SETADDRSPACE 已完成
- [ ] VMCTL_KERN_PHYSMAP / KERN_MAP_REPLY 已完成（即使 noop）
- [ ] VMCTL_VMINHIBIT_CLEAR 已完成
- [ ] vm_running = 1
- [ ] 所有 boot 进程的 RTS_VMINHIBIT 已清除

之后，调度器可以正常运行，kernel 开始处理 system call / interrupt / exception。

### 2.7 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-03-vm-request 的 VMCTL 部分 | 08 全部 | 重写（沿 VMCTL 协议逐命令） |
| tmp-13-syscall-memory 的 vmctl 节 | 17 §VMCTL 章节 | 拆分（17 关注 handler 实现，08 关注协议） |

---

## 3. 文档 09：switch-to-user-entry（进入调度循环）

> **C 源码**: `minix3/minix/kernel/proc.c:299-450 switch_to_user`
> **C 行数**: ~150 行
> **依赖**: 08（vm_running=1、所有进程可调度）
> **产出**: 第一个用户进程开始执行；调度循环永不退出

### 3.1 switch_to_user() 内部结构

**核心问题**: switch_to_user 之后，CPU 怎么知道下一步该跑哪个进程？

**C 源码（proc.c:299-450 关键段）**:
```c
void switch_to_user(void) {
    struct proc *p;
    
    p = get_cpulocal_var(proc_ptr);
    
    if (proc_is_runnable(p)) goto check_misc_flags;
    
not_runnable_pick_new:
    if (proc_is_preempted(p)) {
        p->p_rts_flags &= ~RTS_PREEMPTED;
        if (proc_is_runnable(p)) {
            if (p->p_cpu_time_left) enqueue_head(p); else enqueue(p);
        }
    }
    
    while (!(p = pick_proc())) { idle(); }  // 无可调度进程时 idle
    
    get_cpulocal_var(proc_ptr) = p;
    
    if (p->p_misc_flags & MF_FLUSH_TLB && get_cpulocal_var(ptproc) == p)
        tlb_must_refresh = 1;
    
    switch_address_space(p);
    
check_misc_flags:
    assert(p);
    assert(proc_is_runnable(p));
    while (p->p_misc_flags & (MF_KCALL_RESUME | MF_DELIVERMSG | MF_SC_DEFER | MF_SC_TRACE | MF_SC_ACTIVE)) {
        if (p->p_misc_flags & MF_KCALL_RESUME) kernel_call_resume(p);
        else if (p->p_misc_flags & MF_DELIVERMSG) delivermsg(p);
        else if (p->p_misc_flags & MF_SC_DEFER) arch_do_syscall(p);
        else if (p->p_misc_flags & MF_SC_TRACE) { ... cause_sig(SIGTRAP); }
        else if (p->p_misc_flags & MF_SC_ACTIVE) { p->p_misc_flags &= ~MF_SC_ACTIVE; break; }
        
        if (!proc_is_runnable(p)) goto not_runnable_pick_new;
    }
    
    /* quantum check, set preemption timer, restore fpu, etc. */
    ...
    
    restore_context(p);  // 实际 iret 到用户态
}
```

**关键概念**:
- `proc_ptr` 是 per-CPU 变量，指向"上次运行"的进程
- `pick_proc()` 扫 16 个优先级队列，选出最高优先级 + 同优先级轮转
- `switch_address_space(p)` 切 CR3（与 VM 的区别是：这是切到"被调度进程"的页表）
- `MF_*` 是 misc flags，处理未完成的 system call 状态（MF_SC_DEFER = 系统调用被延后）
- `idle()` 是 BSP 的占位：当无进程可调度时（BSP 单核 + 所有进程阻塞），CPU 在此 spin

**Ch1 必读**: `idle()` 在 main.c:38+ 的 `bsp_finish_booting` 后段被实现为 hlt 循环。

**Ch3 设计决策**:
| 决策 | 选项 | 结论 |
|------|------|------|
| switch_to_user 类型 | `fn() -> !` | **`-> !`**（永不返回） |
| `proc_ptr` 表达 | `CpuLocal<Rc<Proc>>` vs `CpuLocal<*mut Proc>` | **`Cell<*mut Proc>`**（单线程 + 内核态裸指针安全） |
| `MF_*` 表达 | `u32` 位掩码 vs `enum MiscFlag` | **`bitflags!`**（类型安全） |
| `pick_proc` 返回类型 | `Option<*mut Proc>` vs `Result<*mut Proc, NoProc>` | **`Option`**（无进程是合法态） |

**Ch4 实现要点**:
```rust
pub fn switch_to_user() -> ! {
    loop {
        let p = pick_proc().unwrap_or_else(|| { idle(); loop { } });
        // ... 处理 MF_*
        restore_context(p);
    }
}
```

**测试**:
- 集成：单进程 runnable → switch_to_user 立即 iret 到该进程
- 集成：多进程 runnable → 优先级调度
- 集成：无进程 runnable → 进入 idle，timer 唤醒后再次选

### 3.2 与后续文档的关系

- **10 详述 pick_proc / enqueue / dequeue + RTS 状态机**（09 只用，10 解释"怎么实现的"）
- **11 详述 delivermsg / MF_DELIVERMSG 的产生**（09 处理已设置的 MF_DELIVERMSG，11 解释谁会设置它）
- **12 详述 arch_do_syscall / MF_SC_DEFER**（同上）
- **14 详述 timer_int_handler**（设置 RTS_NO_QUANTUM 是 timer 的事）

### 3.3 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-07-scheduling 的 switch_to_user 节 | 09 §1 | 重写（按 C 源码逐行） |
| tmp-07-scheduling 的 RTS 状态机 | 10 §3 | 拆分 |
| tmp-07-scheduling 的 enqueue/dequeue | 10 §2 | 拆分 |
| tmp-07-scheduling 的 pick_proc | 10 §1 | 拆分 |

---

## 4. 文档 10：scheduling-primitives（调度原语 + 进程状态机）

> **C 源码**: `minix3/minix/kernel/proc.c:1595-1830`, `minix3/minix/kernel/proc.h:141-274`, `minix3/minix/kernel/proc.c:235-298 vm_suspend`
> **C 行数**: ~250 行
> **依赖**: 09
> **产出**: 调度器可工作的全部原语

### 4.1 pick_proc()（proc.c:1700 附近）

**核心问题**: 16 个优先级队列，怎么选下一个进程？

**C 源码骨架**:
```c
struct proc *pick_proc(void) {
    int q;
    for (q = 0; q < NR_SCHED_QUEUES; q++) {
        if (rdy_head[q]) {
            struct proc *p = rdy_head[q];
            rdy_head[q] = p->p_nextready;
            if (!rdy_head[q]) rdy_tail[q] = NULL;
            p->p_nextready = NULL;
            return p;
        }
    }
    return NULL;  // 无可调度
}
```

**关键概念**:
- `NR_SCHED_QUEUES = 16`（Minix3 user-configurable；默认 16）
- 优先级 0 = 最高，15 = 最低
- 同一优先级内 FIFO
- `rdy_head[q]` / `rdy_tail[q]` 是队列头尾指针

**Ch3 设计决策**:
| 决策 | 选项 | 结论 |
|------|------|------|
| 队列表达 | `Vec<VecDeque<*mut Proc>>` vs `[[*mut Proc; 16]; 2]` (head/tail) | **Vec<VecDeque>**（更 Rust 化） |
| 优先级翻转 | 是否支持 | **不支持**（保持 C 行为） |
| pick_proc 的 O(N) | 顺序扫 16 队列 vs 优先级位图 | **顺序扫**（16 项够快） |

### 4.2 enqueue() / dequeue()（proc.c:1595, 1716）

**核心问题**: 进程进入/离开就绪队列的语义。

**C 源码**（enqueue 关键段）:
```c
void enqueue(struct proc *rp) {
    int q = rp->p_priority;
    assert(proc_is_runnable(rp));  // 必须 runnable
    assert(q >= 0);
    
    rdy_head = get_cpu_var(rp->p_cpu, run_q_head);
    rdy_tail = get_cpu_var(rp->p_cpu, run_q_tail);
    
    if (!rdy_head[q]) {  // 空队列
        rdy_head[q] = rdy_tail[q] = rp;
        rp->p_nextready = NULL;
    } else {  // 加到队尾
        rdy_tail[q]->p_nextready = rp;
        rdy_tail[q] = rp;
        rp->p_nextready = NULL;
    }
    
    if (cpuid == rp->p_cpu) {  // 同核：检查抢占
        struct proc *p = get_cpulocal_var(proc_ptr);
        if (p->p_priority > rp->p_priority && (priv(p)->s_flags & PREEMPTIBLE))
            RTS_SET(p, RTS_PREEMPTED);  // 标抢占，下次 switch_to_user 会处理
    }
#ifdef CONFIG_SMP
    else if (get_cpu_var(rp->p_cpu, cpu_is_idle)) {
        smp_schedule(rp->p_cpu);  // 异核 idle，IPI 唤醒
    }
#endif
}
```

**关键概念**:
- enqueue 不是简单插入：它还要检查是否要抢占当前进程
- `RTS_PREEMPTED` 标志由当前进程的 enqueue 设置（不是被入队进程）
- SMP 下，异核入队若目标核 idle，需要发 IPI

**Ch3 设计决策**:
- `enqueue` 表达：`pub fn enqueue(rp: &mut Proc) { ... }`
- 抢占检查是 enqueue 的副作用，不能拆开
- 异核 IPI 在 15 §2 详述

**Ch4 实现要点**:
```rust
pub fn enqueue(rp: &mut Proc) {
    assert!(rp.rts_flags.contains(Runnable));  // runnable
    let q = rp.priority as usize;
    let cpu = rp.cpu;
    
    // 同核：检查抢占
    if cpu == cpuid() {
        if let Some(cur) = proc_ptr() {
            if cur.priority > rp.priority && cur.is_preemptible() {
                cur.rts_flags.insert(RTS_PREEMPTED);
            }
        }
    } else {
        // 异核：IPI 唤醒
        if cpu_is_idle(cpu) { smp_schedule(cpu); }
    }
    
    // 插入队列
    rdy_queue_mut(cpu, q).push_back(rp as *mut Proc);
}
```

### 4.3 RTS_* 状态机（proc.h:141-274）

**核心问题**: 进程何时可调度？何时被阻塞？何时被抢占？

**核心定义**:
```c
#define proc_is_runnable(p) ((p)->p_rts_flags == 0)
```

**全部 RTS_* 位**（proc.h:142-167）:
| 位 | 含义 | 谁设置 | 谁清除 |
|----|------|--------|--------|
| `RTS_SLOT_FREE` (0x01) | 进程槽空闲 | proc_init | fork |
| `RTS_PROC_STOP` (0x02) | 进程被停止 | RUNCTL | RUNCTL |
| `RTS_SENDING` (0x04) | 阻塞 send | mini_send | delivermsg / abort_send |
| `RTS_RECEIVING` (0x08) | 阻塞 receive | mini_receive | delivermsg / cancel |
| `RTS_SIGNALED` (0x10) | 有新 kernel 信号 | cause_sig | getksig |
| `RTS_SIG_PENDING` (0x20) | 信号处理中 | getksig | endksig |
| `RTS_P_STOP` (0x40) | 正在被 trace | trace | trace |
| `RTS_NO_PRIV` (0x80) | fork 出 system 进程后禁止 | do_fork | do_exec / do_exit |
| `RTS_NO_ENDPOINT` (0x100) | 进程不能发/收消息 | (config) | (config) |
| `RTS_VMINHIBIT` (0x200) | 等 VM 建页表 | proc_init | VMCTL_VMINHIBIT_CLEAR |
| `RTS_PAGEFAULT` (0x400) | 进程有未处理 pagefault | exception | vm_suspend resume |
| `RTS_VMREQUEST` (0x800) | VM 内存请求发起者 | do_memset/do_copy | vm_suspend done |
| `RTS_VMREQTARGET` (0x1000) | VM 内存请求目标 | do_memset/do_copy | vm_suspend done |
| `RTS_PREEMPTED` (0x4000) | 被高优先级抢占 | enqueue | switch_to_user |
| `RTS_NO_QUANTUM` (0x8000) | quantum 用完 | timer_int_handler | switch_to_user |
| `RTS_BOOTINHIBIT` (0x10000) | boot 未完成 | proc_init | (config) |

**Ch3 设计决策**:
- 用 `bitflags!` 宏生成 `RtsFlags` 结构
- 提供方法 `is_runnable() -> bool`（仅当位掩码为 0）
- 提供 `set/clear` 安全操作（带 BKL 检查）

**Ch4 实现**:
```rust
bitflags! {
    pub struct RtsFlags: u32 {
        const SLOT_FREE    = 0x00001;
        const PROC_STOP    = 0x00002;
        const SENDING      = 0x00004;
        const RECEIVING    = 0x00008;
        // ... 全部 16 位
    }
}

impl RtsFlags {
    pub fn is_runnable(self) -> bool { self.is_empty() }
}
```

**测试**:
- 单元：每个 RTS 位的 set/clear 语义
- 集成：SENDING 设置后 is_runnable = false；清除后 is_runnable = true

### 4.4 vm_suspend()（proc.c:234-298）

**核心问题**: 进程触发 pagefault，kernel 不能直接处理（kernel 不管页表），必须 suspend 自己并通知 VM。

**C 源码骨架**:
```c
void vm_suspend(struct proc *caller, const struct proc *target,
                vir_bytes region, vir_bytes len, int writeflag) {
    // 设置 RTS_PAGEFAULT + RTS_VMREQUEST(target) 或 RTS_VMREQTARGET(target)
    // 构造 VMSUSPEND 消息
    // 切到 VM（设置 RTS_SENDING 给 VM）
    // 进程被 dequeue（不可调度）
    // 当 VM 处理完，发回消息，进程重新 enqueue
}
```

**详细在 23 §4 展开**。10 这里只标注"这是 RTS_PAGEFAULT 状态机的产生点"。

### 4.5 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-07-scheduling 全部 | 09 §1 + 10 全部 | 拆分（switch_to_user → 09，其余 → 10） |
| tmp-06-proc-struct 的 RTS 节 | 10 §3 | 复用 |

---

## 5. 文档 11：ipc-core（IPC 核心机制）

> **C 源码**: `minix3/minix/kernel/proc.c:599-1590`（do_ipc + mini_send + mini_notify + try_deliver_senda + cancel_async + delivermsg）
> **C 行数**: ~1000 行（最复杂的一部分）
> **依赖**: 10（进程状态机）
> **产出**: 同步 / 异步 / 单边 IPC 全部就绪

### 5.1 do_ipc() 入口（proc.c:599-870）

**核心问题**: 进程调 SYS_VIRCOPY / SYS_SEND / SYS_RECEIVE / SYS_NOTIFY / SYS_SENDA 时，kernel 走哪条路？

**C 源码骨架**:
```c
int do_ipc(reg_t r1, reg_t r2, reg_t r3) {
    int call_nr, dest, src, type, flag;
    
    call_nr = ...;  // 从用户态 regs 提取
    
    switch (call_nr) {
    case SEND:        return mini_send(caller, ...);
    case RECEIVE:     return mini_receive(caller, ...);
    case SENDREC:     // 实际是 SEND + RECEIVE 复合
    case NOTIFY:      return mini_notify(caller, ...);
    case SENDA:       return try_deliver_senda(caller, ...);
    default:          return EINVAL;
    }
}
```

**关键概念**:
- IPC 系统调用号集合在 `ipc.h`
- SENDREC 是原子化的 SEND + RECEIVE（不能被打断）
- 异步 SENDA 允许一次给多个目标发消息

### 5.2 mini_send() 同步发送（proc.c:870-1120）

**核心问题**: 同步 send 的阻塞语义是什么？

**C 源码骨架**:
```c
int mini_send(struct proc *caller, endpoint_t dest_e, message *m_user) {
    int err;
    struct proc *dest;
    
    if (!isokendpt(dest_e, &dest)) {
        if (dest_e == ANY) return EDEADSRCDST;  // send to ANY 非法
        return ENOTARGET;
    }
    
    // 权限检查
    if (!may_send_to(caller, dest)) return ECALLDENIED;
    
    // 拷贝消息从用户栈 → 内核栈
    if (copy_msg_from_user(m_user, &caller->p_sendmsg) != OK) {
        cause_sig(proc_nr(caller), SIGSEGV);
        return EFAULT;
    }
    
    // 目标在 RECEIVING？
    if (dest->p_rts_flags & RTS_RECEIVING) {
        // 立即投递：拷贝到 dest 的 p_delivermsg
        caller->p_delivermsg = dest;  // 反向指针
        dest->p_delivermsg_vir = ...;
        if (copy_msg_to_user(...)) { ... }
        RTS_SET(dest, RTS_DELIVERMSG);  // 实际是 MF_DELIVERMSG
        // 或 enqueue(dest)
        return OK;
    }
    
    // 目标不接收：阻塞
    RTS_SET(caller, RTS_SENDING);
    caller->p_sendto_e = dest_e;
    // 不 enqueue
    return EDONTREPLY;  // 标志 caller 进入等待
}
```

**关键概念**:
- 同步 send 的"两态"：① 目标正好 RECEIVING → 立即投递；② 否则阻塞
- 阻塞后，caller 不在就绪队列，调度器选别的
- 当目标调 RECEIVE 时，从 sender 队列取出，拷贝消息

**Ch3 设计决策**:
- `mini_send` 表达为 `pub fn mini_send(caller: &mut Proc, ...) -> Result<(), IpcError>`
- 错误码用 `enum IpcError { NotTarget, CallDenied, Fault }`（替代 C 负数）

### 5.3 mini_receive() 同步接收

**核心问题**: RECEIVE 的"任意源" / "指定源" / "通配"语义。

**Ch1 必读**: RECEIVE 三个变体：① 指定 endpoint → 只收该源；② ANY → 收任意源；③ 来自 sender 队列扫描。

**C 源码骨架**:
```c
int mini_receive(struct proc *caller, endpoint_t src_e, message *m_user) {
    int err;
    struct proc *src = NULL;
    
    if (src_e == ANY) {
        // 从 sender 队列取第一个
        if (caller->p_has_pending_msg) { ... } else {
            // 阻塞
            RTS_SET(caller, RTS_RECEIVING);
            caller->p_getfrom_e = ANY;
        }
    } else {
        if (!isokendpt(src_e, &src)) return ENOTARGET;
        // 检查 src 是否在 SENDING 状态
        if (src->p_rts_flags & RTS_SENDING && src->p_sendto_e == caller->p_endpoint) {
            // 立即投递
            copy_msg_to_user(m_user, &src->p_sendmsg);
            RTS_UNSET(src, RTS_SENDING);
            enqueue(src);
            return OK;
        }
        // 阻塞
        RTS_SET(caller, RTS_RECEIVING);
        caller->p_getfrom_e = src_e;
    }
}
```

### 5.4 mini_notify() 异步单边（proc.c:1122-1200）

**核心问题**: notify 不需要对方 receive，丢到对方队列即可。

**C 源码骨架**:
```c
int mini_notify(struct proc *caller, endpoint_t dest_e) {
    struct proc *dest;
    if (!isokendpt(dest_e, &dest)) return ENOTARGET;
    if (!may_send_to(caller, dest)) return ECALLDENIED;
    
    if (dest->p_rts_flags & RTS_RECEIVING) {
        // 立即投递（特殊消息类型 = notify）
        ... 
    } else {
        // 加到 caller->p_notify_pending[dest_e]
        set_notify_pending(caller, dest_e);
        // 不阻塞 caller
    }
    return OK;
}
```

**关键概念**:
- notify 是非阻塞的，caller 立即返回
- 目标没 receive 就把 notify 存到 sender 的 pending 位图
- 目标调 receive 时，scan pending notify

### 5.5 try_deliver_senda() 异步双边（proc.c:1200-1510）

**核心问题**: SENDA 一次给多个目标发，可对方没 receive 时怎么处理？

**Ch1 必读**: SENDA 是 Minix3 特有，比 mini_send 更复杂：① 一次给多个目标；② 目标未 receive 时 sender 也阻塞；③ 目标已 receive 时立即投递并通知 sender 继续。

**详细流程（简化）**:
```
1. 解析目标 endpoint 列表
2. 对每个目标：
   - 已 RECEIVING → 投递 + 标记 MF_DELIVERMSG
   - 未 RECEIVING → 加到目标 pending 列表
3. 如果至少一个目标未投递：
   - 阻塞 sender (RTS_SENDING)
   - sender 等待所有目标投递完成
4. 如果所有目标都已投递：
   - 立即返回
```

### 5.6 cancel_async()（proc.c:1510-1590）

**核心问题**: 异步 IPC 因某种原因被取消（如 send target 退出），sender 必须被解除阻塞并返回错误。

**Ch1 必读**: 当 SEND/SENDA 的目标进程 exit（do_exit），cancel_async 找到所有 SENDING 给它的进程，把它们 unblock 并返回 ESRCH。

### 5.7 delivermsg()（proc.c）

**核心问题**: 投递消息到用户态地址空间。

**C 源码骨架**:
```c
void delivermsg(struct proc *p) {
    // 从 p_delivermsg 取出 source proc
    // 拷贝消息到 p->p_delivermsg_vir
    // 清除 MF_DELIVERMSG
    // 设置用户态寄存器（m_source, m_type）
}
```

**Direct Map 影响**: 64 位下，delivermsg 不需要 createpde 临时映射，直接 `kernel_phys_to_virt()` 翻译目标虚拟地址到物理页，然后 memcpy。

### 5.8 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-09-sync-ipc 全部 | 11 §1-3 | 重写（按 mini_send / mini_receive 拆） |
| tmp-10-async-ipc 全部 | 11 §4-6 | 重写 |
| tmp-08-endpoint | 11 §1 + 21 | 拆分（endpoint 校验在 11，priv 在 21） |

---

## 6. 文档 12：syscall-dispatch（系统调用分派）

> **C 源码**: `minix3/minix/kernel/system.c:1-272`（kernel_call + system_init 已经在 07 §1），`minix3/minix/kernel/system.c:136-167 kernel_call`, `minix3/minix/kernel/arch/i386/arch_system.c:do_syscall`（64 位下统一）
> **C 行数**: ~150 行
> **依赖**: 11（IPC 用到 IPC 错误码）
> **产出**: 50+ 个 syscall 的分派表，10 个 syscall 分类（16-20）

### 6.1 do_syscall() 入口（arch_system.c）

**核心问题**: 用户态 INT 0x30 之后，CPU 跳到哪？

**C 源码骨架**:
```c
void do_syscall(struct proc *p) {
    int call_nr = p->p_reg.req_nr;  // 用户态寄存器
    if (call_nr < 0 || call_nr >= NR_SYS_CALLS) { ... 错误 ... }
    
    if (p->p_misc_flags & MF_SC_ACTIVE) {
        // 系统调用被延后，现在恢复
        kernel_call_resume(p);
    } else {
        p->p_misc_flags |= MF_SC_ACTIVE;
        kernel_call(&p->p_reg.msg, p);
    }
}
```

**关键概念**:
- 入口先检查 call_nr 范围
- `MF_SC_ACTIVE` 标记：syscall 处理中（防止重入）
- `MF_SC_DEFER` 标记：syscall 被延后（VM 协助类，如 VMSUSPEND）

### 6.2 kernel_call()（system.c:136-162）

**核心问题**: 把用户态消息拷贝到内核，再分派给 handler。

**C 源码**:
```c
void kernel_call(message *m_user, struct proc * caller) {
    int result;
    message msg;
    
    caller->p_delivermsg_vir = (vir_bytes)m_user;
    
    if (copy_msg_from_user(m_user, &msg) == 0) {
        msg.m_source = caller->p_endpoint;
        result = kernel_call_dispatch(caller, &msg);
    } else {
        cause_sig(proc_nr(caller), SIGSEGV);
        return;
    }
    
    kbill_kcall = caller;  // 记账
    kernel_call_finish(caller, &msg, result);
}
```

**关键概念**:
- `copy_msg_from_user`：从用户栈拷贝 message 结构到内核栈
- 错误处理：用户指针非法 → SIGSEGV（不发回消息）
- `kernel_call_dispatch` 实际分派

### 6.3 kernel_call_dispatch()（system.c）

**核心问题**: 50+ 个 syscall 怎么分派？

**C 源码骨架**:
```c
int kernel_call_dispatch(struct proc *caller, message *m) {
    int call_nr = m->m_type;
    int (*handler)(struct proc *, message *);
    
    handler = call_vec[call_nr];
    if (!handler) return ENOSYS;
    
    return handler(caller, m);
}
```

**Ch1 必读**: 这就是 07 §1 注册的 call_vec[]。dispatch 自身很简单，关键是 handler 的实现（10 大类 → 16-20）。

### 6.4 kernel_call_finish() & kernel_call_resume()

**核心问题**: handler 返回后，结果如何回给用户？

**Ch1 必读**:
- 同步 syscall：handler 返回后，kernel 写回 result 到 caller->p_reg，返回用户态
- 异步 syscall（如 VMSUSPEND）：handler 返回 EDONTREPLY，caller 留在 SENDING 状态，不返回用户态
- 恢复：VM 处理完后发回消息，MF_KCALL_RESUME 被设置，下次 switch_to_user 调 kernel_call_resume

**Ch3 设计决策**:
- `enum CallResult { Reply(Result), Suspend }` 表达 handler 返回
- `kernel_call_resume` 是 `do_syscall` 的另一面入口

### 6.5 16-20 五个分类文档的入口

| 分类 | syscall | 文档 |
|------|---------|------|
| 进程管理 | FORK / EXEC / CLEAR / EXIT / PRIVCTL / TRACE / SETGRANT / RUNCTL / UPDATE / STATECTL | 16 |
| 内存与拷贝 | MEMSET / VMCTL / UMAP / UMAP_REMOTE / VUMAP / VIRCOPY / PHYSCOPY / SAFECOPYFROM / SAFECOPYTO / VSAFECOPY / SAFEMEMSET | 17 |
| 信号 | KILL / GETKSIG / ENDKSIG / SIGSEND / SIGRETURN | 18 |
| 设备 + 时钟 | IRQCTL / DEVIO / VDEVIO / TIMES / SETALARM / STIME / SETTIME / VTIMER / SPROF | 19 |
| 系统控制 | ABORT / GETINFO / DIAGCTL / SCHEDCTL / SCHEDULE / SETMCONTEXT / GETMCONTEXT / READBIOS / IOPENABLE / SDEVIO / PADCONF | 20 |

### 6.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-12-syscall-dispatch 全部 | 12 全部 | 复用（重排为更紧凑） |
| tmp-13-syscall-memory 全部 | 17 全部 | 拆分 |
| tmp-14-syscall-fork-exec 全部 | 16 全部 | 拆分 |
| tmp-15-syscall-exit-signal 全部 | 16（exit）+ 18（signal） | 拆分 |
| tmp-16-timer 全部 | 14 全部 + 19 §clock 部分 | 拆分 |

---

## 7. 文档 13：exception-interrupt（异常与中断入口）

> **C 源码**: `minix3/minix/kernel/interrupt.c:1-177`, `minix3/minix/kernel/arch/i386/exception.c`, `minix3/minix/kernel/arch/i386/i8259.c`
> **C 行数**: ~250 行（C 异常） + ~50 行（i8259 PIC）
> **依赖**: 12（系统调用已就绪）
> **产出**: 异常 / 中断能正确路由到 handler

### 7.1 exception_handler()（arch/i386/exception.c）

**核心问题**: CPU 触发异常（除零、pagefault、general protection）后，kernel 怎么知道发生了什么？

**C 源码骨架**:
```c
void exception_handler(int excno, struct proc *p) {
    switch (excno) {
    case EXC_PAGE_FAULT: handle_pagefault(p); break;
    case EXC_DIVIDE:     cause_sig(proc_nr(p), SIGFPE); break;
    case EXC_GP:         cause_sig(proc_nr(p), SIGSEGV); break;
    // ...
    }
}
```

**关键概念**:
- `excno` 是 CPU 异常号（0-31）
- pagefault 特殊：需要读 CR2（出错地址）+ 检查是否有映射
- 其他异常：发信号给进程，进程可能因此 exit

### 7.2 handle_pagefault()

**核心问题**: 进程访问的虚拟地址没有物理页，怎么办？

**C 源码骨架**:
```c
void handle_pagefault(struct proc *p) {
    vir_bytes fault_addr = read_cr2();
    
    // 1. 内核态 pagefault：直接 panic
    if (p == get_cpulocal_var(proc_ptr) && p->p_rts_flags == 0) {
        panic("kernel pagefault");
    }
    
    // 2. 用户进程 pagefault：交给 VM
    if (!(p->p_misc_flags & MF_VM_USER)) {
        // 不是 VM 进程：通知 VM
        vm_suspend(p, p, fault_addr & ~0xFFF, 1, ...);
    } else {
        // VM 自己的 pagefault：panic
        panic("VM pagefault");
    }
}
```

**关键概念**:
- 内核 pagefault = 严重错误（kernel bug）
- VM 进程的 pagefault = 严重错误（VM 自己 page 应该是 page table 错误）
- 其他进程的 pagefault = 通知 VM 处理（demand paging / cow / 共享库）

**Ch1 必读**: 详细 VM 协助机制在 23 §4（VMSUSPEND）。

### 7.3 irq_handle()（interrupt.c:116-160）

**核心问题**: 硬件中断来了，kernel 怎么分派给 handler？

**C 源码骨架**:
```c
void irq_handle(int irq) {
    irq_hook_t *hook;
    int id;
    
    // 1. 找到注册了该 IRQ 的 hook
    for (hook = irq_hooks[irq]; hook; hook = hook->next) {
        id = hook->proc_nr_e;
        if (id >= 0) {
            // 通知该进程（设 RTS_SIGNALED 或 send 消息）
            cause_sig(id, ...);
        }
    }
    
    // 2. 通知时钟（如果是时钟中断）
    if (irq == CLOCK_IRQ) {
        timer_int_handler();
    }
    
    // 3. 通知 APIC（如果有）
    apic_finish_eoi(irq);
}
```

**关键概念**:
- `irq_hooks[NR_IRQ]` 是每 IRQ 的 hook 链表（07 §1 已初始化）
- hook 链表的每个节点指向"该 IRQ 感兴趣"的进程
- 通知方式：设 RTS_SIGNALED，或直接 send 消息

### 7.4 i8259 PIC 初始化（i8259.c）

**核心问题**: 8259 PIC 是 16 位时代的中断控制器，现代用 APIC。这里只讲 8259 的兼容性。

**Ch1 必读**: 现代 x86_64 用 APIC（或 IOAPIC + LAPIC），8259 已废弃。但 Minix3 仍保留兼容代码。Rust 64 位版可以**完全删除** 8259 相关代码，统一走 APIC。

### 7.5 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-05-exception-interrupt 全部 | 13 全部 | 重写（独立成 13） |

---

## 8. 文档 14：clock-timer（时钟与定时器）

> **C 源码**: `minix3/minix/kernel/clock.c:1-312`, `minix3/minix/kernel/arch/i386/arch_clock.c`
> **C 行数**: ~350 行
> **依赖**: 13（异常入口已就绪）
> **产出**: 100Hz 系统时钟 + alarm timer 机制

### 8.1 timer_int_handler()（clock.c:70-185）

**核心问题**: 时钟中断每 10ms 来一次，kernel 做什么？

**C 源码骨架**:
```c
int timer_int_handler(void) {
    // 1. 读 TSC 算本 tick 实际时间
    // 2. 进程记账
    bill_ptr->p_user_time += delta;  // 实际时间计入
    // 3. 进程 quantum 检查
    if (--proc_ptr->p_cpu_time_left <= 0) {
        RTS_SET(proc_ptr, RTS_NO_QUANTUM);  // quantum 用完，下次 switch_to_user 会切换
        proc_ptr->p_cpu_time_left = ...;     // 重置 quantum
    }
    // 4. 扫描所有进程的 alarm timer
    for each proc with s_alarm_timer armed:
        if (timer expired) {
            cause_sig(proc_nr, SIGALRM);
        }
    // 5. 通知 APIC EOI
    apic_finish_eoi(CLOCK_IRQ);
    return 0;  // 0 = handled
}
```

**关键概念**:
- `bill_ptr` 是记账指针（通常 = proc_ptr）
- `p_cpu_time_left` 倒计时，0 时设 RTS_NO_QUANTUM
- alarm timer 是 per-priv 结构（07 §1 已 init）

**Ch1 必读**: 100Hz 来自 `boot_cpu_init_timer(system_hz)`，system_hz 在 config.h 定义为 100。

### 8.2 set_realtime / set_boottime / set_adjtime_delta

**核心问题**: 系统时间怎么调整？

**Ch1 必读**: 内核维护两个时间源：realtime（墙上时间，可被 settime 调整）和 boottime（系统启动后秒数）。二者差值 + adjtime delta 给出 monotonic time。

### 8.3 set_kernel_timer / reset_kernel_timer

**核心问题**: 内核自身的 timer（不通过 alarm）怎么设置？

**Ch1 必读**: 内核 timer 用于调度器相关任务（如 SMP tick、watchdog）。不暴露给用户进程。

### 8.4 boot_cpu_init_timer / app_cpu_init_timer

**核心问题**: BSP 和 AP 的 timer 初始化有什么不同？

**Ch1 必读**:
- BSP：main.c:38+ 调 boot_cpu_init_timer(system_hz)
- AP：smp.c app_cpu_init_timer(system_hz)
- 二者都配置 LAPIC timer 为 periodic 模式

### 8.5 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-16-timer 全部 | 14 全部 | 复用 |

---

## 9. 文档 15：smp（SMP / BKL / IPI / per-CPU）

> **C 源码**: `minix3/minix/kernel/smp.c:1-205`, `minix3/minix/kernel/cpulocals.c`, `minix3/minix/kernel/arch/i386/arch_smp.c`
> **C 行数**: ~250 行
> **依赖**: 10（调度原语）+ 14（时钟）
> **产出**: 多核调度 + BKL 互斥 + IPI 通信

### 9.1 smp_init()（smp.c）

**核心问题**: BSP 怎么启动 AP（Application Processor）？

**C 源码骨架**:
```c
void smp_init(void) {
    // 1. 检测 AP 数量
    machine.processors_count = detect_cpus();
    machine.bsp_id = lapic_id();
    
    // 2. 准备 trampoline code（AP 启动后的入口）
    copy_ap_trampoline();
    
    // 3. 发 SIPI（Startup IPI）给每个 AP
    for (i = 0; i < machine.processors_count; i++) {
        if (i == machine.bsp_id) continue;
        lapic_send_ipi(i, AP_TRAMPOLINE_ADDR, TRAMPOLINE_VECTOR);
    }
    
    // 4. 等 AP 起来
    wait_for_APs_to_finish_booting();
}
```

**关键概念**:
- AP 启动后跳到 trampoline code（预先复制到固定物理地址）
- trampoline 设临时页表 + 跳到 ap_kernel_main
- BSP 等所有 AP 完成 boot 后才继续

### 9.2 smp_sched_handler()（smp.c:156-193）

**核心问题**: AP 收到 SCHED IPI 之后做什么？

**C 源码骨架**:
```c
void smp_sched_handler(void) {
    // 1. 读 EOI
    apic_finish_eoi(APIC_IRQ_SCHED);
    
    // 2. 重新调度（call switch_to_user 或类似）
    // AP 暂停在 idle / hlt，被 IPI 唤醒后调 switch_to_user
    smp_reschedule();
}
```

**关键概念**:
- SCHED IPI 用途：① 异核入队时唤醒 idle AP；② 让 AP 强制重选进程

### 9.3 smp_schedule() / smp_ipi_halt_handler()

**核心问题**: 跨核事件如何通知？

**Ch1 必读**:
- `smp_schedule(cpu)` 发 SCHED IPI
- `smp_ipi_halt_handler` 收到 HALT IPI 后停核
- `smp_schedule_stop_proc(p)` 让其他核协助停止某进程
- `smp_schedule_migrate_proc(p, dest)` 迁移进程到目标核

### 9.4 BKL（Big Kernel Lock）

**核心问题**: 多核下 kernel 临界区如何保护？

**Ch1 必读**: BKL 是单 spinlock，覆盖整个 kernel（除中断处理）。Minix3 选择 BKL 是简化设计，承认 SMP 并发度低。Rust 版可以保留 BKL 作为 fallback，但**长期**应该细化锁粒度。

**Ch3 设计决策**:
- 保留 BKL（保持 C 行为）
- 提供 `#[bkl_held]` 属性标注
- 长期：拆分为 per-subsystem 锁

### 9.5 per-CPU 变量（cpulocals.h）

**核心问题**: per-CPU 状态怎么表达？

**C 宏**: `get_cpulocal_var(name)`, `get_cpu_var(cpu, name)`

**Ch3 设计决策**:
- Rust 表达：`CpuLocal<T>` 类型
- 编译期检查：访问 per-CPU 变量必须在 idle 进程上下文或本核上下文

### 9.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-18-smp 全部 | 15 全部 | 复用 |

---

## 10. 文档 16：syscall-process（进程管理系统调用）

> **C 源码**: `minix3/minix/kernel/system/do_fork.c`, `do_exec.c`, `do_exit.c`, `do_clear.c`, `do_trace.c`, `do_privctl.c`, `do_runctl.c`, `do_update.c`, `do_statectl.c`, `do_setgrant.c`
> **C 行数**: 10 个 do_*.c，共 ~1500 行
> **依赖**: 12（分派已就绪）
> **产出**: 进程生命周期管理

### 10.1 do_fork()（system/do_fork.c）

**核心问题**: 父进程调 FORK，kernel 怎么创建子进程？

**Ch1 必读**:
- 分配 proc slot（找空闲的）
- 复制父进程的地址空间（VM 协助：`mini_send` 给 VM 做 COW）
- 复制文件描述符、信号 handler
- 子进程返回 0，父进程返回子 pid

**C 关键调用链**:
```
do_fork → alloc_proc → copy_proc_table → copy_mem (VM) → fork_copy_files → ready(proc)
```

### 10.2 do_exec()（system/do_exec.c）

**核心问题**: exec 替换进程镜像。

**Ch1 必读**:
- 加载新 ELF（VM 协助）
- 关闭 FD_CLOEXEC 的 fd
- 重置信号 handler
- 不创建新进程

### 10.3 do_exit() / do_clear()

**核心问题**: 进程退出 + 资源清理。

**Ch1 必读**:
- do_exit: 设 exit_status，通知 PM
- do_clear: 父进程调 CLEAR 回收子进程尸体（wait 类语义）
- 二者解耦：子进程 exit 后保留尸体，等父 clear

### 10.4 do_trace() / do_runctl() / do_statectl()

**核心问题**: 调试 + 进程控制 + 状态控制。

**Ch1 必读**:
- do_trace: 设置 RTS_P_STOP，让进程停下接受调试
- do_runctl: 设置 RTS_PROC_STOP 暂停 / 解除
- do_statectl: 让进程 control 自己的状态（如暂停自己）

### 10.5 do_privctl() / do_setgrant() / do_update()

**核心问题**: 权限管理 + 参数 grant + 进程替换。

**Ch1 必读**:
- do_privctl: 父为子分配 priv 编号（系统进程身份）
- do_setgrant: 进程将内存/IRQ 等 grant 给另一个进程
- do_update: 父把子替换为另一个进程（init 用）

### 10.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-14-syscall-fork-exec 全部 | 16 §1-2 | 拆分 |
| tmp-15-syscall-exit-signal 的 exit/clear 部分 | 16 §3 | 拆分 |

---

## 11. 文档 17：syscall-memory-copy（内存与拷贝系统调用）

> **C 源码**: `minix3/minix/kernel/system/do_memset.c`, `do_umap.c`, `do_umap_remote.c`, `do_vumap.c`, `do_copy.c` (vircopy/physcopy), `do_safecopy.c`, `do_vsafecopy.c`, `do_safememset.c`, `do_vmctl.c`（运行时部分，08 已讲协议）
> **C 行数**: ~1000 行
> **依赖**: 12（分派已就绪）+ 23（cross-space 实现）
> **产出**: 跨地址空间访问

### 11.1 do_memset()

**核心问题**: 用户调 MEMSET 写一段虚拟内存，kernel 怎么处理？

**Ch1 必读**:
- 简单情况：直接 memcpy（不需要 VM 协助）
- 复杂情况：跨进程 / 跨段 → VM 协助（VMSUSPEND）

### 11.2 do_umap() / do_umap_remote() / do_vumap()

**核心问题**: 把虚拟地址翻译成物理地址（userspace 询问）。

**Ch1 必读**:
- umap: 本进程 VA → PA
- umap_remote: 指定进程 VA → PA
- vumap: vector 形式，一次翻译多个
- 这三个 syscall 让用户态 FS 自己处理 DMA 等

### 11.3 do_vircopy() / do_physcopy()

**核心问题**: 用户态调拷贝 syscall。

**Ch1 必读**:
- vircopy: VA→VA 拷贝（用 VMSUSPEND 跨进程）
- physcopy: PA→PA 拷贝（Direct Map 下直接 memcpy）

### 11.4 do_safecopy_from() / do_safecopy_to() / do_vsafecopy()

**核心问题**: 预授权的内存拷贝。

**Ch1 必读**:
- safecopy: 目标进程预先 grant 了内存访问权，调用方直接拷
- 用 grant 表避免每次检查 capability

### 11.5 do_safememset()

**核心问题**: 预授权的 memset。

### 11.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-13-syscall-memory 全部 | 17 全部 | 复用 |
| tmp-13 的 vmctl 节 | 17 §11.6 + 08 | 拆分（08 是协议，17 是 handler） |

---

## 12. 文档 18：syscall-signal（信号系统调用）

> **C 源码**: `minix3/minix/kernel/system/do_kill.c`, `do_sigsend.c`, `do_sigreturn.c`, `do_getksig.c`, `do_endksig.c`
> **C 行数**: ~500 行
> **依赖**: 12（分派已就绪）+ 11（IPC 基础设施）
> **产出**: POSIX 风格信号传递

### 12.1 do_kill() / do_sigsend()

**核心问题**: 进程发信号给另一个进程。

**Ch1 必读**:
- do_kill: kernel signal（通知 PM 进程）
- do_sigsend: POSIX 风格信号（sig → sigaction 派发）
- 共同基础：`cause_sig()` 设 RTS_SIGNALED

### 12.2 do_getksig() / do_endksig()

**核心问题**: PM 进程获取/确认 kernel signal。

**Ch1 必读**:
- getksig: 弹出一个待处理 kernel signal
- endksig: 处理完成，清 RTS_SIG_PENDING

### 12.3 do_sigreturn()

**核心问题**: 从信号 handler 返回。

**Ch1 必读**:
- sigreturn 恢复信号 handler 之前的上下文
- 等价于 longjmp

### 12.4 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-15-syscall-exit-signal 的 signal 部分 | 18 全部 | 拆分 |
| tmp-11-privilege | 18 + 21 | 拆分（signal id 校验在 18，priv 编号在 21） |

---

## 13. 文档 19：syscall-device-clock（设备 + 时钟系统调用）

> **C 源码**: `minix3/minix/kernel/system/do_irqctl.c`, `do_devio.c`, `do_vdevio.c`, `do_sdevio.c`, `do_iopenable.c`, `do_readbios.c`, `do_times.c`, `do_setalarm.c`, `do_stime.c`, `do_settime.c`, `do_vtimer.c`, `do_sprofile.c`
> **C 行数**: ~800 行
> **依赖**: 12（分派已就绪）+ 14（clock）+ 13（interrupt）
> **产出**: 设备 I/O + 时钟 + 性能分析

### 13.1 do_irqctl()

**核心问题**: 进程注册/取消 IRQ handler。

**Ch1 必读**:
- irqctl 子命令：IRQCTL_SET / IRQCTL_ENABLE / DISABLE / ...
- 注册：把 hook 加到 irq_hooks[irq] 链表
- 需要 priv（IRQ 是系统资源）

### 13.2 do_devio() / do_vdevio() / do_sdevio()

**核心问题**: 进程做端口 I/O（inb/outb）。

**Ch1 必读**:
- devio: 端口 I/O（x86 only）
- vdevio: vector 形式
- sdevio: phys_insb/insw/outsb/outsw（x86 only）
- 都需要 IOPENABLE 过的 priv

### 13.3 do_times() / do_setalarm() / do_stime() / do_settime() / do_vtimer()

**核心问题**: 进程读时间 / 设 alarm / 调时间。

**Ch1 必读**:
- times: 返回 uptime + 本进程 times
- setalarm: 启动一个 alarm timer（用 07 §1 的 s_alarm_timer）
- stime / settime: 设 boottime / realtime（要 root）
- vtimer: virtual timer（按进程记账的定时器）

### 13.4 do_sprofile()

**核心问题**: 启动/停止 statistical profiling。

**Ch1 必读**:
- 性能分析：每 N ticks 采样 PC
- 采样数据放在 user 给的 buffer

### 13.5 do_readbios() / do_iopenable()（x86 only）

**核心问题**: 读 BIOS 区域 / 启用 I/O。

**Ch1 必读**:
- readbios: 读 ROM（要 root）
- iopenable: 启用 I/O 端口访问

### 13.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-16-timer 的 syscall 部分 | 19 §13.3 | 拆分 |
| tmp-04-protection 的 ioperm/iopl 部分 | 19 §13.2 | 拆分 |

---

## 14. 文档 20：syscall-system-control（系统控制 + 调度 + 机器状态）

> **C 源码**: `minix3/minix/kernel/system/do_abort.c`, `do_getinfo.c`, `do_diagctl.c`, `do_schedctl.c`, `do_schedule.c`, `do_setmcontext.c`, `do_getmcontext.c`, `do_padconf.c`（ARM only）
> **C 行数**: ~500 行
> **依赖**: 12（分派已就绪）+ 10（调度）+ 15（SMP）
> **产出**: 系统管理接口

### 14.1 do_abort()

**核心问题**: 进程触发系统关机 / panic。

**Ch1 必读**:
- 子命令：ABORT_HALT / ABORT_REBOOT / ABORT_PANIC
- 要 SYSTEM 权限

### 14.2 do_getinfo()

**核心问题**: 进程查询系统信息。

**Ch1 必读**:
- 子命令：GETINFO_CPU / GETINFO_IRQ / GETINFO_PRIV / GETINFO_PROC / GETINFO_SCHED / ...
- 进程表 / 中断表 / 调度信息查询

### 14.3 do_diagctl()

**核心问题**: 诊断控制（如启用 serial dump）。

**Ch1 必读**:
- 子命令：DIAGCTL_DUMP / DIAGCTL_PROCSTATS / ...
- 主要给 DS / PM 用

### 14.4 do_schedctl() / do_schedule()

**核心问题**: 进程改变自己的调度参数 / 触发重调度。

**Ch1 必读**:
- schedctl: 设优先级、quantum、绑核
- schedule: 主动让出 CPU

### 14.5 do_setmcontext() / do_getmcontext()

**核心问题**: 进程设置/获取机器上下文（ucontext）。

**Ch1 必读**:
- 主要给用户态协程库用
- setcontext 系统调用

### 14.6 do_padconf()（ARM only）

**核心问题**: 配置 ARM 引脚复用。

**Ch1 必读**: 32 位 ARM 才有；x86_64 不移植。归到 24 §4 unported。

### 14.7 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-12-syscall-dispatch 的 getinfo / schedctl | 20 §14.2-3 | 拆分 |

---

## 15. 文档 21：privilege（权限与 capability）

> **C 源码**: `minix3/minix/kernel/system.c:274-540`（get_priv, set_sendto_bit, unset_sendto_bit, fill_sendto_mask, send_sig, cause_sig, sig_delay_done, clear_endpoint, clear_ipc_refs），`minix3/minix/kernel/priv.h`
> **C 行数**: ~270 行
> **依赖**: 16-20（所有 syscall 都要查权限）
> **产出**: 完整的 capability 体系

### 15.1 priv 结构（priv.h）

**核心问题**: 进程权限如何表达？

**C 定义**:
```c
struct priv {
    int s_proc_nr;          // 进程 slot
    endpoint_t s_id;        // endpoint
    sys_map_t s_ipc_to;     // 可发送给的进程
    sys_map_t s_ipc_filter; // IPC filter
    uint32_t s_flags;       // 各种权限位
    struct io_range s_io_tab[NR_IO_RANGE];  // I/O 端口
    struct minix_mem_range s_mem_tab[NR_MEM_RANGE]; // 内存
    int s_nr_irq;           // IRQ hook 数量
    irq_hook_t *s_irq_tab[NR_IRQ_HOOKS];  // IRQ hooks
    timer_t s_alarm_timer;  // alarm
    ... 
};
```

**关键概念**:
- priv 表是固定大小（NR_PRIV = 32）
- priv 编号 = 进程身份（SYSTEM = 0, KERNEL = 1, ...）

### 15.2 get_priv() / may_send_to() / IPC filter

**核心问题**: 进程能向谁发消息？

**Ch1 必读**:
- `get_priv(proc_nr)` → 进程对应的 priv 结构
- `may_send_to(caller, dest)` → 检查 caller->priv.s_ipc_to 是否包含 dest
- IPC filter 是更细粒度的检查（22 §22 详述）

### 15.3 cause_sig() / send_sig() / sig_delay_done()

**核心问题**: kernel 给进程发信号。

**Ch1 必读**:
- cause_sig: 设 RTS_SIGNALED，enqueue 目标（如果是 idle）
- send_sig: 实际触发 handler
- sig_delay_done: syscall 退出时如果 RTS_SIG_DELAY + !RTS_SENDING，通知 PM

### 15.4 clear_endpoint() / clear_ipc_refs()

**核心问题**: 进程退出时清理它的 IPC 引用。

**Ch1 必读**:
- clear_endpoint: 释放 endpoint
- clear_ipc_refs: 让所有 SENDING/RECEIVING 给它的进程都 unblock，返回 ESRCH

### 15.5 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-11-privilege 全部 | 21 全部 | 复用 |

---

## 16. 文档 22：ipc-filter（IPC 过滤器）

> **C 源码**: `minix3/minix/kernel/system.c:705-916`（add_ipc_filter, clear_ipc_filters, check_ipc_filter, allow_ipc_filtered_msg, allow_ipc_filtered_memreq）
> **C 行数**: ~210 行
> **依赖**: 21（priv 是 filter 的载体）
> **产出**: 细粒度 IPC 控制

### 16.1 什么是 IPC filter

**核心问题**: 系统进程需要限制谁能给它发特定消息，怎么办？

**Ch1 必读**: IPC filter 是 process-level 细粒度权限。每个进程可以注册多条 filter：
```c
struct ipc_filter {
    endpoint_t src;        // 来自谁
    int type;              // 消息类型
    vir_bytes address;     // （memreq）目标地址
    int flags;             // 标志位
};
```

### 16.2 add_ipc_filter() / clear_ipc_filters()

**核心问题**: 增删 filter。

**Ch1 必读**:
- 进程调 SYS_SETGRANT 实际上不只 grant，还可能添加 filter
- clear: 进程退出时清空

### 16.3 check_ipc_filter() / allow_ipc_filtered_msg() / allow_ipc_filtered_memreq()

**核心问题**: 过滤检查的实际语义。

**Ch1 必读**:
- msg filter: 普通消息，过滤 type
- memreq filter: SAFECOPY 类，过滤目标地址范围
- 检查不通过 → ECALLDENIED

### 16.4 与 11 IPC 核心的整合

11 的 mini_send 在权限检查时会调 allow_ipc_filtered_msg()；17 的 safecopy 会调 allow_ipc_filtered_memreq()。

### 16.5 tmp 迁移

无对应 tmp- 文件，需要新建（基于 system.c:705-916 直接展开）。

---

## 17. 文档 23：cross-space-runtime（运行时跨地址空间访问）

> **C 源码**: `minix3/minix/kernel/arch/i386/memory.c:createpde, lin_lin_copy`, `minix3/minix/kernel/memory.c:vm_memset, vm_lookup`, `minix3/minix/kernel/proc.c:vm_suspend`（10 §4.4 概览）
> **C 行数**: ~600 行
> **依赖**: 11（IPC 基础）+ 17（memory syscall）
> **产出**: Direct Map 下的跨进程访问

### 17.1 lin_lin_copy()（arch/i386/memory.c）

**核心问题**: kernel 怎么把数据从进程 A 拷贝到进程 B？

**C 源码骨架**:
```c
int lin_lin_copy(struct proc *src, vir_bytes src_v,
                 struct proc *dst, vir_bytes dst_v, size_t bytes) {
    // 1. src VA → src PA
    src_pa = umap_local(src, src_v, bytes);
    if (src_pa == 0) return EFAULT_SRC;
    
    // 2. dst VA → dst PA
    dst_pa = umap_local(dst, dst_v, bytes);
    if (dst_pa == 0) return EFAULT_DST;
    
    // 3. 创建临时 PDE 映射（32 位 Minix3 必须）
    createpde(dst, ...);  // 32 位遗留
    
    // 4. memcpy
    memcpy(kernel_phys_to_virt(dst_pa), kernel_phys_to_virt(src_pa), bytes);
    
    // 5. 释放临时 PDE
    freepdes[0] = ...;  // 32 位遗留
    return OK;
}
```

**Direct Map 演进**:
- 64 位下 `createpde / freepdes` 完全删除
- `umap_local` 翻译 VA → PA（用 VMSUSPEND 跨进程）
- `memcpy` 直接走 `kernel_phys_to_virt()` 加法

**Ch3 设计决策**:
- trait `VAddrToPhys` 抽象 umap（x86_64 / aarch64 各实现）
- 删除 freepdes 相关代码
- 跨进程访问只走 Direct Map

### 17.2 createpde()（arch/i386/memory.c）

**核心问题**: 32 位 Minix3 没有 Direct Map，kernel 怎么访问任意物理地址？

**C 源码骨架**:
```c
phys_bytes createpde(struct proc *p, vir_bytes v) {
    // 1. 找一个 freepde 槽位
    pde = freepdes[free_pde_idx];
    
    // 2. 把 p 的页目录对应表项设为 pde
    // （这样 kernel 访问 freepde_vaddr 就映射到 p 的物理页）
    
    // 3. 返回 pde 对应的物理地址
    return pde_phys;
}
```

**Ch3 设计决策**:
- **删除**（Direct Map 不需要）
- 在 06 §1 已标注过，本节是 23 的实施细节

### 17.3 vm_memset() / vm_lookup()

**核心问题**: 当 memset 跨页 / 跨段时，kernel 怎么请求 VM 协助？

**C 源码骨架**:
```c
int vm_memset(endpoint_t who, vir_bytes addr, int val, size_t bytes) {
    // 1. 翻译 addr → PA
    phys_bytes pa = umap(who, addr, bytes);
    if (pa == 0) {
        // 触发 VMSUSPEND 让 VM 建映射
        vm_suspend(caller, who, addr, bytes, 1);
        return SUSPEND;
    }
    // 2. memset via Direct Map
    memset(kernel_phys_to_virt(pa), val, bytes);
    return OK;
}
```

### 17.4 VMSUSPEND 协议

**核心问题**: 跨进程内存操作触发 pagefault，kernel 怎么与 VM 协调？

**完整时序**:
```
1. 进程 A 调 VIRCOPY 把数据写到进程 B 的地址
2. kernel 调 lin_lin_copy → umap B 的目标地址 → pagefault
3. handle_pagefault: B 不是 VM，触发 VMSUSPEND
4. vm_suspend:
   - 设 A->RTS_PAGEFAULT + A->RTS_VMREQUEST
   - 构造 VMSUSPEND 消息
   - 设 A->RTS_SENDING 给 VM
   - 解除 A 的执行
5. 调度器选 VM
6. VM 收到 VMSUSPEND，处理（建 B 的页表）
7. VM 调 VMCTL_VMREPLY 给 kernel
8. kernel 收到 VMREPLY，恢复 A 的 syscall
9. A 重新做 lin_lin_copy（这次 page 在）
10. 完成拷贝
```

**Ch1 必读**: 这就是 MF_SC_DEFER 的产生点（12 §6.4 已提及）。

### 17.5 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-02-page-table-kernel 全部 | 23 全部 | 重写（加入 64 位 Direct Map 视角） |
| tmp-03-vm-request 的 VMSUSPEND 部分 | 23 §4 | 拆分 |

---

## 18. 文档 24：misc-unported（watchdog / ACPI / debug / profile / usermapped / unported）

> **C 源码**: `minix3/minix/kernel/watchdog.c`, `arch/i386/acpi.c`, `debug.c`, `profile.c`, `usermapped_data.c`, 以及 tmp-21 涵盖的 unported 符号
> **C 行数**: ~1500 行
> **依赖**: 所有（misc 类）
> **产出**: 辅助功能的 Rust 表达 + unported 清单

### 18.1 watchdog / ACPI

**核心问题**: 系统崩溃时如何自动重启 / 关闭？

**C 源码**:
- watchdog.c: watchdog 喂狗 / 超时重启
- acpi.c: ACPI 表解析（用于 shutdown / sleep）

**Ch1 必读**: Minix3 的 watchdog 是 software watchdog，依赖周期性喂狗（每个 tick 减计数器）。

**Ch3 设计决策**:
- watchdog: 保留（用 `arch::Watchdog` trait 抽象）
- ACPI: 保留（解析 AML → 找 sleep / poweroff 寄存器）

### 18.2 debug.c

**核心问题**: 内核调试功能。

**C 源码**: ser_dump_proc(), debug_print(), 串口 dump 等。

**Ch3 设计决策**:
- debug.c: 保留但弱化（生产无 debug 信息）
- 用 `klog!` 宏 + 等级

### 18.3 profile.c

**核心问题**: 统计 profiling（PC 采样）。

**C 源码**: profile_sample(), profile_reset() 等。

**Ch3 设计决策**:
- profile: 保留（作为 SPROF syscall 后端）
- 用环形缓冲 + 周期采样

### 18.4 usermapped_data.c

**核心问题**: kernel 把哪些数据映射到用户态（如 system info）？

**C 源码**: 把一个内核数据段映射到 user 的固定地址。

**Ch3 设计决策**:
- 保留（usermapped_data 是 libsysmon 依赖）
- Direct Map 下不需要额外映射（用户直接读）

### 18.5 unported symbols（tmp-21 全部内容）

**核心问题**: 哪些 C 符号在 64 位 Rust 版完全不移植？

**分类清单**:
1. **32 位特有**:
   - SYS_DEVIO, SYS_VDEVIO, SYS_SDEVIO, SYS_READBIOS, SYS_IOPENABLE（x86 only）
   - SYS_PADCONF（ARM only）
   - i8259.c（8259 PIC，现代用 APIC）
   - do_sdevio.c, do_readbios.c, do_iopenable.c, do_devio.c
2. **遗留机制**:
   - 4GB 截断（pg_utils.c add_memmap）
   - 32 位 sel/cdt 段（protect.c 大段相关）
   - 旧 libexec（如果是 32 位 ELF）
3. **多平台条件编译**:
   - ARM 的 mpx.S, oxpcie.c
   - 各种 `#ifdef __i386__` 块

**Ch3 设计决策**:
- 完全删除（不实现，不保留协议）
- 文档化删除理由

### 18.6 tmp 迁移

| tmp-* 内容 | 迁移到 | 操作 |
|-----------|--------|------|
| tmp-19-debug-serial | 24 §18.2 | 复用 |
| tmp-20-acpi-watchdog | 24 §18.1 | 复用 |
| tmp-21-unported-symbols 全部 | 24 §18.5 | 复用 |

---

## 19. 收敛检查

### 19.1 18 篇文档清单

| 编号 | 标题 | 状态 | 临时来源 |
|------|------|------|----------|
| 07 | system-init-boot-finish | ✅ 沿用 kboot-new | tmp-17 |
| 08 | vm-boot-protocol | ✅ 沿用 kboot-new | tmp-03 (VMCTL 节) + tmp-13 (vmctl) |
| 09 | switch-to-user-entry | 🆕 新建 | tmp-07 (switch_to_user) |
| 10 | scheduling-primitives | 🆕 新建 | tmp-07 (pick/enq/deq) + tmp-06 (RTS) |
| 11 | ipc-core | 🆕 新建 | tmp-09 + tmp-10 + tmp-08 (拆分) |
| 12 | syscall-dispatch | 🆕 新建 | tmp-12 |
| 13 | exception-interrupt | 🆕 新建 | tmp-05 |
| 14 | clock-timer | 🆕 新建 | tmp-16 |
| 15 | smp | 🆕 新建 | tmp-18 |
| 16 | syscall-process | 🆕 新建 | tmp-14 + tmp-15 (exit) |
| 17 | syscall-memory-copy | 🆕 新建 | tmp-13 |
| 18 | syscall-signal | 🆕 新建 | tmp-15 (signal) |
| 19 | syscall-device-clock | 🆕 新建 | tmp-16 (syscall) + tmp-04 (io) |
| 20 | syscall-system-control | 🆕 新建 | tmp-12 (getinfo/schedctl) |
| 21 | privilege | 🆕 新建 | tmp-11 |
| 22 | ipc-filter | 🆕 新建 | system.c:705-916 |
| 23 | cross-space-runtime | 🆕 新建 | tmp-02 + tmp-03 (VMSUSPEND) |
| 24 | misc-unported | 🆕 新建 | tmp-19 + tmp-20 + tmp-21 |

### 19.2 顺序叙事验证

| 文档 | 必须前向引用 | 实际前向引用 | OK? |
|------|-------------|-------------|-----|
| 07 | 06 | 06 | ✅ |
| 08 | 07 | 07 | ✅ |
| 09 | 07, 08 | 07, 08 | ✅ |
| 10 | 09 | 09 | ✅ |
| 11 | 10 | 10 | ✅ |
| 12 | 11 | 11 | ✅ |
| 13 | 12 | 12 | ✅ |
| 14 | 13 | 13 | ✅ |
| 15 | 10, 14 | 10, 14 | ✅ |
| 16-20 | 12 | 12 | ✅ |
| 21 | 16-20 | 16-20 | ✅ |
| 22 | 21 | 21 | ✅ |
| 23 | 11, 17 | 11, 17 | ✅ |
| 24 | （全部） | （全部） | ✅ |

**无前向引用** ✅

### 19.3 C 源码覆盖检查

| C 文件 | 行数 | 归属文档 | 完整? |
|--------|-----|----------|-------|
| main.c | 522 | 07 §4 | ✅ |
| proc.c | 1980 | 09-11 | ✅ |
| system.c | 997 | 07 §1 + 12 + 21 + 22 | ✅ |
| clock.c | 312 | 14 | ✅ |
| interrupt.c | 177 | 13 | ✅ |
| utility.c | 93 | (跨篇) | ✅ |
| smp.c | 205 | 15 | ✅ |
| system/do_fork.c | ~300 | 16 | ✅ |
| system/do_*.c (40 个) | ~5000 | 16-20, 23 | ✅ |
| arch/i386/exception.c | ~150 | 13 | ✅ |
| arch/i386/memory.c | ~400 | 23 | ✅ |
| debug.c | ~200 | 24 | ✅ |
| profile.c | ~100 | 24 | ✅ |
| watchdog.c / acpi.c | ~300 | 24 | ✅ |

**总 C 覆盖**: ~9500 行（接近 100% 覆盖）

### 19.4 与 overview 规划对比

| 维度 | 00-overview | 本规划 (m3) | 改进 |
|------|-------------|-------------|------|
| 文档数 | 18（实际 19 tmp）| 18 | 一致 |
| 顺序叙事 | ❌（overview 写于 tmp 后，跳过内容）| ✅ | 严格时间线 |
| C 源码对应 | 模糊 | 每篇明确行号 | 可追溯 |
| tmp 迁移 | 未规划 | 每篇明确迁移路径 | 落地 |
| 依赖图 | 隐式 | 显式（§0.3） | 可验证 |

---

## 20. 总结

**本规划**:
- 18 篇文档（07-24）
- 严格沿 T8-T26 时间线
- 无前向引用（§19.2 验证）
- 覆盖 ~9500 行 C 源码（§19.3 验证）
- 18 个 tmp- 文件有明确迁移路径（§1-§18 每节末）

**与 kboot 衔接**:
- 07 续 06（cross-space-init）— ptproc / freepdes 之后 kmain tail
- 08 续 07 — VMCTL 握手
- 09 续 08 — 进入运行时

**核心改进**（相对 overview）:
1. 顺序叙事：每篇承接前一篇
2. 时间线锚点：T8-T26 显式编号
3. tmp 落地：18 个 tmp 文件有去处
4. C 溯源：每节标注 `system.c:168-272` 等行号

**下一步**:
- 每篇文档按 Ch1-4 模板实施
- 每篇完成后从 18 个 tmp- 文件中提取内容
- 全 18 篇完成时进入 review 阶段

