# 99-ipc-global-concepts: 全局概念与常量收口

> **状态**: pending（最小骨架，待改写）
> **定位**: 99 全局概念（常量/错误码/编码收口）
> **源码**: `minix3/sys/sys/ipc.h`、`minix3/sys/sys/sem.h`、`minix3/sys/sys/shm.h`、`minix3/minix/include/minix/com.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: 无（新建）

## 核心点

- 常量表：SEMMNI=10/SEMMNS=60/SEMMSL/SEMMNU=30/SEMUME=10/SEMOPM=100/SEMVMX=32767/SEMAEM=16384、SHMMNI=1024/SHMSEG=32、SEM_ALLOC=01000/SHM_ALLOC=0x0800/SHM_DEST=0x0400、IPC_R=0400/IPC_W=0200/IPC_M=010000、IPC_CREAT=001000/EXCL=002000/NOWAIT=004000/PRIVATE=0、IPC_INFO=500/SEM_STAT=18/SEM_INFO=19
- errno 特殊语义：`SUSPEND`=-998（不回复）、`EDONTREPLY`=203（伪码：不回复）、`EIDRM`=82（Identifier removed）、`EINTR`/`E2BIG`/`EFBIG`/`ERANGE`/`ENOSPC`/`EEXIST`/`ENOENT`/`EACCES`/`EPERM`/`EINVAL`/`ENOMEM`/`EOPNOTSUPP`=45
- IPCID↔IX/SEQ 编码：`IXSEQ_TO_IPCID` / `IPCID_TO_IX` / `IPCID_TO_SEQ`、`_seq` 递增 `& 0x7fff`
- endpoint 语义：`IPC_PROC_NR`、`_ENDPOINT_P` 槽位索引、`m_source` 校验
- 跨服务引用收口：kernel IPC 原语（01-stage-kernel/12）、VM 服务（02-stage-vm）、MIB（10-stage-mib）、RS（03-stage-rs）、PM 进程事件

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 各机制细节（01~10）
