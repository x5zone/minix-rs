# 11-stage-devman — DEVMAN 文档目录

> **状态**: 骨架就绪（plan.md 定稿 2026-08-16；各 doc 为最小骨架，待按 plan.md 改写）
> **主线**: DEVMAN server 启动顺序（main → run_vtreefs → mount 触发 init_hook → devman_init_devices → 主循环）；设备生命周期旅程（ADD → 事件 → devmand → RS bind）为次主线
> **Ground truth**: `minix3/minix/servers/devman/`（4 个 .c，1013 行）+ `minix3/minix/lib/libdevman/`（613 行）+ `minix3/minix/lib/libvtreefs/`（1642 行，使用面）

## 文档清单（15 篇）

| 编号 | 文档 | 语义模块 |
|------|------|---------|
| 00 | `00-devm-overview.md` | 总览：devman 是什么、启动主线图、设备生命周期次主线、导航 |
| 01 | `01-devm-init-main.md` | main()/三个 hooks/run_vtreefs 调用点/SEF 生命周期/mount 触发 init_hook |
| 02 | `02-vtreefs-framework.md` | VTreeFS 框架契约：inode 树、fsdriver 表、read 路径（A-1） |
| 03 | `03-devm-structs.md` | 核心结构全字段、状态机（UNBOUND/BOUND/ZOMBIE）、wire 格式（A-2/A-4/A-5） |
| 04 | `04-device-tree.md` | root_dev + devices/ 树、DFS 查找、路径生成、dev_id 分配 |
| 05 | `05-devm-message-contract.md` | 消息面：DEVMAN_* 常量、字段宏、grant 拷贝、do_reply、RS-only（A-3/A-6/A-9） |
| 06 | `06-event-buf.md` | buf.c skip/offset 缓冲、事件队列（ADD/REMOVE + EOF 消费）、静态信息读取 |
| 07 | `07-devm-add-device.md` | 设备添加：do_add_device + add_child + add_info + devman_id |
| 08 | `08-devm-del-device.md` | 设备删除：引用计数 get/put、ZOMBIE 转换、del_device |
| 09 | `09-devm-bind-unbind.md` | 绑定/解绑：RS 握手、转发 owner、状态转换、**绑定段路径图** |
| 10 | `10-libdevman-client.md` | 客户端库：序列化 + grant + devman_add/del_device + devman_handle_msg |
| 11 | `11-usb-device-model.md` | USB 建模：usb_dev/interface、属性生成、add/remove 设备+接口 |
| 12 | `12-rs-integration.md` | RS 契约：devman_id bind/unbind 握手、system.conf 权限 |
| 13 | `13-devmand-consumer.md` | devmand 消费契约：事件解析、驱动匹配 DSL、启停脚本（外部契约） |
| 99 | `99-devm-global-concepts.md` | 常量/错误码/跨服务引用收口 |

## 关键文件

- `plan.md` — 文档重组计划（定稿，含覆盖契约 §5 + ARCH 清单 §4 + 双轮 review 记录 §7）
- `draft/` — 旧占位 README（素材）
- `checklist.md` — 函数级基线（实现期创建，参照 02-stage-vm/checklist.md 模式）

## 启动链路位置

```
kernel → VM → RS → PM/SCHED/VFS/DS/MIB → IS/DEVMAN/INPUT/IPC → INIT（boot 终点）
```

devman **不在 boot_image**（`kernel/table.c:44-64` 无条目），由 RS 运行时加载（`etc/system.conf:422` `service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }`），挂载为 VTreeFS 文件系统（`/sys`，devmand 默认路径）。
