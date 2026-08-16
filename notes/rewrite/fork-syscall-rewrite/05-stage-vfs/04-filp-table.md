# 04-filp-table: filp 文件表

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 — 核心数据结构：文件描述符与 vnode 的中介
> **源码**: `file.h`（struct filp）、`filedes.c:73-249,313-430`（init/find/lock/close_filp）
> **Rust 模块**: （未实现）filp 模块
> **draft 素材**: `draft/04-filp-struct.md` + `draft/filp-refcount.md`（素材）

## 核心点

- struct filp 全字段：filp_mode/filp_flags/filp_count/filp_vno/filp_pos/filp_lock/select 字段族
- init_filps（main.c:489 调用点，filedes.c:73）
- 查找：get_filp/get_filp2/find_filp/find_filp_by_sock_dev
- 引用计数：filp_count>0 槽占用；close_filp 归零时 put_vnode
- 锁：lock_filp/unlock_filp/unlock_filps、filp_softlock/ioctl_fp
- FSF_UPDATE/BUSY/RD_BLOCK/WR_BLOCK/ERR_BLOCK 标志（select/驱动协同）

## 边界

- fd 表条目管理不覆盖（14）
- pipe/select 对 filp 字段的使用不覆盖（17/23）
