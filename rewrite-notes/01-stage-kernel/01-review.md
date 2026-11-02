# 01-boot-shim-bootstrap 深度 Review 报告

> **审查对象**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3,073 行，sha256 `fda90e0045ed4ba1e7675e833ef99b46c1b770cdebcb5cd6dc52961e05b5f776`）
> **审查日期**：2026-11-02
> **审查模式**：完整 Review（文档 + 关联 Rust 代码），scope=doc，深度模式
> **真值优先序**：Minix3 C 源码行为 > 设计契约 > Rust 实现 > 文档 > AI 分析
> **本轮立场**：**只审不修**。P0 全部保持 open，文档状态 **NOT CONVERGED**。
> **规范制品**：`.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/`（scan / SYMBOLS / structure / excellence / test-audit）；独立验证见 `.review/zcode/01-stage-kernel/VERIFY-CHECK.md`。
>
> **文档性质说明**：本文是 review 报告，不是正式章节文档。它必须保留 `scan.md`、`Gate`、`P0/P1/P2` 等审查术语与审查日期；这些不是正文的过程痕迹，而是报告的取证坐标。因此本文不参与 `tools/doc-style-lint.sh` 对正式文档的 SL-4/SL-5/SL-7 门；正式文档 `01-boot-shim-bootstrap.md` 的文风门结果另见 §4（error=0，仅 L1536 一处残留）。

---

## 0. 范围、方法与结论先行

### 0.1 范围

- **文档**：全文 7 章（概述、C 源码分析、Rust 设计决策、实现详解、测试、诊断、参见），62 个标题。
- **关联 Rust**：`os/boot-shim`、`os/libs/minix-boot`、`os/kernel` 启动路径、`os/arch` 分页/分配器、`os/kernel-image` 直核面，共 41 个路径（`tools/doc-code-map.sh --check` 输出）。
- **C ground truth**：`minix3/minix/kernel/arch/i386/{head.S,pre_init.c,pg_utils.c,kernel.lds,arch_smp.c,arch_system.c,protect.c,memory.c}`、`arch/earm/*`、`multiboot.h`、`param.h`、`const.h`、`main.c`、`table.c`、`vm/{pagetable,main,alloc,utility}.c`、`boot2.c`、`exec.c`、`loadfile_elf32.c`、`boot.cfg.default`。
- **不在范围**：不修改被测文档与 `os/` 代码；不实际启动 QEMU（脚本静态审计）；不改 `prompt/`、`tools/`。

### 0.2 方法

1. **R0 预检**：`review-init`、4 条 `ls`、`design-coverage-check`、`doc-code-map --check`、基线 sha256/行数/工作树状态。
2. **R1 正确性**：Gate H 快照重推；覆盖率穷举；锚点解析；四个章节块（§1–2 / §3 / §4 / §5–7）由只读子代理并行审查；**主会话逐条重放所有 P0/P1 证据命令**；代码维度六项；测试对账；Doc Code Sync 七项。
3. **R2 卓越性**：文档 12 节结构、教学构件、反向推演、学生型走查；代码分层、trait、非法态、死代码、跨架构三问。
4. **R3 收敛**：Issue List 汇总、严重度调和、Gate G `verify-check.py`（seed 20261102，抽样 20%）。
5. **R4/R5**：本报告 + 轨迹复盘（§13）。

### 0.3 结论先行

| 维度 | 结果 |
|---|---|
| Layer 1 正确性 | **FAIL**：14 个 P0 open（12 个 P0-fact + 2 个 P0-test-missing） |
| Layer 2 卓越性 | **PARTIAL**：文档讲法整体成立，但被正确性缺陷拖累；代码有若干系统性改进点 |
| Blocker Gates | Gate D / E / H **FAIL**（文档级）；Gate 0/A/B/C/D-6/G PASS |
| 文档状态 | **NOT CONVERGED** |
| 一句话判决 | 文档的**概念骨架与大部分 C 分析是扎实的**；问题集中在“生产路径叙述停留在旧版本”“测试覆盖声称与实测不符”“若干常量/返回值/路径的具体事实漂移”。这些不是讲法问题，是必须回代码核对的事实问题。 |

---

## 1. 执行摘要

### 1.1 问题计数

| 级别 | 数量 | 构成 |
|---|---:|---|
| P0 | 14 | 12 条事实错误 + 2 条测试缺失/假覆盖 |
| P1 | 32 | 20 条事实、5 条流程/文风、4 条测试自身、3 条代码 |
| P2 | 69 | 38 条事实/表述、16 条代码、13 条测试、2 条文风卫生项 |

### 1.2 最关键的八组发现

1. **生产路径写错**（P0-fact-02/03）。文档把 `UefiBootShim::prepare_boot` 当作 x86_64 UEFI 生产路径，实际 `os/boot-shim/src/main.rs:65` 调的是 `prepare_and_enter_kernel_image(1024)`；`UefiBootShim::prepare_boot` 在仓内没有生产调用方。文档还把 `RiscvUbootBootShim` 写成不存在的 `OpenSbiBootShim`，并称其“没有调用方”，实际 `uboot_main.rs:333` 正在调用它。这会让读者按错误路径排查启动问题，是本轮最重的一组。
2. **C 侧结论与 VM 回收冲突**（P0-fact-01）。§2.4.2 说“其余 11 个模块永不归还、永久少掉”，但 `minix3/minix/servers/vm/main.c:482-520` 明确用 `mem_add_total_pages` 补总量、`exec_bootproc` 后 `free_mem` 归还每个模块 blob。正确表述是“内核 memmap 这本账不收回”，不是“系统永久少掉”。
3. **builder 与字段取值漂移**（P0-fact-04/05/11）。`build_kernel_info` 实际 10 个参数并接收 `param_buf`；`reserved_regions` 在 OpenSBI/riscv 直核腿是 DTB 裁剪出的非空列表，aarch64 直核才是 `0`；`RESERVED_REGION_STORE_LEN=256` 与 `VM_BOOT_HANDOFF_MAX_IDENT=64` 不是同一常量。
4. **分配器与 Step 结构过时**（P0-fact-06/10）。`pt_alloc::register` 在 release 下是“保留首个”而不是“静默改写”；Step 1 的代码块还是旧版 aarch64-only `#[cfg]`，实际已有 `identity_device_windows()` + `DEV_PIECE` + riscv 设备窗口，且还有未写入正文的 Step 2c。
5. **测试转录与覆盖声称不符**（P0-fact-07、P0-test-missing-01/02）。§5.2.1 引用 `cargo test` 输出 28/32，实测 53/65；M6 声称 `paging.rs` 15 条 map_huge 系列，实际 16 条且 0 条 map_huge；M4 声称四入口源码序钉，实际漏掉 v1 接收入口。
6. **诊断里程碑与路径不存在**（P0-fact-08/09/12）。§6.1 的两条 x86 里程碑在源码中零命中；§7.3 的 `handoff.rs` 不存在；§6.3 的大页阶梯 panic 字符串不存在。
7. **测试自身质量问题**（P1-test-01..04、P2-test 系列）。有错误夹具（riscv64 identity 用 `0x8000_0000` 而链接脚本是 `0x80200000`）、不读 `.ld` 的“linker-script”测试、名为 loader 却不调 loader 的测试、影子分类函数测试。
8. **代码层候选**（P1-code-01..03）。EBS 配对循环每轮重新分配 map buffer（违反设计“失败后只复用原缓冲”）；U-Boot 表 `len` 写槽容量而非文件实际长度；公开 `store_kernel_info` 对非 ASCII 名字截断后 `from_utf8_unchecked` 有 UB 风险。
9. **教学走查不达标**（P1-docStyle-02/03/04）。只凭本文的学生走查判定 5 个节“只能靠结论”（§3.1/§3.2/§3.9/§4.7/§6.3），4 处自声明清单未兑现（§2.6.1 五步、§2.6.3/§3.9 三前提、§3.2 四收益、§5.4 M1–M15），另有 11 处过程编号、20+ 处元话语、7 处黑话。

---

## 2. 维度覆盖自检

| 检查维度 | 来源 | 应执行 | 实际 | 跳过理由 |
|---|---|---|---|---|
| §2.0 Claims-Evidence Tracing | doc-checklist | ✅ | ✅ | — |
| §2.1 概念准确性 | doc-checklist | ✅ | ✅ | — |
| §2.2 C 代码引用验证 | doc-checklist | ✅ | ✅ | — |
| §2.3 数据结构覆盖 | doc-checklist | ✅ | ✅ | — |
| §2.4 文档与代码一致性 | doc-checklist | ✅ | ✅ | — |
| §2.4b Rust 习惯用法同步 | doc-checklist | ✅ | ✅（`static mut` 0 命中） | — |
| §2.4c 路径约定 | doc-checklist | ✅ | ✅（`os/kernel/src/` 48 处，无缺前缀） | — |
| §2.4d 符号锚点解析 | doc-checklist | ✅ | ✅（98/98） | — |
| §2.4k 文档 Rust 代码块逐块审查 | doc-checklist | ✅ | ✅（§3/§4 逐块；漂移见 P0-fact-04/10） | — |
| §2.5 架构演进说明 | doc-checklist | ✅ | ✅（三架构对照；`[ARCH]` 缺口 P1-process-01） | — |
| §2.6 文档间交叉引用 | doc-checklist | ✅ | ✅ | — |
| §2.7 图表质量 | doc-checklist | ✅ | ✅（§3 图少，P2 卓越性） | — |
| §2.8 C 源码覆盖完整性 | doc-checklist | ✅ | ✅（SYMBOLS 192 语义域） | — |
| §2.9 设计决策质量 | doc-checklist | ✅ | ✅（outline-review D1–D9） | — |
| §2.10 章节链路验证 | doc-checklist | ✅ | ✅（structure §8/§12） | — |
| §2.11 文档风格 | doc-checklist | ✅ | ✅（style-lint error=0；L1536 残留） | — |
| §3.1–3.4 可读性 | doc-checklist | ✅ | ✅ | — |
| §3.5 教学性 | doc-checklist | ✅ | ✅（学生走查见 §10.3） | — |
| §3.75 过程痕迹 | doc-checklist | ✅ | ✅（1 处残留） | — |
| §3.94 禁引中间产物 | doc-checklist | ✅ | ✅（0 命中） | — |
| §1 Rewrite 质量 | code-checklist | ✅ | ✅ | — |
| §2 硬件抽象 | code-checklist | ✅ | ✅ | — |
| §4 执行模型/SMP | code-checklist | ✅ | ✅ | — |
| §8 命名 | code-checklist | ✅ | ✅ | — |
| §12 no_std | code-checklist | ✅ | ✅ | — |
| §13 设计-代码一致性 | code-checklist | ✅ | ✅（`boot_form` 登记表过时） | — |
| §14 C-Rust 语义对齐 | code-checklist | ✅ | ✅ | — |
| 跨文档联动 | patterns | ✅ | ✅ | — |
| Gate 0/A/B/C/D/D-6/E/G/H | review-process | ✅ | ✅（D/E/H 文档级 FAIL） | — |
| 测试五维 | test-audit | ✅ | ✅ | — |
| 卓越性 §16–21 | code-excellence | ✅ | ✅（Layer 2 PARTIAL） | — |

### 2.1 规则引用清单（每条结论可回溯）

