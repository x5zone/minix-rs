# 03-runtime-init: 运行时初始化（kerninfo/IPC vecs/TLS）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 — 运行时初始化
> **源码**: `minix3/minix/lib/libc/sys/init.c`、`environ.c`、`libc/gen/_errno.c`、`getprogname.c`/`setprogname.c`
> **Rust 模块**: `os/libs/minix-rt`（`init`）、`os/libs/minix-types`（Errno）
> **draft 素材**: 无（新建）

## 核心点

- __minix_init constructor：ipc_minix_kerninfo 获取 + KERNINFO_MAGIC 校验 + MINIX_KIF_IPCVECS 时安装 minix_ipcvecs（init.c:15-35）
- _minix_kerninfo / _minix_ipcvecs 全局状态与 Rust 等价物
- ARCH A-5：errno 槽（C 全局 errno + __errno()）→ Rust Result<_, Errno>
- ARCH A-4：用户态 TLS（i386 无 → x86-64 FS 段 + 架构 trait）
- environ/__progname/setprogname/getprogname 初始化

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: 分配器（06）、panic（07）、IPC 陷阱实现（04）
