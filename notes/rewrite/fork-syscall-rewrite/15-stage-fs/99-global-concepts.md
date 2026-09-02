# 99-global-concepts: FS 全局概念与常量

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段 99）
> **源码**: `minix3/minix/include/minix/vfsif.h`、`fsdriver.h`、`libminixfs.h`、`sffs.h`、`vtreefs.h`、`minix3/minix/include/minix/com.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: 无（新建）

## 核心点

- FS_BASE/REQ 常量表：REQ_GETNODE(1)~REQ_BPEEK(33)、NREQS=34、IS_FS_RQ
- TRNS 编码：TRNS_GET_ID/TRNS_ADD_ID/TRNS_DEL_ID
- 服务号常量：MFS/PFS/PROC/PTY 等 endpoint（com.h）
- 协议结构：fsdriver_node/data/dentry、vfs_ucred_t、m_vfs_fs_*/m_fs_vfs_* 消息布局
- errno 映射表：含 EENTERMOUNT(-301)/ELEAVEMOUNT(-302)/ESYMLINK(-303) 协议错误码
- 能力位：RES_THREADED/RES_HASPEEK/RES_64BIT；挂载标志 REQ_RDONLY/REQ_ISROOT

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 常量如何被使用（各文档）
