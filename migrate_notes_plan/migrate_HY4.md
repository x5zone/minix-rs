# notes 目录迁移计划（HY4）

> 作者：HY4 · 日期：2026-09-23 · 分支：`rewrite`
> 适用范围：`notes/**` 目录重构 + 全仓引用重写 + 工具链/规则适配
> 本文所有数字均为本次实测（`grep` / `find` / `git ls-files` 实跑结果），不是估算。

---

## 0. 结论摘要（先给答案）

| 问题 | 结论 |
|---|---|
| 是否打 tag | **要打，而且打两个**：迁移前 `notes/pre-migrate-<date>` + 迁移后 `notes/post-migrate-<date>`。当前仓库 **零 tag**，这是建立分界点的最低成本手段 |
| 目标布局 | 推荐**方案 B**（保留 `notes/` 命名空间根，语义子目录扁平化）。若坚持方案 A（顶层 `rewrite-notes/`），本文执行步骤**完全不变**，只换一个前缀常量 |
| 迁移是否只是 `mv` | **不是**。纯移动阶段只占 1 个 commit；真正的成本在 **1698 个文件**的引用重写 + **8 个工具脚本** + **367 处规则约定** |
| 能否多 AI 并行 | 能，但**必须按域切分并用 `tools/claim.sh` 领地**。禁止多 AI 同时写 `notes/` 主树（见 §7，C-35 事故教训） |
| 总耗时预估 | 纯移动 0.5 天 / 引用重写 1 天 / 工具与规则 1 天 / 验证与收敛 1 天，合计 **3–4 个独立 commit 批次** |

---

## 1. 现状盘点（实测证据）

### 1.1 目录体量

| 路径 | 文件数 | 说明 |
|---|---|---|
| `notes/rewrite/` | 1007 | 731 `.md` / 251 `.log` / 12 `.txt` / 3 `.bak` / 3 `.bak3` |
| `notes/rewrite/fork-syscall-rewrite/` | 793（git 已跟踪） | 21 个 stage 目录 + `evidence/` + `.review/` + 根散落 27 个 md |
| `notes/study/` | 187（183 `.md`） | 12 个主题子目录 + 3 个 `archive` |
| `notes/redesign/` | 12 | 含 2 个 `.backup` |
| `book/` | 已存在（mdbook 产物） | 顶层目录，不动 |

### 1.2 `fork-syscall-rewrite` 各 stage 文档数

| stage | md 数 | stage | md 数 |
|---|---|---|---|
| `00-master-plan` | 16 | `11-stage-devman` | 72 |
| `01-stage-kernel` | 162 | `12-stage-input` | 72 |
| `02-stage-vm` | 157 | `13-stage-ipc` | 46 |
| `03-stage-rs` | 92 | `14-stage-runtime` | 64（63 md） |
| `04-stage-pm` | 107 | `15-stage-fs` | 105 |
| `05-stage-vfs` | 167 | `16-stage-drivers` | 112 |
| `06-stage-sched` | 79 | `17-stage-net` | 119 |
| `07-stage-ds` | 65 | `18-stage-commands` | 105 |
| `08-stage-is` | 63 | `19-stage-integration` | 9 |
| `09-stage-init` | 74 | `20-redesign` | 2 |
| `10-stage-mib` | 123 | `evidence/` | 266（非 md） |

- `.design/` 目录 **18 个**（每个 stage 一份；`01-stage-kernel/.design` 单目录就有 110 个文件）。
- `notes/rewrite/*.md` 根级杂项 **12 篇**（`README` / `misc` / `invariant` / `modern-hardware-and-rust` / `arch_mapping` / `elf-loader` / `ipc-sendrec` / `minimal-skeleton` / `project-plan` / `project-structure` / `rewrite` / `rewrite-strategy` / `vertical-slice-strategy` / `RECONSTRUCTION-PRINCIPLES`）。
- `notes/rewrite/concepts/` 6 篇（capability / endpoint / fail-stop / typestate / drop-in-kernel-risks / README）。
- `notes/rewrite/archive_bak/` 备份垃圾（`fork-rewr-*.md.bak` / `.bak2` / `.bak3` 等）。

### 1.3 引用面（迁移的真实成本）

```bash
# 排除 notes/ 自身、tmp/、minix3/、.git/ 后，引用了 notes/rewrite 的文件总数
grep -rl "notes/rewrite" -I . | grep -vE "^\./(notes|tmp|minix3|\.git)/" | wc -l
# → 1698
```

| 引用域 | 文件数 / 次数 | 实测命令 |
|---|---|---|
| `.review/`（历史 review 产物） | **1104** 个文件（该目录共 1725 个文件） | `grep -rl "notes/rewrite" .review \| wc -l` |
| `os/`（Rust 源码 doc 注释） | **98** 处 | `grep -rn "notes/rewrite" os/ \| wc -l` |
| `prompt/` + `.claude/` + `.codex/` + `.trae/`（规则与技能） | **367** 处 | `grep -rn "notes/rewrite" prompt/ .claude/ .codex/ .trae/ \| wc -l` |
| `tools/`（脚本） | **8** 个脚本 + 2 个基线文件 | `grep -rln "notes/" tools/` |
| `tools/anchor-suspect-baseline.txt` | **868** 行含路径（共 874 行） | `grep -c "notes/rewrite" tools/anchor-suspect-baseline.txt` |
| `tools/anchor-unresolved-baseline.txt` | 1 行（共 6609 行） | 同上 |
| `notes/` 内部自引用 | **1184** 行绝对路径 + **354** 行 `../` 相对链接 | `grep -rn "notes/rewrite" notes/ --include=*.md \| wc -l` |
| 其他（根脚本/AI 计划） | `check_references.sh`、`CLAUDE.md`、`.zcode/plans/`（14）、`.codebuddy/plans/`、`AI-chats/`、`.trae/documents/`（8） | 见 §3 R9 |

