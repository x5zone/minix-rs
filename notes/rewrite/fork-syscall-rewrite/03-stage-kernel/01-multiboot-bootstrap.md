# 01-multiboot-bootstrap: 从 GRUB 到分页开启

> **分类**: Kernel 硬件发现
> **源码**: `minix3/minix/kernel/arch/i386/pre_init.c`(243行), `pg_utils.c`(317行)
> **说明**: 内核从 GRUB 拿到 multiboot 数据，解析物理内存布局，建立恒等映射，开启分页

---

## 1. 概述

### 1.1 这是内核的第一段代码

内核是操作系统中最先运行的程序。在它之前，只有 GRUB（引导加载器）和 x86 的实模式 BIOS。内核被 GRUB 加载到物理内存的某个位置，此时还没有分页、没有保护模式、没有进程——一切都需要内核自己建立。

`pre_init()` 和 `pg_utils.c` 是内核启动路径的最前端。它们的职责可以浓缩为一句话：

> **从 GRUB 手中接过系统状态，建立初始页表和分页机制，然后把内核启动信息（kinfo_t）交给 kmain()。**

整个过程被执行一次，之后这些代码的大部分就不再需要了（bootstrap 数据在 kmain 之后被释放）。

### 1.2 启动前系统状态

当 GRUB 跳转到内核时，系统处于以下状态：

| 维度 | 状态 |
|------|------|
| **CPU 模式** | 保护模式（Protected Mode），但还没有分页 |
| **寄存器** | EAX = MULTIBOOT_INFO_MAGIC（0x2BADB002），EBX = multiboot_info_t 结构体地址 |
| **物理内存** | GRUB 分配好的区域，但内核只知道地址，不知道大小和布局 |
| **页表** | 不存在——CR0.PG = 0 |
| **堆** | 不存在——没有 malloc，没有全局分配器 |
| **中断** | 不存在——IDT 未建立 |

### 1.3 自举的四个步骤

```
pre_init(magic, ebx)
  │
  ├── 步骤 1: get_parameters(ebx, &kinfo)
  │   ├── 复制 multiboot_info_t 到自家的 kinfo.mbi
  │   ├── 解析启动命令行（multiboot param_buf）
  │   ├── 构建物理内存 map：遍历 GRUB mmap → add_memmap()
  │   └── 切割内核 + boot 模块占用 → cut_memmap()
  │
  ├── 步骤 2: 建立恒等映射
  │   ├── pg_clear() → 页目录清零
  │   ├── pg_identity(&kinfo) → 4MB 大页恒等映射（VA = PA）
  │   └── pg_mapkernel() → 映射内核到高虚拟地址
  │
  ├── 步骤 3: 开启分页
  │   ├── pg_load() → 写 CR3 = 页目录物理地址
  │   └── vm_enable_paging() → CR0.PG=1
  │
  └── 步骤 4: 返回 &kinfo → kmain()
```

### 1.4 两个 C 文件的职责划分

| 文件 | 职责 | 核心函数 |
|------|------|---------|
| `pre_init.c` | 解析 multiboot 数据、构建内存 map、编排启动流程 | `pre_init()`, `get_parameters()`, `overlaps()`, `mb_set_param()` |
| `pg_utils.c` | 页目录/页表操作、分页控制、物理页分配 | `pg_identity()`, `pg_mapkernel()`, `vm_enable_paging()`, `pg_load()`, `pg_alloc_page()`, `pg_map()` |

### 1.5 与 Rust 64 位版本的差异

| 方面 | Minix3 C（x86-32） | minix-rs（x86-64） |
|------|-------------------|-------------------|
| 页表层级 | 2 级（PD + PT） | 4 级（PML4 + PDPT + PD + PT） |
| 页目录条目 | `pagedir[1024]` 固定数组 | PML4/PDPT/PD 都是 512 条目的动态分配 |
| 大页大小 | 4MB（`I386_BIG_PAGE_SIZE`） | 2MB 或 1GB（`PSE` 或 `PSE-1GB`） |
| 地址空间 | 4GB（32 位，实际截断到 `LIMIT 0xFFFFF000`） | 48 位虚拟地址，物理可达 52 位 |
| PGE/PAE | 可选 | 必需（x86-64 要求 PAE） |
| 页表分配 | 静态 `pagetables[6][1024]` | 动态分配（Direct Map 下直接用 `alloc_pfn`） |

