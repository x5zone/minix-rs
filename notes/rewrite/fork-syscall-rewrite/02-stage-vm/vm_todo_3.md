# VM 文档 Review 修改记录 (17-19)

## 17-vm-fork.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.4 vmproc | 虚构vm_stack_low字段、vm_flags类型u32_t→int、缺少vm_boot等6个字段 | 字段虚构和类型错误 |
| §1.4 vir_region | flags类型u32_t→u16_t、def_memtype类型→mem_type_t*、缺少param和AVL节点 | 类型和字段错误 |
| §1.4 phys_block | refcount类型u32_t→u8_t、缺少flags字段 | 类型错误和字段遗漏 |
| §1.4 pt_t | pt_dir_phys类型→u32_t、pt_pt类型→u32_t*、pt_virtop类型→u32_t | 类型全部错误 |
| §2.2 VM_FORK消息 | 值7→VM_RQ_BASE+1=0xC01；字段m4_l1→m1_i1/m1_i2/m1_i3 | 消息值和字段全部错误 |
| §2.6.1 vm_isokendpt | 4步验证逻辑完全虚构，EINVAL→EDEADEPT | 函数实现虚构 |
| §2.8.4 map_writept | vr->phys链表遍历→实际两层函数；physblock_pt_flags()不存在；PTE_W→PTF_WRITE | 代码虚构和标志名错误 |
| §2.9.2 ACL继承 | acl_fork参数vmp实际是子进程(vmc)→修正注释 | 参数语义错误 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 文档结构 | 合并IPC+C源码→Ch2，删除"总结"章，6章结构 | 违反模板 |
| §1.2 | map_writept显示为独立步骤→实际在map_proc_copy_range内部 | 调用位置错误 |
| §1.4/§2.7.2 | pt_t和页表结构添加32/64位差异说明 | 架构差异未标注 |
| §8 参见 | 4篇→10篇 | 参见不完整 |
| 多处 | 补全C代码行号：fork.c:32、pagetable.c:990、region.c:933等 | 行号缺失 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 多处 | 8个ASCII框图→表格/编号列表 | ASCII图滥用 |
| §6 | 约400行Rust测试代码→测试维度表和关键场景列表 | 测试章节应为要点 |

---

## 18-vm-brk.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.4 | 删除虚构vmproc字段(vm_brk/vm_data_top/vm_stack_low) | 字段不存在于vmproc.h，堆顶地址隐含在vir_region的vaddr+length中 |
| §2.3 getnextvr | AVL_GREATER→AVL_EQUAL+incr_iter | 搜索方式错误 |
| §2.4 map_subfree | memory.c→region.c, refcount--→pb_unreferenced | 文件和实现错误 |
| §2.5 map_unmap_region | 遗漏头部收缩分支和MAP_NONE处理 | 代码不完整 |
| §2.6 map_region_extend_upto_v | 遗漏phys_slot/realloc/ev_resize分支 | 代码不完整 |
| §1 | 删除第1章中的C源码分析 | 第1章概述不得包含C源码分析 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 文档结构 | 8章→7章，合并IPC+C源码 | 违反模板 |
| 多处 | 添加4处32/64位差异标注 | 架构差异未标注 |
| §1 | "调用者:PM,进程自身"→"调用者:用户进程（不经过PM）" | 矛盾信息 |
| §7 参见 | 2篇→8篇 | 参见不完整 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 全文 | 约20个ASCII框图→编号列表/表格 | ASCII图滥用 |
| §6 | 约200行测试代码→测试维度表和关键场景表 | 测试章节应为要点 |

---

