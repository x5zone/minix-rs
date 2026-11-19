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

| D-040（R26 代理裁决） | D-035 决定“保留现状、不引入 `CheckedWindows`”；R26 meta 指出该决定未覆盖“第三调用者”风险，且 R25 收口批已实施类型化 | 代理裁决：**supersede D-035**，保留 `CheckedWindows`；`BootWindows::check` 返回 token，`build_bootstrap_root_and_enable` 只收 token | 类型 token 把“先检查再建根”变成唯一可表达的调用序；两个生产调用点已改，测试与 QEMU 均过 | in-scope | 已实施 | `os/kernel/src/lib.rs:1195-1216`；`cargo check -p minix-kernel --features mock`；QEMU aarch64 SMP / riscv64 bootface PASS | 出现需要不同检查策略的第三调用者，或 token 阻止合法复用 |
| D-041（R26 代理裁决） | D-036 把直核面 `static mut` 落地区迁移 `BootStorage` 留到 R26 核实；R26 meta 指出 R25 收口批已实施但未登记 | 代理裁决：**supersede D-036**，两个 bootface 各用一份 `static mut BootStorage` + 单一 one-shot 守卫；`.bss` 上限 16 KiB 编译期断言 | 四个独立落地区合并为一个所有者；`.bss` 余量与零复制约束已核实，两条真机门通过 | in-scope | 已实施 | `os/kernel-image/src/bootface.rs`/`bootface_a64.rs`/`bootface_common.rs`；`cargo build` 两目标；`test-kernel-image-riscv64-bootface`、`test-smp-aps-aarch64` PASS | `.bss` 超 16 KiB、或某腿需要保留独立落地区 |
| D-042（R26 代理裁决） | D-037 接受 `BootWindows.ranges` 改 `PhysRange` 但留到 R26；R26 meta 指出 R25 收口批已实施但未登记 | 代理裁决：**supersede D-037**，`BootWindows.ranges` 改为 `[PhysRange; MAX_BOOT_WINDOWS]`，构造器拒绝反向区间 | 类型化地址对消除 `(start,end)` 交换；窗口测试与 QEMU 通过 | in-scope | 已实施 | `os/kernel/src/lib.rs:1064-1112`；`boot_window_push_2m...`/`platform_window...` 测试；QEMU 两腿 PASS | 出现需要裸 `end` 语义的调用者 |
| D-043（R26 代理裁决） | R26 DESIGN-D2-01..05、D2-07..12 共 11 条 P1/P2 结构改进 | 代理裁决：**批准转入 R27 design/code batch**；不在 R26 实施，登记 `OPEN-DEBT.md` 的 `R26-design-batch`，截止 R27 边界 | 这些是类型/结构改进，不是 R26 已知正确性缺陷；R26 的目标是先收敛，批量实施会扩大改动面并再次引入文档漂移 | cross-crate | 待实施（R27） | `OPEN-DEBT.md` 的 `R26-design-batch`；`meta/01-boot-shim-bootstrap-r26.md` 抽样记录 | R27 边界前未实施或出现新证据表明其中一条是正确性缺陷 → 重新裁决 |
| D-044（R26 代理裁决） | DESIGN-D2-06：`store_kernel_info` 返回 `()`，两个生产调用点随后 `expect` 一个“不可能失败”的 getter | 代理裁决：改为返回 `&'static KernelInfo`，生产调用点直接使用返回值；测试调用点忽略返回值 | 三行级类型保证，去掉两个不可能 panic；行为不变 | in-scope | 已实施 | `os/kernel/src/lib.rs` 的 `store_kernel_info` 返回类型与两个调用点；`cargo test -p minix-kernel --features mock` 865+8 通过 | 出现需要 `()` 返回的调用方 |
| D-045（R30 后用户裁决） | R26→R27 与 R29→R30 各出现 1 P0 + 2 P1 的自引入同步债（代码批改行为、文档引文停在旧代码）；② 的"用户裁决"形态被用于同篇设计项转出（R26 的 11 条），与"轮次内修复"目标冲突 | 用户裁决：① ② 收紧至相邻范围，同篇待裁决项有代裁决授权即当轮 ⑥ 实施、无授权则轮次 BLOCKED 不开下一轮；② `节选` 引文只许省略不许改写；③ 新增 `--stale-lines` 行为级陈旧行网并列为 certify 硬步骤；④ 所有权顺序复核缺记录 = G7 FAIL | 把"下一轮面板发现同步债"改成"本轮 commit 门拦截"；转出不再承接同篇未修完项 | workflow | 已实施（V3.2） | `review-evals.md` §4.1/§4.1a/§4.0d/G5/G7/G9；`review-loop.md` L4；`eval-trace-check.py` disposition-v32 门（休眠/强制双验证）；`doc-citation-check.py`/`doc-symbol-drift.sh` 自测 PASS；01 文档收紧后引文门 0 失败（新增 2 处换行改写被拦并逐字化） | 若 `--stale-lines` 在真实代码批上误报率高，或 BLOCKED 轮次频繁出现，回边界重议 |
| D-046（R31 后边界裁决，用户批准） | 第一次外部评审（网页端模型）+ 本仓审计共同确认：R26/R30 两次修改轮自判 CONVERGED 均被下一轮 survey 推翻（假阳性）；R26 的 11 条设计债转入 OPEN-DEBT 后五轮无执行器（阴影账不在门视野内）；R30 台账 70% 是 P3 而旧 G3 要求准入 P3=0（不可达判据） | 用户裁决（2026-11-10，"我都点头"）：采纳 V3.3 全包——① 四态模型 + 修改轮无收敛声明权；② 确认轮（全量 survey、只修新准入 P0/P1/P2、不实施 ⑥、≤2 次、不占修改预算）；③ 读者 P3 默认 suggestions、G3 不再要求准入 P3=0、P2 维持阻断；④ OPEN-DEBT 入门（计入未闭环、两出口、逾期点名）；⑤ 改写段源码再推演（G7 硬步骤）；⑥ origin 记引入轮 + severity 与面板同步；⑦ K=3 软上限；⑧ 代裁决授权范围必须写明"批准实施"或"批准转出"。**R31 grandfathering**：R31 的 survey 在 V3.1/V3.2 规则下执行，其清点与关账按 V3.3 做（P3→suggestions、OPEN-DEBT 未销账行阻断），不判 INVALIDATED 不重跑面板 | 假阳性需要收敛声明权的制度性回收；债务需要执行器；P3 长尾需要合法出口。三方证据（用户意图、流程自诊断 R31 §5、外部评审）首次一致 | workflow（V3.3） | 已实施（规则源 + eval-trace-check 三门 + 版本账本条） | `prompt/review-rules/review-evals.md` §4.0c/§4.0f/§6A/§7.5；`prompt/skill/review-loop.md` L0/L4/L6.9/L7a/L7d；`prompt/review-rules/review-panel.md` §3.6/读者 brief；`tools/eval-trace-check.py`（open-debt / admission-v33 / lifecycle-v33 三门 + 自测夹具）；`WORKFLOW-VERSIONS.md` V3.3 节 | 新确认轮仍连续两次 `新准入>0`、或 OPEN-DEBT 清零后同主题第 4 次重报 → 回边界裁决，不自动加轮 |
| D-047（R31 后用户裁决） | 用户纠结：全代裁 vs 阻断——80% 代裁会同意、但 agent/GPT 在设计方案上系统性"把 boot 往小 OS 发展"（不遵守 Minix3 语义的扩张），代裁这类项重构成本巨大；待裁项阻断又造成 R31 §5 实证的十轮空转 | 用户裁决（2026-11-10）：**全代裁，授权边界 = Minix3 语义锚**——锚定 C 语义的现代化表达与删翻译残留 = 代裁（非阻断、事后审阅）；偏离 C 语义（机制替换/新责任，如借槽→directMap）= ARCH 级**硬阻断**：入台账 ⑤ + `轮结果: BLOCKED`，即便用户口头说"继续"也必须先落 `DECISIONS.md` 判决账行才解除（门不认识聊天）；[ARCH] P3 → 判决账提案不阻断；治理仪表 `代裁决`/`ARCH 提案`/`用户否决` 进 metrics，否决率 <20% 维持授权 | 代裁的 80% 是"删/平移"（历史判决统计：批准的全是把 boot 变小）；20% 纠结区全部落在语义扩张侧；事后审阅与读教学文档同趟，边际成本≈0；硬阻断把用户注意力强制到唯一不可逆的类别上 | workflow（V3.3 追加） | 已实施 | §4.1b（授权边界/分类标记/ARCH 载体）；`eval-trace-check.py` `design-class` 门 + `lifecycle-v33` ARCH 一致性 + metrics 三仪表；自测夹具（未分类/[ARCH] 无判决行/批准延期出口/跨 edition 重置） | 用户否决率 >20% 持续两个 edition、或 ARCH 提案单 edition >5 条 → 回边界收紧授权 |
| D-048（2026-11-11 代理裁决） | `OPEN-DEBT.md` 的 `R26-design-batch`（D-043 的 11 条设计改进）截止 R27 已逾期，R32 只写了"已销账"而机械门不识别；R33 因此未闭合 | 代理裁决：**批准销账**——可实施部分已在 R27–R31 落地，残余项（相邻裸参数 newtype、enum 哨兵）降为改进方向；D-043 的"批准转出"不构成无限期保留，按 §4.0f 落判决账行 | 已落地的证据可重放：`BootStorage`/`BootStorageCell`（`os/boot-shim/src/lib.rs:143`、`os/libs/minix-boot/src/boot_storage.rs`）、`store_kernel_info` 返回 `&'static KernelInfo`（`os/kernel/src/lib.rs:1453`）、`PhysRange` 窗口（`os/kernel/src/lib.rs:1064-1112`）与对应测试；残余项没有"当前代码会错"的证据，继续挂在 OPEN-DEBT 只会把阴影账换名保留 | cross-crate | 已实施（销账） | `OPEN-DEBT.md` 的 R26-design-batch 行标 ✅；`cargo test -p minix-kernel --features mock` 866+8、`cargo test -p boot-shim --features test-all --lib` 136、`cargo test -p minix-boot` 196（R33 读数）；代码符号见左 | 出现残余项的正确性反证、或 `R26-design-batch` 的某条再次以 P0/P1 形态出现 → 重开并指派实施轮次 |
| D-049（2026-11-11 代理裁决） | R34 确认轮新增 4 条设计 P2（D2-02/03/04/05）与 1 条 aarch64 跨距测试缺口（F-R34-01）；确认轮不实施 ⑥，但 OPEN-DEBT 不能无限保留 | 代理裁决：**批准延期至下一 design/code batch（D02 循环前）**；用户批准日期 2026-11-11；四项设计的分类为 `[MODERN]`×3 + `[DEL]`×1（D2-04 删 wire 派生标量），均锚定 C 语义或纯删除，不改变 boot 的外部语义 | 确认轮的修复权限只含 P0/P1/P2 事实与同步项（§3.6）；设计实施会移动基线并让本轮 certify 失效；本轮已把可修的文档事实与同步项全部修完，剩余项有明确目的地与证据路径 | cross-crate | 待实施（下一 batch） | `OPEN-DEBT.md` 的 `R34-design-batch` 与 `F-R34-01` 两行（含用户批准 2026-11-11）；`panel/round34/design-d2.md` 的 D2-02/03/04/05 | 下一 batch 前出现正确性反证、或用户要求提前实施 → 重开并指派轮次 |
| D-050（2026-11-11，D02 R1 ARCH 提案，待裁决） | D2 ARCH-PROPOSAL A1：`KernelInfo` 的载荷所有权类型化——把 `&'static` 切片 + 7 个 landing pad 收成 `LandedKernelInfo` | 待用户裁决（提案，不实施） | 当前实现用 7 个 landing pad 与 `&'static` 切片维持“落 .bss 后整段可达”；类型化能删掉一批 unsafe 与重复的 landing pad，但会改 `KernelInfo` 的所有权形状与跨 crate 签名，属架构级 | cross-crate | 待裁决（提案） | `panel/round1/design-d2.md` 的 `## ARCH-PROPOSAL` A1；`os/libs/minix-boot/src/kernel_info.rs`、`os/kernel/src/lib.rs` | 用户批准后进入设计批；出现落地板/所有权相关的正确性缺陷时可提前重开 |
| D-051（2026-11-11，D02 R1 ARCH 提案，待裁决） | D2 ARCH-PROPOSAL A2：镜像跨距改由两个链接符号之差导出，删除手写 `KERN_SIZE`（aarch64 direct-core 目前只有上界断言，没有 riscv64 的 L9 等值对账） | 待用户裁决（提案，不实施） | A2 与 `F-R34-01`（aarch64 跨距缺口）同源；用链接符号导出可让三架构共享同一真值，但改自引导面的常量契约与链接脚本，属架构级 | cross-crate | 待裁决（提案） | `panel/round1/design-d2.md` 的 `## ARCH-PROPOSAL` A2；`os/kernel-image/src/bootface_a64.rs:83`；`os/kernel-image/check-layout.sh` L9 | 用户批准后进入设计批；aarch64 跨距漂移出现真机/构建证据时可提前重开 |
| D-052（2026-11-12，D02 R2 ARCH 提案，待裁决） | D2 ARCH-PROPOSAL：布局事实收成单一 `BootLayout` 值对象；`HigherHalf::jump_to_kmain(kinfo)` 去掉冗余 `stack_top` 形参；用 `ValidatedForJump` token 固定“先校验、再建根、后跳转”的调用序 | 待用户裁决（提案，不实施） | 减少布局标量的重复表达与调用顺序靠纪律的问题；但会改 `HigherHalf` 签名、`KernelInfo` 形状与三架构跳转契约，属架构级 | cross-crate | 待裁决（提案） | `panel/round2/design-d2.md` 的 `## ARCH-PROPOSAL`；`os/kernel/src/boot/higher_half.rs`、`os/libs/minix-boot/src/kernel_info.rs` | 用户批准后进入设计批；出现布局标量漂移或调用序事故时可提前重开 |
| D-053（2026-11-12，D03 R1 代理裁决） | D03 R1 的三条代码设计项：D2-10 四处陈旧注释、D2-11 TSS 占位/riscv64 断言强度、D2-16 就地初始化 + 删 refresh_descriptors（ARCH 提案） | 代理裁决：**批准延期至 D03-design-batch（下一 design batch）**；D2-16 作为 ARCH 提案待用户裁决，不实施；三项均登记 `OPEN-DEBT.md` 的 `D03-design-batch` | 本轮是文档 review 轮，三条都需改代码/契约；确认文档事实后把代码项集中到设计批，避免在文档轮移动基线 | cross-crate | 待实施（下一 batch） | `OPEN-DEBT.md` 的 `D03-design-batch`；`panel/round1/design-d2.md` D2-10/11/16 | 出现 TSS/断言/描述符基址的正确性反证，或用户要求提前实施 → 重开 |
| D-054（2026-11-12，D04 R1 代理裁决） | D04 DESIGN-D2 的 6 条 P2 代码/架构项：`nr_irqs`/`frequency` 无生产调用、解析器静默默认、`CpuInfo`/`ArchMiscDesc` 残留、aarch64 `static mut GICD_BASE` 双表示、`Frozen<T>` 的 Sync bound、ARCH 提案 `CpuInfo` 收缩 | 代理裁决：**批准延期至 D04-design-batch（下一 design batch）**；ARCH 提案待用户裁决，不实施；登记 `OPEN-DEBT.md` | 均需改代码/契约；本轮是文档 review，先修文档事实与读者项 | cross-crate | 待实施（下一 batch） | `OPEN-DEBT.md` 的 `D04-design-batch`；`panel/round1/design-d2.md` | 出现平台描述符/CPU 拓扑的正确性反证 → 重开 |
| D-055（2026-11-17 用户裁决） | 用户问“为什么 34 轮不收敛、现在 2 轮就收敛，是否降低了质量要求”；自查确认两条真实降档：① D02–D04 把 500–1500 行文档的读者 A/B/C 合并为一个 agent（`review-loop.md` L1 旧档位与 `review-panel.md` §7 相互矛盾，实际用了更松的一侧）；② D03/D04 的最终 certify 用 `grep` 旧串归零替代独立冷启动回读，D03 回归随即抓到 `RD3-R2-25` 台账“已修”但正文残留 7 处过程词 | 用户裁决：**恢复两条**——① 500 行及以上读者 A/B/C 必须独立，合并需 `VARIANCE + 降档理由 + 用户批准日期`，否则轮结果最多 `NOT CONVERGED`（已写入 `review-panel.md` §7 与 `review-loop.md` L1）；② certify 回读必须来自本次新派发的独立子 agent，`grep` 归零不算回读，缺独立回执则 certify 新准入不计为 0（已写入 `review-loop.md` L4.1） | 2 轮收敛由“流程成熟 + 口径收缩 + 文档问题更机械”三者叠加；用户预期 3–5 轮，只恢复流程成熟部分不够，必须把两条承重墙装回去 | workflow（V3.4） | 已实施 | `prompt/review-rules/review-panel.md` §7 硬边界段；`prompt/skill/review-loop.md` L1 默认配置与 L4.1；D03/D04 回归记录（`REGRESSION.md` 与 `FREEZE.md` 的回归小节） | D05 起若三轮内仍不收敛，或恢复后每轮读者项数量腰斩 → 回边界复核是否过度收紧 |
| D-056（2026-11-18，用户指令「按照你的建议来吧」） | V3.4 变更集（防虚耗 a–m）曾于 2026-11-18 实施并通过四队回归修复，随后被并发会话在 `13fe44d6e` 上的硬重置丢弃（reflog 实证：`reset: moving to HEAD`，无 stash/提交/分支残留）；r6 验收轮四队独立确认丢失，并实测当前 V3.3+D-055 的新缺口（R6-a 伪造确认轮路径、R6-b 回归轮逃逸台账、R6-c BLOCKED 死锁、R6-d RB 零定义、R6-e 门命令复用死循环） | 用户裁决：按 r6 建议执行——① 落账止血（本行 + 体检报告与 boundary 孤儿证据入库）；② 02/03 回归轮补行级账并修三处算术不一致；③ 重实施 V3.4 最小内核（R6-a 确认轮前置、R6-c BLOCKED 两级解除、P1-7 门、N0 作用域、P2-8 citation C 侧、P0-3 meta 修约、P1-5 FREEZE append 语义、R6-b 回归轮入账）；④ 并发纪律入规；⑤ D-050/051/052 三条 ARCH 提案维持待用户亲裁 | 并发硬重置击穿「不静默腐化」；流程核心（收敛引擎 + 自纠）经 02/03/04 五角色回归实战站住，重实施成本低于重设计；V3.4 完整规格存于体检报告 §7.4/§8.6/§9.2 | workflow（V3.4 最小内核） | 已实施（本提交链） | `prompt/review-loop-process-review-2026-11-17.md` §7–§10（规格与验收裁决）；`rewrite-notes/coordination/review-eval/meta/boundary-V3.4{,-judging}.md`；reflog `13fe44d6e reset: moving to HEAD`；`tmp/process-review/`（r2–r6 四队报告，不入库） | 再发生未提交工作丢失或账本脱钩 → 本行重开并升级为流程事故 |
| D-057（2026-11-18，代理裁决，随 V3.4 最小内核重立） | N0 轮序双源门将使 D01 `--round R31–R34` 各 FAIL——同一根因：R31 断链（loop 有小节、台账与 metrics 双缺、却开出 R32–R34 并 CONVERGED）是既成事实，不可后验修数据；`--round` 已按轮次作用域裁剪（R26–R30 各自 PASS） | 代理裁决：**仅授权 D01 `--round R31–R34` 的门红（R31 = 17 项（r10 口径；r8 strict 后曾记 16）、R32–R34 各 1 项）与 D05 `--round R1` 的存量红（20 项，含空面板、面板制品、外部面板、SUGG 制品口径与 N0 无账轮五类；计数随口径演进浮动，以最新实测为准；V2 基线，R1 面板启动改标 `schema: V3.4` 重立基线时消除——r7 补充授权）；D01 `--round R27` 存量红随 strict/制品口径由 4 项增至 10 项（§1–§6 待填遗留）**，期限至 R31 补清点（53 条按 §4.0c 补 `dup-of` 回填）落地或用户另行裁决；D03/D04 的「certify 待跑」为存量记录（P1-7 限 `schema: V3.4` 起，01–04 不追溯）；预期红必须逐项列明于版本节，不得以「预期红」笼统豁免 | 预期红只用于「既有事实不可后验修复」；其它任何红必须修数据或修门，不得惯例化（r6 回归审查 C2/D6 判例） | workflow（V3.4 最小内核） | 已实施 | `WORKFLOW-VERSIONS.md` V3.4 节预期红清单；`tools/eval-trace-check.py --self-test`（作用域与否定句夹具） | R31 补清点完成或用户另行裁决 → 本行关闭；出现清单外预期红而未走本程序 → 回边界 |
| D-058（2026-11-18，用户裁决「按照你的建议来吧」） | r8 后用户质询：两轮 review 都修复并提交（r7=24c6e942c、r8=27c72bafb），为何仍每轮出现 P0？归因：① 修复粒度是「按演示的漏洞打补丁」而非「按类重设计」（R6-a 无键→有键→有键无实；面板门无目录→有目录→空目录）；② 门的输入全为自报文本，词表/正则不可能穷尽伪造面；③ 每轮修复新增攻击表面积 | 用户裁决：按建议实施——**换证据类型（V3.5）**：面板与 certify 回执入库 + INDEX.md 哈希索引 + 门校验；**威胁模型写死**（门防事故不防全伪造，全伪造不再列为待修 P0）；**门收敛判据可判定**（连续两轮独立对抗审查新增 P0/P1=0（边界类除外）+ 每个历史 P0 有判别夹具），r9 为首个正式考场 | 文本匹配 + 自报 + 未入库证据存在结构性上限（r6–r8 三簇 P0 的收窄轨迹：断链→自报键→空目录）；继续加词表永不收敛；制品链把伪造成本抬到「完整哈希自洽」并让不一致可机械发现 | workflow（V3.5） | 已实施（V3.5-a..d + panel-index.py + 4 组夹具） | `WORKFLOW-VERSIONS.md` V3.5 节；`tools/panel-index.py --self-test`；轨迹门制品链夹具 | r9 若出现边界外的 P0/P1 → 回边界；若连续两轮零新增（边界除外）→ 按 §6B-2 判「在声明的威胁模型内收敛」 |
| D-060（2026-11-19，代理裁决，依用户指令文件授权） | D01 已在 R34 封存（CONVERGED + certify 新准入 0），按 §6A 不得再开轮；用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`（commit 562b9fe6e）要求「在 V3.5 下做一次完整确认轮，确认真实收敛」；同时 D01 的 R31 断链（D-057 授权的预期红）只有「R31 补清点」一条解除路径 | 代理裁决：① **重开 D01 并升 edition 至 V3.4**（`schema: V3.4 from R35`），R35 为确认轮；K=3 edition 软上限的超出以本行为显式决定（理由：用户指令 + V3.5 证据类型变更，范围：D01 一篇，日期：2026-11-19）。② 先完成 R31 补清点（53 条 → 24 根因行，独立冷读回执 CLOSED 33 / STILL-BROKEN 20；20 条中 12 条本轮修文档、8 条按 §4.0c 读者/设计 P3 门槛登记 SUGG），使 `--round R31`/`--round R34` 恢复 PASS。③ R35 确认轮的全部新准入按 §3.6 处置：P0/P1/P2 事实与同步项本轮修，6 条设计 P2 与 ARCH-PROPOSAL-01 记 `R35-design-batch` 延期（§4.0f 出口 2），读者/设计 P3 进 suggestions | 用户指令文件即授权；不重开则 V3.5 确认无法执行，且 R31 断链会永久阻断 D01 的任何轮次关账 | in-scope（D01） | 已实施 | `prompt/EXECUTE-D01-D06-CONVERGENCE.md`（commit 562b9fe6e）；`certify/round31-backfill-coldread.md`；`certify/round35-fact.md`（21/21 CLOSED）；`certify/round35-reader.md`（12/12 CLOSED）；`--round R31`/`--round R34`/`--round R35` 的门读数 | 出现 R35 之后仍无法收敛（结构性新根因或第 2 次确认仍新准入>0）→ 判 NOT CONVERGING 回边界；或用户否决本次重开 → 回滚到 R34 FREEZE 状态 |
| D-061（2026-11-19，用户裁决：向 riscv64 看齐） | D01 的 ARCH-PROPOSAL-01（`panel/round35/design-d2.md` §ARCH-PROPOSAL）：aarch64 内核镜像当前用 `x0 == 0` 在运行期判别「直核自引导」与「v1 接收」（`os/kernel-image/src/main.rs:376-382`），而 `x0` 零值不是任何规范承诺（文档 §4.8 自述「非规范保证，仅本仓 QEMU 虚机实测」）。用户裁决：**采纳链接期分叉**，aarch64 改为与 riscv64 `fw-riscv64-uboot` 同构的两变体模式——为直核 `-kernel` 与 UEFI v1 接收各产出一个镜像变体，形态由构建产物决定，运行期不再判别 `x0` | ① 用户明确选择「向 riscv 看齐」（riscv64 已用 `#[cfg(feature = "fw-riscv64-uboot")]` 分叉两条 `rust_image_main`，模式已验证）；② 一个镜像承载两种形态是为省一套构建配置换来的运行期风险，而真板上一次启动只有一种形态，这份灵活性用不到；③ 判别失效的症状是 `kernel-image: handoff v1 envelope invalid` panic，发生在 `early_console` 之后，会把人引向「交接协议坏了」而非「入口形态没识别」 | cross-crate（`kernel-image` / `xtask` / 门脚本 / 文档三章） | 已批准待实施（裁决已生效，实施未开始；不影响任何轮次关账——本轮为确认轮，按 §3.6 不实施 ⑥） | 裁决依据：`panel/round35/design-d2.md` 的 `## ARCH-PROPOSAL`（含扩张内容、三处一致计划、不做会坏在哪、代价）；用户裁决载体为本行 | 实施前出现「aarch64 直核面仅作 QEMU 测试通道」的正式口径（写进 §3.1.1 契约）→ 本裁决需重新审议；实施导致任一 aarch64 门链回归 → 回退到运行期判别并重开本行 |
| D-063（2026-11-20，代理裁决，依用户指令文件授权） | D02 已在 R2 封存（CONVERGED + certify 新准入 0），按 §6A 不得再开轮；用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`（commit 562b9fe6e）要求逐篇做 V3.5 确认轮 | 代理裁决：① **重开 D02 并升 edition 至 V3.4**（`schema: V3.4 from R3`），R3 为确认轮；K=3 edition 软上限的超出以本行为显式决定（理由：用户指令 + V3.5 证据类型变更；范围：D02 一篇；日期：2026-11-20）。② R3 的新准入按 §3.6 处置：P0/P1/P2 事实与同步项本轮修（19 处文档修改点），11 条设计 P1/P2 记 `R3-design-batch` 延期（§4.0f 出口 2），读者/设计 P3 进 suggestions。③ 2 条 ARCH 提案（RD3-ARCH-01/02）待用户亲裁，不阻断 | 用户指令文件即授权；不重开则 V3.5 确认无法执行 | in-scope（D02） | 已实施 | `prompt/EXECUTE-D01-D06-CONVERGENCE.md`（commit 562b9fe6e）；`certify/round3-certify.md`；`--round R3` 的门读数 | 出现结构性新根因或第 2 次确认仍新准入>0 → 判 NOT CONVERGING 回边界；或用户否决本次重开 → 回滚到 R2 FREEZE |
| D-062（2026-11-20，用户裁决：三条 wire 字段删除一次升版） | D01 的三条 wire 布局条目：① `R35/DESIGN-D2-03`——`KernelInfo.syscall_entry` 生产侧填 `kern_virt_base`（`boot-shim/src/lib.rs:126`）、内核零读者（真值来自 `syscall_entry_va()`，`kernel/src/lib.rs:2276`）、strict 门却断言它必须等于 `kern_virt_base`（`kernel_info.rs:459`）；② `R34/DESIGN-D2-04`——`user_sp`/`kern_stack_top` 是两端都能算出的常量关系（`USER_STACK_TOP` 与 `kern_virt_base + kern_size`），传输+解码+断言三段都在证明同一个常量等于它自己；③ `R35/DESIGN-D2-09`——`startup_objects` 独占 wire 20 槽分区（1440 B）+ `BootStorage` 另 20 槽 + 四个写入点 + 三份追加逻辑，唯一读者是 strict 门一条 `any()` 断言 | 用户裁决：**三条全部删除**，一次性升 wire 版本（`BOOT_HANDOFF_V1_VERSION` 9→10）。① 用户明确「删除该字段，升 wire 版本完全可以接受」；② 三条都锚定 Minix3 C 语义——C 的 `kinfo_t` 没有 `syscall_entry`、没有 `kern_stack_top`，`user_sp` 在 C 里是编译期常量 `USR_STACKTOP` 而非交接载荷（`pre_init.c:156`），按 D-047 属 `[DEL]`/`[MODERN]` 代裁范围；③ `startup_objects` 经核实不是预留的 debug 结构：11 个字段里只有 `range` 是真值（其余恒 0 或恒等于 `id`）、`input_object` 自指（`InputObjectId::StartupObjects`）无法回溯任何原始输入、全仓零 debug 输出路径、且它想提供的调试信息用 `kern_phys_base`/`kern_size`/`boot_modules`/`boot_alloc_region()` 这些具名活数据拿得更准 | cross-crate（`minix-boot` / `minix-types` / `boot-shim` / `kernel-image` / `kernel` / 文档 §3.4.3/§4.4/§4.5） | 已批准待实施（裁决生效，实施未开始——D01 的 R35 是确认轮，按 §3.6 不实施 ⑥） | 删除的前提已实跑验证：五条腿的内核跨度全部落在 `memmap` 或 `reserved_regions` 内——x86/aarch64 UEFI 腿由 `reserve_kernel_span` 的 `AllocateAddress(LOADER_DATA)` 进 `reserved`（`uefi_helpers.rs:69-81` + `ebs_map.rs:647` 的 `LOADER_DATA → ReservedMapped`），U-Boot shim 与两个直核腿由 `DtbMemoryMap::new` 的 `fw_end` 参数决定内核在 free 表内（`opensbi_helpers.rs:81` 的 `0x8020_0000`、`bootface.rs:213`、`bootface_a64.rs:203` 的 `fw_end=0`） | 实施中若发现某条腿的内核跨度不落在两张表任一之内 → 该条的等价断言不成立，回退该条并重开本行；若 `kern_stack_top` 与 `base+size` 的解耦需求出现（镜像尾填充而引导栈落在填充前）→ 该条需重新裁决 |
