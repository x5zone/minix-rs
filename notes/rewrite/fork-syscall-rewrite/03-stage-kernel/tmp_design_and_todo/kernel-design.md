# kernel-design: Minix-RS Kernel 完整设计文档

> **分类**: Kernel 全局设计
> **源码**: `minix3/minix/kernel/` (全部 C 源文件)
> **说明**: 合并 kboot-*.md 和 runtime-design-*.md 的最终版 kernel 设计，覆盖从 boot-shim 到运行时全部逻辑
> **创建**: 2026-06-13
> **方法**: 综合 5 份 AI 设计方案的优点，形成最佳实践（详见 §0.2 方案综合分析）
> **原则**:
>   1. **顺序叙事**: 每篇文档承接上一篇，前向零引用
>   2. **时间线优先**: 文档编号 = CPU 实际执行顺序
>   3. **C 行为是 Ground Truth**: 任何 Rust 设计必须能溯源到 C 行为
>   4. **Direct Map 演进在 Ch3 标注**: 不消除 C 行为记录

---

## 0. 两个世界：C 的 32 位 vs Rust 的 64 位 + Direct Map

| 方面 | Minix3 C（32 位 x86） | minix-rs（64 位 + Direct Map） |
|------|----------------------|-------------------------------|
| 地址空间 | 4GB 虚拟地址空间 | 48 位（256TB）虚拟地址空间 |
| 内核访问物理内存 | 无直接映射，需要 `createpde` 临时窗口 | `kernel_phys_to_virt(pa) = KERNEL_DIRECT_MAP_BASE + pa` |
| 跨进程内存拷贝 | `lin_lin_copy` → `freepdes` 临时 PDE 映射 | 翻译 VA→PA → `kernel_phys_to_virt()` → 直接 memcpy |
| 访问进程页目录 | `pagedir_mappings` 登记册 + `p_cr3_v` 窗口 | `kernel_phys_to_virt(cr3_phys)` 一行加法 |
| 页表层级 | 2 级（PD+PT） | 4 级（PML4+PDPT+PD+PT） |
| 页表项大小 | 4 字节 PDE | 8 字节 PDE |
| 大页粒度 | 4MB（PDE 映射） | 1GB（PDPT 映射）或 2MB（PD 映射） |

**Direct Map 的核心影响**：64 位地址空间足够大，可以把全部物理内存线性映射到内核虚拟地址空间的高位。`kernel_phys_to_virt()` 就是一行加法——Minix3 C 的 `createpde`/`freepdes`/`pagedir_mappings` 全部被消除。

**本文档立场**：Ch1-2 必须忠实记录 Minix3 C 的行为（Ground Truth），Ch3-4 可以进行 Direct Map 演进设计。

---

## 0.2 五方案综合分析：各取所长

五份独立 AI 设计方案（m3/kimi/ds/qwen/glm）从不同视角切入同一问题，每份都有独特洞察。本规划**不是选一个基线**，而是综合各方案的最佳实践。

### 五方案独特贡献

| 方案 | 核心视角 | 独特贡献 | 本规划采纳点 |
|------|---------|---------|-------------|
| **m3** | CPU 执行时间线 | 严格 T8-T26 时序点、C 源码行号逐行追踪、tmp 迁移映射表 | §1 时间线骨架 + §3 每篇文档的 C 源码坐标 |
| **kimi** | 三层架构 + 叙事诊断 | "先状态→再调度→再通信→再事件→再服务"的认知顺序；RTS 标志"谁设置/谁清除"列；tmp 文件问题诊断（前向引用/概念碎片化/机制与服务混杂） | §5 三层架构图 + §3.10 RTS 表"谁设置/谁清除"列 + §9 叙事修正策略 |
| **ds** | 事件驱动状态机 | "内核运行时是事件驱动的状态机"元视角；proc.c 函数完整清单表；VMSUSPEND 四层分解（状态定义→状态转换→事件源→服务） | §5 补充"状态机视角" + §3.11 IPC 函数清单 + §3.23 VMSUSPEND 四层分解 |
| **qwen** | 三种激活路径 | "内核没有主循环，只有三种被激活的方式"——系统调用/中断/异常三条入口路径的汇聚点分析；switch_to_user() 状态机伪代码最精确 | §3.09 switch_to_user 控制流伪代码 + §3.13 "三种激活路径"模型 |
| **glm** | 依赖关系图 | 运行时子系统依赖关系 DAG 最清晰；"三个并发循环"模型（调度循环/IPC 循环/中断循环） | §2.2 依赖图 + §5 补充"三个循环"视角 |

### 综合决策原则

| 问题 | m3 | kimi | ds | qwen | glm | **综合最佳实践** |
|------|-----|------|-----|------|-----|-----------------|
| 文档组织轴 | 时间线 | 认知顺序 | 状态机层次 | 激活路径 | 依赖图 | **时间线为主轴，认知顺序为辅轴**（时间线保证不遗漏，认知顺序保证可读性） |
| 叙事起点 | T8 续起 | 先诊断 tmp 问题 | 先定义状态机 | 先讲激活模型 | 先画依赖图 | **先诊断→再定义→再时序**（§0.3 问题诊断 + §5 架构定义 + §1 时序追踪） |
| switch_to_user | 调度入口 | 调度决策点 | 状态转换枢纽 | 三路径汇聚点 | 调度循环核心 | **三路径汇聚的调度决策点**（综合 qwen 的入口分析 + kimi 的决策点概念） |
| RTS 标志表 | 有值+含义 | 有值+含义+谁设置/清除 | 有值+含义 | 简略 | 简略 | **值+含义+谁设置/谁清除**（采纳 kimi 的完整语义） |
| IPC 叙事 | 按原语分节 | 按阻塞/唤醒分节 | 按状态转换分节 | 按入口路径分节 | 按依赖分节 | **按原语分节+阻塞/唤醒语义标注**（m3 结构 + kimi 语义） |
| VMSUSPEND | 协议描述 | 协议+类型设计 | 四层分解 | 入口分析 | 依赖标注 | **四层分解+类型设计**（ds 分解 + kimi 类型） |
| 文档编号 | 07-24 | 08-21 | 08-20 | 07-21 | 08-18 | **07-24**（m3 最完整，覆盖 watchdog/profile/usermapped） |

### 为什么不是"选一个"

1. **m3 的时间线是必要条件但不是充分条件**：时间线保证不遗漏任何 C 函数，但它不解释"为什么调度在 IPC 之前"——这需要 kimi 的认知顺序分析
2. **kimi 的三层架构是理解框架但不是实现指南**：三层架构帮助读者建立心智模型，但具体到每个函数在哪个时序点执行，需要 m3 的时间线
3. **ds 的状态机视角是验证手段**：状态机视角可以验证"是否有遗漏的状态转换"，但它不提供叙事顺序
4. **qwen 的激活路径是入口分析**：三种激活路径帮助理解"内核代码如何被触发"，但它不覆盖 boot 阶段
5. **glm 的依赖图是约束验证**：依赖图确保"文档 N 不依赖文档 N+X"，但它不提供文档内容

**结论**：五方案互补而非互斥。本规划以 m3 时间线为骨架（保证覆盖），以 kimi 三层架构为理解框架（保证可读），以 ds 状态机为验证手段（保证完备），以 qwen 激活路径为入口分析（保证因果），以 glm 依赖图为约束（保证无前向引用）。

---

## 0.3 第一版规划（tmp_*）的问题诊断

> 本节综合 kimi 的问题诊断 + 各方案的共识

tmp 文件的核心问题不是"内容有错"，而是"组织方式对读者不友好"：

| 问题类型 | 例子 | 后果 | 根因 |
|---------|------|------|------|
| **前向引用** | tmp-10 说"MF_KCALL_RESUME 详见 tmp-12" | 读者读到第 10 篇时，12 还没读 | 按主题切割，不顾阅读顺序 |
| **概念碎片化** | `struct proc` 字段分散在 tmp-06/08/09/10 | 读完整套才能拼出全貌 | 没有在首次出现时完整解释 |
| **机制与服务混杂** | VMCTL 既是 boot 协议又是运行时调用 | 同一段代码在两个上下文重复解释 | 没有区分"机制"和"使用机制的服务" |
| **叙事起点不统一** | tmp-04 从"保护模式是什么"开始，tmp-07 从"调度队列是什么"开始 | 读者反复切换心智模型 | 每篇独立建立上下文 |
| **因果关系缺失** | 调度和 IPC 之间缺少"为什么"的过渡 | 读者不知道"为什么先讲调度再讲 IPC" | 按主题并列，不顾因果链 |

**本规划的修正策略**（综合五方案共识）：

