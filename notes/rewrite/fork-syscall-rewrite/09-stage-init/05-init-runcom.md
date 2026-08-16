# 05-init-runcom: 运行 /etc/rc（'r'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'r'`——执行系统启动脚本 `/etc/rc`，决定进入多用户
> **源码**: `minix3/sbin/init/init.c`：`runcom`（974-1019）、`runetcrc`（879-972）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- `runetcrc(trychroot)`：fork → `setctty(_PATH_CONSOLE)` → `execv(sh, "/etc/rc", "autoboot"|NULL)`（`runcom_mode` AUTOBOOT/FASTBOOT）
- 失败回退：fork 失败/非正常退出/非零退出码 → `single_user`
- `CHROOT`（A-5）：`shouldchroot()` 决策后 `runetcrc(1)` 在 `init.root` 指定的 rootdir 内二次执行；`did_multiuser_chroot` 标记
- 成功 → `read_ttys`；`runcom_mode = AUTOBOOT` 复位
- SIGTERM + requested_transition==catatonia 时 `sigsuspend` 等待

## 边界

- **前置依赖**: 04（single_user 退出路径）
- **不覆盖（移交）**: chroot sysctl 机制（12）、ttys 解析（06）
