# 02-higher-half-kernel 深度 Review 报告

> **审查对象**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（1201 行，sha256 `6d5a6f6c397323f50ac12015745ef23782d67b16c19e507110be0bcebc02bb9f`）
> **审查日期**：2026-11-02
> **审查模式**：完整 Review（文档 + 关联 Rust 代码），深度模式；R1 正确性 + R2 卓越性 + 对抗性审阅 + R3 轨迹复盘
> **真值优先序**：Minix3 C 源码行为 > 设计契约（本轮重推的 `02-design.v3`）> Rust 实现 > 文档 > AI 分析
> **本轮立场**：**只审不修**。P0 全部保持 open；文档状态 **NOT CONVERGED**。修复建议写入报告，不落代码。
> **规范制品**：`.review/zcode/01-stage-kernel/02-higher-half-kernel/`（scan / SYMBOLS / structure / excellence / test-audit / VERIFY-CHECK / trace）；设计快照 `.design/02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md`。
> **文档性质说明**：本文是 review 报告，不是正式章节文档；它保留 `P0/P1/P2`、Gate、scan 等审查术语与日期，这些是取证坐标，不参与正式文档的 `doc-style-lint` 门。

---

## 0. 范围、方法与结论先行

### 0.1 范围

- **文档**：全文 7 章（概述、C 源码分析、Rust 设计决策、实现详解、测试要点、诊断、参见），31 个二级标题。
- **关联 Rust**：`os/kernel-image`（三份 `.ld`、`build.rs`、`main.rs`、`bootface.rs`、`bootface_a64.rs`）、`os/boot-shim`（`loader.rs`、`main.rs`、`uefi_helpers.rs`、`opensbi_helpers.rs`、`uboot_main.rs`）、`os/libs/minix-elf`、`os/libs/minix-boot`（`kernel_info.rs`、`handoff_v1_read.rs`）、`os/kernel`（`lib.rs`、`boot/higher_half.rs`、三份 arch `higher_half.rs`、`boot_policy.rs`）、`os/arch`（分页/direct map/stacktrace）、`os/qemu-tests`（`test-higher-half`、`run_qemu.sh`）。
- **C ground truth**：`i386/{kernel.lds,head.S,mpx.S,Makefile.inc,pre_init.c,pg_utils.c,protect.c}`、`kernel/{const.h,proto.h,main.c}`、`sys/arch/i386/include/asm.h`、`earm/{kernel.lds,head.S,Makefile.inc}`、`servers/vm/pagetable.c`、`lib/libsys/stacktrace.c`、`kernel/arch/i386/exception.c`。
- **不在范围**：不修改受审文档与 `os/` 代码；不跑 QEMU（静态审计）；不重写文风；不修 P0。

### 0.2 方法

1. **R0 预检**：`review-init`、4 条 `ls`、`design-coverage-check`、`doc-code-map --check`、`anchor-resolve`、`doc-ref-check`、`doc-code-density`、`doc-style-lint`；基线 sha256/行数/工作树。
2. **R0.5 设计快照重推**：读 v2 design/v3 outline/outline-review，按 C + 当前 Rust 独立生成 `02-outline.v4`、`02-outline-review.v4`、`02-design.v3`；v2 只作参考与反面教材。
3. **R1 正确性**：C 侧逐文件对账；Rust 侧逐锚点句意复核 + 引文块逐块对账；数字复算；Gate B 行为契约；Gate A 覆盖率；Step 3.6 代码维度；Gate E 测试对账；跨文档；Doc Code Sync 七项。
4. **R2 卓越性**：structure.md 12 节 + 六维反查 + 跨章节一致性；文档/代码卓越性；学生型走查；冷读团 A/B/C；独立 CodeReview 子代理。
5. **R3 收敛**：Issue List、严重度调和、Gate G 抽样验证（14/71，seed 20261102）、Rule Discovery、轨迹复盘。

### 0.3 结论先行

| 维度 | 结果 |
|---|---|
| Layer 1 正确性 | **FAIL**：11 个 P0 open（9 P0-fact + 1 P0-test-missing + 1 P0-design-wrong） |
| Layer 2 卓越性 | **PARTIAL**：C 章/设计章讲法强；被生产路径与覆盖表拖累 |
| Blocker Gates | Gate D / E / H.6 **FAIL**；Gate 0/A/B/C/D-6/G **PASS** |
| 文档状态 | **NOT CONVERGED** |
| 一句话判决 | 文档的**三件事主线、C 源码分析、五个设计决策**是扎实的；问题集中在“生产交接路径停留在旧模型”“riscv64 条件 delta 的证据过期”“覆盖表把未覆盖说成已覆盖”。这些是必须回代码核对的事实问题，不是讲法风格问题。 |

---

## 1. 执行摘要

### 1.1 问题计数

| 级别 | 数量 | 构成 |
|---|---:|---|
| P0 | 11 | 9 条事实错误 + 1 条测试覆盖缺失 + 1 条设计快照过期 |
| P1 | 47 | 28 事实/表述 + 3 结构 + 5 文风/教学 + 5 代码 + 5 测试 + 1 流程 |
| P2 | 13 | 表述/卫生/代码健壮性 |

### 1.2 最关键的八组发现

1. **riscv64 的 asm 证据过期**（P0-fact-01）。文档 §4.3 展示 `la` 后无条件 `add delta` 的 11 条序列；当前 `os/kernel/src/arch/riscv64/higher_half.rs:97-117` 是 `auipc t3,0` + `bltu t3,t4,3f` + 第一 `jalr` + `3: add` + 第二 `jalr`，`delta` 只在“当前 PC 低于 `kern_virt_base`”的 direct leg 加；v1 resume leg 已在高半，不得再加。文档把最核心的“为什么只有 riscv64 需要 delta”讲成了旧版。

2. **x86_64 生产路径写成进程内 `arch_boot`**（P0-fact-02）。当前 UEFI 生产是 `prepare_and_enter_kernel_image(1024)`（`os/boot-shim/src/main.rs:68`）→ `enter_kernel_image_v1`（`uefi_helpers.rs:918`）→ 独立镜像 `_start` → `arch_boot_resume_v1`（`os/kernel-image/src/main.rs:354`）；不是文档 §4.3/§4.4 说的“内核编进 shim、main 在进程内调 `arch_boot`、PC 停在垫片镜像低地址”。

3. **“两形态”表漏掉 v1 handoff，且自相矛盾**（P0-fact-03）。表里没有 send/receive 行；riscv64 行说“跳转完成时 PC 在镜像的物理地址”，而 §4.3 与 §5.2 都说加 delta 后 PC 在 `0xffffffc0…` 高半 VMA。三套分类（链接脚本形态/执行视图/装载路径）到 §4.4 才对齐，表还不完整。

4. **`build_kernel_info` 节选少两个实参**（P0-fact-04）。文档 §4.4 的调用是 8 参；实际 `bootface.rs:310-321` 是 10 参，含 `reserved`（DTB 裁剪出的非传统内存）与 `param_buf`（`/chosen/bootargs` 规范化后的字节）。这两个实参有语义，不是省略号能带过的。

5. **builder 的位置与 `use` 路径写错**（P0-fact-05）。唯一定义在 `os/boot-shim/src/lib.rs:48`；`uefi_helpers.rs`/`opensbi_helpers.rs` 只 `use crate::build_kernel_info`；两个自引导面都 `use boot_shim::build_kernel_info`。文档说“各一份实现、复用 opensbi 那份、`use boot_shim::opensbi_helpers::build_kernel_info`、没有第三份副本”，四点都不成立。

6. **`compute_kernel_layout` 的“保留函数体”块漏三守卫**（P0-fact-06）。当前函数有 `filesz > memsz` 拒绝、`paddr.checked_add(memsz)`、`kern_end.checked_sub(kern_base)`；文档块都没有，且 `InvalidSegment` 被写成“只表示物理区间重叠”，实际有四个产生点。

7. **覆盖表把未覆盖说成已覆盖**（P0-test-missing-01）。M4 用 Rust 的 L7（`kernel_boot_stack_*`）证明 C 的 `k_initial_stktop` 落段；M7 说“按 `p_paddr` 摆放”已覆盖，但宿主只跑 `load_segments_into_buffer`，物理写入唯一执行者是真机；M9 说“两形态都到达 `arch_boot_impl`”，v1 接收根本不走 `arch_boot_impl`。

8. **诊断表把 `AT()` 错归成“高半映射缺失”**（P0-fact-08）与 **panic 消息点名三量不实**（P0-fact-09）。`AT()` 只改 LMA/`p_paddr`，不改 VMA 或页表；它写错的症状是段体内容像另一段/首条指令是数据。`NoKernelPageSize` 的 Display 是 `no kernel page size satisfies the layout`，不含 `kern_virt_base`/`kern_phys_base`/`kern_size` 三个量。

---

## 2. 维度覆盖自检与规则引用

### 2.1 维度覆盖自检

| 检查维度 | 来源 | 应执行 | 实际 | 跳过理由 |
|---|---|---|---|---|
| §2.0 Claims-Evidence Tracing | doc-checklist | ✅ | ✅ | — |
| §2.1 概念准确性 | doc-checklist | ✅ | ✅ | — |
| §2.2 C 代码引用验证 | doc-checklist | ✅ | ✅ | — |
| §2.3 数据结构覆盖 | doc-checklist | ✅ | ✅ | — |
| §2.4 文档与代码一致性 | doc-checklist | ✅ | ✅ | — |
| §2.4b Rust 习惯用法同步 | doc-checklist | ✅ | ✅（`static mut` 名单纠错） | — |
| §2.4c 路径约定 | doc-checklist | ✅ | ✅ | — |
| §2.4d 符号锚点解析 | doc-checklist | ✅ | ✅（64/64） | — |
| §2.4k 文档 Rust 代码块逐块审查 | doc-checklist | ✅ | ✅（§3/§4 逐块） | — |
| §2.5 架构演进说明 | doc-checklist | ✅ | ✅（`[ARCH]` 缺口见 P1） | — |
| §2.6 文档间交叉引用 | doc-checklist | ✅ | ✅（07 篇断链见 P1-fact-16） | — |
| §2.7 图表质量 | doc-checklist | ✅ | ✅ | — |
| §2.8 C 源码覆盖完整性 | doc-checklist | ✅ | ✅（SYMBOLS + 手工表） | — |
| §2.9 设计决策质量 | doc-checklist | ✅ | ✅（v4 outline/v3 design） | — |
| §2.10 章节链路验证 | doc-checklist | ✅ | ✅（structure §12） | — |
| §2.11 文档风格 | doc-checklist | ✅ | ✅（style-lint error=0） | — |
| §3.1–3.4 可读性 | doc-checklist | ✅ | ✅ | — |
| §3.5 教学性 | doc-checklist | ✅ | ✅（学生走查 + 冷读团） | — |
| §1 Rewrite 质量 | code-checklist | ✅ | ✅ | — |
| §2 硬件抽象 | code-checklist | ✅ | ✅ | — |
| §4 执行模型/SMP | code-checklist | ✅ | ✅（启动期单线程；无 BKL 相关） | — |
| §8 命名 | code-checklist | ✅ | ✅ | — |
| §12 no_std | code-checklist | ✅ | ✅ | — |
| §13 设计-代码一致性 | code-checklist | ✅ | ✅（v3 12/12） | — |
| §14 C-Rust 语义对齐 | code-checklist | ✅ | ✅ | — |
| §15 细节精确性 | Gate C | ✅ | ✅ | — |
| 跨文档联动 | patterns | ✅ | ✅ | — |
| 测试五维 | test-audit | ✅ | ✅（`test-audit.md`） | — |
| 卓越性 §16–21 | code-excellence | ✅ | ✅（`excellence.md`） | — |
| Gate 0/A/B/C/D/D-6/E/G/H | review-process | ✅ | ✅（D/E/H.6 FAIL） | — |

