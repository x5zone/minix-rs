# 02 · 内核住进高半区：链接、加载与成对跳转

> **C 真值源**: `minix3/minix/kernel/arch/i386/kernel.lds`、`minix3/minix/kernel/arch/i386/head.S`
> **Rust 实现**: `os/kernel/src/boot/higher_half.rs`、`os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs`、`os/kernel-image/src/{main.rs,bootface.rs}`、`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`

## 第 1 章　概念：内核明明被放在低物理内存，凭什么在高虚拟地址运行

### 1.1 问题：一个上不去也下不来的中间态

01 文档结束时，boot-shim 已把控制权交到 `arch_boot`，分页也已打开。但有一个容易被忽略的事实：分页刚启用那一刻，CPU 的指令指针和栈指针**还停在低地址**——因为 `arch_boot` 自己就是被 boot-shim 在低地址上调起来的。系统卡在中间态：页表就绪，可 RIP/PC 与 RSP/SP 仍在低地址窗口。

这个中间态不能直接 `call kmain`。`kmain` 的符号地址是高地址没错，但那条 `call` 会先把**返回地址**压进**当前的低地址栈**；一旦稍后恒等映射被撤、栈操作要落回高半区，就会踩空。要干净地进高半区运行，必须同时做两件事：**把栈指针切到高地址**、**把指令指针跳进高地址**，而且二者要**成对**完成。把执行体从低地址窗口平稳移进高半区的那个动作，就是 `HigherHalf::jump_to_kmain`。

### 1.2 为什么只有内核需要高半区

在微内核架构里，只有内核常驻高地址（x86-32 是 `0xF0400000`，x86-64 是 `0xFFFF_8000_0000_0000` 一带）；VM/PM/VFS 等进程各有各的低地址用户空间。内核必须上高地址有三个理由：**给进程腾出尽可能大的低地址空间**；**高半区的内核页对用户态不可见**（supervisor-only）；**高半区还能腾出一段 direct-map 窗口**让内核够到全部物理内存。这三条对三架构都成立，只是地址数值不同。

### 1.3 物理代码从未移动

这是理解整篇的关键认知，值得单独画出来。内核代码自被加载进物理内存那刻起就**一直待在原地**；所谓"跳进高地址"，不是把代码搬走，而是让 CPU **换一扇映射窗口去看同一块物理内存**：

```
低地址窗口（恒等映射 VA=PA）:   VA 0x0020_0000           ┐
                                                        ├──► 同一块 PA 0x0020_0000 [内核代码]
高地址窗口（内核映射）:         VA 0xFFFF_8000_0020_0000  ┘

jump_to_kmain 做的，只是让 CPU 从上面那扇窗挪到下面那扇窗去看同一段代码。
```

既然物理位置没变，那"该在哪个虚拟地址运行内核"就是**链接期**决定的事——这就引出第二章的链接脚本。

### 1.4 三件事必须配合

> **缩写**：VMA＝链接器为符号解析用的运行时地址；LMA＝bootloader 把段实际放进物理内存的地址。

高半区内核是三样东西协同的结果，缺一不可：

| 组件 | 职责 | 它错了会怎样 |
|---|---|---|
| 链接脚本（`kernel.lds` / `link.ld`） | 定 VMA=高地址、LMA=低物理 | 内核符号地址错，调任何函数都跳到错处 |
| ELF 加载器（boot-shim） | 按 LMA（`p_paddr`）把段放进物理内存 | 内核代码不在预期物理地址 |
| `HigherHalf` trait | 切栈 + 跳进高地址 kmain | RIP/RSP 留在低地址，撤恒等映射就崩 |

一句话：**链接脚本决定"内核以为自己在哪"，加载器决定"内核实际被放在物理内存的哪"，`HigherHalf` 决定"CPU 从哪扇窗执行内核"。**

> **认知减负**：链接脚本的完整语法（`PHDRS`、`SECTIONS` 排序、`ALIGN` 细节）不是本篇重点，只需读懂 VMA/LMA/`AT()` 这组机制即可；更细的段布局属工具链文档。

---

## 第 2 章　C 源码：Minix3 怎么摆好这三样

### 2.1 kernel.lds：VMA、LMA 与 `AT()`

`minix3/minix/kernel/arch/i386/kernel.lds` 用三个符号把"运行地址"和"加载地址"分开：

```
_kern_phys_base = 0x00400000;   // LMA：内核在物理内存的起点（4MB 对齐）
_kern_vir_base  = 0xF0400000;   // VMA：内核认为自己在虚拟空间的位置
_kern_offset    = (_kern_vir_base - _kern_phys_base);   // 偏移 = 0xF0000000
```

