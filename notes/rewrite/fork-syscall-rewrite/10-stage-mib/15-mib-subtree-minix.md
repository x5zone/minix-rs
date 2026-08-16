# 15-mib-subtree-minix: CTL_MINIX 子树

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 子系统子树（init 顺序第四）
> **源码**: `minix.c` 全部 + `minix/sysctl.h`
> **Rust 模块**: `subtree/minix.rs`
> **draft 素材**: 无（新建）

## 核心点

- `CTL_MINIX=32` 顶层子树（minix/sysctl.h ABI：MINIX_TEST=0/MIB=1/PROC=2/LWIP=3）
- test 子树（`MINIX_TEST_SUBTREE` 门控，开发用）：int/bool/quad/string/struct/private/anywrite/dynamic/secret/perm/destroy1/destroy2 → **test87 行为契约**（含描述对齐测试）
- mib 统计子树：nodes/objects/remotes（`MIB_INTPTR` 指向全局计数）
- proc 子树：list/data 表定义（handler 在 20 实现）
- MINIX_LWIP 不在本表：LWIP 通过 RMIB 挂载（`mibtree.c:42-66`）
- A-9：test 子树在生产环境可关闭（MINIX_TEST_SUBTREE=0）

## 边界

- **前置依赖**: 03/06 + test87
- **不覆盖（移交）**: 进程信息实现（16~20）
