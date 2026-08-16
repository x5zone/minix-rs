# 14-init-external-contracts: 对外契约（Minix 集成）

> **状态**: pending（最小骨架，待改写）
> **定位**: init 与 kernel/PM/RS/VFS/procfs/power 驱动的跨主体契约
> **源码**: `minix3/sbin/init/init.c`：`minixreboot`（517-528）、`minixpowerdown`（530-541）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- `minixreboot`（SIGABRT）：fork → `execl("/sbin/shutdown", "-r", "now", "CTRL-ALT_DEL")`——Ctrl-Alt-Del 链路（`tty/arch/i386/keyboard.c:300`）
- `minixpowerdown`（SIGUSR1）：fork → `execl("/sbin/shutdown", "-p", "now", "CTRL-ALT_DEL")`——低电链路（`drivers/power/tps65217/tps65217.c:226`）
- PM 契约：INIT 父进程=自身、INIT_PID=1、scheduler=KERNEL（pm/main.c:188-204）；孤儿收养（pm/forkexit.c:336,396）；INIT 死亡 → stacktrace（pm/forkexit.c:336-341）；reboot 路径 stop init（pm/misc.c:224）；RS 父进程也是 INIT（pm/main.c:203）；调度继承（pm/schedule.c:34,73）
- procfs：INIT 在 RS 表中但不算系统服务（procfs/service.c:195-207）
- boot argv：VM 固定 argv=`{"init",NULL}`（vm/main.c:346-347），`-s/-f` 在 boot 路径不生效
- USR_F 身份（rs/table.c:28）：init 无 SEF/IPC 主循环，生命周期不受 RS 管理

## 边界

- **前置依赖**: 02/03（信号、日志）
- **不覆盖（移交）**: 信号机制本身（02）、shutdown 命令实现（`minix3/sbin/shutdown/`）
