# 06 — 进程表初始化与 boot 进程加载

> **阶段**：C（kernel stage）/ 阶段 C
> **范围**：进程表/特权表/RTS 清空与填充、boot image 遍历、VM ELF 加载、boot→running 转换
> **ground truth**：Minix3 源码 `minix3/minix/kernel/{proc.c,main.c,system.c,protect.c,arch/*}`
> **Rust 实现**：`os/kernel/src/{proc.rs,proc_table.rs,kpriv.rs,capability.rs,lib.rs}` + `os/arch/src/{x86_64,arm64,riscv64}/boot.rs`
> **前置文档**：[03-kmain-cstart.md](./03-kmain-cstart.md)、[05-clock-interrupt-init.md](./05-clock-interrupt-init.md)
> **后续文档**：[07-cross-space-init.md](./07-cross-space-init.md)、[08-system-init-boot-finish.md](./08-system-init-boot-finish.md)、[10-switch-to-user.md](./10-switch-to-user.md)

---

## Ch1. 概述（概念导向，建立心智模型）

> **本章目标**：从 CPU/OS 视角回答"阶段 C 要解决什么问题"。读者读完本章应理解：进程本质、为什么需要进程表/特权表/RTS 标志、VM 鸡生蛋问题、CPU 要回答的四个问题、boot 流程的起点与终点。**本章不讲函数名、不讲 Rust 类型**——这些从 Ch2 开始。

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

#### 1.1.4 三类运行态实体（User Process / System Server / Kernel Subsystem）

> 架构范围：**三架构共性**（仅 Ring 特权级跨架构名称不同：x86-64 ring/EL/privilege mode；本质均为"谁跑 Ring 0 共用 kernel image"）

§1.1.3 的"五部分组成"对所有进程一视同仁——但 Minix3 实际上把**运行态实体**分成三类，**机制完全不同**。这个区分是早期读者最大的认知陷阱之一，也是 [05-clock-interrupt-init.md](05-clock-interrupt-init.md) 讨论 timer 时的关键背景（CLOCK task 与普通 timer 硬件不同层）。本节正式确立三分法。

**三分法总表**：

| 类别 | 特权级 | 地址空间 | Minix3 实例 | minix-rs 翻译 |
|------|--------|---------|-------------|---------------|
| **User Process（用户进程）** | Ring 3 | 独立虚拟地址空间（CR3/satp 切换） | shell、编译器、所有用户应用 | `ProcessKind::User { ... }` |
| **System Server（系统服务器）** | Ring 3 | 独立虚拟地址空间 | PM / VM / RS / VFS / DS / INET 等 | `ProcessKind::Server { ... }` |
| **Kernel Subsystem（内核子系统）** | Ring 0 | **共用 kernel image**（无独立地址空间概念；不切换 CR3） | CLOCK task、SCHED task、SYSTASK、KERNEL | `ProcessKind::KernelSubsystem { subsystem: KernelSubsystemKind, ... }` |

**四项关键澄清**（最易错的概念）：

