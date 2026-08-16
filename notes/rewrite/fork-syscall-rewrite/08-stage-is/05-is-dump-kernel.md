# 05-is-dump-kernel: 内核数据域转储（dmp_kernel.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 转储域（hooks 表第一个语义域）
> **源码**: `minix3/minix/servers/is/dmp_kernel.c`（396 行）
> **Rust 模块**: `dump_kernel.rs`
> **draft 素材**: `draft/tmp_dmp_kernel.c.md`（逐行素材）

## 核心点

- 8 个转储函数：`proctab_dmp`（319/349，PROCLOOP 分页 + PRINTRTS）、`procstack_dmp`（359，+ stacktrace）、`privileges_dmp`（253，priv 表 + ipc_to/k_call_mask）、`image_dmp`（169）、`irqtab_dmp`（122，IRQ hooks + actids）、`kmessages_dmp`（63，kerninfo 环形缓冲，A-3）、`monparams_dmp`（94）、`kenv_dmp`（192）
- helper：`s_flags_str`（218）、`s_traps_str`（236）、`p_rts_flags_str`（300）、`proc_name`（386）
- PROCLOOP/PRINTRTS 宏与 BEG/END_PROC_ADDR 分页语义、全局表 `proc[]`/`priv[]`/`image[]`（55-57）
- kernel 布局 ABI（A-4）：`struct proc`/`priv`/`boot_image`/`kinfo`/`machine`/`kmessages`
- 22 行分页 + `--more--` + prev 游标（A-5）

## 边界

- **前置依赖**: 04 + kernel 布局头文件
- **不覆盖（移交）**: PM/VFS/RS/DS/VM 数据面（06~10）