### 1.4 工具链对路径的硬假设（会直接失效）

| 脚本 | 硬编码 | 失效方式 |
|---|---|---|
| `tools/design-coverage-check.sh` | `MODULE_DIR="notes/rewrite/${MODULE}"`，stage 用 `ls \| grep -E '^[0-9]+-'` | `fork-syscall-rewrite` 这层没了 → `MODULE_DIR` 不存在 |
| `tools/review-init.sh` | `notes/rewrite/{module}/{stage}/{doc}.md`，`{module}` = 路径第一级目录 | `{module}` 推导错位 |
| `tools/review-gate-check.sh` | `MODULE_DIR="notes/rewrite/${MODULE}"` | 同上 |
| `tools/design-index-update.sh` | `notes/rewrite/{module}/{stage}/.design/DESIGN-INDEX.md` | 同上 |
| `tools/doc-style-lint.sh` | `git diff -U0 -- notes/rewrite` | 增量门路径失效（静默失效，最危险） |
| `tools/coverage-extract/coverage-extract.py` | 文档字符串里写死 `notes/rewrite/fork-syscall-rewrite/03-stage-kernel` 等 3 处 | 示例失效 |
| `tools/anchor-suspect-baseline.txt` | 868 行旧路径 | 锚点核对全量误报 |
| `check_references.sh`（根） | `BASE_DIR="notes/rewrite/fork-syscall-rewrite/02-stage-vm/"` | 完全失效 |

> **注意**：`doc-style-lint.sh` 与 `anchor-*.txt` 属于**静默失效**——不会报错，只会「什么都没检查到」。这类失效比崩溃更危险，必须单列验证项。

### 1.5 git 现状

- 分支：`rewrite`（当前），另有 `main` / `nk4a-agent-wip` / `nk4a-review-docs` / `notes-study`。
- `git ls-files notes | wc -l` → **1046**（全部已跟踪，无未跟踪文件）。
- `git tag` → **空**（仓库历史上从未打过 tag）。
- `.git` 体积 262M。

---

## 2. 目标布局

### 2.1 三个候选方案

| 方案 | 形态 | 层级（以 16-smp.md 为例） | 优点 | 缺点 |
|---|---|---|---|---|
| **A**（你的原始设想） | `rewrite-notes/01-stage-kernel/16-smp.md`<br>`redesign-notes/vm_in_kernel/xx.md`<br>`study-notes/minix3_concept/xx.md` | 3 层 | 层级最浅 | 仓库根被 `*-notes/` 稀释（根已有 `minix3/ os/ tools/ prompt/ book/ notes/ tmp/ scripts/ examples/`）；`notes/README.md` 契约落点丢失；与 `book/` 的抽取契约割裂 |
| **B**（**推荐**） | `notes/rewrite/01-stage-kernel/16-smp.md`<br>`notes/redesign/vm-in-kernel/xx.md`<br>`notes/study/<topic>/xx.md` | 4 层 | 命名空间收敛；`notes/README.md` 天然成为目录契约；`book/` 抽取只需一个根前缀；`notes/TODO.md` 保留 | 比 A 深一层 |
| **C** | 保留 `notes/` 根，但把 `fork-syscall-rewrite` **只重命名**为 `boot-order` | 5 层 | **工具链零改动**（`{module}` 推导仍成立） | 层级没降；只解决了痛点 2，没解决痛点 1 |

### 2.2 推荐：方案 B

理由：

1. **两个痛点的共同根因是 `fork-syscall-rewrite` 这一层**，而不是 `notes/rewrite` 这个前缀。删掉它，深度 5→4，语义废弃问题同时消失。
2. 保留 `notes/` 根让 `notes/README.md` 可以直接升级为「四类目录的生命周期契约」（study=只读档案 / rewrite=主工作区 / redesign=未来探索 / book=产物），这是 A 方案做不到的事。
3. **迁移成本与 A 完全相同**——都是一次前缀替换。所以选择只取决于治理，不取决于工作量。

> **前缀参数化**：下文统一使用 shell 变量，切换方案只需改这两行：
> ```bash
> OLD="notes/rewrite/fork-syscall-rewrite"   # 方案 A/B 相同
> NEW="notes/rewrite"                        # 方案 B
> # NEW="rewrite-notes"                      # 方案 A（其余步骤不变）
> ```

### 2.3 方案 B 目标目录树

