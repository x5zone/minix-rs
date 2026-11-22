# 01 引导交接 · P-DESIGN 第二遍（round38）

只读审查；写入仅本文件。输入：`design-d1.md`、`related-code.txt` 所列实现、`01-boot-shim-bootstrap.md` 第 3/4 章、`trait-audit.txt`、`AGENTS.md` 约束。未读作者解释、历史评审、其他面板输出与 `expected/`。
方法：先减法后加法。本轮无新增类型提案，故各 FINDING 的「三问/教学账」按不适用处理（仅 DESIGN-07 复用既有 `PhysRange`，不新增教学概念）。

## DIFF-TABLE

| 维度 | D1 主张 | 现有实现 | 判定 |
|---|---|---|---|
| 状态表达 | 恒有序、两两不交的 `Span{start,len,source}` 集合，`mem_high` 派生 | 两张定长数组加独立 `SourceRecord` 列表；接收侧另建 `FreeRangeSet` | D1 不变量更少，但要改 wire；现状可用（DESIGN-07） |
| 身份 | 三种：`BootFacts<'a>` 借用 / wire / `KernelInfo` | 五种名字：`BootStorage`、`BootReport`、`BootStorageCell`、`BootPrepareResult`、`KernelInfo` | 差一层视图；`BootReport` 可删（减法表第 1 行） |
| 所有权 | 单一线性换手，`&mut` 即窗口 | 静态存储 + 四个 once 布尔 + 同址 no-op + 分类双写 | 现状有两处双实现（DESIGN-01、03） |
| 错误 | `Result<_, BootError>` 加 errno 映射 | `assert`/panic 加十个域错误枚举（`DtbMemMapError`、`HandoffV1ReadError`、`PairError`、`MapError` 等） | D1 增加无消费者的 errno 层，拒绝 |
| 并发 | 借用检查器表达 | `AtomicU64` 游标 + `AtomicBool` 单写 + `SyncUnsafeCell`（BKL 之前单线程） | 等价；现状注释更强，`pt_alloc` 可化简（DESIGN-09） |
| 抽象边界 | 不要 trait | 唯一 trait 是 `FileLoader`（3 实现、1 处 dyn，测试接缝）；`BootShim` trait 已删 | 一致，D1 该条已落地 |
| 可测试性 | 编译失败用例加纯函数性质 | `MockLoader` 加 `should_panic` 加区间对拍 | 等价；D1 的编译失败用例未落地，成本低 |

trait 核实：related-code 涉及的 NO-IMPL 行只有 `BklProtected`/`Sealed`，二者由 `globals.rs:118-125` 的宏生成，不是死 trait；`FileLoader` 的第三个实现是 `loader.rs:688` 的测试 `MockLoader`。差异小结：D1 的 DECISION 01/02/06/07/10 与现状同向（`BootShim` 已删、全局 `kernel_may_alloc` 布尔已无、构造点已单一、参数表语义已按 `[ARCH]` 落地）；实质分歧只在跨度集合与取整（03/04/05）、`Result` 错误层（08）与 `Reclaimed` 状态（09）。

## SUBTRACTION-TABLE

最后一列：仍需加法（是/否）。第 2 行的「是」转入 ARCH-PROPOSAL，不写成 FINDING。

| 提议新增项 | 更少概念的替代物 | 减法够用 | 仍需加法 |
|---|---|---|---|
| D1 `BootFacts<'a>` | 删 `BootReport`：`BootStorage` 加三个标量即借用视图，六个 accessor 随之消失 | 是 | 否 |
| D1 `EarlyMemory` 线性值 | 把 `&mut BumpRegion` 穿进 `Paging` 调用链，删注册槽与三个原子 | 否（需改 Paging 240 余处引用） | 是 |
| D1 `Span{source}` 单集合 | 现有两表加 `audit_source_coverage` 已给同一判据 | 是 | 否 |
| D1 `ModuleDesc::Present/Reclaimed` | 不表示回收态：模块表交接后只读，VM 的排除发生在 `build_vm_handoff` | 是 | 否 |
| D1 `assemble()` 单函数 | `build_kernel_info`（直核）加 `arch_boot_resume_v1` 解码（v1） | 是 | 否 |
| D1 `BootError` 加 errno | 保留各层域错误枚举；入口是 `!`，errno 无消费者 | 是 | 否 |
| 现有四个 `fill_*` once 布尔 | 让 `fill_*` 全部可重写，与 `fill_map_tables` 对齐（同一套重试语义） | 是 | 否 |
| 现有 `PreparedJump`/`prepare`/`jump` | 合并进 `enter_kernel_image_v1` | 是 | 否 |
| 现有 `MemoryRegion`/`MemRange` | `PhysRange` 加 `EMPTY` 关联常量 | 是 | 否 |


