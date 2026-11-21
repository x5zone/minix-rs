# R36 fact 面第三次独立 certify 回读（冷启动）

- 基线：HEAD `49ed0c54a`（`git rev-parse HEAD` 实测）。
- 范围：`/tmp/r36-cert-fact.md` 全部 44 点，100% 逐条回读，不抽样。
- 纪律：只读源码/文档；未读 `panel/round36` 与任何前两轮回执；每条以 HEAD 的直接读数或实跑为准。
- 实跑：`cargo test --list`（boot-shim 146 / minix-boot 185 / minix-elf 27）、`cargo test -p minix-boot`（185 passed / 0 failed）、`cargo test -p boot-shim --features test-all --lib`（146 passed / 0 failed）、`cargo test -p minix-boot test_boot_postconditions_reject_low_virtual_base`（1 passed）、`cargo check -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none`（Finished）、`tools/anchor-resolve.sh --check`（110/110，multi-def=0）、`tools/doc-citation-check.py`（checked=29 failed=0）。
- 未跑 QEMU（按纪律）。

FACT-A-01：CLOSED — `os/libs/minix-boot/src/handoff_v1.rs:296` 为 `BOOT_HANDOFF_V1_VERSION: u32 = 10`；`handoff_v1_read.rs:107` 收发精确比较；doc:1714「当前 10」、doc:1625「9 → 10」、doc:2932「信封版本为 10」；`cargo test -p minix-boot` 185 passed / 0 failed。QEMU `test-sysboot` 未跑（见局限）。
FACT-A-02：CLOSED — `os/arch/src/arch/paging.rs:212-625` 的 `trait Paging` 实测 1 个关联常量（`:213` `PAGE_SIZE`）+ 23 个方法（`:226` phys_addr_limit 至 `:569` unmap_range）；`:197-210` `PteChannel` 三态（KernelDm/VmDm/Identity）、`:270` `new_from_page(root_page, channel)`；doc:1878「24 个成员」逐一列出；anchor 110/110、multi-def=0。
FACT-A-03：CLOSED — doc:2859 仅存删除记录行 `STARTUP_RECORD_STORE`（v10 已删除）；`grep '七个落地区\|七个分区' doc` = 0；doc:1583/1585/1755/2065/2763/2849 均为「六个落地区/六个分区（加一个名字池）」；`os/libs/minix-boot/src/kernel_info.rs:506-509` strict 门实测改为 `covered(self.memmap) || covered(self.reserved_regions)`；citation checked=29 failed=0。
FACT-A-04：CLOSED — `cargo test --list` 三读数 146/185/27；doc:3223-3243 逐文件表与 `grep -c '#\[test\]'` 实跑逐行相同；doc:3246 的求和 146 与 185 成立；doc:3171/3331「六枚」；`cargo test -p minix-boot` 185 passed / 0 failed。
FACT-A-05：CLOSED — `os/libs/minix-boot/src/kernel_info.rs` `grep -c 'fn test_'` = 24；构成实为 rejects 15（`:694` 起）+ accepts 3（`:682/907/950`）+ requiring_sources 5（`:704/709/871/883/895`）+ postconditions 1（`:960`），与 doc:3267 的 15/3/5/1=24 一致。
FACT-A-06：CLOSED — doc:1655/1657/1659「22 个字段」；台账逐行 22 条，`user_sp` 行在 doc:1672；字段总数逐条复算为 22。
FACT-A-07：CLOSED — doc 中 `commit_map_tables` = 0、`push_startup` = 0；`fill_map_tables` 在 doc:1767/1770/2365/2368/2432；源码 `os/libs/minix-boot/src/boot_storage.rs:140/160/175/193/208` 为 `fill_map_tables/fill_modules/fill_platform/fill_param/fill_memmap_sources`。
FACT-A-08：CLOSED — doc:1770「所有权是**一个**」「只有一个所有者」，并写明六类载荷与 `collect_owned`；doc 中 `两个所有者` = 0；`os/kernel-image/src/bootface.rs:145-146`、`bootface_a64.rs:155-156` 均为单个 `BootStorageCell::take`；`memmap_source.rs:918` `collect_owned` 存在。
FACT-A-09：CLOSED — `grep -rn 'build_memmap()' os/qemu-tests` 可执行调用恰 2 处（`test-memmap/src/main.rs:165`、`test-memmap-aarch64/src/main.rs:29`），另 1 处为 `test-rt-birth-aarch64/src/main.rs:107` 注释；`exit_boot_services_and_classify(` 可执行调用 12 处；doc:3176/3270「2 个测试内核」「14 个可执行调用点」「按文件计 15」与实测一致。
FACT-A-10：CLOSED — doc 中 `x0=0 分道` = 0；doc:1454/3075/3106/3177 均写「接收臂 `x0 == 0` 只诚实停机」；`os/kernel-image/src/bootface_a64.rs:28-34` 模块注释改为 D-061 构建产物分叉，`os/kernel-image/src/main.rs:383-392` 为 `fw-aarch64-uefi-recv` 编译期臂。
FACT-A-11：CLOSED — doc 中 `vm_handoff.rs:605` = 0；`anchor-resolve.sh --check` 实测 anchors=110 resolved=110 zero-def=0 multi-def=0。
FACT-A-12：CLOSED — `grep -c 'cfg(feature = "qemu-test-kernels")' os/boot-shim/src/uefi_helpers.rs` = 10；doc:1528 写「10 处」并列出 `UefiPoolAllocator`、`TestMapTables`、`build_memmaps`/`build_memmap`、`exit_boot_services`、`classify_map_bytes_into_test_tables`、`exit_boot_services_and_classify`；10 处 gate 逐行核对（:157/160/403/415/418/427/500/573/1273/1303）。
FACT-A-13：CLOSED — `grep -c 'land_slice(' os/kernel/src/lib.rs` = 4（:1601/1613/1660/1681）；doc:2768「四处调用」；doc:2849 容量表为六个落地区 + 一个名字池。
FACT-A-14：CLOSED — doc:3305「在最终 EBS 内存图之前」；doc 中 `build_memmap() 产出的 conventional` = 0；`os/qemu-tests/.../test-kernel-map/src/main.rs:70-96` 探针分配先于 `exit_boot_services_and_classify()`（:96）。
FACT-A-15：CLOSED — 源码生产调用点恰 2 个：`os/kernel/src/lib.rs:225`（`arch_boot_finish`）、`:347`（`resume_additive`）；`kmain` 形参 `_kernel_info`（:1806）且 :1874-1880 明写 no re-store，无 `store_kernel_info` 调用；一次性守卫在 :1580-1583；doc:2867「生产路径有两个 `store_kernel_info` 调用点」、doc:2768 引文头补「开头的一次性守卫 assert 与相邻接续行有省略」。
FACT-B-01：CLOSED — 同 FACT-A-01：`handoff_v1.rs:296` = 10，doc:1714/1625/2932 一致，`cargo test -p minix-boot` 185/0；`test-sysboot` 未跑（局限）。
FACT-B-02：CLOSED — 同 FACT-A-07：doc 中 `commit_map_tables`/`push_startup` = 0，`fill_map_tables` 闭包在 doc:2365/2368 与源码 `boot_storage.rs:140` 一致。
FACT-B-03：CLOSED — 同 FACT-A-03：startup 只作删除记录（doc:2859），现役统一为六个落地区 + 名字池（doc:1583/1585/1755/2065/2763/2849），strict 门源码 `kernel_info.rs:506-509`。
FACT-B-04：CLOSED — 同 FACT-A-15：两个生产调用点（lib.rs:225/347）、kmain 不重存（lib.rs:1874-1880）、一次性守卫（lib.rs:1580-1583）、doc:2867 与 doc:2768 已同步。
FACT-B-05：STILL-BROKEN — 用户点名的面均已修：doc:2646 标题「两条产出入径与一条校验门链」、doc:2738「镜像地址下限（`kern_virt_base > u32::MAX`）」、doc:2754 表行「`kern_virt_base > u32::MAX`（4 GiB 门）」、doc:3267「`kern_virt_base > u32::MAX`（4 GiB 下限）」；§5.1 已不再描述该门为 higher-half；`kernel_info.rs:527-537` docstring、`:563-567` 断言消息（「不大于 u32::MAX（4 GiB 下限）」）、`:958-968` 负控测试与注释均改；`os/kernel/src/lib.rs:1851-1859` kmain 注释同；`cargo test -p minix-boot test_boot_postconditions_reject_low_virtual_base` → ok。残留原文（`os/libs/minix-boot/src/kernel_info.rs:721-723`，测试模块内、按 Rust 属性规则悬挂在 `test_validate_rejects_unordered_memmap` 上的孤儿 doc 注释）：
    /// D01-R1 D2-04: a stack top past the image end is representable in
    /// the type but semantically impossible — the postcondition gate must
    /// catch it (first push after `mov sp` would hit unmapped memory).
  该注释仍把已随 v10/D-062 删除的 `kern_stack_top` 越界关系写成现行 postcondition 门的职责（同文件 :621 `use minix_types::USER_STACK_TOP;` 也已无使用者并产生 unused import 告警）。最小修复：删除 :721-726 两段孤儿 doc 注释（或改写为「该关系已随字段删除；剩余负控是 `test_boot_postconditions_reject_low_virtual_base`」），并删除 :621 未用 import。
