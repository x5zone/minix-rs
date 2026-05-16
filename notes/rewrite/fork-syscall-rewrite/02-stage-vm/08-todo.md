# 08-todo: 待修复项

> 本文档列出 04/05/07/08 四份文档中需要修改的全部位置。
> 两个背景：
> 1. VmAllocator 需要内置 bump allocator（sub-page 管理），不再以"页级桥接"作为当前实现。
> 2. VM 在 Direct Map 下已特殊化——不再有传统堆、不再需要 brk/sbrk、不为自己写页表。

---

## 08-slab-allocator.md

### A. §1.2 L47：澄清 Minix3 的 malloc/brk 使用方式

**当前**：
```
> ⚠️ **澄清**：VM **可以**使用 malloc。VM 有自己的 `brk` 快速路径（`utility.c:_brk()`），
> 直接调用 `alloc_mem()` 分配物理页并映射到自己的地址空间。
```

**问题**：这是描述 Minix3 的事实，但未标注环境。读者可能以为 Rust 版本也这样。

**修改**：在段落后加一句：
```
> 以上是 Minix3（32 位 C 实现）的现状。Rust 版本在 Direct Map 下不再需要 brk——
> VM 通过 `vm_phys_to_virt()` 直接访问物理页，无需为自身的堆扩展虚拟地址空间。
> 详见 §4.1。
```

---

### B. §3.4 L1653："堆内存分配"表述

**当前**：
```
- VM 自身的堆内存分配不触发内核 IPC（Direct Map 已预先映射全部物理内存，
  分配只需在已映射区域内挑选空闲页，无需写页表或刷 TLB）
```

**问题**："堆内存分配"暗示 VM 仍有传统堆，但 Direct Map 下概念不同。

**修改**：
```
- VM 自身的动态内存分配不触发内核 IPC。Direct Map 已预先映射全部物理内存，
  VmAllocator 从 Direct Map 区域拿页后在内部切分（§4.1 bump allocator），
  整个链路是纯本地操作：alloc_phys → vm_phys_to_virt → 页内切分 → 返回指针。
```

---

### C. §4.1：重大改写——以 bump allocator 为标准实现

**当前状态**：§4.1 的代码展示的是"页级桥接"版本（unit struct + 直接 alloc_phys）。
注释写"当前按页分配，未实现细粒度管理"——这是半成品状态。

**改写要求**：以 bump allocator 为**正文当前实现**。文档应该展示完整方案，不是桥接方案。

**改写后的 §4.1 结构**：

1. **接入原理**（保留现有）：`#[global_allocator]` + `GlobalAlloc` trait + `AtomicPtr` 桥接 `&self → &mut`

2. **设计知识点**（新增）：
   - Rust `alloc` crate 是纯透传——`Box::new()` → `__rust_alloc` → `GLOBAL.alloc(layout)`，中间无任何缓冲
   - 因此 `GlobalAlloc::alloc()` 的实现者必须自己做 sub-page 管理
   - 如果不做 sub-page 管理（"页级桥接"），每次 `Box::new(24B)` 消耗一整页 4096B
   - 对比 Minix3：slab 从 `vm_allocpage()` 拿页后切割为固定大小对象

3. **bump allocator 实现**（正文代码）：
```rust
pub(crate) struct VmAllocator {
    // 当前 bump arena
    arena_base: AssumeSyncCell<*mut u8>,
    cursor: AssumeSyncCell<usize>,
    // backing store 指针（用于 dealloc 整页归还）
    page_alloc_ptr: AtomicPtr<VmPageAllocator>,
}

impl VmAllocator {
    const ARENA_PAGES: usize = 16;   // 预分配 16 页（64KB）作为 arena

    fn refill_arena(&self) {
        let alloc = unsafe { &mut *self.page_alloc_ptr.load(Ordering::SeqCst) };
        let phys = alloc.alloc_phys(Self::ARENA_PAGES, PageAllocFlags::empty())
            .expect("VmAllocator: out of physical memory");
        let va = unsafe { vm_phys_to_virt(phys).0 as *mut u8 };
        self.arena_base.store(va);
        self.cursor.store(0);
    }
}

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        // bump: 对齐 cursor，切出 size 字节
        let base = self.arena_base.load();
        let cursor = self.cursor.load();
        let ptr = base.add(cursor);
        let offset = ptr.align_offset(align);
        let alloc_start = ptr.add(offset);
        let total = offset + size;
        if cursor + total > Self::ARENA_PAGES * PAGE_SIZE {
            self.refill_arena();
            return self.alloc(layout);  // 重新在新 arena 上分配
        }
        self.cursor.store(cursor + total);
        alloc_start
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op：bump allocator 不回收单个对象。
        // arena 页在 VM 进程退出时随 Direct Map 一起释放。
    }
}
```

