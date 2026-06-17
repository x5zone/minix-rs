# 03-kmain-cstart: kmain 入口与 cstart 平台初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/main.c:115-147,403-481`, `minix3/minix/kernel/arch/i386/protect.c:321-367`, `minix3/minix/kernel/arch/earm/protect.c:77-93`
> **说明**: 从 `kmain()` 入口到保护结构就绪——内核建立保护模式基础设施
> **前置**: [02-higher-half-kernel.md](02-higher-half-kernel.md) — CPU 已切换到高地址，进入 `kmain`

---

## 1. 概念：什么是"保护结构"，内核为什么必须自己建立它

### 1.1 一句话

`prot_init()` 让内核从"借用固件的运行上下文"变成"拥有自己的运行上下文"。从此刻起，异常向量、特权级切换、内核栈的归属权都从固件转移到内核。

### 1.2 三个问题

任何正在运行的程序，都需要回答 CPU 的三个隐含问题：

| 问题 | 缺失回答的后果 | 保护结构提供的答案 |
|------|--------------|------------------|
| **异常发生时的处理入口** | 除零/缺页/未定义指令触发时 CPU 无有效入口，通常在 x86-64 上表现为 **triple fault**（连续三次异常导致 CPU 复位），系统直接重启 | 异常向量表（x86-64 IDT / aarch64 VBAR_EL1 / riscv64 stvec） |
| **用户态可访问的内存范围** | 页表/段描述符无 DPL 隔离 → 用户进程可读/写内核数据、可执行内核代码 → **权限隔离完全失效**，任意进程即可破坏内核 | 页表权限位 + 段描述符（x86-64 GDT 段 DPL） |
| **用户态陷入内核时使用的栈** | CPU 继续用用户态栈保存 `cs/rip/ss/rsp` 等上下文；恶意进程可把栈顶设在只读页或构造畸形栈帧 → **上下文被篡改或内核栈溢出** | 内核栈指针（x86-64 TSS.sp0 / aarch64 SP_EL1 / riscv64 sscratch） |

这三类机制的集合称为**保护结构**。保护结构是 CPU 与操作系统之间的契约：操作系统填写保护结构，向 CPU 声明上述问题的答案；CPU 在每次异常、特权级切换、系统调用时查表执行。

### 1.3 为什么不能用固件留下的保护结构

进入 `kmain()` 时，CPU 确实有正在生效的保护结构，但这些结构对内核不可信：

- **生命周期不可控**：UEFI 的 GDT/IDT 位于 UEFI 运行时内存。`ExitBootServices()` 后，这部分内存可被内核作为空闲物理页回收，下一个异常就找不到处理函数。
- **语义不匹配**：UEFI 的 GDT 为运行 PE32+ 程序设计，只含代码段和数据段选择子。Minix3 内核需要 TSS（任务状态段）实现用户态→内核态栈切换，UEFI 的 GDT 中没有 TSS 描述符。
- **架构差异**：x86 用 GDT + IDT + TSS 三层结构；aarch64/riscv64 用单层异常向量。固件留下的保护结构无法满足内核的特权级切换需求。

`prot_init()` 的语义：内核在此刻之前一直借用他人的保护结构（boot-shim 用 UEFI 的，GRUB 用 BIOS 的）。`prot_init()` 之后，内核拥有自己的保护结构。保护结构的所有权从固件转移到内核，是内核从"被引导的程序"转变为"操作系统的核心"的标志。

### 1.4 kmain 的六阶段启动图景

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
│  (02 文档)  │     │ (A/B-1) │     │   ProtectionArch::init()    │
└─────────────┘     └─────────┘     │   ProtectionArch::load()    │
                                    │   TrapEntryArch::init()     │
                                    │   (TrapEntryArch::load()    │
                                    │    推迟到 set_handler 之后) │
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

> **为什么 cstart 必须先于后续所有阶段**：在 cstart 之前，CPU 处于不可信状态：
> - x86-64 没有自己的 GDT/IDT，使用 UEFI/GRUB 留下的描述符表，段选择子可能不正确
> - aarch64/riscv64 的异常向量表基址寄存器未设置，任何异常都会 triple fault
> - 时钟未启动，无法计时
> - 中断控制器未初始化，无法响应硬件事件
>
> 一旦进入 `proc_init()`，就要开始设置进程的段寄存器（x86-64 的 CS/DS/SS）和栈指针，这些操作依赖 GDT 中的段描述符。若 GDT 未就绪，进程切换时会 GP fault。

### 1.5 prot_init 的统一抽象：两个独立的职责

虽然三种架构的 `prot_init()` 代码完全不同（x86-64 填 GDT/IDT/TSS，aarch64 写 VBAR_EL1，riscv64 写 stvec/sscratch），但它们都在做两件正交的事：

1. **建立"特权级与栈"的契约**：声明用户态陷入内核时切到哪个栈、特权级如何转换
2. **建立"异常向量"的契约**：声明异常发生时的处理函数入口

在 Rust 实现中，这两件事被抽象为两个独立 trait：

| Trait | 回答的概念问题 | Minix3 C 中的对应 |
|-------|--------------|------------------|
| `ProtectionArch` | 用户态不能访问的内存范围 + 进程切换时栈的安全转场 | `tss_init()` + GDT 段描述符填充 + GDTR 加载 |
| `TrapEntryArch` | 异常发生时的处理入口 | `idt_init()` 填充异常向量表元数据；IDTR 加载推迟到后续阶段 `set_handler()` 之后 |

