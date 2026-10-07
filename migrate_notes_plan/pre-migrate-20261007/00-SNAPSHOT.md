# 执行前全貌记录留存（notes 目录迁移 · Phase 0 快照）

> **创建**: 采集时刻 17:15～17:20（+08:00），开窗当天

- 采集者：本次迁移的执行会话（Qoder 侧 agent）
- 文档目的：把迁移动手之前的仓库全貌固定下来，使后续独立审阅方能够判断「迁移有没有丢东西、有没有改错、有没有漏改引用」。本文所有数字都是当场跑命令得到的，每条都附产出命令，审阅方可原地复跑。
- 配套数据文件（本目录内，均已入 git）：
  - `manifest.notes.disk.tsv` — notes 树全部磁盘文件（2428 行）：路径 / 字节数 / mtime / sha256 / git 状态
  - `manifest.offtree.tsv` — notes 之外的未跟踪与被忽略文件（2530 行）：路径 / 字节数 / mtime / git 状态
  - `refs.before.counts.tsv` — 旧路径引用命中计数（1747 个文件 × 4 种模式 + 合计）
  - `broken-links.before.txt` — 迁移前既有的 Markdown 断链清单（452 处）
  - `snapshot.sha256` — 上述清单自身的 sha256，用于证明清单没有在被审之前又被改动
- 配套未入卷数据（在 `tmp/pre-migrate-snapshot-20261007/`，随 `tmp/` 整域不入库）：
  - `git-state.txt` — git 层快照原始输出
  - `refs.before.lines.txt` — 98892 行逐处命中明细（`文件:行:模式`），是计数表的底账
  - `pre-migrate-notes-full.tar.gz` — Git 盲区内容的全量压缩包（见第八节）
  - `gen-manifest.py` — 清单生成器（一次性工具，审阅方可读它确认清单是怎么算出来的）

---

## 一、迁移要解决的问题（保留原始意图）

现有文档路径 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md` 从仓库根要穿过四层目录，
其中 `fork-syscall-rewrite` 这一层名字承载的组织思想（以 fork 系统调用为主线）早已被放弃
——实际内容是按启动顺序排列的 00～19 号阶段目录。这一层既加深了路径，又要求每个新读者先补一段历史课。

同时，`notes/` 这个伞目录把三类读者完全不同的东西放在一起：重写工作文档（活跃、有评审流水线）、
再设计探索（未来向）、早期学习笔记（历史档案、未经核实）。

本次迁移把这三类拆到仓库根的 `rewrite-notes/`、`redesign-notes/`、`study-notes/`，
与已有的 `book/` 并列，并顺手删掉 `fork-syscall-rewrite` 这一层。

## 二、git 层基线

```
分支            rewrite
基线 HEAD       bb8a90e05bb850f6230633fca6894aa48a0d524c
提交时刻        2026-10-07 16:28:21 +0800
提交标题        chore(git-line): os 构建产物 145 件退跟踪（os/target_smp 122 + os/tmp 23…）
远端            origin = git@github.com:x5zone/minix-rs.git
                minix3-upstream = https://github.com/Stichting-MINIX-Research-Foundation/minix.git
与 origin/rewrite 的差  0 领先 / 0 落后（已全部推送）
本地分支        main（d9c87bf8b）、rewrite（bb8a90e05）
git worktree    仅主工作树，.wt/ 为空
```

产出命令：`git rev-parse HEAD`、`git branch -vv`、`git rev-list --left-right --count origin/rewrite...rewrite`、`git worktree list`。

### 既有 tag 与本次新增的 tag

迁移前仓库已有 9 个 tag，全部指向 minix3 上游的导入点，不是本仓库自己的里程碑：

| 既有 tag | 指向 |
|---|---|
| v3.1.3 … v3.3.0（9 个） | b015dae3e、7be1d47d2、a9d80e628、272508788、2ec255bb5、8cf623654、116fcea5b、972156d59、588a35b92 |

因此本次迁移的 tag 命名带 `notes/` 前缀，与上游版本号区隔：

```
新增 tag    notes/pre-migrate-20261007（annotated）
tag 对象     6a8f6ade6
指向提交     bb8a90e05
推送决策     不推 origin —— 审阅方就在本机，可直接读本地 tag；
             仓库历史含 1.2G 取证日志，推 tag 对远端无收益
