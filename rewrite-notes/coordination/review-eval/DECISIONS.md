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
| D-013（R18 D-1） | NOT PASS（开放）：生产者侧 `&'static` 切片是假所有权；A1 方向获第三方评审同意 | 采 A1：`BootStorage`（调用方定长数组）+ `BootReport<'a>` 视图；内核侧 `.bss` 存储不动；分腿迁移 | 长期价值最大且不改 wire；否决 B（只改 U-Boot） | cross-crate | 已实施（两腿：UEFI + U-Boot） | `BootStorage`/`BootReport` + 两腿 storage 路径；minix-boot 183 / boot-shim 135 / kernel 868；QEMU 三门 PASS | 迁移破坏任一起动腿或既有不变量 |
| D-014（R18 D-2） | NOT PASS（开放）：arena wire 分区与语义分区逐区双写，两份内存图可能漂移 | 采 A2：唯一语义来源，wire 从同源编码、接收侧解码；删 arena 语义分区；wire 版本 8→9 + 版本拒绝负控 | 语义区仅服务发送侧跳转前的 `KernelInfo`，可移入 shim `.bss`；不做发送侧往返 | cross-crate | 已实施 | wire v9；语义分区删除（arena 8 页，source@7872、end@30624）；版本拒绝负控；宿主测试 184/135/868 | wire v9 布局或容量不足，或旧版本混用 |
| D-015（R18 D-3） | P2：同一物理池两个 bump 分配器，算法与边界无共同来源 | 采 C+A：`BumpRegion` 纯核心、两独立实例、固定分区；编译期常量断言 + 运行期区间断言；B（合并分配器）否决 | 合并游标已被 QEMU U-Boot 门 3/6 panic 证伪；两实例本来就必须分开 | in-scope | 已实施 | `minix-boot::BumpRegion` + 5 项单测；`prepare_boot` 运行期断言；`cargo test -p minix-boot` 180 通过 | 池分区或分配顺序改变 |
| D-016（R18 D-4） | NOT PASS（开放）：`FileLoader` 返回 `Vec`，强制整份拷贝且接口不表达借用 | 采 A：`FileBytes<'a>`（`Borrowed`/`Owned`）；借用期不可覆盖、源/目标不交检查保留、不长期独占 loader | 显式枚举比 `Cow<'static>` 更贴合跨镜像两条腿；C（静态缓冲）仅作原型 | cross-crate | 已实施 | `FileBytes` Borrowed/Owned；U-Boot 借出、UEFI 拥有；loader 测试与 QEMU 三门 PASS | 借用腿无法满足覆盖检查 |
| D-017（R18 D-5） | P2：arena 生命周期缺端到端证据；未列入 VM 自由页扣除 | 采 C：证明 `LastRead(arena) ≺ Publish(VMFreePages)` 后按生命周期回收；验收 = poison 测试 + U-Boot 装载前不相交检查 | 先证明再决定扣除，避免盲目改内存图；B 的隐含缺陷（扣除区内仍可被内部 allocator 重用）采纳 | in-scope | 已实施 | 生命周期合同（`arch_boot_resume_v1` 文档）+ poison 测试 + `load_kernel_with_loader_gated` 装载前门与编译期 LMA 断言；QEMU 三门 PASS | 时序证明失败，或装载前断言无法建立 |
| D-018（R18 D-6） | P1：兜底 bump 取 memmap 第一个足够大的区间，不排除内核/模块/root | 采 A：删除兜底；未注册即 fail-closed；补“缺注册即失败”负控 | 五条生产入口与 30 个测试内核全部注册，兜底零依赖 | in-scope | 已实施 | `require_boot_region` + 正负控两条；`cargo test -p minix-kernel --features mock` 867 通过 | 出现合法的不注册入口 |
| D-019（R18 D-7） | P1：ELF 源缓冲与目标 LMA 预订重叠无负控 | 采 B：保留 fail-closed；补重叠负控（停止且不进入任何段写入） | `AllocateAddress` 预留已落地；负控固定“检查先于写” | in-scope | 已实施 | `source_destination_overlap_stops_before_any_segment_write`；boot-shim 135 通过 | `AllocateAddress` 预留面变化 |
| D-020（R18 D-8） | 登记项：R7-FU-02 与 F-R16-01 重复，R7-FU-03 已被 A+D 取代 | 合并 R7-FU-02→F-R16-01；R7-FU-03 标“被 A+D 取代”归档 | 保留追溯链接，不再作为活动任务 | workflow | 已实施 | `follow-ups.md` 两行更新 | A+D 预留被回退 |
| D-021（R18 P-1） | 流程缺陷 C-16：用户裁决与独立回读 `NOT PASS` 无固定字段，容易被压成“已修” | 加八字段判决账；`reviewer_verdict` 与 `user_decision` 分列，不覆盖 `NOT PASS` | 判决消失是不可恢复的轨迹损失 | workflow | 已实施 | `review-evals.md` §3.5 + 本表 | 字段被合并或 `NOT PASS` 丢失 |
| D-022（R18 P-2） | 流程缺陷 C-18：五道文档门不查表结构，也不解析跨文档相对链接基址 | 加解析器驱动门：表块连续性 + 内部 `.md` 相对链接（按文档目录）；fenced/行内代码排除；第一版不做锚点；正负 fixture | 出现过“空行劈表五门全绿”和“6 个 .md 链接 1 个断” | workflow | 已实施 | `tools/doc-structure-check.py --self-test` 8/8；01 篇 0 问题 | 出现嵌套表/HTML 表等新形态 |
| D-023（R18 P-3） | 流程顺序：先实施再固定基线，不得自动进入下一轮 | 按九步实施；完成后封存基线并停止；R17-panel 仅用户明确指示后启动 | 带病收敛会浪费轮次；面板必须审同一稳定基线 | workflow | 已实施（基线 `10ecd8ef4` 已封存；未进面板） | `loop.md` R18 §2 读数；宿主 184/135+2/868，QEMU 三门 PASS | 用户明确启动面板 |
| D-024（R20 meta） | RB20/R20 meta 的 `NOT PASS` 与半修清单 | 保留 `NOT PASS` 原文；六条 ④ 未修项补修（A-07/A-10/B-08/B-14/B-17/B-18）、§5.2.1 改“整条丢弃”、D1 停跑全量、F-R20-01..03 移入 OPEN-DEBT | 判决不得被“已修”覆盖；机制类盲区 B-54/B-55/B-56 落盘 | workflow | 已实施（R20 收口） | `meta/01-boot-shim-bootstrap-r20.md`、`defect-ledger.md` R20 meta 段、`BLIND-SPOTS.md` B-54..B-56 | 出现新的范围行漏项 |
| D-025（R22 meta） | R15 `READER-A-10/A-12/A-27` 的 ④ 证据闸 | 三行确认为“不改裁决”，改 ③ 并在复验列写保留理由；④ 证据闸对 ③ 不要求代码/测试反证 | 避免历史 ④ 行被机械闸反复点名 | workflow | 已实施 | `defect-ledger.md` R15 三行、`meta/...-r22.md` 五次复核 | 新证据显示该三项应修 |
| D-026（R23 前置） | F-R17-01（单实现 `BootShim` trait）与 F-R17-04（`KernelInfo` 不携带权威）均为 P2，挂着“待用户裁决”，按用户规则阻塞 R23 | **用户未答复结构化提问；agent 按两条推荐项代裁，可回退**：删 trait，U-Boot 腿改 `opensbi_helpers::prepare_boot` 普通函数；`KernelInfo` 13→14 加 `authority`，`validate_requiring_sources()` 改无参 | trait 无测试接缝、无 `dyn`、唯一实现是生产腿，属于死抽象；权威进结构体把“图与声明”合成一个值，消掉两个可接反的实参 | cross-crate | 已实施（R23 前置） | `grep -rn "impl BootShim" os/` 零命中；`validate_requiring_sources(` 无参调用 9 处；`minix-boot` 192、`boot-shim` 135、`minix-kernel` 862+8、`minix-arch` 287；x86/aarch64/riscv64 三 QEMU 门 PASS；文档 §3.2/§3.4.1 已同步 | 出现第二条固件腿需要统一接口，或 `authority` 绑定破坏任一条腿 |

