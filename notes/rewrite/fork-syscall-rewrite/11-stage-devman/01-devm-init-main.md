# 01-devm-init-main: 启动入口与主循环锚点

> **状态**: pending（最小骨架，待改写）
> **定位**: main → `run_vtreefs`（阶段 1 启动入口）
> **源码**: `minix3/minix/servers/devman/main.c`（93 行，全部）+ `lib/libvtreefs/vtreefs.c:sef_local_startup`
> **Rust 模块**: `main.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `main`（main.c:70-91）：三个 hooks 注册（init_hook/read_hook/message_hook）、root_stat（S_IFDIR|0444, uid/gid 0, NO_DEV）、`run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)` 调用点
- `init_hook`（main.c:36-44）：first 守卫 → `devman_init_devices`（04 锚点）
- `read_hook`（main.c:60-68）：inode 分发到 `read_fn`
- `message_hook`（main.c:46-58）：DEVMAN_* 消息分发面（fall-through 见 05/A-3）
- SEF 生命周期：`sef_local_startup`（init fresh/restart）→ `fsdriver_task` 主循环（02）
- mount 触发 init_hook：VFS mount → `fs_mount` → `init_hook`

## 边界

- **前置依赖**: 00 + kernel 文档（RS 加载组）
- **不覆盖（移交）**: VTreeFS 内部机制（02）、消息字段/分发细节（05）、各 handler 实现（07~09）
