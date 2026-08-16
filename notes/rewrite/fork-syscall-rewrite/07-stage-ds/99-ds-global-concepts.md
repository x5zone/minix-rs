# 99-ds-global-concepts: 全局概念收口

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段 99）
> **源码**: `com.h:65,498-507`、`ds.h`、`sysinfo.h:13`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `DS_PROC_NR`、DSF/DS_* 常量表
- 错误码汇总（A-9：EPERM/EINVAL/ESRCH/ENOENT/EEXIST/EAGAIN/ENOMEM/EDONTREPLY）
- 跨服务引用：RS（`manager.c:513,800`）、VFS（`main.c:441`、`misc.c:960-985`）、PM（`misc.c`）、INPUT（`input.c:488-593`）、驱动库（DS_DRIVER_UP）、IS（`dmp_ds.c`）
- 单线程事件循环执行模型声明（与 Kernel SMP+BKL 的区别）

## 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~12）
