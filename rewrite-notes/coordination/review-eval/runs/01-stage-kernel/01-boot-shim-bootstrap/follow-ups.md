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
| R8-FU-01 | DESIGN-R8-02 | 全局 allocator 守卫：pool 分配拒绝内核跨度 | ✅ 已归档（R18） | UEFI 腿 `alloc_module_pages` 拷贝前拒绝内核跨度；U-Boot/自举腿由 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 编译期断言把池与内核 LMA 跨度钉死；另加装载前 gate |
| R8-FU-02 | DESIGN-R8-01 | a2 通道真机取证 vs 删通道 | ✅ 已裁决（2026-11-08，采 A：删） | 删 a2 备用通道（asm 注释 + bootface 分支 + 文档表述），DTB `/chosen` 为唯一生产通道；随批次实施 |
| R10-FU-01 | FACT-R10-01 | CI 增加 kernel-image riscv64 双 feature 构建作业 | ✅ 已裁决（2026-11-08，采 A） | 加 riscv64 `fw-riscv64-none`/`fw-riscv64-uboot` 构建作业并纳入 aarch64；随批次实施 |

## Round 14 转出（F-R14-nn）

面板发现的相邻设计/结构问题，本轮不阻塞：均属跨节重排或跨 crate 重构，风险高于本轮收益；登记去向见"下一步"列。

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R14-01 | FACT-P1-06 | `alloc_lowest` 与 `copy_trampoline` 引文块的省略标记统一 | ✅ 已修（R16 批） | 引文头统一注明"省略号处为未改动的循环变量声明与方法体其余行" |
| F-R14-02 | A-01/B-02 | 七套编号缺单一索引；计数口径靠现场"对账"句 | ✅ 已修（R16 批） | §1.4 编号索引表（编号/指什么/定义节/主要消费者） |
| F-R14-03 | A-14/B-17 | "空闲清单怎么传下去"与"何时换手给 VM"分散在 §2.4.1/§3.4/§3.8/§4.5/§4.7 | ✅ 已修（R16 批） | §3.4 开头一条分工导读把四节职责钉住（C 侧截断/扣减表/七个落地区/换手时序） |
| F-R14-04 | A-16 | Direct Map 承重但 02 篇未生成 | OPEN（02 篇立项轮） | adjacent（R14/READER-A-16）：按 `review-evals.md` §4.2 转移，不阻塞本篇收敛；02 篇立项时补，本篇已保留边界声明 |
| F-R14-05 | A-20 | §4.2 内联闸表看不出 `Excluded`/`DevicePath` 是否经过 | ✅ 已修（R16 批） | §4.2.2 表后明写两道闸的覆盖范围（`checked_len` 不经过 Step 3 定档者） |
| F-R14-06 | B-03 | §3.4 混装结构、字段台账、wire 协议、探针四类内容 | ✅ 已修（R16 批） | §3.4 拆为 3.4.1–3.4.4（维度与结构/协议布局/深拷探针/所有权） |
| F-R14-07 | B-04 | 接线状态"唯一权威表"埋在 §3.1 备选方案之后 | ✅ 已修（R16 批） | 前移为 §3.1.1"接线状态（唯一权威表）"，全文回指改址 |
| F-R14-08 | B-13 | §4.2 后半的分类判据与"顺序"不同轴 | ✅ 已修（R16 批） | §4.2 拆为 4.2.1（九个动作与约束）/4.2.2（五步判定与两道内联闸） |
| F-R14-09 | B-15 | §2.7 结构孤岛；ARM 闭区间 `overlaps` 不入 §5 证据账 | ✅ 已修（R16 批） | §5.1 增 M17 行，登记"无运行期断言，仅源码对照" |
| F-R14-10 | B-16 | §3.4 字段台账结论先于论证 | ✅ 已修（R16 批） | 随 §3.4 拆节把论证放到结论之前 |
| F-R14-11 | B-18 | §4.1 两列表按"是否 UEFI"划分，与形态轴（谁建根/谁发送）不符 | ✅ 已修（R16 批） | §4.1 表改为按阶段归并的概览，形态轴在 §3.7/§4.8 |
| F-R14-12 | B-19 | §3.10/§4.9/§7.5 三张 C↔Rust 对照表无权威 | ✅ 已修（R16 批） | §7.5 为唯一权威索引；§3.10/§4.9 加"权威索引在 §7.5" |
| F-R14-13 | DESIGN-02 | `KernelInfo` 的 `&'static` 切片逼出 static mut/Box::leak | ✅ 已修（R18 D-1 第一阶段） | `BootStorage` + `BootReport<'a>` 落地，UEFI 腿改走 storage 报告；U-Boot 腿第二阶段迁移登记为 F-R18-01 |
| F-R14-14 | DESIGN-04 | arena wire 分区与语义分区逐区双写 | ✅ 已修（R18 D-2） | 删除语义分区；arena 8 页、wire v9、source@7872/end@30624；版本拒绝负控 |
| F-R14-15 | DESIGN-05 | 同一物理池两个 bump 分配器（`BUMP_PTR` 与 `BootAlloc`） | ✅ 已修（R18，D-3） | 保留两个独立实例（合并游标已证伪：QEMU U-Boot 门 3/6 panic）；公共算法提为 `minix_boot::BumpRegion`，池分区加编译期断言（对齐/范围/arena 容量）+ `prepare_boot` 运行期断言（root 是池首、PT 子区间紧随其后） |
| F-R14-16 | DESIGN-06 | DM 候选顺序在四处各写一遍 | ✅ 已修（R17） | `for_each_dm_candidate` 单一候选迭代器，两遍遍历复用 |
| F-R14-17 | DESIGN-07 | `build_identity_windows()` 恒空而 `kern_ident` 仍在 | ✅ 已修（R17） | 删除函数与测试，空窗口以常量表达；handoff 兼容已查 |
| F-R14-18 | DESIGN-08 | `FileLoader` 返回 `Vec` 强制整份拷贝 | ✅ 已修（R18 D-4） | `FileBytes<'a>` 显式 Borrowed/Owned；U-Boot 借出、UEFI 拥有 |
| F-R14-19 | DESIGN-09 | `check_boot_postconditions` 在生产值上不可证伪 | ✅ 已修（R17） | 三条 `#[should_panic]` 负测：syscall 越界/栈顶低于基址/user_sp 越界 |
| F-R14-20 | DESIGN-10 | `memmap ∩ reserved` 未在校验期检查 | ✅ 已修（R17） | `validate_impl_inner` merge-walk 交集检查 + 负测 |
| F-R14-21 | DESIGN-11 | 13 个 pub 字段 + 12 个转发 getter | ✅ 已修（R17） | 删 12 个转发 getter，106 个调用点改字段访问 |
| F-R14-22 | DESIGN-12 | 三种校验语义用两个 bool 编排 | ✅ 已修（R17） | `ValidationLevel::{BeforeTables,Standard,Strict{authority}}` |
| F-R14-23 | DESIGN-13 | `ebs_failed` 与 `ebs_attempts > 0` 重叠 | ✅ 已修（R17） | 删 `ebs_failed`，保留 `ebs_attempts` |
| F-R14-24 | DESIGN-14 | `PagingWithId` 仅测试实现；`HUGE_PAGE_SHIFT` 零读者 | ✅ 已修（R17） | 删除 `PagingWithId`、死常量与 `MockAsid` |
| F-R14-25 | DESIGN-15 | `Plan` 两遍遍历 | ✅ 已修（R17） | 改名 `ClassificationPlan`，不重构遍历 |
| F-R14-26 | DESIGN-03 | `prepare_boot` 约 250 行零生产调用，与生产入口分叉 | ✅ 已修（R17 裁决：删；R18 记账） | 随 `UefiBootShim` 整型删除；`prepare_boot` 只剩 U-Boot 生产入口 `RiscvUbootBootShim` 一处实现 |

