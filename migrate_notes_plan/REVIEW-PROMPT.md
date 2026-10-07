# 交给独立审阅方的审计 PROMPT（notes 目录迁移）

> **创建**: 2026-10-07。使用方式：把下面「=== 复制这一段给审阅 agent ===」到文件末尾的内容整段贴给
> 一个**没有参与本次迁移**的会话（换工具、换模型）。它设计成不依赖本会话的任何记忆也能开工：
> 所有事实都给了可复跑的命令，审阅方不需要相信叙述，只需自己跑命令对账。

---

## 给人类的说明（不用贴给审阅 agent）

- 被审对象：仓库 `/home/xzhao/github/minix-rs`，分支 `rewrite`，迁移提交链
  `notes/pre-migrate-20261007..notes/post-migrate-20261007`。
- 三份审计材料：
  1. 迁移前全貌：`migrate_notes_plan/pre-migrate-20261007/00-SNAPSHOT.md` + 同目录四份清单
  2. 逐动作执行日志：`migrate_notes_plan/EXECUTION-LOG.md`
  3. 门禁取证：`migrate_notes_plan/pre-migrate-20261007/02-VERIFY.md`
- 建议审阅时长：15–25 分钟（命令都是秒级，只有第 7 项的编译对读要等）。
- 审阅方**只读**：不要修改任何文件、不要 `git add`、不要 `git checkout`/`reset`/`clean`
  （工作树里有维护者的在制文件，见 prompt 第 0 条）。

---

## === 复制这一段给审阅 agent ===

你是独立审计方。仓库 `/home/xzhao/github/minix-rs`（分支 `rewrite`）刚完成一次**笔记目录结构迁移**：
把 `notes/{rewrite,study,redesign}` 三棵子树搬到仓库根的 `rewrite-notes/`、`redesign-notes/`、`study-notes/`，
并剥掉中间那层 `notes/rewrite/fork-syscall-rewrite/`，同时把全仓引用、工具链、规则文档改成新路径模型。

你的任务**不是复核我的结论，而是自己跑命令证明或推翻它**。全程只读：
不修改任何文件、不执行 `git add` / `git commit` / `git checkout` / `git reset` / `git clean`。

### 第 0 条：先认清边界

工作树里有两个文件是维护者自己在制的，不属于本次迁移产物，**不要动也不要据此判定迁移有问题**：

```bash
git status --porcelain | grep -v '^??'      # 只应看到 TODO-3ARCH-PARITY-20261006.md 一处 M
git status --porcelain -uall | grep '^??'   # 未跟踪件：new_laptop_migrate/、agents-workflow-optim.md、
                                            # rewrite-notes/coordination/PENDING-DECISIONS-3ARCH-PARITY.md、
                                            # tools/atf-c-compat/probes/p7.c
```

迁移的一个刻意约束是**不代维护者提交**：`rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md`
在提交里的内容必须等于迁移前的内容（迁移只改它的路径），维护者的编辑只存在于工作树。核对：

```bash
git show notes/pre-migrate-20261007:notes/rewrite/fork-syscall-rewrite/TODO-3ARCH-PARITY-20261006.md \
  | diff - <(git show HEAD:rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md) && echo "提交内容一致 ✓"
git diff --numstat -- rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md   # 工作树仍有未提交改动
```

### 第 1 项：内容有没有丢（这是最重要的一项）

迁移前把 notes 树的每个文件都算了 sha256 并记进清单，按合同逐条重算即可证明「零丢失 + 零内容改动」：

