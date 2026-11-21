# D02 R4 独立流程评估（P-META）

- 评估对象：review 工作流，不是重审 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 的内容。
- 角色：冷启动独立流程评估者，未参与 D02 R4 面板、修复与三轮 certify；只读，唯一写入是本文件。
- 仓库根：`/home/xzhao/github/minix-rs`。下文路径除特别说明外均相对仓库根。
- 输入范围：`WORKFLOW-VERSIONS.md`、`CORPUS.md` 的 D02 行、`BLIND-SPOTS.md`、`DECISIONS.md`（重点 D-063/D-064/D-105/D-106）；`runs/01-stage-kernel/02-higher-half-kernel/` 的 `loop.md`、`defect-ledger.md`、`metrics.md`、`OPEN-DEBT.md`、`suggestions.md`、`FREEZE.md`、`certify/` 六份回执；`panel/round4/` 六份面板与 `INDEX.md`；R1/R2/R3 的 metrics 块。未读 `expected/`，未读其他篇目的面板正文。
- 方法：逐条对照台账、面板原文、最终文档与源码；抽样 29 行、覆盖 41/79 个 ID（51.9%）做独立复判，重放 `grep`、`readelf`、`cargo test --list`、`check-layout.sh` 与门工具；四个数按独立重算给出，不采信主 agent 自评。
- 判定口径：PASS = 该项在本轮有实质证据且无未被解释的反例；CONCERN = 主体成立但有会影响下一轮的缺口；FAIL = 有硬缺口，按项目规则不能以当前状态交付。

## 一、结论

| 维度 | 判定 | 一句话 |
|---|---|---|
| 总体 | **FLAT** | 检测与设计轴 IMPROVED（4 条真 P0、1 条真实链接脚本缺陷、2 条读者独有的算术/逻辑错误），但轨迹与去重轴 REGRESSED（41 个抽样 ID 的一致性 78.0%、79/79 根因、制品未入库、盲区回写是幽灵宣称）；净判 FLAT |
| 本轮运行质量 | **NOT PASS** | 抽样覆盖 41 个 ID（51.9%），其中 4 条建议与 1 个子项 C-S10(a) 的处置已过期、4 条 FP 类别错误（32/41 = 78.0%，低于 90%），且轨迹面另有硬缺口 |
| 检测面 | PASS | 抓到了 R3 之后代码移动造成的文档漂移；残余盲区见第九问 |
| 精度 | CONCERN | FINDING 级没有一条被推翻；但 `FP 4` 是类别错误，5 条 SUGG 已被别的发现修掉却仍记“登记不实施” |
| 设计质量 | PASS | 发现可删除的整层（arch `link.ld`）、孤儿节、冗余形参、零读者字段与无 ABI 的 `repr(C)`；真实链接风险已修 |
| 独立性 | PASS（隔离 CONCERN） | 冷读产生 5 个读者独有根因与 3 个设计独有根因；但 FACT 的全仓 `rg` 扫到历史面板片段，读者 B 读了验收规则而读者 A 没读 |
| 范围纪律 | CONCERN | 无相邻阻塞、无冻结决策重提；但确认轮实施了 D2-05 这个代码级设计项，D-106 与 R3 债务重复登记 |
| 教学面 | PASS | 12/12 进入修复队列的读者 FINDING（④）全修，且都属于“照做会得到相反结果”的真障碍；另 2 条读者 P3 FINDING（B-05/B-06）按建议登记；37 条建议未分诊，约四分之一是结构/措辞偏好 |
| 成本 | CONCERN | 8 个 agent、约 5 小时，处在预算上沿；token 连续第四轮未采集；一行旧题句耗了三轮 certify |
| 轨迹完整性 | **FAIL** | 79/79 的账面自洽，但语义根因、严重度分布、FP 语义、制品入库、FREEZE 追加与盲区回写均有缺口 |

## 二、九问

### 1. Detection：PASS，仍有四类盲区

以前漏、现在能发现的：

- R3 之后设计批（I-45..I-48、D-082..D-089）与 wire v10（D-062）移动了代码，而文档 sha 未变。R4 的面板抓到了这一整簇：`FACT-01/02/06`、`DESIGN-D2-01`、`READER-A-01/B-03/C-01`、`REFUTE-05` 都指向 §3.2 迭代器语义（`loop.md:396-400`）。这不是文档自己改出来的旧账，而是基线移动后新产生的漂移，属于真正的“确认轮该抓的东西”。
- 读者独立抓到两条事实/算术错误：§6.2 的 riscv64 `$rip` 比文档自己的 `delta` 公式多 `0x200000`（`panel/round4/reader-a.md` A-02、`reader-c.md` C-04），以及 §3.1 的“下表第 5 行”实际指向结论相反的第 5 行（`reader-b.md` B-01、`reader-c.md` C-02）。
- 设计 D2 抓到 `.lrodata` 孤儿节（`panel/round4/design-d2.md` DESIGN-D2-05）：x86_64 旧工件 5 个 `PT_LOAD`，其中一个是 986 个 `.lrodata.*` 节组成的独立只读段；三份生产链接脚本已为 `.ldata`/`.lbss` 做同族归并，却漏了 `.lrodata`。这是本轮最有价值的代码层发现，后续修复把 x86 `PT_LOAD` 从 5 降到 4（`os/kernel-image/x86_64.ld:63`、`aarch64.ld:62`、`riscv64.ld:68`；最终工件 `readelf -lW os/target/x86_64-unknown-none/release/kernel | grep -c '^  LOAD'` 得 4）。
- 读者 C 抓到 `push 0` 的第三种坏情形（`reader-c.md` C-06），并给出可复算的栈对齐账；这是 FACT 与 DESIGN 都没看到的。