```

产出命令：`git tag -l`、`git for-each-ref --format='%(refname:short) %(objecttype) %(objectname:short) %(*objectname:short)' refs/tags/notes`、`git tag -n99 notes/pre-migrate-20261007`。

**tag 的保护范围要说清楚**：annotated tag 只冻结 git 已跟踪的内容。
本次基线上它覆盖 notes 树里的 1102 个已跟踪文件；
剩下 1326 个磁盘文件（被 `.gitignore` 忽略的 1325 个 + 未跟踪未忽略的 1 个）不在 tag 内，
必须靠第八节的 tar 快照兜底。这是六份计划里 MiMo 与 glm 都强调过的点，本次照做。

## 三、工作树里的在制品（迁移开窗时未提交的内容）

```
 M  notes/rewrite/fork-syscall-rewrite/TODO-3ARCH-PARITY-20261006.md     已跟踪、有未提交改动
 ?? notes/rewrite/fork-syscall-rewrite/PENDING-DECISIONS-3ARCH-PARITY.md  未跟踪、新文件
```

这两件是 NK4C 主线的三架构对齐台账与待裁决清单，按用户裁决「随树搬走、不碰内容」处理：
迁移只改变它们所在目录的路径，一个字节的内容都不改，也不代用户提交。
它们的迁移前 sha256 已经记录在 `manifest.notes.disk.tsv`：

```
notes/rewrite/fork-syscall-rewrite/PENDING-DECISIONS-3ARCH-PARITY.md
      156358 字节  sha256 4cdde71c37cd65078bffb5704c7fc0df8818597fbfcec659a592c631790b7a76
notes/rewrite/fork-syscall-rewrite/TODO-3ARCH-PARITY-20261006.md
       77077 字节  sha256 9b3152dd14232e2cb90de9969aa5cca86c01acbe63307bffb1646d89f043f386
