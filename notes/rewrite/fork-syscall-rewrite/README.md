# Fork系统调用重构文档索引

---

## 总规划类
| 文档 | 内容说明 |
|------|----------|
| `00-master-plan/01-project-overview.md` | 整体架构、设计决策、路线图 |
| `00-master-plan/02-architecture-analysis.md` | 架构分析 |
| `00-master-plan/03-minix3-source-audit.md` | Minix3源码审计 |
| `00-master-plan/04-implementation-roadmap.md` | 顶层：整体路线图 |
| `00-master-plan/05-mock-strategy.md` | Mock策略 |

## 阶段指南类
| 阶段 | 文档 | 内容说明 |
|------|------|----------|
| 阶段1：PM层 | `00-master-plan/06-phase1-pm-guide.md` | PM层实现、代码、测试 |
| 阶段2：VM层 | `00-master-plan/07-phase2-vm-guide.md` | VM CoW机制、实现细节 |
| 阶段3：Kernel层 | `00-master-plan/08-phase3-kernel-guide.md` | Kernel PCB克隆、endpoint算法 |
| 阶段4：VFS层 | `00-master-plan/09-phase4-vfs-guide.md` | VFS文件描述符复制、引用计数 |
| 阶段5：状态机 | `00-master-plan/10-phase5-statemachine-guide.md` | 完整状态机、跨服务协调 |

## 参考文档类
| 文档 | 内容说明 |
|------|----------|
| `00-master-plan/11-decision-records.md` | 技术决策记录 |
| `00-master-plan/12-constants-reference.md` | 常量参考 |
| `00-master-plan/13-source-mapping.md` | 源码文件映射 |
| `00-master-plan/14-verification-checklist.md` | 验证检查清单 |
| `00-master-plan/15-todo-fixes.md` | 待修复项清单 |

## 深度分析类
| 文档 | 内容说明 |
|------|----------|
| `deep-analysis/fork-all-layers-deep-analysis.md` | 四层架构深度分析 |
| `01-stage-pm/mproc-design.md` | PM mp_flags状态位完整分析、状态转换图 |

---

## 文档层级结构

```
00-master-plan/          # 顶层规划
├── 01-project-overview.md
├── 02-architecture-analysis.md
├── 03-minix3-source-audit.md
├── 04-implementation-roadmap.md
├── 05-mock-strategy.md
├── 06-phase1-pm-guide.md
├── 07-phase2-vm-guide.md
├── 08-phase3-kernel-guide.md
├── 09-phase4-vfs-guide.md
├── 10-phase5-statemachine-guide.md
├── 11-decision-records.md
├── 12-constants-reference.md
├── 13-source-mapping.md
├── 14-verification-checklist.md
└── 15-todo-fixes.md

01-stage-pm/             # 底层：PM实现细节
├── README.md
├── mproc-design.md
├── pid-generator.md
├── do-fork-impl.md
└── ...

02-stage-vm/             # 底层：VM实现细节
├── README.md
├── vm-fork-impl.md
└── ...
```

## 内容映射规则

### 1. 顶层路线图 (04-implementation-roadmap.md)
**适合内容**：阶段名称和编号、阶段目标、涉及的服务/模块、依赖关系、里程碑定义

### 2. 中层阶段指南 (06-phase1-pm-guide.md)
**适合内容**：阶段目标详细说明、关键任务列表、接口定义、数据结构概览、验收标准

### 3. 底层实现细节 (01-stage-pm/*.md)
**适合内容**：C源码完整分析、Minix3源码对照、逐行实现代码、详细测试用例

---

## 项目进度

| 阶段 | 内容 | 状态 |
|------|------|------|
| 1 | MProc 结构体与进程表 | ✅ 完成 |
| 2 | do_fork 前半部分 | ✅ 完成 |
| 3 | PID 生成器 | ✅ 完成 |
| 4 | do_fork 后半部分 | ⏳ 待实现 |
| 5 | VM Fork | ⏳ 待实现 |
| 6 | VFS 通知与 SUSPEND | ⏳ 待实现 |
| 7 | do_exit 实现 | ⏳ 待实现 |
| 8 | do_wait4 实现 | ⏳ 待实现 |
| 9 | do_srv_fork 实现 | ⏳ 待实现 |
| 10 | 集成测试 | ⏳ 待实现 |

## 已知问题

| 编号 | 项目 | 优先级 |
|------|------|--------|
| E-1 | `LAST_FEW` 值不一致（C=2, Rust=5） | 高 |
| E-3 | 缺少 `NR_PIDS` / `INIT_PID` 常量 | 高 |
| E-6 | `fork_from` 中 `id.index` 应为 child_index | 高 |
| E-9 | 缺少 `VFS_CALL` 标志映射 | 高 |
| E-10 | `generate_child_pid` 无冲突检测 | 高 |

## 代码实现状态

| 模块 | 实现程度 | 关键缺失 |
|------|----------|----------|
| **minix-types** | 90% | 缺少具体系统调用消息类型常量 |
| **minix-ipc** | 30% | SyscallNum 已定义，send/receive 是 todo!() |
| **PM/mproc** | 80% | Process 结构体、进程表、fork 核心逻辑已实现 |
| **PM 顶层** | 5% | fork/exec/exit/signal/wait 入口全是 todo!() |
| **VM 服务器** | 0% | 完全空壳，无 vmproc 结构体 |
| **VFS 服务器** | 0% | 完全空壳，无 fproc 结构体 |
| **Kernel** | 5% | KProcess 只有 endpoint，其余全是空壳 |

---

### 文件完整性验证
```
总文件数：41个
总行数：21473行
```
