# FACT-B 报告（R36，§4–§6 逐行核对 + 全文测试对账）

审查者：P-FACT-B（独立子 agent）。只读仓库。
机械门读数（命令与退出码均已在会话中实跑）：
- `tools/anchor-resolve.sh`：exit 0，`anchors=108 resolved=108 zero-def=0 multi-def=0`
- `tools/doc-code-map.sh`：exit 0，`共 54 个文件；存在性 54/54 通过`
- `python3 tools/doc-structure-check.py`：exit 0
- `tools/doc-ref-check.sh --check`：exit 0
- `tools/doc-style-lint.sh`：exit 0，`error级=0`
- `python3 tools/doc-citation-check.py`：`blocks=69 checked=29 failed=0 skipped=35 warned=5`（WARN 块见 FACT-B-02/03 手工核对）

## FINDING FACT-B-01（P0-fact）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.4.1（第 1625 行）与 §4.6（第 2920 行）；源码 `os/libs/minix-boot/src/handoff_v1.rs:288`（`BOOT_HANDOFF_V1_VERSION`）
- 严重度：P0
- 问题：文档声称 wire 版本已 `9 → 10` 且「v9 与 v10 互相拒收」，但仓库实际常量仍是 **9**；同一篇文档 §4.6 又说「信封版本仍为 9」，两处互相矛盾，源码只支持 9。
- 影响：读者会相信布局变更（信封 160→128、删 startup 分区）已有版本隔离。实际上新旧两种 9 版信封会被收发双方**当作同一版本接受**，解析错位而非拒收——这正是版本号要防的故障。D-062 的「原子升版」验收不成立。
- 证据或推理：E1。`grep -n 'BOOT_HANDOFF_V1_VERSION' os/libs/minix-boot/src/handoff_v1.rs` → `288:pub const BOOT_HANDOFF_V1_VERSION: u32 = 9;`；`git show 52110580f -- os/libs/minix-boot/src/handoff_v1.rs` 的 diff 没有任何版本号 hunk（只有 commit message 写 `9 → 10`）；`git log --all -G 'BOOT_HANDOFF_V1_VERSION.*10'` 零命中。文档第 1625 行写「`BOOT_HANDOFF_V1_VERSION` 9 → 10，收发两侧精确比较，v9 与 v10 互相拒收」；第 2920 行写「信封版本仍为 9（见 §3.4.3）」；§3.4.3 也写「当前 9」。
- 最小修复：按冻结裁决 D-062 把 `BOOT_HANDOFF_V1_VERSION` 改为 10（连同接收侧精确比较的负控），并统一 §3.4.3/§4.6 的「9」表述；若裁决改判不升版，则删除全文所有 v10 声明与「互相拒收」句。二选一，不能两套并存。落到代码侧这同时是一条 **P0-code-bug**（D-062 的升版未落地，版本隔离失效）；落到文档侧是 P0-fact。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能仓库用 `size`/`arena_len` 字段做二次判别从而仍能拒收旧布局；但 `decode_envelope` 的版本比较是精确比较，`size` 只在版本之后校验，不能替代版本隔离。另可能工作树有未提交的升版改动；`git status` 显示 `os/` 无改动，已排除。
- 对应判据：文档-代码同步七项之「常量」；测试/代码声明须可复跑（实测命令与输出）；P0-fact。

## FINDING FACT-B-02（P0-fact）
- 位置：文档 §4.2（第 2379–2381 行引文块）与 §4.2.1 动作 7 行（第 2418 行）；源码 `os/boot-shim/src/uefi_helpers.rs:1149–1218`、`os/libs/minix-boot/src/boot_storage.rs:140`
- 严重度：P0
- 问题：文档代码引用块与动作表都调用 `storage.commit_map_tables(classification.conventional, classification.reserved)`，引文里还调用 `storage.map_tables_mut()`，这两个方法在仓库中**都不存在**；实际提交发生在 `storage.fill_map_tables(|memmap_slots, reserved_slots| loop { … })` 闭包返回 `MapCounts` 时，槽数组由闭包形参给出。
- 影响：读者按引文去源码找 `commit_map_tables`/`map_tables_mut` 会连续落空；更重要的是「动作 6/7 是两次独立调用、EBS 之后单独提交计数」的时间序被讲错——真实实现把配对循环**包在** `fill_map_tables` 闭包内，写与提交是一步（这也正是 §3.4.5 与 D-067 反复强调的不变量）。引文因此与它自己引用的设计不变量冲突。
- 证据或推理：E1。`grep -rn 'commit_map_tables\|map_tables_mut' --include=*.rs os/` → 两者均零命中；`grep -rn 'fill_map_tables' os/boot-shim/src/uefi_helpers.rs` → `1156: storage.fill_map_tables(|memmap_slots, reserved_slots| loop {`。doc-citation-check 把 2313–2402 标为 WARN，与此处一致。
- 最小修复：删掉引文中的 `commit_map_tables` 与 `map_tables_mut` 两行，改为 `storage.fill_map_tables(|memmap_slots, reserved_slots| loop { … })` 的真实骨架（或把该行改述为「闭包返回 MapCounts，`fill_map_tables` 内部提交」），并在引文头补「配对循环整体位于 fill_map_tables 闭包内」这条省略说明。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `commit_map_tables` 在某个 feature 或历史分支里存在；`grep --include=*.rs os/` 覆盖全工作树，当前树零命中。
- 对应判据：任务项 5（引文块逐字一致/节选标注）；文档-代码同步七项之「符号锚点」；P0-fact。

