# 01 · 从固件到内核：控制权交接前，机器必须变成什么样

> **C 真值源**: `minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/arch/i386/pg_utils.c`
> **Rust 实现**: `os/libs/minix-boot/src/kernel_info.rs`、`os/libs/minix-boot/src/boot_shim.rs`、`os/libs/minix-boot/src/handoff.rs`、`os/libs/minix-boot/src/platform.rs`、`os/boot-shim/src/main.rs`、`os/boot-shim/src/loader.rs`、`os/boot-shim/src/uefi_helpers.rs`、`os/boot-shim/src/opensbi_helpers.rs`、`os/kernel/src/lib.rs:arch_boot`

## 第 1 章　概念：CPU 在跳进内核第一条指令之前需要回答的问题

### 1.1 矛盾的起点：固件提供的是接口，不是契约

一台刚上电的机器，并不"欠"内核任何东西。CPU 复位后从固件手里接过执行权，此时内存里已经躺着一份内核镜像，但内核还没有页表、没有可用的栈、甚至不知道自己被放在物理内存的哪个位置。真正棘手的地方在于：**固件告诉内核"你可以调用我"，却没有告诉内核"世界长什么样"**。

在 32 位 x86 的 Minix3 时代，这个鸿沟由引导加载器 GRUB 填平：GRUB 读取配置文件、把内核 ELF 和一批启动模块（PM、VM、VFS、RS 等用户态服务器镜像）加载到物理内存，再把一张描述"内存布局 + 模块清单"的 multiboot 信息结构交给内核。内核只需要**解析**这张结构，剩下的加载活儿 GRUB 都干完了。

到了 64 位多架构（x86-64、aarch64、riscv64）的世界，情况变了。现代固件——UEFI 和 SBI——只提供**服务接口**（"你可以调用我获取内存映射"），不会替你把内核和模块搬进内存、也不会给你一张现成的布局表。于是必须有一个组件，专门把"固件接口能问出来的信息"转化成"内核可以直接消费的启动契约"。这个组件就是 **boot-shim**。

一句话概括 boot-shim 的历史位置：**GRUB 的职责，被搬进了内核自带的一段引导代码里。**

### 1.2 boot-shim 要交付的三样东西

无论跑在哪个架构、面对哪种固件，控制权真正交到内核手里的那一刻，内核必须能回答三个问题：

1. **我站在哪里？** 内核镜像被放在物理内存的什么地址，运行时的虚拟地址又是什么，边界在哪里。
2. **哪里还能用？** 哪些物理内存是空闲可用的，哪些已经被内核、模块、固件占据。
3. **我要带着谁走？** 启动阶段需要拉起的那些用户态服务器镜像（boot modules）在哪里、叫什么名字。

boot-shim 的全部工作，就是把这三个问题的答案，连同为它们服务的页表根，打包成一份**纯数据**，一次交给内核。

### 1.3 交接为什么必须"只传数据、不传函数指针"

这是本篇最核心的一条设计约束，值得单独讲清楚，因为它决定了后面所有接口的形状。

boot-shim 和内核，在最终形态里是**两个独立的二进制镜像**：一个是 UEFI 应用（PE 格式），一个是链接到高半区的 `kernel.elf`。引导完成后，boot-shim 占用的内存随时可能被回收或重新映射。如果交接时传的是一个**函数指针**——比如"调用这个地址去解析设备树"——那么一旦 boot-shim 的镜像消失，指针指向的代码就成了悬垂引用，内核照着跳过去就是灾难。

所以交接契约被约束成：**跨边界传递的东西，必须全部是值语义的纯数据**（整数、物理地址、指向"引导期间一直存活"的物理内存的切片）。需要传递"能力"时，不传函数指针，而传一个**不透明的分类标签**加一个**物理地址**，让内核在自己这边查表决定怎么处理。这条约束在第 3 章会具体落到 `PlatformDescSource` 和 `BootHandoff` 两个类型上。

> 判据来自项目的一条硬边界：启动契约是"平台把系统送入 OS 的方式"，它不是 OS 的运行时机制，因此不强行去和运行时的抽象统一（见 `prompt/review-rules/review-core-semantics.md` §1.6 三条边界之"启动契约不在抽象射程内"）。本篇讲的就是这份启动契约本身。

### 1.4 架构演进史：控制权是怎么一路交到今天的

要理解 boot-shim 为什么长成这样，得看它替代了什么。控制权交接这件事，在不同年代有不同的分工：

| 年代 | 引导链 | 内核加载方式 | 交给内核的东西 |
|---|---|---|---|
| 32 位 x86 | BIOS → GRUB → multiboot | GRUB 代劳加载 | `multiboot_info` 结构，内核解析即可 |
| 64 位 x86 | UEFI/OVMF → boot-shim | boot-shim 自己做 | `KernelInfo`（无 multiboot） |
| aarch64 | UEFI/OpenSBI → boot-shim | boot-shim 自己做 | `KernelInfo` + 平台描述符句柄 |
| riscv64 | OpenSBI → boot-shim | boot-shim 通过 SBI ecall 询问 | `KernelInfo` + DTB 句柄 |

再往里一层，地址空间布局也在演进。32 位的 Minix3 因为地址空间紧张，内核必须先在一块低地址的 unpaged（未分页）段里跑起来，等页表建好再切到高地址，然后把那块 unpaged 段回收成可用内存。64 位下这个约束消失了——内核从第一条指令就可以直接在高半区运行，不再需要 unpaged 段这个历史包袱。第 3.7 节会处理这个概念在 Rust 版里的遗留。

---

## 第 2 章　C 源码：Minix3 如何回答"控制权易手前必须做什么"

Minix3 的内核侧，这段故事集中在两个文件里：`minix3/minix/kernel/arch/i386/pre_init.c` 和 `minix3/minix/kernel/arch/i386/pg_utils.c`。Rust 版的 boot-shim 要做的事，就是这两份 C 代码职责的重新分配。先看清楚 C 原版怎么做的，才能判断 Rust 版哪些是"保持外部语义的重写"、哪些是"64 位多架构逼出来的演进"。

