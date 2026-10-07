# 全量 REVIEW 干跑轨迹（第二轮会话）：PM-10 / RS-11 / SCHED-14

> **创建**: 2026-10-08
> **关系**: 本文件是 `prompt/REVIEW-TRACE-vfs-08-dryrun.md`（claude 运行时，vfs-08）的**续作**，靶子换成
> PM / RS / SCHED 三个 stage 的三篇文档，运行时换成 **zcode**。两文件编号各自独立：本文件的
> `D#`（流程缺陷）与 `F#`（文档/代码缺陷）不与 vfs-08 文件的 D1–D13 共用序号；交叉引用会显式写明。
> **用途**: ① 检验 `prompt/` 的 REVIEW 流程在**第二个运行时**与**另外三个 stage** 上是否仍可执行、可判定、
> 无自相矛盾；② 产出可复跑取证，供独立审计方复核流程本身；③ 用轨迹反向驱动流程修复。
> **取证位置**: 各轮的原始产物在 `.review/zcode/{stage}/`（scan.md 的各 gate-evidence 块、VERIFY-CHECK.md、
> STATE.md、锚点底账）；该目录不入版本库，本文件只留结论与命令，命令可在当前仓库直接重放。

## 〇、三轮的靶子

| 轮次 | 阶段 | 文档 | 行数 | Rust 侧 | 选它的理由 |
|---|---|---|---|---|---|
| Round 1 | `04-stage-pm` | `10-pm-wait.md` | 352 | `os/servers/pm/src/{wait.rs,exit.rs,mproc/*}` | 用户点名"未人工审阅"的 stage；wait 族横跨 C 语义、状态机、wire 载荷、测试对账四个维度 |
| Round 2 | `03-stage-rs` | `11-rs-publish.md` | 172 | `os/servers/rs/src/*` | 用户点名 stage；短文档，检验"小文档是否被过度流程化" |
| Round 3 | `06-stage-sched` | `14-rs-interaction.md` | 217 | `os/servers/sched/src/*` | 用户点名 stage；跨服务交互面，检验跨文档检查 |

---

# Round 1：`04-stage-pm/10-pm-wait.md`

## 一、按 Step 顺序的执行记录（命令 → 结果 → 判定）

| Step | 命令 | 关键输出 | 判定 |
|---|---|---|---|
| 0 状态初始化 | `bash tools/review-init.sh zcode rewrite-notes/04-stage-pm/10-pm-wait.md` | `❌ tool 必须是 trae、claude 或 codex，得到: zcode` | ⛔ **D1：规范登记的运行时被自己的工具拒绝** |
| 0 预检（Gate H 前置） | 4 条 `ls .design/10-*.v*.md` + `bash tools/design-coverage-check.sh 04-stage-pm` | v1 三快照齐在；`ALL DOCS COMPLETE`（29 篇） | ✅ 无需 Step 0.3 嵌入生成 |
| 0 关联代码清单 | `bash tools/doc-code-map.sh <doc> --check` | 2 文件存在；缺头部 `Rust 实现` 字段 | ⚠️ F-P1（已补） |
| 0.3 v2 快照 | 手工独立推导（C 源 + 当前代码） | `10-{outline,outline-review,design}.v2.md` + design-structure 脚手架 | ✅（成本观察见 §三） |
| 0.5 structure + 12 节 | 人工骨架评审 | §11 复述测试卡在"表序优先"→ 教学类 P1（已修 §2.3） | ✅ 门有效 |
| 1.0 锚点 | `bash tools/anchor-resolve.sh --check <doc>` | 首跑 `anchors=38 resolved=38 zero-def=0`——**全绿但 6 处符号与句意不符** | ⛔ **D6：C 侧句意零覆盖** |
| 1.5 覆盖率 | `python3 tools/coverage-extract/coverage-extract.py pm … --doc-file <仓内路径>` | 首跑 `Doc covered 0 (0.0%)` + **空符号表**；且 Rust 侧灌入 87939 个 `.dockercargo` 第三方符号 | ⛔ **D2/D3：静默失配 + 扫描污染** |
| 1.6 设计对齐 | design v2 九项矩阵 | 7 ✅ / 2 ❌ = **77.8% < 80%** → DESIGN_DIVERGED → code Refactor | ✅ 门有效（正是它逼出两个代码 P0） |
| 2/3/3.5 语义与精度 | 逐条回 C 源（`sed`/`grep` 实测行号） | 行号族（do_wait4/tell_parent/tell_tracer/cleanup/wait_test）**全对**；宏与结构体引用**成片错**（`wait.h:10/32/63`、`utility.c:92`、`ipc.h:445`、`_NSIG 32`） | ✅ 无虚构 |
| 3.6 代码维度 | 逐文件读 + `grep -c panic`/`WaitOutcome` 引用 | 2 处 P0 代码缺陷（`wait_test` 占位、三环三趟遍历）+ 1 处死代码 | ✅ 门有效 |
| 4.5a/E 测试对账 | 文档 8 名逐个 `grep "fn <name>"` | **4/8 名不符**（真名带后缀） | ⛔ P0-testNames → 改文档后复跑 13/13 |
| 6 修复 + 回归 | 改 `exit.rs:wait_test`、`wait.rs:do_wait4` 三环、删 `WaitOutcome`；`docker minix-ci:1.94 cargo test -j 1 -p minix-pm --lib` | 基线 424/0 → 修复后 **426/0**（新增 2 个回归测试） | ✅ |
| 6 反向验证 | 临时回退两处实现后只跑新测试 | **双双 FAILED**（断言即 panic 文本）；恢复后 426/0 | ✅ 测试非空转 |
| 文风门 | `bash tools/doc-style-lint.sh <doc>` | 首跑 `SL-4=4 SL-10=30 \| error级=34` | ⛔ **D7：规范与检查器对 L 后缀口径相反** |
| 元门自验 | `bash tools/review-gate-check.sh zcode 04-stage-pm 10-pm-wait --strict` | 修 D1 后 **9/9 PASS** | ✅ |
| Gate G | `python3 tools/verify-check.py --scan … --symbols … --sample-ratio 0.5 --seed 20261008` | 首跑机械 FAIL（缺 `gate-evidence-G`）→ 补块后 **VERIFY-SELF PASS**；人工填表：issue 10/10 一致、符号真缺口 0 | ✅ **D10：自指闭环需两趟** |
| STATE 自验 | `python3 tools/review-state-validate.py --state .review/zcode/04-stage-pm/STATE.md` | `✅ PASS 无 ERROR` | ✅ |

