# 03-ds-data-structures: 存储与订阅数据结构

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 数据模型
> **源码**: `minix3/minix/servers/ds/store.h`
> **Rust 模块**: `store.rs`、`subscription.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `struct data_store`：`flags`/`key[80]`/`owner[80]`/`union u{u32, mem{data,length,reallen}}`
- `struct subscription`：`flags`/`owner`/`regex`/`old_subs` 位图
- `ds_store[NR_DS_KEYS]`、`ds_subs[NR_DS_SUBS]`（`NR_DS_KEYS=2×NR_SYS_PROCS=128`、`NR_DS_SUBS=4×NR_SYS_PROCS=256`）
- flags 位语义（IN_USE/PRIV_*/TYPE_*）
- SI_DATA_STORE 布局 ABI（A-10：x86-64 `sizeof(struct data_store)` 决定 getsysinfo 原始拷贝契约）

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 槽位分配/查找原语（04）、身份/权限（05）
