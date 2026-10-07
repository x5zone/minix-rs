# notes 目录迁移 · 执行日志

> **创建**: 开窗当天 17:15 起随阶段追加

- 执行者：Qoder 侧 agent（本会话）
- 分支：`rewrite`
- 迁移动机与方案源：`migrate_notes_plan/migrate_{HY4,MiMo,glm,muse,deepseek,qwen}.md` 六份独立方案 + 本会话在开窗当天（基线提交 `bb8a90e05`）做的现实核对
- 本日志用途：逐动作记录执行过程，供**更换软件、更换模型的独立审阅方**审计本次迁移的正确性。
  每条动作都写「做了什么 / 命令 / 结果 / 为什么这么做」，审阅方不需要信任叙述，可以按命令复跑。
- 三份交付物：
  1. 执行前全貌留存：[`pre-migrate-20261007/00-SNAPSHOT.md`](pre-migrate-20261007/00-SNAPSHOT.md)
  2. 本执行日志
  3. 交接给审阅 agent 的审计 PROMPT：`REVIEW-PROMPT.md`（Phase 7 产出）

## 用户裁决清单（本会话问答确定，覆盖六份方案的分歧点）

| # | 分歧点 | 六份方案的分布 | 本次采用 |
|---|---|---|---|
| 1 | 目录骨架 | HY4 主张留 `notes/` 伞；其余五家主张根级三分 | **根级三目录** `rewrite-notes/`、`redesign-notes/`、`study-notes/`，`book/` 不动 |
| 2 | `.review/` 1728 件历史产物 | HY4 要 sed；MiMo/muse/glm 要冻结；qwen 要 `rm -rf` | **冻结不改写**，新写 `.review/PATH-MAPPING.md` 登记新旧前缀 |
| 3 | 全貌保全方式 | MiMo 要先把未跟踪文件提交入库；HY4/glm 只打 tag+tar | **清单表格 + tar 快照，不额外提交**（不代用户提交在制内容） |
| 4 | 执行边界 | — | **全流程，分阶段交互确认**（每阶段结束停下等确认） |
| 5 | 在制两件（TODO-3ARCH / PENDING-DECISIONS） | — | **随树搬走，内容一字不改**，不代提交 |
| 6 | `notes/study` 184 件 | qwen 主张删；其余主张搬 | **原样搬到 `study-notes/`**，README 加定性说明，精简另案 |
| 7 | `rewrite-notes/` 内部子结构 | HY4 要 `00-overview` + `_` 前缀；muse 要 `00-meta`+`99-handoff`；deepseek/glm 要 misc/coordination/evidence/archive | **deepseek/glm 风格**，stage 目录名零修改 |
| 8 | 执行位置 | deepseek/qwen 要专用分支或 worktree；glm 要直接在 `rewrite` | **直接在 `rewrite` 分支**，逐阶段 commit |
| 9 | tag | HY4/MiMo 主张推 origin；本会话按审阅在同机判断 | **本地双 tag，不推远端**（`notes/pre-migrate-20261007` + `notes/post-migrate-*`） |
| 10 | `redesign-notes/` 内部 | glm 扁平；deepseek/MiMo 按主题 | **按主题子目录** `architecture/`、`ipc/`、`fork/`、`vm/`（预留）+ 根 `README.md` |
| 11 | 两份锚点基线 | HY4/qwen 要重生成；deepseek 要 sed 保行号 | **前缀替换 + 行数对账**（874 / 6609 行不变） |
| 12 | 旧路径改写范围 | 各家对白名单口径不一 | **只改必改域**：三棵树内部、`os/` 注释、`tools/`、`prompt/`+三端派生、`CLAUDE.md`、`AGENTS.md`、根 `README.md`。冻结域：`.review/`、`migrate_notes_plan/`、`tmp/`、`AI-chats/`、`new_laptop_migrate/`、`.qoder/`、`.trae/documents/`、`.claude/settings.local.json` |

> 第 12 条在下一次交互确认时补问了一次（问答返回未带上该项），当前按「只改必改域」执行；
> 若用户后来要求扩围，补一轮 sed 即可，方向是单向的（冻结→改写可追加，改写→回退更麻烦）。

---

## Phase 0 · 冻结与基线（开窗）

### 动作 0.1 现实核对（不改动仓库）

**做了什么**：通读六份方案，把它们的事实断言逐条拿命令重跑，得到 15 条偏差（详见留存文档第十节）。计划本身写于开窗前两周。

关键结论：六份方案共同的「仓库零 tag」论据已失效；`notes/study` 与 `archive_bak` 已被跟踪（原方案据此推出的「删了找不回来」不再成立）；
fork 树根散落文件从 27 个涨到 75 个（NK4C 战役新增）；`check_references.sh` 已不存在；
615MB 取证日志已退跟踪；`tools/pattern-gate.sh:5` 是六份方案都没列出的第 10 处硬编码。

**命令**（审阅方可复跑同一批）：

```bash
git tag -l | wc -l; git for-each-ref --format='%(refname:short) %(*objectname:short)' refs/tags
git branch -vv; git rev-list --left-right --count origin/rewrite...rewrite
git ls-files notes | wc -l; find notes -type f | wc -l
git ls-files notes/study | wc -l; git ls-files notes/rewrite/archive_bak | wc -l
git ls-files notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4a-iter11-18/serial_c9a.log | wc -l   # 0 = 不再跟踪
git check-ignore -v notes/rewrite/fork-syscall-rewrite/evidence/20260922-nk4a-iter11-18/serial_c9a.log
grep -rnI -o -E 'notes/rewrite|fork-syscall-rewrite|notes/study|notes/redesign' . \
  --exclude-dir=.git --exclude-dir=minix3 --exclude-dir=target --exclude-dir=target_smp \
  --exclude-dir=.cargo-shared --exclude-dir=.dockercargo --exclude-dir=__pycache__ | wc -l
grep -rn "notes/rewrite" tools/*.sh tools/*.py tools/coverage-extract/*.py
wc -l tools/anchor-suspect-baseline.txt tools/anchor-unresolved-baseline.txt
```

**结果**：偏差表 15 条，写入留存文档第十节，后续所有数字以 Phase 0 实测为准。

### 动作 0.2 打 pre-migrate annotated tag

```bash
git tag -a notes/pre-migrate-20261007 -m "……（正文含布局说明、基线 HEAD、tag 不覆盖的内容清单、对读方法）"
```

**结果**：tag 对象 `6a8f6ade6` → 指向提交 `bb8a90e05`（`git for-each-ref` 验证，见留存文档第二节）。
命名带 `notes/` 前缀是为了与既有的 9 个 minix3 上游 tag（`v3.1.3`～`v3.3.0`）区隔。

**为什么不推远端**：审阅方在本机可直接读本地 tag；仓库历史含 1.2G 取证日志，推 tag 对远端无收益（用户裁决第 9 条）。

