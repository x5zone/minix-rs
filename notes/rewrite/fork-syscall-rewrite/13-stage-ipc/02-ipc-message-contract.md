# 02-ipc-message-contract: IPC 协议面

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（所有 handler 篇的前置）
> **源码**: `minix3/minix/include/minix/com.h:785-796,610-619,1151`、`minix3/minix/include/minix/ipc.h:355-422,1802-1813`、`minix3/minix/servers/ipc/ipc.conf`
> **Rust 模块**: `minix-types`（**缺 ipc.rs 消息类型，A-1**）
> **draft 素材**: 无（新建）

## 核心点

- call numbers：`IPC_BASE 0xD00`、`IPC_SHMGET 0xD01` ~ `IPC_SEMOP 0xD07`（7 个）
- 7 种消息结构字段级语义：`mess_lc_ipc_semget`（key/nr/flag/retid）、`semctl`（id/num/cmd/opt/ret）、`semop`（id/ops 指针/size）、`shmget`（key/size/flag/retid）、`shmat`（id/addr/flag/retaddr）、`shmdt`（addr）、`shmctl`（id/cmd/buf/ret）
- `mess_pm_lsys_proc_event`（endpt/event）+ `PROC_EVENT`/`PROC_EVENT_REPLY`（com.h:610-619）
- `SUSPEND`（-998）特殊语义：handler 返回 SUSPEND → 不回复，后异步恢复
- `ipc.conf` 特权面：system `UMAP`(14)/`VIRCOPY`(15)、uid 0、ipc 可接收端点（SYSTEM USER pm rs log tty ds vm）、vm `REMAP`/`REMAP_RO`/`SHM_UNMAP`/`GETPHYS`/`GETREF`
- SysV 常量：`IPC_R/W/M`、`IPC_CREAT/EXCL/NOWAIT`、`IPC_PRIVATE`、`IPC_RMID/SET/STAT/INFO=500`、`IXSEQ_TO_IPCID` 编码（→ 99 收口）
- **A-1**：minix-types 消息类型缺口（7 组 In/Out + ProcEvent，参照 `vm.rs` 模式）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 消息处理流程（01）、handler 流程（05~08）、权限模型（04）
