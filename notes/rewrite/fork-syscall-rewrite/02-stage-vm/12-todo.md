# 12-vir-region.md 修订记录

## 审查规则

1. Ch1（概述）和 Ch2（C 源码分析）不得包含 Rust 内容
2. 所有 C 代码引用（文件路径、行号）必须与 Minix3 源码一致
3. 所有数值常量必须与 Minix3 源码一致
4. 文档中的 Rust 代码必须与实际 Rust 源码一致
5. 最后一章必须是"参见"
6. Ch3 聚焦"为何"设计决策，Ch4 聚焦"如何"实现

## 审查结果

### 规则 1：Ch1/Ch2 无 Rust 内容 ✓

Ch1（概述）和 Ch2（C 源码分析）均不包含 Rust 代码或 Rust 特有概念，符合要求。

### 规则 2：C 代码引用验证 ✓

| 引用 | 文档标注 | 实际位置 | 结果 |
|------|----------|----------|------|
| `vir_region` 结构体 | `region.h` | `region.h:37-66` | ✓ 一致 |
| `VR_*` 常量 | `region.h` | `region.h:68-79` | ✓ 一致 |
| `phys_block` 结构体 | `region.h` | `region.h:23-33` | ✓ 一致 |
| `phys_region` 结构体 | `phys_region.h` | `phys_region.h:8-21` | ✓ 一致 |
| `map_page_region` | `region.c:463` | `region.c:463` | ✓ 一致 |
| `map_free` | `region.c:568` | `region.c:568` | ✓ 一致 |
| `map_lookup` | `region.c:616` | `region.c:616` | ✓ 一致 |
| `region_new` | `region.c:424` | `region.c:424` | ✓ 一致 |
| `pb_link` | `pb.c:61` | `pb.c:61` | ✓ 一致 |
| `pb_unreferenced` | `pb.c:96` | `pb.c:96` | ✓ 一致 |
| `split_region` | `region.c:1150` | `region.c:1150` | ✓ 一致 |
| `map_copy_region` | `region.c:802` | `region.c:802` | ✓ 一致 |
| `mem_cow` | `pb.c:136` | `pb.c:136` | ✓ 一致 |
| AVL 搜索类型常量 | 文档值 1/2/4/3/5 | `cavl_if.h:26-30` | ✓ 一致 |

### 规则 3：数值常量验证 ✓

所有 `VR_*` 常量值（0x001, 0x004, 0x008, 0x010, 0x040, 0x080, 0x100, 0x200, 0x400）与 `region.h` 一致。

### 规则 4：Rust 代码与实际源码一致性 — 发现 6 处问题，已全部修复

#### 修复 1：VirRegion 结构体字段错误

- **位置**：Ch3 §3.1，VirRegion 结构体定义
- **问题**：`pub parent: Option<NonNull<VmProc>>` 与实际代码不符
- **实际代码**（`vir_region.rs:81`）：`pub parent_slot: Option<UserSlot>`
- **修复**：将 `parent: Option<NonNull<VmProc>>` 改为 `parent_slot: Option<UserSlot>`
- **原因**：实际 Rust 实现使用进程槽索引（`UserSlot`）而非裸指针引用进程，更安全且与 VM 的进程表设计一致

#### 修复 2：VirRegion 结构体缺少 def_memtype 字段

- **位置**：Ch3 §3.1，VirRegion 结构体定义
- **问题**：缺少 `def_memtype` 字段
- **实际代码**（`vir_region.rs:83`）：`pub def_memtype: Option<&'static dyn MemType>`
- **修复**：在 `parent_slot` 和 `remaps` 之间添加 `def_memtype` 字段

#### 修复 3：设计差异表 parent 行错误 + 缺少 def_memtype 行

- **位置**：Ch3 §3.1，设计差异表
- **问题 1**：`parent` 行的 Rust 类型写为 `Option<NonNull<VmProc>>`，应为 `Option<UserSlot>`
- **问题 2**：缺少 `def_memtype` 行
- **修复**：
  - 将 `parent` 行改为 `parent_slot`，Rust 类型改为 `Option<UserSlot>`，说明更新为"C 用指针引用所属进程，Rust 用进程槽索引，避免裸指针"
  - 新增 `def_memtype` 行：C 类型 `mem_type_t*`，Rust 类型 `Option<&'static dyn MemType>`

#### 修复 4：VrFlags 代码不完整

- **位置**：Ch3 §3.2，Rust 实现代码
- **问题**：
  1. 缺少 `pub(crate)` 可见性修饰符
  2. 缺少 `empty()` 方法
  3. 缺少 `insert()` 方法
  4. 缺少 `remove()` 方法
  5. 缺少 `Default` trait 实现
- **修复**：按实际 `vir_region.rs:16-51` 补全所有缺失内容

#### 修复 5：新增 §3.3 VrParam 联合体设计

- **位置**：Ch3，原 §3.3 physblocks 之前
- **问题**：文档在设计差异表中提到 `enum VrParam` 但从未展示实际代码，读者无法对照 C union 与 Rust enum 的映射
- **修复**：新增 §3.3，包含：
  - C union 代码（来自 `region.h`）
  - Rust enum 代码（来自 `vir_region.rs:57-68`）
  - 设计差异对比表（类型安全、File 字段缺少 fdref、默认值、内存布局）
  - 原 §3.3 physblocks 顺延为 §3.4

#### 修复 6：map_copy_region "limbo" 描述错误

- **位置**：Ch5 §5.1，关键设计段落
- **问题**：原文称"map_copy_region 创建的新区域处于'limbo'状态——不增加 phys_block.refcount，由调用者在链接到子进程后负责增加"。但实际代码中 `map_copy_region` 调用了 `pb_reference` → `pb_link`，后者执行 `refcount++`，引用计数确实被增加了。C 源码注释中"不增加 refcount"的描述与实际代码行为矛盾。
- **修复**：改为准确描述——`map_copy_region` 通过 `pb_reference` 共享原物理页，引用计数随之增加。"limbo"状态指新区域尚未插入目标进程 AVL 树，但不影响引用计数正确性。同时指出 C 源码注释与代码行为不一致。

#### 修复 7：AVL 测试代码与实际不符

- **位置**：Ch6 §6.2，示例测试代码
- **问题**：文档使用 `VirRegion::new(VirBytes(...), VirBytes(...), VrFlags::empty())` 直接构造区域，但实际测试代码使用 `make_region(vaddr, length)` 辅助函数
- **修复**：添加 `make_region` 辅助函数定义，测试代码改用 `make_region`

#### 修复 8：AVL 测试列表不完整

- **位置**：Ch6 §6.2，已实现测试表
- **问题**：仅列出 4 个测试，实际 `avl.rs` 中有 14 个测试
- **修复**：补全所有 14 个测试项（`test_avl_iter`、`test_search_type_less/greater/less_equal/greater_equal`、`test_find_slot_basic/in_gap/no_space`、`test_find_all_overlaps`、`test_search_type_flags`）

### 规则 5：最后一章为"参见" ✓

当前最后一章为"7. 参见"，符合要求。

### 规则 6：Ch3 聚焦"为何"，Ch4 聚焦"如何" ✓

- Ch3 "Rust 设计决策"：讨论类型选择理由（为何用 Vec 而非裸指针、为何用 enum 而非 union、为何不用 bitflags 等），符合"为何"
- Ch4 "实现详解"：展示操作流程图和步骤分析（如何分配区域、如何查找、如何分割），符合"如何"
