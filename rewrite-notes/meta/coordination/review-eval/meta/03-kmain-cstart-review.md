# D03 meta-review —— 03-kmain-cstart 循环

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md` 第一节，生效中）
- 基线提交：`859dd2216`；目标文档 `rewrite-notes/01-stage-kernel/03-kmain-cstart.md`（基线 1375 行，当前 1380 行）
- 本轮改动：`git diff --cached --stat` 显示 1 个文件、23 行新增、18 行删除；只有文档被 stage，代码未改
- 被评对象：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart/` 下的 `loop.md`、`metrics.md`、`defect-ledger.md`、`follow-ups.md`，`tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/` 的 7 份面板报告，以及上述 staged diff
- 评的是工作流，不是这篇文档；本报告不参与设计、不作修复、不默认本轮处置正确
- 抽样：81 条台账发现中抽 25 条独立复判（30.9%）；处置类被证据支持 17 条、不成立 8 条，一致性 68.0%（宽口径 22/25 = 88.0%，仍低于 90%），按规则写 `NOT PASS`

## 一、结论

| 项 | 结论 |
|---|---|
| 相对 D01/D02 的 V1 基线 | **FLAT** |
| 本轮运行质量 | **NOT PASS**（抽样一致性 68.0% < 90%） |
| 检测面 | **IMPROVED**：在历史读数“最终 0/0/0”的篇目里发现 3 条 P0、8 条首发 P1（另有 6 条读者 P1 与 1 条反证 P1，多为同族或重复）；设计轴给出 19 条实质发现，其中 5 条是可删除的死抽象/冗余 |
| 处置面 | **REGRESSED**：24 条记 ①，按发现点名的位置全部改到的只有 17 条（70.8%）；7 条部分或错位；转出率 70.4%，高于 D01 的 56.5% 与 D02 的 60.9% |
| 流程完整性 | B-17（轨迹门只认 `## FINDING` 形状）与 B-18（跨角色重复计数、处置互相矛盾）原样复发；新增两类修复侧缺口（修复语句自身的事实未复算、文档修正与被引用代码注释脱钩）与一类分诊缺口（处置类按来源角色分配） |
| 是否支持 V2 | 支持，但只支持两条晋升（C-20 轨迹门 v2、C-23 禁止 follow-up 大桶）；其余候选继续排队 |

判 `FLAT` 而不是 `IMPROVED`：检测确实更宽、更深，但同一版本下 D01 已暴露的轨迹门缺口与跨角色重复病在本轮原样复发，运行质量读数从 D01 的 87.5%、D02 的 84.6% 降到 68.0%，修复率从 43.5%、34.8% 降到 29.6%。判 `FLAT` 而不是 `REGRESSED`：三条 P0 与多条 P1 的发现都与源码对得上，设计轴与读者轴都产出了前两轮没有的实质内容，没有出现新的严重误报，范围纪律与冻结决策两项仍然干净。

## 二、九问

### 1. Detection：PASS

能发现的部分：

- 三条 P0 都是真的，且都落在历史“最终 0/0/0”的篇目里。`FACT-01`（`defect-ledger.md:24`）指出 §4.1 仍在描述已删除的 `FREE_MEMMAP`/Phase A.2/ Phase C 归还链；`FACT-02`（`:25`）指出 `store_kernel_info` 是七个 slice，不是“三个变长载荷/五块”；`FACT-03`（`:26`）指出 `cstart` 不是 `kmain` 的最后一个动作。三条我都回到源码复算，见第三节。
- 四条 P1 事实发现覆盖了不同的失败面：`FACT-04`（aarch64 `SP_EL1` 的 UNDEFINED 断言）、`FACT-05`（39 门只对 `prot_init` 成立）、`FACT-06`（Phase B“只做软件态”与 B.5/C.5/C.6 缺失）、`FACT-07`（`arch_init` 的 APIC 分支）；P2 里也有实质项，例如 `FACT-08`（`IDT_ENTRIES` 行号）与 `FACT-12`（测试计数口径）。
- 设计轴不是命名与注释级意见。`design-d2.md` 的 19 条发现里，五条是可直接删除的死抽象/冗余：`Privilege`/`PrivilegeLevel` 整条特权级轴零非测试消费者（DESIGN-13）、`TrapEntryArch::load_ap` 三个实现全是 `self.load()`（DESIGN-14）、`cpu_count` 只写不读（DESIGN-15）、`unsafe impl Send/Sync` 冗余（DESIGN-16）、`is_trap` 是被覆盖的死值（DESIGN-12）。另有四条 P1 设计问题：NMI 配 IST1 改变嵌套语义（DESIGN-01）、kernel `MAX_CPUS=32` 与 x86 保护结构 8 个槽不一致（DESIGN-02）、`load()` 安全可重入但 `ltr` 非幂等（DESIGN-04）、装载次序无自动证据（DESIGN-19）。
- 盲区账本里标 `DETECTED` 的条目本轮继续出现：B-01/B-02 由 DESIGN-12..16 命中；B-03 由 READER-C-01/02/03/04/06/07/08/12/13/14 命中；B-04 由 READER-A 12 条与 READER-B/C 命中；B-08 由 FACT-02/05/08/12 命中；B-12 由 FACT-04/11 命中。
- 第一次拿到强证据的 OPEN 条目：B-05（假反事实）由 FACT-15、FACT-16、READER-A-10、READER-C-16 命中；B-13（硬件机制泄漏到共享语义层）由 DESIGN-18(b) 命中（`DIVIDE_ERROR`/`PAGE_FAULT`/`KERN_CALL_VECTOR` 这些 x86 向量名放在 `os/arch/src/arch/trap_entry.rs` 这个跨架构模块）。

仍然漏的：

- B-06（带空格的锚点/渲染退化）零命中：本轮只跑了 `anchor-resolve.sh`（80/80 解析），没有检查 CommonMark 渲染。
- B-07（architecture port 被译成“端口”）零命中：读者没有按词义消歧检查。
- B-14（测试计数口径）没有按账本要求用 `cargo test -- --list` 复验；FACT-12 仍用 `grep -c '#\[test\]'` 的静态计数。更严重的是 `cargo test -p minix-arch --no-run` 目前编译失败（45 个错误，`KernelInfo` 初始化缺 `memmap_sources`/`startup_objects`），所以这 375 个测试连“能跑”都不成立。
- B-15（修改公共入口枚举调用方）本轮没有改代码，未检验。
- B-16（材料违规分级）与 B-19（读者画像对设计角色的可见性）本轮没有发生违规，未检验。

一句话：检测面在本轮是 V1 三轮里最宽的一次，尤其是设计轴与反证轴；漏检集中在“渲染/词义/运行面”这三类没有机械装置的检查上。

### 2. Precision：CONCERN

