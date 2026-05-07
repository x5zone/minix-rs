# 06-pagetable-struct: 页表结构

> **分类**: VM库  
> **源码**: [pt.h](minix3/minix/servers/vm/pt.h)  
> **说明**: 定义页表数据结构，可被其他需要地址空间管理的服务使用

> **术语约定**: 为避免混淆，本文档使用数字表示页表层级：
> - **P0**：页目录（Page Directory），x86-32 的顶层
> - **P1**：页表（Page Table），x86-32 的第二层

> ⚠️ **设计目标标注**: 标注 `【设计目标】` 的代码描述架构设计方向，
> 供后续架构实现参考。

---

## 1. 概述

### 1.1 页表的作用

页表（Page Table）是操作系统实现虚拟内存的核心数据结构，负责将虚拟地址（Virtual Address）映射到物理地址（Physical Address）。每个进程拥有独立的页表，实现进程间的地址空间隔离。

**核心功能**:
- **地址转换**: 将虚拟地址转换为物理地址
- **内存保护**: 通过权限位控制读写执行权限
- **共享内存**: 多个虚拟地址映射到同一物理页
- **写时复制 (CoW)**: 通过只读标记实现高效的进程 fork

### 1.2 x86 两级页表结构

Minix3 在 x86 架构上使用经典的**两级页表**结构：

```
虚拟地址 (32-bit)
┌──────────┬──────────┬────────┐
│  页目录   │  页表    │ 页内偏移│
│  10 bits │  10 bits │ 12 bits│
└──────────┴──────────┴────────┘
     │           │          │
     ▼           ▼          ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│ 页目录    │  │ 页表     │  │ 物理页    │
│ (PDE)    │→ │ (PTE)    │→ │ 4KB      │
│ 1024项   │  │ 1024项   │  │          │
└──────────┘  └──────────┘  └──────────┘
     CR3          │              │
                  │              │
                  └──────────────┘
                         +
                    页内偏移 (0-4095)
```

**地址转换过程**:
1. CPU 从 CR3 寄存器获取页目录物理地址
2. 用虚拟地址的高10位索引页目录，得到页表物理地址
3. 用虚拟地址的中10位索引页表，得到物理页框地址
4. 用虚拟地址的低12位作为页内偏移，得到最终物理地址

---

## 2. C 源码分析

### 2.1 页表结构体 pt_t

**Minix3 定义** (`minix3/minix/servers/vm/pt.h`):

```c
typedef struct {
    u32_t *pt_dir;                      // 页目录虚拟地址（VM地址空间内）
    u32_t pt_dir_phys;                  // 页目录物理地址（用于加载CR3）
    u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];  // 页表虚拟地址数组
    u32_t pt_virtop;                    // 虚拟地址分配提示
} pt_t;
```

其中 `ARCH_VM_DIR_ENTRIES` 定义在 `minix3/minix/servers/vm/arch/i386/pagetable.h`：

```c
#define ARCH_VM_DIR_ENTRIES  I386_VM_DIR_ENTRIES  // 即 1024
```

**字段详解**:

#### `pt_dir` — 页目录虚拟地址

指向页目录（P0）的虚拟地址，页目录包含 1024 个 PDE，每个 PDE 存储一个页表（P1）的物理地址。

- **字段值**：虚拟地址，供 VM 代码读写页目录（管理用途）
- **指向内容**：页目录本身，CPU 通过 CR3 加载其物理地址进行硬件地址转换

```c
// pagetable.c:pt_new ([pagetable.c:990](minix3/minix/servers/vm/pagetable.c#L990))
pt->pt_dir = vm_allocpages((phys_bytes *)&pt->pt_dir_phys,
    VMP_PAGEDIR, ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE);
```

