# D04 `04-platform-discovery` 独立 meta-review

> 评估对象是 review 工作流本身，不是这篇文档的最终质量。
> 本轮按 L6 冷启动执行：不参与设计、不参与修复、不默认主 agent 的处置正确。
>
> **Skill Invocation Log**（项目要求显式调用）：
> - `style-bible`：已通过 Skill 工具加载，用于本报告的文风与锚点纪律。
> - `review-loop`：已通过 Skill 工具加载，用于核对 L0–L7 的 meta-review 输入与输出契约。
> - 未调用 `full-review` / `code-excellence` / `test-audit` / `todo-fix`：本轮不是文档或代码修复任务。

## 0. 取证范围与方法

- 工作流版本：V2（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md` 第 12 行）。
- 语料条目：D04（`CORPUS.md` 第二节第 25 行），形态“平台发现 / 多架构”，预期探测器 P-FACT + P-READER + P-DESIGN。
- 轨迹：`runs/01-stage-kernel/04-platform-discovery/{loop,metrics,defect-ledger,follow-ups}.md`。
- 面板原始输出：`tmp/review-eval/01-stage-kernel/04-platform-discovery/panel/` 下 7 份报告（fact、design-d1、design-d2、reader-a、reader-b、reader-c、refute）。
- 本轮改动：`git diff --cached --stat` = 1 个文件、26 行新增、19 行删除；`git status --short` 只有该文档已暂存、`prompt/resources/` 未跟踪、`runs/` 未跟踪。没有代码改动。
- 复放的门：`eval-trace-check.py`（PASS 8 / WARN 2 / FAIL 0）、`anchor-resolve.sh --check`（73/73）、`doc-code-map.sh --check`（53/53）、`doc-ref-check.sh --check`（通过）、`doc-code-density-check.sh --check`（4 个可判章通过）、`doc-style-lint.sh --diff`（52 个新增行，error=0）。
- 未读：`prompt/review-rules/` 的完整规则源（除 `review-loop` 技能文本）、`.design/` 下三份设计快照全文、文档 §3.1–§3.9 与 §4.2–§4.8 的全文（只读了标题、被引段落与 diff 触碰区）、`expected/`、历史 scan。因此本报告对“这些未读章节里还有没有同类缺陷”只能给结构性判断，不能给全篇覆盖结论。

## 一、总体结论

**`FLAT`。**

- 检测面 **`IMPROVED`**：在语料标注“zcode 最终 0/0/0”的篇目里，本轮报出 4 条 P1 事实、6 条 P1 设计、1 条 P1 反证（去重后 11 条不同 P1），并且设计轴给出了可删除抽象与错误建模，不是命名级意见。
- 处置面 **`REGRESSED`**：修复率 15/88 = 17.0%，低于 D03 的 24/81 = 29.6%；转出率 83.0%，高于 D03 的 70.4%；读者项采纳 0/50 = 0%，低于 D03 的 6/46 = 13.0%。15 条 ① 里只有 10 条把发现点名的事实改对（66.7%），4 条 P1 事实修复里只有 FACT-02 一条完全干净。
- 轨迹面 **`REGRESSED`**：V2 声称晋升的两项装置（`dup-of` 合并、禁止 follow-up 大桶）在本轮都没有落地；P-REFUTE 附加表的 claim 映射整体错位，而 `eval-trace-check.py` 仍 PASS。
- 判 `FLAT` 而不是 `REGRESSED`：没有发现硬误报，检测的宽度是四轮里最大的，设计轴、反证轴与读者轴都产出了可复核的实质发现；判 `FLAT` 而不是 `IMPROVED`：处置与轨迹的退化抵消了检测收益，V2 的增量在它自己的目标指标上没有证明有效。

**运行质量：`NOT PASS`（抽样 33/88 = 37.5%，处置类一致性 23/33 = 69.7%，宽口径 28/33 = 84.8%，均低于 90%）。**

## 二、九问问卷

### 1. Detection：以前漏的现在能发现吗？哪些盲区仍漏？

**PASS。**

命中的旧盲区：

- B-01（翻译残留）：DESIGN-04 的 `PlatformContext`（单字段壳加 8 条纯转发，`os/libs/minix-platform/src/global.rs:57-95`）、DESIGN-14 的 `ConsoleDesc`（唯一方法 `as_any`，零生产消费者）、DESIGN-16 的 `pmu_cycle_counter`（全部构造点写 `false`）都是“C 形状的 Rust 复刻”，不是命名问题。
- B-02（设计死代码）：`PlatformSource` / `source()` 与 `ConsoleDesc` / `early_console()` 的零消费者由设计轴用消费者列独立核出，补上了 `trait-audit.py` 只按实现数分档的缺口（`panel/design-d2.md` 第 59 行的口径说明）。
- B-03（跨节矛盾）：FACT-05（§1.6 与 §4.4 对“扩展校验和”的相反表述）、FACT-08（§6.1 归组句与同节表格）、FACT-10（§1.3 与 §1.4 的例外）、FACT-13（§2.4 与 §2.5 对两个函数的描述）都命中了这一类。
- B-04（读者理解障碍）：三份读者报告共 50 条，READER-B-02 的“解析可以晚做”与“VM 起来后字节不再可读”的前提倒置是真实的调度论证支点问题。
- B-08（可复算读数）：FACT-01（51→53）、FACT-02（4→6）、FACT-12（815→819）、FACT-14（两处编译期断言→一处）、FACT-15（六个方法→八个）、FACT-16（FDT 不存在的定义处）全部与 grep 对得上。
- B-12（引文保真）：FACT-03（`getinfo_machine` 的“逐字引用”失实）、FACT-06（`27-28` 行号锚点）、FACT-11（注释被改写未标）都命中了这一类；FACT-17（篇头承诺 `[ARCH: K-2]`、正文零标注）属于三档术语的可追溯性，不是引文保真。
- B-13（硬件机制泄漏）：DESIGN-15 指出 `sswi_setip_base()` 与 `psci_conduit()` 两个架构专属能力挂在共享 trait 上，正是“每个实现为自己不支持的概念返回空答案”的形态；DESIGN-19 的 `PlatformDescKind` 缺 `#[repr(transparent)]` 是跨 ELF 布局保证缺口（`os/libs/minix-boot/src/platform.rs:54`）。

仍然漏掉的：

