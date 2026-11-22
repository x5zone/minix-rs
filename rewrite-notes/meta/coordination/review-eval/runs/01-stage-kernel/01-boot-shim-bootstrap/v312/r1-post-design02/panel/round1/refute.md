DOC 01-boot-shim-bootstrap.md

# 反证报告 · 01-boot-shim-bootstrap（v312-r1-post-design02）

阅读范围：文档全文、`v312/related-code.txt` 所列代码中的相关入口（按 §7.2/§7.3 锚点选择性读取）、`v312/SCOPE.md`、`DECISIONS.md` 的 D-047/D-062/D-110/D-111/D-112/D-116/D-117、`AGENTS.md` 关键约束，以及文档引用的 Minix3 C 源码。未读取其他面板角色的输出。

方法：每条主张先取其最强形式，再找源码里相反的执行路径、失败路径、架构分支或时序条件；找不到反例就写“反证失败（主张成立）”，不凑数。

---

## CLAIM 01

- 主张：I1“值可跨镜像，指针/句柄不可”。三条 v1 腿的 wire 只写固定宽度标量与字节分区，接收侧从私有存储重建切片；两个直核腿同镜像也深拷（文档 §1.2 行 76、§1.0 行 36、§3.2.3 行 1316、§4.4 行 2284）。
- 反证尝试：
  1. 逐字段检查 wire 类型是否有指针：`os/libs/minix-boot/src/handoff_v1.rs:37-100`（`WireMemoryRegion` 两个 `u64`、`WireBootModule` `[u8;16]+u64+u64`、`WirePlatformDescSource` `u32+u32+u64`、`WireSourceRecord` 全定宽）与 `:287-315`（信封全 `u32/u64`）都只有定宽标量与字节数组，没有 `usize`、引用或裸指针。这一半成立。
  2. 找“句柄跨镜像”的反例：`WirePlatformDescSource.addr` 是固件 blob（DTB/RSDP）的**物理地址**，接收侧（`os/kernel/src/lib.rs:728-731` 解码、crate 内后续窗口构造）会把它当访问目标；文档自己把 `PlatformDescSource` 叫作“16 B 句柄”（§3.2.2.3 行 1262）。按 I1 的字面“指针/句柄不可”，这与设计文本直接冲突。它之所以安全，是因为传的是物理地址而不是发送侧的虚拟指针；I1 没有写出这个限定词。
  3. 找“切片没深拷”的反例：v1 接收侧 `arch_boot_resume_v1`（`os/kernel/src/lib.rs:663-751`）把六个分区解码进 `.bss` store 后才构造 `KernelInfo`；两个直核面在 `arch_boot_finish` 经 `store_boot_report`（`:230`）落 `.bss`。没有找到“接收侧仍用发送侧指针”的路径。
- 结果：非阻断（P2，观察）。实质主张（没有发送侧虚拟地址跨镜像）成立；“指针/句柄不可”的字面表述与文档自身的“16 B 句柄”冲突，需要限定为“发送侧虚拟地址指针/进程局部句柄不可”。
- 证据：`os/libs/minix-boot/src/handoff_v1.rs:37-100, 287-315`；`os/libs/minix-boot/src/platform.rs:123-132`；`os/kernel/src/lib.rs:663-751`；文档 §1.2 行 76、§3.2.2.3 行 1262。
- 若我错了：如果这里的“句柄”专指进程局部资源句柄（文件、堆指针），而物理地址被明确归入“值”，那 I1 只是措辞偏松，不是事实错误；但文档目前没有给出这条区分，读者按字面读会得到相反结论。

---

## CLAIM 02

