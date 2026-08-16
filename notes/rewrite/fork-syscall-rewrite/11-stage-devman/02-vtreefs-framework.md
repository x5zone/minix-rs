# 02-vtreefs-framework: VTreeFS 框架契约

> **状态**: pending（最小骨架，待改写）
> **定位**: `run_vtreefs` 主循环与 inode 机制（阶段 1 运行框架）
> **源码**: `minix3/minix/lib/libvtreefs/`（13 文件，1642 行，按 devman 使用面裁剪）+ `minix3/minix/include/minix/vtreefs.h`
> **Rust 模块**: 等价框架（A-1，决策见 plan §7.3）
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `run_vtreefs`/`fs_hooks`（init/cleanup/lookup/getdents/read/write/…/message hook 全表）
- inode 树模型：`add_inode`/`delete_inode`/`get_root_inode`/`get_inode_name`/`get_inode_cbdata`、`inode_stat`
- fsdriver 回调表（table.c：mount/unmount/lookup/putnode/read/write/getdents/…/other）
- mount/unmount（fs_mount 触发 init_hook）
- read 路径（fs_read → read_hook → cbdata 分发）
- indexed 槽（NO_INDEX / sdbm，devman 全用 NO_INDEX）
- ARCH A-1：os/ 无 VTreeFS 等价物 → 框架决策（内部最小实现 vs 公共 crate）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: devman 业务结构（03）、read_fn 具体实现（06）、VTreeFS 非 devman 使用面（procfs stage）
