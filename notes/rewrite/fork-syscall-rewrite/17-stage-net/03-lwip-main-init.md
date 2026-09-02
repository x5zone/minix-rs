# 03-lwip-main-init — lwip 服务骨架（启动链 + 主循环 + 分发）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- lwip 服务骨架：`main`（lwip.c:294）/`startup`（:270，`sef_setcb_init_fresh(init)` + `sef_startup()` 在 :287）/`init`（:196）
- init 全启动链 17 步：`srand48(clock_time)` → `lwip_init()` → `sockevent_init(alloc_socket)` → `mempool_init` → `tcpisn_init` → `mcast_init` → `ipsock_init` → `tcpsock_init` → `udpsock_init` → `rawsock_init` → `ifdev_init` → `loopif_init` → `ethif_init` → `ndev_init` → `rtsock_init` → `lnksock_init` → `route_init` → `bpfdev_init` → `mibtree_init` → `ifconf_init` → `init_timer`（lwip.c:198-252）
- 主循环四路分发：notify（CLOCK→`expire_timers`、DS→`ndev_check`）/ MIB（`rmib_process`）/ VFS（IS_SDEV_RQ→`sockevent_process`、IS_CDEV_RQ/IS_BDEV_RQ→`bpfdev_process`）/ NDEV_RS（`ndev_process`）
- `alloc_socket`（:152）域分发：PF_INET/PF_INET6→TCP/UDP/RAW、PF_ROUTE→rtsock、PF_LINK→lnksock
- `lwip_hook_rand`（:127，lrand48）；`sys_now`（:24）时间源；`set_lwip_timer/expire_lwip_timer/check_lwip_timer` 定时器集成
- `mibtree_init` 注册时序（mibtree.c，在所有模块注册子树之后）
- 服务配置：`lwip.conf`（domain INET/INET6/ROUTE/LINK、ipc SYSTEM/vfs/rs/vm/mib）
- Rust: `os/net/lwip`（bin 入口 + 事件循环）

## 边界

- **前置依赖**: 01/02
- **本篇不覆盖**: 各模块内部实现（04~20）；MIB 服务端（10-stage-mib）。
- **讲述结构**: 见 `plan.md` §3.1