### 2.1 pre_init：内核第一个 C 函数的五步序列

`pre_init(magic, ebx)` 是内核被跳转进来后执行的第一个 C 函数（`minix3/minix/kernel/arch/i386/pre_init.c:pre_init`）。它收到两个参数：`magic` 是引导加载器留下的魔数，`ebx` 指向 multiboot 信息结构。函数体是一条严格有序的序列：

1. **验魔数**：断言 `magic == MULTIBOOT_INFO_MAGIC`。这不是可选的礼貌检查——如果魔数不对，说明我们根本不是被 multiboot 引导器跳进来的，后面所有对 `ebx` 所指结构的信任都不成立。
2. **整理启动信息**：调用 `get_parameters(ebx, &kinfo)`，把散落在 multiboot 结构里的内存映射、模块清单、命令行参数，收敛进一个全局的 `kinfo` 结构（见 2.2）。
3. **建恒等映射**：`pg_clear()` 清空页目录，`pg_identity(&kinfo)` 建立"虚拟地址 = 物理地址"的一比一映射。
4. **把内核映射到高地址**：`kinfo.freepde_start = pg_mapkernel()`，返回第一个空闲页目录项索引。
5. **启用分页**：`pg_load()` 把页目录物理地址写进 CR3，`vm_enable_paging()` 打开分页开关。

**为什么第 3 步和第 4 步这个顺序？** 假设反过来——先只映射高地址的内核，再启用分页。那么启用分页的那一瞬间，CPU 的下一条指令地址（`pre_init` 自己的代码，此刻还跑在低地址）在页表里根本没有映射，立刻就是一个取指缺页，机器直接 triple-fault。恒等映射的作用正是：**开分页的那一刻，当前正在执行的代码所在的物理地址，仍然能被翻译到同样的虚拟地址**，于是 `vm_enable_paging` 返回后代码能继续往下跑。内核映射则是**为了之后**——它把内核该待的高地址也一并铺好。两者叠加，才让"控制权从低地址的引导上下文平稳地移到高地址的内核"成为可能。

把这段序列原样看一遍最直观（`minix3/minix/kernel/arch/i386/pre_init.c:pre_init`，节选函数体）：

```c
kinfo_t *pre_init(u32_t magic, u32_t ebx)
{
    assert(magic == MULTIBOOT_INFO_MAGIC);   // ① 验魔数
    get_parameters(ebx, &kinfo);             // ② 整理启动信息
    pg_clear();                              // ③ 清页目录
    pg_identity(&kinfo);                     // ④ 恒等映射（先铺，保当前代码可继续执行）
    kinfo.freepde_start = pg_mapkernel();    // ⑤ 内核高地址映射
    pg_load();                               // ⑥ 写 CR3
    vm_enable_paging();                      // ⑦ 开分页
    return &kinfo;                           // → 交给 head.S 传给 kmain
}
```

### 2.2 get_parameters：把 multiboot 的散落信息整理成 kinfo_t

`get_parameters(ebx, cbi)`（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`）是"解析"这一步的实体。它做的事情，Rust 版全部搬到了 boot-shim 内部：

- **先抄一份自己的 multiboot 结构**：`memcpy(mbi, ebx, ...)`。原始结构可能在引导服务退出后失效，所以先做一份可信副本。
- **填内核位置**：从链接期符号 `_kern_vir_base`、`_kern_phys_base`、`_kern_size` 读出内核的虚拟基址、物理基址、大小；从 `_kern_unpaged_start`、`_kern_unpaged_end` 算出那段待回收的 bootstrap（unpaged）区间的起止。
- **解析命令行**：如果 multiboot 结构带了 cmdline，逐个拆成 `name=value` 塞进 `param_buf`，并识别其中少数几个已知变量（是否做串口调试、波特率）。
- **定用户栈顶与内核已占字节数**：`user_sp` 被刻意压到一个低一档的地址，好让内核栈溢出时撞进一个"留白"区域被立刻发现，也让内核地址（`0xf...`）和用户地址（`0xe...`）一眼可辨。
- **建内存映射**：优先用结构里的内存映射数组，逐段把"可用"类型喂给 `add_memmap`；退化情况下用 `mem_lower`/`mem_upper` 两段凑出可用内存。
- **重叠体检**：把内核自己也当成一个"模块"，和所有真实模块一起两两查重叠，一旦发现某个模块和内核或别的模块压在同一块物理内存上就 `panic`；对每个通过的区间调用 `cut_memmap` 把它从可用内存里切除。

`get_parameters` 的重头戏是把内存地图建出来、再挨个切除已占用区间（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`，节选）：

```c
/* 建可用内存视图：优先用 multiboot 内存映射，逐段登记 */
for (mmap = (multiboot_memory_map_t *) mbi->mmap_addr;
     (unsigned long) mmap < mbi->mmap_addr + mbi->mmap_length; mmap = /* 下一段 */) {
    if (mmap->mm_type != MULTIBOOT_MEMORY_AVAILABLE) continue;
    add_memmap(cbi, mmap->mm_base_addr, mmap->mm_length);
}
/* 把内核自己也当成一个模块，两两查重叠，冲突即 panic，通过的再切除 */
for (m = 0; m < cbi->mods_with_kernel; m++) {
    if (overlaps(cbi->module_list, cbi->mods_with_kernel, m))
        panic("overlapping boot modules/kernel");
    cut_memmap(cbi, cbi->module_list[m].mod_start, cbi->module_list[m].mod_end);
}
```

这一段是 boot-shim 在 Rust 版里最重的一块职责的来源：C 里 `kinfo_t` 的每一个字段，都对应着 `KernelInfo` 的一个字段（对照见 §3.1）。

### 2.3 memmap 记账：可用内存视图的四种操作

`pg_utils.c` 提供了一组对"可用物理内存"这张视图的原始操作，Rust boot-shim 要复现同样的语义：