| D-027（BOOT-SCOPE 2a） | 跨镜像腿 boot 恒等映射铺低 4GB、boot-shim 建内核高半；用户指出 boot 应只建临时最小页表 | 采 2a：boot-shim 只建最小 identity 窗口（shim image/stack、内核 LMA、root/bump/arena、模块、平台表、设备窗口，2 MiB 粒度）+ 内核高半，跳高入口不变；Direct Map 仍归 kernel；2b（低入口、kernel 建全部）不采；2c（维持 4GB）拒绝 | 用户 2026-11-09 补充：256/512MB 量级即可，够 boot 工作；4GB 只是旧说法。2a 改动面小、直核腿已证明 kernel 建表可行，且叙事与 Minix3 “boot 表是脚手架”对齐 | cross-crate | 已实施（`BootWindows`；QEMU 六门 PASS；kernel 861+8） | `DECISIONS-PENDING-BOOT-SCOPE.md` §8；`loop.md` BOOT-SCOPE 节；doc 01 §1.2/§4.6 `[ARCH]` | 窗口外出现 boot 访问 → 加入窗口表；不得回退 4GB 全铺 |

| D-028（R23 后清账） | 01 相关三条待裁决项（bootface 重复、`kern_ident` 恒空、`pt_alloc` release 语义）与三条 F-R18 观察项 | 用户 2026-11-09 要求“和 01 文档相关的 todo 无论 P 几全部完成”；按默认建议实施：bootface 共享 helper；删除 `kern_ident` + wire 8→9；`register` 全构建 panic；doc 门补锚点/外链；`classify_regions` 复用生产成员源；新增并发 register 测试 | 全部是 01 评审转出的确定项，不需要再等默认建议；用户指令即批准 | cross-crate | 已实施 | `follow-ups.md` 全部 ✅；`OPEN-DEBT.md` F-R23-03 ✅；minix-types/kernel/VM/boot-shim/arch 测试通过 | 若并发测试或 wire v9 破坏某条腿，回退对应项并重开 |