### 2.2 规则引用清单（每条结论可回溯）

| 规则文件 | 本轮实际使用的条款 | 落地 |
|---|---|---|
| `review.md` | 真值优先序；P0 六分类；行为语义优先；硬件抽象；概念抽象；AI 输出模板；自检 1–5；Layer 1/2 | §0.3、§1、§9、§10、§15 |
| `review-process.md` | Step 0–7；Gate 0/A/B/C/D/D-6/E/G/H；L1/L2/L3；Step 0.3 快照；Step 3.6 代码维度；Step 5.6 独立验证；Step 5.7 Rule Discovery；Step 7.1 收敛 | §3、§9、§12、§13、§14 |
| `review-cmds.md` | §一b 决策树；§二 full-review；§一 通用强制门 #1–#9（锚点/测试名/文风/translate/fix-guard/文档-代码同步/产物标注/Ground Truth/教学承载） | §2、§4、§9、§10 |
| `review-doc-checklist.md` | §1 结构；§2.0–2.11；§3 可读性；§4 Doc Code Sync 七项 | §2、§4、§5 |
| `review-code-checklist.md` | §1 Rewrite；§2 硬件抽象；§4 执行模型；§8 命名；§12 no_std；§13 设计-代码；§14 C-Rust；§15 精确性 | §2、§9、§10 |
| `review-core-semantics.md` | 8 字段行为契约；§1.6 四条公理与三问 | §7、§8 |
| `review-patterns.md` | §0 P0 必检 5 项；模式 1–15（文档）、16–29（代码）、35–40（测试）、48–78（叙事/流程）、79–85（架构/锚点） | §9、§13 |
| `review-doc-excellence.md` | §4.1–4.4（教学深度、组织合理性、演进、读者体验） | §10 |
| `review-code-excellence.md` | §16–21（分层、trait、非法态、跨架构、死代码） | §10 |
| `style-bible` | 技术博客文风；禁黑话/缩写/文言文；锚点纪律；过程痕迹禁令；教学构件 | §10、§15 |
| `doc-gen-prompt.md` | 两条总则（骨架不重排、不删教学）；§1.3 资产对账；§1.5 认知四段；§2 十二条写作底线；§4.6 学生走查 | §5、§10、§14 |
| `DOC-ITER-WORKFLOW.md` | S1.0 证据分级；S1.2 引文块逐块对账；S1.4 主张类型选真值源；S1.6 阶段边界；S1.7 主张—依据—适用范围；S2.0 五问；V2 盲测；V3 CodeReview；V5 冷读团；§八 归因；§九 三道理门 | §8、§10、§12、§14 |

---

## 3. Blocker Gates 证据

| Gate | 判定 | 证据 |
|---|---|---|
| Gate 0 | ✅ | scan.md 9 个锚段 + Artifact Inventory |
| Gate A | ✅ | coverage-extract 实跑（1390 C / 160 语义域；21/160 文档、7/160 Rust）；`--check-filled` exit 0 |
| Gate B | ✅ | Top5 行为契约表（§7） |
| Gate C | ✅ | Precision Check 5 元规则（scan §Step 3.5） |
| Gate D | ❌ | P0 必检 5 项：概念准确性 FAIL、C 引用 FAIL、数据结构 PASS、文档-代码 FAIL、测试覆盖 FAIL |
| Gate D-6 | ✅ | structure.md 15 节（12 节模板 + 六维反查 + 章节意图 + 复述） |
| Gate E | ❌ | 23/15 测试计数对；M4/M7/M9 覆盖声明不成立 |
| Gate G | ✅ | VERIFY-CHECK 14/14 = 100%；同 agent + 已重放命令；不表示 CONVERGED |
| Gate H | ❌（H.6） | H.1–H.5 PASS；H.6 doc↔v4 outline 有 7 处 P0 偏离 |

**关键说明**：Gate D/E/H.6 的 FAIL 是文档状态，不是证据缺失；每个 FAIL 都在 §9 有对应 P0。

---

## 4. Doc Code Sync 七项

| # | 检查项 | 结果 | 证据 |
|---|---|---|---|
| 1 | 符号/函数存在性 | ✅ | anchor-resolve 64/64；doc-code-map 24/24 |
| 2 | 函数签名/参数 | ❌ | `build_kernel_info` 8 vs 10 参（P0-fact-04）；`compute_kernel_layout` 缺三守卫（P0-fact-06） |
| 3 | 常量/数值 | ❌ | ARM 距离 0xF0000000 vs 0x70200000（P1-fact-01）；panic 三量（P0-fact-09） |
| 4 | 行为/时序 | ❌ | riscv64 无条件 delta vs 条件 delta（P0-fact-01）；x86 内联 vs v1（P0-fact-02） |
| 5 | 路径/文件 | ✅ | 24/24；07 篇存在性表述自相矛盾（P1-fact-16） |
| 6 | 测试名/计数 | ⚠️ | 23/15 对；M4/M7/M9 覆盖声明错（P0-test-missing-01）；`cargo test --lib` 41 总（P1-test-05） |
| 7 | 注释/引文保真 | ❌ | §4.3 asm 节选、§4.4 调用、§3.3 builder 位置（P0-fact-01/04/05） |

---

## 5. 结构分析摘要（Gate D-6）

- **主题思想**：一份镜像的符号值、装载位置、运行期可访问性由三个不同角色决定；三件事必须对齐。
- **叙事主语**：Ch1 是机制，Ch2 是 C 产物，Ch3 是设计问题，Ch4 是函数/路径；概念驱动为主。
- **叙事弧**：起（两套地址）→ 承（C 五节）→ 转（Rust 五决策）→ 合（实现/测试/诊断）；完整。
- **核心概念**：VMA/LMA、`AT()`、unpaged/objcopy、相对/绝对寻址、恒等映射临时性、栈顶按值交接、`HigherHalf` 契约、2MiB 收口、v1 handoff、设备窗口。
- **结构问题**：① v1 handoff 没有设计决策节（P1-struct-02）；② 三套“形态”无唯一登记表（P1-struct-01）；③ §4.1 越界到 07 篇（P1-struct-03）；④ §1.1 与 §2.1 三常量重复（P1-docStyle-05）。
- **读者复述**：能说出三件事、各自失败落点与“必须成对”；卡点在“哪种形态下 PC/SP 在哪一侧”，需 §4.4 展开。
- **判定**：骨架 PASS；P0-struct 0；P1-struct 3。

---

## 6. 覆盖率穷举（Gate A）

| 指标 | 全量 | 语义域 |
|---|---:|---:|
| C 符号 | 1390 | 160 |
| 文档覆盖 | 21（1.5%） | 21/160（13.1%） |
| Rust 覆盖 | 146（10.5%） | 7/160（4.4%） |
| 有文档无 Rust | — | 14 |
| Gaps | 1230 | 恒 0（废弃口径） |

**解释**：覆盖率工具按“文档提及的符号”取语义域，测不出“整段未提的 C 符号”；本篇射程的 160 个符号里，文档覆盖 21 是工具口径，不代表漏写。真正的覆盖检查由 outline 的代码放置清单、SYMBOLS 语义列与 §2.8 手工表承担。`--check-filled` exit 0。

