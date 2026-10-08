# 03 · 接管执行环境：kmain、cstart 与保护结构

> **C 真值源**: `minix3/minix/kernel/main.c`、`minix3/minix/kernel/arch/i386/protect.c`
> **Rust 实现**: `os/kernel/src/lib.rs`、`os/arch/src/arch/protection.rs:trait ProtectionArch`、`os/arch/src/arch/trap_entry.rs:trait TrapEntryArch`

## 第 1 章　概念：进了高半区，为什么还不能放心跑

### 1.1 问题：CPU 还在用固件留下的那套规则

02 文档末尾，`jump_to_kmain` 把 CPU 送进了内核高半区、开始跑 `kmain`。但"跑在高地址"不等于"跑在**自己**的信任环境里"。此刻段描述符表、中断向量表、任务状态段这些**决定 CPU 怎么在特权级之间切换、异常来了跳到哪**的结构，多半还是固件/引导器临时给的那套——不是内核想要的隔离与门控。内核要做的第一件事，是**换掉 CPU 脚下的地基**：装自己的保护结构与异常入口。这件事的入口就是 `kmain` 里紧接着的 `cstart()`。

### 1.2 特权级是地基，保护结构是它的落地

现代 CPU 用特权级区分内核态与用户态（x86 的 Ring 0/3、ARM 的 EL1/EL0、RISC-V 的 S/U）。"用户态陷入内核"这个动作怎么走、内核栈在哪、哪些段可访问——全由一组体系结构特定的表决定。它们不对等，但语义同一：

| 语义角色 | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 段/内存访问的门控 | GDT（段描述符表） | 靠 EL + 页表，段机制退化 | 靠特权级 + 页表 |
| 异常/中断入口 | IDT（中断描述符表） | VBAR + 向量表 | trap vector 表（stvec） |
| 陷入时切到内核栈 | TSS（RSP0） | SP_EL1 | sscratch |

一句话：**三家都要回答"异常来了跳哪、栈换成谁的、谁能访问什么"，只是 x86 用 GDT/IDT/TSS 三张表、ARM 用 EL+向量+SP_EL1、RISC-V 用特权级+stvec+sscratch。** 内核初始化保护结构，就是在为自己重写这套答案。

```
CPU 遇异常/陷入时问自己的三问：
  跳哪执行?      ── x86: IDT[vec]      ── ARM: VBAR+vec   ── RV:  stvec
  用哪个栈?      ── x86: TSS.RSP0      ── ARM: SP_EL1     ── RV:  sscratch
  能访问什么?    ── x86: GDT/段+页表   ── ARM: EL+页表     ── RV:  特权级+页表
内核的 init_protection 就是重写这三张表，让答案由内核说了算。
```

### 1.3 为什么不能用固件留下的保护结构

引导器给的表只为"把内核蹦起来"服务：它的 GDT 可能没有内核需要的全数据段布局，IDT 可能根本没指向内核的异常处理，TSS 更是没设——一旦有异常或需要陷入切栈，落到别人的（或空的）结构上就是不可控行为。所以 `cstart()` 里第一件重活就是 `prot_init()`：**在开启任何中断前，把这三张表换成内核自己的**。

> **认知减负**：GDT 每个描述符的位字段、IDT 门类型（中断门/陷阱门）的差异，是 x86 体系结构细节，本篇不逐字段展开；只需抓住"三张表＝三个问题的答案"。逐字段建模归 `22-privilege.md` 与后续 trap 篇。

---

## 第 2 章　C 源码：kmain 与 cstart 怎么把地基换掉

### 2.1 kmain 入口：验 BSS、存契约、开闸门

`main.c:kmain` 进门先做四件小事，每件都在防一类启动期故障（`minix3/minix/kernel/main.c:kmain`，节选）：

```c
void kmain(kinfo_t *local_cbi)
{
    static int bss_test;
    assert(bss_test == 0);                       // (1) BSS 段确实被清零
    bss_test = 1;
    memcpy(&kinfo, local_cbi, sizeof(kinfo));    // (2) 把启动契约存成全局
    memcpy(&kmess, kinfo.kmess, sizeof(kmess));
    kernel_may_alloc = 1;                        // (3) 打开"可分配物理内存"闸门
    assert(sizeof(kinfo.boot_procs) == sizeof(image));
    memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs));
    cstart();                                    // (4) 进入平台初始化
}
```