```
notes/
├── README.md                # 【重写】四目录生命周期契约 + 命名规范
├── MIGRATION-MAP.md         # 【新增】机器可读 old→new 全量映射（含 .design/evidence/散落文件）
├── rewrite/                 # 【主工作区】保持外部行为的 Rust 重写文档
│   ├── 00-overview/         # ← 原 00-master-plan/（16）+ concepts/（6）
│   ├── 01-stage-kernel/     # ← 原 01-stage-kernel/（含其 .design/）
│   ├── 02-stage-vm/
│   ├── ...                  # 03-stage-rs … 19-stage-integration 原样平移
│   ├── misc/                # ← 原 notes/rewrite/*.md 杂项 12 篇
│   ├── _evidence/           # ← 原 evidence/（266 日志；_ 前缀 = 中间产物，禁止引用）
│   └── _worklog/            # ← 原根散落 NK4*/HANDOFF*/edge*/new_todo_*（27 个 md，中间产物）
├── redesign/                # 【未来】rewrite 稳定后的 OS 方向探索
│   ├── vm-in-kernel/        # ← 原 20-redesign/rs-cross-layer-pollution.md
│   ├── endpoint/            # ← 原 endpoint_redesign.md
│   ├── semantics/           # ← 原 semantic-modules.md + 20-redesign/tocutou-*.md
│   ├── microkernel/         # ← 原 microkernel-closure-design.md + microkernel-cohesion-design.md
│   ├── arch/                # ← 原 architecture-changes.md
│   ├── fork/                # ← 原 fork-redesign.md
│   ├── ipc/                 # ← 原 ipc-improve.md
│   └── misc/                # ← 原 redesign.md + improve_minix*.md(.backup)
├── study/                   # 【只读档案】早期 Minix3 学习笔记（见 §8）
│   └── <topic>/ …           # 保留现有 12 个子目录结构，不动
└── (book/ 保持在仓库顶层，不进 notes/)
```

**命名约定（新增，写进 `notes/README.md`）**：

- `_` 前缀目录 = 中间产物，**正式文档一律不得引用**（延续 AGENTS.md 的 Hidden Folder Convention，把「隐藏」外化为「下划线前缀」，便于 `ls` 可见 + grep 可排除）。
- stage 目录保留 `NN-stage-xxx` 编号，因为 `tools/design-coverage-check.sh` 用 `grep -E '^[0-9]+-'` 识别 stage，**编号是工具的隐式契约**。
- `.design/` 继续挂在各自 stage 下（不集中），否则 `review-process.md` Step 0 的 4 条 `ls` 预检全部要重写。

---

## 3. 风险登记册（Risk Register）

| ID | 风险 | 证据 / 影响面 | 缓解措施 | 验证 |
|---|---|---|---|---|
| **R1** | 工具脚本硬编码路径 | 8 个脚本 + `check_references.sh` | 先做**参数化改造**（§5 P1），让脚本同时支持新旧布局，再切换 | 每个脚本的 `--self-test` / `--help` 跑通 |
| **R2** | 静默失效（不报错只是没检查） | `doc-style-lint.sh`（增量门）、`anchor-*-baseline.txt`（868 行） | 单独列为 P6 验证项，用**故意埋一个坏锚点**跑正向测试 | 埋点能被检出 |
| **R3** | 历史 review 产物 | `.review/` 1104 个文件 | 全量 sed 重写 + 归档 README 注明「路径已按 MIGRATION-MAP 机械重写」；原始形态由 pre-tag 保真 | Gate M2 残留 = 0 |
| **R4** | 代码侧引用失效 | `os/` 98 处 doc 注释 | 同批 sed；`cargo doc` 不受影响但一致性破损 | `cargo check` 通过 + grep 残留 0 |
| **R5** | 文档内部交叉引用断裂 | 1184 绝对路径 + 354 相对链接 | 新增 `tools/check-notes-links.sh` 做断链扫描（**迁移前先跑一次建立基线**） | 迁移后断链数 ≤ 迁移前基线 |
| **R6** | 锚点基线全量误报 | 868 行旧路径 | 迁移后**重新生成**两条基线（`anchor-migrate.sh --stats-only` 重跑），而不是 sed 旧文件 | 基线行数与迁移前同量级 |
| **R7** | 隐藏目录 / 中间产物遗漏 | 18 个 `.design/`、`fork-syscall-rewrite/.review/`、`evidence/`、`archive_bak/` | 用 `find notes -name ".*"` + 文件清单对账，逐项在 MIGRATION-MAP 登记 | Gate M1 内容 hash 集完全相等 |
| **R8** | git 重命名检测失败 | 793 个文件移动 | **纯移动单独一个 commit**，不与内容修改混在一起；用 `git mv` | `git show --stat -M` 显示 rename 而非 delete+add |
| **R9** | AI 会话产物引用旧路径 | `.zcode/plans/`（14）、`.codebuddy/plans/`、`AI-chats/`、`tmp/`（163 log + 108 md）、`.trae/documents/`（8） | 分为「必须改」「建议改」「不动」三档（见 §6.2） | 按档位核对 |
| **R10** | 多 AI 并行写冲突 | 用户计划多 AI 同时执行 | **域隔离 + `tools/claim.sh` 领地 + worktree 隔离壳**（§7） | `tools/claim.sh list` 无重叠 |
| **R11** | `book/` 未来抽取契约未定 | `book/` 已存在 mdbook 产物 | 迁移时在 `notes/README.md` 写入「book 只从 `notes/rewrite/` 与 `notes/redesign/` 抽取，不引用 `_` 前缀目录」 | 契约条款存在 |
| **R12** | 迁移中途被打断，半成品状态 | — | 每个阶段结束必须打一个可回滚的 commit；阶段间禁止交叉 | §11 回滚表 |
| **R13** | 未跟踪 / 已忽略文件被漏掉 | `git status --porcelain notes` = 0（当前干净） | 迁移前再跑一次确认；`tmp/` 与 `evidence/` 单独决策 | 迁移前 `git status` 干净 |