真正让二者分离的是段定义里的 `AT()`：

```ld
.text : AT(ADDR(.text) - _kern_offset) { *(.text*) }
```

`.text` 的 VMA 是高地址（链接器按它解析符号），而 `AT(...)` 把 LMA 拨回低物理地址（GRUB 按它加载）。效果就是：内核所有符号都指向高地址，可代码实际被放在低物理地址；分页一开，CPU 顺着高地址映射去取指，落到那块低物理内存上。

> **辨析：为什么 phys 与 vir 恰好差一个常量 offset。** 因为链接器把每个符号的"取地址"按 VMA 算，而运行时页表把这段 VMA 映回 LMA=VMA−offset。只要 offset 全局一致，代码里对任意全局变量/函数的引用都能对上——高半区映射正是按这个统一偏移铺的。这也解释了为什么 ARM32 换了一块完全不同的物理基址（`_kern_phys_base = 0x80200000`），`_kern_vir_base` 仍是 `0xF0400000`：**变的只是 LMA，VMA 与"高半区"这个语义不变。**

C 版链接脚本还留着 `.unpaged_text/data/bss` 段组。32 位的内核必须先在**不分页**的低地址跑起来（`pre_init`、`pg_mapkernel` 这些在页表建好前执行的代码不能在高地址，否则取指即缺页），所以专门留一个 unpaged 段承载它们，跳高地址后再回收。**这正是 01-boot-shim-bootstrap.md §3.7 里那个在 64 位 Rust 下失去实体的 bootstrap 概念的出处**——Rust 版这些职责全在 boot-shim 这个独立二进制里（天然跑在低物理地址），内核 ELF 从 `kmain` 起就直接在高地址。

### 2.2 head.S：从 pre_init 返回后，就那几行

`minix3/minix/kernel/arch/i386/head.S` 在 `pre_init` 返回（分页已开、但 RIP/RSP 还在低地址）之后做四件事（入口 `minix3/minix/kernel/arch/i386/head.S:multiboot_init`）：

```asm
        /* 分页已启用，但 RIP/RSP 仍在低地址 */
        mov     $k_initial_stktop, %esp   // (1) 切栈到高地址
        push    $0                        // (2) 栈终止标记
        push    %eax                      // (3) 压入 kinfo 指针
        call    _C_LABEL(kmain)           // (4) 跳到高地址的 kmain
```

`call kmain` 之所以能跳进高地址，是因为链接器早把 `kmain` 的地址解析成了高地址——这条 `call` 的立即数在链接时就定了，运行时不计算。ARM32 的 `head.S` 语义完全相同，只是 `bl` 立即数范围不够，改用 `ldr r2, =kmain` + `bx r2` 间接跳；栈同理从低地址 `ldr sp, =k_initial_stktop` 切过去。

### 2.3 切栈与跳高地址必须成对

这是本篇最"顺序敏感"的一点。四种做法里只有一种对：

| 做法 | 后果 |
|---|---|
| 只切栈不跳 | 栈就位了，但下一条指令仍在低地址窗口，撤恒等映射即崩 |
| 只跳不切栈 | RIP 在高地址，RSP 还在低地址，一 push/call/ret 就落回低地址、撤映射即崩 |
| 先跳后切 | 跳过去第一件事若是压栈，栈还指着低地址——可能写坏高半区映射 |
| **先切栈后跳** | ✅ 栈先就位，跳转后首条高地址指令的执行与栈操作都在高半区 |

C 版严格采用**先切后跳**。这不是风格偏好，是硬件决定的：跳过去就回不了头，栈必须比指令指针先到位。

### 2.4 k_initial_stktop：一个链接期符号

`k_initial_stktop` 指向内核 BSS 末尾预留的初始栈，其 VMA 是高地址，所以天生就是高栈顶。**minix-rs 换了做法**：不再依赖链接脚本符号，栈顶作为参数 `stack_top: VirBytes` 显式传给 `jump_to_kmain`，其值来自 `KernelInfo.kern_stack_top`（由 boot-shim 构造）。这样内核代码与链接脚本解耦，mock 测试也能直接传任意栈顶值——是一次"把隐式约定变显式参数"的 Rewrite。

---

## 第 3 章　Rust 设计：把跳转抽象成 trait

### 3.1 取舍：不保留 `AT()` 那套双地址手工偏移

