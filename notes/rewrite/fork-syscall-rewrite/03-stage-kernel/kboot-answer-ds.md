# 02-answer-ds: 03-stage-kernel 启动叙事的独立设计分析

> **作者**: 独立分析（基于 Minix3 C 源码 + Rust 代码库 + 主流 OS 最佳实践）
> **触发**: 用户对 02-todo.md / 02-design.md 提出的设计困惑
> **范围**: 三个 P0 问题的设计判定、01 修改方案、02 文档规划、最佳实践对比
> **重要**: 本文档独立撰写，不参考其他 AI 生成的 02-answer 文档与历史讨论

---

## 0. 问题速览与结论

| # | 问题 | 我的判定 | 一句话结论 |
|---|------|---------|-----------|
| 1 | 01→02 叙事断裂 | **P0，不可妥协** | 断裂不只是"组织不好"，而是缺失了从 `vm_enable_paging` 到 `kmain` 完整 boot 序列的 200+ 行关键代码——读者不知道"接下来发生了什么" |
| 2 | head.S 高地址跳转在 Rust 版是否必须 | **必须，但形式不同** | head.S 的语义（切栈+跳高地址）是 higher-half kernel 的硬性要求，但 Rust 版通过 boot-shim trampoline 实现，不直接复制 C 的汇编 |
| 3 | 03-stage-kernel 目录覆盖缺口 | **P0，系统性缺失** | `cstart()`/`prot_init()`/`init_clock()`/`intr_init()`/`proc_init()`/`arch_boot_proc()`/`arch_post_init()`/`memory_init()`/`add_memmap()`/`bsp_finish_booting()` 均无独立文档覆盖——这是整个目录级缺失 |

---

## 1. 问题一：叙事断裂的根因与设计决策

### 1.1 断裂的精确位置

```
01-multiboot-bootstrap.md 结束于:
    vm_enable_paging() → return &kinfo

02-page-table-kernel.md 开始于:
    createpde / lin_lin_copy / vm_memset / vm_lookup
```

**中间缺失了什么？** 从 Minix3 C 源码看，`pre_init` 返回后到 `createpde` 等运行时机制生效前，至少经过了以下步骤：

```
head.S 汇编（3 行）
  └── 切栈到 k_initial_stktop、push kinfo 指针、call kmain

kmain() 初始化序列（main.c:96-310）
  ├── memcpy(&kinfo, local_cbi, ...)           ← 保存 boot 参数
  ├── kernel_may_alloc = 1                      ← 开启内核分配器
  ├── memcpy(kinfo.boot_procs, image, ...)      ← 复制 boot 进程镜像
  ├── cstart()                                  ← 平台初始化序列
  │     ├── prot_init() → GDT/IDT/TSS
  │     ├── init_clock()
  │     ├── intr_init()
  │     └── arch_init()
  ├── BKL_LOCK()                                ← 首次获取 BKL
  ├── proc_init()                               ← 清空进程表
  ├── for (i=0; i<NR_BOOT_PROCS; ++i)          ← 加载 boot 进程（VM/PM/VFS/RS）
  │     └── arch_boot_proc() → 加载 ELF 二进制到进程地址空间
  ├── arch_post_init()                          ← 设置 ptproc = VM（让 VM 接管页表）
  ├── memory_init()                             ← 分配 freepdes（VM 运行时机制的前置条件！）
  ├── system_init()                             ← 初始化特权表
  ├── add_memmap(&kinfo, bootstrap_start, ...)  ← 回收 bootstrap 内存
  └── bsp_finish_booting()                      ← 取消 RTS_PROC_STOP，启动所有进程
```

**这 200+ 行代码是理解后续所有文档的前提。** 为什么？因为 02 的 `createpde`/`lin_lin_copy` 等运行时机制都依赖于 `memory_init()` 分配的 `freepdes`，而 `freepdes` 的分配发生在 `kmain` 的初始化序列中。读者如果不理解 `kmain` 的全貌，就无法理解运行时机制"为什么能工作"。

### 1.2 为什么这是 P0 而不是 P1

