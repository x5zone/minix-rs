# 01-boot-shim-bootstrap Round 17 meta-review（冷启动）

- 评审对象：`runs/01-stage-kernel/01-boot-shim-bootstrap/` 的 R17 段（`loop.md:816-862`、`defect-ledger.md:1199-1274`、`metrics.md`）、七份面板原始输出 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round17/`。
- 时间线（`git log`、`stat` 重放）：冻结提交 `ec67e3d4f` 2026-11-09 03:08:23，文档 3308 行、sha256 `544ad466…`（`loop.md:820-821`）；七份面板报告落盘 03:18:55–03:40:08；修复提交 `7a36a9253` 03:46:12，文档 3332 行、sha256 `3b8b0bde…`。冻结窗口内零提交，冻结在行为上守住。
- 本轮没有独立回读文件：`panel/round17/` 只有七份面板报告，没有 `round17-rb.md`，也没有 `BASELINE.txt`。`loop.md:861` 把回读写成“修复段由主 agent 逐条复读（同 agent 局限已在 §8 标注）”，但 R17 段没有 §8。
- 输入：本轮 `loop.md`/`metrics.md`/`defect-ledger.md`/`follow-ups.md`/`OPEN-DEBT.md`、R17 七份面板报告、`BLIND-SPOTS.md`、`WORKFLOW-VERSIONS.md`、`DECISIONS.md`、R14–R16 的 meta 报告、当前文档与关联源码。
- 未采信：主 agent 在 `loop.md` 的自评只作线索，逐条回源；`expected/`、`traps/`、其他篇目的面板输出未读。
- 抽样：36 条（36/87 = 41.4%）完整复判或定位核对，覆盖全部七个角色与全部四种处置类。严格一致 31/36 = 86.1%，实质可接受 34/36 = 94.4%，低于 90% 契约线，按 `review-evals.md` 写 **NOT PASS**。抽样不是随机：刻意覆盖“自报 P0 被降级”“① 带 follow-up 票”“跨角色重复”“② 无档期”四类可疑点，对账本作弊的敏感度高于对平均质量的代表性。

## 结论速览

**总体：`IMPROVED`。** 这是产出层的进步，不是装置层的进步。

支持 `IMPROVED` 的事实：

- 处置率从 R16 的 22/119（18.5%）升到 76/87（87.4%，按台账 ID 展开为 ① 28 + ④ 48）；② 从 97 条降到 1 条；`re-litigation` 0、`adjacent 阻塞` 0。
- 七角色全部到位：独立设计第一遍 D1 自 R4 起停跑（R3 起按 3+ 规则跳过），R17 首次恢复；反证 P-REFUTE 自 R3 之后首次恢复（R4–R16 缺席 13 轮，`loop.md:831`）；读者 A/B/C 维持三角色（B-35 的合并问题自 R14 起已纠正，本轮未复发）。
- 台账从 R16 的 5 列 119 行全 `—`，变成 8 列 + 严重度实际分档（P0 4 / P1 5 / P2 45 / P3 10 / `—` 6 行），且 87 个 ID 唯一、无孤儿行。
- 检测到的是实质设计问题，不是命名/注释：`pt_alloc` 写者侧的数据竞争、`BootStorage` 双表半提交、`KernelInfo` 构造中合法值、容量三处、活占用集合双枚举器、DTB 路径假 `'static`、单实现 trait、测试专用辅助三件套。6 项代码改进完整落地（D2-04/06/07/08/10/11，`7a36a9253`），D2-17 的容量推导注释也已补；我逐项复读，实现与建议一致。D2-05 加了提交路径但旧 setter 仍在，D2-09/12/13 是半修。
- 跨角色重复终于被合并：测试计数一条被 FACT-05/06、READER-A-07、READER-B-03、READER-C-03、CLAIM 03 五个来源同时报出，台账用一行合并，未出现 R16 那种“同一缺陷一个记 ① 一个记 ②”的分裂处置。

支持 `REGRESSED` 或至少 `CONCERN` 的事实：

- **独立回读被取消。** R14–R16 的 `RBn` 是唯一抓住“修复动作自身引入新错”的装置（R16 的 `RB16-03/04/05/08`；其中一条 P1 把 arena 归属改反）。R17 没有 RB，改为同 agent 复读，而这正是 R16 meta 判定“最有效检测器”的装置。B-44 因此不是缓解，是倒退。
- **轨迹门仍是空转。** 我把整个 `## R17 面板轮` 表从副本里删掉后，`eval-trace-check.py` 仍报 `coverage: 所有面板发现都有处置行`、结论 `PASS`（台账处置从 1120 降到 1027）。R16 meta 的 B-31 探针（只删一行）本轮升级为“删整表仍 PASS”。
- **指标三层不同数。** 台账按 ID 展开是 ① 28 / ② 1 / ③ 10 / ④ 48；`metrics.md:4` 与 `loop.md:845-846` 是 ① 25 / ② 1 / ③ 10 / ④ 51。差额恰好是三个被折进 ① 行的读者 ID（`READER-A-07`、`READER-B-03`、`READER-C-03`）：metrics 把 51 条读者项全算 ④，同时 ① 只数了 25。
- **四条 ① 是半修。** `DESIGN-D2-05/09/12/13` 的发现条件仍在生产代码里成立，台账却记整修：`D2-05` 的两个独立 setter 仍是 `pub`；`D2-09` 点名的两处 `Box::leak` 原样保留；`D2-12` 的 trait 仍在；`D2-13` 只删了零调用的 `classified_descriptors`，三个辅助函数与四个 `static mut` 仍在。
- **R17 的 `loop.md` 只有 §0–§5。** 缺 `§6 指标`、`§7 状态`、`§8 给 meta-review 的不确定点`；`§5` 还自引一个不存在的 `§8`。轨迹门仍报“九个固定标题齐全”，因为它在整份文件里找 `## §6` 这类串，命中的是 R0。
- **转出没有档期。** 唯一的 ②（`DESIGN-D2-18`）落到 `F-R17-04`，`follow-ups.md:96` 写“待用户裁决（契约变更）”，没有截止轮次；`F-R17-01/02/03` 同样没有截止轮次。按 `review-evals.md` §4.1a 与 §4.1 的 ⑥，这是不合法的转出。

一句话：**修复与检测显著变好，记账与复验显著变差；前者是行为，后者是装置。**

---

## 一、Detection —— PASS

### 1.1 以前漏的，本轮抓到了

