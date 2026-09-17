# 99-init-global-concepts: 全局概念总表

> **状态**: 正文 v1（2026-09-18，随实体接线轮补全常量）
> **定位**: init 文档共用的常量、路径、全局状态
> **源码**: `minix3/sbin/init/pathnames.h`（40 行）、`minix3/include/paths.h`、`minix3/minix/servers/pm/const.h:9`
> **Rust 模块**: 常量分散在各业务模块（见 §2），信号编号权威在 `minix-types/src/types/signal.rs`

## 核心点

- 常量：`INIT_PID 1`（pm/const.h:9）、`INIT_BSHELL`（paths.h:121,125）、状态字符 `'d'/'s'/'r'/'t'/'m'/'T'/'c'`、超时（GETTY_SPACING 5 / GETTY_SLEEP 30 / WINDOW_WAIT 3 / STALL_TIMEOUT 30 / DEATH_WATCH 10）、`dtrtime 250ms`
- 路径：`/etc/rc`（_PATH_RUNCOM）、`/etc/ttys`（ttyent.h）、`/dev/console`、`/dev/constty`（paths.h:62-63）、`/var/run/utmpx`、`/var/log/wtmpx`（utmpx.h:39-40）、`/var/run/utmp`、`/var/log/wtmp`（utmp.h:42-43）、`/sbin/shutdown`、`_PATH_SLOGGER`（死常量）
- 全局状态（行号 = init.c）：`runcom_mode`（151）、`clang`（173）、`securelevel_present`（177）、`sessions`（187）、`session_db`（194）、`requested_transition`（195/217 双默认值）、`boot_time`（200）、`current_state`（201）、`did_multiuser_chroot`（210）、`rootdir`（211）
- ARCH A-7：构建变体编译宏 → Rust feature flags

## 常量表（Rust 侧落点，2026-09-18 补全）

| 常量 | C 值与锚点 | Rust 落点 |
|---|---|---|
| `INIT_BSHELL` = `/bin/sh` | paths.h（init.c:105 `INIT_BSHELL`） | `runcom::RC_SHELL_PATH`（兼 `single_user` 默认 shell） |
| `/sbin/shutdown` | init.c:521-522/534-535 字面量 | `contracts::SHUTDOWN_PATH` |
| `_PATH_RUNCOM` = `/etc/rc` | pathnames.h:40 | `runcom::RUNCOM_SCRIPT` |
| `_PATH_CONSOLE` = `/dev/console` | paths.h:62 | `single_user::CONSOLE_PATH` |
| `_PATH_CONSTTY` = `/dev/constty` | paths.h:63 | `single_user::CONSTTY_PATH` |
| `INIT_PATH` = `_PATH_STDPATH` | init.c:103/107（八段 PATH） | `single_user::INIT_PATH` |
| `_PATH_TTYS` = `/etc/ttys` | paths.h（ttyent.h 同域） | `driver::TTYS_PATH` |
| `GETTY_SPACING/SLEEP`、`WINDOW_WAIT` | init.c:92-94 | `multi_user::GETTY_*`/`WINDOW_WAIT_SECS` |
| `STALL_TIMEOUT` = 30 | init.c:95 | `log::STALL_TIMEOUT_SECS` |
| `DEATH_WATCH` = 10 | init.c:96 | `shutdown::DEATH_WATCH_SECS` |
| `death_sigs` = {HUP,TERM,KILL} | init.c:1667 | `shutdown::DEATH_SEQUENCE` |
| `RUNLVL_MSG` = `"run-level %c"` | utmpx.h:74 | `utmp::RUNLVL_MSG` |
| 信号编号族 | signal.h:52-84（SIGUSR1=30） | `state_machine::sig`（minix-types 权威镜像，见 §3） |

## 全局状态 → Rust 归宿（C 全局 → 单所有者视图）

**[ARCH: init-host-seam]**（plan.md §4 A-11）：`runcom_mode` → `DriverState.mode`；`clang` → `SignalState.clang`（Arc，handler 与 death 共享）；`requested_transition` → `SignalState.requested` 原子状态字符；`sessions`/`session_db` → `DriverState.sessions`+`db`；`did_multiuser_chroot`/`rootdir` → `DriverState` 字段；`boot_time` → `now_secs()` 时间戳。

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（01~14）
