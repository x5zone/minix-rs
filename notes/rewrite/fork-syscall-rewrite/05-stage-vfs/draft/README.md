# Stage 4: VFS 层实现

> 本目录包含 VFS 层 fork 实现的详细技术文档。

---

## 文档索引

| 文档 | 内容 | 对应阶段 |
|------|------|---------|
| [fproc-design.md](./fproc-design.md) | FProc 结构体设计、C源码分析、Rust实现 | 阶段9 |
| [vfs-fork-impl.md](./vfs-fork-impl.md) | pm_fork 实现、文件描述符复制、引用计数 | 阶段9 |

---

## 快速导航

### 阶段9: VFS fproc 结构体与 pm_fork
→ 查看 [fproc-design.md](./fproc-design.md)
- C 源码分析（fproc.h, misc.c）
- 文件描述符表设计
- FD_CLOEXEC 位图

→ 查看 [vfs-fork-impl.md](./vfs-fork-impl.md)
- pm_fork 完整实现
- filp 引用计数管理
- vnode 引用计数管理

---

## Minix3 源码参考

| 文件 | 位置 | 内容 |
|------|------|------|
| `servers/vfs/fproc.h` | 第 1-80 行 | fproc 结构体定义 |
| `servers/vfs/misc.c` | 第 1-50 行 | pm_fork 实现 |

---

## 实现检查清单

- [ ] FProc 结构体定义
- [ ] 文件描述符表实现
- [ ] FD_CLOEXEC 位图
- [ ] pm_fork 核心逻辑
- [ ] filp 引用计数管理
- [ ] vnode 引用计数管理
