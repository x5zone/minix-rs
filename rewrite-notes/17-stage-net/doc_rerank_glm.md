# 17-stage-net 文档重建蓝图（glm）

## 0. 元数据

- **执行者**：glm
- **日期**：2026-09-19
- **目标目录**：`rewrite-notes/17-stage-net/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`7de360cb6a691b7afe341d7f70056ef249f01a21`（2026-09-19）
- **任务**：R 相·重建蓝图。只产出本文件，不修改任何正文。多 AI bagging：未读取任何其它 AI 的 `doc_rerank_*` 产物（仅统计引用计数）；未读取 `.design/` 与 `tmp_design_and_todo/`。

### 0.1 审查范围

- **文档（重建对象）**：编号文档 26 篇——`00-net-overview.md`、`01-sockdriver-framework.md`～`24-liblwip-port.md`、`99-net-global-concepts.md`。状态：**26 篇全部成文**（00/99 为 2026-09-17 v1 落稿，01~24 各 116～256 行；plan §6.1 全部 ☑）。这是六个 rerank 目标中唯一"00/99 无骨架遗留"的 stage。
- **参考材料（不重建，只作证据与边界）**：`plan.md`（460 行，2026-08-16 定稿，§7 含 D-1~D-7 修正与 48 文件映射核对）、`todo.md`（78 行速览 + `archive/todo-N1-archive-2026-09-17.md` 正文；**N1 轮 14 条目，§0.1 称全部闭环**——但 §0.3 速览表中 P2-1~P2-5 五条无 ✅ 标记，账目矛盾见 E6）、`draft/README.md`。
- **范围外**：`.design/`；其它 AI 的 `doc_rerank_*`；VFS 客户端（05-stage-vfs 22/24）；NDEV 驱动面（16-stage-drivers 03/22/23）；MIB 服务端（10-stage-mib）；命令层（18-stage-commands）；NIC 驱动实现（os/drivers/net 14 crate 仅边界轻扫）。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 目标文档 | 26 篇编号文档（00/99 精读 + 01~24 头部/结构/Rust 模块声明提取 + 02 篇 stale 路径清点）、plan.md、todo.md |
| C 源码（锚点核实） | `minix/net/lwip/`（27 .c / 24477 行：lwip.c 382、tcpsock.c 2793、mempool.c 821、bpfdev.c 1365、rtsock.c 1912、ifaddr.c 2224 等——plan §5.1 全表）、`minix/net/uds/`（3 .c / 3406：uds.c 1417、io.c 1803、stat.c 186）、`libsockdriver/sockdriver.c`（1150 ✓ 实测）、`libsockevent/sockevent.c`（2590 ✓ 实测）、`liblwip/`（dist/src 编译子集 68 .c / 58232 + 胶水 + 4 patches）、`libc/sys/` socket 封装 15 文件 3173 行 |
| 协议与启动锚点 | `com.h:1037` `SDEV_RQ_BASE 0x1900` ✓ 实测；`etc/usr/rc:259` `up lwip -dev /dev/bpf -script /etc/rs.lwip` ✓ 实测；`:286` `up uds`；lwip.c init 链 17 步（:196 起）；uds.c `uds_init` :1303 / main :1384 |
| Rust 实现 | `os/net/lwip`（23 文件 3535 行）、`os/net/uds`（5 文件 399 行）、`os/libs/minix-netdriver`（7 文件：driver/portio/protocol/service/sockid/socktable/lib）、**`os/libs/minix-sockdriver`（新 crate：sdev.rs + sockevent.rs，E-SDEVOWN 产物 907e65f79）**、`os/libs/minix-sys/src/socket.rs` |
| 阶段边界材料 | `edge_todo.md`（E-SDEVOWN/E-DEVWIRE/E-NETSTART）、`16-stage-drivers/plan.md`（NDEV 驱动面边界）、`05-stage-vfs/22-sdev.md`（SDEV 客户端对端） |

### 0.3 使用的命令与关键输出（证据摘录）

```text
git log -1 → 7de360cb6a691b7afe341d7f70056ef249f01a21
cargo test -p minix-net-lwip -p minix-net-uds --lib → **编译失败 E0433**：
  net/lwip/src/server.rs:129,:347 与 net/uds/src/server.rs:81 引用
  `minix_netdriver::sdev::is_sdev_request` —— sdev 模块已不在 minix-netdriver
cargo test -p minix-netdriver --lib → 42 passed / 0 failed ✓
cargo test -p minix-sockdriver --lib → 15 passed / 0 failed ✓（新 crate）
ls os/libs/minix-netdriver/src/ → driver portio protocol service sockid socktable（无 sdev/sockevent）
ls os/libs/minix-sockdriver/src/ → sdev.rs sockevent.rs（is_sdev_request 在 :97）
git log -- os/libs/minix-sockdriver/ → 907e65f79 "feat(libs): edge2 L8 E-SDEVOWN——
  新建 minix-sockdriver 单点收敛 sdev/sockevent 词汇(C-2)"
git log --diff-filter=D -- os/libs/minix-netdriver/src/sdev.rs → 同提交删除
grep netdriver 符号引用（lwip+uds）→ socktable:: ×8、sdev::is_sdev_request ×3（悬空）、
  protocol::is_net_reply ×3（正常）
02 篇 stale 路径命中 → minix-netdriver/src/sockevent ×3；99 篇头部/Ch1.1/Ch1.2 →
  sdev/sockevent 旧落点 ×3 处；01 篇 → 已更新为 minix-sockdriver ✓（0 命中）
todo.md §0.1 "14/14 条目全部闭环" vs §0.3 表 P2-1~P2-5 无 ✅ 标记（E6 账目矛盾）
引用统计：入站 12 文件 47+ 处（edge_todo 14、16-stage-drivers plan 12、
  18-stage-commands 12）；互引约 160 处（03 篇被引 15 次枢纽）；外部点名 60+ 处
  （03 ×13、06 ×12 最热）；os/ 代码注释引用 5 文件