> **辨析（看似多余的步骤）：`assert(bss_test == 0)` 为何不是废话。** C 不保证 BSS 自动清零，那是 bootloader 的责任；若引导器有 bug 或链接脚本把 BSS 范围算错，"本该是 0"的全局就带着垃圾。这一句在最早期把它撞出来，免得脏值在几百行后才引发无法归因的故障——典型的 fail-fast。(1)(3) 都是"闸门"性质的检查，而非业务逻辑。

`kernel_may_alloc` 是启动阶段的**权限闸门**：VM 服务起来之前没有内存分配后端，标志为 0 时任何分配请求都该被拒；置 1 才放行。这个全局标志在 Rust 版被更结构化的方式替代（见 §3.2）。

### 2.2 cstart：一个顺序敏感的调用序列

`main.c:cstart` 依次调（`minix3/minix/kernel/main.c:cstart`，节选）：

```c
void cstart(void) {
    prot_init();          // 主/次核保护结构（GDT/IDT/TSS）
    init_clock();         // 时钟变量
    /* 之后才解析环境变量：ac_layout(L421) / no_apic(L443) / watchdog(L456) / no_smp(L464) */
    intr_init(0);         // 中断控制器（0 = boot CPU）
    arch_init();          // 体系结构收尾（含浮点单元等）
}
```

**为什么是这个顺序，且不可乱**：`prot_init` 必须先——它装好 IDT/TSS，之后的 `intr_init` 才敢使能中断（否则中断来了没有合法的入口和内核栈）；`init_clock` 早于 `intr_init`，因为时钟中断要挂进刚建好的中断系统。这是一条**依赖链**：保护结构 → 时钟变量 → 中断入口 → 架构收尾。Rust 版把这条序拆进了 `kmain` 的若干 Phase（`os/kernel/src/lib.rs` 的 Phase A/B/C），但同样的先后约束仍在。

### 2.3 prot_init（x86）：三张表的静态存储

`minix3/minix/kernel/arch/i386/protect.c:prot_init` 操作的正是 §1.2 说的三张表，它们在 BSS 里静态预留（`protect.c` 节选）：

```c
/* Storage for gdt, idt and tss. */
struct segdesc_s   gdt[GDT_SIZE]      __aligned(DESC_SIZE);
struct gatedesc_s  idt[IDT_SIZE]      __aligned(DESC_SIZE);
struct tss_s       tss[CONFIG_MAX_CPUS];
```

`tss_init(cpu, kernel_stack)` 为每个 CPU 把内核栈顶写进对应 TSS 的 RSP0——这正是 §6 那个安全故事的关键落点：陷入时 CPU 从 TSS.RSP0 取内核栈，而不是继续用用户栈。ARM/RISC-V 无此三表（GDT 尤其无），改由 `EL`/`VBAR`/`SP_EL1`、`stvec`/`sscratch` 承担同一语义——Rust 版把这种差异收进 trait（§3）。

---

## 第 3 章　Rust 设计：把"三张表 / 向量 / sscratch"收进两个 trait

### 3.1 双 trait 抽象

C 侧的多架构切分是**按目录**：`minix3/minix/kernel/arch/i386/protect.c` 与 `arch/earm/protect.c` 各有一份 `prot_init`（riscv 在 Minix3 无 C 端口），并**不是** `#if defined(__i386__)` 预处理块。Rust 用两个架构边界 trait 把差异关进 impl、把统一语义交给 OS 层。真实方法面（节选，去掉 doc 注释）：

