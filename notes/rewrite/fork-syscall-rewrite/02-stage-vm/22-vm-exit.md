# 22-vm-exit: VM_EXIT / WILLEXIT / PROCCTL

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `VM_EXIT`/`WILLEXIT`；VFS transid → `do_procctl`
> **源码**: `minix3/minix/servers/vm/exit.c`（156 行）、`region.c`
> **Rust 模块**: `exit.rs`、`vmproc/vmproc.rs`
> **draft 素材**: `draft/20-vm-exit.md`（素材）

## 核心点

- `do_exit`/`do_willexit`/`do_procctl`
- `free_proc`/`clear_proc`、`reset_vm_rusage`
- `VMPPARAM_*` 语义、VFS transid 路径

## 边界

- **前置依赖**: 03/13/15
- **不覆盖（移交）**: 查询（26）
