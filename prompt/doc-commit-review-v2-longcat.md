# doc-commit-review-v2-longcat — 工作流优化提交的第二轮复核（longcat 份）

> **复核对象（三层）**：
> ① 主提交 `6ff56f440`（教学构件承载门 #9 + 必备构件清单 + 反膨胀仲裁 + outline-first + `doc-code-density-check.sh`）；
> ② 关联提交 `7db999c35`（doc-gen 第 1.5 步）、`2d9f0b46f`（01 文档增强）、`6d91b7b16`（01 文档二修：补 `get_parameters` 贴码 / 补诊断命令 / 辨析三补锚 / 内部引用归正）；
> ③ **两轮工作流修复**：`ff3524c07`（二修：承载门工具 5 缺陷 + 规则对齐 + Gate H precedence + AGENTS/skill 同步）与 `9dbc3cca6`（三修：coverage-extract 语义 gap 恒真修复 + 新增 `doc-ref-check.sh` + 注册表七行 + 镜像 + CI）。
>
> **为什么要做 v2**：四份 v1 审计（ds/glm/longcat/muse）的载重发现，多数已被 ②③ 修掉。v2 的价值不是复述 v1，而是**把 v1 的每一条判据在当前树（HEAD=`69e847093`）上重放**，分清"已闭合 / 仍开放 / v1 未预见"，并对新引入的门做**误报面与漏报面的量化**。
>
> **证据口径**：本报告全部读数为本次实测（L1），命令集中在 §七，可逐条重放；凡属判断者标"判断"。**同 agent 局限**：单 session 审计，未做异构交叉复核；本报告与四份同辈报告一样，是 `prompt/` 下的审计产物，不是笔记树正式文档，引用 Gate 名/路径属事实取证。**日期**：2026-10-08。

---

## 一、结论先行

**二修把 v1 的决定性缺陷真正修掉了，这一点我用独立命令确认；但新判据"承载章按内容锚点识别"引入了一个 v1 未预见的、量级更大的误报面，且它恰好覆盖本仓笔记树的主流章型。** 三句话：

1. **效力级缺陷已闭合（我独立复现）**：v1 最致命的"295 行事故原型过门"已反转——同一份 `c5d21dbfb` 版本现在 `--check` 退 **1** 并点名第 2/4 章；现行 470 行退 **0**。图算码（ASCII 图不再算码）、陈旧计数（"四类审计"/"## 十条"）、`P1-docStyle` 未定义、Gate A 语义 gap 恒真、Gate H 活矛盾均已修。
2. **仍开放的最大一项不是"门不够"，而是"门认错了对象"**：新工具把"章体内出现 `.c:`/`.rs:` 锚点"等同于"该章是分析/实现章"。我按章型聚合全树的 P1 命中，**误报主体是测试章（约 124 处）与概念章（约 57 处）**——这两类章按锚点纪律**必然**引用代码路径，却本不该被要求贴 C/Rust 码。这解释了为什么全树 P1 从 v1 的 44 篇放大到 **259/369（70%）**。
3. **对用户目标的判断**：**生成端已闭环、校验端过度收紧且对象错位**。今天的流程"删掉 draft 重跑"能防住"整章零码"（长文路径），但代价是把大量合格文档判红；而真正该拦的"分析章有码但每个核心函数没贴"（v1 的 C2）仍靠人判。**下一批应修识别判据，不是再加门。**

---

## 二、v1 载重发现的重放（当前树实测）

