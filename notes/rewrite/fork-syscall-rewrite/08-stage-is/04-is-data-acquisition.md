# 04-is-data-acquisition: 数据获取机制（5 条通道）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 数据面机制（所有转储域篇的前置）
> **源码**: `com.h:252,316-331,412-415,476,507,729-734`、`sysinfo.h:11-17`、`callnr.h`（PM_BASE+47/VFS_BASE+48）、`libsys/getsysinfo.c`、`libsys/vm_info.c`、`libsys/sys_diagctl.c`、`syslib.h:166-168,187`
> **Rust 模块**: `minix-sys`（客户端）、`minix-types`
> **draft 素材**: 无（机制面无旧素材）

## 核心点

- 通道 1：kernel 系统调用 `sys_getinfo(GET_*)`（com.h:316-331：KINFO/IMAGE/PROCTAB/MONPARAMS/IRQHOOKS/PRIVTAB/MACHINE/IRQACTIDS）——消息格式见 kernel `25-misc-unported`
- 通道 2：kernel 系统调用 `sys_diagctl_stacktrace`（DIAGCTL_CODE_STACKTRACE，com.h:413；kernel 侧接线 forward reference：`32-stack-tracing` 仍 ENOSYS）
- 通道 3：kernel 映射 `get_minix_kerninfo()->kmessages`（type.h:214-232）——**A-3 usermapped 移除**（需新增 GET_KMESSAGES 或等价机制）
- 通道 4：跨服务 IPC `getsysinfo(who, SI_*, ...)`（libsys/getsysinfo.c）：PM/VFS/RS/DS callnr 映射 + 服务侧 size 精确匹配 + `sys_datacopy` + root 检查（A-4）
- 通道 5：VM IPC `vm_info_stats/usage/region`（libsys/vm_info.c）：VM_INFO + VMIW_STATS/USAGE/REGION（com.h:729,732-734）
- 错误面：各调用失败 → 告警 + return（不 panic）

## 边界

- **前置依赖**: 01 + kernel `25-misc-unported`/`28-usermapped-data`/`32-stack-tracing` + 各服务阶段
- **不覆盖（移交）**: 各服务器布局解释（05~10）、输出格式化与分页