- **B-05（假反事实）没有关闭，而且有反证级证据。** §2.4 第 646 行的“去掉 `if (ret->flags & 1) break;` 会把所有 LAPIC 记录计进 `ncpus`”是错的。按文档自己引的 C 控制流（`minix3/minix/kernel/arch/i386/acpi.c:365-394`），删掉那句后循环只在 `if (!ret) break;` 退出，函数返回 `NULL`，`discover_cpus` 的 `while` 首轮条件即为假，`ncpus` 保持 0，随后走单 CPU 兜底——不是“全部计入”。READER-C-05 用文档内引文独立推出了这一点，但 P-FACT 的阴性声明写着“28 段反事实全部核对；1 段机制错误（FACT-07）”，把这一段漏了。**同一条错误同时证明了两件事：读者角色能发现事实角色漏掉的反事实，且 P-FACT 的阴性声明不可信。**
- **B-06（断链渲染）与 B-07（端口义）本轮没有载体。** 锚点门只验“能否解析到定义”，不验 CommonMark 渲染；语料 D04 的“端口义”主题也没有在正文出现，探测器没有被行使。
- **B-15（公共入口调用方枚举）本轮无法行使。** 本轮零代码改动，DESIGN-01 提议改 `Frozen::freeze` 的签名与语义，但没有枚举 `freeze` 的所有调用方与 QEMU 测试内核形态；这是下一轮 code batch 的门，不是本轮可以判 PASS 的项。

### 2. Precision：误报多少？有没有角色或类别模式？

**CONCERN。**

- 硬误报（文档对、面板错）在抽样 33 条里没有发现，`③ 0` 这个读数本身可以接受。
- 但“88 条发现”是原始计数，没有去重。跨角色重复至少有 10 组、约 12 条冗余；台账没有 `dup-of` 列，`metrics.md` 也没有“去重后缺陷数”。重复模式很清楚：**读者角色报过的跨节矛盾，事实角色也报，台账按来源各记一条，处置类还可能相反**。
  - FACT-05 与 READER-C-19 是同一句、同一位置（§1.6 第 165 行的“C 缺失的 XSDT 与扩展校验和”）：FACT-05 记 ① 已修，READER-C-19 记 ② 转出。同一处缺陷同时“已修”又“待修”。
  - READER-A-07、READER-B-08、READER-C-08 是同一处 `apic_enabled`（C 默认 0、Rust x86 恒 1），三条各自 ②。
  - READER-A-08 与 READER-C-11 是同一处 `board_name=` / `BOARDVARNAME`。
  - READER-B-01 与 READER-C-02 是同一套“四组问题表 4/5/6 行”。
  - READER-B-05 与 READER-C-22 是同一处“八项 vs 七项”。
  - READER-A-02、READER-B-06、READER-C-01 是同一组“ARM / earm / aarch64 与三种来源”的术语问题。
  - READER-A-12、READER-C-20 与反证报告 R-05 是同一处错误变体清单。
  - READER-A-14 与 DESIGN-03 是同一处“per-CPU 私有地址没有读者”。
- 严重度有系统性下压的迹象：`refute.md` 自己写 FACT-01“性质接近 P0-fact”，台账记 P1；READER-C-05 的机制错误本应是事实类修复，记成读者 P2。全轮 0 P0，但这不是“没有 P0 级问题”，而是“分类口径被压到 P1/P2”。这一点见第九问。

### 3. Design quality：是否发现可删除的抽象、翻译残留、错误建模、多余状态、不合理 trait、多余复杂度？

**PASS。**

设计轴不是命名与注释级意见，至少给出了五类实质内容：

- **可删除的抽象**：`PlatformContext`（DESIGN-04，`Frozen<PlatformDescEnum>` 即可等价替代）、`PlatformSource` / `source()`（DESIGN-13，零生产读者）、`ConsoleDesc` / `early_console()`（DESIGN-14，唯一方法是 `as_any`）。这三条都能用“删掉后外部行为不变”来验收，不是风格偏好。
- **翻译残留**：`pmu_cycle_counter: bool`（DESIGN-16，所有构造点写 `false`，`os/arch/src/arm64/arch_init.rs:61` 的分支永不执行）是 C `#ifdef` 的残余；RISC-V `ClockArch` 实例搬了 4 个字段只用 1 个（DESIGN-17），是“把描述符字段搬进实例字段”这一形式的空转。
- **错误建模**：`CpuInfo.gicr_base` / `mtimecmp_addr` 与 `Gicv3Desc` / `ClintDesc` 构成两个真源（DESIGN-03）；`nr_irqs` 在 x86 是 IDT 容量、在 riscv 是 PLIC 源数、在 aarch64 是 QEMU 默认值（DESIGN-12）；三层错误类型在 `parse_by_kind` 与 `init_from_kinfo` 两级被压平后丢弃（DESIGN-06）。
- **多余状态**：`CpuTopology.nr_cpus` 与 `cpus: [CpuInfo; 64]` 之间没有不变式（DESIGN-02），消费者一律裸下标（`os/kernel/src/lib.rs:1655`、`:1960`，`os/kernel/src/smp.rs:1131`、`:1309`）；`CpuTopology` 按值返回 2.5 KB，riscv64 每个硬件中断复制两次（DESIGN-05）。
- **不健全的类型边界**：`Frozen::freeze` 是安全方法，release 下二次冻结直接覆盖已发布的 `&'static T`；`unsafe impl<T: Send> Sync for Frozen<T>` 漏了 `T: Sync`（DESIGN-01，`os/libs/minix-types/src/types/cell.rs:213`、`:227-240`）。我逐行复核过：`debug_assert!`、无条件 `write`、`pub fn freeze` 三项事实都成立。

限制：`design-d2` 没有运行 `cargo build` / `cargo test`，所有代码结论来自逐行阅读与 `rg`；它自己也披露了。DESIGN-08 / 09 / 21 的 C 侧行为是转引自 `design-d1`，没有打开 `minix3/`。因此这三条只能当“待源码核实的设计判断”，不能当“已证实的 C 偏离”。

### 4. Independence：冷读是否产生独立发现，还是复述主 agent 已知内容？独有发现多少？

**CONCERN。**

冷读确实独立的部分：

- READER-C-05 用文档自己引的控制流推翻 §2.4 的反事实，这条 P-FACT 漏了，是真正的独立发现。
- READER-C-12 与 READER-C-13 把 §1.4 的“首失败点在 `init_clock_and_interrupts` 第一行”与 §4.1/§6.1 的“先在 `init_protection` 触发”对上了；两处都是文档内部可判的矛盾，不需要源码。
- READER-B-02 把 §1.3 的“解析可以晚做 / 只要字节还在”与同节“VM 起来后字节不再保证可读”对上，指出调度论证的前提倒置。
- `design-d2` 的 DESIGN-03 / 12 / 19 是它自己 `rg` 与解析仓库内 DTB 得出的，不是复述文档。

不够独立的部分：