仍然漏的：

- **可引导镜像链**：`.lrodata` 修复只验证了 `target/<arch>/release/kernel`。`os/target/image/x86_64/minix.img` 仍是 06:24 的旧镜像，里面的 `/EFI/minix/kernel.elf` 与旧 staging 工件同哈希（`26c044b151249109bea7a757b4aceb56`），5 个 `LOAD`、986 个 `.lrodata` 节；当前 release 内核是 10:42 的 4 个 `LOAD`、0 个 `.lrodata`。见第九问。
- **未编号建议**：`panel/round4/fact.md` 有 5 条未编号的 `## SUGGESTION`，`panel-index.py` 按 `## SUGGESTION <ID>` 计数，所以 INDEX 记 `fact.md` 的 SUGGESTION 为 0；这 5 条只进了 `suggestions.md`，没有台账行。B-17 的“只认固定形状”在 SUGGESTION 上复发。
- **历史材料泄漏**：`fact.md:6` 自述一次全仓 `rg` 把 `panel/round*` 与旧 review 文件的文件名/片段带进了输出（声明未采用）。`loop.md:382-394` 的 §1 材料表没有对这一处记 VARIANCE/DEGRADED，B-16/B-19 的“搜索输出也算材料面”没有落地。
- **去重**：见第八问。跨角色同根被有意保留为独立 ID，79 个“唯一根因”里有约 28 个是重复项。

### 2. Precision：CONCERN

- **FINDING 级没有真误报**：36 条 FINDING 全部被接受或登记，抽样 24 条 FINDING 的处置与我的独立判断一致（24/24）。4 条 P0（§3.2 旧语义、9→12 变体、模块“不经解析”、旧测试名）都能用源码与命令直接证实。
- **`FP 4` 是类别错误**：`metrics.md:105` 的 `FP 4` 对应 `CLAIM-01..04`；`panel/round4/refute.md` 对这四条写的是“反证失败（主张成立）”。被维持的主张不是误报，它们是“文档主干成立”的证据。真正的误报应是“报了但文档/源码证明不成立”的 FINDING，本轮为 0。把两者都叫 FP，会让下一轮无法从指标里区分“精度提高”与“反证成功”。
- **5 条 SUGG 已过期**：`READER-A-S03`（第 5 行指针）已由 `READER-B-01/C-02` 修掉；`READER-A-S06`、`READER-B-S11`、`READER-C-S10(a)`（§5.2 重复 `kern_size`）已由 `FACT-07/DESIGN-D2-10` 修掉；`READER-B-S14`（ARM“待补”）已由 `FACT-09` 修掉。台账仍把它们记成“登记不实施（读者 P3）”，`suggestions.md:21-25` 也照抄。它们不是误报，但处置行与文档现状不一致。
- **角色/类别模式**：读者 C 的 FINDING 集中在算术与跨节复述（A-02/C-04 与 C-02 都是可复算的），精度最高；读者 B 的结构项多为真问题，但 B-S03/B-S04、B-S07/B-S08 分别互相重叠；读者 A 的理解障碍项里 A-S06/A-S03 与事实/结构项重复，A-S07（0.6）与 A-S10（0.65）是低置信推断，正确留在 SUGG；设计 D2 的 10 条全部成立，其中 D2-04/05/06 是代码级；事实 12 条全部成立。
- **校验盲区**：`round4-fact.md` 自己在“观察”里记了一条 P3：§5.1 的证明列写“三类畸形”，而实际覆盖四个 `ElfError` 变体。该条没有台账行，也没有进 `suggestions.md`，是“certify 看见了但工作流没接住”的精度损失。

### 3. Design quality：PASS

本轮设计轴不是命名意见，而是可删除/可合并的结构：

- **整层死文件**：`DESIGN-D2-06` 指出三份 `os/kernel/src/arch/*/link.ld` 没有生产链接读者，唯一读者 `link_ld_lockstep.rs` 只比较两份脚本里的两个常量；三份脚本都没有 `ENTRY(_start)`、`.text.boot` 的 `KEEP`、64 KiB 引导栈与 2 MiB 收口。重放：`grep -rn 'src/arch/.*link\.ld' os --include=*.rs --include=*.toml --include=*.sh --include=Makefile* --exclude-dir=target` 只有 6 处命中，5 处注释加 1 处测试头注释；三份脚本的 `ENTRY(`、`kernel_boot_stack`、`ALIGN(0x200000)` 计数都是 0。
- **真实链接冗余**：`DESIGN-D2-05` 的 `.lrodata` 孤儿节，见第一问。它不是“更 Rust”的偏好，而是“靠链接器版本而不是脚本保证布局”的风险。
- **同型实参 + 12 处 unsafe**：`DESIGN-D2-04` 指出 `build_kernel_info` 前两个形参同为 `&'static [MemoryRegion]`，交换后仍能编译，运行时会静默地把保留区当空闲内存；两个直核面各 6 次 `core::slice::from_raw_parts` 把借用洗成 `'static`，而 `BootStorage::report()` 已经提供命名视图。重放：`sed -n '100,132p' os/boot-shim/src/lib.rs` 的签名前两项同型；`grep -c 'core::slice::from_raw_parts' os/kernel-image/src/bootface.rs` 与 `bootface_a64.rs` 都是 6；`grep -rn 'build_kernel_info(' os --include=*.rs` 得 2 个生产调用点加 3 个测试调用点。
- **冗余形参/零读者**：`DESIGN-D2-07`（`jump_to_kmain` 的 `stack_top` 与 `kinfo` 派生值同源）、`DESIGN-D2-08`（`p_align`/`LoadSegment.align`、`HIGHER_HALF_CALLED`、`virtual_end` 无读者）、`DESIGN-D2-09`（`KernelInfo`/`MemoryRegion`/`BootModule` 的 `repr(C)` 无 ABI 读者）。重放：`grep -n 'jump_to_kmain' os/kernel/src/lib.rs` 的两个生产调用都传 `kinfo.kern_stack_top()`；`grep -rn '\.align' os/libs/minix-elf/src/lib.rs` 只有测试断言；`HIGHER_HALF_CALLED` 只写不读；`virtual_end` 后跟 `let _ = virtual_end;`。
- **减法账**：`design-d2.md` 的 `## SUBTRACTION-TABLE` 有 10 行，采纳 3 行（D2-04、D2-05、D2-07），需用户裁决 1 行（D2-06），拒绝 6 行加法。`metrics.md:116` 的 `减法: 提案 10 / 采纳 3` 与表一致。这是 V3.6 的减法要求在 D02 上的首次实质落地。
- **没发现的**：本轮没有一条“翻译残留”型的 FINDING（B-01），也没有“多余 trait”型。可能是前几轮已经清掉，也可能是设计轴沿用“文档 ↔ 源码”的回退形态（D1 停跑，D-024）后不再按 C 形状扫描。记 CONCERN 级观察，不据此判 FAIL。

