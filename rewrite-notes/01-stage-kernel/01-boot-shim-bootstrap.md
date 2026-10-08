# 01 · 从固件到内核：控制权交接前，机器必须变成什么样

> **C 真值源**: `minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/arch/i386/pg_utils.c`
> **Rust 实现**: `os/libs/minix-boot/src/kernel_info.rs`、`boot_shim.rs`、`handoff.rs`、`platform.rs`、`os/boot-shim/src/{main,loader,uefi_helpers,opensbi_helpers}.rs`、`os/kernel/src/lib.rs:arch_boot`

## 第 1 章　概念：CPU 在跳进内核第一条指令之前需要回答的问题

### 1.1 矛盾的起点：固件提供的是接口，不是契约

一台刚上电的机器，并不"欠"内核任何东西。CPU 复位后从固件手里接过执行权，此时内存里已经躺着一份内核镜像，但内核还没有页表、没有可用的栈、甚至不知道自己被放在物理内存的哪个位置。真正棘手的地方在于：**固件告诉内核"你可以调用我"，却没有告诉内核"世界长什么样"**。

在 32 位 x86 的 Minix3 时代，这个鸿沟由引导加载器 GRUB 填平：GRUB 读取配置文件、把内核 ELF 和一批启动模块（PM、VM、VFS、RS 等用户态服务器镜像）加载到物理内存，再把一张描述"内存布局 + 模块清单"的 multiboot 信息结构交给内核。内核只需要**解析**这张结构，剩下的加载活儿 GRUB 都干完了。

到了 64 位多架构（x86-64、aarch64、riscv64）的世界，情况变了。现代固件——UEFI 和 SBI——只提供**服务接口**，不会替你把内核和模块搬进内存、也不会给你一张现成的布局表。于是必须有一个组件，专门把"固件接口能问出来的信息"转化成"内核可以直接消费的启动契约"。这个组件就是 **boot-shim**。一句话概括它的历史位置：**GRUB 的职责，被搬进了内核自带的一段引导代码里。**

### 1.2 boot-shim 交付的三样东西，与整条启动链路

无论跑在哪个架构、面对哪种固件，控制权真正交到内核手里的那一刻，内核必须能回答三个问题：**我站在哪里**（内核镜像的物理与虚拟地址、边界）、**哪里还能用**（哪些物理内存空闲、哪些被内核/模块/固件占据）、**我要带着谁走**（启动阶段要拉起的用户态服务器镜像在哪里、叫什么）。boot-shim 的全部工作，就是把这三个答案连同为它们服务的页表根，打包成一份**纯数据**一次交给内核。

把整条链路画出来，才能看清 boot-shim 卡在哪个位置：

```
（x86-64 示意；aarch64 走 UEFI/OpenSBI→EL1，riscv64 走 OpenSBI→S-mode，机制同构）
CPU 上电 / 复位
  └─ 固件（BIOS→GRUB，或 UEFI，或 OpenSBI 的 M-mode 阶段）
       ├─ 自检、定位可引导介质
       ├─ 把内核镜像 + boot 模块(PM/VM/VFS/RS) 搬进物理内存
       ├─ 退出启动服务（ExitBootServices / SBI 移交）
       └─ 跳转到内核入口，带着"从固件接口能问出的原始信息"
            └─ boot-shim（内核自带的引导代码，GRUB 职责的承接者）
                 ├─ 问固件要内存映射、解析内核 ELF 与模块
                 ├─ 构造启动契约 KernelInfo（纯数据）
                 ├─ 建引导页表（恒等映射 + 内核高映射）、启用分页
                 └─ 交出控制权给内核
                      └─ arch_boot → kmain(kinfo)   ← 本篇到此为止
```

### 1.3 交接为什么必须"只传数据、不传函数指针"

这是本篇最核心的一条约束，它决定了后面所有接口的形状。boot-shim 和内核在最终形态里是**两个独立的二进制镜像**：一个是 UEFI 应用（PE 格式），一个是链接到高半区的 `kernel.elf`。引导完成后，boot-shim 占用的内存随时可能被回收或重新映射。如果交接时传的是一个**函数指针**，一旦 boot-shim 镜像消失，指针就成了悬垂引用，内核照着跳过去就是灾难。所以交接契约被约束成：**跨边界传递的东西，必须全部是值语义的纯数据**。需要传递"能力"时，不传函数指针，而传一个**不透明分类标签**加一个**物理地址**，让内核在自己这边查表决定怎么处理。

> 判据来自项目的一条硬边界：启动契约是"平台把系统送入 OS 的方式"，不是 OS 运行时机制，因此不强行与运行时抽象统一（见 `prompt/review-rules/review-core-semantics.md` §1.6 三条边界之"启动契约不在抽象射程内"）。