- **`add_memmap`**（`minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`）：登记一段可用区间。它把起止都对齐到页，顺带维护"内存高水位" `mem_high_phys`。关键的一点：它把可用内存**截断在 4GB**（`LIMIT = 0xFFFFF000`），因为 32 位的 Minix3 其余部分处理不了更大的地址。**这个 4GB 天花板是纯纯的 32 位限制**，64 位下必须消失——这也是为什么 Rust 版不再单独跟踪 `mem_high_phys`。
- **`cut_memmap`**（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`）：从可用内存里切除一段（内核或某个模块占了它）。切除可能把原来一整段劈成最多两个子段——比如切掉中间一块，剩下左右两段仍然可用。
- **`alloc_lowest`**（`minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest`）：从低地址端挑最小能满足长度的一块分配出去，分配后顺手 `cut_memmap` 扣掉。
- **`pg_alloc_page`**（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page`）：从内存高地址末端取一个空闲页，专供引导期建页表用。

`add_memmap` 的 4GB 天花板就藏在这几行里（`minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`，节选）：

```c
#define LIMIT 0xFFFFF000
/* 把超过 4GB 的可用内存截断——32 位的其余部分处理不了更大地址 */
if (addr > LIMIT) return;
if (addr + len > LIMIT) {
    len -= (addr + len - LIMIT);
}
addr = roundup(addr, I386_PAGE_SIZE);
len = rounddown(len, I386_PAGE_SIZE);
```

`cut_memmap` 的"切除可能劈成两半"逻辑（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`，节选）——把重叠那段清零，剩余左、右两段再用 `add_memmap` 补回：

```c
cbi->memmap[m].mm_base_addr = cbi->memmap[m].mm_length = 0;
if (substart > memaddr)
    add_memmap(cbi, memaddr, substart - memaddr);   // 左残段
if (subend < memend)
    add_memmap(cbi, subend, memend - subend);       // 右残段
```

这四种操作合起来，就是"内核运行前，谁在动这张内存地图"的完整答案。Rust 版把它们的角色做了重新划分（§3.6、§4.2）。

### 2.4 32 位页表机制：静态池与两种粒度

`pg_utils.c` 里的页表操作，处处带着 32 位 x86 的烙印：

- **一张静态页目录** `pagedir[1024]`（4KB 对齐），和**一个静态页表池** `pagetables[6][1024]`——固定 6 个页表页，够引导期用，用完就 `panic`。
- **`pg_identity`**：用 4MB 大页把从 0 开始的地址一一恒等映射，覆盖到内存高水位；超出真实内存的部分被标记为不可缓存（PWT|PCD），避免把 MMIO 之类当普通内存写坏。
- **`pg_mapkernel`**：用 4MB 大页把内核物理地址映射到它的高半区虚拟地址，返回第一个空闲页目录项。
- **`pg_map`**：通用映射，用 4KB 小页，还能传一个 `PG_ALLOCATEME` 哨兵让它自动去 `pg_alloc_page` 取页。

恒等映射与内核映射都只用 4MB 大页，但两者的 flag 组合不同（`minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity`、`minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel`，各节选核心行）：

```c
// pg_identity：从 0 起逐 4MB 段恒等映射
u32_t flags = I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_USER | I386_VM_WRITE;
if ((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB) <= (phys & I386_VM_ADDR_MASK_4MB))
    flags |= I386_VM_PWT | I386_VM_PCD;   // 超出真实内存的段：禁缓存
pagedir[i] = phys | flags;

