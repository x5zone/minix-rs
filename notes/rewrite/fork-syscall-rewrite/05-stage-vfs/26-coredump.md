# 26-coredump: core dump 生成

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 12 — 进程执行与退出
> **源码**: `coredump.c` 全文件、`misc.c:903-988`（pm_dumpcore）
> **Rust 模块**: （未实现）coredump 模块
> **draft 素材**: 无（新建）

## 核心点

- write_elf_core_file：core 文件骨架（ELF header/program headers/notes/segments）
- get_memory_regions：进程内存区域枚举（与 VM 交互）
- fill_*_header/adjust_offsets/write_buf：ELF 布局与写入
- dump_notes/dump_elf_header/dump_program_headers/dump_segments
- pm_dumpcore 调用点（misc.c:903，入口在 10）

## 边界

- 信号语义不覆盖（`../04-stage-pm/11~13`）
