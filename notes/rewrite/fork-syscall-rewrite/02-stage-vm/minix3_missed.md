# Minix3 VM Server — Rust Rewrite 遗漏逻辑清单

> 本文档对照 Minix3 VM 源码（`/workspace/minix3/minix/servers/vm/`）与 Rust 实现（`/workspace/os/servers/vm/src/`），
> 列出所有尚未实现或仅部分实现的关键逻辑。
>
> 优先级：🔴 关键遗漏（影响核心功能） 🟡 部分实现（框架存在但逻辑不完整） 🟢 计划中（设计文档已描述但未实现）

---

## 1. exit.c — 进程退出处理 🔴

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `free_proc(vmp)` | 释放进程所有内存区域和页表 | ❌ 完全缺失 |
| `clear_proc(vmp)` | 清零进程结构体字段 | ❌ 完全缺失 |
| `do_exit(msg)` | 处理 VM_EXIT 请求 | ❌ 完全缺失 |
| `do_willexit(msg)` | 处理 VM_WILLEXIT 请求 | ❌ 完全缺失 |
| `do_procctl(msg, transid)` | 处理 VM_PROCCTL 请求 | ❌ 完全缺失 |

**影响**: 进程退出是 VM 服务的基本功能，缺失将导致物理内存泄漏（PhysBlock 引用计数不减少、页表不释放）。

---

## 2. region.c — 区域管理操作 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `map_region_init()` | 初始化区域管理器 | ✅ `init_regions()` |
| `map_free(region)` | 释放单个区域的所有 PhysBlock | 🟡 `free_range()` 存在但逻辑不完整 |
| `map_free_proc(vmp)` | 释放进程所有区域 | ❌ 缺失（exit.c 需要） |
| `map_pf(vmp, vr, pr, write)` | 页错误核心处理 | 🟡 `on_pagefault()` 存在但 `mem_cow` 未实现 |
| `map_handle_memory(vmp, ...)` | 批量内存处理 | ❌ 缺失 |
| `map_pin_memory(vmp)` | 锁定内存（不可换出） | ❌ 缺失 |
| `map_writept(vmp)` | 写回所有页表映射 | 🟡 `write_page_table_mappings()` 存在但为占位 |
| `map_ph_writept(vmp, vr, pr)` | 写回单个 PhysRegion 的页表 | ❌ 缺失（CoW 核心操作） |
| `map_proc_copy(dst, src)` | 复制进程地址空间 | 🟡 `copy_regions_with_cow()` 存在 |
| `map_region_extend_upto_v(vmp, v)` | 扩展区域到指定地址 | ❌ 缺失（brk 需要） |
| `map_unmap_region(vmp, r, ...)` | 取消区域映射 | ❌ 缺失（munmap 需要） |
| `map_unmap_range(vmp, ...)` | 取消地址范围映射 | ❌ 缺失 |
| `map_get_phys(vmp, addr, r)` | 获取虚拟地址的物理地址 | ❌ 缺失 |
| `map_get_ref(vmp, addr, cnt)` | 获取虚拟地址的引用计数 | ❌ 缺失 |
| `map_setparent(vmp)` | 设置区域父进程指针 | ❌ 缺失 |
| `copy_abs2region(abs, dest, ...)` | 从绝对物理地址复制到区域 | ❌ 缺失 |
| `get_usage_info(vmp, vui)` | 获取进程内存使用信息 | ❌ 缺失 |
| `get_region_info(vmp, vri, ...)` | 获取区域信息 | ❌ 缺失 |
| `map_sanitycheck(file, line)` | 区域一致性检查 | ❌ 缺失 |
| `map_printmap(vmp)` | 打印进程内存映射 | ❌ 缺失（调试用） |

---

## 3. pb.c — 物理块操作 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `pb_free(pb)` | 释放物理块 | 🟡 `release_ref()` 存在但不释放回分配器 |
| `pb_link(newphysr, newpb, ...)` | 链接 PhysRegion 到 PhysBlock | ✅ `link_to_block()` |
| `pb_unreferenced(region, pr, rm)` | 取消 PhysRegion 引用 | 🟡 `unlink_from_block()` 存在但未调用 memtype 回调 |
| `mem_cow(region, pr)` | 执行 Copy-on-Write | ❌ 完全缺失（CoW 核心操作） |

**影响**: `mem_cow` 是 CoW 机制的执行核心，缺失意味着写时复制无法实际执行。

---

