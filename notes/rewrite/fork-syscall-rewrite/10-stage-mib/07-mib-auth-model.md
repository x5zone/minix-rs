# 07-mib-auth-model: 权限模型（mib_authed）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 权限原语
> **源码**: `main.c:259-275` + 各权限检查点（tree.c）
> **Rust 模块**: `auth.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_authed`：`getnuid(call_endpt)==SUPER_USER` 向 PM 查询，每 call 一次缓存（`MIB_FLAG_AUTH`/`MIB_FLAG_NOAUTH`）
- `CTLFLAG_PRIVATE`：节点私有，未授权 EPERM（节点结构/描述均对未授权隐藏）
- 写权限：`CTLFLAG_READWRITE` 必须；`CTLFLAG_ANYWRITE` 允许非特权写；未授权写 EPERM
- `CTLFLAG_PERMANENT`：节点不可销毁/不可设描述
- 特权操作集合：create / destroy / 描述设置 / 大数据写（>scratch）
- 与 DS `check_auth` 对比：MIB 是超级用户二元模型（无选择性保护位）

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 具体节点的写校验调用点（09/10）