### 1.4 启动前系统状态，以及哪些历史包袱不必深究

控制权交到内核第一段引导代码时，机器的状态是这样的：

| 维度 | 状态 |
|---|---|
| CPU 特权级 | 已在高特权级（x86 保护模式 / aarch64 EL1 / riscv64 S-mode），但还没开分页 |
| 寄存器 | 带着固件给的入口信息（C 版是 EAX=魔数、EBX=multiboot 结构地址；UEFI 是镜像基址；OpenSBI 是 DTB 指针） |
| 物理内存 | 内核/模块已被搬进去，但布局要靠 boot-shim 自己去问、去整理 |
| 页表 | 不存在（分页未开） |
| 堆 / 分配器 | 不存在（还没到能 malloc 的时候） |
| 中断 | 不存在（IDT/向量表未建） |

> **哪些值得理解、哪些可以安全跳过。** 表里"保护模式"是 x86 的术语，但它背后的语义——**内核跑在高特权级、用户态跑在低特权级**——是所有架构共享的（ARM 叫 EL1/EL0）。那些"自己写 OS"的书里大段讲的实模式、A20 地址线、分段机制，是 8086/80286 时代为兼容性打的补丁，ARM 一样也没有，照样跑操作系统；而且固件早已把它们处理完，内核第一行代码就已经在高特权级里了。**值得搞懂的是"为什么需要特权级隔离"，本篇不必深究的是"GDTR 怎么加载、A20 怎么开启"**——它们属于固件与引导加载器的领地，不在 boot-shim 的职责里。

---

## 第 2 章　C 源码：Minix3 如何回答"控制权易手前必须做什么"

Minix3 内核侧的这段故事集中在两个文件：`minix3/minix/kernel/arch/i386/pre_init.c` 和 `minix3/minix/kernel/arch/i386/pg_utils.c`。先看清 C 原版怎么做的，才能判断 Rust 版哪些是"保持外部语义的重写"、哪些是"64 位多架构逼出来的演进"。

### 2.1 pre_init：内核第一个 C 函数的五步序列

`pre_init(magic, ebx)`（`minix3/minix/kernel/arch/i386/pre_init.c:pre_init`）是内核被跳进来后执行的第一个 C 函数，整段函数体本身就是一份"交接清单"：

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

① 的魔数不是礼貌检查：不对就说明我们根本不是被 multiboot 引导器跳进来的，后面所有对 `ebx` 所指结构的信任都不成立——这是个典型的**哨兵魔数**。③④⑤⑥⑦ 的次序也不是随意的。

**为什么第 ④ 步（恒等映射）必须在第 ⑤ 步（内核高映射）之前、且都在启用分页之前？** 假设反过来——先只映射高地址的内核再开分页。那么开分页那一瞬，CPU 下一条要执行的指令（`pre_init` 自己的代码，此刻还跑在低地址）在页表里没有映射，立刻取指缺页、机器 triple-fault。恒等映射保证"开分页前后地址值不变、正在执行的代码无感过渡"，内核映射则**为了之后**——把内核该待的高地址一并铺好。两者叠加，才让"控制权从低地址引导上下文平稳移到高地址内核"成为可能。

### 2.2 get_parameters：把 multiboot 的散落信息整理成 kinfo_t