## 4. pagetable.c — 页表操作 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `pt_new(pt)` | 创建新页表 | ✅ `init_page_table()` |
| `pt_bind(pt, who)` | 绑定页表到进程 | ✅ `bind_page_table()` |
| `pt_free(pt)` | 释放页表 | ❌ 缺失（exit 需要） |
| `pt_mapkernel(pt)` | 映射内核空间 | ❌ 缺失 |
| `pt_writable(vmp, v)` | 检查虚拟地址是否可写 | ❌ 缺失 |
| `pt_writemap(vmp, ...)` | 写入页表映射 | ❌ 缺失（CoW/映射核心） |
| `pt_map_in_range(src, dst, ...)` | 范围内映射页表 | ❌ 缺失 |
| `pt_ptmap(src, dst)` | 映射整个页表 | ❌ 缺失 |
| `pt_checkrange(pt, v, bytes, ...)` | 检查地址范围权限 | ❌ 缺失 |
| `pt_clearmapcache()` | 清除映射缓存 | ❌ 缺失 |
| `pt_ptalloc_in_range(pt, start, end, ...)` | 在范围内分配页表页 | ❌ 缺失 |
| `vm_freepages(vir, pages)` | 释放虚拟页面 | ❌ 缺失 |
| `vm_pagelock(vir, lockflag)` | 锁定/解锁页面 | ❌ 缺失 |
| `vm_addrok(vir, writeflag)` | 检查地址是否有效 | ❌ 缺失 |
| `pt_sanitycheck(pt, file, line)` | 页表一致性检查 | ❌ 缺失 |
| `pt_allocate_kernel_mapped_pagetables()` | 分配内核映射页表 | ❌ 缺失 |
| `pt_init()` | 页表子系统初始化 | ❌ 缺失 |

---

## 5. pagefaults.c — 页错误处理 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `do_pagefaults(msg)` | 页错误主处理入口 | ❌ 缺失（与内核中断对接） |
| `handle_memory_once(vmp, mem, len, ...)` | 单次内存处理 | ❌ 缺失 |
| `handle_memory_start(vmp, mem, len, ...)` | 启动内存处理 | ❌ 缺失 |
| `do_memory()` | 批量内存请求处理 | ❌ 缺失 |

---

## 6. mmap.c — 内存映射 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `do_mmap(msg)` | 处理 mmap 系统调用 | 🟡 框架存在，核心逻辑不完整 |
| `do_munmap(msg)` | 处理 munmap 系统调用 | ❌ 缺失 |
| `do_map_phys(msg)` | 处理物理内存映射 | ❌ 缺失 |
| `do_remap(msg)` | 处理内存重映射 | ❌ 缺失 |
| `do_get_phys(msg)` | 获取物理地址 | ❌ 缺失 |
| `do_get_refcount(msg)` | 获取引用计数 | ❌ 缺失 |
| `do_vfs_mmap(msg)` | VFS mmap 请求 | ❌ 缺失 |
| `munmap_vm_lin(addr, len)` | 按线性地址取消映射 | ❌ 缺失 |

---

## 7. break.c — brk 系统调用 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `do_brk(msg)` | 处理 brk 系统调用 | 🟡 框架存在，核心逻辑不完整 |
| `real_brk(vmp, v)` | 实际堆调整 | ❌ 缺失（需要 `map_region_extend_upto_v`） |

---

## 8. utility.c — 通用工具函数 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `get_mem_chunks()` | 解析引导内存块信息 | ❌ 缺失（启动初始化） |
| `vm_isokendpt(endpoint, procn)` | 验证端点有效性 | ✅ `vm_isokendpt()` |
| `do_info(msg)` | 处理 VM_INFO 请求 | ❌ 缺失 |
| `swap_proc_slot(src, dst)` | 交换进程槽位 | ✅ `swap_proc_slot()` |
| `swap_proc_dyn_data(src, dst, ...)` | 交换进程动态数据 | ❌ 缺失 |
| `do_getrusage(msg)` | 获取资源使用统计 | ❌ 缺失 |
| `adjust_proc_refs()` | 调整进程引用计数 | ❌ 缺失 |

---

## 9. cache.c — 文件系统缓存 🔴

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `cache_lru_touch(hb)` | 更新 LRU 缓存位置 | ❌ 完全缺失 |
| `addcache(dev, dev_off, ino, ...)` | 添加缓存页 | ❌ 完全缺失 |
| `rmcache(cp)` | 移除缓存页 | ❌ 完全缺失 |
| `cache_freepages(pages)` | 释放缓存页面 | ❌ 完全缺失 |
| `get_stats_info(vsi)` | 获取缓存统计 | ❌ 完全缺失 |

**影响**: 文件系统缓存是 Minix3 VM 的重要功能，缺失影响文件映射性能。

---

## 10. fdref.c — 文件描述符引用 🔴

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `fdref_ref(ref, region)` | 增加文件描述符引用 | ❌ 完全缺失 |
| `fdref_deref(region)` | 减少文件描述符引用 | ❌ 完全缺失 |
| `fdref_sanitycheck()` | 引用一致性检查 | ❌ 完全缺失 |

**影响**: 文件映射内存（mem_file）依赖 fdref 管理文件描述符生命周期。

