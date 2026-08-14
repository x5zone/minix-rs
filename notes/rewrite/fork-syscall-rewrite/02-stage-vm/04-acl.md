# 04-acl: 访问控制

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `acl_init()`
> **源码**: `minix3/minix/servers/vm/acl.c`（129 行）
> **Rust 模块**: `acl.rs`
> **draft 素材**: `draft/03-acl.md`（素材）

## 核心点

- `acl_init`/`acl_check`/`acl_set`/`acl_fork`/`acl_clear` 语义
- ACL 位图语义、DEFAULT/SYSTEM 分层

## 边界

- **前置依赖**: 01（`acl_init` 调用点）
- **不覆盖（移交）**: dispatch 中 `acl_check` 接线（15）