| 缺陷 | 本轮证据 | 历史盲区 |
|---|---|---|
| 文档引用了两个全仓不存在的 trait 常量 `HUGE_PAGE_SHIFT` / `PTE_HUGE_IDENTIFIER_BIT`，并据此写了整条设计理由与否决表 | `fact.md:53-64`、`refute.md:7-20`（两个角色独立命中）；我重放 `grep -n "HUGE_PAGE_SHIFT\|PTE_HUGE_IDENTIFIER_BIT"` 于文档与 `os/--include=*.rs` 均为 0；`paging_ext.rs` 的 trait 只有 5 个成员 | B-12 引文保真 |
| §3.8 的 `BootAlloc::alloc` 引文与反事实建立在已删除的 `fetch_add`/`fetch_sub` 上 | `fact.md:67-78`、`refute.md:22-36`；我重放 `grep -n "fetch_add\|fetch_sub"` 于文档与 `boot_alloc.rs` 均为 0；源码现为 `BumpRegion::resumed` → `alloc_pages(1)` → `next.store` | B-05 假反事实 + B-12 |
| §5.2.1 的逐文件表与紧随的求和句对四个文件给出两套数字 | 五个来源同时命中（`fact.md:105-116`、`reader-a.md:65-73`、`reader-b.md:35-43`、`reader-c.md:28-41`、`refute.md:38-48`）；我重放 `grep -c '#[test]'` 得到 loader 22、`ebs_boundary_audit.rs` 2、`kernel_info.rs` 21、`handoff_v1_write.rs` 10，与修复后的表一致 | B-08/B-14 计数口径 |
| `FileLoader` 契约三方互斥：trait 返回 `FileBytes`、§4.3 的 impl 返回 `Vec`、§3.4.5 说“不再 `to_vec`” | `reader-c.md:6-15`；我核 `opensbi_helpers.rs:198-238` 已返回 `Option<FileBytes>` 与 `FileBytes::Borrowed`，修复后三方一致 | B-03 跨节矛盾 |
| `pt_alloc::register` 的写者侧不是互斥：两个线程可同时读到 `already == false` 并写同一 `UnsafeCell` | `design-d2.md:54-63`；修复改为 `PT_CLAIMED.swap(true, AcqRel)` 先抢写权（`pt_alloc.rs:95-121`），我复读确认只有一个写者能进入写指针分支 | B-01/B-13（新增实例） |
| `BootStorage` 两张内存表可“半提交”，长度是两个独立 setter | `design-d2.md:65-74`；修复新增 `commit_map_tables`（`boot_storage.rs:104-132`），两处生产调用点改用 | 新类别（非法状态可表示） |

### 1.2 仍然漏的

- **没有新的运行期代码缺陷。** 四条 P0 全是文档事实（不存在的常量、旧引文、签名、计数），没有一条是可执行代码缺陷。QEMU 三门在修复后重跑（`loop.md:856`），但用途是回归，不是发现。按 B-49，这最多算“抽检过”，不算检测深度提升。
- **B-38 只做了一半。** FACT 跑了 `cargo test -- --list`、`grep -c`、`gdb`/`qemu -d` 类检查，但没有执行文档里那段 `cargo test` 断言本身（`fact.md:180` 自认“未复跑断言结果”）。文档里的可复制命令仍有一条未被真正执行。
- **B-09（门绿但事错）没有主动探针。** 本轮六门全绿，但 panel 自己没有对“门覆盖面 = 事实覆盖面”做阴性声明。真正把门打穿的是我这次 meta 的删除探针，不是面板。
- **材料隔离声明是自述，没有 mechanical 证据。** 七份报告都写“未读历史/其他面板/expected”，但没有一份带读取时的 sha256 或 HEAD。reader-b 写“3309 行”（`reader-b.md:3`），与冻结记录的 3308 行差一行，无法判断是计数差异还是快照差异。

结论：Detection 判 PASS，判据是“六个新事实/设计缺陷被两个以上角色独立命中，且修复后旧串归零”；不是“新类别丰富”。

---

## 二、Precision —— CONCERN

### 2.1 误报与角色模式

- **真正的误报很少。** 36 条抽样中，唯一一条自认非缺陷的发现是 `FACT-09`（行号 759/760 正确，`fact.md:154-162` 自己写“不构成缺陷”）。`CLAIM 04` 是反证失败、主张成立，不是缺陷。读者与 FACT 的其余条目我逐条回源，全部成立。
- **设计第二遍的 20 条里只有 11 条是可实施缺陷。** `DESIGN-D2-01/02/03/14/15/16/19/20` 八条是对 D1 方案的否决；它们作为“设计审阅结论”有 value，但作为“缺陷发现”不是。把 D2 的产出全算进 87 条，会让“发现数”高估可修面约 9%。
- **角色模式与 R16 相反。** R16 是 “fact/reader C 有转化、design/reader B 为零”；R17 是 **reader 的 51 条全部在本轮修（48 条记 ④，3 条测试计数项折进 ① 行）；D2 的 11 条 ① 里，6 条代码改进与 1 条容量注释完整落地、1 条部分（D2-05）、3 条半修（D2-09/12/13）；FACT 的 9 条可修项全部修**。没有哪个角色只产出措辞。
- **真正的精度问题不在“报错”，在“报完成度”。** 四条 ① 是半修（见下），占 21 条 ① 行的 19%、28 个 ① ID 的 14%。这不是误报，是把“改了一半 + 开票”记成“本轮修完”，会让 ① 的账面完成度虚高。

### 2.2 四条半修 ① 的逐条证据

| 台账行 | 台账处置 | 实际状态 | 判据 |
|---|---|---|---|
| `DESIGN-D2-05`（`defect-ledger.md:1262`） | ① “新增 `commit_map_tables(len,len)` 一次提交；两处生产调用点改用” | 半修：`set_memmap_len` 与 `set_reserved_len` 仍是 `pub`（`boot_storage.rs:133、147`），生产调用者为 0，但 API 仍允许只提交一张表；“半提交可表示”这个缺陷条件未消除 | D2-05 的最小修复原文要求“保留旧 setter 为 `pub(crate)` 或直接删” |
| `DESIGN-D2-09`（`:1266`） | ① “`OwnedDtb.bytes` 注释写明 shim 生命周期；Phase 栈化登记 F-R17-03” | 未修：两处 `Box::leak` 仍在 `uefi_helpers.rs:866`、`dtb_copy.rs:126`；`OwnedDtb.bytes` 仍是 `&'static [u8]` | 发现点名的“2 处伪造 `'static`”一处未动 |
| `DESIGN-D2-12`（`:1269`） | ① “trait 文档删除过时描述；去留裁决登记 F-R17-01” | 半修：文档已改（`boot_shim.rs:38-53` 与现状一致），但 `BootShim` 仍是 1 impl / 0 dyn 的单实现 trait；发现的主句“死抽象”未消除 | F-R17-01 是“待用户裁决”，不是本轮修 |
| `DESIGN-D2-13`（`:1270`） | ① “删除零调用 `classified_descriptors`；测试三件套 feature 门登记 F-R17-02” | 半修：`classified_descriptors` 零命中，但 `build_memmaps`（`uefi_helpers.rs:384`）、`exit_boot_services`（`:1129`）、`UefiPoolAllocator`（`:153`）与四个 `static mut` 仍在生产二进制；发现的主体未动 | F-R17-02 是“独立批次”，无截止轮次 |

这四条不是“D2 报错了”，是“D2 报对了、修复只做了一半、台账写成整修”。按 `review-evals.md` §4.1，① 是“范围内真缺陷，本轮修”；按 §8 第 13 项，① 要在台账列出发现点名的全部位置并逐处回读。四条都不满足。

### 2.3 ② 的档期

唯一 ② 是 `DESIGN-D2-18`（`KernelInfo` 不携带 `ram_authority`，混合记录表下可能放行）。`follow-ups.md:96` 的目的地是 `F-R17-04`，状态“待用户裁决（契约变更）”，**没有截止轮次**。按 §4.1a 第 2 种合法形态，必须写“截止 <哪一轮之前必须裁决>”。这是本轮 ② 唯一一条，也是不合法的。

