# notes 目录迁移 · MiMo 独立审计报告

> **创建**: 2026-10-07。审计对象为仓库 `/home/xzhao/github/minix-rs`（分支 `rewrite`）的 notes 目录迁移，
> 协议是 `migrate_notes_plan/REVIEW-PROMPT.md`，本报告按协议要求的交回格式给出结论。
> 所有判定都附可复跑的命令与证据文件，证据落在本目录 `evidence/` 下，脚本落在本目录根下。

---

## 一、交回格式的结论

```
结论：可交付（未发现阻断问题）。另有协议与现实不一致 3 项（P1）、
     文档与证据精度问题 6 项（P2），均不构成对迁移本身的否定。

P0（内容丢失、改错、历史断裂、门失效）：无。逐条证伪过程见第三节各项与第四节。

P1（引用漏改、文档与现实不一致）：
  1. 协议第 1 项的判据「一致 2428 / 缺失 0 / 不一致 0」不可达，照抄会误报阻断问题。
  2. 协议第 3 项的两个期望值已过期（三棵树跟踪数、name-status 只剩改名）。
  3. 协议第 2 项的两条 tar 期望值用「数目录条目」的命令去数「文件条目」。

P2（叙述、可读性、建议）：
  1. 门禁取证里内容完整性一项的加法不成立（2428 + 3 才是全部）。
  2. tar 卷被称为「Git 盲区的唯一恢复源」，但清单里 784 件被忽略文件不在卷内。
  3. 「相对链接重定基 138 处」这个数字无法独立复算，逐条清单被第二轮运行覆盖。
  4. 宿主与容器的全量测试失败面比登记的观察宽得多，且现象是确定性的、不是偶发的。
  5. 必改域里仍有 31 处命令形态的旧参数写法，其中 3 处是面向未来的指引句。
  6. 树外清单里两件文件的字节数与清单对不上（工具自身被后续修订）。

未能核对的项与原因：见第十三节，共 3 条。
```

---

## 二、独立性、环境与只读纪律

- 本目录是本次审计唯一的写入点。审计全程没有 `git add`、`git commit`、`git checkout`、
  `git reset`、`git clean`，没有建临时工作树，没有改 `.git` 里的任何引用。
- 审计结束时的仓库状态与开始时一致：`git status --porcelain | grep -v '^??'` 只有维护者在制的一处
  `rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md`，未跟踪件只多了四个审计目录
  （`DS/`、`GLM/`、`Muse/`、`Mimo/`）与既有的维护者文件。证据：`evidence/item0-boundary.txt`。
- 本会话没有打开 `DS/`、`GLM/`、`Muse/` 三方的报告与证据文件（只列出过目录里的文件名，以及计划类文件开头几行的环境声明）。本报告里的每一条环境事实、每一个数字都由本会话自己跑命令得出，判据只来自原始清单、提交、协议和自己的命令输出。
- 环境事实：`grep --version` 实测 GNU grep 3.11（`/usr/bin/grep`、`/bin/grep` 同一份），`python3` 为 3.12.3，
  仓库根可用磁盘 762G，容器镜像 `minix-ci:1.94`（rustc 1.94.1）在本机可用。
  尽管本机 grep 不是那种会对无效 UTF-8 静默吞输出的实现，取证级计数仍一律用 `python3` 字节级重算，
  协议给出的 grep 命令照跑，两套口径互相印证。

---

## 三、逐项审计结果

