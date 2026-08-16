# 99-global-concepts: VFS 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段收尾）
> **源码**: `const.h`、`glo.h`、`type.h`、`fs.h`、`proto.h`、`utility.c:142-186`（sys_datacopy_wrapper）、minix 外部头
> **Rust 模块**: `minix-types`、`os/servers/vfs/src/call_table.rs` 常量
> **draft 素材**: `draft/09-globals-const.md` + `draft/99-global-concepts.md`（素材）

## 核心点

- 常量表：NR_FILPS/NR_VNODES/NR_MNTS/NR_WTHREADS/NR_LOCKS/NR_SOCKDEVS、FP_BLOCKED_ON_*、SYMLOOP、CTTY_ENDPT
- 全局状态：fp/susp_count/reviving/sending/verbose/m_in/self/workers/err_code/bsf_lock
- 引用计数模型：filp_count / v_ref_count / v_fs_count 双层不变量（draft/99 素材）
- endpoint/transid 术语、who_p/who_e/call_nr 宏
- sys_datacopy_wrapper：跨文档数据拷贝工具
- 64 位类型映射（A-8）、LOCK_DEBUG cfg（A-9）

## 边界

- 一切机制细节不覆盖（01~31）
