# 17-stage-net 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/17-stage-net/`
> **目标**: 以 **双 server 启动顺序为主线**（lwip → uds）+ **网络子系统语义分层**重组 net 全部文档；SDEV（VFS↔socket driver）请求协议为次主线；最终覆盖 Minix3 网络子系统全部语义（libsockdriver + libsockevent + liblwip + lwip server + uds server + libc socket 封装），支撑 `os/net/lwip` + `os/net/uds` + `os/libs/minix-netdriver` 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md` + `14-stage-runtime/plan.md` + `15-stage-fs/plan.md` + `16-stage-drivers/plan.md`（plan 结构参照；15/16 为多 server 主线重定义先例）、`minix3/minix/net/` + `minix3/minix/lib/libsockdriver/` + `minix3/minix/lib/libsockevent/` + `minix3/minix/lib/liblwip/` + `minix3/minix/lib/libc/sys/`（socket 封装）（ground truth）、`os/net/*` + `os/libs/minix-netdriver/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 与 VM 的差异：net 不是单个 server，而是双 server + 三层共享框架

`02-stage-vm` 以 **VM server 启动顺序为主线**——VM 是一个有明确 `init_vm()` 启动链 + 主循环的用户态服务。**17-stage-net 不是单个 server**，它覆盖 2 个 server（lwip + uds）+ 3 个共享框架库 + 1 个用户态 ABI 面：

| 层 | 内容 | C 源码 | 行数 | 性质 |
|----|------|--------|------|------|
| 框架 | libsockdriver | `minix/lib/libsockdriver/sockdriver.c` + `include/minix/sockdriver.h` | 1150 | socket driver 框架：`sockdriver_task/process` 主循环、SDEV 请求/回复、`sdr_*` 回调表（19 项）、safecopy 拷贝辅助 |
| 框架 | libsockevent | `minix/lib/libsockevent/`（sockevent.c 2590 + sockevent_proc.c 52）+ `include/minix/sockevent.h` | 2642 | socket 事件分发：`struct sock` 对象/hash/定时器、`sockevent_ops`、悬挂调用续作、select、错误/关闭传播 |
| 框架 | liblwip | `minix/lib/liblwip/`（第三方 lwIP：`dist/src` 编译子集 68 .c / 58232 行 + `lib/` 胶水 lwipopts.h/lwiphooks.h/arch/cc.h + 4 个 patches） | ~58k | TCP/IP 协议栈本体（第三方导入，[ARCH] N-1） |
| server | lwip | `minix/net/lwip/`（27 个 .c） | 24477 | TCP/IP sockets driver：SEF 启动 + 主循环 + SDEV/sockevent 分发 + NDEV 消费侧 + /dev/bpf + 路由/MIB |
| server | uds | `minix/net/uds/`（uds.c 1417 / io.c 1803 / stat.c 186） | 3406 | UNIX domain sockets driver：SEF 启动 + 主循环 + sockevent + 文件描述符传递 |
| ABI | libc socket | `minix/lib/libc/sys/`（socket.c 等 15 文件） | 3173 | 用户态 socket 系统调用封装（现代 VFS_SOCKET 路径 + 旧式设备 fallback，[ARCH] N-2） |

两个 server 的定位差异：

- **lwip 与 VM 同构**：`lwip.c:270 startup()`（`sef_setcb_init_fresh(init)` + `sef_startup()` 在 :287）→ `lwip.c:294 main()` → `while(running) { ifdev_poll(); check_lwip_timer(); sef_receive_status(ANY); 分发 }`。这是**严格线性的启动链 + 主循环**，与 `02-stage-vm` 的 `init_vm()` + 主循环结构一致。
- **uds 同构但更简单**：`uds.c:1367 uds_startup()`（`sef_setcb_init_fresh(uds_init)` + signal handler → `sef_startup()`）→ `uds.c:1384 main()` → `while(uds_running || uds_in_use > 0) { sef_receive_status(ANY); MIB→rmib_process / 其他→sockevent_process }`。
- **两者共用 sockevent/sockdriver 框架**：框架语义必须先于两个 server 讲述。

### 1.2 新主线：双 server 启动顺序 + 子系统语义分层

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。net 没有单一启动链，但有清晰的 **RS 运行时加载顺序 + 语义依赖顺序**：

```
RS 运行时加载（非 boot_image；/etc/usr/rc:259 "up lwip"、:286 "up uds"）
  │
  ├─ lwip（lwip.c:270 startup → init(:196) → main 主循环）
  │    init()（lwip.c:196）——阶段 2~6 全部文档的锚点：
  │      srand48(clock_time) → lwip_init() → sockevent_init(alloc_socket)
  │      → mempool_init → tcpisn_init → mcast_init
  │      → ipsock_init → tcpsock_init → udpsock_init → rawsock_init
  │      → ifdev_init → loopif_init → ethif_init → ndev_init
  │      → rtsock_init → lnksock_init → route_init → bpfdev_init
  │      → mibtree_init → ifconf_init（默认 loopback）→ init_timer
  │    main() 主循环（lwip.c:294）：
  │      ifdev_poll → check_lwip_timer → sef_receive_status(ANY)
  │      ├─ notify：CLOCK → expire_timers；DS_PROC_NR → ndev_check
  │      ├─ MIB_PROC_NR → rmib_process
  │      ├─ VFS：IS_SDEV_RQ → sockevent_process；IS_CDEV_RQ/IS_BDEV_RQ → bpfdev_process
  │      └─ IS_NDEV_RS → ndev_process（网卡驱动响应）
  │
  └─ uds（uds.c:1367 uds_startup → uds_init(:1303) → main 主循环）
       uds_init()（uds.c:1303）：TAILQ 空闲队列 → udshash_init → uds_io_init（io.c:102）→ uds_stat_init（stat.c:163）
         → sockevent_init(uds_socket)（uds.c:1326）
       main() 主循环（uds.c:1384）：
         while(uds_running || uds_in_use > 0) → sef_receive_status
         ├─ MIB_PROC_NR → rmib_process
         └─ 其他 → sockevent_process
```

**每篇文档必须能回答一个问题：它位于哪个 server（lwip/uds）的启动链或主循环的哪个位置，或位于共享框架的哪一层。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则，由 `15-stage-fs/plan.md §1.1`（FS 多 server 主线重定义先例）和 `16-stage-drivers/plan.md §1.1`（drivers 多进程主线重定义先例）迁移而来。

### 1.3 旧内容的问题与归档

17-stage-net 原仅有占位 `README.md`（scope 定义：lwip/uds/minix-netdriver + "非目标可长期后置"），已移入 `draft/README.md`。无 checklist.md（本 stage 从未进入过 fork 主线写作阶段）。占位 README 的 scope 定义保留为素材，本 plan 将其扩展为完整语义覆盖契约。

### 1.4 SDEV 请求协议次主线

请求协议不充当概念引入的驱动，而是按**框架 → server** 展开（框架文档内部绘制协议布局），协议常量值集中在 99：

```
VFS（05-stage-vfs/22-sdev.md：sdev_socket/sdev_bind/... 客户端侧）
  │  SDEV_RQ_BASE 0x1900（com.h:1037）
  ├─ SDEV_SOCKET(0)/SOCKETPAIR(1)/BIND(2)/CONNECT(3)/LISTEN(4)/ACCEPT(5)
  │   SEND(6)/RECV(7)/IOCTL(8)/SETSOCKOPT(9)/GETSOCKOPT(10)
  │   GETSOCKNAME(11)/GETPEERNAME(12)/SHUTDOWN(13)/CLOSE(14)/CANCEL(15)/SELECT(16)
  ▼
socket driver（lwip / uds，libsockdriver 框架）
  │  SDEV_RS_BASE 0x1980（com.h:1063-1068）
  └─ SDEV_REPLY(0)/SOCKET_REPLY(1)/ACCEPT_REPLY(2)/RECV_REPLY(3)
     SELECT1_REPLY(4)/SELECT2_REPLY(5)
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。draft 素材保留原样。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | 变更 |
|------|------|------|---------|--------|-----------|------|
| 0 总览 | 00 | `00-net-overview.md` | 网络子系统是什么、双 server 启动图、与 VFS/MIB/驱动/命令的边界、文档导航 | 全部 | 全部 | 新建 |
| 1 框架 | 01 | `01-sockdriver-framework.md` | libsockdriver：sockdriver_task/process/terminate/announce、`sdr_*` 回调表（19 项）、SDEV 请求/回复消息布局（com.h 0x1900/0x1980 + ipc.h mess_vfs_lsockdriver_*）、copyin/out/vcopy、pack/unpack、悬挂调用标识 | `lib/libsockdriver/sockdriver.c`、`include/minix/sockdriver.h`、`include/minix/com.h`、`include/minix/ipc.h` | `os/libs/minix-netdriver`（SDEV 类型 + 框架 trait，新建） | 新建 |
| 1 框架 | 02 | `02-sockevent-framework.md` | libsockevent：`struct sock` 对象/hash（256 槽）/定时器、`sockevent_ops` 回调表、悬挂调用续作（sockevent_proc）、select 支持、错误/关闭/shutdown 传播、sockevent_process 分发 | `lib/libsockevent/`（sockevent.c + sockevent_proc.c）、`include/minix/sockevent.h` | 同上（minix-netdriver 或独立 minix-sockevent） | 新建 |
| 2 lwip 骨架 | 03 | `03-lwip-main-init.md` | lwip 服务骨架：main/startup/init 全启动链、主循环四路分发（notify/MIB/SDEV+CDEV-BDEV/NDEV_RS）、alloc_socket 域分发（PF_INET/INET6/ROUTE/LINK）、lwip_hook_rand、mibtree_init 注册时序 | `net/lwip/lwip.c` + `mibtree.c` | `os/net/lwip`（bin 入口 + 事件循环） | 新建 |
| 2 lwip 骨架 | 04 | `04-lwip-mempool.md` | mempool：pbuf 池（PBUF_RAM 链替代 PBUF_POOL 的定制池）、cur/max 统计、pchain 链工具 | `net/lwip/mempool.c` + `pchain.c` | `os/net/lwip`（pbuf 池模块） | 新建 |
| 2 lwip 骨架 | 05 | `05-lwip-util-addr.md` | 公共工具：util（时间换算/根权限/错误转换）、addr（sockaddr 解析校验、sin/sin6/sdlx 互转、SOCKADDR_MAX 断言）、addrpol（RFC 6724 地址选择策略） | `net/lwip/util.c` + `addr.c` + `addrpol.c` | `os/net/lwip`（工具模块） | 新建 |
| 3 lwip socket 族 | 06 | `06-lwip-ipsock.md` | IP socket 公共层：ipsock_socket/选项、src 地址选择（与 addrpol 联动）、hop limit/TOS、IPPROTO_IP/IPV6 选项、连接语义（IP 层） | `net/lwip/ipsock.c` | `os/net/lwip`（IP socket 基类） | 新建 |
| 3 | 07 | `07-lwip-pktsock.md` | 包 socket 共享层：pktsock_socket/input、snd/rcv buf 管理、IP 层输入分发（udp/raw 共用） | `net/lwip/pktsock.c` | `os/net/lwip`（包层模块） | 新建 |
| 3 | 08 | `08-lwip-tcpsock.md` | TCP socket：连接状态机、send/recv 队列、MSS/Nagle/delayed ACK、listen/accept、选项、错误传播、SIGPIPE、tcpisn（ISN 生成 + SHA256） | `net/lwip/tcpsock.c` + `tcpisn.c` | `os/net/lwip`（TCP 模块） | 新建 |
| 3 | 09 | `09-lwip-udpsock.md` | UDP socket：send/recv、多播 TTL/loop、校验和、connect、bind | `net/lwip/udpsock.c` | `os/net/lwip`（UDP 模块） | 新建 |
| 3 | 10 | `10-lwip-rawsock.md` | RAW socket：IPv4/IPv6 raw、ICMP、头部处理、根权限检查（util_is_root） | `net/lwip/rawsock.c` | `os/net/lwip`（RAW 模块） | 新建 |
| 3 | 11 | `11-lwip-lnksock.md` | AF_LINK socket + lldata 链路层数据：链路层路由表、IOCTL 支撑（ifconfig 需求） | `net/lwip/lnksock.c` + `lldata.c` | `os/net/lwip`（链路层模块） | 新建 |
| 3 | 12 | `12-lwip-mcast.md` | 组播：IGMP/MLD 成员管理、全局/每 socket 限制、与 lwIP 成员结构映射 | `net/lwip/mcast.c` | `os/net/lwip`（组播模块） | 新建 |
| 4 lwip 接口面 | 13 | `13-lwip-ndev.md` | NDEV 消费侧：ndev_init/check/process/conf/send/can_recv/recv、驱动 up/down 跟踪（DS notify）、NR_NDEV=8、与 16 的 NDEV 驱动面协议契约 | `net/lwip/ndev.c` + `ndev.h` | `os/net/lwip`（NDEV 客户端）+ `os/libs/minix-netdriver`（协议类型） | 新建 |
| 4 | 14 | `14-lwip-ifdev.md` | 接口对象：ifdev 结构/ifdev_ops 回调表（16 项）、硬件地址列表（IFDEV_NUM_HWADDRS=3）、loopif 环回接口 | `net/lwip/ifdev.c` + `loopif.c` | `os/net/lwip`（接口层） | 新建 |
| 4 | 15 | `15-lwip-ethif.md` | 以太网接口实例：ethif 初始化/收发队列/配置请求、与 ndev 驱动绑定 | `net/lwip/ethif.c` | `os/net/lwip`（以太网接口） | 新建 |
| 4 | 16 | `16-lwip-ifaddr.md` | 接口地址管理：IPv4/IPv6 地址列表、硬件地址活动切换、ifdev 的地址字段专用访问 | `net/lwip/ifaddr.c` | `os/net/lwip`（地址模块） | 新建 |
| 4 | 17 | `17-lwip-ifconf.md` | 接口配置：默认配置（loopback）、SIOC 处理、minix/if.h 扩展（MINIX_SIOCGIFMEDIA/IFGCLONERS）、链路状态 | `net/lwip/ifconf.c` + `include/minix/if.h` | `os/net/lwip`（配置模块） | 新建 |
| 4 | 18 | `18-lwip-bpfdev.md` | /dev/bpf：bpfdev 设备（chardriver 消费侧 + select）、bpf_filter（NetBSD 移植 561 行）、包缓冲（BSD 模型） | `net/lwip/bpfdev.c` + `bpf_filter.c` | `os/net/lwip`（BPF 模块） | 新建 |
| 5 lwip 路由 | 19 | `19-lwip-route.md` | 路由：rttree（radix 树，前缀长度假设）、route 覆盖 lwIP ip4/ip6_route + 网关 hook（lwiphooks）、路由条目管理 | `net/lwip/rttree.c` + `route.c` | `os/net/lwip`（路由模块） | 新建 |
| 5 | 20 | `20-lwip-rtsock.md` | 路由 socket（PF_ROUTE）：rt_msghdr 解析、RTA 数组压缩/展开、消息路由（net.inet/route 树） | `net/lwip/rtsock.c` | `os/net/lwip`（路由 socket 模块） | 新建 |
| 6 uds server | 21 | `21-uds-core.md` | uds 服务核心：对象模型（NR_UDSSOCK=256）、连接状态机（5 状态 + limbo）、udshash（64 槽）、listen/connect/accept 语义、LOCAL_CONNWAIT、sockevent_ops、MIB 状态面（stat.c：net.local.*） | `net/uds/uds.c` + `stat.c` | `os/net/uds`（服务核心） | 新建 |
| 6 | 22 | `22-uds-io.md` | uds 数据面：recv 缓冲段（UDS_BUF=32768）、ancillary data（FD 传递/socketpath/copyfd/凭据，UDS_CTL_MAX=4096）、SOCK_STREAM/SEQPACKET/DGRAM、读写/悬挂续作 | `net/uds/io.c` | `os/net/uds`（I/O 模块） | 新建 |
| 7 ABI | 23 | `23-libc-socket.md` | 用户态 socket 封装：socket/bind/connect/listen/accept/sendto/recvfrom/sendmsg/recvmsg/setsockopt/getsockopt/getsockname/getpeername/shutdown/socketpair、VFS_SOCKET 主路径 + 旧式设备 fallback（[ARCH] N-2） | `minix/lib/libc/sys/`（socket.c 等 15 文件） | `os/libs/minix-sys`（vfs socket 模块） | 新建 |
| 8 第三方栈 | 24 | `24-liblwip-port.md` | liblwip：lwIP 编译子集（core/ipv4/ipv6/netif 68 .c）、lwipopts.h 配置面（NO_SYS/PBUF_POOL_SIZE=0/TCP_* 缓冲）、lwiphooks.h（ISN/路由/gateway 4 hook）、arch/cc.h、4 个 patches、Rust 栈替代决策（[ARCH] N-1） | `lib/liblwip/`（dist/src + lib/ + patches/） | 外部 Rust 栈（smoltcp 或自研） | 新建 |
| 99 全局概念 | 99 | `99-net-global-concepts.md` | SDEV 常量全集（com.h 0x1900/0x1980 + SDEV_OP_* + ipc.h 消息布局）、sockid 命名空间（SOCKID_TCP/UDP/RAW/RT/LNK）、endpoint 约定、与 05/10/16/18 的边界清单 | `com.h` + `ipc.h` + `sockdriver.h` + `sockevent.h` | `minix-types` | 新建 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在子系统中的位置与下一阶段的入口：

```
00（总览）→ 01/02（框架：双 server 共用骨架）
→ 03（lwip 启动链锚点）→ 04/05（资源/工具）
→ 06~12（socket 协议族：ipsock → pktsock → tcp/udp/raw → link/mcast）
→ 13~18（接口与驱动面：ndev → ifdev/ethif → 地址/配置 → bpf）
→ 19/20（路由：rttree/route → rtsock）
→ 21/22（uds：核心 → 数据面）
→ 23（用户态 ABI）→ 24（第三方栈 ARCH）→ 99（全局概念）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel / 02-stage-vm plan §3）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`/`02-stage-vm` 一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的对应、`[ARCH]` 标注
4. **错误处理**——errno 映射（P0：错误类型必须映射 Minix3 errno 值）
5. **测试**——该文档语义模块的 Rust 单测清单与统计
6. **过渡**——本阶段在子系统中的位置 + 下一阶段入口
7. **参见**——绝对路径引用（doc/code/C 源），绝不引用 `.design/`/`tmp_design_and_todo/`

### 3.2 引用规则

- 各文档之间用新编号交叉引用（如 `08-lwip-tcpsock.md` §连接状态机）
- 与其他 stage 交叉引用：`../01-stage-kernel/NN-*.md`、`../02-stage-vm/NN-*.md`、`../05-stage-vfs/22-sdev.md`、`../05-stage-vfs/24-socket.md`、`../10-stage-mib/NN-*.md`、`../14-stage-runtime/NN-*.md`、`../16-stage-drivers/03-netdriver-framework.md`、`../16-stage-drivers/22-net-driver-reference.md`、`../18-stage-commands/NN-*.md`
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"

### 3.3 每篇文档的边界声明

每篇必须含"前置依赖 / 本篇不覆盖什么"声明，写作时禁止内容交叉。关键边界：

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 01 | 00、`../05-stage-vfs/22-sdev.md`（客户端协议面） | libsockdriver 框架、SDEV 消息布局 | sock 对象/续作语义（02）、协议常量值（99） |
| 02 | 01 | sockevent 分发、悬挂/续作、select | SDEV 消息编码（01）、各协议族实现（06~12、21/22） |
| 03 | 01/02 | lwip 启动链、主循环分发、alloc_socket | 各模块内部实现（04~20）、MIB 服务端（10-stage-mib） |
| 04 | 03 | pbuf 池、链工具 | lwIP 内部 pbuf 语义（24） |
| 05 | 03 | 时间/权限/错误工具、sockaddr 解析、地址策略 | 各 socket 模块如何使用（06~12） |
| 06 | 05、07（pktsock 共享层） | IP socket 公共层 | TCP/UDP/RAW 具体语义（08/09/10） |
| 07 | 06 | 包层 snd/rcv 管理、输入分发 | IP 选项语义（06）、具体协议（08/09/10） |
| 08 | 06、07 | TCP 全量 + ISN | lwIP TCP 内部（24）、SIGPIPE 机制（14-stage-runtime/08） |
| 09 | 06、07 | UDP 全量 | 组播成员管理（12）、pktsock 层（07） |
| 10 | 06、07 | RAW 全量 + 权限 | ICMP 语义细节（24） |
| 11 | 06 | AF_LINK socket + 链路层路由 | 以太网接口（15）、ifaddr（16） |
| 12 | 06 | 组播成员管理 | IGMP/MLD 协议实现（24）、具体 socket 使用（08/09） |
| 13 | 03、`../16-stage-drivers/03-netdriver-framework.md` | NDEV 消费侧 | NDEV 驱动面协议（16）、驱动实现（16 的 22/23） |
| 14 | 13 | 接口对象模型 + loopif | 以太网实现（15）、地址管理（16）、配置（17） |
| 15 | 13/14 | ethif 以太网实例 | ifdev 通用层（14） |
| 16 | 14/15 | 地址列表管理 | ioctl 配置面（17） |
| 17 | 16 | 接口配置 ioctl、默认配置 | 地址语义（16） |
| 18 | 02（select）、`../16-stage-drivers/01-chardriver-framework.md` | /dev/bpf + 过滤器 | 网卡驱动（16）、BPF 用户态语义（18-stage-commands） |
| 19 | 03、24（lwIP hook） | 路由表 + 覆盖 | 路由 socket 消息（20） |
| 20 | 19 | rt_msghdr/RTA 格式 | 路由表内部（19） |
| 21 | 02、03 | uds 核心 + MIB 状态面 | 数据面（22）、sockevent 框架（02） |
| 22 | 21 | recv 缓冲/段/ancillary/FD 传递 | uds 连接状态机（21）、VFS copyfd 客户端（05-stage-vfs） |
| 23 | 05（`_syscall`）、`../05-stage-vfs/24-socket.md` | libc socket 封装 | VFS 服务端（05）、socket driver 服务端（本 stage 03/21） |
| 24 | 03（lwip_init 调用面） | lwIP 配置面/胶水/替代决策 | Minix 侧各模块（03~20） |
| 99 | 无 | 全局常量/命名空间/边界 | 一切机制（00~24） |

### 3.4 测试基线（截至 2026-08-16）

- `os/net/lwip`、`os/net/uds`、`os/libs/minix-netdriver` 均为占位 stub，`cargo test` 无实质测试
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.5 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/net/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 17-stage-net 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| N-1 | **lwIP 第三方栈** | liblwip：lwIP 2.x 导入（`dist/src` 编译子集 68 .c / 58232 行 + 4 patches），Minix 胶水 lwipopts.h/lwiphooks.h/arch/cc.h；lwip 服务链接 `-llwip`（`net/lwip/Makefile`） | Rust 栈替代（候选：smoltcp / 自研精简栈 / FFI 保留 C）；首版可先 FFI 后逐层替换；lwipopts 配置面（NO_SYS=1/PBUF_POOL_SIZE=0/TCP_SND_BUF=11*MSS 等）映射为 Rust 常量 | 24（决策）+ 各模块（调用面） | 设计决策（占位 README 已声明"非目标可长期后置"） |
| N-2 | **libc socket 旧式 fallback** | `libc/sys/socket.c` 等：先 `_syscall(VFS_PROC_NR, VFS_SOCKET, ...)`，EAFNOSUPPORT/ENOSYS 时回退 `open(TCP_DEVICE/UDP_DEVICE/IP_DEVICE/UDS_DEVICE)` + `net/gen/*` 旧设备协议（NWIOSIPOPT 等） | 丢弃 fallback（minix-rs VFS SDEV 路径唯一）；14 的 libc 常量面保留现代 socket 常量（AF_*/SOCK_*/SO_*） | 23 | 设计决策（WONTFIX fallback） |
| N-3 | **BPF 过滤器** | `bpf_filter.c`：NetBSD bpf_filter 用户态移植（mbuf→pbuf、无 BPF context）561 行 | Rust 重写 bpf 解释器（或最小子集：tcpdump 常用指令）；`/dev/bpf` 语义保留 | 18 | 待设计 |
| N-4 | **NDEV 消费侧抽象** | `ndev.c`：直接发 NDEV_RQ（com.h 0x1A00）到网卡驱动、DS notify 跟踪 up/down | `minix-netdriver` crate 提供协议类型 + trait；lwip 侧为纯消费方（与 16 框架对称） | 13 | 待设计 |
| N-5 | **IPv6 支持** | `USE_INET6` 条件编译（`net/lwip/Makefile`），PF_INET6 与 PF_INET 同路径 | 首版决策：Rust 栈 IPv6 支持面（smoltcp 支持）；`MINIX_SIOCGIFMEDIA` 等 ioctl 的 IPv6 分支 | 06/09/10/24 | 待设计 |
| N-6 | **sockid 命名空间** | `SOCKID_TCP 0x0 / UDP 0x00100000 / RAW 0x00200000 / RT 0x00400000 / LNK 0x00800000`（lwip.h）+ libsockevent hash 槽（id + id>>16）% 256 | Rust 类型化 SocketId（协议族标记 + 序号），hash 语义保留 | 02/99 | 待设计 |
| N-7 | **远程 MIB（rmib）** | `rmib_process`/`rmib_register`：socket driver 注册 `net.*` 子树，MIB server（10-stage-mib）转发查询 | trait 化 rmib 树（与 10-stage-mib Rust 侧对称）；`net.inet.tcp.isn_secret` 等可写节点 | 03/20/21/23 | 待设计 |
| N-8 | **sockaddr 类型安全** | `SOCKADDR_MAX 256` + `STATIC_SOCKADDR_MAX_ASSERT` + `union sockaddr_any`（sa/sin/sin6/sdlx）；`sockaddr_dlx` 自定最大尺寸版 | Rust `SockAddr` enum（Ipv4/Ipv6/Link/Unix）+ 尺寸不变式（类型系统表达 SOCKADDR_MAX） | 01/05/99 | 待设计 |
| N-9 | **/dev/bpf 字符设备面** | `bpfdev.c`：libchardriver 回调表 + `chardriver_task`（CDEV_CLONED/select/reply） | 消费 `minix-chardriver`（16 框架）Rust 版；BPF 设备语义独立于网络协议栈 | 18 | 待设计 |
| N-10 | **UDS 文件描述符传递** | `io.c`：in-flight FD（`struct uds_fd`）、socketpath(2)/copyfd(2)、uid 0 要求（`uds.conf`）、凭据段 | Rust 安全建模（FD 借用/所有权、拒绝指针逃逸）；socketpath/copyfd 客户端在 05-stage-vfs | 21/22 | 待设计 |
| N-11 | **lwIP 定时器集成** | `lwip.c`：`set_timer(&lwip_timer)` + `sys_check_timeouts` + CLOCK notify `expire_timers`；`sys_now()` 用 getticks*1000/sys_hz | 事件循环 Timer trait（对齐 01-stage-kernel 时钟语义）；Rust 栈时间源注入 | 03/24 | 待设计 |
| N-12 | **服务重启语义** | lwip：stateless restart（不设 `_restart` callback，`lwip.c:275-278` 注释）；uds：SIGTERM 后 `uds_running=FALSE` + 等 socket 关闭（`uds.c:1351-1365`） | Rust SEF 生命周期（03-stage-rs）对接；两 server 重启策略保留 | 03/21 | 待设计 |
| N-13 | **IPv6/组播策略** | `LWIP_IGMP=1` + `mcast.c` 全局成员上限（NR_IPV4_MCAST_GROUP 64 + NR_IPV6_MCAST_GROUP 64 = 128，lwipopts.h）+ per-socket 上限（MAX_GROUPS_PER_SOCKET 8）；`LWIP_MULTICAST_TX_OPTIONS` | 组播成员管理类型化；上限常量保留 | 12/24 | 待设计 |
| N-14 | **错误码映射** | `util_convert_err(err)`：lwIP err_t（ERR_*）→ errno（lwip.h util） | `Errno`（minix-types）与 lwIP err 双射表，P0 约束 | 05/99 | 待设计 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

