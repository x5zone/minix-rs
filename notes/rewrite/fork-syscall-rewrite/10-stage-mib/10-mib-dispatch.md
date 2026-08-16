# 10-mib-dispatch: 分发与名字解析

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 分发（sysctl 次主线核心）
> **源码**: `tree.c:1332-1475`
> **Rust 模块**: `dispatch.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_dispatch` 解析循环：从 `mib_root` 出发逐层下降（消费 call_name）
- 元标识符（id<0）：QUERY/CREATE/DESTROY/DESCRIBE；CREATESYM/MMAP → EOPNOTSUPP；必须为最后分量否则 EINVAL
- 节点判定：is_leaf（非 NODE 类型）/has_func（叶：VERIFY 优先于 func；非叶：非 PARENT）；叶节点带长名 → ENOTDIR；非叶无函数 → 继续下降；名尽 → EISDIR
- 远程节点：`mib_remote_call` → ERESTART（服务死亡）→ 续走本地子树（仅 PARENT 挂载点）；临时挂载点不可续走 → ENOENT
- 写权限检查（叶/函数 + newp）：READWRITE + （ANYWRITE 或 auth）
- **sysctl 次主线路径图**（plan §1.3）：用户态 → 消息解码 → 解析 → 读写 → ENOMEM 语义

## 边界

- **前置依赖**: 03/05/07/09 + 12（远程续走）
- **不覆盖（移交）**: 枚举/描述实现（11）、远程协议（12）
