# 23-todo: Review 修复记录

## 审查文件
`23-vm-munmap.md`

## 审查结果

### 发现 P1 Bug：free_region_pages 和 unmap_range 未执行页表 unmap

#### Bug 描述

原始 `munmap.rs` 存在两个问题：

1. **unmap_range 未调用页表 unmap**
   ```rust
   fn unmap_range(...) {
       // 只调用 free_region_pages()
       free_region_pages(&region, page_alloc); // BUG: 没有取消页表映射
   }
   ```

2. **free_region_pages 未执行页表 unmap**
   ```rust
   fn free_region_pages(region: &VirRegion, page_alloc: &mut VmPageAllocator) {
       for phys_opt in &region.physblocks {
           if let Some(pr) = phys_opt {
               if let Some(phys) = pr.get_phys_addr() {
                   page_alloc.free_page(...); // BUG: 未 unmap
               }
           }
       }
   }
   ```

#### 修复内容

按照文档 23-vm-munmap.md 的设计：

1. **修改 unmap_range 签名**：接收 `&mut PageTable` 参数
2. **修改 free_region_pages 签名**：接收 `&mut PageTable` 参数
3. **添加页表 unmap**：遍历区域中的所有页，调用 `page_table.unmap()`

### 修复文件
- `src/munmap.rs` - unmap_range() 和 free_region_pages() 函数

### 验证
- `cargo check` 通过
