# 03-kmain-entry-protection: kmain 入口与保护模式初始化

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/main.c:115-147`, `minix3/minix/kernel/main.c:403-481`, `minix3/minix/kernel/arch/i386/protect.c:321-367`, `minix3/minix/kernel/arch/earm/protect.c:77-93`
> **说明**: 从 kmain 被调用到 prot_init() 完成——内核建立保护模式基础设施
> **前置**: [02-higher-half-kernel.md](02-higher-half-kernel.md) — trampoline 已将 CPU 切换到高地址

---

## 1. 概述

### 1.0 保护结构：CPU 与 OS 之间的契约

03 文档的主题是 `prot_init()`——但它远不止"设置几个寄存器"。从 OS 理论的角度看，保护结构回答的是 CPU 向内核提出的三个根本问题：

> **异常来了去哪里？** → 异常向量表（IDT/VBAR_EL1/stvec）  
> **用户态不能碰哪里？** → 页表权限位 + 段描述符（supervisor/user 隔离）  
> **进程切换怎么安全转场？** → TSS / 栈指针切换 / 上下文保存机制

这三个问题的答案构成了 CPU 与操作系统之间的**契约**。操作系统填写保护结构，告诉 CPU："当异常发生时，这是我的处理函数；当用户进程运行时，它不能碰我的内存。" 填写保护结构 = 内核**宣告自己对 CPU 的控制权**。

**为什么不能用固件留下的保护结构？**

内核在进入 `kmain()` 时，CPU 确实有正在生效的保护结构：
- x86-64：GRUB/UEFI 的 GDT 和 IDT
- aarch64/riscv64：UEFI/OpenSBI 的异常向量

但这些固件设置对于操作系统来说是不可信赖的，原因有三：

1. **生命周期**：UEFI 的 GDT 和 IDT 在 UEFI 分配的内存中。`ExitBootServices()` 后，这部分内存可能被内核作为空闲物理内存回收——如果内核分配器恰好分配了 GDT 所在的页面，下一个异常就会 triple fault。
2. **语义不匹配**：UEFI 的 GDT 是为运行 PE32+ 程序设计的，基本只包含代码段和数据段的选择子。Minix3 内核需要 TSS（任务状态段）来支持用户态到内核态的栈切换——UEFI 的 GDT 中没有 TSS 描述符。
3. **架构差异**：x86 用 GDT + IDT + TSS 三层结构，ARM/RISC-V 用 VBAR_EL1/stvec 单层结构。prot_init() 必须为每种架构建立其特有的一套机制。

**prot_init() 的哲学意义**：内核在此时刻之前一直"借用"他人的保护结构——boot-shim 使用 UEFI 的，GRUB 使用 BIOS 的。从 `prot_init()` 开始，内核有了**自己的保护结构**。这是内核从"被引导的程序"变为"操作系统的核心"的标志。

### 1.1 kmain 六阶段总览

02 文档结束时，CPU 已在高地址执行 `kmain()`。从 `kmain()` 入口到内核开始调度第一个用户进程，整个 boot 过程可以划分为六个阶段：

| 阶段 | 标记 | 函数 | 做什么 | 文档 |
|------|------|------|--------|------|
| **A: 入口** | T3 | kmain 入口 | memcpy(&kinfo)、BSS 检查、kernel_may_alloc | **本文** |
| **B: cstart** | T3→T4 | cstart() | prot_init → init_clock → intr_init → arch_init | **本文 + 04** |
| **C: 进程表** | T7→T8 | proc_init + arch_boot_proc | 清空进程表、加载 VM ELF | 05 |
| **D: post-init** | T9→T10 | arch_post_init + memory_init | ptproc=VM、freepdes 分配 | 06 |
| **E: system** | T11 | system_init | 特权表初始化 | 07 |
| **F: finish** | T12 | bsp_finish_booting | 回收 bootstrap、切换用户态 | 07 |

本文覆盖 **阶段 A 和阶段 B 的前半部分**（prot_init）。04 文档覆盖阶段 B 的后半部分（init_clock + intr_init + arch_init）。

### 1.2 为什么 cstart 必须先于 proc_init

`cstart()` 是内核从"裸机状态"进入"有保护的状态"的桥梁。在 `cstart()` 之前：

- **没有 GDT/IDT**（x86-64）：CPU 使用 GRUB/UEFI 留下的描述符表，段选择子可能不正确
- **没有异常向量**（aarch64/riscv64）：VBAR_EL1/stvec 未设置，任何异常都会 triple fault
- **没有时钟**：无法计时，无法调度
- **没有中断控制器**：无法响应硬件事件

`proc_init()` 需要设置进程的段寄存器（x86-64 的 CS/DS/SS）和栈指针，这些操作依赖 GDT 中的段描述符。如果 GDT 未初始化，`proc_init()` 设置的段选择子就是无效的，进程切换时会 GP fault。

因此，**cstart 必须先于 proc_init**——这是 Minix3 C 的顺序，也是 Rust 版必须保持的顺序。

> **层级关系**：`kmain` → `cstart()`（含 `prot_init()` 等多个步骤）→ `proc_init()`。`prot_init()` 是 `cstart()` 内部的**第一步**，而 `cstart()` 整体是 `proc_init()` 的**前置条件**。下文 §1.3 展开 `prot_init()`，§1.1 表格中的阶段 B 后续步骤（clock、interrupt、arch_init）在 04 文档中展开。

### 1.3 prot_init() 做了什么

`prot_init()` 是 `cstart()` 内部的第一个调用，负责建立保护模式的基础设施：

| 架构 | prot_init() 做什么 | 等效硬件操作 |
|------|-------------------|-------------|
| x86-64 | 清零 GDT/IDT → 填充段描述符 → 设置 TSS → lgdt/lidt/ltr → 重载段寄存器 → 重建页表 | GDTR/IDTR/TR/CR3 |
| aarch64 | 设置 VBAR_EL1 指向异常向量表 → 重建页表 | VBAR_EL1/TTBR1 |
| riscv64 | 设置 stvec 指向 trap 向量 → 重建页表 | stvec/satp |

**关键观察**：三种架构的 `prot_init()` 都在最后**重建页表**（`pg_clear → pg_identity → pg_mapkernel → pg_load`）。这是因为 `prot_init()` 运行在高地址，而页表是在低地址（`pre_init` 中）建立的。重建页表确保页表结构在高地址也可访问。

### 1.4 Rust 版与 C 版的差异

| 方面 | Minix3 C | minix-rs |
|------|---------|----------|
| BSS 检查 | `assert(bss_test == 0)` 手动验证 | 不需要——Rust 保证静态变量零初始化 |
| memcpy(&kinfo) | `memcpy(&kinfo, local_cbi, sizeof(kinfo))` | `kinfo` 已在 `arch_boot_impl` 中通过 `&KernelInfo` 引用传递 |
| kernel_may_alloc | 全局 `int` 标志 | 编码为 `kmain` 的阶段状态 |
| GDT/IDT 数据结构 | 裸 `u32[]`/`u64[]` + 宏 | `bitflags` + 强类型描述符结构体 |
| prot_init 页表重建 | 内联在 `prot_init()` 中 | 分离到 `arch_boot_impl`（已在 01 中完成） |

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

4. **cstart()**：进入保护模式初始化。这是内核从"被引导的程序"转变为"操作系统"的关键转折点——从此刻起，内核有了自己的保护结构、时钟、中断控制。

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

### 2.3 prot_init() 详解：x86-64

`protect.c:321-367`（x86-32 版，x86-64 版语义相同但使用 64 位描述符格式）：

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

  prot_load_selectors();   // (11) lgdt + lldt + ltr + 重载段寄存器

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
- **步骤 11**：`prot_load_selectors()` 执行 `lgdt`（加载 GDTR）、`lldt`（加载 LDTR）、`ltr`（加载 TR）、重载所有段寄存器（CS/DS/ES/FS/GS/SS）。这是"生效阶段"——从此 CPU 使用我们自己的 GDT。
- **步骤 12-15**：重建页表。为什么需要重建？因为 `pre_init()` 在低地址建立了页表，而 `prot_init()` 在高地址运行。重建确保页表结构在高地址也可访问。**在 Rust 版中，这一步已在 `arch_boot_impl()` 中完成，不需要重复。**

### 2.4 prot_init() 详解：aarch64

`earm/protect.c:77-93`：

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

唯一的硬件操作是设置 `VBAR_EL1`（Vector Base Address Register），指向异常向量表。异常向量表定义了不同类型异常（SVC/IRQ/FIQ/SError）在不同特权级下的入口地址。

### 2.5 prot_init() 详解：riscv64

Minix3 没有 RISC-V 版本，但 RISC-V 的 `prot_init()` 语义可以从架构规范推导：

1. **设置 stvec**：`csrw stvec, trap_vector_base`——指向 trap 向量基址
2. **设置 sscratch**：`csrw sscratch, kernel_stack_top`——保存内核栈顶，用于 U-mode→S-mode 切换
3. **设置 sstatus**：确保 SPP（Supervisor Previous Privilege）位正确
4. **重建页表**：同 x86/ARM

RISC-V 的特权级切换比 x86 简单：trap 发生时，硬件自动将 PC 保存到 sepc，将特权级保存到 sstatus.SPP，然后跳转到 stvec 指向的地址。不需要 GDT/IDT。

---

## 3. Rust 设计决策

### 3.1 决策：kmain 不做 memcpy(&kinfo)

**Minix3 C 的做法**：`kmain` 将 `local_cbi`（栈上的 kinfo_t 拷贝）复制到全局 `kinfo`。

**minix-rs 不做 memcpy**，原因：

1. **`arch_boot_impl` 返回 `&KernelInfo`**：这个引用指向 boot-shim 构造的 `KernelInfo`，它已经在全局静态内存中（boot-shim 的 `KernelInfo` 是 `static` 的）
2. **Rust 的借用规则**：`&KernelInfo` 是不可变引用，不需要拷贝就能安全共享
3. **避免 UB**：C 的 `memcpy` 依赖 `local_cbi` 在栈上有效，而 Rust 的引用保证生命周期安全

**替代方案**：kmain 接收 `&'static KernelInfo`，直接使用，不需要拷贝。

