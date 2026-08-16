# 01-vfs-init-main: 启动入口与初始化骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 — 启动入口与进程模型（锚点文档）
> **源码**: `main.c:54-68,374-499,501-527`（main/sef_*/VFS_PM_INIT 握手/do_init_root）
> **Rust 模块**: `os/servers/vfs/src/main.rs`、`os/servers/vfs/src/main_loop.rs:run/init_fresh`
> **draft 素材**: `draft/10-main-loop.md` 启动部分（素材）

## 核心点

- 启动链：`main()` → `sef_local_startup()` → `sef_cb_init_fresh()`，与 kernel `06-proc-init-boot-proc` 衔接
- VFS_PM_INIT 握手：`sef_receive(PM)` 循环填 fproc 槽 → `ipc_send(PM, OK)` 同步（main.c:416-436）
- init_* 调用点时序：worker_init/init_dmap/init_smap/map_service/init_vnodes/init_vmnts/init_select/init_filps（main.c:445-489）
- 根挂载：`worker_start(do_init_root)` → `worker_allow(FALSE)` → `mount_pfs()` + `mount_fs(MFS, "/")`
- SEF 三回调：`sef_cb_init_fresh(393)/sef_cb_init_lu(352)/sef_cb_lu_prepare(303)`
- `system_hz`/`ds_subscribe`（驱动事件订阅，A-14）

## 边界

- 主循环 5 路分发细节不覆盖（09）
- service_pm 各请求内容不覆盖（10）
- 各表结构体字段不覆盖（02~07）
- mount_fs 内部机制不覆盖（18）