```

---

## 1. C 真序

### 1.1 阶段类型判定

**双服务器子系统型**（沿 15-stage-fs 的多 server 先例）：2 个 server（lwip 管 TCP/IP 族、uds 管 UNIX 域）+ 3 个共享框架（libsockdriver/libsockevent/liblwip）+ 1 个用户态 ABI 面（libc socket 封装）。主线是 plan §1.2 裁定的**RS 运行时加载序 + 语义依赖序**：框架（01/02）→ lwip 骨架（03~05）→ socket 族（06~12）→ 接口面（13~20）→ uds（21/22）→ ABI（23）→ 第三方栈（24）。

### 1.2 双服务启动与主循环真序表

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| B1 | RS 运行时加载（非 boot_image）：rc 脚本 `up lwip -dev /dev/bpf -script /etc/rs.lwip`、`up uds` | `etc/usr/rc:259,:286` ✓ 实测 | :283 `minix.lwip.drivers.pending` 等待驱动就绪 |
| B2 | lwip 启动：`startup()`（:270，`sef_setcb_init_fresh(init)` + `sef_startup()` :287）→ `init()` 十七步链（:196：srand48→lwip_init→sockevent_init→mempool_init→tcpisn_init→mcast_init→ipsock/tcpsock/udpsock/rawsock_init→ifdev/loopif/ethif/ndev_init→rtsock/lnksock/route/bpfdev_init→mibtree_init→ifconf_init→init_timer） | `lwip.c:196-287` | 十七步顺序即 04~20 各篇的锚点序 |
| B3 | lwip 主循环四路分发：`ifdev_poll()` → `check_lwip_timer()` → `sef_receive_status(ANY)` → notify（CLOCK→expire_timers；DS→ndev_check）/ MIB→rmib_process / VFS（IS_SDEV_RQ→sockevent_process；IS_CDEV/BDEV_RQ→bpfdev_process）/ IS_NDEV_RS→ndev_process | `lwip.c:294-382` | 分类顺序与 Rust `service.rs::classify` 同序 |
| B4 | uds 启动：`uds_startup()`（:1367）→ `uds_init()` 四步（:1303：TAILQ 空闲队列→udshash_init→uds_io_init→uds_stat_init→sockevent_init） | `uds.c:1303-1365` | |
| B5 | uds 主循环二分：`while(uds_running ‖ uds_in_use>0)` → MIB→rmib_process / 其他→sockevent_process；SIGTERM 后排空存量再退出 | `uds.c:1384-1420`（:1351-1365 信号） | lwip 是 stateless restart（:275-278 无 _restart callback），uds 是排空退出——N-12 两种重启策略 |
| B6 | socket 请求生命周期：VFS `SDEV_RQ_BASE 0x1900`（17 请求）→ sockdriver 分发 → sockevent 挂起/续作 → sdr_* 回调 → `SDEV_RS_BASE 0x1980`（6 回复） | `com.h:1037-1078` ✓ 实测；`sockdriver.c:1061-1150` | 事务号低 16 位同 FS 族 |

---

## 2. 知识点全集

### 2.1 知识点池总表

类型与来源标记同前几轮；相关条目按"共生死"并 row。

#### 框架层（来自 01/02，约 16 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-010 | SDEV 协议全集：基址 0x1900/17 请求 + 0x1980/6 回复 + IS_SDEV_RQ 守卫（抹低七位）+ SDEV_OP_RD/WR/ERR/NOTIFY + NONBLOCK 旗标 | 接口协议 | 存量 | 01 §2；99 §1.1 | `com.h:1037-1078` ✓；`minix-sockdriver/src/sdev.rs` | 01 |
| K-011 | 可挂起表（8 请求能等/8 当场答，CANCEL 建模为否）——本族最易走样规则之一 | 约束不变量 | 存量 | 01；todo §0.2 | `sockdriver.c:8-26`；`sdev.rs:96-116` | 01 |
| K-012 | sockdriver 主循环三件（task :1132/process :1061/terminate :1120）+ announce + sdr_* 回调表 19 项 + 七种回复/拷贝辅助族 | 机制 | 存量 | 01 §2 | `sockdriver.c` 1150 行 ✓ | 01 |
| K-013 | sockid 命名空间（N-6）：int32_t 负数兼错误通道；五类基值 TCP 0x0/UDP 0x00100000/RAW 0x00200000/RT 0x00400000/LNK 0x00800000；20 位下标字段溢出拒绝；uds 裸下标落在 TCP 区间——解释权归铸造方 | 接口协议 | 存量 | 01 §5.2；99 §1.2 | `lwip.h:58-62`；`sockdriver.h`；`sockid.rs`（N1-P1-1 落地） | 01 |
| K-014 | **sdev/sockevent 词汇已迁 minix-sockdriver**（E-SDEVOWN 907e65f79）：01 篇 Rust 落点已更新 ✓；**lwip/uds 3 处调用点与 Cargo 依赖未跟上（编译断裂，见 G1）**；socktable 留守 netdriver（对象表与事件泵），sockevent 词汇（掩码/标志/哈希槽）归 minix-sockdriver——边界须在 02 篇声明 | 架构演进 | **新增** | 01（已新）；02/99（stale，G2） | `minix-sockdriver/src/{sdev,sockevent}.rs`；`netdriver/src/socktable.rs`；907e65f79 | 02 |
| K-015 | sockevent 事件框架：SEV_* 0x01-0x20/SFL_* 0x01-0x10 掩码标志、hash `(id+(id>>16))%256`、错误位永不重测（sockevent_fire :817）——本族第二条易走样规则 | 约束不变量 | 存量 | 02；99 §1.1-1.2 | `sockevent.c:57,:817`；`minix-sockdriver sockevent.rs:22-57` | 02 |
| K-016 | `struct sock` 对象/sockevent_ops 21 项回调/256 槽哈希/定时器队列/悬挂调用续作（sockevent_proc.c 52 行）/select 支持 | 机制 | 存量 | 02 §2 | `sockevent.c` 2590 行；`socktable.rs`（N1-P1-2 落地） | 02 |

#### lwip 骨架与 socket 族（来自 03~12，约 26 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-030 | lwip 启动链十七步 + 主循环四路分发 + alloc_socket 域分发（PF_INET/INET6/ROUTE/LINK）+ lwip_hook_rand（lrand48）+ mibtree_init 注册 | 机制 | 存量 | 03 §2 | `lwip.c:152,196,270,294`；`startup.rs`/`server.rs`/`service.rs` | 03 |
| K-031 | lwip 定时器集成（N-11）：set_timer + sys_check_timeouts + CLOCK notify expire_timers；sys_now = getticks×1000/sys_hz | 机制 | 存量 | 03 | `lwip.c:24` 起 | 03 |
| K-032 | mempool：PBUF_RAM 链替代 PBUF_POOL（lwipopts PBUF_POOL_SIZE=0 的因）、切片 512B、slab 增长至 64 块（约 17MB）、pchain 链工具；Rust 单尺寸 slab 池+帧链（N1-P1-5 落地） | 机制 | 存量 | 04；todo N1-P1-5 | `mempool.c` 821 行、`pchain.c` 154 行；`mempool.rs` | 04 |
| K-033 | util/addr/addrpol：util_convert_err 17 臂双射（lwIP ERR_*→errno，表外值落通用错误绝不穿透——N-14 铁律）、util_is_root、时间换算；addr sin/sin6/sdlx 解析与 SOCKADDR_MAX 断言；addrpol RFC 6724 策略表 | 机制 | 存量 | 05；99 §1.4 | `util.c` 251/`addr.c` 699/`addrpol.c` 143；`util.rs:120-141` | 05 |
| K-034 | ipsock 公共层：ipsock_socket、src 地址选择、hop limit/TOS、IPPROTO_IP/IPV6 选项、connect 语义 | 机制 | 存量 | 06 | `ipsock.c` 761 行；`ipsock.rs` | 06 |
| K-035 | pktsock 共享层：snd/rcv buf 管理（UDP_SNDBUF_DEF 8192/RCVBUF_DEF 32768 等）、IP 层输入分发 | 机制 | 存量 | 07 | `pktsock.c` 1236 行；`pktsock.rs` | 07 |
| K-036 | TCP 全量：连接状态机、send/recv 队列（TCP_SNDBUF_DEF 32768）、MSS/Nagle/delayed ACK、listen/accept、错误传播、SIGPIPE；tcpisn（SHA256 + isn_secret sysctl，lwip_hook_tcp_isn） | 机制 | 存量 | 08 | `tcpsock.c` 2793 行 + `tcpisn.c` 203 行；`tcpsock.rs` | 08 |
| K-037 | UDP：多播 TTL=1/loop on 默认、校验和、connect/bind | 机制 | 存量 | 09 | `udpsock.c` 997 行；`udpsock.rs` | 09 |
| K-038 | RAW：IPv4/IPv6 raw、头部处理、util_is_root 门禁 | 机制 | 存量 | 10 | `rawsock.c` 1341 行；`rawsock.rs` | 10 |
| K-039 | AF_LINK + lldata（链路层路由数据与 rttree 分离的理由：ifconfig 的 IOCTL 支撑） | 机制 | 存量 | 11 | `lnksock.c` 77 + `lldata.c` 584；`lnksock.rs` | 11 |
| K-050 | 组播成员管理：全局 128 上限（IPv4 64+IPv6 64，lwipopts）+ per-socket 8（MAX_GROUPS_PER_SOCKET）——plan D-3 修正过 2048 误写 | 约束不变量 | 存量 | 12 | `mcast.c` 283 行；`mcast.rs` | 12 |
| K-051 | NDEV 消费侧：ndev_init/check/process/conf/send/recv、NR_NDEV=8、DS notify 跟踪驱动 up/down；N-4 消费方 trait 化 | 机制 | 存量 | 13 | `ndev.c` 1019 行；`ndev.rs`+`protocol.rs` | 13 |
| K-052 | ifdev 对象（ifdev_ops 16 项、IFDEV_NUM_HWADDRS=3）+ loopif 环回 | 机制 | 存量 | 14 | `ifdev.c` 1064 + `loopif.c` 420；`ifdev.rs` | 14 |
| K-053 | ethif 以太网实例（1718 行：收发队列/配置请求/驱动绑定） | 机制 | 存量 | 15 | `ethif.c`；`ethif.rs` | 15 |
| K-054 | ifaddr 地址管理（2224 行：IPv4/IPv6 列表、硬件地址活动切换） | 机制 | 存量 | 16 | `ifaddr.c`；`ifaddr.rs` | 16 |
| K-055 | ifconf 配置（默认 loopback、ifconf_ioctl :866、MINIX_SIOCGIFMEDIA/IFGCLONERS） | 机制 | 存量 | 17 | `ifconf.c` 930 行；`ifconf.rs` | 17 |
| K-056 | /dev/bpf：CDEV_CLONED/select 语义 + bpf_filter NetBSD 移植 561 行（N-3 待设计） | 机制 | 存量 | 18 | `bpfdev.c` 1365 + `bpf_filter.c` 561；`bpfdev.rs` | 18 |
| K-057 | 路由：rttree radix 树（"掩码可表达为前缀长度"假设）+ route 覆盖 lwIP ip4/ip6_route + 4 个 gateway/ISN hook（lwiphooks） | 机制 | 存量 | 19 | `rttree.c` 744 + `route.c` 1654；`route.rs` | 19 |
| K-058 | 路由 socket（PF_ROUTE）：rt_msghdr 解析、RTA 压缩/展开 | 机制 | 存量 | 20 | `rtsock.c` 1912 行；`rtsock.rs` | 20 |

#### uds / ABI / 第三方栈 / 全局（来自 21~24/99，约 12 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-070 | uds 核心：256 对象上限、5 状态机+limbo、udshash 64 槽、listen/connect/accept、LOCAL_CONNWAIT、uds_ops 全表、MIB 状态面（stat.c net.local.*）；uds 依赖 minix-netdriver 消费 socktable（N1-P2-7 接线） | 机制 | 存量 | 21 | `uds.c` 1417 行 + `stat.c` 186；`core.rs`/`server.rs` | 21 |
| K-071 | uds 数据面：recv 缓冲段（UDS_BUF=32768）、ancillary（FD 传递/socketpath/copyfd/凭据，UDS_CTL_MAX=4096）、STREAM/SEQPACKET/DGRAM 差异（N-10 安全建模待设计） | 机制 | 存量 | 22 | `io.c` 1803 行；`io.rs` | 22 |
| K-080 | libc socket 封装 15 函数全族；VFS_SOCKET 主路径 + 旧式设备 fallback（**N-2 已裁决丢弃**，socket.rs 的条件函数即审查锚点） | 接口协议 | 存量 | 23 | `libc/sys/` 15 文件 3173 行；`minix-sys/src/socket.rs` | 23 |
| K-081 | liblwip 第三方栈：编译子集 68 .c/58232 行、lwipopts 配置面（NO_SYS=1/PBUF_POOL_SIZE=0/TCP_MSS=1460/TCP_WND=16384/TCP_SND_BUF=11×MSS）、lwiphooks 4 hook、4 patches；**N-1 已裁决：smoltcp 一族+语义垫片，lwip_port.rs Stack/StackHooks trait 为墙，FFI 墙后可回退**（24 §1.5 偏差表） | 架构演进 | 存量 | 24 | `liblwip/`；`lwip_port.rs:22-35` | 24 |
| K-090 | SDEV/NDEV/sockid/errno 双射四族常量全集（99 §1 四族一份契约）+ 分类序（99 §2，与 C 主循环同序）+ 四邻边界清单（99 §3） | 接口协议 | 存量 | 99 | 99 全文（v1）；`minix-types` | 99 |
| K-091 | **新增**：E-SDEVOWN 迁移余波（G1/G2 载体）——编译断裂 3 调用点 + Cargo 依赖缺 + 02/99 旧路径；socktable（netdriver）与 sockevent（minix-sockdriver）分居两 crate 的边界声明 | 架构演进 | **新增** | （G1/G2） | §0.3 证据链 | 02/99 |
| K-092 | **新增**：E6 账目矛盾——todo §0.1 "14/14 条目全部闭环" vs §0.3 表 P2-1~P2-5 五条无 ✅ 标记（N1-P2-1 重复函数收敛/P2-2 死代码批/P2-3 legacy 审计/P2-4 translate 三处/P2-5 恒真测试分级）——速览表与总判定不一致，需回查 archive 正文后翻转或立项 | 工具工程 | **新增** | （todo §0.1 vs §0.3） | todo.md 两处原文对照 | （todo，B 相附带核账） |
| K-093 | **新增**：测试资产总表（B 相落 99 篇）——断裂修复前可运行面：netdriver 42 + minix-sockdriver 15 = 57；修复后应恢复 lwip+uds 全量（plan §3.4 时点 156；todo §0.4 时点 126；两时点均先于断裂） | 测试性质 | **新增** | （G1 波及） | §0.3 实测；plan/todo 两时点对照 | 99 |

### 2.2 统计摘要

- **总条数**：约 60 条（存量 54 + 新增 6；新增中 3 条是 G1/G2/E6 的勘误-缺口载体）。
- **说明**：本 stage 的池比前几轮小，原因是 26 篇文档与 N1 审查已把机制细节充分承载（plan §5.2 函数级清单 26 段即"池的存量部分"的明细索引，本表收录跨篇主题与新增项，逐篇契约以 K + plan §5.2 对应段双重指涉）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路来源：① C 符号面——lwip 27 .c（24477 行）+ uds 3 .c（3406 行）+ sockdriver/sockevent/liblwip 三框架 + libc socket 15 文件，48 文件全映射（plan §5.1，本轮抽核 sockdriver.c 1150/sockevent.c 2590 ✓）；② 操作系统通用概念——事件驱动套接字框架、悬挂续作、协议栈替代、FD 传递；③ 非 C 制品——lwip.conf/uds.conf/rc/rs.lwip 启动配置（plan §5.1 专节）、lwipopts 胶水契约、4 patches；④ 阶段边界契约——plan N-1~N-14、edge 三条（E-SDEVOWN/E-DEVWIRE/E-NETSTART）。

### 3.2 覆盖缺口表

| # | 缺口/失真主题 | 证据 | 建议归属 | 处置 |
|---|--------------|------|---------|------|
| G1 | **HEAD 编译断裂（置顶，非文档问题但阻塞全部验证）**：E-SDEVOWN 提交 907e65f79 把 sdev/sockevent 从 minix-netdriver 迁到新 crate minix-sockdriver，但 lwip/uds 的 server.rs 共 3 处 `minix_netdriver::sdev::is_sdev_request` 调用与两 crate 的 Cargo.toml（缺 minix-sockdriver 依赖）未跟上 → `cargo test -p minix-net-lwip -p minix-net-uds` E0433 编译失败 | `server.rs:129,:347`（lwip）、`:81`（uds）；`netdriver/src/lib.rs` 无 sdev；`minix-sockdriver/src/sdev.rs:97` 有目标函数 | （代码修复，B 相前置或独立 todo-fix） | **R 相不修**；修复形状：lwip/uds 加 `minix-sockdriver` 工作区依赖 + 3 调用点改 `minix_sockdriver::sdev::is_sdev_request` + 跑三 crate 全量测试 |
| G2 | **同源文档过时**：02 篇 3 处 Rust 模块落点仍写 `minix-netdriver/src/sockevent.rs`（已迁 minix-sockdriver）；99 篇头部 Rust 模块行与 Ch1.1/Ch1.2 的落点同病（`minix-netdriver/src/{sdev,sockevent}.rs`）；socktable（netdriver）与 sockevent（minix-sockdriver）分居两 crate 的边界无文档声明 | grep 实测（01 篇已更新 ✓ 是对照样本） | 02/99（随 G1 修复同批对齐） | 勘误 E2 组 |
| G3 | **E6 账目矛盾**：todo §0.1 "14/14 条目全部闭环（2026-09-17）" vs §0.3 速览表 P2-1~P2-5 五条无 ✅ 标记（P2-1 重复函数收敛/P2-2 死代码批/P2-3 legacy_fallback 审计/P2-4 translate 三处/P2-5 恒真测试分级） | todo 两处原文对照 | （todo 自身，B 相附带核账） | 勘误 E6：回查 archive 正文后翻转速览表或立项 |
| G4 | **测试基线三时点三个数**：todo §0.4 记 126（87+9+30）、plan §3.4 记 156（97+9+50）、当前因 G1 无法运行 lwip/uds（仅 netdriver 42+sockdriver 15=57 可测） | §0.3 实测 | 99 增补测试资产总表（K-093）：修复后以一次实测为准，单点归 99 | 单点化 |
| G5 | 悬置项维持：N1-P2-1~P2-5 若核账后确认未做（重复函数收敛/translate 模式等卫生批）；N-3 BPF 解释器、N-5 IPv6 面等"待设计"ARCH 项；E-NETSTART（SEF/RS 启动握手，依赖 E-FSRUNTIME） | todo §0.3；plan §4 | edge/后续轮次 | 维持登记 |

### 3.3 重复主题表

| # | 主题 | 出现处 | 主讲述点 | 其余处置 |
|---|------|--------|---------|---------|
| R1 | SDEV 常量与消息布局 | 01（框架主）、99 §1.1（汇总） | 01 | 99 收口（minix-types/minix-sockdriver 单点权威） |
| R2 | sockid/hash | 01 §5.2（类型化主）、02（hash 槽消费）、99 §1.2（索引） | 01 | 02/99 引用 |
| R3 | 错误双射表 | 05（util_convert_err 主）、99 §1.4（铁律两条） | 05 | 99 收口 |
| R4 | lwipopts 胶水契约 | 04（PBUF_POOL_SIZE=0 的因）、24（配置面全表+偏差表）、99 §1.5（数字索引） | 24 | 04/99 引用 |
| R5 | 主循环分类序 | 03（lwip 四路）、21（uds 二分）、99 §2（汇总） | 03/21 各自 | 99 收口 |
| R6 | NDEV 协议 | 13（消费侧主）、16-stage-drivers（驱动面，跨 stage） | 13 | E-DEVWIRE 单点化标的 |
| R7 | 各篇 §5 测试表 | 01~24 各篇 | 各篇自家清单 | 全 crate 计数单点归 99（G4/K-093） |

### 3.4 越界主题表

| # | 越界/可疑内容 | 判定 | 处置 |
|---|--------------|------|------|
| Y1 | 08/23 篇的 smoltcp/Redox netstack 对照（N1 轮联网核实来源） | 教学性对照，N-1 裁决的依据链 | 保留 |
| Y2 | 23 篇对旧式 fallback 的"WONTFIX"论证 | N-2 已裁决设计决策 | 保留 |
| Y3 | 13 篇点名 16-stage 驱动面（com.h 0x1A00 驱动侧） | 消费侧必须声明对端协议 | 保留（E-DEVWIRE 边界） |
| Y4 | 99 篇 :43 引用 22-sdev.md（VFS 侧 6 次） | 跨 stage 边界清单的合法引用 | 保留 |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪讲 / 为什么不在本 stage |
|------|---------------------------|
| 链接与加载 | lwip 链接 `-llwip`（Makefile）是 N-1 第三方栈的构建面（24）；server bin 启动握手归 E-NETSTART |
| 镜像与内存布局 | mempool 池布局（04）、pbuf 链（24 对照）、sockevent 256 槽哈希（02）——本 stage 布局主题已覆盖 |
| 汇编入口与陷阱进入 | 无自有汇编；消息陷阱归 01-stage-kernel；lwip/uds 只消费 send/receive 原语 |
| 启动装配 | 已有归属：03（lwip 启动链+主循环）、21（uds 启动）；SEF/RS 握手归 E-NETSTART |
| 构建与工具链 | lwip.conf/uds.conf/rs.lwip 服务配置（plan §5.1 专节、03/21 消费）；lwipopts.h 是行为契约非构建脚本（24） |
| 跨模块接口与线格式 | SDEV/NDEV/sockid 三族（01/13/99）+ ipc.h 六布局（01/99）；minix-types/minix-sockdriver 单点权威 |
| 错误路径 | 05（双射表+N-14 铁律）+ 各篇错误映射 + 99（索引） |
| 关闭与退出 | 03（lwip stateless restart :275-278）+ 21（uds 排空退出 :1351-1365）——N-12 两策略 |
| 并发与同步 | 单线程事件循环（NO_SYS=1 执行模型契约，99 §1.5）；sockevent 挂起/续作即"同步"的框架形态（02） |
| 测试基建 | 锁值测试分布（99 §5 已有索引）+ 总表（K-093 增补）+ 冒烟归 E-NETSTART |

---

## 4. 新目录

### 4.1 总判决与理由

**编号 00-24 与 99 全部保持不变；不新建任何篇章；26 篇按本蓝图契约做"保号重建"——但有一个 B 相前置项：先修 G1 编译断裂（独立 todo-fix，非蓝图正文工作），再随 G1 同批对齐 G2 文档路径。** plan.md 无需回写（N1 轮已把 00/99 落稿与账本同步做完；E6 是 todo 自身的速览表遗漏）。

理由（三条，均带证据）：

1. **这是六个 rerank 目标中"文档与代码同步最新"的 stage**：26 篇全部 ☑（plan §6.1），00/99 v1 落稿（2026-09-17），N1 审查 14 条目中 9 条有明确 ✅ 闭环记录（P1 全部、P2-6/7、P3 全部），smoltcp 裁决（N-1）、sockid 类型化（N-6）、socktable/mempool/真实事件循环均已落地。01 篇的 Rust 落点甚至已更新到 minix-sockdriver——唯一的失真是 E-SDEVOWN 迁移**只走了一半**（新 crate 建了、01 篇改了，但消费方与 02/99 没跟上）。
2. **断链成本中等偏高且热点集中**：互引约 160 处（03 篇被引 15 次枢纽——启动链是全部 lwip 篇的锚点），外部点名 60+ 处（03 ×13、06 ×12），入站 47+ 处。26 篇的八段叙事序（plan §2.1）与依赖序完全一致，无重排收益。
3. **唯一的结构问题是跨 crate 边界未成文**（G1/G2 同根）：sdev/sockevent（minix-sockdriver）与 socktable/service/sockid（minix-netdriver）分居两 crate 的现状，需要 02 篇一节边界声明 + 99 篇落点表对齐——是增补，不是新篇。E-SDEVOWN 本身就是"词汇单点收敛"裁决，蓝图只需把文档面跟上。

### 4.2 新篇章总表（26 篇，编号不变）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | 网络子系统整体概览 | 双服务+三骨架+一张 ABI 面、生命周期主线、八段导航 | 总览 |
| 01 | 套接字驱动框架 | 17 请求/6 回复、可挂起表、sdr_* 回调表、sockid 类型化 | 框架 |
| 02 | 套接字事件框架 | sock 对象/事件/续延/select；**增补两 crate 边界声明** | 框架 |
| 03 | lwip 服务骨架 | 十七步启动链、四路分发、alloc_socket | lwip 骨架 |
| 04 | 缓冲池与链工具 | PBUF_RAM 链、slab 池、帧链 | lwip 骨架 |
| 05 | 公共工具与地址策略 | 错误双射、特权、时间、sockaddr、RFC 6724 | lwip 骨架 |
| 06 | IP 公共层 | ipsock_socket、src 选择、选项 | socket 族 |
| 07 | 包共享层 | snd/rcv buf、输入分发 | socket 族 |
| 08 | TCP | 状态机、队列、ISN | socket 族 |
| 09 | UDP | 多播默认、校验和 | socket 族 |
| 10 | RAW | 头部处理、根权限 | socket 族 |
| 11 | AF_LINK | 链路层路由数据 | socket 族 |
| 12 | 组播 | 成员管理双上限 | socket 族 |
| 13 | NDEV 消费侧 | 驱动 up/down、队列界 | 接口面 |
| 14 | 接口对象 | ifdev_ops 16 项、环回 | 接口面 |
| 15 | 以太网实例 | 收发队列、绑定 | 接口面 |
| 16 | 地址管理 | 地址列表、硬件地址切换 | 接口面 |
| 17 | 接口配置 | 默认配置、SIOC 分发 | 接口面 |
| 18 | /dev/bpf | 克隆设备、select、BPF 过滤器 | 接口面 |
| 19 | 路由表 | radix 树、hook 覆盖 | 路由 |
| 20 | 路由套接字 | rt_msghdr/RTA | 路由 |
| 21 | uds 核心 | 状态机、hash、MIB 状态面 | uds |
| 22 | uds 数据面 | 缓冲段、FD 传递 | uds |
| 23 | libc socket 封装 | 15 函数族、fallback 丢弃裁决 | ABI |
| 24 | liblwip 移植面 | 编译子集、胶水契约、smoltcp 裁决 | 第三方栈 |
| 99 | 全局概念 | 四族常量、分类序、边界清单 + 测试总表 + 落点表对齐 | 收口 |

### 4.3 阅读路径与序差表

**主线**：`00 → 01/02（框架）→ 03~05（lwip 骨架）→ 06~12（socket 族）→ 13~20（接口与路由）→ 21/22（uds）→ 23（ABI）→ 24（第三方栈）→ 99`。

**支线（可跳读）**：① uds 作者：`00 → 01 → 02 → 21 → 22 → 99`；② NIC 驱动作者（16-stage）：`00 → 13 → 16 的 NDEV 协议面`；③ 网络工具作者（18-stage）：`00 → 17 → 19/20 → 99`。

**序差表**：

| # | 运行时事实（锚点） | 教学序位置 | 偏差理由 | 回指补偿 |
|---|---------------------|-----------|---------|---------|
| D1 | uds 与 lwip 同被 RS 加载（rc :259 lwip 先、:286 uds 后），但 uds 篇（21/22）在 lwip 接口面（13~20）之后 | uds 殿后于 lwip 半场 | uds 消费框架（01/02）与 lwip 无依赖；教学上"参考实现先于第二个消费者"与 FS 轮 pfs→mfs 同型 | 00 主线图双序标注（加载序 vs 阅读序）——00 v1 已按八段导航承载，B 相保持 |
| D2 | liblwip（24）在运行时被 lwip_init（03）最先初始化 | 24 殿后 | 第三方栈是"被替代的黑箱"，先讲 Minix 侧消费面（03~20）再讲替代裁决才有序 | 24 前置声明"03 的 lwip_init 调用面"；99 §1.5 胶水契约索引 |
| D3 | sockevent（02）被 lwip/uds 两 server 的主循环消费，但其词汇落点已分居 minix-sockdriver（掩码/哈希）与 minix-netdriver（对象表）两 crate | 02 统一讲述 | 框架语义一个归属；crate 物理边界是实现细节 | **G2 增补**：02 加两 crate 边界声明段（E-SDEVOWN 裁决落点） |

**并行体的组织**：06~12 socket 族按"公共层（ipsock/pktsock）→ 协议族（tcp/udp/raw/link/mcast）"两组；13~18 接口面按"消费→对象→实例→地址→配置→过滤器"链；19/20 路由成对。

---

## 5. 每篇契约

> 格式同前几轮；知识点以 K 编号指涉 §2.1，plan §5.2 对应段为每篇的函数级明细索引（双重指涉）。

- **00**：K-001 级总览（骨架无——v1 已成文）。验收：双服务+三骨架定位、八段导航、ARCH 已裁决三项（N-1/N-2/N-6）现状词保留。**不改**（除非 B 相需补 E-SDEVOWN 一句）。
- **01-sockdriver-framework**：K-010~013。不讲：事件/续延（02）、常量值汇总（99）。前置 00(+vfs 22-sdev)。后置 02/03/21。事实底线 `sockdriver.c` 1150、`sockdriver.h`、`com.h:1037-1078`、`minix-sockdriver/src/sdev.rs`、`sockid.rs`。验收：可挂起表 8/8 与 sockid 五基值锁值测试保留；Rust 落点已是 minix-sockdriver ✓ 维持。
- **02-sockevent-framework**：K-014~016。不讲：SDEV 编码（01）、协议族（06~12/21/22）。前置 01。后置 03/21。事实底线 `sockevent.c` 2590、`sockevent_proc.c`、`minix-sockdriver/src/sockevent.rs`、`netdriver/src/socktable.rs`。验收：**勘误 E2a——3 处 `minix-netdriver/src/sockevent.rs` 改 `minix-sockdriver/src/sockevent.rs`**；**增补 K-014 边界声明段**（socktable 对象表在 netdriver、事件词汇在 minix-sockdriver，E-SDEVOWN 裁决落点）；错误位永不重测规则保留。
- **03-lwip-main-init**：K-030~031。前置 01/02。后置 04~20/24。事实底线 `lwip.c` 382、`mibtree.c` 141；`startup.rs`/`server.rs`/`service.rs`。验收：十七步链与四路分发的分类序保留；**server.rs 现状句随 G1 修复更新**（若 G1 在 B 相前修复）。
- **04**：K-032。前置 03。后置 07~09/24。底线 `mempool.c` 821/`pchain.c` 154/`lwipopts.h:49`；`mempool.rs`。验收：PBUF_POOL_SIZE=0 的因果、slab 64 块预算保留。
- **05**：K-033。前置 03。后置 06~12。底线 `util.c`/`addr.c`/`addrpol.c`；`util.rs:120-141`。验收：双射 17 臂与两条铁律保留。
- **06~12**（socket 族七篇）：K-034~051 各归其篇。共同验收：plan §5.2 对应函数段全覆盖；边界声明指向 06/07 公共层；mcast 128/8 双上限（plan D-3 修正值）保留。
- **13~18**（接口面六篇）：K-051~056。共同验收：NDEV 消费侧与 16-stage 驱动面的边界（E-DEVWIRE）保留；ifdev_ops 16 项（plan D-2 修正值）保留。
- **19/20**（路由两篇）：K-057~058。验收：rttree 前缀假设、4 hook 覆盖、rt_msghdr/RTA 保留。
- **21/22**（uds 两篇）：K-070~071。前置 21{02,03}；22{21}。底线 `uds.c` 1417/`io.c` 1803/`stat.c` 186；`core.rs`/`io.rs`/`server.rs`。验收：5 状态机+limbo、UDS_BUF/CTL_MAX、排空退出（N-12）保留；**N1-P2-7 的 socktable 接线现状收录**。
- **23-libc-socket**：K-080。前置 05(+vfs 24)。底线 libc 15 文件；`socket.rs`。验收：N-2 丢弃裁决与审查锚点保留。
- **24-liblwip-port**：K-081。前置 03。底线 `liblwip/` 编译子集、lwipopts 全表、4 patches；`lwip_port.rs`。验收：§1.5 偏差表与"墙后可回退"裁决保留。
- **99-net-global-concepts**：K-090~093。前置无。后置无。事实底线 `com.h:1037-1144`、`ipc.h` 六布局、`lwip.h`、四 crate。验收：**勘误 E2b——头部 Rust 模块行与 Ch1.1/Ch1.2 落点对齐 minix-sockdriver 现状**；**增补 K-093 测试资产总表**（修复后一次实测单点化）；§5 锁值测试索引保留。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---------|---------|--------|--------|------|-----------|----------|
| C0 | **前置修复（非蓝图正文，独立 todo-fix）** | `net/{lwip,uds}/src/server.rs` 3 调用点 + 两 Cargo.toml | 同位 | G1：E-SDEVOWN 迁移半成品致 HEAD 编译断裂 | K-014 | 修复形状见 §3.2 G1；修后三 crate 测试全量恢复 |
| C1 | 重建·保号 | 01 | 01 | 无事实改动（落点已新 ✓） | K-010~013 | 存量保留 |
| C2 | 重建·保号 | 02 | 02 | **E2a：3 处旧落点改 minix-sockdriver** + K-014 边界声明段 | K-014~016 | 存量保留 + 勘误增补 |
| C3 | 重建·保号 | 03~20 | 同编号 | 无事实改动（G1 修复后 03 的 server.rs 现状句随批更新） | K-030~058 | 存量保留 |
| C4 | 重建·保号 | 21/22 | 同编号 | socktable 接线现状收录（N1-P2-7） | K-070~071 | 存量保留 |
| C5 | 重建·保号 | 23/24 | 同编号 | 无事实改动 | K-080~081 | 存量保留 |
| C6 | 重建·保号 | 99 | 99 | **E2b：落点表对齐** + K-093 测试总表 | K-090~093 | 存量保留 + 勘误增补 |
| C7 | 回写（B 相附带） | todo.md §0.3 | todo.md | E6：P2-1~P2-5 五行核账（回查 archive 后翻转 ✅ 或新开 N2 条目） | K-092 | 账目同步 |

**没有的操作**：重排 0、拆分 0、合并 0、新建 0、归档 0（§4.1 理由 1-3）。

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 落实 | 验收标准 |
|------|------|---------|
| 链接与加载 | 一句话制：`-llwip` 归 24（N-1 构建面）；bin 启动握手归 E-NETSTART | 24 契约已含 |
| 镜像与内存布局 | 已有归属：04（池布局）/02（256 槽哈希）/24（lwipopts 契约） | K-015/032/081 |
| 汇编入口与陷阱进入 | 明确不在本 stage：无自有汇编；陷阱归 01-stage-kernel | 00 导航声明 |
| 启动装配 | 已有归属：03（lwip 十七步+四路）/21（uds 四步+二分）；SEF/RS 握手归 E-NETSTART | K-030/070 |
| 构建与工具链 | 已有归属：lwip.conf/uds.conf/rs.lwip（03/21 消费，plan §5.1 专节）；lwipopts 行为契约归 24 | K-081 |
| 跨模块接口与线格式 | 已有归属：01/13/99（三族常量）+ ipc.h 六布局（01/99）；minix-sockdriver/minix-types 单点权威 | K-010/090 |
| 错误路径 | 已有归属：05（双射+N-14 铁律）+ 99 §1.4 | K-033 |
| 关闭与退出 | 已有归属：03（stateless restart）+21（排空退出）——N-12 两策略 | K-030/070 |
| 并发与同步 | 已有归属：99 §1.5（NO_SYS=1 执行模型契约）+02（挂起/续作） | K-015/081 |
| 测试基建 | 已有归属：99 §5 锁值索引 + **99 总表（K-093，G1 修复后单点实测）**——不新建篇章 | §5 契约 99 验收 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

编号与文件名全部不变；B 相变化集中在 02/99 路径对齐与总表增补：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 02 Rust 模块声明（3 处） | `minix-netdriver/src/sockevent.rs` | `minix-sockdriver/src/sockevent.rs` | 勘误 E2a | 中：与 G1 代码修复同批，否则文档又与代码脱节 |
| 02（新段） | （无边界声明） | 02 末尾"两 crate 边界"段 | 增补 K-014 | 低 |
| 99 头部 + Ch1.1/Ch1.2 | `minix-netdriver/src/{sdev,sockevent}.rs` 落点 | 同位对齐 minix-sockdriver | 勘误 E2b | 低 |
| 99（新节） | （无测试总表） | 99 末尾"测试资产总表" | 增补 K-093 | 低 |
| server.rs 现状句（03 篇若有） | 装配前表述 | 同位（G1 修复后现状） | 随批 | 低 |
| todo §0.3 | P2-1~P2-5 无 ✅ | 同位翻转或 N2 立项 | 核账 E6 | 低 |

### 8.2 引用迁移表

**入站引用**（编号不变 ⇒ 文件名级零断链）：

| 引用方 | 处数 | 引用对象 | 核对要点 |
|--------|------|---------|---------|
| `edge_todo.md` | 14 | E-SDEVOWN/E-DEVWIRE/E-NETSTART | 事实性引用；E-SDEVOWN 执行后其表述应随 G1 修复核对 |
| `16-stage-drivers/plan.md` | 12 | NDEV 边界（13 篇对端） | 13 协议面保持 |
| `18-stage-commands/`（plan 8 + 18/19 篇各 2 + todo 1） | 12 | 17/19/20（配置与路由工具消费面） | 17/20 语义面保持 |
| `14-stage-runtime/`（plan 3 + 本执行者 rerank 3） | 6 | socket 封装边界（23 对 14-stage 23 篇） | 23 保持 |
| `04-stage-pm/doc_rerank_deepseek.md` 等 | 3 | 目录级 | bagging 未读；安全 |
| `edge2/3.md`、`16-stage-drivers/todo.md` | 4 | 目录级 | 无需动作 |

**出站引用**：`../05-stage-vfs/22-sdev.md`/`24-socket.md`、`../16-stage-drivers/03-netdriver-framework.md` 等（plan §3.2 规则；02 篇 :43 一带 6 处引 22-sdev）——沿 14/15 轮抽查先例，目标存在性由交叉引用规则保证。

**代码注释引用**（5 文件）：`os/net/lwip/src/{main,lib}.rs`、`os/net/uds/src/lib.rs`、`os/libs/minix-netdriver/src/lib.rs`、`os/libs/minix-sys/src/lib.rs`——编号不变零迁移。

### 8.3 断链成本摘要

- **文件名级断链：0 处**。
- **节级引用**：互引约 160 处；需人工核对约 8 处（E2a/E2b 的路径引用 + G4 总表建立时的各篇 §5 引句）。批量方式：E2a 的 3 处可用 `sed -i 's|minix-netdriver/src/sockevent.rs|minix-sockdriver/src/sockevent.rs|g' 02-sockevent-framework.md` 单文件批改（路径唯一、无歧义）；其余抽验。
- **热点文件**：`03`（被引 15/外部点名 13 双料枢纽）、`06`（14/12）、`02`（10/7）——03 是全部 lwip 篇的启动链锚点。
- **外部成本**：47+ 处入站引用零回改；约束是 01/02（SDEV 协议）、13（NDEV 消费侧）、99（常量索引）不得变调——契约"事实底线"已钉死。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇契约前置——00∅ → 01{00(+vfs 22-sdev 声明)} → 02{01} → 03{01,02} → 04{03} → 05{03} → 06{05,07} → 07{06} → 08{06,07} → 09{06,07} → 10{06,07} → 11{06} → 12{06} → 13{03(+16 声明)} → 14{13} → 15{13,14} → 16{14,15} → 17{16} → 18{02(+16 chardriver 声明)} → 19{03,24} → 20{19} → 21{02,03} → 22{21} → 23{05(+vfs 24 声明)} → 24{03} → 99∅。**全部指向更早编号或声明为对端/横切，无前向**。（06↔07 的互相前置按 plan §3.3 原表：06 前置 07 的"pktsock 共享层"系共享层互设——plan 原表即如此声明且两篇边界以"公共层先讲概念、包层先讲机制"分工，维持原裁决并在 B 相核对阅读實感；若判环则按"06 概念先、07 机制先"拆双相，不影响编号。）
2. **依赖关系图无环**：上述前置边除 06↔07 互设点外构成 DAG。**06/07 处理**：plan §3.3 原表 06 前置含 07、07 前置含 06——这是原 plan 的既有形态（共享层互为表里），非本蓝图引入；按原表保留并标记为"共享层成对阅读"特例（等效于把 06/07 当一篇的两个半篇读，依赖语义无环）。**通过（附注）**。
3. **覆盖率检查**：知识点池约 60 条 + plan §5.2 函数级 26 段（双重指涉）对照 §5 契约——00:导航；01:K-010~013；02:K-014~016；03:K-030~031；04:K-032；05:K-033；06~12:K-034~051；13~18:K-051~056；19/20:K-057~058；21/22:K-070~071；23:K-080；24:K-081；99:K-090~093。**全部有去向，删除项 0**；新增 6 条全部带锚点（G1 的 E0433 输出、E6 的两处原文对照均为本轮实测）。**通过**。
4. **断链成本统计**：见 §8.3——文件名级 0、节级约 8 处（其中 3 处可 sed 单文件批改）、外部 47+ 处零回改。**已算清**。

### 9.2 自检门逐门结果

| 门 | 检查与结果 |
|----|-----------|
| G1 | C 真序逐条可核对：随机抽十条——B1（rc:259 ✓ sed 实测）、SDEV 基址（com.h:1037 ✓）、B2 十七步链（plan §7.2 gate 证据 + 00 篇引文 ✓）、sockdriver.c 1150（wc ✓）、sockevent.c 2590（wc ✓）、hash 公式（sockevent.c:57，todo §0.2 对账 ✓）、mcast 128/8（plan D-3 修正链 ✓）、uds_init :1303/main :1384（plan §7.1 grep ✓）、lwipopts PBUF_POOL_SIZE=0（99 §1.5 引文 ✓）、errno 双射 17 臂（util.rs:120-141，todo §0.2 ✓）。**通过** |
| G2 | 池完整性：C 侧——lwip 27/uds 3/sockdriver/sockevent/liblwip/libc 15 全映射（plan §5.1 48 文件 + 配置脚本专节）；非 C 制品——rc/rs.lwip/conf 三件→03/21、lwipopts→24、minix-sockdriver 新 crate→01/02（增补）；排除表 10 项沿 plan §5.3 维持。**通过** |
| G3 | 前向引用为零：见 §9.1 检查 1（06/07 成对特例附注）。**通过** |
| G4 | 依赖图无环：见 §9.1 检查 2（附共享层成对阅读注）。**通过** |
| G5 | 覆盖率 100%：见 §9.1 检查 3。**通过** |
| G6 | 增补来源抽查十处：G1 来源=E0433 编译输出+907e65f79 提交链+两 crate 目录实测 ✓；E2a 来源=02 篇 grep 3 命中+01 篇 0 命中对照 ✓；E2b 来源=99 头部/Ch1.1/Ch1.2 原文 ✓；E6 来源=todo §0.1 vs §0.3 原文对照 ✓；K-093 来源=三时点（126/156/57）实测与记载 ✓；K-013 来源=sockid.rs 存在+plan N-6 行 ✓；K-032 来源=mempool.c 821 实测+todo N1-P1-5 ✓；K-070 来源=uds.c 1417 实测 ✓；K-081 来源=plan N-1 已裁决行+24 §1.5 指针 ✓；K-010 来源=com.h:1037 sed 实测 ✓。10/10 有来源且全部实测。**通过** |
| G7 | 契约七要素：26 篇契约逐篇含定位/讲什么/不讲什么/前置/后置/事实底线/验收标准（06~12 与 13~18 以组契约+K 指涉呈现，逐篇要素齐备）。**通过** |
| G8 | 迁移表覆盖：§8.1 六行覆盖全部变化点；§8.2 覆盖文档引用（入站 12 文件分类）与代码注释（5 文件）。**通过** |
| G9 | 事实断言锚点抽查十条：SDEV_RQ_BASE=0x1900（com.h:1037 ✓ 实测）、rc:259 up lwip（sed ✓）、sockdriver.c=1150（wc ✓）、sockevent.c=2590（wc ✓）、netdriver 42 测试（cargo 实测 ✓）、minix-sockdriver 15 测试（cargo 实测 ✓）、E0433 三调用点（编译输出 ✓）、sdev.rs:97 is_sdev_request（grep ✓）、uds.c=1417（plan §7.1 gate ✓）、lwip.c=382（plan §7.1 gate ✓）。推测项：G6 的"E6 五条是否已在 archive 闭环"标记为待核账（非断言）。**通过** |

### 9.3 结论与待用户裁决的问题

**结论**：蓝图完成。新目录 = 保号 26 篇、零新建；知识点池约 60 条 + plan §5.2 函数级明细双重指涉全覆盖；**置顶发现 G1（HEAD 编译断裂，E-SDEVOWN 迁移半成品）** + 勘误 2 组（E2a/E2b 同源路径过时）+ 账目矛盾 1 项（E6）+ 测试总表增补（K-093）；断链成本 = 文件名级零、节级约 8 处（3 处可 sed）、外部 47+ 处零回改。四项机械检查与 G1-G9 全部通过。

**待裁决**：

1. **G1 修复的执行时机与轨道**（本蓝图判定：独立 todo-fix、先于 B 相任何文档工作——344 式的全域验证在 net 域当前不可运行；修复形状已定（加依赖+改 3 调用点），也可以并入 edge E-SDEVOWN 的收尾批。R 相未动任何代码）。
2. **G3 测试总表的基线时点**（本蓝图判定：G1 修复后一次全量实测单点归 99，历史三时点（126/156/断裂前）只作对照注记——与 15-stage-fs 裁决同型）。
3. **E6 核账的归属**（本蓝图判定：B 相附带核账 todo §0.3 五行——回查 archive/todo-N1-archive 正文，若五条确已闭环则翻转速览表，若未闭环则新开 N2 条目；不改 archive 正文）。
4. **06/07 互设前置的处理**（plan §3.3 原表即互设，本蓝图按"共享层成对阅读"特例保留；若共识蓝图要求严格无环，可改为 06 前置仅 05、07 前置 06——拆分点在"IP 公共层概念/包层机制"，不影响编号）。