> 档期口径（历史）：R15/R16 批处理 in-scope 结构项与跨 crate 代码重构（F-R14-01..12、16..26），均已执行完毕；F-R14-04 指向尚未生成的 02 篇，仍开放（R14/READER-A-16，adjacent；按 §4.2 不阻塞本篇收敛，目标轮次 = 02 篇立项轮）。

## Round 16 转出（F-R16-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R16-01 | DESIGN-01 / RB16-08 / R7-FU-02 | U-Boot shim 腿：root/bump 在读取内核布局之前分配，无"模块/root/bump 不落进内核 LMA 跨度"的守卫；arena 在 `boot_alloc_region()` 区间之后，不在整段扣除内，扣除表未单独排除 | ✅ 已修（R18 D-5） | 装载前门：`load_kernel_with_loader_gated` 从 ELF 程序头算 LMA 跨度，断言不越链接期 4 MiB 上界（编译期 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`）且不与 bump 池相交，失败在首个段写入前停机；arena 不做永久扣除，改由生命周期合同 + poison 端到端测试证明（`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`） |
| F-R16-02 | DESIGN-02 | 兜底 bump 选择（`boot_alloc_region()==None`）只取 memmap 第一个够大区间起点，不排除内核/模块/root | ✅ 已修（R18，D-6） | 删兜底扫描；未注册即 panic（fail-closed），消息带 DM 可采界；负控 `missing_boot_allocator_registration_fails_closed` + 正控各一条 |

