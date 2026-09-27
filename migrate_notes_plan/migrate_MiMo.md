# notes 目录重组迁移计划（MiMo）

本文件是 `notes/` 三个分区（rewrite / redesign / study）重组为根级三个笔记区的完整执行计划。计划面向多个 AI 并行执行，每一条事实断言都附带产生它的命令，执行者可以复跑命令核对，不依赖对本文件的信任。

先直接回答开头提出的三个问题：

1. **迁移前要不要打 tag？要，而且单独一个 tag 不够。** 仓库当前一个 tag 都没有（`git tag` 输出为空）。更关键的问题是：`.gitignore` 忽略了 `**/.design/`、`.review/`、`*.bak` 这三类文件（`.gitignore` 文件内容第 27、34、59 行，工具检出），而磁盘上恰恰有大量这类文件——仅 `fork-syscall-rewrite` 下就有 18 个 `.design` 目录。tag 只能冻结被 Git 跟踪的 824 个 notes 文件，冻结不了被忽略的一千三百多个文件。所以冻结动作必须是三件套：**先把未跟踪文件提交入库 → 打 annotated tag → 再打一个覆盖被忽略文件的全量 tar 包**。详见决策 D1。
2. **目标结构好不好？方向对，需要补四个分区。** `rewrite-notes/01-stage-kernel/16-smp.md` 这个两层结构是对的，去掉 `fork-syscall-rewrite` 这层语义已废弃的包装也是对的。需要补的是：根部 14 篇杂项文档的去处（`misc/`）、概念文档的去处（`concepts/`）、正在滚动更新的工作记录的去处（`worklog/`）、1.2 G 证据日志的去处（`evidence/`）。完整目标树见决策 D3。
3. **杂项文档放哪？14 篇全部进 `rewrite-notes/misc/`**，逐篇归属见决策 D4 的表。理由：按 `notes/TODO.md` 里已经写明的拆分标准（redesign 放架构变更、rewrite 放重构相关、study 放学习路线），这 14 篇没有一篇够格进 redesign 分区——它们全是重写工作本身的设计与策略文档。

---

## 1. 现状盘点

### 1.1 规模

以下数字全部由命令产出（2026-09-23 复跑）：

| 事实 | 数字 | 产出命令 |
|------|------|----------|
| `notes/` 文件总数 | 2369（其中 Markdown 2087） | `find notes -type f \| wc -l`、`find notes -name '*.md' \| wc -l` |
| `notes/rewrite` 文件数 | 2168 | `find notes/rewrite -type f \| wc -l` |
| `notes/study` 文件数 | 187 | `find notes/study -type f \| wc -l` |
| `notes/redesign` 文件数 | 12 | `find notes/redesign -type f \| wc -l` |
| `notes/` 根部文件数 | 2（`README.md`、`TODO.md`） | `ls notes/*.md` |
| `fork-syscall-rewrite` 磁盘文件数 | 2108 | `find notes/rewrite/fork-syscall-rewrite -type f \| wc -l` |
| 各 stage 目录数 | 21（`00-master-plan` 到 `20-redesign`） | `ls notes/rewrite/fork-syscall-rewrite` |
| `notes/rewrite` 体积 | 1.3 G，其中 `evidence/` 占 1.2 G | `du -sh notes/rewrite`、`du -sh .../evidence` |
| `evidence/` 大文件 | 多个超过 10 M 的串口日志 | `find .../evidence -type f -size +10M` |

`evidence/` 里是 NK4A/NK4B 两轮工作的串口取证日志（目录名如 `20260922-nk4b-p4-m44-clock`），它决定了两件事：备份 tar 要把它单独打包；在同一个文件系统内移动它只是目录项重命名，秒级完成，不用担心体积。

### 1.2 版本管理覆盖：三个圈

理解下面三个圈的差集，是整个冻结方案的前提：

| 圈 | 范围 | 数量 | 证据 |
|----|------|------|------|
| 圈一：被 Git 跟踪 | `git ls-files notes` | 824 | 命令输出 |
| 圈二：磁盘上存在 | `find notes -type f` | 2369 | 命令输出 |
| 圈三：未跟踪、也未被忽略（status 会报 `??`） | `git status --porcelain -uall -- notes` | 221 | 全部是 `notes/study/`（187）加 `notes/rewrite/archive_bak/` 里非 `.bak` 的部分 |
| 差集：磁盘有、但被 `.gitignore` 忽略 | 2369 − 824 − 221 ≈ 1324 | 大头是 18 个 `.design` 目录（Gate H 的设计快照，按 `.gitignore` 注释属于"中间产物不入库"）、`fork-syscall-rewrite/.review/` 下 3 个文件、各种 `.bak` |

三个推论，每条都直接决定迁移步骤：

- **推论一**：`notes/study` 整个目录没有进过版本管理。如果直接 `rm -rf notes/study`，这 187 个文件是永久删除，连 `git log` 都找不回来。所以 study 无论最终删不删，都必须先提交。
- **推论二**：`.design` 快照是 review 工作流 Gate H 的输入（`prompt/review-rules/review-process.md` 中 Gate H 各项检查都要求 `ls .../.design/{NN}-design.v*.md` 命中），tag 冻结不了它们。它们必须靠 tar 兜底。
- **推论三**：1.2 G 的 `evidence/` 是被跟踪的（112 个文件在 `git ls-files` 中），它已经在 Git 历史里（`git count-objects -vH` 显示 pack 224.51 MiB，压缩后）。移动它是安全的。

### 1.3 谁在引用旧路径（引用面清单）

执行 `grep -rIl 'notes/rewrite|fork-syscall-rewrite'`，按区域统计文件数（排除 `notes/`、`tmp/`、`.review/`、本计划目录）：

