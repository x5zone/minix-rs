# 19-vm-brk: VM_BRK 服务

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `VM_BRK`
> **源码**: `minix3/minix/servers/vm/break.c`（69 行）
> **Rust 模块**: `brk.rs`
> **draft 素材**: `draft/17-vm-brk.md`（素材）

## 核心点

- `do_brk`/`real_brk`：堆扩展/收缩
- `DATA_CHANGED`/`STACK_CHANGED` 通知
- brk 与 region 边界语义

## 边界

- **前置依赖**: 13/15
- **不覆盖（移交）**: mmap 地址空间分配（20）
