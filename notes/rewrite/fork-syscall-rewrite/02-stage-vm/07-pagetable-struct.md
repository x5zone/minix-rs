# 07-pagetable-struct: 页表结构

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `pt_init()`
> **源码**: `minix3/minix/servers/vm/pt.h`、`arch/i386/pagetable.h`、`pagetable.c:pt_init`
> **Rust 模块**: `pagetable/mod.rs`、`direct_map.rs`、`pagetable/vm_self_map.rs`
> **draft 素材**: `draft/06-pagetable-struct.md`（素材）
> **变更**: Direct Map 提升为显式章节

## 核心点

- `pt_t` 结构、`ARCH_VM_*` 宏、`pt_init`/`pt_init_mem` 结构面
- **Direct Map 双视图（ARCH A-1）**：`VM_DIRECT_MAP_BASE`/`KERNEL_DIRECT_MAP_BASE`、`DirectMapArch` trait
- `vm_self_map`（ARCH A-9：静态 spare 目录 → 自映射模块）
- ARCH A-2 页表层级（2 级 PD→PT → 4 级 PML4→PDPT→PD→PT）、A-10 多架构 trait（x86-64/arm64/riscv64）

## 边界

- **前置依赖**: 05/06
- **不覆盖（移交）**: pt 操作（08）