| 维度 | 分析 |
|------|------|
| **读者认知** | 01 结束时读者建立了"boot 是线性过程"的心智模型。02 直接跳到运行时机制，打破了模型。读者无法定位自己"在哪里" |
| **前置知识缺失** | `createpde` 等运行时机制需要理解"内核-VM 协议"才能理解，而"VM 是怎么被 boot 出来的"从未被介绍 |
| **概念依赖链断裂** | `createpde` 依赖 `freepdes`，`freepdes` 在 `memory_init()` 中分配，`memory_init()` 在 `kmain` 中调用——这条链的起点（`kmain`）缺失 |
| **文档承诺违反** | `00-kernel-overview.md` 声明"开机过程是严格线性的"，但 02 违约——这不是"组织不好"，是"对读者的承诺没有兑现" |

### 1.3 设计决策：如何修复

**原则**：不破坏现有文档的完整性，在中间插入"桥梁文档"。

**方案**：新增一篇文档 `02-boot-bridge.md`，覆盖从 `vm_enable_paging` 返回后到 `kmain` 初始化序列结束的完整链路。**不**修改 01 的正文内容（01 的职责是"从 GRUB 到分页开启"，这是完整的闭环），只在 01 末尾增加一个"下一站"导航段。

**为什么不是修改 01 来包含 head.S 部分？** 因为 01 的语义边界是"GRUB → 分页开启"，这是 C 源码的完整闭环。head.S 的汇编代码和 kmain 的初始化序列属于**下一个叙事阶段**——"从分页开启到系统就绪"。强行塞进 01 会破坏 01 的完整性。

---

## 2. 问题二：高地址跳转的设计

### 2.1 Minix3 C 的真相

```asm
/* head.S:78-87 — 关键的三行（实际是四行） */
call   _C_LABEL(pre_init)        /* 调 C 函数，返回 %eax = &kinfo */
mov    $k_initial_stktop, %esp   /* 切栈到 kernel BSS 高地址（符号值=0xF04xxxxx） */
push   $0                        /* 终止栈标记（供调试器回溯栈帧） */
push   %eax                      /* 传入 &kinfo 指针 */
call   _C_LABEL(kmain)           /* 跳到高地址 kmain（符号值=0xF04xxxxx） */
```

**发生了什么？**

1. `pre_init` 返回时，`%eax` 是 `&kinfo` 的虚拟地址——**这已经是高地址**（因为 `kinfo` 在内核 BSS 中，链接到 `0xF0400000+`）
2. `mov $k_initial_stktop, %esp`——切栈。新栈在 BSS 中，链接地址也是高地址。但此时恒等映射还在，所以物理上能访问
3. `call kmain`——`call` 是 PC-relative 寻址，offset 由链接器在编译时计算（VMA=0xF04xxxxx），所以 RIP 跳到高地址
4. 从这一刻起，RIP 和 RSP 都在高地址——CPU 通过页表高地址映射访问物理 RAM 的同一段位置

**关键洞察**：`pre_init` 的返回值和 `kinfo` 的地址在 `pre_init` 返回时就已经是高地址了。但 RIP 还在低地址（恒等映射区）。所以系统处于一个**中间态**：指针高、代码低。head.S 的三行代码完成的是"让 RIP 追上指针"。

### 2.2 为什么 Rust 版必须实现这个语义

| 原因 | 说明 |
|------|------|
| **恒等映射会被移除** | VM 启动后会移除恒等映射（只保留高地址映射）。如果 RSP 仍在低地址，栈帧所在的物理页只能通过高地址访问——但 RSP 指向低地址——**立即崩溃** |
| **内核代码必须可访问** | 同样，RIP 在低地址时，一旦恒等映射移除，CPU 无法取指——**立即崩溃** |
| **这是 higher-half kernel 的硬性要求** | 不是 Minix3 特有的设计——所有 higher-half kernel（Linux、Redox、seL4）都必须做这个跳转 |

**当前 Rust 版的状态**（`os/kernel/src/lib.rs:33-40`）：

```rust
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    let info = arch_boot_impl::<...>(kernel_info, root_page);
    kmain(info)  // ← 没有切栈/跳高地址，RIP 和 RSP 都在低地址
}
```

这是**定时炸弹**——当前能运行只是因为恒等映射还在，一旦 VM 移除恒等映射，内核立即崩溃。

### 2.3 最佳实践对比

