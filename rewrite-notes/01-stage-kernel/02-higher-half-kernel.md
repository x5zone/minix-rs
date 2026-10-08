# 02 · 高半区：同一份字节的两种编号，与一次不能拆开的跳转

> **C 真值源**：`minix3/minix/kernel/arch/i386/kernel.lds`、`minix3/minix/kernel/arch/i386/mpx.S`、`minix3/minix/kernel/arch/i386/head.S`
> **Rust 实现**：`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`、`os/kernel-image/{x86_64,aarch64,riscv64}.ld`、`os/kernel-image/check-layout.sh`、`os/boot-shim/src/loader.rs`、`os/kernel/src/boot/higher_half.rs`、`os/kernel/src/arch/*/higher_half.rs`、`os/kernel/src/lib.rs`

---

## 1. 概述

### 1.1 一个镜像、两套地址

01 篇建立了两件事：启用地址翻译之前，页表里必须已经覆盖"接下来要取指、取栈的地址"；引导期先铺一段虚拟等于物理的过渡映射，再铺内核真正要待的高地址段。那一篇把"高地址段要提前铺好"当事实使用，没有回答它从哪来。

这一篇回答它。出发点是一个具体的别扭：**同一个内核文件里，同时存在两套地址，而它们在文件里都是数字。**

- 一套回答"这些字节要放在物理内存的哪里"。程序还没跑，内存只有物理地址，所以装载只能用这一套。
- 另一套回答"代码里的符号、指针、跳转目标指向哪里"。程序跑起来之后，处理器给出的每个地址都要过页表，所以引用只能用这一套。

于是有三个问题必须回答，本篇按顺序处理：

1. 这两套地址在构建时怎么写进一个文件？（第 2 章看 C，第 4.1 节看重写的两种脚本形态）
2. 谁负责按第一套摆放字节，谁负责把第二套接到第一套上？（第 4.2 节）
3. 从"按第一套地址执行"切换到"按第二套地址执行"的那一步，为什么不能拆开？（第 2.3、2.4、4.3 节）

### 1.2 为什么内核要放在高地址

先只看地址空间这件事。一个 32 位进程的虚拟地址空间是 4 GB。如果内核放在低地址，那么每次进程切换都要重新映射内核常驻的那部分；而内核常驻的东西（代码、静态数据、直接映射窗口）与"哪个进程正在运行"无关。把它放在地址空间的高段，低段整片留给用户态，进程页表里"用户那部分"怎么改都不碰内核，这是高半区的唯一动机。

Minix3 的 32 位布局把这条写得很直：内核虚拟基址 `0xF0400000`，注释给的理由是"map kernel high for max. user vir space"（把内核映射到高处以最大化用户虚拟空间）。同一份文件里另一条注释说明物理基址取 `0x00400000` 是为了"phys 4MB aligned for convenient remapping"。

这里有一个可以现在就问、要到后面才答的问题：**内核放高地址之后，它那些按高地址写死的符号，在装载完成、页表还没建好的那段时间里怎么执行？** C 的答案是"某些代码不放到高地址"（第 2.2 节），另一种做法见第 3.5 节。

### 1.3 一个段的两个编号，分别从谁的角度看

```
一个可执行文件里的段（segment）
  │
  ├─ 装载视角：这段字节要放到物理内存的哪个地址？ ──┐
  │                                                 │ 同一份字节，
  └─ 执行视角：这段代码里的符号、跳转目标指向哪？ ──┘ 两个编号
                          │
                          ▼
        页表把"执行编号"接到"装载编号"上
                          │
                          ▼
        切换执行位置的时机：映射就绪、栈就绪，两者缺一不可
```

图上没有"把代码复制到高地址"这一步。这不是省略：**字节从头到尾没有被搬运过**，"在高地址执行"靠的是映射。这一点在第 2.2 节会以链接脚本的写法再次出现，在第 4.2 节会以装载器代码再次出现。

### 1.4 本篇范围与可以安全跳过的内容

本篇只到"跳进 `kmain` 之前"为止。以下内容不讲，可以安全跳过而不影响本篇结论：

- **过渡映射本身与启用地址翻译的位序**：01 篇的 2.6、2.7、4.2 已建立，本篇直接引用其结论。
- **运行期页表、进程地址空间与页帧管理**：属于页表与内存篇。本篇只用到"内核映射必须存在于将使用的根表里"。
- **QEMU + 调试器的逐指令验证配方**：属于测试篇；本篇只列出宿主侧能自动判定什么、不能判定什么。
- **ELF 解析器的内部**（文件读接口抽象、零分配设计、错误分类）：本篇只用到"段的 `vaddr`/`paddr`/`filesz`/`memsz` 四个字段被怎么用"，其余跳过不影响理解。

> **一处名词约定**（避免在三种叫法之间迷路）：本篇用"装载地址"指字节实际所在的物理地址、"执行地址"指代码引用符号时用的虚拟地址。它们在工具链与文档里另有缩写（分别对应 `paddr` 与 `vaddr` 两个字段名）；看到 `AT()`、`p_paddr`、`LMA` 说的是装载地址，看到 `. = 某高地址`、`p_vaddr`、`VMA` 说的是执行地址。

---

## 2. C 源码分析

### 2.1 三条赋值就把双址布局写定了

`minix3/minix/kernel/arch/i386/kernel.lds` 的开头：