4. **设计权衡**（保留并扩展）：
   - bump vs slab vs free list
   - dealloc no-op 的合理性（VM 长生命周期，单对象不回收）
   - 预分配 16 页的理由
   - 为何不用 jemalloc/mimalloc：它们默认面向有 mmap 的 OS userspace

5. **关键设计决策**段落（保留现有，略作调整）

---

### D. 附录 A：降格重组

**当前问题**：
- "阶段 1（当前）：VmAllocator" 后面立刻就是 jemalloc/mimalloc 对比
- 把 page provider 和 heap allocator 混在一起
- 对比维度（NUMA、多线程）对单线程 VM 无意义

**改写结构**：

```
A.1 当前实现：bump allocator
    - 为什么 bump：单线程、小对象为主、实现简单
    - 当前局限：bump 不回收单对象；未来可能需要 slab 补充

A.2 设计层次
    - Layer 1: PhysAlloc / VmPageAllocator（页提供者）
    - Layer 2: Direct Map（VA 稳定映射）
    - Layer 3: VmAllocator 内部 bump（页内切割）
    - Layer 4: Rust GlobalAlloc trait（接口入口）
    - Layer 5: Box / Vec / String

A.3 未来方向（如需）
    - 如果 profiling 确认 bump 不够：可升级为 slab（为热点类型建专属缓存）
    - 如果 VM 变得多线程：再评估 jemalloc/mimalloc 等第三方分配器
    - 前提：第三方分配器需要 backend 对接 alloc_phys（而非 mmap）
```

删除附录 A 中现有的 jemalloc/mimalloc/snmalloc/tlsf 对比表和不推荐的详细分析。
保留 A.0 "接入指南"（`#[global_allocator]` 工作原理）——这部分教学价值高。

---

### E. §3.0 L1515："高频/极高频率分配"表述

已在之前的 P2-7 修复中处理完毕，确认无遗漏。

---

## 05-vm-allocpage.md

### A. §4.3 L715-728：初始化时序中的 T5 "relocate_to_heap"

**当前**：
```
├── T5: relocate_to_heap()  [独立文档详述]
│       → 搬迁预留区域数据到堆
│       → 释放预留区域物理页回 PhysAllocator
│
└── init_vm() 返回
        → VM 堆完全可用
```

**问题**："搬迁到堆"和"VM 堆完全可用"是 Minix3 的概念。Direct Map 下：
- T5 不再需要"搬迁"语义——VA 来自 Direct Map，不需要分配新 VA 和建立映射
- "VM 堆完全可用"应改为"VM 动态分配完全可用（bump allocator 就位）"

**修改**：
```
├── T5: relocate_to_heap() → phase2_direct_map_extend()  [09 文档详述]
│       → 如有必要，扩展 direct map 覆盖全部物理内存
│       → complete_bootstrap()
│           → bump allocator arena 分配（§4.1）
│           → 自此 Box/Vec 可用（bump allocator 接管 GlobalAlloc）
│
└── init_vm() 返回
        → VM 完全自治（全部物理内存纳入 direct map）
```

---

### B. §5 L837："参见"

**当前**：
```
- [09-vm-relocation.md](09-vm-relocation.md) - 数据搬迁（预留区域 → 堆）
```

**修改**：
```
- [09-vm-relocation.md](09-vm-relocation.md) - Direct Map 扩展与 VM 完全自治
```

---

## 04-physical-memory.md

### A. §7 L1916："堆就绪时机"对比表

**当前**：
```
| **堆就绪时机** | `pt_init()` 后 | `mem_init()` 前就需要预映射内存 |
```

