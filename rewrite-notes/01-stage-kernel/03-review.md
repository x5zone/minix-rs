# 03-kmain-cstart 深度 Review 报告

> **审查对象**：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`（1,360 行，sha256 `7c4ca5a887efb9086ed297a9d4717bf804f3f1dd212e7112fdfae4c080ec2b55`）
> **审查日期**：2026-11-02
> **审查模式**：完整 Review（文档 + 关联 Rust 代码），scope=doc，深度模式；`full-review` 语义
> **真值优先序**：Minix3 C 源码行为 > 设计契约 > Rust 实现 > 文档 > AI 分析
> **本轮立场**：**只审不修**。P0 全部保持 open；文档状态 **NOT CONVERGED**。受审文档与 `os/` 代码零改动。
> **规范制品**：`.review/zcode/01-stage-kernel/03-kmain-cstart/{scan.md,SYMBOLS.md,structure.md}`；独立验证见 `.review/zcode/01-stage-kernel/VERIFY-CHECK.md`；原始轨迹见 `03-review-trajectory.md`。
>
> **文档性质说明**：本文是 review 报告，不是正式章节文档。它必须保留 `scan.md`、`Gate`、`P0/P1/P2` 等审查术语与审查日期；这些不是正文的过程痕迹，而是报告的取证坐标。

---

## 0. 范围、方法与结论先行

### 0.1 范围

- **文档**：全文 7 章（概述、C 源码分析、Rust 设计决策、实现详解、测试要点、诊断、参见），34 个二级/三级标题。
- **关联 Rust**：`tools/doc-code-map.sh --check` 输出 21 个文件，存在性 21/21：
  `os/kernel/src/{lib,globals,memmap,smp,trap_dispatch}.rs`；`os/arch/src/arch/{protection,trap_entry}.rs`；`os/arch/src/{x86_64,arm64,riscv64}/{protection,trap_entry,trap_stub}.rs`；`os/arch/src/lib.rs`；`os/arch/src/x86_64/timer_irq_gate.rs`；三个 `test-protection` 载体。
- **C 真值**：`i386/protect.c`、`main.c`、`pre_init.c`、`pg_utils.c`、`arch_system.c`、`i8259.c`、`clock.c`、`table.c`、`head.S`、`mpx.S`、`klib.S`、`archconst.h`、`arch_proto.h`、`archtypes.h`、`arch_smp.h`、`kernel.h`、`glo.h`、`earm/protect.c`、`vm/pagetable.c`、`vm/main.c`。
- **同目录文档范围**：01/02/04/05/06/07/10/13/14 篇的交叉引用；`.design/03-*` 快照。

### 0.2 方法

1. **R0 预检**：`review-init`、4 条 `ls`、`design-coverage-check`、`doc-code-map --check`、基线 sha/行数/git 状态、四道文档门基线、STATE 校验。
2. **R1 正确性**：Claims-Evidence Tracing；C 引用逐块 `diff`；Rust 引用逐块核对；数据结构覆盖；覆盖率穷举；四张链路表；跨文档逐条开目标文件；Doc Code Sync 七项；测试名对账；代码维度 §1/2/4/8/12/13/14/15；Gate B/C/D。
3. **R2 卓越性**：`structure.md` 12 节 + Gate D-6；文档卓越 §4.1–§4.5；M1–M9 逐节穷举；代码卓越 §16–§21；死代码/非法态/跨架构；文风与承载门。
4. **R2-对抗**：三个材料隔离的只读 agent——A1 代码事实核查（独立、对抗框架）、A2 闭卷学生（只读文档）、A3 冷读结构编辑（只读文档）；A4 独立验证在 R3。
5. **R3 综合**：合并/去重/定级；`verify-check.py` 生成 VERIFY-CHECK；Gate G 抽样 ≥20%，consistency ≥90% 才 PASS。
6. **R4/R5**：本报告 + 轨迹 + 复盘。

### 0.3 结论先行

| 维度 | 结果 |
|---|---|
| Layer 1 正确性 | **FAIL**：6 个 P0 open（5 个 P0-fact + 1 个 P0-code-bug） |
| Layer 2 卓越性 | **PARTIAL/FAIL**：文档教学 15 节中 12 节可自推、3 节只能靠结论；结构骨架可信但有五处接缝；代码有 2 个系统性抽象/死代码问题 |
| Blocker Gates | 0 ✅ / A **PARTIAL** / B ✅ / C ✅ / D ✅ / D-6 ✅（带 P1） / E **PARTIAL** / G ⚠️ **CONCERN**（A4 87.5%） / H ✅（v2 重评） |
| 文档状态 | **NOT CONVERGED** |
| 一句话判决 | C 侧的材料与现算读数总体扎实；问题集中在“文档引用了已不存在的代码载体”“把注释/测试的覆盖说满了”“AP 的 TSS.sp0 在生产路径上没人写”这三类。前两类是事实漂移，第三类是代码缺陷，且文档把它写成了已完成。 |

---

## 1. 执行摘要

### 1.1 问题计数

| 级别 | 数量 | 构成 |
|---|---:|---|
| P0 | 6 | 5 条事实/因果链错误 + 1 条代码缺陷 |
| P1 | 28 | 事实 5、测试 4、代码 5、文档教学/文风 12、流程 2 |
| P2 | 16 | 精度、引用完整性、死代码、冗余、工具假通过、outline 漂移 |

> 计数以 §9 的 Issue List 为准；P0 六分类中本轮只出现 `P0-fact` 与 `P0-code-bug`，没有 `P0-design-*`（设计契约本身与代码的偏离已在 Gate H 重评中处理，不重复计为 P0）。

### 1.2 最关键的八组发现

1. **AP 的 TSS.sp0 在生产路径上没人写**（`P0-code-apTssSp0`）。`X86_64Protection::init()` 把**每个** CPU 槽的 `sp0` 都写成 BSP 的栈顶；`init_ap()` 只 `lgdt/ltr/program_gs`，不写 `sp0`；生产代码里**没有**任何 `set_kernel_stack` 调用（`rg` 只命中 trait 与测试）。测试载体 `test-smp-ap-alive-mb` 在 `init_ap` 之前手工调了 `set_kernel_stack`，正好把生产缺口掩盖掉。后果：AP 的 ring3→ring0 硬件转场会用 BSP 的栈；AP 的 CPU-id 保留字也不会被戳记。文档 §4.3 却写“`init` 给 BSP 写、`set_kernel_stack` 给 AP 写”（`P0-fact-apSetKernelStack`）。
2. **§4.6 的 riscv64 代码引用是伪造/过期**（`P0-fact-riscvBase`）。文档引用 `static KERNEL_TRAP_STACK_BASE`，该符号不存在（anchor 门 `zero-def=1`）；实际 `set_entry_stack_base` 只写 `sscratch`，代码注释明写 “There is deliberately no shared cell”；返回路径由 `TrapReturnArch::prepare_user_return` 每 hart 刷新 `sscratch`。§6.1 的诊断行也建立在同一个不存在的机制上。
3. **§3.6 的 Phase 清单引用块不是原文**（`P0-fact-phaseQuote`）。文档引用里有两处 `D-59`，`os/kernel/src/lib.rs:1355-1361` 的实际注释没有；`timer_irq_gate.rs` 也没有 `D-59`。这是一条“逐字引用”块被改写后仍标成代码引用的事实错误。
4. **§2.1 把 `kinfo.boot_procs` 的读者写错了**（`P0-fact-bootProcs`）。启动循环读写的是 `image[]`（`main.c:171`），`proc_init` 不读 `boot_procs`；VM 读的是循环之后第二次 `memcpy`（`main.c:275`）更新的副本。
5. **§2.2 的 `intr_init` 顺序依赖在因果上不成立**（`P0-fact-intrInitOrder`）。`intr_init(0)` 只编程两片 8259 并屏蔽源，不填 IDT；文档自己说 IF=0，可屏蔽中断不会来。把 `intr_init` 提前并不会“新造”一个按引导方 IDT 递送的窗口——那个窗口从 `kmain` 进入到 `prot_init` 一直存在。§2.0.3 的因果链判定要求把这类“听起来合理但机制不成立”的解释计 P0。
6. **§4.6 的 `SP_EL1` 有两种互斥的失败模式**（`P1-fact-spel1`）。一处说 `msr SP_EL1, Xt` “在架构上属未定义编码，会触发同步异常”，另一处说这条写“改的就是当前栈指针”。二者不能同时成立；`set_entry_sp` 的函数体也没有展示。
7. **文档的教学结构骨架可信，但接缝明显**（A3 冷读）。七章顺序 = 读者七问，不需重排；问题在章内组织轴：按语言切章、按机制论证。§2.5→§2.6、§2.6→§2.7、§2.7→§3.1 三处跳跃；§1 提前消费 §3/§4 的结论；“保护结构”在 §1.4 才定义；22 条反事实在 21 个小节里一节一条，模板指纹明显。
8. **测试覆盖的声称大于证据**（`P1-test-msr`、`P1-test-entryDivergence`、`P1-test-apTss`）。真机 x86 载体只读 GDTR/IDTR/TR/CS/DS，不读任何 SYSCALL MSR，且传给 `configure_syscall` 的是 `info.syscall_entry` 而不是生产的 `syscall_entry_va()`；AP TSS.sp0 没有测试，唯一相关的载体手工补了生产缺失的那一步。

---

## 2. 维度覆盖自检

| 检查维度 | 来源 | 应执行 | 实际 | 跳过理由 |
|---|---|---|---|---|
| §2.0 Claims-Evidence Tracing | doc-checklist | ✅ | ✅ | — |
| §2.1 概念准确性 | doc-checklist | ✅ | ✅ | — |
| §2.2 C 代码引用验证 | doc-checklist | ✅ | ✅ | — |
| §2.3 数据结构覆盖 | doc-checklist | ✅ | ✅ | — |
| §2.4 文档与代码一致性 | doc-checklist | ✅ | ✅ | — |
| §2.4b Rust 习惯用法同步 | doc-checklist | ✅ | ✅ | — |
| §2.4c 路径约定 | doc-checklist | ✅ | ✅ | — |
| §2.4d 符号锚点解析 | doc-checklist | ✅ | ⚠️（85/84，1 zero-def） | — |
| §2.4k 文档 Rust 代码块逐块审查 | doc-checklist | ✅ | ✅ | — |
| §2.5 架构演进说明 | doc-checklist | ✅ | ✅ | — |
| §2.6 文档间交叉引用 | doc-checklist | ✅ | ✅ | — |
| §2.7 图表质量 | doc-checklist | ✅ | ✅ | — |
| §2.8 C 源码覆盖完整性 | doc-checklist | ✅ | ⚠️ Gate A PARTIAL（见 §6） | 语义域 43.3%；工具分母过宽，需人工判别 |
| §2.9 设计决策质量 | doc-checklist | ✅ | ✅ | — |
| §2.10 章节链路验证 | doc-checklist | ✅ | ✅（2 处断链） | — |
| §2.11 文档风格 | doc-checklist | ✅ | ⚠️（过程编号/元话语） | — |
| §3.1–3.4 可读性 | doc-checklist | ✅ | ✅ | — |
| §3.5 教学性 | doc-checklist | ✅ | ⚠️（3/15 节 ②） | — |
| §3.75 过程痕迹 | doc-checklist | ✅ | ⚠️（D-59/D-64①/1.109/1.113） | — |
| §3.94 禁引中间产物 | doc-checklist | ✅ | ✅（0 命中） | — |
| §1 Rewrite 质量 | code-checklist | ✅ | ✅ | — |
| §2 硬件抽象 | code-checklist | ✅ | ⚠️（`init_protection` 6 处 arch cfg） | — |
| §2.5 trait 设计质量 | code-checklist | ✅ | ⚠️（`configure_ipc_entry` 无生产调用） | — |
| §4 执行模型/SMP | code-checklist | ✅ | ❌（AP TSS.sp0） | — |
| §8 命名 | code-checklist | ✅ | ✅ | — |
| §12 no_std | code-checklist | ✅ | ✅ | — |
| §13 设计-代码一致性 | code-checklist | ✅ | ⚠️（design v1 过期） | — |
| §14 C-Rust 语义对齐 | code-checklist | ✅ | ⚠️（AP TSS/注释引用） | — |
| 跨文档联动 | patterns | ✅ | ✅ | — |
| Gate 0/A/B/C/D/D-6/E/G/H | review-process | ✅ | 见 §3 | — |
| 测试五维 | test-audit | ✅ | ✅ | — |
| 卓越性 §16–21 | code-excellence | ✅ | ✅ | — |

### 2.1 规则引用清单（每条结论可回溯）

| 规则文件 | 本轮实际使用的条款 | 落点 |
|---|---|---|
| `review.md` | 真值优先序；P0 六分类；行为语义优先；硬件抽象；概念抽象；Layer 1/2；AI 输出模板；自检 1–5 | §0.3、§2、§9、§10 |
| `review-process.md` | Step 0/0.3/0.5/1/1.5/1.6/2/2.5/3.5/3.5a/3.6/4/4.5/5.6/5.7/7.1；Gate 注册表；L1/L2/L3 | §3、§8、§12、§13 |
| `review-cmds.md` | `full-review`；§一 通用门 #1–#9；§一b 决策树；§二 不做边界 | §0、§2 |
| `review-patterns.md` | §0 P0 必检 5 项；模式 1–15/16–29/35–40/48–78/79–85 | §9 |
| `review-doc-checklist.md` | §1 骨架；§2.0–§2.11；§3.5–§3.96；§4 Doc Code Sync | §2、§4、§5 |
| `review-code-checklist.md` | §1/2/2.5/3/4/6/7/8/9/10/12/13/14/15 | §10、§11 |
| `review-core-semantics.md` | §1.6 四条公理 + 三问；8 字段行为契约 | §7、§8 |
| `review-doc-excellence.md` | §4.1–§4.5 | §10.1 |
| `review-code-excellence.md` | §16–§21；§16.6 非法态；§16.7 跨架构 | §10.2 |
| `style-bible` | 技术博客文风；禁过程痕迹；教学构件；反膨胀≠砍教学 | §4.5、§10.1 |
| `AI-chats/doc-gen-prompt.md` | 两条总则；第 1.5 步；第 2 步 12 条底线；第 3.5 步反证；第 4.6 步学生；第 7 步五门 | §5、§10、§13 |
| `AI-chats/doc-gen-iterate-prompt-v2.md` | 发现型 C / 闭卷型 F；互盲；M 标尺；丢失清单；不得自证 | §10.1、§13 |
| `prompt/DOC-ITER-WORKFLOW.md` | S1 事实验证；S2 结构审阅；S4 V2/V3/V5；§八 归因；§九 G-A/G-B/G-C | §13 |
| `prompt/WORKFLOW-ADVICE-ARCHIVE.md §2.3` | 零附加输入、异构模型、对抗框架、外部裁决、冷启动（未吸收项，参考） | §13.4 |

---

## 3. Blocker Gates

> 每门附实际命令与关键输出；命令均在仓库根执行。L1 = 工具输出，L2 = 手工 grep，L3 = 推断（视为 FAIL，除非注明 MANUAL_FALLBACK）。

### 3.1 Gate 0：制品完整性

| 检查项 | 结果 |
|---|---|
| 9 个扫描锚段 | `03-kmain-cstart/scan.md` 已按模板生成 |
| 关联代码清单 | `doc-code-map --check` 21/21；见 §0.1 |
| 基线 sha/行数 | `7c4ca5a...` / 1360；见轨迹 R0 |

判定：**PASS**（制品在 R4 落盘；本报告与 scan 同步）。

### 3.2 Gate A：覆盖率穷举

```text
$ python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
    --rust-dir os --c-dir minix3/minix/kernel --doc-file 03-kmain-cstart.md \
    --semantic-map tools/coverage-extract/kernel-semantic-map.json \
    --output .review/zcode/01-stage-kernel/03-kmain-cstart/SYMBOLS.md
