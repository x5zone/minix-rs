DOC 01-boot-shim-bootstrap.md

VARIANCE: D-024 D1 停跑，D2 采用文档主张↔源码事实回退

# DESIGN-02 (b) 落地后的 R2 独立设计审查（D-119）

审查范围：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3、§4 的主张，对照
`related-code.txt` 的 48 个文件（按 §7.2/§7.3 符号锚点选择性阅读）与 `trait-audit.json`。
方法：先按 `minix3/minix/kernel/{arch/i386/pre_init.c, arch/i386/pg_utils.c, table.c}`
独立重建 C 语义合同，再逐项核对文档与源码；不读历史评审结论。本报告只写本文件。

基线声明：不改 `os/`，不回退 (a)，不恢复 `build_kernel_info`/`BootStorageCell`/`report_static`/`STORAGE`。
本轮无 `[ARCH]` 提案（见 `## ARCH-PROPOSAL` 段为空）。

---

## DIFF-TABLE

| 维度 | 文档 Ch3/Ch4 主张 | 源码事实 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 | `ValidationLevel` 三档替换两 bool（§4.4 第 4 条）；`BootStorage` 的 `fill_*`「写与提交是一步」，族内半提交不可表示（§3.7） | `ValidationLevel` 为 `BeforeTables`/`Standard`/`Strict` 三变体（`os/libs/minix-boot/src/kernel_info.rs:157-179`）；`fill_*` 闭包返回计数即提交（`boot_storage.rs:141-223`）。另存在 5 个 per-family `*_committed` 布尔，仅用于拒绝第二次 fill，`report()` 不读它们；`report()` 可在任何 family 未填时构造空视图（有测试 `a_fresh_storage_reports_empty_lists_and_zero_param_bytes`） | 文档未提 5 个布尔，也未说「空视图是合法中间态」 | 合理取舍：空视图/部分填充由接收侧校验兜住；5 布尔是重复提交守卫，不是状态机 |
| 状态表达 | wire 信封 128 B、版本精确比较、六分区按 `byte_len` 校验；arena 余量 3584 B | `BOOT_HANDOFF_V1_VERSION = 10`（`handoff_v1.rs:278`）；`size_of::<BootHandoffV1>() == 128` 编译期断言；`check_header` 精确比较 magic/version/size/arena_len（`handoff_v1_read.rs:100-117`）；`WIRE_SOURCES_END == 29184`、`ARENA_CAP-29184 == 3584` 编译期断言 | 一致 | 接受 |
| 身份 | 模块身份是位置：`MODULE_NAMES` 定义装载序，strict 门按位置核名，直核面 `NAME_POOL` 与 `MODULE_NAMES` 编译期逐字节对账；「位置不是进程号」，进程号来自 `BOOT_MODULE_PROC_NRS`（§4.3） | 名字表在 `minix-boot`（`kernel_info.rs:671-673`），进程号表在 kernel（`os/kernel/src/proc.rs:111-124`）；两者仅以下标关联，跨 crate 无编译期链接。strict 名核对（`kernel_info.rs:489-496`）比较的是 loader 按同一 `MODULE_NAMES` 取来的名字（`os/boot-shim/src/loader.rs:598-600`），对名表是恒真 | 「名 ↔ 进程号」的配对没有单一权威；C 是同一张 `{proc nr, name}` 表（`minix3/minix/kernel/table.c:47-64`） | 差异需改，见 DESIGN-01 |
| 身份 | `authority` 放进 `KernelInfo` 后「图和它的声明变成同一个值」，解码路径不再有两个可接反的参数；错配「只会在下一次覆盖审计时才暴露」（§3.2 绑定说明） | v1 接收路径成立（`lib.rs:733-747`，strict 读 `scalars.ram_authority`）。直核路径的 authority 仍是 `arch_boot` 的独立实参（`bootface_common.rs:552,554` 两次传 `DTB`），`arch_boot_finish` 把它存进 `KernelInfo`（`lib.rs:215-238`）后不再做带权威的审计；`kmain` 终门用非 strict 的 `validate()`（`lib.rs:1864`），非 strict 分支的覆盖审计不带 authority 过滤（`kernel_info.rs:518-528`） | 直核面保留了文档声称已消除的配对面；最终边界不按 `KernelInfo.authority` 审计，存进去的 authority 在生产路径上再无读者 | 差异需改，见 DESIGN-02 |
| 所有权 | `BootStorage` 拥有发送侧载荷；`BootReport<'a>` 只是借用视图；接收侧先落 `.bss` 再构造唯一 `KernelInfo`；arena 解码是最后一次读（§3.2/§3.7/§4.6） | 六 slice 的 land/copy 都与文档一致（`lib.rs:1575-1706`）；UEFI 腿 source 只走 wire、storage 切片为空（`uefi_helpers.rs:1239-1242`）；arena poison 测试存在（`vm_handoff.rs:1088`）。细节：直核面 `BootModule.name` 的字节 backing 是 bootface 的物理名字池（`bootface_common.rs` `place_modules` 尾段），UEFI/U-Boot 腿是 `MODULE_NAMES` 的 `.rodata`，都不在 `BootStorage` 内，store 时才拷入 `.bss` | 文档「六类载荷」的表述没有覆盖 name 字节的外部 backing | 合理取舍（`&'static str` 在 `.rodata`/名字池都活到 store）；文档可在 §3.7 点明 |
| 所有权 | 六个落地区加名字池，只有名字池允许截断；`store_kernel_info` 截到 15 B，v1 接收路径按 16 B 原样落（§4.5） | `land_slice`/param/module-count 全部断言 panic；模块名 15 B 截断（`lib.rs:1627`）；v1 接收 `name_len` 取到 16（`lib.rs:694-708`） | 一致 | 接受 |
| 错误 | §3.7 一句话：「半提交 → 走 `fill_*` 的腿第二次调用 panic」 | 「写未提交」这种半提交在现行 `fill_*` API 已不可表示（闭包返回即提交）；第二次 `fill_*` 是**重复提交**，不是半提交（`boot_storage.rs:145,167,182,200,215`） | 错误分类把两种不同状态并成一类 | 文档措辞需改，见 DESIGN-03 |
| 并发 | boot 单线程；Atomic 只为避免 `static mut` UB，不承诺并发安全；`pt_alloc::register` 写一次、双档 panic；VM 在自身上下文注册（§3.4.2） | `BootAlloc` 注释明确「非可线性化、boot-only」（`boot_alloc.rs:14-22`）；`register` 用 compare_exchange 并在 release 也 panic（`pt_alloc.rs:55-67`）；内核与 VM 各用自己的静态副本 | 一致 | 接受 |
| 抽象边界 | kernel 不依赖 boot-shim/uefi；四份依赖表；固件选择 cfg 共 7 处（6 uefi + 1 opensbi）；`qemu-test-kernels` gate 10 处；测试接缝是 `FileLoader` 不是 `prepare_boot`（§3.5） | 四份 manifest 逐项吻合（`os/kernel/Cargo.toml`、`os/boot-shim/Cargo.toml`、`os/libs/minix-boot/Cargo.toml`、`os/kernel-image/Cargo.toml`）；`lib.rs` 实际属性 7 处、`uefi_helpers.rs` 实际 `qemu-test-kernels` 属性 10 处（grep 各多 1 处来自文件内文档注释）；`FileLoader` 3 实现（UEFI、U-Boot、Mock），`dyn` 仅 `load_dumpdtb_blob` 一处；仓内已无 `BootShim` trait | 一致（计数与依赖方向均可复核） | 接受 |
| 可测试性 | `FileLoader` 提供 Mock 接缝；wire round-trip；arena poison 后重发帧的宿主测试；容量负控（§5 引用的机制） | 上述测试均存在；`KernelInfo::source_audit()` 是公开方法但只有它自己的单测调用（`kernel_info.rs:559-569`，唯一使用点 `:774`）；`BootStorage::default` / `BootAlloc::default` 仓内无调用点 | 公开面有无消费者的存项 | 清理级，见 DESIGN-04 |
| 最小化 | `PreparedJump`/`MapCounts`/`PlacementPool` 这类值对象防同型实参交换（§3.6/§4.5） | 三者均在且各有理由；`enter_kernel_image_v1(arena_pa, report, root_page, shim_windows)` 仍有两个 `PhysBytes`（arena 与 root），但只有 3 个具名调用点、未见故障判例 | 保留面，未到需要新类型的门槛 | 合理取舍（见 SUBTRACTION-TABLE 第 3 行） |

