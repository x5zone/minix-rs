# 02-ds-message-contract: DS IPC 协议面

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（所有 handler 篇的前置）
> **源码**: `com.h:65,93,498-507`、`ipc.h:94-115`、`ds.h` 全部、`sysinfo.h:13`
> **Rust 模块**: `minix-types`（**缺 DsReq/DsReply 类型，A-1**）
> **draft 素材**: 无（新建）

## 核心点

- call numbers：`DS_PUBLISH`~`DS_GETSYSINFO`（`DS_RQ_BASE 0x800`，含 A-7 排除 `DS_SNAPSHOT`）
- `mess_ds_req` 字段级语义：`key_grant`/`key_len`/`flags`/`val_in`/`val_len`/`owner`
- `mess_ds_reply`：`val_out`/`val_len`；回复复用请求字段（`do_check` 写回 `flags`/`owner`）
- `union ds_val`（grant/u32/ep）三态载荷
- DSF 标志全集 + 掩码（`DSF_MASK_TYPE 0xFF0`/`DSF_MASK_INTERNAL 0xFFF`、`0x80` 空位）
- `DS_MAX_KEYLEN 80`、key grant 读写约定（CHECK/RETRIEVE_LABEL 为写 grant）
- A-1：minix-types 消息类型缺口（字段全 32 位，64 位布局兼容）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 服务器存储语义（03~05）、handler 流程（07~11）
