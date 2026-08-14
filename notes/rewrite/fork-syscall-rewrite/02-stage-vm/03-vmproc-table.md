# 03-vmproc-table: 进程表与 endpoint 验证

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `vm_slot` 赋值；主循环 → `vm_isokendpt` caller 验证
> **源码**: `minix3/minix/servers/vm/glo.h`、`utility.c:vm_isokendpt`
> **Rust 模块**: `vmproc/table.rs`
> **draft 素材**: `draft/02-vmproc-table.md`（素材）

## 核心点

- `vmproc[VMP_NR]` 全局进程表、slot 分配与查找
- `vm_isokendpt` endpoint 验证语义
- `VMP_EXECTMP` 语义

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 结构字段细节（02）