---

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 Multiboot 常量

```
MULTIBOOT_INFO_MAGIC      = 0x2BADB002
MULTIBOOT_INFO_HAS_MMAP    ← mbi.mi_flags 位：有完整内存 map
MULTIBOOT_INFO_HAS_CMDLINE ← mbi.mi_flags 位：有启动命令行
MULTIBOOT_INFO_HAS_MEMORY  ← mbi.mi_flags 位：有基本内存大小
MULTIBOOT_INFO_HAS_MODS    ← mbi.mi_flags 位：有启动模块列表
MULTIBOOT_MEMORY_AVAILABLE  = 1
MULTIBOOT_MAX_MODS          ← 最大模块数
MAXMEMMAP                   ← 最大内存区域数
MULTIBOOT_VIDEO_BUFFER      ← 视频帧缓冲区地址
MULTIBOOT_PARAM_BUF_SIZE    ← 启动参数字符串缓冲区大小
```

#### 2.1.2 页表相关常量（arch/i386/include/vm.h）

| 常量 | 值 | 说明 |
|------|------|------|
| `I386_PAGE_SIZE` | 4096 | 页大小（4KB） |
| `I386_BIG_PAGE_SIZE` | 4096 × 1024 = 4MB | 大页大小 |
| `I386_VM_DIR_ENTRIES` | 1024 | 页目录条目数 |
| `I386_VM_PRESENT` | 0x001 | PTE/PDE 存在位 |
| `I386_VM_WRITE` | 0x002 | 可写 |
| `I386_VM_USER` | 0x004 | 用户可访问 |
| `I386_VM_PWT` | 0x008 | Write-Through 缓存 |
| `I386_VM_PCD` | 0x010 | Cache Disable |
| `I386_VM_BIGPAGE` | 0x080 | 4MB 大页 |
| `I386_VM_ADDR_MASK` | 0xFFFFF000 | 物理地址掩码（4KB 对齐） |
| `I386_VM_ADDR_MASK_4MB` | 0xFFC00000 | 物理地址掩码（4MB 对齐） |

#### 2.1.3 CR0/CR4 控制位

| 常量 | 位 | 说明 |
|------|------|------|
| `I386_CR0_PE` | bit 0 | 保护模式（GRUB 已设置） |
| `I386_CR0_WP` | bit 16 | 写保护（内核态也不能写只读页） |
| `I386_CR0_PG` | bit 31 | 分页开关 |
| `I386_CR4_PSE` | bit 4 | 大页支持（4MB/2MB） |
| `I386_CR4_PGE` | bit 7 | Global Page 标志 |

#### 2.1.4 地址宏

```
I386_VM_PDE(v) = v >> 22          ← 虚拟地址 → 页目录索引（高 10 位）
I386_VM_PTE(v) = (v >> 12) & 0x3FF ← 虚拟地址 → 页表索引（中 10 位）
I386_VM_PFA(e) = e & 0xFFFFF000    ← PDE/PTE → 物理地址（低 12 位清零）
```

#### 2.1.5 pg_utils.c 内部常量

```
PG_PAGETABLES = 6                 ← 静态页表池：6 个 4KB 页表（共 24KB）
PG_ALLOCATEME                     ← pg_map() 的占位值——调用 pg_alloc_page()
LIMIT         = 0xFFFFF000        ← 物理内存截断到 4GB（32 位限制）
```

### 2.2 核心数据结构

#### 2.2.1 `kinfo_t` — 内核启动信息

```c
typedef struct kinfo {
    multiboot_info_t        mbi;
    multiboot_module_t      module_list[MULTIBOOT_MAX_MODS];
    multiboot_memory_map_t  memmap[MAXMEMMAP];
    int                     mmap_size;
    phys_bytes              mem_high_phys;
    vir_bytes               user_sp;
    vir_bytes               user_end;
    vir_bytes               vir_kern_start;
    phys_bytes              bootstrap_start;
    phys_bytes              bootstrap_len;
    phys_bytes              kernel_allocated_bytes;
    phys_bytes              kernel_allocated_bytes_dynamic;
    int                     freepde_start;
    struct kmessages        *kmess;
    char                    param_buf[MULTIBOOT_PARAM_BUF_SIZE];
    int                     do_serial_debug;
    int                     serial_debug_baud;
    int                     mods_with_kernel;
    int                     kern_mod;
} kinfo_t;
```

