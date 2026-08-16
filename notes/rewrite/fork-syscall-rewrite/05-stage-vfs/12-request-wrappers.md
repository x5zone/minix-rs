# 12-request-wrappers: req_* 请求包装与 REQ_* 协议面

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — FS 通信协议：协议面
> **源码**: `request.c` 全文件、`vfsif.h:41-73`（35 个 REQ_*）、`request.h`
> **Rust 模块**: （未实现）fs 客户端
> **draft 素材**: 无（新建）

## 核心点

- 全部 req_* 包装：req_lookup/req_create/req_readwrite/req_breadwrite/req_getdents/req_readsuper/req_newnode/req_mountpoint/req_putnode/req_unlink/req_rmdir/req_mkdir/req_mknod/req_link/req_rename/req_slink/req_rdlink/req_stat/req_chmod/req_chown/req_utime/req_statvfs/req_ftrunc/req_flush/req_inhibread/req_peek/req_bpeek 等
- 35 个 REQ_* 协议面与 FS_BASE 偏移
- node_details/lookup_res 响应结构
- req_getnode 死协议常量排除（REQ_GETNODE "Should be removed"）

## 边界

- 请求队列机制不覆盖（11）
- 底层 FS 服务端实现不覆盖（`minix3/minix/fs/`）