- **`design-d1` 的 21 条 DECISION 不能算独立设计。** 它自述用 `grep` 列标题时看到了 §3 的小节标题，而 §3 的标题与它的决策议程几乎一一对应：§3.2“trait 做接口、enum 做存储”对 DECISION 05；§3.4“描述符的值搬进实例字段”对 DECISION 03/17；§3.6“不透明标签 + 有序切片”对 DECISION 02；§3.7“从 AssumeSyncCell 到 Frozen”对 DECISION 06/07。它还看到了篇头第 7 行的完整 Rust 文件清单。判 `DEGRADED` 是对的，见第五节。
- **`design-d2` 的 C 真值是二手的。** D2 没有打开 `minix3/`，它的 C 判断全部转引自 D1；而 D1 是 `DEGRADED` 的那一份。D2 对 DESIGN-08 / 09 / 21 都标了“需源码核实”，这个披露救了它，但“设计轴独立核对过 C”这句话在本轮不成立。
- 独有发现的规模：读者 C 的 5 条事实级/一致性级发现、读者 B 的 2 条前提倒置、设计 D2 的 4 条类型/布局缺陷（01/03/12/19），合计约 11–13 条是其他面板没有报过的。这个数字不低，但被约 12 条跨角色重复和 21 条 D1 参考项稀释了。

### 5. Scope discipline：有没有扩大责任边界？相邻问题是否被当阻塞？冻结决策是否被重提？

**PASS（有一处边界模糊，见后）。**

- `re-litigation` 0：D-001 的四章骨架没有被重提；读者 B 明确声明“本报告不涉及四章骨架重排”。D-011 的跨节一致性要求（关键主张在各处复述必须与主张—依据—适用范围一致）是本轮多条发现的判据，但它本身没有被当成新问题重提。
- `adjacent 阻塞` 0：73 条 ② 全部进了 `follow-ups.md`，没有一条被写成当前阻塞。
- 工作树纪律：面板前后只有 `prompt/resources/`（既有未跟踪）与新增 `runs/`；本轮 diff 只动一个 Markdown 文件。
- 边界模糊的一处：`loop.md §0` 的范围声明包含 `related-code.txt` 的 51 个文件，而 DESIGN-01/02/03 的 `cell.rs`、`platform.rs`、`global.rs` 都在这 51 个文件里（我逐条核过 `related-code.txt`）。本轮选择“零代码改动、三条 P1 全部转出”，理由是“代码改动不属于文档轮”（`loop.md §8` 第 2 问）。这个选择在风险上可以接受，但它与“范围包含 51 个代码文件”的声明不一致：范围既然包含它们，就不能用“不在范围”来解释不修。正确写法是“在范围内，但本轮只做文档交付，代码安全修复排入 code batch，理由是避免在一个文档提交里混入三处类型签名变更”。建议在 `loop.md §0` 把“范围”与“本轮交付物”分开写。

### 6. Teaching quality：读者角色是否发现真实理解障碍？采纳率多少？有没有纯文风噪声？

**CONCERN。**

- 真实理解障碍是有的：READER-B-02（启动窗口表自相矛盾）、READER-A-07 / B-08 / C-08（`apic_enabled` 的“保留语义”与默认配置矛盾）、READER-C-12 / C-13（首失败点与“确切名单”）、READER-C-17（`/memory` 例外在 §3.1 / §4.11 被丢掉）都是读者会卡住、且会带走错误不变量的地方；READER-C-10（低置信：`QemuVirtDesc::new()` 的拓扑可能是零骨架）还没有经过源码核实，只能算“待验证的读者卡点”。
- 采纳率 0/50 = 0%。这比 D03 的 6/46 = 13.0% 更差。原因不是“读者项都不值得修”，而是本轮把所有读者项一律 ②，没有做 ③（误报）/④（措辞）的区分，也没有把其中的事实级条目（C-05、C-12、C-13、C-19、C-20）提升为 ①。`loop.md §8` 第 3 问自己承认“本轮未逐条判 ③”。
- 纯文风噪声：抽样里没有“只改措辞”的条目；但有一组低产出的术语/计数重复：A-01、A-02、B-06、C-01 都在讲“ARM / earm / aarch64 / 三种来源”的命名，B-01 与 C-02 都在讲“四组问题 4/5/6 行”，B-05 与 C-22 都在讲“八项”。这些不是噪声，但同一处被三个人各报一次，收益被重复计数放大。
- 教学债的规模：50 条转出，`loop.md §7` 标 `teaching-debt`，但没有给出关闭计划（谁在哪个批次、按什么判据验收）。D03 的教学债也还没关。两轮叠加，未关闭的读者项可能从 40 条量级涨到 90 条量级，这是“转出即遗忘”的风险，不是“已登记即安全”。

### 7. Cost：token、agent 调用、轮次；是否收益递减？

**CONCERN。**

- 成本读数：7 个面板 agent，面板约 90 分钟，修复约 1 小时；token 未采集（`metrics.md` 第 8 行）。按 `CORPUS.md` 第五节预算“面板 5–8 个子 agent、修复轮最多 2 轮”，本轮在预算内，没有超预算说明。
- 产出读数：88 条原始发现（去重后约 76 条），15 条 ①、73 条 ②。修复轮只有 1 轮，没有 L4.5 的回读轮，没有 QEMU，`cargo check -p minix-kernel --features mock` 在 `loop.md §5` 标“待跑”。
- 收益递减的证据：D03 的 81 条发现对应 24 条 ①（29.6%），D04 的 88 条对应 15 条 ①（17.0%）。发现数上升 8.6%，修复数下降 37.5%。同时 `design-debt` 21 条、`teaching-debt` 50 条、`test-debt` 1 条全部未关闭。也就是说，本轮把更多东西“看见”并“转出”，但没有把更多东西“关掉”。
- 另一项成本没有计入：读者面板 50 条里有约 12 条与事实/设计轴重复，按“去重后缺陷”算，实际新增知识约 76 条，单位 agent 的边际产出低于表面读数。

### 8. Trace integrity：每条发现是否有处置行？指标是否自洽？

**FAIL。**

机械覆盖是过的：`eval-trace-check.py` 报 88 条全部有处置行、无孤儿行、metrics 自洽，`PASS 8 / WARN 2 / FAIL 0`；台账 88 行、① 15 行、② 73 行，算术自洽。WARN 的两条（`design-d1.md`、`refute.md` 没有 `## FINDING`）已被 `loop.md §1` 显式豁免，V2 的 `panel-exemption` 装置生效了。

但内容级轨迹有三处硬伤：

1. **P-REFUTE 附加表的 claim 映射整体错位。** 台账（`defect-ledger.md:96-100`）写：
   - “CLAIM 01 | 反证失败；3 条 P1：§1.4 例外清单、51→53、4→6”——实际上 §1.4 例外清单是 CLAIM 02 的 P1，51→53 与 4→6 是 CLAIM 06 的两条 P1。三条被记到了没产出它们的 CLAIM 01 名下。
   - “CLAIM 02 | 反证失败；P2 ×4（acpi 表头 36 字节节选、错误变体 6→13、ARM 零发现、`phys_bytes` u64）”——`refute.md` 第 14 行写 CLAIM 02 是“反证成功”，不是“反证失败”；这四条 P2 分别来自 CLAIM 03（两条）、CLAIM 05（一条）、CLAIM 06（一条）。
   - “CLAIM 03–06 | 反证失败，主张成立 | 无动作”——但 CLAIM 03 与 05 的 P2 已经被上一行转进 D04-F15，CLAIM 06 的两条 P1 已经被第一行认领。写成“无动作”等于把同一批主张的处置藏起来。
   结果是：**处置的实质（三条 P1 已修、四条 P2 转 D04-F15）是对的，但每一条 claim 的归属都错了，而轨迹门看不见。**