FACT-B-06：CLOSED — 同 FACT-A-04：`cargo test --list` 146/185/27，逐文件表实跑逐行相同，doc:3171/3331「六枚」，minix-boot 185/0。
FACT-B-07：CLOSED — 同 FACT-A-05：kernel_info 24 条，构成 15/3/5/1 与 doc:3267 一致。
FACT-B-08：CLOSED — doc:3331 已改「六枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025 / source 297），各持 boot-globals 锁」；源码 `os/kernel/src/lib.rs:6044/6060/6076/6093/6110/6123` 恰 6 条 `#[should_panic]`，每条都有 `crate::test_sync::lock_boot_globals()`；doc:3171 M8 同口径。
FACT-B-09：CLOSED — 同 FACT-A-11：`vm_handoff.rs:605` = 0，anchor 110/110 multi-def=0。
FACT-B-10：CLOSED — 同 FACT-A-15：doc:2768 引文头明写「开头的一次性守卫 assert 与相邻接续行有省略」，源码守卫在 `lib.rs:1580-1583`。
FACT-B-11：CLOSED — doc:3264 已写「`kern_stack_top` 由访问器派生；`user_sp`/`syscall_entry` 已不上线」；`os/boot-shim/src/opensbi_helpers.rs:806-841` 的测试实测只断言 memmap/kern_virt_base/kern_phys_base/kern_size/kern_stack_top()/boot_modules/platform_sources，无 syscall_entry 字段断言。
FACT-B-12：CLOSED — doc:2572 有 `elf_file_end(data)` 两行（修剪在 `load_boot_modules_into`）；源码 `os/boot-shim/src/loader.rs:517-518` 一致；`opensbi_helpers.rs:198-215` 的 `UbootFileLoader::read` 已无 `segment_iter`；citation checked=29 failed=0。
FACT-B-13：CLOSED — doc:3468 已改「Step 2b/2c 也已完成，卡在 Step 3 `enable` 或跳转」；源码 `os/kernel/src/lib.rs:1429`（Step 2b）、`:1475`（Step 2c）、`:1519`（`step1+2 mappings ok`）顺序成立。
FACT-B-14：CLOSED — 同 FACT-A-10：`x0=0 分道` = 0，「诚实停机」在 doc:1454/3075/3106/3177；`bootface_a64.rs:28-34` 与 `main.rs:383-392` 为 D-061 编译期分叉。
FACT-B-15：CLOSED — doc:2605 已写 `r > 0 && r < 4096 - A`；`grep 'r > 4096 - A' doc` = 0；反例 `data.len()=4095, A=4094` 的算式复核成立。
FACT-B-16：CLOSED — 同 FACT-A-08：doc:1770「只有一个所有者」+ 六类载荷 `fill_*`，`两个所有者` = 0；源码 `bootface.rs`/`bootface_a64.rs` 各一个 `BootStorageCell`。
FACT-B-17：CLOSED — 同 FACT-A-14：doc:3305「最终 EBS 内存图之前」与 `exit_boot_services_and_classify()` 来源；`test-kernel-map/src/main.rs:70-96` 顺序一致。
FACT-B-18：CLOSED — 同 FACT-A-09：`build_memmap()` 可执行调用 2 处，classify 12 处，doc:3176/3270 的 2/12/14/15 口径一致。
FACT-B-19：CLOSED — doc:2608 已改「`validate` 只查 `boot_modules` 非空，按位置绑定会静默少初始化几个进程」；doc 中「内核侧的数量断言」= 0；源码 `kernel_info.rs:412-419` 的 validate 仅断言非空，`os/kernel/src/lib.rs:2768-2774` 的 `init_proc_and_boot` 另有 `assert_eq!(boot_modules.len(), NR_BOOT_MODULES, "expected {} boot modules, found {}")`（doc:3272 正确写了它「不会触发」：静默跳过时 len 仍为 12），strict 位置核名在 `kernel_info.rs:444-452`。注：清单原文「内核侧没有这条数量断言」与 HEAD 不符（断言在 `init_proc_and_boot`，经 kmain 调用），当前文档措辞反而与源码一致。
DESIGN-D2-01：CLOSED — 同 FACT-A-01：常量 10、doc 当前 10/版本历史 9→10、minix-boot 185/0；`test-sysboot` 未跑（局限）。
DESIGN-D2-02：CLOSED — 同 FACT-A-03：startup 仅删除记录、六落地区、strict 门 memmap/reserved、citation 29/29。
DESIGN-D2-04：CLOSED — `os/kernel-image/src/bootface_a64.rs:54/58` 引入 `AtomicU64`/`BumpRegion`，`:115-136` 的 `bump_alloc_pages` 用 `BumpRegion::resumed` + 原子游标；`cargo check -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none` 完成（仅 warning）；`test-rt-birth-aarch64` 未跑（QEMU，局限）。
DESIGN-D2-06：CLOSED — 同 FACT-A-02：trait 24 成员（1 const + 23 fn），doc:1878 全列；`PteChannel` 三态与 `new_from_page(..., channel)` 在 doc:2082/2092/2979 有述；anchor 110/110。
DESIGN-D2-07：CLOSED — `os/boot-shim/src/uboot_main.rs:293` `static mut SHIM_TABLE: BootFileTable`，`:301` 有 `size_of::<BootFileTable>() == 16 + 16 * 80` 编译期断言；`grep '\[u8; 1296\]' os/boot-shim/src` = 0；`test-riscv64-uboot-shim.sh` 六路标在脚本 :206-209 + :231（QEMU 未跑，局限）。
DESIGN-D2-08：CLOSED — 同 FACT-B-12：doc:2572 `elf_file_end`、`UbootFileLoader::read` 无 `segment_iter`、citation 29/29 failed=0。
CLAIM-03：CLOSED — 同 FACT-A-01：`handoff_v1.rs:296` = 10，doc:1714/1625/2932，minix-boot 185/0（`test-sysboot` 未跑）。
CLAIM-04：CLOSED — 同 FACT-A-03：doc:2763「六个落地区加一个名字池」、doc:2867 两个生产调用点、strict 门 `kernel_info.rs:506-509`、citation 29/29。
CLAIM-05：CLOSED — doc:1925 与 doc:2939-2940 的现行文字是「2 MiB 分支拒绝已存在的 1 GiB 叶或 2 MiB 叶，调用方把 `AlreadyMapped` 当成功吸收，不重写已有叶」，与源码一致：x86 `os/arch/src/x86_64/paging.rs:1309-1311/1325-1327`、arm64 `os/arch/src/arm64/paging.rs:1235/1248`、riscv64 `os/arch/src/riscv64/paging.rs:1234/1250`；`grep '重复安装同一个叶' doc` → doc:1925；commit `e4374af7e` 存在且是 HEAD 祖先。注：清单原文「2 MiB 路径直接重写同一个叶并返回 Ok」与 HEAD 源码不符，当前文档没有采用该错误结论。
CLAIM-06：CLOSED — 同 FACT-A-04：146/185/27，逐文件表实跑一致，doc:3171/3331「六枚」，minix-boot 185/0。

