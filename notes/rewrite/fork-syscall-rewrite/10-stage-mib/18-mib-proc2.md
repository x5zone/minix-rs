# 18-mib-proc2: KERN_PROC2 进程信息

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 进程信息（PROC2）
> **源码**: `proc.c:596-917`
> **Rust 模块**: `proc/proc2.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_kern_proc2`：req/arg/elsz/elmax 四参数语义；req 过滤面（KERN_PROC_ALL/PID/SESSION/PGRP/TTY/UID/RUID/GID/RGID）
- 内核伪进程（PID 0）单独处理（kmatch 判定：ALL 或 arg==0）
- TTY 过滤细节：REVOKE 未支持（TODO）、zombie 用 fproc_tab 值、NODEV 语义
- SESSION/PGRP 无 job control（同值比较 TODO）
- `fill_proc2_common/kern/user`：kinfo_proc2 字段填充
- A-4：`kinfo_proc2` 布局（ps/top 消费者）

## 边界

- **前置依赖**: 16
- **不覆盖（移交）**: LWP（17）、ARGS（19）