1. **时间线为主轴**：文档编号 = CPU 执行顺序（m3）
2. **认知顺序为辅轴**：先状态→再调度→再通信→再事件→再服务（kimi）
3. **概念首次出现即完整解释**：禁止前向引用（kimi）
4. **一本账原则**：每个 C 函数只属于一篇文档（kimi/ds）
5. **三层架构为理解框架**：基础设施→状态转换→事件入口（kimi/ds）
6. **三种激活路径为入口分析**：系统调用/中断/异常（qwen）
7. **依赖图为约束验证**：文档 N 只依赖 <N（glm）

---

## 1. 完整时间线：T0-T26

### 1.1 Boot 阶段（T0-T7）

| 时序点 | C 函数 | C 源码 | 核心动作 | 文档 |
|--------|--------|--------|---------|------|
| T0 | boot-shim | — | UEFI → 加载 kernel ELF + boot modules → ExitBootServices | 01 |
| T1 | HigherHalf | head.S | 切栈跳高地址、trampoline | 02 |
| T2 | kmain 入口 | main.c:115-147 | memcpy(&kinfo)、BSS 检查、kernel_may_alloc=1、cstart() → prot_init | 03 |
| T3 | cstart 後半 | main.c:403-481 | init_clock()、intr_init()、arch_init() | 04 |
| T4 | BKL + 进程表 | main.c:149-163 | BKL_LOCK()、proc_init()、IPCF_POOL_INIT() | 05 |
| T5 | boot 进程加载 | main.c:157-282 | 特权分配 → arch_boot_proc → VM ELF 解析+页表映射 → VMINHIBIT | 05 |
| T6 | post-init | main.c:283-290 | arch_post_init()（ptproc=VM, pg_info） | 06 |
| T7 | freepdes | main.c:293 | memory_init()（分配 2 个 freepde 临时映射槽位） | 06 |

### 1.2 Boot 完成 + VM 启动（T8-T12.9）

| 时序点 | C 函数 | C 源码 | 核心动作 | 文档 |
|--------|--------|--------|---------|------|
| T8 | system_init() | system.c:168-272 | 注册 50+ 个 system call handler 到 call_vec[] | 07 |
| T9 | add_memmap() | pg_utils.c:86-125 | 回收 bootstrap 内存 | 07 |
| T9.5 | smp_init() | smp.c | 启动 APs 或单核回退 | 07 |
| T10 | bsp_finish_booting() | main.c:38-113 | cpu_identify、announce、RTS_PROC_STOP 解除、timer、fpu、switch_to_user | 07 |
| T11 | VM 第一次被调度 | (02-stage-vm/26) | VM 调 init_page_table → map_kernel 建 direct map | 08 |
| T12.1 | VMCTL_SETADDRSPACE | do_vmctl.c | 切换 CR3 到 VM 的真实页表 | 08 |
| T12.3 | VMCTL_KERN_PHYSMAP | do_vmctl.c | 内核声明需映射的物理区 | 08 |
| T12.5 | VMCTL_KERN_MAP_REPLY | do_vmctl.c | VM 返回虚拟地址 | 08 |
| T12.7 | VMCTL_VMINHIBIT_CLEAR | do_vmctl.c | 解除所有进程的 VMINHIBIT | 08 |
| T12.9 | vm_running = 1 | kernel 全局变量 | runtime 真正就绪 | 08 |

### 1.3 运行时核心（T13-T16）

| 时序点 | C 函数 | C 源码 | 核心动作 | 文档 |
|--------|--------|--------|---------|------|
| T13 | switch_to_user() | proc.c:299-450 | idle → pick_proc → 切地址空间 → iret | 09 |
| T13.1 | pick_proc() | proc.c:1785-1810 | 16 优先级队列遍历 | 10 |
| T13.2 | enqueue()/dequeue() | proc.c:1595/1716 | 队列插入/删除 | 10 |
| T13.3 | RTS_* 状态机 | proc.h:141-228 | runnable iff p_rts_flags == 0 | 10 |
| T14 | 时钟中断 | clock.c:70 | 100Hz tick → RTS_NO_QUANTUM / RTS_PREEMPTED | 14 |
| T15 | 系统调用 trap | arch_system.c | sys_call 入口 → kernel_call | 12 |
| T15.1 | kernel_call() | system.c:136-162 | copy_msg_from_user → kernel_call_dispatch | 12 |
| T15.2 | kernel_call_dispatch | system.c:95-128 | call_vec[syscall_num] 分派 | 12 |
| T16 | mini_send / mini_receive | proc.c:870/1080 | 同步 IPC 阻塞语义 | 11 |
| T16.1 | mini_notify | proc.c:1122 | 异步单边通知 | 11 |
| T16.2 | try_deliver_senda / SENDA | proc.c:1200 | 异步批量发送 | 11 |
| T16.3 | delivermsg | proc.c:263 | 拷贝 m_source → 用户栈 | 11 |

### 1.4 运行时服务（T17-T26）

| 时序点 | C 函数 | C 源码 | 核心动作 | 文档 |
|--------|--------|--------|---------|------|
| T17 | do_fork/exec/exit | system/do_fork.c 等 | 进程生命周期 | 16 |
| T18 | do_memset/umap/vircopy/safecopy | system/do_memset.c 等 | 内存系统调用 | 17 |
| T18.5 | lin_lin_copy | arch/i386/memory.c | 跨地址空间 memcpy | 23 |
| T18.6 | createpde | arch/i386/memory.c | 临时 PDE 映射 | 23 |
| T18.7 | vm_memset/vm_lookup | memory.c | VM 协助的 memset/查询 | 23 |
| T19 | VMSUSPEND 协议 | memory.c | 进程缺页 → VM → resume | 23 |
| T20 | SMP 时钟中断 | smp.c:156 | IPI SCHED → AP 调度 | 15 |
| T21 | exception (page fault) | arch/i386/exception.c | pagefault handler | 13 |
| T22 | ipc_filter | system.c:705-916 | IPC 过滤器 | 22 |
| T23 | privilege (priv) | system.c:274-540, priv.h | 权限位操作 | 21 |
| T24 | watchdog / ACPI | watchdog.c, acpi.c | 监视器 | 24 |
| T25 | debug/profile/usermapped | debug.c, profile.c, usermapped_data.c | 调试 | 24 |
| T26 | unported symbols | (legacy/x86-only) | 64 位/Rust 不移植 | 24 |

---

## 2. 文档编号与依赖图

### 2.1 完整文档序列（01-24）

| 编号 | 文件名 | 核心问题 | C 源码核心 | 状态 |
|------|--------|---------|-----------|------|
| 01 | 01-boot-shim-bootstrap.md | UEFI 如何加载内核？ | pre_init.c | ✅ 已实现 |
| 02 | 02-higher-half-kernel.md | 内核如何跳到高地址？ | head.S, kernel.lds | ✅ 已实现 |
| 03 | 03-kmain-cstart.md | kmain 入口做了什么？ | main.c:115-143, protect.c | ✅ 已实现 |
| 04 | 04-clock-interrupt-init.md | 时钟和中断如何初始化？ | clock.c:48-74, i8259.c | ✅ 已实现 |
| 05 | 05-proc-init-boot-proc.md | 进程表和 boot 进程如何初始化？ | proc.c:119-160, main.c:157-282 | ✅ 已实现 |
| 06 | 06-cross-space-init.md | 跨地址空间基础设施？ | protect.c:370-377, memory.c:707-717 | ✅ 已实现 |
| **07** | **07-system-init-boot-finish.md** | **系统调用注册+启动完成？** | **system.c:168-272, main.c:38-113** | ❌ 需新建 |
| **08** | **08-vm-boot-protocol.md** | **VM 启动后的内核-VM 协商？** | **do_vmctl.c + arch_do_vmctl.c** | ❌ 需新建 |
| **09** | **09-switch-to-user.md** | **调度循环如何启动？** | **proc.c:299-450** | ❌ 需新建 |
| **10** | **10-scheduling-primitives.md** | **调度原语+进程状态机？** | **proc.c:1595-1870, proc.h:141-274** | ❌ 需新建 |
| **11** | **11-ipc-core.md** | **IPC 核心机制？** | **proc.c:599-1590** | ❌ 需新建 |
| **12** | **12-syscall-dispatch.md** | **系统调用如何分派？** | **system.c:52-167** | ❌ 需新建 |
| **13** | **13-exception-interrupt.md** | **异常和中断如何处理？** | **exception.c, interrupt.c, clock.c** | ❌ 需新建 |
| **14** | **14-clock-timer.md** | **时钟中断和定时器？** | **clock.c:70-199** | ❌ 需新建 |
| **15** | **15-smp.md** | **多核如何协同？** | **smp.c, apic.c** | ❌ 需新建 |
| **16** | **16-syscall-process.md** | **进程管理调用？** | **do_fork/exec/clear/exit/privctl/runctl/update/statectl** | ❌ 需新建 |
| **17** | **17-syscall-copy.md** | **跨进程内存拷贝？** | **do_safecopy/umap/vircopy/copy** | ❌ 需新建 |
| **18** | **18-syscall-signal.md** | **信号系统？** | **do_kill/getksig/endksig/sigsend/sigreturn** | ❌ 需新建 |
| **19** | **19-syscall-device.md** | **设备 I/O 调用？** | **do_irqctl/devio/vdevio/sdevio** | ❌ 需新建 |
| **20** | **20-syscall-clock.md** | **时钟调用？** | **do_times/setalarm/stime/vtimer** | ❌ 需新建 |
| **21** | **21-privilege.md** | **权限管理？** | **system.c:274-540, priv.h** | ❌ 需新建 |
| **22** | **22-ipc-filter.md** | **IPC 过滤器？** | **system.c:705-916** | ❌ 需新建 |
| **23** | **23-cross-space-runtime.md** | **运行时跨地址空间？** | **memory.c, arch/i386/memory.c** | ❌ 需新建 |
| **24** | **24-misc-unported.md** | **杂项+不移植？** | **watchdog.c, debug.c, profile.c** | ❌ 需新建 |

