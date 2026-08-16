# 25-exec: exec 全流程

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 12 — 进程执行与退出
> **源码**: `exec.c` 全文件
> **Rust 模块**: （未实现）exec 模块
> **draft 素材**: 无（新建）

## 核心点

- pm_exec：VFS_PM_EXEC 入口，栈帧保存 → 路径解析 → 加载
- Get_read_vp：可执行文件打开与 vnode 建立
- 脚本解释：is_script/patch_stack/insert_arg（#! 链）
- ELF 加载：read_seg/map_header/stack_prepare_elf
- vfs_memmap：VM_MMAP 交互（交叉 02-stage-vm/20-vm-mmap）
- clo_exec：FD_CLOEXEC 关闭

## 边界

- VM mmap 实现不覆盖（`../02-stage-vm/20-vm-mmap.md`）
- PM 侧 exec 状态机不覆盖（`../04-stage-pm/17-exec.md`）
