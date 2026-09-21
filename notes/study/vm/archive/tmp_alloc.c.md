# servers/vm/alloc.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/alloc.c`
> **核心功能**: 物理内存分配

---

## 文件概述

这个文件实现了物理内存的分配和释放。

**核心概念**: 物理页管理，位图分配，预留内存。

---

## 逐行讲解

### 页位图

```c
#define NUMBER_PHYSICAL_PAGES (int)(0x100000000ULL/VM_PAGE_SIZE)
#define PAGE_BITMAP_CHUNKS BITMAP_CHUNKS(NUMBER_PHYSICAL_PAGES)
static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS];
```

**讲解**:
- 位图跟踪空闲页
- 每位代表一个物理页
- 32 位地址空间有 1M 个页

---

### 页缓存

```c
#define PAGE_CACHE_MAX 10000
static int free_page_cache[PAGE_CACHE_MAX];
static int free_page_cache_size = 0;
```

**讲解**:
- 缓存最近释放的页
- 加速分配

---

### 预留内存

```c
#define MAXRESERVEDPAGES	300
#define MAXRESERVEDQUEUES	 15

static struct reserved_pages {
	struct reserved_pages *next;
	int max_available;
	int npages;
	int mappedin;
	int n_available;
	int allocflags;
	struct reserved_pageslot {
		phys_bytes	phys;
		void		*vir;
	} slots[MAXRESERVEDPAGES];
	u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES];
```

**讲解**:
- 预留连续物理内存
- 用于 DMA 等特殊需求
- **mappedin**: 是否已映射

---

## 要点总结

1. **位图分配**: 跟踪空闲页
2. **页缓存**: 加速分配
3. **预留内存**: 满足特殊需求

---

## 互动自测

1. **问题**: 为什么需要预留内存？
   **答案**: DMA 需要物理连续内存，必须预留。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **内存分配** | VM 服务器管理 | 伙伴系统 + Slab | Linux 更复杂 |
| **分配策略** | 简单的位图 | 伙伴系统 | Linux 更高效 |

---

### Rust 重构建议

```rust
pub struct PageAllocator {
    free_pages: Vec<PhysPage>,
    total_pages: usize,
}

impl PageAllocator {
    pub fn alloc(&mut self, count: usize) -> Result<PhysPage, VmError> {
        if self.free_pages.len() < count {
            return Err(VmError::OutOfMemory);
        }
        
        let page = self.free_pages.pop().ok_or(VmError::OutOfMemory)?;
        Ok(page)
    }
    
    pub fn free(&mut self, page: PhysPage) {
        self.free_pages.push(page);
    }
}
```
