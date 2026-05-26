# 12-syscall-dispatch: 系统调用路由

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: kernel_call_dispatch(95) + kernel_call_finish(59) + kernel_call(136)
> **说明**: sys_call 入口 → 查 s_k_call_mask → do_xxx() 分发 → 写回返回值——单次系统调用的完整路径
