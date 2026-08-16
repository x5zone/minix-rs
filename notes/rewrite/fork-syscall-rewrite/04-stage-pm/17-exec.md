# 17: exec

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 8 exec
> **源码**: minix3/minix/servers/pm/exec.c（38/62/130/156）
> **Rust 模块**: exec.rs（stub）
> **draft 素材**: 无

## 核心点

do_exec（转发 VFS_PM_EXEC + SUSPEND）、do_newexec（setuid/TAINTED 判定、mp_name/frame 保存、PARTIAL_EXEC）、exec_restart（成功：catch 重置/sigact 复位、tracer SIGTRAP/SIGSTOP、sys_exec；失败：PARTIAL_EXEC → SIGKILL）、do_execrestart（RS 专用）

## 边界

- **前置依赖**: 05/15/11
- **不覆盖（移交）**: VFS 可执行加载/解释器（05-stage-vfs）、信号重置接收方语义（12）、VM 内存重映射（02-stage-vm）
