# D01 meta-review —— 01-boot-shim-bootstrap

- 评估对象：V1 review-loop 首跑，`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/`
- 工作流版本：V1（`WORKFLOW-VERSIONS.md`，2026-11-07）
- 基线：`31f3c1829`；文档基线 sha256 `9f6a2f39…f655ac`，3086 行；当前工作树文档 3125 行，sha256 `a457625b…b052b2`
- 输入：`loop.md`、`metrics.md`、`defect-ledger.md`、`follow-ups.md`、7 份面板原始输出、`git diff --cached`、`DECISIONS.md`、`BLIND-SPOTS.md`、`CORPUS.md` 的 D01 条目
- 方法：只读复核；抽样 24/85 条（28.2%）独立复判处置；重放计数与轨迹检查命令；不运行完整测试，不运行 QEMU

## 一、总体结论

**IMPROVED（基线已建立），但本轮运行质量 NOT PASS。**

分开说两件事，避免用“工作流进步”掩盖“本轮交付不合格”。

1. **工作流能力相对 V1 之前是 IMPROVED。** 面板第一次把 D01 的设计死代码、读者理解障碍、跨节矛盾和交接面过期同时摆上桌面：FACT-01..08 这一组 P0 事实（`FREE_MEMMAP`、`classify_map`、四页 arena、`KernelInfo` 14→16 字段、`BootHandoffV1` 形状、`bootstrap_alloc_limit` 反向、`pt_alloc` 引文、注册点）不是命名与注释级意见；DESIGN-01..15 里有 10 条是可删除的抽象、C 形状状态或非法状态收敛；READER-A-03、READER-B-10、READER-C-03/C-05/C-13 是真正的理解与因果问题。这些在历史 DOC-ITER 轮次与同 agent 自审里没有稳定出现。
2. **本轮运行本身 NOT PASS。** 抽样一致性按“处置真实成立”读为 21/24 = 87.5%，低于 90%；更严重的是 DESIGN-10 的修复引入了一个未被 L4 发现的 QEMU 回归，FACT-09 的测试计数修复仍然是错的，DESIGN-15 只修了文档侧、代码注释仍然过期。轨迹检查器的“exit 0”也不是按技能文档命令复现出来的。按任务约定，抽样一致性低于 90% 必须写 `NOT PASS`，所以本轮结论只能是“基线建立 + 交付不合格”。

这不是“文档还差几个小问题”。这是“工作流已经能发现问题，但还不能保证修复被正确验证”的典型首跑状态。

## 二、固定问卷

### 1. Detection：PASS

以前漏的现在能发现，而且不是靠同一个脑子复述。

- **事实层**：FACT-01 找到 `FREE_MEMMAP` 已不存在、Phase A.2 已显式删除；FACT-02 找到 `classify_map` 不存在、现行是 `classify`/`adjudicate` 五步；FACT-03 找到 arena 4→9 页；FACT-04 找到 `KernelInfo` 14→16 字段；FACT-05 找到 `BootHandoffV1` 的 `reserved0`→`ram_authority` 与 5→7 分区；FACT-06 找到 `bootstrap_alloc_limit` 无参且语义反向；FACT-07/08 找到 `pt_alloc::register` 引文缺 release 守门与 shim 侧根本没有 `register` 调用；FACT-09 找到测试计数漂移。证据见 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/fact.md` 对应条目与 `git diff --cached` 的文档改动。
- **设计层**：DESIGN-01（`bootstrap_start/len` 与 `free_upper_idx` 的 C 形状）、DESIGN-02（`BootShim` 无内核消费者、无 mock、静态方法不可 `dyn`）、DESIGN-03（`PagingWithId` 仅 mock、两个死常量）、DESIGN-04（`BootForm` 生产零引用、battery 不随形态变化）、DESIGN-05（`static mut` 返回 `&'static` 的别名承诺）、DESIGN-06（`store_kernel_info` 只深拷 5/7）、DESIGN-08（memmap 空洞数组与 `KERNEL_MAY_ALLOC`）、DESIGN-11（GET_KINFO 载荷收窄）都是结构性问题，不是“改个名字”。
- **教学层**：READER-A-03 独立推出 `mb_set_param` 的删除扫描会跳过下一条记录首字符；READER-B-10 指出深拷探针用“排除两个候选”推出第三个未检验解释；READER-C-03 找到交接参数在 §3.1 与 §3.4/§3.7 之间自相矛盾；READER-C-05 指出一次 QEMU/OVMF 单页探针被用来排除真机历史读数。这些是冷读独有发现。
- **仍漏的盲区**：B-05（假反事实）没有被单独检验；B-07（architecture port 误译“端口”）没有出现；B-13（硬件机制泄漏到共享语义层）只被部分触及（`PTE_HUGE_IDENTIFIER_BIT` 作为死常量被报，但没有按“合同里出现硬件位”定性）；B-09（门绿但事错）在本轮以新形态出现，见第 8 项。

### 2. Precision：CONCERN

“误报 0”不是测量结果，是分类口径的产物。

