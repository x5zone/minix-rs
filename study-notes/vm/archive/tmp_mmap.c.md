# servers/vm/mmap.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/mmap.c`
> **核心功能**: 内存映射实现

---

## 文件概述

这个文件实现了 mmap 系统调用。

**核心概念**: 内存映射，区域创建，地址分配。

---

## 逐行讲解

### mmap_region 函数

```c
static struct vir_region *mmap_region(struct vmproc *vmp, vir_bytes addr,
	u32_t vmm_flags, size_t len, u32_t vrflags,
	mem_type_t *mt, int execpriv)
{
	u32_t mfflags = 0;
	struct vir_region *vr = NULL;

	if(vmm_flags & MAP_LOWER16M) vrflags |= VR_LOWER16MB;
	if(vmm_flags & MAP_LOWER1M)  vrflags |= VR_LOWER1MB;
	if(vmm_flags & MAP_ALIGNMENT_64KB) vrflags |= VR_PHYS64K;
	if(vmm_flags & MAP_PREALLOC) mfflags |= MF_PREALLOC;
```

**讲解**:
- **MAP_LOWER16M**: 映射到 16MB 以下
- **MAP_PREALLOC**: 预分配
- 设置区域标志

---

### 地址和长度处理

```c
	if(len <= 0) {
		return NULL;
	}

	if(len % VM_PAGE_SIZE)
		len += VM_PAGE_SIZE - (len % VM_PAGE_SIZE);
```

**讲解**:
- 长度必须大于 0
- 对齐到页大小

---

## 要点总结

1. **mmap**: 内存映射系统调用
2. **区域标志**: 控制映射行为
3. **地址对齐**: 页对齐

---

## 互动自测

1. **问题**: MAP_FIXED 的作用？
   **答案**: 强制映射到指定地址，如果冲突则失败。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **mmap 实现** | VM 服务器 | 内核系统调用 | Minix3 隔离性好，Linux 性能高 |
| **映射标志** | `VR_*` 标志 | `VM_*` 标志 | 类似的设计 |
| **地址对齐** | 页对齐 | 页对齐 | 相同要求 |

---

### Rust 重构建议

```rust
bitflags::bitflags! {
    pub struct MmapFlags: i32 {
        const SHARED = 0x01;
        const PRIVATE = 0x02;
        const FIXED = 0x10;
        const ANONYMOUS = 0x20;
    }
}

pub fn mmap(
    addr: Option<VAddr>,
    length: usize,
    prot: ProtFlags,
    flags: MmapFlags,
    fd: Option<i32>,
    offset: u64,
) -> Result<VAddr, VmError> {
    // 检查参数
    if length == 0 {
        return Err(VmError::InvalidLength);
    }
    
    // 查找可用地址
    let vaddr = if flags.contains(MmapFlags::FIXED) {
        addr.ok_or(VmError::InvalidAddress)?
    } else {
        find_free_region(length)?
    };
    
    // 创建虚拟区域
    let region = VirRegion::new(vaddr, length, prot, flags)?;
    
    Ok(vaddr)
}
```
