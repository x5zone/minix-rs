# D03 回归审查 —— 03-kmain-cstart（FREEZE 后）

- 审查日期：2026-11-13
- 当前 HEAD：`d6c37bce0`
- 冻结提交：`88ac92c60`（R2 确认轮收口）
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 台账：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart/defect-ledger.md`
- 范围：文档全文、FREEZE、R2 台账、`tools/doc-code-map.sh` 列出的 21 个文件与 `minix3/` 下的 C 源
- **结论：FAIL**

FAIL 的依据不是文档被改过（哈希与 FREEZE 一致），而是抽样复验发现一条 P1 在台账中被记为“已修”，正文却仍是原样（REOPEN），另有一条 R2 修复引入的 P2 事实错误（诊断命令的调用者写错）。

## 1. FREEZE 复核

| 项 | FREEZE 记录 | 本次实测 | 结果 |
|---|---|---|---|
| 文档 sha256 | `da85d6cfedbce7f77c2911a827c71210cffad4955d08ab5a0b2d57a9f3438caa` | `sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md` 输出同值 | 一致 |
| 文档行数 | 1393 | `wc -l` → 1393 | 一致 |
| 关联代码存在性 | `doc-code-map.sh --check` 21/21 | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check` → 共 21 个文件；存在性 21/21 通过 | 一致 |
| anchor | 75/75 | `bash tools/anchor-resolve.sh --check <doc>` → `anchors=75 resolved=75 zero-def=0 multi-def=0` | 一致 |
| 内部引用 | structure/ref 通过 | `bash tools/doc-ref-check.sh --check <doc>` → 内部章节引用均可解析 | 一致 |
| R2 `--round` 门 | PASS（11 PASS / 1 WARN / 0 FAIL） | `python3 -I tools/eval-trace-check.py <run-dir> --round R2` → 同一分布，`结论: PASS`，exit 0 | 一致 |
| 冻结后代码改动 | 无 | `git diff --stat 88ac92c60..HEAD` 只含 `rewrite-notes/` 的文档、FREEZE、台账、metrics；无 `os/` 改动 | 一致 |

R2 的唯一 WARN 是 `D03-design-batch` 的已批准延期（D-053）；`DECISIONS.md` 第 135 行有判决账行，`OPEN-DEBT.md` 有对应条目。WARN 不是回归。

## 2. 抽样复验（10 条台账已修项）

