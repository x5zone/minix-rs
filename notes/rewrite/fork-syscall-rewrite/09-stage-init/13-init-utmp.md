# 13-init-utmp: utmp/utmpx 会话日志

> **状态**: pending（最小骨架，待改写）
> **定位**: 会话生命周期在登录/注销日志中的投影（/var/run/utmp[ x]、/var/log/wtmp[ x]）
> **源码**: `minix3/sbin/init/init.c`：`session_utmpx`（1372-1381）、`make_utmpx`（1383-1409）、`get_runlevel`（1411-1427）、`utmpx_set_runlevel`（1429-1458）、`clear_session_logs`（647-666）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- 记录点：LOGIN_PROCESS（add_session）/ DEAD_PROCESS（del_session/clear_session_logs）/ BOOT_MSG/BOOT_TIME + DOWN_MSG（read_ttys）/ RUN_LVL（transition 时 runlevel 转换）
- `make_utmpx`：ut_name/ut_line/ut_type/ut_pid/ut_tv/ut_session/ut_id 构造 + `pututxline`；`session_utmpx` 包装
- `get_runlevel`：状态函数指针 → runlevel 字符（'s'/'r'/'t'/'m'/'T'/'c'/'d'）；`utmpx_set_runlevel` 仅在 sessions 建立后记录
- `clear_session_logs`：`logoutx/logwtmpx`（SUPPORT_UTMPX）+ `logout/logwtmp`（SUPPORT_UTMP，minix 双启用）
- **ARCH A-2**：minix-rs 文件/记录服务未就绪 → defer + 语义契约

## 边界

- **前置依赖**: 06/07/09（记录点来源）
- **不覆盖（移交）**: 会话生命周期（07~11）、DB（08）
