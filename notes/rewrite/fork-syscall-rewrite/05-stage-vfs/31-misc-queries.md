# 31-misc-queries: 杂项服务与查询

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 14 — 控制与杂项
> **源码**: `misc.c:52-116,276-576,989-1006`、`time.c` 全文件、`gcov.c` 全文件
> **Rust 模块**: （未实现）misc 模块
> **draft 素材**: 无（新建）

## 核心点

- do_sync/do_fsync：同步与刷盘
- do_getsysinfo/do_svrctl/do_getrusage：信息与查询
- do_vm_call/dupvm：VM 交互面（交叉 02-stage-vm）
- do_utimens：时间戳更新
- do_gcov_flush：覆盖率刷新
- panic_hook：panic 处理

## 边界

- fcntl 与锁不覆盖（30）
- pm_* 协议不覆盖（10）