1. **"System Task"在 Minix3 术语里严格指 Kernel Subsystem**——许多教学材料（包括部分 Minix 文档）把 PM/VM/RS 称为"系统任务"，其实是误用术语。**PM/VM/RS 的真名是"系统服务器"**（system server）或"用户态特殊进程"——它们在 Ring 3 运行，与普通用户进程机制相同，只是通过 `priv` 特权表获得特殊权限。
2. **只有 Kernel Subsystem 跑 Ring 0**——SYSTASK（内核调用分发）+ CLOCK task（timer handler）两个**共用 kernel memory address space**（来自 [Minix 3 I/O 论文](http://sedici.unlp.edu.ar/bitstream/handle/10915/125322/Documento_completo.pdf-PDFA.pdf?isAllowed=y&sequence=1)）。
3. **Kernel Subsystem 不是"页表切换"的进程**——它从来不离开 kernel image，永远在 Ring 0；调度器进入 kernel Subsystem 时**不动 CR3**，只保存/恢复通用寄存器和少量特权寄存器（SP/RFLAGS 之类）。
4. **三类进程在 §1.1.3 "五部分组成"中某些字段不同**：
   - **VM 状态**：User/Server 有，Kernel Subsystem 没有（共用 kernel image，**不需要页表**）
   - **IPC 状态**：User/Server 之间通过消息传递；Kernel Subsystem 通过直接函数调用
   - **调度属性**：Kernel Subsystem 的 priority 与 User/Server 不互通——CLOCK task 优先级最高，SCHED task 决定其他进程的调度顺序
   - **寄存器状态**：Kernel Subsystem 不需要保存 SS/RSP 全栈（kernel 已经常驻）

**与本文档（阶段 C）的关系**：阶段 C 处理的主要是 System Server（VM 是第一个被装载的；FS/PFS/... 在阶段 E 中由 RS 服务拉起）和 Kernel Subsystem（CLOCK task、SCHED task、SYSTASK 这些其实就是内核子系统，不是从 boot image 装载的 ELF）。

> **本节在全文中的位置**：本节建立"三类运行态实体"的概念底座。后续 [11-scheduling-primitives.md](./11-scheduling-primitives.md) 给出调度器完整设计；[16-smp.md](./16-smp.md) 给出 SMP 下 Kernel Subsystem 的并发模型；本章 §1.4 boot image 仅引用本节对 System Server/Kernel Subsystem 的区分。

### 1.2 CPU 要回答的四个问题

> **与 03 文档的关系**：本文档用"四问"框架；[03-kmain-cstart.md §1.4](./03-kmain-cstart.md) 用"三问"框架。03 三问 = "当前特权级？异常/syscall 跳哪？用哪个栈？"，由 `prot_init()` 回答。**本文第四问（VM 鸡生蛋 / 第一个进程地址空间谁建）是阶段 C 特有的"运行态特化"问题，不属 03 保护结构范畴**——它是"进程首次进入 Ring 3 之前需要由内核手工准备的地址空间"，与 §1.4 "CPU 怎么进入 kernel"的特化方向相反（§1.4 是 user→kernel，本节第四问是 kernel→user 的反向配套）。

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

- 内核 task（CLOCK/SYSTEM/IDLE）：运行在内核态，中断使能
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

| 问题 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 状态寄存器初值（内核 task） | INIT_TASK_PSW=0x1202（IOPL=2，中断使能） | INIT_TASK_PSR（EL1h，中断屏蔽） | INIT_TASK_SSTATUS（SPP=1，SPIE=1） |
| 状态寄存器初值（用户进程） | INIT_PSW=0x0202（IOPL=0，中断使能） | INIT_PSR（EL0t） | INIT_USER_SSTATUS（SPP=0，SPIE=1） |
| 特权级机制 | 段选择子（CS/DS/SS/ES/FS/GS） | EL（Exception Level） | 特权模式（M/S/U） |
| 入口点寄存器 | rip | elr_el1 | sepc |
| 栈指针寄存器 | rsp | sp_el0 | sp |
| ps_strings 寄存器 | rbx | r0 | a0 |

> **架构范围标注**：x86 的"段选择子"是 x86 段机制的遗留产物（虽然现代 x86-64 主要用页式管理，但段选择子仍需设置以选择特权级）。aarch64/riscv64 无此概念，直接用 EL/特权模式区分内核态/用户态。

### 1.3 三件套：进程表、特权表、RTS 位图

用 CPU 四问框架理解阶段 C 的三个核心数据结构——"三件套"：

| 数据结构 | 回答的问题 | 本质 |
|---------|---------|------|
| 进程表 | "进程是谁？" | slot 布局 + 标识符 + 生命周期 |
| 特权表 | "进程被允许做什么？" | 静态能力（类型标志/trap_mask/ipc_to/k_call_mask/io_tab/irq_tab/sig_mgr） |
| RTS 位图 | "进程现在能不能跑？" | 动态状态（非零 = 不可运行） |

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

**misc_flags 与 RTS 的区别**：

- RTS 决定**可运行性**（影响调度队列）
- misc_flags 记录**次要运行时状态**（不影响调度），如 MF_KCALLRET（内核调用返回中）、MF_DELIVERMSG（有待投递消息）等

### 1.4 boot image 与 VM 的"开天辟地"问题

#### 1.4.1 boot image：编译时硬编码的进程清单

阶段 C 要实例化的进程来自 **boot image**——一份编译时硬编码的进程清单。

**为什么编译时硬编码？** boot 期没有文件系统。内核启动时，没有任何进程在运行，没有文件系统可读——清单必须编译进内核镜像。这是 bootstrapping（自举）的另一面：最早的东西必须"自带"。

**三类进程**：

| 类型 | 例子 | p_nr | boot 期处理 |
|------|------|------|------------|
| 内核 task | CLOCK/SYSTEM/IDLE | 负数 | 立即可调度，无 ELF |
| 系统进程 | VM/PM/VFS/RS | 非负 | multiboot 提供 ELF，VM 立即加载，其他等 RS |
| 普通用户进程 | INIT | 非负 | boot 期不存在，由 RS 运行时 fork/exec |

**NR_BOOT_PROCS vs NR_BOOT_MODULES**：

- NR_BOOT_PROCS：boot image 中的进程总数（含内核 task）
- NR_BOOT_MODULES：multiboot 提供的用户态模块数（不含内核 task）

**schedulable 判定**：内核 task + RS + VM 立即可调度；其他用户进程需等 RS 运行时设特权。

**boot image 格式与编译生成**：

boot image 来自 Minix3 `kernel/table.c` 中的 `struct image image[]` 数组——编译时由 `config.h` 的 `NR_TASKS` / `NR_PROCS` 确定大小，每个条目是 `{ proc_nr, flags, proc_name, ipc_to, k_call, stack_size }`。它**不是 ELF**，只是 C 全局数组，编进 `kernel` 二进制。Rust 实现侧对应 `os/kernel/src/proc.rs:75` 的 `KERNEL_TASKS` 常量数组 + `BOOT_MODULE_PROC_NRS`（`proc.rs:88`）+ `CapabilityTemplate` 枚举（见 §3.2 / §3.4）。

**multiboot 模块 vs boot image**：
- **boot image（image[]）**：编译时硬编码进 kernel 二进制的进程清单（含 CLOCK/SYSTEM/IDLE/KERNEL 4 个内核 task + VM/PM/VFS/RS 4 个用户态 boot 模块）
- **multiboot module list（kinfo.module_list）**：bootloader（GRUB/QEMU `-initrd`）在加载 kernel 时通过 multiboot 协议额外提供的 ELF 模块；QEMU 启动命令形如 `qemu-system-x86_64 -kernel kernel.elf -initrd vm.elf,pm.elf,vfs.elf,rs.elf`
- **关系**：boot image 给出"进程清单"，multiboot module 给出"对应 ELF 镜像的物理地址范围"——`init_proc_and_boot()` 用 `kernel_info.boot_modules[i]` 对应 `BOOT_MODULE_PROC_NRS[i]`（见 §4.0）

> **本节细节深读**：[`minix3/minix/kernel/table.c`](https://github.com/minix3/minix/blob/master/minix/kernel/table.c) 的 `image[]` 数组定义；[01-boot-shim-bootstrap.md §1.5](./01-boot-shim-bootstrap.md) 的 multiboot 模块加载协议。

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
2. **内核高半区映射**（阶段 B）：内核代码/数据映射到高地址空间（如 0xFFFFFFFF80000000 以上）
3. **VM 用户态映射**（阶段 C）：VM 的 ELF 段映射到用户态地址空间（如 0x40000000 以下）

**通用 OS 设计模式**：第一个进程必须由内核手工创建，然后它才能创建更多进程。这是 OS 设计的普遍模式——Linux 的 init 进程、Minix3 的 VM 进程，都是这种"开天辟地"的第一个进程。

### 1.5 阶段 C 的执行节拍：从白板到就位但暂停

把前面概念串成执行顺序：**先清空，再填充**。

#### 1.5.1 第一步：清空（C 中对应 `proc_init()`）

- **清空进程表**：所有 slot 设 RTS_SLOT_FREE，设 p_nr 与初始 p_endpoint（generation=0）
- **清空特权表**：建立索引→槽映射（ppriv_addr[i] → &priv[i]）
- **初始化 IDLE 进程**：每 CPU 一个，共享 idle_priv，设 RTS_PROC_STOP（永远不可调度）
- **调用架构相关逻辑清零寄存器状态**（arch_proc_reset）

#### 1.5.2 第二步：填充（C 中对应 main.c 的 boot 循环 + `arch_boot_proc()`）

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

**阶段 C 结束 ≠ 进程可运行**。所有 boot 进程带 RTS_PROC_STOP 标志，被刻意按住不让跑。boot 流程的真正终点是后续阶段的 `bsp_finish_booting()`——它清除停止标志，唤醒 boot 进程并入调度队列。

#### 1.6.1 唤醒后的关键动作

`bsp_finish_booting()` 依次执行：

1. **cpu_identify()**：CPU 识别
2. **vm_running = 0**：标记 VM 尚未运行
3. **proc_ptr/bill_ptr 初始化**：当前运行进程/计费进程指向 IDLE
4. **announce()**：打印内核启动 banner
5. **清除 RTS_PROC_STOP**：遍历用户态 boot 进程（不含内核 task），RTS_UNSET 自动 enqueue
6. **cycles_accounting_init()**：周期计费初始化
7. **boot_cpu_init_timer(system_hz)**：BSP 定时器初始化
8. **fpu_init()**：CPU 级 FPU 使能
9. **SMP 配置**：标记 BSP 就绪，记录处理器数
10. **kernel_may_alloc = 0**：关闭 boot 期内存分配窗口
11. **switch_to_user()**：切换到第一个用户态进程（不返回）

#### 1.6.2 boot 期内存分配窗口

`kernel_may_alloc` 是一个全局标志，控制内核是否能直接分配物理内存：

- **boot 期**（pre_init → bsp_finish_booting）：`kernel_may_alloc = 1`，内核可直接分配物理内存（VM 还没启动，没有内存管理服务）
- **boot 完成后**（bsp_finish_booting 之后）：`kernel_may_alloc = 0`，内核不再直接分配，所有内存请求转给 VM

这个窗口的存在是因为 VM 自己也需要内存来启动——在 VM 能管理内存之前，内核必须代劳。窗口关闭标志着 VM 正式接管内存管理职责。

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

## Ch2. C 源码分析（Ground Truth，Minix3 实际做了什么）

> **本章目标**：以 Minix3 源码为 ground truth，分析阶段 C 的实际实现。每节标注 `file:line` 锚点。**补全 bsp_finish_booting 和 post-init 缺口**——这两个是旧版文档的最大覆盖漏洞。

### 2.0 源码地图：函数分布在哪些文件

阶段 C 的 C 源码分布在 5 个文件、3 个层次：

| 层次 | 文件 | 关键函数 | 行号 | 职责 |
|------|------|---------|------|------|
| 数据结构层 | `kernel/proc.c` | `proc_init()` | 119-161 | 清空进程表/特权表/IDLE |
| 流程层 | `kernel/main.c` | `kmain()` 的 boot 循环 | 165-282 | 遍历 boot image，填充 slot |
| 流程层 | `kernel/main.c` | `bsp_finish_booting()` | 38-109 | boot 流程终点，唤醒进程 |
| 流程层 | `kernel/main.c` | `arch_post_init()`/`memory_init()`/`system_init()` | 284-298 | post-init 阶段 |
| 架构层 | `arch/i386/arch_system.c` | `arch_proc_reset()` | 146-186 | x86 寄存器初始化 |
| 架构层 | `arch/i386/protect.c` | `arch_boot_proc()` | 388-455 | x86 VM ELF 加载 |
| 架构层 | `arch/i386/memory.c` | `arch_proc_init()` | 722-731 | x86 设置 PC/SP/ps_strings |
| 架构层 | `arch/earm/arch_system.c` | `arch_proc_reset()` | — | ARM 寄存器初始化 |
| 架构层 | `arch/earm/protect.c` | `arch_boot_proc()` | — | ARM VM ELF 加载 |
| 通用工具 | `libexec/exec_elf.c` | `libexec_load_elf()` | — | 通用 ELF 加载框架 |

**调用关系**：

```
kmain()
├── proc_init()                          [proc.c:119]
│   └── arch_proc_reset()                [arch_system.c:146]  (per slot)
├── IPCF_POOL_INIT()
├── boot 循环                            [main.c:165-282]
│   ├── reset_proc_accounting()
│   ├── get_priv() + fill_sendto_mask()  [system.c]
│   ├── arch_boot_proc()                 [protect.c:388]
│   │   └── libexec_load_elf()           [exec_elf.c]
│   │       └── pg_alloc 回调
│   └── arch_proc_init()                 [memory.c:722]  (VM only)
├── arch_post_init()
├── memory_init()
├── system_init()
├── add_memmap()
└── bsp_finish_booting()                 [main.c:38]
    ├── cycles_accounting_init()
    ├── boot_cpu_init_timer()
    ├── fpu_init()
    └── switch_to_user()                 [10-switch-to-user.md]
```

### 2.1 proc_init()：清空进程表和特权表

**C 源码** `minix3/minix/kernel/proc.c:119-161`：

```c
void proc_init(void)
{
  struct proc * rp;
  struct priv *sp;
  int i;

  /* 第一遍循环：清空进程表 */
  for (rp = BEG_PROC_ADDR, i = -NR_TASKS; rp < END_PROC_ADDR; ++rp, ++i) {
    rp->p_rts_flags = RTS_SLOT_FREE;     /* 标记 slot 空闲 */
    rp->p_magic = PMAGIC;
    rp->p_nr = i;                         /* 进程号 = slot 索引偏移 */
    rp->p_endpoint = _ENDPOINT(0, rp->p_nr); /* generation=0 */
    rp->p_scheduler = NULL;
    rp->p_priority = 0;
    rp->p_quantum_size_ms = 0;
    arch_proc_reset(rp);                  /* 架构相关寄存器清零 */
  }

  /* 第二遍循环：清空特权表 */
  for (sp = BEG_PRIV_ADDR, i = 0; sp < END_PRIV_ADDR; ++sp, ++i) {
    sp->s_proc_nr = NONE;                 /* 标记 priv 空闲 */
    sp->s_id = (sys_id_t) i;              /* priv 结构索引 */
    ppriv_addr[i] = sp;                   /* 建立 索引→槽 映射 */
    sp->s_sig_mgr = NONE;
    sp->s_bak_sig_mgr = NONE;
  }

  /* 第三遍循环：初始化 IDLE 进程（每 CPU 一个） */
  idle_priv.s_flags = IDL_F;
  for (i = 0; i < CONFIG_MAX_CPUS; i++) {
    struct proc * ip = get_cpu_var_ptr(i, idle_proc);
    ip->p_endpoint = IDLE;
    ip->p_priv = &idle_priv;              /* 共享 idle_priv */
    ip->p_rts_flags |= RTS_PROC_STOP;     /* 永远不可调度 */
    set_idle_name(ip->p_name, i);
  }
}
```

**三遍循环的职责分离**：

| 循环 | 对象 | 关键动作 |
|------|------|---------|
| 第一遍 | 进程表 | 设 SLOT_FREE / p_nr / p_endpoint(generation=0) / 调 arch_proc_reset |
| 第二遍 | 特权表 | 设 s_proc_nr=NONE / s_id / ppriv_addr 映射 / sig_mgr=NONE |
| 第三遍 | IDLE 进程 | 每 CPU 一个，共享 idle_priv，RTS_PROC_STOP 永不清除 |

**关键设计**：

- **p_nr 与 slot 索引一一对应**：`proc_addr(n)` 用数组下标直接定位 slot——O(1) 访问，无需查找
- **p_endpoint 初始 generation=0**：每次 slot 复用 generation 递增，防陈旧引用
- **IDLE 永远不可调度**：RTS_PROC_STOP 永不清除，IDLE 只在 CPU 空闲时被调度器选中作为最后手段

### 2.2 arch_proc_reset()：架构特定寄存器初始化

**x86 源码** `minix3/minix/kernel/arch/i386/arch_system.c:146-186`：

```c
void arch_proc_reset(struct proc *pr)
{
  char *v = NULL;
  struct stackframe_s reg;

  assert(pr->p_nr < NR_PROCS);

  /* 用户进程分配 FPU 状态区 */
  if(pr->p_nr >= 0) {
    v = fpu_state[pr->p_nr];
    assert(!((vir_bytes)v % FPUALIGN));   /* FPUALIGN 对齐 */
    memset(v, 0, FPU_XFP_SIZE);           /* 清零 FPU 状态 */
  }

  /* 清零进程状态 */
  memset(&reg, 0, sizeof(pr->p_reg));
  if(iskerneln(pr->p_nr))
    reg.psw = INIT_TASK_PSW;              /* 内核 task PSW */
  else
    reg.psw = INIT_PSW;                   /* 用户进程 PSW */

  pr->p_seg.fpu_state = v;

  /* 段选择子（x86 段机制遗留） */
  pr->p_reg.cs = USER_CS_SELECTOR;
  pr->p_reg.gs = 
  pr->p_reg.fs = 
  pr->p_reg.ss = 
  pr->p_reg.es = 
  pr->p_reg.ds = USER_DS_SELECTOR;

  arch_proc_setcontext(pr, &reg, 0, KTS_FULLCONTEXT);
}
```

**ARM 源码** `minix3/minix/kernel/arch/earm/arch_system.c`（简化）：

```c
void arch_proc_reset(struct proc *pr)
{
  memset(&pr->p_reg, 0, sizeof(pr->p_reg));
  if(iskerneln(pr->p_nr))
    pr->p_reg.psr = INIT_TASK_PSR;
  else
    pr->p_reg.psr = INIT_PSR;
}
```

**架构差异对照**：

| 项 | x86 | ARM |
|----|-----|-----|
| FPU 状态区 | 有（fpu_state[p_nr]，FPUALIGN 对齐，memset 清零） | 无（CPACR_EL1 系统级配置） |
| 段选择子 | 有（USER_CS_SELECTOR / USER_DS_SELECTOR） | 无（ARM 无段机制） |
| 状态寄存器 | PSW（INIT_TASK_PSW=0x1202 / INIT_PSW=0x0202） | PSR（INIT_TASK_PSR / INIT_PSR） |

**三架构状态寄存器初值表**：

| 架构 | 内核 task 初值 | 用户进程初值 | 关键位 |
|------|--------------|------------|--------|
| x86 | INIT_TASK_PSW=0x1202 | INIT_PSW=0x0202 | IOPL（I/O 特权级）、IF（中断使能） |
| aarch64 | INIT_TASK_PSR | INIT_PSR | EL（Exception Level）、F/I/A/D mask |
| riscv64 | INIT_TASK_SSTATUS | INIT_USER_SSTATUS | SPP（S 态来源）、SPIE（S 态中断使能） |

> **架构范围标注**：x86 的"段选择子"和"FPU 状态区"是 x86 段机制与 fnsave/fxrstor 模型的遗留。aarch64/riscv64 无段机制，FPU 用系统级配置（CPACR_EL1.FPEN / sstatus.FS）。详见 §3.6 FPU 架构演进。

### 2.3 boot image 循环：main.c 的核心

**C 源码** `minix3/minix/kernel/main.c:165-282`（关键片段）：

```c
for (i=0; i < NR_BOOT_PROCS; ++i) {
  int schedulable_proc;
  proc_nr_t proc_nr;
  int ipc_to_m, kcalls;
  sys_map_t map;

  ip = &image[i];                         /* boot image 条目 */
  rp = proc_addr(ip->proc_nr);            /* 定位 slot */
  ip->endpoint = rp->p_endpoint;          /* 同步 endpoint */
  rp->p_cpu_time_left = 0;
  if(i < NR_TASKS)
    strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name));

  if(i >= NR_TASKS) {
    /* 用户进程：取 multiboot 模块 */
    multiboot_module_t *mb_mod = &kinfo.module_list[i - NR_TASKS];
    ip->start_addr = mb_mod->mod_start;
    ip->len = mb_mod->mod_end - mb_mod->mod_start;
  }
  
  reset_proc_accounting(rp);

  /* schedulable 判定 */
  proc_nr = proc_nr(rp);
  schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
    proc_nr == VM_PROC_NR);

  if(schedulable_proc) {
    (void) get_priv(rp, static_priv_id(proc_nr));  /* 静态分配特权 */

    if(proc_nr == VM_PROC_NR) {
      priv(rp)->s_flags = VM_F;
      priv(rp)->s_trap_mask = SRV_T;
      ipc_to_m = SRV_M;
      kcalls = SRV_KC;
      priv(rp)->s_sig_mgr = SELF;         /* main.c:186 — VM 的 sig_mgr 是 SELF */
      rp->p_priority = SRV_Q;
      rp->p_quantum_size_ms = SRV_QT;
    }
    else if(iskerneln(proc_nr)) {
      priv(rp)->s_flags = (proc_nr == IDLE ? IDL_F : TSK_F);
      priv(rp)->s_init_flags = TSK_I;
      priv(rp)->s_trap_mask = (proc_nr == CLOCK || proc_nr == SYSTEM ? CSK_T : TSK_T);
      ipc_to_m = TSK_M;
      kcalls = TSK_KC;
      /* 内核 task 不覆写 p_priority/p_quantum_size_ms，保持 0 */
    }
    else {  /* isrootsysn(proc_nr) — RS */
      priv(rp)->s_flags = RSYS_F;
      priv(rp)->s_init_flags = SRV_I;
      priv(rp)->s_trap_mask = SRV_T;
      ipc_to_m = SRV_M;
      kcalls = SRV_KC;
      priv(rp)->s_sig_mgr = SRV_SM;       /* main.c:208 — RS 的 sig_mgr 是 SRV_SM=ROOT_SYS_PROC_NR */
      rp->p_priority = SRV_Q;
      rp->p_quantum_size_ms = SRV_QT;
    }

    /* 填 IPC 目标掩码 */
    memset(&map, 0, sizeof(map));
    if (ipc_to_m == ALL_M) {
      for(j = 0; j < NR_SYS_PROCS; j++)
        set_sys_bit(map, j);
    }
    fill_sendto_mask(rp, &map);

    /* 填内核调用掩码 */
    for(j = 0; j < SYS_CALL_MASK_SIZE; j++)
      priv(rp)->s_k_call_mask[j] = (kcalls == NO_C ? 0 : (~0));
  }
  else {
    /* 非 schedulable：标记无特权/无时间片 */
    RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM);
  }

  arch_boot_proc(ip, rp);                 /* VM 加载 ELF */

  if(!get_cpulocal_var(proc_ptr))
    get_cpulocal_var(proc_ptr) = rp;

  /* 非 VM 用户进程：等 VM 建页表 + 等 boot 完成 */
  if(rp->p_nr != VM_PROC_NR && rp->p_nr >= 0) {
    rp->p_rts_flags |= RTS_VMINHIBIT;
    rp->p_rts_flags |= RTS_BOOTINHIBIT;
  }

  rp->p_rts_flags |= RTS_PROC_STOP;       /* 所有进程：停止 */
  rp->p_rts_flags &= ~RTS_SLOT_FREE;      /* 清 SLOT_FREE */
}
```

**三类 schedulable 进程的特权授予对照**：

| 进程 | s_flags | s_trap_mask | ipc_to | kcalls | s_sig_mgr | p_priority | p_quantum |
|------|---------|-------------|--------|--------|-----------|-----------|-----------|
| VM | VM_F | SRV_T | SRV_M | SRV_KC | **SELF** | SRV_Q | SRV_QT |
| 内核 task | IDL_F\|TSK_F | CSK_T\|TSK_T | TSK_M | TSK_KC | — | 0（不覆写） | 0（不覆写） |
| RS | RSYS_F | SRV_T | SRV_M | SRV_KC | **SRV_SM** | SRV_Q | SRV_QT |

> **重要修正**：VM 的 `s_sig_mgr` 是 `SELF`（main.c:186），不是 PM_SM。RS 的 `s_sig_mgr` 才是 `SRV_SM=ROOT_SYS_PROC_NR`。这是旧版文档的常见错误。

**p_priority/p_quantum_size_ms 覆写规则**：

| 进程类型 | 覆写 | 值 |
|---------|------|-----|
| VM | ✅ | SRV_Q / SRV_QT |
| RS | ✅ | SRV_Q / SRV_QT |
| 内核 task | ❌ | 保持 0（proc_init 设的初值） |
| 非 schedulable 用户进程 | ❌ | 保持 0（等 RS 运行时设） |

**阶段 C 的 RTS 位设置**：

| 进程类型 | RTS 位 | 含义 |
|---------|--------|------|
| 所有进程 | RTS_PROC_STOP | 停止（等 bsp_finish_booting 唤醒） |
| 所有进程 | 清 RTS_SLOT_FREE | slot 已占用 |
| 非 schedulable | RTS_NO_PRIV \| RTS_NO_QUANTUM | 无特权/无时间片 |
| 非 VM 用户进程 | RTS_VMINHIBIT \| RTS_BOOTINHIBIT | 等 VM 建页表 + 等 boot 完成 |

### 2.4 get_priv() 和 fill_sendto_mask()：特权与 IPC 掩码

**get_priv()** `minix3/minix/kernel/system.c`（简化）：

```c
int get_priv(struct proc *rp, int priv_id)
{
  if (is_static_priv_id(priv_id)) {
    /* 静态分配：boot 期系统进程 */
    rp->p_priv = ppriv_addr[priv_id];     /* 直接索引 */
    if (rp->p_priv->s_proc_nr != NONE)
      return ENOSPC;                       /* 槽已占用 */
    rp->p_priv->s_proc_nr = proc_nr(rp);
  }
  else {
    /* 动态分配：运行时普通进程 */
    for (rp->p_priv = &priv[0]; rp->p_priv < &priv[NR_SYS_PROCS]; rp->p_priv++) {
      if (rp->p_priv->s_proc_nr == NONE) break;
    }
    if (rp->p_priv >= &priv[NR_SYS_PROCS])
      return ENOSPC;
    rp->p_priv->s_proc_nr = proc_nr(rp);
  }
  return OK;
}
```

**set_sendto_bit / fill_sendto_mask**：设 IPC 目标位，保持对称性。

```c
void set_sendto_bit(struct priv *priv, int proc_nr)
{
  /* 设 priv 能向 proc_nr 发 IPC 的位 */
  set_sys_bit(priv->s_ipc_to, proc_nr);
}

void fill_sendto_mask(struct proc *rp, sys_map_t *map)
{
  /* 根据 map 填充 rp 的 s_ipc_to，并保持对称性 */
  for (int i = 0; i < NR_SYS_PROCS; i++) {
    if (get_sys_bit(*map, i)) {
      set_sendto_bit(priv(rp), i);
      /* 对称性：A 能发到 B ⇔ B 能发到 A（除非 B 只支持 RECEIVE） */
      struct proc *target = proc_addr(i);
      if (target->p_priv && !(target->p_priv->s_flags & RECEIVE_OWN))
        set_sendto_bit(target->p_priv, proc_nr(rp));
    }
  }
}
```

**IPC 对称性**是微内核安全模型的关键不变量：A 能发到 B ⇔ B 能发到 A（除非 B 显式声明只接收）。这避免了"单向授权"导致的消息泄漏。

### 2.5 arch_boot_proc()：VM ELF 加载

**x86 源码** `minix3/minix/kernel/arch/i386/protect.c:388-455`：

```c
void arch_boot_proc(struct boot_image *ip, struct proc *rp)
{
  multiboot_module_t *mod;
  struct ps_strings *psp;
  char *sp;

  if(rp->p_nr < 0) return;                /* 内核 task 直接返回 */

  mod = bootmod(rp->p_nr);                /* 查 boot module */

  if(rp->p_nr == VM_PROC_NR) {            /* 仅 VM 特殊处理 */
    struct exec_info execi;
    memset(&execi, 0, sizeof(execi));

    /* exec 参数 */
    execi.stack_high = kinfo.user_sp;
    execi.stack_size = 64 * 1024;          /* 64KB 栈，预分配 */
    execi.proc_e = ip->endpoint;
    execi.hdr = (char *) mod->mod_start;   /* 物理内存直接访问 */
    execi.filesize = execi.hdr_len = mod->mod_end - mod->mod_start;
    strlcpy(execi.progname, ip->proc_name, sizeof(execi.progname));
    execi.frame_len = 0;

    /* 6 回调 */
    execi.copymem = libexec_copy_memcpy;
    execi.clearmem = libexec_clear_memset;
    execi.allocmem_prealloc_junk = libexec_pg_alloc;
    execi.allocmem_prealloc_cleared = libexec_pg_alloc;
    execi.allocmem_ondemand = libexec_pg_alloc;
    execi.clearproc = NULL;

    /* 解析 VM ELF + 映射到 bootstrap 页表 */
    if(libexec_load_elf(&execi) != OK)
      panic("VM loading failed");

    /* 设置 ps_strings 结构 */
    sp = (char *)execi.stack_high;
    sp -= sizeof(struct ps_strings);
    psp = (struct ps_strings *) sp;

    /* SP 下移 3 字：argc/argv/envp */
    sp -= (sizeof(void *) + sizeof(void *) + sizeof(int));

    psp->ps_argvstr = (char **)(sp + sizeof(int));
    psp->ps_nargvstr = 0;
    psp->ps_envstr = psp->ps_argvstr + sizeof(void *);
    psp->ps_nenvstr = 0;

    arch_proc_init(rp, execi.pc, (vir_bytes)sp,
      execi.stack_high - sizeof(struct ps_strings),
      ip->proc_name);

    /* 记录 VM blob 内存区域 */
    add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
    mod->mod_end = mod->mod_start = 0;     /* 标记 module 已消费 */

    kinfo.vm_allocated_bytes = alloc_for_vm;
  }
}
```

**关键步骤**：

1. **内核 task 跳过**：`if(rp->p_nr < 0) return`
2. **bootmod 查找**：遍历 `kinfo.module_list` 匹配 `proc_nr`
3. **仅 VM 特殊处理**：其他用户进程 boot 期不加载 ELF（等 RS 运行时加载）
4. **构造 exec_info**：stack_high/stack_size=64KB/hdr/filesize/frame_len=0
5. **设 6 回调**：copymem/clearmem/3 个 allocmem/clearproc=NULL
6. **libexec_load_elf**：解析 ELF + 映射到 bootstrap 页表
7. **设置 ps_strings**：在栈顶放 ps_strings 结构，SP 下移 3 字
8. **arch_proc_init**：设 p_reg.pc/sp/bx
9. **add_memmap**：记录 VM blob 内存区域
10. **清 mod_start/mod_end=0**：标记 module 已消费，防重复加载
11. **记录 vm_allocated_bytes**：累计 VM 分配的字节数

### 2.6 arch_proc_init()：设置进程初始 PC/SP

**x86 源码** `minix3/minix/kernel/arch/i386/memory.c:722-731`：

```c
void arch_proc_init(struct proc *pr, const u32_t ip, const u32_t sp,
  const u32_t ps_str, char *name)
{
  arch_proc_reset(pr);                    /* 重新清零 */
  strlcpy(pr->p_name, name, sizeof(pr->p_name));

  pr->p_reg.pc = ip;                      /* 入口点 */
  pr->p_reg.sp = sp;                      /* 栈指针 */
  pr->p_reg.bx = ps_str;                  /* x86 约定：bx 存 ps_strings */
}
```

**三架构参数传递寄存器**：

| 架构 | 入口点寄存器 | 栈指针寄存器 | ps_strings 寄存器 |
|------|-----------|-----------|----------------|
| x86 | pc (rip) | sp (rsp) | bx (rbx) |
| aarch64 | pc (elr_el1) | sp (sp_el0) | r0 |
| riscv64 | sepc | sp | a0 |

### 2.7 libexec_load_elf 框架

**libexec_load_elf** 是通用 ELF 加载框架，boot 期只是它的一个特化使用。

**exec_info 结构体字段**：

| 字段 | 类型 | 含义 |
|------|------|------|
| stack_high | vir_bytes | 栈顶高地址 |
| stack_size | size_t | 栈大小（VM: 64KB） |
| proc_e | endpoint_t | 进程端点 |
| hdr | char * | ELF 头（物理地址） |
| hdr_len | size_t | ELF 头长度 |
| filesize | size_t | 文件总长度 |
| progname | char[] | 程序名 |
| frame_len | int | 栈帧长度（VM: 0） |
| pc | vir_bytes | 返回：入口点 |
| load_offset | vir_bytes | 返回：加载基址 |
| copymem | callback | 复制段字节 |
| clearmem | callback | 清零 BSS |
| allocmem_prealloc_junk | callback | 分配页（含垃圾） |
| allocmem_prealloc_cleared | callback | 分配页（清零） |
| allocmem_ondemand | callback | 按需分配页 |
| clearproc | callback | 清进程（VM: NULL） |

**流程**：

```
libexec_load_elf(execi)
├── elf_unpack(execi->hdr, execi->hdr_len, &ehdr)  /* 解析 ELF 头 */
├── elf_has_interpreter(...)                        /* 检查有无 interpreter */
├── 遍历 PT_LOAD 段:
│   ├── allocmem_prealloc_*: 分配物理页 + 映射页表
│   ├── copymem: 复制段字节到映射页
│   └── clearmem: 清零 BSS 部分
├── 分配栈: allocmem_prealloc_cleared(stack_high - stack_size, stack_size)
└── 返回 pc / load_base
```

**错误码**：

- `ENOEXEC`：ELF 无效（魔数错误、段对齐错误等）
- `ENOMEM`：映射失败（物理内存不足）

> **详见附录 A**：libexec_load_elf 框架详解。

### 2.8 bsp_finish_booting()：boot 流程的终点

**这是旧版文档的最大覆盖缺口**——读者必须理解"阶段 C 之后进程怎么开始跑"。

**C 源码** `minix3/minix/kernel/main.c:38-109`：

```c
void bsp_finish_booting(void)
{
  int i;

  cpu_identify();                         /* CPU 识别 */
  vm_running = 0;                         /* VM 尚未运行 */
  krandom.random_sources = RANDOM_SOURCES;
  krandom.random_elements = RANDOM_ELEMENTS;

  /* 当前运行进程/计费进程指向 IDLE */
  get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc);
  get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);

  announce();                             /* 打印 MINIX 启动 banner */

  /* 唤醒 boot 进程：清除 RTS_PROC_STOP，自动 enqueue */
  for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) {
    RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
  }

  cycles_accounting_init();               /* 周期计费初始化 */

  if (boot_cpu_init_timer(system_hz)) {   /* BSP 定时器初始化 */
    panic("FATAL : failed to initialize timer interrupts");
  }

  fpu_init();                             /* CPU 级 FPU 使能 */

#ifdef CONFIG_SMP
  cpu_set_flag(bsp_cpu_id, CPU_IS_READY);
  machine.processors_count = ncpus;
  machine.bsp_id = bsp_cpu_id;
#else
  machine.processors_count = 1;
  machine.bsp_id = 0;
#endif

  kernel_may_alloc = 0;                   /* 关闭 boot 期内存分配窗口 */

  switch_to_user();                       /* 切换到第一个用户态进程 */
  NOT_REACHABLE;                          /* 不返回 */
}
```

**关键点**：

1. **唤醒循环只遍历用户态 boot 进程**（`i < NR_BOOT_PROCS - NR_TASKS`），不含内核 task——内核 task 的 RTS_PROC_STOP 在 proc_init 中设置且永不清除（IDLE）或在 boot 循环中已处理
2. **RTS_UNSET 自动 enqueue**：清除 RTS_PROC_STOP 后，进程自动加入调度队列
3. **kernel_may_alloc = 0**：关闭 boot 期内存分配窗口，VM 即将接管内存管理
4. **switch_to_user() 后 NOT_REACHABLE**：函数不返回，CPU 切换到第一个用户态进程

**boot 流程完整链路**：

```
proc_init() → boot 循环 → arch_post_init() → memory_init() → system_init()
→ add_memmap() → bsp_finish_booting() → switch_to_user()
```

### 2.9 boot 期架构后初始化与内存映射

**post-init 阶段** `minix3/minix/kernel/main.c:284-298`：

```c
arch_post_init();                         /* 架构后初始化 */

/* IPC 调用名注册 */
IPCNAME(SEND);
IPCNAME(RECEIVE);
/* ... */

memory_init();                            /* 内存初始化 */
system_init();                            /* 系统初始化 */

/* 把 bootstrap 物理内存加入空闲列表 */
add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len);
```

**各步骤职责**：

| 函数 | 职责 | 详细文档 |
|------|------|---------|
| `arch_post_init()` | 设 ptproc=VM（VM 的页表成为内核切换目标） | 07 |
| `memory_init()` | 分配 freepdes 空闲页目录 | 07 |
| `system_init()` | 初始化特权表运行时结构 | 08 |
| `add_memmap()` | 向 VM 传递内存映射（记录内存区域） | 07 |
| `IPCF_POOL_INIT()` | IPC filter pool 初始化 | 23-ipc-filter.md |

> **范围边界声明**：这些步骤的详细实现见 07/08 文档，本节只讲它们在 boot 流程中的位置和职责。

---

## Ch3. Rust 设计决策（本质机制，约束驱动）

> **本章目标**：讲 Rust rewrite 捕获的**本质机制**（非 translate 表面）。每节用"假设性推理"（如果 X 设计，会有 Y 问题，所以用 Z）替代"迭代历史"。**讲 WHY，不讲 HOW**（HOW 在 Ch4）。
>
> **核心叙事**：Rust 不是翻译 C 的函数，而是重新表达 C 背后的机制本质，用 Rust 类型系统强制不变量。

### 3.0 核心问题与设计原则

阶段 C 的 Rust rewrite 要捕获的本质：进程表/特权表/RTS/boot image/CPU 上下文/VM ELF 加载。

**强制设计原则**：

| 原则 | 含义 | 违反时的症状 |
|------|------|------------|
| **P1 零堆启动** | boot 期无堆分配器，固定数组 + const fn | `Box<[KProcess]>` 在 boot 期分配 |
| **P2 no_std** | 不链接 std，除 `#[cfg(test)]` | `std::Vec` 出现在 kernel crate |
| **P3 SMP 安全** | BKL 下 Rc/RefCell 跨 CPU 不安全，用 Atomic + BKL | `Rc<RefCell<KProcess>>` 跨 CPU 共享 |
| **P4 硬件抽象为 trait** | 上层不读 arch 私有字段，不用 `#[cfg(target_arch)]` 选行为 | `#[cfg(target_arch = "x86_64")]` 出现在 kernel crate |
| **P5 FPU 是 arch 演进问题** | 不翻译 Minix3 的 fnsave/fxrstor，用现代 XSAVE/CPACR_EL1/sstatus.FS | `fpu_needs_zero: bool` 字段泄漏到 OS 层 |
| **P6 能力是数据，不是裸参数** | 权限/能力打包为结构体/枚举，不用 6 个裸参数 | `configure_boot_priv(flags, init_flags, trap_mask, ...)` |
| **P7 硬件行为下沉** | x86-64 特有行为（如 IOPL）通过 arch trait 方法下沉 | `syscall_device.rs` 直接操作 `X86_64_IOPL_BITS` |

**rewrite vs translate 的区别**：rewrite 捕获本质机制（"进程需要 CPU 上下文" → 不透明 `CpuContext`），translate 复制表面结构（"C 有 `arch_proc_reset` 函数" → Rust 也定义 `ArchProcReset` trait）。C 的 `arch_proc_reset`/`arch_proc_init`/`arch_boot_proc` 三个函数是实现细节，不是 OS 概念。Rust 版用 `build_cpu_context`（覆盖 reset+init）+ free fn `load_vm_elf`（覆盖 boot_proc 的 ELF 加载部分）替代——这是 rewrite，不是 translate。

### 3.1 进程表设计：固定数组 + 指针→索引

**本质**：进程表是"索引→进程状态"的映射。C 用全局数组 `struct proc proc[NR_TASKS+NR_PROCS]` + 裸指针 `struct proc *p_nextready`/`p_scheduler` 表达这个映射和进程间链接。

**约束驱动**：

- **零堆（P1）**→ 不能用 `Box<[KProcess]>` 堆分配，必须用固定数组 `[KProcess; PROC_TABLE_SIZE]`
- **const fn（P1）**→ `ProcessTable::new()` 必须是 `const fn`，编译期完成初始化（BSS 段零运行时开销）
- **SMP 安全（P3）**→ C 的裸指针 `p_nextready`/`p_scheduler` 跨 CPU 共享无法证明安全，改为 `AtomicPtr` 或 `Option<ProcNr>` 索引

**指针→索引的 rewrite**：

| C 字段 | C 类型 | Rust 类型 | 本质理由 |
|--------|--------|----------|---------|
| `p_nextready` | `struct proc *` | `AtomicPtr<KProcess>` | 调度队列链接，跨 CPU 读（idle steal） |
| `p_scheduler` | `struct proc *` | `Option<ProcNr>` | 调度器归属，索引便于边界检查 |
| `p_caller_q` | `struct proc *` | `Option<ProcNr>` | IPC 阻塞队列，索引避免裸指针 |

**边界检查强制**：C 的 `proc_addr(n)` 用数组下标直接定位，无越界检查（越界是 UB）。Rust 的 `get(nr)`/`get_mut(nr)` 返回 `Option<&KProcess>`，把"越界是 UB"变为"越界是编译期/运行期可捕获的 `None`"。

**nr_to_idx：进程号 → 索引的映射函数**：`get(nr)`/`get_mut(nr)` 内部调用 `nr_to_idx(nr)`，映射公式为 `index = nr + NR_TASKS`（同 C 的 `proc_addr` 偏移：`proc_addr(n)` = `&proc[NR_TASKS + n]`）。内核 task（nr < 0）映射到 `0..NR_TASKS`；用户进程（nr ≥ 0）映射到 `NR_TASKS..NR_TASKS+NR_PROCS`。越界（`offset < 0` 或 `offset ≥ PROC_TABLE_SIZE`）返回 `None`——C 的裸索引无检查，Rust 用 `Option` 捕获。`nr_to_idx` 声明为 `pub(crate) const fn`：`const fn` 允许编译期求值（配合编译期不变量检查），`pub(crate)` 使 `syscall::dispatch_ipc_entry` 等路径无需借用 `&mut KProcess` 即可计算索引（避免 split-borrow 别名冲突，FIX-21）。

**假设性推理**：如果在 boot 期用 `Box<[KProcess]>` 堆分配，会破坏零堆约束（boot 期无 `GlobalAlloc`）；如果用裸指针跨 CPU 共享 `p_nextready`，SMP 下无法证明一个 CPU 不会在另一个 CPU 读指针时释放目标进程的 slot。固定数组 + 原子索引是零堆 + SMP 安全的唯一组合。

**全局存储**：`static PROC_TABLE: SyncUnsafeCell<ProcessTable>` 放在 `.kernel.bss` 段，对应 C 的 `EXTERN struct proc proc[]`。`SyncUnsafeCell`（显式 `Sync` + BKL 保护）是 Rust 2024 下 C EXTERN 语义的忠实表达——消除 `static mut`，避免 `static_mut_refs` lint，同时保持零堆与编译期固定地址（`spin::Once` 会引入 heap 分配，违反零堆）。

### 3.2 特权表设计：CapabilityTemplate 枚举替代 6 裸参数

**本质**：特权表是"进程被允许做什么"的静态能力集合。C 用 `struct priv` 的 30+ 裸字段（`s_flags`/`s_trap_mask`/`s_ipc_to`/`s_k_call_mask`/`s_sig_mgr`...）表达，boot 期通过 `get_priv()` + 散落的 `if is_vm { flags=VM_F; ... }` 分支配置。

**约束驱动**：

- **能力是数据（P6）**→ 6 个裸参数 `configure_boot_priv(flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` 缺乏类型安全和 OS 语义，打包为 `ProcessCapability` 结构体 + `CapabilityTemplate` 枚举
- **零堆（P1）**→ `PrivTable` 用固定数组 `[KPriv; NR_SYS_PROCS]` + const fn
- **角色→能力的显式映射**→ C 的 `if is_vm { ... } else if is_root_sys { ... }` 散落分支，Rust 提取为 `CapabilityTemplate` 枚举的 5 变体

**CapabilityTemplate 枚举**：

```rust
pub enum CapabilityTemplate {
    Idle,                                    // IDLE：可计费，无 IPC
    KernelTask { trap_mask: TrapMask },      // CLOCK/SYSTEM/KERNEL
    Vm,                                      // VM：全部 IPC + 全部内核调用
    RootService,                             // RS：全部 IPC + 全部内核调用
    Deferred,                                // 其他：boot 期不授予，等 RS 运行时配置
}
```

每个变体封装该 OS 角色的完整能力定义，`build(self_endpoint) -> ProcessCapability` 是"角色→能力"映射的单一来源。`init_proc_and_boot()` 调用 `grant_capability(nr, template)` 一步完成"分配 slot + 写入能力"，不再散落 `if/else if` 分支。

**假设性推理**：如果让调用方手动传 6 个裸参数（`flags: u32, init_flags: i32, trap_mask: u16, ipc_to: u64, k_call_mask: [u32;2], sig_mgr: Endpoint`），会有三个问题：(1) 参数顺序易错（`flags` 和 `init_flags` 都是整数，类型不区分）；(2) 调用方需要记住每个角色的配置值，散落在主流程；(3) 无法在类型层面阻止"传了 VM 的 flags 但忘了设 sig_mgr"。`CapabilityTemplate` 枚举把角色作为类型，配置作为数据，消除这三个问题。

### 3.3 RTS 位图设计：bitflags + 不变量强制

**本质**：RTS（Run-Time Status）是进程"现在能不能跑"的动态状态。C 用 `p_rts_flags: u32` 位图表达，不变量是"A process is runnable iff p_rts_flags == 0"。

**为什么用位图而非枚举**：进程可同时因多个原因不可运行（如无特权 + 等 VM 建页表 + 被停止），位图支持多原因叠加，枚举只能表达单一状态。

**约束驱动**：

- **类型安全**→ C 的 `#define RTS_SLOT_FREE 0x01` 是裸整数，Rust 用 `bitflags!` 宏生成 `RtsFlags` 类型，位操作有类型检查
- **SMP 原子性（P3）**→ `p_rts_flags: AtomicU32`，多 CPU 调度器读、单 CPU 调度器写，无锁竞争
- **不变量强制**→ `rts_set()`/`rts_unset()` 方法封装"设标志→dequeue"/"清标志→enqueue"的联动，不让调用方手动维护调度队列一致性

**关键 RTS 位（阶段 C 涉及）**：

| 位 | 含义 | 设置时机 |
|----|------|---------|
| `SLOT_FREE` | slot 未分配 | proc_init 清空时 |
| `NO_PRIV` | 无特权 | 非 schedulable 用户进程 |
| `NO_QUANTUM` | 无时间片 | 非 schedulable 用户进程 |
| `VMINHIBIT` | 等 VM 建页表 | 非 VM 用户进程 |
| `BOOTINHIBIT` | 等 boot 完成 | 非 VM 用户进程 |
| `PROC_STOP` | 被停止 | 所有 boot 进程（bsp_finish_booting 清除） |

**完整位集**：类型定义保留 C 的全部 16 位（`bitflags!` 16 个常量）。阶段 C 之外的位在运行时由各子系统设置，消费方文档覆盖：

| 位（阶段 C 外） | 含义 | 设置场景 | 消费方文档 |
|----------------|------|---------|-----------|
| `SENDING` / `RECEIVING` | IPC 阻塞中 | IPC send/receive | 12-ipc-core / 13-syscall-dispatch |
| `SIGNALED` / `SIG_PENDING` / `P_STOP` | 信号送达/待处理/停止 | 信号投递 | 17-syscall-process |
| `NO_ENDPOINT` | 进程 slot 清理中 | `clear_endpoint` 设置，使进程不再被调度（system.c:540-572） | 17-syscall-process |
| `PAGEFAULT` / `VMREQUEST` / `VMREQTARGET` | VM 内存交互 | 缺页/VM 请求 | 12-ipc-core |
| `PREEMPTED` | 被抢占 | SMP/时钟抢占 | 11-scheduling-primitives |

**misc_flags 与 RTS 的区别**：RTS 决定可运行性（影响调度队列），`misc_flags` 记录次要运行时状态（不影响调度）。两者分离避免"改次要状态误触发 enqueue/dequeue"。

**MiscFlags 完整位集**：类型定义保留 C 的全部 18 位（`REPLY_PEND`/`VIRT_TIMER`/`PROF_TIMER`/`KCALL_RESUME`/`DELIVERMSG`/`SIG_DELAY`/`SC_ACTIVE`/`SC_DEFER`/`SC_TRACE`/`EXT_REG_INITIALIZED`/`SENDING_FROM_KERNEL`/`CONTEXT_SET`/`SPROF_SEEN`/`FLUSH_TLB`/`SENDA_VM_MISS`/`STEP`/`MSGFAILED`/`NICED`）。阶段 C 之外使用的位由消费方文档覆盖：`EXT_REG_INITIALIZED`（fork 继承，见 §3.14）、`DELIVERMSG`/`KCALL_RESUME`（消息投递/内核调用恢复，见 13-syscall-dispatch）、`NICED`（用户通过调度参数降低优先级，`sched` 调整时设置/清除，见 11-scheduling-primitives）等。

### 3.4 boot image 类型设计：ProcKind + EntrySpec

**本质**：boot image 是编译时硬编码的进程清单。C 用 `image[]` 数组 + `ip->proc_nr`/`ip->pc`/`ip->stack_addr` 裸字段表达。Rust 用两个 OS 概念类型替代。

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

**假设性推理**：如果用 `pc: VirBytes`（非 Option），无法在类型层面区分"kernel task 无入口点"和"入口点恰好是 0"，调用方需要额外 `is_kernel: bool` 参数。`Option` 让"有无入口点"成为类型信息，编译器强制处理两种情况。

**ProcKind 与 CapabilityTemplate 为什么不合并**：两者各有 5 个变体、看似一一对应（KernelTask/Vm/RootService/UserService/UserProcess），但关注点不同——`ProcKind` 是给 arch 层看的（决定初始 PSW/PSR/sstatus、段选择子、FPU 策略），`CapabilityTemplate` 是给 kernel 层看的（决定 IPC/syscall/trap 权限）。`ProcKind::KernelTask` ≠ `CapabilityTemplate::KernelTask { trap_mask }`：前者表达"运行在内核态"，后者表达"sys_proc 标志 + 有限 trap 集合"。反例：IDLE 进程是 `ProcKind::KernelTask` + `CapabilityTemplate::Idle` 的组合——角色与能力并不一一对应，合并会丢失这种组合自由度。

### 3.5 CpuContextArch trait：唯一的 arch 抽象

**本质**：boot 一个进程，CPU 需要完整的初始状态（PSW/PSR/sstatus + 段选择子 + PC/SP + FPU 策略）。C 用 3 个函数（`arch_proc_reset`/`arch_proc_init`/`arch_boot_proc`）分步构建。Rust 用单个 trait `CpuContextArch` + 关联类型 `CpuContext`（不透明）表达。

**为什么合并 3 个 trait 为 1 个**：

C 的 3 个函数是**实现细节**，不是 OS 概念：

| C 函数 | 职责 | Rust 对应 |
|--------|------|----------|
| `arch_proc_reset` | 清零寄存器 + 设初始 PSW | 合并进 `build_cpu_context` |
| `arch_proc_init` | 设 PC/SP/ps_strings | 合并进 `build_cpu_context`（通过 `EntrySpec`） |
| `arch_boot_proc` | VM ELF 加载 | 拆出为 free fn `load_vm_elf`（三架构相同，假多态） |

如果用 3 个 trait 镜像 C 的 3 函数，会暴露三个问题：(1) `ArchProcInit::init_regs` 三架构实现几乎相同（假多态）；(2) trait 继承链"reset→init→boot"翻译自 C 调用链，不是 OS 概念；(3) 文档自称"init 内部调用 reset"是因果链编造（trait 继承是编译时类型关系，不是运行时调用）。

**CpuContextArch trait 设计**：

```rust
pub trait CpuContextArch {
    /// arch 私有的"进程 CPU 上下文"，kernel 层只存储不 inspect
    type CpuContext: Copy + core::fmt::Debug + Default;
    type TrapFrame;

    /// 为新进程构建 CPU 上下文（boot 期调用）
    fn build_cpu_context(kind: ProcKind, proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext;

    /// 把上下文应用到 trap frame（首次调度时调用）
    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame);

    /// 启用用户 I/O 特权（x86-64: IOPL=3；其他架构: default no-op）
    fn enable_user_io(_ctx: &mut Self::CpuContext) {}
}
```

**命名说明**：trait 名叫 `CpuContextArch`（不是 `BootArch`），因为 `apply_to_trap_frame` 在首次调度时调用，跨越 boot + runtime 两个阶段。关联类型叫 `CpuContext`（不是 `StartupState`），因为这个值长期存储在 `KProcess` 中，不是一次性启动值。

**为什么删 `p_ext_reg_state`（量化论证）**：C 的 `struct proc` 内嵌 `p_ext_reg_state: ExtRegState`（576B/进程，`FPU_XFP_SIZE` 对应 32 位 fnsave 模型）——进程表 256 槽 = **144KB 静态 BSS 浪费**，且 aarch64/riscv64 完全不需要（无 per-process FPU 保存区，见 §3.6 三架构 FPU 模型表）。Rust 版把它拆成两部分：x86-64 的 XSAVE area 下沉到 arch 私有的 `CpuContext` 关联类型（arch 自管自用），aarch64/riscv64 的 FPU 状态由 trap frame 自带（FPCR/FPSR）或 sstatus.FS 表达。删除后 KProcess 不再有与 boot 无关的 576B 固定开销，进程表内存预算从"最坏情况全架构对齐"降为"各架构实际所需"。

**与 trap frame 的区分**：`CpuContext` 是进程**尚未运行**时的初始状态（arch 私有、不透明）；trap frame 是进程**正在运行/被中断**时 CPU 寄存器的保存区（OS 可见）。两者通过 `apply_to_trap_frame` 桥接。

**C 函数职责重分配**：C 版 arch 层有 3 个启动函数——`arch_proc_reset()`（清零寄存器）、`arch_proc_init()`（设 PC/SP/ps_strings）、`arch_boot_proc()`（VM ELF 加载 + 状态构建）。Rust 版不照搬这个划分：C 的 3 函数划分是实现细节，Rust 按 OS 概念分成两个正交操作——"构建状态"（`build_cpu_context`）和"应用状态"（`apply_to_trap_frame`）：

| C 函数 | OS 概念 | Rust 落地 |
|--------|--------|---------|
| `proc_init()` | 进程表初始化为全空槽 | `ProcessTable::new()` const 构造 |
| `arch_proc_reset()` | 为新进程构建初始 CPU 状态 | `build_cpu_context(ProcKind::KernelTask, ...)` |
| `arch_proc_init()` | 为用户进程构建带入口点的 CPU 状态 | `build_cpu_context(ProcKind::Vm, EntrySpec::loaded(...))` |
| `arch_boot_proc()` | 加载 VM ELF + 构建启动状态 | free fn `load_vm_elf()` + `build_cpu_context()` |
| `get_priv()` + 特权设置 | 为进程授予能力 | `PrivTable::grant_capability(nr, CapabilityTemplate::Vm)` |
| boot image 循环 | 按角色编排所有 boot 进程 | `init_proc_and_boot()` 主流程 |

注意 `build_cpu_context` 一个操作承接 C 的 reset/init 两函数：reset 对应 `ProcKind::KernelTask`（无入口点），init 对应 `ProcKind::Vm` 等（带入口点）——"构建状态"按 `ProcKind` + `EntrySpec` 区分两种形态，函数划分按 OS 概念而非按 C 实现细节。

**为什么是 trait 而非 cfg-alias**：3 个架构的 `CpuContext` 字段布局完全不同（x86-64 有段选择子+XSAVE area，aarch64 有 CPACR_EL1 配置，riscv64 有 sstatus.FS），`build_cpu_context` 和 `apply_to_trap_frame` 的实现行为真的不同。trait 提供显式契约 + 支持 mock 测试（`MockCpuContextArch`）+ 零运行时开销（静态分发）。

**为什么是关联类型而非通用结构体（非法状态不可表达）**：如果定义 `struct InitialRegState { status, segment_selectors, fpu_needs_zero }` 供三架构共用，则 (a) `segment_selectors` 在 aarch64/riscv64 永远全零——非法状态可表达（类型系统允许填入无意义值）；(b) `fpu_needs_zero` 在 aarch64/riscv64 永远 false——同理；(c) 所有架构被迫 import `SegmentSelectors` 类型——硬件语义泄漏到 OS 层。关联类型让每个架构定义自己的 `CpuContext`：aarch64 编译时类型系统中**根本不存在** `SegmentSelectors`，"非法状态不可表达"由类型系统强制。这是 Rust 类型设计原则（make invalid states unrepresentable）在硬件抽象上的直接应用。

**enable_user_io 下沉**：x86-64 的 IOPL 位操作是硬件语义，不应泄漏到 kernel 层。通过 `enable_user_io` trait 方法下沉：x86-64 实现设 `psw |= 0x3000`，aarch64/riscv64 用 default no-op。kernel 层调用 `CurrentCpuContextArch::enable_user_io(&mut ctx)`，不接触硬件位。

### 3.6 FPU 架构演进：不翻译 Minix3 的 fnsave [KEY]

> **这是"rewrite not translate"最典型的案例**——FPU 状态管理必须按现代 ISA 模型重新表达。

**本质**：FPU 状态管理是架构演进问题。Minix3 的 `fnsave`/`fxrstor` + `p_seg.fpu_state[FPU_XFP_SIZE]` 是 x86-32 遗留模型（fnsave 是 80387 指令，保存整个 108 字节 FPU 状态）。现代 x86-64 用 XSAVE/XRSTOR + 扩展状态区域；aarch64 用 CPACR_EL1.FPEN 系统级控制；riscv64 用 sstatus.FS 状态机。

**约束驱动（P5）**：不翻译 C 的 `fnsave`/`fxrstor` + `p_seg.fpu_state[FPU_XFP_SIZE]`，用现代 ISA 模型。FPU 完全是 `CpuContext` 的内部字段，kernel 层看不到。

**三架构现代 FPU 模型**：

| 架构 | FPU 控制机制 | per-process 保存区 | 初始化策略 |
|------|------------|-------------------|----------|
| **x86-64** | XSAVE/XRSTOR + XCR0 扩展状态控制 + CR4.OSXSAVE 使能 | XSAVE area（arch 内部） | `fpu_policy` 枚举：KernelTask（不初始化）/ LazyUserInit（首次 FP 指令 trap 时 XSAVE） |
| **aarch64** | CPACR_EL1.FPEN 控制 EL0/EL1 FPU 访问 | 无 per-process 保存区（仅 FPCR/FPSR，context switch 时 save/restore） | `fpu_enable_el0` 布尔（CPACR_EL1 在 cstart 全局配置） |
| **riscv64** | sstatus.FS 字段（Off/Initial/Clean/Dirty 四态） | 无 per-process 保存区 | sstatus.FS = Initial（首次 FP 指令 trap 时 lazy 初始化） |

**x86-64 的 FPU 策略**（arch 内部枚举，OS 看不到）：

```rust
enum X86FpuInitPolicy {
    KernelTask,      // 复用内核 FPU 上下文，不初始化
    LazyUserInit,    // 首次使用时初始化 XSAVE area（现代 lazy 模式，非 memset 清零）
}
```

**假设性推理**：如果翻译 Minix3 的 `fnsave`/`fxrstor` + `p_seg.fpu_state[FPU_XFP_SIZE]`，会泄漏 32 位遗留模型到 OS 层（`fpu_needs_zero: bool` 字段流经 kernel），且无法表达 aarch64/riscv64 的 FPU 控制方式（它们没有 per-process FPU 保存区概念）。现代 XSAVE/CPACR_EL1.FPEN/sstatus.FS 模型才是三架构统一的抽象方向。

### 3.7 VM ELF 加载：free function 而非 trait 方法

**本质**：VM 的 ELF 加载逻辑（解析 ELF + 映射 PT_LOAD 段 + 复制字节 + 清零 BSS + 分配栈）在三架构上**完全相同**——它只依赖 `Paging` trait，不依赖任何架构特定寄存器操作。

**约束驱动**：

- **trait 用于真多态（P3）**→ 三架构实现字节相同，放 trait 是假多态（false polymorphism）
- **归属 crate**→ 放在 `os/arch/src/arch/boot.rs`（arch crate 的 common 模块），因为它依赖 `Paging` trait（arch 拥有）

**free function 设计**：

```rust
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> VmLoadResult
```

用 `minix_elf` crate 替代 C 的 `libexec`，用 `Paging::map()` 替代 `pg_map`。返回 `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`。

**假设性推理**：如果把 `load_vm_elf` 放进 `CpuContextArch` trait，三架构的实现逐行一致（已 grep 验证），trait 派发无意义且增加维护成本（改一处要改三处）。free function 让"三架构共享"成为显式事实，修改只需一处。

### 3.8 零堆启动的工程实现

**本质**：boot 期没有 `GlobalAlloc`，所有数据结构必须编译期固定大小。

**约束驱动（P1）**：

- `ProcessTable`/`PrivTable` 用 `[T; N]` 固定数组，不用 `Box<[T]>`/`Vec<T>`
- `ProcessTable::new()`/`PrivTable::new()` 是 `const fn`，编译期完成初始化
- `KProcess::empty_uninit()` 是 `const fn`，9 个内部类型（`AtomicI32`/`AtomicU64`/`ProcName`/`Endpoint`/`Option` 等）都需要 const-initable（Rust 1.75+ 支持）

**kernel_may_alloc 窗口**：C 在 `bsp_finish_booting()` 中设 `kernel_may_alloc = 0`（main.c:105），关闭 boot 期内存分配窗口。Rust 用 `AtomicBool` 表达，boot 期为 `true`（允许内核直接分配物理内存），`bsp_finish_booting()` 后设 `false`（VM 接管内存管理）。

**假设性推理**：如果在 boot 期用 `Box<[KProcess]>`，`Box::new()` 调用 `GlobalAlloc::alloc`，但 boot 期没有注册 `GlobalAlloc`（堆分配器尚未初始化），会 panic。固定数组 + const fn 是零堆的唯一可行方案。

**fallback：若 `const fn` 受阻**（某些内部类型的 const default 暂时不可行时）：用 `MaybeUninit` 构造数组——先建 `[MaybeUninit<KProcess>; N]` 全 uninit，逐个 `write` 初始化，最后 transmute 为 `[KProcess; N]`：

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
    procs
}
```

但首选仍是让 `KProcess::new()` const——fallback 的 `transmute` 让编译器无法检查未初始化风险，`MaybeUninit` 的 `write` 循环也可能漏掉槽位。

### 3.9 SMP 预留设计

**本质**：Minix3 支持 SMP（多 CPU），进程表是全局共享数据。本设计沿用 Minix3 的 BKL（Big Kernel Lock）模型——所有 CPU 共享一个全局 spinlock，调度决策在 BKL 内串行完成。

**BKL + per-CPU runqueue 的"假并行"语义**：

```
Linux 的真并行调度：
  CPU0 ─→ lock(rq0) ─→ 调度 ─→ unlock(rq0)
  CPU1 ─→ lock(rq1) ─→ 调度 ─→ unlock(rq1)  ← 同时进行

Minix3 的"假并行"调度（BKL 全局串行）：
  CPU0 ─→ BKL_LOCK ─→ 调度 ─→ BKL_UNLOCK
  CPU1 ──── 等待 BKL（自旋）────────────────
  CPU1 ─→ BKL_LOCK ─→ 调度 ─→ BKL_UNLOCK
```

Minix3 的 per-CPU runqueue **不是**为了并行调度，而是为了 cache 局部性 + idle steal（CPU 闲置时从其他 CPU 队列偷进程）。

**SMP 预留字段**：

| 字段 | 类型 | 对应 C 字段 | 用途 |
|------|------|-----------|------|
| `p_cpu` | `AtomicU32` | `p_cpu: unsigned` (proc.h:35) | 当前 CPU |
| `p_cpu_mask` | `CpuMask` | `p_cpu_mask[]` (proc.h:37) | CPU 亲和性位图 |
| `p_nextready` | `AtomicPtr<KProcess>` | `p_nextready` (proc.h:72) | per-CPU 就绪队列链接 |

**并发原语选择**：

| 原语 | 用途 | 为什么不用 Rc/RefCell |
|------|------|---------------------|
| `AtomicU32`/`AtomicI64` | 标志位、计数器 | Rc/RefCell 跨 CPU 不安全（模式 27） |
| BKL spinlock | 全局一致性 | 单线程下无需，SMP 下必须 |
| per-CPU 数据 | CPU 局部状态 | 跨 CPU 读需同步 |

**假设性推理**：如果用 `Rc<RefCell<KProcess>>` 跨 CPU 共享，`RefCell` 的运行时借用检查不是原子操作，两个 CPU 可能同时获得 `&mut`，导致 UB。`AtomicPtr` + BKL 是 SMP 安全的唯一组合。

**per-CPU 数据的 Rust 抽象**（关联 [16-smp.md §?](./16-smp.md)）：Minix3 C 用 `struct __cpu_local_vars` 存放每个 CPU 独立的 `proc_ptr` / `fpu_owner` 等局部状态。本设计用 `CpuLocal<T>`（位于 `os/kernel/src/smp.rs:45-56`）替代原 design 的 `PerCpuData` 提案——`CpuLocal<T>: !Sync` 类型系统保证 per-CPU 数据**编译期禁止跨 CPU 共享引用**，从根上消除 per-CPU 数据被并发访问的可能。Rust 实现细节见 [16-smp.md](./16-smp.md)（per-CPU 抽象 + lazy 初始化）。

**BKL 类型系统强制**（关联 [16-smp.md](./16-smp.md)）：`BklSection<'a>` typed witness（`os/kernel/src/smp.rs:560-578`）把"当前持有 BKL"从注释约定升级为编译期类型证明。`smp_state_with(section, &SmpState)` 等需 `&BklSection<'_>` 的 API 自动拒绝"未持锁调用"，从根上消除"漏 BKL"问题。Rust 实现细节见 [16-smp.md](./16-smp.md)（BklSection witness 设计 + RAII vs non-RAII 取舍）。

> **本节是概念索引**：详细 BKL/per-CPU/调度并行化的设计与代码见 [16-smp.md](./16-smp.md)。本章仅建立"Minix3 是 BKL 全局串行 + per-CPU 局部状态"的心智模型，避免读者在 boot 期误用 `Rc/RefCell`（SMP 跨 CPU UB）。

### 3.10 KPriv 6 子结构分组

**本质**：C 的 `struct priv` 有 30+ 裸字段，缺乏内聚性。Rust 按 OS 语义分组为 6 个子结构。

**6 子结构**：

| 子结构 | 语义 | 包含字段 |
|--------|------|---------|
| `PrivCapability` | 进程被允许做什么（静态配置） | flags/trap_mask/ipc_targets/kernel_calls/signal_manager |
| `PrivSignals` | 信号管理 + 待处理信号 | sig_pending/notify_pending/asyn_pending |
| `PrivIo` | I/O 端口和内存映射 | io_tab/asyn_tab/asyn_size/asyn_endpoint |
| `PrivMem` | 内存配额和映射 | ipcf 等 |
| `PrivIrq` | IRQ 授权 | int_pending/irq_tab |
| `PrivRuntime` | 运行时状态（不属于"能力"） | s_init_flags/alarm_timer 等 |

**假设性推理**：如果把 priv 的 30+ 字段平铺在一个结构体里，`grant_capability` 需要在一堆无关字段中找到要写的 5 个能力字段，可读性差且易错。6 子结构让"写能力只触碰 `capability` 子结构"成为视觉事实。

### 3.11 调度字段与统计设计

**本质**：进程的调度属性（优先级/时间片/CPU 归属）和运行时统计（CPU 时间/IPC 计数）是两类正交数据。

**SchedFields 设计**：

```rust
pub struct SchedFields {
    pub priority: AtomicI8,        // 优先级（决定 enqueue 到哪个 NR_SCHED_QUEUES 子队列）
    pub quantum: Quantum,          // 时间片
    pub cpu: AtomicU32,            // 当前 CPU
    pub cpu_mask: CpuMask,         // CPU 亲和性
    pub scheduler: Option<ProcNr>, // 调度器归属
}
```

**p_priority/p_quantum 覆写**：C 在 boot 循环中对 VM/RS 覆写 `p_priority=SRV_Q`/`p_quantum_size_ms=SRV_QT`（main.c:215-220），内核 task 不覆写（保持 0），非 schedulable 用户进程保持 0（等 RS 运行时设）。Rust 在 `init_proc_and_boot()` 的 Step 3b 中实现同样的覆写逻辑。

**reset_proc_accounting**：C 在 boot 循环中调用 `reset_proc_accounting(rp)` 重置统计（main.c:172）。Rust 的 `Accounting`/`TimeStats`/`CyclesStats` 结构体在 `KProcess::empty_uninit()` 中 const-init 为零值，boot 期无需额外重置。

### 3.12 boot→running 转换设计

**本质**：阶段 C 结束时，所有 boot 进程"就位但暂停"（带 `RTS_PROC_STOP`）。boot 流程的真正终点是 `bsp_finish_booting()` 清除停止标志，唤醒 boot 进程并入调度队列。

**bsp_finish_booting 的 Rust 设计**：

```rust
pub fn bsp_finish_booting(proc_table: &mut ProcessTable, smp_state: &mut SmpState) -> ! {
    // 1. 唤醒 boot 进程：清除 RTS_PROC_STOP（RTS_UNSET 自动 enqueue）
    for i in 0..(NR_BOOT_PROCS - NR_TASKS) {
        if let Some(proc) = proc_table.get_mut(ProcNr::from_user_index(i)) {
            proc.rts_unset(RtsFlags::PROC_STOP);
        }
    }
    // 2. proc_ptr/bill_ptr 初始化（当前运行进程/计费进程指向 IDLE）
    // 3. fpu_init（CPU 级 FPU 使能）
    // 4. kernel_may_alloc = false（关闭 boot 期内存分配窗口）
    // 5. switch_to_user（切换到第一个用户态进程，引用 10-switch-to-user.md）
}
```

**唤醒循环只遍历用户态 boot 进程**（`i < NR_BOOT_PROCS - NR_TASKS`），不含内核 task——内核 task 的 `RTS_PROC_STOP` 在 proc_init 中设置且永不清除（IDLE）或在 boot 循环中已处理。

**kernel_may_alloc 窗口关闭**：`AtomicBool` 从 `true` 设为 `false`，VM 即将接管内存管理，内核不再直接分配物理内存。

### 3.13 错误处理与不变量表达

**本质**：Rust 类型系统可以强制表达 C 中靠注释维护的不变量。

**Option 替代"always-embedded + flag"**：

| C 模式 | Rust 模式 | 强制的不变量 |
|--------|----------|------------|
| `s_proc_nr = NONE` + slot 存在 | `s_proc_nr: Option<ProcNr>` | "slot 未分配"= `None`，编译器强制处理 |
| `p_vm_suspend = NULL` + 隐式 | `p_vm_suspend: Option<VmRequest>` | `RTS_VMREQUEST <==> p_vm_suspend.is_some()` |
| `p_sendmsg = NULL` + 隐式 | `p_sendmsg: Option<Message>` | `RTS_SENDING <==> p_sendmsg.is_some()` |

**错误处理策略**：

| 场景 | 策略 | 理由 |
|------|------|------|
| boot 期不变量违反 | `panic!` | boot 期错误不可恢复，panic 是唯一选择 |
| 运行时能力分配失败 | `Result<PrivId, CapabilityError>` | 调用方可处理（如 RS 重试） |
| ELF 加载失败 | `Result<VmLoadResult, VmLoadError>` | 不静默失败，返回具体错误 |

**假设性推理**：如果用"always-embedded + flag"（字段始终存在 + bool 标志位），调用方可能忘记检查 flag 直接访问字段，导致读到无效数据。`Option` 让"未初始化"成为类型信息，编译器强制 `match`/`if let` 处理。

### 3.14 对照：fork 运行时路径（非 boot 路径）

> **范围声明**：06 文档聚焦 boot 路径；fork 是运行时路径，仅作为对照提及。

**fork_from 设计要点**：

- 继承调度属性（priority/quantum/cpu/cpu_mask）与 IPC 端点
- 重置 accounting/cpuavg（子进程不继承父进程的 CPU 时间统计）
- 队列指针独立（`p_nextready` 清零，子进程未入队）
- FPU 状态继承：`inherit_fpu_state` 把父进程 FPU 策略复制给子进程（fork 与 boot 路径的交汇点）
- `EXT_REG_INITIALIZED` 标志继承：`MiscFlagsBits::EXT_REG_INITIALIZED` 随 fork 从父进程复制给子进程（[proc.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs) `fork_from`），是 x86-64 lazy FPU 切换的关键——标记"该进程的 XSAVE area 已初始化"；`exec` 完成时清除、信号返回/mcontext 恢复时重新设置（`syscall_process.rs`）。aarch64/riscv64 不需要此标志（FPU 状态由 trap frame 自带 FPCR/FPSR 或 sstatus.FS 表达）。该标志是 boot 后运行时路径（exec/signal/fork）与 boot 期 FPU 策略（§3.6 `fpu_policy` 枚举）的衔接点
- RTS 继承策略：子进程不继承 `SENDING`/`RECEIVING`/`PREEMPTED`；初始带 `PROC_STOP`；清 `SLOT_FREE`

### 3.15 设计决策汇总表

| 决策 | 约束 | 替代方案 | 理由 |
|------|------|---------|------|
| 固定数组 `[KProcess; N]` | 零堆（P1） | `Box<[KProcess]>` | boot 期无 GlobalAlloc |
| `const fn new()` | 零堆（P1） | 运行时构造 | BSS 段零运行时开销 |
| `AtomicPtr`/`Option<ProcNr>` | SMP（P3） | 裸指针 | 跨 CPU 共享需原子/索引 |
| `CapabilityTemplate` 枚举 | 能力是数据（P6） | 6 裸参数 | 类型安全 + 角色映射显式 |
| 单 `CpuContextArch` trait | 真多态（P3） | 3 trait 镜像 C | 消除假多态 + 因果链编造 |
| `CpuContext` 不透明关联类型 | 硬件抽象（P4） | 拆解为 `initial_*` 字段 | arch 私有，kernel 不 inspect |
| FPU 下沉到 arch | FPU 演进（P5） | `fpu_needs_zero` 泄漏 | 现代 XSAVE/CPACR/sstatus.FS |
| `enable_user_io` trait 方法 | 硬件下沉（P7） | kernel 操作 IOPL 位 | x86 特有语义不泄漏 |
| free fn `load_vm_elf` | 真多态（P3） | trait 方法 | 三架构相同，假多态 |
| `Option<T>` 替代 flag | 不变量表达 | always-embedded + bool | 编译器强制处理 |
| BKL + per-CPU runqueue | SMP（P3） | per-CPU rq lock | Minix3 简化模型 |
| `KPriv` 6 子结构 | 可读性 | 30+ 裸字段 | 语义分组内聚 |

---

## Ch4. 实现详解（HOW，具体代码）

> **本章目标**：讲 HOW——具体 trait 实现、结构体字段、主流程代码。**与 Ch3 边界**：Ch3=WHY（约束驱动设计），Ch4=HOW（具体实现）。

### 4.0 实现地图：init_proc_and_boot() 主流程

Rust boot 主流程入口是 `init_proc_and_boot(kernel_info: &KernelInfo)`（[lib.rs:706](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)），对应 C 的 `proc_init()` + main.c boot image 循环。

```
init_proc_and_boot(kernel_info)
├── Step 1+2: 获取全局 PROC_TABLE / PRIV_TABLE（static mut BSS）
├── Step 2b: 校验 boot_modules.len() == NR_BOOT_MODULES
├── Step 3a: 遍历 KERNEL_TASKS（IDLE/CLOCK/SYSTEM/...）
│   ├── set_boot_name(name)
│   ├── grant_capability(nr, Idle|KernelTask)
│   ├── build_cpu_context(KernelTask, KERNEL_TASK)
│   └── RTS_SET(PROC_STOP); RTS_CLEAR(SLOT_FREE)
├── Step 3b: 遍历 boot_modules（PM/RS/VM/...）
│   ├── set_boot_name(module.name)
│   ├── schedulable = is_root_sys || is_vm
│   ├── grant_capability(nr, Vm|RootService)  [if schedulable]
│   │   或 RTS_SET(NO_PRIV|NO_QUANTUM)          [if not]
│   ├── load_vm_elf(module) → EntrySpec::loaded  [if VM, mock]
│   ├── add_memmap(module.start, module.len)     [FIX-23: reclaim, mock only]
│   ├── build_cpu_context(proc_kind, entry)
│   ├── RTS_SET(VMINHIBIT|BOOTINHIBIT)  [if not VM]
│   └── RTS_SET(PROC_STOP); RTS_CLEAR(SLOT_FREE)
└── Step 4: boot_procs 信息已在 kernel_info 中
```

后续 `bsp_finish_booting()`（[lib.rs:1162](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）负责唤醒：RTS_UNSET(PROC_STOP) 循环 + 时钟/FPU 初始化 + `switch_to_user()`。

### 4.1 arch 层：CpuContextArch trait 实现

`CpuContextArch` trait（[boot.rs:128](file:///home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs)）是 kernel 层与 arch 层的唯一接口。三架构各自实现：

**x86_64**（[x86_64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/boot.rs)）：

```rust
pub struct X86_64CpuContext {
    pub psw: u64,      // RFLAGS 初值（INIT_PSW / INIT_TASK_PSW）
    pub cs: u16,       // USER_CS_SELECTOR
    pub ds: u16,       // USER_DS_SELECTOR
    pub ss: u16, pub es: u16, pub fs: u16, pub gs: u16,
    pub rip: u64,      // entry.pc
    pub rsp: u64,      // entry.sp
    pub rbx: u64,      // entry.ps_strings（argv 指针）
    pub fpu_policy: X86FpuInitPolicy,  // KernelTask | LazyUserInit
}
```

`build_cpu_context` 根据 `ProcKind` 选 `INIT_PSW`（用户）或 `INIT_TASK_PSW`（内核 task），填段选择子，设 FPU 策略。`enable_user_io` 设 PSW.IOPL=3（x86 特有，驱动需直接 IN/OUT）。`inherit_fpu_state` 复制父进程 `fpu_policy` 给子进程。

**aarch64**（[arm64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/boot.rs)）：

```rust
pub struct AArch64CpuContext {
    pub psr: u64,            // INIT_PSR / INIT_TASK_PSR
    pub pc: u64,             // entry.pc
    pub sp: u64,             // entry.sp
    pub r0: u64,             // entry.ps_strings
    pub fpu_enable_el0: bool, // CPACR_EL1.FPEN 位（per-process）
}
```

`build_cpu_context` 根据 `ProcKind` 设 `INIT_PSR`（EL0 用户）或 `INIT_TASK_PSR`（EL1 内核 task）。用户进程 `fpu_enable_el0=true`，内核 task `false`。无 `enable_user_io`（ARM 用 MMIO 映射替代 IOPL）。

**riscv64**（[riscv64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/boot.rs)）：

```rust
pub struct Riscv64CpuContext {
    pub sstatus: u64,  // INIT_USER_SSTATUS / INIT_TASK_SSTATUS
    pub sepc: u64,     // entry.pc
    pub sp: u64,       // entry.sp
    pub a0: u64,       // entry.ps_strings
}
```

`build_cpu_context` 设 `sstatus.SPP`（1=内核 task，0=用户）和 `SPIE`（用户=1）。FPU 通过 `sstatus.FS` 字段控制（`Initial` 状态，首次 FP 指令 trap 到内核做 lazy init）。

### 4.2 arch 层：load_vm_elf 共享实现

`load_vm_elf` 是 free function（[boot.rs:203](file:///home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs)），非 trait 方法——三架构实现字节相同，放 trait 是假多态。

```rust
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> Result<VmLoadResult, VmLoadError>
```

流程：用 `minix_elf` crate 解析 ELF → 逐页映射 PT_LOAD 段（identity mapping，paddr=vaddr）→ 复制段字节 / 清零 BSS → 分配用户栈（VM_STACK_SIZE=64KB）→ 返回 `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`。

ELF 段标志映射：`PF_R|PF_W|PF_X` → `PageFlags::PRESENT | USER_ACCESSIBLE | WRITABLE | EXECUTABLE`（[boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs) `elf_flags_to_page_flags`）。

错误处理：`VmLoadError::InvalidElf`（ELF 无法解析）/ `MappingFailed`（paging 失败），返回 `Result`，无静默失败。

**FIX-24 (Phase 9): 真实 VM ELF 加载已实现**（mock + 非 mock 路径都已接通）。非 mock 路径通过 `Paging::from_active_root(current_root_phys())` 包装 `arch_boot_impl` 创建并激活的 bootstrap 页表，将 VM ELF 段直接映射进去（identity mapping, paddr=vaddr）。加载完成后 module 物理内存立即通过 `memmap::add_memmap` 回收（撤销 Phase A.2 的 `cut_memmap`）。VM 的 `p_seg.phys_root`/`virt_root` 也同步记录为 bootstrap 根，使随后的 `init_post_and_memory` 能将 VM 安装为 ptproc（`set_ptproc` + `set_current_ptproc_nr`）。VMCTL SetAddrSpace 在 VM 安装自己的页表后会通过 `TlbArch::set_active_root` 替换硬件根（CR3/TTBR0/satp）。详见 [09-vm-boot-protocol.md §4.8](09-vm-boot-protocol.md)。

**FIX-23: boot module 物理内存回收**（mock + 非 mock 路径）：`load_vm_elf` 成功返回后，`init_proc_and_boot` 立即调 `memmap::add_memmap(FREE_MEMMAP, module.start, module.len)` 把 module 的物理内存归还给 kernel 分配器（[lib.rs:901-904 mock](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) / [lib.rs:942-945 非 mock](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。C 对照：`protect.c:450-451` `mod->mod_start = mod_end = 0` 标记已消费。这撤销了 `kmain` Phase A.2 的 `cut_memmap` 临时切除（[lib.rs:326-333](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)），让 module 物理页重新可用。详见 [01-boot-shim-bootstrap.md §2.5 boot module 内存生命周期](01-boot-shim-bootstrap.md)。

### 4.3 kernel 层：ProcessTable 与 PrivTable

**ProcessTable**（[proc_table.rs:57](file:///home/xzhao/github/minix-rs/os/kernel/src/proc_table.rs)）：

```rust
pub struct ProcessTable {
    procs: [KProcess; PROC_TABLE_SIZE],
    sched: Scheduler,
    vm_request_queue: VmRequestQueue,
}
```

`const fn new()`（[proc_table.rs:75](file:///home/xzhao/github/minix-rs/os/kernel/src/proc_table.rs)）：BSS 零初始化 + per-slot 设 `p_nr`/`p_endpoint` + IDLE slot 特殊处理（`PROC_STOP` + name="IDLE"）。全局 `static PROC_TABLE: SyncUnsafeCell<ProcessTable>`（BSS），通过 `crate::proc_table()` 获取。

**`SyncUnsafeCell` 与 `BklProtected` marker trait**（FIX-07: R-01 soundness 修复）：`SyncUnsafeCell` 的 `unsafe impl Sync` 不再是无约束 blanket impl，而是 gated on sealed marker trait `BklProtected`（[lib.rs `bkl_protected` 模块](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）。只有 9 个已审计类型可被 `SyncUnsafeCell` 包裹：`ProcessTable`/`PrivTable`/`IrqManager`/`SmpState`/`FreePdeSlots`/`IpcFilterPool`/`KRandomness`（BKL 串行化）+ `KernelInfo`/`MemMapEntry`（write-once-read-only after boot）。外部 crate 无法实现 `BklProtected`（sealed pattern），从根上消除 `RefCell<T>`/`Rc<T>`/`Cell<T>` 等 `!Sync` 类型被误装进 `static` 的 soundness 漏洞。

关键方法：`get(nr) -> Option<&KProcess>` / `get_mut(nr)` / `is_valid_nr(nr)` / `is_kernel(nr)` / `is_empty(nr)` / `rts_set` / `rts_unset`（自动维护调度队列）。

**PrivTable**（[kpriv.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/kpriv.rs)）：

```rust
pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}
```

`const fn new()`：per-slot 设 `s_id`，`s_proc_nr=None`。关键方法：`assign_static(proc_nr) -> Option<PrivId>` / `configure_boot_priv(...)` / `grant_capability(proc_nr, template) -> Result<PrivId, CapabilityError>`。

SMP 安全：所有方法要求持有 BKL（boot 期单线程，无需锁）。

### 4.4 kernel 层：KProcess 结构

`KProcess`（[proc.rs:767](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs)）字段按语义分组：

| 分组 | 字段 | C 对应 |
|------|------|--------|
| 标识 | `p_nr`, `p_endpoint`, `p_name` | proc.h |
| 状态 | `p_rts_flags` (RtsFlags bitflags), `p_misc_flags` (MiscFlags bitflags) | p_rts_flags, p_misc_flags |
| 调度 | `p_sched: SchedFields { priority, quantum, cpu, cpu_mask, scheduler }` | p_priority, p_quantum_size_ms, p_cpu, p_cpu_mask, p_scheduler |
| 统计 | `p_accounting: Accounting`, `p_time: TimeStats`, `p_cycles: CyclesStats`, `p_cpuavg: CpuAvg` | p_accounting, p_user_time, p_cycles, p_cpuavg |
| IPC | `p_nextready`, `p_caller_q`, `p_q_link` (AtomicI32), `p_getfrom_e`, `p_sendto_e` | p_nextready, p_caller_q, p_q_link, p_getfrom_e, p_sendto_e |
| VM | `p_seg: ProcessSegments`, `priv_id: Option<PrivId>` | p_seg, p_priv |

**fork_from**（[proc.rs:1385](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs)）：运行时路径（非 boot 路径），创建子进程：
- 继承调度属性（priority/quantum/cpu/cpu_mask）与 IPC 端点（`p_getfrom_e`/`p_sendto_e`）
- 重置 accounting/time/cycles/cpuavg（子进程不继承父进程 CPU 时间统计）
- 队列指针独立（`p_nextready`/`p_caller_q`/`p_q_link` = NONE）
- RTS 修正：设 `NO_QUANTUM`，清 `SIGNALED`/`SIG_PENDING`/`P_STOP`/`VMREQUEST`
- MiscFlags 修正：清 `VIRT_TIMER`/`PROF_TIMER`/`SC_TRACE`/`SPROF_SEEN`/`STEP`
- FPU 继承：调 `inherit_fpu_state`（arch trait 方法）

### 4.5 kernel 层：能力授予

`grant_capability`（[kpriv.rs:494](file:///home/xzhao/github/minix-rs/os/kernel/src/kpriv.rs)）是"分配+配置"原子操作，替代 C 的 `get_priv` + 散落 `s_flags`/`s_trap_mask` 赋值：

```rust
pub fn grant_capability(
    &mut self,
    proc_nr: ProcNr,
    template: CapabilityTemplate,
) -> Result<PrivId, CapabilityError>
```

5 模板（[capability.rs:128](file:///home/xzhao/github/minix-rs/os/kernel/src/capability.rs)）：

| 模板 | s_flags | ipc_to | k_call_mask | 用途 |
|------|---------|--------|-------------|------|
| `Idle` | IDL_F (SYS_PROC\|BILLABLE) | NONE | NONE | IDLE 进程 |
| `KernelTask` | TSK_F (SYS_PROC) | NONE | NONE | CLOCK/SYSTEM 等 |
| `Vm` | VM_F\|SRV_F\|BILLABLE | ALL | ALL | VM 进程 |
| `RootService` | RSYS_F\|SRV_F\|BILLABLE | ALL | ALL | RS 进程 |
| `Deferred` | — | — | — | 延迟分配（运行时） |

内部流程：`assign_static(proc_nr)` → 查模板 `capabilities()`/`trap_mask()`/`ipc_mask()`/`kcall_mask()` → `configure_boot_priv`。重复分配返回 `Err(SlotOccupied)`。

### 4.6 kernel 层：KPriv 6 子结构

`KPriv` 按 Minix3 `struct priv` 语义分 6 子结构：

| 子结构 | 字段 | C 对应 |
|--------|------|--------|
| `PrivCapability` | s_proc_nr, s_flags, s_init_flags, s_id | priv.h capability 部分 |
| `PrivSignals` | s_sig_mgr, s_bak_sig_mgr, s_sig_pending | 信号管理 |
| `PrivIpc` | s_trap_mask, s_ipc_to, s_k_call_mask | IPC/陷阱掩码 |
| `PrivIo` | s_io_grant, s_io_tab | I/O 端口范围 |
| `PrivMem` | s_mem_grant, s_mem_tab | 内存区域 |
| `PrivRuntime` | s_irq_mask, s_int_pending | 中断管理 |

每子结构有 `const fn new()`，支持 `PrivTable::new()` 编译期初始化。

### 4.7 主流程：init_proc_and_boot()

`init_proc_and_boot`（[lib.rs:706](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）的 Step 3a/3b 对称结构：

**Step 3a**（内核 task）：遍历 `KERNEL_TASKS`（编译期硬编码），每步：
1. `set_boot_name(name)` — 设进程名
2. `grant_capability(nr, Idle|KernelTask)` — 分配特权
3. `build_cpu_context(KernelTask, KERNEL_TASK)` — arch 构建 CPU 上下文
4. `RTS_SET(PROC_STOP)` + `RTS_CLEAR(SLOT_FREE)` — 标记已占用但暂停

**Step 3b**（用户进程）：遍历 `boot_modules`，每步：
1. `set_boot_name(module.name)`
2. `schedulable = is_root_sys || is_vm` — 判定可调度性
3. 可调度 → `grant_capability(nr, Vm|RootService)`；不可调度 → `RTS_SET(NO_PRIV|NO_QUANTUM)`
4. VM（mock + 非 mock，FIX-24）→ `load_vm_elf` → `EntrySpec::loaded`；其他 → `EntrySpec::DEFERRED`
5. `build_cpu_context(proc_kind, entry)`
6. 非 VM → `RTS_SET(VMINHIBIT|BOOTINHIBIT)` — 等 VM 建页表
7. `RTS_SET(PROC_STOP)` + `RTS_CLEAR(SLOT_FREE)`

`schedulable` 判定对应 C `iskerneln(proc_nr) || isrootsysn(proc_nr) || proc_nr == VM_PROC_NR`（main.c:173）。

### 4.8 boot→running：bsp_finish_booting

`bsp_finish_booting`（[lib.rs:1162](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs)）是 boot 流程终点，类型 `-> !`（never returns）：

```
bsp_finish_booting(proc_table, smp_state) -> !
├── Step 1: VM_RUNNING.store(false)          — VM 尚未运行
├── Step 2: proc_table.set_bill_to_idle()    — bill_ptr/proc_ptr = IDLE
├── Step 3: EarlyConsole::write_str(banner)  — announce()
├── Step 4: for nr in 0..NR_BOOT_PROCS-NR_TASKS:
│           proc_table.rts_unset(nr, PROC_STOP)  — 唤醒 boot 进程
├── Step 5: smp_state.cpu_local_mut(bsp).note_context_switch(tsc)  — cycles_accounting_init
├── Step 6: clock_arch.init_timer(DEFAULT_HZ)  — boot_cpu_init_timer
├── Step 7: bsp_local.fpu_presence = true     — fpu_init
├── Step 8: KERNEL_MAY_ALLOC.store(false)     — 内核不再分配内存
├── Step 8.5: smp::bkl_lock()                 — 获取 BKL
└── Step 9: switch_to_user()                  — 首次调度，不返回
```

Step 4 的 `rts_unset` 自动将新就绪进程加入调度队列（`ProcessTable::rts_unset` 内部调 `Scheduler::enqueue`）。Step 9 `switch_to_user()` 在 09 文档详解。

---

## Ch5. 测试要点

> **本章目标**：验证 Rust 实现的正确性。分 arch/kernel/集成三层 + L3 grep 证据。

### 5.0 测试策略

**三层测试架构**：

| 层级 | 测试目标 | 测试方式 | 设计原则 |
|------|---------|---------|---------|
| **arch 层** | CpuContextArch trait 实现 + load_vm_elf | 单元测试（每架构独立） | 三架构对照，覆盖 FPU 策略差异 |
| **kernel 层** | ProcessTable/PrivTable/KProcess/RtsFlags | 单元测试（mock arch） | rts_set/rts_unset 队列一致性是隐藏不变量 |
| **集成层** | init_proc_and_boot 主流程 + bsp_finish_booting | 集成测试（mock paging） | 端到端验证 boot 流程 |

boot 期 panic（`assert_eq!` / `expect`）vs 运行时 `Result`：boot 期错误是不可恢复的（配置错误），用 panic；运行时错误可恢复，用 `Result`。

### 5.1 arch 层测试

**x86_64**（[x86_64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/x86_64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_psw` | KernelTask 的 PSW=INIT_TASK_PSW，cs/ds=USER_CS/DS_SELECTOR |
| `user_process_uses_init_psw` | Vm 的 PSW=INIT_PSW，rip/rsp/rbx 从 EntrySpec::loaded 填入 |
| `enable_user_io_sets_iopl` | `enable_user_io` 后 PSW.IOPL=3（0x3000） |
| `default_ctx_is_kernel_task_shape` | `Default::default()` 全零，有意义值来自 build_cpu_context |
| `apply_to_trap_frame_copies_registers` | CpuContext → TrapFrame 寄存器复制正确 |
| `inherit_fpu_state_propagates_lazy_user_policy` | fork 时子进程继承父进程 fpu_policy |

