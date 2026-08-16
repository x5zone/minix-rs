# 09-init-multi-user: 多用户稳态（'m'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'m'`——系统稳态：为每个会话启动 getty，waitpid 主循环
> **源码**: `minix3/sbin/init/init.c`：`multi_user`（1528-1567）、`start_getty`（1321-1370）、`start_window_system`（1290-1319）、`setctty`（669-692）、`collect_child`（1460-1500）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- `getsecuritylevel()==0 → setsecuritylevel(1)`（secure 模式提升，调用点归 12）
- 遍历 sessions：无 `se_process` 的会话 `start_getty`；失败 → `requested_transition = clean_ttys`
- `start_getty`：fork → chroot（若 did_multiuser_chroot）→ getty 防抖动（GETTY_SPACING 5s 内重复 → `sleep(GETTY_SLEEP 30)`）→ window 系统（WINDOW_WAIT 3s）→ `execv(se_getty_argv)`（1365）
- `start_window_system`：fork → setsid → `execv(se_window_argv)`（1312）
- `setctty`：minix 分支 `setsid()`（无 revoke）+ `nanosleep(dtrtime 250ms)`（DTR 拉低）+ `open(name, O_RDWR)` + `login_tty`
- waitpid(-1) 主循环 + `collect_child`：find_session → clear_session_logs（归 13）→ del_session → SE_SHUTDOWN 则 free_session，否则 `start_getty` 重启

## 边界

- **前置依赖**: 06/07/08（会话 + DB）
- **不覆盖（移交）**: clean_ttys（10）、关停（11）、utmpx 日志（13）、securelevel 机制（12）
