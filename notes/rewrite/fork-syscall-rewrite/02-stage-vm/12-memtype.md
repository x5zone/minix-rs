# 12-memtype: 内存类型与回调

> **状态**: pending（最小骨架，待改写）
> **定位**: 地址空间数据结构（vir_region.def_memtype 依赖本稿）
> **源码**: `minix3/minix/servers/vm/memtype.h`、`mem_anon.c`、`mem_anon_contig.c`、`mem_directphys.c`、`mem_shared.c`
> **Rust 模块**: `memtype.rs`
> **draft 素材**: `draft/12-memtype.md`（素材）
> **变更**: 前移至 13 之前（消除 region→memtype 前向引用）

## 核心点

- `mem_type_t` 6 类实例（anon / directphys / anon_contig / cache / mappedfile / shared）
- 15 个 `ev_*` 回调签名与语义
- `mem_cow`（COW 分裂）、`phys_setphys`、`shared_setsource` 类型级语义

## 边界

- **前置依赖**: 11
- **不覆盖（移交）**: 回调在 pagefault/fork/mmap 中的调用流程（16/17/18/20/23）