| 区域 | 引用文件数 | 性质 | 迁移时怎么办 |
|------|-----------|------|-------------|
| `os/`（Rust 源码） | 87 | 85 个 `.rs` 的文档注释 `//!` 指向 stage 目录，加 `os/tests/Cargo.toml` 的翻译映射注释与 `os/libs/minix-types/README.md`；另有被本统计排除的 `os/.review/pm/SYMBOLS.md` 也含旧路径 | 必须改，纯注释文本，不碰代码语义 |
| `prompt/` | 10 | review 规则源 + skill 源 + `todo_plan.md` 示例命令 | 必须改，改完跑三端同步工具 |
| `tools/` | 9 | 7 个脚本硬编码路径 + 2 个锚点基线文件的范围注释 | 必须改，见第 6.1 节逐脚本明细 |
| `.codex/` | 7 | 由 `prompt/skill/` 派生 | 不手改，跑 `tools/generate-derived-skills.sh` 重新生成 |
| `.claude/` | 6 | 规则与 skill（`review-scan` 是 `.claude` 独立维护、再派生到 codex） | 手工同步 + 跑校验工具 |
| `CLAUDE.md`、`AGENTS.md` | 各 1 | 目录布局说明、review 流程里的路径模板 | 必须改 |
| `check_references.sh`（仓库根，已被 gitignore） | 1 | `BASE_DIR` 写死 `notes/rewrite/fork-syscall-rewrite/02-stage-vm/` | 改一行 |
| `examples/`（已被 gitignore） | 1 | 示例 README | 顺手改 |
| `AI-chats/` | 4 | 历史会话产物 | 不改，列入残留白名单 |
| `.trae/documents/`、`.zcode/`、`.codebuddy/`、`.patch_doc07.py` | 15 + 16 + 1 + 1 | 均已被 gitignore 的历史会话产物 | 不改，列入白名单 |
| `notes/` 树内部 | 349 个 md 含字符串 `notes/rewrite`，353 个含 `fork-syscall-rewrite` | 文档正文里写的仓库路径（本应自包含，但存量就是这么多） | 必须改，按映射表批量替换 |
| `.github/` | 0 | CI 不引用 notes | 无需处理 |

`.review/`（仓库根、`os/.review`、`fork-syscall-rewrite/.review`）整体被 gitignore，是历史 review 快照，走冻结策略而不是改写策略（决策 D7）。

### 1.4 文档内部链接现状

| 事实 | 数字 | 产出命令 |
|------|------|----------|
| `notes/` 内 Markdown 相对链接总数 | 1413 | 提取 `](...)` 中指向 `.md` 的相对链接计数 |
| `notes/rewrite/fork-syscall-rewrite` 内使用 `../` 跨目录链接的文件 | 38 | `grep -rEl '\]\(\.\./' ... \| wc -l` |
| 其中向上逃逸两层以上（`../../`，会逃出 `fork-syscall-rewrite`）的文件 | 16 | `grep -rEl '\]\(\.\./\.\./' ... \| wc -l` |
| `notes/study` 内相对链接 | 169（分布在 9 个含 `../` 的文件里） | 同上方法 |
| 根部 14 篇杂项之间的互链 | 全部是同目录相对链接 | 提取结果只含 `modern-hardware-and-rust.md` 这类兄弟文件名 |

16 个逃逸文件的链接目标里，需要迁移时逐条改写的类型（按目标归类计数）：

- `../../../concepts/README.md`、`../../concepts/endpoint.md`、`../../../concepts/endpoint.md` —— 指向 `notes/rewrite/concepts/`；
- `../../../redesign/endpoint_redesign.md` —— 跨分区指向 `notes/redesign/`；
- `../../../AI-chats/comments.md` —— 跨出 notes 指向会话产物目录；
- `../../02-stage-vm/draft/vmproc-design.md#L78-L96` —— 带行号锚点的链接，替换时不能破坏 `#L78-L96` 尾巴。

好消息是：**stage 之间的兄弟链接（`../02-stage-vm/...`）和 stage 内部链接（`./xx.md`）在"整棵 `fork-syscall-rewrite` 平移到 `rewrite-notes/`"的方案下深度不变，一个都不用改**。需要动手的只有上面这批逃逸链接和杂项索引。

### 1.5 迁移前就存在的坏链（必须先打基线）

抽样证据：`notes/rewrite/fork-syscall-rewrite` 下不存在 `01-stage-pm` 目录（实际叫 `04-stage-pm`），但至少 3 个文档里写着 `](../01-stage-pm/mproc-design.md)`，而目标文件真实位置是 `notes/rewrite/fork-syscall-rewrite/04-stage-pm/draft/mproc-design.md`。同类还有 `](../deep-analysis/fork-all-layers-deep-analysis.md)`、`](../review-rules/review.md)` 这类指向不存在目录的链接。

**如果不在迁移前记录"迁移前坏链清单"，迁移后跑链接检查会把历史坏链误算成迁移引入的破坏**，验收就失去意义。所以 Phase 0 第一件事就是生成坏链基线文件。

### 1.6 工具链与规则里的路径模型假设

比"字符串写死"更深一层的问题：多处代码假设了 `notes/rewrite/{module}/{stage}/{doc}.md` 的**三层路径模型**，其中 `{module}` 恒等于 `fork-syscall-rewrite`。迁移去掉 module 层后模型变成两层，这不是 sed 能糊过去的，要改逻辑：

| 文件 | 假设位置 | 假设内容 |
|------|----------|----------|
| `tools/review-init.sh` | 路径归一化与 module 提取段 | 期望 `notes/rewrite/{module}/{stage}/{doc-stem}.md`，按 `notes/rewrite/` 前缀切第一级目录当 module |
| `tools/design-coverage-check.sh` | `MODULE_DIR` 赋值与 stage 收集段 | `MODULE_DIR="notes/rewrite/${MODULE}"`，再在 module 目录下按 `^[0-9]+-` 收集 stage |
| `tools/review-gate-check.sh` | design 目录推断段 | 同样的 `MODULE_DIR="notes/rewrite/${MODULE}"`，再遍历其下 stage 目录找 doc |
| `tools/design-index-update.sh` | module 名推断 | `MODULE_NAME="$(basename "$(dirname "$STAGE_DIR")")"`，旧模型下得到 `fork-syscall-rewrite`，新模型下会得到 `rewrite-notes` |
| `tools/doc-style-lint.sh` | 增量门的 git pathspec | `git diff ... -- notes/rewrite` 限定只检查这个前缀的改动 |
| `tools/check-review-rules.sh` | 一致性断言 | 断言 `prompt/review-rules/review-process.md` 里必须出现字面量 `notes/rewrite/{module}/{stage}/.design/`——路径模板一改，这条断言自己先红 |
| `CLAUDE.md`、`AGENTS.md`、`prompt/README.md` | `{module}` 定义 | 都写着"`{module}` = `notes/rewrite/` 下第一级目录" |

---

## 2. 设计决策

每条决策给出：选项、推荐、理由、是否需要用户拍板。**标了「需拍板」的，执行 Phase 2 之前必须先得到答复。**

### D1 冻结与打点：三件套（需拍板：冻结窗口时间）