---

## SUBTRACTION-TABLE（V3.6，C-32）

本轮无新增抽象被采纳；下表列出「考虑过但被更少概念的替代方案压住」的加法候选。因此本报告没有
加法 FINDING，只有「减法式最小修复」FINDING。

| 加法提案 | 减法替代（闭包 / fn 参数 / 现有类型 / 不做） | 减法拦不住的理由 | 保障强度对比 | 新增概念数 | 采纳 |
|---|---|---|---|---|---|
| 给 `BootReport` 加 `authority` 字段并去掉 `arch_boot` 的独立 authority 实参 | `kmain` 改调已有的 `validate_requiring_sources()`（1 行；零新概念） | 无——最终边界在 `kmain` 第一段执行，先于任何消费者 | 减法在最终边界 fail-closed；加法只能把错配提前到构造点报，实际故障点相同 | 0 | 否 |
| 新增跨 crate 的编译期交叉断言/配对类型，绑定 `MODULE_NAMES` 与 `BOOT_MODULE_PROC_NRS` | 把两张表合并成一张 `[(&'static str, ProcNr); 12]`，由它派生两个数组（净减一张表、一个常量） | 无——合并后一次编辑同时移动名字与进程号，恒真核对变成结构性成立 | 减法更强：C 的单表语义被恢复；`BOOT_MODULE_PROC_NRS` 不再是独立抄本 | -1 | 否（采用减法） |
| `RootAndArena { root_page, arena_pa }` 值对象，消除 `enter_kernel_image_v1` 的两个同型 `PhysBytes` | 维持具名实参（3 个调用点、变量名即文档） | 无——没有故障判例，两个值的语义靠调用点命名承担 | 现状略弱；加法的代价 1 类型 + 3 调用点改动 | 1 | 否 |
| 把 `BootStorage` 的 5 个 `*_committed` 布尔收成一个位图/状态枚举类型 | 维持 5 个布尔，或直接删掉（重复 fill 是调用方 bug，接收侧校验仍兜底） | 无——布尔只服务一次重复调用守卫，不参与 `report()` 语义 | 相等；位图会让「5 个独立守卫」变成「一个不透明整数」，理解成本更高 | 0 | 否 |

