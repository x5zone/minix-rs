# 01-vm-init-main: 启动入口与初始化骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: `main.c:93` `main()` → `init_vm()`，阶段 1~5 全部文档的锚点
> **源码**: `minix3/minix/servers/vm/main.c`（`get_mem_chunks` 定义于 utility.c，语义归 05）
> **Rust 模块**: `main.rs`、`global.rs`
> **draft 素材**: `draft/26-vm-init-main.md`（素材）
> **变更**: 从 draft/26 拆分——骨架提前，主循环细节并入 15

## 核心点

- `main`/`init_vm` 启动链（`is_first_time`）、`mem_add_total_pages`（调用点）
- SEF 生命周期：`sef_local_startup`、`sef_cb_init_fresh`、`sef_cb_init_lu_restart`（同时注册 LU 与 restart）、`sef_cb_lu_state_changed`、`sef_cb_init_vm_multi_lu`、`sef_cb_signal_handler`、`do_sef_init_request`
- `exec_bootproc`、`libexec_*`（boot 进程地址空间）、`map_service`
- VM 自身 libc 接口边界声明（`utility.c` 的 `mmap`/`munmap`/`_brk`，无内部调用者，仅声明）

## 边界

- **前置依赖**: 00 + kernel `../01-stage-kernel/09-vm-boot-protocol.md`
- **不覆盖（移交）**: 组件细节（02~15）、主循环 dispatch 细节（15）
