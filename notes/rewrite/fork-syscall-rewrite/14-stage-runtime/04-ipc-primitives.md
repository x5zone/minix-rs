# 04-ipc-primitives: IPC 原语与内核陷阱 ABI

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 — 系统调用机制
> **源码**: `minix3/minix/include/minix/ipcconst.h`、`arch/i386/include/ipcconst.h`、`libc/arch/i386/sys/_ipc.S`、`ipc_minix_kerninfo.S`、`libsys/asynsend.c`
> **Rust 模块**: `os/libs/minix-sys`（send/receive/sendrec/notify）、`os/libs/minix-types`（Notify/ipc）
> **draft 素材**: 无（新建）

## 核心点

- 内核陷阱 ABI：IPCVEC_INTR=33/KERVEC_INTR=32、寄存器约定（eax/ebx/ecx）、IPC_STATUS 编码（ipcconst.h）
- 六原语：SEND=1/RECEIVE=2/SENDREC=3/NOTIFY=4/SENDNB=5/SENDA=16/MINIX_KERNINFO=6（_ipc.S）
- ipc_minix_kerninfo.S（MINIX_KERNINFO 请求，返回 kerninfo 指针）
- asynmsg_t/AMF_* 标志（asynsend.c）
- ARCH A-6：int 32/33 陷阱 → x86-64 syscall 指令 + SyscallArch trait

## 边界

- **前置依赖**: 03（IPC vecs 安装）
- **不覆盖（移交）**: `_syscall` 消息协议与 errno 回传（05）、各服务消息布局（99）