选项：

- 甲：只打 tag。
- 乙：commit + tag。
- 丙：commit + tag + 全量 tar。

**推荐丙。** 理由（假设性推理）：如果只打 tag，那么被 `.gitignore` 忽略的 18 个 `.design` 目录、`fork-syscall-rewrite/.review/` 的 3 个文件、所有 `.bak` 都不在 tag 里；迁移一旦把这些文件挪坏或误删，没有任何东西能把它们变回来。tar 是唯一覆盖 Git 盲区的手段。反过来，如果只打 tar 不打 tag，纯跟踪文件的逐字节回滚就没有 `git reset --hard` 这么干净的路径。两者互补，缺一不可。

具体动作（按顺序，缺任一步不得进入 Phase 1）：

```bash
# 1. 收敛工作树：当前有 3 个已修改文件（复跑 git status --porcelain 确认）
#    AI-chats/daily.todo.md、notes/rewrite/fork-syscall-rewrite/NK4B-WORKLOG.md、tmp/nk4a/vars.fd
#    与并行工作协调：迁移窗口内 notes/、tools/、prompt/ 三个区停写
git status --porcelain

# 2. 把圈三（221 个未跟踪文件：notes/study 全部 + archive_bak 非忽略部分）提交入库
git add notes/ && git commit -m "chore(notes): 将 notes/study 与 archive_bak 纳入版本管理（迁移前置）"

# 3. 打 annotated tag —— 这就是你要的"明确分界点"
git tag -a pre-notes-migration-2026-09-23 -m "notes 三区重组前的冻结点：含全部跟踪文件；被 .design/.review/.bak 规则忽略的文件见配套 tar"

# 4. 全量 tar（含被忽略文件；evidence 单独一卷，放仓库目录之外，不要提交进仓库）
tar -czf  /tmp/minix-rs-pre-migration-20260923.tar.gz \
    --exclude=./.git --exclude=./target --exclude=./tmp \
    --exclude=./notes/rewrite/fork-syscall-rewrite/evidence \
    .
tar -czf /tmp/minix-rs-evidence-20260923.tar.gz \
    ./notes/rewrite/fork-syscall-rewrite/evidence

# 5. 生成三份基线（工具生成，禁止手写）
find notes -type f -print0 | sort -z | xargs -0 md5sum > migrate_notes_plan/manifest.before.md5
grep -rIl 'notes/rewrite\|fork-syscall-rewrite' . \
    --exclude-dir=.git --exclude-dir=tmp > migrate_notes_plan/refs.before.txt
# 坏链基线：Phase 6 的链接检查器先写好、先跑一遍存量，结果存为：
# migrate_notes_plan/broken-links.before.txt
```

迁移完成后打第二个 tag `notes-restructure-done-<date>`，两个 tag 之间就是完整分界。

### D2 目标根位置：根级三目录（需拍板：甲 / 乙）

- 甲（推荐）：仓库根级 `rewrite-notes/`、`redesign-notes/`、`study-notes/`，与 `book/` 并列。这是提问里给出的目标形态。
- 乙：留在 `notes/` 下，只把 `fork-syscall-rewrite` 这层剥掉，变成 `notes/rewrite/01-stage-kernel/...`。

**推荐甲。** 理由一：`book/` 已经在根级（根目录下 `README.md`、`SUMMARY.md`、`book.toml` 齐全），电子书抽取源与成品区并列在根级，四个目录构成完整的"写作区"，概念上干净。理由二：如果选乙，`notes/rewrite-notes/` 这种"notes 下面再叫 notes"的冗余命名要么忍、要么保留旧名 `notes/rewrite`，而旧名和"redesign 也搬进来重组"的语义对不上。理由三：前缀替换（`notes/rewrite` → `rewrite-notes`）本身是映射表驱动的机械操作，1.3 节统计的 150 余个非 notes 文件全部走同一条替换规则，边际成本可控。

乙的代价优势是真实的：`notes/rewrite` 前缀在 `tools/`、`prompt/` 里可以不动，只有 module 层需要处理。如果执行窗口非常紧，乙是合理的降级方案。**两个方案共用本计划其余全部章节，只有路径常量不同，映射表在 Phase 1 按拍板结果填一套即可。**

### D3 目标树（推荐形态）

以甲为准（乙把 `rewrite-notes` 换成 `notes/rewrite`，其余相同）：

```
book/                             # 现有 mdbook 骨架，不动；未来电子书抽取目标区
rewrite-notes/
├── README.md                     # 分区唯一入口（由原 notes/rewrite/README.md 改造，索引指向 misc/）
├── 00-master-plan/               # 原 fork-syscall-rewrite/00-master-plan 原样平移
├── 01-stage-kernel/ … 18-stage-commands/   # 原 fork-syscall-rewrite/NN-stage-* 平移，编号保留
├── 19-stage-integration/
├── concepts/                     # 原 notes/rewrite/concepts（6 篇）
├── misc/                         # 原 notes/rewrite 根部 14 篇杂项（归属表见 D4）
├── worklog/                      # NK4*、HANDOFF-*、edge*、claim-prompt、new_todo_* 等滚动工作记录
├── evidence/                     # 原 fork-syscall-rewrite/evidence（1.2 G，112 个跟踪文件）
└── （各 stage 内的 draft/ archive/ .design/ 随 stage 原样保留，深度不变）
redesign-notes/
├── README.md                     # 新写：说明本区在 rewrite 稳定后启用
├── endpoint/                     # endpoint_redesign.md
├── ipc/                          # ipc-improve.md
├── fork/                         # fork-redesign.md
├── architecture/                 # architecture-changes.md、semantic-modules.md、microkernel-*
├── rs/                           # 原 20-redesign/ 两篇（rs-cross-layer-pollution、tocutou-and-distributed-consistency）
└── misc/                         # improve_minix*.md 及其备份（是否合并见 D6，迁移阶段只移动）
study-notes/
├── README.md                     # 新写：标注"早期学习笔记，AI 生成，非事实基线"
├── arch/ boot/ interrupt/ ipc/ pm/ process/ services/ syscall/ vfs/ vm/   # 原 notes/study 子目录平移
└── learning-path.md、roadmap.md、progress.md、minix_structure.md 等根部文件原样平移
```

结构决策说明：

