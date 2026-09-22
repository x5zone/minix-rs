# servers/vm/pt.h 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/pt.h`
> **核心功能**: 页表结构定义

---

## 文件概述

这个头文件定义了页表结构。

**核心概念**: 页目录，页表，虚拟地址空间。

---

## 逐行讲解

### pt_t 结构

```c
typedef struct {
	u32_t *pt_dir;		/* page aligned */
	u32_t pt_dir_phys;	/* physical address */

	u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];

	u32_t pt_virtop;
} pt_t;
```

**讲解**:
- **pt_dir**: 页目录虚拟地址
- **pt_dir_phys**: 页目录物理地址
- **pt_pt**: 页表指针数组
- **pt_virtop**: 虚拟地址空间顶部提示

---

## 要点总结

1. **页目录**: 一级页表
2. **页表**: 二级页表
3. **地址空间**: 每个进程独立

---

## 互动自测

1. **问题**: 为什么需要 pt_dir_phys？
   **答案**: 加载 CR3 寄存器需要物理地址。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **页表结构** | `pt_t` 结构体 | `pgd_t`/`pmd_t` | Minix3 更简单，Linux 支持更多级别 |
| **页表管理** | 二级页表 | 四级页表（PAE） | Linux 支持更大地址空间 |

---

### Rust 重构建议

```rust
pub struct PageTable {
    dir: Box<[u32]>,           // 页目录
    dir_phys: PhysAddr,        // 页目录物理地址
    tables: Vec<Box<[u32]>>,  // 页表数组
    virt_top: VAddr,           // 虚拟地址顶部
}
```
