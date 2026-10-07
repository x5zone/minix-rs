# 迁移映射表（Phase 1 产出 · 后续所有动作的唯一合同）

- 机器可读正本：[`path-map.tsv`](path-map.tsv)（2428 行，`old_path → new_path → kind → rule`）
- 生成器：`tmp/pre-migrate-snapshot-20261007/gen-path-map.py`（一次性工具，映射规则集中在一个文件里，便于审阅方逐条核）
- 合同地位：Phase 2 的移动、Phase 3 的引用重写、Phase 6 的验收，三方都只认这份表。
  执行中发现漏项，走「改生成器规则 → 重跑 → 重新核对覆盖率」的流程，不手改 TSV。

## 一、覆盖率自检（生成器内置，三项全过才落盘）

| 检查 | 判据 | 结果 |
|---|---|---|
| 覆盖率 | 基线清单 2428 行，每行都在映射表中出现且只出现一次 | 未映射条目 **0** |
| 无碰撞 | 目标路径去重后数量 == 源路径数量 | **2428 = 2428**，无两个旧路径撞同一新路径 |
| 旧树清空 | 没有任何条目的目标仍以 `notes/` 开头 | 残留 **0** |
| 与基线闭合 | `wc -l path-map.tsv` − 表头 == `wc -l manifest.notes.disk.tsv` − 表头 | **2428 = 2428** ✓ |

## 二、映射规则（19 条，按用途分三组）

### 2.1 目录级前缀规则（机械平移，stage 编号一律不动）

| 规则 | 旧前缀 | 新前缀 | 条目数 |
|---|---|---|---|
| R5 | `notes/rewrite/fork-syscall-rewrite/{00-master-plan … 19-stage-integration}/` | `rewrite-notes/{同名}/` | 1814 |
| R4 | `notes/rewrite/fork-syscall-rewrite/evidence/` | `rewrite-notes/evidence/` | 276 |
| R8 | `notes/rewrite/concepts/` | `rewrite-notes/concepts/` | 6 |
| R9 | `notes/rewrite/archive_bak/` | `rewrite-notes/archive/legacy-fork-bak/` | 40 |
| R17 | `notes/study/` | `study-notes/`（内部主题子目录不动） | 184 |

### 2.2 需要判断的归属（文件级例外）

| 规则 | 对象 | 去向 | 条目数 | 判断依据 |
|---|---|---|---|---|
| R7 | fork 树根散落文件 | `rewrite-notes/coordination/` | 73 | 会话过程产物集中一个窝，停写范围好定义（见第四节分组） |
| R6 | `fork-syscall-rewrite/README.md` | `rewrite-notes/misc/legacy-fork-syscall-index.md` | 1 | **六份方案里 HY4 与 glm 主张把它挪成 `00-master-plan/README.md`，实测该目标已存在**（16160 字节的《总规划说明》），挪过去会撞名；deepseek 的改名归档方案是唯一可执行的一支。它内容也确已过时（列的是 `05-mock-strategy.md`、`阶段1：PM层` 这类旧主线） |
| R7′ | `fork-syscall-rewrite/misc_concepts.md` | `rewrite-notes/misc/misc_concepts.md` | 1 | 179 条知识点登记（292023 字节，被 38 处引用），是知识条目而非会话产物，不进 coordination |
| R12 | `notes/rewrite/` 根部其余 12 篇 | `rewrite-notes/misc/{同名}` | 12 | 迁移期不改文件名，保持映射可机械对账 |
| R10 | `notes/rewrite/README.md` | `rewrite-notes/README.md` | 1 | 索引跟着阶段走，Phase 7 重写内容 |
| R11 | `notes/rewrite/RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/RECONSTRUCTION-PRINCIPLES.md` | 1 | 方法论宪法文档留树根，作一级入口 |
| R18 | `notes/README.md` | `rewrite-notes/misc/legacy-notes-root-readme.md` | 1 | 全文只有一句「草稿文件夹」，伞目录退役后归档 |
| R19 | `notes/TODO.md` | `rewrite-notes/misc/legacy-notes-todo.md` | 1 | 192 行的文件拆分待办，其有效结论已被本映射表吸收（拆分口径 = R12/R13/R15） |
| R3 | `fork-syscall-rewrite/.review/`（3 件，被忽略） | `.review/archive/notes-fork-syscall-rewrite-2026-09/` | 3 | 评审产物回到单一父目录；内容冻结不改写 |

### 2.3 跨树迁移（内容换区）