**C 侧缺口（不是工具口径，是人工判定）**：
- `i386/pg_utils.c` 的 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` 五函数只在 §2.2 一笔带过，逐函数契约归 01 篇；本篇只保留 `pg_mapkernel` 的 961/4MB 论证。可接受。
- `i386/protect.c` 只保留 `tss_init(0,&k_boot_stktop)` 一个读者；`prot_init` 归 03 篇。可接受。
- `mpx.S` 的 `k_stacks_*` 只在 §2.4 列容量；per-CPU 栈数组的用途归 SMP 篇。可接受。

---

## 7. Top 5 行为契约表（Gate B）

| # | 函数/位置 | C 行为（或 doc 声称） | Rust 行为 | 差异类型 | 严重度 | C 证据 | Rust 证据 | Reviewer 备注 |
|---|---|---|---|---|---|---|---|---|
| 1 | `compute_kernel_layout` | doc §4.2 节选：无 `filesz>memsz`/checked 守卫 | 当前有三守卫 | doc↔code 描述差异 | P0 | GRUB 内部无对应 | `loader.rs:143-175` | 代码块标“保留函数体”却漏三守卫 |
| 2 | `load_segments_into_phys_memory` | 按 `paddr` 拷贝 `filesz`、清零 `[filesz,memsz)`、重叠拒绝 | 同；半开区间判交叠 | 语义一致 | — | GRUB 内部 | `loader.rs:225-249` | 宿主只测重叠/buffer 孪生 |
| 3 | `SegmentIter::next` | doc §3.2 “只在两处判断” | 三个 `continue`：`parse_phdr` 失败、非 `PT_LOAD`、越界 | doc↔code 描述差异 | P1 | ELF 规范 | `minix-elf:352-388` | 第三个是防御性解析 |
| 4 | `HigherHalf::jump_to_kmain`（riscv64） | doc §4.3：`la` 后无条件 `add delta` | `auipc t3,0` + `bltu` 条件加 delta + 双 `jalr` | doc↔code 描述差异 | P0 | C 无 riscv | `riscv64/higher_half.rs:97-117` | direct/resume 两腿 |
| 5 | `build_kernel_info` | doc §4.4 调用 8 参（无 `reserved`/`param_buf`） | 10 参 | doc↔code 描述差异 | P0 | `pre_init` 语义 | `bootface.rs:310-321`、`lib.rs:48-58` | 两个实参有语义 |

---

## 8. 纵向链路与因果链抽样（含主张—依据—适用范围）

### 8.1 纵向链路

| Ch1 概念 | Ch3 决策 | Ch4 实现 | Ch5 测试 | 链路完整? |
|---|---|---|---|---|
| 双地址 | D-1 三形态脚本 | §4.1 三份 `.ld` | L0/L3b/L3c/L4 | ✅ |
| `AT()` | D-1 生产 `AT()` | `load_segments_into_phys_memory` | L3c/L4 | ✅ |
| 两段式 | 淘汰（无对应） | 无 | 无 | ⚠️ 缺“为什么不需要测试”的说明 |
| 两种寻址 | D-3/D-6 trait + `!` | §4.3 三份 asm | §5.2 三断言 | ✅（但 riscv64 证据过期） |
| 恒等映射临时性 | D-4/D-10 换栈/交接 | §4.3/§4.4 | §5.2 `sp_ok` | ✅ |
| 栈顶 | D-4 按值交接 | `build_kernel_info`/`kern_stack_top` | §5.2 合成值；L7 | ✅ |
| 大页粒度 | D-11 2MiB 收口 | §3.5/§4.1 | L5/L9 | ✅ |
| v1 handoff | ❌ 无设计决策 | §4.4 | §5.4 M9（错） | ❌ P1-struct-02 |

### 8.2 主张—依据—适用范围表（S1.7）

| 主张 | 依据（级别 + 出处） | 适用范围 | 重述处 | 一致性 |
|---|---|---|---|---|
| 符号值、装载位置、运行期可访问性是三件独立的事 | E1：`kernel.lds` 三常量/`AT()`；Rust `compute_kernel_layout` 取 min(vaddr)/min(paddr) | C 与 Rust 生产镜像；测试载体单地址脚本不适用 | §1.1/§1.3/§2.1/§2.5/§3.1 | ✅ |
| `AT()` 只决定 LMA，不影响 VMA/页表 | E1：`kernel.lds` `AT(ADDR(sec)-offset)`；`readelf -lW` 的 `p_paddr` | 所有带 `AT()` 的段 | §2.1/§3.1/§4.2 | ❌ §6.1 写成“映射缺失” |
| 换栈与跳转必须成对且先换栈后跳 | E1：`head.S` 四行；E3：恒等映射移除链 | C 低→高；Rust v1/aarch64/riscv direct；测试载体 inline | §2.3/§3.4/§4.3/§5.2 | ✅ |
| `kern_stack_top = kern_virt_base + kern_size = 镜像尾` | E1：`lib.rs:70`；`compute_kernel_layout`；2MiB 收口 | 所有用 `KernelInfo` 的形态 | §3.3/§4.2/§5.2 | ✅（但“两个栈都在高侧”范围漂移） |
| 只有 riscv64 需要 delta | E1：`riscv64/higher_half.rs` 条件分支；E3：`la` 的 PC 相对 | direct leg（物理视图）；resume leg 不需要 | §4.3/§4.4 | ❌ 旧 asm/表把 direct 规则写成普遍规则 |
| v1 接收侧只做 additive 阶段 | E1：`resume_after_v1_decode` 不调 `arch_boot_impl`；`new_from_page` 清零活动根 | x86_64/aarch64/riscv64 U-Boot | §4.4/§5.4 | ❌ §4.4 L995 说“两形态合流到 arch_boot_impl” |

---

## 9. Issue List

### 9.1 P0（11 条，全部 open）

#### P0-fact-01：§4.3 riscv64 asm 节选与当前代码不符

- **位置**：doc 880–919。
- **文档声称**：`delta` 在 `la` 后无条件 `add`；asm 只有 11 条；没有 `auipc`/`bltu`/第二 `jalr`；没有 resume leg。
- **真值**：`os/kernel/src/arch/riscv64/higher_half.rs:97-117` 有 `auipc t3, 0`、`la t1, {kmain}`、`bltu t3, t4, 3f`、第一 `jalr`、`3:`、`add t1, t1, t2`、第二 `jalr`，以及 `in("t4") kvirt`；`delta` 只在当前 PC 低于 `kern_virt_base` 的 direct leg 加；v1 resume leg 已由 shim 跳到高半，不再加。
- **证据**：`sed -n '97,117p' os/kernel/src/arch/riscv64/higher_half.rs`；01 篇 §3.7 也写“`auipc` 读当前 PC 与 `kern_virt_base` 比较后分道”。
- **规则**：`review.md` P0-fact；`DOC-ITER-WORKFLOW.md` S1.2 引文块逐块对账。
- **修复方向**：按当前 asm 重写节选与三条 bullet；把“direct leg 加 delta、resume leg 不加”写成显式分支；补 `arch_boot_resume_v1` 的来源。

#### P0-fact-02：§4.3/§4.4 把 x86_64 生产路径写成进程内 `arch_boot`

- **位置**：doc 933、952、969。
- **文档声称**：x86_64 UEFI 生产“内核编进 boot-shim 自己的镜像，`main.rs` 在进程内调 `arch_boot`”，PC 停在垫片镜像低地址。
- **真值**：`os/boot-shim/src/main.rs:68` 调 `prepare_and_enter_kernel_image(1024)`；`uefi_helpers.rs:918` 调 `minix_kernel::enter_kernel_image_v1`；独立镜像 `_start` 由 `os/kernel-image/src/main.rs:354` 调 `arch_boot_resume_v1`。内核以独立 ELF 装在高半 VMA（`readelf` 首段 `0xffff800000200000`）。
- **证据**：上述三行；01 篇 §3.7“三种形态”。
- **规则**：P0-fact；Doc Code Sync 第 4 项。
- **修复方向**：x86_64 行改“v1 send/receive + 高半 VMA”；内联只留测试载体；§4.4 的路径图重画。

#### P0-fact-03：§4.4 “两形态”表漏 v1 handoff，riscv64 PC 列自相矛盾

- **位置**：doc 965–971。
- **文档声称**：表覆盖生产矩阵；riscv64 跳转完成时 PC 在“镜像的物理地址”。
- **真值**：v1 handoff 的 send/receive 不在表内；`arch_boot_resume_v1`/`arch_boot_resume_high_half` 不在表内；riscv64 direct leg 加 delta 后 PC 在高半 VMA（§4.3 L917、§5.2 L1066 自证）。
- **证据**：doc 917/1066；`lib.rs:326/386/504`；`kernel-image/main.rs:354/387/411`。
- **规则**：P0-fact；`review-doc-checklist.md` §2.10 链路。
- **修复方向**：补 v1 行；riscv64 列改“高半 VMA（delta 已加）”；列名从“跳转完成时 PC 所在侧”改“进入视图/完成时 PC”两列。

#### P0-fact-04：§4.4 `build_kernel_info` 节选少 `reserved`/`param_buf`

- **位置**：doc 973–989。
- **文档声称**：“保留 KernelInfo 组装与 arch_boot 调用”，块内 8 参。
- **真值**：`os/kernel-image/src/bootface.rs:310-321` 是 10 参：`memmap, reserved, VirBytes(KERN_VIRT_BASE), PhysBytes(KERN_PHYS_BASE), KERN_SIZE, modules, PhysBytes(0), 0, platform_sources, param_buf`；签名在 `os/boot-shim/src/lib.rs:48-58`。
- **证据**：`sed -n '305,325p' os/kernel-image/src/bootface.rs`；`sed -n '48,82p' os/boot-shim/src/lib.rs`。
- **规则**：P0-fact；Doc Code Sync 第 2 项。
- **修复方向**：补两参；说明 `reserved` 来自 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)` 的裁剪结果，`param_buf` 来自 `/chosen/bootargs` 的 `normalize_param_buf`。

#### P0-fact-05：§3.3 builder 位置/`use` 路径写错

- **位置**：doc 491–496。
- **文档声称**：`uefi_helpers.rs` 与 `opensbi_helpers.rs` 各一份实现；自引导面 `use boot_shim::opensbi_helpers::build_kernel_info`；没有第三份副本。
- **真值**：唯一定义在 `os/boot-shim/src/lib.rs:48`；两个 helper 只 `use crate::build_kernel_info`；两个 bootface 都 `use boot_shim::build_kernel_info`；`opensbi_helpers` 不 re-export。
- **证据**：`grep -rn 'fn build_kernel_info' os` → 仅 `lib.rs:48`；`grep -n 'use boot_shim' bootface.rs bootface_a64.rs` → `boot_shim::build_kernel_info`。
- **规则**：P0-fact；锚点句意复核。
- **修复方向**：改为单份 `lib.rs:48`；`kern_stack_top` 赋值行在 `lib.rs:70`，不是 `uefi_helpers.rs`。

#### P0-fact-06：§4.2 `compute_kernel_layout` “保留函数体”块漏三守卫；`InvalidSegment` 定义不完整

- **位置**：doc 758–787、470。
- **文档声称**：函数体无 `filesz>memsz`/checked；`InvalidSegment` 只表示物理区间重叠。
- **真值**：`loader.rs:143-175` 有 `if seg.filesz > seg.memsz { return Err(InvalidSegment) }`、`paddr.checked_add(memsz)`、`kern_end.checked_sub(kern_base)`；`InvalidSegment` 产生点还有 150/155/167/231 四处。
- **证据**：`sed -n '143,175p' os/boot-shim/src/loader.rs`；`grep -n 'InvalidSegment' loader.rs`。
- **规则**：P0-fact；S1.2。
- **修复方向**：代码块补三守卫；`InvalidSegment` 改“多原因：`filesz>memsz`、地址溢出、长度下溢、物理重叠”。

#### P0-fact-07：“两形态在 `arch_boot_impl` 之后合流”不成立

- **位置**：doc 995、1090。
- **文档声称**：两形态都经 `arch_boot_impl` 建根/开分页/DM，然后跳转；M9 两形态都到达 `arch_boot_impl`。
- **真值**：v1 接收走 `arch_boot_resume_v1` → `resume_after_v1_decode`，不调用 `arch_boot_impl`；`enter_kernel_image_v1` 在 shim 侧调 `build_bootstrap_root_and_enable`。只有 direct 面（`bootface`/`arch_boot`）走 `arch_boot_impl`。
- **证据**：`lib.rs:386-470`、`lib.rs:595-650`；`grep -n 'arch_boot_impl' lib.rs` → 805 定义，未在 resume 路径调用。
- **规则**：P0-fact；Gate B。
- **修复方向**：改为“direct 面走 `arch_boot_impl`；v1 接收面在 shim 建根后走 additive resume”。

#### P0-fact-08：§6.1 把 `AT()` 错归成“高半映射缺失”

- **位置**：doc 1106。
- **文档声称**：分页开启后立刻取指失败、故障地址是高地址 → 高半映射未建立或范围不足（§2.1 的 `AT()` 写错、§3.5 的跨距不足）。
- **真值**：`AT()` 只改 LMA/`p_paddr`，不改 VMA 或页表；它写错的症状是段体内容像另一段/首条指令是数据（doc §2.1 L133-137 自证）。高地址页错误的怀疑对象应是高半映射建立/范围（`kern_vir_base`/`kern_size`/`map_huge`）。
- **证据**：doc 133-137 与 1106 对照；`readelf -lW` 的 `p_paddr`/`p_vaddr` 两列。
- **规则**：P0-fact；模式 48 因果链。
- **修复方向**：拆两行；`AT()` 错 → “内容错位/首条指令是数据”；高地址页错误 → “映射范围/页表”。

#### P0-fact-09：§3.5/§6.1 “panic 消息点名三个量”不实

- **位置**：doc 586、612、1109。
- **文档声称**：`NoKernelPageSize` panic 消息点名 `kern_virt_base`/`kern_phys_base`/`kern_size` 三个量/三个对齐条件。
- **真值**：`KernelMapError::NoKernelPageSize` 的 Display 是 `no kernel page size satisfies the layout`（`boot_policy.rs:51`）；`arch_boot_impl` 是 `panic!("arch_boot: page-size policy: {e}")`（`lib.rs:822`）。不含三个量。
- **证据**：上述两行；`grep -n 'NoKernelPageSize' os/kernel/src/boot_policy.rs`。
- **规则**：P0-fact；诊断命令预期输出。
- **修复方向**：改“消息只报无可用粒度，三个量用 `readelf -lW` 自算”，或给 panic 补三量。

#### P0-test-missing-01：§5.4 M4/M7/M9 覆盖声明与证据错配

- **位置**：doc 1085、1088、1090。
- **文档声称**：M4 `k_initial_stktop` 落段“已覆盖（L7）”；M7 按 `p_paddr` 摆放“已覆盖”；M9 两形态到达 `arch_boot_impl`“已覆盖”。
- **真值**：L7 检查 Rust `kernel_boot_stack_*`（64KiB、末段内），不是 C `k_initial_stktop`；M7 宿主只跑 `load_segments_into_buffer`，物理写入唯一执行者是真机；M9 v1 接收不走 `arch_boot_impl`。
- **证据**：`check-layout.sh:226-240`（L7 对象）；`loader.rs:184-219`（buffer 孪生）；`lib.rs:595-650`（v1 接收）。
- **规则**：`review.md` P0-test-missing；Gate E。
- **修复方向**：M4 改“无自动断言（R02-T01）”；M7 改“语义覆盖 + 真机”；M9 改 v1 路径；补 R02-T04（生产 `kmain` 入口对齐）。