```ld
OUTPUT_ARCH("i386")
ENTRY(__k_unpaged_MINIX)

_kern_phys_base = 0x00400000;	/* phys 4MB aligned for convenient remapping */
_kern_vir_base =  0xF0400000;	/* map kernel high for max. user vir space */
_kern_offset = (_kern_vir_base - _kern_phys_base);
```

前两行是基址，第三行是**两者之差**——它才是这一节真正的主角。拿它做一次算术：

```text
0xF0400000 - 0x00400000 = 0xF0000000
```

一个漂亮的数字：`_kern_offset` 恰好是 `0xF0000000`，也就是说高 4 位是 F 的地址都是"内核自己的虚拟地址"，而物理地址落在低 4 MB 附近。1.2 提到的"用户地址与内核地址一眼可辨"，在链接层就已经成立了一半（另一半是 01 篇 2.3 那个 `USR_STACKTOP`）。

再注意 `ENTRY(__k_unpaged_MINIX)`：入口符号带着 `__k_unpaged_` 前缀。也就是说，内核入口位于一个不参与高地址映射的区域——它在 2.2 节里会以分界线的形式再现。

### 2.2 两段式布局：先按装载地址摆一段，再整体抬到执行地址

同一文件的 `SECTIONS`（节选，保留全部关键行，省略的只有段内成员列举）：

```ld
SECTIONS
{
	. = _kern_phys_base;
	__k_unpaged__kern_unpaged_start = .;

	.unpaged_text : { unpaged_*.o(.text) }
	.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }
	.unpaged_bss  ALIGN(4096) : { unpaged_*.o(.bss COMMON) }
	__k_unpaged__kern_unpaged_end = .;

	. += _kern_offset;

	. = ALIGN(4096); usermapped_start = .;
	.usermapped_glo : AT(ADDR(.usermapped_glo) - _kern_offset) { *(.usermapped_glo) }
	...
	.text             : AT(ADDR(.text) - _kern_offset) { *(.text*) }
	.data ALIGN(4096) : AT(ADDR(.data) - _kern_offset) { *(.data .rodata* ) }
	. = ALIGN(4096);
	.bss ALIGN(4096)  : AT(ADDR(.bss) - _kern_offset) { *(.bss* COMMON)
		__k_unpaged__kern_size = . - _kern_vir_base;
		_kern_size = __k_unpaged__kern_size;
	}
	_end = .;
}
```

读这段只需要盯住一个位置：`. += _kern_offset;`。

它之前，位置计数器 `. ` 在 `_kern_phys_base`（4 MB 那个物理地址）上——也就是说 `.unpaged_text` 这些段的**执行地址就是装载地址**，没有抬升。它之后，执行地址被抬到 `0xF04xxxxx`，而每个段又各自写了一个 `AT(ADDR(该段) - _kern_offset)`：`ADDR()` 取的是抬升后的执行地址，减去偏移量就回到装载地址。

于是两条线索合成一个结论：

**`AT()` 在这里同时给出两套编号——段头里的执行地址给链接器算符号用，`AT()` 里的装载地址给装载器摆字节用；而 `unpaged` 段故意不给 `AT()`，因为那部分代码执行时就待在装载地址上。**

这正好回答 1.2 留下的问题：启用翻译之前，处理器只能用"数值上等于物理地址"的地址跑，所以那段必须留在低地址；启用之后要用的代码才抬到高地址。C 的答案是**把内核切成两半**——一半按装载地址执行，一半按执行地址执行，中间那条 `. += _kern_offset` 就是分界线。

第三处细节在 `.bss` 内部：`_kern_size = . - _kern_vir_base;`。内核大小是"当前位置减执行基址"，也就是说它是**执行地址坐标下的跨距**。这个量在第 4.2 节会以另一种方式重算一次，两种算法的差别值得对照。

### 2.3 换栈与跳转必须一起做

01 篇 2.2 引过 `pre_init` 返回之后的那几行，这里把它当成本篇的主角重读（`minix3/minix/kernel/arch/i386/head.S:multiboot_init` 的后半，逐字）：

```asm
	mov	$k_initial_stktop, %esp
	push	$0	/* Terminate stack */
	push	%eax
        call    _C_LABEL(kmain)
```

`pre_init` 返回时，翻译已经启用，`eax` 里是启动信息结构的地址。四行各自做的事：换栈、放栈底标记、把指针压成新栈上的第一个参数、调用主流程。

**"成对"是指前两行与最后一行必须一起发生。** 分别拆开试一次：

*只换栈、不跳高地址*：`mov $k_initial_stktop, %esp` 把栈顶换到高半区的执行地址之后，紧跟的 `push $0` 就要往这个地址写。一旦高半段没在那张表里（也就是 `pg_mapkernel` 没在启用翻译之前做完），**出事的第一条指令就是这条 `push`——栈比跳转更先需要新映射。**

*只跳高地址、不换栈*：`call kmain` 能跳过去（高地址已被映射），但栈还停在低地址那块临时栈上——就是 01 篇 2.1 里那块 `.space 4096`，而它在数据段里。此时不会当场出错，而是**延迟耗尽**：内核主流程的深调用与中断嵌套会把它用尽。所以换栈不是"顺手做的事"，它是跳转能安全持续下去的前置条件。

两个反推合起来给出本篇最重要的一条次序：**在启用翻译之后，先换栈、再跳转；而换栈能用新地址的前提，是高半段已经在那张表里。**这条次序与"代码在高地址执行"是同一件事的两面。

### 2.4 初始栈顶的落点：源码在这里给出两条相反线索

