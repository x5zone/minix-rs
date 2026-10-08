# doc-commit-review-v2-ds — 工作流优化提交的第二轮复核（含对四份 v1 审计的重定基线）

> **复核对象（三层）**：
> ① 被审主提交 `6ff56f440`（教学构件承载门#9 + 必备构件清单 + 反膨胀仲裁 + outline-first + `doc-code-density-check.sh`）；
> ② 关联提交 `7db999c35`（doc-gen 第 1.5 步）、`2d9f0b46f`（01 文档增强 +175 行）；
> ③ **其后的两轮修复**：`ff3524c07`（二修：承载门工具 5 缺陷 + 规则对齐 + Gate H precedence + AGENTS/skill 同步）
> 与 `9dbc3cca6`（三修：coverage-extract 语义 gap 恒真修复 + 新增 `doc-ref-check.sh` + 注册表/镜像同步 + CI）。
>
> **为什么 v2 必须先做这件事**：四份 v1 审计（ds/glm/longcat/muse）的**载重发现早已被②③修掉**（295 行原型放行、
> 图算码、静默绿、陈旧计数、P1-docStyle 未定义、Gate A 恒真…）。照 v1 清单去修会**修已经修过的东西**，
> 而 v2 的价值在于：把 v1 的每一条判据在**当前树**上重放一遍，分清"已闭合 / 仍开放 / v1 未预见"。
>
> **基线**：HEAD = `9dbc3cca6`（复核时的树；命令见 §七）。**证据口径**：本报告全部读数为本次实测（L1），
> 凡属判断者标"判断"；无法在单 agent 内定案者标"需确认"。
> **同 agent 局限**：本复核为单 session，未做异构交叉复核；关键判据均附可重放命令。
> **日期**：2026-10-08。

---

## 一、结论先行

**主干已经立住了，v1 的五条载重发现全部闭合且我逐条复现；当前最该做的不是再加门，而是给新门配"存量口径"与"看不见就得说没看见"守卫。**

三句话展开：

1. **正确性**：主提交声称的落地项（含四道同步门）在 v1 已被证实，**没有再出现"说了没做"**；其后两轮修复把 v1 的效力级缺陷修到了实测可过的程度——`295 行事故原型 → --check 退 1`、`现行 470 行 → --check 退 0`，这是本次工作流优化真正的 DoD，现已达成。
2. **仍开放的最大一项**（v1 只看到 44 篇，我实测 **259/369 = 70%**）：新判据把"分析章零真码"这条标准落到全树后，**存量文档七成命中**，而"存量只报告"**没有任何机器机制**（`grep baseline|--diff tools/doc-code-density-check.sh` = 0）。这不是文档问题，是**政策缺口**：要么给基线/差分，要么给逐篇延期登记面，否则执行者只有两条路——成批返工（未授权）或学会绕开 `--check`（门死亡）。
3. **我带来的独有证据（执行侧）**：我在同日跑过 13 轮真实 review（`.review/zcode/` 13 篇 CONVERGED、62 个 P0 全修）。缺陷分布说明：**现场最高频、最高危、最可机器判的是"指称腐化"（锚点符号错、行号陈旧、虚构符号、落点错），不是"教学构件缺失"**。所以下一批的火力顺序应当是**指称类机器优先**（`doc-ref-check.sh` 的落地方向是对的），教学构件类已够用。

---

## 二、基线重定：v1 载重发现在当前树上的实测状态

