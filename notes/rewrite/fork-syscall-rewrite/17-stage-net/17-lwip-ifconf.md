# 17-lwip-ifconf — 接口配置与 ioctl

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `ifconf_init`（ifconf.c:16）：默认配置——loopback 接口建立
- `ifconf_ioctl`（ifconf.c:866）：接口 ioctl 入口（sop_ioctl 挂载点，lnksock/各 socket 共用）
- ioctl 族：ifreq/ifcap/ifmedia/ifclone/ifaddrpref/v4（ifreq+ifalias）/v6（ifreq+ifalias+ndireq+nbrinfo）/dl（lifaddr）分派
- Minix 扩展：`minix/if.h`（MINIX_SIOCGIFMEDIA/IFGCLONERS，指针安全格式）
- Rust: `os/net/lwip`（配置模块）

## 边界

- **前置依赖**: 16
- **本篇不覆盖**: 地址语义（16）。
- **讲述结构**: 见 `plan.md` §3.1
