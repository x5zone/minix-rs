# 全量 REVIEW 干跑轨迹：05-stage-vfs/08-worker-thread.md

> **创建**: 2026-10-08
> **用途**：这是"拿一篇真文档跑完整 review 流程"的执行轨迹，目的有两个——① 检验 `prompt/` 定义的
> REVIEW 流程是否真的可执行、可判定、无自相矛盾；② 产出可复跑的取证，供独立审计方复核流程本身。
> 每一步的**原始输出**在 `.review/claude/05-stage-vfs/08-worker-thread/`（scan.md 的各 gate-evidence 块）。
> 该目录不入版本库；本文件只留结论与命令，命令可在当前仓库直接重放。

## 一、靶子与为什么选它

| 项 | 值 |
|---|---|
| 阶段 | `05-stage-vfs`（用户点名的两个 stage 之一） |
| 文档 | `rewrite-notes/05-stage-vfs/08-worker-thread.md`，309 行 → 按 Step 0 时间预算表属"1 轮"档 |
| 章节骨架 | 七章齐（概念 / C 源码 / 设计决策 / 实现详解 / 测试要点 / 过渡 / 参见） |
| Rust 侧 | `os/servers/vfs/src/worker.rs` 1855 行、`main_loop.rs`、`fproc.rs` |
| C 侧 | `minix3/minix/servers/vfs/worker.c`（14 个函数） |
| 为什么有代表性 | 它是全项目唯一"多线程被消除为状态机"的架构演化现场（`[ARCH A-1]`），同时压执行模型、契约表、锚点、文风、测试对账五个维度 |

## 二、按 Step 顺序的执行记录（命令 → 结果 → 判定）

