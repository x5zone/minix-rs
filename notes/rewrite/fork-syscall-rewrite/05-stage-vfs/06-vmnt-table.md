# 06-vmnt-table: vmnt 挂载点表

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 — 核心数据结构：挂载点
> **源码**: `vmnt.h`（struct vmnt）、`vmnt.c` 全文件
> **Rust 模块**: （未实现）vmnt 模块
> **draft 素材**: `draft/06-vmnt-struct.md`（素材）

## 核心点

- struct vmnt 全字段：m_fs_e/m_lock/m_comm/m_dev/m_flags/m_fs_flags/m_mounted_on/m_root_node/m_label/m_mount_path/m_mount_dev/m_fstype/m_stats
- init_vmnts（main.c:487 调用点，vmnt.c:127）
- 表管理：get_free_vmnt/find_vmnt/mark_vmnt_free/clear_vmnt/fetch_vmnt_paths
- 锁族：lock_vmnt/unlock_vmnt/downgrade/upgrade、VMNT_READ/WRITE/EXCL 映射
- VMNT_READONLY/CALLBACK/MOUNTING/FORCEROOTBSF/CANSTAT 标志
- vmnt_unmap_by_endpt：FS 退出时清理

## 边界

- mount/umount 流程不覆盖（18）
- m_comm 请求队列不覆盖（11）