**`minix/net/lwip/`（27 个 .c / 24477 行）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `lwip.c` | 382 | 03 | 已核对（init 链/main/分发/alloc_socket） |
| `mempool.c` | 821 | 04 | 已核对（PBUF_RAM 定制池） |
| `pchain.c` | 154 | 04 | 已核对（pbuf 链工具） |
| `util.c` | 251 | 05 | 已核对（timeval→ticks、util_is_root、util_convert_err） |
| `addr.c` | 699 | 05 | 已核对（sockaddr 解析校验/转换） |
| `addrpol.c` | 143 | 05 | 已核对（RFC 6724 策略表） |
| `ipsock.c` | 761 | 06 | 已核对（ipsock_socket/src addr/选项） |
| `pktsock.c` | 1236 | 07 | 已核对（pktsock_socket/input、snd/rcv buf） |
| `tcpsock.c` | 2793 | 08 | 已核对（TCP 全量） |
| `tcpisn.c` | 203 | 08 | 已核对（ISN + SHA256 + isn_secret sysctl） |
| `udpsock.c` | 997 | 09 | 已核对（UDP + pktsock 复用） |
| `rawsock.c` | 1341 | 10 | 已核对（raw + pktsock 复用 + 权限） |
| `lnksock.c` | 77 | 11 | 已核对（AF_LINK socket 支撑 IOCTL） |
| `lldata.c` | 584 | 11 | 已核对（链路层路由数据） |
| `mcast.c` | 283 | 12 | 已核对（IGMP/MLD 成员管理） |
| `ndev.c` | 1019 | 13 | 已核对（NDEV 消费侧） |
| `ifdev.c` | 1064 | 14 | 已核对（ifdev 对象 + ifdev_ops） |
| `loopif.c` | 420 | 14 | 已核对（环回接口） |
| `ethif.c` | 1718 | 15 | 已核对（以太网实例 + 发送队列） |
| `ifaddr.c` | 2224 | 16 | 已核对（地址列表 + 硬件地址切换） |
| `ifconf.c` | 930 | 17 | 已核对（默认配置 + ioctl） |
| `bpfdev.c` | 1365 | 18 | 已核对（/dev/bpf + chardriver + select） |
| `bpf_filter.c` | 561 | 18 | 已核对（NetBSD bpf 移植） |
| `rttree.c` | 744 | 19 | 已核对（radix 树） |
| `route.c` | 1654 | 19 | 已核对（覆盖 lwIP 路由 + gateway hook） |
| `rtsock.c` | 1912 | 20 | 已核对（rt_msghdr/RTA） |
| `mibtree.c` | 141 | 03 | 已核对（net.* 子树注册） |

