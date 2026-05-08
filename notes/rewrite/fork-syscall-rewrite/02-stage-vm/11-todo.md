# 11-memtype.md Review Todo

## Document Fixes
- §3.2 `is_writable` 伪代码: `(*parent).remaps` → `(*parent.as_ptr()).remaps`（NonNull API 适配）

## Rust Code Fixes (Document/Minix3 → Code 对齐)

### memtype.rs
| 方法 | 旧实现 | 新实现 | 原因 |
|------|--------|--------|------|
| `AnonymousMemory::region_id` | 返回硬编码 `1` | 返回 `region.id as u32` | Minix3 `anon_regionid` 返回 `region->id`，文档§4.2一致 |
| `AnonymousMemory::ref_count` | 计数 mapped physblocks | 返回 `1 + region.remaps` | Minix3 `anon_refcount` 返回 `1 + vr->remaps`，文档§4.2一致 |

## 未实现的设计（文档§4.2/§5，标记为"尚未实现"）
- ContiguousAnonymous（连续匿名内存）
- CacheMemory（磁盘缓存）
- MappedFile（文件映射）
- PagefaultResult::NeedAsyncIo / Suspended 变体
- MemTypeError::AccessViolation / RegionNotFound / ProcessNotFound 变体
- 类型注册表（MEM_TYPE_REGISTRY）
- LazyLock<Arc<...>> 全局实例（当前使用简单 static，no_std 兼容）

这些属于后续阶段实现，当前审查不修改。

## Ground Truth 验证
- Minix3 `anon_regionid`: `return region->id` ✓（已对齐）
- Minix3 `anon_refcount`: `return 1 + vr->remaps` ✓（已对齐）
- Minix3 `anon_writable`: 检查 remaps 和 refcount ✓（已对齐）
