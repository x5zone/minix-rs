# DOC-ITER-LEDGER — 文档迭代工作流状态台账（入库的最小可恢复记录）

> 规格：`prompt/DOC-ITER-WORKFLOW.md`（本台账是 §五·5.1 定义的最小记录；完整轮次包与过程
> 产物在 `tmp/doc-iter/`，不入库）。
> 恢复现场的方法：读本表定位目标文档、基线/结果 commit、workflow 版本与包路径，再取包文件。

## 轮次账

| 篇 | 轮 | 基线 commit | 结果 commit | workflow | 审阅状态 | 轮次包（tmp/） |
|---|---|---|---|---|---|---|
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 01 | `7468a9e39` | `227028e23` | v0.1 | 已审（GPT 反馈已回收，见 Round 02 §1.x） | `tmp/doc-iter/01-boot-shim-bootstrap/round-01.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 02 | `227028e23` | `5d0f99e69` | v0.2 | 已审（反馈回收进 Round 03） | `tmp/doc-iter/01-boot-shim-bootstrap/round-02.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 03 | `5d0f99e69` | `f7a50edc5` | v0.3 | 已审（反馈回收进 Round 04） | `tmp/doc-iter/01-boot-shim-bootstrap/round-03.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 04 | `480b09fa4` | `4942c40f7` | v0.4 | 待送审 | `tmp/doc-iter/01-boot-shim-bootstrap/round-04.md`（含缺陷闭环台账 `defect-ledger.md`） |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 05 | `3566785d1` | `668436acc`（+`88666206c` 评审收尾） | v0.4+R05附加 | **已审**（收官轮：Ch5 重写 + 固定件 + C1–C13；评审收尾=§5.3 排序修正） | `tmp/doc-iter/01-boot-shim-bootstrap/round-05.md`（归因包）+ `cold-read-package-r05.md`（冷读包，分投） |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 06 | `88666206c` | `88666206c`（本轮 0 改动） | v0.5+R06回归 | 待送审（回归轮：0 回退〔61/66 重放〕+ 0 新缺陷 + C24 16 样本 0/16〔累计 3/46 带内〕+ C37 3 量 + C33/C18 复核；**纯验证轮**） | `tmp/doc-iter/01-boot-shim-bootstrap/round-06.md`（回归包；§7 C1–C37 全内联） |
| `rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md` | 01 | `3e31894db` | `7abc06623`（+`b1727412f` 重放收尾） | v0.5+R06附加 | 待送审（换篇首轮；**不构成形态检验**，G-C 仍待无 C 源篇） | `tmp/doc-iter/06-proc-init-boot-proc/round-01-06-proc.md`（归因包）+ `cold-read-package-r06.md`（冷读包） |
| `rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md` | 02 | `b1727412f` | `05309612a` | v0.5+R02附加 | 待送审（闭环轮：三项结构残余 + C18 Ch5 改造 + 回读 13 处修复；**判冻结**） | `tmp/doc-iter/06-proc-init-boot-proc/round-02-06-proc.md`（归因包；diff 与台账均已内联） |
| `rewrite-notes/01-stage-kernel/13-syscall-dispatch.md` | 01 | `dd9c6c1fd` | `b413b7f00` | v0.5+R01-13附加 | 待送审（首轮：两条 P0 定案 + 51 条修复 + C18/C24/C25/C26/C28 落地；**不判冻结，拟走第二轮**） | `tmp/doc-iter/13-syscall-dispatch/round-01-r13.md`（归因包；diff/台账/§7 全内联） |
| `rewrite-notes/01-stage-kernel/13-syscall-dispatch.md` | 02 | `b413b7f00` | `94d312d43` | v0.5+R02-13附加 | **已审·同意冻结**（篇级；一处残留经复核为合法技术用语「RAX 回读」，无需改） | `tmp/doc-iter/13-syscall-dispatch/round-02-r13.md`（归因包；diff/台账/§7 全内联） |
| `rewrite-notes/01-stage-kernel/12-ipc-core.md` | 01 | `6ab01cbf8` | `af14a09da` | v0.5+R01-第四篇附加（预填） | 待送审（首轮：三笔 P0 级定案 + SRC-17~20 + C18 反转表 M1–M13；**判否，建议一轮闭环**） | `tmp/doc-iter/12-ipc-core/round-01-r12.md`（归因包；diff/台账/§7 C1–C32 全内联） |
| `rewrite-notes/01-stage-kernel/12-ipc-core.md` | 02 | `af14a09da` | `a2b0338b3` | v0.5+R02-12附加 | **已审·同意冻结**（篇级；裁决 E/F/G 更新、0/30 归因采纳、轴①改"落地完整度"） | `tmp/doc-iter/12-ipc-core/round-02-r12.md`（归因包；diff/台账/§7 C1–C35 全内联） |
| `rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` | 01 | `dc8322d54` | `31f21ce80` | v0.5+R01-05附加（换篇首轮） | **已审·同意判否**（首轮：4 笔 P0-fact + 121 处修复；C24 灵敏度获证；轴②跨端口新档） | `tmp/doc-iter/05-clock-interrupt-init/round-01-r05.md`（归因包；diff 60 hunks/台账/§7 C1–C36 全内联） |
| `rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` | 02 | `31f21ce80` | `dfaf8eb58` | v0.5+R02-05附加（闭环轮） | 待送审（回归扫 40/40+C18 表 M1–M18（未覆盖 5）+C24 16 样本 1 新命中/0 误报+C29 3/3/1/100%+C37 2 失联加注；**判可冻结**） | `tmp/doc-iter/05-clock-interrupt-init/round-02-r05.md`（归因包；diff 5 hunks/台账 F 节/§7 C1–C37 全内联） |
| `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 01 | `dc8322d54` | `a73ad0b99` | v0.5+R01-02附加（换篇首轮） | 待送审（首轮：2 笔 P0 级 + 63 处修复 + C18 反转表 M1–M12 + C24 12 样本 1 命中/0 误报 + C29 40/40/0/100%；轴①仍零覆盖；**判否，建议一轮闭环**） | `tmp/doc-iter/02-higher-half-kernel/round-01-r02.md`（归因包；diff 35 hunks/台账/§7 C1–C37 全内联） |
| `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 02 | `7952e4db7` | ``985ce8780`` | v0.5+R02-02附加（闭环轮） | 待送审（闭环轮：回归扫 35/35 并暴露 R01 冷读漏处置 20 条〔全部落地〕+ C24 15 样本 1 独有命中〔riscv 载体 PC 口径〕+ C38 首跑〔建议入 S5〕+ C29 68/68/1；**判可冻结（篇级）**） | `tmp/doc-iter/02-higher-half-kernel/round-02-r02.md`（归因包；diff 21 hunks/台账 F46–F71/§7 C1–C38 全内联） |
| `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 01 | `66d616452` | `4854219bf` | v0.5+R01-03附加（换篇首轮·v7） | 待送审（首轮：五路装置全开→69 缺陷行/63 修复〔含 2 处 P1：Tss64 字段行、riscv64 两腿〕+ §5.4 反转表新建 + S0' 首跑〔G3=1：02 篇 L-B 入冻结队列〕+ C24 13 样本 1 独有 + C38 独有 0〔触发 R 重评〕+ C29 88/88/2；**判否，建议闭环轮**；**裁决 P：仍 01-stage-kernel，轴⑤零覆盖**） | `tmp/doc-iter/03-kmain-cstart/round-01-r03.md`（归因包；diff 46 hunks/台账 F01–F72/§7 含 S0' 表全内联） |

