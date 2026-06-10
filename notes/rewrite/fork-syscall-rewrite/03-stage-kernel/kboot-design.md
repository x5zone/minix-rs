# 02-design: Kernel Boot 架构设计与讨论记录

> **分类**: Kernel 架构设计
> **说明**: 记录 02-page-table-kernel.md 及整个 03-stage-kernel 目录层级的架构讨论和设计方案
> **创建**: 2025-06-07，由 02-todo.md 中提出的 P0 问题触发

---

## 1. 问题发现

### 1.1 02-page-table-kernel.md 的叙事断裂

02-page-table-kernel.md 当前以**运行时机制**（createpde/lin_lin_copy/vm_memset/vm_lookup）为主体内容，
但这些机制在 `kmain()` → `bsp_finish_booting()` → `switch_to_user()` → VM running 之后才生效。

而 01-multiboot-bootstrap.md 结束于 `vm_enable_paging()`（分页开启）。读者在看完 01 后期待的是**boot 的延续**，
而不是一个"已就绪系统中的运行时机制"。这就形成了叙事断裂——00-kernel-overview.md 承诺的"严格线性 boot 过程"
在 01→02 之间断开。

### 1.2 03-stage-kernel 目录的覆盖缺口

从 Minix3 源码 `main.c:96`（kmain 入口）追踪完整的 boot 序列，发现多个缺口：

| 源码步骤 | 函数 | 现有文档 | 覆盖状态 |
|---------|------|---------|---------|
| `kmain()` 入口 | `memcpy(&kinfo, local_cbi)` + assert BSS | 17-main-init | 有但叙事错位 |
| `cstart()` | `prot_init()` | 04-protection | ✅ |
| `cstart()` | `init_clock()` | 16-timer | ⚠️ 需要确认时序覆盖 |
| `cstart()` | `intr_init()` | 05-exception-interrupt | ✅ |
| `proc_init()` | 清空进程表 | 06-proc-struct | ⚠️ 需确认 |
| `arch_boot_proc()` | 加载 VM ELF | 04-protection | ✅ |
| `arch_post_init()` | 设置 ptproc = VM | **被埋没在 02** | ❌ |
| `memory_init()` | 分配 freepdes | **被埋没在 02** | ❌ |
| `add_memmap(bootstrap)` | 回收 bootstrap 区 | 无人提及 | ❌ **完全缺失** |
| `bsp_finish_booting()` | announce, switch_to_user | 17-main-init | ✅ |

核心结论：**没有一份文档以"01 结束之后 kmain 一步步做什么"为叙事主线来组织这些内容。**
17-main-init.md 最接近，但它是参考资料式的平铺，不是时间线主导的叙事。

### 1.3 高地址跳转缺失：定时炸弹

当前 Rust 版内核作为 `rlib` 链接进 boot-shim，运行在低地址（恒等映射保命）。
虽然页表中建立了 `kern_virt_base → kern_phys_base` 的映射，但 RSP 和 RIP 从未跳到高地址。

这意味着：恒等映射一旦被 VM 移除（VM 启动后必然发生），内核立即崩溃。
当前 `kmain()` 的 `loop {}` 能跑只是因为恒等映射还在且无人碰它。

这 3 个问题被判定为 **P0**——不是"组织不好"的问题，而是文档误导读者、代码有隐藏 bug。

---

## 2. Minix3 C 的 Boot 架构

### 2.1 核心机制：链接脚本 AT() 技巧

Minix3 的 kernel 链接脚本（`kernel.lds`）使用了一个精巧的技巧让 GRUB 把内核代码放在物理低地址，
但编译器生成的符号是高地址的。

```ld
_kern_phys_base = 0x00400000;   /* 物理 4MB */
_kern_vir_base  = 0xF0400000;   /* 虚拟高地址 */
_kern_offset = _kern_vir_base - _kern_phys_base;  /* = 0xF0000000 */

SECTIONS {
    . = _kern_phys_base;                    /* 初始定位 4MB——低地址 */
    .unpaged_text : { unpaged_*.o(.text) }  /* 低地址 */
    . += _kern_offset;                      /* 跳到 0xF0400000——高地址 */

    .text : AT(ADDR(.text) - _kern_offset)  /* AT() 是关键 */
    { *(.text*) }
    /* p_vaddr = 0xF0400000+偏移（高）— 给链接器/编译器符号用 */
    /* p_paddr = 0x00400000+偏移（低）— 给 GRUB 加载器用 */
}
```