| 规则文件 | 本轮实际使用的条款 | 在本报告/制品中的落地 |
|---|---|---|
| `review.md` | 真值优先序；P0 六分类；行为语义优先；硬件抽象；概念抽象；AI 输出模板；自检 1–5；Layer 1/2 | §0.3、§1、§9、§10、§14 |
| `review-process.md` | Step 0–7；Gate 0/A/B/C/D/D-6/E/G/H；L1/L2/L3；Step 5.6 独立验证；Step 5.7 Rule Discovery；Step 7.1 收敛 | §2、§3、§12、§13、scan.md |
| `review-cmds.md` | §二 full-review；§一 通用强制门 #1–#9（锚点/测试名/文风/translate/fix-guard/文档-代码同步/产物标注/Ground Truth/教学承载） | §2、§4、§9、§10 |
| `review-patterns.md` | §0 P0 必检 5 项；模式 1–15（文档）、16–29（代码）、35–40（测试）、48–78（叙事/流程）、79–85（架构/锚点） | §9、§10、scan.md |
| `review-doc-checklist.md` | §1 结构；§2.0–2.11；§3 可读性；§4 Doc Code Sync 七项 | §2、§4、§5 |
| `review-core-semantics.md` | §1.6 四条公理 + 三问；8 字段行为契约 | §7、§10、excellence.md |
| `review-code-checklist.md` | §1 Rewrite；§2 硬件抽象；§4 执行模型/SMP；§8 命名；§12 no_std；§13 设计-代码；§14 C-Rust | §2、§10、scan.md Step 3.6 |
| `review-code-excellence.md` | §16–21（分层、trait、非法态、跨架构、死代码） | §10、excellence.md |
| `review-doc-excellence.md` | §4.1–4.4（教学深度、组织合理性） | §10、excellence.md |
| `style-bible` | 技术博客文风；禁黑话/缩写/文言文；锚点纪律；过程痕迹禁令；教学构件清单；反膨胀≠砍教学 | §4、§10 |
| `AI-chats/doc-gen-prompt.md` | 两条总则（骨架不重排、不删教学）；第 1.5 步大纲；第 2 步 12 条写作底线；第 7 步五道门 | §5、§10、§13 |
| `AI-chats/doc-gen-iterate-prompt-v2.md` | 学生实读；认知跃迁丢失清单；M 标尺；闭环三态判定 | §10.3、§13.3 |

---

## 3. Blocker Gates 证据

| Gate | 状态 | 证据摘要 |
|---|---|---|
| Gate 0 | PASS | 9 个必备锚段齐全；scan/SYMBOLS/structure/VERIFY-CHECK/STATE 均已落盘 |
| Gate A | PASS | `coverage-extract.py` exit 0；`--check-filled` exit 0；语义列 0 残留 |
| Gate B | PASS | Top 5 行为契约表（8 字段 × 5 函数，见 §7） |
| Gate C | PASS | 五元规则表（外部知识/接口纯度/返回值/资源/理由）逐项 |
| Gate D | **FAIL** | 第 1 项 M6 假覆盖；第 3 项 `handoff.rs` 不存在；第 5 项 `build_kernel_info` 签名不符 |
| Gate D-6 | PASS | `structure.md` 12 节；概念层闭环 |
| Gate E | **FAIL** | 测试名基本存在，但 M6 假覆盖、28/32 转录漂移、`paging.rs` 15 vs 16 |
| Gate G | PASS（同 agent） | `verify-check.py`：Gate 0 PASS、8 证据块齐、抽样 21、一致 20、部分一致 1、consistency 95.2%、判定 CONCERN |
| Gate H | **FAIL** | H.1–H.5 PASS；H.6 文档↔outline 有 9 处 P0 偏离（D1–D9） |

> 说明：Gate D/E/H 的失败是**被审文档**未通过内容门，不是 review 过程未执行。因此 scan.md 标 DRAFT、STATE.md 标 NOT CONVERGED 是准确状态。

---

## 4. Doc Code Sync 七项

| 编号 | 检查 | 命令/证据 | 结果 |
|---|---|---|---|
| Sync-1 | 文档代码示例含 `static mut` | `rg "static mut" <doc>` → 0 命中 | ✅ |
| Sync-2 | 文档路径与仓库实际不一致 | `doc-code-map --check` → `handoff.rs` 缺失 | ❌ P0-fact-09 |
| Sync-3 | 虚构常量 | 抽查 `MULTIBOOT_CONSOLE_COLS/LINES`、`I386_VM_ADDR_MASK_4MB`、`PG_PAGETABLES`、`FALLBACK_BUMP_LEN` 均在 C/Rust 命中 | ✅（未发现虚构常量；但有 1 处“共用常量”错误 P0-fact-11） |
| Sync-4 | 文档内部待办未走流程 | `grep 'TODO'` → L1536 作者自问 | ❌ P1-docStyle-01 |
| Sync-5 | 符号锚点可解析性 | `anchor-resolve --check` → anchors=98 resolved=98 zero-def=0 | ✅ |
| Sync-6 | 路径缺 workspace 前缀 | `os/kernel/src/` 48 处；无裸 `kernel/src/` | ✅ |
| Sync-7 | 测试数量声称偏差 >50% | 文档 28/32 vs 实测 53/65 | ❌ P0-fact-07 |

---

## 5. 结构分析摘要（Gate D-6）

- **主题思想**：引导准备 = 把固件交给的原始材料整理成内核能接手的“一份账、一张表、一条交接载荷”。
- **目标读者**：具备分页/ELF 基础的中级读者；篇头已声明。
- **叙事主语**：Ch1 是 CPU/固件/自举链；Ch2 是 C 机制；Ch3/Ch4 是设计决策与落地。
- **驱动方向**：问题→C 证据→设计→实现→测试→诊断，不是函数目录。
- **核心概念**：四样东西、两条次序、两本账、加性恢复、版本化 ABI。
- **跨架构统一抽象**：先给“四样东西”的 OS 语义框架，再分 x86-64/aarch64/riscv64；ARM32 只作 C 对照；明确启动契约不强行统一。
- **双向闭环**：概念层闭环成立；事实层有 P0 断裂（生产路径、builder、Step 结构）。
- **叙事弧**：起（第一行代码之前）→承（C 四件事）→转（64 位换表达）→合（实现/测试/诊断），完整。
- **裸概念复述**：把函数名换成 `[XXX]` 仍可读；通过。
- **纵向链路**：四样东西→D1/D2/D4→§4.2/4.3/4.4→M1/M11/M13；两条次序→D5/D6→§4.6→M5/M6（M6 假覆盖）；交接→D4/D7→§4.5/4.8→M4/M14（不完整/标记错）。

> Gate D-6：**PASS（结构层）**。完整 12 节见 `structure.md`。

---

## 6. 覆盖率穷举（Gate A）

```text
Total C symbols: 1390
Semantic range C symbols: 192
Doc covered (semantic): 61/192 (31.8%)
Rust covered (semantic): 15/192 (7.8%)
有文档无Rust（语义真信号）：46
```

- 61 行语义列已逐条填写：核心映射、机制替换、他篇职责、名称假阳性、仅 C 对照。
- 46 个“有文档无 Rust”中没有发现“本篇语义范围内应有 Rust 而无”的新缺口；绝大多数是 ARM32 对照、`alloc_lowest` 有意未移植、VM 回收等他篇职责。
- 唯一实质性路径缺口是 §7.3 的 `handoff.rs`（P0-fact-09），已在关联代码清单中暴露。
- ARCH 标记：文档 3 处；仅 §3.7 的 FW_PAYLOAD 一条满足“文档 + 设计 + 代码”三处一致；§3.1/§3.7 的另外两条缺设计/代码同义标注（P1-process-01）。

---

## 7. Top 5 行为契约表（Gate B）

| # | 函数 | 输入契约 | 输出契约 | 副作用 | 错误/失败 | 时序 | 生命周期 | 架构演进 | 判定 |
|---|---|---|---|---|---|---|---|---|---|
| 1 | C `pre_init(magic, ebx)` → Rust `prepare_and_enter_kernel_image(bump_pages)` / `RiscvUbootBootShim::prepare_boot` | C：magic+ebx；Rust：bump 页数 | C：`&kinfo`；Rust：`!` 或 `BootPrepareResult` | C：填全局 kinfo、建表、开分页；Rust：装镜像、arena、EBS、建根、跳转 | C：assert panic；Rust：panic/fail-closed | C：assert→收账→建表→开分页；Rust：分配→装载→arena→配对 EBS→建 info→跳转 | C：kinfo 全局；Rust：跨镜像值传递 | 32→64、全局→arena | **doc↔code 偏离**（§4.2 仍写 prepare_boot 八步） |
| 2 | C `get_parameters(ebx,&kinfo)` → Rust `build_memmaps`/`ebs_pair::pair_and_classify` + `build_kernel_info` | C：mbi 指针；Rust：固件内存描述符 | C：填 kinfo 字段；Rust：`KernelInfo` 10 参 | C：memcpy、cmdline、模块、memmap；Rust：分类/归一化/构造 | C：assert；Rust：`Result`/panic | C：先收账后建表；Rust：先分类后构造 | C：全局；Rust：按值 + `.bss` 深拷 | 协议换代 | 基本一致；builder 参数漂移（P0-fact-04） |
| 3 | C `add_memmap/cut_memmap` → Rust `memmap.rs:add_memmap/cut_memmap` | C：`(start,end)`；Rust：`(start,len)` | 可用区间列表 | 改 `mmap_size`/`mem_high_phys` | C：assert/panic；Rust：`MemMapError` | 收账阶段 | 引导期 | 参数语义调整（C 不一致被修正） | **一致** |
| 4 | C `vm_enable_paging` → Rust `X86_64Paging::enable` | C：无；Rust：已建根 | 分页开启 | CR0/CR4/CR3/EFER | C：assert PE；Rust：panic on unsupported | PSE→PG→WP→PGE；Rust EFER.NXE→CR3→CR0→CR4 | 一次性 | 32→64、NXE 新增 | **一致** |
| 5 | C `alloc_lowest` → Rust 未移植（`install_at` + `smp_init`） | C：len；Rust：跳板安装 | C：最低可用基址；Rust：低内存拷贝 | 切 memmap | C：assert；Rust：安装断言 | SMP 启动前 | C：跳板生命周期；Rust：AP 启动 | 机制替换 | **一致（有意不移植）** |

---

## 8. 纵向链路与因果链抽样

### 8.1 纵向链路

| Ch1 概念 | Ch3 决策 | Ch4 实现 | Ch5 测试 | 链路 |
|---|---|---|---|---|
| 四样东西 | D1/D2/D4 | §4.2/4.3/4.4 | M1/M11/M13 | ✅（M13 部分） |
| 两条次序 | D5/D6 | §4.6 | M5/M6 | ⚠️ M6 假覆盖 |
| 交接契约 | D4/D7 | §4.5/4.8 | M4/M14 | ⚠️ M4 不完整、M14 标记错 |
| 分配器换手 | D8 | §4.7 | M7/M9 | ✅ |
| 两本账 | D4/D8 | §2.4.2/§4.7 | M2/M9 | ❌ §2.4.2 事实错 |

### 8.2 因果链抽样（8 条）