**问题**：Rust 版本列说"mem_init() 前就需要预映射内存"，但这不是堆就绪时机的准确描述。
Direct Map 下，VM 的"堆"是 bump allocator 从 Direct Map 切分，不存在传统"堆就绪"时间点。

**修改**：
```
| **堆管理模式**     | `pt_init()` 后 brk 可用（VM 自身有堆） | Direct Map 消除 VM 堆概念：VA = phys + BASE，不需要 brk |
```

（列名从"堆就绪时机"改为"堆管理模式"）

---

### B. §10 L1975："参见"

**当前**：
```
- [05-vm-allocpage.md](05-vm-allocpage.md) - VM 堆初始化与保留页池
```

**修改**：
```
- [05-vm-allocpage.md](05-vm-allocpage.md) - VM 页分配器（alloc_phys + vm_phys_to_virt）
```

---

## 07-pagetable-ops.md

### A. §3.0 之后：新增一节 "VM 特殊化：为什么 VM 不再有传统堆"

**背景**：Direct Map 消除 VM 堆这个结论散落在 04/05/07 的多处。需要一个集中的地方讲清楚。

**建议插入位置**：§3.0 之后，§3.1 之前。

**新增内容要点**：
```
### 3.0.4 VM 特殊化：为什么 VM 不再有传统堆

Direct Map 的双视图模型带来了一个关键后果：VM 不再是"普通进程"。

**普通进程的堆**：虚拟地址空间中的连续增长区域，由 brk/sbrk 管理。
进程请求内存时，内核/VM 需要：找空闲 VA → 分配物理页 → 写页表建立映射。
整个过程涉及 VA 资源管理（find_hole）、页表修改（pt_writemap）、
TLB 刷新。

**VM 的"堆"**：Direct Map 建立后，每一个物理页自动拥有一个 stable VA
（`vm_phys_to_virt(phys) = VM_DIRECT_MAP_BASE + phys`）。
VM 拿物理页的瞬间，VA 就已经确定了。不需要 find_hole、不需要
vm_mappages、不需要为自己写页表。

因此：
- VM 不再需要 brk/sbrk
- VM 不再需要 vm_mappages（为自己）
- VM 的页表在 Direct Map 建立后变为只读不变量
- VM 的动态内存分配通过 bump/slab allocator 在 Direct Map 区域内完成

这符合 VM 的职责身份：VM 是 physical memory owner，不是 memory consumer。
物理页的持有者通过偏移直接访问（Direct Map），被管理者通过申请访问（brk/mmap）。
这不是"不一致"，而是职责差异的自然体现。
```

---

## 代码修改

### global.rs：实现 bump allocator

- 将 `pub(crate) struct VmAllocator;` 改为带字段的结构体（arena_base, cursor）
- `alloc()`：bump 逻辑（切分 → arena 不足时 refill）
- `dealloc()`：保留 no-op，加注释说明 bump 不回收单对象
- 添加 `const ARENA_PAGES: usize = 16`

---

## 修改优先级

| 优先级 | 位置 | 说明 |
|--------|------|------|
| P0 | 08 §4.1 | bump allocator 代码（文档和 global.rs 同步） |
| P0 | 08 附录 A | 降格重组 |
| P0 | 07 §3.0.4 | 新增 VM 特殊化说明 |
| P1 | 04 §7 L1916 | 堆管理模式表修正 |
| P1 | 04 §10 L1975 | 参见描述修正 |
| P1 | 05 §4.3 L715-728 | 初始化时序修正 |
| P1 | 05 §5 L837 | 参见描述修正 |
| P1 | 08 §1.2 L47 | 加 Direct Map 注释 |
| P1 | 08 §3.4 L1653 | 堆描述修正 |

---

## 2026-05-12 更新：双视图模型（Direct Map + HeapArena）

> 以下 TODO 基于对物理地址空间不连续性的重新认识。
> 之前的方案假设 Direct Map 区域可作为 bump allocator 的连续 arena，
> 这是错误的：`VA = PA + BASE`，物理不连续则 VA 不连续。
>
> 新方案引入 HeapArena：VM 预留一段连续 VA 区间，
> 将不连续的物理页逐页映射进去，从而为 Rust 堆提供虚拟连续性。
> Direct Map 仅用于物理页管理（页表操作、元数据访问、CoW 拷贝）。