### 2.2 依赖图（仅向后引用）

```
01 ─→ 02 ─→ 03 ─→ 04 ─→ 05 ─→ 06 ─→ 07 ─→ 08
                                            │
                                            ▼
                                          09 ─→ 10 ─→ 11 ─→ 12
                                            │      │      │
                                            │      │      └──→ 16-20 (syscall 服务)
                                            │      │
                                            ▼      ▼
                                          13 ─→ 14 ─→ 15
                                            │
                                            ▼
                                          21-22 ─→ 23 ─→ 24
```

---

## 3. 各文档详细规划

---

### 07-system-init-boot-finish: 系统调用初始化与启动完成

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/system.c:168-278`, `minix3/minix/kernel/arch/i386/pg_utils.c:86-125`, `minix3/minix/kernel/main.c:38-117`
> **前置**: 06（ptproc 已设置、freepdes 已分配）
> **C 总行数**: ~230 行

#### Ch1: 概念

**核心问题**: kmain 的最后三步——系统调用注册、bootstrap 回收、启动完成——如何把内核从"初始化态"带入"运行态"？

**三个子阶段**：

1. **system_init()（T8）**：注册 50+ 个 system call handler 到 `call_vec[]`
   - `call_vec[NR_SYS_CALLS]` 是 system call 派发表（数组下标 = syscall 号）
   - `map(call, fn)` 是宏：检查范围 + 写 `call_vec[call] = fn`
   - 中断 hook 池（NR_IRQ_HOOKS = 64）初始化为 NONE
   - 每个 priv 结构有 s_alarm_timer，定时器必须先 init

2. **add_memmap(bootstrap)（T9）**：bootstrap 阶段临时占用的内存归还给系统
   - `kinfo.memmap[]` 是物理内存块的 BIOS/UEFI 报告表
   - `kernel_may_alloc` 是 kmain 的"允许 VM alloc"开关，T9 后必须关闭
   - 32 位 4GB 截断是 Minix3 32 位地址空间限制的遗留

3. **bsp_finish_booting()（T10）**：BSP 进入调度循环前的最后准备
   - `vm_running = 0`：所有 do_vmctl 子命令通过此检查
   - `RTS_PROC_STOP` 解除：boot 进程变为可调度
   - `boot_cpu_init_timer(100)`：启动 100Hz 时钟
   - `switch_to_user()`：永不返回，进入调度循环

**system_init 注册的完整 syscall 列表**：

| 类别 | 系统调用 | 处理函数 |
|------|---------|---------|
| 进程管理 | SYS_FORK, SYS_EXEC, SYS_CLEAR, SYS_EXIT, SYS_PRIVCTL, SYS_TRACE, SYS_SETGRANT, SYS_RUNCTL, SYS_UPDATE, SYS_STATECTL | do_fork, do_exec, do_clear, do_exit, do_privctl, do_trace, do_setgrant, do_runctl, do_update, do_statectl |
| 信号 | SYS_KILL, SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN | do_kill, do_getksig, do_endksig, do_sigsend, do_sigreturn |
| 设备 I/O | SYS_IRQCTL, SYS_DEVIO(x86), SYS_VDEVIO, SYS_SDEVIO(x86), SYS_IOPENABLE(x86), SYS_READBIOS(x86) | do_irqctl, do_devio, do_vdevio, do_sdevio, do_iopenable, do_readbios |
| 内存 | SYS_MEMSET, SYS_VMCTL | do_memset, do_vmctl |
| 拷贝 | SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_VIRCOPY, SYS_PHYSCOPY, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY, SYS_SAFEMEMSET | do_umap, do_umap_remote, do_vumap, do_vircopy, do_copy, do_safecopy_from, do_safecopy_to, do_vsafecopy, do_safememset |
| 时钟 | SYS_TIMES, SYS_SETALARM, SYS_STIME, SYS_SETTIME, SYS_VTIMER | do_times, do_setalarm, do_stime, do_settime, do_vtimer |
| 系统控制 | SYS_ABORT, SYS_GETINFO, SYS_DIAGCTL | do_abort, do_getinfo, do_diagctl |
| 性能 | SYS_SPROF | do_sprofile |
| 调度 | SYS_SCHEDULE, SYS_SCHEDCTL | do_schedule, do_schedctl |
| 机器状态 | SYS_SETMCONTEXT, SYS_GETMCONTEXT | do_setmcontext, do_getmcontext |
| ARM | SYS_PADCONF | (ARM only) |

#### Ch2: C 源码分析

- `system.c:168-278` — system_init()：IRQ hook 初始化 + alarm timer 初始化 + call_vec 注册
- `pg_utils.c:86-125` — add_memmap()：bootstrap 内存回收 + 4GB 截断
- `main.c:38-113` — bsp_finish_booting()：cpu_identify → vm_running=0 → announce → RTS_PROC_STOP 解除 → timer → fpu → kernel_may_alloc=0 → switch_to_user()

#### Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| call_vec 表达 | `[Option<fn>; NR_SYS_CALLS]` vs `enum Syscall + match` | **match** | 类型安全 + 编译期穷尽检查 |
| IRQ hook 池 | `Vec<Option<IrqHook>>` vs `BTreeMap<irq, hook>` | **Vec<Option>** | 保持 C 池语义，O(1) 索引 |
| Alarm timer | 每个 priv 一个 timer struct | **保持** | per-priv 而非全局 |
| map() 宏 | 保留 vs 静态断言 | **保留** 为 `const _: () = { ... }` | 编译期检查 |
| add_memmap 4GB 截断 | 保留 vs 删除 | **删除** | 64 位不需要 |
| vm_running 表达 | `AtomicBool` vs `CpuLocal<bool>` | **CpuLocal** | 每核独立 |
| switch_to_user | 普通函数 vs 发散函数 | **`-> !`** | 类型系统表达永不返回 |

#### Ch4: 实现要点

- `enum Syscall { Fork, Exec, ... }` 派生 `From<u16> + TryFrom<u16>`
- `pub fn bsp_finish_booting() -> !` 发散函数
- 编译期断言：所有 `map()` 项已覆盖

#### 测试

- 单元：每个 syscall 号可正确派发到 handler
- 静态：`map(SYS_FORK, do_fork)` 编译期检查

---

### 08-vm-boot-protocol: VM 启动后的内核-VM 协商协议

> **分类**: Kernel IPC 协议
> **源码**: `minix3/minix/kernel/system/do_vmctl.c`, `minix3/minix/kernel/arch/i386/do_vmctl.c`
> **前置**: 07（bsp_finish_booting 完成，VM 已开始运行）
> **C 总行数**: ~230 行

#### Ch1: 概念

**核心问题**: VM 是页表的所有者，但它刚启动时只有 kernel 给的 bootstrap 页表。内核和 VM 之间如何协商，让系统进入"运行时可用"状态？

**时序**：

```
1. switch_to_user() → pick_proc() → 选 VM（唯一无 VMINHIBIT 的进程）
2. VM 用户态代码执行 init_page_table() → map_kernel()
   → 建立 kernel direct map（KERNEL_DIRECT_MAP_BASE, U/S=0, G=1）