### 动作 0.3 新写断链检查器（为建立基线，Phase 4 之前不接线进门）

**做了什么**：六份方案都要求「迁移前先跑一次链接检查建立基线」，但仓库里没有这个工具
（`tools/` 下无 link/refer 类脚本，glm/deepseek 提到的根目录 `check_references.sh` 已被删除）。
新写 `tools/notes-link-check.py`（215 行，Python，与 `tools/review-state-validate.py` 同风格）。

判定规则：跳过 fenced code block 内的行；跳过外链、纯锚点、空目标；剥掉 `#fragment`（含 `#L78-L96` 这类行号锚点）；
解析基准依次尝试「相对当前文件目录」与「相对仓库根」，两者都落空才记断链——因为存量文档里两种写法都有。

**命令与结果**：

```bash
python3 tools/notes-link-check.py --self-test     # ✅ PASS，exit 0
python3 tools/notes-link-check.py notes/ ... --output .../broken-links.before.txt
#   扫描 2228 个 md 文件，断链 452 处
```

自测夹具覆盖四种情形：代码块内断链不检、相对路径可解析、行号锚点剥离、仓库根基准与外链/纯锚点跳过。

### 动作 0.4 生成四份清单（一次性生成器 `tmp/pre-migrate-snapshot-20261007/gen-manifest.py`）

| 产出 | 行数 | 校验 |
|---|---|---|
| `manifest.notes.disk.tsv` | 2428 | 与 `find notes -type f \| wc -l` 相等；`diff` 清单路径列与磁盘列表为空（留存文档第十一节第 3 步） |
| `manifest.offtree.tsv` | 2530 | notes 之外的 2515 ignored + 15 untracked；os 构建产物区 273374 件只记总数不逐列 |
| `refs.before.counts.tsv` | 1747 文件 | 分域合计 98892 处，与逐行底账 `tmp/.../refs.before.lines.txt`（98892 行）一致 |
| `snapshot.sha256` | 7 项 | 从仓库根 `sha256sum -c` 全部 OK |

**守恒核对（三圈账闭合）**：notes 磁盘 2428 = 跟踪 1102 + 被忽略 1325 + 未跟踪未忽略 1。
生成器对每个文件标 `git_status` 列，分类漏洞自检 `awk '$5=="unknown"'` 输出为空——三类枚举互斥且完备。

**为什么清单入 git 而底账不入**：计数表（120KB）与两份 manifest（388KB+203KB）体量小、是审计的核心对照物，入库让审阅方换机也能拿到；
98892 行的逐处命中底账（6.5MB）留在 `tmp/` 并记进 `snapshot.sha256`，因为它只对本地复跑有意义。

### 动作 0.5 打 tar 快照（Git 盲区的唯一恢复源）

```bash
tar -czf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz \
    notes .review migrate_notes_plan/pre-migrate-20261007 tools/notes-link-check.py \
    CLAUDE.md AGENTS.md README.md opencode.json .qoder/specs new_laptop_migrate \
    migrate_notes_plan/agents-workflow-optim.md tools/atf-c-compat/probes/p7.c
```

**结果与验证**：29583270 字节，sha256 `39e68026…`；卷内 `notes/` 条目 2428（与 manifest 行数一致）、
`.review/` 条目 1728（与磁盘计数一致）、`.design/` 条目 1162、615177374 字节的 `serial_c9a.log` 在卷内；`tar -tzf` 全卷可读 rc=0。

**主动记下的一处局限**：tar 卷位于仓库 `tmp/` 内，而 `tmp/` 自提交 `db7ebfe5c` 起整域不入库——
这份快照与它保护的内容命运相同，磁盘上只有一份拷贝。留存文档第八节已写明，建议验收通过后复制到仓库外长期保存。

### 动作 0.6 落笔两份文档并 commit

- `migrate_notes_plan/pre-migrate-20261007/00-SNAPSHOT.md`（执行前全貌留存，362 行）
- 本执行日志
- commit 内容：tag 之外的全部基线产物（1 个新工具 + 4 份清单 + 2 份文档）

**为什么 tag 在文档之前**：tag 要指向迁移动手**之前**的提交，而基线文档本身属于迁移产物；
顺序为「打 tag → 采集 → 写文档 → commit」，这样 pre-tag 与 post-commit 之间不会混入任何迁移自身的内容。

### Phase 0 验收

| 判据 | 命令 | 结果 |
|---|---|---|
| pre-tag 存在且指向预期提交 | `git for-each-ref refs/tags/notes` | `6a8f6ade6 → bb8a90e05` ✓ |
| 三圈账闭合 | `find notes -type f \| wc -l` vs manifest 行数 | 2428 = 1102+1325+1 ✓ |
| 清单未被事后改动 | `sha256sum -c .../snapshot.sha256` | 7 项全 OK ✓ |
| tar 完整 | `tar -tzf` + `sha256sum` | rc=0，29M，4183 条目 ✓ |
| 断链基线可复跑 | `python3 tools/notes-link-check.py notes/` | 425 条（基线 notes 部分 425）✓ |
| 工具自测 | `python3 tools/notes-link-check.py --self-test` | PASS ✓ |
| 在制两件未被我改动 | `git status --porcelain` | 仍是开窗时那两条 ✓ |

**Phase 0 未做（按用户裁决刻意不做）**：不提交 15 件未跟踪文件、不改 `.gitignore`、不动 `minix3/`、不动 `book/`、不推远端。

---

## Phase 1 · 映射表（三份新树与旧树的唯一合同）

### 动作 1.1 枚举迁移面

**做了什么**：把 fork 树根 75 个散落文件逐个量化——字节数、被引用次数、引用来自哪个域（`os/` 代码注释锚点 vs 文档互引）。
数据来路：从基线引用命中的逐行底账取 `file:line` 形式，再逐名回查。

关键发现：**HY4 与 glm 两份方案都把 fork 树 `README.md` 安排去 `00-master-plan/README.md`，但那个目标已存在**
（16160 字节的《总规划说明》），照做会命名碰撞或覆盖丢文件。deepseek 的改名归档方案
（`misc/legacy-fork-syscall-index.md`）是三家里唯一可执行的一支，本次采用。

### 动作 1.2 生成映射表

```bash
python3 tmp/pre-migrate-snapshot-20261007/gen-path-map.py
# ✅ path-map.tsv 写出：2428 行（= 清单 2428 行，闭合）
#    按 kind：git-mv=1102  mv-ignored=1325  mv-untracked=1
#    按规则：R1=2 R3=3 R4=276 R5=1814 R6=1 R7=74 R8=6 R9=40 R10=1 R11=1 R12=12
#            R13=1 R15=11 R17=184 R18=1 R19=1
#    新树分布：.review=3  redesign-notes=14  rewrite-notes=2227  study-notes=184
```