## FINDING DESIGN-01 [DEL] 中 需用户裁决 · `os/boot-shim/src/ebs_map.rs:805-835`
问题：一次分类事实同时写进 wire（`push_memmap`/`push_reserved`）与调用方数组（`memmap_out`/`reserved_out`），两表示无相等对账；`reset_map_partitions()`（`ebs_pair.rs:172`）只清 wire 侧。
影响：今天同源同一步写、两侧都有容量断言，不会发散；但任何一侧改容量、截断或过滤就静默分叉——内核只读 wire，shim 的 EBS 后断言只读 storage（`uefi_helpers.rs:1241-1244`），无交叉检查。`fill_map_tables` 没有 once 守卫（`boot_storage.rs:139-156`），其余四类都有（163/178/196/211），第二次提交可覆盖 storage。
最小修复：分类器只落 storage，`pair_and_classify` 去掉 `serializer` 入参，EBS 成功后由 `report` 一次性编码 wire——`uboot_main.rs:483-520` 已是这个形状。减法替代：删 `reset_map_partitions`、`Classification` 双轨返回与双写断言。
置信度 高。若我错了：若 wire 必须早于 EBS 定稿——arena 是普通内存，`patch_runtime_fields_in` 已在 EBS 后写它，无此约束。

## FINDING DESIGN-02 [DEL] 低 接受 · `os/kernel/src/lib.rs:399-404、427-433、525-533、548`
问题：`prepare_kernel_image_v1` 与 `jump_kernel_image_v1` 全仓唯一调用者是 `enter_kernel_image_v1` 自身（grep 只命中定义与文档注释）；`PreparedJump` 只为这次相邻调用存在。
影响：三件公开面加一个只服务内部的值对象；代码注释写明拆分理由是「once TODO-09 is resolved」打印路标，零欠账规则下「为未完成 TODO 保留的公开接口」本身就是欠账。
最小修复：合并为一个公开入口，跳转降为私有；TODO-09 真要做时先做再拆。减法替代：删 `PreparedJump` 与两个公开函数。
置信度 高。若我错了：若有仓外调用方按符号调用——本仓查无。

## FINDING DESIGN-03 [MODERN] 中 需用户裁决 · `os/kernel/src/lib.rs:653-741` 与 `1553-1700`
问题：「落进 `.bss` 并发布 `KERNEL_INFO`」有两份实现：v1 接收面在 `arch_boot_resume_v1` 里手取六个 store 指针、手写名字截断与 `from_utf8` 回退（681-705）、再用 `from_raw_parts` 拼 `KernelInfo`；直核面走 `store_kernel_info`/`land_slice`。第二份靠 `src.as_ptr()!=dst.as_ptr()` 跳过拷贝来共存。
影响：模块名截断与 UTF-8 回退规则两处维护，任何一侧改动都会让 v1 腿与直核腿行为不同，且差异要到启动后才可见；`land_slice` 用部分重叠语义的 `ptr::copy` 也是为这条同址路径留的。
最小修复：解码完成后调一个 `publish_landed(六个长度, scalars)`，由 store 指针构造 `KernelInfo` 并写 `KERNEL_INFO`；`store_kernel_info` 只留外部来源路径。减法替代：删 resume 内约 40 行落库与 `land_slice` 的指针比较。
置信度 高（两处源码逐行可比）。若我错了：若将来出现第三种半落库来源——那时更该先有单一入口。

## FINDING DESIGN-04 [DEL] 中 接受 · `os/boot-shim/src/resv_claims.rs:261-283、292、358`
问题：5 个模块级 `BootCell`（`DTB_RECORDS`/`N`、`CLAIMS`/`N`、`DTB_RAW`）加 `records()`/`capture()`/`check_current()` 传递的数据从未离开 `ebs_pair.rs:185-200` 的同一函数体。
影响：数据流隐式，调用顺序错误只能在运行期 panic；测试要 `test_serial` 全局锁；新增读者可以绕过 `capture` 读到半填状态。
最小修复：`capture` 返回栈上 `ClaimSet`（64 条约 1.5 KB，记录槽由调用方给），`check(&claims, descs)` 收值；删 5 个静态与 `records()`。减法替代：删全局状态、锁需求与隐式顺序。
置信度 高。若我错了：若 claim 必须跨 `ExitBootServices` 重试存活——重试循环就在同一函数内，栈值天然存活。

