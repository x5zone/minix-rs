# 09-vm-relocation: 初始化数据搬迁

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/alloc.c`  
> **说明**: Bootstrap 阶段结束后，将预留区域中的数据搬迁到堆上

---

## 1. 基本概念

### 1.1 什么是初始化数据搬迁

在 [05-vm-allocpage.md](05-vm-allocpage.md) 中，我们设计了 Typestate 模式：Bootstrap 阶段从内核预留区域分配内存，Normal 阶段从 VM 自己的堆分配。两个阶段之间需要一个过渡——**把 Bootstrap 阶段分配的数据从预留区域搬到堆上**。

```
Bootstrap 阶段                    Normal 阶段
┌──────────────────┐            ┌──────────────────┐
│ ReservedRegion   │            │ VM Heap          │
│  ┌─────────────┐ │            │  ┌─────────────┐ │
│  │ bitmap[]    │ │  搬迁 ──►  │  │ bitmap[]    │ │
│  │ page_cache[]│ │            │  │ page_cache[]│ │
│  │ free_lists[]│ │            │  │ free_lists[]│ │
│  │ ...         │ │            │  │ ...         │ │
│  └─────────────┘ │            │  └─────────────┘ │
└──────────────────┘            └──────────────────┘
```

**搬迁不是简单的 memcpy**。搬迁后的数据位于新的虚拟地址，所有指向旧地址的引用必须更新。

### 1.2 为什么需要搬迁

根本原因：**Bootstrap 阶段不知道物理内存总量**。

```
启动时序:
  T1: Kernel 传递 boot_info（含 memmap[]）
  T2: VmPageAllocator::<Bootstrap>::new()
      → 此时只知道"有一块预留区域可用"
      → 不知道总共有多少物理页
  T3: PhysAllocator::init(mem_chunks)
      → 遍历 memmap[]，计算出 total_pages
      → 此时才知道 bitmap 需要多大
      → 但 bitmap 必须现在就分配（否则 PhysAllocator 无法工作）
      → 只能从预留区域分配
  T4: 预留区域耗尽，无法继续分配
  T5: 必须切换到 Normal 阶段，使用完整堆
  T6: 搬迁：把 T3 中分配的数据搬到堆上
```

如果物理内存总量在编译期已知，可以直接在 BSS 中预留足够空间（Minix3 的做法）。但 Rust rewrite 的目标是通用性——物理内存总量由 boot_info 在运行时决定。

### 1.3 搬迁涉及的数据

回顾 Bootstrap 阶段分配了哪些数据：

| 分配时机 | 数据 | 大小 | 结构 |
|----------|------|------|------|
| PhysAllocator::init() | bitmap[] | total_pages / 8 bytes | 扁平 u64 数组 |
| PhysAllocator::init() | page_cache[] | 可配置（如 256 项） | 扁平 usize 数组 |
| BuddyAllocator::init() | free_list_heads[] | (MAX_ORDER+1) × 4 bytes | 扁平 u32 数组 |
| BuddyAllocator::init() | page_next[] | total_pages × 4 bytes | 扁平 u32 数组 |
| BuddyAllocator::init() | page_orders[] | total_pages × 1 byte | 扁平 u8 数组 |

**关键特征：全部是 SoA（Structure of Arrays）扁平数组**。没有指针、没有嵌套结构、没有链表。这意味着：
- 搬迁 = 分配新数组 + memcpy + 更新引用
- 不需要递归遍历对象图
- 不需要处理自引用循环

### 1.4 搬迁在初始化时序中的位置

```
T0: Kernel 启动 VM 进程
    ├── 映射 reserved_region 到 VM 地址空间
    └── 传递 boot_info

T1: main() → init_vm()
    │
    ├── T2: VmPageAllocator::<Bootstrap>::new(boot_info)
    │
    ├── T3: PhysAllocator::init(mem_chunks)
    │       → 从预留区域分配 bitmap[]、page_cache[] 等
    │
    ├── T4: pt_init()
    │       → 创建页表系统
    │
    ├── T5: allocator.into_normal()
    │       → 从 reserved 切出 PtRegion（3 页）
    │       → 此时 alloc_page() 走 alloc_phys + alloc_virt
    │
    ├── T6: relocate_phys_allocator()  ← 本文档的内容
    │       → 在堆上分配新数组
    │       → 复制数据
    │       → 更新 PhysAllocator 内部引用
    │       → 释放预留区域中的旧数组
    │
    └── init_vm() 返回

