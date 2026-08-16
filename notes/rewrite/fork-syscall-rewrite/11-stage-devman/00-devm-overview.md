# 00-devm-overview: DEVMAN 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/devman/`（4 个 .c，1013 行）+ `minix3/minix/lib/libvtreefs/`（1642 行，devman 使用面）+ `minix3/minix/lib/libdevman/`（613 行）
> **Rust 模块**: `os/servers/devman/` 全部
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- devman 是什么：设备管理器，运行在 VTreeFS 框架之上的用户态服务（`run_vtreefs` 主循环），维护设备树 + 事件队列，协调设备驱动注册与绑定（`/sys` 设备节点与驱动实例生命周期）
- 启动主线图：RS 运行时加载（不在 boot_image）→ `main()` → `run_vtreefs` → mount 触发 `init_hook` → `devman_init_devices` → 主循环（VFS 请求 + `message_hook` 消息分发）（plan §1.2）
- 设备生命周期次主线：驱动 `devman_add_device` → ADD 事件 → devmand 消费 → RS `DEVMAN_BIND` 握手（plan §1.3）
- 文档导航：15 篇（00 + 01~13 + 99），7 阶段，新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH 焦点：VTreeFS 框架依赖（A-1）、message_hook fall-through 缺陷（A-3）、wire 格式（A-4）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~13、99）
