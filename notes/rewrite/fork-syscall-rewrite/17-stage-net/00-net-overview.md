# 00-net-overview: 网络子系统整体概览

> **状态**: 正文 v1 落稿（2026-09-17，架构扫描轮后与台账同批修订）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/net/`（lwip 27 .c + uds 3 .c）+ `minix3/minix/lib/libsockdriver/` + `libsockevent/` + `liblwip/` + `minix3/minix/lib/libc/sys/`（socket 封装 15 文件）
> **Rust 模块**: `os/net/lwip`、`os/net/uds`、`os/libs/minix-netdriver`、`os/libs/minix-sys/src/socket.rs`

## 核心点

- 网络子系统是什么：2 个 server（lwip TCP/IP sockets driver + uds UNIX domain sockets driver）+ 3 个共享框架库（libsockdriver/libsockevent/liblwip）+ 1 个用户态 ABI 面（libc socket 封装），不是单个 server（plan §1.1）
- 双 server 启动图：RS 运行时加载（`etc/usr/rc:259` `up lwip`、`:286` `up uds`）→ lwip `init()` 十三步启动链 + 主循环四路分发 → uds `uds_init()` 4 步 + 主循环二分分发（plan §1.2）
- 与相邻 stage 的边界：VFS 客户端（05-stage-vfs 22-sdev/24-socket）、NDEV 驱动面（16-stage-drivers 03/22/23）、MIB 服务端（10-stage-mib）、命令层（18-stage-commands）（plan §5.3）
- 文档导航：8 阶段 26 篇，新编号交叉引用规则（plan §2）
- 设计原则：位置可回答性（每篇回答"位于哪个 server 的启动链/主循环哪一步或框架哪一层"）/ 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH 全景：14 项设计期候选（plan §4），写文档时逐项确认

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~24、99）

---

## Ch1: 网络子系统是什么——两个服务、三副骨架、一张 ABI 面

Minix3 把"网络"拆成两个用户态服务：`lwip` 管互联网族（TCP/UDP/RAW/ICMP 加上
路由、链路、包过滤这些边缘面），`uds` 管 UNIX 域套接字。两个服务背后站着三副
共享骨架——libsockdriver（请求编号与挂起规则）、libsockevent（对象、事件、
续延）、liblwip（第三方协议栈本体）——外加一张面向一切进程的 ABI 面（libc 的
socket 封装族）。minix-rs 保持这个形状：`os/net/lwip` 与 `os/net/uds` 是两个
服务，`os/libs/minix-netdriver` 承载编号、标识、事件与记账的共享半边，
`os/libs/minix-sys/src/socket.rs` 承载 ABI 面的调用清单与旗标语义。

网络子系统**不是**一个单体服务：VFS 把套接字请求按服务转发，服务之间不说话；
它们共享的是协议与记账，不是内存。这条边界决定了本阶段 26 篇文档的分组方式。

## Ch2: 双服务生命周期主线

一条主线贯穿两个服务：**RS 加载 → SEF 启动拦截 → 启动链 → 主循环 → 终局**。
rc 脚本 `up lwip`（`minix3/etc/usr/rc:259`）触发 RS 拉起服务；服务的 `main`
先走启动链（lwip 十三步：随机种子、库初始化、事件库、高层套接字、接口、驱动
模块、低层套接字、路由、过滤器、管理树、默认配置、定时器、放行；
`minix3/minix/net/lwip/lwip.c:init（L203，工具生成）`），随后进入主循环：收一封消息，按来源分派到四条路之一
（`minix3/minix/net/lwip/lwip.c:startup（L293，工具生成）`）。uds 同构而更小：初始化四步、主循环二分分发、终止时排空
存量套接字再退出（`minix3/minix/net/uds/uds.c:uds_signal`）。

Rust 侧的对应物已经落位：`startup.rs` 的 `Startup` 承担启动门控，
`server.rs` 的 `run` 承担循环与分派，`minix-sef` 承担 SEF 拦截，
`minix-netdriver::service::classify` 承担按来源分类。RS 域注册与特权协议、
QEMU 真机冒烟属于跨服务边界，登记在 edge E-NETSTART，由运行时轨道统一交付。

## Ch3: 文档导航——按依赖顺序读

八段阅读序，编号即依赖方向：

1. **框架**（01、02）：请求编号、可挂起表、对象表、续延、选择、定时器——
   两个服务共同的地基；
2. **lwip 骨架**（03、04、05）：启动链与主循环、缓冲池、时间/权限/地址工具；
3. **socket 族**（06~12）：IP 公共层、包层、TCP/UDP/RAW/链路/组播各自语义；
4. **接口面**（13~20）：NDEV 消费、接口表、以太网、地址、配置、过滤器设备、
   路由、路由套接字；
5. **uds**（21、22）：核心状态机与数据面；
6. **ABI**（23）：用户态 socket 封装的调用清单与旗标；
7. **第三方栈**（24）：liblwip 的移植面与替代裁决；
8. **全局概念**（99）：常量全集、命名空间、边界在此收拢。

## Ch4: 设计原则

四条原则约束全部 26 篇：**位置可回答性**——任一函数都能回答"位于哪个服务的
启动链或主循环哪一步、或哪副骨架的哪一层"；**禁止前向引用**——阅读序即依赖
序；**每篇一个语义单元**——交叉引用代替内容交叉；**ARCH 三处一致**——架构
演进在 plan §4、对应文档、代码注释三处同时标注（当前已裁决：N-1 栈替代、
N-2 丢弃旧式回退、N-6 sockid 类型化）。

## Ch5: ARCH 全景

十四项 ARCH 候选的清单、C 侧现状与演进方向见 `plan.md` §4，本文不重复。
读各篇时遇到 `[ARCH N-x]` 标注，回 plan §4 对表；裁决与偏差记录随各篇
§3 落档（例：N-1 的三路线对比与偏差表在 24 篇 §1.5）。

## Ch6: 过渡

本篇是地图，不是疆域。下一站 01-sockdriver-framework：从十七个请求编号开始，
进入第一副骨架。