生成器内置三项自检，不过就不落盘：覆盖率（未映射 0）、目标碰撞（去重后 2428 = 2428）、旧树残留（目标仍以 `notes/` 开头 0 条）。
R14（redesign 其余文件走默认桶）命中 0，说明 12 篇全部进了显式主题判定。

产出：[`pre-migrate-20261007/path-map.tsv`](pre-migrate-20261007/path-map.tsv)（机器正本）
与 [`pre-migrate-20261007/01-PATH-MAP.md`](pre-migrate-20261007/01-PATH-MAP.md)（19 条规则 + 例外表 + 冻结白名单 + 新树终态）。

### 动作 1.3 三个判断项请示用户（J1/J2/J3）

| 判断项 | 候选 | 用户选定 |
|---|---|---|
| J1 会话过程文件的窝叫什么 | coordination / workflow / worklog / _worklog / 99-handoff-archive | **`coordination/`** |
| J2 那 13 件案卷与专项台账 | 并入 coordination（一条规则）/ 单开 casefiles/ | **并入 `coordination/`** |
| J3 `misc_concepts.md` 去处 | misc/ / concepts/ / coordination/ | **`rewrite-notes/misc/`** |

三项均维持映射表初值，未产生变更；因此本 Phase 提交的就是定稿版本，审阅方可直接用
`path-map.tsv` 反查任意一个旧路径。

定稿校验：`path-map.tsv` sha256 = c43f82c5977b2892c4f181c3b731ceda4e899142080152438cfc5b1888ca89f1
（审阅方可用 `sha256sum migrate_notes_plan/pre-migrate-20261007/path-map.tsv` 对读，确认 Phase 2/3 执行期间合同本身没被偷偷改过。）

卫生门：本会话写这三份文档时复现了既有的「回改生成新错」模式，共三处中文噪声（「扫描」与「锚点」各被写成形近字一次，另有一处介词叠字），逐个回改后按纪律第 11 条重跑可疑字表扫描，三份新文档零命中（表源见 `NK4C-接续PROMPT-20261006r.md` 第 54 行纪律条目，本日志不复抄字表本身，以免记录行自己成了扫描命中项）。

### Phase 1 验收

| 判据 | 结果 |
|---|---|
| 覆盖率 100% | 2428 = 2428，未映射 0 ✓ |
| 无目标碰撞 | 去重后计数相等 ✓ |
| 旧树可完全清空 | 目标仍在 `notes/` 下的条目 0 ✓ |
| 每条目带 kind 与 rule 列 | `git-mv`/`mv-ignored`/`mv-untracked` 三态与基线 git_status 列一一对应 ✓ |
| 文风门 | 两份新文档 `doc-style-lint` error 级 0 ✓ |

---

## Phase 2 · 结构移动（纯移动，零内容改动）

### 动作 2.1 移动单元归并与干跑

`move-notes.py` 把 2428 条映射归并成 25 个目录单元（2323 件）+ 105 个单文件单元，
每个单元先断言「源存在、目标不存在」，再执行同文件系统改名（`shutil.move` 走 `rename(2)`，
1.3G 的 `evidence/` 也是秒级，全程不复制）。干跑先验一遍断言，未落任何目录。

### 动作 2.2 执行与门 M1

```bash
DRY_RUN=0 python3 tmp/pre-migrate-snapshot-20261007/move-notes.py
```

门 M1 用的是逐条比对（比多重集更强）：按映射表把每条 `旧路径的迁前 sha256` 与 `新路径的当前 sha256` 对上，
**2428/2428 一致**，目标缺失 0、内容不一致 0，哈希多重集相等。

### 动作 2.3 执行期抓到一处六份方案都没预见的静默退跟踪

`mv` + `git add` 让 **108 个「已跟踪但命中 `.gitignore`」的文件**（106 个 `*.log` + 2 个 `*.out`）
只入删除、不入新增——旧路径从索引里消失，新路径因命中忽略规则不会被加入，
等于迁移顺手把它们退出了版本管理，而 `git status` 不会报错。
`git diff --cached --name-status` 当时的形态是 994 个 R + 108 个 D（R+D=1102 恰好等于基线跟踪数，
所以数量核对发现不了，只有看「为什么有 D」才暴露）。

修法：按映射表取这 108 个条目的新路径，`git add -f` 逐个补回索引。
复核后 `git ls-files rewrite-notes redesign-notes study-notes | wc -l` = **1102** = 基线跟踪数，
`git diff --cached -M --name-status` = **1102 条 R100**，无 A/D 混杂。

六份方案都写「用 `git mv` 保历史」，本次是 `mv` + `add -f`（因为树内混着被忽略的 `.design` 与日志，
`git mv` 对整目录的移动语义在不同 git 版本上不一致）。最终索引状态与 `git mv` 等价，差异如实记录。

### 动作 2.4 拦住一次「代用户提交」的越界

`git add -A` 把你未提交的 `TODO-3ARCH-PARITY-20261006.md` 的 6 增 2 删一并带进暂存区
（`git diff --cached` 里它显示成 R092 而非 R100，是唯一的非纯改名条目，因此被发现）。
处置：`git update-index --cacheinfo 100644,<HEAD 的 blob beb452f4e>,<新路径>` 把索引退回原内容，
工作树不动。提交后该文件在 `git status` 里仍是 ` M`（你的改动完好），门 M1 的 2428/2428 也仍然成立。

### Phase 2 验收

| 判据 | 结果 |
|---|---|
| 提交形态 = 纯改名 | `1102 files changed, 0 insertions(+), 0 deletions(-)` ✓ |
| 内容零改动 | 门 M1 逐条 sha256 一致 2428/2428 ✓ |
| 跟踪数守恒 | 新树 `git ls-files` = 1102 = 基线 ✓ |
| 旧树清空 | `notes/` 不存在 ✓ |
| 无嵌套错误 | 无 `rewrite-notes/evidence/evidence` 形态；新树 2425 + `.review` 归档 3 = 2428 ✓ |
| 在制内容未被代提交 | 台账索引 = HEAD blob，工作树仍 6/2 ✓ |

---

## Phase 3 · 引用重写（必改域路径前缀 + 相对链接重定基）

### 动作 3.1 范围与冻结（用户裁决）

必改域：三棵树内部的 `.md`/`.txt`、`os/` 的 `.rs`/`.toml`/`.md`（注释与文档字符串）。
树内三个历史区**不改写内容**（用户裁决）：`rewrite-notes/evidence/`（取证日志）、
任何 `.design/`（设计快照）、`rewrite-notes/archive/legacy-fork-bak/`（fork 时代备份）——
实测这三区里有 68 个文件、424 处旧路径，保留原文就是「当时文档写在哪」的历史事实，与 `.review/` 同理。
冻结区只冻结「文件内容」，指向它们的链接照样重定基。

### 动作 3.2 路径前缀重写（`rewrite-refs.py`）

