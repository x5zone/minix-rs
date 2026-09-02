# 06-tty-driver — 终端驱动 tty（boot image）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

boot 终端（kernel/table.c:59）、tty_table、console（arch/i386/console.c）+ rs232 + 键盘（keyboard.c/keymaps）、termios、行规则、suspend/select、/dev/log 重定向（tty.c:270）。C: drivers/tty/tty/ 全部。Rust: os/drivers/tty/tty。

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: 键盘事件协议（12-stage-input）；PTY（07）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
