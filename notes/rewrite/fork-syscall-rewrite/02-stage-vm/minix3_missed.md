# Minix3 VM Server 逻辑遗漏检查

> **生成日期**: 2026-05-08
> **方法**: 对照 minix3/minix/servers/vm/ 源码与 01-27 文档 §2 章节，找出文档 §2 中描述但 §3-§4 中未覆盖的 Minix3 逻辑

---

## 1. pagetable.c — 遗漏函数

### 1.1 `vm_pagelock()` — 页面锁定
**源码**: `pagetable.c:403`
**功能**: 锁定/解锁 VM 自身地址空间中的页面，防止被换出
**文档状态**: 未提及
**重要性**: P2 — minix-rs 当前无 swap 机制，但未来需要

### 1.2 `vm_addrok()` — 地址有效性检查
**源码**: `pagetable.c:440`
**功能**: 检查 VM 自身地址空间中的地址是否可访问（读/写）
**文档状态**: 未提及
**重要性**: P2 — 调试和 sanity check 用途

### 1.3 `pt_ptalloc_in_range()` — 范围内页表分配
**源码**: `pagetable.c:545`
**功能**: 在指定虚拟地址范围内分配页表页
**文档状态**: 07 §2.3.3 间接提及（vm_mappages 调用链），但未单独描述
**重要性**: P2 — 被 pt_writemap 内部调用，Rust Paging::map() 内部处理

### 1.4 `pt_map_in_range()` — 范围内页表映射复制
**源码**: `pagetable.c:631`
**功能**: 在指定虚拟地址范围内复制页表映射（fork 用）
**文档状态**: 17 §2 间接提及（map_proc_copy 调用链），但未单独描述
**重要性**: P1 — fork 核心路径，Rust 实现需覆盖

### 1.5 `pt_ptmap()` — 页表映射复制
**源码**: `pagetable.c:685`
**功能**: 复制页表映射（fork 用）
**文档状态**: 17 §2 间接提及
**重要性**: P1 — fork 核心路径

### 1.6 `pt_writable()` — 页面可写检查
**源码**: `pagetable.c:761`
**功能**: 检查指定虚拟地址在进程页表中是否可写
**文档状态**: 未提及
**重要性**: P2 — CoW 判断辅助函数

### 1.7 `pt_copy()` — 页表复制
**源码**: `pagetable.c:1069`
**功能**: 复制页表结构（fork 用）
**文档状态**: 17 §2 间接提及
**重要性**: P1 — fork 核心路径

### 1.8 `freepde()` — 空闲 PDE 获取
**源码**: `pagetable.c:1028`
**功能**: 从备用页目录池获取空闲 PDE slot
**文档状态**: 07 §2.1 提及 pagedir_mappings 机制
**重要性**: P3 — 方案四中 direct map 替代，不需要

---

## 2. region.c — 遗漏函数

### 2.1 `map_pin_memory()` — 内存锁定
**源码**: `region.c:779`
**功能**: 锁定进程的所有内存页，防止被换出
**文档状态**: 未提及
**重要性**: P2 — minix-rs 当前无 swap，但 mlock 系统调用需要

### 2.2 `copy_abs2region()` — 绝对地址复制到区域
**源码**: `region.c:860`
**功能**: 将物理地址内容复制到虚拟区域的指定偏移
**文档状态**: 10 §2 间接提及（sys_abscopy 调用），但未单独描述
**重要性**: P1 — exec 路径中加载新程序内存使用

### 2.3 `map_writept()` — 写入页表
**源码**: `region.c:906`
**功能**: 将进程所有区域的映射写入页表（CoW 准备后调用）
**文档状态**: 15 §2 间接提及
**重要性**: P1 — fork 后设置 CoW 的关键步骤

### 2.4 `map_region_extend_upto_v()` — 区域扩展
**源码**: `region.c:1002`
**功能**: 扩展进程的堆区域到指定虚拟地址
**文档状态**: 18 §2 描述了 brk 逻辑，但未单独描述此函数
**重要性**: P2 — brk 实现的内部函数