3. VM → SYS_VMCTL(VMCTL_SETADDRSPACE): 切换 CR3 到 VM 的真实页表
4. VM → SYS_VMCTL(VMCTL_KERN_PHYSMAP): 内核声明需映射的物理区
5. VM → SYS_VMCTL(VMCTL_KERN_MAP_REPLY): VM 返回虚拟地址
6. VM 为 PM/VFS/RS 等创建页表
7. VM → SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR): 解除所有进程的 VMINHIBIT
8. vm_running = 1
```

**双视图地址空间模型**：

| 属性 | VM direct map | Kernel direct map |
|------|--------------|-------------------|
| 虚拟地址基址 | `0x0000_0000_8000_0000` | `0xFFFF_8000_0000_0000` |
| U/S 位 | 1（用户态可访问） | 0（仅内核态可访问） |
| G 位 | 0 | 1（CR3 切换不刷新 TLB） |
| 建立者 | Kernel（arch_boot_proc） | VM（`map_kernel()`） |
| 建立时机 | VM 启动前（T5） | VM 启动后（T11） |

**关键洞察**：这不是"两份映射"，而是"同一物理内存在不同特权级下的两个必要窗口"。x86-64 的 U/S 位不可能同时为 0 和 1，因此两个窗口是硬件的必然要求。

#### Ch2: C 源码分析

- `do_vmctl.c` — VMCTL_SETADDRSPACE / VMCTL_KERN_PHYSMAP / VMCTL_KERN_MAP_REPLY / VMCTL_VMINHIBIT_CLEAR
- `arch/i386/do_vmctl.c` — switch_address_space / arch_phys_map / arch_phys_map_reply

#### Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_address_space | 直接写 CR3 vs trait 抽象 | **trait PageTableSwitcher** | 多架构支持 |
| KERN_PHYSMAP / KERN_MAP_REPLY | 保留 vs 删除 | **保留协议，实现为 noop** | 64 位 direct map 已就绪，但协议层保留向后兼容 |
| VMINHIBIT_CLEAR | 逐进程 vs 批量 | **批量** | 保持 C 语义 |
| vm_running 置位时机 | SETADDRSPACE 后 vs VMINHIBIT_CLEAR 后 | **SETADDRSPACE 后** | 与 C 一致 |

#### Ch4: 实现要点

- `trait PageTableSwitcher { fn switch_to(&self, cr3_phys: u64); }`
- x86_64 实现：`write_cr3(cr3_phys)` + TLB 刷新
- KERN_PHYSMAP/KERN_MAP_REPLY 的 64 位实现为 noop，注释标注"32 位遗留"

---

### 09-switch-to-user: 进入调度循环

> **源码**: `minix3/minix/kernel/proc.c:299-450`
> **前置**: 08（vm_running=1、所有进程可调度）
> **C 行数**: ~150 行

#### Ch1: 概念

**核心问题**: switch_to_user 之后，CPU 怎么知道下一步该跑哪个进程？

`switch_to_user()` 是内核的"调度决策点"（kimi）——每次从用户态陷入内核，处理完毕后，都要经过此函数决定下一个运行的进程。它是三种激活路径（qwen）的共同终点，也是三个并发循环（glm）的汇聚点。

**控制流**（综合 qwen 的精确伪代码 + kimi 的决策点分析 + ds 的状态机视角）：

```
switch_to_user():
  p = proc_ptr

check_runnable:
  if p.rts_flags != 0:           // 进程不可运行
    if p has RTS_PREEMPTED:      // 被抢占 → 重新入队
      RTS_UNSET(PREEMPTED)
      enqueue(p)                 // 放回队列尾部
    goto pick_new

  // 进程可运行，检查 misc_flags
check_misc_flags:
  while p has misc flags:
    MF_KCALL_RESUME → kernel_call_resume(p)  // VMSUSPEND 恢复
      → 可能变 non-runnable → goto check_runnable
    MF_DELIVERMSG   → delivermsg(p)          // 投递待处理消息
      → 可能变 non-runnable → goto check_runnable
    MF_SC_DEFER     → arch_do_syscall(p)     // ptrace 延迟的 syscall
    MF_SC_TRACE     → cause_sig(SIGTRAP)     // 通知 tracer
    MF_FLUSH_TLB    → write_cr3(current_cr3) // SMP TLB 刷新
    ... (处理完一个 flag 后重新检查 rts_flags)

  // 所有 misc_flags 处理完毕，检查量子
check_quantum:
  if p.cpu_time_left == 0:
    RTS_SET(NO_QUANTUM)
    notify_scheduler(p)
    goto pick_new

  // 恢复用户态
restore:
  restore_user_context(p)  ← iretq，永不返回

pick_new:
  while (p = pick_proc()) == NULL:
    idle()                    // halt CPU 等待中断
  proc_ptr = p
  switch_address_space(p)
  goto check_runnable
