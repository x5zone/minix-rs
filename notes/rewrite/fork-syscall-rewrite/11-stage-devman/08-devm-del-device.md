# 08-devm-del-device: 设备删除与引用计数

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → DEVMAN_DEL_DEV → do_del_device（阶段 4 生命周期终点）
> **源码**: `minix3/minix/servers/devman/device.c:424-515`
> **Rust 模块**: `del_device.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `do_del_device`（device.c:424-455）：查找（ENODEV）→ REMOVE 事件 → BOUND→ZOMBIE → `devman_put_device` → reply
- `devman_get_device`/`devman_put_device`（:460-480）：refcount 语义（root_dev 除外），put 到 0 → `devman_del_device`
- `devman_del_device`（:485-515）：free infos（delete_inode + free）→ delete 设备 inode → 从 parent children 移除 → put parent → free info/dev
- 与 bind 的交互：ZOMBIE 设备不被 unbind 重置（09）
- ARCH A-5：refcount 类型化/`Rc` 语义

## 边界

- **前置依赖**: 04 + 05 + 06 + 07
- **不覆盖（移交）**: bind 状态机细节（09）、客户端 del 消息构造（10）
