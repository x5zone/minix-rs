# 18-mount: 挂载管理

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 8 — 挂载管理
> **源码**: `mount.c` 全文件、`glo.h`（ROOT_DEV/ROOT_FS_E）
> **Rust 模块**: （未实现）mount 模块
> **draft 素材**: 无（新建）

## 核心点

- do_mount/mount_fs：REQ_READSUPER/REQ_MOUNTPOINT 挂载链
- do_umount/unmount/unmount_all（含 force）
- mount_pfs：管道 FS 挂载（启动链，01 调用点）
- name_to_dev/find_free_nonedev/update_bspec：设备名解析与 bspec 更新
- is_nonedev：伪设备判定
- ROOT_DEV/ROOT_FS_E/have_root 全局

## 边界

- vmnt 表结构不覆盖（06）
- FS 协议包装不覆盖（12）
