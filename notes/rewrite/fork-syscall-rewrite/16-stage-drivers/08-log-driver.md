# 08-log-driver — 日志驱动 log

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

/dev/log 字符设备、诊断捕获（diag.c）、live update、RS 运行时加载（service up /service/log）。C: drivers/system/log/（log.c/diag.c/liveupdate.c）。Rust: os/drivers/system/log。

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: syslog 命令（18-stage-commands）；内核诊断输出（01-stage-kernel）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
