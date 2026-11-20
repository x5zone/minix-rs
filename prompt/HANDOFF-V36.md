# 会话交接：V3.6 实施 + 轨迹修复 + 对抗循环（暂停于轮 1 完成）

> **暂停原因**：用户要更换模型供应商。本文件是**唯一接续入口**——新会话读本文件即可继续，
> 不需要回溯上文。
> **暂停时点**：2026-11-20 21:27，HEAD = `226cb2f22`。

## 一、目标（用户原话结构）

1. 完整实施 V3.6 版本。
2. 根据 `coordination/review-eval/meta/V3.5-execution-summary.md` 与 review 轨迹，修复流程 bug 与漏洞。
3. 1&2 后执行对抗 review 循环：「循环执行 3~5 次，判定收敛可提前退出（收敛必须再额外执行一轮进行确定）」；
   注意：**根据本 session 经验，循环再多次可能都不会收敛，最终应给我一个可用性报告**。

**并发约束**：另有 2 个会话（修 D01–D06 的 todo、交互式裁决），合计占 4 并发。本会话可用 ≤6 并发。
**提交纪律**：只提交自己的改动。工作树里的 `os/qemu-tests/**`、`os/boot-shim/**`、
`rewrite-notes/01-stage-kernel/**` 改动属并发会话，**一律不 stage**。

## 二、已完成（三个提交）

| 提交 | 内容 | 读数 |
|---|---|---|
| `f8d526b9c` | **V3.6 规则实施**（C-31..C-37 全部八条）：威胁模型三问 / 可判边界 / 减法优先 / 教学概念账 / checklist §2.5 化 / 解除 excellence §17.2 活冲突 / 减法计数 / 触发式 P-SUBTRACT | 四门 + 七自测全绿 |
| `ce95ba0b3` | **V3.5 轨迹驱动门修复**（b1/b3/b3b/b3c/b4/b5）：citation 非 ASCII 豁免卡 / OPEN-DEBT 隐形 / 完成词子串 / edition 超限算 PASS / certify `P0 = 0` 误判 | 六篇 FAIL 数零误伤 |
| `226cb2f22` | **V3.6 对抗轮 1 修复**（9 项）：减法表占位行 / D2 缺失 fail-closed / 销账判据统一 / dup-of 出口 / **回执计数声明契约** / 围栏示例表 / edition last_open 轮号 / 极性 | 六篇零误伤，02–06 最终轮 PASS |

**关键设计决定（轮 1 最重要的一条）**：散文极性（`P0 = 0` / `无 P0` / `P0（0）` / `P0/P1：0 / 2` …）
经过三轮打补丁仍出新反例。**停止改正则，改契约**：V3.6+ 回执必须含
`- 遗留 P0: <n> / P1: <n>`，门只读这两个数，缺失即 fail-closed。已写入
`prompt/review-rules/review-panel.md` §4.7。**这条判例是后续所有「文本门」决策的基准**：
能改形态就不要补词表。

## 三、轮 1 的分诊结果（供轮 2 参考）

gate-adversary 报 12 条 → 主 agent 逐条复现 → 9 条已修、3 条登记 V3.7（无实际形态，F7/F8/F9）。

**尚未回报**：`rules-adversary`（攻击规则文本：内部矛盾 / 不可判定条款 / 孤儿需求 / 死锁逃逸 /
范围泄漏 / 自反性）可能仍在后台。**新会话应先检查 `tmp/r1/rules-adversary.md` 是否存在**：
- 存在 → 读它，把它的 P0 并入轮 2 修复清单；
- 不存在 → 重新派一次（brief 见本文件 §六）。

## 四、下一步（精确到命令）

### 轮 2（确认轮）

```bash
cd /home/xzhao/github/minix-rs
git log --oneline -1                     # 期望 226cb2f22
ls tmp/r1/rules-adversary.md 2>/dev/null # 见 §三
```

1. **若 rules-adversary 有 P0**：先修复 → 跑全门套件 → 提交 → 然后才进确认轮。
2. **确认轮内容**（收敛必须有第二轮）：
   - 派 2 个**新的**独立对抗子 agent（不复用轮 1 的），brief 重点换成轮 1 未覆盖的轴：
     - **轴 A：V3.6 的可执行性**——按新规则文本模拟一次真实 review（给一个假想的新文档 +
       一段真实代码），问「这份规则能不能真的产出一份合规的 D2 报告」。目标是找
       **不可判定条款**（两个 reviewer 可得出相反结论且都「合规」）。
     - **轴 B：存量影响与自我一致性**——V3.6 的判据是否与 `review.md` 核心不变量、
       `review-patterns.md` 模式 24/25/80/85、六命令加载路由冲突；C-37 是否真的自反成立。
   - **收敛判据**（用户口径）：`无 P0`。P1 可修可登记，P0 必须清零。
   - 收敛后再**额外执行一轮**（用户明确要求「收敛必须再额外执行一轮进行确定」）。
3. **最多到轮 5**。每轮：对抗 → 复现 → 修复 → 回归（六篇 FAIL 数零变化 + 四门七自测全绿）→ 提交。

### 可用性报告（最终交付物）