Coverage Summary for kernel / 03-kmain-cstart.md:
  Total C symbols: 1390
  Doc covered: 94 (6.8%)
  Semantic range C symbols: 217
  Doc covered (semantic): 94/217 (43.3%)
  Rust covered (semantic): 23/217 (10.6%)
  有文档无Rust (语义真信号): 71

$ python3 tools/coverage-extract/coverage-extract.py --check-filled .../SYMBOLS.md
✅ 覆盖率语义列已全部填充（无 `_待AI补充_`）
```

判定：**PARTIAL**。`--check-filled` 通过；但语义域 43.3% < 80%，71 个“有文档无 Rust”需要人工判别。工具语义映射表是 kernel 全模块的 217 个符号，而本篇 §1.4 明确把范围收窄到 `prot_init`/`cstart`/保护结构；43.3% 是“分母过宽”的信号，不是“本篇漏写 123 个符号”的直接判决。人工抽查见 §6。

### 3.3 Gate B：Top5 行为契约表

| 函数 | 输入 | 输出 | 副作用 | 错误码 | 时序 | 生命周期 | 权限 | 并发 |
|---|---|---|---|---|---|---|---|---|
| `kmain` | `kinfo_t *local_cbi` | 无返回（进入 `cstart`） | BSS 自检、拷贝全局、板子识别、`kernel_may_alloc=1`、boot image 拷贝 | `assert`（BSS/尺寸） | 必须在 `pre_init` 之后、`cstart` 之前 | 引导期一次 | ring0 | 单线程引导 |
| `cstart` | 无 | 无 | `prot_init`→`init_clock`→`intr_init(0)`→`arch_init` | 无 | 四次调用顺序固定 | 引导期一次 | ring0 | 单线程引导 |
| `prot_init` | 无 | 无 | 探测能力、清表、建 GDT/IDT/TSS、装载、重建页表、置 `prot_init_done` | 无（`assert`） | 必须在 `prot_load_selectors` 前建表，之后置标志 | 引导期一次 | ring0 | 单线程引导 |
| `tss_init` | `cpu`、`kernel_stack` | `SEG_SELECTOR(index)`（五个调用点全部丢弃） | 写 GDT TSS 描述符、TSS 结构体、`k_percpu_stacks[cpu]`、SYSENTER/SYSCALL MSR | 无 | `prot_init` 首次、`arch_init`/SMP 二次 | 每 CPU 一次 | ring0 | 单线程引导 / BKL |
| `prot_load_selectors` | 无 | 无 | `lgdt`→`idt_init`→`lidt`→`lldt`→`ltr`→重载 CS/DS/ES/FS/GS/SS | 无（#GP 由硬件） | 表建好之后；`ltr` 必须在 `lgdt` 之后 | 每 CPU 一次（BSP/AP） | ring0 | 单线程引导 |

判定：**PASS**（8 字段齐全；与 C 语义对齐；AP TSS 的 `sp0` 缺口见 Gate E/Issue）。

### 3.4 Gate C：细节精确性

| 元规则 | 位置 | 可疑内容 | 判定 |
|---|---|---|---|
| 外部知识 | §3.1 `ss0` | “CPU 在转场时按 STAR/门推出内核 SS” | P1（ISA 待核） |
| 外部知识 | §4.6 `SP_EL1` | “未定义编码”与“改当前栈指针”互斥 | P1 |
| 通用接口 | `ProtectionArch::init_ap` | 参数 `kernel_stack_top` 只用于 GS，不写 TSS.sp0 | P0-code |
| 返回值 | `pg_mapkernel()` | `prot_init` 丢弃返回值；`pre_init` 存值 | ✅ 一致 |
| 资源闭环 | `store_kernel_info` | 五块载荷，不是三块 | P1 |
| 理由质疑 | §2.2 `intr_init` | 依赖 IDT 的因果链不成立 | P0-fact |

判定：**PARTIAL**（可疑点全部登记；Gate C 本身不阻断，但两项升级为 P0）。

### 3.5 Gate D：P0 必检清单 5 项

| # | 检查项 | 命令/证据 | 判定 |
|---|---|---|---|
| 1 | §5 测试是否真实存在 | `grep -c '#\[test\]'`：16/14/11/2/1/3/2/1/7；`test_stub_coverage_matches_idt_gates`、`test-protection` 三载体均命中 | ✅ |
| 2 | 声明的 trait 是否有 ≥2 行为不同的 impl | `rg "impl.*ProtectionArch"`：x86_64/arm64/riscv64 + MockProtection；`TrapEntryArch` 同 | ✅ |
| 3 | 声明的函数是否在声明文件 | `rg "fn (prot_init|tss_init|init_protection|store_kernel_info)"` 全部命中 | ✅ |
| 4 | 核心算法是否 stub | `configure_ipc_entry` 无生产调用（死代码）；`kernel_may_alloc()` 无调用；非 `todo!/unimplemented!` | ⚠️（P1/P2） |
| 5 | §4 签名是否与实际一致 | `Tss64`/`IdtEntry64`/`GsArea`/`STUB_VECTORS`/`init_protection` 逐项核对 | ✅（`init_ap` 语义不一致另计 P0） |

判定：**PASS**（第 4 项有死代码，但不构成“文档描述的算法是 stub”；已登记 P1/P2）。

### 3.6 Gate D-6：structure.md 12 节

- `structure.md` 12 节全部填写；骨架判定 **PASS**，五处接缝与 4 行元注释进 Issue List。
- 关键判定：Ch1 首句主语是书目对象（P1）；跨架构 3×3 框架前后不一致（P1）；“保护结构”在 §1.4 才定义（P1）；§2.8/§3.5/§3.6 只能靠结论（P1）；22 条反事实模板化（P1-teachingFormalized）。

### 3.7 Gate E：§5 测试章节验证

| 检查项 | 结果 |
|---|---|
| 文档 §5 列出的每个测试 fn | 全部 grep 命中（`tss64_size_is_104_bytes`、`tss64_offsets_correct`、`gdt_descriptors_have_correct_dpl`、`gdt_descriptors_are_flat_mode`、`tss_descriptor_is_64bit`、`tss_descriptor_high_half_carries_base_above_4gib`、`init_creates_tss_descriptor_in_gdt`、`set_kernel_stack_updates_sp0`、`init_sets_tss_sp0_below_reserved_area`、`test_stub_coverage_matches_idt_gates`、`configure_ipc_entry_sets_idt_gate_33`） |
| 测试数量 | 16/14/11/2/1/3/2/1/7，与 §5.1 一致 |
| 真机载体 | `test-protection` 三架构源码逐行核对；x86 只读 GDTR/IDTR/TR/CS/DS，不读 MSR；aarch64 独立探针；riscv64 复用 trait |
| 运行 | 本轮未跑 QEMU（用户决定）；宿主 `cargo test -p minix-kernel --features mock` 856 passed / 0 failed / 8 ignored；`cargo test -p minix-arch --features runtime-window` 通过；默认 `cargo test -p minix-arch` 38 编译错误（feature 形态问题，非本篇缺陷） |
| 缺口 | M4 MSR 真机断言缺失；AP TSS.sp0 无测试；负向顺序无载体（R03-T01）；`configure_ipc_entry` 测试测的是死代码 |

判定：**PARTIAL**（测试存在且数量对；覆盖声称大于证据）。

### 3.8 Gate H：design/outline 门控

| 项 | 结果 |
|---|---|
| H.1 design 存在 | ✅ `03-design.v1.md`（206 行，2026-07-16） |
| H.2 design ↔ code 一致性 | **v1 重评 FAIL（约 60%）**：D3 typestate 未实现（现码全局 `Option`）；D5“无散落 `#[cfg]`”被 `init_protection` 6 处 arch cfg 违反；trait 签名 `init_ap -> Self` vs `&self`、`set_kernel_stack(&self)` vs `&mut self`；D6 引用不存在的 §1.8；`lib.rs:985` 已漂移。**本轮重评产出 `03-design.v2.md`，H.2 对 v2 PASS**（v2 记录现码决策与偏离）。 |
| H.3 design ↔ Minix3 | ✅（三问/D1 对齐；C 的 LDT/TSS/IDT 语义在 v2 中按现状重述） |
| H.4 design-wrong | ✅ 无（v1 的 D3/D5 属“过期”而非“抓错本质”；v2 已修正） |
| H.5 Refactor 区分 | ✅（code Refactor 与 design Refactor 分列） |
| H.6 outline + doc↔outline | ✅ outline v3/outline-review v3 存在；doc↔outline 有 P2 漂移（§2.6 “三个调用” vs 文档“四个调用”；§5.4 不在 outline） |

