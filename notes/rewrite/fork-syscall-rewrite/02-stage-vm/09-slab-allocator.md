# 09-slab-allocator: 内核堆分配（slab → HeapArena）

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `__minix_init()` 前置（堆可用分界线）
> **源码**: `minix3/minix/servers/vm/slaballoc.c`（528 行）
> **Rust 模块**: `heap_arena.rs`、`global.rs`
> **draft 素材**: `draft/08-slab-allocator.md`（素材）

## 核心点

- slab 分配器 → `HeapArena` + 全局 `VmAllocator`（ARCH A-3；slab 有意省略，需给出理由）
- `SLABALLOC`/`SLABFREE` 宏、MEMPROTECT 语义
- `slaballoc`/`slabfree`/`slabstats`

## 边界

- **前置依赖**: 01（`__minix_init` 堆可用分界线）
- **不覆盖（移交）**: 元数据搬迁（10）