## Round 18 转出（F-R18-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R18-01 | D-1 第二阶段 | U-Boot shim 腿仍以一次性 `.bss` 静态量 + `Box::leak`（模块表、平台源、参数）持有载荷；所有权真实但形态未统一 | ✅ 已修（R18 补记） | `prepare_boot(&mut BootStorage, bump_pages)` 填 storage；arena 在函数内分配、startup 五类记录返回前写齐；`BootPrepareResult<'a>` 携带报告；QEMU U-Boot shim 6/6 PASS |
| F-R18-02 | P-2 第一版边界 | 表结构/链接门不做标题锚点校验与外部链接存活检查 | 已登记（workflow 候选） | 出现需要时在 `WORKFLOW-VERSIONS.md` 立版本候选，附真实案例 |
| F-R18-03 | RB17 / R18 D2-08 | `#[cfg(test)] classify_regions` 仍手写一份成员序列，与生产 `for_each_live_bootstrap` 不完全同源 | adjacent（R19 观察） | 测试副本不影响生产单源；若 R19 出现漂移再合并，不阻塞本篇收敛 |
| F-R18-04 | RB17 / R18 D2-04 | `pt_alloc` 的两条单测只覆盖串行二次注册，没有并发双写者压测 | adjacent（R19 观察） | 修的是并发 UB；`PT_CLAIMED` swap 的互斥性可由语义与现有串行负控支撑，压测成本高于收益，登记观察 |

## Round 17 转出（F-R17-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R17-01 | DESIGN-D2-12 | `BootShim` 是单实现 trait（1 impl / 0 dyn），唯一调用点是 `<RiscvUbootBootShim as BootShim>::prepare_boot`；当前既非测试接缝也非已验证扩展点 | 待用户裁决（架构去留；截止 R20） | 选项：保留为未来固件腿接口（文档已按事实降级）/ 改成 `opensbi_helpers::prepare_boot` 普通函数并重写 §3.2。不阻塞本轮 |
| F-R17-02 | DESIGN-D2-13 | UEFI 辅助三件套（`build_memmaps`/`exit_boot_services`/`UefiPoolAllocator` + 四个 `static mut`）只服务 16 个 QEMU 测试内核，生产二进制仍链着 | 已登记（截止 R20） | 移到 `#[cfg(feature = "qemu-test-kernels")]` 或独立模块，并同步 16 个测试内核的依赖声明；需单独回归 QEMU |
| F-R17-03 | DESIGN-D2-09/14 | `dtb_copy::Phase` 住 `static mut`，`OwnedDtb.bytes` 只能是 `'static`（leaked LOADER_DATA）；注释已写明真实生命周期，未改结构 | 已登记（截止 R20） | `OwnedDtb` 已改 raw pointer + `bytes()`，两处 `Box::leak` 已移除；剩余 Phase 栈化（带真实生命周期）作为 R20 重构 |
| F-R17-04 | DESIGN-D2-18 | `KernelInfo` 不携带 `ram_authority`，strict 门由调用方另传；混合记录表下"DTB 记录 + 声明 UEFI 权威"可能放行 | 待用户裁决（契约变更；截止 R20） | 给 `KernelInfo` 加 `authority: InputObjectId`（wire 已有），`validate_requiring_sources()` 改无参；文档"13 字段"改 14。属 ABI/文档契约变更 |

## Round 20 转出（F-R20-nn）

| ID | 来源 | 事项 | 状态 | 下一步 |
|---|---|---|---|---|
| F-R20-01 | DESIGN-D2-01 | `BootPrepareResult`/`enter_kernel_image_v1` 的 `bump_base`/`bump_end`/`arena_pa` 仍是裸 `u64`，与 `root_page: PhysBytes` 并排；非法区间在类型上可表示 | adjacent（R21） | 改为物理区间/`PhysBytes`；改动两处签名与三处调用，需跨 crate 回归 |
| F-R20-02 | DESIGN-D2-03 | `MemoryRegion{base,len}`、`cut(start,size)`、`MemRange{base,end}` 三套端点/长度约定并存，新调用者可把端点当 size 传 | adjacent（R21） | 引入 `Range{start,end}` 并让 `cut` 收区间；与 F-R20-01 同批 |
| F-R20-03 | DESIGN-D2-04 | 测试内核 `build_memmaps`/`exit_boot_services` 仍用 `static mut` + 假 `'static` 切片，与 `BootStorage` 所有权模型相反 | adjacent（R21） | 迁到 `&mut BootStorage` 或绑定 `&self` 生命周期的视图；需回归 16 个测试内核 |