### 2.5 `split_region()` — 区域分割
**源码**: `region.c:1150`
**功能**: 将虚拟区域在指定偏移处分割为两个区域
**文档状态**: 23 §2 间接提及（munmap 路径），但未单独描述
**重要性**: P1 — munmap/mprotect 核心操作

### 2.6 `map_unmap_range()` — 范围取消映射
**源码**: `region.c:1222`
**功能**: 取消指定虚拟地址范围的映射
**文档状态**: 23 §2 描述了 unmap_region，但未描述范围操作
**重要性**: P1 — munmap 核心路径

### 2.7 `map_get_phys()` — 获取物理地址
**源码**: `region.c:1323`
**功能**: 获取指定虚拟地址对应的物理地址
**文档状态**: 未提及
**重要性**: P2 — VM_MAP_PHYS_GET 系统调用需要

### 2.8 `map_get_ref()` — 获取引用计数
**源码**: `region.c:1343`
**功能**: 获取指定虚拟地址对应的物理页引用计数
**文档状态**: 未提及
**重要性**: P2 — 调试和监控用途

### 2.9 `get_usage_info()` / `get_region_info()` — 使用统计
**源码**: `region.c:1395`, `region.c:1452`
**功能**: 获取进程内存使用统计信息
**文档状态**: 未提及
**重要性**: P2 — VM_INFO 系统调用需要

### 2.10 `map_setparent()` — 设置父进程
**源码**: `region.c:1535`
**功能**: 设置进程的父进程标记（fork 后调用）
**文档状态**: 未提及
**重要性**: P2 — fork 后的辅助操作

---

## 3. utility.c — 遗漏函数

### 3.1 `do_info()` — 信息查询
**源码**: `utility.c:100`
**功能**: 处理 VM_INFO 系统调用，返回内存使用统计
**文档状态**: 未提及
**重要性**: P2 — 系统调用接口

### 3.2 `swap_proc_dyn_data()` — 进程动态数据交换
**源码**: `utility.c:312`
**功能**: 交换两个进程槽位的动态数据（live update 用）
**文档状态**: 02 §2.3 提及 swap_proc_slot，但未描述动态数据交换
**重要性**: P2 — RS 重启机制需要

### 3.3 `do_getrusage()` — 资源使用查询
**源码**: `utility.c:426`
**功能**: 处理资源使用查询系统调用
**文档状态**: 未提及
**重要性**: P3 — 可后续添加

### 3.4 `adjust_proc_refs()` — 进程引用调整
**源码**: `utility.c:477`
**功能**: 调整进程引用计数（共享内存相关）
**文档状态**: 未提及
**重要性**: P2 — 共享内存管理

---

## 4. rs.c — 完全未覆盖

### 4.1 `do_rs_set_priv()` — RS 权限设置
**源码**: `rs.c:34`
**重要性**: P2 — RS 重启机制

### 4.2 `do_rs_prepare()` — RS 准备
**源码**: `rs.c:71`
**重要性**: P2 — RS 重启机制

### 4.3 `do_rs_update()` — RS 更新
**源码**: `rs.c:150`
**重要性**: P2 — RS 重启机制

### 4.4 `do_rs_memctl()` — RS 内存控制
**源码**: `rs.c:349`
**重要性**: P2 — RS 重启机制

**说明**: rs.c 整个文件在 01-27 文档中未覆盖。这是 Minix3 的 RS（Restart Server）交互逻辑，包括 VM 实例创建、堆预分配、映射预分配等。minix-rs 需要决定是否支持 RS 重启机制。

---

## 5. vfs.c — 部分覆盖

### 5.1 `vfs_request()` — VFS 请求
**源码**: `vfs.c:60`
**文档状态**: 24 §2 描述了 VFS 交互，但未详细描述请求队列实现
**重要性**: P1 — 文件映射核心路径

