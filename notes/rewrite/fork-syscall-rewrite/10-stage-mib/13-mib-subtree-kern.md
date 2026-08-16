# 13-mib-subtree-kern: CTL_KERN 子树

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 子系统子树（init 顺序第一）
> **源码**: `minix3/minix/servers/mib/kern.c` 全部
> **Rust 模块**: `subtree/kern.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_kern_table` 结构：KERN_* 全集（KERN_MAXID=85），大量槽位未实现（A-9 排除契约：KERN_PROC/KERN_FILE/KERN_MBUF 等）
- 函数节点：clockrate（sys_hz 派生 clockinfo）、hardclock_ticks（getticks）、root_device（`svrctl(PMGETPARAM)` rootdevname）、ccpu（cpuavg）、cp_time（`sys_getcputicks` 单 CPU/求和/数组三态）、consdev（makedev TTY）、drivers（VFS `SI_DMAP_TAB` + "pts" 兼容 hack）、boottime（getuptime）、ipc_info（mock，被 IPC 服务远程覆盖）
- verify 节点：securelvl（只升不降 mock）、forkfsleep（0..MAXSLP*1000）
- 数据节点：ostype/osrelease/version/maxproc/maxfiles/argmax/hostname/hostid/maxptys/maxphys/monotonic_clock 等
- 依赖原语（A-12）：sys_hz/getticks/sys_getcputicks/getsysinfo/svrctl/cpuavg

## 边界

- **前置依赖**: 03/06/07/09 + 10
- **不覆盖（移交）**: 其余子树（14/15）、进程信息（16~20）