> **拆分依据 1 — 加载顺序**：`ProtectionArch::load()` 必须在最终的 `TrapEntryArch::load()` 之前，因为异常处理函数运行在内核态，需要有效的特权级和栈设置。若先加载 IDT 后加载 GDT，第一个异常会因段选择子无效而 triple fault。本文阶段只调用 `ProtectionArch::load()`；`TrapEntryArch::load()` 在 [13-exception-interrupt.md](13-exception-interrupt.md) 阶段 `set_handler()` 填入真实 handler 地址后再执行。
>
> **拆分依据 2 — 无共享代码**：两个职责在各架构上的实现完全独立，合并为单一 trait 不会减少重复代码，反而需要在方法体内用 `#[cfg(target_arch)]` 分发；拆分后每个 trait 的 impl 块按架构独立，无需方法内 cfg。

### 1.6 prot_init 与之前阶段的边界

`prot_init()` 不重建页表，此工作在 01 文档的 `arch_boot_impl()` 中已完成。`prot_init()` 与 `arch_boot_impl()` 的职责划分：

- **`arch_boot_impl()`**：建立页表 + 启用分页 + 切栈跳转到高地址的 `kmain`
- **`prot_init()`**：建立保护结构（异常向量、特权级、内核栈），使 CPU 能正确响应异常

两者是**正交**的关注点：分页决定"哪些虚拟地址可访问"，保护结构决定"异常发生时的行为"。Minix3 C 把页表重建放在 `prot_init()` 末尾，是 32-bit 引导流程的历史遗留；64-bit 重写后，页表在 `arch_boot_impl()` 中已经完成，`prot_init()` 专注于保护结构。

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

**逐行分析**——先理解 C 版每个操作的**目的**，再对比 Rust 版的差异：

1. **BSS 检查 `assert(bss_test == 0)`**：为什么需要？因为 C 语言不保证 BSS 段自动清零——这是 bootloader 的责任。如果 bootloader 有 bug（或链接脚本错误导致 BSS 范围计算错误），未清零的 BSS 会让内核假设为 0 的全局变量包含垃圾值，导致不可预期的行为。这个 assert 是**防御性编程**：在启动早期捕获 bootloader 的错误，而不是让垃圾值在后续代码中引发难以调试的故障。Rust 中不需要——Rust 的 `static` 变量由语言保证零初始化，链接脚本也由编译器生成，不会出现范围计算错误。

2. **memcpy(&kinfo, local_cbi, sizeof(kinfo))**：为什么需要？`local_cbi` 是 `kmain` 的参数，位于调用者的栈帧中。`kmain` 调用 `cstart()`，`cstart()` 再调用 `proc_init()` 等一系列函数，原调用者的栈帧最终会被覆盖。如果不把启动信息拷贝到全局变量 `kinfo`，后续代码访问 `local_cbi` 时读到的是被覆盖后的垃圾值。Rust 中不需要——`arch_boot_impl` 返回的 `&KernelInfo` 指向 boot-shim 在静态存储区构造的 `KernelInfo`，生命周期贯穿整个内核运行期，不存在栈帧被覆盖的问题。

3. **kernel_may_alloc = 1**：为什么需要？在 VM（虚拟内存管理器）启动之前，内核没有专门的内存分配服务。`kernel_may_alloc` 是一个全局标志，告诉内核代码"现在可以安全地调用物理内存分配器了"。在 VM 启动前（此标志为 0），任何内存分配请求都应被拒绝或 panic——因为分配器可能尚未初始化。这个标志的本质是**启动阶段的权限闸门**。Rust 版中可以用更类型安全的方式表达（如枚举 `BootPhase::Early` / `BootPhase::MayAlloc`），但语义相同。

4. **cstart()**：进入保护模式初始化及后续启动流程。cstart 内部依次调用 prot_init、init_clock、intr_init、arch_init，并解析环境变量。这是内核从"被引导的程序"转变为"操作系统"的关键转折点，从此刻起，内核有了自己的保护结构、时钟、中断控制。

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

1. **prot_init()**：建立 GDT/IDT/TSS（x86-64）或异常向量（aarch64/riscv64）。必须在所有其他初始化之前完成——因为没有保护结构，任何异常都会 triple fault。

2. **init_clock()**：初始化时钟源。依赖 prot_init() 建立的中断描述符——时钟中断需要 IDT 中的门描述符。

3. **intr_init(0)**：初始化中断控制器（8259A/APIC）。参数 0 表示"boot 阶段"。依赖 prot_init() 和 init_clock()。

4. **arch_init()**：架构特定的额外初始化。依赖前三步完成。

### 2.3 prot_init() 详解：x86

`protect.c:321-367`（i386 版，64 位语义相同但使用 64 位描述符格式，GDTR.base 为 64 位）：

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
  gdt_desc.base = (u32_t) gdt;           // (3) 设置 GDTR（i386 版为 u32_t，x86-64 为 u64_t）
  gdt_desc.limit = sizeof(gdt)-1;
  idt_desc.base = (u32_t) idt;           // (4) 设置 IDTR（i386 版为 u32_t，x86-64 为 u64_t）
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

