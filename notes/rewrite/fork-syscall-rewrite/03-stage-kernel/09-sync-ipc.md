# 09-sync-ipc: 同步 IPC（SEND / RECEIVE / BOTH / NOTIFY）

> **分类**: Kernel IPC
> **源码**: `minix3/minix/kernel/proc.c: do_ipc()`(L599-773, ~175行), `proc.c` 相关辅助函数
> **说明**: Minix3 微内核通信的四个原语——进程间消息传递的核心协议
