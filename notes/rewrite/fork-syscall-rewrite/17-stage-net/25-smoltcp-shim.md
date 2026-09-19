# 25-smoltcp-shim：语义垫片——Minix 套接字语义与 smoltcp 的映射边界

> **分类**：协议栈替代的架构文档，覆盖两侧数据模型对照、七条映射边界、定时器合成、分期
> **源码**：`minix3/minix/net/lwip/lwip.c`（382 行）、`minix3/minix/lib/libsockdriver/sockdriver.c`（1150 行）、`minix3/minix/lib/libsockevent/sockevent.c`（2590 行）、`minix3/minix/lib/liblwip/lib/lwipopts.h`（选项契约）
> **Rust 模块**：`os/net/lwip/src/stack.rs`（垫片的栈半，`Stack` 特征的实现）、`os/net/lwip/src/lwip_port.rs`（墙：`Stack`/`StackHooks` 特征与选项契约）
> **前置依赖**：`24-liblwip-port.md` §1.5（选型裁决：smoltcp 一族加自研语义垫片，墙后可回退 FFI）、`02-sockevent-framework.md`（事件对象与可挂起表）、`01-sockdriver-framework.md`（请求面）
>
> 本篇不覆盖的内容：
> - 网卡驱动的数据路径（16-stage 边界节；垫片只留 `TrunkDevice` 一个接缝）
> - 路由表与链路套接字自身的表结构（第 19/11/20 篇；它们不进 smoltcp）
> - 各套接字家族操作的逐条翻译（第 08/09/10 篇各自批次落地时写进各自的篇）
>
> 本篇只回答一个问题：**Minix 的套接字语义停在哪些形状上，smoltcp 提供哪些
> 形状，两者的边界面画在哪里、由谁持有、缺什么时谁以什么错误回答。**

## 1. 为什么有这道垫片

N1-P1-3 裁决（2026-09-17）选定协议栈本体走 smoltcp 一族，但 smoltcp 是一台
无阻塞、按时间片推进的协议栈：它没有进程概念，不会挂起谁，也不认识 Minix 的
grant 与消息。Minix 的 lwip 服务则是请求驱动的：VFS 送来一条 sdev 请求，处理
不完就把调用进程挂起，稍后续答。两边之间差的那层翻译就是语义垫片。

垫片的方向由墙的类型面钉死：服务代码只依赖 `lwip_port.rs` 的 `Stack` 与
`StackHooks` 两个特征，永不 `use smoltcp`。栈可以整个换掉（换 smoltcp 版本、
换另一族 Rust 栈、甚至回到 FFI 拼 liblwip），服务侧一行不改——这就是
N1-P1-3 说"墙后可回退 FFI"的确切含义：回退换的是 `stack.rs` 一个模块。

## 2. 两侧数据模型对照

| Minix 侧 | smoltcp 侧 | 对应关系 |
|----------|------------|----------|
| `SockTable`（`minix-netdriver/src/socktable.rs`，线上权威） | `SocketSet`（栈内自有） | 两张表各管各的：服务表管 Minix 语义（套接字标志、事件位、挂起账），栈集合管协议状态；垫片只保存"服务套接字到栈内下标"的一跳 |
| `sock_id`（smap 行内编号，`make_smap_dev` 的高低位） | `SocketHandle`（栈内数组下标） | 同为不透明整数，但两个命名空间不相干；垫片做翻译，永不把栈内下标直接发上线 |
| sdev 十七种请求（`com.h:1037-1078`） | 无对应物（栈只暴露 open/close/readiness/收发帧） | 请求翻译是垫片上最大的一块面，随各套接字家族批次逐条落地（第 08/09/10 篇） |
| 可挂起表（`sockdriver.c:8-26`，八是八否） | 无阻塞：任何操作立即返回 | 垫片承担"没办完写留言条"：不能立即完成的请求按事件对象的挂起面记账，栈推进出结果后续答（第 2.3 节） |
| 三张 grant 的数据面（data/ctl/addr，`request.c` 的 `sdev_readwrite`） | 栈内环形缓冲（`TcpSocket` 的收发缓冲、`PacketBuffer` 的包环） | 数据搬运发生在垫片里：grant 直指用户缓冲，栈缓冲是中转，两端各拷一次；缓冲定容走 lwipopts 契约（第 2.4 节） |
| `sockevent` 事件位（`SEV_*`，`sockevent.h:7-19`） | 就绪位（可读/可写） | 垫片把栈的就绪查询翻译成 SEV 位（`Stack::readiness` 的返回值到 `sockevent` 的唤醒是服务侧的动作，第 02 篇） |
| 主定时器（`lwip_timer`，`lwip.c:255-259` 布防；`lwip.c:315`/`:328` 查与响） | `Interface::poll` 内部的定时器服务与 `poll_delay` | 两个世界各有一个主定时器概念，合成规则见第 3 节 |
| 随机源（`srand48(clock_time(NULL))`，`lwip.c:203-206`） | `Config::random_seed`（`iface/interface/mod.rs:169`，驱动 TCP 初始序号） | 同位：都是把外部熵喂给栈做序号源；服务在启动链第一步算好，构造栈时交进去 |