`head.S` 引用的符号是 `k_initial_stktop`。它的定义在 `minix3/minix/kernel/arch/i386/mpx.S`（节选）：

```asm
.bss
k_initial_stack:
.space	K_STACK_SIZE
LABEL(__k_unpaged_k_initial_stktop)
```

两条线索：

1. **它紧跟在 `k_initial_stack` 之后**，位于 `.bss`。第 2.2 节看到 `.bss` 在分界线**之后**，因此它的执行地址是 `0xF04...` 一侧的高地址——按这条线索，初始栈顶在高地址区。
2. **它的名字带着 `__k_unpaged_` 前缀**，而第 2.1 节的入口符号同样带这个前缀、`.unpaged_bss` 段的成员也带这个前缀，且那些段**不被抬升**——按这条线索，这块栈在低地址区。

这两条不能同时成立，要判定就得追清构建期怎么处理这个前缀（Minix3 用 `.unpaged_*.o` 与符号改名把"不分页"这一类区分开，`LABEL()` 宏与改名规则在构建脚本里，本篇**没有追到那条规则的实现**）。因此本篇把结论停在可核实的范围内：

- **可以断言**：这块栈在链接布局里被当作"与 `unpaged` 同一类"的东西命名，说明写它的人认为它与"翻译启用前后都能用"这件事有关；`head.S` 用的名字是不带前缀的 `k_initial_stktop`，说明两者之间存在一条构建期的对应关系。
- **不可断言**（标 `[待验证]`）：这块栈最终落在低地址还是高地址。判定它需要追 `LABEL`/改名规则，或直接在真机上打印换栈后的 `esp` 值。

为什么值得花一节写这个"不知道"？因为**它是本篇唯一一处两条源码线索相互矛盾的地方**。只取其中一条（比如只看它在 `.bss`）就能得出一个看上去合理的结论，而这个结论一旦错了，症状不会立刻出现，只会让后面某一步"为什么换栈没炸"变得无法解释。

顺带把可核实的部分交代完：`minix3/minix/kernel/arch/earm/mpx.S` 里 ARM 端口是同一段形状（同名符号、同前缀），所以这不是某个端口的笔误。

### 2.5 布局、装载、跳转三件事的合流

```
可执行文件（字节 + 每段的执行地址/装载地址）
  │
  ├─ 装载器按"装载地址"摆放字节，未初始化部分按 memsz-filesz 清零
  │
  ├─ 页表把"执行地址 → 装载地址"接上
  │     ├─ 过渡段：执行地址 = 装载地址（01 篇）
  │     └─ 高半段：执行地址 = 装载地址 + 固定偏移（本节 2.1、2.2）
  │
  └─ 切换执行的时机：新栈可用 + 跳转目标已映射，缺一不可（2.3）
```

不变的东西是一句话：**装载地址归装载器管，执行地址归页表管，两者之间只靠一个固定偏移量缝合；偏移量必须对所有段一致，否则"一段映射"这句话就不成立。**最后半句在 C 里靠 `_kern_offset` 一处定义、处处 `AT()` 引用；重写靠什么守住，是 §3.2 的主题。

而"唯一没查清的事"（2.4）不影响 2.3 的成对性结论——它只影响“换栈后 `esp` 落在哪一段”这个诊断问题，§6 给出真机判定它的办法。

---

## 3. Rust 设计决策

本章有 C 对应面的每一节，第一句都是上一节里 C 的真实做法；重写的设计决策由它逼出。

### 3.1 谁来放置装载地址：两种脚本，别当成一种

**C 用 `AT()` 在同一个脚本里同时写下两套编号**（2.2），前提是"有一个不认识这个布局的装载器，只认文件里写的装载地址"。

重写里有两类链接脚本，它们对 `AT()` 的选择**相反**：

| 脚本 | 服务对象 | 用不用 `AT()` | 为什么 |
|---|---|---|---|
| `os/kernel/src/arch/x86_64/link.ld`、`os/kernel/src/arch/{aarch64,riscv64}/link.ld`（内核 crate 侧） | 由引导垫片读取并装载 | 不用（x86-64 那份在文件头直说由引导垫片处理装载地址） | 装载方是自己人，它读段的 `paddr` 字段摆放，不需要脚本再标一遍 |
| `os/kernel-image/{x86_64,aarch64,riscv64}.ld`（独立生产镜像） | 交给外部装载器/载体 | 用 | 外部装载器只按文件里的装载地址摆字节，脚本不标就没有第二套编号的来源 |

因此"重写在 Rust 里丢掉了 `AT()`"这句话，**对第一类成立、对第二类不成立**：同一件事在仓内有两份相反的实现，拿其中一份去描述全部就会错。真正的设计决策不是"要不要 `AT()`"，而是：**谁负责摆放字节，谁就要拿到装载地址**；脚本只是把这个责任归属写成文件而已。

### 3.2 偏移量必须对所有段一致

**C 把所有段的偏移统一到 `_kern_offset` 这一个符号上**（2.1、2.2），于是一句"内核从执行地址 `0xF0400000` 起、长 `_kern_size`"就足以描述整块映射。

重写把这个约束升级成了两条可执行的东西，因为它需要的输入恰好只有一个二元组：`os/boot-shim/src/loader.rs:fn compute_kernel_layout` 只取所有可装载段的 `min(vaddr)` 与 `min(paddr)` 填进启动契约，内核页表据此建立"一段"映射。**如果各段偏移不同，`min(vaddr)` 与 `min(paddr)` 就描述不了其余段**——它们会被映射到错误的位置，而且错误要到第一次访问时才显现。

