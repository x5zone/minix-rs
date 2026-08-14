# 15-ipc-dispatch: IPC 分发与主循环

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环（`main.c:112-190`）：收消息 → 验证 → 分发 → 回复
> **源码**: `minix3/minix/servers/vm/main.c:112-190,520-580`、`com.h`
> **Rust 模块**: `ipc/dispatcher.rs`、`ipc/transport.rs`
> **draft 素材**: `draft/24-vm-ipc-dispatch.md` + `draft/26-vm-init-main.md` 主循环部分（素材）
> **变更**: 合并——分发 + 主循环成一篇，前置于服务（18+）

## 核心点

- `vm_calls[]`/`CALLMAP` 注册面、`acl_check` 接线
- 主循环 5 优先级分发（VFS transid / RS_INIT / VM_PAGEFAULT / CALLMAP / notify）
- `SUSPEND` 伪返回码、transid 路由、`is_ipc_notify` 处理

## 边界

- **前置依赖**: 01 + 02~14 全部就绪
- **不覆盖（移交）**: 各 handler 实现（16~26）