### 概念模型

```
问题              机制           一句话定义
─────────────────────────────────────────────────
物理页可达性      Direct Map     任意物理页有稳定 VA，无需分配
虚拟连续性        HeapArena     预留连续 VA 区间，按需映射物理页
用户进程 VA 管理  VirRegion     进程地址空间的语义区域
```

### 依赖链（线性，无循环）

```
PhysAlloc (L1: 物理页提供者)
    ↓
Direct Map (L2: 物理页可达性，纯计算，无状态)
    ↓
HeapArena (L2': 虚拟连续性，三个整数，无堆依赖)
    ↓
VmAllocator (L3: bump/slab 在 Arena 内切分)
    ↓
GlobalAlloc (L4: Rust 堆接口)
    ↓
VirRegion / PhysRegion / PhysBlock (L5: 用户进程区域管理)
```

---

### TODO-D1: 新增 HeapArena 模块（代码）

**文件**: `os/servers/vm/src/heap_arena.rs`（新建）

**内容**:
- `HeapArena` 结构体：`base: VirBytes`, `limit: VirBytes`, `top: VirBytes`
- `grow(pages, page_alloc, pt) -> Result<()>`: 分配物理页 + 映射到 Arena 连续 VA
- `shrink(pages, page_alloc, pt)`: 解除映射 + 释放物理页
- `alloc_va(size, align) -> Option<VirBytes>`: 在 Arena 内分配连续 VA
- 不依赖 GlobalAlloc（三个 VirBytes 字段，可在栈/BSS 上构造）

**关键**: 物理页逐页分配（`alloc_phys(1)`），可以碎片化，但 VA 始终连续。

---

### TODO-D2: 新增 VM 自身页表映射接口（代码）

**文件**: `os/servers/vm/src/pagetable/mod.rs` 或新模块

**内容**:
- `vm_self_mappages(va, phys, flags)`: 在 VM 自身页表中建立映射
- `vm_self_unmappages(va, count)`: 在 VM 自身页表中解除映射
- 页表页通过 Direct Map 稳定可达，不会触发递归
- 需要获取 VM 自身页表的根指针（CR3 或 boot_info 提供）

---

### TODO-D3: 定义堆地址空间常量（代码）

**文件**: `os/arch/src/direct_map.rs`（DirectMapArch trait）

**内容**:
- `VM_HEAP_BASE`: 堆区域起始 VA（如 `VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE`）
- `VM_HEAP_SIZE`: 堆区域大小（如 64MB）
- x86_64 实现和 mock 实现均需添加

---

### TODO-D4: 重构 VmAllocator（代码）

**文件**: `os/servers/vm/src/global.rs`

**修改**:
- `refill_arena()`: 改用 `HeapArena::grow()` 映射物理页，而非 `vm_phys_to_virt()`
- `arena_base` 指向 HeapArena 区域的 VA，而非 Direct Map 区域
- `dealloc`: 仍可 no-op（bump 不回收单对象），但注释更新
- 移除 `use crate::direct_map::vm_phys_to_virt`（堆分配器不再依赖 Direct Map）

---

### TODO-D5: 08-slab §1 L47-49 修正（文档）

**当前**: "Rust 版本在 Direct Map 下不再需要 brk——VM 通过 `vm_phys_to_virt()` 直接访问物理页，无需为自身的堆扩展虚拟地址空间"

**问题**: 过于绝对。VM 仍需要连续 VA 的堆，只是不需要 Minix3 那套递归映射。

**修改方向**: 改为说明 Direct Map 解决"物理页可达性"，HeapArena 解决"虚拟连续性"，
两者各司其职。VM 不需要 brk/sbrk，但需要 HeapArena。

---

### TODO-D6: 08-slab §4.1 重写（文档）

**当前**: bump allocator 的 arena 基于 Direct Map（`vm_phys_to_virt(phys)`）

**问题**: Direct Map VA 不连续，bump cursor 无法跨越空洞。

