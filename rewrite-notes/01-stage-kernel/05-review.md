# 05 · 时钟与中断初始化 — 深度 Review

- **对象**：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`（R1 2105 行 / 169661 字节；R2 2196 行 / 177098 字节）
- **绑定哈希**：sha256 `8cacc3c86db660b06a3986c1bc15f241c992d3e77e0cda6245c571aae96de7fe`
- **基线**：git HEAD `2320f7959a63d80a54dc80c932248856eea03c7e`；工作树仅 `M AI-chats/daily.todo.md`（与本轮无关）
- **模式**：文档 + 关联 Rust 代码，深度完整 review；R1 只审不修，R2 修复后回归
- **R1 结论**：**NOT CONVERGED**（11 个 open P0：7 个 P0-fact、2 个 P0-code-bug、2 个 P0-test-missing；13 个 P1，8 个 P2）。
- **R2 最终状态**：**CONVERGED_WITH_WARNING_BACKLOG**（P0=0、P1=0、P2=0 open；R05-T01/T05/T06 backlog；详见 §17）。R1 的发现保留在 §0–§16，作为修复前台账。

> 本文是结果报告；逐条命令与原始输出在 `.review/zcode/01-stage-kernel/05-clock-interrupt-init/trace/`，阶段时间线在 `05-review-trajectory.md`。

---

## 0. 结论先行

这篇文档的主干是对的：它把 `cstart` 的三行调用、8259A 的十次写、`arch_init` 的六件事、`boot_cpu_init_timer` 的三段，以及 Rust 的 `ClockArch`/`InterruptRouter`/`PerCpuInterruptUnit`/`TimerIrqGate` 四条抽象线讲成了一条可复述的因果链。问题不在“讲得通不通”，而在“讲的当前代码是不是这一份”：

1. **两个真实的代码缺陷**：
   - `pic_init` 从片 ICW2 写 `IRQ0_VECTOR + 8` = **0x58**，而 IDT 门、trap stub 与函数自己的注释都按从片 **0x70–0x77**。IRQ 8–15 一旦放行，CPU 会收到没有门的 `0x58–0x5F`。
   - `IrqManager::dispatch_claimed` 在“没有 handler”时先 mask 再返回，不执行 `complete(claimed)`。aarch64 的 `ICC_IAR1_EL1` 已经 claim 了 `Some(id)` 时，`ICC_EOIR1_EL1` 永远不会写回；SGI 被特判，其他 INTID 没有保护。
2. **七个文档事实缺口**：`ClockArch` 11/12 方法；aarch64 `ArchInit` 2/3 动作且漏 GICD base 播种；riscv `claim` 引文把 `claim_context` 的函数体画进了 `claim`；riscv 载体“只有 PASS”实际有 FAIL 分支；§4.7 从片 0x70 与代码 0x58 冲突未披露；§4.1 的“五步/省略号只省注释”藏掉了 PD-22 注入与 aarch64 SGI；6 处“按原文”引文与当前源码不一致。
3. **两个测试缺口**：PIC 从片向量没有一致性测试；aarch64 空链 claim 的 complete 没有测试。另有 §5.1 漏列 12 条 host-runnable pin 测试。

因此 R1 不能给 CONVERGED，也不能把“v4 大纲已生成”当成正文已对齐。R2 已按“先代码、再文档事实、再引文与测试”的顺序修复，最终状态见 §17。

---

## 1. 执行摘要

### 1.1 范围与方法

| 项 | 内容 |
|---|---|
| 文档 | 05 全文七章、2105 行；逐节读，不以旧 review/旧 defect ledger 代替取证 |
| 关联代码 | `doc-code-map` 28 文件全部存在；另补 4 个 host-runnable pin 测试文件 |
| C ground truth | `main.c`/`clock.c`/`interrupt.c`/`i8259.c`/`arch_clock.c`/`arch_system.c`/`apic.c`/`earm/*` |
| 机械门 | 锚点 108/108；doc-code-map 28/28；doc-ref PASS；density PASS；style error=0 |
| 测试 | kernel mock 856 passed；arch lib 289；plat 7；pin 3/4/3；0 failed |
| 对抗性装置 | 学生型冷读、结构编辑冷读、独立 CodeReview 子代理；结论经主会话重放 |
| 未做 | 未跑 QEMU；未修改文档/代码；未把历史 `.review`/`tmp/doc-iter` 结论直接采纳 |

### 1.2 计数

| 级别 | 数量 | 说明 |
|---|---|---|
| P0-fact | 7 | 事实/引文/覆盖错误 |
| P0-code-bug | 2 | PIC 从片向量；空链 claim 未回执 |
| P0-test-missing | 2 | 两个关键行为无测试 |
| P1 | 13 | 方法集、`use` 计数、测试清单、注释债务、结构重复 |
| P2 | 8（+1 closed） | mock 兜底、口径、死块、旧快照 |

### 1.3 最关键八组发现

1. `pic_init` 从片 ICW2 = 0x58，IDT 只装 0x70–0x77（P0-code-bug-01 / P0-fact-05）。
2. aarch64 空链 claim 不回执（P0-code-bug-02 / P0-test-missing-02）。
3. aarch64 `ArchInit::init()` 有三动作，正文写两动作并漏 GICD base 播种（P0-fact-02）。
4. `ClockArch` 12 方法、`PerCpuInterruptUnit` 5 方法，正文写 11/2（P0-fact-01 / P1-fact-01）。
5. riscv `claim` 引文错位，隐藏 per-hart `claim_context`（P0-fact-03）。
6. riscv 载体有显式 FAIL 分支，正文说“只有 PASS”（P0-fact-04）。
7. §4.1 的“五步”省略号藏掉 PD-22 注入与 aarch64 SGI，“每条线都屏蔽”在 aarch64 不成立（P0-fact-06）。
8. 6 处“按原文”引文与当前源码不一致（P0-fact-07）。

---

## 2. 维度覆盖自检与规则引用

### 2.1 维度覆盖

| 维度 | 执行 | 证据 |
|---|---|---|
| 概念准确性 | ✅ | C ground truth 表；§2.3 十次写复算；§2.2 `init_clock` 纯软件 |
| C 代码引用 | ✅ | `trace/logs/07-c-ground.log`、`11-c-seq.log`、`19-c-pic.log` |
| 数据结构/方法集 | ✅ | `02-traits.log`（12/6/5）；`SYMBOLS.md` |
| 源码覆盖完整性 | ✅ | `coverage-extract` 99/152 语义域；`--check-filled` exit 0 |
| 设计决策质量 | ✅ | 设计 v3 §1 每条决策有取舍/否决理由；Gate H |
| 章节链路 | ✅ | `structure.md` §12；Ch3→Ch4 全节回指 |
| 代码维度 | ✅ | `gate-evidence-code`；执行模型/硬件抽象/非法态/死代码 |
| 跨文档 | ✅ | 03/04/08/14/15/16 抽查；`doc-ref` PASS |
| 测试五维 | ✅ | `test-audit.md`；Gate E |
| 卓越性 | ✅ | `excellence.md`；冷读团 + 独立 CodeReview |
| 对抗性归因 | ✅ | §12；子代理结论逐条重放 |
| 轨迹复盘 | ✅ | §14；负抽样 3 节 |

### 2.2 规则引用清单

- `review.md`：真值优先序、P0 六分类、Layer 1/2、§15 Precision Check。
- `review-process.md`：Step 0–7、Gate 0/A/B/C/D/D-6/E/G/H、Step 0.3、Step 3.5a/3.5b、Step 3.6、Step 5.6/5.7、Step 7.1。
- `review-doc-checklist.md`：§2.1 概念、§2.2 C 引用、§2.4 文档-代码一致、§2.8 覆盖、§2.9 设计决策、§2.10 章节链路、§4 Doc Code Sync 七项。
- `review-code-checklist.md`：§1/§2/§4/§8/§12/§13/§14/§15。
- `review-core-semantics.md`：行为契约、§1.6 抽象语义与边界。
- `review-patterns.md`：§0 P0 必检、模式 5/73（文档漂移）、模式 83（锚点）、模式 85（跨架构）。
- `review-doc-excellence.md` §4.1–4.4；`review-code-excellence.md` §16–21（§16.6 非法态、§16.7 跨架构）。
- `review-cmds.md` §一b 与通用强制门 #1–#9；`fix-guard.md`（只审不修）。
- `DOC-ITER-WORKFLOW.md` S1.0/S1.2/S1.4/S1.6/S1.7、S2.0、V2/V3/V5、§八/§九。

---

## 3. Blocker Gates 证据（R1）

| Gate | 判定 | 证据 |
|---|---|---|
| Gate 0 | ✅ PASS | scan 9 个锚段齐全 |
| Gate A | ✅ PASS | `coverage-extract` 实跑；`--check-filled` exit 0 |
| Gate B | ✅ PASS（表已产出） | Top5 行为契约见 §7；P0-code-bug-01/02 入 Issue List |
| Gate C | ✅ PASS（5 元规则已产出） | `scan.md` Step 3.5；可疑点入 Issue List |
| Gate D | ✅ PASS（狭清单 5 项） | 测试存在/trait ≥2 impl/函数位置/无 stub/签名一致 |
| Gate D-6 | ✅ PASS | `structure.md` 12 节；缺口入 Issue List |
| Gate E | ❌ FAIL | §5.1 漏 12 条 pin；M6 缺口未点名；P0-test-missing-01/02 |
| Gate G | ⚠️ CONCERN（R1） | `VERIFY-CHECK.md` 16/16 = 100%，但 R1 的 Gate E/H FAIL；R2 见 §17 |
| Gate H | ❌ FAIL（H.3/H.6） | H.1/H.2/H.4/H.5 PASS；正文未对齐 v4，且有 P0 缺失 |

> Gate D 的 PASS 只表示 P0 必检的五个窄项通过，不表示没有 P0；P0 由 §9 Issue List 承载。Gate G 的 100% 是抽样一致性，不是文档收敛。

---

## 4. Doc Code Sync 七项

| 项 | 检查 | 结果 |
|---|---|---|
| Sync-1 | 文档含 `static mut` | 无 |
| Sync-2 | 文档路径与仓库 | 相对链接均存在（01/02/03/04/README）；`os/` 路径存在 |
| Sync-3 | 虚构 C 常量 | `IRQ0_VECTOR`/`IRQ8_VECTOR`/`CASCADE_IRQ`/`CLOCK_IRQ`/`ICW*`/`SQUARE_WAVE`/`PROBE_TICKS` 均在 C 源命中 |
| Sync-4 | 文档内 TODO 未走流程 | 无 `> **TODO` |
| Sync-5 | 符号锚点 | 108/108 resolved，0 定义 0，多定义 0 |
| Sync-6 | 路径缺 `os/` 前缀 | 命令块内 `kernel/src/...` 是在 `cd os` 之后，属合法相对路径；正文链接均带 `os/` |
| Sync-7 | 测试数量偏差 >50% | 47/9/3/4/4/3/4/3 全部对上；**但 §5.1 漏 12 条 host-runnable pin**（P1-fact-05，不属 50% 偏差，属覆盖描述不完整） |

---

## 5. 结构分析摘要（Gate D-6）

- **主题思想**：让时间流动是一条不可换序的三段链；C 拆在 `cstart` 与 `bsp_finish_booting` 两个时间点，Rust 用三个 trait 钉住“源/处理者/门”的次序。
- **主语**：Ch1 系统、Ch2 C 函数、Ch3 Rust/设计者、Ch4 类型。§2→§3 缺过渡，是全篇最明显的结构断点。
- **重复**：§3.7/§4.7 同一段讲三遍；§1.1/§2.2/§2.4 的 `system_hz` 污染清单三写；三架构对照碎在 §1.3/§2.5/§4.4/§4.6。
- **收束**：没有收束。§5.4 的“未实跑”与 §6.2 的 `[待验证]` 把结尾交给免责，结论停在 §4.7。
- **承诺兑现**：§3.1/§3.3/§4.1/§4.5/§4.6/§4.7/§5.1/§5.2 共 9 处标题承诺未完全兑现，详见 `structure.md` §5。
- **判定**：结构骨架 A-；结构承诺兑现 FAIL。

---

## 6. 覆盖率穷举（Gate A）

- 命令：`python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel --doc-file 05-clock-interrupt-init.md --semantic-map tools/coverage-extract/kernel-semantic-map.json`
- C 符号 1390；语义范围 152；文档覆盖 99/152 = 65.1%；Rust 覆盖 22/152 = 14.5%；有文档无 Rust 77。
- 语义列已由本轮逐行补齐；`--check-filled` exit 0。
- 有文档无 Rust 的 77 项中，多数是 C 侧背景（`arch/earm/*`、`apic.c` 校准链、`omap_*`），不是本篇必须映射的语义；少数是正文应补的 Rust 对应（`restart_local_timer`、`set_irq_priority`、`claim_context`、`enable_sgi`、GICD seed）。

---

## 7. Top 5 行为契约（Gate B）

| # | 函数/位置 | C 行为（或 doc 声称） | Rust 行为 | 差异类型 | 严重度 | C 证据 | Rust 证据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| 1 | `init_clock_and_interrupts` | C `cstart` 三行；doc 称“五步”，省略号只省注释 | 实际 Step 2 含 x86/riscv PD-22 check-and-reject + VA 注入；Step 2 后 aarch64 SGI | doc↔code | P0 | `main.c:403-475` | `lib.rs:2187-2244,2255-2261` | 省略的是可执行安全代码；aarch64 “全屏蔽”不成立 |
| 2 | `boot_init_timer` | C 编程 + 注册两调用；首次放行在 `put_irq_handler`/ARM 显式 unmask | 编程 → 注册 → 门 → x86 `pic_init` | C→Rust | P1 | `clock.c:294-304`、`interrupt.c:65` | `lib.rs:3471-3518` | 3 条 `use`，doc 写 2 条 |
| 3 | `ClockArch::init_timer` | C 按 `lapic_addr` 分 APIC/PIT | x86 PIT、aarch64 CNTP、riscv SBI TIME | C→Rust | P1 | `arch_clock.c:131-153` | `arch/clock.rs:117`、三实现 | aarch64 引文注释已漂移 |
| 4 | `InterruptRouter::init` | C 十次 `outb` / OMAP 映射 | x86 LAPIC+IOAPIC、GICv3、PLIC；`pic_init` 在 `boot_init_timer` 末尾 | C→Rust | P1 | `i8259.c:28-52` | `plat/src/*/interrupt.rs` | 从片 0x58 vs IDT 0x70（P0-code-bug-01） |
| 5 | `IrqManager::dispatch_claimed` | C 空链 mask → 返回，不 ack | Rust 空链 mask → 返回 `Spurious`，不 `complete(claimed)` | C→Rust | P0 | `interrupt.c:116-135` | `irq_manager.rs:737-742`、`trap_dispatch.rs:2607-2614` | aarch64 已 claim 的 `Some(id)` 未回执 |

---

## 8. 纵向链路与因果链抽样

### 8.1 纵向链路

| 检查项 | 结果 | 证据 |
|---|---|---|
| Ch1 核心概念 → Ch3 决策 | ✅ | §1.1→§3.1/3.2；§1.2→§3.3；§1.3→§3.8 |
| Ch3 决策 → Ch4 实现 | ✅ | 八条决策均有对应 §4 节 |
| Ch4 核心类型/函数 → Ch5 测试 | ⚠️ | PIC 从片向量、aarch64 空链 claim 无测试；`restart_local_timer` 有 pin 但 §5.1 未列 |
| Ch5 测试 → Ch3 决策 | ⚠️ | M6 的“部分覆盖”未点名 claim 缺口 |

### 8.2 因果链抽样（主张—依据—适用范围）

| 抽样 | 文档因果链 | 复核 | 判定 |
|---|---|---|---|
| 1 | `arch_init` 前置 → `system_hz=0` → `PROBE_TICKS=0`/除零 | `apic.c:169/491/517/520` 与 `clock.c:init_clock` 唯一赋值点成立 | ✅ |
| 2 | 跳过注册 → 空链 → `Spurious` | aarch64 上还多一个“已 claim 未 complete”；文档只写返回错误 | ⚠️ 不完整 |
| 3 | 先放行后编程 → 比较器复位 0 → 风暴 | `CNTP_CTL=0x2`/`sie.STIE` 不动，机制成立 | ✅ |
| 4 | 删 `pic_init` → 8259A 固件状态 → PIT 到不了 0x50 | 逻辑成立；但 `pic_init` 从片部分本身是坏的 | ⚠️ |
| 5 | 注册即放行 → `actids` 清零 → `unmask` | C `irq_actids` 与 Rust `actids` 对齐 | ✅ |

---

## 9. Issue List

> 完整 33 条表在 `scan.md`；本节给 P0/P1 全部与 P2 摘要，ID 与 scan 一一对应。P0-fact-05 与 P0-code-bug-01 是同一根因的“文档/代码”双登记；P0-code-bug-02 与 P0-test-missing-02 是“缺陷/测试”双登记，修复时一起处理。

### 9.1 P0（11 open）

| ID | 位置 | 文档声称 | 真值 / 证据 | 修复方向 |
|---|---|---|---|---|
| P0-fact-01 | §3.1 L715–731 | `ClockArch` 11 方法，清单 11 个 | 实际 12 个，漏 `restart_local_timer`（`os/arch/src/arch/clock.rs:210`）；`wake_timer_pin` 3 条测试覆盖它 | 补第 12 签名，改“十一”为“十二” |
| P0-fact-02 | §4.6 L1550–1575 | aarch64 `ArchInit::init()` “两个动作：FPEN + PMU”；FPEN 与 C 等价 | 实际三块：FPEN（`arch_init.rs:52-56`）、PMU（61-75）、`set_gicd_base`（89-96）；C ARM `arch_init` 无 CPACR 写 | 写三动作；讲 GICD seed 的 boot 作用；删“与 C 等价” |
| P0-fact-03 | §4.5 L1489–1507 | riscv `claim` 直接读 PLIC claim；只列 claim/complete | 实际 `claim` 一行委托 `claim_context(self.context)`（`plat/src/riscv64/interrupt.rs:291-301`）；读写在 `claim_context:259-267`；trait 还有 `complete_context`/`enable_sgi` | 按 `claim_context` 重写引文，补方法 |
| P0-fact-04 | §5.2 L1917 | aarch64/riscv 载体源码“只有 PASS 一处”，FAIL 由脚本补打 | riscv 在 UART first-source 超时分支自己打 `### TEST_RESULT: FAIL test-timer-irq-riscv64 ###`（`test-timer-irq-riscv64/src/main.rs:365-369`） | 按三架构实际分支改写 |
| P0-fact-05 | §4.7 L1640/L1652-1653/L1671 | 从片 ICW2 重映射 0x70，与 IDT 一致 | 代码 `IRQ0_VECTOR + 8` = 0x58（`plat/src/x86_64/interrupt.rs:75`）；IDT/桩 0x50–0x57、0x70–0x77 | 文档登记 0x58/0x70 冲突；代码改 `IRQ8_VECTOR` |
| P0-fact-06 | §4.1 L1053–1113 | “五步”；节选省略号只省解释性注释；`ArchInit` 期间每条线都屏蔽 | Step 2 含 x86/riscv PD-22 注入（`lib.rs:2187-2244`）；Step 2 后 aarch64 SGI（2255-2261）；SGI 0 已开 | 补 Step 2.1/2.2/2.5；限定“全屏蔽” |
| P0-fact-07 | §3.5 L926–939；§4.3 L1245；§4.4 L1310；§4.5 L1445–1472；§5.2 L1890 | 6 处“代码引用（按原文）” | `Removed by D-59`→`Deliberately not done here`；`pre-D-59`→“bug (the gate opens only after the source is armed)”；`S-3c/D-59`→`handled separately`；`pre-D-61`→`an earlier implementation`；aarch64 多 `(K8 pairing)`；`D-59 boot contract`→`controller's mask_all...` | 逐块按当前源码重录，或标“历史注释” |
| P0-code-bug-01 | `os/plat/src/x86_64/interrupt.rs:75` | 从片 ICW2 应为 0x70 | 实际写 0x58；IRQ8–15 落 0x58–0x5F，IDT 无门 | 改 `IRQ8_VECTOR` 或 `IRQ0_VECTOR + 0x20`；补测试 |
| P0-code-bug-02 | `os/kernel/src/irq_manager.rs:737-742` | 无 handler 返回 `Spurious`，C 空链不 ack | 先 mask 后返回，不 `complete(claimed)`；aarch64 `Some(id)` 未写 `ICC_EOIR1_EL1`；`trap_dispatch.rs:2609-2614` 只打印 | Spurious 分支先 `complete(claimed)`（`Some` 时）；补测试 |
| P0-test-missing-01 | §5.1/§5.3；`x86_64/interrupt.rs` | M8/M9/M14 部分覆盖；从片位级未证 | 没有断言从片 ICW2 与 IDT 桩一致；`test_timer_irq_is_ioapic_input_zero` 只钉 `TIMER_IRQ=0` | 加 `PIC2 == IRQ8_VECTOR` 与 0x58–0x5F 不在桩范围的 host 断言 |
| P0-test-missing-02 | §5.3 M6；`irq_manager.rs:1220` | “spurious 语义”部分覆盖 | `spurious_irq` 只断言 `Err(Spurious)`，不检查 `complete`；无 aarch64 `Some(id)` 无 handler 测试 | MockController 记录 complete；补 aarch64 语义单测/载体变体 |

