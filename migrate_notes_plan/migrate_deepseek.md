# Minix-RS 笔记目录迁移计划（deepseek 版）

> 本计划的目标读者是执行迁移的 AI 会话与项目维护者。计划本身只处理"目录搬迁 + 引用同步"，
> 不改写任何文档的语义内容。所有事实陈述都给出可复现的命令或文件路径，执行者可以先复现再动手。

---

## 0. 一页速览

推荐的目标布局（与本书 `book/` 平级）：

```
rewrite-notes/              重写工作区（主力）
  README.md
  00-master-plan/ ... 19-stage-integration/
  concepts/  coordination/  evidence/  misc/  archive/
redesign-notes/             重设计探索区
  README.md  architecture/  ipc/  fork/  vm/
study-notes/                学习笔记区（先保全，后精简）
  arch/ boot/ interrupt/ ipc/ pm/ process/ services/ syscall/ vfs/ vm/  misc/  archive/
```

执行顺序（每一阶段单独提交，禁止把改名与改内容混在同一次提交）：

```
P0 冻结与备份  →  P1 纯改名提交  →  P2 机械引用替换提交  →  P3 文档链接与旧编号修复提交
→  P4 工具链适配提交  →  P5 规则集三端同步提交  →  P6 收尾提交  →  P7 独立验收
```

两个必须先回答的问题：

1. **是否打标签**：必须打。标签给出可回退的分界点，但标签保护不了未跟踪内容
   （`notes/study` 有 176 个文件只存在于磁盘），所以标签之外还要加文件清单与压缩包，见第 6 章 P0。
2. **是否顺手修复既存断链**：建议纳入。迁移后 `notes/rewrite/` 前缀本身就会消失，
   断链检查必然要跑；既存的旧阶段编号引用（`01-stage-pm` 等 26+71+6+6 处）如果不同步修掉，
   迁移后的链接检查无法通过，也会把真实遗漏淹没在噪声里，见第 5.4 节。

---

## 1. 事实基线（迁移前必须承认的现状）

### 1.1 版本库与工作树

| 事实 | 证据命令 | 结果 |
|------|----------|------|
| 当前分支 | `git branch --show-current` | `rewrite` |
| 分支活跃度 | 前后两次 `git log -1 --format=%h` | 本计划写作期间分支从 `435f71943` 前进到 `44e64fcfa`，说明有其他会话在持续提交 |
| 领先远端 | `git log --oneline origin/rewrite..rewrite \| wc -l` | 约 `1560` 个提交未推送 |
| 标签 | `git tag` | 空，仓库没有任何标签 |
| 工作树状态 | `git status --short` | `AI-chats/daily.todo.md` 与 `tmp/nk4a/vars.fd` 已修改；`tmp/nk4a/` 下一批未跟踪文件 |
| 分支关系 | `git merge-base rewrite notes-study` | `d9c87bf8b`；`notes-study` 仅比共同祖先多一个提交 `893b3ceea`（新增 11 个 syscall 笔记） |

分支在计划写作期间仍在前进，这一点直接影响执行：P0 的冻结不是"打个标签就完了"，
而是要先与所有活跃会话确认停止提交（或约定一个安静的迁移窗口），把 `HEAD` 固定下来。
否则 P1 的纯改名提交会与别人的提交交错，文件守恒核对失去基准。

`notes/study` 在当前分支上没有任何文件被跟踪：`git ls-files notes/study | wc -l` 为 0，
但磁盘上有 187 个文件。这是本迁移最大的数据风险源。

### 1.2 规模盘点

下表的数字是写作时的快照，测量命令在附录 D。仓库当前仍有其他会话在向 `evidence/` 写入取证文件
（写作期间跟踪文件从 766 增长到 769），所以 P0 阶段必须重新测量一次，并以那一份清单作为文件守恒的基准。

| 目录 | 磁盘文件数 | 磁盘体积 | 已跟踪文件数 |
|------|-----------:|---------:|-------------:|
| `notes/rewrite/`（含子树） | 2125 | 1.3 GB | 769 |
| ├─ `notes/rewrite/fork-syscall-rewrite/` | 2065 | 1.3 GB | 749 |
| │  ├─ 各 stage 目录与 `00-master-plan`（不含证据与快照） | 650 | 约 30 MB | 650 |
| │  ├─ `evidence/` | 226 | 1.2 GB | 72 |
| │  └─ 根部协作文档与 `README.md` | 27 | 约 200 KB | 27 |
| ├─ `notes/rewrite/` 根部杂项文档（直接位于该层） | 14 | 约 250 KB | 14 |
| └─ `notes/rewrite/concepts/` | 6 | 约 100 KB | 6 |
| `notes/redesign/` | 12 | 432 KB | 10 |
| `notes/study/` | 187 | 5.6 MB | 0（`notes-study` 分支跟踪其中 11 个） |
| `book/` | 仅 mdBook 骨架，无章节内容 | — | 6 |

中间产物：`notes/rewrite` 下 `.design/` 共 18 个目录、1159 个文件（被 `.gitignore` 的 `**/.design/` 忽略）；
`notes/rewrite/fork-syscall-rewrite/.review/` 3 个文件；仓库根 `.review/` 共 1725 个文件（整体被忽略）。
另有 `notes/rewrite/archive_bak/` 40 个文件，完全未跟踪。

### 1.3 未被版本库保护的内容（风险清单）

| 内容 | 位置 | 文件数 | 说明 |
|------|------|-------:|------|
| 学习笔记主体 | `notes/study/**` | 约 176 | 当前分支未跟踪，只在 `notes-study` 分支跟踪 11 个 |
| fork 时代备份 | `notes/rewrite/archive_bak/` | 40 | 完全未跟踪，多数被 `*.bak` 规则忽略 |
| 设计快照 | `notes/rewrite/fork-syscall-rewrite/*/.design/` | 1159 | 被忽略；属于可再生成的中间产物 |
| 审查状态 | 仓库根 `.review/` 与 fork 树 `.review/` | 1728 | 被忽略；历史产物 |
| 超大证据 | `notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4a-iter11-18/serial_c9a.log` | 1 | 615,177,374 字节，已由提交 `e4c6e8224` 跟踪，但不在 `origin/rewrite` |

### 1.4 引用分布（谁依赖这些路径）

统计口径：`git grep -l 'notes/rewrite' -- <目录>`，只统计已跟踪文件，排除 `minix3/`。

| 消费方 | 涉及文件数 | 主要形式 |
|--------|-----------:|----------|
| `notes/rewrite/` 内部文档 | 约 291 | 正文中的路径字符串、相对 Markdown 链接 |
| `os/`（Rust 源码注释） | 86 | 文档锚点 `notes/rewrite/fork-syscall-rewrite/{stage}/...` |
| `tools/` | 9 | 脚本内的路径常量、基线文件、使用说明 |
| `prompt/`（规则源） | 10 | 路径模式、模块解析规则、命令示例 |
| `.claude/` `.codex/` `.trae/`（派生规则） | 18 | 与规则源同源的路径文本 |
| `AGENTS.md`、`CLAUDE.md` | 2 | 目录布局与模块定义 |
| `AI-chats/` | 4 | 任务记录与提示词模板 |
| 合计（去重） | 129 | — |

字符串层面的总量：`fork-syscall-rewrite` 在已跟踪文件中出现 2320 次，其中 2231 次带完整前缀
`notes/rewrite/fork-syscall-rewrite/`，这类可以直接做前缀替换；剩余约 89 次是裸树名（出现在规则示例、
文档标题、任务记录中），需要人工判断。

### 1.5 链接完整性现状（迁移必须顺手处理）

两类问题在迁移前就已存在：

