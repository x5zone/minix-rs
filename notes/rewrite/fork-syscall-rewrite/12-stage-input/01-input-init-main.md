# 01-input-init-main: 启动入口与初始化序列

> **状态**: pending（最小骨架，待改写）
> **定位**: main → `input_startup` → `input_init` → `chardriver_task`（阶段 1 启动入口）
> **源码**: `minix3/minix/servers/input/input.c:646-704`（input_init/input_startup/main）+ `input.h` 全部
> **Rust 模块**: `main.rs`、`init.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `main`（input.c:696-704）：`input_startup()` + `chardriver_task(&input_tab)` 调用点（702）
- `input_startup`（:685-694）：`sef_setcb_init_fresh(input_init)` + `sef_startup()`（SEF 生命周期）
- `input_tab`（:31-42）：chardriver 回调注册表（open/close/read/ioctl/cancel/select/other）
- `input_init`（:646-683）：初始化序列——`devs[10]` 清零（input_revmap 回填 minor，锚 03）、`ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)`（锚 11）、`chardriver_announce()`（锚 02）、TTY_INPUT_UP → TTY（`ipc_send`，锚 13）
- SEF init fresh 回调契约（restart 语义：`chardriver_announce` 清 open_devs + CLEAR_IPC_REFS）

## 边界

- **前置依赖**: 00 + kernel 文档（RS 加载组）
- **不覆盖（移交）**: chardriver 内部机制（02）、设备结构细节（03）、各 handler 实现（06~10）、驱动生命周期（11）