内存来源由 `vm_allocpages()` 根据全局变量 `pt_init_done` 决定（[pagetable.c:328](minix3/minix/servers/vm/pagetable.c#L328)）：
- **init 阶段**（`pt_init_done == 0`）：来自静态 BSS `static_sparepages[]`/`static_sparepagedirs[]`
- **normal 阶段**（`pt_init_done == 1`）：来自 `alloc_mem()` 物理分配器

#### `pt_dir_phys` — 页目录物理地址

页目录的物理地址，用于加载到 CR3 寄存器。与 `pt_dir` 指向同一物理页，构成虚实对应关系。

`pt_new()` 调用 `vm_allocpages()` 时通过输出参数同时获得虚拟地址和物理地址。

```c
// pagetable.c:pt_new
vm_allocpages((phys_bytes *)&pt->pt_dir_phys, ...);  // 输出物理地址
```

fork 时通过 `pt_bind()` 传递给内核：

```c
// pagetable.c:pt_bind ([pagetable.c:1358](minix3/minix/servers/vm/pagetable.c#L1358))
return sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys, pdes);
```

`pt_bind()` 将页目录物理地址登记到 `pagedir_mappings`，并通过 `sys_vmctl_set_addrspace()` 通知内核更新进程的 CR3 寄存器，激活该进程的页表。

#### `pt_pt[]` — 页表虚拟地址缓存

长度为 1024 的指针数组，`pt_pt[pde]` 存储第 `pde` 个页表（P1）的虚拟地址。与 `pt_dir[pde]` 指向同一个物理页，但 `pt_dir[pde]` 存物理地址（给 CPU 用），`pt_pt[pde]` 存虚拟地址（给 VM 用）。

```c
// pagetable.c:pt_ptalloc ([pagetable.c:494](minix3/minix/servers/vm/pagetable.c#L494))
p = vm_allocpage(&pt_phys, VMP_PAGETABLE);  // p=虚拟地址, pt_phys=物理地址
pt->pt_pt[pde] = p;                          // 缓存虚拟地址
pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags  // 存入页目录（物理地址）
    | ARCH_VM_PDE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW;
```

页表按需分配，初始时 `pt_pt[pde]` 为 NULL，首次映射该范围时才分配。PDE 的 flags 由调用者传入的 `flags` 参数与固定的 `PRESENT|USER|RW` 组合而成。

#### `pt_virtop` — 虚拟地址分配提示（当前未使用）

设计意图是作为查找空闲虚拟地址空间的起始位置提示。但实际代码中存在两个问题：

1. `findhole()` 使用自己的静态变量 `lastv`，而非此字段
2. `findhole()` 只操作 `vmprocess->vm_pt`（VM 自身的页表），不处理用户进程页表

```c
// pagetable.c:pt_new ([pagetable.c:1019](minix3/minix/servers/vm/pagetable.c#L1019))
pt->pt_virtop = 0;  // 初始化为 0，但从未被读取

// pagetable.c:findhole ([pagetable.c:155](minix3/minix/servers/vm/pagetable.c#L155)) - 只给 VM 自己用
static u32_t findhole(int pages)
{
    static void *lastv = 0;  // 静态变量，VM 单例进程使用
    pt_t *pt = &vmprocess->vm_pt;  // VM 自己的页表
    vmin = VM_OWN_MMAPBASE;  // VM 地址空间中的查找范围
    vmax = VM_OWN_MMAPTOP;
    // ...
}
```

**结论**：`pt_virtop` 在当前 Minix3 实现中未实际使用。每个进程的 `pt_t` 都有此字段，但 `findhole()` 只处理 VM 自身的地址空间，且使用静态变量 `lastv` 而非此字段。用户进程的虚拟地址分配由 region 机制管理，不使用此字段。

### 2.2 页目录项 (PDE)

> **注意**：以下为 x86-32 的 PDE/PTE 格式，x86-64 使用 64-bit PTE，位定义完全不同。此处仅作为 Minix3 原版设计的参考。

PDE 和 PTE 共享相同的 32-bit 格式：高 20 位存储**页框物理地址**，低 12 位存储属性标志。注意这是条目中存储的物理地址，与 CPU 用虚拟地址索引页表的查找过程是两回事。

```
┌─────────────────────┬───────────────┐
│   页框物理地址       │   属性标志    │
│     20 bits         │    12 bits    │
│   [31:12]           │    [11:0]     │
└─────────────────────┴───────────────┘
```

物理地址 4KB 对齐，低 12 位全为 0，因此只需存储高 20 位。

**PDE 标志位**:

| 位 | 标志 | 值 | 说明 |
|----|------|-----|------|
| 0 | `I386_VM_PRESENT` | 0x001 | 页表存在，可访问 |
| 1 | `I386_VM_WRITE` | 0x002 | 页表可读写（否则只读） |
| 2 | `I386_VM_USER` | 0x004 | 用户模式可访问 |
| 3 | `I386_VM_PWT` | 0x008 | 写穿透缓存 |
| 4 | `I386_VM_PCD` | 0x010 | 禁用缓存 |
| 5 | `I386_VM_ACC` | 0x020 | 已访问（硬件设置） |
| 7 | `I386_VM_BIGPAGE` | 0x080 | 4MB大页（PDE 专属，跳过 P1） |

**PDE 值构造**:
```c
// pagetable.c:pt_ptalloc (i386)
pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK /* 0xFFFFF000 */) | flags
    | ARCH_VM_PDE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW;
```

### 2.3 页表项 (PTE)

格式与 PDE 相同（高 20 位物理地址 + 低 12 位标志），但标志位含义不同。

**PTE 标志位**:

| 位 | 标志 | 值 | 说明 |
|----|------|-----|------|
| 0 | `I386_VM_PRESENT` | 0x001 | 物理页存在 |
| 1 | `I386_VM_WRITE` | 0x002 | 页面可写 |
| 1 | `I386_VM_READ` | 0x000 | 页面只读（值为0，与WRITE互斥） |
| 2 | `I386_VM_USER` | 0x004 | 用户模式可访问 |
| 3 | `I386_VM_PWT` | 0x008 | 写穿透缓存 |
| 4 | `I386_VM_PCD` | 0x010 | 禁用缓存 |
| 5 | `I386_VM_ACC` | 0x020 | 已访问（硬件设置） |
| 6 | `I386_VM_DIRTY` | 0x040 | 已修改（硬件设置，PTE 专属） |
| 7 | `I386_VM_PS` | 0x080 | 页大小（PDE 中为 BIGPAGE） |
| 8 | `I386_VM_GLOBAL` | 0x100 | 全局页（TLB 不刷新） |
| 9-11 | `I386_VM_PTAVAIL1-3` | 0xE00 | 软件可用位 |

**物理地址提取**:
```c
phys_addr = pte & I386_VM_ADDR_MASK;  /* 0xFFFFF000 */
```

### 2.4 x86 特定定义

**Minix3 常量** (`minix3/minix/include/arch/i386/include/vm.h`):

```c
#define I386_PAGE_SIZE          4096        // 4KB 页大小
#define I386_VM_DIR_ENTRIES     1024        // 页目录项数
#define I386_VM_PT_ENTRIES      1024        // 页表项数
#define I386_VM_DIR_ENT_SHIFT   22          // 页目录索引位移
#define I386_VM_PT_ENT_SHIFT    12          // 页表索引位移
#define I386_VM_PT_ENT_MASK     0x3FF       // 页表索引掩码
#define I386_VM_ADDR_MASK       0xFFFFF000  // 物理地址掩码
#define I386_VM_PFA_SHIFT       22          // 页框地址位移
```

**地址转换宏**:

```c
// 从虚拟地址提取页目录索引
#define I386_VM_PDE(v)  ((v) >> I386_VM_DIR_ENT_SHIFT)

// 从虚拟地址提取页表索引
#define I386_VM_PTE(v)  (((v) >> I386_VM_PT_ENT_SHIFT) & I386_VM_PT_ENT_MASK)

// 从页表项提取物理地址
#define I386_VM_PFA(e)  ((e) & I386_VM_ADDR_MASK)

// 从虚拟地址提取页框号
#define I386_VM_PAGE(v) ((v) >> I386_VM_PFA_SHIFT)
```

---

## 3. Rust 设计决策

### 3.0 问题与建模

Rust 版本面向现代 64 位硬件（x86-64、arm64），需要为页表设计合适的抽象。

**Minix3 的做法**：
  - `pt_t` 将页目录指针、物理地址、页表缓存等硬件细节直接编码进结构体，与 x86-32 紧耦合；支持 arm32 则依赖 `#if defined()` 条件编译，每增加一种架构便引入更多分支
  - 在 64 位架构下，x86-64 采用四级页表，arm64 采用另一套层级和粒度的描述符，无法以单一结构体统一表达；继续沿用"结构体 + 条件编译"的方式，复杂度将进一步上升

问题在于建模方式：OS 需要的是**机制**（映射地址、设置权限、切换地址空间），而非**硬件细节**（第几个 PDE 指向哪个页表）。正确的建模应当抽象机制，而非描述硬件——描述"做什么"，而非"怎么做"。

**Rust 版本的做法**：
  - 抽象机制，定义 `Paging` trait 作为页表操作的规范接口
  - 各架构自行实现具体结构体，通过静态分派绑定，零运行时开销
  - OS 层仅依赖 trait 抽象，不感知底层结构体布局
  - 可移植性：新增架构时，依据 trait 规范实现对应逻辑即可

此外，面向未来还需为 TLB 进程标识（x86-64 的 PCID / arm64 的 ASID）预留支持空间。该机制为 TLB 条目标记进程标识，避免上下文切换时刷新整个 TLB，从而显著减少 TLB miss 开销。

### 3.1 Paging Trait 设计

> **定义位置**: `minix_arch::paging`

**设计原则**: 抽象机制，不暴露硬件细节。

> **关于 `pt_virtop`**: Minix3 的 `pt_virtop` 字段经分析为冗余（仅 VM 自身使用，
> 且 `findhole()` 等函数使用静态变量替代），Rust 版本不包含此字段。

```rust
/// 页表管理 trait
///
/// 定义页表的核心操作，各架构需要实现此 trait。
///
/// **设计选择**：当前 trait 采用"扁平映射"语义——`map()` 在调用者看来
/// 是单步操作，中间页表（PDPT/PD/PT 等）的按需分配由实现内部处理，
/// 不暴露给调用者。这简化了 VM 层的使用，但意味着调用者无法直接控制
/// 中间层条目。若未来需要 THP split/merge、migration entry 等精细控制，
/// 可扩展此 trait 或引入新的 `PagingLevel` trait。
pub trait Paging {
    /// 页大小
    const PAGE_SIZE: usize;

    /// 创建新的页表
    fn new() -> Result<Self, PageTableError> where Self: Sized;

    /// 销毁页表
    unsafe fn destroy(&mut self);

    /// 映射虚拟地址到物理地址
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 原子覆盖映射，对应 Minix3 pt_writemap() + WMF_OVERWRITE
    fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>;

    /// 取消映射虚拟地址
    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;

    /// 更新页标志
    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 查询虚拟地址的映射信息
    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;

    /// 获取页表根物理地址（用于激活页表）
    fn root_paddr(&self) -> PhysBytes;

    /// 激活此页表
    unsafe fn switch(&self);

    /// 刷新与本页表相关的 TLB 条目
    ///
    /// 不使用 ASID 时等价于全局刷新；使用 PCID/ASID 时仅刷新当前地址空间的条目。
    /// 若需跨地址空间的全局刷新，应由上层 VM 通过特定接口完成。
    unsafe fn flush_tlb(&self);

    /// 刷新指定虚拟地址的 TLB 条目
    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes);

    /// 批量映射连续页面（默认方法，arch 实现可覆盖）
    ///
    /// 默认实现逐页调用 map()。arch 实现可覆盖以利用硬件优化：
    /// - x86-64：批量映射后单次 CR3 reload 替代多次 INVLPG
    /// - ARM64：利用 TLBI range 指令
    ///
    /// 对应 Minix3 pt_writemap() 的批量映射语义。
    fn map_range(
        &mut self,
        vaddr_start: VirBytes,
        paddr_start: PhysBytes,
        pages: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    /// 批量取消映射（默认方法，arch 实现可覆盖）
    ///
    /// 与 map_range 对称，默认实现逐页调用 unmap()。
    fn unmap_range(
        &mut self,
        vaddr_start: VirBytes,
        pages: usize,
    ) -> Result<(), PageTableError>;

    // REMOVED: check_range
    //
    // Minix3 原版 pt_checkrange() 在全源码中仅有一处调用，且被 #if SANITYCHECKS
    // 包裹（region.c:746-751，在 map_pf() 中），属于 debug-only 断言而非生产 API。
    // 该函数无硬件优化空间（仅是 query() 的循环），VM 层需要时可自行循环 query()
    // 实现。因此不纳入 Paging trait，保持 trait 只包含硬件必须提供语义的操作。
}
```

**关键设计决策**:
1. **机制抽象**: 只定义操作，不定义内部结构
2. **架构无关**: 不包含 PDE/PTE 等硬件特定概念
3. **类型安全**: 使用 `VirBytes`、`PhysBytes`、`PageFlags` 类型

**错误类型**:

> **定义位置**: `minix_arch::paging`，6 个变体。

```rust
/// 页表错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    /// 地址未页对齐或超出有效范围
    InvalidAddress,
    /// 虚拟地址已存在映射
    AlreadyMapped,
    /// 虚拟地址不存在映射
    NotMapped,
    /// 物理页分配失败（中间页表或目标页）
    AllocationFailed,
    /// 权限不足（如对只读页执行写操作）
    PermissionDenied,
    /// 操作不被当前架构支持
    NotSupported,
}
```

### 3.2 PageFlags 类型 

> **定义位置**: `minix_arch::paging`，使用 `bitflags` 定义（底层 `u16`）。

**设计原则**: 使用 `bitflags`（`u16` 底层）而非裸位操作，兼顾内存效率和语义清晰。

**为什么用 bitflags 而非 bool 结构体**：若使用 7 个 `bool` 字段的结构体，占用 7+ 字节（含 padding 可能 8~12 字节）；而 `bitflags` 包裹 `u16` 仅需 2 字节，且留足扩展空间（当前 7 位 + NO_CACHE/WRITE_THROUGH/GUARD_PAGE 等约 10 位，16 位足够覆盖）。更重要的是，标志位的组合操作在 bitflags 下更自然——`flags & !WRITABLE` 比 `PageFlags { writable: false, ..flags }` 更简洁。`bitflags` 同样具备类型安全和语义清晰性：`flags.contains(PageFlags::WRITABLE)` 明确表达查询意图，编译器在静态分派下可将转换完全内联，零运行时开销。

```rust
bitflags::bitflags! {
    /// 页表项标志位
    ///
    /// OS 层的语义接口，各架构 Paging 实现内部负责将其翻译为硬件 PTE 位编码。
    /// 使用 bitflags（u16 底层）兼顾内存效率和语义清晰。
    ///
    /// 标志分为两类：
    ///
    /// **状态类**（直接映射硬件，语义跨架构一致）：
    /// - `PRESENT` / `WRITABLE` / `USER_ACCESSIBLE` / `ACCESSED` / `DIRTY`
    ///
    /// **策略类**（需要翻译，部分架构为反逻辑）：
    /// - `EXECUTABLE`：x86-64 为 NX 位（反逻辑），ARM64 为 PXN 位（反逻辑），RISC-V 为 X 位（正逻辑）
    /// - `GLOBAL`：x86-64 为 G 位（正逻辑），ARM64 为 nG 位（反逻辑）
    /// - `WRITE_THROUGH` / `NO_CACHE`：缓存策略，各架构编码差异大
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PageFlags: u16 {
        const PRESENT         = 1 << 0;
        const WRITABLE        = 1 << 1;
        const USER_ACCESSIBLE = 1 << 2;
        const EXECUTABLE      = 1 << 3;
        const GLOBAL          = 1 << 4;
        const WRITE_THROUGH   = 1 << 5;
        const NO_CACHE        = 1 << 6;
        const ACCESSED        = 1 << 7;
        const DIRTY           = 1 << 8;
    }
}

impl PageFlags {
    /// User-space read-only. Minix3: `PTF_PRESENT|PTF_USER`
    pub const fn read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    /// User-space read-write. Minix3: `PTF_PRESENT|PTF_USER|PTF_WRITE`
    pub const fn read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    /// Kernel read-only (global).
    pub const fn kernel_read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::GLOBAL.bits()
        )
    }

    /// Kernel read-write (global).
    pub const fn kernel_read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::GLOBAL.bits()
        )
    }

    /// Kernel executable (global, W^X).
    pub const fn kernel_executable() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::EXECUTABLE.bits() | Self::GLOBAL.bits()
        )
    }
}
```

**相比C的优势**:
- 类型安全：使用 bitflags 而非裸位操作
- 语义清晰：常量名直接表达含义
- 编译期检查：标志位组合错误在编译时发现

**OS 语义 flags → 硬件 flags 的转换**：`PageFlags` 是 OS 层的语义接口，各架构 `Paging` 实现内部负责将其翻译为硬件 PTE 位编码。`map()` 不是高频操作（fork / mmap / page fault 级别），且转换在静态分派下完全内联，零运行时开销。

```rust
// 【设计目标】以下为未来架构实现的示例代码，当前未实现
// x86-64：语义 flags → PTE 位编码
impl X86_64Paging {
    fn flags_to_hw(flags: PageFlags) -> u64 {
        let mut pte = 0u64;
        if flags.contains(PageFlags::PRESENT)         { pte |= 1; }          // bit 0
        if flags.contains(PageFlags::WRITABLE)        { pte |= 1 << 1; }    // bit 1
        if flags.contains(PageFlags::USER_ACCESSIBLE) { pte |= 1 << 2; }    // bit 2
        if flags.contains(PageFlags::GLOBAL)          { pte |= 1 << 8; }    // bit 8
        if !flags.contains(PageFlags::EXECUTABLE)     { pte |= 1 << 63; }   // XD, 反逻辑
        pte
    }
}

// ARM64：语义 flags → PTE 位编码（位位置完全不同）
impl Arm64Paging {
    fn flags_to_hw(flags: PageFlags) -> u64 {
        let mut pte = 0u64;
        if flags.contains(PageFlags::PRESENT)         { pte |= 1; }          // Valid bit
        if flags.contains(PageFlags::WRITABLE)        { pte |= 1 << 6; }    // AP[1]
        if flags.contains(PageFlags::USER_ACCESSIBLE) { pte |= 1 << 7; }    // AP[0]
        if !flags.contains(PageFlags::EXECUTABLE)     { pte |= 1 << 54; }   // PXN
        if !flags.contains(PageFlags::GLOBAL)         { pte |= 1 << 11; }   // nG, 反逻辑
        pte
    }
}
```

### 3.3 PagingWithId Trait（PCID/ASID 预留）

> **定义位置**: `minix_arch::paging_ext`

**设计原则**: TLB 进程标识是跨架构通用机制，但并非所有架构都支持，因此作为可选 trait。

**各架构 TLB 进程标识对比**:

| 架构 | 机制 | 位宽 | 寄存器 |
|------|------|------|--------|
| x86-64 | PCID（Process-Context Identifier） | 12-bit（0-4095） | CR4.PCIDE 启用，CR3[11:0] 传递 PCID |
| ARM64 | ASID（Address Space Identifier） | 8/16-bit | TTBR0_EL1[63:48] 或 TTBR1_EL1[63:48] |
| RISC-V | ASID | 16-bit（Sv39/Sv48） | satp[63:44] |

**trait 定义**:

```rust
/// TLB 进程标识支持（可选 trait）
///
/// 为 TLB 条目标记进程标识，避免上下文切换时刷新整个 TLB，
/// 从而显著减少 TLB miss 开销。
///
/// **注意**：本 trait 不定义 ASID/PCID 的生命周期语义和复用策略。
/// ASID 的分配/回收策略、TLB shootdown 一致性维护、generation counter
/// 等机制由上层 VM 管理器负责。本 trait 仅提供底层硬件操作的原语。
pub trait PagingWithId: Paging {
    /// 此架构的地址空间标识符类型
    type AddressSpaceId: Copy + Eq + Debug + Send;

    /// 分配一个新的地址空间 ID
    fn alloc_asid(&self) -> Result<Self::AddressSpaceId, PageTableError>;

    /// 释放地址空间 ID
    fn free_asid(&self, id: Self::AddressSpaceId);

    /// 激活页表并指定 ASID
    unsafe fn switch_with_asid(&self, id: Self::AddressSpaceId);

    /// 刷新指定 ASID 的 TLB 条目
    unsafe fn flush_tlb_asid(&self, id: Self::AddressSpaceId);

    /// 刷新指定虚拟地址在指定 ASID 中的 TLB 条目
    unsafe fn flush_tlb_addr_asid(&self, vaddr: VirBytes, id: Self::AddressSpaceId);
}
```

**各架构实现策略**:

- **x86-64**: `type AddressSpaceId = Pcid`（newtype around u16）；`switch_with_asid` 通过 `CR3 = root_paddr | id.bits()` 实现，无需全局 TLB 刷新；`flush_tlb_asid` 使用 `INVLPCID` 指令
- **ARM64**: `type AddressSpaceId = Asid`（newtype around u16）；`switch_with_asid` 通过 `TTBR0_EL1 = root_paddr | (id.bits() << 48)` 实现；`flush_tlb_asid` 使用 `TLBI VAAE1IS` 指令
- **RISC-V 64**: `type AddressSpaceId = Asid`（newtype around u16）；`switch_with_asid` 通过 `satp = (mode << 60) | (id.bits() << 44) | (root_paddr >> 12)` 实现；`flush_tlb_asid` 使用 `SFENCE.VMA x0, id` 指令

### 3.4 HugePages Trait（大页预留）

> **定义位置**: `minix_arch::paging_ext`

**设计原则**: 大页支持同样是跨架构通用但非必须的机制，作为可选 trait。

**各架构大页对比**:

| 架构 | 大页大小 | PTE 标志 |
|------|---------|---------|
| x86-64 | 2MB（PD level）/ 1GB（PDPT level） | PS bit（bit 7） |
| ARM64 | 2MB（block entry at PMD）/ 1GB（block entry at PUD） | level=2/1 block descriptor |
| RISC-V | 2MB（megapage at PD）/ 1GB（gigapage at PDPT） | PTE 的 RSW 字段标记 |

**trait 定义**:

```rust
/// 大页支持（可选 trait）
pub trait HugePages: Paging {
    /// 支持的大页大小列表
    const HUGE_PAGE_SIZES: &'static [usize];

    /// 使用大页映射
    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    /// 检查指定大小的大页是否支持
    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }
}
```

### 3.5 架构无关常量

**设计原则**: 通过 trait 关联常量提供，而非全局常量。

```rust
/// 通过 Paging trait 获取页大小
fn example<P: Paging>() {
    let page_size = P::PAGE_SIZE;  // 4096 for most architectures
}
```

**为什么不用全局常量**:
- 不同架构页大小可能不同（如 ARM64 支持 4KB/16KB/64KB）
- 通过 trait 关联常量，代码自动适配目标架构
- Mock 实现可以自定义页大小用于测试

**MockPaging 的常量**:

```rust
impl Paging for MockPaging {
    const PAGE_SIZE: usize = 4096;  // 使用标准 4KB 页
}
```

---

## 4. 实现详解

> 本章使用的地址类型（`VirBytes`/`PhysBytes`）和架构抽象定义见 §5。

### 4.1 MockPaging 实现

**Mock 实现**:

```rust
/// Mock 分页实现
///
/// 用于用户态测试的软件模拟实现。
/// 不操作真实硬件，仅在内存中维护映射表。
///
/// **线程模型**：非并发安全。`mappings` 字段使用 `BTreeMap` 且未加锁，
/// 仅设计用于单线程测试（`#[cfg(test)]`）。多线程测试需外部同步。
#[derive(Debug)]
pub struct MockPaging {
    /// 页表 ID
    id: usize,
    /// 虚拟地址到(物理地址, 标志)的映射
    mappings: BTreeMap<u64, (u64, PageFlags)>,
    /// 页表根物理地址（模拟）
    root_phys: u64,
}

impl Paging for MockPaging {
    const PAGE_SIZE: usize = 4096;

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        let id = MOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            id,
            mappings: BTreeMap::new(),
            root_phys: 0x1000 + (id as u64 * 0x1000),
        })
    }

    unsafe fn destroy(&mut self) {
        self.mappings.clear();
        if ACTIVE_MOCK_TABLE.load(Ordering::SeqCst) == self.id {
            ACTIVE_MOCK_TABLE.store(NO_ACTIVE_TABLE, Ordering::SeqCst);
        }
    }

    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>
    {
        let v = vaddr.0;
        let p = paddr.0;

        if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
            return Err(PageTableError::InvalidAddress);
        }

        if self.mappings.contains_key(&v) {
            return Err(PageTableError::AlreadyMapped);
        }

        self.mappings.insert(v, (p, flags));
        Ok(())
    }

    fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>
    {
        let v = vaddr.0;
        let p = paddr.0;

        if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
            return Err(PageTableError::InvalidAddress);
        }

        let old = self.mappings.insert(v, (p, flags))
            .map(|(old_p, old_f)| (PhysBytes(old_p), old_f));
        Ok(old)
    }

    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        let v = vaddr.0;

        if v % Self::PAGE_SIZE as u64 != 0 {
            return Err(PageTableError::InvalidAddress);
        }

        match self.mappings.remove(&v) {
            Some((p, _)) => Ok(PhysBytes(p)),
            None => Err(PageTableError::NotMapped),
        }
    }

    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
        -> Result<(), PageTableError>
    {
        match self.mappings.get_mut(&vaddr.0) {
            Some((_, f)) => {
                *f = flags;
                Ok(())
            }
            None => Err(PageTableError::NotMapped),
        }
    }

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        self.mappings.get(&vaddr.0)
            .map(|(p, f)| (PhysBytes(*p), *f))
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_phys)
    }

    unsafe fn switch(&self) {
        ACTIVE_MOCK_TABLE.store(self.id, Ordering::SeqCst);
    }

    unsafe fn flush_tlb(&self) {}

    unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {}
}
```

**使用示例**:

```rust
use minix_arch::paging::{Paging, MockPaging, PageFlags};
use minix_types::{VirBytes, PhysBytes};