```

**关键洞察**（综合四视角）：
- **qwen 视角**：switch_to_user 不是简单的"选进程→切上下文"，而是一个**检查点循环**——每次进入都检查当前进程是否还该继续运行
- **kimi 视角**：它是"调度决策点"——决定下一个运行的进程
- **ds 视角**：它是状态机的"恢复用户态"出口——所有状态转换最终都汇聚于此
- **glm 视角**：它是三个循环的汇聚点——调度循环、IPC 循环、中断循环都通过此函数返回用户态

#### Ch2: C 源码分析

- `proc.c:299-450` — switch_to_user() 完整逻辑
- `proc.c:176-230` — idle()：halt CPU 等待中断

#### Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| switch_to_user 表达 | 循环函数 vs 状态机 enum | **loop + continue** | 与 C 控制流自然对应 |
| proc_ptr 表达 | 全局 static vs CpuLocal | **CpuLocal<*mut KProcess>** | SMP 下每核独立 |
| restore_user_context | 内联 asm vs trait | **trait ContextSwitch** | 多架构支持 |

#### Ch4: 实现要点

- `pub fn switch_to_user() -> !` 发散函数
- 内部 `loop { ... }` + `continue` 实现状态机
- `CpuLocal<proc_ptr>` 持有当前进程指针

---

### 10-scheduling-primitives: 调度原语 + 进程状态机

> **源码**: `minix3/minix/kernel/proc.c:1595-1870`, `minix3/minix/kernel/proc.h:141-274`
> **前置**: 09
> **C 行数**: ~250 行

#### Ch1: 概念

**进程可运行当且仅当 `p_rts_flags == 0`**。任何 RTS_* 置位意味着进程不在就绪队列中。

**RTS 标志位完整语义**：

| 位 | 值 | 含义 | 谁设置 | 谁清除 |
|----|-----|------|--------|--------|
| RTS_SLOT_FREE | 0x001 | 槽位空闲 | proc_init | fork |
| RTS_PROC_STOP | 0x002 | 进程被停止 | RUNCTL | RUNCTL |
| RTS_SENDING | 0x004 | 阻塞于 SEND | mini_send | delivermsg/abort_send |
| RTS_RECEIVING | 0x008 | 阻塞于 RECEIVE | mini_receive | delivermsg/cancel |
| RTS_SIGNALED | 0x010 | 有新 kernel 信号 | cause_sig | getksig |
| RTS_SIG_PENDING | 0x020 | 信号处理中 | getksig | endksig |
| RTS_P_STOP | 0x040 | 被 ptrace 停止 | trace | trace |
| RTS_NO_PRIV | 0x080 | fork 后等特权 | do_fork | do_exec/do_exit |
| RTS_NO_ENDPOINT | 0x100 | 端点失效 | (config) | (config) |
| RTS_VMINHIBIT | 0x200 | 等 VM 建页表 | proc_init | VMCTL_VMINHIBIT_CLEAR |
| RTS_PAGEFAULT | 0x400 | 有未处理缺页 | exception | vm_suspend resume |
| RTS_VMREQUEST | 0x800 | VM 请求发起者 | do_memset/do_copy | vm_suspend done |
| RTS_VMREQTARGET | 0x1000 | VM 请求目标 | do_memset/do_copy | vm_suspend done |
| RTS_PREEMPTED | 0x4000 | 被高优先级抢占 | enqueue | switch_to_user |
| RTS_NO_QUANTUM | 0x8000 | 量子耗尽 | timer_int_handler | switch_to_user |
| RTS_BOOTINHIBIT | 0x10000 | boot 未完成 | proc_init | (config) |

**MiscFlags**（不影响调度，但在 switch_to_user 的 check_misc_flags 中处理）：

| 标志 | 含义 |
|------|------|
| MF_DELIVERMSG | 有待投递消息 |
| MF_KCALL_RESUME | 内核调用需恢复（VMSUSPEND 后） |
| MF_SC_DEFER / MF_SC_ACTIVE / MF_SC_TRACE | ptrace 系统调用追踪 |
| MF_REPLY_PEND | SENDREC 的回复待收 |
| MF_MSGFAILED | 消息投递失败 |
| MF_FLUSH_TLB | TLB 需刷新（SMP） |
| MF_SENDING_FROM_KERNEL | 消息来自内核 |
| MF_VIRT_TIMER / MF_PROF_TIMER | 虚拟/性能定时器活跃 |
| MF_FPU_INITIALIZED | FPU 状态已初始化 |
| MF_SENDA_VM_MISS | 异步发送因 VM 修改地址空间失败 |

**多级优先级队列**：

- `NR_SCHED_QUEUES = 16` 个队列，0 最高，15 最低
- TASK_Q(0): 内核任务（不可抢占）, SERVER_Q(1-2): 系统服务, USER_Q(3-14): 用户进程, IDLE_Q(15): 空闲
- `pick_proc()`: 从 0 开始扫描，返回第一个非空队列的队首
- `enqueue()`: 尾部插入，若优先级高于当前进程且当前可抢占 → RTS_PREEMPTED
- `enqueue_head()`: 头部插入（抢占后恢复用）
- `dequeue()`: 从队列移除

#### Ch2: C 源码分析

- `proc.h:141-274` — RTS_* / MF_* 宏定义 + proc_is_runnable
- `proc.c:1595-1668` — enqueue()
- `proc.c:1670-1714` — enqueue_head()
- `proc.c:1716-1783` — dequeue()
- `proc.c:1785-1810` — pick_proc()
- `proc.c:1860-1891` — notify_scheduler()
- `proc.c:1893-1910` — proc_no_time()

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| RTS 位标志 | `RtsFlags` bitflags | Rust 惯用法，`is_runnable() = is_empty()` |
| MiscFlags 位标志 | `MiscFlags` bitflags | 同上 |
| 全局 run_q_head[] | `Scheduler` struct + PerCpu | SMP 下每核独立 |
| pick_proc O(N) 扫描 | 维护 highest_non_empty_queue 索引 | O(1) 优化 |
| enqueue 抢占检查 | `enqueue()` 返回 `PreemptAction` | 显式表达抢占决策 |

#### Ch4: 实现要点

- `bitflags! { pub struct RtsFlags: u32 { ... } }` + `impl RtsFlags { fn is_runnable(self) -> bool { self.is_empty() } }`
- `Scheduler` 结构体持有 `run_q_head/run_q_tail: [Option<ProcNr>; 16]`
- 对应 Rust 代码：`os/kernel/src/sched.rs`

---

### 11-ipc-core: IPC 核心机制

> **源码**: `minix3/minix/kernel/proc.c:599-1590`
> **前置**: 10（进程状态机）
> **C 行数**: ~1000 行（最复杂的部分）

#### Ch1: 概念

**六个 IPC 原语**（m3 结构）+ **阻塞/唤醒语义标注**（kimi 补充）：

| 原语 | 语义 | 阻塞条件 | 唤醒条件 | RTS 标志变化 |
|------|------|---------|---------|-------------|
| SEND | 发送消息 | 目标未在 RECEIVE | 目标调用 RECEIVE | RTS_SENDING set/clear |
| RECEIVE | 接收消息 | 无匹配消息可用 | 有消息到达 | RTS_RECEIVING set/clear |
| SENDREC | 先 SEND 再 RECEIVE（原子） | SEND 或 RECEIVE 阻塞 | 两步都完成 | RTS_SENDING → RTS_RECEIVING |
| NOTIFY | 发送轻量通知 | **永不阻塞** | — | 无（写入 s_notify_pending 位图） |
| SENDNB | 非阻塞发送 | 目标未就绪返回 ENOTREADY | — | 无 |
| SENDA | 异步批量发送 | 不阻塞（扫描表逐个投递） | — | 无 |

**消息投递的延迟拷贝设计**：
- IPC 调用时，消息不直接写入接收方用户空间
- 存入内核缓冲区 `p_delivermsg`，设置 `MF_DELIVERMSG`
- switch_to_user 在恢复进程前调用 `delivermsg()` 完成实际拷贝

**阻塞与队列**：
- SEND 阻塞：RTS_SENDING，加入目标的 p_caller_q
- RECEIVE 阻塞：RTS_RECEIVING，记录 p_getfrom_e
- 唤醒：对方完成匹配后，清除 RTS 标志，调用 enqueue()

**NOTIFY 的待处理位图**：
- 目标未在 RECEIVE 时，通知不丢失
- 存入发送方特权结构的 `s_notify_pending` 位图
- 目标下次 RECEIVE 时，has_pending_notify() 发现位图非空，立即投递

**死锁检测**：
- `deadlock()` 在 SEND/RECEIVE 阻塞前检查
- 跟踪 p_caller_q 链，发现环则返回 ELOCKED

**RECEIVE 的消息来源优先级**：
1. 待处理通知（s_notify_pending）
2. 待处理异步消息（s_asyn_pending）
3. 同步发送者（p_caller_q 中的进程）

#### Ch2: C 源码分析

**proc.c IPC 函数完整清单**（ds 补充）：

| 函数 | 行号 | 语义 | 阻塞/唤醒 |
|------|------|------|----------|
| `do_sync_ipc()` | proc.c:479-598 | IPC 入口路由 | 路由到 mini_send/receive/notify |
| `do_ipc()` | proc.c:599-698 | 系统调用级 IPC 入口 | 权限检查 + 路由 |
| `deadlock()` | proc.c:703-770 | 死锁检测 | 检查 p_caller_q 环 |
| `has_pending_notify()` | proc.c:773-810 | 检查待处理通知 | 非阻塞查询 |
| `has_pending_async()` | proc.c:811-868 | 检查待处理异步消息 | 非阻塞查询 |
| `mini_send()` | proc.c:870-965 | 同步发送 | **阻塞**：RTS_SENDING |
| `mini_receive()` | proc.c:967-1120 | 同步接收 | **阻塞**：RTS_RECEIVING |
| `mini_notify()` | proc.c:1122-1196 | 异步通知 | **永不阻塞** |
| `try_deliver_senda()` | proc.c:1200-1330 | 异步批量投递 | 非阻塞逐个尝试 |
| `mini_senda()` | proc.c:1331-1346 | 异步发送入口 | 非阻塞 |
| `try_async()` | proc.c:1348-1507 | 异步消息尝试投递 | 非阻塞 |
| `cancel_async()` | proc.c:1510-1590 | 取消异步消息 | 清理 |
| `delivermsg()` | proc.c:263-297 | 延迟消息投递 | MF_DELIVERMSG 清除 |

#### Ch3: Rust 设计决策

| C 机制 | Rust 演进 | 理由 |
|--------|----------|------|
| p_caller_q 链表 | `SenderQueue` 封装 | 类型安全 |
| deadlock() | `detect_deadlock() -> Option<Cycle>` | 显式表达 |
| mini_send/receive | `IpcEngine::send/receive` | 封装 IPC 状态机 |
| 消息拷贝 | `copy_msg_to_user() -> Result<(), PageFault>` | 统一错误处理 |
| asynmsg_t 表 | `AsyncMessageTable` 结构 | 类型安全 |

---

### 12-syscall-dispatch: 系统调用分派

> **源码**: `minix3/minix/kernel/system.c:52-167`
> **前置**: 11（IPC 错误码）
> **C 行数**: ~150 行

#### Ch1: 概念

**系统调用 vs IPC**：
- IPC（do_ipc）：进程间消息传递，内核只做中转
- 系统调用（kernel_call）：进程请求内核执行特权操作

**入口路径**：
```
用户态: sys_call(SYS_VMCTL, ...)
  → INT 0x80 / SYSENTER
  → 汇编 sys_call 入口
  → if m_type >= KERNEL_CALL:
      → kernel_call(m_user, caller)
        → copy_msg_from_user → kernel_call_dispatch → call_vec[call_nr] → handler
        → kernel_call_finish
    else:
      → do_ipc(call_nr, r2, r3)