- 台账记 ③ 误报 0、④ 措辞 0、⑤ 证据不足 0。这个读数不能当作精度证明：81 条里 57 条（70.4%）是 ② 转出，转出项没有逐条判“误报/措辞偏好/真教学债”。`loop.md:100` 自己承认“reader B/C 的 57 条转出项中，有多少是真正的教学债、多少是文风偏好；本轮未逐条判 ③”。
- 我抽到的低置信项至少五条：READER-B-15（自拷贝 `memcpy` 的未定义行为前提）、READER-B-16（LDT“必须存在”的循环论证）、READER-B-17（UEFI 内存归属前提缺锚）、READER-C-16（“危险不在此刻”与描述符缓存）、READER-C-17（C 的两次 `tss_init` 在 Rust 侧只剩一次）。这些是读者自己标“低置信”的论证/前提问题，不是硬错误；它们被转出而不是被判 ③/④，说明“误报 0”只是“没有裁判”。
- 类别与角色模式很清楚：FACT 16 条全部 ①；DESIGN 19 条里 2 条 ①、17 条 ②；READER-A 12 条里 2 条 ①、10 条 ②；READER-B 17 条里 2 条 ①（其中一条是 ID 错配）、15 条 ②；READER-C 17 条里 2 条 ①、15 条 ②。处置类与来源角色强相关，而不是与严重度或可修性相关。
- 最明显的角色偏差：P-REFUTE 的 CLAIM 05 有一条 P1 文档事实——§4.5 说 GsArea 三个字段“由 `program_gs` 写入；入口汇编只读”，而 `os/arch/src/x86_64/trap_stub.rs:373-377` 的 `x86_syscall_entry` 第一条就是 `mov qword ptr gs:{gs_user_rsp}, rsp`。这条 P1 被转进 D03-F13，而同轮 FACT 的 P1 全部记 ①（其中 FACT-04/05/06 的修复并不完整，见第八问）。这不是精度问题，是分诊偏差；但它让“误报 0”的读数更不可信。
- 没有发现硬误报。抽样 25 条里，没有一条是“文档说的是对的、面板报错了”。所以精度问题的性质是“未测量 + 角色偏差”，不是“报得多而错”。

### 3. Design quality：PASS

设计轴给出的是可删除的抽象、错误建模、多余状态与不合理 trait，不是命名与注释：

- 可删除的抽象：`Privilege`/`PrivilegeLevel` 整条轴（约 120 行，零非测试消费者，`design-d2.md:585-619`）；`TrapEntryArch::load_ap`（三个实现全是 `self.load()`，`design-d2.md:622-652`）；`cpu_count`（三个结构体各一个字段，只写不读，`design-d2.md:655-680`）；`unsafe impl Send/Sync`（字段本来就自动派生，手写反而移除未来加字段时的编译器检查，`design-d2.md:684-710`）；`is_trap`（`init` 写陷阱门、安装器全部重写成中断门，初值从未生效，`design-d2.md:546-581`）。
- 翻译残留：单实现 trait 的逐成员清理（保留 trait，删死成员）、`kernel_stack_top == 0` 哨兵、`is_trap` 的布尔状态、`MemMapError::ZeroLength` 这个 C 没有的失败分支（`design-d2.md:39-50`、`:460-503`）。
- 错误建模：`load()` 是安全 `&self` 可重入方法，但 x86 第二次 `ltr` 会撞 busy TSS 描述符 `#GP`（DESIGN-04）；`kernel_stack_top == 0` 同时表示“无”和“合法零值”（DESIGN-06）；四个 `MAX_CPUS` 数字没有编译期关联（DESIGN-02）；GDT 描述符指向 TSS 的地址稳定契约只靠调用顺序（DESIGN-04/08）。
- 多余状态：`init` 给全部 8 个 TSS 槽写 BSP 栈顶作占位，直到 `smp_init` 逐个覆盖（DESIGN-08）；`cpu_count` 是假状态（DESIGN-15）。
- 不合理 trait/多余复杂度：`load_ap` 纯别名（DESIGN-14）；`HigherHalf` 的“统一调用点、不带 `#[cfg]`”理由与三个 `#[cfg]` 调用点相反（DESIGN-18a）；五个手写门清单、`configure_syscall` 与 `write_syscall_msrs` 的复制粘贴（DESIGN-11/17）。

不足有两点。第一，19 条设计发现里 17 条转出，2 条（DESIGN-10/11）只在文档侧修，代码零改动；`loop.md:63` 明确“本轮未改代码”。第二，D2 的 VERDICT 把“实现与设计主张是否一致”和“设计是否最小”混在一起（`design-d2.md:853-879`），而 D1 提出的更小方案（单 `Tables`、`Loaded` 凭据、`CpuTableOps` 记录器）只被逐条对照了 10/14，剩余 4 条（D01/D10/D12/D14）没有对照记录。设计轴的“找得到”成立，“设计是否已经最小”的结论不完整。

### 4. Independence：PASS

- D1 是真冷读：`design-d1.md:3-7` 声明没有打开 `os/` 下任何文件、没有读 §3 及以后、没有读 `.design/` 快照与 `reader-profile.md`；它从 24 个 C 源与 §1/§2 独立提出 14 条设计决策，包括 `Loaded` 凭据、`CpuTableOps` 窄 trait、单 `Tables`、DPL 集合相等断言等。
- D2 是真对照，不是复述：文件时间戳显示 `design-d1.md` 06:02 落盘、`design-d2.md` 06:46 落盘，中间隔 44 分钟；D2 的 DIFF-TABLE 逐维写“d1 方案 / 现码证据 / 差异判断”，并引用 d1 D02/D03/D04/D05/D06/D07/D08/D09/D11/D13 共 10 条决策。它明确拒绝 d1 的宏改造、单 `Tables` 合并、能力类型，也明确接受 d1 没提的“全槽占位是层间依赖的必然结果”。这是独立判断，不是抄写。
- 读者是真冷读：A/B/C 只读文档全文与 `reader-profile.md`，不读源码、diff、历史版本、其他面板输出。它们独立产出了主 agent 原本不知道的内容：A-06/B-05/C-05 三人都独立发现 §3.2 把系统调用入口归到 §2.5（实际在 §2.4 的 `tss_init`）；B-03 发现 §2.8“归纳”节携带前文从未分析的新不变量；C-01 发现 §4.7 的 `Option` 映射保住的是“已初始化”不是“已装载”；C-10 发现 §5.2 把“负向缺口”写成覆盖的“承担者”。
- 独有发现的比例：我按“同一处缺陷”合并后，81 条里约 20 条是跨角色重复（见下），约 60 条只由一个角色报出。跨角色重复是收敛证据，不是抄袭证据；没有任何一条发现是复述主 agent 已有的内容（面板没有拿到主 agent 的历史评审）。
- 保留：D1 的 14 条决策没有进台账；D2 只对照了 10 条；D1/D2 的对照记录在 D2 的报告里，不在 `loop.md` 的轨迹里。独立性强，可追溯性弱。

### 5. Scope discipline：CONCERN

- 冻结决策检查通过：`loop.md:39` 记 `re-litigation` 0；我抽读 D-001（四章骨架）、D-002（短锚加链接）、D-003（保真度标签）、D-004（关联清单只在 §7）、D-005（“腿”是受控术语）、D-010（已冻结篇目不为风格重开）、D-012（同 agent 验证声明）的相关文本，没有发现重提。D-005 尤其干净：§4.6 在首次使用“腿”时给出定义，没有要求全局替换。
- 相邻问题检查通过：`metrics.md:6` 记 `adjacent 阻塞` 0；57 条 ② 全部只进 `follow-ups.md`，没有一条被升级为本轮阻塞。DESIGN-18 对 `HigherHalf`/`Paging`/`HugePages`/`BootArch` 的 trait 审计明确写“不在本篇射程”，没有把相邻模块的结论当阻塞。
- 但边界有三处漏：
  1. D2 读了 21 个关联文件之外的 7 个 Rust 文件与 3 个 C 文件：`os/kernel/src/boot/higher_half.rs`、`os/arch/src/arch/paging.rs`、`os/arch/src/arch/paging_ext.rs`、`os/arch/src/arch/boot_arch.rs`、`os/arch/tests/arch_small_files_audit.rs`、`os/kernel/src/proc.rs`、`os/arch/src/x86_64/ap_early_entry.rs`，以及 `pg_utils.c` 的 `add_memmap`、`arch_proto.h` 的 `tss_s`、`mpx.S` 的 SYSCALL 入口。`loop.md:20` 的 D2 条目写“26 个 Rust 文件”，21 + 7 = 28，数字也对不上；报告头披露了额外文件，但 `loop.md §0` 的范围声明写“未列入清单的文件不在范围”。这不是把相邻问题当阻塞，而是把“读”的范围悄悄放大。
  2. 四条 P1 设计发现（DESIGN-01/02/04/19）都在 21 个关联文件内，却全部转出；转出的理由（代码改动不属于文档轮）成立，但其中 DESIGN-01（嵌套 NMI 风险）与 DESIGN-02（kernel 32 与 arch 8 的容量差）有本可同轮完成的文档披露没有做。这不是扩大责任边界，是缩小了本该承担的那一格。
  3. 文档修复把 `os/kernel/src/trap_dispatch.rs` 加进 §7.3，`doc-code-map.sh` 从基线的 21 个文件变成 22 个；`loop.md §0` 仍写 21 个文件、18,063 行，§5 写 22/22。基线口径与修复后的口径没有对齐。
