# 05-physical-memory: 物理内存布局与分配器

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `get_mem_chunks()` → `mem_init(mem_chunks)`
> **源码**: `minix3/minix/servers/vm/alloc.c`、`utility.c:get_mem_chunks`
> **Rust 模块**: `phys_mem/*`、`alloc_stats.rs`
> **draft 素材**: `draft/04-physical-memory.md`（素材）
> **变更**: 补 `get_mem_chunks`（原 draft/26 素材，语义归本稿）

## 核心点

- `get_mem_chunks`、`mem_init`、`alloc_mem`/`free_mem`
- `reservedqueue_*` 保留队列、`memstats`/`printmemstats`、`usedpages_*`
- `mem_add_total_pages`（定义）
- ARCH A-5：单一位图 → bitmap + buddy + segment-tree 三实现，`PhysAllocator` trait

## 边界

- **前置依赖**: 00/01（`get_mem_chunks` 调用点）
- **不覆盖（移交）**: `vm_allocpage` 页分配（06）