### 3.2 决策：不做 BSS 检查

**Minix3 C 的做法**：`assert(bss_test == 0)` 验证 BSS 段被正确清零。

**minix-rs 不做 BSS 检查**，原因：

1. **Rust 保证零初始化**：`static` 变量在 `.bss` 段中，Rust 链接器保证 `.bss` 段被清零
2. **boot-shim 的 `load_segments_into_buffer`** 已清零 BSS 段（`memzero(bss_start, bss_end - bss_start)`）
3. **如果 BSS 未清零，Rust 的 `Option<T>` 等类型会 UB**——这是比 assert 更严重的错误，但 Rust 的类型系统在编译时排除了这种情况

### 3.3 决策：cstart 分为两个阶段

**Minix3 C 的做法**：`cstart()` 是一个函数，顺序调用 `prot_init → init_clock → intr_init → arch_init`。

**minix-rs 将 cstart 分为两个阶段**：

1. **`init_protection()`**：`ProtectionArch::init()` + `ProtectionArch::load()` + `TrapEntryArch::init()` + `TrapEntryArch::load()`
2. **`init_clock_and_interrupts()`**：时钟初始化 + 中断控制器初始化 + 架构特定初始化

**原因**：

1. **`init_protection()` 是"保护模式生效"**：在 `ProtectionArch::load()` 之前，任何异常都会 triple fault。保护结构加载完成后，内核有了可靠的异常处理能力。
2. **`init_clock_and_interrupts()` 是"中断可用"**：在 `init_clock()` + `intr_init()` 之后，内核可以响应硬件事件。
3. **文档对应**：`init_protection()` 对应本文（03），`init_clock_and_interrupts()` 对应 04 文档。

