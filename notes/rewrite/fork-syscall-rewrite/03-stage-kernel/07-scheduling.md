# 07-scheduling: 进程调度

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.c` 调度部分(~700行)
> **说明**: pick_proc / enqueue / dequeue / switch_to_user / idle / 优先级队列——决定谁运行、运行多久