| 发现 ID | 原严重度 | 复验命令/证据 | 结果 |
|---|---|---|---|
| RD3-01（RD3-FACT-01） | P0 | `grep -c "全局自拷贝" doc` → 0；`grep -c "暂存副本 → 最终全局" doc` → 1；doc:141 已改为暂存副本到最终全局的搬运 | PASS |
| RD3-02（RD3-FACT-02） | P0 | `grep -c "把同一句话再说一遍" doc` → 0；`grep -c "承重赋值" doc` → 1；doc:160/169 区分未分页副本与普通副本，`kmain` 的置 1 是承重赋值 | PASS |
| RD3-03（RD3-FACT-03） | P0 | `grep -c "KERNEL_MAY_ALLOC" doc` → 0；`grep -c "D01-F08 淘汰" doc` → 0（R2 把措辞改成“D01-F08 删除”，见 doc:974/1198）；`os/kernel/src/lib.rs:3916-3919` 是 FreeRangeSet/调用图的现行注释 | PASS（旧串归零；新串因后续轮改写为同义表述，按现文复核） |
| RD3-08（RD3-FACT-08） | P0 | `grep -c "同一个静态页目录数组" doc` → 0；`grep -c "不是同一数组的原地重填" doc` → 1；doc:624/671 写两份 `pg_utils` 对象、两张 `pagedir`、`pg_load` 切 CR3 | PASS |
| RD3-R2-01 | P0 | doc:974 明写 `bootstrap_start/len` 已随 D01-F03 从 `KernelInfo` 删除；`os/kernel/src/lib.rs:1858-1863` 注释同口径（字段已删，无可回收对象）；`grep -c "bootstrap_start" doc` → 1（仅删除陈述） | PASS |
| RD3-R2-02 | P0 | doc:868-880 写 `GsArea.user_rsp` 由 SYSCALL 入口写；`os/arch/src/x86_64/trap_stub.rs:376` 为 `mov qword ptr gs:{gs_user_rsp}, rsp`；同文件 `program_gs`（684-698）只写 `kernel_stack_top`/`cpu_id` 并把 `user_rsp` 置 0 | PASS |
| RD3-R2-03 | P0 | doc:956 写 Rust 41 门、无 IOAPIC 64 门表、0xF0/0xF1 与 C 相反；`sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs \| grep -cE '^\s+\(0x\|^\s+\([0-9]+,'` → 41；`grep -rn "gate_table_ioapic" os/arch/src os/kernel/src os/plat/src` 无命中；`minix3/minix/kernel/arch/i386/apic.h:90-91` 为 0xF0=timer、0xF1=sched，`trap_stub.rs:207-214` 为 0xF0=sched、0xF1=timer | PASS |
| RD3-R2-25 | P1 | 该行处置写“已修：「本轮未跑」「D01-F08」「没有出入可披露」等评审残留”。逐条重放：doc:1279/1280 仍有“本轮未跑”；doc:80-81 仍有“（本篇不引节号）”两处；doc:958 仍有“正说明这一点”；doc:974/1198 仍有“D01-F08”。`git log -S "本轮未跑" -- <doc>` 显示该串引入后无删除提交 | **REOPEN** |
| RD3-R2-28 | P1 | doc:900 定义“Step 1（02 篇 §4.4 的 boot 阶段 Step 1）”；`grep -c "Step 8" doc` → 0，未再出现未定义编号 | PASS |
| RD3-R2-29 | P2 | 九个文件锚定 `#[test]` 逐文件为 16/13/11/2/1/3/2/1/6，合计 55；doc:1232 写 55 并说明 `riscv64/trap_stub.rs` 的 56−1 注释命中；arch 全量 373/54 与 `os/arch/tests` 87/21 与命令输出一致 | PASS |

复验方式说明：台账中标记为“冷读 CLOSED”的条目（RD3-R2-01/02/03 及多数 R2 reader 项）没有可重放的 grep 串，本次按“正文 + 源码位置”重读；能在现任源码定位的，按源码位置核对，未引入主观改写。

## 3. 关键事实源码再核对（3 条）

### KF-1 §2.1 两行 `memcpy` 不是自拷贝（doc:141-157）

为什么关键：这是 R1 的两条 P0 根因之一；§2.1 的“次序承重”（doc:186）与反事实（doc:188）都建立在这份对象图上，写反则整节的因果链失效。

证据：

- `minix3/minix/kernel/arch/i386/pre_init.c:237`：`return &kinfo;`（`pre_init` 返回自己的 `.unpaged_bss` 副本）
- `minix3/minix/kernel/arch/i386/head.S:86-87`：`push %eax` → `call kmain`（返回值成为 `local_cbi`）
- `minix3/minix/kernel/main.c:115`：`void kmain(kinfo_t *local_cbi)`；`:128`：`memcpy(&kinfo, local_cbi, sizeof(kinfo));`
- `minix3/minix/kernel/arch/i386/pre_init.c:117`：`cbi->kmess = &kmess;`；`minix3/minix/kernel/glo.h:76`：`EXTERN int kernel_may_alloc;`

结论：文档写“`local_cbi` 指向 `pre_init` 的暂存副本，`memcpy` 搬进 `.usermapped` 最终全局”与源码一致。

### KF-2 §3.7 / §1.2 门数与 APIC 路径（doc:956-958）