### 3.4 决策：GDT/IDT 描述符用 bitflags + 强类型

**Minix3 C 的做法**：GDT 描述符是 `u32` 数组，用宏设置位域：

```c
#define PRESENT       0x80
#define DPL0          0x00
#define DPL3          0x60
#define CODE          0x18
#define DATA          0x10
```

**minix-rs 使用 bitflags + 强类型结构体**：

```rust
bitflags::bitflags! {
    pub struct SegmentAccess: u8 {
        const PRESENT    = 1 << 7;
        const DPL_RING0  = 0 << 5;
        const DPL_RING3  = 3 << 5;
        const CODE       = 1 << 3;
        const DATA       = 0 << 3;
        const READABLE   = 1 << 1;
        const WRITABLE   = 1 << 1;
        const ACCESSED   = 1 << 0;
    }
}
```

**原因**：

1. **类型安全**：`SegmentAccess` 是独立类型，不会与 `u8` 混淆
2. **可组合**：`PRESENT | DPL_RING3 | CODE | READABLE` 比 `0xFA` 更清晰
3. **Rust 惯例**：bitflags 是 Rust 生态中处理位域的标准方式

### 3.5 决策：prot_init 不重建页表

**Minix3 C 的做法**：`prot_init()` 末尾调用 `pg_clear → pg_identity → pg_mapkernel → pg_load` 重建页表。

