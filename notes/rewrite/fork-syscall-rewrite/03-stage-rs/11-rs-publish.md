# 11-rs-publish: 服务发布与撤销

> **分类**: 阶段 4 — 服务创建与配置（从槽位到运行进程的第五步：发布）
> **源码**: `minix3/minix/servers/rs/manager.c`（`publish_service`—787、`unpublish_service`—864）、`minix3/minix/lib/libsys/mapdriver.c`（`mapdriver`）、`minix3/minix/include/minix/ipc.h:1473`（`mess_lsys_vfs_mapdriver`）、`minix3/minix/include/minix/com.h:858-859`（`DEVMAN_BIND`/`DEVMAN_UNBIND`）、`minix3/minix/include/minix/rs.h:154-161`（`rs_pci`）、`minix3/minix/include/minix/const.h:132`（`NO_DEV`）
> **Rust 模块**: `os/servers/rs/src/publish.rs`（`should_map_driver`/`should_set_pci_acl`/`should_bind_devman`/`unpublish_result`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md`（创建完成）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`dev_nr`/`devman_id` 字段语义）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/09-rs-exec.md`（`setuid(0)` hack 的第一次出现）
> **说明**: 创建（10）之后，服务有了进程但**别人还找不到它**。发布 = 把服务写进三个"目录"：DS 的 label→endpoint 映射（全局查找）、VFS 的驱动表（`mapdriver`）、devman 的设备绑定；PCI 服务还有第四个（PCI ACL）。撤销是发布的逆操作，但语义上更宽松（best-effort）。本文档建模四个"是否发布"判定谓词与撤销的错误聚合——全部纯函数。

---

## 1. 概念：让服务可以被找到

### 1.0 章节引言

`create_service`（10）结束时，服务的 priv/sched/exec/VM 全部就位，但它仍然是一个**孤儿进程**：DS 查不到它的 label，VFS 不知道它是驱动，devman 不知道它管哪个设备。`publish_service`（manager.c:787-858）把这些"外部注册"一次做完；`unpublish_service`（manager.c:864-921）在终止/更新时反向撤销。本文档回答的问题是：**发布要注册哪些外部目录、每个注册的触发条件是什么、撤销失败如何处理**。

> **本章不讲什么**（机制一律移交）:
> - DS/VFS/devman/PCI 的消息布局（`19-rs-external-interfaces.md`）——本文档只固化触发谓词与调用顺序
> - `kill_service`（发布失败时的统一退出路径）的机制（`15-rs-terminate-restart.md`）——本文档只陈述"发布失败 → `kill_service(rp, errstr, r)`"这一外部行为
> - `detach_service` 的降权重发布（`15-rs-terminate-restart.md`）——那是 15 的清理分支，复用 `ds_publish_label` 但语义不同
> - `setuid(0)` hack 的完整解释——第一次出现在 10（create_service 内），本文档只说明它在 mapdriver 前**再次**出现（manager.c:818）的原因

### 1.1 发布 = 注册进四个外部目录（WHAT）

```
publish_service（manager.c:787-858）
  ├─ 1. DS：ds_publish_label(label, endpoint, DSF_OVERWRITE)（800）── 全局 label 注册
  ├─ 2. VFS：dev_nr>0 || nr_domain>0 → setuid(0) + mapdriver(...)（806-824）── 驱动表
  ├─ 3. PCI：rsp_nr_device || rsp_nr_class → pci_set_acl(&pci_acl)（828-835）── 设备访问控制（USE_PCI）
  └─ 4. devman：devman_id != 0 → DEVMAN_BIND（840-853）── 设备绑定
  → OK（858）
```

四个注册**互相独立**：DS 注册无条件，其余三个按槽位字段触发。任一步失败 → `kill_service(rp, errstr, r)`（服务直接被判死，15 处理善后）。

### 1.2 为什么"发布"独立于"创建"（WHY）

创建（10）把进程变成"内核认识的系统服务"；发布把进程变成"系统里可被发现、可被路由的服务"。两者分开有两个理由：

1. **DS 是单点事实源**：`ds_publish_label` 让任何进程能按 label 查到 endpoint（`ds_retrieve_label_endpt`）。如果创建时立即发布，`RS_UP` 调用者在 `do_up` 返回前就能查到服务——但服务还没初始化完成（12 的 ready 握手）。发布与运行（12）之间隔着初始化，所以发布是独立步骤。
2. **驱动时序**：`mapdriver` 依赖 VFS 已收到 PM 的 fork 通知（见 `setuid(0)` hack，manager.c:806-818 注释）。这是"创建后、mapdriver 前"的外部时序约束，只能靠发布步骤内的 hack 保证。

### 1.3 撤销的宽松语义（WHAT）

`unpublish_service`（manager.c:864-921）是 best-effort：