## 3. 映射边界七条

**第一条：身份。** Minix 的 `sock_id` 与 smoltcp 的 `SocketHandle` 是两个命名
空间。垫片是唯一的翻译点：`Stack::open` 返回的 `StackSocket` 里装的是栈内
下标，服务侧把它存进自己的表行；上线发给 VFS 的永远是 `make_smap_dev` 的
设备号，不是栈内下标。句柄失效的语义两边对齐：栈内对象随关闭消失，之后再
按已关闭句柄查询，垫片回缺省就绪位，不炸。

**第二条：状态。** smoltcp 的 TCP 状态机（`tcp::State`：Listen、SynSent、
Established、CloseWait 等）比 sockdriver 面上的状态细。垫片不做全状态翻译，
只翻译服务真正消费的两个面：就绪位（可读/可写，喂 `SEV_RECV`/`SEV_SEND`）
与错误（操作失败时的 `STACK_*` 线上值，`util.rs` 的 N-14 双射表）。中间状态
（半开、正在关闭）不单独上报——C 侧同样不暴露它们，行为面保持一致。

**第三条：阻塞。** Minix 请求可以挂起调用进程（VFS 侧的 `suspend_on_sdev`），
smoltcp 的每个调用都立即返回。垫片的规则是"没办完写留言条"：请求不能立即
完成时，按第 02 篇事件对象的挂起面记下（哪个套接字、哪种事件、什么请求），
`Stack::poll` 推进出结果后，服务扫挂起账续答。挂起与续答的编排住在服务侧
（sockevent 批次），垫片只保证一件事：`poll` 之后，就绪位是新鲜的。

**第四条：数据面。** grant 直指用户缓冲，栈只认自己的缓冲，两端各拷一次：
收到数据先落栈内环形缓冲，垫片再从栈缓冲拷进用户 grant；发送反向同理。
缓冲定容走 lwipopts 契约（`lwip_port.rs` 的常量组）：TCP 接收窗 16384、
发送缓冲 11 × 1460（`lwipopts.h:267`/`:282`），UDP 与 RAW 的包环按同一
量级取整块。契约常量是垫片必须守住的行为面——`Stack::open` 的缓冲按它们
定容，测试锁死。

**第五条：地址。** Minix 的 `sockaddr_in`/`sockaddr_in6` 与 smoltcp 的
`IpEndpoint` 双向转换住在垫片。族不对、长度不对的地址在转换处拒绝（线上
EINVAL），不进栈。端口字节序的翻译也在这里（线上网络序、栈内主机序）。

**第六条：不进栈的面。** 路由套接字（第 19/20 篇）、链路套接字（第 11 篇）、
过滤器设备（第 18 篇）、管理树（`mibtree.c`）都是 Minix 自有语义，不映射到
smoltcp：C 的 lwIP 里它们同样绕过协议栈直连服务表。垫片不收它们的消息，
`Stack::open` 对 ICMP 家族先以通用错误回答——C 侧的 ICMP 走 RAW 协议口，
smoltcp 的独立 ICMP 套接字不在本特性集里，等第 07 篇批次裁决接线方式。

**第七条：墙与时间。** 设备半边是 `stack.rs` 的 `TrunkDevice`：16-stage 网卡
数据路径落地前收发恒空，接口一切就绪只是不见包；`Stack::receive_frame`/
`Stack::transmit_frame` 如实回答"网络未接"与"暂无待发帧"，不假装收发。
时间半边按墙的约定一律是毫秒数（C `sys_now` 的同一时间基）：服务从单调时钟
换算出毫秒送进 `Stack::poll`，栈报回的下次交付时刻也是毫秒。两种实现
（smoltcp 或回退的 FFI 栈）都吃同一个时间形状。