判定：**PASS（以 v2 为依据）**；v1 过期登记 `P1-process-designStale`。

### 3.9 Gate G：VERIFY-CHECK

- `verify-check.py` 生成 `.review/zcode/01-stage-kernel/VERIFY-CHECK.md`，抽样 10/50（20%），seed 20261102；机械检查 9 锚段 + 8 个 gate-evidence 块全过。
- **A4 独立验证**：8 个抽样 issue 中 6 CONSISTENT + 2 PARTIAL，一致性 **87.5%** → **Gate G = CONCERN（<90%）**。A4 确认 AP TSS 缺口真实、riscv 静态不存在、boot_procs 读者写错；另指出 `P1-test-entryDivergence` 可降 P2、`P1-docStyle-axisSelfClaim` 过重，以及 P0 riscv 修复需覆盖 §5.4 M8（回归轮已补）。
- 主会话的 95% 同 agent 暂测不作为最终判定；修复后的回归验证见 R2-6/R2-7（A5 待回）。

---

## 4. Doc Code Sync 七项

| # | 项 | 结果 | 证据 |
|---|---|---|---|
| 1 | 函数名 | ✅ | `prot_init`/`tss_init`/`idt_init`/`prot_load_selectors`/`init_protection`/`arch_boot_impl`/`store_kernel_info` 全部命中 |
| 2 | 签名 | ⚠️ | `init_protection`/`store_kernel_info`/`setup_tss_for_cpu` 一致；`ProtectionArch::init_ap` 的文档语义（“creates per-CPU TSS entry”）与实现（pure load，不写 sp0）不一致 → P0-code |
| 3 | 字段 | ✅ | `Tss64` 104 字节/偏移、`IdtEntry64` 16 字节、`GsArea` 3 字段、`X86_64Protection` 4 字段与文档一致 |
| 4 | 常量 | ✅ | `GDT_ENTRIES=22`、`TSS64_SIZE=104`、`STUB_VECTORS=41`、`IDT_ENTRIES=256`、`X86_64_STACK_TOP_RESERVED=16` 全部复算一致 |
| 5 | 路径 | ⚠️ | 文档内路径全部存在；**代码注释** 6 处引用 `rewrite-notes/03-stage-kernel/...`（应为 `01-stage-kernel`）→ P1-code |
| 6 | 测试名 | ✅ | §5 列出的测试全部命中；数量一致 |
| 7 | C 源引用 | ✅ | 本轮逐块 `diff` 的 C 引用全部一致；`gate_table_pic` 16/`exceptions` 23/`USER_PRIVILEGE` 6、`tss_init` 五调用点、`GDT_SIZE=7` 全部复算一致 |

---

## 5. structure.md 摘要（Gate D-6）

- **主题思想**：三张表回答 CPU 三问；`kmain`/`cstart` 接管三份答案；64 位下三表去留与 trait 化。✅
- **目标读者**：已声明；但 §1.1 首句引 02 篇 §4.4，未读 02 篇的读者被挡在门外。P1。
- **叙事主语**：段级 CPU/所有权 ✅；§1.1 首句是书目对象 ❌ P1。
- **驱动方向**：问题→C→设计→实现→测试→诊断 ✅；§1.1 “按调用顺序展开”与 §2.3/§2.4 的按表分组不符 P1。
- **大纲**：七章核心命题成立 ✅。
- **核心概念**：11 个概念，依赖有序，无 L2 术语前置 ✅。
- **跨架构统一抽象**：§1.2 先给三问框架 ✅；但列头“x86-64”混入 i386 C、§2.7 ARM 列是 earm-32、§4.6 未复述 3×3 → P1。
- **双向闭环**：声明上只到“门里有真入口”，完整往返交 14/10 篇 ✅；但 §3.1/§4.3 讲 `sysret64` 返回路径，边界未一句话合法化 → P1。
- **起承转合**：弧线成立；三处接缝（§2.5→§2.6、§2.6→§2.7、§2.7→§3.1）→ P1。
- **元注释**：8 处，其中 4 处应删/移（§1.1 目录散文、§5.4 自我辩护、§1 教学目标结论前置、§1.3 `#ifdef` 过早）→ P1。
- **裸概念复述**：能复述主线；卡点 5 处（`SP_EL1` 互斥、riscv 全局单元、Rust 标志读者、第二个保留字读者、门核对者）→ P1/P2。
- **纵向链路**：Ch3→Ch1&2 ✅；Ch4→Ch3 ✅；测试→Ch3+4 2 处 ⚠️；代码→Ch4 1 处 P0（AP TSS）。

---

## 6. 覆盖率与人工判别（Gate A 补充）

工具语义域 217 个符号、文档覆盖 94（43.3%）。人工判别规则：

1. **本篇射程内**（`prot_init`/`tss_init`/`idt_*`/`prot_load_selectors`/`kmain`/`cstart`/`init_clock`/`pg_*`/`vir2phys`/`tss_s`/`gate_table_s`/相关常量与位域）：全部有对应节，✅。
2. **他篇职责**（`memory_init`/`libexec_pg_alloc`/`arch_boot_proc`/`smp_init`/`switch_to_user`/`system_init`/`arch_post_init`）：§1.4 已分流，✅（不要求本篇展开）。
3. **仅提及**（`arch_ser_init`/`isb`/`write_vbar`/`tss_init_all`/`bsp_finish_booting`/`boot_cpu_init_timer`/`prepare_shutdown`/`minix_shutdown`）：文档给了锚点与去向，✅。
4. **名称假阳性**（`if`/`irq`/`proc`/`cpu`/`panic`/`EXTERN`/`RPL`/`PRESENT`/`DPL`/`DPL_SHIFT`）：不计覆盖缺口。
5. **真实缺口**：`arch_finish_switch_to_user` 有引用、有节选，✅；`vm_enable_paging` 只在 §1.3 提一句，属 01/02 篇，✅；`PG_ALLOCATEME` 只提及，属 01/02 篇，✅。

结论：工具 43.3% 主要是分母把 kernel 全模块符号算进来；本篇射程内的覆盖没有发现“整块函数未讲”的 P0。**Gate A 仍标 PARTIAL**，因为机器门未达 80%，且 `SYMBOLS.md` 的 AI 列只做了分档判断，不是逐符号深审。

---

## 7. Top5 行为契约表（Gate B）

见 §3.3。8 字段完整；`tss_init` 的 AP `sp0` 缺口是本轮最重要的行为契约违反（见 `P0-code-apTssSp0`）。

---

## 8. 纵向链路与因果链抽样

### 8.1 纵向链路

| Ch1 概念 | Ch3 决策 | Ch4 实现 | Ch5 测试 | 链路完整? |
|---|---|---|---|---|
| 三问/三表 | §3.1/§3.2 | §4.2–§4.4 | `test-protection` 三架构 | ✅ |
| TSS.sp0 保留字 | §3.1/§3.2 | §4.3 `setup_tss_for_cpu` | 宿主 2 条 + 真机首次用户进入 | ⚠️ AP 写入无生产调用（P0） |
| 异常入口 | §3.2/§3.7 | §4.4 | trap 14 条 + 覆盖检查 | ✅ |
| 系统调用 MSR | §3.4 | §4.5 | 真机只读表寄存器 | ⚠️ M4 声称过高（P1） |
| GS 区 | §3.4 | §4.5 | 无宿主；真机间接 | ⚠️（P2） |
| 跨架构入口 | §3.1/§3.3 | §4.6 | aarch64/riscv64 载体 | ✅ |
| 页表重建淘汰 | §3.5 | §4.2 | 01/02 篇 | ✅ |

### 8.2 因果链抽样（Step 3.5b，5 条）

| 位置 | 设计解释 | 因果链 | 每步成立? | 判定 |
|---|---|---|---|---|
| §2.2 | “`intr_init` 需要 IDT 已装填” | PIC 编程→需要门→否则异常按引导方 IDT | ❌ `intr_init` 不填门、全屏蔽、IF=0；窗口与 `intr_init` 无关 | **P0-fact-intrInitOrder** |
| §2.3 | “LDT 占位必须存在，因为 `lldt` 要求 LDT 类型描述符” | `lldt` 校验类型→槽必须存在 | ✅ | ✅ |
| §2.4 | “`sp0` 减两个保留字，否则 CPU 号写到栈外” | 栈顶两字归内核→不减则越界 | ✅ | ✅ |
| §2.6 | “`pg_identity` 的 PWT/PCD 标记超出内存顶端” | 4MB 对齐比较→包含顶端所在块 | ⚠️ 包含真实内存，不是严格“超出” | P2-fact |
| §4.6 | “`sscratch` 返回重锚到 `KERNEL_TRAP_STACK_BASE`” | 全局单元→返回写入 | ❌ 静态不存在；实际 per-hart `prepare_user_return` | **P0-fact-riscvBase** |

---

## 9. Issue List

> 每条 P0/P1 附“位置 + 问题 + 证据 + 修复方向”。P0 全部 open；P1/P2 只登记，不在本轮修。
> 证据命令均在仓库根可重放；行号为文档当前行号（评审快照）。

### 9.1 P0（6）