| # | v1 发现（出处） | 我的复算 | 现测 | 判定 |
|---|---|---|---|---|
| 1 | 295 行事故原型被门放行（glm C1 / muse V-3 / ds §3.1） | `git show c5d21dbfb:…> /tmp/d.md; tools/doc-code-density-check.sh --check /tmp/d.md; echo $?` | **EXIT=1**，点名第 2/4 章 | ✅ 已闭合 |
| 2 | 图算码 / 语言无关计数（muse V-4 / ds §3.2） | 工具输出含"码/令/树/架/辨"分列；`treeonly` 夹具被拦 | 语言感知计数在位 | ✅ 已闭合 |
| 3 | `<300 行全文降级` 放走短稿（ds O-1 / muse F-1） | `sed -n '44p' prompt/review-rules/review-cmds.md` | 降级键改为"该章自身 < 20 行" | ✅ 已闭合 |
| 4 | 门文字比尺度严、②③④ 无读数（muse V-9 / ds §3.3） | `review-cmds.md:44` | 改为两阻断项 + 三读数项 | ✅ 已闭合（残留见 §三 A） |
| 5 | `P1-docStyle` 未定义（ds §4.6） | `grep -rn P1-docStyle prompt/review-rules/` | `review-cmds.md:44` 定义 + `review.md:545` 归 Layer 1 | ✅ 已闭合 |
| 6 | 陈旧计数 `三类审计`/`## 八条`（muse V-6/V-7） | `grep` 两处 SKILL | `四类审计结果` / `## 十条` | ✅ 已闭合 |
| 7 | 枚举同步缺 4 处（ds §4.2 / glm C3-a,b） | `grep -c 承载门 AGENTS.md …/full-review/SKILL.md …/review-excellence-skill.md` | 前三处已加；review-core 有英文行 | ✅ 已闭合 |
| 8 | Gate A 语义 gap 恒真循环假绿（muse V-12 / mimo G-1） | `coverage-extract.py --check-filled` | 恒 0 标"已废弃"+ 新真信号"有文档无Rust"；`--check-filled` 正反 rc=1/rc=0 | ✅ 已闭合 |
| 9 | outline-first 与 Gate H 活矛盾（muse V-8 / ds §4.4） | `grep "新文档 outline 契约优先级" review-process.md .claude/rules/review-process.md` | 两处都在，且写明"以 §一b 为准" | ✅ 已闭合 |
| 10 | 内部引用对账缺工具（ds P-2 / muse F-9） | `tools/doc-ref-check.sh --self-test` | 在位、rc=0 | ✅ 已落地（残留见 §三 C） |
| 11 | 负抽样 / R2 反证 / draft 收割 / 交付绑定（ds P-3/P-4、glm F、mimo G-3/G-6） | `grep -n "0.13\|5.6c\|5.6d\|0.7.5\|1.5c" review-process.md` | 五条注册表行齐 | ✅ 已落地（残留见 §三 E） |
| 12 | 四道同步门 | 逐条复跑 | gen/check/lint/trae 全 0 | ✅ 已闭合 |

**净结果**：v1 的 12 类载重发现里 **12 类闭合或落地**，无"说了没做"。**这一点必须说清楚**：任何拿 v1 清单当待办的人，会重复劳动——先用 §七 重放。

---

## 三、仍开放的问题（我实测；按危害排序）

### A【最高】识别判据把"提到锚点"当成"分析/实现章"，误报面覆盖主流章型

**根因**：`doc-code-density-check.sh` 的 `mech` 判据是"章体内出现 `(\.c:|\.rs:)`"。规则文本也照抄为"承载章按**内容**识别（章体内有 C/Rust 符号锚点）"。但按本仓锚点纪律，**任何**引用代码的章节（测试章、概念章、参见章）都会带这种锚点。

**我按章型聚合全树 P1 命中**（命令见 §七）：

| 命中章型 | 次数 | 是否真该贴码 |
|---|---|---|
| `实现详解`（含变体） | 118 | 真阳性 |
| `C 源码分析`（含变体） | 111 | 真阳性 |
| **`测试要点`/`测试`/`测试矩阵`** | **约 124** | **误报**（测试章列测试名，不列实现码） |
| **`概念`** | **约 57** | **误报**（概念章无码正常） |
| `Rust 设计决策` | 94 | 边界（该给类型骨架，但规则只点了"分析/实现章"） |
| `过渡`/`参见`/`附录` | 约 8 | 误报（短章，多数靠 <20 行降级漏过） |

**独立夹具**（命令见 §七）：一篇 40 行、每行引用 `kernel_info.rs:KernelInfo` 的"第 5 章 测试要点"、零围栏 → `--check` **rc=1**；一篇"错误处理设计"章带一个锚点 → 同样 **rc=1**。现行 01 文档的 Ch5（测试要点）只因碰巧不足 20 行才没被误伤。

**影响**：这是全树 P1 从 44 篇放大到 259 篇（70%）的主因（测试+概念两类约占 180/259）。它把"门该拦的干巴稿"和"合格文档"一起判红，直接触发 §三 B 的存量墙。

**建议**（一处判据改动）：`mech` 不能只靠锚点。三选一或叠加：
- (a) **排除词表**：标题含 `测试|概念|参见|附录|过渡|导航|清单` 的章不判承载（glm N1 的方案，本报告给出它覆盖的量级）；
- (b) **标题纳入**：只有标题含 `分析|实现|详解|源码|设计` 的章才是候选承载章，锚点只作"命中后是否贴码"的二次判据；
- (c) **读 outline**：以 `review-cmds.md §一b` 已强制的 outline"代码放置栏"为准逐节判定——规则互锁已造好，工具还没用上（ds O-3b 提过，此处重申）。

