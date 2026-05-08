# 13-region-avl.md 修改记录

## 规则 1 违规修复：Ch1/Ch2 包含 Rust 内容

### 1.1 移除 Ch2 中 32 位 vs 64 位对比表的 Rust 列

**位置**: 原 2.1.1 节"32 位 vs 64 位差异"表格

**问题**: 表格包含"minix-rs (64 位)"列，引用了 `i8`、`u64` 等 Rust 类型，以及"Rust 用最小类型"等 Rust 专属描述，违反 Ch2 不得包含 Rust 内容的规则。

**修改**: 删除"minix-rs (64 位)"列，仅保留 Minix3 (32 位) 列和说明列。将 `factor` 说明从"Rust 用最小类型"改为"实际只用 -1/0/1"，`AVL_MAX_DEPTH` 说明从"64 位下可适当增大"改为"good for ~2M nodes"（与源码注释一致）。

## 规则 2 违规修复：C 代码引用不准确

### 2.1 regionavl.h 描述缺少 unavl.h

**位置**: Ch1"与 Minix3 的对应关系"表格

**问题**: `regionavl.h` 描述为"模块入口（组合以上头文件）"，但实际文件还 `#include "unavl.h"`。

**修改**: 改为"模块入口（组合以上头文件及 unavl.h）"。

### 2.2 regionavl_defs.h 代码片段缺少 AVL_NULL

**位置**: Ch1 代码片段

**问题**: 代码片段缺少 `#define AVL_NULL NULL`，该宏在源码中存在且被 `region_init` 等函数使用。

**修改**: 添加 `#define AVL_NULL NULL // 空句柄`，标注"（部分）"表示非完整列表。

### 2.3 region_search 代码与实际源码不符

**位置**: 原 2.2.4 节"精确查找"代码

**问题**: 文档展示了一个简化版 `region_search`，使用 `if (st & AVL_LESS) found = h` 的直观逻辑。实际 C 源码使用 `target_cmp` 和 XOR 高位掩码 `(cmp ^ target_cmp) & L__MASK_HIGH_BIT` 判断同号，算法逻辑完全不同。

**修改**: 替换为与实际 `cavl_impl.h` 源码一致的宏展开版本，包含 `target_cmp`、`cmp = -target_cmp`、XOR 同号判断等关键逻辑。

### 2.4 section 标题函数名错误

**位置**: 原 2.2.4 节标题

**问题**: 标题为"region_find - 查找区域"，但 Minix3 中不存在 `region_find` 函数，实际函数名为 `region_search`。

**修改**: 改为"region_search - 搜索区域"。

### 2.5 迭代器使用场景函数名错误

**位置**: 2.3.1 节"使用场景"表格

**问题**:
- `region_copy_slab` 在 Minix3 源码中不存在，fork 时实际调用 `map_copy_region`
- `map_free` 是释放单个区域的函数，进程终止时遍历释放所有区域的函数是 `map_free_proc`
- `region_sanitycheck` 不存在，实际函数是 `map_sanitycheck`

**修改**: 逐一替换为正确的函数名。

### 2.6 迭代器代码示例函数名错误

**位置**: 2.3.1 节"VM 中的使用"代码示例

**问题**: `region_sanitycheck(r)` 不存在。

**修改**: 改为 `map_sanitycheck(__FILE__, __LINE__)`。

## 规则 4 违规修复：文档 Rust 代码与实际实现不符

### 4.1 VirRegion 结构体字段不匹配

**位置**: Ch3 和 Ch4 多处

**问题**: 文档中的 VirRegion 与实际 `vir_region.rs` 差异巨大：
- `parent: Weak<VmProc>` → 实际为 `parent_slot: Option<UserSlot>`
- `mem_type: Arc<dyn MemType>` → 实际为 `def_memtype: Option<&'static dyn MemType>`
- `phys_regions: Vec<PhysRegion>` → 实际为 `physblocks: Vec<Option<Box<PhysRegion>>>`
- `id: u32` → 实际为 `id: i32`
- `remaps: u32` → 实际为 `remaps: i32`
- `param: RegionParam` → 实际为 `param: VrParam`
- `factor: BalanceFactor` → 实际为 `factor: i8`
- `end_addr` 使用 `VirBytes(self.vaddr.0 + self.length.0)` → 实际为 `self.vaddr + self.length`

**修改**: 所有 VirRegion 定义替换为与 `vir_region.rs` 一致的版本。

### 4.2 不存在的 BalanceFactor 类型

**位置**: 原 Ch4 4.1 节

**问题**: 文档定义了 `BalanceFactor` 结构体（含 `LEFT_HEAVY`、`BALANCED`、`RIGHT_HEAVY` 常量和 `inc()`、`dec()`、`needs_rebalance()` 方法），但实际代码中不存在此类型，`factor` 字段直接使用 `i8`。

