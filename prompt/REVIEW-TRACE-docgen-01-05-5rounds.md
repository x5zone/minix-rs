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

## Round 3 · num=03（kmain/cstart）+ GPT 教学反馈（元层，改变优化目标）

- 生成 03（169→182 行）：kmain/cstart/protect.c 节选 + 三架构保护结构表 + 威胁模型反例 + 命令。五门绿。commits 见 claim/DOCGEN-ITER-03-qoder。
- **F（CodeReview 子代理）首判 NOT PASS 75%**：逮到 §3.1 "C 用三套 #if 各写 prot_init" **事实错**（实为分目录 i386/earm，riscv 无 C 端口、无 #if）、trait 节选无签名、VBAR 归属错 → 已按真实源码修（还原 ProtectionArch/TrapEntryArch 方法面、纠正分工）。
- **GPT 元反馈（用户转达，改写了"高质量"的定义）**：旧稿=带读者读源码的教材；新稿=架构说明。**压缩压错地方**（Ch4 该有"打开代码怎么读"的实地导览被压成结论）；"本质语义/落点/钉死"抽象句密度过高=把**过程知识塌成结论知识**，OS 教材不可这样压；`BootShim`"编译器把关流程完整"是**越界**（trait 只保证接口/数据面存在，不保证 impl 调了 ExitBootServices）。真目标应是 **认知保真、不是字符精简**（机制文档 700–1000 行更合理，500 是错目标）。bagging 会**集体过拟合到"干净"**，缺一个"只能凭本文、不许调用外部 OS 常识答题的**学生型 reviewer**"。
- **D（把元反馈固化进工作流，本轮核心产出）**：`doc-gen-prompt.md`（`b4dc9fc71`）新增 **#11 反过度压缩 + 四层认知序（概念骨架→C源码导读→设计演化→当前代码实地导览）+ 第 4.6 步 学生型走查**（只凭本文答题、任一题需脑补=教学链断=P1）。目标从"过机器门+简洁"重定义为"认知保真+教材感"。
- 据 GPT 回溯修 **01**（`d6ac09a1d`）：§3.2 修 BootShim 越界句、§4.5 补回 prepare_boot→kmain 实地导览；03 同步补 §4.2 实地导览。四道同步门全绿。

## Round 4 · num=04（platform discovery）

- 生成 04（147→180 行，四层含实地导览）。领 claim/DOCGEN-ITER-04-qoder。
- 提交时逮到并修 **自己上轮引入的 doc-code-map 回归**：`sed 's/:.*//'` 误放 `expand_braces` 之前→大括号路径 `{a.rs:x,b.rs}` 被从首冒号截断误判缺失（`dbd76e0b5` 后续 fix）。主树跑 04=7/7、01=9/9、02=6/6、03=3/3。
- **首次执行第 4.6 步 学生型走查（只凭本文、禁外部知识）**——抓到 4 处教学链断点：Q6（opaque tag 的 why 外包给 01）、Q4（"语义同形状异"只断言无两实现对照）、Q7（"实地导览"只是 ASCII 地图无真实代码）、Q2（DTB/ACPI/OpenSBI 黑话未释义）。**证明学生型 reviewer 能逮到机器门/CodeReview 逮不到的"教学链断"——正是 GPT 要补的工种**。
- 全部补齐（`c4d1cfca2`）：§1.3 释义名词、§3.2 加 ApicDesc/Gicv3Desc/PlicDesc 两实现对照+downcast 真实代码、§3.3 就地讲透三条路为何选 opaque tag、§4 引 global.rs `PLATFORM.freeze(...)` 真实行。
- **D（把走查教训固化）**：doc-gen #11 加"**流程图/地图≠导览**，实地导览须内嵌真实代码行(带 path:fn)"。

## Round 5 · num=05（clock/中断）