**关键字段说明**：

| 字段 | 来源 | 用途 |
|------|------|------|
| `mmap_size` / `memmap[]` | `get_parameters()` 解析 | 空闲物理内存区域列表——内核和 VM 的分配器都依赖它 |
| `mem_high_phys` | `add_memmap()` 更新 | 可用的最大物理地址 |
| `module_list[]` | GRUB → 拷贝 | 启动进程二进制信息（PM/VM/VFS/RS 等） |
| `bootstrap_start/len` | 链接器符号 | bootstrap 代码的范围，kmain 之后可以释放 |
| `freepde_start` | `pg_mapkernel()` 返回 | 内核映射后第一个空闲 PDE——用户空间从此开始 |

#### 2.2.2 `multiboot_memory_map_t`

```c
struct multiboot_mmap {
    u32_t   mm_size;
    u64_t   mm_base_addr;
    u64_t   mm_length;
    u32_t   mm_type;        // MULTIBOOT_MEMORY_AVAILABLE(=1)
};
```

#### 2.2.3 `multiboot_module_t`

```c
struct multiboot_module {
    u32_t   mmo_start;
    u32_t   mmo_end;
    u32_t   mmo_string;     // 命令行字符串物理地址
    u32_t   mmo_reserved;
};
```

#### 2.2.4 `pagedir[1024]` — 静态页目录

```c
static u32_t pagedir[1024]  __aligned(4096);
```

编译时静态分配的 4KB 页目录。`pg_load()` 通过 `vir2phys(pagedir)` 获取物理地址写入 CR3。

#### 2.2.5 `pagetables[6][1024]` — 静态页表池

```c
#define PG_PAGETABLES 6
static u32_t pagetables[PG_PAGETABLES][1024] __aligned(4096);
```

6 个 4KB 页表，用于 `pg_map()` 的 4KB 粒度映射。6 个用完即 `panic()`。

### 2.3 关键函数分析

#### 2.3.1 `pre_init()` — 内核入口点（pre_init.c:217）

```c
kinfo_t *pre_init(u32_t magic, u32_t ebx)
{
    assert(magic == MULTIBOOT_INFO_MAGIC);
    get_parameters(ebx, &kinfo);
    pg_clear();
    pg_identity(&kinfo);
    kinfo.freepde_start = pg_mapkernel();
    pg_load();
    vm_enable_paging();
    return &kinfo;
}
```

由汇编代码 `cstart` 调用，是内核执行的第一个 C 函数。验证魔术值 → 解析 multiboot → 恒等映射 → 内核映射 → 开分页 → 返回 `&kinfo`。

#### 2.3.2 `get_parameters()` — 解析 GRUB 数据（pre_init.c:94）

步骤：拷贝 mbi → 初始化 kinfo 字段 → 解析命令行 → 构建物理内存 map (`add_memmap`×N) → 拷贝模块列表 → 检查重叠 (`overlaps`) → 切除占用 (`cut_memmap`)

#### 2.3.3 `add_memmap()` — 添加可用内存区域（pg_utils.c:86）

硬截断到 4GB（`LIMIT 0xFFFFF000`）→ 4KB 对齐 → 填入 `cbi->memmap[]` → 更新 `mem_high_phys`。

#### 2.3.4 `cut_memmap()` — 切除已占用区域（pg_utils.c:32）

将 `[start, end)` 从 memmap 中切除，余量通过 `add_memmap()` 写回。

#### 2.3.4b `alloc_lowest()` — 分配最低物理页（pg_utils.c:65）

遍历 memmap 找到满足 `len` 大小的最低基址区域，调用 `cut_memmap()` 切除并返回。用于 `pre_init` 中分配内核栈等需要低地址的物理内存。与 `pg_alloc_page()`（从尾部取）互补——一个取最低，一个取最高。

#### 2.3.5 `pg_identity()` — 恒等映射（pg_utils.c:162）