- **步骤 1-5**：清零并设置描述符表指针。这是"准备阶段"。
- **步骤 6-10**：填充 GDT 段描述符。64 位模式下，代码段和数据段都是 flat（base=0, limit=full），但 DPL（Descriptor Privilege Level）不同：内核段 DPL=0，用户段 DPL=3。
- **步骤 11**：`prot_load_selectors()` 执行 `lgdt`（加载 GDTR）、`idt_init()`（填充 IDT 门描述符）、`idt_reload()`（加载 IDTR，即 `lidt`）、`lldt`（加载 LDTR）、`ltr`（加载 TR）、重载所有段寄存器（CS/DS/ES/FS/GS/SS）。这是"生效阶段"——从此 CPU 使用我们自己的 GDT 和 IDT。Rust 版将 IDTR 加载推迟到 `set_handler()` 之后，但 GDT/TSS 仍在本阶段生效。

#### 2.3.1 idt_init() 分析

`idt_init()`（protect.c:260-264）负责填充 IDT（Interrupt Descriptor Table），是异常向量表的核心初始化逻辑：

```c
static void idt_init(void)
{
  idt_copy_vectors_pic();                              // (a) 填充 PIC 中断向量
  idt_copy_vectors(gate_table_exceptions,               // (b) 填充 CPU 异常向量
    sizeof(gate_table_exceptions) / sizeof(gate_table[0]));
}
```

**gate_table 数据结构**：每个 `gate_table` 条目定义一个 IDT 门描述符的配置：

| 字段 | 含义 | 示例 |
|------|------|------|
| `vector` | IDT 向量号 | 0（除零错误）、14（缺页） |
| `handler` | 处理函数地址 | `divide_error`、`page_fault` |
| `dpl` | 描述符特权级 | 0（内核）或 3（用户可触发） |
| `ist` | Interrupt Stack Table 索引 | 0（不使用 IST）、2（DF 使用 IST2） |

**gate_table_pic[]**（protect.c:107-125）：PIC 中断向量，通过 `VECTOR(irq)` 宏映射为 `0x50-0x57`（IRQ0-7，即 vector 80-87）和 `0x70-0x77`（IRQ8-15，即 vector 112-119），DPL=0（内核特权级，与 C 中 `INTR_PRIVILEGE` 一致）。
**gate_table_exceptions[]**（protect.c:127-152）：CPU 异常向量，如除零（vector 0, DPL=0）、断点（vector 3, DPL=3）、缺页（vector 14, DPL=0）等。

在 Rust 版中，`X86_64TrapEntry::init()` 完成相同功能：用 `set_gate()` 填充 IDT 条目，DPL 和 IST 配置与 C 版 `gate_table` 一致。x86-64 版额外补全了 C `gate_table_exceptions[]` 中的向量 16-19（`#MF/#AC/#MC/#XM`）和 34-35（`KERN_CALL_VECTOR_UM / IPC_VECTOR_UM`），以及 `gate_table_pic[]` 对应的 80-87、112-119 硬件中断向量；向量 9 和 15 在 64-bit 长模式下保留，故意省略。

> **注意：handler 地址当前为 placeholder 0**。`X86_64TrapEntry::init()` 目前只配置 DPL/IST/门类型等元数据，所有 handler 地址填 `0`（代码注释见 `os/arch/src/x86_64/trap_entry.rs:151`）。实际异常处理函数地址需要在 [13-exception-interrupt.md](13-exception-interrupt.md) 阶段通过 `set_handler()` 填入，然后才能调用 `load()` 让 IDT 真正生效。本文档聚焦保护结构的"蓝图"准备，不处理异常分发。

- **步骤 12-15**：重建页表。为什么需要重建？因为 `pre_init()` 在低地址建立了页表，而 `prot_init()` 在高地址运行。重建确保页表结构在高地址也可访问。**在 Rust 版中，这一步已在 `arch_boot_impl()` 中完成，不需要重复**——详见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.4。

### 2.4 prot_init() 详解：aarch64

> **注意**：Minix3 的 ARM 版本（earm）是 32 位，代码比 64 位简化。以下分析基于 32 位 ARM C 代码，Rust 版已扩展为 64 位（aarch64）语义。

`earm/protect.c:77-93`（远短于 x86-64）：

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

ARM64 的 `prot_init()` 比 x86 简单得多——因为 ARM64 没有段描述符/GDT 机制。ARM64 的特权级切换由硬件自动处理（异常发生时自动切换 EL0→EL1），不需要软件设置描述符表。

硬件操作只有两个：
1. **设置 VBAR_EL1**：`write_vbar(&exc_vector_table)`——指向异常向量表基址。异常向量表定义了不同类型异常（SVC/IRQ/FIQ/SError）在不同特权级下的入口地址。
2. **设置 SP_EL1**（x86 的 `tss_init` 等价）：ARM64 C 代码中 `tss_init()` 设置 `SP_EL1` 为内核栈顶。当异常从 EL0 进入 EL1 时，CPU 自动从 SP_EL0 切换到 SP_EL1。在 Rust 版中，`AArch64Protection::init()` 通过 `msr sp_el1` 完成相同操作。

### 2.5 prot_init() 详解：riscv64

Minix3 没有 RISC-V 版本，但 RISC-V 的 `prot_init()` 语义可以从架构规范推导：