// 创建页表
let mut page_table = MockPaging::new()?;

// 映射页面
let vaddr = VirBytes(0x1000);
let paddr = PhysBytes(0x2000);
page_table.map(vaddr, paddr, PageFlags::read_write())?;

// 查询映射
if let Some((phys, flags)) = page_table.query(vaddr) {
    assert_eq!(phys, paddr);
    assert!(flags.contains(PageFlags::WRITABLE));
}

// 激活页表
unsafe { page_table.switch(); }

// 获取根地址（用于进程切换）
let root = page_table.root_paddr();
```

**关键设计点**:
1. **纯软件实现**: 不依赖任何硬件特性
2. **BTreeMap 存储**: 使用标准库的 BTreeMap 维护映射
3. **完整 trait 实现**: 实现 Paging trait 的所有方法

### 4.2 地址对齐

**设计决策**: `page_align`/`page_align_down` 作为模块级自由函数定义在
VM crate 的 `pagetable/mod.rs` 中（以 `PageTable` 的 `PAGE_SIZE` 为基准），
而非 `Paging` trait 的方法或具体实现的私有方法。理由：对齐操作不依赖页表实例，
只依赖 `PAGE_SIZE` 常量，作为自由函数更自然（`page_align(addr)` vs
`pt.page_align(addr)`）。放在 VM crate 而非 `minix_types`，是因为
`PAGE_SIZE` 是 `Paging` trait 的关联常量，VM crate 是唯一的使用方。

**对齐辅助方法**:

```rust
// VM crate: pagetable/mod.rs 中的模块级自由函数
pub(crate) fn page_align(addr: VirBytes) -> VirBytes {
    let ps = <PageTable as Paging>::PAGE_SIZE as u64;
    VirBytes((addr.0 + ps - 1) & !(ps - 1))
}

