# 07-panic-output: 诊断输出与 panic 路径

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 — 内存分配与终局
> **源码**: `minix3/minix/lib/libsys/kputc.c`、`sys_diagctl.c`、`panic.c`、`assert.c`、`libminc`（`_snprintf.c`/`fputs.c`）、`libc/gen/itoa.c`、`stderr.c`
> **Rust 模块**: `os/libs/minix-rt`（panic handler）、`os/libs/minix-sys`（write/诊断）
> **draft 素材**: 无（新建）

## 核心点

- kputc 缓冲（DIAG_BUFSIZE）+ sys_diagctl（DIAGCTL_CODE_DIAG/STACKTRACE/REGISTER/UNREGISTER）
- libsa printf 家族（subr_prf/printf/strerror）语义 → [ARCH] A-2 core 格式化替代
- minix-rt panic handler：先 spin 后输出（draft README 声明的演进顺序）
- ARCH A-8：诊断输出通道设计决策（见 plan §7.3：panic 走内核通道，正常日志走 VFS）

## 边界

- **前置依赖**: 03、05（write/诊断）
- **不覆盖（移交）**: exit 消息语义（08）、printf 全量实现（[ARCH] core 替代）