| # | 文档解释 | 因果链 | 判定 |
|---|---|---|---|
| 1 | §2.2 对调两条 push 会失败 | magic 收到 ebx（物理地址）→ 首条 assert `magic == 0x2BADB002` 失败 → panic，但 `do_serial_debug` 尚未置起，串口无声 | ✅ 成立 |
| 2 | §2.4.2 其余模块永不归还 | 内核 `kinfo.memmap` 不加回 → 但 VM `mem_add_total_pages` + `free_mem` 归还 → 结论“系统永久少掉”不成立 | ❌ P0-fact-01 |
| 3 | §2.5 去掉 `-1` 会误报相切 | A=[0x100000,0x200000)、B=[0x200000,0x300000)：`OVERLAP(A,B)` 检查 B.start 是否 < A.end（假）与 B.end-1 是否在 A 内（假）→ 不误报；加 `-1` 后检查 B.start-1=0x1FFFFF 在 A 内（真）→ 误报 | ✅ 成立 |
| 4 | §2.5 “真重叠两个方向都会报” | A 包含 B 时 `OVERLAP(A,B)` 两项均假，只有 `OVERLAP(B,A)` 真 → “两个方向都报”不成立 | ❌ P2-fact-02 |
| 5 | §2.6.1 反序 identity/mapkernel | 961 起先写内核项，后写恒等项覆盖为 `0xF0400000|…` → `call kmain` 的取指落到不含内核镜像的物理块 | ✅ 成立（“没有 DRAM”措辞过强，P2-fact-12） |
| 6 | §2.6.3 反序 `freepde_start` | `freepdes[0]=961+k-1` → 首次跨空间拷贝借 0 号槽 → 改写内核最后 4MB → `reload_cr3` 生效 → 下一次访问 fault 或读到别的进程 4MB | ✅ 成立 |
| 7 | §4.1 若把注册挪到 `arch_boot` 之后 | Step 0 兜底判据 `boot_alloc_region()==None` → 从 memmap 取 256 页 → boot-shim 的 1024 页空置 | 前半成立；后半“二次注册 release 静默改写”不成立（P0-fact-06） |
| 8 | §4.8 接收端不能重建根 | `new_from_page` 清零 512 项活动根 → 下一次取指 fault | ✅ 成立 |

---

## 9. Issue List

> 完整 115 条（含 P2）见 `scan.md` 的 `## Issue List`。本节给出 P0/P1 的完整条目与 P2 的分组摘要；每条 P0/P1 的证据命令均已在主会话重放。

### 9.1 P0（14 条，全部 open）

#### P0-fact-01：§2.4.2 把“内核账不收回”写成“系统永久少掉”

- **位置**：doc 683-689。
- **事实**：C 内核 `kinfo.memmap` 确实不再加回模块区间；但 VM 启动时 `mem_add_total_pages(len)` 补总量，`exec_bootproc` 后 `free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len))` 归还每个模块 blob。
- **证据**：`sed -n '482,520p' minix3/minix/servers/vm/main.c` → 注释“The kernel's freelist does not include boot-time modules; let the allocator know that the total memory is bigger.”、`mem_add_total_pages`、`free_mem`。
- **规则**：`review.md` 真值优先序；P0-fact。
- **修复方向**：把结论限定为“内核 `kinfo.memmap` 这本账不再收回”；补 VM 侧回收一段；把“代价”改为“VM 接手前不可用，之后由 VM 回收”；据此重写 687 行的对账公式。

#### P0-fact-02：生产 UEFI 路径被写成 `prepare_boot`

- **位置**：doc 1379、1977-1979、2001-2064。
- **事实**：`os/boot-shim/src/main.rs:65` 调 `prepare_and_enter_kernel_image(1024)`；x86_64 与 aarch64 共用它；`UefiBootShim::prepare_boot` 无生产调用方。
- **证据**：`sed -n '60,67p' os/boot-shim/src/main.rs`；`grep -rn 'UefiBootShim::prepare_boot' os` 仅注释。
- **规则**：P0-fact；Doc Code Sync。
- **修复方向**：§4.1 表、§4.2 全节按 v1 配对循环重写；§3.2 的“调用点”改为现状；把 `prepare_boot` 标注为辅助/历史形态；§6.1 同步。

#### P0-fact-03：`OpenSbiBootShim` 不存在，且“无调用方”为假

- **位置**：doc 1317、1381、1410、1427。
- **事实**：实际类型是 `RiscvUbootBootShim`（`opensbi_helpers.rs:211`），`prepare_boot(1024)` 由 `uboot_main.rs:333` 调用。
- **证据**：`grep -n 'pub struct.*BootShim' os/boot-shim/src/opensbi_helpers.rs`；`grep -n 'prepare_boot(1024)' os/boot-shim/src/uboot_main.rs`。
- **规则**：P0-fact。
- **修复方向**：改名；区分“riscv64 UEFI-shim 腿未接线”与“U-Boot shim 已接线”；同步 1315/1410/1427。

#### P0-fact-04：`build_kernel_info` 少写第 10 个参数

- **位置**：doc 2254-2288、2304。
- **事实**：实际签名 10 参，末参 `param_buf: &'static [u8]`；所有调用点传 normalizer 结果。
- **证据**：`sed -n '48,59p' os/boot-shim/src/lib.rs`。
- **规则**：P0-fact。
- **修复方向**：补第 10 参；删除 `param_buf: &[]` 与“加载选项解析未接线”的旧说法；§4.4 表改为“各腿 normalizer 输出”。

#### P0-fact-05：`reserved_regions` 的腿映射颠倒

