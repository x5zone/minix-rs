# 04-todo: Review 修复记录

## 审查文件
`04-physical-memory.md`

## 审查结果

### P1 修复

#### 1. 文件结构树更新

**问题**: §8 文件结构树将 `direct_map.rs` 列在 `phys_mem/` 下，但实际代码中 `direct_map.rs` 位于 `src/` 顶层。`early_heap.rs` 在代码中已不存在（方案四删除），但文档仍列出。

**修复**: 更新文件结构树：
- 从 `phys_mem/` 中删除 `early_heap.rs` 和 `direct_map.rs`
- 新增 `src/ (顶层)` 段，列出 `direct_map.rs` 和 `alloc_page.rs`

**验证**: `ls os/servers/vm/src/phys_mem/*.rs` 确认无 `early_heap.rs`；`ls os/servers/vm/src/direct_map.rs` 确认在顶层。

### Rust 代码审查

1. **direct_map.rs** — 已实现 `vm_phys_to_virt()`、`kernel_phys_to_virt()`、`virt_to_phys()`、`is_direct_map_virt()`，与文档 §4.4 描述一致
2. **alloc_page.rs** — `VmPageAllocator` 已使用 `vm_phys_to_virt()`，`ReservedRegion` 已简化为纯物理页预留（无 VA 分配），与文档 §2.3 一致
3. **phys_mem/ 模块** — 无 `early_heap.rs`，与方案四一致
4. **kernel_phys_to_virt** — 当前实现与 `vm_phys_to_virt` 相同（都是 `phys + DIRECT_MAP_BASE`）。根据 ptregion_design.md §8.1.3，两者应映射到不同 VA 窗口（VM direct map U/S=1, Kernel direct map U/S=0）。当前是简化实现，后续需区分

### 未修复项（P2，记录备查）

1. **kernel_phys_to_virt 与 vm_phys_to_virt 未区分**：当前两者实现相同。ptregion_design.md §8.1.3 描述的双视图模型（VM direct map + Kernel direct map）需要在页表层面区分，但 `phys_to_virt()` 函数本身在单地址空间下返回相同 VA 是正确的——区别在于 PTE 的 U/S 标志位，而非 VA 值。需要确认设计意图
2. **PAF_CLEAR 仍为 TODO**：文档 §5.3 中标注 `sys_memset` 需内核 IPC，方案四下可通过 `vm_phys_to_virt()` 直接 memset，无需 IPC。但代码中尚未实现
3. **BitmapAllocator init 签名**：文档 §5.3 方案四标注说 init 签名应从 `init(early_heap, ...)` 变为 `init(alloc, ...)`，但代码中 BitmapAllocator 的 init 仍接受 `&mut [u64]` 切片参数（由调用方分配），这是更灵活的设计