结论：Precision 判 CONCERN。检测精度好（抽样误报 0–1），处置完成度精度差，且差的方向固定为“① 记整修 + follow-up 票代替档期”。

---

## 三、Design quality —— PASS

本轮设计发现的质量是 R14 以来最高的一轮，且不是命名/注释级：

1. **并发/非法状态**：`pt_alloc` 的单次写槽用 `&` 共享引用写全局，`AtomicBool` 只发布不互斥（D2-04）；修复改为 `swap` 抢写权。这是“可表示的数据竞争”，不是风格问题。
2. **可表示但语义不可能的状态**：`BootStorage` 两张表可以只提交一张（D2-05）；`KernelInfo` 的“构造中”是合法值，builder 置空后由调用方补写（D2-06）。
3. **翻译残留/重复来源**：同一容量住在三个 crate 的三处，跨 crate 只有运行期断言（D2-07）；“活占用区间”有两个手写枚举器（D2-08）；`BootStorage` 的 `Result` 与 panic 两套容量耗尽表达（D2-10）。
4. **假所有权**：DTB 暂存路径的 `Box::leak` 与 `&'static`（D2-09）；这一点与 D1 的“按值交接、不留全局单例”正面冲突，D2 用它反证 D1 的 typestate 只在跨边界时才需要（D2-14）。
5. **死抽象与测试专用面**：`BootShim` 单实现 trait（D2-12）；UEFI 辅助三件套只服务 16 个 QEMU 测试内核（D2-13）。
6. **抽象边界**：D1 的“垫片只做三件事”漏了 `.bss` 清零、hart 选举与 `cld`（D2-15）；`Paging` 的“六方法”清单与 14 成员 trait 不符（D2-16）；D1 的恒等映射收窄会破坏四条实现依赖（D2-20）。
7. **容量论证缺失**：40 槽是 C 保真但无上界证据，128 也没有；真正的问题是“用一个未测量的数当容量”（D2-17）。

独立性也成立：D1 只读 C 源与 `§1/§2`，提出五条决策；D2 用源码否掉了其中三条的事实前提（D2-01/02/03），这不是复述，是两遍设计的对抗。实施侧我逐条复读：D2-04 的 `swap`、D2-05 的 `commit_map_tables`、D2-06 的新参数、D2-07 的四条编译期等式与 `FREE_SET_CAP` 引用、D2-08 的 `for_each_live_bootstrap`、D2-10 的 `#[cfg(test)]`、D2-11 的三腿参数计数，全部与建议一致且无行为回归。

扣分项只有一条：D2-05/09/12/13 的建议没有被完整执行，却被记成 ①（见第二节）。这不影响“发现问题”的质量，影响“修完没有”的判定。

结论：Design quality 判 PASS。

---

## 四、Independence —— PASS（面板） / CONCERN（验证）

### 4.1 面板侧：独立发现占比高

- 七份报告都声明材料隔离；`refute.md:5` 与 `design-d1.md:3` 明确未读现有 Rust / 作者结论 / 其他角色输出。本轮冻结窗口零提交，面板看到的是同一份 3308 行文档。
- 去重后独有发现约 76/87（87%）。重复集中在六组：`A-01=B-01`（3.4 小节序）、`A-03=B-05`（栈对齐）、`A-04=B-04`（D1 次序）、`B-06=C-12`（§1.4 指针）、`FACT-01=CLAIM01`、`FACT-02=CLAIM02`，以及测试计数的五源簇。除测试计数外，每组都是两个角色从不同视角撞到同一处，属正常交叉验证。
- D1 是真正的冷设计：它不知道 `AllocAuthority` 要替代的 `kernel_may_alloc` 已被整体删除，也不知道五步建表已收进一个 private 函数；D2 用源码逐条指出前提不成立。这个“提出—反证”对是独立性的最强证据。

### 4.2 验证侧：独立回读消失

- R15/R16 的 `RBn` 都在面板 + 修复之后独立读修复段，并各自抓出 4 条修复残留（其中含 P1）。R17 没有 RB；`loop.md:861` 写“由主 agent 逐条复读”。同 agent 复读不是零价值，但它与修复者共享同一套心智模型，正是 R16 meta 判定“最有效”的那个装置被移除。
- 后果可测：我抽到的 4 条半修 ① 全部是“主 agent 自己写在台账里的处置句已经承认只做了一半”（“注释写明”“登记 F-R17-03”“去留裁决登记”“feature 门登记”），但没有一条被降级。这正是独立回读应当拦下的形态。
- 判决账：`DECISIONS.md` 从 R18 起有八字段表（D-013–D-023），这是真进步；但 R17 没有任何 `reviewer_verdict` 行，因为没有 RB。B-44 的“回读判决”半边仍然是空的。

结论：Independence 的面板半边判 PASS（冷读产出了独立发现，独有比例高），验证半边判 CONCERN（无 RB，修复段只由主 agent 复读）。

---

## 五、Scope discipline —— CONCERN

### 5.1 账面大幅改善

- ② 只有 1 条（1.1%），R16 是 97 条（81.5%）；`adjacent 阻塞` 0；`re-litigation` 0。
- 没有重提冻结决策：D-001（四章架构）、D-002（短锚+链接）、D-004（清单只放 §7）、D-005（“腿”是受控术语）都没有被任何角色当成新问题。读者 B-18 要求删 §1.3，修复选择了“给空节补读法”而不是删节，也没有触碰章骨架。
- 没有把相邻模块当阻塞：`R14/READER-A-16` 是 adjacent，按 §4.2 不阻塞，本轮没有把它升级。

### 5.2 但“① 全修”是账面数字

- 真实 in-scope 未完成项不是 1 条，而是 1 条 ② 加 4 条半修 ① = 5 条，占 87 条的 5.7%。仍比 R16 的 81.5% 好一个数量级，但“1.1%”这个读数不成立。
- `F-R17-01/02/03/04` 全部没有截止轮次。按 `review-evals.md` §4.1 的 ⑥，代码重构要么本轮实施，要么进 `OPEN-DEBT.md`（有负责人、有截止轮次），不能留在 `follow-ups.md` 无限期搁置。`OPEN-DEBT.md:3` 却写“当前无跨 crate 欠账”，而 `F-R17-04` 要改 `KernelInfo`（minix-boot）+ 接收侧校验 + 文档 ABI，是跨 crate 契约变更。B-47 复发：欠账表自称唯一欠账源，与台账不一致。
- 文档里两次出现“R17 DESIGN-03”（`01-boot-shim-bootstrap.md:1374、1454`），但 R17 台账没有 `DESIGN-03` 行，面板也没有这个 ID；它只存在于 `loop.md:757` 的散文里。ID 没有可回指的台账行，属 B-31/B-17 家族的记账卫生问题，不是事实错误。

结论：Scope discipline 判 CONCERN。行为上不再“大桶转出”，但出现了新的出口——半修记 ① + 无档期 follow-up 票——它让“in-scope 待修 = 0”在账面成立、在代码上不成立。

---

## 六、Teaching quality —— PASS