规则 135 条 = 单文件例外 108 + 目录前缀 24 + 树根裸名兜底 3，按旧路径长度降序应用
（qwen 的顺序纪律：短前缀先吃长前缀会留下 `rewrite-notes/fork-syscall-rewrite/` 这种半截残留）。

结果：413 个文件、1508 处替换。三条硬约束都做了机器核验：

| 约束 | 核验命令 | 结果 |
|---|---|---|
| 不增删行（锚点基线行号继续有效） | `git diff --numstat` 逐文件比 +/− | 非对称文件数 **0**（唯一例外是你在制的台账，那 6/2 是你的改动） |
| 不刷行尾（本仓有 CRLF 文件史） | 与 tar 基线对读同一文件的 CR 行数 | 两边都是 0，字节级替换天然不动行尾 |
| 不误伤代码 | `git diff -U0 -- os/ \| grep '^+' \| grep -vP '^\+\s*(//\|/\*\|\*\|#)'` | 仅 1 行，是 `minix-types/README.md` 的 markdown 列表项；88 个 `.rs` 全部落在注释内 |

裸名提及（不带路径形态的 `fork-syscall-rewrite`）**不自动改**，出清单交人工判定：
实测 73 行，抽样确认全是历史叙述（如 `00-master-plan/README.md:5` 讲「本目录的文档主线已调整」、
`doc_rerank_HY4.md:98` 引用旧树名做统计口径）。按裁决保留原文，清单留在
`tmp/pre-migrate-snapshot-20261007/bare-mentions.txt` 供审阅方逐条复核。

### 动作 3.3 相对链接重定基（`relink.py`，两轮才做对）

**第一轮漏了一类，被 门 M3 抓出来。** 第一版只处理「旧目标是被搬走的那个文件」的情形；
门 M3 实跑出 **16 条迁移自己引入的新断链**，全是第二类：链接目标本来指向仓库根的 `os/`、`minix3/`
（位置没变，但树从 4 层搬到 2 层，`../../../os/...` 的层数就不够了）。补规则后第二轮再修 20 条。

两轮合计 138 处重定基（118 + 20）。**留档缺陷**：`relink.py` 每轮都覆盖式写同一份报告文件，第一轮那 118 条清单已被第二轮的 20 条覆盖，138 这个数字目前只能由本日志两次运行的输出自证，没有可复算的底账（GLM 与 MiMo 两方都指出）。复算替代路径：门 M3 的断链集合对账 + 行级归因（三家审计各自独立做过，结论一致）。后续同类工具应把每轮报告分文件写。典型形态：

```
rewrite-notes/00-master-plan/README.md      ../README.md                →  ../misc/legacy-fork-syscall-index.md
rewrite-notes/02-stage-vm/draft/00-vm-overview.md  ../../../concepts/README.md  →  ../../concepts/README.md
redesign-notes/README.md                    architecture-changes.md     →  architecture/architecture-changes.md
rewrite-notes/coordination/NK4C-WORKLOG.md  ../../../os/kernel/src/proc.rs  →  ../../os/kernel/src/proc.rs
```

第一条是本次最危险的形态：**链接文本没变、仍然可解析，但解析到了另一个文件**
（`../README.md` 在新树里指向重写后的新索引，而不是被归档改名的旧索引）。断链检查抓不到它，
只有按映射表反查目标才能发现——这也是为什么必须用脚本按合同重算而不是「数一下断链条数」。

### 动作 3.4 三道门

| 门 | 判据 | 结果 |
|---|---|---|
| 门 M2 零残留 | 必改域内 `notes/rewrite`、`notes/study`、`notes/redesign` 命中文件数 | **0 / 0 / 0** ✓ |
| 冻结区未被误改 | 三区旧路径命中文件数应与基线测量一致 | `.design` 56、`evidence` 4、`legacy-fork-bak` 8（= 实测值）✓ |
| 门 M3 断链只减不增 | 基线逐条映射进新坐标后与迁移后集合对读 | 基线 288 条 → 迁移后 257 条；**新增 0**；消失 31 条全部可解释（深度变浅后反而解析成功，或重定基指向正确目标）✓ |

### Phase 3 遗留（如实登记，不顺手修）

- 旧阶段编号引用（`01-stage-pm`、`03-stage-kernel`、`04-stage-vfs`、`05-stage-sched`）与迁移前就坏的
  相对链接照原样保留，前缀替换后它们仍指向不存在的目录（迁移前也一样）。这是用户裁决第「不顺手修」条，
  属迁移后的独立任务。
- `os/libs/minix-types/README.md:125` 指向的 `fork-syscall-plan.md` 迁移前就不存在
  （`find` 只在 `archive/legacy-fork-bak/` 里找到 `fork-syscall-plan-part1.md`/`-part3.md`/`-backup.md`），
  机械前缀替换把它写成了 `rewrite-notes/fork-syscall-plan.md`，仍是悬空。后继文档建议判定为
  `rewrite-notes/00-master-plan/01-project-overview.md`（现行总览），已登记进 Phase 7 的 MIGRATION.md 待人裁决。
- 406→503 条「迁移前即悬空」的链接清单在 `tmp/pre-migrate-snapshot-20261007/relink-report.tsv`。
  两轮之间数字变大不是回归：第二轮把「指向仓库根真实文件」的那一类从"无法解析"改判为"可重定基"，
  剩下的才留在悬空桶里。

---

## Phase 4 · 工具链适配 + 规则三端同步

### 动作 4.1 引入唯一路径真源

新增 `tools/notes-layout.conf`（25 行）：三棵树目录名 + 树别名 + 状态目录键约定说明。
五个脚本改为 `source` 该文件，不再各自写死目录名（用户裁决：要配置文件，不要写死）。
本段原写「六个」，把 `check-review-rules.sh` 也计了进去——它不 source 真源，改的是规则文本里的字面断言，行为正确但归类别；此措辞由 DS 审计指出后改正。

**踩到的一次自伤**：该文件由工具写入时带上了 CRLF 行尾，被 bash `source` 时报
`line 16: $'
': command not found`，四个脚本同时失灵且报的是「配置文件语法错」而不是「路径找不到」。
处置：统一转成 LF（与本仓规范一致，抽样 `tools/doc-style-lint.sh`、`CLAUDE.md`、`prompt/review-rules/review-process.md`
均为 LF）。同批把本会话早先写出的 `tools/notes-link-check.py`、`pre-migrate-20261007/00-SNAPSHOT.md`、
`01-PATH-MAP.md` 三份也查出来是 CRLF 并转为 LF。教训：**新工具产出的文件要按仓库既有规范验一次行尾**，
否则一个不可见的字节会让一个 shell 脚本整体不可用。

### 动作 4.2 六个脚本的路径模型改造（三层 → 两层）

