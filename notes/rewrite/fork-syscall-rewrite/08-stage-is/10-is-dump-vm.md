# 10-is-dump-vm: VM 数据域转储（dmp_vm.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域
> **源码**: `minix3/minix/servers/is/dmp_vm.c`（157 行）
> **Rust 模块**: `dump_vm.rs`
> **draft 素材**: `draft/tmp_dmp_vm.c.md`（逐行素材）

## 核心点

- `vm_dmp`（55）：`vm_info_stats` 首屏 → `sys_getproctab` → 逐进程 `vm_info_usage` + `vm_info_region` 批处理
- `print_region`（11）：连续相同 region 折叠 + PROT_READ/WRITE/EXEC 保护位显示
- 批处理状态机：prev_i/prev_base 游标、首屏 header、LINES=24 边界、`--more--`/`IS: internal error` 分支
- VM_INFO 协议（A-4）：`vm_stats_info`/`vm_usage_info`/`vm_region_info` 布局（`../02-stage-vm/26-vm-queries.md`）
- 错误处理：vm_info 失败 → 告警 + continue

## 边界

- **前置依赖**: 04 + VM_INFO 协议（`02-stage-vm` `26-vm-queries`）
- **不覆盖（移交）**: VM 服务器内部语义（`02-stage-vm`）
