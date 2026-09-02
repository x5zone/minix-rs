# 21-uds-core — uds 服务核心（对象模型 + 连接状态机）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- uds 服务（uds.c:1417）：`uds_init`（:1303，TAILQ 空闲队列 → udshash_init → uds_io_init(io.c:102) → uds_stat_init(stat.c:163) → sockevent_init(uds_socket, :1326)）、`uds_startup`（:1367，SEF + SIGTERM handler）、`uds_signal`（:1349）、`main`（:1384，MIB→rmib_process / 其他→sockevent_process）
- 对象模型：`NR_UDSSOCK=256` 静态数组、空闲队列、`udshash` 64 槽（dev,ino→socket）
- 连接状态机 5 状态（Unconnected/Listening/Connecting/Connected/Disconnected）+ limbo socket（LOCAL_CONNWAIT 语义，uds.h）
- `uds_socket`（:222）域/类型分发（SOCK_STREAM/SEQPACKET/DGRAM）、`uds_ops`（sop_* 全表，sockevent 21 项）
- MIB 状态面：`uds_get_info`（stat.c:11，kinfo_pcb）+ `uds_stat_init`（stat.c:163，net.local.*）
- 服务配置：`uds.conf`（domain LOCAL、uid 0 供 socketpath/copyfd）
- Rust: `os/net/uds`（服务核心）

## 边界

- **前置依赖**: 02、03
- **本篇不覆盖**: 数据面/ancillary/FD 传递（22）；sockevent 框架（02）。
- **讲述结构**: 见 `plan.md` §3.1