**`minix/net/uds/`（3 个 .c / 3406 行）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `uds.c` | 1417 | 21 | 已核对（对象/状态机/hash/SEF 生命周期） |
| `io.c` | 1803 | 22 | 已核对（数据面/段/ancillary/FD 传递） |
| `stat.c` | 186 | 21 | 已核对（net.local.* MIB + kinfo_pcb） |

**框架库**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `lib/libsockdriver/sockdriver.c` | 1150 | 01 | 已核对（18 项 sdr_* + 7 个回复/拷贝辅助函数族） |
| `lib/libsockevent/sockevent.c` + `sockevent_proc.c` | 2590 + 52 | 02 | 已核对（sock 对象/ops/续作/select） |
| `lib/liblwip/dist/src/`（core/ipv4/ipv6/netif 编译子集） | 68 .c / 58232 | 24 | 已核对（Makefile.inc 子集 + patches） |
| `lib/liblwip/lib/`（lwipopts.h/lwiphooks.h/arch/cc.h） | — | 24 | 已核对 |

**`minix/lib/libc/sys/`（socket 封装，15 文件 / 3173 行）**

| C 文件 | 新文档 | 核对 |
|--------|--------|------|
| `socket.c`（247）/`socketpair.c`（159） | 23 | 已核对（VFS_SOCKET 主路径 + _uds/_tcp/_udp/_raw fallback） |
| `bind.c`（231）/`connect.c`（195）/`listen.c`（51）/`accept.c`（162） | 23 | 已核对（legacy fallback 存在，见 §4 N-2） |
| `sendto.c`（299）/`recvfrom.c`（369）/`sendmsg.c`（204）/`recvmsg.c`（204） | 23 | 已核对 |
| `setsockopt.c`（296）/`getsockopt.c`（290）/`getsockname.c`（192）/`getpeername.c`（181）/`shutdown.c`（93） | 23 | 已核对（getpeername 与 getsockname 同族，grep 实证） |