| ID | 类型 | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|---|
| `P0-code-apTssSp0` | code-bug | `os/arch/src/x86_64/protection.rs:367-375,392-431`；`os/kernel/src/smp.rs:1136,1148,1590` | 生产路径没有 `set_kernel_stack`：`init()` 把所有 CPU 槽的 `sp0` 写成 BSP 栈顶；`init_ap()` 只 `lgdt/ltr/program_gs`，不写 `sp0`；AP 的 ring3→ring0 会用 BSP 栈，CPU-id 保留字也不戳记。 | `rg -n "set_kernel_stack" os/arch/src os/kernel/src --glob '!**/target/**'` 只命中 trait 与测试；`test-smp-ap-alive-mb:288-301` 手工调 `set_kernel_stack(1, ...)` 正好补偿生产缺口 | 在 `smp_init` 发 SIPI 前对每个 AP 调 `with_protection_mut(|p| p.set_kernel_stack(logical, stack_top))`，或把 `init_ap` 改为 `&mut self` 并在其中写 `sp0`；补 AP 侧断言/测试 |
| `P0-fact-apSetKernelStack` | fact | §3.2 L773；§4.3 L1012 | 文档称 `set_kernel_stack` 给 AP 写、`init` 给 BSP 写；生产代码没有 AP 侧调用。 | 同上；`os/arch/src/x86_64/protection.rs:397` 的 `init_ap` 注释也声称“BSP wrote sp0 via set_kernel_stack”，与 `rg` 结果矛盾 | 改文档为“当前实现未在 AP 侧调用；修复见代码项”，或在代码修好后保持文档；两条同步 |
| `P0-fact-riscvBase` | fact | §4.6 L1130-1146；§6.1 L1279 | 引用的 `static KERNEL_TRAP_STACK_BASE` 不存在；实际 `set_entry_stack_base` 只写 `sscratch`，代码注释明写“deliberately no shared cell”；返回路径由 `TrapReturnArch::prepare_user_return` 每 hart 刷新。 | `bash tools/anchor-resolve.sh --check ...` → `zero-def=1`（`os/arch/src/riscv64/protection.rs:static KERNEL_TRAP_STACK_BASE`）；`sed -n '70,85p' os/arch/src/riscv64/protection.rs` 无该静态；`os/arch/src/riscv64/trap_return.rs:118-126` 写 per-hart `sscratch` | 重写 §4.6 的 riscv64 段：删 `KERNEL_TRAP_STACK_BASE`，改为“per-hart `sscratch` + `prepare_user_return` 每次用户返回刷新”；§6.1 同步 |
| `P0-fact-phaseQuote` | fact | §3.6 L875-886 | “逐字引用”块有两处 `D-59`；实际 `lib.rs:1355-1361` 没有；`timer_irq_gate.rs` 也没有 `D-59`。 | `sed -n '1355,1361p' os/kernel/src/lib.rs`；`grep -n "D-59" os/arch/src/x86_64/timer_irq_gate.rs` 无命中 | 删掉引用块与正文里的 `D-59`，引用实际注释；若 `D-59` 是过程编号，按 style-bible 不进正文 |
| `P0-fact-bootProcs` | fact | §2.1 L166 | 称启动循环与 `proc_init` 读 `kinfo.boot_procs`；实际循环读写 `image[]`，`proc_init` 不读该表；VM 读的是循环后第二次 `memcpy` 的副本。 | `sed -n '171p;275p' minix3/minix/kernel/main.c`；`sed -n '119,159p' minix3/minix/kernel/proc.c`；`grep -rn "boot_procs" minix3/minix/` | 改为“循环读写 `image[]`；循环后 `main.c:275` 再 `memcpy` 给 VM；`proc_init` 不读该表” |
| `P0-fact-intrInitOrder` | fact | §2.2 L253-264 | 称 `intr_init(0)` 需要 IDT 已装填，提前会新造“按引导方 IDT 递送”的窗口；实际 `intr_init` 只编程 8259 并屏蔽源，不填门；IF=0 下可屏蔽中断不会来；该窗口从 `kmain` 进入到 `prot_init` 一直存在。 | `sed -n '28,52p' minix3/minix/kernel/arch/i386/i8259.c`；`sed -n '68,73p' minix3/minix/kernel/arch/i386/head.S`；文档自己 L264 写“IF=0” | 把依赖改写成“`intr_init` 不依赖 IDT；真正的不变量是任何 `prot_init` 之前的异常/NMI 都无门可走”；顺序仍保留，但归因改成“先建门、再编程控制器”的防御性次序 |

### 9.2 P1（28）

| ID | 类型 | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|---|
| `P1-fact-storeKernelInfo` | fact | §4.1 L941 | 称 `store_kernel_info` 做“三块落地”；实际五块：reserved_regions、memmap、boot_modules+name pool、platform_sources、param_buf。 | `sed -n '1119,1230p' os/kernel/src/lib.rs` | 改为“五块”或“三块 + 平台源/参数缓冲” |
| `P1-fact-ss0` | fact | §3.1 L733 | “CPU 在转场时按 STAR/门推出内核 SS”混淆两条路径：STAR 是 SYSCALL/SYSRET，门是中断/异常；64 位 TSS 无 `ss0`，门转场 SS 语义需按 SDM 说清。 | `os/arch/src/x86_64/protection.rs:118-146`（Tss64 无 ss0）；外部 ISA 知识（Intel SDM Vol 3A §6.14.2 / AMD APM Vol 2 §4.1.5）；标 `[待 SDM 复核]` | 拆成两句：SYSCALL 用 STAR；门转场 SS 由 64 位模式规则决定（不要用“推出”一词） |
| `P1-fact-spel1` | fact | §4.6 L1126 vs L1150 | 同一节给 `msr SP_EL1` 两种互斥失败模式：未定义编码/同步异常 vs 改当前栈指针。 | 两段原文；`os/arch/src/arm64/protection.rs:140-165` 注释只支持前者 | 选一个（按 ARM ARM 复核），删另一个；展示 `set_entry_sp` 函数体 |
| `P1-test-msr` | test | §5.4 M4 L1239；§5.2 L1200-1204 | 称真机载体“调 `configure_syscall` 并读寄存器”；实际只读 GDTR/IDTR/TR/CS/DS，不读 STAR/LSTAR/SFMASK/EFER。 | `sed -n '76,82p;86,153p' os/qemu-tests/.../test-protection/src/main.rs`；无 `rdmsr` | 改 M4 为“MSR 值无真机断言”；在 §5.3 增缺口；或在载体加 `rdmsr` 判据 |
| `P1-test-entryDivergence` | test | §5.2 L1200-1204 | 称载体按 kmain Phase B 的次序复用生产路径；实际传的是 `info.syscall_entry`，生产用 `syscall_entry_va()`。 | `test-protection/src/main.rs:81` vs `os/kernel/src/lib.rs:2049` | 文档注明载体用 KernelInfo 元数据；或测试改用 `syscall_entry_va()` |
| `P1-code-deadIpc` | code | `os/arch/src/arch/trap_entry.rs:159-167`；`os/arch/src/x86_64/trap_entry.rs:301-320,518-563` | `configure_ipc_entry` 无生产调用；trait 注释称“OS code calls configure_ipc_entry()”，但 `init_protection` 只调 `install_trap_stubs`；测试测的是死代码。 | `rg -n "configure_ipc_entry" os --glob '!**/target/**'`：只有 trait/impl/注释/测试 | 或接线（在 install 之后按需设置 vector 33），或删方法/测试/注释；与 §3.4/§4.5 同步 |
| `P1-test-apTss` | test | §5.3/§5.4 | AP TSS.sp0 无测试；`test-smp-ap-alive-mb` 手工 `set_kernel_stack` 补偿，掩盖生产缺口。 | 同上 P0-code 证据 | 修代码后加 AP 侧断言：`tss[cpu].sp0 == ap_stack_top - 16`，且 `sp0+8 == cpu` |
| `P1-test-gateCoverageClaim` | test | §5.2 L1214；§5.4 M3 | §5.2 说“门有效性不在真机载体的覆盖里”；`test-user-trap` 走生产 `init_protection`（装真 stub）并以 CPL3 `int 0x21` 进门，是真机上的门有效性正向覆盖。 | `test-user-trap/src/main.rs:252`；`run_all.sh:96`；`init_protection` 内 `install_trap_stubs` | 把该句限定为“test-protection 三载体不覆盖门有效性”；M3 补 `test-user-trap` 证据 |
| `P1-fact-vir2physOvergeneral` | fact | §2.6 L649 | “`prot_init` 在高半地址上执行，任何内核符号的值都要减平移”过度概括；`pg_load` 在改名后的 `pg_utils.o` 里，调用解析到恒等版 `__k_unpaged_vir2phys`，`pagedir` 在 `.unpaged_bss`（低 VMA），不需要减平移。 | `kernel.lds:19,22`；`pg_utils.c:249`；`Makefile.inc:51-52`；文档自己的括注 | 改成“看符号落在哪一段：高半段减平移、unpaged 段不减”；明确 `pg_load` 用恒等版 |
| `P1-fact-verbatimQuotes` | fact | §4.4 L1074；§3.4 L849 | “整函数/代码引用”块加入源码没有的过程编号：`install_idt_handlers` 的注释多了 `by S-4`，`GsArea` 的注释多了 `S-4 L2:`。 | `trap_stub.rs:622-624`（无 `by S-4`）；`trap_stub.rs:646`（无 `S-4 L2:`） | 删掉插入词，或把块标注降为“改写/节选（注释有增补）” |
| `P1-code-docPaths` | code | `os/arch/src/arch/protection.rs:42-51`；`arch/trap_entry.rs:52-53`；`x86_64/protection.rs:8-9`；`x86_64/trap_entry.rs:21-22`；`riscv64/trap_entry.rs:45`；`globals.rs:45` | 代码注释引用 `rewrite-notes/03-stage-kernel/...`（应为 `01-stage-kernel`）与不存在的 §1.7/§3.1/§3.2/§3.3。 | `grep -rn "rewrite-notes/03-stage-kernel"`；文档标题只有 §3.1–§3.7，无 §1.7 | 修路径与节号；按 §14.4 逐条 grep 验证 |
| `P1-code-initComment` | code | `os/arch/src/x86_64/protection.rs:363-366` vs `:281-294` | `init()` 注释称描述符“永不需重建”，与 `refresh_tss_descriptors` 及 E1 注释冲突。 | 两段原文；`lib.rs:2003-2011` 的 E1 注释 | 删“never need rebuilding”，改为“init 时地址未定，搬迁后必须 refresh” |
| `P1-code-cfgLeak` | code | `os/kernel/src/lib.rs:2010,2020,2056,2065,2070,2075,2082` | `init_protection`（OS 语义层）用 6 处 `#[cfg(target_arch)]` 直接依赖 `minix_arch::x86_64::trap_stub::program_gs`/`X86_64Protection` 具体类型；违反“OS 层不依赖具体架构实现模块/类型”的边界。 | 原文 + `review-code-checklist.md` §2 边界澄清；design v1 D5 | 把 GS 区、TSS refresh、dispatcher 注册收进 `ProtectionArch`/`TrapEntryArch` 的方法（默认 no-op），kernel 侧不再 cfg |
| `P1-code-initProtectionDoc` | code | `os/kernel/src/lib.rs:1974-1980` | 文档注释仍说“表随后丢弃、由后续阶段重建”，函数体已装真入口并 `load()`；文档 §5.3 已披露。 | 原文 + `:2032-2049` | 更新注释；这是代码侧收口，不是文档能修的范围 |
| `P1-docStyle-riscvLoad` | docStyle | §4.6 L1146 | “`load` 在 riscv64 上没有需要执行的步骤”未区分 `ProtectionArch::load` 与 `TrapEntryArch::load`；后者写 `stvec`（真机判据要求非零）。 | `os/arch/src/riscv64/trap_entry.rs` 的 `load` 写 `stvec`；§5.2 判据 | 写明“`ProtectionArch::load` 无事；`TrapEntryArch::load` 写 `stvec`” |
| `P1-docStyle-selfCopy` | docStyle | §2.1 L138 | 自拷贝证明依赖 `pre_init` 的 `return &kinfo`、`head.S` 的 `push %eax`、`glo.h` 的 `#define kmess kmessages`、`cbi->kmess = &kmess`，但都没展示。 | `sed -n '217,237p' pre_init.c`；`head.S:79-83`；`glo.h:35` | 补 3 行真实节选，或把“依据”改成可复制的 grep 命令 |
| `P1-docStyle-threeSections` | docStyle | §2.8/§3.5/§3.6 | 闭卷学生三态审计：15 节中 12 节 ①、3 节 ②（只能靠结论）、0 节 ③。§2.8 的不变量未展示 `prot_init_done` 读者；§3.5 的“函数体无 CR3”未展示函数体；§3.6 的全屏蔽前提未展示函数。 | `tmp/review03/A2-student.md` §2；`arch_smp.c:87` 的 `assert(prot_init_done)`；`lib.rs:1988-2086`；`init_clock_and_interrupts` | 补“缺席断言”的可核对实物（完整函数/命令/节选），或明确写“不在本节核对” |
| `P1-docStyle-crossRef22` | docStyle | §2.6 L649 | “`pre_init.o` 与 `pg_utils.o` 都在改名名单里（§2.2）”指 02 篇 §2.2，但同号命中本文 §2.2（cstart）；`doc-ref-check` 假通过。 | `AWK=gawk tools/doc-ref-check.sh --check` 输出全绿；02 篇 §2.2 才是改名名单（`Makefile.inc`） | 写成 `02 篇 §2.2`（同一行带 `.md` 或“02 篇”限定） |
| `P1-docStyle-processNumbers` | docStyle | L849/878/883/886/943/1074/1126 | 正文出现 `D-59`、`D-64①`、`S-4`、`1.109`、`1.113` 等仓库内部过程编号；`1.109`/`1.113`/`D-64①` 还被写成“源码注释里的编号”，但 `grep` 在对应源码里无命中；`doc-style-lint` 的 SL-11 不覆盖这些形态。 | `grep -nE '\bD-[0-9]{1,3}\b|\bS-[0-9]+[a-z]?\b|1\.10[0-9]|1\.11[0-9]'`；`grep -n "1.109\|1.113" os/arch/src/arm64/protection.rs` 无命中；`grep -rn "D-64" os/ --include=*.rs` 无命中 | 正文改为无编号陈述；引用源码注释里的编号要标注“按原文引用”，无命中的删掉“在注释里”的归属 |
| `P1-docStyle-ch1Subject` | docStyle | §1.1 L18 | 概念章第一句主语是“`02-higher-half-kernel.md` §4.4 用一张状态表…”，不是 CPU/OS/机制。 | `review-doc-checklist.md` §1.Ch1；原文 | 第一句改为机制主语（“跳转完成时，CPU 的 GDTR/IDTR 仍指向引导方的表”），再引 02 篇 |
| `P1-docStyle-crossArchFrame` | docStyle | §1.2 L42-46；§2.7 L685 | 3×3 表列头写“x86-64”，格子里是 C 的 i386；§2.7 的 ARM 列是 earm-32，§1.2 的 aarch64 列是 64 位；§4.6 收尾未复述 3×3 的 Rust 侧。 | 原文；A3 §2.2/§6.2 | 把 x86 列拆成 C/Rust 两代；给 earm↔aarch64 一句对位；§4.6 末补同构三列表 |
| `P1-docStyle-axisSelfClaim` | docStyle | §1.1 L34 | 称“C 侧按调用顺序展开”，但 §2.3 先 GDT 段填充、§2.4 后讲 `tss_init`，而 `prot_init` 里 `tss_init` 先执行；真实轴是按表分组。 | 文档 §2.3 引用块 vs §2.4；A3 §1.2 | 改自述为“按表分组、组内按调用序”，或调整节序 |
| `P1-docStyle-termLate` | docStyle | §1.4 L87 | “保护结构”在 §1.1–§1.3 已用三次，定义放在 §1.4 的跳过表里。 | 原文；A3 §3.1 | 把定义移到 §1.2 首；§1.4 只留边界 |
| `P1-docStyle-meta` | docStyle | §1.1 L34；§5.4 L1232；§1 L7 | 目录散文剧透 §2.8 的轴；§5.4 为小节存在辩护；教学目标把“淘汰”写进目标。 | 原文；A3 §6.4 | 删/后移；教学目标改问题式 |
| `P1-docStyle-teachingFormalized` | docStyle | §2.1–§4.6 | 22 条反事实在 21 个小节里一节一条，模板指纹明显；读者感到“作者不断宣布这里有重点”。 | A3 §0/§1.2；`grep -c "会怎样"` 22 | 改成“每机制一条”，删重复结论；保留真正有后果的条目 |
| `P1-docStyle-repetition` | docStyle | §3.3/§4.2；§3.1/§4.3；§5.3/§5.4/§5.4-TODO；§1.4/§7.1 | 同一段代码摘录两次（第二次更弱）；SYSRET anchor 规则三次；`R03-T0x` 三次；交棒地图两次。 | A3 §4.2 Z1–Z6；原文 | 删 §3.3 摘录、§4.2 回指段、§4.3 的 anchor 复述；§5.3/§5.4 合一；§1.4/§7.1 二选一 |
| `P1-process-designStale` | process | `.design/03-design.v1.md` | v1（2026-07-16）与现码约 60% 一致：D3 typestate 未实现、D5 无 cfg 被违反、trait 签名不符、D6 引用不存在的 §1.8、`lib.rs:985` 漂移。 | 逐条对照；本轮产出 `03-design.v2.md` | 以 v2 为 Gate H 依据；v1 只作历史参考 |
| `P1-process-coverage` | process | Gate A | 语义域 94/217=43.3% < 80%；71 个“有文档无 Rust”。 | 工具输出；§6 人工判别 | 调整 semantic-map 或把分母限定到本篇射程；补 1–2 个真实缺口后再评 Gate A |

