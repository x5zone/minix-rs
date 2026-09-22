# servers/vm/cache.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/cache.c`
> **核心功能**: 文件系统缓存管理

---

## 文件概述

这个文件实现了文件系统缓存块的数据结构和管理。

**核心概念**: LRU 缓存，哈希查找，页面缓存。

---

## 逐行讲解

### 数据结构

```c
#define HASHSIZE 65536

static struct cached_page *cache_hash_bydev[HASHSIZE];
static struct cached_page *cache_hash_byino[HASHSIZE];
static struct cached_page *lru_oldest = NULL, *lru_newest = NULL;

static u32_t cached_pages = 0;
```

**讲解**:
- **cache_hash_bydev**: 按设备号哈希
- **cache_hash_byino**: 按 inode 号哈希
- **lru_oldest/newest**: LRU 链表头尾

---

### LRU 删除

```c
static void lru_rm(struct cached_page *hb)
{
	struct cached_page *newer = hb->newer, *older = hb->older;

	if(newer) newer->older = older;
	if(older) older->newer = newer;

	if(lru_newest == hb) lru_newest = older;
	if(lru_oldest == hb) lru_oldest = newer;

	cached_pages--;
}
```

**讲解**:
- 从 LRU 链表中删除页面
- 更新前后指针
- 减少计数

---

## 要点总结

1. **缓存**: 文件系统页面缓存
2. **LRU**: 最近最少使用替换
3. **哈希**: 快速查找

---

## 互动自测

1. **问题**: 缓存的作用？
   **答案**: 加速频繁访问的数据，减少磁盘 I/O。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **页缓存** | VM 服务器管理 | 页缓存 | 类似的设计 |
| **缓存策略** | 简单的 LRU | LRU + 工作集 | Linux 更复杂 |

---

### Rust 重构建议

```rust
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct PageCache {
    cache: LruCache<PhysAddr, Arc<[u8]>>,
    total_size: usize,
    max_size: usize,
}

impl PageCache {
    pub fn new(max_pages: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(max_pages).unwrap()),
            total_size: 0,
            max_size: max_pages * 4096,
        }
    }
    
    pub fn get(&mut self, addr: PhysAddr) -> Option<Arc<[u8]>> {
        self.cache.get(&addr).cloned()
    }
    
    pub fn insert(&mut self, addr: PhysAddr, data: Arc<[u8]>) {
        if self.cache.len() >= self.cache.cap().get() {
            self.cache.pop_lru();
        }
        self.cache.put(addr, data);
    }
}
```