2. **V2 声称晋升的 `dup-of` 合并没有落地。** 台账表头只有“发现 ID / 来源角色 / 严重度 / 类别 / 处置类 / 处置 / 证据 / 复验”，没有 `dup-of`；`tools/eval-trace-check.py` 里也搜不到 `dup-of` 或 `fix-scope`。第 2 问列出的至少 10 组重复因此既没有被合并，也没有被统计。`WORKFLOW-VERSIONS.md` 第 12 行把“`dup-of` 合并”写成 V2 的已晋升内容，与实现不符。
3. **V2 的 C-23“禁止 follow-up 大桶”没有装置。** `follow-ups.md` 里 D04-F11 一行覆盖 DESIGN-09..21 共 13 条，D04-F12 / F13 / F14 各覆盖 16 / 12 / 22 条读者项；台账里 21 条 DESIGN 的处置列全部写“转出：follow-ups.md D04-F03..F11（DESIGN-0x：结构/行为设计项）”，是一个区间而不是逐条去处。C-23 是 V2 晋升的规则，但本轮的第一份 V2 轨迹就大面积违反它，且没有任何门报错。

另外两处会计问题：

- `loop.md §0` 写“关联代码 51 个文件、`doc-code-map.sh --check` 51/51”，`loop.md §5` 写“53/53”。两者都对，但前者是修复前、后者是修复后（§5.3 的分布表新增了 `vm_handoff.rs` 与 `ebs_pair.rs`，doc-code-map 随即从 51 涨到 53）。轨迹没有解释这个变化，读者会以为读数漂移。下一轮的基线要用 53，不是 51。
- `metrics.md` 的“误报 0”与“reader 转出项可能含少量措辞偏好”并列，但没有 ③/④ 计数支撑。这不是说谎，是“未测量”——`loop.md §8` 第 3 问自己承认了。

### 9. 本轮最值得但工作流仍看不见的一件事（开放题）

**严重度与分类没有外部校准，`0 P0` 是标签而不是测量。**

工作流现在能看见“有没有发现”“有没有处置行”“门是不是绿的”，看不见“这条发现的类别与严重度是不是对的”。类别由发现它的面板自报，主 agent 照抄进台账，没有任何装置拿外部标准复核。D04 有两处方向相反的例证：

- FACT-01 是“可复算命令的预期输出是假的”（文档写 51，实际 53），`refute.md` 自己说它“性质接近 P0-fact”，台账记 P1，全轮 0 P0。
- §2.4 的反事实把“删掉 `flags` 过滤”的后果写成“把所有记录计进 `ncpus`”，实际控制流返回 `NULL`、得到 0 颗 CPU；这是关于一道安全闸为什么必要的机制错误，台账记成读者 P2 并转出，没有修。

如果下一轮拿 D04 的“0 P0 / 误报 0”当基线，它会以为这篇已经干净；实际上有一个 P0 类问题被降级并留在正文。工作流需要一台校准装置：从 `CORPUS.md` 的 R01–R12 与历史 P0 判例里固定一小批“严重度已知”的样本，每轮让面板对同一批样本定级，比较面板定级与历史定级；或者规定“任何被 meta-review 认为类别有争议的发现，必须在台账里同时记面板类与复核类，并以复核类计入 metrics”。

## 三、P1 修复复核

先给结论：**4 条 P1 事实里，只有 1 条（FACT-02）完全干净，另外 3 条部分成立；REFUTE P1 的 §1.4 本体成立但跨节不完整。15 条 ① 的干净率是 10/15 = 66.7%，与 D03 的 17/24 = 70.8% 基本持平，V2 没有改善修复完整性。**