### 9.2 P1（13 open）

| ID | 位置 | 问题 | 修复方向 |
|---|---|---|---|
| P1-fact-01 | §3.3 L809–824 | `InterruptRouter` 5/6、`PerCpu` 2/5 方法 | 补 `set_irq_priority`、`claim_context`、`complete_context`、`enable_sgi`，或注明“只列子集” |
| P1-fact-02 | §3.7 L962 | 题注“两条 `use`”，实际 3 条 | 改“三条” |
| P1-fact-03 | §3.5 L912–939 | “三架构实际内容”只贴 riscv；表漏 GICD seed | §3.5 只做边界表；三架构内容统一放 §4.6 |
| P1-fact-04 | §4.7.1 L1716–1720 | 把 Redistributor 唤醒/分组归给 `init_cpu_interface` | 拆到 `init_redistributor` |
| P1-fact-05 | §5.1 L1844–1857 | 漏 `os/arch/tests` 12 条 host-runnable pin | 补表与 `cargo test --test ...` 读数 |
| P1-fact-06 | §5.3 M6 L1943 | “部分覆盖”未点名 aarch64 空链 claim 缺口 | 写明与 P0-code-bug-02 的关系 |
| P1-fact-07 | §4.6 L1596 | “C 的 `arch_init` 有一件事可做（内存裁剪）” | 改比较对象；C x86 有六件事 |
| P1-code-comment-01 | `plat/src/x86_64/interrupt.rs:63-66` | `pic_init` “Must be called before `init_timer`”与调用点矛盾 | 改注释或前移；文档 §5.4 登记 |
| P1-code-comment-02 | `arch/src/arch/clock.rs:102-107` | `init_timer` 说 `init_clock_and_interrupts` 首次调用、`bsp` 再调 | 改注释；文档已披露 |
| P1-code-comment-03 | `arch/src/x86_64/timer_irq_gate.rs:10,46` | PIT 交付写成 IOAPIC input 0 | 改 input 2 |
| P1-code-comment-04 | `plat/src/interrupt.rs:115-128` | trait 表仍列 `ack`/`eoi`；PLIC `mask_all` 写 threshold | 更新表/删过期行 |
| P1-code-comment-05 | `arch/src/arch/clock.rs:115-116` | 说 C 侧 `stop_local_timer` 无具名函数 | C 有 `clock.h:14`/`arch_clock.c:155` |
| P1-struct-01 | §3.7/§4.7 | 同一“编程-注册-放行”近乎逐字重复 | §3.7 留取舍表，代码下沉 §4.7 |

