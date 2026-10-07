# notes 目录迁移 · GLM 独立审计逐项结果

> **审计方**: GLM（ZCode 会话），2026-10-07 19:03 开始。
> **协议**: `migrate_notes_plan/REVIEW-PROMPT.md` 第 0～8 项 + 审计计划 `00-AUDIT-PLAN.md` 第 4 节的自加检查项 A1～A6。
> **方法声明**: 本文档每一条判定都附带实际执行过的命令与实测输出（原始输出在 `evidence/` 对应文件里），
> 没有一条判定来自对执行方叙述的信任。期望值本身也被独立检验，不作为已知答案使用。
> **独立性**: 全程未读取 `migrate_notes_plan/DS/`（DeepSeek 审计方产物）任何内容。

## 工具偏差声明（影响取证方式）

`/usr/bin/grep` 实际是 ugrep 7.8.4（`grep --version` 实证，见 `evidence/item0-boundary.txt` 前的环境记录）。
该实现对无效 UTF-8 文件存在静默吞输出行为，因此本审计的取证级计数一律用 python3 字节级重算
（脚本：`GLM/audit_item1.py`、`GLM/audit_item4_residue.py`、`GLM/audit_item8_decisions.py` 等），
grep 只作快速参考。编译验证走项目铁律配方 docker `minix-ci:1.94`（`-m 2g -j 1`）。

---

## 第 0 项 · 边界（在制文件与工作树）

| 判据（协议原文） | 实测 | 判定 |
|---|---|---|
| 非 untracked 改动只有 TODO-3ARCH 一处 M | `git status --porcelain` 排除 `??` 后仅 ` M rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md` | ✓ |
| untracked = 4 件 + 审计目录 | agents-workflow-optim.md、new_laptop_migrate/、PENDING-DECISIONS、p7.c，另加 DS/ 与 GLM/（两审计方自身产物，预期内） | ✓ |
| 迁移只改该文件路径不改内容 | `git show notes/pre-migrate-20261007:notes/…/TODO-3ARCH…` 与 `git show HEAD:rewrite-notes/…/TODO-3ARCH…` diff 为空 | ✓ |
| 工作树仍有维护者未提交改动 | `git diff --numstat` = 6 增 2 删 | ✓ |
| 工作树文件 sha256 与 manifest 记录一致 | TODO=`9b3152dd…`、PENDING=`4cdde71c…`，与 `00-SNAPSHOT.md` §3 逐字一致 | ✓ |
| 索引退回 blob（日志动作 2.4/7.3 声称 beb452f4e） | `git rev-parse HEAD:…TODO-3ARCH…` = `beb452f4e…` | ✓ |

证据：`evidence/item0-boundary.txt`。

## 第 1 项 · 内容对账（最重要的一项）

**三圈账与合同闭合**（`evidence/item1-content.txt`，脚本 `GLM/audit_item1.py`）：

- manifest 行数 2428 = path-map 行数 2428 = 三棵树磁盘文件数 2428（2425 个映射目标 + 3 个迁移后新建件）。
- manifest 路径集合与 path-map 旧路径集合**双向差为 0**（协议只对行数，本审计加强为集合相等）。
- manifest 的 git_status 分布 tracked 1102 / ignored 1325 / untracked 1，与 00-SNAPSHOT §4 一致。
- kind 分布 git-mv 1102 / mv-ignored 1325 / mv-untracked 1；rule 分布 R1=2 R3=3 R4=276 R5=1814 R6=1 R7=74 R8=6 R9=40 R10=1 R11=1 R12=12 R13=1 R15=11 R17=184 R18=1 R19=1；新树分布 rewrite-notes 2227 / study-notes 184 / redesign-notes 14 / .review 3 —— **与执行日志动作 1.2 声称值逐项相等**（自加检查 A6 的抽样验证点）。

**逐条内容对账与 333 个差异的归因**（`evidence/item1c-attribution.txt`，脚本 `GLM/audit_item1c.py`）：

协议第 1b 项的期望「一致 2428 / 0 / 0」在 HEAD 上**不可能达成**——Phase 3 引用重写本就会修改内容
（执行日志动作 3.2：413 个文件 1508 处替换）。实测：与迁移前 sha256 不一致的文件 **333 个**，
归因方法与结果：

