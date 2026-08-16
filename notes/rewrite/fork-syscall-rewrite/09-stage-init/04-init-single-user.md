# 04-init-single-user: 单用户状态（'s'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'s'`——维护模式：单用户 shell
> **源码**: `minix3/sbin/init/init.c`：`single_user`（694-877）
> **Rust 模块**: 无（用户态进程逻辑）
> **draft 素材**: 无

## 核心点

- `getsecuritylevel()>0 → setsecuritylevel(0)`（降级到 insecure 模式）
- fork shell：`setctty(_PATH_CONSTTY|_PATH_CONSOLE)` → `execv("-sh", ...)` / `INIT_BSHELL` fallback（797-808）
- `SECURE`（A-7）：console tty 非 secure 且 securelevel≥2 时要求 root 密码（`getttynam`/`getpwnam`/`crypt`）
- `ALTSHELL`（A-7）：允许输入替代 shell 路径
- waitpid(-1, WUNTRACED) 主循环：shell 退出 → runcom；SIGKILL 终止 → `sigsuspend` 等待 reboot；停止 → SIGCONT 重启
- `requested_transition` 触发（SIGHUP/SIGTERM/SIGTSTP）→ 返回对应状态

## 边界

- **前置依赖**: 02/03（信号、日志）
- **不覆盖（移交）**: /etc/rc（05）、ttys（06）、securelevel 机制（12）、setctty 细节（09）
