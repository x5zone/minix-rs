# 11-mib-query-describe: 节点枚举与描述

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 元标识符（QUERY/DESCRIBE）
> **源码**: `tree.c:90-241,918-1096`
> **Rust 模块**: `query.rs`、`describe.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_query`（CTL_QUERY）：版本校验（SYSCTL_VERSION + 节点/根版本匹配）、静态（按 id 升序）+ 动态（链表序）枚举、sysctlnode 数组输出
- `mib_copyout_node`：内部标志剥离（PARENT/VERIFY/REMOTE）、PRIVATE 可见性、立即值内嵌、节点类型特殊规则（远程 csize/clen、PARENT 子节点信息、函数标记 `SYSCTL_NODE_FN` 防 trace 下探）
- `mib_describe`（CTL_DESCRIBE）：批量（同 query 遍历）或单节点（sysctlnode 定位）；设置描述路径（auth + 无已有描述 + 非 PERMANENT + 版本匹配 + copyin_str + strdup + OWNDESC）
- `mib_copyout_desc`：sysctldesc 布局（descr_num/ver/len/str）、私有节点过滤、`roundup2` 对齐
- A-4：sysctlnode/sysctldesc 交换格式布局 ABI（sysctlgetmibinfo 消费者）

## 边界

- **前置依赖**: 03/06/07
- **不覆盖（移交）**: 分发控制流（10）