1. **旧阶段编号引用**。阶段目录曾整体重排，以下旧编号仍散落在文档与工具说明中：

   | 旧编号 | 现编号 | 出现次数 | 涉及文件数 |
   |--------|--------|---------:|-----------:|
   | `01-stage-pm` | `04-stage-pm` | 26 | 6 |
   | `03-stage-kernel` | `01-stage-kernel` | 71 | 33 |
   | `04-stage-vfs` | `05-stage-vfs` | 6 | 4 |
   | `05-stage-sched` | `06-stage-sched` | 6 | 3 |
   | `deep-analysis/`（目录已不存在） | 内容现存于 `archive_bak/fork-all-layers-deep-analysis.md` | 6 | 2 |

   验证命令：`git grep -c '01-stage-pm\|03-stage-kernel\|04-stage-vfs\|05-stage-sched' -- . ':!minix3'`。

2. **相对链接深度错位**。同一批文档在不同时期以不同目录深度写成，导致 `../` 层数不一致。
   两个可直接复核的例子：
   - `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/10-switch-to-user.md` 中的 `](../../os/kernel/src/lib.rs)`，
     解析结果是 `notes/rewrite/os/kernel/src/lib.rs`，实际不存在（正确目标在仓库根的 `os/`）。
   - `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md` 中的 `](../../../minix3/minix/servers/vm/cache.h)`，
     解析结果是 `notes/minix3/...`，实际不存在（正确目标在仓库根的 `minix3/`）。

   这意味着迁移不能只做字符串替换，必须运行一次"按新树逐文件解析"的链接检查，见附录 B。

### 1.6 与迁移直接冲突的既存问题

1. **615 MB 大文件**。`serial_c9a.log` 已进入本地历史（提交 `e4c6e8224`），
   但不在 `origin/rewrite`。一旦推送 `rewrite` 分支，GitHub 的单文件 100 MB 限制会直接拒绝。
   删除工作树文件不能解决问题（对象仍在历史里），需要单独的历史重写任务，见决策 D14。
2. **双索引且均已过时**。重写区有两个索引：`notes/rewrite/README.md`（根）与
   `notes/rewrite/fork-syscall-rewrite/README.md`（fork 树）。前者是主题索引，
   后者写的是已经不存在的 `01-stage-pm/`、`deep-analysis/`、`05-mock-strategy.md`。
   真正与当前结构一致的是 `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md`
   （按服务启动顺序列出 00 到 20 号目录）。
3. **工具中的模块语义已经漂移**。`tools/review-init.sh` 把
   `notes/rewrite/{module}/{stage}/{doc}.md` 中 `notes/rewrite/` 之后的第一级目录当作模块，
   于是 `fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md` 的模块名是 `fork-syscall-rewrite`；
   但磁盘上历史审查产物 `.review/claude/03-stage-kernel/...` 的模块名却是阶段名，
   说明早期布局是 `notes/rewrite/{stage}/...`，包装层 `fork-syscall-rewrite` 是后加的。
   本迁移把包装层去掉，等于回到"阶段直接在树根下"的形态，必须同步重定模块语义，见决策 D3。

---

## 2. 目标结构与命名（推荐方案）

### 2.1 总体布局

```
book/                        电子书，未来从 rewrite-notes 抽取内容；本次不动
rewrite-notes/               重写工作区（语义冻结式重写，主力）
  README.md                  新的唯一入口索引
  00-master-plan/            顶层规划（沿用原名，保持与 00-master-plan/README.md 的对应）
  01-stage-kernel/ ... 19-stage-integration/
  concepts/                  跨阶段概念（endpoint、capability、typestate 等）
  coordination/              协作板与交接文档（edge_todo、NK4A/NK4B 等）
  evidence/                  实验取证（按 日期-任务 分组）
  misc/                      杂项文档与历史索引（project-plan、invariant 等，含 fork 树旧 README）
  archive/                   迁移归档（legacy-fork-bak 等未跟踪备份）
redesign-notes/              重设计探索区
  README.md
  architecture/              架构级改进（内聚、闭包、语义模块、跨层污染等）
  ipc/                       endpoint 与 IPC 协议重设计
  fork/                      fork 语义重设计
  vm/                        预留：未来 vm_in_kernel 等分支
study-notes/                 学习笔记区（冻结保全，精简另立任务）
  arch/ boot/ interrupt/ ipc/ pm/ process/ services/ syscall/ vfs/ vm/
  misc/                      学习路线、进度、结构总览
  archive/                   逐文件 AI 笔记与历史备份
```

### 2.2 为什么放在仓库根，而不是继续放在 `notes/` 下面

三个理由，按重要性排序：

