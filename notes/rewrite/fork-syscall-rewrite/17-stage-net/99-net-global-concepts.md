# 99-net-global-concepts — 网络子系统全局概念

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- SDEV 常量全集：`SDEV_RQ_BASE 0x1900`（17 请求，com.h:1037-1061）+ `SDEV_RS_BASE 0x1980`（6 回复，com.h:1063-1068）+ `SDEV_OP_RD/WR/ERR/NOTIFY` + `SDEV_NONBLOCK/NOFLAGS`
- 消息布局：`mess_vfs_lsockdriver_*` 6 布局（ipc.h：addr/getset/ioctl/select/sendrecv/simple）
- sockid 命名空间：`SOCKID_TCP 0x0/UDP 0x00100000/RAW 0x00200000/RT 0x00400000/LNK 0x00800000`（lwip.h），libsockevent hash（id + id>>16）% 256
- endpoint 约定：VFS/MIB/DS/CLOCK/网卡驱动（NDEV）与 socket driver 的消息来源与身份
- 边界清单：与 05-stage-vfs（SDEV 客户端）/10-stage-mib（rmib 服务端）/16-stage-drivers（NDEV 驱动面 + chardriver）/18-stage-commands（网络工具）的完整边界（plan §5.3）
- errno 映射：`util_convert_err` 的 lwIP ERR_* ↔ errno 双射表（ARCH N-14）
- 网络常量值归属：`sys/socket.h`/`netinet/in.h`/`sys/un.h`/`net/bpf.h` 等常量值（与 14-stage-runtime/13 分工）
- Rust: `minix-types`

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制（00~24）。
