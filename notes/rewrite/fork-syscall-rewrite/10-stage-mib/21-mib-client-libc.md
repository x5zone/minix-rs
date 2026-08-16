# 21-mib-client-libc: libc sysctl(3) 客户端契约（外部）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 11 客户端契约（外部，不实现）
> **源码**: `lib/libc/gen/sysctl.c`（391 行）+ `sysctlgetmibinfo.c`（611）+ `sysctlbyname.c`（67）+ `sysctlnametomib.c`（72）+ `minix/lib/libc/sys/__sysctl.c`（63）
> **Rust 模块**: 用户态 libc（**不实现，A-10**）
> **draft 素材**: 无（新建）

## 核心点

- `sysctl(3)`：`name[0]==CTL_USER` → `user_sysctl` 本地子树（libc 内静态表，不进 MIB）；其余 → `__sysctl`
- `__sysctl`：消息构造（oldp/oldlen/newp/newlen/namelen/name 内嵌）+ `_syscall(MIB_PROC_NR, MIB_SYSCTL)` + **oldlen 失败也回写**（libc 侧约定，sysctl(8) 依赖）
- `sysctlgetmibinfo`：QUERY/DESCRIBE 元标识符遍历 + 自排序 + 版本跟踪（`__cvt_node_out`）
- `sysctlbyname`/`sysctlnametomib`：名字 ↔ MIB 数字转换
- ENOMEM 语义在 libc 侧处理：oldlen > savelen → errno=ENOMEM
- A-10：minix-rs 不重写 libc，本文档为外部契约（保证 libc 侧兼容）

## 边界

- **前置依赖**: 02 + 服务器语义
- **不覆盖（移交）**: MIB 服务器内部（03~20）、rmib（22）
