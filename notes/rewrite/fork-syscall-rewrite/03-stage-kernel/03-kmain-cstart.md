# 03-kmain-cstart: kmain 入口与 cstart 平台初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/main.c:115-147,403-481`, `minix3/minix/kernel/arch/i386/protect.c:321-367`, `minix3/minix/kernel/arch/earm/protect.c:77-93`
> **说明**: 从 `kmain()` 入口到保护结构就绪——内核建立保护模式基础设施
> **前置**: [02-higher-half-kernel.md](02-higher-half-kernel.md) — CPU 已切换到高地址，进入 `kmain`

---

## 1. 概念：保护结构——内核如何让 CPU 硬件为自己建立可信运行上下文

### 1.0 章节引言

本章建立"保护结构"的概念模型——内核如何让 CPU 硬件为自己建立可信的运行上下文。

> **本章不讲什么**:
> - 页表多级结构、TLB（内存管理文档范围）
> - GDT 段描述符位级格式、IDT Gate 格式（Ch2 C 源码分析）
> - syscall 实现细节、swapgs/MSR_LSTAR（[12-syscall.md](12-syscall.md)）
> - trap frame 布局、IST/sscratch 交换指令细节（[13-exception-interrupt.md](13-exception-interrupt.md)）
>
> 这些在后续章节展开，本章只建概念地基。

### 1.1 什么是"保护"——为什么要保护

**信任边界**：内核可信，用户进程不可信。这是操作系统存在的根本前提之一。

**具体例子**：用户程序执行 `*((int*)0)=0`（写空指针）或 `sys_kill_all_processes()`（试图直接调用内核功能）。为什么不能成功？

- 不是因为"用户"二字有魔法
- 而是因为 CPU 硬件 + OS 配置协同阻止：用户态特权级低 → 该地址在页表里无权限（或该功能需 syscall 入口）→ CPU 触发缺页异常 → 跳到内核 handler → 内核杀掉进程

> **为什么用 `*((int*)0)=0` 而非 `*(0xffffffff80000000)=0`**：避免初学者分心"为什么这个地址是内核地址"。空指针更直观，权限拒绝的核心机制相同。

这个例子预告了三件事：特权级、页表权限位、异常处理——正是后文要展开的三个机制。

**"保护"的定义**：让不可信代码不能破坏可信代码（内核）和其他不可信代码（别的进程）。保护 = 特权级 + 内存权限 + 异常处理 的协同。

### 1.2 硬件特权级——保护的地基

**三架构特权级对照**:

| 架构 | 内核特权级 | 用户特权级 | 当前特权级存放 |
|------|-----------|-----------|---------------|
| x86-64 | ring 0 | ring 3（ring 1/2 不用） | CPL（CS 低 2 位） |
| aarch64 | EL1 | EL0（EL2/3 留给虚拟化/固件） | PSTATE.M |
| riscv64 | S-mode | U-mode（M-mode 留给固件） | CPU 当前状态（trap 来源由 sstatus.SPP 记录） |

**CPU 如何知道当前特权级**：简述 CPL / PSTATE.M / sstatus 的角色——CPU 每条指令执行时都隐含知道自己在哪个特权级，这是硬件强制的，软件无法伪造。

> **RISC-V 严谨表述**：RISC-V 当前运行级别是 CPU 当前状态（S-mode 或 U-mode），**不是**靠 sstatus.SPP 表示。SPP 只在 trap 发生时记录"trap 来自哪个特权级"（0=U-mode, 1=S-mode），供 sret 返回时决定降回哪级。即：SPP 是"来源记录"，非"当前状态"。当前状态由 CPU 内部硬件状态机维护，软件不可直接读取一个"当前 mode"寄存器（需通过 sstatus.SPP 间接推断仅在 trap 入口时有效）。

**特权级切换的两种触发**:

- **被动**：异常（除零/缺页/未定义指令）→ CPU 自动切到内核态
- **主动**：系统调用（syscall/svc/ecall）→ 用户主动请求进入内核

两者都涉及特权级跨越，都需要 CPU 硬件配合。

### 1.3 跨特权级的统一流程

**统一流程图**（syscall/exception 分叉 + 三架构统一抽象 + 返回闭环 + 顺序修正 + 保存现场统一）:

```
用户态运行
    │
    ├──→ syscall / svc / ecall   （用户主动请求服务）
    │
    └──→ exception               （CPU 被动发现错误：除零/缺页/未定义指令）
            │
            ▼
    CPU 进入更高特权级（ring3→ring0 / EL0→EL1 / U→S）
            │
            ▼
    跳到异常入口 + 切换到可信内核栈
    （x86/aarch64：CPU 读向量表时原子完成跳入口+切栈；
     riscv64：跳到 stvec 后软件第一条指令交换 sscratch 切栈）
            │
            ▼
    保存现场（cs/rip/ss/rsp/rflags 等；
            由硬件、软件或两者协作完成，具体机制因架构而异）
            │
            ▼
    内核 handler 执行
            │
            ▼
    返回指令（iretq/eret/sret）：恢复现场 + 切回用户栈 + 降特权级
            │
用户态继续运行
```

**关键点 1（顺序修正）**：x86 读 IDT 门描述符时，CPU **同时**完成切栈（从 TSS.sp0 加载 RSP）+ 跳入口（加载门描述符里的 handler 地址），是原子操作，非"先跳再切"。aarch64 类似（EL0→EL1 时硬件切 SP_EL1 并跳 VBAR_EL1 指向的入口）。riscv64 不同：CPU 只负责跳到 stvec，切栈由 handler 第一条指令 `csrrw sp, sscratch, sp` 软件完成。流程图抽象层把"跳入口+切栈"合并为一步，三架构都成立，§1.4c 再分架构展开硬件 vs 软件。

**关键点 2（保存现场统一）**：三架构"保存现场"也不统一——x86 CPU 硬件自动压入 SS:RSP:RFLAGS:CS:RIP，其余寄存器软件保存；aarch64 大部分寄存器软件保存；riscv64 全部软件保存。流程图用"保存现场"抽象表述，加注"由硬件、软件或两者协作完成，具体机制因架构而异"，细节见 [13-exception-interrupt.md](13-exception-interrupt.md)。

**syscall vs 异常的本质区别**（细节见 [12-syscall.md](12-syscall.md)）:

- 异常 = CPU 被动发现错误，用户进程"不想"进入内核
- syscall = 用户主动、受控请求服务，是合法的特权级跨越
- 两者在"切栈+保存现场+跳入口"的硬件机制上相同，区别在触发意图和返回后用户态是否继续原指令

**返回闭环（保护结构是双向门，非单向门）**:

保护结构不是单向门（只管"用户→内核"），而是双向门（"内核→用户"返回同样重要）。返回时 CPU 必须能：恢复用户现场 + 切回用户栈 + 降特权级——这三件事由返回指令原子完成。

**返回指令对照表**:

| 架构 | 返回指令 | 完成动作 |
|------|---------|---------|
| x86-64 | `iretq` | 弹出 RIP/CS/RFLAGS/RSP/SS，CS 低 2 位决定降回 ring3 |
| aarch64 | `eret` | 恢复 PC/PSTATE，PSTATE.M 降回 EL0 |
| riscv64 | `sret` | 恢复 PC，sstatus.SPP 决定降回 U-mode |

细节（swapgs、sscratch 二次交换、用户态恢复后的指令位置）留给 [12-syscall.md](12-syscall.md)/[13-exception-interrupt.md](13-exception-interrupt.md)。

**为什么这个流程需要 CPU 预先配置**：切栈要知道切到哪个栈，跳入口要知道跳到哪个地址——这些答案 CPU 不会自己生成，必须 OS 预先填好。

### 1.4 CPU 视角三问

> 从 CPU 的角度看，保护结构就是回答三个运行时问题：
> 1. 当前什么特权级？
> 2. 异常/syscall 发生时跳到哪里？
> 3. 陷入内核时用哪个栈？
>
> `prot_init()` 的全部工作，就是给 CPU 配置回答这三个问题的数据结构。后文三问逐一展开，每问对应 prot_init 的一类配置。
>
> **注**：第一问"特权级"是核心，它进一步决定可访问的内存范围（见 §1.4a）——但内存访问控制是特权级的"后果"，不是与特权级并列的独立问题。特权级是地基，页表权限位是落地手段。

#### 1.4a 第一问：当前什么特权级？（+ 特权级如何落地为内存访问控制）

**特权级如何落地为内存访问控制**:

特权级本身只是个"标签"（§1.2 讲过），不能直接阻止访问。真正阻止访问的是**内存管理数据结构里的权限位**——而哪些权限位生效，由当前特权级决定：

- x86 页表 PTE 的 U/S 位（0=Supervisor only, 1=User 可访问）；段描述符 DPL
- aarch64 页表 PTE 的 AP 位、UXN/PXN
- riscv64 页表 PTE 的 U 位 + R/W/X

**耦合关系（层次清晰）**：特权级决定"能不能"，页表权限位"执行能不能"。两者缺一不可。**特权级是核心，页表权限位是落地手段（后果），非并列**。回到 §1.1 的例子：`*((int*)0)=0` 失败，正是因为 ring3 + PTE.U=0 协同阻止——ring3 是"为什么不能"，PTE.U=0 是"如何阻止"。

**边界声明**：保护结构不"做"内存管理，但依赖内存管理的数据结构落地权限。页表组织（多级/TLB）是内存管理文档的事，本文只讲权限位这一耦合点。

**特权级 → 页表权限位 → 异常处理 三层关系**:

```
特权级（为什么不能）
    │
    ▼
CPU 发起内存访问
    │
    ▼
检查页表权限位（如何阻止）
    │
    ├── 允许 → 访问成功
    └── 拒绝 → 触发异常（阻止后的动作）
                    │
                    ▼
                跳到内核 handler（见 §1.4b）
```

这张图建立三层层次：特权级（为什么不能）→ 页表权限位（如何阻止）→ 异常处理（阻止后动作）。

**prot_init 的职责（三架构不对等）**:

- **x86**：prot_init 配置 GDT 段描述符 DPL（初始代码段/数据段描述符），是 x86 特权级检查的依据（`protect.c:345-348`）。
- **aarch64/riscv64**：prot_init 在第一问**几乎不配置**——特权级是 CPU 固定的（EL0/EL1、U/S 是硬件定义），页表权限位由 arch_boot_impl 建立（02 文档），不归 prot_init。
- **不对等的来源**：§1.4 引子说"每问对应 prot_init 一类配置"，但第一问只在 x86 有 prot_init 配置（GDT DPL），aarch64/riscv64 在第一问 prot_init 几乎不干活。这是 x86 历史包袱（段描述符 DPL）导致的，非三架构共性。第一问的"答案"在 aarch64/riscv64 由 arch_boot_impl（页表权限位）+ CPU 硬件（固定特权级）共同提供，prot_init 只在 x86 补充段描述符 DPL。