- 读者三角色共 51 条，全部在本轮修（48 条记 ④，`READER-A-07`/`B-03`/`C-03` 三条测试计数项折进 ① 行）；抽样 20 余条逐条回源，全部成立且修复忠实。代表性例子：
  - `READER-A-01/B-01`：3.4.5 排在 3.4.4 之前。修复后标题顺序为 3.4.1→3.4.2→3.4.3→3.4.4（深拷）→3.4.5（所有权），编号与顺序一致（`grep '^#### 3\.4\.[0-9]'`）。
  - `READER-A-05`：U/S 位“无实际效果”漏了 `CR4.SMEP = 0` 前提。修复在 §2.6.1 补全前提，并说明 Rust 侧不置 USER 正是为了不依赖它（文档 900 行）。
  - `READER-B-07`：以冒号悬空的引导句被移到 §3.4.2 标题之后（文档 1568–1572）。
  - `READER-C-01`：`FileLoader` 三方互斥；修复后 §3.2 trait、§4.3 impl、§3.4.5 文本三处一致，且源码 `opensbi_helpers.rs:198` 确为 `Option<FileBytes<'b>>`。
  - `READER-C-15`：`BootStorage` 只列五类容量、报告借用七类切片。修复补了 source 40（`DTB_SOURCE_CAP`）与 startup 20（`WIRE_STARTUP_CAP`），并说明 UEFI 腿的 source 槽留空、写 wire（文档 1667）；与 `boot_storage.rs:56-75` 一致。
  - `READER-C-07`：扣除表的模块行未限定 UEFI 腿。修复后该行明确“UEFI blobs are `LOADER_DATA`, so this row is a no-op there”（文档 2040 附近）。
  - `READER-A-14`：§7.5 把 `k_initial_stktop` 与 `kern_stack_top` 画成一对一；修复改为“角色对应，不是同一对象”（文档 3332）。
- 真实理解障碍占比高：`FileBytes` 三方契约、64 条交接上限的可达停机、UEFI 模块不进 free list、SMEP 前提、七套计数的换算、快照/恒等窗口的定义，都是会让有基础的读者停下来的点，不是文风偏好。
- 纯文风噪声约 12/51（24%），集中在 P3：`B-13`（删“不是‘两道’”）、`B-19`（三条→四条）、`B-20`（回指无锚）、`A-11`（计数免责声明改换算说明的一部分）。这些已修，不影响判断。
- 采纳率 51/51 = 100%（其中三条 `READER-A-07`/`B-03`/`C-03` 折进 ① 的测试计数行，也修了）。R16 的 reader 转化率约 10–30%，本轮是结构性改善。

结论：Teaching quality 判 PASS。

---

## 七、Cost —— CONCERN

| 指标 | R14 | R15 | R16 | R17 |
|---|---:|---:|---:|---:|
| 角色报告 | 8 | 7 | 7 | 7 |
| 原始发现 | 109 | 112 | 119 | 87 |
| 本轮修（按 ID） | 75 | 31 | 22 | 76 |
| 修复率 | 68.8% | 27.7% | 18.5% | 87.4% |
| 转出 | 29（26.6%） | 81（72.3%） | 97（81.5%） | 1（1.1%，真实约 5.7%） |
| 独立回读 | RB13 | RB15 | RB16 | 无 |
| 面板耗时 | 约 28 分 | 约 29 分 | 约 32 分 | 约 22 分（03:18–03:40） |
| token | 未采集 | 未采集 | 未采集 | 未采集 |

- 单位成本明显下降：每修一条摊到的面板产出从 R16 的 5.2 条降到 1.14 条；面板 22 分钟、7 个 agent，产出 87 条、修 76 条。
- 但省下的成本有一部分是“取消 RB”换来的。若把 RB 加回来（1 个 agent、约 20–30 分钟），本轮成本会回到 R16 水平；换来的可能是 4 条半修被当场降级。是否值得，取决于下一轮是否恢复 RB。
- 仪表仍不完整：token 连续五轮“未采集”；`loop.md` R17 §1 没有记录并发数（B-37 的期望探测器是“派发前先数存活子 agent + 主线程 + 计划派发数”）；`metrics.md:7` 的“重构 提出 8 / 接受 8 / 实施 8 / 登记 4”无法从台账复算（台账 ① 的 D2 行是 11 条，其中 4 条半修、1 条只改注释）。
- 收益递减判断：本轮不是递减，因为换了新基线（R18 架构改动使旧结论失效），87 条里至少 6 条是新事实/新设计缺陷；但“文档事实 + 读者体验”仍占绝大多数，真代码缺陷只有 D2-04（潜在 UB）一条。下一轮如果仍在同基线上跑到 80+ 条，收益递减就该判 FAIL。

结论：Cost 判 CONCERN。绝对成本低、产出高，但成本仪表不完整，且“低成本”部分来自取消独立回读。

---

## 八、Trace integrity —— FAIL

### 8.1 做对的部分

- 87 个 ID 唯一，无重复行，无孤儿行；多 ID 合并行 10 行，合并的是真重复（测试计数簇、A-01/B-01、A-04/B-04、B-06/C-12）。
- 表头是 8 列（`发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验`），比 R16 的 5 列完整；严重度不再是全 `—`（P0 4 / P1 5 / P2 45 / P3 10 / `—` 6）。
- 台账、`loop.md §2`、`metrics.md` 的发现总数一致为 87。
- 每条 ① 的复验列非空（不是 `TBD`/`-`），虽然多数只写“源码对照/五门”。

### 8.2 六处硬伤

**（1）五类计数三源不一致。** 我用 `python3 -I` 按 `## R17 面板轮` 之后的行解析，展开多 ID 行：

```
rows 70；IDs 87；unique 87
per-ID 处置: ① 28 / ② 1 / ③ 10 / ④ 48 / ⑤ 0
row-level 处置: ① 21 / ② 1 / ③ 5 / ④ 43
```

`metrics.md:4` 与 `loop.md:845-846` 是 `① 25 / ② 1 / ③ 10 / ④ 51`。差额来源可精确定位：

- `defect-ledger.md:1209` 把 `FACT-05、FACT-06、READER-A-07、READER-B-03、READER-C-03、CLAIM 03` 合成一行，处置 ①；
- metrics 把 51 条读者项全算 ④，于是 `A-07/B-03/C-03` 在 ④ 里被计了一次、在 ① 里没有计；
- `loop.md:845` 的 ① 清单又把 `READER-C-01` 算进 ①（台账 `:1244` 记 ④），方向相反。

结果：87 的总和自洽，①/④ 的分割错 3 条。轨迹门的 `check_metrics` 只校验五项之和等于总数，所以报 PASS。

**（2）覆盖门对整节删除无感。** 我把 run 目录与 `panel/round17` 复制到 `/tmp`，删掉整个 `## R17 面板轮` 表后重跑：

```
[PASS] panel: 面板报告解析到 81 条发现（含轮次子目录）
[PASS] metrics: 发现总数与处置之和一致（87）
[PASS] ledger: 台账解析到 1027 条处置
[PASS] coverage: 所有面板发现都有处置行
汇总: PASS 9 / WARN 2 / FAIL 0；结论 PASS
```

只把 `DESIGN-D2-18` 改名（删掉唯一 ② 行）同样 PASS。原因是 `ledger_ids` 是全文件所有轮次的并集，R17 的 87 个短 ID 与更早轮次同名，覆盖检查被同名行顶替。B-31 从 R13 记到 R17，本轮从“删一行无感”升级为“删整表无感”。