### 9.3 P2（16）

| ID | 类型 | 位置 | 问题 | 证据 |
|---|---|---|---|---|
| `P2-fact-i8259Mask` | fact | §2.2 L262 | “最后把全部中断线屏蔽”与 master 掩码 `~(1 << CASCADE_IRQ)`（IRQ2 未屏蔽）有张力；slave 全屏蔽，所以没有实际源。 | `i8259.c:40,49` |
| `P2-fact-pgIdentityBoundary` | fact | §2.6 L619 | PWT/PCD 按 4MB 对齐块判定，包含 `mem_high_phys` 所在块；不是严格“超出实际内存顶端”。 | `pg_utils.c:162-180` |
| `P2-code-staticMut` | code | `os/arch/src/x86_64/protection.rs:325`；`trap_stub.rs:656`；`smp.rs:1564` | `static mut IST_STACKS`/`GS_AREAS`/`AP_KERNEL_STACKS` 仍用 `static mut`；项目惯用法要求避免。 | `rg "static mut" os --type rust` |
| `P2-code-deadSymbols` | code | `os/kernel/src/lib.rs:2884`；`x86_64/protection.rs:44-47,80-81` | `kernel_may_alloc()` getter 无调用；`X86PrivilegeLevel::get`、`USER_DS_SELECTOR` 带 `#[allow(dead_code)]` 无调用。 | `rg` 结果 |
| `P2-code-sendSyncComment` | code | `os/arch/src/x86_64/protection.rs:188-198` | `unsafe impl Send/Sync` 注释称 `set_kernel_stack` 在 BKL 下调用；生产没有调用，注释的理由悬空。 | `rg "set_kernel_stack"` |
| `P2-outline-drift` | process | `.design/03-outline.v3.md:165` | outline 写 §2.6“三个调用”，文档写“四个调用”（文档正确）；文档新增 §5.4 不在 outline。 | 两份标题/原文 |
| `P2-docStyle-ifdefEarly` | docStyle | §1.3 L69 | `#ifdef` 计数方法论出现在读者还没见 §2.7 源码之前；应移到 §2.7 开头。 | A3 §6.4 |
| `P2-docStyle-outline54` | docStyle | §5.4 | §5.4 是文档新增的反转核对表，outline v3 没有；内容有价值，但应回写 outline 或合并 §5.3。 | outline v3 §5；文档 §5.4 |
| `P2-test-m4-unnamed` | test | §5.4 M4 | 说“真机载体调 configure_syscall 并读寄存器”，没点名读的是表寄存器而非 MSR；读者会误以为 MSR 有断言。 | 同 P1-test-msr |
| `P2-test-m6-unnamed` | test | §5.3 R03-T02 | 说“真机链覆盖了首次用户态进入”，没点名 `test-user-trap`；读者无法复核。 | `run_all.sh:96`；`test-user-trap/src/main.rs:252` |
| `P2-docStyle-titleStyle` | docStyle | §2.3/§2.4/§2.6/§3.2 | 标题是清点式（“五类动作与两张表的容量”“四个调用、两条理由、两份 vir2phys”），不回指 §1.2 的三问；不构成事实错误，但降低概念章的连续性。 | A3 §6.4 |
| `P2-docStyle-sysretRepeat` | docStyle | §3.1 L754；§4.3 L979/L1037 | SYSRET anchor 规则讲三遍；后两遍没有新增推理。 | A3 §4.2 Z2 |
| `P2-fact-emptyLocation` | fact | §2.1 L154 | 说 `EMPTY` 在“同文件头部”；实际在 `alloc_lowest` 函数体内（`pg_utils.c:69`）。 | `sed -n '60,72p' pg_utils.c` |
| `P2-fact-identityMapComment` | fact | §3.5 L867 | 说 `IDENTITY_MAP_END` 旁注释是 `C: pg_identity() maps 1024 × 4MB = 4GB`；实际注释在 `build_bootstrap_root_and_enable` 的 Step 1（`lib.rs:916`），文字是 `Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64)…`。 | `sed -n '914,917p' lib.rs`；`os/arch/src/lib.rs:219` |
| `P2-fact-parityGloss` | fact | §4.4 L1041 | 把源码注释里的 “parity” 解释成“与 DPL 口径对齐”；源码意思是“与 C 的 `gate_table_exceptions[]` 条目集合对齐”（C 也有 NMI/#DF）。 | `trap_stub.rs:622-624` |
| `P2-test-aarch64M2` | test | §5.4 M2/M7 | M2 把 aarch64 独立探针记成“真机装载面已覆盖”；载体自己 `msr vbar_el1`/`msr DAIFSet`，不调 `TrapEntryArch::load`。 | `test-protection-aarch64/src/main.rs:392-410`；无 `CurrentTrapEntry` 调用 |

---

## 10. 卓越性审查

### 10.1 文档卓越（§4.1–§4.5）

- **叙事结构**：七章顺序 = 读者七问，骨架不需要重排；弧线成立。接缝三处（§2.5→§2.6、§2.6→§2.7、§2.7→§3.1）。
- **读者体验**：Ch1 首句主语是书目对象；§1 提前消费 §3/§4 的结论；“保护结构”定义在 §1.4；§1.2 的 3×3 框架前后不一致。
- **教学深度**：A2 闭卷 10 题中 9 题可答/1 题需一步推断；15 节中 12 节 ①、3 节 ②、0 节 ③。§2.8/§3.5/§3.6 是“只给结论”的三处。
- **Design 视角**：§3 七个决策都有“否掉什么/代价”，但 §3 越界进实现（27 字段 TSS 表、`init_protection` 摘录、`GsArea` 结构体）；§4.2 有一段纯回指。
- **组织合理性（13 项）**：骨架/目标读者/主线/概念引入/跨架构/双向闭环/章间移交 7 项 ✅；章内轴/去重/术语前置/元注释/标题回指 5 项 ⚠️；反事实密度 1 项 ❌（模板化）。
- **判定**：**PARTIAL**（Layer 2 不阻断 CONVERGED，但登记 `excellence-pending`）。

### 10.2 代码卓越（§16–§21）