1024 × 4MB 大页恒等映射。超出 `mem_high_phys` 的 PDE 加 `PWT | PCD`（禁用缓存）。

#### 2.3.6 `pg_mapkernel()` — 内核高地址映射（pg_utils.c:186）

4MB 大页映射内核到高虚拟地址。返回第一个空闲 PDE 号。

#### 2.3.7 `vm_enable_paging()` — 开启分页（pg_utils.c:204）

执行：清 PG+PGE → 开 PSE → 开 PG → 开 WP → 开 PGE。

#### 2.3.8 `pg_load()` / `pg_alloc_page()` / `pg_map()`

`pg_load()` 写 CR3；`pg_alloc_page()` 从 memmap 尾部取 4KB；`pg_map()` 4KB 粒度映射。

### 2.4 调用关系

```
cstart (assembly)
  └── pre_init(magic, ebx)
        ├── get_parameters(ebx, &kinfo)
        │     ├── memcpy(mbi, ebx)
        │     ├── add_memmap() × N → cut_memmap() × mod_count
        │     └── overlaps() × mod_count
        ├── pg_clear()
        ├── pg_identity(&kinfo)     ← 4MB 大页恒等映射
        ├── pg_mapkernel()          ← 内核高地址映射
        ├── pg_load() → write_cr3()
        └── vm_enable_paging()      ← CR0.PG=1
        → return &kinfo
kmain(cbi)
  └── pg_map() / pg_alloc_page()   ← kmain 阶段继续使用
```

### 2.5 设计要点

**为什么不用 malloc**：pre_init 阶段所有数据结构编译时静态分配（`kinfo` BSS、`pagedir[1024]` 静态数组、`pagetables[6][1024]` 池）。

**为什么先恒等映射再映射内核**：开分页后 CPU 立即使用页表。不开恒等映射，正在执行的代码会页错误。

**为什么 4GB 截断**：32 位 x86 只能寻址 4GB。`LIMIT 0xFFFFF000` 是物理限制。64 位 Rust 必须删除。

**为什么物理内存从尾部取**：减少大块连续区域前端的碎片化。

---

## 3. Rust 设计决策

> 本章解释从 Ch1&2 的 C 源码到 Rust 设计的每一个关键选择。每个决策给出：为什么选这条路径、替代方案有哪些、为什么否决替代方案。

### 3.1 UEFI 替代 Multiboot（架构演进）

**C 源码依据**：§2.3.1 `pre_init(magic, ebx)` — Multiboot 是由 GRUB 传递的 32 位引导协议。

**决策**：minix-rs 使用 UEFI 作为引导协议，替代 Minix3 的 Multiboot+GRUB。

**理由**：

| 维度 | Multiboot (C) | UEFI (Rust) |
|------|--------------|-------------|
| 架构支持 | x86-32 专用 | x86-64、aarch64、riscv64 全部支持 |
| 内存发现 | 遍历 `multiboot_memory_map_t` 手动解析 | `GetMemoryMap()` 直接返回 |
| 内核模块 | `memcpy(module_list, ebx)` 手动拷贝 | `ImageHandle` protocol 标准接口 |
| QEMU 支持 | `-kernel` 参数（有限） | OVMF / AA64 UEFI / RISC-V UEFI |
| 后续扩展 | 32 位地址截断到 4GB | 64 位原生，不截断 |

Multiboot 是 1995 年的规范，只考虑了 x86-32。三种 64 位架构中 UEFI 是唯一跨架构一致的引导协议。

**替代方案 & 否决理由**：

| 方案 | 否决原因 |
|------|---------|
| 保留 Multiboot + 64 位扩展 | ARM/RISC-V 无等效协议 |
| 各架构独立引导（x86 Multiboot + ARM PSCI + RISC-V SBI） | 三种引导路径 = 过度复杂 |
| Linux 风格（UEFI stub 内嵌 kernel） | 调试不便分离更新 |

### 3.2 boot-shim = 独立 crate `boot-uefi`

**C 源码依据**：§2.4 `pre_init()` → `kmain()` — C 版本 bootstrap 在 kmain 后被释放。

**决策**：UEFI 引导逻辑放在独立 crate，kernel 不依赖 UEFI 类型。