- **位置**：doc 1490、2290、2295。
- **事实**：OpenSBI/riscv 直核调 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)`（非空）；aarch64 直核调 `DtbMemoryMap::new(&fdt, 0)`（空）；UEFI 生产走 `ebs_pair` 分类。
- **证据**：`grep -n 'DtbMemoryMap::new' os/boot-shim/src/opensbi_helpers.rs os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs` → `246:KERN_PHYS_BASE`、`193:KERN_PHYS_BASE`、`191:0`。
- **规则**：P0-fact。
- **修复方向**：按腿重写 reserved 取值与理由；删除“OpenSBI/直核传 `&[]`”。

#### P0-fact-06：`pt_alloc::register` 的 release 语义写反

- **位置**：doc 1999、2589。
- **事实**：release 下 `if already { return; }` 保留首个分配器；debug 下 `debug_assert!` 直接失败。不是“静默改写”。
- **证据**：`sed -n '87,120p' os/arch/src/arch/pt_alloc.rs`。
- **规则**：P0-fact。
- **修复方向**：重写两处反事实；区分 boot-shim 只调 `init_boot_pt_alloc`、注册在 `boot_validate_and_prepare` 内。

#### P0-fact-07：§5.2.1 的 cargo 输出不可复现

- **位置**：doc 2716-2724。
- **事实**：实测 `boot-shim --features test-all --lib` = 53 passed；`minix-boot` = 65 passed；`minix-elf` = 23 passed。文档写 28/32，且同篇 2756 行算术与实测一致。
- **证据**：主会话实跑 `cargo test -p boot-shim --features test-all --lib` → `53 passed`；`cargo test -p minix-boot -p minix-elf` → `65 passed` / `23 passed`。
- **规则**：L1 证据；Doc Code Sync。
- **修复方向**：用真实转录替换 28/32；保留 `test result:` 行与日期。

#### P0-fact-08：§6.1 两条 x86 里程碑不存在

- **位置**：doc 2889-2903。
- **事实**：`boot-shim: [raw] pt_alloc registered, entering kernel arch_boot` 与 `boot-shim: handing control to kernel arch_boot` 全仓零命中；生产 x86 路径的里程碑是 `[raw] arena=…` → `[raw] entering kernel image v1`。
- **证据**：`grep -rn 'pt_alloc registered, entering kernel arch_boot\|handing control to kernel arch_boot' os` → 0；`grep -n 'entering kernel image v1' os/boot-shim/src/uefi_helpers.rs` → 917。
- **规则**：P0-fact；锚点必须存在。
- **修复方向**：删除伪里程碑；x86 段按 `prepare_and_enter` 的真实输出重写。

#### P0-fact-09：§7.3 列出不存在的 `handoff.rs`

- **位置**：doc 3037。
- **事实**：实际文件是 `handoff_v1.rs`、`handoff_v1_read.rs`、`handoff_v1_write.rs`；§7.3 还漏了 `uboot_main.rs`、`ebs_pair.rs`、`ebs_map.rs`。
- **证据**：`ls os/libs/minix-boot/src/handoff*.rs`。
- **规则**：P0-fact。
- **修复方向**：改路径；补遗漏文件。

#### P0-fact-10：§4.6 Step 1 代码块过时、缺 Step 2c、x86 归因错

- **位置**：doc 2448-2472、2507、2513。
- **事实**：实际 `identity_device_windows()` 同时覆盖 aarch64 MMIO 与 riscv64 `kernel_identity_mmio_regions()`；分片常量是 `DEV_PIECE`；`PageSize` 类型。另有 Step 2c（`lib.rs:1043-1085`）把 x86 LAPIC/IOAPIC、riscv PLIC/UART 经 `device_window_regions` 装入自举根。
- **证据**：`grep -n 'DEV_PIECE\|identity_device_windows\|Step 2c\|device_window_regions' os/kernel/src/lib.rs` → 936/937/1043/1058。
- **规则**：P0-fact。
- **修复方向**：按当前 `lib.rs:936-958` 重写 Step 1；补 Step 2c；改 x86 归因。

#### P0-fact-11：`RESERVED_REGION_STORE_LEN=256` 不是 `vm_handoff` 共用常量

- **位置**：doc 2432。
- **事实**：`VM_BOOT_HANDOFF_MAX_IDENT = 64`；`vm_handoff` 返回 `[ZERO; 64]`，不引用 256。
- **证据**：`grep -n 'VM_BOOT_HANDOFF_MAX_IDENT' os/libs/minix-types/src/types/boot.rs` → `83:...=64`；`grep -n 'RESERVED_REGION_STORE_LEN' os/kernel/src/globals.rs` → 313。
- **规则**：P0-fact。
- **修复方向**：删除“共用常量”；说明 256 的独立来源（真机 probe × 余量）。

#### P0-fact-12：§6.3 大页阶梯 panic 字符串不存在

- **位置**：doc 2964。
- **事实**：实际是 `arch_boot: page-size policy: {e}`（`lib.rs:822`），`KernelMapError::NoKernelPageSize` 的 Display 是 `no kernel page size satisfies the layout`（`boot_policy.rs:51`）。
- **证据**：`grep -rn 'must be aligned to at least FALLBACK_HUGE_PAGE_SIZE' os` → 0。
- **规则**：P0-fact。
- **修复方向**：改为实际字符串；同步 02 篇旧引文（如存在）。

#### P0-test-missing-01：M6 是假覆盖

- **位置**：doc 2694、2758。
- **事实**：`paging.rs` 实际 16 条测试，0 条针对 `map_huge`；没有测试断言 `[0, IDENTITY_MAP_END)` 全覆盖、PA=VA、supervisor-only。
- **证据**：`grep -c '#\[test\]' os/arch/src/x86_64/paging.rs` → 16；`grep -n 'fn .*map_huge'` → 仅实现 1145。
- **规则**：P0-test-missing。
- **修复方向**：补记录型 mock 测试；或把 M6 降级为 TODO-05 并删除“已覆盖”表述。

#### P0-test-missing-02：M4 漏掉 v1 接收入口

- **位置**：doc 2692、2758；TODO-01。
- **事实**：`store_before_alloc_pin.rs` 只钉三个 `arch_boot` + `arch_boot_resume_high_half`；`arch_boot_resume_v1` 与三条 `resume_after_v1_decode` 未纳入。
- **证据**：`sed -n '32,67p' os/kernel/tests/store_before_alloc_pin.rs`；`grep -n 'arch_boot_resume_v1' os/kernel/src/lib.rs` → 504。
- **规则**：P0-test-missing。
- **修复方向**：把 v1 接收链纳入 pin；或在 TODO-01 明确“只覆盖 inline + aarch64 resume”。

### 9.2 P1（32 条）

#### 事实类（19 条）

| ID | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|
| P1-fact-01 | doc 1376 | cfg 计数 3+1；实际 5+1 | `grep -c 'cfg(feature' os/boot-shim/src/lib.rs` → 5 | 改为“五处 + 一处 cfg(all)” |
| P1-fact-02 | doc 1377 | 平台发现锚到 `arch/boot.rs`（实为 `CpuContextArch`/ELF 装载） | `grep -n 'pub trait' os/arch/src/arch/boot.rs` | 改指 `minix-platform` |
| P1-fact-03 | doc 1408, 1415 | `minix-boot` 只依赖 `minix-types`；实际另有 `fdt` | `sed -n '8,12p' os/libs/minix-boot/Cargo.toml` | 补 `fdt`；改“无实现”表述 |
| P1-fact-04 | doc 1790 | `tlbi alle1is`；实际 `vmalle1is`；`elr/vbar` 归因错 | `grep -n 'alle1is' os/arch/src/arm64/tlb.rs` | 改 `vmalle1is`；证据改指 `01-code-design.md:306` |
| P1-fact-05 | doc 1817-1835 | `register` 引用漏 release 守卫 | `sed -n '87,110p' os/arch/src/arch/pt_alloc.rs` | 补 `let already`/`if already` |
| P1-fact-06 | doc 1284 | 引用写 `build_memmaps()`；源码 `UefiMemoryMap.collect()` | `sed -n '241p' os/boot-shim/src/uefi_helpers.rs` | 改代码行；标注死路径 |
| P1-fact-07 | doc 2294-2295 | UEFI memmap 来源写 `UefiMemoryMap`；生产用 `ebs_pair` | `grep -n 'pair_and_classify'` → 845 | 改为配对循环分类 |
| P1-fact-08 | doc 2508 | `[SD19-gate]` 标记不存在 | `grep -rn 'SD19-gate' os/kernel/src/lib.rs` → 0 | 删除或补真实标记 |
| P1-fact-09 | doc 2582 | x86_64 “prepare_boot 返回后”设 bump 区间；实际在 `prepare_and_enter` 内 | `grep -n 'init_boot_pt_alloc'` → 765/339 | 按腿改写 |
| P1-fact-10 | doc 2648 | 复算命令 `KERN_VIRT_BASE` 不匹配 `KERNEL_*` | 实跑 grep 仅命中注释与 ENTRY | 改命令并贴输出 |
| P1-fact-11 | doc 2758 | `store_kernel_info` 4 条；实际 6 条 | `grep -n 'fn test_store_kernel_info'` → 6 | 改 6（1+5） |
| P1-fact-12 | doc 2774 | “唯一单元测试”；实际 4 条 | `grep -c '#\[test\]' uefi_helpers.rs` → 4 | 改“4 条之一” |
| P1-fact-13 | doc 2816, 2976 | aarch64 两条标记；实际 3 条 | `grep -n 'MARK3' test-shim-bootmarks-aarch64.sh` | 补第三标记与两形态 |
| P1-fact-14 | doc 2702, 2837 | x86 第三条写 `handing control`；实际 `entering kernel image v1` | `grep -rn 'handing control' os` → 0 | 改两处 |
| P1-fact-15 | doc 2824 | 称 cmd-smoke“只到调度器”；实际 Stage 3b 三条 | `sed -n '137,151p' test-cmd-smoke.sh` | 改写 |
| P1-fact-16 | doc 2992 | riscv `-d int` 格式与 QEMU 实际不符 | 实跑 QEMU → `riscv_cpu_do_interrupt: ...` | 改为实际格式 |
| P1-fact-17 | doc 2812-2824, 3048 | QEMU 覆盖图漏 4 个脚本 | `grep -n 'MARK' test-shim-bootmarks-x86.sh`；`run_all.sh` 无注册 | 补两行与四入口 |
| P1-fact-18 | doc 2703, 2968 | `assert_bootstrap_outside_memmap` 时序写成 EBS 前 | `grep -n 'assert_bootstrap_outside_memmap'` → 893-895 在 867 后 | 改为“prepare_boot 在 EBS 前；v1 在 EBS 后” |
| P1-fact-19 | doc 2777 | `boot_form` “逐形态断言”；实际同一夹具 5 次 | `sed -n '173,186p' boot_form.rs` | 改述或改测试 |
| P1-fact-20 | doc 2248 | 交换 `tty`/`mib` 的下标写错：原序 6=`tty`、7=`mib`；交换后 6=`mib`、7=`tty`，文档却说“位置 7 上的 BootModule 记的是 mib” | `sed -n '2248p' <doc>`；`grep -n 'BOOT_MODULE_PROC_NRS' -A14 os/kernel/src/proc.rs` | 改为“位置 6”，并说明位置 7 的 `tty` 被绑到 `mib` 进程号 |

#### 流程/文风（5 条）

| ID | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|
| P1-docStyle-01 | doc 1536 | 正文残留 `TODO 这个是TODO是交互式文档类TODO，我读不懂？？` | `grep -n 'TODO'` | 删除；若仍有疑点，写成可证伪的待确认项并登记 §5.3 |
| P1-docStyle-02 | doc §3.1/§3.2/§3.9/§4.7/§6.3 | 学生走查判定 5 个节为“只能靠结论”：§3.1 只给“GetMemoryMap 一次”；§3.2 两个 FileLoader 实现只展示一个；§3.9 的 pte_walk/DM 无代码；§4.7 `resume` 体被 `...` 挖空；§6.3 失败表多数行无锚点 | 学生走查逐节三态表 | 补关键代码/推演，或明写“只作对照” |
| P1-docStyle-03 | doc §2.6.1/§2.6.3/§3.2/§3.9/§5.4 | 自声明清单未兑现：§2.6.1 “五步逐函数解码”只解 3 步；§2.6.3/§3.9 “三个前提”只展开第一条；§3.2 “四条收益”只两条有证据；§5.4 “M1–M15 全部可观测”与 M5/M13 矛盾 | 学生走查清单兑现检查 | 逐项兑现或改标题/降级 |
| P1-docStyle-04 | doc 1315/1317/1427/1536/1594/1995/2137/2705/2824/2840 等 | 正文含过程编号（D-73/D-72/I-22/N-5/SD19/T1/T2）与高密度元话语/未定义术语；学生走查列 11 处过程编号、20+ 处元话语、7 处黑话 | 学生走查教学红旗表 | 编号移附录；删元话语；建术语表 |
| P1-process-01 | doc 1255, 1706 | 两个 `[ARCH]` 标记缺设计/代码同义标注 | `grep -n '\[ARCH' 01-code-design.md` → 仅 304；`grep -rn '引导协议从 Multiboot' os` → 0 | 补标注或降级 Rewrite |

#### 测试自身（4 条）

| ID | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|
| P1-test-01 | `os/kernel/src/lib.rs:5631` | riscv64 identity 测试用 `0x8000_0000`，与 `riscv64.ld:42` 的 `0x80200000` 矛盾 | `grep -n KERNEL_PHYS_BASE os/kernel-image/riscv64.ld` | 改夹具或删除纯字面量测试 |
| P1-test-02 | `os/kernel/src/lib.rs:5653-5737` | 三条 linker-script 测试不读 `.ld` | `sed -n '5653,5660p'` 无文件读取 | 改为读取 `.ld` 解析 |
| P1-test-03 | `os/boot-shim/src/loader.rs:421-443` | 名为 loader 的测试不调 `load_kernel_with_loader` | `sed -n '421,443p'` | 让测试走生产组合或改名 |
| P1-test-04 | `os/kernel/src/vm_handoff.rs:562-627` | 分类测试用影子 `classify_regions`，不调生产 `classify` | `grep -n 'fn classify'` → 563/219 | 构造 KernelInfo+allocator 调生产 |

#### 代码（3 条）

| ID | 位置 | 问题 | 证据 | 修复方向 |
|---|---|---|---|---|
| P1-code-01 | `os/boot-shim/src/uefi_helpers.rs:837-865` | 配对循环每轮重分配 map buffer，违反设计“失败后只复用原缓冲、零新分配”；重试泄漏 pool 缓冲 | `sed -n '836,846p'` | 移到 loop 外；未 EBS 时才扩容 |
| P1-code-02 | `os/boot-shim/src/uboot_main.rs:274-286` | U-Boot 表 `len` 写槽容量（kernel 2MiB、模块 1MiB），不是文件实际长度；>1MiB 静默截断，`BootModule.len` 失真 | `sed -n '274,286p'` | 从 ELF 头反推长度，或 boot.scr 写 filesize |
| P1-code-03 | `os/kernel/src/lib.rs:1165-1177` | 公开 `store_kernel_info` 截断后 `from_utf8_unchecked`；非 ASCII 可切开多字节 → UB | `grep -n 'from_utf8_unchecked'` → 1176 | 改 `from_utf8` + 拒绝，或入口 assert `is_ascii()` |

### 9.3 P2（64 条，分组摘要）

- **事实/表述（38 条）**：`arch=` 只写不读（P2-fact-01）、“真重叠两个方向都报”不成立（02）、漏 `prot_init` 整表重建（03）、`mmap_size` 术语不一致（04）、“三个 0”应为“五个”（05）、`I386_VM_ADDR_MASK` 误引 `pg_info`（06）、LIMIT 归因（07）、`virtual_copy`/`lin_lin_copy` 命名（08）、`pg_map` 漏 `mapped_pde` 约束（09）、VM 16 条截断（10）、`load_stack` 对齐依据（11）、“没有 DRAM”过强（12）、alloc_lowest 归因不全（13）、两本账口径（14）、块题注与内容不符（15/16）、ARM `thismod` 自由变量（17）、`user_end` 证据（18）、`minix_panicing` 锚点（19）、`boot_validate_and_prepare` 表述（20）、`KernelInfo` 用途（21）、`enter_kernel_image_v1` 引用（22）、DTB 是否 arena 载荷（23）、`memmap.rs` 注释称模块注释（24）、`arch_boot` 交接面（25）、“同一份镜像”前提（26）、N-5 探针待确认（27）、`serial_debug_baud`（28）、“GetMemoryMap 一次”（29）、M4 弱观测（30）、CI 可分辨（31）、check-layout L6/L7（32）、转录循环（33）、aarch64 `gic-version`（34）、v0–v3/v0–v4（35）、“两条入口共用 loading”（36）、排障提示（37）、`pg_identity` 覆写范围（38）。
- **代码（16 条）**：minix-elf 静默跳段（01）、loader 溢出/`filesz<=memsz`（02）、`vm_handoff` 除法/截断（03）、memmap debug 溢出（04）、opensbi 乘法未 checked（05）、DTB 原子只写不读（06）、DTB 8 区间静默丢弃（07）、`write_param_buf` 不清尾（08）、`free_upper_idx` 折 0（09）、`bootstrap_tree_candidates` O(n²)（10）、bump 四份实现（11）、`ebs_map` 过滤顺序（12）、builder 10 参/加法溢出（13）、`table_magic_ok` 未查对齐（14）、`in_memmap` 加法未 checked（15）、`param_landing` 二次 `&mut`（16）。
- **测试（13 条）**：空测试（01）、`split_huge` 局部变量（02）、同源探针（03）、`boot_form` 计数抓不到枚举扩展（04）、magic 测试不调生产（05）、`static mut` 测试（06，已降卫生项）、重复 `#[test]`（07）、脚本前置检查（08）、aarch64 FAIL 分支漏 MARK3（09）、`rfind` 未去注释（10）、文本 pin 脆弱（11/12）、check-layout 脆弱（13）。
- **文风卫生（2 条）**：`第 8 号条目` 触发 `doc-ref-check` 假阳性（工具误伤）；`P2-docStyle-02` 列出未逐行解码的 6 组代码块（alloc_pagetable/createpde/arch_post_init/BootHandoffV1/pt_alloc/BootAlloc/ARM head.S）。