| 协议条目 | 判定 | 关键数字 | 证据文件 |
|---|---|---|---|
| 第 0 项 边界 | 通过 | 一处在制改动、既有未跟踪件、双标签指向 `bb8a90e05` 与 `964e3e28e` | `evidence/item0-boundary.txt` |
| 第 1 项 内容不丢 | 通过 | 映射目标 2428 全在，0 缺失；内容有变化 333 个文件，全部可由合同解释 | `item1a`、`item1f` |
| 第 2 项 清单与 tar 未被事后改动 | 通过 | `sha256sum -c` 7 项全 OK；tar 卷 4183 文件条目与声明逐项相符 | `item2`、`item2b`、`item2e` |
| 第 3 项 改名历史与跟踪守恒 | 通过 | 纯移动提交 1102 文件 0 增 0 删；跟踪数 1102 → 1105（3 份新增入口文档）；`--follow` 可回溯 | `item3`、`item3b`、`item3c` |
| 第 4 项 引用残留 | 通过 | 必改域残留 0（字节级与 grep 两套口径一致）；冻结区 4 / 8 / 56 与登记一致 | `item4`、`item4-protocol`、`item4d` |
| 第 5 项 链接与锚点 | 通过 | 迁移引入的新断链 0；锚点基线 874 / 6609 行数不变、路径列全部存在 | `item5`、`item5c`、`item5f` |
| 第 6 项 门活性 | 通过 | 三类门都做了正向探针，工作树零改动 | `item6a`、`item6b`、`item6c` |
| 第 7 项 编译与测试 | 通过（带观察项） | `cargo check --workspace --tests` 0 error；`os/` 198 处增删行全是注释；测试现象见第九节 | `item7a`–`item7j` |
| 第 8 项 裁决执行 | 通过 | 抽验的裁决逐条对上 | `item8a`、`item8b`、`item8c`、`item8d` |

---

## 四、内容一致性：为什么 333 个文件内容变了却仍然通过

迁移合同本身就授权改内容：一是把旧路径写法换成新路径，二是相对链接按新目录深度重新定位，
三是给三棵树各写一份入口文档。所以「逐条 sha256 全等」这个判据在本次迁移里本来就不成立，
必须把内容变化拆到行级才能判断有没有夹带私货。

三层检查的实测结果：

1. **集合层**：`path-map.tsv` 2428 行，`manifest.notes.disk.tsv` 2428 行，两边完全一一对应，没有重复目标；
   现在三棵树磁盘上的 2428 个文件里，2425 个是映射目标，3 个是迁移新写的入口文档
   （`rewrite-notes/MIGRATION.md`、`study-notes/README.md`、`redesign-notes/vm/README.md`）；
   另有 3 个映射目标按合同搬进了 `.review/archive/notes-fork-syscall-rewrite-2026-09/`。
   旧的 `notes/` 目录已不存在。证据：`evidence/item1a-content-reconcile.txt`。
2. **字节层**：2095 个文件 sha256 与迁移前完全一致，0 个缺失，333 个内容有变化。
   变化全部落在 `tracked` 类文件里，1325 个被忽略文件与 1 个未跟踪文件一个字节都没动，
   说明取证日志、设计快照、备份稿这些冻结区确实没被写过。证据：`evidence/item1g-frozen-zones.txt`。
3. **行级层**：把「路径映射表替换 + 目录前缀替换 + 相对链接重定基」当作机械变换去还原旧文本，
   1358 处替换完全命中合同；剩下需要人工判读的真实编辑共 149 行、分布在 9 个文件里，
   逐行核对结果是三类，没有任何一类属于夹带：
   - 140 行是两份入口文档的重写（`rewrite-notes/README.md` 133 行、`redesign-notes/README.md` 7 行），
     执行日志 Phase 7 明确登记了这两笔动作与理由；
   - 2 行在 `rewrite-notes/00-master-plan/README.md`，一行把 20 号阶段改成「两篇再设计稿已迁出」，
     一行把上级索引链接改指归档后的名字，执行日志同样登记过；
   - 7 行是纯路径改写，本方还原器没覆盖到具体形态（被冻结的 `.review/...` 路径保持原样、
     `file://` 绝对路径、`archive_bak` 目录规则、裸 `fork-syscall-rewrite` 提法），逐行对照映射表全部吻合。

   证据：`evidence/item1f-real-edits-summary.txt`、`evidence/item1f-real-edits.txt`。

