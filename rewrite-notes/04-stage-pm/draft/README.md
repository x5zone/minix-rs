# Stage 1: PM (Process Manager) 层实现

> 本目录包含 PM 层 fork 实现的详细技术文档。
>
> - **中层指南**: [../00-master-plan/06-phase1-pm-guide.md](../00-master-plan/06-phase1-pm-guide.md)
> - **顶层路线图**: [../00-master-plan/04-implementation-roadmap.md](../00-master-plan/04-implementation-roadmap.md)

## 文档索引

| 文档 | 内容 | 对应阶段 |
|------|------|---------|
| [mproc-design.md](./mproc-design.md) | MProc 结构体设计、C源码分析、Rust实现 | 阶段1 |
| [pid-generator.md](./pid-generator.md) | PID生成器算法、C源码分析、实现代码 | 阶段3 |
| [do-fork-impl.md](./do-fork-impl.md) | do_fork实现、字段复制规则、测试用例 | 阶段2、4 |
| [pm-call-vm-fork.md](./pm-call-vm-fork.md) | PM调用VM fork（PM侧IPC）| 阶段5 |
| [pm-call-vfs-fork.md](./pm-call-vfs-fork.md) | PM调用VFS fork（PM侧IPC、SUSPEND机制）| 阶段6 |
| [exit-impl.md](./exit-impl.md) | do_exit实现、进程退出流程、僵尸处理 | 阶段7 |
| [wait-impl.md](./wait-impl.md) | do_wait4实现、pidarg解析、子进程回收 | 阶段8 |
| [srv-fork-impl.md](./srv-fork-impl.md) | do_srv_fork实现、RS专用fork | 阶段9 |
| [integration-test.md](./integration-test.md) | 集成测试场景、测试用例设计 | 阶段10 |

## 快速导航

### 阶段1: MProc 结构体与进程表基础
→ 查看 [mproc-design.md](./mproc-design.md)
- C 源码分析（mproc.h）
- 四份进程表对比
- Rust 结构体设计

### 阶段2: do_fork 核心逻辑（上）
→ 查看 [do-fork-impl.md](./do-fork-impl.md)
- C 源码分析（forkexit.c 前半部分）
- 参数检查与槽位分配

### 阶段3: PID 生成器
→ 查看 [pid-generator.md](./pid-generator.md)
- C 源码分析（utility.c）
- 冲突检测算法
- 循环复用机制

### 阶段4: do_fork 核心逻辑（下）
→ 查看 [do-fork-impl.md](./do-fork-impl.md)
- C 源码分析（forkexit.c 后半部分）
- 进程结构复制与初始化

## Minix3 源码参考

| 文件 | 位置 | 内容 |
|------|------|------|
| `mproc.h` | 第 1-100 行 | mproc 结构体定义 |
| `forkexit.c` | 第 47-145 行 | do_fork 函数实现 |
| `utility.c` | 第 32-52 行 | get_free_pid 函数 |

## 实现检查清单

- [ ] mproc-design.md 完成
- [ ] pid-generator.md 完成
- [ ] do-fork-impl.md 完成
- [ ] 所有文档包含 C 源码分析
- [ ] 所有文档包含 Rust 实现代码
- [ ] 所有文档包含测试用例