## FINDING FACT-B-03（P0-fact）
- 位置：文档第 4 章导语（第 2271 行）、§4.2（第 2377、2383、2404 行）、§4.2.1（第 2422、2424 行）、§4.5（第 2751、2756、2837、2847、2853、2855、2857 行）、§4.7（第 3027 行）、§5.2.3（第 3297 行）、§6.3（第 3453、3460 行）；另在 §3 内有同根因实例（属 P-FACT-A 范围，供合并）：§3.4.1 第 1585 行、§3.4.3 第 1749、1753 行、§3.7 第 2059 行。源码 `os/kernel/src/globals.rs`、`os/libs/minix-boot/src/handoff_v1.rs`
- 严重度：P0
- 问题：v10（D-062）已删除 startup 分区与 `STARTUP_RECORD_STORE`，但 §4–§6 仍有成组陈述把它们当成现存对象：`STARTUP_RECORD_STORE`/`STARTUP_RECORD_STORE_LEN`/`WIRE_STARTUP_CAP`（§4.5 容量表第 2847 行）、「七个落地区」「七个分区」「七块」「七个载荷」（第 4 章导语第 2271 行、§4.5 标题与第 2837/2853/2855/2857 行、§4.7 第 3027 行、§6.3 第 3460 行）、「EBS 之后由省略段补写 startup」「6b 动作」「startup 记录由省略段直写 wire」（§4.2 第 2377/2383 行、§4.2.1 第 2422/2424 行）、「startup-object 记录仍保留 arena 的身份」（§4.7 第 3027 行）、「第三条是内存图来源行，在 startup 写入与三道断言之后」（§5.2.3 第 3297 行）、「decode_* … memmap/reserved/modules/platform/param/source/startup 七个分区」（§6.3 第 3453 行）。源码里这些常量与 store 全部不存在，实际落地区是 6 个（reserved/memmap/boot_module/platform/param/source）加 1 个名字池，wire 载荷分区是 6 个（无 startup）。
- 影响：§4.5 自称「全文交接容量的唯一权威」，却让读者按 7 个落地区与 7 个 wire 分区检查代码；按文档验收会在 `STARTUP_RECORD_STORE` 上落空。§4.2 的 6b 时间序整段描述一个不存在的写段，读者会把「EBS 后补写」当成现行行为。
- 证据或推理：E1。`grep -rn 'STARTUP_RECORD_STORE\|STARTUP_RECORD_STORE_LEN\|WIRE_STARTUP_CAP' --include=*.rs os/` → 零命中（唯一残留是 `handoff_v1_read.rs:763` 一句注释里的 `WIRE_STARTUP_CAP`）；`grep -n 'static .*_STORE' os/kernel/src/globals.rs` → 7 个 static，其中名字池 `BOOT_MODULE_NAME_STORE` 属池，数据落地区 6 个；`grep -n 'land_slice(' os/kernel/src/lib.rs` → 4 处（reserved/memmap/platform/source），另 boot_modules 与 param 内联。`os/boot-shim/src/uefi_helpers.rs:1220` 的 v10 注释明确「removed the startup-object partition」。
- 最小修复：把 §4.2 引文里的 startup 省略行与 §4.2.1 的 6b 段落删除或改成「v10 起不存在」；§4.5 标题与容量表删 `STARTUP_RECORD_STORE` 行，把「七个落地区/七个分区/七块」统一改为 6+1（或「六个落地区加一个名字池」）；§6.3 第 3453 行分区枚举删 startup、第 3460 行「七块」改「六块加名字池」。修后重跑 `anchor-resolve` + `citation-check`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能「七个分区」把信封本身算作一区，从而 6 个载荷 + 信封 = 7；但 §4.5 是「落地区」不是 wire 分区，且容量表明确列出 `STARTUP_RECORD_STORE` 这一不存在的实体，无法用信封口径解释。
- 对应判据：文档-代码同步七项之「常量/路径/符号锚点」；任务项 4（常量与行为合同）；P0-fact。

## FINDING FACT-B-04（P0-fact）
- 位置：文档 §4.5 第 2855 行；源码 `os/kernel/src/lib.rs:1806`（`kmain`）与 `:1870–1877`（D-071 注释）
- 严重度：P0
- 问题：文档写「`store_kernel_info` 有三个调用点……`kmain` 拿到的是全局副本、再存一次也是同址。三个生产调用点现在都使用返回值：`kmain` 用 `let kernel_info = store_kernel_info(kernel_info);` 继续读同一槽」。实际 D-071 已删除 `kmain` 的 re-store，`kmain` 形参带下划线且不再调用 `store_kernel_info`；生产调用点只有两个（`arch_boot_finish`、`resume_additive`）。
- 影响：读者会以为存在第三个调用点并据此理解 `land_slice` 的同址守卫；实际 kmain 的路径是直接读全局槽，第二次 store 现在会触发 one-shot 断言 panic（而不是「同址跳过」）。这条错误还会让「第二通道」与「同址守卫」两段论证失去一半前提。
- 证据或推理：E1。`grep -rn 'store_kernel_info(' --include=*.rs os/kernel/src/lib.rs` → 生产调用仅 `:225`、`:347`；`:1806 pub fn kmain(_kernel_info: &KernelInfo)`；`:1874–1877` 注释写「no re-store here … a second store would rewrite a live &'static slot (now a guard failure, see store_kernel_info)」。文档引文块 2755–2835 也未包含 `store_kernel_info` 开头的 one-shot `assert!`。
- 最小修复：把第 2855 行改为「两个生产调用点（`arch_boot_finish` 真拷贝、`resume_additive` 同址跳过）」；`kmain` 一句改为「`kmain` 通过 `kernel_info()` 读同一 `.bss` 槽，不再 re-store（D-071）」；同时把引文头「注释与相邻接续行有省略」补上「开头的一次性守卫 assert 省略」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `#[cfg(feature = "qemu_test")]` 的另一份 `kmain`（`:2207`）仍调用 store；但那不是生产入口，且文档说的是生产三调用点。
- 对应判据：文档-代码同步七项之「符号锚点/路径」；任务项 3（文档声称的函数调用是否真实存在）；P0-fact。

