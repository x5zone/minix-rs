# D03 kmain/cstart — P-FACT 事实审查（R1 full）

- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`（1688 行，sha256 `5b142de2…354f7`）
- 关联代码：`runs/01-stage-kernel/03-kmain-cstart/related-code.txt`（22 文件）
- 只读范围：文档全文、22 个关联 Rust 文件、文档引用的 C 源、机械门输出
- 冻结决策只取编号与结论：D-099、D-100、D-103、D-115（未读历史论证）
- 本报告只报有证据的事实项；未复算的计数在「阴性声明」里列明。

## 机械门读数（本轮实际运行）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check <doc>` | anchors=75 resolved=75 zero-def=0 multi-def=0 | PASS |
| 文档-代码映射 | `tools/doc-code-map.sh <doc> --check` | 22 个文件，存在性 22/22 | PASS |
| 表结构/内部链接 | `python3 tools/doc-structure-check.py <doc>` | 1 个文件可解析 | PASS |
| 内部章节引用 | `tools/doc-ref-check.sh --check <doc>` | 全部可解析 | PASS |
| 文风 | `tools/doc-style-lint.sh <doc>` | error 级 0 | PASS |
| 引文对账 | `python3 tools/doc-citation-check.py <doc>` | blocks=48 checked=32 failed=0 skipped=16 warned=0 | PASS（机械面） |
| 承载门 | `tools/doc-code-density-check.sh --check <doc>` | 4 个可判章均有码/命令，豁免 0 | PASS |

引文工具的 `skipped=16` 是未声明可机械对账的块；本报告对关键引文另做了手工核对（见下）。

---

## FINDING FACT-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1095`（§3.7 末段）
- 严重度：P1
- 问题：文档称「真机载体不调 `install_trap_stubs`（§5.2）、门里保持占位 0 也能通过」，与同一篇 §5.2 的两处陈述及 x86_64 载体源码相反。
- 影响：读者会据此认为 x86_64 真机链根本没有装真门，从而把「`load()` 不检查门有效性」的证据基础读成「载体没装门也能过」。实际上载体装了真门，只是 `load()` 自身仍不校验；结论（需新增装载前守卫）仍成立，但支撑它的事实句是错的，且与 §5.2 直接互相打脸。
- 证据或推理：E1。
  - 载体源码：`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:85-89` 依次 `CurrentTrapEntry::init()`、`install_trap_stubs(&mut trap)`、`trap.configure_syscall(...)`、`trap.load()`；`:167-174` 还读 GS_BASE 证明 `program_entry_base` 已执行。
  - 同篇 §5.2 `:1489`：「x86_64 版还调 `install_trap_stubs` 与 `program_entry_base`，门里是真入口」；`:1503`：「`test-protection` 的 x86_64 载体现在调 `install_trap_stubs` 装真入口」。
- 最小修复：把 `:1095` 的「真机载体不调 `install_trap_stubs`（§5.2）、门里保持占位 0 也能通过」改为「`load()` 本身不检查门有效性；x86_64 载体虽然调用了 `install_trap_stubs`，但 `load()` 内仍无覆盖断言，故负向仍无载体」。
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：若 `:1095` 的「真机载体」专指某个不调 installer 的载体（如 aarch64/riscv64），则需在句中点名；但同句挂了 `（§5.2）`，而 §5.2 的 x86_64 段明确相反，按现有文字读仍是自相矛盾。

## FINDING FACT-02（P1）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1322`（§4.4，紧接 `:1321`）
- 严重度：P1
- 问题：`:1322` 与 `:1321` 对同一件事给出互斥陈述：`1321` 说 `set_handler` 把 IST 一起写（2→1、8→2、其余 0）；`1322` 说 `set_handler` 的 IST「一律写 0（它没有 IST 参数）」，随后由 `set_gate_ist` 恢复。源码中 `set_handler` 确实写 IST，且 `set_gate_ist` 在全仓不存在。
- 影响：读者会得到两个相反的「唯一写者」模型；按 `:1322` 去代码里找 `set_gate_ist` 会找不到，进而怀疑 D-103 已落地的单写者设计没实现。两段还逐字重复了后半句（`TSS.ist[ist-1]` 与 `STUB_VECTORS` 布尔值），是明显的编辑残留。
- 证据或推理：E1。
  - `os/arch/src/x86_64/trap_entry.rs:330-343`：`set_handler` 内 `let ist = match vector.get() { 2 => 1, 8 => 2, _ => 0 };` 后调 `set_gate(...)`。
  - `os/arch/src/x86_64/trap_stub.rs:613-634`：`install_idt_handlers` 只调 `set_handler`，无第二次 IST 写。
  - `grep -rn "set_gate_ist" os/` 无匹配（仅 `os/target` 权限告警，无源码命中）。
- 最小修复：删除 `:1322` 整段；若想保留硬件索引说明，把「硬件按 `TSS.ist[ist-1]` 取值…」一句并入 `:1321`，并删掉 `set_gate_ist` 与「一律写 0」。
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：除非 `set_gate_ist` 在 `os/arch` 之外的生成代码里存在，或工作区有未入库的补丁；但 `trap_stub.rs` 的 `install_idt_handlers` 全文已逐行读过，没有第二次写。

