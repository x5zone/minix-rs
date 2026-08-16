# 08-ds-retrieve: 数据检索（do_retrieve / do_retrieve_label）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 数据面 handler
> **源码**: `minix3/minix/servers/ds/store.c:383-454`
> **Rust 模块**: `retrieve.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `do_retrieve`：`get_key_name` → lookup（ESRCH）→ `check_auth(DSF_PRIV_RETRIEVE)`（EPERM）→ 4 类型输出
- STR/MEM：`MIN(val_len, mem.length)` 截断拷贝 + `val_len` 写回
- `do_retrieve_label`：按 ep 反查 key（`sys_safecopyto`）
- 测试契约：`tests/ds/dstest.c`（u32/str/mem 检索、`get_len=8` 截断）

## 边界

- **前置依赖**: 02/03/05
- **不覆盖（移交）**: 发布（07）、订阅（10）