// pg_mapkernel：内核高半区映射——注意，没有 I386_VM_USER
pagedir[pde] = kern_phys | I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_WRITE;
```

> **辨析一：越界段为何要禁缓存（PWT|PCD）。** `pg_identity` 把从 0 到 4GB 全用大页铺满，但真实 DRAM 只到 `mem_high_phys`。高水位之上那些"地址存在却不是内存"的段，其实是 MMIO 设备寄存器或地址空洞。给它们开着缓存，写下去会滞留在 cache、读上来会拿到陈旧值，设备"写后读"的语义当场被破坏。所以一旦 4MB 段号越过 `mem_high_phys`，就补上 PWT（写穿透）+ PCD（不进 cache）——这正是源码注释那句"map memory that does not correspond to physical memory as non-cacheable"的意思。
>
> **辨析二：恒等映射设了 USER，内核映射却不设，这种不对称是有意的。** `pg_identity` 的 flags 里带着 `I386_VM_USER`，看着像给用户态开了门——其实 boot 阶段 CPU 在 Ring 0，U/S 位对内核自身的访问根本不生效，它是个"无所谓"的位。真正表达意图的是 `pg_mapkernel`：它刻意**不**设 `I386_VM_USER`，把内核高半区标成 supervisor-only。而这份恒等映射也留不到运行期——VM 服务在 `pt_init`（`minix3/minix/servers/vm/pagetable.c:pt_init`）里显式跳过所有 BIGPAGE 条目、另建自己的页目录，所以 boot 恒等映射从 VM 接手那刻就被丢弃，任何 Ring 3 进程都用不到它。Rust 版更干脆：boot-shim 页表只在内核 boot 期使用、从不递给用户态，因此压根不设 USER 位。

**为什么 6 个页表页在 C 里够用、在 Rust 里不够？** 因为 32 位是两级页表（页目录 + 页表），引导期只需覆盖很少的映射；64 位是四页层级（x86-64 的 PML4→PDPT→PD→PT），同样的地址范围要动用多得多的页表页。一张静态的 6 页池，撑不住 64 位引导建表的需求——这正是 Rust 版把"静态页表池"换成"bump 分配区"的动机。

### 2.5 启用分页的寄存器顺序：一个不能错的时序

`vm_enable_paging`（`minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`）是一段对 CR0/CR4 的精细操作，它的价值在于**顺序本身承载语义**：

1. 断言 CPU 已在保护模式（CR0.PE 置位）——引导加载器应当把我们放在这里。
2. **先清掉 PG 和 PGE**：因为 PGE（全局页）必须晚于 PG 打开，先把两者都清零，排除遗留状态。
3. 置 CR4.PSE：声明页表里有 4MB 大页。
4. 置 CR0.PG：真正打开分页。
5. 置 CR0.WP：打开写保护（内核态也尊重只读页）。
6. 若 CPU 支持，最后才置 CR4.PGE。

这段"清 → PSE → PG → WP → PGE"的时序，是硬件规定的依赖链。它在 Rust 版里不能原样搬——64 位用的是不同的控制位（比如 x86-64 的长模式位）——所以这段被抽象成架构 trait 各自的职责，OS 语义层看不见 CR0/CR4 的具体位（§3.6）。

C 版这段寄存器操作原样如下（`minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`，节选）：

```c
assert(cr0 & I386_CR0_PE);                        // 引导器应已把 CPU 置于保护模式
write_cr0(cr0 & ~I386_CR0_PG);                    // 先关 PG
write_cr4(cr4 & ~(I386_CR4_PGE | I386_CR4_PSE));  // 再清 PGE 和 PSE
/* ...重读 cr0/cr4... */
cr4 |= I386_CR4_PSE;  write_cr4(cr4);             // 声明用 4MB 大页
cr0 |= I386_CR0_PG;   write_cr0(cr0);             // 开分页
cr0 |= I386_CR0_WP;   write_cr0(cr0);             // 开写保护
if (pgeok) cr4 |= I386_CR4_PGE;  write_cr4(cr4);  // 最后才开全局页
```

> **辨析三：清 PSE 这步看着多余，实则是顺序的地基。** 为什么要先 `~(PGE | PSE)` 把这几个位全清、再从头设？源码注释给了两条硬约束：一是 `"PGE must be enabled after PG"`——PGE 依赖 PG 已开；二是 `"Our page table contains 4MB entries"`——页表用的是 4MB 大页，必须先声明 PSE、才能开 PG 去用这些条目。先把遗留状态归零、再按"清 PG/PGE → 开 PSE → 开 PG → 开 WP → 开 PGE"的次序重新点亮，才让"启用分页"在任何 BIOS/引导器残留下都得到确定的结果。

### 2.6 调用关系：一眼看全交接前的动作序列

```
head.S: multiboot_init（汇编入口，压 magic/ebx）
  └─ pre_init(magic, ebx)
       ├─ assert(magic == MULTIBOOT_INFO_MAGIC)
       ├─ get_parameters(ebx, &kinfo)
       │    ├─ memcpy(mbi, ebx)                 抄一份可信副本
       │    ├─ 解析 cmdline → param_buf
       │    ├─ memcpy(module_list)              拷模块清单
       │    ├─ add_memmap() × N                 建可用内存视图
       │    ├─ 把内核当作第 N+1 个模块
       │    ├─ overlaps() × (N+1)               重叠体检，冲突即 panic
       │    └─ cut_memmap() × (N+1)             切除已占用区间
       ├─ pg_clear()                            清页目录
       ├─ pg_identity(&kinfo)                   4MB 大页恒等映射（越界段禁缓存）
       ├─ kinfo.freepde_start = pg_mapkernel()  内核高半区映射（supervisor-only）
       ├─ pg_load() → write_cr3(pagedir)
       └─ vm_enable_paging()                    CR0/CR4：清→PSE→PG→WP→PGE
       → return &kinfo
head.S: call kmain(&kinfo)   （控制权正式易手，本篇到此为止）
```

---

## 第 3 章　Rust 设计：用类型系统重新表达这份交接

C 版把一切都写进了一个全局 `kinfo_t`。Rust 版要做的不是照搬字段，而是**基于 64 位多架构的需要，重新设计这份启动契约**——该保留的保留，该淘汰的淘汰，该新增的新增，并用类型系统把"交接必须是纯数据"这条约束钉死。

### 3.1 KernelInfo：字段不是抄 kinfo_t，而是按新架构重排

`KernelInfo`（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`）是 boot-shim 交给内核的核心结构，对应 C 的 `kinfo_t`。它的字段设计遵循一条原则：**保留的字段语义不变，被删的字段都是 32 位限制或诊断信息，新增的字段是 64 位多架构逼出来的**。

先看它的骨架（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`，节选代注释）：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub reserved_regions: &'static [MemoryRegion],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub free_upper_idx: Option<usize>,
    // …user_sp / kern_stack_top / syscall_entry / boot_modules…
    pub bootstrap_start: PhysBytes,   // Rust 端口恒 PhysBytes(0)
    pub bootstrap_len: u64,           // 恒 0，由 validate 硬断言
    pub platform_sources: &'static [PlatformDescSource],
    pub param_buf: &'static [u8],     // 当前恒 &[]
}
```

**保留但换类型**（语义不变，用 newtype 把地址的含义写进类型）：

| C 字段 | Rust 字段 | 说明 |
|---|---|---|
| `vir_kern_start` | `kern_virt_base: VirBytes` | 内核虚拟基址，newtype 区分虚拟/物理 |
| `_kern_phys_base` | `kern_phys_base: PhysBytes` | 内核物理基址 |
| `_kern_size` | `kern_size: u64` | 用 u64 而非 usize，杜绝在窄目标上截断 |
| `freepde_start` | `free_upper_idx: Option<usize>` | 建完映射后根级第一个空闲索引，跨 x86-64/aarch64/riscv64 统一 |
| `user_sp` | `user_sp: VirBytes` | 用户栈顶 |
| `module_list[]` | `boot_modules: &'static [BootModule]` | 启动模块清单，借用切片代替定长数组 |

**新增**（32 位单一架构不需要，64 位多架构才需要）：

- `kern_stack_top`、`syscall_entry`：内核初始栈顶与系统调用入口。C 里这些在 `prot_init` 阶段设置，Rust 提前放进启动契约，让高半区入口在接手第一眼就能看到。
- `platform_sources: &'static [PlatformDescSource]`：平台描述符句柄，承载 DTB/RSDP 物理地址跨镜像传递（见 3.4）。
- `reserved_regions: &'static [MemoryRegion]`：固件在退出服务时仍标记为"占用"的非自由物理区间。它的出现有一个非常具体的动机——见 3.5 的专门讨论。

