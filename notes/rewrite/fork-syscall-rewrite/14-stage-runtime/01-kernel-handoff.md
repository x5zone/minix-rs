# 01-kernel-handoff: 内核交付 ABI 与 exec 初始栈

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 — 内核交付与程序启动（锚点文档）
> **源码**: `minix3/minix/include/minix/type.h`（minix_kerninfo/kuserinfo）、`minix3/minix/lib/libc/sys/kernel_utils.c`、`stack_utils.c`、`lib/csu/` 栈约定
> **Rust 模块**: `os/libs/minix-rt`（kerninfo 访问）、`os/libs/minix-boot`（user_sp 提供方）
> **draft 素材**: 无（新建）

## 核心点

- minix_kerninfo 结构全字段与 userland ABI 约束（type.h:214-247，KERNINFO_MAGIC=0xfc3b84bf）
- kuserinfo 与 KUSERINFO_HAS_FIELD 版本探测（type.h:205-212）
- exec 初始栈构建：minix_stack_params/minix_stack_fill（stack_utils.c）、ps_strings 布局
- get_minix_kerninfo / minix_get_user_sp（kernel_utils.c，kerninfo 访问器）
- ARCH A-7：64 位地址空间（vir_bytes/指针）

## 边界

- **前置依赖**: 00、`../01-stage-kernel/09-vm-boot-protocol.md`
- **不覆盖（移交）**: `_start` 内部流程（02）、kerninfo 获取系统调用实现（04）、IPC vecs 安装（03）