- **stage 编号前缀必须保留**（`01-`、`02-`……）。`tools/design-coverage-check.sh` 用 `ls | grep -E '^[0-9]+-'` 发现 stage，编号一丢整个工具失明。
- **为什么不套 `minix3_concept/` 中间层**：`notes/study` 现有的一级子目录（arch、boot、ipc、vm……）本身就是概念分类，再包一层 `minix3_concept/` 只增加深度不增加信息。提问里的 `study-notes/minix3_concept/xx.md` 若坚持要，套层是纯移动操作，不影响其它决策——此点列入第 10 节拍板清单。
- **worklog 单列**：NK4B-WORKLOG 这类文件迁移期间还在被写（当前就是已修改状态），和正式文档混在一起会让"迁移窗口停写"的范围说不清，所以给它们一个明确的窝，停写范围就好定义。
- **embedded `.review`**（`fork-syscall-rewrite/.review/`，3 个文件）搬去 `.review/legacy/fork-syscall-rewrite/`，让 review 产物回到单一父目录（决策 D7）。

### D4 根部 14 篇杂项的逐篇归属

| 源文件 | 目标 | 理由 |
|--------|------|------|
| `README.md` | `rewrite-notes/README.md` | 分区索引，改造后链接改指 `misc/` |
| `rewrite.md` | `rewrite-notes/misc/rewrite.md` | Rust 重构总览，属于 rewrite 分区 |
| `RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/misc/` | 重写原则 |
| `rewrite-strategy.md` | `rewrite-notes/misc/` | 重写策略 |
| `vertical-slice-strategy.md` | `rewrite-notes/misc/` | 切片策略，同上 |
| `project-plan.md` | `rewrite-notes/misc/` | 项目规划 |
| `project-structure.md` | `rewrite-notes/misc/` | 结构设计 |
| `minimal-skeleton.md` | `rewrite-notes/misc/` | 骨架设计 |
| `arch_mapping.md` | `rewrite-notes/misc/` | 架构映射 |
| `modern-hardware-and-rust.md` | `rewrite-notes/misc/` | `notes/TODO.md` 的拆分记录明确它由 improve_minix 的"现代硬件与 Rust"章节拆来、归 rewrite |
| `invariant.md` | `rewrite-notes/misc/` | 内核不变量分析 |
| `misc.md` | `rewrite-notes/misc/misc.md` | 异步消息表分析 |
| `ipc-sendrec.md` | `rewrite-notes/misc/` | IPC 语义分析 |
| `elf-loader.md` | `rewrite-notes/misc/` | ELF 加载分析 |

没有一篇进 redesign，判断标准沿用 `notes/TODO.md` 已写明的拆分标准：redesign 只收"架构变更方向"的文档。`concepts/` 6 篇保持独立目录不并入 `misc/`，因为它们是跨分区复用的概念词条（capability、endpoint、typestate……），和"某篇分析笔记"性质不同。

### D5 study 分区处置（需拍板：删 / 留 / 瘦身）

选项：

- 甲：迁移时直接删。
- 乙：先提交、整体平移到 `study-notes/`、README 顶部标注定性，删除另案处理（**推荐**）。
- 丙：迁移时就筛一遍，留下的入库、不留的只进 tar。

**推荐乙。** 理由：这 187 个文件当前不在版本管理里（1.2 节圈三），"直接删"等于不可恢复删除；而"迁移"和"清理"是两件风险性质不同的事——迁移的验收标准是"内容零丢失、引用零断裂"，一旦混进删除动作，验收矩阵要同时证明"删的都是该删的"，复杂度翻倍。先提交后，`git rm` 任何时候都能做，且 `git log` 永远可考古。**迁移阶段只移动，不删除任何内容文件。**

`archive_bak/`（40 个文件，多为 `.bak`）同理：非 `.bak` 部分随 Phase 0 提交入库并平移到 `rewrite-notes/archive/`；`.bak` 因被 gitignore，进 tar 后可从工作树清除——清除动作也建议放到迁移完成后的独立 commit。

### D6 redesign 分区组织

现有 12 个文件加 `20-redesign/` 的 2 个文件，按 D3 的主题桶（endpoint / ipc / fork / architecture / rs / misc）归位。两个注意点：

- `improve_minix.md` 与 `improve_minix.md.backup` 字节数相同（74982），`improve_minix_refactored.md` 与它的 `.backup` 亦然（82726）。是否合并是 `notes/TODO.md` 里挂着的旧待办，**迁移阶段不处理**，只移动；合并放到迁移后的独立任务，避免迁移 diff 里混入内容删改。
- `redesign-notes/` 在 rewrite 尚未定稿的当下的定位是"预开分区"：文件进去了，但 `README.md` 要写明"本区在 rewrite 稳定后启用，当前内容是前期探索存量"，防止后续 review 把它当活跃工作区扫。

### D7 历史 review 产物策略：冻结 + 映射，不改写

三处 review 产物，全部被 gitignore：

1. 仓库根 `.review/`（多工具 STATE、scan、SYMBOLS，几十个目录）；
2. `os/.review/`（至少 `pm/SYMBOLS.md` 含旧路径）；
3. `notes/rewrite/fork-syscall-rewrite/.review/`（3 个文件）。

策略：

- **历史快照不改写正文。** 它们记录的是"当时文档在哪个路径"这一历史事实，批量 sed 会毁掉考古价值，收益为零——没有任何工具会去读历史 STATE 里的路径来定位文件。
- **第 3 处搬进 `.review/legacy/fork-syscall-rewrite/`**，在 `.review/PATH-MAPPING.md`（新写）里记录一行：旧前缀 `notes/rewrite/fork-syscall-rewrite/` 对应新前缀 `rewrite-notes/`。
- **活跃 STATE 自然过渡**：`.review/codex/vm/STATE.md` 这类收敛态文件不动；下一次对该模块发起 review 时，`review-init.sh` 会按新路径模型建新产物，旧 STATE 的 Per-Doc 表继续作为历史参照。
- **两个锚点基线**（`tools/anchor-unresolved-baseline.txt`、`tools/anchor-suspect-baseline.txt`）：token 本体是符号名不含路径（基线文件头部注释写明格式），只需改头部"范围"注释里的旧路径描述；更彻底的做法是迁移后按头部注释里的重生成命令重跑一遍，覆盖旧文件。

### D8 并行写入冻结窗口（需拍板：窗口时间）

当前工作树有 3 个已修改文件，其中 `NK4B-WORKLOG.md` 正是滚动工作记录。多个 AI 并行迁移期间，若还有人在写 notes 或改 tools，Phase 2 的"内容哈希多重集相等"验收会直接失败在与迁移无关的文件上。因此：