## FINDING FACT-03（P2）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1260`（§4.4 首段）
- 严重度：P2
- 问题：句子以「C 的 39 条门清单对照不变」开头，随后把 `0xFF`、`0xF0`、`0xF1` 三个向量列入该清单；C 的 39 条门里没有这三个向量，它们是 Rust 侧新增的 LAPIC/调度向量。
- 影响：读者按这句复算 C 的门数会得到 42，与同篇 `:1093` 的「41 = 39 − 1 + 3」及 C 源的两张表（16+23）冲突；把 Rust 的扩展误记成 C 的既有事实，破坏了「C 真值」这条基线。
- 证据或推理：E1。
  - C 源孔径读数：`awk '/gate_table_pic\[\]/,/};/' …/protect.c | grep -c 'hwint'` = 16；`awk '/gate_table_exceptions\[\]/,/};/' … | grep -cE '_VECTOR'` = 23；合计 39，其中无 0xF0/0xF1/0xFF。
  - Rust 表：`os/arch/src/x86_64/trap_stub.rs:168-224` 的 `STUB_VECTORS` 共 41 项，末尾三项为 `0xF0`、`0xF1`、`0xFF`。
  - 同篇 `:1093` 已正确写明「Rust 比 C 少 1 条异常（向量 9）、多 3 条 LAPIC/调度相关向量」。
- 最小修复：把 `:1260` 拆成两句：「C 的 39 条门清单：异常区含 DPL=3 的 3/4，软中断区 32–35 全 DPL=3，PIC 区覆盖 0x50–0x57 与 0x70–0x77。Rust 在此之上多出 0xFF、0xF0、0xF1 三个单独向量。」
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：若作者本意是「Rust 的 41 条门清单对照 C 的 39 条」，则句首主语写错；无论哪种读法，现有文字都把三个 Rust 向量归给了 C。

## FINDING FACT-04（P2）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1165`（§4.2 引文块头注）与 `:1176` 的省略号
- 严重度：P2
- 问题：引文头注称「省略号处为 `register_current_arch` 的说明注释」，但 `:1176` 处的第一个省略号实际替换掉的是 `with_protection(|prot| prot.program_entry_base(...))` 调用和整个 Step 2 注释块；`register_current_arch` 的说明注释对应的是 `:1181` 前的第二个省略号。头注把两处省略混为一谈，且引文正文因此看不到 `program_entry_base`。
- 影响：读者按引文核对「保留…保护结构、门装配与装载的次序」时，会以为 `init_protection` 没有 `program_entry_base` 这一步；而 §5.1 的顺序钉与 §3.4/§4.5 都把它当作承重步骤。引文的省略说明与省略内容不一致，属于「节选未如实标注」。
- 证据或推理：E1。
  - 源码 `os/kernel/src/lib.rs:2398-2412`：`with_protection(|prot| prot.load());` 之后紧接 `with_protection(|prot| prot.program_entry_base(bsp_logical, kernel_info.kern_stack_top()));`，再往后才是 Step 2 注释与 `let mut trap = ...`。
  - `os/kernel/tests/init_protection_order_pin.rs:55-73` 明确钉 `load < program_entry_base`，证明该调用是生产顺序的一部分。
- 最小修复：在引文中补一行 `with_protection(|prot| prot.program_entry_base(bsp_logical, kernel_info.kern_stack_top()));`，或把头注改为「第一个省略号为 `program_entry_base` 调用与 Step 2 注释；第二个省略号为 `register_current_arch` 的说明注释」。
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：除非作者把 `program_entry_base` 视为「保护结构之外」而有意省略，但同段正文与 §5.1 都把它算进这条顺序，省略后再称「保留次序」不成立。

## FINDING FACT-05（P2）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1481`（§5.1）
- 严重度：P2
- 问题：文档称 `os/kernel/tests/init_protection_order_pin.rs` 有「三条启动顺序源码钉」，实际文件有 4 个 `#[test]`；被漏掉的是 `bsp_identity_has_one_source`（BSP 逻辑号单一来源）。
- 影响：测试计数少 1，且漏掉一条真实存在的启动期不变量；读者会以为 BSP 选举只由 `init_protection` 的正文保证、没有源码钉。
- 证据或推理：E1。
  - `grep -n '#\[test\]' os/kernel/tests/init_protection_order_pin.rs` → 4 处；函数名为 `bsp_identity_has_one_source`、`protection_store_refresh_load_entry_base_order`、`trap_stubs_installed_before_idt_load`、`ap_stack_programmed_before_sipi`。
- 最小修复：把「三条」改为「四条」，并在列举中补「`init_protection` 的 BSP 逻辑号只来自 `elected_bsp_hw_id`（`bsp_identity_has_one_source`）」。
- 置信度：高
- 是否需源码核实：否（已核）
- 若我错了：若作者把「启动顺序源码钉」限定为三条顺序链、把 BSP 选举测试排除在外，则应改写成「三条顺序钉 + 一条身份来源钉」，否则「三条」与文件内 4 个 `#[test]` 的读数不符。

