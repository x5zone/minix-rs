# 00-is-overview: IS 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/is/`（8 个 .c，1151 行）+ 启动证据（`kernel/table.c:44-64`、`etc/rc.minix:117`、`etc/system.conf:271-277`）
> **Rust 模块**: `os/servers/is/` 全部
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- IS 是什么：调试转储聚合器（Information Server），按功能键触发显示 kernel/PM/VFS/RS/DS/VM 状态
- 启动主线图：启动条件（`rc.minix:117` debug_fkeys）→ RS 运行时加载 → `sef_cb_init_fresh`（fkey 注册）→ 主循环 dispatch（plan §1.2）
- 无 boot_image 登记语义：不在 `table.c:44-64`，endpoint 由 RS 动态分配（无 `IS_PROC_NR`，A-9）
- 文档导航：12 篇（00 + 01~10 + 99），新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~10、99）