- Phase 2 前 `git status --porcelain` 必须干净（或仅剩迁移自己的产物）；
- 冻结范围 = `notes/`、`tools/`、`prompt/`、`CLAUDE.md`、`AGENTS.md`、`os/**/*.rs` 的注释；
- 窗口时长预估：Phase 0-2 是小时级，Phase 3-6 视并行人力是半天到一天级；`evidence/` 移动不占时间（同文件系统 rename）。

---

## 3. 成功判据（迁移不变量）

六条，全部可机器判定：

| 编号 | 不变量 | 判定命令（终态执行） |
|------|--------|----------------------|
| I1 | 结构移动阶段内容零改动：Phase 2 结束时，新旧两棵树的文件内容哈希多重集相等 | 对比 Phase 2 前后各生成一份 `md5sum` 清单（只比较哈希不比较路径），`sort \| uniq -c` 后 diff 为空 |
| I2 | 新增坏链为零：相对坏链基线，坏链集合只减不增 | 链接检查器跑全树，`comm -13 broken-links.before.txt broken-links.after.txt` 输出为空 |
| I3 | 活区路径残留为零：`notes/rewrite`、`fork-syscall-rewrite` 两个字符串在活区文件中零命中 | `rg -l 'notes/rewrite\|fork-syscall-rewrite' tools prompt CLAUDE.md AGENTS.md README.md os rewrite-notes redesign-notes study-notes check_references.sh` 输出为空；白名单区（第 7 节）允许命中 |
| I4 | 工具链烟雾全绿 | 见第 9 节验证门 G4 |
| I5 | 三端规则一致：改完 `prompt/` 源后派生文件无漂移 | `tools/generate-derived-skills.sh --check` + `tools/diff-trae-skills.sh --only-diff` + `tools/check-review-rules.sh` 三条退出码全 0 |
| I6 | 代码编译与测试无新错误：`os/` 只动了文档注释 | `cargo build && cargo test && cargo clippy` 与迁移前基线比对，无新增 failure/warning |

I3 的白名单（允许保留旧路径的区域）：`.review/`、`os/.review/`、`.trae/documents/`、`.zcode/`、`.codebuddy/`、`AI-chats/`、`migrate_notes_plan/`、`tools/*-baseline.txt` 的历史注释段、`.claude/settings.local.json` 的历史权限条目（可选清理）。

---

## 4. 执行阶段

依赖关系：`0 → 1 → 2 → (3 ∥ 4) → 5 → 6 → 7 → 8`。括号内可并行。每个阶段独立成 commit（或 commit 组），保证任何一步都能单独 revert。

### Phase 0 — 冻结与基线（串行，唯一执行者）

- 步骤：按 D1 的五步命令序列执行；写 `migrate_notes_plan/broken-links.before.txt`（链接检查器可在本阶段先以"只读模式"落地，它是 Phase 6 的同一工具）。
- 产物：`pre-notes-migration-2026-09-23` tag、两个 tar、`manifest.before.md5`、`refs.before.txt`、`broken-links.before.txt`、一个把 221 个未跟踪文件入库的 commit。
- 验收：`git tag` 能列出新 tag；`tar -tzf` 能列出 `.design` 文件；`wc -l manifest.before.md5` 等于 `find notes -type f | wc -l`。
- 不可并行原因：tag、tar、manifest 必须描述同一时刻的同一棵树。

### Phase 1 — 映射表（串行定稿，可双人互审）

- 步骤：产出 `migrate_notes_plan/mapping.md`，内容分两部分：
  1. **目录级规则**（约 10 条）：如"`notes/rewrite/fork-syscall-rewrite/NN-stage-*/` → `rewrite-notes/NN-stage-*/`"、"`notes/study/<x>/` → `study-notes/<x>/`"；
  2. **例外表**（逐文件）：D4 的 14 篇、`20-redesign` 的 2 篇、worklog 文件枚举（NK4*、HANDOFF-*、edge*、claim-prompt、new_todo_*）、embedded `.review` 的 3 个文件、`archive_bak` 的去向。
- 验收：写一个覆盖检查脚本——把 `find notes -type f` 的每一行 join 到映射规则上，**未映射条目数必须为 0**。这份映射表是后续所有并行角色的唯一合同：移动、改写、验收三方都只认它。
- 互审点：例外表有没有漏（特别核对 `fork-syscall-rewrite` 根部 29 个散文件（`git ls-files` 按第一级目录统计中非 stage、非 evidence 的条目）与 `01-stage-kernel` 下 `smp_gpt*.md`、`doc_rerank_*.md` 这类非编号文件）。

### Phase 2 — 结构移动（串行，唯一写者）

- 步骤：按映射表执行。已跟踪的 824 个文件用 `git mv`（保历史）；未跟踪与被忽略的文件用 `mv`（同文件系统，1.3 G 也是秒级）。**本阶段禁止修改任何文件内容。**
- 产物：一个纯结构 commit：`chore(notes): 三区重组——剥除 fork-syscall-rewrite 包装层，根级四分区就位`。
- 验收：I1（哈希多重集相等）；`test ! -e notes`（或 `notes/` 只剩约定保留物，预期是清空后删除）；`git status` 干净。
- 回滚点：本 commit 可整体 revert。

### Phase 3 — 文本改写（并行，三个子轨，文件集不相交）

- **轨 B（树内文档）**：`rewrite-notes/`、`redesign-notes/`、`study-notes/` 内部。三类活：16 个逃逸链接逐条改写（注意保留 `#L78-L96` 这类锚点尾巴）；14 篇 misc 的 README 索引链接补 `misc/` 前缀；349+353 个文件里的旧路径字符串按映射表替换。
- **轨 C1（代码区）**：`os/` 的 87 个文件，`notes/rewrite/fork-syscall-rewrite/` → `rewrite-notes/` 的纯字符串替换，只碰注释行。
- **轨 C2（项目根文档）**：`CLAUDE.md`、`AGENTS.md`、根 `README.md`（其中"notes/ contains personal notes"一段要改成四分区说明）、`check_references.sh`、`examples/`。
- 验收：I3 在各自文件集上为零；`git diff` 逐段人工扫一眼，确认替换没有误伤 URL、锚点、代码块里的同名标识符。
- 替换纪律：只替换两种形态——反引号包裹的完整路径（`` `notes/rewrite/...` ``）与 Markdown 链接括号内的路径（`](notes/...`）；**不要做裸字符串全局 sed**，避免把讨论"旧路径长什么样"的分析句子也改掉（那类句子若存在，应保留原文并按映射表加一句新路径对照）。

