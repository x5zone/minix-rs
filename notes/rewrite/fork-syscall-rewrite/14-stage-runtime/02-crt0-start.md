# 02-crt0-start: 程序入口与静态链接启动

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 — 内核交付与程序启动
> **源码**: `minix3/lib/csu/common/crt0-common.c`、`minix3/lib/csu/arch/x86_64/crt0.S`、`minix3/lib/csu/common/crtbegin.c`
> **Rust 模块**: `os/libs/minix-rt`（`_start`）
> **draft 素材**: 无（新建）

## 核心点

- ___start 全流程：environ 处理、preinit/init_array/fini_array、_libc_init、main 调用与返回值→exit（crt0-common.c）
- x86_64 crt0.S：栈对齐、__start→___start 参数搬运
- argc/argv/environ 来源与 __progname/__ps_strings 初始化
- ARCH A-1：静态链接（无 ld.elf_so/libc.so，_DYNAMIC 弱引用路径消除）

## 边界

- **前置依赖**: 01（kerninfo ABI）
- **不覆盖（移交）**: kerninfo 获取与 IPC vecs 安装（03）、allocator 初始化（06）、exit 消息语义（08）
