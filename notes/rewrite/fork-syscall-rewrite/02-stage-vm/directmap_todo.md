# Direct Map 设计变动 — 文档修改 TODO

> **创建日期**: 2026-05-08
> **状态**: 待讨论
> **设计依据**: `ptregion_design.md` §8 Direct Map 设计方案

---

## 变动总览

Direct Map 设计引入后，文档 01-19 需要的修改分为以下几类：

| 变动类别 | 影响范围 | 核心变化 |
|---------|---------|---------|
| PtRegion 删除 | 05, 07, 09, ptregion_design.md §1-7 | 所有 PtRegion 引用替换为 `vm_phys_to_virt()` |
| EarlyHeap 删除 | 04, 05 | 所有 EarlyHeap 引用替换为 1GB direct map + bitmap |
| ReservedRegion 简化 | 05, 09 | VA 分配功能删除，仅保留物理页预留语义 |
| `alloc_virt()` 删除 | 05 | `alloc_virt()` → `vm_phys_to_virt()`，不再需要独立 VA 分配 |
| `phys_to_virt()` 双视图 | 06, 07, 08, 10, 14, 16 | 区分 `vm_phys_to_virt()` 与 `kernel_phys_to_virt()` |
| `sys_abscopy` → direct map memcpy | 10, 11, 15, 16 | CoW 页复制不再需要内核系统调用 |
| `pagedir_mappings` 删除 | 07 | 内核通过 direct map 直接访问进程页目录 |
| `createpde/freepde` 删除 | 07 | 不再需要临时映射窗口 |
| `spare_pagequeue` 删除 | 04, 05 | direct map 消除递归根源 |
| 3 阶段启动 | 04, 05 | bitmap → expand → optional migrate |
| VM 初始页表结构 | 06 | 4 页：PML4 + PDPT_A + PD_A + PDPT_B |
| Kernel direct map 只读不变量 | 07 | `map_kernel()` 后不再修改 kernel direct map PTE |
| DirectMapArch trait | 06 | 新增跨架构抽象 |

---

## 04-physical-memory.md — 重度修改

### TODO-04-1: EarlyHeap 整节替换
**位置**: §4.2 EarlyHeap 实现（~L1087-1141）
**现状**: 描述 `EarlyHeap` bump allocator，从 BSS 段分配
**修改**: 整节替换为 "Direct Map + Bitmap 启动"。描述：
- Kernel 传递 1GB direct map（PDPT_B[0] = 1GB huge page）
- VM 启动即可通过 `vm_phys_to_virt()` 访问前 1GB 物理内存
- Bitmap 元数据从前 1GB 可用物理页分配
- EarlyHeap 不再需要，删除 `early_heap.rs`
**参考**: ptregion_design.md §8.1.5, §8.5 Phase 1

### TODO-04-2: 物理内存初始化流程重写
**位置**: §4.4 物理分配器初始化流程（~L1046-1050 的阶段描述）
**现状**: 阶段 3-5 描述切出 early_heap → 初始化 early_heap → 初始化物理分配器
**修改**: 替换为 3 阶段启动：
- Phase 1: Bootstrap — 1GB direct map → bitmap allocator
- Phase 2: Direct Map 扩展 — bitmap 分配页表页 → 扩展覆盖全部物理内存
- Phase 3: 分配器迁移 — bitmap → buddy（策略决定）
**参考**: ptregion_design.md §8.5

### TODO-04-3: Bitmap/FreeList/Buddy init 签名变更
**位置**: §4.5/4.6/4.7 中 `init(early_heap: &mut EarlyHeap, ...)` 签名
**现状**: 所有 init 函数接受 `&mut EarlyHeap` 参数
**修改**: 替换为 `init(phys_alloc: &mut BitmapAllocator, ...)` 或直接通过 `vm_phys_to_virt()` 访问
**参考**: ptregion_design.md §8.5 Phase 1

### TODO-04-4: spare_pagequeue 引用删除
**位置**: §2 中 Minix3 spare_pagequeue 相关描述（~L814-831）
**现状**: 描述 Minix3 的 `spare_pagequeue` 机制
**修改**: 在 Minix3 历史描述中保留（作为背景），但明确标注 "minix-rs 不需要此机制，direct map 消除递归根源"
**参考**: ptregion_design.md §8.9