为什么关键：这是 R2 三条 P0 之一；41 这个数字在 §1.2、§3.7、§4.4、§4.7 与 §5.1 多处引用，且“Rust 没有 64 门表”是跨架构对照的核心差异。

证据：

- C 门表：`awk '/gate_table_pic\[\]/,/};/' .../protect.c | grep -c 'hwint'` → 16；`awk '/gate_table_exceptions\[\]/,/};/' ... | grep -cE '_VECTOR'` → 23；合计 39。
- Rust：`STUB_VECTORS` 计数 41；`minix3/minix/kernel/arch/i386/apic.h:90-91` 的 0xF0/0xF1 角色与 Rust `os/arch/src/x86_64/trap_stub.rs:207-214` 相反；`os/plat/src/x86_64/interrupt.rs:349/488-490` 的注释与映射表确认只按 16 条 ISA IRQ 路由、PCI-level pins 屏蔽。
- Rust 源内无 `gate_table_ioapic` 或等价的 64 门表。

结论：文档的 39、41、41 = 39 − 1 + 3、以及“没有 64 门表”均与源码一致。

### KF-3 §4.2 `init_protection` 的装载次序（doc:984-1001）

为什么关键：§3.2（两个 trait 的顺序契约）、§3.3（地址稳定）、§3.7（门先装后 load）三条设计都声称在 `init_protection` 落地；次序写错会让三条设计同时失去代码依据。

证据（`os/kernel/src/lib.rs`）：

- `:2230` `CurrentProtection::init`；`:2231` `store_protection`；`:2239` `refresh_descriptors`；`:2240` `prot.load()`；`:2263` `CurrentTrapEntry::init`；`:2275` `install_trap_stubs`；`:2276` `configure_syscall`；`:2282` `with_trap_entry(|trap| trap.load())`。

结论：现码次序与文档节选一致（`store` 在 `load` 前、`refresh` 紧跟搬迁、`prot.load` 在 trap 初始化前、stub 安装在 IDT `load` 前）。

## 4. REOPEN

### REOPEN RD3-R2-25（P1，读者/评审残留）

- 台账处置：`defect-ledger.md` 的 RD3-R2-25 行写“已修：「本轮未跑」「D01-F08」「没有出入可披露」等评审残留”，复验列写“冷读 CLOSED（二次回读 RD3-R2-25）”。
- 实际正文（doc 当前行号）：
  - 1279：`… → **R03-T03**；本轮未跑 QEMU`
  - 1280：`… 由 \`test-smp-aps\` 端到端覆盖（本轮未跑）`
  - 80-81：两处“（本篇不引节号）”
  - 958：`… 门里保持占位 0 也能通过，正说明这一点`
  - 974、1198：两处 `D01-F08`
- 缺什么：台账所述的最小修复没有落到正文。原面板的最小修复是：`本轮未跑` 改为“截至本文定稿未附 QEMU 运行记录”；`D01-F08` 改为自包含描述；删“没有出入可披露”与“正说明这一点”；删两处“（本篇不引节号）”。当前只有“没有出入可披露”被改写，其余均仍在。
- 影响：这不是事实错误，而是读者面 P1——正文仍把评审框架（“本轮”、issue 编号、作者备注）当作文档内容。FREEZE 的“R2 台账 未闭环 = 0”对这一行不成立。