---

## FINDING DESIGN-01（P2）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:671-673`（`MODULE_NAMES`）、`os/kernel/src/proc.rs:111-124`（`BOOT_MODULE_PROC_NRS`）、`os/kernel/src/misc.rs:577`、`os/kernel/src/lib.rs:2878`、`os/kernel/src/vm_handoff.rs:633`、`os/kernel/src/misc.rs:3192,3275`（按下标的消费点）；文档 §4.3「若把 `MODULE_NAMES` 里 `tty` 与 `mib` 两项互换会怎样」
- 严重度：P2
- 问题：C 的 boot image 是**一张** `struct boot_image image[]`，每行是 `{proc nr, name}` 对（`minix3/minix/kernel/table.c:47-64`）。Rust 把它拆成两张只有下标关系的表：名字表住 `minix-boot`，进程号表住 kernel，两张表之间没有编译期链接。文档 §4.3 已经把「互换两项后三条 v1 腿全部装载成功、strict 门只看到每个位置都有名字、症状推迟到第一次对 tty/mib 的 syscall」写成反事实，但没有给出结构上的关闭方式；strict 名核对比较的是 loader 用同一张 `MODULE_NAMES` 取来的名字，对名表恒真，所以它不是独立证据。
- 影响：一次只改名字表的重排会静默改变「哪个服务跑在哪个进程号」。启动能走到 `kmain`，进程表也能建起来，故障表现为某个服务不响应或收到协议外回复，与根因隔着整个启动流程。这是文档自己承认的静默故障面，而且改动两个文件中的任意一个都不会编译失败。
- 证据或推理：`grep -rn "MODULE_NAMES" os/kernel/src/*.rs` 只命中注释（`lib.rs:1642`、`proc.rs:85`），kernel 不引用名字表；`BOOT_MODULE_PROC_NRS` 的 5 个消费点全部按 `i` 取值；两张表分属两个 crate，`NR_BOOT_MODULES` 只钉住**数量**（`proc.rs:84-85`、`kernel_info.rs:648-670`），不钉配对。
- 最小修复：在 `minix-boot`（或 `minix-types`，两者都已被 kernel 依赖）放一张 `pub const BOOT_MODULE_TABLE: [(&'static str, ProcNr); NR_BOOT_MODULES]`，`MODULE_NAMES` 与 kernel 的 `BOOT_MODULE_PROC_NRS` 都从它按 `const fn` 派生。行为不变；净减一张表与一处独立抄本；§4.3 的反事实随之从「不可检测」变成「一次编辑同时移动名字与进程号」。
- 减法替代（V3.6，C-32）：不新增类型，改用「合并表」这一减法（见 SUBTRACTION-TABLE 第 2 行）。若只想最小动文档，可退一步加一条宿主测试硬编码 12 对期望值；那会形成第三份抄本，不如合并。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（本提案不新增类型/字段，只合并两张现有表）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净减概念：名表与进程号表由 2 个变 1 个）。
- 置信度：高（表位置、消费点、跨 crate 无链接均逐条 grep 核实）。
- 是否需源码核实：已核实；未运行跨架构构建（合并是纯常量重排，不改语义）。
- 若我错了：若 `BOOT_MODULE_PROC_NRS` 的顺序与 `MODULE_NAMES` 的耦合被某个 CI 门（本报告未找到）在别处对账，则严重度降为 P3，只余「单一权威」的可读性收益。