**DoD**：上述测试章/概念章夹具 `--check` 退 0；同时"295 行原型"仍退 1、"现行 470 行"仍退 0（三条同时成立）。

### B【高】存量口径无机制：259/369 命中，而"存量只报告"只是一句话

```
$ n=0; for d in rewrite-notes/*/[0-9]*.md; do bash tools/doc-code-density-check.sh --check "$d" >/dev/null 2>&1 || n=$((n+1)); done; echo $n
259          # 全树 369 篇，70.2%（v1 旧判据下为 44 篇）
$ grep -c "baseline\|--diff" tools/doc-code-density-check.sh
0
```

条文写"存量只报告、增量阻断"，但**没有机制区分二者**：`--check` 对谁跑谁退 1。这与本仓既有先例（`unsafe-audit --baseline --diff`、`doc-style-lint` 存量报告 + `--diff` 阻断）不一致。若不补，执行者只有两条路：成批返工（未授权）或学会绕开 `--check`（门死亡）。

**建议**：加 `--baseline <file>`（沿用 `unsafe-baseline.txt` 形态）；`--check --baseline` 基线内退 0、基线外退 1；259 篇转 backlog 登记。**DoD**：`--check --baseline` 全树退 0；移出一篇即退 1。

### C【中】`doc-ref-check.sh` 有两处缺口：假阴性 + 未接入任何门

1. **假阴性**：判据正则要求"第 N.M **章节**"或"§N.M"。实测 `见第 9.9 那套做法。`（无"节/章"字）→ **rc=0 漏过**（命令见 §七）。本仓正文常见"第 2.1 那套""第 4.1 那个"式引用（现行 01 文档第 457/462 行即此形），全部在射程外。
2. **未接入**：`grep -rn doc-ref-check prompt/skill/cmds/ AGENTS.md .claude/rules/` = **0**。工具只在注册表 0.12 行，`full-review` 强制门清单、`review-cmds` 通用门表都没有它——**规则登记了，执行入口不知道它存在**（与 v1 glm N5 同型的"规则存在、触发点不知道"）。

**建议**：判据扩到"第 N.M + 名词"式（`第 [0-9]+\.[0-9]+` 后不接数字即可），或按 ds P-2 原案补"目标节标题关键词与引用处语境词交集"；同时把它写进 `full-review` 薄壳强制门 + 要求 scan.md 出 `gate-evidence-ref` 块。**DoD**：`第 9.9 那套` 夹具退 1；full-review 薄壳含该门。

### D【中】report 模式红横幅 + exit 0 是新的误读面

```
$ bash tools/doc-code-density-check.sh rewrite-notes/02-stage-vm/02-vmproc-struct.md | tail -2
⛔ 承载门未过：3 个阻断项
$ echo $?   # → 0（report 模式）
```

同一屏里"⛔ 未过"与退出 0 并存，与 v1 批评过的"缺码与均贴码并存"是同类自相矛盾，只是换了位置。**建议**：report 模式横幅中性化（"（report 模式：仅报告；判定以 --check 为准）"），或横幅随模式分支。**DoD**：report 输出不再含"⛔"。

### E【中】`prompt/skill/review-process-skill.md` 落后 7 条新注册表行（同型问题第二轮复发）

`9dbc3cca6` 给 `review-process.md` 注册表加了 0.11/0.12/1.5c/5.6c/5.6d/0.7.5/0.13 七行，`review-process-skill.md` grep 全零（命令见 §七）。v1 glm C3-c（excellence-skill 漏同步）修掉后，同一机制缺口在另一个文件原样复发——`prompt/README.md` 说的"AI 检查 skill 是否需要同步"靠人记，两轮两次漏。

**建议**：把这条人工检查**机械化**——在 `check-review-rules.sh` 加一个对账：`review-process.md` 检查项注册表与 `review-process-skill.md` 对应表的**行 id 集合必须相等**（一行 diff 逻辑，模式与既有 L 系门一致）。**DoD**：故意删 skill 里一行 → 门红；补齐 → 门绿。

### F【低】工具死代码

`C_RUST_RE`（L24，注释自称"awk 内用 index 判定"，实际 awk 用 `islang`）与 `self_test()` 里的 `check()` 助手（L108，四个断言全用手写）均未使用。顺手清。

### G【低，需确认】64/369 篇"零承载章"静默 ✅——但观察到的群体基本无害