### TODO-04-5: 文件结构树更新
**位置**: 末尾的文件结构树（~L1748）
**现状**: 包含 `early_heap.rs`
**修改**: 删除 `early_heap.rs`，新增 `direct_map.rs`
**参考**: ptregion_design.md §8.10

---

## 05-vm-allocpage.md — 重度修改

### TODO-05-1: PtRegion 引用全面替换
**位置**: §3.5 alloc_phys/alloc_virt 拆分（~L579-594）、§4.1 VmPageAllocator 结构（~L412-449）、§4.3 ReservedRegion（~L642-724）、§5.4 测试（~L893-900）
**现状**: 大量 PtRegion 引用：`pt_region: Option<PtRegion<O>>`、`alloc_virt()`、`PtRegion::from_reserved_with_ops()`
**修改**:
- `alloc_virt()` 整个函数删除，替换为 `vm_phys_to_virt(phys)`
- `pt_region` 字段从 `VmPageAllocator` 删除
- `PtRegion::from_reserved_with_ops()` 调用删除
- `alloc_page()` 简化为 `alloc_phys() → vm_phys_to_virt(phys)`
**参考**: ptregion_design.md §8.1.4, §8.6, §8.7

### TODO-05-2: ReservedRegion 简化
**位置**: §4.2 ReservedRegion（~L642-724）
**现状**: ReservedRegion 包含 VA 分配功能（`alloc_contig_virt`），为 PtRegion 提供 VA
**修改**: ReservedRegion 的 VA 分配功能删除（direct map 替代），仅保留物理页预留语义。`alloc_contig_virt()` 删除。ReservedRegion 变为纯粹的 "已知物理页列表"
**参考**: ptregion_design.md §8.10 删除清单

### TODO-05-3: VmPageAllocator 结构体重写
**位置**: §4.1 VmPageAllocator 定义（~L364-449）
**现状**: Bootstrap/Normal 两阶段，Bootstrap 含 `phys_alloc` + `pt_ops`，Normal 含 `pt_region`
**修改**: 简化为单阶段（direct map 从启动即可用）：
- 删除 `phys_alloc: Option<...>` 的 Option 包裹
- 删除 `pt_ops: Option<RealPtOps>`
- 删除 `pt_region: Option<PtRegion<O>>`
- 删除 `into_normal()` 转换函数
- 保留 `reserved: ReservedRegion`（但简化为纯物理页列表）
**参考**: ptregion_design.md §8.5 Phase 1

### TODO-05-4: spare_pagequeue 引用更新
**位置**: §2 Minix3 allocpage（~L163-171, L246, L311, L517）
**现状**: 描述 Minix3 的 `spare_pagequeue` 和 `STATIC_SPARE_PAGES`
**修改**: 在 Minix3 历史描述中保留，但明确标注 "minix-rs 不需要此机制，direct map 消除递归根源"
**参考**: ptregion_design.md §8.9

### TODO-05-5: 递归问题分析更新
**位置**: §3.3 递归问题（~L836）、§5.4 测试（~L865-900）
**现状**: 描述 alloc_virt 导致的递归问题及 PtRegion 的解决方案
**修改**: 更新结论 — direct map 从结构上消除递归根源：
- 新 PT 页 = `alloc_phys() → vm_phys_to_virt()`，不需要 map、不需要 find_hole、不需要 ensure_tables
- 删除 alloc_phys/alloc_virt 拆分的必要性说明
**参考**: ptregion_design.md §8.1.4 理由 4

---

## 06-pagetable-struct.md — 中度修改

### TODO-06-1: VM 初始页表结构新增
**位置**: 需要新增一节或在现有初始化描述中添加
**现状**: 未描述 VM 初始页表的具体结构
**修改**: 新增 "VM 初始页表" 描述：
- 4 页结构：PML4 + PDPT_A + PD_A + PDPT_B
- PDPT_B[0] = 1GB direct map
- 三种架构等价结构表
**参考**: ptregion_design.md §8.4

### TODO-06-2: DirectMapArch trait 新增
**位置**: 需要新增一节或在架构相关描述中添加
**现状**: 未描述跨架构 direct map 抽象
**修改**: 新增 DirectMapArch trait 定义及三种架构实现
**参考**: ptregion_design.md §8.3

### TODO-06-3: 地址空间布局更新
**位置**: 如有地址空间布局描述
**现状**: 可能使用旧布局（无 VM direct map）
**修改**: 更新为 §8.2 的双视图布局（VM direct map + Kernel direct map）
**参考**: ptregion_design.md §8.2

