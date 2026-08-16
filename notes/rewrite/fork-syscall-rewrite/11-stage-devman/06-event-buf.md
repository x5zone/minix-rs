# 06-devm-event-buf: 事件队列与输出缓冲

> **状态**: pending（最小骨架，待改写）
> **定位**: events 文件 + read 路径（阶段 3 读写机制）
> **源码**: `minix3/minix/servers/devman/buf.c`（129 行全部）+ `device.c:142-183`（事件/静态信息读取）+ `device.c:75-136`（事件生产）
> **Rust 模块**: `buf.rs`、`event_queue.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- buf.c 四函数：buf_init/printf/append/result（skip/offset 语义，BUF_SIZE-1 上限，vsnprintf 长度处理）
- `devman_event_read`（device.c:142-168）：TAILQ_LAST 取最老事件、read 全部（r==0）后移除并 free（EOF 消费语义，A-8）
- `devman_static_info_read`（:173-183）："%s\n"
- 事件格式：`"ADD <path> 0x%08x"` / `"REMOVE <path> 0x%08x"`（ADD_STRING/REMOVE_STRING，DEVMAN_STRING_LEN-11 路径预算）
- 事件生产方（:75-136 add/remove_event）与 devmand 消费格式的对称性（13）
- ARCH A-8：TAILQ → VecDeque，EOF 语义保持

## 边界

- **前置依赖**: 03（event 结构）+ 02（read 路径）
- **不覆盖（移交）**: 事件生产触发点（07/08）、devmand 消费（13）