`get_parameters(ebx, cbi)`（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`）是"解析"的实体，Rust 版把它的职责全搬进了 boot-shim 内部。它的重头戏是建内存地图、再把已占用区间挨个切掉：

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

除此之外它还做几件事，每件都对应 Rust boot-shim 的一项职责：先 `memcpy` 一份自己的 multiboot 结构（原始结构在引导服务退出后会失效）；从链接期符号读出内核的虚拟/物理基址与大小；把命令行拆成 `key=value` 塞进 `param_buf`；把 `user_sp` 刻意压低一档，好让内核栈溢出撞进"留白"被发现、也让内核地址（`0xf..`）与用户地址（`0xe..`）一眼可辨。最后那个重叠体检尤其关键——它把内核当作第 N+1 个模块参与两两查重叠，任何两个镜像压在同一块物理内存上就当场 `panic`，否则后续建表会覆盖内核自身。

### 2.3 memmap 记账：可用内存视图的四种操作

`pg_utils.c` 提供一组对"可用物理内存"这张视图的原始操作。`add_memmap` 的 4GB 天花板藏在这几行（`minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap`）：

```c
#define LIMIT 0xFFFFF000
/* 把超过 4GB 的可用内存截断——32 位的其余部分处理不了更大地址 */
if (addr > LIMIT) return;
if (addr + len > LIMIT) { len -= (addr + len - LIMIT); }
addr = roundup(addr, I386_PAGE_SIZE);
len = rounddown(len, I386_PAGE_SIZE);
```

`cut_memmap`（`minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap`）的"切除可能劈成两半"逻辑：把重叠那段清零，剩余左、右两段再用 `add_memmap` 补回：

```c
cbi->memmap[m].mm_base_addr = cbi->memmap[m].mm_length = 0;
if (substart > memaddr) add_memmap(cbi, memaddr, substart - memaddr);   // 左残段
if (subend  < memend)  add_memmap(cbi, subend,  memend  - subend);      // 右残段
```

另有 `alloc_lowest`（从低地址端挑最小够长的一块，分后顺手 `cut_memmap`）与 `pg_alloc_page`（从高地址末端取一个空闲页，专供引导建表）。这四种操作合起来，就是"内核运行前谁在动这张内存地图"的完整答案——其中那个 `LIMIT` 4GB 天花板是纯纯的 32 位限制，64 位下必须消失，这也是 Rust 版不再单独跟踪 `mem_high_phys` 的原因（Rust 侧的角色重划见 §3.6、§4.2）。

### 2.4 32 位页表机制：静态池与两种粒度

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

> **辨析：越界段为何要禁缓存（PWT|PCD）。** `pg_identity` 把从 0 到 4GB 全用大页铺满，但真实 DRAM 只到 `mem_high_phys`。高水位之上那些"地址存在却不是内存"的段，其实是 MMIO 设备寄存器或地址空洞。给它们开着缓存，写下去会滞留在 cache、读上来会拿到陈旧值，设备"写后读"的语义当场被破坏。所以一旦 4MB 段号越过 `mem_high_phys`，就补上 PWT（写穿透）+ PCD（不进 cache）——正是源码注释那句"map memory that does not correspond to physical memory as non-cacheable"的意思。
>
> **辨析：恒等映射设了 USER，内核映射却不设，这种不对称是有意的。** `pg_identity` 的 flags 带着 `I386_VM_USER`，看着像给用户态开门——其实 boot 阶段 CPU 在 Ring 0，U/S 位对内核自身访问根本不生效，它是个"无所谓"的位。真正表达意图的是 `pg_mapkernel`：它刻意**不**设 `I386_VM_USER`，把内核高半区标成 supervisor-only。而这份恒等映射也留不到运行期——VM 服务在 `pt_init`（`minix3/minix/servers/vm/pagetable.c:pt_init`）里显式跳过所有 BIGPAGE 条目、另建自己的页目录，所以 boot 恒等映射从 VM 接手那刻就被丢弃，任何 Ring 3 进程都用不到它。Rust 版更干脆：boot-shim 页表只在内核 boot 期使用、从不递给用户态，因此压根不设 USER 位。

支撑这些映射的是**一张静态页目录** `pagedir[1024]`（4KB 对齐）和**一个静态页表池** `pagetables[6][1024]`（`alloc_pagetable`，6 个页表页，用完即 `panic`）。为什么 6 个页表页在 C 里够用、在 Rust 里不够？因为 32 位是两级页表、引导期覆盖很少；64 位是四页层级，同样地址范围要动用多得多的页表页——静态池撑不住，这正是 Rust 换成 bump 分配区的动机（§3.6）。

### 2.5 启用分页的寄存器顺序：一个不能错的时序

`vm_enable_paging`（`minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging`）是一段对 CR0/CR4 的精细操作，它的价值在于**顺序本身承载语义**：

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

> **辨析：清 PSE 这步看着多余（"看似多余的步骤"），实则是顺序的地基。** 为什么要先 `~(PGE | PSE)` 把这几个位全清、再从头设？源码注释给了两条硬约束：一是 `"PGE must be enabled after PG"`——PGE 依赖 PG 已开；二是 `"Our page table contains 4MB entries"`——页表用的是 4MB 大页，必须先声明 PSE、才能开 PG 去用这些条目。先把遗留状态归零、再按"清 PG/PGE → 开 PSE → 开 PG → 开 WP → 开 PGE"重新点亮，才让"启用分页"在任何 BIOS/引导器残留下都得到确定结果。

这段时序是硬件规定的依赖链，Rust 版不能原样搬（64 位用不同的控制位），所以被抽象成架构 trait 各自的职责，OS 语义层看不见 CR0/CR4 的具体位（§3.6）。

### 2.6 调用关系：一眼看全交接前的动作序列

```
head.S: multiboot_init（汇编入口，压 magic/ebx）
  └─ pre_init(magic, ebx)
       ├─ assert(magic == MULTIBOOT_INFO_MAGIC)
       ├─ get_parameters(ebx, &kinfo)
       │    ├─ memcpy(mbi, ebx)                 抄可信副本
       │    ├─ 解析 cmdline → param_buf
       │    ├─ memcpy(module_list)              拷模块清单
       │    ├─ add_memmap() × N                 建可用内存视图
       │    ├─ 把内核当第 N+1 个模块
       │    ├─ overlaps() × (N+1)               重叠体检，冲突即 panic
       │    └─ cut_memmap() × (N+1)             切除已占用区间
       ├─ pg_clear() / pg_identity()            恒等映射（越界段禁缓存）
       ├─ kinfo.freepde_start = pg_mapkernel()  内核高映射（supervisor-only）
       ├─ pg_load() → write_cr3(pagedir)
       └─ vm_enable_paging()                    CR0/CR4：清→PSE→PG→WP→PGE
       → return &kinfo
