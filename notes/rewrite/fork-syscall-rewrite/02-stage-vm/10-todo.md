# 10-phys-block.md Review Todo

## Document Review
- 10-phys-block.md 文档本身无需修改，设计描述与 Minix3 源码一致

## Rust Code Fixes (Document → Code 对齐)

### phys_region.rs — 重大重写
按照文档设计规范，修正了以下偏离：

| 字段/方法 | 旧实现 | 新实现 | 原因 |
|-----------|--------|--------|------|
| `PhysBlock.phys` | `u64` | `PhysBytes` | 文档设计使用类型安全的 PhysBytes |
| `PhysBlock.refcount` | `u8` | `u16` | 文档设计为 u16，u8 可能溢出 |
| `PhysBlock.first_region` | `Option<*mut PhysRegion>` | `Option<NonNull<PhysRegion>>` | NonNull 提供类型安全保证 |
| `PhysBlock::MAP_NONE` | `u64` 常量 | `PhysBytes` 常量 | 与 phys 字段类型一致 |
| `PhysRegion.ph` | `Option<*mut PhysBlock>` | `Option<NonNull<PhysBlock>>` | NonNull 替代裸指针 |
| `PhysRegion.parent` | `Option<*mut VirRegion>` | `Option<NonNull<VirRegion>>` | NonNull 替代裸指针 |
| `PhysRegion.next_ph_list` | `Option<*mut PhysRegion>` | `Option<NonNull<PhysRegion>>` | NonNull 替代裸指针 |
| `bind_block()` 参数 | `*mut PhysBlock` | `NonNull<PhysBlock>` | 签名与字段类型一致 |
| `link_to_block()` 参数 | `(*mut PhysBlock, *mut VirRegion, ...)` | `(NonNull<PhysBlock>, NonNull<VirRegion>, ...)` | 签名与字段类型一致 |
| `get_phys_addr()` 返回 | `Option<u64>` | `Option<PhysBytes>` | 与 PhysBlock.phys 类型一致 |
| `get_refcount()` 返回 | `Option<u8>` | `Option<u16>` | 与 PhysBlock.refcount 类型一致 |
| 所有方法体 | 直接 `(*ptr)` 解引用 | `(*ptr.as_ptr())` 解引用 | NonNull 需通过 as_ptr() 获取裸指针 |
| 所有测试 | 使用裸指针字面量 | 使用 `NonNull::from()`/`NonNull::new()` | 适配新 API |

### memtype.rs — 级联修复
- 添加 `PhysBytes` 导入
- `AnonymousMemory::is_writable()`: `(*parent).remaps` → `(*parent.as_ptr()).remaps`（NonNull 解引用）
- `get_phys_addr()` 返回 `Option<PhysBytes>`，与 `PhysBlock::MAP_NONE` 类型比较保持正确

### vir_region.rs — 级联修复
- 添加 `PhysBytes` 导入
- `VrParam::Direct { phys: u64 }` → `Direct { phys: PhysBytes }`
- `prepare_cow()`: `unwrap_or(0)` → `unwrap_or(PhysBlock::MAP_NONE)`（类型安全）

### fork.rs — 级联修复
- `link_phys_blocks()`: `(*block_ptr).add_ref()` → `(*block_ptr.as_ptr()).add_ref()`（NonNull 解引用）

### vmproc_handle.rs — 级联修复
- `setup_cow_for_all_regions()`: `(*block).add_ref()` → `(*block.as_ptr()).add_ref()`（NonNull 解引用）
- `write_page_table_mappings()`: `&*block_ptr` → `&*block_ptr.as_ptr()`（NonNull 解引用）

## Ground Truth 优先级验证
- Minix3 源码: PhysBlock 使用 `phys_bytes` 类型（u64 包装）、refcount 为 int
- 文档设计: PhysBytes/u16/NonNull — 作为设计规范
- Rust 实现: 已对齐文档设计 ✓
