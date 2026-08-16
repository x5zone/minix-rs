# 00-vfs-overview: VFS 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/vfs/`（全部 33 个 .c + 15 个 .h）
> **Rust 模块**: `os/servers/vfs/src/` 全部
> **draft 素材**: `draft/00-vfs-overview.md` + `draft/99-global-concepts.md`（素材）

## 核心点

- VFS 是什么：用户态文件系统服务器，Minix3 唯一多线程（mthread）服务器；单线程事件循环是 minix-rs 的 ARCH A-1 演进
- 启动主线图：`sef_cb_init_fresh`（main.c:393）各步骤 → 主循环 5 路分发（plan §1.2）
- 文档导航：14 阶段 33 篇，新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- 服务面概览：64 个 VFS 调用（table.c）+ 12 个 VFS_PM 请求（com.h:520-531）+ 35 个 REQ_* 协议面（vfsif.h）
- fork 次主线定位：`10-pm-protocol`（plan §1.3）

## 边界

- 不覆盖任何机制细节；一切机制交给 01~31
- 全局常量/术语/引用计数模型交给 99
