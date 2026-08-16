# 12-mib-remote-subtrees: 远程子树注册与转发

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 8 远程子树（跨服务协议）
> **源码**: `remote.c` 全部 + `tree.c:1538-1842`
> **Rust 模块**: `remote.rs`、`tree/mount.rs`
> **draft 素材**: 无（新建）

## 核心点

- `endpts[]` 表（`MIB_ENDPTS=1<<5=32`）：endpt/label/nodes 链表；`mib_remote_init` 复位
- 注册（MIB_REGISTER 单向，SENDREC→ENOSYS）：label 校验（`ds_retrieve_label_name`，A-12）→ 同 label 新 endpoint 触发 `mib_down`（旧挂载点清理）→ 空闲 eid → rid 冲突拒绝 → `mib_mount` → 链表挂接
- `mib_mount` 挂载策略：miblen≥2（禁顶层挂载）、flags 白名单、csize/clen ≤ 4096；路径走查（无元 id、非 REMOTE/PRIVATE 的 PARENT 节点）；挂载点=已有节点（flags 精确匹配 + 无动态子节点 + upgrade）或临时创建（`mib_remote_info` 取 name/desc + scan + 单块分配）
- 卸载：`mib_unmount`（PARENT→恢复 csize/clen；临时→mib_remove）+ `mib_deregister`/MIB_DEREGISTER
- `mib_remote_call`：COMMON_MIB_CALL relay（name/oldp/newp 三 grant + user_endpt + flags + root_ver/tree_ver）；IPC 失败 → `mib_down` + ERESTART；服务返回 ERESTART → `mib_do_deregister`
- 死亡检测局限：无主动订阅（TODO 注释）；未来异步化方向（req_id 已预留）
- **远程子树次主线路径图**（plan §1.3）

## 边界

- **前置依赖**: 02/03/06 + DS `08`
- **不覆盖（移交）**: 客户端 rmib（22）、分发续走调用点（10）
