# 04-mib-static-tree-init: 静态树定义与初始化

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 静态树（boot 锚点）
> **源码**: `main.c:36-64,384-413`、`tree.c:1476-1536`、`mib.h` 宏
> **Rust 模块**: `tree/static_tree.rs`、`tree/init.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_table[]` 7 个顶层节点：kern/vm/net/hw/user/vendor/minix（前 6 个 `_P|_RO`，vendor `_RW`；net 等待远程注册、user 由 libc 处理、vendor 供第三方写）
- `mib_root`：根节点 `_RW`、`node_scptr=mib_table`、用户态无法直接访问
- `MIB_*` 静态初始化宏全集：`MIB_NODE`/`MIB_ENODE`/`MIB_BOOL`/`MIB_INT`/`MIB_QUAD`/`MIB_*PTR`/`MIB_STRING`/`MIB_STRUCT`/`MIB_FUNC`/`MIB_INTV`/`MIB_INIT_ENODE`
- `mib_init` 回调（SEF boot 锚点，A-7）：kern→vm→hw→minix 子树 init + `mib_tree_init` + `mib_remote_init`；fresh/restart 复用
- `mib_tree_init`：root ver=1、计数复位；`mib_tree_recurse`：csize=node_size、clen/nodes 计数、ver/parent 逐层传递

## 边界

- **前置依赖**: 03 + 01
- **不覆盖（移交）**: 子树内部节点表（13~15）、endpts 表（12）
