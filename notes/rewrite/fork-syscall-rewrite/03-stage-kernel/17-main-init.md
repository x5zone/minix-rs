# 17-main-init: kmain 与内核启动流程

> **分类**: Kernel 时间与初始化
> **源码**: `minix3/minix/kernel/main.c`(522行), `arch/i386/arch_system.c: arch_init`()(246), `bsp_finish_booting`()
> **说明**: kmain → bsp_finish_booting → announce → 启动所有 boot_proc → 进入调度循环——开机全链路