| # | v1 发现（出处） | 我的复算命令 | 现测 | 判定 |
|---|---|---|---|---|
| 1 | 295 行事故原型被门放行（glm C1 / muse V-3 / ds §3.1） | `git show c5d21dbfb:…> /tmp/old295.md; bash tools/doc-code-density-check.sh --check /tmp/old295.md; echo $?` | **EXIT=1**，点名第 2/4 章 | ✅ **已闭合** |
| 2 | 图算码、语言无关计数（muse V-4 / ds §3.2） | 工具输出已带"码/令/树/架/辨/短/豁免"七列读数 | 语言感知计数在位 | ✅ **已闭合** |
| 3 | 章标题词表识别承载章 → 33 篇静默绿（ds §3.4 / muse V-5） | `bash tools/doc-code-density-check.sh rewrite-notes/03-stage-rs/00-rs-overview.md` | 打印 **⛔ 承载门未过：3 个阻断项**（不再 ✅） | ✅ **已闭合**（exit 语义见 O-2） |
| 4 | `<300 行全文降级` 的降级键（ds O-1 / muse F-1） | `sed -n '44p' prompt/review-rules/review-cmds.md` | 降级键 = **该章自身 < 20 行**或显式 exempt 标记 | ✅ **已闭合** |
| 5 | 门 #9 文字比 qwen §6 尺度严、②③④ 无机器读数（muse V-9 / ds §3.3 / longcat 2.1） | 同上 | 改为**两阻断项（客观可判）+ 三读数项（人工）** | ✅ **已闭合**（残留见 O-2） |
| 6 | `P1-docStyle` 未定义（ds §4.6） | `grep -rn "P1-docStyle" prompt/review-rules/` | `review-cmds.md:44` 定义 + `review.md:545` 归入 P1/Layer 1 | ✅ **已闭合** |
| 7 | 陈旧计数：`三类审计`/`## 八条`（muse V-6/V-7） | `grep -n` 两处 SKILL | `四类审计结果` / `## 十条` | ✅ **已闭合** |
| 8 | 枚举同步缺 4 处（ds §4.2 / glm C3-a,b） | `grep -c 承载门 AGENTS.md prompt/skill/cmds/full-review/SKILL.md prompt/skill/review-excellence-skill.md .claude/rules/review-core.md` | 前三处已加；`review-core.md` 有英文行（`P1-docStyle`/`teaching-artifact`） | ✅ **已闭合**（副本侧见 O-4） |
| 9 | Gate A 语义 gap 恒真循环假绿（muse V-12/F-7、mimo G-1） | 跑一次覆盖率 | 打印 `Gaps (semantic): 0 ※ 恒 0，已废弃（循环定义）` + **`有文档无Rust (语义真信号): 2`** | ✅ **已闭合**（但见我 §四 E-2） |
| 10 | outline-first 与 Gate H 活矛盾（muse V-8 / ds §4.4 / glm C3-d） | `grep -n "新文档 outline 契约优先级" prompt/review-rules/review-process.md .claude/rules/review-process.md` | 两处都有（副本用另一措辞） | ✅ **已闭合** |
| 11 | 新 outline 无唯一落点（ds §4.5） | 同 10 的 precedence 段 | precedence 指明"以 `review-cmds §一b` 为准" | ⚠️ **部分**（落点仍是"命令层为准"，`.design` 与 `.review` 两址并存，见 O-1 备注） |
| 12 | 内部引用对账缺工具（ds P-2 / muse F-9 / mimo G-4） | `ls tools/doc-ref-check.sh && bash tools/doc-ref-check.sh --self-test` | 在位、self-test rc=0 | ✅ **已落地** |
| 13 | 负抽样 / R2 反证探针 / draft 收割 / 交付物绑定（ds P-3/P-4、glm F、mimo G-3/G-6） | `grep -n "0.13\|5.6c\|5.6d\|0.7.5\|1.5c" prompt/review-rules/review-process.md` | 五条注册表行齐（0.13 标注"人工步骤，工具化待后续"） | ✅ **已落地** |

**净结果**：v1 的 13 类载重发现里 **12 类闭合、1 类部分**。四份 v1 审计的 §四"未落地清单"（glm 9 项、muse ~12 项、longcat 10 项）**在写报告的时刻是准确的**，
但其后两轮修复已把其中多数收编——**任何拿 v1 清单当待办的人，必须先用 §七的命令重放一遍**，否则会重复劳动。

---

## 三、仍开放的问题（我实测；按危害排序）

### O-1【最高】存量口径无机制：259/369 篇命中，而"存量只报告"只是一句话