### Phase 4 — 工具链改造（与 Phase 3 并行，文件集与轨 B/C 不相交）

- 步骤：按第 6.1 节逐脚本明细改造；核心是把三层路径模型换成两层：`{root}/{section}/{doc}.md`，其中 `{root}` 默认 `rewrite-notes`，`{section}` 承担原 `{stage}` 职能，原 `{module}` 的位置在 review 产物路径 `.review/{tool}/{module}/` 中由 `{section}` 顶替（这与现存 `.review/codex/01-stage-kernel/`、`.review/claude/03-stage-kernel/` 的目录风格本来就是一致的，等于把两种历史风格收敛到带编号的那种）。
- 每个脚本配一条烟雾命令，写进第 9 节 G4。
- 产物：一个独立 commit：`feat(tools): review 工具链适配两层笔记路径模型`。

### Phase 5 — 规则源同步（依赖 Phase 3 轨 C2 与 Phase 4 完成）

- 步骤：改 `prompt/review-rules/`（`review-process.md` 34 处、`review-patterns.md` 11 处、`review-doc-checklist.md` 3 处）与 `prompt/skill/`（5 个 skill 源）及 `prompt/README.md`、`prompt/todo_plan.md` 的示例命令；改 `tools/check-review-rules.sh` 里那条字面量断言（它断言的就是旧模板字符串）；然后跑三端同步：`tools/generate-derived-skills.sh` 重新生成 `.trae/skills/` 与 `.codex/skills/`；`.claude/rules/` 与 `.claude/skills/review-scan/` 手工同步（`prompt/README.md` 的三端同步说明节写明了哪些是生成、哪些是手工）。
- 验收：I5 三条命令全绿。
- 纪律：**派生目录（`.codex/`、`.trae/skills/`）永远不手改**，只改 `prompt/` 源再生成；`.claude/skills/review-scan/` 是例外（它是向 codex 派生的源之一），改它后同样要跑 `--check`。

### Phase 6 — 验证门（依赖 3、4、5）

跑第 9 节 G1-G7 全部命令，任何一门红 → 修 → 重跑，不允许"带红通过"。产物：`migrate_notes_plan/VERIFY.md`，逐门贴命令与输出。

### Phase 7 — 收尾

- `notes/TODO.md` 的内容（拆分待办）已随迁移过时：其有效结论已体现在本计划 D4/D6，把该文件归档为 `rewrite-notes/misc/notes-todo-archive.md` 或删除（需拍板，默认归档）。
- 新写三份 README：`rewrite-notes/README.md`（索引改造）、`redesign-notes/README.md`、`study-notes/README.md`（定性标注）。
- `.review/PATH-MAPPING.md`、两个锚点基线的范围注释。
- 复核 `AGENTS.md` 目录布局段与 `CLAUDE.md` 的 Key Constraints 段与真实树一致。

### Phase 8 — 打终态 tag

`git tag -a notes-restructure-done-<date>`，并在 tag 注释里写明两个 tag 的对读方法。至此迁移闭环。

---

## 5. 多 AI 并行分工

### 5.1 角色与文件所有权

| 角色 | 独占写入范围 | 负责阶段 |
|------|-------------|----------|
| **R（整合者）** | `migrate_notes_plan/`、git 操作（commit/tag/mv）、`notes/`→新树的移动本身 | Phase 0、1、2、6、7、8；唯一允许 `git mv`/`git commit` 的角色 |
| **T（工具轨）** | `tools/`、`prompt/`、`.claude/`、`.codex/`、`.trae/skills/`、`CLAUDE.md`、`AGENTS.md` | Phase 4、5 |
| **D（文档轨）** | `rewrite-notes/`、`redesign-notes/`、`study-notes/` 三棵树内部的 `.md` | Phase 3 轨 B |
| **S（源码轨）** | `os/` 下 87 个文件、根 `README.md`、`check_references.sh`、`examples/` | Phase 3 轨 C |

所有权按目录切分而不是按任务切分，目的是让任何两个角色的写入文件集**交集恒为空**，从机制上消灭并行写的冲突。

### 5.2 时序与合并规则

1. R 完成 Phase 0-2 后广播"结构就位"，D、T、S 同时开工；
2. 三轨各自在独立分支上作业，回归 R 的集成分支时以 rebase 为准；
3. T 完成 Phase 4 是 Phase 5 的前置；D、S 完成是 Phase 6 的前置；
4. **禁止跨轨伸手**：T 发现树内有坏链，报告给 D，不自己去改 `rewrite-notes/`；D 发现工具路径没改完，报告给 T；
5. `mapping.md` 只有 R 能改。执行中发现映射表漏项 → 走"例外表增补"流程，R 增补后广播，各轨重取。

### 5.3 冲突热点（已知的两人同写高危点）

| 文件/区域 | 为什么危险 | 规避 |
|-----------|-----------|------|
| `CLAUDE.md`、`AGENTS.md` | 既含路径模板（T 的活）又含目录布局说明（C2 的活） | 归 T 独占，C2 轨把发现的改动需求提给 T |
| `prompt/todo_plan.md` | 既是规则源又满是示例命令 | 归 T |
| `rewrite-notes/misc/README.md`（原 `notes/rewrite/README.md`） | 索引改造属 D，但若 T 也在里面改路径模板会撞 | 归 D，T 不碰 tree 内文件 |
| Phase 2 期间的整棵树 | 唯一写者原则被破坏的唯一场景 | R 独占窗口，其它角色此阶段只读 |
| `.claude/settings.local.json` | 历史权限条目里全是旧路径命令 | 白名单放着不改，或迁移收尾由 T 单独一个 commit 清理 |

---

## 6. 引用改写明细

### 6.1 工具链（T 轨，逐文件）

