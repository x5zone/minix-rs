# 99-is-global-concepts: 全局概念收口

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段 99）
> **源码**: `com.h`（is_notify:93、TTY_FKEY_CONTROL:874、FKEY_*:875-877、GET_*:316-331、DIAGCTL_CODE_*:412-415、VM_INFO:729、VMIW_*:732-734）、`sysinfo.h:11-17`、`keymap.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- TTY_PROC_NR、F-key 常量表（F1~F12/SF1~SF12，keymap.h）
- 常量表：GET_*（sys_getinfo 子请求）、SI_*（getsysinfo 数据项）、VMIW_*（VM_INFO）、DIAGCTL_CODE_*
- 错误码汇总：EDONTREPLY（notify 不回复）/EPERM/EINVAL/ENOSYS
- 单线程事件循环执行模型声明（与 Kernel SMP+BKL 的区别）
- 无固定 endpoint（A-9）：IS_PROC_NR 不存在，endpoint 由 RS 动态分配
- 跨服务引用：TTY（func_key 通知）、VM（do_info）、PM/VFS/RS/DS（do_getsysinfo）、kernel（do_getinfo/do_diagctl）

## 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~10）
