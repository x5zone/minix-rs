# 07-devm-add-device: 设备添加

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → DEVMAN_ADD_DEV → do_add_device（阶段 4 生命周期起点）
> **源码**: `minix3/minix/servers/devman/device.c:223-277,314-418`
> **Rust 模块**: `add_device.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `do_add_device`（device.c:223-277）：grant 拷贝 → 找 parent（ENODEV）→ `devman_dev_add_child` → state=UNBOUND/owner=ep → ADD 事件 → reply dev_id
- `devman_dev_add_child`（:345-397）：dev_id 分配、add_inode 目录、entries 循环 `devman_dev_add_info`、devman_id 静态文件、parent refcount get
- `devman_dev_add_info`（:404-418）：STATIC → add_static_info；DYNAMIC → A-6 defer（fall-through -1）
- `devman_dev_add_static_info`（:314-339）：DEVMAN_STRING_LEN 截断语义、default_file_stat、dev->infos 链表
- wire 解码：`devman_device_info`/`entry` 的 offset 布局（A-4）

## 边界

- **前置依赖**: 04 + 05 + 06
- **不覆盖（移交）**: 引用计数删除面（08）、bind 状态（09）