反向的证伪尝试也做了：把 333 个变更文件按目录分类，`evidence/`、`.design/`、
`archive/legacy-fork-bak/` 三个冻结区各 0 命中，`redesign-notes/` 与 `study-notes/` 共 3 个变更
（都在 README 类入口文档上），与登记的授权动作一致。

---

## 五、跟踪守恒与改名历史

- 迁移前标签 `notes/pre-migrate-20261007` 是 annotated 标签，指向 `bb8a90e05`，与全貌记录写的基线一致。
- 纯移动提交 `e90e4173d` 的形态是 `1102 files changed, 0 insertions(+), 0 deletions(-)`，
  也就是说移动那一刻全部文件都是 `R100`，`git log --follow` 对抽查的
  `rewrite-notes/01-stage-kernel/16-smp.md` 一直读回到迁移前的历史。
- 把观察区间拉到「迁移前标签 → 当前 HEAD」，`git diff --name-status -M` 给出 1100 改名、2 删除、5 新增。
  这两条删除是 `notes/rewrite/README.md` 与 `notes/rewrite/fork-syscall-rewrite/edge3.md`，
  两条都能成对对上（前者被入口文档重写、后者被引用重写改动了内容，字节相似度掉出 git 默认重命名阈值），
  内容对读与 `--follow` 都正常：`git log --follow -- rewrite-notes/coordination/edge3.md` 能读回迁移前的提交。
  跟踪数守恒核对：1102 − 2 + 5 = 1105，恰等于当前三棵树的跟踪文件数，旧 `notes/` 路径下跟踪文件为 0。
- 三棵树当前跟踪 1105 = 迁移前 1102 + 迁移新入库 3（`rewrite-notes/MIGRATION.md`、
  `rewrite-notes/README.md`、`study-notes/README.md`，另加 `redesign-notes/vm/README.md`、
  `tools/notes-layout.conf`、`tools/notes-link-check.py` 在树外）。

证据：`evidence/item3-history-conservation.txt`、`evidence/item3b-rename-pairs.txt`、
`evidence/item3c-edge3-readme-pairing.txt`、`evidence/item3d-similarity-lines.txt`。

---

## 六、引用残留与冻结区

- 必改域（三棵树、`os/`、`tools/`、`prompt/`、`.claude/`、`.codex/`、`.trae/`、`CLAUDE.md`、
  `AGENTS.md`、根 `README.md`）里匹配 `notes/rewrite|notes/study|notes/redesign` 的残留：
  字节级扫描 0 文件 0 命中，协议原样的 grep 命令也 0 输出，两套口径一致。
- 刻意保留的两处白名单命中数与登记相符：`rewrite-notes/MIGRATION.md` 21 行、
  `.claude/settings.local.json` 38 行。
- 冻结区旧路径原文与迁前基线逐项相同：`rewrite-notes/evidence/` 4 个文件、
  `rewrite-notes/archive/legacy-fork-bak/` 8 个文件、各阶段 `.design/` 56 个文件（共 1162 份快照）。
- `.review/` 确实没有被改写：含旧路径的文件仍有 1108 个，
  `git diff notes/pre-migrate-20261007..HEAD -- .review` 输出为空，
  迁入的 3 个归档件齐全，新写的 `.review/PATH-MAPPING.md` 已就位。
- 树外 2530 件未跟踪与被忽略文件按清单的字节数与 mtime 逐条对读：2528 件完全未动、0 件消失，
  只有 2 件字节数不同，都是本方工具自身的缓存与脚本（见第十二节第 6 条）。

证据：`evidence/item4-summary.txt`、`evidence/item4-protocol-grep.txt`、`evidence/item4d-frozen-file-counts.txt`、
`evidence/item8d-offtree-untouched.txt`。

---

## 七、链接与锚点

