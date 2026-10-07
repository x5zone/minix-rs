# 14-stage-runtime — 用户态运行时/标准库（占位）

> **状态**: 占位（目录已建，待规划实装）
> **定位**: minix-rs 用户态共享运行时的实装 stage——一切 userland（server/fs/driver/命令）的共享前置

## 范围
- `os/libs/minix-rt` 实装：allocator（VM mmap 供给的 slab）、panic 输出、`_start`/argv、TLS
- `os/libs/minix-sys` 实装：fork/exec/exit/waitpid/read/write/open/mmap/kill 等 syscall 封装
- errno/termios 等常量对齐（`minix3/minix/include/` + `minix3/minix/lib/libc/sys/`）
- `[ARCH]` 决策：用户态静态链接（minix3 无 `libc.so`，`libexec/ld.elf_so` 不需要）

## C 对应
- `minix3/minix/lib/libc/`、`minix3/minix/lib/libminc/`、`minix3/lib/csu/`

## 占位现状
- `os/libs/minix-rt/`（stub：alloc 返回 null、panic 死循环）
- `os/libs/minix-sys/`（stub：syscall 封装全部 `todo!()`）

## 依赖
- 无（所有其他 stage 依赖本 stage）
