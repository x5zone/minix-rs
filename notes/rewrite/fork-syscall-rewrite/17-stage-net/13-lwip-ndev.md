# 13-lwip-ndev — NDEV 网卡驱动消费侧

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- NDEV 消费侧（ndev.c:1019）：`ndev_init/check/process/conf/send/can_recv/recv`（ndev.h 接口）
- 驱动跟踪：`NR_NDEV=8`，驱动 up/down（DS notify → `ndev_check`），初始化请求未回复前跟踪 ndev 对象（无 ethif）
- `struct ndev_conf`：模式/组播列表/能力/标志/媒体/硬件地址（NDEV_SET_*/NDEV_MODE_*/NDEV_CAP_*/NDEV_FLAG_*）
- NDEV_RS 回复处理（主循环分发中的 `IS_NDEV_RS` 分支）
- 协议契约：与 16-stage-drivers/03-netdriver-framework（驱动面）对称
- Rust: `os/net/lwip`（NDEV 客户端）+ `os/libs/minix-netdriver`（协议类型，ARCH N-4）

## 边界

- **前置依赖**: 03、`../16-stage-drivers/03-netdriver-framework.md`
- **本篇不覆盖**: NDEV 驱动面协议（16）；具体网卡驱动实现（16 的 22/23）。
- **讲述结构**: 见 `plan.md` §3.1
