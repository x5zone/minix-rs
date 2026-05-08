# 12-vir-region.md Review Todo

## Document Fixes
- VrParam::Direct 字段类型: `phys: u64` → `phys: PhysBytes`（与 Rust 代码对齐，类型安全）

## Rust Code Fixes (Document/Minix3 → Code 对齐)

### vir_region.rs
| 方法 | 修复 | 原因 |
|------|------|------|
| `split()` | 添加 `split_len == 0` 检查 | Minix3 调用者有 `assert(split_len > 0)`，零长度分割无意义 |

## 已知设计差异（Allowed Evolution，不修改）
- `parent` 字段: C 用 `struct vmproc*`，Rust 用 `Option<UserSlot>`（索引替代指针，类型安全）
- `physblocks`: C 用 `struct phys_region**`，Rust 用 `Vec<Option<Box<PhysRegion>>>`（Rust 惯用方式）
- `VrParam::File` 缺少 `fdref` 字段（文件映射尚未完整实现）
- `prepare_cow()` 为占位实现（页表操作待后续阶段）
- `split()` 不内联调用 `on_split` 回调（由调用者负责，与 Minix3 分离设计不同）

## Ground Truth 验证
- Minix3 `split_region`: `assert(split_len > 0)` ✓（已对齐）
- Minix3 `split_region`: `assert(!(split_len % VM_PAGE_SIZE))` ✓（已有检查）