- DS 删除失败 → 记结果（`shutting_down` 时忽略，manager.c:879-881）；
- PCI 删除失败 → 记结果（同上，manager.c:890-893）；
- devman 解绑失败 → **只打印，不改结果**（manager.c:897-914）；
- VFS/VM **不需要通知**——进程退出时自动清理（manager.c:884 注释）。

---

## 2. C 源码分析

### 2.1 DS label 注册（manager.c:800-803）

`ds_publish_label(rpub->label, rpub->endpoint, DSF_OVERWRITE)`——把 `label → endpoint` 写进 DS，`DSF_OVERWRITE` 允许覆盖已有同名条目（重启场景）。失败 → `kill_service(rp, "ds_publish_label call failed", r)`。这是发布中**唯一无条件**的步骤：每个服务都必须能被 label 查到。

### 2.2 mapdriver：驱动映射（manager.c:805-823）

触发条件（manager.c:806）：

```c
if (rpub->dev_nr > 0 || rpub->nr_domain > 0) {
```

`dev_nr` 是主设备号（`NO_DEV = 0`，const.h:132），`nr_domain` 是 socket 驱动域数——两者任一非零说明这是驱动。mapdriver 前**再次** `setuid(0)`（manager.c:818），注释与 10 的 create_service 完全一致（manager.c:807-817）：非阻塞 fork 让 VFS 可能还没收到 PM 的 fork 消息，`setuid(0)` 强制阻塞通信保证 mapdriver 时序。随后 `mapdriver(rpub->label, rpub->dev_nr, rpub->domain, rpub->nr_domain)`（manager.c:820-821）——libsys 包装，底层是发给 VFS 的 `mess_lsys_vfs_mapdriver`（ipc.h:1473）。失败 → `kill_service(rp, "couldn't map driver", r)`。

### 2.3 PCI ACL（manager.c:826-835，USE_PCI 条件编译）

触发条件（manager.c:828-829）：`rpub->pci_acl.rsp_nr_device || rpub->pci_acl.rsp_nr_class`。命中时把 `rs_pci` 结构（rs.h:154-161）填上 label/endpoint 后 `pci_set_acl(&pci_acl)`（manager.c:833）——告诉 PCI 驱动这个 endpoint 能访问哪些设备/类别。失败 → `kill_service`。

### 2.4 devman 绑定（manager.c:837-853）

触发条件（manager.c:840）：`rpub->devman_id != 0`。流程：

1. `ds_retrieve_label_endpt("devman", &ep)`（manager.c:841）——查 devman 自己的 endpoint；失败 → `kill_service(rp, "devman not running?", r)`；
2. 构造 `DEVMAN_BIND` 消息（`DEVMAN_BIND` 操作码 com.h:858；`DEVMAN_ENDPOINT`/`DEVMAN_DEVICE_ID` 字段 com.h:864-865）并 `ipc_sendrec(ep, &m)`（manager.c:846-849）；
3. `m.DEVMAN_RESULT != OK` → `kill_service`（manager.c:850-852）。

### 2.5 unpublish_service（manager.c:864-921）

撤销按发布的反向顺序，但错误处理是 best-effort：

- `ds_delete_label(rpub->label)`（manager.c:878）：失败且 `!shutting_down` → 打印 + `result = r`（manager.c:879-881）；
- VFS/VM 不通知（manager.c:884 注释："cleanup is done on exit automatically"）；
- `pci_del_acl(endpoint)`（manager.c:889，USE_PCI）：失败且 `!shutting_down` → 打印 + `result = r`（manager.c:890-893）；
- devman 解绑（manager.c:897-914）：`DEVMAN_UNBIND` 消息（com.h:859），任何失败**只打印**，不改 `result`；
- 返回 `result`（首个非 OK 错误或 OK）。

---

## 3. Rust 设计决策

### 3.1 纯谓词 + 结果聚合

发布/撤销的**动作**全是外部 IPC（DS/VFS/PCI/devman，19 接线），但"**是否**发布"与"**如何**聚合错误"是纯逻辑。`publish.rs` 拥有：

- `should_map_driver(&PublicSlot) -> bool` —— `dev_nr > 0 || nr_domain > 0`（manager.c:805）
- `should_set_pci_acl(&PublicSlot) -> bool` —— PCI 触发判定（见 3.3）
- `should_bind_devman(&PublicSlot) -> bool` —— `devman_id != 0`（manager.c:840, 897）
- `unpublish_result(ds_ok, pci_ok, devman_ok, shutting_down) -> i32` —— 撤销错误聚合（manager.c:878-914）

**发布编排本身 DEFERRED（19）**：`ds_publish_label`/`mapdriver`/`pci_set_acl`/devman 消息按 §2 顺序组装，任一步失败接 15 的 `kill_service`。

### 3.2 devman_id 的 Option 建模

C 的 `int devman_id`（0 = 未绑定）映射为 `Option<i32>`（02 已建模）：`None` 或 `Some(0)` 都表示未绑定，`Some(n>0)` 才触发绑定。`should_bind_devman` 用 `is_some_and(|id| id != 0)` 统一两种"未绑定"形态。