#### 1.4b 第二问：异常/syscall 发生时跳到哪里？（异常入口）

**WHY**：CPU 发现异常或收到 syscall 时，自己不知道该执行哪个函数——必须有一张表告诉它"向量号 X → handler 地址 Y"。无有效入口 → triple fault（x86）/ 异常嵌套失控。

**HOW（三架构）**: x86 IDT / aarch64 VBAR_EL1 / riscv64 stvec。一句话流程：CPU 发现异常 → 查向量表 → 找到 handler → 跳转。

**嵌套异常用哪个栈**（细节见 [13-exception-interrupt.md](13-exception-interrupt.md)）:

- x86-64：同特权级不切栈（用当前 RSP）；NMI/double-fault/MCE 用 IST（TSS 里 7 个专用栈）避免依赖可能已损坏的内核栈。
- aarch64：EL1 再陷异常仍用 SP_EL1，需小心保存/恢复。
- riscv64：sscratch 在内核态时存的是用户 sp，必须检测 sstatus.SPP 判断"来自用户还是内核"再决定是否交换——RISC-V trap 处理已知复杂点。

**prot_init 的职责**：填充异常向量表元数据（IDT 门描述符的 DPL/IST/门类型；VBAR/stvec 指向汇编定义的向量表）。注意：handler 地址在本文阶段仍为 0，真正加载推迟到 [13-exception-interrupt.md](13-exception-interrupt.md) `set_handler()` 之后。

#### 1.4c 第三问：陷入内核时用哪个栈？（跨特权级栈切换）