### 9.3 P2（8 open + 1 closed）

| ID | 位置 | 问题 |
|---|---|---|
| P2-fact-01 | §4.9 L1774 | “三条 cfg 转发”漏 mock 兜底 |
| P2-fact-02 | §4.7 L1692 | `boot_init_timer` 之后“三步”过度简化 |
| P2-fact-03 | §2.6 L535/L552 | “两个动作”与“三段结构”表述矛盾 |
| P2-fact-04 | §5.3 M5/M13 | “已覆盖/真机三架构”与 §5.4“未实跑”口径冲突 |
| P2-fact-05 | §5.4 L1972 | “还有三处”未列 `pic_init` 注释与 `InterruptRouter` 表 |
| P2-code-01 | `lib.rs:3472-3474` | 空的 `#[cfg(not(feature="mock"))] {}` 死块 |
| P2-doc-01 | §1.2 L92 vs §2.3 | “每条线屏蔽”与主片留 IRQ2 口径不一致 |
| P2-doc-02 | §1.3 L102 | x86 时钟源“PIT 或 LAPIC”未分 C/Rust 现状 |
| P2-design-01 | 旧 v2 design | `boot_init_timer` 已删除的旧结论；v3 已重推，标 CLOSED |

### 9.4 修复顺序与禁止事项

