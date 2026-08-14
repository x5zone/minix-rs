# 17-cow-mechanism: 写时复制（CoW）

> **状态**: pending（最小骨架，待改写）
> **定位**: 页错误路径（16）的分裂机制；fork 时写保护设置
> **源码**: `minix3/minix/servers/vm/pb.c:mem_cow`、`mem_anon.c`
> **Rust 模块**: `cow_exec_pf.rs`、`region/page_state.rs`
> **draft 素材**: `draft/14-cow-mechanism.md`（素材）

## 核心点

- `mem_cow` 语义、写保护设置、引用计数分裂
- `writable` 回调

## 边界

- **前置依赖**: 11/12
- **不覆盖（移交）**: file-backed `cow_block`（23）