---

## 07-pagetable-ops.md — 重度修改

### TODO-07-1: §3.0 架构决策更新
**位置**: §3.0 架构决策：取消 pagedir_mappings，采用直接映射区（~L778-811）
**现状**: 已描述 kernel direct map 方案，但未涉及 VM direct map 和双视图模型
**修改**: 更新为双视图模型：
- Kernel direct map: U/S=0, 存在于所有进程页表
- VM direct map: U/S=1, 仅存在于 VM 进程页表
- 两者映射同一物理内存，只是 VA 窗口和权限不同
- `phys_to_virt()` 拆分为 `vm_phys_to_virt()` 和 `kernel_phys_to_virt()`
**参考**: ptregion_design.md §8.1.3

### TODO-07-2: pagedir_mappings 历史描述标注
**位置**: §2.1 中 pagedir_mappings 的详细描述（~L237-340）
**现状**: 详细描述了 pagedir_mappings 机制
**修改**: 保留作为 Minix3 历史参考，但在开头添加更醒目的标注："minix-rs 已删除此机制，kernel 通过 direct map 直接访问进程页目录"。现有 §2.1 开头的阅读提示可加强
**参考**: ptregion_design.md §8.9

### TODO-07-3: pt_bind 语义更新
**位置**: §2.1.2 pt_bind（~L311-388）、§4 Rust 实现（~L1065）
**现状**: pt_bind 包含 pagedir_mappings 登记步骤
**修改**: 明确 pt_bind 不再需要 pagedir_mappings 登记步骤，仅保留 `sys_vmctl_set_addrspace()` 通知内核。已有 §4 的说明（L1065），可进一步强化
**参考**: ptregion_design.md §8.9

### TODO-07-4: pt_mapkernel 更新
**位置**: §2.1.1 pt_mapkernel 描述
**现状**: 描述映射内核代码段 + pagedir_mappings
**修改**: 更新为映射内核代码段 + kernel direct map（1GB huge pages, U/S=0, G=1）。强调 kernel direct map 只读不变量
**参考**: ptregion_design.md §8.8.1

### TODO-07-5: createpde/freepde 历史描述标注
**位置**: §2 中 createpde/freepde 相关描述
**现状**: 描述 Minix3 的临时映射窗口机制
**修改**: 保留作为 Minix3 历史参考，添加标注 "minix-rs 不需要此机制，kernel direct map 替代"
**参考**: ptregion_design.md §8.9

### TODO-07-6: sys_datacopy 简化
**位置**: §3.0 中跨进程内存拷贝描述（~L790-799）
**现状**: 描述 freepdes 临时映射 → 拷贝 → 清除
**修改**: 更新为 VM 可直接通过 direct map 完成跨进程复制，无需 kernel 切换 PDE。`sys_datacopy` 简化
**参考**: ptregion_design.md §8.11

---

## 08-slab-allocator.md — 轻度修改

### TODO-08-1: Slab 对象访问方式更新
**位置**: 如有通过 PtRegion/alloc_virt 访问 slab 对象的描述
**现状**: 可能通过旧机制访问 slab 元数据
**修改**: 更新为通过 `vm_phys_to_virt()` 访问 slab 对象。Slab 元数据在 direct map 中可直接访问
**参考**: ptregion_design.md §8.6

---

## 09-vm-relocation.md — 中度修改

### TODO-09-1: ReservedRegion::from_boot_info 更新
**位置**: ~L1258 `ReservedRegion::from_boot_info(&boot_info)`
**现状**: ReservedRegion 包含 VA 分配功能
**修改**: 更新 ReservedRegion 为纯物理页预留列表，删除 VA 分配相关逻辑
**参考**: ptregion_design.md §8.10 删除清单

### TODO-09-2: PtRegion 搬迁逻辑删除
**位置**: 09-todo.md L52-59 中 PtRegion 搬迁逻辑
**现状**: 描述 `relocate_phys_allocator()` 委托给 pt_region
**修改**: 删除 PtRegion 搬迁逻辑，替换为 direct map 扩展逻辑
**参考**: ptregion_design.md §8.5 Phase 2

---

## 10-phys-block.md — 中度修改