- follow-ups 的粒度是最大的范围问题：`D03-F12a`、`D03-F12b`、`D03-F12c` 三行分别压了 10、15、15 条读者发现（`follow-ups.md:16-18`），目的地是“闭环轮”“教学债批次”，没有文件、没有验收判据、没有触发条件。这是 C-23 要禁止的“大桶”，而 C-23 目前只是 V2 候选，本轮没有生效。

结论：窄口径的“不把相邻问题当阻塞、不重提冻结决策”是 PASS；宽口径的“读的范围、转出的粒度、文档侧该改未改”是 CONCERN。合起来给 CONCERN。

### 6. Teaching quality：CONCERN

- 读者确实发现了真实理解障碍，而且不少是“照着读会得到相反结论”：A-06/B-05/C-05（§3.2 把系统调用入口归到 §2.5，实际在 §2.4）、A-07（`lidt` 与 `lgdt` 之间的危险窗口写成“装载后”）、B-01（`cstart` 不是最后动作）、B-03（归纳节引入新论证）、B-08/C-12（“三张表”的表里有五行，LDT 与页表重建段不属于三张表）、C-01（`Option` 映射的是“已初始化”不是“已装载”）、C-06（“①和③各只有一套”与 TSS 每 CPU 一个字段冲突）、C-07（§1.2 的 Rust 列漏了 GS 区）、C-10（负向缺口被写成覆盖的“承担者”）。这些是教学债，不是文风偏好。
- 采纳率：台账记 6 条读者发现 ①（A-09、A-10、B-01、B-13、C-02、C-03），6/46 = 13.0%；其余 40 条转出。按报告内容，B-13 与 B-14 的 ID 互换了（见第八问），实际被修的是 B-14（§3.6/§5.3 的悬空指针），B-13（§2.2 表格与反事实重复）没有修。所以“6 条”这个数字对，但对应的条目错了一条。
- 纯文风噪声：我抽到的低置信项里，B-12（§4.3 的 `#GP(0x28)` 被注释解码与反事实各讲一遍）、B-13（§2.2 表格与反事实同义）、B-15（自拷贝 `memcpy` 的未定义行为前提）、B-16（LDT“必须存在”的循环论证）、B-17（UEFI 内存前提缺锚）、C-16（“危险不在此刻”与描述符缓存）更接近“论证重复/前提未建立”，不是“句子不好看”。没有发现纯审美的“改个词更顺”类噪声；这与 D02 的 7%–10% 噪声比例相近或更低。
- 真正的问题不是噪声，是“便宜的高置信修复没有做”。A-06/B-05/C-05 是同一句话的归属错误，改一句就能闭合；A-07 加“之间”两个字；B-01 改“最后一个动作”的限定；B-08/C-12 改标题或拆表。这些都在本轮文档射程内，却全部进了“教学债批次”。教学轴的“找得到”成立，“当场修”的比例只有 13%，而且没有分诊规则区分“一句话可闭合”与“需要重写一节”。

### 7. Cost：CONCERN

- 预算：`CORPUS.md:70-72` 给单篇面板 5–8 个子 agent、主 agent 修复轮最多 2 轮、meta-review 1 个。本轮 7 个面板 agent、修复 1 轮、meta-review 1 个，在预算内。
- token：未采集（`metrics.md:8` 写“token 未采集”）。因此“收益递减”只能从处置结构与轮次判断，不能从单位成本判断。
- 处置结构：81 条里 24 条 ①（29.6%）、57 条 ②（70.4%）、0 条 ③/④/⑤。D01 是 37/85 = 43.5% 修、56.5% 转；D02 是 24/69 = 34.8% 修、60.9% 转。第三个真实篇目，修复率再降 5 个百分点，转出率再升 9 个百分点。
- 债务总量：17 条设计债、40 条教学债、1 条测试债（`cargo test -p minix-arch --no-run` 编译失败，45 个错误）。下一轮若对同一篇再做冷读，读者最可能把未修的 40 条再报一遍；在债务没有分诊之前再派 7 个 agent，边际收益接近零。
- 检测尚未递减：三条 P0 是在历史“0/0/0”的篇目里新发现的，说明“冷读 + 事实复算”仍有增量。递减的是“闭合”，不是“发现”。
- 另一处成本口径问题：`CORPUS.md:75-79` 的“上次运行结果”仍停在“基线脚手架 / 未运行”，D01/D02/D03 的读数没有回写；`BLIND-SPOTS.md` 的修改时间（05:53）早于本轮 run 目录（06:50），D03 的检测证据也没有入账。语料与盲区账本本身没有随轮次维护。

### 8. Trace integrity：FAIL

- 面板到台账的完备性只在 `## FINDING` 形状上成立。`eval-trace-check.py` 的实际输出是：`WARN panel: design-d1.md 没有 ## FINDING <ID> 行`、`WARN panel: refute.md 没有 ## FINDING <ID> 行`、`PASS panel: 面板报告解析到 81 条发现`、`PASS coverage: 所有面板发现都有处置行`、`PASS 6 / WARN 2 / FAIL 0`、退出码 0（本会话重放）。也就是说，工具自己知道两份报告没有被解析，结论仍是 PASS。
- 漏处置的实际内容：
  - `design-d1.md` 的 14 条 `DECISION` 没有任何处置行。`loop.md:20` 把它们写成“14 条 DECISION（不是 FINDING）”，这是形式主义的切割；B-17 的要求是“设计提案入台账”，不是“只把 FINDING 入台账”。14 条里只有 10 条被 D2 引用对照，D01/D10/D12/D14 没有对照记录。
  - `refute.md` 的 6 条 `CLAIM` 有一个独立的 `## P-REFUTE 附加` 表（`defect-ledger.md:87-96`），比 D02 进步；但该表没有“处置类”列，CLAIM 03/04 的“无动作”没有类别，CLAIM 05 的 P1 被转出而不是 ①。工具仍不解析它，指标也不计这 6 条。
  - `loop.md:78` 写“81 条发现全部有处置行、无孤儿行、metrics 自洽”。对 81 条 `## FINDING` 成立；对面板的全部产出（81 + 14 + 6 = 101 条）不成立。
