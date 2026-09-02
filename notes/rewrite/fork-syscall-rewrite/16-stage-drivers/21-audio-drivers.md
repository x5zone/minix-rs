# 21-audio-drivers — 音频驱动（libaudiodriver + 7 声卡）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

audio 请求协议、libaudiodriver（audio_fw.c/liveupdate.c）、es1370/es1371 参考 + AC97 + 其余变体差异矩阵。C: lib/libaudiodriver/ + drivers/audio/（7 个）。Rust: os/drivers/audio/*。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: 声音系统上层（18-stage-commands）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