**为什么需要 AT()？** GRUB 按 ELF 的 p_paddr（物理加载地址）加载内核。链接脚本不能改变 GRUB 的行为，
只能通过 AT() 让同一个 ELF 包含两个信息：VMA（高地址）= 符号值，LMA（低地址）= GRUB 的物理放置位置。

### 2.2 物理加载与地址空间切换

GRUB 加载后，同一段物理代码有两个虚拟窗口：

```
物理 RAM 0x400000:  [--- kernel binary ---]
                      ↑                    ↑
                      | 恒等映射            | 偏移映射
                      | (VA=PA)             | (VA = PA + 0xF0000000)
                      |                     |
             RIP 0x400000 (pre_init)     RIP 0xF0400000 (kmain)
```

切换发生在 `head.S` 的三行代码：

```asm
  call pre_init                    ← 在恒等映射区执行

  mov $k_initial_stktop, %esp      ← 切栈到 kernel BSS 高地址
                                     （符号值高，页表翻译到物理低地址 RAM）

  call _C_LABEL(kmain)             ← kmain 符号值 = 0xF04xxxxx
                                     CPU 跳到高虚拟地址 → 页表 → 物理 0x400000+
                                     RIP 从此在高地址
```

**物理代码从未移动。** `call kmain` 只是让 RIP 从走恒等映射改为走高映射。

### 2.3 为什么切栈是必须的

`call kmain` 之前必须先切换栈到高地址。如果不切：

| | 不切栈 | 切栈 |
|---|---|---|
| `call kmain` 压栈地址 | 低地址（0x004xxxxx） | 高地址（0xF04xxxxx） |
| kmain 内局部变量地址 | 低地址 | 高地址 |
| 恒等映射移除后 | **崩溃**（栈不可访问） | 安全 |

**切栈和跳高地址必须成对出现。** 只跳不切或者只切不跳都意味着部分执行状态在低地址，
恒等映射移除时崩溃。

### 2.4 多模块：只有内核需要 AT()

```
GRUB 配置（boot.cfg）:
  load_mods /boot/minix_default/mod*   ← VM/PM/VFS/RS/… 多个文件
  multiboot /boot/minix_default/kernel ← 内核

kmain() 中:
  for 每个 boot process:
    arch_boot_proc(ip, rp)
      if p_nr == VM_PROC_NR:           ← ★ 只有 VM 特殊
        libexec_load_elf(&execi)       ← 内核自带 ELF 加载器
        → 分配页 → 拷贝 → 设置 pc/sp
      else:
        ip->start_addr = mod->mod_start  ← 仅记录物理地址
      RTS_SET(rp, RTS_VMINHIBIT)        ← 等 VM 创建页表
```

| Binary | ELF 类型 | VMA | LMA | 谁映射 | 说明 |
|--------|---------|-----|-----|--------|------|
| **kernel** | AT() 特殊 ELF | 高地址 | 低物理 | 自己建恒等+高映射 | 自举后在高地址运行 |
| **VM** | 普通 ELF | = LMA | = LMA | kernel's arch_boot_proc | 加载到 bootstrap 页表 |
| **PM/VFS/…** | 普通 ELF | = LMA | = LMA | VM 启动后建页表 | 等 VM 设置地址空间 |

只有内核需要在启动早期跳到高地址（因为内核永远驻留，需要从用户地址空间中隔离出来）。
其他服务器进程是普通用户态进程，由 VM 管理页表，不需要高地址。

---

## 3. minix-rs 当前状态与差异

### 3.1 当前架构（有问题的状态）