**配置与启动脚本（语义契约：服务注册/启动顺序）**

| 文件 | 新文档 | 核对 |
|------|--------|------|
| `net/lwip/lwip.conf`（domain INET/INET6/ROUTE/LINK、ipc SYSTEM/vfs/rs/vm/mib、system KILL） | 03/99 | 已核对 |
| `net/uds/uds.conf`（domain LOCAL、uid 0、ipc SYSTEM/vfs/rs/vm/mib） | 21/99 | 已核对 |
| `etc/usr/rc`（:259 `up lwip -dev /dev/bpf -script /etc/rs.lwip`、:286 `up uds`、:283 `minix.lwip.drivers.pending` 等待） | 00/03/21 | 已核对 |
| `etc/rs.lwip`（lwip 重启恢复脚本：`minix-service down/up` + TCPISN 重载 + 网络 daemon 重启清单） | 03（重启语义） | 已核对 |
| `net/uds/unix.8`（uds 服务 man page） | 21（参考） | 已核对 |

**头文件**

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `minix/com.h`（SDEV_RQ_BASE 0x1900/SDEV_RS_BASE 0x1980 + 17 请求 + 6 回复 + SDEV_OP_*/SDEV_*FLAGS） | 01/99 | 已核对 |
| `minix/ipc.h`（mess_vfs_lsockdriver_* 6 布局） | 01/99 | 已核对 |
| `minix/sockdriver.h`（struct sockdriver/sockdriver_call/sockdriver_data/packed_data + 函数声明） | 01 | 已核对 |
| `minix/sockevent.h`（struct sock/sockevent_ops/sockevent_* API） | 02 | 已核对 |
| `minix/rmib.h`（struct rmib_call/node/oldp/newp + rmib_* API） | 03/20/21 | 已核对 |
| `minix/netdriver.h`（NDEV 协议类型） | 13（消费侧）/16（驱动面） | 已核对 |
| `minix/if.h`（MINIX_SIOCGIFMEDIA/SIOCIFGCLONERS + 指针安全格式） | 17 | 已核对 |
| `net/bpf.h`、`net/if.h`、`net/if_media.h`、`netinet/in.h`、`sys/socket.h`、`sys/un.h`、`sys/ioc_net.h` | 各模块 | 已核对（常量/结构依赖，值归 99） |
| `net/lwip/lwip.h`（SOCKID_* mask、sockaddr_any、模块接口声明） | 03/99 | 已核对 |
| `net/lwip/` 各模块头（ndev.h/ifdev.h/ifaddr.h/ipsock.h/pktsock.h/route.h/rtsock.h/rttree.h/addr.h/util.h/mcast.h/ethif.h/lldata.h/tcpisn.h/bpfdev.h/pchain.h） | 各模块 | 已核对 |

