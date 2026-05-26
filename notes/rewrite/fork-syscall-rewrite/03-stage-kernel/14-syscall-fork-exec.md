# 14-syscall-fork-exec: sys_fork 和 sys_exec

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: sys_fork(L136-306, ~170行) + `arch_system.c` fork 相关
> **说明**: 内核 fork——创建子进程 proc 结构、复制寄存器、生成 endpoint、设置特权位