- 断链集合对账：迁移前基线 452 行，去重并映射进新坐标后与现状对比，**迁移引入的新断链 = 0**。
- 再用本方口径独立复核一遍：按「文件:行号 + 链接目标」把现状报告与迁移后清单做集合差，
  三棵树范围内新增 0 条、消失 0 条，两边唯一集合都是 372 条；把范围扩到清单全集，新增同样是 0 条。
  现状扫三棵树共 2133 个 Markdown 文件、373 处断链，与清单里「三棵树 373 处 + 清单外 27 处 = 400 处」对得上。
- 锚点基线：`tools/anchor-suspect-baseline.txt` 874 行、`tools/anchor-unresolved-baseline.txt` 6609 行，
  与迁前行数一致；前者路径列指向的文件全部存在；两份基线相对迁移前标签的差异全部是路径前缀行，
  没有别的内容变化。

证据：`evidence/item5-links-anchors.txt`、`evidence/item5c-baseline-diff.txt`、
`evidence/item5d-broken-now-vs-baseline.txt`、`evidence/item5f-scope-reconcile.txt`。

---

## 八、门的活性（防静默失效）

协议要求证明三件事：文风增量门还能拦住坏行、断链检查器还能抓到坏链、规则三端与工具在新布局下仍然可用。
按只读纪律换成不碰工作树的做法，三件事各自得到正面证据：

1. **文风增量门**：`tools/doc-style-lint.sh --self-test` 通过；把含裸日期的违规文本放在 `/tmp` 下走同一套
   规则引擎，抓到 1 条 `SL-4` 并以 1 退出；用历史提交区间跑 `--diff notes/pre-migrate-20261007..HEAD`，
   输出直接落在 `rewrite-notes/...` 上（命中汇总 9025 处），证明增量门的 pathspec 指向新三棵树而不是空集；
   干净工作树上跑默认区间，摘要行打印「限 rewrite-notes redesign-notes study-notes」，且只统计到维护者在制的那一个文件。
   这条门最危险的失效形态是「不报错、只是什么都不查」，上面这条历史区间探针正是针对它设计的。
2. **断链检查器**：`python3 tools/notes-link-check.py --self-test` 通过；把一个阶段目录复制到 `/tmp` 并放一条
   坏链接，检查器报出 1 处、命中探针文件，仓库内对应目录 `git status` 无任何变化。
3. **规则三端与工具**：`tools/check-review-rules.sh` 退出码 0；
   `tools/generate-derived-skills.sh --check` 报「No drift detected」；
   `tools/design-coverage-check.sh 01-stage-kernel` 退出码 1（发现缺失，属业务结果）；
   `tools/design-coverage-check.sh fork-syscall-rewrite` 退出码 2 并给出「旧布局的 module 参数已退役，见 `rewrite-notes/MIGRATION.md`」的可读说明，而不是静默找不到目录。

证据：`evidence/item6a-lint-probe.txt`、`evidence/item6b-linkcheck-probe.txt`、`evidence/item6c-rules-and-tools.txt`。

---

## 九、编译面与测试

**编译面干净，且可以证明与迁移无因果。**

- `cargo check --workspace --tests` 退出码 0，0 个 error。
- 迁移前标签到当前 HEAD 之间 `os/` 共 90 个文件、+101/−101 行；其中 88 个 `.rs` 文件的 198 处增删行
  逐条判定全部落在注释形态里（`//`、`/*`、`*`、空行），非注释新增行只有 1 行，
  是 `os/libs/minix-types/README.md` 的一条 Markdown 列表项；另外两个非 `.rs` 文件的改动是
  README 列表项与 `os/tests/Cargo.toml` 里的一条注释。
- 没有测试读取被搬动的文档：`os/` 测试代码里与笔记路径相关的唯一命中是一条文档注释。

**测试面比登记的观察宽，而且现象是确定性的。**

- 宿主连跑三遍全量测试（默认并行、`-j1`、再默认一遍），失败集合三次完全一致：
  11 个测试目标失败，其中 `minix-ds` 是断言失败（`test_sys_kernel_hosted_reports_eio` 期望 `EIO` 实得 `Err(38)`），
  `minix-is` 有 2 条断言失败，其余 9 个是测试进程异常退出；通过 4698 条、断言失败 1 条。