- **§16 API 设计**：`ProtectionArch`/`TrapEntryArch` 两 trait 有 ≥2 行为不同 impl（三架构 + mock），边界合理；但 `init_ap(&self, ...)` 不能写 `sp0`，与 trait 文档“creates per-CPU TSS entry”矛盾；`set_kernel_stack(&mut self)` 无生产调用。
- **§16.6 非法态封堵**：`X86_64Protection` 的 `cpu_id` 用 `assert` 封顶；`Tss64` packed；`PROTECTION`/`TRAP_ENTRY` 用 `Option` + panic accessor，未初始化是确定失败点。**AP “TSS.sp0=0/错栈”没有被类型或断言封住**，是非法态漏网。
- **§16.7 跨架构判定**：两 trait 对三架构表达“特权级+入口+栈”，语义同一；但 `init_protection` 的 6 处 cfg 把 x86 具体类型泄漏进 OS 层。
- **§17 表达力**：`InterruptVector` newtype、`Privilege` enum、`X86PrivilegeLevel`、`Tss64` 字段名与 C 对齐；`STUB_VECTORS` 用 `(u8, bool)` 表，直观。
- **§18 性能**：boot 路径零堆；`IST_STACKS` 静态；`refresh_tss_descriptors` 8 次循环可忽略。
- **§19 代码即文档**：模块注释丰富，但有 6 处路径/节号错误；`init()` 与 `refresh_tss_descriptors` 注释互相矛盾；`init_ap` 注释声称不存在的 `set_kernel_stack` 调用。
- **§20 可测试性**：`X86_64Protection` 的 GDT/TSS 可注入地址测试；`Tss64` packed 需 `read_unaligned`；AP 路径无宿主测试。
- **§21 测试质量**：16/14/11 宿主测试覆盖位与字段；`test_stub_coverage_matches_idt_gates` 是正向覆盖；`configure_ipc_entry_sets_idt_gate_33` 测死代码；AP TSS 无测试；真机不读 MSR。
- **死代码清单**：`configure_ipc_entry`（无生产调用）、`kernel_may_alloc()`（无调用）、`X86PrivilegeLevel::get`（`allow(dead_code)`）、`USER_DS_SELECTOR`（`allow(dead_code)`）。每项“为何死 + 消除影响”见 §9。
- **判定**：**PARTIAL/FAIL**（AP TSS 是正确性缺陷；cfg 泄漏/dead code 是 Layer 2）。

---

## 11. 测试对账（Gate E + test-audit 五维）

### 11.1 计数与名称

| 文件 | `#[test]` 数 | 文档声称 | 一致? |
|---|---:|---:|---|
| `x86_64/protection.rs` | 16 | 16 | ✅ |
| `x86_64/trap_entry.rs` | 14 | 14 | ✅ |
| `x86_64/trap_stub.rs` | 11 | 11 | ✅ |
| `arm64/protection.rs` | 2 | 2 | ✅ |
| `arm64/trap_entry.rs` | 1 | 1 | ✅ |
| `arm64/trap_stub.rs` | 3 | 3 | ✅ |
| `riscv64/protection.rs` | 2 | 2 | ✅ |
| `riscv64/trap_entry.rs` | 1 | 1 | ✅ |
| `riscv64/trap_stub.rs` | 7 | 7 | ✅ |

### 11.2 五维

1. **完备性**：M1/M2/M3/M5/M6/M7/M8/M9/M10 有对应载体；M4（MSR 值）无真机断言；AP TSS.sp0 无测试；负向顺序无载体（R03-T01 成立）。**反向偏差**：§5.2 说“门有效性不在真机载体的覆盖里”过宽——`test-user-trap` 走生产 `init_protection` 并以 CPL3 `int 0x21` 进门，是真机上的门有效性正向覆盖（`P1-test-gateCoverageClaim`）。
2. **测试自身正确性**：`test_stub_coverage_matches_idt_gates` 只比 `gate_present` 与 `has_stub`，不比 handler 地址非零；`configure_ipc_entry_sets_idt_gate_33` 测死代码；`test-smp-ap-alive-mb` 手工补 `set_kernel_stack`，掩盖生产缺口；aarch64 `test-protection` 是探针，不是生产 `TrapEntryArch::load` 的覆盖（`P2-test-aarch64M2`）。
3. **冗余**：`privilege_level_roundtrip` 在三份 protection 里重复，属跨架构同构，可接受；`tss64_size`/`offsets` 与 `init_creates_tss_descriptor_in_gdt` 有部分重叠，可接受。
4. **无效**：未发现空测试/`assert!(true)`；`#[ignore]` 在 boot_integration 有理由（宿主 COM1 SIGSEGV）。
5. **虚构**：§5 声称的测试全部存在；但 M4 的“读寄存器”与载体实际不符（P1）；R03-T02 未点名载体（P2）。

### 11.3 QEMU 静态审计

- `run_all.sh` 三架构清单包含 `test-protection`/`test-protection-aarch64`/`test-protection-riscv64`；PASS 标记分别为 `### TEST_RESULT: PASS test-protection[-aarch64|-riscv64] ###`，与文档一致。
- x86 载体：`CurrentProtection::init/load` + `CurrentTrapEntry::init/configure_syscall(info.syscall_entry)/load`；不调 `install_trap_stubs`；读 GDTR/IDTR/TR/CS/DS；不读 MSR。
- aarch64 载体：独立探针，`VBAR_EL1`/`CurrentEL`/`DAIF`/`SPSel`/只读 `SP_EL1`；不调 `init_protection`。
- riscv64 载体：复用 `CurrentProtection::init/load` + `CurrentTrapEntry::init/load`；读 `stvec`/`sscratch`/`sstatus`。
- 本轮未跑 QEMU（用户决定）：Gate E 标记“静态审计 + 宿主测试”，不把“未跑”写成“通过”。

---

## 12. Rule Discovery（Step 5.7）

| # | 候选模式 | 案例 | 归类 | 是否建议升级 |
|---|---|---|---|---|
| 1 | **引用已删除载体**：文档“逐字引用”一个已不存在的静态/函数，anchor 门 zero-def 是唯一警报 | §4.6 `KERNEL_TRAP_STACK_BASE` | 文档-代码同步 | 建议加入模式库（P0-fact 子类）；每次 review 必跑 anchor `--check` |
| 2 | **同号跨文档假通过**：`doc-ref-check` 只按本文标题解析 `§N.M`，跨文档同号会假绿 | §2.6 `（§2.2）` | 工具局限 | 建议在 review 流程加“跨文档引用逐条开目标文件”人工门 |
| 3 | **注释自述与修复函数冲突**：一个函数注释说“永不需重建”，另一个函数专门重建 | `init()` vs `refresh_tss_descriptors` | 代码注释一致性 | 建议加入 §15.5 理由可质疑性 |
| 4 | **测试补偿掩盖生产缺口**：测试手工调生产没调的函数，使测试通过而生产缺失 | `test-smp-ap-alive-mb` 手工 `set_kernel_stack` | 测试有效性 | 建议加入 test-audit 维度 2（测试自身正确性） |
| 5 | **结构件模板化**：每个小节都有同型反事实，读者失去梯度 | 22 条/21 节 | 教学形式化 | 建议作为 `P1-teaching-formalized` 的新判据（已有同名模式，补“密度”证据） |

> 规则升级不擅自改 `prompt/`；以上为候选，交用户裁决。

---

## 13. 轨迹复盘

> 原始轨迹见 `03-review-trajectory.md`。本节回答“本轮 review 自身有没有遗漏/缺陷/可改进”。

### 13.1 计划维度 vs 实际执行

- 计划中的 R0–R5 全部执行；QEMU 未跑（用户决定），以静态审计 + 宿主测试替代，Gate E 标 PARTIAL。
- 计划中的 A1/A2/A3 全部派出；A1 已返回（1 P0 / 5 P1 / 13 P2），其中 7 条主报告未覆盖的新发现已复跑并并入 §9：`P1-test-gateCoverageClaim`、`P1-fact-vir2physOvergeneral`、`P1-fact-verbatimQuotes`、`P2-fact-emptyLocation`、`P2-fact-identityMapComment`、`P2-fact-parityGloss`、`P2-test-aarch64M2`；其余与主报告一致（riscv P0、boot_procs、D-59、M4、docPaths）。
- 未执行：`cargo test -p minix-arch` 默认形态（38 编译错误）；改用 `--features runtime-window` 通过。默认形态的失败是 feature 配置问题，不是本篇缺陷。
- 未执行：`cargo clippy -D warnings` 因仓库既有 36/75 条告警失败；记录为基线债，不归因于 03。

### 13.2 每条发现的首次出现阶段

- P0-code-apTssSp0：R1 代码维度（主会话读 `init_ap`/`smp_init` 时发现），A1 独立复核一致。
- P0-fact-riscvBase：R0 anchor 门 zero-def + R1 代码对照；A1 独立复核一致，并指出 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs` 明确禁止该静态回归。
- P0-fact-phaseQuote：R1 Rust 引用逐块核对；A1 独立复核一致。
- P0-fact-bootProcs：R1 C 引用/调用关系核对；A1 独立复核一致，并补出 VM 的 `servers/vm/main.c:266/498` 读者。
- P0-fact-intrInitOrder：A2 闭卷学生（独立发现）+ 主会话重放 i8259；A1 判 P2（“安全次序而非数据依赖”），主会话按 §2.0.3 保留 P0，严重度分歧已在 §13.5 记录。
- P1 教学类：A2/A3 独立报告 + 主会话原文核对。
- P1 代码注释类：R1 代码维度 + `grep` 路径；A1 补出 5 个文件的错误路径。
- P1/P2 引用完整性：A1 补出 `by S-4`/`S-4 L2:` 两处逐字引用增补、`EMPTY` 位置、`IDENTITY_MAP_END` 注释、`parity` 释义、`vir2phys` 过度概括、aarch64 M2 探针误记。

### 13.3 零发现反证探针（随机抽 3 条）

| 抽查 | 命令 | 结果 |
|---|---|---|
| `GDT_SIZE=7` | `printf '%d\n' $((6+1))` | 7 ✅ |
| `STUB_VECTORS=41` | `sed -n '/const STUB_VECTORS/,/^];/p' ... \| grep -cE '^\s+\(0x|^\s+\([0-9]+,'` | 41 ✅ |
| `tss_init` 五调用点 | `grep -rn "tss_init(" minix3/minix/kernel/ \| grep -v "int tss_init"` | 5 ✅ |

> 三条均复现；本轮“正向读数”没有发现漂移。
> **反向探针**：A1 独立发现 7 条主报告未覆盖的新 P1/P2（见 §13.2），说明主会话的首轮扫描有盲区；这些项已逐条复跑并入 §9，不是“零发现”放行。

### 13.4 最弱项自检（5 问）

1. **C 源码覆盖**：SYMBOLS 94/217=43.3%，但工具分母过宽；本篇射程内未发现整块函数漏写。最弱项是 `arch_finish_switch_to_user` 只有节选、没有完整函数。A1 另补出 `EMPTY` 位置、`vir2phys` 过度概括、`parity` 释义、`IDENTITY_MAP_END` 注释四处精度问题，均已并入 P1/P2。
2. **链路验证**：Ch3→Ch1&2 ✅；Ch4→Ch3 ✅；测试→Ch3+4 2 处 ⚠️；代码→Ch4 1 处 P0。
3. **跨文档**：01/02/04/05/06/07/10/13/14 路径全部存在；§2.6 的 `（§2.2）` 是同号假通过，已单列。
4. **错误路径**：Ch2 的错误场景（IDT 未装载、TSS 未建、AP 栈错、GS 被清）都有反事实；但 AP TSS.sp0 的错误路径文档没提。
5. **Design 对齐**：v1 过期已重评 v2；v2 记录 AP TSS/cfg 泄漏为偏离；Gate H.2 对 v2 PASS。

### 13.5 “如果我错了，最可能错在哪”

- **AP TSS.sp0**：最可能错在“还有一处我没找到的 `set_kernel_stack` 调用”（如汇编/宏/其他 crate）。已用 `rg` 全仓（排除 target）+ 读 `smp_init`/`smp_ap_tail` 双证；若仍有，证据在 `os/` 之外。
- **`KERNEL_TRAP_STACK_BASE`**：最可能错在“它在另一个文件/另一个名字”。anchor 门 zero-def + `rg` 全仓未命中；若存在，是 anchor 工具的解析问题。
- **D-59**：最可能错在“它在 `timer_irq_gate.rs` 的注释里，只是 grep 没命中”。已 `grep -n "D-59"` 无命中；若存在，是文件/编码问题。
- **`ss0`**：最可能错在“我对 64 位门转场 SS 的记忆有误”。已标 `[待 SDM 复核]`，未升级为 P0。
- **`intr_init` 因果**：最可能错在“`intr_init` 在别的架构实现里会填 IDT”。本篇 C 真值是 i386 的 `i8259.c`；ARM 侧走 `omap_intr.c`，不在本篇射程。

### 13.6 本次 review 自身的缺陷与改进

| 缺陷 | 影响 | 改进 |
|---|---|---|
| A1/A2/A3 与主会话同模型 | 盲点可能重合 | 材料隔离 + A2/A3 只读文档 + 主会话重放；在报告里登记“同模型”局限，不宣称异构 |
| 未跑 QEMU | Gate E 证据弱 | 静态逐行审计 + 宿主测试；明确写“未跑”，不写“通过” |
| 覆盖率工具分母过宽 | Gate A 43.3% 可能误导 | 人工分档 + 在报告里解释；建议调整 semantic-map |
| `03-design.v1` 过期 | Gate H 若直接采信会误判 | 重评 v2；v1 只作历史参考 |
| `doc-ref-check` 同号假通过 | §2.6 的 `（§2.2）` 未被工具拦住 | 跨文档引用逐条开目标文件；建议加人工门 |
| 报告写作时间偏长 | 可能过度检查 | 下一轮按 chapter 拆分；本轮已按 R1/R2 分阶段 |

### 13.7 Layer 判定

- **Layer 1**：**FAIL**（6 P0 open；P0 六类中 fact/code-bug 各占）。
- **Layer 2**：**PARTIAL/FAIL**（文档 3/15 节 ②、结构五接缝、代码 cfg 泄漏/死代码；AP TSS 已升 P0）。
- **CONVERGED**：**否**。只审不修，P0 全部 open。

---

## 14. 确认清单与限制

- [x] 所有 P0 有可重放证据（命令 + 输出 + 锚点）。
- [x] 所有 P1 有位置与修复方向。
- [x] 维度覆盖自检表无静默跳过；Gate A/E 的 PARTIAL 已说明。
- [x] Doc Code Sync 七项已跑。
- [x] 测试名对账全量执行；QEMU 未跑已登记。
- [x] 对抗 agent A1/A2/A3 完成并归因；A1 的 7 条新发现已复跑并入 §9。
- [x] 轨迹含失败与返工（默认 arch 测试失败、clippy 基线失败、Gate G 两次返工、A1 长尾后返回）。
- [x] 零发现探针 3 条复现；A1 反向探针补出 7 条新发现。
- [x] Gate G：预修复版 **CONCERN（A4 87.5%）**；修复后的回归验证见 R2-6/R2-7（A5 待回）。
- [ ] P0 修复：本轮不做（只审不修）。

**限制**：
- 未跑 QEMU；真机行为以静态审计与宿主测试推断。
- 未做逐符号 217 行深审；SYMBOLS 的 AI 列是分档判断。
- `ss0`/`SP_EL1` 的 ISA 细节依赖外部手册，标 `[待 SDM/ARM ARM 复核]`。
- A1 独立代码核查已返回；其 7 条新发现已并入 §9，其余与主报告一致。A4 独立验证在报告落盘时未返回，VERIFY-CHECK 的人工段由主会话按“同 agent + 已重放 grep”标注（见 §3.9）。

---

## 附录 A：本轮可重放命令（关键）

```bash
# 基线
sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md
bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md --check --format=md
AWK=gawk bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md
bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md

