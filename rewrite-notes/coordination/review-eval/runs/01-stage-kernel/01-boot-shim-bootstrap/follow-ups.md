# D01 follow-ups —— Round 0 清债进度

> V2.1 全修策略：范围内真缺陷一律本轮修。本表追踪每条的处置状态。
> 状态：✅ 已修 / ② 裁决 / ② 相邻 / ④ 待修 / STUCK
>
> **Round 0 终态（2026-11-07）：18/18 闭环**——14 修复 + 4 用户裁决后修复（F03/F07/F08/F10，见上四行）。相邻 3 条（F04/F05/F06）归 code-excellence 命令，不阻塞收敛门。D01 进入 Round 1 全面板。

| ID | 来源 | 事项 | 状态 | 说明 |
|---|---|---|---|---|
| F01 | 复验 | minix-arch 45 编译错误 | ✅ 已修 `6e369393a` | 289 通过 |
| F02 | DESIGN-05 | static mut 二次写入 UB | ✅ 已修 `11feb8582` | 一次性守卫 + 测试豁免 |
| F03 | DESIGN-01 | 删 bootstrap_start/len；wire 版本 +1 | ✅ 已修 `f80ae358b` | 用户裁决删字段；wire v8，envelope 184→160；报告位填 0 |
| F04 | DESIGN-02 | 删 BootShim trait | ② 相邻 | code-excellence；跨模块 |
| F05 | DESIGN-03 | 删 PagingWithId + 死常量 | ② 相邻 | code-excellence；跨模块 |
| F06 | DESIGN-04 | BootForm 删除或降级 | ② 相邻 | code-excellence |
| F07 | DESIGN-07 | startup_objects 规范集合 | ✅ 已修 `49f774ff3` | 用户裁决规范集合并统一；四腿 kernel+modules+root+bump+arena；strict 门覆盖断言；CAP 20 |
| F08 | DESIGN-08 | memmap ledger 替换 | ✅ 已修 `c03a26875` | 用户裁决替换；FreeRangeSet 有序集合，等价对拍锁行为；KERNEL_MAY_ALLOC 删除 |
| F09 | DESIGN-09 | MemoryRegion 非空/对齐 | ✅ 已修 `95682d4e1` | validate_impl 断言 |
| F10 | DESIGN-11 | GET_KINFO 载荷收窄 [ARCH] | ✅ 已修 `79579f954` | 用户裁决标 [ARCH] 三处一致；行为基线=KinfoStruct |
| F11 | DESIGN-13 | Serializer authority 必填 | ✅ 已修 `95682d4e1` | 构造器参数 + 删 write_ram_authority |
| F12 | READER-A 6 条 | §1.2 依赖边等 6 项 | ✅ 已修 | A-01 补第 5 条依赖边；A-02 补链接序证据链；A-03 实测删除扫描只对首条记录生效（5 处）；A-04 补回规则改条件句；A-11 "或"改双表址通道；A-15 栈顶/镜像尾两对象拆开 + L7 交叉引用 |
| F13 | READER-B 15 条 | §1.4 边界重叠等 15 项 | ✅ 已修 | B-01 边界改写+下游按节列；B-02 时序改"切账先于任何分配"；B-04 两处括注改写；B-05 指代点名；B-06 删错误判据；B-07 路线表行补全；B-08 否决理由改"分叉架构名非语义"；B-09 标题+首段+收益逐条标注；B-11 表/段分工重写；B-12 桥接句；B-13 权威表+三处指针化；B-14 差异两侧划法+两交代（顺带修窗口清单漂移）；B-15 修假回指+§4.1 前向指针；B-18 表头对照说明；B-19 四前提清单+§4.9 回指；B-20 截断单说+§3.4 设计回应 |
| F14 | READER-B-03 | 961 重叠推理重复四次 | ✅ 已修 | §1.2 只留依赖方向；§1.3 第 4 句改指向；§2.8 第二条压缩（§2.6.1 为唯一完整推演） |
| F15 | READER-B-10/C-05 | 探针结论强于证据 | ✅ 已修 | 探针结论拆三层（已排除/未排除/独立假设）；"尽早深拷"降级为预防性不变量；§5.1/§5.3 口径同步 |
| F16 | READER-B-16 | 四套形态计数 | ✅ 已修 | §3.7 标题加术语对账注（形态/生产腿/交接路径三维度） |
| F17 | READER-B-17 | §5 三表矛盾 | ✅ 已修 | §5.1 TODO 列带编号+状态（唯一口径）+列语义注；§5.3 列名改"现状与回退判据"；§5.4 引用 §5.1 标签 |
| F18 | READER-C 12 条 | 一致性与指针项 | ✅ 已修 | C-06 三个→两个下游；C-08 同 B-15；C-09 五条腿口径统一+§4.8 表补 aarch64 直核行；C-10 归还时点锚 main.c:301/105；C-11 bootstrap 行拆出+mmap_size 归删除（17→18）；C-12 LOAD_BSS 双路证据链；C-13 忽略错误重写（含 lib.rs 注释改正）；C-14 "整根换掉"改"同根逐页撤"；C-15 八处指针修（procnr/批次记录/01-code-design/取值→定义/五条链/§6.4 补两脚本/lds 引文补 _kern_size）；C-16 同 B-20 |

