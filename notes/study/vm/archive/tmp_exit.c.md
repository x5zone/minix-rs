# servers/vm/exit.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/exit.c`
> **核心功能**: 进程退出内存清理

---

## 文件概述

这个文件实现了进程退出时的内存清理。

**核心概念**: 内存释放，资源清理，引用计数。

---

## 逐行讲解

### 清理进程

```c
void free_proc(struct vmproc *vmp)
{
	region_init(&vmp->vm_regions_avl);
	...
}

void clear_proc(struct vmproc *vmp)
{
	acl_clear(vmp);
	vmp->vm_flags &= ~VMF_INUSE;
	...
}
```

**讲解**:
- **free_proc**: 释放进程内存
- **clear_proc**: 清理进程信息
- 清除 ACL、标志等

---

## 要点总结

1. **内存释放**: 释放进程占用的内存
2. **资源清理**: 清理各种资源
3. **标志清除**: 标记槽位为空闲

---

## 互动自测

1. **问题**: 进程退出时需要释放哪些资源？
   **答案**: 内存区域、页表、共享内存引用等。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **exit 实现** | VM 服务器 | 内核系统调用 | Minix3 隔离性好，Linux 性能高 |
| **资源释放** | `map_free_proc` | `exit_mm` | 类似的逻辑 |

---

### Rust 重构建议

```rust
impl Drop for VmProc {
    fn drop(&mut self) {
        // 自动释放所有内存区域
        for (_, region) in self.regions.drain() {
            drop(region);
        }
        // 释放页表
        drop(self.page_table);
    }
}
```