1. **层级深度**。当前最深的文档是 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md`，
   从仓库根要穿过 4 层目录才见到文件。放到仓库根后是 `rewrite-notes/01-stage-kernel/16-smp.md`，
   穿过 2 层。目录层数越少，相对链接的出错面越小。
2. **语义边界**。仓库根 `README.md` 把 `notes/` 描述为"personal notes and drafts"。
   重写文档已经是项目的一等交付物（有独立的审查流水线、锚点校验、覆盖率工具），
   继续挂在"草稿"语义下面既误导读者，也让工具规则里"`notes/rewrite/` 下面第一级目录就是模块"
   这种脆弱假设一直存在。
3. **未来分工**。`book/`、`rewrite-notes/`、`redesign-notes/`、`study-notes/` 四个平级目录，
   正好对应四个互不混淆的阶段：电子书抽取、重写、重设计探索、源码学习。

代价是明确的：工具与规则中的 `notes/rewrite/` 硬编码要改，这部分工作量已列入第 6 章 P4、P5。

### 2.3 备选方案与取舍

| 方案 | 形态 | 优点 | 缺点 |
|------|------|------|------|
| A（推荐） | 仓库根三个新目录 | 层级最浅，语义最清晰，与 `book/` 平级 | 需要改工具硬编码 |
| B | `notes/rewrite-notes/{stage}/...` | 改动量最小，`notes/` 前缀保留 | 仍有 3 层；`notes` 的"草稿"语义仍在；与用户目标不符 |
| C | `docs/rewrite/...` | 通用命名 | 与未来 `book/` 的定位冲突，丢掉"笔记/交付物"的区分 |

如果维护者选择 B，本计划仍然成立，只需把全文中的 `rewrite-notes/` 统一替换为
`notes/rewrite-notes/`，并把 P4 的工具改动量减半（`notes/` 前缀可保留）。但推荐 A。

### 2.4 为什么保留 `NN-stage-*` 编号与名字

阶段目录名同时被三套东西引用：`os/` 的 86 个源文件注释、`tools/` 的路径推断、
文档之间数十处跨阶段相对链接。数字前缀还承担"按启动顺序阅读"的排序职责
（`00-master-plan/README.md` 是对应关系的权威来源）。重命名阶段目录只会放大风险，
不产生收益，因此本次不重命名、不重排编号。

---

## 3. 决策登记册

执行者在 P0 阶段逐条确认下表。每一条都给出推荐值；没有收到否决意见时按推荐值执行，
并把最终裁定写进 `rewrite-notes/misc/migration-decisions.md` 作为审计记录。

| 编号 | 决策点 | 推荐值 | 备选项 | 影响面 |
|------|--------|--------|--------|--------|
| D1 | 新根位置 | 仓库根 `rewrite-notes/`、`redesign-notes/`、`study-notes/` | 放在 `notes/` 下面 | 全量路径与工具 |
| D2 | 树名 | 三个带 `-notes` 后缀的名字 | 复用 `notes-rewrite` 等 | 全量路径 |
| D3 | 模块语义 | 树本身是模块（`rewrite`、`redesign`、`study`），第一级目录是阶段 | 沿用"第一级目录是模块" | 规则源、三端派生、工具 |
| D4 | 重写区杂项文档 | 全部落 `rewrite-notes/misc/` | 按主题散落 | 14 个文件与约 20 条引用 |
| D5 | 协作过程文档 | 落 `rewrite-notes/coordination/` | 落 `misc/` | 27 个文件 |
| D6 | 实验证据 | 落 `rewrite-notes/evidence/`，内部结构不变 | 留在原树 | 226 个文件与文档引用 |
| D7 | 学习笔记保全方式 | 先提交到迁移分支再搬（可回退） | 只打压缩包不入库 | 176 个本地文件 |
| D8 | 学习笔记去留 | 整体保留为 `study-notes/`，精简另立任务 | 立即删除 | 187 个文件 |
| D9 | 重设计主题分类 | 按 4.4 节默认表落位 | 暂不分类整体搬迁 | 12 个文件与 4 条引用 |
| D10 | 旧编号与断链修复 | 纳入本次 P3 | 只修迁移自身引入的断链 | 旧编号 115 处 + 其他断链约 95 处 |
| D11 | 兼容软链接 | 不留。历史路径只存在于 git 历史中 | 在旧路径放软链 | 工具重复扫描风险 |
| D12 | 分支策略 | 从 `rewrite` 开 `notes-migration` 分支执行，验收后合回 | 直接在 `rewrite` 上做 | 回滚与并行 |
| D13 | 标签命名 | `pre-notes-migration-20260922` 与 `post-notes-migration-20260922` | 无标签 | 回退分界点 |
| D14 | 615 MB 证据文件 | 独立任务处理（历史重写 + 压缩摘要），不并入本次迁移 | 保持现状不推送 | 推送能力 |
| D15 | 双写审查产物路径 | 新路径含阶段：`.review/{tool}/rewrite/{stage}/{doc-stem}/` | 维持只含文档名 | 同名 `todo.md` 冲突 |

关于 D11 的说明：保留软链接看起来"零风险"，但 `find`、`rg`、覆盖率扫描都会因此出现
重复命中或跟随不跟随的分歧，反而让后续审查结果不可复现。
迁移的兼容性由"一次性替换所有引用"保证，而不是由软链接保证。

关于 D15 的说明：多个阶段存在同名文档（例如 `01-stage-kernel/todo.md` 与 `05-stage-vfs/todo.md`），
当前"模块目录 + 文档名"的状态布局在去掉模块层后必然碰撞，所以状态目录要包含阶段名。
这同时解决了历史审查产物 `.review/claude/03-stage-kernel/` 与 `.review/claude/fork-syscall-rewrite/`
两套布局并存造成的混乱。

---

## 4. 完整迁移映射

以下"+ 未跟踪"指该内容当前不被 git 跟踪，不能用 `git mv` 直接操作（要么先用 `git add` 纳入跟踪，
要么用普通 `mv`）。执行前把本章内容落成机器可读的映射文件
`tools/notes-migration-2026-09/path-map.tsv`，格式为 `旧路径<TAB>新路径<TAB>类型`，
迁移脚本与链接修复脚本共用同一份映射，避免两处规则不一致。

### 4.1 目录级映射

| 旧路径 | 新路径 | 说明 |
|--------|--------|------|
| `notes/rewrite/fork-syscall-rewrite/00-master-plan/` | `rewrite-notes/00-master-plan/` | 顶层规划 |
| `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/` … `19-stage-integration/` | `rewrite-notes/01-stage-kernel/` … `19-stage-integration/` | 19 个阶段目录，名字不动 |
| `notes/rewrite/fork-syscall-rewrite/evidence/` | `rewrite-notes/evidence/` | 内部按日期-任务分组的子目录保持原样 |
| `notes/rewrite/fork-syscall-rewrite/20-redesign/` | 拆入 `redesign-notes/architecture/`（2 个文件，见 4.4） | 目录本身消失 |
| `notes/rewrite/fork-syscall-rewrite/.review/`（未跟踪） | `.review/archive/notes-fork-syscall-rewrite-2026-09/` | 中间产物归档，不随主树迁 |
| `notes/rewrite/fork-syscall-rewrite/*/.design/`（未跟踪） | 随所属阶段目录整体移动，路径变为 `rewrite-notes/{stage}/.design/` | 无引用依赖，不必逐个处理 |
| `notes/rewrite/concepts/` | `rewrite-notes/concepts/` | 6 个文件 |
| `notes/rewrite/archive_bak/`（未跟踪） | `rewrite-notes/archive/legacy-fork-bak/` | 40 个文件，保持未跟踪 |
| `notes/rewrite/README.md` | `rewrite-notes/README.md` | 迁移后重写为以阶段表为主的新索引 |
| `notes/rewrite/` 根部其余 13 个文档 | `rewrite-notes/misc/` | 逐文件见 4.2 |
| `notes/redesign/` | `redesign-notes/{architecture,ipc,fork}/` | 逐文件见 4.4 |
| `notes/study/` | `study-notes/` | 整体改名，主题子目录不动 |
| `notes/README.md` | `study-notes/misc/notes-directory-legacy-readme.md` | 只有一句"草稿文件夹"，作为历史保留 |

### 4.2 重写区根部杂项文档（13 个 + 索引 1 个）

| 旧路径 | 新路径 | 归属判断依据 |
|--------|--------|--------------|
| `notes/rewrite/README.md` | `rewrite-notes/README.md` | 主题索引，升级为新树入口 |
| `notes/rewrite/RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/misc/RECONSTRUCTION-PRINCIPLES.md` | 重写方法论 |
| `notes/rewrite/arch_mapping.md` | `rewrite-notes/misc/arch_mapping.md` | 硬件机制到 trait 的映射 |
| `notes/rewrite/elf-loader.md` | `rewrite-notes/misc/elf-loader.md` | ELF 加载实现设计 |
| `notes/rewrite/invariant.md` | `rewrite-notes/misc/invariant.md` | 内核不变量分析 |
| `notes/rewrite/ipc-sendrec.md` | `rewrite-notes/misc/ipc-sendrec.md` | SENDREC 原子性源码分析 |
| `notes/rewrite/minimal-skeleton.md` | `rewrite-notes/misc/minimal-skeleton.md` | 最小骨架设计 |
| `notes/rewrite/misc.md` | `rewrite-notes/misc/misc.md` | 异步消息表分析 |
| `notes/rewrite/modern-hardware-and-rust.md` | `rewrite-notes/misc/modern-hardware-and-rust.md` | 重写设计哲学 |
| `notes/rewrite/project-plan.md` | `rewrite-notes/misc/project-plan.md` | 项目规划 |
| `notes/rewrite/project-structure.md` | `rewrite-notes/misc/project-structure.md` | 工程结构设计 |
| `notes/rewrite/rewrite-strategy.md` | `rewrite-notes/misc/rewrite-strategy.md` | 语义冻结策略 |
| `notes/rewrite/rewrite.md` | `rewrite-notes/misc/rewrite.md` | 重写总览（与根 `rewrite.md` 文件同名不冲突） |
| `notes/rewrite/vertical-slice-strategy.md` | `rewrite-notes/misc/vertical-slice-strategy.md` | 纵向切片策略 |
| `notes/rewrite/fork-syscall-rewrite/README.md` | `rewrite-notes/misc/legacy-fork-syscall-index.md` | fork 时代索引，内容已过时，改名以区别于新入口 |

判断规则：文档的主题是"如何重写、如何验证重写"则归 `rewrite-notes/misc/`；
文档的主题是"架构应该变成什么样"则归 `redesign-notes/`（4.4 节没有从这 14 个里划走文件，
因为它们的行文对象都是重写工程本身，包括 `modern-hardware-and-rust.md`）。

### 4.3 协作与交接文档（27 个）

| 旧路径（相对 fork 树根） | 新路径 | 备注 |
|--------------------------|--------|------|
| `edge_todo.md` | `rewrite-notes/coordination/edge_todo.md` | 跨阶段条目唯一入口，活跃文档 |
| `edge_todo_archive.md` | `rewrite-notes/coordination/edge_todo_archive.md` | 已闭单条目 |
| `edge1.md` `edge2.md` `edge3.md` `edge4.md` | `rewrite-notes/coordination/` 同名 | 上一轮三线并行编排 |
| `new_edge1.md` … `new_edge4.md` | `rewrite-notes/coordination/` 同名 | 新一轮三线并行编排 |
| `claim-prompt.md` | `rewrite-notes/coordination/claim-prompt.md` | 并发领用提示词 |
| `HANDOFF-NK4A-boot-first-light.md`、`HANDOFF-NK4A-review-and-redo.md`、`NK4A-HANDOFF-STATUS.md`、`NK4A-QWEN-OPENING-PROMPT.md`、`NK4A-QWEN-WORKLOG.md`、`NK4A-REVIEW-REPORT.md`、`NK4A-TODO.md` | `rewrite-notes/coordination/` 同名 | NK4-A 已收口，保留作证据 |
| `NK4B-OPENING-PROMPT.md`、`NK4B-TODO.md`、`NK4B-WORKLOG.md` | `rewrite-notes/coordination/` 同名 | NK4-B 进行中 |
| `new_todo_deepseek.md`、`new_todo_glm.md`、`new_todo_muse.md`、`new_todo_qwen.md`、`new_todo_HY4.md` | `rewrite-notes/coordination/` 同名 | 多模型任务书历史 |
| （不在上表）`doc_rerank_*.md`（各阶段目录内，84 个） | 原地不动 | 属于阶段级过程产物，移动只会制造噪声 |

### 4.4 重设计文档主题归类（12 个）

| 旧路径 | 新路径 | 归类依据 |
|--------|--------|----------|
| `notes/redesign/redesign.md` | `redesign-notes/README.md` | 原目录说明，升级为入口 |
| `notes/redesign/endpoint_redesign.md` | `redesign-notes/ipc/endpoint_redesign.md` | Endpoint 协议重设计 |
| `notes/redesign/ipc-improve.md` | `redesign-notes/ipc/ipc-improve.md` | IPC 性能与语义改进 |
| `notes/redesign/fork-redesign.md` | `redesign-notes/fork/fork-redesign.md` | fork 语义重设计 |
| `notes/redesign/architecture-changes.md` | `redesign-notes/architecture/architecture-changes.md` | 系统结构变更 |
| `notes/redesign/improve_minix.md` | `redesign-notes/architecture/improve_minix.md` | 架构改进总思考 |
| `notes/redesign/improve_minix_refactored.md` | `redesign-notes/architecture/improve_minix_refactored.md` | 上一文档的重构版；重复内容去留另立任务 |
| `notes/redesign/microkernel-cohesion-design.md` | `redesign-notes/architecture/microkernel-cohesion-design.md` | 模块内聚 |
| `notes/redesign/microkernel-closure-design.md` | `redesign-notes/architecture/microkernel-closure-design.md` | 架构闭包 |
| `notes/redesign/semantic-modules.md` | `redesign-notes/architecture/semantic-modules.md` | 语义模块抽象 |
| `notes/rewrite/fork-syscall-rewrite/20-redesign/rs-cross-layer-pollution.md` | `redesign-notes/architecture/rs-cross-layer-pollution.md` | Rust 跨层污染 |
| `notes/rewrite/fork-syscall-rewrite/20-redesign/tocutou-and-distributed-consistency.md` | `redesign-notes/architecture/tocutou-and-distributed-consistency.md` | 分布式一致性探索 |

`redesign-notes/vm/` 本次不放入任何文件，作为未来 `vm_in_kernel` 分支的落点预留。
`notes/redesign/*.backup` 两个未跟踪文件不进新树，随 P0 压缩包归档。

### 4.5 学习笔记

| 旧路径 | 新路径 | 说明 |
|--------|--------|------|
| `notes/study/{arch,boot,interrupt,ipc,pm,process,services,syscall,vfs,vm}/` | `study-notes/` 下同名子目录 | 主题结构不变 |
| `notes/study/daily.md`、`learning-path.md`、`minix_structure.md`、`progress.md`、`roadmap.md` | `study-notes/misc/` 同名 | 路线与进度类文档 |
| `notes/study/pm/archive/`、`syscall/archive/`、`vfs/archive/`、`vm/archive/` | `study-notes/` 下同名 | `tmp_*.c.md` 逐文件笔记，整目录保留 |
| `notes/study/**/*.bak`、`progress.md.backup_*` | 随压缩包归档，不进入新树 | 被 `.gitignore` 忽略 |
| `notes/README.md` | `study-notes/misc/notes-directory-legacy-readme.md` | 历史说明 |

如果维护者选择 D8 的"立即删除"分支，删除前必须先完成 P0 的压缩包与清单，
并在提交信息里写明删除范围；推荐先保留，把精简留给独立的 `style-fix` 类任务。

### 4.6 特意不迁移或需要后续处理的内容

| 内容 | 处理 |
|------|------|
| `minix3/`、`os/`、`tools/`、`prompt/`、`book/`、`AI-chats/`、`tmp/` | 原地不动，只改其中的引用 |
| 仓库根 `.review/` 的旧模块目录（`03-stage-kernel/`、`claude/` 等） | 归档到 `.review/archive/`，不进入 git |
| `.design/` 快照 | 随阶段目录迁移，保持被忽略状态 |
| `tmp/nk4a/`（内核注释引用的取证日志） | 原地不动，本次不触碰 |
| 615 MB 的 `serial_c9a.log` | 迁移只保证它随证据目录移动；历史清理见 D14 |

---

## 5. 引用更新矩阵

| 编号 | 消费方 | 文件数 | 处理方式 | 验证命令 |
|------|--------|-------:|----------|----------|
| R1 | `notes/rewrite/` 内部正文路径 | 291 | 前缀替换 `notes/rewrite/fork-syscall-rewrite/` → `rewrite-notes/` | `git grep -c 'notes/rewrite/fork-syscall-rewrite'` 结果为 0（白名单见 A1） |
| R2 | `notes/rewrite/` 根部杂项路径 | 少量 | 按 4.2 表逐个替换为 `rewrite-notes/misc/...` | `git grep 'notes/rewrite/project-plan\|notes/rewrite/rewrite\.md'` 为 0 |
| R3 | 文档间相对链接 | 约 95 | 按附录 B 脚本重算相对路径；同阶段内链接不变 | 脚本退出码 0 |
| R4 | 裸树名 `fork-syscall-rewrite`（非路径） | 约 89 处 | 人工判断：改为 `rewrite-notes` 或删除词 | `git grep -n 'fork-syscall-rewrite'` 逐条登记 |
| R5 | `os/` 源码注释文档锚点 | 86 | 与 R1 同一前缀替换 | `git grep -n 'notes/rewrite' -- os/` 为 0 |
| R6 | `tools/` 脚本与基线 | 9 | 路径常量与模块解析按第 6 章 P4 改造；基线文件做前缀替换 | 工具自测全过 |
| R7 | `prompt/` 规则源 | 10 | 模块语义与路径示例改造 | `tools/check-review-rules.sh` 通过 |
| R8 | `.claude/`、`.codex/`、`.trae/` 派生 | 18 | 先改源，再运行生成脚本；手工维护的两处（`.claude/rules/`、`.claude/skills/review-scan/`）同步手改 | `tools/generate-derived-skills.sh --check` 无漂移 |
| R9 | `AGENTS.md`、`CLAUDE.md` | 2 | 更新目录布局与模块定义 | 人工复核 |
| R10 | `AI-chats/` | 4 | 提示词模板与演示文档更新；`daily.todo.md` 作为历史任务记录保留原样 | 人工复核 |
| R11 | `.trae/documents/irq-notify-and-doc14-regression-review.md` | 1 | 与 R1 同一前缀替换 | 路径检查脚本 |
| R12 | 审查状态 `.review/**/*.md`（未跟踪） | 若干 | 前缀替换或随归档冻结；新状态从 P4 后的新路径生成 | `tools/review-state-validate.py` |
| R13 | `tools/anchor-suspect-baseline.txt`、`tools/anchor-unresolved-baseline.txt` | 2 | 前缀替换（只改路径不删行）；迁移期间禁止改动文档行数 | 行数不变（874 / 6609） |
| R14 | `migrate_notes_plan/` 与本计划 | 1 | 保留原路径引用作为历史对照，不做替换 | 不适用 |

关于 R13 的行数约束需要展开说明：锚点基线记录的是"文档路径:行号"，本次迁移的所有文档编辑
都限定为**单行内的字符串替换**（路径前缀、链接文字），不新增也不删除行；只要守住这条约束，
基线里的行号在迁移后依然指向同一行，前缀替换即可继续使用。
如果 P3 的链接修复无法保持行数（例如必须拆行），那么在同一提交里重新推导受影响基线，
推导方法见 `prompt/todo_plan.md` 的锚点章节（S1 迁移语义风险一节）。

---

## 6. 执行阶段

每一阶段都遵循同一个节奏：**先跑门禁，再动手；提交后立刻复验**。
禁止跨阶段混合提交，尤其禁止把 P1 的改名与 P2 的文本替换放进同一个提交。
原因：git 的改名检测按内容相似度工作，同一个提交里既有改名又改内容会让
`git log --follow` 与 `git blame` 的追溯质量下降，也让回滚无法按阶段进行。

### P0 冻结与备份

前置条件：工作树状态已知且可解释。

```bash
# 1) 确认基线提交并打标签（annotated tag）
git switch rewrite
git status --short                      # 记录：AI-chats/daily.todo.md、tmp/nk4a/vars.fd 的修改
git log -1 --format='%H %s'             # 记录迁移前提交 SHA
git tag -a pre-notes-migration-20260922 -m "notes 目录迁移前的分界点"