| 发现 | 台账写 | 复核 | 证据 |
|---|---|---|---|
| FACT-01（51→53） | ① 已修 | **部分成立**：53 与分布表正确（我复跑 `rg -o ... \| wc -l` 得 53，逐文件分布 10/9/6/1×7/21 也与表一致）；但 §6.2 #4 的“预期输出”在 `\| head -3` 之后仍列了 `handoff_v1_write.rs:1`、`kerninfo.rs:1` 与 `...`（`04-platform-discovery.md:2593-2600`），`head -3` 不可能再输出这两行。这条命令原本的问题就是“展示与真实行为不符”，修完仍不符。 | `sed -n '2590,2602p'`；`rg -c 'platform_sources:\s*&\[\]' os/` |
| FACT-02（4→6） | ① 已修 | **成立**：`grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs` = 6；1 条 `KernelInfo` + 5 条 bootstrap memmap 的划分与测试名一致。 | `grep -c`；`fact.md` FACT-02 |
| FACT-03（`getinfo_machine` 逐字引用） | ① 已修 | **部分成立**：标题从“逐字引用”改成了“节选（…MachineStruct 的构造已抽到 machine_record，块内为调用序列示意）”，标签本身不再宣称逐字；但块内仍然是被删掉的内联 `MachineStruct { ... }`，不是现行 `getinfo_machine` 的调用序列。现行源码 `os/kernel/src/misc.rs:1122-1143` 是 `let machine = machine_record(...)`，`apic_enabled` 的 `#[cfg]` 也在 `machine_record` 内（`:1108-1120`）。所以“块内为调用序列示意”这句话本身是假的，读者按块去找 `machine_record` 仍然找不到。 | `sed -n '2225,2255p'` 文档；`sed -n '1100,1148p' os/kernel/src/misc.rs` |
| FACT-04（Phase A.5/B.5 归 kmain） | ① 已修 | **部分成立**：§6.1 第 2573 行改对了（`kmain` 的 Phase B.5，`arch_boot_impl` 只负责 Step 0–4）；但 §7 第 2658 行仍写“`os/kernel/src/lib.rs` — `arch_boot_impl` 的 Phase A.5”。发现原文明名点了 `:2567` 与 `:2651` 两处，只改了一处。 | `rg -n 'arch_boot_impl' 04-platform-discovery.md` → 1875、2573、2658 |
| FACT-05（扩展校验和） | ① 已修 | **成立**：§1.6 改成“C 缺失 XSDT（Rust 已补）；扩展校验和两边都不校验（Rust 保持 C 的 20 字节口径）”。Rust 侧 `os/libs/minix-platform/src/acpi.rs:205-221` 确实按 `revision >= 2` 选 XSDT；C 侧只校验 20 字节。 | `sed -n '200,225p' os/libs/minix-platform/src/acpi.rs` |
| FACT-06（`27-28` 行号） | ① 已修 | **成立**：文档引文现在与 `os/libs/minix-platform/src/device_tree.rs:33-35` 的 `os/kernel/src/lib.rs:fn arch_boot_impl` 一致。 | diff 第 121-123 行 |
| FACT-07（ACPI 越界反事实） | ① 已修 | **成立**：`MAX_RSDT` 是 35，`sdt_trans[35]` 是第一个越界写，所以“从 `i=35` 开始”正确；`acpi_phys_copy` 在 `!vm_running` 时 `memcpy` 后 `return 0`（`minix3/minix/kernel/arch/i386/acpi.c:68-75`），所以“不会打印 ERROR 后退出”也正确。 | `sed -n '10,40p'`、`sed -n '66,80p'`、`sed -n '308,342p'` C 源 |
| FACT-08（§6.1 归组句） | ① 已修 | **部分成立**：归组句改成了“行 2 靠调用栈；行 3 靠 panic 文本与构建档位；行 4 与行 5 靠 panic 文本与异常发生点”，内容对了；但同一句的收尾仍写“**三类**故障的判据来源不同”（`04-platform-discovery.md:2569`），与开头的“归四类”直接冲突。这是修复时新引入的句内矛盾。 | `sed -n '2560,2572p'` |
| FACT-10（只搬地址 + `/memory` 例外） | ① 已修 | **成立（就本条而言）**：§1.3 第 87 行改成“把窗口内的平台发现动作压到最小——只搬地址……；memmap 装配所需的 `/memory` 解析是 §1.4 的例外”。但 REFUTE P1 随后在 §1.4 加了第二条例外（aarch64 UEFI 腿），§1.3 的这句话没有跟着更新，变成“只有一处例外”与 §1.4 的“两处例外”不一致。 | diff 第 13 行；`sed -n '96,106p'` |
| FACT-12（815→819） | ① 已修 | **成立**：`*addr = lapic_addr` 在 `minix3/minix/kernel/arch/i386/memory.c:819`，815 是 `else if (index == lapic_mapping_index) {`。 | `sed -n '813,821p'` |
| FACT-13（`acpi_get_ioapic_next`） | ① 已修 | **部分成立**：§2.4 改成“它没有 `for (;;)`，取一条记录、成功则 `idx++` 后返回”，与 C 源一致；但 §2.5 第 650 行仍写“`acpi_get_ioapic_next` 与 `acpi_get_lapic_next` 名字对称、实现对称”。发现原文的影响段已经点名“与 §2.5 的‘实现对称’叠加后，读者会以为两个函数都是带 `for(;;)` 的游标遍历”，修 §2.4 时没有回改 §2.5，于是新的“形状不同”与旧的“实现对称”并存。 | `rg -n '名字对称、实现对称'` → 第 650 行 |
| FACT-14（两处编译期断言→一处） | ① 已修 | **成立**：`os/kernel/src/lib.rs:1045-1049` 是唯一的 MMIO 对齐 const 断言；Step 1 的 `assert_eq!` 在 `:956-960` 是运行期断言；`os/arch/src/arm64/paging.rs:663` 是 `USER_VA_LIMIT`，`device_window.rs` 的 const 断言都在 x86/riscv 分支。 | `rg -n 'const _: \(\) = assert'` |
| FACT-15（六个方法→八个） | ① 已修 | **成立**：`PlatformDesc` 有 8 个方法，6 个必需 + `sswi_setip_base` / `psci_conduit` 两个带默认实现；§4.10 现在写“八个方法，其中六个必需 + 两个可选能力”。 | `os/libs/minix-boot/src/platform.rs:253-288`；`panel/refute.md` CLAIM 05 第 3 条 |
| FACT-16（FDT 定义处） | ① 已修 | **成立**：文档改成“kind 常量叫 `DTB`，没有 `FDT` 这个 kind 常量；`FDT` 只作为格式名出现在注释与 `fdt_layout` 的内部常量名里”，与 `os/libs/minix-boot/src/fdt_layout.rs:100-103` 一致。 | `rg -n '\bFDT\b' os/libs/minix-boot/src/fdt_layout.rs` |
| REFUTE P1（§1.4 aarch64 UEFI dtb_copy） | 已修 | **本体成立，跨节不完整**：§1.4 新增的例外与 `os/boot-shim/src/uefi_helpers.rs:269/289/291`、`cross_validate_config_dtb`（`:919-960`）对得上；`note` 的 8 字节对齐断言（`os/boot-shim/src/dtb_copy.rs:81-86`）、`preflight` 的 header-overflow `expect` 与 overlap `assert`（`:110-135`）、`authorize_and_copy` 的调用点 panic（`:289`）也都真实存在。问题有三处：(a) “5 条 panic 停机路径”这个括号跟在三个函数之后：如果读成整条链，它少算了（我数到 `note` 的 assert、`preflight` 的 `expect` 与 `assert`、两个调用点的 `unwrap_or_else` panic、`cross_validate_config_dtb` 内的 5 条，合计 10 个停机点）；如果只指 `cross_validate_config_dtb`，措辞应写明“该函数内 5 条”；(b) §3.1 第 1027 行仍写“把 boot-shim 的工作限制在把固件给的物理指针取回来”，§4.11 ② 仍写“只搬物理地址，不解析”，两处都没有 `/memory` 例外，更没有新增的 aarch64 例外（READER-C-17 已点名这两处，但被 ② 转出）；(c) §1.3 的新句只说 `/memory` 一处例外。 | `sed -n '260,295p'`、`sed -n '915,960p'` uefi_helpers；`sed -n '70,140p'` dtb_copy；文档 `:87`、`:1027`、`:2332` |

**额外两条 P1 转出裁决（见第 4 问与第六节）：**

| 发现 | 台账写 | 复核 |
|---|---|---|
| DESIGN-01（`Frozen` 二次冻结 UB） | ② D04-F03 | **转出合理**：`cell.rs:227-240` 的 `debug_assert!` + 无条件 `write` + `pub fn freeze` 三项都已核实；`unsafe impl<T: Send> Sync`（`:213`）漏 `T: Sync` 也成立。`follow-ups.md` D04-F03 给了文件 `os/libs/minix-types/src/types/cell.rs`、两条验收（二次冻结返回错误或 panic；loom/宿主测试覆盖），是三条里最完整的。 |
| DESIGN-02（`nr_cpus` 无不变式） | ② D04-F04 | **转出合理**：`platform.rs:175` / `:184` 是公开字段，`os/kernel/src/lib.rs:1655`、`:1960`、`os/kernel/src/smp.rs:1131/1309`、`os/kernel/src/irq_manager.rs:291` 全是裸下标；D04-F04 给了文件与“越界拓扑在构造期失败”这一可测验收。 |
| DESIGN-03（`CpuInfo` 双真源） | ② D04-F05 | **转出合理，验收偏弱**：`CpuInfo.gicr_base` / `mtimecmp_addr` 在 `minix-platform` 之外没有读者（我复跑 `rg` 只命中注释与 `minix-platform` 自身），D04-F05 给了文件；但验收写“拓扑体积与读者核对”，这不是可机械判定的判据，应改成 `size_of::<CpuTopology>()` 的断言或“`rg` 零读者 + 拓扑体积”两条命令。 |

