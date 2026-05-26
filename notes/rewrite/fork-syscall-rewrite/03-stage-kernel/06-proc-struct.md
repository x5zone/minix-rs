# 06-proc-struct: struct proc 进程控制块

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.h`(~160行结构体) + `const.h` RTS/misc 标志位
> **说明**: proc 结构体的所有字段 + RTS_FLAGS 逐位解释——内核"认识"一个进程的唯一方式
