# 17-mib-proc-lwp: KERN_LWP 进程线程信息

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 进程信息（LWP）
> **源码**: `proc.c:224-595`
> **Rust 模块**: `proc/lwp.rs`
> **draft 素材**: 无（新建）

## 核心点

- `get_lwp_stat` 状态机：ZOMB/DEAD/STOP/RUN/SLEEP 分类 + 睡眠附加信息
- wchan 6 类编码（低 8 位）：0x00 内核任务/0x01 RTS/0x02 PM/0x03 VFS/0x04 MIB/0xff 进程；上 8 位类内信息；wmesg 文本生成
- `fill_lwp_common/kern/user`：l_lid=endpoint、swtime/slptime、优先级、cpu、rtime、pctcpu（cpuavg）
- `mib_kern_lwp`：pid/elsz/elmax 语义（pid<0 全列表、pid=0 内核任务、pid>0 单进程）、EXTRA_PROCS 预留、ESRCH
- A-4：`kinfo_lwp` 布局（ps(1) 消费者）

## 边界

- **前置依赖**: 16
- **不覆盖（移交）**: 其他 PROC 接口（18~20）