```

**VMSUSPEND 挂起-恢复协议**：
- handler 返回 VMSUSPEND(-996)：保存请求消息，设置 MF_KCALL_RESUME
- switch_to_user 下次检查到 MF_KCALL_RESUME → kernel_call_resume
- VM 处理完缺页后，进程恢复执行

#### Ch2: C 源码分析

- `system.c:52-94` — call_vec 定义
- `system.c:59-94` — kernel_call_finish()
- `system.c:95-128` — kernel_call_dispatch()
- `system.c:136-167` — kernel_call()
- `system.c:612-638` — kernel_call_resume()

#### Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| handler 返回值 | i32 vs enum | `enum CallResult { Reply(i32), Suspend }` | 类型安全 |
| call_vec | 数组 vs match | **match**（与 07 一致） | 编译期穷尽 |
| VMSUSPEND | 魔术数 vs 类型 | `VMSUSPEND = -996` 保留为常量 | 与 C errno 对齐 |

---

### 13-exception-interrupt: 异常与中断处理

> **源码**: `minix3/minix/kernel/exception.c`, `minix3/minix/kernel/interrupt.c`, `minix3/minix/kernel/clock.c:70-199`
> **前置**: 10（进程状态）, 11（IPC——中断通过 mini_notify 唤醒进程）

#### Ch1: 概念

**三种激活路径**（qwen 核心贡献）——内核没有主循环，只有三种被激活的方式：

| 路径 | 入口 | 触发 | 处理 | 出口 |
|------|------|------|------|------|
| 硬件中断 | IDT[IRQ+32] → irq_handle() | 外部设备异步 | 遍历 hook 链 → mini_notify → switch_to_user | 调度决策 |
| CPU 异常 | IDT[vec] → exception_handler() | 当前指令导致 | 用户态→cause_sig / 页错误→转发VM / 内核态→panic | 调度决策 |
| 系统调用 | INT 0x80 → sys_call | 用户进程主动 | kernel_call / do_ipc | 调度决策 |

**三条路径的出口都是同一个**——`switch_to_user()`。这验证了 §5.4 的结论：switch_to_user 是内核运行时的核心枢纽。

**页错误处理**（最复杂的异常）——四层分解（ds 视角）：

| 层次 | 问题 | C 机制 |
|------|------|--------|
| 状态定义 | 缺页进程的状态？ | RTS_PAGEFAULT + p_vmrequest |
| 状态转换 | 缺页如何触发/恢复？ | exception → mini_send(VM) → VMCTL_CLEAR_PAGEFAULT |
| 事件源 | 什么触发缺页？ | 用户态访问未映射页 / 内核态 cross_space_copy |
| 服务 | VM 如何处理？ | 分配物理页 → 映射 → 通知内核恢复进程 |

**时钟中断**（调度的驱动力）——状态机视角（ds）：

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

**中断到调度的完整路径**：
```
硬件中断 → IDT → BKL_LOCK → irq_handle → mini_notify(target)
  → 唤醒等待进程 (RTS_UNSET(RECEIVING))
  → switch_to_user → pick_proc → restore_user_context
```

#### Ch2: C 源码分析

- `interrupt.c:29-69` — put_irq_handler()
- `interrupt.c:75-107` — rm_irq_handler()
- `interrupt.c:116-160` — irq_handle()
- `exception.c:180-286` — exception_handler()
- `exception.c:49-131` — pagefault()
- `clock.c:70-199` — timer_int_handler()

---

### 14-clock-timer: 时钟中断与定时器

> **源码**: `minix3/minix/kernel/clock.c`
> **前置**: 13

#### Ch1: 概念

- 100Hz 时钟中断驱动调度
- 量子管理：p_cpu_time_left 递减 → RTS_NO_QUANTUM → notify_scheduler
- alarm timer：per-priv 同步闹钟
- 虚拟/性能定时器：p_virt_left / p_prof_left

---

### 15-smp: 多核协同

> **源码**: `minix3/minix/kernel/smp.c`, `minix3/minix/kernel/arch/i386/apic.c`
> **前置**: 10（调度原语）

#### Ch1: 概念

- BKL (Big Kernel Lock)：spinlock，保证同一时刻只有一个 CPU 在内核态
- 临界区禁止：睡眠/调度/等待 IPC 在 spinlock 内禁止
- per-CPU 数据：run_q_head/run_q_tail, proc_ptr, cpu_is_idle
- IPI：smp_schedule (跨 CPU enqueue 唤醒), smp_schedule_vminhibit
- CPU 亲和性：p_cpu, p_cpu_mask

---

### 16-20: 系统调用服务文档

| 编号 | 文件名 | 覆盖 |
|------|--------|------|
| 16 | 16-syscall-process.md | SYS_FORK/EXEC/CLEAR/EXIT/PRIVCTL/RUNCTL/UPDATE/STATECTL |
| 17 | 17-syscall-copy.md | SYS_VIRCOPY/PHYSCOPY/SAFECOPYFROM/TO/VSAFECOPY/UMAP/VUMAP/SAFEMEMSET/MEMSET |
| 18 | 18-syscall-signal.md | SYS_KILL/GETKSIG/ENDKSIG/SIGSEND/SIGRETURN + cause_sig |
| 19 | 19-syscall-device.md | SYS_IRQCTL/DEVIO/VDEVIO/SDEVIO/IOPENABLE/READBIOS |
| 20 | 20-syscall-clock.md | SYS_TIMES/SETALARM/STIME/SETTIME/VTIMER |

每篇文档遵循 Ch1(概念) → Ch2(C源码) → Ch3(Rust设计) → Ch4(实现) → 测试 → 参见 结构。

---

### 21-privilege: 权限管理

> **源码**: `minix3/minix/kernel/system.c:274-540`, `minix3/minix/kernel/priv.h`

- struct priv：系统进程独占，用户进程共享 USER_PRIV
- s_flags: PREEMPTIBLE / BILLABLE / SYS_PROC / DYN_PRIV_ID / CHECK_IO_PORT / CHECK_IRQ / CHECK_MEM / ROOT_SYS_PROC / VM_SYS_PROC / LU_SYS_PROC / RST_SYS_PROC
- s_trap_mask / s_ipc_to / s_k_call_mask：IPC 和 syscall 权限掩码
- s_notify_pending / s_asyn_pending：挂起通知位图
- s_grant_table / s_grant_entries：grant 表（safecopy 用）
- get_priv() / set_sendto_bit() / fill_sendto_mask()

---

### 22-ipc-filter: IPC 过滤器

> **源码**: `minix3/minix/kernel/system.c:705-916`

- add_ipc_filter() / check_ipc_filter() / allow_ipc_filtered_msg()
- IPC 过滤规则：允许/拒绝特定源的消息类型

---

### 23-cross-space-runtime: 运行时跨地址空间访问

> **源码**: `minix3/minix/kernel/arch/i386/memory.c`, `minix3/minix/kernel/memory.c`
> **前置**: 17（syscall-copy）

**VMSUSPEND 四层分解**（ds 视角）：

| 层次 | 问题 | C 机制 | Rust 设计 |
|------|------|--------|----------|
| 状态定义 | "进程被 VM 挂起时，状态保存在哪？" | `p_vmrequest` + RTS_PAGEFAULT/VMREQUEST/VMREQTARGET | `VmSuspendContext` enum |
| 状态转换 | "挂起/恢复如何触发？" | `vm_suspend()` 设置状态 → `vm_suspend_exit()` 清除状态 | `vm_suspend()` / `vm_resume()` 方法 |
| 事件源 | "什么操作可能触发 VMSUSPEND？" | `do_memset` / `do_copy` / `do_safecopy` 跨地址空间访问 | 同，但 Direct Map 消除了大部分 |
| 服务 | "VMSUSPEND 解决了什么问题？" | 32 位下内核无法直接访问进程地址空间 → 需要 VM 协助 | 64 位 Direct Map 消除了大部分，但 VM 仍需处理缺页 |

**Direct Map 演进**：
- `createpde` / `lin_lin_copy` / `vm_memset`：被 `kernel_phys_to_virt()` 替代（消除）
- `vm_lookup`：保留（VM 需要查询物理页映射）
- `VMSUSPEND`：保留但场景大幅减少（仅缺页时触发，不再用于正常跨空间拷贝）

**proc.c 中跨空间相关的完整函数清单**（ds 补充）：

| 函数 | 行号 | 语义 | Direct Map 影响 |
|------|------|------|----------------|
| `vm_suspend()` | proc.c:234-298 | 挂起进程等待 VM | 保留 |
| `vm_suspend_exit()` | — | VM 完成后恢复 | 保留 |
| `vm_check()` | — | 检查是否需要 VM 协助 | 简化（大部分返回 false） |
| `createpde()` | memory.c | 临时 PDE 映射 | **消除** |
| `lin_lin_copy()` | memory.c | 跨进程 memcpy | **消除** |
| `vm_memset()` | memory.c | VM 协助 memset | **消除** |
| `vm_lookup()` | memory.c | 查询物理页映射 | 保留 |

---

### 24-misc-unported: 杂项与不移植

> **源码**: `minix3/minix/kernel/watchdog.c`, `debug.c`, `profile.c`, `usermapped_data.c`

- watchdog / NMI handler
- debug 输出（kputc / printf）
- profile（sprof）
- usermapped_data（用户态可读的内核数据页）
- 不移植的 x86-only 特性

---

## 4. 进程状态全图

### 4.1 struct proc 语义分组

| 分组 | 字段 | 一句话语义 |
|------|------|-----------|
| 身份 | p_nr, p_endpoint, p_name, p_magic | 这个进程是谁 |
| 寄存器 | p_reg (stackframe_s) | 上次切出时 CPU 寄存器的快照 |
| 地址空间 | p_seg (segframe_s: cr3, fpu_state, ldt) | 进程的地址空间根在哪里 |
| 调度 | p_priority, p_quantum_size_ms, p_cpu_time_left, p_cpu, p_scheduler | 调度器怎么对待它 |
| 运行状态 | p_rts_flags (16 位) | 为什么它现在不能运行 |
| 运行时标记 | p_misc_flags (20+ 位) | 运行时需要处理的临时条件 |
| IPC 链表 | p_nextready, p_caller_q, p_q_link, p_getfrom_e, p_sendto_e | 它在哪些队列里、等谁、给谁发 |
| 消息缓冲 | p_sendmsg, p_delivermsg, p_delivermsg_vir | 正在发/待投递的消息 |
| VM 挂起 | p_vmrequest | 缺页时挂起的请求上下文 |
| 记账 | p_accounting, p_user_time, p_sys_time, p_cycles | 用了多少 CPU 时间 |
| 特权 | p_priv → struct priv | 它能做什么 |
| 定时器 | p_virt_left, p_prof_left | 用户态/性能分析定时器 |

### 4.2 struct priv 关键字段

| 字段 | 语义 |
|------|------|
| s_flags | PREEMPTIBLE / BILLABLE / SYS_PROC / DYN_PRIV_ID |
| s_trap_mask | 允许的 IPC 原语掩码 |
| s_ipc_to | 允许发送的目标位图 |
| s_k_call_mask | 允许的内核调用位图 |
| s_notify_pending / s_asyn_pending | 挂起通知/异步消息位图 |
| s_sig_mgr / s_bak_sig_mgr | 信号管理器 / 备份信号管理器 |
| s_alarm_timer | 同步闹钟定时器 |
| s_grant_table / s_grant_entries | grant 表（safecopy 用） |

---

## 5. 内核运行时架构：三个互补视角

> 本节综合 kimi 的三层架构 + ds 的状态机视角 + glm 的三循环模型 + qwen 的激活路径

### 5.1 视角一：三层架构（kimi）——理解框架

从"读者如何理解运行时"的角度，内核代码可分为三层：

```
┌─────────────────────────────────────────┐
│  第一层：事件入口（怎么进内核）           │
│  系统调用陷阱 → kernel_call()           │
│  硬件中断     → irq_handle()            │
│  CPU 异常     → exception_handler()     │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│  第二层：状态转换（进程怎么动）           │
│  调度：pick_proc / enqueue / dequeue    │
│  IPC：mini_send / mini_receive          │
│  信号：cause_sig / sig_delay_done       │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│  第三层：基础设施（状态存在哪里）         │
│  struct proc（进程状态）                │
│  struct priv（权限状态）                │
│  RTS / MiscFlags（状态编码）            │
│  调度队列 / IPC 队列（状态组织）        │
└─────────────────────────────────────────┘
```

**叙事顺序与上述层次相反**：先讲第三层（08-10），再讲第二层（11），再讲第一层（12-13），最后讲基于这些入口的具体服务（16-24）。

### 5.2 视角二：事件驱动状态机（ds）——完备性验证

从"进程在哪些状态之间转换"的角度，内核运行时是一个事件驱动的状态机：

```
状态定义层：RTS 标志位定义进程的所有可能状态
    ↓