### 4. Independence：PASS（隔离 CONCERN）

- **独立发现的量**：读者独有且主 agent 原本没有的实质根因至少 5 个：`B-02`（Rust 侧“有没有外部装载者”的前提倒置）、`B-01/C-02`（§3.1 表格行号）、`A-02/C-04`（§6.2 `$rip` 差 2 MiB）、`C-05`（`.lrodata` 孤儿段的 LMA 继承规则）、`C-06`（只去 `push 0` 也会坏）。设计独有 3 个：`D2-04`、`D2-05`、`D2-06`。合计 8 个 FINDING 级独有根因，占 36 条 FINDING 的 22%。
- **独立复核的价值**：`REFUTE-05/06` 与 FACT/DESIGN 独立命中同一簇；`READER-A-01/B-03/C-01` 三份读者报告在没有源码的情况下都判出 §3.2 自相矛盾，说明该簇不是“主 agent 先入为主”。
- **隔离缺口一**：`fact.md:6` 的全仓 `rg` 扫到 `panel/round*` 与旧 review 文件名/片段。声明“未采用”不等于“未接触”，按 D01 的 B-16 分级应记 VARIANCE 或 DEGRADED，`loop.md:382-394` 没有记。
- **隔离缺口二**：读者 B 读了 `prompt/review-rules/review-evals.md` §4.0e（`reader-b.md:3`），读者 A 明确拒绝读（`reader-a.md:6`），读者 C 说该文件在禁读清单（`reader-c.md:219`）。同一轮三个读者角色拿到的材料不同，读者 B 的“清单条目”因此比 A/C 更精确，独立性也更高；但这是材料矩阵的不对称，不是设计好的分层。
- **模型**：六个面板与三轮 certify 都是同一模型（`opencode-go-chat/deepseek-v4.1-flash`），无跨模型复核；`loop.md:391` 已披露。这是独立性上限，不是本轮独有的缺陷。

### 5. Scope discipline：CONCERN

- **相邻问题**：`metrics.md:113` 记 `adjacent 阻塞: 0`；2 条 ② 都是 D02 内的设计项，没有把相邻模块升级为阻塞。成立。
- **冻结决策**：`metrics.md:113` 记 `re-litigation: 0`。抽读 D-062（v10 字段删除）、D-082（ELF 修剪）、D-084（SegmentIter `Err`）、D-089（buffer 两循环）与 D-064（boot-shim 安装型全局的路线 B），没有发现被重提。D-105/D-106 是新的延期裁决，不是对旧用户判决的翻案。
- **D-105/D-106 是否命中旧判决**：D-105（`build_kernel_info` 改收 `BootReport`）不命中；D-064 的主题是 U-Boot 腿的两个安装型全局与 `prepare_boot` 参数化，D-105 改的是共享 builder 与两个直核面，二者不重叠。D-106（删/补三份 arch `link.ld`）不命中任何用户判决，但它与 `OPEN-DEBT.md:1` 的 `R3-design-batch` 里的“arch link.ld 无读者”是同一项，也与 V1 `DESIGN-05` → `follow-ups.md` 的 `D02-F03` 是同一项。按台账它是 R4 的“新准入”、新根因；按内容它是第三次登记。`re-litigation: 0` 在“用户判决”口径下成立，在“债务重复登记”口径下不成立。
- **D2-05 的 `.lrodata` 修复与 D-064 类决策**：不冲突。D-064 的实质是“保留现有结构，补一致性，不为结构美学动跨腿入口签名”；`.lrodata` 归并正是把脚本已经对 `.ldata`/`.lbss` 用过的同族归并补到漏掉的兄弟节，属于“补一致性”而不是“参数化重构”。如果硬要说风险，它改了三份生产链接脚本并重建工件，属于代码行为变化，但方向与 D-064 的原则同向。
- **确认轮的范围扩张**：`panel/round4/design-d2.md` 的 DESIGN-D2-05 自己写“接受，进入下一 code batch”，主 agent 在 R4 的批一直接实施了它（`loop.md:414`、commit `cf2dc2b62`），并借此改了三个架构的工件。R4 §0 没有重述确认轮的修复权限，§3 只解释 D2-04/D2-06 为什么不实施 ⑥，没有解释 D2-05 为什么可以例外。按结果看这是正确的修复（真实链接风险），按轮型纪律看这是未说明的范围扩张。记 CONCERN，不判 FAIL。
- **D-105/D-106 的批准日期可复算性**：`DECISIONS.md:188-189` 与 `OPEN-DEBT.md:2` 都写 2026-11-21，日期一致；`D-105` 的“依用户 2026-11-21 授权”没有像 D-063 那样的指令文件或 commit 指针，只有一句自述。`defect-ledger.md:283` 还写“用户批准 2026-11-21（R36/R4 代裁授权）”，把 D01 的 R36 授权拿来给 D02 用，属于跨篇引用。日期可复算，授权来源不可独立复算。