---

## 10. 卓越性审查

### 10.1 文档侧

- **教学构件**：C/Rust 节选、调用树、三架构对照、失败诊断、反向推演均有；§2 的代码块逐行对比质量高，未发现常量或指令次序错误。
- **反向推演**：§2.2/2.3/2.4/2.5/2.6/2.7、§3.4/3.5/3.6/3.8/3.9、§4.1/4.2/4.5/4.6/4.7/4.8 均有具体后果；**§2.6.4 只有描述，没有“若不跳过 BIGPAGE 会怎样”的反向推演**（P2 卓越性）。
- **图**：§2.8 调用树、§4.1 阶段表；§3 章几乎无图，可补一张“三形态交接”图（P2 卓越性）。
- **文风**：`doc-style-lint` error=0；未发现黑话/压缩简写/文言文；未发现已判坏的比喻；L1536 残留作者自问（P1-docStyle-01）。
- **结构**：章级骨架合规；概念层闭环、叙事弧、裸概念测试通过。

### 10.2 代码侧（摘要）

- **强项**：`BootArch` 的关联类型绑定把“架构策略错配”变成编译期事实；`BootDmInstaller` 与 `HugePages` 的机制/策略分离；`handoff_v1` 的编译期布局断言；`BklProtected` 密封。
- **短板**：UEFI 双入口（`prepare_boot` 死路径）、`build_kernel_info` 10 参、`FileLoader` 丢底层错误、bump 四份实现、`map_huge` 冲突纪律未文档化。
- **死代码清单**：`UefiBootShim::prepare_boot`、`UefiMemoryMap`、`MultibootMemoryMap`、`PagingWithId`、`build_identity_windows` 空实现、`bootstrap_start/len` 回收死分支。
- **跨架构 §16.7**：`BootShim`/`FileLoader`/`HugePages`/`BootArch` 统一的是语义；`BootHandoffV1` 统一的是形状，但属启动契约例外；三形态不合并符合公理三。
- **Layer 2 代码判定**：PARTIAL；优先做死代码消除与 `BootPayload` 结构体。

### 10.3 学生型走查

> 方法：一个只读子代理，禁用外部 OS 知识，只凭本文回答 8 道机制题，并对 32 个分析/设计/诊断节做三态判定（①能自己推出 / ②只能靠结论 / ③文中没提）。

**闭卷问答**：8 题中 6 题可只凭本文答对；2 题需要外部知识：
- 第 2 题：`add_memmap` 取整反向后“会坏在哪里”的具体下游失败点，文档只给“会向外扩出一个不存在的页”，没有再往下推到分配/写入。
- 第 5 题：x86_64 为什么不能像 aarch64 那样内联执行，文档没有单独给出（只给了依赖方向与 aarch64 TTBR0 的理由）。

**逐节三态**：①=27 节，②=5 节，③=0 节。5 个“只能靠结论”的节是：
- §3.1：只给“`GetMemoryMap` 一次，产出两份列表”，没有 x86/aarch64 的实际分流过程，且与 §4.2 的配对循环冲突。
- §3.2：两个 `FileLoader` 实现只展示了 U-Boot 侧的 `UbootFileLoader`，UEFI 侧的 `impl FileLoader` 没有代码。
- §3.9：`pte_walk`、Direct Map 窗口建立、`inherit_supervisor_half` 都只有结论，三个前提如何逐条失效没有推演。
- §4.7：`BootAlloc::resume` 的函数体被 `...` 挖空，读者看不到它写哪些字段、以什么次序写。
- §6.3：失败模式表多数行只给“症状→信号”映射，没有代码/日志锚点；部分行用“类 panic”这类模糊指称。

**教学红旗**：
- 过程编号：D-73/D-72/I-22/N-5/SD19/T1/T2 等 11 处，首次出现未定义。
- 元话语：20+ 处“这里很重要/容易读漏/只取…即可”式导读，集中出现在 §1、§2.3、§2.5、§3.2、§4.1、§5.4。
- 未定义术语/黑话：7 处“腿/产线/链/形态/面”混用。
- 未逐行解码的代码块：`alloc_pagetable`、`createpde` 的 `pdeval`、`arch_post_init`、`BootHandoffV1` 20+ 字段、`pt_alloc`/`BootAlloc::alloc`、ARM `head.S`。
- 自声明清单未兑现：§2.6.1 “五步逐函数解码”只解 3 步；§2.6.3/§3.9 “三个前提”只展开第一条；§3.2 “四条收益”只两条有证据；§5.4 “M1–M15 全部可观测”与 M5/M13 矛盾。

**总评**：没有达到“第一次读的人能顺着它自己把机制想一遍”。C 侧 §2.1–§2.6 大体达到；Rust 设计/实现半本不合格，§3/§4 大量是决策记录与状态汇报；§5 的计数矛盾反而削弱了证据章的可信度。走查新增的问题已登记为 `P1-docStyle-02/03/04`、`P1-fact-20`、`P2-fact-38`、`P2-docStyle-02`。

---

## 11. 测试对账（Gate E）

### 11.1 计数与实跑

| 命令 | 实测 | 文档 | 判定 |
|---|---|---|---|
| `cargo test -p boot-shim --features test-all --lib` | `53 passed` | `28 passed` | ❌ P0-fact-07 |
| `cargo test -p minix-boot -p minix-elf` | `65 passed` / `23 passed` | `32 passed` | ❌ P0-fact-07 |
| `cargo test -p minix-kernel --features mock` | lib `857 passed / 8 ignored`；集成 3+2+1 | 分文件计数基本对 | ✅ |
| `cargo test -p minix-arch --features runtime-window --test pt_alloc_write_once --test arch_boot_exception_audit --test paging_encoding_pins` | `2 + 8 + 7 passed` | 2 条（pt_alloc） | ✅（其余未声明） |
| `check-layout.sh all` | `CHECK-LAYOUT=PASS（all）` | L0–L9 | ✅（L6/L7 描述过强，P2） |

### 11.2 QEMU 静态审计

| 脚本 | 实际标记 | 文档 | 判定 |
|---|---|---|---|
| `test-shim-bootmarks-x86.sh` | 5 条 | 未提 | ❌ P1-fact-17 |
| `test-shim-bootmarks-aarch64.sh` | 3 条 | 2 条 | ❌ P1-fact-13 |
| `test-cmd-smoke.sh` | Stage 3b 3 条，第三条 `entering kernel image v1` | `handing control`；又称“只到调度器” | ❌ P1-fact-14/15 |
| `test-riscv64-uboot.sh` | `PASS hello-boot-riscv64` | 一致 | ✅ |
| `test-riscv64-uboot-shim.sh` | 6 条 | 未提 | ❌ P1-fact-17 |
| `test-kernel-image-riscv64-bootface.sh` | B1/B2/B3 | 一致 | ✅ |

### 11.3 测试自身问题（五维）

- **完备性**：M4 v1 接收链缺；reserved 正向深拷/精确容量缺；`NoKernelPageSize`、`GetMapStatus`、reserved/platform 容量溢出缺。
- **自身正确性**：`boot_form` 同一夹具 5 次；riscv64 夹具 `0x8000_0000` vs `0x80200000`；三条 linker-script 测试不读 `.ld`；loader 测试不调 loader；`vm_handoff` 影子分类；`split_huge` 局部变量；huge-page 同源探针；空测试。
- **冗余**：重复 `#[test]`；`handoff_v1` 3 条重复编译期断言；identity/kernel mapping 两处重复。
- **无效**：空测试/弱测试；文本 pin 断言注释字符串。
- **虚构**：M6 假覆盖；28/32 转录；`uefi_helpers` “唯一”；`store_kernel_info` 4 vs 6；`boot_form` “逐形态”。

---

## 12. Rule Discovery

- **新模式一：死路径被当现役主线叙述**。案例：`UefiBootShim::prepare_boot` 无生产调用方，文档 §3.2/§4.2/§6.1 仍以它为主线。判定 P0-fact。建议规则：文档描述“某入口/函数”时必须给出“生产调用方”锚点；无调用方的函数只能以“辅助/历史形态”出现，其里程碑不得进入诊断主线。落地：`review-doc-checklist.md` §4。
- **新模式二：同篇测试转录与算术自相矛盾**。案例：§5.2.1 的 28/32 与同节末尾算术 53/65 并存。判定 P0-fact。建议规则：文档内任何 `cargo test` 转录必须与同篇计数表使用同一命令、同一日期；出现两个口径时以实测为准，并在 scan.md 记“转录漂移”。落地：`review-process.md` Step 4.5a。
- **新模式三：`[ARCH]` 标记只有文档侧**。案例：§3.1/§3.7 两个标记在设计文档与代码中无同义标注。判定 P1-process。建议规则：Gate H 增加“每个 `[ARCH]` 标记必须在设计文档与代码中各有一条同义标注”的机械检查。落地：`review-process.md` Gate H.5。

---

## 13. 轨迹复盘（基于本轮 review 的执行轨迹）

> 方法：重读 `scan.md`、`VERIFY-CHECK.md`、`trace/` 与四个章节块的原始证据，按“覆盖遗漏 / 结论缺陷 / 假阴性探针 / 过程缺陷”四项检查。发现缺口即补审，补审记录在本节。

### 13.1 覆盖遗漏检查

**逐节对照**：文档 62 个标题逐节核对，全部有检查记录或“无发现声明”。其中 §1.1–§1.4、§2.1–§2.8、§3.1–§3.10、§4.1–§4.9、§5.1–§5.4、§6.1–§6.4、§7.1–§7.5 均有对应审查表。

**逐文件对照**：41 个关联路径中 40 个存在；`handoff.rs` 缺失已列 P0-fact-09。未检查的文件：`os/boot-shim/src/ebs_map.rs`/`ebs_pair.rs` 由代码维度与测试审计覆盖；`os/kernel-image/src/bootface_common.rs` 未单独深审（它在 §4.8 的直核面语义内，但本轮未逐行核对 `be_cell/in_memmap/fdt_table_pa/fdt_bootargs/param_landing`）。**这是一处覆盖遗漏**，补审如下：

- `bootface_common.rs` 的 `be_cell` 大端读取、`in_memmap` 边界、`table_magic_ok` 魔数、`fdt_table_pa` 从 `/chosen` 取址、`param_landing`；与 `bootface.rs`/`bootface_a64.rs` 的调用一致。补审发现 3 个 P2 候选，已补入 `scan.md`：
  - `P2-code-14`：`table_magic_ok` 直接 `read_volatile::<u64>`，调用点只查 `in_memmap`，未查 8 字节对齐；`/chosen` 指向未对齐地址时 RISC-V 可能 trap。
  - `P2-code-15`：`in_memmap`/`in_memmap_range` 的 `r.base.0 + r.len` 未 checked，畸形 DTB 可溢出。
  - `P2-code-16`：`param_landing()` 返回 `&'static mut`，无一次性守卫；二次调用会构造两个 `&mut`。
  - 未发现新 P0/P1。补审记录写入 trace。

**逐门对照**：9 个 Gate 全部有证据块；7 个 Sync 项全部执行。无遗漏。