1. **先代码**：P0-code-bug-01 → P0-code-bug-02；每步补测试（P0-test-missing-01/02），否则修复后自动升级为 P0-test-missing。
2. **再文档事实**：P0-fact-01/02/03/04/05/06；每改一处跑 `anchor-resolve` + `doc-code-map`。
3. **再引文**：P0-fact-07 逐块对当前源码重录；不要用“历史注释”掩盖。
4. **再 P1/P2**：按 `fix-guard.md` 一次一条；不要在同一轮改代码与重写文风。
5. **禁止**：把 `pic_init` 的 0x58 改成 0x70 却不补测试；把 `dispatch_claimed` 的 Spurious 分支改成无条件 `complete` 而不区分 `Some/None`（x86 `None` 会多写 LAPIC EOI）；把 §4.1 的省略号继续标成“只省注释”。

---

## 10. 卓越性审查

### 10.1 文档

- **主题思想 A**：一根“让时间流动”的线贯穿 §2.6/§3.7/§4.7；好。
- **教学性 A-**：20+ 处反事实推演落到寄存器/线号；但 §3.7/§4.7 重复，读者第三遍开始跳读。
- **引文纪律 D**：6 处“按原文”与当前源码不符；这是最大短板。
- **诚实披露 B**：§5.4 披露 4 类注释差，但未披露自己的引文漂移与 `pic_init` 注释矛盾。
- **结构编辑冷读**：§2→§3 主语断裂；`system_hz` 三写；三架构对照碎成四片；结尾无收束。
- **学生型冷读**：主因果链可复述，但 0/2 三重编号（IRQ 号/IOAPIC 引脚/IDT 向量）、“全屏蔽”与 IRQ2、“两个动作”与“三段”会把读者带错。