写到 `prompt/V3.6-USABILITY-REPORT.md`，必须包含：
- V3.6 八条判据的**实际可执行性评估**（哪些能机械判、哪些仍靠人）
- 门缺陷修复的实测效果（b1–b5 + 轮 1 九项，各附前后读数）
- **未收敛说明**（若轮 5 仍有 P0）：残留清单 + 为何判定「可用但不收敛」
- 给 V3.7 的候选清单（当前已知：F7 表头别名 / F8 空白首格 / F9 from-R 作用域 /
  p1 确认轮发现量入 metrics / p2 读者档位门 / p3 角色枚举 / p4 refute 处置类 /
  p5 D 号唯一性与引用存在性 / p6 style-lint 对 runs 日期豁免）

## 五、回归基准（必须逐轮复跑，数字必须完全一致）

```bash
git show HEAD:tools/eval-trace-check.py > /tmp/etc_base.py   # 基线仅首次需要
for d in 01-boot-shim-bootstrap 02-higher-half-kernel 03-kmain-cstart \
         04-platform-discovery 05-clock-interrupt-init 06-proc-init-boot-proc; do
  python3 tools/eval-trace-check.py \
    rewrite-notes/coordination/review-eval/runs/01-stage-kernel/$d 2>&1 \
    | tail -2 | grep -o 'FAIL [0-9]*'
done
```

**当前基线（2026-11-20 21:27）**：`105 / 77 / 84 / 16 / 17 / 0`。
⚠️ 注意：这个数会随**并发会话**推进 D01–D06 的 todo 而变化（D01 曾从 100 → 105，
因并发会话新增 I-40/I-42/I-44 真实欠账）。**判定「零误伤」的正确方法是对同一次
HEAD 跑旧/新两版工具对比**，而不是记忆中的数字。

```bash
# 六篇最终轮（应全 PASS，除 D01 R35 的真实欠账）
for d in ...; do  # 同上的循环
  r=$(ls .../$d/certify/ | grep -o "round[0-9]*" | sort -u | tail -1 | sed 's/round/R/')
  python3 tools/eval-trace-check.py .../$d --round ${r:-R1} 2>&1 | tail -1
done
```

**四门 + 七自测**（每次提交前）：
```bash
bash tools/generate-derived-skills.sh && bash tools/check-review-rules.sh \
  && bash tools/lint-review-rules.sh && bash tools/diff-trae-skills.sh --only-diff
python3 tools/eval-trace-check.py --self-test
python3 tools/trait-audit.py --self-test
python3 tools/doc-citation-check.py --self-test
bash tools/doc-symbol-drift.sh --self-test
python3 tools/panel-index.py --self-test
bash tools/lint-review-rules.sh --self-test
```

## 六、子 agent brief 模板（轮 2 用，参数化）

```text
READ-ONLY adversarial review. Do NOT modify repo files. Write only to tmp/r2/<file>.md.
Working dir /home/xzhao/github/minix-rs.

射程：<本轮新增的提交范围，用 git diff <base>..HEAD -- prompt tools>
禁止：cargo / git checkout / reset / stash（并发会话正在改 os/）。git log/status 可用。

轴：<A 可执行性 | B 存量影响与自洽>

HUNT：<按轴给出具体问题清单，要求每条必须附 verbatim 引文 + file:line 或可执行 probe>

输出：每条给 id / 严重度（P0 = 门接受真实缺陷或规则内部矛盾不可执行 / P1 = 会误导
reviewer / P2 = 措辞范围风险）/ 证据 / 最小修法。无发现必须写「本类无发现」。
最多 12 条，按严重度排序。
```

## 七、当前工作树状态（交接时的快照）

- 本会话已提交：`f8d526b9c`、`ce95ba0b3`、`226cb2f22`（均在 `rewrite` 分支）
- 未跟踪且**非本会话**（勿提交）：`prompt/review-loop-r26-r29-report.md`、
  `rewrite-notes/01-stage-kernel/01-design-learn.md`、
  `rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md`
- 已修改且**非本会话**（勿提交）：`os/qemu-tests/**`、`os/boot-shim/**`、
  `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`、`01-todo.md`、
  `04-platform-discovery.md` 及其 `FREEZE.md`
- 本会话的轨迹目录：`tmp/trail-analysis/`（两份子 agent 报告）、`tmp/r1/`（gate-adversary
  报告 + probe 脚本）、`tmp/cit-probe/`（citation 盲区复现）

## 八、已落盘的关键文档（新会话必读）

| 文件 | 内容 |
|---|---|
| `prompt/V35-TRAIL-DEFECTS.md` | V3.5 轨迹分析：6 项门缺陷（已修）+ 6 项流程缺陷（登记 V3.7）+ 被复核为假的一条 |
| `prompt/R1-V36-ADVERSARIAL-2026-11-20.md` | 轮 1 完整分诊表 + 「停止给散文极性打补丁」的设计决定 + 回归读数 |
| `rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md` | V3.6 版本节（8 条判据 + 减法账本 + 同步门读数 + 实施记录 + 轨迹驱动修复节） |
| `prompt/review-rules/review-code-checklist.md` §3.5 | C-31 三问 + C-33 可判边界（执行端唯一落点） |
| `prompt/review-rules/review-panel.md` §4.3 / §4.3b / §4.7 | D2 减法表、P-SUBTRACT brief、回执计数声明 |