**裁决：三条 P1 设计项转出是合理的**（本轮零代码改动、三条都改签名/删字段、风险不应混进一个文档提交），但要求 (a) DESIGN-09..21 不要塞进 D04-F11 大桶；(b) D04-F05 的验收改成可执行判据；(c) 下一轮 code batch 必须按 C-09 枚举 `freeze` 的调用方与 QEMU 测试内核形态。

## 四、处置抽样复判

抽样 33 条 / 88 条 = **37.5%**，覆盖 17 条事实项（全部 FACT）、6 条设计项（DESIGN-01/02/03 是 3 条 P1，DESIGN-04/09/16 是 3 条 ②）、10 条读者项。其中 **15 条 ① 全部复判（100%）**，另抽 18 条 ②，所以下面的“① 干净率”不是抽样估计，是对本轮全部 ① 的逐条判定。

判定口径：① 成立 = 发现点名的全部位置都改到，且替换句自身为真；② 成立 = 发现本身为真，转出（而非本轮修）有合理理由，去处具体；③/④/⑤ 本轮为零，不抽。

| # | 发现 | 台账 | 独立复判 | 依据 |
|---:|---|---|---|---|
| 1 | FACT-01 | ① | 部分成立 | 53 与分布表对；§6.2 #4 的 `head -3` 后仍有输出行 |
| 2 | FACT-02 | ① | 成立 | 6 条 `#[test]`，1+5 划分正确 |
| 3 | FACT-03 | ① | 部分成立 | 标签改了，块内仍是已删除的内联构造；“调用序列示意”不真 |
| 4 | FACT-04 | ① | 部分成立 | §6.1 改了，§7 第 2658 行未改 |
| 5 | FACT-05 | ① | 成立 | Rust 按 revision 选 XSDT；C 只校验 20 字节 |
| 6 | FACT-06 | ① | 成立 | 引文与 `device_tree.rs:33-35` 一致 |
| 7 | FACT-07 | ① | 成立 | `MAX_RSDT=35`；`acpi_phys_copy` 返回 0 |
| 8 | FACT-08 | ① | 部分成立 | “归四类”与“三类故障”并存 |
| 9 | FACT-09 | ① | 成立 | `&mut self` 方法不写字段（`rg` 无命中） |
| 10 | FACT-10 | ① | 成立（本条） | §1.3 已限定为“平台发现动作”；跨节问题归 REFUTE |
| 11 | FACT-11 | ② | 成立 | 注释非原文，D04-F01 有去处 |
| 12 | FACT-12 | ① | 成立 | `memory.c:819` 是 `*addr = lapic_addr` |
| 13 | FACT-13 | ① | 部分成立 | §2.4 改成“形状不同”，但 §2.5 仍写“实现对称” |
| 14 | FACT-14 | ① | 成立 | `lib.rs:1045` 一处 const 断言；Step 1 是运行期 `assert_eq!` |
| 15 | FACT-15 | ① | 成立 | 8 个方法 = 6 必需 + 2 可选 |
| 16 | FACT-16 | ① | 成立 | `fdt_layout.rs:100-103` 有 FDT 常量，文档改法正确 |
| 17 | FACT-17 | ② | 成立 | `[ARCH: K-2]` 正文零标注，D04-F02 有去处 |
| 18 | DESIGN-01 | ② | 成立 | 类型级不健全性属实，D04-F03 有文件与验收 |
| 19 | DESIGN-02 | ② | 成立 | 公开字段 + 裸下标属实，D04-F04 有验收 |
| 20 | DESIGN-03 | ② | 成立 | 零外部读者属实，D04-F05 验收偏弱 |
| 21 | DESIGN-04 | ② | 成立（类别） | `PlatformContext` 纯转发属实；但台账引“D04-F03..F11”区间，未指 D04-F06 |
| 22 | DESIGN-09 | ② | 成立（类别） | 注释“保留第一条”与代码“最后一条覆盖”矛盾属实；D04-F11 是大桶 |
| 23 | DESIGN-16 | ② | 成立（类别） | `pmu_cycle_counter` 全 false、分支永不执行；D04-F11 是大桶 |
| 24 | READER-A-01 | ② | 成立 | “C x86-64”标签未定义，教学债 |
| 25 | READER-A-07 | ② | 成立（未去重） | `apic_enabled` 矛盾属实；与 A-08/B-08/C-08 重复，无 `dup-of` |
| 26 | READER-A-15 | ② | 成立 | 26→18 的解释不可复算，事实面板已确认 26/18 两数本身正确 |
| 27 | READER-B-02 | ② | 成立 | §1.3 的表与正文前提倒置 |
| 28 | READER-C-05 | ② | **不成立** | §2.4 的反事实机制错误，属事实类，应 ①；已复核 C 控制流 |
| 29 | READER-C-12 | ② | **不成立** | §1.4 与 §4.1/§6.1 的首失败点矛盾，文档内可判，应 ① |
| 30 | READER-C-13 | ② | **不成立** | “确切名单就是这三个构造点”与下一段 `init_protection` 冲突，应 ① |
| 31 | READER-C-17 | ② | **不成立** | §3.1/§4.11 的绝对句未带 `/memory` 例外，应 ①（REFUTE P1 只修了 §1.4） |
| 32 | READER-C-19 | ② | **不成立** | 与 FACT-05 同一句同一位置；FACT-05 已 ① 修，重复项却记 ② |
| 33 | READER-C-20 | ② | 成立（未去重） | 错误变体清单不完整属实；与反证 R-05、A-12 重复 |

计数：成立 23 条（第 2、5、6、7、9、10、11、12、14、15、16、17、18、19、20、21、22、23、24、25、26、27、33 行）；部分成立 5 条（第 1、3、4、8、13 行）；不成立 5 条（第 28、29、30、31、32 行）。

**严格一致性 23/33 = 69.7%；宽口径（部分成立算类别正确）28/33 = 84.8%。两者都低于 90%，按 D-012 与本任务要求写 `NOT PASS`。**

## 五、`design-d1` 的 `DEGRADED` 是否影响 21 条 DECISION

**结论：`DEGRADED` 足够，不需要升级为作废；但它使“独立设计”这句话在本轮不成立，且 21 条 DECISION 只能当参考，不能当设计轴的独立证据。**