```
boot-uefi crate                     kernel crate
     │                                  │
     │ UEFI GetMemoryMap()              │ 纯裸机，无 UEFI
     │ UEFI ExitBootServices()         │
     │ 构建 KernelInfo                  │
     │                                  │
     └─→ 跳转 kernel ────────────→ arch_boot(kernel_info, paging) → kmain()
```

**移植性**：换非 UEFI 板 → 换 `boot-uefi` 为 `boot-coreboot`。kernel 一行不改。

**替代方案 & 否决理由**：kernel 内部模块 → kernel 永远依赖 uefi crate；条件编译 `#[cfg(uefi)]` → 违反 trait 静态分派原则。

### 3.3 Paging trait 统一 —— 启动和运行时用同一个 trait

**C 源码依据**：§2.3.5 `pg_identity()`、§2.3.6 `pg_mapkernel()`、§2.3.7 `vm_enable_paging()` — C 版本无统一抽象。

**决策**：boot 阶段的页表操作整合进已有的 `Paging` + `HugePages` trait，不定义新的 `PageTableBoot` trait。大页参数（`HUGE_PAGE_SIZE`）从 `HugePages: Paging` 扩展 trait 获取，而非新增到 `Paging` 本身。

```rust
// C 的行为                    // Paging + HugePages trait 方法
pg_identity()                  → paging.map_huge(0, 0, HUGE_SIZE, W | X) × N
pg_mapkernel()                 → paging.map_huge(kern_virt, kern_phys, HUGE_SIZE, W | X)
pg_load() + vm_enable_paging()→ paging.enable()
alloc_pagetable()              → 不在 trait 中（boot-shim 从 UEFI 分配 root page）
```

**trait 扩展**（boot 阶段新增 `new_empty` + `enable` 到 `Paging`；大页参数从 `HugePages` 获取）：

```rust
// paging.rs — Paging trait 新增 boot 方法
pub trait Paging {
    // 已有方法
    const PAGE_SIZE: usize;
    fn new() -> Result<Self, PageTableError> where Self: Sized;
    fn map(&mut self, vaddr, paddr, flags) -> Result<(), PageTableError>;
    fn unmap(&mut self, vaddr) -> Result<PhysBytes, PageTableError>;
    fn query(&self, vaddr) -> Option<(PhysBytes, PageFlags)>;

    // 新增 boot 阶段方法
    /// Create from a known physical page (during boot, no global allocator).
    /// C 对应: alloc_pagetable() — pg_utils.c:123
    fn new_empty(root_page: PhysBytes) -> Self;

    /// Load root table and enable MMU.
    /// C 对应: pg_load() + vm_enable_paging() — pg_utils.c:204,247
    /// # Safety
    /// Caller must have set up identity mapping covering current RIP.
    unsafe fn enable(&self) -> PhysBytes;
}

// paging_ext.rs — HugePages trait（已有，定义在 02-stage-vm/06-pagetable-struct.md §3.4）
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [usize];
    const HUGE_PAGE_SIZE: u64;           // Direct Map 首选大页大小
    const HUGE_PAGE_SHIFT: u32;
    const FALLBACK_HUGE_PAGE_SIZE: u64;  // 首选不可用时的回退
    const PTE_HUGE_FLAGS: u64;

    fn map_huge(&mut self, vaddr: VirBytes, paddr: PhysBytes,
                size: usize, flags: PageFlags) -> Result<(), PageTableError>;
    fn supports_huge_page(size: usize) -> bool { ... }
    fn supports_1gb_page() -> bool { ... }
}
```

**为什么 `HUGE_PAGE_SIZE` 放在 `HugePages` 而非 `Paging`**：
- `HugePages` 是 `Paging` 的超集 trait（`HugePages: Paging`），boot 阶段用 `P: HugePages` bound 即可同时访问两者
- 所有目标架构（x86-64/ARM64/RISC-V）都实现了 `HugePages`，boot 阶段大页是必须的（C 源码 `pg_identity()` 和 `pg_mapkernel()` 都用 4MB 大页）
- `map_huge()`、`supports_1gb_page()`、`PTE_HUGE_FLAGS` 等高级操作保留在扩展 trait，保持 `Paging` 核心职责清晰
- 详见 02-stage-vm/06-pagetable-struct.md §3.4 和 §5.3.1 的 trait 职责划分