### 10.2 代码

- **分层 A-**：`ClockArch`/`TimerIrqGate`/`InterruptRouter`/`PerCpuInterruptUnit`/`ArchInit` 五个边界合理；`ClockArch` 职责偏宽但可接受。
- **非法态封堵**：定时器三段、`CLOCK_STATE`、`IRQ_MANAGER` 封得住；PIC 从片向量、aarch64 空链 claim 两处封不住（P0）。
- **死代码/注释**：`boot_init_timer` 空 cfg 块；5 类过期注释（P1/P2）。
- **跨架构 §16.7**：统一语义不统一形状；`TimerIrqGate` 的 x86 no-op 是语义选择，不是“为对称而对称”。
- **不能声称**：代码“卓越”；两处 P0 使 Layer 2 只能 PARTIAL。

### 10.3 冷读团与独立 CodeReview 归因

| 来源 | 独立发现 | 与主会话重叠 | 采纳 |
|---|---|---|---|
| 学生冷读 | 三重编号、全屏蔽/IRQ2、“两个动作/三段”、PIT/LAPIC 分叉、编号未解码 | 4/5 | 5（三重编号与 PIT/LAPIC 降为 P2/P1 表述） |
| 结构冷读 | §3.7/§4.7 重复、`system_hz` 三写、三架构碎、结尾无收束 | 4/6 | 6（P1-struct-01、P2 结构项） |
| 独立 CodeReview | 9 项代码/文档不一致 + `arch_small_files_audit` 2 条 pin | 9/10 | 10（全部重放后采纳） |

独立 CodeReview 的 10 项：方法数、§3.3 方法集、§4.1 注入、§4.6 三动作、riscv claim 委托、PIC 0x58、riscv 载体 FAIL、§5.1 漏 12 pin、`pic_init` 注释矛盾、空链 claim 未回执。主会话逐条重放 `nl -ba`/`grep`/`cargo test` 后进入 Issue List；没有“按票数采纳”。

---

## 11. 测试对账（Gate E）

### 11.1 宿主测试

| 文件 | `#[test]` | host | 结论 |
|---|---|---|---|
| `kernel/src/clock.rs` | 47 | 47 | ✅ |
| `kernel/src/irq_manager.rs` | 9 | 9 | ✅ |
| `arch/src/arch/clock.rs` | 3 | 3 | ✅ |
| `arch/src/arch/timer_irq_gate.rs` | 4 | 2 | ✅ |
| `plat/src/interrupt.rs` | 0 | 0 | ✅ |
| `plat/src/x86_64/interrupt.rs` | 4 | 4 | ✅ |
| `plat/src/arm64/interrupt.rs` | 3 | 0 | ✅ |
| `plat/src/riscv64/interrupt.rs` | 4 | 0 | ✅ |
| `arch/src/x86_64/arch_init.rs` | 3 | 3 | ✅ |
| `arch/src/arm64/arch_init.rs` | 0 | 0 | ✅ |
| `arch/src/riscv64/arch_init.rs` | 0 | 0 | ✅ |

实跑：kernel mock 856 passed / 0 failed / 8 ignored；arch lib 289；plat 7；均 0 failed。

### 11.2 漏列的 host-runnable pin

| 文件 | 条数 | 覆盖 |
|---|---|---|
| `os/arch/tests/arm64_arch_init_pin.rs` | 3 | aarch64 ArchInit 构造/FPEN/PMU |
| `os/arch/tests/riscv64_arch_init_pin.rs` | 4 | riscv ArchInit 空体/PMP/sie |
| `os/arch/tests/wake_timer_pin.rs` | 3 | `restart_local_timer` 三架构 + BSP guard |
| `os/arch/tests/arch_small_files_audit.rs` | 2 | arm64/riscv `TIMER_IRQ` 0/30/0 |
| **合计** | **12** | 全部 host-runnable；§5.1 未列 |

### 11.3 缺口

- **P0-test-missing-01**：`pic_init` 从片 ICW2 与 IDT 桩一致性。
- **P0-test-missing-02**：`dispatch_claimed` 空链 `complete(claimed)`。
- **M6**：§5.3 只说“部分覆盖”，未点名 aarch64 `Some(id)` 空链。
- **M5/M13**：QEMU 未跑，§5.3 的“已覆盖/真机载体”应改为“测试存在，未实跑”。

---

## 12. 对抗性审阅与独立验证

### 12.1 材料隔离

- 学生冷读只给文档全文，不给源码/规则/主会话判断；结构冷读只给文档；独立 CodeReview 给文档 + 允许源码清单，不给 `.review`/`.design`。
- 所有子代理结论先过五类归因（真缺陷/转出/假阳性/非缺陷表述/证据不足），再由主会话重放命令。

### 12.2 Gate G

- `verify-check.py --scan ... --state ... --symbols ... --sample-ratio 0.5 --seed 20261102`
- 抽样 16 条 issue，独立重判 16/16 一致；49 个符号抽样已填。
- R1 时 Gate E/H 本身 FAIL，所以 Gate G 判 **CONCERN**，不是 PASS；R2 已修复并见 §17。
- 同 agent 局限：验证者与被验证者是同一模型；已重放 `nl -ba`/`grep`/`cargo test` 命令，未跨 agent 复核全部结论。

### 12.3 负抽样

从未被 Issue List 命中的章节抽 3 节：
- §6.1 的“引脚 2 = 0x50”向量算式：`ioapic_route_and_mask` 对 pin 2 取 irq 0 → `0x50`，正确。
- §6.2 命令 3 的“9 行”读数：实际 `grep -c` = 9，正确。
- §7 的链接：`anchor-resolve` 108/108，正确。
- 结论：3 节未命中新缺陷；负抽样通过。

---

## 13. Rule Discovery（Step 5.7）