## FINDING DESIGN-02（P2）

- 位置：`os/kernel-image/src/bootface_common.rs:552,554`；`os/kernel/src/lib.rs:215-238`（`arch_boot_finish`）、`244-276`（两个 `arch_boot` 收 authority 实参）、`1864`（`kmain` 的 `validate()`）；`os/libs/minix-boot/src/kernel_info.rs:276-278`（strict 读 `self.authority`）、`518-528`（非 strict 审计不带 authority）
- 严重度：P2
- 问题：文档 §3.2 说把 authority 放进 `KernelInfo` 后「图和它的声明变成同一个值」「错配只会在下一次覆盖审计时才暴露」。v1 路径确实如此。但两个直核面的 authority 仍是 `arch_boot` 的**独立实参**：`bootface_common` 先在 report 上做一次 strict 审计（传 `DTB`），再把 `DTB` 作为另一个实参交给 `arch_boot`，由其写入 `KernelInfo.authority`。此后没有任何门把存储后的 `KernelInfo.authority` 与记录对账：`kmain` 用的是非 strict 的 `validate()`，而它的条件覆盖审计（`kernel_info.rs:518-528`）不带 authority 过滤。生产路径上 `KernelInfo.authority` 的唯一读者是 strict 门，而直核腿的 strict 门作用在**存储之前**的 report 上（report 没有 authority 字段）。因此「DTB 记录 + 声明 UEFI 权威」这类错配在直核路径上仍可被发布，且最终边界看不见。
- 影响：今天两个调用点都写 `DTB`，没有实际故障。结构上它保留了文档声称已消除的「两个可以接反的实参」面；未来第三条直核腿或一次重构把两处写岔，产物是「权威声明与来源记录不一致的 KernelInfo」进入 `kmain`，后续任何严格审计才报错，而那时已经过了所有启动校验。
- 证据或推理：`grep -rn "validate_requiring_sources()" os/` 的生产调用只有 `lib.rs:747`（v1 接收）与 `bootface_common.rs:552`（report，非 `KernelInfo`）；`kmain` 的 `kernel_info.validate()`（`lib.rs:1864`）走 `ValidationLevel::Standard`，`is_strict()` 为假。`kmain` 的 cfg 是 `#[cfg(all(not(feature = "mock"), not(feature = "qemu_test")))]`（`lib.rs:1819`），QEMU 测试内核不编译它，所以改成 strict 不会影响测试内核路径。
- 最小修复：`kmain` 把 `kernel_info.validate()` 改成 `kernel_info.validate_requiring_sources()`（1 行），让五个腿的最终边界按存储后的 `authority` 复核一次记录；同步改文档 §3.7/§4.4.2 的门链描述（「kmain 的 validate」→「kmain 的 strict validate」）。若担心测试形态，可在同一 cfg 内完成，无需新类型。
- 减法替代（V3.6，C-32）：上面的 1 行改动就是减法（复用已有方法、零新概念）。备选加法「给 `BootReport` 加 authority 字段」被压住（SUBTRACTION-TABLE 第 1 行）。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（无新增类型/字段）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（零新增概念；门链叙述少一个「非 strict 终门」例外）。
- 置信度：高（调用点与 cfg 逐条核实）。
- 是否需源码核实：已核实到「strict 只在 v1 接收与 bootface report 上调用」这一事实；未跑真机门验证改成 strict 后的启动（但所有生产腿都携带记录，strict 的前置条件由 leg 级门已证）。
- 若我错了：若某条生产腿合法地携带空 `source_records`（当前五条都不），改 strict 会让它在 `kmain` 停机；那时应改为「kmain 在 records 非空时按 authority 审计」的条件式，而不是回退到非 strict。

