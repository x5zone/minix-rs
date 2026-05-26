# 10-async-ipc: 异步 IPC

> **分类**: Kernel IPC
> **源码**: `minix3/minix/kernel/proc.c`: mini_senda(1331), try_async(1348), cancel_async(1510), has_pending 系列
> **说明**: ASYNC 消息传递——发送方不阻塞、通过异步表传递通知的性能优化路径