### 6. Teaching quality：PASS

- **真实理解障碍**：进入修复队列的 12 条读者 FINDING（④）全部是“照文档做会得到相反/错误结果”的障碍，不是措辞偏好：§3.2 的 `continue`/`Err` 互斥、§3.1 的表格行号、§6.2 的 `$rip` 加法、§4.5 的“栈顶值进结构体字段”、§3.4 的 `push 0` 第三种坏情形、§4.1 的 `.lrodata` LMA 规则。读者面板另有 2 条 P3 FINDING（B-05 的计数口径、B-06 的悬空 `D02-F04/F05`）按建议登记。`round4-reader.md` 的 12 个修改点 12/12 CLOSED 与我的抽查一致（见第三节）。
- **采纳率**：读者 ④ 12/12 修（100%）；读者 FINDING 总数 14，另 2 条 P3 留建议。读者 P3 建议 37 条，正式采纳 0 条，但其中 5 条已被 FINDING 的修复顺带解决（A-S03、A-S06、B-S11、B-S14、C-S10(a)），实际命中 5/37 = 13.5%。设计轴 3 条 P3 进 `suggestions.md`，未实施。
- **纯文风噪声**：约 8–10 条属于结构/措辞偏好（B-S03/B-S04 的重复讲解、B-S06 的 §1.1 范围、B-S13 的“三处/两处”措辞、B-S15 的自指指针、C-S10(b,d,f) 的笔误与措辞），占 37 条的 22%–27%。比例不高，且都留在 P3，不阻断。
- **真正的问题**：37 条建议没有分诊规则，`suggestions.md:19` 只按角色分组，没有“下一轮必须处理 / 只登记 / 已由发现解决”的三态。下一位读者重读时最可能把这 37 条再报一遍，尤其是 B-S01（§3.1 与 §4.4 两张表）、B-S02（961 段的位置）这些结构项。

### 7. Cost：CONCERN

- **预算**：`CORPUS.md:71` 给单篇 5–8 个子 agent；本轮 8 个（6 面板 + 2 certify），在预算上沿。耗时约 5 小时（`metrics.md:117`），高于 R3 的约 4 小时。
- **token**：连续第四轮“未采集”（R1/R2/R3/R4 的 metrics 都是同一句）。B-36（收益递减无仪表）仍然 OPEN，因此无法回答“单位成本是否下降”，只能看结构。
- **收益**：4 条 P0、1 条真实链接缺陷、2 条读者独有的算术/逻辑错误，收益高于 R2（0 P0）与 R3（0 P0）。从严重度看没有递减。
- **过程成本**：§3.2 的一行旧题句耗了三轮读者 certify（`round4-reader.md` 第三轮才 12/12；`certify-r4-pass1-reader.md` 首轮 10/12）。这不是模型能力问题，而是修复批只改了题句下面的代码块、没有同时改题句（`certify-r4-pass1-fact.md:10-14`）。同类问题在 D01 R36 已经出现过（B-68），说明“同根的所有制品类别”仍未成为修复清单的强制项。
- **递减信号**：79 个 ID 里约 28 个是跨角色重复，若按语义根因计，单位根因的成本比账面高约 1.5 倍。下一轮若继续按 ID 计费，会把“重复报告”当成“新发现”。

### 8. Trace integrity：FAIL

账面自洽的部分：

- `metrics.md:99-118` 的 79 个 ID 与 `defect-ledger.md` 的 79 行一一对应；处置 `① 19 / ② 2 / ④ 12 / FP 4 / SUGG 42` 之和为 79；`未闭环: 0`；`eval-trace-check.py --round R4` 实跑 `PASS 11 / WARN 2 / FAIL 0`，覆盖、收口、metrics、open-debt 都通过。
- `panel-index.py --check panel/round4` 通过：6 个文件哈希与计数一致；`FINDING 36 / SUGGESTION 37 / CLAIM 6` 与 INDEX 一致。
- 机械门复跑一致：anchor `54/54 zero-def=0 multi-def=0`；citation `blocks=32 checked=24 failed=0 skipped=4 warned=4`；doc-code-map `30/30`；ref/style/density 全绿；`check-layout.sh all` PASS（x86 L3a=4）；测试 `minix-elf 27`、`boot-shim 135`（默认）/`146`（test-all）、`loader::tests 31`，与我重放的 `cargo test --list` 一致。
- D-105/D-106 的日期与 OPEN-DEBT 行可复算：`DECISIONS.md:188-189` 与 `OPEN-DEBT.md:2` 都是 2026-11-21；`OPEN-DEBT` 无未销账行。

不自洽/不可复算的部分：