| 文件 | 改什么 | 验收 |
|------|--------|------|
| `tools/review-init.sh` | 用法示例与路径归一化段：接受 `rewrite-notes/{section}/{doc}.md` 与 `{section}/{doc}.md` 两种输入；module 提取改为取根前缀后的第一级目录 | 用 `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` 跑一次，产物落在 `.review/<tool>/01-stage-kernel/03-kmain-cstart/` |
| `tools/design-coverage-check.sh` | `MODULE_DIR` 改为 `rewrite-notes`（全量模式按 `^[0-9]+-` 直接收集 section）或 `rewrite-notes/{section}`（单 section 模式）；用法字符串同步 | `design-coverage-check.sh --all` 与 `--stage 02-stage-vm` 各跑一次 |
| `tools/review-gate-check.sh` | 同上的 `MODULE_DIR` 推断段 | 指定一个 doc 跑一次，Gate H 能列出 design 快照 |
| `tools/design-index-update.sh` | `MODULE_NAME` 推断改为 `basename "$STAGE_DIR"`（section 名）或显式参数；头部注释示例路径 | 对 `rewrite-notes/01-stage-kernel` 跑一次，DESIGN-INDEX 头部路径正确 |
| `tools/doc-style-lint.sh` | git pathspec `-- notes/rewrite` 改为三个新区（`-- rewrite-notes redesign-notes study-notes`）；用法说明同步 | `doc-style-lint.sh --dir rewrite-notes` 跑通 |
| `tools/check-review-rules.sh` | 字面量断言 `notes/rewrite/{module}/{stage}/.design/` 改为新模板 `rewrite-notes/{section}/.design/`（与 Phase 5 对 `review-process.md` 的改动**同一个 commit**，否则中间态必红） | 脚本退出码 0 |
| `tools/coverage-extract/coverage-extract.py` | 文档字符串里的示例目录 | `--help` 可读 |
| `tools/anchor-unresolved-baseline.txt`、`tools/anchor-suspect-baseline.txt` | 头部范围注释里的旧路径；或按注释中的重生成命令整体重跑覆盖 | 头部无 `notes/rewrite` 字样 |
| `tools/review-line-check.sh`、`tools/doc-code-map.sh`、`tools/doc-snippet-extract.sh`、`tools/anchor-migrate.sh`、`tools/anchor-resolve.sh` | 接受显式路径参数，无硬编码（已核对）；只需改用法注释里的示例 | `--help`/`--self-test` 通过 |

### 6.2 规则源与派生（T 轨）

| 文件 | 处置 |
|------|------|
| `prompt/review-rules/review-process.md`（34 处） | 路径模板批量替换 + `{module}` 定义段重写（新语义：`{module}` := `rewrite-notes/` 下第一级目录，即 section） |
| `prompt/review-rules/review-patterns.md`（11 处）、`review-doc-checklist.md`（3 处） | 示例命令路径替换 |
| `prompt/skill/review-{process,doc,patterns,coverage,agent-ide}-skill.md`（5 个源） | 同上 |
| `prompt/README.md`、`prompt/todo_plan.md` | 示例与目录说明替换；`todo_plan.md` 是执行期工作文件，示例必须保持可运行 |
| 派生 | `tools/generate-derived-skills.sh` 重新生成 `.trae/skills/` + `.codex/skills/`；`.claude/rules/*` 与 `.claude/skills/review-scan/**` 手工同步；`tools/diff-trae-skills.sh --only-diff` + `tools/generate-derived-skills.sh --check` 验证 |
| `CLAUDE.md`、`AGENTS.md` | 目录布局段、`{module}` 定义、Step 0 预检的 `ls notes/rewrite/...` 命令模板 |

### 6.3 代码与库 README（S 轨）

- `os/` 下 85 个 `.rs`：`//!` 注释里的 `notes/rewrite/fork-syscall-rewrite/<stage>/` → `rewrite-notes/<stage>/`；
- `os/tests/Cargo.toml`：翻译映射账本注释路径；
- `os/libs/minix-types/README.md`：其中"架构设计：`notes/rewrite/fork-syscall-rewrite/fork-syscall-plan.md`"这条引用指向的文件本来就在 `archive_bak`（未跟踪），迁移后按 D5 落到 `rewrite-notes/archive/fork-syscall-plan.md`，路径按实际落点写；
- `os/.review/pm/SYMBOLS.md`：在 gitignore 白名单策略下可不改；若改，归 S 轨。

### 6.4 树内文本（D 轨）

- 349 个含 `notes/rewrite` 的 md + 353 个含 `fork-syscall-rewrite` 的 md：按映射表的字符串对照做定向替换；
- 16 个逃逸链接文件：逐条改（数量小、锚点敏感，不建议批量）；
- `misc/` 索引：原 `README.md`、`rewrite.md` 的表格链接补 `misc/` 前缀。

---

## 7. 风险登记册

| # | 风险 | 触发即现的信号 | 缓解 | 兜底 |
|---|------|---------------|------|------|
| R1 | tag 冻结不了被 gitignore 的 `.design`/`.review`/`.bak` | Phase 0 只打了 tag 没打 tar | D1 三件套强制顺序 | 从 tar 恢复 |
| R2 | 未跟踪的 study/archive_bak 被移动弄丢 | `manifest.before.md5` 里有、终态树里无 | Phase 0 先 commit | `git show pre-notes-migration...:path` |
| R3 | 迁移窗口内他人并行写入 | I1 哈希对不上、diff 混入无关文件 | D8 冻结公告 + 开窗前 status 干净 | 把无关改动拆出迁移 commit |
| R4 | 链接批量替换误伤锚点/URL | 坏链集合出现新条目，或 diff 里出现 `#L` 尾巴被切 | 两种形态定向替换 + 禁裸 sed；16 个逃逸文件逐条手改 | I2 捕获，逐条回退 |
| R5 | 三层路径模型的工具逻辑漏改 | G4 烟雾命令红 | 6.1 表逐脚本验收，不靠 grep 完事 | 单脚本 revert |
| R6 | 三端规则漂移 | `check-review-rules.sh` 非 0 | Phase 5 强制三连命令 | 从源重新生成 |
| R7 | 历史 review 产物引用失效引发误判 | 有人拿旧 STATE 路径找文件 | D7 冻结策略 + PATH-MAPPING | 映射表反查 |
| R8 | `evidence/` 1.2 G 拖慢或搞挂备份 | tar 超时、磁盘满 | evidence 单独卷；移动用 `mv` 不用复制 | tag 内已有跟踪部分可从 Git 恢复 |
| R9 | 锚点基线过期导致锚点检查误报 | `anchor-resolve --check` 输出突变 | 迁移后按基线文件头部命令重生成 | 旧基线随 tar 可查 |
| R10 | 迁移中途有人对旧路径发起新 review | `review-init.sh` 报路径不存在 | 冻结窗口覆盖 Phase 0-5 | 属预期失败，窗口结束自愈 |
| R11 | 同号双文件（如 `07-cross-space-init.md` 与 `07-paging_init_gpt.md`、`06-proc-init-boot-proc.md` 与 `06-todo.md`）在映射时被当成重复合并 | 哈希清单条目数下降 | 映射规则：一对一平移，禁止合并去重 | I1 立刻捕获 |
| R12 | `os/` 注释替换碰到非注释代码 | cargo build 报错 | 替换目标是带 `notes/` 前缀的完整路径串，代码中不可能出现该形态 | I6 捕获 |

