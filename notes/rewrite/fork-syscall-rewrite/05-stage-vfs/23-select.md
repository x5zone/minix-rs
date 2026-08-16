# 23-select: select 多路复用

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 — 多路复用
> **源码**: `select.c` 全文件
> **Rust 模块**: （未实现）select 模块
> **draft 素材**: 无（新建）

## 核心点

- do_select：fd 集合拷贝（copy_fdsets）→ 状态过滤（select_filter）→ 分型请求
- 四类请求：select_request_char/sock/file/pipe
- tab2ops/ops2tab：SEL_RD/WR/ERR 位图
- 阻塞管理：select_cancel_all/cancel_filp/select_return/select_unsuspend_by_endpt
- 回复路径：select_callback/select_*_reply1/2（cdev/sdev 两路）
- 定时器：select_timeout_check/set_timer、CLOCK notify → expire_timers（main.c:112）
- filp select 字段（selectors/ops/flags）与 FSF_* 标志

## 边界

- socket 系统调用不覆盖（24）
- pipe 阻塞机制不覆盖（17）
