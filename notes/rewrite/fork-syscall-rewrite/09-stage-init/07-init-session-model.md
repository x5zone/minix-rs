# 07-init-session-model: 会话数据结构与生命周期

> **状态**: pending（最小骨架，待改写）
> **定位**: 会话模型——init 管理的每个终端会话的完整状态
> **源码**: `minix3/sbin/init/init.c`：`new_session`（1142-1183）、`free_session`（1123-1140）、`setupargv`（1185-1220）、`construct_argv`（1101-1121）
> **Rust 模块**: `session.rs`（规划）
> **draft 素材**: 无

## 核心点

- `session_t` 全字段：se_index/se_process/se_started/se_flags/se_device/se_getty(+argv)/se_window(+argv)/se_prev/se_next
- `SE_SHUTDOWN 0x1`（不重启）/ `SE_PRESENT 0x2`（在 /etc/ttys 中）
- 生命周期：`new_session`（按 ttys 行，插入链表尾）↔ `free_session`（释放 argv/device/window）
- `setupargv`：`"getty name"` / window 命令组装 + `construct_argv` 空格分词（`" \t"` 分隔符）
- `se_started` 防抖动时间戳（配合 GETTY_SPACING/GETTY_SLEEP）

## 边界

- **前置依赖**: 06（read_ttys 调用点）
- **不覆盖（移交）**: 会话 DB（08）、启动/重启流程（09）、清理（10）