**（3）R17 的 `loop.md` 缺三个固定节，门仍报齐全。** R17 段只有 `§0 基线`、`§1 面板`、`§2 发现`、`§3 归因`、`§4 修复`、`§5 复验`；缺 `§6 指标`、`§7 状态`、`§8 给 meta-review 的不确定点`。`loop.md:861` 还写“同 agent 局限已在 §8 标注”，但 R17 没有 §8。门的 `check_loop_sections` 在整份文件里 `find("## §6")`，命中的是 R0 的节，所以报“九个固定标题齐全”。这是 R16 meta 第 5 条的同一形态，本轮从“缺两个”变成“缺三个”。

**（4）面板豁免命中的是 R0，不是 R17。** `design-d1.md` 只有 5 条 `DECISION`，`refute.md` 只有 6 条 `CLAIM`，都不含 `## FINDING`。门报：

```
[WARN] panel: design-d1.md 没有 `## FINDING <ID>` 行
[PASS] panel-exemption: design-d1.md 无 FINDING，loop.md 已豁免
```

但 `grep -n "豁免" loop.md` 在 R17 段没有任何命中；豁免正则匹配到的是 `loop.md:28`（R0 的“无发现面板：`design-d1.md`…”）。R17 实际没有写“D1 的 5 条 DECISION 只作参考、不入缺陷计数”的理由。B-17/B-26 复发。

**（5）轨迹门没有被真正执行过，§5 留的是占位。** `loop.md:861` 写“trace 门：`eval-trace-check.py <run-dir> --panel-dir …/panel/round17` 待本轮台账收口后执行”。但台账与 metrics 在同一个修复提交 `7a36a9253` 里已经收口，§5 仍是“待执行”。我补跑的结果是 `PASS 9 / WARN 2 / FAIL 0`，其中 WARN 两条正是 D1/refute 没有 FINDING。也就是说：**本轮官方的轨迹结论不存在，只有一个待办句；我跑出来的是 PASS，但这个 PASS 的覆盖面已被 (2) 证明近乎空转。**

**（6）证据列有跨轮残留与口径混用。**

- `DESIGN-D2-07`/`DESIGN-D2-08` 的证据写“`minix-kernel` 860 通过”（`:1264、1265`），而 `loop.md:855` 写“`minix-kernel --features mock` 868”。按 `loop.md:794`（R18）的口径，860 是通过数、868 是含 8 条 ignored 的总数；但 R17 段没有写这个等式，读者会读成矛盾。
- `metrics.md:7` 的“提出 8 / 接受 8 / 实施 8 / 登记 4”无法从台账复算：D2 的 ① 行有 11 条，其中 4 条半修、1 条只改注释。
- 6 行严重度仍是 `—`（全是 ③ 非缺陷：FACT-09、CLAIM 04、D2-01/02/03 行、D2-14、D2-15/16/19/20 行）。C-15 要求 severity 非空；对非缺陷用 `—` 可以讨论，但规则文本没有这个例外。
- 面板报告没有一份写读取时的 sha256/HEAD；`round17/` 没有 `BASELINE.txt`。所以“面板读的是冻结版”只能靠提交时间线人工推断，不能机械复核。B-33 仍 OPEN，B-43 有 3309/3308 的一行差。

### 8.3 抽样复判（36 条，节选）

| # | 台账行 | 处置 | 我的复判 | 判据 |
|---|---|---|---|---|
| 1 | FACT-01 + CLAIM 01 | ① | PASS | 文档与 Rust 源两处 `HUGE_PAGE_SHIFT\|PTE_HUGE_IDENTIFIER_BIT` 均 0 命中；`paging_ext.rs` trait 5 成员 |
| 2 | FACT-02 + CLAIM 02 | ① | PASS | 文档与 `boot_alloc.rs` 的 `fetch_add/fetch_sub` 均 0；源码为 `BumpRegion::resumed` → `alloc_pages(1)` |
| 3 | FACT-03 | ① | PASS | §3.7 引文改为 `report.*` 四标量与 `boot_validate_and_prepare` 四实参 |
| 4 | FACT-04 | ① | PASS | §4.4 区分“两直核腿共用 builder / 三条 v1 腿共用 storage” |
| 5 | FACT-05 | ① | PASS | 重放 `grep -c`：loader 22、ebs_boundary 2、kernel_info 21、handoff_write 10，与表一致 |
| 6 | FACT-06 | ① | PASS | 重放函数名：rejects 9 / accepts 3 / requiring 4 / postconditions 4 / getters 1 = 21 |
| 7 | FACT-09 | ③ | PASS | `exec.c` 759/760 正确，758 是 `mi_flags`，省略可接受 |
| 8 | CLAIM 04 | ③ | PASS | `alloc_lowest` 唯一调用 `arch_smp.c:76`；`main.c` 的 `cstart→system_init→smp_init` 次序成立 |
| 9 | CLAIM 05 | ① | PASS | §4.4 已写“清零只对跨镜像腿成立；直核面 `_start` 只清到 `__bss_end`；安全性不依赖清零” |
| 10 | CLAIM 06 | ① | PASS | §2.4.1.4/§3.4 已补 `VM_BOOT_HANDOFF_MAX_REGIONS=64` 与停机行为 |
| 11 | DESIGN-D2-01 | ③ | PASS | `FreeRangeSet` 只在 `classify` 与测试中使用，无可变账的第二消费者 |
| 12 | DESIGN-D2-02 | ③ | PASS（证据有瑕） | 无活跃全局整数；但“`grep -rn kernel_may_alloc os/` 零命中”不成立（`lib.rs` 有注释命中，`os/target/fmtchk` 有旧副本） |
| 13 | DESIGN-D2-04 | ① | PASS | `PT_CLAIMED.swap(true, AcqRel)` 先抢写权，再写指针、后发布（`pt_alloc.rs:95-121`） |
| 14 | DESIGN-D2-05 | ① | CONCERN | `commit_map_tables` 已加且两处调用，但两个旧 setter 仍 `pub`，半提交仍可表示 |
| 15 | DESIGN-D2-06 | ① | PASS | `build_kernel_info` 收 `memmap_sources`/`startup_objects` 两参；`grep '\.startup_objects *='` 零命中 |
| 16 | DESIGN-D2-07 | ① | PASS | `globals.rs:382-385` 四条 `const _: () = assert!`；`FREE_SET_CAP = MEMMAP_REGION_STORE_LEN` |
| 17 | DESIGN-D2-08 | ① | PASS | `for_each_live_bootstrap` 在 `vm_handoff.rs:689`，`classify` 与 `vm_boot_exclusions` 共用 |
| 18 | DESIGN-D2-09 | ① | **FAIL** | `uefi_helpers.rs:866` 与 `dtb_copy.rs:126` 的 `Box::leak` 仍在；发现点名的两处一处未动 |
| 19 | DESIGN-D2-10 | ① | PASS | `push_module` 收进 `#[cfg(test)]`；模块文档说明 slot 直写与 setter 契约 |
| 20 | DESIGN-D2-12 | ① | CONCERN | trait 文档已按事实改写；trait 本体仍在，去留是 `F-R17-01` 裁决 |
| 21 | DESIGN-D2-13 | ① | CONCERN | `classified_descriptors` 已删；`build_memmaps`/`exit_boot_services`/`UefiPoolAllocator` 仍在 |
| 22 | DESIGN-D2-18 | ② | **FAIL** | `F-R17-04` 无截止轮次；且是跨 crate 契约变更，`OPEN-DEBT.md` 却写“无跨 crate 欠账” |
| 23 | READER-C-01 | ④ | PASS | §3.2 trait、§4.3 impl、§3.4.5 三处均为 `FileBytes`；源码 `opensbi_helpers.rs:198` 一致 |
| 24 | READER-C-15 | ④ | PASS | 文档补 source 40 / startup 20；`boot_storage.rs:56-75` 一致 |
| 25 | READER-B-08 | ④ | PASS | §2.8 已删“两条/三条”二分，改为“五条证据都在本章，按小节列” |
| 26 | READER-A-05 | ④ | PASS | §2.6.1 补 `CR4.SMEP = 0` 前提与 Rust 侧对照 |
| 27 | READER-A-13 | ④ | PASS | “其余五类”与 `PageTableError` 6 变体一致 |
| 28 | READER-B-01/A-01 | ④ | PASS | 3.4.4 在 3.4.5 之前，编号与顺序一致 |
| 29 | READER-A-14 | ④ | PASS | §7.5 改为“角色对应，不是同一对象” |
| 30 | READER-B-10 | ④ | PASS | §2.4.1 开篇把 2.4.1.4 声明为“同一份清单生命周期的第三件事” |

