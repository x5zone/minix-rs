# 09-ds-delete: 数据删除（do_delete）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 数据面 handler
> **源码**: `minix3/minix/servers/ds/store.c:583-651`
> **Rust 模块**: `delete.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `do_delete` 全流程：身份（EPERM）→ `get_key_name` → lookup（ESRCH）→ owner 检查（EPERM）→ 类型分支
- LABEL 级联：先清订阅（owner==label 的 `free_sub_slot`），再清数据项（逐个 `update_subscribers(dsp,0)` + 清位）
- STR/MEM：`free(data)`；`update_subscribers(dsp,0)` + notify；flags 清零
- 测试契约：`tests/ds/dstest.c`（delete → 再检索 ESRCH）

## 边界

- **前置依赖**: 02/03/04/05/10
- **不覆盖（移交）**: 订阅机制内部（10）