```text
$ n=0; for d in rewrite-notes/*/[0-9]*.md; do bash tools/doc-code-density-check.sh --check "$d" >/dev/null 2>&1 || n=$((n+1)); done; echo $n
259                      # 全树 369 篇，70.2%
$ grep -c "baseline\|--diff" tools/doc-code-density-check.sh
0
```

- v1（ds §3.5）在旧判据下测到 **44 篇（11.9%）**并已预警"存量无口径 → 门会死"。新判据（内容识别承载章 + 语言感知计数 + 诊断章命令）把这个数字**放大到 259 篇**：也就是说 **"干巴稿"不是例外，是本仓笔记树的主流形态**。
- 条文写"存量只报告、增量阻断"，但**没有任何机制区分二者**：`--check` 对谁跑谁退 1，全靠执行者记得"这篇是存量就用 report 模式"。这与本仓既有先例（`unsafe-audit --baseline --diff`、`doc-style-lint` 存量只报告 + `--diff` 增量阻断）不一致。
- **建议（一条判据即可验收）**：加 `--baseline <file>`（沿用 `unsafe-baseline.txt` 形态，一次性生成 259 篇清单）；
  `--check` 在基线内退 0、基线外（新增或本轮移出）退 1；再把 259 篇转成一个 backlog 批次登记（可与 `coordination/` 的 TODO 台账联动）。
  验收：`--check --baseline` 全树退 0；把其中任一篇从基线移除 → 该篇退 1。
- **顺带**：这也是新门**唯一必须先做的**——不然"增量阻断"在下一次 review 里就会撞上存量墙（我在 13 轮里已经看到同类现象：新加的检查一旦对旧文档生效，agent 的第一反应是"先确认是不是存量"）。

### O-2【中】report 模式的红色横幅 + exit 0 是新的误读面；豁免无泛滥哨

- 我实测：`00-rs-overview.md`（存量）在 **report 模式**下打印 `⛔ 承载门未过：3 个阻断项`，**exit 0**。条文确实写了"report 恒 exit 0、`--check` 才退 1"（`review-cmds.md:44`），
  但在同一屏里出现"⛔ 未过"与"退出 0"，与 v1 批评过的"同一段输出里缺码与均贴码并存"是**同一类自相矛盾**，只是换了个位置。
  建议：report 模式的横幅改为中性语（`（report 模式：仅报告；判定以 --check 为准）`），或让横幅文字随模式分支。
- 豁免：工具输出有"豁免"列（读数可见 ✓），但**没有泛滥哨**——一个 agent 可以把每个分析章都标 `<!-- doc-code-density: exempt reason=… -->` 让门永绿。
  建议：单篇豁免章数 ≥ 阻断候选章数的一半时打 `WARN 豁免泛滥`（不阻断，只让它进入 scan.md 的视野）。
- 这两条成本都很低（改文案 + 一个计数），但直接把"门假绿"的剩余空间收掉——见 §三 E-3 的元规则。

### O-3【中】三项"读数项"没有可复核协议

条文把三架构对照/调用图/源码级辨析降为"读数项（不自动阻断、交 review 逐条判定或登记豁免）"——**降级本身是对的**（客观可判性优先），
但 v1 的教训正是"没有机器读数的构件会退化成自述"。新落地的 `5.6d Gate G 负抽样`（未被 issue 命中的章节 ≥2 节）**只覆盖"漏检"，不覆盖"读数项判定是否可复核"**。
建议：读数项在被判"缺"或"有"时，scan.md 必须粘**引文**（glm C3 / ds P-5 提过的"引文栏"）——即"判定必须给证据行"，而非"我看了，有"。
成本：一行条文，零工具。

### O-4【低，需确认】Claude 端 `review-process` 副本的 Blocker Gates 表未列 #9

- 实测：`.claude/rules/review-process.md`（726 行英文常驻摘要，非全文镜像）已带上 Gate H 的 precedence 段 ✓；
  `.claude/rules/review-core.md` 有 `P1-docStyle` 英文行 ✓；但副本的 **Blocker Gates 表（第 17 行 H 行那类）没有 #9 行**。