| D-029（TODO-09） | 01 §6.1 建根返回后、跳转前缺发送侧路标；三次打印尝试在 QEMU 串口均不可见 | 用户裁决：不强制增加该诊断打印；不为一个诊断信息过度设计。保留现有 `[raw] entering kernel image v1` 与接收侧 `###` 两端的定位能力，A→C 间隙由 QEMU `-d int` 覆盖 | CR3 切换后 shim 侧不可观测是 PE/CR3 架构特性（三次尝试实证），修缺口风险大于收益 | in-scope | 已关闭（不做） | doc TODO-09 ✅；`loop.md` 清账节 | 若真机排障需要该间隙证据，改用 QEMU `-d int` / GDB 而非加打印 |

| D-030（R24 DESIGN-01） | `KernelLayout::ident_regions()` 恒空、`map_kernel` 只为断言为空收参数，形成三层空通道 | 删除方法、参数与断言语义；VM 解码与调用点同步 | 类型系统已表达“没有这个输入”；保留空接口只会诱导未来误用 | cross-crate | 已实施 | `grep -rn 'ident_regions' os` 零命中；`map_kernel` 7 参；kernel 865+8 / vm 548 通过 | 出现真实非空 identity 需求时重新引入 |
| D-031（R24 DESIGN-02） | 平台描述符窗口只取地址所在 2 MiB 片，跨边界 blob 只有半片在恒等窗口 | **补长度**：`PlatformDescSource` 增加 `len`（占原 wire 16 B 的 padding，`0` = 旧生产者回退单片）；DTB 读 `totalsize`、RSDP 读 `length`；`push_platform_windows` 按跨度建窗 | 用户裁决明确“补长度”，不得以“消费者在 DM 之后”代替；wire 尺寸不变，旧 `0` 仍可读 | cross-crate | 已实施 | `PlatformDescSource::{with_len,len,has_len}`；`dtb_total_size`/`rsdp_total_size`；wire 写读 `off+4`；`push_platform_windows` 跨度 + `platform_window_uses_the_recorded_blob_length`；minix-boot 196 / kernel 865+8 通过 | 出现无法在 boot 前读取长度的新 blob 类型 |
| D-032（R24 DESIGN-03） | `BootWindows::push` 对反序/零长度静默丢弃，把错误推到 enable 后取指 | 改 fail-fast：`start < end` 否则 panic；`push_2m` 用 `checked_add` | 当前调用点无一处依赖静默丢弃；可诊断的启动错误优于缺页 | in-scope | 已实施 | `boot_windows_reject_reversed_ranges`、`boot_window_push_2m...`；kernel 865+8 通过 | 出现合法的空窗口调用点 |
| D-033（R24 DESIGN-04） | Step 0 仍要求 `phys_addr_limit >= IDENTITY_MAP_END`，引用已退役的 4 GiB 全铺前提 | **按窗口表推导**：移除 Step 0 的全局上界断言；窗口组装后 `assert_boot_windows_addressable` 逐窗比较 `end <= phys_addr_limit` | BOOT-SCOPE 2a 的最小窗口只需各自可寻址；保留全局断言会拒绝低物理地址合法平台 | in-scope | 已实施 | `assert_boot_windows_addressable`；`boot_windows_{accept,reject}_ranges_*`；kernel 865+8 通过 | 窗口表之外又出现 boot 访问 |
| D-034（R24 DESIGN-05） | UEFI 腿 `BootReport` 的 source/startup 为空切片，类型与 U-Boot/直核腿相同，未来消费者可能误判 | 加 `records_in_wire: bool` **咨询标记**（当前无消费者、无运行期强制）；UEFI 发送侧置 true；文档写明当前是生产者契约 | 零行为变化；它是给未来消费者的显式提示，不是类型级强制 | cross-crate | 已实施 | `boot_storage.rs` 字段 + UEFI 置位；doc §3.4.5；minix-boot 196 通过 | 出现新的部分视图腿 |