## FINDING FACT-B-05（P0-fact）
- 位置：文档 §4.4 第 2726、2727、2742、2746 行；源码 `os/libs/minix-boot/src/kernel_info.rs:255`（`validate_requiring_sources`）、`:472–506`（strict 门实现）、`:554–570`（`check_boot_postconditions`）
- 严重度：P0
- 问题：§4.4 用两层描述虚报了现行校验面。第一层是 strict 门：文档说它「另要求 `startup_objects` 至少一条覆盖内核镜像跨度（startup 不按 authority 过滤）」，并「核对三条派生标量（`user_sp`/`syscall_entry`/`kern_stack_top`）等于其唯一来源」；源码中 `startup_objects` 已不存在，三条派生标量断言也已删除，现行等价检查是「内核物理跨度必须落在某条 `memmap` 或 `reserved_regions` 区间内」。第二层是 `check_boot_postconditions`：文档（第 2726 行校验层次第 3 条与第 2742 行门表）说它查「`syscall_entry` 落在镜像跨度内；`kern_stack_top` 不越过跨度上界；用户栈不越入内核半区」；源码里这三条读取字段的断言都随 D-062 删除，**只剩一条** `kern_virt_base > u32::MAX`（higher-half）。
- 影响：读者会把两组不存在的门当成现行防线：一个校验不存在的字段，一个校验不存在的恒等式。门表最后一行「生产者漏写 provenance、声明了没有记录的权威、伪造了模块名或派生标量」中的「伪造派生标量」不可能发生（值不再上线）；`check_boot_postconditions` 实际防线被高估三条，排查时会向一个不存在的断言归因。
- 证据或推理：E1。`grep -n 'startup_objects' os/libs/minix-boot/src/kernel_info.rs` → 零命中；`check_boot_postconditions` 函数体（`:554–570`）只有 `assert!(self.kern_virt_base.0 > u64::from(u32::MAX), ...)`；源码注释 `:434–439` 明确「v10 removed the three derived-value equalities」，`:555–564` 明确「these four assertions … are gone」。`requires_startup_kernel_record()` 的实现是「内核跨度落在 memmap/reserved」（`:488–506`）。
- 最小修复：第 2726 行第 3 条与第 2742 行门表改为「仅 `kern_virt_base` 必须在 higher half（v10 后唯一剩余的后置条件）」；第 2727 行删「另要求 `startup_objects` …」并改为「另要求内核物理跨度落在 `memmap` 或 `reserved_regions` 的某条区间内」；第 2746 行做同样替换并删「核对三条派生标量…」与「伪造派生标量」字样。若要恢复后置条件强度，应在代码里补真实断言，而不是在文档里补描述。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `check_boot_postconditions` 在某个 target/feature 下有另一份实现；`grep -rn 'fn check_boot_postconditions' os/` 全树只有一个定义。
- 对应判据：文档-代码同步七项之「符号锚点/常量」；任务项 4（行为合同）；P0-fact。

## FINDING FACT-B-06（P0-fact）
- 位置：文档 §5.2.1 第 3182–3234 行（含实测转录与逐文件表）、§5.1 M8 第 3157 行、§5.3 TODO-03 第 3317 行；源码实测见证据
- 严重度：P0
- 问题：文档自称「实跑」的测试数与当前仓库不符，且逐文件表有 6 个文件过期。文档：boot-shim 136 / minix-boot 195 / minix-elf 23；实测：146 / 184 / 27。逐文件：`loader.rs` 22→31、`kernel_info.rs` 31→23、`memmap_source.rs` 74→75、`handoff_v1_read.rs` 21→17、`resv_claims.rs` 13→14、`minix-elf/src/lib.rs` 23→27。kernel 侧 `dm_coverage.rs` 10→11、`x86_64/paging.rs` 17→20、`store_kernel_info` 系列 8→7。
- 影响：§5 的全部覆盖结论都建立在「这些数字是实跑读数」上；数字过期意味着「哪些测试存在、哪些契约被覆盖」的清单不可信。读者按文档的 `cargo test` 命令核对会立刻对不上，进而无法判断是文档旧还是代码坏。
- 证据或推理：E1。`cargo test -p boot-shim --features test-all --lib -- --list` → `146 tests`；`cargo test -p minix-boot --lib -- --list` → `184 tests`；`cargo test -p minix-elf --lib -- --list` → `27 tests`。逐文件 `grep -c '#\[test\]'` 见上。`grep -rn 'fn test_store_kernel_info_rejects_' os/kernel/src/lib.rs` → 6 个（无 startup）；`grep -c '#\[test\]' os/kernel/src/dm_coverage.rs` → 11；`os/arch/src/x86_64/paging.rs` → 20。
- 最小修复：重跑文档给出的两条命令与逐文件循环，整表替换数字；M8/TODO-03 的「七枚」改 6，删 startup 负控项；§5.2.1 kernel 侧三个计数同步。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `cargo test --list` 把 `#[should_panic]` 与 feature-gated 测试计入方式与 `grep -c` 不同；但文档明说「按 `#[test]` 出现次数计」，两套口径我都跑了，结论一致（且 146/184/27 与逐文件加总一致）。
- 对应判据：任务项 2（测试计数）与任务项 3（测试名对账）；P0-fact。

## FINDING FACT-B-07（P1-fact）
- 位置：文档 §5.2.1 第 3253 行；源码 `os/libs/minix-boot/src/kernel_info.rs`
- 严重度：P1
- 问题：文档写 `kernel_info.rs` 的构成「`test_validate_rejects_*`（16 条）与 `test_validate_accepts_*`（3 条），另加 `test_validate_requiring_sources_*`（8 条）与 `test_postconditions_reject_*`（4 条）= 该文件 31 条」。实际前缀计数为 15 / 3 / 5 / **0**，文件共 23 条；`test_postconditions_reject_*` 一条都不存在。
- 影响：四个测试族名中有一个整族是虚构的；读者会去找四条「postconditions 负控」并据此认为 `check_boot_postconditions` 的边界被覆盖。v10 删掉四个断言后，postcondition 只剩一条 higher-half 断言，且没有专门的负控测试。
- 证据或推理：E1。`grep -c 'fn test_validate_rejects'` → 15；`test_validate_accepts` → 3；`test_validate_requiring_sources` → 5；`test_postconditions_reject` → 0；`grep -c '#\[test\]'` → 23。`grep -rn 'postcondition' os/libs/minix-boot/src/kernel_info.rs` 只有函数定义与文档注释，无测试名。
- 最小修复：按实际重写该格：15 + 3 + 5 = 23，删 `test_postconditions_reject_*` 族；若要保留 postcondition 覆盖声明，先补真实测试再写。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能测试函数在 `os/kernel` 侧而非 `kernel_info.rs`；`grep -rn 'test_postconditions_reject' os/` 全树零命中。
- 对应判据：任务项 3（测试名对账）；P1-fact。