### TODO-10-1: sys_abscopy → direct map memcpy
**位置**: ~L278 `sys_abscopy(old_page, new_page, VM_PAGE_SIZE)`
**现状**: 使用内核系统调用 `sys_abscopy` 复制物理页
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`：
```rust
core::ptr::copy_nonoverlapping(
    vm_phys_to_virt(old_phys),
    vm_phys_to_virt(new_phys),
    4096,
);
```
**参考**: ptregion_design.md §8.7 CoW 复制

### TODO-10-2: PhysBlock 物理页访问方式
**位置**: PhysBlock 中通过旧机制访问物理页内容的描述
**现状**: 可能通过 PtRegion 或 createpde 访问
**修改**: 统一通过 `vm_phys_to_virt(phys)` 访问
**参考**: ptregion_design.md §8.6

---

## 11-memtype.md — 中度修改

### TODO-11-1: sys_abscopy → direct map memcpy
**位置**: ~L1720-1721, ~L1750 中 `sys_abscopy` 引用
**现状**: `on_pagefault` 中 CoW 操作使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7 CoW 复制, §8.10 修改清单

### TODO-11-2: alloc_virtual_space 引用更新
**位置**: ~L4523 `vmp.alloc_virtual_space(length)`
**现状**: 使用 `alloc_virtual_space` 分配虚拟地址空间
**修改**: 确认此函数的语义 — 如果是分配进程虚拟地址空间（mmap 等），则不受影响；如果是分配 VA 来访问物理页，则替换为 `vm_phys_to_virt()`
**参考**: ptregion_design.md §8.6

---

## 12-vir-region.md — 轻度修改

### TODO-12-1: VrParam::Direct 语义确认
**位置**: VrParam::Direct 相关描述
**现状**: `VrParam::Direct` 使用 `PhysBytes` 标记直接映射
**修改**: 确认 VrParam::Direct 在 direct map 下的语义 — 物理页已有 stable VA，Direct 映射的建立更简单
**参考**: ptregion_design.md §8.7

---

## 13-region-avl.md — 无修改

AVL 树实现与 direct map 无关，不需要修改。

---

## 14-phys-region.md — 中度修改

### TODO-14-1: phys_to_virt() 访问方式更新
**位置**: PhysRegion 中访问物理页内容的描述
**现状**: 可能通过旧机制（PtRegion/createpde）访问物理页
**修改**: 统一通过 `vm_phys_to_virt(phys)` 访问。`link_to_block`/`unlink_from_block` 不受影响（操作链表指针，不访问页内容）
**参考**: ptregion_design.md §8.6, §8.7

### TODO-14-2: NonNull deref 模式更新
**位置**: NonNull<T> 相关描述
**现状**: NonNull 指向的物理页内容可能通过旧方式访问
**修改**: NonNull 的 deref 应通过 `vm_phys_to_virt()` 获取的指针。确保所有 `unsafe { (*ptr).field }` 模式使用 direct map 地址
**参考**: ptregion_design.md §8.6

---

## 15-cow-mechanism.md — 重度修改

### TODO-15-1: mem_cow() 核心实现重写
**位置**: §2 mem_cow 核心流程（~L43, L205-206, L232, L250）
**现状**: `sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE)` 复制页面
**修改**: 替换为 direct map memcpy：
```rust
fn mem_cow(pr: &mut PhysRegion) -> Result<(), VmError> {
    let old_phys = pr.get_phys_addr().ok_or(VmError::NoPhysBlock)?;
    let new_phys = bitmap.alloc_mem(1, PageAllocFlags::empty())?;
    unsafe {
        core::ptr::copy_nonoverlapping(
            vm_phys_to_virt(old_phys),
            vm_phys_to_virt(new_phys),
            4096,
        );
    }
    pr.unlink_from_block();
    pr.link_to_block(new_block, parent);
    Ok(())
}
```
**参考**: ptregion_design.md §8.7 CoW 复制

### TODO-15-2: sys_abscopy 说明更新
**位置**: §2 中 `sys_abscopy` 系统调用说明（~L250）
**现状**: "内核提供的物理内存复制接口，直接操作物理地址，无需映射到虚拟地址空间"
**修改**: 更新为 "minix-rs 中 VM 拥有 direct map，可直接通过 `vm_phys_to_virt()` 访问物理页并 memcpy，不再需要 `sys_abscopy` 系统调用"
**参考**: ptregion_design.md §8.7, §8.9

### TODO-15-3: Rust 实现代码更新
**位置**: §4 Rust 实现（~L986, L1164）
**现状**: Rust 代码中仍使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7

### TODO-15-4: vm_bytecopies 说明更新
**位置**: ~L1756 vm_bytecopies 说明
**现状**: 描述 `vm_bytecopies` 与 `sys_abscopy` 的关系
**修改**: 更新为 VM 通过 direct map 直接 memcpy，`vm_bytecopies` 统计逻辑不变
**参考**: ptregion_design.md §8.7

---

## 16-pagefault.md — 中度修改

### TODO-16-1: sys_abscopy → direct map memcpy
**位置**: ~L1316-1317, ~L1338, ~L3103 中 `sys_abscopy` 引用
**现状**: 页错误处理中 CoW 使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7

### TODO-16-2: 页表更新方式更新
**位置**: 页表写入操作描述
**现状**: 可能通过旧机制（PtRegion/createpde）写入页表项
**修改**: 页表项写入通过 `vm_phys_to_virt()` 获取页表页 VA，直接写入
**参考**: ptregion_design.md §8.6, §8.7

---

## 17-vm-fork.md — 轻度修改

### TODO-17-1: fork 页表创建简化
**位置**: fork 中创建子进程页表的描述
**现状**: 可能涉及 PtRegion/pagedir_mappings 步骤
**修改**: fork 创建子进程页表简化为：
1. `bitmap.alloc_mem(1)` → 分配新页目录物理页
2. `vm_phys_to_virt(dir_phys)` → 清零
3. `pt_mapkernel(dir_ptr)` → 建立内核映射（含 kernel direct map）
4. 复制父进程的用户空间映射
**参考**: ptregion_design.md §8.7 创建新进程页表

### TODO-17-2: fork 中 CoW 设置
**位置**: fork 中 CoW 标记设置
**现状**: 使用 `sys_abscopy` 或旧机制
**修改**: CoW 页复制使用 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7 CoW 复制

---

## 18-vm-brk.md — 轻度修改

### TODO-18-1: 物理页分配方式确认
**位置**: brk 中分配物理页的描述
**现状**: 可能通过 `alloc_page()` 分配
**修改**: `alloc_page()` 内部已简化为 `alloc_phys() → vm_phys_to_virt()`，brk 调用方不受影响。确认无需额外修改
**参考**: ptregion_design.md §8.7

---

## 19-vm-map.md — 轻度修改

### TODO-19-1: VM_MAP_PHYS 实现简化
**位置**: VM_MAP_PHYS 服务描述
**现状**: 物理内存映射可能涉及 createpde 临时映射
**修改**: VM_MAP_PHYS 实现简化 — VM 已有 direct map，映射物理地址到进程虚拟地址不需要临时映射窗口
**参考**: ptregion_design.md §8.9

### TODO-19-2: mmap 物理页访问方式
**位置**: mmap 中分配和访问物理页的描述
**现状**: 可能通过旧机制访问物理页
**修改**: 统一通过 `vm_phys_to_virt()` 访问
**参考**: ptregion_design.md §8.6

---

## 01~03 — 无修改或极轻度修改

### TODO-01-1: vmproc 结构确认
**01-vmproc-struct.md**: vmproc 结构本身不受 direct map 影响（`vm_pt` 字段语义不变）。确认无需修改。

### TODO-02-1: vmproc 表确认
**02-vmproc-table.md**: 进程表管理不受 direct map 影响。确认无需修改。

### TODO-03-1: ACL 确认
**03-acl.md**: 访问控制与 direct map 无关。确认无需修改。

---

## 修改优先级

| 优先级 | 文档 | 原因 |
|--------|------|------|
| P0-必须 | 04, 05 | 核心数据结构变更（EarlyHeap/PtRegion 删除，启动流程重写） |
| P0-必须 | 07 | 页表操作是核心路径，pagedir_mappings/createpde 删除影响大 |
| P0-必须 | 15 | CoW 是核心机制，sys_abscopy 替换为 direct map memcpy |
| P1-重要 | 06, 10, 11, 14, 16 | 页表结构/物理页访问方式变更 |
| P2-一般 | 08, 09, 12, 17, 18, 19 | 间接影响，修改量小 |
| P3-确认 | 01, 02, 03, 13 | 大概率无需修改，需确认 |