### 3.3 PCI 面 fail-closed（ARCH A-10）

plan §5.4 明确 `USE_PCI` 条件编译段 defer：minix-rs 没有 PCI 驱动面。`rs_pci` 结构（rs.h:154-161）未建模，`should_set_pci_acl` 恒返回 `false`——**fail-closed**：不发布 PCI ACL 比发布错误 ACL 安全。语义契约（"非零 device/class 计数触发 `pci_set_acl`"，manager.c:828-829）保留在 doc 中，19 接线时把谓词换成真实字段判定。

### 3.4 撤销错误聚合的忠实性

`unpublish_result` 的四个参数对应四个外部结果，`shutting_down` 注入（C 读全局 `shutting_down`，glo.h:46，由 13 的 `do_shutdown` 设置）。C 的 `result` 被每个失败块**覆盖**写入（最后失败优先，manager.c:881/892）；两个调用点（manager.c:1138、request.c:143）都**忽略返回值**，所以具体错误码不可观测。Rust 以 `EIO` 占位保留"任一失败 → 非 OK"的形状（`pci_del_ok` 失败同样覆盖写入，与 C 一致），devman 失败永远不影响结果（manager.c:897-914）。19 接线时把 `EIO` 替换为真实 `ds`/`pci` 错误码。

---

## 4. 实现详解

### 4.1 模块结构

`os/servers/rs/src/publish.rs`：

| 函数 | C 对应 | 说明 |
|------|--------|------|
| `should_map_driver(pub_)` | manager.c:806 | `dev_nr > 0 \|\| nr_domain > 0` |
| `should_set_pci_acl(pub_)` | manager.c:828-829 | 恒 `false`（A-10 fail-closed） |
| `should_bind_devman(pub_)` | manager.c:840, 897 | `devman_id` 非零 |
| `unpublish_result(ds, pci, devman, shutting_down)` | manager.c:878-914 | 失败覆盖写入（最后失败优先）；调用方忽略返回值；devman 只记日志 |

### 4.2 关键不变量

1. **DS 注册无条件**：`should_*` 谓词只覆盖 2~4 步；第 1 步（DS）在 19 编排中恒执行。
2. **驱动判定以 `NO_DEV = 0` 为准**：`dev_nr > 0` 即驱动（const.h:132，02 §2.4 已修正 NO_DEV 事实）。
3. **devman 失败不污染结果**：`unpublish_result` 的 `devman_ok` 参数被显式忽略（`let _ = devman_ok`），忠实 C 的"只打印"语义。
4. **撤销是幂等的 best-effort**：任一外部失败不阻止其余步骤继续执行（与发布相反——发布任何失败都 kill）。

---

## 5. 测试要点

`publish.rs` 内 5 项测试（`cargo test -p minix-rs --lib publish`）：

1. `should_map_driver`：`dev_nr > 0` 触发；`nr_domain > 0` 触发；两者皆 0（`NO_DEV`）不触发。
2. `should_bind_devman`：`None` 不触发；`Some(0)` 不触发（≡ C 的 0）；`Some(5)` 触发。
3. `unpublish_result`：DS 失败且非 shutting_down → `EIO`；PCI 失败且非 shutting_down → `EIO`；shutting_down 压制错误记录 → `OK`；devman 失败不影响结果 → `OK`。

测试总数声明：本文档范围为 **5 项**（`publish` 模块内）。全局 `cargo test -p minix-rs --lib` = 181 通过（随并行模块增长，以各 doc 范围为准）。

---

## 6. 过渡

发布完成（manager.c:858 `OK`）后，服务在系统中**可见**：DS 能按 label 查到它、VFS 知道它的驱动属性、devman 绑定完成。但 RS 自己的 `RS_INIT` 握手还没发生——服务还没被允许运行。下一步：

- **12-rs-init-run**：`run_service` 发 `RS_INIT` 并等待 ready 握手——"服务正式运行"；
- 运行后，07 的心跳监控、13 的 `RS_DOWN`（stop_service → 若退出 → 15 的 `unpublish_service` 消费点）、16 的更新（新实例发布后旧实例撤销）都围绕本文档的发布/撤销语义展开。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` —— `setuid(0)` hack 的第一次出现（manager.c:656）与第二次（manager.c:818）的同一外部时序约束
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` —— `dev_nr`/`nr_domain`/`devman_id` 字段与 `NO_DEV = 0` 事实
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` —— `kill_service`（发布失败）与 `detach_service`（降权重发布）的机制
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/16-rs-live-update.md` —— 新实例发布后旧实例撤销的顺序
- `minix3/minix/servers/rs/manager.c:787-921` —— ground truth
- `minix3/minix/include/minix/ipc.h:1473` —— `mess_lsys_vfs_mapdriver` 消息槽
- `minix3/minix/include/minix/com.h:858-859` —— `DEVMAN_BIND`/`DEVMAN_UNBIND`
