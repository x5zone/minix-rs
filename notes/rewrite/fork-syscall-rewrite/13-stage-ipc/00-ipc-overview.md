# 00-ipc-overview: IPC server 总览

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 0 总览（导航）
> **源码**: `minix3/minix/servers/ipc/`（4 个 .c，1690 行）
> **Rust 模块**: `os/servers/ipc-server/`（全部，当前空壳 stub）
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- IPC server 是什么：SysV 信号量（semget/semctl/semop）与共享内存（shmget/shmat/shmdt/shmctl）的**用户态对象管理服务**，通过 kernel IPC 收发消息
- **与 kernel IPC 机制的边界**：send/receive/notify 原语 = `../01-stage-kernel/12-ipc-core.md` + `notes/rewrite/ipc-sendrec.md`（本 stage 不覆盖）；crate 名 `ipc-server` 避免混淆
- boot 链位置：**不在 boot_image**（`kernel/table.c` 无 ipc 条目），由 RS 运行时加载（ipc.conf 定义特权面），与 IS/devman/input 同属 RS 加载组
- **启动主线图**：RS 加载 → `main`/`sef_local_startup` → `sef_cb_init_fresh`（rmib_register kern.ipc 子树）→ 主循环（notify 忽略 / PM PROC_EVENT / MIB / call_vec 分发 / 回复 / update_refcount_and_destroy）→ SIGTERM 收口（plan §1.2）
- **SysV 调用旅程次主线**：libc → IPC server dispatch → sem（05/06）/ shm（07/08）→ 04 权限 → VM（映射/引用计数）+ PM（进程事件）（plan §1.3）
- 文档导航：12 篇（00 + 01~10 + 99），按 6 阶段组织；每篇回答"它在 sef_cb_init_fresh / 主循环 dispatch 的哪个位置"

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（01~10、99）