- `metrics.md` 写 `③ 0 / ④ 0`，`loop.md §3` 自己承认“reader B/C 的转出项可能含少量措辞偏好”。也就是说，所有非阻塞项都被放进 ②，没有一条被独立判成 ③ 或 ④；精度因此没有被测量。
- 按内容抽样看，真正的误报率不高，估计 5–9%：READER-B-11（表后段落重复表）、READER-B-12（§3.6 第二个可核对点偏题）、READER-B-13（同一状态事实四五处重述）更接近“可以更紧”的结构偏好；READER-C-11（字段账目分类）有解释空间。它们都有文档内依据，所以不是凭空误报，但也不该全部算“真缺陷”。
- **角色模式**：FACT 与 DESIGN 的精度最高，几乎每条都能落到源码或文档原句；READER-B 量最大（20 条），也是风格/结构偏好最集中的角色；READER-C 的 17 条里 C-05/C-12 是低置信高价值，C-13 是真实的失败路径问题。若下一轮要测精度，优先抽 READER-B 的 ② 项。
- 证据指针：`metrics.md:4-5`、`loop.md:43`、`defect-ledger.md` 的 READER-B-11/12/13 行、`panel/reader-b.md:236-284`。

### 3. Design quality：PASS

不是只有命名与注释级意见；设计角色确实找到了可删除的抽象与错误建模。

- 可删除的抽象：`BootShim`（无内核消费者、无 mock、静态方法不可 `dyn`）、`PagingWithId`（唯一实现是 `MockPaging`）、`BootForm`（生产零引用，battery 对五个形态构造同一份 `KernelInfo`）、`PTE_HUGE_IDENTIFIER_BIT`/`HUGE_PAGE_SHIFT`（全仓零读取）。
- 翻译残留与 C 形状：`bootstrap_start/len` 用两个整数编码一个布尔事实，生产调用全部传 `(0,0)`；`free_upper_idx` 的 `Option` 在唯一报告点被 `unwrap_or(0)` 抹平；`KERNEL_MAY_ALLOC` 只剩一个 debug 断言；`memmap.rs` 的空洞数组、高水位、零长=空仍是 C 的容器形状。
- 错误建模与非法状态：`MemoryRegion` 允许零长/未对齐；`Serializer::new` 不接收 authority；`BootPrepareResult` 的 `root_page`/bump 无构造校验；`uninit_alloc` 把“未注册”折成“内存不足”；`wire_module_to_boot_module` 用 `unwrap_or(0)` 把越界长度变成零长模块。
- 设计角色还做了反向判断，拒绝了 D1 的过度删除：`FileLoader` 有两个生产实现 + mock + `&dyn`，保留；`PageSize`/`HugePages` 核心、`BootArch`、`RawOps`、`dtb_copy::Phase` 保留；D1 的“零 trait”“只铺 reported_top”“typed Params”“checksum”等被逐条拒绝。这比“为了更 Rust 而删”更有价值。
- 证据指针：`panel/design-d2.md:26-100`（翻译残留与死抽象）、`:102-288`（15 条 FINDING）、`:291-346`（VERDICT 与不采纳 d1 的提案）。

### 4. Independence：PASS（D1 降级）

冷读确实产生独立发现，不是复述主 agent 已知内容。

- READER-A/B/C 只拿到文档与读者画像，未读源码、其他面板输出、diff、历史评审。它们独立收敛到同一批问题（A-08 与 C-01 都指向 `read_required` 不是唯一落点；A-09 与 C-02 都指向 `register` 引文缺 `if already`；A-10 与 FACT-16 都指向“四条 vs 五条”），这是独立复现，不是抄写。
- 冷读独有发现至少有：A-03（`mb_set_param` 删除扫描跳首字符）、A-05（`AllocatePages`/`allocate_pool`/`Vec`/`Box` 三套命名）、A-15（`kern_stack_top = base + size` 与 L7 断言不等价）、B-10（探针结论强于证据）、B-16/B-17（三/五/四套计数与三表互不认）、C-03（交接参数矛盾）、C-05（单形态探针外推真机）、C-13（`map_huge` 忽略错误的理由与失败类别不匹配）、C-14（“整根换掉”与“不得重建根”矛盾）、C-17（`pt_free`/`alloc_mem` 在 Ch2 无出处）。
- 唯一降级项是 P-DESIGN D1：它自述读了 Ch3/Ch4 标题、`kernel/const.h`、`servers/vm/utility.c`、`servers/vm/main.c` 调用点。它没有读现有 Rust 实现，也没有读 Ch3/Ch4 正文；结论层面可接受，但独立性不能再记为“完全材料隔离”。详见第四节。
- 证据指针：`panel/reader-a.md:3-13`、`panel/reader-b.md:3-7`、`panel/reader-c.md:3-5`、`panel/design-d1.md:6-17`。

### 5. Scope discipline：CONCERN

单条发现的范围门执行得不错，聚合范围失控。