# 2) 整仓备份（bundle 含被移动的分支）
git bundle create /tmp/opencode/minix-rs-pre-migration-20260922.bundle \
    rewrite notes-study --tags

# 3) 未跟踪与忽略内容的清单与压缩包（标签保护不了它们）
find notes -type f | sort > /tmp/opencode/notes-files-before.txt
xargs -a /tmp/opencode/notes-files-before.txt sha256sum > /tmp/opencode/notes-manifest-before.sha256
tar -czf /tmp/opencode/notes-untracked-20260922.tar.gz \
    notes/study notes/rewrite/archive_bak \
    notes/rewrite/fork-syscall-rewrite/.review \
    $(find notes/rewrite -type d -name .design | tr '\n' ' ')

# 4) 工作树清理：把不相关的修改先提交或暂存，保证迁移提交的 diff 只剩改名
git add AI-chats/daily.todo.md tmp/nk4a/vars.fd
git commit -m "chore(notes-migration): 冻结迁移前的无关工作树改动"
```

门禁：`git status --short` 输出为空；`tar -tzf /tmp/opencode/notes-untracked-20260922.tar.gz | wc -l`
与 `find` 清单的差值可解释（忽略项正常）。

### P1 纯改名提交

在一套派生的脚本与映射文件上工作，脚本本身也要提交：

```bash
git switch -c notes-migration rewrite
mkdir -p tools/notes-migration-2026-09
# path-map.tsv 由第 4 章生成；move.sh 见附录 A
bash tools/notes-migration-2026-09/move.sh
```

执行要点：

1. **目标目录存在与否决定 `git mv` 的语义**。`git mv A B` 在 `B` 已存在时会把 `A` 移动成 `B/A`。
   因此脚本必须先移动"要成为目录本身"的路径（`evidence/`、`concepts/`、`study/`），
   再移动其中的文件，不要预先 `mkdir` 目标。
2. **未跟踪目录用普通 `mv`**。`archive_bak/`、`.design/`、`.review/` 这类内容不在索引里，
   `git mv` 不适用；用 `mv` 移动到归档位置，保持它们的忽略状态。
3. **学习笔记按 D7 先纳入跟踪**。在 `notes-migration` 分支上先提交一次
   `git add notes/study && git commit -m "chore(notes): 迁移前保全学习笔记"`，
   然后再整体改名，否则 176 个文件永远进不了版本库，回滚也没有依据。
4. **`.review` 归档**：`mv notes/rewrite/fork-syscall-rewrite/.review
   .review/archive/notes-fork-syscall-rewrite-2026-09/`。
5. **清空空目录**：`find notes -type d -empty -delete`，随后确认 `notes/` 只剩空壳或直接删除。

提交前的门禁（必须全部通过才允许提交）：

```bash
# 只暂存迁移涉及的路径，禁止 git add -A（会把本计划、tmp 残留等无关文件卷进改名提交）
git add -A rewrite-notes redesign-notes study-notes notes
git status --short | grep -v '^R' | wc -l      # 期望 0：全部是改名记录
git diff --cached --numstat | awk '$1!=0 || $2!=0' | wc -l   # 期望 0：没有增删行
find notes -type f | wc -l                     # 期望 0：旧树已清空
find rewrite-notes redesign-notes study-notes -type f | wc -l  # 与迁移前总数相符（允许归档扣除）
```

提交信息模板：`refactor(notes): 纯改名迁移 notes 三区到仓库根（无语义改动）`。

### P2 机械引用替换提交

只做"旧前缀到新前缀"的字符串替换，全部是单行内替换，行数不变：

```bash
# 2.1 路径前缀（os/、notes/ 内部、AI-chats、.trae/documents 等）
#     排除项：minix3（原始 C 代码不动）、migrate_notes_plan（本计划保留历史路径）、
#     AI-chats/daily.todo.md（历史任务记录，按 R10 保留原样）
git grep -l -F 'notes/rewrite/fork-syscall-rewrite/' -- . \
    ':!minix3' ':!migrate_notes_plan' ':!AI-chats/daily.todo.md' \
  | while IFS= read -r f; do
      sed -i 's|notes/rewrite/fork-syscall-rewrite/|rewrite-notes/|g' "$f"
    done