---

## SUGGESTION

### SUGGESTION FACT-S1（P3）

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:588`（§2.4）
- 问题：称 `tss_init` 返回值「在 C 树的五个调用点全部被丢弃」，但列举的 `arch/i386/include/arch_smp.h:18` 是宏定义内部的一次使用，不是独立调用点；按「调用点」口径实际是四处（earm arch_system.c、i386 protect.c、i386 arch_smp.c、i386 arch_system.c）。
- 影响：计数口径不严，读者复算会对不上「五处」。
- 证据或推理：E2（grep 行已由文档给出，未逐行展开宏体）。
- 最小修复：把「五个调用点」改为「四处调用 + 一处宏内复用」，或直接写「五处命中」。
- 置信度：中
- 是否需源码核实：是
- 若我错了：若 `arch_smp.h:18` 的宏在别处展开成一次真实调用且文档把它算作调用点，则「五处」可成立；但按宏定义行本身计为调用点仍不准确。

---

## 阴性声明（扫了什么、发现数、N=0 不覆盖什么）

- 锚点：75 个全部解析，0 个 ZERO-DEF/MULTI-DEF；未覆盖锚点语义是否指向正确函数（工具只查存在性），已对 `init_protection`、`load_with_tss`、`refresh_descriptors`、`set_handler`、`install_idt_handlers` 逐一手核。
- 路径：22/22 关联代码存在；未核 `minix3/` 下 C 文件是否全部被 §7.2 列出（只抽查了 `main.c`、`protect.c`、`pg_utils.c`、`arch_smp.c`、`mpx.S`）。
- 常量/结构：`GDT_ENTRIES = 6 + 32×2 = 70`、`TSS64_SIZE = 104`、`X86_64_STACK_TOP_RESERVED = 16`、`KERN_CS_ACCESS=0x9A`/`KERN_DS_ACCESS=0x92`/`USER_CS_ACCESS=0xFA`/`USER_DS_ACCESS=0xF2`、`TSS64_ACCESS=0x89`、`STUB_VECTORS=41`、`IDT_ENTRIES=256` 均与源码一致；未逐个核对 TSS 27 字段表的每一行（抽查 `sp0`/`ss0`/`ist`/`iobase` 一致）。
- 测试计数：`x86_64/protection.rs=16`、`x86_64/trap_entry.rs=13`、`x86_64/trap_stub.rs=12`、`arm64/protection.rs=0`、`arm64/trap_entry.rs=1`、`arm64/trap_stub.rs=3`、`riscv64/protection.rs=0`、`riscv64/trap_entry.rs=1`、`riscv64/trap_stub.rs=7`（其中 `:906` 是注释里的 `#[test]`，实际 6）均与 `grep -c '#[test]'` 一致。**未实跑** `cargo test -p minix-arch -- --list`，故 §5.1 的 376/52、266、355、87/21 四个聚合数本轮未复算，不作阴性结论。
- 引文：机械对账 32 块 0 失败；手工逐字核了 §2.5 `prot_load_selectors`、§2.6 `pg_load`/两份 `vir2phys`、§4.3 `GDT_USER_ANCHOR_INDEX` 注释、§4.4 `install_idt_handlers`、§4.6 `set_entry_stack_base`，均与现行源码一致（`set_gate_ist` 除外，见 FACT-02）。未逐字核 §2.1/§2.2 的 `kmain`/`cstart` 大块 C 引文（只核了函数边界与关键行）。
- C/Rust 覆盖：C 侧 `prot_init`、`tss_init`、`prot_load_selectors`、`idt_init`、`idt_copy_vectors`、`int_gate`、`pg_identity`、`pg_load`、两份 `vir2phys`、earm `prot_init` 均有对应说明；Rust 侧 22 个关联文件均有至少一处引用。未发现「文档声称覆盖但代码中不存在」的符号（`set_gate_ist` 是反向：文档引用但代码不存在，已报 FACT-02）。
- 冻结决策：D-099 的 §3.3 第四候选与「搬移-重建是值语义代价」已落地于 `:966-975`；D-100 的 IST 语义句已在 `:887` 与 `:920` 一带出现（`[ARCH: IST 取代 i386 同栈嵌套]` 在 `:887`）；D-103 的 installer 单写者已在代码与 `:1076`/`:1260` 落地（`:1322` 残留已报 FACT-02）；D-115 的独立账户不属文档事实面。以上只作状态核对，未重开论证。

## 汇总

- FINDING：5 条（P1×2、P2×3）
- SUGGESTION：1 条（P3）
- 机械门：7/7 PASS
- 关键事实结论：`init_protection` 的 busy 检查、`set_handler` 单写者、IST 2/8、41 门、GDT 70 槽、TSS 104 字节、测试计数均与源码一致；主要问题集中在 §4.4 的编辑残留（FACT-02）、§3.7 与 §5.2 的自相矛盾（FACT-01）、§4.4 把 Rust 新增门归给 C（FACT-03）、§4.2 省略号标注失实（FACT-04）与 §5.1 测试计数少 1（FACT-05）。