---

## 8. 回滚方案

按"损伤半径从小到大"三层：

1. **单阶段回滚**：迁移拆成 8 个以内独立 commit（结构 1 个 + 各轨文本若干 + 工具 1 个 + 规则 1 个），任何一层出问题 `git revert` 对应 commit，不影响其它层。
2. **整体回滚（跟踪文件）**：
   ```bash
   git checkout -b rollback-notes pre-notes-migration-2026-09-23
   # 或在主线上：
   git reset --hard pre-notes-migration-2026-09-23   # 仅当迁移 commit 尚未推送共享时
   ```
3. **整体回滚（被忽略文件）**：
   ```bash
   tar -xzf /tmp/minix-rs-pre-migration-20260923.tar.gz -C /path/to/repo
   ```
   tar 是 `.design`、`.review`、`.bak` 这些 Git 盲区的唯一恢复源——这也是 D1 坚持三件套的原因。

回滚后验收：`git status` 干净、`find notes -type f | wc -l` 回到 2369、坏链数回到基线水平。

---

## 9. 最终验证门（Phase 6 逐条执行并记录到 VERIFY.md）

| 门 | 命令 | 通过标准 |
|----|------|----------|
| G1 结构与内容 | 对 Phase 2 前后哈希清单做多重集比对；`find notes` 期望不存在（或为空） | 哈希多重集相等；旧树清空 |
| G2 链接完整性 | 链接检查器跑 `rewrite-notes/ redesign-notes/ study-notes/` | `comm -13 broken-links.before.txt after.txt` 为空（I2） |
| G3 路径残留 | `rg -l 'notes/rewrite\|fork-syscall-rewrite' tools prompt CLAUDE.md AGENTS.md README.md os rewrite-notes redesign-notes study-notes check_references.sh` | 输出为空（I3，白名单除外） |
| G4 工具烟雾 | `review-init.sh` 试跑一个 doc；`design-coverage-check.sh` 全量 + 单 section；`review-gate-check.sh` 试跑；`design-index-update.sh` 试跑；`doc-style-lint.sh --dir rewrite-notes`；`anchor-resolve.sh --self-test`；`anchor-migrate.sh --self-test` | 全部退出码 0 |
| G5 规则三端一致 | `tools/generate-derived-skills.sh --check`；`tools/diff-trae-skills.sh --only-diff`；`tools/check-review-rules.sh` | 三条退出码 0（I5） |
| G6 代码基线 | `cargo build`、`cargo test`、`cargo clippy` | 与迁移前比对无新增失败（I6）；预期零语义影响（只改了注释路径） |
| G7 文档入口可达 | 人工打开 `rewrite-notes/README.md`、`redesign-notes/README.md`、`study-notes/README.md`，点穿一级链接 | 无死链，索引覆盖 D4 全部 14 篇 |

---

## 10. 开放问题（拍板清单）

执行 Phase 2 前需要逐条答复：

1. **D2 选甲（根级 `rewrite-notes/` 等）还是乙（留在 `notes/` 下）？** 推荐甲；乙是可接受的降级，只影响路径常量。
2. **D5 study 分区：迁移后保留（推荐，README 标注定性）还是后续删除？** 若删除，建议给一个时间点（例如"迁移完成并稳定两周后"独立执行）。
3. **`study-notes/` 是否套 `minix3_concept/` 中间层？** 推荐不套（现有子目录即分类）；套层与否只影响 D 轨一条移动规则。
4. **D8 冻结窗口什么时候开？** 需要与 NK4B 滚动工作（`NK4B-WORKLOG.md` 当前在写）协调出一个停写时段。
5. **`archive_bak` 的 `.bak` 文件与两个 `.backup` 文件**：tar 保留后从工作树清除，还是原地不动？推荐 tar 后清除（它们是重复备份，且 gitignore 本来就不收）。
6. **1.2 G 的 `evidence/` 长期去留**：迁移照搬；是否改为外部存储/发布附件，属迁移之后的独立议题，本计划只做"单独打包 + 原样平移"。
7. **`notes/TODO.md`**：归档为 `rewrite-notes/misc/notes-todo-archive.md`（推荐）还是直接删？

---

## 附：本计划引用的全部证据命令一览

```bash
# 规模
find notes -type f | wc -l                          # 2369
find notes -name '*.md' | wc -l                     # 2087
for d in notes/study notes/redesign notes/rewrite; do find $d -type f | wc -l; done  # 187 / 12 / 2168
du -sh notes/rewrite notes/rewrite/fork-syscall-rewrite/evidence   # 1.3G / 1.2G

# 版本管理三圈
git ls-files notes | wc -l                          # 824
git status --porcelain -uall -- notes | grep -c '^??'   # 221
git check-ignore -v notes/rewrite/fork-syscall-rewrite/01-stage-kernel/.design/01-design.v1.md
git tag                                             # 空
git count-objects -vH                               # size-pack: 224.51 MiB

# 引用面
grep -rIl 'notes/rewrite' . --exclude-dir=.git --exclude-dir=notes --exclude-dir=tmp \
  --exclude-dir=.review --exclude-dir=migrate_notes_plan | ...     # os 87 / prompt 10 / tools 9 / ...
grep -rIl 'notes/rewrite' notes --include='*.md' | wc -l           # 349
grep -rIl 'fork-syscall-rewrite' notes --include='*.md' | wc -l     # 353

# 链接
grep -rEl '\]\(\.\./\.\./' notes/rewrite/fork-syscall-rewrite --include='*.md' | wc -l   # 16
grep -rEl '\]\(\.\./' notes/rewrite/fork-syscall-rewrite --include='*.md' | wc -l        # 38

# 工具假设
grep -n 'notes/rewrite' tools/review-init.sh tools/design-coverage-check.sh \
  tools/review-gate-check.sh tools/design-index-update.sh tools/doc-style-lint.sh \
  tools/check-review-rules.sh tools/coverage-extract/coverage-extract.py

# 当前脏文件（开窗前复跑）
git status --porcelain | grep -v '^??'
```
