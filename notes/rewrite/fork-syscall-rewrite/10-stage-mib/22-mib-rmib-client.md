# 22-mib-rmib-client: RMIB 客户端契约（libsys）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 11 客户端契约（远程服务侧）
> **源码**: `minix/lib/libsys/rmib.c`（1089 行）+ `minix/include/minix/rmib.h`
> **Rust 模块**: `minix-sys`（**RMIB 客户端模块，A-8**）
> **draft 素材**: 无（新建）

## 核心点

- rmib 节点/树：`rmib_node`（flags/size/data/func/verify/子数组）+ **稀疏节点**（协议 id 子树，net.inet/inet6 用）
- 注册/注销/重注册：`rmib_register`（`MIB_REGISTER` asynsend3 单向 + `rmib_init` 子树递归）、`rmib_deregister`、`rmib_reregister`（MIB 重启后）、`rmib_reset`（测试）
- 请求处理：`rmib_process`（仅 MIB_PROC_NR 来源）+ `rmib_info`（COMMON_MIB_INFO name/desc grants）+ `rmib_call`（COMMON_MIB_CALL：rmib 解析 + 读写/函数分派 + 稀疏 id）
- 拷贝面：`rmib_copyout`/`rmib_vcopyout`（sys_safecopyto/sys_vsafecopy 到 MIB grant）、`rmib_copyin`/`rmib_copyin_str`、`RMIB_STACKBUF=257` 写缓冲
- 权限：`rmib_authed`（MIB 传 user_endpt + flags 布尔）
- 消费方：IPC（`kern.ipc`）、LWIP（`net.inet`/`net.inet6`/`minix.lwip`）、UDS（`net.local`）

## 边界

- **前置依赖**: 02 + 12
- **不覆盖（移交）**: MIB 服务器侧协议（12）