## Round 7 转出与待裁决

| ID | 来源 | 事项 | 状态 | 说明 |
|---|---|---|---|---|
| R7-FU-01 | D2-03 | BootForm 注册表：保留/删除/接到真实入口 | ✅ 已裁决（2026-11-08，采 A：删） | 删注册表/电池/数量测试，历史形态留教学文档；随 code-excellence 批次（与 F04/F05 同批） |
| R7-FU-02 | D2-02 同族 | root/bump 分配先于内核装载，预防需重排两入口步骤 | ✅ 已并入 F-R16-01（R18） | 检测面已有（装载后复核 root/bump 不重叠）；预防面（装载前不相交）并入 F-R16-01 的物理不相交合同，按 D-5 验收项实施 |
| R7-FU-03 | D2-13 | 内核 LMA 与 AllocatePages 的权衡（是否预留内核区间） | ✅ 已归档（R18：被 A+D 取代） | `reserve_kernel_span` 用 `AllocateAddress` 在内核布局确定后立即预留 LMA 跨度（A+D 方案，R13/R14 落地）；"分配时拒绝 + 装载后复核"退为第二道防线，不再有待裁决的权衡 |
| R8-FU-01 | DESIGN-R8-02 | 全局 allocator 守卫：pool 分配拒绝内核跨度 | 已登记 | 最小修复仅覆盖 ELF 段 src/dest 断言；完整守卫留 code-excellence |
| R8-FU-02 | DESIGN-R8-01 | a2 通道真机取证 vs 删通道 | ✅ 已裁决（2026-11-08，采 A：删） | 删 a2 备用通道（asm 注释 + bootface 分支 + 文档表述），DTB `/chosen` 为唯一生产通道；随批次实施 |
| R10-FU-01 | FACT-R10-01 | CI 增加 kernel-image riscv64 双 feature 构建作业 | ✅ 已裁决（2026-11-08，采 A） | 加 riscv64 `fw-riscv64-none`/`fw-riscv64-uboot` 构建作业并纳入 aarch64；随批次实施 |

## Round 14 转出（F-R14-nn）