## FINDING REG-D03-01（P2）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1346`（§6.2 第四组，GDB 命令块）。
- 证据：该行写 `(gdb) finish  # 停在调用者（bsp_finish_booting 返回后）——IDT 的装载在函数体后半段`。生产内核里 `init_protection` 的唯一调用点是 `os/kernel/src/lib.rs:1784`，位于 `kmain`（`:1673`）的 Phase B 内；`bsp_finish_booting` 定义在 `:3806`，不调用 `init_protection`。`finish` 停下的调用者是 `kmain`，不是 `bsp_finish_booting`。
- 来历：该括号是 RD3-R2-30（P1，“`finish` 写成‘到函数返回前’，实际停在调用者”）的修复产物，提交 `193707da8` 把旧句改为“停在调用者（bsp_finish_booting 返回后）”；“停在调用者”改对了，调用者名字写错了。
- 影响：§6.2 的定位是“可复制命令与预期输出”。读者按注释预期自己回到 `bsp_finish_booting`，实际在 `kmain` 的 Phase B；后续读表寄存器时对“当前处在启动序列哪一段”的判断会偏移。命令本身仍可执行。
- 最小修复：把括号改成“停在调用者（`kmain` 的 Phase B）”，或改成“停在调用者（`kmain`，`init_protection` 的调用点）”；后半句“IDT 的装载在函数体后半段”保留。

## 5. 局限声明

- **同 agent 复核**：本次审查、此前面板与 certify 使用同一模型；没有跨模型独立复核。已重放的命令与源码行号列在上文，便于第三方复算。
- **未跑真机**：QEMU 三架构未跑。`test-protection`、`test-user-trap` 与 `test-smp-*` 的判据只按文档与源码核对，不构成运行时证据。
- **未重跑全部文档门**：只重跑了 FREEZE 明确列出的 sha/行数/`doc-code-map`/`anchor`/`doc-ref`/`eval-trace --round R2`；citation、style-lint、structure 与承载门未重跑，引用 FREEZE 结论。
- **cite 工具盲区仍在**：`D03-citation-tool-batch` 的 `/* */` 盲区是冻结时已批准的延期，本次未重开，也未用其它工具补做 42 个块的逐块校验。
- **设计批未重裁**：`D03-design-batch`（D-053）的代码项、`[ARCH]` 提案与 P3/SUGGESTION 不在本次范围；本次只确认它们仍以登记状态存在。
- **抽样不是穷举**：10 条样本覆盖 P0/P1/P2 与事实/代码/读者三类，但不等于 48 条全部可重放；未抽到的“冷读 CLOSED”行仍可能有同类误记。

---

# D03 回归审查（R3 确认轮后）

- 审查日期：2026-11-20
- 基线提交：`7e5f8ba5d`（`git rev-parse --short HEAD`；`git status --porcelain` 对本文档与 run 目录无未提交改动，另有 3 个无关未跟踪文件）
- 冻结提交：`7e5f8ba5d`（与 HEAD 同一提交，即 R3 V3.4 确认轮收口提交）
- 冻结记录：`FREEZE.md` 的 `## R3 确认轮（V3.4；D-066 授权的重开确认）` 节
- **结论：FAIL**

FAIL 定性：R3 的实质核验全部通过——8 处修改点在最终树上逐 hunk 在位（`git diff c2ba58596 7e5f8ba5d` 恰 10 hunk，+10/-10）、六门读数与 FREEZE 逐字一致、台账逐 ID 聚合与 metrics 完全吻合、certify 回执的 8 个行号锚在最终文档逐字在位、判决链（D-066）与制品链完整。不一致集中在冻结记录的指纹与历史标注层，共 3 条，逐条列于 §1。

## 1. FREEZE 记录复核（表）