| 候选 | 案例 | 归类 | 结论 |
|---|---|---|---|
| R-DISC-01 “披露自指盲区” | §5.4 披露代码注释过期，但自己的 6 处引文块也过期；披露清单没有覆盖引文 | 新模式候选 | 建议加入 `DOC-ITER` S1.2：凡有“注释与现码不符”披露段，必须对同篇引文块跑一次 FID，并把差异并入披露表 |
| R-DISC-02 “trait 方法集漏默认方法” | `ClockArch` 11/12、`InterruptRouter` 5/6、`PerCpu` 2/5 | 现有模式 5/73 的变体 | 建议 review 清单加一行：trait 签名示意必须与 `grep 'fn '` 计数对齐，默认方法也算 |
| R-DISC-03 “载体 PASS/FAIL 分支” | §5.2 说 riscv 只有 PASS，实际有 FAIL | 现有测试虚构/漂移模式 | 已有 Gate E 可拦；建议把“源码分支”而不是“脚本补打”作为判据 |

本轮未发现需要立即修改规则源的新 P0 流程模式；R-DISC-01 建议进下一轮规则评审。

---

## 14. 轨迹复盘

### 14.1 覆盖遗漏

- 文档 7 章、全部 40+ 小节均有阅读记录；28 个关联文件全部打开/抽查。
- 未逐行读 `arch/earm` 的全部 BSP 文件，只读 `arch_system.c`/`omap_intr.c`/`omap_timer.c`/`hw_intr.c`；这对本篇射程够用，但“ARM 全屏”不是本轮结论。
- 负抽样 3 节（§6.1/§6.2/§7）未命中新缺陷。
- 仍可能漏：`os/arch/tests` 中其他与本篇相关的 pin（已补 4 个文件，未穷举整个 tests 目录）；QEMU 实跑未做。

### 14.2 结论缺陷

- 初判 `P0-fact-05` 与 `P0-code-bug-01` 为同一根因，保留双登记是刻意：文档说 0x70、代码写 0x58，修一侧不够。
- `P0-fact-06` 的严重度可能有人判 P1（“只是省略”）；本轮判 P0，因为省略的是 PD-22 check-and-reject 与 aarch64 SGI，且“全屏蔽”安全论证被破坏。
- `P0-code-bug-01` 目前是**潜在**缺陷（从片线全屏蔽，启动载体不触发）；本轮仍判 P0，因为函数契约与 IDT 门直接冲突，一旦 IRQ8–15 放行就是无门向量。
- `P0-code-bug-02` 的触发需要“已 claim 的 valid INTID 且无 handler”；SGI 已特判，但 remove_hook 竞态/固件遗留 SPI 仍可能。判 P0 是因为它违反 claim/complete 配对协议，不是因为它每天都触发。
- 没有把 L3 推断写成 L1：每个 P0 都有文件行号或命令输出；QEMU 相关一律标“未实跑”。

### 14.3 对抗性装置有效性

| 装置 | 独立发现 | 重叠 | 采纳 | 评价 |
|---|---|---|---|---|
| 学生冷读 | 5 | 4 | 5 | 有效：发现“全屏蔽”与 IRQ2、三重编号、PIT/LAPIC 分叉 |
| 结构冷读 | 6 | 4 | 6 | 有效：重复、三写、三架构碎片、结尾无收束 |
| 独立 CodeReview | 10 | 9 | 10 | 高有效：新增 `arch_small_files_audit` 2 条 pin；确认 9 项 |
| 负抽样 | 0 新 | — | — | 3 节无新缺陷，说明主会话覆盖尚可 |

### 14.4 过程改进

1. **引文块自动对账前移**：`DOC-ITER` S1.2 的 FID 应在每次文档提交前跑，而不是 review 时手工 grep；本轮 6 处漂移本可机器发现。
2. **trait 方法集计数工具化**：对 `pub trait` 的 `fn ` 计数与文档“N 个方法”对账；默认方法/后加方法最易漏。
3. **测试清单工具化**：`cargo test --list` 只覆盖 lib；应把 `os/*/tests` 也纳入 §5.1 的生成器。
4. **设计快照新鲜度**：`05-design.v2` 的“boot_init_timer 已删除”能存活到 2026-11，说明快照没有与代码变更联动；建议在 `design-coverage-check` 加“关键符号存在性”抽查。
5. **空链 claim 配对检查**：把 `dispatch_claimed` 的 `return Err(Spurious)` 前是否 `complete` 做成静态检查/测试模板。

### 14.5 本轮 review 自身是否达标

- 覆盖：P0/P1 有证据，P2 有清单；未跑 QEMU 是明确限制。
- 结论：11 P0 open、Gate E/H FAIL、Gate G CONCERN；报告没有给 CONVERGED 背书。
- 不足：未做跨 agent 全量复核；`arch/earm` 只读关键文件；QEMU 三架构未跑；P0-code-bug-01 的“真实触发路径”未在 QEMU 上复现（静态判定）。
- 自评：**达到可交付的 review 报告标准，但不是“修复完成”标准**。

---

## 15. 确认清单与限制

- [x] 目标文档哈希绑定
- [x] 28 个关联文件存在性
- [x] 锚点 108/108
- [x] C ground truth 关键函数复算
- [x] Gate 0/A/B/C/D/D-6 证据
- [x] Gate E 测试实跑
- [x] Gate G 抽样 16/16（CONCERN）
- [x] Gate H v4/v3 快照
- [x] 冷读团/学生/独立 CodeReview
- [x] Rule Discovery
- [x] 轨迹复盘
- [x] QEMU 三架构实跑（R2 已做，见 §17）
- [ ] 跨 agent 全量复核（未做）
- [x] 修复（R2 已完成，见 §17）

**限制**：
1. R1 未跑 QEMU；R2 已跑 x86/aarch64/riscv 三架构，见 §17。
2. R1 未修改文档/代码；R2 已修复全部 P0/P1/P2，见 §17。
3. 同 agent 验证，已重放命令但不等于独立异构复核。
4. `cargo check` 有既有 warning；`clippy -D warnings` 未作为门（既有告警非本轮引入）。

---

## 16. 附录

### 16.1 关键命令

```bash
# 基线
git rev-parse HEAD
sha256sum rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md

# 机械门
bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md --check
AWK=gawk bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md

# 覆盖率
python3 tools/coverage-extract/coverage-extract.py kernel rewrite-notes/01-stage-kernel \
  --doc-file 05-clock-interrupt-init.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json
python3 tools/coverage-extract/coverage-extract.py --check-filled \
  .review/zcode/01-stage-kernel/05-clock-interrupt-init/SYMBOLS.md

# 测试
cd os
cargo test -p minix-kernel --features mock --lib
cargo test -p minix-arch --features runtime-window --lib
cargo test -p minix-plat --lib
cargo test -p minix-arch --test arm64_arch_init_pin
cargo test -p minix-arch --test riscv64_arch_init_pin
cargo test -p minix-arch --test wake_timer_pin

# Gate G
python3 tools/verify-check.py --scan .review/zcode/01-stage-kernel/05-clock-interrupt-init/scan.md \
  --state .review/zcode/01-stage-kernel/STATE.md \
  --symbols .review/zcode/01-stage-kernel/05-clock-interrupt-init/SYMBOLS.md \
  --sample-ratio 0.5 --seed 20261102 \
  --output .review/zcode/01-stage-kernel/05-clock-interrupt-init/VERIFY-CHECK.md
```