```
boot-shim.efi (UEFI 应用)
└── minix-kernel (rlib 链接进来)
    ├── arch_boot_impl() — 建页表、开分页
    └── kmain() — loop {}（被函数调用，RIP 在低地址）

编译方式：
  编译 boot-shim 时，minix-kernel 作为 rlib 被链接在一起
  整个 .efi 在 UEFI 加载时被放到 UEFI 分配的物理地址（低地址）

问题：
  - RIP 从未跳到高地址（没有独立 kernel ELF，没有链接脚本，没有 trampoline）
  - RSP 一直用 UEFI 栈（不是内核自己的栈）
  - 恒等映射移除后内核立即崩溃
  - kmain() 为空（loop {}），所以目前没出事——但这是定时炸弹
```

### 3.2 与 Minix3 C 的差异对比

| 方面 | Minix3 C | minix-rs (当前) | 问题 |
|------|----------|----------------|------|
| 内核编译产物 | 独立 ELF，链接到 `0xF0400000` | rlib，链接进 boot-shim | ❌ |
| 物理加载 | GRUB 按 p_paddr 放到 `0x400000` | UEFI 随机分配 | ❌ |
| 高映射 | `0xF0400000 → 0x00400000` 映射真实代码 | 映射到 0/0x100000，**代码不在此** | ❌ |
| 跳高地址 | `call kmain` (PC-relative, 0xF04xxxxx) | 没有 trampoline | ❌ |
| 内核栈 | BSS 中的 `k_initial_stack` (K_STACK_SIZE) | 一直用 UEFI 栈 | ❌ |
| ELF 加载 | kernel 自带 `libexec_load_elf` | 没有实现 | ❌ |
| boot 模块 | GRUB `load_mods` → `kinfo.module_list[]` | `boot_modules: &[]` 空 | ❌ |

### 3.3 UEFI vs GRUB 差异

| | GRUB | UEFI |
|---|---|---|
| 加载内核 | 按 ELF p_paddr | 只加载 .efi (PE/COFF)，不支持 ELF |
| 加载模块 | `load_mods` 命令 | boot-shim 自己读分区（`SimpleFileSystemProtocol`） |
| 内存分配 | 固定物理地址 | `AllocatePages` / `ExitBootServices` |
| 初始栈 | GRUB 给 4KB `load_stack` | UEFI 给 128KB+ 栈 |
| 页表 | 由 kernel 从零建立 | UEFI 可能已有简单页表（shim 需覆盖） |

**关键差异**：UEFI 的栈足够大、不在将被回收的段中——所以 minix-rs 不需要"从 4KB 栈切到 BSS 大栈"这个步骤。
但"从低地址跳到高地址"仍然必须做。

---

## 4. 最佳实践方案

### 4.1 设计原则

1. **诚实 ELF，不用 AT()** — kernel ELF 的 VMA 就是真实的高地址，boot-shim 自己算物理地址
2. **独立 kernel binary** — kernel 编译为独立 ELF，链接到 `kern_virt_base`
3. **boot-shim 统一做加载** — 读分区、解析 ELF、拷贝物理页、建映射、trampoline
4. **AT() 不需要保留** — 我们全盘控制 boot-shim，不需要 GRUB 时代的 hack

### 4.2 总体架构

```
编译时:

  minix-kernel (独立 ELF binary)           boot-shim (UEFI 应用)
  ┌──────────────────────────┐             ┌──────────────────────────┐
  │ 链接脚本: VMA=高地址     │             │ main() {                 │
  │ . = KERN_VIRT_BASE;      │             │   1. 读 ESP 分区         │
  │ .text : { *(.text*) }     │             │      → vm.bin, pm.bin... │
  │ .bss: __kernel_stack_top  ├──嵌入────→ │   2. 分配物理页          │
  │ 入口: _start             │             │   3. 拷贝 boot modules   │
  └──────────────────────────┘             │   4. ExitBootServices    │
                                           │   5. 建恒等+高映射       │
                                           │   6. 开分页              │
                                           │   7. trampoline → 高地址 │
                                           └──────────────────────────┘
                                                    │
                                                    ▼
                                           ┌──────────────────────────┐
                                           │  minix-kernel @ 高地址    │
                                           │  kmain() {               │
                                           │    cstart() → proc_init() │
                                           │    → ... → switch_to_user │
                                           │  }                       │
                                           └──────────────────────────┘
```

