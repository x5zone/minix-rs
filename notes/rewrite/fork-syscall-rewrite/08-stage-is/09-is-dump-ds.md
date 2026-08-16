# 09-is-dump-ds: DS 数据域转储（dmp_ds.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域
> **源码**: `minix3/minix/servers/is/dmp_ds.c`（52 行）
> **Rust 模块**: `dump_ds.rs`
> **draft 素材**: `draft/tmp_dmp_ds.c.md`（逐行素材）

## 核心点

- `data_store_dmp`（9）：`getsysinfo(DS_PROC_NR, SI_DATA_STORE)` → data_store 表格式化
- data_store 布局 ABI（A-4）：**DS `07-stage-ds` A-10 消费者契约**（`struct data_store`，NR_DS_KEYS，`../07-stage-ds/03-ds-data-structures.md`）
- DSF_IN_USE 过滤 + DSF_MASK_TYPE 四类型输出（U32/STR/MEM/LABEL）
- 静态 prev_i 环形翻页游标（跳过未用槽位，到尾回绕）+ 22 行分页（A-5）

## 边界

- **前置依赖**: 04 + DS 布局（`07-stage-ds`）
- **不覆盖（移交）**: DS 服务器内部语义（`07-stage-ds`）
