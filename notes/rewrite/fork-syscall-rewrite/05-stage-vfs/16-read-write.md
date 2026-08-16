# 16-read-write: read/write 数据通路

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 — 文件描述符与文件 I/O
> **源码**: `read.c` 全文件、`write.c` 全文件、`glo.h`（bsf_lock）
> **Rust 模块**: （未实现）read/write 模块
> **draft 素材**: 无（新建）

## 核心点

- do_read/do_write/do_read_write_peek/read_write：常规文件读写主链
- bsf 锁（lock_bsf/unlock_bsf/check_bsf_lock）：块特殊文件全局串行化
- do_getdents：目录项读取
- rw_pipe：管道读写路径（与 17 交叉）
- 跨 FS 读写：req_readwrite/req_breadwrite（12）

## 边界

- pipe 阻塞语义不覆盖（17）
- 驱动收发细节不覆盖（20~22）
