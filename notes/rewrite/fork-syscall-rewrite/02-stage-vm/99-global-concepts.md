# 99-global-concepts: 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（所有文档共享）
> **源码**: `minix3/minix/servers/vm/com.h`、`vm.h`、`glo.h`
> **Rust 模块**: `os/libs/minix-types`
> **draft 素材**: `draft/99-global-concepts.md`（素材）

## 核心点

- endpoint/generation 语义
- `VM_*`/`VMP_*` 常量表、全局变量表

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制（见 00~26）