head.S: call kmain(&kinfo)   （控制权正式易手，本篇到此为止）
```

---

## 第 3 章　Rust 设计：用类型系统重新表达这份交接

C 版把一切写进一个全局 `kinfo_t`。Rust 版不照搬字段，而是**基于 64 位多架构重新设计这份启动契约**，并用类型系统把"交接必须是纯数据"钉死。

### 3.1 KernelInfo：字段不是抄 kinfo_t，而是按新架构重排

先看骨架（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`，节选带注释）：

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

字段取舍的完整对照如下（**保留的语义不变、删的都是 32 位限制或诊断信息、新增的是 64 位多架构逼出来的**）：

| C 字段 | Rust 处置 | 类型 / 理由 |
|---|---|---|
| `vir_kern_start` / `_kern_phys_base` | 保留·改型 | `kern_virt_base: VirBytes` / `kern_phys_base: PhysBytes`——newtype 把"虚拟/物理"写进类型 |
| `_kern_size` | 保留·改型 | `kern_size: u64`（不用 usize，杜绝窄目标截断） |
| `freepde_start` | 保留·改型 | `free_upper_idx: Option<usize>`，跨三架构统一；**boot-shim 现传 `None`、由内核用前校验** |
| `module_list[]` | 保留·改型 | `boot_modules: &'static [BootModule]`（借用切片代定长数组） |
| `multiboot_info` | 淘汰 | UEFI/OpenSBI 不用 multiboot |
| `mem_high_phys` / 4GB 截断 | 淘汰 | 64 位地址充裕，隐含在 memmap 里，无需单独跟踪 |
| `kernel_may_alloc` | 淘汰 | 全局标志不安全，用类型状态/显式 token |
| （无 C 对应） | 新增 | `kern_stack_top`、`syscall_entry`：C 在 `prot_init` 设置，Rust 提前放进契约 |
| （无 C 对应） | 新增 | `platform_sources`、`reserved_regions`（见 §3.4、§3.5） |
| `param_buf` | **保留但当前空** | `&'static [u8]`，boot-shim 现传 `&[]`——见下方辨析 |

> **辨析：`param_buf` 的"保留但为空"是一处诚实披露。** 早期设计曾判它"boot-shim 内部消化、不必传给内核"而计划移除，但实现最终保留为 `&'static [u8]`，只是当前 boot-shim 传空切片 `&[]`（UEFI 启动选项尚未解析成 `key=value`）。内核侧把空切片当"无引导参数"处理，与 C 里 `param_buf[0]=='\0'` 行为一致（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`）。它到底删除还是等 UEFI 解析落地后填充，是尚未收敛的决策，本篇按实现现状描述。

### 3.2 BootShim trait：把固件差异关在实现里

```rust
pub trait BootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}
```

（`os/libs/minix-boot/src/boot_shim.rs:BootShim`）

| 方案 | 内容 | 代价 | 取舍 |
|---|---|---|---|
| **trait（选用）** | 一个原语把交付面凝成一个 `prepare_boot(bump_pages) -> BootPrepareResult` | 略多样板 | ✅ 契约的**数据面**由类型强制：不交出 `KernelInfo`+`root_page`+`bump` 就编译不过；封装（不混用 UEFI/OpenSBI helper）；可注入 mock |
| 一组自由函数 | 每个固件导出几个函数 | 调用方得自己拼 | ❌ 容易漏交某个字段、或混用两套 helper |

选哪个固件实现由 Cargo feature gate 在编译期决定；OS 语义层只认 trait，不写 `#[cfg(target_arch)]`。这正是"抽象机制而非描述硬件"——OS 层要的是"准备好启动上下文"这个**机制**，不是"调用 EFI 服务表"这个**手段**。

> **辨析（一句常见误读）：trait 并不能保证实现内部"把四步都做了"。** 它只保证：两个固件都得交出一个字段齐全的 `BootPrepareResult`（这是类型强制的）。至于"内存快照必须在 `ExitBootServices()` 之前取"这类**步序**，trait 管不到——它是靠 impl 内部约定 + 源码注释声明"must be called before exit_boot_services" + 测试去守的，不是编译器把关。把"接口统一"说成"类型系统保证流程完整"是越界；前者真、后者假。