**修改**: 删除 `BalanceFactor` 定义，`factor` 字段改为 `i8`。

### 4.3 不存在的 AvlNode 结构体

**位置**: 原 Ch4 4.1 节

**问题**: 文档定义了独立的 `AvlNode` 结构体（含 `lower`、`higher`、`factor` 字段和 `new_leaf()`、`child_heights()`、`update_factor()` 方法），但实际代码中 AVL 字段直接嵌入 `VirRegion`，不存在独立的 `AvlNode`。

**修改**: 删除 `AvlNode` 定义，AVL 字段直接在 VirRegion 中展示。

### 4.4 不存在的旋转函数和平衡维护代码

**位置**: 原 Ch4 4.2-4.3 节

**问题**: 文档包含完整的 `rotate_right`、`rotate_left`、`rotate_left_right`、`rotate_right_left`、`rebalance` 函数，以及使用 `Vec<*mut VirRegion>` 路径追踪和 `unsafe` 代码的 `insert`/`remove` 实现。实际代码中：
- 不存在任何旋转函数
- `insert` 是简单的递归 BST 插入，不维护平衡
- `remove` 是简单的递归 BST 删除，不维护平衡
- `factor` 字段保留但未使用

**修改**: 将 4.2 节替换为实际的递归 BST 插入/删除实现，4.3 节改为"查找优化"。原旋转和平衡维护代码移至"待实现的平衡维护"小节，明确标注为未实现。

### 4.5 insert 返回类型错误

**位置**: 原 Ch4 4.3 节

**问题**: 文档中 `insert` 返回 `Option<VirRegion>`（重复键时返回旧节点），实际代码返回 `()`（void），重复键时原地替换。

**修改**: 与实际代码一致，`insert` 返回 `()`。

### 4.6 RegionIterMut 内部类型错误

**位置**: Ch3 3.3 节

**问题**: 文档中 `RegionIterMut` 使用 `stack: Vec<&'a mut VirRegion>`，实际代码使用 `stack: Vec<*mut VirRegion>` 配合 `_marker: PhantomData<&'a mut VirRegion>`。

**修改**: 与实际代码一致。

### 4.7 可见性修饰符错误

**位置**: Ch3/Ch4 多处

**问题**: 文档使用 `pub` 修饰所有类型和方法，实际代码使用 `pub(crate)`。

**修改**: 所有 `pub` 改为 `pub(crate)`。

### 4.8 SearchType 常量可见性

**位置**: Ch4 4.3 节

**问题**: 文档中 `SearchType` 常量使用 `pub const`，实际使用 `pub(crate) const`。

**修改**: 与实际代码一致。

### 4.9 不存在的 VR_ACCESSED 标志

**位置**: Ch3 3.3 节可变迭代器示例

**问题**: `region.flags |= VR_ACCESSED` 使用的 `VR_ACCESSED` 在 Minix3 和 Rust 代码中均不存在。

**修改**: 改为 `region.flags |= VrFlags::WRITABLE`。

### 4.10 search 函数语法错误

**位置**: 原 Ch4 4.3 节

**问题**: 文档中 `search_node` 使用 `cmp.0` 访问 `Ordering` 的内部值，但 `Ordering` 没有 `.0` 字段。实际代码使用 `cmp_val: i32` 变量。

**修改**: 与实际代码一致，使用 `let cmp_val = if cmp == Ordering::Less { -1i32 } else { 1i32 };`。

### 4.11 实现策略描述不准确

**位置**: Ch3 3.1 节"实现策略"

**问题**:
- "insert（插入并平衡）"→ 实际不维护平衡
- "remove（删除并平衡）"→ 实际不维护平衡
- "全部使用安全 Rust，无裸指针、无 unsafe 块"→ 实际 `RegionIterMut` 使用 `*mut VirRegion` 和 `unsafe`

**修改**: 更新为与实际代码一致的描述。

### 4.12 当前实现状态描述不完整

**位置**: Ch3 3.1 节"当前实现状态"

**问题**: 列表过于简略，未反映实际实现的完整 API。

**修改**: 扩展为完整的 API 列表，明确标注 `factor` 字段保留但未使用。

### 4.13 缓存优化代码中的 VirRegion 字段名错误

**位置**: Ch5 5.3 节

**问题**: 优化布局代码使用 `phys_regions: Vec<PhysRegion>` 等旧字段名。

**修改**: 替换为与实际代码一致的字段名。

## 规则 5 验证：最后一章为"参见"

**结果**: 最后一章为"## 7. 参见"，符合要求。无需修改。

## 规则 6 验证：Ch3 聚焦"why"，Ch4 聚焦"how"

**结果**: Ch3（Rust 设计决策）讨论实现选择、设计目标和 API 设计理由，聚焦"why"。Ch4（实现详解）展示具体代码实现，聚焦"how"。结构合理，无需调整。