## 二、Round 1 的流程缺陷（D 系列）

**D1 · 运行时表与两个元门脱节（阻断级）**
`review-process.md` §Step 0 的运行时表登记五家（trae/claude/codex/zcode/qoder），但
`tools/review-init.sh` 与 `tools/review-gate-check.sh` 都只认前三家，对 zcode 直接拒绝退出。
后果：ZCode 会话**按规范无法用推荐命令**初始化状态目录与跑元门；执行者只能手工 mkdir，元门则完全用不了
（等于 zcode 会话没有元门）。
处置：两工具加入 zcode/qoder（含帮助文本），复跑 `review-gate-check.sh zcode 04-stage-pm 10-pm-wait --strict` → 9/9 PASS。

**D2 · `coverage-extract.py --doc-file` 静默失配（假报告级）**
脚本按 `doc_dir` 相对名比较（`is_in_target_doc` 用 `doc_file in docs`），传仓内相对路径时**恒不命中**，
输出"文档覆盖 0.0% + 空符号表"，且**不报错、不警告**。规范源 Step 1.5 的命令模板写 `--doc-file {target-doc}.md`，
既可读成 basename 也可读成路径——照抄者极易踩中。
处置：脚本归一化（取 basename）+ 未命中即 `exit 2` 并列可用值；规范模板改为显式写"文件名（不含目录）"。

**D3 · `coverage-extract.py` 扫进 vendored cargo registry**
`--rust-dir os` 递归进 `os/.dockercargo/`，87939 个第三方符号（anstream/…）灌进 SYMBOLS.md 的
"Rust 实现符号"清单，把供 AI 反向核对的列表变成噪声。
处置：两处 `os.walk` 跳过隐藏目录与 `target`。

**D4 · 文档覆盖把中间产物当文档（假绿级）**
`check_doc_coverage` 把 `doc_dir` 下所有 `.md` 计入"文档覆盖"，包括 `.design/` 快照、`draft/`、
`doc_rerank_*.md`、`plan.md`、`todo.md`。AGENTS.md 明说 `.design/` 是中间产物、正式文档绝不引用，
让它计入覆盖会把 coverage 灌成假绿。
处置：已跳过隐藏目录（`.design/`）；`draft/`、`doc_rerank_*`、`plan/todo` 是否算"文档"需裁决（保留意见，见 §三）。

