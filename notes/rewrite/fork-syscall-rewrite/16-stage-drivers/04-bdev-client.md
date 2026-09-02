# 04-bdev-client — 块设备客户端库 libbdev

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

bdev_open/close/read/write/gather/scatter/ioctl、异步面（bdev_*_asyn + bdev_wait_asyn + 回调）、minor 映射、bdev_driver 绑定。C: lib/libbdev/（bdev.c/call.c/driver.c/ipc.c/minor.c）。Rust: minix-bdev。

## 边界

- **前置依赖**: 02（块协议驱动侧）
- **本篇不覆盖**: VFS/FS 消费逻辑（05-stage-vfs/15-stage-fs）；驱动侧（02）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
