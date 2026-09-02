# 24-liblwip-port — 第三方 lwIP 栈移植面（ARCH N-1）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开；本篇为 ARCH 决策文档）

## 核心点

- liblwip 构成：`dist/src`（第三方 lwIP，编译子集 core/ipv4/ipv6/netif 68 .c / 58232 行）+ `lib/` 胶水（lwipopts.h / lwiphooks.h / arch/cc.h）+ `patches/`（4 个：weak 标记/IP forwarding 运行时控制/RA 忽略/避免大连续分配）
- lwipopts.h 配置面：NO_SYS=1、SYS_LIGHTWEIGHT_PROT=0、PBUF_POOL_SIZE=0、LWIP_ARP/ICMP/IGMP/UDP/TCP=1、LWIP_DHCP/DNS/AUTOIP=0、TCP_MSS=1460、TCP_WND=16384、TCP_SND_BUF=11*MSS、MEMP_NUM_TCP_PCB=NR_TCPSOCK*3/2、NR_IPV4/IPV6_MCAST_GROUP=64
- lwiphooks.h 4 hook：`lwip_hook_tcp_isn`、`lwip_hook_ip4_route`/`ip6_route`（panic 哨兵）、`lwip_hook_etharp_get_gw`/`nd6_get_gw`
- Minix 侧调用面：`lwip_init()`、`sys_now()`、`sys_check_timeouts()`（lwip.c 集成）
- **Rust 栈替代决策（ARCH N-1）**：候选 smoltcp / 自研精简栈 / FFI 保留 C；首版可 FFI 后逐层替换；lwipopts 常量映射为 Rust 常量
- Rust: 外部 Rust 栈（smoltcp 或自研）

## 边界

- **前置依赖**: 03（lwip_init 调用面）
- **本篇不覆盖**: Minix 侧各模块（03~20）。
- **讲述结构**: 见 `plan.md` §3.1