## FINDING DESIGN-03（P3）

- 位置：文档 §3.7「四类错误各有落点」段（「半提交 → 走 `fill_*` 的腿第二次调用 panic」）；源码 `os/libs/minix-boot/src/boot_storage.rs:141-223`
- 严重度：P3
- 问题：文档把「半提交」的落点写成「第二次调用 `fill_*` 会 panic」。这两件事不是同一个状态：旧的两步 API 的「半提交」是「槽已写、长度未提交」，现行 `fill_*` 把写与提交做成一步，该状态**不可表示**；第二次 `fill_*` 是**重复提交**，由 `*_committed` 断言拦下。文档同时声称「族内半提交状态不可表示」和「半提交 → 第二次调用 panic」，前后指的不是同一个东西。
- 影响：错误分类表把一个已消失的状态保留为「有落点」，读者会以为半提交仍需防守。实际需要防守的是重复提交与跨族部分填充（后者由接收侧校验兜住，文档已写）。
- 证据或推理：`boot_storage.rs:145` 等五处 `assert!(!self.*_committed, "... already committed")`；`report()`（`:255-277`）不读任何 committed 标记，注释明确「No dirty-flag assertion is needed」。
- 最小修复：把该行改成「重复提交 → 第二次 `fill_*` panic；半提交（写而未提交）在现行 API 中不可表示」。
- 减法替代（V3.6，C-32）：文档改一句，不新增任何代码概念。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若作者所谓的「半提交」指「同族第二次调用破坏了已有提交」，那只是术语用法，按行文上下文仍建议改成「重复提交」以免与旧 API 的状态混淆。

## FINDING DESIGN-04（P3，[DEL]）

- 位置：`os/libs/minix-boot/src/kernel_info.rs:559-569`（`pub fn source_audit`）、`:774`（唯一调用点，单测）；同类存项 `os/libs/minix-boot/src/boot_storage.rs:280-284`、`os/kernel/src/boot_alloc.rs:158-162`（`Default` 实现）
- 严重度：P3
- 问题：`KernelInfo::source_audit()` 是公开方法，全仓唯一使用点是它自己的单测；生产路径的来源审计走 strict 门与非 strict 条件审计。`BootStorage`/`BootAlloc` 的 `Default` 实现仓内也没有调用点（`::default()` 一次未出现）。这属于「翻译残留」里的无消费者公开面：C 没有对应物，Rust 侧也没有读者。
- 影响：公开 API 面比实际契约大；读者会以为存在一条「只审计来源、不 panic」的消费路径。维护成本低，但违反「最小公开面」。
- 证据或推理：`grep -rn "source_audit"` 仅 `kernel_info.rs:559` 定义、`:774` 测试；`grep -rn "BootStorage::default\|BootAlloc::default"` 无命中；`static BOOT_ALLOC = BootAlloc::new()` 用的是 `new`。
- 最小修复：删除 `source_audit()`（或加 `#[cfg(test)]`），删除两个未使用的 `Default` 实现；若作者要保留诊断 API，则在文档 §3.3 写明它的消费者。
- 减法替代（V3.6，C-32）：删除即减法，无替代需求。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用（删除）。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用（净减公开面）。
- 置信度：中高（`source_audit` 已全仓核实；`Default` 的隐式泛型使用未穷举，但静态实例走 `new`）。
- 是否需源码核实：`source_audit` 已核实；`Default` 建议用 `cargo clippy` 的 dead-code 面再确认一次。
- 若我错了：若 `source_audit` 是给外部 crate（本仓之外）的稳定诊断面，则应保留并在文档登记，而不是删除；本条降为「文档未登记消费者」。

