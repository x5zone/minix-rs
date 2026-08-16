# 05-ds-identity-auth: 进程身份与权限检查

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 身份/权限
> **源码**: `minix3/minix/servers/ds/store.c:110-156`
> **Rust 模块**: `identity.rs`、`auth.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `ds_getprocname`：endpoint→名字（DS 自身 → "ds"；label 反查；NULL）
- `ds_getprocep`：名字→endpoint（label 正查，未找到 panic 路径）
- `check_auth`：权限位**未置 → 放行**（DSF_PRIV_* 是选择性保护，非默认保护）；置位 → owner 名相等
- 身份与数据的 owner 绑定语义

## 边界

- **前置依赖**: 03
- **不覆盖（移交）**: boot 映射的 owner="rs" 来源（06）
