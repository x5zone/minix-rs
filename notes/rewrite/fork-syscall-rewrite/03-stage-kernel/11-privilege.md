# 11-privilege: struct priv 特权结构体

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/priv.h`(80行), `system.c` 特权操作(918-973)
> **说明**: priv 结构体的每个字段、s_k_call_mask 位图、priv_add_irq/io/mem——谁有权做什么的权限矩阵