---

## 4. 分界点：tag 与回滚基线

### 4.1 结论

**必须打 tag**。理由（针对本仓库实测情况）：

1. **当前 `git tag` 为空**——没有任何历史分界点，一旦迁移出问题，只能靠 `git reflog` + commit hash 抢救，而 reflog 会过期。
2. 迁移将移动 **793 个已跟踪文件** + 修改 **1698 个引用文件**。这不是「可以 `Ctrl+Z` 的操作」。
3. tag 让审计成为可能：`git diff notes/pre-migrate-20260923..HEAD --stat -- notes/` 能一眼看出「到底动了什么」，这是 review 迁移正确性的唯一可靠手段。
4. tag 成本 ≈ 0（`.git` 262M，一个 tag 是 41 字节的 ref）。

### 4.2 三层保险（都要做）

```bash
# ── 保险 1：迁移前 annotated tag（分界点 + 审计基线）──────────────
git status --porcelain          # 必须为空；不为空先提交或 stash
git tag -a notes/pre-migrate-20260923 \
        -m "notes 目录迁移前基线：fork-syscall-rewrite 布局的最后状态"
git tag -n99 notes/pre-migrate-20260923      # 回读确认

# ── 保险 2：镜像裸克隆（防 .git 损坏 / 误 gc / 误 reset）──────────
git clone --mirror . /tmp/minix-rs-mirror-20260923.git
ls /tmp/minix-rs-mirror-20260923.git         # 确认生成

# ── 保险 3：保护分支（tag 不动，分支可前进，双重兜底）────────────
git branch notes/pre-migrate-20260923-bak    # 纯指针分支，零成本
```

### 4.3 迁移后 tag

```bash
git tag -a notes/post-migrate-<date> \
        -m "notes 目录迁移完成：布局见 notes/README.md，映射见 notes/MIGRATION-MAP.md"
```

### 4.4 tag 命名约定（本次顺便建立）

`notes/<event>-<YYYYMMDD>`，例如 `notes/pre-migrate-20260923`。后续重大目录变更沿用此前缀。

> **推送决策（需你拍板）**：`git push origin notes/pre-migrate-20260923`。
> 打在本地即可回滚；推到远端才能让其他机器/其他 AI 会话以同一基线对齐。**建议推**（见 §13 Q3）。

---

## 5. 分阶段执行计划

> **总原则**：先改工具（让工具同时认新旧两种布局），再搬文件，最后重写引用，最后收口规则。
> 顺序颠倒会导致「文件搬走了，脚本全废」的长时间裸奔窗口。

### P0 — 冻结与基线（0.5 天，串行，1 人）

| 项 | 内容 |
|---|---|
| 目标 | 建立可回滚基线 + 完整清单 |
| 步骤 | 1. `git status` 确认干净<br>2. 打 pre-tag + 镜像克隆 + 保护分支（§4.2）<br>3. `find notes -type f -exec md5sum {} \; \| sort -k2 > /tmp/notes-before.md5`<br>4. `find notes -type f \| sort > /tmp/notes-before.list`<br>5. `grep -rn "notes/rewrite" -I . \| grep -vE "^\./(tmp\|minix3\|\.git)/" > /tmp/refs-before.txt`（行数记账：1698 文件）<br>6. **跑一次断链基线**：`tools/check-notes-links.sh`（若不存在则 P1 先造）→ `/tmp/links-before.txt`<br>7. 声明**冻结窗口**：迁移期间禁止任何会话直接编辑 `notes/` 主树 |
| 完成判据 | 三个 `/tmp/*.md5 / *.list / *.txt` 文件生成；tag 可读 |
| 回滚 | `git reset --hard notes/pre-migrate-20260923`（**仅在自己的 worktree 内执行**，见 §7） |

### P1 — 工具链参数化（1 天，可与 P2 并行）

**核心动作：引入单一路径真源 `tools/notes-layout.conf`，消灭硬编码。**

```bash
# 新增 tools/notes-layout.conf
NOTES_ROOT="notes"          # 方案 B；方案 A 改为 "."
REWRITE_DIR="notes/rewrite" # 方案 A 改为 "rewrite-notes"
```

改造清单（**全部改为：先读 conf，读不到再回退旧路径**，保证新旧布局都能跑）：

| 脚本 | 改造点 |
|---|---|
| `tools/design-coverage-check.sh` | `MODULE_DIR` → 支持「无 module 层」：若 `notes/rewrite/<MODULE>` 不存在，则 `MODULE_DIR=notes/rewrite` 且 stage 由 `--stage` 指定 |
| `tools/review-init.sh` | `{module}` 推导增加 fallback：路径不含 module 层时 `MODULE=<stage>` |
| `tools/review-gate-check.sh` | 同 `design-coverage-check.sh` |
| `tools/design-index-update.sh` | 路径拼接读 conf |
| `tools/doc-style-lint.sh` | `git diff -- notes/rewrite` → `git diff -- "$REWRITE_DIR"`（**当前已失效，必须修**） |
| `tools/coverage-extract/coverage-extract.py` | 文档字符串示例更新 |
| `check_references.sh`（根） | `BASE_DIR` 改为参数化 |
| `tools/review-state-validate.py` | 检查是否也含路径假设（本次未扫到，P1 需补扫确认） |