| 规则 | 对象 | 去向 | 条目数 |
|---|---|---|---|
| R1 | `fork-syscall-rewrite/20-redesign/` 两篇 | `redesign-notes/architecture/` | 2 |
| R13 | `notes/redesign/redesign.md` | `redesign-notes/README.md` | 1 |
| R15 | `notes/redesign/` 其余 11 篇 | 按主题分桶（下表） | 11 |

`redesign-notes/` 主题分桶明细：

| 主题目录 | 文件 |
|---|---|
| `architecture/`（8） | architecture-changes.md、improve_minix.md、improve_minix.md.backup、improve_minix_refactored.md、improve_minix_refactored.md.backup、microkernel-cohesion-design.md、microkernel-closure-design.md、semantic-modules.md |
| `ipc/`（2） | endpoint_redesign.md、ipc-improve.md |
| `fork/`（1） | fork-redesign.md |
| `vm/`（0） | 本轮不放入文件，留作未来分支落点。git 不跟踪空目录，Phase 7 写一个 `vm/README.md` 存根说明这是预留方向 |

两个 `.backup` 与正本 `cmp` 逐字节相同（实测通过），本轮只随树移动不合并——合并是内容删改，属迁移后的独立任务。

## 三、按 git 状态分的移动方式（决定用 `git mv` 还是 `mv`）

| kind | 条目数 | 方式 | 原因 |
|---|---|---|---|
| `git-mv` | 1102 | `git mv` | 已跟踪，保历史与 `git log --follow` |
| `mv-ignored` | 1325 | 普通 `mv` | 被 `.gitignore` 忽略，git 不感知；移动后继续保持忽略态（`.design` 1162、`*.log` 155 及其余） |
| `mv-untracked` | 1 | 普通 `mv` | 未跟踪未忽略的 `PENDING-DECISIONS-3ARCH-PARITY.md`，按裁决不代为提交 |

`git mv` 的嵌套陷阱（deepseek R4 提出，本次照防）：`git mv A B` 在 `B` 已存在时会把 `A` 塞成 `B/A`。
执行时**不预先 `mkdir` 目标目录**，改为「先移动要成为目录本身的路径，再移动其中的文件」，
Phase 2 的移动脚本按目录整体移动，移动后立即做结构核对（顶层目录数与预期相等 + 无二级嵌套）。

## 四、`coordination/` 里 73 个文件的分组（供快速审阅）

| 组 | 数量 | 例子 |
|---|---|---|
| 开场与接续提示词 | 16 | `NK4C-接续PROMPT-20261006r.md`、`NK4A-QWEN-OPENING-PROMPT.md`、`claim-prompt.md`、`NK4C-NEW-MACHINE-OPENING-PROMPT.txt` |
| 滚动工作记录 | 4 | `NK4C-WORKLOG.md`（2625867 字节）、`NK4B-WORKLOG.md`、`NK4A-QWEN-WORKLOG.md` |
| 交接件 | 8 | `HANDOFF-NK4A-boot-first-light.md`、`MISC-CONCEPTS-SESSION-HANDOFF-20260927.md`、`nk4c-glm-riscv-session-handoff-20261002.md` |
| 评审报告与审计表 | 14 | `NK4C-REVIEW-REPORT-R34-20261006.md`、`NK4C-R34-P1-AUDIT-TABLE-20261006.txt`、`NK4-REGRESSION-REVIEW-20260922.md` |
| 待办台账 | 6 | `NK4A-TODO.md`、`NK4B-TODO.md`、`edge_todo.md`、`edge_todo_archive.md` |
| 并行编排 | 8 | `edge1-4.md`、`new_edge1-4.md` |
| 多模型任务书 | 5 | `new_todo_{deepseek,glm,muse,qwen,HY4}.md` |
| 缺陷案卷 | 7 | `NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`、`NK4C-BUG-RISCV64-TRANSIENT-PTE.md`、`riscv瞬态页表崩溃取证方法论.md`、`trap-boundary-message-materialization.md` |
| 换机迁移记录 | 1 | `NK4C-MIGRATION-20260930.md` |
| 专项台账与登记 | 4 | `PATTERN-SCAN-REPORT-20260923.md`、`ADDRESS-CONSTANT-AUDIT.md`、`STRUCTURAL-DEBT-REGISTER-20260930.md`、`NK4C-QEMU-ENVIRONMENTS.md` |
| 三架构对齐两件套 | 2 | `TODO-3ARCH-PARITY-20261006.md`（用户在制，只搬路径）、`PENDING-DECISIONS-3ARCH-PARITY.md`（未跟踪，只搬路径） |