**逐模式族对照**：`review-patterns.md` §0（5 项）与文档族 1–15、代码族 16–29、测试族 35–40、叙事族 48–78、架构族 79–85 中，本轮适用项均已覆盖；未适用项（如并发族中与 boot 无关的 BKL 细节）在代码维度按 §4 抽样。

### 13.2 结论缺陷检查

- 抽样 21 条 issue 重放证据：20 条一致，1 条部分一致（P2-test-06 的“并行 UB”因 `RUST_TEST_THREADS=1` 不成立，已降为卫生项）。
- 检查“L3 推断写成 L1”：P0-fact-01/02/03/04/05/06/07/08/09/10/11/12 均有命令输出或源码直读；P0-test-missing-01/02 有 grep 计数与源码位置。未发现把推断当事实的 P0。
- 检查“引用过期快照”：本轮 Gate H 快照为 v3/v4 新生成；正文问题均以 C/Rust 现势为准，未用旧快照自证。
- 检查“用文档 §3 自证”：设计快照从 C+Rust 推导，未把文档 §3 当来源。
- 检查“未复现的 ✅”：Gate D 第 5 项、Gate E 的 M6 已明确标 FAIL，没有用“✅”掩盖。

### 13.3 假阴性探针

从“未被任何 issue 命中”的节中随机抽 3 节补审：

1. **§2.6.4（VM 继承规则）**：重读 `vm/pagetable.c:1282-1286` 与 `protect.c:321-366`；发现 `prot_init` 整表重建未在正文出现，已补为 P2-fact-03。探针命中，说明首轮有遗漏。
2. **§4.5（store_kernel_info）**：重读 `lib.rs:1117-1216` 与 `globals.rs`；发现 `reserved_regions` 正向深拷未测、精确容量边界未测，已补为 P1-test-gap（归入 §9.3 P2-test 组）与 P0-test-missing-02 的关联缺口。
3. **§6.2（panic 路径）**：重读 `main.rs:69-113` 与 `lib.rs:3165`；未发现新问题；panic 缓冲/UTF-8 回退/raw serial 与源码一致。
4. **学生型走查（第四个探针，独立子代理）**：只凭本文回答 8 题、逐节三态；结果 5 个节为“只能靠结论”（§3.1/§3.2/§3.9/§4.7/§6.3），4 处自声明清单未兑现，11 处过程编号、20+ 处元话语。补入 `P1-docStyle-02/03/04`、`P1-fact-20`、`P2-fact-38`、`P2-docStyle-02`。

> 结论：假阴性探针命中 3/4（§2.6.4、§4.5、学生走查），说明首轮覆盖不完美；已把命中项补入 Issue List。这也是本轮 review 需要复盘的直接证据。

### 13.4 过程缺陷与改进

- **子代理证据复验**：四个章节块 + 代码维度 + 测试审计共 6 个代理；主会话对全部 P0 与抽样 P1 重放了命令。发现 1 处代理结论过强（P2-test-06 的并行 UB），已降级。
- **未跑 QEMU**：真机标记只做静态审计；`§5.2.3` 的“已覆盖”不是本轮实测。报告中已如实标注。
- **未修**：本轮只审不修，P0 全部 open；状态 NOT CONVERGED 是准确状态。
- **规则/工具缺口**：`doc-ref-check.sh` 对“第 8 号条目”这类自然语言误报；`verify-check.py` 的 issue 提取为 115 条，与逐行计数一致。建议在 `review-process.md` 记录这两个工具限制，避免下一轮把误报当文档缺陷。

### 13.5 复盘结论

- **遗漏**：§2.6.4 的 `prot_init` 缺环、§4.5 的 reserved 正向测试缺口、学生走查的 5 个“只能靠结论”节与 4 处清单未兑现，均由假阴性探针补出。
- **缺陷**：首轮把“代理报告”当成“已验证证据”的风险已通过主会话重放控制；但 6 个代理的覆盖边界仍有 1 个文件（`bootface_common.rs`）未逐行审，已在 13.1 补审。
- **可改进**：下一轮应把“生产调用方锚点”和“测试转录唯一口径”写成生成期硬约束（Rule Discovery），并在 Gate H 增加 `[ARCH]` 三处标注的机械检查。

---

## 14. 确认清单与限制

### 14.1 Step 7 自检

- [x] 范围声明/执行模式/时间预算已输出。
- [x] STATE.md 已读取并更新（NOT_CONVERGED）。
- [x] 源码文件清单已输出（§0.1、scan.md Step 1）。
- [x] 覆盖率穷举已输出（SYMBOLS.md + `--check-filled` exit 0）。
- [x] Gate 0 九锚段齐全。
- [x] Gate A 证据块齐全。
- [x] Top 5 行为契约表（8 字段 × 5 函数）已输出。
- [x] Gate B/C/D/D-6/E/H 证据块齐全（D/E/H 文档级 FAIL）。
- [x] 链路验证、概念准确性、C 引用、数据结构、C 覆盖、文档风格表已输出。
- [x] Gate E 测试名 grep + cargo test 实跑。
- [x] Gate H H.1–H.6 已核（H.6 FAIL）。
- [x] 维度覆盖自检表已输出（§2）。
- [x] 最弱项自检：C 覆盖、链路、跨文档、错误路径、Design 对齐均已回答。
- [x] Layer 1/2 判定已输出（§0.3）。
- [x] Skill Invocation Log 已输出（scan.md）。
- [x] scan.md 单文件 + STATE.md + Open P0/P1/P2 同步。
- [x] Artifact Inventory 已输出（scan.md）。
- [x] Gate G VERIFY-CHECK 已生成（CONCERN，95.2%）。
- [x] Rule Discovery 已填写（§12）。
- [x] 修改项已生成（§9；本轮只审不修）。

### 14.2 限制

1. **同 agent 验证**：Gate G 由同一 session 的同一模型完成；已重放 21 条抽样的 grep/sed/cargo 命令，并在 VERIFY-CHECK 标注局限。consistency 95.2%。
2. **未跑 QEMU**：真机标记只做静态审计；`N-5` 探针读数未复跑（P2-fact-27 标待确认）。
3. **未修**：14 个 P0、32 个 P1、69 个 P2 全部 open；本文档不能标 CONVERGED。
4. **代理覆盖边界**：`bootface_common.rs` 在首轮未逐行审，已在 §13.1 补审；发现 3 个 P2 候选（P2-code-14/15/16），未发现新 P0/P1。
5. **工具误报**：`doc-ref-check.sh` 命中“第 8 号条目”是自然语言误伤；`verify-check.py` 的 issue 提取为 115 条，与逐行计数一致；无需修正。

### 14.3 修复优先级建议（若后续进入修复轮）

1. **先修 P0-fact-02/03/04/05/06**：生产路径、类型名、builder、reserved、pt_alloc——这五条决定读者对启动链的整张地图。
2. **再修 P0-fact-01/07/08/09/10/11/12**：C 回收结论、测试转录、里程碑、路径、Step 结构、常量、panic 字符串。
3. **补 P0-test-missing-01/02**：M6 恒等映射测试、M4 v1 接收链 pin。
4. **修 P1-code-01/02/03**：EBS 重试分配、U-Boot 表长度、`from_utf8_unchecked`——这三条是代码层最值得先动的。
5. **P1 事实类按章节批量修**：§3（cfg/依赖/平台锚点/tlbi）、§4（Step/时序/命令）、§5（计数/标记）、§7（路径）。
6. **P2 与卓越性**：按 `excellence.md` 的 a–h 逐项评估，不一次性全做。

### 14.4 产物清单

| 产物 | 路径 |
|---|---|
| 本报告 | `rewrite-notes/01-stage-kernel/01-review.md` |
| scan.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/scan.md` |
| STATE.md | `.review/zcode/01-stage-kernel/STATE.md` |
| SYMBOLS.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/SYMBOLS.md` |
| structure.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/structure.md` |
| excellence.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/excellence.md` |
| test-audit.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/test-audit.md` |
| design-structure.md | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/design-structure.md` |
| VERIFY-CHECK.md | `.review/zcode/01-stage-kernel/VERIFY-CHECK.md` |
| Gate H 快照 | `.design/01-outline.v4.md`、`01-outline-review.v4.md`、`01-design.v3.md` |
| trace | `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/trace/` |

---


---

## 附录 A：P2 完整清单（69 条）

> 本表是 `scan.md` Issue List 中 P2 的压缩视图；完整证据命令与修复方向见 `scan.md`。