# 2.2 根部杂项与 concepts（按 4.2、4.1 表逐个处理，避免误伤同名子串）
sed -i 's|notes/rewrite/concepts/|rewrite-notes/concepts/|g' $(git grep -l 'notes/rewrite/concepts/')
# project-plan.md 等 13 个文件：由 path-map.tsv 驱动替换为 rewrite-notes/misc/...

# 2.3 学习笔记与重设计（重设计需要主题映射，见 path-map.tsv；
#     不能简单把 notes/redesign/ 换成 redesign-notes/，因为每个文件进了不同主题目录）
sed -i 's|notes/study/|study-notes/|g' $(git grep -l 'notes/study/')
# notes/redesign/{endpoint_redesign,ipc-improve}.md 等按 4.4 表逐个映射

# 2.4 锚点基线前缀（保持行数）
sed -i 's|notes/rewrite/fork-syscall-rewrite/|rewrite-notes/|g' \
    tools/anchor-suspect-baseline.txt tools/anchor-unresolved-baseline.txt
```

门禁：

```bash
git grep -c 'notes/rewrite/fork-syscall-rewrite' -- . \
    ':!minix3' ':!migrate_notes_plan' ':!AI-chats/daily.todo.md'                            # 期望 0
git grep -n 'notes/redesign/' -- . ':!minix3' ':!migrate_notes_plan'                        # 期望 0
wc -l tools/anchor-suspect-baseline.txt tools/anchor-unresolved-baseline.txt               # 期望 874 / 6609 不变
```

提交信息模板：`refactor(notes): 机械替换迁移路径引用（仅路径字符串）`。

### P3 文档链接与旧编号修复提交

这一阶段处理第 1.5 节的两类既存断链，以及相对链接的重新计算。
推荐把检查与修复合成一个脚本（附录 B），它的输入是 `path-map.tsv`：

```bash
python3 tools/notes-migration-2026-09/relink.py --check rewrite-notes redesign-notes study-notes
python3 tools/notes-migration-2026-09/relink.py --write rewrite-notes redesign-notes study-notes
python3 tools/notes-migration-2026-09/relink.py --check rewrite-notes redesign-notes study-notes
```

修复规则：

1. **同树兄弟链接不动**。阶段目录之间的 `../02-stage-vm/...`、`../00-master-plan/...`
   在树整体去掉一层包装后依然成立，脚本不应改写它们。
2. **跨树链接重算**。指向 `concepts/`、`minix3/`、`os/`、`redesign-notes/` 的相对链接
   在树深度变化后 `../` 层数变化，脚本以"新路径逐文件解析"为准重写。
3. **旧编号替换**按附录 C 的别名表逐条处理，只改指向已重排阶段的旧路径，
   不改写正文里讨论历史的描述（例如 `00-master-plan/README.md` 里的"旧主线"说明本身是历史记录，
   保持原样）。
4. **无法解析的链接不猜**。脚本输出"无法解析清单"，由执行者逐条判断：
   目标确实不存在（如 `review/codex/...`、`deep-analysis/...`）就改为指向现行等价文档，
   或改成普通文字说明；判断结果写入 `rewrite-notes/misc/migration-decisions.md`。

门禁：链接检查脚本退出码 0，或"无法解析清单"为空且所有例外都在决策记录中有条目。

提交信息模板：`docs(notes): 修复迁移后相对链接与旧阶段编号引用`。

### P4 工具链适配提交

按下列顺序改，先改被依赖的解析逻辑，再改使用说明：

| 文件 | 改动 |
|------|------|
| `tools/review-init.sh` | 路径解析同时支持 `rewrite-notes/`、`redesign-notes/`、`study-notes/` 三棵树；模块取树名，阶段取第一级目录；状态目录带阶段（D15） |
| `tools/review-gate-check.sh` | `MODULE_DIR` 的解析与 `review-init.sh` 一致；设计快照推断复用同一函数 |
| `tools/design-coverage-check.sh` | 参数从"模块名"改为"树路径或 `rewrite`"；用法示例更新 |
| `tools/design-index-update.sh` | 使用说明与断言里的 `notes/rewrite/{module}/{stage}/.design` 改为 `{tree}/{stage}/.design` |
| `tools/doc-style-lint.sh` | 增量模式的 diff 范围 `-- notes/rewrite` 改为三棵树；默认目录模式不变 |
| `tools/review-state-validate.py` | 路径白名单加入三个新根目录 |
| `tools/coverage-extract/coverage-extract.py` | 只改文档字符串中的示例路径；确认参数未硬编码 |
| `tools/check-review-rules.sh` | 第 147 行对 `.design` 路径的断言改为新写法 |

每个脚本改完必须跑自测与一次真实调用：

```bash
bash tools/anchor-resolve.sh --self-test
bash tools/anchor-migrate.sh --self-test
bash tools/unsafe-audit.sh --self-test
bash tools/design-coverage-check.sh rewrite --stage 01-stage-kernel
bash tools/review-init.sh claude rewrite-notes/01-stage-kernel/03-kmain-cstart.md m3
bash tools/doc-style-lint.sh --dir rewrite-notes/01-stage-kernel
```

门禁：`tools/review-init.sh` 生成的 `.review/{tool}/rewrite/01-stage-kernel/03-kmain-cstart/`
目录与 `scan.md`、`structure.md`、`SYMBOLS.md` 路径符合 D15；`git status` 中不出现意外文件。

### P5 规则集三端同步提交

先改规范源，再生成派生，最后校验：

```bash
# 5.1 规范源（prompt/）
#   prompt/README.md            模块定义与同步说明
#   prompt/review-rules/review-process.md      模块解析规则、双写路径
#   prompt/review-rules/review-patterns.md     命令示例中的路径
#   prompt/review-rules/review-doc-checklist.md 命令示例中的路径
#   prompt/skill/review-{coverage,doc,patterns,process}-skill.md
#   prompt/skill/review-agent-ide.md
#   prompt/todo_plan.md          锚点基线路径
# 5.2 手工维护的两处派生（生成脚本不管它们）
#   .claude/rules/review-core.md、review-process.md
#   .claude/skills/review-scan/**
#   .codex/skills/review-scan/**
# 5.3 再生成其余派生
tools/generate-derived-skills.sh
tools/check-review-rules.sh
tools/diff-trae-skills.sh
tools/generate-derived-skills.sh --check
```

模块语义的新表述建议统一为：

> `{module}` 取目标文档所在工作树的根目录名（`rewrite`、`redesign`、`study`）；
> `{stage}` 取工作树根下的第一级目录名（例如 `01-stage-kernel`）；
> 状态目录为 `.review/{tool}/{module}/{stage}/{doc-stem}/`。

门禁：`tools/check-review-rules.sh` 退出码 0；`tools/generate-derived-skills.sh --check`
报告无漂移；`git grep -n 'notes/rewrite/{module}'` 在 `prompt/` 与派生目录中为 0。

### P6 收尾提交

1. 重写 `rewrite-notes/README.md`：以上游 `00-master-plan/README.md` 的阶段表为准，
   列 00 到 19 目录、`concepts/`、`coordination/`、`evidence/`、`misc/` 的用途与入口文档；
   不再保留 fork 时代的"阶段 1-5"叙述。
2. 重写 `redesign-notes/README.md`：说明探索区定位、主题目录与未来分支（`vm/`）。
3. `study-notes/` 增加一行状态说明：内容冻结，精简任务另立（写在 `study-notes/misc/README.md`）。
4. 仓库根 `README.md` 的目录说明加入三个新目录（一行即可）。
5. `.gitignore` 复核：`**/.design/`、`**/tmp_design_and_todo/`、`*.bak`、`.review/` 的匹配在新路径下依旧生效；
   如把证据目录纳入大小策略，增加对应规则。
6. 基线复核：`tools/anchor-resolve.sh --check` 抽样跑若干文档（含已迁移路径），
   确认前缀替换后仍能解析；如发现解析失败集中出现，回到 R13 的约束检查。
7. 提交迁移工具目录 `tools/notes-migration-2026-09/`（映射文件、脚本、清单、决策记录），
   使整个迁移可审计、可复现。

提交信息模板：`docs(notes): 迁移后索引与归档收尾`。

### P7 独立验收

由**未参与迁移**的 AI 会话执行，输入是 P0 的清单与 P6 的提交范围。
验收步骤与期望结果见第 9 章。任何一项不通过，整批退回对应阶段修正后重跑该项。

---

## 7. 多 AI 并行协作协议

### 7.1 原则

1. **P1 与 P2 只允许一个写者**。改名与全局前缀替换具有全局副作用，
   并行执行必然互相覆盖；这两步由一个会话串行完成。
2. **P3 之后可以并行，但按文件集切分**。不同工作流不触碰同一文件，
   合并时以 `path-map.tsv` 与编辑范围为准做核对。
3. **每个工作流有独立的输入、输出与门禁**，产物写进
   `tools/notes-migration-2026-09/reports/{工作流名}.md`，包括命令与输出摘要。
4. **共享工作树时先看再改**。开工前 `git status --short` 必须干净；
   发现他人未提交改动立即停止并报告，不做"顺手合并"。

### 7.2 工作流切分

| 工作流 | 范围 | 前置 | 产出 | 门禁 |
|--------|------|------|------|------|
| W1 核心搬迁 | P1、P2（全仓路径替换） | P0 完成 | 两个提交 + path-map.tsv | 第 6 章 P1、P2 门禁 |
| W2 源码注释 | `os/**` 的锚点替换与复核 | W1 | 86 个文件的替换报告 | `git grep -n 'notes/rewrite' -- os/` 为 0 |
| W3 工具链 | `tools/**`（除迁移目录） | W1 | P4 提交 | 工具自测与真实调用全过 |
| W4 规则三端 | `prompt/**`、`.claude/**`、`.codex/**`、`.trae/**`、`CLAUDE.md`、`AGENTS.md` | W1 | P5 提交 | `check-review-rules.sh` 与派生校验全过 |
| W5 文档链接 | 三棵树内 `*.md` | W1 | P3 提交 | 链接检查脚本退出码 0 |
| W6 学习与重设计树 | `study-notes/**`、`redesign-notes/**` | W1 | 主题落位与 README | 专属目录内容清单一致 |

W2 到 W6 可以并行执行；W3 与 W4 有交叉点（`check-review-rules.sh` 的断言涉及规则文本），
合并顺序固定为 W2 → W3 → W4 → W5 → W6，后合并者先 rebase 再跑自己的门禁。

### 7.3 工作树隔离（可选）

仓库已有 `.wt/` 忽略目录与 `tools/claim.sh` 工作树约定，可以直接复用：

```bash
git worktree add .wt/migrate-w2 -b migrate/w2-notes-migration notes-migration
git worktree add .wt/migrate-w3 -b migrate/w3-notes-migration notes-migration
# 各自提交后回到主工作树按 7.2 顺序合并
```

隔离的收益是并行会话互不干扰，代价是合并冲突需要人工处理。
由于各工作流的文件集不重叠，冲突预期只出现在共享的 `path-map.tsv` 与迁移报告目录，
把这两个位置划归 W1 独占即可。

---

## 8. 风险登记册

| 编号 | 风险 | 触发条件 | 影响 | 处置 |
|------|------|----------|------|------|
| R1 | 未跟踪内容丢失 | 执行 `git clean` 或直接删除旧目录 | 176 个学习笔记、40 个备份、1159 个设计快照不可恢复 | P0 先打压缩包与清单；学习笔记先提交再搬（D7） |
| R2 | 615 MB 大文件阻塞推送 | 推送 `rewrite` 分支 | 远端拒绝，协作中断 | 独立任务 D14；迁移本身不再新增超大文件 |
| R3 | 改名历史断裂 | 同一次提交既改名又改内容 | `git log --follow`、`git blame` 质量下降 | P1 纯改名、P2 起才改内容；门禁检查 diff 只有改名记录 |
| R4 | 目标目录已存在导致嵌套 | 预先 `mkdir` 目标后执行 `git mv` | 产生 `rewrite-notes/evidence/evidence` 之类错误层级 | 脚本先移动"目录整体"，移动前后各跑一次结构核对 |
| R5 | 并行会话互相覆盖 | 多人同时编辑同一文件 | 修改丢失或冲突 | 7.1 的单写者与文件集切分 |
| R6 | 遗漏引用 | 前缀替换只覆盖部分消费方 | 审查工具找不到文档，`os/` 注释悬空 | 第 5 章矩阵逐项签字；R1/R5 用 `git grep` 计数归零做硬门禁 |
| R7 | 链接检查误报 | 代码块中的示例链接、外链、锚点 | 无法区分真实断链 | 附录 B 脚本跳过代码块、外链与纯锚点；例外逐条登记 |
| R8 | 锚点基线失效 | 迁移期间文档行数变化 | 待复核锚点清单与文档错位 | 所有迁移编辑限单行替换（R13 约束）；否则在同提交重推基线 |
| R9 | 状态与审查流水线中断 | 模块语义改变但工具未同步 | `review-init.sh` 生成错位路径 | P4 与 P5 必须同批验收；D15 的状态路径用真实调用验证 |
| R10 | 规则派生漂移 | 只改源不改派生或反之 | 三端规则行为不一致 | `tools/generate-derived-skills.sh --check` 与 `check-review-rules.sh` 双门禁 |
| R11 | 旧编号修复改错对象 | 把历史叙述中的旧编号也替换了 | 文档历史失真 | 别名表只作用于路径形态（带 `/`），叙述性文字人工复核 |
| R12 | `notes/` 残留目录被外部脚本引用 | 仓库外还有脚本或会话记忆引用旧路径 | 外部工具失效 | 决策记录里写明新路径；根 README 增加一行目录说明 |
| R13 | 忽略规则在新路径失效 | 归档位置改变 | `.design`、备份文件意外入库 | P6 逐条核对 `.gitignore` 模式 |
| R14 | mdBook 构建受影响 | `book/` 未来引用旧路径 | 电子书抽取阶段才发现断链 | 本计划不动 `book/`；后续抽取任务以新树为唯一来源 |

---

## 9. 验收标准

由独立会话在 P7 逐条执行，结果写入 `tools/notes-migration-2026-09/reports/acceptance.md`。

| 序号 | 检查项 | 命令 | 期望 |
|------|--------|------|------|
| A1 | 旧前缀清除 | `git grep -c 'notes/rewrite/fork-syscall-rewrite' -- . ':!minix3' ':!migrate_notes_plan' ':!AI-chats/daily.todo.md'` | 0 |
| A2 | 旧树引用清除 | `git grep -n 'notes/redesign/\|notes/study/' -- . ':!minix3' ':!migrate_notes_plan'` | 仅剩决策记录中的白名单条目（`AI-chats/daily.todo.md` 属白名单） |
| A3 | 源码注释归位 | `git grep -n 'notes/rewrite' -- os/` | 0 |
| A4 | 相对链接全通 | 附录 B 脚本对三棵树 `--check` | 退出码 0，或例外列表为空且有决策记录 |
| A5 | 纯改名提交质量 | `git log --stat -1 <P1 提交>` | 全部为 rename 记录，增删行数为 0 |
| A6 | 文件守恒 | 迁移前清单与迁移后 `find` 计数对账 | 除归档与非跟踪忽略项外一一对应 |
| A7 | 学习笔记保全 | `git ls-files study-notes \| wc -l` | 大于等于迁移前纳入跟踪的数量；本地文件数为 187 减去归档项 |
| A8 | 工具自测 | `tools/anchor-resolve.sh --self-test` 等自测命令 | 全部通过 |
| A9 | 规则一致 | `tools/check-review-rules.sh`、`tools/generate-derived-skills.sh --check` | 退出码 0 |
| A10 | 审查闭环演练 | `tools/review-init.sh claude rewrite-notes/01-stage-kernel/03-kmain-cstart.md m3` | 生成 `.review/{tool}/rewrite/01-stage-kernel/03-kmain-cstart/` 下三个产物 |
| A11 | 覆盖率工具演练 | `python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel --rust-dir os --c-dir minix3/minix/kernel --doc-file 03-kmain-cstart.md` | 正常产出，无路径错误 |
| A12 | 编译未受影响 | `cargo check`（仓库根 `os/`） | 通过 |
| A13 | 工作树干净 | `git status --short` | 空 |
| A14 | 标签齐备 | `git tag \| grep notes-migration` | 两个标签都存在 |

补充说明：A1 中 `migrate_notes_plan/` 目录内的旧路径引用是历史任务描述，保留不替换；
A12 的 `cargo check` 只验证迁移没有误改 Rust 代码（注释替换是纯文本，不影响编译）。

---

## 10. 回滚预案

| 阶段 | 回滚动作 | 数据影响 |
|------|----------|----------|
| P1 提交后 | `git reset --hard pre-notes-migration-20260922`；未跟踪内容用 P0 压缩包恢复 | 无内容损失 |
| P2 到 P6 任一提交后 | `git revert` 对应提交，或整体 `git reset --hard` 到 P1 提交再挑选重做 | 需要重跑后续门禁 |
| 已合并回 `rewrite` 后 | `git revert -m 1 <合并提交>`；若合并是快进则回到标签 | 同上 |
| 未跟踪内容误删 | 解压 `/tmp/opencode/notes-untracked-20260922.tar.gz`，用清单校验 | 依赖 P0 完整执行 |
| 历史清理（D14）出错 | 从 P0 的 `git bundle` 恢复整个分支 | 需要重新执行清理 |

回滚后必须重跑 A1、A4、A8 三项，确认仓库回到一致状态。

---

## 11. 本计划不覆盖的后续工作

1. **重写区内容整理**：`00-master-plan/05-phase1-pm-guide.md` 到 `10-phase5-sched-guide.md`
   仍以 fork 主线五阶段叙述，需要按启动顺序主线重写；`15-todo-fixes.md`、各阶段 `todo.md`
   的状态需要一次全量对账。建议用 `style-fix` 与 `todo-fix` 类任务分批处理。
2. **学习笔记精简**：`study-notes/**/archive/tmp_*.c.md` 是逐文件 AI 笔记，
   是否保留按主题决定；精简前先做一次目录清单评审。
3. **615 MB 证据历史清理**：见 D14。建议流程为
   `git bundle` 备份 → `git filter-repo --strip-blobs-bigger-than 10M`（仅本地未推送历史）
   → 把原始日志移出仓库并生成压缩摘要 → 更新 `evidence/` 下的引用说明。
4. **电子书抽取**：`book/` 的内容建设从 `rewrite-notes/` 抽取，阶段顺序以
   `rewrite-notes/00-master-plan/README.md` 为准，文档内部链接以 P3 修复后的链接为基准。
5. **重设计分支**：`redesign-notes/vm/` 预留给 `vm_in_kernel` 方向；
   开启探索分支前先把 `redesign-notes/architecture/` 下的既有分析收敛一次，避免同一主题多稿并存。
6. **文档锚点复核债**：`tools/anchor-suspect-baseline.txt` 记录的 868 处可疑锚点、
   `tools/anchor-unresolved-baseline.txt` 记录的无法解析清单，都是迁移前遗留的语义复核债，
   本次只做路径前缀维护，不代偿处理。

---

## 附录 A 迁移脚本骨架

`tools/notes-migration-2026-09/move.sh`（P1 使用；`set -euo pipefail`，先干跑再真跑）：

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

DRY_RUN="${DRY_RUN:-1}"
run() { if [[ "$DRY_RUN" == "0" ]]; then "$@"; else printf '[dry-run] %s\n' "$*"; fi; }

# 目标根（注意：不要预先创建 study-notes，见 R4）
run mkdir -p rewrite-notes redesign-notes

# 1) 阶段目录、master-plan、evidence 整体进入 rewrite-notes
for name in 00-master-plan 01-stage-kernel 02-stage-vm 03-stage-rs 04-stage-pm \
            05-stage-vfs 06-stage-sched 07-stage-ds 08-stage-is 09-stage-init \
            10-stage-mib 11-stage-devman 12-stage-input 13-stage-ipc \
            14-stage-runtime 15-stage-fs 16-stage-drivers 17-stage-net \
            18-stage-commands 19-stage-integration evidence; do
  run git mv "notes/rewrite/fork-syscall-rewrite/${name}" "rewrite-notes/${name}"
done

# 2) 重写区根部杂项（逐文件，避免同名冲突）
run mkdir -p rewrite-notes/misc rewrite-notes/coordination
run git mv notes/rewrite/README.md rewrite-notes/README.md
for f in RECONSTRUCTION-PRINCIPLES.md arch_mapping.md elf-loader.md invariant.md \
         ipc-sendrec.md minimal-skeleton.md misc.md modern-hardware-and-rust.md \
         project-plan.md project-structure.md rewrite-strategy.md rewrite.md \
         vertical-slice-strategy.md; do
  run git mv "notes/rewrite/${f}" "rewrite-notes/misc/${f}"
done
run git mv notes/rewrite/concepts rewrite-notes/concepts

# 3) fork 树根部协作文档（27 个，完整清单见第 4.3 节）
run git mv notes/rewrite/fork-syscall-rewrite/README.md \
        rewrite-notes/misc/legacy-fork-syscall-index.md
for f in edge_todo.md edge_todo_archive.md edge1.md edge2.md edge3.md edge4.md \
         new_edge1.md new_edge2.md new_edge3.md new_edge4.md claim-prompt.md \
         NK4A-HANDOFF-STATUS.md NK4A-QWEN-OPENING-PROMPT.md NK4A-QWEN-WORKLOG.md \
         NK4A-REVIEW-REPORT.md NK4A-TODO.md NK4B-OPENING-PROMPT.md NK4B-TODO.md \
         NK4B-WORKLOG.md HANDOFF-NK4A-boot-first-light.md HANDOFF-NK4A-review-and-redo.md \
         new_todo_deepseek.md new_todo_glm.md new_todo_muse.md new_todo_qwen.md new_todo_HY4.md; do
  run git mv "notes/rewrite/fork-syscall-rewrite/${f}" "rewrite-notes/coordination/${f}"
done

# 4) 20-redesign 的两个文件并入 redesign-notes/architecture
run mkdir -p redesign-notes/architecture redesign-notes/ipc redesign-notes/fork
run git mv notes/rewrite/fork-syscall-rewrite/20-redesign/rs-cross-layer-pollution.md \
        redesign-notes/architecture/rs-cross-layer-pollution.md
run git mv notes/rewrite/fork-syscall-rewrite/20-redesign/tocutou-and-distributed-consistency.md \
        redesign-notes/architecture/tocutou-and-distributed-consistency.md

# 5) 重设计文档（第 4.4 节整表）
run git mv notes/redesign/redesign.md redesign-notes/README.md
# ... 其余 9 个文件按 4.4 表逐条 git mv ...

# 6) 学习笔记（先执行过 git add notes/study 并提交）
run git mv notes/study study-notes
run mkdir -p study-notes/misc
for f in daily.md learning-path.md minix_structure.md progress.md roadmap.md; do
  run git mv "study-notes/${f}" "study-notes/misc/${f}"
done

# 7) 未跟踪归档（普通 mv）
run mkdir -p rewrite-notes/archive .review/archive
run mv notes/rewrite/archive_bak rewrite-notes/archive/legacy-fork-bak
run mv notes/rewrite/fork-syscall-rewrite/.review \
       .review/archive/notes-fork-syscall-rewrite-2026-09

# 8) 清空旧树
run find notes -type d -empty -delete
```

