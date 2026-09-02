# 01-fsdriver-task: fsdriver 主循环与请求分发

> **状态**: pending（最小骨架，待改写）
> **定位**: 所有 FS server 的运行时骨架（框架层第 1 篇）
> **源码**: `minix3/minix/lib/libfsdriver/fsdriver.c`、`table.c`、`minix3/minix/include/minix/vfsif.h`
> **Rust 模块**: `minix-fs`（driver 框架）
> **draft 素材**: 无（新建）

## 核心点

- `fsdriver_task` 主循环：`sef_receive_status(ANY)` → `fsdriver_process`；`fsdriver_running || fsdriver_mounted` 终止条件；EINTR/sef_cancel 语义
- `fsdriver_process` 分发：`is_ipc_notify` 或 `m_source != VFS_PROC_NR` → `fdr_other`（不发回复）；transid 提取 `TRNS_GET_ID`/`TRNS_DEL_ID`；`fsdriver_mounted || call_nr == REQ_READSUPER` 才受理
- callvec 分发表（table.c，32 项）：`call_nr -= FS_BASE` 查表，未实现 → ENOSYS，未挂载非 READSUPER → EINVAL
- REQ_* 协议全集（vfsif.h）：REQ_GETNODE(1)~REQ_BPEEK(33)，`NREQS=34`，`IS_FS_RQ` 判定；REQ_GETNODE 标注 "Should be removed" 无对应项
- 回复协议：`m_out.m_type = TRNS_ADD_ID(r, transid)`；asyn_reply（多线程 FS，本 stage 单线程不支持）
- `fsdriver_terminate`：置 running=FALSE + `sef_cancel`

## 边界

- **前置依赖**: 00、`../05-stage-vfs/09-main-loop.md`
- **不覆盖（移交）**: 请求适配细节（02）、copy/dentry/lookup 辅助（03）、协议常量值（99）
