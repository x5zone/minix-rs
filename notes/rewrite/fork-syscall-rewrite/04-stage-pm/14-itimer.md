# 14: itimer

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 定时器
> **源码**: minix3/minix/servers/pm/alarm.c（33/70/82/93/160/222/247/279/317）
> **Rust 模块**: timer 模块（未实现，A-7）
> **draft 素材**: 无

## 核心点

do_itimer（ITIMER_REAL/VIRTUAL/PROF）、ticks↔timeval 转换（向上取整/溢出保护）、is_sane_timeval/MAX_SECS、set_alarm（set_timer/ALARM_ON）、get_realtimer/set_realtimer、getset_vtimer（sys_vtimer）、check_vtimer（SIGVTALRM/SIGPROF 重启）、cause_sigalrm（到期 → check_sig SIGALRM）、CLOCK notify 接线（main.c:65-67）

## 边界

- **前置依赖**: 04/11
- **不覆盖（移交）**: 内核定时器实现（01-stage-kernel/15-clock-timer.md）、信号投递细节（11/12）
