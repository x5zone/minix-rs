# 14-mib-subtree-vm-hw: CTL_VM 与 CTL_HW 子树

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 子系统子树（init 顺序第二/三）
> **源码**: `vm.c` 全部 + `hw.c` 全部
> **Rust 模块**: `subtree/vm.rs`、`subtree/hw.rs`
> **draft 素材**: 无（新建）

## 核心点

- CTL_VM：loadavg（loadinfo 历史槽计算 1/5/15 分钟）、uvmexp2（vm_info_stats → uvmexp_sysctl：pagesize/pagemask/pageshift/npages/free/filepages + unused1=largest 扩展）、maxslp/uspace
- CTL_HW：machine/machine_arch（编译期常量）、ncpu（CONFIG_MAX_CPUS）、byteorder、pagesize、physmem/usermem（vm_info_stats/usage，int/quad 双版本）、ncpuonline（sys_getmachine）
- 未实现槽位（A-9）：VM_METER/UVMEXP/NKMEMPAGES/ANONMIN 等；HW_MODEL/DISKNAMES/IOSTATS 等
- 依赖（A-12）：`vm_info_stats`/`vm_info_usage`（libvmclient → VM server，交叉引用 `02-stage-vm`）

## 边界

- **前置依赖**: 03/06 + VM `26`
- **不覆盖（移交）**: kern 子树（13）、minix 子树（15）