1. 333 = `git diff --name-only e90e4173d..HEAD` 三树内容变更 336 减 3 个新建件（MIGRATION.md、
   study-notes/README.md、redesign-notes/vm/README.md），集合级闭合（表面差 8 个中文名文件为
   git `core.quotePath` 引号伪差，用 `git ls-files -z` 字节级排除）。
2. 每个差异文件在纯移动提交 e90e4173d 时的 blob 哈希与 manifest 记录一致（移动零内容改动）。
3. e90e→HEAD 的行级差异用「path-map 派生的合同变换」（2428 条文件级 + 104 条由 path-map 结构推导的
   目录级 + 3 条树根兜底 + 1 条剥层规则；推导过程独立于执行方脚本）逐行解释：
   **321/333 可被合同完全解释**；剩余 12 个文件的全部差异落进执行日志记载过的动作：
   - redesign-notes/README.md（Phase 7 顶部状态段 7 行 + 动作 3.3 示例表第 3 行的链接重定基）
   - rewrite-notes/README.md（Phase 7「重写为新入口」，96→69 行；其新内容里的 8 个计数断言
     经磁盘实测逐一吻合，`evidence/item1e-readme-counts.txt`）
   - 00-master-plan/README.md L38（动作 7.1 记载的一行启动顺序表修正）与 L146（动作 3.3 记载的
     `../README.md → ../misc/legacy-fork-syscall-index.md` 重定目标）
   - 12/14-stage-input-runtime/todo.md 与 coordination/edge1/2/3/new_edge1/2/3.md
     （目标文件搬进 coordination 后的相对链接重定基，目标全部实存，`evidence/item1d-retargets.txt`）
   - concepts/endpoint.md（`endpoint_redesign.md` 按 path-map R15 行重定基到 `redesign-notes/ipc/`）
4. 行数变更文件只有 2 个（两个 README 重写），与「不增删行」约束的适用范围（机械替换文件）不冲突。
5. 冻结区（`.design/`、`evidence/`、`archive/legacy-fork-bak/`）内容变更文件数 = **0**。

**跟踪面对账**：跟踪数 1105 = 1102（映射的 git-mv 全数在跟踪面）+ 3 新建件；映射目标不在跟踪面的
1326 个全部是 mv-ignored（1325）+ mv-untracked（1），git-mv 缺失数 **0** —— Phase 2 修复的
108 个静默退跟踪在终态没有残留。

**第 1 项判定：通过**（333 个差异全部归因到记载动作；零丢失、移动零改动、冻结区零改动成立）。

## 第 2 项 · 清单与 tar 防篡改

- `sha256sum -c snapshot.sha256`：7/7 全 OK（`evidence/item2-tamper.txt`）——manifest×2、
  refs 计数表、断链基线、引用底账、git-state、tar 卷全部未被事后改动。
- tar 卷 sha256 = `39e68026…`（与 00-SNAPSHOT §8 与 snapshot.sha256 一致）。
- tar 条目分解（`evidence/item2-tar-split.txt`）：文件条目 4183（= 00-SNAPSHOT 声称值），其中
  notes/ 2428、.review/ 1728、.design/ 1162；目录条目 583。协议第 2 项给的 `grep -c '^notes/'`
  命令会连目录条目一起计数（实测 2536），是协议文档的口径缺陷，不是快照缺陷（P2-3）。
- **tar↔manifest 集合级对账**（自加 A3，`evidence/item2b-tar-content.txt`，脚本 `GLM/audit_a3_tar.py`）：
  notes/ 文件条目集合与 manifest 路径集合双向差 0；抽样 38 件内容哈希与 manifest 一致
  （含 615MB 的 serial_c9a.log、在制两件、.design/.log/.bak/.backup/内嵌 .review 每类代表）。
- 双 tag：pre → bb8a90e05、post → 964e3e28e，与日志收口表及 tag 重打记录一致。

**第 2 项判定：通过**。发现 1 处留档笔误：`00-SNAPSHOT.md` §5.2 记录的 p7.c sha256 前 12 位
`b8da7f7b726` 与实测 `b8da7f7b7276…` 不符（第 11 位掉字；tar 与磁盘三方一致证明内容无恙，P2-1）。

## 第 3 项 · 改名历史与跟踪数守恒

- `git log --follow -- rewrite-notes/01-stage-kernel/16-smp.md` 穿透迁移提交读到迁移前历史 ✓。
- 纯移动提交 e90e4173d 的权威口径 `git diff-tree -M -r --name-status`：**1102 × R100，0 A，0 D** ✓。
  协议给的 `git show --stat -M | grep -c '=>'` 量到 177 是 `--stat` 的显示行为，不是改名数。
