# 24-文本类游戏

> **状态**: pending（最小骨架，待改写）
> **定位**: 文本类游戏
> **源码**: `minix3/games/{adventure,monop,fortune,fish,wargames,wtf,random}/`
> **Rust 模块**: `os/commands/games/{adventure,monop,fortune,fish,wargames,wtf,random}`
> **draft 素材**: 无（新建）

## 核心点

- - 7 个文本游戏行为契约：adventure/monop（大文本冒险/大富翁）/fortune/fish（格言）/wargames/wtf/random
- - 文本数据文件面：adventure/monop/fortune 的数据文件格式与体积
- - [ARCH] A-10 重大决策：语义重写 vs 数据文件迁移（文本数据归 Rust assets）

## 边界

- - **前置依赖**: 06/10
- - **不覆盖（移交）**: 数据文件格式决策（本 stage）