状态转换层：调度器 / IPC / 信号 触发状态转换
    ↓
事件源层：系统调用 / 中断 / 异常 是转换的触发源
    ↓
服务层：fork / safecopy / VMCTL 等是具体的状态转换组合
```

**验证方法**：对每个 RTS 标志位，检查"谁设置/谁清除"是否在文档中完整覆盖。如果某个 RTS 标志位的设置者或清除者没有被任何文档覆盖，则存在遗漏。

### 5.3 视角三：三个并发循环（glm）——运行时动态

从"CPU 在运行时做什么"的角度，内核运行时是三个循环的交织：

| 循环 | 入口 | 触发 | 核心函数 | C 源码 |
|------|------|------|----------|--------|
| **调度循环** | `switch_to_user()` | 永不退出 | `pick_proc()` → `restore_user_context()` | proc.c:299-474 |
| **IPC 循环** | `do_ipc()` | 进程陷入 | `mini_send/receive/notify` | proc.c:479-698 |
| **中断循环** | `exception_handler()` / `irq_handle()` | 硬件异步 | `pagefault()` / `timer_int_handler()` | exception.c:180+, interrupt.c:116+, clock.c:70 |

**关键洞察**：三个循环不是独立的——IPC 循环中的 `mini_send` 会修改调度循环中的队列；中断循环中的 `timer_int_handler` 会设置 `RTS_NO_QUANTUM` 触发调度循环中的进程切换。它们通过 `switch_to_user()` 这个汇聚点交织在一起。

### 5.4 视角四：三种激活路径（qwen）——入口分析

从"内核代码如何被触发"的角度，内核没有主循环，只有三种被激活的方式：

```
用户态进程 A 正在执行
  │
  ├─ [路径 1] 硬件中断 ──→ IDT[IRQ+32] → irq_handle() → mini_notify → switch_to_user
  │
  ├─ [路径 2] CPU 异常 ──→ IDT[vec] → exception_handler()
  │                         → 页错误? → 转发 VM → switch_to_user
  │                         → 其他? → cause_sig() → switch_to_user
  │
  └─ [路径 3] 系统调用 ──→ INT 0x80/SYSENTER → sys_call
                            → do_ipc() 或 kernel_call() → switch_to_user