# 覆盖率
python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
  --rust-dir os --c-dir minix3/minix/kernel --doc-file 03-kmain-cstart.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --output .review/zcode/01-stage-kernel/03-kmain-cstart/SYMBOLS.md
python3 tools/coverage-extract/coverage-extract.py --check-filled .review/zcode/01-stage-kernel/03-kmain-cstart/SYMBOLS.md

# AP TSS 缺口
rg -n "set_kernel_stack" os/arch/src os/kernel/src os/qemu-tests --glob '!**/target/**'
rg -n "\.sp0\s*=" os/arch/src os/kernel/src --glob '!**/target/**'
sed -n '367,431p' os/arch/src/x86_64/protection.rs
sed -n '1130,1150p;1582,1600p' os/kernel/src/smp.rs

# riscv 静态
sed -n '70,85p' os/arch/src/riscv64/protection.rs
sed -n '114,126p' os/arch/src/riscv64/trap_return.rs

# Phase 引用
sed -n '1355,1361p' os/kernel/src/lib.rs

# boot_procs
sed -n '171p;275p' minix3/minix/kernel/main.c
sed -n '119,159p' minix3/minix/kernel/proc.c

# intr_init
sed -n '28,52p' minix3/minix/kernel/arch/i386/i8259.c
sed -n '68,73p' minix3/minix/kernel/arch/i386/head.S

# 测试
cargo test -p minix-kernel --features mock
cargo test -p minix-arch --features runtime-window
```

## 附录 B：Open Questions

| OQ | 问题 | 需要谁裁决 |
|---|---|---|
| OQ-1 | `ss0` 的 64 位门转场语义以 Intel SDM 还是 AMD APM 为准？ | 架构/文档作者 |
| OQ-2 | `SP_EL1` 在 `SPSel=1` 时 `msr` 是 UNDEFINED 还是合法写？ | ARM ARM 复核 |
| OQ-3 | AP TSS.sp0 修复选“BSP 发 SIPI 前 `set_kernel_stack`”还是“`init_ap` 改 `&mut self`”？ | 代码作者 |
| OQ-4 | `03-design.v1` 是保留历史还是标记 superseded？ | 用户 |
| OQ-5 | Gate A 的 semantic-map 是否应限定到本篇射程？ | 工具/流程维护者 |

---

# 第二轮回归 Review（修复后，2026-11-02）

> **触发**：第一轮 6 P0 / 28 P1 / 16 P2 全部处置后，按用户要求做深度回归 full review。
> **方法**：逐条修复台账 + 四道文档门 + 宿主/交叉编译 + QEMU 三架构 `test-protection` + `test-smp-aps` + A6 闭卷复读；A5 独立修复核查最终 **PASS**（6/6 P0 CLOSED；4 条 P1 首轮 PARTIAL 已在 follow-up 补齐；0 new P0/P1）。
> **本轮改动边界**：只改 `03-kmain-cstart.md`、其关联 Rust 代码、`test-protection` 载体、`.review`/`.design` 制品；未触碰 02 篇、`AI-chats/daily.todo.md` 或其它并发在制品。

## R2-1 修复台账（P0）

| ID | 状态 | 修复内容 | 回归证据 |
|---|---|---|---|
| `P0-code-apTssSp0` | **CLOSED** | `smp_init` 在 `boot_ap` 前对每个 AP 调 `with_protection_mut(|p| p.set_kernel_stack(logical, stack_top))`；`init_ap` 增加 `sp0 == kernel_stack_top - 16` 的 production 断言。 | `os/kernel/src/smp.rs:1144`；`os/arch/src/x86_64/protection.rs:430`；QEMU `test-smp-aps` PASS（4 CPU 全部 online） |
| `P0-fact-apSetKernelStack` | **CLOSED** | 代码补上 AP 侧 `set_kernel_stack` 后，§3.2/§4.3 的“`init` 给 BSP 写、`set_kernel_stack` 给 AP 写”成为事实；`init_ap` 注释同步。 | 同上；`rg set_kernel_stack os/kernel/src/smp.rs` 命中 |
| `P0-fact-riscvBase` | **CLOSED** | §4.6 的伪造代码块删除，改为实际 `set_entry_stack_base`（只写 `sscratch`）；§4.6/§5.4 M8/§6.1/R03-T06 全部改为 per-hart `sscratch` + `prepare_user_return`。 | `anchor-resolve --check` → `zero-def=0`；`rg KERNEL_TRAP_STACK_BASE 03-kmain-cstart.md` 无命中；QEMU riscv `test-protection` PASS |
| `P0-fact-phaseQuote` | **CLOSED** | §3.6 引用块删掉两处 `D-59`，恢复 `lib.rs:1355-1361` 的逐字文本；正文解释改为“Phase B 软件态 / Phase F 硬件编程与门放行”。 | `rg D-59 03-kmain-cstart.md` 无命中；引用块与源码逐行一致 |
| `P0-fact-bootProcs` | **CLOSED** | §2.1 改为“循环读写 `image[]`；循环后 `main.c` 的第二次 `memcpy` 把填好 `start_addr`/`len` 的 `image[]` 同步给 `kinfo.boot_procs`；其读者是 VM”。 | `main.c:171,275`；`proc.c:119-159`；`servers/vm/main.c:266,498` |
| `P0-fact-intrInitOrder` | **CLOSED** | §2.2 表与反事实改为“`intr_init` 不依赖 IDT；保留在 `prot_init` 之后是防御性次序；任何 `prot_init` 之前的异常/NMI 都无门可走”。 | `i8259.c:28-52`；`head.S:68-73`；文档自身 IF=0 句 |

## R2-2 修复台账（P1）

| 组 | ID | 状态 | 修复/说明 |
|---|---|---|---|
| 事实 | `P1-fact-storeKernelInfo` | CLOSED | “三块”改“五块”（含 platform_sources、param_buf） |
| 事实 | `P1-fact-ss0` | CLOSED | 改为“门转场 SS 置 NULL；SYSCALL 的 SS 由 STAR 推出” |
| 事实 | `P1-fact-spel1` | CLOSED | §4.6 统一为 UNDEFINED 编码 + 同步异常；删掉“换掉当前栈”的第二种说法 |
| 事实 | `P1-fact-vir2physOvergeneral` | CLOSED | 改为“看符号落在哪一段”；明确 `pg_load` 用恒等版、`pagedir` 在 `.unpaged_bss` |
| 事实 | `P1-fact-verbatimQuotes` | CLOSED | 删掉 `install_idt_handlers`/`GsArea` 引用块里源码没有的 `by S-4`、`S-4 L2:` |
| 测试 | `P1-test-msr` | CLOSED | `test-protection` 在 `configure_syscall` 后用 `rdmsr` 校验 STAR/LSTAR/SFMASK/EFER.SCE；QEMU x86 PASS |
| 测试 | `P1-test-entryDivergence` | CLOSED | 载体改用生产入口 `syscall_entry_va()`；§5.2 同步 |
| 测试 | `P1-test-gateCoverageClaim` | CLOSED | §5.2/§5.4 M3 补 `test-user-trap` 的真机门有效性正向覆盖 |
| 测试 | `P1-test-apTss` | CLOSED | `init_ap` production 断言；`test-smp-aps` 端到端覆盖；`test-smp-ap-alive-mb` 仍可手工设 sp0 |
| 代码 | `P1-code-deadIpc` | CLOSED | 从 trait/三架构 impl/mock/test 删除 `configure_ipc_entry`；`syscall.rs`/`trap_dispatch.rs` 注释改为 `install_trap_stubs` |
| 代码 | `P1-code-cfgLeak` | CLOSED | 新增 `ProtectionArch::refresh_descriptors`/`program_entry_base`；dispatcher 注册收进 `trap_dispatch::register_current_arch`；`init_protection` 无 `#[cfg]` |
| 代码 | `P1-code-docPaths` | CLOSED | 6 处 `rewrite-notes/03-stage-kernel` → `01-stage-kernel`，节号改为 §3.2/§3.1/§1.2 |
| 代码 | `P1-code-initComment` | CLOSED | `init()` 注释改为“实例搬迁后必须 `refresh_descriptors`” |
| 代码 | `P1-code-initProtectionDoc` | CLOSED | `init_protection` 文档注释改为“装真入口并 `load()`，无丢弃重建阶段” |
| 文档 | `P1-docStyle-riscvLoad` | CLOSED | 明确 `ProtectionArch::load` 无事、`TrapEntryArch::load` 写 `stvec` |
| 文档 | `P1-docStyle-selfCopy` | CLOSED | §2.1 补四行原文（`return &kinfo`/`push %eax`/`#define kmess`/`cbi->kmess`） |
| 文档 | `P1-docStyle-threeSections` | CLOSED | §2.8 补 `assert(prot_init_done)`；§3.5 补“无 CR3”grep；§3.6 补 `mask_all`/`intr.init()` 读数 |
| 文档 | `P1-docStyle-crossRef22` | CLOSED | `（§2.2）` → `02 篇 §2.2` |
| 文档 | `P1-docStyle-processNumbers` | CLOSED | 正文 `D-59`/`D-64①`/`S-4`/`1.109`/`1.113` 全部删除；`grep` 无命中 |
| 文档 | `P1-docStyle-ch1Subject` | CLOSED | §1.1 首句改为 CPU/GDTR/IDTR 主语 |
| 文档 | `P1-docStyle-crossArchFrame` | CLOSED | §1.2 拆 C i386 / Rust x86-64 / aarch64 / riscv64 四列；§2.7 补 earm↔aarch64 对位；§4.6 末补 3×3 Rust 表 |
| 文档 | `P1-docStyle-axisSelfClaim` | CLOSED | 删掉“按调用顺序展开”的自述；改为“按表与职责分组” |
| 文档 | `P1-docStyle-termLate` | CLOSED | “保护结构”定义移到 §1.2；§1.4 只留引用 |
| 文档 | `P1-docStyle-meta` | CLOSED | 删 §1.1 目录散文、§5.4 自我辩护、教学目标结论前置；删“先读/逐字读/读者可以/需要读者”等元话语 |
| 文档 | `P1-docStyle-repetition` | CLOSED | §3.3 代码块改为 E1 注释；§4.3 SYSRET 规则改为回指 §3.1；§5.4 第三张 TODO 表删除 |
| 文档 | `P1-docStyle-teachingFormalized` | **WITHDRAWN** | `style-bible` 第 9 条要求每个分析/设计节至少一处反向推演；A6 复读 15/15 节 ①。这是规则层要求，不是本篇缺陷；转 OQ-6 |
| 流程 | `P1-process-designStale` | CLOSED | 产出 `03-design.v2.md`；v1 标 SUPERSEDED |
| 流程 | `P1-process-coverage` | **TOOL-LIMIT** | 43.3% 的分母是 kernel 全模块 217 符号；人工按 §1.4 射程分档后无整块漏写。工具 semantic-map 调整转 OQ-5 |