**minix-rs 不在 prot_init 中重建页表**，原因：

1. **已在 `arch_boot_impl()` 中完成**：01 文档中，`arch_boot_impl()` 已经建立了恒等映射和内核高地址映射，并启用了分页
2. **避免重复工作**：C 版重建页表是因为 `pre_init()` 在低地址建立页表，`prot_init()` 在高地址运行时需要确保页表结构可访问。Rust 版的 `arch_boot_impl()` 已经在高地址映射中建立了页表
3. **语义等价**：Rust 版的 `arch_boot_impl()` + `HigherHalf::jump_to_kmain()` 等价于 C 版的 `pre_init()` + `head.S trampoline` + `prot_init()` 中的页表重建

### 3.6 架构差异对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 保护结构 | GDT + IDT + TSS | VBAR_EL1 | stvec + sscratch |
| 特权级 | Ring 0/3（段描述符 DPL） | EL1/EL0（异常级别） | S-mode/U-mode（sstatus.SPP） |
| 特权切换 | SYSCALL/SYSRET（MSR） | SVC/ERET（异常向量） | ecall/sret（trap 向量） |
| 内核栈 | TSS.sp0（硬件自动切换） | SP_EL1（异常时硬件切换） | sscratch（软件交换） |
| 描述符数量 | GDT: 5+NR_CPUS, IDT: 256 | 无描述符表 | 无描述符表 |
| prot_init 核心操作 | lgdt + lidt + ltr | write VBAR_EL1 | csrw stvec + sscratch |

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
- `load()`：将向量表基址写入硬件寄存器（`lidt`、`msr VBAR_EL1`、`csrw stvec`），从此刻起异常和中断有去向。

两个 trait 的顺序不可交换：`ProtectionArch::load()` 必须在 `TrapEntryArch::load()` 之前——因为异常处理函数运行在内核态，需要有效的特权级和栈设置。如果先加载 IDT 后加载 GDT，第一个异常就会因为段选择子无效而 triple fault。

### 4.1 kmain 入口骨架

> 设计决策：§3.1（不做 memcpy）、§3.2（不做 BSS 检查）