- 主张：接收侧统一走 `store_boot_report`，先把切片深拷进内核 `.bss`，再生成唯一的 `KernelInfo<'static>`（文档 §4.0 小结行 1888、§4.4 行 2284、§4.4 字段表 `source_records` 行 2324）。
- 反证尝试：
  1. 枚举调用点：`grep -rn "store_boot_report(" os/ --include=*.rs` 只有 `os/kernel/src/lib.rs:230`，位于 `arch_boot_finish`（直核面尾部）；`store_boot_report` 是 `pub(crate)`，没有第二个生产调用点。
  2. 走 v1 接收路径：`arch_boot_resume_v1`（`os/kernel/src/lib.rs:663-751`）先 `decode_memmap`/`decode_reserved_regions`/`decode_modules`/`decode_platform_sources`/`decode_param_buf`/`decode_sources` 直接写入 `.bss` 的六个 store，再构造 `KernelInfo`，然后 `resume_after_v1_decode` → `resume_additive`；`resume_additive` 在 `:325` 调用的是 `store_kernel_info`，不是 `store_boot_report`。
  3. 找文档内部相反句：§3.7 行 1840 写“v1 接收面 … 后 `store_kernel_info`”；§4.4.1 行 2374 写“v1 接收面 … `store_kernel_info`”；§4.5 行 2397 写“v1 接收面的 `resume_additive` 仍走 `store_kernel_info` 包装”；§4.8 行 2540 写“v1 接收面是 `store_kernel_info` → …”。四处细节都与 §4.0/§4.4 的“统一走 `store_boot_report`”相反。
- 结果：阻断（P1）。这是核心交接机制的事实错误，且在概述性小结与实现章开头重复出现，和同章细节节互相矛盾。若评审把“核心机制错名”归入 P0-fact，可升为 P0。
- 证据：`os/kernel/src/lib.rs:230`（唯一 `store_boot_report` 调用）、`:325`（`resume_additive` 的 `store_kernel_info`）、`:663-751`（v1 解码路径）；文档行 1888、2284、2324 与行 1840、2374、2397、2540。
- 若我错了：只有一种读法能救这句话——把 `store_boot_report` 当成“`land_and_store` 这个共用落地函数”的代称。但 `store_boot_report` 是真实存在的另一个 wrapper（`os/kernel/src/lib.rs:1571`），文档的 §4.5 也把它和 `store_kernel_info` 并列区分；按现有代码，v1 接收侧没有调用它。修法是一句话：“直核面走 `store_boot_report`，v1 接收面走 `store_kernel_info`，两者共用 `land_and_store`。”

---

## CLAIM 03

- 主张：UEFI 腿的语义表住在 shim 自己的**静态区（`.data`）**，`BootStorage::new()` 非全零所以落在 `.data`（文档 §4.2 代码节选行 1961）；`BootStorage` 是单所有者，UEFI 腿只拥有五类（五个 `fill_*` 入口，`fill_map_tables` 一次提交两表）（§3.7 行 1842）。
- 反证尝试：
  1. 找 UEFI 腿 `storage` 的存储类：`os/boot-shim/src/uefi_helpers.rs:1086` 是 `let mut storage = minix_boot::BootStorage::new();`——`prepare_and_enter_kernel_image` 的**栈上局部变量**。它被 `fill_*` 可借地修改，随后 `storage.report(...)` 被序列化；整个函数是 `-> !`，栈帧活到跳转。
  2. 对照另外三条发送腿：`os/boot-shim/src/uboot_main.rs:475` 也是 `let mut storage = ...`（`uboot_main` 栈帧）；`os/kernel-image/src/bootface_common.rs:354` 也是 `let mut storage = ...`，其源注释（`:352-354`）明写 “It lives on this `-> !` frame”。生产上没有 `static BootStorage`（全仓 grep 未找到 `static ... BootStorage`）。
  3. 检查 `BootStorage::new()` 的文档：`os/libs/minix-boot/src/boot_storage.rs:109-111` 只说“Const-constructible for a static; because `new()` contains non-zero defaults, **such a static** lands in `.data`, not `.bss`”——这是一个条件句，不是“生产 storage 在 `.data`”的事实断言。文档把条件句改写成了事实。
  4. 检查计数：`BootStorage` 有五个 `fill_*` 方法（`boot_storage.rs:141/163/178/196/211`）；UEFI 腿实际调用四个入口（`fill_modules`、`fill_platform`、`fill_param`、`fill_map_tables`；`uefi_helpers.rs:1087/1099/1103/1167`），覆盖五类数据；第五个 `fill_source_records` 只由 U-Boot 腿调用（`opensbi_helpers.rs:316`）。所以“UEFI 腿拥有五类”正确，但括号里的“五个 `fill_*` 入口”是 API 面计数，不是 UEFI 腿的调用计数，读起来会算成六类。