**WHY**：若 user→kernel 继续用用户栈保存上下文（cs/rip/ss/rsp/rflags），用户能把栈顶设在只读页或构造畸形栈帧 → 篡改陷入上下文或内核栈溢出。必须切到可信内核栈。CPU 进入内核第一件事不是执行 handler，而是"找个安全地方保存现场"——这就是栈。详细安全论证见 [附录 A：为什么必须切到内核栈](#附录-a为什么必须切到内核栈)。

**HOW（三架构，核心知识点）**:

| 架构 | 栈切换方式 | 关键寄存器/结构 |
|------|-----------|----------------|
| x86-64 | CPU 硬件自动：ring3→ring0 从 TSS.sp0 加载 RSP，压入 SS:RSP:RFLAGS:CS:RIP | TSS.sp0 |
| aarch64 | CPU 硬件自动：EL0→EL1 切到 SP_EL1 | SP_EL1 |
| riscv64 | **硬件不切**：软件 `csrrw sp, sscratch, sp` 交换；sscratch 存内核栈顶 | sscratch |

**RISC-V 软件切栈特色**：三架构里唯一软件切栈的，trap handler 第一条指令就是交换。sscratch 在用户态时存内核栈顶，在内核态时存用户栈顶——一个寄存器两种用途，靠 sstatus.SPP 区分。

**prot_init 的职责**：配置内核栈顶到 TSS.sp0（x86，`protect.c:154` 的 `tss_init()`）/ SP_EL1（aarch64）/ sscratch（riscv64）。

#### 1.4 总结表

| CPU 运行时问题 | 缺失回答的后果 | 保护结构提供的答案 |
|---------------|--------------|------------------|
| **1. 当前什么特权级？** | 特权级无隔离 → 用户进程可读/写内核数据、可执行内核代码 → **权限隔离完全失效** | 特权级寄存器（CPL/PSTATE.M/sstatus）+ x86 GDT 段描述符 DPL |
| **2. 异常/syscall 发生时跳到哪里？** | 除零/缺页/未定义指令触发时 CPU 无有效入口，通常在 x86-64 上表现为 **triple fault**，系统直接重启 | 异常向量表（x86-64 IDT / aarch64 VBAR_EL1 / riscv64 stvec） |
| **3. 陷入内核时用哪个栈？** | CPU 继续用用户态栈保存 `cs/rip/ss/rsp` 等上下文；恶意进程可把栈顶设在只读页或构造畸形栈帧 → **上下文被篡改或内核栈溢出** | 内核栈指针（x86-64 TSS.sp0 / aarch64 SP_EL1 / riscv64 sscratch） |

> **注**：第一问的"后果"——内存访问控制——作为特权级的落地手段展开（见 §1.4a），不作为独立问题。特权级是核心，页表权限位是落地手段。

### 1.5 三架构保护结构对照表

读者看完 §1.4 三问后，本表把三架构的答案并列，让"三架构其实是一回事"一目了然。这是 §1.4 三问的横向收束。

| CPU 运行时问题 | x86-64 | aarch64 | riscv64 |
|---------------|--------|---------|---------|
| 当前特权级 | ring 0/3 | EL0/EL1 | U/S-mode |
| 特权级落地为内存权限（后果） | 页表 U/S + 段 DPL | 页表 AP/UXN/PXN | 页表 U + R/W/X |
| 异常入口 | IDT | VBAR_EL1 | stvec |
| 内核栈切换 | TSS.sp0 | SP_EL1 | sscratch |
| 嵌套异常用哪个栈 | IST（专用栈） | SP_EL1（不切，同栈） | 当前 sp（不切，同栈） |
| 返回用户态 | `iretq` | `eret` | `sret` |

> **关于"嵌套异常用哪个栈"行**：三架构统一为"用哪个栈"抽象层——x86 用 IST 专用栈，aarch64/riscv64 不切栈用当前栈。"如何判断是否切栈"的细节（如 riscv64 检测 `sstatus.SPP`）见 §1.4b 文字和 [13-exception-interrupt.md](13-exception-interrupt.md)。

**统一性结论**：尽管三架构实现载体不同（GDT/IDT/TSS vs VBAR/SP_EL1 vs stvec/sscratch），但都在回答同样的 CPU 三问——保护结构本质是跨架构共性的。差异只在"用什么数据结构承载答案"，不在"要回答哪些问题"。

### 1.6 为什么不能用固件留下的保护结构

进入 `kmain()` 时，CPU 确实有正在生效的保护结构（UEFI/BIOS/ bootloader 留下的），但内核不能直接用，必须 `prot_init()` 重建：

- **生命周期不可控**：UEFI 的 GDT/IDT 位于 UEFI 运行时内存。`ExitBootServices()` 后，这部分内存可被内核作为空闲物理页回收，下一个异常就找不到处理函数。
- **语义不匹配**：UEFI 的 GDT 为运行 PE32+ 程序设计，只含代码段和数据段选择子。Minix3 内核需要 TSS（任务状态段）实现用户态→内核态栈切换，UEFI 的 GDT 中没有 TSS 描述符。
- **架构差异**：x86 用 GDT + IDT + TSS 三层结构；aarch64/riscv64 用单层异常向量。固件留下的保护结构无法满足内核的特权级切换需求。

`prot_init()` 的语义：内核在此刻之前一直借用他人的保护结构（boot-shim 用 UEFI 的，GRUB 用 BIOS 的）。`prot_init()` 之后，内核拥有自己的保护结构。保护结构的所有权从固件转移到内核，是内核从"被引导的程序"转变为"操作系统的核心"的标志。

### 1.7 x86 为什么还保留 GDT

> 读者可能问：aarch64/riscv64 没有 GDT 照样实现特权级和栈切换，为什么 x86 还有？本节回答这个疑问。

**GDT 在 x86-64 承载的两个功能角色**:

1. **段描述符 DPL**：CPU 在控制转移（far call/jump、中断）时检查 DPL，是 x86 特权级检查的依据。对应 §1.4a 第一问。
2. **TSS 描述符**：TSS 存 `sp0`（ring3→ring0 内核栈指针）和 IST（嵌套异常专用栈）。对应 §1.4c 第三问。

> **核心知识 vs 实现细节**：**TSS 是核心知识**（§1.4c 第三问的答案），**GDT descriptor bit 格式是实现细节**（参见 §2 C 源码分析）。

**为什么 x86 还保留 GDT（回答标题疑问）**:

- 64-bit 下段式内存隔离已废弃（flat segment，base=0, limit=max），但 x86 架构规定 **TSS 必须通过 GDT 描述符引用**——所以 GDT 不能完全去掉。
- 即：GDT 存在是因为 x86 架构要求 TSS 描述符必须挂在 GDT 里，**不是 OS 概念需要 GDT**。
- aarch64/riscv64 用 CSR/系统寄存器直接存内核栈指针（SP_EL1/sscratch），不需要 GDT 这种间接表。

**历史遗留**：64-bit 用 flat segment（base=0, limit=max），段式内存隔离已废弃，内存保护交给页表。Minix3 代码涉及 GDT 是因为 x86 架构要求，不是 OS 概念需要。

### 1.8 prot_init 在六阶段启动中的位置

02 文档结束时，CPU 已在高地址执行 `kmain()`。从 `kmain()` 入口到内核开始调度第一个用户进程，过程分为六个阶段：

| 阶段 | 关键动作 | 本质 |
|------|---------|------|
| **A: 入口** | 校验 kinfo、打开"内核可分配内存"闸门 | 准备运行期数据 |
| **B: cstart** | 建立保护结构、初始化时钟、初始化中断控制器、架构相关初始化 | 从"裸机"过渡到"有保护的运行环境" |
| **C: 进程表** | 创建进程表项、加载 boot modules 的 ELF | 准备好被调度实体 |
| **D: post-init** | 启动 VM 进程、分配空闲页目录 | 内存管理上线 |
| **E: system** | 初始化特权表（对应 [07-system-init-boot-finish.md](07-system-init-boot-finish.md) 的 `system_init()`） | 权限系统上线 |
| **F: finish** | 回收 bootstrap 内存、切换到用户态 | 启动完成 |

本文覆盖 **A 与 B 的前半部分（保护结构）**；B 后半部分（时钟、中断、arch_init）在 04 文档展开。

```
┌─────────────┐     ┌─────────┐     ┌─────────────────────────────┐
│  boot-shim  │────→│  kmain  │────→│ init_protection()           │  ← 本文范围
│  (02 文档)  │     │ (A/B-1) │     │   (= C 版 prot_init())      │
└─────────────┘     └─────────┘     │   填三问答案：              │
                                    │     特权级 + 内核栈 + 异常入口 │
                                    │   (异常入口加载推迟到         │
                                    │    set_handler 之后)         │
                                    └─────────────────────────────┘
                                              │
                                              ▼
                                    ┌─────────────────────────────┐
                                    │ init_clock_and_interrupts() │  ← 04 文档范围
                                    │   init_clock()              │
                                    │   intr_init()               │
                                    │   arch_init()               │
                                    └─────────────────────────────┘
```

> **呼应 §1.4 CPU 视角**：`prot_init()` 在六阶段的 **B 阶段**执行，就是在这个阶段给 CPU 填三个答案（见 §1.4）——当前特权级依据（x86 GDT DPL）、异常入口（IDT/VBAR/stvec）、内核栈（TSS.sp0/SP_EL1/sscratch）。

> **为什么 cstart 必须先于后续所有阶段**：在 cstart 之前，CPU 处于不可信状态——x86-64 没有自己的 GDT/IDT，使用 UEFI/GRUB 留下的描述符表；aarch64/riscv64 的异常向量表基址寄存器未设置，任何异常都会 triple fault。一旦进入 `proc_init()`，就要开始设置进程的段寄存器（x86-64 的 CS/DS/SS）和栈指针，这些操作依赖 GDT 中的段描述符。若 GDT 未就绪，进程切换时会 GP fault。
>
> **架构差异注记**：上述"段寄存器依赖 GDT 描述符"的约束是 x86 特有的——aarch64/riscv64 没有 GDT/段描述符概念，进程切换不涉及段寄存器加载。但三架构都要求 `cstart` 先于 `proc_init`：aarch64/riscv64 的 `proc_init` 会设置进程的异常向量上下文和栈指针，若 `prot_init` 未先就绪，进程切换后首个异常会因向量表基址未设置而失控。即：x86 的强制原因是 GDT，aarch64/riscv64 的强制原因是异常向量+内核栈——三架构都强制，但强制点不同。

### 1.9 prot_init 与 arch_boot_impl 的边界

`prot_init()` 不重建页表，此工作在 02 文档的 `arch_boot_impl()` 中已完成。`prot_init()` 与 `arch_boot_impl()` 的职责划分：

- **`arch_boot_impl()`**：建立页表（含权限位）+ 启用分页 + 切栈跳转到高地址的 `kmain`
- **`prot_init()`**：建立保护结构（异常向量、特权级、内核栈），使 CPU 能正确响应异常

> **保护结构与内存管理的关系**：两者非正交，而是**依赖关系**——保护结构依赖页表权限位作为落地内存访问控制的手段：
> - 分页（页表权限位）是保护结构落地内存访问控制的**手段**（见 §1.4a）。
> - `arch_boot_impl` 建立页表（含权限位）；`prot_init` 建立异常向量+特权级+内核栈。页表权限位是保护结构在内存访问控制维度的执行载体。
>
> 即：保护结构不"做"内存管理，但**依赖**内存管理的数据结构落地权限。

### 1.10 本章小结

本章从 CPU 运行时视角提出三个问题，建立了"保护结构"的概念模型。

**核心脉络回顾**：

- **三问框架**：当前什么特权级？异常/syscall 跳哪？用哪个栈？`prot_init()` 的全部工作就是给 CPU 配置这三个答案的数据结构。
- **三架构统一性**：尽管实现细节不同（TSS/SP_EL1/sscratch，IDT/VBAR/stvec），但三个架构回答 CPU 三问的逻辑完全一致——保护结构本质是跨架构共性的。
- **边界回顾**：
  - 保护结构**依赖**页表权限位落地内存访问控制（非正交，见 §1.9）。
  - 不能用固件留下的保护结构（生命周期/语义/架构差异，见 §1.6）。
  - GDT 是 x86 历史包袱（TSS 必须挂 GDT，见 §1.7）。

三问框架贯穿后续所有章节——Ch2 分析 Minix3 C 代码如何实现三问的答案，Ch3/Ch4 用 Rust 类型系统重新表达，Ch5 用测试验证答案是否正确配置。

**一句话总结**：保护结构的本质，就是让 CPU 硬件成为内核的"安全门卫"——只允许合法的特权级切换，只跳转到内核信任的入口，只使用内核控制的栈。

---

## 2. C 源码分析

### 2.1 kmain 入口：memcpy + BSS 检查 + kernel_may_alloc

`main.c:115-147`：

```c
void kmain(kinfo_t *local_cbi)
{
  struct boot_image *ip;
  register struct proc *rp;
  register int i, j;
  static int bss_test;

  /* bss sanity check */
  assert(bss_test == 0);   // (1) BSS 段验证
  bss_test = 1;

  /* save a global copy of the boot parameters */
  memcpy(&kinfo, local_cbi, sizeof(kinfo));   // (2) 拷贝启动信息
  memcpy(&kmess, kinfo.kmess, sizeof(kmess));

  machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));
#ifdef __arm__
  arch_ser_init();
#endif
  DEBUGBASIC(("MINIX booting\n"));

  kernel_may_alloc = 1;   // (3) 允许内核分配内存

  assert(sizeof(kinfo.boot_procs) == sizeof(image));
  memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs));

  cstart();   // (4) 进入 cstart
```

**逐行分析**——理解 C 版每个操作的**目的**：

1. **BSS 检查 `assert(bss_test == 0)`**（`main.c:122-124`）：C 语言不保证 BSS 段自动清零，这是 bootloader 的责任。若 bootloader 有 bug 或链接脚本 BSS 范围计算错误，未清零的 BSS 会让内核假设为 0 的全局变量包含垃圾值。这个 assert 在启动早期捕获 bootloader 错误，避免垃圾值在后续代码中引发难以调试的故障。

2. **memcpy(&kinfo, local_cbi, sizeof(kinfo))**（`main.c:128`）：`local_cbi` 是 `kmain` 的参数，只在 `kmain` 调用链内可见。将其拷贝到全局变量 `kinfo` 后，内核所有代码（中断 handler、其他子系统、非 `kmain` 调用链的函数）都能访问启动信息。

3. **kernel_may_alloc = 1**（`main.c:142`）：VM 启动前内核没有专门的内存分配服务。这个全局标志告诉内核代码"现在可以安全调用物理内存分配器了"。标志为 0 时（VM 启动前），任何内存分配请求都应被拒绝或 panic——分配器可能尚未初始化。本质是**启动阶段的权限闸门**。

4. **cstart()**（`main.c:147`）：进入保护模式初始化及后续启动流程。cstart 内部依次调用 prot_init、init_clock、intr_init、arch_init，并解析环境变量。这是内核从"被引导的程序"转变为"操作系统"的关键转折点，从此刻起，内核有了自己的保护结构、时钟、中断控制。

### 2.2 cstart() 调用序列

`main.c:403-481`：

```c
void cstart(void)
{
  register char *value;

  /* low-level initialization */
  prot_init();          // (1) 保护模式初始化

  /* determine verbosity */
  if ((value = env_get(VERBOSEBOOTVARNAME)))
      verboseboot = atoi(value);

  /* Initialize clock variables. */
  init_clock();         // (2) 时钟初始化

  /* ... 中间是环境变量解析 ... */

  intr_init(0);         // (3) 中断初始化

  arch_init();          // (4) 架构特定初始化
}
```

cstart 的四个调用严格有序：

1. **prot_init()**（`main.c:411`）：建立 GDT/IDT/TSS（x86-64）或异常向量（aarch64/riscv64）。必须在所有其他初始化之前完成——因为没有保护结构，任何异常都会 triple fault。
2. **init_clock()**（`main.c:418`）：初始化时钟源。依赖 prot_init() 建立的中断描述符——时钟中断需要 IDT 中的门描述符。
3. **intr_init(0)**（`main.c:472`）：初始化中断控制器（8259A/APIC）。参数 0 表示"boot 阶段"。依赖 prot_init() 和 init_clock()。
4. **arch_init()**（`main.c:474`）：架构特定的额外初始化。依赖前三步完成。

### 2.3 prot_init() 详解：x86

`protect.c:321-364`（i386 版，64 位语义相同但使用 64 位描述符格式，GDTR.base 为 64 位）：

```c
void prot_init(void)
{
  extern char k_boot_stktop;

  if(_cpufeature(_CPUF_I386_SYSENTER))
    minix_feature_flags |= MKF_I386_INTEL_SYSENTER;
  if(_cpufeature(_CPUF_I386_SYSCALL))
    minix_feature_flags |= MKF_I386_AMD_SYSCALL;

  memset(gdt, 0, sizeof(gdt));   // (1) 清零 GDT
  memset(idt, 0, sizeof(idt));   // (2) 清零 IDT

  /* Build GDT, IDT, IDT descriptors. */
  gdt_desc.base = (u32_t) gdt;           // (3) 设置 GDTR
  gdt_desc.limit = sizeof(gdt)-1;
  idt_desc.base = (u32_t) idt;           // (4) 设置 IDTR
  idt_desc.limit = sizeof(idt)-1;
  tss_init(0, &k_boot_stktop);           // (5) 初始化 TSS

  /* Build GDT */
  init_param_dataseg(&gdt[LDT_INDEX],    // (6) LDT（32 位遗留，64 位不用）
    (phys_bytes) 0, 0, INTR_PRIVILEGE);
  gdt[LDT_INDEX].access = PRESENT | LDT;
  init_codeseg(KERN_CS_INDEX, INTR_PRIVILEGE);   // (7) 内核代码段
  init_dataseg(KERN_DS_INDEX, INTR_PRIVILEGE);    // (8) 内核数据段
  init_codeseg(USER_CS_INDEX, USER_PRIVILEGE);    // (9) 用户代码段
  init_dataseg(USER_DS_INDEX, USER_PRIVILEGE);    // (10) 用户数据段

  prot_load_selectors();   // (11) lgdt + idt_init + idt_reload + lldt + ltr + 重载段寄存器

  /* Rebuild page tables */
  pg_clear();              // (12) 清零页表
  pg_identity(&kinfo);     // (13) 恒等映射
  pg_mapkernel();          // (14) 内核高地址映射
  pg_load();               // (15) 加载 CR3

  prot_init_done = 1;      // (16) 标记完成
}
```

**关键步骤分析**：

- **步骤 1-5**（`protect.c:328-336`）：清零并设置描述符表指针。这是"准备阶段"。
- **步骤 6-10**（`protect.c:339-345`）：填充 GDT 段描述符。64 位模式下，代码段和数据段都是 flat（base=0, limit=full），但 DPL（Descriptor Privilege Level）不同：内核段 DPL=0（`INTR_PRIVILEGE`），用户段 DPL=3（`USER_PRIVILEGE`）。这一步对应 §1.4a 第一问——x86 特权级检查的依据。
- **步骤 11**（`protect.c:350`）：`prot_load_selectors()` 执行 `lgdt`（加载 GDTR）、`idt_init()`（填充 IDT 门描述符）、`idt_reload()`（加载 IDTR，即 `lidt`）、`lldt`（加载 LDTR）、`ltr`（加载 TR）、重载所有段寄存器（CS/DS/ES/FS/GS/SS）。这是"生效阶段"——从此 CPU 使用我们自己的 GDT 和 IDT。
- **步骤 12-15**（`protect.c:357-360`）：建立（重建）bootstrap 页表。`pre_init()`（`pre_init.c:217`）已经做过几乎相同的页表设置，并把 `pg_mapkernel()` 的返回值存入了 `kinfo.freepde_start`（`pre_init.c:232`）。`prot_init()` 之所以再次 `pg_clear()` + `pg_identity()` + `pg_mapkernel()`，不是因为内核映射参数变了——`kern_vir_start` / `kern_phys_start` / `kern_kernlen` 是 `pg_utils.c:14-16` 的静态变量，内容不变——而是因为：
  1. `prot_init()` 作为保护子系统的统一初始化入口，选择从零重建页表（连同 GDT/IDT/TSS 一起），确保保护结构处于已知状态；
  2. 重建后的页表是**内核重定位完成后**建立的官方 bootstrap 页表，后续 `arch_boot_proc()` 会把 VM 进程直接加载到这个页表里运行（`protect.c:480+`）；
  3. 此时使用的 `kinfo` 已经经过 `kmain()` 补充（`nr_procs`、`nr_tasks`、`boot_procs` 等，`main.c:128-431`），不再只依赖 boot loader 的原始 multiboot 数据。
  
  具体调用：
  - `pg_clear()` 清空页目录；
  - `pg_identity(&kinfo)` 建立 1:1 映射（供 LAPIC、显存等需要物理地址的设备）；
  - `pg_mapkernel()` 把内核映射到高地址；
  - `pg_load()` 激活新页表。
- **步骤 16**（`protect.c:364`）：标记 `prot_init_done = 1`，表示 bootstrap 页表和保护结构已就绪，可以安全启动 VM 和后续服务，AP 初始化也可以依赖这套结构。

#### 2.3.1 tss_init() 分析

`protect.c:154-167`：

```c
int tss_init(unsigned cpu, void * kernel_stack)
{
	struct tss_s * t = &tss[cpu];
	int index = TSS_INDEX(cpu);
	struct segdesc_s *tssgdt;

	tssgdt = &gdt[index];
	init_param_dataseg(tssgdt, (phys_bytes) t,
			sizeof(struct tss_s), INTR_PRIVILEGE);
	tssgdt->access = PRESENT | (INTR_PRIVILEGE << DPL_SHIFT) | TSS_TYPE;

	/* Build TSS. */
	memset(t, 0, sizeof(*t));
	t->ds = t->es = t->fs = t->gs = t->ss0 = KERN_DS_SELECTOR;
	...
}
```

`tss_init()` 做两件事（对应 §1.4c 第三问）：

1. **在 GDT 中创建 TSS 描述符**（`protect.c:160-163`）：TSS 必须通过 GDT 描述符引用（这是 §1.7 讲的"x86 架构要求"）。
2. **填充 TSS 内容**（`protect.c:166-167`）：设置 `ss0`（ring0 栈段选择子）和 `sp0`（ring0 栈指针，在 `earm/protect.c:32` 可见类似逻辑 `t->sp0 = ((unsigned) kernel_stack) - ARM_STACK_TOP_RESERVED`）。当 ring3→ring0 切换时，CPU 硬件自动从 TSS.sp0 加载 RSP。

#### 2.3.2 idt_init() 分析

`idt_init()`（`protect.c:260-263`）负责填充 IDT（Interrupt Descriptor Table），是异常向量表的核心初始化逻辑（对应 §1.4b 第二问）：

```c
void idt_init(void)
{
	idt_copy_vectors_pic();                              // (a) 填充 PIC 中断向量
	idt_copy_vectors(gate_table_exceptions);             // (b) 填充 CPU 异常向量
}
```

**gate_table 数据结构**（`arch_proto.h:217-221`）：

```c
struct gate_table_s {
  void(*gate) (void);      // 处理函数地址
  unsigned char vec_nr;    // IDT 向量号
  unsigned char privilege; // 描述符特权级（DPL）
};
```

> **注**：C 版 `gate_table_s` 只有 3 个字段（gate/vec_nr/privilege），**没有 IST 字段**。IST（Interrupt Stack Table）是 x86-64 长模式的扩展，32-bit i386 代码不使用。64-bit 重写时 IST 作为新增配置项引入（见 Ch3/Ch4）。

**gate_table_pic[]**（`protect.c:107-125`）：PIC 中断向量，通过 `VECTOR(irq)` 宏映射为 `0x50-0x57`（IRQ0-7）和 `0x70-0x77`（IRQ8-15），DPL=0（`INTR_PRIVILEGE`）。

**gate_table_exceptions[]**（`protect.c:127-152`）：CPU 异常向量，如除零（`DIVIDE_VECTOR`, DPL=0）、断点（`BREAKPOINT_VECTOR`, DPL=3）、缺页（`PAGE_FAULT_VECTOR`, DPL=0）等。DPL=3 的向量（断点 `breakpoint_exception`、溢出 `overflow`、IPC/kernel_call 软中断）允许用户态通过 `int $n` 指令主动触发；DPL=0 的向量只能由 CPU 异常或内核触发。

**idt_copy_vectors()**（`protect.c:245-253`）遍历 gate_table 数组，对每个条目调用 `int_gate()` 填充 IDT 门描述符：

```c
void idt_copy_vectors(struct gate_table_s * first)
{
	struct gate_table_s *gtp;
	for (gtp = first; gtp->gate; gtp++) {
		int_gate(idt, gtp->vec_nr, (vir_bytes) gtp->gate,
				PRESENT | INT_GATE_TYPE |
				(gtp->privilege << DPL_SHIFT));
	}
}
```

`int_gate()`（`protect.c:227-237`）把 handler 地址拆分到 IDT 门描述符的 offset_low/offset_high 字段，设置 selector=KERN_CS_SELECTOR，设置 p_dpl_type 字段（Present + DPL + 门类型）。

> **handler 地址当前已填入**：C 版 `idt_init()` 在 `prot_init()` 阶段就把真实 handler 地址填入 IDT 并 `lidt` 加载。这与 Rust 版的"先填元数据、handler 地址推迟到 `set_handler()` 后"不同（见 Ch3/Ch4）。

### 2.4 prot_init() 详解：aarch64

> **注意**：Minix3 的 ARM 版本（earm）是 32 位，代码比 64 位简化。以下分析基于 32-bit ARM C 代码（`earm/protect.c`）。

`earm/protect.c:77-93`（远短于 x86）：

```c
void prot_init(void)
{
  /* tell the HW where we stored our vector table */
  write_vbar((reg_t)&exc_vector_table);   // (1) 设置异常向量表基址

  /* Rebuild page tables */
  pg_clear();              // (2) 清零页表
  pg_identity(&kinfo);     // (3) 恒等映射
  pg_mapkernel();          // (4) 内核高地址映射
  pg_load();               // (5) 加载 TTBR1

  prot_init_done = 1;      // (6) 标记完成
}
```

ARM 的 `prot_init()` 比 x86 简单得多——因为 ARM 没有段描述符/GDT 机制。ARM 的特权级切换由硬件自动处理（异常发生时自动切换 EL0→EL1，32-bit ARM 是 USR→SVC），不需要软件设置描述符表。

硬件操作只有两个（对应 §1.4 三问）：

1. **设置 VBAR**（`earm/protect.c:80`）：`write_vbar(&exc_vector_table)`——指向异常向量表基址（对应 §1.4b 第二问）。异常向量表定义了不同类型异常（SVC/IRQ/FIQ/Reset/Undefined）的入口地址。
2. **设置 SP_EL1 / sp0**（`earm/protect.c:33-47` 的 `tss_init()`）：ARM C 代码中 `tss_init()` 设置 `t->sp0 = kernel_stack - ARM_STACK_TOP_RESERVED`（`earm/protect.c:42`，对应 §1.4c 第三问）。当异常从用户态进入内核态时，CPU 自动切换到内核栈。

> **第一问（特权级）在 ARM 上 prot_init 几乎不配置**：ARM 特权级（EL0/EL1 或 USR/SVC）是 CPU 固定的，由异常触发自动切换，不需要软件配置描述符。页表权限位由 `pg_*` 系列函数建立（步骤 2-5），不归 `prot_init` 的保护结构职责。这印证了 §1.4a 讲的"三架构不对等"——第一问在 aarch64/riscv64 由 arch_boot_impl（页表权限位）+ CPU 硬件（固定特权级）共同提供。

### 2.5 prot_init() 详解：riscv64

Minix3 没有 RISC-V 版本，但 RISC-V 的 `prot_init()` 语义可以从架构规范推导（对应 §1.4 三问）：

1. **设置 stvec**（对应 §1.4b 第二问）：`csrw stvec, trap_vector_base`——指向 trap 向量基址。
2. **设置 sscratch**（对应 §1.4c 第三问）：`csrw sscratch, kernel_stack_top`——保存内核栈顶。U-mode→S-mode 时 trap handler 用 `csrrw sp, sscratch, sp` 把 `sp` 换成内核栈顶，同时把原用户栈指针存入 `sscratch`；`sret` 返回前再交换回来。
3. **设置 sstatus**：确保 SPP（Supervisor Previous Privilege）位正确——但 SPP 只在 trap 时被硬件记录，不需要 prot_init 预配置。
4. **重建页表**：同 x86/ARM。

RISC-V 的特权级切换比 x86 简单：trap 发生时，硬件自动将 PC 保存到 `sepc`，将特权级保存到 `sstatus.SPP`，然后跳转到 `stvec` 指向的地址。不需要 GDT/IDT。

> **第一问（特权级）在 RISC-V 上 prot_init 几乎不配置**：RISC-V 特权级（S/U-mode）是 CPU 固定的，由 trap 触发自动切换。页表权限位（U 位 + R/W/X）由 arch_boot_impl 建立。同 aarch64，第一问的"答案"由 arch_boot_impl + CPU 硬件共同提供。

**参考规范**:
- RISC-V *Privileged Architecture Manual* §4.1.5 (stvec) — 决定 trap 入口地址
- RISC-V *Privileged Architecture Manual* §4.1.6 (sscratch) — U-mode → S-mode 切换时临时寄存器
- RISC-V *Privileged Architecture Manual* §4.1.7 (sepc) — trap 时保存 PC
- RISC-V *Privileged Architecture Manual* §4.1.2 (sstatus) — SPP/SUM/MXR 等控制位

### 2.6 设计要点：C 版 prot_init 的三架构共性

尽管三架构 C 代码差异巨大（x86 40+ 行，ARM 10+ 行，RISC-V 推导），但都在回答 §1.4 的 CPU 三问：

| CPU 三问 | x86 C 代码 | aarch64 C 代码 | riscv64（推导） |
|---------|-----------|---------------|---------------|
| 1. 特权级 | GDT 段描述符 DPL（`protect.c:342-345`） | CPU 固定，prot_init 不配置 | CPU 固定，prot_init 不配置 |
| 2. 异常入口 | IDT 填充（`protect.c:260-263` `idt_init()`） | VBAR 设置（`earm/protect.c:80`） | stvec 设置 |
| 3. 内核栈 | TSS.sp0（`protect.c:154` `tss_init()`） | sp0（`earm/protect.c:42` `tss_init()`） | sscratch 设置 |

**共性结论**：C 版 `prot_init()` 的核心职责跨架构一致——给 CPU 填三个答案。差异只在"用什么数据结构承载答案"（x86: GDT/IDT/TSS；aarch64: VBAR+sp0；riscv64: stvec+sscratch）。这为 Ch3 的 trait 抽象提供了依据。

---

## 3. Rust 设计：保护结构的类型系统建模

Ch1 §1.4 用 CPU 视角三问统一了保护结构的概念——当前特权级？异常跳哪？用哪个栈？Ch2 §2.3/§2.6 分析了 C 版 `prot_init()` 内部的两个独立职责：`tss_init()`（`protect.c:154`，保护结构：特权级+内核栈）和 `idt_init()`（`protect.c:260`，异常向量），跨架构都在回答这三问。本章用 Rust 类型系统把三问的答案建模为两个 trait，让概念→抽象→实现三层闭环。

### 3.1 核心抽象：ProtectionArch + TrapEntryArch

Rust 版把三问的答案抽象为两个 trait：

**CPU 问题→Trait 映射表**（Ch1→Ch3→Ch4 三层闭环）:

| CPU 运行时问题（Ch1 §1.4） | Rust Trait | 架构实现（Ch4） |
|---------------------------|-----------|----------------|
| 1. 当前什么特权级？ | `ProtectionArch` | x86: GDT 段描述符 DPL；aarch64/riscv64: CPU 固定 |
| 2. 异常/syscall 跳哪？ | `TrapEntryArch` | x86: IDT；aarch64: VBAR_EL1；riscv64: stvec |
| 3. 陷入内核用哪个栈？ | `ProtectionArch` | x86: TSS.sp0；aarch64: SP_EL1；riscv64: sscratch |

`ProtectionArch` 承担第一问+第三问（特权级+内核栈，都是"保护上下文"职责，对应 C 版 `tss_init()` + GDT 段描述符填充），`TrapEntryArch` 承担第二问（异常入口，是"trap 路由"职责，对应 C 版 `idt_init()`）。这个拆分对应 Ch2 §2.3 分析的 C 版 `prot_init()` 内部两个独立职责。

**为什么不用单一 trait + cfg**:

| 维度 | 单一 trait + cfg | 两个 trait |
|------|----------------|-----------|
| 加载顺序 | 文档中说明"先 load prot 后 load trap" | 类型层面强制，`init_protection` 函数显式调用两者 |
| 实现数量 | trait body 内堆 cfg 分支 | 每个 trait 的实现都是单一职责 |
| 单元测试 | 测整个 trait 较复杂 | 单独测 `ProtectionArch::load()` 和 `TrapEntryArch::load()` |
| 跨架构共性 | 共性被 cfg 淹没 | `ProtectionArch` 的接口对所有架构表达"内核栈 + 特权级"，跨架构一致性更清晰 |

### 3.2 类型系统替代运行时标志

C 版 `prot_init()` 最后（`protect.c:364`）设置 `prot_init_done = 1`，向后续代码（尤其是 AP 初始化）声明"保护结构已就绪"。

Rust 版中，`ProtectionArch::init()` 返回一个具体的结构体实例，调用者必须持有该实例才能调用 `load()`。这种设计把"是否已初始化"从运行时标志转化为类型状态：
- 没有实例 → 无法调用 `load()`；
- 已调用 `load()` → 保护结构生效。

SMP 阶段 AP 初始化同样遵循该模式：`init_ap(cpu_id, stack_top)` 返回 AP 的保护结构实例，再调用 `load()`。不需要额外的 `prot_init_done` 标志。

### 3.3 启动里程碑的函数拆分

C 版 `cstart()`（`main.c:403-481`）把保护结构初始化、时钟初始化、中断初始化、arch_init 都放一个函数里。Rust 版拆分为 `init_protection()`（本文）和 `init_clock_and_interrupts()`（04 文档），对应两个不同的"启动里程碑"：

1. **`init_protection()` 之后**：GDT/TSS/SYSCALL-MSR 生效，CPU 可安全进行系统调用；IDT 需等到 `set_handler()` 完成后才真正加载，因此异常响应在 13 文档阶段才完全可用。
2. **`init_clock_and_interrupts()` 之后**：CPU 可响应硬件中断。

这两个里程碑不可交换：中断控制器初始化后硬件中断可能立即到来，保护结构（GDT/TSS/SP_EL1/sscratch）必须先就绪，否则缺少有效的内核栈与特权级上下文会 triple fault。

---

## 4. 实现详解

> Ch1 §1.4 提出 CPU 视角三问，`prot_init()` 给 CPU 填三个答案。本节展示 Rust 实现如何对应这三问——`ProtectionArch` 承担第一问+第三问（特权级+内核栈），`TrapEntryArch` 承担第二问（异常入口）。Ch3 §3.1 的 CPU 问题→Trait 映射表是本节的总纲。

Rust 版将 `prot_init()` 拆分为两个 trait 抽象：`ProtectionArch` 负责**特权级隔离与内核栈设置**，`TrapEntryArch` 负责**异常/中断/系统调用入口**。这两个 trait 的拆分依据是 Ch2 §2.3 分析的 C 版 `prot_init()` 内部的两个独立职责——`tss_init()`（保护结构）和 `idt_init()`（异常向量）。

两个 trait 的顺序不可交换：最终 `ProtectionArch::load()` 必须在 `TrapEntryArch::load()` 之前——因为异常处理函数运行在内核态，需要有效的特权级和栈设置。如果先加载 IDT 后加载 GDT，第一个异常就会因为段选择子无效而 triple fault。

### 4.1 ProtectionArch trait 抽象

trait 定义回答 CPU 三问中的第一问（特权级）和第三问（内核栈）：

```rust
pub trait ProtectionArch: Sized {
    type PrivilegeLevel: Copy + Eq + core::fmt::Debug;

    const KERNEL_PRIVILEGE: Self::PrivilegeLevel;
    const USER_PRIVILEGE: Self::PrivilegeLevel;

    fn to_privilege(level: Self::PrivilegeLevel) -> Privilege;
    fn from_privilege(privilege: Privilege) -> Self::PrivilegeLevel;
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);
    fn load(&self);
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}
```

> 实现位置：`os/arch/src/arch/protection.rs:71`。
>
> 各方法语义：
>
> - `PrivilegeLevel` 关联类型：封装架构特有的特权级表示（x86-64 Ring、ARM64 EL、RISC-V mode），避免上层代码直接接触硬件编码。
> - `to_privilege` / `from_privilege`：OS 概念 `Privilege::Kernel/User` 与架构编码之间的双向转换。
> - `init()`：建立保护结构的"蓝图"——x86-64 填充 GDT 描述符和 TSS，aarch64 设置 SP_EL1，riscv64 准备 sscratch。
> - `set_kernel_stack()`：配置特权级切换时的目标内核栈。这是安全转场的核心——用户态触发异常或系统调用时，CPU 必须知道切换到哪个栈，否则会继续使用用户态栈（已映射但不可信）。
> - `load()`：将蓝图写入硬件寄存器（`lgdt`/`ltr`、`msr` 等），从此刻起保护结构生效。
> - `init_ap()`：AP（应用处理器）启动时的初始化，与 BSP 的 `init()` 共享大部分逻辑但有少量差异（如 AP 不需要 `lgdt` 全局同步）。
>
> **细节**：`PrivilegeLevel` 是 trait associated type，bound 为 `Copy + Eq + Debug`，封装架构特有的特权级表示（x86-64 `Ring(0/3)` / aarch64 `EL(0/1)` / riscv64 `Mode(S/U)`），让上层代码只接触 OS 概念（`Privilege::Kernel/User`），不直接接触硬件编码。
>
> **关于 `init` / `init_ap` / `load` 的语义分离**：
> - `init()`：建立保护结构的"蓝图"，在内存中准备好 GDT 描述符、TSS、异常向量表
> - `load()`：把蓝图写入硬件寄存器（`lgdt`/`ltr`、`msr`、`csrw`），使契约生效
> - `init_ap()`：AP（应用处理器）启动时的初始化，与 BSP 的 `init()` 共享大部分逻辑但有少量差异（如 AP 不需要 `lgdt` 全局同步）

### 4.2 TrapEntryArch trait 抽象

trait 定义回答 CPU 三问中的第二问（异常入口）：

```rust
pub trait TrapEntryArch: Sized {
    fn init() -> Self;
    fn configure_syscall(&mut self, entry_point: VirBytes);
    fn load(&self);
    fn load_ap(&self);
    fn set_handler(&mut self, vector: InterruptVector, handler: VirBytes, user_accessible: bool);
}
```

> 实现位置：`os/arch/src/arch/trap_entry.rs:76`。
>
> 各方法语义：
>
> - `init()`：填充异常向量表——CPU 异常（除零、页错误等）、硬件中断（PIC/IOAPIC）、系统调用入口。
> - `configure_syscall()`：配置系统调用机制。x86-64 需要写入 MSR（LSTAR/SFMASK），aarch64/riscv64 使用异常向量中的统一入口，无需额外配置。
> - `load()`：将向量表基址写入硬件寄存器（`lidt`、`msr VBAR_EL1`、`csrw stvec`），从此刻起异常和中断有去向。**本文阶段不调用 `trap.load()`**，因为 handler 地址仍为 0；真正的加载在 [13-exception-interrupt.md](13-exception-interrupt.md) 阶段 `set_handler()` 之后。
> - `load_ap()`：AP 启动时的异常向量加载。
> - `set_handler()`：设置具体中断/异常的处理函数。上层代码传 OS 概念（"时钟中断"、"页错误"），底层实现映射到架构特有的向量号。OS 概念与硬件编码完全解耦。
>
> **为什么 `configure_syscall` 是独立方法**：x86-64 的系统调用入口由 MSR 配置（LSTAR MSR），与 IDT 中的异常入口完全独立；aarch64/riscv64 的系统调用走统一异常入口（SVC/ecall），无需额外配置。将系统调用入口作为独立方法，使架构差异体现在方法体内，调用者无需 `#[cfg]`。
>
> **为什么 `set_handler` 用 `InterruptVector` 枚举**：上层代码传 OS 概念（"时钟中断"、"页错误"），底层实现映射到架构特有的向量号。OS 概念与硬件编码完全解耦。

> **为什么 aarch64/riscv64 的 `set_handler` 是 no-op**：ARM64 使用固定异常向量表（VBAR_EL1 指向汇编定义的 16 个入口），RISC-V 使用 Direct 模式（stvec 指向统一入口）。这两种架构的中断分发在汇编层面完成，具体 handler 的路由由软件分发器在运行时完成，不需要像 x86-64 那样在 IDT 中动态修改门描述符。因此 `set_handler` 在 ARM64/RISC-V 上是空操作——硬件向量表在 `load()` 时一次性设置完毕。

### 4.3 init_protection() 的实现

> 概念：建立"特权级 + 栈"契约（`ProtectionArch`）+ 准备"异常向量"契约（`TrapEntryArch`）

```rust
fn init_protection(kernel_info: &KernelInfo) {
    // 步骤 1: 回答"用户态陷入内核时切到哪个栈"
    // —— ProtectionArch::init() 写入内核栈顶到 TSS.sp0 / SP_EL1 / sscratch
    // C: tss_init(0, &k_boot_stktop) — protect.c:338
    let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);
    // 步骤 2: 让"特权级 + 栈"契约生效
    // —— x86-64 写 GDTR + ltr；aarch64 写 SP_EL1 后 isb；riscv64 写 sscratch（CSR 立即生效）
    prot.load();

    // 步骤 3: 准备"异常向量表"内容（handler 地址仍为 0，仅填充元数据）
    // C: idt_init() 在这里已经填入真实 handler 地址并加载 IDT；Rust 拆分到 13 文档阶段。
    let mut trap = CurrentTrapEntry::init();
    // 步骤 4: 配置系统调用入口（仅 x86-64 写 LSTAR MSR；aarch64/riscv64 用统一异常入口）
    // C: SYSCALL MSR 设置 — protect.c:189-205
    trap.configure_syscall(kernel_info.syscall_entry);
    // 注意：本文阶段不调用 trap.load()。IDT 元数据（DPL/IST/门类型）已就绪，
    // 但 handler 地址为 0，现在加载会导致任何异常跳转到地址 0。
}
```

> 实现位置：`os/kernel/src/lib.rs:574`。C 版对应 `protect.c:321`（x86）/ `protect.c:77`（ARM）。

> **运行时更新内核栈**: 上面 `ProtectionArch::init(0, kern_stack_top)` 只在 **boot 阶段**写入 BSP（CPU 0）的内核栈。**进程调度时切换到新进程的内核栈**则通过 `ProtectionArch::set_kernel_stack(cpu_id, new_stack_top)` 单独完成——x86-64 写 `TSS.sp0`，aarch64 写 `SP_EL0`/`sscratch`，riscv64 写 `sscratch`。SMP 阶段新增 AP 初始化时也通过 `init_ap(cpu_id, stack_top)` + `set_kernel_stack()` 双步完成。

**代码与 CPU 三问的对应**（Ch1 §1.4 → Ch3 §3.1 → Ch4 三层闭环）:

| 代码 | Ch1 §1.4 的 CPU 问题 | 架构差异点 |
|------|------------------|----------|
| `CurrentProtection::init(0, kern_stack_top)` | 第三问：陷入内核用哪个栈？ | x86-64: TSS.sp0；aarch64: SP_EL1；riscv64: sscratch |
| `prot.load()` | 第一问+第三问：让特权级与栈契约生效 | x86-64: GDTR + ltr；aarch64: isb；riscv64: 无显式 load（CSR 立即生效） |
| `CurrentTrapEntry::init()` | 第二问：异常向量表里有什么？ | x86-64: IDT 门描述符（handler 地址暂为 0，仅元数据）；aarch64/riscv64: 异常向量表由汇编定义，软件只配置入口 |
| `trap.configure_syscall(syscall_entry)` | 第二问：系统调用走哪个入口？ | x86-64: LSTAR MSR；aarch64/riscv64: 统一异常入口，无需配置 |
| `trap.load()`（本文不调用） | 第二问：让异常向量生效 | x86-64: IDTR；aarch64: VBAR_EL1 + isb；riscv64: stvec；推迟到 `set_handler()` 之后 |

### 4.4 三架构的"概念 → 代码"映射

下表是概念到代码的"存在性证明"：读者无需细读代码即可验证 Ch1 §1.4 的 CPU 三问被每个架构正确回答。

| CPU 三问 | x86-64 | aarch64 | riscv64 |
|---------|--------|---------|---------|
| 1. 特权级生效 | `lgdt gdt_desc` + `ltr TSS_SEL`（`os/arch/src/x86_64/protection.rs:267`） | `isb`（SP_EL1 写入后同步，`os/arch/src/arm64/protection.rs:118`） | 无显式 load（CSR 写入即生效，`os/arch/src/riscv64/protection.rs:105`） |
| 3. 内核栈顶存放 | `TSS.sp0 = kern_stack_top - X86_64_STACK_TOP_RESERVED`，并保留顶部 16 字节存放进程指针与 CPU id（`os/arch/src/x86_64/protection.rs:195`） | `msr SP_EL1, kern_stack_top`（`os/arch/src/arm64/protection.rs:92`） | `csrw sscratch, kern_stack_top`（`os/arch/src/riscv64/protection.rs:92`） |
| 2. 异常向量表内容 | IDT 256 个门描述符（handler 地址暂为 0，仅元数据） | 异常向量表（汇编定义） | trap 向量（汇编定义） |
| 2. 异常向量表生效 | `lidt idt_desc`（`os/arch/src/x86_64/trap_entry.rs:251`，本文阶段不执行，推迟到 `set_handler()` 后） | `msr vbar_el1, &exc_vector_table` + `isb`（`os/arch/src/arm64/trap_entry.rs:66`） | `csrw stvec, &trap_vector`（`os/arch/src/riscv64/trap_entry.rs:66`） |
| 2. 系统调用入口 | `wrmsr MSR_LSTAR, syscall_entry`（`os/arch/src/x86_64/trap_entry.rs:217`） | 走 SVC 异常入口（无需配置，`os/arch/src/arm64/trap_entry.rs:45`） | 走 ecall 异常入口（无需配置，`os/arch/src/riscv64/trap_entry.rs:41`） |
| 3. 用户态陷入内核栈切换 | CPU 硬件自动用 TSS.sp0 | 异常时硬件自动用 SP_EL1 | U→S 时 `sscratch` 存内核栈顶，handler 用 `csrrw` 交换 sp↔sscratch（sp=内核栈，sscratch=用户栈） |

> **代码不在文档中展开**：完整实现在 `os/arch/src/{x86_64,aarch64,riscv64}/{protection,trap_entry}.rs`。文档列出代码作为概念存在性证明，足以让读者理解 Ch1 §1.4 的 CPU 三问如何被回答；完整代码细节（位编码、寄存器顺序、barrier 类型）属于实现层，不在概念文档展开。

### 4.5 架构特定的类型别名

```rust
#[cfg(target_arch = "x86_64")]
pub type CurrentProtection = crate::x86_64::protection::X86_64Protection;
#[cfg(target_arch = "aarch64")]
pub type CurrentProtection = crate::aarch64::protection::AArch64Protection;
#[cfg(target_arch = "riscv64")]
pub type CurrentProtection = crate::riscv64::protection::Riscv64Protection;

pub type CurrentTrapEntry = /* 同样模式 */;
```

`CurrentProtection` / `CurrentTrapEntry` 是编译期确定的类型别名，调用者（`init_protection()`）无需 `#[cfg]` 即可获得正确的实现。

### 4.6 次要实现差异

除 §4.1–§4.5 的核心实现外，Rust 版相对 C 版还有若干次要差异，集中记录如下：

| 差异点 | C 版行为 | Rust 版处理 | 理由 |
|--------|---------|------------|------|
| `kmain` memcpy(&kinfo) | `main.c:128` 拷贝 `local_cbi` 到全局 `kinfo`（`local_cbi` 是 `kmain` 参数，作用域限于 `kmain` 调用链；拷贝到全局使非 `kmain` 调用链的代码也能访问启动信息） | `arch_boot_impl()` 返回 `&'static KernelInfo`，无需拷贝 | 借用规则保证生命周期 |
| BSS 检查 | `main.c:122-124` `assert(bss_test==0)` | 不做 | Rust `static` 语言保证零初始化；boot-shim 清零 BSS |
| GDT 描述符位运算 | `protect.c:340-348` 裸 `u32` + 宏 | `bitflags!` 宏（`PRESENT\|DPL_RING3\|CODE\|READABLE`） | 表达力 + 类型安全 |
| 重建页表 | `protect.c:357-362` `pg_clear/identity/mapkernel/load` | 不重建 | `arch_boot_impl()` 已建恒等+高地址映射（见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.4） |
| `board_id` | `main.c:130` 设置 `machine.board_id` | 不设置 | 板级识别下放到 `arch_init()`/`plat` crate，`kmain()` 保持架构无关 |
| `boot_procs` 拷贝 | `main.c:140-141` memcpy `image[]` 到 `kinfo.boot_procs` | 不拷贝 | boot-shim 已构造 `KernelInfo.boot_modules`，`init_proc_and_boot()` 直接读引用 |
| ARM 早期串口 | `main.c:132-134` `#ifdef __arm__ arch_ser_init()` | 统一到 `arch_init()` | `kmain()` 架构无关，早期设备初始化集中到 `arch_init()` |

**架构实现层对照**（概念层对照见 §1.5）:

| 概念 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 异常向量基址 | IDT（GDTR 指向）+ lidt | VBAR_EL1 | stvec |
| 特权级 | Ring 0/3（段描述符 DPL） | EL1/EL0 | S-mode/U-mode |
| 特权切换指令 | SYSCALL/SYSRET（MSR） | SVC/ERET | ecall/sret |
| 内核栈指针 | TSS.sp0（硬件自动切换） | SP_EL1（异常时硬件切换） | sscratch（U→S 时 handler 用 `csrrw` 交换 sp↔sscratch） |
| 特权级抽象 | GDT 段描述符 DPL | 系统寄存器 | CSR |

---

## 5. 测试要点

> Ch1 §1.4 提出 CPU 视角三问，本节测试验证这三个答案是否正确配置。测试表的"对应"列引用 Ch1 §1.4 的三问（a/b/c）。

### 5.1 QEMU + GDB 验证

```bash
# x86-64: 验证 GDT 已加载
qemu-system-x86_64 -kernel kernel.elf -s -S
(gdb) break init_protection
(gdb) continue
(gdb) step
(gdb) info registers gdtr  # 应显示 GDT 基址在高地址

# aarch64: 验证 VBAR_EL1 已设置
qemu-system-aarch64 -machine virt -kernel kernel.elf -s -S
(gdb) break init_protection
(gdb) continue
(gdb) step
(gdb) print $vbar_el1  # 应非零

# riscv64: 验证 stvec 已设置
qemu-system-riscv64 -machine virt -kernel kernel.elf -s -S
(gdb) break init_protection
(gdb) continue
(gdb) step
(gdb) print $stvec  # 应非零
```

### 5.2 QEMU 测试内核（三架构）

三个架构各有独立的 QEMU 测试内核，在 `os/qemu-tests/test-kernels/kernel/bootstrap/` 下：

| 测试内核 | 架构 | 验证项 | 对应 Ch1 §1.4 |
|---------|------|--------|--------------|
| `test-protection` | x86_64 | GDT 已加载（GDTR.base != 0）、IDT 已加载（IDTR.base != 0）、TSS 已加载（TR != 0）、CS/DS RPL=0 | 全部三问 |
| `test-protection-aarch64` | aarch64 | CurrentEL=EL1、VBAR_EL1 非零、DAIF 全屏蔽、SPSel 可切换、SP_EL1 读写验证、kern_stack_top 在内核 VA 范围 | §1.4b 异常入口 + §1.4a 特权级 |
| `test-protection-riscv64` | riscv64 | stvec 已设置（Direct 模式）、sscratch=kern_stack_top、sstatus 为 S-mode | 全部三问 |

> **aarch64 的 SP_EL1 写入与栈保护**：ARM64 用 `SP_EL1` 作为异常进入时的内核栈指针。当 `SPSel=1`（默认）时，`SP_EL1` 与当前 `sp` 是同一个物理寄存器：执行 `msr SP_EL1, xN` 会立即改变当前栈指针。如果 `set_kernel_stack` / `init` 的汇编实现不先保存当前 `sp`、写完再恢复，那么这条 MSR 指令本身就会把当前栈切到尚未初始化的 `kernel_stack_top` 上，当前函数的返回地址、局部变量、寄存器保存区全部丢失，紧接着的下一条指令就会因栈无效而崩溃或返回到随机地址。正确的序列是：保存 `sp` → `msr SP_EL1, xN` → 恢复 `sp`，让当前执行流继续用旧栈，只有异常进入时才自动切换到 `SP_EL1`。`test-protection-aarch64` 包含 SP_EL1 读写验证：写入一个测试值后读回，确认新内核栈指针已正确设置。

运行方式：

```bash
cd os/qemu-tests
./run_qemu.sh x86_64   <path>/test-protection.efi
./run_qemu.sh aarch64  <path>/test-protection-aarch64.efi
./run_qemu.sh riscv64  <path>/test-protection-riscv64
```

或批量运行：

```bash
cd os/qemu-tests && ./run_all.sh
```

### 5.3 已有单元测试

本节仅列出与 **保护结构**（`ProtectionArch` + `TrapEntryArch`）直接相关的单元测试。时钟相关 trait（`ClockArch`）的测试（例如 `test_read_tsc_default_delegates_to_read_ticks`）属于 [04-clock-interrupt-init.md](04-clock-interrupt-init.md) 的范围，不在本节展开。

#### x86_64（`os/arch/src/x86_64/{protection,trap_entry}.rs`）

| 测试 | 验证的 CPU 问题 | 对应 Ch1 §1.4 |
|------|--------------|--------------|
| `tss64_size_is_104_bytes` | TSS 结构体布局符合 Intel SDM | §1.4c 内核栈 |
| `tss64_offsets_correct` | TSS.sp0/IST/iobase 偏移正确 | §1.4c 内核栈 |
| `segment_selectors_correct` | CS/DS 选择子值（0x08/0x10/0x1B/0x23）正确 | §1.4a 特权级 |
| `privilege_level_roundtrip` | Ring0↔Kernel, Ring3↔User 转换正确 | §1.4a 特权级 |
| `idt_entry64_size_is_16_bytes` | IDT 门描述符布局符合 Intel SDM | §1.4b 异常入口 |
| `idt_ptr_size_is_10_bytes` | IDT 指针（limit+base）布局正确 | §1.4b 异常入口 |
| `star_register_value_correct` | STAR MSR 中内核/用户段选择子位置正确 | §1.4a 特权级 |
| `gdt_descriptors_have_correct_dpl` | GDT 描述符 DPL 字段（内核段=0，用户段=3） | §1.4a 特权级 |
| `gdt_descriptors_are_flat_mode` | 64-bit code (L=1), page granularity (G=1) | §1.4a 特权级 |
| `set_kernel_stack_updates_sp0` | `set_kernel_stack()` 写入 TSS.sp0 语义 | §1.4c 内核栈 |
| `set_kernel_stack_panics_on_invalid_cpu_id` | cpu_id 越界检查 | §1.4c 内核栈 |
| `tss_descriptor_is_64bit` | TSS 描述符类型=64-bit TSS available | §1.4c 内核栈 |
| `init_fills_gdt_correctly` | `init()` 填充 GDT 描述符（access byte, L bit） | §1.4a 特权级 |
| `init_sets_tss_sp0_below_reserved_area` | `init()` 设置 TSS.sp0 = kernel_stack_top - X86_64_STACK_TOP_RESERVED | §1.4c 内核栈 |
| `init_sets_cpu_count` | `init()` 设置 cpu_count = cpu_id + 1 | §1.4c 内核栈 |
| `init_creates_tss_descriptor_in_gdt` | `init()` 在 GDT 中创建 TSS 描述符 | §1.4c 内核栈 |
| `gdt_null_entry_is_zero` | GDT[0] = 0（null descriptor） | §1.4a 特权级 |
| `tss_iobase_disables_io_bitmap` | TSS.iobase = 0x8000 禁用 I/O bitmap | §1.4c 内核栈 |
| `set_handler_sets_dpl_correctly` | `set_handler()` DPL=3/0 正确设置 | §1.4b 异常入口 |
| `set_handler_writes_handler_address` | `set_handler()` 地址正确拆分到 IDT 字段 | §1.4b 异常入口 |
| `gate_type_constants_correct` | 中断门=0xE, 陷阱门=0xF, Present=0x80 | §1.4b 异常入口 |
| `msr_constants_correct` | STAR/LSTAR/SFMASK/EFER MSR 地址正确 | §1.4b 异常入口 |
| `star_register_layout` | STAR SYSCALL/SYSRET CS/SS 选择子正确 | §1.4a 特权级 |
| `sfmask_clears_if_on_syscall` | SFMASK 仅清除 IF (bit 9) | §1.4a 特权级 |
| `idt_init_sets_exception_gates` | `init()` 设置异常/IRQ 门描述符 | §1.4b 异常入口 |
| `idt_init_breakpoint_has_dpl3` | INT3 (vector 3) DPL=3 | §1.4b 异常入口 |
| `idt_init_overflow_has_dpl3` | INTO (vector 4) DPL=3 | §1.4b 异常入口 |
| `idt_init_double_fault_uses_ist2` | Double fault (vector 8) IST=2 | §1.4b 异常入口 |
| `idt_init_nmi_uses_ist1` | NMI (vector 2) IST=1 | §1.4b 异常入口 |
| `idt_init_kernel_exceptions_have_dpl0` | 内核异常 DPL=0 | §1.4b 异常入口 |
| `idt_init_reserved_vectors_not_present` | x86-64 保留向量 9/15 不设置门描述符 | §1.4b 异常入口 |
| `idt_init_sets_syscall_ipc_vectors` | 向量 32-35（系统调用/IPC soft-int）已设置且 DPL=3 | §1.4b 异常入口 |
| `idt_init_sets_pic_vectors` | 向量 80-87、112-119（PIC 硬件中断）已设置且 DPL=0 | §1.4b 异常入口 |

#### ARM64（`os/arch/src/arm64/{protection,trap_entry}.rs`）

| 测试 | 验证的 CPU 问题 | 对应 Ch1 §1.4 |
|------|--------------|--------------|
| `privilege_level_values` | EL1=1, EL0=0 | §1.4a 特权级 |
| `privilege_level_roundtrip` | EL1↔Kernel, EL0↔User 转换正确 | §1.4a 特权级 |
| `kernel_privilege_is_el1` | KERNEL_PRIVILEGE = EL1 | §1.4a 特权级 |
| `user_privilege_is_el0` | USER_PRIVILEGE = EL0 | §1.4a 特权级 |
| `el1_maps_to_kernel` | EL1 → Privilege::Kernel | §1.4a 特权级 |
| `el0_maps_to_user` | EL0 → Privilege::User | §1.4a 特权级 |
| `protection_has_cpu_count` | AArch64Protection 结构体可构造 | §1.4c 内核栈 |
| `trap_entry_init_returns_unit_struct` | AArch64TrapEntry::init() 不 panic | §1.4b 异常入口 |
| `configure_syscall_is_noop` | ARM64 SVC 无需 MSR 配置 | §1.4b 异常入口 |
| `set_handler_is_noop` | ARM64 固定向量表，set_handler 为 no-op | §1.4b 异常入口 |

#### RISC-V（`os/arch/src/riscv64/{protection,trap_entry}.rs`）

| 测试 | 验证的 CPU 问题 | 对应 Ch1 §1.4 |
|------|--------------|--------------|
| `privilege_level_values` | S_MODE=1, U_MODE=0 | §1.4a 特权级 |
| `privilege_level_roundtrip` | S_MODE↔Kernel, U_MODE↔User 转换正确 | §1.4a 特权级 |
| `kernel_privilege_is_s_mode` | KERNEL_PRIVILEGE = S_MODE | §1.4a 特权级 |
| `user_privilege_is_u_mode` | USER_PRIVILEGE = U_MODE | §1.4a 特权级 |
| `protection_has_cpu_count` | Riscv64Protection 结构体可构造 | §1.4c 内核栈 |
| `trap_entry_init_returns_unit_struct` | Riscv64TrapEntry::init() 不 panic | §1.4b 异常入口 |
| `configure_syscall_is_noop` | RISC-V ecall 无需 CSR 配置 | §1.4b 异常入口 |
| `set_handler_is_noop` | RISC-V Direct 模式，set_handler 为 no-op | §1.4b 异常入口 |

### 5.4 测试缺口

对照 Ch1 §1.4 的 CPU 三问和 §4 的实现，以下场景尚无测试覆盖：

| 缺口 | 对应 CPU 问题 | 优先级 | 说明 |
|------|------------|--------|------|
| init/load 顺序约束 | 全部三问 | P1 | §4.3 强调 ProtectionArch::load() 必须先于 TrapEntryArch::load()，无测试验证违反顺序的后果 |
| `init_ap` 路径验证 | §1.4c 内核栈 | P1 | AP 启动路径完全未测试（需要 SMP 硬件/模拟） |

> **说明**：上述两项均依赖 SMP 多核支持，将在 SMP 阶段补充。

---

## 6. 过渡：从 init_protection() 到 init_clock_and_interrupts()

`init_protection()` 完成后，CPU 已具备正确的特权级、内核栈切换能力（x86-64 TSS、aarch64 SP_EL1、riscv64 sscratch）以及系统调用入口（x86-64 LSTAR MSR），但异常向量表（IDT/VBAR_EL1/stvec）尚未真正加载到硬件，中断控制器也尚未初始化，时钟尚未启动。

> 用 Ch1 §1.4 的 CPU 三问框架看：第一问（特权级）和第三问（内核栈）已就绪；第二问（异常入口）的元数据已准备但尚未加载到硬件——真正的加载推迟到 [13-exception-interrupt.md](13-exception-interrupt.md) 阶段 `set_handler()` 之后。

| 架构 | init_protection() 之后的状态 |
|------|----------------------|
| x86-64 | GDT 已加载（GDTR）、TSS 已加载（TR）、CS/DS/SS/ES 正确；IDT 元数据已准备但 **IDTR 尚未加载** |
| aarch64 | SP_EL1 设置为内核栈；VBAR_EL1 将在后续阶段设置 |
| riscv64 | stvec 指向 trap 入口、sscratch 保存内核栈 |

`init_clock_and_interrupts()` 接管这些工作：初始化时钟源 → 初始化中断控制器（8259A / APIC / GIC / PLIC）→ 架构特定初始化。**这一阶段必须在 `init_protection()` 之后**，因为中断控制器初始化后硬件中断可能立即到来；虽然本文阶段 IDT/VBAR/stvec 尚未最终加载，但保护结构（GDT/TSS/SP_EL1/sscratch）必须先就绪，否则后续加载向量表或中断到来时缺少有效的内核栈与特权级上下文，同样会 triple fault。

反过来，**`init_clock_and_interrupts()` 也不能在 `init_protection()` 之前**：
- **ARM GIC / RISC-V PLIC**：中断控制器寄存器通过 MMIO 访问，必须已在页表中映射才能读写；
- **x86-64 8259A/APIC**：8259A 通过 I/O 端口访问，Local APIC 通过 MMIO（默认 `0xFEE00000`）访问，这些映射同样由 `arch_boot_impl()` 在 02 文档阶段完成。

`init_protection()` 本身不重建页表，但它运行时已保证 `arch_boot_impl()` 建立的恒等映射 + 内核高地址映射生效（见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.4），因此 `intr_init()` 可以安全访问 MMIO。`init_protection()` 与 `init_clock_and_interrupts()` 的顺序不可交换。

---

## 7. 参见

- [02-higher-half-kernel.md](02-higher-half-kernel.md) — CPU 已切换到高地址
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — 时钟与中断控制器初始化
- [12-syscall.md](12-syscall.md) — syscall 实现细节（SYSCALL/SYSRET、swapgs、LSTAR MSR）
- [13-exception-interrupt.md](13-exception-interrupt.md) — 异常/中断处理（trap frame、IST、sscratch 交换、`set_handler()` 加载向量表）

---

## 附录 A：为什么必须切到内核栈

> 本附录展开 §1.4c 第三问"WHY"背后的完整安全论证。核心命题：**用户栈不可信，用户控制 RSP 值和栈页权限，可以构造陷阱；切到内核栈后这些攻击全部失效**。

### A.1 威胁模型

用户态对栈有完全控制权：

- **RSP 值可任意设置**：`mov rsp, <任意值>` 是普通指令，无需特权。
- **栈页权限可任意设置**：通过 `mprotect`/`mmap` 系统调用，用户可把任意页设为只读、不可执行、甚至 `PROT_NONE`。
- **栈内容可任意构造**：用户可预填任意字节到栈页。

如果 CPU 在 user→kernel 陷入时继续用用户栈压入陷入上下文（x86-64 的 `SS:RSP:RFLAGS:CS:RIP`），用户就能利用上述控制权发起以下攻击。

### A.2 攻击向量

#### 攻击 1：RSP 指向只读页 → 陷入即死（DoS）

```
用户态：
  mprotect(rsp_page, PROT_READ);   // 把当前栈页设为只读
  int 0x80;                         // 触发系统调用

CPU 陷入（ring3→ring0）：
  硬件自动 push SS:RSP:RFLAGS:CS:RIP 到 [RSP]
  → 写只读页 → page fault
  → 此时特权级已是 ring0，但 RSP 还是坏的
  → page fault handler 想压栈保存现场，又写坏栈 → double fault
  → double fault handler 同样 → triple fault
  → CPU reset，内核 panic
```

**后果**：一个普通用户进程让整个内核崩溃。DoS 攻击成功。

#### 攻击 2：RSP 指向用户可控缓冲区 → 篡改返回地址

```
用户态：
  char buf[64];
  // 在 buf 高地址区预填伪造的 RIP（指向内核提权 gadget）
  *(uint64_t*)(buf + 56) = kernel_privilege_addr;
  asm("mov rsp, %0; int 0x80" :: "r"(buf));

CPU 陷入：
  push SS:RSP:RFLAGS:CS:RIP 到 buf 低地址
  RSP 递减指向 buf 内部

内核 handler 执行后 iretq：
  弹出 CS:RIP —— 如果内核在某个窗口期误读了 buf 高地址的伪造值
  （或栈帧布局 bug，或 SMP 下另一核用户线程同时改 buf）
  → 跳到 kernel_privilege_addr 执行 → 提权
```

**SMP 加速场景**：CPU A 陷入内核用用户栈，CPU B 上的同进程用户线程同时改 `buf`，篡改 CPU A 的陷入上下文（TOCTOU）。

#### 攻击 3：RSP 指向内核数据区 → 破坏内核结构

```
用户态（利用某页表 bug 或未撤销的临时映射）：
  mov rsp, &idt_entry;   // RSP 指向 IDT 表项
  int 0x80;

CPU 陷入：
  push SS:RSP:RFLAGS:CS:RIP → 覆盖 IDT 表项
  → 后续异常跳到被污染的 handler 地址
  → 内核控制流劫持
```

**前提**：用户态能拿到一个指向内核数据的可写映射。这本身是 bug，但历史上多次出现（如未撤销的 `vm_mappages` 临时映射、COW 页处理错误等）。

#### 攻击 4：RSP 递减溢出 → 覆盖低地址内核数据

```
用户态：
  mov rsp, 0x1000;   // RSP 设得很小
  int 0x80;

CPU 陷入：
  RSP 递减：0xFF8, 0xFF0, 0xFE8... 写入低地址
  如果 0xFC0 附近映射了内核页表/其他进程内核栈 → 被覆盖
```

### A.3 切到内核栈为什么安全

内核栈由内核分配，用户**不可见、不可写**：

| 架构 | 内核栈位置 | 页表权限 | 用户能否访问 |
|------|-----------|---------|------------|
| x86-64 | TSS.sp0 指向的页 | PTE.U=0, PTE.RW=1 | ❌ |
| aarch64 | SP_EL1 指向的页 | AP=EL1-only | ❌ |
| riscv64 | sscratch 指向的页 | PTE.U=0 | ❌ |

切换机制由硬件强制（x86/aarch64）或内核预设的指令强制（riscv64），用户无法干预：

**x86-64**：ring3→ring0 时，CPU 硬件原子完成：
1. 读 `TSS.sp0` → RSP（丢弃用户 RSP）
2. 读 `TSS.ss0` → SS
3. 压入旧 SS:RSP（用户栈位置，供 `iretq` 恢复）+ RFLAGS + CS:RIP **到内核栈**

用户 RSP 的值只被当作"数据"压入内核栈，不再被用作写入地址。

**aarch64**：SP_EL0（用户）和 SP_EL1（内核）是独立寄存器，EL0→EL1 时硬件切到 SP_EL1，用户 SP_EL0 的值不动。

**riscv64**：软件切栈，但第一条指令 `csrrw sp, sscratch, sp` 是内核预设在 `stvec` 的，用户改不了。交换后 `sp`=内核栈，`sscratch`=用户栈。

### A.4 攻击向量与切换机制的对应

| 攻击 | 利用用户控制权 | 切到内核栈后为何失效 |
|------|--------------|-------------------|
| 1 只读页 DoS | RSP 指向只读页 | 内核栈页 PTE.W=1，且用户无法修改内核页表 |
| 2 篡改返回地址 | RSP 指向用户缓冲区 | 压栈目标变为内核栈，用户无法写入 |
| 3 破坏内核结构 | RSP 指向内核数据 | 内核栈地址由内核选定，用户无法控制 TSS.sp0/SP_EL1/sscratch 的值 |
| 4 栈溢出覆盖 | RSP 递减溢出 | 内核栈有 guard page（PTE.U=0 + 未映射），溢出触发 page fault 而非覆盖数据 |

### A.5 一句话总结

用户栈的 RSP 值和栈页权限都在用户控制下。如果 CPU 继续用用户栈压入陷入上下文，用户可以：(1) 让压栈失败 → DoS；(2) 让压栈写到用户可控区域 → 篡改返回地址提权；(3) 让压栈覆盖内核数据 → 破坏内核。切到内核栈（TSS.sp0/SP_EL1/sscratch）后，压栈目标由内核控制，用户无法干预，这四类攻击全部失效。

这就是 §1.4c"必须切到可信内核栈"背后的完整安全论证——保护结构不仅是"功能正确性"问题，更是"安全边界"问题。