#### P0-design-wrong-01：`.design/02-design.v2.md` 过期，Gate H 不能直接用它

- **位置**：`.design/02-design.v2.md` D-1、§6.2、§4.3。
- **v2 声称**：“不保留 `AT()`”“link.ld 未设 ENTRY”“riscv `la` 后无条件加 delta”“x86 进程内 arch_boot”。
- **真值**：三份生产脚本各 4 条 `: AT(`；`ENTRY(_start)`；条件 delta；v1 handoff。v2 与代码冲突。
- **证据**：v2 文本 + `grep -c ': AT(' os/kernel-image/*.ld` → 4/4/4；`build.rs`；`riscv64/higher_half.rs`；`main.rs:68`。
- **规则**：`review.md` P0-design-wrong；Gate H.2/H.4。
- **修复方向**：已重推 `02-design.v3.md`（12 条决策，12/12 与代码一致）；v2 标 superseded；后续 review 先比快照与当前代码，不把旧快照当 ground truth。

### 9.2 P1（47 条，按主题分组）

#### A. 事实/表述（28 条）

| ID | 位置 | 问题 | 证据 | 修复 |
|---|---|---|---|---|
| P1-fact-01 | §2.3 L266 | ARM `kmain` 与调用点“相距约 0xF0000000” | 实际 `0x70200000`（1794 MiB）；`bl` ±32MB 的结论仍成立 | 改数字；保留“超出范围” |
| P1-fact-02 | §3.1 L382 | 表把 `KERNEL_HIGH_BASE` 直接映射到 `kern_virt_base` | x86_64 `kern_virt_base = HIGH_BASE+PHYS_BASE`（首段 VMA）；aarch64/riscv 才是 `VIRT_BASE` | 拆行/注明两种约定 |
| P1-fact-03 | §3.3 L513 | “入口 64KiB 引导栈与 `kern_stack_top` 都在高一侧” | 物理视图下 `_start` 的 `lea rsp,[rip+...]` 取低物理地址；高半视图才取高 | 按形态分列“入口 SP 所在侧”与“换栈目的” |
| P1-fact-04 | §3.4 L551-555 | 返回 `()` 的理由“调用者栈帧在低地址一侧” | 高半视图下调用者帧在高侧；真正理由是换栈后返回地址不在当前栈 | 改理由；保留 `-> !` 结论 |
| P1-fact-05 | §4.3 L931 | aarch64 `ldr`+`br` “以运行地址=链接地址为前提” | 字面量值是链接期绝对值；literal 读取是 PC 相对；§2.3 又说它不依赖该前提 | 按“值绝对/读取 PC 相对”拆开讲 |
| P1-fact-06 | §4.3 L935/L941 | 标题第三例“去掉 `push 0`”，正文第三例“去掉 `xor rbp`” | 标题与正文不一致；`push 0` 只在括注里作对齐问题 | 标题改“清帧指针”；`push 0` 单列 |
| P1-fact-07 | §4.1 L688 | “孤儿节会被排到 `ALIGN` 之后，跨距非 2MiB（L5 会红）” | x86_64 `.lrodata.*` 是未被 `.rodata` 通配收走的孤儿，但落在收口前，L5 通过 | 改为“落点由链接器版本决定，可能破坏 L5”；用 `.lrodata` 实例说明 |
| P1-fact-08 | §4.1 L705/L643 | “aarch64 与 x86_64 逐段同形，只有基址与 AT 表达式不同”；`.text` 节选“只保留 .text 与 .rodata 段体” | x86_64 多 `.ap_early_entry`/`.ltext`，无 `OUTPUT_ARCH`；节选还漏 `PROVIDE(__text_end)` | 补差异；节选标注“注释/尾部有删节” |
| P1-fact-09 | §4.2 L824 | 自引导面“三个 `static mut`：`MODULE_TABLE`、`MEMMAP_STORAGE`、`PLATFORM_SOURCE`” | `bootface.rs` 只有 `MODULE_TABLE`/`PLATFORM_SOURCE`；`bootface_a64.rs` 是 `BUMP_PTR`/`MODULE_TABLE`/`PLATFORM_SOURCE`；无 `MEMMAP_STORAGE` | 按文件列正确名单 |
| P1-fact-10 | §4.5 L1015 | “装载者从 GRUB 换成 boot-shim 或镜像自己” | 自引导镜像不重拷自己的段（§3.1 L339、§4.4 L993 自证）；摆放者是 U-Boot/OpenSBI | 改“boot-shim 或引导器” |
| P1-fact-11 | §2.2 L214、§4.5 L1023 | “高地址一侧没有任何映射”导致取指失败 | 分页未开，没有“映射”层；故障是物理地址无 DRAM/MMIO | 改“高物理地址没有 DRAM 背书” |
| P1-fact-12 | §2.4 L302 | `0x526F` 魔数“必须落在文件开头（只读第一个扇区）” | 源注释“must be the first data entry”；文件头说的是 text 段常量 | 改注释归属 |
| P1-fact-13 | §2.3 L236 | 帧指针链“走到 0 才停” | 还有 `hbp <= bp` 防环终止 | 补终止条件 |
| P1-fact-14 | §2.2 L212 | ARM 名单差异不完整 | 还少 `_cpufeature.o`/`_cpuid.o`，多 `${BSP_OBJS_UNPAGED}` | 补齐 |
| P1-fact-15 | §5.4 M6 L1087 | “minix-elf 23 测试含两个 loader 侧变体” | `NoLoadSegments`/`InvalidSegment` 测试在 `boot-shim/loader.rs` | 改证据列 |
| P1-fact-16 | §1 L10、§4.1 L715、§7.1 L1176 | 07 篇“尚未落到本目录、链接已撤”，但又给链接 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md` 存在 | 统一为活链接 |
| P1-fact-17 | §3.5 L561/L584 | “三量整除”未定义；1GiB 档不可达只说基址不对齐 | 三量是 `kern_virt_base`/`kern_phys_base`/`kern_size`；当前 4MiB 跨距本身不可能是 1GiB 倍数 | 定义三量；补 1GiB 不可达的第二理由 |
| P1-fact-18 | §3.3 L515 | 收口填充与 64KiB 引导栈“都被清零” | 入口 asm 只清到 `__bss_end`；收口填充在 `__bss_end` 之后，自引导面不保证清零 | 改“装载方清到段末/入口清到 __bss_end” |
| P1-fact-19 | §2.5 L327 | 归纳表把“跳转携带相对位移”写成通用结论 | ARM `bx` 携带绝对值（§2.3 L266 自证） | 按架构拆行 |
| P1-fact-20 | §2.3 L238 | “位移略大于 0xF0000000，落在 0xF000_xxxx”；“低侧总长远大于后三项” | `0xF000_xxxx` 需要 `L+O−C−5 < 0x10000`，文档未给上界；只需“约 0xF0000000、绝对值<2^31” | 删精确窗口断言或给 `nm` 读数 |
| P1-fact-21 | §1.1 L57 | `USR_DATATOP` 的 `0xE0000000`/`0xF0000000` 两构建与“0xe.. 对 0xf..”解释混写 | 只有 `_MINIX_MAGIC` 构建是 0xE0000000；默认构建用户顶 0xF0000000，与内核 0xF0400000 同前缀 | 写清两种构建各取何值 |
| P1-fact-22 | §4.4 L954/L1005 | `arch_boot_impl` 只写“校验→建根→开分页→DM” | 当前还有 `plan_boot_pages`、Step 2b aarch64 MMIO、Step 2c 设备窗口 | 补步骤或声明“设备窗口归 07 篇” |
| P1-fact-23 | §4.4 L1003-1007 | 状态表“PC 按形态、SP 在高侧”未逐形态 | v1/aarch64/riscv direct 的 PC/SP 不同；测试载体低 PC | 加“形态”列或注 |
| P1-fact-24 | §3.2 L470 | “九个变体各有宿主测试” | `InvalidSegment` 只测重叠；三新守卫无测试 | 改“部分有测试” |
| P1-fact-25 | §4.1 L719 | 删 `.lbss`/移 `KEEP` 的后果叙述与 §6.2 的 `.lrodata` 实例冲突 | `.lrodata` 是孤儿但 L5 未红 | 统一“孤儿落点不必然破坏 L5” |
| P1-fact-26 | §5.4 M1/M12 | M1/M12 标“已覆盖”，证据是手工 `readelf`/一次性复跑 | 无自动断言；M1 的 C 侧段归属无断言（R02-T01） | 标 L2/一次性；指向 R02-T01 |
| P1-fact-27 | §2.2 L214 | unpaged 对象被交回空闲链，未说明普通对象是否另有高半副本 | 同一源文件可同时有普通 `.o` 与 `unpaged_*.o`；`klib.o` 等仍有高半副本 | 补一句“两份副本/只在引导期使用” |
| P1-fact-28 | §4.4 L995 | `store_kernel_info` 的“真机读数：同一 VA 读回全零”作为因果证据 | 这是 E2 现象，不是 E1 机制；文档把它写成原因链的一环 | 标 E2/“现场记录”，补机制（页表不再映射 shim 堆） |

#### B. 结构/教学（3 + 5 条）

| ID | 位置 | 问题 | 修复 |
|---|---|---|---|
| P1-struct-01 | §3.1/§4.3/§4.4 | 三套分类（链接脚本形态/执行视图/装载路径）到 §4.4 才对齐，术语“形态”重载 | 在 §1.3 或 §3.1 前给唯一登记表；后文只引行号 |
| P1-struct-02 | §3/§4.4 | v1 handoff 是当前生产路径，但没有设计决策节，§3 完全没提 | 在 §3.1/§3.3 之间加“交接形态”决策（或并入 §3.1 取舍表） |
| P1-struct-03 | §4.1 L707-715 | §1.3 把页表多级结构判给 07 篇，§4.1 却展开 VPN[2]/三架构叶子判据，并自称“只用到检查存在” | 压成一句 + 回指 07；VPN 判据移 07 或作旁注 |
| P1-docStyle-01 | §2.2 | 只讲“对象文件名决定落段”，未讲“同一源文件两份对象”，读者会问“unpaged 副本回收后驱动还能用吗” | 补一段“两份副本/引导期专用” |
| P1-docStyle-02 | §2.3 L238 | 32 位相对位移的模加回绕未讲，读者从 −268MB 推不出 0xF0400000 | 补一句“EIP 32 位模加回绕” |
| P1-docStyle-03 | §3.4 L535/L555 | psABI 不变量与入口 RSP 结论互斥；`kmain` 裸函数只对 qemu_test 成立 | 改“自定义 naked 入口契约”或按 psABI 调整 |
| P1-docStyle-04 | §4.3 L937 | 把 §4.3 L919 已给的 delta 失效链与 `0x8020…` 读数原样再讲一遍 | 压缩为回指 |
| P1-docStyle-05 | §1.1/§2.1 | 三常量与 `0xF0000000` 在 Ch1/Ch2 各出现一次，§6.2 又复算 | Ch1 只留概念+三列图；数字下移 §2.1 |

#### C. 代码（5 条）

| ID | 位置 | 问题 | 证据 | 修复 |
|---|---|---|---|---|
| P1-code-01 | `os/kernel-image/src/main.rs` 头注释 | 仍写“`_start` parks”“生产内联 arch_boot”“riscv 安装路径未实现” | 实际 `_start` 清 BSS/立栈/调 `rust_image_main`；生产走 v1；riscv 有 U-Boot shim 腿 | 更新注释；这是文档误写的来源之一 |
| P1-code-02 | `os/boot-shim/src/main.rs` panic 注释、`uefi_helpers.rs:556` 注释 | 仍写“内核链接进 shim，main 调 arch_boot/kmain”“x86/riscv 保留进程内 arch_boot” | `main.rs:68` 调 v1；`uefi_helpers.rs:918` 调 `enter_kernel_image_v1` | 更新注释 |
| P1-code-03 | `os/kernel/src/lib.rs:544` | `arch_boot_resume_v1` 对 handoff 模块名用 `from_utf8_unchecked`，`decode_modules` 只拷贝 16 字节原始名字 | 非 ASCII 名字 → UB；注释只写“protocol ASCII” | 在 decode 边界校验 ASCII 或用 `from_utf8` 回退 |
| P1-code-04 | `os/kernel/src/arch/x86_64/higher_half.rs` + `lib.rs:1365` | 生产 `kmain` 是普通 Rust 函数，入口 RSP=`stack_top−16`（16 倍数），与 psABI 入口 `(RSP+8)` 16 倍数不一致 | `objdump` 显示 `kmain` 有 `push rbp…sub rsp`；无 `movaps`，暂无可见故障 | 评估改 `naked` 入口或调整 `stack_top`；加编译期/运行期断言 |
| P1-code-05 | `os/kernel/src/lib.rs:218/291/663` | `arch_boot` 三份 `#[cfg]` 重复 `store_kernel_info`/`register_panic_diagnostic`/jump | 设计 §3.4 的 trait 只收 `jump_to_kmain`；入口分派仍散 | 仿 `BootArch` 关联类型收拢，或注释说明为何保留 |