- **需确认**：Claude 端是同时加载 `review-core.md` + `review-process.md`（则无风险），还是择一（则存在"加载了 process 却不知道 #9"的路径）。
  这需要仓库主人或 Claude 端配置方定案，单 agent 无法从仓库内容判定。

### O-5【低】"Gate 权威注册表（唯一定位表）"与"检查项注册表"的关系没写明

ds §4.1 的诉求（#9 未进"唯一定位表"）现在的结果是：**#9 进了「检查项注册表」0.11 行**——
按本仓的反膨胀元规则（"任何新检查规则一律以检查项注册表一行加入，禁止新开 Step"）这**是正确位置**，不是漏登记。
但两张表的定位差异没写：读者从"唯一定位表"出发仍看不到 #9。建议在 `review-process.md` 的 Gate 权威注册表下加一句
"通用强制门（如 #9 承载门、L 系文风门）登记在「检查项注册表」，本表只收 Blocker Gates"。

---

## 四、我的独有输入：执行侧证据（四份 v1 只读了提交，没有跑过这套流程）

### E-1 缺陷分布说明：该优先补的是"指称完整性"，不是"教学构件"

同日我在 `pm / rs / sched / vfs / ds / drivers / input / net` 八个 stage 上跑了 13 轮完整 review（产物在 `.review/zcode/`），
13 篇全部 CONVERGED，**P0=62 全修**。按类型聚合这 62 个 P0：

| 类型 | 占比（目测） | 例子 |
|---|---|---|
| **指称腐化**（锚点符号错 / 行号陈旧 / 虚构符号 / 落点错 / 正文引 `.design`） | **绝大多数** | `forkexit.c:wait_test` 指向的句子讲的是 `zombify`；`utility.c:92` 实为 144；`RS_NORESTART` 全仓 0 命中；`wait.rs` 的函数实为 `exit.rs` |
| 真实代码语义偏移 | 7 处 | `wait_test` 占位、三环表序、信号量掩码、AES 密钥、input 24/20 字节步幅… |
| 教学构件缺失 | **0** | —— |

**判断**：对一个已经"每节都有码"的文档，读者仍会被错误锚点带进沟里；而一个"少了调用图"的文档，读者多半还能读懂。
所以"效果最大化"的下一批应该把预算压在**指称类机器**上（`doc-ref-check.sh` 已落地是正确方向；
`anchor-suspect-scan.py` + C 侧基线 3548 处已落地；`1.5c` 的 `--check-filled` 已能拦"语义列没填完"）。
教学构件侧有 #9 的两项客观阻断 + 三项读数，**已够用**。

### E-2 我的 13 轮证据被已废弃指标污染，需回炉标注

我已复核：`Gaps (semantic)` 现被官方标注"**恒 0，已废弃（循环定义）**"，真信号换成"有文档无Rust"。
但**我在 13 轮里把那个恒 0 当 Gate A 证据写进了 scan.md**（如 Round 1 的 `gate-evidence-A`：「Gaps (semantic): 0」）。
也就是说：这条 bug 不只是理论问题，它已经在当日被消费成"覆盖率通过"的证据。处置：
（a）我在 `prompt/REVIEW-TRACE-rounds9-13.md` 与 `-rounds4-8.md` 的 Gate A 处补一行更正（本轮已做）；
（b）建议对 13 轮里"以 Gaps(semantic)=0 支撑 Gate A PASS"的段落做一次回炉复核（用新的"有文档无Rust"口径重跑）。

### E-3 新元规则提案：「看不见就得说没看见」守卫（建议进检查项注册表）

本批的三次事故是同一个形状：**门在"没看"的时候报"通过"**。
- 295 行原型：因为降级，"看了但没拦"（v1 C1/V-3）；
- 33 篇零承载章：因为词表，"没看就 ✅"（v1 3.4）；
- 我 13 轮里 8 条工具缺陷：首候选解析、孤儿检查空转、空证据块、硬编码比例…全是"静默通过"（`prompt/REVIEW-TRACE-rounds4-8.md` R7-D1/R6-D5/R7-D4/R8-D4/R8-D5…）。

