# 03-init-logging-failure: 日志与故障路径

> **状态**: pending（最小骨架，待改写）
> **定位**: 全部状态的共享基础设施——日志三件套 + 致命信号处理
> **源码**: `minix3/sbin/init/init.c`：`stall`（440-455）、`warning`（457-470）、`emergency`（472-488）、`disaster`（504-515）
> **Rust 模块**: `log.rs`（规划）
> **draft 素材**: 无

## 核心点

- 日志三件套语义：`stall`（vsyslog LOG_ALERT + `sleep(STALL_TIMEOUT)` 防刷屏）、`warning`（LOG_ALERT 不睡眠）、`emergency`（LOG_EMERG）
- `openlog("init", LOG_CONS, LOG_AUTH)`；ARCH A-3：minix-rs 无 syslog 服务 → 简化通道（defer）
- `disaster`：致命信号（SIGFPE/SIGILL/SIGSEGV/SIGBUS）→ emergency + sleep + `_exit(sig)`（触发内核 reboot 路径）
- 超时常量：`STALL_TIMEOUT 30`

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 状态转换（02）、minixreboot/minixpowerdown（14）、时间抽象（ARCH A-10，依赖 minix-rt）