## 汇总

- CLOSED：44 / 44（certify #3 残项复读 CLOSED；见文末「残项复读」节）。
- STILL-BROKEN：0 / 44。FACT-B-05 的原 STILL-BROKEN 判定已由 certify #3 残项复读推翻：`kernel_info.rs` 的孤儿测试 doc 注释与未用 `USER_STACK_TOP` import 已删，`lib.rs` 两段 docstring 已对齐两个生产调用点与二次 store panic。
- 前两轮（A-01/B-01/D2-01/CLAIM-03 的 `test-sysboot PASS`，D2-04 的 `test-rt-birth-aarch64`，D2-07 的 `test-riscv64-uboot-shim 6/6`）涉及的 QEMU 结论本轮未运行，只核对了脚本标记与源码断言，见「局限」。

## 跨项观察（不在 44 点判据内，但与 A-15/B-04/B-10 同源）

`os/kernel/src/lib.rs:1544-1547` 的 `land_slice` docstring 仍写「the same-address case is skipped so `kmain` can re-store its own global copy」，`:1563-1573` 的 `store_kernel_info` docstring 仍写「kmain Phase A calls this」「A later call updates the slot in place」。这两处与 HEAD 源码（`kmain` 不调用、`lib.rs:1580-1583` 第二次 store 直接 panic）以及本轮已修的 doc:2867（「两个 `store_kernel_info` 调用点」「`kmain` 不再调用」）互相矛盾。该 docstring 早于本轮存在，但本轮的 .md 修正使其与源码注释的冲突显式化；建议后续把两段 docstring 改为「两个生产调用点（`arch_boot_finish`/`resume_additive`）+ 一次性守卫」，与 .md 对齐。

