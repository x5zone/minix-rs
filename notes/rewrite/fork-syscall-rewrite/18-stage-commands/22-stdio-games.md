# 22-纯 stdio 游戏

> **状态**: pending（最小骨架，待改写）
> **定位**: 纯 stdio 游戏
> **源码**: `minix3/games/{factor,primes,bcd,morse,number,pig,arithmetic,caesar,banner,ppt}/`
> **Rust 模块**: `os/commands/games/{factor,primes,bcd,morse,number,pig,arithmetic,caesar,banner,ppt}`
> **draft 素材**: 无（新建）

## 核心点

- - 10 个纯 stdio 游戏行为契约：factor/primes（数论）/bcd/morse/number（数字转换）/pig/arithmetic（算术）/caesar（密码）/banner/ppt（文本图形）
- - 依赖面：仅 exec + stdio + exit（最早可验收批）
- - [ARCH] A-10 关联：语义重写 vs 数据迁移决策（本类无大文本数据）

## 边界

- - **前置依赖**: 06（stdio 面）
- - **不覆盖（移交）**: 终端控制游戏（23）

