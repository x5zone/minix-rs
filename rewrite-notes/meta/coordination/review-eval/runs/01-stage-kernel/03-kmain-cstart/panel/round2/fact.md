# D03 R2 · P-FACT 独立事实审查

- 对象：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`（1688 行，工作树 R1 修复后版本）
- 关联代码：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/03-kmain-cstart/related-code.txt` 所列 22 文件
- 方式：只读。未读 round1 panel、defect-ledger、archive、expected（目录只列名，未取内容）
- 结论：机械门全绿；R1 重点修复点绝大多数经复核成立；发现 1 处编辑性重复（P1）、1 处失效交叉引用（P2）、1 处锚点/链接文字不一致（SUGGESTION）

## 一、机械门读数（先跑门，附命令与输出）

| 门 | 命令 | 结果 |
|---|---|---|
| 出版标准 DP-1..DP-6 | `python3 tools/doc-publication-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `无新增违规` |
| 教程 rubric | `python3 tools/tutorial-rubric-check.py <doc>` | `检查 6 章，0 章缺项` |
| 教学构件承载门 | `tools/doc-code-density-check.sh --check <doc>` | `✅ 承载门阻断项通过（4 个可判章均有码/命令）` |
| 文风 lint | `tools/doc-style-lint.sh <doc>` | `命中汇总：| error级=0` |
| 引文机械对账 | `python3 tools/doc-citation-check.py <doc>` | `blocks=48 checked=32 failed=0 skipped=16 warned=0` |
| 内部章节引用 | `AWK=gawk tools/doc-ref-check.sh --check <doc>` | 1 处疑点 `§3.2.4`；经人工核对为**假阳性**（见下） |
| 锚点句意复核 | `python3 tools/anchor-suspect-scan.py <doc>` | 1 处：`619: os/arch/src/arch/protection.rs:trait ProtectionArch → 符号名未出现在本句`（F3） |
| 模式回归 | `bash tools/pattern-gate.sh --diff` | `PASS=19 FAIL=0 SKIP=0` |

`doc-ref-check` 假阳性说明：文档 §4.1（第 1146 行）写的是「见 01 篇 §3.2.4/§5.3」，指向外部文档 `01-boot-shim-bootstrap.md`；该文件确有 `#### 3.2.4 深拷不变量与真机探针`（第 1317 行）与 `### 5.3`（第 2644 行）。工具只按同一行左侧有无 `.md` 抑制外部引用，「01 篇」这种写法未被识别，不构成文档缺陷。

## 二、R1 重点修复点复核（逐项给证据）