| 项 | FREEZE 记录 | 实测命令 | 实测读数 | 结果 |
|---|---|---|---|---|
| 文档行数 | 1396 | `wc -l rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 1396 | 一致 |
| 文档 sha256（R3 复核后） | `42646faa93f4…5e918` | `sha256sum` 同文件 | `64659b6c1a9b…60f` | **不一致（P0-记录）** |
| `--round R3` | PASS | `python3 tools/eval-trace-check.py <run-dir> --round R3` | PASS 11 / WARN 2 / FAIL 0，结论 PASS | 一致 |
| `--round R1/R2` 仍 PASS | 仍 PASS | 同上，`--round R2` / `--round R1` | 均 PASS 11 / WARN 2 / FAIL 0 | 一致（WARN 数含义见 §2） |
| anchor 75/75 | 75/75 | `tools/anchor-resolve.sh --check <doc>` | anchors=75 resolved=75 zero-def=0 multi-def=0 | 一致 |
| citation `checked=32 failed=0` | checked=32 failed=0 | `python3 tools/doc-citation-check.py <doc>` | blocks=42 checked=32 failed=0 skipped=10 warned=0 | 一致 |
| structure / ref 通过 | 通过 | `doc-structure-check.py` / `doc-ref-check.sh --check` | 均 exit 0 | 一致 |
| style error=0 | error=0 | `tools/doc-style-lint.sh <doc>` | error级=0 | 一致 |
| density 通过 | 通过 | `tools/doc-code-density-check.sh --check <doc>` | 承载门阻断项通过（4 可判章均有码/命令） | 一致 |
| doc-code-map 21/21 | 21/21 | `tools/doc-code-map.sh --check <doc>` | 存在性 21/21 通过 | 一致 |
| panel-index 6 文件哈希一致 | 一致 | `python3 tools/panel-index.py <run-dir>/panel/round3 --check` | 6 个文件哈希与计数一致，exit 0 | 一致 |
| 未闭环 = 0 | 0 | 见 §4 独立聚合脚本 | 0 | 一致 |
| OPEN-DEBT `R3-design-batch` 有 D-066 判决行与 2026-11-20 日期 | 有 | 读 `OPEN-DEBT.md` | 行在，含「用户批准 2026-11-20（代理裁决 D-066）」 | 一致 |
| DECISIONS.md 的 D-066 | 存在 | `grep -n "D-066" …/review-eval/DECISIONS.md` | L147 一整行判决（授权/处置/出口条件/已实施） | 一致 |
| 制品：panel/round3 6 文件 + INDEX、certify/round3-certify.md | 存在 | `ls panel/round3/ certify/` | 7 + 1 个文件全在 | 一致 |

**不一致项逐条列出（FAIL 依据）：**

1. **文档 sha256 指纹失实（§1 表第 2 行）。** `FREEZE.md` 头部记「文档 sha256：`42646faa93f43007…`（R3 复核后；R2 原值 `e54cad69…`）」。实测最终树与冻结提交 `7e5f8ba5d` 的值为 `64659b6c1a9bb4579979cf04e21e2bf63fc1751359bc79f2e2ae6c5d92a3160f`。对该文档全部 32 个提交版本逐一算 sha（`git log --format=%h -- <doc>` 循环 `git show <c>:<doc> | sha256sum`），无任何提交等于 `42646faa`；它也不是 run 目录 7 个制品中任何一个的 sha256。即该记录值在仓库内不可追溯：要么指纹误录，要么「R3 复核后」与提交 `7e5f8ba5d` 之间文档又被一次未记账的改动。内容未丢失的旁证：行数同为 1396；certify 回执引用的 8 个行号锚（L188/535/714/832/942/988/1235/1279）在最终文档逐字在位；diff 恰为 10 hunk 与 8 处修改点对应。
2. **历史标注口径错位。** FREEZE 头部「文档行数：1396（R2 时点）」与「R2 原值 `e54cad69…`」：实测 R2 冻结提交 `88ac92c60` 的文档是 1393 行 / `da85d6cf…`（`git show 88ac92c60:<doc> | wc -l` = 1393，`… | sha256sum | cut -c1-16` = da85d6cfedbce7f7）；`e54cad69`/1396 行是 `c2ba58596`（2026-11-18 完整 review 收口、R3 基线）的值，而 FREEZE 自身的「完整 review（2026-11-18）」节也称「最终文档 sha256 `e54cad69…`」。同一指纹在 FREEZE 内被标成两个时代，R2 时点的行数/指纹标注不实。
3. **台账 RD-R3-03 处置格「共 9 个门」与实况不符，且多处处置格截断。** 代码实况是 8 个 trap-gate 死写点：`sed -n '198,260p' os/arch/src/x86_64/trap_entry.rs | grep -cE 'is_trap=true|is_trap: true|\btrue\b'` = 8，向量 0、1、3、4、32、33、34、35；`OPEN-DEBT.md` 写「[DEL] init() 的 8 个 trap gate 死写点」，doc §3.7（L942）写「先把 8 个异常/软中断门写成 trap gate」——仅台账该格写 9，且该格句中断在「整条重写；C 」，RD-R3-04（断在「VirBytes::n 」）、RD-R3-05/06/07/10/11/24/25 等处置格同样中途截断。延期处置本身（② → R3-design-batch → D-066）成立，但处置正文不可完整复读。

## 2. 轨迹门与制品链

- `python3 tools/eval-trace-check.py <run-dir> --round R3` 全输出：panel 74 条 / loop 九节齐全 / L6+L6.5 齐 / 台账 330 条处置 / 无孤儿 / 两行 open-debt WARN（`D03-design-batch`、`R3-design-batch` 均为已批准延期待确认判决行）/ metrics 74 与逐 ID 聚合一致 / 未闭环 0 / edition-trial PASS。**结论 PASS**。`--round R2`、`--round R1` 同为 PASS（11/2/0）。注：R2 时代记录为 11/1/0，今天的第 2 个 WARN 来自 OPEN-DEBT 全局新增的 `R3-design-batch` 行——该工具的 open-debt 检查不随 `--round` 隔离，三轮同显 2 WARN，属工具口径而非记录失实。
- `python3 tools/panel-index.py <run-dir>/panel/round3 --check` → 「panel-index: 6 个文件哈希与计数一致」，exit 0。INDEX 计数独立求和：FINDING 15+2+3+0+2+0 = 22，SUGGESTION 0+2+16+21+7+0 = 46，CLAIM 6，合计 74，与 metrics R3 块「FINDING 22 + SUGGESTION 46 + CLAIM 6」同源一致。
- 制品链存在性：`panel/round3/` = INDEX.md + fact/design-d2/reader-a/reader-b/reader-c/refute 共 7 文件；`certify/round3-certify.md` 在（CLOSED 8 / STILL-BROKEN 0）；`DECISIONS.md` L147 D-066 在。FREEZE 声称的产物全部存在。

## 3. 六门读数

| 门 | 命令 | 读数 | exit | 与 FREEZE 对照 |
|---|---|---|---|---|
| anchor | `tools/anchor-resolve.sh --check <doc>` | anchors=75 resolved=75 zero-def=0 multi-def=0 | 0 | 75/75 一致 |
| citation | `python3 tools/doc-citation-check.py <doc>` | blocks=42 checked=32 failed=0 skipped=10 warned=0 | 0 | checked=32 failed=0 一致（10 跳过与 `/* */` 盲区登记一致） |
| structure | `python3 tools/doc-structure-check.py <doc>` | 表结构与内部 .md 链接均可解析 | 0 | 一致 |
| ref | `tools/doc-ref-check.sh --check <doc>` | 内部章节引用均可解析 | 0 | 一致 |
| style | `tools/doc-style-lint.sh <doc>` | error级=0 | 0 | 一致 |
| density | `tools/doc-code-density-check.sh --check <doc>` | 承载门阻断项通过（4/4 可判章有码/命令） | 0 | 一致 |
| code-map | `tools/doc-code-map.sh --check <doc>` | 21 个文件存在性 21/21 | 0 | 一致 |

## 4. 未闭环与债务

- 独立聚合脚本（解析台账 `## R3 确认轮` 节 29 行）：根因行数 29、RD-R3-01..29 连续无缺、发现总数 74、唯一发现 ID 74、按处置类（按发现条数）① 5 / ② 5 / ④ 3 / SUGG 56 / FP 5、⑤/⑥/OPEN 行 = 0。与 metrics R3 块的「发现总数 74、唯一根因数 29、本轮新准入 29、处置 ① 5 / ② 5 / ④ 3 / FP 5 / SUGG 56、未闭环 0」逐字段一致；eval-trace 亦判「metrics 各分档与台账本轮逐 ID 聚合一致（74）」。
- `OPEN-DEBT.md` 两行均为已批准延期：`D03-design-batch`（D-053，2026-11-12）、`R3-design-batch`（D-066，2026-11-20；5 条设计 P2 名单 + 2 条 ARCH 提案 RD03-A1/A2，与 D-066 判决文本一致）。
- `DECISIONS.md` L147 D-066 判决行完整：重开授权（用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`）、edition 升 V3.4、5 条设计 P2 记 `R3-design-batch`、2 条 ARCH 待亲裁、状态「已实施」、重开条件（结构性新根因或第 2 次确认仍新准入 > 0 → NOT CONVERGING）。
- 未闭环 = 0 成立；ARCH 提案 2 条挂账待用户亲裁，FREEZE 已声明不阻断。

## 5. 一致性抽查（6 行，覆盖 ①/②/④/SUGG/FP 五类）

| 台账行 | 类 | 复放命令与读数 | 结果 |
|---|---|---|---|
| RD-R3-01（①，fact 计数口径） | ① | `grep -rE '^[[:space:]]*#\[test\]' os/arch/src --include='*.rs' \| wc -l` = 373、`-rl` 文件数 54；宽口径 `grep -r '#\[test\]' …` = 374/54；`grep -h -c '#\[test\]' os/arch/tests/*.rs \| awk` = 95、tests 文件 21、tests 行首 87。doc L1235「行首 `#[test]` 属性」口径句在位，「374 处/54 文件」「95 处/21 文件」各 1 处 | 复现 |
| RD-R3-03（②，design P2 延期） | ② | `OPEN-DEBT.md` R3-design-batch 行含 D-066 + 2026-11-20 + 5 条设计 P2 名单，与 D-066 判决一致；但台账处置格写「共 9 个门」vs 代码 8 个 `set_gate(..., true)` 写点（见 §1 不一致 3） | 延期成立；处置格数字误记 |
| RD-R3-09（④，reader P2 tss_init 参数位次） | ④ | doc L714「第二参 `&k_boot_stktop`，第一参是 CPU 序号 0」在位；`grep -n "tss_init(0, &k_boot_stktop)" minix3/minix/kernel/arch/i386/protect.c` → `338:  tss_init(0, &k_boot_stktop);` | 复现 |
| RD-R3-23（④，reader P2 lgdt→lidt 窗口） | ④ | doc「异常递送轴」1 处、L535 含「窗口终点也不是 `prot_init` 入口而是其内部的 `idt_reload`」；`sed -n '304,306p'` protect.c = `x86_lgdt(&gdt_desc);` / `idt_init();` / `idt_reload();`，`:268-270` `idt_reload` 内 `x86_lidt(&idt_desc)` | 复现 |
| RD-R3-11（SUGG，「按下的」生造措辞） | SUGG | `grep -c "按下的" <doc>` = 3——SUGG 未修，与台账「登记 suggestions.md 的 R3 段」处置一致（按 §4.0c 不阻断） | 一致（suggestions.md 本体不在允许读取清单，未核） |
| RD-R3-27/28（FP，refute 反证失败 5 条） | FP | `minix3/minix/kernel/arch/i386/apic.h` 90-91 行 = `APIC_TIMER_INT_VECTOR 0xf0` / `APIC_SMP_SCHED_PROC_VECTOR 0xf1`；`minix3/minix/kernel/arch/i386/include/archconst.h:45` = `COPROC_SEG_VECTOR 9`；`STUB_VECTORS`（trap_stub.rs:168 起）手工清点 41 项；`arch/protection.rs:99-101` 与 `arch/trap_entry.rs:109-111` 契约注释（load 先于 init）在位；`grep -rn "SP_EL1" os/arch/src/arm64/` 非注释写点 = 0；`os/arch/src/riscv64/trap_stub.rs:536` = `csrrw sp, sscratch, sp` | 复现 |

另核 RD-R3-29（①，CLAIM-16 文档半边）：doc L942「Rust 侧的门类型有**两个写点**：`X86_64TrapEntry::init` 先把 8 个……」在位，与 `trap_entry.rs` init 体 8 条 `set_gate(..., true)`（0/1/3/4/32/33/34/35）及 `set_handler` 整条重写互证。

## 6. 局限声明

- 同模型审计：本报告所有命令均在最终工作树重放，但判读者与 R3 主 agent 同模型族，未做跨工具独立复核；VERIFY-CHECK 的同 agent 局限在此同样适用。
- 面板正文（`panel/round3/*.md` 6 文件）与 `suggestions.md` 不在本次允许读取清单内：面板发现内容仅经 `panel-index --check`（哈希+计数）与 INDEX 求和间接覆盖；SUGG 56 条的「登记 suggestions.md」未逐条核对，仅抽 RD-R3-11 验证了 doc 侧未修的一致性。
- `42646faa` 无法在 32 个提交版本或现存制品中重构，因此「冻结记录定稿后、提交前文档是否被改动、改了什么」不可判定；仅能确认最终树满足全部实质核验与 certify 的 8 个行号锚。
- FREEZE/面板声称的「fact 实测 25 块引文逐字一致、约 90 项常量现算一致」为面板自报值，未逐项复算（面板正文不在允许清单）。
- QEMU 未跑，与 FREEZE 声明一致；R3 无代码行为改动，宿主测试与真机门不在本审计范围。
- eval-trace 的 open-debt WARN 为全局口径：`--round R1/R2` 下也会计入 R3-design-batch 行，按轮读数时该分项不随轮隔离。

**总判定：FAIL（记录层 3 条不一致，见 §1）；实质核验（修改点、六门、轨迹门、聚合、判决链、制品链、抽查复放）全部通过，R3 修复内容确认存在于最终工作树。**

## R3 回归审计的补修记录（主 agent，append-only）

上面是独立审计者的原始结论（FAIL，3 条记录层不一致）。按 §4h 已全部处置：

| # | 审计发现 | 处置 | 复验 |
|---|---|---|---|
| 1 | FREEZE 记 R3 修复后 sha `42646faa…`，最终树实为 `64659b6c…`；`42646faa` 在任何提交里都不存在 | 根因：主 agent 在算 sha 之后又改了 §4.2 引文标注（嵌套括号导致 citation 门 SKIP+1），FREEZE 沿用旧值。改为 `64659b6c…`（最终工作树实测） | `sha256sum` → `64659b6c…`，与 FREEZE 一致 |
| 2 | 「1396（R2 时点）」「R2 原值 e54cad69」错位：R2 冻结提交实为 1393/`da85d6cf`，`e54cad69`/1396 是 R3 面板基线 | 三个时点分行如实标注（R2 冻结 da85d6cf/1393 → R3 基线 e54cad69/1396 → R3 修复后 64659b6c/1396） | `git show 88ac92c60:...` 行数与 sha 由审计者实测；FREEZE 文本已改 |
| 3 | 台账 RD-R3-03 写「共 9 个门」，代码实况 8 个 | 改「8 个门」（与 doc L942、OPEN-DEBT 一致） | `grep -c "8 个门" defect-ledger.md` → 1；代码 `trap_entry.rs` 的 init 内 `set_gate(...,true)` 8 处 |

补修后实测：`--round R1/R2/R3` 三轮 PASS；anchor 75/75、citation `checked=32 failed=0`、panel-index 6 文件哈希一致。