| Step | 命令 | 关键输出 | 判定 |
|---|---|---|---|
| 0 状态恢复 | `bash tools/review-init.sh claude <doc>` | 建好 `{stage}/{doc-stem}/` 并打印派生路径与 coverage 命令 | ✅ 可用 |
| 0 预检（Gate H 前置） | `ls .design/08-*` + `bash tools/design-coverage-check.sh 05-stage-vfs` | 三快照 v1 齐在；`ALL DOCS COMPLETE` | ✅ 无需 Step 0.3 嵌入生成 |
| 0 关联代码清单 | `bash tools/doc-code-map.sh <doc>` | ❌ 无关联代码：文档没有头部 `Rust 实现` 字段 | ⛔ **Gate 0 阻断 → 先修再续** |
| 1.0 锚点 | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=82 resolved=70 zero-def=7 multi-def=5` | ⚠️ 7 处 ZERO-DEF 待逐条核 |
| 0 文风门 | `bash tools/doc-style-lint.sh <doc>` | `SL-4=1 SL-10=23 \| error级=24` | ⛔ 阻断 |
| 1.5 覆盖率 | `coverage-extract.py vfs … --doc-file …` | C 符号 415；语义域 297，Doc 59、Rust 37，**Gaps 0** | ✅ |
| 2/3/3.5 语义与精度 | 逐条对 C 源核符号（见第三节） | 4 处 C 函数无同名 Rust 函数，其中 2 处经查证实为改名（`stop_by_endpoint`、`yield_now`） | ✅ 无虚构 |
| 3.6 代码维度 | `grep -cE 'todo!\|unimplemented!\|panic!'`、`grep -c unsafe`、`unsafe-audit --diff`、`cargo test -p minix-vfs --lib` | 全 0；**538 passed / 0 failed** | ✅ |
| 4.5a/E 测试对账 | 文档 19 个测试名逐个 grep 代码 | **缺失 0** | ✅ |
| 6 独立验证 | 写 VERIFY-CHECK.md 并重放每条命令 | 9 项全部可复跑 | ⚠️ 同 agent，局限已标注 |
| 元门自验 | `bash tools/review-gate-check.sh claude 05-stage-vfs 08-worker-thread --strict` | 首轮 **8/9**（缺 gate-evidence-code）→ 补齐后 **9/9** | ✅ |
| STATE 自验 | `python3 tools/review-state-validate.py --state .review/claude/05-stage-vfs/STATE.md` | `✅ PASS 无 ERROR`，1 个 WARNING（未带 `--scan`，属可选项） | ✅ |

## 三、干跑发现的真缺陷（都是流程侧，不是文档的错）

**D1 · `anchor-resolve.sh` 制造假 P0-fact，两处独立成因**（该文档 7/7 个"零定义"全是误报）

1. basename 索引只收录**全树唯一**的文件名（`cnt[bz]==1`）。`pipe.c` 在 `minix3/minix/lib/libc/sys/` 与
   `minix3/minix/servers/vfs/` 各有一份 → 被剔除 → 裸文件名锚点掉进 `文件不存在 → 按 P0-fact 处理`。
   **歧义被当成缺失**。全树 85 篇文档在用裸文件名写法，等于这批文档随时可能被下游 agent 判为
   "含 7 类 P0 事实错误"并去"修正确内容为错"。
2. C 函数定义识别只认"`NAME(` 的下一行是 `{`"。Minix3 大量使用**跨行签名**（返回类型独占一行、
   参数续行数行），如 `cdev.c:280 cdev_io(int op, dev…,\n\tunsigned long bytes, int flags)\n{` → 误判无定义。

修法：索引改收全量重名文件 + 裸文件名按"哪个候选真含该符号定义"消歧；唯一命中即 resolved 并注明消歧路径；
多命中含定义 → MULTI-DEF（要补限定）；全都不含 → 才是真 ZERO-DEF。签名识别改为前瞻最多 6 行找 `{`（遇 `;`/`}` 停）。
修后同一文档 `zero-def=0`、`anchors` 由 82→81（少掉的那条是 F3 的伪锚点）。
自测已加真实现场用例（`pipe.c:pipe_suspend` 与 `cdev.c:cdev_io`）；反向验证：把跨行签名支持削弱后自测立刻 FAIL，
证明夹具不是空转。**我一度把这两个工具的故障当成文档的 P0**，是逐条回 C 源核对才纠正——这条判断纪律记在这里。

**D2 · 撤回：这条是我误判，不是缺陷。** 我传 `--diff` 时看到横幅写"全量检查"、且 120 秒没结束，
就断定"未知旗标被静默接受"。实际 `--diff` 是 `pattern-gate.sh` 的真实参数（置 `DO_DIFF=1`），
它按设计**同时**跑 P1–P6 全量项再附增量门，所以慢；横幅那句"附增量门 RANGE=HEAD"正是它认出旗标的证据。
我加的那个"参数守卫"反而是错的——它的帮助文本宣称"本工具只支持 --full"，而 `--full` 根本不是参数，
且它会把 `--full` 这类真实不存在的输入按自己的错误文案拒绝。已删除该守卫。
顺手修的是真存在的一处小毛病：`-h/--help` 走 `usage()` 却固定 `exit 2`，把"问怎么用"当成"用错了"；
现显式请求帮助退 0。四组实测：`--help` rc=0、`--bogus` rc=2、`--self-test` rc=0、`--diff` 正常被识别在跑。

> 教训（与前两轮的"凭 rc 断言 --help 不支持"同族）：**报别人缺陷之前先读完它的参数表**。
> 我这轮三次误判全都没有越过"看一眼实现"这一步，而每一次都会浪费审计方一轮复算。

**D3 · `SYMBOLS.md` 两个百分比并列不带口径**：`文档覆盖 59 (14.2%)` 与 `59/297 (19.9%)` 同屏出现，
前者分母是整个 C 目录全部符号、后者是语义域符号。缺口径说明会被读成"本篇只覆盖 14%"，进而误判覆盖率缺陷。
已在 `coverage-extract.py` 输出模板里加口径注释。

**D4 · 底账的生成入口没写进规范**：`anchor-resolve.sh --extract` 早就存在并能输出
"文档 TAB 行号 TAB 锚点"三列，但 `style-bible` 的锚点底账格式节只给格式不给工具，我只能手工逐行抄。
已在规范里补 `--extract` 与移出后复跑 `--check` 两条命令。

**D5 · 规范源与检查器对 VERIFY-CHECK 的位置要求不一致**：AGENTS.md 与 `review-process.md` 说 claude/codex
产物写在 `.review/{tool}/{stage}/{doc-stem}/`，而 `review-gate-check.sh` 只认 stage 级
`.review/{tool}/{stage}/VERIFY-CHECK.md`。我按规范写→ 元门判 FAIL；把文件挪到 stage 级→ 元门会过但
多篇并行 review 时同名文件互相覆盖。已改检查器接受文档级（并优先取文档级），保留 stage 级兼容历史。
**这是元门反过来暴露规范自身矛盾的一例。**

**D6 · 元门确实会抓执行者的偷懒**：我第一版 scan.md 漏了 `gate-evidence-code`（Step 3.6 关联代码维度证据块），
`review-gate-check.sh --strict` 判 8/9 并列明缺失项。补齐后才 9/9。说明"B4.1 强制证据块"这条规则是可执行的，
不是纸面要求。

## 四、文档与代码侧的实质发现（对照"文档没写错"这一层）

| 编号 | 严重度 | 内容 | 状态 |
|---|---|---|---|
| F1 | P1 | 缺头部 `Rust 实现` 字段，Gate 0 的关联代码清单直接判失败 | 已修：3 个文件，存在性 3/3 |
| F2 | P1 | Doc-Sync-7：测试基线记 96（`main_loop` 23、`fproc` 13），实测 538（94、22），偏差远超 50%；且正文带日期 | 已修：改实测口径 + 复核命令，去日期 |
| F3 | P1 | `` `bdev.c:cdev.c` `` 的冒号让"两个文件"被读成"bdev.c 里的符号 cdev.c"，既误导读者又产出假 P0 | 已修措辞，zero-def 1→0 |
| F4 | P2 | 23 行含 27 处 `（Lnn，工具生成）` 工具派生坐标占正文（SL-10） | 已按"有讲解就留，只有坐标就走"移入 `.review` 底账，正文 error 级 24→0 |
| F5 | P2 | 5 处 MULTI-DEF 需补限定（`worker.c:worker_thread` 474/572、`main_loop.rs:fn lu_prepare` 125/917） | 未修：属锚点卫生项，按 D5/北极星不占收敛轮次，记入 backlog |
| F6 | 观察 | 标题"`NR_WTHREADS=9` 真实线程到请求槽状态机…"我一度判为易误读；复查 §3/§4 后撤回——标题本身是"从…到…"句式，且 §4 明确 `worker_yield` ↔ `yield_now` 为 `noop`，代码注释与测试三处一致 | 不改 |
| — | 结论 | 生产代码未改一行（只改文档与工具），故无需代码回归；仍跑了 `cargo test -p minix-vfs --lib` 留基线：538 passed / 0 failed | — |

## 五、对"这套流程本身"的结论

1. **可执行性**：Step 0 → 7 的每一步都有对应命令、都能产出判定，包括元门（`review-gate-check.sh`）与
   STATE 校验（`review-state-validate.py`）——它们真的会因缺件而 FAIL，不是装饰。
2. **最危险的失效模式已定位并修掉**：假 P0 制造机（D1）。它的危害不是"多报几个问题"，而是**驱动 agent
   去把正确内容改错**，并让"回真源核对"这条 Ground Truth 纪律退化为"回工具输出核对"。
3. **流程的自反性成立**：本轮出现三次"我以为发现缺陷、其实是自己用错工具/写错检查"（D1 的误判、
   `timeout eval` 的调用方式错、PIPESTATUS 取退出码错）。这说明单靠人读输出会误判，
   **门的结论必须由命令重放支撑**——这正是 gate-evidence 与 L10–L17 这类元门存在的理由。
4. **仍缺的能力（未实施，登记）**：
   - 底账抽取目前仍需手工（`--extract` 能列锚点清单，但"从正文删坐标 + 写底账 + 保持 sha 可对账"
     的闭环工具没有，本轮我是一次性脚本做的）；
   - 符号锚点的 `SKIP 多行号` 与 `path:line` 类引用不在校验射程内（工具自述如此），
     与 TODO-1 的 D 类判定（合法保留）需要对齐一句说明；
   - `pattern-gate.sh` 全量耗时 >2 分钟，不适合进 CI（CI 里确实没放它）。
5. **流程验证产物**：`.review/claude/05-stage-vfs/STATE.md` + `{doc-stem}/` 下 scan/structure/SYMBOLS/
   VERIFY-CHECK/anchors 底账；`review-gate-check.sh --strict` 9/9 PASS，`review-state-validate.py` PASS。

---

## 六、第二轮补跑：把"全部可执行的 review"真的跑完（并纠正我上一节的未证实数字）

第一节到第五节只跑了流程的前半。**漏跑的检查**：行号漂移门、H3 代码块逐块审查、Gate G 的机械化验证器。
补跑后新出四条缺陷（编号接前文，D 系列续到 D7–D9，issue 续到 F10/F11）：

| 命令 | 结果 | 判定 |
|---|---|---|
| `tools/review-line-check.sh {doc}` | 37 项行号引用全 OK，0 漂移 | ✅ 该文档无行号漂移 |
| `tools/doc-snippet-extract.sh {doc}` | 无输出 | 该文档只有 2 个 C 代码块、无 Rust 块 → H3 检查面为空，属正常不适配而非漏检 |
| `tools/verify-check.py --scan scan.md …` | 首跑判**机械不合规**：9 个锚段名不匹配、`gate-evidence-*` 不是围栏块、issue 编号非 `P[012]-xxx` | ⛔ D7（见下） |

**D7 / F10 · 机械契约只活在脚本里（本轮最值的流程发现）**
`review-gate-check.sh` 只看产物文件是否存在，给我的 scan.md 判 **9/9 PASS**；
而 `verify-check.py` 按字面锚段名与围栏格式判**机械不合规**。同一制品两个门结论相反，
且更严格一方的判据**从未写进规范源**（`review-process.md` 里 `grep verify-check.py` 命中 0 次）。
后果：执行者按猜测产出 scan.md，Gate G 机器验证形同虚设，元门还给绿灯。
处置：把契约写进 `review-process.md`「scan.md 的机械契约」小节（9 个实名锚段 + ` ```gate-evidence-X ` 围栏
+ issue 首列必须 `P[012]-标识` + Gate G 产物由该脚本生成后再填人工段），并按契约重写本轮 scan.md。

**D8 · 验证器的符号抽取不认反引号**：SYMBOLS.md 的符号行是 ``| `send_work` | comm.c:37 | …``，
旧正则只认无反引号形式 → 59 个真符号**一个都没抽到**，只从汇总表捞到一个 `Gaps`，
于是"已抽样验证符号覆盖"整段静默空转。修后同一文件抽出 59 个，抽样 20 个，真缺口 0、
命名匹配产物 4。另修**报告把抽样比例写死成「20%」**而与命令行参数不符（自述失真）。

**F11 / `P2-toolGapLabel` · Gate A 的覆盖标签会产假缺口**：✅/⚠️/❌ 按符号名匹配，
未登记改名映射的符号（`do_work`、`service_pm`、`drv_sendrec`、`sef_cb_lu_prepare`）即使语义已由
请求槽状态机承担也判成缺口，所以「完全缺口 117」不可直接当收敛依据；本轮收敛依据改用语义域口径
（`Gaps (semantic): 0`），并在 scan.md 与 STATE.md 标注。

**纠正我自己的未证实数字**：上一轮我在 VERIFY-CHECK 草稿里写过「符号抽样 9 个」，那是照工具首跑输出
手推的，实际抽样是 **20** 个（`sample_symbols` 取 `max(5, len×ratio)`）。已按实测改写，并把该口径写进报告。
这正是本仓「统计门槛数字严禁抄表，必须现算」那条纪律的又一次应验——我自己也没豁免。

**Gate G 终判**：issue 抽样 5/5 一致（100%）、符号抽样 20 个真缺口 0、机械化检查 8/8 证据块齐全、
收敛三项逐条重放 → **PASS**（附 `P2-toolGapLabel` 保留意见）。产物：
`.review/claude/05-stage-vfs/08-worker-thread/{scan,structure,SYMBOLS,VERIFY-CHECK,08-worker-thread.anchors}.md`
与 `.review/claude/05-stage-vfs/STATE.md`；元门 `review-gate-check.sh --strict` **9/9**，
`review-state-validate.py` PASS，CI 四道门全绿。

---

## 七、第三轮：把"元门自己"也验一遍（又抓到两条，含一条我自己的产物不合规）

补跑到这里，我把**验收产物的两个元门**也各自做了埋点复现，结果一好一坏：

**D10 · `review-state-validate.py` 会把路径从中间截断，产假告警**
它的 `PATH_REF_PATTERN` 负向后顾是 `(?<![\w/])`，**漏了连字符**。于是
`` `rewrite-notes/05-stage-vfs/08-worker-thread.md` `` 被从 `notes` 处截断匹配成
`notes/05-stage-vfs/08-worker-phase.md`… 实际抽出的是 `notes/05-stage-vfs/08-worker-thread.md`，
该路径不存在 → 报 **ERROR: STATE.md 引用不存在的文件**。
复现（修前）→ 抽取结果 `['notes/05-stage-vfs/…']`；修复（后顾加 `-`，并把三棵树名并列进前缀）后 →
`['rewrite-notes/05-stage-vfs/…']`。这是本轮**第三条**"歧义/截断被当成缺失"同族缺陷，与前两条 D1 同因。

**D11 · 假告警一修，真问题立刻现形：我的 STATE.md 不合模板**
修掉 D10 后校验器报出 `缺失必备段: ['Phase Completion Log', 'Per-Doc Status', 'Session Status']`。
也就是说此前那句"✅ PASS 无 ERROR"是**被假告警遮蔽后的误读**（我第一次跑它时它先报了别的），
而我的 STATE.md 确实漏了模板三个段。已按模板补齐（Session Status / Per-Doc Status / Phase Completion Log，
每段都指向 scan.md 的具体 gate-evidence 块）。修后：`✅ PASS 无 ERROR、无 WARNING`。

**这一轮的自我提醒**：我在第六节纠正了自己写下的"符号抽样 9 个"（实际 20），这一节又证明
"我看到校验器绿了"不等于"校验器查了我以为它查的东西"。流程里所有"✅"都必须在**修掉自身缺陷之后**复跑一次才算数。

## 八、最终状态（三项要求逐条对账）

| 要求 | 证据 |
|---|---|
| 一篇文档跑完全部可执行 review | Step 0/0.5/1/1.0/1.5/2/3/3.5/3.6/4.5/5/5.6/5.7/7 全部执行；产物 5 份 + 底账 1 份在 `.review/claude/05-stage-vfs/` |
| 门挡住就先修再跑 | Gate 0（缺字段）、文风门（24 error）、Gate G 机械验证（格式不合规）、STATE 校验（缺段 + 假告警）四次挡住，全部先修后复跑通过 |
| 若改代码要回归 | 未改 `os/` 生产代码；仍留 `cargo test -p minix-vfs --lib` = 538 passed / 0 failed 基线，另跑 `unsafe-audit --diff`、`check-rs-unwired` 均 PASS |
| 用轨迹反向验证流程本身 | 得 11 条流程缺陷（D1–D11），其中 3 条是"假 P0/假告警"同族、4 条是"契约只活在脚本里"、4 条是工具健壮性；全部已修或登记，并把契约写回 `review-process.md` |

**元门终态**：`review-gate-check.sh --strict` 9/9、`review-state-validate.py` 无 ERROR 无 WARNING、
CI 四道门全绿、六个工具自测全 rc=0。

---

## 九、D12：我把别人提交的文件差点删了（自纠）

跑自测后清理残留时我执行了 `rm -rf tools/.anchor-resolve-selftest`。事后 `git status` 出现
` D tools/.anchor-resolve-selftest/*` —— 说明这些文件**是被跟踪的**：commit `1b78ee7d3`
「WIP(stop)…历轮遗留 untracked 文件（notes/.trae/.zcode/AI-chats/tools 等）」那次批量 add
把三个自测生成目录一起扫进了版本库。已 `git checkout --` 还原，再改为**只取消跟踪**
（`git rm -r --cached`，磁盘保留、自测可重建），让 `.gitignore` 里 `tools/.*-selftest/` 那条规则真正生效。

两条教训：
1. **删任何东西之前先确认它是否被跟踪**——未跟踪才可删，已跟踪的删除必须是显式决策，不能是顺手清理。
   我这次的判据本应写成"跑一次 `git ls-files <目标>` 非空就停手"。
2. **别用 `2>/dev/null` 吞命令错误**：我第一次 `git rm -r --cached ... 2>/dev/null` 因为把不存在的
   `tools/__pycache__` 混在同一条命令里而整体失败，我却据此以为已取消跟踪；直到 `git status` 仍显示
   被跟踪才暴露。同一条错误我在迁移期就犯过一次（`git add` 被吞导致提交未发生）。

被误跟踪的生成物清单（`git ls-files | grep selftest` 实测）：`.anchor-resolve-selftest` 4 个、
`.anchor-migrate-selftest` 3 个、`.doc-snippet-selftest` 1 个，全部已取消跟踪；
`minix3/**/selftest*` 与 `tools/atf-c-compat/md5_selftest.c` 是真实源码，未受影响。

---

## 十、第四条与第五条可执行检查（收口）

| 检查 | 命令 | 结果 |
|---|---|---|
| Step 0.7 TODO 陈旧预检（模式 70） | `bash tools/todo-staleness-check.sh rewrite-notes/05-stage-vfs/todo.md` | `PASS (0 staleness issues)` |
| 地址常量扫描（代码维度） | `python3 tools/address-constant-scan.py os/servers/vfs/src/{worker,main_loop}.rs` | 四类全 0（LITERAL/SHIFT/FAMILY/CONST） |
| 未接线标记扫描 | `bash tools/check-rs-unwired.sh` | `✅ PASS: no unwired markers` |

至此可执行检查全部跑完。**共 14 条流程观察**：成立并修复 D1、D3–D12 共 11 条，
误判撤回 D2 一条（我自己没读参数表就报缺陷，已在轨迹就地更正），
不适配说明一条（`doc-snippet-extract` 对本篇无 Rust 代码块，输出为空属正常）。

---

## 十一、清掉 review 自身遗留的两条 P2（把干跑做到零开放发现）

**A. 5 处锚点不是"需补限定"，而是指错了对象（我上轮判低了一档）**
逐条回 C 源核对后：`worker_thread` 在 `worker.c` 里是**结构体类型名**；那几处命中的其实是
`worker_suspend`（474 行）与 `worker_get`（572 行）的返回类型。按语义逐条改真名：

| 文档位置 | 原锚点 | 依据 | 改为 |
|---|---|---|---|
| §4 表 `worker_suspend` 行 | `worker.c:worker_thread` | 该行讲保存 err 并挂起 → `worker.c:474 worker_suspend` | `worker.c:worker_suspend` |
| §4 表 `worker_get` 行 | `worker.c:worker_thread` | 槽位索引查表 → `worker.c:572 worker_get` | `worker.c:worker_get` |
| §4 协程往返行 | `worker.c:worker_thread/504` | 493-504 是 `worker_resume` 的 `self = org_self; err_code = self->w_err_code` | `worker.c:worker_resume` |
| §5 测试表 | `worker.c:worker_thread` | 测的是 suspend→resume 往返，恢复侧 | `worker.c:worker_resume` |
| §3 为什么段 | `main_loop.rs:fn lu_prepare` | 文件内有自由函数 125 与 `impl VfsState` 方法 917 两处同名 | `main_loop.rs:impl VfsState::lu_prepare` |

`multi-def` 由 5 降到 1；`zero-def` 仍 0；`doc-style-lint` error 级 0；`review-line-check` 37 项仍全 OK。

**B. D13（新发现，工具缺陷）**：剩下的那 1 处 `multi-def` 不是文档问题。
按验证器自身的判定逻辑独立复现（同一 `typ=VfsState`、`method=lu_prepare`）只命中 **917 一行**；
而 `anchor-resolve.sh` 报的是「行 411,7504」——**411 是 `impl VfsState {`、7504 是 `impl Default for VfsState {`**，
都是 impl 声明行而非 `fn` 行。也就是说它对 `impl 类型::方法` 这种限定名走错了分支（或回落到了按类型名查条目）。
处置：**不为了把灯改绿而回退文档表述**——文档现在写的是语义正确的限定名。登记为工具缺陷待修，
并在 scan.md 的 Issue List 里以 `P1-toolImplQual` 记录（严重度 P1：它会误伤每一个正确使用限定名的锚点）。

**C. 结构第 11 节的白话建议已采纳**：`block_all` 的门控含义在 §1 末补了一句白话解释
（见文档 §1 结尾），使读者不回 §2 也能复述本篇主旨。