pub(crate) fn page_align_down(addr: VirBytes) -> VirBytes {
    let ps = <PageTable as Paging>::PAGE_SIZE as u64;
    VirBytes(addr.0 & !(ps - 1))
}
```

**使用示例**:

```rust
use minix_vm::pagetable::page_align;

// 对齐到页边界
let aligned = page_align(VirBytes(0x1234));
assert_eq!(aligned, VirBytes(0x2000));

// 向下对齐
let down = page_align_down(VirBytes(0x1234));
assert_eq!(down, VirBytes(0x1000));
```

### 4.3 标志位操作

**PageFlags 操作**:

```rust
// 创建用户可读写页
let flags = PageFlags::read_write();

// 创建用户只读页
let flags = PageFlags::read_only();

// 自定义标志
let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER_ACCESSIBLE;

// 修改标志（CoW 时设置为只读）
let cow_flags = PageFlags::read_write() - PageFlags::WRITABLE;

// 检查标志
if flags.contains(PageFlags::WRITABLE) {
    // 页面可写
}
```

**常用标志组合**:

| 组合 | 用途 |
|------|------|
| `read_only()` | 代码段 |
| `read_write()` | 数据段、堆、栈 |
| `kernel_read_only()` | 内核只读页 |
| `kernel_read_write()` | 内核读写页 |
| `kernel_executable()` | 内核代码页 |

**硬件自动设置的标志**:

| 标志 | 设置时机 | 用途 |
|------|----------|------|
| `accessed` | CPU 读取/写入页面时 | 页面置换算法 |
| `dirty` | CPU 写入页面时 | 判断是否需要写回磁盘 |

**架构差异**: `accessed` 和 `dirty` 在不同架构上的语义不同：

| 架构 | Accessed | Dirty |
|------|----------|-------|
| x86-64 | 硬件自动设置（PTE bit 5） | 硬件自动设置（PTE bit 6） |
| ARM64 | 硬件设置（AF bit） | 软件管理（DBM feature 可选） |
| RISC-V | 软件（Sv39）/ 硬件（Svpbmt） | 软件（Sv39）/ 硬件（Svdet） |

设计决策：`PageFlags` 的 `accessed`/`dirty` 字段在所有架构上保留，但语义不同——x86-64 上为硬件自动设置、软件只读；ARM64/RISC-V 上可能需要软件模拟（在 `query()` 时通过架构特定方式检查）。

**trait 抽象如何容纳这种差异**：x86-64 倾向于由硬件自动完成 accessed/dirty 的设置，RISC-V 倾向于由软件在 page fault handler 中完成。关键在于，trait 抽象的是 **OS 可观测的行为**，而非内部机制。OS 的协议在所有架构上一致：

1. `map()` → 建立映射，设置初始标志
2. [进程访问页面] → 这一步对 trait 不可见
3. `query()` → 查询当前状态（accessed/dirty 等）

差异藏在第 2 步的实现里——x86-64 由硬件在 CPU 访问时自动设置 PTE 位，RISC-V 由 page fault handler 调用 `update_flags()` 设置后恢复执行。从 trait 调用者的视角看，第 2 步是黑盒，只关心第 3 步 `query()` 返回的结果是否正确。

关于时间点语义：x86-64 的 accessed 位在 CPU 访问页面的同一时刻由硬件设置，RISC-V 的 accessed 位在 page fault handler 中设置，比实际访问稍晚。但这不影响 trait 抽象，因为 OS 调用 `query()` 检查时 handler 一定已经执行完毕，且页面置换算法是周期性扫描，不要求实时精确。若 OS 需要主动标记，`update_flags()` 已覆盖——各架构实现会执行对应的 TLB 刷新指令（x86-64 的 INVLPG / RISC-V 的 SFENCE.VMA），语义一致。

**并发安全**：对于硬件自动设置 accessed/dirty 的架构（如 x86-64），软件在读取-修改-写回 PTE 时需注意与硬件更新之间的竞争（例如使用原子 RMW 操作或仅在中断禁用下操作）。该细节由各架构 `Paging` 实现内部保证，trait 调用者无需关心。

---

## 5. 与全局概念的关系

### 5.1 物理地址 vs 虚拟地址

**Minix3 类型定义**:

```c
// minix/include/minix/type.h
typedef unsigned long phys_bytes;  // 物理地址/长度
typedef long unsigned int vir_bytes;  // 虚拟地址/长度
```

**Rust 类型定义**:

```rust
/// 虚拟地址/字节数
///
/// 用于进程地址空间中的地址和长度。
/// 被 PM、VM、VFS、Kernel 共用。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VirBytes(pub u64);

