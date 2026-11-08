# DECISIONS —— 已裁决与已冻结决策账本

> 用途：防止审查者把“已经裁决过的设计选择”当成新问题重提。
> 规则：命中本账本且没有新证据的发现，在 L2 归因时记为 `re-litigation`，不计入文档缺陷；有新证据的，必须写明证据类型与来源，按正常发现处理。
> 维护：只登记用户裁决过的、或已有多次取证冻结的决策。不要把所有设计选择都塞进来，否则账本会变成第二套规则。

## D-001 四章教学架构不是优化对象

- **决策**：文档的“概述与概念 / C 源码分析 / Rust 设计决策 / 实现详解”四章骨架属于教学架构，不属可优化对象；除非有实质性错误，不得以“叙事更好”为由重排、合并或拆分章节。章内顺序可以调整，但必须逐条记录理由。
- **来源**：`AI-chats/doc-gen-prompt.md:20`；`prompt/DOC-ITER-WORKFLOW.md:510-521` 反模式第 5 条。
- **不要重提**：“把四章改成三章更顺”“把设计章并进实现章”“按调用顺序重排章节”。

## D-002 锚点采用“短锚 + 链接”，链接可点击性不在射程内

- **决策**：正文符号锚点采用短锚加链接的双通道；源文件保留完整路径供工具与评审使用；链接是否可点击不属于本轮目标。禁止使用会在可见文本里带空格或冒号的形态；禁止没有符号目标的链接。
- **来源**：W-11 与 R6，`prompt/WORKFLOW-OPEN-ITEMS.md:153-195`、`342`。
- **不要重提**：“把短锚改回完整路径”“让所有链接可点击”“用脚注替代链接”。

## D-003 保真度标签不得删除

- **决策**：`逐字引用`、`代码引用`、`教学简化`、`有改写`、`反面示例`、`反例（勿这样写）` 等标签是引文与源码之间的可 diff 契约，不得以“看起来像噪声”为由删除。
- **来源**：R5，`prompt/WORKFLOW-OPEN-ITEMS.md:341`；W-11 的标签改名记录。
- **不要重提**：“标签太啰嗦，删掉更干净”“统一成一种标签”。

## D-004 关联代码清单只放 §7

- **决策**：同一份关联代码清单只收拢在 §7；文档头不放清单，也不放指向 §7 的指针。工具去适应文档形态，不要求文档为工具让路。
- **来源**：W-10 与 R7，`prompt/WORKFLOW-OPEN-ITEMS.md:83-96`、`343`。
- **不要重提**：“头部也列一份方便读者”“加一句‘清单见 §7’”。

## D-005 “腿”是受控术语，不是一律禁用

- **决策**：“腿”作为仓库自造比喻，允许在首次出现时给出定义并保持同一含义；禁止的是无定义滥用和把契约违反误标成反事实。不得做全局替换。
- **来源**：W-7 与 W-9，`prompt/WORKFLOW-OPEN-ITEMS.md:51-59`、`73-81`。
- **不要重提**：“全文删掉‘腿’”“统一换成‘分支’”。

## D-006 Kernel 与用户态服务器的执行模型不同

- **决策**：Kernel 是 SMP 加 BKL（自旋锁），共享数据需要 `Arc` 加 `Mutex` 或原子类型；用户态服务器是单线程事件循环，`!Send`、`!Sync`、`Rc`、`RefCell` 合理。不得把用户态服务器的 `Rc<RefCell>` 当成内核侧缺陷，也不得把内核侧的共享可变状态当成用户态风格。
- **来源**：`AGENTS.md` 的 Key Constraints；`prompt/resources/README.md` 第 2 条冲突说明。
- **不要重提**：“所有 `Rc<RefCell>` 都必须消除”“内核也必须用单线程模型”。

## D-007 不引入 pre-commit 钩子

- **决策**：不把 review 门禁挂到 pre-commit；门在 review 流程内触发。理由是钩子会把卫生修复强制塞进提交路径。
- **来源**：OQ5，`prompt/WORKFLOW-OPEN-ITEMS.md:32`。
- **不要重提**：“加一个 pre-commit 自动跑五门”。

## D-008 `CLAUDE.md` 已退役，`AGENTS.md` 是唯一入口

