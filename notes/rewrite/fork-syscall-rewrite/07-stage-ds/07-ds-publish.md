# 07-ds-publish: 数据发布（do_publish）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 数据面 handler
> **源码**: `minix3/minix/servers/ds/store.c:287-381`
> **Rust 模块**: `publish.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `do_publish` 全流程：身份 → LABEL 仅 RS → `get_key_name` → `lookup_entry`/`lookup_label_entry` → alloc/overwrite/EEXIST → 4 类型写入 → 属性设置 → `update_subscribers(dsp,1)`
- 类型分支：U32/LABEL 直接写值；STR/MEM malloc/realloc（A-3）+ `sys_safecopyfrom` + STR NUL 收尾
- 通知接线：写入成功后向匹配订阅者置位 + notify（10）
- 测试契约：`tests/ds/dstest.c`（EEXIST / OVERWRITE / STR / MEM / LABEL EPERM）

## 边界

- **前置依赖**: 02/03/04/05/10
- **不覆盖（移交）**: 检索（08）、删除（09）、订阅机制内部（10）
