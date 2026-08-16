# 00-mib-overview: MIB 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/mib/`（8 个 .c，4990 行）+ boot 证据（`kernel/table.c:58-62`、`kernel/main.c:265-267`）
> **Rust 模块**: `os/servers/mib/` 全部
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- MIB 是什么：sysctl(2) 系统调用的实现者，以对象树（OID）形式维护系统配置/统计信息（NetBSD sysctl 信息模型的 MINIX3 版）
- 启动主线图：boot 登记（`table.c:60`）→ VMINHIBIT 解除 → `mib_init`（子树 init + tree_init + remote_init）→ 主循环 dispatch（plan §1.2）
- sysctl(2) 次主线：用户态 → MIB_SYSCTL → 名字解析 → 节点读写 → ENOMEM 溢出语义（plan §1.3）
- 远程子树次主线：服务注册 → 挂载 → relay 转发 → 死亡恢复 ERESTART（plan §1.3）
- 文档导航：24 篇（00 + 01~22 + 99），新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~22、99）