| 脚本 | 改什么 | 验证命令与结果 |
|---|---|---|
| `tools/review-init.sh` | 按 `{tree}/{stage}/{doc}.md` 解析；`{stage}` 顶替 `{module}` 做状态目录键；省略树前缀时逐树探测；旧布局路径给出「已退役 + 对照表位置」的可读报错 | `review-init.sh claude rewrite-notes/08-stage-is/00-is-overview.md m3` → Derived Paths 表 tree=rewrite-notes、stage=08-stage-is、状态目录 `.review/claude/08-stage-is/`，并正确命中 `.design/00-design.v1.md` ✓ |
| `tools/design-coverage-check.sh` | 第一参由 module 改为 stage 或树别名（`rewrite`/`redesign`/`study`）；`MODULE_DIR` 读 conf；JSON 键 `module` 换成 `tree`+`arg` | `01-stage-kernel` → 53 doc、36 complete；`rewrite` → 546 doc；`fork-syscall-rewrite` → exit 2 带可读报错 ✓ |
| `tools/review-gate-check.sh` | 第二参由 module 改为 stage；在三棵树里定位 `<stage>/<doc-stem>.md`；定位失败 exit 2（不再静默报「快照缺失」） | `claude 08-stage-is 00-is-overview m3` → 设计/大纲快照均在新路径命中 ✓ |
| `tools/design-index-update.sh` | 用法示例改 `{tree}/{stage}`；传入旧布局 `notes/...` 路径直接报错；索引标题带上所在树 | 在 `/tmp` 副本上跑 110 个快照的 stage → exit 0，DESIGN-INDEX.md 正常生成 ✓ |
| `tools/doc-style-lint.sh` | **增量门 pathspec 由 `-- notes/rewrite` 改为三棵树**（这是六份方案都点名的静默失效点） | `--self-test` PASS；范围摘要行改为打印三棵树 ✓（埋点正向验证见 Phase 6 门 M5） |
| `tools/check-review-rules.sh` | 字面断言 `notes/rewrite/{module}/{stage}/.design/` → `{tree}/{stage}/.design/`，与规则文本同笔修改 | `check-review-rules.sh` 输出 consistent、exit 0 ✓ |

另外三处工具文本同步：`tools/coverage-extract/coverage-extract.py` 示例路径、
`tools/pattern-gate.sh` 头部注释（六份方案都没列出的第 10 处硬编码）、
`tools/review-state-validate.py` 与 `tools/verify-check.py` 的用法示例（`{module}` → `{stage}`）。

### 动作 4.3 规则域机械重写 + 语义改写

机械层：`rewrite-rules.py`（与 Phase 3 共用 path-map 派生的规则表，另加 6 条路径模型规则）
改写 29 个文件 / 491 处。新增的四类模型规则：
`notes/rewrite/{module}/{stage}` → `{tree}/{stage}`（208 处主干模板）、
`.review/{trae|claude|codex|{tool}}/{module}` → `…/{stage}`（约 140 处状态目录模板）、
`design-coverage-check.sh {module}` → `{stage}`、`# Review State: {module}` → `{stage}`。

语义层（需要改句子而不是换字符串，共 7 处 + 派生副本）：
`prompt/review-rules/review-process.md` 的 `{module}` 定义段、
`prompt/skill/review-process-skill.md` 的路径变量段与「两个概念」段、
`prompt/README.md` 的路径变量段、`CLAUDE.md` 的 State Management 段、`AGENTS.md` 的目录布局段、
`.claude/rules/review-process.md` 与 `.claude/skills/review-scan/checks/process.md`（含 `.codex` 孪生副本）的英文定义句。

保留不动的 `{module}` 有 152 处：它们指 Minix3 模块名（`minix3/minix/servers/{module}/`、
`os/servers/{module}/src/`、`coverage-extract.py {module}`、`{module}-semantic-map.json`），
与退役的路径 module 层是不同概念——这正是规则原文反复强调「不得混用」的那一对。
两处标题注释改写为显式声明「以下 `{module}` 指 Minix3 模块名，不是评审状态目录键 `{stage}`」。

### 动作 4.4 三端派生同步

```bash
bash tools/generate-derived-skills.sh        # GEN 全部 9 个 skill × trae/codex
bash tools/generate-derived-skills.sh --check # ✅ No drift detected
bash tools/check-review-rules.sh              # consistent，exit 0
bash tools/lint-review-rules.sh               # 0 个失败
bash tools/diff-trae-skills.sh --only-diff    # ✅ 全部 9 个 skill 完全同步
```

派生脚本自身的 sed 规则也有一处 `trae/{module}` → `codex/{module}` 的适配，随键名一起改为 `{stage}`。
**这里我自己制造了一次故障**：替换时把 sed 左值的花括号转义丢了（`├── trae/{stage}/`），
POSIX sed 把 `{` 当区间表达式定界符，报 `Invalid content of \{\}`，
派生中途失败并让 `.codex/skills/review-code-skill/SKILL.md` 半损坏
（`check-review-rules.sh` 随即报 name 不匹配 + description 未加引号）。
处置：左值改回 `├── trae/\{stage\}/`，重跑生成，`--check` 与 `check-review-rules.sh` 双双回到干净。
教训：**改工具脚本内部的正则/sed 片段时，转义层级属于语义的一部分**，
改完必须 `bash -n` + 真跑一次派生 + 跑一致性门，不能只肉眼读。

冒烟测试留下的痕迹已清理：`review-init.sh` 会创建 `.review/claude/08-stage-is/STATE.md` 骨架与
`00-is-overview/` 子目录，测试后按 mtime 定位并删除，使该冻结目录回到只含原有 `SYMBOLS.md` 的状态。

### 动作 4.5 新增两份映射文档

- `rewrite-notes/MIGRATION.md`（125 行，已入 git）：这次迁移的动机、目录级与文件级对照、
  75 个散落文件的去处、评审路径模型新旧对照（工具参数逐个列出）、阶段编号重排史、
  已知悬空引用、旧内容回捞方法、以及「本次刻意没做的事」清单。
  工具脚本报错文案、CLAUDE.md/AGENTS.md、`.review/PATH-MAPPING.md` 都指向这一份。
- `.review/PATH-MAPPING.md`（26 行，在 gitignore 目录内，只作本地便利指针）：
  说明 `.review/` 为什么冻结不改写、新旧前缀对照、新 review 的状态目录约定。

### Phase 4 验收

