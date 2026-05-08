# 04-todo: Review 修复记录

## 审查文件
`04-physical-memory.md`

## 审查结果

### P0 修复

#### 1. 行号错误：alloc.c:57-72 → alloc.c:33-41

**问题**: §2.0 核心数据结构引用 `alloc.c:57-72`，但实际代码中：
- `NUMBER_PHYSICAL_PAGES` 定义在行 33
- `free_pages_bitmap` 在行 35
- `PAGE_CACHE_MAX` 在行 36
- `free_page_cache` 在行 37
- `mem_low/mem_high` 在行 41

行 57-72 对应的是 `reservedqueues` 结构体定义，不是核心数据结构部分。

**修复**: 改为 `alloc.c:33-41`。

**验证**: `grep -n "NUMBER_PHYSICAL_PAGES\|free_pages_bitmap\|PAGE_CACHE_MAX\|free_page_cache\|mem_low" minix3/minix/servers/vm/alloc.c`。

### P1 修复

#### 2. EarlyHeap 代码与实际 Rust 实现不一致

**问题**: §4.2 中 `EarlyHeap` 使用 `*mut u8` 原始指针，但实际 Rust 代码使用 `NonNull<u8>`。此外：
- 文档中 `EarlyHeap::empty()` 构造，实际代码为 `EarlyHeap::new()`
- 文档中 `init()` 缺少 `assert` 检查，实际代码有 `assert!(size > 0)` 和 `assert!(!start.is_null())`
- 文档中 `alloc_slice()` 未处理 `count == 0`，实际代码有 `if count == 0 { return &mut []; }`
- 文档中 `alloc_aligned()` 使用 `self.current as usize`，实际代码使用 `self.current.as_ptr() as usize`

**修复**: 更新 §4.2 代码为与实际 Rust 代码一致的 `NonNull<u8>` 版本，补充 `new()`、`assert` 检查、零计数处理。同时更新 §4.3 使用示例（`EarlyHeap::new()` + `used()/remaining()` 方法）。

**验证**: 对比 `os/servers/vm/src/phys_mem/early_heap.rs`。

#### 3. PhysAllocType::SegmentTree 的 metadata_size_exact 使用 SegmentNode 而非元组

**问题**: §5.7 中 `PhysAllocType::SegmentTree` 的 `metadata_size_exact` 使用 `size_of::<SegmentNode>()`，但实际 Rust 代码使用 `size_of::<(usize, usize, usize, usize)>()`。代码中 `SegmentNode` 是 `(usize, usize, usize, usize)` 的类型别名（在 `segment_tree_alloc.rs` 中 `#[cfg(feature)]` 保护下），而 `mod.rs` 中的 `metadata_size_exact` 不能引用 feature-gated 类型，因此使用元组。

**修复**: 改为 `size_of::<(usize, usize, usize, usize)>()` 并添加注释说明 SegmentNode 字段含义。

**验证**: 对比 `os/servers/vm/src/phys_mem/mod.rs:105-113`。

### 源码行号验证

| 引用 | 文档标注 | 实际位置 | 一致? |
|------|----------|----------|-------|
| `alloc.c:242` alloc_mem | L242 | L242=函数签名 | ✅ |
| `alloc.c:289` free_mem | L289 | L289=函数签名 | ✅ |
| `alloc.c:348` memstats | L348 | L348=函数签名 | ✅ |
| `alloc.c:33-41` 核心数据结构 | L33-41 | ✅ (已修复) | ✅ |
| `vm.h:22-27` PAF 标志 | L22-27 | L22=PAF_CLEAR, L27=PAF_ALIGN16K | ✅ |
| `pagetable.c:1151` pt_init | L1151 | L1151=reservedqueue_new | ✅ |
| `pagetable.c:264` vm_getsparepage | L264 | L264=函数签名 | ✅ |

### Rust 代码审查

Rust 代码与文档设计基本一致，无需修改：

- `PhysAllocator` / `PhysAllocatorStats` trait 与 §3.0/§5.1 一致
- `PhysMemStats` 结构体与 §5.1 一致
- `PageAllocFlags` bitflags 值与 §6.2 一致（0x01/0x02/0x04/0x08/0x10/0x40）
- `AllocError` enum 与 §6.3 一致
- `PhysBytes` newtype 与 §6.1 一致
- `BitmapAllocator` 结构体与 §5.3 一致
- `BuddyAllocator` SoA 结构与 §5.4 一致
- `PhysAllocType` enum 与 §5.7 一致
- `EarlyHeap` 实现与 §4.2 一致（已修复文档）
- `BootMemRegion` 与 §3.0 一致
- `CLICK_SIZE`/`CLICK_SHIFT` 常量与 §1.2 一致
- `PhysAllocator` trait 额外有 `reloc_array_count/info/update` 方法（用于搬迁），文档 §4.4 提及了搬迁机制但未详述这些方法，属于合理简化

### 上一轮 review 修复验证

上一轮 04-todo.md 记录了 2 个修复：
1. ✅ P0: §1.4 已移除 Rust 实现状态说明，替换为交叉引用
2. ✅ P0: §2.3.5 已移除 Rust 实现状态说明，替换为交叉引用

### 未修复项（P2，记录备查）

1. 文档 §2.1.1 的 `alloc_mem()` 和 `alloc_pages()` 代码块是伪代码（简化了实际实现），这是合理的简化
2. 文档 §2.1.2 的 `findbit()` 代码块也是伪代码，简化了 chunk 跳过优化的细节
3. `PhysAllocator` trait 中的 `reloc_array_count/info/update` 方法未在文档中详述，但文档 §4.4 提及了搬迁机制
4. `BitmapAllocator` 实际代码有 `MemStats` 字段和 `stats` 模块，文档 §5.3 未提及，但 §8 文件结构中列出了 `stats.rs`
5. 文档 §5.3 中 `PAF_CLEAR` 标注为 TODO，实际代码中也是 TODO（需内核 IPC sys_memset），一致