另有 6 条定位核对全部 PASS：`A-03`（栈对齐裁决句）、`A-09/A-10`（腿与 bump 定义）、`A-16`（§3.1.1 指针）、`B-02`（7→8 完整 24 字节算式）、`B-17`（§7.5 权威）、`C-08`（五步消费者改 §4.2.2/§5.2.1）。

严格一致 31/36 = 86.1%；把三条 CONCERN 算入“实质可接受”为 34/36 = 94.4%。两者都按 90% 契约判 **NOT PASS**（严格线不达）。失败方向高度集中：**② 与半修 ①，不是读者项，也不是事实项。**

---

## 九、本轮最值得、但工作流仍看不见的一件事

**工作流看不见“缺陷的成立条件还在不在”，只看“旧串在不在”。**

R17 的 ① 验收基本是“旧串归零 + 五门绿”。这套判据能抓住“没改”，抓不住“改了一半”：

- `D2-09` 的成立条件是“两处 `Box::leak` 生产路径”。台账写“注释写明真实生命周期”，旧句消失了，两处 `Box::leak` 仍在。门绿。
- `D2-05` 的成立条件是“半提交可表示”。修复加了 `commit_map_tables`，旧调用点消失，两个 `pub` setter 仍在。门绿。
- `D2-12` 的成立条件是“单实现 trait 承担无收益抽象”。文档里的假描述消失了，trait 仍在。门绿。
- `D2-13` 的成立条件是“测试专用辅助三件套在生产二进制里”。`classified_descriptors` 删了，三件套仍在。门绿。

这不是“修复者偷懒”，是**判据的形状**决定的：`review-evals.md` §8 第 13 项要求“列出发现点名的全部位置，修完逐处回读”，但没有要求“列出发现成立的条件，修完逐条检查条件是否消失”。前者是文本位置，后者是语义条件。D2-09 点名的位置是两行 `Box::leak`，如果修复者只回读“注释那一行”，fix-scope 就是 1/2；但台账写的是“注释写明”，读者会以为位置已经覆盖。

这件事值得优先补，因为它同时解释四个表面无关的现象：① 的修复率虚高（76/87）、② 的比例虚低（1/87）、`OPEN-DEBT` 自称清零、`F-R17-01..04` 无档期。四者都是同一个洞：**“修了一半”在轨迹里没有形状。**

---

## 十、结论与 BLIND-SPOT-DELTA

### 10.1 总体结论

**`IMPROVED`。**

相对 R16：修复率 18.5% → 87.4%；② 81.5% → 1.1%（真实约 5.7%）；角色 5 → 7（D1 与 P-REFUTE 恢复）；台账 5 列全 `—` → 8 列 + 严重度分档；设计发现从“零代码落地”到 6 项完整落地 + 1 项部分落地（D2-05）+ 3 项半修（D2-09/12/13）；测试计数漂移在连续多轮复发后首次被四个来源同时抓住并精确修好（我复算 9/3/4/4/1 与 22/2/21/10 全部吻合）。这些都是行为层的真进步。

不判 `REGRESSED` 的理由：R17 没有像 R15 那样“把上一轮 L6 的三条规则全部换类绕过”。它执行了“旧串归零”（我抽查的旧串全为 0）、“转出带档期”（虽然 F-R17 票没有截止轮次，但至少每条 ② 有目的地）、“先建 open-debt 表”（表在，但内容与台账不一致）。行为没有更坏。

不判 `FLAT` 的理由：产出显著变好，且不是“更差的运行点上的持平”。R16 净增约 90 条欠账，R17 只新增 4 条有票的代码重构项与 1 条裁决项，欠账第一次真正下降。

但必须写清楚：**这是产出层的 IMPROVED，装置层有两处倒退。** 一是取消独立回读，二是轨迹门继续空转（删整表仍 PASS）。如果下一轮沿用本轮配置（无 RB、无轮次作用域门），G8 的“零新增”会因为半修与同名 ID 顶替而永远可以“账面达成”。

### 10.2 BLIND-SPOT-DELTA