### 5.2 语义模块覆盖清单（函数级）

> 每篇文档必须覆盖的函数清单以 §5.1 映射为基线。以下列出跨文件的关键语义，防止"文件有映射但函数漏掉"：

- **01**：`sockdriver_task`（sockdriver.c:1132）/`sockdriver_process`（:1061）/`sockdriver_terminate`（:1120）/`sockdriver_announce`（:47）；17 个 SDEV 请求 do_* 分派；`sockdriver_reply_generic/accept/recv/select`；`sockdriver_copyin/out`、`vcopyin/out`、`copyin_opt/copyout_opt`；`sockdriver_pack_data/unpack_data`；`struct sockdriver_call`（sc_endpt/sc_req/_sc_grant/_sc_len）、`struct sockdriver_select`、`struct sockdriver_packed_data`
- **02**：`sockevent_init/process/clone/raise/set_error/set_shutdown`；`sockevent_get_domain/type/opt/is_listening/is_shutdown`；`sockhash_*`（256 槽）；`sockevent_pending`/`socktimer` 队列；`sockevent_ops` 全 21 项回调（sop_pair→sop_free）；悬挂调用续作（sockevent_proc.c）
- **03**：`main`（lwip.c:294）/`startup`（:287）/`init`（:198）/`alloc_socket`（:152）/`sys_now`（:24）/`set_lwip_timer`/`expire_lwip_timer`/`check_lwip_timer`；四路分发（notify/MIB/SDEV+CDEV-BDEV/NDEV_RS）；`lwip_hook_rand`（lrand48）；`mibtree_init`（mibtree.c:141）
- **04**：`mempool_init/cur_buffers/max_buffers`；`pchain_end/pchain_size`；PBUF_RAM 链替代 PBUF_POOL 语义（lwipopts PBUF_POOL_SIZE=0）
- **05**：`util_convert_err`（lwIP ERR_*→errno 双射）、`util_is_root`、`util_timeval_to_ticks`/`util_ticks_to_timeval`；`addr_*`（AF_UNSPEC 检查、sin/sin6/sdlx 解析、SOCKADDR_MAX 断言）；`addrpol_get_label/get_scope`（RFC 6724）
- **06**：`ipsock_socket`（ipsock.c:123）、src 地址选择（`ipsock_get_src_addr`，udpsock.c:107 引用）、IPPROTO_IP/IPV6 选项、hop limit/TOS、多播选项、connect 语义
- **07**：`pktsock_socket`（pktsock.c:59）/`pktsock_input`、snd/rcv buf 默认值（UDP_SNDBUF_DEF 等）、IP 层输入分发
- **08**：`tcpsock_socket`（tcpsock.c:242）、连接状态机、send/recv 队列、MSS/Nagle/delayed ACK、listen/accept、SO_* 选项、错误传播（sockevent_set_error）、SIGPIPE；`tcpisn_init`/`lwip_hook_tcp_isn`（SHA256 + isn_secret）
- **09**：`udpsock_socket`（udpsock.c:116）/`udpsock_bind`/input、多播 TTL/loop 默认（TTL=1/loop on）、校验和
- **10**：`rawsock_socket`（rawsock.c:290）、IPv4/IPv6 raw 头部处理、ICMP、util_is_root 权限
- **11**：`lnksock_socket`（lnksock.c:41）、IOCTL 支撑集；`lldata_*` 链路层路由（与 rttree 分离的原因）
- **12**：`mcast_init`、成员加入/离开、全局 128 上限（IPv4 64 + IPv6 64）、per-socket 上限（MAX_GROUPS_PER_SOCKET 8）、与 lwIP IGMP/MLD 结构映射
- **13**：`ndev_init/check/process/conf/send/can_recv/recv`（ndev.h）、NR_NDEV=8、驱动 up/down 跟踪（DS notify → ndev_check）
- **14**：`ifdev_*`（ifdev.c 全量 + ifdev.h）、`ifdev_ops` 16 项（iop_init→iop_destroy）、硬件地址列表（IFDEV_NUM_HWADDRS=3/IFHWAF_VALID/FACTORY）；`loopif_init`（loopif.c:420）
- **15**：`ethif_*`（ethif.c 1718 行：初始化/收发队列/配置请求/ifdev_ops 实现）
- **16**：`ifaddr_*`（ifaddr.c 2224 行：IPv4/IPv6 地址列表、活动硬件地址切换、与 ifdev 的字段边界）
- **17**：`ifconf_init`（默认 loopback 配置）、`ifconf_ioctl`（ifconf.c:866）、MINIX_SIOCGIFMEDIA/IFGCLONERS（minix/if.h）
- **18**：`bpfdev_*`（bpfdev.c：CDEV_CLONED/select/read/write/ioctl）、`bpf_filter_ext`（bpf_filter.c:561）
- **19**：`route_init`、`lwip_hook_ip4_route/ip6_route` + `lwip_hook_etharp_get_gw/nd6_get_gw`（lwiphooks.h）、rttree 前缀树假设（"mask 可表达为 prefix length"）
- **20**：`rtsock_socket`（rtsock.c:311）、rt_msghdr/RTA 压缩展开、`net.inet/route` 树
- **21**：`uds_init`（uds.c:1303）/`uds_startup`/`uds_signal`/`main`（:1384）、5 状态机（Unconnected/Listening/Connecting/Connected/Disconnected）+ limbo、udshash（64 槽）、LOCAL_CONNWAIT、`uds_ops`（sop_* 全表）、`uds_get_info`（stat.c:11）
- **22**：`uds_io_init`、recv 缓冲段（UDS_BUF=32768）、ancillary（FD 传递/凭据，UDS_CTL_MAX=4096）、SOCK_STREAM/SEQPACKET/DGRAM 差异、悬挂续作
- **23**：`__socket`→`_syscall(VFS_PROC_NR, VFS_SOCKET)` + `_tcp/_udp/_raw/_uds_socket` fallback；`bind/connect/listen/accept/sendto/recvfrom/sendmsg/recvmsg/setsockopt/getsockopt/getsockname/getpeername/shutdown/socketpair` 全族
- **24**：lwipopts.h 全配置面（NO_SYS/PBUF_POOL_SIZE=0/MEM_SIZE/TCP_MSS=1460/TCP_WND=16384/TCP_SND_BUF=11*MSS/MEMP_NUM_*）、lwiphooks.h 4 hook、arch/cc.h、patches 4 个（weak 标记/IP forwarding 运行时控制/RA 忽略/避免大连续分配）

