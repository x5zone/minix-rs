# 05-mib-tree-lookup: 节点查找（mib_find）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 树查找原语
> **源码**: `minix3/minix/servers/mib/tree.c:34-88`
> **Rust 模块**: `tree/lookup.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_find(parent, id)`：id<0 返回 NULL；静态数组 O(1)（`IS_STATIC_ID` + `node_flags≠0`）；动态链表 O(n)（按 id 升序，遇 >id 提前终止）
- `prevpp` 输出：动态节点删除所需的指针-指针（供 mib_remove 使用）
- 静态/动态同 id 语义：静态优先；静态槽位 flags==0 视为未占用
- 调用方：dispatch（10）、describe/destroy（08/11）、mount 路径走查（12）

## 边界

- **前置依赖**: 03
- **不覆盖（移交）**: 名字解析主循环（10）、动态插入（08）
