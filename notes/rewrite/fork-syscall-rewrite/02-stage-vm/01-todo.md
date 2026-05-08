# 01-vmproc-struct.md Review 修改记录

## 修改概要

对 01-vmproc-struct.md 进行深度 review，修复 P0/P1 问题，并同步修改关联 Rust 代码。

---

## P0 修复

### 1. Ch2 §2.2.3 包含 Rust 内容（违反文档结构规则）

**问题**: §2.2.3 "初始化要求" 中包含 Rust 实现细节：
> Rust: `VmProc::vacant()` 因 const 初始化约束使用 `UserSlot(0)` 占位，实际 `vm_slot` 在 `get_empty()` 或 `alloc_empty_slot()` 中根据数组索引设置

**规则**: 第1章（概述）和第2章（背景与约束/C 源码分析）不得包含 Rust 相关内容。

**修改**: 将 Rust 实现细节替换为指向 §4.3 和 §5.1.2 的交叉引用。

### 2. Ch3 §3.2.6 vm_boot 包含 "Rust 设计差异" 表格

**问题**: §3.2.6 vm_boot 字段详解中包含完整的 Rust 设计差异对比表（类型、所有权、初始化三行对比），以及 Rust 使用值语义的三点理由。

**规则**: Ch3（C 源码分析）不得包含 Rust 相关内容。

**修改**: 删除 "Rust 设计差异" 表格和理由说明，替换为一句摘要 + 指向 §4.3 和 §5.1 的交叉引用。

### 3. Ch3 §3.2.6 vm_bytecopies 包含 Rust 代码对比

**问题**: §3.2.6 vm_bytecopies 字段详解中：
- 类型描述混合 C/Rust：`int（C，32-bit）/ u64（Rust，64-bit）`
- 包含 "类型差异说明" 段落解释 Rust 为何用 u64
- 包含 Rust 代码片段 `#[cfg(feature = "vmstats")] pub byte_copies: u64`
- 常量编译说明混合 C/Rust：`VMSTATS（C）或 vmstats feature（Rust）`

**规则**: Ch3（C 源码分析）不得包含 Rust 相关内容。

**修改**:
- 类型描述改为纯 C：`int（仅在 VMSTATS 启用时存在）`
- 删除 "类型差异说明" 段落
- 删除 Rust 代码片段
- 常量编译说明改为纯 C 描述
- 添加一句 Rust 差异摘要 + 指向 §5.1 的交叉引用

---

## P1 修复

### 4. Ch5 `clear()` 对比表缺少 `map_free_proc()` 行

**问题**: Minix3 `free_proc()` 调用了 `map_free_proc()` 释放映射页（exit.c:35），但对比表中没有对应行。Rust 的 `vm_regions_avl.clear()` 实际合并了 `map_free_proc()` + `region_init()` 两个操作。

**修改**: 在对比表中添加 "映射页释放" 行：`map_free_proc()` → `—` → `vm_regions_avl.clear()` → ✅

### 5. Rust 代码 `write_page_table_mappings()` 硬编码 PAGE_SIZE

**问题**: `vmproc_handle.rs:356` 使用 `const PAGE_SIZE: u64 = 4096;` 硬编码页大小，违反硬件抽象原则。应通过 `Paging` trait 的关联常量获取。

**修改**: 改为 `const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;`

---

## 未修改项（记录备查）

- **Ch2 结构**: 当前 Ch2 为"背景与约束"而非标准结构的"C 源码分析"。内容上与 Ch1 有重叠，但未做大规模重构，仅修复了 Rust 内容违规。
- **Ch6 生命周期与状态机**: 内容详实，typestate view 设计与 Rust 代码一致，无需修改。
- **Ch7 测试与验证**: 覆盖了构造、字段访问、状态标志、Drop、clear()、Minix3 行为对比等维度，无需修改。