`path-map.tsv` 的类型列为 `git-mv` 或 `mv`，脚本从映射文件生成，而不是把清单散落在代码里；
第 6 章 P1 的门禁对脚本执行结果做结构核对。

## 附录 B 链接检查与修复脚本（要点）

`tools/notes-migration-2026-09/relink.py`。检查逻辑：

1. 遍历目标树下的 `*.md`，跳过代码围栏（以三个反引号或三个波浪号开头的行）内的内容。
2. 提取 `[文字](目标)` 中不是 `http`、`https`、`mailto`、纯 `#` 的目标，去掉 `#锚点` 后再判断。
3. 以文件所在目录为基准解析目标；存在即通过，不存在则记录 `文件:行:目标`。
4. 修复模式：对命中 `path-map.tsv` 的旧目标，按新路径重算相对路径并回写；
   对旧阶段编号目标，先套用附录 C 的别名表，再重算。
5. 输出两类报告：已修复清单、无法解析清单。无法解析的条目不做自动改写。

脚本自身要跑一个自测样例（`tools/notes-migration-2026-09/selftest/` 下放三个小文件，
覆盖"代码块内链接不检、相对路径重算、别名替换"三种情况），形式与
`tools/anchor-resolve.sh --self-test` 一致。

## 附录 C 旧阶段编号别名表

