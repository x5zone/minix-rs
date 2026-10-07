# 笔记目录迁移对照表（notes 伞布局 → 仓库根三棵树）

> **创建**: 2026-10-07（迁移完成当天）。本页是永久保留的旧→新查找入口：
> 任何会话、工具或历史产物里拿着旧路径，都能在这里查到文件现在的位置。
> 机器可读的全量映射（2428 条逐文件）在 `migrate_notes_plan/pre-migrate-20261007/path-map.tsv`（已入 git）。

## 一、这次迁移解决了什么

旧布局把重写文档写成四节路径 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md`，
其中 `fork-syscall-rewrite` 这一层名字承载的组织思想（以 fork 系统调用为主线）早已被放弃
——实际内容按服务的启动执行顺序排列（见 `00-master-plan/README.md` 的主线变更说明）。
这一层既加深了路径，又要求每个新读者先补一段历史课。

`notes/` 这个伞目录又把三类读者完全不同的东西放在一个屋檐下：活跃的重写文档、未来的再设计探索、
早期的 AI 学习笔记。伞目录本身不承载任何工具语义（没有任何脚本把 `notes/` 当整体寻址）。

迁移后的三棵树与 `book/` 并列在仓库根，规范路径两节：

```
rewrite-notes/       活跃重写文档（有评审流水线、锚点校验、覆盖率工具）
redesign-notes/      再设计探索（rewrite 定稿后启用；当前内容是前期存量）
study-notes/         早期 Minix3 学习笔记（AI 生成，未经 C 源码校验，不作事实基线）
book/                mdBook 成品区（本次未触碰；未来只从 rewrite-notes 与 redesign-notes 抽取）
```

## 二、目录级对照（旧 → 新）

| 旧位置 | 新位置 | 说明 |
|---|---|---|
| `notes/rewrite/fork-syscall-rewrite/00-master-plan/` | `rewrite-notes/00-master-plan/` | 阶段目录名与编号一律未改 |
| `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/` … `19-stage-integration/` | `rewrite-notes/01-stage-kernel/` … `19-stage-integration/` | 同上 |
| `notes/rewrite/fork-syscall-rewrite/evidence/` | `rewrite-notes/evidence/` | 取证日志，内容冻结未改写 |
| `notes/rewrite/fork-syscall-rewrite/20-redesign/` | `redesign-notes/architecture/` | 两篇换区：内容属再设计探索 |
| `notes/rewrite/concepts/` | `rewrite-notes/concepts/` | 跨阶段概念词条 |
| `notes/rewrite/archive_bak/` | `rewrite-notes/archive/legacy-fork-bak/` | fork 时代备份，原样保留 |
| `notes/rewrite/{12 篇杂项}.md` | `rewrite-notes/misc/` | 文件名一律未改 |
| `notes/rewrite/README.md` | `rewrite-notes/README.md` | 内容已按新布局重写 |
| `notes/rewrite/RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/RECONSTRUCTION-PRINCIPLES.md` | 方法论宪法文档留在树根 |
| `notes/redesign/redesign.md` | `redesign-notes/README.md` | 目录说明升级为分区入口 |
| `notes/redesign/{endpoint_redesign,ipc-improve}.md` | `redesign-notes/ipc/` | 按主题分桶 |
| `notes/redesign/fork-redesign.md` | `redesign-notes/fork/` | 同上 |
| `notes/redesign/{architecture-changes,improve_minix,improve_minix_refactored,microkernel-cohesion-design,microkernel-closure-design,semantic-modules}.md` | `redesign-notes/architecture/` | 同上 |
| `notes/redesign/*.md.backup` | `redesign-notes/architecture/` | 两个备份与正本逐字节相同，本次只随树移动不合并 |
| `notes/study/` | `study-notes/` | 主题子目录原样未动 |
| `notes/README.md` | `rewrite-notes/misc/legacy-notes-root-readme.md` | 伞目录退役归档 |
| `notes/TODO.md` | `rewrite-notes/misc/legacy-notes-todo.md` | 文件拆分待办归档；其有效结论已体现在本次映射表 |
| `notes/rewrite/fork-syscall-rewrite/README.md` | `rewrite-notes/misc/legacy-fork-syscall-index.md` | 旧主线索引改名归档（`00-master-plan/README.md` 已有现行总规划说明，两个名字不能撞） |
| `notes/rewrite/fork-syscall-rewrite/.review/` | `.review/archive/notes-fork-syscall-rewrite-2026-09/` | 评审产物回到单一父目录 |

## 三、fork 树根那 75 个散落文件的去处

旧树根散落的会话过程文件（六份迁移计划写作时是 27～29 个，NK4C 战役推进到 75 个）
统一收进 `rewrite-notes/coordination/`，共 73 个；另两个有独立去处：

- `README.md` → `rewrite-notes/misc/legacy-fork-syscall-index.md`（改名归档，理由见上表）
- `misc_concepts.md` → `rewrite-notes/misc/misc_concepts.md`（知识条目登记，不是会话产物）

`coordination/` 内部按来源分组（不改文件名，逐文件映射见 path-map.tsv）：
开场与接续提示词 16 个、滚动工作记录 4 个、交接件 8 个、评审报告与审计表 14 个、
待办台账 6 个、并行编排 `edge*` 8 个、多模型任务书 5 个、缺陷案卷 7 个、
换机迁移记录 1 个、专项台账 4 个、三架构对齐两件套 2 个。

## 四、评审工作流的路径模型变更（工具与规则都跟着改了）

module 层退役后，评审状态目录的分组键由 `{module}` 换成 `{stage}`：

| 项 | 旧 | 新 |
|---|---|---|
| 文档路径 | `notes/rewrite/{module}/{stage}/{doc}.md` | `{tree}/{stage}/{doc}.md` |
| 状态目录 | `.review/{tool}/{module}/` | `.review/{tool}/{stage}/` |
| 设计快照 | `notes/rewrite/{module}/{stage}/.design/` | `{tree}/{stage}/.design/` |
| 覆盖率预检参数 | `tools/design-coverage-check.sh fork-syscall-rewrite` | `tools/design-coverage-check.sh 01-stage-kernel`（或树别名 `rewrite`） |
| 门控检查参数 | `tools/review-gate-check.sh <tool> <module> <doc-stem>` | `tools/review-gate-check.sh <tool> <stage> <doc-stem>` |
| 初始化器 | `tools/review-init.sh <tool> notes/rewrite/…/<stage>/<doc>.md` | `tools/review-init.sh <tool> <stage>/<doc>.md`（可省树前缀，脚本按三棵树探测） |

`{tree}` 的取值集中在 `tools/notes-layout.conf` 一个文件里，下次再改目录只动那一处。

**`{stage}` 与覆盖率脚本的 `--module` 仍然是两个概念**（这条约束没变）：
前者是阶段目录名（如 `01-stage-kernel`），后者是 Minix3 模块名（`kernel`、`vm`、`pm`、`vfs`、`rs`、`ds`、`inet`、`fs`）。

历史状态目录不搬迁：`.review/` 整体是 gitignore 的历史产物，
本次迁移按计划**冻结**（不批量改写其中旧路径），新 review 由 `tools/review-init.sh` 按新模型建新目录。
`03-stage-kernel` 这类旧阶段编号目录名是重排前的历史键名，同样保持原样。

## 五、阶段编号的重排史（旧编号 → 现行编号）

阶段目录曾在 fork 主线与启动顺序主线之间重排过一次。**本次迁移没有改任何阶段编号**，
但迁移前的文档里仍散留着旧编号引用（按「迁移不顺手修内容」的纪律原样保留，属独立后续任务）：

计数口径：三棵树全量（含 `evidence/`、`.design/`、`legacy-fork-bak/` 三个冻结区），按**出现次数**计，
复算命令 `grep -rIo '<旧编号>' rewrite-notes redesign-notes study-notes \| wc -l`。
本表原抄自迁移计划写作时的测量，口径未标注且已随 NK4C 战役的文档增长过期，现按实测更正。

| 旧编号引用 | 现行编号 | 出现次数 | 命中文件数 |
|---|---|---|---|
| `01-stage-pm` | `04-stage-pm` | 28 | 7 |
| `03-stage-kernel` | `01-stage-kernel` | 76 | 38 |
| `04-stage-vfs` | `05-stage-vfs` | 7 | 5 |
| `05-stage-sched` | `06-stage-sched` | 7 | 4 |
| `deep-analysis/`（目录已不存在） | 内容在 `rewrite-notes/archive/legacy-fork-bak/fork-all-layers-deep-analysis.md` | 11 | 4 |

## 六、已知悬空引用（迁移前后同样悬空，非本次造成）

| 位置 | 引用 | 情况 |
|---|---|---|
| `os/libs/minix-types/README.md` | `rewrite-notes/fork-syscall-plan.md` | 该文档在迁移前就不存在；`rewrite-notes/archive/legacy-fork-bak/` 里有 `fork-syscall-plan-part1.md`、`-part3.md`、`-backup.md` 等分片。现行等价文档建议判定为 `rewrite-notes/00-master-plan/01-project-overview.md`，**待人裁决**后单笔修正 |
| 各 stage 文档 | `../01-stage-pm/…`、`../deep-analysis/…`、`../review-rules/review.md` | 旧编号与旧目录，见第五节 |
| 迁移前既有断链 | 三棵树部分 425 行（全域 452 行，另含 `prompt/` 13、`.trae/` 12、`migrate_notes_plan/` 2） | 迁移后三棵树 373 行（全域 400 行）。三种口径下**迁移引入的新增都是 0**：原始行 425→373、去重键 288→257、独立复扫与 `broken-links.after.txt` 逐行相等。基线见 `migrate_notes_plan/pre-migrate-20261007/broken-links.before.txt`，对账方法见执行日志 Phase 3 一节 |

| `rewrite-notes/04-stage-pm/draft/mproc-design.md:2654` | `rewrite-notes/mp-flags-analysis.md` | 迁移前该引用就指向不存在的路径（真实文件在 `archive/legacy-fork-bak/`），机械前缀替换如实保留了悬空，坏度不变 |
| `rewrite-notes/18-stage-commands/doc_rerank_deepseek.md:2184` | `rewrite-notes/edge*.md` 通配 | 迁移前的通配能命中 `fork-syscall-rewrite/edge_todo.md`，迁移后该文件在 `coordination/` 下，通配落空。这是机械替换对「散文里的通配符」的语义极限（不是链接，链接门管不到） |

散文里的陈旧命令写法（`tools/design-coverage-check.sh fork-syscall-rewrite …` 一类）在三棵树内还有若干处，
绝大多数是历史台账对既往验收轮次的回溯记录（按裁决保留原文）；
面向未来的指引句已就地改为新参数（`rewrite-notes/09-stage-init/plan.md`、`prompt/todo_plan.md`、
`prompt/skill/review-coverage-skill.md` 各一处，连同 `{rw-module}` 一并收敛）。

## 七、旧内容如何回捞

- 迁移前全貌：annotated tag `notes/pre-migrate-20261007`（指向提交 `bb8a90e05`）
- 被 `.gitignore` 排除、tag 保护不到的内容（`.design` 快照、`*.log` 取证日志、`.bak`/`.backup`）：
  `tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz`（sha256 记在
  `migrate_notes_plan/pre-migrate-20261007/snapshot.sha256`）
- 逐文件旧→新映射：`migrate_notes_plan/pre-migrate-20261007/path-map.tsv`
- 单条路径回查：`git show notes/pre-migrate-20261007:<旧路径>`；
  或直接读该文件的 git 历史：`git log --follow -- <新路径>`（改名以 rename 记录入 git，历史可续读）
- 迁移的决策与逐动作记录：`migrate_notes_plan/EXECUTION-LOG.md`、`migrate_notes_plan/pre-migrate-20261007/00-SNAPSHOT.md`

## 八、本次迁移刻意没有做的事

1. 没有改任何阶段编号、文件名（除第三节说明的两个改名归档项）。
2. 没有删除任何内容文件（含 `study-notes/` 的 184 个早期笔记、`legacy-fork-bak/` 的 40 个备份、
   `redesign-notes/architecture/` 里那两个与正本逐字节相同的 `.backup`）——删除与精简是独立任务。
3. 没有改写 `.review/`、`tmp/`、`AI-chats/`、`new_laptop_migrate/`、`.qoder/`、`migrate_notes_plan/`
   里的旧路径字符串：它们是历史产物或历史叙述，保留原文才是证据。
4. 没有触碰 `book/` 与 `minix3/`。
5. 没有顺手修既有断链与旧阶段编号引用（见第六节）。