T7: 主循环开始
```

---

## 2. Minix3 C 源码分析

### 2.1 Minix3 如何"避免"搬迁：BSS 静态分配

Minix3 不需要搬迁，因为物理内存管理器的元数据在编译期就分配好了：

```c
// minix3/minix/servers/vm/alloc.c

#define MAX_FREE_PAGE_CACHE 256

static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS(MAX_FREEPAGES)];
static u32_t free_page_cache[MAX_FREE_PAGE_CACHE];
```

`PAGE_BITMAP_CHUNKS(MAX_FREEPAGES)` 在编译期展开为固定值（如 128KB），对应 4GB 地址空间。`free_pages_bitmap` 和 `free_page_cache` 都在 BSS 段中，由内核在加载 ELF 时自动映射。

**Minix3 的初始化流程**：

```c
// alloc.c
void mem_init(chunks)
{
    // free_pages_bitmap 已在 BSS 中，无需分配
    // 直接写 bitmap 即可
    for each chunk in chunks:
        for each page in chunk:
            FREE_BIT(free_pages_bitmap, page_num);
}
```

**优点**：零分配开销，初始化简单。
**代价**：bitmap 大小固定，浪费内存（如果物理内存只有 256MB，bitmap 仍然占 128KB）；不支持超过 4GB 的物理内存。

### 2.2 pt_init_done：Minix3 的阶段切换

Minix3 也有"初始化阶段"和"正常运行阶段"的区分，但用的是运行时标志：

```c
// minix3/minix/servers/vm/pagetable.c:328
static int pt_init_done;

// minix3/minix/servers/vm/pagetable.c:1311
void pt_init(...)
{
    ...
    pt_init_done = 1;  // 初始化完成
}
```

`pt_init_done` 的作用：在初始化完成前，`vm_mappages()` 不调用 `alloc_mem()`（因为物理内存管理器还没就绪）。初始化完成后，`pt_init_done = 1`，`vm_mappages()` 恢复正常行为。

**与 Typestate 的对比**：

| 维度 | Minix3 `pt_init_done` | Rust Typestate |
|------|----------------------|----------------|
| 机制 | 运行时 int 标志 | 编译期类型参数 |
| 错误检测 | 运行时（可能遗漏检查） | 编译期（不可能遗漏） |
| 旧状态可访问性 | 始终可访问 | 被消耗，不可访问 |
| 内存布局 | BSS 固定大小 | 运行时按需分配 |

### 2.3 spare_pagequeue：Minix3 对递归的处理

Minix3 的 `vm_allocpage()` 可能递归调用自身（见 [05-vm-allocpage.md §2.4](05-vm-allocpage.md#24-allocpage-的递归调用问题)）。Minix3 的解法是 `spare_pagequeue`——一个固定大小的备用页池：

```c
// minix3/minix/servers/vm/pagetable.c:60-107
// SPAREPAGES 因架构而异: i386=20, arm=150, SANITYCHECKS=200
#define SPAREPAGES 20
static void *spare_pagequeue;
static char static_sparepages[VM_PAGE_SIZE*STATIC_SPAREPAGES]
    __aligned(VM_PAGE_SIZE);
```

递归路径上的 `alloc_mem()` 从备用池取页，不走正常分配路径。但这个池是固定大小的——如果递归深度超过 20，系统会 panic。

**与 PtRegion 的对比**：Rust 方案用 PtRegion 从结构上消除了递归，不需要备用池，不需要担心耗尽。这是"治本"与"治标"的区别。

### 2.4 Minix3 方案在 Rust 下的局限性

Minix3 的 BSS + 运行时标志方案在 Rust 下有三个问题：

1. **BSS 不可行**：Rust `no_std` + 自定义 allocator 环境下，没有 BSS 段的自动管理。`static mut` 需要固定大小，而 bitmap 大小由运行时决定。
2. **运行时标志不安全**：`pt_init_done` 是全局可变状态，Rust 的借用检查器无法追踪它的状态变化。Typestate 将状态编码到类型中，编译器可以验证。
3. **固定大小浪费**：Minix3 的 bitmap 固定为 128KB（对应 4GB），在 64 位系统上不够用，在小内存系统上浪费。

---

## 3. Rust 设计决策

### 3.1 搬迁策略选择

搬迁的核心问题是：**如何把数据从旧位置移到新位置，同时保证系统在搬迁期间的一致性**。有三种策略：

#### 策略一：原地升级（In-place Upgrade）

**思路**：不搬迁。直接把预留区域的内存"标记"为堆的一部分，PhysAllocator 的元数据永远留在原地。

```
搬迁前:  [ Reserved Region | ... ]
搬迁后:  [ Reserved Region | ... ]  ← 同一块内存，只是"身份"变了
          ↑
          └── 现在属于 VM Heap
