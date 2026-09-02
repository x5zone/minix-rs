# 09-编辑器面

> **状态**: pending（最小骨架，待改写）
> **定位**: 编辑器面
> **源码**: `minix3/bin/ed/、minix/usr.bin/mined/`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - ed：行编辑器语义（地址/命令/缓冲/正则；bin/ed 为 Minix3 安装面）
- - mined：Minix 全屏编辑器（minix/usr.bin/mined）
- - [ARCH] 编辑器选型决策：minix3 无 vi（无 usr.bin/vi），mined 为系统编辑器；minix-rs 是否补 vi 面
- - 与 08 共享正则语法；与 05 行编辑面分离

## 边界

- - **前置依赖**: 08
- - **不覆盖（移交）**: shell 行编辑（05）、终端控制（13）