**选项半（SDEV_SETSOCKOPT/SDEV_GETSOCKOPT 的墙方法，选项批补充）。**
C 的选项处理分两层：libsockevent 框架半承接 SOL_SOCKET 的开关型与容量型
选项（`sockevent.c:1823-1966`），各套接字模块的 `sop_setsockopt` 承接协议级
选项（`tcpsock.c:2123`、`udpsock.c:580`）。本模型两层都在服务，墙上只开一个
收拢口子：`Stack::sockopt_tcp` 与 `Stack::sockopt_udp`，形状是
`(socket, name, Option<i32>) -> Result<Option<i32>, i32>`——带值即设置
（回 `Ok(None)`），`None` 即查询（回当前值）。收拢形 vs 逐选项一个墙方法
的取舍：后者会让墙随选项清单膨胀且每加一项都动特征，收拢后墙上只有两个
方法，名字的"已支持名单"作为策略留在服务路（sockopt_road），墙只认名单
内的名字并落效果，不认识的名字按栈参数错误回答、由路折成 ENOPROTOOPT
（C 框架对驱动不认识的选项同样要求 ENOPROTOOPT，`sockdriver.c:843-844`
的对偶契约）。已落名单与效果：SO_KEEPALIVE（TCP，使能取 C pcb 缺省空闲值
`TCP_KEEPIDLE_DEFAULT` = 7200000 毫秒，`tcp_priv.h:138-139`）、SO_SNDBUF/
SO_RCVBUF（TCP/UDP 各按 `tcpsock.c:86-91`/`udpsock.c:29-34` 的三档契约
界内改记、查询回当前值）、SO_BROADCAST（UDP；smoltcp 无对位 API，广播
收发是栈内行为 `udp.rs:483`，旗标记在服务侧槽位上，查询语义保留——发送
面的差异在此登记）。**已登记差异**：物理缓冲维持 lwipopts 契约尺寸、不随
SO_SNDBUF/SO_RCVBUF 改（smoltcp 缓冲构造后定容）；协议级选项（NODELAY、
KEEPIDLE 族、组播族）与框架开关族的其余名字（REUSEADDR、LINGER、
LOWAT/TIMEO 等，`sockevent.c:1857-1966`）统一按 ENOPROTOOPT 诚实拒绝，
随各自后续批次评估。

## 4. 定时器的合成

C 的定时器面是三件套：主定时器布防（`init_timer(&lwip_timer)` 加
`recheck_timer = TRUE`，`lwip.c:255-259`）、每趟循环先查一遍
（`check_lwip_timer`，`lwip.c:315`）、时钟响铃服务到期项
（`expire_timers`，`lwip.c:328`）。smoltcp 把"到点的定时器随一遍推进而
服务"合成进一次 `Interface::poll`，`poll_delay` 报下次需要推进的时刻。

垫片的合成规则：`Stack::poll` 一次做完"推进加服务"，返回下次交付时刻
（`PollWhen::At` 携带毫秒时刻，或 `Never` 表示睡到下一个消息到来）；布防
语义由调用方兑现——启动链第七阶段布防后的第一趟主循环就调一次 `poll`
（对应 C 注释"万一上楼时就有定时器开了张，进消息循环先查一遍"）。主循环
每趟循环先查的那半（`check_lwip_timer` 在接收之前的调用位）随主循环批次
接线，本篇只立语义位。

## 5. 分期与旗标

- **本批（启动链步 2 与步 13）**：`SmoltcpStack` 构造、墙特征全实现、
  TCP/UDP/RAW 三家族的开户与就绪位、`TrunkDevice` 接缝、启动链两步接线。
  入站帧与出站帧以"网络未接"如实回答。
- **随后批**：套接字家族操作逐条翻译（第 08/09/10 篇各自的批次）、
  sockevent 挂起与续答编排（第 02 篇批次）、地址转换与 select 面。
- **等 16-stage**：`TrunkDevice` 换真网卡设备（收发两个半），数据面队列随
  N1-P1-5 的帧模型细化。
- **留白登记**：主循环每趟的先查半（第 4 节）；ICMP 家族接线方式（第 3 节
  第六条）。

## 6. 测试

- **垫片自身**（`stack.rs`）：开户回收槽位、三家族各占一槽且 ICMP 如实报
  错、失效句柄回缺省就绪位、空栈推进报"睡到下个消息"、关闭未打开句柄按
  错误回答、选项半的保活与容量往返（`sockopt_tcp`/`sockopt_udp` 的界检查
  与旗标位）。
- **启动链**（服务二进制）：七步走满后栈已构造且定时器已布防；时钟响铃
  路推进栈。
- **逐家族批次追加**：每个套接字家族落地时补该家族的操作往返测试（宿主
  侧以墙特征为界，可用形状替身与真垫片各跑一遍）。

## 7. 参见

`24-liblwip-port.md`（墙的常量与裁决记录）、`03-lwip-main-init.md` §2.2
（十三步落点表）、`02-sockevent-framework.md`（挂起与续答）、
`01-sockdriver-framework.md`（请求面）、16-stage 边界节（网卡接缝）。
