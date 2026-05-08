# 05-todo: Review 修复记录

## 审查文件
`05-vm-allocpage.md`

## 审查结果

### P2 修复

#### 1. 行号范围不精确：pagetable.c:333-389 → 333-392

**问题**: §2.4 递归链分析中引用 `pagetable.c:333-389`，但 `vm_allocpages` 函数体实际到行 392。行 389 是 `level--;`，行 390 是 `vm_self_pages++;`，行 392 是 `return ret;`（函数的最后一行）。

**修复**: 改为 `pagetable.c:333-392`。

**验证**: 读取 `minix3/minix/servers/vm/pagetable.c:333-392`，确认行 392 为 `return ret;`。

### 源码行号验证

| 引用 | 文档标注 | 实际位置 | 一致? |
|------|----------|----------|-------|
| `pagetable.c:328` pt_init_done | L328 | L328=声明 | ✅ |
| `pagetable.c:333` vm_allocpages | L333 | L333=函数签名 | ✅ |
| `pagetable.c:333-392` 完整函数体 | L333-392 | ✅ (已修复) | ✅ |
| `pagetable.c:494-523` pt_ptalloc | L494-523 | L494=函数签名, L523=设置pt_dir | ✅ |
| `pagetable.c:235` vm_freepages | L235 | L235=函数签名 | ✅ |
| `pagetable.c:1088` pt_init | L1088 | L1088=函数签名 | ✅ |
| `pagetable.c:1311-1327` pt_init结尾 | L1311-1327 | L1311=pt_init_done=1, L1327=alloc_cycle | ✅ |

### Rust 代码审查

Rust 代码与文档设计完全一致，无需修改：

- `VmPageAllocator<S, O>` typestate 结构与 §3.3 一致
- `Bootstrap`/`Normal` 类型参数与 §3.3 一致
- `ReservedRegion` 结构体（含 `high_watermark`）与 §4.2 一致
- `ReservedRegion::alloc_page()` 从 high_watermark 开始搜索与 §4.2 一致
- `ReservedRegion::alloc_contig_virt()` 使用 `&mut self` 与 §4.2 一致
- `PtRegion<O>` 结构体（含 `current_pt_base`）与 §4.3 一致
- `PtRegion::from_reserved_with_ops()` 接受 `&mut ReservedRegion` 与 §4.3 一致
- `PtRegion::alloc_pt_page()` 剩余不足 8 时自动扩展与 §4.3 一致
- `PtRegion::expand()` 更新 `current_pt_base` 与 §4.3 一致
- `PtOps` trait（6 个方法）与 §4.1 一致
- `RealPtOps` 使用 `write_volatile`/`read_volatile` 与 §4.1 一致
- `VmPageAllocator<Normal>.alloc_phys/alloc_virt/alloc_page` 与 §3.5 一致
- `into_normal(mut self)` 签名与 §3.3 一致
- 测试覆盖 §5 中所有测试要点

### 上一轮 review 修复验证

上一轮 05-todo.md 记录了 6 个 P0 修复和 5 个 P1 修复：
1. ✅ P0-1: ReservedRegion 已添加 high_watermark 字段
2. ✅ P0-2: alloc_page 已使用 high_watermark 搜索
3. ✅ P0-3: alloc_contig_virt 已使用 &mut self + high_watermark
4. ✅ P0-4: PtRegion 已添加 current_pt_base 字段
5. ✅ P0-5: PtRegion::virt_to_phys 已删除
6. ✅ P0-6: into_normal 签名已改为 mut self
7. ✅ P1-4: VMP_SPARE(0) 已添加
8. ✅ P1-5: from_reserved_with_ops 已使用 &mut ReservedRegion
9. ✅ P1-6: 初始化已设置 current_pt_base: start
10. ✅ P1-7: expand() 已更新 current_pt_base
11. ✅ P1-8: "待修复问题"段落已替换为 high_watermark 设计意图说明

### 未修复项（P2，记录备查）

1. 文档 §2.1 的 `vm_allocpages` 代码块省略了 `assert(reason >= 0 && reason < VMP_CATEGORIES)` 和 `assert(pages > 0)`，这是合理的简化
2. 文档 §2.2 的 `pt_init()` 代码块是简化版本，省略了 `sys_umap` 的详细参数
3. 文档 §4.3 `PtRegion::expand()` 代码块中 `self.current_pt = pt_virt;` 的缩进不一致（多了 4 空格），但不影响理解
4. Rust 代码中 `VmPageAllocator<Normal>` 有 `free_page()` 和 `relocate_phys_allocator()` 方法，文档 §3.3/§3.5 未提及，但 §4.4 时序图中的 T6 涉及了搬迁