**D5 · `anchor-resolve.sh` 的 C 侧索引盲区（假 P0 制造机同族）**
其 C 符号索引只认函数 / 结构体 / 宏 / 枚举 / `#define`。**结构体成员**（`mp_procgrp`、`mp_exitstatus`…）与
**typedef 名**（`mess_lc_pm_wait4`、`mess_pm_lc_wait4`）一律判 `ZERO-DEF → 按 P0-fact 处理`。
本篇改写时把 6 处错锚点修成"精确成员锚点"（这正是 Step 1.0 想要的精度），反而一次冒出 5 个假 P0。
与 vfs-08 的 D1 同族（"歧义/盲区被当成缺失"），但触发面不同（那次是 basename 唯一性与跨行签名）。
处置：索引新增"成员声明行"与"typedef 收尾行 `} NAME;`"两个形态；自测加真实现场用例
（`long counter;` 带行尾注释、`} clock_info;`），期望 resolved 5→7；复跑本篇 `zero-def=0`。

**D6 · 锚点句意复核的 C 侧零覆盖（漏检级）**
`tools/anchor-suspect-baseline.txt` 的 868 条**全部是 `os/` 锚点，`minix3/` 一条都没有**。
本篇 6 处 C 锚点符号与句子讨论对象不符（`wait_test`↔`zombify`、`tracer_died`↔`cleanup`、
`exit_restart`↔`do_wait4`、`nice_to_priority`↔`set_rusage_times`、`sigaction`↔`mp_*` 四个成员），
而 `--check` 全绿、基线也不列——**机械门 100% 放行，事实错误成片存在**。
处置：规范侧补"按目标侧对称覆盖"的要求；基线文件按侧生成（本轮未实施，登记为流程修复项）。

**D7 · Step 1.0 锚点约定与 SL-10 文风门口径相反（自相矛盾级）**
`review-process.md` Step 1.0 的锚点约定表把 `（Lnnn，工具生成）` 列为"**写入正式文档**"的合法形态
（"行号（可选）｜工具派生，写成 `（L123，工具生成）`"）；而 `tools/doc-style-lint.sh` 的 SL-10 把**同一形态**
判为"工具派生行号提示占正文（应移入底账）"，且 `match_one` 只豁免 SL-5b/SL-8 —— 即 SL-10 被计入
`error级`，文风门（"零 error 命中"）直接阻断。执行者被两套规范同时指挥。
本篇 30 处命中 → 首跑 error 级 34。
处置：按 `style-bible` 的硬裁决执行（坐标是审查看的、讲解是读者看的；移入锚点底账
`.review/zcode/04-stage-pm/10-pm-wait.anchors.md`，用 `anchor-resolve.sh --extract` 生成）；**规范源 Step 1.0 的表待改**。

**D8 · 正文引用 `.design/` 没有任何门在查（漏检级）**
AGENTS.md 判"正式文档引用 `.design/` = P0-process-violation"，但 `doc-style-lint.sh` 无此规则、
`pattern-gate.sh` 无此模式、`anchor-resolve.sh` 只解析锚点。本篇 §3 与 §7 各一处命中，全靠人工发现。
处置：登记为流程修复项（给 pattern-gate 或 doc-style-lint 加一条规则）。

**D9 · `doc-code-map.sh` 不展开头部字段的花括号路径（漏文件级）**
头部写 `os/servers/pm/src/mproc/{wait,lifecycle,guardianship,table}.rs` 时，清单只列出花括号前那半段路径，
关联代码清单因此漏 6 个文件（同 stage 的 09-pm-exit.md 同样受影响）。
处置：本轮规避（头部逐文件列出）；工具待修（登记）。

**D10 · Gate G 证据块是自指闭环（规范未写明两趟）**
`verify-check.py` 要求 scan.md 含 `gate-evidence-G` 块，而该块的内容来自它自己生成的 VERIFY-CHECK.md
——首跑必然 `VERIFY-SELF FAIL`。属设计使然（先出骨架、回填证据块、再自检），但规范没写这一步。
处置：规范侧补一句"Gate G 两趟"；本轮按两趟执行（首跑 FAIL → 补块 → `VERIFY-SELF PASS`）。

**D11 · 覆盖率工具短名子串假阳性（保留意见）**
"文档覆盖"按子串匹配（`if name in content`），`US` 这类短符号命中任意含 "US" 的文本。
符号抽样里 `US` 被抽中，需人工判为假阳性。
处置：未修（登记保留意见）。