所以这条约束不能只靠"写脚本时注意"。§4.1 与 §5 会看到它被写成宿主可跑的断言。

### 3.3 两个基址都是被机制钉住的，不是取的好看数字

**C 的两个基址由注释给出理由**（2.1：物理基址 4 MB 对齐便于重映射；虚拟基址取高处以腾空用户空间）。重写的基址同样各有来历，而且三个架构都不是同一个数：

| 架构 | 执行基址 | 物理基址 | 钉住它们的机制 |
|---|---|---|---|
| x86-64 | `0xFFFF800000000000` | `0x200000` | 规范高地址；物理侧取 2 MiB 对齐，避开实模式区与扩展 BIOS 数据区 |
| ARM64 | `0xFFFF800000000000` | `0x40200000` | 两个转换基址寄存器共用同一张 L0 根表、翻译长度 T1SZ=16 ⇒ 高半区从索引 256 的表项开始，该值正是那个表项的边界；物理侧是 QEMU virt 的 DRAM 起点 `0x40000000` 加 2 MiB |
| RISC-V | `0xFFFFFFC000000000` | `0x80200000` | Sv39 是 39 位虚拟地址、按 bit 38 符号扩展到 64 位，规范高址的第三个索引段取 256；物理侧同样是对应机型 DRAM 起点加 2 MiB |

这张表里最值得自己算一遍的是 RISC-V 那一行的反面教训。RISC-V Sv39 的第三个索引段怎么取：`VPN[2] = (addr >> 30) & 0x1FF`。拿脚本点名的三个值算：

| 地址 | `VPN[2]` | 后果 |
|---|---|---|
| `0xFFFFFFC000000000` | 256 | 规范高址，与低半不重合 |
| `0xFFFFC00000000000` | **0** | 与低半地址重合 |
| `0xFFFFFC0000000000` | **0** | 同上 |

后两个看长得像"一堆 F 加一个高位"，算完就知道它们的低 39 位全零——**与低半地址其实是同一个位置**。选错的症状不是链接失败，而是"能跑，但某一次切换根表之后突然落进不该存在的映射"，因为两张映射从一开始就抓在同一个索引上。

把三行合起来看，高半区放多高不是审美值：它取决于该架构**怎么把虚拟地址切成层级、哪一个边界保证不与低半重合**。

再看 3.1 那张表里的"谁来摆放字节"——它不是泛指，三个地方都有具体名字：32 位路径上是 GRUB，靠配置文件里的 `multiboot` 与 `load_mods` 两条指令把内核与模块摆好（并把模块按页对齐）；64 位的 x86-64 与 ARM64 上是引导垫片自己，具体就是 §4.2 那个按 `paddr` 写字节的函数；RISC-V 上则是 U-Boot 先 `fatload` 拿到文件、再由引导垫片放置（§3.6 会看到它为何不能干脆也用固件接口）。

### 3.4 成对跳转要不要按架构分支

**C 的四条指令是写在两份汇编里的**（x86 的 `mov $..., %esp` / `call`，ARM 端口的 `ldr sp, =...` / 分支），每个架构一份，调用点也各写一次。

重写把这段收进一个接口类型。`os/kernel/src/boot/higher_half.rs:trait HigherHalf`（接口体逐字，安全前提写在方法文档里）：

```rust
pub trait HigherHalf {
    /// Perform the higher-half transition: switch stack to high address
    /// and jump to kmain.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - Paging is enabled with both identity and kernel high mappings
    /// - `kinfo` pointer is valid and accessible at high address
    /// - `stack_top` is a valid, 16-byte-aligned virtual address in the
    ///   kernel's high-half mapping
    /// - This is called exactly once, from the boot CPU
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !;
}
```

方法只有一个，而**没有接收者**；安全前提四条：翻译已启用且过渡段与高半段都已映射、契约指针在高地址下仍可读、栈顶是已对齐到 16 字节的执行地址、只在启动处理器上调用一次。栈顶作为一个参数传进来，不写在某处。

买到的是：调用点不需要按架构写条件编译；可以注入一个假实现来检查调用次序（不真的执行指令）。**买不到的是汇编的正确性**——每个实现体里那几行指令是否恰好满足"先换栈再跳"的次序、寄存器有没有踩到调用约定的保留位，都要靠实现体自己与真机。这与 01 篇 3.2 对 `BootShim` 的结论同形：接口约束形状，不约束流程。

### 3.5 "不分页那一段"换了归属，不是被删

**C 需要一段待在装载地址上执行的代码**（2.1 的入口带 `__k_unpaged_` 前缀、2.2 的分界线），因为启用翻译前它只能按物理地址跑。

重写里内核没有这样一段，因为**执行"启用翻译前那段时间"的代码不在内核镜像里**——它是引导垫片自己（01 篇 4.1、4.4 已经处理过它的后果：契约里那个"不分页段长度"字段被断言恒为 0）。独立生产镜像侧的做法是把入口段单独命名并强制保留（`os/kernel-image/x86_64.ld` 的 `ENTRY(_start)` 与 `.text.boot` 的 `KEEP`），入口本身仍然在高半区的执行地址上，因为**跳进它之前，映射已经建好了**。

这是本篇与 01 篇真正的接缝：01 篇解释了"为什么目的地要先铺"，这里解释"于是目的地可以直接放在高半区，不再需要一个低地址替身"。

