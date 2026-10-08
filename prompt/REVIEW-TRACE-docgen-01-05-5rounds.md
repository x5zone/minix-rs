# REVIEW-TRACE — doc-gen 五轮迭代（01-stage-kernel 01~05）

> 外层驱动 `AI-chats/doc-gen-iterate-prompt.md`，内层 `AI-chats/doc-gen-prompt.md`。每轮一节，**只写增量、含失败与返工**。追加前自检 `grep -c '^## Round'` 防复述。
> 工作树：`.wt/docgen-iter-01-qoder`（分支 `claim/DOCGEN-ITER-01-qoder`）；owner=qoder。
> 并发：另有 `.wt/p-all-05-qoder`（P-ALL-05）活跃；避让 `os/kernel`、`os/arch`、`coordination/TODO-3ARCH-PARITY*`、`todo-fix-prompt.md`。

## Round 0 · 清理既有 01（一次性）

- 现算读数：机器版 01 = 501 行；draft/01 人工版 = 2019 行（对比基线）。
- 动作：`git mv 01-boot-shim-bootstrap.md → draft/01-boot-shim-bootstrap.gen-20261008.md`；`git rm 01-todo.md`。commit `e28a3615e`。
- `.design/01-*` 与 `.review/` 系 gitignored → 不随工作树检出 → Round 1 走 **Step 0.3 就地现推大纲/design**（旧 outline 不继承，符合规程）。
- 中产决策（trace 要求写选择）：**删除**旧快照路径（工作树本就没有），非"保留当证据"。

<!-- 以下各轮 Round 1..5 由执行时逐轮追加 -->

## Round 1 · num=01（boot-shim bootstrap）

- **A 生成**：选题承上版（N01 边界，用户已确认）；1.3 资产对账收割 draft 漏收项（启动链路图/启动前状态表+认知减负 callout/诊断命令）；1.5 现推 outline.v3（含章型/代码放置/讲法资产 + 承载门预检）；正文 413 行、LF。
- **Gate H**：工作树无 .design（gitignored）→ Step 0.3 就地现推 01-design/outline/outline-review.v1 → design-coverage-check PASS。
- **E 提交**：五道文档门全绿（density --check / ref-check gawk / anchor 10/10 / doc-code-map 5/5 / style 0）+ sha256 绑定。doc commit `d9752796e`。
- **C 对比（draft 2019/102 围栏 vs new 413/30 围栏；两侧承载门均 4 可判章）**：new 更守纪律（无 draft 手工行号/过程痕迹/UEFI 附录越界）。三分类：执行退化=§6.3 命令偏薄、可复制命令欠预期输出；有意不继承=draft §4 pt_alloc/direct_map/附录（越界，归后续篇）；**规则缺口(待裁决)**见下。
- **F 回归（CodeReview 子代理，explicit_target）**：一致性≈100% PASS；锚点 10/10 + 手动补验 9 个结构/trait 锚 + 12 段节选逐行忠实 + 测试读数 minix-boot 17/boot-shim 14 精确对账；**逮到 1 个 P2**：§6.3 命令注释残留工单号 `fix27`（过程痕迹入正文）→ 已清 `b78f3780f`。
- **D 改进（本轮暴露、待用户裁决的 2 条规则缺口）**：
  1. `doc-style-lint` 只报 error 级，**不检测正文工单号/过程标记**（`fixNN`、`续-N`、`D-N①`、`首亮`、`（…现场）`）→ F 复核能兜但属人工；建议加 SL-11 窄模式 + 夹具（需权衡 FP）。
  2. 承载门只数"承载章有无码/诊断章有无命令"，**测不出"码够不够、命令带不带预期输出"**（底线#8/#9 是纯人判项、无机器读数）→ 拟加"命令块须含预期输出行"的启发式读数（先 report 不阻断）。
  - 本轮**未擅自改规范源**加门（避免五轮把规则加爆）；两条列为待裁决。

### Round 1 D 落地（用户裁决：本轮就落地两条缺口）

- `tools/doc-style-lint.sh`：加 **SL-11**（error 级，工单号/过程标记 `fixNN`/`续-N`/`首亮`/`（…现场|取证|工单|bagging）`，**含代码围栏内**）+ self-test 夹具 ticket.md。
- `tools/doc-code-density-check.sh`：加**诊断命令"须带预期输出"report 读数**（命令块有但无预期输出行 → `⚠无预期输出`，不阻断）。
- 双引擎 self-test 全绿；01 文档 error=0、Ch6 无告警。commit `fb77c6364`（rewrite 主线，工作流 infra）。四道门 PASS。

## Round 2 · num=02（higher-half）

- 生成 02 正文 229 行（kernel.lds/head.S/HigherHalf/x86 asm 节选 + 双映射图 + 三件事表 + 成对跳4况 + 三架构表 + riscv64 L2 撞槽反例 + 命令）。承载门/anchor/ref/style/codemap 全绿。commits 4dd8e7fc6/a59821985。
- **C 对比**：draft/02 1329 行/大量围栏 vs new 229/精简——new 更守纪律；差距暴露 **2 条工具盲点**（下 D）。
- **D 落地**：① anchor-resolve 0 锚点守卫（`3629a9d74`）；② doc-code-density 承载识别扩至 `.S/.h/.lds` 锚点 + `ld/lds` 计入节选码（`427d3a453`）——修"C/汇编/链接脚本分析章因锚点形态逃过承载门"。
- **F 回归（CodeReview 子代理）**：一致性≈92%，锚点/代码/事实/测试/文风 5 项零违规，逮到 **1 个真实错**：§6.3 `run_qemu.sh` 参数顺序写反（应 `<arch> <path>`）→ 已修 01、02 两篇（F 复核价值实证——机器门测不出命令语义错）。
