# 03-ipc-mib-registration: kern.ipc 远程 MIB 子树

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（boot 期 `sef_cb_init_fresh` 注册，主循环 MIB 消息处理）
> **源码**: `minix3/minix/servers/ipc/main.c:27-47,54-79` + `minix3/minix/lib/libsys/rmib.c`（外部客户端）
> **Rust 模块**: `mib_client`（A-6，归属 minix-sys 或复用 `../10-stage-mib/22-mib-rmib-client.md` 契约）
> **draft 素材**: 无（新建）

## 核心点

- `sef_cb_init_fresh` → `rmib_register({CTL_KERN, KERN_SYSVIPC}, 2, &kern_ipc_node)`（本地失败 panic；远端失败静默）
- `kern_ipc_table`：`KERN_SYSVIPC_INFO`（函数节点）、`MSG=0`（无消息队列）、`SEM=1`、`SHM=1` + 5 个 "not yet supported" 槽位（SHMMAX/SHMMNI/SHMSEG/SHMMAXPGS/SHMUSEPHYS，A-8 排除）
- `kern_ipc_info`：`call_namelen==1` 校验、**无权限要求**（NetBSD sysvipc_info 语义）、`SEM_INFO=5` → `get_sem_mib_info`（05）、`SHM_INFO=6` → `get_shm_mib_info`（08）、default `EOPNOTSUPP`
- RMIB 客户端契约：`rmib_register`/`rmib_deregister`/`rmib_process`（A-6；主循环 MIB 消息 → `rmib_process`）
- `init_restart` = `init_fresh`（重启重新注册子树）
- `KERN_SYSVIPC=82`、INFO=1/2/3/4、SEM_INFO=5/SHM_INFO=6（sysctl.h:275,684-705）

## 边界

- **前置依赖**: 02 + MIB `22-mib-rmib-client`
- **不覆盖（移交）**: sysvipc_info 具体实现（05/08）、MIB 服务内部（10-stage-mib）