## 三、Round 1 的文档/代码缺陷（F 系列，摘要）

详见 `.review/zcode/04-stage-pm/10-pm-wait/scan.md` 的 Issue List。总量：**P0=11、P1=8、P2=2、OQ=1**（全部修复，OQ 上交）。

代码侧两条 P0（本轮唯一改到 `os/` 的地方）：

| 编号 | 缺陷 | 依据 | 修复与验证 |
|---|---|---|---|
| P0-waitTestStub | `exit.rs:fn wait_test` 忽略 `right_child`（自述 "For 09 we simplify…10 will refine"），`WaitState::is_waiting_for` 只在测试里被调用 → 父在等**别的**子进程时也会被交付 | `forkexit.c:wait_test` 583-587 | 接入 `is_waiting_for`；新增 `test_check_parent_waits_for_matching_child_only`（反向验证 FAILED） |
| P0-ringOrder | `wait.rs:fn do_wait4` 三趟独立遍历 → 交付对象由"表序优先"变"环序优先" | `forkexit.c:do_wait4` 501-548 | 合并为单趟逐子判定；新增 `test_wait4_table_order_beats_ring_order`（反向验证 FAILED） |

文档侧 P0 里最值钱的一条是 **P0-designRefInDoc**：正式文档引用 `.design/`（D8 的同案），
以及 **P0-waitHRefs / P0-nsigValue / P0-ipcRef / P0-utilRef** 四条成片的引用错误——
它们共同的特征是：**旧行号迁移把锚点符号换成了"就近的前一个符号"，句子照旧，于是"看起来权威、实则指错"**。

## 四、成本观察（流程侧，未修）

- **v2 快照重推导是每轮强制**（"默认每轮 review 都重新评估，无例外"）。本篇 352 行文档 + 2 个 Rust 文件，
  重推导产出 outline/outline-review/design 三份 + design-structure 脚手架，占本轮约 **1/3** 的工时。
  在小文档（< 300 行）上，"每轮全量重推导"的收益/成本比值得复议——保留意见，不改规则（改规则需用户裁决）。
- 三轮全流程（含修复与回归）在单会话内可完成，未触发 Resume Point（附录 A.2）。

---

# Round 2：`03-stage-rs/11-rs-publish.md`

## 一、执行记录（命令 → 结果 → 判定）

| Step | 命令 | 关键输出 | 判定 |
|---|---|---|---|
| 0 状态初始化 | `bash tools/review-init.sh zcode rewrite-notes/03-stage-rs/11-rs-publish.md` | ✅ 建好目录、打印派生路径与 coverage 模板 | ✅ **D1 修复生效**（Round 1 时同一命令被拒） |
| 0 预检 | 4 条 `ls` + `design-coverage-check.sh 03-stage-rs` | v1 三件齐在；ALL DOCS COMPLETE | ✅ |
| 1.0 锚点 | `anchor-resolve.sh --check` | 首跑 `anchors=51 resolved=12 zero-def=0 **multi-def=39**`——39 条全是同一根因（符号写成 `rproc`） | ⛔ **D12：MULTI-DEF 的"符号选错"形态** |
| 1.5 覆盖率 | `coverage-extract.py rs … --doc-file 11-rs-publish.md` | 语义域 Gaps 0 | ✅ |
| 2/3 语义与精度 | 逐条回 C 源 | 行号族全对；`glo.h:46`（实 51）错 | ✅ 无虚构 |
| 4.5a/E 测试对账 | 文档 §5 声称 5 项、列的是函数名 | 实测 6 个 `fn test_*` | ⛔ P0-testNames（修后 6/6） |
| 文风门 | `doc-style-lint.sh` | `SL-4=1 SL-10=32 \| error级=33` | ⛔（同 D7） |
| 6 修复 + 回归 | 39 处锚点改真名、§5 重写、§4.1 补 2 行、§3.1 改述；`cargo test -j 1 -p minix-rs --lib` | 354 passed / 0 failed（修前修后同） | ✅ |
| 元门 | `review-gate-check.sh zcode 03-stage-rs 11-rs-publish --strict` | **9/9 PASS** | ✅ |
| Gate G | `verify-check.py --sample-ratio 0.5 --seed 20261008` | 首跑即 PASS（`gate-evidence-G` 预置） | ✅ |

**Round 2 的新流程发现**：