同时**新增两个工具**：
- `tools/notes-link-check.sh`：扫描 `notes/**/*.md` 的本地 md 链接（含 `#anchor`），输出断链清单。
- `tools/notes-layout-verify.sh`：断言 conf 与实际目录一致，供 CI/手工调用。

| 完成判据 | 旧布局下所有脚本行为不变（`--self-test` 通过）；新 conf 指向新布局时脚本能正确解析 |
| 回滚 | `git revert` 该 commit |

### P2 — `notes/study` 处理（可与 P1 并行，见 §8）

### P3 — 主体移动（0.5 天，串行，1 人，**纯 `git mv`，不改任何内容**）

```bash
OLD="notes/rewrite/fork-syscall-rewrite"; NEW="notes/rewrite"

# 3.1 stage 目录平移（含各自的 .design/）
for d in "$OLD"/[0-9][0-9]-*/; do git mv "$d" "$NEW/$(basename "$d")"; done

# 3.2 特殊归属
git mv "$OLD/00-master-plan" "$NEW/00-overview"
git mv "$OLD/concepts"       "$NEW/00-overview/concepts"   # 或保留为 notes/rewrite/concepts
git mv "$OLD/20-redesign"    notes/redesign/_incoming      # 内容再按主题拆到 redesign/*/
git mv "$OLD/evidence"       "$NEW/_evidence"
git mv "$OLD/.review"        .review/legacy-fork           # 内部 review 产物并入根 .review/

# 3.3 根散落 md → 中间产物区
mkdir -p "$NEW/_worklog"
git mv "$OLD"/NK4*.md "$OLD"/HANDOFF*.md "$OLD"/edge*.md "$OLD"/new_*.md "$NEW/_worklog/"

# 3.4 根级杂项 md → misc
mkdir -p "$NEW/misc"
git mv notes/rewrite/*.md "$NEW/misc/"    # 注意排除 README.md（要重写，不移动）

# 3.5 archive_bak：确认 pre-tag 已打 → git rm（不留 .bak 垃圾）
git rm -r notes/rewrite/archive_bak

# 3.6 README / TODO 重写
#   notes/README.md      → 四目录生命周期契约
#   notes/MIGRATION-MAP.md → 新增，见 §5.5
```

**提交规范**：本阶段产出**恰好一个 commit**，message 形如：
```
refactor(notes): 迁移 fork-syscall-rewrite → notes/rewrite/{stage}/（纯移动，无内容修改）

- 删除语义废弃的 fork-syscall-rewrite 层（深度 5→4）
- .design/ 随各自 stage 平移；evidence/ 与根散落 md 归入 _ 前缀中间产物区
- 映射表见 notes/MIGRATION-MAP.md；基线 tag notes/pre-migrate-20260923
```

| 完成判据 | Gate M1 通过（内容 hash 集完全相等，仅路径变）；`git show --stat -M` 显示 rename |
| 回滚 | `git revert` 单 commit 即可完整还原 |

### P4 — 全仓引用重写（1 天，可分域并行，见 §7）

按 §6 的域划分执行。核心命令：

```bash
# 4.1 文档内部绝对路径（1184 行）
grep -rl "$OLD" notes/ --include=*.md | xargs sed -i "s|$OLD|$NEW|g"

# 4.2 .review/ 历史产物（1104 文件）
grep -rl "$OLD" .review/ | xargs sed -i "s|$OLD|$NEW|g"

# 4.3 os/ 代码注释（98 处）
grep -rl "$OLD" os/ | xargs sed -i "s|$OLD|$NEW|g"

# 4.4 规则与技能（367 处）—— 见 §6.3，必须走派生同步，不能直接 sed
```

**禁止**：本阶段不得顺手「优化」文档内容、不得修 typo、不得调格式。一次只做一件事（fix-guard 原则）。

| 完成判据 | Gate M2（残留 0）+ Gate M3（断链不恶化） |
| 回滚 | `git revert`；因 P3/P4 分离，可单独回滚 |

### P5 — 规则与技能同步（0.5 天，**必须串行，1 人**）

规则文件是三端派生的，改错会引发 `check-review-rules.sh` 失败：

```
prompt/  （源）  →  .claude/  .codex/  .trae/
```

1. 改源：`prompt/review-rules/review-process.md`（§403、412-417、446-449、1323-1328）、`review-doc-checklist.md`、`review-patterns.md`、`prompt/README.md`、`prompt/todo_plan.md`、`prompt/skill/*.md`（5 个）。
2. 按 `prompt/README.md` 的同步命令重生成派生文件。
3. `bash tools/check-review-rules.sh` 必须通过。
4. 同步 `CLAUDE.md`（§15、24、25、41、76、299、311）、`AGENTS.md`（Directory Layout 段 + Review Workflow 段）。

| 完成判据 | `tools/check-review-rules.sh` exit 0；`tools/lint-review-rules.sh` exit 0 |
| 回滚 | `git revert` |