### 16.2 R1 关键证据行（修复前）

| 结论 | 命令/文件 | R1 读数 |
|---|---|---|
| `ClockArch` 11 方法 | `os/arch/src/arch/clock.rs` | R1 计数 12，正文写 11 |
| `PerCpu` 2 方法 | `os/plat/src/interrupt.rs:184-242` | R1 实际 5，正文写 2 |
| aarch64 两动作 | `os/arch/src/arm64/arch_init.rs:52-96` | R1 实际三动作，正文漏 GICD |
| riscv claim 直接读 | `os/plat/src/riscv64/interrupt.rs:291-301` | R1 实际委托 `claim_context` |
| PIC 从片 0x58 | `os/plat/src/x86_64/interrupt.rs:75` | R1 实际 `IRQ0_VECTOR + 8` |
| IDT 0x70 | `os/arch/src/x86_64/trap_entry.rs:242-243` | `0x70 + irq` |
| 空链不 complete | `os/kernel/src/irq_manager.rs:737-742` | R1 return before complete |
| riscv 载体 FAIL | `test-timer-irq-riscv64/src/main.rs:365-369` | explicit FAIL |
| §5.1 漏 pin | `os/arch/tests/*` | 3+4+3+2 = 12 相关 |

### 16.3 修复后的验收建议

1. `pic_init` 从片 ICW2 改成 `IRQ8_VECTOR`，新增 host 测试断言 `0x70` 且在 IDT 桩范围；`cargo test -p minix-plat --lib`。
2. `dispatch_claimed` 空链 `complete(claimed)`（`Some` 时），新增 MockController 调用序列断言；aarch64 语义单测或 QEMU 变体。
3. 文档按 v4 大纲重写 §3.1/§3.3/§4.1/§4.5/§4.6/§4.7/§4.9/§5.2；引文块逐块对当前源码。
4. §5.1 补 12 条 pin；§5.3 M6 点名缺口；§5.4 把“未实跑”与“已覆盖”口径统一。
5. 修后跑第二轮 full review，目标 P0=0、Gate E/H PASS、Gate G PASS；在此之前不得写 CONVERGED。

---

## 17. R2 修复与深度回归（2026-11-03）

> R1 是只审不修，结论 NOT CONVERGED。R2 按 `fix-guard.md` 逐条修复，再跑机械门、宿主测试、三架构 QEMU 与回归抽样。本节是最终状态；R1 的 Issue List 保留在 §9，作为修复前台账。

### 17.1 基线与修复范围

| 项 | 值 |
|---|---|
| R1 基线 HEAD | `2320f7959a63d80a54dc80c932248856eea03c7e` |
| R1 文档哈希 | `8cacc3c86db660b06a3986c1bc15f241c992d3e77e0cda6245c571aae96de7fe` |
| R2 文档哈希 | `f6d4b6440aa378175cb2e761cd0e59437b1c8765b85f47adae099e3bec14cd52` |
| R2 文档行数 | 2196 行 / 177098 字节 |
| 修复文件 | 05 正文；`os/plat/src/x86_64/interrupt.rs`；`os/kernel/src/irq_manager.rs`；`os/kernel/src/lib.rs`；`os/plat/src/interrupt.rs`；`os/arch/src/arch/{clock,timer_irq_gate}.rs`；`os/arch/src/x86_64/{clock,timer_irq_gate}.rs`；`os/arch/src/riscv64/timer_irq_gate.rs` |
| 未触碰 | `AI-chats/daily.todo.md`、`rewrite-notes/coordination/PENDING-DECISIONS-CROSS-BOARD-20261102.md`、04 review 产物 |

### 17.2 修复台账（33/33）

| ID | 状态 | 修复点 | 验证 |
|---|---|---|---|
| P0-fact-01 | ✅ FIXED | §3.1 补 `restart_local_timer`，改“十二个方法” | trait 计数 12 |
| P0-fact-02 | ✅ FIXED | §4.6 写 FPEN/PMU/GICD 三动作；删“与 C 等价” | `arch_init.rs:52-96` |
| P0-fact-03 | ✅ FIXED | §4.5 改为 `claim` 委托 + `claim_context` 两段引文 | `riscv64/interrupt.rs:291-301` |
| P0-fact-04 | ✅ FIXED | §5.2 写 riscv 显式 FAIL 分支 | `main.rs:365-369` |
| P0-fact-05 | ✅ FIXED | §4.7 写 `IRQ8_VECTOR=0x70`，登记 0x58 的错误路径 | code 0x70 |
| P0-fact-06 | ✅ FIXED | §4.1 新增 2a/2b/2c 子步与限定语 | `lib.rs:2187-2261` |
| P0-fact-07 | ✅ FIXED | 6 处引文逐块改为当前源码 | 旧串 grep 全 0 |
| P0-code-bug-01 | ✅ FIXED | `pic_init` 从片 ICW2 改 `IRQ8_VECTOR` | `test_pic_slave_icw2...`；QEMU x86 PASS |
| P0-code-bug-02 | ✅ FIXED | 空链 `Some(claimed)` 先 `complete` 再返回 | `spurious_claimed_irq...`；10 irq tests |
| P0-test-missing-01 | ✅ FIXED | 新增 PIC 从片向量一致性测试 | `minix-plat` 8 passed |
| P0-test-missing-02 | ✅ FIXED | 新增空链 claim 回执测试 | `minix-kernel` 858 passed |
| P1-fact-01 | ✅ FIXED | §3.3 补 `set_irq_priority`、per-context、`enable_sgi` | trait 计数确认 |
| P1-fact-02 | ✅ FIXED | 删除 §3.7 重复代码块；§4.7 保留三条 `use` | 无“两条 use” |
| P1-fact-03 | ✅ FIXED | §3.5 只留 riscv 示例，三架构内容回指 §4.6；表补 GICD | doc 检查 |
| P1-fact-04 | ✅ FIXED | §4.7.1 把 Redistributor 归 `init_redistributor` | doc 检查 |
| P1-fact-05 | ✅ FIXED | §5.1 补 4 个 pin 测试文件（12 条相关） | 3/4/3/5 passed |
| P1-fact-06 | ✅ FIXED | §5.3 M6 改名与缺口同步 | doc 检查 |
| P1-fact-07 | ✅ FIXED | §4.6 改“C x86 六件事/ARM 四件/riscv 空” | doc 检查 |
| P1-code-comment-01 | ✅ FIXED | `pic_init` 注释改为“末尾调用、IF=0 安全” | grep |
| P1-code-comment-02 | ✅ FIXED | `ClockArch::init_timer` 注释改为 `boot_init_timer` 唯一调用 | grep |
| P1-code-comment-03 | ✅ FIXED | x86 `TimerIrqGate` 与 trait 表改 IOAPIC 输入 2 | grep |
| P1-code-comment-04 | ✅ FIXED | `InterruptRouter` 表删 `ack/eoi`，PLIC `mask_all` 改 enable=0 | grep |
| P1-code-comment-05 | ✅ FIXED | `stop_local_timer` 注释改 C 具名函数 | grep |
| P1-struct-01 | ✅ FIXED | §3.7 只留签名示意，函数体下沉 §4.7 | doc 检查 |
| P2-fact-01 | ✅ FIXED | §4.9 补 mock 兜底 | doc 检查 |
| P2-fact-02 | ✅ FIXED | §4.7 改“主干三步 + 回指 08” | doc 检查 |
| P2-fact-03 | ✅ FIXED | §2.6 改“两个调用、三段语义、第三段在注册路径” | doc 检查 |
| P2-fact-04 | ✅ FIXED | §5.3 M5/M13 改为“已实跑” | QEMU 日志 |
| P2-fact-05 | ✅ FIXED | §5.4 披露段改为当前注释一致声明 | doc 检查 |
| P2-code-01 | ✅ FIXED | 删 `boot_init_timer` 空 cfg 块 | grep |
| P2-doc-01 | ✅ FIXED | §1.2 改“除级联外全屏蔽” | doc 检查 |
| P2-doc-02 | ✅ FIXED | §1.3 分 Rust PIT / C APIC 现状 | doc 检查 |
| P2-design-01 | ✅ CLOSED | v3 设计快照替代 v2 的“boot_init_timer 已删除” | `.design/05-design.v3.md` |

