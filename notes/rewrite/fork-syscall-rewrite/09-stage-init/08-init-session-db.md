# 08-init-session-db: 会话数据库

> **状态**: pending（最小骨架，待改写）
> **定位**: pid → session 快速查找（collect_child 回收路径使用）
> **源码**: `minix3/sbin/init/init.c`：`start_session_db`（1021-1036）、`add_session`（1038-1060）、`del_session`（1062-1078）、`find_session`（1080-1099）
> **Rust 模块**: `session_db.rs`（规划）
> **draft 素材**: 无

## 核心点

- **ARCH A-1**：Berkeley DB 内存哈希表（`dbopen(NULL, O_RDWR, 0, DB_HASH, NULL)`，`db.h:72,213`，pid→session 指针）→ Rust `HashMap<pid_t, Rc<Session>>`（或 slot 数组），消除 libdb 依赖
- `start_session_db` 失败 → read_ttys 回退（warning "start_session_db failed, death"）
- `add_session`/`del_session`：DB put/del + utmpx 记录（调用点，机制归 13）
- `find_session`：DB get，未命中返回 NULL

## 边界

- **前置依赖**: 07（session 类型）
- **不覆盖（移交）**: session 字段细节（07）、utmpx 机制（13）