## FINDING DESIGN-05（P3，[DEL]）

- 位置：`os/libs/minix-boot/src/handoff_v1_write.rs:391-393`；`os/boot-shim/src/uboot_main.rs:472-473,485`；运行时串口字符串 `os/boot-shim/src/uboot_main.rs:476`
- 严重度：P3
- 问题：v10 已删除 startup-object 分区与记录（`handoff_v1.rs:112,121`、`memmap_source.rs:560` 只保留 retired tag），但三处生产注释仍把「startup 分区/startup record」写成现行结构：`reset_map_partitions` 说「startup-object partition 故意不清」——该分区在 arena 布局里已不存在；`uboot_main` 说 `prepare_boot` 写「memmap/reserved/sources/param/startup from the argv DTB」且 arena「named by a startup record」。另外 `uboot_main.rs:476` 把决策号 `(d73)` 写进启动串口输出，读者在真机日志里会看到过程编号。
- 影响：读者按注释找「startup 分区」会落空；与 `code-publication-check.py` 的 CP-3/CP-4 判据（过程编号、迭代叙事）同属类。无行为影响。
- 证据或推理：`grep -rn "startup"` 命中上述三处；`handoff_v1.rs` 的 arena 常量只到 `WIRE_SOURCES_END`；`uboot_main.rs:476` 为 `raw_serial_line("boot-shim: kernel + 12 modules loaded (d73)")`。
- 最小修复：删除 `reset_map_partitions` 的 startup 句；把 `uboot_main` 两处注释改成现行五段/无 startup；删掉串口字符串里的 `(d73)`。
- 减法替代（V3.6，C-32）：删除即减法。
- 三问（V3.6，C-31；仅类型级提案必填）：不适用。
- 教学概念账（V3.6，C-35；仅加法提案必填）：不适用。
- 置信度：高（字符串与注释逐字核对）。
- 是否需源码核实：已核实。
- 若我错了：若 `(d73)` 是某个真机门脚本依赖的路标文本，删除前需同步该脚本；本报告未核 §5.4/门脚本，故先按「注释必修、串口串待门脚本确认」提出。

---

## ARCH-PROPOSAL

本轮无 `[ARCH]` 提案。上述五条都是在既有五腿形态、既有 wire 协议与既有 `arch_boot`/`resume_additive`
收敛层内部收紧不变量或清理残留，不改变「boot 是什么」，不新增责任/字段/检查/策略。

---

## VERDICT

**值得改（P2，建议进本轮修复）**

- DESIGN-01：把 `MODULE_NAMES` 与 `BOOT_MODULE_PROC_NRS` 合并成一张 `{name, proc_nr}` 表。这是 C 单表的直接回归，净减一张表，且关掉文档 §4.3 自己承认的静默故障面。代价：`minix-boot` 的常量形状变化、kernel 的一处常量改为派生；行为不变。
- DESIGN-02：`kmain` 终门改 strict（1 行）。它把 §3.2「权威与图是同一个值」的设计意图落实到最终边界，并让直核面存储后的 `authority` 第一次被审计。QEMU 测试内核不编译 `kmain`，无兼容风险。

**文档措辞修正（P3）**

- DESIGN-03：§3.7 的「半提交 → 第二次 fill panic」是全篇唯一一处可判定的分类错误，改成「重复提交」即可。

**清理（P3）**

- DESIGN-04：删 `source_audit`（与两个未用的 `Default`）或登记消费者；DESIGN-05：删三处 startup 残留注释与串口串里的 `(d73)`。

**合理取舍，不建议动**