```rust
/// Kernel main — called after the higher-half transition.
///
/// This function runs at the kernel's high virtual address.
/// It orchestrates the six-phase boot sequence:
///
/// Phase A (this function): Entry — validate kinfo, allow kernel alloc
/// Phase B: cstart — prot_init + clock + intr + arch_init
/// Phase C: proc_init + arch_boot_proc
/// Phase D: arch_post_init + memory_init
/// Phase E: system_init
/// Phase F: bsp_finish_booting + switch_to_user
///
/// C: main.c:115-147
pub fn kmain(kernel_info: &KernelInfo) -> ! {
    // Phase A: Entry
    // C: memcpy(&kinfo, local_cbi, sizeof(kinfo)) + kernel_may_alloc = 1
    // Rust: no memcpy needed — kernel_info is already a &KernelInfo reference
    // Rust: no BSS check needed — Rust guarantees zero-initialization

    // Phase B: cstart — protection + clock + interrupt
    init_protection(kernel_info);        // prot_init equivalent
    init_clock_and_interrupts();         // clock + intr + arch_init (covered in 04)
    // Phase C: proc_init + arch_boot_proc (covered in 05)
    // Phase D: arch_post_init + memory_init (covered in 06)
    // Phase E: system_init (covered in 07)
    // Phase F: bsp_finish_booting + switch_to_user (covered in 07)

    loop {}
}
```

### 4.2 init_protection()：保护模式初始化

> 设计决策：§3.3（cstart 分阶段）、§3.5（不重建页表）

```rust
/// Phase 1 of cstart: initialize protection structures.
///
/// This must be the very first thing called in kmain, because
/// without valid GDT/IDT (x86-64) or VBAR_EL1/stvec (aarch64/riscv64),
/// any exception will cause an unrecoverable triple fault.
///
/// C: prot_init() — protect.c:321
fn init_protection(kernel_info: &KernelInfo) {
    // Step 1: Initialize protection structures (GDT/TSS on x86-64,
    // VBAR_EL1 on aarch64, stvec/sscratch on riscv64)
    let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);
    prot.load();

    // Step 2: Initialize trap entry table (IDT on x86-64,
    // exception vectors on aarch64, trap vector on riscv64)
    let trap = CurrentTrapEntry::init();
    trap.configure_syscall(kernel_info.syscall_entry);
    trap.load();
}
```

### 4.3 ProtectionArch trait（已有实现）

`ProtectionArch` trait 已在 `os/arch/src/protection.rs` 中定义，包含：

- `init(cpu_id, kernel_stack_top)` — 初始化保护结构
- `set_kernel_stack(cpu_id, stack_top)` — 设置内核栈
- `load()` — 加载到硬件
- `init_ap(cpu_id, stack_top)` — AP 初始化

x86-64 的实现 (`os/arch/src/x86_64/protection.rs`) 已完成，包含 GDT/TSS 管理。

### 4.4 aarch64 ProtectionArch 实现

ARM64 的保护机制比 x86-64 简单——没有 GDT/IDT，只有 VBAR_EL1 和 SP_EL1。

```rust
pub struct AArch64Protection {
    cpu_count: u32,
}

impl ProtectionArch for AArch64Protection {
    type PrivilegeLevel = AArch64PrivilegeLevel;

    const KERNEL_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL1;
    const USER_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL0;

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // Set SP_EL1 to the kernel stack top for exception entry.
        unsafe {
            asm!("msr sp_el1, {}", in(reg) kernel_stack_top.get());
        }
        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        unsafe {
            asm!("msr sp_el1, {}", in(reg) stack_top.get());
        }
    }

    fn load(&self) {
        // VBAR_EL1 is set by TrapEntryArch::load() after the
        // exception vector table is initialized.
        // Instruction Synchronization Barrier ensures all prior
        // system register writes are visible.
        unsafe {
            asm!("isb");
        }
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        unsafe {
            asm!("msr sp_el1, {}", in(reg) kernel_stack_top.get());
        }
    }
}
```

### 4.5 riscv64 ProtectionArch 实现

