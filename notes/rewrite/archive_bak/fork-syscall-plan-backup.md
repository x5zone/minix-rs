# Fork 系统调用纵向切片重构计划

> **目标**: 将 Minix3 的 fork 系统调用从 C 逐步重构为 Rust
> **策略**: 小步快跑，每次 100-300 行，逻辑与基建同步推进
> **范围**: PM (Process Manager) + 必要的内核支持

---

## 文档索引

本文档已拆分为三个部分，便于维护和查阅：

| 部分 | 文件 | 内容 |
|------|------|------|
| **Part 1** | [fork-syscall-plan-part1.md](fork-syscall-plan-part1.md) | 总体架构 + 阶段 1~3（MProc、do_fork 前半、PID 生成器） |
| **Part 2** | [fork-syscall-plan-part2.md](fork-syscall-plan-part2.md) | 阶段 4~10（do_fork 后半、VM/VFS、exit/wait/srv_fork、集成测试） |
| **Part 3** | [fork-syscall-plan-part3.md](fork-syscall-plan-part3.md) | 附录（源码对应、常量参考、技术决策、待修复项） |

---

## 当前进度

| 阶段 | 内容 | 状态 |
|------|------|------|
| **1** | MProc 结构体与进程表 | ✅ 完成 |
| **2** | do_fork 前半部分 | ✅ 完成 |
| **3** | PID 生成器 | ❌ 待实现 |
| **4** | do_fork 后半部分 | ❌ 待实现 |
| **5** | VM Fork (Mock) | ❌ 待实现 |
| **6** | VFS 通知与 SUSPEND | ❌ 待实现 |
| **7** | do_exit 实现 | ❌ 待实现 |
| **8** | do_wait4 实现 | ❌ 待实现 |
| **9** | do_srv_fork 实现 | ❌ 待实现 |
| **10** | 集成测试 | ❌ 待实现 |

---

## 关键待修复项

> 详见 [Part 3 附录 E](fork-syscall-plan-part3.md#附录-e待修复项清单)

| 编号 | 项目 | 优先级 |
|------|------|--------|
| E-1 | `LAST_FEW` 值不一致（C=2, Rust=5） | 高 |
| E-3 | 缺少 `NR_PIDS` / `INIT_PID` 常量 | 高 |
| E-6 | `fork_from` 中 `id.index` 应为 child_index | 高 |
| E-9 | 缺少 `VFS_CALL` 标志映射 | 高 |
| E-10 | `generate_child_pid` 无冲突检测 | 高 |