### 5.3 排除表（WONTFIX / 移交）

| 项 | 归属 | 说明 |
|----|------|------|
| VFS 客户端侧 `servers/vfs/socket.c` + `sdev.c` + `smap.c` | 05-stage-vfs（22-sdev/24-socket） | socket 系统调用服务端 + SDEV 客户端库，已规划 |
| NDEV 驱动面（libnetdriver + 14 个网卡驱动） | 16-stage-drivers（03-netdriver-framework + 22/23） | 网卡驱动主循环/NDEV 协议驱动面；17 仅消费（13） |
| MIB 服务端（10-stage-mib） | 10-stage-mib | rmib 远端节点服务端；17 为节点注册方 |
| 网卡驱动具体实现（virtio_net/dp8390 等） | 16-stage-drivers | 与 17 的 ndev 消费侧对称 |
| ifconfig/route/ping/tcpdump 等命令 | 18-stage-commands | 用户态网络工具（消费本 stage 的 socket/ioctl） |
| `net/gen/*` 旧式网络设备协议（TCP_DEVICE/UDP_DEVICE/IP_DEVICE） | [ARCH] N-2（WONTFIX） | 旧 inet server 遗留，minix-rs 不移植 |
| liblwip 全量移植 | [ARCH] N-1（设计决策） | 第三方栈替代策略（24 定稿） |
| sockdriver 的 select 语义与 VFS select 联动 | 02（框架内）+ 05-stage-vfs/23-select | 驱动侧续作在 02，VFS 客户端在 05 |
| `sys/socket.h`/`netinet/in.h` 常量全集 | 14-stage-runtime/13-constants-abi + 99 | 常量值集中，17 侧重使用语义 |
| 网卡热插拔/多网卡拓扑管理 | 13/14（基础）+ 16（驱动） | 单网卡语义先行 |