理由：

1. **泄漏的内容不是无关噪声。** D1 自述看到了 §3 的小节标题，而 §3 的标题与它的决策议程直接对应：§3.1“解析职责的切分”对 DECISION 01（发现发生在哪一层，交接物是什么形状）；§3.2“trait 做接口、enum 做存储”对 DECISION 05（描述符类型形状与共享接口宽度）；§3.6“不透明标签 + 有序切片”对 DECISION 02（来源选择与多来源顺序）；§3.7“从 AssumeSyncCell 到 Frozen”对 DECISION 06/07（冻结原语、冻结时机）。它还看到了篇头第 7 行的完整 Rust 文件清单。作为“独立设计”，这份产物的选题已经被设计章目录塑形。
2. **它没有污染台账。** D1 的 21 条 DECISION 没有 `## FINDING`，`loop.md §1` 已显式豁免并注明“只作参考”；台账的 21 条 DESIGN 全部来自 `design-d2`。因此 `defect-ledger.md` 的 21 条没有被 D1 直接喂入。
3. **但 D2 的 C 真值是二手的。** D2 自己写“我没有打开 `minix3/` 原始 C”，它的 C 判断转引自 D1。于是 DESIGN-08 / 09 / 21 的“C 侧行为”是一条 `D1（DEGRADED）→ D2` 的链。D2 对这三条都标了“需源码核实”，且都是 P2 / 需用户裁决，不是 ①；风险被披露，但没有被消除。
4. **D2 的独立性另有一半是成立的。** D2 对现有 Rust 的 21 条发现（Frozen、nr_cpus、CpuInfo、PlatformContext、ClockArch、ArchInit、repr、ACPI unsafe）来自它自己的逐行阅读与 `rg`，我抽样复核了 01/02/03/04/09/16，全部与源码对得上；D2 还明确拒绝了 D1 的几个方案（平台源表达“接受现状”、`PlatformDescKind` newtype、删除 `ClockArch` trait）。这说明 D2 不是 D1 的复读机。

**处置建议**：保留 D1 为“warm design / 参考”，但 (a) 不再在 `loop.md §1` 或任何汇总里把 D1 称作“独立设计”；(b) 下一轮 D1 的 brief 要明确“只给 §1–§2 正文，禁止 `grep '^#'` 或列标题”，或者干脆接受 D1 是 warm read 并把它排除出独立性统计；(c) D2 的 C 侧断言在 code batch 前必须由能读 `minix3/` 的角色复核一次。

## 六、`BLIND-SPOT-DELTA`

### 新增

- **B-23：P-REFUTE 附加表的 claim 映射不被校验。** 台账可以把 CLAIM 02 记成“反证失败”（原文是“反证成功”），可以把 CLAIM 03/05/06 的 P2 记到 CLAIM 02 名下，可以把 CLAIM 06 的两条 P1 记到 CLAIM 01 名下，而 `eval-trace-check.py` 仍 PASS。这与 B-17 不同：B-17 是“形状不被接受”，B-23 是“形状被接受、内容错位”。D04 证据见 `defect-ledger.md:96-100` 与 `panel/refute.md:11-18`。
- **B-24：严重度/分类自报没有外部校准。** 面板自报的类别与严重度被台账照抄，没有装置拿外部标准复核，于是“0 P0”可能是标签而不是测量。D04 的两处证据：`refute.md` 说 FACT-01“性质接近 P0-fact”，台账记 P1；§2.4 的反事实机制错误（READER-C-05）记成读者 P2 ②。这与 B-08（读数漂移）不同：B-08 是“数字与实测不符”，B-24 是“类别与事实不符”。

### 关闭

- 无。B-17 的“形状”部分被 V2 的 `panel-exemption` 与 P-REFUTE 附加表接住了，但“内容映射”没有，所以 B-17 只能收窄，不能关闭。

### 仍 OPEN（带 D04 证据）

| 盲区 | D04 证据 | 状态变化 |
|---|---|---|
| B-05 假反事实 | §2.4 第 646 行删掉 `flags` 过滤的反事实机制错误；P-FACT 的 28 段核对漏了它，READER-C-05 抓到 | 再次复现，未关闭 |
| B-06 断链渲染 | 锚点门 73/73，但只验解析不验渲染；本轮无渲染抽样 | 未测 |
| B-07 端口义 | 本轮无载体 | 未测 |
| B-08 可复算读数 | FACT-01/02/12/14/15/16 命中；FACT-01 修后仍留下不可复现的输出块 | 检测稳定，修复质量未改善 |
| B-09 门绿但事错 | 五门全绿，但 §2.4/§2.5 与 §1.3/§1.4 的新矛盾在门后；`eval-trace-check.py` PASS，但 P-REFUTE 映射错位、`dup-of`/大桶规则未落地 | 再次复现，未关闭 |
| B-15 公共入口调用方枚举 | 本轮零代码改动；DESIGN-01 提议改 `freeze` 但没有枚举调用方与 QEMU 形态 | 未行使 |
| B-16 材料违规分级 | D1 记 `DEGRADED` 而不是 `INVALIDATED`，处理方向对，但分级规则仍是候选 C-08，没有晋升 | 部分改善 |
| B-17 轨迹门形状 | `panel-exemption` 生效；但 P-REFUTE 附加表只查存在、不查映射 | 收窄，未关闭 |
| B-18 跨角色重复 | 至少 10 组重复；FACT-05 与 READER-C-19 同时 ①/②；`dup-of` 未实现 | 再次复现，未关闭 |
| B-19 材料矩阵/画像泄漏 | D1 第二次看到设计章标题与篇头 Rust 文件清单；`reader-profile.md` 的“教学目标”与文档篇头第 10 行同文，说明画像隔离对 D1 本来就漏 | 再次复现，未关闭 |
| B-20 修复句自身未复算 | 5/15 ① 部分成立（FACT-01/03/04/08/13）；REFUTE P1 的 §1.3/§3.1/§4.11 未跟上 | 最严重的一次复现 |
| B-21 doc 修正与被引用代码注释脱钩 | 本轮无代码注释改动，未行使 | 未测 |
| B-22 处置按来源角色分配 | 5 条读者事实级发现被判 ②；① 的分配仍按面板来源而不是修复完成度 | 再次复现，未关闭 |

## 七、建议变更

按 `WORKFLOW-VERSIONS.md` 的规则：每次最多晋升 2 条；每条必须给案例 ID、目标文件、期望效果、回归风险、替代或合并了哪个旧装置。

### 建议晋升 1：C-25（L4.5 修复复验 / `fix-scope`）

