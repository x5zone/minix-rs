# 06 — 进程表初始化与 boot 进程加载

> **阶段**：C（kernel stage）/ 阶段 C
> **范围**：进程表/特权表/RTS 清空与填充、boot image 遍历、VM ELF 加载、boot→running 转换
> **ground truth**：Minix3 源码 `minix3/minix/kernel/{proc.c,main.c,system.c}` + `minix3/minix/kernel/arch/*/{protect.c,memory.c,arch_system.c}`
> **Rust 实现**：`os/kernel/src/{proc.rs,proc_table.rs,kpriv.rs,capability.rs,lib.rs}` + `os/arch/src/{x86_64,arm64,riscv64}/boot.rs`
> **前置文档**：[03-kmain-cstart.md](./03-kmain-cstart.md)、[05-clock-interrupt-init.md](./05-clock-interrupt-init.md)
> **后续文档**：[07-cross-space-init.md](./07-cross-space-init.md)、[08-system-init-boot-finish.md](./08-system-init-boot-finish.md)、[10-switch-to-user.md](./10-switch-to-user.md)

---

## Ch1. 概述（概念导向，建立心智模型）

阶段 C 解决的问题：进程本质、为什么需要进程表/特权表/RTS 标志、VM 鸡生蛋问题、CPU 要回答的四个问题、boot 流程的起点与终点。

### 1.0 本章讲什么

Minix3 内核启动分为 6 个阶段（A-F），本章聚焦**阶段 C：进程表初始化与 boot 进程加载**。

| 阶段 | 名称 | 关键动作 | 文档 |
|------|------|---------|------|
| A | boot shim 引导 | bootloader 接管，加载内核镜像 | 01-boot-shim-bootstrap.md |
| B | kmain / cstart | 内核进入 C 入口，初始化平台、内存、arch | 03-kmain-cstart.md |
| **C** | **进程表初始化与 boot 进程加载** | **清空进程表/特权表 → 遍历 boot image → 加载 VM ELF** | **本文档** |
| D | 跨空间初始化 | 确认 direct_map 就绪（废弃 freepdes/ptproc 临时窗口） | 07-cross-space-init.md |
| E | 系统服务初始化 | RS 启动其他系统服务 | 08-system-init-boot-finish.md |
| F | 首次切换到用户态 | 调度器选中第一个用户进程，切换 | 10-switch-to-user.md |

阶段 A/B 已完成内核自身的初始化（平台探测、内存布局识别、arch 早期设置）。阶段 C 接手的是一张白板：进程表空空如也，没有任何用户态进程存在。阶段 C 的任务是**把编译时硬编码的 boot image 清单实例化为进程表中就位（但暂停）的进程**，为阶段 D/E/F 做准备。

**目标读者**：已理解 Minix3 微内核基本结构（微内核/系统服务/用户进程分层、IPC 消息传递模型）的开发者。

**本章不讲什么**：阶段 A/B/D/E/F 的详细实现（见对应文档）；Rust 类型设计（Ch3）；具体代码实现（Ch4）。

### 1.1 进程的本质

要理解阶段 C 在做什么，必须先回答一个更根本的问题：**进程是什么？**

#### 1.1.1 CPU 时间的"断点续传"

CPU 本质上只有一条执行流：取指 → 译码 → 执行 → 写回，周而复始。但操作系统要让用户感觉"多个程序在同时运行"——浏览器、编辑器、终端、内核守护进程似乎都在并行工作。

这是怎么做到的？答案是**CPU 时间的"断点续传"**：

- OS 让进程 A 跑几毫秒，然后**暂停 A**，把 CPU 让给进程 B
- 暂停 A 时，必须**完整保存 A 当前的所有状态**：通用寄存器、程序计数器（PC）、栈指针（SP）、状态寄存器、页表基址……
- 等下次轮到 A 时，**恢复这些状态**，A 从被暂停的位置继续执行，仿佛从未被打断

这种"保存 → 切换 → 恢复"的机制，让单条 CPU 执行流在多个进程之间快速轮转，营造出"同时运行"的幻象。**进程就是这种"可暂停、可恢复的执行流"的抽象**。

#### 1.1.2 核心矛盾与桥梁

这里有一个核心矛盾：

```
单 CPU 只有单执行流  vs  OS 要提供多进程抽象
```

桥梁就是**进程表**——一个数据结构，保存每个进程被暂停时的完整状态。当 CPU 从进程 A 切换到进程 B 时：

1. 把 A 的当前 CPU 状态写入进程表中 A 的槽位（slot）
2. 从进程表中 B 的槽位读出 B 上次暂停时的状态
3. 加载到 CPU 寄存器，B 恢复执行

进程表的本质就是**"断点"的存档数据结构**：每个进程一个 slot，slot 里存放该进程被暂停时的完整快照。

#### 1.1.3 进程在内核眼中的组成

从内核视角看，一个进程由五部分组成：

| 组成 | 内容 | 回答的问题 |
|------|------|----------|
| slot | 进程表中的一个槽位 | "进程在哪儿存？" |
| 寄存器状态 | 通用寄存器 + PC + SP + 状态寄存器 | "进程被暂停时 CPU 是什么状态？" |
| 调度属性 | 优先级、时间片、所属 CPU、调度器指针 | "进程什么时候能跑？跑多久？" |
| IPC 状态 | 等待的消息、阻塞队列指针 | "进程在等谁？谁在等它？" |
| VM 状态 | 页表指针、内存映射 | "进程的地址空间长什么样？" |

阶段 C 的工作，就是为 boot image 中的每个进程**填好这五部分**，让它们"就位但暂停"，等待后续阶段唤醒。

#### 1.1.4 三类运行态实体（User Process / System Server / Kernel task）

> 架构范围：**三架构共性**（仅 Ring 特权级跨架构名称不同：x86-64 ring/EL/privilege mode；本质均为"谁跑 Ring 0 共用 kernel image"）
>
> **深度版本见 [00-kernel-overview.md §1.4.1](./00-kernel-overview.md)**：那里从"执行上下文 vs 代码+状态"两个维度解释三类实体机制的推导过程。本节给出三分法结论 + boot 语境下的差异。

§1.1.3 的"五部分组成"对所有进程一视同仁——但 Minix3 实际上把**运行态实体**分成三类，机制不同。三类区分同时也是 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) 讨论 timer 时的关键背景（CLOCK task 与普通 timer 硬件不同层）。

**三分法总表**：

| 类别 | 特权级 | 地址空间 | Minix3 实例 | boot 期形态 | minix-rs 翻译 |
|------|--------|---------|-------------|-------------|---------------|
| **User Process（用户进程）** | Ring 3 | 独立虚拟地址空间（CR3/satp 切换） | INIT、以及一切由 RS fork/exec 的应用 | **boot 期尚不存在**——由 RS 在阶段 E 运行时创建 | `ProcKind::UserProcess` |
| **System Server（系统服务器）** | Ring 3 | 独立虚拟地址空间 | VM / PM / VFS / RS / DS / sched 等 | 从 multiboot 模块加载 ELF，进表待调度 | `ProcKind::{Vm, RootService, UserService}` |
| **Kernel task（内核 task，即一般所说的 Kernel Subsystem）** | Ring 0 | **共用 kernel image**（无独立地址空间概念；不切换 CR3） | CLOCK / SYSTEM(SYSTASK) / IDLE / HARDWARE / ASYNCM | **无 ELF**，编译时内建在 kernel 镜像里，仅占 proc 槽位 | `ProcKind::KernelTask` |

**三项关键澄清**（最易错的概念）：

1. **"System Task"术语严格指 Kernel task**——`NR_TASKS=5`、负 endpoint 的实体只有 ASYNCM/IDLE/CLOCK/SYSTEM/HARDWARE 五个（`table.c:44-51`）。许多教学材料（包括部分 Minix 文档）把 PM/VM/RS 称为"系统任务"，是误用术语。**PM/VM/RS 的真名是"系统服务器"**（system server）——它们在 Ring 3 运行，与普通用户进程机制相同，只是通过 `priv` 特权表获得特殊权限。**现代 Minix3 的调度策略由用户态 `sched` server 承担（`SCHED_PROC_NR` 为正 endpoint）**，它同样不是内核 task。
2. **Kernel task 的"运行"是事件驱动的，不是独立执行流**——而且在当前现代 Minix3 实现中，**它们的 `proc` 槽位不会被调度器作为 execution context 恢复**。证据有两重：
   - **RTS 阻断**：boot 结束时内核只清除非内核 task 的 `RTS_PROC_STOP`（`main.c:64-66`），内核 task 的这个标志（`main.c:268`）永不被解析路径触碰，`main.c:62` 注释称其为 "former kernel tasks"——`RTS_PROC_STOP` 恒置意味着 `proc_is_runnable()` 永远为 false
   - **无恢复点**：`arch_proc_init()`（设置 PC/SP 的入口）只在 `do_exec` 路径被调用（`system/do_exec.c:45` + `arch/i386/memory.c:722-732`），kernel task 从未被设置执行入口——即使 RTS 阻塞被绕过，调度器也没有合法的恢复点去切到 kernel task
   - **没有主循环实现**：当前源码中不存在 `sys_task()`/`clock_task()`，仅 `system.c:12` 残留一处注释
   - CLOCK 的实际入口是 `timer_int_handler()`（时钟中断到来时执行，`clock.c:70`），SYSTASK 的实际入口是 `kernel_call()`（进程 trap 进来时查 `call_vec` 分发表，`system.c:136-163`）——根本原因是 kernel task **没有独立地址空间**（共享 kernel image），且 kernel task 从未被 `arch_proc_init()` 设置执行入口（见上条"无恢复点"），因此调度器既无可切换的页表（不切换 CR3），也无须保存完整用户态寄存器快照（kernel 常驻，无用户态寄存器）。机制推导见 [00-kernel-overview.md §1.4.1](./00-kernel-overview.md)。
3. **IDLE 是例外**——每 CPU 一个，是唯一真正自持执行流的内核实体（无就绪进程时 `idle()` 空转等待中断，`proc.c:176-193`）。它几乎无状态，恰好反证"执行流"与"状态"是两个正交维度。

**三类实体在 §1.1.3 "五部分组成"中的差异**（boot 语境下关注这几列）：

| 组成 | User / Server | Kernel task |
|------|---------------|-------------|
| **VM 状态** | 有（独立页表，CR3/satp 切换） | 无（共用 kernel image，不需要页表） |
| **IPC 状态** | 有（消息传递） | 有（IPC 身份，可被 notify/send），但无独立执行流去处理——靠事件入口 |
| **调度属性** | 有（优先级/时间片，由 sched server 决定） | 有槽位但几乎不被调度器选中（boot 后）；优先级与 User/Server 不互通 |
| **寄存器状态** | 完整快照（SS/RSP 全栈） | 不需要保存完整用户栈（kernel 常驻，无用户态寄存器） |

§1.4 boot image 的清单**同时包含**两类实体——System Server 的 VM（第一个被装载的 ELF；PM/FS/... 在阶段 E 由 RS 服务拉起）和 Kernel task（CLOCK/SYSTEM/IDLE/HARDWARE/ASYNCM，编译时内建、无 ELF）。阶段 C 为它们填 proc 槽位、设特权，但 Kernel task 的"运行"不属于阶段 C 范畴（见 [14-exception-interrupt.md](./14-exception-interrupt.md) 与 [12-ipc-core.md](./12-ipc-core.md)）。调度器完整设计（含"内核 task 不可抢占、时间片耗尽直接重置"）见 [11-scheduling-primitives.md](./11-scheduling-primitives.md)；SMP 下 Kernel task 的并发模型见 [16-smp.md](./16-smp.md)。

### 1.2 CPU 要回答的四个问题

> **框架关系**：本节用"四问"框架；[03-kmain-cstart.md §1.4](./03-kmain-cstart.md) 用"三问"框架（"当前特权级？异常/syscall 跳哪？用哪个栈？"，由阶段 B 的 `prot_init()` 静态配置）。两套框架服务于同一目标——「让 CPU 能正常跨特权级工作」——但所处的阶段与回答的问题不同：
>
> - §1.4「三问」是阶段 B 的**硬件保护结构配置**（GDT/IDT/栈），回答「CPU 进入内核态后如何被保护、何处响应异常」。
> - 本节第四问（VM 鸡生蛋 / 第一个进程的地址空间谁建）是阶段 C 的**运行态特化准备**——VM 的 bootstrap 页表必须由内核手工搭好，第一个用户进程才能进入 Ring 3。这是阶段 C 引入的额外问题，不在 §1.4「三问」范围内。

boot 一个进程，本质上是让 CPU 能开始执行这个进程的代码。要做到这一点，CPU 必须回答四个问题：

1. **寄存器初值是什么？**（通用寄存器 + PC + SP + 状态寄存器）
2. **用户态入口在哪？**（ELF entry point——进程的第一条指令地址）
3. **栈和参数怎么传？**（SP 指向栈顶，参数通过 ps_strings 结构传递）
4. **第一个进程的地址空间谁建？**（VM 的 bootstrap 页表——鸡生蛋问题）

#### 1.2.1 状态寄存器：CPU 的"模式开关"

状态寄存器（x86 称 PSW/RFLAGS，ARM 称 PSR，RISC-V 称 sstatus）保存 CPU 的关键控制位：

- **条件标志**：上一条指令运算结果（零/进位/符号/溢出）
- **中断使能位**：当前是否允许响应中断
- **当前特权级**：CPU 现在运行在内核态还是用户态

boot 一个进程时，必须设置状态寄存器的初值，决定进程**首次运行时的特权级和中断状态**。例如：

- 内核 task（CLOCK/SYSTEM/IDLE）：设置内核态初值（`INIT_TASK_PSW`/`INIT_TASK_PSR`，中断策略各架构不同，见 §1.2.3 三架构对照表）——但如前所述（§1.1.4），这个初值只会被"借用执行"，它们不作为独立运行实体被调度
- 用户进程（VM/RS/普通服务）：运行在用户态，中断使能

#### 1.2.2 ps_strings：参数传递的小型结构体

用户进程启动时需要接收参数（argc/argv/envp）。Minix3 的做法是在栈顶放一个 `ps_strings` 结构体：

```
栈布局（高地址 → 低地址）：
+------------------+
| envp[n] = NULL   |
| envp[0..n-1]     |  环境变量指针数组
+------------------+
| argv[argc] = NULL|
| argv[0..argc-1]  |  参数指针数组
+------------------+
| argc             |  参数个数
+------------------+  ← SP 最终指向这里
| ps_strings 结构体 |  含 argv/envp 指针，便于启动代码定位
+------------------+
```

进程首次运行时，启动代码（C runtime）通过 ps_strings 结构体定位 argv/envp，然后调用 `main(argc, argv, envp)`。ps_strings 的地址通过特定寄存器传递：

| 架构 | 传递 ps_strings 的寄存器 |
|------|----------------------|
| x86-64 | rbx |
| aarch64 | r0 |
| riscv64 | a0 |

#### 1.2.3 三架构对照表

> **统一抽象层**：尽管三个 ISA 在寄存器名、状态位、特权级术语上各不相同，但**它们面对"boot 一个进程"时都必须回答同一个四问**——本表把每问映射到各 ISA 的具体实现：
>
> - **状态寄存器初值**：决定首次运行的特权级 + 中断策略。三 ISA 的状态寄存器分别是 RFLAGS（x86-64）/ PSTATE（aarch64）/ `sstatus`（riscv64）。
> - **特权级机制**：决定哪些代码可执行特权指令。x86-64 通过段选择子 + CS（CPL）实现；aarch64 通过 Exception Level（EL0/EL1/EL2/EL3）；RISC-V 通过模式位（M/S/U）。
> - **入口点寄存器**：进程首次运行的指令地址。x86-64 `rip` / aarch64 `elr_el1` / RISC-V `sepc`——三者均通过 `arch_proc_init()` 写入。
> - **栈指针寄存器**：进程首次运行的栈顶。x86-64 `rsp` / aarch64 `sp_el0` / RISC-V `sp`。
> - **ps_strings 寄存器**：进程启动代码定位参数的依据。x86-64 `rbx` / aarch64 `r0` / RISC-V `a0`。

| 问题 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 状态寄存器初值（内核 task） | INIT_TASK_PSW=0x1202（IOPL=1，IF=1） | INIT_TASK_PSR（EL1h，F=1） | INIT_TASK_SSTATUS（SPP=1，SPIE=1） |
| 状态寄存器初值（用户进程） | INIT_PSW=0x0202（IOPL=0，IF=1） | INIT_PSR（EL0t） | INIT_USER_SSTATUS（SPP=0，SPIE=1） |
| 特权级机制 | 段选择子（CS/DS/SS/ES/FS/GS） | EL（Exception Level） | 特权模式（M/S/U） |
| 入口点寄存器 | rip | elr_el1 | sepc |
| 栈指针寄存器 | rsp | sp_el0 | sp |
| ps_strings 寄存器 | rbx | r0 | a0 |

> **架构范围标注**：x86 的"段选择子"是 x86 段机制的遗留产物（虽然现代 x86-64 主要用页式管理，但段选择子仍需设置以选择特权级）。aarch64/riscv64 无此概念，直接用 EL/特权模式区分内核态/用户态。
>
> **数值语境说明**：本表"状态寄存器初值"列反映的是 minix-rs 内部常量（x86-64 见 `os/arch/src/x86_64/boot.rs:29/34`，INIT_TASK_PSW=0x1202 / INIT_PSW=0x0202）。Minix3 C 端 x86 对应值为 0x1200 / 0x0200（`arch/i386/include/archconst.h:118-119`），差异在于 `IF` 位（中断标志）的显式置位。aarch64 / riscv64 列仅 minix-rs 64 位移植有定义（Minix3 C 端 `arch/` 下只有 i386 与 32-bit ARM），故 aarch64 列对应 AArch64 EL 模型，riscv64 列对应 RISC-V SPP/SPIE，与 32-bit ARM 的 USR32_MODE/SVC32_MODE 不属同一 ISA 层级。

### 1.3 三件套：进程表、特权表、RTS 位图

用 CPU 四问框架理解阶段 C 的三个核心数据结构——"三件套"：

| 数据结构 | 回答的问题 | 本质 |
|---------|---------|------|
| 进程表 | "进程是谁？" | slot 布局 + 标识符 + 生命周期 |
| 特权表 | "进程被允许做什么？" | 静态能力（类型标志/trap_mask/ipc_to/k_call_mask/io_tab/irq_tab/sig_mgr） |
| RTS 位图 | "进程现在能不能跑？" | 动态状态（非零 = 不可运行） |

本节只给每件建**最小心智模型**（~30 行/件）；字段全集与运行时读写转交专门文档（进程表 → 17，特权表 → 22，RTS → 11）。

#### 1.3.1 进程表：进程的"身份证"

进程表是一个数组，每个元素是一个 slot，存放一个进程的完整状态。slot 布局：

```
slot 索引:  0 .. NR_TASKS-1     NR_TASKS .. NR_PROCS-1
            ┌────────────────┐  ┌────────────────┐
进程类型:   │ 内核 task       │  │ 用户进程        │
            │ (CLOCK/SYSTEM/ │  │ (VM/PM/VFS/RS/ │
            │  IDLE/KERNEL)  │  │  INIT/...)     │
            └────────────────┘  └────────────────┘
p_nr:       -NR_TASKS .. -1     0 .. NR_PROCS-NR_TASKS-1
            (负数)               (非负)
```

**关键标识符**：

- **p_nr（进程号）**：等于 slot 索引偏移。负数是内核 task，非负是用户进程。`proc_addr(n)` 用数组下标直接定位 slot——O(1) 访问。
- **p_endpoint（端点号）**：= (generation, p_nr) 二元组。每次 slot 被回收复用，generation 递增。

**为什么要 generation？** 考虑场景：进程 A 持有进程 B 的端点号，想给 B 发 IPC 消息。但 B 已经退出，它的 slot 被复用给了新进程 C。如果端点号只含 p_nr，A 会把消息发给 C——这是**陈旧引用**问题。generation 机制让 A 持有的旧端点号（generation=5）与新进程 C 的端点号（generation=6）不匹配，内核检测到不匹配后返回错误，A 知道 B 已退出。

这是分布式系统中"陈旧引用"防御的经典模式——用版本号区分同名实体的不同代际。

**slot 生命周期**：

```
SLOT_FREE ──分配──> 运行/阻塞/停止 ──退出──> 回收回 SLOT_FREE
                         ↑                    │
                         └────────────────────┘
                         (slot 复用，generation 递增)
```

> 字段全集（标识/调度/IPC/统计/上下文/生命周期 6 分组）→ Ch2 §2.1；slot 生命周期运行时读写（fork 继承/exit 回收）→ [17-syscall-process.md §1.1](./17-syscall-process.md)。

#### 1.3.2 特权表：进程的"能力清单"

特权表独立于进程表，存放进程的**静态能力**——进程被允许做什么。

**为什么特权独立于进程表？** 资源稀缺。进程表有 NR_PROCS（数百）个 slot，但特权表只有 NR_SYS_PROCS（数十）个 slot——只有系统进程需要独立特权，普通用户进程共享默认特权。如果把特权字段平铺到进程表，每个 slot 都要携带 30+ 字段的特权信息，浪费大量内存。

**特权的内容**（静态能力）：

| 能力 | 含义 |
|------|------|
| 类型标志 | IDL_F/TSK_F/SRV_F/RSYS_F/VM_F 等，标识进程角色 |
| trap_mask | 允许触发哪些 trap（系统调用/异常） |
| ipc_to | 允许向哪些进程发 IPC |
| k_call_mask | 允许调用哪些内核系统调用 |
| io_tab | 允许访问哪些 I/O 端口 |
| irq_tab | 允许处理哪些中断 |
| sig_mgr | 信号管理器（谁负责处理这个进程的信号） |

**分配方式**：

- **静态分配**（boot 期）：内核 task/RS/VM 立即获静态特权
- **动态分配**（运行时）：普通用户进程由 RS 在运行时配置

阶段 C 的 boot 循环中，**只有 schedulable 进程（内核 task + RS + VM）立即获静态特权**；其他用户进程标记为"无特权/无时间片"（RTS_NO_PRIV|RTS_NO_QUANTUM），等 RS 运行时配置。

> 存储分治（priv[64] 静态表 + USER_PRIV 共享槽）→ Ch2 §2.2；字段语义与 CapabilityTemplate → [22-privilege.md §1.1-§1.3](./22-privilege.md)。

#### 1.3.3 RTS 位图：进程的"运行开关"

RTS（Run-Time Status）是一个位图，记录进程当前不可运行的原因。

**为什么用位图而非枚举？** 因为进程可以**同时因多个原因不可运行**。例如，一个普通用户进程在 boot 期可能同时：

- 无特权（RTS_NO_PRIV）
- 无时间片（RTS_NO_QUANTUM）
- 等 VM 建页表（RTS_VMINHIBIT）
- 等 boot 完成（RTS_BOOTINHIBIT）

枚举只能表达单一原因，位图支持多原因叠加——这是位图相对枚举的本质优势。

**核心不变量**：

> **A process is runnable iff p_rts_flags == 0**

即：RTS 位图全零 = 进程可运行；任何一位被设 = 进程不可运行。

**阶段 C 涉及的关键 RTS 位**：

| 位 | 含义 | 设置时机 |
|----|------|---------|
| RTS_SLOT_FREE | slot 空闲 | proc_init 清空时 |
| RTS_NO_PRIV | 无特权 | 非 schedulable 用户进程 |
| RTS_NO_QUANTUM | 无时间片 | 非 schedulable 用户进程 |
| RTS_VMINHIBIT | 等 VM 建页表 | 非 VM 用户进程 |
| RTS_BOOTINHIBIT | 等 boot 完成 | 非 VM 用户进程 |
| RTS_PROC_STOP | 进程停止 | 所有 boot 进程（阶段 C 结束状态） |

**RTS_SET/UNSET 的隐藏职责**：设置/清除 RTS 位时，会**自动维护调度队列一致性**——设标志 → 从调度队列 dequeue，清标志 → enqueue。这避免了调用方手动维护队列的遗漏风险。

misc_flags 与 RTS 分工：RTS 决定**可运行性**（影响调度队列），misc_flags 记录**不影响调度的次要状态**（如 MF_DELIVERMSG 有待投递消息、MF_PROF_TIMER 性能 profiling）。

> 16 位全集与「清位即入队」状态机 → Ch2 §2.1.6；enqueue/dequeue 联动细节 → [11-scheduling-primitives.md §1.1](./11-scheduling-primitives.md)。

### 1.4 boot image 与 VM 的"开天辟地"问题

#### 1.4.1 boot image：编译时硬编码的进程清单

阶段 C 要实例化的进程来自 **boot image**——一份编译时硬编码的进程清单。

**为什么编译时硬编码？** boot 期没有文件系统。内核启动时，没有任何进程在运行，没有文件系统可读——清单必须编译进内核镜像。这是 bootstrapping（自举）的另一面：最早的东西必须"自带"。

**三类进程**：

| 类型 | 例子 | p_nr | boot 期处理 |
|------|------|------|------------|
| 内核 task | CLOCK/SYSTEM/IDLE/HARDWARE/ASYNCM | 负数 | 立即获静态特权（`main.c:196-200`），无 ELF；但 `RTS_PROC_STOP` 永不清除（`main.c:64-66`），**不会作为运行实体被调度** |
| 系统进程 | VM/PM/VFS/RS/sched | 非负 | multiboot 提供 ELF，VM 立即加载，其他等 RS |
| 普通用户进程 | INIT | 非负 | boot 期不存在，由 RS 运行时 fork/exec |

**NR_BOOT_PROCS vs NR_BOOT_MODULES**：`NR_BOOT_PROCS = NR_TASKS + NR_BOOT_MODULES = 5 + 12 = 17`（`param.h:9`）是 boot image `image[]` 的总长（5 内核 task + 12 用户态模块）；`NR_BOOT_MODULES = INIT_PROC_NR + 1 = 12`（`com.h:74`）只算用户态 boot 模块，与 multiboot 模块列表 `kinfo.module_list` 一一对应（`main.c:182` 用 `kinfo.module_list[i - NR_TASKS]` 对应 `image[NR_TASKS + i]`）。

**编号映射表**：boot image 顺序（multiboot 模块索引）≠ proc 号——第 i 个 multiboot 模块必须落到 `com.h` 固定 proc 号指定的槽位：

| 模块索引 i（boot image 顺序） | 进程 | C proc_nr（com.h） | endpoint（gen=0） |
|----|----|----|----|
| 5 | DS | 6 | 6 |
| 6 | RS | 2 | 2 |
| 7 | PM | 0 | 0 |
| 8 | SCHED | 4 | 4 |
| 9 | VFS | 1 | 1 |
| 10 | MEM | 3 | 3 |
| 11 | TTY | 5 | 5 |
| 12 | MIB | 7 | 7 |
| 13 | VM | 8 | 8 |
| 14 | PFS | 9 | 9 |
| 15 | MFS | 10 | 10 |
| 16 | INIT | 11 | 11 |

**为什么 DS/RS 排在最前**（位置 vs 加载 vs 调度，三个维度分开看）：

1. **「位置第一」≠「加载第一」≠「执行第一」**——三个概念正交：
   - **位置**（image[] 数组顺序）：DS=image[5]——只是 multiboot 模块的物理摆放约定，决定 GRUB 把每个 ELF 放在哪段物理内存（`kinfo.module_list[i].mod_start`）。
   - **加载**（ELF 真的被解析+映射进页表）：只有 VM——`main.c:265` 的 `if (proc_nr == VM_PROC_NR) arch_boot_proc()` 是 boot 循环里**唯一**调用；其他 11 个模块的 ELF 在阶段 C **根本不会被加载**。
   - **执行**（CPU 真正开始跑这进程）：调度器按优先级 + 状态选；阶段 C 结束时所有 boot 进程都带 `RTS_PROC_STOP`，最终顺序由 `bsp_finish_booting` 决定（§1.5/§4.8）。

