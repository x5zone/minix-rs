# 99-init-global-concepts: 全局概念总表

> **状态**: pending（最小骨架，待改写）
> **定位**: init 文档共用的常量、路径、全局状态
> **源码**: `minix3/sbin/init/pathnames.h`（40 行）、`minix3/include/paths.h`、`minix3/minix/servers/pm/const.h:9`
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- 常量：`INIT_PID 1`（pm/const.h:9）、`INIT_BSHELL`（paths.h:121,125）、状态字符 `'d'/'s'/'r'/'t'/'m'/'T'/'c'`、超时（GETTY_SPACING 5 / GETTY_SLEEP 30 / WINDOW_WAIT 3 / STALL_TIMEOUT 30 / DEATH_WATCH 10）、`dtrtime 250ms`
- 路径：`/etc/rc`（_PATH_RUNCOM）、`/etc/ttys`（ttyent.h）、`/dev/console`、`/dev/constty`（paths.h:62-63）、`/var/run/utmpx`、`/var/log/wtmpx`（utmpx.h:39-40）、`/var/run/utmp`、`/var/log/wtmp`（utmp.h:42-43）、`/sbin/shutdown`、`_PATH_SLOGGER`（死常量）
- 全局状态（行号 = init.c）：`runcom_mode`（151）、`clang`（173）、`securelevel_present`（177）、`sessions`（187）、`session_db`（194）、`requested_transition`（195/217 双默认值）、`boot_time`（200）、`current_state`（201）、`did_multiuser_chroot`（210）、`rootdir`（211）
- ARCH A-7：构建变体编译宏 → Rust feature flags

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（01~14）