## FINDING FACT-B-08（P1-fact）
- 位置：文档 §5.3 TODO-03 第 3317 行、§5.1 M8 第 3157 行；源码 `os/kernel/src/lib.rs:6044–6129`
- 严重度：P1
- 问题：TODO-03 写「七枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025 / source 297 / startup 21）」；仓库只有六枚，`startup 21` 对应的 `test_store_kernel_info_rejects_startup_overflow` 不存在。
- 影响：读者会认为 startup 落地区仍有溢出负控；实际该落地区已删除，这条覆盖是虚报。
- 证据或推理：E1。`grep -rn 'fn test_store_kernel_info_rejects_' os/kernel/src/lib.rs` → reserved/memmap/boot_module/platform/param/source 六条，无 startup。
- 最小修复：删「/ startup 21」并把「七枚」改「六枚」；M8 同步。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能负控名不带 `test_store_kernel_info_rejects_` 前缀；`grep -rn 'startup_overflow' os/` 全树零命中。
- 对应判据：任务项 3（测试名对账）；P1-fact。

## FINDING FACT-B-09（P2-fact）
- 位置：文档 §4.1 第 2303 行（`os/kernel/src/vm_handoff.rs:605`）、§4.7 第 3021 行（`os/kernel/src/lib.rs:741`）
- 严重度：P2
- 问题：两处手写行号锚点指向错误行：`kernel_allocated_dynamic: boot_alloc_used_bytes()` 在 `vm_handoff.rs:663`（不是 605）；`resume_after_v1_decode` 的三个 cfg 定义在 `lib.rs:773/781/800`（741 附近是 `decode_param_buf`）。
- 影响：读者按行号跳转会落到无关代码；行号锚点还会随源码漂移，属于项目锚点约定明确禁止的手工维护形态。
- 证据或推理：E1。`grep -n 'kernel_allocated_dynamic' os/kernel/src/vm_handoff.rs` → `663:`；`grep -n 'fn resume_after_v1_decode' os/kernel/src/lib.rs` → 773/781/800。
- 最小修复：删行号，改符号锚点（`os/kernel/src/vm_handoff.rs:fn build_vm_handoff` 或直接写字段名；`os/kernel/src/lib.rs:fn resume_after_v1_decode`）。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能作者引用的是历史提交的行号；文档面向现行源码，仍应更新。
- 对应判据：`prompt/review-rules/review-process.md` Step 1.0 锚点约定（禁止手工行号）；文档-代码同步七项之「路径/符号锚点」。

## FINDING FACT-B-10（P2-fact）
- 位置：文档 §4.5 第 2756 行引文头与第 2772 行起的 `store_kernel_info` 引文；源码 `os/kernel/src/lib.rs:1574–1583`
- 严重度：P2
- 问题：引文头称只省略「注释与相邻接续行」，但源码函数体开头的 one-shot 守卫 `assert!(KERNEL_INFO.get().is_none(), "store_kernel_info: a second store would rewrite a live &'static slot")`（非注释、非接续行）被略去，正文也未提。
- 影响：读者看不到「第二次 store 会 panic」这条现行合同，而 §4.5 正文恰在解释「同址跳过」，容易让人以为重复 store 是无害的幂等操作。
- 证据或推理：E1。`sed -n '1574,1583p' os/kernel/src/lib.rs` 显示 assert 位于 `unsafe {` 之前；引文 2755–2835 没有它，也没有省略标记。
- 最小修复：引文补一行 `assert!(...);`（或加「一次性守卫 assert 省略」说明），并在正文第 2855 行说明「第二次 store 现为 panic，不是 no-op」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能引文头「相邻接续行」被作者用来涵盖前置守卫；但 assert 是独立语句，且正文的「同址跳过」叙述依赖它是否被展示。
- 对应判据：任务项 5（引文块节选标注）；文档-代码同步七项之「内部 TODO/行为合同」。

## FINDING FACT-B-11（P2-fact）
- 位置：文档 §5.2.1 第 3250 行；源码 `os/boot-shim/src/opensbi_helpers.rs:806–840`
- 严重度：P2
- 问题：表格称 `test_build_kernel_info_riscv64_fields_match_input`「字段逐一映射（含派生字段 `kern_stack_top`/`syscall_entry`）」。测试确实存在，但只断言 `kern_stack_top()` 访问器；`syscall_entry` 已不是 `KernelInfo` 字段，测试也没有对它做任何断言（源码注释只说真值来自 `syscall_entry_va()`）。
- 影响：读者会以为 strict/字段映射测试仍在保护 `syscall_entry` 的交接一致性；该合同已被 D-062 删除。
- 证据或推理：E1。测试体 `:806–840` 的断言只覆盖 memmap、kern_virt_base/phys_base/size、`kern_stack_top()`、boot_modules、platform_sources；无 `syscall_entry` 断言。
- 最小修复：该格改为「字段逐一映射（`kern_stack_top` 由访问器派生；`user_sp`/`syscall_entry` 已不上线）」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `syscall_entry` 在测试模块的其他测试里被断言；`grep -n 'syscall_entry' os/boot-shim/src/opensbi_helpers.rs` 需复核（本测试体内无）。
- 对应判据：任务项 3（测试名对账/契约描述）；P2-fact。