| 复核项 | 结果 | 证据 |
|---|---|---|
| §4.4 `set_handler` 写 IST / 删除 `set_gate_ist` | **有缺陷**（F1） | `os/arch/src/x86_64/trap_entry.rs:270-287` `set_handler` 内 `match vector.get() { 2=>1, 8=>2, _=>0 }`；`grep -rn "set_gate_ist" os/` 无命中（仅 target 权限告警），「仓内没有 set_gate_ist」成立。但文档第 1321、1322 行是同一段的两个近重复版本 |
| §3.7 载体调 `install_trap_stubs` | 成立 | `os/qemu-tests/.../test-protection/src/main.rs:85-89`：`CurrentTrapEntry::init()` → `install_trap_stubs(&mut trap)` → `configure_syscall(...)`；`os/kernel/src/lib.rs:2433-2440` 同样 install 先于 `with_trap_entry(|trap| trap.load())` |
| §4.4 C 39 / Rust 41 | 成立 | C：`awk '/gate_table_pic\[\]/,/};/' protect.c \| grep -c hwint` = 16；`...exceptions... \| grep -cE _VECTOR` = 23；合计 39。Rust：`sed -n '/const STUB_VECTORS/,/^];/p' trap_stub.rs \| grep -cE '^\s+\(0x\|^\s+\([0-9]+,'` = 41。向量 9 存在（`archconst.h:45 COPROC_SEG_VECTOR 9`，`protect.c:137` 收表），DPL=3 异常仅 3/4（`grep -c USER_PRIVILEGE` = 6 = 4 软中断 + 2 异常）。41 拆解 18+4+16+3 与 `STUB_VECTORS` 逐项吻合（0–8、10–14、16–19；32–35；0x50–0x57、0x70–0x77；0xF0、0xF1、0xFF） |
| §4.2 省略号与次序叙述 | 成立 | `os/kernel/src/lib.rs:2371-2440`：`CurrentProtection::init` → `store_protection` → `refresh_descriptors` → `load` → `program_entry_base` → `CurrentTrapEntry::init` → `install_trap_stubs` → `configure_syscall` → `trap.load`。文档省略号声明「`program_entry_base` 调用、Step 2 说明注释、`register_current_arch` 说明注释」与原文一致 |
| §5.1 四条源码钉 | 成立 | `os/kernel/tests/init_protection_order_pin.rs` 恰 4 个 `#[test]`：`bsp_identity_has_one_source`、`protection_store_refresh_load_entry_base_order`、`trap_stubs_installed_before_idt_load`、`ap_stack_programmed_before_sipi`；断言的 marker 与文档四条一一对应 |
| §2.4 五处命中 | 成立（行号逐条吻合） | `grep -rn "tss_init(" minix3/minix/kernel/ \| grep -v "int tss_init"` 输出恰 5 行：`earm/arch_system.c:113`、`i386/protect.c:338`、`i386/include/arch_smp.h:18`、`i386/arch_smp.c:272`、`i386/arch_system.c:256` |
| §2.1 `bss_test` / `_TABLE` | 成立 | `main.c` 内 `static int bss_test; assert(bss_test == 0); bss_test = 1;`；`table.c` 第 30 行 `#define _TABLE`；`grep -rn "int kernel_may_alloc" minix3/minix/kernel/` 三行（`glo.h:76`、`earm/pre_init.c:38`、`i386/pre_init.c:33`）。`main.c:133`（`env_get(BOARDVARNAME)`）、`:147`（`cstart()`）、`:157`（`proc_init()`）、`:171`（boot image 循环）逐行对得上 |
| §2.8 指向 §4.7 的 `smp_ap_tail` 指针 | **失效**（F2） | `grep -n "smp_ap_tail" <doc>` 只命中第 830 行；§4.7 是 C/Rust 对照表，无 `smp_init` 调用链。`smp_ap_tail` 实际在 `os/kernel/src/smp.rs:1629/1681/1723` |
| §4.1 别名成因与去处 | 成立 | `os/kernel/src/lib.rs:1807-1808` 的 `kernel_info().expect("kmain: KERNEL_INFO must be stored by arch_boot before jump")` 与引文逐字一致；「01 篇 §3.2.4/§5.3」两节确实存在（见上节） |
| §2.0 / §6.1 `lgdt`/`tss_init` 相关表述 | 成立 | §2.0 学习目标与 §2.3（五类动作）、§2.4（一条描述符 + 一个结构体）一致；§6.1 把 `ltr` 的 #GP 归因到 TSS 描述符/TR 而不是 IDT，与 C 语义一致 |
| §2.5 异常递送轴 | 成立 | `load_with_tss` 的步骤 1 只 `lgdt`（`os/arch/src/x86_64/protection.rs:475-479`），步骤 3 才 `lidt`；文档「取指轴安全、异常递送轴在 lgdt→idt_reload 窗口不设防」与代码次序及 C `prot_load_selectors`（lgdt→idt_init→idt_reload）一致 |
| §3.1 / §4.7 LDT 引文 | 成立 | `protection.rs:531-535` 注释「C loads LDTR via lldt(LDT_SELECTOR), but 64-bit mode does not use LDT. The Rust GDT has no LDT descriptor at all (slots 3/5 are the user-CS anchor/copy)」；§4.7 行表述一致。C 侧 `LDT_INDEX 5`（`archconst.h:18`）与 §1.3/§2.3 的槽号吻合 |
| protection.rs IST ARCH 标记 | 成立 | `os/arch/src/x86_64/protection.rs:248-255` 含 `[ARCH: IST replaces i386 same-stack nesting]`，与 §3.1 的 `[ARCH: IST 取代 i386 同栈嵌套]` 同义 |