- 结果：阻断（P1）——`.data` 静态区断言是具体的事实错误，且落在“所有权/存储位置”这个高风险面上；计数括号是非阻断（P2）的措辞问题。
- 证据：`os/boot-shim/src/uefi_helpers.rs:1086`；`os/boot-shim/src/uboot_main.rs:477`；`os/kernel-image/src/bootface_common.rs:352-354`；`os/libs/minix-boot/src/boot_storage.rs:109-111, 141-223`；`os/boot-shim/src/opensbi_helpers.rs:308-324, 398-405`；文档行 1961、1842。
- 若我错了：如果编译器把这个 10 KiB 级、被 `&mut` 修改、内容运行期才确定的局部提升成 `.data` 静态——这在 Rust/LLVM 语义下不应发生；如果另有一个 `static BootStorage`——全仓检索没有。修法：改成“语义表住在调用方的栈帧里；`new()` 可 const 构造，若做成 static 会落在 `.data` 而非 `.bss`，但生产三条腿都用栈上局部”。

---

## CLAIM 04

- 主张：I4“校验先于消费”，且各腿门链顺序为——直核面 strict → `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 `validate`；v1 接收面 strict → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare` → … → `store_kernel_info` → `kmain` 的 `validate`，**不经过** `validate_before_tables`（文档 §3.7 行 1844、§4.4.1 行 2374）。
- 反证尝试：
  1. 直核面：`arch_boot_finish`（`os/kernel/src/lib.rs:225-238`）确实是 `validate_before_tables` → `check_boot_postconditions` → `store_boot_report` → `arch_boot_impl`；`arch_boot_impl`（`:953-974`）开头调 `boot_validate_and_prepare`。`bootface_common.rs:554` 在调 `arch_boot` 之前显式 `report.validate_requiring_sources(RamAuthority::DTB)`。顺序与文档一致。
  2. v1 接收面：`arch_boot_resume_v1`（`:747-750`）是 `kinfo.validate_requiring_sources()` → `kinfo.check_boot_postconditions()` → `boot_alloc::resume_boot_pt_alloc` → `resume_after_v1_decode`；`resume_additive`（`:308-325`）先 `boot_validate_and_prepare`，再 `plan_dm_policy`、`set_current_root_phys`、`store_kernel_info`，最后 `establish_boot_dm`。顺序与文档一致；`resume_additive` 体内确实没有 `validate_before_tables`。
  3. 找“v1 解码即消费”的反例：`decode_*` 只写 `.bss` store，不建表、不映射、不分配；strict 门在 `plan_dm_policy`/`establish_boot_dm` 之前完成。没有找到反例。
  4. 找省略项：`kmain` 在 `:1867` 调 `validate()`，紧接着 `:1874` 还调 `check_boot_postconditions()`；§3.7/§4.4.1 的链只写 `kmain` 的 `validate`，漏了这一次高级门（重复门，不是缺失门）。另外，`Strict` 是 `validate_view` 的最高档，包含 `validate_before_tables` 的全部断言；“v1 不经过 pre-table”容易被读成“v1 直到 kmain 才检查 memmap 非空”，文档没有点明 strict 已覆盖。§4.4.1 的 `kmain` 行括号“QEMU 测试内核合法为空”属于 `validate_before_tables` 的豁免，而 `kmain` 本身在 `qemu_test` 下不编译（`:1821` 的 cfg），该括号落在这一行会误导。
