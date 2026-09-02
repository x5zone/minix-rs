# 06-allocator: 内存分配器（slab over VM mmap）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 — 内存分配与终局
> **源码**: `minix3/minix/lib/libc/sys/brk.c`、`sbrk.c`、`stdlib/malloc.c`（NetBSD）、`libc/arch/.../brksize.S`
> **Rust 模块**: `os/libs/minix-rt`（`alloc`/`free`）
> **draft 素材**: 无（新建）

## 核心点

- brk/sbrk 用户态视图：_brksize 比较、溢出检测、VM_BRK 交互（brk.c:22-40、sbrk.c）
- NetBSD malloc（brk/mmap 混合、magic 节）→ [ARCH] A-3 slab 分配器（VM mmap 供给小块缓存 + 大块直接 mmap）
- brksize.S 与 _brksize 全局
- 分配器初始化时机与首次分配触发（懒初始化）

## 边界

- **前置依赖**: 03、10（mmap/brk 消息封装接口约定）
- **不覆盖（移交）**: mmap/munmap/brk 消息封装细节（10）、VM 服务端语义（02-stage-vm）、panic（07）