## FINDING FACT-B-12（P0-fact）
- 位置：文档 §4.3（第 2530–2582 行的 `load_boot_modules_into` 引文、第 2586–2588 行的事实清单、第 2593–2628 行的 `FileLoader` 引文与〔编注〕）；源码 `os/boot-shim/src/loader.rs:485–544`、`os/boot-shim/src/opensbi_helpers.rs:198–215`
- 严重度：P0
- 问题：D-082（I-36）把 ELF 尾部修剪从 `UbootFileLoader::read` 移到 `load_boot_modules_into`，文档 §4.3 两段代码引用与四条事实清单全都没跟上：
  1. `load_boot_modules_into` 引文在 `let data = data.as_slice();` 之后直接跳到 `num_pages`，**略去了**现行源码的两行可执行逻辑
     `let used = elf_file_end(data).unwrap_or(data.len()); let data = &data[..used];`；
  2. 事实清单第 3 条仍写「模块镜像按文件字节数拷贝」、第 4 条仍写「`BootModule.len` 记录的是**文件字节数**，不是页数」——现行 `len` 是 `elf_file_end` 修剪后的运行期文件长度（最后一个 PT_LOAD 的 `offset+filesz`），不是整份文件字节数；
  3. `UbootFileLoader::read` 引文仍展示已删除的 `segment_iter` 修剪段与 `FileBytes::Borrowed(&slice[..used])`，现行实现直接 `FileBytes::Borrowed(slice)`；
  4. 紧跟引文的〔编注〕说「源码同处的英文注释……该注释待同步」——它挂在已删除的 `read` 修剪段上，「源码同处」已不存在；该英文注释的真实位置是生产者 `uboot_main.rs:286`（`staged_elf_used_len` 的文档注释），那里确实仍留着「the table stores the slot capacity」的旧说法，但同一注释的下文已解释「解析 staged ELF 头可得到真实文件尾」，且 `build_shim_table` 写进表的是 `used`（`uboot_main.rs:391/417`）。因此〔编注〕的结论（注释与实现不符）仍有对象，但位置指错，且它服务的引文段已整体过时。
- 影响：读者会以为模块装载拷贝整份文件、`BootModule.len` 等于盘上文件长度；按此计算模块占用区间会偏大，也会误解 U-Boot 腿与 UEFI 腿的 `len` 是否同口径（D-082 的目的正是统一跨腿长度）。第 4 点的〔编注〕会让读者在 `opensbi_helpers::read` 里找一条已经不在那里的源码注释，而真正待同步的 `uboot_main.rs:286` 注释没有被点名。
- 证据或推理：E1。`sed -n '485,544p' os/boot-shim/src/loader.rs` 显示 `elf_file_end` 两行与 `len: data.len()`；`sed -n '198,215p' os/boot-shim/src/opensbi_helpers.rs` 显示 `Some(FileBytes::Borrowed(slice))`，且注释逐字写「D-082（I-36）……再跑 `segment_iter` 是对已修剪数据的幂等无效功」；`sed -n '284,295p' os/boot-shim/src/uboot_main.rs` 显示旧注释与其下文自相矛盾，`sed -n '388,394p'` 与 `:414,420` 显示表里写的是 `used`。doc-citation-check 对 2593–2628 标 WARN；2530–2582 未被工具识别出漏行。
- 最小修复：三处同步：① 引文补 `elf_file_end` 两行；② 第 3/4 条改为「按 ELF 运行期文件长度（最后 PT_LOAD 文件尾）拷贝；`BootModule.len` 是该修剪后的字节数」；③ 重写 `FileLoader` 引文为现行 `read`（无 `segment_iter`），把〔编注〕移到生产者 `uboot_main.rs:286` 并点名该处旧注释（或直接建议删掉「table stores the slot capacity」一句）。修后重跑 `citation-check` 与 `anchor-resolve`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `elf_file_end` 对生产模块恒等于 `data.len()`（例如模块无 `.comment/.symtab`）；但源码注释明说 36 个模块 ELF 的末尾有非 NOBITS 的 `.comment/.symtab/.debug_*` 会被裁掉，且这正是 D-082 的动机，故不成立。
- 对应判据：任务项 5（引文逐字/节选标注）、任务项 4（行为合同）；文档-代码同步七项之「符号锚点/常量」；P0-fact。

## FINDING FACT-B-13（P1-fact）
- 位置：文档 §6.3 第 3454 行；源码 `os/kernel/src/lib.rs:1329–1530`
- 严重度：P1
- 问题：失败模式表写「串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根的后半段：Step 1/2 映射已过，但 **Step 2b/2c/3 与跳转尚未完成**）」。源码中 `boot_stage!("kernel: step1+2 mappings ok\n")` 位于 Step 2b 与 Step 2c **之后**、Step 3 `enable()` **之前**；看到这一行意味着 2b/2c 已经完成，尚未完成的只有 Step 3 与跳转。
- 影响：这是诊断章给出的定位规则：按此表，读者会把「2b/2c 已完成」误判成「2b/2c 未完成」，在错误阶段找问题；同篇 §6.1（第 3388 行）对同一路标的解释是正确的，两处自相矛盾。
- 证据或推理：E1。`sed -n '1329,1530p' os/kernel/src/lib.rs` 的顺序是 Step 2b 块 → Step 2c 块 → `boot_stage!("kernel: step1+2 mappings ok\n")` → Step 3 `paging.enable()`。§6.1 第 3388 行只写「Step 1/2 已过（建根后半、跳转前）」。
- 最小修复：第 3454 行括注改为「Step 1/2 映射已过、Step 2b/2c 也已完成，卡在 Step 3 `enable` 或跳转」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 aarch64 的 2b 在 `enable` 之后？源码 2b 块在 `boot_stage` 之前，不成立。
- 对应判据：任务项 4（行为合同/诊断信号）；P1-fact。

## FINDING FACT-B-14（P2-fact）
- 位置：文档 §4.8 表第 3094 行（aarch64 直核行）；源码 `os/kernel-image/src/main.rs:407–413`
- 严重度：P2
- 问题：表行写 aarch64 直核面「无入参（`x0=0` 分道，DTB 从 RAM 基址固定位置读）」。现行直核产物是独立构建形态（`fw-aarch64-none`），入口 `rust_image_main(_arena_pa: usize)` 直接忽略入参、不做任何 `x0 == 0` 分道；`x0=0` 分道已被 D-061 删除，同节第 3049 行也明确写「不再被解释成该走直核自举——直核形态是另一个构建产物，判据在编译期就定了」。
- 影响：读者会把 D-061 已删除的运行期判别当成现行机制，在直核启动路径上寻找一个不存在的 `x0` 分支。
- 证据或推理：E1。`sed -n '407,413p' os/kernel-image/src/main.rs` 显示直核臂 `fn rust_image_main(_arena_pa: usize) -> ! { bootface_a64::bootface_a64() }`，无 `if arena_pa == 0`；UEFI 接收臂才有该判别。
- 最小修复：表行改为「无入参（直核产物在编译期选定，`arena_pa` 未使用），DTB 从 `DTB_PA = DRAM_BASE` 读」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能旧固件在 `x0` 上留下 0，但源码不读它，分道仍不存在。
- 对应判据：任务项 4（行为合同）；文档-代码同步七项之「符号锚点」。

