# 04-ds-slot-management: 槽位分配与查找原语

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 存储原语
> **源码**: `minix3/minix/servers/ds/store.c:11-107`
> **Rust 模块**: `store.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- `alloc_data_slot`/`alloc_sub_slot`：首空闲线性扫描，满 → NULL
- `free_sub_slot`：`regfree` + `memset` + flags 清零
- `lookup_entry`（key+type 双条件）、`lookup_label_entry`（LABEL + num）、`lookup_sub`（owner）
- A-4：静态数组 → Rust 类型化表 + newtype 索引（线性扫描语义保留）

## 边界

- **前置依赖**: 03
- **不覆盖（移交）**: 身份映射（05）、handler 调用面（07~11）