- `re-litigation` 0：D-001（四章骨架）没有被重提，READER-B 明确写“未提四章骨架重排”；D-011（主张表）也没有被当成新问题。`adjacent 阻塞` 0：所有 ② 项都进了 `follow-ups.md`，没有写成当前阻塞。
- 但 `follow-ups.md` 有 18 条（D01-F01..F18），远高于 `review-evals.md` 收敛标准第 4 条的“每篇新增不超过 3 条”。这不是说 18 条都该阻塞；而是说本轮把 48/85 条发现整体转出，下一轮要重新面对这 48 条。按“范围先声明”的字面，主 agent 没有越界；按“收敛”的实质，范围在聚合层已经扩散。
- 更具体的范围漏洞：`loop.md §0` 把范围定义为“44 个关联代码 + 本轮修复触碰的文件”，但 `arch_boot` 的调用方 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half*/src/main.rs` 不在 44 个文件里。DESIGN-10 的修复改的正是这个公共入口，结果打断了这三个 QEMU 测试内核。范围声明没有包含“被修改函数的调用方”，这是范围模型的缺口，不是执行者的疏忽。详见第 9 项。
- 证据指针：`loop.md:15`、`loop.md:41-42`、`follow-ups.md` 全表、`metrics.md:6-7`。

### 6. Teaching quality：CONCERN

读者角色发现了真实理解障碍，但采纳率低于收敛标准。

- 真实障碍的例子：A-03 的 `param_buf` 单 NUL/双 NUL 推导会让读者按文档写出错误的手算；A-05 的 `AllocatePages`/`allocate_pool`/`Vec`/`Box` 三套命名直接影响 EBS 边界理解；A-09/C-02 的 `register` 引文让读者以为 release 下会覆盖；C-03 的交接参数让读者做出一个按值传 `KernelInfo` 的跨镜像 ABI；C-05/B-10 的探针结论会让读者把“排除两个候选”当成“第三个原因已被证明”。这些不是文风偏好。
- 采纳率：读者三类共 53 条（A 16 + B 20 + C 17），本轮修掉 15 条（A 10 + C 5，B 0），15/53 = 28.3%；`review-evals.md` 收敛标准第 6 条要求“采纳率至少 50%”。如果把“转出到 follow-ups”也算“采纳”，则 53/53 有去处；但“采纳率”若指修掉，本轮不达标。工作流没有区分这两个口径，这是指标定义的缺口。
- 纯文风噪声：不多，估计 3–5 条（B-11/B-12/B-13 一类），低于 20% 的噪声上限。主要问题不是噪声，而是“发现了但没修”的教学债规模。
- 证据指针：`defect-ledger.md` 的 READER-A/B/C 行、`follow-ups.md` 的 D01-F12..F18、`metrics.md:13-14`、`review-evals.md:215-220`（收敛标准第 6 条）。

### 7. Cost：CONCERN

预算内，但收益在尾部递减，且 token 没有采集。

- 成本读数：7 个面板 agent，面板约 32 分钟，修复约 2 小时；token 写“未采集”。`CORPUS.md` 的单篇预算为“面板 5–8 个子 agent；主 agent 修复轮最多 2 轮；meta-review 1 个”，7 个在预算内，修复轮数也没有超。
- 收益分布：前 20 条左右（8 条 FACT P0 + 设计死代码/深拷/校验）价值最高；后 20 条（READER-B 的结构/重复/指针）大多是教学债，且 20 条里 0 条本轮修。按“边际发现的价值”看，尾部已经在递减。
- 下一轮成本会更高：48 条转出项 + 18 条 follow-up 要在下一篇闭环轮重新取证；如果不在下一篇之前做一次优先级裁剪，下一轮的面板成本会超过本轮，而新增价值未必更高。
- 证据指针：`metrics.md:8`、`loop.md:102`、`CORPUS.md:69-73`。

### 8. Trace integrity：FAIL

“每条 FINDING 都有处置行”这一条成立；但“指标自洽、复验可信”不成立。

- **成立的**：85 条 `## FINDING` 与 `defect-ledger.md` 的 85 行一一对应；`metrics.md` 的 `发现总数 85 = ① 37 + ② 48 + ③ 0 + ④ 0 + ⑤ 0`；37 = FACT 17 + DESIGN 5 + READER-A 10 + READER-C 5；48 = DESIGN 10 + READER-A 6 + READER-B 20 + READER-C 12。这三份文件在 85/37/48 上自洽。
- **不成立的**：
  1. `loop.md §7` 写“已修 40 条”，`metrics.md` 写 ① 37。差额 3 正好是 P-REFUTE 的 CLAIM 04/05/06；但这 3 条已声明“并入 FACT”，再计入“已修”就是重复计数。40 与 37 必须二选一。
  2. 文档 §5.2.1 的测试计数仍是错的。文档写 `boot-shim --lib` 97、`minix-boot` 144；实际 `rg '^\s*#\[test\]'` 在 `os/boot-shim/src/*.rs` 是 125（漏了 `dtb_copy.rs` 15 条、`resv_claims.rs` 13 条），在 `os/libs/minix-boot/src/*.rs` 是 175（漏了 `fdt_layout.rs` 29 条、`lib.rs` 2 条）。`loop.md §5` 自己写 `cargo test -p minix-boot`: 175 通过，和文档的 144 直接冲突。FACT-09 的修复只是把旧表的 15 个文件重算了一遍，没有发现旧表本身漏了 4 个文件；这是 B-08 的复发，不是已关闭。
  3. DESIGN-15 标 ①“文档数字与注释同步”，但 `os/libs/minix-boot/src/kernel_info.rs:28-48` 的 `reserved_regions` 注释仍然写“内核链进 boot-shim PE、在这些低物理地址执行、VM 必须恒等重放”，而 `os/kernel/src/vm_handoff.rs:563-574` 已写“identity windows RETIRED from process roots”。文档侧改了，代码注释没改；“注释同步”不成立。
  4. DESIGN-10 标 ①“已修”，但 `arch_boot_finish` 新增的 `kernel_info.validate()` 要求 `boot_modules` 非空；三个 `test-higher-half` QEMU 测试内核都构造 `boot_modules: &[]`，会在 QEMU 里 panic。`loop.md §5` 没有跑 QEMU。详见第 9 项。
  5. `tools/eval-trace-check.py` 按技能文档的命令（只给 run 目录）运行会 FAIL：它默认找 `<run-dir>/panel`，实际面板在 `tmp/review-eval/.../panel`；只有加 `--panel-dir` 才 PASS。`loop.md §5` 只写“exit 0”，没有记录这个 flag，命令不可复现。
  6. `design-d1.md` 的 14 个 `## DECISION` 与 `refute.md` 的 6 个 `## CLAIM` 都没有 `## FINDING` 行；trace checker 对它们只给 WARN，不纳入 85 条覆盖。`loop.md §2` 说 REFUTE 的 3 条已并入 FACT，但 REFUTE 的 A1–A8 观察（例如 `reserved_regions` 恒等重放退役、EBS map buffer 每轮新分配不释放、`pg_map` 的 pde 单调前提）没有处置行；D1 的 14 个决策也没有逐条处置行。
  7. `BLIND-SPOTS.md` 的 13 条“检测证据”全部还是“（待填）”。本轮没有把“哪些 OPEN 项被本轮接住”写回账本；下一轮无法从账本判断进步，只能靠这份 meta 重建。