### 3.3 两种交接形态：内存内联 与 跨镜像方案 A

C 版 `pre_init` 返回一个 `&kinfo` 全局指针、`head.S` 再传给 `kmain`——全程一个二进制。Rust 版这里出现 C 版没有的分岔，因为"boot-shim 和内核是不是同一个二进制"本身在演进。**形态一（内存内联）**：内核 crate 链进 boot-shim PE，`prepare_boot` 产出 `BootPrepareResult`（`os/libs/minix-boot/src/boot_shim.rs:BootPrepareResult`：`kernel_info` / `root_page` / `bump_base` / `bump_end`），`arch_boot` 就地接手。**形态二（跨镜像方案 A）**：boot-shim 建表、开分页后**绝对跳转**进独立的高半区 `kernel.elf` 的 `_start`。两镜像各链自己的 `minix_kernel` 副本、**不共享 `.bss`**，所以 boot-shim 建立的一切必须经一块显式数据块带过去：

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoff {
    pub kernel_info: KernelInfo,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,   // 比 BootPrepareResult 多出：内核镜像续用 bump 不重发已给页
    pub bump_end: u64,
}
```

（`os/libs/minix-boot/src/handoff.rs:BootHandoff`）

> **反例推演：方案 A 为何是必须的、不是锦上添花。** 内联形态下 aarch64 内核代码住在低地址恒等映射窗口（归 TTBR0 管）。若沿用内联，aarch64 第一次切到某进程地址空间（换 TTBR0 + `tlbi alle1is`）时，第一个被抹掉的就是内核自己取指用的映射——下一条指令就 fault。方案 A 从源头解决：boot-shim 只做"必须在固件/PE 上下文里做"的事（收集信息、建表、开分页），然后把执行权交给住在**永不被切换的 TTBR1 高半区**的独立内核镜像；从那一跳之后整个内核引导都跑在高虚拟地址上，对根切换免疫。`BootHandoff` 敢按值拷贝跨镜像传递，靠三重保证：`#[repr(C)]` 钉死字段偏移、`Copy` 保证纯数据、切片载荷指向引导窗口内存活的物理内存；一条编译期断言 `size_of::<BootHandoff>() <= 4096` 兜住尺寸，超了 fail-fast 而非写穿下一次 bump 分配。

`#[repr(C)]` 出现在这份契约的每一层（`KernelInfo`、`MemoryRegion`、`BootModule`、`PlatformDescSource`、`BootHandoff`），意义不是"和 C 互操作"，而是**钉死字段顺序与偏移**，让写它的 boot-shim 和读它的内核镜像这两个分开编译的镜像对同一块内存有一致的理解。

### 3.4 平台描述符句柄：不传函数指针的另一种写法

§1.3 的规矩在这里落到具体类型。内核之后要解析 DTB/ACPI 认识硬件，解析逻辑在 `minix-platform` crate。最省事的做法是交接时传函数指针，但指针地址解析在 boot-shim 镜像、函数体在别处，boot-shim 内存一回收就悬垂。Rust 版传**不透明分类标签 + 物理地址**：

```rust
pub struct PlatformDescSource {
    kind: PlatformDescKind,   // 一个包着 u32 的不透明标签
    phys_addr: PhysBytes,
}
```

（`os/libs/minix-boot/src/platform.rs:PlatformDescSource`）

上层只看见 `(kind, phys_addr)` 这对纯数据，看不见 `kind` 是 DTB 还是 RSDP——品牌名活在 `minix-platform` 的解析层，靠 `parse_by_kind()` 按标签分发。`platform_sources` 是**有序列表**：真实 ARM64 SBBR 服务器可能同时给 DTB 和 ACPI，内核按顺序试、取第一个解析成功者（对齐 Linux 的 `acpi=on/off/force`）。本篇只讲到"句柄如何跨边界安全传递"；描述符被解析成什么、承载哪些硬件字段，是平台发现那一篇的主题。

### 3.5 reserved_regions：一条被真实故障逼出来的字段

`KernelInfo.reserved_regions`（`os/libs/minix-boot/src/kernel_info.rs:KernelInfo`）值得单独讲，它的存在完全由"外部可观察行为"逼出。在 UEFI/OVMF 下，内核被静态链进 boot-shim 的 PE 镜像，运行时经 EDKII 重定位后，它的代码、GDT、IDT、TSS 实际待在**低物理地址**（约 `0x1da–0x1db_xxxxxx`），而非从未执行的独立 `kernel.elf` 的高半链接地址。

