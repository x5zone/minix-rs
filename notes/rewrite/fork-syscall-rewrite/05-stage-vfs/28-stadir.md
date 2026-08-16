# 28-stadir: 目录与 stat

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 13 — 目录/链接/权限
> **源码**: `stadir.c` 全文件
> **Rust 模块**: （未实现）stadir 模块
> **draft 素材**: 无（新建）

## 核心点

- do_chdir/do_fchdir/do_chroot：目录切换与 change_into
- do_stat/do_fstat/do_lstat：元数据读取
- do_statvfs/do_fstatvfs/do_getvfsstat：文件系统统计
- update_statvfs/fill_statvfs：statvfs 缓存（m_stats）

## 边界

- 路径解析不覆盖（13）
- 权限检查不覆盖（29）