| 条目 | 变化 | 证据 |
|---|---|---|
| **B-50 半修记整修：① 只查旧串/文本位置，不查缺陷成立条件** | **新增 OPEN** | `D2-05` 两个 `pub` setter 仍在（`boot_storage.rs:133、147`）；`D2-09` 两处 `Box::leak` 仍在（`uefi_helpers.rs:866`、`dtb_copy.rs:126`）；`D2-12` trait 仍在；`D2-13` 三件套仍在（`uefi_helpers.rs:153、384、1129`）；四行均记 ① |
| **B-51 轮次作用域门：loop 固定节、面板豁免、覆盖集合都按全文件匹配** | **新增 OPEN** | 删整个 R17 表 → `coverage PASS`；R17 段缺 §6/§7/§8 → “九个固定标题齐全”；`design-d1/refute` 豁免命中 `loop.md:28`（R0）而非 R17 |
| B-46 门不覆盖文档源结构与跨文档链接 | **CLOSED** | `tools/doc-structure-check.py` 落地并在 `rules-consistency.yml:98-105` 常驻；本文件 `doc-structure-check` 通过，表结构/内部 `.md` 链接有解析面 |
| B-35 合并读者 = 覆盖面损失 | **DETECTED（R17 三角色恢复）** | `loop.md:831` 列 A/B/C 三角色；读者产出 16/20/15，独有项约 39 条；但规则仍允许合并，未机械封堵 |
| B-31 台账无轮次作用域 | **仍 OPEN，恶化并量化** | 删整张 R17 表后覆盖门仍 PASS（处置 1120→1027）；87 个短 ID 全部与更早轮次同名 |
| B-40 指标三层不同数 | **仍 OPEN** | 台账按 ID 28/1/10/48；`metrics.md:4` 与 `loop.md:845-846` 25/1/10/51；`loop.md:845` 的 ① 清单自身不含 `A-07/B-03/C-03` 且多算 `C-01` |
| B-44 判决不落账 | **REGRESSED（回读半边）** | R17 无 `round17-rb.md`，`loop.md:861` 改同 agent 复读且自引不存在的 §8；`DECISIONS.md` 八字段表已建（D-013–D-023），但无 R17 `reviewer_verdict` 行 |
| B-22 处置类按来源角色分配，不按完成度 | **复发** | 4 条半修 ①（D2-05/09/12/13） |
| B-42 转出三件套 | **部分缓解，换新形态** | ② 从 97 降到 1；但 `F-R17-01..04` 全部无截止轮次，半修 ① 代替 ② 成为新出口 |
| B-47 欠账源自身不收敛 | **仍 OPEN** | `OPEN-DEBT.md:9` 写“当前无跨 crate 欠账”，而 `F-R17-04` 是跨 crate 契约变更；四张 F-R17 票不在表内 |
| B-17 / B-26 非 FINDING 面板逃出处置 | **仍 OPEN** | D1 的 5 条 DECISION 没有台账行；豁免命中的是 R0 行 |
| B-33 冻结基线无机械门 | **仍 OPEN** | `round17/` 无 `BASELINE.txt`；七份报告无 sha256/HEAD；只有提交时间线可核 |
| B-43 迟交回读读旧快照 | **CONCERN** | `reader-b.md:3` 写 3309 行，冻结记录 3308 行；报告不写读取 sha，无法判 STALE |
| B-49 严重度地板为零 | **部分缓解，仍 OPEN** | 四条 P0 仍全是文档项；但修复后重跑 QEMU 三门（`loop.md:856`），运行期抽检存在 |
| B-38 文档里的可复制命令从未执行 | **部分缓解，仍 OPEN** | FACT 实跑 `--list`/`grep -c`/`gdb`，但 `fact.md:180` 自认未复跑 `cargo test` 断言 |
| B-39 ① 以“注释并存”代替“消除” | **部分缓解，新形态** | 文档旧串全为 0；但 `D2-09` 的“注释写明”不是消除，是 B-50 的实例 |
| B-08 / B-14 测试计数口径 | **DETECTED（强证据）** | 四个来源 + CLAIM 03 同时命中；修复后 22/2/21/10 与 9/3/4/4/1=21 我逐条复算吻合 |
| B-05 假反事实 | **DETECTED（新证据）** | FACT-02/CLAIM 02：反事实建立在已删除的 `fetch_add` 上 |
| B-12 引文保真 | **DETECTED（新证据）** | FACT-01/02/03/07：四处 Rust 引文与源码不一致，其中两处常量不存在 |
| B-01 / B-02 / B-03 / B-04 / B-10 / B-11 | **DETECTED（守住）** | D2 的死抽象/非法状态、reader 的跨节矛盾与理解障碍、0 re-litigation、0 adjacent 阻塞 |
| B-36 / B-37 成本与并发仪表 | **仍 OPEN** | token 未采集；R17 §1 没有并发数 |
| B-45 同节兄弟缺陷留活口 | **未触发（仍 OPEN）** | 抽样未见“修一条、同节另一条原样留下”的新实例；规则未变 |
| B-48 撤诉式修复 | **未触发（仍 OPEN）** | 本轮没有把断言改成“未复核”的实例；但 B-50 是它的近亲 |

### 10.3 建议变更（下一版最多晋升 2 条）

**C-19 半修不得记整修（fix-scope 2.0）**

- 案例：D2-05（setter 仍 `pub`）、D2-09（两处 `Box::leak`）、D2-12（trait 仍在）、D2-13（三件套仍在）。
- 目标文件：`prompt/review-rules/review-evals.md` §4.1 的 ① 定义与 §8 第 13 项、`prompt/review-rules/fix-guard.md` 的“修后留痕”、`tools/eval-trace-check.py`。
- 期望效果：每条 ① 必须同时写“发现点名的位置”和“发现成立的条件”；修完逐条检查条件是否消失。任一生产工件仍在，降级为“①部分”，并在 `OPEN-DEBT.md` 建行（负责人 + 截止轮次）。① 的完成度不再由“旧串归零”单独决定。
- 回归风险：会有一批 ① 被降级，账面修复率下降；需要人工判断“条件是否消失”，不能完全机械化。
- 替代/合并：替代 C-25（发现级 fix-scope）与 B-22；把 C-11 的旧串检查吸收为子检查，不再作为全部。

**C-20 轮次作用域全量门**

- 案例：删整张 R17 表仍 `coverage PASS`；R17 缺 §6/§7/§8 仍“九节齐全”；`design-d1/refute` 豁免命中 R0；面板 81/87（6 条 CLAIM 不进覆盖）。
- 目标文件：`tools/eval-trace-check.py`（coverage、loop-sections、panel-exemption 全部按 `## R<nn>` 段解析）、`prompt/review-rules/review-evals.md` §3.2/§3.4。
- 期望效果：每轮的覆盖只看本轮段；每轮的固定小节只看本轮段；每个面板文件的豁免必须在本轮段命中；`CLAIM` 与 `DECISION` 也要有覆盖或显式豁免。删一行、删整表、缺小节都会红。
- 回归风险：历史轮次的台账命名不统一（`R1`/`Round 1`/`R13` 混用），需要一次迁移或兼容层；可能先红一批旧轮次。
- 替代/合并：合并 C-17（ID 前缀）与 C-15（三源恒等）为同一道轮次门；关闭 B-31/B-51。

**C-21 metrics 由台账自动生成（按 ID 展开）**

- 案例：`metrics.md:4` 25/1/10/51 与台账 28/1/10/48；`loop.md:845` 的 ① 清单与台账方向相反；`metrics.md:7` 的“重构 8/8/8/4”不可复算。
- 目标文件：`tools/eval-trace-check.py`、`review-evals.md` §3.4、`metrics.md`。
- 期望效果：五类计数、严重度分布、①/② 档期由台账一次性生成；多 ID 合并行按 ID 展开；`loop.md §6` 只引用生成结果，禁止手写。
- 回归风险：合并行的“一行多 ID”需要明确每个 ID 的处置，写作成本上升。
- 替代/合并：替代 C-12（metrics 自动生成）；关闭 B-40 的可复算半边。

**C-22 独立回读与判决账强制**

- 案例：R17 无 RB，四条半修未被拦；R16 的 RB 在同轮抓出 4 条（含 1 P1）。
- 目标文件：`prompt/skill/review-loop.md` L3.5/L4、`prompt/review-rules/review-panel.md` L1、`review-evals.md` §3.5。
- 期望效果：没有 `RBn` 文件不得声明“① 全修”；RB 的 `VERDICT` 必须进 `DECISIONS.md` 八字段表；同 agent 复读只能作为补充，需明确豁免理由。
- 回归风险：每轮多 1 个 agent、20–30 分钟；收敛轮次可能延后。
- 替代/合并：替代 R16 的“RB 先入账”实践；补 B-44 的回读半边；与 C-16（判决账）合并实施。

**C-23 冻结机械锚**

