# 13-syscall-memory: 内存相关系统调用（sys_vmctl 等）

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: sys_vmctl / sys_vm_map / vm_ctl_* + `arch/i386/arch_system.c` 相关
> **说明**: VM 完成页表决策后通过 sys_vmctl 通知内核执行——SET_PDBR、MAP_PHYS、GET_MAPPING 等