```rust
pub trait ProtectionArch: Sized {
    type PrivilegeLevel: Copy + Eq + core::fmt::Debug;   // 特权级是 OS 语义：类型共享、取值各异
    const KERNEL_PRIVILEGE: Self::PrivilegeLevel;
    const USER_PRIVILEGE: Self::PrivilegeLevel;
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes); // x86 TSS.sp0 / arm TPIDR_EL1 / riscv sscratch
    fn load(&self);
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
    // …to_privilege / from_privilege：与 OS 层 Privilege 互转…
}
pub trait TrapEntryArch: Sized {
    fn init() -> Self;
    fn configure_syscall(&mut self, entry_point: VirBytes);  // x86 SYSCALL MSR / arm SVC / riscv ecall
    fn configure_ipc_entry(&mut self, entry_point: VirBytes);
    fn load(&self);                                          // x86 装 IDT / arm 设 VBAR_EL1 / riscv 装 stvec
    fn set_handler(/* 按向量号装 handler */);
}
```

（`os/arch/src/arch/protection.rs:trait ProtectionArch`、`os/arch/src/arch/trap_entry.rs:trait TrapEntryArch`）

> **辨析（两个 trait 的分工，别混）：** “陷入时换哪个栈”归 `ProtectionArch::set_kernel_stack`（x86 写 TSS.sp0、arm 写 TPIDR_EL1、riscv 写 sscratch）；“异常/中断来了跳哪”归 `TrapEntryArch::load`（x86 装 IDT、arm 设 VBAR_EL1、riscv 装 stvec）。VBAR 是 `TrapEntryArch` 的职责，不是 `ProtectionArch`——两个 trait 各自把“同一语义在不同硬件上的不同承载”分开。

### 3.2 取舍：用类型状态替代 `kernel_may_alloc` 运行时闸门

| 方案 | 做法 | 代价 | 取舍 |
|---|---|---|---|
| **类型/显式 token（选用）** | 内存分配能力由"拿到某 token / 某初始化后才存在的类型"表达 | 需设计承载类型 | ✅ 未初始化时压根构造不出该值，编译期就挡住早分配 |
| `kernel_may_alloc` 全局标志（C） | 运行时 `if (!kernel_may_alloc) panic` | 全局可变、可被忘记置位或竞态 | ❌ 违反 KERNEL 面"`Rc`/裸全局标志跨 CPU 不安全"的约束 |

这是"把隐式编码变成显式协议"的典型：C 用一个进程范围的可变 int 表达"现在能不能分配"，Rust 让这件事由类型系统在编译期保证（分配入口只在初始化完成后才可获得）。

### 3.3 里程碑拆分

C 的 `cstart()` 是一根函数串；Rust 把 `kmain` 拆成 Phase A/B/C（收契约、`init_protection`、`proc_init` …），每个 Phase 有边界与日志路标，便于在真机上定位"卡在哪个里程碑"。

---

## 第 4 章　实现：init_protection 与三架构落点

`kmain` 在 Phase B 调 `init_protection(kernel_info)`（`os/kernel/src/lib.rs`，prot_init 对位），它按目标架构分派到 `ProtectionArch`/`TrapEntryArch` 的实现：x86-64 写 GDT 段描述符、IDT 门表、每 CPU 的 TSS.RSP0；aarch64 设 VBAR 与 `SP_EL1`；riscv64 装 stvec 与 `sscratch`。**三家写的是完全不同的寄存器/表，但都在回答 §1.2 那三个问题**——OS 语义层（`kmain`）只调 `init_protection`，看不见这些差异（跨架构四公理，`review-core-semantics.md` §1.6）。

`init_protection` 之后才轮到 `proc_init`（清进程表）与 boot 进程诞生——这条先后正是 §2.2 依赖链在 Rust 侧的延续：先把异常入口与内核栈备好，再让可能触发陷入的调度跑起来。

### 4.2 实地导览：现在打开代码，kmain 是按这个顺序排的

上面是"为什么"；这里是"打开仓库怎么顺着读"。从 `jump_to_kmain` 落地的 `kmain` 往下，执行路径是一条带里程碑日志的链（锚点均 `os/kernel/src/lib.rs`）：

```
kmain(kernel_info)                       // 本篇入口，Phase A
  │  store KernelInfo 进全局、validate()      ← 接住 01 交来的契约
  ├─ Phase B: init_protection(kernel_info)    ← 本篇章：装保护结构（prot_init 对位）
  ├─ (init_clock_and_interrupts 等)          ← 时钟/中断 → 05 篇细讲
  └─ Phase C: proc_init() + arch_boot_proc   ← 清进程表、boot 进程诞生 → 06 篇
```