| 系统 | 方式 | 关键特征 |
|------|------|---------|
| **Minix3 C** | AT() 在链接脚本中 + head.S 汇编 trampoline | 同一段代码在物理 4MB 加载，虚拟 0xF0400000 执行。AT() 告诉 GRUB 加载到低物理地址，链接器用高虚拟地址生成符号 |
| **Redox** | kernel ELF 链接到高地址 + bootloader 解析 ELF 段 | 更"诚实"：kernel ELF 的 VMA 就是真实高地址，bootloader 负责把段拷贝到低物理地址，建页表，然后 trampoline 跳高地址 |
| **Linux** | `lretq` far return | 64 位特有的 far return 指令，同时切 CS 和 RIP 到高地址。不需要 AT() — 压缩内核解压后已经在高地址 |
| **seL4** | elfloader 加载 kernel ELF + kernel 自己准备栈 | elfloader 把 kernel ELF 的 PT_LOAD 段拷贝到物理内存，建页表，跳到 kernel entry。kernel 自己在 entry 函数里准备栈 |

### 2.4 我对 Rust 版的设计建议

**选择 Redox 路径**（独立 kernel ELF + boot-shim 加载 + trampoline）：

**为什么不是 AT() 路径？**
1. AT() 是 GNU ld 特有的语法，Rust 链接器（lld）不支持
2. AT() 依赖 GRUB 的 Multiboot 加载器，而 minix-rs 用 UEFI，没有 GRUB 替你处理 LMA vs VMA
3. AT() 把"物理地址"编码在链接脚本中——这是平台相关的（x86: 4MB, ARM: 可能不同），不符合 minix-rs 的硬件抽象原则

**为什么不是 Linux 的 `lretq` 路径？**
1. Minix-rs 是 64 位，`lretq` 在 x86-64 上可用，但它同时切 CS 和 RIP——需要正确的 GDT 设置
2. `lretq` 是 x86 特有的——在 ARM64/RISC-V 上无对应语义
3. 不符合 minix-rs 的硬件抽象原则

**推荐方案：独立的 `trampoline` 模块**

```
trampoline（汇编文件，各 arch 独立）
  ├── 切换到高地址内核栈（RSP = kernel_stack_top，高虚拟地址）
  ├── 跳转到高地址 kmain（通过绝对跳转或 jmp 寄存器）
  └── 永不返回

boot-shim 调用流程:
  1. arch_boot_impl() → 建恒等映射 + 高映射 + 开分页
  2. 返回 KernelInfo 引用
  3. 调用 arch_trampoline(kernel_info, kmain_entry)
     → 汇编代码执行切栈+跳转
     → 永不返回
```

**为什么要用裸汇编文件而不是 Rust 内联 asm？**

Rust 的 `asm!` 宏在函数体内执行时，编译器会插入 prologue/epilogue（保存/恢复寄存器、操作栈帧）。如果我们在 `asm!` 中切 RSP，编译器生成的 prologue 访问的是**旧栈**，epilogue 试图恢复的也是旧栈——这会导致未定义行为。

**`naked` 函数**（`#[naked]`）可以避免 prologue/epilogue，但：
- `#[naked]` 在 stable Rust 上不可用（需要 nightly）
- 在 `#[naked]` 函数中写 `asm!` 仍然有一些限制

**最佳实践**：用独立的 `.S` 汇编文件（`global_asm!` 或 build.rs 编译），与 Rust 代码完全解耦。

### 2.5 当前代码的改造路径

**第一步**：把 kernel 变成独立 binary（有自己的链接脚本）

```
os/kernel/Cargo.toml:
  [[bin]]
  name = "minix-kernel"
  path = "src/main.rs"
  test = false

os/kernel/linker.ld:
  /* 内核链接到高地址，不依赖 AT() */
  . = 0xFFFF_8000_0000_0000;  /* 或平台特定的高地址 */
  .text : { *(.text*) }
  .rodata : { *(.rodata*) }
  .data : { *(.data*) }
  .bss : { *(.bss*) }
```

**第二步**：boot-shim 加载 kernel ELF（不再是 rlib 链接）

```
boot-shim:
  1. 从 boot_modules 中找到 kernel ELF
  2. 解析 ELF PT_LOAD 段
  3. 分配物理页
  4. 拷贝段到物理页
  5. 记录 kernel entry point
```

**第三步**：实现 trampoline

```
os/kernel/src/arch/x86_64/trampoline.S:
  mov rsp, {kernel_stack_top}   ← 高地址栈
  jmp  {kmain_entry}            ← 高地址入口

os/kernel/src/lib.rs:
  pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
      let info = arch_boot_impl::<...>(kernel_info, root_page);
      // 不直接调 kmain，而是通过 trampoline
      unsafe { arch_trampoline(info.kernel_stack_top, kmain as usize); }
  }
```