## FINDING FACT-B-15（P2-fact）
- 位置：文档 §4.3 第 2586 行
- 严重度：P2
- 问题：向上取整的说明把「少一页」的条件写反了：「写小了（加数 A ≤ 4094）时，只有余数 r 满足 `r > 4096 - A` 才会少一页」。设 `n = q·4096 + r`（1 ≤ r ≤ 4095），`(n+A)/4096 = q + (r+A)/4096`，少一页（仍为 q）的条件是 `r + A < 4096`，即 `r < 4096 - A`；`r > 4096 - A` 恰好是会进位、**不会**少页的那一侧。文档给的反例 `r=4095, A=4094` 仍落正确侧（4095+4094 ≥ 4096），所以例子没暴露这个反向。
- 影响：这是「为什么写 4095 而不是 4096」的唯一推导；条件写反后，读者复算会在边界值（如 r = 4096-A）得到相反结论，从而无法验证「4095 是最小加数」这一结论。
- 证据或推理：E1（算术）。取 A=4094、r=2：`(n+4094)/4096 = q + 4096/4096 = q+1`，正确进位，不是少一页；按文档的不等式 `2 > 2` 为假，文档会说「不会少一页」，与实际一致但理由错——再看 r=1：`1 > 2` 假，文档说「不会少」，实际 `(1+4094)<4096` 会少一页。故不等式方向反了。
- 最小修复：把 `r > 4096 - A` 改为 `r < 4096 - A`（或等价的 `r ≤ 4095 - A`），并把反例补上边界 `r = 4096 - A`。
- 置信度：高
- 是否需源码核实：否（纯算术；源码 `(data.len()+4095)/4096` 已核实）
- 若我错了：除非作者把「少一页」定义为「比正确结果多一页」，但上下文明显相反。
- 对应判据：任务项 4（常量/行为合同的可复算性）；P2-fact。

## FINDING FACT-B-16（P1-fact）
- 位置：文档 §3.4.5 第 1764 行；源码 `os/libs/minix-boot/src/boot_storage.rs:140–260`
- 严重度：P1
- 问题：该段仍按已删除的 API 描述直核自举面的 `BootStorage`：「模块、平台源、参数三类走 `set_*_len` 提交，startup 走 `push_startup` 逐条追加」「`BootStorage` 只承载 modules、platform、param、startup 四类；`memmap`、`reserved` 与 source 记录仍由 DTB 收集器的进程级静态量持有……`BootStorage` 的 memmap/reserved/source 槽在直核腿留空」。现行 `BootStorage` 的公开写入口是五个 `fill_*` 闭包（`fill_map_tables`、`fill_modules`、`fill_platform`、`fill_param`、`fill_memmap_sources`），没有 `set_*_len`/`push_startup`；`memmap`、`reserved`、source 记录都有 storage 槽，且 `prepare_boot` 与两个 `bootface` 都通过 `fill_map_tables`/`fill_memmap_sources` 填充它们。
- 影响：这段是本篇对「直核腿所有权」的权威说明；按它读代码会在 `BootStorage` 上找不到四个方法，并把「两份所有者」的现状理解反了（现行直核腿的 memmap/reserved/source 已在 storage 内，不再只靠 DTB 收集器静态量）。它与同节第 1762 行刚描述的 `fill_*` 五填充点直接矛盾。
- 证据或推理：E1。`grep -n 'pub fn ' os/libs/minix-boot/src/boot_storage.rs` 只列 `fill_*`、访问器与 `report`；`grep -rn 'set_modules_len\|push_startup' --include=*.rs os/` 零命中；`os/kernel-image/src/bootface_a64.rs:348` 与 `os/boot-shim/src/opensbi_helpers.rs:308` 都调用 `fill_map_tables`。
- 最小修复：重写第 1764 行为现行 API：四类载荷 + memmap/reserved/source 全走 `fill_*`；删除「startup 走 push_startup」「memmap/reserved/source 槽留空」与「两个所有者」的旧结论，或改写为「D-062/D-013 之后已收进 storage」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能有另一套 `set_*_len` 名字在 `BootStorageCell` 上；`grep` 全树零命中，且 `fill_*` 是唯一写入口。
- 对应判据：文档-代码同步七项之「符号锚点」；任务项 4；P1-fact（位置在 §3，属 P-FACT-A 的章节，若 A 已报同一段可合并为一条）。