#### D. 测试（5 条）

| ID | 位置 | 问题 | 修复 |
|---|---|---|---|
| P1-test-01 | `loader.rs:143-175` | `filesz>memsz`、`checked_add`、`checked_sub` 三守卫无测试 | 补三个畸形 ELF 测试 |
| P1-test-02 | `os/kernel/src/lib.rs:1365` | 生产 `kmain` 入口栈对齐未测（只测 qemu_test naked） | 加“生产入口 RSP 契约”测试或在 `kmain` 入口断言 |
| P1-test-03 | `os/kernel/src/arch/*/link.ld` | 常量与生产脚本无锁步断言 | 加对照测试或删除该形态（R02-T02） |
| P1-test-04 | §5.4 M4/M7/M9 | 覆盖声明与证据错配 | 改表 + 补 TODO（R02-T01/T03/T04） |
| P1-test-05 | §5.1 L1042 | `cargo test -p boot-shim --lib` 实跑 41，文档上下文是 15 个 loader 测试 | 写清“41 总/15 loader”或加 `-- loader::` 过滤 |

#### E. 流程（1 条）

| ID | 位置 | 问题 | 修复 |
|---|---|---|---|
| P1-process-01 | `.design/02-design.v2.md` | v2 快照过期，Gate H.2/H.4 若直接用 v2 会误判 | 已重推 v3；v2 标 superseded；后续 review 先比快照与当前代码 |

### 9.3 P2（13 条，完整见附录 A）

- `load_segments_into_phys_memory` 自身不查 `filesz>memsz`，依赖 `compute_kernel_layout` 先跑（P2-code-01）。
- `SegmentIter` 的 `p_offset + p_filesz` 无 checked，畸形 ELF 可让 `seg_end` 回绕（P2-code-02）。
- `Elf64Phdr.p_align` 无读者（P2-fact-04）。
- `Elf64Ehdr` 丢弃字段列表漏 `e_version`/`e_ident`（P2-fact-02）。
- §6.2 L4 示例只给 x86 delta，aarch64/riscv 不同（P2-fact-01）。
- §4.4 状态表使用 Phase A/B、Step 1–4 未定义（P2-fact-05）。
- §5.1 `cargo test` 命令口径（P2-test-01）。
- `KERN_SIZE` 手写常量、L9 只 riscv（P2-test-02）。
- `run_qemu.sh` 未实跑（P2-test-03）。
- §1.1/§2.1 三常量重复（P2-docStyle-01）。
- §4.1 `. = ALIGN` 删除后果在 §3.5 已讲，§4.1 又复述（P2-docStyle-02）。
- “值得注意的是”类元话语（P2-docStyle-03）。

---

## 10. 卓越性审查

### 10.1 文档侧

- **教学深度（强）**：§2.1 五处 `AT()` 逐行、§2.3 两种寻址与三推演、§2.4 三证据链、§3.5 三档粒度，都是“看代码→观察→推理”的写法；每个设计决策有 C 负担与取舍表。
- **组织合理性（中）**：章级骨架好；扣分在 §4.4 三套分类晚对齐、v1 handoff 无设计决策节。
- **架构演进（中）**：`[ARCH]` 只在 §4.3 第三形态与 §4.4 设备窗口旁注出现；两段式淘汰、64KiB 栈、v1 handoff 未统一标 `[ARCH]`。
- **读者体验（中强）**：目标读者/前置/教学目标齐全；§1.3 认知减负；每条反向推演落到具体对象。扣分：§2.3 的 32 位模加未讲、§3.4 psABI 论证自相矛盾、§4.3 标题与正文错位。
- **文风**：`doc-style-lint` error=0；锚点 64/64；承载门 4/4 可判章通过；无黑话/过程痕迹。

### 10.2 代码侧

- **分层/trait（中强）**：`HigherHalf` 三实现、`FileLoader` 两实现、`PageAllocator` 函数指针，抽象边界清楚；`arch_boot` 三份 `#[cfg]` 重复是扣分项。
- **非法态封堵（中）**：`-> !`、Safety 四条、`KernelMapPolicy` 存布局；生产 `kmain` 入口对齐是自定义契约，未进类型/断言；`from_utf8_unchecked` 依赖协议假设。
- **跨架构判定（中强）**：三份 asm 逐条对照；riscv64 的 direct/resume 双路径是真实差异；文档旧版把差异压成“无条件 delta”，正是卓越性反例。
- **死代码/无读者（弱）**：`os/kernel/src/arch/*/link.ld` 无生产链接读者；`p_align` 无读者；`KERN_SIZE` 手写常量只有 L9 对账。都有文档披露，但没有代码侧锁步/删除决策。
- **测试（中）**：宿主 23/15 通过；生产物理写入、生产 `kmain` 入口对齐、arch link.ld 锁步没有测试。

### 10.3 学生型走查（闭卷）

- 7 题中 6 题能从文档得到答案；Q6（riscv64 为什么需要 delta）按文档旧版会答成“物理视图无条件加”，与当前代码不符——这是 P0-fact-01 的教学后果。
- 逐节三态：§3.1（判据缺证据）、§3.3（64KiB 容量未论证）、§3.4（psABI 前提无锚点）、§3.5（1GiB/2MiB 常量无锚点）、§4.3（medany 展开无证据）、§4.4（`arch_boot_resume_high_half` 只一句）为“只能靠结论”。
- 自列清单：§1.2 三件事、§2.4 三证据、§3.4 四条 Safety、§4.2 九变体、§5.4 M1–M12 都有展开；§5.4 M4/M7/M9 的“已覆盖”未兑现。

### 10.4 冷读团（A/B/C）

| 角色 | 独立发现 | 重叠 | 被采纳 | 代表发现 |
|---|---|---|---:|---|
| A 初学者路径 | 12 | 6 | 6 | §2.3 的 32 位模加未讲；§4.3 aarch64 literal pool 与 §2.3 相反；§4.4 三形态表把内联标成生产脚本 |
| B 结构编辑 | 12 | 7 | 7 | §1.1 抢了 §2.1 的逐行解码；§3.4 缺“执行地址=链接地址”前提；§4.1 越界 07；§4.4 表不完整 |
| C 因果链 | 14 | 8 | 8 | §4.4 riscv PC 列自相矛盾；§4.4 “两形态合流”不成立；§6.1 AT 与映射混为一谈；§3.3 “两个栈都在高侧” |

**归因**：冷读团的多数发现与主会话/子代理一致（重叠 21/38）；独立发现中 3 条升为 P1（P1-fact-05/07/19），1 条升为 P0（P0-fact-03 的 riscv PC 列）。没有“只靠票数”采纳的项；每条升格前都重放了源码或文档对照。

---

## 11. 测试对账（Gate E）

### 11.1 计数与实跑

| 对象 | 文档 | 实测 | 判定 |
|---|---|---|---|
| `minix-elf` | 23 | `cargo test -p minix-elf --lib` → 23 passed | ✅ |
| `boot-shim/loader.rs` | 15 | `grep -c '#[test]'` → 15；`cargo test -p boot-shim --lib` → 41 passed（15 loader） | ✅ 文件计数；⚠️ 命令口径 |
| `check-layout.sh` | L0–L9 | 三架构全 PASS，exit 0；L9 `0x400000 = 0x400000` | ✅ |
| `test-higher-half` | 三断言 | 静态核对 `kmain_verify`；未跑 QEMU | ⚠️ 静态 |

### 11.2 测试自身问题

- `test_bss_region_is_zeroed_after_load` 用 `0xAB`/`0xCC` + 哨兵，逐字节核对三件事；夹具与断言一致。✅
- `test_compute_kernel_layout_from_mock_elf` 的 ELF 夹具字段与期望值一致。✅
- `test_first_overlapping_pair_*` 半开区间、相邻不算、空表；与实现一致。✅
- `test_load_boot_modules_*` 模拟分配器指向泄漏 `Vec`，不写真实物理内存。✅
- 缺口：`filesz>memsz`、`checked_add`/`checked_sub`、`parse_phdr` 防御失败、生产 `kmain` 入口对齐、arch link.ld 锁步、`from_utf8_unchecked` 负例。

### 11.3 真机静态审计

- 测试载体 `KernelInfo` 的 `kern_virt_base=0xFFFF_8000_0000_0000`、`kern_size=0x200_000`、`kern_stack_top=0xFFFF_8000_0020_0000`；与 §5.2 一致。
- `run_qemu.sh` 用 `strings | grep '### TEST_RESULT: PASS'`；`SKIP` 与 `FAIL` 分开，0/1 退出码；与 §5.2 一致。
- `kmain_verify` 三断言；PC 只打印 `INFO`；与 §5.2 一致。
- 未跑 QEMU：不验证串口实际输出、不验证三架构启动；报告标注限制。

---

## 12. 对抗性审阅归因与独立验证

### 12.1 归因（五类）

> 注：五类按“处置动作”计，可与 Issue List 的条数交叉（例如 P1-struct-03 既在 ① 登记，也在 ② 转出），不是互斥分区。

| 类 | 数量 | 代表 |
|---|---:|---|
| ① 真缺陷·本轮登记 | 58（P0 11 + P1 47） | P0-fact-01..09、P0-test-missing-01、P0-design-wrong-01 |
| ② 真缺陷·转出 | 5 | 07 篇页表接口细节（P1-struct-03）、arch link.ld 删除/锁步（P1-test-03）、生产 kmain 对齐（P1-code-04，交 code-excellence） |
| ③ 假阳性 | 3 | “§2.5 归纳表错”部分内容已在 §2.3 修复（降 P2）；“§4.4 状态表 Phase 未定义”是读者推断（降 P2）；“L8 `minix_kernel9arch_boot` 拼写错”是 mangled 名匹配（不成立） |
| ④ 非缺陷·表述可优化 | 8 | §1.1/§2.1 重复、§4.3 delta 失效链重复、`cargo test` 命令口径 |
| ⑤ 证据不足 | 2 | ARM `0xF000_xxxx` 精确窗口（保留 P1，但改“无依据”而非“错误”）；riscv64 只读审计未跑 QEMU（标限制） |

### 12.2 独立验证（Gate G）

