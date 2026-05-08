# 17-vm-fork.md Review Todo

## Document Review
- 文档无需修改

## Rust Code Fixes (Minix3 → Code 对齐)

### fork.rs — link_phys_blocks 修复
| 修复 | 旧实现 | 新实现 | 原因 |
|------|--------|--------|------|
| `link_phys_blocks` | 仅调用 `add_ref()` 增加引用计数 | 调用 `link_to_block()` 完整链入 PhysBlock 链表 | Minix3 `pb_link` 不仅增加 refcount，还设置 parent、next_ph_list、更新 first_region |
| 添加 `NonNull` 导入 | 无 | `use core::ptr::NonNull;` | `link_to_block` 需要 `NonNull<VirRegion>` 参数 |
| `parent_ptr` 传递 | 无 | `NonNull::from(&*region)` | 子进程 PhysRegion 的 parent 应指向子进程的 VirRegion |

**Bug 影响**：
- 旧代码：子进程 PhysRegion 的 `parent` 为 None，`next_ph_list` 为 None，不在 PhysBlock 的链表中
- 修复后：子进程 PhysRegion 正确链入，`parent` 指向子进程 VirRegion，`next_ph_list` 指向链表下一个节点
- 这修复了 CoW 遍历引用者和 unlink 操作的正确性

## 已知设计差异（Allowed Evolution，不修改）
- `clone_region_for_fork` 复制 `ph` 指针后由 `link_to_block` 覆盖设置（冗余但无害）
- `handle_memory_once` 等内核交互函数尚未实现

## Ground Truth 验证
- Minix3 `pb_link`: 设置 ph、parent、next_ph_list、first_region、refcount++ ✓（已对齐）
- Minix3 `map_copy_region`: 复制区域元数据 + pb_link 物理块 ✓（已对齐）
