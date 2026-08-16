# 06-init-read-ttys: /etc/ttys 解析与会话重建（'t'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'t'`——读取 `/etc/ttys`，重建会话链表与数据库
> **源码**: `minix3/sbin/init/init.c`：`read_ttys`（1222-1288）、`do_setttyent`（1792-1809）
> **Rust 模块**: `ttys.rs`（规划）
> **draft 素材**: 无

## 核心点

- 清空旧会话链表（`free_session`）+ `start_session_db()` 失败回退（chroot 时 → death，否则 → single_user）
- `do_setttyent`（chroot 感知的 setttyentpath）+ `getttyent` 循环 → 每行 `new_session`（session_index 递增）
- 会话数据（utmpx 首次时 `make_utmpx(BOOT_MSG/BOOT_TIME)`、wtmpx down_time 推断）——调用点归 06，机制归 13
- `/etc/ttys` 格式：`name getty [status] [window]`（`ttyent.h`），`TTY_ON`/`TTY_SECURE` 标志
- ARCH A-6：libc getttyent/setttyent → Rust 自有解析器

## 边界

- **前置依赖**: 02/03
- **不覆盖（移交）**: 会话结构细节（07）、DB 实现（08）、utmpx 机制（13）
