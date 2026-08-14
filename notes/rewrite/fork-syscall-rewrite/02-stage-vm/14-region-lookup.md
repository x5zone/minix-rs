# 14-region-lookup: 区域查找（AVL → BTreeMap）

> **状态**: pending（最小骨架，待改写）
> **定位**: 地址空间数据结构（region 的索引面）
> **源码**: `minix3/minix/servers/vm/regionavl.c`、`cavl_*.h`、`unavl.h`
> **Rust 模块**: `region/region_map.rs`
> **draft 素材**: `draft/13-region-avl.md`（素材）
> **变更**: 改名（原 region-avl，语义 = 查找）

## 核心点

- `region_find_slot*` 查找面
- ARCH A-4：Walt Karas AVL（cavl 宏模板）→ `BTreeMap<VirBytes, VirRegion>`，O(log n) 语义等价

## 边界

- **前置依赖**: 13
- **不覆盖（移交）**: 区域生命周期（13）
