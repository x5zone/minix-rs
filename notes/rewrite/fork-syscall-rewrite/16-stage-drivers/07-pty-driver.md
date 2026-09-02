# 07-pty-driver — 伪终端驱动 pty

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

伪终端对 master/slave（pty.c + pty/tty.c）、pty 挂起/取消、与 ptyfs 交互（PTYFS_SET/PTYFS_DEL → 15-stage-fs/20-ptyfs）。C: drivers/tty/pty/pty.c + tty.c（ptyfs.c 归 15）。Rust: os/drivers/tty/pty。

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: ptyfs 树语义（15-stage-fs/20）；VFS 侧 /dev/pts。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
