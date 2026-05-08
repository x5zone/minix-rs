# 10-todo: Review 修复记录

## 审查文件
`10-phys-block.md`

## 审查结果

### 无需修复

文档质量高，方案四变更已描述。sys_abscopy → vm_phys_to_virt() + copy_nonoverlapping() 替换已在文档中标注。

### 验证通过项

1. **方案四变更**：文档已标注 sys_abscopy 替换为 vm_phys_to_virt() + copy_nonoverlapping()
2. **Rust 代码现状**：CoW 页面复制逻辑在 cow_exec_pf.rs 中尚未完全实现（只标记 major fault，未实际复制页面）
3. **PhysBlock/PhysRegion**：代码与文档一致，link_to_block/unlink_from_block 实现正确

### 未修复项（P2）

1. **CoW 页面复制未实现**：cow_exec_pf.rs 中 needs_cow && fault.write 路径只标记 major fault，未执行实际页面复制。方案四要求使用 vm_phys_to_virt() + copy_nonoverlapping()。