- 指标的算术自洽、语义不自洽：`metrics.md:3-7` 的“81 = ①24 + ②57”自洽；`metrics.md:7` 的“重构 提出 19 / 接受 6 / 实施 2 / 登记 17”不自洽——6 + 17 + 2 = 25，不等于 19；三个数不是同一集合的划分（实施 2 应含在接受 6 内，登记 17 又与 ② 的 17 条设计转出重合）。“接受”与“实施”没有定义，无法复算。`loop.md:63` 的“19 条设计发现全部转出”也与台账不符：DESIGN-10/11 是 ①，台账的 ② 只有 17 条设计发现。
- ① 行的完成度被高估。24 条 ① 里，按发现点名的位置全部改到的只有 17 条（70.8%）；7 条部分或错位：
  - `FACT-02`：§4.1 改成七个 slice，但 §4.7 的对照行仍写“把五个载荷落进 `.bss`”（`03-kmain-cstart.md:1185`），而发现点名了 §4.7。
  - `FACT-04`：§4.6 改成“理由写在 `protection.rs` 的 `fn init` 的注释里……不是 UNDEFINED”，但 `os/arch/src/arm64/protection.rs:140-144` 的注释仍写“ARMv8 D1.5 declares that encoding UNDEFINED when `SPSel=1`……this is exactly the boot trap observed during bring-up”。文档现在把载体注释（`test-protection-aarch64/src/main.rs:376-377`）的判断归给了生产注释，与它引用的源注释相反。
  - `FACT-05`：§3.7 加了限定，但 §2.5 仍写“39 即 IDT 上被置为 present 的全部门”（`:589`），§4.7 仍写“增加 LAPIC/调度向量”（`:1196`）；新加的“C 的总门数会变成 84/86”（`:946`）本身是错的，按 C 源复算是 90/92（推导见第三节）。
  - `FACT-06`：§3.6 改了“software only”，但“把 kmain 的全部工作划成 Phase”（`:896`）与缺失的 B.5/C.5/C.6 没有补。
  - `FACT-13`：§7.2 补了 `arch_clock.c`、`earm/pre_init.c`、`servers/vm/main.c`，§7.3 补了 `timer_irq_gate.rs`、`trap_dispatch.rs`；但 §2.6 引用的 `minix3/minix/servers/vm/pagetable.c:pt_init`（`:631`）与 §4.6 引用的 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs`（`:1167`）仍不在 §7 清单里。
  - `READER-B-13`：台账记 P1、①、“同 FACT-14”，但 reader-b 的 B-13 是“§2.2 依赖表与反事实重复”（`reader-b.md:291-309`），不是 §3.6 的悬空指针；真正对应 FACT-14 的是 B-14（`reader-b.md:311-329`），台账却把 B-14 记成 P2、②、转出。两条的 ID 与内容互换了。
  - `READER-C-03`：§4.7 改成“`intr.init()` 已编程 LAPIC/IOAPIC，时钟门在 Phase F 放行”，但 §5.4 M10 仍写“控制器未编程、IF=0”（`:1274`），同一文档内新造了一处直接矛盾。
- 验证列是模板，不是逐条证据。16 条 FACT 的“复验”列全是同一句“L4 文档五门重跑”（`defect-ledger.md:24-39`）；`eval-trace-check.py` 只检查“非空且不是占位”，所以一句模板就能过。文档五门（锚点、代码映射、引用、承载、文风）都是文档级检查，不能证明某一条事实被重算。
- 工作流账本自身没有回写：`CORPUS.md` 的“上次运行结果”仍写“未运行”；`BLIND-SPOTS.md` 的 B-05/B-13/B-17/B-18 没有 D03 证据行；`WORKFLOW-VERSIONS.md` 的 D02 段仍是 D03 之前的边界状态。元评审的 delta 如果不回写，下一轮还会从同一状态开始。

### 9. 本轮最值得但工作流仍看不见的一件事：修复后的新句子没有独立复核（FAIL，开放题）

工作流的检测是独立的：P-FACT、P-DESIGN、P-READER、P-REFUTE 各自冷读，主 agent 汇总。工作流的修复不是独立的：主 agent 按面板的“最小修复”改写文档，然后跑文档五门；五门只证明“锚点能解析、路径存在、章节引用能解析、章节有码、文风无违规”，不证明“新句子的事实是对的”。本轮有两个活证据：

- `FACT-05` 的面板建议只说了“把 39 限定为 `prot_init` 完成时”，主 agent 在 §3.7 加了一句“C 的总门数会变成 84/86”。这个数字来自 `refute.md:57` 的口算，按 C 源复算是 90/92；84/86 把异常数取成 Rust 的 18，漏掉 4 条软中断与 0xF0 定时器门。新句子通过了五门，但没有通过事实复算。
- `FACT-04` 的面板发现只说了“生产注释与载体注释冲突，需裁定”，主 agent 在 §4.6 写成“理由写在 `protection.rs` 的 `fn init` 的注释里……不是 UNDEFINED”。生产注释仍写 UNDEFINED；文档现在引用了它没有说的话。五门全绿，因为锚点仍能解析到 `fn init`。

这两条不是“面板报错了”，而是“修复这个动作没有被第二个视角检查”。工作流把面板发现当作输入、把主 agent 的修复当作输出，缺少对输出的独立复算。这个缺口的价值高于再加一个面板角色：面板已经能发现旧错误，但新错误是在修复阶段引入的，只有“修复 diff 的冷读/复算”能接住。建议在 L4 与 L6 之间加一个 L4.5“修复复验”：由未参与修复的实例对 staged diff 逐条重放发现点名的命令，并对每个新数字/新引文给出独立推导；成本约一个 agent，收益是防止“修一个错、引入另一个错”。

## 三、P0/P1 重点复核

### FACT-01（§4.1 旧 `FREE_MEMMAP`/Phase A.2 链）

发现成立，修复成立。重放：

```
$ grep -rn "FREE_MEMMAP" os/kernel/src
os/kernel/src/memmap.rs:523: ... (Phase A.2 was removed with FREE_MEMMAP)
$ grep -rn "add_memmap(" os/kernel/src | grep -v "fn add_memmap"
（生产代码只有 memmap.rs 内部 cut 拆分与测试；lib.rs:1568 是注释）
```

`os/kernel/src/lib.rs:1450-1460` 明写“Phase A.2 is intentionally empty: the kernel keeps no mutable copy of the boot memmap”；`os/kernel/src/vm_handoff.rs:257-259` 有 kernel image 的 `record.push`。文档 §4.1 改成“七个 slice、Phase A.2 为空、无 `FREE_MEMMAP`、扣除在 `vm_handoff::classify`、`init_proc_and_boot` 与 Phase F 没有 `add_memmap`”，与源码一致。

### FACT-02（`store_kernel_info` 七个 slice）

发现成立，修复部分成立。`os/kernel/src/lib.rs:1139-1268` 依次落 `RESERVED_REGION_STORE`、`MEMMAP_REGION_STORE`、`BOOT_MODULE_STORE`/名字池、`PLATFORM_SOURCE_STORE`、`PARAM_BUF_STORE`、`SOURCE_RECORD_STORE`、`STARTUP_RECORD_STORE`，最后 `:1258` 的 `let mut k = *kernel_info;` 拷贝结构体并重写 7 个 slice 指针。§4.1 已改；§4.7 的“把五个载荷落进 `.bss`”（`:1185`）没有改，而发现点名了 §4.7。

### FACT-03（`cstart` 不是最后动作）

发现成立，修复成立。`minix3/minix/kernel/main.c:147-327`：`cstart()` 返回后还有 `BKL_LOCK()`、`proc_init()`、boot image 循环、`arch_post_init()`、`memory_init()`、`system_init()`、`add_memmap()`、`smp_init()`、`bsp_finish_booting()`。§2.2 已改成“`kmain` 的一个早期动作；返回后继续执行……”，与源码一致。

### FACT-04（aarch64 `SP_EL1`）

发现成立，修复的证据链不成立。发现的核心是“文档说 UNDEFINED，同仓载体注释说 accessible，两处互斥”；这一点成立。修复把 §4.6 改成“`SP_EL1` 在 EL1 可访问（HCR_EL2.TSP=0）；崩溃原因是写它换掉当前栈，不是 UNDEFINED”，并称“理由写在 `protection.rs` 的 `fn init` 的注释里”。但 `os/arch/src/arm64/protection.rs:140-144` 的注释仍写“ARMv8 D1.5 declares that encoding UNDEFINED when `SPSel=1`……this is exactly the boot trap observed during bring-up”；载体注释 `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs:376-377` 才写“fully accessible from EL1 (HCR_EL2.TSP=0 confirmed)……not by EL2 trapping”。文档现在引用了生产注释没有说的话，且没有把生产注释标成待改的过期注释。正确的收口是二选一：改 `protection.rs` 的注释（本轮未改代码，可登记为 follow-up），或在文档里写“生产注释与载体注释冲突，待 ARM ARM 裁定”，不能把载体判断伪装成生产注释。

### FACT-05（39 门只对 `prot_init` 成立）

发现成立，修复部分成立且引入了新的错误数字。§2.5 的 39 只在 `idt_init` 完成时成立，这一点我从 C 源复核：`gate_table_pic` 16 条、`gate_table_exceptions` 23 条（19 条异常 + 4 条软中断），合计 39。APIC 路径的 present 门数按 C 源复算：

- `gate_table_ioapic`：64 条 `apic_hwint`（0x50–0x8F）+ `apic_spurios_intr`（0xFF）+ `apic_error_intr`（0xFE）= 66 条。
- `gate_table_common`：4 条（32–35），与 `gate_table_exceptions` 的 4 条软中断是同一组向量，替换而不是新增。
- `gate_table_smp`（`CONFIG_SMP`）：2 条（0xF1/0xF2）。
- `apic_idt_init` 的 `int_gate_idt(APIC_TIMER_INT_VECTOR, ...)`：1 条（0xF0）。
- 19 条异常 + 4 条软中断 + 66 条 APIC 门 + 2 条 SMP 门 + 1 条定时器门 = 92；非 SMP 为 90。

文档新加的“84/86”把异常数取成 Rust 的 18（跳过了 C 的向量 9），并漏掉 4 条软中断与 0xF0 定时器门，至少少 6 条；`refute.md:57` 的推导也犯了同一个错。文档还说“Rust 侧也会相应增加”，但 Rust 的 `X86_64TrapEntry::init` 只装 41 条门（`os/arch/src/x86_64/trap_entry.rs:238-243` 的 16 条 IRQ 向量 + 0xF0/0xF1/0xFF），没有 64 条 IOAPIC 门；`os/plat/src/x86_64/interrupt.rs:23-35` 也只定义 0x50/0x70 两组 16 条。§2.5 的“39 即全部门”（`:589`）与 §4.7 的“增加 LAPIC/调度向量”（`:1196`）没有改。发现成立，但修复没有闭合，且新数字是错的。

### FACT-06/07（Phase B 控制器与 `arch_init` 的 APIC 分支）

FACT-07 成立且修复成立：`minix3/minix/kernel/arch/i386/arch_system.c:246-288` 的 `arch_init` 除 `tss_init`、`ser_init`、`acpi_init` 外，还在 `USE_APIC && !CONFIG_SMP` 分支调用 `apic_single_cpu_init()`（`apic.c:1125-1157`），后者 `apic_idt_init(0)` + `idt_reload()`；`arch_init` 还设置 `k_stacks` 并 `cut_memmap(BIOS_MEM_BEGIN, ...)`。§2.2 已改。

FACT-06 成立，修复只做了一半：`os/kernel/src/lib.rs:1524-1546` 有 B.5 `kerninfo::init_kerninfo`、C.5 `IPC_FILTER_POOL`、C.6 `krandom::init` 三个补充动作；§3.6 的“把 kmain 的全部工作划成 Phase”（`:896`）仍在，Phase 清单仍只有 A–F，没有 B.5/C.5/C.6。“Phase B 只做软件态”已改成“Phase B 的 `intr.init()` 会编程 LAPIC/IOAPIC 控制器侧；Phase F 只放行时钟门”，这一半成立。

## 四、四条 P1 设计转出评估

| 发现 | 转出是否合理 | 去处是否具体 | 裁决 |
|---|---|---|---|
| DESIGN-01 NMI IST1 | 合理：改变 C 的嵌套 NMI 语义，属于需要用户裁决的架构选择 | `D03-F01` 给了两个选项（保留并标 `[ARCH]`，或改回 C 的嵌套行为），但没有决策记录、责任人、验收判据 | 部分合理。文档侧应同轮加一句“IST 递送无条件重载 RSP，第二个 NMI 会覆盖外层帧；本内核的 NMI handler 不可重入”，这是不依赖用户裁决的事实披露，本轮没有做。 |
| DESIGN-02 MAX_CPUS 32 vs 8 | 合理：改容量是代码批次，不属于文档轮 | `D03-F02` 写“code batch：加编译期关联或统一容量；优先于其它设计债”，没有文件、测试、验收判据 | 部分合理。文档 §3.1 仍写“Rust（x86-64，`MAX_CPUS`=8）”（`:762`、`:771`），没有披露 kernel `MAX_CPUS=32` 与 arch 8 的差；按 C-24 的口径，文档句应同轮降级。 |
| DESIGN-04 `load()` 可重入 | 合理：改 `unsafe`/一次性令牌/`debug_assert` 是 API 契约变更，需要裁决 | `D03-F04` 给了三个候选（`unsafe`、一次性令牌、`debug_assert` + 文档），仍没有决策记录 | 合理。这是四条里去处最像“可执行决策”的一条，但仍缺验收判据。 |
| DESIGN-19 装载次序无断言 | 合理：改测试载体属于测试批次 | `D03-F11` 写“test batch：加宿主顺序断言或 QEMU 载体覆盖”，没有测试名、文件、验收判据 | 部分合理。这是四条里目的地最像“桶”的一条；至少应点名 `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs` 与一条具体断言（例如“装 stub 后 `int $3` 进入内核”）。 |

总评：四条在“代码改动不在本轮”的范围内都可以转出，不存在把相邻问题当阻塞的问题；但按 C-23 的标准，只有 DESIGN-04 的目的地勉强具体，DESIGN-02/19 是桶，DESIGN-01 的文档侧披露被漏掉。更重要的是，这四条都在 `related-code.txt` 的 21 个文件内，读者会以为“本篇范围包含这些代码”，而本轮对它们零改动；范围声明与转出口径需要在下一轮写清楚。

## 五、D1→D2 对照与 B-17

### 5.1 D2 是否真的做了 D1→D2 对照

做了，而且是实质对照：

- 时间戳：`design-d1.md` 06:02:07 落盘，`design-d2.md` 06:46:50 落盘；D2 不是并行启动。
- D2 的 DIFF-TABLE 有 7 个维度，每行写“d1 方案 / 现码证据 / 差异判断 / 判定”，并引用 d1 的 D02、D03、D04、D05、D06、D07、D08、D09、D11、D13 共 10 条决策。
- D2 明确拒绝 d1 的三项设计（单 `Tables` 合并、宏生成门清单、能力类型），也明确接受 d1 没看到的“全槽占位是层间依赖的必然结果”。这不是复述。

不是全量对照：

- D1 的 D01（启动信息以什么形态进入内核）、D10（系统调用入口如何组织）、D12（页表重建是否进入本阶段）、D14（两行 `memset` 与重启路径）在 D2 报告里没有逐条对照记录。D2 的 DIFF 表第 5 行提到 memmap 往返、第 6 行提到 syscall 的两 trait 分担，但没有把 D01/D10/D12/D14 的取舍写成表。
- D1 的 14 条 `DECISION` 没有进台账；D2 的 19 条 `FINDING` 进了台账。结果是“设计提案的淘汰理由”只存在于 D2 的叙述里，不在轨迹里。下一轮若有人问“为什么不用 D1 的单 `Tables` 方案”，只能去读 942 行的 D2 报告，不能在台账里查到一行。

### 5.2 D1 的 14 条 DECISION 未进台账是否属于 B-17 的轨迹门缺口

是，而且是 B-17 的原样复发。B-17 的原文是“反证主张与设计提案入台账”，期望探测器是“每个面板文件要么产出至少一条可处置项、要么在 `loop.md §1` 显式豁免并说明理由；反证主张单独入台账（`REFUTE-xx`）”。本轮的实际情况：

- `refute.md` 的 6 条 CLAIM 有一个独立的附加表，算部分满足；但该表没有处置类、没有严重度列，CLAIM 03/04 的“无动作”没有类别。
- `design-d1.md` 的 14 条 DECISION 没有附加表、没有豁免说明；`loop.md:20` 只说“14 条 DECISION（不是 FINDING）”，这不是 B-17 要求的“显式豁免并说明理由”，而是用标题形状绕开。
- `eval-trace-check.py` 对这两个文件只报 WARN，结论仍 PASS；这与 D02 meta-review 判 Trace integrity FAIL 的机制完全相同。

所以：B-17 仍是 OPEN，D03 是它的第二次连续复现。按盲区账本“连续两次复现”的门槛，它已经可以从 OPEN 升为“DETECTED（作为缺口本身被稳定发现）”，但更好的是直接把它修掉：在 `defect-ledger.md` 加一个 `## P-DESIGN 提案处置` 表，14 行，每行写“接受/拒绝/合并 + 理由 + D2 对应发现或‘无’”；在 `## P-REFUTE 附加` 表加“处置类”和“严重度”两列。这是零工具改动、只改台账的收口，成本低于改 `eval-trace-check.py`。