### 3.6 同一个语义，三种机器动作

把四步骨架写出来，差异立刻可辨（真实指令见第 4.3 节）：

| 步骤 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| 换栈顶 | `mov rsp, {stktop}` | `mov sp, {stktop}` | `mv sp, {stktop}` |
| 栈对齐到 16 字节 | `and rsp, -16` | **不能直接对 `sp` 做 `and`**，需借一个临时寄存器中转 | `li t0,-16` 再 `and sp, sp, t0` |
| 帧指针归零 | `xor rbp, rbp` | `mov x29, #0` | `li s0, 0` |
| 跳往高地址入口 | `push 0` 后 `call {kmain}` | `br {kmain}` | `la` 取址后 **加偏移量** 再 `jalr` |
| 参数递送 | 契约指针经寄存器（01 篇 4.3 说明了为什么不能只靠它） | 同左 | 同左，且该寄存器在本块内不得复用 |

四行是同一个语义（先备好可执行的栈，再跳到执行地址），第五行的差别才是内容：**RISC-V 的 `la` 拿不到执行地址，必须自己补偏移**；ARM64 则是被指令格式限制绕了个弯。为什么需要补，§4.4 用一个真实故障说明。

---

## 4. 实现详解

### 4.1 两套脚本的真实差别（落实 3.1、3.2、3.3）

内核 crate 侧，x86-64（`os/kernel/src/arch/x86_64/link.ld`，节选；两个基址与位置计数器）：

```ld
KERN_VIRT_BASE = 0xFFFF800000000000;
KERN_PHYS_BASE = 0x00200000;    /* 2MB, typical physical load address */

SECTIONS
{
    . = KERN_VIRT_BASE;
    .text : { *(.text .text.*) }
    ...
    PROVIDE(kern_virt_base = KERN_VIRT_BASE);
    PROVIDE(kern_phys_base = KERN_PHYS_BASE);
    PROVIDE(kern_size = . - KERN_VIRT_BASE);
```

它与 C 的两处对应一眼可见：`PROVIDE` 出的三个符号名，正是 01 篇里 C 用链接期符号读的那三个（内核虚拟基址、物理基址、大小）；`. = KERN_VIRT_BASE` 对应 2.2 的"抬升"。不同的是这里**没有 `AT()`**，也就是这里没有第二套编号——按 3.1，装载由引导垫片读 `paddr` 完成。

独立生产镜像侧，x86-64（`os/kernel-image/x86_64.ld`，节选）：

```ld
KERNEL_HIGH_BASE = 0xFFFF800000000000;
KERNEL_PHYS_BASE = 0x200000;

SECTIONS
{
    . = KERNEL_HIGH_BASE + KERNEL_PHYS_BASE;
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_HIGH_BASE)
    {
        KEEP(*(.text.boot))
    }
```

**注意位置计数器：这里是 `高半基址 + 物理基址`**，与 ARM64 那份的 `. = KERN_VIRT_BASE` 不同形。为什么两个架构的第一段执行地址不一样，而两边都还能用？答案在偏移量一致性上：x86-64 让 `vaddr = 高半基址 + paddr`，ARM64 让 `vaddr = 高半基址 + (paddr - 物理基址)`，两者都满足"所有段同一个平移量"（3.2），差别只在那个平移量的具体值。校验脚本对这两条惯例分别有断言（4.5 展开），并且明写"两条都满足装载方的取最小值契约"。

ARM64 生产镜像（`os/kernel-image/aarch64.ld`，节选，展示 `AT()` 表达式与 3.3 的来历注释）：

```ld
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)
```

`AT()` 里的表达式正是 3.2 那条"所有段一致"的写法化表达：偏移量 = `KERNEL_VIRT_BASE - KERNEL_PHYS_BASE`，与 C 的 `_kern_offset` 同构，只是这次由每个段各自引用两个基址算出，而不是引用一个共用符号。

### 4.2 按装载地址摆放：装载器的真实步骤（落实 3.1、3.2）

摆放与统计在 `os/boot-shim/src/loader.rs`。布局计算（函数体节选，保留全部关键行）：

```rust
pub fn compute_kernel_layout(elf_data: &[u8]) -> Result<KernelLoadResult, minix_elf::ElfError> {
    let mut iter = minix_elf::segment_iter(elf_data)?;
    let entry_point = iter.ehdr().e_entry;

    let mut kern_phys_base = u64::MAX;
    let mut kern_virt_base = u64::MAX;
    let mut kern_end_phys = 0u64;
    let mut found_load = false;

    while let Some(seg) = iter.next() {
        found_load = true;
        kern_phys_base = kern_phys_base.min(seg.paddr);
        kern_virt_base = kern_virt_base.min(seg.vaddr);
        kern_end_phys = kern_end_phys.max(seg.paddr + seg.memsz);
    }

    if !found_load {
        return Err(minix_elf::ElfError::NoLoadSegments);
    }

    let kern_size = kern_end_phys - kern_phys_base;
    ...
}
```

对照 2.2 末尾那行 `_kern_size = . - _kern_vir_base;`，这里有个不显眼但重要的差别：**C 的内核大小在执行地址坐标下算，Rust 在装载地址坐标下算**（`max(paddr + memsz) - min(paddr)`）。只要 3.2 的偏移量一致成立，两者数值相同；一旦不一致，C 那个算法会给出一个"看起来合理"的跨距，而这个算法给出的是"实际占用的物理范围"。放在物理内存分配语境里，后者才是不会撒谎的那一个。（`os/boot-shim/src/loader.rs:fn compute_kernel_layout`）