- **决策**：项目指令只从 `AGENTS.md` 与其路由的规范源读取；不再恢复 `CLAUDE.md`，也不在运行时端另立一套规则。
- **来源**：R2，`prompt/WORKFLOW-OPEN-ITEMS.md:338`；`AGENTS.md` 开头说明。
- **不要重提**：“补一份 `CLAUDE.md` 给 Claude Code”。

## D-009 外部参考只借原则，不借结构

- **决策**：`prompt/resources/` 的外部技能与规范只作参考。采纳时借原则、按本项目语境重写；不直接复制文本，不照搬其目录结构。无 license 的条目只作内部参考。
- **来源**：R4，`prompt/WORKFLOW-OPEN-ITEMS.md:340`；`prompt/resources/README.md` 的“引用边界”。
- **不要重提**：“把某个外部 SKILL.md 原样装进来”。

## D-010 已判可冻结的篇目不为风格偏好重开

- **决策**：DOC-ITER 台账中标注“已审·同意冻结”或“判可冻结（篇级）”的篇目，不再因为风格、措辞、结构偏好重开。只有两类情况可以改：出现新证据的事实错误；本轮循环因关联代码或用户指定窗口触碰该篇。重开时必须写明触发条件。
- **来源**：`rewrite-notes/coordination/DOC-ITER-LEDGER.md` 轮次账中的冻结行；W-16 的 H4，`prompt/WORKFLOW-OPEN-ITEMS.md:296`。
- **不要重提**：“这篇虽然冻结了，但我觉得还能再顺一遍”。

## D-011 跨节一致性以主张—依据—适用范围表为唯一源

- **决策**：文档中的关键因果主张必须先登记到主张—依据—适用范围表，再在概述、小结、设计、实现各处复述；任何一处复述与表不一致，都是跨节一致性缺陷。改一处机制，先改表，再全文对齐。
- **来源**：`prompt/DOC-ITER-WORKFLOW.md:206-216`。
- **不要重提**：“凭通读印象判断有没有矛盾”。

## D-012 同 agent 验证可接受，但必须声明并重放命令

- **决策**：P0 的独立验证优先跨 agent；单 agent 可接受，但必须写明“同 agent 验证 + 已重放 grep 命令”，并给出抽样比例与一致性。一致性低于 90% 写 NOT PASS。
- **来源**：`prompt/review-rules/review-process.md:1916-1926`；`AGENTS.md` 的 VERIFY-CHECK 局限条款。
- **不要重提**：“同 agent 验证一律无效”。

## R18 判决账（八字段，P-1 起）

> 格式契约见 `prompt/review-rules/review-evals.md` §3.5。本表从 R18 起使用八列；
> 历史条目（D-001–D-012）保留散文形态。`reviewer_verdict` 与 `user_decision`
> 分列：用户接受风险时前者仍保留 `NOT PASS`。