### 4.3 各 Binary 的处理方式

| Binary | 来源 | ELF 类型 | 加载方式 | 映射方式 |
|--------|------|---------|---------|---------|
| **kernel** | 编译时嵌入（`include_bytes!`） | VMA=高地址 | boot-shim 算物理地址，拷贝 | 恒等 + 高映射（arch_boot_impl） |
| **VM** | UEFI 读分区（`/boot/vm.bin`） | 普通 ELF | boot-shim 拷贝到物理页 | `arch_boot_proc` 加载到 bootstrap 页表 |
| **PM/VFS/…** | UEFI 读分区 | 普通 ELF | boot-shim 记录物理地址 | VM 启动后 `VMCTL_SETADDRSPACE` |

**为什么内核需要编译时嵌入？** 因为内核必须在 `ExitBootServices` 之前就在内存中，
而 `ExitBootServices` 之后文件系统不可用。嵌入保证内核数据随 boot-shim 一起进内存。

**为什么其他服务器可以读分区？** 因为它们只需要在内存中，不需要在高地址运行。
`ExitBootServices` 之前通过 UEFI 的 `SimpleFileSystemProtocol` 读入，之后 boot-shim 把物理地址传给 kmain。

### 4.4 minix-kernel 链接脚本设计

```ld
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

KERN_VIRT_BASE = 0xFFFF_8000_0000_0000;
KERN_PHYS_BASE = 0x0010_0000;

SECTIONS {
    . = KERN_VIRT_BASE;

    .text : AT(ADDR(.text) - KERN_VIRT_BASE + KERN_PHYS_BASE) {
        _start = .;
        *(.text*)
    }

    .rodata : AT(ADDR(.rodata) - KERN_VIRT_BASE + KERN_PHYS_BASE) {
        *(.rodata*)
    }

    .data : AT(ADDR(.data) - KERN_VIRT_BASE + KERN_PHYS_BASE) {
        *(.data*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERN_VIRT_BASE + KERN_PHYS_BASE) {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;

        . = ALIGN(16);
        __kernel_stack_start = .;
        . += 16K;
        __kernel_stack_top = .;
    }
}
```

**这个 AT() 是必要的吗？** 是的——但原因和 Minix3 不同。这里 AT() 不是给 GRUB 用，
而是给 boot-shim 的计算公式用的。boot-shim 读 ELF 时：
```
phys = vaddr - KERN_VIRT_BASE + KERN_PHYS_BASE
```
这是一种**干净的计算关系**，不是 hack。如果不喜欢 AT()，也可以把物理地址放在一个单独的 section 中。

### 4.5 boot-shim 的 trampoline

在 `arch_boot_impl()` 的开分页之后：

```rust
// Step 4: Trampoline to high address.
// SAFETY: 恒等映射 + 高映射均已建立。当前 RIP 在低地址（恒等映射），
// 需要将 RIP 和 RSP 切换到高地址。
//
// C: head.S 中：
//   mov $k_initial_stktop, %esp   ← 切栈
//   call kmain                    ← 跳到高地址
unsafe {
    asm!(
        "mov rsp, {stack}",   // 切到高地址内核栈
        "jmp {entry}",         // RIP 跳到高地址 kmain
        stack = in(reg) kernel_stack_top,
        entry = in(reg) kmain_entry,
        options(noreturn)
    );
}
```

`kmain_entry = kern_virt_base + (elf_entry - KERN_VIRT_BASE)` = `elf_entry`（正确计算后）。
`kernel_stack_top` 来自链接脚本符号 `__kernel_stack_top`。

### 4.6 boot-shim 的 ELF 加载器（~50 行手写）

```rust
/// Minimal ELF64 loader for minix-kernel and boot modules.
/// Reads PT_LOAD segments and copies them to the correct physical pages.
fn load_segments(elf_bytes: &[u8], kern_virt_base: u64, kern_phys_base: u64)
    -> (u64 /* entry */, u64 /* phys_base */)
{
    let hdr = elf_header(elf_bytes);
    for phdr in program_headers(elf_bytes, hdr.phoff, hdr.phnum) {
        if phdr.p_type != PT_LOAD { continue; }
        let vaddr = phdr.p_vaddr;
        let phys  = vaddr - kern_virt_base + kern_phys_base;
        let src   = &elf_bytes[phdr.p_offset..][..phdr.p_filesz];
        copy_to_phys(PhysBytes(phys), src);
    }
    (hdr.entry, kern_phys_base)
}
```