2. **DS/RS 在 image[] 里靠前，只决定了 DS/RS 的 ELF 物理地址最先被登记到 multiboot 列表**——table.c 注释（L38-41）说明：DS 必须第一个以保证系统事件可靠异步发布（NOTIFY 消息），RS 紧随其后以处理周期性 ping。这是一个**"地址顺序约定"**，不是"加载时序"——DS/RS 的 ELF 也从未在阶段 C 被加载。

   **注释的真实意图与可验证的物理后果**：注释的"DS must be first"是**作者的设计意图声明**——但**这不是运行时正确性约束**，C 代码里没有强制 image[] 顺序的 assert / `#error`。Minix3 当前实现里：
   - boot 循环**原子执行**（持 BKL，无调度切换）——循环结束后所有 12 个 user boot 进程的 proc 表项都已就绪；
   - 其他进程通过 endpoint 数字（如 `DS_PROC_NR=6`）定位 DS——**不依赖 image[] 位置**；
   - 调度优先级由 main.c:209-210 显式写 `SRV_Q` 决定——**与 image[] 位置无关**。

   因此 image[] 顺序**没有可验证的运行时影响**——注释反映的是**作者的风格偏好或早期设计意图**，而非程序正确性所依赖的隐式顺序。**这是 Minix3 上游的设计选择**：依赖一个未在代码中显式强制的数组顺序表达"基础服务靠前"的意图——读者只能通过 table.c 注释理解。**minix-rs 在这里做了显式化改进**：以 [`BOOT_MODULE_PROC_NRS[i]`](file:///os/kernel/src/proc.rs#L130-L142) 替代 image[] 数组下标的隐式含义——每个 index 都明确给出对应的 ProcNr 与 com.h 来源，读者无须数位置（详见 §4.0 / §4.7）。

3. **其他 11 个模块的 ELF 怎么进来**：躺在 multiboot 物理内存里等 RS exec——RS 唤醒后通过 IPC 让 VM 按需建页表并 exec（这是运行时路径，详见 17-syscall-process.md 与 09-vm-boot-protocol.md）。**"加载"这个动作只对 VM 发生一次，其他模块的加载属于运行时 exec，不是 boot 期动作**。

proc 号决定进程表槽位（`proc_addr(proc_nr)`）与 endpoint（gen=0 时 endpoint = proc 号，服务器间靠 endpoint 寻址），因此必须按 C proc 号落槽；Rust 侧映射表是 `BOOT_MODULE_PROC_NRS[i]`（`os/kernel/src/proc.rs:130`）。

**schedulable 判定**：内核 task + RS + VM 立即可调度；其他用户进程需等 RS 运行时设特权。

**格式与 multiboot 关系**：boot image 是 `kernel/table.c` 的 `struct boot_image image[NR_BOOT_PROCS]` C 全局数组（条目 `{ proc_nr, proc_name[16], endpoint, start_addr, len }`，**不是 ELF**，编进 kernel 二进制；Rust 侧对应 `KERNEL_TASKS`（proc.rs:117）+ `BOOT_MODULE_PROC_NRS` + `CapabilityTemplate`，见 §3.2/§3.4）。multiboot module list 则是 bootloader（GRUB/QEMU `-initrd`）提供的 ELF 物理地址范围——前者给"进程清单"，后者给"ELF 镜像在哪"，`init_proc_and_boot()` 按 `kernel_info.boot_modules[i]` ↔ `BOOT_MODULE_PROC_NRS[i]` 配对（见 §4.0）。

> **细节深读**：[`minix3/minix/kernel/table.c`](https://github.com/minix3/minix/blob/master/minix/kernel/table.c) 的 `image[]` 定义；[01-boot-shim-bootstrap.md §1.5](./01-boot-shim-bootstrap.md) multiboot 模块加载协议；[08-system-init-boot-finish.md §1.3](./08-system-init-boot-finish.md) 与 [09-vm-boot-protocol.md §1.2](./09-vm-boot-protocol.md) 的前后衔接；02-stage-vm 的 VM 侧视角。

#### 1.4.2 VM 的"开天辟地"问题

VM（Memory Manager）是 Minix3 微内核中负责管理地址空间的进程。每个用户进程的页表都由 VM 创建和维护。但 VM 自己也需要地址空间才能运行——而 VM 启动前，没有进程有自己的页表。

这是一个**鸡生蛋问题**：

```
VM 必须运行才能为其他进程创建页表
→ 但 VM 自己也需要地址空间
→ 没有"别人"能为 VM 创建页表
→ 内核必须亲自用 bootstrap 页表"抱"VM 起来
```

**解决方案**：内核手工用 **bootstrap 页表**为 VM 建立地址空间。bootstrap 页表包含三部分映射（跨三阶段生长）：

1. **恒等映射**（阶段 A/B）：物理地址 = 虚拟地址，让内核早期启动时代码能直接访问物理内存
2. **内核高半区映射**（阶段 B）：内核代码/数据映射到高地址空间（如 0xFFFFFFFF80000000 以上，**x86-64 canonical hole 上半区**，跨架构具体地址不同——aarch64 用 `-2GB` 偏移、riscv64 由 `linker.ld` 配置）
3. **VM 用户态映射**（阶段 C）：VM 的 ELF 段映射到用户态地址空间（如 0x40000000 以下，**x86-64 ELF default load address**，跨架构不同——aarch64/riscv64 由各自 toolchain 决定）

**通用 OS 设计模式**：第一个进程必须由内核手工创建，然后它才能创建更多进程。这是 OS 设计的普遍模式——Linux 的 init 进程、Minix3 的 VM 进程，都是这种"开天辟地"的第一个进程。

### 1.5 阶段 C 的执行节拍：从白板到就位但暂停

把前面概念串成执行顺序：**先清空，再填充**。

#### 1.5.1 第一步：清空（C 中对应 `proc_init()`）

- **清空进程表**：所有 slot 设 RTS_SLOT_FREE，设 p_nr 与初始 p_endpoint（generation=0）
- **清空特权表**：建立索引→槽映射（ppriv_addr[i] → &priv[i]）
- **初始化 IDLE 进程**：每 CPU 一个，共享 idle_priv，设 RTS_PROC_STOP（永远不可调度）
- **调用架构相关逻辑清零寄存器状态**（arch_proc_reset）

#### 1.5.2 第二步：填充（C 中对应 main.c 的 boot 循环 + `arch_boot_proc()`）

> **节拍描述边界**：本节拍需要标注每个步骤的具体调用（如 `arch_boot_proc`、`RTS_SET`、`SRV_Q`）以保证可追溯到 Ch2 源码分析。这些函数名/宏名是**节拍描述的锚点**，不是 Ch1 概念定义。

遍历 boot image，为每个 entry 填充 slot：

1. **取 proc_addr**：根据 p_nr 定位 slot
2. **同步 endpoint**：设 p_endpoint = _ENDPOINT(0, p_nr)
3. **复制名字**：strlcpy(p_name, image->proc_name)
4. **取 boot module**：bootmod(p_nr) 查 multiboot 模块
5. **reset_proc_accounting**：重置统计
6. **判断 schedulable**：iskerneln || isrootsysn || VM_PROC_NR
7. **schedulable 进程特权授予**：
   - VM：VM_F / SRV_T / SRV_M / SRV_KC / s_sig_mgr=SELF / SRV_Q / SRV_QT
   - 内核 task：IDL_F|TSK_F / TSK_I / CSK_T|TSK_T / TSK_M / TSK_KC
   - RS：RSYS_F / SRV_I / SRV_T / SRV_M / SRV_KC / SRV_SM / SRV_Q / SRV_QT
8. **p_priority/p_quantum_size_ms 覆写**：VM/RS → SRV_Q/SRV_QT；内核 task 不覆写；非 schedulable 用户进程保持 0
9. **非 schedulable**：RTS_SET(RTS_NO_PRIV | RTS_NO_QUANTUM)
10. **arch_boot_proc(ip, rp)**：VM 加载 ELF（其他进程跳过）
11. **非 VM 用户进程**：RTS_SET(RTS_VMINHIBIT | RTS_BOOTINHIBIT)
12. **所有进程**：RTS_SET(RTS_PROC_STOP)，清 RTS_SLOT_FREE

#### 1.5.3 为什么必须先清空再填充

填充时依赖 slot 编号与 endpoint 已就绪。如果先填充再清空，填充过程中访问的 slot 可能尚未初始化（p_nr/p_endpoint 未设），导致 bootmod 查找失败。

#### 1.5.4 阶段 C 结束状态

所有 boot 进程**就位但等待唤醒**——每个进程的 slot 已填好，但都带 RTS_PROC_STOP 标志，被刻意按住不让跑。唤醒发生在阶段 C 之后的 `bsp_finish_booting()`。

### 1.6 boot 流程的终点：唤醒与首次切换

**阶段 C 结束 ≠ 进程可运行**——所有 boot 进程都带着 RTS_PROC_STOP 被刻意按住。boot 流程的真正终点是后续阶段的 `bsp_finish_booting()`。

#### 1.6.1 唤醒动作

`bsp_finish_booting()` 依次做四类事：

1. **收尾准备**：CPU 识别、`vm_running = 0`、proc_ptr/bill_ptr 指向 IDLE、announce() 打印 banner
2. **唤醒**：遍历用户态 boot 进程清 RTS_PROC_STOP——RTS_UNSET 自动把它们加入调度队列（内核 task 保持 STOP，永不作为运行实体调度）
3. **平台初始化**：BSP 定时器初始化、FPU 使能、周期计费初始化、SMP 就绪标记
4. **交棒**：关闭 `kernel_may_alloc` 窗口 → `switch_to_user()` 切到第一个用户态进程（不返回）

逐行对照见 [08-system-init-boot-finish.md §4.6](./08-system-init-boot-finish.md)（Rust 流程展开见本文档 §4.8）。

#### 1.6.2 boot 期内存分配窗口

`kernel_may_alloc` 是全局标志：boot 期（VM 未启动）内核可直接分配物理内存，代 VM 履行内存管理职责；`bsp_finish_booting` 将其关闭，此后所有内存请求转给 VM。窗口关闭 = VM 正式接管内存管理。

### 1.7 本章小结

| 概念 | 本质 |
|------|------|
| 进程 | CPU 时间的"断点续传"——可暂停、可恢复的执行流 |
| 进程表 | 保存每个进程"断点"的存档数据结构 |
| CPU 四问 | 寄存器初值/入口点/栈和参数/地址空间 |
| 三件套 | 进程表（进程是谁）+ 特权表（能做什么）+ RTS 位图（现在能不能跑） |
| boot image | 编译时硬编码的进程清单（bootstrapping 的自举） |
| VM 鸡生蛋 | 第一个进程必须由内核手工创建（OS 设计普遍模式） |
| 阶段 C 节拍 | 先清空（proc_init）→ 再填充（boot 循环 + arch_boot_proc） |
| boot 流程终点 | bsp_finish_booting 唤醒 boot 进程 + 关闭内存分配窗口 + switch_to_user |

接下来的章节将深入 Minix3 源码（Ch2）、Rust 设计决策（Ch3）、具体实现（Ch4）、测试（Ch5），从概念落到代码。

---

## Ch2. C 源码分析：Minix3 数据结构与存储模型

> **本章职责**：讲清阶段 C 要操作的核心数据结构——进程表、特权表、RTS 位图、boot image——在 Minix3 中**如何存储**（存储形态、容量、索引方式、共享策略），并把字段**按 OS 语义分组**，说明每组在阶段 C 中承担的初始化动作。本章只写 C 侧（ground truth）；这些存储决策在 Rust 中如何重新表达见 Ch3；阶段 C 执行流程（清空→填充→加载→唤醒）的概念框架见 §1.5、Rust 实现见 Ch4。
>
> **组织方式**：以「分组」为单元，每组遵循固定模板：**字段 → OS 语义 → C 中的结构关系 → 阶段 C 中的作用 → 运行时细节引用**。字段可以全列（作为导航索引），但运行时用法不展开——它们属于后续功能文档（11/12/17/22 等，分工见 Ch6 职责边界矩阵）。

**C 源码锚点索引**（本章引用的函数与结构；boot 流程的逐步执行分析见 Ch4）：

| 函数 / 结构 | 位置 | 本章使用 |
|------------|------|---------|
| `struct proc`（60+ 字段，含嵌套） | `kernel/proc.h:22-137` | §2.1 |
| `struct priv`（31 字段） | `kernel/priv.h:21-66` | §2.2 |
| `proc[]` / `priv[]` / `ppriv_addr[]` 表定义 | `proc.h:283` / `priv.h:94-95` | §2.1.0 / §2.2.0 |
| `proc_init()` | `kernel/proc.c:119-159` | §2.1.0、§2.2.0（清空节拍执行者） |
| `arch_proc_reset()` | `arch/i386/arch_system.c:146-186` | §2.1.4 |
| `arch_proc_init()` | `arch/i386/memory.c:722-731` | §2.1.4 |
| `get_priv()` | `kernel/system.c:274-303` | §2.2.0 |
| `set_sendto_bit()` / `fill_sendto_mask()` | `kernel/system.c:307-330` / `349-359` | §2.2.3 |
| boot image 循环 | `kernel/main.c:164-271` | §2.1.6、§2.2.1、§2.2.3（填充节拍） |
| `arch_boot_proc()` | `arch/i386/protect.c:388-455` | §2.3 |
| `image[]` 清单 | `kernel/table.c:44-65` | §2.3 |

### 2.0 存储约束总述：为什么 Minix3 / minix-rs 的内核表是编译期定容的静态数组

阶段 C 的所有存储决策都由同一条**三层因果链**决定。这一节是全章总纲——后面每个「为什么这样存」都可以追溯到这三层。

**第一层：Minix3 模型本身是编译期固定容量。** 内核里不存在「表会增长」的概念——进程数、特权槽数在编译期就是常量：

| 常量 | 值 | 定义位置 | 含义 |
|------|-----|---------|------|
| `NR_TASKS` | 5 | `include/minix/com.h:56` | 内核 task 数（ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL，p_nr −5..−1） |
| `NR_PROCS` | 256 | `include/minix/sys_config.h:8`（经 `config.h:31`） | 用户进程槽上限（p_nr 0..255） |
| `NR_SYS_PROCS` | 64 | `include/minix/sys_config.h:9`（经 `config.h:32`） | 特权槽上限 |
| `NR_BOOT_PROCS` | `NR_TASKS + LAST_SPECIAL_PROC_NR + 1` = 17 | `include/minix/param.h:9` | boot image 条目数（5 task + 12 模块） |
| `NR_BOOT_MODULES` | `INIT_PROC_NR + 1` = 12 | `include/minix/com.h:74` | boot image 中用户态模块数 |

两个容量关系由编译期强制：`NR_BOOT_PROCS (17) ≤ NR_SYS_PROCS (64)`（`kernel/priv.h:101-102` 的 `#error` 检查——boot image 每个成员都必须能分到特权槽，否则内核拒绝编译）；进程表槽总数 `NR_TASKS + NR_PROCS` = 261（`kernel/proc.h:283`）。

**第二层：核心表的整个生命周期从 boot 前延续到关机，而 boot 期 Minix3 选择不使用堆分配器。** `proc_init()` 执行时 `kmain` 刚开始执行不久：物理内存清单尚未整理、VM 尚未启动。Minix3 在这个阶段主动不初始化堆分配器（`main.c:142` 的 `kernel_may_alloc = 1` 仅允许内核使用空闲内存段，不是动态分配）——Minix3 内核中没有 `malloc` / `kmalloc` / `slab` / `alloc_pages` 之类的分配器调用，唯一出现的位置是 `arch/i386/memory.c` 的 `freepdes` 数组预取。这意味着此阶段唯一能用的存储是编译期预留的静态内存。C 源码中 `proc[]`、`priv[]` 都是 `EXTERN` 数组（见 `proc.h:283` 的 `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`、`priv.h:94-95` 的 `EXTERN struct priv priv[NR_SYS_PROCS]` 与 `EXTERN struct priv *ppriv_addr[NR_SYS_PROCS]`），直接落在 BSS，内核从第一条指令起就能通过固定地址访问它们。这两张表的生命周期是「永远存在」（进程表此后被 fork/exit 反复复用直到关机），所以放在 BSS 同时满足「生命周期」「编译期地址」「零运行期构造」三重需求。

> **注：这不是技术唯一性，是 Minix3 的设计选择**。进程表采用编译期定容的静态数组，是因为 Minix3 内核在 boot 期不建立堆分配器（上文证据）。若想支持更灵活或更大的容量，理论上存在多种设计方向——例如动态分配进程表槽、或保留虚地址区段待运行时再填充。但这类改动改变的是内核的**存储模型**，属于 Rewrite / Refactor / Architectural Evolution 三级术语中的 **Architectural Evolution**，且超出「Rust 重写 Minix3、不发明新内核」的项目边界。**本文档不展开评估上述方向的工程细节**——仅指出它们存在；跨内核存储形态的论证与取舍见下表（Minix3 / minix-rs / Linux / Redox / seL4 对照）。

> **与 Rust `no_std` 的关系**：`no_std` ≠ 没有 allocator——`no_std` 只是不用标准库，`alloc` crate 可以提供堆。minix-rs 内核选择静态数组的真正原因是第二层：**kernel boot 阶段堆分配器尚未建立**（堆要等内存管理初始化之后才有可用资源，而进程表在堆可用之前就必须工作）。

> **运行期堆存在性限定**："boot 阶段尚未建立"只是时序表述；约束的完整边界是**内核全生命周期不建堆**——本节上文"C 内核中没有 `malloc`/`kmalloc`/`slab`/`alloc_pages` 之类的分配器调用"覆盖的是整个生命周期而非仅 boot 期，minix-rs 沿用该存储模型。`minix-kernel` 最终链接为 kernel image（binary 入口，`#![cfg_attr(not(test), no_main)]`，`os/kernel/src/lib.rs:12`）；当前 Cargo.toml 用 `[lib]` 形态发布是为了让 `cargo test` 直接以 libtest 入口复用同一份源码——这与"自身不注册 `#[global_allocator]`"是两件事，后者是设计纪律：生产构建无 `global_allocator`，`alloc` 仅在 `#[cfg(test)]` 链接。裸机构建与该约束一致：boot-shim 的唯一分配器是 UEFI boot services 期的 `uefi::allocator::Allocator`（`os/boot-shim/src/main.rs:31`），`ExitBootServices` 后失效；`minix-kernel` 不引入 UEFI/OpenSBI 分配器，仅引用 `alloc` 让测试代码可用 `BTreeMap`/`Vec` 等容器做白盒验证。测试构建豁免：宿主 `cargo test` 走 `std::alloc::System`（`os/boot-shim/src/lib.rs:26`，`#[cfg(test)]`），qemu 测试内核各自注册分配器（如 `os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/src/main.rs:46`）。

**Kernel 零堆存储形态**：进程表用 `static [KProcess; PROC_TABLE_SIZE]`（静态侵入链节点，`caller_q_head/tail` + `send_q_link` 槽索引）；特权表同形态；闹钟定时器用 `KPriv::runtime.s_alarm_timer: AlarmTimerNode`（内嵌节点）+ `ClockState.timers_head: Option<PrivId>`（链头）；SENDA 不缓存表，由 priv 缓存 `s_asyntab/s_asynsize/s_asynendpoint`（C: priv.h:28，proc.c:1320-1323），`mini_senda` 逐条读用户表；panic 消息走 `page_fault.rs` 的 `FmtBuf` 栈缓冲。所有运行期结构均为 C 同构的索引式侵入链 / 静态数组 / 栈缓冲，零堆分配。

**第三层：因此 Rust 表达 `[T; N]` 是自然选择，不是妥协。** Minix3 的 BSS 静态数组在 Rust 中一一对应——本节作为「存储约束总纲」，仅列被第一层（编译期定容）+ 第二层（boot 期零堆）直接决定的**两张核心表**：

| 核心表 | C 形态 | Rust 形态 | 详细分组 |
|-------|--------|----------|---------|
| 进程表 | `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`（proc.h:283，BSS） | `static PROC_TABLE`（BSS，lib.rs:1459），内部 `[KProcess; NR_TASKS + NR_PROCS]`（proc_table.rs:57） | §2.1（含 slot 定位、RTS、就绪队列等） |
| 特权表 | `EXTERN struct priv priv[NR_SYS_PROCS]`（priv.h:94） | `static PRIV_TABLE`（BSS，lib.rs:1464），内部 `[KPriv; NR_SYS_PROCS]`（kpriv.rs:781） | §2.2（含特权槽定位、用户进程特权共享等） |

> **本表不列的存储决策**：slot 定位宏（`proc_addr(n)`）、RTS 位图（`p_rts_flags`）、就绪队列链接（`p_nextready`）、特权槽与进程的映射（`ppriv_addr[]`）、用户进程特权共享（`USER_PRIV_ID`）等——这些都是**具体字段的存储与寻址方式**，由对应 §2.1.x / §2.2.x 分组展开。本总纲只回答「为什么是静态数组」，不替后续分组章节抢具体字段的存储与寻址细节。RTS **不是独立表**——它是 `struct proc` 内的 `p_rts_flags` 字段，详见 §2.1.6 生命周期控制组。

> **Rust 设计的论证归属**：上表 Rust 形态仅标注「存储位置 + 行号」，类型选择 / 同步原语 / newtype 抽象的论证在 §3.1–§3.10 展开，本节不重复。

**第四层（跨架构共性）**：C 把架构相关内容平铺在 `struct proc` 内（`p_reg` 全套寄存器、`p_seg` 段选择子 + FPU 缓冲指针）；minix-rs 的 `KProcess`（proc.rs:855）把 OS 层字段（标识/调度/IPC/记账）与架构私有内容分离——后者收敛为两个不透明字段：`cpu_context: CurrentCpuContext`（proc.rs:972，内核层不读其内部字段）与 `fpu_state: CurrentFpuState`（proc.rs:990）。OS 语义因此在三个架构间只有一份定义，架构差异被隔离在 arch crate 内（trait 设计见 §3.5）。

> **架构范围说明**（避免把「编译期定容」误读为内核通用规律）：本节论证的三层因果链（编译期固定容量 → boot 期零堆 → `[T; N]` 自然表达）只在 **Minix3 / minix-rs 模型下**成立——这是 Minix3 的设计选择，**不是内核通用规律**。其他内核的存储形态各有不同：
>
> | 系统 | 进程表存储 | 容量 | 堆分配器状态 |
> |------|-----------|------|-------------|
> | **Minix3** | `EXTERN struct proc proc[NR_TASKS+NR_PROCS]`（BSS 静态数组） | **编译期定容**（256 槽） | boot 期无堆 |
> | **minix-rs**（本项目） | `static PROC_TABLE`（BSS，lib.rs:1459），内部 `[KProcess; PROC_TABLE_SIZE]`（proc_table.rs:58，`PROC_TABLE_SIZE = NR_TASKS + NR_PROCS`，即 261） | **继承 Minix3** | 继承 Minix3 |
> | **Linux** | `struct task_struct` 链表 + slab cache（`alloc_task_struct_node()`） | **运行时动态分配** | boot 早期 bootmem → mm_init 后切换 buddy system |
> | **Redox** | `Vec<Box<Process>>` + ralloc | **运行时动态分配** | boot 早期静态分配器，runtime 切 ralloc |
> | **seL4** | 全静态数组 + capability 表 | **编译期定容** | 完全不用堆，用 typed memory + untyped capability 池 |
>
> 「编译期定容」是 Minix3 / seL4 这类**继承自早期微内核设计**的选择，不是所有内核的**必经之路**。Linux/Redox 用动态分配处理**「进程数不确定」**的真实需求——这是另一种合理选择，只是模型不同。读者若接触过 Linux 内核源码，看到「编译期定容」不要误以为是通用规律。

> **为什么 Minix3 / minix-rs 主动接受这个限制**（非"没能力改进"的辩护）：第一层（编译期固定容量）是**主动设计选择**而非被迫——Minix3 用定容换取：
> - **简单性**——进程表 = 普通数组，不需要 IDR / pidfd 这类 id 分配器；
> - **形式化验证可行性**——seL4 正是基于全静态结构做了完整形式化证明（动态分配难以验证）；
> - **可预测性**——内存占用编译期确定，无 fork OOM；
> - **教学价值**——学生能在脑中跑通调度模型。
>
> 「改为动态分配 proc 表」属于**架构演进**（Rewrite / Refactor / Architectural Evolution 三级术语中的最高一级）——是结构性变化，不是 minix-rs（Rewrite 范畴）的任务范围。如果未来要朝这个方向演进，需独立项目（minix-rs-evolved）。**minix-rs 的目标是 Rust 重写 Minix3，不是发明新内核**。
>
> 与 direct map 的对比：direct map 是「减负式 redesign」——消除 freepdes / ptproc / map_page 等机制，降低复杂度；「动态分配 proc 表」是「加负式 redesign」——需要 id 分配器、退出时的资源回收、OOM 处理、扩容同步保护。**两者方向相反**——direct map 让内核更简单，动态分配让内核更复杂。两种都是合理选择。

> **「256」的精确含义**——「kernel `proc[]` 261 槽」是**全系统进程身份的刚性上限**（任何"活的进程"必然落进 kernel 表）。这里要解释的真正误区不是"kernel 是不是全系统上限"——**它就是上限**——而是另一件事：读者可能误以为"所有 server 的进程表**必须**用静态数组，静态数组是全系统的'解决方案'"。实际是：
>
> - **静态数组的强制使用**只发生在 **kernel `proc[]`** 和 **VM `VmProcTable`**——它们都在**堆分配器客观上还未建立**的时序点被迫建立：kernel `proc_init()` 跑在 kmain 早期（`minix3/minix/kernel/main.c:157`，堆还未建立）；VM `VmProcTable` 是 BSS `static VM_PROC_TABLE`（`os/servers/vm/src/vmproc/table.rs:59`），VM 早期 `memset(vmproc, 0)` 时其自身 page allocator 也未建。两者的**客观时序约束**完全相同。
> - **PM/VFS/DS 等后启动的 server**——它们在 VM `exec_bootproc` 拉起时（VM 自身 page allocator 已建好）才启动，**此时堆已可用**。所以它们**无所谓静态/动态**——有能力用 `Vec<1024+>`，也有能力用 `mproc[NR_PROCS+1]` 静态数组。**minix-rs 继承 Minix3 的形态选了静态数组**，**不是因为"必须"**，而是因为既然 kernel 表已经锁死了 261（再大的容器永远装不满——超过 261 个的进程身份根本不存在），用动态容器是**有能力、但没必要的**。
>
> 所以准确的措辞是：**「kernel 261」是全系统进程身份的刚性上限**——任何动态容器扩展到 1024+ 在该系统中**注定装不满**。server 用静态数组是 minix-rs 对上游 Minix3 形态的一一对应继承，**不是全系统的"统一约束"**——其他 server 用动态数组也是合理的，只是不必要。

> 本节之后的分组均只写 C 侧；表中 Rust 形态在 Ch3 逐项展开论证。

### 2.1 进程表存储（C 侧）

#### 2.1.0 整体：proc[261] 静态数组与「身份即索引」

进程表本体（proc.h:283）：

```c
EXTERN struct proc proc[NR_TASKS + NR_PROCS];	/* process table */
```

配套的定位宏（proc.h:265-279）把「进程号」直接翻译成数组下标：

```c
#define BEG_PROC_ADDR (&proc[0])
#define END_PROC_ADDR (&proc[NR_TASKS + NR_PROCS])
#define proc_addr(n)      (&(proc[NR_TASKS + (n)]))   /* p_nr → slot 指针，O(1) */
#define proc_nr(p)        ((p)->p_nr)
#define iskerneln(n)      ((n) < 0)                   /* 负号 = 内核 task */
#define isrootsysn(n)     ((n) == ROOT_SYS_PROC_NR)   /* RS = 2 */
```

**OS 语义**：这是 §2.0 第一层「身份即索引」的落地——`p_nr` 不是「在表里搜出来的键」，而是「slot 在数组中的位置偏移」（负偏移给内核 task）。`proc_addr(n)` 一次乘加完成寻址，这是 C 静态数组方案的基石，也是「容量必须编译期确定」的直接受益者。

**阶段 C 中的作用**：`proc_init()` 第一遍循环（proc.c:129-140）把 261 个 slot 逐个从「BSS 零状态」转成「已宣布的空槽」：

```c
for (rp = BEG_PROC_ADDR, i = -NR_TASKS; rp < END_PROC_ADDR; ++rp, ++i) {
    rp->p_rts_flags = RTS_SLOT_FREE;          /* 标记 slot 空闲 */
    rp->p_magic = PMAGIC;                     /* 完整性哨兵（const.h:164，0xC0FFEE1） */
    rp->p_nr = i;                             /* 进程号 = slot 索引偏移（-5..255） */
    rp->p_endpoint = _ENDPOINT(0, rp->p_nr);  /* generation=0 的初始端点 */
    rp->p_scheduler = NULL;
    rp->p_priority = 0;
    rp->p_quantum_size_ms = 0;
    arch_proc_reset(rp);                      /* 架构相关清零（§2.1.4） */
}
```

**为什么必须显式清空、不能依赖 BSS 零值**：BSS 零值只在第一次 boot 时成立；这张表随后会被 fork/exit 反复复用，上一任 slot 的残留字段必须重置。「本槽空闲」的权威标记就是 `p_rts_flags == RTS_SLOT_FREE`（判空宏 `isemptyp`，proc.h:273-274）。此后每个 slot 的字段由下面 6 个分组承载；boot 循环（main.c:164-271）再从中挑出 boot image 成员完成填充（§2.3）。

#### 2.1.1 标识组：进程是谁、谁能找到它

字段：`p_nr`（proc.h:25）/ `p_endpoint`（proc.h:82）/ `p_name[PROC_NAME_LEN=16]`（proc.h:80；type.h:145）/ `p_magic`（proc.h:127）。

**OS 语义**：内核需要一个稳定、O(1) 可寻址的标识，把「进程」与系统消息、调度、信号关联起来；同时要防止 slot 复用产生的「陈旧引用」。

**C 中的结构关系**：
- `p_nr`：slot 索引偏移（−5..255），`proc_addr(n)` 的键。
- `p_endpoint`：generation 化身份 `_ENDPOINT(generation, p_nr)`。slot 退出后被复用时 `p_nr` 相同而 generation 不同；A 若持旧 endpoint 向已退出的 B 发消息，会被 generation 不匹配挡住。初始 generation=0（proc.c:133）。
- `p_magic`：slot 完整性哨兵，`proc_ptr_ok(p)` 校验（proc.h:174）。

**阶段 C 中的作用**：`proc_init` 重排全部 261 个 `p_nr` 并写 generation=0 端点；boot 循环把 image 条目的端点同步回 `ip->endpoint`（main.c:174），让 image 数组记录「这条目最终拿到什么端点」。进程名只有 task 在 boot 期写入（`strlcpy`，main.c:177），用户态模块的名字由各自启动后自报。

**运行时细节**（endpoint 代际何时递增、fork 子进程如何继承）：详见 [17-syscall-process.md](./17-syscall-process.md)。

#### 2.1.2 调度属性组：什么时候跑、跑多久、在哪个 CPU

字段：`p_priority`（proc.h:30，char）/ `p_quantum_size_ms`（proc.h:32）/ `p_cpu_time_left`（proc.h:31）/ `p_nextready`（proc.h:72）/ `p_scheduler`（proc.h:34）/ `p_cpu`（proc.h:35）/ SMP 追加 `p_cpu_mask`、`p_stale_tlb`（proc.h:36-45，`#ifdef CONFIG_SMP`）。

**OS 语义**：描述进程的调度身份——优先级、剩余时间片、就绪队列链接、可运行的 CPU 集合。

**C 中的结构关系**：`p_nextready` 是**侵入式就绪链表**的 next 指针（同优先级就绪进程串成一条链，链头存放在调度队列数组中，本字段只负责「链上下一个是谁」）；`p_scheduler` 指向时间片用尽时应通知的外部调度器（boot 期为 NULL）。

**阶段 C 中的作用**：第一遍循环全部置零/NULL（proc.c:134-136）；boot 循环对每个 boot 进程设 `p_cpu_time_left = 0`（main.c:175）；VM 与 RS 覆写 `p_priority = SRV_Q`、`p_quantum_size_ms = SRV_QT`（main.c:209-210、232-233），内核 task 保持 0。阶段 C 不做任何队列操作——这组字段在阶段 C 只是「等待运行时使用的初值」。

**运行时细节**（enqueue/dequeue 如何维护 p_nextready、pick_proc 如何选进程）：详见 [11-scheduling-primitives.md](./11-scheduling-primitives.md)。

#### 2.1.3 IPC 状态组：在等谁、谁在等我、消息怎么投递

字段：`p_getfrom_e`（proc.h:75）/ `p_sendto_e`（proc.h:76）/ `p_caller_q`（proc.h:73）/ `p_q_link`（proc.h:74）/ `p_sendmsg`（proc.h:84）/ `p_delivermsg`（proc.h:85）/ `p_delivermsg_vir`（proc.h:86）/ `p_pending`（proc.h:78，内核信号挂起位图）。

**OS 语义**：进程在 IPC 中的挂起状态与消息缓冲——RECEIVE 时等谁（p_getfrom_e）、SEND 时发给谁（p_sendto_e）、谁排在我的发送者队列里（p_caller_q/p_q_link）、消息暂存（p_sendmsg/p_delivermsg/p_delivermsg_vir）；`p_pending` 是内核信号投递的进程侧簿记。

**C 中的结构关系**：发送者队列同样是侵入式链——`p_caller_q` 是队列头，`p_q_link` 串起下一个想发给本进程的进程；`p_delivermsg` 配合 `p_misc_flags` 的 MF_DELIVERMSG 位表达「消息已就绪待拷贝」。

**阶段 C 中的作用**：无——`proc_init` 不触碰这些字段，它们保持 BSS 零值。这不是遗漏：boot 期没有任何 IPC 发生，「零值 = 无 IPC 挂起」恰好是正确初态。列出它们是因为阶段 C 之后的每一次 IPC 都在这组字段上留下状态（RECEIVING/SENDING 的 RTS 位是它们的状态机镜像，见 §2.1.6）。

**运行时细节**（六原语、SENDING/RECEIVING 状态机、delivermsg 延迟拷贝）：详见 [12-ipc-core.md](./12-ipc-core.md)。

#### 2.1.4 执行上下文组：被暂停时 CPU 长什么样

字段：`p_reg`（proc.h:23，`struct stackframe_s` 全套用户寄存器）/ `p_seg`（proc.h:24，`struct segframe` 段描述符）/ `p_vmrequest`（proc.h:95-124，VM 挂起请求保存区）。

**OS 语义**：进程被切换出去那一刻 CPU 的完整现场；以及进程因缺页等内存事件被挂起、等 VM 修复时，请求参数与结果的暂存处。

**C 中的结构关系**：
- `p_reg`：x86 的 stackframe_s 含通用寄存器、段选择子（cs/ds/es/fs/gs/ss）、PC/SP/PSW。
- `p_seg`：x86 侧存段选择子与 `fpu_state` 指针（指向 per-process FPU 缓冲）；ARM 无段机制，结构退化。
- `p_vmrequest`：嵌套结构——`nextrestart`/`nextrequestor` 两条挂起链、请求类型（VMSTYPE_KERNELCALL/DELIVERMSG/MAP）、保存的请求消息、参数与 VM 结果（proc.h:95-124）。

**阶段 C 中的作用**——这是 boot 进程「第一次能被 CPU 执行」的关键：

1. `arch_proc_reset()`（arch_system.c:146-182，第一遍循环 per slot 调用）：清零 `p_reg`、按内核/用户身份设初值 PSW（x86：INIT_TASK_PSW=0x1200 / INIT_PSW=0x0200，archconst.h:118-119）、为用户进程分配并清零 FPU 缓冲（`fpu_state[p_nr]`，FPUALIGN 对齐）、设用户段选择子（USER_CS_SELECTOR/USER_DS_SELECTOR）。三架构状态寄存器/入口/栈/ps_strings 寄存器对照见 §1.2.3（C 端只有 i386 与 32-bit ARM 两个实现，earm 无 per-process FPU 缓冲）。
2. `arch_proc_init()`（memory.c:722-731，仅 VM 调用）：重新 reset 后写入入口三要素——`p_reg.pc = ip`、`p_reg.sp = sp`、`p_reg.bx = ps_strings`（x86 用 bx 传 ps_strings；aarch32 用 retreg）。其余用户 boot 进程在阶段 C 结束时保持「寄存器零值 + VMINHIBIT」的占位状态，其真正的上下文要等 RS 运行时 exec 才重建（→ 17）。
3. `p_vmrequest` 保持零值——阶段 C 没有 VM 请求。

**运行时细节**（switch_to_user 恢复现场、VM 请求协议、FPU 保存策略）：详见 [10-switch-to-user.md](./10-switch-to-user.md)、[09-vm-boot-protocol.md](./09-vm-boot-protocol.md)、[31-fpu-context-switching.md](./31-fpu-context-switching.md)。

#### 2.1.5 记账/统计组：给调度员看的账（导航归类）

字段：`p_accounting{enter_queue, time_in_queue, dequeues, ipc_sync, ipc_async, preempted}`（proc.h:48-55）/ `p_dequeued`（proc.h:57）/ `p_user_time` / `p_sys_time`（proc.h:59-60）/ `p_virt_left` / `p_prof_left`（proc.h:62-63）/ `p_cycles` / `p_kcall_cycles` / `p_kipc_cycles`（proc.h:65-67）/ `p_tick_cycles`（proc.h:69）/ `p_cpuavg`（proc.h:70）。

**OS 语义**：调度记账——进程占用 CPU/队列/IPC 的量化记录，供调度决策与 ps(1) 展示。

**阶段 C 中的作用**：boot 循环对每个 boot 进程调用 `reset_proc_accounting()`（main.c:186），保证账本从零开始；其余字段保持 BSS 零值。本组不逐字段展开——阶段 C 只需保证「账本是零」。

**运行时细节**（sched_proc 参数更新、周期计费）：详见 [11-scheduling-primitives.md](./11-scheduling-primitives.md)、[17-syscall-process.md](./17-syscall-process.md)。

#### 2.1.6 生命周期控制组：现在能不能跑（RTS 16 位全集）+ 有什么权限

字段：`p_rts_flags`（proc.h:27，`volatile u32_t` 位图）/ `p_misc_flags`（proc.h:28）/ `p_priv`（proc.h:26，指向所属特权槽）。

**OS 语义**：`p_rts_flags` 回答「进程现在能不能跑」，`p_priv` 回答「进程被允许做什么」，`p_misc_flags` 承载不挂起进程的控制位（如 MF_DELIVERMSG）。

**不变量**：`p_rts_flags == 0 ⟺ runnable`。进程可同时因多个原因不可运行（等 VM 建页表 + 被停止），所以是位图而非枚举。

**RTS 16 位全集**（proc.h:142-166；0x2000 位未使用）：

| 位 | 值 | 含义 |
|----|-----|------|
| `RTS_SLOT_FREE` | 0x0001 | slot 空闲 |
| `RTS_PROC_STOP` | 0x0002 | 进程已被停止 |
| `RTS_SENDING` | 0x0004 | 阻塞在发送 |
| `RTS_RECEIVING` | 0x0008 | 阻塞在接收 |
| `RTS_SIGNALED` | 0x0010 | 新内核信号到达 |
| `RTS_SIG_PENDING` | 0x0020 | 信号处理中暂时不可运行 |
| `RTS_P_STOP` | 0x0040 | 进程被 trace 停止 |
| `RTS_NO_PRIV` | 0x0080 | 无特权（fork 出的系统进程暂不可跑） |
| `RTS_NO_ENDPOINT` | 0x0100 | 无端点，不能收发消息 |
| `RTS_VMINHIBIT` | 0x0200 | 等 VM 建好页表 |
| `RTS_PAGEFAULT` | 0x0400 | 有未处理缺页 |
| `RTS_VMREQUEST` | 0x0800 | 是 VM 内存请求的发起方（挂起中） |
| `RTS_VMREQTARGET` | 0x1000 | 是 VM 内存请求的目标 |
| `RTS_PREEMPTED` | 0x4000 | 被更高优先级抢占 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片耗尽 |
| `RTS_BOOTINHIBIT` | 0x10000 | 等 boot 流程完成（VM 建好页表后仍要等全局启动结束） |

**阶段 C 的位设置时机**（boot 进程从表中诞生到被唤醒前的完整履历——每位「为什么此时必须置上」）：

| 时机 | 进程 | 源码动作 | 为什么 |
|------|------|---------|--------|
| `proc_init` 第一个循环（`minix3/minix/kernel/proc.c:129-140`） | 261 个 slot 全遍历 | `p_rts_flags = RTS_SLOT_FREE`（L130） | 宣告空槽 |
| `proc_init` 第二个循环（`minix3/minix/kernel/proc.c:151-158`） | IDLE（每 CPU 一个） | `p_rts_flags \|= RTS_PROC_STOP`（L156，**永不清除**） | IDLE 是调度器的最后手段，只在无事可做时被选中 |
| boot 循环，**非 schedulable 分支**（`minix3/minix/kernel/main.c:251-254`） | 12 个 module 中**非** RS/VM 的 10 个（DS/PM/SCHED/VFS/MEM/TTY/MIB/PFS/MFS/INIT） | `RTS_SET(NO_PRIV \| NO_QUANTUM)`（L253） | 特权与时间片尚未授予，等 RS 运行时通过 PrivCtl 授予 |
| boot 循环，**非 VM 用户进程**（`minix3/minix/kernel/main.c:264-267`） | 同上 10 个 + RS（**共 11 个**，kernel task 与 VM 都不满足 `rp->p_nr >= 0` 且 `!= VM_PROC_NR`） | `\|= RTS_VMINHIBIT \| RTS_BOOTINHIBIT`（L265-266） | **两个位清除时机不同**：`RTS_VMINHIBIT` 由 VM 经 vmctl `VMCTL_VMINHIBIT_CLEAR`（`system/do_vmctl.c:143`）清除（VM 为该进程建好页表时，per-process）；`RTS_BOOTINHIBIT` 由 VM 经 vmctl `VMCTL_BOOTINHIBIT_CLEAR`（`system/do_vmctl.c:167`）清除（bsp_finish_booting 完成后，全局通知）。通常 VMINHIBIT 先（per-process），BOOTINHIBIT 后（全局），但**两者是独立动作**，不是同一阶段。 |
| boot 循环尾部，**对全部 17 个 boot 进程**（`minix3/minix/kernel/main.c:269-270`） | 17 个 boot 进程（含 IDLE？不——见下行） | `\|= RTS_PROC_STOP`；`&= ~RTS_SLOT_FREE`（L269-270） | 从「空槽」转正为「已占用但暂停」，等统一唤醒 |
| `bsp_finish_booting` 入队（`minix3/minix/kernel/main.c:64-66`） | `for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++)`——遍历 12 个 module（**包括 VM、RS**），**不**遍历 5 个 kernel task，**不**遍历 IDLE | **只** `RTS_UNSET(proc_addr(i), RTS_PROC_STOP)`（L65） | 入调度视野；**此函数不动 `RTS_VMINHIBIT` / `RTS_BOOTINHIBIT`**——这两个位由 VM 在自己初始化完成后通过 vmctl `VMCTL_VMINHIBIT_CLEAR`（`do_vmctl.c:143`）和 `VMCTL_BOOTINHIBIT_CLEAR`（`do_vmctl.c:167`）分别清除（VMINHIBIT per-process，BOOTINHIBIT 全局） |
| IDLE slot | 每 CPU 一个 | **永不清除** PROC_STOP | 调度器最后手段 |

`RTS_SET`/`RTS_UNSET` 宏在置位/清位的同时联动就绪队列（清到 0 时入队）——联动细节属调度器，06 只锁不变量。`p_priv` 的赋值发生在 boot 循环的 `get_priv()`（§2.2.0）；IDLE 例外地指向共享的 `idle_priv`（proc.c:154）。

**运行时细节**（不变量与队列一致性、每位由谁置/清）：详见 [11-scheduling-primitives.md](./11-scheduling-primitives.md)；`p_priv` 语义见 [22-privilege.md](./22-privilege.md)；MF_* 位见 [12-ipc-core.md](./12-ipc-core.md)。

### 2.2 特权表存储（C 侧）

#### 2.2.0 整体：priv[64] 实体表 + ppriv_addr[64] 桥接表 + USER_PRIV 分治

进程表有 261 个槽，特权表只有 64 个——这两张表容量不对等是刻意的**分治**设计（priv.h:7-9 注释明言 "very space efficient"）：每个进程都需要一个进程槽，但**只有系统进程需要独立的特权结构**，普通用户进程全部共享一份 `USER_PRIV`。`struct priv` 有 31 个字段（priv.h:21-66），若按进程平铺就是 31 × 数百份；分治后是 31 × 64 份 + 一份共享。共享槽的定义本身就是这个设计的注脚：

```c
#define USER_PRIV_ID	static_priv_id(ROOT_USR_PROC_NR)   /* priv.h:18 */
#define static_priv_id(n)          (NR_TASKS + (n))        /* priv.h:12 */
#define ROOT_USR_PROC_NR  INIT_PROC_NR                     /* com.h:78 */
```

展开 `USER_PRIV_ID` 的宏（按代码块逐级）：

```
USER_PRIV_ID
  = static_priv_id(ROOT_USR_PROC_NR)        /* priv.h:18 */
  = (NR_TASKS + (ROOT_USR_PROC_NR))           /* priv.h:12 展开 static_priv_id(n) */
  = (NR_TASKS + (INIT_PROC_NR))              /* com.h:78 展开 ROOT_USR_PROC_NR */
  = (5 + 11)                                 /* NR_TASKS=5、INIT_PROC_NR=LAST_SPECIAL_PROC_NR=11 */
  = 16
```

`NR_STATIC_PRIV_IDS = NR_BOOT_PROCS = 17`（`minix3/minix/include/minix/priv.h:10`），**静态特权槽段索引范围是 `[0, 17)`**——所以 `USER_PRIV_ID = 16` 正是**静态段的最后一个槽**。**「根用户进程」INIT 的特权槽就是全体用户进程的共享槽**（fork 子进程在 `minix3/minix/kernel/system/do_fork.c:105` 被赋给 `priv_addr(USER_PRIV_ID)`）。

但**整个 `priv[]` 表大小是 `NR_SYS_PROCS = 64`**（不是 17）——前 17 个槽是**静态段**（boot 时分配），后 47 个是**动态段**（运行时分配给动态系统服务）。`USER_PRIV_ID = 16` 是**静态段内**的最后，不是**整张表**的最后。

存储本体（`priv.h:94-95`）：

```c
EXTERN struct priv priv[NR_SYS_PROCS];		/* system properties table（大小 64） */
EXTERN struct priv *ppriv_addr[NR_SYS_PROCS];	/* direct slot pointers（大小 64） */
```

**术语 `sys_id` / `sys_id_t`**（`minix3/minix/kernel/type.h:10`）：

> `typedef short sys_id_t;` —— 16 位整型，含义是「特权槽在 `priv[]` 数组中的下标」。对应 `struct priv::s_id` 字段（`minix3/minix/kernel/priv.h:23`）。

与 `proc_nr` 的区别：`proc_nr` 是进程在 `proc[]` 数组中的下标（`-5..255`，区分 kernel task 与用户进程）；`sys_id` 是特权槽在 `priv[]` 数组中的下标（`0..63`）。两者是**独立索引空间**——一个 proc_nr 对应一个 `s_id`，但**同一个 `s_id` 可被多个进程共享**（最典型：`USER_PRIV_ID = 16` 被所有用户进程共享）。

**双表索引结构**：

- **实体表 `priv[]`**：连续存储 64 个 `struct priv`（每个 ~80 字节，总计约 5 KB）
- **指针桥接表 `ppriv_addr[]`**：64 个 `struct priv *` 指针，每个指向 `priv[]` 对应槽位（64 位系统占 512 字节）

**为什么需要桥接表**（`minix3/minix/kernel/priv.h:89-92`）：

如果只有 `priv[]`，用 sys_id 找槽指针要做指针算术：`&priv[i] = priv + i * sizeof(struct priv)`。在 Minix3 早期（2009 年版本），编译器对编译期已知的 `sizeof(struct priv)` 还不一定能保证优化为移位 + 加法——也就是每次 IPC 路径要走一次乘法。桥接表把"索引→指针"变成"查表→解引用一次"：`priv_addr(i) = ppriv_addr[i]` 直接读出槽指针——这是**用 512 字节 BSS 空间换 IPC 路径消除一次乘法**的显式微优化。

**现代编译器下已基本无价值**：GCC/Clang 自 2010 年代起对 `EXTERN struct priv priv[]` 的 `&priv[i]` 会直接生成 LEA + 移位指令（步长编译期已知），无需桥接表。**minix-rs Rust 端已经抛弃桥接表**——`PrivTable::get(id)`（`os/kernel/src/kpriv.rs:801-808`）直接 `&self.privs[idx]`，Rust 编译器生成最优寻址（无乘法）。所以这个桥接表是**Minix3 C 时代的历史包袱**，Rust 重写时自然消失。

**寻址宏族**（`minix3/minix/kernel/priv.h:79-84`）——围绕 sys_id 与 proc_nr 的双向翻译：

| 宏 | 输入 | 输出 | 用途 |
|---|------|------|------|
| `priv_addr(id)` | `sys_id` | `struct priv *` | **id → 槽指针**（核心查询） |
| `priv(rp)` | `proc` | `struct priv *` | **进程 → 槽指针**（经 `rp->p_priv` 指针） |
| `priv_id(rp)` | `proc` | `sys_id` | **进程 → 槽 id** |
| `id_to_nr(id)` | `sys_id` | `proc_nr` | **id → 关联的进程号** |
| `nr_to_id(nr)` | `proc_nr` | `sys_id` | **进程号 → 槽 id** |

**索引空间独立性**：

`priv[]` / `ppriv_addr[]` 按 `sys_id`（[0, 64)）索引，`proc[]` 按 `proc_nr`（[-5, 256)）索引——**两个 ID 空间是独立的**。一个 `sys_id` 可以被**多个进程共享**（典型：`USER_PRIV_ID = 16` 被所有用户进程共享，`fork()` 出来的子进程在 `minix3/minix/kernel/system/do_fork.c:105` 直接赋给 `priv_addr(USER_PRIV_ID)`）。也就是说，**`priv[]` 表的 64 个槽位是"特权身份池"，`proc[]` 表的 256 个槽位是"进程身份池"**——前者是后者的"多对一共享关系"。

**零堆的存储形态**：两张表都是 `EXTERN` BSS 静态数组（§2.0 第二层），编译期定容；priv.h:101-103 的 `#error` 保证 `NR_BOOT_PROCS (17) ≤ NR_SYS_PROCS (64)`——boot image 每个成员都必须能分到特权槽，否则内核拒绝编译。

**静态段与动态段**：特权槽按分配时机分两段（priv.h:74-77 的地址宏以 `NR_STATIC_PRIV_IDS` 为界；`include/minix/priv.h:10` 定义 `NR_STATIC_PRIV_IDS = NR_BOOT_PROCS`）：

- **静态段 `[0, 17)`**：boot 期由 `get_priv()` 以静态 id 分配（§2.2.1）；
- **动态段 `[17, 64)`**：运行时 `get_priv(rp, NULL_PRIV_ID)` 线性扫描 `s_proc_nr == NONE` 的空闲槽（system.c:274-303），耗尽返回 `ENOSPC`；静态路径上 id 越界返回 `EINVAL`、槽已被占用返回 `EBUSY`。分配成功即绑定：`rc->p_priv = sp; rc->p_priv->s_proc_nr = proc_nr(rc)`——进程表到特权槽的连接就在这一步建立。

**C 6 组 → Rust 8 子结构**：本节把 31 个字段按职责分为 6 组（下表导航）；Rust 端把「身份/能力组」按绑定/能力/init 三种读写时机拆成 3 个子结构，并把「运行时组」的异步发送表三字段并入信号簿记子结构，成为 8 子结构（22 §4.2；设计论证见 Ch3 §3.2/§3.10）。

| 分组 | 字段（含 priv.h 行号） | 完成的 OS 语义 | 阶段 C 动作 | 后续文档 |
|------|----------------------|--------------|------------|---------|
| 身份/能力（§2.2.1） | `s_proc_nr` / `s_id` / `s_flags` / `s_init_flags`（22-25） | 槽属于谁、什么角色 | boot 循环静态授予（仅 schedulable 三类） | 22 |
| 信号（§2.2.2） | `s_sig_mgr` / `s_bak_sig_mgr` / `s_notify_pending` / `s_asyn_pending` / `s_int_pending` / `s_sig_pending`（40-45） | 信号管理器与挂起簿记 | 仅 VM/RS 写 `s_sig_mgr` | 19 |
| IPC（§2.2.3） | `s_trap_mask` / `s_ipc_to` / `s_k_call_mask`（34-38） | 三层能力掩码 | 角色常量 + `fill_sendto_mask` | 22/23/12 |
| I/O（§2.2.4） | `s_nr_io_range` / `s_io_tab` / `s_nr_irq` / `s_irq_tab`（53-60） | 端口/IRQ 白名单 | 不触碰（BSS 零 = 无授权） | 20 |
| 内存（§2.2.5） | `s_nr_mem_range` / `s_mem_tab` / `s_ipcf` / `s_stack_guard` / `s_diag_sig`（46-57） | 内存白名单、IPC 过滤、栈保护 | 不触碰 | 24/23 |
| 运行时（§2.2.6） | `s_alarm_timer` / `s_grant_*` / `s_state_*` / `s_asyntab` / `s_asynsize` / `s_asynendpoint`（28-32、48、61-65） | 闹钟与进程侧服务表的挂载点 | 不触碰（BSS 零 = 未挂载） | 21/17 |

（4 + 6 + 3 + 4 + 5 + 9 = 31 字段，与 priv.h:21-66 一一对应。）

#### 2.2.1 身份/能力组：这个特权槽属于谁、是什么角色

字段：`s_proc_nr`（priv.h:22）/ `s_id`（priv.h:23）/ `s_flags`（priv.h:24）/ `s_init_flags`（priv.h:25）。

**OS 语义**：绑定与角色——`s_proc_nr` 回答「槽绑定到哪个进程」，`s_flags` 回答「这个进程是什么身份」（可抢占？计费？系统进程？根系统进程？VM？），`s_init_flags` 记录 init 阶段交给谁初始化。

**C 中的结构关系**：
- `s_proc_nr` 是槽的占用键：`NONE` 表示空闲——`get_priv()` 动态分配就靠扫描它找空槽；`id_to_nr(id)` 宏直接读它（priv.h:83）。
- `s_id` 是槽自身的下标：`s_ipc_to` 位图按它索引别的进程（§2.2.3），`priv_id(rp)` 读它（priv.h:80）。
- `s_flags` 的角色位（`PREEMPTIBLE`/`BILLABLE`/`SYS_PROC`/`ROOT_SYS_PROC` 等）位表见 [22-privilege.md](./22-privilege.md) §1.2。

**阶段 C 中的作用**：boot 循环只对 **schedulable 三类**（内核 task / RS / VM）调 `get_priv(rp, static_priv_id(proc_nr))`（main.c:200）做静态授予并覆写角色字段；其余 10 个用户模块不获得特权（打上 `RTS_NO_PRIV`，§2.1.6），等 RS 运行时经 PrivCtl 动态授予：

| 进程 | s_flags | s_init_flags | s_trap_mask | s_sig_mgr | p_priority / quantum |
|------|---------|--------------|-------------|-----------|----------------------|
| VM | `VM_F` | —（不覆写） | `SRV_T` | `SELF`（main.c:208） | `SRV_Q` / `SRV_QT`（main.c:209-210） |
| 内核 task | `IDL_F`（IDLE）/ `TSK_F` | `TSK_I` | `CSK_T`（CLOCK/SYSTEM）/ `TSK_T` | —（不覆写） | 保持 0（proc_init 初值） |
| RS | `RSYS_F` | `SRV_I` | `SRV_T` | `SRV_SM`（main.c:231） | `SRV_Q` / `SRV_QT`（main.c:232-233） |

角色常量（`VM_F`/`TSK_F`/`SRV_T`/`TSK_M` 等）的位定义见 22 §1.2。

**运行时细节**（s_flags 位表、PrivCtl 动态授予、fork 子进程共享 USER_PRIV）：详见 [22-privilege.md](./22-privilege.md)。

#### 2.2.2 信号组：谁是信号管理器、挂起了什么

字段：`s_sig_mgr`（priv.h:40）/ `s_bak_sig_mgr`（priv.h:41）/ `s_notify_pending`（priv.h:42）/ `s_asyn_pending`（priv.h:43）/ `s_int_pending`（priv.h:44）/ `s_sig_pending`（priv.h:45）。

**OS 语义**：信号投递的目标与簿记——`s_sig_mgr`/`s_bak_sig_mgr` 指定系统信号的接收者与备份接收者；`s_notify_pending` 记录哪些特权槽发来过 notification（按 sys_id 置位的位图）、`s_asyn_pending` 记录挂起的异步消息源、`s_int_pending` 记录挂起的硬件中断号、`s_sig_pending` 是 POSIX 信号集。

**C 中的结构关系**：三个 pending 位图都是「源 → 本进程」的记账，投递时由内核合并成一次 notification 唤醒（合并规则见 19）；`s_sig_mgr` 存的是 endpoint 而非槽指针，跨 slot 复用天然安全。

**阶段 C 中的作用**：boot 循环只写两处 `s_sig_mgr`——VM = `SELF`（自己管自己的信号，main.c:208）、RS = `SRV_SM`（根系统进程，main.c:231）；其余字段保持 BSS 零值（零 = 无挂起、无备份管理器）。

**运行时细节**（内核信号 SIG_*、通知合成、backup 切换）：详见 [19-syscall-signal.md](./19-syscall-signal.md)。

#### 2.2.3 IPC 组：允许哪些 trap、能给谁发、能调哪些内核调用

字段：`s_trap_mask`（priv.h:34，short 位图）/ `s_ipc_to`（priv.h:35，sys_map_t 位图）/ `s_k_call_mask[SYS_CALL_MASK_SIZE]`（priv.h:38，bitchunk 数组）。

**OS 语义**：三层能力掩码——能触发哪些内核 trap（IPC 原语）、能给哪些进程发消息、能调哪些 kernel call。微内核「最小权限」的落点。

**C 中的结构关系**：
- `s_ipc_to` 的位序是 **sys_id 不是 proc_nr**：`may_send_to(rp, nr)`（priv.h:86）先用 `nr_to_id(nr)` 把目标 proc_nr 翻译成对方的 sys_id 再查位。填图时的循环下标同样在 id 空间——boot 循环对 `ALL_M` 的展开就是对 `[0, NR_SYS_PROCS)` 逐位 `set_sys_bit`（main.c:239-241）。
- `s_k_call_mask` 每个 kernel call 号一位。

**阶段 C 中的作用**：三类掩码在 boot 循环中以「角色常量 + 全 0/全 ~0」的粗粒度填充：trap_mask 来自角色常量（§2.2.1 表）；ipc_to 来自 `TSK_M`/`SRV_M` 位图常量（`ALL_M` 特例全置），经 `fill_sendto_mask` 逐位写入（main.c:244）；`s_k_call_mask` 按 `kcalls == NO_C ? 0 : ~0` 整组置 0 或全 1（main.c:247-248）。细粒度的按位授权是运行时 PrivCtl 的事。

**IPC 对称性不变量**（微内核安全模型）：A 能发到 B ⇔ B 能回 A，除非 B 只声明了 RECEIVE trap。真正实现点是 `set_sendto_bit()`（system.c:307-330）：设位之后检查目标 `s_trap_mask & ~(1 << RECEIVE)` 非零则**反向设位**（system.c:319-322）；`fill_sendto_mask()`（system.c:349-359）本身不含对称代码，只是按 map 逐位调 `set_sendto_bit`/`unset_sendto_bit`——对称性是 `set_sendto_bit` 的副作用。完整论证见 22 §1.3。

**运行时细节**（掩码检查点、ipc filter 规则）：详见 [12-ipc-core.md](./12-ipc-core.md)、[23-ipc-filter.md](./23-ipc-filter.md)。

#### 2.2.4 I/O 组：能访问哪些端口、处理哪些 IRQ

字段：`s_nr_io_range`（priv.h:53）/ `s_io_tab[NR_IO_RANGE]`（priv.h:54）/ `s_nr_irq`（priv.h:59）/ `s_irq_tab[NR_IRQ]`（priv.h:60）。

**OS 语义**：设备驱动的硬件白名单——允许访问的 I/O 端口区间表、允许挂接的 IRQ 号表。「计数 + 定容数组」形态，计数 ≤ 0 = 无授权。

**C 中的结构关系**：内核在 I/O 类 kernel call（端口读写、IRQ 挂接）中逐项校验请求是否落在授权区间内——白名单存在特权槽，校验发生在系统调用层。

**阶段 C 中的作用**：无——boot 循环不触碰。内核 task 靠 ring 0 天然拥有全部硬件访问；用户态驱动模块的端口/IRQ 授权等 RS 运行时经 PrivCtl 填入。BSS 零 = 不允许任何 I/O，恰为安全初态。

**运行时细节**（PrivCtl 授予路径、kernel call 校验）：详见 [20-syscall-device.md](./20-syscall-device.md)。

#### 2.2.5 内存组：能访问哪些内存、过滤规则与栈保护

字段：`s_nr_mem_range`（priv.h:56）/ `s_mem_tab[NR_MEM_RANGE]`（priv.h:57）/ `s_ipcf`（priv.h:46，`ipc_filter_t *`）/ `s_stack_guard`（priv.h:49）/ `s_diag_sig`（priv.h:51）。

**OS 语义**：SAFE(mem) 类 kernel call 的物理内存白名单（驱动直读直写内存段的授权）、per-进程 IPC 过滤规则指针（跨空间 IPC 的包过滤）、内核 task 的栈溢出哨兵、诊断消息是否转信号的开关。

**为什么 `s_ipcf` 在内存组而不是IPC 组**：虽然 `s_ipcf` 在 IPC 路径被使用（`minix3/minix/kernel/ipc.h:21` 在 IPC 接收时检查），但它**本质是"IPC 期间允许的内存访问白名单"**——通过 IPC filter 限制进程间数据传输的内容。minix-rs Rust 端按此归入 `PrivMem`（`os/kernel/src/kpriv.rs:363-372`），与 `s_mem_tab` 同源——都是"进程可访问的资源范围"控制。分组标准是**权限模型视角**（按"授权什么资源"分组），不是**调用路径视角**（按"在哪里被使用"分组）。

**C 中的结构关系**：`s_mem_tab` 与 I/O 组同构（计数 + 定容数组）；`s_ipcf` 指向全局过滤规则池中的条目；`s_stack_guard` 只对内核 task 有意义（哨兵值 `STACK_GUARD`，priv.h:68-69）。

**阶段 C 中的作用**：保持 BSS 零值。过滤规则池的全局初始化 `IPCF_POOL_INIT()` 发生在 boot 循环之前（main.c:158），但 per-进程 filter 赋值是运行时的事。

**运行时细节**（mem range 授权、ipc filter 匹配、栈保护检查）：详见 [24-cross-space-runtime.md](./24-cross-space-runtime.md)、[23-ipc-filter.md](./23-ipc-filter.md)。

#### 2.2.6 运行时组：闹钟、grant/state 表、异步发送表的挂载点

字段：`s_alarm_timer`（priv.h:48，`minix_timer_t`）/ `s_grant_table` / `s_grant_entries` / `s_grant_endpoint`（priv.h:61-63）/ `s_state_table` / `s_state_entries`（priv.h:64-65）/ `s_asyntab` / `s_asynsize` / `s_asynendpoint`（priv.h:28-32）。

**OS 语义**：运行期挂载到特权槽上的服务登记处——同步闹钟定时器；grant 表（数据授权机制）与 state 表（状态存取机制）的「地址 + 条目数 + 归属端点」三元组；异步发送表（SENDA 的接收缓冲区，**表体在进程自己的地址空间**，特权槽只登记地址、元素数与归属端点）。

**C 中的结构关系**：`s_asynsize == 0` 即「异步表未挂载」（priv.h:29-30 注释）；`s_alarm_timer` 是嵌入的定时器结构，未挂闹钟时闲置。登记信息是 `vir_bytes` 用户态地址——内核不持有表体，只在 IPC/syscall 路径中借它定位。

**阶段 C 中的作用**：无——全部保持 BSS 零值（= 未挂载），又是「零值恰为正确初态」：boot 期没有任何 grant/state/asyn 活动。

**运行时细节**（sys_setalarm、grant 表协议、SENDA 拷贝路径）：详见 [21-syscall-clock.md](./21-syscall-clock.md)、[17-syscall-process.md](./17-syscall-process.md)、[12-ipc-core.md](./12-ipc-core.md)。

### 2.3 boot image 存储：image[] 清单与阶段 C 后的流程链路

§2.1/§2.2 按字段分组讲了「每个槽被怎么初始化」；本节收拢流程视角：boot image 是什么、模块二进制从哪来、阶段 C 之后的步骤链。

**image[]：编译时硬编码的进程清单**（table.c:44-65）。`struct boot_image image[NR_BOOT_PROCS]` 共 17 个条目——前 5 个是内核 task（ASYNCM/IDLE/CLOCK/SYSTEM/HARDWARE），后 12 个是用户态模块（DS/RS/PM/SCHED/VFS/MEM/TTY/MIB/VM/PFS/MFS/INIT）。两个关键事实：

1. **数组顺序 ≠ proc 号**：条目携带的 `proc_nr` 是 `com.h` 的固定值（如 DS=6、RS=2、PM=0、INIT=11），与数组顺序是两张表。boot 循环用 `proc_addr(ip->proc_nr)` 把每个条目放进正确的进程表槽位——用户态服务器之间按 proc 号寻址才不会错位（完整编号对照见 §1.4.1）。
2. **用户态模块的二进制来自 multiboot module**：`i ≥ NR_TASKS` 的条目从 `kinfo.module_list[i − NR_TASKS]` 取 `mod_start/mod_end`（main.c:181）——image[] 只定义「谁、什么号、叫什么」，代码内容由 boot loader 摆放的 module 链表提供。

**VM 是 boot 期唯一加载 ELF 的用户进程**：`arch_boot_proc()`（arch/i386/protect.c:388-455）对内核 task（`p_nr < 0`）直接返回；对 VM 构造 `exec_info`（64KB 栈、ELF 头直指 module 物理地址），以 6 个内存回调调 `libexec_load_elf` 解析 ELF 并映射进 bootstrap 页表，在栈顶布置 `ps_strings`，再由 `arch_proc_init()` 写入入口三要素（§2.1.4）；最后把 VM blob 的物理内存 `add_memmap` 回收进空闲表、清 `mod_start/mod_end = 0` 防止重复回收。其余 11 个用户模块 boot 期不加载 ELF——保持「寄存器零值 + RTS_VMINHIBIT」的占位状态，等 VM 运行后由 RS exec 重建（→ [09-vm-boot-protocol.md](./09-vm-boot-protocol.md)、[17-syscall-process.md](./17-syscall-process.md)；Rust 侧 load_vm_elf 见 §4.2）。

**boot 流程完整链路**（阶段 C 在其中的位置）——按 `minix3/minix/kernel/main.c` 源码顺序：

```
proc_init()                          清空 261 槽（`minix3/minix/kernel/main.c:157`）
→ boot 循环                          填充 §2.1/§2.2 各组字段 + arch_boot_proc 加载 VM
  (minix3/minix/kernel/main.c:164-271)
→ IPCNAME 注册                        SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA
  (main.c:285-290)
→ arch_post_init()                   设 ptproc=VM，记录 VM 的 cr3（**仅设指针**，不切换页表）
  (main.c:283)                       → 07
→ memory_init()                      预取 2 个 freepdes 空闲页目录（PDE）到 freepdes[] 池
  (main.c:293)                       → 07
→ system_init()                      初始化 IRQ 钩子 + 所有 priv[].s_alarm_timer + syscall 分发表
  (main.c:295)                       → 08
→ add_memmap()                       bootstrap 阶段用过的物理内存归还 VM 的 free list
  (main.c:301)                       → 07
→ bsp_finish_booting()               CPU 识别 + 唤醒 12 个用户态 boot 进程
  (main.c:316/324)                       (RTS_UNSET PROC_STOP, main.c:65) + 时钟中断初始化
                                     + FPU init + announce 启动 banner → 08
→ kernel_may_alloc = 0               关闭内核"自由用物理内存"窗口（VM 接管前）
  (main.c:105)
→ switch_to_user()                   切换到第一个用户态进程，进入正常运行
  (main.c:107)
```

相邻但不在本链路内的调用：`IPCF_POOL_INIT()` 在 boot 循环之前（main.c:158，→ 23）。

---

## Ch3. Rust 设计决策（按分组重表达）

### 3.0 核心设计原则

阶段 C 的 Rust rewrite 要捕获的本质：进程表/特权表/RTS/boot image/CPU 上下文/VM ELF 加载。存储形态的推导起点是 §2.0 的三层因果链（编译期定容模型 → boot 期零堆 → `[T; N]` 自然表达）——本章每条决策都能回溯到它或下面的 P1–P7。

**强制设计原则**：

| 原则 | 含义 | 违反时的症状 |
|------|------|------------|
| **P1 零堆启动** | boot 期无堆分配器，固定数组 + const fn | `Box<[KProcess]>` 在 boot 期分配 |
| **P2 no_std** | 不链接 std，除 `#[cfg(test)]` | `std::Vec` 出现在 kernel crate |
| **P3 SMP 安全** | BKL 下 Rc/RefCell 跨 CPU 不安全，用 Atomic + BKL | `Rc<RefCell<KProcess>>` 跨 CPU 共享 |
| **P4 硬件抽象为 trait** | 上层不读 arch 私有字段，不用 `#[cfg(target_arch)]` 选行为 | `#[cfg(target_arch = "x86_64")]` 出现在 kernel crate |
| **P5 FPU 是 arch 演进问题** | 不翻译 Minix3 的 fnsave/fxrstor，用现代 FXSAVE/CPACR_EL1.FPEN/sstatus.FS | `fpu_needs_zero: bool` 字段泄漏到 OS 层 |
| **P6 能力是数据，不是裸参数** | 权限/能力打包为结构体/枚举，不用 6 个裸参数 | `configure_boot_priv(flags, init_flags, trap_mask, ...)` |
| **P7 硬件行为下沉** | x86-64 特有行为（如 IOPL）通过 arch trait 方法下沉 | `syscall_device.rs` 直接操作 `X86_64_IOPL_BITS` |

**rewrite vs translate 的区别**：rewrite 捕获本质机制（"进程需要 CPU 上下文" → 不透明 `CpuContext`），translate 复制表面结构（"C 有 `arch_proc_reset` 函数" → Rust 也定义 `ArchProcReset` trait）。C 的 `arch_proc_reset`/`arch_proc_init`/`arch_boot_proc` 三个函数是实现细节，不是 OS 概念。Rust 版用 `build_cpu_context`（覆盖 reset+init）+ free fn `load_vm_elf`（覆盖 boot_proc 的 ELF 加载部分）替代——这是 rewrite，不是 translate。

**不变量表达：`Option` 替代「always-embedded + flag」（横切全部分组）**：

| C 模式 | Rust 模式 | 强制的不变量 |
|--------|----------|------------|
| `s_proc_nr = NONE`（`minix3/minix/kernel/proc.c:142`）— `NONE` 是宏常量 `31743`（`minix3/minix/include/minix/endpoint.h:55`），与合法槽位 **不重叠** | `s_proc_nr: Option<ProcNr>`（`os/kernel/src/kpriv.rs`） | "slot 未分配" = `None`，编译器强制处理 |
| `p_vmrequest.type == VMSTYPE_SYS_NONE` + 嵌套结构体始终存在（`minix3/minix/kernel/proc.h:88-117`） | `p_vm_suspend: Option<VmSuspendContext>`（`os/kernel/src/proc.rs:1013`） | `RTS_VMREQUEST ⟺ p_vm_suspend.is_some()`（Rust 端代码不变量注释） |
| `p_sendmsg`（`message` 类型）+ `RTS_SENDING` 位（`minix3/minix/kernel/proc.h:84`） | `p_sendmsg: Option<Message>` | `RTS_SENDING ⟺ p_sendmsg.is_some()` |
| `initial_pc = 0` + `arch_proc_reset` 初始化零值（`minix3/minix/kernel/proc.h:32`）— 用 BSS 零值作哨兵 | `EntrySpec.pc: Option<VirBytes>`（`os/arch/src/arch/boot.rs:75-83`） | "无入口点"与"入口点恰好为 0"类型可区分 |

如果用"always-embedded + flag"（字段始终存在 + bool 标志位），调用方可能忘记检查 flag 直接访问字段，读到无效数据。`Option` 让"未初始化"成为类型信息，编译器强制 `match`/`if let` 处理。

**错误处理策略**（同一原则的另一面）：

| 场景 | 策略 | 理由 |
|------|------|------|
| boot 期不变量违反 | `panic!` | boot 期错误不可恢复；Minix3 / minix-rs 选择 panic（替代方案：直接 halt / qemu debug-exit / skip-faulty-slot 等技术上可行，但属于设计选择） |
| 运行时能力分配失败 | `Result<PrivId, CapabilityError>` | 调用方可处理（如 RS 重试） |
| ELF 加载失败 | `Result<VmLoadResult, VmLoadError>` | 不静默失败，返回具体错误 |

### 3.1 进程表设计：固定数组 + 指针→索引（对应 §2.1）

**C 结构**：进程表是"索引→进程状态"的映射——`EXTERN struct proc proc[NR_TASKS+NR_PROCS]` 全局数组 + 侵入式裸指针（`p_nextready`/`p_scheduler`/`p_caller_q`）表达进程间链接；`proc_addr(n)` 裸下标定位，越界是 UB。

**Rust 表达**（约束驱动）：

- **零堆（P1）**→ 不能用 `Box<[KProcess]>` 堆分配，必须用固定数组 `[KProcess; PROC_TABLE_SIZE]`；`KProcess::new_zeroed()` 与 `ProcessTable::new()` 都标记为 `pub const fn`——**不是优化**而是**必须**：`static PROC_TABLE` 的初始化器（[lib.rs:1459](file:///os/kernel/src/lib.rs#L1459)）要求 const 表达式，否则编译失败。const 初始化还顺带实现"运行期零构造开销"，但**避免 `Once` / `lazy_static` 不是为性能**——是为消除首次访问的 init 分支 + 简化 boot 期 init 顺序依赖（项目内未引入这些 lazy init 机制，工程细节见 §3.8）
- **SMP 安全（P3）**→ Minix3 用 BKL（spinlock）保证串行化；Rust 把 C 的 `p_nextready` 裸指针改为 `AtomicI32` 索引——裸指针的编译器重排隐患由 `AtomicI32::load(Relaxed)` 显式消除（细节见下方表格 + 段尾展开）。`caller_q_head`/`caller_q_tail`/`send_q_link` 同理（链内容是 `ProcNr` 槽索引，无悬垂指针问题）。minix-rs 继承 Minix3 的 BKL 模型——而非 Linux 的 RCU / seqlock 路径。

**指针→索引的 rewrite**：

| C 字段 | C 类型 | Rust 类型 | 本质理由 |
|--------|--------|----------|---------|
| `p_nextready` | `struct proc *` | `AtomicI32` | 调度队列链接，C 端未标 `volatile`（proc.h:72）——BKL 的 LOCK 前缀 + mfence 已阻止 CPU 硬件重排，但编译器重排理论上仍可把对裸指针的读提到 BKL 之外。`AtomicI32::load(Relaxed)` 显式消除编译器重排隐患，不再依赖 GCC 约定俗成；固定槽索引不悬垂 |
| `p_scheduler` | `struct proc *` | `Option<ProcNr>` | 调度器归属，`None` 表内核默认调度；索引便于边界检查 |
| `p_caller_q` | `struct proc *`（队头） | `caller_q_head: Option<ProcNr>` + `caller_q_tail: Option<ProcNr>`（目标槽） | IPC 发送者等待队列头/尾；侵入式 FIFO 同构保留（链头在目标槽），`caller_q_tail` 是 O(1) 尾插扩展（C 遍历到尾，proc.c:960-964，FIFO 序不变） |
| `p_q_link` | `struct proc *`（后继） | `send_q_link: Option<ProcNr>`（发送方槽） | 链后继在发送方槽（同 C 分布）；索引链接无引用语义，零堆且入队不可失败（运行时语义见 12-ipc-core §2.5） |

**为什么这样表达**：`p_nextready` 用 `AtomicI32` 而非普通指针——C 端 `p_nextready` 是普通指针（`minix3/minix/kernel/proc.h:72`），**未标 `volatile`**。BKL 在 Minix3 实现中含 `xchg` LOCK 前缀 + `mfence`（`minix3/minix/kernel/arch/i386/klib.S:733-755`），CPU 硬件重排被阻止；但**编译器重排**仍可能把 `p_nextready` 的读提到 BKL 之前（C 标准不要求编译器理解 spinlock 语义）。Minix3 依赖"`p_rts_flags` 等字段声明 `volatile`、`p_nextready` 借助 BKL + spinlock 内存序约定"——理论上有隐患，实际靠 GCC 约定俗成不出错。**minix-rs 用 `AtomicI32::load(Relaxed)` 显式消除编译器重排隐患**，不再依赖编译器约定。`p_scheduler` 只在持 BKL 的调度决策中读写，`Option` 换取类型安全；`p_caller_q` 的队列语义（FIFO 等待者）在 C 里靠进程表内嵌链表字段拼出，Rust **同构保留侵入链布局**（链头/尾在目标槽、后继在发送方槽），仅把指针换成 `Option<ProcNr>` 槽索引——索引不悬垂（目标槽/发送方槽要么在表内、要么字段为 `None`），且全程零堆（链头/尾/后继三字段都是 `Option<ProcNr>`，作为 `KProcess` 静态数组 `[KProcess; PROC_TABLE_SIZE]` 的字段存储在 `.bss` 段，无运行期分配；运行时语义详见 [12-ipc-core §3.2](12-ipc-core.md)）。

**边界检查强制**：C 的 `proc_addr(n)` 无越界检查（越界是 UB）。Rust 的 `get(nr)`/`get_mut(nr)`（`os/kernel/src/proc_table.rs:105-112`）返回 `Option<&KProcess>`，把"越界是 UB"变为"越界是可捕获的 `None"`。内部经 `nr_to_idx(nr)` 映射：`index = nr + NR_TASKS`（同 C 的 `proc_addr` 偏移）——内核 task（nr < 0）映射到 `0..NR_TASKS`，用户进程（nr ≥ 0）映射到 `NR_TASKS..NR_TASKS+NR_PROCS`，越界返回 `None`。`nr_to_idx` 声明为 `pub(crate) const fn`：`const fn` 允许编译期求值；`pub(crate)` 限定 `dispatch_ipc_entry` 等 kernel crate 内部路径直接调用、无需走 `&ProcessTable` 入口。

**全局存储**：`static PROC_TABLE: SyncUnsafeCell<ProcessTable>` 放在编译期静态段（实际链接段由 link.ld 与 `ProcessTable` 字段初值决定，可能落在 `.bss` 或 `.data`——对应 C 的 `EXTERN struct proc proc[]`）。`SyncUnsafeCell`（显式 `Sync` + BKL 保护）是 Rust 2024 下 C EXTERN 语义的忠实表达——消除 `static mut`，避免 `static_mut_refs` lint，同时保持零堆与编译期固定地址（`spin::Once` 会引入 heap 分配，违反零堆）。

**与 C 的差异**：仅安全增强——数组形态、索引公式、per-CPU runqueue 语义与 C 完全一致；差异只在"裸指针 → 原子索引/侵入链槽索引"与"UB → `Option`"。

**为什么不用其他方案**：

- **不用 `Box<[KProcess]>` 堆分配**：boot 期无 `GlobalAlloc`，违反 §3.1 第三层（编译期地址）的硬约束
- **不用裸指针跨 CPU 共享 `p_nextready`**：编译期对裸指针的重排无法在 BKL 模型下保证安全，需要 `AtomicI32` 显式约束读写序
- **结论**：固定数组 + 原子索引是**零堆 + SMP 安全**这两条硬约束在当前 Minix3 BKL 模型下的唯一可行组合

### 3.1.1 KProcess 资源所有权模型（`[T; N]` 的第三层理由）

`[KProcess; N]` 静态数组能成立，除了**零堆**（§2.0 第二层）之外，还有第三个配套前提：**KProcess 不拥有需要隐式释放的 OS 资源**。若不满足，静态数组方案的"槽位复用"会撞上 Rust 的一个经典陷阱——`procs[i] = new_proc` 覆盖一个活着的 slot 时，旧值被隐式 drop，若旧值间接拥有带 OS side effect 的 `Drop`（close fd / release inode / wake waiter…），就会出现**未经显式设计的资源释放**。

这个陷阱的本质一句话就能点破：**Rust 的 `Drop` 把"OS 资源生命周期"偷偷绑定到了 Rust 对象生命周期**。内核本应把这两条生命周期分开管理——slot 里的数据只是进程状态，覆盖/复用它是普通内存操作；而 OS 资源的释放是协议行为（谁释放、何时释放、按什么顺序），必须由显式代码路径表达。一旦某些字段带上隐式 `Drop`，"覆盖赋值"这个普通内存操作就悄悄兼职了资源释放，释放时机从协议决定退化为对象生死决定。

规避这个陷阱的手段是让 KProcess 只装"可以被整体替换 / reset / 初始化"的状态与身份引用：

| 资源类别 | KProcess 中的对应 | 释放方式 |
|---------|------------------|---------|
| **状态**（可 reset/覆盖） | `p_rts_flags`/`p_misc_flags`（bitflags）、`Accounting`/`TimeStats`/`CyclesStats`/`CpuAvg`、`p_dequeued` | 随槽位覆盖即可，无资源语义 |
| **身份引用**（non-owning identity） | `p_nr: ProcNr`、`priv_id: Option<PrivId>`、`p_endpoint`/`p_getfrom_e`/`p_sendto_e: Endpoint`、`p_priv` 索引、`caller_q_head`/`caller_q_tail`/`send_q_link`（侵入链索引，槽位身份字段） | 索引值本身，不拥有任何对象 |
| **arch 状态缓冲**（零初始化） | `cpu_context: CurrentCpuContext`、`fpu_state: CurrentFpuState` | 编译期零构造；运行期由 arch trait 显式 save/restore（§3.5 / §3.6） |

> KProcess 全字段无堆容器——`caller_q_head`/`caller_q_tail`/`send_q_link` 三字段是 `Option<ProcNr>` 槽索引（侵入链分布，详见 [12-ipc-core §3.2](12-ipc-core.md)），归入"身份引用"类；队列腾空协议由 `clear_ipc`/`clear_ipc_refs` 在 `dispatch_clear` 中显式执行（详见 [12-ipc-core §2.5](12-ipc-core.md)）。

minix-rs 的 kernel 侧因此满足一个更精确的判别：**KProcess 的手写 `Drop` 不执行任何资源释放**——它有且只有一个动作：当 `SLOT_FREE` 位已被清除（槽位占用中）却仍被隐式销毁时 `panic!`（fail-fast 报警）。真正的 OS 资源（fd / inode / endpoint 对象）在微内核边界之外的 user-space server（PM 的 `mproc`、VFS 的 `fproc`），kernel 进程表本就不持有。这一边界的直接后果：

- **槽位退出 = 显式协议，不依赖 Drop**：进程清理走 `dispatch_clear`（`os/kernel/src/syscall_process.rs:408-497`）的逐步显式操作——`release_address_space` → 移除 IRQ 钩子 → `clear_endpoint` → `reset_alarm_timer`（闹钟侵入链摘除，见 15-clock-timer §4.3） → `rts_set(SLOT_FREE)` → 清 FPU 标志 → 释放特权槽关联。每步都是显式 kernel call，不靠隐式析构；防御性 `Drop` 只在此协议被**绕过**时报警，不替代也不干扰协议本身。
- **槽位交换 = 位搬运，无隐式动作**：live update 的 `swap_slots`（[proc_table.rs:126-141](file:///os/kernel/src/proc_table.rs#L126-L141)）用 `core::mem::swap` 整体交换两个 slot——等价 C 的 `*src = orig_dst; *dst = orig_src` 覆盖赋值，`mem::swap` 是位搬运、不触发 `Drop`，防御性 `Drop` 同样不干预这一步。
- **意外销毁 = fail-fast 报警，非资源释放**：`KProcess`/`KPriv` 的 `Drop` 体只检查占用标记（`SLOT_FREE` / `s_proc_nr`），占用中即 `panic!`。这给"槽位复用"补上了第 §3.1.1 开头所述陷阱的最后一环——覆盖活槽这类开发期错误不再是**静默**的"旧值被隐式 drop"，而是立刻崩溃定位。测试侧同理：测试若把占用槽当普通值丢弃，报警会迫使测试走显式豁免夹具（`os/kernel/src/test_helpers.rs`）。

> 这条设计原则是 `[T; N]` 静态数组成立的必要组成，与 §2.0 三层因果链（编译期定容 → boot 期零堆 → 自然表达）互补：前者回答"为什么能静态存"，本节回答"为什么静态存不会引入隐式生命周期语义"——`Drop` 被设计成"报警而非释放"，保证了**对象生命周期 ≠ OS 资源生命周期**，二者在 kernel 侧彻底解耦。`impl Drop` 同时自动禁止 `Copy`（Rust：有 `Drop` 的类型不能 `Copy`），KProcess 不再是"一坨可以随便复制的值"，而是"有生命周期语义的实体"。

### 3.2 特权表设计：CapabilityTemplate 枚举替代 6 裸参数（对应 §2.2）

**C 结构**：特权表是"进程被允许做什么"的静态能力集合——`struct priv` 的 30+ 裸字段（`s_flags`/`s_trap_mask`/`s_ipc_to`/`s_k_call_mask`/`s_sig_mgr`...），boot 期通过 `get_priv()` + 散落的 `if is_vm { flags=VM_F; ... }` 分支配置。

**Rust 表达**（约束驱动）：

- **能力是数据（P6）**→ 6 个裸参数 `configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` 缺乏类型安全和 OS 语义，打包为 `ProcessCapability` 结构体 + `CapabilityTemplate` 枚举
- **零堆（P1）**→ `PrivTable` 用固定数组 `[KPriv; NR_SYS_PROCS]` + const fn（与进程表同构，§3.8）
- **角色→能力的显式映射**→ C 的 `if is_vm { ... } else if is_root_sys { ... }` 散落分支，Rust 提取为 `CapabilityTemplate` 枚举的 5 变体（[capability.rs:167](file:///os/kernel/src/capability.rs#L167)）

```rust
pub enum CapabilityTemplate {
    Idle,         // IDLE：可计费，无 IPC trap，无 kcall
    KernelTask,   // CLOCK/SYSTEM/KERNEL/ASYNCM：内核 task，
                  //   ipc_to = NONE（不能主动 SEND 到其他 endpoint）；
                  //   trap_mask 默认 NONE，CLOCK/SYSTEM 的 RECEIVE trap
                  //   由 grant_capability 按 proc_nr 特例授予（见下方注）
    Vm,           // VM：全部 IPC + 全部内核调用
    RootService,  // RS：全部 IPC + 全部内核调用
    Deferred,     // 其他：boot 期不授予，等 RS 运行时配置
}
```

5 个变体均为单元变体——"trap_mask" 等细节字段不放在变体上，而是通过 `CapabilityTemplate::capabilities() -> ProcessCapability`（flags）、`CapabilityTemplate::trap_mask() -> TrapMask`（role 默认值）、`ipc_mask()`/`kcall_mask()` 三个关联方法查询；具体数值写入 `PrivFlags.s_flags` 与 `PrivIpc.s_trap_mask` 两个子结构。`init_proc_and_boot()` 调用 `grant_capability(nr, template)` 一步完成"分配 slot + 写入 flags/trap_mask/ipc_to/kcall_mask/sig_mgr"，不再散落 `if/else if` 分支。

> **trap_mask 的两层职责**：`CapabilityTemplate::trap_mask()` 返回**角色默认值**（模板是 role 抽象，与具体进程身份无关），其中 `KernelTask` 默认 `TrapMask::NONE`（与 `Idle`/`Deferred` 同——代表"任何非 Vm/非 RS 的角色都无 IPC trap"）。**CSK_T 特例**（C 在 [priv.h:59](file:///minix3/minix/include/minix/priv.h#L59) 给 CLOCK/SYSTEM 配的 `(1 << RECEIVE)`）由 `PrivTable::grant_capability` 应用——它知道 proc_nr，按 `(proc_nr == CLOCK || proc_nr == SYSTEM)` 切换 `TrapMask::RECEIVE`（[kpriv.rs:1021-1032](file:///os/kernel/src/kpriv.rs#L1021-L1032)）。这是 C `main.c:218-219` `(proc_nr == CLOCK || proc_nr == SYSTEM ? CSK_T : TSK_T)` 的 Rust 同构——CSK_T 知识保持在 init 阶段、与 C 同位置（[capability.rs:180-205](file:///os/kernel/src/capability.rs#L180-L205) doc-comment 说明）。测试 [`test_grant_capability_kernel_task_clock_system_csk_t`](file:///os/kernel/src/kpriv.rs) + [`test_grant_capability_kernel_task_non_cs_gets_tsk_t`](file:///os/kernel/src/kpriv.rs) + [`test_grant_capability_kernel_task_ipc_to_consistent_no_send`](file:///os/kernel/src/kpriv.rs) 三组验证：CLOCK/SYSTEM → CSK_T=4、KERNEL/ASYNCM → TSK_T=0、所有 kernel task ipc_to=NO_M（不能主动 SEND）。

**为什么这样表达**：如果让调用方手动传 6 个裸参数（`flags: u32, init_flags: i32, trap_mask: u16, ipc_to: u64, k_call_mask: [u32;2], sig_mgr: Endpoint`），会有三个问题：(1) 参数顺序易错（`flags` 和 `init_flags` 都是整数，类型不区分）；(2) 调用方需要记住每个角色的配置值，散落在主流程；(3) 无法在类型层面阻止"传了 VM 的 flags 但忘了设 sig_mgr"。`CapabilityTemplate` 枚举把角色作为类型，配置作为数据，消除这三个问题。

**与 C 的差异**：外部行为不变——每个角色的 flags/trap_mask/ipc_to/k_call_mask/sig_mgr 数值与 C 一致（含 CSK_T 特例，见上方注）；差异只在配置的组织方式：分支逻辑收拢进枚举的关联方法（role 默认）+ `grant_capability` 的 per-process 特例（CSK_T），主流程不再散落 `if/else if`。

KPriv 内部的 8 子结构分组见 §3.10（索引节）与 [22-privilege.md §4.2](./22-privilege.md)（权威展开）。

### 3.3 RTS 位图设计：bitflags + AtomicU32 + 强类型不变量（对应 §2.1.6）

**C 现状**：RTS（Run-Time Status）是进程"现在能不能跑"的动态状态——`volatile u32_t p_rts_flags` + `#define RTS_SLOT_FREE 0x01` 等 16 个裸整数宏（proc.h:142-166），不变量"A process is runnable iff `p_rts_flags == 0`"靠注释与约定维护。两个**待解决问题**：
1. **裸宏 + 裸整数**位操作无类型检查——`p->p_rts_flags |= 0x80` 与 `|= 0x80000000` 编译期都通过，运行期靠内存里的值判真伪。
2. **`volatile` + BKL 约定**——`volatile` 防编译器优化掉读，C 标准不要求理解 spinlock 语义，所以 BKL 持有性是**隐式契约**（K&R 注释 + 程序员守纪律）。

**Rust 表达**：`RtsFlagsBits`（`bitflags!`，16 位全集与 C 一一对应，见 §2.1.6）+ `RtsFlags(AtomicU32)` newtype（[proc.rs:1031-1307](file:///os/kernel/src/proc.rs) / [sched.rs](file:///os/kernel/src/sched.rs)）。位操作有类型检查，不再是裸整数与位运算符的自由组合。

**与 C 的差异**：位值、位语义、设置时机与 C 完全一致；差异在**类型层**（裸宏 → bitflags 类型）与**并发层**（`volatile` → 显式 Atomic + BKL 协议）——"可运行 ⟺ 0"从注释约定升级为**封装方法强制**（`rts_set/rts_unset` 集中处理 RTS 变更与 enqueue/dequeue 联动）。调度联动（dequeue/enqueue 时机）的具体差异见下方"并发协议"段。

**为什么这样表达**：

- **位图而非枚举**：进程可同时因多个原因不可运行（如无特权 + 等 VM 建页表 + 被停止），位图支持多原因叠加，枚举只能表达单一状态。
- **不变量强制**：`rts_set()`/`rts_unset()` 方法封装"设标志 → dequeue"/"清零 → enqueue"的联动（[proc_table.rs:282/304](file:///os/kernel/src/proc_table.rs#L282-L304)），不让调用方手动维护调度队列一致性（联动细节 → 11-scheduling-primitives）。
- **`SLOT_FREE` 等阶段 C 关键位**的设置时机与 §2.1.6 表一致：`SLOT_FREE`（proc_init 清空，proc.rs:1261）、`NO_PRIV`/`NO_QUANTUM`/`VMINHIBIT`/`BOOTINHIBIT`（非 schedulable / 非 VM 用户进程）、`PROC_STOP`（所有 boot 进程，bsp_finish_booting 清除）。
- **misc_flags 与 RTS 的区别**：RTS 决定可运行性（影响调度队列），`misc_flags` 记录次要运行时状态（不影响调度）。两者分离避免"改次要状态误触发 enqueue/dequeue"。`MiscFlags` 同样保留 C 的全部 18 位（proc.rs:196-214）；阶段 C 之外的位由消费方文档覆盖：`EXT_REG_INITIALIZED`（fork/exec/signal，见 17）、`DELIVERMSG`/`KCALL_RESUME`（消息投递，见 13）、`NICED`（调度参数调整，见 11）。

**并发协议**——BKL 提供互斥，Atomic 提供类型安全，Relaxed/Acquire 区分"读场景"三层协同：

**（一）概念结论**。**主路径写者**在 BKL 保护区内修改 RTS：`rts_set`/`rts_unset`（[proc_table.rs:282/304](file:///os/kernel/src/proc_table.rs#L282-L304)）均要求 caller 已持 BKL；其他 CPU 的 `AtomicU32` 读则不持 BKL。具体到每个字段用 `Relaxed` 还是 `Acquire` 取决于"读者是否在持锁状态下"——下面分开讲。**注意**：`sched_proc`（[sched.rs:321-411](file:///os/kernel/src/sched.rs#L321-L411)）是**特例**——它持 `&mut KProcess` 不经 BKL 改 RTS 位（见下方（二）裸路径），对 BKL 体系无依赖，靠 syscall 出口统一重调度（pick_proc）。

**（二）写路径分两条**——`AtomicU32` 装的是位值，谁改、用什么封装，是两个独立设计点：
- **封装路径**：`table.rts_set(ProcNr(0), RtsFlagsBits::PROC_STOP)`（[sched.rs:503](file:///os/kernel/src/sched.rs#L503)）经 `rts_set` 封装（[proc_table.rs:282](file:///os/kernel/src/proc_table.rs#L282)），RTS 变化自动联动 dequeue/enqueue——这是调度器主体采用的写法。
- **裸路径**：`p.p_rts_flags.set(RtsFlagsBits::NO_QUANTUM)`（[sched.rs:366](file:///os/kernel/src/sched.rs#L366)）、`p.p_rts_flags.clear(RtsFlagsBits::NO_QUANTUM)`（[sched.rs:408](file:///os/kernel/src/sched.rs#L408)）**不经** `rts_set`/`rts_unset`，是 `sched_proc`（`SYS_SCHEDPROC` 实现，[sched.rs:321-411](file:///os/kernel/src/sched.rs#L321-L411)）内部的**临时标记**写法——它拿 `&mut KProcess`（无法访问调度队列），只能裸改位；依赖 syscall 出口统一重调度（pick_proc）。
  - 与 C 的差异：`sched_proc` 的裸 set/clear **尚未完全对齐 C 的 `RTS_SET/RTS_UNSET` 宏**（system.c:674/697，宏会触发 dequeue/enqueue）。当前行为是"出口重调度"模型（对外部行为正确，因 sched_proc 改参数后统一走 pick_proc），但改 priority 后 runqueue 位置不立即重排。是否改为持有 `&mut ProcessTable` 并完整对齐 C 语义，**deferred 到调度相关文档（11-scheduling-primitives）review 时再决策**（见 todo.md D-52）。

**（三）读路径按字段分**——读的核心问题是"是否在持锁状态下"，决定 `Acquire` vs `Relaxed`：

`p_rts_flags` 走 `RtsFlags` API——`is_set` 用 `load(Ordering::Acquire)`（[proc.rs:282](file:///os/kernel/src/proc.rs#L282)）。这是因为 `p_rts_flags` 是**可运行性判定**——调度器、其他 CPU、甚至中断上下文都可能读它（**不持 BKL 的无锁读**），必须 `Acquire` 与写者的 `Release`（`RtsFlags::set`/`unset`/`set_raw` 用 `AcqRel`，proc.rs:286/291/301）配对，建立"写者在 BKL 释放前对 `p_rts_flags` 的所有写入"与"读者 `load` 之后读到的所有内存"之间的 happens-before。**注意**：Acquire 防止的是 `load` **之后**的读被 CPU 重排到 `load` **之前**（而不是 `load` 本身被重排），目的是保证读者后续读到 BKL 释放后的完整一致视图——避免看到"新位图 + 旧其他内存"的撕裂组合。

`p_nextready` 走裸 `AtomicI32` + `load(Ordering::Relaxed)`（[sched.rs:153, 167](file:///os/kernel/src/sched.rs#L153)）——读场景固定在"调度器遍历就绪链表"内，且**已持 BKL**。为什么能用 `Relaxed` 取决于两个设计选择：
- **类型选择**：C 里 `p_nextready` 是 `struct proc *`（proc.h:72）；Rust 把"指向下一个进程"从**内存地址**转换成**进程编号 i32**——`AtomicI32` 装的是 `ProcNr.0`，不是地址。这是 Rust 类型系统的**重新表达**（rewrite 而非 translate）：(a) 裸指针 `*mut KProcess` 跨 CPU 共享时是 `!Sync`，编译期被 ban；(b) 不用 `ProcNr` newtype 是刻意选择（[proc.rs:31](file:///os/kernel/src/proc.rs#L31) 注释：`.0` for `AtomicI32` interop and array indexing），链遍历高频 `load → as_i32 → 比较` 用 `i32` 零开销。
- **内存序选择**：`Relaxed` 本身只保证**单字段的原子性**（读不会读到撕裂值，且本字段的读不会被重排到本字段的写之前；写不会被重排到本字段的读之后——这是 `Atomic<T>` 类型在所有 Ordering 下都保证的**单字段次序**），**不建立跨字段 happens-before**——这正是 C 里 `volatile` 也做不到的事（C `volatile` 只防编译器优化，不参与内存模型）。Rust 用 `Acquire`/`Release` 才会建立 happens-before。`p_nextready` 用 `Relaxed` 是**故意的弱序选择**——读者和写者之间的互斥由 BKL 提供（写者 `sched_enqueue/sched_dequeue` 持 BKL，读者调度器遍历也持 BKL），同一时刻不存在"无锁并发读写"，所以不需要 `Acquire`/`Release` 的额外屏障开销。

**关键反推（未来防御）**：如果有一天去掉 BKL、改用 RCU 或 per-CPU lock，`p_nextready` 的 `Relaxed` 就会**不安全**——必须升到 `Acquire`/`Release` 才能与新锁协议同步。这是 Relaxed 选择的隐藏代价，写在此处留给未来重构者。

一句话总结：Rust 用原子类型表达 C 裸指针的链表链接字段，用最弱内存序 `Relaxed` 读取；它正确**不是因为 Relaxed 有同步能力**，而是因为**所有写者都在 BKL 保护区内**——互斥来自 BKL（跨字段顺序不需要靠 Atomic 提供），`Atomic` 防撕裂读 + 防单字段乱序，`Relaxed` 把跨字段屏障开销降到零。

### 3.4 boot image 类型设计：ProcKind + EntrySpec（对应 §2.3）

**C 结构**：boot image 是编译时硬编码的进程清单——`image[]` 数组 + `ip->proc_nr`/`ip->pc`/`ip->stack_addr` 裸字段；"这是内核 task 还是 VM"靠 `iskerneln()`/`isrootsysn()`/`VM_PROC_NR` 散落判断。

**Rust 表达**：两个 OS 概念类型。

**ProcKind 枚举**（进程角色，OS 概念不是硬件概念）：

```rust
pub enum ProcKind {
    KernelTask,    // CLOCK/SYSTEM/IDLE/KERNEL：内核态，无 ELF
    Vm,            // VM：用户态，boot 期加载 ELF
    RootService,   // RS：用户态，boot 期不加载 ELF
    UserService,   // 其他系统服务：boot 期不加载，RS 运行时加载
    UserProcess,   // 用户进程：boot 期不存在，fork/exec 创建
}
```

`ProcKind` 替代 C 的 `iskerneln`/`isrootsysn`/`VM_PROC_NR` 散落判断。arch 层根据 `kind` 决定初始 PSW/PSR/sstatus、段选择子、FPU 策略。

**EntrySpec 结构体**（入口点规格）：

```rust
pub struct EntrySpec {
    pub pc: Option<VirBytes>,         // 入口点 PC，None=未加载
    pub sp: Option<VirBytes>,         // 初始 SP，None=未设置
    pub ps_strings: Option<VirBytes>, // ps_strings 地址，None=无（kernel task）
}
```

`Option` 表达"暂未确定"——kernel task 无入口点（`EntrySpec::KERNEL_TASK` 全 None），非 VM 用户进程延后加载（`EntrySpec::DEFERRED`），VM 进程 ELF 已加载（`EntrySpec::loaded(pc, sp, ps_strings)`）。

**为什么这样表达（EntrySpec 用 Option）**：如果用 `pc: VirBytes`（非 Option），无法在类型层面区分"kernel task 无入口点"和"入口点恰好是 0"，调用方需要额外 `is_kernel: bool` 参数。`Option` 让"有无入口点"成为类型信息，编译器强制处理两种情况。

**类型层局限（仍依赖 `ProcKind` 区分）**：当前 `KERNEL_TASK` 与 `DEFERRED` 都是全 None 的 `EntrySpec`，从 `EntrySpec` 自身无法区分两者——区分必须通过 `ProcKind::KernelTask` vs `ProcKind::UserService`。这是 `Option` 表达"未知 vs 已知"的常见边界：两个"未知"语义（"永远不会有" vs "将来会有"）无法用 `Option<VirBytes>` 区分，需借助伴生类型（`ProcKind`）。

**ProcKind 与 CapabilityTemplate 为什么不合并**：两者各有 5 个变体、看似一一对应（KernelTask/Vm/RootService/UserService/UserProcess），但关注点不同——`ProcKind` 是给 arch 层看的（决定初始 PSW/PSR/sstatus、段选择子、FPU 策略），`CapabilityTemplate` 是给 kernel 层看的（决定 IPC/syscall/trap 权限）。`ProcKind::KernelTask` ≠ `CapabilityTemplate::KernelTask`：前者表达"运行在内核态"，后者表达"task 标志 + TrapMask 关联方法"。反例：IDLE 进程是 `ProcKind::KernelTask` + `CapabilityTemplate::Idle` 的组合——角色与能力并不一一对应，合并会丢失这种组合自由度。

**与 C 的差异**：行为不变（进程清单、编号映射、加载时机同 §2.3）；差异在"散落判断 → 类型决策"——arch 层对 `ProcKind` 的 `match` 不可漏分支，漏处理在编译期报错。

### 3.5 CpuContextArch trait：arch CPU 状态抽象的核心接口（对应 §2.1.4）

**C 结构**：boot 一个进程，CPU 需要完整的初始状态（PSW/PSR/sstatus + 段选择子 + PC/SP + FPU 策略）。C 用 3 个函数分步构建：`arch_proc_reset()`（清零寄存器 + 初始 PSW）、`arch_proc_init()`（设 PC/SP/ps_strings）、`arch_boot_proc()`（VM ELF 加载 + 状态构建）。

**为什么合并为 1 个 trait（不镜像 C 调用链）**：C 的 3 个函数是**实现细节**，不是 OS 概念。Rust 按 OS 概念重新划分——"构建状态"（`build_cpu_context`）与"应用状态"（`apply_to_trap_frame`）两个正交操作 + 一个共享 free fn：

| C 函数 | OS 概念 | Rust 落地 |
|--------|--------|---------|
| `proc_init()` | 进程表初始化为全空槽 | `ProcessTable::new()` const 构造（§3.1/§3.8） |
| `arch_proc_reset()` | 为新进程构建初始 CPU 状态 | `build_cpu_context(ProcKind::KernelTask, ...)` |
| `arch_proc_init()` | 为用户进程构建带入口点的 CPU 状态 | `build_cpu_context(ProcKind::Vm, EntrySpec::loaded(...))` |
| `arch_boot_proc()` | 加载 VM ELF + 构建启动状态 | free fn `load_vm_elf()`（§3.7）+ `build_cpu_context()` |
| `get_priv()` + 特权设置 | 为进程授予能力 | `grant_capability(nr, template)`（§3.2） |
| boot image 循环 | 按角色编排所有 boot 进程 | `init_proc_and_boot()` 主流程（§4.7） |

`build_cpu_context` 一个操作承接 C 的 reset/init 两函数：reset 对应 `ProcKind::KernelTask`（无入口点），init 对应 `ProcKind::Vm` 等（带入口点）——"构建状态"按 `ProcKind` + `EntrySpec` 区分两种形态，函数划分按 OS 概念而非按 C 实现细节。如果用 3 个 trait 镜像 C 的 3 函数：(1) `ArchProcInit::init_regs` 三架构实现几乎相同（假多态）；(2) trait 继承链"reset→init→boot"翻译自 C 调用链，不是 OS 概念——Rust 抽象应表达 OS 概念而非镜像 C 函数链。

**Rust 表达**（[boot.rs:145](file:///os/arch/src/arch/boot.rs#L145)）：

```rust
pub trait CpuContextArch {
    /// arch 私有的"进程 CPU 上下文"，kernel 层只存储不 inspect
    type CpuContext: Copy + core::fmt::Debug + Default;
    /// arch 私有 trap frame 类型。同 `CpuContext` 的 `Copy + Default` 论证
    type TrapFrame: Copy + core::fmt::Debug + Default;

    /// 为新进程构建 CPU 上下文（boot 期调用）
    fn build_cpu_context(kind: ProcKind, proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext;

    /// 把上下文应用到 trap frame（首次调度时调用）
    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame);

    /// 启用用户 I/O 特权（x86-64: RFLAGS.IOPL=3；其他架构: no-op）
    fn enable_user_io(ctx: &mut Self::CpuContext) {
        let _ = ctx; // 显式"故意忽略"——x86-64 在实现中用 ctx
    }
}
```

**命名说明**：trait 名叫 `CpuContextArch`（不是 `BootArch`），因为 `apply_to_trap_frame` 在首次调度时调用，跨越 boot + runtime 两个阶段。关联类型叫 `CpuContext`（不是 `StartupState`），因为这个值长期存储在 `KProcess` 中，不是一次性启动值。

**`CpuContext` 与 trap frame 的区分**：`CpuContext` 是进程**尚未运行**时的初始状态（arch 私有、不透明）；trap frame 是进程**正在运行/被中断**时 CPU 寄存器的保存区（OS 可见）。两者通过 `apply_to_trap_frame` 桥接（调用时序见 §3.12）。

**为什么是关联类型而非通用结构体（非法状态不可表达）**：如果定义 `struct InitialRegState { status, segment_selectors, fpu_needs_zero }` 供三架构共用，则 (a) `segment_selectors` 在 aarch64/riscv64 永远全零——非法状态可表达（类型系统允许填入无意义值）；(b) `fpu_needs_zero` 在 aarch64/riscv64 永远 false——同理；(c) 所有架构被迫 import `SegmentSelectors` 类型——硬件语义泄漏到 OS 层。关联类型让每个架构定义自己的 `CpuContext`：aarch64 编译时类型系统中**根本不存在** `SegmentSelectors`，"非法状态不可表达"由类型系统强制。这是 Rust 类型设计原则（make invalid states unrepresentable）在硬件抽象上的直接应用。

**为什么是 trait 而非 cfg-alias**：3 个架构的 `CpuContext` 字段布局完全不同（x86-64 有段选择子 + `fpu_policy`，aarch64 有 `fpu_enable_el0`，riscv64 有 sstatus.FS；保存区都不在 `CpuContext` 里，而在 `KProcess.fpu_state`），`build_cpu_context` 和 `apply_to_trap_frame` 的实现行为真的不同。trait 提供显式契约 + 支持 mock 测试（`MockCpuContextArch`）+ 零运行时开销（静态分发）。

**FPU 保存区为什么保留在 KProcess、但用 arch 私有类型承载**：C 的 `struct proc` 经 `p_seg.fpu_state` 指向内核静态 FPU 保存数组（每进程 512B，x86 模型，见 §2.1.4）。Rust 版**保留** per-process 保存区（Linux `thread_struct`、Redox 每任务 FPU context 均为同构设计——上下文切换与信号返回需要一个进程私有的落点），但把"裸 char 数组 + `p_seg` 指针手工指向"换成 arch 私有的类型化缓冲 `KProcess.fpu_state: CurrentFpuState`（[proc.rs:990](file:///os/kernel/src/proc.rs#L990)）。`CpuContext` 只承载 FPU **策略**（如 x86-64 的 `fpu_policy` 枚举，§3.6），不承载保存区；kernel 层不 inspect 保存区布局，只经 `FpuArch::save/restore` trait 保存/恢复。各架构保存区类型与字节数（512/528/264B）→ §4.9。

**enable_user_io 下沉（P7）**：x86-64 的 IOPL 位操作是硬件语义，不应泄漏到 kernel 层。通过 `enable_user_io` trait 方法下沉：x86-64 实现设 `psw |= 0x3000`，aarch64/riscv64 用 default no-op。kernel 层调用 `CurrentCpuContextArch::enable_user_io(&mut ctx)`，不接触硬件位。

**与 C 的差异**：C 三函数调用链 → 一 trait 两方法 + 一 free fn；行为等价——初始寄存器值、PSW/段选择子逐位与 C 对齐（§5.1 测试验证）。

### 3.6 FPU 架构演进：不翻译 Minix3 的 fnsave（设计部分，[ARCH]）

> **这是 "rewrite not translate" 最典型的案例**——FPU 状态管理按现代 ISA 模型重新表达。C 侧 FPU 机制的逐函数分析见 [31-fpu-context-switching.md](./31-fpu-context-switching.md)；实现细节（`CurrentFpuState` 类型/字节数/内存预算）→ §4.9。

**C 结构（i386）**：`fnsave`/`fxrstor` 是 x86 指令模型——`fnsave` 是 80387 时代指令（108B x87 状态），现代 CPU 走 `fxsave/fxrstor`（512B，[arch_system.c:103-105/201](file:///minix3/minix/kernel/arch/i386/arch_system.c#L103-L105)）。保存区**不在 proc 结构内**：`p_seg.fpu_state` 是 `char *` 指针（[archtypes.h:35](file:///minix3/minix/include/arch/i386/include/archtypes.h#L35)），指向 arch 层静态池 `fpu_state[NR_PROCS][FPU_XFP_SIZE]`（256×512B，FPUALIGN=16 对齐，[arch_system.c:144](file:///minix3/minix/kernel/arch/i386/arch_system.c#L144)）；`arch_proc_reset` 为用户进程按 `p_nr` 分配槽位并清零，内核 task 指针为 NULL（[arch_system.c:148-168](file:///minix3/minix/kernel/arch/i386/arch_system.c#L148-L168)）。`FPU_XFP_SIZE = 512`（[fpu.h:43](file:///minix3/minix/include/arch/i386/include/fpu.h#L43)）。Rust 侧「内嵌值」组织对照见 §4.9。

**设计论证（为什么不用 fnsave/fxrstor 翻译路径，P5）**：翻译会把 x86 指令语义泄漏到 OS 层（`fpu_needs_zero: bool` 字段流经 kernel），且无法表达 aarch64/riscv64 的 FPU 控制方式（CPACR_EL1.FPEN / sstatus.FS 状态机）。三架构现代模型：

| 架构 | FPU 控制机制 | 初始化策略 |
|------|------------|----------|
| **x86-64** | CR4.OSFXSR 使能 FXSAVE/FXRSTOR（512B 固定布局，非可变长 XSAVE） | `fpu_policy` 枚举：KernelTask（不初始化）/ LazyUserInit（首次 FP 指令 trap 时惰性初始化保存区） |
| **aarch64** | CPACR_EL1.FPEN 控制 EL0/EL1 FPU 访问 | `fpu_enable_el0` 布尔（CPACR_EL1 在 cstart 全局配置） |
| **riscv64** | sstatus.FS 字段（Off/Initial/Clean/Dirty 四态） | sstatus.FS = Initial（首次 FP 指令 trap 时 lazy 初始化） |

x86-64 的 FPU 策略（arch 内部枚举，OS 看不到）：

```rust
enum X86FpuInitPolicy {
    KernelTask,      // 复用内核 FPU 上下文，不初始化
    LazyUserInit,    // 首次使用时初始化 XSAVE area（现代 lazy 模式，非 memset 清零）
}
```

**为什么这样表达**：三架构**都有** per-process 保存区，但保存/恢复指令与惰性策略不同（FXSAVE/FXRSTOR、FPSIMD load/store、F 扩展 load/store）——这正是 `CurrentFpuState` 按架构独立定义、`FpuArch` trait 提供统一接口的原因（`FpuArch::save/restore` 调用点 [smp.rs:523](file:///os/kernel/src/smp.rs#L523)）。保存区是 `KProcess.fpu_state`（arch 私有类型），`CpuContext` 只承载 FPU 策略；kernel 层不 inspect 保存区布局。现代 FXSAVE/CPACR_EL1.FPEN/sstatus.FS 模型才是三架构统一的抽象方向。

**与 C 的差异（架构演进标注）**：FPU 状态管理按现代 ISA 模型重新表达（保存区语义——每进程一份、切换时保存/恢复——保持等价）——不是纯 rewrite：指令模型换了，外部行为（进程间 FPU 状态隔离、惰性初始化时机）不变。详见 [31 §3](./31-fpu-context-switching.md) 与 `os/arch/src/*/fpu.rs`。

**与运行时路径的衔接点**：FPU 状态的 fork 继承（`EXT_REG_INITIALIZED` 标志 + 保存区整体复制）是 boot 期 FPU 策略（`fpu_policy` 枚举）与运行时路径（exec/signal/fork）的交汇点，详见 [17-syscall-process.md §1.1](./17-syscall-process.md) 与 [31](./31-fpu-context-switching.md)。

### 3.7 VM ELF 加载：free function 而非 trait 方法（对应 §2.3）

**C 结构**：`arch_boot_proc()` 内嵌的 VM ELF 加载——解析 ELF + 映射 PT_LOAD 段 + 复制字节 + 清零 BSS + 分配栈，经 libexec 框架（函数指针回调）分发。

**Rust 表达**：

```rust
pub fn load_vm_elf<P: Paging, A: PhysAccess>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
    vm_alloc: &mut VmBootAllocator,   // 一次性 VM image 帧分配器（从高地址切；细节见 frame.rs）
    access: &A,                       // PA → kernel VA（PhysAccess；生产 = CurrentDirectMap）
) -> Result<VmLoadResult, VmLoadError>
```

用 `minix_elf` crate 替代 C 的 `libexec`（7 回调框架与 Rust 的组织差异见 §4.2），用 `Paging::map()` 替代 `pg_map`。**VM Bootstrap Memory Handoff**（详见 [frame.rs](file:///os/arch/src/arch/frame.rs)）：物理帧由 `VmBootAllocator` 决定（消费 `VmBootRegion::select_multi(memmap, exclusions)` 验证区，按 `start` 降序的高地址分配），PA 与 ELF VA **解耦**——VM 的物理帧是 `VmBootAllocator` 选出的任意空闲帧，与 ELF 段的虚拟地址无任何约束关系（语义对齐 C 的 `PG_ALLOCATEME`：C 端 `pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo)` 由分配器挑帧，参见 [protect.c:379-384](file:///minix3/minix/kernel/arch/i386/protect.c#L379-L384)）。数据拷贝经 `PhysAccess`（`CurrentDirectMap` 的 `kernel_phys_to_virt`），每页先 zero 再 copy（`.bss`/partial page）。返回 `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`。归属 crate：`os/arch/src/arch/boot.rs`（arch crate 的 common 模块），因为它依赖 arch 拥有的 `Paging` trait。

**为什么这样表达**：三架构的加载逻辑**完全相同**——它只依赖 `Paging`/`PhysAccess` trait，不依赖任何架构特定寄存器操作。放 trait 是假多态（false polymorphism）：三实现逐行一致，改一处要改三处，trait 派发无意义且增加维护成本。free function 让"三架构共享"成为显式事实，修改只需一处。

**与 C 的差异**：加载结果相同（pc/sp/ps_strings/allocated_bytes 与 C 对齐，§5.1 测试验证）；差异在组织——C 的函数指针回调框架（libexec）→ Rust 泛型 free function，加载细节（ELF 解析/映射策略）→ 02-stage-vm 文档。

### 3.8 零堆启动的工程实现（§2.0 第三层的 Rust 落地）

**C 结构**：boot 期没有堆分配器（§2.0 第二层），C 用 `EXTERN` 数组落静态段，内核从第一条指令起经固定地址访问，所有数据结构编译期固定大小。

**Rust 表达（P1）**：

- `ProcessTable`/`PrivTable` 用 `[T; N]` 固定数组，不用 `Box<[T]>`/`Vec<T>`
- `ProcessTable::new()`/`PrivTable::new()` 是 `const fn`，编译期完成初始化
- `KProcess::new_zeroed()` 是 `const fn`（[proc.rs:1268](file:///os/kernel/src/proc.rs#L1268)），9 个内部类型（`AtomicI32`/`AtomicU64`/`ProcName`/`Endpoint`/`Option` 等）都需要 const-initable（Rust 1.75+ 支持）
- `[const { KProcess::new_zeroed() }; PROC_TABLE_SIZE]`（[proc_table.rs:75-76](file:///os/kernel/src/proc_table.rs#L75-L76)）在编译期构建完整数组，调用点 `static PROC_TABLE` 触发 const 上下文求值，运行期零开销

**kernel_may_alloc 窗口**：C 在 `bsp_finish_booting()` 中设 `kernel_may_alloc = 0`（[main.c:105](file:///minix3/minix/kernel/main.c#L105)，kmain 开始时置 1），关闭 boot 期内存分配窗口。Rust 用 `AtomicBool` 表达，boot 期为 `true`（允许内核直接分配物理内存），`bsp_finish_booting()` 后设 `false`（VM 接管内存管理）。

> **术语说明**：`const fn` 表达的是 **static compile-time initialization**——构造在编译期完成，运行期不再执行。**这不等于"BSS 段"**：是否最终落到 `.bss` / `.data` / `.rodata` 由对象的实际初始值和链接器决定。`KProcess::new_zeroed()` 内含 `AtomicI32` / `AtomicU32` / `Option<ProcNr>` / `Endpoint` 等非零初始字段，实际链接段可能是 `.data` 而非 `.bss`。因此准确表述是"static compile-time initialization, avoiding runtime construction"，而不是"BSS = compile-time initialization"。

**与 C 的差异**：存储形态一一对应（静态段固定地址、零运行期构造）；差异只在"链接器隐式布局 → 编译期显式 const 求值"，以及分配窗口的显式 `AtomicBool` 门控。

**假设性推理**：如果在 boot 期用 `Box<[KProcess]>`，`Box::new()` 调用 `GlobalAlloc::alloc`——minix-rs 当前在 `os/kernel/src/boot_alloc.rs` 中只实现了页表页的 bump allocator（`BootAlloc::alloc()` 第 L52-L62 行注释明确：仅分配 4 KiB 页、L57 `fetch_add(0x1000)` 步进、`return (PhysBytes(pa), VirBytes(pa))` 为 identity map 返回），**没有实现通用对象的 `GlobalAlloc`**（`boot_alloc.rs` 中没有 `impl GlobalAlloc for BootAlloc` 之类的代码）。直接调用 `Vec<KProcess>::new()` 会因无 `GlobalAlloc` 而 panic。所以 minix-rs 选择固定数组 + const fn 方案——这是与上游 Minix3 存储形态一一对应的设计选择（§2.0 三层因果链）。

### 3.9 SMP 预留设计

**心智模型前置**——先建立读者对 Minix3 调度器数据结构的认知，再讲并发协议。本节只搭**最少的认知骨架**，调度器数据结构与算法的完整展开见 §11-scheduling-primitives，BKL 类型系统强制的实现细节见 [16-smp.md](./16-smp.md)。

**就绪队列数据结构（先于并发协议）**——Minix3 把每个 CPU 各自的"待运行进程"组织成一条**侵入式链表**，链头在调度器数组里，链节点嵌入进程结构。定义两个最小概念：

- **`p_nextready: i32`**（C 端 `struct proc *`，proc.h:72）——`KProcess` 上的"链后继"字段，存**下一个就绪进程的 `ProcNr`（不是地址）**。这是 §3.3 末尾讨论的"裸指针 → 槽索引"转型的具体字段；详细为什么用 i32 不用裸指针见 [§3.3 关键反推段](#33-rts-位图设计bitflags--atomicu32--强类型不变量对应-216)。
- **`NONE_PROC_NR = -1`**——链尾哨兵。"下一个进程"指向 `NONE_PROC_NR` 表示这是链尾。
- **`run_q_head[NR_SCHED_QUEUES]`**（每个优先级一条链，[sched.rs:79](file:///os/kernel/src/sched.rs#L79)）——16 条链头的数组（每条对应一个优先级）。CPU 选下一个进程时看自己**所在 CPU 的 run_q_head**。

整条链表结构（CPU 视角）：

```text
CPU 的 run_q_head[priority]
   │
   ▼
┌──────────┐ p_nextready ┌──────────┐ p_nextready ┌──────────┐
│ Proc A   │ ──────────►│ Proc B   │ ──────────►│ Proc C   │
└──────────┘            └──────────┘            └──────────┘
  ▲                                              │
  └──── 同优先级进程串成一条链 ────────────────────┘
                                                   ▼
                                            (NONE_PROC_NR = -1)
```

接下来读 §3.3 末尾的 "`Relaxed` 模式解析" 时，"调度器遍历就绪链表"就有具体对象可想象了——`pick_proc` 就是沿着 `run_q_head[q] → p_nextready → p_nextready → ...` 找到第一个 runnable 的进程。

**并发模型（BKL 全局串行 + per-CPU runqueue）**——在上面的数据结构基础上，看 Minix3 与 Linux 的差异：

```text
Linux 的真并行调度：                    Minix3 的"假并行"调度（BKL 全局串行）：
  CPU0 → lock(rq0) → 调度 → unlock       CPU0 → BKL_LOCK → 调度 → BKL_UNLOCK
  CPU1 → lock(rq1) → 调度 → unlock       CPU1 ──── 等待 BKL（自旋）─────────────
       （同时进行）                        CPU1 → BKL_LOCK → 调度 → BKL_UNLOCK
```

关键理解点（**这是 §3.9 的核心论点**）：

- **Linux 的 `lock(rq0)`** 是 per-CPU runqueue 锁——CPU0 调度 CPU0 的队列时 CPU1 可以同时调度 CPU1 的队列，真正并行。
- **Minix3 的 BKL_LOCK** 是**全局** spinlock——任意时刻**只有一个 CPU 在调度**（即使它跑的是自己 per-CPU 队列的进程）。`BKL_LOCK` 在其他 CPU 上是"自旋等待"，不是"做别的事"。
- **Minix3 的 per-CPU runqueue 不是为了并行调度**——既然任意时刻只有一个 CPU 跑调度代码，并行度其实是 1。per-CPU runqueue 的真正价值是 **cache 局部性**：CPU0 选中的进程大概率还在 CPU0 的 L1/L2 cache 里（因为它之前在 CPU0 上跑过），跨 CPU 调度会强制 cache miss（CPU1 调度 CPU0 队列里的进程 → 该进程在 CPU0 的 cache 上 → CPU1 必须重新加载）。每个 CPU 跑自己队列上的进程是 cache-friendly 的妥协。

> **架构范围说明**：SMP 安全组合不止 BKL + atomics 一种——还存在 per-CPU locks / RCU / sequence lock / MCS lock / lock-free queue 等方案。本项目沿用 Minix3 的简化模型（BKL 全局串行调度决策 + per-CPU runqueue 实现 cache 局部性），故未引入这些方案。如果未来需要支持 BKL 之外的更细粒度并发（如 Linux-style 的 per-CPU rq lock），架构层会有相应演进。

**Rust 表达**——有了数据结构 + 并发模型这两个前置认知，Rust 的字段表达为什么是这样就清晰了：

- **`p_nextready: AtomicI32`**（[proc.rs:919](file:///os/kernel/src/proc.rs#L919)）——装的是 `ProcNr.0`（i32），不是 `*mut KProcess` 指针。这样选 i32 是因为：(a) 跨 CPU 共享时裸指针是 `!Sync`，编译期被 ban；(b) `AtomicI32` 比 `AtomicI32<ProcNr>` 在链遍历时零开销（`load → as_i32 → 比较`）。这是 §3.3 末段 [L1081 "类型选择"项](06-proc-init-boot-proc.md#L1081) 的实战字段。
- **`load(Ordering::Relaxed)`**（[sched.rs:153, 167](file:///os/kernel/src/sched.rs#L153)）——`pick_proc` 遍历链表时已经持 BKL，写者（`sched_enqueue/sched_dequeue`）同样持 BKL；同一时刻不存在"无锁并发读写同一 `p_nextready`"。所以 `Relaxed` 够——它只防"撕裂读 + 本字段读写乱序"，跨字段顺序由 BKL 提供（详见 §3.3 末段 [关键反推](06-proc-init-boot-proc.md#L1084)）。
- **`NONE_PROC_NR = -1` 是**链尾哨兵——遍历时遇到 `next == NONE_PROC_NR` 就停。这在 i32 字段上很自然（`-1` 永远不会是合法 `ProcNr`）；如果用裸指针，需要 `NULL` 或专门 sentinel，复杂度相同。

如果用 `Rc<RefCell<KProcess>>` 跨 CPU 共享，`RefCell` 的运行时借用检查不是原子操作，两个 CPU 可能同时获得 `&mut`，导致 UB——所以 Rust 路径要么走 `&mut`（持 BKL 内），要么走 `AtomicI32`（无锁读）。**BKL + AtomicI32** 组合是 Minix3 BKL 模型下的唯一可行办法（不引入 Linux 的 RCU / seqlock 等更细粒度机制）。

**类型系统强制**（细节 → [16-smp.md](./16-smp.md)）：`CpuLocal<T>: !Sync`（[smp.rs:137](file:///os/kernel/src/smp.rs#L137)）让 per-CPU 数据**编译期禁止跨 CPU 共享引用**，从根上消除 per-CPU 数据被并发访问的可能；`BklSection<'a>` typed witness（[smp.rs:867](file:///os/kernel/src/smp.rs#L867)）把"当前持有 BKL"从注释约定升级为编译期类型证明——需 BKL 的 API 以 `&BklSection<'_>` 为参数，自动拒绝"未持锁调用"。

**与 C 的差异**：并发模型与 C 完全一致（BKL + per-CPU runqueue）；差异只在 Rust 类型系统把 C 靠注释/纪律维护的约束（"per-CPU 数据不跨 CPU"、"持 BKL 才能调用"）变成编译期强制。

> **本节是概念索引**：详细 BKL/per-CPU/调度并行化的设计与代码见 [16-smp.md](./16-smp.md)（per-CPU 抽象 + BklSection witness 设计）；调度器算法（`pick_proc`/`sched_enqueue`/`sched_dequeue`）见 §11-scheduling-primitives。本章仅建立"Minix3 是 BKL 全局串行 + per-CPU 局部状态"的心智模型，避免读者在 boot 期误用 `Rc/RefCell`（SMP 跨 CPU UB）。

### 3.10 KPriv 8 子结构（索引，对应 §2.2.1–§2.2.6）

**C 结构**：`struct priv` 有 30+ 裸字段平铺（C 侧 6 组语义划分见 §2.2），缺乏内聚性；`grant_capability` 需在一堆无关字段中找到要写的 5 个能力字段。

**Rust 表达**：按 OS 语义分组为 **8 个子结构**（[kpriv.rs](file:///os/kernel/src/kpriv.rs)）：

`PrivIdentity` / `PrivFlags` / `PrivInit` / `PrivSignals` / `PrivIpc` / `PrivIo` / `PrivMem` / `PrivRuntime`（KPriv 字段序，同 kpriv.rs:400-409），每个子结构独立 `const fn new()` 构造。

**为什么这样表达（分组原则：读写时机 / 锁粒度）**：

- `PrivIdentity { s_proc_nr: Option<ProcNr>, s_id: SysId }`——身份域。boot 期 `assign_static` 一次性写入，运行期只读。
- `PrivFlags { s_flags: ProcessCapability }`——能力域（[capability.rs:67](file:///os/kernel/src/capability.rs#L67) newtype：C wire 位布局 + 组合位，见 22-privilege.md §4.1）。boot 期 `configure_boot_priv` 一次性写入 + 设备添加时增量更新（CHECK_* 位）。
- `PrivInit { s_init_flags: i32 }`——init 状态域。boot 期逐步清零，运行期恒为 0。
- 其余 5 个子结构（Signals/Ipc/Io/Mem/Runtime）按 §2.2.2–§2.2.6 的语义分组一一对应。

分组让"写身份只触碰 `identity`、写能力只触碰 `flags`、写 init 状态只触碰 `init`"成为视觉事实。**8 子结构的字段全集、构造函数与逐字段论证 → [22-privilege.md §4.2](./22-privilege.md)（权威展开，本文不重复）**。

**协议边界：两个层面的「一致」，约束强度不同**

跨空间传一份权限描述，要分别回答两个问题——答案不同，混为一谈会得出错误的契约：

**① bit 的含义（位值）**：`0x010` 是 SYS_PROC、`0x040` 是 CHECK_IRQ——位值必须内核与 RS 同一套，因为 `data_copy` 按裸字节搬运，两端对同一字节解释不同才是真正的破坏。严格说，**位值只要求 minix-rs 内部一致**（内核与 RS 都是本 workspace 的 Rust 代码，不与 C 版进程通信）。但本设计仍采 C 的位布局（`const.h:143-154`），三个理由按约束强度递减：
- **位值会被导出到用户态**：GET_WHOAMI 直接返回 `privflags = priv(caller)->s_flags`（C: do_getinfo.c:139），GET_PRIV/GET_PRIVTAB 经 `PrivInfoStruct` 导出 `s_flags`。观察者按 Minix3 语义解读这些值（IS 诊断、调试、对照 C 手册）——换一套位值即改变**外部可观测行为**，违反 Rewrite 契约。这是硬约束：要自造位值就得连导出路径一起换，纯支出零收益。
- **Ground truth 可追溯**：位值采 C 布局后，每个位常量的权威出处就是 const.h:143-154（Ground Truth 链顶端：C 源 > design > Rust）；自造布局则要自写文档当权威，多出一个可漂移源，review 无法对 C 源做 grep 对照。
- **零成本**：既然总得选一套，选 C 的使 `from_wire`/`to_wire` 退化为加宽/截断（无重映射表）；且 Rust RS 的 `PrivFlags` 本就按 const.h 位值定义（[privilege.rs:76](file:///os/servers/rs/src/privilege.rs#L76)），生态已收敛于此。

**② 结构体的字段排布**：这个**不**要求对齐 C。C `struct priv` 混有内核私有字段（`s_alarm_timer` 定时器节点、内部指针），本就不该整体暴露给用户态；`PrivUpdateRequest` 只装 `update_priv` 实际读取的字段，是 Rust 端独立定义的新协议结构。于是布局的硬契约只剩一条：**内核的定义 = RS 的填写 = `data_copy` 的大小，三方内部一致**。

剩下的字段顺序是 Rust 的自由度，本设计把它花在**镜像 KPriv 8 子结构序**上（去掉内核私有的 `PrivRuntime`）：

- `PrivIdentity`：`s_id`；
- `PrivFlags`：`s_flags`；
- `PrivInit`：`s_init_flags`；
- `PrivSignals`：`s_sig_mgr` / `s_bak_sig_mgr`——RS 只读写信号管理器，pending 簿记是内核私有；
- `PrivIpc`：`s_trap_mask` / `s_ipc_to` / `s_k_call_mask`；
- `PrivIo` + `PrivMem`：I/O → IRQ → 内存三组资源白名单，组内计数在表前（与 C `priv.h` 同惯例）。

镜像的收益在协议另一端兑现：RS 侧的填写结构 `Privilege`（[privilege.rs](file:///os/servers/rs/src/privilege.rs)）按同一顺序排列字段，内核定义与 RS 填写逐字段可对照，布局漂移在 review 中直接目检可见。

一句话总结：**位值跟 C（外部可观测 + ground truth），布局跟自己（两端都是 Rust）**。内核内部永远持类型化表示——`ProcessCapability` 低 11 位与 C 位布局 1:1，Rust 扩展位（KILL/SIGS_SYS/OWN_ID）放在 bit 16-18：u16 的 wire 物理装不下，`to_wire` 剥掉、`from_wire` 进不来，「内核私有语义」与「对外协议」被位宽天然隔开，无需运行时检查。`SysProcKind` enum 表达 4 选 1 互斥的 ROOT/VM/LU/RST 位、`is_sys_proc()`/`is_preemptible()`/`is_billable()` 谓词——这些只是读同一批位的类型层糖，不改变 wire 上是什么。

**与 C 的差异**：字段全集与语义一一对应（外部行为、IPC 协议位布局不变）；差异只在内部组织（平铺 → 按读写时机分组）与类型增强（谓词方法、互斥位 enum）。

### 3.11 调度字段与统计设计（对应 §2.1.2 / §2.1.5）

**C 结构**：进程的调度属性（`p_priority`/`p_quantum_size_ms`/`p_cpu`/`p_cpu_mask`/`p_scheduler`）和运行时统计（`p_accounting`/`p_cpuavg`/cycles 计数）是两类正交数据，但平铺在 `struct proc` 顶层。

**Rust 表达**：调度属性收进 `SchedFields` 子结构（[proc.rs:551](file:///os/kernel/src/proc.rs#L551)）：

```rust
pub struct SchedFields {
    pub priority: AtomicU8,        // 优先级（决定 enqueue 到哪个 NR_SCHED_QUEUES 子队列）
    pub quantum: Quantum,          // 时间片
    pub cpu: AtomicU32,            // 当前 CPU
    pub cpu_mask: CpuMask,         // CPU 亲和性位图（Default = 全 CPU 允许）
    pub scheduler: Option<ProcNr>, // 调度器归属（None = 内核默认调度，对应 C 的 NULL）
}
```

统计拆为 3 个子结构：`Accounting`（排队/出队计数，[proc.rs:596](file:///os/kernel/src/proc.rs#L596)）、`TimeStats`（CPU 时间，[proc.rs:678](file:///os/kernel/src/proc.rs#L678)）、`CyclesStats`（cycle 计数，[proc.rs:744](file:///os/kernel/src/proc.rs#L744)），全部原子字段。

**阶段 C 作用**（只讲初始化动作，运行时协议 → 11-scheduling-primitives）：

- **p_priority/p_quantum 覆写**：C 在 boot 循环中对 VM/RS 覆写 `p_priority=SRV_Q`/`p_quantum_size_ms=SRV_QT`（[main.c:209-210](file:///minix3/minix/kernel/main.c#L209-L210) VM / [main.c:232-233](file:///minix3/minix/kernel/main.c#L232-L233) RS），内核 task 不覆写（保持初值），非 schedulable 用户进程保持初值（等 RS 运行时设）。Rust 在 `init_proc_and_boot()` 的 Step 3b 中实现同样的覆写逻辑。
- **reset_proc_accounting**：C 在 boot 循环中调用 `reset_proc_accounting(rp)`（[main.c:186](file:///minix3/minix/kernel/main.c#L186)）重置统计。Rust 的 `Accounting`/`TimeStats`/`CyclesStats` 在 `KProcess::new_zeroed()` 中 const-init 为零值，boot 期无需额外重置——§2.1.5「阶段 C 仅清零」在 Rust 侧的对应表达。

**为什么这样表达**：调度属性是"每次调度决策都读"的热数据，统计是"记账累计"的冷数据；拆开后两者的读写时机差异显式化，也与 §2.1.2（阶段 C 触碰）/§2.1.5（阶段 C 仅清零）的概念分组对应。

**与 C 的差异**：数值与时机一致；差异在组织（平铺 → 调度/统计子结构分离）与初值方式（显式 reset 调用 → const 零值，少一次运行期函数调用）。

### 3.12 boot→running 转换设计

**C 结构**：阶段 C 结束时所有 boot 进程"就位但暂停"（带 `RTS_PROC_STOP`）；boot 流程的真正终点是 `bsp_finish_booting()` 清除停止标志、唤醒 boot 进程并入调度队列，随后切换到第一个用户态进程。

**Rust 表达**（[lib.rs:1698](file:///os/kernel/src/lib.rs#L1698)）：

```rust
fn bsp_finish_booting(
    proc_table: &mut ProcessTable,
    smp_state: &mut SmpState,
) -> ! {
    // Step 1: vm_running = false（VM 尚未接管内存管理）
    // Step 2: bill_ptr = proc_ptr = idle_proc（C: get_cpulocal_var(bill_ptr) = idle_proc）
    // Step 3: 唤醒 boot 进程：清除 RTS_PROC_STOP（rts_unset 自动 enqueue）
    //   for i in 0..(NR_BOOT_PROCS - NR_TASKS) {
    //       if let Some(proc) = proc_table.get_mut(ProcNr::from_user_index(i)) {
    //           proc.rts_unset(RtsFlags::PROC_STOP);
    //       }
    //   }
    // Step 4: kernel_may_alloc = false（关闭 boot 期内存分配窗口，§3.8）
    // Step 5: fpu_init（CPU 级 FPU 使能：CR4.OSFXSR / CPACR_EL1 / sstatus.FS，§3.6）
    // Step 6: enable_timer_irq（TimerIrqGate::enable_timer_irq，三架构硬件位）
    // Step 7: IRQ chain 注册（IrqManager::register_hook）
    // Step 8: kernel_may_alloc = false（关闭 boot 期内存分配窗口，§3.8）
    // Step 8.5: 获取 BKL（Big Kernel Lock，内核全局互斥自旋锁）——调度
    //          循环与中断路径共享的临界区边界（lib.rs:2095-2106）
    // Step 9: switch_to_user（切换到第一个用户态进程，→ 10-switch-to-user.md；
    //          trap frame 不在 boot 时预写——调度循环每次分派前从
    //          cpu_context 重建，见 10 §4.2）
}
```

**为什么这样表达**：

- **唤醒循环只遍历用户态 boot 进程**（`i < NR_BOOT_PROCS - NR_TASKS`），不含内核 task——内核 task 的 `RTS_PROC_STOP` 在 proc_init 中设置且永不清除（IDLE）或在 boot 循环中已处理。
- **trap frame 在分派时重建，而非 boot 时预写**：`cpu_context` 扮演 C 的 `p_reg` 角色——既是初始状态也是保存状态（trap entry 保存路径就绪后由其覆盖写入）。调度循环每次分派前用 `apply_to_trap_frame` 从 `cpu_context` 重建 trap frame（`finish_and_restore` 第 7 步，lib.rs:2689），因此"第一次运行"与"被中断后恢复"走同一条路径，不存在"预写 frame 无人保存"的悬空状态。这也是 §3.5 把 trait 命名为 `CpuContextArch`（而非 `BootArch`）的原因：`apply_to_trap_frame` 跨越 boot + runtime 两个阶段。
- 唤醒经 `rts_unset` 封装（§3.3 不变量强制）——清除标志与重新入队是原子语义，调用方无法"清了标志忘了入队"。

**与 C 的差异**：动作序列与 C 的 `bsp_finish_booting` 逐条对应；差异在类型表达（`rts_unset` 封装 enqueue 联动 vs C 的 RTS_UNSET 宏展开）与 `-> !` 类型化"不再返回"。

### 3.13 设计决策汇总表

| 决策 | 对应 Ch2 分组 | 约束 | 被排除方案 | 理由 |
|------|-------------|------|---------|------|
| 固定数组 `[KProcess; N]` + `const fn new()` | §2.1.0 / §2.2.0 | 零堆（P1） | `Box<[KProcess]>` / 运行时构造 | boot 期无 GlobalAlloc；static compile-time initialization（§3.1/§3.8） |
| `SyncUnsafeCell` 全局存储 | §2.1.0 | 零堆（P1） | `static mut` / `spin::Once` | C EXTERN 语义 + 零堆（§3.1） |
| `AtomicI32` 索引 / `Option<ProcNr>` 侵入链索引 | §2.1.3 | SMP（P3）+ 零堆（P1） | 裸指针 / `VecDeque` 堆队列 | 跨 CPU 共享需原子/索引；caller_q 侵入式 FIFO 同构保留（链头/尾在目标槽、后继在发送方槽），零堆（§3.1） |
| `ProcNr`/`Endpoint` newtype | §2.1.1 | 类型安全 | 裸 int | 编译期阻止"拿 p_nr 当 endpoint 用"（§3.0/§3.1） |
| `CapabilityTemplate` 枚举 | §2.2.1/§2.2.3 | 能力是数据（P6） | 6 裸参数 | 类型安全 + 角色映射显式（§3.2） |
| `RtsFlags(AtomicU32)` bitflags | §2.1.6 | SMP（P3） | `volatile u32` + 宏 | 类型化位操作 + BKL 并发协议（§3.3） |
| `ProcKind` + `EntrySpec` | §2.3 | 类型安全 | 散落 iskerneln 判断 | 决策收拢为类型，match 不可漏分支（§3.4） |
| 单 `CpuContextArch` trait | §2.1.4 | 真多态（P4） | 3 trait 镜像 C | 消除假多态，表达 OS 概念而非镜像 C 函数链（§3.5） |
| `CpuContext` 不透明关联类型 | §2.1.4 | 硬件抽象（P4） | 通用 `InitialRegState` | arch 私有，kernel 不 inspect；非法状态不可表达（§3.5） |
| FPU 下沉到 arch（[ARCH]） | §2.1.4 | FPU 演进（P5） | `fpu_needs_zero` 泄漏 | 现代 FXSAVE/CPACR/sstatus.FS（§3.6 → 31） |
| `enable_user_io` trait 方法 | §2.1.4 | 硬件下沉（P7） | kernel 操作 IOPL 位 | x86 特有语义不泄漏（§3.5） |
| free fn `load_vm_elf` | §2.3 | 真多态（P4） | trait 方法 | 三架构相同，假多态（§3.7 → 02-stage-vm） |
| BKL + per-CPU runqueue | §2.1.2 | SMP（P3） | per-CPU rq lock | Minix3 简化模型（§3.9 → 11/16） |
| `SchedFields` + 统计 3 子结构 | §2.1.2/§2.1.5 | 可读性 | 平铺 | 调度属性与统计分离（§3.11） |
| `KPriv` 8 子结构 | §2.2.1–§2.2.6 | 可读性 | 30+ 裸字段 | 按读写时机分组（§3.10 → 22 §4.2） |
| `Option<T>` 替代 flag | 全部分组 | 不变量表达 | always-embedded + bool | 编译器强制处理（§3.0） |

**无独立设计决策的分组**（沿用 C 形态，汇总一行带过）：

| Ch2 分组 | Rust 处理 | 运行时承接 |
|---------|----------|-----------|
| §2.1.5 记账/统计组 | `Accounting`/`TimeStats`/`CyclesStats` 字段对应 C，const 零值初始化（§3.11） | 11 §3 |
| §2.2.2 信号组 | `PrivSignals` 子结构字段对应 C（§3.10 → 22 §4.2）；阶段 C 仅清零 | 19 |
| §2.2.4 I/O 组 | `PrivIo` 子结构对应 C（§3.10）；阶段 C 仅挂接 | 20 |
| §2.2.5 内存组 | `PrivMem` 子结构对应 C（§3.10）；`s_ipcf`/`s_stack_guard` 为已知限制（→ 22 §4.2 D9） | 24/23 |
| §2.2.6 运行时组 | `PrivRuntime` 子结构对应 C（§3.10）；阶段 C 仅挂接 | 21/17 |

---

## Ch4. 实现详解（具体代码）

### 4.0 实现地图：init_proc_and_boot() 主流程

Rust boot 主流程入口是 `init_proc_and_boot(kernel_info: &KernelInfo)`（[lib.rs:812](file:///os/kernel/src/lib.rs)），对应 C 的 `proc_init()` + main.c boot image 循环。

```
init_proc_and_boot(kernel_info)
├── Step 1+2: 获取全局 PROC_TABLE / PRIV_TABLE（static mut BSS）
├── Step 2b: 校验 boot_modules.len() == NR_BOOT_MODULES
├── Step 3a: 遍历 KERNEL_TASKS（IDLE/CLOCK/SYSTEM/...）
│   ├── set_boot_name(name)
│   ├── grant_capability(nr, Idle|KernelTask)
│   ├── build_cpu_context(KernelTask, KERNEL_TASK)
│   └── RTS_SET(PROC_STOP); RTS_CLEAR(SLOT_FREE)
├── Step 3b: 遍历 boot_modules（boot image 顺序：DS/RS/PM/SCHED/VFS/...）
│   ├── nr = BOOT_MODULE_PROC_NRS[i]  （image 顺序 → C proc 号，映射表见 §1.4.1）
│   ├── set_boot_name(module.name)
│   ├── schedulable = is_root_sys || is_vm
│   ├── grant_capability(nr, Vm|RootService)  [if schedulable]
│   │   或 RTS_SET(NO_PRIV|NO_QUANTUM)          [if not]
│   ├── load_vm_elf(module, paging, vm_alloc, access) → EntrySpec::loaded  [if VM]
│   │   ├── mock:     VmBootAllocator::new(VmBootRegion::select_multi(memmap, exclusions)) + MockPaging + MockDirectMap
│   │   └── 非 mock:  VmBootAllocator::new(VmBootRegion::select_multi(memmap, exclusions))
│   │                 + CurrentPaging::from_active_root(current_root_phys())
│   │                 + X86_64DirectMap/AArch64DirectMap/Riscv64DirectMap（PhysAccess）
│   │                 （VM VA → 分配的 PA，非 identity；exclusions = kernel 镜像 + boot modules；frame 分配的语义对齐见下方"VM Bootstrap Memory Handoff 的语义对齐"段）
│   ├── add_memmap(module.start, module.len)     [reclaim：mock + 非 mock 都执行，C: protect.c:450-451]
│   ├── p_seg.phys_root/virt_root = bootstrap 根  [非 mock，供 init_post_and_memory 断言有效 + set_current_ptproc_nr 安装 VM 为 ptproc]
│   ├── build_cpu_context(proc_kind, entry)
│   ├── RTS_SET(VMINHIBIT|BOOTINHIBIT)  [if not VM]
│   └── RTS_SET(PROC_STOP); RTS_CLEAR(SLOT_FREE)
└── Step 4: boot_procs 信息已在 kernel_info 中
```

后续 `bsp_finish_booting()`（[lib.rs:1806](file:///os/kernel/src/lib.rs)）负责唤醒：RTS_UNSET(PROC_STOP) 循环 + 时钟/FPU 初始化 + `switch_to_user()`。

### 4.1 arch 层：CpuContextArch trait 实现（§3.5 设计的落地）

`CpuContextArch` trait（[boot.rs:145](file:///os/arch/src/arch/boot.rs)）是 kernel 层与 arch 层的 CPU 状态抽象接口（与 `CpuLocalArch` / `InterruptController` 等其他 trait 并列）。三架构各自实现：

**x86_64**（[x86_64/boot.rs](file:///os/arch/src/x86_64/boot.rs)）：

```rust
pub struct X86_64CpuContext {
    pub(super) psw: u64,   // RFLAGS 初值（INIT_PSW / INIT_TASK_PSW）
    pub(super) cs: u64,    // USER_CS_SELECTOR
    pub(super) ds: u64,    // USER_DS_SELECTOR
    pub(super) ss: u64, pub(super) es: u64, pub(super) fs: u64, pub(super) gs: u64,
    pub(super) rip: u64,   // entry.pc
    pub(super) rsp: u64,   // entry.sp
    pub(super) rbx: u64,   // entry.ps_strings（argv 指针）
    fpu_policy: X86FpuInitPolicy,  // KernelTask | LazyUserInit
    /// GP register save area for signal handling (RAX, RCX, RDX, RSI, RDI, RBP, R8-R15).
    /// Indexed by `X86_64GpReg` constants. Updated by trap entry path
    /// and read/written by `SignalContext`. Length: GP_REGS_LEN = 14.
    pub(super) gp_regs: [u64; X86_64CpuContext::GP_REGS_LEN],
}
```

`build_cpu_context` 根据 `ProcKind` 选 `INIT_PSW`（用户）或 `INIT_TASK_PSW`（内核 task），填段选择子，设 FPU 策略。`enable_user_io` 设 PSW.IOPL=3（x86 特有，驱动需直接 IN/OUT）。`inherit_fpu_state` 复制父进程 `fpu_policy` 给子进程。

**aarch64**（[arm64/boot.rs](file:///os/arch/src/arm64/boot.rs)）：

```rust
pub struct AArch64CpuContext {
    pub psr: u64,            // INIT_PSR / INIT_TASK_PSR
    pub pc: u64,             // entry.pc
    pub sp: u64,             // entry.sp
    pub r0: u64,             // entry.ps_strings
    pub fpu_enable_el0: bool, // CPACR_EL1.FPEN 位（per-process）
    /// GP register save area for signal handling (X1-X30).
    /// Indexed by `AArch64GpReg` constants (0 = X1, ..., 29 = X30/LR).
    /// Updated by trap entry path and read/written by `SignalContext`. Length: GP_REGS_LEN = 30.
    pub(super) gp_regs: [u64; AArch64CpuContext::GP_REGS_LEN],
}
```

`build_cpu_context` 根据 `ProcKind` 设 `INIT_PSR`（EL0 用户）或 `INIT_TASK_PSR`（EL1 内核 task）。用户进程 `fpu_enable_el0=true`，内核 task `false`。无 `enable_user_io`（ARM 用 MMIO 映射替代 IOPL）。

**riscv64**（[riscv64/boot.rs](file:///os/arch/src/riscv64/boot.rs)）：

```rust
pub struct Riscv64CpuContext {
    pub sstatus: u64,  // INIT_USER_SSTATUS / INIT_TASK_SSTATUS
    pub sepc: u64,     // entry.pc
    pub sp: u64,       // entry.sp
    pub a0: u64,       // entry.ps_strings
    /// GP register save area for signal handling (X1, X3-X9, X11-X31).
    /// Indexed by `Riscv64GpReg` constants. X0 hardwired zero, X2 (sp)
    /// and X10 (a0) are named fields. Updated by trap entry path and
    /// read/written by `SignalContext`. Length: GP_REGS_LEN = 30.
    pub(super) gp_regs: [u64; Riscv64CpuContext::GP_REGS_LEN],
}
```

`build_cpu_context` 设 `sstatus.SPP`（1=内核 task，0=用户）和 `SPIE`（用户=1）。FPU 通过 `sstatus.FS` 字段控制（`Initial` 状态，首次 FP 指令 trap 到内核做 lazy init）。

### 4.2 arch 层：load_vm_elf 共享实现

`load_vm_elf` 是 free function（[boot.rs:296](file:///os/arch/src/arch/boot.rs)），非 trait 方法——三架构实现字节相同，放 trait 是假多态。

```rust
pub fn load_vm_elf<P: Paging, A: PhysAccess>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
    vm_alloc: &mut VmBootAllocator,   // 一次性 VM image 帧分配器（frame.rs VM Bootstrap Memory Handoff）
    access: &A,                       // PA → kernel VA（生产 = CurrentDirectMap）
) -> Result<VmLoadResult, VmLoadError>
```

流程：用 `minix_elf` crate 解析 ELF → 逐页 `VmBootAllocator::alloc_page()` 取物理帧 → `PhysAccess` 转 kernel VA → **先 zero 再 copy**（`.bss`/partial page）→ `Paging::map(vm_va → pa)` 建立 VM 映射（**非 identity**：`VmBootAllocator` 从高地址切帧，所以 `paddr ≠ vaddr`）→ 分配用户栈（VM_STACK_SIZE=64KB，帧也来自 `VmBootAllocator`）→ 按 C 的 `arch_boot_proc`（protect.c:410-427）在栈顶布置 ps_strings 并填充内容 → 返回 `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`。

> **VM Bootstrap Memory Handoff 的语义对齐**：C `PG_ALLOCATEME`（[protect.c:379-384](file:///minix3/minix/kernel/arch/i386/protect.c#L379-L384)）让分配器从空闲列表挑选物理帧，`pg_alloc_page`（[pg_utils.c:138-160](file:///minix3/minix/kernel/arch/i386/pg_utils.c#L138-L160)）由 `mmap_size-1` 向下扫 memmap、从每段末尾返回；Rust `VmBootAllocator`（[frame.rs](file:///os/arch/src/arch/frame.rs)）复现"按物理地址降序跨段 bump"的同一行为，每段首帧 = `region.end - PAGE_SIZE`。`VmBootRegions` 由 `select_multi` 一次切片 exclusions（区段间互不相交、按 `start` 降序），与 C `cut_memmap`（[pre_init.c:190-214](file:///minix3/minix/kernel/arch/i386/pre_init.c#L190-L214)）对应。完整机制说明（不变量、const generic 容量、与 C 字段级对应、为何高地址优先）见 [`frame.rs` 模块 doc-comment](file:///os/arch/src/arch/frame.rs)。

**初始栈布局**（与 C 逐字节一致）：`struct ps_strings` 放在 `stack_high - 32`（LP64 下四字段 8+4+8+4，按 8 字节对齐补足为 32B）；初始 SP 再下移 20 字节（两个指针 + 一个 int）到 `stack_high - 52`，对应启动代码的 argc/argv/envp 三个字。四个字段按 C 赋值：`ps_argvstr = sp + 4`（= stack_high - 48）、`ps_nargvstr = 0`、`ps_envstr = ps_argvstr + 8`（= stack_high - 40）、`ps_nenvstr = 0`——VM 启动时既无命令行参数也无环境变量，所以两个计数字段为 0。ps_strings 的地址经寄存器交给进程（x86-64 `rbx` / aarch64 `r0` / riscv64 `a0`，见 §1.2.2），启动代码据此定位参数。

ELF 段标志映射：`PF_R|PF_W|PF_X` → `PageFlags::PRESENT | USER_ACCESSIBLE | WRITABLE | EXECUTABLE`（[boot.rs](file:///os/arch/src/arch/boot.rs) `elf_flags_to_page_flags`）。

错误处理：`VmLoadError::InvalidElf`（ELF 无法解析）/ `OutOfMemory`（`VmBootAllocator` 帧耗尽）/ `MappingFailed`（paging 失败），返回 `Result`，无静默失败。

**与 C libexec 框架的组织差异**：C `libexec_load_elf`（`minix3/minix/lib/libexec/`）经 7 个函数指针回调（copymem / clearmem / allocmem×3 / clearproc / memmap，统一接收 `struct exec_info *`）分发内存操作，且**不按 PT_LOAD 过滤**——[exec_elf.c:185](file:///minix3/minix/lib/libexec/exec_elf.c#L185) 以 `if(!(ph->p_flags & PF_R))` 按 flags 而非 type 分支处理所有 phdr。Rust `load_vm_elf` 用 `minix_elf` 直接解析、按 PT_LOAD 过滤后调 `Paging`——回调框架消除，段过滤显式收紧（有意选择的实现差异；加载结果与 C 对齐，§5.1 验证）。libexec 框架逐字段详解已从本文档移除（C 通用 exec 框架，非阶段 C 主线），深读见 `minix3/minix/lib/libexec/{libexec.h,exec_elf.c}`。

**真实 VM ELF 加载**（mock + 非 mock 路径都接通）：非 mock 路径通过 `Paging::from_active_root(current_root_phys())` 包装 `arch_boot_impl` 创建并激活的 bootstrap 页表，将 VM ELF 段直接映射进去（帧来自 `VmBootAllocator`，VM VA → 分配的 PA，**非 identity**；`CurrentDirectMap` 作为 `PhysAccess` 提供数据拷贝的 kernel VA；`PG_ALLOCATEME` 的语义对齐见上一段"VM Bootstrap Memory Handoff 的语义对齐"）。加载完成后 module 物理内存立即通过 `memmap::add_memmap` 回收（撤销 kmain 阶段的 `cut_memmap` 临时切除）。VM 的 `p_seg.phys_root`/`virt_root` 也同步记录为 bootstrap 根，使随后的 `init_post_and_memory` 能将 VM 安装为 ptproc（`set_ptproc` + `set_current_ptproc_nr`）。VMCTL SetAddrSpace 在 VM 安装自己的页表后会通过 `TlbArch::set_active_root` 替换硬件根寄存器。**ISA scope**：

| ISA | 硬件根寄存器 | 角色 |
|-----|-------------|------|
| x86-64 | CR3 | root PD 物理地址（4 级页表） |
| aarch64 | TTBR0_EL1 | EL0/EL1 translation table base |
| riscv64 | satp | supervisor-mode 地址翻译寄存器（Sv39/Sv48） |

详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)。

**boot module 物理内存回收**（mock + 非 mock 路径）：`load_vm_elf` 成功返回后，`init_proc_and_boot` 立即调 `memmap::add_memmap(FREE_MEMMAP, module.start, module.len)` 把 module 的物理内存归还给 kernel 分配器（[lib.rs:996 mock](file:///os/kernel/src/lib.rs) / [lib.rs:1083 非 mock](file:///os/kernel/src/lib.rs)）。C 对照：`protect.c:450-451` `mod->mod_start = mod_end = 0` 标记已消费。这撤销了 kmain 阶段的 `cut_memmap` 临时切除（[lib.rs:353](file:///os/kernel/src/lib.rs)），让 module 物理页重新可用。详见 [01-boot-shim-bootstrap.md §2.5 boot module 内存生命周期](01-boot-shim-bootstrap.md)。

### 4.3 kernel 层：ProcessTable 与 PrivTable

**ProcessTable**（[proc_table.rs:57](file:///os/kernel/src/proc_table.rs)）：

```rust
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: VmRequestQueue,
}
```

`const fn new()`（[proc_table.rs:75](file:///os/kernel/src/proc_table.rs)）：BSS 零初始化 + per-slot 设 `p_nr`/`p_endpoint` + IDLE slot 特殊处理（`PROC_STOP` + name="IDLE"）。全局 `static PROC_TABLE: SyncUnsafeCell<ProcessTable>`（BSS），通过 `crate::proc_table()` 获取。

**为什么 `ProcessTable` 能放在 `static` 里？**：`ProcessTable` 含裸指针与 `Cell` 等 `!Sync` 字段，Rust 默认拒绝它跨线程共享。但 `static PROC_TABLE` 必须能被多核访问——这条规则不能违反。`SyncUnsafeCell<ProcessTable>` 通过 `unsafe impl Sync` 强行声明它"线程安全"——但这是 blanket 实现，**任何 `T` 都能被 `SyncUnsafeCell` 包裹**，包括 `RefCell<T>`、`Rc<T>`、`Cell<T>` 这种**真**不安全的类型，绕过 Rust 的并发检查。修复方式：把 `unsafe impl Sync` 收成"白名单"——`unsafe impl<T: BklProtected> Sync for SyncUnsafeCell<T>`，其中 `BklProtected` 是 sealed trait（[lib.rs `bkl_protected` 模块](file:///os/kernel/src/lib.rs)），只有本 crate 显式 `impl` 的类型才算通过审计。白名单 8 个类型：**BKL 串行化组** = `ProcessTable`/`PrivTable`/`IrqManager`/`SmpState`/`IpcFilterPool`/`KRandomness`（访问前必须抢 BKL，所以多线程看似共享实则串行）；**write-once-read-only 组** = `KernelInfo`/`MemMapEntry`（boot 期单线程初始化，运行期永远只读）。任何其他类型——`RefCell`、`Rc`、`Cell` 或外部 crate 自定义类型——都被编译期拒绝装进 `static`。零运行时开销：`BklProtected` 是 marker trait，无方法无字段，纯粹是编译期资格证明。

关键方法：`get(nr) -> Option<&KProcess>` / `get_mut(nr)` / `is_valid_nr(nr)` / `is_kernel(nr)` / `is_empty(nr)` / `rts_set` / `rts_unset`（自动维护调度队列）。

**PrivTable**（[kpriv.rs](file:///os/kernel/src/kpriv.rs)）：

```rust
pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}
```

`const fn new()`：per-slot 设 `s_id`，`s_proc_nr=None`。关键方法：`assign_static(proc_nr) -> Option<PrivId>` / `configure_boot_priv(...)` / `grant_capability(proc_nr, template) -> Result<PrivId, CapabilityError>`。

SMP 安全：所有方法要求持有 BKL（boot 期单线程，无需锁）。

### 4.4 kernel 层：KProcess 结构（对应 §2.1 分组 + §3.1 设计）

`KProcess`（[proc.rs:850](file:///os/kernel/src/proc.rs)）字段按语义分组：

| 分组 | 字段 | C 对应 | 资源语义 |
|------|------|--------|---------|
| 标识 | `p_nr`, `p_endpoint`, `p_name` | proc.h | 身份引用（non-owning） |
| 状态 | `p_rts_flags` (RtsFlags bitflags), `p_misc_flags` (MiscFlags bitflags) | p_rts_flags, p_misc_flags | 状态（可 reset） |
| 调度 | `p_sched: SchedFields { priority, quantum, cpu, cpu_mask, scheduler }` | p_priority, p_quantum_size_ms, p_cpu, p_cpu_mask, p_scheduler | 状态（可 reset） |
| 统计 | `p_accounting: Accounting`, `p_time: TimeStats`, `p_cycles: CyclesStats`, `p_cpuavg: CpuAvg` | p_accounting, p_user_time, p_cycles, p_cpuavg | 状态（可 reset） |
| IPC | `p_nextready` (AtomicI32), `caller_q_head`/`caller_q_tail` (Option\<ProcNr\>), `send_q_link` (Option\<ProcNr\>), `p_getfrom_e`, `p_sendto_e` | p_nextready, p_caller_q（链头，proc.h:73）, p_q_link（链后继，proc.h:74）, p_getfrom_e, p_sendto_e | 全部为槽索引/端点（身份引用，零堆）；侵入链布局同构——链头在目标槽、后继在发送方槽（C 语义，proc.c:953-954）；`caller_q_tail` 是 Rust 的 O(1)-append 扩展，C 走链到尾（O(n)），FIFO 顺序不变 |
| VM | `p_seg: ProcessSegments`, `priv_id: Option<PrivId>` | p_seg, p_priv | 身份引用（non-owning） |
| FPU | `fpu_state: CurrentFpuState`（arch 私有类型，per-arch 512/528/264B，保存区内嵌在 KProcess） | C 是两级间接：`p_seg.fpu_state`（`char *`，i386 archtypes.h:35）指向 arch 层静态池 `fpu_state[NR_PROCS][FPU_XFP_SIZE]`（arch_system.c:144）；详见 §3.6 / §4.9 | **C 指针 + 池 → Rust 内嵌 buffer**（消除一级间接，消除"指针指向池但池从未独立分配"的假所有权；零初始化、无资源所有权） |

**所有字段均为 trivial-droppable**——`KProcess` 的手写 `Drop` 只做一件事：占用槽（`SLOT_FREE` 已清除）被隐式销毁时 `panic!` 报警，**不执行任何资源释放**（fd / inode / endpoint 对象在 user-space server）。槽位清理走显式 `dispatch_clear` 协议、槽位交换走 `core::mem::swap` 位搬运（详见 §3.1.1），**不依赖隐式析构**。这正是 `[KProcess; N]` 静态数组能安全复用的前提（§3.1.1）——报警器只捕获"协议被绕过"，不替代协议本身。

**fork_from**（[proc.rs:1502](file:///os/kernel/src/proc.rs)）：运行时路径（非 boot 路径），创建子进程：
- 继承调度属性（priority/quantum/cpu/cpu_mask）与 IPC 端点（`p_getfrom_e`/`p_sendto_e`）
- 重置 accounting/time/cycles/cpuavg（子进程不继承父进程 CPU 时间统计）
- 队列指针独立（`p_nextready` = NONE、`caller_q_head`/`caller_q_tail`/`send_q_link` = None，子进程未入队也不在任何目标的发送者队列——`p_q_link` 同构为 `send_q_link` 槽索引，fork 路径清 None 等价 C 的置 NULL）
- RTS 修正：设 `NO_QUANTUM`，清 `SIGNALED`/`SIG_PENDING`/`P_STOP`/`VMREQUEST`
- MiscFlags 修正：清 `VIRT_TIMER`/`PROF_TIMER`/`SC_TRACE`/`SPROF_SEEN`/`STEP`
- FPU 继承：`EXT_REG_INITIALIZED` 置位时调 `inherit_fpu_state`（arch trait 方法）复制初始化策略，并复制 `fpu_state` 保存区（`CurrentFpuState: Copy`，等价 C 的 `memcpy`；衔接点分析见 §3.6 与 17-syscall-process §1.1）

### 4.5 kernel 层：能力授予（grant_capability）

`grant_capability`（[kpriv.rs:995](file:///os/kernel/src/kpriv.rs#L995)）是"分配+配置"原子操作，替代 C 的 `get_priv` + 散落 `s_flags`/`s_trap_mask` 赋值：

```rust
pub fn grant_capability(
    &mut self,
    proc_nr: ProcNr,
    template: CapabilityTemplate,
) -> Result<PrivId, CapabilityError>
```

5 模板（[capability.rs:128](file:///os/kernel/src/capability.rs)）：

| 模板 | s_flags | trap_mask | ipc_to | k_call_mask | 用途 |
|------|---------|-----------|--------|-------------|------|
| `Idle` | IDL_F (SYS_PROC\|BILLABLE) | NONE | NONE | NONE | IDLE 进程 |
| `KernelTask` | TSK_F (SYS_PROC) | NONE | NONE | NONE | CLOCK/SYSTEM 等 |
| `Vm` | VM_F\|SRV_F (= SYS_PROC\|VM_SYS_PROC\|PREEMPTIBLE) | ALL | ALL | ALL | VM 进程 |
| `RootService` | RSYS_F\|SRV_F (= SYS_PROC\|PREEMPTIBLE\|ROOT_SYS_PROC) | ALL | ALL | ALL | RS 进程 |
| `Deferred` | — | NONE | NONE | NONE | 延迟分配（运行时由 RS 配置） |

> **注**：`BILLABLE` 仅 `IDL_F` / `USR_F` 模板需要，VM 和 RS 都是 `SYS_PROC`，不参与用户态计费（参见 [capability.rs:102-114](file:///os/kernel/src/capability.rs#L102-L114) `ProcessCapability` 组合位定义——IDL_F/TSK_F/SRV_F/DSRV_F/RSYS_F/VM_F/USR_F 是 C `priv.h:36-49` 组合的 OR，非独立位）。`SRV_F = SYS_PROC|PREEMPTIBLE`，`RSYS_F = SRV_F|ROOT_SYS_PROC`，`VM_F = SYS_PROC|VM_SYS_PROC`。

> **注**：`Vm`/`RootService` 的 `trap_mask = ALL` 对应 C 的 `SRV_T = ~0`（[main.c:204-217](file:///minix3/minix/kernel/main.c#L204-L217)）——VM/RS 开机即需 IPC 握手（SENDREC），掩码必须全 1。`ALL` 为 `u32::MAX`，wire 边界 `TrapMask::to_wire()` 截为 `u16` 的 `0xFFFF`（-1 补码），`from_wire()` 再符号扩展回全 1。
>
> **trap_mask 的三段设计**（运行时检查位于 [ipc.rs:1586-1596](file:///os/kernel/src/ipc.rs#L1586-L1596)）：
> 1. **wire 边界一次符号扩展**：C 的 `short s_trap_mask` 在每次读取时经 `short` → `int` 整型提升（proc.c:552），`0xFFFF` 因此在 int 宽度下覆盖 SENDA（位号 16）。Rust 把提升收敛到 `TrapMask::from_wire`（`u16 → i16 → u32` 符号扩展），存储即 C 的有效形式，`to_wire()` 截断可逆；
> 2. **运行时检查用 `TrapMask::contains(1 << call)`**：位号是运行时值 `call_nr`，无命名位可走 bitflags 的枚举位；`contains` 语义同 C 的 `& (1 << call)`，且类型上三种掩码不可互换（22-privilege.md Ch3 D5）；
> 3. **wire 宽度只保留在 `PrivUpdateRequest`**：跨空间 IPC 字段 `s_trap_mask: u16`、`s_ipc_to: u64`、`s_k_call_mask: [u32; 2]` 保持与 C 一致的裸宽度；编解码仅发生在 kpriv.rs `update_from_request`（内核入方向）与 misc.rs `PrivInfoStruct::from_kpriv`（导出方向）。
>
> **注**：表中 `KernelTask`/`Idle` 的 `trap_mask = NONE` 是**模板角色默认值**，对应 C 的 `TSK_T = 0`（[priv.h:60](file:///minix3/minix/include/minix/priv.h#L60)，"other kernel tasks"）；CLOCK/SYSTEM 的 `CSK_T`（`1 << RECEIVE`，[priv.h:59](file:///minix3/minix/include/minix/priv.h#L59)）不在模板里，由 `grant_capability` 按 `proc_nr` 逐实例覆写为 `RECEIVE`——与 C `main.c:218-219` 的三元选择 `proc_nr == CLOCK || SYSTEM ? CSK_T : TSK_T` 同构。两层职责的完整论证见 §3.2「trap_mask 的两层职责」。

内部流程：`assign_static(proc_nr)` → 查模板 `capabilities()`/`trap_mask()`/`ipc_mask()`/`kcall_mask()` → `configure_boot_priv`。重复分配返回 `Err(SlotOccupied)`。

### 4.6 kernel 层：KPriv 8 子结构（按 §3.10）

`KPriv` 按 Minix3 `struct priv` 语义分 **8 子结构**。分组"为什么"（读写时机 / 锁粒度）→ §3.10；字段全集与逐字段论证 → [22-privilege.md §4.2](./22-privilege.md)（权威，本文不重复字段清单）。

| 子结构（KPriv 字段序） | 定义（kpriv.rs） | C 对应（priv.h） |
|------------------------|------------------|------------------|
| `PrivIdentity` | [kpriv.rs:93](file:///os/kernel/src/kpriv.rs#L93) | `s_proc_nr` / `s_id`（:22-23 头两字段） |
| `PrivFlags` | [kpriv.rs:148](file:///os/kernel/src/kpriv.rs#L148) | `s_flags`（:24） |
| `PrivInit` | [kpriv.rs:130](file:///os/kernel/src/kpriv.rs#L130) | `s_init_flags`（:25） |
| `PrivSignals` | [kpriv.rs:223](file:///os/kernel/src/kpriv.rs#L223) | 信号簿记（:40-45）+ 异步发送表并入（:28-32） |
| `PrivIpc` | [kpriv.rs:265](file:///os/kernel/src/kpriv.rs#L265) | trap / ipc-to / k-call 三掩码（:34-38） |
| `PrivIo` | [kpriv.rs:290](file:///os/kernel/src/kpriv.rs#L290) | I/O + IRQ 白名单（:53-54, 59-60） |
| `PrivMem` | [kpriv.rs:317](file:///os/kernel/src/kpriv.rs#L317) | 内存白名单（:56-57）+ `s_ipcf`（:46）/ `s_stack_guard`（:49）/ `s_diag_sig`（:51） |
| `PrivRuntime` | [kpriv.rs:349](file:///os/kernel/src/kpriv.rs#L349) | alarm 定时器（:48）+ grant/state 表（:61-65） |

定义锚点按文件内位置排列，与 KPriv 字段序（identity → flags → init → …，kpriv.rs:400-409）不必同序。每子结构有 `const fn new()`，支撑 `PrivTable::new()` 编译期初始化（[22-privilege.md §4.3](./22-privilege.md)）。

### 4.7 主流程：init_proc_and_boot()

`init_proc_and_boot`（[lib.rs:812](file:///os/kernel/src/lib.rs)）的 Step 3a/3b 对称结构：

**Step 3a**（内核 task）：遍历 `KERNEL_TASKS`（编译期硬编码），每步：
1. `set_boot_name(name)` — 设进程名
2. `grant_capability(nr, Idle|KernelTask)` — 分配特权（模板默认 `TSK_T`=NONE，CLOCK/SYSTEM 的 `CSK_T`→RECEIVE 例外由 grant_capability 按 `proc_nr` 施加，见 §4.5 注 / §3.2）
3. `build_cpu_context(KernelTask, KERNEL_TASK)` — arch 构建 CPU 上下文
4. `proc.set_boot_cpu_context(cpu_context)` — 将 arch-private 上下文写入 KProcess
   （arch-private 字段如 `fpu_policy` / `gp_regs` 存放在 KProcess 内部 buffer，
    kernel 层不 inspect；详见 §4.1）
5. `RTS_SET(PROC_STOP)` + `RTS_CLEAR(SLOT_FREE)` — 标记已占用但暂停

**Step 3b**（用户进程）：遍历 `boot_modules`，每步：
1. `set_boot_name(module.name)`
2. `schedulable = is_root_sys || is_vm` — 判定可调度性
3. 可调度 → `grant_capability(nr, Vm|RootService)`；不可调度 → `RTS_SET(NO_PRIV|NO_QUANTUM)`
4. VM（mock + 非 mock）→ `load_vm_elf` → `EntrySpec::loaded`；其他 → `EntrySpec::DEFERRED`
5. `build_cpu_context(proc_kind, entry)` — arch 构建 CPU 上下文
6. `proc.set_boot_cpu_context(cpu_context)` — 将 arch-private 上下文写入 KProcess
7. 非 VM → `RTS_SET(VMINHIBIT|BOOTINHIBIT)` — 等 VM 建页表
8. `RTS_SET(PROC_STOP)` + `RTS_CLEAR(SLOT_FREE)`

`schedulable` 判定对应 C `iskerneln(proc_nr) || isrootsysn(proc_nr) || proc_nr == VM_PROC_NR`（main.c:196）。

### 4.8 boot→running：bsp_finish_booting

`bsp_finish_booting`（[lib.rs:1836](file:///os/kernel/src/lib.rs)）是 boot 流程终点，类型 `-> !`（never returns）：

```
bsp_finish_booting(proc_table, smp_state) -> !
├── Step 1: VM_RUNNING.store(false)          — VM 尚未运行
├── Step 2: proc_table.set_bill_to_idle()    — bill_ptr/proc_ptr = IDLE
├── Step 3: EarlyConsole::write_str(banner)  — announce()
├── Step 4: for nr in 0..NR_BOOT_PROCS-NR_TASKS:
│           proc_table.rts_unset(nr, PROC_STOP)  — 唤醒 boot 进程
├── Step 5: let bsp_id = smp_state.bsp_cpu_id();
│           if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
│               bsp_local.note_context_switch(tsc);  — cycles_accounting_init
│           }                                          (防御性 if let：cpu_locals 是
│                                                       MAX_CPUS 固定数组，BSP 恒命中)
├── Step 6: clock_arch.init_timer(DEFAULT_HZ, current_cpuid().raw())  — boot_cpu_init_timer
│           (第二参数是 per-CPU ID，每个 CPU 初始化自己的定时器)
├── Step 7: bsp_local.fpu_presence = true     — fpu_init
├── Step 8: KERNEL_MAY_ALLOC.store(false)     — 内核不再分配内存
├── Step 8.5: core::mem::forget(smp::bkl_lock())  — RAII guard 必须 forget
│           (BklGuard 在 Drop 时释放 BKL，但 BKL 必须跨 switch_to_user()
│            持有——若 let guard 自然 drop，会在 switch_to_user 前提前释放
│            BKL，违反 C: BKL_LOCK()/switch_to_user() 的语义不变性)
└── Step 9: switch_to_user()                  — 首次调度，不返回
```

Step 4 的 `rts_unset` 自动将新就绪进程加入调度队列（[proc_table.rs:304](file:///os/kernel/src/proc_table.rs) 判"非就绪→就绪"转换后调 `sched_enqueue`（:314），后者经 `Scheduler::enqueue_queue_tail` 入队）。Step 9 `switch_to_user()` 在 09 文档详解。C 侧 12 步完整对照与差异处置 → [08-system-init-boot-finish.md §4](./08-system-init-boot-finish.md)。

### 4.9 FPU 实现细节（§3.6 设计论证的实现侧）

> §3.6 讲「为什么不用 fnsave/fxrstor 翻译路径」（设计论证 + [ARCH] 标注）；本节讲「Rust 侧 FPU 状态到底长什么样」——类型定义、字节数、内存预算与 trait 实现位置。逐函数机制分析见 [31-fpu-context-switching.md](./31-fpu-context-switching.md)。

**类型定义链**：kernel 层仅可见的类型别名 `CurrentFpuState`（[lib.rs:216-222](file:///os/arch/src/lib.rs#L216-L222)，经 `minix_arch` 再导出），与 `CurrentCpuContext` 共同构成 kernel 层可见的全部 arch 类型；真实类型在 arch crate 内部按 `#[cfg(target_arch)]` 编译期选择，kernel 层零 `#[cfg]`：

```rust
// os/arch/src/lib.rs — 三选一（编译期）
pub type CurrentFpuState = crate::x86_64::fpu::X86_64FpuState;    // x86_64
pub type CurrentFpuState = crate::arm64::fpu::AArch64FpuState;    // aarch64
pub type CurrentFpuState = crate::riscv64::fpu::Riscv64FpuState;  // riscv64
```

**三架构保存区布局与字节数**（类型均为 `Copy` + `Default`——整体复制即 fork 继承语义，见 §4.4；全零即合法初值，见 [fpu_arch.rs:59-65](file:///os/arch/src/arch/fpu_arch.rs#L59-L65)）：

| 架构 | 类型 | 布局 | 字节 | 对齐 | 定义位置 |
|------|------|------|------|------|---------|
| x86-64 | `X86_64FpuState` | `[u8; 512]`（FXSAVE 固定布局） | 512 | 16 | [fpu.rs:37](file:///os/arch/src/x86_64/fpu.rs#L37) |
| aarch64 | `AArch64FpuState` | 32×Q 寄存器（`[[u64; 2]; 32]`）+ FPSR + FPCR + pad | 528 | 16 | [fpu.rs:38](file:///os/arch/src/arm64/fpu.rs#L38) |
| riscv64 | `Riscv64FpuState` | 32×f 寄存器（`[u64; 32]`）+ FCSR + pad | 264 | 8 | [fpu.rs:37](file:///os/arch/src/riscv64/fpu.rs#L37) |

三类型都 `#[repr(C)]` + 显式对齐：FXSAVE/FXRSTOR 与 FPSIMD load/store 要求 16 字节对齐，RISC-V 的 `fsd/fld` 只需 8 字节自然对齐。对齐由 `repr` 静态保证，`save`/`restore` 的 unsafe 块以它为 SAFETY 论据（[fpu.rs:99-100](file:///os/arch/src/x86_64/fpu.rs#L99-L100)）。

**C 侧存储组织对照（指针 + arch 层池 → 内嵌值）**：C（i386）的保存区不在 proc 结构内——`p_seg.fpu_state` 是 `char *` 指针（[archtypes.h:35](file:///minix3/minix/include/arch/i386/include/archtypes.h#L35)），`arch_proc_reset` 把用户进程的指针指到 arch 层静态池 `fpu_state[NR_PROCS][FPU_XFP_SIZE]` 的 `p_nr` 槽位并清零，内核 task 指针保持 NULL（[arch_system.c:144-168](file:///minix3/minix/kernel/arch/i386/arch_system.c#L144-L168)）。Rust 把「指针 + 池」重组织为「内嵌值」：`KProcess.fpu_state: CurrentFpuState`（[proc.rs:990](file:///os/kernel/src/proc.rs#L990)），261 槽每槽都嵌一块保存区。语义不变量保持——每用户进程一份保存区、切换时保存/恢复、fork 整体复制（C `memcpy` [do_fork.c:67](file:///minix3/minix/kernel/system/do_fork.c#L67) ↔ Rust `Copy` [proc.rs:1678](file:///os/kernel/src/proc.rs#L1678)）。差异：Rust 的内核 task 也带一块恒零保存区（`fpu_policy = KernelTask` 永不使用）——用户不可见，无外部行为差异；数据结构重组织属 rewrite 允许范围（§3.0）。

**`KProcess.fpu_state` 内存预算**：per-process 保存区随架构变化，进程表 FPU 总开销（261 槽，§2.0）：

| 配置 | Rust：内嵌 261 槽 | C（i386）对照 |
|------|------------------|---------------|
| x86-64 | 261 × 512B ≈ 130KB | 池 256 × 512B = 128KB（独立于 proc 数组）+ 每进程 8B 指针 |
| aarch64 | 261 × 528B ≈ 135KB | 无 C 对照（[ARCH] 现代模型，§3.6） |
| riscv64 | 261 × 264B ≈ 67KB | 无 C 对照（[ARCH] 现代模型，§3.6） |

x86-64 总量与 C 同量级（130KB vs 128KB）；三种配置都仍是编译期定容（§2.0 第一/三层），不引入任何堆分配。组织差异是「按 `p_nr` 间接绑定 arch 层池」到「随进程表静态布局直接内嵌」。

**Linux `thread_struct` 对照**（外部参照，示意性对照——Linux 源码不在本仓库，未逐字段核对）：

| 架构 | Minix-RS | Linux 侧锚点 |
|------|----------|-------------|
| x86-64 | `X86_64FpuState`（512B FXSAVE 区） | `thread_struct` 内 FPU 保存区（`struct fxregs_state`，fxsave 512B） |
| aarch64 | `AArch64FpuState`（32×Q + FPSR/FPCR） | `struct fpsimd_state`（`arch/arm64/include/uapi/asm/ptrace.h`；代码注释显式引用，[fpu.rs:34-35](file:///os/arch/src/arm64/fpu.rs#L34-L35)） |
| riscv64 | `Riscv64FpuState`（32×f + FCSR） | `struct __riscv_d_ext_state`（`arch/riscv/include/uapi/asm/ptrace.h`，f[32] + fcsr） |

共同模型：per-process 一整块 FPU 保存区 + 上下文切换时保存/恢复。差异：Linux 现代 x86-64 用可变长 XSAVE 系列（按 CPU 特性动态布局），Minix-RS 固定 512B FXSAVE——与 Minix3 的 osfxsr 路径一致（§3.6 表格），换取静态定容的可预算性。

**`FpuArch` trait 与三架构实现**：trait 定义 [fpu_arch.rs:59](file:///os/arch/src/arch/fpu_arch.rs#L59)——`type State` 关联类型 + 7 方法（`init` / `save` / `restore` / `enable` / `disable` / `disable_exception` / `is_present`），实现者是无状态 ZST：

| 架构 | 实现类型 | 位置 | save/restore 指令 |
|------|---------|------|------------------|
| x86-64 | `X86_64FpuArch` | [fpu.rs:69](file:///os/arch/src/x86_64/fpu.rs#L69) | `fxsave` / `fxrstor` |
| aarch64 | `AArch64FpuArch` | [fpu.rs:86](file:///os/arch/src/arm64/fpu.rs#L86) | `stp` / `ldp` q0–q31（FPSIMD） |
| riscv64 | `Riscv64FpuArch` | [fpu.rs:79](file:///os/arch/src/riscv64/fpu.rs#L79) | `fsd` / `fld` f0–f31 |

调用点与 boot 期行为：

- **运行期唯一 trait 调用点**：SMP 迁移的 SAVE_CTX 路径（[smp.rs:519-524](file:///os/kernel/src/smp.rs#L519-L524)，持 BKL）——`EXT_REG_INITIALIZED` 且本 CPU 持有 FPU 所有权时 `disable_exception()` + `save(&mut p.fpu_state)`，随后清 `fpu_owner`（C 对照 `smp.c:173-176` disable_fpu_exception/save_local_fpu/release_fpu）。
- **boot 期（阶段 C）**：不触碰保存区内容——`KProcess` 构造时 `fpu_state` 只是零初始化（[proc.rs:1255, 1299](file:///os/kernel/src/proc.rs#L1255)，`CurrentFpuState::default()`/`new()`）；per-arch 初始化策略（`fpu_policy` / `fpu_enable_el0` / sstatus.FS）由 `build_cpu_context` 写入 CpuContext（§4.1），首次 FP 指令的惰性初始化属运行时路径（[31-fpu-context-switching.md](./31-fpu-context-switching.md)）。
- **BSP FPU 存在性探测**：`bsp_finish_booting` Step 7 直接置 `bsp_local.fpu_presence = true`（[lib.rs:1812-1825](file:///os/kernel/src/lib.rs#L1812-L1825)，三目标架构均有 FPU；C 对照 `fpu_init` 写 per-CPU `fpu_presence`，读方 `is_fpu()` [main.c:518](file:///minix3/minix/kernel/main.c#L518-L521)）。
- **待确认**：`FpuArch::init`（对应 C `fpu_init` 的 CR0/CR4 硬件使能部分）在当前 kernel boot 路径未接线（全项目仅测试调用 [fpu_arch.rs:210](file:///os/arch/src/arch/fpu_arch.rs#L210)）；硬件使能时机需对照 arch 启动代码核实（to confirm，不在本文档范围）。

---

## Ch5. 测试要点

### 5.0 测试策略

**三层测试架构**：

| 层级 | 测试目标 | 测试方式 | 设计原则 |
|------|---------|---------|---------|
| **arch 层** | CpuContextArch trait 实现 + load_vm_elf | 单元测试（每架构独立） | 三架构对照，覆盖 FPU 策略差异 |
| **kernel 层** | ProcessTable/PrivTable/KProcess/RtsFlags | 单元测试（mock arch） | rts_set/rts_unset 队列一致性是隐藏不变量 |
| **集成层** | ProcessTable 初始状态 + VM slot 存在性 | 集成测试（`boot_integration.rs`）| 验证 boot 起始状态（**未实际跑 init_proc_and_boot 全流程**，仅验 ProcessTable::new 后状态） |

boot 期 panic（`assert_eq!` / `expect`）vs 运行时 `Result`：boot 期错误是不可恢复的（配置错误），用 panic；运行时错误可恢复，用 `Result`。

### 5.1 arch 层测试

**x86_64**（[x86_64/boot.rs](file:///os/arch/src/x86_64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_psw` | KernelTask 的 PSW=INIT_TASK_PSW，cs/ds=USER_CS/DS_SELECTOR |
| `user_process_uses_init_psw` | Vm 的 PSW=INIT_PSW，rip/rsp/rbx 从 EntrySpec::loaded 填入 |
| `enable_user_io_sets_iopl` | `enable_user_io` 后 PSW.IOPL=3（0x3000） |
| `default_ctx_is_kernel_task_shape` | `Default::default()` 全零，有意义值来自 build_cpu_context |
| `apply_to_trap_frame_copies_registers` | CpuContext → TrapFrame 寄存器复制正确 |
| `inherit_fpu_state_propagates_lazy_user_policy` | fork 时子进程继承父进程 fpu_policy |

**aarch64**（[arm64/boot.rs](file:///os/arch/src/arm64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_psr` | KernelTask 的 PSR=INIT_TASK_PSR，fpu_enable_el0=false |
| `user_process_uses_init_psr_and_fpen_user` | Vm 的 PSR=INIT_PSR，fpu_enable_el0=true |
| `all_user_kinds_get_fpen_user` | Vm/RootService/UserService/UserProcess 均 fpu_enable_el0=true |
| `default_ctx_is_zeroed` | Default 全零 |
| `inherit_fpu_state_propagates_fpu_enable_el0` | fork 时子进程继承父进程 fpu_enable_el0 |

**riscv64**（[riscv64/boot.rs](file:///os/arch/src/riscv64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_sstatus` | KernelTask 的 sstatus=INIT_TASK_SSTATUS，SPP=1 |
| `user_process_uses_init_sstatus` | Vm 的 sstatus=INIT_USER_SSTATUS，SPP=0，SPIE=1 |
| `default_ctx_is_zeroed` | Default 全零 |
| `inherit_fpu_state_copies_sstatus` | fork 时子进程继承父进程 sstatus |

**load_vm_elf**（[arch/boot.rs](file:///os/arch/src/arch/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `entry_spec_kernel_task_all_none` | KERNEL_TASK 的 pc/sp/ps_strings 全 None |
| `entry_spec_deferred_all_none` | DEFERRED 的 pc/sp/ps_strings 全 None |
| `entry_spec_loaded_sets_all_some` | loaded() 的 pc/sp/ps_strings 全 Some |
| `elf_flags_rwx_to_page_flags` | PF_R\|PF_W\|PF_X → PRESENT\|USER\|WRITABLE\|EXECUTABLE |
| `elf_flags_r_only` | PF_R → PRESENT\|USER，无 WRITABLE/EXECUTABLE |
| `load_vm_elf_invalid_elf_returns_err` | 无效 ELF 返回 `Err(InvalidElf)` |
| `load_vm_elf_out_of_memory_returns_err` | `VmBootAllocator` 帧耗尽返回 `Err(OutOfMemory)` |
| `load_vm_elf_places_stack_and_ps_strings_like_c` | 最小 ELF 加载后 sp=stack_high-52、ps_strings=stack_high-32，且 ps_strings 四字段已按 C 填充（经 mock `PhysAccess` 的帧内容验证） |
| `load_vm_elf_zeroes_bss_and_copies_filesz` | `memsz > filesz`：`.bss` 区为 0、file 区为原字节；`Paging` 映射 VM VA → 分配的 PA（VA≠PA） |

**memmap cut_memmap**（[memmap.rs](file:///os/kernel/src/memmap.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_cut_memmap_no_overlap` | cut 范围不重叠任何 entry → 无变化 |
| `test_cut_memmap_full_overlap` | cut 范围完全覆盖 entry → entry 被清除 |
| `test_cut_memmap_prefix_split` | 切除 entry 头部 → suffix 写回 |
| `test_cut_memmap_suffix_split` | 切除 entry 尾部 → prefix 写回 |
| `test_cut_memmap_middle_split` | 切除 entry 中段 → prefix + suffix 都写回 |
| `test_cut_memmap_spans_multiple_entries` | cut 跨越两个 entry → 各自 prefix/suffix 写回 |
| `test_cut_memmap_alignment_round_down_start` | 非对齐 start 向下取整（cut 扩大） |
| `test_cut_memmap_alignment_round_up_end` | 非对齐 end 向上取整（cut 扩大） |
| `test_cut_memmap_zero_length_no_op` | 零长度 cut 为 no-op |
| `test_cut_memmap_preserves_total_free_memory` | cut 后总空闲内存减少量 = cut 大小（无泄漏） |

### 5.2 kernel 层测试

**ProcessTable**（[proc_table.rs](file:///os/kernel/src/proc_table.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_process_table_new` | const fn 可编译，所有 slot SLOT_FREE |
| `test_process_table_const_init_per_slot_nr` | per-slot p_nr 正确（-NR_TASKS..） |
| `test_process_table_const_init_idle_name` | IDLE slot name="IDLE" |
| `test_process_table_idle` | IDLE slot 有 PROC_STOP 标志 |
| `test_is_valid_nr` | is_valid_nr 边界检查 |
| `test_is_kernel` | is_kernel: nr<0 为 true |
| `test_rts_set_unset` | rts_set/rts_unset 维护调度队列一致性 |
| `test_rts_set_unset_multiple_flags` | 多标志位 set/unset |
| `test_rts_set_idempotent` | 重复 set 同一标志为 no-op |

**PrivTable / grant_capability**（[kpriv.rs](file:///os/kernel/src/kpriv.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_priv_table_new` | const fn 可编译，per-slot s_id 正确 |
| `test_priv_table_const_init_sets_per_slot_s_id` | s_id 按 slot 索引递增 |
| `test_priv_table_assign_static` | assign_static 分配 priv slot |
| `test_priv_table_assign_static_user_proc` | 用户进程 assign_static |
| `test_priv_table_configure_boot_priv` | configure_boot_priv 设 flags/masks |
| `test_grant_capability_idle` | Idle 模板：IDL_F flags，三个 mask 全 NONE |
| `test_grant_capability_vm` | Vm 模板：VM_F flags，trap_mask/ipc_to/k_call_mask 全 ALL（SRV_T = ~0） |
| `test_grant_capability_root_service` | RootService 模板：RSYS_F flags，trap_mask/ipc_to/k_call_mask 全 ALL（SRV_T = ~0） |
| `test_grant_capability_deferred_no_flags` | Deferred 模板：无 flags，三个 mask 全 NONE |
| `test_grant_capability_duplicate_fails` | 重复分配返回 `Err(SlotOccupied)` |

**KProcess / fork_from**（[proc.rs](file:///os/kernel/src/proc.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_kprocess_new` | new_zeroed 全零 |
| `test_boot_module_proc_nrs_match_c_comh` | `BOOT_MODULE_PROC_NRS` 12 项逐一等于 C `com.h` proc 号（§1.4.1 映射表）；gen=0 endpoint == proc 号；RS/VM 常量一致 |
| `test_kprocess_runnable` | 无 PROC_STOP 时 runnable |
| `test_rts_flags_runnable` | RtsFlags runnable 判定 |
| `test_rts_flags_multiple` | 多标志位组合 |
| `test_sched_fields_new` | SchedFields 初始化 |
| `test_accounting_new` | Accounting 初始化 |
| `test_fork_from_basic` | fork 基本继承 |
| `test_fork_from_accounting_reset` | 子进程 accounting 重置 |
| `test_fork_from_independent_queues` | 子进程队列指针独立 |
| `test_fork_from_inherits_ipc_endpoints` | 继承 IPC 端点 |
| `test_fork_from_cycles_reset` | 子进程 cycles 重置 |
| `test_fork_from_rts_flags_corrections` | RTS 标志修正 |
| `test_fork_from_misc_flags_corrections` | MiscFlags 修正 |
| `test_fork_from_inherits_fpu_state_when_initialized` | 父进程已用 FPU（`EXT_REG_INITIALIZED`）时，子进程继承标志并复制 `fpu_state` 保存区（等价 C i386 `memcpy`；mock 下保存区为 ZST，逐字节复制由 `CurrentFpuState: Copy` 保证） |
| `test_fork_from_zeroes_fpu_state_when_parent_unused` | 父进程未用 FPU 时，子进程保存区保持零化、不置标志 |

### 5.3 集成测试

**init_proc_and_boot**（[boot_integration.rs:139](file:///os/kernel/tests/boot_integration.rs#L139)）：

| 测试名 | 验证点 |
|--------|--------|
| `init_proc_and_boot_test` | ProcessTable 创建 + VM slot 存在 + p_seg 默认状态 + p_magic(PMAGIC) |

**bsp_finish_booting**（[lib.rs](file:///os/kernel/src/lib.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_bsp_finish_booting_step_5_7_side_effects` | Step 5 cycles_accounting + Step 7 fpu_presence |
| `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | 单 CPU 下仅 BSP 初始化，AP CPU 保持默认 |

### 5.4 测试覆盖矩阵

> **知识点编号说明**：原矩阵使用"A.8/B.8/C.5/D.7/E.8/F.5/H.2/L.1/M.5/N.9"等编号，源自早期 outline 草案的全局概念索引，但该索引在当前 doc tree 中**未保留**（99-global-concepts.md 无对应条目）。reader 凭这些编号无法回溯到任何doc 章节。为避免误导，下表改用 doc 06 自身的章节定位（§3.x 设计决策 + §4.x 实现详解）。

| 设计决策 / 实现要点 | 对应章节 | 测试用例 | 层级 | 状态 |
|---------------------|---------|---------|------|------|
| ProcessTable 设计 | §3.1 / §4.3 | `test_process_table_new` / `test_process_table_const_init_per_slot_nr` / `test_process_table_const_init_idle_name` | kernel | ✅ |
| KProcess 结构 | §4.4 | `test_kprocess_new` / `test_kprocess_runnable` | kernel | ✅ |
| PrivTable 设计 | §3.2 / §4.3 | `test_priv_table_new` / `test_priv_table_const_init_sets_per_slot_s_id` | kernel | ✅ |
| CapabilityTemplate 5 模板 | §3.2 / §4.5 | `test_grant_capability_idle` / `test_grant_capability_vm` / `test_grant_capability_root_service` | kernel | ✅ |
| boot 期 grant_capability 授予 | §4.5 / §4.7 | `test_grant_capability_deferred_no_flags` / `test_grant_capability_duplicate_fails` | kernel | ✅ |
| RtsFlags bitflags + 调度队列 | §3.3 | `test_rts_set_unset` / `test_rts_set_unset_multiple_flags` / `test_rts_set_idempotent` | kernel | ✅ |
| init_proc_and_boot 主流程 | §4.0 / §4.7 | `init_proc_and_boot_test` | 集成 | ✅ |
| 三架构 CpuContext 构建 | §4.1 | `kernel_task_uses_init_task_psw` / `kernel_task_uses_init_task_psr` / `kernel_task_uses_init_task_sstatus` | arch | ✅ |
| enable_user_io（x86 特有）| §4.1 | `enable_user_io_sets_iopl` | arch | ✅ |
| load_vm_elf ELF 加载 | §4.2 | `load_vm_elf_invalid_elf_returns_err` / `load_vm_elf_out_of_memory_returns_err` / `load_vm_elf_places_stack_and_ps_strings_like_c` / `load_vm_elf_zeroes_bss_and_copies_filesz` / `elf_flags_rwx_to_page_flags` | arch | ✅ |
| fork_from 运行时继承 | §4.4（衔接点：§3.6 / 17 §1.1） | `test_fork_from_basic` / `test_fork_from_accounting_reset` / `test_fork_from_independent_queues` / `test_fork_from_inherits_fpu_state_when_initialized` / `test_fork_from_zeroes_fpu_state_when_parent_unused` | kernel | ✅ |
| CapabilityError 错误路径 | §3.0 / §4.5 | `test_grant_capability_duplicate_fails` | kernel | ✅ |
| SchedFields 调度字段 | §3.11 / §4.4 | `test_sched_fields_new` | kernel | ✅ |
| bsp_finish_booting 唤醒流程 | §4.8 | `test_bsp_finish_booting_step_5_7_side_effects` / `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | kernel | ✅ |

**测试统计（截至 2026-08-17）**：

- arch 层单元测试（x86_64/arm64/riscv64 + arch/boot.rs）：**22 个**（§5.1 所列 boot 上下文相关；x86_64/boot.rs 全文 11 个、另 5 个 stacktrace 测试未列入，四文件实际共 27 个）
- kernel 层单元测试（proc_table.rs + kpriv.rs + proc.rs）：**33 个**（§5.2 所列；三文件实际共 92 个，§5.2 只列与 doc 06 相关的代表性测试）
- 集成测试（boot_integration.rs + lib.rs tests 模块）：**3 个**（boot_integration.rs 全文 2 个只列 1 个；lib.rs tests 模块共 36 个，只列 2 个 bsp_finish_booting 相关）
- memmap 辅助测试（cut_memmap 系列）：**10 个**（memmap.rs 全文 16 个，另 6 个 add_memmap 测试未列入）
- **§5.1–§5.3 所列测试总数：68 个**（22 + 33 + 3 + 10）

`os/kernel/src/` 与 `os/arch/src/` 中完整的测试函数清单可由 `cargo test --workspace` 输出获取——§5.1/§5.2/§5.3 只列与 doc 06 直接相关的代表性测试，并非全量清单（如 proc.rs 共 40 个测试，§5.2 只列 16 个）。

---

## Ch6. 参见

**前置文档**：
- [03-kmain-cstart](./03-kmain-cstart.md) — kmain/cstart 的早期 boot 流程（Phase A/B）
- [05-clock-interrupt-init](./05-clock-interrupt-init.md) — 时钟与中断初始化（bsp_finish_booting Step 6 依赖）

**后续文档**：
- [07-cross-space-init](./07-cross-space-init.md) — 跨地址空间初始化（VM 建页表后唤醒其他进程）
- [08-system-init-boot-finish](./08-system-init-boot-finish.md) — SYSTEM 进程初始化与 boot 收尾
- [09-vm-boot-protocol](./09-vm-boot-protocol.md) — VM 协商与 VMCTL 协议（06 只讲 boot 期加载，协议细节在 09）
- [10-switch-to-user](./10-switch-to-user.md) — switch_to_user 首次调度（bsp_finish_booting Step 9）

**运行时承接文档**（06 建好表、设好初值；运行时读写归它们）：
- [11-scheduling-primitives](./11-scheduling-primitives.md) — 调度原语 enqueue/dequeue/pick_proc；RTS→队列联动细节
- [12-ipc-core](./12-ipc-core.md) — IPC 六原语、SENDING/RECEIVING 状态机、delivermsg
- [17-syscall-process](./17-syscall-process.md) — fork/exec/exit/clear 的字段读写与 slot 生命周期
- [19-syscall-signal](./19-syscall-signal.md) — 信号管理器与信号投递（priv 信号组运行时）
- [20-syscall-device](./20-syscall-device.md) — I/O 端口/IRQ 授权的运行时使用
- [21-syscall-clock](./21-syscall-clock.md) — 闹钟定时器（priv 运行时组 s_alarm_timer）
- [22-privilege](./22-privilege.md) — priv 字段语义、三类掩码、CapabilityTemplate 深度展开
- [23-ipc-filter](./23-ipc-filter.md) — IPC 过滤（s_ipc_to / s_k_call_mask 运行时检查）
- [24-cross-space-runtime](./24-cross-space-runtime.md) — 跨空间运行时（s_mem_tab / s_ipcf 使用）
- [31-fpu-context-switching](./31-fpu-context-switching.md) — FPU 状态管理逐函数机制（06 §3.6/§4.9 只讲设计演进与实现形态）

**专题文档**：
- [00-kernel-overview](./00-kernel-overview.md) — 内核整体架构与 BKL/SMP 模型

### 6.1 职责边界矩阵

「先总后分」约定的可视化锚点——06 负责存储与初值，运行时行为逐项转交：

| 问题 | 06 | 11 | 12 | 17 | 22 | 其他 |
|------|----|----|----|----|----|------|
| proc/priv/RTS 表怎么存（C 形态 + Rust 表达） | ✅ | | | | 引用 | |
| 字段有哪些、语义分组索引 | ✅ 导航 | | | | | |
| RTS 不变量（runnable iff 0） | ✅ 口号 | ✅ 联动细节 | | | | |
| enqueue/dequeue/pick_proc | | ✅ | | | | |
| IPC 状态机 / delivermsg | | | ✅ | | | |
| fork/exec/exit 修改 proc 字段 | | | | ✅ | | |
| endpoint generation 递增 | | | | ✅ | | |
| priv 字段语义 / 三类掩码 | 引用 | | | | ✅ | |
| boot image 概念与编号表 | ✅ | | | | | 01/08/09 细节 |
| ELF 加载细节 | 骨架 | | | | | 02-stage-vm |
| VM boot protocol | 引用 | | | | | 09 |
| switch_to_user / 首次切换 | 引用 | | | | | 10 |
| FPU 架构演进 | 设计+实现形态 | | | | | 31 逐函数机制 |
