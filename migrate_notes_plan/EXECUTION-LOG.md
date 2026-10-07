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
