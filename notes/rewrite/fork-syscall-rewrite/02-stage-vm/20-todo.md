# 20-todo: Review 修复记录

## 审查文件
`20-cow-exec-pagefault.md`

## 审查结果

### 发现 P0 Bug：handle_pagefault 未分配页/执行 CoW

#### Bug 描述

原始 `handle_pagefault()` 函数存在两个严重缺陷：

1. **未映射页面未分配物理页** (L82-85)
   ```rust
   if is_unmapped {
       active.inc_minor_fault();
       return Ok(()); // BUG: 页从未分配，进程继续访问无效地址
   }
   ```

2. **CoW 页面未执行复制** (L87-93)
   ```rust
   if needs_cow && fault.write {
       if !region.is_writable() {
           return Err(PageFaultError::AccessViolation);
       }
       active.inc_major_fault();
       return Ok(()); // BUG: CoW 复制从未执行
   }
   ```

#### 修复内容

按照文档 20-cow-exec-pagefault.md §4 的设计，完整实现了：

1. **未映射页面处理**：
   - 调用 `page_alloc.alloc_page()` 分配新物理页
   - 创建 `PhysRegion` 并绑定到 `PhysBlock`
   - 调用 `page_table.map()` 建立页表映射
   - 设置正确的可写/只读标志

2. **CoW 执行处理**：
   - 获取旧物理页地址
   - 分配新物理页
   - 使用 `vm_phys_to_virt()` + `copy_nonoverlapping()` 复制页面内容
   - 解绑旧 `PhysBlock`，绑定新 `PhysBlock`
   - 更新页表映射为可写

### 修复文件
- `src/cow_exec_pf.rs` - handle_pagefault() 函数

### 验证
- `cargo check` 通过