1. **设置 stvec**：`csrw stvec, trap_vector_base`——指向 trap 向量基址
2. **设置 sscratch**：`csrw sscratch, kernel_stack_top`——保存内核栈顶。U-mode→S-mode 时 trap handler 用 `csrrw sp, sscratch, sp` 把 `sp` 换成内核栈顶，同时把原用户栈指针存入 `sscratch`；sret 返回前再交换回来。
3. **设置 sstatus**：确保 SPP（Supervisor Previous Privilege）位正确
4. **重建页表**：同 x86/ARM

RISC-V 的特权级切换比 x86 简单：trap 发生时，硬件自动将 PC 保存到 sepc，将特权级保存到 sstatus.SPP，然后跳转到 stvec 指向的地址。不需要 GDT/IDT。

**参考规范**:
- RISC-V *Privileged Architecture Manual* §4.1.5 (stvec) — 决定 trap 入口地址
- RISC-V *Privileged Architecture Manual* §4.1.6 (sscratch) — U-mode → S-mode 切换时临时寄存器
- RISC-V *Privileged Architecture Manual* §4.1.7 (sepc) — trap 时保存 PC
- RISC-V *Privileged Architecture Manual* §4.1.2 (sstatus) — SPP/SUM/MXR 等控制位

---

## 3. Rust 设计决策

### 3.1 决策：`kmain` 不做 memcpy(&kinfo)

`arch_boot_impl()` 返回 `&'static KernelInfo`，此引用指向 boot-shim 构造的静态 `KernelInfo`，生命周期贯穿整个内核运行期。Rust 的借用规则保证引用安全，memcpy 冗余。

### 3.2 决策：不做 BSS 检查

Rust 的 `static` 变量由语言保证零初始化；boot-shim 的 `load_segments_into_buffer()` 也清零 BSS 段。若 BSS 未清零，Rust 的 `Option<T>` 等类型会产生 UB，Rust 类型系统在编译时排除此类风险。

### 3.3 决策：`cstart` 拆分为两个独立函数

`init_protection()` 对应本文，`init_clock_and_interrupts()` 对应 04 文档。拆分原因：

1. **保护结构加载 vs 中断可用** 是两个不同的"启动里程碑"：`init_protection()` 之后 GDT/TSS/SYSCALL-MSR 生效，CPU 可安全进行系统调用；IDT 需等到 `set_handler()` 完成后才真正加载，因此异常响应在 13 文档阶段才完全可用。`init_clock_and_interrupts()` 之后 CPU 可响应硬件中断。
2. **文档对应**：每个函数对应一个阶段，方便分散到不同文档

### 3.4 决策：GDT 描述符用 bitflags + 强类型

C 用裸 `u32` 数组 + 宏位运算；Rust 用 `bitflags!` 宏。`PRESENT | DPL_RING3 | CODE | READABLE = 0xFA` 比 C 版的 `0xFA` 表达力更强，且类型不会与 `u8` 混淆。

### 3.5 决策：`prot_init` 不重建页表

minix-rs 在 `arch_boot_impl()` 已建立恒等映射 + 内核高地址映射，`prot_init()` 专注于保护结构。此处理与 C 版的语义等价，区别仅在于页表重建的执行时机。

### 3.6 决策：抽象为两个 trait（`ProtectionArch` + `TrapEntryArch`）

| 维度 | 单一 trait + cfg | 两个 trait |
|------|----------------|-----------|
| 加载顺序 | 文档中说明"先 load prot 后 load trap" | 类型层面强制，`init_protection` 函数显式调用两者 |
| 实现数量 | trait body 内堆 cfg 分支 | 每个 trait 的实现都是单一职责 |
| 单元测试 | 测整个 trait 较复杂 | 单独测 `ProtectionArch::load()` 和 `TrapEntryArch::load()` |
| 跨架构共性 | 共性被 cfg 淹没 | `ProtectionArch` 的接口对所有架构表达"内核栈 + 特权级"，跨架构一致性更清晰 |

### 3.7 架构差异对照

| 概念 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 异常向量基址 | IDT（GDTR 指向）+ lidt | VBAR_EL1 | stvec |
| 特权级 | Ring 0/3（段描述符 DPL） | EL1/EL0 | S-mode/U-mode |
| 特权切换指令 | SYSCALL/SYSRET（MSR） | SVC/ERET | ecall/sret |
| 内核栈指针 | TSS.sp0（硬件自动切换） | SP_EL1（异常时硬件切换） | sscratch（U→S 时 handler 用 `csrrw` 交换 sp↔sscratch） |
| 特权级抽象 | GDT 段描述符 DPL | 系统寄存器 | CSR |

### 3.8 决策：`kmain` 不设置 `board_id`

C 版 `kmain()` 在 `main.c:130` 设置 `machine.board_id = get_board_id_by_name(...)`，用于 ARM BSP（TI OMAP）的板级相关初始化：时钟、串口、复位、中断控制器等都需要根据板型（BeagleBoard/BeagleBoard XM）分支。

Rust 版目前未移植 ARM BSP 的板级分支代码：
- `KernelInfo` 中不存在 `board_id` 字段；
- x86-64 的 `arch_init()` 直接初始化 COM1 串口、ACPI、APIC，无需板型识别；
- 若未来支持 ARM BSP，板型识别应放在 `arch_init()` 或 `plat` crate 中完成，而不是在 `kmain()` 这一架构无关入口里。