| D-035（R25 DESIGN-02） | 窗口物理上限检查靠两次自由函数调用，第三个调用者可绕过 | 保留现状：`prepare_kernel_image_v1`/`arch_boot_impl` 两处成对调用 `assert_boot_windows_addressable`，由 `boot_windows_*` 测试钉住；不引入 `CheckedWindows` 新类型 | 当前只有两个生产调用点，类型化改动的收益是防未来误用，代价是改两处启动 API 与测试；留待出现第三调用者再升级 | in-scope | 已决定（保留） | `os/kernel/src/lib.rs:496`、`:975`；`boot_windows_accept/reject_ranges_*` 测试 | 出现第三个 `build_bootstrap_root_and_enable` 调用者 |
| D-036（R25 DESIGN-09） | 两个直核面各自维护 `static mut` 平台源/模块表/param/startup 落地区 | 保留现状；迁移到 `BootStorage` 需要核实 `.bss` 余量、DTB 表可复制性与零复制约束，未完成前不改 | D-013 只要求 UEFI/U-Boot 两腿迁移；直核面每镜像 +12 KiB `.bss` 与一次内存图复制的代价未核实 | in-scope | 待核实（R26 承接） | `bootface.rs`/`bootface_a64.rs` 的 `static mut`；`BootStorage` 尺寸与 `.bss` 读数未采集 | 核实显示 `.bss` 有余量且零复制可放弃 |
| D-037（R25 DESIGN-10） | `BootWindows.ranges` 仍存裸 `(u64,u64)`，未用 `PhysRange` | 接受面板建议但本轮不实施；登记为 R26 的设计清理项 | 改用 `PhysRange` 需给 `PhysRange` 补 const 空值/构造并改两处窗口循环，收益是类型化地址对，非行为缺陷 | in-scope | 待实施（R26） | `design-d2.md` DESIGN-10；`lib.rs:1064-1067` | R26 仍未实施则重新裁决 |

| D-038（R25 用户裁决） | ③ 被当作合法延期桶；R25 26 条 ③ 中 19 条无 §4.1 反证 | 删除 ③；误报改 `FP` 且必须有源码/测试/文档原文反证；无证据按 `⑤` 未闭环；`②` 必须有“用户批准 + YYYY-MM-DD”；R25+ metrics 必须有 `未闭环: 0`，否则门 FAIL | 26 轮中反复出现“发现→③ 延期→下一轮再发现”；用户明确要求删除 ③ | workflow | 已实施（V2.3） | `review-evals.md` §4.1/§4f/G5、`review-loop.md` L2/L5/L6.9、`eval-trace-check.py` 自测 PASS；R25 scoped 现 FAIL（未闭环 10） | 若再出现“换名桶”（如 FP 被滥用）则回退/收紧 |

| D-039（R25 用户裁决） | 全量发现轮反复生成 P2/P3/读者偏好，G8“连续两轮零新增”不可达；R24→R25 发现量 118→124 | 改轮型状态机：discovery 只跑一次，清空后进入 verification；verification 只查修复 hunk/未闭环项/随机 20%，零新 in-scope 真缺陷才 CONVERGED；同一验证轮内最多 2 次 delta，第 2 次仍有新缺陷即 STUCK；上一轮未闭环 >0 时禁止出现下一轮小节；收敛后 FREEZE | 用户明确要求“把流程往收敛改”，不再接受靠下一轮拖延；每轮全量面板本身制造新偏好 | workflow | 已实施（V2.4） | `review-evals.md` §4.0a/§6A、`review-loop.md` L0/L1/L6.9/L7、`review-panel.md` §3.5、`eval-trace-check.py` 自测 PASS | 若 verification 仍反复产生新 P0/P1，说明文档/实现本身未稳定，转 STUCK 交用户而非继续加轮 |