RISC-V 的保护机制最简单——只有 sscratch（保存内核栈顶）和 sstatus（控制特权级）。

```rust
pub struct Riscv64Protection {
    cpu_count: u32,
}

impl ProtectionArch for Riscv64Protection {
    type PrivilegeLevel = Riscv64PrivilegeLevel;

    const KERNEL_PRIVILEGE: Riscv64PrivilegeLevel = Riscv64PrivilegeLevel::S_MODE;
    const USER_PRIVILEGE: Riscv64PrivilegeLevel = Riscv64PrivilegeLevel::U_MODE;

    fn to_privilege(level: Riscv64PrivilegeLevel) -> Privilege {
        match level {
            Riscv64PrivilegeLevel::S_MODE => Privilege::Kernel,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> Riscv64PrivilegeLevel {
        match privilege {
            Privilege::Kernel => Riscv64PrivilegeLevel::S_MODE,
            Privilege::User => Riscv64PrivilegeLevel::U_MODE,
        }
    }

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // Set sscratch to the kernel stack top.
        // On trap from U-mode, the trap handler assembly code
        // swaps sp and sscratch to obtain the kernel stack pointer.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }
        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        unsafe {
            asm!("csrw sscratch, {}", in(reg) stack_top.get());
        }
    }

    fn load(&self) {
        // sscratch is already set by init().
        // stvec is set by TrapEntryArch::load().
        // CSRs take effect immediately on write.
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }
    }
}
```

### 4.6 aarch64 TrapEntryArch 实现

```rust
pub struct AArch64TrapEntry;

impl TrapEntryArch for AArch64TrapEntry {
    fn init() -> Self {
        // On ARM64, the exception vector table is defined in assembly
        // (exc_vector_table). We just need to set VBAR_EL1 to its address.
        Self
    }

    fn configure_syscall(&mut self, _entry_point: VirBytes) {
        // ARM64 uses SVC instruction which goes through the same
        // exception vector table. No separate configuration needed.
    }

    fn load(&self) {
        extern "C" {
            static exc_vector_table: u8;
        }
        unsafe {
            let vbar = &exc_vector_table as *const u8 as u64;
            asm!("msr vbar_el1, {}", in(reg) vbar);
            // Instruction Synchronization Barrier: ensures VBAR_EL1
            // write is visible to subsequent exception handling.
            asm!("isb");
        }
    }

    fn load_ap(&self) {
        self.load();
    }

    fn set_handler(
        &mut self,
        _vector: InterruptVector,
        _handler: VirBytes,
        _user_accessible: bool,
    ) {
        // ARM64 uses a fixed exception vector table defined in assembly.
        // Dynamic handler registration is done in software by the
        // exception dispatcher, not by modifying the VBAR table.
    }
}
```

### 4.7 riscv64 TrapEntryArch 实现

```rust
pub struct Riscv64TrapEntry;

impl TrapEntryArch for Riscv64TrapEntry {
    fn init() -> Self {
        // On RISC-V, the trap vector is defined in assembly.
        // We just need to set stvec to its address.
        Self
    }

    fn configure_syscall(&mut self, _entry_point: VirBytes) {
        // RISC-V uses ecall instruction which goes through the same
        // trap vector (stvec). No separate configuration needed.
    }

    fn load(&self) {
        extern "C" {
            static trap_vector: u8;
        }
        unsafe {
            let stvec_addr = &trap_vector as *const u8 as usize;
            // Set MODE=Direct (0) and BASE=trap_vector (aligned to 4)
            asm!("csrw stvec, {}", in(reg) stvec_addr);
        }
    }

    fn load_ap(&self) {
        self.load();
    }

    fn set_handler(
        &mut self,
        _vector: InterruptVector,
        _handler: VirBytes,
        _user_accessible: bool,
    ) {
        // RISC-V uses Direct mode (all traps go to stvec BASE).
        // Dynamic handler registration is done in software by the
        // trap dispatcher based on scause, not by modifying stvec.
    }
}
```