```bash
# 1a 三圈账是否闭合：磁盘 = 跟踪 + 忽略 + 未跟踪；映射表与清单行数相等
find rewrite-notes redesign-notes study-notes -type f | wc -l                       # 期望 2428
echo $(( $(wc -l < migrate_notes_plan/pre-migrate-20261007/manifest.notes.disk.tsv) - 1 ))  # 期望 2428
echo $(( $(wc -l < migrate_notes_plan/pre-migrate-20261007/path-map.tsv) - 1 ))            # 期望 2428

# 1b 逐条内容对账：按 path-map 把旧路径的迁前 sha256 与新路径当前 sha256 比对
python3 - <<'PY'
import hashlib
from pathlib import Path
R = Path("/home/xzhao/github/minix-rs")
def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""): h.update(c)
    return h.hexdigest()
pre = {}
for line in (R/"migrate_notes_plan/pre-migrate-20261007/manifest.notes.disk.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    p, size, mt, d, st = line.split("\t"); pre[p] = d
bad = miss = ok = 0
for line in (R/"migrate_notes_plan/pre-migrate-20261007/path-map.tsv").read_text(encoding="utf-8").splitlines()[1:]:
    old, new, kind, rule = line.split("\t")
    f = R / new
    if not f.is_file(): miss += 1; continue
    if sha(f) == pre[old]: ok += 1
    else: bad += 1; print("  不一致:", old, "→", new)
print(f"缺失 {miss}（期望 0）／一致 {ok}／有差异 {bad}（差异须逐条可归因，见上方三段式判据）")
PY
```

判据（三段式，按字面执行才能得出正确结论）：
1. **缺失必须为 0**（映射目标全部在位）。
2. **不一致项必须逐条可归因**——本次迁移在第 3 阶段按裁决改写了必改域的引用、
   第 7 阶段重写了三份入口文档，所以「不一致 0」这个期望本身就是错的（三家审计都指出这一点）。
   正确的期望是：不一致项全部落在 `git-mv` 类跟踪文件里，且每一处都能由
   「合同前缀替换 / 相对链接重定基 / 已登记的入口文档重写」解释；
   三个冻结区（`evidence/`、任何 `.design/`、`archive/legacy-fork-bak/`）必须 0 处被改。
   迁移方留档的实测分布可作对照：2095 全等 + 333 有差异，其中 180 由前缀替换完全解释。
3. **纯移动边界看提交 `e90e4173d`**——那里才是 2428/2428 逐字节全等、零增删行。

### 第 2 项：清单与 tar 本身有没有被事后改动

```bash
sha256sum -c migrate_notes_plan/pre-migrate-20261007/snapshot.sha256   # 从仓库根跑，7 项应全 OK
sha256sum tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz  # 期望 39e68026187bc72f…
tar -tzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz | grep '^notes/' | grep -vc '/$'          # 期望 2428（直接 grep -c 会把 108 个目录条目数进去得 2536）
tar -tzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz | grep '\.design/' | grep -vc '/$'     # 期望 1162（不筛目录得 1180）
tar -tzf tmp/pre-migrate-snapshot-20261007/pre-migrate-notes-full.tar.gz | grep '^\.review/' | grep -vc '/$'     # 期望 1728
git show notes/pre-migrate-20261007 --no-patch --format='%tagger %subject'                     # tag 应存在且指向 bb8a90e05
```

### 第 3 项：改名历史有没有断（`git log --follow` 还能不能往回读）

```bash
git log --oneline --follow -- rewrite-notes/01-stage-kernel/16-smp.md | head -3
git show --shortstat -M e90e4173d | tail -2      # 期望 1102 files changed, 0 insertions(+), 0 deletions(-)
git show --name-status -M e90e4173d | grep -c '^R100'   # 期望 1102（纯移动提交处改名是 R100）
git ls-files rewrite-notes redesign-notes study-notes | wc -l    # 期望 1105 = 迁移前 1102 + 迁移新入库的 3 份入口文档（MIGRATION.md、study-notes/README.md、redesign-notes/vm/README.md）
git ls-files notes/pre-migrate-20261007 2>/dev/null | wc -l >/dev/null
git ls-tree -r --name-only notes/pre-migrate-20261007 -- notes | wc -l   # 期望 1102
```

特别检查一件事：**有没有文件被顺手退出跟踪**。`mv` + `git add` 会让「已跟踪但命中 `.gitignore`」的文件
只从索引里消失（迁移方声称已用 `git add -f` 补回 108 个），核对方式就是上面两条 1102 相等，以及：