**淘汰**（纯 32 位限制或非契约信息）：

- 整个 `multiboot_info`：UEFI/OpenSBI 根本不用 multiboot，这张结构在 Rust 版没有对应物。
- `mem_high_phys`：64 位下它隐含在内存映射里，无需单独跟踪。
- 4GB 截断上限：64 位地址空间充裕，这个天花板连同 `add_memmap` 里的截断逻辑一起消失。

这里要如实记录一个**当前的设计张力**：`param_buf`（引导参数字符串缓冲）在早期设计里被判定为"boot-shim 内部即可消化、不必传给内核"而计划移除，但实现最终**保留**了它，类型是 `&'static [u8]`，目前 boot-shim 传的是空切片 `&[]`（UEFI 启动选项尚未解析成 `key=value` 格式）。内核侧把空切片当作"没有引导参数"处理，与 C 里 `param_buf[0] == '\0'` 时的行为一致（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`）。它去还是留，是一个尚未收敛的决策，本篇按实现现状描述，评估留给后续。

### 3.2 BootShim trait：把固件差异关在实现里

boot-shim 的对外抽象是一个 trait：

```rust
pub trait BootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
```

（`os/libs/minix-boot/src/boot_shim.rs:BootShim`）

**为什么用 trait 而不是几个自由函数？** 三个理由，都直接来自这条交接的约束：

1. **类型安全**：trait 把"建内存映射、分配根页表、划 bump 区、退出固件服务"这几步捆成一个必须整体实现的原语。实现 `BootShim` 就得全部交付，编译器替你把关，不会出现"UEFI 那套配了内存映射却忘了退服务"。
2. **封装性**：UEFI 和 OpenSBI 各自的 helper 被收在各自的 impl 里，不可能张冠李戴地把 UEFI 的建映射和 OpenSBI 的分配根页拼到一起。
3. **可测试性**：内核单元测试可以注入一个 mock 实现，不需要真固件。

选哪个固件实现，由 Cargo 的 feature gate 在编译期决定；OS 语义层的代码只认 trait，不写 `#[cfg(target_arch)]` 之类的条件编译。这正是"抽象机制而非描述硬件"的落点——OS 层要的是"准备好启动上下文"这个**机制**，不是"调用 EFI 服务表"这个**手段**。

### 3.3 两种交接形态：内存内联 与 跨镜像方案 A

C 版 `pre_init` 返回一个 `&kinfo` 全局指针，`head.S` 再把它传给 `kmain`——全程一个二进制，指针直接有效。Rust 版这里出现了 C 版没有的分岔，因为"boot-shim 和内核是不是同一个二进制"这件事，本身在演进。

**形态一（内存内联）**：内核 crate 直接链进 boot-shim 的 PE 镜像，`prepare_boot` 产出一个 `BootPrepareResult`（`os/libs/minix-boot/src/boot_shim.rs:BootPrepareResult`，含 `kernel_info` / `root_page` / `bump_base` / `bump_end`），内核侧的 `arch_boot` 就地接手。

**形态二（跨镜像，方案 A）**：boot-shim 建好引导页表、开了分页之后，**绝对跳转**进一个独立的、链接在高半区的 `kernel.elf` 的 `_start`。两个镜像各自链了自己的 `minix_kernel` 副本，**不共享 `.bss` 全局**——所以 boot-shim 在物理内存里建立的一切，必须通过一块显式的数据块带过去。这块数据就是 `BootHandoff`（`os/libs/minix-boot/src/handoff.rs:BootHandoff`），它比 `BootPrepareResult` 多带一个 `bump_next` 游标：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoff {
    pub kernel_info: KernelInfo,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
}
```

**为什么方案 A 不是锦上添花，而是必须的？** 它修的是一个 aarch64 上的根因缺陷。内联形态下，内核代码住在低地址的恒等映射窗口里，而这个窗口归 TTBR0 管；aarch64 一旦切到某个进程的地址空间（换 TTBR0 并 `tlbi alle1is`），第一个被抹掉的就是内核自己取指用的映射——下一条指令就 fault。方案 A 从源头解决：boot-shim 只做那些"必须在固件/PE 上下文里做"的事（收集信息、建表、开分页），然后把执行权交给住在**永不被切换的 TTBR1 高半区**的独立内核镜像。从那一跳之后，整个内核引导都跑在高虚拟地址上，对根切换天然免疫。

`BootHandoff` 之所以敢用裸结构体按值拷贝跨镜像传递，靠的是三重保证：`#[repr(C)]` 钉死字段顺序、`Clone + Copy` 保证是纯数据、它的切片载荷指向"引导窗口内一直存活"的物理内存，内核镜像在第一次根切换之前就能通过仍在生效的恒等映射读到。一条编译期断言还兜住尺寸：整个 blob 不得超过一个 4KiB 页，超了就编译失败，而不是悄悄写穿到下一次 bump 分配上。

**这份交接契约里，`#[repr(C)]` 出现在每一层**（`KernelInfo`、`MemoryRegion`、`BootModule`、`PlatformDescSource`、`BootHandoff`）。它的意义不是"要和 C 互操作"，而是**把字段顺序和每个 POD 字段的偏移钉死**，好让写它的 boot-shim 和读它的内核镜像这两个分开编译的镜像，对同一块内存有完全一致的理解。

### 3.4 平台描述符句柄：不传函数指针的另一种写法

第 1.3 节立的"只传数据、不传函数指针"规矩，在这里落到具体类型。问题很实际：内核之后要解析 DTB 或 ACPI 表来认识硬件，这些解析逻辑在 `minix-platform` crate 里。最省事的做法是在交接时传一个函数指针 `fn(PhysBytes) -> ...`，但那个指针的地址解析在 boot-shim 镜像里、函数体在另一处，一旦 boot-shim 内存被回收，指针就悬垂。

