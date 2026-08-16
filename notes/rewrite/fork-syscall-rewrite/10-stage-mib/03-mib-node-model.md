# 03-mib-node-model: 树节点模型

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 树模型（所有树操作的前置）
> **源码**: `minix3/minix/servers/mib/mib.h:110-280`
> **Rust 模块**: `tree/node.rs`、`tree/flag.rs`
> **draft 素材**: 无（新建）

## 核心点

- `struct mib_node` 全字段：flags/size/ver/parent/三态 union（child/remote/立即值）/name/desc
- 4 类节点矩阵（PARENT×REMOTE）：真实子树 / 函数驱动 / 临时挂载点 / 覆盖挂载点
- `struct mib_dynode`：按 id 有序链表 + 内嵌 name（+data/desc）
- 标志系统：CTLTYPE 类型位（低 4 位）+ CTLFLAG 标志位 + SYSCTL_VERS 高位；内部标志复用 NetBSD 位（ROOT/ALIAS/MMAP）不暴露用户态
- 计数：`mib_nodes`/`mib_objects`/`mib_remotes`；scratch 缓冲（`SCRATCH_SIZE = max(PAGE_SIZE, sizeof(sysctldesc)+MAXDESCLEN=1024)`）
- 位域拆分：`MIB_EID_BITS=5`（32 个远程服务）/`MIB_RC_BITS=12`（4096 子节点）
- A-2：Rust enum 节点建模（`NodeKind::{Static, Data, Func, Remote}`）

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 查找（05）、静态树定义（04）、远程字段用途（12）