**关于 stack_top 的获取**：`kernel_stack_top` 是内核 BSS 中的一个符号，链接器赋予它高虚拟地址。Rust 中可以通过 `extern "C"` 声明来获取这个符号的地址。

---

## 3. 问题三：文档覆盖缺口与目录重构

### 3.1 当前状态

| 文档 | 覆盖内容 | 缺失 |
|------|---------|------|
| 00-kernel-overview.md | 整体架构、启动线概览 | 只有概览，无详细展开 |
| 01-multiboot-bootstrap.md | GRUB → pre_init → 分页开启 | head.S 三行、kmain 全部 |
| 02-page-table-kernel.md | 运行时页表机制（createpde 等） | 运行时机制的前置条件（freepdes 从哪来？VM 怎么被 boot 的？） |
| tmp-* 各文档 | 各子系统参考资料 | 分散在 tmp-* 中，未按时间线组织 |

### 3.2 我的设计原则

**原则 1：时间线叙事优先，参考资料为辅**

00-kernel-overview.md 已经确立了"严格线性"的叙事承诺。后续文档应该坚持这个承诺——从 GRUB 开始，按时间顺序，每一步都讲清楚。

**原则 2：一篇文档 = 一个完整的叙事阶段**

不要把一个阶段拆成 5 个小文档——读者需要频繁切换上下文。一篇文档应该覆盖一个完整的、可理解的叙事阶段。

**原则 3：参考资料文档（tmp-*）作为"附录"被引用**

tmp-* 文档（如 `tmp-04-protect.md`、`tmp-05-idt.md`）提供了各子系统的详细参考资料。时间线文档**引用**它们，但**不重复**它们的内容。职责分离：时间线文档讲"什么时候发生了什么"，tmp-* 文档讲"每个子系统怎么工作的"。

**原则 4：新文档是"新建"，不是"替换"**

01 文档保持完整不变（只加导航段）。02 重新定位为"参考资料"（不改内容，只改定位）。新增文档填补中间缺失的叙事阶段。

### 3.3 推荐的文档重构方案

```
03-stage-kernel/
  ├── 00-kernel-overview.md          ← 不变（整体架构概览）
  ├── 01-multiboot-bootstrap.md      ← 小改（末尾加导航段，定位到 02-boot-bridge）
  ├── 02-boot-bridge.md             ← 新增（核心：从分页开启到 kmain 初始化序列结束）
  ├── 03-page-table-kernel.md       ← 改名（原 02-page-table-kernel.md，重新定位为"运行时机制"）
  ├── 04-protect-interrupt.md       ← 新增（GDT/IDT/TSS 保护模式，整合 tmp-04/05/06）
  ├── 05-clock-timer.md             ← 新增（时钟与定时器，整合 tmp-09/10/12）
  ├── 06-proc-scheduling.md         ← 新增（进程管理、调度、IPC，整合 tmp-07/08/11/13/14/15/16）
  └── tmp-*/                        ← 保留（作为参考资料被引用，不作为叙事主体）
```

**与 02-design.md 的 10 篇文档方案对比**：

| 维度 | 02-design.md 方案 | 我的方案 |
|------|------------------|---------|
| 新文档数 | 10 篇 | 4 篇（02-boot-bridge, 04-protect-interrupt, 05-clock-timer, 06-proc-scheduling） |
| 覆盖范围 | 每个子系统独立一篇 | 相关子系统合并，一篇覆盖一个完整阶段 |
| 碎片化程度 | 高（每篇 100-200 行，读者来回跳） | 低（每篇 400-600 行，自包含） |
| tmp-* 处理 | 未明确 | 保留为参考资料，由新文档按需引用 |
| 叙事连续性 | 单篇独立但需要读者自行拼合 | 每篇按时间线推进，读者顺序阅读即可 |

### 3.4 各新文档的详细内容规划

#### 02-boot-bridge.md（新增，核心文档）

**语义边界**：从 `vm_enable_paging` 返回后，到 `kmain` 初始化序列结束，系统处于"调度就绪"状态。

**内容大纲**（约 600 行）：