- 跟踪数守恒：pre tag notes/ 跟踪 1102 = 三棵树现跟踪 1105 − 3 个迁移后新建件 ✓。
- `pre..HEAD` 的 name-status = 1100 R + 5 A + 2 D（协议期望「只有 R」）。2 个 D
  （`notes/rewrite/README.md`、`notes/rewrite/fork-syscall-rewrite/edge3.md`）经查证
  **不是内容丢失**：它们的旧路径 blob 与对应新路径在 e90e4173d 的 blob 逐一相同
  （`evidence/item3b-d-pairs.txt`）——纯移动提交里它们是 R100，累积 diff 里因文件数超过
  git 非精确改名检测上限（默认 1000）而落到 D+A 形态（README 重写 96→69 行、edge3.md
  118 行全量链接改写）。5 个 A = 3 个新建件 + 上述 2 个 D 的新侧。

**第 3 项判定：通过**（协议期望值「1102」与「只有 R」是按 Phase 2 时点写的，未计入 Phase 7 新建件
与改名检测极限，P2-4 记档）。

## 第 4 项 · 引用残留（ugrep 陷阱 → python3 字节级）

`GLM/audit_item4_residue.py` 字节级扫描必改域（三棵树、os、tools、prompt、.claude、.codex、.trae、
CLAUDE.md、AGENTS.md、README.md，排除冻结目录）：

- 总命中 61 处，全部落在两个白名单文件：`.claude/settings.local.json` 40 处（留档时 38 行，
  差 2 为其后新授权命令的追加，符合「新命令会重新走授权」的冻结语义）、`rewrite-notes/MIGRATION.md`
  21 处（对照表本体）。**白名单外残留 = 0**。
- 冻结区保留计数实测 evidence=4 / legacy-fork-bak=8 / .design=56，与执行日志门 M2 声称值
  **逐个精确相等**——历史原文未被改写。

**第 4 项判定：通过**（这条是协议判定纪律里的一票否决项）。

## 第 5 项 · 链接与锚点

- 门 M3 复跑（协议脚本 compare-links.py，先读其源码确认逻辑）：基线 452 行 → 映射去重 288 条 →
  迁移后 257 条，**迁移引入的新断链 = 0**，消失 31 条。与执行日志/02-VERIFY 数字逐字一致。
- **独立新跑**：`python3 tools/notes-link-check.py rewrite-notes redesign-notes study-notes` 当此扫描
  2133 个 md，断链 373 处，与 `broken-links.after.txt` 三树部分**集合级逐行相等**
  （`evidence/item5b-fresh-links.txt`）——Phase 4/7 之后的改动没有引入新断链；抽样 3 条人工核实
  均为迁移前既有类别（旧阶段编号、已消失目录）。
- 锚点基线：874 / 6609 行不变 ✓；旧路径残留 0 ✓（python3 字节级）；suspect 基线引用的 101 个
  文档路径逐条 `test -f` 缺失 0 ✓。

**第 5 项判定：通过**。

## 第 6 项 · 质量门正向埋点（做完即复原）

| 埋点 | 实测 | 判定 |
|---|---|---|
| 文风增量门 | 往 `rewrite-notes/misc/misc.md` 追加「审计埋点 2026-01-01 Gate X」行，`doc-style-lint.sh --diff` 范围行打印「限 rewrite-notes redesign-notes study-notes」并命中 SL-4、SL-7 各一条；恢复后 sha256 与埋点前相同，`git status` 无该文件 | ✓ 静默失效已修复且真实生效 |
| 断链检查器 | 探针文档 `rewrite-notes/01-stage-kernel/99-glmprobe.md` 放坏链，检查器命中 1 条；探针已删 | ✓ |
| 锚点门 | 同一探针写 `os/kernel/src/lib.rs:fn glm_probe_symbol_that_does_not_exist`，`anchor-resolve.sh --check` 报 ZERO-DEF；不经管道重测 rc=1（本审计第一遍也踩了「管道量到 tail 状态」的坑，与执行日志 Phase 6 记载的教训一致）；探针已删 | ✓ |
| 规则三端 | `check-review-rules.sh` consistent rc=0；`generate-derived-skills.sh --check` No drift | ✓ |
| 覆盖率工具 | 新布局 `01-stage-kernel` rc=1（业务结果）、树别名 `rewrite` rc=1；旧布局参数 `fork-syscall-rewrite` 给出「已退役 + MIGRATION.md 位置」可读报错 rc=2 | ✓ |