不依赖第三方 ELF crate——只需要 5 个字段（p_type/p_vaddr/p_offset/p_filesz/p_entry），
手写 50 行比引入依赖更可靠。

### 4.7 文档重构方案

| 文档 | 当前问题 | 目标 |
|------|---------|------|
| **01-multiboot-bootstrap.md** | 缺 Rust 差异说明 | 加 §1.6: 无栈切换（UEFI 不需要）+ 高地址跳转待实现 |
| **02-page-table-kernel.md** | 运行时机制被当作 boot 续篇 | 开头声明"本文机制在 VM running 后才生效"；boot 阶段的页表操作移到 17-main-init |
| **17-main-init.md** | 参考资料式平铺 | 重构为时间线叙事：kmain→cstart→proc_init→arch_post_init→memory_init→bsp_finish_booting |
| **新增: 01b-bridge-to-kmain.md** | 可选 | 独立说明链接脚本、AT()、栈切换、ELF 加载机制（约 200-300 行） |

---

## 5. 关键设计决策记录

### 决策 1：独立 kernel ELF（不在 rlib 链接）

- **选项 A**：保持 rlib，添加 inline asm 跳转到 `kern_virt_base + 固定偏移`
  - 问题：RIP 跳过去了，但符号还是低地址的，调试困难
- **选项 B**：独立 ELF binary + 编译时嵌入 ✅ **选中**
  - 优势：VMA 真实、调试器正确、与大陆 OS 标准一致

### 决策 2：boot-shim 从 UEFI 分区读 boot 模块

- **选项 A**：所有 boot 模块嵌入 boot-shim
  - 问题：boot-shim 体积膨胀，更新一个服务器需要重新编译 boot-shim
- **选项 B**：ExitBootServices 前用 UEFI 文件系统读分区 ✅ **选中**
  - 优势：灵活、与 GRUB 的 `load_mods` 语义一致

### 决策 3：AT() 不保留

- **选项 A**：保留 AT()，保持和 Minix3 C 一致
  - 问题：AT() 晦涩难懂，且 minix-rs 全盘控制加载路径，不需要
- **选项 B**：诚实 VMA + boot-shim 算物理地址 ✅ **选中**
  - 优势：ELF 干净、逻辑在 Rust 代码中（易维护）

### 决策 4：ELF 加载器手写不依赖第三方

- **选项 A**：使用 `xmas-elf` crate
  - 问题：增加 boot-shim 的依赖链
- **选项 B**：手写 ~50 行 ✅ **选中**
  - 优势：依赖为零，ELF header 是极其稳定的规范

### 决策 5：17-main-init 重构为时间线叙事

- **选项 A**：保持参考资料式结构
  - 问题：读者无法跟随 boot 顺序理解 "为什么要先做 A 再做 B"
- **选项 B**：以 `kmain()` 行号为时间线组织 ✅ **选中**
  - 优势：与 01 的叙事衔接，读者从 boot_bootstrap 一路读到 switch_to_user

---

## 6. 与 Redox OS 的对齐程度

| 设计维度 | Redox OS | minix-rs (目标) |
|---------|----------|----------------|
| 内核是独立二进制 | ✅ | ✅ |
| bootloader 加载内核 ELF | ✅ | ✅ |
| 内核链接到高地址 | ✅ | ✅ |
| 开分页后切 RIP/RSP 到高地址 | ✅ | ✅ |
| 内核 BSS 有自己的栈 | ✅ | ✅ |
| AT() 技巧 | ❌ 不需要 | ❌ 不需要 |
| bootloader 读分区加载服务模块 | ✅ | ✅ |
| ELF 加载器在 bootloader 中 | ✅ | ✅ |

---

## 7. 未解决的问题 / 待办事项