- 抽样 14/71（20%）：P0=3、P1=11；覆盖文档事实、代码注释、设计快照、测试覆盖、结构五类。
- 一致性 **14/14 = 100%**；每个抽样项都重放了 `sed`/`grep`/`objdump`/`cargo` 命令。
- 同 agent 验证局限：已在 `VERIFY-CHECK.md` 标注“同 agent + 已重放命令”；未跨 agent；未跑 QEMU。
- Gate G PASS 只表示“review 结论可复核”，不表示文档 CONVERGED。

---

## 13. Rule Discovery

- **本次是否发现新模式？** ✅
- **模式名**：**“代码注释过期 → 文档照抄注释 → 事实链整体停留在旧生产路径”**。
- **案例**：`os/kernel-image/src/main.rs:24-32` 仍写“`_start` parks”“生产内联 arch_boot”；`os/boot-shim/src/main.rs:74-76` 仍写“内核链接进 shim”；`uefi_helpers.rs:556-561` 仍写“x86/riscv 保留进程内 arch_boot”。文档 §4.3/§4.4 的 x86_64 内联模型与这些注释一致，但与 `main.rs:68`/`uefi_helpers.rs:918`/`kernel-image/main.rs:354` 的当前调用链相反。
- **判定**：P0-fact（文档）；P1-code（注释）。
- **归类**：文档/代码跨层；真值源选择。
- **规则草案**：凡文档写“生产路径/调用链”，必须以**调用点 grep**（`rg 'fn_name\('` 从入口反向查）为准，不得以文件头注释/模块 doc 为准；代码注释自身也要纳入“注释与调用链同步”检查。
- **建议落地**：`review-doc-checklist.md` §2.4 增一行“生产调用链以调用点为准”；`review-code-checklist.md` §13 增“模块头注释与当前生产调用链一致”。
- **第二模式**：**“设计快照与生产路径脱节”**。v2 的 D-1/§6.2/§4.3 与当前代码冲突，若直接拿 v2 做 Gate H 会把正确代码判成偏离。规则草案：Gate H 的 H.1 不仅要“快照存在”，还要“快照与当前代码的生产路径锚点一致”；不一致 → H.1 不算通过，先重推。
- **第三模式**：**“覆盖表用错证据层级”**。M4 用 Rust L7 证明 C 落段、M7 用 buffer 孪生证明物理写入、M9 用旧路径证明合流。规则草案：覆盖表每行必须标证据等级（E1/E2/E3）与检查对象（C/Rust/真机）；“已覆盖”不得跨对象借用。

---

## 14. 轨迹复盘（基于本轮 review 的执行轨迹）

> 详细轨迹见 `.review/zcode/01-stage-kernel/02-higher-half-kernel/trace/00-timeline.md` 与 `logs/`。本节按“覆盖遗漏 / 结论缺陷 / 假阴性探针 / 过程改进”四项检查。

### 14.0 执行轨迹（摘要）

| 阶段 | 动作 | 关键产出/发现 |
|---|---|---|
| R0 基线 | `git rev-parse`、`sha256sum`、`wc -l`、`git status`；`review-init`；机械门 | 1201 行；64/64 锚点；24/24 文件；三架构 L0–L9 PASS；style error=0 |
| R0.5 快照 | 读 v2 design/v3 outline；从 C+Rust 重推 | v2 的 D-1/§6.2/§4.3/x86 内联与代码冲突；v3 12 条决策 12/12 一致 |
| R1 C 侧 | 子代理 + 主会话重放 C 文件 | 27 对象/五处 AT()/961/USR 值/pt_init 均真；ARM 距离 0x70200000；魔数/栈回溯/ARM 名单不完整 |
| R1 Rust | 子代理 + 主会话重放 loader/minix-elf/bootface/boot entry | riscv64 条件 delta；x86 v1 handoff；builder 单份；compute 三守卫；10 参调用 |
| R1 测试/构建 | `cargo test`、`check-layout`、`cargo check`、`unsafe-audit` | 23/41 passed；三架构布局 PASS；clippy `-D warnings` 因既有告警失败（已记录） |
| R2 结构/卓越 | structure 12 节、excellence、冷读 A/B/C、闭卷学生 | 骨架 PASS；38 条对抗发现，21 条重叠；1 条升 P0、3 条升 P1 |
| R3 覆盖/复盘 | coverage-extract、SYMBOLS 填列、假阴性探针 | 160 语义域；探针命中 4/4；补 P1-fact-19、P0-test-missing-01、P2-fact-01 |
| R3 验证/报告 | `verify-check.py` 两趟、抽样 14/71、写报告 | 14/14 = 100%；11 P0/47 P1/13 P2；NOT CONVERGED |

### 14.1 覆盖遗漏检查

**逐节对照**：文档 31 个二级标题逐节核对，全部有检查记录或“无发现声明”。重点节：§1.1–§1.3、§2.1–§2.5、§3.1–§3.5、§4.1–§4.5、§5.1–§5.4、§6.1–§6.2、§7.1–§7.4。

**逐文件对照**：24 个 doc-code-map 文件全部存在；人工补 4 个（`bootface_a64.rs`、`uboot_main.rs`、`handoff_v1_read.rs`、`run_qemu.sh`）。未逐行审的文件：`os/arch/src/arch/direct_map.rs` 只在 §3.1/§4.1 的 VPN 判据用到；`os/arch/src/arch/stacktrace.rs` 只在 §2.3 的帧指针链对照；`os/qemu-tests/test-kernels/.../test-higher-half-aarch64/src/main.rs` 未逐行（与 x86_64 同形，静态抽样）。这三处不构成 P0 遗漏，但属于覆盖边界，已在限制中写明。

**逐门对照**：9 个 Gate 全部有证据块；7 个 Sync 项全部执行。无遗漏。

**逐模式族对照**：`review-patterns.md` §0（5 项）与文档族 1–15、代码族 16–29、测试族 35–40、叙事族 48–78、架构族 79–85 中，本轮适用项均已覆盖；未适用项（如并发族中与 boot 无关的 BKL 细节）在代码维度按 §4 抽样。

**补审**：写报告前重读 `os/kernel-image/src/main.rs` 的 `rust_image_main` 三分支，发现 `bootface_a64.rs` 的 `arena_pa==0` 分支也走 `arch_boot_resume_v1`（aarch64 direct `-kernel` 路径），修正了“aarch64 生产只有 v1”的过度概括；该细节已写入 §4.4 的路径表，不新增 P0。

### 14.2 结论缺陷检查

- 抽样 14 条 issue 重放证据：14 条一致（100%）；其中 P1-fact-05（aarch64 literal pool）从“事实错误”改判为“表述矛盾”，因为反汇编显示 `ldr` 的 literal 读取确实是 PC 相对，但加载值本身是链接期绝对值；这是“结论对、解释各说一半”，不是“前提反了”。
- 检查“L3 推断写成 L1”：P0-fact-01..09 都有源码直读或命令；P0-test-missing-01 有 grep/源码；P0-design-wrong-01 有 v2 文本 + 当前代码。未发现把推断当事实的 P0。
- 检查“引用过期快照”：Gate H 用 v3/v4 新快照；正文问题均以 C/Rust 现势为准，未用 v2 自证。
- 检查“用文档 §3 自证”：设计快照从 C+Rust 推导，未把文档 §3 当来源。
- 检查“未复现的 ✅”：Gate D/E/H.6 明确 FAIL；M4/M7/M9 明确 P0-test-missing；没有用“✅”掩盖。

### 14.3 假阴性探针

从“未被任何 issue 命中”的节中抽 3 节补审：

1. **§2.5 本章归纳**：重读 §2.3 的 ARM 段与 §2.5 表，发现“跳转携带相对位移”被写成通用结论（P1-fact-19）。探针命中，说明首轮以 P0 为主时漏了章末归纳。
2. **§5.3 布局契约分工**：重读 `check-layout.sh` L7/L9 与 §5.4 M4，发现 L7 对象是 Rust 引导栈，不是 C `k_initial_stktop`（P0-test-missing-01 的一部分）。探针命中。
3. **§6.2 命令组**：重读 `readelf` 输出与 L4 预期，发现示例只给 x86 的 `0xffff800000000000`，aarch64/riscv 的 delta 不同（P2-fact-01）。探针命中，降 P2。
4. **冷读团/学生走查**：A/B/C 三角色共 38 条发现，21 条与主会话重叠；独立发现中 1 条升 P0、3 条升 P1。说明首轮覆盖不完美，但对抗性装置接住了。

> 结论：假阴性探针命中 4/4（§2.5、§5.3、§6.2、冷读团），首轮覆盖不完美；命中项已补入 Issue List。

### 14.4 过程缺陷与改进

- **子代理证据复验**：3 个只读子代理（C 侧/Rust loader/Rust boot entry）+ 4 个冷读/学生代理；主会话对全部 P0 与抽样 P1 重放了命令。发现 1 处代理结论过强（P1-fact-05 的“事实反转”），已改判“表述矛盾”。
- **未跑 QEMU**：真机标记只做静态审计；§5.2 的 PASS 不是本轮实测。报告中已如实标注。
- **未修**：本轮只审不修，P0 全部 open；NOT CONVERGED 是准确状态。
- **工具缺口**：`coverage-extract.py` 的语义域按“文档提及”取符号，测不出“整段未提的 C 符号”；`verify-check.py` 的 issue 正则要求第二列是级别，P1 表最初没有级别列，导致只抽到 P0；已在 scan 加 P1/P2 索引表修正。`doc-style-lint --diff` 的参数是 git revision，不是文件路径；本轮用全量模式。建议在 `review-process.md` 记录这两个工具限制。
- **流程改进**：① 生产调用链以调用点 grep 为准（Rule Discovery 第一模式）；② Gate H.1 加快照-代码生产路径一致性；③ 覆盖表每行标证据等级与检查对象；④ `verify-check.py` 的 issue 正则与 P1 表格式对齐，或在规范里固定“第二列必须是级别”。

### 14.5 本轮 review 自身的缺陷（自我批评）

- **检查分配偏斜**：首轮三个子代理分别覆盖 C 侧、loader/parser、boot entry/asm；§6 诊断章没有专门的验证者。P0-fact-08（`AT()` 被归成映射缺失）与 P0-fact-09（panic 消息点名三量）是冷读团 C 发现的，不在首轮 C/Rust 子代理的检查范围内。这说明“分块审查”会漏掉跨章节的诊断表；下一轮应给 §6 单独分配一个验证者，或主会话逐条重放诊断表每一行的“怀疑对象 → 确认命令”。
- **计数有重叠**：P1=47 含 3 组同源重叠（P1-fact-07/25 都是孤儿节、P1-fact-03/18 都是“两个栈都在高侧”的后果、P1-fact-19/P1-docStyle-05 都是章末归纳/重复），独立项约 44；P2 也有 2 条与 P1 同源。下一轮应先归并再计 `weighted_new`，避免用重叠项抬高“发现量”。
- **首轮代理结论过强 1 处**：P1-fact-05 最初被判“事实反转”，反汇编后改为“表述矛盾”；说明“代理说事实错”不能直接升 P0/P1，必须主会话重放。
- **未验证边界**：`os/arch/src/arch/direct_map.rs`、`os/arch/src/arch/stacktrace.rs`、aarch64/riscv 的 `test-higher-half` 载体未逐行；QEMU 未跑；`cargo clippy -D warnings` 未过。它们不改变 P0 结论，但属于“未验证”，不是“已验证通过”。

### 14.6 复盘结论

