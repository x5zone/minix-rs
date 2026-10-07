# 05-stage-vfs 旧主线素材归档（draft/）

> **状态**: 归档。本目录为 **fork 主线**时代的全部旧文档与素材，已由 `../plan.md`（新主线：VFS server 启动顺序）取代。
> **使用规则**: 正式文档绝不直接引用本目录内容；仅作为素材按 `../plan.md` §2 的 "draft 来源" 列检索。

## 素材索引（旧主线 → 新文档）

| 旧文档 | 内容 | 新文档归属（plan.md §2） |
|--------|------|--------------------------|
| `00-vfs-overview.md` | VFS 架构总览（fork 视角） | 00 |
| `01-fproc-struct.md` | fproc 结构基本字段 | 02 |
| `02-fproc-flags.md` | fp_flags 与阻塞状态 | 02 |
| `03-fproc-cred.md` | 凭证字段（uid/gid/umask） | 02 |
| `04-filp-struct.md` | filp 文件表条目 | 04 |
| `05-vnode-struct.md` | vnode 结构 + 引用计数 | 05 |
| `06-vmnt-struct.md` | vmnt 挂载点结构 | 06 |
| `07-tll-lock.md` | 三级锁机制 | 07 |
| `08-worker-thread.md` | 工作线程框架 | 08 |
| `09-globals-const.md` | 全局变量与常量 | 03/19/99 |
| `10-main-loop.md` | 主循环与消息分发 | 01/09 |
| `11-service-pm.md` | PM 消息处理与 fork 路由 | 10 |
| `12-pm-fork-copy.md` | pm_fork fproc 复制 | 10 |
| `13-pm-fork-filp.md` | pm_fork filp 引用计数 | 10 |
| `14-pm-fork-vnode.md` | pm_fork vnode 引用计数 | 10 |
| `15-pm-fork-flags.md` | pm_fork 标志重置 | 10 |
| `16-pm-fork-reply.md` | VFS-PM fork 回复协议 | 10 |
| `17-filedes.md` | 文件描述符管理（close 路径） | 14 |
| `18-comm.md` | VFS 进程间通信 | 11 |
| `19-pm-exit.md` | pm_exit（fork 逆操作） | 10 |
| `99-global-concepts.md` | 全局概念暂存区 | 99 |
| `fproc-design.md` | fproc 设计（早期） | 02 |
| `pm-fork-impl.md` | pm_fork 实现（早期） | 10 |
| `fd-table-copy.md` | fd 表复制（提纲） | 14 |
| `filp-refcount.md` | filp 引用计数（提纲） | 04 |
| `vnode-refcount.md` | vnode 引用计数（提纲） | 05 |

## 说明

- 本目录无 checklist.md——05-stage-vfs 从未建立函数级检查表；覆盖基线以 `../plan.md` §5 为准。
- 旧文档中基于 fork 主线的前向引用/补述在归档后不再维护；新主线从零按 `../plan.md` 改写。