埋点前后 `git status --porcelain` 逐行比对一致（`evidence/item6-probes.txt`、`item6b-gates.txt`）——
**工作树零残留**。

**第 6 项判定：通过**（一票否决项）。

## 第 7 项 · 编译面与测试

**文本层**（`evidence/item7-os-diff.txt`）：`pre..HEAD` 的 os/ 域改动 = 89 个 `.rs`（非注释新增行
**全部为 0**，python3 行级独立复核）+ `os/tests/Cargo.toml` 1 行注释改写 + `minix-types/README.md`
1 行列表项。语义编译面在源码层零改动。

**docker 权威配方**（`minix-ci:1.94`，`-m 2g -j 1`，CARGO_HOME=树内 .dockercargo）：

- `cargo check --workspace --tests -j 1`：**RC=0，零 error**（3m33s；warning 均为死代码类）。
- `cargo test --workspace -q -j 1`：进行到 `minix-driver-rt --lib` 时测试进程 SIGSEGV（signal 11），
  RC=101 —— **执行方在宿主上记录的现象在权威配方下复现**。
- `cargo test -p minix-driver-rt --lib` 单跑 ×3（docker）：14/14 全过 ×3。
- `cargo test --workspace -q -j 1 --exclude minix-driver-rt`：进行到 `minix-ds --lib` 时
  112 过 1 败，RC=101 —— 与执行方宿主第 3 遍（同为 minix-ds、同为 112+1）同构。
- **pre-tag 对照**（自加）：`git archive notes/pre-migrate-20261007 os` 导出到 /tmp（不触碰仓库），
  docker 同配方 `cargo test -p minix-driver-rt --lib` 单跑 ×3：14/14 全过 ×3 —— 单跑稳定性在
  迁移前后一致。
- **pre-tag 全量对照**（自加，决定性证据）：pre-tag 源码同配方 `--no-fail-fast` 全量 test 呈现
  与 HEAD **逐一相同的失败签名**（同一测试 `test_sys_kernel_hosted_reports_eio`、同一断言
  `Err(38)` vs `Err(5)`、同一文件 `servers/ds/src/server.rs:1002`；driver-rt 与 minix_init 同样
  SIGSEGV 且二进制哈希不同——崩溃跟随源码构造而非构建产物；minix-is 同样两个 hosted 测试失败）。
  证据：`evidence/item7d-pretag-fulltest.txt`。

判读：失败对象在 crate 间漂移（driver-rt SIGSEGV / ds 1 例败）、单跑稳定、os/ 语义等价
（纯注释差异），且 **pre-tag 源码在同一权威配方下逐一复现全部失败** ——
执行方「既有不稳定、与迁移无因果」的判定由推断升级为直接实证。

**第 7 项判定：通过**（check 门全绿；test 波动判为既有不稳定，证据见 `evidence/item7-*.txt` 与
docker 运行日志）。

## 第 8 项 · 12 条用户裁决执行对读