## 六、BLIND-SPOT-DELTA

### 新增（候选，尚未入账）

- **B-20 修复语句自身的事实未复算。** 证据：`FACT-05` 的修复在 §3.7 新加“C 的总门数会变成 84/86”，按 C 源复算是 90/92；`FACT-04` 的修复把载体注释的判断归给生产注释，而生产注释仍写 UNDEFINED。两者都通过了文档五门。类别：修复侧事实 / 可复算读数。期望探测器：L4.5 修复复验，或一个 `doc-claim-recheck.sh` 对每个新数字/新引文重放推导。与 B-08 的关系：B-08 是“文档里的旧读数漂移”，B-20 是“修复引入的新读数没有复算”；建议单列，否则 B-08 的“重跑旧命令”接不住新句子。
- **B-21 文档修正与被引用的过期代码注释脱钩。** 证据：`FACT-04`；文档说“理由写在 `protection.rs` 的 `fn init` 的注释里”，注释说 UNDEFINED，载体说不是 UNDEFINED。类别：文档-代码同步 / 引文归属。期望探测器：`fix-guard` 增加一条“引文归属核对”：凡是文档写“理由写在 X 的注释里”，必须把 X 的注释与文档句逐字对照；不一致时要么改注释，要么在文档里标注冲突。与 B-12 的关系：B-12 是“引文块被润色”，B-21 是“转述被归错来源”；B-12 的逐块 diff 接不住转述。
- **B-22 处置类按来源角色分配，而非按严重度。** 证据：P-REFUTE 的 CLAIM 05 P1（GsArea `user_rsp` 被入口写）被转出，而同轮 FACT 的 P1 全部 ①；READER-B 的 15 条全部 ②，即使 A-06/B-05/C-05 这种一句话可闭合的归属错误也是 ②；READER-B-14（与 FACT-14 同一处）因为 ID 互换被记 ②。类别：流程 / 分诊。期望探测器：L3 归因按“严重度 + 可修性”排序，不按来源角色；同一处缺陷用 `dup-of` 合并，合并后的处置取最高严重度。

