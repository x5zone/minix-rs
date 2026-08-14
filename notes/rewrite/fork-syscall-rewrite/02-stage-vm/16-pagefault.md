# 16-pagefault: 页错误处理

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → `VM_PAGEFAULT` 分发 → `do_pagefaults`
> **源码**: `minix3/minix/servers/vm/pagefaults.c`（418 行）
> **Rust 模块**: `cow_exec_pf.rs`、`vm_server.rs`
> **draft 素材**: `draft/15-pagefault.md`（素材）

## 核心点

- `do_pagefaults`、`handle_pagefault`
- `handle_memory_start/once/step/final/continue`、`do_memory` 状态机
- `pf_errstr`、VFS 回调（`vfs_callback_t`）

## 边界

- **前置依赖**: 13/15
- **不覆盖（移交）**: CoW 分裂机制（17）
