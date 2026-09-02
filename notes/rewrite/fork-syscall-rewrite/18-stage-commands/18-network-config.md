# 18-网络配置与诊断

> **状态**: pending（最小骨架，待改写）
> **定位**: 网络配置与诊断
> **源码**: `minix3/sbin/{ifconfig,route,ping,ping6}/、usr.sbin/{arp,ndp,traceroute,traceroute6,rdate,rtadvd}/、usr.bin/netstat/、minix/commands/{netconf,slip,swifi}/、etc/{hosts,services,protocols}`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 配置命令契约表（14 命令）：ifconfig/route/netconf（接口/路由配置）、arp/ndp（邻居表）
- - 诊断命令：ping/ping6/traceroute/traceroute6/netstat（raw socket 面）
- - 时间/路由守护面：rdate/rtadvd；串行 IP：slip；无线：swifi
- - hosts/services/protocols 数据库使用面
- - [ARCH] A-7：经 17-stage-net socket 封装的 ioctl/raw socket 面

## 边界

- - **前置依赖**: 13、17-stage-net（socket ABI）
- - **不覆盖（移交）**: lwip/uds 实现（17-stage-net）