- **遗漏**：§2.5 归纳表的通用化、§5.3 的 L7 对象错配、§6.2 的 L4 示例单架构，由假阴性探针补出；未发现新的 P0 级遗漏。
- **缺陷**：首轮把“代理报告”当“已验证证据”的风险已通过主会话重放控制；1 处代理结论过强已改判；1 处“aarch64 生产只有 v1”的过度概括已修正。
- **可改进**：生产调用链注释同步、设计快照新鲜度、覆盖表证据分级、`verify-check` 格式对齐；这四项写入 §13 Rule Discovery，交下一轮规则演化。
- **本轮 review 自身是否达到可交付标准**：是。主报告有 11 个 P0 的 L1/L2 证据、47 个 P1 的可复核清单、13 个 P2 的分组；Gate G 14/14；轨迹与复盘齐全；文档状态 NOT CONVERGED 如实。

---

## 15. 确认清单与限制

### 15.1 Step 7 自检

- [x] 范围声明/执行模式/时间预算已输出。
- [x] STATE.md 已读取；02 结果单独记录，不覆盖 01。
- [x] 源码文件清单已输出（§0.1、scan Step 1）。
- [x] 覆盖率穷举已输出（SYMBOLS + `--check-filled` exit 0）。
- [x] Gate 0 九锚段齐全。
- [x] Gate A 证据块齐全。
- [x] Top 5 行为契约表（8 字段 × 5 函数）已输出。
- [x] Gate B/C/D/D-6/E/H 证据块齐全（D/E/H.6 文档级 FAIL）。
- [x] 链路验证、概念准确性、C 引用、数据结构、C 覆盖、文档风格表已输出。
- [x] Gate E 测试名 grep + cargo test 实跑。
- [x] Gate H H.1–H.6 已核（H.6 FAIL）。
- [x] 维度覆盖自检表已输出（§2）。
- [x] 最弱项自检：C 覆盖、链路、跨文档、错误路径、Design 对齐均已回答。
- [x] Layer 1/2 判定已输出（§0.3）。
- [x] Skill Invocation Log 已输出（scan）。
- [x] scan.md 单文件 + 02 状态；Open P0/P1/P2 已同步。
- [x] Artifact Inventory 已输出（scan）。
- [x] Gate G VERIFY-CHECK 已生成（14/14 = 100%）。
- [x] Rule Discovery 已填写（§13）。
- [x] 修改项已生成（§9；本轮只审不修）。

### 15.2 限制

1. **同 agent 验证**：Gate G 由同一 session 的同一模型完成；已重放 14 条抽样的 grep/sed/objdump/cargo 命令，并在 VERIFY-CHECK 标注局限。consistency 100%。
2. **未跑 QEMU**：真机标记只做静态审计；`test-higher-half` 的三条断言未实跑。
3. **未修**：11 个 P0、47 个 P1、13 个 P2 全部 open；本文档不能标 CONVERGED。
4. **代码维度限制**：`cargo clippy -D warnings` 因既有 75 条告警失败（非本轮改动引入）；代码结论以 `cargo check`、`unsafe-audit` 与源码直读为准。
5. **代理覆盖边界**：`direct_map.rs`、`stacktrace.rs`、aarch64/riscv 测试载体未逐行；不构成 P0 遗漏，但属覆盖边界。

### 15.3 修复优先级建议（若后续进入修复轮）

1. **先修 P0-fact-01/02/03/04/05**：riscv64 asm、x86 生产路径、两形态表、`build_kernel_info` 实参、builder 位置——这五条决定读者对整条启动链的地图。
2. **再修 P0-fact-06/07/08/09**：`compute_kernel_layout`、合流点、`AT()` 诊断、panic 消息。
3. **补 P0-test-missing-01**：M4/M7/M9 覆盖表；R02-T01/T03/T04。
4. **重推/替换 P0-design-wrong-01**：v3 已生成；v2 标 superseded；Gate H 用 v3。
5. **P1 事实类按章节批量修**：§2（ARM 距离/魔数/栈回溯/ARM 名单）、§3（表/栈/psABI/三量）、§4（asl 节选/静态 mut/装载者/Step）、§5（M6/命令）、§6（AT 诊断）。
6. **P1 代码类**：更新 `os/kernel-image/src/main.rs`/`os/boot-shim/src/main.rs`/`uefi_helpers.rs` 的过期注释；补 `from_utf8_unchecked` 校验；评估生产 `kmain` 入口对齐。
7. **P2 与卓越性**：按 `excellence.md` 的 a–h 逐项评估，不一次性全做。

---

## 16. 附录

### 附录 A：P2 完整清单（13 条）

| ID | 位置 | 问题 | 建议 |
|---|---|---|---|
| P2-fact-01 | §6.2 L1138 | L4 示例只给 x86 的 `0xffff800000000000`；aarch64 是 `0xffff7fffbfe00000`、riscv64 是 `0xffffffbf7fe00000` | 加“按架构”注 |
| P2-fact-02 | §3.2 L431 | `Elf64Ehdr` 丢弃字段列了 `e_flags/e_shoff/e_shentsize/e_shnum/e_shstrndx`，漏 `e_version`/`e_ident` | 补全或写“等” |
| P2-fact-03 | §4.2 L822 | `load_segments_into_buffer` 用 `paddr.checked_sub(buf_base).unwrap_or(0)`，低于基址的段静默落到 0 | 改为测试夹具断言或返回错误 |
| P2-fact-04 | §3.2 L431 | `p_align` 无读者（文档已说） | 删除字段或加“保留给未来对齐检查” |
| P2-fact-05 | §4.4 L1003 | 状态表用 Phase A/B、Step 1–4，正文未定义 | 加一句回指 03/01 |
| P2-code-01 | `loader.rs:225` | `load_segments_into_phys_memory` 自身不查 `filesz>memsz`，依赖 `compute_kernel_layout` 先跑 | 在 Safety 契约写明“调用前必须已过 compute” |
| P2-code-02 | `minix-elf:372` | `p_offset + p_filesz` 无 checked，畸形 ELF 可回绕 | 用 `checked_add` 并 `continue`/报错 |
| P2-test-01 | `lib.rs:544` | `from_utf8_unchecked` 无负例 | 补非 ASCII 名字测试 |
| P2-test-02 | `§3.5` | `KERN_SIZE` 手写常量只有 L9；其他架构无锁步 | 加生成/断言 |
| P2-test-03 | `§5.2` | `run_qemu.sh` 未实跑 | 有条件时补三架构回归 |
| P2-docStyle-01 | §1.1/§2.1 | 三常量重复 | Ch1 只留概念 |
| P2-docStyle-02 | §4.1 L719 | `ALIGN` 删除后果在 §3.5 已讲 | 压缩为回指 |
| P2-docStyle-03 | 多处 | “值得注意的是”类元话语 | 逐条按“删掉后读者仍能发现”判 |

### 附录 B：设计快照 v2 → v3 差异（摘要）

| # | v2 | v3 | 依据 |
|---|---|---|---|
| 1 | D-1 “不保留 AT()” | D-1 生产保留 `AT()`；有无读者决定形态 | `os/kernel-image/*.ld` 各 4 条 `: AT(` |
| 2 | §6.2 “link.ld 未设 ENTRY” | 生产镜像 `ENTRY(_start)` | `build.rs`、三份 `.ld` |
| 3 | §4.3 “无条件 delta” | D-9 条件 delta，direct/resume 两腿 | `riscv64/higher_half.rs` |
| 4 | x86 进程内 `arch_boot` | D-10 v1 send/receive；内联只测试载体 | `main.rs:68`、`uefi_helpers.rs:918`、`kernel-image/main.rs:354` |
| 5 | builder 两份 | D-12 单份 `lib.rs:48` | grep |
| 6 | `FileLoader`/`KernelLoadResult` 在 loader.rs | 保留；builder 移到 `lib.rs` | 源码 |
| 7 | aarch64 `bl` ±128MB | AArch64 `bl` ±128MB；C ARM32 `bl` ±32MB；分开写 | ARM ARM / doc §2.3 |
| 8 | `arch_boot` 4 份 cfg | 保留为已知限制；`arch_boot_resume_*` 另有两入口 | `lib.rs` |
| 9 | aarch64/riscv64 未压 NULL 终止符 | 设计差异仍在；x86_64 `push 0` 另有对齐作用；FP=0 是链终止点 | 三份 asm |
| 10 | Q2 “未压 NULL 不是 bug” | 保留结论；补“生产 kmain 非 naked”的入口对齐风险 | `objdump` |

### 附录 C：关键命令与输出（摘录）

```text
$ grep -n 'auipc t3\|bltu\|jalr\|in("t4")' os/kernel/src/arch/riscv64/higher_half.rs
105: "auipc t3, 0"
109: "bltu t3, t4, 3f"
110: "jalr x0, t1, 0"
113: "jalr x0, t1, 0"
116: in("t4") kvirt

$ sed -n '68p' os/boot-shim/src/main.rs
    boot_shim::uefi_helpers::prepare_and_enter_kernel_image(1024)

$ sed -n '918p' os/boot-shim/src/uefi_helpers.rs
    minix_kernel::enter_kernel_image_v1(arena_pa.0, &kernel_info, root_page)

$ sed -n '354p' os/kernel-image/src/main.rs
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)

$ sed -n '48,58p' os/boot-shim/src/lib.rs
pub fn build_kernel_info(
    memmap: &'static [minix_boot::MemoryRegion],
    reserved_regions: &'static [minix_boot::MemoryRegion],
    ...
    param_buf: &'static [u8],
) -> minix_boot::KernelInfo

$ sed -n '143,175p' os/boot-shim/src/loader.rs
    if seg.filesz > seg.memsz { return Err(minix_elf::ElfError::InvalidSegment); }
    let seg_end = seg.paddr.checked_add(seg.memsz)...;
    let kern_size = kern_end_phys.checked_sub(kern_phys_base)...;

$ printf '0x%x\n' $((0xF0400000-0x80200000))
0x70200000

$ SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all | tail -3
  [L9] PASS  跨距对账：ELF 实测 0x400000 = bootface.rs KERN_SIZE 0x400000
CHECK-LAYOUT=PASS（all）

$ (cd os && cargo test -p minix-elf --lib) | tail -1
test result: ok. 23 passed; 0 failed; 0 ignored; ...
$ (cd os && cargo test -p boot-shim --lib) | tail -1
test result: ok. 41 passed; 0 failed; 0 ignored; ...
```

### 附录 D：产物清单

| 产物 | 路径 |
|---|---|
| 本报告 | `rewrite-notes/01-stage-kernel/02-review.md` |
| scan | `.review/zcode/01-stage-kernel/02-higher-half-kernel/scan.md` |
| structure | `.review/zcode/01-stage-kernel/02-higher-half-kernel/structure.md` |
| SYMBOLS | `.review/zcode/01-stage-kernel/02-higher-half-kernel/SYMBOLS.md` |
| excellence | `.review/zcode/01-stage-kernel/02-higher-half-kernel/excellence.md` |
| test-audit | `.review/zcode/01-stage-kernel/02-higher-half-kernel/test-audit.md` |
| VERIFY-CHECK | `.review/zcode/01-stage-kernel/02-higher-half-kernel/VERIFY-CHECK.md` |
| trace | `.review/zcode/01-stage-kernel/02-higher-half-kernel/trace/00-timeline.md` + `logs/` |
| design v4 | `.design/02-outline.v4.md`、`02-outline-review.v4.md` |
| design v3 | `.design/02-design.v3.md` |
| 状态 | `.review/zcode/01-stage-kernel/STATE.md`（只追加 02 行） |

---

## 17. R2 修复与深度回归（2026-11-02）

