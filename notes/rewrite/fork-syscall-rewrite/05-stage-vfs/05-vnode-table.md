# 05-vnode-table: vnode 表

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 — 核心数据结构：文件对象缓存
> **源码**: `vnode.h`（struct vnode）、`vnode.c` 全文件
> **Rust 模块**: （未实现）vnode 模块
> **draft 素材**: `draft/05-vnode-struct.md` + `draft/vnode-refcount.md`（素材）

## 核心点

- struct vnode 全字段：v_fs_e/v_inode_nr/v_mode/v_size/v_ref_count/v_fs_count/v_bfs_e/v_dev/v_sdev/v_vmnt/v_lock
- init_vnodes（main.c:486 调用点，vnode.c:138）
- 生命周期：get_free_vnode/find_vnode/dup_vnode/put_vnode/vnode_clean_refs
- 双层引用计数：v_ref_count（VFS 层）/ v_fs_count（底层 FS 层）延迟同步，256 阈值触发 clean_refs
- 锁：lock_vnode/unlock_vnode/upgrade_vnode_lock、VNODE_* 映射到 TLL_*

## 边界

- 挂载表关联不覆盖（06）
- 路径解析对 vnode 的使用不覆盖（13）