附带复核成立的事实（挑关键）：`GDT_SIZE` 复算（`IDT_SIZE 256`、`DESC_SIZE 8`、`TSS_INDEX_FIRST 6`、`CONFIG_MAX_CPUS 1` → 7）；`X86_STACK_TOP_RESERVED = 2 * sizeof(reg_t)`（`archconst.h:149`）；`protect.c:51 pp->p_reg.psw |= 0x3000`；`init_param_dataseg` 底座的 `--size; /* convert to a limit, 0 size means 4G */`（`protect.c:65`）；`mpx.S:202-209` 的 SYSCALL per-CPU 入口宏与文档引文逐行一致；earm `tss_init` 的 `ARM_STACK_TOP_RESERVED` 减法与 `sp0 + sizeof(reg_t)` 写点（`earm/protect.c:42,47`）；`MAX_CPUS = 32`（`os/libs/minix-types/src/types/com.rs:51`），`6 + 32×2 = 70` 成立；§5.1 的 9 文件测试计数 16/13/12/0/1/3/0/1/7 与 `grep -c '#[test]'` 完全一致，riscv64 `trap_stub.rs` 的 7 中确含 1 条注释命中（第 906 行），实际 6 条与正文自述一致；arch crate 全量 `#[test]` 合计 377 含该注释行，正文「376 个/52 文件」按属性口径成立。

## 三、FINDINGS

### F1（P1，编辑性；R1 合并残留）§4.4 出现两个近重复段落

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1321-1322`（§4.4，紧随 `install_idt_handlers` 代码块之后）
- 证据：
  ```
  $ sed -n '1321p;1322p' <doc> | diff - <(...)   # 两行逐字比较
  仅首句不同：
  1321: `set_handler` 写门时把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0。硬件按 ...
  1322: `set_handler` 自己把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0；仓内没有 `set_gate_ist`。硬件按 ...
  后半两句（`TSS.ist[ist-1]` 取值、`setup_tss_for_cpu` 只填两格、`STUB_VECTORS` 布尔值只被
  `test_errcode_classification_matches_sdm` 使用、`TRAPSTUB vec err` 不读布尔值）逐字相同。
  ```
- 影响：同一事实连写两遍，读者会以为第二遍在补新信息；出版标准下的冗余缺陷。R1 显然是把修订句追加在旧句之后而未删旧句。
- 最小修复：删除第 1321 行，保留第 1322 行（它包含「仓内没有 `set_gate_ist`」这一 R1 新增结论）；或把 1321 的第一句并入 1322 后删除 1321。
- 置信度：1.0（逐字 diff 可判）。
- 若我错了：唯一可能是作者有意用两段做「写者身份 → 无第二写者」的递进；但两段后半逐字相同、且没有连接词，排除递进读法。

### F2（P2，事实/引用失效）§2.8 的 `smp_ap_tail` 指向 §4.7，但 §4.7 没有 `smp_init` 调用链

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:830`
- 原文：「C 侧的 AP 启动序列中（Rust 侧对应 `init_ap` 见 §4.3、`smp_ap_tail` 见 §4.7 的 smp_init 调用链）」
- 证据：
  ```
  $ grep -n "smp_ap_tail" <doc>
  830: ... `smp_ap_tail` 见 §4.7 的 smp_init 调用链 ...
  $ sed -n '1394,1412p' <doc>   # §4.7 全文：C 对照表 + 收尾段，无 smp_init 调用链
  $ grep -n "fn smp_ap_tail" os/kernel/src/smp.rs
  1629/1681/1723（三架构实现）
  ```
