# 15-syscall-exit-signal: sys_exit / sys_kill 与信号处理

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: sys_exit / sys_kill + sig_delay_done(454) + cause_sig(389)
> **说明**: 进程退出、信号发送与分发——kernel 负责机制（信号传递），PM 负责策略（信号处理）