| 判据 | 命令 | 结果 |
|---|---|---|
| 六个脚本语法完好 | `bash -n` × 6 + `py_compile` | 全过 ✓ |
| 三端规则一致 | `generate-derived-skills.sh --check` / `check-review-rules.sh` / `lint-review-rules.sh` / `diff-trae-skills.sh --only-diff` | 四条全绿 ✓ |
| 工具在新布局上真能用 | review-init / review-gate-check / design-coverage-check / design-index-update / doc-style-lint / anchor-resolve / anchor-migrate / unsafe-audit 自测与真实调用 | 全部 exit 0 ✓ |
| 门 M2 必改全域 | 全域 `notes/(rewrite\|study\|redesign)` 命中计数 | 三棵树、`os/`、`prompt/`、`.codex/`、`.trae/`、根 README 全 0；剩余 41 处全部是刻意保留项：`.claude/settings.local.json` 38（授权命令历史，按裁决冻结）+ `tools/notes-layout.conf`、`CLAUDE.md`、`AGENTS.md` 各 1（描述退役本身的句子）；`tools/anchor-*baseline.txt` 另计（Phase 5 处理）✓ |
| 文风门 | `doc-style-lint.sh rewrite-notes/MIGRATION.md` | error 级 0 ✓ |

---

## Phase 5 · 两份锚点基线路径前缀替换

六份方案在这里分两派：HY4 与 qwen 主张「重生成，不要 sed」（理由是行号与语义位置都会变），
deepseek 主张「前缀替换 + 行数对账」。本次按用户裁决取后者，理由是可验证性更强：
重生成会连带改变锚定符号集，把尚未复核的 868 处历史债标记一次性洗掉，
而替换前后可以逐行对账；行号安全性由 Phase 3 的「单行内替换、行数不变」纪律保证。

实测两派的前提都成立，但影响面比两份方案的说法都小：

```
tools/anchor-suspect-baseline.txt     874 行 → 874 行；替换 868 处；替换后旧路径残留 0
tools/anchor-unresolved-baseline.txt  6609 行 → 6609 行；替换 1 处（只有第 2 行的范围注释含旧路径，
                                       其余 token 本身是符号名不带路径）
```

替换同样走 Phase 1 的合同规则表（不是手写四条 sed），并且做完一条正向核对：
把 suspect 清单里出现的文档路径逐条 `test -f`，**缺失 0**——
这证明新路径确实指向迁移动过的文件，而不只是字符串换掉了。

门 M7 卫生检查同时跑过：三棵新树无空目录，工作树残留只有用户在制文件与原有未跟踪件。

## Phase 6 记录（编译与工具门，逐条证据见 02-VERIFY.md）

Phase 6 的门禁证据单独落在 `migrate_notes_plan/pre-migrate-20261007/02-VERIFY.md`，
本文只记过程与判断。

### 门 M5 的两次埋点（这次迁移最关键的一门）

六份方案里 HY4 单独强调：`doc-style-lint.sh` 与锚点基线属于**静默失效**——不报错，只是什么都不查。
所以不能只看「门跑通了」，必须故意埋一个坏东西看它能不能抓到。

| 埋点 | 做法 | 结果 |
|---|---|---|
| 断链 | 在 `rewrite-notes/01-stage-kernel/` 放一份临时文档，引用一个不存在的 `.md` | `notes-link-check.py` 抓到 1 条 ✓ |
| 坏锚点 | 同一份临时文档里写 `os/kernel/src/lib.rs:fn probe_symbol_that_does_not_exist` | `anchor-resolve.sh --check` 报 ZERO-DEF 并 **exit 1** ✓（第一次测退出码时我用了管道，量到的是 `tail` 的状态；重测确认工具本身返回 1） |
| 文风增量门 | 往 `rewrite-notes/misc/misc.md` 追加一行含裸日期与 `Gate X` 的文本，跑 `--diff` | 门的范围行打印「限 rewrite-notes redesign-notes study-notes」，并抓到 `SL-4` 与 `SL-7` 各一条 ✓（旧写法钉在 notes 前缀上时，这两条会静默漏过） |

三份埋点文件验证后全部撤销：临时文档删除、`misc.md` 按备份逐字节复原，
`git status` 回到埋点前的形态（只有基线两文件与用户在制件）。

### 门 M6 的编译面

`cargo check --workspace --tests`（os/ 工作区，本地 `-j 2`）exit 0，零 error。
411 条 warning 全是死代码类告警，与迁移无因果：`os/` 的改动逐行核验只在注释与文档字符串内
（Phase 3 的门已量过：非注释新增行仅 1 行，是 `minix-types/README.md` 的列表项）。
为把这句话变成可核对的证据，另在迁移前 tag 的临时工作树上跑同一条命令做告警数对读，结果写进 02-VERIFY.md。

---

## Phase 7 · 收尾（三份入口文档 + 交付物 + 终态 tag）

### 动作 7.1 三份 README 与一份存根

| 文件 | 内容 | 门 |
|---|---|---|
| `rewrite-notes/README.md` | 重写为新入口：20 个阶段目录逐个带正式文档数、阶段内 `draft/`、`archive/`、`.design/` 三类子目录说明、五个功能区（`concepts/`、`misc/`、`coordination/`、`evidence/`、`archive/`）用途与计数、从零开始的阅读路线、与 `os/`、`minix3/`、`book/` 的关系 | 文风门 error 级 0 |
| `redesign-notes/README.md` | 顶部加状态段：本区是重写定稿后才启用的探索区，当前内容是从两处原样迁来的存量思考，含互相冲突的方案，不能当实现依据；开新方向前先收敛同主题已有分析 | 同上 |
| `study-notes/README.md` | 定性 + 使用规则：AI 生成、未逐篇对 C 源码校验、正式文档不引用、只可当线索；并写明精简与删除是独立任务（附理由） | 同上 |
| `redesign-notes/vm/README.md` | 空目录存根：说明这是「VM 是否内进内核地址空间」这条未裁决路线的落点。git 不跟踪空目录，不写这份文件该目录就不存在 | 同上 |

`rewrite-notes/00-master-plan/README.md` 的启动顺序表里有一行 `20 | 20-redesign/`，
那个目录已被本次迁移移走——**这是迁移自己造成的失真**，所以改了那一行指向新位置
（只改这一行，不顺手重写该文档的其余叙述）。

### 动作 7.2 交付物清点

用户要求的三件都在：

1. 执行前全貌留存：`pre-migrate-20261007/00-SNAPSHOT.md` + 四份清单 + `snapshot.sha256`
2. 执行详细日志：本文件（Phase 0 到 Phase 7 逐动作，含三处自造故障与两处越界的如实记录）
3. 审计交接 PROMPT：`migrate_notes_plan/REVIEW-PROMPT.md`
4. 附带：`rewrite-notes/MIGRATION.md`（入 git 的永久对照表）、`pre-migrate-20261007/02-VERIFY.md`（门禁取证）、
   `.review/PATH-MAPPING.md`（gitignore 内本地指针）

### 动作 7.3 又摔了一次同一个坑（第二次）