**未登记的顺手修复**：
- `os/arch/src/x86_64/clock.rs` 的 `read_tsc()` 注释从“not in Minix3”改为 `read_tsc_64()` C 锚点。
- `os/arch/src/x86_64/clock.rs` 的 `intr_init_8254` 注释改为 `init_8253A_timer`。
- `os/kernel/src/lib.rs` 的 `boot_init_timer` 头注释把“IOAPIC IRQ 0 line”改为“IOAPIC input 2 for ISA IRQ0”。
- `os/arch/src/riscv64/timer_irq_gate.rs` 的模块注释从“直接写 CLINT MMIO”改为“SBI TIME ecall”。

### 17.3 回归门禁

| 门 | R2 结果 | 证据 |
|---|---|---|
| Gate 0 | ✅ PASS | 9 个锚段齐全 |
| Gate A | ✅ PASS | 新跑 `coverage-extract`：C 符号 1390，语义域 151，doc 99/151=65.6%，Rust 23/151=15.2%；`--check-filled` exit 0 |
| Gate B | ✅ PASS | Top5 行为契约的 P0 差异已关闭：PIC 0x70、空链 complete、§4.1 子步、aarch64 三动作、riscv claim 委托 |
| Gate C | ✅ PASS | 5 元规则无新增可疑点；`pic_init` 注释与调用点已一致 |
| Gate D | ✅ PASS | 测试/trait/签名/无 stub 五项 |
| Gate D-6 | ✅ PASS | `structure.md` 12 节；P1-struct-01 已修 |
| Gate E | ✅ PASS | 宿主 858/289/8；pin 3/4/3/5；三架构 QEMU PASS |
| Gate G | ✅ PASS | `VERIFY-CHECK-R2.md`：12/12 = 100%；同 agent + 命令重放 |
| Gate H | ✅ PASS | H.1 v3/v4 在场；H.2 12/14=85.7% + D1/D2 已修；H.3/H.6 正文已对齐 v4 |

### 17.4 测试与 QEMU

| 命令 | 结果 |
|---|---|
| `cargo test -p minix-kernel --features mock --lib` | **858 passed / 0 failed / 8 ignored** |
| `cargo test -p minix-arch --features runtime-window --lib` | **289 passed / 0 failed** |
| `cargo test -p minix-plat --lib` | **8 passed / 0 failed** |
| `cargo test -p minix-arch --test arm64_arch_init_pin` | 3 passed |
| `cargo test -p minix-arch --test riscv64_arch_init_pin` | 4 passed |
| `cargo test -p minix-arch --test wake_timer_pin` | 3 passed |
| `cargo test -p minix-arch --test arch_small_files_audit` | 5 passed |
| QEMU x86_64 `test-timer-irq` | `### TEST_RESULT: PASS test-timer-irq ###` |
| QEMU aarch64 `test-timer-irq-aarch64` | `### TEST_RESULT: PASS test-timer-irq-aarch64 ###` |
| QEMU riscv64 `test-timer-irq-riscv64` | `### TEST_RESULT: PASS test-timer-irq-riscv64 ###` |

原始日志：`.review/.../trace/logs/43-postfix-tests.log`、`45-qemu-x86.log`、`46-qemu-aarch64.log`、`47-qemu-riscv64.log`。

### 17.5 回归新发现

- **无新 P0/P1**。
- 修复过程中顺手修掉 4 处未登记的 P2 注释债务（`read_tsc`、`intr_init_8254`、`boot_init_timer` 的 IOAPIC 引脚、riscv `TimerIrqGate` 的 MMIO/SBI 表述）。
- 残留 backlog（非本轮 Issue List）：R05-T01（位级寄存器真值）、R05-T05（用户态 tick）、R05-T06（风暴注入）。这三项需要 GDB/MMIO 单步、CPL3 调度链或故障注入载体，不在当前修复射程。

### 17.6 最终判定

- **P0**: 0 open（11/11 fixed）。
- **P1**: 0 open（13/13 fixed）。
- **P2**: 0 open（8/8 fixed；P2-design-01 由 v3 快照关闭）。
- **Layer 1**: **PASS**。
- **Layer 2**: **PASS_WITH_BACKLOG**（结构/教学 A-；R05-T01/T05/T06 为测试深度 backlog）。
- **文档状态**: **CONVERGED_WITH_WARNING_BACKLOG**。
- **不得声称**: 位级寄存器真值已证、用户态 tick 已证、风暴注入已证、跨 agent 全量复核完成。

---

**报告状态**：`CONVERGED_WITH_WARNING_BACKLOG`（P0=0；P1=0；P2=0 open；backlog R05-T01/T05/T06）。R1 的 `NOT CONVERGED` 结论保留在 §0–§16，作为修复前历史。
