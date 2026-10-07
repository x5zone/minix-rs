# REVIEW-TRACE — pm/09-pm-exit 全量 review 实跑轨迹与流程验证

> 2026-10-08，Qoder（claude 运行时路径）。对 `rewrite-notes/04-stage-pm/09-pm-exit.md` 执行 cmd `full-review`（scope=doc：文档+关联代码+文风+覆盖率+测试对账+修复+回归），并以此轨迹对 `prompt/review-rules/` + `.claude/rules/` 定义的 review 流程做第二次实证检验（第一次为 `REVIEW-TRACE-vfs-08-dryrun.md`）。
> 中间产物：`.review/claude/04-stage-pm/09-pm-exit/{scan,structure,SYMBOLS}.md` + `.review/claude/04-stage-pm/{STATE,VERIFY-CHECK}.md`。
> 结果：**CONVERGED**（P0=6 全修、P1=10 修 9+1 OQ、P2=8 修 5+3 backlog；回归 `cargo test -p minix-pm` 420→424+13 全绿；跨 session 独立验证 7/7 一致）。

## 一、轨迹（按执行序，含全部实跑命令）

| # | 环节 | 命令/动作 | 结果 |
|---|------|----------|------|
| 1 | 规范装载 | 读 `.claude/rules/review-core.md` + `review-process.md` + `fix-guard.md` + `review-cmds.md §二` | ⚠️ AGENTS.md 要求先读的 `CLAUDE.md` 不存在（已退役，引用未更新） |
| 2 | 选靶 | vfs 已被上轮干跑占用 → 选 pm `09-pm-exit.md`（342 行，关联 `os/servers/pm/src/exit.rs` 1294 行，design 三快照齐在） | ✅ |
| 3 | Skill 调用 | `Skill(full-review)` 成功；`Skill(review-doc-skill)` NOT_FOUND（Qoder 仅注册 6 个 cmd skill） | ⚠️ 观察 ① |
| 4 | Step 0 | `tools/review-init.sh claude …`（STATE 骨架 + 派生路径 + Gate A 命令模板）→ 4 条 `ls .design/09-*.v*.md` + `tools/design-coverage-check.sh 04-stage-pm` | ✅ 三快照齐在（Gate H.1/H.6 PASS，29 docs 全 complete） |
| 5 | Step 0 关联代码清单 | `tools/doc-code-map.sh --check` | ❌→✅ 发现缺头部 `Rust 实现` 字段 + `vm/src/vm.rs` 不存在（P1-2/P1-5，后修复，复检 3/3） |
| 6 | Step -0.5 | `cargo test -p minix-pm`（基线） | ✅ 420 passed 全绿（干净起点） |
| 7 | Gate A | `coverage-extract.py … --doc-file 09-pm-exit.md` | ❌ **被阻断**：工具 L448 起 markdown 误入 Python 源（上轮修复落地事故）→ 修工具 → `py_compile` + 重跑 ✅（SYMBOLS：语义域 36 符号，Gaps(semantic)=0） |
| 8 | Step 1.0 | `tools/anchor-resolve.sh --check` | ✅ 43/43 resolved——但人工句意复核发现 **8 处 S1 邻近伪符号锚点**（do_srv_fork/do_exit/wait_test/sigaction 挂错句子），工具 zero-def=0 抓不到 |
| 9 | Step 1 | `sed -n` 逐段实读 forkexit.c（246-469/594-806）+ mproc.h + main.c | ✅ 文档 C 引用行号全部对上；据此发现代码侧真 bug（见 #12） |
| 10 | Step 0.5 | structure.md 12 节 + 一致性矩阵 + 读者复述测试 | ✅ Gate D-6（断裂链 2 条入 Issue） |
| 11 | Step 2-3.6 | Gate B 8 字段×5 函数；Gate C 精度五项；Step 3.6 代码维度（clippy/`unsafe-audit.sh --diff`/stub 扫描） | ❌→✅ 抓出 P0-1~4（zombify 先序 / TRACE_EXIT no-op / tracer_died 级联缺失 / disinherit 超集）+ 修后 clippy 无新增、bare unsafe=0 |
| 12 | 修复（fix-guard） | 一次一条：读 ±5 → grep 现状 → 修 → grep 复验；代码 7 条 + 文档 14 条 + 工具 1 条 | ✅ 我自己引入的 2 条 clippy warning 与 1 处过程痕迹（"本轮 P0-3"入正文）也被门抓到并清除 |
| 13 | 测试回归 | `cargo test -p minix-pm`（每批修后复跑） | ✅ 420→424（+4 新回归测试）+13 集成全绿；`is_exiting()` 僵尸族语义核实排除 SIGKILL 级联误杀风险 |
| 14 | Gate E / 4.5a | 逐测试名 `rg "fn …"`；数量对账 | ❌→✅ 19/19 命中（修名后）；声称 166/176 vs 实测 424 → P2-2 重写 §5.2 |
| 15 | 文风增量门 | `tools/doc-style-lint.sh --diff` | ❌→✅ 首轮 9 error（含我触碰行的存量日期/工具提示）→ 改直陈式/移底账 → 本篇 0 命中（余 3 命中属会话前已脏文件） |
| 16 | 复检 | anchor-resolve 48/48 zero-def=0；doc-code-map 3/3；全链重跑 | ✅ |
| 17 | Gate G | `tools/verify-check.py` 首轮机械化 **FAIL**（我的 scan 未按 9 锚段精确命名 + gate-evidence 需围栏块而非标题）→ 重组 scan.md → 机检 9 锚段+8 围栏块全 PASS → 人工抽样 5/5（`git show HEAD:` 重放原文证据）→ 因 P0≥1 触发 Multi-Agent 规则：派**独立 subagent** 跨 session 重放 V1-V7 → 7/7 一致 100% | ✅ PASS（同模型家族保留意见如实记录） |
| 18 | 收尾 | STATE.md CONVERGED + `review-state-validate.py` PASS + Step 5.7 Rule Discovery（R-1/R-2/R-3）+ Step 7.1 成本评估（首轮，无停止规则触发） | ✅ |