- 生成 05（137→150 行）。领 claim/DOCGEN-ITER-05-qoder。
- **两道 review 都逮到真错**（本流程的双层价值实证）：① 学生型走查(4.6)抓内部自相矛盾——"05 从 pd 取频率"是假的(软件 tick=DEFAULT_HZ 编译期)，且 §4 实地导览把"消费 pd"那段用省略号吞了(违反刚写的 #11)；② CodeReview(F)再抓 §2.2 把 C 说成"编译期常量"(实为运行期 env_get/Rust 才编译期——**说反**)、§5 `test-clock-timer` 是**虚构测试名**(真名 test-timer-irq)、§4 import 漂、§6 register_hook。
- 全按 ground truth 修（`45b7a1dfe`→`7582e20a3`，两次 fix）：§4 补真消费 pd 的 Step2/Step3 代码、频率三处对齐、idt_init 真两行、测试名去虚构、register_hook。五门绿、test-timer-irq 三变体确认存在。

## 第 5 轮收尾 · 跨轮汇总

### 1. 五篇读数 vs draft 基线（现算，行数/承载门可判章）
| num | 新篇行 | draft行 | 新承载章 | 新篇含 |
|---|---|---|---|---|
| 01 | 435 | 2019 | 4 | 6C+6Rust节选/调用树/三架构表/诊断命令/认知减负/实地导览 |
| 02 | 229 | 1329 | 4 | kernel.lds/head.S/HigherHalf/x86 asm/双映射图/威胁式流程 |
| 03 | 182 | 1197 | 3 | kmain/cstart/protect.c/双 trait 真签名/威胁模型反例/实地导览 |
| 04 | 180 | 1255 | 3 | get_board_name/PlatformDesc/Frozen 取舍/两实现对照/实地导览真码 |
| 05 | 150 | 2263 | 1 | idt_init/init_clock/init_clock_and_interrupts 真码/两频率细分/诊断 |
新篇更短但更守纪律(无 draft 手工行号/过程痕迹/越界附录)，且每篇都补齐四层。**诚实**:仍显著短于 draft——draft 更长部分含越界展开(附录/他篇细节)，非纯教学损失；但"每篇实地导览够不够厚"仍可在后续人工回炉加粗。

### 2. 五轮累计工作流改动(标命中次数=此后是否真挡到过问题)
| 改动 | 落地 | 命中 |
|---|---|---|
| doc-style-lint SL-11 工单号检测 | Round1 `fb77c6364` | 1(拦 fix27 类) |
| density 命令预期输出读数 | Round1 | 1 |
| anchor 0 锚点守卫 | Round2 `3629a9d74` | 1(02 抽0锚暴露) |
| density 承载识别扩 .S/.h/.lds+ld/lds | Round2 `427d3a453` | 1(02 Ch2) |
| doc-code-map 抽带符号锚点 | Round3 `dbd76e0b5` | 1(03) |
| **doc-code-map expand_braces 顺序回归修** | Round4 | 自纠(自己上轮引入) |
| doc-gen #11 四层+反过度压缩+第4.6 学生型走查 | Round3 `b4dc9fc71`/Round4 收紧 | **高**(学生走查两轮逮到机器门全过的真错) |
第4.6 学生型走查命中率最高——是本五轮最有用的新增。

### 3. 遗留/待裁决/假改进(诚实，不粉饰收敛)
- **仍待裁决**：承载门只数"有无码"、测不出"码够不够/每步是否真兑现"（05 §4 曾吞码全绿=机器门盲区，靠 4.6 兜）。是否给 density 加"实地导览须含≥1 path:fn 真码行"的阻断读数——未做(需权衡 FP)。
- **假改进风险=0**：五轮新增项均有≥1 次真实命中(见上表)，无"加了没用"的门。
- **已知不完美**：03 §6.2 riscv64 L2、05 频率等多处需人工 ground-truth 才能逮——机器门+两 review 是纵深防御，非任一单点可替。五轮流程本身可稳定产出过全部门且经两 review 的稿，但"教材厚度是否已达 draft 级"最终仍需人读判。
