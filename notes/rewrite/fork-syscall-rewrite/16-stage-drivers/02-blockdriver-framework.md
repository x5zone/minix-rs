# 02-blockdriver-framework — 块驱动框架 libblockdriver

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

blockdriver_task 主循环（driver.c 单线程 / driver_mt.c 多线程 / driver_st.c）、BDEV_* 协议（com.h:963-976）、bdr_* 回调表、分区/几何（drvlib.c partition() + partition.h）、live update、trace、mq。C: lib/libblockdriver/ 全部。Rust: minix-blockdriver。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: 具体存储驱动（05/15~17）；libbdev 客户端（04）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