摆放本身。`os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory`（节选）：

```rust
    let segments: Vec<_> = iter.collect();
    if let Some(_) = first_overlapping_pair(&segments) {
        return Err(minix_elf::ElfError::InvalidSegment);
    }
    for seg in &segments {
        let dest = seg.paddr as *mut u8;
        let src = &elf_data[seg.offset as usize..(seg.offset + seg.filesz) as usize];
        unsafe {
            dest.copy_from_nonoverlapping(src.as_ptr(), src.len());
            if seg.memsz > seg.filesz {
                let bss_start = dest.add(seg.filesz as usize);
                let bss_size = (seg.memsz - seg.filesz) as usize;
                core::ptr::write_bytes(bss_start, 0, bss_size);
            }
        }
    }
```

先查重叠（01 篇 2.5 的内核版，那里是"把内核当额外一项"，这里是"段与段之间"，同样用半开区间判据，见 `os/boot-shim/src/loader.rs:fn first_overlapping_pair`）；再按 `paddr` 拷贝 `filesz` 字节；最后把 `memsz - filesz` 那段清零——**未初始化数据段不是文件里的内容，是装载时凭长度差造出来的零**，这正是 2.2 里 `.bss` 需要 `AT()` 却不需要文件字节的道理。

### 4.3 三个架构的成对跳转（落实 3.4、3.6）

x86-64。`os/kernel/src/arch/x86_64/higher_half.rs:impl X86_64HigherHalf::jump_to_kmain`（内联汇编部分逐字）：

```rust
                "mov rsp, {stktop}",
                "and rsp, -16",
                "xor rbp, rbp",
                "push 0",
                "call {kmain}",
                stktop = in(reg) stack_top.0,
                kmain = sym kmain,
                in("rdi") kinfo,
                options(noreturn)
```

这五行与 2.3 那四条指令是同一件事（换栈、对齐、帧指针归零、放栈底标记、调用），差别全在**对齐这一步被写成了指令**。01 篇 2.1 里那条没有注释的 `push $0` 在这里有了显式对应物：同一个 16 字节对齐约定，C 靠压栈次数隐式满足，重写用一条运算写清楚。这也解释了为什么那条 `push $0` 的用途只能标 `[待验证]` 而不能反推——隐式约定的现场没有留下证据。

ARM64。`os/kernel/src/arch/aarch64/higher_half.rs:impl AArch64HigherHalf::jump_to_kmain`（节选）：

```rust
                "mov sp, {stktop}",
                "mov x1, #-16",
                "mov x2, sp",
                "and x2, x2, x1",
                "mov sp, x2",
```

多出来的几步不是冗余：该文件注释只说了一件事——**ARM64 不允许把 `sp` 作为 `and` 的目的寄存器**。被强制的是"结果不能直接写回 `sp`"，所以"借一个临时寄存器算、再把算完的值搬回 `sp`"这两步不可省；而块里多出的那条 `mov x2, sp` 只是当前写法（先把 `sp` 复制到 `x2`，再让 `x2` 同时当源与目的），把 `sp` 直接当源来算也是合法的。读这段的收获因此不是"ARM64 多花三条指令"，而是：**指令集能强制你多绕一步，但绕法本身仍是一个可选的实现细节——两者别混为一谈。**

RISC-V。`os/kernel/src/arch/riscv64/higher_half.rs:impl Riscv64HigherHalf::jump_to_kmain`（节选）：

```rust
                "mv sp, {stktop}",
                "li t0, -16",
                "and sp, sp, t0",
                "li s0, 0",
                "fence.i",
                "la t1, {kmain}",
                "add t1, t1, t2",
                "jalr x0, t1, 0",
                stktop = in(reg) stack_top.0,
                in("t2") delta,
                kmain = sym kmain,
                in("a0") kinfo,
                options(noreturn)
```

这一份里有三处值得停下来的地方：

- `and sp, sp, t0` 与 ARM64 相反——**RISC-V 允许直接对 `sp` 做 `and`**，所以只需要把 `-16` 先装进寄存器。同一条对齐动作，三个架构三种写法，都不是风格差异。
- `fence.i`：同步取指缓存。它出现在跳之前，因为这段代码自己就是"刚被搬进内存的指令"。
- 临时寄存器的选择有禁区：该块注释说明**不能占用 `a0`**（它带着契约指针，要活到 `kmain` 真的开始），也不能用与外部调用相关的 `a6/a7`（监督器调用会破坏它们）。所以偏移量走 `t2`、目标地址走 `t1`。
- 还有 Rust 侧的一个限制：这些被写过的寄存器（`sp`、`t0`、`t1`、归零的 `s0`）**没法声明成汇编块的输出**，因为编译器拒绝在 `noreturn` 的块里声明输出。正确性论证换了方式——正因为它不返回，那些写入对下游不可观察。

调用点只有一处，栈顶来自契约字段而非链接期符号（`os/kernel/src/lib.rs`，x86-64 分支，逐字）：

```rust
    unsafe { X86_64HigherHalf::jump_to_kmain(kinfo, kinfo.kern_stack_top) }
```

`kern_stack_top` 由引导垫片在构造契约时填入。这是 3.4 那层抽象真正落地的地方：**栈顶从"脚本里一个符号名"变成"契约里一个字段"**，于是宿主测试可以传任意值来验证调用次序，不必链接出一个真内核。

