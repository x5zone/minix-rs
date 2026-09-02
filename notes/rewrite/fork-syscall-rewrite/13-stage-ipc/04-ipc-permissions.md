# 04-ipc-permissions: SysV 权限模型

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（05~08 各 handler 的前置原语）
> **源码**: `minix3/minix/servers/ipc/utility.c`（49 行）
> **Rust 模块**: `perms.rs`
> **draft 素材**: 无（新建）

## 核心点

- `check_perm(req, who, mode)` 全语义：uid=0 root 绕过；同 uid → `mode & 0700`；同 gid → `0070`（mode>>=3）；其他 → `0007`（mode>>=6）；返回 `(mode && ((mode & req_mode) == mode))`
- `getnuid`/`getngid` 依赖（minix-sys，A-5）
- 掩码调用点矩阵：semctl（SETVAL/SETALL→IPC_W、IPC_SET/IPC_RMID→owner EPERM、其余→IPC_R）、semop（全 0 op → IPC_R 否则 IPC_W）、shmget（flag 原样）、shmat（SHM_RDONLY→IPC_R 否则 IPC_R|IPC_W）、shmctl（IPC_STAT/SHM_STAT→IPC_R、IPC_SET/IPC_RMID→owner EPERM）
- `prepare_mib_perm`：`ipc_perm` → `ipc_perm_sysctl` 字段拷贝（_key/uid/gid/cuid/cgid/mode/_seq，MIB 输出用）

## 边界

- **前置依赖**: 02（IPC_R/W 常量）
- **不覆盖（移交）**: 消息字段（02）、handler 流程（05~08）