因此 `kmain()` 省略 `board_id` 设置，不是功能缺失，而是把板级相关逻辑下放到架构初始化层。

### 3.9 决策：`kmain` 不拷贝 `boot_procs`

C 版 `kmain()` 在 `main.c:140-141` 执行 `memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs))`：
- `image[]` 是编译期静态表（`table.c`），描述内核任务和用户态 boot modules；
- C 的 `kinfo` 是可变全局结构体，需要在运行期把 `image[]` 拷贝进去，供后续 `proc_init()` 使用。

Rust 版中，`boot-shim` 已经把 boot modules 信息构造进 `KernelInfo.boot_modules`，`kmain` 拿到的是不可变引用。`init_proc_and_boot()` 直接读取 `kernel_info.boot_modules` 创建进程表项，无需也不应再拷贝一次。这等价于把 C 的"静态表 → 全局 kinfo → proc_init"三条链路压缩为"boot-shim → KernelInfo → init_proc_and_boot"两条链路。

### 3.10 决策：ARM 串口初始化不在 `kmain` 中

C 版 `kmain()` 在 `main.c:132-134` 通过 `#ifdef __arm__ arch_ser_init();` 做 ARM 早期串口初始化，用于 BSP 板级调试输出。

Rust 版把串口初始化统一到 `arch_init()`：
- x86-64：`os/arch/src/x86_64/arch_init.rs` 的 `ser_init()` 初始化 COM1；
- aarch64/riscv64：若需要早期串口，应在各自 `arch_init()` 中完成。

这样 `kmain()` 保持架构无关，所有架构相关的早期设备初始化集中到 `arch_init()`，与 `04-clock-interrupt-init.md` 的职责边界一致。

### 3.11 决策：用类型系统替代 `prot_init_done` 运行时标志

C 版 `prot_init()` 最后设置 `prot_init_done = 1`，用于向后续代码（尤其是 AP 初始化）声明"保护结构已就绪"。

Rust 版中，`ProtectionArch::init()` 返回一个具体的结构体实例，调用者必须持有该实例才能调用 `load()`。这种设计把"是否已初始化"从运行时标志转化为类型状态：
- 没有实例 → 无法调用 `load()`；
- 已调用 `load()` → 保护结构生效。

SMP 阶段 AP 初始化同样遵循该模式：`init_ap(cpu_id, stack_top)` 返回 AP 的保护结构实例，再调用 `load()`。不需要额外的 `prot_init_done` 标志。

---

## 4. 实现详解

Rust 版将 `prot_init()` 拆分为两个 trait 抽象：`ProtectionArch` 负责**特权级隔离与内核栈设置**，`TrapEntryArch` 负责**异常/中断/系统调用入口**。这两个 trait 的拆分依据是 C 版 `prot_init()` 内部的两个独立职责——`tss_init()`（保护结构）和 `idt_init()`（异常向量）。

**`ProtectionArch` 的抽象语义**：

> 回答"用户态不能碰哪里"和"进程切换怎么安全转场"。

- `PrivilegeLevel` 关联类型：封装架构特有的特权级表示（x86-64 Ring、ARM64 EL、RISC-V mode），避免上层代码直接接触硬件编码。
- `init()`：建立保护结构的"蓝图"——x86-64 填充 GDT 描述符和 TSS，aarch64 设置 SP_EL1，riscv64 准备 sscratch。
- `set_kernel_stack()`：配置特权级切换时的目标内核栈。这是安全转场的核心——用户态触发异常或系统调用时，CPU 必须知道切换到哪个栈，否则会继续使用用户态栈（已映射但不可信）。
- `load()`：将蓝图写入硬件寄存器（`lgdt`/`ltr`、`msr` 等），从此刻起保护结构生效。

**`TrapEntryArch` 的抽象语义**：

> 回答"异常来了去哪里"。

- `init()`：填充异常向量表——CPU 异常（除零、页错误等）、硬件中断（PIC/IOAPIC）、系统调用入口。
- `configure_syscall()`：配置系统调用机制。x86-64 需要写入 MSR（LSTAR/SFMASK），aarch64/riscv64 使用异常向量中的统一入口，无需额外配置。
- `load()`：将向量表基址写入硬件寄存器（`lidt`、`msr VBAR_EL1`、`csrw stvec`），从此刻起异常和中断有去向。**本文阶段不调用 `trap.load()`**，因为 handler 地址仍为 0；真正的加载在 [13-exception-interrupt.md](13-exception-interrupt.md) 阶段 `set_handler()` 之后。

两个 trait 的顺序不可交换：最终 `ProtectionArch::load()` 必须在 `TrapEntryArch::load()` 之前——因为异常处理函数运行在内核态，需要有效的特权级和栈设置。如果先加载 IDT 后加载 GDT，第一个异常就会因为段选择子无效而 triple fault。

### 4.1 init_protection() 的实现

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

