# 09-random-driver — 随机数驱动 random

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

/dev/random + /dev/urandom、熵池、AES 后端（rijndael）、阻塞语义。C: drivers/system/random/（main.c/random.c/aes/）。Rust: os/drivers/system/random。

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: 内核随机性（01-stage-kernel）；密码学库选型。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