/// 物理地址
///
/// 用于物理内存地址。
/// 被 VM、Kernel 使用。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysBytes(pub u64);
```

> **为什么用 `u64` 而非 `usize`**：虚拟/物理地址的宽度由架构决定，不等于指针大小。
> x86-64 的虚拟地址仅 48 位（或 57 位 w/ L5PT）但存储在 64 位寄存器中，
> 物理地址宽度为 52 位。使用 `u64` 明确表达"这是一个 64 位地址值"，
> 而 `usize` 的语义是"指针大小"，在 32 位目标上为 32 位，会导致地址截断。
> 此外，`u64` 保证跨平台布局一致，便于内核-用户态共享结构体。

**类型对比**:

| 特性 | `VirBytes` | `PhysBytes` |
|------|------------|-------------|
| 语义 | 进程虚拟地址空间 | 物理内存地址 |
| 转换 | 需要页表转换 | 直接访问内存 |
| 隔离 | 每个进程独立 | 全局共享 |
| 用途 | 用户态指针、区域长度 | DMA、页表操作 |

**为什么使用 newtype 而非裸类型**:

```rust
// ❌ 错误：类型混淆
fn copy_data(dst: u64, src: u64, len: u64);  // 哪个是虚拟地址？哪个是物理地址？

// ✅ 正确：类型安全
fn copy_data(dst: VirBytes, src: PhysBytes, len: VirBytes);  // 清晰明确
```

**类型安全的好处**:
1. **编译期检查**: 不会将虚拟地址误传给需要物理地址的函数
2. **文档化**: 类型名直接表达语义
3. **重构安全**: 修改类型定义时编译器帮助发现所有使用点

**算术运算支持**:

```rust
// VirBytes 支持算术运算
let base = VirBytes(0x1000);
let size = VirBytes(0x100);
let end = base + size;  // VirBytes(0x1100)