- 这 11 个 crate 逐个单跑全部绿（`evidence/item7e-failing-crates-single.txt` 里 11 行 `rc=0`）。
- 按项目权威配方在容器 `minix-ci:1.94` 里重跑，失败目标清单与宿主**完全相同**（同样是那 11 个）。
  说明这不是负载抖动，而是全量测试形态下的确定性行为，与「每遍换一个 crate」的登记描述不符：
  登记里出现过的「换 crate」是 fail-fast 与 `--exclude` 顺序造成的，不是随机性。
- **迁移前对照组**：把迁移前标签的受控内容（排除 notes 树）导出到 `/tmp` 下的独立目录，
  用同一条容器命令再跑一遍，失败目标清单与迁移后**逐条相同**，同样是那 11 个。
  这条对照把「与迁移无关」从推断变成了实测：同一批测试在迁移前的状态下就已经是这个样子。
- 与迁移的因果判断合起来有四条：代码行零改动（上文）、测试不读被搬动的文档、
  11 个 crate 单跑全绿、迁移前对照组结果相同。

证据：`evidence/item7a-os-static.txt`、`evidence/item7b-cargo-check.txt`、
`evidence/item7c-test-workspace-summary.txt`、`evidence/item7e-failing-crates-single.txt`、
`evidence/item7f-rs-comment-only.txt`、`evidence/item7h-j1-control.txt`、
`evidence/item7k-container.txt`、`evidence/item7l-container-pre.txt`。

---

## 十、裁决执行情况

执行日志开头登记了 12 条裁决，逐条抽验结果：

| 裁决 | 核对方式 | 结果 |
|---|---|---|
| 根级三分目录 | 目录列举 | `rewrite-notes/`（20 个阶段目录 + `concepts/`、`misc/`、`coordination/`、`evidence/`、`archive/` 五个功能区）、`redesign-notes/`（`architecture/`、`fork/`、`ipc/`、`vm/`）、`study-notes/` 均在位 |
| `.review/` 冻结不改写 | 含旧路径文件数 + `git diff` + 新增映射文档 | 1108 个文件仍含旧路径、提交内容零改动、`.review/PATH-MAPPING.md` 已就位、迁入 3 个归档件 |
| 清单 + tar，不代用户提交 | 迁移提交链的新增/删除文件全集 | 新增 19 件全部是迁移交付物与工具，删除 2 件都有配对，没有把用户在制内容提交进去 |
| 在制两件随树搬走、内容不改 | 提交内容与迁移前标签逐字节对读 + 磁盘 sha256 对读 | 两件都一致；工作树里维护者的未提交改动仍然只存在于工作树 |
| `study-notes` 原样搬 | 内容对读 + 入口文档 | 184 件内容零变化，`study-notes/README.md` 写明 AI 生成、未与 C 源码核对、不能当事实依据 |
| stage 目录名零修改 | 目录列举 | `00-master-plan` 到 `19-stage-integration` 一个名字都没改 |
| 锚点基线前缀替换 + 行数对账 | 行数 + 与标签的差异 | 874 / 6609 行不变，差异全是路径前缀行 |
| 只改必改域 | 字节级残留扫描 | 残留 0，白名单两处命中数与登记相符 |
| 双标签只在本地 | `git ls-remote --tags origin 'notes/*'` | 无输出，未推远端 |
| 评审状态目录键由 `{module}` 换 `{stage}`、覆盖率脚本 `--module` 不变 | 规则定义段 + 剩余用法清点 | `prompt/review-rules/review-process.md` 的定义段把两个概念写死区分开；`prompt/` 下剩余 65 处 `{module}` 全部落在 Minix3 模块语义上 |

证据：`evidence/item8a-ruling5-inflight.txt`、`evidence/item8b-rulings.txt`、
`evidence/item8c-module-stage.txt`、`evidence/item8d-offtree-untouched.txt`。

---

