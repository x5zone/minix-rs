# servers/vm/pagetable.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/pagetable.c`
> **核心功能**: 页表管理

---

## 文件概述

这个文件实现了页表的管理操作。

**核心概念**: 页目录，页表，映射管理。

---

## 逐行讲解

### 全局变量

```c
static struct pdm {
	int		pdeno;
	u32_t		val;
	phys_bytes	phys;
	u32_t		*page_directories;
} pagedir_mappings[MAX_PAGEDIR_PDES];

struct vmproc *vmprocess = &vmproc[VM_PROC_NR];
```

**讲解**:
- **pagedir_mappings**: 页目录映射
- **vmprocess**: VM 进程自身的信息

---

### 备用页

```c
#if SANITYCHECKS
#define SPAREPAGES 200
#else
#define SPAREPAGES 20
#endif
```

**讲解**:
- 备用页用于紧急情况
- 调试模式需要更多备用页

---

### 大页支持

```c
static int bigpage_ok = 1;
```

**讲解**:
- 支持大页（4MB）
- 提高效率

---

## 要点总结

1. **页目录**: 每个进程一个
2. **备用页**: 紧急情况使用
3. **大页**: 提高效率

---

## 互动自测

1. **问题**: 为什么需要备用页？
   **答案**: 防止内存不足时无法分配页表，导致死锁。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **备用页** | 固定数量（20-200） | 动态分配 | Minix3 更保守，Linux 更灵活 |
| **大页支持** | 可选（`bigpage_ok`） | 默认启用 | Linux 更激进 |

---

### Rust 重构建议

```rust
pub const SPARE_PAGES: usize = if cfg!(debug_assertions) { 200 } else { 20 };

pub struct PageDirMapping {
    pde_no: usize,
    val: u32,
    phys: PhysAddr,
    page_directories: Box<[u32]>,
}

pub struct PageTableManager {
    mappings: Vec<PageDirMapping>,
    spare_pages: Vec<PhysPage>,
    bigpage_enabled: bool,
}
```