## R2-3 修复台账（P2）

| ID | 状态 | 修复/说明 |
|---|---|---|
| `P2-fact-i8259Mask` | CLOSED | 改为“slave 全屏蔽；master 只放开 cascade 线” |
| `P2-fact-pgIdentityBoundary` | CLOSED | 改为“从 `mem_high_phys` 所在 4MB 对齐块起标记；最后含真实内存的块也被一起标记” |
| `P2-code-staticMut` | CLOSED | `IST_STACKS`/`AP_KERNEL_STACKS` 改不可变 `static`；`GS_AREAS` 改 `UnsafeCell` + 手动 `Sync`（x86 三处；其它架构的 `static mut` 不在 03 关联文件表） |
| `P2-code-deadSymbols` | **CORRECTED** | `X86PrivilegeLevel::get` 删除；`kernel_may_alloc()` 由 `memmap::add_memmap` 的 production `debug_assert` 使用；`USER_DS_SELECTOR` 实际被 `trap_stub.rs:437` 使用，原 `#[allow(dead_code)]` 是陈旧标注，已恢复 |
| `P2-code-sendSyncComment` | CLOSED | `Send/Sync` 注释改为“BSP 在 BOOT_LOCK 下写 sp0；AP 只读” |
| `P2-outline-drift` | CLOSED | `03-outline.v4.md` 已把 §2.6 改为“四个调用”、补 §5.4 |
| `P2-docStyle-ifdefEarly` | CLOSED | `#ifdef` 方法论从 §1.3 移到 §2.7 开头 |
| `P2-docStyle-outline54` | CLOSED | §5.4 保留反转表，第三张 TODO 表删除；v4 outline 收录 §5.4 |
| `P2-test-m4-unnamed` | CLOSED | §5.4 M4 明确“x86 载体 `rdmsr` 校验四个 MSR；本轮未跑” → 已跑，见 R2-4 |
| `P2-test-m6-unnamed` | CLOSED | §5.3 R03-T02 点名 `test-user-trap` |
| `P2-docStyle-titleStyle` | **WITHDRAWN** | 清点式标题便于核对覆盖度，A3 也判为“规则偏好，不是文档缺陷”；转 OQ-7 |
| `P2-docStyle-sysretRepeat` | CLOSED | §4.3 反事实改为回指 §3.1，不再复述规则 |
| `P2-fact-emptyLocation` | CLOSED | `EMPTY` 改为“`alloc_lowest` 函数体内（`pg_utils.c:69`）” |
| `P2-fact-identityMapComment` | CLOSED | 改为 `build_bootstrap_root_and_enable` Step 1 的实际注释文字 |
| `P2-fact-parityGloss` | CLOSED | “parity” 改为“与 C 的 `gate_table_exceptions[]` 条目集合对齐” |
| `P2-test-aarch64M2` | CLOSED | §5.4 M2 注明 aarch64 是独立探针，不调 `TrapEntryArch::load` |

## R2-4 回归门禁与测试

| 门/测试 | 命令 | 结果 |
|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check` | `anchors=83 resolved=83 zero-def=0 multi-def=0` |
| 章节引用 | `AWK=gawk tools/doc-ref-check.sh --check` | 全绿 |
| 文风 | `tools/doc-style-lint.sh` | error=0 |
| 承载门 | `tools/doc-code-density-check.sh --check` | 4/4 可判章通过 |
| 关联代码 | `tools/doc-code-map.sh --check` | 21/21 |
| 宿主内核 | `cargo test -p minix-kernel --features mock` | 856 passed / 0 failed / 8 ignored |
| 宿主 arch | `cargo test -p minix-arch --features runtime-window` | 全绿 |
| 交叉编译 | `cargo check -p minix-kernel --target {aarch64-unknown-none,riscv64gc-unknown-none-elf} --no-default-features` | 均 exit 0 |
| 载体编译 | `cargo check -p test-protection --target x86_64-unknown-uefi --features fw-x86-uefi` | exit 0 |
| 真机 x86 | `run_qemu.sh x86_64 .../test-protection.efi` | **PASS**（含新增 `SYSCALL MSRs verified`） |
| 真机 aarch64 | `run_qemu.sh aarch64 .../test-protection-aarch64.efi` | **PASS** |
| 真机 riscv64 | `run_qemu.sh riscv64 .../test-protection-riscv64` | **PASS** |
| 真机 SMP | `run_qemu.sh x86_64 .../test-smp-aps.efi` | **PASS**（4 CPU online；AP TSS.sp0 修复的端到端证据） |
| 覆盖率 | `coverage-extract.py ... --output tmp/review03/SYMBOLS-regression.md` | 94/217=43.3%；人工射程分档无整块漏写（Gate A 仍标 PARTIAL，工具分母问题） |

## R2-5 A6 闭卷回归复读

- **10 题**：Q1–Q9 全部可从文档回答；Q10 需一步推断（MSR 读回证明“写入值”，不证明 SYSCALL 路径）。
- **逐节三态**：15 节 **① 15 / ② 0 / ③ 0**；§2.8/§3.5/§3.6 三处上轮 ② 全部升为 ①。
- **A6 指出的残项**已修：§6.2 计数 14→13；§2.1 块“三行”→四行；§5.4 重复收尾删除；§2.4 `cs`/`ds` 备份字段补“无读者”；§2.7 earm `sp0` 补 `arch_finish_switch_to_user` 读者；§3.4 `GsArea` 补 `program_gs` 写入者；§2.2 `arch_init` 补函数体事实；§2.8 反事实补 AP `ltr`/`sp0` 具体现场。
- **剩余**：§2.6 的 `USER` 位“不替 C 树编动机”是作者主动拒绝解释，属认识论纪律；§2.4 的 32 位 TSS 备份字段“无读者”已写明；不再有 ②/③ 节。

## R2-6 A5 独立修复核查

- **A5 首轮**：6/6 P0 **CLOSED**；22 个抽样 P1 中 18 CLOSED + 4 PARTIAL；0 REGRESSION；1 个新 P2（`A5-N1`：两处 trap_entry 文件头仍指 §1.3）。4 个 PARTIAL 分别是 `P1-code-docPaths`（§1.3 语义错）、`P1-code-initProtectionDoc`（Step 2 inline 注释仍说“later boot phase”）、`P1-fact-storeKernelInfo`（“三块/三个载荷”残留）、`P1-test-apTss`（缺 `sp0+8` CPU-id 戳记断言）。
- **Follow-up 修复**：① 三个 trap_entry 文件头改为 §1.2 + §4.6；② `lib.rs` Step 2 inline 注释改为 `install_trap_stubs` 装真入口并在下面 `load()`；③ 文档 §4.1 的“三块/三个载荷”全部改“五块/五个载荷”；④ `init_ap` 增加 `sp0+8 == cpu_id` 的 `cfg(not(test))` 断言。
- **复验**：anchor 83/83 zero-def=0；doc-ref 全绿；style error=0；density 4/4；code-map 21/21；宿主 856 + arch 289；aarch64/riscv64/test-protection 编译全绿。A5 二轮复核这 4 点全部 CLOSED；唯一新增 P2（`arch/trap_entry.rs` 重复 §1.2 项目符号）已删除。
- **A5 最终判决：PASS**（0 new P0/P1）。
- **A5 的备注**：工作树里 `os/boot-shim/*`、`os/kernel-image/*`、`higher_half.rs`、`minix-elf` 等改动属另一批（02 篇方向）修复，不在 03 射程；未发现它们翻转 03 的 P0/P1 结论。

## R2-7 回归判定

| 维度 | 结果 |
|---|---|
| P0 | **0 open**（6/6 CLOSED，均有回归证据） |
| P1 | **0 open doc**（26 CLOSED；1 WITHDRAWN 规则层；1 TOOL-LIMIT 覆盖率分母） |
| P2 | **0 open doc**（14 CLOSED；1 CORRECTED；1 WITHDRAWN 风格偏好；A5-N1 注释冗余已删） |
| Layer 1 | **PASS**（P0=0；Gate 0/B/C/D/D-6/E/H 有证据；Gate A 标 PARTIAL 但人工射程分档无漏写；Gate G 预修复 CONCERN，回归 A5 二轮复核 **PASS**） |
| Layer 2 | **PASS with note**（A6 15/15 节 ①；结构五接缝中三处已通过改写/回指缓解；§1.4/§7.1 的交棒地图保留为参见索引） |
| 文档状态 | **CONVERGED_WITH_WARNING**（Gate A 工具分母、QEMU 已跑、A5 PASS） |

**一句话**：第一轮的 6 个 P0 全部闭环；AP TSS.sp0 的修复由 `test-smp-aps` 真机 PASS 证实；riscv 的伪造静态已从正文与诊断中清除；`test-protection` 三架构真机 PASS，x86 新增的 MSR 读回也 PASS。剩余风险集中在工具口径（Gate A 43.3%）与 A5 独立核查的最终一致性，不在文档事实或生产代码路径上。

## R2-8 新增 Open Questions

| OQ | 问题 | 需要谁裁决 |
|---|---|---|
| OQ-6 | `P1-docStyle-teachingFormalized` 是否应升级为规则层问题（style-bible #9 要求每节一条反事实，A3 认为模板化）？ | 规则维护者 |
| OQ-7 | 清点式标题（“五类动作与两张表的容量”等）是否保留？A3 判为规则偏好，非文档缺陷。 | 文档作者 |
| OQ-8 | Gate A 的 semantic-map 是否按文档射程过滤，避免 43.3% 的分母误导？ | 工具维护者 |