| ID | 位置 | 一句话描述 |
|---|---|---|
| P2-fact-01 | doc 400 | “get_value/env_get 服务 arch=/board= 两个键”过泛；`arch=` 只写不读 |
| P2-fact-02 | doc 770 | “真重叠两个方向都会报”不成立（包含关系只有一个方向发现） |
| P2-fact-03 | doc 1052 | “自举页表两个下游”漏 `prot_init` 整表重建 |
| P2-fact-04 | doc 442 vs 492 | `mmap_size` “有效项数”与“遍历边界”不一致 |
| P2-fact-05 | doc 179 | “这三个 0”应为“五个 0” |
| P2-fact-06 | doc 1095 | `I386_VM_ADDR_MASK` 的“用到”列误引 `pg_info` |
| P2-fact-07 | doc 488 | LIMIT 归因只写 `phys_bytes`；源码注释是“rest of minix can't deal with bigger” |
| P2-fact-08 | doc 1044 | “virtual_copy 里借槽”实际在 `lin_lin_copy` |
| P2-fact-09 | doc 993 | `pg_map` 两条约束漏 `mapped_pde` 单调性 |
| P2-fact-10 | doc 691 | 未提 VM 只读前 16 条 memmap（`NR_MEMS`） |
| P2-fact-11 | doc 239 | `load_stack_start` 4096 对齐依据写成段对齐；实际依赖 head.o 是 unpaged 首项 |
| P2-fact-12 | doc 893 | “第一条指令地址/没有 DRAM”过强 |
| P2-fact-13 | doc 641 | alloc_lowest 未致故障只归因 `pg_alloc_page` 方向；漏 VM 分配方向与跳板生命周期 |
| P2-fact-14 | doc 687 | “总 DRAM”与“E820 可用总量”混用；`alloc_lowest` 不是每页累加 |
| P2-fact-15 | doc 956-972 | 块题注说含 `pg_map`，块内只有 `alloc_pagetable` |
| P2-fact-16 | doc 1016-1042 | 省略号归类不准，省掉了 `pdeval` 构造 |
| P2-fact-17 | doc 1195 | ARM `INRANGE` 上界用自由变量 `thismod` 未分析 |
| P2-fact-18 | doc 1478 | `user_end` 处置引 `USER_STACK_TOP`（那是 user_sp） |
| P2-fact-19 | doc 1477 | `minix_panicing` 落点写 boot-shim `fn panic`；实际是内核 `PANIC_DIAG_ACTIVE` |
| P2-fact-20 | doc 1686 | “只采集能力事实与校验恒等窗口”漏注册/兜底 |
| P2-fact-21 | doc 1496 | “KernelInfo 只用于直核腿与测试内核”不成立（UEFI 两侧也构造） |
| P2-fact-22 | doc 1735-1764 | `enter_kernel_image_v1` 引用把泛型实参写成 `AArch64BootArch`、删 `black_box`、未标 asm 只展示 aarch64 |
| P2-fact-23 | doc 1786 | 称 DTB 是 arena 五块载荷之一；实际 DTB 以平台源指针进入 |
| P2-fact-24 | doc 1471 | 把 `memmap.rs` 的函数文档称“模块注释” |
| P2-fact-25 | doc 1712 | “Rust 侧交接面是一个函数 arch_boot”；生产 UEFI 是 `enter_kernel_image_v1`/`arch_boot_resume_v1` |
| P2-fact-26 | doc 1959 | 共同前提“建表代码与运行时同处一份镜像”对跨镜像腿不成立 |
| P2-fact-27 | doc 1538 | N-5 探针读数无法静态复核 |
| P2-fact-28 | doc 1475-1476 | `serial_debug_baud` “由平台实现持有”；实际固定 115200 分频 |
| P2-fact-29 | doc 1265, 1315 | “GetMemoryMap 一次”对生产 v1 配对循环不成立 |
| P2-fact-30 | doc 2844 | 称 M4 只能弱观测；实际有源码序钉 |
| P2-fact-31 | doc 2822 | 称三条脚本退出码在 CI 可分辨；实际只有 uboot 脚本进 workflow |
| P2-fact-32 | doc 2803-2804 | check-layout L6/L7 描述强于实现（不读 ENTRY、不查方向） |
| P2-fact-33 | doc 2730-2754 | 转录循环列表与输出不符（不含 ebs_boundary_audit 却输出该行） |
| P2-fact-34 | doc 2985-2986 | 裸 aarch64 QEMU 命令缺 `gic-version=3` |
| P2-fact-35 | doc 2893 | 称 v0–v3 四个标记；实际 v0–v4 五个 |
| P2-fact-36 | doc 2879 | “loading kernel.elf from ESP… 两条入口共用”为假，仅 prepare_boot |
| P2-fact-37 | doc 2980 | 排障提示“看脚本打印出现/未出现两行”对 uboot/bootface 脚本不适用 |
| P2-fact-38 | doc 893 | “被覆盖的范围 961..961+k”不精确：`pg_identity` 写满 961..1023，有害的只是内核镜像那几项 |
| P2-code-01 | `os/libs/minix-elf/src/lib.rs:358-377` | 段越界/解析失败被 `continue` 静默跳过；与定长槽位叠加会“半装载成功” |
| P2-code-02 | `os/boot-shim/src/loader.rs:135-163` | `paddr+memsz` 未 checked；未校验 `filesz <= memsz` |
| P2-code-03 | `os/kernel/src/vm_handoff.rs:477-478` | `kern_size/4096 - text_pages` 未 checked，`as u32` 截断 |
| P2-code-04 | `os/kernel/src/memmap.rs:186,102` | 取整加法在 debug 先溢出 panic，release 保护才生效 |
| P2-code-05 | `os/boot-shim/src/opensbi_helpers.rs:440` | `num_pages*4096` 未 checked（UEFI 侧用了 checked_mul） |
| P2-code-06 | `os/libs/minix-boot/src/memmap_source.rs:39-44,129-130` | `DTB_FREE_N`/`DTB_RESERVED_N` 只写不读；`collect(&self)` 写 `static mut` 返回 `'static`，二次调用覆盖 |
| P2-code-07 | `os/libs/minix-boot/src/memmap_source.rs:148-165` | DTB max 8 区间超出静默丢弃，free/reserved 可能不对称 |
| P2-code-08 | `os/libs/minix-boot/src/handoff_v1_write.rs:254-262` | `write_param_buf` 二次短写不清尾 |
| P2-code-09 | `os/kernel/src/misc.rs:700-704` | `free_upper_idx=None` 在 ABI 出口折成 0 |
| P2-code-10 | `os/arch/src/boot_dm.rs:411-420` | `bootstrap_tree_candidates` 每次重扫 memmap，O(n²) |
| P2-code-11 | `os/kernel-image/src/bootface_a64.rs:119-140` | 第二份 bump 分配器；`uboot_main::ShimBump` 是第四份 |
| P2-code-12 | `os/boot-shim/src/ebs_map.rs:179` | `checked_len` 在过滤前执行，高位 reserved 可使整图 fail-closed |
| P2-code-13 | `os/boot-shim/src/lib.rs:48` | `build_kernel_info` 10 参 clippy `too_many_arguments`；`kern_stack_top` 加法无溢出检查 |
| P2-code-14 | `os/kernel-image/src/bootface_common.rs:85-92` | `table_magic_ok` 直接 `read_volatile::<u64>`，调用点只查 `in_memmap`，未查 8 字节对齐；DTB `/chosen` 指向未对齐地址时 RISC-V 可能 trap |
| P2-code-15 | `os/kernel-image/src/bootface_common.rs:68-81` | `in_memmap`/`in_memmap_range` 的 `r.base.0 + r.len as u64` 未 checked；畸形 DTB 可溢出 |
| P2-code-16 | `os/kernel-image/src/bootface_common.rs:122-129` | `param_landing()` 返回 `&'static mut`，无一次性守卫；二次调用会构造两个 `&mut`（UB） |
| P2-test-01 | `os/arch/src/x86_64/paging.rs:1672` | 空测试（零断言） |
| P2-test-02 | `os/arch/src/x86_64/paging.rs:1572` | `split_huge` 测试只算局部变量，不调用生产 |
| P2-test-03 | `os/arch/src/x86_64/paging.rs:1551` | huge-page 能力测试与实现同源探针，假绿 |
| P2-test-04 | `os/libs/minix-boot/src/boot_form.rs:93-100` | 计数测试抓不到“枚举新增但数组未改” |
| P2-test-05 | `os/boot-shim/src/opensbi_helpers.rs:599-615` | magic 测试不调用生产 `boot_file_table()` |
| P2-test-06 | `os/boot-shim/src/opensbi_helpers.rs:420-448,648-769` | 测试并行读写 `static mut BUMP_PTR`，无同步（UB） |
| P2-test-07 | `os/kernel/src/lib.rs:6046-6047` | 同一测试函数两个 `#[test]` 属性，计数多 1 |
| P2-test-08 | `os/qemu-tests/*.sh` | `fdisk`/`python3`/`strings` 未纳入 `command -v` 检查，缺件走 FAIL 而非 SKIP |
| P2-test-09 | `os/qemu-tests/test-shim-bootmarks-aarch64.sh:183-189` | FAIL 分支只回打 MARK1/2，漏 MARK3 |
| P2-test-10 | `os/boot-shim/tests/ebs_boundary_audit.rs:59-99` | `rfind` 在未去注释 body 上找最后出现；块注释未剥离 |
| P2-test-11 | `os/kernel/tests/store_before_alloc_pin.rs:18-66` | 文本 pin 可命中注释；`fn_body` 依赖列 0 `}` |
| P2-test-12 | `os/arch/tests/arch_boot_exception_audit.rs`、`paging_encoding_pins.rs` | 断言注释/字符串文本，逻辑坏了仍可假绿 |
| P2-test-13 | `os/kernel-image/check-layout.sh:205-260` | L8 依赖 legacy mangling；L5/L7 取最后 PT_LOAD；L6 不读 ENTRY |
| P2-docStyle-01 | doc 1536 之外 | 元话语/过程痕迹复扫未命中阻断项；`第 8 号条目` 触发 doc-ref-check 假阳性（工具限制） |
| P2-docStyle-02 | doc §2.6.2/§2.6.3/§2.6.4/§2.7/§3.4/§3.8 | 代码块未逐行解码：`alloc_pagetable`、`createpde` 的 `pdeval`、`arch_post_init`、`BootHandoffV1` 20+ 字段、`pt_alloc`/`BootA… |

> **第一轮最终状态**：Layer 1 FAIL；Layer 2 PARTIAL；**NOT CONVERGED**。本报告只审不修；修复已在第二轮执行，结论见文末“第二轮回归 Review”。

---

# 第二轮回归 Review（修复后，2026-11-02）

> **触发**：第一轮 review 的 P0/P1/P2 修复后，按用户要求做深度回归 full review。
> **方法**：两个只读回归代理（文档侧、代码/测试侧）逐条复核修复，主会话重放 P0/P1 证据并重跑门禁与测试。
> **基线**：文档 3,060 行，sha256 `ee4e9fc6a48e889fdbc62c185581fe517c7783c51792f2ec068afd8470dc7b5d`；代码改动 13 个文件（含 02 篇与 draft 的测试名同步）。

## R2-1 修复台账（第一轮 Issue → 处置）

| 组 | 修复内容 | 证据 | 状态 |
|---|---|---|---|
| P0-fact-01 | §2.4.2 改为“内核账不收回；VM `mem_add_total_pages` + `free_mem` 回收” | `minix3/minix/servers/vm/main.c:482-520` | **closed** |
| P0-fact-02 | §4.1 表、§4.2 全节改写为 `prepare_and_enter_kernel_image`；`prepare_boot` 标为辅助/无生产 caller | `os/boot-shim/src/main.rs:65`；`uefi_helpers.rs:759-919` | **closed** |
| P0-fact-03 | `OpenSbiBootShim` → `RiscvUbootBootShim`；区分 U-Boot 腿与未接线的 UEFI-shim 腿 | `opensbi_helpers.rs:224`；`uboot_main.rs:333` | **closed** |
| P0-fact-04 | `build_kernel_info` 补第 10 参 `param_buf`；字段表改为各腿 normalizer 输出 | `os/boot-shim/src/lib.rs:48-59` | **closed** |
| P0-fact-05 | `reserved_regions` 按腿：U-Boot/riscv 直核 `KERN_PHYS_BASE`，aarch64 直核 0，UEFI 配对循环 | `DtbMemoryMap::new` 三处调用 | **closed** |
| P0-fact-06 | `pt_alloc::register` release 改为“保留首个” | `pt_alloc.rs:96-101` | **closed** |
| P0-fact-07 | 测试转录 28/32 → 53/65 | 实跑 | **closed** |
| P0-fact-08 | §6.1 删除两条伪里程碑，改为生产 v1 里程碑 | `grep -F` 逐条命中 | **closed** |
| P0-fact-09 | §7.3 `handoff.rs` → `handoff_v1*`，补 `uboot_main`、`ebs_pair`、`ebs_map` 等 | `doc-code-map --check` 42/42 | **closed** |
| P0-fact-10 | §4.6 Step 1 重写（`DEV_PIECE`/`identity_device_windows`），补 Step 2c，改 x86 归因 | `os/kernel/src/lib.rs:918-1085` | **closed** |
| P0-fact-11 | 256 与 `vm_handoff` 的 64 拆开 | `VM_BOOT_HANDOFF_MAX_IDENT=64` | **closed** |
| P0-fact-12 | 大页阶梯 panic 字符串改为实际 `arch_boot: page-size policy: …` | `boot_policy.rs:51`；`lib.rs:822` | **closed** |
| P0-test-missing-01 | M6 改为“部分覆盖”；新增 `identity_piece_policy_covers_low_4gib_and_marks_device_windows`；`test_boot_flow_identity_and_kernel_map` 改用 `kernel_read_write()|EXECUTABLE` 并断言无 USER | `lib.rs:5341-5380`；实跑通过 | **closed（全量 4GiB 叶子映射仍待真机，TODO-05）** |
| P0-test-missing-02 | `store_before_alloc_pin` 增加 x86/riscv `resume_after_v1_decode`，共六入口 | `lib.rs:603-660`；实跑通过 | **closed** |
| P1 事实 19 条 | §3/§4/§5/§6/§7 的 cfg 计数、平台锚点、依赖、`tlbi vmalle1is`、register 引用、memmap 来源、`[SD19-gate]`、init 位置、grep 命令、测试计数/名称、QEMU 标记、EBS 时序等逐条修正 | 见 `scan.md` 第一轮 Issue List 与 R2 复核 | **closed** |
| P1-docStyle-01 | 删除正文作者 TODO | 全文 `grep TODO` 仅剩 §5 TODO 登记 | **closed** |
| P1-docStyle-02 | §3.1 换生产入口节选；§3.2 补 UEFI `FileLoader` 实现；§3.9 展开三个前提；§4.7 补 `resume` 函数体 | 文档与源码对照 | **部分 closed（§6.3 失败表仍以符号锚点为主）** |
| P1-docStyle-03 | 五步解码改为“三步解码”；三个前提逐条展开；四条收益标注 evidence；§5.4 补 M6/M5/M13 | 文档 | **closed** |
| P1-docStyle-04 | 移除 D-73/D-72/I-22/N-5/T1/T2/D-70 等过程编号；清理高频元话语 | 全文 `grep` 零命中（`MNXBOOT1` 除外） | **部分 closed（仍有少量导读语）** |
| P1-process-01 | `[ARCH]` 三处一致标注 | 未补设计/代码标注 | **open** |
| P1-test-01 | riscv64 identity 夹具改 `0x8020_0000` 与高半 virt | `lib.rs:5691` | **closed** |
| P1-test-02 | 三条 linker-script 测试改为读取 `os/kernel-image/*.ld` 并解析真实符号 | `lib.rs:5718-5770`；实跑通过 | **closed（回归中修正了首版读错文件的问题）** |
| P1-test-03 | loader 测试改名为与调用一致 | `loader.rs:433/449` | **closed** |
| P1-test-04 | `vm_handoff` 影子分类测试 | 未改 | **open（P1，测试仍用 `classify_regions` 影子函数）** |
| P1-code-01 | EBS 配对循环“每轮重分配” | 回归复核确认：外层循环只在 `NeedBiggerBuffer`（EBS 前）迭代，`INVALID_PARAMETER` 在 `pair_and_classify` 内复用缓冲 | **WITHDRAWN（误判）** |
| P1-code-02 | U-Boot 表长度改为按 staged ELF 的 `max(p_offset+p_filesz)` 写入；超出槽容量打印明确路标 | `uboot_main.rs:253-340`；riscv 交叉编译通过 | **部分 closed（mfs 17MB > 1MB 槽仍会截断；需要改 staging 布局）** |
| P1-code-03 | `store_kernel_info` 改 `from_utf8` + 最长合法前缀 | `lib.rs:1167-1184` | **closed** |
| P2 事实/表述 | 37 条中绝大多数已修（`arch=` 只写不读、`overlaps` 双向报、`prot_init` 第三下游、`mmap_size` 术语、五个 0、`pg_info` 误引、LIMIT 归因、`lin_lin_copy`、`pg_map` 第三约束、VM 16 条、对齐依据、961 覆写范围、两本账、题注、ARM `thismod`、`user_end`、`minix_panicing`、`boot_validate`、`KernelInfo` 用途、`black_box`、DTB 载荷、`arch_boot` 收敛点、`mmap` API、`serial_debug_baud`、GetMemoryMap、M4 弱观测、CI、L6/L7、转录、gic、v0–v4、loading 前缀、排障提示、`pg_identity` 范围） | 逐条见 `scan.md` | **closed** |
| P2 代码 | `minix-elf` 静默跳段、loader 溢出、vm_handoff 截断、memmap 饱和、opensbi 乘法、DTB 原子、DTB 截断、param 清尾、`ebs_map` 过滤、`table_magic` 对齐、`in_memmap` 溢出、`param_landing` 单次 | 13 个代码文件改动 | **部分 closed；`free_upper_idx` 折 0、bump 四份实现、`BootPayload`、`boot_dm` O(n²)、`minix-elf` strict 迭代器仍为 backlog** |
| P2 测试 | 空测试、`split_huge`、同源探针、`boot_form` 计数、magic 测试、`static mut`、重复 `#[test]`、脚本前置检查、FAIL 分支、`rfind`、文本 pin、check-layout 脆弱性 | 见 `scan.md` | **部分 closed（P2-test-01..13 中约半数已修，剩余为 backlog）** |

## R2-2 回归新发现问题与处置

| ID | 级别 | 位置 | 描述 | 处置 |
|---|---|---|---|---|
| R2-01 | P1 | §3.1 | 代码块仍以 `fn prepare_boot` 代表 UEFI 路径 | **已修**：改写为 `prepare_and_enter_kernel_image` 的生产节选，并标注 `prepare_boot` 为辅助形态 |
| R2-02 | P1 | §3.7 | “riscv64 的 UEFI-shim 腿落地后”与“未接线”矛盾 | **已修**：改为“U-Boot shim 腿落地后” |
| R2-03 | P1 | §5.4 | M6 未列入“部分覆盖” | **已修**：改为 M5/M6/M13 |
| R2-04 | P1 | §6.1 | `boot-shim: param_buf bytes=… first=…` 前缀不存在 | **已修**：改为 `  param_buf …`（两个前导空格） |
| R2-05 | P2 | §4.2 | 步表 7/8 顺序与代码相反 | **已修**：7 复核、8 构造 |
| R2-06 | P2 | §4.2 | 节选头注未说明省略语句 | **已修**：头注改为“注释与部分语句有删节” |
| R2-07 | P2 | §4.1 | 非 UEFI 列混淆 bootface 与 U-Boot shim | **已修**：加脚注区分两条腿 |
| R2-08 | P2 | §7.3/§4.4 | 漏 `uboot_main`/`riscv64-uboot.ld`；memmap API 写错 | **已修**：补路径；改 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect()` |
| R2-09 | P2 | §4.6/§6.1 | “编译期断言”“无标记”“只在有料时出声”不准确 | **已修**：改为运行期断言/无额外行为标记/区分两条诊断 |
| R2-10 | P2 | §5.2.1/§5.2.2 | 循环列表漏 `ebs_boundary_audit`；`build_memmaps` 调用点描述错 | **已修**：补列表；改为辅助形态经 `UefiMemoryMap::collect` |
| R2-11 | P2 | §5.2.3 | “三条链/三个脚本”与五条不符；MARK3 时序 | **已修**：改为五条/五个；写明 MARK3 在 EBS 后 |
| R2-12 | P2 | §3.2/§7.5/§6.3 | “入口只有一个”；§7.5 旧链；预算示例 `prepare_boot(N)` | **已修**：限定 trait 入口；改生产链；改预算函数名 |
| R2-13 | P2 | L858/L1043 | “于是8 号”“在里两个方向”错字 | **已修** |
| R2-14 | P2 | `main.rs:3` | 注释仍称调用 `prepare_boot` | **已修** |
| C2-1 | P0-test-missing 候选 | `lib.rs` linker 测试 | 首版 `include_str!` 读了无消费者的 `os/kernel/src/arch/*/link.ld`，符号名也错 | **已修**：改读 `os/kernel-image/*.ld`，x86 用 `KERNEL_HIGH_BASE+KERNEL_PHYS_BASE`，其余用 `KERNEL_VIRT_BASE`，并断言 `ALIGN(0x200000)` |
| C2-2 | P1 | 02/draft | 测试改名后旧名残留 | **已修** |
| C2-3 | P1 | 01 §3.2/§4.3/§4.5 | 代码摘录仍是旧实现（`to_vec`、`from_utf8_unchecked`） | **已修**：同步修剪与 `from_utf8` 回退 |
| C2-4 | P1 | `uboot_main.rs` | U-Boot 表长度写槽容量；mfs 17MB > 1MB 槽会静默截断 | **部分修**：按 staged ELF 实际长度写入，超槽打印明确路标；真正支持 mfs 需改 staging 布局（backlog） |
| C2-5 | P2 | `vm_handoff.rs` | `kern_data_pages` 的 `unwrap_or(0)` 把不变量破裂折成 RWX 哨兵 | **已修**：`checked_sub` + `expect`，text 也 `try_from` |
| C2-6 | P2 | `memmap.rs` | 饱和加后溢出分支成死代码 | **已修**：删除死分支并注释顶端语义 |
| C2-7 | P2 | `bootface_common.rs` | 未对齐表的诊断落到通用 “no BootFileTable” | **open（P2）**：建议返回区分性错误/路标 |
| C2-8 | P2 | 多文件 | 新分支缺测试（`filesz>memsz`、修剪、非 ASCII 截断、bump 耗尽、param 二次短写） | **open（P2 backlog）** |
| C2-9 | P2 | `lib.rs` | `DEV_PIECE` 测试字面量 | **已修**：提为模块常量，测试引用 |

## R2-3 修复后门禁与测试

| 门/测试 | 命令 | 结果 |
|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check` | `anchors=95 resolved=95 zero-def=0 multi-def=0` |
| 章节引用 | `AWK=gawk tools/doc-ref-check.sh --check` | 全部可解析 |
| 文风 | `tools/doc-style-lint.sh` | error=0 |
| 承载门 | `tools/doc-code-density-check.sh --check` | 4 个可判章通过 |
| 代码清单 | `tools/doc-code-map.sh --check` | 42/42 通过 |
| boot-shim | `cargo test -p boot-shim --features test-all` | 53 + 5 passed / 0 failed |
| minix-boot / minix-elf | `cargo test -p minix-boot -p minix-elf` | 65 + 23 passed / 0 failed |
| minix-kernel | `cargo test -p minix-kernel --features mock` | 856 passed / 0 failed / 8 ignored；集成 3+2+1 |
| minix-arch | `cargo test -p minix-arch --features runtime-window` | 各 target 全绿（约 376 passed） |
| riscv shim | `cargo check -p boot-shim --target riscv64gc-unknown-none-elf --no-default-features --features fw-riscv-uboot --bin riscv64-uboot-shim` | 通过 |
| kernel-image riscv | `cargo check -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none` | 通过 |
| kernel-image aarch64 | `RUSTFLAGS='-C target-feature=-neon,-fp-armv8' cargo check -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none` | 通过 |

## R2-4 残余与判定

- **P0：0 open**。第一轮 14 个 P0 全部闭环；回归中 C2-1 的测试读错文件已修。
- **P1：4 open**：
  1. `P1-test-04`：`vm_handoff` 分类测试仍用影子 `classify_regions`，未直接调生产 `classify`。
  2. `P1-process-01`：两个 `[ARCH]` 标记仍缺设计/代码同义标注。
  3. `P1-code-02` 残余：U-Boot 模块槽 1 MiB 容不下 17 MB 的 mfs，已改为显式路标 + 截断，真正支持需改 staging 布局。
  4. `P1-docStyle-02/04` 残余：§6.3 失败表以符号锚点为主、少量导读语未清。
- **P2 backlog**：`free_upper_idx` ABI 折 0、bump 四份实现、`BootPayload` 结构体、`boot_dm` O(n²)、`minix-elf` strict 迭代器、`C2-7` 未对齐诊断、`C2-8` 新分支测试缺口等。
- **Layer 1**：**PASS**（P0=0；所有 P0 有修复与验证）。
- **Layer 2**：**PARTIAL**（文档讲法改善，但 §6.3 与部分设计节仍可加强；代码设计级 P2 未全做）。
- **最终状态**：**CONVERGED with warning / backlog**。严格按 Step 7.1 的“本轮新增 P1 ≤ 1”口径，回归轮新增了 4 个 P1（R2-01..04，均已修）与 C2-1..C2-4，因此仍建议下一轮清完上述 4 个 P1 后再宣布完全 CONVERGED。当前不再有 P0 阻断。

> **修复边界**：本轮修的是“文档事实错误、代码安全/错误处理、测试虚构与误判”。设计级重构（`BootPayload`、bump 统一、`free_upper_idx` ABI、`boot_dm` 性能）属于 `code-excellence` 的射程，未在本轮强行改，已登记为 backlog。