### P6 — 锚点基线重建（0.5 天）

**不要 sed 旧基线**（路径改了，语义位置也变了，sed 出来的基线是假的）。

```bash
bash tools/anchor-migrate.sh --stats-only "$NEW"     # 重新统计
# 重新生成 tools/anchor-suspect-baseline.txt / anchor-unresolved-baseline.txt
bash tools/anchor-resolve.sh --self-test              # 必须 exit 0
```

| 完成判据 | 新基线行数与旧基线（874 / 6609）同量级，偏差 >20% 需人工解释 |
| 回滚 | `git checkout` 这两个文件 |

### P7 — 验证与收敛（1 天，见 §9）

### P8 — 收口（0.5 天）

- 打 post-tag（§4.3）。
- `notes/README.md` 终稿、`notes/TODO.md` 清理迁移相关条目。
- 更新 `AI-chats/daily.todo.md` 中的路径引用。
- **删除 `migrate_notes_plan/`**（迁移完成后该目录即废；在删除前确认 6 份计划已归档到 `tmp/` 或 commit 历史）。
- 清理 `/tmp/*.md5` 等临时文件（**保留** `/tmp/minix-rs-mirror-*.git` 到验证期结束）。

---

## 6. 引用重写规范

### 6.1 铁律

1. **一次只改一个域**，改完立刻 grep 验证，再进入下一域（fix-guard）。
2. **只替换路径，不碰内容**。`sed` 表达式必须是纯 `s|OLD|NEW|g`，禁止附带任何其它编辑。
3. **锚点纪律（模式 83）**：`path:fn NAME` 这类符号锚点中的路径同样要替换，但 `（Lnnn，工具生成）` 后缀**不得手改**。
4. **代码块内的路径也要改**（`anchor-migrate.sh` 会跳过 fence 内的行号，但路径替换不受此限）。
5. 每个域改写后执行：`grep -rn "$OLD" <域> | wc -l` → 必须 0。

### 6.2 域划分与处置档位

| 域 | 文件数 | 档位 | 处置 |
|---|---|---|---|
| `notes/` | 1184 行 | **必须改** | sed |
| `.review/` | 1104 | **必须改** | sed + 归档 README 注明 |
| `os/` | 98 处 | **必须改** | sed（仅 doc 注释，不影响编译） |
| `tools/` | 8 脚本 + 2 基线 | **必须改**（脚本）/ **重建**（基线） | P1 参数化 + P6 重建 |
| `prompt/` `.claude/` `.codex/` `.trae/` | 367 处 | **必须改（走派生同步）** | P5 |
| 根 `CLAUDE.md` / `AGENTS.md` / `check_references.sh` | 3 | **必须改** | 手改 |
| 根 `.zcode/plans/`（14）`.codebuddy/plans/`（1） | 15 | **建议改** | sed（AI 会话计划，会被再次读取） |
| `AI-chats/`（3）`.trae/documents/`（8） | 11 | **建议改** | sed |
| `tmp/`（163 log + 108 md） | 大量 | **不动** | 一次性日志，价值随时间衰减；且 `tmp/` 天然是垃圾区 |
| `migrate_notes_plan/`（6） | 6 | **不动** | 迁移计划本身就是历史记录，保留旧路径反而是证据 |
| `notes/MIGRATION-MAP.md` | 1 | **必须含旧路径** | 映射表的左列就是旧路径，属预期残留 |
| `.git/logs/` | — | 不可改 | reflog |

### 6.3 规则文件的特殊性

`prompt/` 是源，`.claude/` `.codex/` `.trae/` 是派生。**直接 sed 派生文件会导致下次同步时被覆盖回去**。
正确顺序：改 `prompt/` → 跑 `prompt/README.md` 里的同步命令 → `tools/check-review-rules.sh` 验证。

涉及的具体约定（必须逐条核对，`grep -n "notes/rewrite" prompt/review-rules/review-process.md`）：
- L171 / L412-417 / L446-449 / L1323-1328：`notes/rewrite/{module}/{stage}/.design/` 的四条 `ls` 预检
- L403：`{module}` = `notes/rewrite/` 下第一级目录名 —— **这条必须重写**（module 层消失后定义失效）
- L1136 / L1558：示例命令中的具体路径

> **L403 是本次迁移唯一一处「语义级」规则改动**，不是机械替换。建议改为：
> `{module}` = 文档所在 stage 目录名去掉 `NN-stage-` 前缀后的模块名（如 `01-stage-kernel` → `kernel`），与覆盖率脚本的 `--module kernel` 对齐。
> 这一改动会顺带消除原文档里「两个不同概念不得混用」的历史包袱。**需你拍板**（§13 Q2）。

---

## 7. 多 AI 并行：域隔离协议

**最大的并发风险不是工作量，是写冲突。** 仓库已有 `tools/claim.sh`（分支=锁，worktree=隔离壳），正是为 C-35 事故（某会话在主树 `checkout`+`reset`，销毁了其他 agent 的在制品）而生。

### 7.1 强制协议

```bash
# 每个 AI 开工前（不是可选，是强制）
tools/claim.sh claim notes-migrate-P3 hy4
tools/claim.sh verify          # 自检：位置、分支、在制数
# → 自动建 .wt/notes-migrate-P3-hy4/ 专属工作树，只在该树内开工
```