**aarch64**（[arm64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_psr` | KernelTask 的 PSR=INIT_TASK_PSR，fpu_enable_el0=false |
| `user_process_uses_init_psr_and_fpen_user` | Vm 的 PSR=INIT_PSR，fpu_enable_el0=true |
| `all_user_kinds_get_fpen_user` | Vm/RootService/UserService/UserProcess 均 fpu_enable_el0=true |
| `default_ctx_is_zeroed` | Default 全零 |
| `inherit_fpu_state_propagates_fpu_enable_el0` | fork 时子进程继承父进程 fpu_enable_el0 |

**riscv64**（[riscv64/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `kernel_task_uses_init_task_sstatus` | KernelTask 的 sstatus=INIT_TASK_SSTATUS，SPP=1 |
| `user_process_uses_init_sstatus` | Vm 的 sstatus=INIT_USER_SSTATUS，SPP=0，SPIE=1 |
| `default_ctx_is_zeroed` | Default 全零 |
| `inherit_fpu_state_copies_sstatus` | fork 时子进程继承父进程 sstatus |

**load_vm_elf**（[arch/boot.rs](file:///home/xzhao/github/minix-rs/os/arch/src/arch/boot.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `entry_spec_kernel_task_all_none` | KERNEL_TASK 的 pc/sp/ps_strings 全 None |
| `entry_spec_deferred_all_none` | DEFERRED 的 pc/sp/ps_strings 全 None |
| `entry_spec_loaded_sets_all_some` | loaded() 的 pc/sp/ps_strings 全 Some |
| `elf_flags_rwx_to_page_flags` | PF_R\|PF_W\|PF_X → PRESENT\|USER\|WRITABLE\|EXECUTABLE |
| `elf_flags_r_only` | PF_R → PRESENT\|USER，无 WRITABLE/EXECUTABLE |
| `load_vm_elf_invalid_elf_returns_err` | 无效 ELF 返回 `Err(InvalidElf)` |

**memmap cut_memmap**（[memmap.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/memmap.rs) tests 模块，FIX-23）：

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

**ProcessTable**（[proc_table.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/proc_table.rs) tests 模块）：

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

**PrivTable / grant_capability**（[kpriv.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/kpriv.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_priv_table_new` | const fn 可编译，per-slot s_id 正确 |
| `test_priv_table_const_init_sets_per_slot_s_id` | s_id 按 slot 索引递增 |
| `test_priv_table_assign_static` | assign_static 分配 priv slot |
| `test_priv_table_assign_static_user_proc` | 用户进程 assign_static |
| `test_priv_table_configure_boot_priv` | configure_boot_priv 设 flags/masks |
| `test_grant_capability_idle` | Idle 模板：IDL_F flags |
| `test_grant_capability_vm` | Vm 模板：VM_F flags，ALL masks |
| `test_grant_capability_root_service` | RootService 模板：RSYS_F flags |
| `test_grant_capability_deferred_no_flags` | Deferred 模板：无 flags |
| `test_grant_capability_duplicate_fails` | 重复分配返回 `Err(SlotOccupied)` |

**KProcess / fork_from**（[proc.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_kprocess_new` | new_zeroed 全零 |
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

### 5.3 集成测试

**init_proc_and_boot**（[boot_integration.rs:138](file:///home/xzhao/github/minix-rs/os/kernel/tests/boot_integration.rs)）：

| 测试名 | 验证点 |
|--------|--------|
| `init_proc_and_boot_test` | ProcessTable 创建 + VM slot 存在 + p_seg 默认状态 + p_magic(PMAGIC) |

**bsp_finish_booting**（[lib.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/lib.rs) tests 模块）：

| 测试名 | 验证点 |
|--------|--------|
| `test_bsp_finish_booting_step_5_7_side_effects` | Step 5 cycles_accounting + Step 7 fpu_presence |
| `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | 单 CPU 下仅 BSP 初始化，AP CPU 保持默认 |

### 5.4 L3 grep 证据：旧 API 0 残留

旧 API（`assign_static` + `configure_boot_priv` 两步调用）已被 `grant_capability` 替代，但底层方法仍保留（`grant_capability` 内部调用）。grep 证据验证上层不再直接调用旧 API：

```bash
# grant_capability 调用点（上层应只用 grant_capability，不直接调 assign_static + configure_boot_priv）
rg "grant_capability" os/kernel/src/lib.rs
# → init_proc_and_boot 中 3 处调用（kernel task / VM / RS）

# 旧字段 initial_pc/initial_sp 0 残留
rg "initial_pc|initial_sp|initial_ps_strings_reg|initial_status" os/ --type rust -g '!*.md'
# → 0 matches（旧字段已移除，改用 EntrySpec）
```

**完整的验证不变量集**（重构完成的 4 项 grep 检查）：

```bash
# 1. 硬件术语泄漏检查：OS 层不得出现 arch 内部类型名
rg "SegmentSelectors|fpu_needs_zero|InitialRegState" os/kernel/src/
# → 0 matches（这些名字只允许出现在 os/arch/src/）

# 2. 旧 trait 0 残留
rg "ArchProcReset|ArchProcInit|BootProcArch" os/ --type rust
# → 0 matches（3 个 trait 已合并为 CpuContextArch）

# 3. boot 阶段 alloc 不可达
rg "alloc::" os/kernel/src/ --type rust | rg "boot|init_proc"
# → 0 matches（boot 路径不得调用分配器，见 §3.8）

# 4. arch 抽象唯一入口：kernel 层只经 CurrentCpuContextArch 类型别名访问
rg "CurrentCpuContextArch" os/kernel/src/
# → 只出现在 trait 调用处（build_cpu_context / apply_to_trap_frame / enable_user_io）
```

检查 1 验证"硬件语义零泄漏"（§3.5/§4.1 的设计约束），检查 2 验证 trait 合并完成（旧 3 trait 只应存在于 git 历史），检查 3 验证零堆约束（§3.8），检查 4 验证 kernel 层无直接 arch 类型引用（§4.1）。

### 5.5 测试覆盖矩阵

| 知识点 | 测试用例 | 层级 | 状态 |
|--------|---------|------|------|
| A.8 ProcessTable | `test_process_table_new` / `test_process_table_const_init_per_slot_nr` / `test_process_table_const_init_idle_name` | kernel | 已实现 |
| A.9 KProcess 结构 | `test_kprocess_new` / `test_kprocess_runnable` | kernel | 已实现 |
| B.8 PrivTable | `test_priv_table_new` / `test_priv_table_const_init_sets_per_slot_s_id` | kernel | 已实现 |
| B.10 CapabilityTemplate | `test_grant_capability_idle` / `test_grant_capability_vm` / `test_grant_capability_root_service` | kernel | 已实现 |
| B.12 boot 期授予 | `test_grant_capability_deferred_no_flags` / `test_grant_capability_duplicate_fails` | kernel | 已实现 |
| C.5 RtsFlags | `test_rts_set_unset` / `test_rts_set_unset_multiple_flags` / `test_rts_set_idempotent` | kernel | 已实现 |
| D.7 init_proc_and_boot | `init_proc_and_boot_test` | 集成 | 已实现 |
| E.8 三架构 CpuContext | `kernel_task_uses_init_task_psw` / `kernel_task_uses_init_task_psr` / `kernel_task_uses_init_task_sstatus` | arch | 已实现 |
| E.12 enable_user_io | `enable_user_io_sets_iopl` | arch | 已实现 |
| F.5-F.9 load_vm_elf | `load_vm_elf_invalid_elf_returns_err` / `elf_flags_rwx_to_page_flags` | arch | 已实现 |
| H.2-H.4 fork 继承 | `test_fork_from_basic` / `test_fork_from_accounting_reset` / `test_fork_from_independent_queues` | kernel | 已实现 |
| L.1 CapabilityError | `test_grant_capability_duplicate_fails` | kernel | 已实现 |
| M.5 SchedFields | `test_sched_fields_new` | kernel | 已实现 |
| N.9 bsp_finish_booting | `test_bsp_finish_booting_step_5_7_side_effects` / `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | kernel | 已实现 |

---

## 附录 A. libexec_load_elf 框架详解

> **附录目标**：深入讲 C 的 `libexec_load_elf` 通用框架，作为 §2.7 的补充。Rust 用 `minix_elf` crate 替代，但理解 C 框架有助于对照。

### A.1 exec_info 结构体字段全集

C 的 `struct exec_info`（libexec.h）封装 ELF 加载所需全部信息：

| 字段 | 类型 | 用途 |
|------|------|------|
| `stack_high` | vir_bytes | 用户栈顶地址 |
| `stack_size` | size_t | 栈大小 |
| `proc_e` | endpoint_t | 目标进程端点 |
| `hdr` | Elf_Ehdr | ELF 头 |
| `hdr_len` | size_t | ELF 头长度 |
| `filesize` | size_t | 文件大小 |
| `progname` | char[] | 程序名 |
| `frame_len` | size_t | 栈帧长度（argc/argv/envp） |
| `pc` | reg_t | 入口 PC（输出） |
| `load_offset` | vir_bytes | 加载偏移（输出） |

加 6 个回调函数指针字段（见 A.2）。

### A.2 6 回调机制详解

`libexec_load_elf` 通过 6 回调抽象内存操作，使框架与具体内存管理解耦：

| 回调 | 签名 | 用途 |
|------|------|------|
| `copymem` | (dst, src, len) | 复制段字节到映射页 |
| `clearmem` | (dst, len) | 清零 BSS |
| `allocmem_prealloc_junk` | (vaddr, len) | 预分配页（保留旧内容） |
| `allocmem_prealloc_cleared` | (vaddr, len) | 预分配页（清零） |
| `allocmem_ondemand` | (vaddr, len) | 按需分配页（page fault 时） |
| `clearproc` | (proc_e) | 清除进程残留状态 |

按 ELF 段类型调用不同回调：PT_LOAD 的 `p_filesz>0` 段用 `allocmem_prealloc_cleared` + `copymem`；`p_memsz>p_filesz` 的 BSS 部分用 `clearmem`。

### A.3 ELF 段处理流程

```
libexec_load_elf(exec_info)
├── elf_unpack(hdr) → 解析 ELF 头
├── elf_has_interpreter(hdr) → 检查 interpreter（动态链接器）
├── for phdr in program_headers:
│   └── if phdr.p_type == PT_LOAD:
│       ├── allocmem_prealloc_cleared(vaddr, memsz)
│       ├── copymem(vaddr, file_offset, filesz)
│       └── clearmem(vaddr + filesz, memsz - filesz)  [BSS]
├── allocmem_prealloc_cleared(stack_high - stack_size, stack_size)
├── exec_info.pc = hdr.e_entry
└── return OK
```

Rust 的 `load_vm_elf` 用 `minix_elf` crate 解析 + `Paging` trait 方法映射，逻辑等价但类型安全。

---

## Ch6. 参见

**前置文档**：
- [03-kmain-cstart](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md) — kmain/cstart 的早期 boot 流程（Phase A/B）
- [05-clock-interrupt-init](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/05-clock-interrupt-init.md) — 时钟与中断初始化（bsp_finish_booting Step 6 依赖）

**后续文档**：
- [07-cross-space-init](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/07-cross-space-init.md) — 跨地址空间初始化（VM 建页表后唤醒其他进程）
- [08-system-init-boot-finish](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/08-system-init-boot-finish.md) — SYSTEM 进程初始化与 boot 收尾
- [10-switch-to-user](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/10-switch-to-user.md) — switch_to_user 首次调度（bsp_finish_booting Step 9）

**相关文档**：
- [00-kernel-overview](file:///home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/03-stage-kernel/00-kernel-overview.md) — 内核整体架构与 BKL/SMP 模型