## FINDING DESIGN-05 [DEL] 低 接受 · `os/kernel/src/lib.rs:860-885、888`
问题：三字段值对象加两个逐字段拷贝构造器，唯一消费者是 `boot_validate_and_prepare`；注释仍说「四个位置实参收成一个」，而第四参 `kern_stack_top` 已在 wire v10 删除。
影响：3 个调用点多一层中间名；三个参数类型互异（`VirBytes`/`PhysBytes`/`u64`），本来就不可能交换。
最小修复：签名改收三个值，删结构体与两个构造器。减法替代：纯删约 30 行。
置信度 高。若我错了：若近期还要加同型第四参——那正是值对象该出现的时候，现在没有。

## FINDING DESIGN-06 [DEL] 低 接受 · `os/libs/minix-boot/src/kernel_info.rs:171-173、499-523`
问题：`requires_startup_kernel_record` 的名字指向 v10 已删的 `startup_objects`；实现检查「内核物理跨度被 memmap 或 reserved_regions 覆盖」，上方注释却写「at least one record must cover」。`validate_at`（221-224）只是一行转发。
影响：概念账里多一个不存在的对象，读者会去找来源记录；跨版本读者无法判断这是旧名还是新语义。
最小修复：改名 `requires_kernel_span_coverage` 并同步注释；三处调用直接走 `validate_impl_inner`，删 `validate_at`。减法替代：纯改名加删一层转发。
置信度 高。若我错了：若计划恢复 startup 记录——当前方向相反（v10 删了它）。

## FINDING DESIGN-07 [MODERN] 中 需用户裁决 · `kernel_info.rs:578`、`vm_handoff.rs:56/307`、`address.rs:196`
问题：同一「物理区间」有三种表示——`MemoryRegion{base,len:usize}`（交接与落地区）、`MemRange{base,end}`（`FreeRangeSet` 内部）、`PhysRange{start,end}`（`BootWindows`，`new` 拒反向）；`vm_handoff.rs:56` 同文件同时 import 前两者。
影响：`MemoryRegion` 可表示 `len=0` 与 `base+len` 溢出，宽度 `usize` 与 wire 的 `u64`（`handoff_v1.rs:40`）不一致，解码要 `try_from`（`handoff_v1_read.rs:389`）；每个消费者都得自己 `checked_add` 加页对齐断言（`validate` 内约 30 行）。`PhysRange` 加 `EMPTY` 哨兵的先例已在 `lib.rs:1159`。
最小修复：`PhysRange` 提为唯一区间，加 `PhysRange::EMPTY` 关联常量供定长数组哨兵；`MemoryRegion`/`MemRange` 删除，wire 编解码在边界换算。约 30 文件，建议单列一轮。减法替代：删两个类型与两套校验。
置信度 中（可行性高，收益取决于是否愿意动 wire 边界）。若我错了：若定长哨兵数组的可读性比类型统一更重要。

## FINDING DESIGN-08 [MODERN] 中 需用户裁决 · `os/libs/minix-boot/src/memmap_source.rs:510-535`
问题：11 个字段里，审计判据只消费 `kind`、`range`、`address_domain`、`input_object`（352-399）；`origin` 只被测试读（`ebs_pair.rs:573` 等）；`id`、`local_source`、`snapshot_id`、`attribute`、`virtual_start` 在生产代码无读者。
影响：72 B 乘 296 槽等于 21312 B arena（`handoff_v1.rs:367-384`）被无人消费的字段撑大；「每字节可追溯」实际只需要 4 个字段。
最小修复：二选一——把 forensic 字段接进诊断输出（消费即保留），或在 wire v11 收窄。需用户裁决（改协议与版本）。减法替代：收窄记录即删容量与编解码分支。
置信度 中。若我错了：若有仓外或离线消费者读完整 72 B 记录——本仓查无。