```

两条 sha256 是**磁盘当前态**（TODO-3ARCH 那行含未提交改动），取自 `manifest.notes.disk.tsv`，审阅方可用
`sha256sum <该文件>` 在迁移前后对读：迁移动手前后这两个值必须相等（只换路径，不换内容）。

注意：tag 指向的是提交态，**不含** TODO-3ARCH 文件的这处未提交改动。
审阅方要比对「迁移前磁盘态」时，应以 `manifest.notes.disk.tsv` 与 tar 卷为准，不要以 tag 的 blob 为准。
这条差异是有意保留的（不代用户提交在制改动），不算迁移缺陷。

产出命令：`git status --porcelain`、`git status --porcelain -uall`。

## 四、三圈账：跟踪 / 磁盘 / 被忽略

这是整个留存工作的地基：三个集合的差集决定了哪些内容只有一份拷贝。

| 圈 | 数量 | 产出命令 |
|---|---|---|
| notes 磁盘文件总数 | 2428 | `find notes -type f \| wc -l` |
| └ 其中 Markdown | 2133 | `find notes -name '*.md' \| wc -l` |
| notes 被 git 跟踪 | 1102 | `git ls-files notes \| wc -l` |
| notes 被 .gitignore 忽略 | 1325 | `git ls-files --others --ignored --exclude-standard \| grep -c '^notes/'` |
| notes 未跟踪且未被忽略 | 1 | `git status --porcelain -uall -- notes \| grep -c '^??'` |
| 守恒核对 | 1102 + 1325 + 1 = 2428 ✓ | 与清单里 git_status 列的分布一致 |

被忽略的 1325 个文件按类别拆开（产出命令：对 `manifest.notes.disk.tsv` 的 ignored 行按路径模式分类计数）：

| 类别 | 文件数 | 是什么 | 迁移时怎么对待 |
|---|---|---|---|
| `*​/.design/` 设计快照 | 1162 | 评审工作流要求每个 stage 先有可对照的设计稿快照（预检项的输入），18 个 stage 各一份 | 随所属 stage 目录整体移动，保持被忽略状态 |
| `*.log` 取证日志 | 155 | NK4A/NK4B 串口日志（`db7ebfe5c` 起 `*.log` 整域不入库） | 随 `evidence/` 整体移动 |
| `*.bak` / `*.bak.N` | 3 | `archive_bak/` 里的旧备份 | 随 `archive_bak/` 移动 |
| `notes/redesign/*.backup` | 2 | `improve_minix.md.backup`、`improve_minix_refactored.md.backup` | 随 redesign 移动；两份与正本 `cmp` 逐字节相同（实测），是否合并属迁移后的独立议题 |
| notes 内嵌 `.review/` | 3 | `fork-syscall-rewrite/.review/` 的历史评审产物 | 随树移动到 `.review/` 单一父目录下归档 |
| 合计 | 1325 | | |

被跟踪文件里最大的五个（说明「大文件已经退跟踪」这件事确实发生了）：

| 字节数 | 路径 |
|---|---|
| 2625867 | `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md` |
| 1048576 | `notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4b-p3-m34/virt-gic3.dtb` |
| 430806 | `notes/rewrite/fork-syscall-rewrite/15-stage-fs/doc_rerank_deepseek.md` |
| 411298 | `notes/study/syscall/archive/syscall-detail.md` |
| 382703 | `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/doc_rerank_deepseek.md` |

615177374 字节的 `evidence/20260922-nk4a-iter11-18/serial_c9a.log` **不在跟踪列表里**
（`git ls-files -s` 对它返回空，`git check-ignore -v` 指向 `.gitignore:19:*.log`）。
deepseek 计划里的 D14「615MB 大文件阻塞推送」议题在开窗当天（提交 `db7ebfe5c` 之后）已经不存在
（分支与 origin 齐平，推送成功）。本次迁移它只作为磁盘文件随 `evidence/` 移动，并收在 tar 卷里。

## 五、未跟踪文件详细表格（全仓 15 件 + notes 内 1 件）

用户要求「对所有未跟踪的文件，需要有一个详细记录的表格」。
`git status --porcelain -uall` 报出的未跟踪且未被忽略的文件，开窗时共 15 件；
其中 1 件在 notes 树内（进 `manifest.notes.disk.tsv`），其余 14 件在树外（进 `manifest.offtree.tsv`）。

### 5.1 树内未跟踪（1 件）

| 路径 | 字节数 | mtime | 迁移去向 |
|---|---|---|---|
| `notes/rewrite/fork-syscall-rewrite/PENDING-DECISIONS-3ARCH-PARITY.md` | 156358 | mtime 1791316966 | `rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md` |

### 5.2 树外未跟踪（14 件）

| 路径 | 字节数 | sha256 前 12 位 | 与迁移的关系 |
|---|---|---|---|
| `migrate_notes_plan/agents-workflow-optim.md` | 21561 | 04485db7f180 | 迁移计划的旁支讨论，不入新树，内容保持旧路径写法（历史证据） |
| `new_laptop_migrate/GIT取证会话记录-20260930.md` | 22803 | —（清单内全称） | 冻结不改写，列入旧路径白名单 |
| `new_laptop_migrate/GIT带病commit取证-20261007.md` | 32261 | — | 同上 |
| `new_laptop_migrate/ZCODE恢复PROMPT-git取证线-20260930.txt` | 3335 | — | 同上 |
| `new_laptop_migrate/git清理.md` | 13609 | — | 同上 |
| `new_laptop_migrate/侧边技术博客.md` | 10515 | — | 同上 |
| `new_laptop_migrate/可新增测试扫描.md` | 3157 | — | 同上 |
| `new_laptop_migrate/增量review.md` | 3175 | — | 同上 |
| `new_laptop_migrate/清零者.md` | 3150 | — | 同上 |
| `new_laptop_migrate/知识点扫描.md` | 2827 | — | 同上 |
| `new_laptop_migrate/diseased-scan-20261007/commits.tsv` | 27192 | e0503c6b9c29 | 取证数据，冻结 |
| `new_laptop_migrate/diseased-scan-20261007/paths.tsv` | 29996 | bc450e9032e3 | 同上 |
| `new_laptop_migrate/diseased-scan-20261007/summary.json` | 35049 | 92f7ae146315 | 同上 |
| `tools/atf-c-compat/probes/p7.c` | 1155 | b8da7f7b726 | 探针夹具，与迁移无关 |
| `tools/notes-link-check.py` | 8135 | e50a893a42b8 | **本次 Phase 0 新写的断链检查器**，为建立基线而生；会在 Phase 0 的入库提交里正式跟踪 |

注：sha256 前 12 位一列中，带「—」的三项完整值在 `manifest.offtree.tsv` 里没有 sha256 列（该清单只记路径/字节数/mtime/git 状态），
需要逐字节审计时从 tar 卷取。树内文件的 sha256 一律以 `manifest.notes.disk.tsv` 为准。

### 5.3 被忽略但值得单独点名的（树外）

`manifest.offtree.tsv` 里 2515 行被忽略文件按顶层目录分布（os 的构建产物区已排除，计数见下一行）：

| 顶层目录 | 被忽略/未跟踪文件数 | 与迁移的关系 |
|---|---|---|
| `.review/` | 1728 | 历史评审产物，**冻结不改写**（用户裁决），只在新写的映射文档里登记旧前缀 |
| `tmp/` | 409 | 一次性工作产物，冻结 |
| `tools/` | 310 | `__pycache__`、atf 构建产物，与迁移无关 |
| `book/` | 43 | mdbook 构建产物，本次不碰 |
| `os/` | 22 | 构建/缓存残留 |
| `new_laptop_migrate/` | 14 | 换机取证材料，含上面 5.2 的 13 件 |
| `migrate_notes_plan/` | 1 | `agents-workflow-optim.md` |
| `.qoder/` | 1 | `specs/NK4C信号双形态落地_task-fc7.md`，内含 6 处旧路径，冻结 |
| `.qoderignore`、`.claude/` | 各 1 | 配置 |
| 已排除 | 273374 | `os/target/`、`os/target_smp/`、`os/.cargo-shared/`、`os/.dockercargo/`、`os/tmp/` 构建产物，与迁移无关且体积大，只记总数不逐条列 |

## 六、引用面基线（迁移的真实成本）

统计口径：`grep -rnI -o -E 'notes/rewrite|fork-syscall-rewrite|notes/study|notes/redesign' .`，
排除目录 `.git`、`minix3`、`target`、`target_smp`、`.cargo-shared`、`.dockercargo`、`__pycache__`。
每处命中算一次，所以一个文件里同一路径写两遍计两次。

| 域 | 命中处数 | 处置档位（用户裁决） |
|---|---|---|
| `.review/` | 86879 | **冻结不改**，写 `.review/PATH-MAPPING.md` 登记新旧前缀 |
| `migrate_notes_plan/` | 6185 | **冻结不改**（六份计划描述的就是旧布局，保留才是证据） |
| `notes/` | 3277 | **必改** |
| `tools/` | 1773 | **必改**（其中 `anchor-suspect-baseline.txt` 868 行是前缀替换 + 行数对账，见第七节） |
| `os/` | 204 | **必改**（88 个 `.rs` + 1 个 `.toml` + 1 个 `.md` 的注释，不影响编译） |
| `prompt/` | 162 | **必改**（规则源，改完重新派生三端） |
| `.claude/` | 133 | **必改**（派生文件走同步链，不手改；`settings.local.json` 冻结） |
| `.codex/` | 79 | **必改**（派生，走 `tools/generate-derived-skills.sh` 重新生成） |
| `new_laptop_migrate/` | 71 | **冻结不改** |
| `.trae/` | 53 | **必改**（派生） |
| `tmp/` | 49 | **冻结不改** |
| `AI-chats/` | 10 | **冻结不改**（用户未勾选改写档位） |
| `CLAUDE.md` | 8 | **必改** |
| `.qoder/` | 6 | **冻结不改** |
| `AGENTS.md` | 3 | **必改** |
| 合计 | 98892 | 逐处明细见 `tmp/pre-migrate-snapshot-20261007/refs.before.lines.txt`（98892 行） |

一个计划里的过时项：glm 与 deepseek 都提到要改仓库根的 `check_references.sh`（`BASE_DIR` 写死旧路径）。
实测该文件**已不存在**（`ls check_references.sh` → No such file），它属于 `a7ac382ec` 那次「目录外残留清理」被删掉的一次性脚本。本次迁移不需要处理它。

工具链硬编码实测清单（`grep -rn "notes/rewrite" tools/*.sh tools/*.py tools/coverage-extract/*.py`）：

| 位置 | 内容 | 漏改后果 |
|---|---|---|
| `tools/check-review-rules.sh:147` | 断言 `prompt/review-rules/review-process.md` 里存在字面量 `notes/rewrite/{module}/{stage}/.design/` | 与规则文本必须同一提交成对修改，否则恒红 |
| `tools/doc-style-lint.sh:207` 与 `:227` | 增量文风门的 diff 范围写死 `-- notes/rewrite` | **静默失效**：门不再拦任何新增行且不报错，本次最危险的一项 |
| `tools/design-coverage-check.sh:46` | `MODULE_DIR="notes/rewrite/${MODULE}"` | 设计快照预检找不到目录 |
| `tools/review-gate-check.sh:28`、`:101`、`:105` | 同上 + module 说明文案 | 同上 |
| `tools/review-init.sh:5`、`:6`、`:47`、`:115`、`:116`、`:126`～`:129` | 按 `notes/rewrite/{module}/{stage}/{doc}.md` 形状切 module | 产物路径错位 |
| `tools/design-index-update.sh:2`、`:5`、`:16` | 用法示例与路径模板说明 | 示例失效 |
| `tools/coverage-extract/coverage-extract.py:15`、`:20`、`:25` | 文档字符串里的三条示例命令 | 示例失效 |
| `tools/pattern-gate.sh:5` | 头部注释引用 `notes/rewrite/fork-syscall-rewrite/PATTERN-SCAN-REPORT-20260923.md` | 注释失效（六份计划都没列出这一处，是本次新发现的） |
| `tools/anchor-suspect-baseline.txt` | 874 行里 868 行以旧 doc 全路径开头 | 锚点核对全量误报 |
| `tools/anchor-unresolved-baseline.txt` | 6609 行里只有第 2 行的范围注释含旧路径（token 本身不带路径） | 只需改一行注释 |

## 七、锚点基线现状

```
tools/anchor-suspect-baseline.txt     874 行（含旧路径 868 行）
tools/anchor-unresolved-baseline.txt  6609 行（含旧路径 1 行，即第 2 行范围注释）
```

suspect 清单的每行形如 `{doc路径}:{文档自身行号}: {代码路径}:fn NAME（Lnnn，工具生成）→ 句意复核提示`。
其中的行号是**文档自己的行号**，只要迁移全程遵守「单行内替换、不增删行」，行号继续有效，
所以处置方式是路径前缀机械替换 + 替换前后行数对账（874/6609 不变），不重生成
——重生成会连带改变锚定符号集，把尚未复核的 868 处历史债标记洗掉。这是用户裁决项。

## 八、tar 快照（Git 盲区的唯一恢复源）

```
位置        tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz
体积        29583270 字节（磁盘上 29M；notes 树本身 1.3G，其中 1.2G 是压缩比极高的串口日志）
sha256      39e68026187bc72fa9dd12672c0d0881bbda4146deb0ad7d9646ee98dcd86eb1
            （同目录留有 .sha256 文件）
文件条目数  4183
覆盖        notes/ 全部 2428 个磁盘文件（含 1162 个 .design 快照、261 个 .log 含 615MB 的 serial_c9a.log、
            3 个 .bak、2 个 .backup、内嵌 .review 3 件）
            + 仓库根 .review/ 全部 1728 件
            + migrate_notes_plan/pre-migrate-20261007/ 六份清单
            + 15 件未跟踪文件与 CLAUDE.md / AGENTS.md / README.md / opencode.json / .qoder/specs
完整性校验  tar -tzf 全卷可读通过（rc=0）；卷内 notes/ 条目 2428 与清单行数一致；
            .review/ 条目 1728 与磁盘计数一致
恢复命令    tar -xzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz -C <目标目录>
```

**必须提醒的一条局限**：tar 卷放在仓库的 `tmp/` 目录下，而 `tmp/` 自提交 `db7ebfe5c` 起整域不入库。
也就是说这份快照和它保护的 Git 盲区内容命运相同——磁盘上只有一份拷贝。
迁移验收通过后建议把 tar 卷复制一份到仓库外（例如移动硬盘或对象存储）再长期保留。

产出命令：`du -sh notes .review tmp`、`df -h .`（当时 `/` 分区剩余 767G）。

## 九、断链基线（迁移前就已经坏的东西）

`broken-links.before.txt` 记录 452 处断链，扫描范围是 `notes/ AI-chats/ prompt/ .claude/ .codex/ .trae/ migrate_notes_plan/ new_laptop_migrate/ book/*.md README.md CLAUDE.md AGENTS.md os/libs`，共 2228 个 md 文件。

按域拆开（审阅方可以用同一命令重算）：

| 域 | 断链数 |
|---|---|
| `notes/` | 425 |
| `prompt/` | 13 |
| `.trae/` | 12 |
| `migrate_notes_plan/` | 2 |
| 合计 | 452 |

计数命令要说清一个坑：报告文件在第 4 行有个空行分隔表头与正文，直接 `grep -vc '^#'` 会得到 453。用下面两条之一：

```bash
grep -Pvc '^#|^$' migrate_notes_plan/pre-migrate-20261007/broken-links.before.txt   # 输出 452
awk 'NR>4' migrate_notes_plan/pre-migrate-20261007/broken-links.before.txt | wc -l  # 输出 452
```

抽样几条典型的既有坏链（都不是迁移造成的）：

```
notes/rewrite/fork-syscall-rewrite/00-master-plan/03-minix3-source-audit.md:68  ../01-stage-pm/mproc-design.md
      —— 旧阶段编号：PM 现在是 04-stage-pm，历史上排在第一位，重排后引用没跟着改
notes/rewrite/fork-syscall-rewrite/00-master-plan/03-minix3-source-audit.md:72  ../deep-analysis/fork-all-layers-deep-analysis.md
      —— 目录已不存在，目标文件在 archive_bak/ 里
```

这条基线的作用是：迁移后的断链集合必须是它的子集（或按映射表可解释地收缩），
**新增一条都算迁移引入的破坏**。没有这份基线，验收门会把 452 处历史坏链误算成迁移造成的。

产出命令：`python3 tools/notes-link-check.py <上述路径> --output migrate_notes_plan/pre-migrate-20261007/broken-links.before.txt`
（该工具本次新写，`--self-test` 通过，覆盖「代码块内链接不检 / 相对与仓库根双基准解析 / 外链与纯锚点跳过 / 行号锚点剥离」四个判定）。

## 十、六份计划的现实偏差汇总（核对结论）

六份方案写于本次开窗的两周前（各文件抬头的自述日期），以下前提已经变化。执行时以本表为准，不以计划正文的旧数字为准。

| # | 计划里的前提 | 开窗当天实测 | 对执行的影响 |
|---|---|---|---|
| 1 | 仓库零 tag，所以「打 tag 是最低成本的分界点」 | 已有 9 个 tag，全是 minix3 上游版本号 | tag 照打，命名加 `notes/` 前缀区隔 |
| 2 | 分支 rewrite / main / nk4a-agent-wip / nk4a-review-docs / notes-study | 只剩 rewrite / main | 无需处理遗留分支 |
| 3 | 领先 origin 约 1560 提交未推送 | 0 领先 0 落后 | 大文件推送议题（deepseek D14）作废 |
| 4 | 615MB `serial_c9a.log` 已被跟踪、会阻塞推送 | 不再被跟踪（`*.log` 忽略规则），磁盘仍在 | tar 必须含它，git 不含它 |
| 5 | `notes/study` 187 个文件全部未跟踪，删了就永久丢失 | **184 个全部已跟踪** | 「study 删除不可逆」的前提不成立；本次仍按裁决原样搬走 |
| 6 | `archive_bak/` 40 个全部未跟踪 | 37 个已跟踪，3 个 `.bak` 被忽略 | 同上，用 `git mv` 而非 `mv` |
| 7 | fork 树根散落 27～29 个 md | **75 个顶层条目**（NK4C 系列新增约 46 个，含 `.txt` 与中文文件名） | 所有计划的工作文件清单都已过时，映射表按实测重新枚举 |
| 8 | notes 跟踪数 824 / 1046，磁盘 2369 | 1102 跟踪，2428 磁盘 | 数字以本文件为准 |
| 9 | 存在 `check_references.sh`（根，需改 BASE_DIR） | **文件已不存在** | 从改写清单里剔除 |
| 10 | 引用面 1698 文件（`notes/rewrite`） | 1747 个文件含四种旧路径之一，98892 处命中；新增 `new_laptop_migrate/`(11 文件)、`.qoder/`(1) 两个计划未覆盖的域 | 白名单要含这两个新域 |
| 11 | 工具硬编码 8 处 | 实测 10 处位置（新增 `tools/pattern-gate.sh:5`；`doc-style-lint.sh` 有 207 与 227 两行） | Phase 4 清单按实测 |
| 12 | 两份锚点基线都要前缀替换 | unresolved 6609 行里只有 1 行含旧路径（token 不带路径） | 只需改 1 行注释 |
| 13 | `.review/` 1725 文件、被 gitignore | 1728 文件、跟踪数仍为 0 | 未变；「rm -rf 重建」在本机不可逆，故不采纳 |
| 14 | 顶层无 `new_laptop_migrate/`；`migrate_notes_plan/` 只有 6 份 | 新增 `new_laptop_migrate/`（13 件）与第 7 份计划 `agents-workflow-optim.md` | 纳入白名单与快照 |
| 15 | `.gitignore` 规则：`.review/`、`**/.design/`、`*.bak` | 另有 `/tmp/`、`/evidence/`、`*.log`、`/os/target_smp/`、`/os/tmp/`、`/*.bin`、`.qoder*`（`db7ebfe5c`、`bb8a90e05` 两次入库决策） | 迁移后 `.gitignore` 复核时要知道这些新规则，不要误当成异常 |

## 十一、审阅方独立复核清单

不需要信任本文数字，逐条复跑即可：

```bash
cd /home/xzhao/github/minix-rs

# 1. tag 是否指向预期的迁移前提交
git for-each-ref --format='%(refname:short) %(*objectname)' refs/tags/notes
git show notes/pre-migrate-20261007 --no-patch

# 2. 三圈账
find notes -type f | wc -l                    # 期望 2428
git ls-files notes | wc -l                     # 期望 1102
git ls-files --others --ignored --exclude-standard | grep -c '^notes/'   # 期望 1325

# 3. 清单与磁盘是否一致（迁移前跑的清单，此刻应当逐行相等）
awk -F'\t' 'NR>1{print $1}' migrate_notes_plan/pre-migrate-20261007/manifest.notes.disk.tsv | sort > /tmp/mine.txt
find notes -type f | sort > /tmp/disk.txt
diff /tmp/mine.txt /tmp/disk.txt               # 迁移动手前期望空；动手后期望只剩新树路径

# 4. 清单自身没被事后改动（路径是仓库根相对写法，必须在仓库根下跑）
sha256sum -c migrate_notes_plan/pre-migrate-20261007/snapshot.sha256
#    该文件覆盖 7 项：三份清单 + 断链基线 + 引用底账 + git-state + tar 卷，全部应输出 OK

# 5. tar 卷完整且含 Git 盲区
sha256sum tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz   # 期望 39e68026…
tar -tzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz | grep -c '^notes/'          # 期望 2428
tar -tzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz | grep -c '\.design/'        # 期望 1162

# 6. 引用面
grep -rnI -o -E 'notes/rewrite|fork-syscall-rewrite|notes/study|notes/redesign' . \
  --exclude-dir=.git --exclude-dir=minix3 --exclude-dir=target --exclude-dir=target_smp \
  --exclude-dir=.cargo-shared --exclude-dir=.dockercargo --exclude-dir=__pycache__ | wc -l   # 期望 98892

# 7. 断链基线可用同一命令重跑（注意：迁移动手后目录变了，只能在旧树上跑）
python3 tools/notes-link-check.py notes/ --output /tmp/recheck.txt
grep -Pvc '^#|^$' /tmp/recheck.txt                          # 单跑 notes/ 期望 425
grep -c '^notes/' migrate_notes_plan/pre-migrate-20261007/broken-links.before.txt   # 基线里 notes/ 部分，也是 425
#    两者集合应当逐行相等：diff <(grep '^notes/' 基线 | sort) <(grep '^notes/' /tmp/recheck.txt | sort) 为空
```
