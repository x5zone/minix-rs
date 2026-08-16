# 27-link: 链接/重命名/截断

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 13 — 目录/链接/权限
> **源码**: `link.c` 全文件
> **Rust 模块**: （未实现）link 模块
> **draft 素材**: 无（新建）

## 核心点

- do_link/do_unlink：硬链接创建与删除
- do_rename：改名/移动
- do_truncate/do_ftruncate：截断（文件/路径两入口）
- do_slink/do_rdlink：符号链接创建与读取

## 边界

- 路径解析不覆盖（13）
