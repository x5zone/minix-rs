# 10-stage-mib — MIB 文档目录

> **状态**: 骨架就绪（plan.md 定稿 2026-08-16；各 doc 为最小骨架，待按 plan.md 改写）
> **主线**: MIB server 启动顺序（mib_init 子树注册 → tree_init → remote_init → 主循环 dispatch）；`sysctl(2)` 调用旅程为次主线
> **Ground truth**: `minix3/minix/servers/mib/`（8 个 .c，4990 行）

## 文档清单（24 篇）

| 编号 | 文档 | 语义模块 |
|------|------|---------|
| 00 | `00-mib-overview.md` | 总览：MIB 是什么、启动主线图、sysctl 次主线、导航 |
| 01 | `01-mib-init-main.md` | main()/SEF 注册、主循环骨架、mib_sysctl 消息解码、ENOMEM 语义 |
| 02 | `02-mib-message-contract.md` | 协议面：call numbers、6 种消息结构、sysctlnode/sysctldesc 交换格式（A-1/A-4） |
| 03 | `03-mib-node-model.md` | mib_node/mib_dynode 结构、4 类节点矩阵、标志系统、计数（A-2） |
| 04 | `04-mib-static-tree-init.md` | 静态树（7 顶层节点 + MIB_* 宏）、mib_init 回调、tree_init |
| 05 | `05-mib-tree-lookup.md` | mib_find：静态 O(1) + 动态有序链表 O(n) |
| 06 | `06-mib-copy-io.md` | 拷贝/长度/relay 原语（copyout/copyin/copyin_str/relay grants） |
| 07 | `07-mib-auth-model.md` | mib_authed 权限模型（PRIVATE/ANYWRITE/READWRITE/PERMANENT） |
| 08 | `08-mib-dynamic-nodes.md` | 动态节点生命周期：create/scan/add/remove/destroy/版本递增（A-3） |
| 09 | `09-mib-data-access.md` | 数据节点读写：getptr/read/write/readwrite + verify + bool 消毒 |
| 10 | `10-mib-dispatch.md` | 分发与名字解析、元标识符、ENOTDIR/EISDIR、远程 ERESTART 续走、**sysctl 次主线路径图** |
| 11 | `11-mib-query-describe.md` | CTL_QUERY/CTL_DESCRIBE 枚举与描述、节点序列化 |
| 12 | `12-mib-remote-subtrees.md` | 远程子树：endpts 表、mount/unmount、COMMON_MIB_* relay、死亡检测、**远程次主线** |
| 13 | `13-mib-subtree-kern.md` | CTL_KERN 子树（KERN_* 全集 + 函数/verify/数据节点） |
| 14 | `14-mib-subtree-vm-hw.md` | CTL_VM + CTL_HW 子树（loadavg/uvmexp2/physmem/...） |
| 15 | `15-mib-subtree-minix.md` | CTL_MINIX 子树（test 测试子树 + mib 统计 + proc 表） |
| 16 | `16-mib-proc-tables.md` | 进程表快照：三表拉取 + 节流/闩锁 + PID 哈希（A-6） |
| 17 | `17-mib-proc-lwp.md` | KERN_LWP：lwp 状态机 + wchan 6 类编码 |
| 18 | `18-mib-proc2.md` | KERN_PROC2：req 过滤面 + kinfo_proc2 填充 |
| 19 | `19-mib-proc-args.md` | KERN_PROC_ARGS：ps_strings + 页游走 + 截断语义 |
| 20 | `20-mib-minix-proc.md` | MINIX_PROC list/data（ProcFS 布局 ABI，A-5） |
| 21 | `21-mib-client-libc.md` | libc sysctl(3) 系列 + CTL_USER + __sysctl（外部契约 A-10） |
| 22 | `22-mib-rmib-client.md` | RMIB 客户端（libsys/rmib.c）：注册/注销/请求处理（A-8） |
| 99 | `99-mib-global-concepts.md` | 常量/错误码/跨服务引用收口 |

## 关键文件

- `plan.md` — 文档重组计划（定稿，含覆盖契约 §5 + ARCH 清单 §4 + 双轮 review 记录 §7）
- `draft/` — 旧占位 README（素材）
- `checklist.md` — 函数级基线（实现期创建，参照 02-stage-vm/checklist.md 模式）

## 启动链路位置

```
kernel → VM → RS → PM/SCHED/VFS/DS/MIB → IS/DEVMAN/INPUT/IPC → INIT（boot 终点）
```

MIB 在 boot_image 中（`kernel/table.c:60`，紧跟 TTY、先于 VM），是 sysctl(2) 的系统服务；init(8) 启动期即调用 sysctl。
