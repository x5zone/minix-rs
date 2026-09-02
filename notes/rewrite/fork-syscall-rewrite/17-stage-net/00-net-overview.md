# 00-net-overview: 网络子系统整体概览

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/net/`（lwip 27 .c + uds 3 .c）+ `minix3/minix/lib/libsockdriver/` + `libsockevent/` + `liblwip/` + `minix3/minix/lib/libc/sys/`（socket 封装 15 文件）
> **Rust 模块**: `os/net/lwip`、`os/net/uds`、`os/libs/minix-netdriver`

## 核心点

- 网络子系统是什么：2 个 server（lwip TCP/IP sockets driver + uds UNIX domain sockets driver）+ 3 个共享框架库（libsockdriver/libsockevent/liblwip）+ 1 个用户态 ABI 面（libc socket 封装），不是单个 server（plan §1.1）
- 双 server 启动图：RS 运行时加载（`etc/usr/rc:259` `up lwip`、`:286` `up uds`）→ lwip `init()` 17 步启动链 + 主循环四路分发 → uds `uds_init()` 4 步 + 主循环二分分发（plan §1.2）
- 与相邻 stage 的边界：VFS 客户端（05-stage-vfs 22-sdev/24-socket）、NDEV 驱动面（16-stage-drivers 03/22/23）、MIB 服务端（10-stage-mib）、命令层（18-stage-commands）（plan §5.3）
- 文档导航：8 阶段 26 篇，新编号交叉引用规则（plan §2）
- 设计原则：位置可回答性（每篇回答"位于哪个 server 的启动链/主循环哪一步或框架哪一层"）/ 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH 全景：14 项设计期候选（plan §4），写文档时逐项确认

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~24、99）