| decision_id | reviewer_verdict | user_decision | decision_reason | scope | implementation_status | evidence | reopen_condition |
|---|---|---|---|---|---|---|---|
| D-013（R18 D-1） | NOT PASS（开放）：生产者侧 `&'static` 切片是假所有权；A1 方向获第三方评审同意 | 采 A1：`BootStorage`（调用方定长数组）+ `BootReport<'a>` 视图；内核侧 `.bss` 存储不动；分腿迁移 | 长期价值最大且不改 wire；否决 B（只改 U-Boot） | cross-crate | 已实施（第一阶段：UEFI 腿）；U-Boot 腿迁移登记为 F-R18-01 | `BootStorage`/`BootReport` + UEFI storage 路径；minix-boot 184 / boot-shim 135 / kernel 868；QEMU 三门 PASS | 迁移破坏任一起动腿或既有不变量 |
| D-014（R18 D-2） | NOT PASS（开放）：arena wire 分区与语义分区逐区双写，两份内存图可能漂移 | 采 A2：唯一语义来源，wire 从同源编码、接收侧解码；删 arena 语义分区；wire 版本 8→9 + 版本拒绝负控 | 语义区仅服务发送侧跳转前的 `KernelInfo`，可移入 shim `.bss`；不做发送侧往返 | cross-crate | 已实施 | wire v9；语义分区删除（arena 8 页，source@7872、end@30624）；版本拒绝负控；宿主测试 184/135/868 | wire v9 布局或容量不足，或旧版本混用 |
| D-015（R18 D-3） | P2：同一物理池两个 bump 分配器，算法与边界无共同来源 | 采 C+A：`BumpRegion` 纯核心、两独立实例、固定分区；编译期常量断言 + 运行期区间断言；B（合并分配器）否决 | 合并游标已被 QEMU U-Boot 门 3/6 panic 证伪；两实例本来就必须分开 | in-scope | 已实施 | `minix-boot::BumpRegion` + 5 项单测；`prepare_boot` 运行期断言；`cargo test -p minix-boot` 180 通过 | 池分区或分配顺序改变 |
| D-016（R18 D-4） | NOT PASS（开放）：`FileLoader` 返回 `Vec`，强制整份拷贝且接口不表达借用 | 采 A：`FileBytes<'a>`（`Borrowed`/`Owned`）；借用期不可覆盖、源/目标不交检查保留、不长期独占 loader | 显式枚举比 `Cow<'static>` 更贴合跨镜像两条腿；C（静态缓冲）仅作原型 | cross-crate | 已实施 | `FileBytes` Borrowed/Owned；U-Boot 借出、UEFI 拥有；loader 测试与 QEMU 三门 PASS | 借用腿无法满足覆盖检查 |
| D-017（R18 D-5） | P2：arena 生命周期缺端到端证据；未列入 VM 自由页扣除 | 采 C：证明 `LastRead(arena) ≺ Publish(VMFreePages)` 后按生命周期回收；验收 = poison 测试 + U-Boot 装载前不相交检查 | 先证明再决定扣除，避免盲目改内存图；B 的隐含缺陷（扣除区内仍可被内部 allocator 重用）采纳 | in-scope | 已实施 | 生命周期合同（`arch_boot_resume_v1` 文档）+ poison 测试 + `load_kernel_with_loader_gated` 装载前门与编译期 LMA 断言；QEMU 三门 PASS | 时序证明失败，或装载前断言无法建立 |
| D-018（R18 D-6） | P1：兜底 bump 取 memmap 第一个足够大的区间，不排除内核/模块/root | 采 A：删除兜底；未注册即 fail-closed；补“缺注册即失败”负控 | 五条生产入口与 30 个测试内核全部注册，兜底零依赖 | in-scope | 已实施 | `require_boot_region` + 正负控两条；`cargo test -p minix-kernel --features mock` 867 通过 | 出现合法的不注册入口 |
| D-019（R18 D-7） | P1：ELF 源缓冲与目标 LMA 预订重叠无负控 | 采 B：保留 fail-closed；补重叠负控（停止且不进入任何段写入） | `AllocateAddress` 预留已落地；负控固定“检查先于写” | in-scope | 已实施 | `source_destination_overlap_stops_before_any_segment_write`；boot-shim 135 通过 | `AllocateAddress` 预留面变化 |
| D-020（R18 D-8） | 登记项：R7-FU-02 与 F-R16-01 重复，R7-FU-03 已被 A+D 取代 | 合并 R7-FU-02→F-R16-01；R7-FU-03 标“被 A+D 取代”归档 | 保留追溯链接，不再作为活动任务 | workflow | 已实施 | `follow-ups.md` 两行更新 | A+D 预留被回退 |
| D-021（R18 P-1） | 流程缺陷 C-16：用户裁决与独立回读 `NOT PASS` 无固定字段，容易被压成“已修” | 加八字段判决账；`reviewer_verdict` 与 `user_decision` 分列，不覆盖 `NOT PASS` | 判决消失是不可恢复的轨迹损失 | workflow | 已实施 | `review-evals.md` §3.5 + 本表 | 字段被合并或 `NOT PASS` 丢失 |
| D-022（R18 P-2） | 流程缺陷 C-18：五道文档门不查表结构，也不解析跨文档相对链接基址 | 加解析器驱动门：表块连续性 + 内部 `.md` 相对链接（按文档目录）；fenced/行内代码排除；第一版不做锚点；正负 fixture | 出现过“空行劈表五门全绿”和“6 个 .md 链接 1 个断” | workflow | 已实施 | `tools/doc-structure-check.py --self-test` 8/8；01 篇 0 问题 | 出现嵌套表/HTML 表等新形态 |
| D-023（R18 P-3） | 流程顺序：先实施再固定基线，不得自动进入下一轮 | 按九步实施；完成后封存基线并停止；R17-panel 仅用户明确指示后启动 | 带病收敛会浪费轮次；面板必须审同一稳定基线 | workflow | 待实施（基线封存后填） | 本 run 的 `loop.md §7` 待填 | 用户明确启动面板 |