> **运行时更新内核栈**: 上面 `ProtectionArch::init(0, kern_stack_top)` 只在 **boot 阶段**写入 BSP（CPU 0）的内核栈。**进程调度时切换到新进程的内核栈**则通过 `ProtectionArch::set_kernel_stack(cpu_id, new_stack_top)` 单独完成——x86-64 写 `TSS.sp0`，aarch64 写 `SP_EL0`/`sscratch`，riscv64 写 `sscratch`。SMP 阶段新增 AP 初始化时也通过 `init_ap(cpu_id, stack_top)` + `set_kernel_stack()` 双步完成。
> 
> **§5.3 测试覆盖核对**:
> - x86_64: `protection.rs` 15 个 + `trap_entry.rs` 18 个 = **33 个** ✓ 与 §5.3 列表一致
> - aarch64: `protection.rs` 7 个 + `trap_entry.rs` 3 个 = **10 个** ✓ 与 §5.3 列表一致
> - riscv64: `protection.rs` 5 个 + `trap_entry.rs` 3 个 = **8 个** ✓ 与 §5.3 列表一致
> - `set_handler_is_noop` 测试在 aarch64/riscv64 `trap_entry.rs:111/108` 实际存在 ✓

**代码与概念的对应**：

| 代码 | §1.5 的概念问题 | 架构差异点 |
|------|--------------|----------|
| `CurrentProtection::init(0, kern_stack_top)` | "内核栈顶放哪里" | x86-64: TSS.sp0；aarch64: SP_EL1；riscv64: sscratch |
| `prot.load()` | "让特权级契约生效" | x86-64: GDTR + ltr；aarch64: isb；riscv64: 无显式 load（CSR 立即生效） |
| `CurrentTrapEntry::init()` | "异常向量表里有什么" | x86-64: IDT 门描述符（handler 地址暂为 0，仅元数据）；aarch64/riscv64: 异常向量表由汇编定义，软件只配置入口 |
| `trap.configure_syscall(syscall_entry)` | "系统调用走哪个入口" | x86-64: LSTAR MSR；aarch64/riscv64: 统一异常入口，无需配置 |
| `trap.load()`（本文不调用） | "让异常向量生效" | x86-64: IDTR；aarch64: VBAR_EL1 + isb；riscv64: stvec；推迟到 `set_handler()` 之后 |

### 4.2 ProtectionArch trait 抽象

trait 定义回答"用户态不能访问的内存范围 + 进程切换时栈的安全转场"：

```rust
pub trait ProtectionArch: Sized {
    type PrivilegeLevel: Copy + Eq + Debug;

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

> **细节**：`PrivilegeLevel` 是 trait associated type，bound 为 `Copy + Eq + Debug`，封装架构特有的特权级表示（x86-64 `Ring(0/3)` / aarch64 `EL(0/1)` / riscv64 `Mode(S/U)`），让上层代码只接触 OS 概念（`Privilege::Kernel/User`），不直接接触硬件编码。
>
> **关于 `init` / `init_ap` / `load` 的语义分离**：
> - `init()`：建立保护结构的"蓝图"，在内存中准备好 GDT 描述符、TSS、异常向量表
> - `load()`：把蓝图写入硬件寄存器（`lgdt`/`ltr`、`msr`、`csrw`），使契约生效
> - `init_ap()`：AP（应用处理器）启动时的初始化，与 BSP 的 `init()` 共享大部分逻辑但有少量差异（如 AP 不需要 `lgdt` 全局同步）

### 4.3 TrapEntryArch trait 抽象

trait 定义回答"异常来了去哪里"：

```rust
pub trait TrapEntryArch: Sized {
    fn init() -> Self;
    fn configure_syscall(&mut self, entry_point: VirBytes);
    fn load(&self);
    fn load_ap(&self);
    fn set_handler(&mut self, vector: InterruptVector, handler: VirBytes, user_accessible: bool);
}
```

> **为什么 `configure_syscall` 是独立方法**：x86-64 的系统调用入口由 MSR 配置（LSTAR MSR），与 IDT 中的异常入口完全独立；aarch64/riscv64 的系统调用走统一异常入口（SVC/ecall），无需额外配置。将系统调用入口作为独立方法，使架构差异体现在方法体内，调用者无需 `#[cfg]`。
>
> **为什么 `set_handler` 用 `InterruptVector` 枚举**：上层代码传 OS 概念（"时钟中断"、"页错误"），底层实现映射到架构特有的向量号。OS 概念与硬件编码完全解耦。

> **为什么 aarch64/riscv64 的 `set_handler` 是 no-op**：ARM64 使用固定异常向量表（VBAR_EL1 指向汇编定义的 16 个入口），RISC-V 使用 Direct 模式（stvec 指向统一入口）。这两种架构的中断分发在汇编层面完成，具体 handler 的路由由软件分发器在运行时完成，不需要像 x86-64 那样在 IDT 中动态修改门描述符。因此 `set_handler` 在 ARM64/RISC-V 上是空操作——硬件向量表在 `load()` 时一次性设置完毕。

### 4.4 三架构的"概念 → 代码"映射

下表是概念到代码的"存在性证明"：读者无需细读代码即可验证 §1.5 的概念问题被每个架构正确回答。

