# 23-终端控制游戏

> **状态**: pending（最小骨架，待改写）
> **定位**: 终端控制游戏
> **源码**: `minix3/games/{worm,worms,rain,colorbars,tetris,snake,rogue}/`
> **Rust 模块**: `os/commands/games/{worm,worms,rain,colorbars,tetris,snake,rogue}`
> **draft 素材**: 无（新建）

## 核心点

- - 7 个终端控制游戏行为契约：worm/worms/rain/colorbars（动画）/tetris/snake/rogue（游戏逻辑）
- - curses/terminfo 依赖面：6 个 Makefile 链接 -lterminfo（worm/worms/tetris/rogue/colorbars/rain）
- - [ARCH] A-2 关联：termios/转义序列 vs terminfo 移植决策决定本类实现方式
- - 依赖面：需 13（termios）落地后实装

## 边界

- - **前置依赖**: 13（termios/curses）
- - **不覆盖（移交）**: curses 库实现（13/99 决策）