- 证据指针：`metrics.md:3-7`、`loop.md:95-97`、`loop.md:112`、`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2765-2815`、`os/libs/minix-boot/src/kernel_info.rs:28-48`、`os/kernel/src/vm_handoff.rs:563-574`、`tools/eval-trace-check.py:330-350`、`BLIND-SPOTS.md` 各条“检测证据”。

### 9. 本轮最值得但工作流仍看不见的一件事

**修复的“调用方影响面”不在工作流的视野里。**

具体案例：DESIGN-10 要求 `arch_boot_finish` 在 `store_kernel_info` 前调用 `KernelInfo::validate()` 与 `check_boot_postconditions()`。设计角色只看了 44 个关联文件；修复者照做了；宿主测试 `cargo test -p minix-kernel --features mock` 通过，因为 `arch_boot_finish` 带 `#[cfg(not(feature = "mock"))]`，mock 形态根本不编译它。但 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half{,-aarch64,-riscv64}/src/main.rs` 直接调用 `minix_kernel::arch_boot`，并构造 `boot_modules: &[]`；`KernelInfo::validate` 的第 5 条断言要求 `boot_modules` 非空。结果是三个 QEMU 测试内核会在 `arch_boot_finish` 的第一行 panic，而 L4 的复验清单里没有 QEMU。

这不是“某个人忘了跑测试”。这是工作流的范围模型只包含“文档声明的关联代码 + 修复触碰的文件”，不包含“被修改函数的全部调用方/合同面”。`AGENTS.md` 已经写了“触碰 arch 或 boot 时按 `AGENTS.md` 跑 QEMU”，`review-loop` 的 L3 也写了同样的话；但 L4 的清单只列了宿主测试、五门和文档门，没有把“调用方清单 + 形态门”作为独立步骤。设计角色看不到 QEMU 测试内核（不在 44 个文件里），修复者也没有被要求枚举调用方。这就是本轮最值得、但工作流仍看不见的一件事。

第二值得的是测试计数的“文档自证循环”：P-FACT 按文档自己列的 15 个文件复算，得到 97/144；没有问“这个文件清单本身是否穷举”。正确的口径应该从 crate 的全部 `src/*.rs` 或 `cargo test -- --list` 出发，再看文档覆盖了哪些。这个盲区让一个 P0 级的 B-08 在“已修”之后仍然是错的。

## 三、抽样复判

抽样 24/85 条（28.2%），覆盖 FACT、DESIGN、READER-A/B/C 与 ①/② 两类处置。判据：① = 缺陷为真、本轮修复完成、复验有效；② = 缺陷为真、转出目的地有效。不是只看“标签是否写成 ①”。

| # | 发现 | 台账类 | 独立复判 | 证据 |
|---:|---|---|---|---|
| 1 | FACT-01 | ① | 成立 | 文档 1482 行已改为“内核不保留可变 memmap，Phase A.2 已移除，`FREE_MEMMAP` 不再存在”；`rg FREE_MEMMAP` 只剩这一处说明 |
| 2 | FACT-02 | ① | 成立 | 文档 2129 行改为 `classify`/`adjudicate` 五步判定，类型映射与 `ebs_map.rs:610-644` 一致 |
| 3 | FACT-03 | ① | 成立 | 文档 1564 行 `ARENA_CAP = 9 * 4096`，1571 行写九页/七段/`WIRE_STARTUP_END = 36480`；`rg '四页'` 0 命中 |
| 4 | FACT-04 | ① | 成立 | 文档 1467 行“16 个字段”，引文含 `memmap_sources`/`startup_objects`；与 `kernel_info.rs:23-160` 一致 |
| 5 | FACT-05 | ① | 成立（主项） | 文档 1533-1558 行引文含 `ram_authority`/`sources_byte_len`/`startup_byte_len`，共 25 字段；1571 行写七个分区。残留：1567 行“先校验 magic/version/size/arena_len”仍未列 `ram_authority`/`root_page`/bump 校验（见第五节） |
| 6 | FACT-06 | ① | 成立 | 文档 2148 行 `fn bootstrap_alloc_limit() -> u64`，语义改为 `checked_sub(1)` 约束末字节 |
| 7 | FACT-07 | ① | 成立 | 文档 1859-1876 行引文含 `let already = ...load(Acquire)`、`if already { return; }` |
| 8 | FACT-08 | ① | 成立 | 文档 1918-1921 行写 shim 只 `init_boot_pt_alloc`，`register` 在 `boot_validate_and_prepare`；与 `lib.rs:776-778` 一致 |
| 9 | FACT-09 | ① | **不成立** | 文档 2771-2815 行仍是 97/144；实际属性计数为 125/175；`loop.md:83` 自己写 175。修复只重算了旧文件清单，没有发现漏掉的 `dtb_copy.rs`/`resv_claims.rs`/`fdt_layout.rs`/`lib.rs` |
| 10 | FACT-11 | ① | 成立 | 文档 2138、2949 行明确“没有单独的 `reserved-big` 行”，诊断串补了 `unmapped=` |
| 11 | FACT-15 | ① | 成立 | 文档 1316、1395 行均为 `uboot_main.rs:395`；`rg ':333'` 0 命中 |
| 12 | DESIGN-06 | ① | 成立 | `os/kernel/src/lib.rs` diff 把 `memmap_sources`/`startup_objects` 深拷并重指，新增两条溢出负控；7/7 落地区 |
| 13 | DESIGN-10 | ① | **不成立（修复引入回归）** | `arch_boot_finish` 先 `validate()` + `check_boot_postconditions()`；三个 `test-higher-half` 测试内核 `boot_modules: &[]`，会触发 `KernelInfo::validate` 第 5 条断言 panic；L4 未跑 QEMU |
| 14 | DESIGN-12 | ① | 成立 | `pt_alloc.rs` 的 `uninit_alloc` 改 `panic!`、`alloc_pt_page` 前置 `is_registered()` 断言；`handoff_v1_read.rs` 的 `wire_module_to_boot_module` 返回 `Result`，测试改 `.unwrap()` |
| 15 | DESIGN-14 | ① | 成立 | `boot_alloc.rs` 注释改为“单线程、原子类型不提供并发正确性；SMP 需 CAS 或 per-CPU” |
| 16 | DESIGN-15 | ① | **不成立（部分修复）** | 文档数字与 `handoff_v1.rs:69` 注释已改；但 `kernel_info.rs:28-48` 的 `reserved_regions` 注释仍是“内核链进 boot-shim PE、恒等重放”，与 `vm_handoff.rs:563-574` 的“identity windows RETIRED”冲突 |
| 17 | READER-A-05 | ① | 成立 | 文档 1266 行改为“三类：`allocate_pages`、`allocate_pool`（`Vec`/`Box`）、`println!`” |
| 18 | READER-A-09 | ① | 成立 | 文档 1859-1876 行引文已补 release 守门，与 §4.7 一致 |
| 19 | READER-C-03 | ① | 成立 | 文档 1265 行“传递参数”改为“arena 基址（`rbx`/`x0`）；`KernelInfo` 已序列化进 v1 arena，根页表在信封运行期字段” |
| 20 | READER-C-07 | ① | 成立 | 文档 2558 行 Step 2c 明确 LAPIC/IO-APIC 只由 Step 2c 安装，`kernel_identity_mmio_regions` 是 Step 1 清单 |
| 21 | DESIGN-01 | ② | 成立 | 生产调用全部传 `(0,0)`（`uefi_helpers.rs:1171`、`bootface.rs:333`、`bootface_a64.rs:313`、`opensbi_helpers.rs:313`）；唯一读点是 `validate` 的断言；转出 D01-F03 需要 wire 版本 +1，合理 |
| 22 | DESIGN-05 | ② | 成立 | `memmap_source.rs` 的 `DTB_*` 与三个 boot face 的 `STARTUP_OBJECTS` 从 `static mut` 返回 `&'static`；二次 collect 覆盖；转出 D01-F02 合理 |
| 23 | DESIGN-11 | ② | 成立 | `misc.rs:711-730` 只拷 48 B `KinfoStruct`；C `do_getinfo.c:66-70` 拷 `sizeof(struct kinfo)`；外部 ABI 风险真实，转出 D01-F10 需用户裁决合理 |
| 24 | READER-B-10 | ② | 成立 | `reader-b.md:219-232` 的“排除两个候选→第三个原因”结构确实成立；`reader-c.md:188-225` 独立复现；转出 D01-F15 合理 |

**一致性**：

- 按“处置真实成立”判：21/24 = **87.5%**，低于 90%，按任务要求写 `NOT PASS`。
- 按“只看处置类标签是否匹配”判：22/24 = 91.7%（把 DESIGN-10 视为“类对、修复坏”）。即使按这个更宽的口径，本轮仍因 40 vs 37、97/144 vs 125/175、trace checker 命令不可复现而 NOT PASS。
- 三条不成立项的共性是“修复没有覆盖完整的合同面”：FACT-09 漏了 crate 里其他测试模块；DESIGN-15 漏了 `kernel_info.rs` 注释；DESIGN-10 漏了 QEMU 测试内核调用方。这不是三个孤立的粗心，而是同一个“范围=文档列表”的方法问题。

## 四、P-DESIGN D1 的 PARTIAL-VIOLATION

**判断：需降级（结论可接受，独立性降级；按现行规则字面应作废）。**

D1 自述读了四类材料：

1. 文档标题大纲（`grep -n '^#'`，含 Ch3/Ch4 小节标题）；
2. `minix3/minix/kernel/const.h` 第 1–53 行，用于 `USR_STACKTOP`/`USR_DATATOP` 取值；
3. `minix3/minix/servers/vm/utility.c` 第 40–79 行与 `vm/main.c` 的调用点，用于下游容量约束；
4. 若干只用于定位符号的 `grep`。

它没有读现有 Rust 实现，也没有读 Ch3/Ch4 正文。这一点是关键：D1 的独立性风险主要来自“看见现有 Rust 设计或后续章节的结论”，而不是“看见 C 真值”。

- **对设计结论的影响**：有限。D1 的 14 个 DECISION 里，DECISION 03/13 引用了 `vm/utility.c` 的“VM 只读前 16 条、按数组顺序、零长不报错”约束；这是一个来自清单外 C 源的真实下游约束，它增加了一个设计输入，但没有泄露 Rust 实现或文档结论。DECISION 12（trait 放哪）受 Ch3/Ch4 标题影响而选择了这个题目，但论证完全建立在 C 侧 `i386` 与 `earm` 两份 `pre_init.c` 的漂移上，不依赖标题。`const.h` 只影响一个栈顶取值的判断。
- **对已修项的影响**：没有。本轮 5 条设计修复（DESIGN-06/10/12/14/15）都来自 D2 对现有 Rust 的独立复核，不是 D1 的采纳项；10 条转出里 DESIGN-01 源自 D1 的 NOT-COPIED 11，但 D2 自己复核了生产调用点。D1 的污染没有进入任何一条“已修”。
- **流程层面**：`review-panel.md` §6 写得很硬：“读取禁止材料并在报告中承认的，该角色本轮输出作废。”D1 承认了，按字面应作废；主 agent 改成了规则外的 `PARTIAL-VIOLATION`，这是流程违规。可以理解它的工程动机（D1 的结论已经被 D2 逐条复核，直接扔掉可惜），但规则与执行不一致本身要在 V2 解决。
- **建议**：把材料违规分成两级。读到“现有 Rust 实现 / Ch3-Ch4 正文 / 历史评审”= 输出作废；读到“清单外的 C 真值 / 符号定位 grep”= 输出保留但标 `DEGRADED-INDEPENDENCE`，且该角色的结论不能单独作为设计证据，必须由 D2 或 P-FACT 复核。D1 的 DECISION 03/12/13 应标“清单外证据”。如果下一轮要求严格独立性，D1 应重跑，并把 `vm/utility.c`、`const.h` 纳入 C 语义材料包。

## 五、数字对账

### 5.1 85、37/48/0/0/0

三份文件在这组数上自洽：

- `loop.md §2`：P-FACT 17 + P-DESIGN D2 15 + P-READER A 16 + B 20 + C 17 = 85。
- `defect-ledger.md`：85 行；① 37 = FACT 17 + DESIGN 5 + A 10 + C 5；② 48 = DESIGN 10 + A 6 + B 20 + C 12；③④⑤ = 0。
- `metrics.md`：`发现总数 85`、`处置 ① 37 / ② 48 / ③ 0 / ④ 0 / ⑤ 0`。

**结论：85 与 37/48/0/0/0 自洽。** 唯一的数字冲突是 `loop.md §7` 的“已修 40 条”。40 很可能是 37 + P-REFUTE 的 3 条，但这 3 条已经并入 FACT-15/03/05/07/09，不能在“已修”里再计一次；应改为 37，或明确写“37 条台账发现 + 3 条反证复核”。

### 5.2 文档测试计数

- 文档 §5.2.1：`boot-shim --lib` 97、`minix-boot` 144。
- 实际 `rg '^\s*#\[test\]'`：
  - `os/boot-shim/src/*.rs` = 125（`dtb_copy.rs` 15、`resv_claims.rs` 13 未进文档表）；
  - `os/libs/minix-boot/src/*.rs` = 175（`fdt_layout.rs` 29、`lib.rs` 2 未进文档表）。
- `loop.md §5` 自己写 `cargo test -p minix-boot`: 175 通过。这个读数与文档的 144 直接冲突，也证明 144 不是包总数。
- 文档还写“默认 uefi-only 形态只编 loader + uefi + ebs_map + ebs_pair”；实际 `#[cfg(feature = "uefi")]` 还包含 `dtb_copy` 与 `resv_claims`，默认 uefi 形态也不是 97。这个 97 不对应任何 feature 组合。

**结论：FACT-09 的修复未完成，B-08 复发。** 根因是 P-FACT 以文档自己的 15 个文件为清单复算，没有先问“这个清单是否穷举”。

### 5.3 trace checker 命令

- 技能文档 L2 的命令：`python3 tools/eval-trace-check.py rewrite-notes/.../01-boot-shim-bootstrap`。
- 实际运行：`[WARN] 没有 panel 目录`，然后 `[FAIL] 台账有孤儿处置行`（85 条全部），`exit 1`。
- 加 `--panel-dir tmp/review-eval/.../panel` 才 `exit 0`，且对 `design-d1.md`/`refute.md` 只给 WARN（没有 `## FINDING`）。
- `loop.md §5` 只写“85 条发现全部有处置行，无孤儿行，metrics 自洽，exit 0”，没有记录 `--panel-dir`，也没有记录那两条 WARN。

**结论：轨迹门的绿色不可从文档命令复现。** 这是 B-09 的新实例：门绿了，但门的输入路径与覆盖范围没有被说清。

### 5.4 其他

- `follow-ups.md` 18 条；READER-C 的 06、08–17 共 12 行都指向 D01-F18“reader-c 一致性/指针项”，但 C-13（`map_huge` 忽略错误）与 C-17（`pt_free`/`alloc_mem`）不是“一致性/指针”项，目的地描述不准确。
- `BLIND-SPOTS.md` 13 条检测证据全部“（待填）”；本轮没有回写。
- `loop.md §7` 的 `NOT CONVERGED（design-debt + teaching-debt + test-debt pending）` 与 48 条转出、18 条 follow-up 一致，状态判断正确。

## 六、BLIND-SPOT-DELTA

### 新增

| 编号（建议） | 盲区 | 本轮证据 |
|---|---|---|
| N1 | 测试计数按“文档自己的文件清单”复算，清单漏文件时系统性少报 | 文档 97/144 vs 实际 125/175；漏 `dtb_copy.rs`、`resv_claims.rs`、`fdt_layout.rs`、`lib.rs` |
| N2 | 修复的调用方/合同面不在范围：改公共入口只验证“关联代码+宿主测试”，不枚举 QEMU/集成测试调用方 | DESIGN-10 的 `validate()` 打断三个 `test-higher-half` QEMU 内核；L4 无 QEMU 读数 |
| N3 | 轨迹门只认 `## FINDING`，D1 的 `## DECISION` 与 REFUTE 的 `## CLAIM`/A1–A8 观察逃出覆盖；技能命令默认找不到 `tmp` 面板 | `eval-trace-check.py` 默认 FAIL、加 `--panel-dir` 才 PASS；`design-d1.md`/`refute.md` 只有 WARN |
| N4 | 精度用“③=0”表示，但所有非阻塞项都归 ②；没有 ③/④ 抽样，误报率没有被测量 | `metrics.md` ③④=0；`loop.md §3` 自认 reader B/C 可能含措辞偏好 |
| N5 | 转出目的地可以写成“reader-c 一致性/指针项”这类笼统桶，实质问题（失败路径、所有权）没有独立去处 | `defect-ledger.md` C-06/08–17 全部指向 D01-F18；C-13/C-17 不是一致性项 |
| N6 | 材料违规只有“作废/不作废”二元规则，没有“清单外 C 真值”的分级；执行者只能自创 `PARTIAL-VIOLATION` | `loop.md §1` 的 D1 违规记录；`review-panel.md` §6 |
| N7 | 文档里的“同一天实跑”转录没有对应的 loop 命令记录；修复者可以把属性计数写成 `cargo test` 输出 | 文档 §5.2.1 的 97/144；`loop.md §5` 只有 minix-boot 175、minix-kernel 865，没有 boot-shim 97 |

### 关闭

无。本轮没有把任何盲区机械封堵到“不可能再犯”的程度；B-02 有 `trait-audit.py` 与 D2，但仍是“探测”而非“封堵”。

### 仍 OPEN

| 盲区 | 本轮状态 | 说明 |
|---|---|---|
| B-01 翻译残留 | 已出现 | DESIGN-01/02/07/08/13 都是 C 形状或翻译残留；仍未关闭 |
| B-02 设计死代码 | 已出现 | `BootShim`、`PagingWithId`、`BootForm`、两个死常量；`trait-audit` + D2 接住，但仍 OPEN |
| B-03 跨节矛盾 | 已出现 | READER-C-01/02/03/04/07 等；仍未关闭 |
| B-04 读者理解障碍 | 已出现 | READER-A/B/C 共 53 条；仍未关闭 |
| B-05 假反事实 | 未单独出现 | REFUTE 只查了 6 条主张的前提，没有专门扫“若…会怎样”的反事实；仍 OPEN |
| B-06 断链锚点 | 已出现 | 基线 `anchor-resolve` 报 2 个 ZERO-DEF（`FREE_MEMMAP`、`classify_map`）；修复后 98/98；仍 OPEN（不是机械封堵） |
| B-07 端口义直译 | 未出现 | 本轮没有按词义消歧扫“port/端口”；仍 OPEN |
| B-08 可复算读数漂移 | 已出现但未修复完 | FACT-09 的 97/144 仍错；仍 OPEN |
| B-09 门绿但事错 | 新实例 | trace checker 默认命令 FAIL、加 flag 才 PASS；文档“实跑”转录无 loop 记录；仍 OPEN |
| B-10 重复提已裁决问题 | 未出现 | `re-litigation` 0；D-001/D-011 没有被重提；仍是“避免”，不是“封堵” |
| B-11 范围蔓延 | 部分 | `adjacent 阻塞` 0，但 48 条转出、18 条 follow-up；仍 OPEN |
| B-12 引文块被改写 | 已出现 | FACT-07/13、READER-A-09/C-02；仍 OPEN |
| B-13 硬件机制泄漏 | 部分 | `PTE_HUGE_IDENTIFIER_BIT` 被当死常量报出；没有按“合同里出现硬件位”系统扫；仍 OPEN |

## 七、建议变更

版本纪律是每次最多晋升 2 条；下面按“热修 / V2 首选 / 候选”排序。全部是合并或升级现有装置，没有净增装置。

| # | 案例 ID | 目标文件 | 期望效果 | 回归风险 | 替代/合并 |
|---|---|---|---|---|---|
| R1 | D01-DESIGN-10-QEMU | `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half*/src/main.rs` 与/或 `os/kernel/src/lib.rs` | 三个 QEMU 测试内核不再因 `boot_modules: &[]` 触发 `KernelInfo::validate`；生产 fail-fast 保留 | 放宽 `validate` 会削弱生产 fail-fast；建议给测试内核补一个最小的非空 `boot_modules` 或加 `#[cfg(test)]` 专用校验入口 | 代码热修，不是工作流装置；不改规则 |
| R2 | D01-DESIGN-10-QEMU | `prompt/skill/review-loop.md` L3/L4、`prompt/review-rules/fix-guard.md` | 修改 arch/boot 公共入口时，强制枚举调用方（含 `qemu-tests`、`tests`、`bootface`）并跑对应形态门；宿主测试绿不再等于完成 | QEMU 时间成本；调用方清单可能不全 | 合并进 L4“回归扫描”与 `AGENTS.md` 既有 QEMU 要求；不新增 agent |
| R3 | D01-FACT-09 | `prompt/review-rules/review-doc-checklist.md` Sync-7、或新增 `tools/doc-test-count-check.sh` | 测试计数从 crate 全部 `src/*.rs` 或 `cargo test -- --list` 出发，文档表只作为被对账对象；自动发现 97→125、144→175 | feature 组合与 `#[ignore]` 需要固定口径；可能误报 cfg 排除项 | 升级现有 Sync-7，不是净增 |
| R4 | D01-TRACE | `tools/eval-trace-check.py`、`prompt/skill/review-loop.md` L2/L5、`prompt/review-rules/review-panel.md` §5 | 技能命令默认能找到 `tmp/review-eval/.../panel`；D1 的 `## DECISION` 与 REFUTE 的 `## CLAIM` 要么纳入覆盖，要么要求一张显式映射表 | 台账行数增加；映射表可能变成形式主义 | 合并进现有 trace checker；把“面板目录”写进 run 契约 |
| R5 | D01-BLIND-SPOTS | `prompt/review-rules/review-evals.md` §5/§6、`loop.md` 模板 | 每轮 `loop.md` 增加 `BLIND-SPOTS.delta`，至少填写“本轮接住/未接住/新增”；下一轮不靠 meta 重建 | 记账成本 | 合并进 meta-review 的 `BLIND-SPOT-DELTA`；不新增文件 |
| R6 | D01-D1-VIOLATION | `prompt/review-rules/review-panel.md` §3/§6 | 材料违规分级：现有 Rust/Ch3-Ch4 正文=作废；清单外 C 真值=保留但标 `DEGRADED-INDEPENDENCE`；消除自创 `PARTIAL-VIOLATION` | 分级可能被滥用为“保留输出”的借口 | 替代现行二元“作废”规则；同时把 `vm/utility.c`、`const.h` 纳入 C 语义包 |
| R7 | D01-READER-ADOPT / D01-PRECISION | `prompt/review-rules/review-evals.md` §3.4/§4.1、`metrics.md` 模板、`review-panel.md` §5 | 区分“读者发现已修”与“读者发现已转出”，按 50% 收敛标准判；每轮强制抽 10 条 ② 项判 ③/④，给出误报率 | 额外工作量；抽样可能被形式化 | 合并进 meta-review 已有的 20% 抽样与“重构”指标；不新增装置 |

