# 18-lwip-bpfdev — /dev/bpf 与 BPF 过滤器

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `bpfdev.c`（1365 行）：/dev/bpf 字符设备（`bpfdev_tab`，bpfdev.c:1346）
- chardriver 消费侧：CDEV_CLONED（bpfdev_get_minor）、read/write/ioctl、select（chardriver_reply_select）、task 回复
- 包缓冲：BSD 模型（每 BPF 设备单用户进程假设，不支持并发调用）
- `bpf_filter_ext`（bpf_filter.c:149，561 行）：NetBSD bpf_filter 用户态移植（mbuf→pbuf、无 BPF context、内存 store 访问校验）
- 主循环分发：VFS CDEV/BDEV 请求 → `bpfdev_process`
- Rust: `os/net/lwip`（BPF 模块）+ 消费 `minix-chardriver`（16 框架，ARCH N-9）

## 边界

- **前置依赖**: 02（select）、`../16-stage-drivers/01-chardriver-framework.md`
- **本篇不覆盖**: 网卡驱动（16）；BPF 用户态语义（18-stage-commands）。
- **讲述结构**: 见 `plan.md` §3.1