---

## 6. 实施顺序

### 6.1 文档改写状态跟踪

> 实施顺序 = 叙述顺序：框架 → lwip（骨架→socket 族→接口面→路由）→ uds → ABI → 第三方栈 → 全局概念。每篇完成后勾选。

| 序 | 文档 | 状态 |
|----|------|------|
| 1 | `00-net-overview.md` | ☐ |
| 2 | `01-sockdriver-framework.md` | ☐ |
| 3 | `02-sockevent-framework.md` | ☐ |
| 4 | `03-lwip-main-init.md` | ☐ |
| 5 | `04-lwip-mempool.md` | ☐ |
| 6 | `05-lwip-util-addr.md` | ☐ |
| 7 | `06-lwip-ipsock.md` | ☐ |
| 8 | `07-lwip-pktsock.md` | ☐ |
| 9 | `08-lwip-tcpsock.md` | ☐ |
| 10 | `09-lwip-udpsock.md` | ☐ |
| 11 | `10-lwip-rawsock.md` | ☐ |
| 12 | `11-lwip-lnksock.md` | ☐ |
| 13 | `12-lwip-mcast.md` | ☐ |
| 14 | `13-lwip-ndev.md` | ☐ |
| 15 | `14-lwip-ifdev.md` | ☐ |
| 16 | `15-lwip-ethif.md` | ☐ |
| 17 | `16-lwip-ifaddr.md` | ☐ |
| 18 | `17-lwip-ifconf.md` | ☐ |
| 19 | `18-lwip-bpfdev.md` | ☐ |
| 20 | `19-lwip-route.md` | ☐ |
| 21 | `20-lwip-rtsock.md` | ☐ |
| 22 | `21-uds-core.md` | ☐ |
| 23 | `22-uds-io.md` | ☐ |
| 24 | `23-libc-socket.md` | ☐ |
| 25 | `24-liblwip-port.md` | ☐ |
| 26 | `99-net-global-concepts.md` | ☐ |

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**Gate 证据**（命令 + 输出）：