| 旧引用形态 | 新目标 | 备注 |
|------------|--------|------|
| `notes/rewrite/fork-syscall-rewrite/01-stage-pm/` | `rewrite-notes/04-stage-pm/` | 旧编号时代 PM 在第一位 |
| `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/` | `rewrite-notes/01-stage-kernel/` | 规则与 `.trae` 文档中最多，71 处 |
| `notes/rewrite/fork-syscall-rewrite/04-stage-vfs/` | `rewrite-notes/05-stage-vfs/` | — |
| `notes/rewrite/fork-syscall-rewrite/05-stage-sched/` | `rewrite-notes/06-stage-sched/` | — |
| `deep-analysis/fork-all-layers-deep-analysis.md` | `rewrite-notes/archive/legacy-fork-bak/fork-all-layers-deep-analysis.md`（未跟踪）或改写为现行等价文档 | 目标未跟踪，引用需人工判断 |

同一张表也用于无前缀形态（例如 `.trae/documents/` 里的
`notes/rewrite/fork-syscall-rewrite/03-stage-kernel/todo.md`）。
表中"新目标"只是路径部分，实际替换时若原引用带文件名，需要把文件名一并保留。

## 附录 D 迁移前清单命令（P0 用）

```bash
# 目录结构与文件数
find notes -maxdepth 4 -type d | sort
find notes -type f | wc -l
git ls-files notes | wc -l

# 引用分布
git grep -l 'notes/rewrite' -- . ':!minix3' | wc -l
git grep -o 'fork-syscall-rewrite' -- . ':!minix3' | wc -l

# 未跟踪与忽略
git status --porcelain -uall notes/ | wc -l
git check-ignore -v notes/rewrite/fork-syscall-rewrite/01-stage-kernel/.design/01-design.v1.md

# 分支与提交
git branch -vv
git log --oneline -1
git tag

# 基线行数
wc -l tools/anchor-suspect-baseline.txt tools/anchor-unresolved-baseline.txt
```

清单结果贴进 `tools/notes-migration-2026-09/reports/inventory.md`，作为 A6 文件守恒的对照基准。