面板发现的相邻设计/结构问题，本轮不阻塞：均属跨节重排或跨 crate 重构，风险高于本轮收益；登记去向见"下一步"列。

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R14-01 | FACT-P1-06 | `alloc_lowest` 与 `copy_trampoline` 引文块的省略标记统一 | 已登记（R15） | 下一轮修复批按 earm 块的同一写法处理 |
| F-R14-02 | A-01/B-02 | 七套编号缺单一索引；计数口径靠现场"对账"句 | 已登记（R15） | 在 §1.4 后加"编号索引"表（名字/范围/定义节/消费者），再删重复对账段；需全篇回归 |
| F-R14-03 | A-14/B-17 | "空闲清单怎么传下去"与"何时换手给 VM"分散在 §2.4.1/§3.4/§3.8/§4.5/§4.7 | 已登记（R15） | 写一条跨节导读（不新增机制），或在 §3.4 收一个兑现点 |
| F-R14-04 | A-16 | Direct Map 承重但 02 篇未生成 | 已登记（R17（02 篇）） | 02 篇立项时补；本篇保留边界声明 |
| F-R14-05 | A-20 | §4.2 内联闸表看不出 `Excluded`/`DevicePath` 是否经过 | 已登记（R15） | 分类章表格重排时补一列"是否经闸" |
| F-R14-06 | B-03 | §3.4 混装结构、字段台账、wire 协议、探针四类内容 | 已登记（R15） | 拆节（wire 协议独立、探针移 §5 邻域）；属结构性重排，单独立项 |
| F-R14-07 | B-04 | 接线状态"唯一权威表"埋在 §3.1 备选方案之后 | 已登记（R15） | 前移到 §1.4 或独立 §1.5，全文回指改址 |
| F-R14-08 | B-13 | §4.2 后半的分类判据与"顺序"不同轴 | 已登记（R15） | 拆 §4.2.1 或独立节；标题已先改为"顺序与描述符分类" |
| F-R14-09 | B-15 | §2.7 结构孤岛；ARM 闭区间 `overlaps` 不入 §5 证据账 | 已登记（R15） | 二选一：降为 §2.5 小注，或在 §5.1 增 M17 登记"无断言" |
| F-R14-10 | B-16 | §3.4 字段台账结论先于论证 | 已登记（R15） | 论证前置或加一行"论证见 §3.5/§3.9" |
| F-R14-11 | B-18 | §4.1 两列表按"是否 UEFI"划分，与形态轴（谁建根/谁发送）不符 | 已登记（R15） | 表轴重定义，需同步 §3.7/§4.8 |
| F-R14-12 | B-19 | §3.10/§4.9/§7.5 三张 C↔Rust 对照表无权威 | 已登记（R15） | 指定 §7.5 为唯一索引，其余两表加"索引见 §7.5" |
| F-R14-13 | DESIGN-02 | `KernelInfo` 的 `&'static` 切片逼出 static mut/Box::leak | ✅ 已修（R18 D-1 第一阶段） | `BootStorage` + `BootReport<'a>` 落地，UEFI 腿改走 storage 报告；U-Boot 腿第二阶段迁移登记为 F-R18-01 |
| F-R14-14 | DESIGN-04 | arena wire 分区与语义分区逐区双写 | ✅ 已修（R18 D-2） | 删除语义分区；arena 8 页、wire v9、source@7872/end@30624；版本拒绝负控 |
| F-R14-15 | DESIGN-05 | 同一物理池两个 bump 分配器（`BUMP_PTR` 与 `BootAlloc`） | ✅ 已修（R18，D-3） | 保留两个独立实例（合并游标已证伪：QEMU U-Boot 门 3/6 panic）；公共算法提为 `minix_boot::BumpRegion`，池分区加编译期断言（对齐/范围/arena 容量）+ `prepare_boot` 运行期断言（root 是池首、PT 子区间紧随其后） |
| F-R14-16 | DESIGN-06 | DM 候选顺序在四处各写一遍 | 已登记（R16） | 提为单一常量/函数，四处引用 |
| F-R14-17 | DESIGN-07 | `build_identity_windows()` 恒空而 `kern_ident` 仍在 | 已登记（R16） | 删除函数与字段，或恢复语义；先查 handoff 兼容 |
| F-R14-18 | DESIGN-08 | `FileLoader` 返回 `Vec` 强制整份拷贝 | ✅ 已修（R18 D-4） | `FileBytes<'a>` 显式 Borrowed/Owned；U-Boot 借出、UEFI 拥有 |
| F-R14-19 | DESIGN-09 | `check_boot_postconditions` 在生产值上不可证伪 | 已登记（R16） | 断言改为对独立输入，或降级为 debug 检查 |
| F-R14-20 | DESIGN-10 | `memmap ∩ reserved` 未在校验期检查 | 已登记（R16） | `validate` 加交集检查（先确认 C 侧无此约束） |
| F-R14-21 | DESIGN-11 | 13 个 pub 字段 + 12 个转发 getter | 已登记（R16） | 二选一：字段私有 + getter，或删 getter |
| F-R14-22 | DESIGN-12 | 三种校验语义用两个 bool 编排 | 已登记（R16） | 改具名 enum/构造器 |
| F-R14-23 | DESIGN-13 | `ebs_failed` 与 `ebs_attempts > 0` 重叠 | 已登记（R16） | 删一个状态，保留有语义的那个 |
| F-R14-24 | DESIGN-14 | `PagingWithId` 仅测试实现；`HUGE_PAGE_SHIFT` 零读者 | 已登记（R16） | 先做读者审计（含未来 PCID 计划），再删 |
| F-R14-25 | DESIGN-15 | `Plan` 两遍遍历 | 已登记（R16） | 建议只改名（`ClassificationPlan`），不重构 |
| F-R14-26 | DESIGN-03 | `prepare_boot` 约 250 行零生产调用，与生产入口分叉 | ✅ 已修（R17 裁决：删；R18 记账） | 随 `UefiBootShim` 整型删除；`prepare_boot` 只剩 U-Boot 生产入口 `RiscvUbootBootShim` 一处实现 |

