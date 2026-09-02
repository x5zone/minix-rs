# 00-runtime-overview: 用户态运行时整体概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/lib/csu/`、`minix3/minix/lib/libc/`、`minix3/minix/lib/libminc/`、`minix3/minix/lib/libsys/`（共享部分）、`minix3/minix/include/`（用户态 ABI）
> **Rust 模块**: `os/libs/minix-rt`、`os/libs/minix-sys`、`os/libs/minix-types`
> **draft 素材**: `draft/README.md`（素材）

## 核心点

- runtime 是什么：一切 userland（server/fs/driver/命令）共享的运行时库层，不是 server（无主循环/IPC 分发）
- 生命周期主线图：内核交付 → crt0 → 运行时初始化 → syscall 服务 → 终局（plan §1.1）
- 文档导航：6 阶段 15 篇，新编号交叉引用规则（plan §3.2）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH A-2：no_std + Rust core/alloc 替代 libc（libminc/Makefile 组成为排除基线）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~13、99）