```

**实现**：在 `into_normal()` 时，不释放预留区域，而是将其虚拟地址范围注册到 VM 的虚拟地址分配器中，标记为"已占用"。后续堆分配从预留区域之后开始。

**优点**：
- 零拷贝，最快
- 不需要更新任何引用
- 实现最简单

**缺点**：
- 预留区域的位置由内核决定，可能不在 VM 期望的堆区域
- 预留区域大小有限（通常几 MB），堆的起始位置被"钉"在这里
- 如果预留区域在虚拟地址空间的中间，会造成地址空间碎片
- 语义不干净：预留区域的物理页是内核分配的，VM 堆的物理页是 VM 自己分配的，混在一起管理复杂

#### 策略二：复制搬迁（Copy Relocation）

**思路**：在堆上分配新内存，把数据复制过去，更新引用，释放旧内存。

```
搬迁前:
  Reserved Region:  [bitmap][page_cache][free_lists]
  VM Heap:          [.......................................]

搬迁后:
  Reserved Region:  [free    ][free      ][free       ]  ← 归还
  VM Heap:          [bitmap][page_cache][free_lists][...]
```

**实现步骤**：
1. 在堆上分配与旧数组相同大小的新数组
2. memcpy 数据
3. 更新 PhysAllocator 内部指针指向新数组
4. 释放预留区域中的旧数组

**优点**：
- 堆的布局完全由 VM 控制
- 预留区域可以完全释放
- 语义清晰：Bootstrap 数据是"临时"的，Normal 数据是"永久"的

**缺点**：
- 需要一次完整拷贝
- 需要更新引用（但 SoA 结构使这很简单——只需更新几个指针）
- 搬迁期间 PhysAllocator 处于不一致状态（需要短暂暂停分配）

#### 策略三：双缓冲（Double Buffering）

**思路**：分配新数组，复制数据，然后**原子地**切换 PhysAllocator 的内部指针。搬迁期间，旧数组仍然可用。

```
搬迁前:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]
  PhysAllocator.new_bitmap ──► NULL

搬迁中:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]  ← 仍可用
  PhysAllocator.new_bitmap ──► [New Bitmap in Heap]          ← 已复制

切换后:
  PhysAllocator.bitmap ──► [New Bitmap in Heap]
  PhysAllocator.new_bitmap ──► NULL
  [Old Bitmap] 释放
```

**优点**：
- 搬迁期间 PhysAllocator 始终可用（旧指针仍有效）
- 切换是原子的（单条赋值语句）
- 如果搬迁失败，可以回滚（保留旧数组）

**缺点**：
- 短暂的双倍内存占用
- 实现复杂度最高
- 对于 SoA 扁平数组来说，过度设计

#### 选择：复制搬迁

对于当前场景，**复制搬迁**是最佳选择：

1. **SoA 结构使引用更新极简**：只需更新 3~5 个 slice 指针，不需要遍历对象图。
2. **搬迁窗口极短**：memcpy 几个数组只需要微秒级时间。搬迁期间暂停分配是可接受的（初始化阶段没有并发请求）。
3. **双缓冲过度设计**：双缓冲的价值在于"搬迁期间系统仍可用"，但初始化阶段没有其他线程竞争 PhysAllocator。
4. **原地升级有长期代价**：把预留区域钉在地址空间中间，后续的虚拟地址管理会变复杂。

### 3.2 搬迁的边界条件

**搬迁期间 PhysAllocator 的一致性**：

搬迁期间，PhysAllocator 的内部指针指向旧数组。搬迁完成后，指针切换到新数组。在切换瞬间，PhysAllocator 处于不一致状态。解决方案：

- 搬迁在 `init_vm()` 中执行，此时还没有其他线程
- 搬迁期间不调用 `alloc_mem()` / `free_mem()`
- 搬迁是同步的、不可中断的

**搬迁失败的处理**：

如果堆分配失败（内存不足），搬迁无法完成。此时系统无法进入 Normal 阶段。处理方式：
- panic（初始化阶段，没有恢复的必要）
- 或者回退到原地升级（保留预留区域，标记为堆的一部分）

### 3.3 与 Typestate 的配合

搬迁是 `into_normal()` 之后的独立步骤。Typestate 保证了搬迁的调用时机：

```rust
// 编译期保证：只有 Normal 阶段才能调用搬迁
impl VmPageAllocator<Normal> {
    pub(crate) fn relocate_phys_allocator(&mut self) {
        // self.pt_region 可用（由 into_normal 初始化）
        // self.phys_alloc 不可用（已被 into_normal 消耗）
        // 搬迁通过 pt_region 分配新内存
    }
}
```

---

## 4. 实现详解

### 4.1 搬迁接口

```rust
// os/servers/vm/src/alloc_page.rs

