# 04-device-tree: 设备树与查找

> **状态**: pending（最小骨架，待改写）
> **定位**: root_dev + devices/ 树（阶段 2 核心数据结构）
> **源码**: `minix3/minix/servers/devman/device.c:187-207,283-308,45-70` + 静态变量（:16-39）
> **Rust 模块**: `device_tree.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `devman_init_devices`（device.c:187-207）：root_dev（dev_id=0, major=-1）+ "devices" 目录 + "events" 文件
- `_find_dev`/`devman_find_device`（:283-308）：递归 DFS 查找
- `devman_generate_path`（:45-70）：递归路径生成 + ENOMEM 上限（A-10）
- `next_device_id` 分配（:16，A-5）、default_dir_stat/default_file_stat（:18-33，file size 0x1000）
- ARCH A-10：strcat 递归 → Rust 显式长度上限

## 边界

- **前置依赖**: 03
- **不覆盖（移交）**: 事件队列（06）、添加/删除流程（07/08）