**规则草案**（三句，适用一切新增门/工具）：
1. 输出必须打印**它检查了多少个对象**（分母），例如"承载章 5 / 全部章 7"；
2. **分母为 0 时禁止打 ✅**，改为"未识别到 X，需人工确认"并使 `--check` 退非零；
3. 横幅文字必须与判定分支一致：**不得在同一屏内同时出现"未过/缺"与"均通过"**。

成本：一次文案 + 一个计数器；收益：把本仓最容易复发的一类缺陷（从 vfs-08 的 D1 到本批的 O-2 都是它）变成可检。

---

## 五、对四份 v1 审计的元评价

| 审计 | 最强项（值得保留的判法） | 已过时 / 需重算的部分 | 结论可用性 |
|---|---|---|---|
| **ds** | 效力核查（把门当产品验收：事故原型 + 369 篇全扫 + 三类反例）；体系一致性（注册表/枚举/镜像/落点/P1-docStyle）；O-1…O-11 全部带验收判据 | §3.5 的 44 篇、§3.4 的 33 篇是**旧判据**下的数（现为 259 篇；"33 篇静默绿"已变"⛔ 3 阻断项"）；§4.4/4.5 的 Gate H 矛盾已被 precedence 段封口 | **最可用**：把它的 O 清单当模板，但每条先用 §七命令重放 |
| **glm** | C1 的"阈值与事故形态**精确重合**"（294 vs 300）是有洞察力的量化；C5"人工 +175 行未回炉"是四条里唯一指出**入库动作本身**缺取证的 | C1/C2/C4 已被二修闭合；§四"未落地 9 项"多数已收编（见 §二 #12/#13） | 裁量准，但清单需重放 |
| **muse** | V 系列 + F 系列的**结构化最完整**；F-1…F-4 的"先修哪几条"排序与我的判断一致；bagging 覆盖矩阵（~20 项需求）是四份里唯一的"覆盖面账本" | 覆盖矩阵的"未落地 12 项"写在二修之前，**现在要按 §二 重算**；英文稿，与仓库其余文档语言不一致（不影响判读） | **覆盖矩阵最有用**，但必须按现测刷新 |
| **longcat** | 工具自测与文件落地的逐项核验（1.3 表）做得干净 | **结论过宽**：2.1"规则与工具不一致"（②③④无读数）在二修后已改为"两阻断+三读数"；§四"PASS（附条件）"没做效力检验（未喂事故原型），是四份里唯一**没发现 295 行放行**的 | 作为"落地核验"读，不作"效力判定"用 |

**一处口径分歧的裁决**：muse 说"落地的 #9 文字比 qwen §6 尺度严"（缺①–④任一类 = P1），ds 说"工具范围比条文宽"（42/61 命中落在"Rust 设计决策"章）——
二修选择了**第三条路**：**两项客观可判的阻断（码/命令）+ 三项读数**，并把"承载章"从标题词表改为内容锚点识别。
**我的裁决：这是正确解**——它同时消掉了 muse 的"严而无据"与 ds 的"宽而无文"，且可机器判的部分全部落到了机器上。

---

## 六、建议（按批次；每条给 DoD）

**批次 1（本次就该做，都是小改）**
1. **O-1 存量口径**：`--baseline` + 259 篇基线 + backlog 登记。DoD：`--check --baseline` 全树退 0；移出一篇即退 1。
2. **O-2 文案与哨**：report 横幅中性化 + 豁免泛滥 WARN。DoD：report 模式输出不再含"⛔"；构造一份全章豁免的夹具 → 出 WARN。
3. **E-3 守卫三句**进检查项注册表（新 id 顺延）。DoD：新门/工具的自测里至少一条"分母为 0 不得绿"的夹具。
4. **O-5** 两表关系一句话。DoD：`grep "检查项注册表" prompt/review-rules/review-process.md` 在 Gate 权威注册表附近有交叉引用。