### 4.8 CurrentProtection / CurrentTrapEntry 类型别名

在 `os/arch/src/lib.rs` 中添加架构特定的类型别名：

```rust
#[cfg(feature = "x86_64")]
pub type CurrentProtection = crate::x86_64::protection::X86_64Protection;

#[cfg(feature = "arm64")]
pub type CurrentProtection = crate::arm64::protection::AArch64Protection;

#[cfg(feature = "riscv64")]
pub type CurrentProtection = crate::riscv64::protection::Riscv64Protection;

#[cfg(feature = "x86_64")]
pub type CurrentTrapEntry = crate::x86_64::trap_entry::X86_64TrapEntry;

#[cfg(feature = "arm64")]
pub type CurrentTrapEntry = crate::arm64::trap_entry::AArch64TrapEntry;

#[cfg(feature = "riscv64")]
pub type CurrentTrapEntry = crate::riscv64::trap_entry::Riscv64TrapEntry;
```

---

## 5. 测试要点

### 5.1 QEMU + GDB 验证

```bash
# x86-64: 验证 GDT 已加载
qemu-system-x86_64 -kernel kernel.elf -s -S
(gdb) break init_protection
(gdb) continue
(gdb) step  # 执行 prot.load()
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

### 5.2 单元测试

| 测试 | 验证内容 |
|------|---------|
| `test_gdt_segments_flat` | x86-64 GDT 段描述符是 flat（base=0, limit=full） |
| `test_gdt_privilege_levels` | 内核段 DPL=0，用户段 DPL=3 |
| `test_tss_sp0_set` | TSS.sp0 设置后可正确读取 |
| `test_protection_init_load` | `init()` + `load()` 不 panic |
| `test_trap_entry_init_load` | `init()` + `load()` 不 panic |

---

## 5.5 过渡：从 init_protection() 到 init_clock_and_interrupts()

`init_protection()` 完成后，CPU 已处于正确的保护模式：

| 架构 | init_protection() 完成后的状态 |
|------|--------------------------|
| x86-64 | GDT 已加载（GDTR）、IDT 已加载（IDTR）、TSS 已加载（TR）、CS/DS/SS/ES 正确 |
| aarch64 | VBAR_EL1 指向异常向量表、SP_EL0 设置为用户栈 |
| riscv64 | stvec 指向 trap 入口、sscratch 保存内核栈 |

此时 CPU 可以安全地响应异常和中断——但中断控制器尚未初始化，时钟尚未启动。`init_clock_and_interrupts()` 接管这些工作：

```
init_protection()                init_clock_and_interrupts()
├── ProtectionArch::init()       ├── ClockState::new()
├── ProtectionArch::load()       ├── ClockArch::init_timer(hz)
├── TrapEntryArch::init()        ├── InterruptController::init()
├── TrapEntryArch::load()        └── ArchInit::init()
└── CPU 可响应异常               └── CPU 可响应时钟中断
```

**为什么 phase2 必须在 phase1 之后**：中断控制器初始化后，硬件中断可能立即到来。如果 IDT/VBAR/stvec 尚未设置，中断触发时 CPU 无法找到处理程序，导致 triple fault。

**为什么 phase2 不能在 phase1 之前**：`intr_init()` 在某些架构上需要访问 MMIO（ARM GIC、RISC-V PLIC），这些地址需要页表映射。`prot_init()` 重建页表后，MMIO 地址才可访问。

---

## 6. 参见

- [02-higher-half-kernel.md](02-higher-half-kernel.md) — trampoline 将 CPU 切换到高地址
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — cstart 后半段：时钟与中断初始化
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/arch/src/protection.rs` — ProtectionArch trait 定义
- `os/arch/src/trap_entry.rs` — TrapEntryArch trait 定义
- `os/arch/src/x86_64/protection.rs` — x86-64 GDT/TSS 实现
- `os/arch/src/x86_64/trap_entry.rs` — x86-64 IDT 实现