**红线**：
1. 禁止在主工作树（`/home/xzhao/github/minix-rs`）执行 `git checkout` / `git reset` / `git clean -fd` / `--force` 类操作。
2. 禁止两个 AI 领同一域。
3. 完成后 `tools/claim.sh release <ID> <owner>`。

### 7.2 域分配建议

| 域 | 规模 | 可并行性 | 说明 |
|---|---|---|---|
| P0 基线 | 小 | **串行，1 人** | 所有人依赖它 |
| P1 工具链 | 中 | 可 1–2 人 | 按脚本分：`design-coverage-check`+`review-init`+`review-gate-check` 一人；`doc-style-lint`+`coverage-extract`+新增脚本 一人 |
| P2 study | 小 | 可 1 人，与 P1 并行 | 独立目录 |
| P3 主体移动 | 大（793 文件） | **串行，1 人** | 必须单 commit，并行必冲突 |
| P4 引用重写 | 大 | **可 3 人并行**（按 §6.2 域分：`notes/` / `.review/` / `os/`） | 域间零重叠 |
| P5 规则同步 | 中 | **串行，1 人** | 派生关系，串行才安全 |
| P6 锚点重建 | 小 | 串行 | 依赖 P4 |
| P7 验证 | 中 | 可 2 人（一人跑 Gate，一人人工抽查） | |

**推荐并行度**：P0 → (P1 ∥ P2) → P3 → P4（3 路）→ P5 → P6 → P7。关键路径 4 个串行点。

---

## 8. `notes/study` 处置细则

现状：183 个 `.md`，12 个主题子目录（`arch` `boot` `interrupt` `ipc` `pm` `process` `services` `syscall` `vfs` `vm`）+ 3 个 `archive`（`pm/archive` `syscall/archive` `vfs/archive`）。

**建议：不整体删除，做「三分法」**。理由：git tag 已保证可回捞，删除不会丢失任何东西；但「可直接删除」的判断需要逐目录做，不能一刀切。

| 类别 | 判定标准 | 处置 |
|---|---|---|
| **A 保留** | 含 Minix3 C 源码的原创梳理（有具体 `minix3/...c:Lnnn` 引用、有自绘图表、非泛泛而谈） | 原地保留，移入 `notes/study/<topic>/` |
| **B 精简** | 骨架有价值但大半是 AI 泛化叙述 | 保留目录 + 每篇顶部加 `> **状态**: 精简版（<date>），完整版见 tag notes/pre-migrate-20260923`，正文删至 30% |
| **C 删除** | 与 `notes/rewrite/` 内容重复 / 纯 AI 生成的教科书式复述 / 已被 rewrite 文档取代 | `git rm`，并在 MIGRATION-MAP 记一行 |

执行要求：
- 三分结果写进 `notes/MIGRATION-MAP.md` 的 study 段，逐文件登记（183 行）。
- **B 类的「精简」不得在 P3 同一 commit 里做**，单独一个 commit `docs(study): 精简 ...`。
- `*/archive` 三个子目录默认归 C（archive 的 archive）。

---

## 9. 验证门（Gate）

| Gate | 命令 | 通过判据 |
|---|---|---|
| **M1** 内容完整性 | `find notes -type f -exec md5sum {} \; \| sort -k2` 对比 `/tmp/notes-before.md5` 的 hash 列 | hash 多重集**完全相等**（证明纯移动未丢未改） |
| **M2** 零残留 | `grep -rn "fork-syscall-rewrite" -I . \| grep -vE "^\./(\.git\|tmp\|minix3\|migrate_notes_plan)/" \| grep -v "notes/MIGRATION-MAP.md" \| wc -l` | **0** |
| **M3** 断链不恶化 | `tools/notes-link-check.sh notes/` 对比 `/tmp/links-before.txt` | 断链数 ≤ 迁移前 |
| **M4** 工具可用 | `bash tools/design-coverage-check.sh <module>` / `tools/review-init.sh --help` / `bash tools/check-review-rules.sh` / `bash tools/lint-review-rules.sh` / `tools/anchor-resolve.sh --self-test` | 全部 exit 0 |
| **M5** 静默失效探测 | 在 `notes/rewrite/` 下**故意插入**一处断链 + 一处坏锚点，跑 `notes-link-check.sh` 与 `anchor-resolve.sh --check` | **必须被检出**（否则说明工具已静默失效）；验证后撤销埋点 |
| **M6** 构建完好 | `cd os && cargo check --workspace` | 与迁移前一致（98 处仅在注释，预期无影响） |
| **M7** git 卫生 | `git status --porcelain \| wc -l`；`find notes -type d -empty` | 0 未提交残留；无空目录 |
| **M8** 规则一致 | `bash tools/check-review-rules.sh` + `bash tools/diff-trae-skills.sh` | 三端派生一致 |

**M5 是本次最关键的一 gate**——R2 指出 `doc-style-lint.sh` 与锚点基线会静默失效，只有正向埋点测试能暴露。

---

## 10. 杂项文档归属（12 篇根级 md）

按内容判定（已读各文件首段）：