> 本节是对 §9 Issue List 的修复结果与修复后的第二次 full review。原 §9 保留 R1 历史判定，本节记录 R2 状态；两节合起来读才是完整轨迹。

### 17.1 P0 修复台账（11/11）

| ID | 修复 | 证据 |
|---|---|---|
| P0-fact-01 | §4.3 按当前 `riscv64/higher_half.rs` 重写 `auipc/bltu` 条件 delta + 双 `jalr`；§4.4 表改为 direct/resume 两腿 | doc 916–946；`riscv64/higher_half.rs:99-120` |
| P0-fact-02 | §4.3/§4.4 把 x86_64 生产改为 v1 send/receive；内联只留测试载体 | doc 362/1003；`boot-shim/src/main.rs:68`、`uefi_helpers.rs:919`、`kernel-image/src/main.rs:376` |
| P0-fact-03 | §3.1/§4.4 两表补 v1、riscv64 自引导/测试载体、aarch64 direct-core、x86/a64 测试载体四行；riscv PC 改高半 VMA | doc 366–372、1021–1028 |
| P0-fact-04 | §4.4 `build_kernel_info` 补 `reserved`/`param_buf`，10 参与源码一致 | doc 1020–1031；`bootface.rs:310-321`、`lib.rs:48-59` |
| P0-fact-05 | §3.3 builder 改为单份 `os/boot-shim/src/lib.rs:48`；删除“各一份/opensbi 复用” | doc 506；`grep -rn 'fn build_kernel_info' os` 仅一处 |
| P0-fact-06 | §4.2 `compute_kernel_layout` 补 `filesz>memsz`、`checked_add`、`checked_sub`；§3.2 `InvalidSegment` 改四原因 | doc 779–816、485；`loader.rs:135-175` |
| P0-fact-07 | §4.4 “合流”改为 v1 走 `resume_after_v1_decode`、direct 走 `arch_boot_impl` | doc 1051；`lib.rs:591-655/793-830` |
| P0-fact-08 | §6.1 拆“高地址映射缺失”与“`AT()` 常量→内容错位”两行 | doc 1161–1162 |
| P0-fact-09 | §3.5/§6.1 改为消息只报 `no kernel page size satisfies the layout`，三量自算 | doc 595/603/629/1164；`boot_policy.rs:51` |
| P0-test-missing-01 | §5.4 M4/M7/M9 改“部分覆盖”；补 R02-T04；M1 改部分 | doc 1145/1148/1150/1146 |
| P0-design-wrong-01 | 重推 `02-design.v3.md`；v2 头标 SUPERSEDED 并列出四处冲突 | `.design/02-design.v3.md`、`.design/02-design.v2.md:3-5` |

### 17.2 P1/P2 修复统计

- **P1 47/47 已落地**：事实/表述 28、结构 3、文风/教学 5、代码 5、测试 5、流程 1。`P1-test-02` 的代码契约已修（x86 `sub rsp,8`、`sp_aligned` 架构相关），生产入口断言仍由 qemu_test 载体覆盖，登记为 R02-T04，不再作为文档/代码 P1。
- **P2 13 条**：9 条已修（L4 示例、Elf 字段、`unwrap_or`、Phase 指针、Safety 注释、`SegmentIter` checked、from_utf8 校验、Ch1 重复、ALIGN 复述）；4 条为接受/backlog：`p_align` 无读者、`KERN_SIZE` 仅 L9、QEMU 未跑、`from_utf8` 负例未单列。
- **代码注释同步**：`kernel-image/src/main.rs` 头注释、`boot-shim/src/main.rs` panic 注释、`uefi_helpers.rs` v1 注释、`minix-boot/src/boot_form.rs` 的 `X86UefiInline` 说明均已更新。
- **代码修**：`arch_boot` 三份 cfg 抽成 `arch_boot_finish` 泛型 helper；`arch_boot_resume_v1` 的模块名改 `from_utf8` 校验；`load_segments_into_buffer` 去掉 `unwrap_or(0)` 静默降级；`SegmentIter` 的 offset/filesz 加法改 checked。
- **测试新增**：`loader.rs` 三个守卫测试；`os/kernel/tests/link_ld_lockstep.rs`；`store_before_alloc_pin.rs` 跟进 helper。

### 17.3 回归门禁

| 门/检查 | 结果 |
|---|---|
| 文档锚点 | `anchors=59 resolved=59 zero-def=0 multi-def=0` |
| 内部引用 | `doc-ref-check --check` 全通过 |
| 承载门 | `doc-code-density --check` 通过（4/4 可判章） |
| 文风 | `doc-style-lint` error=0 |
| 关联代码 | `doc-code-map --check` 26/26 |
| 覆盖率 | 20/160 文档、6/160 Rust；`--check-filled` exit 0 |
| 解析器测试 | `cargo test -p minix-elf --lib` → 23 passed |
| boot-shim 测试 | `cargo test -p boot-shim --lib` → 44 passed（18 个 loader） |
| minix-boot 测试 | `cargo test -p minix-boot --lib` → 65 passed |
| 内核宿主测试 | `cargo test -p minix-kernel --features mock` → 各 target 全绿，0 failed（lib 856 passed） |
| 锁步测试 | `cargo test -p minix-kernel --features mock --test link_ld_lockstep` → 1 passed |
| 三架构布局 | `bash os/kernel-image/check-layout.sh all` → `CHECK-LAYOUT=PASS（all）` |
| Gate 0/A/B/C/D/D-6/E/G/H | R2 全 PASS；Gate G 见 `VERIFY-CHECK-R2.md`（14/14 = 100%） |

### 17.4 回归新发现与处置

第二次 full review 由独立回归验证子代理 + 主会话重放完成，发现并修掉 18 处“修复引入/未同步”问题，主要有：

1. §3.2 迭代器节选仍是无 checked 版本 → 已按 `minix-elf/src/lib.rs:357-387` 重写，改为“防御性表项计算/解析、类型过滤、越界”三类语义。
2. §3.1 说“没有锁步断言”、M5 说“锁步测试”、R02-T02 又登记 TODO → 已统一：§3.1 写锁步测试存在；R02-T02 关闭；缺口降为 T01/T03/T04。
3. §3.5 “三个基址都不是 1GiB 对齐” → 改为“三个物理基址不对齐；aarch64/riscv 虚拟基址对齐，但 4MiB `kern_size` 仍不满足 1GiB”。
4. §3.3 把 `kern_stack_top` 叫“链接符号” → 改为“`kernel_boot_stack_*` 是链接符号；`kern_stack_top` 是 `KernelInfo` 字段”。
5. §4.4 v1 路径顺序“建根→写 arena” → 改为“写 arena → EBS → `enter_kernel_image_v1` 建根/补字段”。
6. §4.4 “不经任何寄存器交接” → 改为“地址不依赖 shim 栈引用；指针仍按 ABI 放在 RDI/X0/A0”。
7. §4.4 测试/直核路径只写 UEFI helpers → 补 riscv64 载体直接 `arch_boot`、aarch64 direct-core `bootface_a64`。
8. §4.5 “分页前代码不在内核镜像里” → 改为“主体在 boot-shim/入口汇编；没有 C 那种由同一装载器放到低物理地址的 unpaged 段”。
9. §4.3 aarch64 literal pool “必须在链接地址可读” → 改为“literal 与代码同镜像摆放；direct-core 低地址读低 literal，值仍是高 VMA”。
10. §6.2 FAIL 字符串 `SP not 16-byte aligned` → 改为源码实际字符串 `SP not at the architecture's entry alignment`。
11. §4.3 delta 操作数 `0xffffffbf7ffe0000` → 改为 `0xffffffbf7fe00000`。
12. §5.4 M1 把 C `kernel.lds` 标“L4 机器覆盖” → 改为“Rust 生产脚本 L4；C 侧静态阅读，无机器断言”。
13. §4.4 状态表分页证据只写 `arch_boot_impl` → 改为“v1: shim 建根 + 接收侧 DM；direct: `arch_boot_impl` Step 1–4 + 2b/2c”。
14. §5.2 `RSP ≡ 0` 用于 aarch64/riscv → 改为 `SP ≡ 0`。
15. §3.1 riscv64 测试载体误标“生产镜像（AT）” → 拆成“riscv64 自引导（生产镜像）”与“riscv64 测试载体（载体单地址）”两行。
16. `kmain_verify` 的 `sp_aligned` 初版统一改成 8 mod 16，会误伤 aarch64/riscv → 改为按架构：x86_64 8 mod 16，aarch64/riscv64 0 mod 16；源码与 doc 同步。
17. `os/libs/minix-boot/src/boot_form.rs` 的 `X86UefiInline` 说明仍像生产路径 → 补模块注记：它是后置条件 battery 的历史/测试形态，不是生产路径 map。
18. `os/kernel-image/src/main.rs` 头注释把 v1 顺序写成“建根→写 arena” → 改为“写 arena → EBS → `enter_kernel_image_v1` 建根/补字段”。

### 17.5 最终判定

| 项 | 结果 |
|---|---|
| P0 | **0 open**（11/11 fixed） |
| P1 | **0 open**（47/47 落地；P1-test-02 的测试残项转 R02-T04） |
| P2 | 9 fixed，4 accepted/backlog |
| Layer 1 | **PASS**（Gate 0/A/B/C/D/D-6/E/G/H 全 PASS） |
| Layer 2 | **PARTIAL**（主体 A-/B+；残余 P2、R02-T01/T03/T04、QEMU 未跑） |
| 文档状态 | **CONVERGED_WITH_WARNING_BACKLOG** |
| 不得声称 | “QEMU 三架构已实测通过”“生产入口 psABI 已有自动断言”“C `kernel.lds` 有机器断言” |

### 17.6 残余与限制

1. **R02-T01**：C `k_initial_stktop` 段归属无自动断言；L7 检查的是 Rust 引导栈。
2. **R02-T03**：riscv64 载体的 PC 高半判据未加；`delta` 路径仍只有静态/源码证据。
3. **R02-T04**：生产 `kmain` 入口的 ABI 栈形断言仍只在 qemu_test 载体；生产入口没有独立断言。
4. **P2-test-02**：`KERN_SIZE` 手写常量只有 L9（riscv64）对账。
5. **P2-test-01**：`from_utf8` 校验已加，但非 ASCII 名字的负例测试未单列。
6. **QEMU 未跑**：三架构真机回归仍属静态审计；`check-layout` 是宿主布局断言，不等于启动实测。
7. **clippy**：`cargo clippy -D warnings` 仍因既有 75 条告警失败（非本轮改动）；代码结论以 `cargo check`、`unsafe-audit`、源码直读与测试为准。
8. **同 agent 验证**：Gate G 为同 agent + 命令重放；未跨 agent。

### 17.7 本轮修改文件

- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 代码：`os/kernel/src/lib.rs`、`os/kernel/src/arch/x86_64/higher_half.rs`、`os/kernel-image/src/main.rs`、`os/boot-shim/src/main.rs`、`os/boot-shim/src/uefi_helpers.rs`、`os/boot-shim/src/loader.rs`、`os/libs/minix-elf/src/lib.rs`、`os/libs/minix-boot/src/boot_form.rs`
- 测试：`os/kernel/tests/store_before_alloc_pin.rs`、新增 `os/kernel/tests/link_ld_lockstep.rs`
- 设计快照：`.design/02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md`；`.design/02-design.v2.md` 标 SUPERSEDED
- review 制品：`02-review.md`、`.review/zcode/01-stage-kernel/02-higher-half-kernel/{scan,structure,SYMBOLS,excellence,test-audit,VERIFY-CHECK,VERIFY-CHECK-R2,trace}`
