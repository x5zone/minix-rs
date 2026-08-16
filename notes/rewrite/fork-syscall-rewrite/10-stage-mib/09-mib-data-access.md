# 09-mib-data-access: 数据节点读写

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 6 数据访问（常规叶节点）
> **源码**: `tree.c:1098-1330`
> **Rust 模块**: `data/readwrite.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_getptr`：立即值（bool/int/quad）vs 指针数据（string/struct）；IMMEDIATE 的 STRING/STRUCT 返回 NULL
- `mib_read`：STRING 用 `strlen+1`、其他用 node_size；部分拷贝
- `mib_write`：先拷入临时缓冲（原子性，防半程失败破坏节点值）→ verify 回调 → 类型校验（非字符串精确长度 / 字符串 ≤ 缓冲）→ bool 消毒 → 存回
- 临时缓冲：`newlen+1 ≤ SCRATCH_SIZE` 用 scratch；否则仅特权用户 malloc（未授权 EPERM），分配失败 EINVAL
- `mib_readwrite`：读 + 写组合，返回旧长度（供函数 handler 复用）
- A-11：分配失败不返回 ENOMEM 的全局约定

## 边界

- **前置依赖**: 03/06/07
- **不覆盖（移交）**: 分发（10）、函数节点 handler（13~20）