**D12 · MULTI-DEF 的"符号选错"形态（规则缺口）**
`review-process.md` Step 1.0 的判定只写"多定义 → 要求文档补 impl 限定"。本篇 39 条 MULTI-DEF **补限定也修不好**——
它们不是"同名多处定义"，而是"锚点符号根本不是句子讨论的对象"（`rproc` 是结构体类型名，句子讲的是 `publish_service`）。
与 Round 1 的 D6（C 侧零覆盖）同族、表现不同：**ZERO-DEF 与 MULTI-DEF 都可能只是"符号选错"**，规则需要第三种判定。
处置：规范侧补"多定义先判是否符号选错（句意复核）"；本篇 39 条逐条改真名后 `multi-def=0`。

## 二、Round 2 的文档缺陷（F 系列，摘要）

**P0=3**（锚点符号 39 处、`glo.h:46`、测试名 5≠6）｜**P1=5**（编排接线状态与代码不符、§4.1 缺 2 个已实现函数、测试总数 208→354、代码注释 `glo.h:46`、复述卡点）｜**P2=1**。
详见 `.review/zcode/03-stage-rs/11-rs-publish/scan.md`。

---

# Round 3：`06-stage-sched/14-rs-interaction.md`

## 一、执行记录

| Step | 命令 | 关键输出 | 判定 |
|---|---|---|---|
| 0 预检 | `review-init.sh zcode` + 4 条 `ls` + design-coverage-check | 全绿 | ✅ |
| 1.0 锚点 | `anchor-resolve.sh --check` | `anchors=19 resolved=9 **multi-def=10**`（同 D12 根因） | ⛔ → 改真名后 20/20 |
| 1.5 覆盖率 | `coverage-extract.py sched … --semantic-map …` | **无 `sched-semantic-map.json`** → Rust 侧恒 0%；不传 `--rust-dir` 时静默 `rust_dir=None`（`Found 0 Rust symbols`） | ⛔ **D13：工具数据缺口伪装成覆盖率缺口** |
| 2/3 语义与精度 | 逐条回 C 源 | `manager.c` 456→455、466→470 两处错；`sched.rs` 三处坐标漂移 | ✅ 无虚构 |
| 4.5a/E 测试对账 | §5 声称"既有 4 个" | 实测 6 个（漏 2 个 panic 断言测试）；基线命令指向不含所列测试的 crate（`minix-sched`=88，新增测试在 `minix-rs`=354） | ⛔ P1（已修） |
| 文风门 | `doc-style-lint.sh` | `SL-10=13 \| error级=13` | ⛔（同 D7） |
| 元门 | `review-gate-check.sh zcode 06-stage-sched 14-rs-interaction --strict` | **9/9 PASS** | ✅ |
| Gate G | `verify-check.py --sample-ratio 0.5 --seed 20261008` | 首跑即 PASS | ✅ |

**Round 3 的新流程发现**：

**D13 · 工具数据缺口伪装成覆盖率缺口（误判级）**
`sched` 模块没有 `*-semantic-map.json`（`tools/coverage-extract/` 下 12 张表里没有它），
于是 Gate A 的"Rust 覆盖"恒 0%。规则只写了"Rust 覆盖率为 0% 时先检查 `--rust-dir`/`--semantic-map` 是否正确，
或确实缺失实现"——**缺了"映射表本身不存在"这一分支**，执行者容易把工具数据缺口写成实现缺口（P0 误报）。
另外不传 `--rust-dir` 时脚本静默 `rust_dir=None`（打印 `Rust: N/A`），不报错。
处置：本轮新建 `tools/coverage-extract/sched-semantic-map.json`（18 条映射）；规范侧补判定分支。

## 二、Round 3 的文档缺陷（F 系列，摘要）

**P0=2**（锚点符号 10 处、`manager.c` 两处行号）｜**P1=4**（基线命令错 crate、既有测试漏 2、复述卡点、缺头部字段）｜**P2=1**。
详见 `.review/zcode/06-stage-sched/14-rs-interaction/scan.md`。

---

# 三轮汇总：流程缺陷清单（本文件 D 系列）