```
1. head.S 三行代码的语义（C 版）
   1.1 切栈：为什么需要切栈？旧栈 vs 新栈
   1.2 跳高地址：call kmain 做了什么
   1.3 Rust 版的等价实现：trampoline 设计

2. kmain 入口：从 boot 参数到内核状态
   2.1 memcpy(&kinfo, ...)：保存 boot 参数
   2.2 kernel_may_alloc = 1：内核分配器启动
   2.3 memcpy(kinfo.boot_procs, ...)：复制 boot 进程镜像

3. cstart()：平台初始化序列
   3.1 prot_init() → GDT/IDT/TSS（引用 tmp-04-protect.md）
   3.2 init_clock()（引用 tmp-09-clock.md）
   3.3 intr_init()（引用 tmp-05-idt.md）
   3.4 arch_init()

4. 进程表初始化
   4.1 BKL_LOCK()：首次获取 BKL
   4.2 proc_init()：清空进程表
   4.3 arch_boot_proc()：加载 VM 二进制到进程地址空间
       - 从 boot_modules 中找到 VM ELF
       - 解析 ELF 段 → 分配页 → 拷贝 → 设置页表
       - 同样处理 PM/VFS/RS

5. 启动后配置
   5.1 arch_post_init()：设置 ptproc = VM（让 VM 接管页表）
   5.2 memory_init()：分配 freepdes（VM 运行时机制的前置条件）
   5.3 system_init()：初始化特权表
   5.4 add_memmap(bootstrap)：回收 bootstrap 内存（0x400000-0x410000 的代码段和数据段）

6. 启动所有进程
   6.1 bsp_finish_booting()：取消 RTS_PROC_STOP，启动所有进程
   6.2 switch_to_user()：进入调度循环，选择第一个用户进程

7. Rust 版设计映射
   7.1 boot-shim 如何加载 kernel ELF
   7.2 trampoline 如何切栈+跳高地址
   7.3 kmain 初始化序列的 Rust 实现方案
```

#### 03-page-table-kernel.md（原 02，重新定位）

**语义边界**：从"VM 已经启动并与内核建立协议"之后，内核侧的运行时页表机制。

**内容**：保留原有 createpde/lin_lin_copy/vm_memset/vm_lookup 等内容，但：
- 在文档开头增加"前置条件"段：说明 `freepdes` 来自 `memory_init()`（02-boot-bridge.md §5.2），`ptproc` 来自 `arch_post_init()`（02-boot-bridge.md §5.1）
- 重新编号为 03，保持叙事顺序

#### 04-protect-interrupt.md（新增）

**语义边界**：保护模式（GDT/IDT/TSS）和中断处理。

**内容**：整合 tmp-04-protect.md、tmp-05-idt.md、tmp-06-exception.md 的内容，按"初始化→运行时"组织。

#### 05-clock-timer.md（新增）

**语义边界**：时钟中断和定时器管理。

**内容**：整合 tmp-09-clock.md、tmp-10-timer.md、tmp-12-watchdog.md 的内容。

#### 06-proc-scheduling.md（新增）

**语义边界**：进程管理、调度和 IPC。

**内容**：整合 tmp-07-proc.md、tmp-08-ipc.md、tmp-11-sched.md、tmp-13-system.md、tmp-14-kernel-call.md、tmp-15-notify.md、tmp-16-utility.md 的内容。

**注意**：tmp-17-main-init.md 的内容分散到 02-boot-bridge.md（kmain 初始化序列部分）和新的子系统文档中。tmp-17 不再作为独立文档。

---

## 4. 01 文档是否需要修改？

### 4.1 我的判断：只需微调，不需要大改

**01 文档的职责**是"从 GRUB 到分页开启"——这是 C 源码的完整闭环。它的叙事是自洽的，内容是正确的。

**需要修改的地方**：

1. **末尾增加"下一站"导航段**（约 10 行）：

```markdown
## 1.7 下一站：从分页开启到 kmain

> 本文档覆盖了 `pre_init()` 的全过程。`pre_init()` 返回后，控制权回到 `head.S` 汇编代码：
> - 切栈到内核高地址栈（`k_initial_stktop`）
> - 跳转到高地址 `kmain()`（`main.c:96`）
> - `kmain()` 执行完整的初始化序列：`cstart()` → `proc_init()` → `arch_boot_proc()` → `memory_init()` → `bsp_finish_booting()`
>
> 这些内容在 → [02-boot-bridge.md](./02-boot-bridge.md) 中详细展开。
>
> **Rust 版注意**：UEFI 下没有 GRUB 的 Multiboot 协议，`head.S` 的三行代码由 `boot-shim` 的 trampoline 模块替代。详见 02-boot-bridge.md §1.3。
```