- 结果：反证失败（主张成立）。附两条非阻断观察（P2）：门链漏列 `kmain` 的第二次 `check_boot_postconditions`；“QEMU 测试内核合法为空”的豁免挂错了门。
- 证据：`os/kernel/src/lib.rs:225-238, 308-325, 663-751, 862-937, 953-974, 1821, 1867, 1874`；`os/kernel-image/src/bootface_common.rs:554`；文档行 1844、2374。
- 若我错了：如果能找到 `arch_boot_resume_v1` 在 strict 之前消费解码值的路径，或 `resume_additive` 里实际存在 `validate_before_tables` 调用，我会推翻这条；两者都没有。

---

## CLAIM 05

- 主张：Rust 侧不存在“分页开启前执行、之后可归还”的内核段；所有生产链接布局都是高半 VMA，QEMU 直核测试内核虽 VMA=LMA 但也没有这种段；因此 `bootstrap_start/len` 字段删除是类型级安全，误回收接口面整体消失（文档 §3.4.1 行 1378-1397）。
- 反证尝试：
  1. 查三份链接脚本：`os/kernel-image/x86_64.ld`、`aarch64.ld`、`riscv64.ld` 的生产内核段均以高半 `KERNEL_VIRT_BASE`/`KERNEL_HIGH_BASE` 起址，LMA 由 `AT(...)` 指到物理基址；riscv64 直核面起步执行于物理视图（medany 平移视图），但那是**执行位置**，不是独立链接段。
  2. 找“可单独界定的 bootface 段”：`bootface` 代码只在分页前执行，但与内核其余代码同在一个链接布局里，没有独立起止符号；不能单独归还，文档已说明。
  3. 找“Rust 侧仍有 bootstrap 区间字段”：`KernelInfo`（`os/libs/minix-boot/src/kernel_info.rs:21-31` 起的字段列表）没有 `bootstrap_start/len`；`handoff_v1.rs:287-315` 的信封也没有。`vm_handoff::classify`（`os/kernel/src/vm_handoff.rs:452-568`）只做 `free.cut(...)`，没有 `add_memmap` 式的加回路径。
- 结果：反证失败（主张成立）。
- 证据：三份 `.ld`；`os/libs/minix-boot/src/kernel_info.rs`；`os/libs/minix-boot/src/handoff_v1.rs:287-315`；`os/kernel/src/vm_handoff.rs:452-568`；文档行 1378-1397。
- 若我错了：如果某份生产链接脚本存在低地址独立段，或 Rust 侧保留了“把区间加回空闲表”的接口，这条就不成立。当前两者都没有；文档也已把 QEMU 测试内核和 bootface 的物理视图执行显式排除在陈述外，范围是自洽的。

---

## CLAIM 06

- 主张：`load_mods` 必须写在 `multiboot` 前面；反序时模块链表为空，`HAS_MODS` 仍置位，`get_parameters` 的 `assert(mi_flags & HAS_MODS)` 会通过，真正停住内核的是 `assert(mi_mods_count > 0)`（文档 §2.1 行 239）。`mb_set_param` 的“已存在先删”只对首条记录可靠生效（§2.3 行 406-414）。
- 反证尝试：
  1. 正常反序路径：`boot2.c:453-460` 的 `command_load_mods` 只调 `load_mods(arg, module_add)`；`exec.c:163-193` 的 `module_add_common` 只分配并挂路径；真正的读文件在 `module_init`（`exec.c:501+`），由 `exec_multiboot` 在 `exec.c:739-761` 调用；`HAS_MODS` 在 `btinfo_modulelist` 非空时置位。反序时 `num=0`、`HAS_MODS` 置位，内核确实停在 `pre_init.c:167-169` 的 `assert(mi_mods_count > 0)`。主张的正常路径成立。
  2. 找失败路径反例：`module_init` 在 `btinfo_modulelist = alloc(len)` 失败时提前 `return`（`exec.c` 中 `if (btinfo_modulelist == NULL) { printf("WARNING: couldn't allocate module list\n"); wait_sec(...); return; }`），此时 `HAS_MODS` **不会**置位，内核会停在 `assert(mi_flags & HAS_MODS)`，而不是条数断言。文档“反序时…标志位仍会置上”是绝对句，漏了这条分配失败分支；按文档 §2.6 的诊断口径“先看模块条数，不看标志位”在这条路径上会误诊。另，`module_add_common` 在单条模块分配失败时只打印 “couldn't allocate module” 并继续，模块数可能少于 12，文档的“12 个文件都在”只在分配成功时成立。
  3. `mb_set_param`：`pre_init.c:44-52` 的删除扫描确实在第一轮 `while (*p++)` 越过 NUL 后再 `p++`，使第二条记录从第二个字符起被比较；只有待删键是首条记录时可靠命中。文档的字节图与 `xboard=2` 中段命中例子都能从这段源码推出，主张成立。