1. **“唯一根因数 79 / 重复合并 0”是 ID 记账，不是语义去重**。R3 的同一套指标是“95 个 ID / 37 个根因”，R4 退回 1:1。最明显的重复簇：§3.2 迭代器语义 8 个 ID（FACT-01/02/06、D2-01、A-01、B-03、C-01、CLAIM-05）、测试账/载体 6 个（FACT-05/07、D2-10、A-S06、B-S11、C-S10(a)）、v1 载荷 3 个（FACT-04、D2-03、CLAIM-06）、`kern_stack_top` 字段 3 个（FACT-12、B-04、C-03）、§3.1 行号 3 个（B-01、C-02、A-S03）、其余 10 对两两重复（A-02/C-04、D2-05/C-05、FACT-03/D2-02、FACT-09/B-S14、A-S10/C-S05、A-S07/C-S08、B-06/A-S01、C-06/A-S02、B-S05/C-S06、B-S09/C-S10(e)、B-S03/B-S04）。按“每簇算一个”去重，至少 28 个 ID 是重复，语义缺陷根约 47 个（含 4 条被维持的反证主张则为 51 个主题）。
2. **严重度分布与台账不符**。`loop.md:399` 写 “P0 = 4、P1 = 4、P2 = 19、P3 = 9”；按台账逐行聚合（只算 36 条 FINDING，不含 CLAIM）是 P0 = 4、P1 = 5、P2 = 20、P3 = 7。若把两条 CLAIM-05/06 的 P1 计入，P1 = 7；若把 CLAIM-01..04 的 P3 计入，P3 = 11。无论哪种口径都对不上 4/19/9。metrics 的 R4 块没有严重度行，因此这个不一致只在 loop 里，工具没有对账。
3. **FP 4 的语义错误**：见第二问。CLAIM-01..04 是“反证失败、主张成立”，不是误报；把它们记 FP 会让 `误报` 指标失去意义。
4. **制品链没有真正入库**：`certify/round4-fact.md`、`round4-reader.md`、`certify-r4-pass1-fact.md`、`certify-r4-pass1-reader.md` 四份都是 git 未跟踪文件（`git status --porcelain` 以 `??` 列出）；`loop.md` 的 R4 §5–§8、L6、L6.5 与 `defect-ledger.md` 的“冷读回执 CLOSED”列是未提交修改；`FREEZE.md` 只追加到 R3，没有 R4 节。V3.5-b 要求 certify 回执“入库”，V3.4-m7 要求 FREEZE append；当前状态是“文件在正确路径但不在仓库历史里”，工具只查路径存在，所以仍 PASS。
5. **历史回执与最终回执只靠文件名区分，且文件名对不上**：`loop.md:423` 说“回执 `round4-fact-pass1.md` 保留”，实际文件是 `certify-r4-pass1-fact.md`；`loop.md:424` 说“pass1/pass2 保留”，目录里只有 `certify-r4-pass1-reader.md`，没有 pass2 回执；`round4-fact.md` 标题写“第二次”，`round4-reader.md` 写“第三次”，但文件名不带轮次。历史与最终的区分在内容上成立，在命名与引用上不成立。
6. **未编号建议没有台账行**：`fact.md` 的 5 条 `## SUGGESTION` 不进 INDEX（INDEX 记 0），不进 79 个 ID，只在 `suggestions.md:24` 有一段。按“每条发现都有处置行”的标准，这 5 条是孤儿；按“不阻断的建议”标准可以接受，但工具无法区分两者。
7. **BLIND-SPOTS 的回写是幽灵宣称**：`loop.md:459` 的 L6.5 写“已记 BLIND-SPOTS B-68 的扩展（设计批实施后必须重跑 stale-lines/citation，而不是只看文档 sha）”。`BLIND-SPOTS.md` 的 B-68（`:543-550`）只讲“同根修复的制品类别射程无人负责”，没有“设计批后重跑 stale-lines”这一句；文件 mtime 是 09:30:33，早于 R4 修复结束（约 11:24），也没有新增 B-69。该宣称没有对应工件。
8. **certify 自己发现但没接住的一条**：`round4-fact.md:156` 的“观察”指出 §5.1 的“三类畸形”措辞与实际四个变体不符；它没有台账行，也没有进 suggestions。这是“同一份回执里判了 18/18 CLOSED，却把一条 P3 留在判定集外”的典型案例。
9. **D-106 与 R3 债务重复**：`OPEN-DEBT.md:1` 的 `R3-design-batch` 已列“arch link.ld 无读者”，`:2` 的 `R4-design-batch` 又列 D2-06 的同一项；D2-07 也重复 V1 的 `DESIGN-03`/`follow-ups.md` 的 `D02-F01`。这不影响“是否有旧用户判决被推翻”，但影响“债务是否真的在减少”。
10. **确认轮的实施边界未记**：D2-05 是确认轮内实施的代码级设计项，`loop.md:411-419` 只写“批一”做了什么，没有写“为什么它可以越过 §3.6 的 ⑥ 不实施”。这不是伪造，是缺一行授权说明。

### 9. 本轮最值得但工作流仍看不见的一件事

**行为门验证的是“内核 ELF”，不是“可引导镜像”。**

R4 把 `.lrodata` 从孤儿节收进 `.rodata`，三份生产链接脚本都改了，`check-layout.sh all` 也 PASS（x86 `PT_LOAD` 4）。但 `check-layout.sh` 读的是 `os/target/x86_64-unknown-none/release/kernel`；`test-sysboot.sh` 构建的是另一颗测试内核 `test-sysboot`（该包目录只有 `Cargo.toml` 与 `src/main.rs`，不使用 `os/kernel-image` 的生产镜像）；`test-riscv64-uboot-shim.sh` 是 riscv。真正会被 UEFI 引导的 `os/target/image/x86_64/minix.img`（mtime 06:24）里仍然是旧内核：我从镜像里解出 `/EFI/minix/kernel.elf`，它与旧 staging 工件同哈希，5 个 `LOAD`、986 个 `.lrodata` 节；当前 release 内核是 4 个 `LOAD`、0 个 `.lrodata`。也就是说，本轮最真实的那条代码修复，在“可引导镜像”这一层没有被重新装配，也没有任何门把“镜像里的 kernel 哈希 == 当前 release kernel”作为断言。若有人用 `SKIP_BUILD=1` 跑 `test-e5-ds.sh`/`test-shim-bootmarks-x86.sh`，或者把 `minix.img` 直接拿去引导，得到的仍是旧布局。