### 关闭

- 无。本轮没有任何一条盲区达到“最近两次回归复现”的 `DETECTED` 门槛，也没有任何一条机械封堵到 `CLOSED`。

### 仍 OPEN（本轮有证据但未入账）

| 盲区 | 本轮证据 | 账本状态 |
|---|---|---|
| B-05 假反事实 | FACT-15（EFER 窗口不可达）、FACT-16（`prot_init_done` 前提不成立）、READER-A-10、READER-C-16 | `OPEN`，检测证据栏仍为“（待填）”；这是第一次强命中，D04 若再命中可升 `DETECTED` |
| B-08 可复算读数漂移 | FACT-02（七个 slice 对“五块”）、FACT-05（84/86 对 90/92）、FACT-08（113 对 114）、FACT-12（9 文件 56 对 crate 375/54 + tests 94/20） | `OPEN`，证据栏仍停在 D01 的 125/175；本轮新增“修复引入新读数”这一子类，见 B-20 |
| B-09 门绿但事错 | `eval-trace-check.py` 对 `design-d1.md`/`refute.md` 报 WARN 后仍 PASS；文档五门全绿而 §2.5/§4.7/§5.4 M10 仍有被本轮证据推翻的句子 | `OPEN`，无新证据行 |
| B-13 硬件机制泄漏到共享语义层 | DESIGN-18(b)：x86 向量名 `DIVIDE_ERROR`/`PAGE_FAULT`/`KERN_CALL_VECTOR`/`IPC_VECTOR` 放在 `os/arch/src/arch/trap_entry.rs` 这个跨架构模块 | `OPEN`，第一次强命中；D04 复现可升 `DETECTED` |
| B-14 测试计数口径 | FACT-12 仍用 `grep -c`，没有用 `cargo test -- --list`；`cargo test -p minix-arch --no-run` 45 个错误 | `OPEN`，未满足账本要求的复验命令 |
| B-15 公共入口调用方枚举 | 本轮未改代码，未检验 | `OPEN`，状态不变 |
| B-16 材料违规分级 | 本轮无材料违规；D2 读了 7 个额外 Rust 文件与 3 个 C 文件，披露了但没有分级 | `OPEN`，规则待 V2 |
| B-17 轨迹门只认 `## FINDING` | `design-d1.md` 14 条 DECISION 无处置行；`refute.md` 6 条 CLAIM 只有无类别的附加表；工具 WARN+PASS | `OPEN`，第二次连续复现（D02、D03） |
| B-18 跨角色重复计数 | 至少 12 组重复：FACT-03/B-01；FACT-14/A-09/B-14/C-02；FACT-15/A-10；FACT-16/A-04/B-02/C-15；FACT-06/07/C-03；FACT-09/DESIGN-11；FACT-01/DESIGN-10；A-06/B-05/C-05；B-08/C-12；B-11/C-08；B-13/B-14 的 ID 互换；C-03/FACT-06 的 M10 残留 | `OPEN`，第二次连续复现 |
| B-19 材料矩阵未覆盖作者前置区 | 本轮 D1 没有读 `reader-profile.md`；D2 也没有读；没有发生 D02 那种泄漏 | `OPEN`，未检验 |

已 `DETECTED` 且本轮继续复现的：B-01（DESIGN-12..16 的死抽象/翻译残留）、B-02（同左）、B-03（READER-C 的跨节矛盾）、B-04（READER-A/B/C 的理解障碍）、B-10（`re-litigation` 0）、B-11（`adjacent 阻塞` 0）、B-12（FACT-04/11 的引文问题）。这些条目的“检测证据”栏仍只有 D01/D02，没有加 D03；按账本第 3 行的要求，元评审应把 D03 的证据回写。

## 七、建议变更

V2 每次最多晋升 2 条（`WORKFLOW-VERSIONS.md:4`）。下面把两条晋升、三条排队分开；案例 ID 用 D03 的台账 ID。

### V2 晋升候选

**C-20 轨迹门 v2：设计提案与反证主张入台账，每个面板文件要么有可处置项、要么显式豁免。**