- **案例 ID**：FACT-01、FACT-03、FACT-04、FACT-08、FACT-13、REFUTE P1（D04）。
- **目标文件**：`prompt/skill/review-loop.md` 的 L3/L4；`defect-ledger.md` 的表头加 `fix-scope` 列；`tools/eval-trace-check.py` 读该列。
- **期望效果**：每条 ① 必须在台账列出发现点名的全部位置（文件 + 锚点或行号），修完逐处 `grep` 回读并把命中写进复验列；缺一处即降级为 ②。按这个口径，D04 的 FACT-01/03/04/08/13 与 REFUTE P1 会被自动降级，`① 15` 会变成 `① 10 / ② 78`，与事实更接近。
- **回归风险**：每条 ① 的记账成本上升；修复者可能把“点名位置”写少以规避。缓解：只对 P0/P1 与点名位置 ≥2 的发现强制；`fix-scope` 由发现原文的位置清单生成，不允许修复者自己缩表。
- **替代或合并的旧装置**：替代“复验 = L4 文档五门重跑”作为 ① 的唯一证据。五门只能证明锚点、引用、密度、文风，不能证明正文事实改全。

### 建议晋升 2：C-27（P-REFUTE 附加表映射校验）

- **案例 ID**：D04 的 P-REFUTE 附加表（`defect-ledger.md:96-100`）。
- **目标文件**：`tools/eval-trace-check.py`；`prompt/review-rules/review-panel.md` 的 P-REFUTE brief；`prompt/skill/review-loop.md` L2 第 5 步。
- **期望效果**：附加表每行必须写 `CLAIM nn`、该 claim 自己的反证结果（成功/失败）、以及它产出的 finding ID 或 follow-up ID；工具把 `refute.md` 的六行总表与台账逐 claim 对齐，claim 号错位、结果词矛盾、产出 ID 缺失任一情况都 FAIL。按这个口径，D04 的表会 FAIL（CLAIM 02 被写成反证失败、CLAIM 03/05/06 的 P2 被记到 CLAIM 02、CLAIM 06 的 P1 被记到 CLAIM 01）。
- **回归风险**：对自由文本表格做结构化解析容易脆；缓解：固定三列（claim、结果、产出），结果只允许“反证成功 / 反证失败”两个词，产出列只允许 ID 列表。
- **替代或合并的旧装置**：扩展 V2 的 C-20（它只要求“主张入附加表”，不校验归属），不新增第二张表。

### 必须修正的 V2 记录（不是新候选）

V2 的变更描述里有两项在本轮没有装置或没有落地，建议在 `WORKFLOW-VERSIONS.md` 里二选一：补装置，或把记录改成“仅文字规则、未装置化”。

1. **`dup-of` 合并**：`WORKFLOW-VERSIONS.md:12` 写 V2 已晋升“`dup-of` 合并”，但 `defect-ledger.md` 没有 `dup-of` 列，`tools/eval-trace-check.py` 也没有相关检查。要么在台账加列并把“同一处缺陷”的合并键写进 L2，要么把 V2 的这一句删掉。
2. **C-23 禁止 follow-up 大桶**：D04-F11 一行覆盖 13 条设计项，D04-F12/13/14 各覆盖 16/12/22 条读者项。C-23 若继续生效，需要工具侧的“单条 follow-up 最多覆盖 N 条发现、每条发现必须有具体文件与验收”检查；否则应把 C-23 标为“未生效”，不要让下一轮以为它已经在拦。

### 维护性修正（低优先级，不计入晋升）

- `prompt/skill/review-loop.md` 的 L4.3 写 `tools/doc-style-lint.sh --diff <doc>`，而工具现在把 `--diff` 的参数当 git revision，传文件路径会以 exit 2 拒绝执行（实测：`REAL_EXIT=2`）。`loop.md §5` 用的是不带参数的 `--diff`，读数正确；但技能文本会让下一个执行者踩空。改成 `tools/doc-style-lint.sh --diff`（先 `git add`）或 `tools/doc-style-lint.sh <doc>`（全量）。
- `loop.md §0` 与 §5 的关联代码数量 51→53 的变化要写一句“§5.3 新增两个夹具文件后 doc-code-map 变为 53”，否则下一轮会把 51 当权威基线。

## 八、局限声明

- **同 agent 验证 + 已重放命令。** 本报告是单 session 内的只读复核，复核者与被复核对象共享同一模型。所有结论都附了可重放的 `rg` / `sed` / `grep` / 门命令；我实际重放了 `rg -o 'platform_sources:\s*&\[\]'`（53）、`grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs`（6）、`rg -n 'arch_boot_impl'`（3 处）、`rg -n '名字对称、实现对称'`（1 处）、`sed` 读 `acpi.c` 的 `MAX_RSDT`/`acpi_phys_copy`/`acpi_get_ioapic_next`/`acpi_get_lapic_next`、`cell.rs` 的 `Frozen`、`platform.rs` 的 `CpuTopology`/`CpuInfo`、`global.rs` 的 `PlatformContext`/`PlatformDescEnum`、`uefi_helpers.rs` 的 `cross_validate_config_dtb`、`dtb_copy.rs` 的 `note`/`preflight`，以及五个文档门。没有重放的结论都在表里标了“未复核”或“转引”。
- **抽样一致性低于 90%，按规则写 `NOT PASS`。** 严格口径 23/33 = 69.7%，宽口径 28/33 = 84.8%。
- **未读范围**：`prompt/review-rules/` 的完整规则源（除 `review-loop` 技能）、`.design/` 三份快照全文、文档 §3.1–§3.9 与 §4.2–§4.8 全文、`expected/`、历史 scan。因此本报告能证明“抽样的 33 条处置里有 10 条不成立或部分成立”，不能证明“其余 55 条全部成立”；也不能证明“未读章节没有同类缺陷”。
- **C 侧行为**：§2.4 的反事实与 FACT-07 都回到 `minix3/minix/kernel/arch/i386/acpi.c` 原文核对；DESIGN-08/09/21 的 C 前提没有打开对应 C 源，沿用面板的“需源码核实”标记。
- **未运行**：`cargo build` / `cargo test` / `cargo check` / QEMU。本轮没有代码改动，代码结论只到“类型与调用关系层面”，不包含运行时行为。

## 附：给主 agent 的两条先行动作

1. **在进入 D05 之前，先做一次 30 分钟的 L2 重归因**：把 READER-C-05、C-12、C-13、C-17、C-19 改为 ①（或至少把 C-05 升为 P0-fact），把 FACT-05 与 READER-C-19 的重复合并，把 P-REFUTE 附加表的 claim 归属改正。否则下一轮会继承一个“0 P0、误报 0”的假基线。
2. **先把 5 条部分成立的 ① 补完**（FACT-01 的 §6.2 输出块、FACT-03 的块内容或标签、FACT-04 的 §7、FACT-08 的“三类”、FACT-13 的 §2.5），并把 REFUTE P1 的 §1.3、§3.1、§4.11 三处补齐。这六处都是单行或单句改动，成本远低于它们留在正文里误导下一轮的成本。