### 4.4 为什么 RISC-V 必须自己加偏移量（落实 3.6）

上一节的 `add t1, t1, t2` 是全篇最不显然的一行。它的来历写在同文件的注释里，下面完整转述，因为这是一次典型的"每一步都对、合起来错"。

RISC-V 的默认代码模型让 `la`（取一个符号地址）展开成"取当前指令位置附近的相对地址"。放在普通程序里毫无问题——程序在自己的装载位置执行，相对地址就是正确地址。可这段代码的场景特殊：**它此刻仍在过渡映射覆盖的低地址上执行，而它要跳去的 `kmain` 位于高半区的执行地址。**`la` 忠实地算出一个"相对于此处"的地址，于是拿到的是 `kmain` 在低地址那一侧的运行期位置。

后果不在这一跳本身。跳过去之后，内核照样跑；直到第一次为进程切换根表——新根表只在高半区映射内核（01 篇 4.3 的 `reserved_regions` 与此无关，那是另一条路），于是**下一条取指就落空**。症状是"引导一切正常、第一次切地址空间才死"，最难归因。

修法就是那一行加法：把整张镜像的平移量 `delta` 加上去，落到真正的执行地址。`delta` 的算法本身也有一处讲究（同文件，逐字）：

```rust
        let delta = kinfo
            .kern_virt_base()
            .0
            .checked_sub(kinfo.kern_phys_base().0)
            .expect("higher_half: kern_virt_base must be >= kern_phys_base");
```

为什么用 `checked_sub` 而不是普通减法：发布构建默认关掉溢出检查，普通减法在布局被写错时会**回绕成一个看似可用的数**，跳转照样发生，只是落点错到别处；`checked_sub` 让这个不可能成立的情况在启动瞬间中止，并且把原因写进消息。也就是说这行代码买的是"布局错误立刻可见"，而它与 3.2 那条"偏移量一致性"是同一条前提的两次守护——一次在构建期（4.5 的脚本断言），一次在运行期（这里）。

对照 3.3 的教训看更有味道：x86-64 与 ARM64 都不需要这一步加法，因为它们的取指方式（`call` 一个链接期符号、`br` 一个寄存器值）解析出的就是执行地址；RISC-V 需要，是因为它的默认代码模型假设"程序在链接时假设的位置上运行"，而引导期恰好不是。**同一段语义，被工具链的一个默认值改变了实现。**

### 4.5 布局一致性的宿主可判定部分（落实 3.2、3.3）

3.2 说这条约束"不能只靠写脚本时注意"。它的落点是宿主上就能跑的一组断言（`os/kernel-image/check-layout.sh`，注释里的清单，逐条摘述）：

| 断言 | 守什么 | 回指 |
|---|---|---|
| 预期表里的两个基址 = 脚本里声明的两个 | 脚本与校验表两处手写的漂移 | 3.3 |
| 文件类型必须是可执行、而非位置无关 | 内核镜像的装载面没有重定位处理器 | 3.3 |
| 机器类型与预期相符 | 防止拿一个架构的工件当另一个用 | — |
| 首段的可执行地址/装载地址 = 该架构预期基址 | 两套惯例各自成立 | 4.1 的 `. = 高半基址` 与 `. = 高半基址 + 物理基址` |
| **所有段的（执行地址 − 装载地址）是同一个量** | 一段映射描述得下整块内核 | **3.2** |
| 跨距按 2 MiB 收口 | 大页阶梯不接受任意边界 | 01 篇 4.2 第二步 |
| 入口 = 脚本声明的入口 = 符号值，且落在首段内 | 入口没被裁到别处 | 3.5 |
| 引导栈符号存在、自顶向下 64 KiB、整体落在末段内 | 换栈后第一个 `push` 就在映射里 | 2.3、2.4 |
| 主流程符号仍在镜像里 | 链接期保留标记真的生效 | 3.5 |

这张表里"跨距按 2 MiB 收口"与"引导栈落在末段内"两条，正是 2.3、2.4 那对次序在**构建期**的可判定版本：栈在映射内 ⇒ 换栈安全；跨距对齐 ⇒ 高半段能用大页铺完。也就是说 C 靠人读脚本记住的那些前提，这里每一条都有一个能跑的断言对应——这是本篇与"教学承载物在实现里"最贴合的一个例子。

---

## 5. 测试要点

数量现算，不抄表：

```bash
cd os && ls qemu-tests/test-kernels/kernel/bootstrap/ | grep '^test-higher-half'
bash os/kernel-image/check-layout.sh all   # 逐架构跑一遍静态布局断言
```

- 高半区跳转的真机载体：`os/qemu-tests/test-kernels/kernel/bootstrap/` 下三份——`test-higher-half`、`test-higher-half-aarch64`、`test-higher-half-riscv64`（上面命令可复算）。它们覆盖的是"换栈与跳转是否真的成对发生"——只有真机能判定，因为判定的是取指。
- 静态布局：`check-layout.sh` 的九条断言（4.5 的表，逐条可跑）。它管不到取指、也管不到 `delta` 是否被加对。

哪些只能真机：3.3 的规范高址合法性（链接器不校验语义）、4.4 的 `la` 解析结果、`fence.i` 的必要性、2.4 那个落点问题。