- 结果：非阻断（P2，观察）。排序课与 `mb_set_param` 分析成立；“标志位仍会置上”的绝对句有一个稀有但真实的分配失败反例，诊断表需要加一句限定。
- 证据：`minix3/sys/arch/i386/stand/boot/boot2.c:453-460`；`minix3/sys/arch/i386/stand/lib/exec.c:163-193, 501-570, 739-761`；`minix3/minix/kernel/arch/i386/pre_init.c:44-52, 167-169`；文档行 239、406-414、863-867。
- 若我错了：如果 boot2 的 `alloc()` 在这个阶段不可能返回 NULL（例如模块表大小固定且堆必然足够），这条分支在目标环境不可达，那么绝对句可以保留；但源码里这条 `return` 分支存在，文档若声称“必然”就超出了代码事实。修法是把“标志位仍会置上”改成“正常分配下标志位仍会置上；若模块表分配失败，则停在 HAS_MODS 断言”。

---

## 汇总

- 阻断（P1）：CLAIM 02（“接收侧统一走 `store_boot_report`”，与 v1 接收路径及同章四节相反）；CLAIM 03 的 `.data` 静态区断言（生产三条发送腿的 `BootStorage` 都是栈上局部，非 `.data`）。
- 非阻断（P2）：CLAIM 01（I1“句柄不可”与 `PlatformDescSource` 的物理地址句柄措辞冲突）；CLAIM 03 的 `fill_*` 计数措辞；CLAIM 04 的门链省略项；CLAIM 06 的 `module_init` 分配失败路径。
- 反证失败（主张成立）：CLAIM 04（门链顺序）、CLAIM 05（无 unpaged 段/字段删除）、CLAIM 06 的 `mb_set_param` 部分。

---

## 未覆盖

以下是本轮时间/工具边界内没有核到的分支，不计入上面的“主张成立”，后续轮次应补：

- 真机门链与 QEMU 运行：`test-shim-bootmarks-x86.sh`、`test-riscv64-uboot-shim.sh`、`test-kernel-image-riscv64-bootface.sh`、`test-smp-aps-aarch64.sh` 均未执行；§5.2 的读数只做了静态对账（`#[test]` 计数与 feature 门），没有实跑 `cargo test`。
- 跨目标分支：aarch64 / riscv64 目标未编译，`cfg(target_arch=...)` 的分支（`prepare_kernel_image_v1` 的 x86 `cli`、riscv64 的 medany 探针、aarch64 的 `arch_boot_resume_high_half` 标签）只做静态阅读。
- `ebs_pair::pair_and_classify` 的错误重试全路径（`NeedBiggerBuffer`/`RetryExhausted`/`BufferReuseInsufficient`）与 `authorize_and_copy` 的 DTB 边界只读到入口段，未逐分支核对。
- `vm_handoff::classify` 对“U-Boot/shim/暂存文件/DTB 是否在 VM free list 中”的完整生命周期：只核了 arena 合同与 `classify` 的五类扣除源，没有证明所有在 classify 时仍被读的 blob 都被扣除或已死。
- `store_kernel_info` 的一次性全局槽在多测试线程下的隔离行为：只核了守卫（`os/kernel/src/lib.rs:1586-1589`）与生产两个调用点，没有跑 `minix-kernel --features mock` 验证测试间是否互相干扰。
- `BootStorage` 的实际栈帧占用与 UEFI 固件栈余量：只算了结构体约 10 KiB 与 `bootface_common` 的 16 KiB 上限，没有在真机/OVMF 上观测栈余量。