**修改方向**:
- arena 改为基于 HeapArena（预留连续 VA 区间）
- `refill_arena()` 改为 `HeapArena::grow()`：逐页分配物理页，映射到 Arena 连续 VA
- 保留 bump 的简单性，但底层从 Direct Map 迁移到 HeapArena
- 代码示例中 `vm_phys_to_virt(phys)` 替换为 `HeapArena` 映射

---

### TODO-D7: 08-slab 附录 A 重写（文档）

**当前**: 层次描述为 `PhysAlloc → Direct Map → VmAllocator → GlobalAlloc → Box/Vec`

**问题**: 缺少 HeapArena 层，且"Direct Map 消除 VM 堆概念"的说法错误。

**修改方向**:
```
Layer 1: PhysAlloc / VmPageAllocator（页提供者）
Layer 2: Direct Map（物理页可达性，va=pa+BASE）
Layer 2': HeapArena（虚拟连续性，预留连续 VA + 逐页映射物理页）
Layer 3: VmAllocator 内部 bump（Arena 内切割）
Layer 4: Rust GlobalAlloc trait（接口入口）
Layer 5: Box / Vec / String
```

---

### TODO-D8: 07-pagetable-ops §3.0.4 修正（文档）

**当前**: "VM 不再有传统堆"、"VM 的页表在 Direct Map 建立后变为只读不变量"

**问题**:
1. VM 仍有堆（HeapArena），只是不需要 brk/sbrk
2. VM 需要为 HeapArena 写页表，页表不再是只读不变量

**修改方向**:
- "VM 不再有传统堆" → "VM 的堆不由 brk 驱动，而是由 HeapArena + vm_self_mappages 驱动"
- "页表只读不变量" → "页表修改仅限 HeapArena 区域，页表页通过 Direct Map 可达，无递归风险"
- 保留"VM 不需要 find_hole / vm_mappages（为自己）"的正确结论
- 保留"VM 是 physical memory owner"的身份论述

---

### TODO-D9: 04-physical-memory §7 L1916 修正（文档）

**当前**: "Direct Map 消除 VM 堆概念：VA = phys + BASE，不需要 brk"

**问题**: Direct Map 消除的不是"堆"，而是"为访问物理页而分配 VA"。

**修改方向**: 改为"Direct Map 消除 VM 对 brk 的依赖：物理页通过偏移直接可达，
堆的连续 VA 由 HeapArena 提供（独立于 Direct Map）"

---

### TODO-D10: 05-vm-allocpage §4.3 初始化时序修正（文档）

**当前**: T5 "relocate_to_heap()" → "搬迁预留区域数据到堆"

**问题**: Direct Map 下不需要"搬迁"语义。HeapArena 的初始化应在 T5 中描述。

**修改方向**:
```
T5: heap_arena_init()
    → HeapArena 初始化（预留连续 VA 区间）
    → 首批物理页映射到 HeapArena
    → bump allocator arena 就位
    → 自此 Box/Vec 可用
```

---

### TODO-D11: 08-slab §3.4 L1653 修正（文档）

**当前**: "VM 自身的堆内存分配不触发内核 IPC"

**问题**: HeapArena 需要写页表（vm_self_mappages），但这不是内核 IPC，
而是 VM 修改自己的页表。

**修改方向**: 改为"VM 自身的动态内存分配不触发内核 IPC。
HeapArena 扩展时通过 vm_self_mappages 修改 VM 自身页表，
但页表页通过 Direct Map 可达，无递归风险。
alloc_phys → HeapArena::grow → vm_self_mappages → 页内切分 → 返回指针。"

---

### TODO-D12: 09-vm-relocation 重写（文档）

**当前**: 描述 BSS 静态数据搬迁到堆的过程

**问题**: Direct Map + HeapArena 下，搬迁语义完全不同：
- Direct Map 消灭了"为物理页找 VA"的需求
- HeapArena 的 grow 机制需要描述
- 不再有 BSS → 堆的搬迁

**修改方向**: 重写为"Direct Map 扩展与 HeapArena 初始化"，
描述 Phase 1（1GB direct map）→ Phase 2（扩展覆盖全部物理内存）→
HeapArena 初始化（预留 VA + 首批映射）→ bump allocator 就位

---

### TODO-D13: 12-vir-region 明确层次关系（文档）

