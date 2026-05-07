# 06-pagetable-struct.md 审查修改记录

## 审查规则

1. Ch1（概述）和 Ch2（C 源码分析）不得包含 Rust 内容
2. 所有 C 代码引用（文件路径、行号）必须与 Minix3 源码一致
3. 所有数值常量必须与 Minix3 源码一致
4. 文档中的 Rust 代码必须与 `/workspace/os/servers/vm/src/` 中的实际代码一致
5. 最后一章必须为"参见"
6. Ch3 聚焦"为什么"（设计决策），Ch4 聚焦"怎么做"（实现）

## 修改清单

### 修改 1：Ch2 移除 Rust 引用（规则 1）

**位置**：§2.1 `pt_virtop` 字段详解，结论段落

**问题**：结论段落末尾包含"Rust 版本不包含此字段。"，属于 Rust 内容，违反 Ch2 不得包含 Rust 内容的规则。

**修改**：删除"Rust 版本不包含此字段。"（该信息已在 Ch3 §3.1 的"关于 `pt_virtop`"注释中涵盖）

**验证**：Ch1 和 Ch2 现在均不包含任何 Rust 内容。

---

### 修改 2：C 代码片段缺少 `& ARCH_VM_ADDR_MASK` 掩码（规则 2）

**位置**：§2.1 `pt_pt[]` 字段详解，`pt_ptalloc` 代码片段

**问题**：文档中 `pt->pt_dir[pde] = pt_phys | flags`，但实际 Minix3 源码（pagetable.c:532）为 `pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags`。缺少 `& ARCH_VM_ADDR_MASK` 掩码操作，该掩码用于清除物理地址低 12 位标志位，确保只取页框地址。

**修改**：`pt_phys | flags` → `(pt_phys & ARCH_VM_ADDR_MASK) | flags`

**验证源码**：`/workspace/minix3/minix/servers/vm/pagetable.c` 第 532 行

---

### 修改 3：MockPaging 代码与实际 Rust 源码不一致（规则 4）

**位置**：§4.1 MockPaging 实现

**问题**：多处代码与 `/workspace/os/arch/src/paging.rs` 中的实际实现不一致：

| 项目 | 文档原文 | 实际代码 | 修改内容 |
|------|----------|----------|----------|
| `new()` 实现 | `next_id()` / `allocate_root_phys()` 占位符 | `MOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst)` / `0x1000 + (id as u64 * 0x1000)` | 替换为实际实现 |
| `new()` 签名 | 缺少 `where Self: Sized` | 有 `where Self: Sized` 约束 | 补充约束 |
| `destroy()` 方法 | 缺失 | 实际代码中有实现（清空 mappings + 重置 ACTIVE_MOCK_TABLE） | 补充方法 |
| `map()` 对齐检查 | 仅检查 `vaddr` | 同时检查 `vaddr` 和 `paddr` 对齐 | 补充 `paddr` 检查 |
| `remap()` 对齐检查 | 仅检查 `vaddr` | 同时检查 `vaddr` 和 `paddr` 对齐 | 补充 `paddr` 检查 |
| `unmap()` 对齐检查 | 无 | 有 `vaddr` 对齐检查 | 补充对齐检查 |
| `update_flags()` | 缺失 | 实际代码中有实现 | 补充方法 |
| `switch()` 实现 | `ACTIVE_TABLE = Some(self.id)` | `ACTIVE_MOCK_TABLE.store(self.id, Ordering::SeqCst)` | 替换为实际实现 |
| `flush_tlb()` | 缺失 | 实际代码中有空实现 | 补充方法 |
| `flush_tlb_addr()` | 缺失 | 实际代码中有空实现 | 补充方法 |

**验证源码**：`/workspace/os/arch/src/paging.rs` 第 290-417 行

---

### 修改 4：VirBytes/PhysBytes 类型定义缺少属性（规则 4）

**位置**：§5.1 物理地址 vs 虚拟地址

**问题**：
- `VirBytes` 缺少 `#[repr(transparent)]` 属性和 `Default` derive
- `PhysBytes` 缺少 `#[repr(transparent)]` 属性

**修改**：
- `VirBytes` 添加 `#[repr(transparent)]` 和 `Default` derive
- `PhysBytes` 添加 `#[repr(transparent)]`

**验证源码**：`/workspace/os/libs/minix-types/src/types/address.rs` 第 17-18 行（VirBytes）、第 95-97 行（PhysBytes）

---

### 修改 5：`PageFlags::user_read_write()` 方法名不存在（规则 4）

**位置**：§5.3.3 使用示例

**问题**：文档使用 `PageFlags::user_read_write()`，但实际代码中不存在此方法。正确方法名为 `PageFlags::read_write()`。

**修改**：`PageFlags::user_read_write()` → `PageFlags::read_write()`

**验证**：在 `/workspace/os` 全代码库中搜索 `user_read_write` 无结果；`read_write()` 定义于 `/workspace/os/arch/src/paging.rs` 第 52 行。

---

### 修改 6：ActiveProc debug_assert 消息不准确（规则 4）

**位置**：§5.3.2 vmproc 中的页表存储，`page_table()` 和 `page_table_mut()` 方法

**问题**：文档中 debug_assert 消息为 `"vm_pt accessed before init"`，但实际代码为 `"vm_pt accessed before init_page_table()"`。后者更精确地指向初始化函数名。

**修改**：`"vm_pt accessed before init"` → `"vm_pt accessed before init_page_table()"`

**验证源码**：`/workspace/os/servers/vm/src/vmproc/vmproc_handle.rs` 第 284 行和第 294 行

---

## 审查通过项

以下项目经审查确认无误，无需修改：

- **Ch1/Ch2 无其他 Rust 内容**：Ch1 纯概念描述，Ch2 纯 C 源码分析（修改 1 已处理唯一违规）
- **C 代码行号引用**：所有行号引用（pagetable.c:328, 494, 990, 1019, 1358, 1427; findhole:155）均与源码一致
- **数值常量**：§2.2 PDE 标志位、§2.3 PTE 标志位、§2.4 x86 特定定义中的所有常量值均与 `/workspace/minix3/minix/include/arch/i386/include/vm.h` 一致
- **最后一章为"参见"**：Ch7 标题为"参见"，符合规则
- **Ch3 聚焦"为什么"**：§3.0 问题与建模、§3.1-3.5 均以设计决策原因为主线
- **Ch4 聚焦"怎么做"**：§4.1-4.3 均以实现细节为主线
- **Paging trait 定义**：与 `/workspace/os/arch/src/paging.rs` 一致
- **PageTableError 枚举**：6 个变体与源码一致
- **PageFlags bitflags 定义**：9 个标志位及辅助方法与源码一致
- **PagingWithId trait**：与 `/workspace/os/arch/src/paging_ext.rs` 一致
- **HugePages trait**：与 `/workspace/os/arch/src/paging_ext.rs` 一致
- **page_align/page_align_down 函数**：与 `/workspace/os/servers/vm/src/pagetable/mod.rs` 一致
- **VmProc 结构体**：简化展示但核心字段与源码一致
- **ActiveProc::init_page_table/bind_page_table**：逻辑与源码一致
- **VmProc::clear()**：简化展示但逻辑与源码一致