2. **§1.2 完整启动链路图的末尾**，在 `head.S: call kmain` 之后，增加一个简短的注释说明 `kmain` 的初始化序列将在 02 中展开。

### 4.2 不需要修改的地方

- 01 的所有正文内容（§1.1-§1.6, §2.1-§2.10）——这些是 C 源码的准确描述，不需要改动
- 01 的语义边界——"从 GRUB 到分页开启"是完整的，不应扩展到 head.S 或 kmain

---

## 5. 02 文档应该如何规划？

### 5.1 当前 02 的三个角色

当前的 `02-page-table-kernel.md` 实际上承担了三个角色：

| 角色 | 说明 | 是否合适 |
|------|------|---------|
| 运行时页表机制参考资料 | createpde/lin_lin_copy/vm_memset 的技术细节 | **合适**——但应放在"VM 启动后"的叙事位置 |
| 启动叙事续篇 | 接在 01 之后，但内容不对应 | **不合适**——读者期望的是 kmain 初始化序列，不是运行时机制 |
| 目录结构占位符 | 作为 02 号文档存在 | **不合适**——它打破了对 00 的"线性叙事"承诺 |

### 5.2 重构方案

**不变**：02-page-table-kernel.md 的内容保留（技术上是正确的），改为 `03-page-table-kernel.md`。

**新增**：`02-boot-bridge.md`（§3.4 详细规划）。

**效果**：读者按 01→02→03→04→05→06 顺序阅读，获得完整的线性叙事。

### 5.3 写什么内容

参见 §3.4 的详细大纲。核心原则：

- **时间线驱动**：每个文档按"初始化阶段→运行时"组织，而不是"功能分类"
- **引用而非重复**：tmp-* 文档的详细内容通过链接引用，不重复
- **C 源码对照**：每个关键步骤标注对应的 C 源码文件和行号
- **Rust 设计映射**：每个文档末尾有"Rust 版设计映射"段，说明如何在 Rust 中实现等价语义

---

## 6. 最佳实践总结

### 6.1 Higher-Half Kernel 的通用模式

| 步骤 | 所有 higher-half kernel 的共同点 | Minix3 C | minix-rs Rust |
|------|-------------------------------|----------|--------------|
| 1. 物理地址入口 | 内核在低物理地址启动 | head.S→pre_init（物理 4MB） | boot-shim.efi（UEFI 加载） |
| 2. 内存发现 | 解析 bootloader 提供的内存布局 | multiboot mmap | UEFI GetMemoryMap |
| 3. 恒等映射 | 建立 VA=PA 的映射（4GB 或 1GB） | pg_identity（4MB 大页 × 1024） | arch_boot_impl（1GB 大页 × 4） |
| 4. 高地址映射 | 映射内核 VMA 到物理 RAM | pg_mapkernel（4MB 大页） | arch_boot_impl（1GB 大页） |
| 5. 开分页 | 写 CR3 / satp + enable | pg_load + vm_enable_paging | Paging::enable |
| 6. **Trampoline** | **切栈+跳高地址** | **head.S 三行** | **trampoline.S（裸汇编）** |
| 7. 内核初始化 | 初始化子系统、启动进程 | kmain→cstart→...→bsp_finish_booting | kmain 初始化序列 |

### 6.2 关键设计约束

1. **Trampoline 必须用裸汇编**：不能用 Rust 内联 asm，因为编译器会插入 prologue/epilogue
2. **内核必须是独立 ELF binary**：不能是 rlib，因为 rlib 没有自己的链接脚本和 VMA
3. **boot-shim 负责加载**：解析 kernel ELF 的 PT_LOAD 段，拷贝到物理内存，建页表
4. **恒等映射在 trampoline 后可以不立即移除**：VM 在启动后自行移除恒等映射（`sys_vmctl`）

### 6.3 与 Redox 的对齐

minix-rs 的 boot-shim 概念与 Redox 的 bootloader 高度一致。关键差异：
- Redox 的 bootloader 是独立项目，minix-rs 的 boot-shim 在同一个 workspace 内
- Redox 在 BIOS 和 UEFI 下有不同实现，minix-rs 当前只支持 UEFI
- 核心流程（ELF 解析→页表建立→trampoline→kernel entry）是相同的

