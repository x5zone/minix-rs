# 03-is-dump-dispatch: 转储分派（hooks 表与次主线）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 转储分派（主循环 notify 入口，次主线路径图）
> **源码**: `minix3/minix/servers/is/dmp.c`（132 行，`dmp.c:17-131`）
> **Rust 模块**: `dispatch.rs`
> **draft 素材**: `draft/tmp_dmp.c.md`（逐行素材）

## 核心点

- hooks 表（dmp.c:17-36）：16 项 key→function→name（F1/F3/F4/F5/F6/F7/F8/F10/SF1~SF6/SF8/SF9）
- `do_fkey_pressed`（dmp.c:73）：`fkey_events` 拉取位图 → `pressed` 宏双位图匹配 → 逐项调用 dump 函数
- `pressed` 宏（dmp.c:70-72）：F1-F12 与 SF1-SF12 位偏移编码
- `key_name`（dmp.c:103）：键名格式化（" F%d"/"Shift+F%d"）
- `mapping_dmp`（dmp.c:120）：打印键映射表（无分页）
- **次主线路径图**：F-key 按压 → TTY 通知 → do_fkey_pressed → 数据面 → 转储域（plan §1.3）
- EDONTREPLY：fkey 通知不回复

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 各 dump 函数体（05~10）、fkey 协议（02）
