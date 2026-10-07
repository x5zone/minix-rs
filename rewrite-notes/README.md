# rewrite-notes — 重写文档区

> **创建**: 2026-10-07（目录迁移当天）。本目录曾位于 notes 伞目录下、并比现在多一层已废弃的模块层目录名；
> 逐条路径对照与工具参数变更见 [MIGRATION.md](MIGRATION.md)。

这里存放 Minix3 各模块用 Rust 重写的正式设计文档：一份 C 源概念对应一篇文档，
按服务的启动执行顺序编号成 20 个阶段目录。每篇文档都有配套的评审流水线、
符号锚点校验与覆盖率穷举，是本仓库的一等交付物（不是草稿）。

## 阶段目录一览

编号就是阅读顺序，也是系统真实的启动因果顺序。权威说明（每个服务为什么排在这个位置、
谁把它的映像装进内存）在 [00-master-plan/README.md](00-master-plan/README.md)。

| 编号 | 目录 | 对应服务 | 正式文档数 |
|---|---|---|---|
| 00 | [00-master-plan/](00-master-plan/) | — 顶层规划与决策记录 | 15 |
| 01 | [01-stage-kernel/](01-stage-kernel/) | Kernel | 38 |
| 02 | [02-stage-vm/](02-stage-vm/) | VM | 28 |
| 03 | [03-stage-rs/](03-stage-rs/) | RS | 22 |
| 04 | [04-stage-pm/](04-stage-pm/) | PM | 22 |
| 05 | [05-stage-vfs/](05-stage-vfs/) | VFS | 33 |
| 06 | [06-stage-sched/](06-stage-sched/) | SCHED | 16 |
| 07 | [07-stage-ds/](07-stage-ds/) | DS | 14 |
| 08 | [08-stage-is/](08-stage-is/) | IS | 12 |
| 09 | [09-stage-init/](09-stage-init/) | INIT | 16 |
| 10 | [10-stage-mib/](10-stage-mib/) | MIB | 24 |
| 11 | [11-stage-devman/](11-stage-devman/) | DEVMAN | 15 |
| 12 | [12-stage-input/](12-stage-input/) | INPUT | 16 |
| 13 | [13-stage-ipc/](13-stage-ipc/) | IPC | 12 |
| 14 | [14-stage-runtime/](14-stage-runtime/) | RUNTIME | 15 |
| 15 | [15-stage-fs/](15-stage-fs/) | FS（mfs/pfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs） | 26 |
| 16 | [16-stage-drivers/](16-stage-drivers/) | DRIVERS | 27 |
| 17 | [17-stage-net/](17-stage-net/) | NET（lwip + uds） | 27 |
| 18 | [18-stage-commands/](18-stage-commands/) | COMMANDS | 26 |
| 19 | [19-stage-integration/](19-stage-integration/) | — 跨服务集成与端到端测试 | 1 |

阶段目录内部还可能有三类子目录，它们随所属阶段存放：

- `draft/` — 尚未定稿的稿子，评审时不作为交付物。
- `archive/` — 该阶段已被取代的历史稿。
- `.design/` — 评审流程用的可复用设计快照（中间产物，正式文档不引用，已被 `.gitignore` 排除）。

## 其他子区

| 目录 | 是什么 | 数量 |
|---|---|---|
| [concepts/](concepts/) | 跨阶段的概念词条：capability、endpoint、fail-stop、typestate、内核内替换的风险 | 6 |
| [misc/](misc/) | 不属于单一阶段的重写工程文档：项目规划、结构设计、纵向切片策略、语义冻结策略、内核不变量、异步消息表、SENDREC 原子性、ELF 加载、架构机制映射、现代硬件与 Rust，以及伞目录退役时归档进来的两份说明 | 16 |
| [coordination/](coordination/) | 多会话协作的过程产物：开场与接续提示词、滚动工作记录、交接件、评审报告与审计表、待办台账、并行编排、缺陷案卷。只读为主，新内容不要写在这里 | 73 |
| [tmp/evidence/](tmp/evidence/) | 按日期与任务命名的实验取证（串口日志、设备树、脚本输出）。审计证据，不做改写 | 276 |
| [archive/legacy-fork-bak/](archive/legacy-fork-bak/) | 以 fork 系统调用为主线时期的备份稿 | 40 |

取证产物为什么放在 `tmp/` 而不是本树：仓库的落盘规则要求运行日志、二进制镜像、评审与工具运行证据一律进 `tmp/` 下按功能划分的子目录，见 [../prompt/agents-workflow-optim.md](../prompt/agents-workflow-optim.md)。**代价要认**：这批文件不再被 Git 跟踪，换机器或误删不可从版本库找回；迁移前的全量副本在压缩卷里（见 [MIGRATION.md](MIGRATION.md) 的回捞一节），而压缩卷本身也在 `tmp/` 内，所以它同样只有一份磁盘拷贝。
| [MIGRATION.md](MIGRATION.md) | 迁移前后的路径对照、评审工具参数变更、已知悬空引用、旧内容回捞方法 | — |

## 从零开始的阅读路线

1. [misc/rewrite.md](misc/rewrite.md) — 重写目标与设计原则的总述。
2. [RECONSTRUCTION-PRINCIPLES.md](RECONSTRUCTION-PRINCIPLES.md) — 重构原则（什么可以改、什么必须保持外部行为）。
3. [misc/project-plan.md](misc/project-plan.md) 与 [misc/vertical-slice-strategy.md](misc/vertical-slice-strategy.md) — 怎么切、为什么按纵向切片推进。
4. [misc/rewrite-strategy.md](misc/rewrite-strategy.md) — 语义冻结的做法与验证清单。
5. [00-master-plan/README.md](00-master-plan/README.md) — 启动顺序与目录对应关系，然后按编号进入各阶段。

## 目录之外

- `os/` — Rust 实现的源码；每篇阶段文档的锚点指向那里的具体符号。
- `minix3/` — 原始 Minix3 C 源码，是唯一的事实基准，不修改。
- `redesign-notes/` — 再设计探索区，在重写定稿后启用。
- `study-notes/` — 早期学习笔记，由 AI 生成、未经 C 源码校验，不能当事实依据引用。
- `book/` — 电子书成品区，内容定稿后从本目录抽取。
