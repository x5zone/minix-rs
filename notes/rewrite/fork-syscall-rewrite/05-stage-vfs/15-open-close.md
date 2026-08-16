# 15-open-close: open/close/lseek

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 — 文件描述符与文件 I/O
> **源码**: `open.c` 全文件
> **Rust 模块**: （未实现）open 模块
> **draft 素材**: 无（新建）

## 核心点

- do_open/do_creat/common_open：路径解析 → new_node → 设备/管道分派
- new_node：REQ_LOOKUP/REQ_CREATE 与 vnode 建立
- pipe_open：管道打开路径（与 17 交叉）
- do_mknod/do_mkdir：节点创建
- do_close/do_lseek/actual_lseek：定位与关闭
- mode_map：oflags → R_BIT/W_BIT

## 边界

- 路径解析不覆盖（13）
- 设备打开细节不覆盖（19~22）