| # | 裁决 | 实测 | 判定 |
|---|---|---|---|
| 1 | 根级三目录 | 三树 + book/ 未动 | ✓ |
| 2 | .review/ 冻结 + PATH-MAPPING.md | **1728 件 tar↔磁盘逐字节相同**（不同 0 / 缺失 0）；1108 件仍含旧路径原文；archive 区 3 件；PATH-MAPPING.md 存在 | ✓（强验证） |
| 3 | 清单+tar 保全，不代提交 | 15 件迁移前未跟踪件全部仍未被跟踪（3 件点名 + new_laptop_migrate 14 件） | ✓ |
| 4 | 分阶段交互确认 | 12 裁决表 + J1/J2/J3 问答 + 裁决 12 补问记录；交互过程本身无法取证，凭材料自证 | ✓（过程性） |
| 5 | 在制两件随树搬走不改内容 | 第 0 项已证（提交态一致 + 工作树改动完好 + 索引退回 beb452f4e） | ✓ |
| 6 | study-notes 原样搬 + README 定性 | README 含「冻结/AI 生成/不当依据」定性段；study 文件在 2428 对账内 | ✓ |
| 7 | rewrite-notes 内部 deepseek/glm 风格 | concepts/misc/coordination/evidence/archive 五区实存，stage 目录名零修改（path-map R5 规则集） | ✓ |
| 8 | 直接在 rewrite 分支逐阶段 commit | 9 个提交全部在 rewrite | ✓ |
| 9 | 本地双 tag 不推远端 | 双 tag 本地存在；远端状态 UNVERIFIED（本会话 ls-remote 无网络输出，连 9 个上游 tag 也未列出，属网络凭证限制） | ✓/UNVERIFIED |
| 10 | redesign-notes 按主题子目录 | architecture/ ipc/ fork/ vm/ 实存 + 根 README | ✓ |
| 11 | 锚点基线前缀替换 + 行数对账 | 874/6609 行不变、残留 0、路径实存（第 5 项） | ✓ |
| 12 | 只改必改域 | 第 4 项白名单外残留 0；冻结域计数与基线一致（.qoder 6 / AI-chats 10 / new_laptop 71 逐个相等；tmp 增长来自迁移自身按要求写入 tmp 的工作产物）；.qoder/specs 与 new_laptop_migrate 15 件 tar↔磁盘哈希相同 | ✓ |
| — | 协议第 8 项点名的 {stage}/{module} 分离 | `prompt/review-rules/review-process.md:403` 定义段明确两个概念不得混用；prompt/ 残余 65 处 `{module}` 抽样全是 Minix3 模块概念（`os/servers/{module}/`、`coverage-extract.py {module}` 等） | ✓ |

证据：`evidence/item8-decisions.txt`、`item8b-frozen-recount.txt`、`item8c-module-stage.txt`。

## 自加检查项 A1～A6 结果

- **A1 post-tag 后两提交**：`964e3e28e` 纯新增 `tools/notes-layout.conf`（25 行配置）；
  `77be8773a` 仅追加 EXECUTION-LOG.md 40 行。无夹带（`evidence/item9-aux.txt`）。
- **A2 合同 sha**：path-map.tsv 实测 sha256 与执行日志动作 1.3 记录值逐字相同——合同在执行期间未被改动。
- **A3 tar↔manifest 互证**：第 2 项已完成（集合相等 + 38 件抽样 + 5 件冻结未跟踪件 tar==磁盘）。
- **A4 裁决全量对读**：第 8 项表（协议只点名 3 条，本审计 12 条全查）。
- **A5 交付物完备性**：用户原始要求的四件产物（tag / 未跟踪文件表格 / 执行日志 / 审计 PROMPT）
  全部存在且已入库；另有 01-PATH-MAP.md、02-VERIFY.md、broken-links.after/after.txt、MIGRATION.md、
  .review/PATH-MAPPING.md。00-SNAPSHOT §5 的未跟踪文件表格覆盖全仓 15 件并逐一给出处置去向。
- **A6 日志数字抽查**：kind/rule/新树分布、冻结区 4/8/56、M3 的 288→257、锚点 874/6609、
  bare-mentions 73 行、path-map sha256 —— 实测值与日志声称**全部一致**。
  两处未能精确复核：①「1508 处替换 / 413 文件」与「138 处重定基」的逐处计数（relink-report.tsv
  只留了第二轮的 20 条 REWRITE 记录，首轮 118 条未留档），但两者的**效果**已被残留扫描与
  断链集合对账覆盖；②settings.local.json 冻结（未入 tar 无基线哈希，仅能验证其后追加语义与
  未被迁移提交触碰——该文件未被 git 跟踪，`git log pre..HEAD -- <path>` 为空）。

## 审计过程中的自查更正（诚实记录）

1. 第一版 item1 脚本误报「9 个 git-mv 目标不在跟踪面」——`git ls-files` 默认 quotePath 把
   非 ASCII 路径加引号转义导致集合比对伪差，改用 `-z` 后澄清（`evidence/item1-content.txt` 保留
   了误报原文与 item1c 的澄清输出）。
2. 第一版合同变换只含文件级映射，漏目录前缀与剥层规则，333 个差异中 161 个落人工判读；
   补齐推导规则后收敛到 12 个（全部是记载动作），过程两次迭代见 evidence 文件的历史版本。
3. 锚点埋点第一遍经管道量 rc 量到 tail 的 0，重测为 1——与执行日志自述踩过同一个坑。