// 与 u64 的混合运算
let aligned = VirBytes(0x1234) + 0x100 - 0x34;  // VirBytes(0x1300)
```

### 5.2 架构抽象

**设计原则**: 通过 trait 抽象架构差异，统一 OS 接口。

**架构差异对比**:

> ⚠️ 以下 ARM64/RISC-V 信息来自架构手册，非 Minix3 源码（Minix3 仅支持 x86-32），
> 仅供 Rust 版本设计参考。

| 特性 | Mock | x86-64 | ARM64 | RISC-V 64 |
|------|------|--------|-------|-----------|
| 页大小 | 4KB | 4KB（也支持 2MB/1GB 大页） | 4KB/16KB/64KB | 4KB（也支持 2MB/1GB） |
| 页表级数 | 1（模拟） | 4 级（PML4→PDPT→PD→PT） | 3-4 级（PGD→PUD→PMD→PTE） | 3-4 级（Sv39/Sv48/Sv57） |
| 虚拟地址宽度 | 64-bit（模拟） | 48-bit（57-bit w/ L5PT） | 48-bit（52-bit w/ FEAT_LVA） | 39/48/57-bit |
| 物理地址宽度 | 64-bit（模拟） | 52-bit | 48-bit（52-bit w/ FEAT_PA） | 56-bit |
| PTE 大小 | N/A | 64-bit | 64-bit | 64-bit |
| PTE 条目数/表 | N/A | 512 | 512（4KB页）/ 2048（16KB页） | 512 |
| NX/Execute 位 | 模拟 | Bit 63（XD） | PXN/AP[1] | X（bit 3） |
| Global 页 | 模拟 | Bit 8（G） | nG bit（反逻辑） | N/A（ASID 替代） |
| ASID/PCID | 模拟 | PCID（CR4.PCIDE） | ASID（TTBR0/1_EL1） | ASID（satp） |
| 大页支持 | N/A | 2MB（PD）/ 1GB（PDPT） | 2MB（block）/ 1GB（block） | 2MB（megapage）/ 1GB（gigapage） |
| Dirty/Accessed | 模拟 | 硬件自动设置 | Access: 硬件; Dirty: 软件 | 软件（Sv39）/ 硬件（Sv57 w/ Svpbmt） |
| TLB 刷新 | N/A | INVLPG / INVLPCID / MOV CR3 | TLBI VAAE1IS 等 | SFENCE.VMA |

**Paging trait 统一接口**:

```rust
/// 页表管理 trait
///
/// 各架构实现此 trait，OS 代码使用统一接口。
///
/// **设计选择**：当前 trait 采用"扁平映射"语义——`map()` 在调用者看来
/// 是单步操作，中间页表的按需分配由实现内部处理，不暴露给调用者。
/// 若未来需要 THP split/merge 等精细控制，可扩展此 trait 或引入
/// 新的 `PagingLevel` trait。
pub trait Paging {
    /// 页大小（架构相关）
    const PAGE_SIZE: usize;

    /// 创建页表
    fn new() -> Result<Self, PageTableError>;

    /// 映射页面
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 激活页表
    unsafe fn switch(&self);
}
```

**使用泛型编写架构无关代码**:

```rust
/// 创建进程地址空间
///
/// 使用泛型约束，支持任何实现 Paging trait 的架构。
fn create_address_space<P: Paging>() -> Result<P, PageTableError> {
    let mut page_table = P::new()?;

    // 映射内核空间
    let kernel_start = VirBytes(0xFFFF800000000000);
    let kernel_phys = PhysBytes(0x0);
    page_table.map(kernel_start, kernel_phys, PageFlags::read_only())?;

    Ok(page_table)
}

// Mock 测试
#[cfg(test)]
fn test_create_address_space() {
    let pt = create_address_space::<MockPaging>();
    assert!(pt.is_ok());
}

// 未来 x86-64 实现
// let pt = create_address_space::<X86_64Paging>();
```

**PageFlags 架构适配**:

```rust
/// 页标志（架构无关表示，bitflags u16 底层）
bitflags::bitflags! {
    pub struct PageFlags: u16 {
        const PRESENT         = 1 << 0;
        const WRITABLE        = 1 << 1;
        const USER_ACCESSIBLE = 1 << 2;
        const EXECUTABLE      = 1 << 3;
        const GLOBAL          = 1 << 4;
        // ...
    }
}

// 【设计目标】以下为未来架构实现的示例代码，当前未实现
// x86-64 实现：转换为硬件标志位
impl X86_64Paging {
    fn flags_to_hw(flags: PageFlags) -> u64 {
        let mut pte_flags = 0u64;
        if flags.contains(PageFlags::PRESENT)         { pte_flags |= 1; }
        if flags.contains(PageFlags::WRITABLE)        { pte_flags |= 2; }
        if flags.contains(PageFlags::USER_ACCESSIBLE) { pte_flags |= 4; }
        if !flags.contains(PageFlags::EXECUTABLE)     { pte_flags |= 1 << 63; }
        pte_flags
    }
}