每一里程碑都打一行 `boot_stage!("kernel: kmain C proc_init ok\n")` 式日志——这不是装饰，而是 §6 定位的基础：哑机器时，串口上**最后出现的那条 `boot_stage!`** 就把故障夹在两个里程碑之间。读这篇时按这个序去 `lib.rs` 里找对应函数，Phase B 的 `init_protection` 分派到 §3.1 那两个 trait 的实现，Phase C 之前的 `validate`/store 就是 01-boot-shim-bootstrap.md §4.1 那条 caller-saved 陷阱的落地点。**把这条链走一遍，“保护结构为什么在 proc_init 之前”就不再是一句结论，而是你在代码里亲眼看到的位置。**

---

## 第 5 章　测试要点

保护结构正确性由三层守：`os/kernel/src` 内 `init_protection`/TSS/IDT 相关的 `#[cfg(test)]` 单测；`os/qemu-tests/test-kernels/kernel/bootstrap/` 的 `test-protection` 三架构端到端（异常入口与内核栈在真机上生效）；GDB 单步可核。**测试数现算为准**（`rg -c '#\[test\]'`、引导内核目录 `ls ... | grep -c protection`），不抄表。

> **认知减负**：真机门需 QEMU，宿主 `cargo test` 只覆盖到静态建模与 mock 序；GDT/IDT 实际装载生效只能在真机验证。

---

## 第 6 章　诊断与安全：为什么陷入必须切内核栈

这一章回答一个安全本质问题，也是本篇最该讲透的反例——**如果 CPU 陷入内核时继续用用户栈，会怎样。**

### 6.1 威胁模型：用户完全掌控自己的栈

用户态对它的栈有绝对权力：`mov rsp, <任意值>` 是普通指令无需特权；`mprotect`/`mmap` 能把任意页设成只读、不可执行、甚至 `PROT_NONE`；栈内容可预填任意字节。而 x86-64 在 user→kernel 陷入时会压入 `SS:RSP:RFLAGS:CS:RIP`——如果这一步落在**用户可控的栈**上，攻击面立刻打开。

### 6.2 反例推演：不切内核栈会坏什么

- **DoS（把栈指到只读页）**：用户把 RSP 设成指向一个只读页再触发 `int`，CPU 压陷入帧时写只读页 → 双重 fault → 内核还没跑任何一行处理代码就崩。用户不花任何特权就弄死系统。
- **内容劫持（预填栈 + 指到可控页）**：用户预写好帧布局的字节到可控页、把 RSP 指过去再陷入，伪造的 `RIP/CS` 若被 CPU 在切换前采信，就可能改变内核恢复执行的去向。

**切到内核栈为何根治它**：`prot_init` 给每 CPU 的 TSS 写好 `RSP0 = 内核栈顶`（§2.3 的 `tss_init`）。硬件在特权级提升时**先从 TSS 拿 RSP0 换栈、再压帧**——陷入帧落在内核独占、用户不可读写的栈上，上面两个攻击都失去立足点。ARM/RISC-V 用 `SP_EL1`/`sscratch` 承担同一"换到内核栈再保存上下文"的职责。**这不是风格选择，是硬件强制的安全边界**：谁把栈交给用户，谁就把陷入帧交给攻击者。

### 6.3 可复制的定位命令

```bash
# 端到端验证保护结构在真机生效（异常入口 + 陷入切内核栈；预期：串口打出 PASS 标记）
os/qemu-tests/run_qemu.sh <arch> <test-kernel>   # 例：arch=x86_64，test-kernel 为对应 .efi

# 看内核陷入栈是怎么被钉进 TSS.RSP0 / SP_EL1 / sscratch 的
rg -n "RSP0|rsp0|tss_init|SP_EL1|sscratch" os/arch/src os/kernel/src
```

预期输出的关键行：正常是 `test-protection` 打出一次 PASS、无异常早崩；若在内核起来前就 triple fault，优先回看 `prot_init` 是否在 `intr_init` 之前完成（§2.2 的顺序链）。
