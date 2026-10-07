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

**D2 · `pattern-gate.sh` 对未知旗标静默接受**：我传 `--diff`（它没有这个模式）时它照样跑全量并打印
"附增量门 RANGE=HEAD"，实测 120 秒未结束。调用者以为跑了增量门，实际等的是全量。已加参数守卫：
未知参数 exit 2 并指向正确的增量工具。

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