这不是文档缺陷，也不是本轮面板的错；它是工作流的门边界：`check-layout`、`readelf`、`test-sysboot` 都各自成立，但没有一条把“生产链接脚本 → 内核 ELF → xtask 装机 → minix.img → boot-shim 读取”串成一条可判的链。建议把这条链做成一个门（见第六节第 3 条）。

## 三、抽样复判（29 行 / 41 个 ID，51.9%）

抽样覆盖 41 个 ID（51.9%）：FACT 12、DESIGN 7、读者 FINDING 11、读者建议 5（4 个编号加 `C-S10` 的子项 a）、CLAIM 6。表按台账行组织，29 行里有 7 行是多 ID 合并。命令都在本会话重放；`cargo test --list` 用 `CARGO_TARGET_DIR=/tmp/d02-meta-target`，旧镜像解包用 `/tmp/d02-kernel-from-img.elf`，都没有写仓库 `target/`。

| # | 发现 | 台账类 | 独立判定 | 重放证据 |
|---:|---|---|---|---|
| 1 | FACT-01 | ① | 成立 | `grep -n '非 \`PT_LOAD\` 类型' doc` 命中文档 `:458/:503`；`os/libs/minix-elf/src/lib.rs:399/403/410/421/424/432` 六个 `Some(Err)`，唯一 `continue` 在 `:414` |
| 2 | FACT-02 | ① | 成立 | `sed -n '157,198p' os/libs/minix-elf/src/lib.rs` 数出 12 个变体；`grep -n 'InvalidSegment' os/boot-shim/src/loader.rs` 的生产返回点是 `:196/:208/:353` |
| 3 | FACT-03 | ① | 成立 | `os/boot-shim/src/loader.rs:227` 的 `elf_file_end` 调 `segment_iter`，`:517` 修剪；`cargo test -p boot-shim --lib -- --list` 得 135，`--features test-all` 得 146 |
| 4 | FACT-04 | ① | 成立 | `grep -n 'byte_len' os/libs/minix-boot/src/handoff_v1.rs` 只有六个描述符；`grep -n 'decode_' os/kernel/src/lib.rs` 六个解码；文档 `:366` 六块 |
| 5 | FACT-05 | ① | 成立 | `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` = 27；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` = 31；`cargo test --list` 得 27/135/146 |
| 6 | FACT-06 | ① | 成立 | 文档 `:1120` 有 `reports_segment_data_out_of_bounds`；`grep -c 'skips_segment_data_out_of_bounds' doc` = 0 |
| 7 | FACT-07 | ① | 成立 | 文档 `:1152` 是 `kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`；载体源码 `:54-55` 同形；旧字面量 0 命中 |
| 8 | FACT-08 | ① | 成立 | `grep -n 'BUMP_NEXT' os/kernel-image/src/bootface_a64.rs` 命中 `:119`；文档 `:886` 已改；`BUMP_PTR` 文档 0 |
| 9 | FACT-09 | ① | 成立 | `earm/Makefile.inc:61-64` 名单无 `mpx.o`；`earm/mpx.S:265-272` 切回 `.data`；`earm/kernel.lds:18` 只收 `unpaged_*.o`；文档 `:324` 已写结论 |
| 10 | FACT-10/11 | ① | 成立 | `mpx.S` 651 行、`k_boot_stktop` 641、其后 10 行；`riscv64/higher_half.rs:42-51` 是 direct leg，52 是 resume leg；`store_kernel_info` 的 `reserved_regions` 句在函数体 `:1588` |
| 11 | FACT-12 | ① | 成立 | `grep -n 'kinfo.kern_stack_top' doc` 只命中 `:582/:586`，均为带括号访问器；`grep -c 'kinfo.kern_stack_top\`' doc` = 0；`kernel_info.rs:200` 是访问器 |
| 12 | DESIGN-D2-01 | ① | 成立 | 同 #1；D2-01 是同一簇的设计侧表述 |
| 13 | DESIGN-D2-02/03 | ① | 成立 | 同 #3/#4 |
| 14 | DESIGN-D2-04 | ② | 成立 | `build_kernel_info` 前两参同型；两个 bootface 各 6 次 `from_raw_parts`；5 个调用点；D-105 延期 |
| 15 | DESIGN-D2-05 | ① | 成立 | 当前 x86 工件 4 个 `LOAD`、0 个 `.lrodata`；旧 staging 5 个 `LOAD`、986 个 `.lrodata`；三份脚本 `:63/:62/:68` 都收 `.lrodata`；`check-layout.sh all` PASS |
| 16 | DESIGN-D2-06 | ② | 成立，但债务重复 | 6 处 `src/arch/.*link.ld` 命中都是注释/测试；三份脚本 `ENTRY(`/`kernel_boot_stack`/`ALIGN(0x200000)` 计数为 0；与 `OPEN-DEBT.md:1` 的 R3 项重复 |
| 17 | DESIGN-D2-10 | ① | 成立 | 27/31/135 实测；文档 `:526` 已写 wire 三标量；载体表达式逐字同形 |
| 18 | READER-A-01 | ④ | 成立 | 同 #1；文档 `:458` 与 `:503` 已同口径 |
| 19 | READER-A-02 | ④ | 成立 | `printf '0x%x' $((0x80200160 + 0xffffffbf7fe00000))` = `0xffffffc000000160`；文档 `:1253` 同值；旧值 0 命中 |
| 20 | READER-B-01 / C-02 | ④ | 成立 | 文档 `:366` 改为“下表第 3 行”；表数据第 3 行是 riscv64 测试载体；`grep -c '下表第 5 行' doc` = 0 |
| 21 | READER-B-02 | ④ | 成立 | 文档 `:408` 已改为“把 C 内联在引导器里的这段解析搬到独立 boot-shim”，并写明“差别不在有没有外部装载者” |
| 22 | READER-B-04 / C-03 | ④ | 成立 | 文档 `:1103` 是“栈顶值进派生访问器”；`grep -c '栈顶值进结构体字段' doc` = 0 |
| 23 | READER-C-04 | ④ | 成立 | 同 #19；旧 `0xffffffc000200160` 0 命中 |
| 24 | READER-C-05 | ④ | 成立 | 旧 staging ELF 的 `.lrodata` 段 VMA=`0xffff800000290000`、LMA=`0x290000`，平移量与其它段一致；文档 `:740` 的两个读数与我的 `readelf` 一致 |
| 25 | READER-C-06 | ④ | 成立 | 文档 `:603` 已列三种坏情形；按 `and`/`push`/`sub`/`call` 的 8 字节账复算，第三种确实使入口 RSP ≡ 0 mod 16 |
| 26 | READER-B-05 | SUGG | 成立 | 文档 `:588` 已补“两个调用点与三份实现分支不是同一个计数” |
| 27 | A-S03 / A-S06 / B-S11 / B-S14 / C-S10(a) | SUGG | 处置过期 | 五条的目标文本都已由 #5/#7/#9/#20 修掉；台账仍写“登记不实施” |
| 28 | CLAIM-05 / CLAIM-06 | ① | 成立 | 与 #1/#4 同簇；不是新根因，是独立复核 |
| 29 | CLAIM-01..04 | FP | 类别不成立 | `refute.md` 原文是“反证失败（主张成立）”；应记“主张维持/非缺陷”，不是误报 |

**一致性**：41 个 ID 中，32 个处置与我的独立判断一致。不一致的有 9 个：`A-S03`、`A-S06`、`B-S11`、`B-S14` 四条建议的目标文本已被别的发现修掉，台账仍写“登记不实施”；`CLAIM-01..04` 四条被记成 `FP`，实际是“主张成立、非缺陷”；`C-S10` 的子项 a 与 `B-S11` 同源，算第 9 个。32/41 = **78.0%**，低于项目的 90% 门槛，故本轮运行质量写 **NOT PASS**。若只算 30 个 FINDING 级抽样 ID，30/30 = 100%；问题集中在 SUGG 桶的维护性与 FP 的语义，不在 4 条 P0 的修复正确性。

## 四、四个数

| 数 | 运行报告 | 独立重算 | 依据 |
|---|---|---|---|
| 唯一根因数 | 79（`metrics.md:100`） | **约 47 个缺陷根**（含 4 条被维持的反证主张为 51 个主题） | 79 个 ID 中至少 28 个是跨角色/同根重复；按每簇一个去重后余 51 个主题，再扣 4 条非缺陷的 CLAIM-01..04 |
| 自引入数 | 0（`metrics.md:104`） | **2**（1 条已修、1 条未收） | 批一改了 §3.2 的代码块却留下题句 `:458`，制造了同节自相矛盾（`certify-r4-pass1-fact.md:10-14`，`0f5a5bd9d` 修复）；批一在 §5.1 新写“三类畸形”（文档 `:1120`），certify 判它计数不精确但未入台账（`round4-fact.md:156`） |
| 重复合并数 | 0（`metrics.md:103`） | **≥25，聚类表为 28** | 见第八问第 1 条；最强簇是 §3.2 的 8 个 ID、测试账的 6 个、v1 载荷/栈字段/表格行各 3 个 |
| edition 是否稳定 | V3.4（`metrics.md:112`） | **稳定**：R1/R2 为 V3.3，R3/R4 为 V3.4；D-063 明确授权 R3 升版 | `loop.md` 各轮 §0 的 `edition` 行；`DECISIONS.md:145` 的 D-063。注意 V3.5 制品链与 V3.6 减法表被部分采用但没有升 schema，`eval-trace-check.py --round R4` 仍打印“edition V3.3 试验窗口 1/3”，说明工具读到的不是 R4 的 V3.4 标记 |

## 五、BLIND-SPOT-DELTA

以下编号是候选，供下一版 `BLIND-SPOTS.md` 采用；本报告不改该文件。

- **B-69 语义去重回归**：`唯一根因数` 按 ID 计数，跨角色同根不合并；R3 曾把 95 个 ID 聚成 37 个根因，R4 退回 79/79，`重复合并 0` 与语义重复 ≥25 并存。探测器：metrics 增 `语义根` 字段；台账要求 `dup-of`；或恢复 R3 的聚簇 ID。
- **B-70 制品链“入库”只查文件存在，不查 git 跟踪**：四份 certify 回执 untracked、loop §5–§8 未提交、FREEZE 没有 R4 append，`eval-trace-check.py --round R4` 仍 PASS。探测器：`certify-artifact` 用 `git ls-files` 或 `git status --porcelain` 判“已入库”；轮关闭时 `FREEZE.md` 必须有本轮终态行。
- **B-71 行为门止于内核 ELF，不到可引导镜像**：`check-layout` 读 `target/<arch>/release/kernel`，`test-sysboot` 构建另一颗测试核；`minix.img` 仍嵌旧内核（5 个 `LOAD`、986 个 `.lrodata`）。探测器：链接脚本变更后，解出镜像内 `/EFI/minix/kernel.elf` 的哈希/`readelf` 与当前 release 内核比对。
- **B-72 未编号 SUGGESTION 逃逸**：`## SUGGESTION` 没有 ID 时 `panel-index.py` 记 0，`fact.md` 的 5 条建议因此不在 INDEX/台账/metrics。探测器：`panel-index` 对无编号建议给 WARN；或要求每条建议有 ASCII ID。
- **B-73 历史回执命名与引用对不上**：`loop.md` 引用 `round4-fact-pass1.md`，实际是 `certify-r4-pass1-fact.md`；reader 的 pass2 回执缺失；最终回执标题写“第二次/第三次”但文件名不带轮次。探测器：回执文件名固定为 `round<N>-<face>-pass<M>.md` 与 `round<N>-<face>-final.md`，loop 用 glob 校验。
- **B-74 全局检索把历史评审带进材料面**：`fact.md:6` 的全仓 `rg` 扫到 `panel/round*` 与旧 review 文件名/片段，未记 VARIANCE。探测器：FACT/DESIGN 的 `rg` 必须带 `--glob '!rewrite-notes/coordination/**'`，或把“检索输出”纳入材料矩阵。
- **B-75 严重度分布无人对账**：`loop.md:399` 的 4/4/19/9 与台账的 4/5/20/7 不符，metrics 没有严重度键。探测器：metrics 增 `严重度: P0/P1/P2/P3`，工具按台账逐 ID 对账。

## 六、建议变更

1. **把“唯一根因”和“重复合并”改成语义口径**：恢复 R3 的聚簇 ID（或强制 `dup-of`），在 metrics 里同时记 `ID 数` 与 `语义根数`；否则 79/79 会被读成“本轮有 79 个独立缺陷”，而实际约 47 个。
2. **把 FP 桶拆成“误报”和“主张维持”**：`CLAIM-01..04` 记 `N/A-claim-held`（或 ④），`FP` 只留给“报了但被源码/文档推翻”的 FINDING。metrics 的 `误报` 才会是精度指标。
3. **增加“可引导镜像一致性”门**：链接脚本变更后，跑 `xtask image --arch <arch>` 并解出 `/EFI/minix/kernel.elf`，断言其哈希等于当前 `target/<arch>/release/kernel`，再跑一次 `test-shim-bootmarks-x86`/`test-sysboot`。这是第九问的直接补丁。
4. **轮关闭前把制品真正入库**：`git add` 四份 certify 回执与 loop §5–§8；`FREEZE.md` 追加 R4 终态（edition、基线、最终 sha、certify 读数、未闭环 0、R4-design-batch 债务）；`eval-trace-check` 增加 `git ls-files` 检查。
5. **修严重度与台账的对账**：把 `loop.md:399` 改成台账实测的 4/5/20/7（或说明它统计的是哪 36 条），并在 metrics 的 R4 块加一行 `严重度`。
6. **给 SUGG 桶加三态**：`已由发现解决（dup-of 哪个 ID）` / `只登记` / `下一轮必须处理`。至少把 A-S03、A-S06、B-S11、B-S14、C-S10(a) 标成已解决，避免 37 条里混着 5 条陈旧项。
7. **修正历史回执的命名与引用**：`round4-fact-pass1.md` → `certify-r4-pass1-fact.md`；补 reader pass2 回执，或在 loop 里明说“pass2 未单独留档，只在最终回执的汇总行里”。
8. **把 D-106 与 R3-design-batch 合并成一行**：同一项（arch `link.ld`）不要在两个债务行里各占一行；D2-07 也标 `dup-of V1 DESIGN-03/D02-F01`。D-105/D-106 的“用户批准”要指向具体的指令文件或 commit，不能只写日期，更不能借 D01 的 R36 授权。
9. **给确认轮内实施 D2-05 补一句授权说明**：在 R4 §3/§4 写明“D2-05 是 P2 代码缺陷而非 ⑥ 设计项，因此按确认轮 P2 事实修复权限实施”，或把它移到下一 code batch。当前只写了 D2-04/D2-06 为什么不做，没写 D2-05 为什么可以做。
10. **回写 BLIND-SPOTS**：把 L6.5 宣称的“B-68 扩展”真正写进 B-68 或新增 B-69；否则下一轮会继续引用一个不存在的工件。

## 七、局限声明

- 本报告是单 agent 的只读复核，不是跨模型或跨工具验证；关键命令（`grep`、`readelf`、`cargo test --list`、`check-layout.sh`、`eval-trace-check.py`、`panel-index.py`、`doc-citation-check.py`、`anchor-resolve.sh`）都在本会话重放过，输出已写入上表或正文。
- 抽样 41/79 = 51.9%，高于要求的 20%；FINDING 级处置一致性 30/30 = 100%，含 SUGG/FP 桶的整体一致性 32/41 = 78.0%，按项目规则写 **NOT PASS**。这个 NOT PASS 指向轨迹/建议桶的维护性与 FP 语义，不指向 4 条 P0 的修复正确性。
- 未跑 QEMU/真机；未运行完整测试套件（只跑 `--list` 与既有工件）；未对 `.lrodata` 做全链接器矩阵实验；未复核 `round4-reader.md` 的 GNU ld 最小复现（只核对文档引用的旧工件读数与我的 `readelf` 一致）。
- 未读 `expected/`、其他篇目的面板正文、`minix3/` 原始 C 源；对 C 语义的判断只到 `panel/round4/refute.md` 给出的路径与我的抽查范围。
- 总体判 **FLAT**：检测与设计轴相对 R2/R3 是 IMPROVED（4 条真 P0、1 条真实链接缺陷、2 条读者独有错误），轨迹与去重轴是 REGRESSED（79/79 根因、制品未入库、FREEZE 未追加、盲区回写无工件）。两个方向相互抵消，净判 FLAT；若只看检出可写 IMPROVED，但那会把轨迹退步算成零。