## 二、用轨迹验证 prompt review 流程：成立的部分

1. **Step 0 硬阻断预检有效**：4 条 ls + design-coverage-check 实测可跑、结论正确（pm 29 docs 快照全齐），杜绝了"跳过预检"类模式 69。
2. **Blocker Gates 真拦人**：Gate A 拦住了坏工具（不修就无法产出 SYMBOLS）；Gate G 机械化拦住了我的 scan 格式漂移；fix-guard 的"修前读行/修后 grep"在 22 条修复中无一误伤（一次 `name_str` 编译错被即时回归捕获并修正）。
3. **Ground Truth 优先链有效**：4 个 P0 全部由"doc 声称 ↔ C 实读 ↔ 代码实读"三方对账抓出——只看 doc 或只看 code 都会漏（代码注释写着"simplified as no-op"但 doc 声称已实现，是 doc-code 断链而非注释可发现）。
4. **反"无脑一致"原则可用**：⑤签名差异按"哪个更好"判定改 doc 不改码；INIT/守卫差异分类进"设计决策/已知缺口"表而非一律消灭。
5. **回归防护闭环成立**：修前基线 420 → 每批修后复跑 → 最终 424+13 全绿 + 4 条行为修复各配回归测试——用户关心的"别把代码改坏"有实证。
6. **cmd 边界条款生效**：卓越性/文风重写未越界做（P2-7/8 按 full-review "不做边界"转 style-fix backlog）。

## 三、用轨迹验证：流程的空档（建议修订，均未擅自改规范源）

