# 02-mib-message-contract: MIB IPC 协议面

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（所有 handler 篇的前置）
> **源码**: `com.h:613-622,1026-1028`、`ipc.h`、`sys/sys/sysctl.h`、`minix/sysctl.h`
> **Rust 模块**: `minix-types`（**缺 mib.rs 消息类型，A-1**）
> **draft 素材**: 无（新建）

## 核心点

- call numbers：`MIB_SYSCTL`（阻塞 SENDREC）/`MIB_REGISTER`/`MIB_DEREGISTER`（单向，SENDREC 返回 ENOSYS 防交叉死锁）、`MIB_BASE 0x1800`、`NR_MIB_CALLS=3`
- 6 种消息结构字段级语义：`mess_lc_mib_sysctl`/`mess_mib_lc_sysctl`/`mess_lsys_mib_register`/`mess_mib_lsys_call`/`mess_mib_lsys_info`/`mess_lsys_mib_reply`（ipc.h）
- 交换格式：`sysctlnode`/`sysctldesc`（SYSCTL_VERS_1 布局 ABI，A-4）、元标识符 CTL_QUERY=-2/CREATE=-3/CREATESYM=-4/DESTROY=-5/MMAP=-6/DESCRIBE=-7
- 类型/标志宏：CTLTYPE_*（NODE=1..BOOL=6）、CTLFLAG_* 全集、`SYSCTL_VERS/FLAGS/TYPE` 掩码、`SYSCTL_NODE_FN`、内部标志 PARENT/VERIFY/REMOTE 不暴露
- 顶层 CTL_* id（KERN=1/VM=2/NET=4/HW=6/USER=8/VENDOR=11/MINIX=32）+ KERN_* 全集（PROC2=47/ARGS=48/LWP=64/BOOTTIME=83，KERN_MAXID=85）
- errno 特殊语义（A-11）：ENOMEM 溢出约定、分配失败不返回 ENOMEM、EEXIST 带 oldlen、EDONTREPLY/ERESTART
- A-1：minix-types 消息类型缺口（全部字段为 32/64 位标量，64 位布局兼容）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 消息解码流程（01）、拷贝原语实现（06）、树语义（03~20）