## 十一、P1 清单（协议与现实不一致）

**第 1 条（P1）　协议第 1 项的判据不可达。**
协议原文要求「按合同逐条重算即可证明零丢失 + 零内容改动」，判据写成「一致 2428 / 缺失 0 / 不一致 0」，
并附了直接比较 sha256 的脚本。实测这条命令会打印 333 条不一致，照字面判据会把一次合规迁移判成阻断问题。
原因在于同一份执行日志里 Phase 3 授权了必改域的引用重写、Phase 7 授权了入口文档重写，
两者都会改变字节。正确的判据应该是「映射目标全部存在 + 每一处内容变化都能被合同解释」，
本报告第四节给出了该判据下的结论。建议把协议第 1 项的期望值与脚本一起改成两段式（集合层 + 行级层）。

**第 2 条（P1）　协议第 3 项的两个期望值已过期。**
其一：`git ls-files rewrite-notes redesign-notes study-notes | wc -l` 的期望值写 1102，实测 1105，
差额是迁移自己新入库的 3 份入口文档，属于裁决范围内的正常新增。
其二：`git diff --name-status -M ... | grep 只有 R 没有 D` 的期望不成立，实测 1100 R + 2 D + 5 A。
两条删除的成因在第五节：内容被授权改写后字节相似度掉出 git 默认重命名阈值，
跟踪数守恒与 `--follow` 回溯都不受影响。协议若不改，审阅方会在这里误报阻断问题。

**第 3 条（P1）　协议第 2 项的两条 tar 期望值口径错位。**
`tar -tzf ... | grep -c '^notes/'` 期望 2428、实测 2536，差的 108 是目录条目；
`grep -c '\.design/'` 期望 1162、实测 1200，差的 38 也是目录条目。按文件条目计分别是 2428 与 1162，
与清单、与执行日志的声明完全一致。命令应改成只数文件条目，或者把期望值改成含目录的口径。

---

## 十二、P2 清单

**第 1 条（P2）　门禁取证里内容完整性一项的加法不成立。**
原文写「新树 + 归档 = 基线 = 映射表 = 2428」，但新树 2428 加归档 3 是 2431。
正确关系是：新树 2428 = 映射目标落在树内的 2425 + 迁移新增 3；
映射目标 2428 = 2425 + 搬进归档的 3。数字本身都对，等式写错了。

**第 2 条（P2）　tar 卷的自我描述过强。**
第八节标题写「Git 盲区的唯一恢复源」、正文用「全量」，但 `manifest.offtree.tsv` 的 2530 件里
有 784 件不在卷内：`tmp/` 409、`tools/` 308、`book/` 43、`os/` 22、`.claude/settings.local.json`、`.qoderignore`，
全部是被忽略的文件（15 件未跟踪文件全在卷内，逐条核过）。
该节的覆盖清单本身把范围一项项写清楚了，所以这是措辞与覆盖范围不一致，不是漏做。
其中真正不可再生的只有 `.claude/settings.local.json` 与 `tools/vendor/` 下的头文件，建议在措辞上收窄。

**第 3 条（P2）　「相对链接重定基 138 处」无法独立复算。**
逐条清单 `tmp/pre-migrate-snapshot-20261007/relink-report.tsv` 被第二轮运行覆盖，
现在只剩 20 条 `REWRITE` 与 503 条 `DANGLING`，加不出 138。
断链对账本身不受影响（新断链 0 由两套口径各自独立证明），但这个数字没有可复跑的底账。
建议把两轮清单分别存档，或在执行日志里写明第二轮覆盖了第一轮。

**第 4 条（P2）　宿主与容器的全量测试失败面比登记的观察宽。**
门禁取证登记的是「每遍一个 crate 崩溃、且每遍不是同一个 crate、判定为负载相关」；
实测是固定 11 个目标失败（默认并行与 `-j1`、宿主与容器四次运行集合一致），其中 3 条是断言失败而非崩溃。
登记里「换 crate」的直接原因是 fail-fast 与 `--exclude`，不是随机性。
这一项不改变迁移的判定（代码零改动、单跑全绿），但会影响后来人对仓库健康度的判断，
也会影响审阅协议里那条「只允许一个 crate 偶发崩溃」的豁免条款。

