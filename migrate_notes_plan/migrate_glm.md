# notes 目录迁移计划（glm 独立方案）

- 作者：glm
- 日期：2026-09-22
- 盘点基线：`rewrite` 分支工作树（HEAD = 759d3463e 附近，含未提交改动，见 Phase 0）
- 独立性声明：本文档未读取 `migrate_notes_plan/` 下其他模型的方案（`migrate_deepseek.md`、`migrate_muse.md`、`migrate_qwen.md`），全部结论来自本节末尾列出的实地盘点命令输出。

---

## 一、结论先行

1. **布局**：推荐根目录三分——`rewrite-notes/`、`redesign-notes/`、`study-notes/`，`book/` 已在根目录且不动，`notes/` 目录整体消失。这就是你提出的方向，我认可，理由见决策 D1。
2. **tag**：需要。当前仓库一个 tag 都没有（`git tag | wc -l` = 0），迁移前打附注标签 `pre-notes-migration`，验收通过后打 `post-notes-migration`，这是最便宜的回滚锚点。
3. **三个决定性发现**（不看这三条就动手会出事故）：
   - `notes/rewrite/fork-syscall-rewrite/` 根下住着**活跃的多 AI 协作区**：27 个根级工作文件（`NK4A-*`、`NK4B-*`、`HANDOFF-*`、`edge*.md`、`new_todo_*.md`、`claim-prompt.md`）加 `evidence/` 下 13 个证据目录。NK4B 迭代正在进行（近期提交均为 nk4b）。**迁移必须等当前迭代收口，并设冻结窗口**。
   - **工具链与路径强耦合**：`tools/check-review-rules.sh:147` 对规则文本里的字面路径做断言；`tools/doc-style-lint.sh:207` 把文风门的 diff 范围硬编码为 `notes/rewrite`；`tools/review-init.sh:126-129` 靠 `notes/rewrite/` 前缀推导 module。漏改任何一处，对应的质量门会**静默失效**（不是报错，是不再检查）。
   - **`.design/`、`.review/`、`notes/study/` 全部不在版本库里**（`.gitignore:27` 忽略 `.review/`，`.gitignore:34` 忽略 `**/.design/`；`notes/study/` 是未跟踪目录）。tag 保护不到它们，迁移前必须单独做文件系统快照。
4. **规模**：`notes/` 下共 2326 个文件（其中 2084 个 markdown）。全仓（排除 `.git`、`tmp/`、`target/`、`minix3/`、日志）有 **427 个文件、2748 处**写着 `notes/rewrite` 路径。其中需要改写的约 180 个文件，其余是按本方案政策冻结的历史产物（见 D9）。

---

## 二、现状盘点（证据锚点）

以下数字全部来自 2026-09-22 在工作树上实测的命令，复验命令附在括号里。

### 2.1 四区角色与体量

| 区域 | 路径 | 体量 | git 状态 | 现状判断 |
|------|------|------|----------|----------|
| 书稿 | `book/`（仓库根） | mdbook 骨架 + 构建产物 | 已跟踪 | 自包含，CI（`.github/workflows/mdbook.yml`）只构建 `./book`，与 notes 无耦合，**本迁移不碰** |
| 学习笔记 | `notes/study/` | 187 个文件，按主题分目录（arch/boot/interrupt/ipc/pm/process/services/syscall/vfs/vm） | **全部未跟踪** | 内容多为 AI 生成，含 `.bak`/`.backup`/`archive/` 垃圾；去留是独立决策（见 D7） |
| 重写文档 | `notes/rewrite/` | 14 个散文件 + `concepts/`（6 个 md）+ `archive_bak/`（6 个，未跟踪）+ `fork-syscall-rewrite/`（主体，约 2100 个文件） | 除 `archive_bak/` 外已跟踪 | 活跃工作区，路径四到五层深（691 个 md 在 4 层、1266 个在 5 层） |
| 再设计 | `notes/redesign/` | 12 个文件（含 2 个字节级重复的 `.backup`） | 已跟踪 | 已有实质内容，未来承接 OS 方向探索 |

（`find notes -type f | wc -l`；`find notes -name '*.md' -printf '%d\n' | sort -n | uniq -c`；`git status --porcelain notes/`）

### 2.2 `notes/rewrite/fork-syscall-rewrite/` 内部三类内容

这个目录里混着三种性质完全不同的东西，迁移策略必须分开处理：