impl VmPageAllocator<Normal, RealPtOps> {
    /// 将 PhysAllocator 的元数据从预留区域搬迁到堆上。
    pub(crate) fn relocate_phys_allocator(&mut self) {
        let pt_region = self.pt_region.as_mut()
            .expect("PtRegion must be initialized before relocation");
        pt_region.relocate_phys_allocator();
    }
}
```

### 4.2 PtRegion 中的搬迁逻辑

搬迁逻辑集中在 `PtRegion::relocate_phys_allocator()` 中，而非 PhysAllocator trait 的默认方法。这样设计的原因是：搬迁需要同时使用 PhysAllocator（分配物理页）和 PtRegion（分配虚拟地址并建立映射），将逻辑放在 PtRegion 中可以自然地访问两者。

```rust
// os/servers/vm/src/pt_region.rs

impl<O: PtOps> PtRegion<O> {
    pub(crate) fn relocate_phys_allocator(&mut self) {
        let count = self.phys_alloc.reloc_array_count();
        if count == 0 {
            return;
        }

        // 1. 在堆上分配新数组（物理页 + 虚拟地址 + 映射）
        let mut new_virts: [Option<VirBytes>; 4] = [None; 4];
        let mut total_bytes: [usize; 4] = [0; 4];

        for i in 0..count {
            let (_old_ptr, elem_count, elem_size) = self.phys_alloc.reloc_array_info(i);
            let bytes = elem_count * elem_size;
            total_bytes[i] = bytes;
            let pages = (bytes + PAGE_SIZE - 1) / PAGE_SIZE;

            for _ in 0..pages {
                let phys = self.phys_alloc.alloc_mem(1, PageAllocFlags::empty())
                    .expect("relocation: failed to allocate physical page");
                let virt = self.alloc_pt_page()
                    .expect("relocation: failed to allocate virtual address");
                if new_virts[i].is_none() {
                    new_virts[i] = Some(virt);
                }
                self.write_data_pte(virt, phys);
            }
        }

        // 2. 复制数据
        for i in 0..count {
            let (old_ptr, _elem_count, _elem_size) = self.phys_alloc.reloc_array_info(i);
            let new_virt = new_virts[i].unwrap();
            unsafe {
                core::ptr::copy_nonoverlapping(old_ptr, new_virt.0 as *mut u8, total_bytes[i]);
            }
        }

        // 3. 更新 PhysAllocator 内部指针
        let new_ptrs: [*mut u8; 4] = [
            new_virts[0].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[1].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[2].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[3].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
        ];
        self.phys_alloc.update_relocated_arrays(&new_ptrs[..count]);
    }
}
```

### 4.3 PhysAllocator trait 的搬迁接口

搬迁接口使用分步查询设计（`reloc_array_count()` + `reloc_array_info()`），而非返回 `Vec`。这避免了搬迁期间对堆分配器的依赖（搬迁时 alloc 可能尚未就绪）：

```rust
// os/servers/vm/src/phys_mem/alloc_trait.rs

pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError>;
    fn free_mem(&mut self, base: PhysBytes, clicks: usize);
    fn total_count(&self) -> usize;

    /// 需要搬迁的数组数量。
    fn reloc_array_count(&self) -> usize { 0 }

    /// 获取第 index 个数组的信息。
    /// 返回 (当前指针, 元素个数, 元素大小)。
    fn reloc_array_info(&self, _index: usize) -> (*const u8, usize, usize) {
        (core::ptr::null(), 0, 0)
    }

    /// 更新内部指针，指向新数组。
    /// new_ptrs 的顺序与 reloc_array_info() 的 index 顺序一致。
    fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {}
}
```

### 4.4 BitmapAllocator 的搬迁

```rust
// os/servers/vm/src/phys_mem/bitmap_alloc.rs

impl PhysAllocator for BitmapAllocator {
    fn reloc_array_count(&self) -> usize {
        2  // bitmap + page_cache
    }

    fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.bitmap.as_ptr() as *const u8, self.bitmap.len(), 8),
            1 => (self.page_cache.as_ptr() as *const u8, self.page_cache.len(), 8),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        unsafe {
            self.bitmap = core::slice::from_raw_parts_mut(
                new_ptrs[0] as *mut u64,
                self.bitmap.len(),
            );
            self.page_cache = core::slice::from_raw_parts_mut(
                new_ptrs[1] as *mut usize,
                self.page_cache.len(),
            );
        }
    }
}
```

### 4.5 BuddyAllocator 的搬迁

```rust
// os/servers/vm/src/phys_mem/buddy_alloc.rs

impl PhysAllocator for BuddyAllocator {
    fn reloc_array_count(&self) -> usize {
        3  // free_list_heads + page_next + page_orders
    }

    fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.free_list_heads.as_ptr() as *const u8, self.free_list_heads.len(), 4),
            1 => (self.page_next.as_ptr() as *const u8, self.page_next.len(), 4),
            2 => (self.page_orders.as_ptr() as *const u8, self.page_orders.len(), 1),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        unsafe {
            self.free_list_heads = core::slice::from_raw_parts_mut(
                new_ptrs[0] as *mut u32,
                self.free_list_heads.len(),
            );
            self.page_next = core::slice::from_raw_parts_mut(
                new_ptrs[1] as *mut u32,
                self.page_next.len(),
            );
            self.page_orders = core::slice::from_raw_parts_mut(
                new_ptrs[2] as *mut u8,
                self.page_orders.len(),
            );
        }
    }
}
```

### 4.6 搬迁的完整调用链

```rust
// os/servers/vm/src/main.rs (示意)

fn init_vm(boot_info: BootInfo) -> VmPageAllocator<Normal, RealPtOps> {
    // T2: Bootstrap
    let reserved = ReservedRegion::from_boot_info(&boot_info);
    let phys_alloc = Box::new(UninitPhysAllocator);
    let mut alloc = VmPageAllocator::<Bootstrap>::new(reserved, phys_alloc);

    // T3: 初始化 PhysAllocator（从预留区域分配元数据）
    let mem_chunks = get_mem_chunks(&boot_info);
    let phys_alloc = BitmapAllocator::init(&mut alloc, &mem_chunks);
    // ... 将 phys_alloc 注入到 alloc 中 ...

    // T4: 初始化页表
    pt_init(&mut alloc);

    // T5: 切换到 Normal
    let mut alloc = alloc.into_normal();

    // T6: 搬迁 PhysAllocator 元数据
    alloc.relocate_phys_allocator();

    alloc
}
```

---

## 5. 测试

### 5.1 搬迁后数据一致性

```rust
#[test]
fn test_relocation_data_integrity() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();

    // 搬迁前记录 PhysAllocator 状态
    let stats_before = normal.pt_region.as_ref().unwrap().phys_alloc.memstats();

    // 执行搬迁
    normal.relocate_phys_allocator();

    // 搬迁后状态一致
    let stats_after = normal.pt_region.as_ref().unwrap().phys_alloc.memstats();
    assert_eq!(stats_before.free_pages, stats_after.free_pages);
    assert_eq!(stats_before.largest_free, stats_after.largest_free);
}
```

### 5.2 搬迁后可正常分配

```rust
#[test]
fn test_allocation_after_relocation() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();
    normal.relocate_phys_allocator();

    // 搬迁后仍可正常分配
    let phys = normal.alloc_phys(1, PageAllocFlags::empty());
    assert!(phys.is_ok());
}
```

### 5.3 搬迁后释放再分配

```rust
#[test]
fn test_free_realloc_after_relocation() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();

    // 搬迁前分配一页
    let phys = normal.alloc_phys(1, PageAllocFlags::empty()).unwrap();

    // 搬迁
    normal.relocate_phys_allocator();

    // 释放
    normal.pt_region.as_mut().unwrap().phys_alloc.free_mem(phys, 1);

    // 重新分配同一页
    let phys2 = normal.alloc_phys(1, PageAllocFlags::empty()).unwrap();
    assert_eq!(phys.as_u64(), phys2.as_u64());
}
```

---

## 6. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（搬迁的主要对象）
- [05-vm-allocpage.md](05-vm-allocpage.md) - 页分配器（Typestate 模式，Bootstrap/Normal 阶段）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（搬迁依赖的映射能力）
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器（搬迁后堆的主要使用者）

---

*分类: VM库 | 使用范围: 仅 VM 内部，初始化阶段*