- 案例 ID：D03 `design-d1.md` 的 14 条 DECISION、`refute.md` 的 6 条 CLAIM；D02 的 `design-d1.md` 12 条 DECISION、`refute.md` 2 P1/9 P2/7 观察。
- 目标文件：`tools/eval-trace-check.py`、`rewrite-notes/coordination/review-eval/runs/*/defect-ledger.md` 的台账模板、`prompt/review-rules/review-panel.md` §7、`prompt/skill/review-loop.md` L4/L6。
- 期望效果：每个面板文件必须产出至少一条可处置项，或在 `loop.md §1` 写“无处置项 + 理由”；`DECISION` 与 `CLAIM` 也解析成行，`CLAIM` 行带严重度与处置类；没有索引的报告 = FAIL，不是 WARN。
- 回归风险：V1 的自由格式面板会失败；给一个 `--allow-unindexed` 迁移开关，并在 `WORKFLOW-VERSIONS.md` 记一次。指标分母会从“FINDING 数”变成“面板项数”，D01/D02 的历史百分比不可直接比较，需要在语料里保留两套读数。
- 替代/合并：替代 C-02（当前只查 `## FINDING` 形状）；不新增第二套轨迹工具，只扩展现有的 `eval-trace-check.py`。

**C-23 禁止 follow-up 大桶：每条转出项给具体目的地与验收判据。**

- 案例 ID：D03 `D03-F12a`/`D03-F12b`/`D03-F12c` 三行压了 10/15/15 条读者发现；`D03-F02`/`D03-F11` 的目的地是“code batch”“test batch”。
- 目标文件：`prompt/skill/review-loop.md` L5、`prompt/review-rules/review-evals.md` 的转出表、`rewrite-notes/coordination/review-eval/runs/*/follow-ups.md` 模板。
- 期望效果：每条转出项一行，写“目标文件/函数、触发条件、验收判据（测试名或命令）、优先级”；三行桶拆成 40 行（可以按同一文件合并，但每行必须可验收）；没有验收判据的行判 FAIL。
- 回归风险：台账行数从 15 行涨到 60–80 行，主 agent 的整理成本上升；用“同一目标文件可合并、验收判据不可省”折中。
- 替代/合并：替代当前“教学债批次/闭环节/code batch”三个桶；与 C-20 的“处置索引”共用同一份台账，不新增第二份 follow-up 文件。

### 排队候选（D04 边界再判）

**C-24（原候选，D03 证据加强）：P1 文档侧声明同轮降级。** 案例：DESIGN-02 的 §3.1 “MAX_CPUS=8”没有披露 kernel 32；P-REFUTE CLAIM 05 的 §4.5 “入口汇编只读”没有改。目标：`prompt/review-rules/fix-guard.md` 与 `review-process.md` 的 L3/L4。期望：代码转出时，文档里与该代码行为绑定的声明必须同轮改成“当前实现/待修/差异”，不能留旧断言。回归风险：文档轮会多出代码相关的句子，可能触碰“不为漂亮改文档”；用“只改被发现点名的句子”限制。替代/合并：并入 fix-guard 的“一次一条”与 Doc Code Sync 的既有检查，不新增门；它与 C-04（合并三套读者装置）没有重叠，不合并。

**C-25（新）：修复复验（L4.5）。** 案例：FACT-05 的 84/86、FACT-04 的注释归属。目标：`prompt/skill/review-loop.md` L4 与 L6 之间、`tools/eval-trace-check.py` 之后。期望：未参与修复的实例对 staged diff 逐条重放发现点名的命令，并对每个新数字/新引文给出独立推导；不通过则回到 L3。回归风险：成本增加一个 agent；用“只复核 ① 行与新增数字/引文”控制范围。替代/合并：替代当前的“主 agent 自跑五门 + 同一 agent VERIFY-CHECK”；与 D-012 的“同 agent 验证需声明”合并，不另立一套。

**C-26（新）：引文归属核对。** 案例：FACT-04。目标：`prompt/review-rules/fix-guard.md` 与 `review-doc-checklist.md` §4。期望：文档里“理由写在 X 的注释里”这类转述，必须与 X 的注释逐字对照；不一致时改注释或标注冲突。回归风险：需要读被引用的源文件，文档轮会多一步；用“只核对被发现点名的引文”限制。替代/合并：并入 Doc Code Sync 的“符号锚点”项，不新增第八项；与 B-12 的“引文块逐块 diff”共用同一套 diff 命令。

## 八、局限声明

- 本报告由单个 meta-review agent 完成，没有第二个 agent 独立复核。所有引用的命令都在本会话重放过，输出以当前工作树为准；这是“同 agent 验证 + 已重放命令”，不是跨 agent 验证。
- 抽样 25/81 = 30.9%；处置一致性 17/25 = 68.0%（严格口径），宽口径 22/25 = 88.0%，都低于 90%，按 D-012 与任务要求写 `NOT PASS`。
- 抽样以有明确文档位置的 ① 行为主（24 条 ① + 1 条 ②）；四条 P1 设计转出在第四节单独裁决，没有计入 25 条的百分比；57 条 ② 里只抽了 8 条做教学价值判断，没有逐条判定“真教学债/文风偏好”。因此本报告能证明“① 行有 7 条未完全闭合”，不能证明“57 条 ② 全是教学债”。
- 未跑完整测试；只跑了 `cargo test -p minix-arch --no-run`（编译失败，45 个错误）与文档门（`anchor-resolve.sh`、`doc-code-map.sh`、`doc-ref-check.sh`、`doc-code-density-check.sh`、`doc-style-lint.sh --diff`、`eval-trace-check.py`）；没有跑 QEMU，没有跑 `cargo test -p minix-kernel --features mock` 的全量。
- 未读 `prompt/review-rules/` 的规范源；判断依据是 `WORKFLOW-VERSIONS.md`、`CORPUS.md`、`BLIND-SPOTS.md`、`DECISIONS.md`、本轮的 `loop.md`/`metrics.md`/`defect-ledger.md`/`follow-ups.md`、7 份面板报告、staged diff、以及为复核事实而读的 `os/` 与 `minix3/` 源文件。规则文本的引用来自面板报告与元数据，不是我重新解释的规则。
- 本报告只写入 `rewrite-notes/coordination/review-eval/meta/03-kmain-cstart-review.md`；没有修改仓库中的其他文件。

## 附录 A：抽样复判表