**修改方向**: 明确 VirRegion 依赖 GlobalAlloc（L5），
与 HeapArena（L2'）是不同层次的概念。
VirRegion 的 Vec<PhysRegion> 等字段通过 GlobalAlloc 分配，
底层经过 HeapArena 获得连续 VA。

---

### TODO-D14: 27-vm-init-main 更新初始化时序（文档）

**修改方向**: 初始化时序更新为 5 层线性启动：
```
L0: 静态/BSS（boot_info 解析）
L1: PhysAlloc（物理页分配器，metadata 从 boot_info 物理内存分配）
L2: Direct Map（物理页可达性，内核已建立）
L2': HeapArena（虚拟连续性，VM 初始化时建立）
L3: VmAllocator + GlobalAlloc（bump allocator 就位）
L4: VirRegion / PhysRegion / PhysBlock（用户进程区域管理）
```

---

### TODO-D15: direct_map.rs 测试更新（代码）

**文件**: `os/servers/vm/src/direct_map.rs`

**修改**: mock_map 的 offset 机制需要适配 HeapArena。
测试中 `set_mock_phys_base()` 设置 Direct Map 偏移，
HeapArena 需要独立的 mock 机制。

---

### TODO-D16: vm_server.rs 初始化更新（代码）

**文件**: `os/servers/vm/src/vm_server.rs`

**修改**: `VmServer::new()` 中添加 HeapArena 初始化逻辑。
HeapArena 需要访问 VM 自身页表，可能需要 boot_info 提供页表根指针。

---

### TODO-D17: 08-slab §2.0 物理页获取链路更新（文档）

**当前**: `slaballoc → newslabdata → vm_allocpage → vm_mappages`

**问题**: Direct Map 下不再有 `vm_mappages`。

**修改方向**: 更新为 Direct Map 下的链路：
`slaballoc → newslabdata → alloc_phys → vm_phys_to_virt`（Minix3 旧链路）
→ Rust 版本：`Box::new → GlobalAlloc → VmAllocator → HeapArena::grow → alloc_phys + vm_self_mappages`

---

### TODO-D18: 05-vm-allocpage §2.4 方案四分析限定（文档）

**当前**: "Direct Map 消灭了 VA 分配"

**问题**: Direct Map 消灭的是"为访问物理页而分配 VA"，不是"所有 VA 分配"。

**修改方向**: 限定为"Direct Map 消灭了为访问物理页而进行的 VA 分配（find_hole + vm_mappages）。
堆的连续 VA 分配由 HeapArena 承担，这是不同层次的问题。"

---

### TODO-D19: 10-27 文档审查与调整（文档）

**范围**: 10-phys-block.md, 14-phys-region.md, 18-vm-brk.md,
22-vm-brk-complete.md, 25-client-alloc-lib.md 等 09-27 全部文档

**修改方向**:
- 删除或重写基于"VM 不需要堆"假设的段落
- 更新 SLABALLOC → Box::new 的对应关系
- 更新初始化时序引用
- 明确 HeapArena 与 Direct Map 的职责边界
- 09（含）之后的文档可大幅调整甚至删除重建

---

### 修改优先级

| 优先级 | TODO | 说明 |
|--------|------|------|
| P0 | D1 | HeapArena 模块实现（基建） |
| P0 | D2 | VM 自身页表映射接口（基建） |
| P0 | D3 | 堆地址空间常量（基建） |
| P0 | D4 | VmAllocator 重构（核心代码） |
| P0 | D6 | 08 §4.1 重写（核心文档） |
| P0 | D8 | 07 §3.0.4 修正（概念纠正） |
| P1 | D5 | 08 §1 L47 修正 |
| P1 | D7 | 08 附录 A 重写 |
| P1 | D9 | 04 §7 修正 |
| P1 | D10 | 05 §4.3 修正 |
| P1 | D11 | 08 §3.4 修正 |
| P1 | D12 | 09 重写 |
| P1 | D17 | 08 §2.0 链路更新 |
| P1 | D18 | 05 §2.4 限定 |
| P2 | D13 | 12 层次关系明确 |
| P2 | D14 | 27 初始化时序更新 |
| P2 | D15 | 测试更新 |
| P2 | D16 | vm_server 初始化更新 |
| P2 | D19 | 10-27 文档审查调整 |