## 规则变更裁决记录

| 日期 | 版本 | 变更 | 来源 | 状态 |
|---|---|---|---|---|
| 2026-10-13 | v0.1 | 建立（见 v0.1 变更记录） | 用户裁决 + GPT 对话 | 已生效 |
| 2026-10-13 | v0.2 | S1 证据盘点 / S4 盲测判分三分 / E7 三级化 + C1 澄清 / §八 归因记录 / §九 三道理门 / F5 / 包与台账 | Round 01 实证 + 外部评审（用户裁决） | 已生效 |
| 2026-10-14 | v0.3 | S1.5 关键因果主张 / S1.6 运行阶段边界 / S2.1-6 职责漂移+机械对账 / V5 分阶段冷读 / §八 四类归因 / S5 发布扫描+台账重放 | Round 02 实证 + 外部评审（用户裁决"继续"） | 已生效 |
| 2026-10-13 | v0.5 | 换篇前五项升级（W1 双包分投 / W3 覆盖矩阵入包 / W5 阴性清单入包 / W7 冻结属性框架 / W8 冷读含修改段复检）+ W2(b) §4b 收窄触发 + §7 收集节入模板；W4 暂缓 | R05 评审（用户裁决继续） | 已生效（`3e31894db`） |
| 2026-10-13 | v0.4 补充 | S5 发布扫描追加「文档头固定件在场」；轮次包分投（冷读包/归因包）+【§3 起点/终点】标记 + §7 收集节；冷读协议两增（错误自述 + 低置信配额，原始输出未合并入 §7） | R04 外部评审的「R05 附加收集要求」（用户裁决继续执行 R5） | 已生效（三处执行项）；其余收集项待 v0.5 裁决 |
| 2026-10-13 | v0.4 | S1.0 证据分级（E1–E5）+ S1.7 主张—依据—适用范围表 + S2.1.7 跨节因果一致性检查 / V5 重构为冷读团（A/B/C 全文冷读、材料隔离、7 字段、附带发现） / 缺陷闭环台账（六字段、未发现≠发现未修） / 修复前五类归因 + 不以票数裁决 + 冷读边际价值指标 / S4.7 变更闭环检查 / 轮次类型（优化轮/闭环验证轮） / 包 §4b 源码证据节 | Round 03 实证 + 外部评审（冷读子 agent 提案、缺陷闭环、证据类型分离）+ 用户裁决"效果最大化" | 已生效 |
| — | — | 候选规则 ③（行号引用检查）、④（对账表集合守恒） | Round 01 | 登记待工具排期 |
| — | — | 入口 A 完整重做回流 | v0.2 裁决 | 待入口 B 在 2~3 篇不同形态文档上稳定 |