### 5.2 `do_vfs_reply()` — VFS 回复处理
**源码**: `vfs.c:109`
**文档状态**: 24 §2 间接提及
**重要性**: P1 — 文件映射核心路径

---

## 6. mem_cache.c — 完全未覆盖

### 6.1 `mem_type_cache` — 缓存内存类型
**源码**: `mem_cache.c:39`
**功能**: 缓存内存类型的 mem_type 实现，包括 cache_pagefault、cache_reference、cache_unreference、cache_resize、cache_lowshrink
**文档状态**: 26 §2 提及了 cache memtype，但未详细描述 cache_lowshrink 等函数
**重要性**: P1 — 缓存回收路径

---

## 7. fdref.c — 完全未覆盖

### 7.1 fdref 引用计数
**源码**: `fdref.c`, `fdref.h`
**功能**: 文件描述符引用计数管理，用于文件映射的 fork/exit
**文档状态**: 24 §2 间接提及 fdref，但未详细描述引用计数机制
**重要性**: P1 — 文件映射 fork/exit 核心路径

---

## 8. mmap.c — 未单独覆盖

### 8.1 `do_mmap()` — mmap 系统调用
**源码**: `mmap.c`
**文档状态**: 19 §2 描述了 map_region，但未描述 do_mmap 的完整系统调用处理
**重要性**: P1 — mmap 系统调用入口

---

## 9. 遗漏汇总

| 优先级 | 遗漏项 | 文件 | 说明 |
|--------|--------|------|------|
| P1 | pt_map_in_range / pt_ptmap | pagetable.c | fork 页表复制核心路径 |
| P1 | copy_abs2region | region.c | exec 加载新程序内存 |
| P1 | map_writept | region.c | fork 后 CoW 页表设置 |
| P1 | split_region | region.c | munmap/mprotect 区域分割 |
| P1 | map_unmap_range | region.c | munmap 范围操作 |
| P1 | vfs_request / do_vfs_reply | vfs.c | 文件映射核心路径 |
| P1 | mem_type_cache (cache_lowshrink) | mem_cache.c | 缓存回收路径 |
| P1 | fdref 引用计数 | fdref.c | 文件映射 fork/exit |
| P1 | do_mmap | mmap.c | mmap 系统调用入口 |
| P2 | vm_pagelock / vm_addrok | pagetable.c | 页面锁定和地址检查 |
| P2 | map_pin_memory | region.c | mlock 系统调用 |
| P2 | map_get_phys / map_get_ref | region.c | VM_MAP_PHYS_GET 等查询 |
| P2 | get_usage_info / get_region_info | region.c | VM_INFO 系统调用 |
| P2 | rs.c 全部函数 | rs.c | RS 重启机制 |
| P2 | swap_proc_dyn_data | utility.c | Live update |
| P2 | adjust_proc_refs | utility.c | 共享内存引用调整 |
| P3 | pt_writable | pagetable.c | CoW 辅助 |
| P3 | do_getrusage | utility.c | 资源查询 |
| P3 | freepde | pagetable.c | 方案四不需要 |

---

## 10. 与上次 minix3_missed.md 的差异

上次生成的 minix3_missed.md 主要关注 Phase A 的文档修复遗漏。本次重新生成基于完整的 01-27 review，重点关注：

1. **新增**: rs.c 整个文件的遗漏（RS 重启机制）
2. **新增**: fdref.c 引用计数机制的遗漏
3. **新增**: mem_cache.c 的 cache_lowshrink 函数遗漏
4. **新增**: mmap.c 系统调用入口的遗漏
5. **保留**: region.c 中多个函数的遗漏（split_region, map_unmap_range 等）
6. **保留**: pagetable.c 中 fork 相关函数的遗漏

**方案四影响**: 上述遗漏项中，freepde 等函数在方案四中不需要（direct map 替代），但 fork/exec/munmap 相关函数仍然需要实现。
