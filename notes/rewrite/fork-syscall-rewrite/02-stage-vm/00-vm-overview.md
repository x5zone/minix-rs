# 00-vm-overview: VM 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/vm/`（全部 24 个 .c）
> **Rust 模块**: `os/servers/vm/src/` 全部
> **draft 素材**: `draft/00-vm-overview.md`（素材）

## 核心点

- VM 是什么：用户态服务进程，单线程事件循环（与 Kernel 的 SMP+BKL 模型不同）
- 启动主线图：`init_vm()` 各步骤 → 主循环 dispatch（plan §1.2）
- 文档导航：9 阶段 28 篇，新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH A-6：地址空间宽度 32 位 → 64 位（`MMAP_BASE`/`MMAP_TOP`）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~26、99）