Phase 7 提交前用 `git add -u -- rewrite-notes ...` 暂存文档改动时，
又把维护者的在制台账 `TODO-3ARCH-PARITY-20261006.md`（未提交的 6 增 2 删）带进了暂存区——
与 Phase 3 是同一个失误。这次靠 Phase 5 之后新加的「每次 add 之后必查暂存清单」这一步当场抓到：
`git diff --cached --name-only | grep TODO-3ARCH` 命中 1，
于是把索引退回迁移前 blob `beb452f4e`（工作树内容不动，改动仍留在工作树未提交）。
教训升级为硬性步骤：**只要工作树存在他人/用户 in-progress 文件，任何 `git add -u` 或 `git add -A` 之后
都要立刻跑一次「暂存清单里不该出现它们」的断言**，不能靠记忆。

### 动作 7.4 终态 tag

```bash
git tag -a notes/post-migrate-20261007 -m "……"      # 指向 Phase 7 提交
git diff --stat notes/pre-migrate-20261007..notes/post-migrate-20261007 -- notes | tail -1   # 旧树消失量
```

两个 tag 的对读方法是本次审计的主线：`notes/pre-migrate-20261007` 是旧布局最后状态（内容层），
`notes/post-migrate-20261007` 是新布局第一个状态，中间六个提交分别是基线、映射表、纯移动、
引用重写、工具与规则、锚点基线、收尾文档。

---

## 收口状态

| 阶段 | 提交 | 一句话结果 |
|---|---|---|
| Phase 0 | `d29b0f191` | 全貌留存闭合（2428 = 1102 + 1325 + 1），pre-tag 与 tar 落位 |
| Phase 1 | `5ae87f503` | 2428 条映射合同，覆盖率 100%、无碰撞、旧树零残留 |
| Phase 2 | `e90e4173d` | 纯移动：`1102 files changed, 0 insertions(+), 0 deletions(-)`；抓到并补回 108 个静默退跟踪 |
| Phase 3 | `c2ef81f79` | 必改域旧路径归零；相对链接重定基 138 处；新断链 0 |
| Phase 4 | `c399d33ae` | 六脚本 + 规则三端改到新路径模型；修 doc-style-lint 静默失效 |
| Phase 5 | `90ab7c744` | 两份锚点基线换前缀，行数 874/6609 不变 |
| Phase 6 | 并入收尾提交 | 八道门取证落 `02-VERIFY.md`；静默失效埋点三类全抓到 |
| Phase 7 | 本笔 | 三份 README + MIGRATION.md 修订 + 审计 PROMPT + post-tag |

未做且已登记为独立任务的：旧阶段编号与迁移前既有断链（452 条基线，迁移后 257）；
`fork-syscall-plan.md` 悬空引用的后继判定；`study-notes/` 精简；两个 `.backup` 合并；
`evidence/` 1.2G 与 tar 卷的长期存放（建议复制到仓库外）。

---

## 收口后的两处自查补交（同一类根因：新文件与暂存清单没复查）

### 补交一：`tools/notes-layout.conf` 漏入库（P0 级，已修）

Phase 4 与 Phase 7 都用 `git add -u -- tools` 之类的**只更新已跟踪文件**的方式暂存，
而 `tools/notes-layout.conf` 是本次新建的文件，一直没进过索引。后果不是「本机坏了」——本机一切正常——
而是**新检出上六个工具全废**（`source` 一个不存在的文件，或在 `set -u` 下变量未定义），
本次「工具链已适配」的结论只在当前工作树成立。

补交提交 `964e3e28e`，并做了新检出的真实验证（不只是 `git ls-files` 看一眼）：

```bash
git worktree add --detach .wt/verify-layout HEAD
cd .wt/verify-layout
ls tools/notes-layout.conf                                    # 存在
bash -c 'source tools/notes-layout.conf && echo ${NOTES_TREES[*]}'
#   → rewrite-notes redesign-notes study-notes
bash tools/design-coverage-check.sh 01-stage-kernel; echo $?  # rc=1（发现缺失＝业务结果，非 rc=2 用法错误）
cd .. && git worktree remove --force .wt/verify-layout
```

（新检出的 `.design/` 被 gitignore 排除，所以覆盖率工具在那个树上必然报缺快照——
rc=1 正是预期的业务结果，说明它确实找到了 `rewrite-notes/01-stage-kernel` 这棵树。）

### tag 重打记录

`notes/post-migrate-20261007` 第一次打在 `3ac2e12de`，因为上面这个漏项重打到 `964e3e28e`。
两个提交都只在本地、未推送，重打不影响任何共享历史；原因写进了 tag 注释正文最后一条。

### 根因与固化步骤

两次失误（在制文件被代提交、新文件漏入库）同一个根因：**add 之后没复查暂存清单的构成**。
固化成两条硬步骤，后续任何迁移类任务照抄：

1. `git add` 之后立刻跑 `git diff --cached --name-only | grep -E '<他人 in-progress 路径>'`，期望 0 命中。
2. 任何**新建**文件必须出现在 `git add` 的显式路径清单里，并在提交后跑一次
   `git ls-files <新文件路径>` 确认已跟踪；交付前在临时 worktree 里做一次「新检出可用性」验证。

---

## 四方独立审计的回应（GLM／DS／MiMo／Muse 各一份报告，逐条核对后修或驳回）

四份报告的一致结论：**P0 = 0**，内容层、跟踪层、引用层、门禁层、编译层五面通过独立复算。
下面逐条给出处置。核对方式一律是「自己跑命令复算，不采信叙述」，包括对审计方自身的两处纠正。

### 已修的实质问题