### 冻结篇目内容修复队列（裁决 N（b）：接触该篇任一动作时顺手修，或由用户指定窗口）

| 篇 | 缺陷 | 发现轮次 |
|---|---|---|
| 01（已冻结） | §5.1 反转表 M10 行"L0–L9 **十一条**断言"三元错值——L0–L9 实为 10 个主编号、现脚本 13 条（x86_64）/14 条（riscv64）、01 篇自表 10 行 | 02 篇 R01 C33 发现，R02 复核仍成立 |
| 02（已判可冻结） | 冷读 A 低置信第 2 条（L-B）：trait 契约（§3.4 四条前置条件）未承载"以什么执行视图进入"这条按架构不同的隐含前提 | 03 篇 R01 S0' 清点清出（G3=1） |

## 跨范围待办（不属于文档迭代的提交，登记防丢）

- `os/kernel/src/lib.rs` Step 1 注释的 `sstatus.SUM` 说法与 RISC-V 规范相反（规范：
  `Irrespective of SUM, the supervisor may not execute code on pages with U=1`；SUM 只管 load/store）。
  01 篇文档已按规范陈述并在 §4.6 加注；**源码注释本身待另开一笔修正**（Round 01 discovery）。
- `os/boot-shim/src/uefi_helpers.rs` 的两条注释互相冲突：D-64②（快照顺序）说模块占页经
  `allocate_pages`、kernel 段也在"全部 LOADER_DATA 分配"之列；F8 更正（真机 d1a）说 kernel 段与
  boot 模块段"直写 conventional RAM（不经 UEFI 分配）"。代码事实：kernel 段直写（`load_segments_into_phys_memory`），
  模块段经 `alloc_module_pages`（`allocate_pages(LOADER_DATA)`）。文档侧已按代码事实修正（Round 02），
  **源码注释的措辞待另开一笔统一**。
- `os/boot-shim/src/uefi_helpers.rs:238-239` 的注释引用 "01-boot-shim-bootstrap.md §3.5.1"，
  文档无该小节（内容在 §3.5）；源码指针待改（Round 02 V3-lite discovery）。
- **切片读坏的机制归因未定案**：`os/kernel/src/globals.rs`（`.bss` 落地区注释）与 `os/kernel/src/lib.rs`
  的相关注释把"切片源页读成零"解释为"内核自己的页表不重放这些堆页的 1:1 映射"；但真机读数
  "15 条 conventional 在建立 Direct Map 覆盖时（切表之后）还完好、到 classify 才全零"与该解释相抵，
  更像"页被当空闲复用后清零"。01 篇文档已改为按读数陈述、不选边；**源码注释待取证定案后统一**
  （Round 03 V3-lite discovery）。
- `os/boot-shim/src/main.rs` 的预算注释称 1024 页池"costs every boot 4 MiB of VM-selectable memory"，
  而 `os/kernel/src/vm_handoff.rs:160-168` 明写 UEFI 路径下 bump 区不在 conventional 列表、切割是
  no-op；两处注释冲突，待统一（Round 03 V3-lite discovery）。
- `os/kernel/src/lib.rs` Step 1 的 "Ignore errors — some ranges may not be backed by physical memory,
  but that's fine; the CPU won't access them." 注释与 `[SD19-gate]` 标记：该理由与 `map_huge` 实际
  失败类（`alloc_pt_page()?` 分配失败 / `AlreadyMapped`）不闭合——映射路径不检查物理内存是否存在；
  01 篇文档已按机制分层陈述，**源码注释待另开一笔修订**（Round 04 A13 discovery）。
- 01 篇构建侧残留（文档侧已按效果陈述，规则出处待核查）：Minix3 内核构建的栈对齐假设
  （`-mpreferred-stack-boundary` 旗标或 `pre_init` 反汇编）无源码证据（Round 04 C7 discovery）。
- `os/kernel/src/vm_handoff.rs` 模块级文档的表头行「reserved module blobs | KernelInfo.boot_modules()
  minus the VM module (reclaimed after ELF copy)」与实现矛盾——代码处已改为全量保留（含 VM 自己，
  成修注释说明原因）；**表头待更新**（Round 05 C2 discovery，文档侧已注明「表头注释的旧口径」）。
- Rust 侧模块分配（UEFI `AnyPages`）与内核段落位（直写固定物理地址）之间的重叠防护**未找到对应机制**：
  C 侧由 `overlaps`（模块+内核当 N+1）挡；Rust 侧 `loader.rs:first_overlapping_pair` 只查内核自身
  PT_LOAD 段，且内核段在模块分配之前直写（`load_kernel_with_loader` 立即写入）。真机未复现，待核实
  （Round 05 C附带4 discovery）。