- `BootStorage` 的 5 个 `*_committed` 布尔：只是重复提交守卫，不影响 `report()` 语义；换位图反而增加理解成本。
- `enter_kernel_image_v1` 的 `arena_pa`/`root_page` 两个同型 `PhysBytes`：只有 3 个具名调用点，未见故障判例；加值对象不达 §3.5 的代价门槛。
- 直核面 `BootModule.name` 的字节 backing 在 `BootStorage` 之外：`&'static str` 在 `.rodata`/物理名字池都活到 store，语义安全；只建议文档补一句。
- `PreparedJump`/`MapCounts`/`PlacementPool`/`BootPrepareResult` 等值对象：均已由 D-073/D-067 类判例背书，且源码与文档一致，不宜再动。

**文档事实面：核对通过，无 P0/P1 事实错误**

本次逐条核对的数值与结构均与源码一致：wire 版本 10、信封 128 B、`WIRE_END=7872`、
`WIRE_SOURCES_END=29184`、余量 3584 B；落地区 128/256/12/4/1024/296、名字 15 vs 16；VM 64/47；
`KinfoStruct` 48 B 与 offset 0/8/16/24/28/32/38；`BOOT_PT_BUMP_PAGES=1024`、直核
`PT_BUMP_PAGES=256`；固件 cfg 7 处（6 uefi + 1 opensbi）与 `qemu-test-kernels` 10 处；四份
Cargo 依赖表；五步 `adjudicate` 与两道内联闸；`BOOT_HANDOFF_V1_VERSION` 精确比较；
`land_and_store` 的同址跳过与一次写入；`kmain` 的 `validate`→`check_boot_postconditions` 次序；
`KernelInfo` 10 字段、`BootReport` 9 字段。十个 `param_buf` 可观察分歧与源码逐条对得上。

**需要用户裁决的事项**：无。DESIGN-01/02 是重写内部的表达与门强度调整，不改外部行为，也没有
`[ARCH]` 语义；若作者认为「合并两表」触及 `minix-boot`/kernel 的 crate 分工（谁拥有进程号），
可按 §3.5.2 的既有方向裁决放 `minix-boot` 还是 `minix-types`，两者都不新增依赖边。

---

## 未覆盖

时间盒到点，以下未核，明确不当作已证：

- Ch4 §4.7 的 C↔Rust 函数落点表只抽核了 `pre_init`/`get_parameters`/`add_memmap`/`cut_memmap`/
  `alloc_pagetable`/`overlaps`/`pg_*` 的主干符号，其余行（`alloc_lowest`、`pg_map` 粒度等）未逐行对。
- `os/kernel/src/{pte_walk,smp}.rs`、`os/arch/src/arch/{paging,paging_ext}.rs` 的运行期语义未展开；
  本报告只核了它们与 boot 交接面的签名/调用关系（`map_kernel` 两参、`inherit_supervisor_half` 存在）。
- `resv_claims.rs`、`dtb_copy.rs` 只核了「preflight 分配、配对循环内 copy、EBS 前不分配」的时序，
  未逐条核 claim 覆盖算法；`memmap_source.rs` 的 `audit_source_coverage` 只核了错误变体与
  authority 过滤点，未核其区间算法。
- Ch3 §2 的 C 侧其余论断（`bootstrap_*` 链接脚本、ARM32 `pre_init` 对照、`get_parameters` 的
  逐行推演）只读了 `i386/pre_init.c`、`pg_utils.c`、`table.c`、`kernel.lds` 节选。
- §3.1.1 的接线状态与 §5.4 的真机门、以及测试计数（188/136/866/27）未运行、未核；本报告未执行
  任何 `cargo` 构建或 QEMU 门。
- `trait-audit.json` 只按 boot 相关名字查询（`FileLoader`/`Paging`/`HugePages`/`BootArch`/
  `HigherHalf`/`DirectMapArch`/`EarlyConsole`/`PlatformDesc` 等）；`KernelInfoView` 不在该 JSON
  里，本报告用 grep 自行核实为 2 实现。非 boot crate 的审计读数未复核。
- 文档 §1、§5–§7 未审（本轮范围是 Ch3/Ch4）。
