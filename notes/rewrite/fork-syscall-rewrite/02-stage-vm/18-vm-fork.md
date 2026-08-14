# 18-vm-fork: VM_FORK 服务

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `VM_FORK`
> **源码**: `minix3/minix/servers/vm/fork.c`（116 行）、`region.c:map_proc_copy`
> **Rust 模块**: `fork.rs`、`vm_server.rs`
> **draft 素材**: `draft/16-vm-fork.md`（素材）
> **变更**: 绘制 fork 次主线路径图（03→04→06→07/08→11→12→13→17→16）

## 核心点

- `do_fork` 全流程、endpoint 合成
- `map_proc_copy`、`pt_ptmap`、`acl_fork`
- **fork 次主线路径图**（plan §1.3）

## 边界

- **前置依赖**: 02/03/04/06/07/08/11/12/13/17
- **不覆盖（移交）**: CoW 机制（17）、页错误（16）