```bash
git diff --name-status -M notes/pre-migrate-20261007..HEAD -- notes rewrite-notes redesign-notes study-notes \
  | awk '{print substr($1,1,1)}' | sort | uniq -c
  # 区间期望 1100 R + 5 A + 2 D：两条 D 是内容被授权改写后相似度掉出 git 改名阈值的展示形态
  # （rewrite-notes/README.md 入口重写、coordination/edge3.md 引用改写），
  # 必须逐条验证「目标在位 + git log --follow 可续读 + 移动提交 e90e4173d 处是 R100」，不得当成丢失
```

### 第 4 项：引用有没有漏改

必改域 = 三棵树内部、`os/`、`tools/`、`prompt/`、`.claude/`、`.codex/`、`.trae/`、`CLAUDE.md`、`AGENTS.md`、根 `README.md`。
两个刻意白名单：`rewrite-notes/MIGRATION.md`（它本身就是对照表）、`.claude/settings.local.json`（历史授权命令，按裁决冻结）。

```bash
grep -rnI -E "notes/rewrite|notes/study|notes/redesign" \
  rewrite-notes redesign-notes study-notes os tools prompt .claude .codex .trae CLAUDE.md AGENTS.md README.md \
  --exclude-dir=evidence --exclude-dir=.design --exclude-dir=legacy-fork-bak \
  --exclude-dir=target --exclude-dir=target_smp --exclude-dir=.review --exclude-dir=.git \
  --exclude-dir=minix3 --exclude-dir=.cargo-shared --exclude-dir=.dockercargo --exclude-dir=__pycache__ \
  | grep -vE "^rewrite-notes/MIGRATION.md:|^\.claude/settings\.local\.json:"      # 期望无输出
```

三个树内冻结区必须**保留**旧路径原文（取证日志、设计快照、fork 时代备份，改写等于篡改证据）：

```bash
for d in rewrite-notes/evidence rewrite-notes/archive/legacy-fork-bak; do
  printf "%-38s %s\n" "$d" "$(grep -rlI 'notes/rewrite' $d 2>/dev/null | wc -l)"
done   # 期望与迁前基线一致：evidence 4 个文件、legacy-fork-bak 8 个文件
```

### 第 5 项：链接与锚点有没有被迁移改坏

迁移把文档从四层搬到两层，`../` 的层数随之变化；最危险的形态是**链接文本没变、解析目标变了**。

```bash
# 5a 断链集合对账：基线映射进新坐标后，迁移引入的新断链必须为 0
python3 tmp/pre-migrate-snapshot-20261007/compare-links.py    # 期望「迁移引入的新断链 = 0」
# 5b 锚点基线：行数必须与迁前一致（874 / 6609），且路径列指向真实文件
wc -l tools/anchor-suspect-baseline.txt tools/anchor-unresolved-baseline.txt
awk -F: 'NR>6 && $1 ~ /^[a-z]/ {print $1}' tools/anchor-suspect-baseline.txt | sort -u \
  | while read -r f; do [ -f "$f" ] || echo "MISSING: $f"; done | head    # 期望无输出
```

### 第 6 项：质量门是不是还在工作（防静默失效）

迁移前 `tools/doc-style-lint.sh` 的增量门把 diff 范围钉在 `notes/rewrite` 上；改完目录后这种门**不会报错，
只会什么都不查**。请做正向埋点（做完记得撤销）：

```bash
# 6a 文风增量门：往某个受检文档追加一行含裸日期的文本，跑 --diff 必须抓到，然后恢复
cp rewrite-notes/misc/misc.md /tmp/restore-misc.md
printf '\n埋点 2026-01-01 一处。\n' >> rewrite-notes/misc/misc.md
bash tools/doc-style-lint.sh --diff 2>&1 | tail -3      # 期望看到 SL-4 命中该文件
cp /tmp/restore-misc.md rewrite-notes/misc/misc.md      # 必须撤销

# 6b 断链与坏锚点：临时文件放一份坏链接，跑检查器必须抓到，然后删除
printf '# 探针\n\n[坏链](./no-such-file.md)\n' > rewrite-notes/01-stage-kernel/99-probe.md
python3 tools/notes-link-check.py rewrite-notes/01-stage-kernel --output /tmp/probe.txt >/dev/null
grep -c 99-probe /tmp/probe.txt                          # 期望 ≥1
rm -f rewrite-notes/01-stage-kernel/99-probe.md          # 必须撤销

# 6c 规则三端一致 + 工具在新布局可用
bash tools/check-review-rules.sh; echo "rc=$?"           # 期望 0
bash tools/generate-derived-skills.sh --check | tail -1  # 期望无漂移
bash tools/design-coverage-check.sh 01-stage-kernel >/dev/null; echo "rc=$?"   # 0 或 1 均可，2 = 用法错误
bash tools/design-coverage-check.sh fork-syscall-rewrite 2>&1 | head -2        # 期望给出「已退役」的可读报错
```