- [ ] **高地址 trampoline 实现**：`arch_boot_impl()` 中开分页后的 asm 跳转
- [ ] **minix-kernel 链接脚本**：创建 `os/kernel/src/link.ld`
- [ ] **build.rs**：先编译 kernel binary，再嵌入 boot-shim
- [ ] **UEFI 分区读取**：在 ExitBootServices 前读 `/boot/vm.bin` 等
- [ ] **KernelInfo.boot_modules**：填充 boot 模块的物理地址列表
- [ ] **kmain() 实现**：空 `loop {}` 替换为真正的 boot 延续
- [ ] **17-main-init.md 重构**：时间线叙事重写
- [ ] **01-multiboot-bootstrap.md 补充**：加 Rust 差异说明
- [ ] **02-page-table-kernel.md 声明**：加叙事定位说明

---

## 8. 文档重构：新线性叙事结构

### 8.1 从 02 开始的文档已全部加 `tmp-` 前缀

原 02~21 号文档（保留 02-todo、02-design、99-global-concepts、00-overview、01-multiboot-bootstrap) 均已加上 `tmp-` 前缀。
这些文档内容本身没有错误，但叙事顺序不符合线性 boot 过程。暂不删除，作为参考资料。

### 8.2 新的线性文档映射

从 01 结束到 `switch_to_user()` 之间的所有步骤，严格按 Minix3 C 源码的 `kmain()` 行号顺序规划：

```c
// main.c — 完整的 boot 序列
96  void kmain(kinfo_t *local_cbi) {
      // ── 阶段 A: kmain 入口 ──
104     memcpy(&kinfo, local_cbi, sizeof(kinfo));
106     memcpy(&kmess, kinfo.kmess, sizeof(kmess));
111     kernel_may_alloc = 1;

      // ── 阶段 B: cstart — 架构初始化 ──
126     cstart();
        // prot_init() → GDT/IDT/TSS
        // init_clock() → 时钟初始化
        // intr_init() → 中断初始化
        // arch_init() → 架构初始化

      // ── 阶段 C: BKL + 进程表 ──
143     BKL_LOCK();
146     proc_init();   // 清空 proc 表
        for(i=0; i<NR_BOOT_PROCS; ++i) {
            // arch_boot_proc(ip, rp) — 设置进程 arch 状态
            // ★ VM 特殊处理：ELF 加载到 bootstrap 页表
        }

      // ── 阶段 D: arch_post_init + memory_init ──
283     arch_post_init();   // ptproc = VM, pg_info()
285     memory_init();      // freepdes 分配

      // ── 阶段 E: system_init + 回收 bootstrap ──
289     system_init();
293     add_memmap(&kinfo, bootstrap_start, bootstrap_len);

      // ── 阶段 F: bsp_finish_booting — 进入调度 ──
306     bsp_finish_booting();
          // announce(), 清 RTS_PROC_STOP,
          // init_timer(), fpu_init(), switch_to_user()
    }
```

### 8.3 新文档提案

| 编号 | 文档标题 | 覆盖范围 | 角色 |
|------|---------|---------|------|
| **00** | kernel-overview | 不变 | 整体架构 |
| **01** | multiboot-bootstrap | 不变，但结尾扩展 | GRUB→分页开启→Rust差异说明 |
| **02** | **boot-bridge** | **全新文档** | 过渡：从分页开启到 kmain——AT()、head.S 三行、ELF 加载、trampoline、高地址切换 |
| **03** | **elf-loader** | **全新文档** | boot-shim 的 ELF 加载器设计、boot 模块加载、KernelInfo.boot_modules |
| **04** | **kmain-entry** | **全新文档** | kmain() 入口逻辑：memcpy、kernel_may_alloc、BSS 检查 |
| **05** | **cstart** | **全新文档** | cstart()：prot_init + init_clock + intr_init + arch_init 的叙事整合 |
| **06** | **bkl-proc-init** | **全新文档** | BKL_LOCK、proc_init、boot_image 数组、RTS 标志初始化（VMINHIBIT/BOOTINHIBIT） |
| **07** | **arch-boot-proc** | **全新文档** | arch_boot_proc——VM ELF 加载到 bootstrap 页表、libexec_load_elf |
| **08** | **arch-post-init** | **全新文档** | arch_post_init：ptproc 设置、pg_info()、freepdes 分配 |
| **09** | **bsp-finish** | **全新文档** | system_init、add_memmap(bootstrap)、bsp_finish_booting、switch_to_user |
| **10** | **vm-boot-negotiate** | **全新文档** | VM 启动后协商：arch_enable_paging、arch_phys_map、VMCTL 协议 |
| **tmp-*** | 参考资料 | 保留不删 | 原文档内容正确但叙事错位，作为参考资料 |