> **反例推演：不给这些窗口建映射会怎样。** VM 给每个进程建的页表，如果不把"内核其实跑在里面的低地址恒等窗口"也映射进去，那么第一次 `mov cr3` 切到该进程页表后的**下一条取指就缺页、直接 triple-fault**。所以 boot-shim 把固件在退出服务时标记为占用的非自由区间，连同内核运行时恒等窗口，通过 `reserved_regions` 交内核，内核建进程页表时按恒等（VA=PA、仅特权态）补进去。OpenSBI/U-Boot 与宿主测试传空切片——它们不把内核链进 shim，没有这个运行时窗口。

它的教训是普适的：**同一份启动契约，在不同固件形态下必须携带的信息并不相同**。C 版单一 x86 不会遇到，跨到 UEFI 重定位 + 多架构才暴露。

### 3.6 多架构页表抽象：只抽象"交接需要成立"的那部分

C 版 `pg_*` 直接操作 32 位页目录/CR0-CR4，Rust 版把机制抽象进架构边界，OS 语义层只依赖"建立映射、启用分页"，看不见 PML4/TTBR/SATP 这些形状：

| 架构 | 页表层级 | 根寄存器 |
|---|---|---|
| x86-64 | PML4 → PDPT → PD → PT（4 级） | CR3（单根） |
| aarch64 | L0 → L1 → L2 → L3（4 级） | TTBR0 / TTBR1（**双根**，低半/高半各一） |
| riscv64 | Sv39：VPN[2] → VPN[1] → VPN[0]（3 级） | SATP（单根） |

C 的静态页表池 `pagetables[6][1024]` 在 64 位撑不住，换成由 `bump_base`/`bump_end` 界定的 **bump 分配区**。这里要守住一条边界：本篇只在"控制权交接成立所必需"的深度上讲页表（即"交接那一刻恒等映射 + 内核高映射 + 分页已开"这个必须为真的状态）。

> **认知减负：64 位多级页表怎么增量映射/解映射、direct_map 怎么建立、非法状态怎么用类型封堵，都是运行期内存管理的主题，归后续文档；本篇不展开，读者可安全跳过。**

### 3.7 bootstrap（unpaged 段）概念在 64 位下的遗留

C 版内核先在低地址 unpaged 段跑，切到高地址后把这段回收（`add_memmap`）。64 位高半区从第一条指令就在高地址运行，没有独立 unpaged 段，这个概念失去实体。`KernelInfo` 仍保留 `bootstrap_start`/`bootstrap_len`，但语义变了：boot-shim 恒传 `(PhysBytes(0), 0)`，`validate()` 把 `bootstrap_len == 0` 立为硬不变量（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）。这两个字段是保留、还是彻底删除，仍是一个开放的取舍；本篇按实现现状——"保留 + guard 恒不触发"——描述。

---

## 第 4 章　实现落地：控制权真正易手的那一刻

### 4.1 arch_boot：内核接手的第一动作

内核侧入口 `arch_boot(kernel_info, root_page)`（`os/kernel/src/lib.rs:arch_boot`）做三件事：

```rust
store_kernel_info(kernel_info);                     // 入口即按值拷进内核 .bss
let kinfo: &'static KernelInfo = crate::kernel_info()
    .expect("arch_boot: KERNEL_INFO store failed — store_kernel_info is one-shot");
register_panic_diagnostic();                        // 诊断注册前移：arch_boot 阶段 panic 必须可见
arch_boot_impl::<X86_64Paging>(kinfo, root_page);   // 建恒等 + 内核高映射 + 开分页
unsafe { X86_64HigherHalf::jump_to_kmain(kinfo, kinfo.kern_stack_top) }
```

> **辨析（顺序敏感 + 寄存器约定）：第一件事"拷进 .bss"藏着一个真实的坑。** 最早的实现把 shim 栈上的 `&KernelInfo` 通过一段 `noreturn` 内联汇编的入参寄存器直接传给 `kmain`。但 x86-64 System V（以及 aarch64 的 x0）里那个参数寄存器是 **caller-saved**：中间任何一次调用都可能覆写它。一旦覆盖，`kmain` 拿到的"内核信息指针"其实是别的值——现场曾观测到它恰好落进一段调试字符串的开头，`validate` 读到一堆 ASCII 垃圾当场 panic。修复是釜底抽薪：入口第一时间把 `KernelInfo` 拷进 `.bss` 全局，此后建表、跳 `kmain` 一律用这个全局副本的引用——全局副本的地址是链接期定死的相对常量，不经过任何寄存器交接。

### 4.2 方案 A 的两段式：boot-shim 侧 与 内核镜像侧

跨镜像形态把接手拆成两半，各跑在各的镜像里。boot-shim 侧 `bootstrap_to_kernel_image`（`os/kernel/src/lib.rs:bootstrap_to_kernel_image`，aarch64）：校验并建好引导根页表、启用分页，然后从 bump 区顺序切一个 4KiB 页、把 `BootHandoff` 写进去，最后用一段绑定了 `x0`（承载 blob 物理地址）的高半区绝对跳转 `br` 进 `kernel.elf` 的 `_start`——跳转目标在高半区（bit55 置位），走永不被切换的 TTBR1，跳转本身不可能缺页。

