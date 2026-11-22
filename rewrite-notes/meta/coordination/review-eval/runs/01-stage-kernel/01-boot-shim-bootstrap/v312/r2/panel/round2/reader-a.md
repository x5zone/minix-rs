DOC 01-boot-shim-bootstrap.md

# Reader A（初学者路径）— R2 冷读报告

阅读范围：只读 `/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 全文（2951 行）。未读源码、其他文档、其他角色输出、expected/。按读者画像（C 阅读、OS 启动基本概念、Rust ownership 基础、物理/虚拟地址/页表基础；不具备本项目内部类型、三架构启动路径与前置章节未讲授的内部契约）逐节走全文。下文节号均指该文件内的标题编号，行号为该文件行号。

## FINDING READER-A-01（P2）

- 位置：§4.1（第 1933 行）“窗口构造点见 §4.3 的 `BootWindows`，DM 建表同样从 `pt_alloc` 取页”；§3.2.3（第 1297 行）`BootHandoffV1` 引文编注“声明了权威却无对应 source 记录则 fail-closed（§3.2 的 source 规范集合与 §4.4 的 `validate_requiring_sources`）”。
- 严重度：P2
- 问题：命中清单第 5 条（指针不落空）。按第一个指针翻到 §4.3，只看到取件函数、`MODULE_NAMES`、`entry_path_eq`/`UbootFileLoader`，没有 `BootWindows`；第二个指针说“§3.2 的 source 规范集合”，但 §3.2 的四个子节是字段台账、旧字段去向/GET_KINFO、arena wire 协议、深拷探针，覆盖规则实际在 §3.3（“覆盖审计是 I2 的判据：free 表的每个字节必须能被 `kind = Memory` 的记录解释……”）。两处都把我送到不含目标内容的节。
- 影响：我必须在全文搜索 `BootWindows` 与“source 规范集合”才能继续；按节阅读的路径在这里断一次。若我以为 §4.3 本该有窗口构造，会先怀疑自己漏读了一整段，而不是怀疑指针写错。
- 证据或推理：`BootWindows` 在本文件的出现位置是 §3.6 的 `PreparedJump` 引文（第 1732 行）、§4.1（第 1933 行）、§4.2.2（第 2138 行）、§4.4（第 2370、2372、2388 行）；§4.3 正文范围（第 2160–2286 行）没有它。§3.3 才是两张表与来源覆盖的定义节（“free 表的每个字节必须能被 `kind = Memory` 的记录解释”）。
- 最小修复：第一个指针改成“§4.2.2/§4.4 的 `BootWindows`”（或 §3.6 的窗口构造引文）；第二个改成“§3.3 的来源覆盖规则”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能 `BootWindows` 在 §4.3 的某个代码引文块里被我漏看——但我按出现行号核对，§4.3 范围内没有该标识符；也可能作者把 §3.2.1“权威与内存图绑定”一段当作“source 规范集合”，但那里既没有列覆盖规则，也没有用这个词。

## FINDING READER-A-02（P2）

- 位置：§4.8（第 2545 行）“Direct Map 覆盖建立发生在启动信息存储之后、进入 `kmain` 之前，但两条腿的链不同：直核面是 `store_boot_report` → `arch_boot_impl`（`build_bootstrap_root_and_enable` → `establish_boot_dm`）→ `jump_to_kmain`；v1 接收面是 `store_kernel_info` → `resume_additive` 内的 `establish_boot_dm` → `jump_to_kmain`”。
- 严重度：P2
- 问题：命中清单第 2、3 条（因果主张的证据与前提）。这句话按箭头读，`store_kernel_info` 发生在进入 `resume_additive` 之前；但 §3.7 说“v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`”，§4.5 说“`resume_additive` 的 `store_kernel_info` 源已是前一步解码落进 store 的切片（同址，跳过）”，§3.6 引的 `resume_additive` 文档注释也是“validate/register allocators, land the KernelInfo into this image's `.bss`, establish the direct map, then enter `kmain`”。三处都把 store 放在 `resume_additive` 之内且在校验之后；§4.8 的箭头串却把 store 放在 `resume_additive` 之前。
- 影响：如果按 §4.8 读，我会以为 v1 接收面先 store/发布 `KernelInfo`、再校验，这与 I4“校验先于消费”的整个论证相反；我也会把 §4.4.1 的门链顺序画错（v1 接收面到底是 strict → postconditions → resume_boot_pt_alloc → boot_validate → store，还是 store 先于 resume_additive）。
- 证据或推理：§3.7（第 1845 行）的“v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`”；§4.5（第 2461 行）的“`resume_additive` 的 `store_kernel_info` 源已是前一步解码落进 store 的切片（同址，跳过）”；§3.6（第 1797–1798 行）的引文“validate/register allocators, land the KernelInfo into this image's `.bss`, establish the direct map, then enter `kmain`”。
- 最小修复：把 §4.8 的 v1 链改成“v1 接收面在 `resume_additive` 内先 `boot_validate_and_prepare`、后 `store_kernel_info`，再 `establish_boot_dm` → `jump_to_kmain`”；或至少加“（`store_kernel_info` 在 `resume_additive` 内，见 §3.7）”，并说明箭头不是严格时间序。
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能作者用“store_kernel_info → `resume_additive` 内的 establish_boot_dm”表示“store_kernel_info（在 resume_additive 内）→ establish_boot_dm”，箭头只连 store 与 DM、不连 resume_additive 的入口；但字面顺序仍会被读成 store 先于 resume_additive，且 §4.8 没有“之内”的括号。

## FINDING READER-A-03（P2）

- 位置：§3.1 分工表第一行（第 1086 行）：“| 识别可执行文件 | ELF 头（本仓 boot2）／前 8KB 的识别头（GRUB 一类 Multiboot 引导器）（2.1） | UEFI 的 PE/COFF 头（标准可执行格式） | ELF 头 + U-Boot `bootelf` 约定 |”。
- 严重度：P2
- 问题：命中清单第 2、6 条（同一行混了两个对象，结论缺前提）。C 列与 U-Boot 列描述的是“内核镜像被识别”的格式；UEFI 列写 PE/COFF，但同一张表的下一行说 boot-shim 自己读 `loader::KERNEL_PATH`，§4.3 给出 `KERNEL_PATH = "/EFI/minix/kernel.elf"`，§4.2 用 `compute_kernel_layout(elf_data.as_slice())` 解析 ELF。我无法判断 UEFI 列说的是 boot-shim 自身（由固件按 PE/COFF 装载）还是内核（ELF）。
- 影响：我会以为 UEFI 路径的内核是 PE/COFF，随后看到 `kernel.elf` 与 ELF 段解析时无法调和；或者我会把 C 列（内核识别）与 UEFI 列（shim 识别）当成同一对象，误读整张分工表的三列对照。
- 证据或推理：§4.3（第 2166 行）`pub const KERNEL_PATH: &str = "/EFI/minix/kernel.elf";`；§4.2（第 1952–1956 行）`let elf_data = file_loader.read_required(loader::KERNEL_PATH);` 与 `loader::compute_kernel_layout(elf_data.as_slice())`；§4.2（第 1962 行）`loader::load_segments_into_phys_memory(&layout)`（ELF 段装载）。
- 最小修复：把 UEFI 列改成“boot-shim 自身：PE/COFF（固件装载）；它读取的内核：ELF 头”；若这一行只想说内核，就直接写“ELF 头”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能这一行的“可执行文件”专指 boot-shim 自身，而 C 列的“ELF 头”指 boot2 识别内核；但三列并列时读者会默认同一对象，且 UEFI 列没有写“shim 自身”。

## FINDING READER-A-04（P2）

- 位置：§3.2（第 1149 行）“**维度一：通道形态**（交接怎么送到接收方）随交接形态变；"跨镜像 / 直核 / 三形态"的定义在 §3.6，这里先用。”
- 严重度：P2
- 问题：命中清单第 1、5 条（术语首次定义与指针）。这个短语列了两个形态名（跨镜像、直核）和一个计数（三形态），没有第三个形态名；§3.6 的计数表说三形态是“同镜像内调用 / 跨镜像跳转 / 直核自引导”。我无法判断“三形态”指哪三个，也无法判断“直核”是否就是“直核自引导”。
- 影响：紧接着的“三形态一致的是‘必须深拷进私有存储’这一条要求”失去指代；我不知道它是否包括历史的“同镜像内调用”，也不知道两个直核腿与三个 v1 腿如何落进这三种形态。
- 证据或推理：§3.6（第 1835 行）的计数表“形态 | 3 | 同镜像内调用 / 跨镜像跳转 / 直核自引导”；§3.6 又说“同镜像是测试/历史形态，当前生产腿只有跨镜像与直核两类”。
- 最小修复：改成“跨镜像、直核与同镜像（历史形态）三种通道形态的定义在 §3.6，这里先用”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能“跨镜像 / 直核 / 三形态”是排版时把“同镜像”误删成“三形态”；但现文没有第三个形态名，读者只能猜。

## FINDING READER-A-05（P3）

- 位置：§3.5.1（第 1549 行）“U-Boot 腿的引导准备入口是普通函数 `prepare_boot`，不使用单实现 trait 做抽象。理由有三条：唯一实现是 U-Boot 腿的 `RiscvUbootBootShim`；没有 `dyn BootShim`，`prepare_boot` 还是关联函数，不参与对象分派；……”
- 严重度：P3
- 问题：命中清单第 6 条（同一段内两处说法冲突）。前一句说它是普通函数，后一句说它“还是关联函数”；我不知道它到底是 `opensbi_helpers` 的自由函数，还是 `RiscvUbootBootShim` 的关联函数。
- 影响：我无法画出 `uboot_main → prepare_boot` 的调用图，也无法判断“没有 dyn BootShim”这条理由是在说现行设计还是被否决的旧 trait 设计；读到 §3.5.1 末尾“`prepare_boot` 只承担前者的固件差异”时仍不确定它的定义位置。
- 证据或推理：紧接着的段落（第 1551 行）“现在 U-Boot 腿的入口是一条普通函数：`fn prepare_boot`，由 `fn uboot_main` 直接调用”；锚点是 `os/boot-shim/src/opensbi_helpers.rs:fn prepare_boot`。
- 最小修复：拆成两句：“旧的单实现 trait 里 `prepare_boot` 只是关联函数，不参与对象分派；现在它是 `opensbi_helpers` 的普通函数，由 `uboot_main` 直接调用。”
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能“还是关联函数”是“即使按 trait 写法也只是关联函数”的省略，作者在同一段里已用“不使用单实现 trait”限定了语境；但字面上“普通函数”与“关联函数”仍互斥。

## FINDING READER-A-06（P3）

- 位置：§3.2.1（第 1175 行）“`kernel_info.rs` 的 `struct KernelInfo` 共 10 个字段。字段删除时去掉了 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个（见 §3.4.1 与 §3.4.3）；`authority` 是随后加入的那一个，总数由 13 变成 14；wire v10（设计裁决）又删掉 `startup_objects`、`user_sp`、`kern_stack_top`、`syscall_entry` 四个，落到 10。”
- 严重度：P3
- 问题：命中清单第 4 条（计数只有一个权威口径）。本段没有给出删除前的 16；按字面算 13 − 3 + 1 − 4 = 7，与“落到 10”和开头“共 10 个字段”冲突。§3.2.2 才补出“由 16 变成 13”。
- 影响：我会怀疑字段台账的总数，或者把 13 当成删除前的数，从而把 `KernelInfo` 的字段数记成 7 或 11；后面 §4.4 的九字段视图与 §4.5 的落地区表也会跟着对不上。
- 证据或推理：§3.2.1 开头“共 10 个字段”；§3.2.2（第 1222 行）“使字段数由 16 变成 13、`authority` 加入后为 14”；§3.2.1 的“删除”行列 22 个 C 字段，但那是另一个计数。
- 最小修复：在本段一次写全“16 → 13（删三个）→ 14（加 authority）→ 10（v10 删四个）”；后续只留指针。或在本段加“（删除前为 16，见 §3.2.2）”。
- 置信度：高
- 是否需源码核实：否
- 若我错了：可能作者默认读者已经看过 §3.2.2 的 16→13，或“字段删除时”的 13 是删除后的数；但 §3.2.2 在本段之后，线性读者先看到的是缺前提的算式。

## SUGGESTION（未达阻断门槛的改进机会）

以下每条注明是否命中读者验收清单条目；因为不阻断阅读，按要求放 SUGGESTION 而不放 FINDING。

1. **`Sv39`/`satp` 首次出现无定义**（命中清单第 1 条）。位置：§3.4.3（第 1523 行）“riscv64 复制 Sv39 L2 的 256–511 项、satp 切换整根”；§4.1（第 1933 行）“riscv64 为 Sv39 root”。最小修复：首次出现处加一句“Sv39 是 RISC-V 的三级页表格式；satp 是装载根页表物理地址的寄存器”。置信度：高。
2. **“直核腿/直核面”“UEFI 链/产线/腿”同义混用**（命中清单第 1、10 条）。位置：§1.1（第 70 行）“两个直核腿同镜像”；§1.0 术语只定义“直核面”；§3.1.1、§3.6 交替用“UEFI 链”“UEFI 产线”“UEFI 腿”。最小修复：在 §1.0 术语加一行“直核腿 = 直核面；UEFI 链 = UEFI 产线 = UEFI 腿”，或全文统一一个词。置信度：高。
3. **§2.7 的“四步”与 §1.4 的“`pg_*` 五步”没有显式换算**（命中清单第 4 条）。位置：§2.7（第 970–975 行）四步表；§1.4（第 117 行）“`pg_*` 五步”。最小修复：在四步表下加一句“四步把五个函数按语义合并：步骤 2 = `pg_clear`+`pg_identity`+`pg_mapkernel`；步骤 3 = `pg_load`+`vm_enable_paging`”。置信度：高。
4. **§5.1 的 I3 行是单个巨型单元格**（命中清单第 6、9 条）。位置：§5.1（第 2612 行）“I3 所有权明确”一行同时讲五条腿、四条门链、aarch64 直核、riscv64 bootface、aarch64 UEFI、走到 `kmain` 的证据与“不证明什么”。最小修复：拆成 2–3 行（宿主证据 / 真机证据 / 不证明什么）或改成三个 bullet；每条腿一行更易读。置信度：高。
5. **§3.2.2 一节混装四个主题**（命中清单第 6 条）。位置：§3.2.2 标题“旧字段去向、GET_KINFO 外部面、控制台与 `param_buf`”。最小修复：把 GET_KINFO 与两种控制台移到 §4 的接口/诊断章，或把它们降为 §3.2.2 的两个独立小节并在标题里标清；字段去向单独成节。置信度：中。
6. **正文里的过程/历史词**（命中清单第 7 条）。位置：§3.2.1（第 1175 行）“字段删除时”“随后加入”“设计裁决”；§3.4.1（第 1389–1402 行）“先是把 `(0, 0)` 写死”“曾有一次”“那次险情”；§4.6（第 2509 行）“合并曾被试过……合并方案已回滚”。最小修复：把这些集中到一个明确标注的“设计史/裁决记录”块，正文只留现状与判据；或改写成现在时（“当前设计是……；被否决的方案是……”）。置信度：中。
7. **“UEFI 图常超 40 条”缺证据指针**（命中清单第 2、3 条）。位置：§3.2.1（第 1181 行）。最小修复：加“（本仓 UEFI 快照的条目数读数见 §5 的测试或某次门日志）”或改成“本仓观察到的 UEFI 快照可超过 40 条”。置信度：中。
8. **“所以 1 MiB 够用”缺实测前提**（命中清单第 3 条）。位置：§4.1（第 1933 行）“只是窗口数与逐页拆解面远小于跨镜像腿，所以 1 MiB 够用”。最小修复：补一句“按当前构建 `boot_alloc_used_bytes()` 的读数为 … 页”或改成“当前构建的余量足够；预算仍是上限，不是实测值”。置信度：中。
9. **§3.4.3 的“见 §3.4”是自指/过宽指针**（命中清单第 5 条）。位置：§3.4.3（第 1529 行）“wire 版本 7→8 的字节数变化见 §3.4”。该字节数变化实际写在 §3.4.1（第 1389 行）。最小修复：改成“见 §3.4.1”。置信度：高。
10. **§2.4.2.3 先用“第三行常态”后定义**（命中清单第 1 条）。位置：§2.4.2.3（第 705 行）“切除一次都不发生（第三行常态）”；定义在第 724 行“本篇把……落在第三行称为第三行常态”。最小修复：把定义移到第 705 行之前，或首次出现处写成“（下表第三行，后文称第三行常态）”。置信度：高。
11. **§3.4.1 过度回收表的列名与两行内容不一致**（命中清单第 6 条）。位置：第 1393 行表头“误回收后先坏在哪（按‘取页时谁还活着’读）”，但第 1397–1398 两行写“**不构成先坏点**”。最小修复：列名改成“误回收后的受害面（含‘不构成先坏点’的行）”，或把“不构成先坏点”另立一列。置信度：中。
12. **§3.2.2.2 “dmp_kernel.c 读四个字段”没有点名**（命中清单第 2 条）。位置：第 1255 行“`dmp_kernel.c` 读四个字段”。最小修复：写出四个字段名，或指向具体行号；否则读者无法复核“至少四个活读取点”的清单。置信度：中。

## 未覆盖

- 未核实任何源码层事实：`BootWindows` 的真实定义位置、`prepare_boot` 的真实签名与所属（自由函数/关联函数）、UEFI 内核的真实可执行格式、各门链的真实调用序、§5.2 的测试读数、§5.4 的脚本输出，均只按文档内句子判断。若作者认为某条需要源码核实，请按该条标注复核；本报告没有自行查源码。
- 未读 02 篇（`02-higher-half-kernel.md`）的 J3、4.3、4.5 节，因此没有核对本篇对 02 篇的承诺（`k_initial_stktop` 判据、换栈、`AT()`、运行期映射）是否真的兑现；只检查了本篇内的指针是否指向本篇存在的节。
- 未读其他文档、其他角色输出、expected/、历史面板文件；未做跨篇一致性或与 C 源码的对账。
- 未逐条核对 §7.2/§7.3 的文件清单是否都存在，也未运行文中的任何 `grep`/`cargo`/QEMU 命令。
- 未检查 Markdown 渲染效果（只按原文逐行阅读）；标题与段落之间缺空行的地方不影响我的理解，未列为发现。