**注意**：以上编号是临时占位。每个文档的编号可能在撰写过程中调整（拆分或合并）。

---

## 9. 文档 02-boot-bridge 详细设计提案

### 9.1 为什么值得独立成文档

之前认为"200-300 行"的短文档就够了，但进一步分析后发现**远不止 300 行**：

| 子主题 | 内容量预估 | 难度 |
|--------|-----------|------|
| AT() 机制详解 | ~100 行 | 中：需要讲清楚 VMA/LMA/加载器行为 |
| Minix3 head.S 三行深层分析 | ~150 行 | 高：为什么要切栈？不切会怎样？call 的 PC-relative 机制 |
| minix-rs 独立 ELF 设计 | ~100 行 | 中：链接脚本 + build.rs + 嵌入 |
| ELF 加载器设计 | ~150 行 | 中：Program Header 解析、物理地址计算、拷贝 |
| boot-shim trampoline | ~100 行 | 高：asm 切栈 + jmp 到高地址，为什么两件事必须同时做 |
| UEFI 加载 boot 模块 | ~150 行 | 中：文件系统读分区、AllocatePages、ExitBootServices 时序 |
| 与 Redox 对比 | ~50 行 | 低：架构对齐 |
| **合计** | **~800 行** | |

### 9.2 标题建议

- `02-boot-bridge.md` — 从分页开启到 kmain 的桥梁
- 副标题：AT()、ELF 加载、高地址切换的原理与设计

### 9.3 结构草案

```
## 1. 概述
### 1.1 问题：分页开完了，然后呢？
### 1.2 本章覆盖的 C 源码全景
### 1.3 三行代码改变一切（head.S 的 mov/call）

## 2. C 源码分析
### 2.1 链接脚本 kernel.lds：VMA vs LMA
  - _kern_phys_base = 0x400000 / _kern_vir_base = 0xF0400000
  - AT() 指令的作用
### 2.2 head.S：从恒等映射到高地址
  - mov $k_initial_stktop, %esp：为什么切栈
  - push %eax：传递 kinfo
  - call kmain：PC-relative 跳转
### 2.3 为什么只有内核需要这么做
  - 服务器进程（VM/PM）是普通 ELF，不需要高地址

## 3. Rust 设计决策
### 3.1 为什么不需要 AT()（全盘控制 boot-shim）
### 3.2 独立 ELF vs rlib 链接
### 3.3 手写 ELF 加载器（不依赖第三方）

## 4. 实现详解
### 4.1 minix-kernel 的链接脚本
### 4.2 boot-shim 的 ELF segment 加载
### 4.3 恒等映射 + 高映射建立
### 4.4 asm trampoline（切栈 + 跳转）
### 4.5 UEFI 分区读取 boot 模块

## 5. 测试要点
## 6. 参见
```

---

## 10. kmain() 分解分析：proc_init 之前还有多少内容

在 `kmain()` 能够调用 `proc_init()` 和 `arch_boot_proc()`（加载 VM）之前，
必须先执行 `cstart()`。而 `cstart()` 本身是多个子系统初始化的容器：