> 档期口径：R15 批处理本篇 in-scope 结构项（F-R14-01/02/03/05/06/07/08/09/10/11/12），R16 批处理跨 crate 代码重构（F-R14-13..25）与 `prepare_boot` 裁决；F-R14-04 指向尚未生成的 02 篇，定在 R17。

## Round 16 转出（F-R16-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R16-01 | DESIGN-01 / RB16-08 / R7-FU-02 | U-Boot shim 腿：root/bump 在读取内核布局之前分配，无"模块/root/bump 不落进内核 LMA 跨度"的守卫；arena 在 `boot_alloc_region()` 区间之后，不在整段扣除内，扣除表未单独排除 | ✅ 已修（R18 D-5） | 装载前门：`load_kernel_with_loader_gated` 从 ELF 程序头算 LMA 跨度，断言不越链接期 4 MiB 上界（编译期 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`）且不与 bump 池相交，失败在首个段写入前停机；arena 不做永久扣除，改由生命周期合同 + poison 端到端测试证明（`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`） |
| F-R16-02 | DESIGN-02 | 兜底 bump 选择（`boot_alloc_region()==None`）只取 memmap 第一个够大区间起点，不排除内核/模块/root | ✅ 已修（R18，D-6） | 删兜底扫描；未注册即 panic（fail-closed），消息带 DM 可采界；负控 `missing_boot_allocator_registration_fails_closed` + 正控各一条 |

## Round 18 转出（F-R18-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R18-01 | D-1 第二阶段 | U-Boot shim 腿仍以一次性 `.bss` 静态量 + `Box::leak`（模块表、平台源、参数）持有载荷；所有权真实但形态未统一 | 已登记（code-excellence 下一批） | `prepare_boot` 接收 `&mut BootStorage`，`BootPrepareResult` 携带报告视图；判据与截止见 `OPEN-DEBT.md` |
| F-R18-02 | P-2 第一版边界 | 表结构/链接门不做标题锚点校验与外部链接存活检查 | 已登记（workflow 候选） | 出现需要时在 `WORKFLOW-VERSIONS.md` 立版本候选，附真实案例 |
