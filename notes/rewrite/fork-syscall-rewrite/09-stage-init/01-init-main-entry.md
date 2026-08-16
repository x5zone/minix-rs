# 01-init-main-entry: main() 入口与进程身份

> **状态**: pending（最小骨架，待改写）
> **定位**: boot 链终点 → `main()` [init.c:229] 入口全流程，02~14 的锚点
> **源码**: `minix3/sbin/init/init.c`：`main`（229-367）、`mfs_dev`（1703-1790）
> **Rust 模块**: `os/commands/sbin/init/src/main.rs`
> **draft 素材**: 无

## 核心点

- 身份校验：`getuid()!=0 → EPERM`、`getpid()!=1 → "already running"`
- `setsid()` 建立初始会话；`mfs_dev()`（/dev/console 缺失时跑 MAKEDEV，minix 专用）
- `getopt`：`-s` 单用户 / `-f` fastboot（真实 boot 路径由 VM 固定 argv=`{"init",NULL}`，无参数）
- 信号注册调用点（handler 语义归 02/03/14）：SIGABRT→minixreboot、SIGUSR1→minixpowerdown、致命信号→disaster、SIGHUP/SIGTERM/SIGTSTP→transition_handler、SIGALRM→alrm_handler、SIGTTIN/SIGTTOU→SIG_IGN
- `close(0/1/2)` + `securelevel_present = has_securelevel()` → `transition(requested_transition)`

## 边界

- **前置依赖**: 00 + `../01-stage-kernel/06-proc-init-boot-proc.md`、`../01-stage-kernel/09-vm-boot-protocol.md` + PM 初始化（`../04-stage-pm/`）
- **不覆盖（移交）**: 状态机细节（02）、信号 handler 语义（02/03/14）、securelevel 机制（12）、/dev 建设细节（mfs_dev 归本 doc 但 MAKEDEV 脚本语义从简）