**为什么这是好的**：
- ≥3 个行为不同的实现（x86-64 PML4、aarch64 TTBR1、riscv64 Sv39）→ ✅ 多态
- 调用者 `arch_boot<P: HugePages>()` 使用 trait bound → ✅
- 描述机制（映射页面、开启 MMU）而非策略 → ✅

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| `PageTableBoot` trait | 与 `Paging::map/query` 重复，同一硬件两个 trait 说不通 |
| `HUGE_PAGE_SIZE` 移入 `Paging` | 违反 02-stage-vm 的 trait 职责划分（大页是可选能力，非核心 Paging）；且 `HugePages` trait 已存在，重复定义矛盾 |

### 3.4 KernelInfo 字段精简

**C 源码依据**：§2.2.1 `kinfo_t` — 20+ 字段。

**决策**：Rust 只保留 7 字段。

| 删除的 C 字段 | 理由 |
|--------------|------|
| `mbi` | UEFI 不需要 raw multiboot |
| `bootstrap_start/len` | UEFI 不区分 |
| `kernel_allocated_bytes` | 根据 memmap 动态计算 |
| `do_serial_debug/serial_debug_baud` | 独立 debug 模块 |
| `param_buf[]` | UEFI LoadOption |
| `mmap_size` | `&[MemoryRegion]` 自带长度 |
| `mem_high_phys` | memmap 最后 region 的 end |

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 保留全部字段（1:1 翻译 `kinfo_t`） | UEFI 引导路径不产生 `mbi`/`param_buf` 等字段，强行保留需要 `Option<>` 包装，增加无意义复杂度 |
| 保留 `mem_high_phys` 单独字段 | 可从 `memmap.last().end` 推导，冗余字段违反单一数据源原则 |

`free_upper_idx` 统一起名（x86-64 = PML4 索引，aarch64 = TTBR1 L0 索引，riscv64 = VPN[2] 上界）。

**`overlaps()` 的 Rust 等价**：C 源码 `overlaps()`（pre_init.c:77）检查 boot 模块是否与内核镜像重叠。UEFI 引导路径中，boot-uefi crate 通过 `GetMemoryMap()` 获取的内存描述已由 UEFI 固件保证不重叠，因此不需要 Rust 等价函数。如果未来支持非 UEFI 引导（如 coreboot），需在对应 boot-shim 中实现重叠检测。

### 3.5 4GB 截断删除（架构演进）

**C 源码依据**：§2.3.3 `add_memmap()` — `LIMIT 0xFFFFF000`。

**决策**：Rust 删除 `LIMIT`。64 位有 48 位虚拟 + 52 位物理地址，不需要截断。

### 3.6 分散定义，集中实现

`Paging` trait 定义在 `os/arch/src/paging.rs`（靠近使用方），各架构实现在 `arch/x86_64/`、`arch/aarch64/`、`arch/riscv64/` 集中。kernel 只依赖 trait，不引用具体类型。

### 3.7 QEMU 集成测试结构

Mock 可测的用 `#[cfg(test)]` 单元测试。真硬件的用 `qemu-tests/`，每个集成测试编译为独立 `.efi`，串口输出 PASS/FAIL。

### 3.8 三层 crate 依赖

```
boot-uefi → minix-types, minix-arch  (依赖 uefi crate，不依赖 kernel)
kernel    → minix-types, minix-arch  (no uefi dependency)
minix-arch → minix-types             (定义 Paging trait)
```

kernel 不依赖 boot-uefi。KernelInfo 等共享类型下沉到 `minix-types`，两个 crate 互不依赖。

---

## 4. 实现详解

> 每个结构的引导语解释核心思路，代码注释标注 C 源码对应。
> 实际代码路径见各节标注。当前代码可通过 `cargo test -p minix-kernel --features mock` 验证。

### 4.1 KernelInfo — C 的 kinfo_t 对应

> 设计决策：§3.4, §3.5
> 实际文件：`os/libs/minix-types/src/kernel_info.rs` (48行)

