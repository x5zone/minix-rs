# 11-ds-getsysinfo: 系统信息查询（do_getsysinfo）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 数据面 handler
> **源码**: `minix3/minix/servers/ds/store.c:653-678`、`servers/is/dmp_ds.c`、`libsys/getsysinfo.c:22`
> **Rust 模块**: `getsysinfo.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `do_getsysinfo`：`SI_DATA_STORE` → size 精确匹配（EINVAL）→ `sys_datacopy(SELF→caller)`
- 布局 ABI 消费者：IS `dmp_ds.c` 按 `store.h` 布局直接解释 ds_store
- A-10：`#[repr(C)]` 兼容 vs ARCH 偏离 + IS 同步改造（两方案三处一致标注）
- 客户端入口：`libsys/getsysinfo.c:22`（DS_PROC_NR → DS_GETSYSINFO）

## 边界

- **前置依赖**: 02/03 + IS `dmp_ds.c`
- **不覆盖（移交）**: 存储内部语义（03~05）
