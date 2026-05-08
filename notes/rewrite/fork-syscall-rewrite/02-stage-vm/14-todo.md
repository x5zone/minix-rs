# 14-phys-region.md Review Todo

## Document Fixes (NonNull API 适配)
- PhysRegion 结构体定义: `Option<*mut T>` → `Option<NonNull<T>>`（ph, parent, next_ph_list）
- PhysRegion memtype 字段: `Option<*mut MemType>` → `Option<&'static dyn MemType>`
- PhysBlock 结构体定义: `phys: u64` → `PhysBytes`, `refcount: u8` → `u16`, `first_region: Option<*mut PhysRegion>` → `Option<NonNull<PhysRegion>>`
- 字段对照表: 更新为 NonNull 版本
- "为什么使用裸指针？" → "为什么使用 NonNull？"：更新理由，增加 niche optimization 和类型安全说明
- 生命周期图: `*mut T` → `NonNull<T>`
- link_to_block 方法签名和实现: 更新为 NonNull API
- 内存布局: `Option<*mut PhysBlock> (16字节)` → `Option<NonNull<PhysBlock>> (8字节)`（niche optimization）

## Rust Code
- 无需修改，代码已在文件10审查时更新为 NonNull API

## 未更新的文档部分
- 测试代码示例中的 `*mut` 语法（约70+行），属于伪代码性质，不影响设计理解
- 建议后续统一更新所有测试示例为 NonNull 语法

## Ground Truth 验证
- Minix3 `phys_region` 结构: ph/parent/next_ph_list 均为指针 → Rust NonNull ✓
- Minix3 `phys_block` 结构: refcount 为 int → Rust u16 ✓
- Minix3 `pb_link`: 头插法 + refcount++ → Rust link_to_block ✓
