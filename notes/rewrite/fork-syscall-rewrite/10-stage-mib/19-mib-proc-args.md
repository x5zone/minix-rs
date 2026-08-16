# 19-mib-proc-args: KERN_PROC_ARGS 参数读取

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 进程信息（PROC_ARGS）
> **源码**: `proc.c:918-1176`
> **Rust 模块**: `proc/proc_args.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_kern_proc_args`：pid/req 两参数；ARGV/ENV/NARGV/NENV 四模式
- ps_strings 读取：`mp_frame_addr + mp_frame_len - sizeof(pss)` 定位；max=roundup(min(frame_len, ARG_MAX), PAGE_SIZE)
- 页游走算法：vector 页 + 字符串页双缓冲、`copybudget=(ARG_MAX/PAGE_SIZE)*2` 防 rogue、memchr NUL 分段
- 截断语义：本调用返回截断后长度而非 ENOMEM（libkvm 依赖，与其他 sysctl 不同）
- setproctitle 场景（ps_strings 指针指向 frame 外数据，≤2048B）说明

## 边界

- **前置依赖**: 16 + VM/内核拷贝能力
- **不覆盖（移交）**: PROC2（18）