| 文件 | 归属 | 理由 |
|---|---|---|
| `README.md` | **留在 `notes/` 根** | 升级为目录契约，不进 misc |
| `RECONSTRUCTION-PRINCIPLES.md` | `rewrite/`（顶层，不进 misc） | 重构方法论，是 rewrite 的一级入口 |
| `rewrite-strategy.md` | `rewrite/`（顶层） | 语义冻结原则，同上 |
| `vertical-slice-strategy.md` | `rewrite/`（顶层） | 纵向切片策略，同上 |
| `project-plan.md` / `project-structure.md` | `rewrite/00-overview/` | 与 `00-master-plan` 同类 |
| `invariant.md`（103K） | `rewrite/00-overview/` | 内核不变量总表，属总览级 |
| `modern-hardware-and-rust.md`（70K） | `rewrite/misc/` | 设计哲学/性能理论，非具体模块 |
| `arch_mapping.md` | `rewrite/misc/` | 硬件→trait 抽象映射 |
| `elf-loader.md` | `rewrite/misc/` | 跨模块机制（kernel+PM+VM） |
| `ipc-sendrec.md` | `rewrite/misc/` | 单一机制的深挖（`13-stage-ipc/` 是 stage 视角，可考虑并入；**需拍板**） |
| `minimal-skeleton.md` | `rewrite/misc/` | 工程性目标文档 |
| `misc.md`（26K） | `rewrite/misc/` | 已是杂项 |
| `rewrite.md` | `rewrite/`（顶层） | 本目录的总述，与 README 同级 |

> 即：`rewrite/` 顶层只放**一级入口文档**（README / RECONSTRUCTION-PRINCIPLES / rewrite-strategy / vertical-slice-strategy / rewrite），其余进 `00-overview/` 或 `misc/`。

---

## 11. 回滚预案

| 阶段 | 回滚方式 | 代价 |
|---|---|---|
| P0 后任何时刻 | `git reset --hard notes/pre-migrate-20260923`（**只在自己的 worktree 内**） | 零 |
| P1 工具改造 | `git revert <P1-commit>` | 低（notes 结构未变） |
| P2 study | `git revert <P2-commit>` | 低 |
| P3 纯移动 | `git revert <P3-commit>` | 低（单 commit，rename 完整） |
| P4 引用重写 | `git revert <P4-commit>` | 中（P3 已生效，会短暂不一致，但可逆） |
| P5 规则同步 | `git revert <P5-commit>` + 重跑派生同步 | 中 |
| P6 锚点重建 | `git checkout HEAD~1 -- tools/anchor-*baseline.txt` | 低 |
| 灾难性（`.git` 损坏） | `git clone /tmp/minix-rs-mirror-20260923.git` | 低（靠保险 2） |

---

## 12. 交付物清单

- [ ] `notes/pre-migrate-20260923` tag（+ 可选推送）
- [ ] `/tmp/minix-rs-mirror-20260923.git` 镜像
- [ ] `notes/MIGRATION-MAP.md`（机器可读全量映射，含 study 三分结果）
- [ ] `notes/README.md`（四目录生命周期契约 + `_` 前缀约定 + book 抽取契约）
- [ ] 4 个 commit：P3 纯移动 / P4 引用重写 / P5 规则同步 / P6 锚点重建（+ P1 工具 / P2 study 各一）
- [ ] 新增 `tools/notes-layout.conf`、`tools/notes-link-check.sh`、`tools/notes-layout-verify.sh`
- [ ] Gate M1–M8 全部通过的证据（命令 + 输出，写进 commit message 或 `tmp/`）
- [ ] `notes/post-migrate-<date>` tag

---

## 13. 需你拍板的问题

| # | 问题 | 我的倾向 |
|---|---|---|
| **Q1** | 方案 A（顶层 `*-notes/`）还是方案 B（保留 `notes/` 根）？ | **B**。A/B 执行成本完全相同，B 的治理性更好 |
| **Q2** | `review-process.md` L403 的 `{module}` 定义如何改？ | 改为「stage 目录名去 `NN-stage-` 前缀」，与 `coverage-extract --module kernel` 对齐，顺带消除「两个概念不得混用」的历史包袱 |
| **Q3** | pre-tag 是否 push 到 origin？ | **推**。多 AI 多机器场景下，本地 tag 无法作为共同基线 |
| **Q4** | `.review/` 1104 个历史产物：sed 重写 还是 归档不改？ | **sed 重写**。原始形态由 pre-tag 保真（真正的历史在 git 里），而不重写会让全仓 grep 长期被污染、Gate M2 无法归零 |
| **Q5** | `notes/study` 三分法中「C 删除」的尺度？ | 保守：只删与 rewrite 明确重复 / 纯 AI 泛化复述的；拿不准的一律归 B（精简保留） |
| **Q6** | `evidence/`（266 日志）与 `archive_bak/` 是否删除？ | `archive_bak/` 删（纯 `.bak` 垃圾，tag 已保底）；`evidence/` 保留到 `notes/rewrite/_evidence/`（含 `gs.log` 等过程证据，删了难复现） |
| **Q7** | `concepts/` 放 `rewrite/00-overview/concepts/` 还是 `rewrite/concepts/`？ | `rewrite/00-overview/concepts/`，避免 `rewrite/` 顶层再冒出第二个概念目录 |
| **Q8** | 冻结窗口多长？ | 建议 P0→P3 期间（约 1 天）`notes/` 完全冻结；P4 起按域解禁 |