---

## 11. 内存类型实现 🟡

| Minix3 文件 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `mem_anon.c` | 匿名内存 | ✅ `AnonymousMemory` |
| `mem_anon_contig.c` | 连续匿名内存 | ❌ 缺失 |
| `mem_cache.c` | 缓存内存 | ❌ 缺失 |
| `mem_directphys.c` | 直接物理映射 | 🟡 `DirectPhysicalMemory` 存在但 `phys_setphys` 缺失 |
| `mem_file.c` | 文件映射内存 | 🟡 `MappedFileMemory` 存在但 VFS 交互缺失 |
| `mem_shared.c` | 共享内存 | ❌ 缺失 |

---

## 12. vfs.c — VFS 交互 🔴

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `vfs_request(reqno, fd, vmp, ...)` | 向 VFS 发送请求 | ❌ 完全缺失 |
| `do_vfs_reply(msg)` | 处理 VFS 回复 | ❌ 完全缺失 |

**影响**: 文件映射和页面换入依赖 VFS 交互，缺失导致文件映射无法工作。

---

## 13. rs.c — 资源服务交互 🔴

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `do_rs_set_priv(msg)` | 设置 RS 权限 | ❌ 完全缺失 |
| `do_rs_prepare(msg)` | RS 准备 | ❌ 完全缺失 |
| `do_rs_update(msg)` | RS 更新 | ❌ 完全缺失 |
| `do_rs_memctl(msg)` | RS 内存控制 | ❌ 完全缺失 |

---

## 14. main.c — 主循环与初始化 🟡

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `main()` | VM 主循环 | 🟡 `main.rs` 存在但消息循环不完整 |
| `init_vm()` | VM 初始化 | 🟡 `global::init()` 存在但不完整 |
| `do_sef_init_request(msg)` | SEF 初始化 | ❌ 缺失 |

---

## 15. slaballoc.c — Slab 分配器 🟢

| Minix3 函数 | 功能 | Rust 状态 |
|-------------|------|-----------|
| `slabfree(mem, bytes)` | 释放 slab 对象 | ✅ 由 Rust `global_alloc` 替代 |
| `slablock(mem, bytes)` | 锁定 slab 对象 | ❌ 缺失 |
| `slabunlock(mem, bytes)` | 解锁 slab 对象 | ❌ 缺失 |

**说明**: Minix3 的 slab 分配器已被 Rust 的 `global_alloc` 系统替代，这是设计上的改进。

---

## 关键遗漏优先级排序

### 🔴 P0 — 必须实现（核心功能缺失）

1. **exit.c 全部函数** — 进程退出处理，否则物理内存泄漏
2. **mem_cow** — CoW 执行核心，否则写时复制无法工作
3. **map_ph_writept / pt_writemap** — 页表写入，否则 CoW 页表保护无法设置
4. **VFS 交互** — 文件映射依赖
5. **fdref** — 文件描述符引用管理

### 🟡 P1 — 重要但可延后

6. **map_free_proc** — 释放进程所有区域（exit 依赖）
7. **map_region_extend_upto_v** — brk 堆扩展核心
8. **map_unmap_region / map_unmap_range** — munmap 核心逻辑
9. **pt_free** — 页表释放（exit 依赖）
10. **pt_mapkernel** — 内核空间映射
11. **do_pagefaults** — 页错误主入口
12. **cache.c 全部** — 文件系统缓存
13. **mem_shared** — 共享内存类型
14. **mem_anon_contig** — 连续匿名内存

### 🟢 P2 — 可后续实现

15. **do_info / do_getrusage** — 信息查询
16. **map_sanitycheck / pt_sanitycheck** — 一致性检查
17. **map_printmap** — 调试输出
18. **RS 交互** — 资源服务
19. **do_remap / do_get_phys / do_get_refcount** — 辅助系统调用
20. **handle_memory_once / handle_memory_start** — 批量内存处理
21. **vm_pagelock / vm_addrok** — 页面锁定和验证
22. **swap_proc_dyn_data** — 进程动态数据交换
23. **adjust_proc_refs** — 引用计数调整

---

## 审查中已修复的 Rust 代码问题

| 文件 | 修复内容 | 对应文档 |
|------|----------|----------|
| phys_region.rs | PhysBytes/u16/NonNull 类型对齐 | 10-phys-block |
| memtype.rs | NonNull 解引用、region_id/ref_count 对齐 Minix3 | 11-memtype |
| vir_region.rs | PhysBytes 类型、split_len==0 检查 | 12-vir-region |
| fork.rs | link_phys_blocks 改用 link_to_block（pb_link 等价） | 17-vm-fork |
| vmproc_handle.rs | NonNull 解引用修复 | 10-phys-block |
| 14-phys-region.md | 文档 NonNull/PhysBytes/u16 API 更新 | 14-phys-region |
