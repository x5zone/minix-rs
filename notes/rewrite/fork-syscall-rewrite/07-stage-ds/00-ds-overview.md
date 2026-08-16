# 00-ds-overview: DS 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/ds/`（2 个 .c，811 行）+ boot 证据（`kernel/table.c:44-64`、`kernel/main.c:196,265-267`）
> **Rust 模块**: `os/servers/ds/` 全部
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- DS 是什么：发布/订阅数据存储服务，系统服务的动态注册中心（label→endpoint、驱动状态、核心服务状态）
- 启动主线图：boot 登记 → `sef_cb_init_fresh`（RS 握手）→ 主循环 dispatch（plan §1.2）
- boot 两层语义：登记顺序第一（`table.c:52`）vs 执行顺序 VMINHIBIT（`kernel/main.c:265-267`）
- 文档导航：14 篇（00 + 01~12 + 99），新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~12、99）
