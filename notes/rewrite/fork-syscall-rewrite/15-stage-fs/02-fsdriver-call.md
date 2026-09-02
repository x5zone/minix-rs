# 02-fsdriver-call: fsdriver 请求适配器（call.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 框架层：REQ_* → fdr_* 回调的协议适配
> **源码**: `minix3/minix/lib/libfsdriver/call.c`
> **Rust 模块**: `minix-fs`（请求适配）
> **draft 素材**: 无（新建）

## 核心点

- 31 个适配器按 6 组：挂载组（readsuper/unmount/mountpoint）、节点组（putnode/newnode）、数据组（read/write/peek/getdents/trunc/inhibread）、命名空间组（create/mkdir/mknod/link/unlink/rmdir/rename/slink/rdlink）、元数据组（stat/chown/chmod/utime/statvfs）、块组（bread/bwrite/bpeek/bflush/newdriver/flush/sync）
- 统一模式：参数提取（m_vfs_fs_*）→ 校验 → `fdr_*` 回调 → 回复构造（m_fs_vfs_*）
- READSUPER 特殊路径：`fsdriver_mounted` EBUSY 检查、`fsdriver_getname` 取 label、`fdr_driver` 绑定、RES_* 能力位协商（HASPEEK 推断）
- 消息布局全集：`minix3/minix/include/minix/ipc.h` 的 m_vfs_fs_*/m_fs_vfs_* 结构
- 错误传播：回调返回值 → m_type 错误码；特殊错误码 EENTERMOUNT/ELEAVEMOUNT/ESYMLINK（-301/-302/-303）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: fdr_* 服务端实现（06~24）、copy/dentry 辅助（03）
