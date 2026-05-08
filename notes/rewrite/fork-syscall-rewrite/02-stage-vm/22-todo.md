# 22-todo: Review 修复记录

## 审查文件
`22-vm-brk-complete.md`

## 审查结果

### 发现 P1 Bug：free_region_pages 未执行页表 unmap

#### Bug 描述

原始 `free_region_pages()` 函数只释放物理页到分配器，从未取消页表映射：

```rust
fn free_region_pages(region: &VirRegion, page_alloc: &mut VmPageAllocator) {
    for phys_opt in &region.physblocks {
        if let Some(pr) = phys_opt {
            if let Some(phys) = pr.get_phys_addr() {
                page_alloc.free_page(PmPhysBytes::new(phys.0)); // BUG: 未 unmap 页表
            }
        }
    }
}
```

这导致：
1. 页表仍保留指向已释放物理页的映射
2. 后续分配可能重用该物理页
3. 进程可能访问到错误的物理页（安全漏洞）

#### 修复内容

按照文档 22-vm-brk-complete.md 的设计：

1. **修改函数签名**：接收 `&mut PageTable` 参数
2. **添加页表 unmap**：在释放物理页前，先取消所有页表映射
3. **更新调用点**：在 shrink_heap 中正确获取和使用 page_table

### 修复文件
- `src/brk.rs` - free_region_pages() 函数和 shrink_heap()

### 验证
- `cargo check` 通过
