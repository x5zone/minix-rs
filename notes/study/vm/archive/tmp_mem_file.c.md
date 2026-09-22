# servers/vm/mem_file.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/mem_file.c`
> **核心功能**: 文件映射内存类型实现

---

## 文件概述

这个文件实现了文件映射内存类型的方法。

**核心概念**: 文件映射，mmap，页面缓存。

---

## 逐行讲解

### 内存类型结构

```c
struct mem_type mem_type_mappedfile = {
	.name = "file-mapped memory",
	.ev_unreference = mappedfile_unreference,
	.ev_pagefault = mappedfile_pagefault,
	.ev_sanitycheck = mappedfile_sanitycheck,
	.ev_copy = mappedfile_copy,
	.writable = mappedfile_writable,
	.ev_split = mappedfile_split,
	.ev_lowshrink = mappedfile_lowshrink,
	.ev_delete = mappedfile_delete,
	.pt_flags = mappedfile_pt_flags,
};
```

**讲解**:
- 定义文件映射内存类型的操作
- 与匿名内存类似但有关键区别
- 页错误时从文件读取

---

### 页表标志

```c
static int mappedfile_pt_flags(struct vir_region *vr){
#if defined(__arm__)
	return ARM_VM_PTE_CACHED;
#else
	return 0;
#endif
}
```

**讲解**:
- ARM 平台使用缓存
- x86 平台无特殊标志

---

## 要点总结

1. **文件映射**: mmap 实现
2. **页错误**: 从文件读取
3. **写回**: 脏页写回文件

---

## 互动自测

1. **问题**: 文件映射的特点？
   **答案**: 有后备存储（文件），修改可写回文件。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **文件映射** | `mem_type_file` | `file_vm_ops` | 类似的设计 |
| **缺页处理** | 从文件读取 | `filemap_fault` | 类似的逻辑 |

---

### Rust 重构建议

```rust
pub struct FileMem {
    file: Arc<File>,
    offset: u64,
}

impl MemType for FileMem {
    fn pagefault(&self, region: &VirRegion, offset: usize) -> Result<(), VmError> {
        // 分配物理页
        let page = alloc_page()?;
        
        // 从文件读取数据
        self.file.read_at(self.offset + offset as u64, page.as_mut_slice())?;
        
        // 创建物理块
        let pb = PhysBlock::new(page);
        
        // 链接到虚拟区域
        region.link_phys_block(offset, pb)?;
        
        Ok(())
    }
    
    fn writable(&self, region: &VirRegion, offset: usize) -> bool {
        // 检查文件是否可写
        self.file.is_writable()
    }
}
```