// ARM64 实现：转换为硬件标志位
impl Arm64Paging {
    fn flags_to_hw(flags: PageFlags) -> u64 {
        let mut pte_flags = 0u64;
        if flags.contains(PageFlags::PRESENT)         { pte_flags |= 1; }
        if flags.contains(PageFlags::WRITABLE)        { pte_flags |= 1 << 6; }
        if !flags.contains(PageFlags::EXECUTABLE)     { pte_flags |= 1 << 54; }
        pte_flags
    }
}
```

**目录结构**:

```
minix-arch crate
├── src/
│   ├── lib.rs          # 导出 trait + CurrentPaging
│   ├── paging.rs       # Paging trait + PageFlags + PageTableError
│   ├── paging_ext.rs   # PagingWithId + HugePages（可选 trait）
│   ├── mock/
│   │   └── mod.rs      # MockPaging（实现 Paging + PagingWithId）
│   ├── x86_64/
│   │   ├── mod.rs      # X86_64Paging（实现 Paging + PagingWithId + HugePages）
│   │   ├── pte.rs      # x86-64 PTE/PDE 位域操作（内部使用）
│   │   └── pcid.rs     # PCID 管理
│   ├── arm64/
│   │   ├── mod.rs      # Arm64Paging（实现 Paging + PagingWithId + HugePages）
│   │   ├── pte.rs      # ARM64 PTE 位域操作（内部使用）
│   │   └── asid.rs     # ASID 管理
│   └── riscv64/
│       ├── mod.rs      # Riscv64Paging（实现 Paging + PagingWithId + HugePages）
│       ├── pte.rs      # RISC-V PTE 位域操作（内部使用）
│       └── asid.rs     # ASID 管理

VM crate
├── src/
│   ├── pagetable/
│   │   └── mod.rs      # type PageTable = CurrentPaging; 重导出 PageFlags 等
│   ├── vmproc/         # 使用 PageTable + Paging trait
│   └── region/         # 使用 PageFlags
```

**关键原则**:
- `Paging` trait 在 `minix-arch` crate 中定义，各架构实现在子模块中集中管理
- VM 只使用 `Paging` trait + `PageFlags`，不直接操作 PTE/PDE
- PTE/PDE 位域是架构内部实现（`x86_64/pte.rs` 等），不导出给 VM

### 5.3 Trait 层次结构与 vmproc 存储

#### 5.3.1 Trait 层次结构

```
                    ┌─────────────────┐
                    │   Paging        │  ← 核心页表操作（必须实现）
                    │  (paging.rs)    │
                    └────────┬────────┘
                             │
           ┌─────────────────┼─────────────────┐
           │                 │                 │
           ▼                 ▼                 ▼
    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
    │ PagingWithId │  │  HugePages   │  │ VmPagingExt  │
    │(paging_ext)  │  │(paging_ext)  │  │(paging_ext)  │
    └──────────────┘  └──────────────┘  └──────────────┘
         可选              可选            VM 策略层
      (PCID/ASID)       (大页支持)      (bind/map_kernel)
```

**Trait 职责划分**:

| Trait | 职责 | 对应 Minix3 | 必须实现 |
|-------|------|-------------|----------|
| `Paging` | 核心页表操作 | `pt_new`/`pt_free`/`pt_writemap` | ✅ 所有架构 |
| `PagingWithId` | TLB 进程标识 | 无（Minix3 未用 PCID） | ❌ 可选 |
| `HugePages` | 大页支持 | `I386_VM_BIGPAGE` | ❌ 可选 |
| `VmPagingExt` | VM 进程管理 | `pt_bind`/`pt_mapkernel`（Rust 设计聚合） | ✅ VM 需要 |

**为什么分离为多个 trait**:

1. **关注点分离**: `Paging` 是硬件机制抽象，`VmPagingExt` 是 VM 策略操作
2. **可选功能**: `PagingWithId`/`HugePages` 不是所有架构都支持
3. **编译期检查**: 未实现 `PagingWithId` 的架构无法使用 ASID 相关 API

#### 5.3.2 vmproc 中的页表存储

**Minix3 原版**:

```c
// minix/servers/vm/vmproc.h
struct vmproc {
    pt_t vm_pt;  // 直接嵌入，pt_t 是固定大小的结构体
    // ...
};
```

**Rust 设计**:

> **PageTable 类型**: `PageTable` 是类型别名，指向当前架构的具体页表实现：
> ```rust
> // VM crate: pagetable/mod.rs
> pub(crate) type PageTable = minix_arch::CurrentPaging;
> ```
> `CurrentPaging` 根据编译时 `feature` 指向具体类型（如 `MockPaging`、`X86_64Paging`）。
> 该具体类型同时实现多个 trait：
> - `Paging` — 核心页表操作（必须）
> - `VmPagingExt` — VM 策略操作（bind_to_process, map_kernel）
> - `PagingWithId` — TLB 进程标识（可选）
> - `HugePages` — 大页支持（可选）

```rust
// VM crate: vmproc/vmproc.rs
use core::mem::MaybeUninit;
use crate::pagetable::PageTable;

/// VM 进程控制块
///
/// 对应 Minix3 的 `struct vmproc`。
/// 注意：VmProc 不直接导出，外部代码通过 typestate view 访问。
pub(crate) struct VmProc {
    // ... 其他字段 ...

    /// 页表 - 未初始化直到 `init_page_table()` 被调用。
    ///
    /// Minix3 的 `pt_t` 直接嵌入在 `vmproc` 中。
    /// Rust 版本使用 `MaybeUninit<PageTable>` 实现延迟初始化，
    /// 因为 `PageTable::new()` 可能失败（内存分配错误）。
    pub(crate) vm_pt: MaybeUninit<PageTable>,

    /// `vm_pt` 是否已初始化（必须在 `assume_init` 前检查）
    pub(crate) vm_pt_initialized: bool,