### 第 7 项：编译面与代码语义未受影响

```bash
git diff --unified=0 notes/pre-migrate-20261007..HEAD -- os/ \
  | grep -E "^\+" | grep -v "^+++" | grep -vP '^\+\s*(//|/\*|\*|#|\s*$)' | wc -l
# 期望 0 或仅 README/TOML 的列表项行；任何 Rust 代码行出现即为 P0

cd os && cargo check --workspace --tests 2>&1 | tail -3   # 期望零 error
cd os && cargo test --workspace -q 2>&1 | tail -5         # 见下条注意
```

**审阅方会撞上的一个已知现象**：`cargo test --workspace` 里 `minix-driver-rt --lib` 偶发 SIGSEGV
（迁移方第一遍遇到、单跑与第二遍全绿，判定为环境性抖动，与迁移无因果的佐证是
`os/` 只有注释行变化 + 该 crate 测试不读文件系统）。如果你复现到，请用
`cargo test -p minix-driver-rt --lib` 单跑多次确认是否偶发，并把结论写进报告——
这一项是本审计里唯一允许「与迁移无关的既有不稳定」结论的编译门。

### 第 8 项：语义决策是否被正确执行（对照用户裁决）

用户裁决共 12 条，逐条列在 `migrate_notes_plan/EXECUTION-LOG.md` 开头。请核对执行是否走样，
尤其这三条：

1. `.review/`（1728 个未跟踪历史产物）**冻结不改写**，只新增映射文档 →
   验证：`grep -rl 'notes/rewrite' .review/ | wc -l` 应当仍很大（未被 sed），
   且 `.review/archive/notes-fork-syscall-rewrite-2026-09/` 有 3 个文件。
2. 树内三个历史区（`evidence/`、任何 `.design/`、`archive/legacy-fork-bak/`）**内容不改写** →
   第 4 项末段命令。
3. 评审状态目录键由 `{module}` 换成 `{stage}`，而覆盖率脚本的 `--module`（Minix3 模块名）**不变** →
   核对 `prompt/review-rules/review-process.md` 的定义段与 `grep -c "{module}" prompt/` 剩余用法是否都属 Minix3 概念。

### 输出要求

按下面格式交回，每条都要带你实际跑过的命令与输出摘要，不接受只写「已核对」：

```
结论：可交付 / 有阻断问题（P0）/ 有非阻断问题（P1-P2）
P0（内容丢失、改错、历史断裂、门失效）：逐条列，每条带证据与定位
P1（引用漏改、文档与现实不一致）：…
P2（叙述、可读性、建议）：…
未能核对的项与原因：…
```

判定纪律：只要第 1 项（内容一致性与差异可归因性）、第 3 项（跟踪数守恒）、第 4 项（引用残留）、
第 6 项（门有效性）里任何一条不成立，就是 P0。第 7 项的全量测试失败集合经三家审计用迁移前对照实测，
是**先于迁移存在的确定性缺陷**（宿主与容器、并行与 `-j1` 都是同一集合），
除非能证明与 `os/` 的注释改动有关，否则记 P2 观察项，不要写成「偶发抖动」。
审计材料位置：`migrate_notes_plan/pre-migrate-20261007/00-SNAPSHOT.md`（迁移前全貌与三圈账）、
`migrate_notes_plan/EXECUTION-LOG.md`（逐动作与决策）、
`migrate_notes_plan/pre-migrate-20261007/02-VERIFY.md`（门禁取证）、
`rewrite-notes/MIGRATION.md`（旧→新对照）。

## === 复制到此结束 ===