```rust
// minix-types crate 中定义 — boot-uefi 和 kernel 共享
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: usize,
    pub free_upper_idx: usize,
    pub user_sp: VirBytes,
    pub boot_modules: &'static [BootModule],
}

pub struct MemoryRegion {
    pub base: PhysBytes,
    pub len: usize,
}

pub struct BootModule {
    pub name: &'static str,
    pub start: PhysBytes,
    pub len: usize,
}
```

### 4.2 Paging trait 扩展 + HugePages trait

> 设计决策：§3.3
> 实际文件：`os/arch/src/paging.rs`（`new_empty` + `enable` 新增方法）、`os/arch/src/paging_ext.rs`（`HugePages` trait，已有定义）

在 `os/arch/src/paging.rs` 的 `Paging` trait 上新增 `fn new_empty(root_page: PhysBytes) -> Self;`、`unsafe fn enable(&self) -> PhysBytes;`。大页参数（`HUGE_PAGE_SIZE`、`map_huge()` 等）从 `paging_ext::HugePages: Paging` 扩展 trait 获取，boot 阶段通过 `P: HugePages` bound 同时访问两者。文档注释标注各架构汇编差异（x86-64 `mov cr3`、aarch64 `msr TTBR1_EL1`、riscv64 `csrw satp`）。

### 4.3 x86-64 架构实现

> ⚠️ 以下 `enable()` 为伪代码，当前实际实现为 `todo!()`。汇编序列描述了预期行为。

```rust
impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

    fn new_empty(root_page: PhysBytes) -> Self {
        // 当前实现: todo!()
        // 预期行为: 清零 PML4 页，记录物理/虚拟地址
        Self { pml4_phys: root_page }
    }

    unsafe fn enable(&self) -> PhysBytes {
        // 伪代码 — 当前实际实现为 todo!()
        // 预期汇编序列:
        //   mov cr3, pml4_phys          ← 加载页表根
        //   mov rax, cr0; or rax, PG   ← CR0.PG=1 开分页
        //   mov cr0, rax
        //   mov rax, cr0; or rax, WP   ← CR0.WP=1 写保护
        //   mov cr0, rax
        self.pml4_phys
    }
    // map/unmap/query/new — 已有实现
}

impl HugePages for X86_64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21]; // 1GB, 2MB
    const HUGE_PAGE_SIZE: u64 = 1 << 30;           // 首选 1GB
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;  // 回退 2MB
    const PTE_HUGE_FLAGS: u64 = 1 << 7;            // PS bit

    fn map_huge(&mut self, vaddr: VirBytes, paddr: PhysBytes,
                size: usize, flags: PageFlags) -> Result<(), PageTableError> {
        // 当前实现: todo!()
        todo!()
    }
}
```

### 4.4 boot-uefi crate

```rust
// os/boot-uefi/src/main.rs
#[entry]
fn uefi_main(_image: Handle, system_table: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&system_table).unwrap();

    let mmap = system_table.boot_services().memory_map(
        uefi::table::boot::MemoryType::LOADER_DATA).unwrap();

    let kernel_image = locate_kernel(&system_table);

    let root_page = system_table.boot_services()
        .allocate_pages(uefi::table::boot::AllocateType::AnyPages,
                        uefi::table::boot::MemoryType::LOADER_DATA, 1)
        .unwrap();

    let kernel_info = KernelInfo {
        memmap: build_memmap(&mmap, kernel_image),
        kern_virt_base: VirBytes(kernel_image.virt_base),
        kern_phys_base: PhysBytes(kernel_image.phys_base),
        kern_size: kernel_image.size,
        free_upper_idx: 0,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        boot_modules: &[],
    };

    let (_rt, _mmap) = system_table.exit_boot_services(
        uefi::table::boot::MemoryType::LOADER_DATA);

    arch_boot(&kernel_info, root_page);
    unreachable!()
}
```

### 4.5 kernel 入口 — main.rs