| # | 缺陷 | 级别 | 处置 |
|---|------|------|------|
| D1 | 运行时表登记五家，两个元门只认三家（zcode 被拒） | 阻断 | ✅ 已修两工具（含帮助文本），复跑 9/9 |
| D2 | `coverage-extract.py --doc-file` 静默失配 → 0% 假报告 | 假报告 | ✅ 已修（归一化 + 未命中 exit 2） |
| D3 | `--rust-dir os` 扫进 `os/.dockercargo`（87939 第三方符号） | 噪声 | ✅ 已修（跳隐藏目录） |
| D4 | 文档覆盖把 `.design/` 等中间产物当文档 | 假绿 | ✅ 已修（跳隐藏目录）；`draft/rerank` 待裁决 |
| D5 | `anchor-resolve.sh` C 侧不索引结构体成员与 typedef → 假 ZERO-DEF | 假 P0 | ✅ 已修 + 自测加现场用例 |
| D6 | 锚点句意复核的 C 侧零覆盖（基线只有 `os/`） | 漏检 | ✅ 新增 `tools/anchor-suspect-scan.py`（按侧扫描）+ 生成 C 侧基线（**3501 处 / 776 篇**，此前为 0 覆盖）；规范注册表 1.1 已指向该工具 |
| D7 | Step 1.0 锚点约定与 SL-10 文风门口径相反 | 自相矛盾 | ✅ 规范侧改 Step 1.0；三篇文档坐标已移入底账 |
| D8 | 正文引用 `.design/` 无任何门在查 | 漏检 | ✅ 规范侧补检查项（Round 1 实证 1 例） |
| D9 | `doc-code-map.sh` 不展开花括号路径 | 漏文件 | ✅ 已修（含 `{}` 的正则 + 递归展开；10-pm-wait 清单 2→9、09-pm-exit 10 个文件） |
| D10 | Gate G 证据块自指闭环，规范未写"两趟" | 规范缺口 | ✅ 规范侧补一句 |
| D11 | 覆盖率工具短名子串假阳性（`US`） | 假绿 | ✅ 已修（词边界匹配；10-pm-wait 文档覆盖 20→17，假阳性消失） |
| D12 | MULTI-DEF 的"符号选错"形态，规则只有"补限定"一条 | 规则缺口 | ✅ 规范侧补第三种判定 |
| D13 | 模块缺语义映射表 → Rust 覆盖恒 0%，规则无此分支 | 误判 | ✅ 新建 sched 映射表 + 规范侧补分支 |

**三轮合计**：流程缺陷 13 条（12 条已修/已补规范，1 条登记）；文档/代码缺陷 **P0=16、P1=17、P2=4、OQ=1**（全部修复，OQ 上交）；
代码侧改动仅 Round 1 的两处 P0（`exit.rs:wait_test`、`wait.rs:do_wait4` 三环）+ 1 处死代码删除 + Round 2 的 1 处注释，
全部经 `cargo test` 回归（`minix-pm` 424→426、`minix-rs` 354、`minix-sched` 88，均 0 failed）并做反向验证。

---

# 收尾：四项登记项全部落地（2026-10-08 第二轮会话续）

| 项 | 处置 | 证据 |
|----|------|------|
| **OQ-1**（`W_EXITCODE` 的 `sig_status` 位型） | 按 Ground Truth 优先序**取 C 语义**：`main.c:358` 的 `mp_sigstatus \|= WCOREFLAG` + `forkexit.c:713` 不掩码 ⇒ 符号扩展；`forkexit.c:749` 显式 `& 0377` ⇒ 掩码。Rust 侧逐位照搬（`exit.rs` 的 `tell_parent`/`tell_tracer`） | 测试改断言 `(0o200\|6) as i8 as i32`；`cargo test -p minix-pm --lib` = 426 passed / 0 failed；文档 §2.6 补符号扩展说明 |
| **D9**（花括号路径不展开） | `doc-code-map.sh` 提取正则纳入 `{}` + 递归展开（修了 bash `local` 同句赋值看不到前项的坑） | 10-pm-wait 关联代码 2→**9** 文件；09-pm-exit **10** 文件 |
| **D11**（短名子串假阳性） | `coverage-extract.py` 文档覆盖改**词边界**匹配 | 10-pm-wait 文档覆盖 20→17（假阳性消失），语义域 Gaps 仍 0 |
| **D6**（C 侧句意零覆盖） | 新增 `tools/anchor-suspect-scan.py`（按侧扫描、可生成基线）+ 生成 `tools/anchor-suspect-baseline-c.txt` | **C 侧 3501 处可疑锚点 / 776 篇**（此前 0 覆盖）；三轮已修文档自检仅余 3 处已知假阳性形态（表格行/块级锚点） |