## 19-vm-map.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 标题 | VM_MAP/VM_UNMAP→VM_MMAP/VM_MUNMAP | 常量名不存在 |
| 概述表 | 缺少消息类型值，VM_UNMAP_PHYS处理函数说明不准确 | 信息不完整 |
| §3 map_page_region | 函数签名和实现虚构(region_search_free不存在)→region_find_slot+region_new+ev_new+MF_PREALLOC | 完全虚构 |
| §3 mem_type定义 | 字段名.ev_alloc/.ev_free不存在，文件路径mem_type.c不存在→memtype.h真实定义 | 完全虚构 |
| §3 VR标志值 | VR_ANON=0x01/VR_WRITABLE=0x02等→真实值VR_WRITABLE=0x001/VR_ANON=0x100等 | 完全错误 |
| §3 mmap_file_cont | 缺少writable变量初始化，ipc_send无错误检查 | 代码不完整 |
| §3 mappedfile_pagefault | 虚构代码(文件名mem_mappedfile.c不存在)→文字描述+参见 | 完全虚构 |
| §3 map_lookup | AVL_EQUAL→AVL_LESS_EQUAL，参数名prev不存在 | 搜索类型和参数错误 |
| §3 do_map_phys | phys_addr→phaddr, ret_addr→reply；缺少SELF处理和地址对齐 | 字段名错误和逻辑遗漏 |
| §3 map_unmap_range | 虚构简化版→region_start_iter+split_region真实实现 | 完全虚构 |
| §3 map_unmap_region | free_phys_block/free(vr->physblocks)不存在→ev_unreference回调 | 虚构函数 |
| §3 mem_type_directphys | .ev_alloc/.ev_free不存在→phys_copy/phys_unreference/phys_pagefault等 | 完全虚构 |
| §2 munmap POSIX语义 | "Minix3实现会返回错误"→map_unmap_range对未映射地址返回OK(符合POSIX) | 描述不准确 |
| §2 do_munmap | 严重不完整→完整实现(含VM_UNMAP_PHYS/VM_SHM_UNMAP处理) | 代码不完整 |
| §2 mmap libc | "直接向VM发送"→minix_mmap_for间接调用，MAP_THIRDPARTY | 描述不准确 |
| §3 VM_MMAPBASE/VM_MMAPTOP | 虚构固定常量0x40000000/0x70000000→运行时计算值+_MINIX_MAGIC条件编译 | 虚构常量 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 文档结构 | Ch2+Ch3合并→Ch2，后续编号调整 | 违反模板 |
| Rust MmapFlags | MAP_ANONYMOUS=0x20→0x1000等 | 与Minix3源码不一致 |
| Rust AddressSpace | MMAP_BASE=0x4000_0000→64位地址空间值 | 32位值 |
| §6 | 测试代码→测试维度和关键场景 | 测试章节应为要点 |
| §7 参见 | 3篇→8篇 | 参见不完整 |
| §2 VFS协作 | 补充mmap_file_cont回调和ipc_send解除阻塞 | 描述过度简化 |
| §2 VM_MMAPBASE | 添加32/64位差异标注 | 架构差异未标注 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 全文 | 15个ASCII框图→文字/表格/列表 | ASCII图滥用 |
| 页脚 | VM_MAP,VM_UNMAP→VM_MMAP,VM_MUNMAP,VM_MAP_PHYS,VM_UNMAP_PHYS | 常量名错误 |

---

# 未完成 TODO（留待后续处理）

## 全局性TODO

1. **Rust代码与文档同步验证**：多个文档的第3-4章Rust代码未与实际os/servers/vm/src/下的Rust代码交叉验证，可能存在Rust实现与文档描述不一致
2. **Rust测试代码迁移**：17-vm-fork.md、18-vm-brk.md等文档的第3-4章中仍有部分Rust测试代码，建议后续迁移到§6测试章节或独立测试文件
3. **§3-4 Rust代码去重**：14-phys-region.md的§3.2链表安全与§4.4有Rust代码重复，建议去重
4. **行号稳定性**：所有C代码行号引用基于当前Minix3源码快照，若源码更新需重新验证

## 各文档遗留TODO

### 10-phys-block.md
- 验证Rust PhysBlock实现与C源码的完整对应关系
- 补充phys_block在共享内存场景下的生命周期说明

### 11-memtype.md
- §4.2 DirectPhysical.is_writable的Rust实现需与os/servers/vm/src/实际代码验证
- 补充mem_type_shared的ev_new/ev_delete完整流程

### 12-vir-region.md
- §5 fork内容精简后需确认与17-vm-fork.md无信息遗漏
- split_region的r2迁移循环代码需与region.c:1150-1220逐行对照

### 13-region-avl.md
- CAVL宏展开的完整代码需进一步验证
- AVL树在64位下的性能差异需补充基准测试数据