Rust 版的解法是传一个**不透明分类标签 + 物理地址**：

```rust
pub struct PlatformDescSource {
    kind: PlatformDescKind,   // 一个包着 u32 的不透明标签
    phys_addr: PhysBytes,
}
```

（`os/libs/minix-boot/src/platform.rs:PlatformDescSource`）

上层只看见 `(kind, phys_addr)` 这对纯数据，看不见 `kind` 到底是 DTB 还是 RSDP——那些品牌名活在 `minix-platform` 的解析层，通过 `parse_by_kind()` 按标签分发。`PlatformDescKind` 是 `Copy` 且只含一个 `u32`，DTB=1、RSDP=2 这些常量放在交接层，让 boot-shim 不必依赖 `minix-platform`（那会拖进架构相关的解析代码）就能构造句柄。

**DTB 与 RSDP 可以共存**：真实的 ARM64 SBBR 服务器可能同时给 DTB 和 ACPI。`platform_sources` 因此是一个**有序列表**，内核按顺序试，取第一个能成功解析的——这套模型对齐 Linux 的 `acpi=on/off/force`。注意本篇只讲到"句柄如何跨边界安全传递"这一层；描述符**被解析成什么、承载哪些硬件字段**，是平台发现那一篇的主题，这里不展开。

### 3.5 reserved_regions：一条被真实故障逼出来的字段

`KernelInfo.reserved_regions`（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`）值得单独讲，因为它的存在完全是"外部可观察行为"逼出来的，最能体现 Rewrite 不是照搬 C。

在 UEFI/OVMF 下，内核被静态链进 boot-shim 的 PE 镜像，运行时经 EDKII 重定位后，它的代码、GDT、IDT、TSS 其实待在**低物理地址**（约 `0x1da–0x1db_xxxxxx` 这一带），而不是那个从未真正执行过的独立 `kernel.elf` 副本的高半区链接地址。问题是：VM 给每个进程建的页表，如果不把这些"内核其实跑在里面的低地址恒等窗口"也映射进去，那么第一次 `mov cr3` 切到该进程页表的下一条取指就会缺页、直接 triple-fault。

于是 boot-shim 把固件在退出服务时标记为占用的那批非自由内存区间，连同内核运行时的恒等窗口一起，通过 `reserved_regions` 交给内核；内核在给进程建页表时，把这些窗口按恒等（VA=PA、仅特权态）补进去。OpenSBI/U-Boot 和宿主测试传空切片——它们不把内核链进 shim，没有这个运行时窗口。

这个字段的教训是普适的：**同一份"启动契约"，在不同固件形态下必须携带的信息并不相同**。C 版单一 x86 世界不会遇到这个，跨到 UEFI 重定位 + 多架构才暴露出来。

### 3.6 多架构页表抽象：只抽象"交接需要成立"的那部分

C 版的 `pg_*` 函数族，直接操作 32 位的页目录/页表/CR0-CR4。Rust 版把这些抽象进架构边界，OS 语义层只依赖"建立映射、启用分页"这样的**机制**，看不见 PML4/TTBR/SATP 这些**形状**：

| 架构 | 页表层级 | 根寄存器 |
|---|---|---|
| x86-64 | PML4 → PDPT → PD → PT（4 级） | CR3 |
| aarch64 | L0 → L1 → L2 → L3（4 级，TTBR0/TTBR1 双根） | TTBR0 / TTBR1 |
| riscv64 | Sv39：VPN[2] → VPN[1] → VPN[0]（3 级） | SATP |

C 的静态页表池 `pagetables[6][1024]` 在 64 位下撑不住，换成由 `bump_base`/`bump_end` 界定的 **bump 分配区**：引导期建表需要的页，从这段连续物理内存里顺序切取。

这里要守住一条边界，否则本篇会越界吃下分页机制那一篇的内容：**本篇只在"控制权交接成立所必需"的深度上讲页表**——即"交接那一刻，恒等映射 + 内核高映射 + 分页已打开"这个必须为真的状态。至于 64 位多级页表怎么增量地映射/解映射、direct_map 怎么建立、非法状态怎么用类型封堵，属于运行期内存管理，归后续文档。

### 3.7 bootstrap（unpaged 段）概念在 64 位下的遗留

C 版内核先在低地址的 unpaged 段跑，切到高地址后把这段回收成可用内存（`add_memmap`）。64 位高半区从第一条指令就在高地址运行，根本没有独立的 unpaged 段，这个概念随之失去实体。

`KernelInfo` 仍保留 `bootstrap_start`/`bootstrap_len` 两个字段，但语义变了：boot-shim 恒定传 `(PhysBytes(0), 0)`，`validate()` 更是直接把 `bootstrap_len == 0` 立为硬不变量（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）。这两个字段是保留、还是彻底删除，仍是一个开放的取舍；本篇按实现现状——"保留 + guard 恒不触发"——描述。

---

## 第 4 章　实现落地：控制权真正易手的那一刻

### 4.1 arch_boot：内核接手的第一动作

内核侧的入口是 `arch_boot(kernel_info, root_page)`（`os/kernel/src/lib.rs:arch_boot`）。它做三件事：把 `KernelInfo` **按值**收进内核自己的 `.bss` 全局、调用 `arch_boot_impl` 用根页表建立恒等 + 内核高映射并启用分页、最后经 `HigherHalf::jump_to_kmain` 跳到 `kmain`。

```rust
store_kernel_info(kernel_info);                     // 入口即按值拷进内核 .bss
let kinfo: &'static KernelInfo = crate::kernel_info()
    .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
