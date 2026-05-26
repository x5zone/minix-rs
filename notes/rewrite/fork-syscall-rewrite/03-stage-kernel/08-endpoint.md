# 08-endpoint: Endpoint 进程标识与 Generation 机制

> **分类**: Kernel 进程抽象
> **源码**: `minix3/minix/kernel/proc.c` endpoint 部分(~100行), `minix3/minix/include/minix/endpoint.h`
> **说明**: endpoint_lookup / isokendpt_f / generation 递增——解决 slot 重用导致的"错把 B 当成 A"问题