案卷与专项台账这 13 件是**过程产物还是技术文档**的边界最模糊：它们被文档互引（4–21 处），
其中 4 件还被 `os/` 代码注释当锚点引用（`TODO-3ARCH-PARITY`、`NK4C-BUG-RISCV64-TRANSIENT-PTE`、
`NK4C-OPENING-PROMPT`、`ADDRESS-CONSTANT-AUDIT`，各 1 处）。
把它们单列成 `rewrite-notes/casefiles/` 也是一种收法，见第五节待裁决项。

## 五、待用户确认的三个判断（不影响其余 2425 条）

| # | 判断 | 本表当前取的值 | 备选 |
|---|---|---|---|
| J1 | `coordination/` 这个目录名 | `coordination/`（deepseek 用名） | glm 用 `workflow/`、MiMo 用 `worklog/`、HY4 用 `_worklog/`、muse 用 `99-handoff-archive/` |
| J2 | 第四节里那 13 件案卷与专项台账 | 一并放 `coordination/`，一条规则 | 单开 `rewrite-notes/casefiles/`，多一条规则、语义更准 |
| J3 | `misc_concepts.md` 的去处 | `rewrite-notes/misc/` | `rewrite-notes/coordination/`（与其余散文件同规则）或 `rewrite-notes/concepts/`（与概念词条同区） |

## 六、引用重写合同（Phase 3 用，派生自本表）

改写只允许两种形态，禁止裸串全局 sed（MiMo 的纪律，防把「讨论旧路径长什么样」的句子也改掉）：

1. **路径前缀替换**：按本表 `rule` 分组的 `旧前缀 → 新前缀`，最长前缀优先（防 `notes/rewrite/` 先吃掉 `notes/rewrite/fork-syscall-rewrite/` 留下半截残留）。
2. **Markdown 相对链接重定基**：按基线里的 16 个跨目录逃逸链接逐个手改，保留 `#L78-L96` 这类锚点尾巴。

必改域：`rewrite-notes/`、`redesign-notes/`、`study-notes/` 三棵树内部、`os/` 注释（91 文件）、
`tools/`（脚本与两处注释）、`prompt/` 源 + `.claude/` `.codex/` `.trae/` 派生（派生走 `tools/generate-derived-skills.sh` 重新生成，不手改）、
`CLAUDE.md`、`AGENTS.md`、根 `README.md`。

冻结域（允许保留旧路径，验收门按白名单排除）：
`.review/`（86879 处，含新迁入的归档 3 件）、`migrate_notes_plan/`（6185 处，六份方案与本次留存文档描述的就是旧布局）、
`tmp/`（49 处）、`AI-chats/`（10 处）、`new_laptop_migrate/`（71 处）、`.qoder/`（6 处）、
`.trae/documents/`、`.zcode/`、`.codebuddy/`、`.claude/settings.local.json`。

预期残留（不是漏改）：`rewrite-notes/MIGRATION.md` 的新旧对照表、本映射表 TSV 本身、
`redesign-notes/README.md` 里对退役伞目录的说明句。

## 七、新树终态（对照 `00-SNAPSHOT.md` 第四节）

```
rewrite-notes/                     2227 件
  README.md                        Phase 7 重写为新索引
  RECONSTRUCTION-PRINCIPLES.md
  MIGRATION.md                     Phase 7 新写（旧→新对照 + 冻结区清单 + module 语义变更说明）
  00-master-plan/ … 19-stage-integration/     1814 件（含各自 .design/ 1162 件随父移动）
  concepts/                          6 件
  misc/                             16 件（12 篇根级杂项 + 旧 fork 索引 + misc_concepts + 伞目录 README/TODO 归档）
  coordination/                     73 件
  evidence/                        276 件（1.2G 串口日志，含 615MB 那个）
  archive/legacy-fork-bak/          40 件
redesign-notes/                     14 件 + README.md（由 redesign.md 升级）+ vm/ 存根
study-notes/                        184 件（主题子目录原样）
.review/archive/notes-fork-syscall-rewrite-2026-09/   3 件（搭车归档，git 不跟踪）
book/                                不动
minix3/                              不动（ground truth）
```

`notes/` 伞目录在 Phase 2 结束后应为空并删除（含空目录清理 `find notes -type d -empty -delete`）。
