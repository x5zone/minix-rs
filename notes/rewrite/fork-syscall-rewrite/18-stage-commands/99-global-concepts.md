# 99-全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念
> **源码**: `minix3/minix/commands/DESCRIBE/（构建面）`
> **Rust 模块**: `os/commands/*`
> **draft 素材**: 无（新建）

## 核心点

- - 命令安装面：bin/sbin/usr.bin/usr.sbin 分层 + PATH 语义
- - [ARCH] A-1：用户态静态链接（ld.elf_so 不实现，ldd 语义变化）
- - [ARCH] A-4：命令参数框架（自研 argparse + --help/usage 约定）
- - [ARCH] A-5：退出码/errno 约定（Result + errno 映射，POSIX 退出码保持）
- - 命令契约表模板（§3.6）：选项/输入输出/退出码/错误面/Rust 模块
- - /etc 配置约定 + DESCRIBE 构建面 + curses/terminfo 决策汇总（A-2）

## 边界

- - **前置依赖**: 全部
- - **不覆盖（移交）**: 一切机制细节（01~24）

