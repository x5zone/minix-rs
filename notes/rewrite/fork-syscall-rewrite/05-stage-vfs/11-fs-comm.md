# 11-fs-comm: FS 通信原语

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — FS 通信协议：请求队列
> **源码**: `comm.c` 全文件、`request.h`、`com.h:909-912`（transid）、`vfsif.h:79-81`
> **Rust 模块**: （未实现）fs 通信模块
> **draft 素材**: `draft/18-comm.md`（素材）

## 核心点

- sendmsg/send_work/fs_sendmore/fs_cancel：请求队列刷新与并发窗口（c_max_reqs）
- fs_sendrec/drv_sendrec/vm_sendrec：同步往返（worker_wait）
- queuemsg：无空闲 worker 时排队
- VFS_TRANSID 编码：TRNS_ADD_ID/GET_ID/DEL_ID，回复路由
- m_comm 队列、sending 计数、VMNT_CALLBACK 抑制
- vm_vfs_procctl_handlemem（VM 交互，交叉 02-stage-vm）

## 边界

- req_* 包装函数不覆盖（12）
- 具体调用流程不覆盖（14~31）