1. **stage 文档树（主体，应 rigid 移动）**：`00-master-plan/`（16 个 md，fork 时代的总规划）、`01-stage-kernel` 到 `18-stage-commands`（每个含 `.design/` 快照，部分含 `draft/`、`archive/`、`plan.md`、`todo.md`，其中 plan 17 份、todo 18 份）、`19-stage-integration/`（8 个 md）、`20-redesign/`（2 个 md）。
2. **活动协作区（多 AI 工作流的中枢）**：根下 26 个工作文件（NK4A/NK4B 的 opening prompt、worklog、TODO、HANDOFF，`edge1-4`、`new_edge1-4`、`edge_todo*`、`new_todo_{deepseek,glm,muse,qwen,HY4}`、`claim-prompt.md`）+ `evidence/`（13 个按日期和任务命名的证据目录）+ 内嵌 `.review/`（gitignore 忽略，Trae 时代产物）。其中 21 个文件里写着旧路径引用。
3. **历史产物**：`doc_rerank_*.md` 共 84 份（各 stage 一到五份，AI 重排评审报告）、`archive/` 内 41 个 md、`archive_bak/` 6 个文件。这些是记录，不是活文档。

（`ls notes/rewrite/fork-syscall-rewrite`；`ls notes/rewrite/fork-syscall-rewrite/*/plan.md | wc -l`；`find notes/rewrite -name 'doc_rerank*' | wc -l`）

### 2.3 引用面全景

`grep -rIl 'notes/rewrite'`（排除 `.git`、`tmp/`、`target/`、`minix3/`、`*.log`）命中 **427 个文件、2748 处**，分区如下：

| 分区 | 文件数 | 性质 | 处置 |
|------|--------|------|------|
| notes 树内部 | 295 | 文档互相引用路径 | 随树移动后**统一改写**（含冻结报告，理由见 D9） |
| `os/` 代码注释 | 87 | crate 的 `//!` 模块文档指向设计文档 | 独立 commit 改写 + 构建门 |
| 规则与技能（`prompt/`、`.claude/`、`.codex/`、`.trae/`、`.agents/`） | 28 | review 工作流规范 | **必须改**，且要走派生同步链（Phase 3） |
| `tools/` | 9 | 7 个脚本 + 2 个锚点基线文件 | **必须改**，见 2.4 强耦合清单 |
| `AI-chats/` | 4 | `daily.todo.md` 活文档 + 2 份可复用的 rerank prompt + demo 记录 | 前 3 改，demo 冻结 |
| `migrate_notes_plan/` 其他方案 | 3 | 将被本批次规划取代 | 不动 |
| 根目录 | 2 | `check_references.sh`、`README.md` | 改 |
| `.review/`（仓库根） | 1104 | 评审中间产物，全部被 gitignore | **冻结不改**，靠映射文档兜底 |

补充两个口径：活着的 `STATE.md` 共 23 个，其中 10 个写着旧路径（清单见 Phase 5）；`CLAUDE.md` 7 处、`AGENTS.md` 3 处。

（`grep -rIo 'notes/rewrite' ... | wc -l`；分区 awk 统计命令见附录 A）

### 2.4 工具链强耦合清单（漏一处就静默失效）

| 位置 | 耦合方式 | 漏改后果 |
|------|----------|----------|
| `tools/check-review-rules.sh:147` | 断言 `prompt/review-rules/review-process.md` 里存在字面量 `notes/rewrite/{module}/{stage}/.design/` | 规则文本改了、检查器没改 → 恒红；反向漏改 → 规则门对新路径失明 |
| `tools/doc-style-lint.sh:207` | 文风门的 diff 范围硬编码 `git diff ... -- notes/rewrite` | 文风门对新路径**永久 no-op**，最危险的一处 |
| `tools/review-init.sh:126-129` | 靠 `notes/rewrite/` 前缀切出 module 名 | 新路径下解析失败或解析错层级 |
| `tools/design-coverage-check.sh:46`、`tools/review-gate-check.sh:105` | `MODULE_DIR="notes/rewrite/${MODULE}"` 硬编码 | Gate H 预检直接找不到目录 |
| `tools/coverage-extract/coverage-extract.py:15-25` | 硬编码 stage 目录列表（含 `fork-syscall-rewrite` 前缀） | 覆盖率提取失败 |
| `tools/anchor-suspect-baseline.txt`、`tools/anchor-unresolved-baseline.txt` | 每行以 `notes/rewrite/fork-syscall-rewrite/...:行号` 开头（工具生成） | 锚点基线对不上新路径，锚点门全红 |
| `prompt/` 与 `.claude/`、`.codex/`、`.trae/` 的派生关系 | `tools/generate-derived-skills.sh` 从 `prompt/skill/` 生成两套派生集，`check-review-rules.sh` 校验同步 | 只改派生件不改源 → 下次生成时回漂 |
| `.claude/settings.local.json` | 权限允许清单里嵌着旧路径的完整命令 | 仅缓存性质，**可不改**（新命令会重新走授权） |

