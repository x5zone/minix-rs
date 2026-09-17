# 99-net-global-concepts — 网络子系统全局概念

> **状态**: 正文 v1 落稿（2026-09-17，架构扫描轮后与台账同批修订）
> **Rust 模块**: `os/libs/minix-netdriver/src/{sdev,sockid,sockevent,service}.rs`、`os/net/lwip/src/{util,lwip_port}.rs`、`os/libs/minix-sys/src/socket.rs`

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

---

## Ch1: 常量全集——四族一份契约

网络子系统的全部跨消息常量分四族。每一族的"权威源"在 C 里都是单一头文件；
Rust 侧按"数字归一处、语义归一层"落位，本篇给出索引，数值以各篇 §5 的锁值
测试为准。

### 1.1 套接字设备请求族（SDEV）

请求基址 `SDEV_RQ_BASE 0x1900` 带十七个请求，回复基址 `SDEV_RS_BASE 0x1980`
带六种回复，守卫宏 `IS_SDEV_RQ/IS_SDEV_RS` 靠"抹掉低七位等于基址"判定
（`com.h:1037-1078`）。传输旗标 `SDEV_NOFLAGS 0x00`/`SDEV_NONBLOCK 0x01`
（`com.h:1071-1072`）与选择位 `SDEV_OP_RD/WR/ERR/NOTIFY`（`com.h:1075-1078`）
同族。Rust 落点：`minix-netdriver/src/sdev.rs`（编号枚举、守卫、可挂起表、
旗标常量）。其中可挂起规则（八个能等、八个当场答，`sockdriver.c:8-26`）与
错误位永不重测的规则（`sockevent.c:817`）是本族最容易走样的两条，01/02 篇
各有对表。

### 1.2 套接字标识族（sockid）

`sockid_t` 是 `int32_t`，负数兼作错误通道（`sockdriver.h:27-28`）；五类基值
`SOCKID_TCP 0x0`、`SOCKID_UDP 0x00100000`、`SOCKID_RAW 0x00200000`、
`SOCKID_RT 0x00400000`、`SOCKID_LNK 0x00800000`（`lwip.h:58-62`）按"基址或
数组下标"铸成标识（`tcpsock.c:140` 等五处）；UNIX 域服务铸的是裸下标
（`uds.c:97-101`），数值上落在 TCP 区间——解释权归铸造方，VFS 视其为不透明。
哈希规则 `(id + (id >> 16)) % 256`（`sockevent.c:48-57`）消费裸编号。
Rust 落点：`minix-netdriver/src/sockid.rs`（新类型、二十位下标字段、负数
拒绝）与 `sockevent.rs::hash_slot`。

### 1.3 网卡设备族（NDEV）

请求基址 `NDEV_RQ_BASE 0x1A00`、回复基址 `NDEV_RS_BASE 0x1A80`
（`com.h:1085-1144`），模式/能力/旗标/组播回退各位一义。Rust 落点：
`minix-netdriver/src/protocol.rs`（枚举、守卫、队列界 8 与 2、组播回退规则）。
该族的单一来源化是 edge E-DEVWIRE 的标的——设备族五套常量终归一处，net 侧
随裁决迁移。

### 1.4 错误双射族（栈错误 ↔ errno）

lwIP 的 `err_t` 负值与 Minix errno 的双射由 `util_convert_err` 承担
（`util.c` 转换函数；Rust `os/net/lwip/src/util.rs` 的 17 臂 `STACK_* →
ERR_*` 表）。两条铁律：表外的栈值落通用错误，绝不静默穿透；`ERR_INPROGRESS`
与 `ERR_WOULDBLOCK` 在 C 源里就标注"不该被抛出"，映射保留只为不漏值。

### 1.5 第三方栈胶水契约

`PBUF_POOL_SIZE 0`（`lwipopts.h:80`）意味着池归服务自管——切片 512 字节
（`lwipopts.h:49`）、slab 增长至六十四块（`mempool.c:238`，约十七 MB）；
接收窗口 16384（`lwipopts.h:267`）与发送缓冲 11 倍分段（`:282`）是吞吐契约；
单线程无系统层（`NO_SYS 1`，`lwipopts.h:14`）是执行模型契约。这组数字在
Rust 侧由 `lwip_port.rs` 的常量映射承载，任何一项变动都是行为变更，须走
[ARCH] 三处一致。替代栈的裁决与偏差表见 24 篇 §1.5。

## Ch2: endpoint 约定——谁发来什么走哪条路

两个服务的消息面由来源端点定界，分类顺序与 C 主循环同序
（`lwip.c:293-382`；Rust `minix-netdriver/src/service.rs::classify`）：
时钟通知驱动定时器（`Endpoint::CLOCK`）；数据存储通知携带网卡上下线
（`Endpoint::DS`）；管理信息库请求来自 MIB 服务（`Endpoint::MIB`）；虚拟文件
系统（`Endpoint::VFS`）送来套接字设备请求与过滤器设备请求；网卡驱动的回复
与状态走回复基址。SEF 拦截在这一切之前：RS 的 ping 当场回 pong，系统服务的
信号请求上抛，普通消息才进分派（`minix-sef` 的 `sef_receive_status`）。

## Ch3: 边界清单——网络与四个邻 stage

- **05-stage-vfs**：SDEV 协议的客户端（`../05-stage-vfs/22-sdev.md`）与
  socket 调用的转发面（`../05-stage-vfs/24-socket.md`）；服务侧在本阶段
  01/02 篇。线上一份协议两处实现的收敛标的在 edge E-SDEVOWN。
- **10-stage-mib**：`net.*` 子树的注册与查询转发（rmib），解锁条款在
  edge E-RMIBWIRE。
- **16-stage-drivers**：NDEV 驱动面与字符/块框架（`../16-stage-drivers/03-netdriver-framework.md`）；
  设备族常量单一来源在 edge E-DEVWIRE；NIC 实装归其 todo。
- **18-stage-commands**：netconfig/netservices 等工具消费本阶段服务
  （`../18-stage-commands/NN-*.md`）。

## Ch4: ARCH 全景对照

十四项 ARCH 候选的现状表在 `plan.md` §4（本篇不复制，避免两处真相）。
当前已裁决三项：N-1 栈替代（smoltcp 一族加语义垫片，墙后可回退）、N-2 旧式
回退丢弃（审查锚点即 `socket.rs` 的条件函数与 23 篇 §3）、N-6 sockid 类型化
（`sockid.rs`）。其余各项随对应篇目落档。

## Ch5: 常量的锁值测试分布

全局常量的可执行形态分散在各模块、各篇 §5 各自登记：SDEV 编号与旗标在 01 篇
§5.1（六个测试）、sockid 在 01 篇 §5.2（五个）、事件与哈希在 02 篇 §5.1
（四个）、lwip 胶水契约在 24 篇 §5.1。复现入口统一为（工作目录 `os/`）：
`cargo test -p minix-netdriver -p minix-net-lwip`。

## Ch6: 过渡

全局概念至此收拢：四族常量、一个分类序、四条边界、一张 ARCH 表。反向阅读的
读者（从 99 进来）建议回到 00 篇按导航序重走一遍——常量只有回到产生它们的
机制里才有意义。
