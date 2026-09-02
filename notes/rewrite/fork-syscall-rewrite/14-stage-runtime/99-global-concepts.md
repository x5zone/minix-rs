# 99-global-concepts: 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（所有文档共享）
> **源码**: `minix3/minix/include/minix/type.h`、`ipc.h`、`com.h`、`endpoint.h`、`const.h`、`config.h`
> **Rust 模块**: `os/libs/minix-types`
> **draft 素材**: 无（新建）

## 核心点

- endpoint/generation 语义（type.h/endpoint.h）
- message 布局与 56 字节负载约束（ipc.h，_ASSERT_MSG_SIZE）
- 服务号常量（com.h：PM=0/VFS=1/RS=2/VM=8/MIB=7）、minix_ipcvecs 结构
- 全局状态表（_minix_kerninfo/_minix_ipcvecs 等）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制（见 00~13）
