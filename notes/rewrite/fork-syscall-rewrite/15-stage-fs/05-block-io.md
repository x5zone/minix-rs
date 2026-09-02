# 05-block-io: 块 I/O（libminixfs bio）

> **状态**: pending（最小骨架，待改写）
> **定位**: 磁盘 FS 与块设备的 I/O 边界
> **源码**: `minix3/minix/lib/libminixfs/bio.c`
> **Rust 模块**: `minix-fs`（bio）、`minix-bdev`
> **draft 素材**: 无（新建）

## 核心点

- `lmfs_driver`：dev+label → 块驱动端点绑定（RS 查询 + bdev 打开）
- `lmfs_bio`：bread/bwrite/bpeek 统一实现（dev/data/bytes/pos/call → bdev 请求），FSC_READ/FSC_WRITE/FSC_PEEK 区分
- `lmfs_bflush`：冲刷指定 dev 全部脏块
- `lmfs_get_partial_block`（inc.h）：子页块读取（isofs FIXME 场景）
- 与 `minix-bdev` 客户端接口的交互约定（打开/读/写/冲刷）
- NEW_DRIVER 后的重绑定路径（REQ_NEW_DRIVER → lmfs_driver）

## 边界

- **前置依赖**: 04
- **不覆盖（移交）**: 块设备驱动实现（16-stage-drivers）、缓存策略（04）