### 14-phys-region.md
- §4/§5中大量Rust测试代码建议后续迁移到§6测试章节
- §3.2链表安全章节与§4.4有Rust代码重复，建议去重

### 15-cow-mechanism.md
- sys_abscopy在minix-rs中的对应实现需验证
- TLB刷新机制在x86-64下的差异需补充

### 16-pagefault.md
- x86-64页错误码的完整处理逻辑需补充
- SPAREPAGES机制与05-vm-allocpage.md的交叉验证

### 17-vm-fork.md
- do_fork中ACL继承的完整流程需与acl.c验证
- pt_new页表分配失败时的清理路径需补充

### 18-vm-brk.md
- do_brk中VM_UNMAP_PHYS分支的完整代码需补充
- brk与栈区域的边界检查逻辑需与Minix3源码验证

### 19-vm-map.md
- mmap_file_cont异步回调的完整流程需补充
- VM_SHM_UNMAP处理逻辑需与共享内存文档交叉验证
- MAP_THIRDPARTY标志在minix-rs中的实现需验证

---

# Rust 代码修改记录

## 已修复的Rust代码问题

### 1. MAP_NONE 值修正（P0）

| 文件 | 修改 | 原因 |
|------|------|------|
| phys_region.rs:20 | `MAP_NONE: u64 = 0` → `0xFFFF_FFFF_FFFF_FFFE` | Minix3使用0xFFFFFFFE作为"未映射"哨兵值，Rust原用0导致物理地址0无法作为有效映射 |

### 2. memtype.rs 物理地址检查统一使用 MAP_NONE（P0）

| 位置 | 修改 | 原因 |
|------|------|------|
| AnonymousMemory::is_writable | `unwrap_or(0) == 0` → `unwrap_or(MAP_NONE) == MAP_NONE` | 与Minix3 anon_writable一致 |
| AnonymousMemory::on_unreference | `unwrap_or(0) != 0` → `unwrap_or(MAP_NONE) != MAP_NONE` | 与Minix3 anon_unreference一致 |
| AnonymousMemory::on_pagefault | `unwrap_or(0) == 0` → `unwrap_or(MAP_NONE) == MAP_NONE` | 与Minix3 anon_pagefault一致 |
| DirectPhysical::is_writable | `unwrap_or(0) != 0` → `unwrap_or(MAP_NONE) != MAP_NONE` | 与Minix3 phys_writable一致 |
| DirectPhysical::on_pagefault | `*base_phys == 0` → `== MAP_NONE`；`unwrap_or(0) != 0` → `unwrap_or(MAP_NONE) != MAP_NONE` | 与Minix3 phys_pagefault一致 |
| SharedMemory::is_writable | `unwrap_or(0) != 0` → `unwrap_or(MAP_NONE) != MAP_NONE` | 与Minix3 shared_writable一致 |

### 3. vir_region.rs VrParam 默认值修正（P1）

| 位置 | 修改 | 原因 |
|------|------|------|
| VrParam::default() | `phys: 0` → `phys: PhysBlock::MAP_NONE` | 默认值应与MAP_NONE一致 |

### 4. vir_region.rs prepare_cow 逻辑修正（P0）

| 位置 | 修改 | 原因 |
|------|------|------|
| prepare_cow() | `refcount>1 && is_writable() && phys_region.is_writable()` → `has_phys_block() && is_writable() && !phys_region.is_writable()` | 原逻辑错误：当phys_region可写时做CoW，实际应在不可写时做CoW。与Minix3的map_ph_writept→pr_writable调用链一致 |

## Rust 代码遗留 TODO

1. **MappedFile 和 Cache memtype 缺失**：memtype.rs 中缺少 `MappedFile` 和 `Cache` 的 MemType 实现，Minix3 有 `mem_type_mappedfile` 和 `mem_type_cache`
2. **AnonContiguous memtype 缺失**：缺少 `AnonContiguous` 的 MemType 实现
3. **pb_new(MAP_NONE) 场景**：region_new 中创建新 phys_block 时应使用 `PhysBlock::new(PhysBlock::MAP_NONE)` 而非 `PhysBlock::new(0)`
4. **MAP_NONE 在页表操作中的使用**：pagetable 模块中可能也有硬编码的 0 需要替换为 MAP_NONE