内核镜像侧 `arch_boot_resume_high_half`（`os/kernel/src/lib.rs:arch_boot_resume_high_half`，aarch64）：接手后的续跑，**只允许纯增量**——校验 `KernelInfo`、续上 bump 游标（不重发已给的页）、把 `KernelInfo` 落进本镜像 `.bss`、建 direct map、进 `kmain`。它**绝不能重建页表**：那个根此刻正活着并映射着这段代码本身，重建会清零正在用的根、下一条取指就 fault。于是 boot-shim 建根、内核镜像续用同一根，职责在两个镜像之间干净切开。

### 4.3 validate：类型系统管不到的那几条运行时不变量

`KernelInfo` 字段全 `pub`（boot-shim 要构造、内核要读、测试要断言），但几条不变量类型表达不了，由 `KernelInfo::validate` 在 `kmain` 入口集中检查（`os/libs/minix-boot/src/kernel_info.rs:fn validate`）：

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

> **辨析：为什么用运行时 `validate` 而不是类型封堵。** 这份数据来自另一个二进制，是"跨 ABI 的裸字节"——编译期类型再强，也约束不了对面镜像往这块内存里写了什么。所以对跨边界数据，"入口集中校验、非法即 fail-fast panic"是业界通行做法（Redox bootloader 解析后校验 multiboot 信息、Linux 入口检查 EFI/ATAG 参数），每条断言都带描述性消息，boot 期 panic 的第一现场基本就是这些消息之一。类型封堵只在能封处用：`free_upper_idx: Option<usize>` 就把"boot-shim 还没算这个值"编码进了类型。

### 4.4 三架构：机制各异，语义同一

boot-shim 这件事，三家**语义动作完全一致**——把散落在固件接口里的信息整理成 `KernelInfo`，建恒等与内核高映射、启用分页、交出控制权给 `kmain`。差异全在**机制形状**这一层：

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 页表根 | 单个 CR3 | TTBR0 + TTBR1（**双根**） | 单个 SATP |
| 内存映射来源 | UEFI 服务取 EFI memory map | UEFI 或 OpenSBI | OpenSBI 经 SBI ecall |
| 平台句柄 | RSDP（ACPI） | DTB 或 RSDP 共存 | DTB |
| 早期控制台 | ISA 串口 | PL011 MMIO | SBI ecall console |
| 交接形态 | 内联 `arch_boot` | 内联 + 方案 A 跨镜像 | 内联 `arch_boot` |
| `reserved_regions` | 非空（重定位恒等窗口） | 依固件 | 空切片 |

> **本质语义是什么。** 上表里"双根 vs 单根""CR3 vs TTBR vs SATP""内联 vs 跨镜像"全是**硬件承载的形状**，不是 OS 语义。按跨架构四公理（`review-core-semantics.md` §1.6），OS 语义层只要求"每个进程有自己的地址翻译空间""交接是纯数据"——它压根不把"页表根的数量"当语义。aarch64 用两个根、x86-64 用一个根，都是同一 OS 语义的合法实现。所以 boot-shim 对内核暴露的接口（`BootShim` + `KernelInfo` + `arch_boot`）三家共用一份，差异关在各架构实现里，不泄漏到 OS 语义层。（三个早期控制台后端在 `platform.rs:ConsoleDesc` 的文档里列名——IsaSerialDesc / MmioSerialDesc / SbiConsoleDesc——实现各在 `minix-platform` 的 arch 子模块。）

### 4.5 实地导览：从 `prepare_boot` 到 `kmain`，现在打开代码怎么顺

前面是设计判断；这里给一条"顺着读当前实现"的路径（锚点真实，按序去仓库里找）：

```
UefiBootShim::prepare_boot / OpenSbiBootShim::prepare_boot   (boot-shim 侧入口)
  ├─ 取内存映射 + 加载 kernel.elf/模块            (loader.rs / uefi_helpers.rs / opensbi_helpers.rs)
  ├─ 构造 KernelInfo（含 reserved_regions）       (build_kernel_info)
  └─ exit_boot_services() → 返回 BootPrepareResult   ← 退出固件服务（次序靠注释+测试守，非 trait）
        ↓ 交回内核侧
arch_boot(kernel_info, root_page)                (os/kernel/src/lib.rs:arch_boot)
  ├─ store_kernel_info → .bss 全局副本            ← 避 caller-saved（§4.1）
  ├─ arch_boot_impl → build_bootstrap_root_and_enable  (建恒等+内核高映射、开分页)
  └─ [内联] jump_to_kmain ─ 或 ─ [跨镜像/方案A] bootstrap_to_kernel_image → _start (02-higher-half-kernel.md)
        ↓
      kmain → 03-kmain-cstart.md
```