## FINDING DESIGN-09 [MODERN] 低中 接受 · `os/arch/src/arch/pt_alloc.rs:61-69、89-91、98-148`
问题：同一不变量（已注册）由三处表达——`uninit_alloc` 占位 panic、`PT_REGISTERED` 布尔、`alloc_pt_page` 内 assert；写权又由 `PT_CLAIMED` 再序列化，分配与归还各一套，共 6 个静态。
影响：`is_registered()` 用 Relaxed load，模块文档却说「Acquire-load `is_registered()==true`」才发布写（实际 Acquire 在 `alloc_pt_page` 的第二条 load），契约与实现不一致；删一个布尔无法判断哪处才是权威。
最小修复：每槽一个 `AtomicUsize`（0 等于未注册），`register` 用 CAS 0 到函数指针，`is_registered` 等于非零；删 `CLAIMED`、`REGISTERED`、两个占位函数与 `UnsafeCell` 包装。减法替代：6 静态降为 2。
置信度 中高。若我错了：若将来需要「注册中」第三态——当前没有这个状态。

## FINDING DESIGN-10 [MODERN] 低 接受 · `os/kernel/src/lib.rs:427-433`；`uefi_helpers.rs:1253-1266`；`uboot_main.rs:528`
问题：内核公开入口收裸 `(u64, u64)` 对；文档 §4.4（2338 行）自定「进入内核内部一律 `PhysRange`」，同时把这个形参列为例外，属自我豁免。
影响：两个调用方各维护 `[tuple; 3]` 加计数；区间反转只由 `BootWindows::push` 的运行期断言拦。
最小修复：形参改 `&[PhysRange]`（boot-shim 已依赖 minix-types，`PhysRange::new` 是 `pub const`），或公开 `BootWindows` 让 shim 先 push。减法替代：删两处 `(start,end)` 记账与文档例外条款。
置信度 高。若我错了：若 shim 侧拿不到构造类型化区间的机会——它可以。

## FINDING DESIGN-11 [MODERN] 中 接受 · `os/kernel/src/globals.rs:275`；`01-boot-shim-bootstrap.md` §3.7（1837 行）
问题：`globals.rs` 说 `KERNEL_INFO` 由 `prepare_kernel_image_v1` 在 v1 腿写入，但该函数是发送侧、手里没有 `KernelInfo`，v1 腿的实际写者是 `resume_additive`（`lib.rs:320`）；文档 §3.7 说 `'static` 由 `BootStorageCell::take` 支撑，实际是 `report_static`（`boot-shim/src/lib.rs:178`）。
影响：两条陈述把读者引向发送侧或不存在的桥，与 §3.2「发送侧不构造 `KernelInfo`」直接冲突。
最小修复：两处分别改为 `resume_additive` 与 `report_static`。减法替代：不新增内容，只校正锚点。
置信度 高（源码逐行核对）。若我错了：若 `prepare_kernel_image_v1` 在别处也写 `KERNEL_INFO`——grep 无此调用。

## ARCH-PROPOSAL

`[ARCH: 引导期页表页分配从全局注册槽改为随 Paging 调用链传递的线性值]`
内容：删 `os/arch/src/arch/pt_alloc.rs` 的六个静态与 `os/kernel/src/boot_alloc.rs` 的全局实例，让 `map`/`map_huge` 收一个 `&mut EarlyMemory`；`register`/`resume` 两个跨镜像动作变成「把值传过去」。
代价：`Paging`/`HugePages` 是 242 处引用的宽 trait，三架构实现与 VM 侧分配器要一起改；这是本报告成本最高的一项。
外部行为：页地址可能变化，用户态不可观察；不新增 errno。
三处同步：文档 §3.4.2、设计注记与代码需同时标注 `[ARCH]`；判据是下一次 Paging 重构窗口，不单独开轮。

## VERDICT

- 结论：需修改。D1 的方向大体与现状重合，真正要改的是现状残留（DESIGN-01、03 是 D1 未识别的结构性问题）；按 D1 加东西则被减法表逐条挡住。
- 拒绝 D1 项：`BootError` 加 errno（入口是 `!`，无消费者）；`ModuleDesc::Reclaimed`（模块表只读，无状态可迁移）；`BootFacts` 仅改名（减法表第 1 行给出更省的删法）。
- 接受（低风险删减）：DESIGN-02、04、05、06、09、10、11。
- 需用户裁决：DESIGN-07（`PhysRange` 统一，动 wire 边界）、DESIGN-08（`SourceRecord` 收窄，改协议版本）、DESIGN-01 与 03（单写与单落库，动 UEFI 与接收侧主链）。
- 证据强度：全部 FINDING 已重放 grep 与逐行读源（L1/L2）；同 agent 复核，未做跨工具只读复核；DESIGN-07 与 08 的改动面为估算，因此单列为需裁决项。
