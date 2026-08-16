# 04: ipc-dispatch

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 主循环与异步协议
> **源码**: minix3/minix/servers/pm/main.c:49/59-110/250-274、minix3/minix/servers/pm/table.c、minix3/minix/include/minix/callnr.h（47 个调用号）
> **Rust 模块**: ipc/dispatcher.rs（部分：仅 Fork）、minix-types/src/ipc/pm.rs（A-4/A-5）
> **draft 素材**: 无（draft 无主循环文档）

## 核心点

主循环（main.c:59-110）、CLOCK notify 跳过、pm_isokendpt 验证、EXITING 丢弃、call_vec 分发（47 个调用）、SUSPEND 不回复、reply()（main.c:250-274）、调用统计

## 边界

- **前置依赖**: 01~03
- **不覆盖（移交）**: VFS 回复处理（05）、事件回复（06）、各 do_xxx 内部逻辑（07~20）