| # | 空档 | 证据（本轨迹） | 建议 |
|---|------|---------------|------|
| F-1 | **AGENTS.md 第一读指引失效**：要求"先读 CLAUDE.md"，该文件 2026-10-07 已退役删除 | `ls CLAUDE.md` → No such file | ~~待修~~ **已闭环**：磁盘版 AGENTS.md 已改为"本文件是唯一入口"；本会话新发现的两处旧引用（EXECUTION-LOG/agents-workflow-optim）已改指 `WORKFLOW-OPEN-ITEMS.md` |
| F-2 | **Gate 0 的"9 个锚段"名称只在 `verify-check.py:GATE0_ANCHORS` 硬编码**，review-process.md 未列举，且 gate-evidence 必须是围栏块（```gate-evidence-X```）而非标题——首次按规范文字写出的 scan 被机检判 FAIL | 本轨迹 #17 首轮 FAIL 实录 | review-process.md Gate 0 行内列出 9 个精确段名 + 围栏格式示例（或让 verify-check.py 报错时打印期望模板） |
| F-3 | **Step 4.5b 强制格式与 doc-style-lint SL-4 冲突**：模板要求 §5 写 `截至 YYYY-MM-DD`，SL-4 判正文日期为 error | 本轨迹 #15：改写后零命中才过门；08-worker-thread 已收敛为无日期口径 | 模板改"当前 N passed + 复核命令"（scan.md R-2） |
| F-4 | **Multi-Agent 强制规则在单工具 session 是死锁**：P0≥1 必须跨 agent 否则 Gate G FAIL，但规范未给出 subagent/跨 session 缓解路径 | 本轨迹 #17：用独立 subagent（同模型家族、零共享上下文）重放满足之 | Step 5.6 增补"跨 session 独立验证（含 subagent）为可接受缓解，须标注同模型保留意见" |
| F-5 | **工具变更无修后验证要求**：fix-guard/Step 5.5 的修后验证只覆盖 cargo/grep，`tools/` 下脚本改坏无人拦（上轮 SYMBOLS 口径说明修复把 markdown 写进了 Python） | 本轨迹 #7 Gate A 阻断 | fix-guard 增第 6 条：改 `tools/*.py|*.sh` 必跑 `--self-test`/`py_compile` + 一次真实调用 |
| F-6 | **anchor-resolve 只能证"符号存在"，不能证"句锚一致"**；`anchor-suspect-baseline.txt` 不含 09-pm-exit（迁移时基线未覆盖 pm stage），8 处伪锚全靠人工句读 | 本轨迹 #8/#16 + `grep -c 09-pm-exit tools/anchor-suspect-baseline.txt` → 0 | scan.md R-1；建议按 S1 规则把"句中符号 vs 锚点符号一致率"做进 suspect 清单生成，并补登各 stage 基线 |
| F-7 | **子 skill 强制调用条款在非 Claude 运行时不可满足**（review-core §MANDATORY Skill Invocation） | 本轨迹 #3 NOT_FOUND | 规则承认"cmd skill 实调 + 子技能文件直读"为替代合规（scan.md R-3） |
| F-8 | AGENTS.md 仍写 `.review/{tool}/{module}/` 旧 module 层，review-process.md 已改 `{stage}`（2026-10-07 迁移） | 本轨迹选靶时的路径核对 | AGENTS.md 同步 stage 语义（低风险，顺带） |

## 四、与上一轮（vfs/08 干跑）轨迹的对照

- 上轮发现（F5 anchor-resolve 假 P0、F6 pattern-gate 静默、F7 SYMBOLS 口径、F8 底账入口未入规范）均已修复**且 F7 的修复本身引入本轮回 #7 的阻断**——印证"工具变更需修后验证"（F-5）的必要性，两轮轨迹形成因果链。
- 上轮未走 Skill 实调（干跑豁免已登记），本轮 `full-review` 实调成功；上轮 scan 结构一次通过机检的路径，本轮因新写 scan 未对照精确段名而 FAIL 一轮——佐证 F-2。
- 新增发现密度：本轮 24 项（6 P0）显著高于上轮 11 项（0 P0），主因是 pm/exit 代码线首次被 full-review 实触（trace/tracer_died 属历史未深检区）。