| 概念问题 | x86-64 | aarch64 | riscv64 |
|---------|--------|---------|---------|
| 内核栈顶存放 | `TSS.sp0 = kern_stack_top - X86_64_STACK_TOP_RESERVED`，并保留顶部 16 字节存放进程指针与 CPU id | `msr sp_el1, kern_stack_top` | `csrw sscratch, kern_stack_top` |
| 特权级生效 | `lgdt gdt_desc` + `ltr TSS_SEL` | `isb`（SP_EL1 写入后同步） | 无显式 load（CSR 写入即生效） |
| 异常向量表内容 | IDT 256 个门描述符（handler 地址暂为 0，仅元数据） | 异常向量表（汇编定义） | trap 向量（汇编定义） |
| 异常向量表生效 | `lidt idt_desc`（本文阶段不执行，推迟到 `set_handler()` 后） | `msr vbar_el1, &exc_vector_table` + `isb` | `csrw stvec, &trap_vector` |
| 系统调用入口 | `wrmsr MSR_LSTAR, syscall_entry` | 走 SVC 异常入口（无需配置） | 走 ecall 异常入口（无需配置） |
| 用户态陷入内核栈切换 | CPU 硬件自动用 TSS.sp0 | 异常时硬件自动用 SP_EL1 | U→S 时 `sscratch` 存内核栈顶，handler 用 `csrrw` 交换 sp↔sscratch（sp=内核栈，sscratch=用户栈） |

> **代码不在文档中展开**：完整实现在 `os/arch/src/{x86_64,aarch64,riscv64}/{protection,trap_entry}.rs`。文档列出代码作为概念存在性证明，足以让读者理解 §1.5 的概念问题如何被回答；完整代码细节（位编码、寄存器顺序、barrier 类型）属于实现层，不在概念文档展开。

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

---

## 5. 测试要点

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

| 测试内核 | 架构 | 验证项 | 对应 §1.2 |
|---------|------|--------|----------|
| `test-protection` | x86_64 | GDT 已加载（GDTR.base != 0）、IDT 已加载（IDTR.base != 0）、TSS 已加载（TR != 0）、CS/DS RPL=0 | 全部三个 |
| `test-protection-aarch64` | aarch64 | CurrentEL=EL1、VBAR_EL1 非零、DAIF 全屏蔽、SPSel 可切换、SP_EL1 读写验证、kern_stack_top 在内核 VA 范围 | 异常向量 + 特权级 |
| `test-protection-riscv64` | riscv64 | stvec 已设置（Direct 模式）、sscratch=kern_stack_top、sstatus 为 S-mode | 全部三个 |

> **aarch64 的 SP_EL1 写入与栈保护**：经 QEMU 实测验证，`mrs HCR_EL2` 从 EL1 触发异常（HCR_EL2 不可从 EL1 直接读取），但 SP_EL1 的访问不受影响——`msr SP_EL1` 不触发异常，证明 HCR_EL2.TSP=0。此前观察到的 `msr SP_EL1` 崩溃并非 EL2 陷出导致，而是因为当 `SPSel=1`（默认值）时，SP_EL1 就是当前栈指针——写入 SP_EL1 会立即改变当前 SP，导致后续栈操作访问无效地址。修复方案：在 `msr SP_EL1` 前保存当前 SP 到通用寄存器，写入后立即用 `mov sp, saved_sp` 恢复。aarch64 测试现已包含 SP_EL1 读写验证（写入测试值后读回比对）。

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

| 测试 | 验证的概念问题 | 对应 §1.2 |
|------|--------------|----------|
| `tss64_size_is_104_bytes` | TSS 结构体布局符合 Intel SDM | 内核栈指针 |
| `tss64_offsets_correct` | TSS.sp0/IST/iobase 偏移正确 | 内核栈指针 |
| `segment_selectors_correct` | CS/DS 选择子值（0x08/0x10/0x1B/0x23）正确 | 特权级隔离 |
| `privilege_level_roundtrip` | Ring0↔Kernel, Ring3↔User 转换正确 | 特权级隔离 |
| `idt_entry64_size_is_16_bytes` | IDT 门描述符布局符合 Intel SDM | 异常向量 |
| `idt_ptr_size_is_10_bytes` | IDT 指针（limit+base）布局正确 | 异常向量 |
| `star_register_value_correct` | STAR MSR 中内核/用户段选择子位置正确 | 特权级隔离 |
| `gdt_descriptors_have_correct_dpl` | GDT 描述符 DPL 字段（内核段=0，用户段=3） | 特权级隔离 |
| `gdt_descriptors_are_flat_mode` | 64-bit code (L=1), page granularity (G=1) | 特权级隔离 |
| `set_kernel_stack_updates_sp0` | `set_kernel_stack()` 写入 TSS.sp0 语义 | 内核栈指针 |
| `set_kernel_stack_panics_on_invalid_cpu_id` | cpu_id 越界检查 | 内核栈指针 |
| `tss_descriptor_is_64bit` | TSS 描述符类型=64-bit TSS available | 内核栈指针 |
| `init_fills_gdt_correctly` | `init()` 填充 GDT 描述符（access byte, L bit） | 特权级隔离 |
| `init_sets_tss_sp0_below_reserved_area` | `init()` 设置 TSS.sp0 = kernel_stack_top - X86_64_STACK_TOP_RESERVED | 内核栈指针 |
| `init_sets_cpu_count` | `init()` 设置 cpu_count = cpu_id + 1 | 内核栈指针 |
| `init_creates_tss_descriptor_in_gdt` | `init()` 在 GDT 中创建 TSS 描述符 | 内核栈指针 |
| `gdt_null_entry_is_zero` | GDT[0] = 0（null descriptor） | 特权级隔离 |
| `tss_iobase_disables_io_bitmap` | TSS.iobase = 0x8000 禁用 I/O bitmap | 内核栈指针 |
| `set_handler_sets_dpl_correctly` | `set_handler()` DPL=3/0 正确设置 | 异常向量 |
| `set_handler_writes_handler_address` | `set_handler()` 地址正确拆分到 IDT 字段 | 异常向量 |
| `gate_type_constants_correct` | 中断门=0xE, 陷阱门=0xF, Present=0x80 | 异常向量 |
| `msr_constants_correct` | STAR/LSTAR/SFMASK/EFER MSR 地址正确 | 异常向量 |
| `star_register_layout` | STAR SYSCALL/SYSRET CS/SS 选择子正确 | 特权级隔离 |
| `sfmask_clears_if_on_syscall` | SFMASK 仅清除 IF (bit 9) | 特权级隔离 |
| `idt_init_sets_exception_gates` | `init()` 设置异常/IRQ 门描述符 | 异常向量 |
| `idt_init_breakpoint_has_dpl3` | INT3 (vector 3) DPL=3 | 异常向量 |
| `idt_init_overflow_has_dpl3` | INTO (vector 4) DPL=3 | 异常向量 |
| `idt_init_double_fault_uses_ist2` | Double fault (vector 8) IST=2 | 异常向量 |
| `idt_init_nmi_uses_ist1` | NMI (vector 2) IST=1 | 异常向量 |
| `idt_init_kernel_exceptions_have_dpl0` | 内核异常 DPL=0 | 异常向量 |
| `idt_init_reserved_vectors_not_present` | x86-64 保留向量 9/15 不设置门描述符 | 异常向量 |
| `idt_init_sets_syscall_ipc_vectors` | 向量 32-35（系统调用/IPC soft-int）已设置且 DPL=3 | 异常向量 |
| `idt_init_sets_pic_vectors` | 向量 80-87、112-119（PIC 硬件中断）已设置且 DPL=0 | 异常向量 |