---

## 7. 实施优先级

| 优先级 | 任务 | 说明 |
|--------|------|------|
| **P0 立即** | 新增 `02-boot-bridge.md` | 填补叙事断裂，覆盖 kmain 初始化序列 |
| **P0 立即** | 01 末尾加导航段 | 让读者知道"接下来读什么" |
| **P0 近期** | 实现 trampoline（裸汇编） | 修复代码 bug：恒等映射移除后内核崩溃 |
| **P0 近期** | 把 kernel 改成独立 binary + 链接脚本 | 支持高地址链接 |
| **P1 近期** | 重命名 02→03（page-table-kernel） | 恢复叙事顺序 |
| **P1 中期** | 新增 04-protect-interrupt.md | 整合 tmp-04/05/06 |
| **P1 中期** | 新增 05-clock-timer.md | 整合 tmp-09/10/12 |
| **P1 中期** | 新增 06-proc-scheduling.md | 整合 tmp-07/08/11/13/14/15/16 |
| **P2 后续** | tmp-* 文档作为参考资料保留 | 不删除，但不再作为叙事主体 |

---

## 8. 未解决的问题

1. **kernel ELF 的加载位置**：boot-shim 应该把 kernel ELF 加载到哪个物理地址？Minix3 C 加载到 4MB。Rust 版可能是 2MB 或 1MB。需要根据 UEFI 内存布局确定。

2. **高地址的具体值**：Minix3 C 使用 `0xF0400000`（32 位）。64 位 Rust 版应该使用什么高地址？Linux 使用 `0xFFFF_FFFF_8000_0000`。Redox 使用 `0xFFFF_FFFF_8000_0000`。需要与 `minix_arch` 的地址空间布局对齐。

3. **boot_modules 的传递方式**：当前 UEFI 下 boot_modules 是空的。如何在 UEFI 下传递 VM/PM/VFS 的二进制？是嵌在 UEFI binary 中还是通过 UEFI 协议传递？

4. **arch_boot_proc 的 ELF 加载器**：当前 Rust 版没有 ELF 加载器。需要实现 PT_LOAD 段解析、物理页分配、拷贝、页表设置。这部分应该在 02-boot-bridge.md 中覆盖。

5. **multiboot 到 UEFI 的语义迁移**：01 文档大量使用 multiboot 术语。在 02-boot-bridge.md 中需要明确说明 UEFI 下的等价语义。01 文档中也要标注"UEFI 下对应物"。

---

## 附录 A：Minix3 C 源码关键文件对照

| 文件 | 行数 | 关键函数 | 对应 Rust 模块 |
|------|------|---------|---------------|
| `arch/i386/head.S` | 89 | `MINIX` 入口, `call pre_init`, `call kmain` | `trampoline.S`（新增） |
| `arch/i386/pre_init.c` | 243 | `pre_init()`, `get_parameters()` | `os/kernel/src/lib.rs` arch_boot_impl |
| `arch/i386/pg_utils.c` | 317 | `pg_identity()`, `pg_mapkernel()`, `pg_load()`, `vm_enable_paging()` | `minix_arch::paging::HugePages` |
| `arch/i386/protect.c` | 456 | `prot_init()`, `arch_boot_proc()`, `arch_post_init()` | 待实现 |
| `arch/i386/memory.c` | `memory_init()` | 分配 freepdes | 待实现 |
| `main.c` | 522 | `kmain()`, `cstart()`, `bsp_finish_booting()`, `announce()` | `os/kernel/src/lib.rs` kmain |
| `arch/i386/kernel.lds` | 152 | AT() 机制, `_kern_phys_base`, `_kern_vir_base` | `os/kernel/linker.ld`（新增） |

## 附录 B：网络研究参考

- Redox OS bootloader: https://gitlab.redox-os.org/redox-os/bootloader
- Linux x86_64 boot: https://github.com/torvalds/linux/blob/master/arch/x86/kernel/head_64.S
- seL4 elfloader: https://docs.sel4.systems/projects/elfloader
- rust-osdev bootloader: https://github.com/rust-osdev/bootloader
- OSDev Higher Half Kernel: https://wiki.osdev.org/Higher_Half_Kernel
- OSDev Creating a 64-bit kernel: https://wiki.osdev.org/Creating_a_64-bit_kernel