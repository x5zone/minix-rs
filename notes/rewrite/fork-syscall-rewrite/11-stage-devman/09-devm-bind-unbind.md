# 09-devm-bind-unbind: 设备绑定与解绑

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → DEVMAN_BIND/UNBIND → do_bind/unbind_device（阶段 4 生命周期绑定段）
> **源码**: `minix3/minix/servers/devman/bind.c`（105 行全部）
> **Rust 模块**: `bind.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `do_bind_device`（bind.c:7-55）：RS-only（:14 EPERM）→ 查找（ENODEV）→ 转发 owner（ipc_sendrec DEVMAN_BIND）→ OK 时 state=BOUND + get → reply RS
- `do_unbind_device`（:56-105）：RS-only（:63）→ 转发 owner（DEVMAN_UNBIND）→ OK 或 ENODEV=19（:85 驱动已自删特例）→ 非 ZOMBIE 置 UNBOUND + put → reply RS
- 转发消息对端：libdevman `devman_handle_msg`/`do_bind`/`do_unbind`（10）
- DEVMAN_ENDPOINT 传递链：RS 设置 → devman 转发 → 驱动 bind_cb 入参（05/10/12）
- 绑定段路径图（plan §1.3）
- ARCH A-9：RS-only fail-closed

## 边界

- **前置依赖**: 05 + 08（refcount）+ 10（响应面）
- **不覆盖（移交）**: 客户端 bind_cb 实现（10/11）、RS 侧触发（12）