register_panic_diagnostic();                        // 诊断注册前移：arch_boot 阶段的 panic 必须可见
arch_boot_impl::<X86_64Paging>(kinfo, root_page);   // 建恒等 + 内核高映射 + 开分页
unsafe { X86_64HigherHalf::jump_to_kmain(kinfo, kinfo.kern_stack_top) }
```

其中"第一件事"藏着一个真实的坑，值得作为经验讲出来。最早的实现是把 shim 栈上的 `&KernelInfo` 通过一段 `noreturn` 内联汇编的入参寄存器直接传给 `kmain`。但在 x86-64 System V（以及 aarch64 的 x0）里，那个参数寄存器是 **caller-saved**：中间任何一次函数调用都可能把它覆写。一旦覆盖发生，`kmain` 拿到的"内核信息指针"其实是别的值——现场曾观测到它恰好落进一段调试字符串的开头，`validate` 读到一堆 ASCII 垃圾当场 panic。修复是釜底抽薪：入口第一时间把 `KernelInfo` 拷进 `.bss` 全局，此后建表、跳 `kmain` 一律用这个全局副本的引用，因为全局副本的地址是链接期就定死的相对常量，不经过任何寄存器交接。

### 4.2 方案 A 的两段式：boot-shim 侧 与 内核镜像侧

跨镜像形态把上面这一步拆成两半，各跑在各的镜像里。

**boot-shim 侧 `bootstrap_to_kernel_image`**（`os/kernel/src/lib.rs:bootstrap_to_kernel_image`，aarch64）：校验并建好引导根页表、启用分页，然后从 bump 区顺序切一个 4KiB 页，把 `BootHandoff` 写进去，最后用一段绑定了 `x0`（承载 blob 物理地址）的高半区绝对跳转，`br` 进 `kernel.elf` 的 `_start`。跳转目标在高半区（bit55 置位），走永不被切换的 TTBR1，跳转本身不可能缺页。

**内核镜像侧 `arch_boot_resume_high_half`**（`os/kernel/src/lib.rs:arch_boot_resume_high_half`，aarch64）：这是接手后的续跑，它**只允许做纯增量**的动作——校验 `KernelInfo`、把 bump 游标续上（不重发已给的页）、把 `KernelInfo` 落进这个镜像自己的 `.bss`、建立 direct map、进 `kmain`。它**绝不能重建页表**：那个根此刻正活着并映射着这段代码本身，重建会清零正在用的根，下一条取指就 fault。于是 boot-shim 建根、内核镜像续用同一根，职责在两个镜像之间干净地切开。

### 4.3 validate：类型系统管不到的那几条运行时不变量

`KernelInfo` 的字段全是 `pub`（boot-shim 要构造、内核要读、测试要断言），但有几条不变量是类型表达不了的，由 `KernelInfo::validate` 在 `kmain` 入口集中检查（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）：

1. `bootstrap_len == 0`——Rust 端口无 unpaged 段，非零会触发回收、可能吞掉固件区。
2. `kern_size > 0`——boot-shim 必须真的加载了内核。
3. `kern_stack_top` 16 字节对齐——满足 AAPCS64 / SysV 栈对齐。
4. `memmap` 非空——至少一段可用内存。
5. `boot_modules` 非空——至少 PM/VM/VFS/RS。
6. `param_buf.len() <= 1024`——对齐 C 的 `MULTIBOOT_PARAM_BUF_SIZE`。

任何一条不过都是 `panic`：这是一条 fail-fast 契约，产出非法 `KernelInfo` 的 boot-shim bug 不可恢复，越早炸越好。

```rust
pub fn validate(&self) {
    assert_eq!(self.bootstrap_len, 0, "…no unpaged section…");
    assert!(self.kern_size > 0, "…boot-shim did not load kernel?");
    assert!(self.kern_stack_top.0.is_multiple_of(16), "…16-byte aligned…");
    assert!(!self.memmap.is_empty(), "…memmap must not be empty…");
    assert!(!self.boot_modules.is_empty(), "…boot_modules must not be empty…");
    assert!(self.param_buf.len() <= 1024, "…MULTIBOOT_PARAM_BUF_SIZE…");
}
```

### 4.4 三架构：机制各异，语义同一

boot-shim 这件事，x86-64、aarch64、riscv64 三家的**语义动作完全一致**——把散落在固件接口里的信息整理成一份 `KernelInfo`，建好恒等映射与内核高映射、启用分页，然后把控制权交给 `kmain`。差异全在**机制形状**这一层：

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 页表根 | 单个 CR3 | TTBR0 + TTBR1（**双根**，低半/高半各一） | 单个 SATP |
| 层级 | PML4→PDPT→PD→PT | L0→L1→L2→L3 | Sv39 三级 |
| 内存映射来源 | UEFI 服务取 EFI memory map | UEFI 或 OpenSBI | OpenSBI 经 SBI ecall |
| 平台句柄 | RSDP（ACPI） | DTB 或 RSDP 共存 | DTB |
| 早期控制台 | ISA 串口 | PL011 MMIO | SBI ecall console |
| 交接形态 | 内联 `arch_boot` | 内联 `arch_boot` + 方案 A 跨镜像 | 内联 `arch_boot` |
| `reserved_regions` | 非空（内核重定位的低物理恒等窗口） | 依固件 | 空切片（不把内核链进 shim） |

> **本质语义是什么。** 上表里"双根 vs 单根""CR3 vs TTBR vs SATP""内联 vs 跨镜像"全是**硬件承载的形状**，不是 OS 语义。按跨架构四公理（`prompt/review-rules/review-core-semantics.md` §1.6），OS 语义层只要求"每个进程有自己的地址翻译空间""交接是纯数据"——它压根不把"页表根的数量"当语义。aarch64 用两个根、x86-64 用一个根，都是同一 OS 语义的合法实现。所以 boot-shim 对内核暴露的接口（`BootShim` trait + `KernelInfo` + `arch_boot`）三家共用一份，固件与页表的差异关在各架构实现里，不泄漏到 OS 语义层——这正是"抽象语义，不描述机制"的落点。（三个早期控制台后端在 `os/libs/minix-boot/src/platform.rs:ConsoleDesc` 的文档里列名——IsaSerialDesc / MmioSerialDesc / SbiConsoleDesc——实现各在 `minix-platform` 的 arch 子模块。）

---

## 第 5 章　测试要点

这份启动契约的正确性，分单元测试与真机端到端两层守。

**契约不变量测试**：`validate()` 的正常与六条拒绝分支各自有单元测试——合法信息不 panic，非零 `bootstrap_len`、零 `kern_size`、错位的栈顶、空的 `memmap`、空的 `boot_modules`、超长 `param_buf` 各触发对应的 `should_panic`（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo` 的 tests 模块）。这直接对应第 4.3 的六条不变量。