### 2.5 迁移前就已经坏的东西（本迁移只记录、不顺手修）

1. `os/` 代码注释里有一处悬空引用 `notes/rewrite/fork-syscall-rewrite/fork-syscall-plan.md`——这个文件已不存在（`ls` 报 No such file）。机械替换前缀救不了它，需要人工判定向后指向哪份文档。
2. `check_references.sh` 的 `TARGET_FILES` 清单用的是过期文件名（如 `06-pagetable-struct.md`，实际文件是 `07-pagetable-struct.md`），说明路径腐烂早已发生。迁移只更新它的 `BASE_DIR`（`check_references.sh:26`），清单过期另立任务。
3. 文档间相对链接的既有断链情况未知——所以 Phase 0 要先跑一遍链接检查器取基线 B0，迁移后只要求"不新增断链"，不背历史包袱。

---

## 三、设计决策

每条按"问题 → 如果反着做会怎样 → 结论"给出。

### D1 目标布局：根目录三分，不保留 notes/ 伞目录

- **问题**：是 `rewrite-notes/`、`redesign-notes/`、`study-notes/` 三个根目录，还是保留 `notes/` 伞目录只删 `fork-syscall-rewrite` 层？
- **推理**：如果保留 `notes/`，层级问题只解决一半（4 层变 3 层），而 `notes/` 这个伞本身今天不承载任何东西——没有任何工具把 `notes/` 当整体寻址，它只是三个子目录名字的前缀。也就是说伞目录只收租（每条规范路径多一节、每次 grep 多一层目录），不提供便利。等 study 精简、book 独立之后，伞下更空。
- **结论**：根目录三分。规范文档路径从 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md`（4 节）降到 `rewrite-notes/01-stage-kernel/16-smp.md`（2 节）。

### D2 stage 目录名原样保留，禁止顺手重命名

- **推理**：`01-stage-kernel` 到 `18-stage-commands` 这些名字被数百份评审产物、AI 记忆、工具默认参数（`coverage-extract.py` 的 stage 列表）按名字引用。如果迁移同时改 stage 名，引用改写从"前缀替换"升级为"全网重连"，出错概率和验证成本都翻好几倍，而且两类变更混在一个 diff 里无法独立回滚。
- **结论**：本次迁移**只动目录骨架，不动任何 stage 编号和文件名**（唯一例外：两个 README 的位置调整，见映射表）。stage 重命名/重编号如果将来要做，单独立项。

### D3 `fork-syscall-rewrite` 层删除，内容不重组

- **推理**：这个目录名的语义（以 fork 为主线）早已废弃，实际内容（01 kernel → 02 vm → … → 18 commands）本来就是按启动顺序组织的，删掉这一层名字没有任何信息损失。反过来，如果在迁移时顺手把"内容也重新归类"（比如把 00-master-plan 挪进 archive），评审产物和 AI 记忆里的对应关系会全部悬空。
- **结论**：`00-master-plan/` 到 `19-stage-integration/` 原样 rigid 移到 `rewrite-notes/` 下。`00-master-plan/` 是 fork 时代规划这一事实，记入 MIGRATION.md 备忘，是否标档后续再议。

### D4 协作文件集中到 `rewrite-notes/workflow/`

- **问题**：26 个协作文件加 `evidence/` 是平铺在 `rewrite-notes/` 根，还是收进子目录？
- **推理**：如果平铺，stage 目录（20 个）会被 30 个工作文件淹没，而工作文件和概念文档的读者、生命周期完全不同。如果收进子目录，内部互引只要整体 rigid 移动就不受影响，绝对路径引用由改写器统一处理（见 Phase 2 的映射对机制，天然支持文件级映射，不需要为它单写 sed 规则）。
- **结论**：`rewrite-notes/workflow/` 收编 26 个根级协作文件 + `evidence/`；内嵌 `.review/` rigid 随树到 `rewrite-notes/.review/`（它被 gitignore，是搭车移动，事后可清理）。
- **附带**：fork 树的旧 `README.md` 内容本就是 `00-master-plan/` 的索引表，移到 `rewrite-notes/00-master-plan/README.md`；新根索引 `rewrite-notes/README.md` 在 Phase 5 重写。

### D5 `20-redesign/` 归入 `redesign-notes/`

- **推理**：`20-redesign/` 下 2 份文档（`tocutou-and-distributed-consistency.md`、`rs-cross-layer-pollution.md`）性质就是再设计探索，和 `notes/redesign/` 现有 12 份同类。如果留在 `rewrite-notes/` 下，"20-XX" 编号会冒充第 20 个 stage，语义误导。
- **结论**：移到 `redesign-notes/` 根（仅 2 个文件，链接重定基成本可忽略）。

### D6 散文件归 `rewrite-notes/misc/`

14 个 `notes/rewrite/*.md` 散文件按你定的规则（rewrite/redesign 二选一后进 misc）处置如下。**迁移期不改任何文件名**（misc.md 这个名字没有信息量，但改名留给后续整理任务，保持迁移 diff 可机械对账）：

| 文件 | 去向 | 备注 |
|------|------|------|
| `README.md` | 重写为 `rewrite-notes/README.md` | 新索引，Phase 5 产 |
| `RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/RECONSTRUCTION-PRINCIPLES.md` | 宪法性文档留根 |
| `rewrite.md` | `misc/` | 早期总览，已被 README 取代 |
| `rewrite-strategy.md` | `misc/` | |
| `vertical-slice-strategy.md` | `misc/` | |
| `project-plan.md` | `misc/` | |
| `project-structure.md` | `misc/` | |
| `arch_mapping.md` | `misc/` | |
| `elf-loader.md` | `misc/` | |
| `invariant.md` | `misc/` | |
| `ipc-sendrec.md` | `misc/` | |
| `minimal-skeleton.md` | `misc/` | |
| `misc.md` | `misc/misc.md` | 内容是异步消息表锁设计，后续整理时按内容改名 |
| `modern-hardware-and-rust.md` | `misc/` | 唯一的 redesign 候选，后续整理时裁决 |

`concepts/`（6 个 md）整体移到 `rewrite-notes/concepts/`——它是跨 stage 概念文档区，stage 文档里有指向它的相对链接（`../../../concepts/README.md`），同根移动后由链接重定基统一修复。

### D7 study / redesign / book

- `book/`：已在根目录，自包含，不碰。
- `notes/redesign/` → `redesign-notes/`，整体 rigid 移动；2 个 `.backup` 重复文件先 `cmp` 确认同字节后删除（Phase 0）。
- `notes/study/` → `study-notes/`，整体移动，**不在迁移里做内容精简**（删 `.bak`、并主题目录是独立任务）。未跟踪状态意味着：移动用 `mv` 而非 `git mv`，无历史可断；tag 覆盖不到它，靠 Phase 0 的 tar 快照兜底。你草图里的 `study-notes/minix3_concept/` 伞层建议不建——现有主题目录（arch/boot/ipc/…）本身就是概念分类，再加一层伞纯属搬运成本。

### D8 tag 与分支策略

- **推理**：如果开独立分支做迁移，多 AI 并行的日常工作（NK4 系列）会持续改 `notes/`，分支存活越久合并冲突越大。直接在 `rewrite` 上做、用冻结窗口把迁移压进一个 session，diff 干净、回滚点清晰。
- **结论**：
  1. Phase 0 收尾打 `pre-notes-migration`（附注标签，注明旧布局终点）。
  2. 全程在 `rewrite` 分支，约 6 个 commit（每 Phase 一个或两个）。
  3. 验收门全过后打 `post-notes-migration`。
  4. 回滚方案见 Phase 末尾。

### D9 冻结线：跟树走的一律改写，留在原地的一律冻结

- **推理**：如果对"历史报告保真"而把 84 份 doc_rerank、41 份 archive 排除在改写外，以后每次 grep 验收都要维护一份约 130 个文件的白名单，而白名单本身会成为新的腐烂源。这些报告跟随文档树一起移动，路径字符串跟树走、树已改名，字符串不改就全是死引用。
- **结论**：
  - **改写**：`rewrite-notes/`、`redesign-notes/`、`study-notes/` 三棵树内部全部路径字符串（含冻结报告）；Tier 1/2/3 活引用（规则、工具、os 注释、活工作文件）。
  - **冻结**：仓库根 `.review/` 全部 1104 个文件（它们不移动，是 gitignore 的工作产物）；`AI-chats/demo/`；`migrate_notes_plan/` 其他方案。
  - 兜底：`rewrite-notes/MIGRATION.md` 记录新旧路径映射，任何人（含 AI 记忆）拿着旧路径都能查到新位置。

### D10 未跟踪内容的保护

`git tag` 只保护已跟踪内容。`.design/`（每 stage 的设计快照，gitignore）、内嵌 `.review/`（gitignore）、`notes/study/`（未跟踪）都不在 tag 里。所以 Phase 0 增加一步 tar 快照，成本一条命令，把"回滚后设计快照去哪了"从悬念变成确定。

---

## 四、目标布局与映射

### 4.1 布局终稿

```
rewrite-notes/
├── README.md                  # 新索引（Phase 5 重写）
├── RECONSTRUCTION-PRINCIPLES.md
├── MIGRATION.md               # 新旧路径映射表（入版本库）
├── 00-master-plan/            # 原样（含旧 fork 索引 README）
├── 01-stage-kernel/ … 18-stage-commands/   # 原样（含 .design/、draft/、archive/、plan/todo）
├── 19-stage-integration/      # 原样
├── concepts/                  # 原样
├── misc/                      # 12 个散文件（名字不变）
├── workflow/                  # 26 个协作文件 + evidence/
└── .review/                   # 内嵌评审产物（搭车，gitignore）

redesign-notes/                # 原 notes/redesign 12 份 − 2 个 .backup + 原 20-redesign 2 份
study-notes/                   # 原 notes/study 187 个文件，原样
book/                          # 不动
```

### 4.2 映射规则（模式级）

改写器不使用四条死 sed，而是吃一份**目录/文件级映射对清单**（最长前缀优先），清单由 Phase 1 的 `git mv` 实际记录机械归并生成，人只审清单不改清单。这样 workflow/ 子目录、20-redesign 换树这类"非均匀映射"不需要任何特判：

```
notes/rewrite/fork-syscall-rewrite/evidence/…     → rewrite-notes/workflow/evidence/…
notes/rewrite/fork-syscall-rewrite/{26 个协作文件} → rewrite-notes/workflow/{同名}
notes/rewrite/fork-syscall-rewrite/README.md      → rewrite-notes/00-master-plan/README.md
notes/rewrite/fork-syscall-rewrite/.review/…      → rewrite-notes/.review/…
notes/rewrite/fork-syscall-rewrite/…              → rewrite-notes/…
notes/rewrite/{12 个散文件}                        → rewrite-notes/misc/{同名}
notes/rewrite/RECONSTRUCTION-PRINCIPLES.md        → rewrite-notes/RECONSTRUCTION-PRINCIPLES.md
notes/rewrite/concepts/…                          → rewrite-notes/concepts/…
notes/redesign/…                                  → redesign-notes/…
notes/study/…                                     → study-notes/…
```

### 4.3 改写对象与特例

| 层 | 范围 | 方式 |
|----|------|------|
| Tier 1 规则与工具 | `prompt/`（源）→ `generate-derived-skills.sh` → `.claude/`、`.codex/`、`.trae/`（派生）；`AGENTS.md`、`CLAUDE.md`；`tools/` 9 个文件；`check_references.sh` 的 `BASE_DIR` | 改源 → 跑派生脚本 → 跑 `check-review-rules.sh` 仲裁 |
| Tier 2 活工作区 | `AI-chats/daily.todo.md`、2 份 rerank prompt；10 个活 `STATE.md`；三棵树内部全量 | 映射对改写器一次跑完 |
| Tier 3 代码注释 | `os/` 87 个文件的 `//!` 路径引用 | 独立 commit + 构建门 |
| 特例（人工逐条） | 见下表 | 不许混进批量改写 |

特例清单（迁移中人工裁决，每条一行记录进 MIGRATION.md）：

1. `os/` 里指向已删除文档 `fork-syscall-plan.md` 的悬空注释——判定后继文档后手改。
2. `tools/review-init.sh:115-129`——路径解析逻辑重写：接受 `rewrite-notes/` 前缀，module 语义从"项目目录名"变为"stage 目录名"，用法文案同步。
3. `tools/check-review-rules.sh:147` 与 `prompt/review-rules/review-process.md`——字面断言与规则文本**同一 commit** 内成对修改，Step 0 预检命令变为 `ls rewrite-notes/{stage}/.design/{NN}-*.v*.md`。
4. `tools/doc-style-lint.sh:207,227`——diff 范围换成 `rewrite-notes`（含文案）。
5. `tools/coverage-extract/coverage-extract.py:15-25`——stage 列表换新前缀。
6. `tools/design-coverage-check.sh:46`、`tools/review-gate-check.sh:28,101-105`——`MODULE_DIR` 与 module 提示文案。
7. 两个锚点基线 txt——路径前缀机械替换（行号列不动，它们是工具生成的），替换后重跑锚点工具对账行数。
8. `.review/{tool}/` 的 STATE 路径约定——新评审按 `.review/{tool}/{stage}/` 组织（module 层消失），存量目录不挪，写进 MIGRATION.md。

---

## 五、执行计划

前置条件（写死，不许跳）：NK4B 迭代已收口（`NK4B-WORKLOG.md` 有收口记录，且 `AI-chats/daily.todo.md` 无未完成的 notes 写入任务）；冻结公告已发（daily.todo.md 置顶 + 各 AI opening prompt 暂停线）；执行在一个 session 内一口气完成。

### Phase 0 预检 · 清理 · 基线 · tag（1 个 commit + tag）

1. 处理存量脏文件：`AI-chats/daily.todo.md`、`os/qemu-tests/.../test-smp-topo-aarch64/src/main.rs`、`tmp/nk4a/vars.fd` 已是修改态——先单独提交或 stash，保证迁移 commit 的 diff 只含迁移内容（多 AI 对账依赖干净 diff）。
2. 清理（各一 commit 或并入本 commit）：`cmp` 确认后删 `notes/redesign/*.backup` 2 个；删 `notes/rewrite/archive_bak/`（6 个 fork 时代备份，未跟踪）；`notes/study/` 去留此时定案（留 → 原样待 Phase 1 搬；删 → 本阶段删，之后不再有 study 相关步骤）。
3. 快照未跟踪内容：`tar czf tmp/pre-migration-untracked-$(date +%Y%m%d).tar.gz notes/study $(find notes/rewrite -type d -name '.design' -o -type d -name '.review')`。
4. 采集基线（全部存 `tmp/migration/`）：
   - B0 链接基线：跑附录 B 链接检查器，记录"可解析/断链"两个集合。
   - R0 引用基线：2.3 节分区计数重跑一遍存档。
   - 锚点基线：重跑锚点检查工具，确认当前输出行数与 `tools/anchor-*-baseline.txt` 一致。
   - 工具基线：`bash tools/check-review-rules.sh` 当前必须 PASS——若现在就红，先记录为存量问题，迁移后不得新增红项。
5. `git tag -a pre-notes-migration -m "notes 旧布局终点：notes/{rewrite,study,redesign}；迁移方案见 migrate_notes_plan/migrate_glm.md"`。

**验收门**：工作树 clean（除刻意保留项）；四个基线文件落盘；tag 存在（`git tag -n`）。

### Phase 1 移动（4 个 commit）

| 批 | 内容 | 命令骨架 |
|----|------|----------|
| 1 | fork 树主体（stage 目录 + 19/20 + 内嵌 .review，**不含**要搬家的 26 个协作文件、evidence、README——先把它们 mv 到临时旁路或按批 3 顺序处理） | `mkdir rewrite-notes && git mv notes/rewrite/fork-syscall-rewrite/<各保留子目录> rewrite-notes/` |
| 2 | `concepts/`、12 个散文件 → `misc/`、`RECONSTRUCTION-PRINCIPLES.md`、旧 fork README → `00-master-plan/README.md` | 逐个 `git mv` |
| 3 | 26 个协作文件 + `evidence/` → `rewrite-notes/workflow/` | `git mv`（evidence 整目录） |
| 4 | `notes/redesign/` → `redesign-notes/`（`git mv`）；`20-redesign/` 2 个文件 → `redesign-notes/`（`git mv`）；`notes/study/` → `study-notes/`（未跟踪：`mv` + `git add`） | 见左 |

执行顺序上批 1 与批 3 谁先谁后皆可，关键是**每一批一个 commit**，且批后 `git status --porcelain` 核对无散落文件。`.design/`、内嵌 `.review/` 是 gitignore 内容，随父目录在文件系统层面自然跟走，git 不感知，无需处理。

**验收门**：`git diff --name-status -M pre-notes-migration..HEAD | grep -c '^R'` 等于预期移动数（Phase 0 时按映射表预先算好）；`ls notes/` 只剩该消失的空壳。

### Phase 2 链接重定基 + 路径改写（1 个 commit）

1. 从 `git diff --name-status -M pre-notes-migration..HEAD` 的 R 行机械生成移动清单 `tmp/migration/move-manifest.txt` 与目录级映射对 `tmp/migration/rewrite-rules.txt`（人工只审不改）。
2. 跑附录 C 改写器：对三棵树**全部** md（D9 政策）做两件事——markdown 相对链接按"老绝对目标 → 新位置重定基"；绝对路径字符串按映射对最长前缀替换。改写器对解析失败的目标只记录不修改。
3. Tier 1/2 文件集跑同一改写器（`prompt/`、`AGENTS.md`、`CLAUDE.md`、`tools/` 文本部分、`AI-chats/daily.todo.md`、2 份 rerank prompt、10 个活 STATE.md）。
4. 特例 8 项逐条人工处理（4.3 表），每条一行记录。
5. bare 残留复核：`grep -rn 'notes/rewrite\|notes/study\|notes/redesign' <改写范围>` 逐条看——无尾斜杠的裸提及（如"notes/rewrite 目录"）机器规则覆盖不到，人工顺手改。

**验收门**：三棵树内 `notes/rewrite|notes/study|notes/redesign` 残留 = 0；Tier 1/2 文件集残留 = 0；改写器"解析失败清单"⊆ B0 断链集合（即只允许本来就断的）。

### Phase 3 派生同步 + 工具自检（1 个 commit）

1. `bash tools/generate-derived-skills.sh`（`prompt/skill/` 源已在 Phase 2 改过）。
2. `bash tools/check-review-rules.sh` → PASS（它同时校验字面断言与派生同步，是本 Phase 的总裁判）。
3. 逐个试跑并比对迁移前行为：`doc-style-lint.sh` 抽 3 个文件（确认它现在真的在扫新路径，而不是静默空转）；`design-coverage-check.sh` 试跑 2 个 stage；`coverage-extract.py` 试跑 1 个 stage，输出与迁移前同构；两个锚点基线替换后重跑，行数与 R0 一致。
4. 不通过 → 修 → 重跑，直到全绿；此 Phase 禁止顺手改其他东西。

**验收门**：`check-review-rules.sh` PASS + 4 个工具试跑记录落盘 `tmp/migration/`。

### Phase 4 os 代码注释改写（1 个 commit + 构建门）

87 个文件的 `//!` 注释做前缀替换（含特例 1 的悬空引用人工判定）。虽然只动注释，但按仓库铁律走构建门：docker `minix-ci:1.94` 构建 + 测试（`-m 2g -j 1`），预期全绿。

**验收门**：`grep -rIl 'notes/rewrite' os/ | wc -l` = 0（悬空特例按判定结果计）；构建门绿。

### Phase 5 收尾（1 个 commit）

1. 重写 `rewrite-notes/README.md`（新索引：三分区说明、stage 目录一览、MIGRATION.md 链接）。
2. 写 `rewrite-notes/MIGRATION.md`：迁移日期、两个 tag 名、映射规则、特例处理记录（8 项）、冻结区清单（根 `.review/`、`AI-chats/demo/`）、module→stage 语义变化说明（评审工具 module 参数与 `.review/{tool}/{stage}/` 新约定）、`00-master-plan` 的 fork 时代属性备忘。
3. `AI-chats/daily.todo.md` 解除冻结公告，注明迁移完成。
4. 更新各 AI 记忆区（repo 外）：本次会话我方负责更新 glm 自己的记忆条目。
5. `git tag -a post-notes-migration -m "notes 新布局首个版本，验收门全过"`。

**验收门**：G1-G6 全表（第六节）逐条打勾。

### 回滚方案

- Phase 1-2 之间发现方向性错误：`git reset --hard pre-notes-migration`，然后 untar Phase 0 快照恢复 `.design/`、内嵌 `.review/`、`study-notes/` 到原位。**禁止**用 `git clean -fd` 清理新目录——会把未跟踪的 `.design/` 一并删掉。
- Phase 3-5 发现问题：不需要整体回滚，定向修 commit 即可；只有当工具链损坏无法短修时才整体回滚。
- 回滚前确认没有其他 AI 正在这个工作树上干活（冻结窗口天然保证这一点）。

---

## 六、验收门总表

| 门 | 命令 | 期望 |
|----|------|------|
| G1 移动完整性 | `git diff --name-status -M pre-notes-migration..HEAD | grep -c '^R'` | = Phase 0 预算的移动数 |
| G2 残留清零 | `grep -rn 'notes/rewrite\|notes/study\|notes/redesign' rewrite-notes redesign-notes study-notes prompt AGENTS.md CLAUDE.md tools os AI-chats/daily.todo.md` | 0 行（冻结区 `.review/`、`AI-chats/demo/` 不在扫描范围） |
| G3 链接对账 | 附录 B 链接检查器跑 B1，与 B0 集合差 | 可解析集合不变；新增断链 = 0 |
| G4 工具链 | `check-review-rules.sh` PASS；doc-style-lint / design-coverage-check / coverage-extract / 锚点工具试跑记录 | 全绿且行为与 R0 同构 |
| G5 构建 | docker `minix-ci:1.94` 构建 + 测试（`-m 2g -j 1`） | 绿 |
| G6 协作连续性 | MIGRATION.md 入库；daily.todo.md 公告更新；glm 记忆更新 | 三件齐 |

---

## 七、风险登记表

| 级别 | 风险 | 缓解 |
|------|------|------|
| P0 | `doc-style-lint.sh` 范围漏改 → 文风门静默 no-op | 特例 4 强制 + Phase 3 用"抽 3 文件必须命中"验证真的在扫 |
| P0 | `check-review-rules.sh` 字面断言与规则文本不同步 | 特例 3 规定同一 commit 成对修改 |
| P1 | 冻结窗口失败，其他 AI 并发写 notes → 迁移 diff 混入他人内容 | 前置条件写死（NK4B 收口 + 公告）；每批 commit 后 `git status` 核对 |
| P1 | os 注释批量替换踩到悬空引用 | 特例 1 先人工判定，替换器对该文件跳过 |
| P1 | AI 记忆/会话上下文持旧路径找错文件 | MIGRATION.md + daily.todo 公告 + memory 更新三重兜底 |
| P2 | 评审历史产物断链（`.review/` 1104 文件不修） | 接受（D9 政策），MIGRATION.md 映射兜底 |
| P2 | 未跟踪内容（.design/study）回滚丢失 | Phase 0 tar 快照 + 回滚禁用 `git clean` |
| P3 | `book/` 误伤 | 不在迁移触碰清单内，G5 构建门附带验证 CI 配置未动 |

---

## 八、开放问题（需你裁决，不阻塞方案评审）

1. `notes/study/`：留（原样搬 187 个文件）还是删？若留，精简（去 `.bak`/`archive/`、并主题目录）另立任务。
2. `workflow/` 子目录名是否认可（备选：平铺在 `rewrite-notes/` 根——代价是 30 个工作文件淹没 stage 目录，收益是映射规则再少一条特例）。
3. `20-redesign/` 2 个文档进 `redesign-notes/` 根，还是暂留 `rewrite-notes/` 待 redesign 区有骨架后再迁？
4. tag 命名（`pre-notes-migration` / `post-notes-migration`）是否认可。
5. 执行时机：NK4B 收口后立刻做，还是排进下一个迭代间隙？
6. `modern-hardware-and-rust.md` 是 12 个散文件里唯一的 redesign 候选，本次按你的规则全进 `misc/`，后续整理时单独裁决。

---

## 九、附录

### 附录 A：引用面分区统计命令

```bash
grep -rIl 'notes/rewrite' --exclude-dir=.git --exclude-dir=tmp \
  --exclude-dir=target --exclude-dir=minix3 --exclude='*.log' . |
awk -F/ '{if ($1==".review") b="review-frozen"; else if ($1=="notes") b="notes-tree";
  else if ($1=="os") b="os-comments"; else if ($1=="tools") b="tools";
  else if ($1=="prompt"||$1==".claude"||$1==".codex"||$1==".trae"||$1==".agents") b="rules";
  else if ($1=="AI-chats") b="ai-chats"; else b="root"} {c[b]++} END {for (k in c) print c[k], k}'
```

### 附录 B：链接检查器（B0/B1 共用，检查相对链接可解析性）

对每个 md 文件：抽取 `[...](target)` 中以 `./`、`../` 或裸相对开头的链接；按文件所在目录解析成仓库内绝对路径；记录"解析成功 / 目标不存在"两个集合。B0 在 Phase 0 于旧树上跑，B1 在 Phase 2 后于新树上跑，验收标准是可解析集合不变。

### 附录 C：路径改写器（规格）

输入：`move-manifest.txt`（R 行）+ 待改写文件列表。步骤：

1. 由清单归并出映射对（目录级优先、文件级补充），按前缀长度降序排序。
2. 对每个文件逐行做两类处理：markdown 相对链接先按**老路径**解析出仓库内绝对目标，再按映射对换算后重新生成相对新位置的链接（链接重定基）；其余路径字符串按映射对做最长前缀替换（`path:line` 锚点里的行号天然不受影响）。
3. 解析失败的目标记入 `unresolved-report.txt`，不修改原文。
4. 输出统计：每文件替换处数、unresolved 数，供 G2/G3 门对账。

规格约 60 行 python，放 `tmp/migration/rewrite_paths.py`（一次性工具，不入 `tools/` 正式工具链）。