```
docs with 0 bearing chapters: 64 / 369
```

抽样全部落在 `00-master-plan/*`（overview/roadmap/phase-guide），这些**本就无需代码**，静默通过是正确的。所以这**不是**高危漏报；但按"看不见就得说没看见"（ds E-3）原则，工具应在"全篇识别不到承载章"时**打印分母**（"承载章 0/全部 N 章"）而非直接 ✅，以免真正的机制文档万一无锚点时被静默放过。**建议**：打印分母 + 分母为 0 时横幅改"未识别到承载章，需人工确认"。

---

## 四、对 v1 longcat 自评的更正（我 v1 漏掉了什么）

诚实起见：我 v1 报告的 **§一"PASS（附条件）"没有做效力检验**——我只跑了 `--self-test` 和现行 470 行文档，**没有把 295 行事故原型喂给工具**，因此**四份 v1 里唯一没发现"295 行放行"的就是我**（ds v2 §五也点了这一条）。这是方法缺陷：把门当"文件落地核验"读，没当"产品验收"读。v2 我补了事故原型回放（§二 #1），结论随之更正。

此外我 v1 §2.1"规则与工具不一致（②③④无读数）"的措辞，在二修后已改为"两阻断 + 三读数"，应更新为"部分闭合"；v1 §三 列的多项改进点（受审绑定、内部引用、负抽样）现已在注册表落地。**教训**：审计新门时，必须用**它声称要拦的历史缺陷原型**回测（本仓 0.9 行"修复的形态覆盖自检"正是此理），否则"全绿"只证明文件在，不证明门有牙。

---

## 五、对 ds/glm 两份 v2 的元评价

| 报告 | 我认同的 | 我的差异/补充 |
|---|---|---|
| **v2-ds** | 载重发现重放表（12 类闭合）准确；E-3"看不见就得说没看见"守卫方向对；E-1"缺陷分布里教学构件缺失为 0、指称腐化为主"有执行侧说服力 | ds 把 64 篇零承载章的风险说得偏重（其观察群体实为 master-plan，基本无害，见 §三 G）；ds 未量化误报面的章型分布，我补了 §三 A 的表 |
| **v2-glm** | N1"测试章误伤"是二修唯一会错拦合格文档的行为——判断准确；N4"同型同步缺口第二轮复发"是四份里对机制层最犀利的观察；C5 对 01 二修的复核（辨析三引真实注释、lint 0 error）扎实 | glm 的 N1 只有单个夹具，我给出**全树章型聚合**证明它不是边角而是主流（测试+概念约 180 处）；glm 未给 doc-ref-check 的假阴性，我补了 §三 C |

**一处裁决**：二修选"两项客观阻断 + 三项读数"是对 muse"严而无据"与 ds"宽而无文"的正确解——**方向正确，但识别判据（§三 A）把正确解执行歪了**：它让客观阻断落到了错误的章型上。修好识别判据后，这个三层结构（阻断/读数/降级）是值得保留的设计。

---

## 六、建议（按批次，每条给 DoD）

**批次 1（本次就该做，都是判据/文案级小改）**
1. **A 识别判据**：`mech` 增加标题排除/纳入，或改读 outline 代码放置栏。DoD：测试章/概念章夹具退 0；295 原型退 1、470 现行退 0 三条同时成立。
2. **B 存量口径**：`--baseline` + 259 篇基线 + backlog 登记。DoD：`--check --baseline` 全树退 0；移出一篇退 1。
3. **D 文案**：report 横幅中性化。DoD：report 输出不含"⛔"。
4. **E 机械化对账**：`check-review-rules.sh` 比对注册表行 id 集合。DoD：删一行 skill → 门红。

**批次 2（指称完整性优先）**
5. **C**：`doc-ref-check.sh` 扩判据 + 接入 `full-review` 强制门 + `gate-evidence-ref` 块。DoD：`第 9.9 那套` 退 1；薄壳含该门。
6. **G**：工具打印分母，分母为 0 不得直接 ✅。
7. **F**：清死代码。

**批次 3（收割）**
8. v1 里仍未收编的共识项逐条一句话裁决：复述测试产物形态（glm/muse）、新手问题预演（glm C-2）、excellence B 级收紧、读数项引文栏（ds O-3）——每条给"进/不进/缓 + 理由"。

**明确不建议**：在批次 1 修好识别判据之前，再新增任何"教学构件"类门或工具。理由同 ds E-1：现场缺陷分布不支持，且误报面会让新门一落地就撞存量墙。

---

## 七、复算命令（本报告全部读数的出处）

