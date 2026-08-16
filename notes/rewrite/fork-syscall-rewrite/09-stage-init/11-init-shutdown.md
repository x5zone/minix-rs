# 11-init-shutdown: 关停状态（'c'/'d'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'c'` catatonia（boring）与 `'d'` death（shutdown）
> **源码**: `minix3/sbin/init/init.c`：`catatonia`（1634-1647）、`death`（1661-1701）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- `catatonia`：全部会话 SE_SHUTDOWN（不再重启 getty）→ 返回 multi_user（等待信号）
- `death`：全部 SE_SHUTDOWN + `logwtmpx("~","shutdown",...)`（归 13）→ 三轮 `kill(-1, SIGHUP/SIGTERM/SIGKILL)`（death_sigs）
- 每轮 `alarm(DEATH_WATCH 10)` + waitpid 循环，`clang` 由 `alrm_handler` 置位判定轮次结束；`errno==ECHILD` → 回 single_user
- 三轮后仍有进程存活 → warning "some processes would not die; ps axl advised" → single_user
- `disaster` 的 `_exit(sig)` 触发内核 reboot 路径（联动 03/14）

## 边界

- **前置依赖**: 09（会话回收路径）
- **不覆盖（移交）**: 单条会话回收细节（09）、utmp 日志（13）、minixreboot（14）
