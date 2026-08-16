# 02-init-state-machine: 状态机骨架与信号转换

> **状态**: pending（最小骨架，待改写）
> **定位**: `transition(requested_transition)` [init.c:624] 状态机主循环
> **源码**: `minix3/sbin/init/init.c`：`transition`（624-644）、`handle`（369-392）、`delset`（394-409）、`transition_handler`（1502-1526）、`alrm_handler`（1649-1659）
> **Rust 模块**: `state_machine.rs`（规划）
> **draft 素材**: 无

## 核心点

- 7 状态常量：`DEATH 'd'`/`SINGLE_USER 's'`/`RUNCOM 'r'`/`READ_TTYS 't'`/`MULTI_USER 'm'`/`CLEAN_TTYS 'T'`/`CATATONIA 'c'`
- `state_t` 函数指针表 + `transition()` 无退出 for 循环（返回 `state_func_t` 即下一状态）
- `requested_transition`（双默认值：195 runcom / 217 single_user）+ `transition_handler` 信号→状态映射（SIGHUP→clean_ttys、SIGTERM→death、SIGTSTP→catatonia）
- 信号注册机制：`handle()`/`delset()`（sigaction/sigprocmask，`SA_NOCLDSTOP`，注释 "XXX SA_RESTART?"）、job control 保护（SIGTTIN/SIGTTOU SIG_IGN）
- `alrm_handler`（DEATH_WATCH 闹钟，置 `clang`）
- POSIX 职责参考：`NOTES`（孤儿回收/controlling terminal/job control）
- ARCH A-8：信号抽象依赖 minix-rt（待核实）

## 边界

- **前置依赖**: 01（信号注册调用点）
- **不覆盖（移交）**: 各状态函数体（04~11）、日志（03）、minix 专用 reboot/powerdown（14）
