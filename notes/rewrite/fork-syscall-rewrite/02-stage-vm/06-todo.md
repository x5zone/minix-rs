# 06-todo: Review 修复记录

## 审查文件
`06-pagetable-struct.md`

## 审查结果

### P0 修复

#### 1. direct_map.rs 双视图地址空间修正

**问题**: 文档 §3.6.0 明确描述了双视图地址空间布局：
- VM direct map: `0x00000000_80000000` 附近（U/S=1，用户态可访问）
- Kernel direct map: `0xFFFF8000_00000000`（U/S=0，仅内核态可访问）

但代码 `direct_map.rs` 中 `vm_phys_to_virt()` 和 `kernel_phys_to_virt()` 都使用 `DIRECT_MAP_BASE = 0xFFFF_8000_0000_0000`，返回相同的 VA。这意味着 VM（ring 3 进程）会尝试访问 U/S=0 的内核地址，导致页错误。

**修复**:
- 将 `DIRECT_MAP_BASE` 拆分为 `VM_DIRECT_MAP_BASE = 0x0000_0000_8000_0000` 和 `KERNEL_DIRECT_MAP_BASE = 0xFFFF_8000_0000_0000`
- `vm_phys_to_virt()` 使用 `VM_DIRECT_MAP_BASE`
- `kernel_phys_to_virt()` 使用 `KERNEL_DIRECT_MAP_BASE`
- `virt_to_phys()` 自动检测属于哪个 direct map 区域
- `is_direct_map_virt()` 检查两个区域
- 更新 `alloc_page.rs` 和 `lib.rs` 中的所有引用
- 新增 `test_kernel_phys_to_virt` 测试

**文件**: `os/servers/vm/src/direct_map.rs`, `os/servers/vm/src/alloc_page.rs`, `os/servers/vm/src/lib.rs`

**验证**: `cargo check` 编译通过，`cargo test direct_map` 4 个测试全部通过。

**注意**: `VM_DIRECT_MAP_BASE = 0x0000_0000_8000_0000` 是根据文档 §3.6.0 的地址空间布局图选择的（VM direct map 在 VM 代码/数据之后）。实际值可能需要根据 kernel 的 VM 初始页表设置调整。

### 文档审查

1. **文档 §3.6 方案四新增**：完整描述了 VM 初始页表结构（4 页）、DirectMapArch trait、双视图布局，与 ptregion_design.md §8 一致。
2. **Minix3 源码引用**：`pt.h` pt_t 定义、`pagetable.c:990` pt_new、`pagetable.c:494` pt_ptalloc — 全部验证通过。
3. **Paging trait**：设计合理，与代码一致。
4. **PageFlags**：bitflags u16 设计，与代码一致。
5. **DirectMapArch trait**：文档中定义为【设计目标】，代码中尚未实现。当前 `direct_map.rs` 是简化版本。

### 未修复项（P2，记录备查）

1. **DirectMapArch trait 未实现**：文档 §3.6.3 定义了 `DirectMapArch` trait（含 `supports_1gb_page()`、`HUGE_PAGE_SHIFT`、`PTE_HUGE_FLAGS` 等），代码中尚未实现。当前 `direct_map.rs` 是硬编码的 x86-64 常量。
2. **VM_DIRECT_MAP_BASE 值待确认**：当前选择 `0x0000_0000_8000_0000`（2GB 处），与文档 §3.6.0 的布局图一致，但实际值需要与 kernel 的初始页表设置协调。