**纯数据 / 跨二进制安全测试**：验证 `PlatformDescSource` 是 `Copy`、只含 `(u32 kind, u64 phys_addr)`，构造/取回/拷贝后字段一致（`os/libs/minix-boot/src/platform.rs` 的 `test_platform_desc_source_copy`）。这条守的是第 1.3 立的"只传数据、不传函数指针"边界——它保证交接契约不会因为内核独立成 ELF 而变成悬垂引用。

**KernelInfo 构造与模块装载测试**：契约的"生产者"一侧由几组单元测试钉住——`test_build_kernel_info_fields`（`os/boot-shim/src/uefi_helpers.rs`）验证 UEFI 侧把内核位置、内存映射等字段填对；`test_build_kernel_info_uses_riscv64_user_sp`（`os/boot-shim/src/opensbi_helpers.rs`）验证 riscv64 侧的用户栈顶；`test_load_boot_modules_loads_all_in_contract_order`、`test_first_overlapping_pair_disjoint`/`_overlap`、`test_load_kernel_with_loader_computes_layout`（`os/boot-shim/src/loader.rs`）验证启动模块按契约顺序装载、以及内核与模块的重叠体检（对应第 2.2 的 `overlaps`/`cut_memmap` 语义）。

**真机端到端引导测试**：`prepare_boot → 建表 → 启用分页 → 跳 kmain` 这条完整交接链，由 `os/qemu-tests/test-kernels/kernel/bootstrap/` 下的三架构引导测试覆盖——`hello-boot`、`test-kernel-map`、`test-paging-enable`、`test-higher-half`、`test-protection` 各有 x86-64/aarch64/riscv64 变体。它们跑真固件路径，验证的是第 2.1 那套"交接前机器必须变成什么样"在硬件上确实成立。

### 尚未覆盖、后续要补的保障

- `free_upper_idx` 目前 boot-shim 传 `None`、由内核用前校验（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`），"这个根级空闲索引最终由谁负责计算"还没有测试或明确定论，应补齐。
- 第 4.1 那个"引用跨 `noreturn` 内联汇编经 caller-saved 寄存器交接"的陷阱，目前只靠"入口即拷进 `.bss` 全局"这一约定守着，缺一个能主动捕获该形态的静态或运行期检查。

---

## 第 6 章　启动失败与诊断：交接链炸了怎么定位

boot 阶段还没有完整的中断、调度与 IPC，一旦出错就是一台哑机器。这一阶段的可观测性靠三根支柱撑起来。

### 6.1 EarlyConsole：内核接手后的第一根路标

`arch_boot` 入口第一时间通过 EarlyConsole 把原始参数指针打出来（`os/kernel/src/lib.rs:arch_boot`），紧接着打"已进入 arch_boot"。为什么第一动作就是打指针？因为要判定 boot-shim→arch_boot 的**第一跳交接是否已经坏**——如果连入口打印的地址都是垃圾，问题在进内核之前；如果地址正常但后续 `validate` 挂了，问题在拷贝/建表环节。EarlyConsole 是个 trait，x86 走串口、aarch64 走 MMIO UART、riscv64 走 SBI console ecall，品牌名关在各架构实现里（`os/libs/minix-boot/src/platform.rs:ConsoleDesc`）。

与 EarlyConsole 配套的是 panic 诊断注册的**时机前移**：`register_panic_diagnostic` 被提到 arch_boot 一开始就调用，因为 arch_boot 阶段的任何 panic 都必须可见——此时 EarlyConsole、栈回溯这些依赖全部就绪，注册没有理由晚于第一个可 panic 的校验。

### 6.2 validate fail-fast：错误信息即诊断

第 4.3 那六条 `assert` 不是摆设：每条都带描述性消息（如 `bootstrap_len must be 0`、`memmap must not be empty`）。boot 期 panic 的第一现场基本就是这些消息之一，直接指向是 boot-shim 漏填了哪个字段。

### 6.3 两条真实取证签名

| 症状 | 根因 | 定位锈点 |
|---|---|---|
| `validate` 读到一堆 ASCII 垃圾当场 panic，kinfo 指针恰为某调试串地址 | 引用跨 `noreturn` 内联汇编经 caller-saved 寄存器交接被覆写（§4.1） | `os/kernel/src/lib.rs:arch_boot` |
| 刚 `mov cr3` 切到进程页表后下一条取指就 #PF，报错地址落在低物理恒等窗口 | 该窗口未回放进进程页表（§3.5） | `os/libs/minix-boot/src/kernel_info.rs:KernelInfo` |

这两条都是 boot 阶段"哑机器"的典型形态：没有内核日志可回溯，只能靠 EarlyConsole 的第一根路标 + 报错地址落在哪个区间来判断。它们也印证了本篇的主线——**交接前机器必须变成什么样**一旦没满足，故障会以最难定位的方式在交接刚完成后暴露。

### 6.4 可复制的定位命令

```bash
# 端到端引导回归：boot 哑机器时先看串口是否打出 PASS 标记
#   （run_qemu.sh 接收起 test-kernel 与 arch，用法见 os/qemu-tests/README.md）
os/qemu-tests/run_qemu.sh <test-kernel> <arch>

# 判断 reclaim guard 是否恒不触发（§3.5 fix27 现场）：生产构造点应全传 (0,0)
rg -n "bootstrap_len" os/kernel/src/lib.rs os/libs/minix-boot/src/kernel_info.rs

# 确认跨镜像交接只传纯数据（§1.3）：blob 定义与尺寸断言
rg -n "struct BootHandoff|size_of::<BootHandoff>" os/libs/minix-boot/src/handoff.rs
```