```rust
// os/kernel/src/main.rs
#[cfg(target_arch = "x86_64")]
fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    arch_boot_impl::<X86_64Paging>(kernel_info, root_page)
}
// 同模式 aarch64 → Aarch64Paging, riscv64 → Riscv64Paging

fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    let mut paging = P::new_empty(root_page);
    let huge_size = P::HUGE_PAGE_SIZE as usize;

    // Step 1: 恒等映射 (C: pg_identity — pg_utils.c:162)
    for region in kernel_info.memmap {
        let mut addr = region.base.0;
        let end = addr + region.len;
        while addr < end {
            paging.map_huge(VirBytes(addr), PhysBytes(addr),
                            huge_size, PageFlags::read_write()).unwrap();
            addr += huge_size;
        }
    }

    // Step 2: 内核高地址映射 (C: pg_mapkernel — pg_utils.c:186)
    let mut offset = 0;
    while offset < kernel_info.kern_size {
        paging.map_huge(
            VirBytes(kernel_info.kern_virt_base.0 + offset),
            PhysBytes(kernel_info.kern_phys_base.0 + offset),
            huge_size, PageFlags::kernel_read_write()).unwrap();
        offset += huge_size;
    }

    // Step 3: 开分页 (C: pg_load + vm_enable_paging)
    unsafe { paging.enable() };

    kmain(kernel_info)
}
```

> `#[cfg(target_arch)]` 在入口中是**编译边界选择编译单元**，不是"OS 逻辑中选硬件"。上层代码只依赖 trait。
>
> `P: HugePages` bound 替代了原来的 `P: Paging`。因为 `HugePages: Paging`，boot 阶段自动获得 `Paging` 的所有方法（`new_empty`、`enable`、`map` 等），同时可直接访问 `P::HUGE_PAGE_SIZE` 和 `P::map_huge()`。

> **与 C 源码的差异**：C 的 `pg_mapkernel()` 只设 `PRESENT | BIGPAGE | WRITE`，无 GLOBAL 位。Rust 代码中 `PageFlags::kernel_read_write()` 是否含 GLOBAL 取决于 `PageFlags` 定义——如果含 GLOBAL，则是 64 位架构的合理增强（x86-64 内核映射加 GLOBAL 可避免 TLB 刷新），但需明确标注为架构演进而非 1:1 对齐。

### 4.6 与 Minix3 C 的函数对照

| C 函数 (Ch2) | Rust 实现 | 位置 |
|-------------|----------|------|
| `pre_init(magic, ebx)` | 不需要 — UEFI 替代 | — |
| `get_parameters(ebx)` | `uefi_main()` GetMemoryMap | `boot-uefi/src/main.rs` |
| `add_memmap()` | 不需要 — UEFI 直接返回 | — |
| `cut_memmap()` | 不需要 — UEFI 已扣减 | — |
| `overlaps()` | 不需要 — UEFI 固件保证不重叠 | — |
| `alloc_lowest()` | 不需要 — UEFI 分配器替代 | — |
| `pg_clear()` | `Paging::new_empty(root_page)` | `arch/x86_64/paging.rs` |
| `pg_identity(&kinfo)` | `arch_boot_impl` Step 1 (`map_huge`) | `kernel/src/main.rs` |
| `pg_mapkernel()` | `arch_boot_impl` Step 2 (`map_huge`) | `kernel/src/main.rs` |
| `pg_load()`+`vm_enable_paging()` | `Paging::enable()` | `arch/x86_64/paging.rs` |
| `alloc_pagetable()` | 不需要 — boot-uefi 分配 | — |

---

## 5. 测试要点

### 5.1 单元测试（mock，`#[cfg(test)]`）

- `MemoryRegion` 的重叠检测、对齐、转换为 PFN 范围
- `KernelInfo` 构造器从 mock UEFI mmap 构建
- `Paging` mock 实现的 `new_empty()` + `enable()` 不 panic

### 5.2 集成测试（qemu-tests/）

**test_memmap**: 启动 → 读 memmap → 断言 kernel 物理地址范围内有条目
**test_paging_enable**: 启动 → 开启分页 → 串口输出 `PASS`（证明分页开启后代码未崩溃）
**test_kernel_map**: 启动 → 开分页 → 访问内核高地址变量 → 断言值正确
**test_arch_regression**: CI 中三种架构各自跑 test_paging_enable

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) — Kernel 整体架构概览
- [02-page-table-kernel.md](02-page-table-kernel.md) — 内核页表操作（memory.c 核心）
- `os/arch/src/paging.rs` — Paging trait 定义 + PageFlags
- `os/arch/src/x86_64/paging.rs` — x86-64 Paging 实现

---

*分类: Kernel 硬件发现*