**批次 2（指称完整性优先，与我 13 轮的缺陷分布对齐）**
5. `doc-ref-check.sh` 接进 `full-review` 的**强制门清单**（现在只在注册表 0.12 行）。DoD：full-review 薄壳含该门 + 任一 review 制品的 `gate-evidence-ref` 块 ≥1。
6. `1.5c --check-filled` 进 Gate A 的**判定句**（不是可选）。DoD：以残留 `_待AI补充_` 非 0 的 SYMBOLS.md 判 Gate A 未闭环。
7. **E-2 回炉**：用"有文档无Rust"口径重跑 13 轮的 Gate A 结论，结论进各自 scan.md 的一次性更正段。

**批次 3（收割，可缓）**
8. v1 里仍未收编的项（muse F-11 的 i–v、O-3 的引文栏、ds O-10 的模式库条目）逐条一句话裁决。DoD：每条有"进/不进/缓 + 理由"。

**明确不建议做的**：在批次 1 之前再新增任何"教学构件"类门或工具。理由见 E-1：现场缺陷分布不支持，且 O-1 的存量墙会让新门在第一次落地时就撞墙。

---

## 七、复算命令（本报告全部读数的出处）

```bash
cd "$(git rev-parse --show-toplevel)"        # 基线 HEAD=9dbc3cca6

# ① 载重发现在当前树上的实测
git show c5d21dbfb:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md > /tmp/old295.md
bash tools/doc-code-density-check.sh --check /tmp/old295.md; echo "295原型 EXIT=$?"     # →1（v1 时为 0）
bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md; echo "现行 EXIT=$?"  # →0
bash tools/doc-code-density-check.sh rewrite-notes/03-stage-rs/00-rs-overview.md           # →⛔ 3 阻断项（report，exit0）

# ② 存量规模与基线缺口（O-1）
n=0; for d in rewrite-notes/*/[0-9]*.md; do bash tools/doc-code-density-check.sh --check "$d" >/dev/null 2>&1 || n=$((n+1)); done; echo "P1=$n"   # →259
grep -c "baseline\|--diff" tools/doc-code-density-check.sh                                  # →0

# ③ 规则与镜像
sed -n '44p' prompt/review-rules/review-cmds.md                                            # #9 全文（两阻断+三读数+降级键）
grep -c "P1-docStyle\|teaching-artifact" .claude/rules/review-core.md                       # →1
grep -n "新文档 outline 契约优先级" prompt/review-rules/review-process.md .claude/rules/review-process.md   # 两处都在
grep -n "0.11\|0.12\|1.5c\|5.6c\|5.6d\|0.7.5\|0.13" prompt/review-rules/review-process.md   # 注册表七行

# ④ 三修新增能力
bash tools/doc-ref-check.sh --self-test >/dev/null; echo "ref ST=$?"
python3 tools/coverage-extract/coverage-extract.py --check-filled .review/zcode/04-stage-pm/09-pm-exit/SYMBOLS.md 2>&1 | tail -1
python3 tools/coverage-extract/coverage-extract.py pm rewrite-notes/04-stage-pm --rust-dir os \
  --c-dir minix3/minix/servers/pm --doc-file 09-pm-exit.md \
  --semantic-map tools/coverage-extract/pm-semantic-map.json --output /tmp/g.md 2>&1 | tail -6   # 有文档无Rust=真信号

# ⑤ 四道同步门（复核时全绿）
bash tools/generate-derived-skills.sh --check && bash tools/check-review-rules.sh \
  && bash tools/lint-review-rules.sh && bash tools/diff-trae-skills.sh --only-diff
```

---

*本文件是审计报告而非笔记树正式文档：它引用 `.review/`、`tmp/` 与规则源路径属事实取证。
审计方声明：DS 单 session（同 agent 局限见卷首），关键判据已逐条给出可重放命令。*