```bash
cd "$(git rev-parse --show-toplevel)"        # 基线 HEAD=69e847093

# ① v1 载重发现重放
git show c5d21dbfb:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md > /tmp/d.md
bash tools/doc-code-density-check.sh --check /tmp/d.md; echo "295原型 EXIT=$?"        # →1
bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md; echo "现行 EXIT=$?"  # →0
bash tools/doc-code-density-check.sh --self-test && bash tools/doc-ref-check.sh --self-test

# ② 存量规模（B）与零承载章（G）
n=0; for d in rewrite-notes/*/[0-9]*.md; do bash tools/doc-code-density-check.sh --check "$d" >/dev/null 2>&1 || n=$((n+1)); done; echo "P1=$n"   # →259
grep -c "baseline\|--diff" tools/doc-code-density-check.sh                                  # →0
z=0; for d in rewrite-notes/*/[0-9]*.md; do o=$(bash tools/doc-code-density-check.sh "$d" 2>/dev/null); nb=$(printf '%s\n' "$o" | awk -F'  +' 'NR>2&&NF>3{print $2}' | grep -c '^1$'||true); [ "$nb" = 0 ] && z=$((z+1)); done; echo "zero-bearing=$z"  # →64

# ③ 误报面：按章型聚合 P1（A）
for d in rewrite-notes/*/[0-9]*.md; do bash tools/doc-code-density-check.sh --check "$d" 2>/dev/null | grep "P1 承载缺码"; done \
  | sed -E 's/  +[0-9].*P1 承载缺码.*//' | sed 's/[[:space:]]*$//' | sort | uniq -c | sort -rn | head -20

# ④ 独立夹具（A）
{ printf '# t\n## 第 5 章 测试要点\n'; for i in $(seq 1 40); do echo "测试 os/libs/minix-boot/src/kernel_info.rs:KernelInfo 第 $i 行"; done; } > /tmp/tc.md
bash tools/doc-code-density-check.sh --check /tmp/tc.md; echo "test-chapter EXIT=$?"   # →1（误报）

# ⑤ doc-ref-check 假阴性（C）
printf '# t\n## 第 1 章 概念\n### 1.1 小节\n见第 9.9 那套做法。\n' > /tmp/rn.md
bash tools/doc-ref-check.sh --check /tmp/rn.md; echo "noun-form EXIT=$?"                # →0（漏过）
grep -rn "doc-ref-check" prompt/skill/cmds/ AGENTS.md .claude/rules/ | head             # →0（未接入）

# ⑥ report 红横幅 + exit0（D）/ 死代码（F）/ skill 未同步（E）
bash tools/doc-code-density-check.sh rewrite-notes/02-stage-vm/02-vmproc-struct.md | tail -1; echo "report EXIT=$?"  # ⛔ 但 0
grep -n "C_RUST_RE\|  check()" tools/doc-code-density-check.sh
grep -c "0\.11\|0\.12\|5\.6c\|0\.7\.5\|1\.5c" prompt/skill/review-process-skill.md        # →0

# ⑦ 四道同步门（复核时全绿）
bash tools/generate-derived-skills.sh --check && bash tools/check-review-rules.sh \
  && bash tools/lint-review-rules.sh && bash tools/diff-trae-skills.sh --only-diff
```

---

## 八、总体评价

| 维度 | 结论 |
|---|---|
| v1 载重发现是否修透 | **12/12 闭合或落地**（我逐条复现） |
| 二修正确性 | 工具自测 + 三工具 self-test + 四道同步门全绿；coverage 修正方向与实现均对 |
| 新引入问题 | **1 个高危害（A 识别判据误报主流章型）+ 1 个高危害（B 存量口径缺口）**，二者同源（误报面→存量墙） |
| 用户目标达成度 | **生成端闭环；校验端能拦"整章零码"但误报过广、漏报靠人判**——部分达成 |

**审计结论：二修让门第一次真正有牙（295 原型被拦），但牙长在了错的章型上。** 建议先把 §三 A 的识别判据与 §三 B 的存量口径修掉，再谈下一批；否则新门会在第一次全量落地时因 70% 存量命中而失去执行者的信任。**修 A+B 后，本提交可作为"生成端 + 校验端"双闭环的合格基线。**

---

*本文件是审计报告而非笔记树正式文档：它引用 `.review/`、`tmp/` 与规则源路径属事实取证。审计方声明：longcat-2.5-preview-free 单 session（同 agent 局限见卷首），关键判据已逐条给出可重放命令。*