```
kmain()
  │
  ├── memcpy(kinfo), kernel_may_alloc=1          ← 02-todo.md L21
  │
  ├── cstart()
  │     ├── prot_init()      ← GDT/IDT/TSS     ← 当前在 tmp-04-protection
  │     ├── init_clock()     ← 时钟变量        ← 当前在 tmp-16-timer
  │     ├── intr_init()      ← 中断向量        ← 当前在 tmp-05-exception-interrupt
  │     └── arch_init()      ← 架构初始化      ← 当前在 tmp-17-main-init
  │
  ├── BKL_LOCK()             ← 首次获取 BKL    ← 当前在 00-overview §1.5
  │
  ├── proc_init()            ← 清空 proc 表     ← 当前在 tmp-06-proc-struct
  │
  └── for: arch_boot_proc()  ← 加载 VM         ← 当前在 tmp-04-protection
```

### 10.1 现有文档的内容覆盖（只是叙事错位）

| 子系统 | 函数 | 当前文档 | 覆盖情况 | 新叙事中的角色 |
|--------|------|---------|---------|---------------|
| 保护模式 | `prot_init()` | tmp-04-protection | ✅ 内容完整 | 在 **kmain→cstart** 时间线中引用 |
| 时钟 | `init_clock()` | tmp-16-timer | ⚠️ 需确认 | 在 **kmain→cstart** 时间线中引用 |
| 中断 | `intr_init()` | tmp-05-exception-interrupt | ✅ | 在 **kmain→cstart** 时间线中引用 |
| 架构初始化 | `arch_init()` | tmp-17-main-init | ⚠️ 需确认 | 在 **kmain→cstart** 时间线中引用 |
| BKL | `BKL_LOCK()` | 00-overview §1.5.1 | ✅ 概念 | 在 **kmain 阶段 C** 中强调"首次获取" |

### 10.2 关键判断：不需要为 cstart 内的每个子系统新建文档

`cstart()` 中的 `prot_init()` / `init_clock()` / `intr_init()` / `arch_init()` 已经有
独立的参考资料文档（tmp-*）。在**新线性叙事中**，只需要一篇 **cstart 文档**（§8 中的 #05-doc）
来完成：

1. 说明"kmain 调用了 cstart"
2. 列出 cstart 初始化了哪些子系统
3. 引用各子系统的参考资料文档
4. 重点说明初始化顺序依赖关系

**不需要为每个子系统再写一篇"启动视角"的文档**——那会大量重复现有内容。

### 10.3 真正需要拆分的是这些

| 需要新建 | 原因 | 原来在哪 |
|---------|------|---------|
| **02-boot-bridge** | AT + ELF 加载 + trampoline——全新知识 | 不存在 |
| **03-elf-loader** | boot-shim 读分区 + 解析 ELF + 物理拷贝 | 不存在 |
| **04-kmain-entry** | kmain 入口的前几步：memcpy、BSS check | tmp-17-main-init 中埋没 |
| **05-cstart** | cstart 的叙事整合 | 分散在多个 tmp-* 中 |
| **06-bkl-proc-init** | BKL + proc_init + boot image 数组 | 分散在 tmp-06/17 中 |
| **07-arch-boot-proc** | VM 的 ELF 加载 + arch_boot_proc | tmp-04-protection 中埋没 |
| **08-arch-post-init** | ptproc / pg_info / freepdes | **tmp-02-page-table-kernel 中埋没** |
| **09-bsp-finish** | system_init + add_memmap + bsp_finish_booting | tmp-17-main-init 中埋没 |
| **10-vm-boot-negotiate** | VM 启动后协议 | **tmp-02-page-table-kernel 中埋没** |

### 10.4 和 00-overview 的阶段映射

新文档与 00-overview.md §2 的阶段划分完全对齐：

| 阶段 | 00-overview 描述 | 新文档 |
|------|-----------------|--------|
| 阶段 1: 硬件发现 | GRUB→内存map→大页恒等→开分页 | **01** (已有) |
| *新增过渡* | （从分页开启到 kmain 的桥梁） | **02 + 03** (新建) |
| 阶段 2-4: kmain | 保护模式/进程表/IPC/时钟 | **04 + 05** (新建) + tmp-* (引用) |
| 阶段 3: 保护+中断 | 实际上是 cstart 的内容 | 引用 tmp-04/05 |
| 阶段 4: 内核主流程 | kmain→bsp_finish_booting | **06→09** (新建) |
| 阶段 5: VM 协商 | VM 启动后的页表协商 | **10** (新建) |