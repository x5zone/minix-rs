# 04-lwip-mempool — pbuf 内存池与链工具

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- mempool 定制池：`PBUF_POOL_SIZE=0`（lwipopts.h）下用 PBUF_RAM 链替代 PBUF_POOL 分配（mempool.c:821）
- `mempool_init/cur_buffers/max_buffers`（池容量统计，mempool.c）
- 池规模依据：`(TCP_SNDBUF_DEF + TCP_RCVBUF_DEF) * NR_TCPSOCK / (MEMPOOL_BUFSIZE * MEMPOOL_LARGE_COUNT)`（mempool.c:233-234）
- `pchain_end/pchain_size`（pchain.c:154，pbuf 链工具）
- Rust: `os/net/lwip`（pbuf 池模块）

## 边界

- **前置依赖**: 03
- **本篇不覆盖**: lwIP 内部 pbuf 语义（24）；各 socket 模块如何消费池（08/09）。
- **讲述结构**: 见 `plan.md` §3.1
