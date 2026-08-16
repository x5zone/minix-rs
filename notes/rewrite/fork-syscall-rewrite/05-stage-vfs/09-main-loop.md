# 09-main-loop: 主循环与消息分发

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 — 并发基础：运行时心脏
> **源码**: `main.c:54-192,263-302,580-663,921-973`、`table.c`（call_vec）、`glo.h`
> **Rust 模块**: `os/servers/vfs/src/main_loop.rs`（VfsState/run）、`os/servers/vfs/src/call_table.rs`（CallTable/VfsCallNum）
> **draft 素材**: `draft/10-main-loop.md` 主循环部分（素材）

## 核心点

- 主循环 5 路分发：FS transid 回复 / PM / notify（DS/KERNEL/CLOCK）/ 负 endpoint task / 设备 reply（BDEV/CDEV/SDEV）/ 正常 syscall
- get_work/do_work/do_reply/handle_work 骨架
- call_vec 64 调用（table.c:17-81），Rust 侧 VfsCallNum 枚举（call_table.rs:28）——ARCH A-2
- reply/replycode、err_code、SUSPEND 语义（A-5 契约入口）
- transid 路由：IS_VFS_FS_TRANSID → worker_get → do_reply
- VfsState 聚合（main_loop.rs:75）——ARCH A-4

## 边界

- service_pm 各请求内容不覆盖（10）
- FS 请求队列机制不覆盖（11）
- bdev/cdev/sdev reply 内部不覆盖（20~22）