- 案例：`round17/` 无 `BASELINE.txt`；报告无 sha；`reader-b` 3309 对冻结 3308。
- 目标文件：`tools/eval-trace-check.py`、`review-panel.md` L1、`review-evals.md` §3.2。
- 期望效果：冻结时写 `BASELINE.txt`（sha256 + HEAD + 行数）；每份面板报告头写读取时的 sha；门比对，不一致整份标 `STALE`，不进台账。
- 回归风险：换行/末尾空行的计数差异可能造成假红，需要规范化后比较。
- 替代/合并：关闭 B-33/B-43。

按 `WORKFLOW-VERSIONS.md` 的“每次最多晋升 2 条”，建议下一版先推 **C-19 与 C-20**：它们分别堵住“半修”和“门空转”两个最大的洞；C-21/C-22/C-23 作为下一版候选。

---

## 十一、局限声明

- **同 agent 验证 + 已重放命令。** 我是冷启动子 agent，但与主 agent 使用同一模型。本报告所有结论都附了文件行号、提交或命令，重放的命令与读数见附录。抽样严格一致 86.1%，低于 90%，按 `review-evals.md` 写 `NOT PASS`。
- **快照。** 交付态是 `7a36a9253`（03:46:12），文档 3332 行 sha `3b8b0bde…`；冻结态是 `ec67e3d4f`（03:08:23），3308 行 sha `544ad466…`。面板报告落在两者之间；我按“冻结文档 + 修复提交后的当前文档”双读，判断修复是否落地。
- **没有构建、没有真机。** 我没有重跑 `cargo test`、QEMU 或任何构建；测试读数以 `loop.md`/`fact.md` 的命令与输出为据，我只重放了 `grep -c`、`grep -n`、`python3` 解析与 `doc-code-map`/`anchor-resolve`/`doc-structure-check` 三类文档门。QEMU 三门 PASS 是主 agent 自述，未独立复验。
- **抽样不是随机。** 36 条刻意覆盖每个角色、四种处置类、以及所有“① 带 follow-up 票”的行；对“账本作弊”的敏感度高于对“平均质量”的代表性。86.1% 不能外推成“七分之一的处置都错”，它主要由 4 条半修 ① 与 1 条无档期 ② 拉低。
- **轨迹门探针写在 `/tmp`。** 我复制了 run 目录与 `panel/round17` 到 `/tmp/r17probe-*` 与 `/tmp/r17probe2-*` 做删除实验，没有修改仓库内任何文件；唯一写入是本报告。
- **仓库是活的。** 评审期间主 agent 在 `loop.md` 末尾追加了一个未提交的 `## R18 面板轮` 小节（10 行，基线 `7a36a9253`、文档 3332 行），发生在 R17 段之后，不改变本报告对 R17 的行号与结论。`git status` 里 `M loop.md` 是那次追加，不是我改的；我只写了本报告。
- **编号卫生。** `WORKFLOW-VERSIONS.md` 里 C-08/C-09/C-10 出现两组不同含义（R13 段与 R16 段），我在建议里使用未被占用的 C-19–C-23；下一版升版前应先把候选编号合并去重。
- 我没有写 `BLIND-SPOTS.md` 与 `WORKFLOW-VERSIONS.md`；Q1/Q2/Q3 的建议以文本形式留在本文，落盘由主 agent 在 L6.5 决定。

---

## 附录：我重放的命令与读数

```text
# 台账 R17 段解析（python3 -I，按 `## R17 面板轮` 之后的行）
rows 70；IDs 87；unique 87
per-ID 处置: ① 28 / ② 1 / ③ 10 / ④ 48 / ⑤ 0
row-level 处置: ① 21 / ② 1 / ③ 5 / ④ 43
multi-ID rows 10；severity: P0 4 / P1 5 / P2 45 / P3 10 / — 6

# 轨迹门（当前仓库）
python3 tools/eval-trace-check.py <run> --panel-dir .../panel/round17
[WARN] panel: design-d1.md 没有 `## FINDING <ID>` 行
[WARN] panel: refute.md 没有 `## FINDING <ID>` 行
[PASS] panel: 面板报告解析到 81 条发现（含轮次子目录）
[PASS] panel-exemption: design-d1.md 无 FINDING，loop.md 已豁免
[PASS] panel-exemption: refute.md 无 FINDING，loop.md 已豁免
[PASS] loop-sections: loop.md 九个固定标题齐全
[PASS] metrics: 发现总数与处置之和一致（87）
[PASS] ledger: 台账解析到 1120 条处置
[PASS] coverage: 所有面板发现都有处置行
汇总: PASS 9 / WARN 2 / FAIL 0；结论 PASS

# 探针一：把 DESIGN-D2-18 行改名（唯一 ② 消失）
→ coverage PASS；结论 PASS

# 探针二：删掉整个 `## R17 面板轮` 节
→ [PASS] ledger: 台账解析到 1027 条处置
→ [PASS] coverage: 所有面板发现都有处置行
→ 结论 PASS（87 行里没有一行是门真正在查的）

# R17 loop 小节
## R17 面板轮
### §0 基线
### R17 §1 面板
### R17 §2 发现
### R17 §3 归因
### R17 §4 修复
### R17 §5 复验
（无 §6/§7/§8；§5 自引不存在的 §8）

# 文档门（当前树 7a36a9253）
anchor-resolve: anchors=109 resolved=109 zero-def=0 multi-def=0
doc-code-map: 46/46 存在
doc-structure-check: 表结构与内部 .md 链接均可解析

# 测试计数重放
grep -c '#\[test\]': loader.rs 22 / ebs_boundary_audit.rs 2 /
  kernel_info.rs 21 / handoff_v1_write.rs 10
kernel_info 函数名：rejects 9 / accepts 3 / requiring 4 /
  postconditions 4 / getters 1 = 21

# 四条半修的源码核对
set_memmap_len / set_reserved_len: boot_storage.rs:133、147 仍是 pub，生产调用者 0
Box::leak: uefi_helpers.rs:866、dtb_copy.rs:126 仍在
BootShim: boot_shim.rs:38-53 文档已改；impl 仍 1 个（opensbi_helpers.rs）
classified_descriptors: 0 命中；build_memmaps uefi_helpers.rs:384、
  exit_boot_services :1129、UefiPoolAllocator :153 仍在

# ② 与欠账
follow-ups.md:93-96：F-R17-01..04 均无截止轮次
OPEN-DEBT.md:9：“当前无跨 crate 欠账”，与 F-R17-04（跨 crate 契约变更）冲突

# 证据口径
loop.md:855：minix-kernel --features mock 868
defect-ledger.md:1264/1265：minix-kernel 860 通过
loop.md:794（R18）：860 + 8 ignored = 868

# 冻结与提交时间线
ec67e3d4f 03:08:23（冻结，文档 3308 行 sha 544ad466…）
panel/round17/*.md 03:18:55–03:40:08（七份，无 BASELINE.txt）
7a36a9253 03:46:12（修复，文档 3332 行 sha 3b8b0bde…）
窗口内其他提交：无

# 文档内的悬空 ID
01-boot-shim-bootstrap.md:1374、1454：“R17 DESIGN-03”
R17 台账与 panel/round17 均无 DESIGN-03；只存在于 loop.md:757 的散文
```