```

**三条路径的出口都是同一个**——`switch_to_user()`。这个函数是内核的"调度决策点"，每次从用户态陷入内核，处理完毕后，都要经过此函数决定下一个运行的进程。

### 5.5 四视角的综合

| 视角 | 回答的问题 | 对文档规划的贡献 |
|------|-----------|----------------|
| 三层架构 | "读者应该按什么顺序理解？" | 决定文档的叙事顺序（先基础设施→再状态转换→再事件入口） |
| 状态机 | "有没有遗漏的状态转换？" | 验证文档覆盖的完备性 |
| 三循环 | "运行时 CPU 在做什么？" | 理解运行时的动态行为 |
| 激活路径 | "内核代码如何被触发？" | 理解控制流的因果关系 |

**四视角共同指向同一个结论**：`switch_to_user()` 是内核运行时的核心枢纽——它是三层架构中"事件入口→状态转换"的桥梁，是状态机中"状态转换→恢复用户态"的出口，是三循环的汇聚点，是三条激活路径的共同终点。

---

## 6. Minix3 C 源码覆盖验证

### 6.1 kernel/ 目录核心文件

| 文件 | 行数 | 核心职责 | 文档覆盖 |
|------|------|---------|---------|
| main.c | ~324 | kmain boot 序列 | 03-07 |
| proc.c | ~1980 | 调度、IPC、进程管理 | 09-11 |
| system.c | ~900 | 系统调用分派、特权、信号 | 07, 12, 21-22 |
| clock.c | ~310 | 时钟中断、定时器 | 04, 14 |
| interrupt.c | ~170 | IRQ hook 注册/分发 | 13 |
| smp.c | ~205 | SMP 启动、BKL、IPI | 15 |
| debug.c | ~563 | 内核调试输出 | 24 |
| watchdog.c | ~112 | 看门狗定时器 | 24 |
| profile.c | — | 性能统计 | 24 |
| utility.c | 93 | panic/kputc | 24 |
| usermapped_data.c | 15 | 用户态可读数据 | 24 |
| cpulocals.c | — | CPU 本地变量 | 15 |
| table.c | — | boot image 定义 | 05 |

### 6.2 system/ 目录 do_*.c 文件

| 文件 | 系统调用 | 文档覆盖 |
|------|---------|---------|
| do_fork.c | SYS_FORK | 16 |
| do_exec.c | SYS_EXEC | 16 |
| do_clear.c | SYS_CLEAR | 16 |
| do_exit.c | SYS_EXIT | 16 |
| do_privctl.c | SYS_PRIVCTL | 16 |
| do_runctl.c | SYS_RUNCTL | 16 |
| do_update.c | SYS_UPDATE | 16 |
| do_statectl.c | SYS_STATECTL | 16 |
| do_kill.c | SYS_KILL | 18 |
| do_getksig.c | SYS_GETKSIG | 18 |
| do_endksig.c | SYS_ENDKSIG | 18 |
| do_sigsend.c | SYS_SIGSEND | 18 |
| do_sigreturn.c | SYS_SIGRETURN | 18 |
| do_irqctl.c | SYS_IRQCTL | 19 |
| do_devio.c | SYS_DEVIO | 19 |
| do_vdevio.c | SYS_VDEVIO | 19 |
| do_safecopy.c | SYS_SAFECOPYFROM/TO/VSAFECOPY | 17 |
| do_copy.c | SYS_PHYSCOPY | 17 |
| do_umap.c | SYS_UMAP | 17 |
| do_umap_remote.c | SYS_UMAP_REMOTE | 17 |
| do_vumap.c | SYS_VUMAP | 17 |
| do_memset.c | SYS_MEMSET | 17 |
| do_safememset.c | SYS_SAFEMEMSET | 17 |
| do_vmctl.c | SYS_VMCTL | 08, 16 |
| do_times.c | SYS_TIMES | 20 |
| do_setalarm.c | SYS_SETALARM | 20 |
| do_stime.c | SYS_STIME | 20 |
| do_settime.c | SYS_SETTIME | 20 |
| do_vtimer.c | SYS_VTIMER | 20 |
| do_abort.c | SYS_ABORT | 24 |
| do_getinfo.c | SYS_GETINFO | 24 |
| do_diagctl.c | SYS_DIAGCTL | 24 |
| do_trace.c | SYS_TRACE | 24 |
| do_schedule.c | SYS_SCHEDULE | 24 |
| do_schedctl.c | SYS_SCHEDCTL | 24 |
| do_setgrant.c | SYS_SETGRANT | 24 |
| do_mcontext.c | SYS_SETMCONTEXT/GETMCONTEXT | 24 |
| do_sprofile.c | SYS_SPROF | 24 |

### 6.3 覆盖统计

- kernel/ 核心文件：13/13 ✅
- system/ do_*.c 文件：38/38 ✅
- 总覆盖率：100%

---

## 7. Rust 实现现状与差距

### 7.1 已实现的模块

| 模块 | 文件 | 对应 C | 覆盖程度 |
|------|------|--------|---------|
| KProcess | os/kernel/src/proc.rs | proc.h struct proc | ~60%（核心字段+RtsFlags+MiscFlags+调度字段） |
| Scheduler | os/kernel/src/sched.rs | proc.c enqueue/dequeue/pick_proc | ~50%（队列操作，缺抢占/量子管理） |
| KPriv/PrivTable | os/kernel/src/kpriv.rs | priv.h struct priv | ~40%（基本结构，缺权限掩码操作） |
| ProcessTable | os/kernel/src/proc_table.rs | proc.c proc_init | ~30%（表结构，缺 boot proc 初始化） |
| VmSuspend | os/kernel/src/vm.rs | memory.c VMSUSPEND | ~20%（类型定义，缺协议实现） |
| IRQ Manager | os/kernel/src/irq_manager.rs | interrupt.c | ~10%（骨架） |
| arch/x86_64 | os/kernel/src/arch/x86_64/ | arch/i386/ | ~15%（higher_half 基础映射） |
| boot | os/kernel/src/boot/ | boot-shim | ~20%（boot_alloc） |

### 7.2 完全未实现的核心模块

| 模块 | 对应 C | 优先级 |
|------|--------|--------|
| switch_to_user | proc.c:299-450 | P0 |
| mini_send/receive/notify | proc.c:599-1196 | P0 |
| kernel_call/dispatch | system.c:52-167 | P0 |
| exception_handler | exception.c | P0 |
| clock/timer | clock.c | P1 |
| SMP/BKL | smp.c | P1 |
| do_fork/exec/exit | do_fork.c 等 | P1 |
| do_safecopy/umap | do_safecopy.c 等 | P1 |
| do_vmctl | do_vmctl.c | P1 |

---

## 8. 关键设计决策汇总

> 每个决策标注来源方案，体现综合而非单选

### 8.1 架构级决策

| # | 决策 | 选项 | 结论 | 理由 | 来源 |
|---|------|------|------|------|------|
| D1 | 内核编译产物 | rlib vs 独立 ELF | **独立 ELF** | VMA 真实、调试器正确、trampoline 前提 | kboot-todo 4/4 AI 一致 |
| D2 | 高地址跳转 | 无 vs trampoline | **trampoline** | 恒等映射移除后内核崩溃 | kboot-todo 4/4 AI 一致 |
| D3 | AT() 技巧 | 保留 vs 不保留 | **不保留** | boot-shim 自行计算物理地址更清晰 | kboot-todo 3/4 AI 一致 |
| D4 | ELF 加载器 | 第三方 crate vs 手写 | **手写 ~50 行** | 依赖为零，ELF header 极其稳定 | kboot-todo 4/4 AI 一致 |
| D5 | boot 模块加载 | 嵌入 vs UEFI 读分区 | **UEFI 读分区** | 灵活，与 GRUB load_mods 语义一致 | kboot-todo 4/4 AI 一致 |
| D6 | Kernel Direct Map 建立者 | Kernel vs VM | **VM** | VM 是页表所有者，map_kernel() 是 VM 的职责 | m3 + kimi |
| D7 | 文档组织轴 | 时间线 vs 认知顺序 vs 状态机 | **时间线为主，认知顺序为辅** | 时间线保证不遗漏，认知顺序保证可读性 | m3(时间线) + kimi(认知) 综合 |

### 8.2 运行时设计决策

| # | 决策 | 选项 | 结论 | 理由 | 来源 |
|---|------|------|------|------|------|
| D8 | call_vec 表达 | 数组 vs match | **match** | 类型安全 + 编译期穷尽检查 | m3 + kimi 一致 |
| D9 | RTS 标志 | 裸整数 vs bitflags | **bitflags** | Rust 惯用法 | 5/5 AI 一致 |
| D10 | switch_to_user | 普通函数 vs `-> !` | **`-> !`** | 类型系统表达永不返回 | m3 + kimi 一致 |
| D11 | 跨进程拷贝 | createpde vs Direct Map | **Direct Map** | 64 位一行加法替代临时映射 | 5/5 AI 一致 |
| D12 | BKL 表达 | 全局变量 vs SpinLock | **SpinLock** | 明确临界区 | ds + glm |
| D13 | per-CPU 数据 | 全局数组 vs CpuLocal | **CpuLocal** | 类型安全，SMP 正确 | ds + glm |
| D14 | IPC 错误码 | C errno vs Rust enum | **enum IpcError** | 类型安全 | kimi + ds |
| D15 | VMSUSPEND | 魔术数 vs 类型 | **VmSuspendContext enum** | 类型安全的状态机 | ds(四层分解) + kimi(类型设计) |
| D16 | 硬件抽象 | 直接操作 vs trait | **trait** | 多架构支持，机制vs策略分离 | 5/5 AI 一致 |
| D17 | switch_to_user 实现 | 循环函数 vs 状态机 enum | **loop + continue** | 与 C 控制流自然对应 | m3 + qwen(伪代码) |
| D18 | IPC 叙事结构 | 按原语 vs 按阻塞/唤醒 | **按原语+阻塞/唤醒标注** | m3 结构清晰 + kimi 语义完整 | m3 + kimi 综合 |
| D19 | VMSUSPEND 叙事 | 协议描述 vs 四层分解 | **四层分解+类型设计** | ds 分解保证完备 + kimi 类型保证安全 | ds + kimi 综合 |
| D20 | 运行时理解框架 | 单一视角 vs 多视角 | **四视角互补** | 每个视角回答不同问题 | 五方案综合 |

---

## 9. 与第一版规划的区别

第一版规划（tmp_* 文件）的三个结构性问题已在 §0.3 详细诊断。本规划的修正：

| 问题 | tmp 做法 | 本规划修正 | 来源 |
|------|---------|-----------|------|
| 叙事起点不统一 | 每篇独立建立上下文 | 时间线为主轴，认知顺序为辅轴 | m3 + kimi |
| 因果关系缺失 | 按主题并列 | 先状态→再调度→再通信→再事件→再服务 | kimi + ds |
| 运行时 vs 初始化混淆 | 跨空间机制在 02 | T10（switch_to_user）是分界点 | m3 + qwen |
| 前向引用 | "详见后文" | 禁止前向引用，概念首次出现即完整 | kimi |
| 概念碎片化 | struct proc 分散在 4 篇 | 首次出现时完整解释 | kimi |
| 缺少完备性验证 | 无 | 状态机"谁设置/谁清除"验证 | ds |
| 缺少因果分析 | 无 | 三种激活路径分析 | qwen |
| 缺少约束验证 | 无 | 依赖图保证无前向引用 | glm |

**核心区别**：第一版是"按主题整理知识"，本规划是"按认知顺序组织叙事"。前者对作者方便，后者对读者友好。

---

## 10. 参见

- [kboot-new.md](kboot-new.md) — Boot 阶段 01-06 的详细规划
- [kboot-design.md](kboot-design.md) — Boot 架构设计讨论记录
- [kboot-problem.md](kboot-problem.md) — Boot 阶段 P0 问题清单
- [kboot-todo.md](kboot-todo.md) — Boot 阶段 TODO 列表
- [runtime-design-m3.md](runtime-design-m3.md) — 运行时设计（m3 版，时间线视角）
- [runtime-design-kimi.md](runtime-design-kimi.md) — 运行时设计（kimi 版，三层架构视角）
- [runtime-design-ds.md](runtime-design-ds.md) — 运行时设计（ds 版，状态机视角）
- [runtime-design-qwen.md](runtime-design-qwen.md) — 运行时设计（qwen 版，激活路径视角）
- [runtime-design-glm.md](runtime-design-glm.md) — 运行时设计（glm 版，依赖图视角）
