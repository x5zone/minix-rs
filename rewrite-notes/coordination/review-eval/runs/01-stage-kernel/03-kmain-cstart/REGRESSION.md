# D03 回归审查 —— 03-kmain-cstart（FREEZE 后）

- 审查日期：2026-11-13
- 当前 HEAD：`a4afb4927`
- 冻结提交：`b00c44292`（R2 确认轮收口）
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
| 冻结后代码改动 | 无 | `git diff --stat b00c44292..HEAD` 只含 `rewrite-notes/` 的文档、FREEZE、台账、metrics；无 `os/` 改动 | 一致 |

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
- 来历：该括号是 RD3-R2-30（P1，“`finish` 写成‘到函数返回前’，实际停在调用者”）的修复产物，提交 `70e6f6127` 把旧句改为“停在调用者（bsp_finish_booting 返回后）”；“停在调用者”改对了，调用者名字写错了。
- 影响：§6.2 的定位是“可复制命令与预期输出”。读者按注释预期自己回到 `bsp_finish_booting`，实际在 `kmain` 的 Phase B；后续读表寄存器时对“当前处在启动序列哪一段”的判断会偏移。命令本身仍可执行。
- 最小修复：把括号改成“停在调用者（`kmain` 的 Phase B）”，或改成“停在调用者（`kmain`，`init_protection` 的调用点）”；后半句“IDT 的装载在函数体后半段”保留。

## 5. 局限声明

- **同 agent 复核**：本次审查、此前面板与 certify 使用同一模型；没有跨模型独立复核。已重放的命令与源码行号列在上文，便于第三方复算。
- **未跑真机**：QEMU 三架构未跑。`test-protection`、`test-user-trap` 与 `test-smp-*` 的判据只按文档与源码核对，不构成运行时证据。
- **未重跑全部文档门**：只重跑了 FREEZE 明确列出的 sha/行数/`doc-code-map`/`anchor`/`doc-ref`/`eval-trace --round R2`；citation、style-lint、structure 与承载门未重跑，引用 FREEZE 结论。
- **cite 工具盲区仍在**：`D03-citation-tool-batch` 的 `/* */` 盲区是冻结时已批准的延期，本次未重开，也未用其它工具补做 42 个块的逐块校验。
- **设计批未重裁**：`D03-design-batch`（D-053）的代码项、`[ARCH]` 提案与 P3/SUGGESTION 不在本次范围；本次只确认它们仍以登记状态存在。
- **抽样不是穷举**：10 条样本覆盖 P0/P1/P2 与事实/代码/读者三类，但不等于 48 条全部可重放；未抽到的“冷读 CLOSED”行仍可能有同类误记。
