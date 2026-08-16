# 99-mib-global-concepts: 全局概念收口

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段 99）
> **源码**: `com.h`、`sys/sys/sysctl.h`、`minix/sysctl.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `MIB_PROC_NR=7`（endpoint.rs）、`MIB_BASE 0x1800`、`NR_MIB_CALLS=3`
- 常量总表：CTL_* 顶层 id、CTLTYPE_*、CTLFLAG_*、SYSCTL_VERS_1、元标识符、KERN_*/VM_*/HW_*/MINIX_*/TEST_* id
- 错误码汇总（A-11）：EPERM/EINVAL/ESRCH/ENOENT/EEXIST/EBUSY/ENOTEMPTY/ENOTDIR/EISDIR/ENOMEM/EOPNOTSUPP/ENAMETOOLONG/ENOSYS + EDONTREPLY/ERESTART
- 跨服务引用：kernel（copy/grant/时钟/CPU 统计）、PM（getnuid/GETPARAM/SI_PROC_TAB）、VFS（SI_DMAP_TAB/SI_PROCLIGHT_TAB）、VM（vm_info_*）、DS（label）、ProcFS（MINIX_PROC）、IPC/LWIP/UDS（RMIB）
- 单线程事件循环执行模型声明（与 Kernel SMP+BKL 的区别）

## 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~22）