| 方案 | 做法 | 取舍 |
|---|---|---|
| **不保留 AT()（选用）** | 内核 ELF 链接进高半区，加载与映射由 boot-shim/分页层负责 | 64 位下 high-half + direct-map 模型让"运行时地址=链接地址"更统一，省掉 C 那套 phys/vir offset 手工账 |
| 照搬 AT() | 沿用 `_kern_offset` 手工对齐 | 把 32 位的地址空间窘迫移植进 64 位，无收益 |

### 3.2 独立 kernel.elf

01-boot-shim-bootstrap.md §3.3 已讲清 boot-shim 与内核是两个镜像、方案 A 用 `BootHandoff`（含 `bump_next` 游标）跨镜像交状态。本篇补上镜像**这一侧**：`kernel.elf` 的 `_start` 接住 x0 递来的 blob 指针，再往下就是本章的高半区落位与 `jump_to_kmain`。

### 3.3 HigherHalf trait：把三种跳转写法收进一个签名

```rust
pub trait HigherHalf {
    /// 切栈到高地址 + 跳进高地址 kmain。有去无回。
    ///
    /// # Safety
    /// 调用方须保证：分页已开且恒等与内核高映射都在；`kinfo` 在高地址有效；
    /// `stack_top` 是高半区里 16 字节对齐的合法栈顶；只在 boot CPU 上调一次。
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !;
}
```

（`os/kernel/src/boot/higher_half.rs:trait HigherHalf`）

**为什么用 trait 而非 `#[cfg(target_arch)]` 散函数**：三家的具体写法根本不同（x86-64 `mov rsp`+`call`、aarch64 `mov sp`+`br`、riscv64 `mv sp`+`jalr`），trait 把这层差异封装进各自 impl；`arch_boot_impl` 统一调 `HigherHalf::jump_to_kmain`，不写条件编译；mock 实现还能不执行真指令就验证调用序。

### 3.4 三架构差异对照：机制各异，语义同一

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 高半区靠什么 | 长模式 + 高地址页表 | TTBR1（永不被根切换动） | Sv39 高半区（VPN[2]≥256） |
| 跳转写法 | `mov rsp,栈顶`→`call kmain` | `mov sp`→`br` | `mv sp`→`jalr` |
| 页表根 | CR3（单根） | TTBR0/TTBR1（双根） | satp（单根） |

三家要达成的 OS 语义一模一样：**把执行体搬进一段永不受进程根切换影响的内核虚拟空间**。差异是硬件形状（寄存器名、根数量、跳转指令），不该漏进 OS 层——这与 01-boot-shim-bootstrap.md §4.4 的收口同源（跨架构四公理，`review-core-semantics.md` §1.6）。

---

## 第 4 章　实现：跳转真正落地的那一刻

### 4.1 链接脚本与 ELF 加载

各架构的 `link.ld` 把内核段 VMA 定在高半区，LMA/落位由 boot-shim 的加载器按 `p_paddr` 处理：boot-shim 解析 `kernel.elf` 的 PT_LOAD 段、逐段拷进物理内存、清零 BSS，再建好高半区映射——于是内核符号（`kmain` 等）在链接期就是高地址，运行时那份映射把它们落到物理代码上。

### 4.2 成对跳转的 x86-64 实现

`X86_64HigherHalf::jump_to_kmain` 是一段内联汇编，逐行对应 §2.3 的"先切栈后跳"（`os/kernel/src/arch/x86_64/higher_half.rs:fn jump_to_kmain`）：

```rust
core::arch::asm!(
    "mov rsp, {stktop}",     // (1) 切栈到高地址
    "and rsp, -16",          // (2) 16 字节对齐（System V ABI）
    "xor rbp, rbp",          // (3) 清帧指针
    "push 0",                // (4) 栈终止
    "call {kmain}",          // (5) 跳进高地址 kmain
    stktop = in(reg) stack_top.0,
    kmain  = sym kmain,
    in("rdi") kinfo,         // kinfo 按 ABI 走 RDI 传参
    options(noreturn),
);
```

`-> !` + `options(noreturn)` 在类型层面声明"有去无回"，编译器不会在其后再生成代码。这里藏着一个 01-boot-shim-bootstrap.md §4.1 讲过的坑的镜像面：**`in("rdi") kinfo` 把参数寄存器钉死**——因为 `kinfo` 是引用，若让它经普通寄存器过这段 `noreturn` 汇编，中间任何 spill/复用都可能覆写它。

### 4.3 内核镜像这一侧

跨镜像方案 A 里，boot-shim 写完 `BootHandoff`、`br` 进高半区 `_start` 后，`kernel.elf` 侧走 `arch_boot_resume_high_half`（`os/kernel/src/lib.rs:fn arch_boot_resume_high_half`）：只做纯增量（校验 KernelInfo、续 bump、落进本镜像 `.bss`、建 direct map、进 `kmain`），**绝不重建页表**——那个根正活着映射着这段代码（原因见 01-boot-shim-bootstrap.md §4.2）。

