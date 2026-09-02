# 19-procfs: ProcFS：进程信息文件系统

> **状态**: pending（最小骨架，待改写）
> **定位**: 虚拟树变体：进程信息展示（RS 运行时加载）
> **源码**: `minix3/minix/fs/procfs/`（8 个 .c）
> **Rust 模块**: `os/fs/procfs`
> **draft 素材**: 无（新建）

## 核心点

- procfs 定位：进程/内核信息虚拟 FS（VTreeFS hooks 实现）
- 静态树构造：root_files 数组 → construct_tree 递归建目录
- 动态 PID 子树（tree.c）：`proc_list`（minix_proc_list）、`pid_from_slot`（NR_TASKS 偏移）、`check_owner`/`make_stat`、init_tree（内核进程表同步）
- pid.c：per-PID 文件（cmdline/stat 等）、service.c：服务信息文件、cpuinfo.c：CPU 信息
- hooks 实现：lookup_hook/getdents_hook/read_hook/rdlink_hook
- buf.c：I/O 缓冲管理；util.c：辅助

## 边界

- **前置依赖**: 18
- **不覆盖（移交）**: 内核进程表细节（01-stage-kernel）、VTreeFS 框架（18）