**盲区**（如实列出）：其一，2.4 未追清的构建期前缀机制；其二，`os/kernel/src/boot/higher_half.rs` 这个接口本身没有单元测试（01 篇 5.3 同样发现 `boot_shim.rs`、`handoff.rs` 无测试，两个引导期接口的保护都来自实现体与真机载体，不来自宿主测试）。

---

## 6. 诊断

三类失败形状与各自该查哪一层：

```bash
# 一：脚本与校验表的基址是否漂移（3.3、4.1、4.5）
grep -nE 'KERN_VIRT_BASE|KERN_PHYS_BASE|KERNEL_HIGH_BASE|KERNEL_PHYS_BASE|AT\(' \
    os/kernel/src/arch/*/link.ld os/kernel-image/*.ld

# 二：偏移量是否对所有段一致（3.2；不成立时看哪一段跑偏）
readelf -lW <内核镜像> | grep -E 'LOAD|Entry'

# 三：换栈与跳转的指令序列是否完整（2.3、4.3）
rg -n 'mov rsp|and rsp, -16|xor rbp|mov sp|and sp, sp|fence\.i|la t1|add t1|jalr|br \{kmain\}' \
    os/kernel/src/arch/*/higher_half.rs
```

预期关键行与读法：

- 命令一：同一架构的两份脚本，虚拟基址必须同一个值；物理基址若不同，要能在 3.3 那张表里找到对应机型的 DRAM 起点来历，否则就是漂移。
- 命令二：每条 `LOAD` 行的两个地址之差必须完全相同；出现第二种差值，3.2 的前提已经破了，症状会推迟到第一次访问或第一次换根才显现。
- 命令三：三条"换栈 + 对齐 + 归零"必须都出现在跳转之前。RISC-V 那一份里若少了 `add t1, t1, t2`，就是 4.4 那个故障的复发。

**症状定位表**（把 2.3、4.4 的推演反过来用）：

| 症状 | 第一嫌疑 | 验证方式 |
|---|---|---|
| 换栈后的第一条访存指令就故障 | 新栈地址没在映射里（2.3 前半、4.5 的栈落点断言） | 打印换栈后的栈指针，对照首段与末段的地址区间 |
| 跳过去了，但第一次换根就死 | 执行位置在低半侧（4.4 的 RISC-V 情形），或高半窗口没被重放（01 篇 4.3） | 比对程序计数器与镜像首段的可执行地址区间 |
| 参数在 `kmain` 里读成垃圾值 | 契约指针走的寄存器被复用（01 篇 4.3 同类） | 看接口块里参数寄存器有没有被当临时寄存器用 |

---

## 7. 参见

**C 真值源**

- `minix3/minix/kernel/arch/i386/kernel.lds` —— 双址布局、`_kern_offset`、不分页段与 `AT()`（2.1、2.2）
- `minix3/minix/kernel/arch/i386/mpx.S` —— 初始栈与内核栈的预留、`__k_unpaged_` 前缀线索（2.4）
- `minix3/minix/kernel/arch/earm/mpx.S`、`minix3/minix/kernel/arch/earm/head.S` —— ARM 端口的同形写法（2.4、3.4）
- `minix3/minix/kernel/arch/i386/head.S` —— 换栈与跳转那四行（2.3）

**Rust 实现**

- `os/kernel/src/arch/x86_64/link.ld`、`os/kernel/src/arch/aarch64/link.ld`、`os/kernel/src/arch/riscv64/link.ld` —— 内核 crate 侧脚本，不用 `AT()`（3.1、4.1）
- `os/kernel-image/x86_64.ld`、`os/kernel-image/aarch64.ld`、`os/kernel-image/riscv64.ld` —— 生产镜像脚本，用 `AT()`，两个基址的来历注释（3.3、4.1）
- `os/kernel-image/check-layout.sh` —— 九条静态布局断言（4.5、5.1）
- `os/boot-shim/src/loader.rs:fn compute_kernel_layout`、`os/boot-shim/src/loader.rs:fn load_segments_into_phys_memory`、`os/boot-shim/src/loader.rs:fn first_overlapping_pair` —— 按装载地址摆放、清零、段间查重（4.2）
- `os/kernel/src/boot/higher_half.rs` 与 `os/kernel/src/arch/*/higher_half.rs`、`os/kernel/src/lib.rs` 的调用点 —— 成对跳转的接口与三架构实现（3.4、4.3、4.4）
- `os/boot-shim/src/{uefi_helpers.rs,opensbi_helpers.rs}`、`os/kernel-image/src/{main.rs,bootface.rs}` —— 契约里 `kern_stack_top` 的来源与镜像入口（4.3）
- `os/arch/src/arm64/paging.rs`、`os/arch/src/riscv64/paging.rs`、`os/arch/src/arch/direct_map.rs` —— 高半区边界与根表层级（3.3）

**相关主题篇**（尚未生成的只给主题名）

- 01 篇（控制权交接）：过渡映射、启用翻译的位序、契约字段的形状与跨二进制交接
- 保护结构篇：中断入口表与任务状态段（本篇 2.3 的"深调用会耗尽临时栈"依赖它才知道上限）
- 页表与内存篇：运行期根表、直接映射窗口与页帧分配
- 多核启动篇：辅助处理器从哪个地址、哪段栈起步（与 2.4 的落点问题同类）

**外部规范来源**（本篇引用仓库外依据处均已就地标明）

- ARM 架构参考手册：两个转换基址寄存器与翻译长度决定的高半区边界
- RISC-V 特权规范与调用规范：Sv39 的规范地址范围、代码模型的相对取址行为