#### ARM64（`os/arch/src/arm64/{protection,trap_entry}.rs`）

| 测试 | 验证的概念问题 | 对应 §1.2 |
|------|--------------|----------|
| `privilege_level_values` | EL1=1, EL0=0 | 特权级隔离 |
| `privilege_level_roundtrip` | EL1↔Kernel, EL0↔User 转换正确 | 特权级隔离 |
| `kernel_privilege_is_el1` | KERNEL_PRIVILEGE = EL1 | 特权级隔离 |
| `user_privilege_is_el0` | USER_PRIVILEGE = EL0 | 特权级隔离 |
| `el1_maps_to_kernel` | EL1 → Privilege::Kernel | 特权级隔离 |
| `el0_maps_to_user` | EL0 → Privilege::User | 特权级隔离 |
| `protection_has_cpu_count` | AArch64Protection 结构体可构造 | 内核栈指针 |
| `trap_entry_init_returns_unit_struct` | AArch64TrapEntry::init() 不 panic | 异常向量 |
| `configure_syscall_is_noop` | ARM64 SVC 无需 MSR 配置 | 异常向量 |
| `set_handler_is_noop` | ARM64 固定向量表，set_handler 为 no-op | 异常向量 |

#### RISC-V（`os/arch/src/riscv64/{protection,trap_entry}.rs`）

| 测试 | 验证的概念问题 | 对应 §1.2 |
|------|--------------|----------|
| `privilege_level_values` | S_MODE=1, U_MODE=0 | 特权级隔离 |
| `privilege_level_roundtrip` | S_MODE↔Kernel, U_MODE↔User 转换正确 | 特权级隔离 |
| `kernel_privilege_is_s_mode` | KERNEL_PRIVILEGE = S_MODE | 特权级隔离 |
| `user_privilege_is_u_mode` | USER_PRIVILEGE = U_MODE | 特权级隔离 |
| `protection_has_cpu_count` | Riscv64Protection 结构体可构造 | 内核栈指针 |
| `trap_entry_init_returns_unit_struct` | Riscv64TrapEntry::init() 不 panic | 异常向量 |
| `configure_syscall_is_noop` | RISC-V ecall 无需 CSR 配置 | 异常向量 |
| `set_handler_is_noop` | RISC-V Direct 模式，set_handler 为 no-op | 异常向量 |

### 5.4 测试缺口

对照 §1.2 的三个概念问题和 §4 的实现，以下场景尚无测试覆盖：

| 缺口 | 对应概念问题 | 优先级 | 说明 |
|------|------------|--------|------|
| init/load 顺序约束 | 全部三个概念问题 | P1 | §4.1 强调 ProtectionArch::load() 必须先于 TrapEntryArch::load()，无测试验证违反顺序的后果 |
| `init_ap` 路径验证 | 用户态陷入内核时使用的栈 | P1 | AP 启动路径完全未测试（需要 SMP 硬件/模拟） |

> **说明**：上述两项均依赖 SMP 多核支持，将在 SMP 阶段补充。

---

## 6. 过渡：从 init_protection() 到 init_clock_and_interrupts()

`init_protection()` 完成后，CPU 已具备正确的特权级、内核栈切换能力（x86-64 TSS、aarch64 SP_EL1、riscv64 sscratch）以及系统调用入口（x86-64 LSTAR MSR），但异常向量表（IDT/VBAR_EL1/stvec）尚未真正加载到硬件，中断控制器也尚未初始化，时钟尚未启动。

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