### 4.4 完整启动流程：把 01 和本篇接起来

```
上电/复位
  └─ 固件：加载 kernel.elf + boot 模块 → 交给 boot-shim
       └─ boot-shim（低物理地址执行）
            ├─ 构造 KernelInfo / 收集 reserved_regions、platform_sources
            ├─ build bootstrap 页表：恒等映射 + 内核高半区映射
            ├─ 启用分页（此刻 RIP/RSP 仍在低地址窗口）
            └─ arch_boot
                 └─ HigherHalf::jump_to_kmain   ← 本篇
                      ├─ (1) 切栈 → 高地址栈顶（先）
                      └─ (2) 跳转 → 高地址 kmain（后）
                           └─ kmain：内核从此全程跑在高半区
```

一句话：**boot-shim 摆好映射与栈，`jump_to_kmain` 让 CPU 从低地址窗口"换窗"进高半区，先切栈后跳、成对完成，kmain 起内核便常驻高半区。**

---

## 第 5 章　测试要点

跳转正确性由端到端引导测试守：`os/qemu-tests/test-kernels/kernel/bootstrap/` 下的 `test-higher-half`（x86-64/aarch64/riscv64 三变体）跑真固件路径，验证切栈+跳转后 `kmain` 能在高半区起来；`os/kernel/tests/boot_integration.rs` 覆盖 `arch_boot` 到 `kmain` 的衔接。**测试数以现算为准**：`rg -c '#\[test\]' os/kernel/tests/`、引导内核目录 `ls os/qemu-tests/test-kernels/kernel/bootstrap/ | grep higher-half`，不抄表。

> **认知减负**：真机门依赖 QEMU，宿主 `cargo test` 只编到 `HigherHalf` 的签名与 mock 序；跳转指令本身的可执行验证只在真机上，读者在纯软件环境跑不到属正常。

---

## 第 6 章　诊断：高半区跳转这一跳，最容易栽在哪

### 6.1 成对性没满足长什么样

回看 §2.3 的四种错法，落到现象上是两类：只跳不切，内核进 `kmain` 后第一次压栈就写到低地址、撤恒等映射后随机崩；只切不跳，撤恒等映射后下一条取指即缺页。定位靠"崩在取指还是崩在栈操作"区分是哪一半没做到。

### 6.2 一个真实故障：riscv64 高半区映射踩了恒等映射的槽

riscv64 的一组引导测试曾出现"无限重复打印启动横幅、最终 PANIC"——这不是随机 flake，是高半区映射与恒等映射在页表根上撞了同一个槽。

> **反例推演（讲清它为什么必然发生）。** Sv39 是三级页表（L2→L1→L0），根表 L2 每项覆盖 1GB，索引是 `l2_index(vaddr) = (vaddr >> 30) & 0x1FF`；且 bits[63:39] 必须全 0 或全 1，于是 512GB 虚拟空间被对称切成低半区（VPN[2]=0–255）与高半区（VPN[2]=256–511）。恒等映射铺 `L2[0..3]`（覆盖 0–4GB），内核高映射本应落在**高半区那半边**。可当某个测试把 `kern_virt_base` 直接等于 `kern_phys_base`（都是 `0x8000_0000`），高半区映射的 `l2_index` 算下来落进了 `[0..3]`——**和恒等映射抢同一个 L2 槽**，一个把另一个盖掉，映射错乱，CPU 换窗后取指落到不该落的地方，于是重启又打印、又 PANIC。教训：**"高地址"不只是一个数值，而是"落在页表根的另一个半区"**；identity 测试偷懒让 virt==phys 时，必须确认它没踩进恒等映射占用的根槽。

### 6.3 可复制的定位命令

```bash
# 跑高半区跳转的端到端回归（预期：串口打出 PASS 标记；反复打印启动横幅即命中 §6.2 那类映射错乱）
os/qemu-tests/run_qemu.sh <arch> <test-kernel>   # 例 x86_64 + 对应 .efi

# 看某架构高半区映射用的 l2/vpn 索引算式，核对它是否落在高半区那半边
rg -n "l2_index|VPN\[2\]|>> 30|KERN_VIRT|kern_virt_base" os/arch/src/riscv64/ os/kernel/src/arch/riscv64/
```

预期输出的关键行：正常是**一次** PASS 标记、无重复横幅；若看到启动横幅反复出现，就是 §6.2 的映射撞槽，去核 `l2_index` 是否落进了恒等映射占用的根槽。