**第 5 条（P2）　必改域里仍有 31 处命令形态的旧参数写法。**
`tools/design-coverage-check.sh fork-syscall-rewrite ...` 这类命令在必改域里还有 31 处命中
（已排除归档目录、设计快照与对照表本身）。多数是历史台账里对既往验收的回溯记录，照裁决口径不算漏改；
但其中 3 处是面向未来的指引句，照抄会拿到「已退役」报错：`rewrite-notes/09-stage-init/plan.md` 的工作流说明、
`prompt/todo_plan.md` 的示例命令、`rewrite-notes/10-stage-mib/todo.md` 的命令片段。
建议要么就地改成新参数，要么在 `rewrite-notes/MIGRATION.md` 的已知悬空引用一节补一条登记。

**第 6 条（P2）　树外清单里两件文件的字节数对不上。**
`tools/__pycache__/verify-check.cpython-312.pyc`（缓存再生成）与 `tools/notes-link-check.py`
（8135 → 7950 字节，工具在清单生成之后被继续修订过）与 `manifest.offtree.tsv` 记录的字节数不同，
mtime 未变。清单是迁移开窗时的快照，被审对象是迁移本身，所以这不影响判定，
但它说明清单对「执行方自己在开窗后修订的文件」有天然的滞后，措辞上值得注明。

---

## 十三、未能核对的项

1. **协议第 6 项的原文埋点没有按原样执行。**
   为守只读纪律，改成了三类不碰工作树的等价正向探针（第八节）。
   能证明的是「规则引擎活着、pathspec 指向新树、检查器能抓坏链接」；
   不能证明的是「在工作树里就地追加一行再撤销」这条动作链本身。
2. **冻结域内的旧路径没有逐行与迁移前基线对账。**
   `.review/`、`migrate_notes_plan/`、`tmp/`、`AI-chats/`、`new_laptop_migrate/` 这些冻结域
   只做了抽查（含旧路径的文件数、`git diff` 为空、清单字节数对读），
   没有把每个文件的每一行与迁移前内容逐行比对。范围依据是：这些文件不在迁移提交链的改动集里。
3. **树外被忽略文件只能核对存在性与字节数，核不了内容。**
   `manifest.offtree.tsv` 只有路径、字节数、mtime、git 状态四列，没有 sha256 列；
   要核内容只能取 tar 卷，而 784 件不在卷内（第十二节第 2 条）。

---

## 十四、复跑方式

```bash
# 内容一致性三层检查
python3 migrate_notes_plan/Mimo/audit_1_content.py
python3 migrate_notes_plan/Mimo/audit_1f_real_edits.py   # 依赖 /tmp/opencode/changed-list.tsv，由 audit_1_content.py 的口径生成

# 必改域残留（字节级）
python3 migrate_notes_plan/Mimo/audit_4_refs.py

# 协议原文命令
sha256sum -c migrate_notes_plan/pre-migrate-20261007/snapshot.sha256
python3 tmp/pre-migrate-snapshot-20261007/compare-links.py
bash tools/doc-style-lint.sh --diff notes/pre-migrate-20261007..HEAD
bash tools/check-review-rules.sh

# 三类门的正向探针（均不改工作树）
bash tools/doc-style-lint.sh --self-test
bash tools/doc-style-lint.sh /tmp/opencode/probe-violation.md
python3 tools/notes-link-check.py --self-test

# 编译与测试
cd os && cargo check --workspace --tests
cd os && cargo test --workspace --quiet --no-fail-fast
cargo test -q -p minix-ds --lib          # 11 个失败目标逐个单跑的写法
```

复跑任何一条都不需要写权限；唯一会写文件的是 `--output` 参数与构建缓存目录。
