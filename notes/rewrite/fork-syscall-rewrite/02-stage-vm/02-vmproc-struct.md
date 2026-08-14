# 02-vmproc-struct: 进程槽结构体

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `memset(vmproc)` + `init_proc(VM_PROC_NR)`（main.c:262）
> **源码**: `minix3/minix/servers/vm/vmproc.h`
> **Rust 模块**: `vmproc/*`
> **draft 素材**: `draft/01-vmproc-struct.md`（素材）

## 核心点

- `vmproc` 结构全字段语义
- `VMF_*` 标志位、生命周期状态机
- `init_proc` 槽初始化语义（VM 自身槽 + 普通进程槽）

## 边界

- **前置依赖**: 01（memset 语义）
- **不覆盖（移交）**: 表管理/endpoint 验证（03）
