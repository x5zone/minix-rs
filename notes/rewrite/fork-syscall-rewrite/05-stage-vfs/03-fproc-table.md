# 03-fproc-table: fproc 表与 endpoint 验证

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 — 进程模型：表与查找
> **源码**: `utility.c:94-127`（isokendpt_f/okendpt）、`fproc.h:117-124`（fproc_light）、`glo.h`
> **Rust 模块**: `os/servers/vfs/src/fproc.rs`（FProcTable）
> **draft 素材**: `draft/09-globals-const.md` 部分（素材）

## 核心点

- fproc[NR_PROCS] 表：槽位与内核进程表同下标、endpoint 提取槽位
- PID_FREE=0 槽空闲语义；VFS_PM_INIT 填槽（调用点在 01）
- okendpt/isokendpt_f：endpoint → 槽位验证，`fproc_addr`/`who_p` 宏
- fproc_light：MIB 服务只读轻量表（A-7，缺口标注）
- `FProcTable::get/get_mut/find_by_endpoint`（Rust 已实现）

## 边界

- 结构字段细节不覆盖（02）
- 槽位使用方：PM fork/exit（10）、fd 操作（14）