    // ... 其他字段 ...
}
```

**为什么用 `MaybeUninit<PageTable>` 而非直接嵌入**:

| Minix3 `pt_t` | Rust `MaybeUninit<PageTable>` |
|---------------|-------------------------------|
| 固定大小结构体，可零初始化 | 初始化可能失败（内存分配、硬件资源等） |
| `pt_new()` 填充字段，无错误处理 | `PageTable::new()` 返回 `Result` |
| `memset(&vm_pt, 0, sizeof(pt_t))` | `MaybeUninit::uninit()` 表示未初始化状态 |

> **关于堆分配**: Mock 实现使用 `BTreeMap`（完整堆环境），但真实架构的页表实现
> 可能使用固定大小数组或 early heap。无论内部实现如何，`MaybeUninit` 提供了
> 延迟初始化的能力，让 `VmProc::vacant()` 可以是 `const fn`。

> **为什么不用 `Option<PageTable>`**：`Option` 的自动 `Drop` 会在 `VmProc` 被 drop
> 时自动调用 `PageTable::drop()`，这与内核的显式生命周期管理理念冲突——内核偏好
> `unsafe fn destroy()` 显式销毁，而非依赖 RAII。`MaybeUninit` 没有 `Drop`，
> 天然避免此问题。`ManuallyDrop<Option<PageTable>>` 虽然也能防止自动 drop，
> 但比 `MaybeUninit + bool` 更啰嗦，且语义类似。此外，typestate view
> （`EmptySlot` → `ActiveProc` → `ExitingProc`）在编译期保证了"初始化后才能访问"，
> `vm_pt_initialized` 字段是防御性运行时检查，不是主要保障。

**初始化失败处理**:

| 进程类型 | 失败处理 | 原因 |
|----------|----------|------|
| **init 进程** | `panic!` | 系统启动阶段内存充足，失败说明系统无法启动 |
| **普通进程** | 返回 `Err` | 用户态进程创建，内存可能不足，由调用者决定处理方式 |

**页表初始化流程**:

```rust
// VM crate: vmproc/vmproc_handle.rs
// 方法在 ActiveProc typestate view 上实现
impl ActiveProc<'_> {
    /// 初始化页表（对应 Minix3 的 `pt_new()` + `pt_mapkernel()`）
    pub(crate) fn init_page_table(&mut self) -> Result<(), PageTableError> {
        let mut pt = <PageTable as Paging>::new()?;
        pt.map_kernel()?;
        self.inner.vm_pt.write(pt);
        self.inner.vm_pt_initialized = true;
        Ok(())
    }

    /// 绑定页表到内核（对应 Minix3 的 `pt_bind()`）
    pub(crate) fn bind_page_table(&self) -> Result<(), PageTableError> {
        let pt = self.page_table();
        pt.bind_to_process(self.endpoint())
    }

    /// 获取页表引用（已初始化状态）
    pub(crate) fn page_table(&self) -> &PageTable {
        debug_assert!(self.inner.vm_pt_initialized, "vm_pt accessed before init_page_table()");
        unsafe { self.inner.vm_pt.assume_init_ref() }
    }

    /// 获取页表可变引用（已初始化状态）
    pub(crate) fn page_table_mut(&mut self) -> &mut PageTable {
        debug_assert!(self.inner.vm_pt_initialized, "vm_pt accessed before init_page_table()");
        unsafe { self.inner.vm_pt.assume_init_mut() }
    }
}
```

**页表销毁流程**:

```rust
// VM crate: vmproc/vmproc.rs
// 在 VmProc 上实现，通过 ExitingProc::reap() 调用
impl VmProc {
    /// 清理进程资源（对应 Minix3 的 `pt_free()` 等）
    ///
    /// # Safety
    /// 调用者必须确保页表不再被任何 CPU 使用。
    pub(crate) unsafe fn clear(&mut self) {
        if self.vm_pt_initialized {
            self.vm_pt.assume_init_mut().destroy();
            self.vm_pt_initialized = false;
        }
        // ... 清理其他资源 ...
    }
}
```

#### 5.3.3 类型别名统一入口

```rust
// VM crate: pagetable/mod.rs

/// 当前架构的页表实现类型
///
/// 根据编译时选择的架构特性，指向具体的实现：
/// - `feature = "mock"` → `MockPaging`
/// - `feature = "x86_64"` → `X86_64Paging`
/// - `feature = "arm64"` → `Arm64Paging`
pub(crate) type PageTable = minix_arch::CurrentPaging;

// 重导出常用类型
pub(crate) use minix_arch::paging::{PageFlags, PageTableError};

// 辅助函数
pub(crate) fn page_align(addr: VirBytes) -> VirBytes { /* ... */ }
pub(crate) fn page_align_down(addr: VirBytes) -> VirBytes { /* ... */ }
```

**使用示例**:

```rust
// 在 ActiveProc typestate view 上操作
fn setup_process_memory(active: &mut ActiveProc) -> Result<(), PageTableError> {
    // 1. 初始化页表（内部调用 map_kernel）
    active.init_page_table()?;

    // 2. 绑定到内核（对应 Minix3 的 pt_bind）
    active.bind_page_table()?;

    // 3. 映射用户区域
    let pt = active.page_table_mut();
    pt.map(
        VirBytes::new(0x400000),  // 用户空间起始地址
        PhysBytes::new(0x1000000),
        PageFlags::read_write(),
    )?;

    Ok(())
}
```

#### 5.3.4 与 Minix3 的对应关系

| Minix3 | Rust | 说明 |
|--------|------|------|
| `pt_t vm_pt` | `MaybeUninit<PageTable>` | 延迟初始化 |
| `pt_new(&vm_pt)` | `ActiveProc::init_page_table()` | 创建页表 + 映射内核 |
| `pt_free(&vm_pt)` | `VmProc::clear()` | 释放二级页表（不释放页目录，见 [pagetable.c:1427](minix3/minix/servers/vm/pagetable.c#L1427)） |
| `pt_bind(&vm_pt, vmp)` | `ActiveProc::bind_page_table()` | 绑定进程 |
| `pt_mapkernel(&vm_pt)` | `PageTable::map_kernel()` | 映射内核（init_page_table 内部调用） |
| `pt_writemap(...)` | `Paging::map()` / `Paging::update_flags()` | 映射页面（`WMF_WRITEFLAGSONLY` 对应 `update_flags`） |
| `pt_clearmapcache()` | （内部实现） | 清除映射缓存 |

---

## 6. 测试与验证

> 实际测试代码位于 `os/arch/src/paging.rs` 的 `mock::tests` 模块，
> 使用 `#[cfg(test)]` + `#[cfg(feature = "mock")]` 条件编译。
> 以下仅列出测试设计意图，不重复实际代码。

### 6.1 测试覆盖矩阵

| 测试场景 | 验证点 | 对应 Minix3 行为 |
|----------|--------|-----------------|
| `new()` + `destroy()` | 页表创建/销毁生命周期 | `pt_new()` / `pt_free()` |
| `map()` + `query()` | 映射后可查询 | `pt_writemap()` + `pt_checkrange()` |
| `map()` 重复映射 → `AlreadyMapped` | 不允许静默覆盖 | `pt_writemap()` 无 `WMF_OVERWRITE` 时行为 |
| `remap()` 覆盖映射 | 原子替换旧映射 | `pt_writemap()` + `WMF_OVERWRITE` |
| `unmap()` → `query()` 返回 None | 取消映射后不可查询 | `pt_writemap(MAP_NONE, 0)` |
| `unmap()` 未映射地址 → `NotMapped` | 取消不存在的映射报错 | Minix3 无此检查（静默忽略） |
| `update_flags()` 保留物理地址 | 只改标志不改映射目标 | `pt_writemap()` + `WMF_WRITEFLAGSONLY` |
| 未对齐地址 → `InvalidAddress` | 地址必须页对齐 | Minix3 隐式依赖硬件检查 |
| `PageFlags::read_only()` / `read_write()` | 预设标志位正确性 | `PTF_PRESENT|PTF_USER` / `PTF_PRESENT|PTF_USER|PTF_WRITE` |
| `PageFlags` 位运算（`-` / `|`） | CoW 清 WRITABLE、共享加 GLOBAL | Minix3 手动位操作 |

### 6.2 运行测试

```bash
cargo test -p minix-arch --features mock
```

---

## 7. 参见

- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作
- [01-vmproc-struct.md](01-vmproc-struct.md) - vm_pt 字段
- [17-vm-fork.md](17-vm-fork.md) - fork 时的页表复制

---

*分类: VM库 | 可被其他服务使用*