- `ls minix3/minix/net/lwip/*.c | wc -l` → 27；`wc -l minix3/minix/net/lwip/*.c | tail -1` → 24477
- `ls minix3/minix/net/uds/*.c | wc -l` → 3；`wc -l minix3/minix/net/uds/*.c | tail -1` → 3406
- `rg -n "sockdriver_task|sockdriver_process|sockdriver_terminate|sockdriver_announce" lib/libsockdriver/sockdriver.c` → 47/1061/1120/1132
- `rg -n "^main\(|^startup|^init\(|^alloc_socket" net/lwip/lwip.c` → 152/196/270/294
- `rg -n "^uds_init|^uds_startup|^uds_signal|^main\(" net/uds/uds.c` → 1303/1349/1367/1384
- `rg -n "NR_IPV4_MCAST_GROUP|NR_IPV6_MCAST_GROUP" lib/liblwip/lib/lwipopts.h` → 64/64（mcast.c 全局上限 128）
- `ls minix3/minix/lib/libc/sys/ | rg -i "sock|peer|bind|connect|listen|accept|shutdown|send|recv" | wc -l` → 15（含 getpeername.c；3173 行）

**发现与修正**（全部已落入 §2/§4/§5）：

| # | 问题 | 级别 | 修正 |
|---|------|------|------|
| D-1 | `startup()` 行号 287→270（287 是 `sef_startup()` 调用处）；`init()` 198→196；`uds_startup()` 1377→1367 | P1 | §1.1/§1.2 已修正 |
| D-2 | `sdr_*` 回调实为 19 项（初稿写 18）；`sockevent_ops` 实为 21 项（初稿写 19）；`ifdev_ops` 实为 16 项（初稿写 17） | P1 | §2/§5.2 已修正 |
| D-3 | mcast 全局上限误写 2048；实为 `NR_IPV4_MCAST_GROUP(64) + NR_IPV6_MCAST_GROUP(64) = 128`，per-socket 上限 `MAX_GROUPS_PER_SOCKET(8)` | P1 | §4 N-13/§5.2 已修正 |
| D-4 | libc socket 封装文件数漏计 getpeername.c：14→15 文件，行数 ~2992→3173 | P1 | §1.1/§2/§5.1 已修正 |
| D-5 | `ifconf_ioctl` 行号 930（文件长度）→866（函数定义处） | P1 | §5.2 已修正 |
| D-6 | lwip stateless 注释行号 304-307→275-278 | P1 | §4 N-12 已修正 |
| D-7 | 缺少启动配置/服务定义文件（lwip.conf/uds.conf/rc/rs.lwip）的覆盖条目 | P1 | §5.1 新增"配置与启动脚本"小节 |

**拆分合理性结论**：26 篇（00 + 01~24 + 99），与 15/16 stage 的 26 篇体量一致；单篇最大 C 语义 2793 行（tcpsock）与 VM 最大篇（08-pagetable-ops ~71KB）相当；每篇以"server 启动链位置或框架层级"自答定位，边界无交叉（§3.3 全表核对）。

**语义全覆盖结论**：27/27 lwip .c + 3/3 uds .c + sockdriver/sockevent/liblwip 3 框架 + 15/15 libc socket 封装全部落入新文档（§5.1 映射表），无孤儿 C 文件。

### 7.2 minix3 源码回归 review（2026-08-16）

**Gate 证据**（关键 grep 实证，全部与 §5.1/§5.2 断言一致）：

- `rg -n "UDP_SNDBUF_DEF|UDP_RCVBUF_DEF" net/lwip/udpsock.c` → 8192/32768（:30-33）；`TCP_SNDBUF_DEF/TCP_RCVBUF_DEF` tcpsock.c:87-90 → 32768/MAX(TCP_WND,32768)
- `rg -n "lwip_hook_rand" net/lwip/lwip.c` → :127；`bpf_filter_ext` bpf_filter.c:149
- `rg -n "ip4_route|ip6_route|weak" net/lwip/route.c` → weak-symbol 覆盖 + gateway hook（:7-8）
- `rg -n "prefix" net/lwip/rttree.c` → "所有掩码可表达为 prefix length"假设（:5-8）
- `rg -n "ifconf_ioctl" net/lwip/lnksock.c` → `sop_ioctl = ifconf_ioctl`（:75，AF_LINK 仅支撑 IOCTL 的证据）
- `rg -n "up lwip|up uds" etc/usr/rc` → :259/:286；`etc/rs.lwip` 恢复脚本语义核对
- `sed -n '190,300p' net/lwip/lwip.c` → init 链 17 步全序列核对（srand48→lwip_init→sockevent_init→...→ifconf_init→init_timer）
- `sed -n '340,382p' net/lwip/lwip.c` → 主循环四路分发核对（notify/MIB/SDEV+CDEV-BDEV/NDEV_RS）
- `sed -n '1295,1420p' net/uds/uds.c` → uds_init 四步 + 主循环 MIB/其他二分核对

**回归核对结论**：

1. **覆盖完整性**：§5.1 映射表逐行与 `ls`/`wc` 实证一致；27+3+3+15 = 48 个 C 文件全部有归属，无遗漏、无重复归属。
2. **ARCH 演进**：14 项 ARCH（N-1~N-14）全部有 Minix3 行为对照点（文件/行号），其中 N-1（lwIP 第三方栈）与 N-2（libc 旧式 fallback）为最大演进面；directMap 类"Minix3 无、minix-rs 引入"的演进在本 stage 表现为 Rust 栈替代 + 类型化 sockid/SockAddr/rmib。
3. **与相邻 stage 边界**：NDEV 驱动面（16-stage-drivers/03+22/23）、VFS 客户端（05-stage-vfs/22+24）、MIB 服务端（10-stage-mib）、命令层（18-stage-commands）均已在 §5.3 排除表交叉核对，无重复范围。
4. **结论**：plan.md 可作为 17-stage-net 后续工作的直接依据。

---

## 8. 参见

- `../01-stage-kernel/00-kernel-overview.md`（组织原则来源）
- `../02-stage-vm/plan.md`（单 server 主线先例）
- `../14-stage-runtime/plan.md`（非 server 主线重定义 + libc 边界先例）
- `../15-stage-fs/plan.md`（多 server 主线重定义先例）
- `../16-stage-drivers/plan.md`（驱动子系统 + 16/17 边界）
- `../05-stage-vfs/plan.md`（VFS socket 客户端侧 22/24）
- `draft/README.md`（原占位素材）