| # | 台账 ID | 台账类 | 独立复判 | 依据 |
|---:|---|---|---|---|
| 1 | FACT-01 | ① | 成立 | §4.1 已改；`grep -rn FREE_MEMMAP os/kernel/src` 只剩 `memmap.rs:523` 的注释；`lib.rs:1450-1460` Phase A.2 为空；生产代码无 `add_memmap` 调用 |
| 2 | FACT-02 | ① | 不成立（部分） | §4.1 已改七个 slice；§4.7 `:1185` 仍写“五个载荷”，而发现点名了 §4.7 |
| 3 | FACT-03 | ① | 成立 | `main.c:147-327` 的 `cstart` 之后还有 BKL/proc_init/boot 循环/smp_init/bsp_finish_booting；§2.2 已改 |
| 4 | FACT-04 | ① | 不成立（证据错位） | §4.6 说“理由写在 `protection.rs` 的 `fn init` 注释里……不是 UNDEFINED”，但 `protection.rs:140-144` 仍写 UNDEFINED；载体 `test-protection-aarch64/src/main.rs:376-377` 才写 accessible |
| 5 | FACT-05 | ① | 不成立（部分 + 新错） | §3.7 加了限定；§2.5 `:589` 仍写“39 即全部门”，§4.7 `:1196` 仍写“增加 LAPIC/调度向量”；新加的 84/86 按 C 源复算应为 90/92 |
| 6 | FACT-06 | ① | 不成立（部分） | “software only”已改；§3.6 `:896` 的“把 kmain 的全部工作划成 Phase”与缺失的 B.5/C.5/C.6 没有改 |
| 7 | FACT-07 | ① | 成立 | `arch_system.c:246-288` 有 `apic_single_cpu_init` 与 `apic_idt_init`；§2.2 已改 |
| 8 | FACT-08 | ① | 成立 | `grep -n const IDT_ENTRIES os/arch/src/x86_64/trap_entry.rs` 输出 114；文档两处已对齐 |
| 9 | FACT-09 | ① | 成立 | `has_errcode` 只被 `test_errcode_classification_matches_sdm` 使用；§4.4 已改 |
| 10 | FACT-10 | ① | 成立 | §4.6 已改成“从 EL0 进 EL1 时硬件选中 banked `SP_EL1`；不存在 x86 TSS 那样的内存取栈” |
| 11 | FACT-11 | ① | 成立 | `os/arch/src/riscv64/protection.rs:70` 是 `pub(crate) fn`；文档已补 |
| 12 | FACT-12 | ① | 成立 | 9 文件合计 56；crate 全量 `src/*.rs` 375/54、`tests/*.rs` 94/20；§5.1 已补子集说明 |
| 13 | FACT-13 | ① | 不成立（部分） | §7.2/§7.3 补了 5 个文件，但 §2.6 引用的 `servers/vm/pagetable.c` 与 §4.6 引用的 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs` 仍不在清单 |
| 14 | FACT-14 | ① | 成立 | §3.6 的“出入在 §5.3 有披露”已改成“已同步，没有出入可披露”；三处一致 |
| 15 | FACT-15 | ① | 成立 | §4.2 已加“窗口内没有 CPL3 用户态（Phase B，IF=0）”；与 `configure_syscall` 调用点一致 |
| 16 | FACT-16 | ① | 成立 | §2.8 已加“前提不成立：`prot_init_done` 的唯一读者 `copy_trampoline` 由 `smp_start_aps` 在 `smp_init` 内调用，而 `smp_init` 在 `cstart` 之后”；与 `main.c:311-316` 一致 |
| 17 | DESIGN-10 | ① | 成立 | 同 FACT-01；§4.1 已改 |
| 18 | DESIGN-11 | ① | 成立 | 同 FACT-09；§4.4 已改 |
| 19 | READER-A-09 | ① | 成立 | 同 FACT-14；§3.6/§4.2/§5.3 三处一致 |
| 20 | READER-A-10 | ① | 成立 | 同 FACT-15；§4.2 已加可达性 |
| 21 | READER-B-01 | ① | 成立 | 同 FACT-03；§2.2 已改 |
| 22 | READER-B-13 | ① | 不成立（ID 错配） | reader-b 的 B-13 是 §2.2 表格与反事实重复（`reader-b.md:291-309`），不是 FACT-14 的 §3.6 悬空指针；台账把它记成“同 FACT-14”，实际没有修 |
| 23 | READER-B-14 | ② | 不成立（应为 ①） | reader-b 的 B-14 才是 §3.6/§5.3/§4.2 的悬空指针（`reader-b.md:311-329`），与 FACT-14 同一处，已被修；台账把它记 ② 转出 |
| 24 | READER-C-02 | ① | 成立 | 同 FACT-14；§3.6 已改 |
| 25 | READER-C-03 | ① | 不成立（部分） | §4.7 改成“`intr.init()` 已编程 LAPIC/IOAPIC”；§5.4 M10 `:1274` 仍写“控制器未编程、IF=0”，同一文档新造矛盾 |

计数：成立 17 条（第 1、3、7、8、9、10、11、12、14、15、16、17、18、19、20、21、24 行）；不成立 8 条（第 2、4、5、6、13、22、23、25 行）。严格一致性 17/25 = 68.0%。宽口径把“至少有一个对应改动”算成立，FACT-02/05/06/13 与 READER-C-03 可算入，FACT-04 仍不成立，READER-B-13/B-14 仍不成立，得 22/25 = 88.0%，仍低于 90%。

## 附录 B：关键命令与输出

| 命令 | 输出 |
|---|---|
| `python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart` | `WARN design-d1.md 没有 ## FINDING <ID> 行`、`WARN refute.md 没有 ## FINDING <ID> 行`、`PASS 面板报告解析到 81 条发现`、`PASS 6 / WARN 2 / FAIL 0`、退出码 0 |
| `grep -c '^## FINDING' tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/*.md` | fact 16、design-d2 19、reader-a 12、reader-b 17、reader-c 17、design-d1 0、refute 0 |
| `grep -c '^## DECISION' tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/design-d1.md` | 14 |
| `grep -c '^## CLAIM' tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/refute.md` | 6 |
| `grep -rn "FREE_MEMMAP" os/kernel/src` | `memmap.rs:523` 一行注释 |
| `grep -n "let mut k = \*kernel_info" os/kernel/src/lib.rs` | `1258:        let mut k = *kernel_info;` |
| `grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs` | `114:const IDT_ENTRIES: usize = 256;` |
| `awk '/static struct gate_table_s gate_table_ioapic/,/^};/' minix3/minix/kernel/arch/i386/apic.c \| grep -c apic_hwint` | 64 |
| `awk '/static struct gate_table_s gate_table_common/,/^};/' minix3/minix/kernel/arch/i386/apic.c \| grep -c 'VECTOR\|_orig\|_um'` | 4 |
| `awk '/static struct gate_table_s gate_table_smp/,/^};/' minix3/minix/kernel/arch/i386/apic.c \| grep -c apic_ipi` | 2 |
| `grep -n "int_gate_idt(APIC_TIMER_INT_VECTOR" minix3/minix/kernel/arch/i386/apic.c` | `913` 一行 |
| `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `anchors=80 resolved=80 zero-def=0 multi-def=0`，退出码 0 |
| `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check` | 22/22 通过；基线 `HEAD` 版本是 21 个文件 |
| `bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 内部章节引用均可解析，退出码 0 |
| `bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 4 个可判章均有码/命令，阻断项通过，退出码 0 |
| `bash tools/doc-style-lint.sh --diff` | 1 个文件 / 46 个新增行，error=0，退出码 0 |
| `cd os && cargo test -p minix-arch --no-run` | 45 个错误，`KernelInfo` 初始化缺 `memmap_sources`/`startup_objects` |

## 附录 C：本轮 repeat 组的归并表

| 组 | 成员 | 同一处缺陷 |
|---|---|---|
| 1 | FACT-03 / READER-B-01 | `cstart` 不是 `kmain` 的最后一个动作 |
| 2 | FACT-14 / READER-A-09 / READER-B-14 / READER-C-02 | §3.6 指向 §5.3 的“注释与函数体出入”悬空 |
| 3 | FACT-15 / READER-A-10 | EFER.SCE 窗口不可达 |
| 4 | FACT-16 / READER-A-04 / READER-B-02 / READER-C-15 | `prot_init_done` 反事实的因果链不成立 |
| 5 | FACT-06 / FACT-07 / READER-C-03 | 控制器状态与 `arch_init` 的 APIC 分支 |
| 6 | FACT-01 / DESIGN-10 | §4.1 的旧 `FREE_MEMMAP`/Phase A.2 链 |
| 7 | FACT-09 / DESIGN-11 | `STUB_VECTORS` 的 errcode 布尔没有生产读者 |
| 8 | A-06 / B-05 / C-05 | §3.2 把系统调用入口归到 §2.5 |
| 9 | B-08 / C-12 | §3.1“三张表”的表里有五行 |
| 10 | B-11 / C-08 | §2.1 五步与 §4.1/§4.7 的映射不对账 |
| 11 | READER-B-13 / READER-B-14 | 台账把两条的内容互换了 |
| 12 | P-REFUTE CLAIM 02 / FACT-01 / FACT-07 | 反证成功项并入 FACT-01/07 |