| 发现 | 提出方 | 我的复算 | 处置 |
|---|---|---|---|
| 状态目录键改名漏了第二种拼写 `{rw-module}` | DS 报告 P1 段第 1 条、Muse 报告 P1 段第 1 条（独立复现） | 实测 77 行 / 11 个文件，且 `review-process.md:1240` 的注释把它解释成「rewrite 模块名」= 已退役的那个概念 | 源与手工副本 50 处改 `{stage}`，三条消歧注释重写为「`{minix3-module}` 是 Minix3 模块名 vs `{stage}` 是评审状态目录键」，重跑派生与四门（`--check` 无漂移、`check-review-rules` consistent、`lint-review-rules` 0 失败、`{rw-module}` 残留 0，`{minix3-module}` 72 处按概念保留） |
| 三处面向未来的陈旧命令示例仍用旧参数 | DS 报告 P2 段第 5 条、MiMo 报告 P2 段第 5 条 | 实测确认 `prompt/todo_plan.md:826`、`prompt/skill/review-coverage-skill.md:92`、`rewrite-notes/09-stage-init/plan.md:170` 照抄会拿到「已退役」报错 | 三处就地改为新参数；历史台账里的回溯记录（`02-stage-vm/todo.md` 等十余处）按裁决保留原文，并在 MIGRATION.md 第六节记明这条边界 |
| MIGRATION.md 第五节旧编号残留数抄自计划、口径未标注且已过期 | DS 报告 P2 段第 4 条 | 我复算：`01-stage-pm` 28、`03-stage-kernel` 76、`04-stage-vfs` 7、`05-stage-sched` 7、`deep-analysis` 11（计划写的是 26/71/6/6/6） | 表里换成实测值 + 增列命中文件数 + 写明口径与复算命令，并注明「原抄自计划、已过期」 |
| MIGRATION.md 第六节断链口径混用（452 全域原始行 vs 257 三棵树去重键） | DS 报告 P2 段第 3 条 | 我复算：三棵树原始行 425→373，全域 452→400，去重键 288→257 | 三种口径并列写出，每种都标「新增 0」 |
| MIGRATION.md 第六节少登记两处散文悬空 | GLM 报告 P2 段第 3 条 | 两条都成立：`mproc-design.md:2654` 我额外用 tar 卷验了迁移前坐标同样悬空（真实文件在 `archive/legacy-fork-bak/mp-flags-analysis.md`）；`doc_rerank_deepseek.md:2184` 的 `edge*.md` 通配确实因文件进 `coordination/` 而落空 | 两条补登进第六节 |
| `00-SNAPSHOT.md` 里 p7.c 的 sha256 前缀少一位 | GLM 报告 P2 段第 1 条、Muse 报告 P2 段第 3 条 | 实测 `b8da7f7b7276…`，留档写 `b8da7f7b726` | 表格改对（内容无恙，tar/磁盘/工具三方读数一致） |
| tar 卷被称「Git 盲区的唯一恢复源」，实际 784 件被忽略文件不在卷内 | MiMo 报告 P2 段第 2 条 | 复算：`tmp/` 409、`tools/` 308、`book/` 43、`os/` 22、两个配置件，确实不在卷内；15 件未跟踪件全在卷内 | 标题与正文收窄为「notes 与 .review 两区」，并新增「卷外范围」段逐项列明，同时写明 `manifest.offtree.tsv` 不含 sha256 列这一天然局限 |
| 执行日志称「六个脚本 source 真源」 | DS 报告 P2 段第 2 条 | 实际 5 个（`check-review-rules.sh` 改的是字面断言，不 source） | 措辞改「五个」并写明为何它不在此列 |
| 门 M2 取证块 0 命中时无输出、不可读 | GLM 报告 P2 段第 5 条 | 成立 | 在取证块里显式写出「0 命中」的可读说明 |
| 交接件第 1 项判据「一致 2428 / 不一致 0」与迁移自身的有意重写冲突 | DS 报告 P1 段第 2 条、Muse 报告 P2 段第 1 条、GLM 报告 P2 段第 2 条、MiMo 报告 P1 段第 1 条（四家同指） | 完全成立：Phase 3/7 本就要改 333 个文件内容，字面照抄会把合规迁移误判成 P0 | 判据改为三段式（缺失必须 0 + 差异必须可归因 + 纯移动纯净性看 `e90e4173d`），脚本内的期望串一并改 |
| 交接件第 2 项 tar 计数命令把目录条目算进去 | 四家同指 | 复算：不筛 2536/1180，筛后 2428/1162 | 命令补 `grep -v '/\$'`，并加 `.review/` 一条（1728） |
| 交接件第 3 项两个期望值过期（跟踪数 1102、name-status 只有 R） | MiMo 报告 P1 段第 2 条、GLM 报告 P2 段第 2 条 | 复算：跟踪 1105 = 1102 + 3 份新入口文档；区间 diff 1100 R + 5 A + 2 D，两条 D 是相似度掉出改名阈值 | 期望值改为 1105 与「R+A+D 且必须逐条验目标在位 + `--follow` 可续 + 移动提交处是 R100」 |
| 02-VERIFY 门 M1 的等式写错（新树 2428 + 归档 3 ≠ 2428） | MiMo 报告 P2 段第 1 条 | 成立：正确关系是 2428 = 2425 + 3 新增；映射 2428 = 2425 + 3 归档 | 等式重写 |
| 「全量测试偶发抖动」的判读错误 | DS 报告 P2 段第 8 条、GLM 裁决回应、MiMo 报告 P2 段第 4 条、Muse 交叉 | 我自己复跑坐实：`--no-fail-fast` 下 **10 个测试二进制 SIGSEGV + 1 条断言失败 = 11 个目标**，4586 用例通过；单跑 `-p minix-sys` 316/0、`-p minix-rt` 59/0 全绿；崩溃对象固定，不是随机 | **接受改判**：02-VERIFY 观察项整段重写为「先于迁移存在、由构建配置（特性统一）决定的确定性缺陷」，并解释我先前看到的「换 crate」来自 fail-fast 与 `--exclude` 改变命令形态；同时按用户要求把这条健壮性问题登记成 WORKLOG 待办 T-STAB-1（含甲乙丙三个待查方向与门形建议） |
| relink 报告被第二轮覆盖，138 无底账 | GLM 报告 P2 段第 4 条、MiMo 报告 P2 段第 3 条 | 成立：现存的 `relink-report.tsv` 只有第二轮 20 条 | 日志里写明覆盖事实、给出替代复算路径（门 M3 + 行级归因），并记下「同类工具每轮报告应分文件写」 |

### 驳回或降级的主张（附理由）

1. **MiMo 报告 P2 段第 6 条「树外清单里两件文件字节数对不上」**：不是缺陷。清单是开窗时点快照，
   `tools/notes-link-check.py` 在快照之后被我自己修订（8135→7950），`__pycache__` 是再生成缓存。
   该条自己的判读（「不影响判定，措辞值得注明」）我采纳，已在 `00-SNAPSHOT.md` 第八节的清单局限里注明。
2. **DS 报告 P2 段第 1 条 与 MiMo 未能核对项 3 对「清单该含 sha256」的暗示**：保持现状。
   树外那 2530 件里 2515 件是构建产物与缓存，逐文件哈希的体积收益不抵噪声；
   真正需要逐字节保的是 notes 与 .review，两者都有 sha256 或 tar 卷。已把这条局限写进文档而不是改清单。
3. **Muse 第 6 项「不做工作树内埋点」的方法差异**：不算问题。三家（含我）都已用真实埋点证明门活性，
   Muse 用静态范围核对 + 自测等价替代，结论一致；其未核项自述准确。

### 我自己追加的一条纪律

四方审计里有三家独立指出交接件判据不可达（第 1 项「不一致 0」）。这是我在写交接件时把
「Phase 2 时点的性质」错写成「终态时点的性质」——同一句话在两个时点上真值不同。
以后写审计交接件，判据必须绑定到具体提交（`e90e4173d` 处验纯移动，`HEAD` 处验可归因性），
不能只写一个数字。

### 四方审计留档的完整性

`migrate_notes_plan/{GLM,DS,Mimo,Muse}/` 三份报告 + 证据 + 审计脚本全部保留、不删改，
它们的报告结论与我的处置逐条可对照。审计方对本仓库的写入仅限各自目录，工作树在审计前后一致。