- 影响：读者按图索骥到 §4.7 找不到所称调用链；属于错误交叉引用，不是行号漂移。
- 最小修复：把括注改为「Rust 侧对应 `init_ap` 见 §4.3（装载次序）与 `os/kernel/src/smp.rs` 的 `smp_ap_tail`」；若想保留节号，§4.5 只讲 GS/MSR 分工，也不含 `smp_init` 链，指向代码路径最稳。
- 置信度：0.95。
- 若我错了：除非「§4.7」指外部 07 篇（`07-cross-space-init.md`）的 smp 链；但本句未写「篇」，且 §4.7 就在本文，读者只会按本文节号读。

### F3（SUGGESTION）§2.5 的链接文字与锚点目标不一致

- 位置：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:619`
- 原文：``Rust 侧的 [`fn load`](os/arch/src/arch/protection.rs:trait ProtectionArch) 把「每个 CPU 的常驻实例只装载一次」写进 trait 契约``
- 证据：
  ```
  $ python3 tools/anchor-suspect-scan.py <doc>
  <doc>:619: os/arch/src/arch/protection.rs:trait ProtectionArch → 符号名未出现在本句，需句意复核
  $ sed -n '79,82p;140,149p' os/arch/src/arch/protection.rs   # 契约确实写在 trait 文档里
  ```
  锚点 `trait ProtectionArch` 解析到 trait 定义，链接文字却是 `fn load`；句子里也没有出现 `ProtectionArch` 字样。
- 影响：点击跳到 trait 定义而不是 `load` 方法；锚点扫描持续报可疑，给后续轮次留下噪声。
- 最小修复：把锚点改为 `os/arch/src/arch/protection.rs:fn load`（trait 内方法的锚点解析规则若支持），或把链接文字改为 `trait ProtectionArch` 并在句内补出该名字。
- 置信度：0.9。
- 若我错了：作者可能有意让链接文字描述「load 契约」而锚点定位到承载契约的 trait 文档；即便如此，链接文字与锚点目标不一致仍会误导读者，最坏也只是建议级。

## 四、未发现问题的重点区域（明确记录，避免下轮重复劳动）

- `§2.4` 的 C 引文（`tss_init` 全文、`arch_finish_switch_to_user` 片段、`mpx.S` SYSCALL 入口）与 `minix3/` 源逐行一致；`protect.c:51`、`:65`、`:137`、`:298`、`:321`、`:338` 等行号均在窗内。
- `§2.3` 的 `prot_init` 全文、`idt_init`/`idt_copy_vectors`/`int_gate` 引文与 C 源一致；GDT 七槽表、39 门读数、DPL 分布（3/4 与 32–35）全部复核通过。
- `§3.1`/`§4.3` 的 Rust GDT 槽位（anchor=3、user DS=4、user CS=5、TSS 从 6 起、每 CPU 两槽）、`Tss64` 104 字节/`ist[7]`/`iobase=0x8000`、`X86_64_STACK_TOP_RESERVED=16`、IST 8 KiB 双槽静态数组与代码一致。
- `§4.5` 的 STAR/LSTAR/SFMASK/EFER 写入次序与 `trap_entry.rs:200-237` 一致；`KERN_CS_SELECTOR` 位于 STAR[47:32]、anchor 位于 [63:48] 的叙述正确。
- `§4.6` 的 aarch64 `TPIDR_EL1`/`set_entry_sp` no-op 论证与 `arm64/protection.rs:90-160` 一致；riscv64 `csrw sscratch` 引文与 `riscv64/protection.rs` 逐字一致；`prepare_user_return` 在 `riscv64/trap_return.rs:118`。
- `§5.2` 的三载体调用面与 `test-protection/src/main.rs:79-89` 一致；riscv64 `trap_stub.rs` 的测试计数注释命中已被正文主动说明。

## 五、结论

- R1 修复点中，仅 §4.4 的 `set_handler` IST 段在落地时留下重复段落（F1）；其余列出的修复点均经代码/源码复核成立。
- 另有两处非阻断缺陷：§2.8 的失效节号引用（F2）与 §2.5 的锚点/文字不一致（F3）。
- 无 P0 事实错误：未发现常量、测试名与计数、C 引文、路径或行为断言与源码不符的证据。