**优先级**：R1 是本轮代码热修，必须在下一轮之前做；R2、R3 是 V2 首选的 2 条（都直接命中本轮的真实失败）；R4–R7 登记为候选，等 V2 边界或下一篇闭环轮再裁决。

## 八、局限声明

- **同 agent 验证 + 已重放命令。** 本报告由与主 agent 同一模型的冷启动子 agent 完成；我重放了 `rg` 计数、`python3 tools/eval-trace-check.py`（默认与 `--panel-dir` 两种）、`git diff --cached` 阅读，但没有跑完整 `cargo test`，没有跑 QEMU。QEMU 回归是静态代码路径证据（`arch_boot_finish` → `validate` → `boot_modules` 非空断言；三个测试内核构造 `&[]`），不是实际 QEMU 运行结果；置信度高，但仍应由下一次 QEMU 门确认。
- **抽样一致性低于 90%：NOT PASS。** 24 条抽样中 3 条不成立（FACT-09、DESIGN-10、DESIGN-15），按“处置真实成立”判为 21/24 = 87.5%。按“只看标签”判为 22/24 = 91.7%，但这个更宽的口径不能覆盖 40 vs 37、97/144 vs 125/175、trace 命令不可复现这三项；因此本轮整体仍判 `NOT PASS`。
- **token 未采集。** 我无法给出 token 成本；成本判断基于 agent 数与 wall-clock。
- **未读取 `expected/`。** 本报告没有用期望答案反推；D01 的语料条目与面板原始输出是我判断 detection 的主要依据。
- **D1 判断是定性判断。** 我读了 D1 全文与 `review-panel.md` §3/§6，判断“需降级”而不是“需作废”；如果项目坚持 §6 的字面，D1 应作废并重跑，这会让本轮设计证据只剩 D2 的 15 条（其中 5 条已修、10 条转出），不影响已修项的代码事实。