## FINDING FACT-B-17（P1-fact）
- 位置：文档 §5.2.3 第 3291、3293 行；源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs:77–145`
- 严重度：P1
- 问题：§5.2.3 自称是深拷不变量「三层判别」的权威记录，却把探针的内存图来源写错：文档说 `in_snapshot_free` 读的是「该测试内核自己 `build_memmap()` 产出的 conventional-only 投影（在探针分配之前取得，且它没有生产侧的 EBS 配对/分类循环）」，并把样本限定为「快照后分配」。实际 `test-kernel-map` **不调用** `build_memmap()`；它在探针分配**之后**调用 `exit_boot_services_and_classify()`，用 `ExitBootServices` 返回的最终内存图（`MemoryMapOwned`）分类，`in_snapshot_free` 查的是这份最终图。
- 影响：三层判别的第二层论证（`in_snapshot_free=0` 是「探针页不在 conventional 投影里」的构造结果）结论仍成立，但依据的机制描述不对：读者若按文档去读 `build_memmap` 或假定投影在探针前取得，会无法复现该读数，也无法判断探针到底排除了什么。该节是「权威记录」，机制错写会污染后续引用（§4.5、§5.1 M4、§6.3 都只引用不重述）。
- 证据或推理：E1。`sed -n '77,145p' .../test-kernel-map/src/main.rs`：探针 `allocate_pages` 在 ~78 行，`let (memmap, _reserved_regions) = uefi_helpers::exit_boot_services_and_classify();` 在 ~89 行；`grep -n 'build_memmap' .../test-kernel-map/src/main.rs` 零命中。`os/boot-shim/src/uefi_helpers.rs:1303–1324` 的 `exit_boot_services_and_classify` 用 `boot::exit_boot_services` 的返回图分类。
- 最小修复：第 3291 行「加一枚对照页——内存快照之后……」改为「在最终 EBS 内存图之前分配」；第 3293 行把「`build_memmap()` 产出的 conventional-only 投影（在探针分配之前取得）」改为「`exit_boot_services_and_classify()` 对 `ExitBootServices` 最终图分类的结果（探针分配之后取得）」；第 3293 行「快照后分配」同步改为「最终图之前分配」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能 `exit_boot_services_and_classify` 内部先调 `build_memmap`；源码 `:1303–1324` 没有，直接 exit + classify。
- 对应判据：任务项 4（行为合同）、任务项 7（反事实/证据前提）；P1-fact。

## FINDING FACT-B-18（P2-fact）
- 位置：文档 §5.1 M13 第 3162 行、§5.2.1 第 3256 行
- 严重度：P2
- 问题：两处都称 `build_memmaps` 的「单表包装 `build_memmap()` 被 15 个 QEMU 测试内核调用」。实测：只有 **2 个**测试内核有 `build_memmap()` 的可执行调用（`test-memmap`、`test-memmap-aarch64`），另 1 个只在注释里提到；「15」是 `build_memmap`/`build_memmaps`/`exit_boot_services_and_classify` 三者的**并集**内核数（12 个用 `exit_boot_services_and_classify`）。句子把并集计数安到了单表包装上。
- 影响：数字本身有出处，但归属错：读者会以为 15 个测试内核直接调用了 `build_memmap()`，从而高估这条单表路径的覆盖面。覆盖结论（UEFI 内存图 helper 在真机测试内核里被广泛调用）仍成立，但按字面核对会对不上。
- 证据或推理：E1。`grep -rl 'build_memmap()' os/qemu-tests/test-kernels | wc -l` → 3（含 1 处注释；`grep -rn` 确认可执行调用仅 `test-memmap-aarch64:29`、`test-memmap:165`）；`grep -rl 'exit_boot_services_and_classify' ... | wc -l` → 12；`grep -rlE 'build_memmaps?|exit_boot_services_and_classify' ... | wc -l` → 15。
- 最小修复：把「单表包装 `build_memmap()` 被 15 个 QEMU 测试内核调用」改为「UEFI 内存图 helper（`build_memmap()` 与 `exit_boot_services_and_classify()`）合计被 15 个 QEMU 测试内核使用，其中 `build_memmap()` 直接调用 2 处」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若作者所谓「调用」包含经 helper 间接触达，则 15 成立；但 `build_memmap()` 不是 `exit_boot_services_and_classify()` 的调用链成员（后者走 `classify_map_bytes_into_test_tables`），间接也到不了它。
- 对应判据：任务项 3（测试计数/覆盖对账）；P2-fact。

## FINDING FACT-B-19（P2-fact）
- 位置：文档 §4.3 第 2589 行
- 严重度：P2
- 问题：反事实「如果改成静默跳过，错误会推迟到内核侧的数量断言，那里只有'期望 12、实际 N'，没有缺失的名字」前提不成立：内核侧没有这条数量断言。`KernelInfo::validate` 只断言 `boot_modules` 非空；`BOOT_MODULE_PROC_NRS[i]` 的按位置绑定在模块数少于 12 时只是少初始化几个进程，没有「期望 12、实际 N」的消息。
- 影响：读者会以为存在一条内核侧数量断言作为最后防线，从而低估「静默跳过」的后果；反事实的结论（错误变得不可归因）仍成立，但依据的失败点描述错。
- 证据或推理：E1。`grep -rn 'boot_modules must not be empty' os/libs/minix-boot/src/kernel_info.rs`；`grep -rn 'expected 12\|expects 12\|12 boot modules' --include=*.rs os/kernel/src` 零命中数量断言；`os/kernel/src/lib.rs:2828` 的循环按 `boot_modules.iter().enumerate()` 走，缺项不触发断言。
- 最小修复：改为「错误会推迟到内核侧：`validate` 只查非空，按位置绑定会静默少初始化进程；现场没有缺失的名字」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：可能有另一条 `NR_BOOT_PROCS` 长度断言在别处；全内核 `grep` 未找到「期望 12」类消息。
- 对应判据：任务项 7（反事实前提）；P2-fact。

## SUGGESTION

- **§6.4 第 3503 行的「U-Boot shim」应改为「U-Boot 接收镜像」**（P3，非事实阻断）：该句列的是「三条 v1 接收腿」，而 U-Boot shim 是发送侧可执行体（`riscv64-uboot-shim`），真正调用 `resume_additive` 的是 `fw-riscv64-uboot` 构建的接收镜像。读者按字面去 shim 里找 `resume_additive` 会落空。
- **§6.3 第 3456 行「shim 从 `LoadedImage.info()` 取自己的 image/stack」**（P3）：image 取自 `LoadedImage.info()`，stack 取自当前 SP（`shim_stack_window`）；同篇 §4.6 第 2916 行写对了，建议对齐。
- **§4.4 第 2693 行 `boot_validate_and_prepare` 引文**（P3）：引文头列了「尺寸阶梯与诊断路标宏」的省略，但未提 `kern_stack_top` 16 字节对齐断言也被略去；建议在引文头补一句，避免逐字引用时被当成完整校验序。
- **§6.1 第 3377 行「v1 接收腿打的是 `kernel-image: step4 …` 那一行」**（P3）：同表第 3374/3379 行已写明该两行只有 aarch64 接收腿打印，x86_64/riscv64 传空串不打印；第 3377 行的「v1 接收腿」应限定为「aarch64 v1 接收腿」，否则与前两行自相矛盾。

## 检查覆盖与阴性声明

- **锚点**：跑 `tools/anchor-resolve.sh`，扫全文 108 个符号锚点 → `resolved=108 zero-def=0 multi-def=0`。N=0 未解析锚点。不覆盖：解析到同名但语义错误的符号（工具只查存在性；本报告用源码阅读补了 §4–§6 的关键符号，未逐符号复核 §2 的 C 锚点）。
- **路径存在性**：跑 `tools/doc-code-map.sh`，扫 54 个声明的文件 → 54/54 存在。N=0 缺失。不覆盖：路径是否正确实现文档所述职责（另由逐节源码核对补）。
- **结构/内部引用/文风/引文**：`doc-structure-check.py` exit 0；`doc-ref-check.sh --check` exit 0；`doc-style-lint.sh` error=0；`doc-citation-check.py` blocks=69 checked=29 failed=0 warned=5。5 个 WARN 块全部手工查看：§3.8（2106–2173）未发现新事实错误（属 P-FACT-A 范围，未展开）；§4.2（2313–2402）→ FACT-B-02；§4.3（2593–2628）→ FACT-B-12；§4.6（2939–2962）的 asm 重排已在引文头披露，未报。
- **测试名对账**：把文档中出现的 19 个具名测试 + 4 组 `*` 通配族逐个 grep：`test_postconditions_reject_*` 0 条（FACT-B-07）；`test_store_kernel_info_rejects_startup_overflow` 0 条（FACT-B-08）；其余具名测试全部存在（含 `boot_windows_*`、`missing_boot_allocator_registration_fails_closed`、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`、`picks_the_preferred_size_when_supported_and_aligned`、`classified_tables_are_disjoint_and_expected`、`test_device_flags_set_pwt_and_pcd`）。N=0 的其余名字不覆盖「测试体是否真的断言了文档所说的契约」（只对 FACT-B-11 做了测试体阅读）。
- **测试计数**：用工具自己的口径（`grep -c '#[test]'`）复算 21 个文件，并用 `cargo test -- --list` 权威复核三个包：boot-shim 146 / minix-boot 184 / minix-elf 27；文档转录为 136 / 195 / 23。6 个逐文件数字不符，已并入 FACT-B-06。N=0 不覆盖：文档未列出的包/测试二进制（如 `minix-kernel` 集成测试目录）。
- **常量/签名/行为合同**：逐项核对 §4.2.2 的 `KNOWN`（16 常量/31 位，Python 复算一致）、`LAST_KNOWN=15`、`AMBIGUOUS`/`PROFILE_ONLY`/`TYPE_INDEPENDENT` 位集、`checked_shape`/`checked_len` 条件、§4.2 的 `BOOT_PT_BUMP_PAGES=1024`、`ARENA_CAP=32768`、`WIRE_SOURCES_END=29184`、§4.5 的 8 个容量常量、§4.6 的 `MAX_BOOT_WINDOWS=32`/`DEV_PIECE`/`BOOT_IDENTITY_MAP_END`、§4.7 的 `VM_BOOT_HANDOFF_MAX_*`。发现：`BOOT_HANDOFF_V1_VERSION` 9≠10（01）、`commit_map_tables`/`map_tables_mut` 不存在（02）、`STARTUP_RECORD_STORE*` 不存在（03）、`check_boot_postconditions` 只剩 1 条断言（05）、向上取整不等式反向（15）。N=0 不覆盖：C 侧常量（§2 范围，属 A）。
- **引文逐字**：手工比对 §4.2（`prepare_and_enter_kernel_image`）、§4.3（`load_boot_modules_into`、`entry_path_eq` + `UbootFileLoader::read`）、§4.5（`land_slice` + `store_kernel_info`）、§4.6（Step 1/2、`enable`）、§6.2（panic handler）六个块。发现 2 个块有未披露的可执行语句省略/替换（02、12）、1 个块省略一次性守卫未标注（10）。N=0 不覆盖：§2 的 C 引文块（属 A）。
- **反事实前提**：读了 §4.1（挪 register）、§4.3（换 tty/mib、静默跳过缺件）、§4.5（半深拷）、§4.6（恒等映射只覆盖可用内存、virt==phys 照做 Step 2、去掉 L5）、§4.8（riscv 照搬 x86 形态）、§6.2（panic 里 format!）、§6.3（降级 debug_assert）共 10 处。发现 2 处前提/机制与源码不符（17、19）；其余前提与源码一致。N=0 不覆盖：反事实的因果推演是否穷尽（只核对前提）。
- **阴性声明**：`grep -rn 'startup_objects|STARTUP_RECORD_STORE|WIRE_STARTUP_CAP|commit_map_tables|map_tables_mut|set_*_len|push_startup'` 在 `os/` 的结果为 0（除注释）；本报告据此报 01/02/03/16。这些 grep 的 N=0 只说明当前树无该符号，不覆盖「代码注释里仍写着与实现不符的旧说法」；后者只在 FACT-B-12 的〔编注〕处做了核对。
- **未覆盖**：§5.2.3 的探针读数（`pa=0x5e9b000`、`0x4a00_0000_0001_ff00`）需要 QEMU/OVMF 实跑，本次只读环境未复跑，按「未复核读数」处理，不据此报事实错误；§2 的 C 源逐行计数（pre_init 八条语句等）属 A 的范围，未在 B 的预算内重做。

## 完成声明

本轮 P-FACT-B 只读审查完成：写入范围仅本报告文件，未修改任何源码、文档、轨迹文件。
- FINDING：19 条（P0=7，P1=5，P2=7）。
- SUGGESTION：4 条（P3 级措辞/指代问题，不混入 FINDING）。
- 机械门：anchor 108/108、code-map 54/54、structure exit 0、ref-check exit 0、style error=0、citation failed=0（warned=5 已手工分诊）。
- 未展开根因：无（19 < 40）。