拿到这篇后沿这条链从 `prepare_boot` 往下点，就能看到"为什么交接必须是纯数据"在每一步具体长什么样——而不是只读到结论。

---

## 第 5 章　测试要点

这份启动契约的正确性，分单元测试与真机端到端两层守。**基线实测**（`cargo test` 现算，不抄表）：`minix-boot` 单元 17 通过（`validate` 正常与六条 `should_panic` 拒绝分支 + `PlatformDescSource` Copy + getters）；`boot-shim`（uefi feature）14 通过（`test_build_kernel_info_fields`、`test_load_boot_modules_loads_all_in_contract_order`、`test_first_overlapping_pair_*` 重叠体检、`test_load_kernel_with_loader_computes_layout`、bss 清零）。`opensbi` feature 下的 riscv 构造测试（`test_build_kernel_info_uses_riscv64_user_sp`）需带该 feature 才编译。端到端引导 `prepare_boot → 建表 → 启用分页 → 跳 kmain` 由 `os/qemu-tests/test-kernels/kernel/bootstrap/` 三架构引导内核覆盖（`hello-boot`、`test-kernel-map`、`test-paging-enable`、`test-higher-half`、`test-protection` 各 ×{x86-64,aarch64,riscv64}）。

> **诚实披露的缺口**：`free_upper_idx` 由谁最终计算尚无定论与测试；"引用跨 `noreturn` asm 经 caller-saved 寄存器交接"的陷阱目前只靠"入口即拷 `.bss`"约定守，无自动拦截；方案 A 的 `BootHandoff` blob 写入/游标推进无宿主单元断言，靠真机门端到端覆盖。

---

## 第 6 章　启动失败与诊断：交接链炸了怎么定位

boot 阶段还没有完整的中断、调度与 IPC，一旦出错就是一台哑机器。可观测性靠三根支柱撑。

### 6.1 EarlyConsole：内核接手后的第一根路标

`arch_boot` 入口第一时间通过 EarlyConsole 把原始参数指针打出来（`os/kernel/src/lib.rs:arch_boot`），紧接着打"已进入 arch_boot"。为什么第一动作就是打指针？为了判定 boot-shim→arch_boot 的**第一跳交接是否已坏**——连入口地址都是垃圾则问题在进内核之前，地址正常但后续 `validate` 挂了则问题在拷贝/建表环节。EarlyConsole 是个 trait，x86 走串口、aarch64 走 MMIO UART、riscv64 走 SBI console ecall（`os/libs/minix-boot/src/platform.rs:ConsoleDesc`）。配套的是 panic 诊断注册的**时机前移**：`register_panic_diagnostic` 提到 arch_boot 开头，因为此时 EarlyConsole、栈回溯全部就绪，注册没有理由晚于第一个可 panic 的校验。

### 6.2 两条真实取证签名

| 症状 | 根因 | 定位锚点 |
|---|---|---|
| `validate` 读到 ASCII 垃圾 panic，kinfo 指针恰为某调试串地址 | 引用跨 `noreturn` asm 经 caller-saved 寄存器交接被覆写（§4.1） | `os/kernel/src/lib.rs:arch_boot` |
| 刚 `mov cr3` 切到进程页表后下一条取指就 #PF，报错地址落在低物理恒等窗口 | 该窗口未回放进进程页表（§3.5） | `os/libs/minix-boot/src/kernel_info.rs:KernelInfo` |

### 6.3 可复制的定位命令

```bash
# 端到端引导回归：哑机器先看串口是否打出 PASS 标记（run_qemu.sh 取 test-kernel 与 arch，用法见 os/qemu-tests/README.md）
os/qemu-tests/run_qemu.sh <arch> <test-kernel>   # 例 x86_64 + 对应 .efi

# 判断 reclaim guard 是否恒不触发：生产构造点应全传 (0,0)
rg -n "bootstrap_len" os/kernel/src/lib.rs os/libs/minix-boot/src/kernel_info.rs

# 确认跨镜像只传纯数据（§1.3）：blob 定义与 ≤1 页的尺寸断言
rg -n "struct BootHandoff|size_of::<BootHandoff>" os/libs/minix-boot/src/handoff.rs
```

`run_qemu.sh` 的预期输出关键行是串口打出的 PASS 标记；若卡在它之前，比对 `arch_boot` 的 EarlyConsole 路标落在哪一步，即可把故障定位到"交接前状态未满足"的哪一节——这也正是本篇主线：**交接前机器必须变成什么样**一旦没满足，故障会以最难定位的方式在交接刚完成后暴露。