## 局限

1. QEMU 门链未运行：`test-sysboot`、`test-riscv64-uboot-shim`、`test-rt-birth-aarch64` 的 PASS 状态不是本轮直接观测，只核对了脚本存在性、六路标 grep 与源码断言；对应条目的「PASS」证据等级为源码/脚本级，不是运行期级。
2. 同 agent 验证：所有 grep/测试由本 session 同一模型重放，没有跨工具只读复核；结论以可重放命令为准。
3. 未读 `panel/round36` 与任何前两轮回执，因此本回执不引用它们的结论，也不对它们的发现质量做评价。
4. 测试内核计数按 `#[test]` 出现次数口径；doc-tests 不计入（与文档口径一致）。

## 残项复读（certify #3，HEAD `624fcccf1`）

复核范围：仅 FACT-B-05 的三处残留；基线 `624fcccf158c4c24fc9043e89aaa287856234f1f`（`git rev-parse HEAD` 实测）。未跑 QEMU。

1. `os/libs/minix-boot/src/kernel_info.rs` 测试模块：CLOSED — `use minix_types::USER_STACK_TOP;` 已删（测试模块现在只有 `use super::*;`；全文 `USER_STACK_TOP` 仅剩 :434 与 :553 两处注释里的历史/语义说明，不是 import）；原 :721-726 的两段孤儿 doc 注释已替换为普通注释 :719-722：「v10 (D-062) deleted the derived `kern_stack_top` field and its "stack top past the image end" relation; the remaining postcondition negative control is `test_boot_postconditions_reject_low_virtual_base` below.」；`cargo test -p minix-boot --no-run` 对 kernel_info.rs 无 unused import 告警。
2. `os/kernel/src/lib.rs` 的 `land_slice`/`store_kernel_info` docstring：CLOSED — `land_slice` 现写「the same-address case is skipped so `resume_additive` can re-store the slices it decoded into the store」（:1546-1548）；`store_kernel_info` 现写「Production has two call sites: `arch_boot_finish` (a real copy) and `resume_additive` (same-address skip). `kmain` reads the slot through `kernel_info()` and does not re-store」与「The slot is written exactly once per leg (I-35(a)/D-071): a second store panics instead of updating the slot in place」（:1567-1573）；生产调用点实测仍为 :225（`arch_boot_finish`）与 :347（`resume_additive`）。
3. 旧描述指向 `check_boot_postconditions`：CLOSED — `grep -rn -i --exclude-dir=target 'higher.half address|higher.half relation|higher.half propert|高半地址关系' os/ rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` = 0（exit 1）；`grep -rn --exclude-dir=target 'postcondition gate must catch|后置条件门必须' os/` = 0（exit 1）；唯一 `stack top past the image end` 命中是 :721 的删除说明，明确写 v10 已删并指到 `test_boot_postconditions_reject_low_virtual_base`，不构成对现行门的旧描述。

结论：三处均 CLOSED。FACT-B-05 由 STILL-BROKEN 改为 CLOSED；本文件前文 FACT-B-05 行与「跨项观察」中的对应残留描述均被本节取代。`cargo test -p minix-boot test_boot_postconditions_reject_low_virtual_base` 复跑仍为 1 passed / 0 failed（184 filtered out）。

- 历史回执：本 pass 的结论以正文汇总为准（pass1 38 CLOSED/6 STILL-BROKEN、pass2 43/1）；本轮最终结论 CLOSED/PASS（44/44，见 `round36-fact.md`）。

- V3.6 遗留声明：本轮 edition V3.4；V3.6 的减法门与“修复点=制品类别射程”门未作为轮门执行，按 §6A 的 V3.4 口径关闭。
