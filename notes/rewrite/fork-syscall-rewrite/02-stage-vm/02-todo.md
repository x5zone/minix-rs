# 02-vmproc-table.md Review 修改记录

## 修改概要

对 02-vmproc-table.md 进行深度 review，修复 P0/P1 问题。

---

## P0 修复

### 1. Ch2 §2.2 包含 Rust 内容

**问题**: §2.2 "显式生命周期控制" 包含 Rust 实现细节：
- "Rust Drop 由作用域触发"
- "VmProc 的 Drop 为空操作（debug_assert 检查状态）"
- "预初始化所有槽位为 vacant() 状态"
- "状态由 VmFlags 管理"

**修改**: 将 Rust 实现细节替换为 Minix3 的处理方式描述 + 指向 §5.1 的交叉引用。

### 2. Ch2 §2.3 包含 Rust 实现映射表

**问题**: §2.3 "与 Minix3 语义对齐" 的对比表第二列直接列出 Rust 实现类型（如 `AssumeSyncCell<VmProc>`、`VmFlags::IN_USE`、`proc.clear()`、`VmProc::vacant()`）。

**修改**: 将对比表改为纯语义描述（O(1) 索引、状态判断、显式重置、全零初始化），添加指向 §5.1 的交叉引用。

### 3. Ch3 §3.2.1 包含 "Rust 实现" 代码块

**问题**: §3.2.1 vm_isokendpt 分析中包含完整的 Rust 实现代码（`impl VmProcTable { pub fn vm_isokendpt(...) -> Option<UserSlot> }`）和设计差异讨论。

**修改**: 删除 Rust 代码块和设计差异讨论，替换为一句摘要 + 指向 §5.3 的交叉引用。

### 4. vm_isokendpt 文档代码与实际 Rust 代码不一致

**问题**: 文档中 `vm_isokendpt` 返回 `Option<UserSlot>`，但实际 Rust 代码（table.rs）返回 `Result<UserSlot, EndpointError>`，区分 `InvalidSlot`（EINVAL）和 `DeadEndpoint`（EDEADEPT）。文档的设计差异说明称"Rust 统一返回 None"，与实际代码矛盾。

**修改**: 更新交叉引用描述，准确反映 Rust 实现使用 `Result<UserSlot, EndpointError>` 区分两种错误。

---

## P1 修复

### 5. 缺少 "参见" 章节

**问题**: 文档最后一章是"附录"，缺少标准的"参见"章节。根据文档结构规范，最后一章应为参见。

**修改**: 在附录后添加 "8. 参见" 章节，引用 01-vmproc-struct.md、00-vm-overview.md、03-acl.md、06-pagetable-struct.md。

---

## 未修改项（记录备查）

- **Ch2 结构**: 当前 Ch2 为"设计目标与约束"而非标准结构的"C 源码分析"。内容上与 Ch1 有重叠，但未做大规模重构。
- **Ch4 技术选型分析**: 内容详实，选型论证充分，无需修改。
- **Ch5 Rust 实现设计**: 与实际 Rust 代码一致，无需修改。
- **附录A**: 地址稳定性论证完整，无需修改。
