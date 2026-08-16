# 03-devm-structs: 核心数据结构与 wire 格式

> **状态**: pending（最小骨架，待改写）
> **定位**: 设备/事件/inode 结构 + 序列化格式（阶段 2 核心数据结构）
> **源码**: `minix3/minix/servers/devman/devman.h`（108 行）+ `devinfo.h`（35 行）+ `minix/include/minix/devman.h` wire 结构
> **Rust 模块**: `structs.rs`、`wire.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `devman_device` 全字段：dev_id/name/ref_count/major/state/owner/inode/parent/info/siblings/children/infos
- `devman_inode`/`devman_event`/`devman_event_inode`/`devman_static_info_inode`
- 状态机：DEVMAN_DEVICE_UNBOUND=0 / BOUND=1 / ZOMBIE=2
- 常量：BUF_SIZE 4097、DEVMAN_STRING_LEN 128、ADD_STRING/REMOVE_STRING
- wire 格式：`devman_device_info`/`devman_device_info_entry`（offset 布局，A-4）；`subsystem_offset` server 不读（标注）
- ARCH A-2（TAILQ/malloc → Rust 类型）、A-5（dev_id/refcount → Newtype/u32）

## 边界

- **前置依赖**: 02（inode 挂接）
- **不覆盖（移交）**: 树操作（04）、消息面（05）、状态转换触发点（07~09）
