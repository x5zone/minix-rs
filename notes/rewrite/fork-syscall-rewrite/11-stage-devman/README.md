# 11-stage-devman — DEVMAN 文档目录

> **状态**: 全部完成（plan.md 定稿 2026-08-16；15 篇正文 + Rust 实现 + full-review CONVERGED，2026-09-04）
> **主线**: DEVMAN server 启动顺序（main → run_vtreefs → mount 触发 init_hook → devman_init_devices → 主循环）；设备生命周期旅程（ADD → 事件 → devmand → RS bind）为次主线
> **Ground truth**: `minix3/minix/servers/devman/`（4 个 .c + 3 个 .h，共 1013 行）+ `minix3/minix/lib/libdevman/`（2 个 .c + 1 个 .h，共 603 行）+ `minix3/minix/lib/libvtreefs/`（10 个 .c + 4 个 .h，共 1642 行，使用面）

## 文档清单（15 篇，全 reviewed）

| 编号 | 文档 | 语义模块 | review 状态 |
|------|------|---------|------------|
| 00 | `00-devm-overview.md` | 总览：devman 是什么、启动主线图、设备生命周期次主线、导航 | CONVERGED |
| 01 | `01-devm-init-main.md` | main()/三个 hooks/run_vtreefs 调用点/SEF 生命周期/mount 触发 init_hook | CONVERGED |
| 02 | `02-vtreefs-framework.md` | VTreeFS 框架契约：inode 树、fsdriver 表、read 路径（A-1 已决策：内部模块） | CONVERGED |
| 03 | `03-devm-structs.md` | 核心结构全字段、状态机（UNBOUND/BOUND/ZOMBIE）、wire 格式（A-2/A-4/A-5） | CONVERGED |
| 04 | `04-device-tree.md` | root_dev + devices/ 树、DFS 查找、路径生成、dev_id 分配 | CONVERGED |
| 05 | `05-devm-message-contract.md` | 消息面：DEVMAN_* 常量、字段宏、grant 拷贝、do_reply、RS-only（A-3/A-6/A-9） | CONVERGED |
| 06 | `06-event-buf.md` | buf.c skip/offset 缓冲、事件队列（ADD/REMOVE + EOF 消费）、静态信息读取 | CONVERGED |
| 07 | `07-devm-add-device.md` | 设备添加：do_add_device + add_child + add_info + devman_id | CONVERGED |
| 08 | `08-devm-del-device.md` | 设备删除：引用计数 get/put、ZOMBIE 转换、del_device | CONVERGED |
| 09 | `09-devm-bind-unbind.md` | 绑定/解绑：RS 握手、转发 owner、状态转换、**绑定段路径图** + Server 装配 | CONVERGED |
| 10 | `10-libdevman-client.md` | 客户端库：序列化 + grant + devman_add/del_device + devman_handle_msg | CONVERGED |
| 11 | `11-usb-device-model.md` | USB 建模：usb_dev/interface、属性生成、add/remove 设备+接口 | CONVERGED |
| 12 | `12-rs-integration.md` | RS 契约：devman_id bind/unbind 握手、system.conf 权限 | CONVERGED |
| 13 | `13-devmand-consumer.md` | devmand 消费契约：事件解析、驱动匹配 DSL、启停脚本（外部契约，不实现） | CONVERGED |
| 99 | `99-devm-global-concepts.md` | 常量/错误码/跨服务引用收口 | CONVERGED |

## Rust 实现（`os/servers/devman/` + `os/libs/minix-sys/` + `os/libs/minix-types/`）

| 模块 | 来源篇 | 测试 |
|---|---|---|
| `hooks.rs`（FsHooks/RootStat/ServerConfig/FirstGuard/SEF） | 01 | 7 |
| `vtreefs/`（inode 树 + 服务器 + 传输） | 02 | 24 |
| `structs.rs` + `wire.rs`（形状 + 解析） | 03 | 7 |
| `device_tree.rs`（双树 + 寻路） | 04 | 7 |
| `ipc/`（相位视图 + 单分派） | 05 | 5 |
| `buf.rs` + `event_queue.rs` + `files.rs`（缓冲/队列/分发） | 06 | 9 |
| `add_device.rs` / `del_device.rs` / `bind.rs` + `server.rs` | 07/08/09 | 3 + 5 + 6 |
| `rs_contract.rs`（RS 握手决策） | 12 | 4 |
| minix-sys `devman_client.rs` + `usb_model.rs` | 10/11 | 7 + 5 |
| minix-types com.rs DEVMAN 块 + Errno assoc | 05/02/07/08 | 1 + 5（assoc 无独立测试，值测试覆盖） |

测试基线（2026-09-04）：`cargo test -p minix-devman` **78 passed** / `cargo test -p minix-sys` **13 passed** / `cargo test -p minix-types` **139 passed**；`cargo clippy` 新文件 0 警告。

## 待决事项（不阻塞，见 `.review/codex/devman/STATE.md`）

- OQ-3：设备名空格 07 是否拒绝（倾向拒，待用户决）
- P1-6/P1-1T/P1-10/P1-12：生产传输接线（minix-sys IPC `todo!()` 落地时），含 Transport 单 impl 三处
- OQ-1：SefHooks 生产 impl（minix-sef 实装时）

## 关键文件

- `plan.md` — 文档重组计划（定稿，含覆盖契约 §5 + ARCH 清单 §4 + 双轮 review 记录 §7；§6.1 全 reviewed）
- `draft/` — 旧占位 README（素材）
- `.review/codex/devman/` — 15 篇 scan/structure/SYMBOLS/VERIFY-CHECK + STATE.md（Codex 专属）
- `.design/` — 每篇 outline/outline-review/design 快照（版本化，review 输入非 ground truth）

## 启动链路位置

```
kernel → VM → RS → PM/SCHED/VFS/DS/MIB → IS/DEVMAN/INPUT/IPC → INIT（boot 终点）
```

devman **不在 boot_image**（`kernel/table.c:44-64` 无条目），由 RS 运行时加载（`etc/system.conf:422` `service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }`），挂载为 VTreeFS 文件系统（`/sys`，devmand 默认路径）。
