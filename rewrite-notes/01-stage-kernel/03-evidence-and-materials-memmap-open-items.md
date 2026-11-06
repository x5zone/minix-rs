# 内存图政策评审简报 —— 取证材料与复核锚点

> **历史材料，不是现行规则**：本文件记录取证过程与逐轮读数，其中出现的旧公式、旧符号均为历史文本；现行规则只以 `01-adjudication-brief-memmap-open-items.md` 为准。

> **创建**：2026-11-04（第十轮评审回填之后）
> **为什么单独成册**：送审正文已经长到无法整份贴给看不到仓库的评审人，而下面两节属于**取证坐标**与**取证计划**——它们不是待裁政策，读者不需要在评审第一遍时读完它们。正文的第十节与第十三节保留编号与指针，本节内容**逐字移出，未改写、未删减**。
> **与正文、档案的分工**：正文 `01-…` 是当前政策状态、合同条款、待裁问题与事实分级；审阅历史 `02-…` 是各轮改变了什么、双方各自撤回过什么；本册 `03-…` 是**拿到仓库访问权之后要复核的位置清单**（第一节）与**升级为实现合同所需的材料包与逐轮取证项**（第二节）。
> **一条不可反悔的约定**：移出不等于降级。正文里凡写「现量命令见 §10」「按 §13 材料包 N 取证」之处，指向的就是本册；本册每条锚点都带**本方核对轮次与现量命令**，读者可以逐条重跑。
> **一条第十轮新增的规则**：本册任何「本仓某处是缺陷 / 还没有做」的表述，必须与一条本方跑过的现量命令同段出现——这是正文 §0 的取证硬规则在本册的同等适用。

---

## 一、拿到仓库访问权后的复核位置清单（原正文第十节）

**分类与政策**

- `os/boot-shim/src/ebs_map.rs:MEMORY_DESCRIPTOR_SIZE`、`OFF_ATTRIBUTE` —— 40 字节规范长度与属性偏移
- `os/boot-shim/src/ebs_map.rs:attr` —— 逐位常量、`KNOWN` 允许掩码、`TYPE_INDEPENDENT`；`:UNKNOWN_FOR_TESTING` —— §7.1 副产品的现场
- `os/boot-shim/src/ebs_map.rs:PolicyError` —— 含「页数为零」「起点对齐」「长度溢出」「超出可表达范围」等独立错误码（§0.2 几何约束的代码事实）
- `os/boot-shim/src/ebs_map.rs:adjudicate` —— 裁决顺序的实现（§2.1）。**本方核实当前实现是「四段」形状**：未知类型与未知/ISA 属性位 → 拒绝启动类型 → 类型强制处置（`RESERVED`/`UNUSABLE` 排除、`MMIO` 两类走设备路径）→ `TYPE_INDEPENDENT` 位裁决，之后是属性准入加类型表，末端一条 `_ => 诊断成员` 兜底。第六轮新增的「类型与生命周期属性一致性」那段**代码里还没有**；落地时它插在类型强制处置与独立属性裁决之间，且只检查「运行时服务类型缺 `RUNTIME` 位」这一个方向（现量命令：`grep -n 'TYPE_INDEPENDENT' os/boot-shim/src/ebs_map.rs` 看它前面的那段判据里有没有按类型分支的一致性检查）
- `os/boot-shim/src/ebs_map.rs:Disposition`、`:Classification` —— 五种去向，其中排除与设备路径不产生输出；三个计数器里没有排除的位置（§5.1 依据）
- `os/boot-shim/src/ebs_map.rs:classify` / `Plan` / `Plan::write_to` —— 可失败预检与不可失败写入的凭证形态（§7.2 第 1 条）
- `os/boot-shim/src/ebs_map.rs:validate_header` —— **本方核实的「描述符版本已在校验」**：版本不等于规范定义值即整图作废，步长小于 40 报错，步长不是 8 的倍数报错，`map_size` 不是步长整数倍报错，条数由 `map_size / 实际步长` 得出（不是「缓冲区长度 ÷ 40」）。因此第五轮那条「补齐描述符版本门控」在本仓是**记录缺口而非校验缺口**——三个版本字段没有一个进入摘要行或致命行（现量命令：`grep -n 'descriptor_version' os/boot-shim/src/ebs_map.rs os/boot-shim/src/ebs_pair.rs`）
- `os/boot-shim/src/ebs_map.rs:adjudicate` 最后一段类型表末尾的 `_ => Ok((Disposition::Side, len))` —— **本方核实的隐式政策**：运行时服务代码/数据、`ACPI_NVS`、`PERSISTENT` 都落在这条兜底分支上，结果与 §2.1 补的显式政策一致，但正文从未命名它；在兜底之前新增任何准入分支都可能静默改变这三类的去向（§11.4 第 2 条现存缺陷）
- `os/boot-shim/src/ebs_map.rs:SIDE_CAP`（64）与 `os/libs/minix-boot/src/handoff_v1.rs:WIRE_MEMMAP_CAP`（128）、`:WIRE_RESERVED_CAP`（256） —— 容量合同与行 7 的命名问题
- `os/boot-shim/src/uefi_helpers.rs:build_memmaps` 与 `:build_memmap` —— 测试腿的分类入口与只取可用表的封装（15 个引导期测试载体经后者共用）
- `os/boot-shim/src/uefi_helpers.rs:exit_boot_services` —— **封装返回的快照被丢弃**那处注释（§3.1 的代码事实）
- `os/boot-shim/src/uefi_helpers.rs` 的池分配器（`allocate_pool(LOADER_DATA)`）与 `:pa_limit` —— 引导期堆类型（行 5 守卫依据）与架构物理上界
- `os/boot-shim/src/uefi_helpers.rs` 的 aarch64 设备树快照段 —— 两份内存来源并存、`/memory` 只作「出界闸」不作来源、自占用记账待裁决的注释（§4.4 的代码事实）
- `os/boot-shim/src/ebs_pair.rs:pair_and_classify` —— 生产式事务循环（路径 B），经函数指针访问固件服务

**架构属性（裁决一依据）**

- `os/arch/src/arm64/paging.rs:MAIR_EL1_VALUE`、`os/arch/src/arm64/ap_early_entry.rs` —— 主处理器与从处理器都写 `MAIR_EL1`
- `os/arch/src/x86_64/paging.rs:flags_to_pte`、`os/arch/src/x86_64/ap_early_entry.rs` —— 全仓无 `PAT` 编程点，从处理器路径也没有
- `os/arch/src/riscv64/paging.rs:flags_to_pte` —— 不写 PBMT
- `os/arch/src/x86_64/{ap_early_entry.rs,fpu.rs,paging.rs}` 的 `CR0` 写点 —— **第九轮按新规则复核过计数**：`grep -rn 'mov cr0' os/arch --include=*.rs` 命中 6 行，第 6 行是 `os/arch/src/arch/paging.rs` 的一句文档注释（描述 `mov cr0, cr0 | PG` 那类顺序），不是写点；真实写点 5 处，全部是「读出—改自己关心的位—写回」，**没有任何一处读、检查或清零 CD / NW**（§1.3 条款⑦）
- `os/boot-shim/src/ebs_pair.rs` 与两份上游文档的交叉核对 —— 本轮新增的取证规则是「写缺陷前先查项目自己的文档」：退出状态码那三条同时存在于设计笔记 §6.4 的伪码、阶段台账九步序列表之后那句「失败面按状态码分派」，以及两枚宿主用例（现量命令：`grep -rn -e 失败面按状态码分派 -e ExitStatus rewrite-notes os/boot-shim/src`）
- `os/kernel/src/lib.rs:identity_device_windows` —— 设备窗口属性的逐架构来源（x86-64 依赖固件 MTRR；RISC-V 注释记下「模拟器可用、真实硬件不满足设备语义」）

**映射覆盖与两棵树（裁决六依据）**

- `os/kernel/src/lib.rs:build_bootstrap_root_and_enable` —— 恒等段无差别铺到 4 GiB 上界、映射失败直接忽略
- `os/kernel/src/lib.rs:identity_block_touches_device`、`:identity_piece_is_device` —— 大块与设备窗口相交时拆片加设备属性的现成钩子（禁止清单要复用的位置）
- `os/kernel/src/dm_coverage.rs`（模块说明的四来源并集）、`:bootstrap_tree_candidates`、`:kernel_dm_pa_end`、`:establish_boot_dm` —— 最终直接映射的来源与覆盖末端
- `os/kernel/src/boot_alloc.rs` —— 自举页表分配器与「必须先登记再建表」的时序
- `os/boot-shim/src/loader.rs` —— 「引导期由固件恒等映射物理 RAM」的说明，§6.4 那条已知局限的依据

- `os/boot-shim/src/ebs_map.rs:checked_len` —— **第六轮找到的「两步 checked」样板**：先 `checked_mul(页数, 页大小)`、再 `checked_add(起点, 字节长度)`，末端与架构上界比较另给独立错误码，三者各有宿主用例逐条断言。§4.4.2 的溢出合同以此为形状，而不是以那条饱和的 C 复刻函数为形状（现量命令：`grep -n -e checked_mul -e checked_add os/boot-shim/src/ebs_map.rs`）
- `os/kernel/src/memmap.rs:cut_memmap` 的**末端计算** —— **本方核实为饱和加法**，且注释自述「溢出的末端夹到地址空间顶端、最上面那 4 KiB 不被切掉，对真实内存表不可达」。这是第六轮那条「两种行为未必天然一致」的读数依据：它对自己的调用场景是对的，但不能当规划阶段的溢出样板（现量命令：`grep -n 'saturating_add' os/kernel/src/memmap.rs`）
- `os/boot-shim/src/uefi_helpers.rs:assert_bootstrap_outside_memmap` —— **第六轮查出的现存脆弱性**：相交判定的两处末端用裸加法；今天不可达（上游 `checked_len` 已拒过溢出），但没有类型层面的保证。现量命令：`grep -n 'fn assert_bootstrap_outside_memmap' -A 12 os/boot-shim/src/uefi_helpers.rs` 看末端是否为裸加
- `os/boot-shim/src/uefi_helpers.rs:pa_limit` —— 架构可表达上界的**独占语义**与两个实际取值（AArch64 `1 << 48`、x86-64 `1 << 52`，其余目标判定为接线错误）；分类器判的是 `末端 > 上界` 才越界，故「末端恰好等于上界」合法（§4.4.2 上界条款的读数来源）
- `os/boot-shim/Cargo.toml` 与 `os/kernel/Cargo.toml`、`os/libs/minix-boot/Cargo.toml` 的依赖段 —— **第六轮的归属读数**：引导壳已经依赖内核库（所以不存在「反向依赖」这个前提），内核依赖 `minix-types` 与 `minix-boot`，而 `minix-boot` 只依赖 `minix-types` 与设备树库、不依赖内核。两个消费方的交集因此是 `minix-boot`/`minix-types`，新区间数学落这里（§4.4.2）
- `os/kernel/src/dm_coverage.rs` 的来源并集说明与**已知残余缺口**那段 —— **§6.8 的现场证据**：注释记录内核镜像与栈不在常规内存表、不是启动模块、也不是自举页表树，只靠前三个来源时它们的直接映射别名从未被装过，走别名访问即缺页（留着具体故障地址与「每轮复现」的读数）；并把「一条保留描述符同时跨越候选与无关页时尾部没有叶子」记为需要区间减法才能闭合的未竟项
- `os/kernel/src/boot_alloc.rs:boot_alloc_region` 与交接头的 `bootstrap_start`/`bootstrap_len`/`root_page`/`kern_stack_top` —— 登记表各字段的**现有取值来源**：自举区可枚举，页表根与镜像有物理地址，但**栈只交出虚拟栈顶**（§6.8 第三问的由来）
- **第七轮的关键读数：物理地址上界在本仓有两份实现，而且内存图准入用的是粗的那一份**
  - `os/boot-shim/src/uefi_helpers.rs:pa_limit` —— 分类器用的上界，按 `cfg(target_arch)` 取编译期常量（AArch64 为 1 左移 48、x86-64 为 1 左移 52），其余目标判定为接线错误。它的注释自己写着「计划由分页层的 `phys_addr_limit` 访问器取代；在此之前，这个上界的作用是防止分类器拒绝高地址内存的描述符」——即本仓已承认这是占位，而且它防的是**过严**，没有防**过松**
  - `os/arch/src/x86_64/paging.rs:phys_addr_limit` —— **能力上限**：先取扩展叶子 `0x8000_0008` 的 EAX 低 8 位（扩展叶子不足时判为不支持），再与 4 级分页格式的 52 位取小；位数为 0 或不小于 64 时报错而不是猜。现量命令：`grep -n 'cpuid_phys_addr_bits' os/arch/src/x86_64/paging.rs`
  - `os/arch/src/arm64/paging.rs:phys_addr_limit` —— 读 `PARANGE` 编码，低于 48 位时直接判为不支持
  - `os/arch/src/riscv64/paging.rs:phys_addr_limit` —— 平台位数与分页模式编码位数取小（Sv39 下为 1 左移 48；两个常量分别是 48 与 56）。**这条读数修正评审人「文档没给 RISC-V 取值」的前提**：有值，缺的是那个平台位数按平台登记的证据
  - `os/kernel/src/lib.rs` 里那条「分页层能力上界不小于恒等段上界」的断言，以及 `os/kernel/src/boot_policy.rs` 用能力上界判物理末端是否越界的判据 —— 两份上界**已经**在这两处会合，说明能力上限在内核侧是实际生效的判据，不是文档里的概念
- `os/kernel/src/dm_coverage.rs` 的四份来源并集与 `bootstrap_tree_candidates`（最多两个候选：页表根页 + 自举分配区，且对「根页落在自举区内部」做了前向裁剪并防了空余量回绕）—— 这是 §6.8 第三种派生结果的**接入位置**；那段来源说明同时留着一句「需要对已装候选做区间减法才能闭合」的未竟项，因此第七轮的验收要求是它被消掉，而不是在它旁边再加一份来源
- `os/boot-shim/src/ebs_pair.rs` 的 `map_buf` 与 `os/boot-shim/src/ebs_map.rs:MapStorage`（描述符数组与旁路清单两块静态存储）—— 本方核实**原始内存图缓冲与分类器输出存储是两块不同的存储**，故按可追溯性判据复查时前者要单列为一个登记对象。现量命令：`grep -rn map_buf os/boot-shim/src/ebs_pair.rs`
- `os/boot-shim/src/main.rs` 的中止处理路径 —— 注释写明它「驻留」、刻意不用堆、把诊断文本格式化进栈上一块定长缓冲；这就是「引导壳自身映像与执行栈必须登记」的现场理由：它可运行的前提正是那些页没被分配器拿走
- `os/boot-shim/src/uefi_helpers.rs` 的设备树快照取用点 —— 出界闸在本仓的**唯一生产调用点**（两条裸机腿从不叫那个函数），故 A 案落地的判据是生产调用点 1 → 0（§13 对应行已按此改写）

**设备树来源与两条 DTB 的数据流（第十轮核对）**

- `os/libs/minix-platform/src/kind.rs:parse_by_kind` —— **内核按交接结构里的原始物理地址解引用解析设备树**（`DeviceTreeDesc::parse(source.phys_addr())`），其安全注释自述「调用方必须保证该地址指向有效固件表并在调用期间保持可读」。这条读数否证了本文第七轮起写的「原始缓冲区最后使用点＝复制完成」——**复制这个动作在本仓不存在**（现量命令：`grep -rniE 'copy|clone|memcpy' os/boot-shim/src --include=*.rs` 里与设备树相关的命中为 0）
- `os/boot-shim/src/uefi_helpers.rs:find_platform_sources` —— 配置表那份设备树**只被登记成一个地址**（`PlatformDescSource::new(DTB, PhysBytes(e.address))`），引导壳从不按它解析任何节点（现量命令：`grep -n 'DEVICE_TREE_GUID' os/boot-shim/src/uefi_helpers.rs` 命中三行：注释、常量定义、这一处取地址）
- `os/libs/minix-platform/src/kind.rs` 的 RSDP 分支注释 —— 仓库内一条**历史实测读数**：AAVMF 交出的配置表里「8 个条目、ACPI2 在、**两个设备树 GUID 都不在**」。本方把它按 **[待复现]** 登记（它是一句注释里的记录，本轮未重新跑），并据此给出 §12.4 的新问与 §13 的取证项
- **保留语义在本仓没有任何读取点**（第十轮把范围从「这条腿」扩到「全仓」）：`grep -rn 'reserved-memory' os --include=*.rs` 读数为 0；`grep -rn -e DtbMemoryReservationBlock -e memory_reservations os --include=*.rs` 读数为 0
- 本轮新增的集合与判据在代码里都还不存在（按本文的规则，写「缺」就要能给出现量命令）：`grep -rn -e FinalMapDenyRanges -e StartupAccessRequirements -e DeviceAddressRanges os --include=*.rs` 读数为 0；最近的同形物是 `os/kernel/src/lib.rs:identity_device_windows`（`grep -rn identity_device_windows os --include=*.rs` 读数为 5，回答「哪些设备窗口进引导根恒等段」，但没有义务记录、授权来源与容量合同）

**RISC-V 原子用法的规模（第十轮）**：`grep -rc -e compare_exchange -e AtomicU64 os/kernel os/libs --include=*.rs` 的命中相加为 122 处；`grep -rn -e Zacas -e Zabha os --include=*.rs` 读数为 0 —— 即**准入清单要求的「实现路径」这一层在本仓连名字都还没有**

**设备树来源**

- `os/libs/minix-boot/src/memmap_source.rs:DtbMemoryMap`、`:dtb_covers_conventional` —— `/memory` 解析、固件常驻区裁剪与出界闸的实现
- `os/libs/minix-boot/src/memmap_source.rs:DTB_MEMMAP_MAX` —— **本方核实的静默丢弃**：上限为 8；`collect` 里对可用表与保留表都是「未满才写、满了什么都不做」，没有失败也没有诊断。多 bank 机器上表现为少识别一整块 RAM 且无线索。§4.3 的多段 `reg` 条款因此对本仓是一条**现存缺陷**，不是假想风险（现量命令：`grep -n 'DTB_MEMMAP_MAX' os/libs/minix-boot/src/memmap_source.rs` 后看两处写入点有无 `else`）
- `os/kernel/src/memmap.rs:cut_memmap` —— **本仓已有的页级归一化同源实现**：起点向下、末端向上取整到 4 KiB，末端相加饱和并注释了「不裁掉最高地址空间最后一页」这一选择；复刻 C 行为且有宿主用例。§4.4.2 的区间数学应与它同源，不再各写一遍
- `os/boot-shim/src/ebs_pair.rs:pair_and_classify` 循环体 —— 重试循环内**没有**任何引导服务控制台写入。**第九轮修正本方此前在此处写的「纯属巧合、没有断言」**：`os/boot-shim/tests/ebs_boundary_audit.rs` 已对 `prepare_boot` 与 `prepare_and_enter_kernel_image` 两条入口做文本级审计（剥注释、扫边界后的引导服务路径 / 分配 / 释放 / 析构，带两枚注入负控），缺口因此是「文本级而非类型级、且不覆盖重试轮内部」，不是「什么都没有」（§3.4）
- `os/boot-shim/src/ebs_pair.rs` 的退出状态分类 —— **本方第九轮核出本文与代码的位置关系与预想相反**：循环体按「成功 / 参数错误重试 / 其它立即失败」三路分派，重试总次数由一个常量限制（读数为 2），文件头注释已把这三条列为自身执行的硬合同；§3.4 本轮补的状态码表因此是**把已有实现写进文档**，不是新政策（现量命令：`grep -n -e EBS_ATTEMPT_LIMIT -e ExitStatus os/boot-shim/src/ebs_pair.rs`）
- `os/boot-shim/src/uefi_helpers.rs:exit_boot_services` —— **本方第九轮核出的现存缺陷第五条**：路径 A 那条封装把底层退出调用的返回值整体丢弃，不匹配也不检查，失败与成功在调用方看来一样；调用者为 `prepare_boot` 与 22 个测试内核入口（现量命令：`grep -n 'fn exit_boot_services' -A 10 os/boot-shim/src/uefi_helpers.rs` 看它有没有对结果做判断）
- `os/libs/minix-boot/src/memmap_source.rs:clip_reserved` —— 「固件常驻区」当前的形状：**一个上界**，只能裁掉低地址前缀（§4.3 那条局限的依据）
- 交接结构里的 `reserved_regions` 字段（`os/libs/minix-boot/src/handoff_v1.rs`，由 `os/boot-shim/src/uefi_helpers.rs:build_memmaps` 产出、`os/kernel/src/lib.rs` 解码）—— **本方核实它是分类器的输出，不是规划阶段的输入**：生产者就是本仓自己的 UEFI 分类路径，没有任何外部来源往它写保留区间。因此 §0.3 数据流里那个「启动信息保留输入」是本文自己画出来的 phantom（现量命令：`grep -rn 'reserved_regions' os --include=*.rs | grep -v '^os/target'`，逐条看写入点）
- `os/boot-shim/src/opensbi_helpers.rs`、`os/kernel-image/src/bootface.rs` —— 两条裸机腿传入该上界（引导壳那条传零 = 不裁剪）
- 依赖库 `fdt`（`os/Cargo.toml` 钉死为 `=0.1.5`）的 `Fdt::memory_reservations` —— 设备树自身保留映射块的遍历接口；**本仓零调用点**（现量命令：`grep -rn 'memory_reservations' os --include=*.rs | grep -v dockercargo`）

**引导壳的固件配置表**

- `os/boot-shim/src/uefi_helpers.rs` 里取设备树配置表项的一段（平台描述来源：ARM64 优先设备树、否则退回 ACPI）—— §4.4.1「换输入」可复用的现成来源
- `os/boot-shim/src/ebs_pair.rs:pair_and_classify` 的重试循环顶部 —— 每轮先清空交接区的两份映射分区（对应 §3.3 问题 C 所说的运行时重置）

**依赖库与规范出处**

- `uefi-raw-0.9.0/src/table/boot.rs:MemoryAttribute` —— 与 `ebs_map.rs:attr` 逐位比对（§7.1）
- `uefi-raw-0.9.0/src/table/revision.rs:EFI_2_100` —— **本方核实的选档反证**：依赖库的修订常量最高就是 `2.100`，且其文档断言把它字符串化成 `"2.10"`，因此「读修订字段即可选档」在本仓连 2.11 的对照常量都不存在
- `uefi-raw-0.9.0/src/table/boot.rs:MemoryAttribute` 里没有「可热插拔」位 —— 本方核实（现量命令：`grep -c 'HOT_PLUG' uefi-raw-0.9.0/src/table/boot.rs` 读数为 0），这是 §2.5 把该位定性为**档外扩展位**而不是「本档已知位」的依据
- 系统表的厂商身份字段（`FirmwareVendor` / `FirmwareRevision`）—— 本仓**没有任何读取点**，现量命令：`grep -rn -e FirmwareRevision -e fw_vendor -e firmware_vendor os --include=*.rs`；白名单条目需要的数据来源因此也还不存在
- `uefi-0.33.0/src/boot.rs` 的退出函数文档 —— 「引导服务的两类数据会变成空闲内存」与「退出前须清理会调用释放的对象」清单（行 5 依据）
- UEFI 2.10 规范「Memory Attribute Definitions」（`07_Services_Boot_Services` 一节）、EDK2 `MdePkg/Include/Uefi/UefiSpec.h`、Linux `include/linux/efi.h` —— §7.1 的三处独立取值出处
- 设备树规范 `/reserved-memory` 与 `no-map` 相关小节 —— §4.2–§4.6 引用的规范原文（本方未取得，**[待验证]**）

**设计讨论与台账**

- `rewrite-notes/01-stage-kernel/01-design-note-memmap-consolidation.md` §2.5.6、§2.5.7、§2.6、§2.7、§5、§7
- `rewrite-notes/01-stage-kernel/todo.md` §7.4、`01-stage-kernel/01-todo.md` §3

---

---

## 一b、落码证据：设备树保留来源（提交 `38668f843`）

| 项 | 读数 | 命令 |
|---|---|---|
| 静态保留声明扣除 | `dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables` | `cd os && cargo test -p minix-boot --lib` |
| 前缀去重 / 越界声明 / 保留映射块 | `dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free` | 同上 |
| 拒绝形状（重叠 / 回绕 / 宽 cells / 容量） | `dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`、`dtb_more_than_capacity_is_rejected_not_truncated` | 同上 |
| 引导区重叠守卫 | `test_bootstrap_outside_memmap_rejects_wrapping_region`、`_allocation` | `cd os && cargo test -p boot-shim --lib` |
| 三架构编译 | x86_64 / aarch64（`fw-none-image`，另加 `-C target-feature=-neon,-fp-armv8`）/ riscv64 | `cargo check -p kernel-image --target <triple> --features fw-none-image`；`cargo check -p boot-shim --target riscv64gc-unknown-none-elf` |
| 真实固件引导 | riscv bootface QEMU PASS，memmap 行不变 | `bash os/qemu-tests/test-kernel-image-riscv64-bootface.sh` |

**边界（写进证据，不让它被读大）**：QEMU/OpenSBI 的运行时 DTB 未在固件前缀之上暴露保留声明，因此该次 QEMU 运行只证明「接入不改变既有引导」；扣除行为本身由上述单元测试覆盖，真实板卡的保留声明样本仍待取得。

## 一c、第二十七轮新增代码读数（正文合同的事实锚）

| 正文主张 | 读数 | 现量命令 |
|---|---|---|
| AArch64 锁定 4 KiB granule | `TCR_EL1` 的 `T0SZ=16`、`TG0/TG1=0`；`PhysFrame::SIZE = 0x1000` 且有编译期断言 | `grep -n -e TCR_T0SZ -e TG0 -e "assert_eq!(PhysFrame::SIZE" os/arch/src/arm64/paging.rs os/arch/src/arch/frame.rs` |
| raw 端口 I/O 只在包装层 | 5 个文件：`arch/src/x86_64/clock.rs`、`boot-shim/src/lib.rs`、`plat/src/x86_64/port_io.rs`、`interrupt.rs`、`early_console.rs`；`kernel/`、`servers/` 零命中 | `grep -rn -e 'asm!("in ' -e 'asm!("out ' os --include=*.rs`（逐条看路径，不要只看计数） |
| `PERSISTENT` 落诊断成员 | `ty::PERSISTENT` 与 `ACPI_NVS` 同测为 `Disposition::Side` | `grep -n -B 4 -A 2 ty::PERSISTENT os/boot-shim/src/ebs_map.rs` |
| 诊断成员完整存储、超限拒绝启动 | `SIDE_CAP = 64`、`MapError::TooManySideEntries` | `grep -n -e SIDE_CAP -e TooManySideEntries os/boot-shim/src/ebs_map.rs` |
| 设备窗口政策是静态表 | `identity_device_windows() -> &'static [(u64, u64)]` | `grep -n -A 3 "fn identity_device_windows" os/kernel/src/lib.rs` |

## 一d、第二十八轮事实表：设备树各来源的实现状态

| 来源 | 现量命令 | 读数 |
|---|---|---|
| `/memory` | `grep -n "for node in self.fdt.all_nodes()" -A 20 os/libs/minix-boot/src/memmap_source.rs` | 第 1 段读 `reg`，unit-address 与多段都覆盖 |
| `/reserved-memory` | `grep -n "fn lookup_reserved_memory" -A 8 os/libs/minix-boot/src/memmap_source.rs` 与第 3 段 | `find_node` + 可见性交叉检查；`reg`、`no-map`；`reusable` 未区分 |
| MRB | `grep -n "memory_reservations" os/libs/minix-boot/src/memmap_source.rs` | 第 2 段；库处理 `{0,0}` 终止 |
| UEFI 配置表 DTB | `grep -n "DEVICE_TREE_GUID" -A 6 os/boot-shim/src/uefi_helpers.rs` | 只把条目地址登记为 `PlatformDescSource::DTB`，不解析不复制 |
| 三层状态 | 解析层：前三行具备；转换层：只有可用表/保留表；接入层：未做 | 由代码结构直接读出，不靠字符串命中 |

## 一e、第二十九轮 as-built 读数（主文档重写的依据）

| 主张 | 读数 | 命令 |
|---|---|---|
| 只有一棵项目根 | `establish_boot_dm` 注释：`additive on the active bootstrap root`，只写新 PTE | `grep -n "additive on the active bootstrap root" -A 3 os/kernel/src/dm_coverage.rs` |
| DM 候选并集 | `memmap ∪ bootstrap tree ∪ boot modules ∪ reserved`（源-4 并集过滤） | `grep -n "memmap . bootstrap tree" -A 3 os/kernel/src/dm_coverage.rs` |
| 无禁止集合、无审计遍历 | `grep -rn "Deny" os/kernel/src os/arch/src` 只命中权限/能力语义；`grep -rln "fn audit"` 为空 | 同左 |
| 交接容量 | 可用表 128、保留表 256 | `grep -n -e WIRE_MEMMAP_CAP -e WIRE_RESERVED_CAP os/libs/minix-boot/src/handoff_v1.rs` |
| 诊断成员完整存储且失败关闭 | `SIDE_CAP = 64` + `TooManySideEntries`；`types_that_earn_no_ordinary_mapping_go_to_the_side_list` | `grep -n -e SIDE_CAP -e TooManySideEntries os/boot-shim/src/ebs_map.rs` |

## 一f、第三十一轮代码切片读数（`/reserved-memory` 形状校验）

| 主张 | 读数 | 命令 |
|---|---|---|
| 父节点 `ranges` 必须存在且为空 | `ReservedMemoryRangesNotEmpty`；`dtb_reserved_memory_without_empty_ranges_is_rejected` | `grep -n "ranges_empty" -A 4 os/libs/minix-boot/src/memmap_source.rs` |
| `no-map` 与 `reusable` 互斥 | `ConflictingReservationAttributes`；`dtb_reservation_with_no_map_and_reusable_is_rejected` | `grep -n "ConflictingReservationAttributes" -A 3 os/libs/minix-boot/src/memmap_source.rs` |
| 采集器测试与包读数 | 22 条采集用例；`cargo test -p minix-boot --lib` 77 通过 | 同左 |
| 真实固件 | riscv bootface QEMU PASS（OpenSBI DTB 的 `ranges` 为空） | `bash os/qemu-tests/test-kernel-image-riscv64-bootface.sh` |

## 一g、第三十二轮代码读数

| 主张 | 读数 | 命令 |
|---|---|---|
| 标记属性必须为空 | `NonEmptyReservationAttribute`；`dtb_reservation_with_nonempty_marker_is_rejected` | `grep -n "NonEmptyReservationAttribute" os/libs/minix-boot/src/memmap_source.rs` |
| cells 与根一致 | `dtb_reserved_memory_cells_must_match_root` | 同文件 `root_cells` |
| UEFI 磁盘 DTB 只作诊断 | `report_memmap_source` 不再 panic，打印 `diagnostic only`；`prepare_boot` 同样降级 | `grep -n "diagnostic only" os/boot-shim/src/uefi_helpers.rs` |
| UEFI 目标不再漏检 | `cargo check -p boot-shim --target aarch64-unknown-uefi` / `x86_64-unknown-uefi` 均通过 | 同左 |
| aarch64 UEFI 启动门 | `test-shim-bootmarks-aarch64` PASS（MARK3 已同步为诊断行） | `bash os/qemu-tests/test-shim-bootmarks-aarch64.sh` |

## 一h、第三十三轮代码读数

| 主张 | 读数 | 命令 |
|---|---|---|
| MRB 条目重叠拒绝 | `OverlappingMemoryReservations`；`dtb_overlapping_memory_reservations_are_rejected` | `grep -n "OverlappingMemoryReservations" os/libs/minix-boot/src/memmap_source.rs` |
| MRB 扫描上限 | `DTB_MRB_MAX = 16`；`TooManyMemoryReservations`；`dtb_too_many_memory_reservations_are_rejected` | 同左 |
| 证据基线 | 提交 `413ff9265`；`git rev-parse HEAD` 与 `git status --short` 同批核对 | `cd os && git rev-parse HEAD` |

## 一i、第三十四轮代码读数

| 主张 | 读数 | 命令 |
|---|---|---|
| `status` 准入 | `UnsupportedMemoryNodeStatus`；`dtb_disabled_memory_node_is_rejected` | `grep -n "UnsupportedMemoryNodeStatus" os/libs/minix-boot/src/memmap_source.rs` |
| `hotpluggable` 准入 | `HotpluggableMemoryUnsupported`；`dtb_hotpluggable_memory_is_rejected` | 同左 |
| UEFI 无热插拔位定义 | `grep -rn HOT_PLUG uefi-raw-0.9.0/src` 零命中 → 该位走未知属性拒绝 | `grep -rn HOT_PLUG /home/xzhao/.cargo/registry/src/*/uefi-raw-0.9.0/src` |

## 一j、第三十五轮代码读数

| 主张 | 读数 | 命令 |
|---|---|---|
| 零长度 MRB 条目拒绝 | `ZeroLengthMemoryReservation`；`dtb_zero_length_memory_reservation_is_rejected` | `grep -n "ZeroLengthMemoryReservation" os/libs/minix-boot/src/memmap_source.rs` |
| 热插拔属性位负控 | `hot_pluggable_bit_fails_closed_as_unknown`（`WB \| 0x400` → `UnknownAttribute`） | `grep -n "hot_pluggable_bit" os/boot-shim/src/ebs_map.rs` |
| 静态设备窗口 | LAPIC `0x0FEE_0000+4K`、IO-APIC `0x0FEC_0000+4K`、平台 MMIO `0x0800_0000+32M`、PLIC `0x0C00_0000+2M`、UART `0x1000_0000+4K` | `grep -n -A 4 "kernel_identity_mmio_regions" os/arch/src/lib.rs`；`grep -n -e MMIO_PA_BASE -e MMIO_WINDOW_LEN os/libs/minix-platform/src/arch/aarch64.rs` |

## 一k、第三十六轮读数（PLIC 与属性位）

| 主张 | 读数 | 命令 |
|---|---|---|
| QEMU PLIC 区间 | `plic@c000000` 的 `reg = <0x00 0xc000000 0x00 0x600000>`（smp=1） | `timeout 10 qemu-system-riscv64 -machine virt -smp 1 -m 256M -display none -bios default -machine dumpdtb=/tmp/p1.dtb` 后 `dtc -I dtb -O dts /tmp/p1.dtb` |
| PLIC context 起点 | 规范布局 `+0x20_0000`，每 context 4 KiB；S 模式 context = `0x20_0000 + (2N+1)*0x1000` | QEMU `hw/riscv/virt.c` 的 PLIC 布局（上游） |
| 窗口修正 | `PLIC_MAX_HARTS = 32`、`PLIC_WINDOW_LEN = 0x20_0000 + 2*32*0x1000` | `grep -n -A 4 "PLIC_MAX_HARTS" os/arch/src/lib.rs` |
| 热插拔位值 | `EFI_MEMORY_HOT_PLUGGABLE = 0x0010_0000`（EDK2 `MdePkg/Include/Uefi/UefiSpec.h`） | `grep -n "0x0010_0000" os/boot-shim/src/ebs_map.rs` |

## 一l、第三十七轮读数（PLIC 包络）

| 主张 | 读数 | 命令 |
|---|---|---|
| 设备窗口按 2 MiB 片映射 | `DEV_PIECE = 1 << 21` | `grep -n "const DEV_PIECE" os/kernel/src/lib.rs` |
| PLIC 资源 0x24_0000、包络 0x40_0000 | `PLIC_RESOURCE_LEN` / `PLIC_WINDOW_LEN` 与两条编译期断言 | `grep -n -e PLIC_RESOURCE_LEN -e PLIC_WINDOW_LEN os/arch/src/lib.rs` |
| 包络仍在 QEMU aperture 内 | `reg` 为 `0xc000000 + 0x600000` | `dtc -I dtb -O dts /tmp/p1.dtb`（见 一k） |

## 一m、第三十八轮读数（DTB 首访与 fdt 库边界）

| 主张 | 读数 | 命令 |
|---|---|---|
| 指针 8 字节对齐 | `dtb_blob_aligned` + 两个宿主测试；三处 boot 面在首次解引用前检查 | `grep -n "dtb_blob_aligned" os/libs/minix-boot/src/lib.rs os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs os/boot-shim/src/opensbi_helpers.rs` |
| totalsize 上限 | `dtb_len_ok`（40 B–64 MiB）；OpenSBI 腿另加 checked 加法 | `grep -n -e dtb_len_ok -e checked_add os/boot-shim/src/opensbi_helpers.rs` |
| `fdt 0.1.5` 的校验范围 | `Fdt::new` 仅查 magic 与 `data.len() >= totalsize`；`memory_reservations()` 直接按 `off_mem_rsvmap` 切分，无对齐/偏移边界检查 | `sed -n "175,205p" /home/xzhao/.cargo/registry/src/*/fdt-0.1.5/src/lib.rs`；`sed -n "243,260p" .../fdt-0.1.5/src/lib.rs` |

## 一n、第三十九轮读数（FDT 容器与设备资源）

| 主张 | 读数 | 命令 |
|---|---|---|
| FDT 容器校验 | `fdt_layout::validate` + 10 条测试（对齐/越界/缺终止项/截断/重叠） | `cd os && cargo test -p minix-boot --lib fdt_layout` |
| 五处接入 | riscv/aarch64 bootface、OpenSBI、aarch64 UEFI 两处堆副本 | `grep -rn "fdt_layout::validate" os --include=*.rs` |
| UART 资源范围 | `serial@10000000` 的 `reg = <0x00 0x10000000 0x00 0x100>`（256 B） | `dtc -I dtb -O dts /tmp/p1.dtb` |
| PLIC 资源与中断 | `reg = 0xc000000 + 0x600000`；`interrupts-extended = <0x02 0x0b 0x02 0x09>` | 同上 |

## 一o、第四十轮读数（容器版本与块顺序）

| 主张 | 读数 | 命令 |
|---|---|---|
| FDT 版本策略 | 只接受 `version = 17`、`last_comp_version ≤ 17`；`rejects_unsupported_fdt_versions` | `grep -n -e UnsupportedVersion -e FDT_VERSION os/libs/minix-boot/src/fdt_layout.rs` |
| 项目块顺序 | MRB 必须位于 structure 之前；`rejects_a_layout_whose_mrb_follows_the_struct_block` | 同上 |
| MRB 不落入 header | `rejects_an_mrb_that_starts_inside_the_header` | 同上 |
| 真实固件 | QEMU/OpenSBI 生成的 DTB 为 v17/last_comp 16，两个 QEMU 腿 PASS | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh`、`test-shim-bootmarks-aarch64.sh` |

## 一p、第四十一轮读数（FDT 版本子集与结构块边界）

| 主张 | 读数 | 命令 |
|---|---|---|
| 版本子集 | `version = 17`、`last_comp_version = 16`；`rejects_unsupported_fdt_versions` | `grep -n -e FDT_VERSION -e FDT_LAST_COMP_VERSION os/libs/minix-boot/src/fdt_layout.rs` |
| 结构块对齐与 END | `off_dt_struct % 4 == 0`、末 4 字节为 `0x9`；两条测试 | 同上 |
| 完整块顺序 | `MRB → structure → strings`；`rejects_strings_before_the_structure_block` | 同上 |
| 真实固件 | QEMU/OpenSBI DTB 为 v17/16、结构块 4 字节对齐且以 END 结尾，两个 QEMU 腿 PASS | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh`、`test-shim-bootmarks-aarch64.sh` |

## 一q、第四十二轮读数（预扫尝试与容器收口）

| 主张 | 读数 | 命令 |
|---|---|---|
| MRB 条目上限在扫描函数内 | `MRB_MAX_ENTRIES = 16`、`TooManyReservations`；`rejects_more_reservations_than_the_collector_accepts` | `grep -n -e MRB_MAX_ENTRIES -e TooManyReservations os/libs/minix-boot/src/fdt_layout.rs` |
| strings 块非对齐可接受 | `accepts_an_unaligned_strings_block` | 同文件 |
| 预扫的失败形态 | riscv bootface `fault_fetch`，`cause=1`，`epc=0xffffffc000032330`（`validate_structure` 的 `jr`） | `qemu-system-riscv64 ... -d int -D /tmp/q.log` 后看 `riscv_cpu_do_interrupt` |
| 根因 | `riscv64-linux-gnu-objdump` 显示 token 分派被编译为 `.rodata` 跳转表 + `jr`；该阶段执行的映射不覆盖跳转表绝对地址 | 同左 + `nm \| grep switch.table` |

## 一r、第四十三轮读数（跳转表禁用与预扫落地）

| 主张 | 读数 | 命令 |
|---|---|---|
| 选项拼写 | rustc 1.94.1 提供 `-C jump-tables=val`（默认 yes），没有 `no-jump-tables` | `rustc -C help \| grep jump` |
| 构建注入 | xtask 对 kernel-image 的 aarch64/riscv 注入 `RUSTFLAGS`，现含 `-C jump-tables=no` | `grep -n "jump-tables=no" os/xtask/src/image.rs os/.cargo/config.toml` |
| 机器码证据 | `validate_structure` 反汇编只剩直接 `jalr`，无表驱动 `jr` | `riscv64-linux-gnu-objdump -d --start-address=<fn> ...` |
| 反面证据 | 去掉选项时 bootface `fault_fetch`（cause=1，`epc` 在 `validate_structure` 的 `jr`） | 上一轮 QEMU `-d int` 读数 |
| 引导门 | riscv bootface 与 aarch64 UEFI 均 PASS | `os/qemu-tests/test-kernel-image-riscv64-bootface.sh`、`test-shim-bootmarks-aarch64.sh` |

## 二、升级为实现合同所需的材料包（原正文第十三节）

| 包 | 内容 | 状态 |
|---|---|---|
| 一 · 属性配置档与三架构证据 | 逐位常量与允许掩码的**出处**（出版物标识 + 日期 + 段落/头文件）、固件身份与表头修订字段的读取（两者作白名单佐证，都不能选档；描述符结构版本本仓已在校验，缺的是把它一并记录）、实际固件的原始属性样本、x86 页表旗标转换与从处理器入口、AArch64 两棵树的叶子构造路径、RISC-V 平台清单与**分区原子能力** | 常量比对已做，但其结论按第四轮降级为「缺出版物标识」（§7.1）；本仓常量组合按第五轮定名为有身份的实现兼容档（§2.5）；固件身份与表头修订值的读取、原始样本、平台清单三项是本仓缺口（描述符版本的**记录**也是，校验不是） |
| 二 · 路径 A 的实际接线 | 封装返回快照后的真实调用顺序；输出存储区声明与地址；内核从接收交接数据到切页表的代码；启动对象映射来源与覆盖验收 | 已确认「快照被丢弃」「恒等段无差别覆盖」两处（§3.1、§0.3）；其余接线时取 |
| 三 · 实际设备树 | 各设备树引导腿（含 aarch64 的 UEFI 腿那份磁盘镜像快照）实际收到的设备树或其反编译文本、设备树自身的保留映射块条目、设备树收集器解析代码、直接映射候选范围来源清单 | 解析行为已核（只解 `/memory` 加前缀裁剪，未解保留子树）；三条腿的实际形态未取 |
| 四 · 诊断与判定接口 | 样本容量与写入接口、预检与写入接口、三架构串口实现、判定脚本识别通过/失败的部分 | 分类接口与容量已核；串口与判定脚本部分未取，是 §3.3 与 §5 两张表定稿的必要输入 |

【本段是第四轮的判断，已被第五至第八轮逐层改写；现行范围见本节末尾与 §8 的第八轮阻断表】本轮最该先解决的两件事，本方当时与第四轮判断一致：**属性位的配置档白名单与实际固件样本**（否则行 1 与逐位表都建立在未门控的属性字上），以及**设备树保留来源的消费**（它直接改变可用内存集合，且本仓已有一条 UEFI 腿同时握着两份输入）。裁决六的那几个集合对象与这两件事同批设计——它们决定要传哪些范围、要多大容量、按什么粒度兑现，以及 L3 的验收怎么跑。

**第五至第七轮的判断修正与取证表已移入档案第五节**；本节只留现行材料包与下面的取证项。
**第八轮追加的取证项**（评审人说这一轮的重心是「把合同变成能被代码与测试证明的东西」；标注「本方已核」的行给出现量命令）：

| 取证项 | 归属 | 判据 |
|---|---|---|
| 被类型强制排除的范围产生完整区间 | 引导壳分类器 | 本方已核现状：三处空分支（预检计数、写入分发、提交落盘）都不发区间，且那两条去向连长度都返回零——现量命令 `grep -n 'Disposition::Excluded' os/boot-shim/src/ebs_map.rs`（三处命中即预检计数、写入分发、提交落盘那三个空分支）与 `grep -n 'Disposition::Excluded, 0' os/boot-shim/src/ebs_map.rs`。验收：固件保留与不可用两类各构造若干描述符，断言其区间出现在 `TypeExcludedRanges` 而不是只出现在计数或可截断样本里 |
| **来源完整性测试**作为独立一组 | 区间规划阶段 | 与 R1–R13 的关系测试分开跑。判据形状由评审人给：每种来源的每个有效输入条目，要么被映射进某个完整政策集合，要么被显式拒绝。本方特意写一条反例用例：**故意漏收不可用类型的实现必须仍然通不过这一组**，而它能通过当时那十条等式（R5b 分区式与 R11–R13 正是为了堵这类漏收而加） |
| 四维映射验收在两棵树上分别执行 | 内核架构层 + 引导壳 | 每个登记对象断言四件事：每一页有映射；访问所用的虚拟地址关系与登记的路径种类一致；叶子权限满足登记的最小权限且不超出允许组合；属性索引与共享性符合平台合同且与同页其它别名一致。评审人的那一例要成为一条具名用例：物理页已映射但路径与权限错误，**断言这条在覆盖检查里失败** |
| 栈的物理范围来源 | 引导壳 + 内核 | 本方已核：交接结构里栈那一格是虚拟地址类型、无物理栈底字段（现量命令 `grep -n 'kern_stack_top' os/libs/minix-boot/src/handoff_v1.rs`）。验收要证明范围取自分配点或已有页表映射并验证过覆盖完整，**断言不能由加载器数据类型加内存图快照反推**——一条描述符可能覆盖多个对象 |
| 设备树复制后的缓冲区、登记表的存储、两块静态区间数组 | 引导壳 | 三者各成一个登记对象并给出物理范围与最后使用阶段；登记表自身存储若与规划工作区共用，要有明确的包含关系证明。本方已核一处相关读数（不是缺陷）：设备树解析返回的区间表指向静态数组 `DTB_FREE`/ `DTB_RESERVED` 而非局部缓冲，故局部缓冲析构不影响其有效性 |
| 引导壳中止路径的可达阶段用控制流确定 | 引导壳 | 本方已核三处读数（进入内核的函数返回不返回型、页表切换在该函数内部、切换后同段仍有一处会中止的取值）。现量命令 `grep -n 'build_bootstrap_root_and_enable' os/kernel/src/lib.rs` 看它相对调用点的位置。验收要按阶段而不是按「是否登记」决定映射要求 |
| `CR0.CD / CR0.NW` 逐核核对 | 内核架构层 | 本方已核：全仓 5 处写 `CR0` 全是读改写自己关心的位（PE/PG/TS/MP/NE/EM/WP），无一处读或清这两位。**现量命令与读数要一起给**：`grep -rn 'mov cr0' os/arch --include=*.rs` **命中 6 行**，其中一行是 `os/arch/src/arch/paging.rs` 里描述切换顺序的文档注释，**真实写点 5 处**（浮点初始化两处、分页启用一处、从处理器早期入口两处）。这是本轮那条「计数类断言不能只看计数」的第二处现场——本方第九轮先前的「5 处」是靠逐文件数出来的，若有人照那条命令数出 6 并据此改政策，就会把一个注释当成一次写入。补一条从处理器入口与主处理器各一次的读回断言，与 `PAT`、MTRR 一致性同批 |
| 来源收集缓冲超限改为拒绝启动 | 设备树来源 + 引导壳 | 本方已核现状为静默丢弃（8 格上限用满后两处写入点都没有 `else`）。判据按评审人收紧：影响准入、PMM 排除或映射禁止的缓冲超限一律拒绝启动，只打诊断不算；诊断样本仍允许截断 |
| 方案 B 的构建期绑定与外部清单 | 测试设施 + 引导壳 | 每个受支持构建产物绑定一个配置档；流水线记录产物摘要与环境清单的绑定关系；引导壳只使用绑定的那张表。边界要写进输出：引导壳能因属性组合违反绑定档而失败，**不能**声称已验证固件身份。评审人列的旧任务（读取厂商字段以建立白名单条目、登记项上限为 1）本轮已作废 |


**第十轮追加的取证项**（他给的下一轮范围是「围绕四条数据流逐条对照：谁生产、保存在哪、谁消费、容量如何预检、超限怎样失败、用什么测试证明完整」；本方按这四条组织下面的行）：

| 数据流 | 取证项 | 判据（本方按代码读数写） |
|---|---|---|
| 来源 → 统计范围 → 排除 → 可用 | **假闭合必须有一条反例用例** | 构造一条形状：某条 `RESERVED` 或 `UNUSABLE` 描述符的范围**没有**进入 `TypeExcludedRanges`（模拟实现把它在构造统计域时就减掉了），断言分区式 R5b 与来源完整性测试**同时失败**。本方本轮把构造式改成两层，正是为了让这类「先减再算」的形状可被抓到——第九轮的写法下它会通过 |
| 来源 → 统计范围 → 排除 → 可用 | `DeviceTreeReservedRanges` 的两路来源都要出现 | 每个输入区间断言其来源标签（文件头保留映射块 / `/reserved-memory` 子节点 `reg`）在完整集合与诊断里都保留；合并或截断导致标签丢失时容量预检要失败 |
| 设备树保留 → 两个禁止集合 → 两棵树 | `FinalMapDenyRanges` 的成员与传递 | 三条用例：成员逐项符合§0.3 那张五项构造式（`ReservedMappedRanges` 与启动对象页**必须不在**其中）；R15「最终根实际映射 ∩ L2 禁止 = ∅」在真实页表遍历上执行；R14 在候选来源重新引入一页时必须失败 |
| 设备树保留 → 两个禁止集合 → 两棵树 | 设备树复制链路的端到端读数 | 本方已核现状：无复制、内核用原始地址（现量命令见 §10）。落地后的验收：引导壳复制完成 → 两个消费者都读**同一份复制目标** → 原始地址不再出现在任何跨边界记录里；并断言「若内核仍用原始地址而该地址进了禁止集合，启动必须失败」 |
| 登记表 → PMM 保留 / 映射义务 ABI → 实际叶子 | 对象目录的每一块存储都有对应条目 | 按 §6.8 那张三类目录逐条断言：物理范围可追溯、最后使用阶段明确、在两棵树上的访问方式各自登记。**本方特意加一条防错断言**：目录里出现「设备窗口」类条目时，必须走独立设备映射义务而不是登记表——防止本方本轮刚分开的两类又被塞回一张表 |
| 登记表 → PMM 保留 / 映射义务 ABI → 实际叶子 | R13a–R13e 各一条独立失败的用例 | 每个子判据一条：R13b 用「映射存在但虚拟关系错」的形状（评审人第八轮给的那一例）；R13c 用「要求 `RX` 给了 `RWX`」；R13e 用「同页一个只读可执行、另一个可写」。断言**逐条失败可定位**，不允许一条大用例覆盖五个子判据 |
| 配置表 DTB 原始地址 → 复制目标 → 全部消费者 | 三条腿的配置表条目清单要成为可复现读数 | 把条目数量、两个 DTB GUID 的存在性、被选中的那一项的校验值打进串口输出，**把仓库注释里那条历史实测换成当场读数**，再决定设备树保留解析在第一版有没有输入 |
| 跨架构阶段切换 | 两次切换各一条用例 | 构造「在地址转换同步序列之内访问一个只在新根下有映射的对象」，断言失败；四档不因此增加。过渡窗口的双侧覆盖只针对登记过义务的对象，禁止集合在两侧都要成立 |
| 类型 11 / 12 | 固件样本要坐实两条处置 | 类型 12 第一版拒绝启动的前提是「三个目标架构的实际固件不输出它」——本方核出这一条**没有样本证据**，故列为取证门；样本若显示存在输出，本条改判「域外完整保留」，且那次改动必须带样本出处 |
| 架构语义 | 原子准入表要挂在代码生成上 | 断言登记项来自**最终指令序列**（LR/SC 循环 / `AMOCAS.D` / Zabha 原生 / 模拟 / 锁回退），不是来自 `compare_exchange` 这个 API 名；内核侧现有 122 处用法先做归类，归类不全的平台不得进入「已验证」档 |

**第九轮追加的取证项**（评审人这一轮要求「不再做泛泛审阅，改为围绕材料做合同逐条对照」；本方把每行写成可判定形状，标注「本方已核」的给出现量命令）：

| 取证项 | 归属 | 判据 |
|---|---|---|
| 统计范围按启动方式各自构造，且分区式可测 | 区间规划阶段 | 两条腿各写一条构造断言：UEFI 腿的统计范围由固件内存图与前缀阈值形成、裸机腿由设备树 RAM 候选形成，**两者不得共用一句「由两个区间来源共同形成」**。验收要能抓住本方与评审人共同给出的那个反例：一个有效输入区间既没进可用表也没进任何排除集合时，分区式必须失败（当时那十条等式全通过）。另加两条按来源分的用例：**内存类登记对象越出统计范围必须拒绝启动**，**设备地址类越出统计范围不算矛盾**（它只承担映射义务，改由设备窗口政策校验其授权来源）。本方是先写了前者、再按中止路径的追溯要求核出串口寄存器窗口，才发现只写前者会误判合法设备地址——两条都要有用例 |
| 两个派生集合的分叉要可复现 | 区间规划阶段 + 引导壳 | 用例形状：一个对象其最后一次使用发生在阶段 p 之前（原始设备树缓冲区在复制完成后即不再需要映射），断言它**已不在** p 阶段的映射义务集合、**仍在** p 阶段的 PMM 保留集合。本方特意选这个例子，因为它就是本仓今天真实存在的形态；若实现把两件事压回一个集合，这条用例会以「义务集合非空」或「保留集合变小」的形式失败 |
| L2 完整禁止集与最终根覆盖 | 内核 + 区间规划阶段 | 三条分别成用例：`BootMapDenyRanges ⊆ FinalMapDenyRanges`；最终直接映射遍历结果与禁止集**不相交**（不是「候选来源恰好没包含」）；「允许映射但不得分配」的保留区在最终根里**真的有**叶子。并锁掉本仓已记录的那个缺口：一条保留描述符同时跨越候选对象与无关页时，尾部必须有叶子——这条按本方读数仍是未闭合的区间减法（§10 那条来源说明），验收即「缺口被消掉」，不是「在旁边再加一份来源」 |
| 阶段转换表与切换时点归属 | 引导壳 + 内核 | 表按评审人的判据落成五列：阶段名 / 进入条件 / 退出事件 / 当前活动的页表树 / PMM 是否可用。验收断言归属**按根寄存器实际写入的时点**而不是函数返回：同一个对象在切换前后都被访问时，两侧都要登记义务。本方已核的三处读数（进入内核的函数返回不返回型、切换发生在其内部、切换后同段仍有一处会中止）就是第一条用例 |
| 设备树来源身份 | 引导壳 | 本方已核现状：磁盘那份只解析 `/memory`（`grep -rn 'DUMPDTB_PATH' os/boot-shim/src`），配置表那份引导壳只登记地址、从不解析（`grep -n 'DEVICE_TREE_GUID' os/boot-shim/src/uefi_helpers.rs`）。验收断言 `/reserved-memory` 与 `no-map` 的政策输入**只来自被认定的那一份**，并给出该份在第一次退出尝试之前的复制完成点；另一份若被读进任何集合即失败 |
| 权限精确、页级冲突与别名权限 | 内核架构层 | 四类用例：登记 `RX` 的叶子给成 `RWX` 必须失败（不是「上界内即可」）；同一 4 KiB 页里两个对象分别要求 `RX` 与 `RW` 时，归并到页级后必须有一条被拒；直接映射别名的偏移计算溢出必须失败而不是回绕；同页一个别名只读可执行、另一个可写时整体判定失败——**逐条叶子各自合格不等于 `W^X` 成立** |
| 退出状态码分类：补文档与锁死用例，不补实现 | 引导壳 | 本方已核：配对循环三路分派已实现（`grep -n -e EBS_ATTEMPT_LIMIT -e ExitStatus os/boot-shim/src/ebs_pair.rs`）。「其它状态码不得进重试」这条用例**本仓已经有**（模拟固件返回一个非参数错误状态，断言循环立即失败且不再重读图），本方本轮核出后不再把它列为待补；验收因此只剩一条：**路径 A 那条封装不再丢弃返回值**——现状是失败与成功对调用方不可区分（§3.4 现存缺陷第五条），本仓有 22 个测试内核入口走它；那处注释解释的是「返回的内存图为什么可以丢」，**没有解释状态码为什么可以丢**——本方按原样抄下这条区别，免得将来有人拿那句注释当「已经讨论过」的证据 |
| 架构侧四项 | 各架构 + 测试设施 | x86：`CR0.CD / CR0.NW` 任一核不满足即**不通过普通主存合同**，处置写明 fail-closed 且不得靠顺手清位解决；`PAT` 索引位按 4 KiB 与每种实际启用的大页**分别**验收，且做「纯函数索引位 + 真实树遍历得到的叶子属性」两层。RISC-V：平台清单落代码常量表、识别串唯一命中、多重匹配与未登记均 fail-closed；原子登记细化到操作类别 / 宽度 / 对齐 / 排序强度 / 证据来源五项；上界统一为 `effective_pa_limit = min(platform_pa_limit, translation_format_pa_limit)`，本方已核两个常量分别是 48 与 56，正文不得再写成「Sv39 只有 48 位」 |

**第六轮起累积、并由第七、第八与第九轮就地更新的取证项**（这些行仍然有效，只是不再按轮次分表——同一行可能写着第六轮的判据和第八轮的修正）：
| 取证项 | 归属 | 判据 |
|---|---|---|
| 第 4 步的单向一致性校验落地，并锁死方向 | 引导壳分类器 | 宿主用例：运行时服务类型缺写回属性位之外的形状里，「运行时服务类型 + 无运行期位」→ 分类错误；「加载器数据 + 运行期位」→ 诊断成员；「设备窗口 + 运行期位」→ 设备路径。三条用例必须**成组存在**，删掉任一条时另一条要能失败——这是防止有人把判据改回双向的唯一手段 |
| 8 KiB 那条判据：第七轮**撤销**，不再作为取证项 | — | 撤销理由与两处读数冲突登记在 §2.1；评审人核对 UEFI 2.10 与 Errata A 为 4 KiB、8 KiB 属 Itanium 章节，本方检索到的是 2.9A 页面的同一位置却写着 8 KiB。**留一条最小核对项**：将来若有人再主张更严的对齐要求，必须给出出版物标识、章节位置与所属架构分支三项，否则不得进入政策表 |
| 启动对象登记表的第一版落地 | 引导壳 | 登记表在**分类之前**组装完成；条目数与对象身份一一对应；禁止清单自身缓冲区、分类器输出数组、原始设备树缓冲区三项**必须在表内**；宿主用例构造「清单禁掉自己」的形状并断言拒绝启动。现量命令（本方已核）：`grep -n 'fn assert_bootstrap_outside_memmap' -A 12 os/boot-shim/src/uefi_helpers.rs` 说明相交检查这一形状在本仓已有先例，登记表要把它从「一处特判」升为「按登记表遍历」 |
| 十三条集合关系各一条宿主断言 | 区间规划阶段 | 逐条断言，含三条冗余（R6、R7、R8）；冗余条的用途是让「某类范围漏出排除集合」这种缺陷各自有独立的报警用例（§0.3） |
| 相交断言的裸加法改为 checked | 引导壳 | `grep -n 'saturating_add' os/kernel/src/memmap.rs` 保持现状不动（那是它自己的语义）；但 `assert_bootstrap_outside_memmap` 的两处末端计算改为 checked，并补一条构造性用例：末端回绕的区间必须被判为相交 |
| 运行时身份读取**只作诊断**（第八轮之后不再是选档前置） | 引导壳 | 厂商串与厂商版本从「零读取点」变为可读并打进入口诊断（本方已核现状：`grep -rn -e FirmwareRevision -e fw_vendor os --include=*.rs` 读数为 0）；「登记项数量上限为 1」这条随运行时选择器在第八轮作废，**不得再作为前置** |
| RISC-V 一致性/排序两列的证据 | 平台清单 | 每个受支持平台的普通内存候选区间同时登记：是否为一致主存、采用哪种排序模型、内核依赖的排序强度是否被覆盖；缺任一列即该档位从「已验证」降为「已识别但未验证」，第一版不得启动 |
| 三架构的模拟器环境与固件读数 | 测试设施 | 按第六轮材料要求登记机器类型、处理器参数、固件版本与镜像校验值——**作为流水线证据**，不参与运行时匹配（§7.1 的分层）；本方核对：这些字段本仓目前**没有任何一处**集中记录 |

---

## 三、本册的维护约定

1. 正文每轮回填时，新增或改写的**复核锚点**追加到本册第一节，**取证项与材料包**追加到第二节；正文只留指针，不复制内容。
2. 一条锚点被后续轮次否证时，**就地标注推翻理由并保留原读数**，不删除——本册与档案遵守同一条纪律。
3. 本册不参与「能否签字」的判定：评审人签的是正文的合同条款；本册回答的是「这条读数在哪里可以复核、下一批要取什么证据」。
4. 本册创建于正文完成第十轮回填之后，拆出的两节是第十节与第十三节；拆分没有改变任何一条政策判断，正文的编号与交叉引用保持不变。
5. **第十一轮又移入七段**（正文行数上限 1800 行是本方与用户约定的可粘贴边界）：原 §0.2、§1.5、§2.4、§3.1、§5.1、§6.1、§7.2、§9 全部移入本册第三至第八节，正文相应位置留编号与指针。移出的都是**形状事实、机制背景、取证面与词表**，没有一条政策判断、待裁问题或合同条款被移出。
5. **每次拆分后必须做一次逐行核对**：被移出段落的每一个非空行，都要能在目标文件里检索到，命中数为 0 的行即为丢失。本轮核对结果记在下面这行。


**第十一轮追加的取证项**（他这一轮规定只对「公式、集合边界、跨界载体、实际页表结果」四类做逐条审计，本方按四类组织；
每行都要能回答：谁生产、保存在哪、谁消费、容量如何预检、超限怎样失败、用什么测试证明完整）：

| 类别 | 取证项 | 判据 |
|---|---|---|
| 公式 | **来源冲突先于减法**（新 R17） | 构造用例：同一页既是 `/memory` 候选又是设备窗口，断言**拒绝启动**而不是被 `SourceDomain − DeviceAddressRanges` 悄悄抹掉；再用一条形状证明第十轮的写法会通过全部现有等式 |
| 公式 | `ExplicitPolicyDeniedRanges` 的封闭性 | 断言该集合第一版为空；构造一个未登记成员进入它，断言**来源完整性测试失败**——这条用例是防止它变成换名兜底桶的唯一手段 |
| 公式 | `PmmExcludedRanges` **不跨界**的口径一致 | 交接结构里不得出现它的字段；同时断言它在引导壳内有完整存储与容量预检。**本方按他的第 12 项裁定取第二案**，验收就是两处不再互相打脸 |
| 边界 | R15a / R15b 分别取实际页表 | 遍历真实的引导根与最终根：前者与 `BootMapDenyRanges` 不相交、后者与 `FinalMapDenyRanges` 不相交；**被测集合必须是直接映射那一类**，混入设备映射或专用路径即测试写法错误 |
| 边界 | L2 不禁止专用路径的语义边界 | 第一版不提供绕过 L2 的专用路径例外：构造「对象页属 L2 且只有专用路径可访问」的形状，断言**拒绝启动**；将来开这条口子必须同时改 `DedicatedPath` 合同（他第 11 项的告诫） |
| 载体 | 设备映射义务表的完整 ABI | 字段齐全（物理范围、虚拟关系、精确权限、设备内存语义、共享性、授权来源）、固定宽度编码、布局版本、独立容量核算；断言它与映射义务 ABI **不共用余量** |
| 载体 | `FirmwareRuntimeMmioRanges` 与 `DeviceWindowRanges` 不合并 | 构造类型 11 描述符，断言它**不生成**任何驱动映射义务；构造设备窗口来源，断言它不出现在类型 11 的集合里 |
| 载体 | 设备义务也进禁止冲突检查（新 R18） | 让串口寄存器窗口同时落入 `BootMapDenyRanges`，断言拒绝启动——这条正是本方第十轮「串口窗口不属于登记表」那条裁定留下的缺口 |
| DTB | 两条腿各自的权威输入 | UEFI 腿：配置表条目 → 五项可读性与结构校验 → 复制 → 引导壳与内核**都**读复制目标；非 UEFI 腿：`dtb_ptr()` 交付的那份要有自己的范围、最后使用阶段与映射要求，且 DTB 本体落在保留映射块时**读取先于禁止生效** |
| DTB | 覆盖对照与四态退化 | 范围覆盖不完整（规范强制）→ 拒绝；类型不符（本项目加强）→ 拒绝并给出实际/应有类型；absent / 单一 / 重复 / 损坏四态各一条用例，**损坏与不存在的处置不同** |
| 阶段 | 两个状态机的乘积 | 断言 `BootServicesState` 与 `MemoryLifecyclePhase` 是两个独立字段；未登记组合（如已进 S2 却仍 `BootServicesAvailable`）按实现缺陷拒绝；S4 名称与进入条件一致，「服务器就绪」只是事件 |
| R13 | R13a–R13e 独立失败 + 语义层验收 | R13d 测**有效 OS 内存语义**（x86 由 PAT/MTRR 合成、AArch64 由 MAIR 与共享性、RISC-V 由平台 PMA），断言合同里不出现「叶子属性索引」这种架构字段名；R13e 按（物理页, 阶段或转换窗口）归并，`RX`+`R` 允许、`RX`+`RW` 拒绝、跨阶段不冲突、切换窗口内合并判定 |
| 取证 | 三条腿配置表条目清单的可复现读数 | 把条目数量、两个 DTB GUID 存在性、被选条目的校验值打进串口输出，**替换仓库注释里那条历史实测**；读数决定第一版设备树保留解析在 UEFI 腿上是否有输入 |
| 口径 | 「122 行匹配」的正确使用 | 本方第十轮把匹配行数写成了使用点数（他第 28 项）；后续统计原子使用要按调用点或按产物指令清单另行给出，**不得再用行数冒充** |


---

## 二b、第十一轮追加的复核锚点（并入第一节使用）

**第十一轮追加的复核锚点**——本方按他的第 10、11 项自己核对，每条都带现量命令与读数：

- 设备树复制在本仓**不存在**：`grep -rniE 'copy|clone|memcpy' os/boot-shim/src --include=*.rs` 中与设备树相关的命中为 0
- 内核按**原始物理地址**解析配置表那份设备树：`os/libs/minix-platform/src/kind.rs:parse_by_kind` 调
  `DeviceTreeDesc::parse(source.phys_addr())`，其安全注释要求该地址在调用期间保持可读
- 引导壳只登记地址不解析：`grep -n 'DEVICE_TREE_GUID' os/boot-shim/src/uefi_helpers.rs` 命中三行（注释、常量、一次取地址）
- 仓库内一条**待复现**历史实测：同文件 RSDP 分支注释记着 AAVMF「8 条目、ACPI2 在、两个设备树 GUID 都不在」
- 设备树两路保留输入在本仓的**原始来源名还不存在**：
  `grep -rn -e DtbMemoryReservationBlockRanges -e DtbReservedMemoryRegRanges -e AuthoritativeDtbSource os --include=*.rs` 读数为 0
- 本轮新增的公式成员同样不存在：`grep -rn -e ExplicitPolicyDeniedRanges -e PrefixRangeView -e DeviceMappingRequirements -e FirmwareRuntimeMmioRanges -e DeviceWindowRanges os --include=*.rs` 读数为 0
- 最近的同形物（可作改造起点）：`grep -rn identity_device_windows os --include=*.rs` 读数为 5


**第十二轮追加的取证项**（他给出的下一轮顺序是：统计域与域外来源 → 禁止集合与义务关系 → 交接 ABI → 对象登记表 → DTB 数据流 → 文档状态同步）：

| 类别 | 取证项 | 判据 |
|---|---|---|
| 统计域 | 类型 11 描述符的归属 | 构造一条 `EfiMemoryMappedIO` 描述符、其地址不落入任何设备窗口：断言它**被减出统计域**、不进排除侧、也不生成设备义务；再用第十一轮的公式跑一次，断言 `R5b` 在该形状下失败（证明补丁确实必要） |
| 统计域 | R17 兼容矩阵五条 | 每个类别对各一条用例：RAM×设备窗口拒绝、类型 11×RAM 拒绝、类型 11×设备窗口仅允许两种匹配形状、保留声明×设备窗口按具名政策、未登记类别对拒绝；再加一条「部分交叉」被拒的用例 |
| 禁止与义务 | R13-D 与五维共用 | 设备义务故意给错权限（设备页要求不可执行却给了可执行）与给错虚拟关系，断言两条都**独立失败**且失败记录指向设备义务而不是启动对象义务 |
| 禁止与义务 | R18 的路径维度 | 同一条设备义务在 `DeviceWindowAccess` 下通过、在 `FinalDirectMap` 下与 `FinalMapDenyRanges` 相交时失败；再断言一条未登记路径被拒绝而不是默认判据 |
| 交接 ABI | 设备义务表与映射义务表各自容量 | 两张表分别预检、分别失败；断言不存在「共用余量」的分支（构造只放得下其中一张的容量，断言启动失败而不是静默截断一张） |
| 对象登记 | 跨界 ABI 存储自身 | 对象目录里每个跨界数组（禁止清单、`FinalMapDenyRanges`、映射义务、设备义务、登记表自身）都能追溯到物理范围与最后使用阶段；「登记表自身符号取自链接期静态范围」要有一条静态检查（不依赖运行期登记结果） |
| DTB | `PostShimConsumer` 三值 | 三条腿各断言自己的取值；`OriginalSource` 分支要有一条「原始 DTB 落入禁止集合 ⇒ 必须有安全副本或拒绝启动」的用例；`None` 分支要有「全部读取发生在禁止生效之前」的顺序断言 |
| DTB | 无 DTB 的准入 | 构造「无条目 + 平台证据缺失」⇒ 拒绝；「无条目 + 平台证据齐全」⇒ 启动并打出「无设备树保留语义输入」；「有条目但损坏」⇒ 拒绝（与前者可区分） |
| 语义 | 五字段语义合同 | 断言 ABI 里不出现任何架构编码值名（PAT 索引 / AttrIdx / PBMT / PBMT 类扩展名），并断言五字段各自可独立判失败 |
| 平台 | RISC-V `SupportedAccess` 四项 | 取指、普通读写、硬件页表读、硬件页表写各一条；缺任一项的平台档案降为「已识别但未验证」；Coherence 与 MemoryOrdering 分列，不得合并成一个标签 |


**第十三轮追加的取证项**（他的要求是「只围绕 R17、访问义务三态、设备映射义务来源与 R10/R18 做逐式裁决」）：

| 取证项 | 判据 |
|---|---|
| R17 全矩阵覆盖 | 为每一行类别对各构造一条相交用例；**未登记类别对**构造一条并断言拒绝；`RAM × 设备树保留` 在两条腿上的验证步骤分别断言（UEFI 先覆盖后类型、非 UEFI 直接切除） |
| 三态访问义务 | `MustMap` 缺页失败、`NoRequirement` 既不进 R13 也不进 R19、`MustNotMap` 在实际页表里出现叶子时失败；`NoRequirement` 用原始 DTB 复制后的缓冲区做实例 |
| 路径判据 | 每条 `DeniedFor` 一条用例；`DeviceWindowAccess` 与两份禁止集合的冲突各一条；未定义路径被拒绝而不是默认放行 |
| MRB 与 no-map 的强度差 | MRB 条目与设备窗口相交 ⇒ 拒绝启动；`no-map` 与设备窗口相交 ⇒ 按具名政策；同一 MRB 内条目重叠 ⇒ 归一化之前拒绝 |
| 设备义务生产依据 | 断言设备义务的每条记录都能追溯到 `DeviceWindowRanges` 的一条窗口与启动路径的一条实际访问需求；两棵树各自的义务分别可查 |
| 状态机乘积 | 允许组合表逐格断言；非法组合按实现缺陷拒绝；非 UEFI 腿 `NotApplicable` |
| RISC-V 访问粒度 | 取指、普通读写（按内核实际宽度与对齐）、硬件页表读、硬件页表写各一条；按区域限定范围，不把每种能力扩成全网必备 |


---

## 三、UEFI 内存图描述符的形状（原正文 §0.2，第十一轮移入）

### 0.2 UEFI 内存图描述符的形状（规范事实）

每条描述符有四个语义字段。版本 1 的描述符**总长 40 字节**，其中包含一个 4 字节的补齐位（补齐位不是 40 之外的额外长度）：

| 偏移 | 字段 | 宽度 | 内容 |
|---|---|---|---|
| 0 | `Type` | 32 位 | 这块区域**是什么** |
| 4 | （隐式对齐间隙） | 32 位 | 常见 64 位 ABI 下 `Type` 之后为把下一个 64 位字段推到 8 字节边界而留下的间隙。**规范的结构定义里没有名为它的字段，也没有规定它必须为零** |
| 8 | `PhysicalStart` | 64 位 | 物理起点（页对齐） |
| 16 | `VirtualStart` | 64 位 | 运行期虚拟地址，引导期读到的是未转换的原值 |
| 24 | `NumberOfPages` | 64 位 | 页数（每页 4 KiB），**必须非零** |
| 32 | `Attribute` | 64 位 | 这块区域**支持怎么用** |

分类器用到 `Type`、`PhysicalStart`、`NumberOfPages`、`Attribute` 四个字段；`VirtualStart` 属于「结构里有、语义上此刻用不上」。**那 4 个字节不是描述符的语义字段：解析器不得依赖其内容，也不得因为读到非零就判格式错误**（本方核实现状：解析层只访问偏移 0/8/24/32，从不读偏移 4–7，因此今天没有这种误判，但这条要写成合同而不是靠巧合）。三条几何约束（起点对齐、长度不溢出、页数非零）本仓都已在解析或裁决层实现，其中页数非零是一条独立的错误码（代码事实）。

两个容易踩的形状事实：

- **步长可以大于 40**。固件按「未来可能加字段」的方式排布描述符，实际步长由固件自己报告。因此「缓冲区长度 ÷ 40」推不出条数，任何假设步长等于 40 的实现都可能读到空洞。
- **`Type` 与 `Attribute` 是两个正交的维度**。类型是固件对区域用途的断言，属性是**一组异质的区域标志**，不同位的语义类别不同（能力、性质、映射要求、政策提示），它不是「此刻正在被怎么用」的记账，也不是一种统一的东西（异质性的逐类说明见本节下方）。把属性读出来原样翻译成页表标志是错的，只按类型分类同样是错的。**`Attribute` 是一组异质的、相互独立的区域属性标志**：不同位可能表达**硬件能力**（保护能力各位、处理器加密能力位）、**内存自身的性质**（缓存类各位、非易失位）、**操作系统必须遵守的映射要求**（运行期位——它说的是操作系统需要在建立运行期虚拟地址映射时为该区域给出映射，不是一句「硬件能做到什么」），以及**面向操作系统的资源管理提示**（特定用途位、可热插拔位——规范对这些位的措辞是建议与提示）。它既不是当前页表的权限状态，也**不是一张可以整体翻译成页表标志的能力位图**。本方上一版把属性整体写成「那块区域支持的能力位图，表示可以做到什么」是**过宽的**，第八轮评审人指出它与本文后面对运行期位的用法自相矛盾——本方核对成立：那一句话把四类不同的东西压成了一种，而本文整个裁决顺序正建立在「不要把不同的东西压成一种」之上。改写后的直接收益是 §2.1 第 4 步与 §2.5 的逐位表有了准确的概念基础：逐位分档处理之所以必要，恰恰因为这些位不是同类。

**这一条是 §7.1 那个阻断项的根源**：属性位的含义完全由规范版本决定，位定义取错就等于把已知位判成未知位、或反过来。

标准类型有 16 个（数值 0 到 15）：`RESERVED`(0)、`LOADER_CODE`(1)、`LOADER_DATA`(2)、`BOOT_SERVICES_CODE`(3)、`BOOT_SERVICES_DATA`(4)、`RUNTIME_SERVICES_CODE`(5)、`RUNTIME_SERVICES_DATA`(6)、`CONVENTIONAL`(7)、`UNUSABLE`(8)、`ACPI_RECLAIM`(9)、`ACPI_NVS`(10)、`MMIO`(11)、`MMIO_PORT_SPACE`(12)、`PAL_CODE`(13)、`PERSISTENT`(14)、`UNACCEPTED`(15)。数值更高的区段分三段：标准类型之后的**未分配数值区**（`0x10` 到 `0x6FFFFFFF`）、**OEM 保留区**（`0x70000000` 到 `0x7FFFFFFF`）、**OS loader 保留区**（`0x80000000` 到 `0xFFFFFFFF`）——三段语义不同，不能并成一段处理。

属性位图里本文会反复出现的位：**缓存类** `UC`/`WC`/`WT`/`WB`/`UCE`（以及现代用法同属缓存策略的 `WP`）；**保护能力** `RO`/`RP`/`XP`（硬件能做到什么，不是当前权限）；**独立用途属性** `RUNTIME`（需要运行期重新映射）、`NV`（非易失）、`SP`（特定用途内存）；**元信息** `MORE_RELIABLE`、`CPU_CRYPTO`；**ISA 掩码**（一组架构专用位加一个 `ISA_VALID` 有效位）。属性位的解释**不取任何笼统的规范版本号**，而由配置档标识决定（§7.1、§2.5）。


@@@PART@@@

---

## 四、逐裁决需要补的证据面（原正文 §1.5 与 §2.4，第十一轮移入）

### 1.5 需要补的证据

| 架构 | 必须提交的证据 |
|---|---|
| x86-64 | `PAT` 初始化代码；各逻辑处理器 `PAT` 一致性的处理位置；MTRR 原始读数与其有效类型分析；**各处理器 MTRR 一致性核对**；设备页取不可缓存索引的实际验证 |
| AArch64 | `MAIR_EL1` 读回值（每颗核）；引导根与最终根**各自**的叶子索引统计；**普通主存叶子的共享性字段核对**（与属性索引分开统计）；切到最终根后实际启用的根的确认 |
| RISC-V 64 | 支持平台清单（按 §1.4 第 3 问的三档粒度）；每个平台的属性承诺来源；**按物理区间分级的原子能力证据**（LR/SC 与原子读改写两类分开、含内核依赖点的映射）；平台识别信息与该合同的关联方式。**这一项是准入条件，不是可选验收**：只证明处理器有指令不算通过。**第九轮按评审人第 11 项再细化一层**：登记要到「内核实际使用的那个操作 + 那个宽度」这一级——操作类别（LR/SC、交换、比较交换、加减、位逻辑）、**操作宽度（8/16/32/64 位里内核真的用到哪些）**、对齐要求、内核依赖的排序强度、以及每项支持的证据来自规范 / 平台文档 / 实测中的哪一类。判据（评审人给，本方采纳）：内核若使用 64 位原子变量而平台只保证部分宽度的原子读改写，**「实现了 A 扩展」不能顶替区域级能力证明**；规范本身也把原子能力等级与自然对齐访问宽度挂钩。**第十轮再按他的第五节把这张表的主键换掉**：登记的行不再是抽象 API 名称，而是**内核实际生成或调用的原子实现路径 × 操作数宽度 × 对齐要求 × 区域级 PMA 等级 × 排序要求**。他给的理由在实现层是本方无法反驳的：`compare_exchange` 与「AMO 的交换 / 加减 / 位逻辑」不在同一层——基础 A 扩展就能用 LR/SC 序列实现 XLEN 宽度的比较交换，而**硬件 `AMOCAS` 属 Zacas 扩展**；8 / 16 位的原子操作另涉 **Zabha**，且 Zabha 对子字节 AMO 的扩展**不蕴含** LR/SC 同时获得 8 / 16 位支持。所以同一句 `compare_exchange` 编译成 LR/SC 循环与编译成 `AMOCAS.D` 是**两条不同的准入合同**：前者落在「该区域支持 64 位 LR/SC 且有进展保证」，后者还要落「该区域支持 Zacas 的 `AMOCAS.D`」。8 / 16 位同理，要看最终代码生成是哪一种：**Zabha 原生指令 / 更宽原子操作的模拟 / 库内锁回退**。本方据此加两条实现约束：① **平台准入清单必须来自内核实际采用的实现，不能来自 API 名称**，因此这张表的取证要挂在编译产物的指令清单上，而不是源码里的调用点；② 本方核出这一条在本仓**连输入都不存在**——内核侧原子用法只在源码层可读，没有任何一处把实现路径登记成准入输入（本方读数：内核与库层里**含 `compare_exchange` 或 64 位原子类型字样的行共 122 行**——**第十一轮按他的第 28 项把口径写准**：现量命令 `grep -rc -e compare_exchange -e AtomicU64 os/kernel os/libs --include=*.rs` 统计的是**匹配行数**，既不是调用次数也不是生成的原子指令条数，同一行含两个词也不会变成两个使用点；本方上一版把它写成「122 处用法」是**把行数当成使用点数**，正是本方第十轮刚立的那条「计数类断言不能只看计数」的又一次违反。真正的准入依据仍是这张表要求的最终编译产物指令路径；而 `grep -rn -e Zacas -e Zabha os --include=*.rs` **读数为 0**——两个扩展名在本仓文本里都还没被命名） |

**已撤回的建议**：不再要求「故意把 `PAT0` 改成不可缓存、用量测的启动时间差证明这条合同值得做」——那只能说明性能影响，不能证明映射语义正确。取而代之：合成的 MTRR 组合用例、寄存器原始读数、以及对实际建立的映射做遍历验收。

---


@@@PART@@@

### 2.4 需要补的证据（按腿分工，上一轮已修正）

| 腿 | 采什么 |
|---|---|
| x86-64（UEFI） | 内存图四时点样本：读图时、自举分配之后、退出前一刻、与成功版本戳配对的那次快照。每条记录类型、物理起点、页数、属性 |
| AArch64（UEFI） | 同上，另加该腿实际收到的设备树快照（这条腿同时有两份来源，见 §4.4） |
| RISC-V（裸机/设备树） | 实际接收到的设备树（或其文本形式）、平台属性依据、启动前后的相关内存范围证据、以及设备树自身的保留映射块条目（文件头之后那份区间列表） |

另两项：固件身份与表头修订值、以及**读图服务返回的描述符版本**（三者都要记录，属性解释由配置档标识决定，不由其中任何一个单独决定，见 §7.1）；引导期结束时存活分配对象的类型清单（行 5 的守卫依赖它）。


@@@PART@@@

---

## 五、当前实现的读数与登记在案的等待项（原正文 §5.1 与 §7.2，第十一轮移入）

### 5.1 当前实现做到哪里（代码事实，缺口有两层）

| 输出 | 记了什么 | 没记什么 |
|---|---|---|
| 可用表 / 保留表 | 区间（起点 + 长度） | 类型与属性（它们只影响裁决，不进输出） |
| 诊断样本 | 区间（起点 + 长度） | 类型与属性——所以「命中了哪一位」「缺写回位的是哪一类」无法从输出复原 |
| 排除 / 设备路径 | **什么都不留**：三个计数器（可用、保留、样本）里没有它们的位置，既没有条数也没有范围 | —— |

于是「不静默忽略」目前只兑现到「样本区间可见」这一层，而被类型强制排除的那一类连区间与计数都没有。


@@@PART@@@

### 7.2 登记在案但不属于本批裁决的等待项

1. **两遍扫描的接口形态已被落码单方面选定**（预检产出凭证、写入不返回错误）。合同是冻结的，但「凭证」与「完成函数直接返回普通值」原本约定由原型比较后择一，实现直接选了前者。若评审人要求换形态，代价是改代码而不是改文档。
2. **另一份固件内存属性表**：规范另有独立一张表描述运行时服务内存的保护属性。第一版不支持运行时服务，因此不实现；将来支持时不能假设描述符属性字已包含全部保护布局。
3. **运行时服务的虚拟地址映射合同**：带运行时属性位的区域若要使用，需要一套与直接映射不同的映射与地址转换约定。目前只保证「不进分配器、成为诊断成员」。
4. **池分配释放后的页级归属**：从内存池取出的块在对象释放之后页是否退回常规内存——规范只保证池分配来自专门的池，不保证页立刻归还。待实测事实，不是政策问题。
5. **ACPI 支持落地之后**：`ACPI_RECLAIM` 的回收、`ACPI_NVS` 是否需要映射，会随行 2、行 5 一起重开。
6. **可信物理内存拓扑来源**：若将来出现独立可信的 RAM 拓扑来源，是否重新引入地址过滤——现在的答案是不引入（冻-2）。
7. **引导路标条目的合并**：台账记账工作，不含内存图语义裁决。
8. **本方登记的一条独立违例**（发现于本批核查，不在本文射程，也不由本文顺手删改）：引导壳的 aarch64 UEFI 腿里存在**运行时错误文本混用非英文与工单代号**的情况，与「代码注释应为英文且自解释」的项目约定不一致。位置：`os/boot-shim/src/uefi_helpers.rs`（设备树快照解析失败与出界闸两处中止文本）。现量命令：`grep -n 'panic!("boot-shim' os/boot-shim/src/uefi_helpers.rs` 后逐条查看文本语言。建议按登记流程单独开条目，由认领者处理。

**第五轮追加的三条等待项**：

9. **保留区间输入若将来真实出现**：本文已从数据流里删掉第三个区间输入（本仓没有那份列表，§10 现量命令）。若将来出现一个由本项目自己交出的保留区间列表，必须**同批**给出生产者、字段布局、生命周期、准入语义与消费者，并重新并入 §4.4.2 的区间数学；在此之前它不得出现在流程图里。
10. **x86 有效内存类型分析器**：第五轮要求把「分析 MTRR」写成有独立输入、输出与合成用例的**纯计算组件**，而不是在启动代码里临时拼几条寄存器判断。它至少要覆盖：MTRR 是否启用、默认类型、固定范围、可变范围、重叠区间的优先级、物理地址宽度与有效掩码，以及与当前 PAT 布局组合后的最终有效类型。**原始寄存器值一致不等于有效类型一致**——前者是输入证据，后者才是每颗逻辑处理器对每个相关物理区间的验收对象。归属内核架构层，与 §1.3 的条款 ⑤⑥ 同批实现。
11. **架构语义合同的接口分层**：第五轮给出四层职责（来源适配只做解析并保留来源证据 / 通用政策层做区间分类与分配、映射资格，不读 `PAT`、`MAIR`、PMA / 架构语义合同判断并兑现平台保证 / 架构页表实现把语义兑现成页表，不重新推导来源政策），并特别警告「一个叫 `MemoryArch` 的 trait 若塞满 `set_pat`、`write_mair`、`configure_pma`，只是把硬件操作搬到 trait 后面，没有实现语义抽象」。**本方采纳其流程结论：本轮不引入任何大而全的架构 trait**，等政策与消费者明确后按真实行为差异确定接口。这条属于接口设计方向，不属于本文的内存图政策射程，故登记而不展开。

**第六轮追加的四条等待项**：

12. **8 KiB 那条判据已撤销（第七轮）**：第六轮本方曾把「带运行期属性位的描述符须 8 KiB 对齐、长度为 8 KiB 整数倍」登记为待逐字核对的规范判据，并留下「严格报错还是记录后继续」这道选择题。第七轮评审人核对 UEFI 2.10 与其 Errata A 后指出实际要求是 **4 KiB**，8 KiB 那一支属于 Itanium 架构章节；本方的检索读数与另一份公开翻译在同一位置给出两种数值，冲突已登记在 §2.1。**结论是撤销这条判据而不是给它选一个严格度**——本仓解析层现有的 4 KiB 检查已经与三个目标架构的要求一致。若将来项目出于自己的理由要施加更严的对齐，那是新增政策，需要独立理由与用例。
13. **相交断言的裸加法**（本方本轮查出的现存脆弱性）：引导壳那条「常规内存不得与自举分配相交否则拒绝启动」的断言用普通加法算两端末端；今天因上游已用 checked 运算而不可达，但绕过分类器的新调用会拿到一个静默失效的守卫。改法与用例见 §13 追加的取证项。归属引导壳，不需要政策裁决。
14. **运行时身份字段的读取点**（第七轮升为结构性缺口，不再算等待项）：白名单第一层里有两项（厂商串与厂商版本）在本仓**零读取点**，设备树平台识别串只有裸机腿可读。第六轮本方据此建议「第一版只登记一条已核实组合」，**第七轮撤销该建议作为解决方案**——按本文自己的算法，必需字段读不到就是零命中，会拒绝本项目自己的三条腿；声明不约束则唯一命中是虚假的唯一性。现行处置是在方案 A / B 之间显式择一（本方选 B：把限制写成外部支持前提并声明这不是引导程序自己完成的识别），身份键闭合前保留 fail-closed。
15. **启动对象登记表的阶段档数**：**第九轮裁定为保留四档、不再讨论档数**。未决的换成另一件事——四档有没有覆盖每一次状态变化；本方按 §6.8 的转换表登记了两个候选事件（交接结构被内核解码完成、禁止清单自身存储被消费完成），它们不改页表但可能改变生命周期，留给下一轮交叉审计判定。
**第七轮追加的三条等待项**：

16. **Itanium 分支的那一条要不要留痕**：本仓不引导 Itanium，因此本方建议**只留一条注释性登记**（本文 §2.1 的冲突表已经承担这个作用），代码与政策表都不出现 8 KiB。若评审人认为连登记都不必，本方把它降级为一条撤回说明。
17. **启动对象的释放点**：第一版一律保持排除，因此回收能力为零。将来要收窄时，需要的不是「谁释放了」这一个事件，而是**每个对象最后使用阶段的正式定义**——它与 §6.8 的登记表生命周期字段是同一件事的两次表述，应该一起设计，不要先做分配器的归还路径。
18. **配置表快照与设备树复制的先后**：第七轮把「首次退出尝试之前完成复制」定为合同，本仓 aarch64 那条腿的设备树快照来自配置表查询、其原始缓冲区又被直接映射来源引用。两处复制规则（§4.3 与本节 §3.4）必须落在同一个动作上，否则会出现「一处说已复制、另一处仍用原始指针」。这属于代码接线顺序问题，等下一轮交叉审计一并看。
**第八轮追加的四条等待项**：

19. **`TypeExcludedRanges` 要动返回值形状**：本方核实裁决函数对固件保留类型与不可用类型**返回长度零**，因此「让被排除的范围产生一份完整区间」不是下游加一个集合就行，要先让那两条去向带上长度。这属于合同还是实现细节，本方登记为待裁（§0.5 第 1b 问）。
20. **`PmmPolicyUniverse` 的形成过程本身要有容量合同**（第九轮由建议升为强制）：统计范围按启动方式分别构造，它自己也要经过收集、排序、合并与临时缓冲，同样可能溢出。本文的处置与 §4.4.2 那条同源——**形成统计范围所需的来源缓冲与工作区超限一律拒绝启动**，不适用「诊断样本可以截断」那一档；评审人在第八轮第 20 项与第九轮第 15 项两处给的「应当成为强制合同而不是等待确认」，本方采纳，这一项因此**不再等待确认**。
21. **四维映射合同的第二维需要一份「虚拟地址关系」的表示法**：恒等、别名、专用路径这三类要在验收里可比对，本仓现在只有「物理地址到虚拟地址的换算」这一种函数形状，没有可登记的「关系种类」。这不是政策问题，是它的数据形状未定，本方登记而不代决。
22. **阶段边界归属**：本方第八轮给出「最后一次使用归前一阶段」的取舍，它同时影响 R4、R7、R10 与映射义务四处的用例；评审人若改取「归后一阶段」（宁可多排除一页），本文要按他的选择重写这四处的措辞而不是只加一句备注。


---


@@@PART@@@

---

## 六、裁决六的问题现场：两棵页表树的代码事实（原正文 §6.1，第十一轮移入）

### 6.1 问题（代码事实，不变）

按 §0.3：引导根的恒等段**无差别铺低 4 GiB**，只有已知设备窗口参与拆分；类型政策、诊断成员、被排除范围、设备树 `no-map` 都不参与这段构造。于是三条已写下的承诺在两层含义上强度不同：

| 承诺 | L2（最终直接映射） | L3（引导根恒等段） |
|---|---|---|
| 「诊断成员不建普通映射」 | 成立 | **不成立** |
| 「被排除的区域不进入普通分配器」 | 成立 | 不适用（排除本身不管引导根）；**它不等于「不许访问」**，见下 |
| 「`no-map` 不进标准系统内存映射」 | 成立（前提是把禁止范围喂给大页判据） | **不成立** |

本方上一版把第二行写成「OS 不得访问」，与「未进最终直接映射」混为一谈；第二轮指出不分配、不进最终别名区、不进引导根、不允许非受控访问是四件事（§0.3 的四层表）。**第五轮进一步要求把这句彻底删掉**，本方照办——它与本简报自己的决定直接冲突：既然不把 UEFI 的固件保留类型整批放进映射禁止清单（§6.6），这些范围的实际状态就是「不进可用表、不进最终直接映射、但仍被引导根的恒等段覆盖」，写「不许访问」等于用一个没有任何集合承诺支撑的说法盖住三件不同的事。

准确表述：**被排除的区域不进入普通分配器；是否禁止建立页表映射，由对应来源的映射政策单独决定。** 规范侧也支持这样分——固件保留类型的语义是「不可用」、不可用内存类型的语义是「有错误、不得使用」，两者的「不要用」都不等于设备树 `no-map` 那条页表合同。

---

## 七、路径 A / 路径 B 的机制背景（原正文 §3.1，第十一轮移入）

### 3.1 机制背景：两种顺序，以及我们现在实际站在哪一条上

引导壳必须做一件带有事务性质的工作：读内存图得到一个版本戳，然后用**同一个键**去调用退出函数——中间只要有任何分配改动了内存图，键就失效，必须重读重试。

**先把事务表述改准确（采纳第二轮纠正）**：规范里退出引导服务这个调用**接收**版本戳、**返回**状态码，它本身不返回一份内存图；内存图是此前由读图服务取得的。「成功退出」的含义是**固件接受了与这个键对应的那次快照**。因此路径 A 的准确流程是：

1. 完成所有自举分配；
2. 调用读图服务，**保留**描述符快照及其版本戳；
3. 用该版本戳调用退出服务；
4. 只有成功后，才把**与成功版本戳配对的那份快照**交给分类器。

本项目自己的退出封装函数可以返回一个含快照的结果对象，但那是**封装层保留下来的读图数据**，不是固件退出调用的返回值。文档层面的意义在于：退出之后**不能**再调用读图服务去取所谓「最终图」——那时读图服务已经失效。这不推翻路径 A，只是让事务合同准确。

| 顺序 | 做法 | 关键差异 |
|---|---|---|
| **生产式（路径 B）** | 读图 → 分类 → 用同键退出；键失效则重来 | 分类发生在引导服务还活着时；读图与退出之间**零分配**，所以图不会因为自己的动作而改变 |
| **路径 A** | 先完成全部自举分配 → 取快照并留底 → 用该键退出 → 成功后用那份快照分类 | 那份快照就是退出所依据的那张图，因此包含所有自举占用（含读图缓冲自己那一块）；代价是分类发生在引导服务已失效之后 |

**当前实现的准确位置（代码事实）**：生产腿走路径 B（一个用函数指针表达读图与退出的配对循环，可被模拟固件在宿主测试里驱动）。**辅助/测试腿两条都不是**——它在退出之前自己另读一份快照，然后把封装函数返回的那份快照**显式丢弃**（那段丢弃的注释理由是「内核已经通过另一个入口拿到了常规内存区间」）。因此路径 A 在本仓**尚未接线**，实际改动面是：把取表入口从「自己调读图服务」改成「消费与成功键配对的留底快照」，并让 15 个共用它的测试载体跟着改调用形状。


@@@PART@@@

---

## 八、术语表（原正文第九节，第十一轮移入；正文各术语在首次出现的政策段落里都有就地解释）

## 9. 术语表

| 术语 | 含义 |
|---|---|
| 内存图 | 固件交出的物理地址区间清单，每条描述符 = 类型 + 起点 + 页数 + 属性位图 |
| 引导壳 | 由固件加载、负责准备页表与交接数据、然后进入内核的一段独立可执行代码 |
| 描述符步长 | 相邻两条描述符起始地址之差；规范长度是最小值，固件可以用更大的值做向后兼容扩展 |
| 属性位图 | 描述该区域**支持**什么用法的位集合，不是「此刻正被怎么用」的状态 |
| 属性配置档 | 一个有标识的实现兼容档：身份维度组合 + 逐位取值 + 允许掩码 + 出处 + 验证样本。它**不宣称规范归属**，也不能由表头修订值单独选出 |
| 表头修订 / 描述符版本 / 配置档标识 | 三个互不替代的版本概念：系统表所符合的接口修订、内存描述符的结构版本（规范单独定义为 1）、属性字按哪张表解释。**描述符版本 1 不蕴含属性位布局 1** |
| 归一化集合族 | 同一页级坐标下的多个语义集合：`PmmExcludedRanges(p)`、`BootMapDenyRanges`、`FinalMapDenyRanges`、`StartupObjectPmmReservedRanges(阶段)`、`StartupAccessRequirements(树,阶段)` 与其派生的 `StartupMappingRequirements(树,阶段)` （**第十一轮更新**：本词条第十轮还写着已被拆掉的旧名 `StartupRequiredObjectRanges(阶段)`，而他指出的风险是真的——第三方读者会照术语表那个名字去实现，而那个名字在正文里已经不存在了）、来源诊断 |
| 写回缓存 | 主存的标准缓存策略：写先进缓存、之后回写。普通内存的默认期望 |
| 不可缓存 / 设备属性 | 访问直达地址空间，不经缓存。对寄存器是必需的，对普通内存是灾难 |
| 普通主存语义 | 三架构统一的目标：按主存方式访问、共享时保持一致、原子操作按主存语义生效。不等于「必须有物理缓存」，也不等于某个具体属性名 |
| `IA32_PAT` / MTRR | x86-64 上决定有效内存类型的两套机制；叶子页表项的 `PAT`/`PCD`/`PWT` 三位共同选择 `IA32_PAT` 的 8 个表项之一（普通页与大页的 PAT 位位置不同），选中的表项值再与该地址的 MTRR 类型按架构组合表合成有效类型（**表项选写回而 MTRR 为不可缓存时，结果仍是不可缓存**）；`PAT` 每逻辑处理器一份，MTRR 按物理地址范围设定，多核下要求各核**有效类型**一致（不是要求寄存器原始值看起来一致） |
| `MAIR_EL1` / 属性索引 | AArch64 的属性表寄存器 / 叶子页表项用来选表项的 3 位索引 |
| 物理内存属性 / Svpbmt / PBMT | RISC-V 的平台定义、由硬件检查的地址属性（部分固定，部分平台支持 M 态重配）/ 可选扩展 / 页表项里那两位（只有「跟随属性」「不可缓存主存」「强序 I/O」，且需 M 态使能） |
| 直接映射 | 内核把物理内存线性映进自己的虚拟地址空间，物理地址加固定偏移即可访问 |
| 引导根 / 最终直接映射 | 两棵页表树：切根之前使用的那棵（含一段无差别恒等映射）与内核正式的物理别名区 |
| L1–L4 四层约束 | 不分配 / 不进最终直接映射 / 不进引导根恒等段 / 不允许非受控访问。四者不等价，本文不再用「不建普通映射」一句话统称 |
| 物理页分配器 | 内核分配与回收物理页的组件 |
| 可用表 / 保留表 | 两份输出：可自由分配 / 可访问但不得再分配 |
| 诊断成员集合 / 诊断样本 | 成员=所有「保留但不建普通映射」的范围（语义集合，完整）；样本=实际存储的有界数组（可截断） |
| 启动映射禁止清单 | 跨交接边界的集合：禁止引导根与最终根映射的页；**不可截断**、装不下即拒绝启动；条目只装规范化区间，来源与原因留在诊断侧 |
| MapKey / 版本戳 | 内存图的版本标识；退出引导服务必须携带取得该键的同一次读取，且只返回状态码 |
| 退出引导服务 | 结束固件引导期服务的调用；此后固件的分配、控制台、释放接口全部失效 |
| 交接结构 | 引导壳交给内核的固定布局数据块，字段按基址加偏移定位，不含进程内指针 |
| 恒等段 | 引导期把物理地址与虚拟地址等价的映射区间，让还没切页表时代码能继续跑 |
| 切根 | 把页表根从引导期页表切换到内核页表的动作 |
| 夹具型测试 | 只把某份数据当输入、不对该数据本身做断言的测试，被测对象是别的代码 |
| 六步裁决顺序 | §2.1 那张表：未知或不支持 → 存在即拒绝 → 类型强制处置 → 类型与生命周期属性的一致性 → 普通候选 × 独立属性 → 来源准入与架构语义准入 |
| 切除 | 设备树表达「这片 RAM 里有一段不要用」的方式：保留节点地址与 `/memory` 区间相交是正常形态 |
| 保留映射块 | 设备树文件头之后的独立保留区间列表，与 `/reserved-memory` 子树是两种输入，不能只实现后者；规范允许设备树本体出现在其中 |
| `ReservedPrefixEnd` | 本项目的一个物理地址前缀阈值（低于它的整段当固件常驻区），**不是区间集合**，不参与与其它保留范围的并集 |
| 启动必需对象 | 内核镜像、入口代码、栈、交接数据、分类输出数组等跨切根仍需访问的对象；多处「拒绝启动」判据共用这一份清单 |
| 格式上限 / 能力上限 | 同一个「物理地址上界」概念的两种含义：分页**格式**能编出的最大物理地址，与这颗处理器 / 这块平台在当前分页配置下**实际可表示并可访问**的上界。本仓内核侧用的是后者、引导壳分类器用的是前者，第七轮把它列为结构性缺口（§4.4.4） |
| 三类边界 | 判定启动对象是否需要登记的判据（第七轮取代「最小完备集有多少项」的问法）：内存图改变、页表切换、物理页分配器启用。会跨越其中任一条的对象都要能追溯物理范围、最后使用阶段、以及在每棵树上的访问方式 |
| `SourceDomain` | **第十一轮改为按启动方式分支定义**（他的第 6 项后半：笼统写「所有来源区间的并」会把两条腿的输入形状又压成一句话）。**UEFI 腿**只含固件内存图里每一条有效描述符的范围；**非 UEFI 腿**含 `/memory` 的 RAM 候选、设备树 `/reserved-memory` 各子节点的 `reg` 与文件头保留映射块条目。**它是「收到了什么」，不是「政策允许什么」**——这一层分开正是第十轮那处假闭合的修法 |
| `DeviceAddressRanges` | **第十轮新增的域外集合**：设备窗口类地址。它既不进可用表也不进 `PmmExcludedRanges`，但**必须有完整的不可截断存储**（域外不等于不记录），消费者见 §2.1 第 3 步与 §6.8 的设备映射义务 |
| `DeviceTreeReservedRanges` | 设备树两路保留来源（`DtbMemoryReservationBlockRanges` 与 `DtbReservedMemoryRegRanges`）页级归一化后的并集，带来源标签（§0.3 第十轮补入构造式） |
| `ReservedPrefixEnd` / `PrefixRangeView` | **第十一轮改名**（他的第 5 项：本方上一版同时留着「阈值不是区间集合」的旧文与`ReservedPrefixRanges` 这个集合名，三个地方两套口径）。现行：**`ReservedPrefixEnd` 是唯一标量输入，不是集合、不进容量清单**；`PrefixRangeView = [0, ReservedPrefixEnd)` 是规划阶段按共同页级坐标处理时的**派生区间视图**，作为公式成员参与分区，**没有独立存储**。旧名 `ReservedPrefixRanges` 从本文除名 |
| `MappingDeniedSet(树)` | 按树选择的映射禁止集合：引导根取 `BootMapDenyRanges`、最终根取 `FinalMapDenyRanges`（R10 第十轮改写） |
| `MappingSatisfies` | R13 的合同谓词：对每条映射义务核对五个子判据 R13a–R13e，任一失败即该树该阶段验收不通过 |
| `PmmPolicyUniverse` | 「不许分配」这件事的**统计范围**，**第十一轮按 §0.3 的两层构造式整体改写**（本词条第十轮只改了一半，仍是第九轮那个「先减掉待统计排除项」的旧公式，与正文相反——这是本方自己第 21 项要防的双公式并存）。现行定义：
  `SourceDomain(启动方式)` → 只减 `DeviceAddressRanges` → `PmmPolicyUniverse`；
  · **UEFI 腿**：`SourceDomain` = 全部有效固件描述符的范围。设备树的两路保留声明是**需要对照的政策输入**，在没有覆盖证明时既不得默默扩张统计域、也不得默默缩小它；
  · **非 UEFI 腿**：`SourceDomain` = `/memory` 的 RAM 候选 **并** 设备树两路保留范围；
  · 两条腿都只扣 `DeviceAddressRanges`（**且必须先过 R17 的来源自洽检查**），前缀派生视图、类型强制排除、诊断成员等一律**在域内**参与分区。
它不是整个物理地址空间；本文所说「完整集合」一律相对于它而言（§0.3） |
| 义务来源 / 候选来源 | 两种不同的东西：**候选来源**回答「哪些区间可以被拿来建映射」（本仓现有四份并集）；**义务来源**回答「哪些对象必须能通过什么方式被访问，且权限与属性要求是什么」。第八轮按评审人裁定把登记表定在前者以外的后者，避免它被当成第五份候选来源 |
| 来源完整性测试 | 与「集合关系测试」并列的第二组验收：每种来源的每个有效输入条目，必须要么被映射进某个完整政策集合，要么被显式拒绝，不得只出现在可截断的诊断样本里。关系测试结构上看不见漏收（§0.3，第八轮） |
| 闭合（本文用词纪律） | 一个抽象只有点名了**每一个**下游消费者、并写明接不上时会怎样失败，才允许标为已闭合；否则一律写「结构已定、接线待核验」。第七轮起生效，起因是本方第六轮把接错了下游的登记表标成了已闭合 |
| `StartupObjectInventory` | 本项目自己的启动对象**登记表**（身份、物理范围、生命周期、来源、映射要求五字段）。它是集合规划阶段的**第四个输入**，但**不是**第三种物理内存来源、也**不是**另一份保留区间表；由引导壳在分类之前组装（§6.8） |
| 两步走（区间数学） | 先用 checked 加法验证**原始**末端 `start + len`，溢出即拒绝该输入；之后才做上界钳制、页级向外取整与集合规划。顺序不可交换——让饱和加法先跑，格式错误的长度会被夹成「地址空间顶端」，伪装成合法的最大地址（§4.4.2） |
| 运行时可读身份 / 流水线证据 | 白名单字段的分层：前者是运行中的引导程序真能读到的维度，**只有它参与匹配**；后者（模拟器版本与参数、固件镜像哈希等）只有宿主知道，作为测试流水线的落盘证据，不得冒充运行时身份（§7.1） |
| 配置档标识（`AttributeProfileId`） | 匹配**返回**的结果，不是匹配输入。把它同时写成维度与结果会构成循环定义（第六轮修正） |

---


@@@PART@@@


---

## 九、逐位处置表（原正文 §2.5，第十二轮移入）

正文保留该表的合同性质与优先级（档外位先拒绝），这张表提供每个位的逐行判定，供实现与用例对照。

§2.3 的第 3 条判据是「基线内每个已知属性位都有明确处置，五者必居其一」。此前正文只按类别写，没有逐位兑现——本轮补齐。**但下表不是按「UEFI 2.10」这个笼统标签兑现的**：第四轮确认 2.10 本身有多份正式发布物且属性位取值并不相同（§7.1），因此每张逐位表都必须绑定一个**配置档标识**才有意义。

**本档的身份（第五轮要求，本方采纳）**：下表既不是 2.10 原始版、也不是勘误版、也不是 2.11，而是**以本仓钉死的依赖库头文件布局为基础的实现兼容档**，标识记为 `MEMATTR_PROFILE_UEFI_RAW_0_9`。这个名字刻意不宣称任何规范归属——按 §7.1 的矩阵，本仓常量里 `WP`/`MORE_RELIABLE` 与原始版一致、`ISA_MASK` 与勘误版一致，**没有任何一份出版物与本档完全吻合**。列名「本档取值」的实际数值由 §7.1 的白名单条目决定，换档就要换整张表与它绑定的用例。

| 属性位 | 本档取值 | 处置 | 说明 |
|---|---|---|---|
| `UC` / `WC` / `WT` / `UCE` | `0x1` / `0x2` / `0x4` / `0x10` | **记录但不改变政策** | 缓存能力声明。一个区域可以同时声明多种能力，准入判断只看是否具备普通主存所需的属性；不具备时才走第 5 步的缺失处置 |
| `WB` | `0x8` | **参与准入** | 进两张表的必要条件之一，且必须与裁决一的架构合同一起看（描述符声明写回不等于内核映射按写回兑现） |
| `WP` | `0x1000` | **记录但不改变政策** | 规范把它归在**物理内存保护属性**一类下，描述「硬件支持把该区域配置为写保护」；同一份规范又说它目前通常被用作缓存属性。所以准确表述是：**它是能力声明，不是当前页表的实际写权限**。第一版记录该位，既不因它存在而修改页表写权限，也不因它存在而改变普通内存准入；规范自身的这两重归类要照写出来，不要只挑其中一句 |
| `RP` / `XP` / `RO` | `0x2000` / `0x4000` / `0x20000` | **记录但不改变政策** | 这三位的含义是「硬件**能够**做该保护」，不是当前权限状态。不得仅因位存在就把区域判成不可读、不可执行或只读 |
| `NV` | `0x8000` | **改变分配 + 改变映射**（第 5 步） | 独立于类型：即使类型是常规内存也不进分配器、不建普通映射，成为诊断成员。属性与类型可能不一致，判断只看属性 |
| `SP` | `0x40000` | **改变分配 + 改变映射**（第 5 步） | **规范说的是提示**：该区域为特定用途预留，建议操作系统不要把不可迁移的核心数据与代码放进来；规范并没有要求所有带该位的区域绝对不得映射。本项目的「不进分配器、也不建普通映射」是**比规范更严的第一版准入政策**，正文与注释都要这样标注，不能写成规范强制 |
| `RUNTIME` | `0x8000_0000_0000_0000` | **改变分配 + 改变映射**（第 5 步） | 需要运行期专用虚拟映射，与普通映射是两回事。**普通机器上正常出现，不得因本项目当前不调用运行时服务而拒绝启动** |
| `MORE_RELIABLE` | `0x10000` | **记录但不改变政策** | 政策表登记「当前不解释」；不得静默忽略。这一位的取值在**不同发布物里不同**（见 §7.1 的四份读数），所以「同一位在不同档里含义相反」的风险是真实的，不是假设 |
| `CPU_CRYPTO` | `0x80000` | **记录但不改变政策** | 加密能力标记，第一版不据此改变任何资格 |
| 「可热插拔」 | **本档未定义**（依赖库布局里没有这一位） | **出现即报错**（第 1 步，按档外扩展位处理） | **第五轮抓出的漏项，本方核实后确认**：主流实现头文件把这一位定义为 `0x100000`，而本档钉死的依赖库里没有它（现量命令见 §10）。所以它既不是本档已知位、也不该被当成「空白保留位」——记录要能区分这两种情况（§7.1 的三组用例第二组）。若将来改绑到含该位的档，它必须作为**已知但不支持**的位进入 §2.2 行 1 的第三类处置，而不是继续按未知位失败 |
| `ISA_VALID` | `0x4000_0000_0000_0000` | **出现即报错**（第 1 步） | 本项目的三个架构都不解释 ISA 掩码 |
| `ISA_MASK` 覆盖的位 | 掩码段（本档为 `0x0FFFF00000000000`） | **出现即报错**（第 1 步） | 掩码非零而有效位为零的组合同样报错——那是不自洽的输入，不是「没有属性」。掩码本身的取值也随发布物不同（§7.1） |
| 本档未分配的空档位 | 配置档之外的任何未登记位 | **出现即报错**（第 1 步） | 「保留位」的判据依赖 §7.1 选出的配置档；**选档失败时根本走不到这一行** |

**为什么这张表不能靠「未知位一律容错」来实现**（第四轮把上一版的例子升级了）：同一个数值位在不同发布物里的身份可以完全不同——按四份读数，`0x10000` 在一份里是「更可靠」、在另一份里是「可热插拔」；`0x1000` 在一份里是写保护、在另一份里是「更可靠」。**两种解释都合法且方向相反**：前者只是提示、后者禁止把不可迁移的核心数据放进去。所以必须先按已核实的固件组合选档或拒绝，再谈逐位处置；把某一张逐位表套到任何版本的属性字上，等价于猜固件读的是哪一份 PDF。

**逐位表的验收方式**：本表每一行都要有一条对应的宿主测试（位单独置位、其余取**该 `AttributeProfileId` 的合法组合夹具**——夹具随档提供，不硬编码某个版本号），加上 §7.1 的三组未知位用例；行与用例的对应关系要能在测试文件里按名字找到，否则「逐位都有处置」仍是文档承诺。

---


### 9b、矩阵完备性的验收判据（原正文 §2.3，第十二轮移入）

固定的执行路线只保证裁决函数一定有返回值，不保证每个返回值对应的政策是有意的。完整的验收至少需要：

1. 所有标准类型 0–15 都有明确政策；
2. 三段未知类型区间都有明确的失败动作；
3. 基线内每个已知属性位都有明确处置（五者必居其一：参与准入 / 改变分配 / 改变映射 / 记录但忽略 / 出现即报错）——逐位兑现表见 §2.5；
4. ISA 有效位与掩码的组合完整定义；
5. 未知属性位明确区分 fail-closed 与显式支持的后续属性；
6. 每个类型与独立属性的组合都由这六步顺序唯一决定；
7. 所有进两张表的描述符还须通过裁决一的架构准入。

测试穷举**政策等价类的交叉组合**，并对所有未知位位置做逐位测试；随机采样只作补充。具名用例除四个顺序组合外，含：`CONVENTIONAL + WB + RUNTIME`、`LOADER_DATA + 可热插拔`、**未知位与强制排除类型的组合**（检验第 1 步与第 3 步谁先）、已登记的项目类型与未登记的 OS loader 数值的边界。本方补一条：**逐位测试必须把「保留位」与「后续版本已定义位」分成两组**，否则会重复行 1 那个测试哨兵的混淆。

**关系式 → 测试追溯表（第十五轮建立；`assertion_id → test_id`，25 条原子断言）**

| 断言 | 关系 | 测试占位 |
|---|---|---|
| A01–A04 | R1–R4 | `t_r1` … `t_r4` |
| A05–A06 | R4b-s / R4b-p | `t_r4b_source` / `t_r4b_policy` |
| A07–A08 | R5 / R5b | `t_r5` / `t_r5b` |
| A09–A16 | R6–R13 | `t_r6` … `t_r13` |
| A17 | R13-D | `t_r13d` |
| A18 | R14 | `t_r14` |
| A19–A20 | R15a / R15b | `t_r15a` / `t_r15b` |
| A21–A25 | R16 / R17 / R18 / R19 / R20 | `t_r16` … `t_r20` |
| A26 | R21 | `t_r21` |

（每条断言至少一条正例 + 一条反例；R6/R7/R8/R14 四条冗余各有独立报警用例。新增关系时在表尾追加，不重排既有编号。）

## 十、集合角色分组表（原正文 §6.7，第二十一轮移入）

| 分组 | 成员 | 特征 |
|---|---|---|
| **来源集合** | `SourceDomain`、`DtbMemoryReservationBlockRanges`、`DtbReservedMemoryRegRanges`、`FirmwareRuntimeMmioRanges`、`DeviceWindowRanges`、`RamLikeSourceRanges`、`DeviceLikeSourceRanges` | 「收到了什么」，不参与政策判断（**没有 `DeviceAddressRanges` 这个第三种名字**） |
| **统计与分区集合** | `PmmPolicyUniverse`、`AvailableRanges(p)`、`PmmExcludedRanges(p)` | 页级坐标、阶段参数、互为补集 |
| **政策集合** | `TypeExcludedRanges`、`DeviceTreeReservedRanges`、`DiagnosticMemberRanges`、`ReservedMappedRanges`、`StartupObjectPmmReservedRanges(p)`、`ExplicitPolicyDeniedRanges`(第一版 = ∅) | 具名成员，来源可追溯 |
| **禁止集合** | `BootMapDenyRanges`(L3)、`FinalMapDenyRanges`(L2) | 按树与路径选判据，不是同一件事的两种写法 |
| **义务集合** | `StartupAccessRequirements(树,阶段)` 及其派生 `StartupMappingRequirements`、`DeviceMappingRequirements(树,阶段)` | 回答「必须怎么访问」，不回答「映射从哪里来」 |
| **派生视图（无独立存储）** | `PrefixRangeView`、`MappingDeniedSet`/`MappingDeniedForPath` 的取值、各集合的相交结果 | **不得进容量清单冒充缓冲区**（§5.3 那条纪律） |
| **有界样本（可截断）** | §5 的诊断样本 | 给人看的那一份，不承载政策结果 |

## 十四、诊断字段表（原正文 §5.2，第二十三轮移入）

| 字段 | 意义 |
|---|---|
| `step` | 落在六步裁决的哪一步 |
| `disposition` | 可用 / 保留映射 / 诊断成员 / 排除 / 设备路径 / 致命 |
| `reason` | 具体政策原因，例如类型排除、独立属性命中、缺普通属性准入 |
| `raw_type` | 原始类型 |
| `physical_start` | 原始物理起点 |
| `number_of_pages` | 原始页数 |
| `attribute` | 完整原始属性字 |

## 十五、四份材料的读数矩阵（第二十三轮移入）

| 材料 | 发布身份 | `WP` | `MORE_RELIABLE` | `HOT_PLUGGABLE` | `ISA_MASK` |
|---|---|---|---|---|---|
| UEFI 2.10 原始版 PDF | 2022 年 8 月正式发布 | `0x1000` | `0x10000` | 未定义 | `0x0FFF000000000000` |
| UEFI 2.10 Errata A PDF | 2024 年 8 月勘误发布 | `0x100` | `0x1000` | 未定义 | `0x0FFFF00000000000` |
| UEFI 2.11 PDF | 2024 年 12 月正式发布 | `0x100` | `0x1000` | `0x10000` | `0x0FFF000000000000` |
| EDK2 当前主干头文件 | **实现，不是规范勘误** | `0x1000` | `0x10000` | `0x100000` | `0x0FFFF00000000000` |

## 十六、第四十四轮：fdt 0.1.5 的消费能力读数（属性序与深度）

依赖源码：`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fdt-0.1.5/src/node.rs`

| 位置 | 读数 | 后果 |
|---|---|---|
| `properties()`（:53） | 仅当前 token 为 `FDT_PROP` 时产出，否则 `done = true` | 属性后置对属性查询不可见 |
| `children()`（:81） | 先跳过前导属性，之后只接受 `FDT_BEGIN_NODE`，否则 `done = true` | 子节点后属性**终止兄弟迭代**，子树被静默截断 |
| `all_nodes()`（:395） | `parents: [&[u8]; 64]`，`parent_index` 从 0 起、先加一后写入 | 根存下标 1，**深度 63 是可走完的上限**；深度 64 越界 |

QEMU 实测（`qemu-system-riscv64 -M virt,dumpdtb=/tmp/virt_rv.dtb -smp 4 -m 256M`，QEMU 8.2.2）：`totalsize=5734`、最大深度 5、属性后置 0 处。

门读数（提交 `d615d0190`）：minix-boot **114**、boot-shim **67**；`test-kernel-image-riscv64-bootface.sh` PASS（OpenSBI 打补丁链）、`test-shim-bootmarks-aarch64.sh` PASS；`cargo check -p boot-shim --target x86_64-unknown-uefi` 通过。

## 十七、第四十五轮：来源记录层的容量与门读数

| 项 | 读数 |
|---|---|
| 记录容量 | `DTB_SOURCE_MAX = 8 + 8 + 16 + 8 = 40`（memory、prefix、MRB、reservation 的上限之和）；两张投影表装得下的输入这里一定装得下 |
| 原始范围 | `raw_*` 不取整不裁剪；`page_*` 为投影用的向外取整范围（MRB 测试：`raw [0x8000_1000, 0x8000_1800)`、`page [0x8000_1000, 0x8000_2000)`） |
| 新测试 | `dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span` |
| 门 | minix-boot **118**、boot-shim **67**；`test-kernel-image-riscv64-bootface.sh` PASS、`test-shim-bootmarks-aarch64.sh` PASS（exit 0）；`cargo check -p boot-shim --target x86_64-unknown-uefi` 通过 |
| 行为不变 | `collect()` 仍返回两张表；记录层是附加输出，`bootface`/`opensbi`/`uefi` 调用点未改 |

## 十八、第四十六轮：记录模型、容量负控与外部红

| 项 | 读数 |
|---|---|
| 记录结构 | `SourceRecord { id: u16, kind, origin, raw_start, raw_end }`，无页范围；`kind` 六类（含 `ReservedReusable`）；`origin` 四类（含 `MrbEntry(n)` / `ReservedMemoryChild(n)`） |
| 容量 | `DTB_SOURCE_MAX = 8 + 8 + 16 + 8 = 40`，由输入上限推出；负控 `dtb_no_map_declarations_count_against_the_declaration_cap` 构造 9 条 RAM 外 `no-map`（不进任何投影表）并确认第 9 条被拒（`TooManyReservedRegions`），证明容量不是从投影表反推 |
| 入口一致性 | `dtb_collect_and_collect_with_sources_agree`：两条入口的两张表逐项相同 |
| 生产接入 | `grep -rn 'collect_with_sources' --include=*.rs os`（排除库自身）零命中；四条 DTB 调用点均 `collect()` |
| 门 | minix-boot **120**、boot-shim **67**；riscv bootface PASS（exit 0）；`cargo check -p minix-boot --target aarch64-unknown-none`、`-p boot-shim --target aarch64-unknown-uefi`、`-p boot-shim --target x86_64-unknown-uefi` 均通过 |
| aarch64 QEMU | **阻断**：`xtask image assembly failed`；`error[E0046]: not all trait items implemented, missing: sendrec`，`os/servers/rs/src/lib.rs:203`，由 `2d292a06b`（sef E-NETSTART 切片）引入，与本切片无关 |

## 十九、第四十七轮：输入/派生分层与容量可达性

| 项 | 读数 |
|---|---|
| `class()` | `FirmwarePrefix` → `DerivedReservation`；其余五类 → `InputDeclaration` |
| origin | `MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`、`MrbEntry(n)`、`FirmwarePrefix { memory_id }` |
| 容量可达 | `dtb_source_capacity_is_reachable_at_exactly_forty`：8 段 `/memory` + 8 条前缀（`fw_end` 覆盖每个段首）+ 16 条 MRB + 8 条声明 = 40，`collect_with_sources` 成功且各类计数相符 |
| 共用上限 | `dtb_declaration_cap_is_shared_across_reservation_kinds`：普通/reusable/no-map 各 3 条（共 9）被拒（`TooManyReservedRegions`） |
| 派生链 | `dtb_firmware_prefix_links_to_its_memory_record`：前缀 `memory_id` 指向的正是其切分的 `/memory` 记录 |
| 门 | minix-boot **123**、boot-shim **67**；riscv bootface PASS（exit 0）；aarch64 镜像组装仍 BLOCKED（`E0046`，`servers/rs/src/lib.rs:203`），aarch64 启动行为未验证 |

## 二十、第四十八轮：重复声明裁决与派生参数

| 项 | 读数 |
|---|---|
| 字段 | `range_start/range_end`（中性）；派生前缀的 `range_* = M_i ∩ [0, F)`，`origin = FirmwarePrefix { memory_id, cut: F }` |
| 精确重复（普通保留） | `dtb_identical_reservations_are_deduplicated_not_rejected`：两张子节点同范围同语义 → `reserved` 只有 1 条，来源记录 2 条且 `id` 不同 |
| 精确重复（MRB） | 第四十九轮改判：`dtb_identical_mrb_entries_are_rejected`——同范围两条 → `OverlappingMemoryReservations`（DTSpec 5.3.2 原文「These given regions shall not overlap each other」） |
| 同范围不同语义 | `dtb_same_range_with_different_semantics_is_a_conflict` → `ConflictingReservationAttributes` |
| 反向完整性 | `dtb_every_input_declaration_has_exactly_one_record`：2 段 RAM + 1 条 MRB + 2 条声明 → 恰好 5 条输入记录，origin 集合与输入元组一一对应 |
| 门 | minix-boot **127**、boot-shim **67**；riscv bootface PASS（exit 0）；aarch64 镜像组装仍 BLOCKED（`E0046`），启动行为未验证 |

## 二十一、第四十九轮：MRB 重复改判与四条读证明

| 项 | 读数 |
|---|---|
| MRB 重复 | `dtb_identical_mrb_entries_are_rejected` → `OverlappingMemoryReservations`；规范原文经 spec 页面核对 |
| `/reserved-memory` 重复 | 保持项目级去重（`dtb_identical_reservations_are_deduplicated_not_rejected`） |
| 门 | minix-boot **127**、boot-shim **67**；riscv bootface PASS（exit 0）；aarch64 仍 BLOCKED（`E0046`） |

## 二十二、第五十轮：三语义重复矩阵与 RAM 重叠

| 项 | 读数 |
|---|---|
| 语义枚举 | `ReservationIntent::{Ordinary, Reusable, NoMap}`；同范围跨语义 → `ConflictingReservationAttributes` |
| 新测试 | `dtb_reusable_duplicates_are_deduplicated_not_rejected`、`dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_partial_overlap_of_memory_ranges_is_rejected` |
| 旧夹具修正 | `dtb_multi_region_and_unit_address_variant` 原输入 `(0x8000_0000, 0x4000_0000)` + `(0x8800_0000, …)` 实为重叠；改为 `0x400_0000` 后不相交 |
| 门 | minix-boot **131**、boot-shim **67**；riscv bootface PASS（exit 0）；aarch64 仍 BLOCKED（`E0046`） |

## 二十三、第五十一轮：单节点多 reg 重复与合同修订

| 项 | 读数 |
|---|---|
| 新夹具 | `build_fdt_multi_reg(&[("memory@80000000", &[(0x8000_0000,0x4000),(0x8000_0000,0x4000)])])`：一个节点两个相同 `reg` 元组 |
| 新测试 | `dtb_identical_memory_tuples_are_deduplicated_not_rejected`：`free.len()==1`、两条 `Memory` 记录、origin 分别为 `{0,0}`/`{0,1}` |
| 提交性质 | 仅测试模块改动（diff 单 hunk 在 `mod tests` 内），生产路径未变 |
| aarch64 QEMU | 尝试复跑：`servers/rs` 的 `sendrec` 已由 `b71bce1d0` 修复，但 `servers/sched/src/kernel_api/transport.rs:160` 的 `SefIpcAdapter` 仍缺（`E0046`），镜像组装仍失败；启动行为未验证 |
| 门 | minix-boot **132**、boot-shim **67** |

## 二十四、第五十二轮：纯文档裁决记录

| 项 | 结论 |
|---|---|
| 普通静态 + MRB 同页 | 第一版拒绝；替代路径（放弃 MRB 强化）已写明 |
| 缓存资格 | `CacheAttributeEligiblePages(p)` 进入 PMM 公式 |
| `no-map` / `EfiReservedMemoryType` 内的 DTB 读取 | 第一版拒绝，例外路径未实现 |
| `reusable` owner | 仅声明身份；引用关系集合模型待建 |
| `collect()` MRB 归因 | 明确为旧接口限制，不靠扩表修复 |
| 门 | 无代码改动；沿用 minix-boot **132**、boot-shim **67**；lint 0 |

## 二十五、第五十三轮：歧义属性位与 aarch64 阻断更新

| 项 | 读数 |
|---|---|
| 歧义位 | `0x0001_0000`：PDF=HOT_PLUGGABLE / 仓库=MORE_RELIABLE；实现 `AmbiguousAttribute`，测试 `the_ambiguous_bit_fails_closed_under_either_name` |
| 既有负控 | `0x0010_0000`（仓库=HOT_PLUGGABLE）仍走 `UnknownAttribute`（`hot_pluggable_bit_fails_closed_as_unknown`） |
| 门 | minix-boot **132**、boot-shim **68**；x86_64-unknown-uefi check 通过 |
| aarch64 | 仍阻断：`servers/rs` 已修（`b71bce1d0`），`servers/sched/src/kernel_api/transport.rs:161` 的 `SefIpcAdapter` 仍缺 `sendrec` |

## 二十六、第五十四轮：纯文档合同补强

| 项 | 结论 |
|---|---|
| O-12k 重试 | 预留缓冲 + 纯内存重试 + 不打印日志 + 仅 `EFI_INVALID_PARAMETER` + 上限；五条负控待实现 |
| 配置表存放类型 | Runtime/Boot/Loader/ACPIReclaim/ACPINVS 允许具名只读；Reserved 拒绝 |
| 页粒度冲突 | 字节不相交但同页、类型要求不同 → 拒绝 |
| x2APIC | 独立 MSR 访问域；不实现则锁定 xAPIC 并拒绝 x2APIC |
| PA/VA | 临时保留与重叠判断按 PA；Reachability 为端到端可读 |
| 门 | 无代码改动；沿用 minix-boot **132**、boot-shim **68**；lint 0 |

## 二十七、第五十五轮：属性冲突域与 EFI DTB 类型

| 项 | 读数 |
|---|---|
| 歧义域 | `AMBIGUOUS = 0x1_F000`（`WP/RP/XP/NV/MORE_RELIABLE` 在两套编码下互斥）；负控 `every_contested_attribute_bit_fails_closed` 覆盖 5 个位 × 2 种类型 |
| 代价 | `NON_VOLATILE` 进入拒绝域；`TYPE_INDEPENDENT` 的 NV 分支真实输入不可达 |
| EFI DTB | 标准表必须 `EfiACPIReclaimMemory`（UEFI 2.11 §4.6.1.3 转述）；与静态保留同页拒绝、与 MRB 同页走例外 |
| 门 | minix-boot **132**、boot-shim **69**；x86_64-unknown-uefi check 通过 |
| aarch64 QEMU | 再次尝试：`servers/rs`、`servers/sched` 已修，`servers/vfs/src/main_loop.rs:7635`（`VfsIpc`）仍缺 `sendrec`（`E0046`）；镜像组装失败，启动行为未验证 |

## 二十八、第五十六轮：高位属性钉住与 aarch64 首过

| 项 | 读数 |
|---|---|
| 高位属性 | `WB|0x2_0000` → Allocate（RO 元数据）；`WB|0x4_0000` → Side（SP 独立属性）；`WB|0x8_0000` → Allocate（CPU_CRYPTO）；`WB|0x10_0000` → UnknownAttribute；测试 `high_bit_attributes_follow_the_repository_encoding` |
| 门 | minix-boot **132**、boot-shim **70**；x86_64-unknown-uefi check 通过 |
| aarch64 QEMU | **PASS**（exit 0）：`test-shim-bootmarks-aarch64.sh`；串口 `memmap source: uefi-getmemorymap (… conv=13 dtb_regions=1)` 与 `entering kernel image v1`。外部 `E0046` 三处（rs/sched/vfs）已在 `b71bce1d0`、`ae4ef5c76`、`8dc30f75e` 修复 |

## 二十九、第五十七轮：profile 门控与双快照

| 项 | 读数 |
|---|---|
| 高位属性 | `0x2_0000/0x4_0000/0x8_0000` → `ProfileAttribute`（`profile_only_attributes_fail_closed_without_a_pinned_profile`）；`0x10_0000` → `UnknownAttribute` |
| 连带 | `TYPE_INDEPENDENT` 仅 `RUNTIME` 可达；NV/SP 分支不可达（已在文档注明） |
| 门 | minix-boot **132**、boot-shim **70**；x86_64-unknown-uefi check 通过 |
| aarch64 QEMU | 门控落地后**再次 PASS**（exit 0）；日志 `diagnostic only` 不等于配置表 DTB 首读授权已验证 |

## 三十、第五十八轮：纯文档更正记录

| 项 | 结论 |
|---|---|
| 属性证据 | 2.10 原始版含高位 RO/SP/CPU_CRYPTO；Errata A/2.11 移位。上一轮表述已撤回 |
| profile 解锁 | 需「钉 profile + 改无条件拒绝」两步；仅加 profile 不解锁 NV |
| 快照身份 | UEFI `InputObjectId = MapSnapshotId`；Preflight 不产记录；重试整体废弃 |
| 容量失败 | 三位置三分；仅 EBS 后重取坚持不扩容 |
| 门 | 无代码改动；沿用 minix-boot **132**、boot-shim **70**；lint 0 |

## 三十一、第五十九轮：纯文档

| 项 | 结论 |
|---|---|
| O-06 可达性 | 当前只有 `RUNTIME` 可达；NV/SP 均 fail-closed（旧句已撤） |
| 属性矩阵 | 三列九项完整表；PDF 低位 `0x100–0x800` 落未知位 |
| PMM | 补 `ArchMemoryAccessEligiblePages`；`WbConfigurablePages` 表能力；平台拒绝单列 |
| `/reserved-memory` | cells/`ranges` 拒绝为项目输入子集，非 DTSpec 格式错误 |
| 门 | 无代码改动；沿用 minix-boot **132**、boot-shim **70**；lint 0 |

## 三十二、第六十轮：错误分类与架构准入

| 项 | 读数 |
|---|---|
| 新错误 | `MissingReservedMemoryRanges`、`UnsupportedReservedMemoryRanges`、`MissingReservedMemoryCells`、`MismatchedReservedMemoryCells` |
| 新测试 | `dtb_missing_reserved_memory_ranges_is_rejected`、`dtb_missing_reserved_memory_cells_is_rejected`；「宽 cells」用例改判 mismatch |
| 门 | minix-boot **134**、boot-shim **70**；riscv bootface PASS（exit 0）；x86_64-unknown-uefi check 通过 |

## 三十三、第六十一轮：纯文档

| 项 | 结论 |
|---|---|
| UEFI Reserved 语义 | 规范层只写 `NotUsable`（待核：强访问禁令说法）；项目政策三列分开 |
| RISC-V 准入 | 页表读写 + `RsrvEventual` + PMP（逐 hart、隐式页表访问）；AMO/LR-SC 分列 |
| PMP 证据 | M-mode 提供者或 profile 承诺；禁止探测式全区间证明；次级 hart 入场门槛 |
| 共享语义 | `OrdinaryCachePolicyFeasiblePages` 取代 `WbConfigurablePages` |
| 范围端点 | `PhysicalRange { start: u64, end_exclusive: u128 }` 统一 |
| 门 | 无代码改动；沿用 minix-boot **134**、boot-shim **70**；lint 0 |

## 三十四、第六十二轮：纯文档

| 项 | 结论 |
|---|---|
| PMM | `PmmEligible(page, role, p)` 四用途；全局集合与用途化二选一（选用途化） |
| 缓存 | `OrdinaryCachePolicyRepresentablePages`（PMM）vs `ObservedMappingEffectiveAttribute`（MappingAudit） |
| RISC-V | PTE 更新方案条件化；Svade 不要求 RsrvEventual；hart 方案一致性 |
| UEFI Reserved | `DenyOsUse` 三层；不设硬件屏障 |
| AArch64 | 64 KiB 分组属性规则纳入审计适用性 |
| 门 | 无代码改动；沿用 minix-boot **134**、boot-shim **70**；lint 0 |

## 三十五、第六十三轮：两层 PMM 代数与动态保留拒绝

| 项 | 读数 |
|---|---|
| 代码落点 | `os/libs/minix-boot/src/memmap_source.rs`：枚举新增 `UnsupportedDynamicReservation`；`collect_inner` 的 `/reserved-memory` 子节点形状分流；测试夹具 `build_fdt_child(extra, emit_reg)`，`build_fdt_full_opts` 增 `emit_reg` / `extra` 两参 |
| 新测试 | `dtb_size_only_reservation_is_rejected_as_dynamic`、`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`、`reg_and_size_together_uses_reg_and_ignores_size`（断言来源记录范围是 `reg` 的 `0x8000_4000..0x8000_5000`，不是 `size` 请求的 `0x2000`） |
| `cargo test -p minix-boot --lib` | 137 passed; 0 failed |
| `cargo test -p boot-shim --lib` | 70 passed; 0 failed |
| QEMU riscv64 | `bash qemu-tests/test-kernel-image-riscv64-bootface.sh` → `### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###`（B1/B2/B3 三点位） |
| QEMU aarch64 | `bash qemu-tests/test-shim-bootmarks-aarch64.sh` → `### TEST_RESULT: PASS test-shim-bootmarks-aarch64 ###`；串口 `boot-shim: memmap source: uefi-getmemorymap (dtb /memory cross-checked, diagnostic only; conv=13 dtb_regions=1)` |
| 交叉编译 | `cargo check -p boot-shim --target x86_64-unknown-uefi`、`cargo check -p minix-boot --target riscv64gc-unknown-none-elf`、`cargo check -p minix-boot --target aarch64-unknown-none` 均 Finished |
| 主文档 | 1110 行（上限 1700）；代码围栏计数 22（偶数） |
| 规范取证 | RISC-V `menvcfg.ADUE` 段与「Memory ordering PMA」句由检索到的全文镜像取得，**逐字与 PDF 定位待复核**；DTSpec 的 `size` 独有=动态请求、`reg`+`size` 按 `reg`、动态区不入 UEFI 内存图三句本次**未取到原文**（readthedocs 与 devicetree-org.github.io 的多个候选地址返回 404 或 fetch failed），故本轮按项目输入子集落地 |
| AArch64 64 KiB | Table 2.5 / 2.6 的字段划分沿用先前轮次的记录，本轮只做结构化收紧：分组键、比较字段、profile 适用性、未映射子页；未新增规范断言 |
| QEMU x86_64（同轮补记） | `bash qemu-tests/test-shim-bootmarks-x86.sh` → `### TEST_RESULT: PASS test-shim-bootmarks-x86 ###`；串口 `boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)`，即该腿不解析 ESP DTB，`UnsupportedDynamicReservation` 分支未被执行 |


## 三十六、第六十四轮：主源取证与本轮读数

| 项 | 读数 |
|---|---|
| 主源取法 | `curl` 直取规范仓库原文（不经二级镜像）：`raw.githubusercontent.com/riscv/riscv-isa-manual/main/src/priv/supervisor.adoc`（2079 行）、`.../src/priv/machine.adoc`（3639 行）、`raw.githubusercontent.com/devicetree-org/devicetree-specification/main/source/chapter3-devicenodes.rst`（1010 行）、`.../source/chapter5-flattened-format.rst` |
| RISC-V 关键句 | `supervisor.adoc:1653` 锚点 `norm:pte_ad_scheme_uniform`「All harts in a system must employ the same PTE-update scheme as each other.」；`:1604` Svade 页故障定义；`:1608` 「When the Svade extension is not implemented, the following scheme applies」；硬件更新方案要求「The page tables must be located in memory with hardware page-table write access and *RsrvEventual* PMA」 |
| RISC-V ADUE | `machine.adoc:2194-2216`：`norm:menvcfg_adue_op`、`norm:menvcfg_adue_rdonly0`、附注「hence Svade is implemented when ADUE=0」 |
| RISC-V PMA | `machine.adoc:3052-3074` Reservability 三级 + `Ziccrse` 要求；`:3122-3133` 「Coherent main memory regions always have either the RVWMO or RVTSO memory model. Incoherent main memory regions have an implementation-defined memory model.」；`:3258-3292` Idempotency PMAs（主存假定幂等、I/O 读写分别规定、非幂等禁止提前/推测隐式访问）——**无 `Non-resident` 这一 PMA 名** |
| DTSpec | `chapter3-devicenodes.rst:266-300`（reg 或 size、`alignment`/`alloc-ranges` 约束动态分配、「If both `reg` and `size` are present … `reg` property taking precedence and `size` is ignored」、`no-map`/`reusable` 互斥）；`:236-260` 父节点三项 Usage=R + 「should use the same values as for the root node, and `ranges` should be empty」；`:372-382` 静态保留入内存图（`no-map`→`EfiReservedMemoryType`、其余→`EfiBootServicesData`）与「Dynamic reserved memory regions must not be listed in the UEFI memory map」；`chapter5-flattened-format.rst:229`「These given regions shall not overlap each other」、`:246-247` MRB 用 `EfiReservedMemoryType` 是 *should* |
| 未取证 | UEFI 2.11：`curl -I https://uefi.org/specs/UEFI/2.11/…` 返回 **403**，AArch64 平台绑定与 Table 2.5/2.6 分工、§4.6.1.3 的 DTB 表类型要求仍沿用评审人核对结论，标为**本方未取证** |
| 主简报 | 1183 行（上限 1700）；代码围栏 26（偶）；表格列数不一致 0 处 |
| 门 | 本轮无代码改动：minix-boot **137**、boot-shim **70**、riscv bootface 与 aarch64/x86_64 shim 三条腿沿用上一笔读数；纯文档提交按 lint/表格/行数三门 |


## 三十七、第六十五轮：A/D 分支式与端点分名

| 项 | 读数 |
|---|---|
| 代码落点 | `os/libs/minix-boot/src/memmap_source.rs`：新枚举 `UnsupportedPhysicalAddressEnd`；`const PHYS_END: u128 = 1u128 << 64`；`physical_end(start, len)` 三分；`declared_page_range` 改用它并对「向外取整落到 2^64」同名单列；`/memory` 循环由 `start.checked_add(len)` + 三处 `start + len` 改为单个已验证的 `end` |
| 夹具改动 | `build_fdt_full_opts` 的 `/memory` `reg` 编码由 `[0, base32, 0, size32]` 改为 `[hi(base), lo(base), hi(size), lo(size)]`——2^32 以下逐字节等价，因此旧用例不变；这是能表达顶端范围的必要条件 |
| 新测试 | `dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end`（末端恰为 2^64）；`dtb_top_partial_page_reservation_is_refused_as_unsupported_end`（末端落在最高页内 1 字节，取整后才触界）；`dtb_memory_range_past_2_64_is_range_overflow`（越过 2^64 仍归畸形） |
| 编写过程中的两处自误 | 第一版三个用例把底址写成 `0xffff_ffff_f000`（48 位，末端是 2^48，用例根本不会触发边界）；夹具 hi/lo 忘记截成 `u32` 导致 `reg` 变成 16 字节、19 个用例连带失败。两处都由测试读数当场暴露后修正 |
| `cargo test -p minix-boot --lib` | 141 passed; 0 failed |
| `cargo test -p boot-shim --lib` | 70 passed; 0 failed |
| 交叉编译 | `cargo check -p minix-boot --target riscv64gc-unknown-none-elf`、`--target aarch64-unknown-none`、`cargo check -p boot-shim --target x86_64-unknown-uefi` 均 Finished |
| QEMU（串行） | riscv bootface `### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###`；aarch64 `### TEST_RESULT: PASS test-shim-bootmarks-aarch64 ###`（`conv=13 dtb_regions=1`）；x86 `### TEST_RESULT: PASS test-shim-bootmarks-x86 ###`（`no dumpdtb.dtb on ESP`，不进入采集器） |
| 主简报 | 1265 行（上限 1700）；围栏 30（偶）；表格列数不一致 0 处；lint error 0 |
| 规范钉法 | RISC-V：official release 入口 `v20260120`，模块 Supervisor ISA 1.13 / Svade 1.0 / Svadu 1.0 / Machine ISA 1.13，源 commit `riscv/riscv-isa-manual@418bdc4990ae2d60a86f1e11e8d7795aa279da61`；DTSpec：`devicetree-org/devicetree-specification@8314f1e1c9965482e5c1080d4118f52fef827fce`（在线 readthedocs 页面 `unknown-rev`，不作冻结版本引用）；RVA20/22/23 与 UEFI 2.11 未取（403），列为本方未取证 |
| 一处流程事故 | 后台 QEMU 腿的日志写在 WSL `/tmp`，`/tmp` 在两次调用之间被回收，日志连 `TEST_RESULT` 一起丢失；后台命令的退码来自管道尾部的 `tail`，`grep` 找不到文件也报 0——因此 aarch64/x86 两条腿改为前台重跑并直接读输出，本轮三条腿读数全部来自实际打印 |


## 三十八、第六十六轮：版本状态回写与新合同

| 项 | 读数 |
|---|---|
| 本轮评审对照的版本 | 评审意见引用的「当前公式」是**第六十四轮的布尔式**（逐字引用已在第七十二轮材料清理中删除；第六十五轮 `bd8a89eee`/合并后 `2eca58006` 已改分支式），并称「无 `SystemHartSet`」；两者都不在现行主文档 |
| 合并态锚点（`2eca58006`） | `PteUpdateScheme(hart) =` 分支在约 1032 行；`SystemHartSet`/`ParticipatingHartSet` 在约 1074 行；`HartRuntimeAdmission` 在约 1122 行、既存对象段在约 1126 行；`grep -c 规范原文未取到` = 0 |
| 本轮修掉的呈现缺陷 | 撤回说明曾把被撤销的布尔式逐字留在正文（`grep` 计数 1），检索会命中；本轮删除，正文只留「上一版是布尔式、已撤销」，逐字记录移入本档案第六十五轮块 |
| 新增合同 | `SystemHartSet` 清单来源（固件 ABI / DT `/cpus` / ACPI MADT）与「永不启用」政策；`HartRuntimeAdmission` 增映射与访问路径维度；`ProfileMemoryObligations(isa_profile)` 与 `PlatformMemoryProfile` 分离并补 `Ziccif`；`AtomicFallbackPlan` 五字段与两条覆盖要求 |
| 口径更正 | 顶端页：上一轮写「项目政策拒绝」，本轮改为「记录层应接受 `== 2^64`，今天拒绝是 `u64` 记录层的临时限制，`UnsupportedPhysicalAddressEnd` 将来只属投影层」——与 O-11 的 `PhysicalRange` 目标一致 |
| 证据更新 | DTSpec 记站点 revision `v0.4-12-g8314f1e`（本方只核 commit 前缀 8314f1e 一致，tag 串未独立核）；UEFI §4.6.1.3 记为「评审人核对 / 本方 403 未取证」两侧分开，并补「固件不修改 ≠ 应用不修改」边界 |
| 门 | 无代码改动：minix-boot **141**、boot-shim **70**、三条 QEMU 腿沿用 `2eca58006` 读数；主简报 1285 行（上限 1700）、围栏 30（偶）、表格列数不一致 0、lint error 0 |


## 三十九、第六十七轮：版本横幅与访问类别化

| 项 | 读数 |
|---|---|
| 本轮评审对照的版本（推定） | 引用的布尔式与「无 `SystemHartSet`/无 `AtomicFallbackPlan`」对应 `2eca58006` 之前；`7aeea8964` 中这四项均在文（第 1032 行分支式附近的负控表、1074 行 SystemHartSet、1126 行既存对象、995 行 AtomicFallbackPlan） |
| 结构性对策 | 主文档标题下新增**自指**版本横幅：以 `git log -1 --format=%h -- <本文件>` 取「本文件最近一次改动的提交」——硬编码哈希会在提交后立刻过期，自指形式才是长期可用的核对锚点 |
| 旧符号清零 | `grep -c 'fallback_available'` = 0、`grep -c 'HartAdmittedBytes'` = 0，旧布尔中间量的名称与公式也已从全部材料删除——正文不再保留已撤销符号的原文，避免检索误命中 |
| 本轮新增 | `AccessRequirement`/`HartAccessEligibleBytes(hart, req)`/`RoleRequirements(role, ctx)`；`Ssccptr` 与「第一版不声明 RVA 合规 ⇒ 空集≠已满足」；`AtomicFallbackPlan.required_memory` 与锁/状态路径准入；O-11 第三条硬约束；AArch64 `EfiReserved`↔`EfiReservedMemoryType` 映射；§7 逐项签字视图；`SourceRecord` 变体拆分候选登记 |
| 主简报 | 1318 行（上限 1700）；围栏 30（偶）；表格列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **141**、boot-shim **70** 复跑；QEMU 腿沿用 `7aeea8964` 读数 |


## 四十、第六十八轮：页表准入入口与版本标识

| 项 | 读数 |
|---|---|
| 本轮评审自述依据 | 「提交 `f52730bef`」「137/70」——`f52730bef` 为第五十九轮前后的提交；当前尖子 `9cf7f574d`，测试 **141/70**。第三次按旧文本评审 |
| 版本标识加强 | 横幅从「提交号」扩为「提交号 + **内容轮次**」：提交号需仓库、轮次只需读文本即可判断，离线材料也有判据 |
| 本轮新增 | `PageTableRoleAdmission(page, ctx)`（PMM 与对象审计两个消费者）；`RamSourcePageDomain = FullPages(Union(ResolveCompatibleClaims(RamSourceDomain)))`；Profile 清单非全集（`Za128rs`/`Zicclsm` 提醒）；§7 新增「页表用途准入入口」行 |
| 判为已解决（本轮 `grep -n` 实读，不是估算） | `PteUpdateScheme(hart) =` **1065**；`SystemHartSet =` **1107**；`PageTableRoleAdmission(page, ctx)` **1022**；`HartAccessEligibleBytes(hart, req) =` **1142**；既存存活对象 **1173**；`AtomicFallbackPlan {` **990**；非一致主存裁定（`MemoryOrderingCompatiblePages(ctx)` 条 ②）**1030** |
| 主简报 | 1331 行（上限 1700）；围栏 32（偶）；表格列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **141**、boot-shim **70** 复跑；QEMU 腿沿用 `9cf7f574d` 读数 |


## 四十一、第六十九轮：PhysicalRange 落码

| 项 | 读数 |
|---|---|
| 代码落点 | `os/libs/minix-boot/src/memmap_source.rs`：新增 `pub struct PhysicalRange { start: u64, end_exclusive: u128 }`（`new` / `len` / `projectable_end`）；`SourceRecord.range: PhysicalRange`；`physical_end` 返回 `u128` 且只判 `> 2^64`；`push_source` 收 `PhysicalRange`；`/memory`、FirmwarePrefix、MRB、`/reserved-memory` 四处构造记录都改用它 |
| 行为差异 | **无**：最高页仍在投影层被 `UnsupportedPhysicalAddressEnd` 拒绝（`projectable_end() == None`），错误码与拒绝时机对调用者不变；变的是类型层——记录现在能表达 `2^64`，且拒绝点从「输入求和」移到「投影」 |
| 新测试 | `physical_range_expresses_the_top_page`（最高页可表达、`len() == 0x1000`、`projectable_end() == None`；普通范围 `Some(...)`） |
| 回归读数 | `cargo test -p minix-boot --lib` **142 passed**；`boot-shim` **70 passed**；`grep -c checked_add` 仍为 0 |
| 交叉编译 | `minix-boot --target riscv64gc-unknown-none-elf`、`--target aarch64-unknown-none`、`boot-shim --target x86_64-unknown-uefi` 均 Finished（touch 后确认实际重编） |
| QEMU（串行） | riscv bootface PASS（B1/B2/B3）；aarch64 PASS（`conv=13 dtb_regions=1`）；x86 PASS（`no dumpdtb.dtb on ESP`，不进入采集器） |
| 主简报 | 1331 行（上限 1700）；围栏 32（偶）；表格列数不一致 0 处；lint error 0 |
| 评审事实核对 | 本轮意见仍引用布尔式与 `HartAdmittedBytes`；四项 `grep -c` 仍为 0，锚点与上一轮相同（本轮改动使行号整体后移，锚点以 `grep -n` 现读为准） |


## 四十二、第七十轮：PMP 操作级判定与排除归因

| 项 | 读数 |
|---|---|
| 主源复核 | `curl` 取 `riscv/riscv-isa-manual@418bdc49…` 的 `src/priv/machine.adoc`（3639 行），`sed -n '3575,3600p'` 读出 PMP 优先级段：`norm:pmp_entry_priority`（lowest-numbered entry that matches **any byte** determines）、`norm:pmp_full_match_required`（must match **all bytes** or the operation fails）、`norm:pmp_rwx_check`、`norm:pmp_no_entry_match`，以及规范自带的 `0xC–0xF` / `0x8–0xF` 反例 |
| 纠错 | 上一版「TOR/NAPOT、条目优先级、跨界范围都要**逐字节合成**」与上述规则冲突：PMP 不是逐字节交集，而是「整次访问」判定。公式由 `HartAccessEligibleBytes(hart, req)` 改为 `PmpPermits(hart, req, [a,b))` + `AdmittedRanges(hart, req)`；符号旧名 `grep -c` = 0 |
| 新增 | `ArchitectureAdmissionRejected{…, state}` 与 `DefinitelyIneligible | InsufficientEvidence` 两态；设备窗口三分（`ArchitecturalRegisterAperture`/`PlatformDeviceAperture`/`PageAlignedMappingEnvelope`）与「无包络证据时只映射 4 KiB」的规则；`Zicclsm`；`ADUE=0` 的来源区分；第四行条件改为「两者均被证据证明未实现」 |
| 不变量 | 第 3 条与 O-03 统一：映射禁止与访问禁止各自独立、不得互相代替 |
| 主简报 | 1359 行（上限 1700）；围栏 36（偶）；表格列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **142**、boot-shim **70**；QEMU 腿沿用 `c47108aaa` 读数 |


## 四十三、第七十一轮：来源审计接上启动路径

| 项 | 读数 |
|---|---|
| 代码落点 | `minix-boot`：`SourceAuditError { FreeSpanWithoutMemorySource, ReservedSpanWithoutSource }` + `audit_source_coverage(free, reserved, sources)` + 内部 `records_cover`（贪心并集覆盖）；`lib.rs` 导出 `PhysicalRange`/`SourceAuditError`/`audit_source_coverage`；`os/kernel-image/src/bootface.rs:203` 改 `collect_with_sources()` 并调用审计 |
| 审计规则 | free 投影的每条 span 必须被 `Memory` 记录（可多条接续）覆盖；reserved 投影的每条 span 必须被 `FirmwarePrefix`/`ReservedOrdinary`/`ReservedReusable` 记录覆盖；`MrbEntry`/`ReservedNoMap` 不参与 reserved 投影，符合「no-map 出两表」的既有语义 |
| 新测试 | `source_audit_accepts_the_collector_output`、`source_audit_rejects_a_free_span_without_a_memory_record`、`source_audit_rejects_a_reserved_span_without_a_reservation_record`、`source_audit_chains_adjacent_memory_records` |
| `cargo test -p minix-boot --lib` | 146 passed; 0 failed |
| `cargo test -p boot-shim --lib` | 70 passed; 0 failed |
| QEMU（串行） | riscv bootface（**含新审计**）PASS，B1/B2/B3 全；aarch64 PASS（`conv=13 dtb_regions=1`）；x86 PASS（`no dumpdtb.dtb on ESP`） |
| 交叉编译 | `minix-boot` riscv64gc/aarch64-none、`boot-shim` x86_64-uefi 均 Finished |
| 本轮更正 | xAPIC 授权范围 `[0xFEE0_0000, 0xFF00_0000)` = **2 MiB**（上一轮文档写 18 MiB，是算术错误）；IO-APIC `[0xFEC0_0000, 0xFEE0_0000)` 同为 2 MiB |
| 主简报 | 1363 行（上限 1700）；围栏 36（偶）；表格列数不一致 0 处；lint error 0 |
| 证据边界 | 本轮仍无 UEFI 原文（uefi.org 403），也无 M-mode 提供 hart/PMP/A-D 证据的实现；两条 QEMU 腿不覆盖 U-Boot shim 入口 |


## 四十四、第七十三轮：三处准入谓词收敛

| 项 | 读数 |
|---|---|
| DTSpec CPU 状态原文 | `curl` 取 `devicetree-org/devicetree-specification@8314f1e1…` 的 `source/chapter3-devicenodes.rst`，`sed -n '636,670p'` 读出：`"okay"` = The CPU is running；`"disabled"` = The CPU is in a quiescent state（quiescent CPU 可由 `enable-method` 重新启用，「This property is required for CPUs with a status property with a value of `disabled`」）；`"fail"` = The CPU is not operational or does not exist，且「A CPU with `fail` status does not affect the system in any way」 |
| 主文档更动 | ① 页表准入三分（`PageTableBackingEligible`/`PageTableAllocationEligible`/`ExistingPageTableObjectEligible`），旧 `PageTableRoleAdmission` 计数归零；② 公式加入 `PmaPermits`/`PhysicalAccessPermits`/`AllowedOperationStarts`/`AllowedAccessFootprints`/`PagesFullyInside`（末者在第七十四轮撤销），`AdmittedRanges`/`RoleEligibleBytes` 归零；③ `PmpImplementationState` 与有效特权级；④ Profile 义务三分与取指拆分；⑤ `HartInventoryEvidence`；⑥ 旧字段名与 §7 状态表更新 |
| 符号核查 | `PageTableRoleAdmission` = 0、`AdmittedByteRanges` = 0、`RoleEligibleBytes` = 0、`range_start/range_end` = 0；`PageTableBackingEligible` = 4、`PhysicalAccessPermits` = 2、`PagesFullyInside` = 2（第七十四轮归零，改为全称判定式） |
| 主简报 | 1421 行（上限 1700）；围栏 38（偶）；表格列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 读数 |

## 四十五、第七十四轮：整页合格全称谓词与 PMP 权限位取证

| 项 | 读数 |
|---|---|
| 本轮取证的源与版本 | `riscv/riscv-isa-manual@418bdc4990ae2d60a86f1e11e8d7795aa279da61`：`src/priv/machine.adoc`（PMP 与 Smepmp 小节）、`src/priv/supervisor.adoc`（地址翻译算法）、`src/unpriv/zaamo.adoc`、`src/unpriv/zicclsm.adoc`、`src/unpriv/ziccamoa.adoc`、`src/unpriv/ziccif.adoc`、`src/unpriv/ziccrse.adoc`；`riscv/riscv-profiles` 的 `src/rva23-profile.adoc`（blob sha `4fce3ebf34393d7a52564dfab53b69730fab514d`，该文件最近提交 `2852ae1f1bfbc34334fcee8b8345472e4811dfd7`） |
| 命令 | `curl -sL https://raw.githubusercontent.com/riscv/riscv-isa-manual/418bdc4990ae2d60a86f1e11e8d7795aa279da61/src/priv/machine.adoc`，随后 `grep -n "pmp_load_fault\|pmp_store_fault\|pmp_misaligned" machine.adoc`；profile 侧 `grep -n "Ziccif\|Ziccamoc\|Svadu\|Ssccptr" rva23.adoc` |
| 读到的行号（实测，非推算） | `machine.adoc` 3417–3424（三条故障规范）、3568–3575（非对齐行为）、3577–3595（优先级/全匹配/RWX/无条目）、2379–2467（Smepmp 字段）、3320–3337（NOTE 与有效特权级）、3314–3318（粒度）；`supervisor.adoc` 1739–1749（A/D 更新步骤与 store 过 PMP）；`rva23.adoc` 89–106（U64 强制）、162–170（Zacas/Ziccamoc 属 Development Options）、244–256（S64 强制含 Svade/Ssccptr）、359–361（Svadu 属 Expansion Options） |
| 主文档更动 | ① 整页合格改写成 `RolePageEligible` 全称式，新增 `LegalStarts` 双合同与 `legal_starts_contract`；② 拆 `PageRoleEligibility`/`ObjectAccessEligibility`；③ 自然对齐规则与 `a + width` 的 `u128` 约束；④ `RequiredPermission` 权限位表；⑤ Smepmp 只接受 `MML=0 ∧ MMWP=0`；⑥ `Absent` 必须附平台支持限制；⑦ O-11 三方组合表 + 页/字节粒度表；⑧ `EfiReservedMemoryType` 首层改「Not usable」；⑨ APIC 逐逻辑处理器；⑩ §2.1 字段名与顶端页规则、§6 profile 钉版、§7 状态降级 |
| 符号核查（主文档实测计数） | `RolePageEligible` 6、`LegalStarts` 5、`ObjectAccessEligibility` 4、`PageRoleEligibility` 1、`RequiredPermission` 2、`SmepmpUnsupportedConfiguration` 2、`PagesFullyInside` **0**、`Not usable` 4、`逐逻辑处理器` 2 |
| 待核清单（不升级成规范引用） | 硬件 A/D 隐式写的 store-过-PMP 条款**无 norm 锚点**；`EfiReservedMemoryType` 的「Not usable」与 §4.6.1.3、§2.3.6 仍为评审人核对，本环境抓 uefi.org 返回 403；QEMU 构建版本 / machine version / 固件镜像哈希仍未记录；`PmpEvidence` 提供者与 `mseccfg` 读取路径未实现 |
| 主简报 | 1530 行（上限 1700）；围栏 42（偶）；表格 60 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 四十六、第七十五轮：Footprint 顶端边界、取指跨页与 PMP 证据稳定性

| 项 | 读数 |
|---|---|
| 本轮取证的源与版本 | `riscv/riscv-isa-manual@418bdc4990ae2d60a86f1e11e8d7795aa279da61`：`src/unpriv/intro.adoc`（IALIGN 与保护边界取指）；`src/priv/machine.adoc`（PMP 故障规范、Smepmp `RLB`/`MML`/`MMWP`）；`riscv/riscv-profiles` 的 `src/rva23-profile.adoc`（`Za64rs` 几何、RVA23S64 强制项，blob `4fce3ebf34393d7a52564dfab53b69730fab514d`） |
| 命令 | `curl -sL .../src/unpriv/intro.adoc` 后 `grep -n "IALIGN\|protection boundary" intro.adoc`；`grep -n "Za64rs" rva23.adoc` |
| 读到的行号（实测） | `intro.adoc` 452–460（「IALIGN is 32 in the base ISA … relax IALIGN to 16」）、528–531（「when such an instruction borders a protection boundary」，位于 NOTE 内、非编号条目）、537–539（指令按 16 位 parcel 存放）；`rva23.adoc` 97–98（「Reservation sets are contiguous, naturally aligned, and a maximum of 64 bytes」）；`machine.adoc` 2383–2398（`RLB` 可临时修改锁定规则） |
| 主文档更动 | ① `Footprint` 改 `require width > 0 ∧ a < 2^64 ∧ a + width ≤ 2^64`，允许 `end == 2^64`；② `PageTable` 需求拆五项（显式写两种方案都必需）；③ 新增 `InstructionFetchPattern`/`FetchFootprints` 与页级/对象级两条取指规则；④ `ObjectAccessEligibility` 拆物理与映射两谓词，`pa_start` 须独立输入；⑤ MRB 表拆「原始字节相交」与「仅页级投影相交」；⑥ `PhysicalProtectionInventory`/`PlatformProtectionPermits` 进公式，`RLB` 进稳定性证明；⑦ profile 五档校验表与 `ReservationSetRequirement`；⑧ 空集前置条件、`GeneralDataCompleteness`、`CacheBlockZero`；⑨ §2.1/O-11/§6 现状同步；⑩ §7 增四行 |
| 符号核查（主文档实测计数） | `PlatformProtectionPermits` 6、`PhysicalProtectionInventory` 2、`GeneralDataCompleteness` 2、`InstructionFetchPattern` 3、`FetchFootprints` 4、`ReservationSetRequirement` 3、`ObjectMappedAccessEligible` 3、`ObjectPhysicalAccessEligible` 4、`rsrv_eventual_required` 3、`PagesFullyInside` **0**、`RoleEligiblePages` **0** |
| 待核清单（不升级成规范引用） | x86 变长指令可跨页（最长 15 字节）按架构事实使用，未取 Intel SDM 原文；UEFI Table 7.10 与 §4.6.1.3 由评审人独立取到，本方 uefi.org 仍 403；AArch64 Table 2.5/2.6 仍是评审人核对；`PmpEvidence` 的 provider 承诺与变更通知机制未实现 |
| 主简报 | 1617 行（上限 1700）；围栏 46（偶）；表格 62 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 四十七、第七十六轮：基底/用途两层、硬件 PTE 更新、cache-block 许可规则

| 项 | 读数 |
|---|---|
| 横幅时间实测 | 命令 `date "+%Y-%m-%d %H:%M %Z (%z)"` → `2026-11-05 18:54 CST (+0800)`。上一轮横幅的 `20:35` 未实测，超前约 1 小时 43 分，已更正；规则改为「横幅时间必须由该命令实测」 |
| 本轮取证的源与版本 | `riscv/riscv-isa-manual@418bdc4990ae2d60a86f1e11e8d7795aa279da61`：`src/unpriv/cmo.adoc`（cache-block 许可与块级一致性）；`src/priv/supervisor.adoc`（A/D 更新步骤的原子性）；`src/priv/machine.adoc`（`norm:pmp_cbm_fault` 指向 CMO 章节） |
| 命令 | `curl -sL .../src/unpriv/cmo.adoc` 后 `grep -n "norm:pmp_same\|norm:cbm_access\|norm:cbz_access\|norm:cbo_rsv" cmo.adoc` |
| 读到的行号（实测） | `cmo.adoc` 319–321（`norm:pmp_same`：块内 PMP 位一致且 W⇒R）、323–325（`norm:pma_same`）、327–328（`norm:cbo_rsv`：不满足则 UNSPECIFIED）、336–345（CMO 访问大小等于 cache block，可被拆成多次操作，逐次 PMP 检查）、352–362（`norm:cbm_access`：load **或** store 许可即允许；`norm:cbm_unperm_fault`）、373–381（`norm:cbz_access`：store 许可且 PMA 支持）、383–389（`norm:cbp_access`：预取不产生异常）；`supervisor.adoc` 1745–1748（A/D 更新「Perform the following steps atomically: Compare … Then store」） |
| 主文档更动 | ① `RoleAdmit ⇔ RolePageEligible` 单一展开式，`U ≠ ∅` 进 `PmmRoleEligible`；② 拆 `BaseAccessRequirements` / `RoleSpecificRequirements`，`GeneralData → ∅` 成为定义；③ 新增 `ImplicitHardwarePteUpdate` 与 `AccessRequirement.atomicity`；④ `PageTableBackingEligible` 补「显式 PTE 读写与方案无关」；⑤ `ObjectAccessUse` + `ActualFootprints` + 区间级 `Resolve`；⑥ 合同 A 补「完整 footprint 约束」并给三类具体化；⑦ 稳定性改「不可变 / 变更前协调」二选一；⑧ `ReservationSetGeometryPermits` 进原子准入；⑨ `CacheBlockManagement` 入枚举 + 块级一致性规则；⑩ 负控与 §7 增行 |
| 符号核查（主文档实测计数） | `RoleRequirements` **0**、`VAResolution` **0**（旧名已清）；`BaseAccessRequirements` 6、`RoleSpecificRequirements` 5、`ImplicitHardwarePteUpdate` 4、`ActualFootprints` 4、`ReservationSetGeometryPermits` 4、`CacheBlockManagement` 2 |
| 待核清单（不升级成规范引用） | cache-block 的 PMA 支持位（`norm:cbz_access` 提到的 supported access type）由平台 profile 提供，本方未取具体 PMA 编码；x86 变长指令跨页仍按架构事实；UEFI 侧仍是评审人取证；`PmpEvidence` 的不可变承诺协议未实现 |
| 主简报 | 1663 行（上限 1700）；围栏 46（偶）；表格 62 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 四十八、第七十七轮：政策授权层、隐式页表读与架构边界

| 项 | 读数 |
|---|---|
| 本轮是否新增规范取证 | 无新增抓取；复用上轮已取锚点（`cmo.adoc` 的 `norm:cbm_access`/`norm:cbz_access`/`norm:pmp_same`/`norm:pma_same`/`norm:cbo_rsv`；`supervisor.adoc` 1745–1748 的原子更新步骤）。本轮改动全部由简报内部一致性推出 |
| 评审人本轮独立核对（第三方） | UEFI 2.11 属性位低位组 `WP=0x100`/`RP=0x200`/`XP=0x400`/`NV=0x800`，与 2.10 原始版高位组不同；标准 DTB 位于 `EfiACPIReclaimMemory`、按物理地址解释、固件安装后不得修改（UEFI 应用可改）；`EfiReservedMemoryType` 类型表字面「Not usable」。本方环境抓 uefi.org 仍 403，未复现 |
| 主文档更动 | ① `ObjectPolicyAccessEligible` 政策层 + 三层分工（来源政策 / 硬件准入 / 映射兑现）；② `PageTable` 补回 `{ (ImplicitPageWalk, Read, PteSize, PteAlignment) }` 并给显式读写绑宽度；③ `RoleRequirementsComplete`/`Applicable` 与非空前置、`ctx.ParticipatingCpuSet ≠ ∅`；④ `AtomicRoleEligible` 四项合取 + 第一版「LR/SC 一律要 `RsrvEventual`」；⑤ `ProfileAdmissionValid` / `ProfileMemoryEligiblePages`；⑥ `MemoryAdmissionContext` 身份三分；⑦ `ExpectedTranslationSegments` 分段映射与取指实例宽度；⑧ 共享层/adapter 三条硬规则；⑨ `MappingAudit` 补正向授权与负向禁令；⑩ §7 把三项 P0（政策授权层、隐式页表读、需求完整性）列为显式阻塞项 |
| 压缩记录 | 用途 role 表（6 行）收成 1 行；`ImplicitHardwarePteUpdate`/`AtomicFallbackPlan`/`HartInventoryEvidence`/`Footprint`/`InstructionFetchPattern`/`PhysicalProtectionInventory`/几何判定共收紧 12 行；净 +1 行 |
| 符号核查（主文档实测计数） | `ObjectPolicyAccessEligible` 2、`ExpectedTranslationSegments` 2、`RoleRequirementsComplete` 3、`RoleRequirementsApplicable` 2、`AtomicRoleEligible` 1、`ProfileAdmissionValid` 2、`ProfileMemoryEligiblePages` 2、`participating_cpu_set_id` 3、`hart_set_id` 1（仅出现在「统一改名」规则句中）、`RequiredHarts` **0**、`ctx.profile_id` **0** |
| 待核清单 | cache-block 的 PMA supported-access-type 读数需平台 profile；AArch64 Table 2.5/2.6 与 UEFI 属性位仍只有第三方核对；`PmpEvidence` 的不可变承诺协议未实现；共享层改名（`ctx.H` 符号、`Pmp*` 谓词归属 adapter）尚未反映到任何代码 |
| 主简报 | 1696 行（上限 1700）；围栏 50（偶）；表格 61 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 四十九、第七十八轮：谓词接入、对象访问闭合与共享层类型

| 项 | 读数 |
|---|---|
| 本轮新增规范取证 | `src/priv/supervisor.adoc` 1701–1705 的 `norm:lr_sc_rsrv_set_page`：「For implementations with both page-based virtual memory and the ext:a[] standard extension, the LR/SC reservation set must lie completely within a single base physical page (i.e., a naturally aligned 4 KiB physical-memory region).」；`src/unpriv/cmo.adoc` 383–389 的 `norm:cbp_access`（prefetch 许可条件与「不许可时不产生异常、不访问内存」）。命令：`grep -n "reservation set" supervisor.adoc`、`grep -n "cbp_access" cmo.adoc` |
| 评审人本轮撤回 | 上一轮「A/D 真值表最后一行与规范冲突」的判断被其本人撤回：区分「Svadu 不存在导致 ADUE 只读 0」与「Svadu 存在且 ADUE 读回 0」是正确的，第四行并非已证错误。本方无需改动该行 |
| 评审人本轮独立核对（第三方） | UEFI `GetMemoryMap` 属 Memory Allocation Services，首次 `ExitBootServices` 失败后再次取图与规范的部分关闭规则不冲突；标准 DTB 位于 `EfiACPIReclaimMemory`、按物理地址解释；`EfiReservedMemoryType` 为「Not usable」；2.11 属性位低位组与 2.10 原始版不同 |
| 主文档更动 | ① `RoleAdmit(AtomicSharedState, …)` 派发 + `AtomicRoleEligible` 逐 CPU 几何；② `AdmissionContextValid` / `ProfileMemoryEligiblePages` 进公式；③ `ActualAccessInstance` + `AccessAlignmentSatisfied` + `RequiredCpus ≠ ∅`；④ `GeneralDataCompleteness` 进 `BasePageEligible`；⑤ LR/SC 基础页内约束与 profile 几何分层；⑥ `operation` 语义名 + `adapter_evidence` 槽位；⑦ `CacheMaintenance{Prefetch}`；⑧ `PolicyProjectionKey`；⑨ O-12g 与 §7 状态同步 |
| 压缩记录 | 删除 5 处被正式定义取代的散文（O-11 两个时点、两半 bullet、访问类别段、历史说明段、标准 DTB 表段），收紧 4 个代码块；净 +1 行（1696 → 1697） |
| 符号核查（主文档实测计数） | `ActualFootprints` **0**（已撤销）、`ActualAccessInstance` 2、`AccessAlignmentSatisfied` 2、`RoleAdmit(AtomicSharedState` 3、`AdmissionContextValid` 3、`ProfileMemoryEligiblePages` 3、`GeneralDataCompleteness` 4、`base_page_containment` 2、`lr_sc_rsrv_set_page` 2、`CacheMaintenance` 3、`PolicyProjectionKey` 1、`NormalRead` 2 |
| 待核清单 | AArch64/x86 adapter 的语义→机制展开表未写；`PmpEvidence` 不可变承诺协议、`ReservationSetGeometryPermits` 的 profile 来源、`NotApplicable` 分支均无实现；x86 变长指令跨页仍按架构事实；UEFI 属性位与 Table 2.5/2.6 仍只有第三方核对 |
| 主简报 | 1697 行（上限 1700）；围栏 50（偶）；表格 61 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 五十、第七十九轮：上下文闭合、对象清单状态与映射意图

| 项 | 读数 |
|---|---|
| 本轮是否新增规范取证 | 无新增抓取；复用已取锚点（`supervisor.adoc` 的 `norm:lr_sc_rsrv_set_page`、`norm:pte_ad_scheme_uniform`；`cmo.adoc` 的 `norm:cbm_access`/`norm:cbz_access`/`norm:cbp_access`/`norm:pmp_same`/`norm:pma_same`；`machine.adoc` 的 PMP/Smepmp 与 PMA 章节）。本轮改动全部由简报内部一致性推出 |
| 评审人本轮独立核对（第三方） | DTSpec：MRB 的「启动程序明确用途」前提、`no-map` 的「禁止标准系统映射 + 限制非受控推测访问」与 `reusable` 互斥、静态保留的 UEFI 类型对应、MRB 的 `should`；UEFI：标准 DTB 位于 `EfiACPIReclaimMemory` 且固件不得修改（应用可改）、首次 `ExitBootServices` 失败后可再次取图但仅限 Memory Allocation Services。本方环境对 uefi.org 仍 403 |
| 主文档更动 | ① `ctx` 补 `legal_starts_contract` 与 `ParticipatingCpuSet = Resolve(...)`；② `ProfileAdmissionState` 四态与中性语义；③ `ObjectAccessUse` 补身份字段、`ObjectAccessInventoryState`、`RequiredCpuScope`；④ 原子拆 `AtomicPageRoleRequirement` / `AtomicAccessInstance`；⑤ `MappingIntent` 进审计；⑥ O-05 改 `MutableMappingMetadata`；⑦ `ArchitectureAdmissionRejected` 补 profile 证据；⑧ `PhysicalProtectionInventory` 补 `scope`；⑨ §7 增「准入上下文闭合」行 |
| 压缩记录 | 在 O-12i 内合并 15 处相邻段落空行（纯格式，未删文字），净 −8 行；主简报 1697 → 1698 行 |
| 符号核查（主文档实测计数） | `ParticipatingCpuSet = Resolve` 2、`legal_starts_contract` 4、`ProfileAdmissionState` 2、`ObjectAccessInventoryState` 2、`RequiredCpuScope` 1、`AtomicPageRoleRequirement` 1、`AtomicAccessInstance` 2、`MappingIntent` 1、`MutableMappingMetadata` 1、`profile_memory_obligation_evidence` 1、`use.access_range` **0** |
| 待核清单 | `Resolve(id, inventory_snapshot)` 的清单来源（固件 ABI / DT `/cpus` / ACPI MADT）未实现；`NotApplicable` 分支、`NoAccess(proof)` 的证据形状、`AtomicAccessInstance` 的 `reservation_set_evidence` 采集路径均未落；AArch64/x86 adapter 的语义→机制展开表未写 |
| 主简报 | 1698 行（上限 1700）；围栏 50（偶）；表格 61 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 五十一、第八十轮：hart 一致性退路、原子唯一展开与对象空集

| 项 | 读数 |
|---|---|
| 本轮是否新增规范取证 | 无新增抓取；评审人本轮按固定提交 `418bdc4990ae2d60a86f1e11e8d7795aa279da61` 复核了 A/D 四分支、`norm:pte_ad_scheme_uniform`、硬件更新方案的页表写能力与 `RsrvEventual`、`norm:lr_sc_rsrv_set_page` 的 4 KiB 页内约束，结论与本方一致 |
| 评审人本轮独立核对（第三方） | UEFI 2.11：标准 DTB 位于 `EfiACPIReclaimMemory`；`ExitBootServices` 因无效 `MapKey` 失败后须重新 `GetMemoryMap`，且固件可能已部分关闭 Boot Services；属性位矩阵 `WP/RP/XP/NV = 0x100/0x200/0x400/0x800` 与 2.11 列一致；AArch64 §2.3.6 的 64 KiB 分组规则确为特定 EFI 类型要求 |
| 主文档更动 | ① hart 一致性负控改「拒绝平台」；② 原子合并为 `AtomicSharedStateEligible` 唯一入口（含 family/进展/几何，AMO-only 几何 `NotApplicable`）；③ `ReservationSetGeometrySatisfied` 页级签名；④ 对象四项空集前置 + `RequiredCpuScope` 二分支 + `physical_footprints` 定义为实际硬件操作 + `AccessAlignmentSatisfied` 进合取 + `access_requirement` 定为模板；⑤ `ProfileAdmissionUsable` / `ProfileConformanceSatisfied` 分离；⑥ `progress_guarantee` 取代 `rsrv_eventual_required`；⑦ `AdmissionContextId` 构造字段；⑧ `UNUSABLE` 独立 TypePolicy 禁止项；⑨ §7 三行按评审建议改写 |
| 压缩记录 | O-01..O-12c 内合并 12 处相邻段落空行（纯格式），净 −12 行；主简报 1710 → 1698 行 |
| 符号核查（主文档实测计数） | `AtomicRoleEligible` **0**（已改名合并）、`AtomicSharedStateEligible` 3、`ProfileAdmissionUsable` 3、`ProfileConformanceSatisfied` 2、`progress_guarantee` 3、`rsrv_eventual_required` **0**、`RequiredCpuScope(use)` 2、`AccessAlignmentSatisfied(use)` 1 |
| 待核清单 | `AtomicRequirementsComplete` / `AccessRequirementsSatisfied` 的证据形状未定；`RequiredCpuScope` 的 `NotApplicable(reason)` 依据未定；`physical_footprints` 的「实际硬件操作」展开由各架构 adapter 提供，尚无实现；`UNUSABLE` 禁止项在代码中仍只靠 `TypeAllocatablePages` |
| 主简报 | 1698 行（上限 1700）；围栏 50（偶）；表格 61 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 五十二、第八十一轮：访问主体证明、footprint 绑定与共享层语义

| 项 | 读数 |
|---|---|
| 本轮是否新增规范取证 | 无新增抓取；复用上轮锚点（`norm:lr_sc_rsrv_set_page`、`norm:pmp_full_match_required`、`norm:pmp_misaligned_access_behavior`、DTSpec CPU `status` 三态） |
| 评审人本轮复核 | 按公开的 RISC-V `v20260120` 发布版页面复核 A/D 分支、系统 hart 一致、4 KiB 预约集页内约束、PMP 整次操作覆盖与半写风险；按 DTSpec 复核 `fail` 的「not operational **or does not exist**」歧义与 `disabled` 需 `enable-method`。结论：本轮无需推翻技术方向，问题在合同是否被唯一、完整接入 |
| 主文档更动 | ① 撤销 `NotApplicable` 逃逸口 → `ObjectAccessSubject = CpuAccess(cpus)`，非 CPU 主体 `UnsupportedAccessActor`；`NoAccess` 需机器可检查证明；② `ActualAccessInstance` 带 `virtual_footprint`/`expected_physical_footprints`/`instruction_or_access_id`/`reservation_set_evidence`，判定式加 `TranslateExpected(...)` 绑定；③ `AccessAlignmentSatisfied` 逐实例，并区分架构有效地址与分解后硬件操作；④ 撤销 `AtomicAccessInstance`，页级与对象级共用 `ReservationSetGeometryCore`；⑤ `AtomicSharedStateEligible` 去掉与通用展开重复的 `AccessRequirementsSatisfied`；⑥ `ExecutionPrivilege` 取代 `{S,U}`，`ctx.H` 量词改 `ParticipatingCpuSet`，`AtomicUpdate` 只留 `family`；⑦ CPU 状态五行表与 `SystemHartSet` 语义选定；⑧ `PageTableBackingEligible` 补排序/Profile 区域义务/内存类别，幂等政策改标为项目政策；⑨ Profile 三层解耦 + `ProfileConformanceSatisfied` 聚合；⑩ O-12k 插入 `RevalidateBootInputObjectsAgainstFinalMap`；⑪ §7 按评审建议改写 |
| 篇幅处理（纪律 8） | 桥接段与其代码块合一（−5）、§7 阻塞组表并 2 行（−4）、三个稳定代码块改单行（−6）、合并剩余 10 处段落空行（−10）、原子策略两段合一（−2）；主简报 1697 → 1695 行 |
| 符号核查（主文档实测计数） | `AtomicRoleEligible` **0**、`AtomicAccessInstance` **0**、`RequiredCpuScope` **0**、`rsrv_eventual_required` **0**、`ctx.H` 6（仅出现在 RISC-V 章节与「adapter 内写法」说明）、`ObjectAccessSubject` 1、`TranslateExpected` 2、`ReservationSetGeometryCore` 3、`ExecutionPrivilege` 1 |
| 待核清单 | 非 CPU 访问主体（DMA/IOMMU/固件）无合同；`NoAccess` 证明来源与 `reservation_set_evidence` 采集路径未定；AArch64/x86 adapter 的 `ExecutionPrivilege` 映射表未写 |
| 主简报 | 1695 行（上限 1700）；围栏 48（偶）；表格 62 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 五十三、第八十二轮：页表唯一合同、原子展开等式与三层访问展开

| 项 | 读数 |
|---|---|
| 本轮是否新增规范取证 | 无新增抓取；评审人用公开 `v20260120` 页面复核 A/D 分支、系统 hart 一致、4 KiB 预约集页内约束、PMP 整次操作覆盖与半写风险，均未推翻本方方向 |
| 主文档更动 | ① `RolePageEligible(page, PageTable, ctx) = PageTableBackingEligible(page, ctx)`，幂等政策进 backing；② `RoleSpecificRequirements(AtomicSharedState) = ExpandAtomic(RegisteredAtomicAlgorithms)` 等式 + `AtomicRequirementsComplete` 改为「每个算法完整展开」；③ `AtomicProgressRequirement` 三分；④ 页级几何加 `∀ a ∈ LegalStarts`；⑤ 对象访问拆 `VirtualAccessInstance → TranslatedAccessFragments → ActualMemoryOperations`，逐操作用自己的 requirement，对齐改名 `ArchitecturalAlignmentSatisfied`；⑥ `ProfileContextState` 改名与 `AllRegionObligationsSatisfied`；⑦ `ArchitectureAdmissionDenial` 归因记录；⑧ `NoAccess` 蕴含空清单 + 审计范围；⑨ `RUNTIME_SERVICES_CODE/DATA` 永久排除政策；⑩ §7 三行按评审建议改写 |
| 符号核查（主文档实测计数） | `ProfileAdmissionState` **0**、`DeclaredAndSatisfied` **0**、`expected_physical_footprints` **0**、`AccessAlignmentSatisfied` **0**（改名后）、`ReservationSetGeometryPermits` **0**（合并进 Core）、`VirtualAccessInstance` 1、`ActualMemoryOperations` 3、`ProfileContextState` 2、`ExpandAtomic` 2、`ArchitectureAdmissionDenial` 1、`AtomicProgressRequirement` 1 |
| 篇幅处理（纪律 8） | §6 版本钉法块并 8 行、「下一轮证据」表转段落并 6 行、§7 相邻行合并 3 行、删备选方案段 1 行、合并 8 处段落空行；1717 → 1692 行 |
| 待核清单 | 统一页表叶子观测记录（三个审计共用）尚未定义字段；`ReservationSetFor(a, algorithm)` 的 adapter 语义未定；`NoAccess` 证明的机器可检查形式未定；`ExpandAtomic` 的完整性证明路径未定 |
| 主简报 | 1692 行（上限 1700）；围栏 48（偶）；表格 61 块、列数不一致 0 处；lint error 0 |
| 门 | 无代码改动：minix-boot **146**、boot-shim **70**；QEMU 腿沿用 `f866b92c3` 之后的读数 |

## 五十四、结构重构批：自正文移入的取证原文与穷举清单

> **本批性质**：主简报体积收缩（结构重构批，不是评审轮）。下列内容**逐字**从 `01` 移出，未改写、未删减；`01` 中被移空的位置改为一句话结论加指针。移出不等于失效——凡属**取证引文、用例穷举、逐轮读数、带行号的定位表**，都以本节为家；`01` 里写「见 `03`」指的就是这里。

### §6 证据与复核：逐轮锚点穷举与复核登记原文

## 6. 证据与复核

| 证据 | 内容 |
|---|---|
| 提交 `38668f843`（**功能落码提交，不是测试基线**） | DTB 保留来源落码：`/reserved-memory`、MRB、容量/回绕/重叠/形状拒绝、引导区 checked 守卫 |
| `cargo test -p minix-boot --lib` | 146 通过（采集器用例见 §2.1） |
| `cargo test -p boot-shim --lib` | 70 通过（分类器用例见 §2.2） |
| 交叉编译 | `x86_64-unknown-none`、`aarch64-unknown-none`（`fw-none-image`）、`riscv64gc-unknown-none-elf`、`x86_64-unknown-uefi`、`aarch64-unknown-uefi` |
| QEMU | `test-kernel-image-riscv64-bootface.sh` PASS（`### TEST_RESULT: PASS ###`，B1/B2/B3 三点位齐全，仅覆盖 bootface 入口）；`test-shim-bootmarks-aarch64.sh` PASS（串口读到 `boot-shim: memmap source: uefi-getmemorymap (dtb /memory cross-checked, diagnostic only; conv=13 dtb_regions=1)` 与内核进入行）；`test-shim-bootmarks-x86.sh` PASS（串口 `boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)`——该腿没有 ESP 上的 `dumpdtb.dtb`，**DTB 采集器根本没被进入**，故它只证明 x86 启动未回归，不构成对本切片新分支的覆盖）。三条腿均在**本轮边界改动之后**重跑：第六十三轮的 `size`-only 与本轮的 `2^64` 分名都不改变 QEMU/OpenSBI 与 EDK2 实际递交的 DTB（其实例子节点都带 `reg`，也没有任何范围触到顶端页），故启动行为不变——这是「未观察到变化」，不是「已验证新分支」；两个新分支目前只有宿主测试覆盖。riscv 腿 B1/B2/B3 点位仍全；aarch64 腿读到 `conv=13 dtb_regions=1` 与内核进入行；x86 腿仍是 `no dumpdtb.dtb on ESP`（不进入采集器） |
| 来源记录层 | `collect_with_sources()` 交出六类 `SourceRecord`（容量 40）；`no-map`、RAM 外声明、MRB 出表后仍可追溯：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`；`collect()` 语义不变；`dtb_collect_and_collect_with_sources_agree` 钉住两条入口不漂移；`dtb_no_map_declarations_count_against_the_declaration_cap` 钉住「RAM 外的 no-map 不进任何表仍占声明容量」；`dtb_reusable_duplicates_are_deduplicated_not_rejected`、`dtb_ordinary_and_reusable_same_range_is_a_conflict`、`dtb_reusable_and_no_map_same_range_is_a_conflict`、`dtb_partial_overlap_of_memory_ranges_is_rejected` 钉住三分裁决 |
| 本轮规范原文取证（主源直取，可复核） | RISC-V：`riscv/riscv-isa-manual` @ `418bdc4990ae2d60a86f1e11e8d7795aa279da61`（`main` 只作阅读入口，不作为冻结版本）`src/priv/supervisor.adoc`（两种 A/D 方案、`norm:pte_ad_scheme_uniform`「All harts in a system must employ the same PTE-update scheme as each other.」、硬件更新方案要求「hardware page-table write access and *RsrvEventual* PMA」）与 `src/priv/machine.adoc`（`norm:menvcfg_adue_op`、`norm:menvcfg_adue_rdonly0`、Memory Ordering PMA、Reservability PMA 三级、Idempotency PMAs）。设备树：`devicetree-org/devicetree-specification` 主分支 `source/chapter3-devicenodes.rst`（`/reserved-memory` 父子节点属性表、`reg`/`size` 优先关系、动态区不入 UEFI 内存图、`no-map` 与 `reusable` 互斥）与 `source/chapter5-flattened-format.rst`（MRB「shall not overlap each other」、以 `EfiReservedMemoryType` 列出是 *should*）。RISC-V 与设备树两处已改为本方直接取证；**UEFI 侧的证据等级本轮上调一档**：`EfiReservedMemoryType` 在类型表（Table 7.10）的字面「Not usable」、`GetMemoryMap()` 对「无物理硬件支撑」与「有物理支撑但 OS 不应访问」的区分、以及 §4.6.1.3 的「标准 `EFI_DTB_TABLE_GUID` 必须位于 `EfiACPIReclaimMemory`、按物理地址解释、运行时不修正、固件安装后不得修改」三项，已由**评审人独立取到 UEFI 2.11 原文并给出章节/表号**；本方环境抓取 uefi.org 仍返回 403，**本方未复现**。因此这三项按「第三方独立核对」引用，不写成「本方已验证」。**仍是缺口**：AArch64 Table 2.5/2.6 的分工只有评审人核对，没有本方或第三方的原文引用。**边界**：规范保证「固件不修改」不等于「所有 UEFI 应用/驱动都不修改」，复制期间源对象的一致性仍须由当前 loader 自己保证 |
| 保留声明形状分流 | `size`-only / 无 `reg` 无 `size` / `reg`+`size` 三种形状各有专名错误与专测（见 O-12g 行 c）；`UnsupportedDynamicReservation` 拒绝的是 **DTSpec 3.5 允许、但第一版不支持**的合法动态保留请求，规范语义已由本方直取原文核实 |
| 边界端点分流 | `address + size` 在 `u128` 里三分：`>2^64` → `RangeOverflow`；记录层的 `range.end_exclusive` 是 `u128`，**恰好等于 `2^64` 现在可表达**；投影侧 `declared_page_range` 向外取整达到 `2^64` 时报 `UnsupportedPhysicalAddressEnd`，`collect()` 构表前再用 `projectable_end()` 复查同一条件；四测试 `dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_top_partial_page_reservation_is_refused_as_unsupported_end`、`dtb_memory_range_past_2_64_is_range_overflow`；`grep -c 'checked_add' …/memmap_source.rs` = 0 |
| 近几轮规范取证（本方直取，锚点即条目名） | `src/priv/machine.adoc` 的 PMP 小节：`norm:pmp_cfg_permissions`（R/W/X 按访问类型许可）、`norm:pmp_rwx_warl`（R=0 且 W=1 为保留编码）、`norm:pmp_exec_fault`、`norm:pmp_load_fault`（load 与 load-reserved 查 R）、`norm:pmp_store_fault`（store、store-conditional、AMO、cache-block zero 查 W）、`norm:pmp_misaligned_access_behavior`（非对齐访问可被拆成多次操作，逐次独立检查；通过检查的那一半 store 可能已可见）、`norm:pmp_check_priv_modes`（有效特权级 S/U 全覆盖，含 M-mode 下 `MPRV=1 ∧ MPP∈{S,U}` 的数据访问）、`norm:pmp_check_pagetable_access`（页表访问的有效特权级为 S）、`norm:pmp_l_bit_m_mode_enforcement`、`norm:pmp_no_entry_match`、`norm:pmp_entry_count`（可零条）、`norm:pmp_granularity` 与 `norm:pmp_a_field_napot_na4`（4 字节粒度）、Smepmp 的 `norm:mseccfg_mml_warl`、`norm:mseccfg_mml_pmpcfg_l_op`、`norm:mseccfg_mml_m_rule_op`、`norm:mseccfg_mml_soru_rule_op`、`norm:mseccfg_mml_shared_l0_op`、`norm:mseccfg_mml_shared_l1_op`、`norm:mseccfg_mml_shared_lrwx_1111_op`、`norm:mseccfg_mml_x_restrict`、`norm:pmp_rw01_rsv`、`norm:mseccfg_mmwp_op_warl`、`norm:mseccfg_rlb_op_warl`、`norm:mseccfg_mml_mmwp_rlb_rst`，以及 PMP 小节那段**无编号 NOTE**「some platforms may provide other PMP structures in addition to or instead of the scheme described in this section」；原子侧 `src/unpriv/zaamo.adoc` 的 `norm:amo_op` 与 `norm:pma_amo_levels`；主存义务 `norm:Zicclsm_misaligned_support`、`norm:Ziccamoa_amo_pma`、`norm:Ziccrse_rsrv_eventual`。**第七十七轮评审人另独立核到** UEFI 2.11 属性位低位组（`WP=0x100`/`RP=0x200`/`XP=0x400`/`NV=0x800`）与 2.10 原始版高位组的差异，与本简报 O-06 的冲突位矩阵一致；本方仍未取到原文。**本轮新增（第七十七轮）**：`src/unpriv/cmo.adoc` 的 `norm:pmp_same`（块内 PMP 位一致且 W⇒R）、`norm:pma_same`、`norm:cbo_rsv`（不满足则行为 UNSPECIFIED）、`norm:cbm_access`（cache-block management 只要 load **或** store 许可）、`norm:cbz_access`（cache-block zero 需 store 许可且 PMA 支持）、`norm:cbp_access`；`src/priv/supervisor.adoc` 1745–1748（A/D 更新「Perform the following steps atomically … Then store _pte_」）。**本轮仍缺的**：`src/priv/supervisor.adoc` 地址翻译算法里「对 PTE 的 store 会先过 PMA/PMP 检查」这句话**没有 norm 锚点**（本方直取原文，按算法步骤引用），因此硬件 A/D 隐式写查 W 的依据强度低于命名规范条目；cache-block 的 PMA supported-access-type 读数需由平台 profile 提供，本方未取；UEFI 与 `EfiReservedMemoryType` 的「Not usable」措辞仍是评审人核对，本方未独立取到原文 |
| FDT 容器校验 | `fdt_layout::validate` **28 条测试**（`accepts_a_canonical_layout`、`accepts_an_unaligned_strings_block`、`accepts_a_property_before_children`、`rejects_a_property_after_a_child`、`rejects_an_unknown_token`、`rejects_a_second_root`、`rejects_unbalanced_end_nodes`、`rejects_an_end_token_in_the_middle`、`rejects_a_property_running_past_the_block`、`rejects_a_property_name_outside_the_strings_block`、`rejects_an_unterminated_property_name`、`rejects_nesting_beyond_the_parser_limit`、`the_parser_consumes_the_deepest_tree_this_gate_accepts`、`rejects_misaligned_blob_address`、`rejects_misaligned_mrb_offset`、`rejects_mrb_outside_the_object`、`rejects_missing_terminator_before_the_struct_block`、`rejects_a_terminator_only_beyond_totalsize`、`rejects_more_reservations_than_the_collector_accepts`、`rejects_struct_block_past_totalsize`、`rejects_overlapping_struct_and_strings_blocks`、`rejects_totalsize_larger_than_the_buffer`、`rejects_a_structure_block_without_the_end_token`、`rejects_unbacked_structure_alignment`、`rejects_unsupported_fdt_versions`、`rejects_a_layout_whose_mrb_follows_the_struct_block`、`rejects_an_mrb_that_starts_inside_the_header`、`rejects_strings_before_the_structure_block`）；五处调用点：`bootface.rs`、`bootface_a64.rs`、`opensbi_helpers.rs`、`uefi_helpers.rs` 两处 |
| 解析库依赖事实 | `fdt 0.1.5` `node.rs`：`properties()` 首个非属性 token 即停；`children()`/`all_nodes()` 遇「子节点后属性」即结束；`all_nodes()` 父栈 64 槽、根在下标 1（最大安全深度 63）。QEMU 8.2.2 `dumpdtb` 全树 0 处属性后置 |

**规范版本钉法（本轮补）**——引用规范要钉**发布版与源 commit**，不能引用活动页面：

```
RISC-V Privileged Architecture
  official release 入口标签: v20260120（docs.riscv.org/reference/isa/v20260120/priv/…）
  相关模块: Supervisor ISA 1.13、Svade 1.0、Svadu 1.0、Machine ISA 1.13
  本方本轮实际读取的源: github.com/riscv/riscv-isa-manual
      @ 418bdc4990ae2d60a86f1e11e8d7795aa279da61  src/priv/supervisor.adoc, src/priv/machine.adoc
  用到的锚点: norm:pte_ad_scheme_uniform、norm:menvcfg_adue_*、norm:pma_rsrv_levels、norm:Ziccrse_pma_rsrv_req、
      norm:Ziccamoa_pma_amo_req、norm:Ziccamoc_pma_amo_req；同 commit 另读 machine.adoc 的 PMP/Smepmp 小节与
      unpriv 的 zaamo/zicclsm/ziccamoa/ziccif/ziccrse（完整锚点清单见 `03`）

RISC-V Profiles
  本方本轮实际读取的源: github.com/riscv/riscv-profiles  src/rva23-profile.adoc
      blob sha 4fce3ebf34393d7a52564dfab53b69730fab514d
      该文件最近一次提交 2852ae1f1bfbc34334fcee8b8345472e4811dfd7
  读到的事实（只作 profile 义务表来源，第一版不声明合规）: RVA23U64 强制 Ziccif/Ziccrse/Ziccamoa/Zicclsm/
      Za64rs/Zic64b/Zicbom/Zicbop/Zicboz；Zacas 与 Ziccamoc 属 Development Options；强制预约集条目是 Za64rs
      （≤64 B），Za128rs 只在文末术语表；RVA23S64 强制 Svbare/Sv39/Svade/Ssccptr/Sstvecd/Ss1p13，
      Svadu 是 Expansion Option。该清单由本方直取，此前「按转述登记」的缺口已撤销

Devicetree Specification
  本方本轮实际读取的源: github.com/devicetree-org/devicetree-specification
      @ 8314f1e1c9965482e5c1080d4118f52fef827fce  source/chapter3-devicenodes.rst,
                                                   source/chapter5-flattened-format.rst
  站点 revision 显示 v0.4-12-g8314f1e（评审人读出，本方只核到 commit 前缀一致）；在线页标 unknown-rev，只作阅读入口
```

**Profile 与 UEFI 仍是缺口**：RVA20/RVA22/RVA23 各自的版本号与能力清单要**另行钉住**，不并入某一份 Privileged ISA；UEFI 2.11 的 §4.6.1.3 与 AArch64 平台绑定（Table 2.5/2.6 分工）本环境抓取返回 403，仍属**本方未取证**。

**下一轮最该补的证据（可复核事实，不是新公式）**：　· x86_64 OVMF 与 aarch64 AAVMF 的实际镜像身份、版本、哈希 **+ 该固件实际返回的代表性描述符及其 `Attribute` 原始值 + 项目接受的完整位解释表**（要证明的是运行固件**实际用了哪套编码**，不是它自报符合哪个规范版本；两条腿的固件也不必同源）　· QEMU 构建版本、machine version、accelerator、CPU 型号、SMP 配置（逐 `PlatformMemoryProfile`）（设备窗口与 PLIC 公式目前仍是 profile 候选值）　· `PreflightMap` / 首次 `FinalMap` 的缓冲容量策略与 `EFI_BUFFER_TOO_SMALL` 处理（三种失败位置的合同已定，缺实现与测试）　· 四条 DTB 输入路径的启动 ABI 合同，特别是「复制完成前原 DTB 不被并发修改」的依据（O-11 的内容一致性条件目前是假设）

**证据分层（编译通过 ≠ 启动验证）**：

| 层级 | 本轮状态 |
|---|---|
| `minix-boot` 宿主单测 | 146 PASS |
| `boot-shim` 宿主单测 | 70 PASS |
| riscv64 启动路径 QEMU | PASS（exit 0） |
| aarch64 镜像组装 | **PASS**（`b71bce1d0`/`ae4ef5c76`/`8dc30f75e` 依次补齐 `sendrec` 实现后） |
| aarch64 启动行为 | **PASS**：`test-shim-bootmarks-aarch64.sh` exit 0，串口读到 memmap 来源行与内核进入行。**但日志中的 `dtb /memory cross-checked, diagnostic only` 不代表 O-11 的配置表 DTB 首读授权已验证**——配置表 DTB 尚未进入政策链，该测试证明的是今天实际接入的路径 |

**证据基线**：本轮代码提交 `f52730bef`。复核命令与顺序（缺一不可）：

```
git rev-parse HEAD
git status --short
cd os && cargo test -p minix-boot --lib
cd os && cargo test -p boot-shim --lib
```

146/70 条测试是**该 HEAD、该工作树状态**下的读数（§2.1、§2.2、§6 三处数字必须一致）。`git status --short` 的期望输出只有用户自有文件 `AI-chats/daily.todo.md` 一行；出现其它改动即说明测试不是在干净基线上跑的。基线之后如有新提交，必须重跑并更新本段。

复核方式：对 §2 的每条主张，文中给出了测试名或代码位置；对 §5 的每条问题，文中给出了失败形态——评审人可以只凭本文判断「问题是否成立」。
**测试通过不等于输入完整性**：146 与 70 条测试说明的是「已覆盖的行为」，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源。这几项各自需要独立的完整性论证（O-07、O-11、O-12）。

### O-12i 三级 Reservability 逐字引句

**原子用途要把「指令可执行」与「算法进展假设」分开**：主源把 LR/SC 支持分三级——「RsrvNone indicates that no LR/SC operations are supported (the location is non-reservable). RsrvNonEventual indicates that the operations are supported (the location is reservable), but without the eventual success guarantee described in the unprivileged ISA specification. RsrvEventual indicates that the operations are supported and provide the eventual success guarantee.」因此光登记指令族与宽度不够，用途记录要带：

### O-12i fallback 论证段

**`fallback` 必须是可追溯的计划引用，不是布尔值**：一个「有 fallback」的布尔标记可能只说明「源码里还存在另一个函数」，而那个函数自己可能也在用 LR/SC、或依赖该区域并不支持的 AMO。计划至少要证明四件事：① 能完成**同一算法操作**；② 不依赖刚被判为不足的 LR/SC 进展保证；③ 它实际使用的操作与宽度在该页上**确有 PMA 支持**（走同一条 `RoleAdmit`，不能用未经验证的操作替代另一个未经验证的操作）；④ 检测到缺乏进展时**真的切换**过去。合同必须显式覆盖两点：**如何检测需要 fallback**（`trigger` 与 `bounded_retry_policy`，不能把「存在另一个函数」当成检测机制），以及**fallback 自身依赖的操作、宽度与内存区域也要过准入**（`required_operations`/`required_widths`/`required_memory`）——包括 fallback 用到的锁变量、状态字与它们的访问路径，不能出现「fallback 依赖一块从未过准入的共享内存」。

### O-12i 原子三件事段重复句

- **只用 AMO 的算法**：要求相应 AMO 家族与宽度即可，不因它要求 `RsrvEventual`；`reservation_set` 为空、不检查预约集。
- **LR/SC 的几何、进展与页内约束是三件事**：基础页内约束（`norm:lr_sc_rsrv_set_page`，第一版即使不声明 profile 也成立）∧ 进展要求（第一版一律 `RsrvEventual`）∧ 声明 profile 时的几何（`Za64rs`）。三项并列，缺一不得通过原子用途准入。**不依赖最终成功的 `RsrvNonEventual` + fallback 路径整体不开放**：规范允许它，但前提是 `FallbackPlanSatisfied` 有真证据，第一版没有。 第一版策略：所有实际使用 LR/SC 的算法一律要求语义级「最终成功」进展保证（RISC-V 即 `RsrvEventual`）；`RsrvNonEventual` 与 `AtomicFallbackPlan` 路径整体留待扩展——规范允许它，但前提是 fallback 计划有真证据，第一版没有。
- 「We recommend providing RsrvEventual support for main memory regions where possible」是规范的 NOTE（推荐，不是强制），不能当准入依据引用。
- **声明符合的 ISA Profile 会追加全局 PMA 义务，只查当前源码恰好用到的指令不够**。这里有两个不同的 profile，不能合并：

### O-12i 内存序 PMA 逐字引句

- `MemoryOrderingCompatiblePages(ctx)`：**内存序 PMA 的落点**（此前只在 RISC-V 讨论里被提到，没有归属集合）。Machine-Level ISA「Memory Ordering PMA」小节主源逐字规定：「Regions of the address space are classified as either *main memory* or *I/O* for the purposes of ordering by the FENCE instruction and atomic-instruction ordering bits.」以及「Coherent main memory regions always have either the RVWMO or RVTSO memory model. Incoherent main memory regions have an implementation-defined memory model.」内核的同步原语按其中一种模型书写，因此：① 该集合进入**基底层**（用途无关）——用途特有的原子要求仍归 `RoleAdmit(AtomicSharedState, ·, ctx)`；② **第一版不支持由软件维护一致性的非一致主存**：区间被声明为主内存但 Coherence PMA 显示非一致 → 无论其内存模型能否被证明，都不得进入 `PmmBaseAvailable(ctx)`；实现自定义排序的区域同理被排除。规范在「Coherence and Cacheability PMAs」小节逐字把两件事分开——「Coherence is a property defined for a single physical address, and indicates that writes to that address by one agent will eventually be made visible to other coherent agents in the system. **Coherence is not to be confused with the memory consistency model of a system**, which defines what values a memory read can return given the previous history of reads and writes to the entire memory system.」并写明「the use of hardware-incoherent regions is discouraged due to software complexity, performance, and energy impacts」。两条规则各答各的问题：

### O-12i A/D 两方案逐字引句

> Two schemes to manage the A and D bits are defined: … The *Svade* extension: when a virtual page is accessed and the A bit is clear, or is written and the D bit is clear, a page-fault exception is raised. … When the Svade extension is not implemented, the following scheme applies. … The page tables must be located in memory with hardware page-table write access and *RsrvEventual* PMA. … All harts in a system must employ the same PTE-update scheme as each other.（锚点 `norm:pte_ad_scheme_uniform`）

### O-12i hart 集合职责与反例

职责不重叠：`SystemHartSet` 只用来验证 PTE 更新方案的**全系统**一致性；`ParticipatingHartSet` 只用来计算普通物理访问准入的**逐 hart 交集**。反例（4 hart 平台）：hart 0/1/2 参与、hart 3 暂不启动且实际方案与其它三个不同——**不能**因为 hart 3 没进 `ctx.H` 就宣称 `norm:pte_ad_scheme_uniform` 已满足。该证明由 M-mode/固件按 SystemHartSet 逐 hart 给出，缺任一 hart 的证明即 `Unknown`。
**清单从哪来、永不启用的 hart 怎么算，必须写死**：`SystemHartSet` 不能由 `ctx.H` 反推，也不能只停留成一个名字——它要有一份可核对的清单与来源（固件启动 ABI 的 hart 清单 / 设备树 `/cpus` 的 `reg` 集合 / ACPI MADT 处理器条目，取其一并记录代次）。清单缺失、与实际启动 hart 不一致或来源不可证 → `Unknown` → 拒绝启动。**「永不启用」不等于「不属于系统」**：某个 hart 若只是本项目暂不调度，它仍属规范所说的 system hart，其方案仍须被证明；只有能给出该 hart 在平台上**不存在**的物理事实（不是「我们不启动它」）时，才可以把它从清单里去掉。把它从 `ctx.H` 删除，规范义务一点也不会消失。

### O-12i CPU status 逐字引句

**清单的完整性要按 CPU 节点状态语义判定**（DTSpec 逐字）：`"okay"` = The CPU is running；`"disabled"` = The CPU is in a quiescent state，且可由 `enable-method` 重新启用（「This property is required for CPUs with a status property with a value of `disabled`」）；`"fail"` = The CPU is not operational or does not exist，并且「A CPU with `fail` status does not affect the system in any way」。由此：`disabled` **不能**简单排除——它仍可能是未来可启用的 system hart，必须进入一致性证明；`fail` **不能直接当成物理不存在**——DTSpec 的措辞是「not operational or does not exist」，两种读法对系统集合的后果不同：

### O-12i 基底与用途两层重复表述

**名字与定义必须一一对应；基底与用途特有是两层**：`RoleAdmit(role, page, ctx)` 是声明式接口，其操作化定义**只有** `RolePageEligible(page, role, ctx)` 一个展开式。`CommonCpuAccessEligiblePages(ctx)` 只消费 `BaseAccessRequirements(ctx)`，`RoleAdmit` 只消费 `RoleSpecificRequirements(role, ctx)`——`GeneralData` 的用途特有集合因此**定义为空**，它不再同时扮演「基底要求」与「空的额外要求」。`PageTable` 的用途特有要求即 `PageTableBackingEligible` 的访问部分。**只有 `GeneralData` 可以按定义为空**：`PageTable` 因隐式遍历读与显式 PTE 写始终必需而非空，`Executable` 必须有完整取指模式，`AtomicSharedState` 必须有完整算法清单；其它用途出现空集是 `InsufficientEvidence`，前置条件是 `RoleRequirementsComplete` 与 `RoleRequirementsApplicable`。把 `RoleAdmit` 存成一个布尔字段会丢掉 `req`/起始/CPU 三个量词维度。

### O-12i 全称量词段重复表述

**全称量词不能替换成「字节被覆盖」**：一页每个字节都被某次成功访问覆盖，并不代表从该页任一合法起始地址发起的最宽访问都能成功——编号最低的条目只需命中一个字节就要覆盖整次访问，跨边界的操作整体失败（规范那个覆盖 `0xC–0xF` 的条目遇到 `0x8–0xF` 的 8 字节访问的例子）。`AdmittedFootprints` / `AllowedOperationStarts` 这类「先把可允许访问收集成集合、再比包含」的写法可以当等价推导，但**判定式本身必须写成对起始地址全称量化**，否则实现里很容易退化成字节并集。PMP 粒度最细 4 字节（`norm:pmp_granularity`、`norm:pmp_a_field_napot_na4`「These modes support four-byte granularity」），页内边界是真实存在的，这正是必须量到起始地址的原因。**判定式的索引必须是 `AccessRequirement` 而不是页**：同一 hart 同一条 PMP 配置下，「普通读写可用但隐式页表读不可用」「页表可读但 A/D 隐式写不可用」「读写可用但取指不可用」「读写可用但某 AMO 宽度不可用」四类差异真实存在，字节集合会把它们抹平。**一个页不满足 `PageTable` 用途，不代表它不能作为 `GeneralData`**——把页表条件塞回基底会把用途层架空。

### O-12i 页级与对象级互不冒充

一个对象可以跨过两页，其中一页不作为该用途的分配候选，但对象自己的既有访问点仍可能全部合法；反过来，整页合格也证明不了某个跨越页尾的对象访问合法。页级谓词与对象级谓词不得互相冒充：`PmmRoleEligible` 回答「能不能把这一页交给该用途」，`ObjectAccessEligibility` 回答「这个对象现在经这些 VA 是否真的可用」。
**第一版只准入「按架构对齐的整次访问」——三类对齐必须分开**：`norm:pmp_misaligned_access_behavior`「On some implementations, misaligned loads, stores, and instruction fetches may be decomposed into multiple memory operations, some of which may succeed before an access-fault exception occurs, as described in the RVWMO specification. PMP checking is performed on each memory operation independently.」以及紧随其后的「In particular, a portion of a misaligned store that passes the PMP check may become visible, even if another portion fails the PMP check.」——**半写可见**是真实失败形态：一次非对齐 store 可以把落在合格页内的那一半写进去，再在越界那一半上触发故障，页级资格完全拦不住。因此 `alignment` **不能**统一理解成「等于宽度」：

### O-12i IALIGN 逐字引句

**取指不能套用「宽度 = 对齐」**：RISC-V 的逐字依据是 `src/unpriv/intro.adoc` 的「IALIGN is 32 in the base ISA, but some ISA extensions, including the compressed ISA extension, relax IALIGN to 16.」（该文件 452–460 行），以及「the instruction-fetch unit on machines with ILEN >32 report an illegal-instruction exception rather than an access-fault exception when such an instruction borders a protection boundary, complicating variable-instruction-length fetch and decode」（同文件 528–531 行，位于 NOTE 内，不是编号规范条目）——**规范明确承认一条指令可以 border 一个保护边界**。于是页尾 `page_end − 2` 处的 32 位指令 footprint 是 `[page_end−2, page_end+2)`，跨两页。x86 的可变长指令（最长 15 字节）同理，该条按架构事实使用，本方未取 Intel SDM 原文，登记为待核。

### O-12i PMP 整次访问判定逐字引句

**PMP 不是逐字节权限的交集，而是「整次访问」的判定**——规范逐字规定：「PMP entries are statically prioritized. The lowest-numbered PMP entry that matches any byte of a memory operation determines whether that operation succeeds or fails.」（`norm:pmp_entry_priority`）以及「The matching PMP entry must match all bytes of a memory operation, or the operation fails, irrespective of the L, R, W, and X bits.」（`norm:pmp_full_match_required`）；规范自带的反例是：一条覆盖 `0xC–0xF` 的条目遇到 `0x8–0xF` 的 8 字节访问会**失败**，哪怕逐字节看只有一半越界。所以判定输入必须是**整次访问**（基址 + 宽度 + 对齐 + 特权级 + 路径）。

### O-12i PMP 权限表逐字引句列

| 操作 | 查的位 | 有效特权级 | 依据 |
|---|---|---|---|
| `Load` / `Lr` | **R** | S 或 U | `norm:pmp_load_fault`：「Attempting to execute a load or load-reserved instruction that accesses a physical address within a PMP region without read permissions raises a load access-fault exception.」 |
| `Store` / `Sc` / `Amo(·)` / cache-block zero | **W** | S 或 U | `norm:pmp_store_fault`：「Attempting to execute a store, store-conditional, AMO, or cache-block zero instruction that accesses a physical address within a PMP region without write permissions raises a store access-fault exception.」 |
| `Fetch` | **X** | S 或 U | `norm:pmp_exec_fault`：无执行权限的取指触发 instruction access-fault |
| 隐式页表**读** | **R** | **S**（固定） | `norm:pmp_check_pagetable_access`：「PMP checks are also applied to page-table accesses for virtual-address translation, for which the effective privilege mode is S.」＋ `norm:pmp_cfg_permissions` |
| 硬件 A/D **隐式写** | **W** | **S**（固定） | 遍历算法把这一步直接写成对 PTE 的 **store**：「If a store to the PTE at address __a__+__va.vpn__[__i__]×PTESIZE would violate a PMA or PMP check, raise an access-fault exception corresponding to the original access type.」（`src/priv/supervisor.adoc` 地址翻译算法第 5 步的分支，该句**无独立 norm 锚点**，本方本轮直取原文）＋ `norm:pmp_store_fault` 的 store→W。**强度差别要写清**：条款存在，但它是算法步骤里的一句话而非编号规范条目，且同一步还要求「Compare…set a/d…store」原子完成——这正是硬件方案另要 `RsrvEventual` 与硬件页表写权限的原因 |
| 软件更新 PTE | 显式 `Write` 的 **W** | S | 同一张表，只是 `path = Explicit` |

### O-12i 三条配套的逐字引句

- **权限按写侧检查 ≠ 原子能力由 PMP 决定**。`norm:amo_op` 描述 AMO 为「atomically load a data value … then store the result back」，一次操作同时含 load 与 store，但规范的故障归类把 AMO 划到 store 侧；同时 `norm:pmp_rwx_warl` 规定 R/W/X 里「R=0 且 W=1」是**保留编码**，因此一条可写的合法条目必然也可读。**AMO 的族（`AMONone`/`AMOSwap`/`AMOLogical`/`AMOArithmetic`，`norm:pma_amo_levels`）与支持的宽度、以及 `Ziccamoa`/`Ziccamoc` 这类主存义务全部由 `PmaPermits` 承担**，拿 PMP 位代替会造出「PMP 可写就以为支持该 AMO」的错误准入。
- **`privilege` 是有效特权级，且合同只覆盖 `{S, U}`**。`norm:pmp_check_priv_modes`：「PMP checks are applied to all accesses whose effective privilege mode is S or U, including instruction fetches and data accesses in S and U mode, and data accesses in M-mode when the MPRV bit in `mstatus` is set and the MPP field in `mstatus` contains S or U.」由此三条结论：① `MPRV=1 ∧ MPP∈{S,U}` 的 M-mode 数据访问落在本合同内，但内核第一版不在 M-mode 用 MPRV 访问普通主存，所以这一格记为「合同覆盖、当前无消费者」；② `norm:pmp_l_bit_m_mode_enforcement` 说 L 清零时「any M-mode access matching the PMP entry will succeed」，**M-mode 能访问不构成 S/U 准入的证据**；③ 位判定本身按特权级分支：`norm:pmp_rwx_check`（L 置位或特权级为 S/U 时，只有对应访问类型的 R/W/X 置位才成功）与 `norm:pmp_no_entry_match`（无条目命中时 M 成功；S/U 在至少实现一条时失败）。
- **PMP 与 PMA 并行且都要通过**：PMP 管有效权限，PMA 管访问类型、宽度、取指、页表读写与原子能力。缺任一项，公式就表达不出「PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝」这条已列入的负控。

### O-12i cache-block 逐字引句

**cache-block 三类操作的许可规则与普通读写不同**（`src/unpriv/cmo.adoc` 原文；prefetch 类 `norm:cbp_access` 在许可时按 load/store/fetch 任一，不许可时**不产生异常也不访问内存**，与另两类不同）：`norm:cbm_access` 说 cache-block management「is permitted to access the specified cache block whenever a load instruction **or store instruction** is permitted to access the corresponding physical addresses」——是「读**或**写许可」，两者都不允许而仅取指允许时行为 UNSPECIFIED；它**不是**「按读权限检查」。`norm:cbz_access` 说 cache-block zero 要求 **store** 许可**且** PMA 表明支持该访问类型。另有块级一致性约束 `norm:pmp_same` / `norm:pma_same`：整个 cache block 内 PMP 位与 PMA 必须一致，且「if write permission is granted by the PMP access control bits, read permission shall also be granted」，不满足时 `norm:cbo_rsv` 规定行为 UNSPECIFIED。因此将来启用这两类操作时，准入必须按**整个 cache block** 判并检查块内一致性，只按单次访问 footprint 判会漏掉 UNSPECIFIED 分支。

### O-12i Smepmp 逐字引句与锚点列

**Smepmp 的支持边界（第一版裁定）**：`mseccfg.MML`（Machine Mode Lockdown）「changes the interpretation of the `pmpcfg.L` bit」——置位后 `L` 不再表示「锁定且在所有模式强制」，而是 `L=1` → *M-mode-only*、`L=0` → *S/U-mode-only*，并且原本保留的 `pmpcfg.RW=01` 与 `pmpcfg.LRWX=1111` 变成 *Shared-Region* 编码（`norm:mseccfg_mml_pmpcfg_l_op`、`norm:mseccfg_mml_m_rule_op`、`norm:mseccfg_mml_soru_rule_op`、`norm:mseccfg_mml_shared_l0_op`、`norm:mseccfg_mml_shared_l1_op`、`norm:mseccfg_mml_shared_lrwx_1111_op`）；`MMWP` 把「无匹配条目的 M-mode 访问」的默认从 ignored 改成 denied（`norm:mseccfg_mmwp_op_warl`）；`RLB` 是临时绕过锁定的开关（`norm:mseccfg_rlb_op_warl`）；三者复位为 0「unless the platform mandates a different reset value」（`norm:mseccfg_mml_mmwp_rlb_rst`），`MML`/`MMWP` 还是 sticky 位。**同一张 R/W/X 表在 `MML=1` 下整张是错的**——查表的位含义已经变了。第一版因此只接受 `smepmp_state = {MML=0, MMWP=0}`（`RLB` 只登记不参与判定）；`MML=1`、`MMWP=1`，或 `mseccfg` 读不到/无法证明 → `SmepmpUnsupportedConfiguration`，在平台准入阶段拒绝，不做「按标准语义近似」。将来若要支持，得把 Shared-Region 的三种编码、`norm:mseccfg_mml_x_restrict`（不能新增可执行的 *M-mode-only* 或锁定的 *Shared-Region* 规则）与 `norm:pmp_rw01_rsv` 一并写进 `PmpPermits`，那是另一份合同。`mseccfg` 同 `pmpcfg*` 一样只能 M-mode 访问，所以这一格也归 `PmpEvidence.provider`，不能靠探测。

### O-12i Absent 段逐字引句

**`Absent` 不等于「没有物理内存保护」**：规范允许零条 PMP（`norm:pmp_entry_count`「Implementations may implement zero, 16, or 64 PMP entries」），但同一节的 NOTE 逐字写着「Platforms vary widely in demands for physical memory protection, and some platforms may provide other PMP structures in addition to or instead of the scheme described in this section.」因此 `PmpImplementationState = Absent` 只表示**标准 PMP 单元不施加额外限制**，它不证明平台没有别的保护结构。第一版两条出路，必须择一登记进 `PmpEvidence`：① 平台 profile 给出带实现名与版本的支持限制声明——「本 profile 下该区间除标准 PMP 外无其它物理访问限制」；② 拿不出该声明 → 证据不足 → fail-closed，不得把 `Absent` 当成整段通过。把 `Absent` 直接写成 `PmpPermits => true` 而不附支持限制，等于把「没观察到保护」写成「没有保护」。

### O-12i 稳定性四条与待补测试穷举

**证据稳定性：`RLB` 进稳定性证明，不是只登记**。`norm:mseccfg_rlb_op_warl` 说 `RLB` 提供「a mechanism to temporarily modify *Locked* PMP rules」，即 `RLB=1` 时 `L=1` 的条目仍可被改。于是「`RLB` 只登记不参与判定」只在**位解释**层面成立，不能当作证据有效期依据。完整合同四条：① 若依赖 `L=1` 条目保证稳定，必须证明 `RLB` 不允许绕过锁定（`RLB=0` 且无法再置位）；② 只要求 `RLB=0` 也不够——没有 `L` 的条目仍可由 M-mode 修改；③ 真正要证明的是**整份有效配置**在 `stability_until` 内不变——而且**事后通知不够**：provider 改完再通知，旧投影可能已被分配器或访问路径使用，事后递增代次无法撤销已经发生的非法访问。第一版只接受两种合同之一：
   **A 不可变配置合同**（第一版选它）：provider 承诺从提供证据到生命周期结束，不修改相关 PMP、Smepmp 与额外保护配置；
   **B 协调式变更合同**：变更**前**先通知内核，内核停止相关访问、完成受影响对象检查与必要的迁移或隔离，再允许配置变化；变化完成后递增代次、重建投影、重新开放访问。
   `stability_until` 因此不是单纯的时间期限，而是 provider 对「配置不可变」或「变更受协调」的承诺；没有该承诺的证据不得用于任何跨阶段分配或访问授权；④ 任何改变生效配置的事件（M-mode 改写 `pmpcfg*`/`pmpaddr*`、写 `mseccfg`、hart 上线使该 hart 的配置进入参与集合）都必须递增 `evidence_generation` 并使全部依赖投影失效。拿不到承诺与通知的 provider，`stability_until` 只能记「未声明」，其证据不得跨阶段缓存。
**待补测试（按用途区分，不做「全局原子/全局可访问」的粗暴结论）**：MAIR 配置正确但叶子选错 `AttrIndx` 必须失败；PAT 项正确但 MTRR 使有效类型非 WB 必须失败；任一参与 hart 的有效 PMP 拒绝该区间的**普通读写** → 不得进入 `PmmBaseAvailable`；仅拒绝**隐式页表访问** → 基底保留、`PmmRoleRejected(PageTable)` 命中；PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝；PMA 无 AMO 但普通读写可用 → 按用途处理；一致主内存标为实现自定义排序 → 不得进入基底；A/D 方案的六行负控见上表（尤其「Svade/Svadu 均不可证 → 拒绝启动」与「两 hart 方案不同 → 拒绝该平台配置」）；参与 hart 集合新增一个 PMP 更严的 hart → 旧 `ctx` 的全部投影失效并重建；LR/SC 算法依赖最终成功而区域仅 `RsrvNonEventual` 且无 fallback → 该用途不合格，同页 `GeneralData` 仍合格；一页只有一半落在准入域内 → 该用途不合格；`ranges` 缺失与非空分别落到各自错误；`FinalMap` 扩容后旧快照与投影全部失效；**起始地址负控**——同一页内存在一个起始地址使 footprint 越过 PMP 条目边界 → 该页在此宽度下不合格，即使逐字节看全部被覆盖过；**非对齐负控**——合格页尾部发起的半字/字访问越过条目边界 → 必须拒绝或走显式软件路径，不得按整页合格放行；**合同负控**——同一 `evidence_generation` 下 `legal_starts_contract` 从 A 切到 B → 全部投影失效；**Smepmp 负控**——`mseccfg.MML=1` 或 `MMWP=1` 或 `mseccfg` 不可读 → `SmepmpUnsupportedConfiguration`，不得套用 R/W/X 表；`Absent` 且平台拿不出「无其它保护结构」支持限制 → fail-closed；**Footprint 顶端负控**——`a = 0xffff_ffff_ffff_fff8` 且宽度 8 必须合格（`end == 2^64`），`a = 0xffff_ffff_ffff_fff9` 必须越界；**取指跨页负控**——页尾 `page_end − 2` 处的 32 位取指：页级资格按保守片段集排除该起始，对象级按 `FetchFootprints` 同时判两页；**空集负控**——`BaseAccessRequirements` 为空、`U = ∅`、或某条 `req` 的 `LegalStarts` 为空 → `InsufficientEvidence`，不得真空通过；**稳定性负控**——`RLB=1` 或 provider 没有变更通知 → `stability_until` 不得用于跨阶段缓存，配置变更未递增 `evidence_generation` 必须失败；**保护清单负控**——`Implemented` 但 profile 既未证明「无未建模结构」也未列出其它结构 → `PlatformProtectionPermits` 不得默认为真；**profile 负控**——声明 RVA23S64 而 `Svade` 未实现 → 不合格，即使通用公式落到硬件更新；**基底真空负控**——`BaseAccessRequirements` 为空、`PmmRoleEligible` 的 `U = ∅`、或任一基底 `req` 的 `LegalStarts` 为空 → `InsufficientEvidence`，不得通过；**用途特有集合负控**——`RoleSpecificRequirements(GeneralData) = ∅` 必须**不**触发失败（空集是定义），但把它当成「基底要求」的写法必须失败；**硬件 PTE 更新负控**——`HardwareManagedAd` 下缺 `ImplicitHardwarePteUpdate` 的任一项（隐式 W、硬件页表写、原子更新、`RsrvEventual`）→ 页表用途不合格；**对象区间负控**——VA 起点正确但区间中段落到别名或属性不兼容映射 → `ObjectMappedAccessEligible` 失败；**跨页取指对象负控**——32 位指令起于页尾 2 字节、下一页取指不合格 → 失败；**稳定性负控**——provider 只承诺「变更后通知」→ 不接受，须给不可变或变更前协调承诺；**预约集几何负控**——支持 LR/SC 且 `RsrvEventual`，但预约集几何不满足声明 profile → 原子用途不合格；**cache-block 负控**——`CacheBlockManagement`/`CacheBlockZero` 在第一版一律 `UnsupportedAccessOperation`；**本轮新增负控**：硬件页表读未登记 → `PageTable` 用途不合格（即使显式写通过）；`RoleRequirementsComplete` 为假或 `Executable` 的 `instruction_lengths` 未登记 → `InsufficientEvidence`；`ctx.ParticipatingCpuSet = ∅` → 基底判定不得通过；对象物理准入通过但来源政策未授权（MRB / `no-map` / `EfiReservedMemoryType`）→ `ObjectPolicyAccessEligible` 失败；连续 VA 映射到两段不连续 PA 而 `expected_translation` 只登记一段 → 失败；叶子落在正向授权内但与某条 `DenyMapping` 相交 → `MappingAudit` 失败；LR/SC 算法在第一版遇到 `RsrvNonEventual` 区域 → 一律拒绝（fallback 分支未开放）。

### §2.1 来源记录层逐字段叙述与用例名

**来源记录层（DTB 侧，第一阶段）**：`collect_with_sources()` 在同样的遍历里为每条声明留一条 `SourceRecord { id, kind, origin, range: PhysicalRange }`——`kind` 六类：`Memory`、`FirmwarePrefix`、`MrbEntry`、`ReservedOrdinary`、`ReservedReusable`、`ReservedNoMap`；`SourceKind::class()` 把它们分成**输入声明**（`Memory`/`MrbEntry`/`Reserved*` 五类）与**派生保留**（`FirmwarePrefix`）。**字段名是中性词**：输入声明的 `range` 就是声明范围；派生前缀的 `range` 是派生结果 `M_i ∩ [0, F)`（`F = firmware_reserve_end`），所以不叫 `raw_*`。`origin` 给出位置：`MemoryNode { node, reg }`、`ReservedMemoryChild { node, reg }`、`MrbEntry(n)`（`node` 只数匹配节点、`reg` 只数该节点 `reg` 属性内的元组，均从 0 起、按文档顺序）；前缀的 `origin` 是 `FirmwarePrefix { memory_id, cut }`，既指回被切分的 `/memory` 记录，又保留派生参数 `F` （原 DTB 释放后仍能解释「为什么截在这里」）。`id` 是**输入对象内的局部 ID**；跨输入引用须用 `(InputObjectId, LocalSourceId)`，不能单用 `id`。**记录不携带页范围**：`/memory` 投影向内取整（`FullPages`），保留声明与 `no-map` 向外取整，MRB 访问禁止保留精确字节。`range.end_exclusive` 是 `u128` 独占端，因此记录层能表达 `2^64` 本身：`physical_end` 只在 `start + len > 2^64` 时报 `RangeOverflow`，恰好等于 `2^64` 是合法顶端边界。拒绝发生在**投影**侧——`declared_page_range` 向外取整后达到 `2^64` 时报 `UnsupportedPhysicalAddressEnd`，`collect()` 构表时用 `projectable_end()` 复查同一条件。`PhysicalRange` 是两条腿共用的类型，因此 UEFI 描述符顶端页将来接入记录层时不会再靠 `u64` 加法偶然放过；但 UEFI 侧的 `SourceRecord` **今天仍未产出**（见 O-12g），这句只说类型合同，不说 UEFI 腿已实现。容量 40 由**输入上限**推出（`/memory` 段与声明各受 `DTB_MEMMAP_MAX`、`no-map` 同样计数、前缀不多于 `/memory` 段、MRB 受 `DTB_MRB_MAX`），按 **reg 元组**计数而非节点数；超限拒绝（`TooManySourceRecords`）。`collect()` 语义不变、只取前两张表。测试：`dtb_no_map_reservation_keeps_a_source_record`、`dtb_memory_reservation_record_keeps_the_raw_range`、`dtb_reservation_outside_memory_keeps_its_record`、`dtb_source_records_cover_every_projected_span`、`dtb_collect_and_collect_with_sources_agree`、`dtb_no_map_declarations_count_against_the_declaration_cap`、`dtb_declaration_cap_is_shared_across_reservation_kinds`、`dtb_source_capacity_is_reachable_at_exactly_forty`、`dtb_firmware_prefix_links_to_its_memory_record`、`dtb_identical_reservations_are_deduplicated_not_rejected`、`dtb_identical_mrb_entries_are_rejected`、`dtb_same_range_with_different_semantics_is_a_conflict`、`dtb_every_input_declaration_has_exactly_one_record`、`dtb_identical_memory_tuples_are_deduplicated_not_rejected`、`dtb_missing_reserved_memory_ranges_is_rejected`、`dtb_missing_reserved_memory_cells_is_rejected`。

### §2.1 拒绝条件表的 DTSpec 逐字引句

| 来源 | 解析内容 | 拒绝条件 |
|---|---|---|
| `/memory` | 全树枚举 + `reg()`，支持 unit-address 与多段 | 单表超过 8 段（`TooManyFreeRegions` / `TooManyReservedRegions`）；`address + size` 越过 `2^64`（`RangeOverflow`，畸形）；末端恰为 `2^64`：**记录层接受**（`SourceRecord.range` 已是 `PhysicalRange`，`end_exclusive: u128`），拒绝发生在**投影层**（`projectable_end()` 返回 `None` → `UnsupportedPhysicalAddressEnd`），投影改用 `u128` 后才可能交付最高页 |
| `/reserved-memory` | 路径查找 + 可见性交叉检查；父节点的 `#address-cells`/`#size-cells` 必须存在**且与根节点取值相同**（DTSpec 的父节点属性表把 `#address-cells`/`#size-cells`/`ranges` 三项 Usage 都标为 R=Required，另逐字写「should use the same values as for the root node, and `ranges` should be empty so that address translation logic works correctly」——因此**缺失是格式错误**，而「不同/非空」只是 *should*；本项目要求相同与为空属**项目输入子集**，不是规范要求）；`ranges` 必须存在且为空（同为**项目输入子集**，非空 `ranges` 不代表非法，而是尚无转换路径）；子节点必须有可解析 `reg`；只有 `size` 无 `reg` 是**动态分配请求**——DTSpec 3.5 逐字：「Each child node may either use a `reg` property to specify a specific range of reserved memory, or a `size` property with optional constraints to request a dynamically allocated block of memory」，且「A reserved memory node requires either a `reg` property for static allocations, or a `size` property for dynamics allocations. Dynamic allocations may use `alignment` and `alloc-ranges` properties to constrain where the memory is allocated from. If both `reg` and `size` are present, then the region is treated as a static allocation with the `reg` property taking precedence and `size` is ignored.」。因此 `reg`+`size` 并存时按 `reg` 静态处理、`size` 与 `alignment`/`alloc-ranges` 一并忽略是**规范规定**，不是本方选择；`no-map`/`reusable` 必须是**空属性**；两者互斥 | `size`-only 动态请求（`UnsupportedDynamicReservation`：**合法输入的不支持子集**——规范允许该形状，本启动没有可为其选址的动态分配器，静默忽略会把页交给 PMM；见 O-12g 行 c）；缺 `reg` 且无 `size`（`UnsupportedReservedDeclaration`）；缺 cells（`MissingReservedMemoryCells`）、cells 与根不同（`MismatchedReservedMemoryCells`，项目子集）；`ranges` 缺失（`MissingReservedMemoryRanges`，格式错误）与 `ranges` 非空（`UnsupportedReservedMemoryRanges`，输入合法但暂不支持转换）分开；同时声明 `no-map` 与 `reusable`（`ConflictingReservationAttributes`）；标记属性带非空值（`NonEmptyReservationAttribute`）；声明重叠（`OverlappingReservedRanges`）；节点可见却取不到（`ReservedMemoryUnreachable`） |
| MRB | `memory_reservations()` 条目（地址+长度，`{0,0}` 终止） | `address + size` 越过或落到 `2^64`（`RangeOverflow` / `UnsupportedPhysicalAddressEnd` 分名；后者是今天的临时拒绝，目标在记录层接受）；**零长度非终止项**（`ZeroLengthMemoryReservation`）；**条目之间重叠**（`OverlappingMemoryReservations`）；**超过 16 条**（`TooManyMemoryReservations`，缺失终止项也会落到这里）。对齐、`totalsize` 边界与终止项**由 `fdt_layout::validate` 在解析库之前检查**（五条输入路径接入）；`DtbMemoryMap` 自身不做头部校验，也不声称做过 |

### §2.1 重复容忍与 owner 关系段

`/reserved-memory` 的重复容忍是**项目级输入策略**，不是规范授权；MRB 没有这层容忍。`reusable` 在去重时保留的是**声明身份**（两条记录各有 `id`）；owner 关系尚未建立，不能把记录数量当成 owner 数量——`memory-region` 引用是多对多（一个设备可引用多个区域、一个区域可被多个设备引用），模型应是 `DeviceMemoryRegionReference`（或 `MemoryRegionAssociation`）的引用集合，而非单个 `OwnerId` 字段。**引用不等于所有权**：只有平台 binding 或驱动注册信息进一步证明回收职责时，才提升为 `ReclaimAuthority`。`/memory` 的**部分重叠**拒绝同样是项目策略：**不是数学必然**——§3 允许纯集合层对兼容区间 `Union`，部分重叠在集合模型里可以安全归一化；拒绝的理由是「严格输入策略：部分重叠视为固件异常」，它不依赖今天 PMM 的实现，也不声称 DTSpec 禁止 `/memory` 重叠。若将来改为并集，来源记录仍须各留一条。`reusable` 的 owner 关系在去重时不得合并（来源记录各留一条正是为此）。
- `reusable` 在记录层已独立成 `ReservedReusable`；两张投影表仍把它按保留处理（不实现 owner 关联、回收能力或生命周期处置），第一版保守地不回收，但不丢原始声明语义。

### §2.1 采集用例穷举与不保证段

测试（`cargo test -p minix-boot --lib`，146 通过）中的采集用例：`dtb_single_memory_region_fw_end_zero`、`dtb_multi_region_and_unit_address_variant`、`dtb_more_than_capacity_is_rejected_not_truncated`、`dtb_declared_reservation_is_never_conventional`、`dtb_no_map_reservation_leaves_both_tables`、`dtb_reservation_inside_the_firmware_prefix_is_not_duplicated`、`dtb_reservation_outside_memory_claims_no_ram`、`dtb_memory_reservation_block_is_carved_from_free`、`dtb_overlapping_reservations_are_rejected`、`dtb_reservation_with_wrapping_end_is_rejected`、`dtb_reserved_memory_with_wide_cells_is_rejected`、`dtb_carving_that_exceeds_capacity_is_rejected`、`dtb_reserved_memory_without_empty_ranges_is_rejected`、`dtb_reservation_with_no_map_and_reusable_is_rejected`、`dtb_reservation_with_nonempty_marker_is_rejected`、`dtb_reserved_memory_cells_must_match_root`、`dtb_overlapping_memory_reservations_are_rejected`、`dtb_too_many_memory_reservations_are_rejected`、`dtb_disabled_memory_node_is_rejected`、`dtb_hotpluggable_memory_is_rejected`、`dtb_zero_length_memory_reservation_is_rejected`、`dtb_size_only_reservation_is_rejected_as_dynamic`、`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`、`reg_and_size_together_uses_reg_and_ignores_size`、`dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end`、`dtb_top_partial_page_reservation_is_refused_as_unsupported_end`、`dtb_memory_range_past_2_64_is_range_overflow`。
**不保证什么**：不产出禁止集合、不产出 PMM 排除集合、不实现 `reusable` 的 owner 回收、不处理 UEFI 来源。**`collect()` 的两张表无法表达 MRB 来源**：MRB 范围会从 conventional 扣除，但不会进入 reserved（reserved 的语义是「可达、永不再分配」，MRB 是「不得访问」）；调用方若不持有 `SourceRecord`，排除原因与访问禁止来源都不可追溯。这是旧接口的**有意取舍**，修复方向是让政策所有者持有记录，而不是把 MRB 塞进 reserved（那会把「不得访问」误表达成「可达但不分配」）。因此 `dtb_collect_and_collect_with_sources_agree` 只证明两条接口的**投影一致**，不证明旧接口保留来源完整性。
**输入准入（已实现，不是「已知限制」）**：`/memory` 的 `status` 缺失或为 `okay` 才采集；其它值一律拒绝启动（`UnsupportedMemoryNodeStatus`）——节点自己说不可用的内存不得交给 PMM。带 `hotpluggable` 的 `/memory` 拒绝启动（`HotpluggableMemoryUnsupported`）：内核没有内存移除机制。`/reserved-memory` 子节点的 `status` 不筛选——保留声明即使 `disabled` 也按声明处理（保守方向，只会多留不会误放行）。**仍是项目限制**：`reusable` 不回收；记录保留其类型，回收语义留给后续生命周期层。

### O-06 冲突位叙述与矩阵重复

- **冲突位域整体 fail-closed（已实现）**：`0x1000..=0x1_0000` 在两套官方编码里含义不同——`0x4000` 在一套里是 `SP`（必须排除出 PMM）、另一套里是 `XP`（仅保护能力元数据）；`0x8000` 是 `CPU_CRYPTO` vs `NV`；`0x1000` 是 `MORE_RELIABLE` vs `WP`；`0x2000` 是 `RO` vs `RP`；`0x1_0000` 是 `HOT_PLUGGABLE` vs `MORE_RELIABLE`。任一读法都可能把必须排除的内存伪装成元数据，因此整个 `0x1_F000` 在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed` 与 `the_ambiguous_bit_fails_closed_under_either_name`）。
- `0x0010_0000`（仓库编码的 `HOT_PLUGGABLE`）：`uefi-raw 0.9` 无该常量，走未知位拒绝（`hot_pluggable_bit_fails_closed_as_unknown`）。
- **代价**：`NON_VOLATILE` 位现在也落进拒绝域——模型仍认识 NV 语义（`TYPE_INDEPENDENT` 保留该分支），但**当前分类器不接受带 NV 位的输入，该分支在真实输入上不可达**；恢复它需要**两步**：先钉住带版本证据的固件编码 profile，再把 `0x1_F000` 的无条件拒绝改为按 profile 解释——只要无条件拒绝还在，仅添加 profile 记录仍然解不开 `NV`。若决定后续版本也不支持该属性，应明说，不要让读者以为 profile 一到手就自动解锁。`SPECIAL_PURPOSE`（`0x4_0000`）属于 `ProfileAttribute` 拒绝域、低位 `0x4000` 属于冲突拒绝域，**当前同样不可达**。**位值证据不是「三个已发布文本 vs EDK2」**：UEFI 2.10 原始发布版与 EDK2/Linux 同组，2.10 Errata A 与 2.11 PDF 是移位后的另一组（见下方完整矩阵）。钉住的是「当前 `uefi-raw` 与 Linux 头文件一致的编码」这一事实，而不是某份 PDF 的表格；将来升级 `uefi-raw` 时**必须重核全部已识别属性位**，不能只迁移 hot-pluggable 一位。

### O-06 高位 profile 门控叙述

**高位属性改为 profile 门控（已实现），理由要按两版发布文本写对**：`RO`/`SP`/`CPU_CRYPTO` **在 UEFI 2.10 原始发布版里就是 `0x2_0000`/`0x4_0000`/`0x8_0000`**；2.10 Errata A 与 2.11 PDF 把它们移到低一组（`0x2000`/`0x4000`/`0x8000`）。所以问题不是「已发布规范没有这些位置」，而是**两个正式发布版本族对同一属性采用了不同位值**，而内核无法从 `uefi-raw` 常量判断启动固件采用哪一版。三者因此在 profile 未钉住前**直接拒绝**（`PolicyError::ProfileAttribute`，测试 `profile_only_attributes_fail_closed_without_a_pinned_profile`）；`0x10_0000` 仍由未知位闸拒绝。将来 `UefiMemoryAttributeProfile` 必须绑定**实际启动固件镜像身份**（x86 与 aarch64 分别记录，不能假设同源），并**同时钉住低位组与高位组的完整映射**——只授权三个高位会得到「部分属性可识别、其余冲突位永久拒绝」的半套 profile。

### §2.2 分类器用例穷举

测试（`cargo test -p boot-shim --lib`，70 通过）：`types_that_earn_no_ordinary_mapping_go_to_the_side_list`、`excluded_and_device_types_stay_out_of_every_list`、`rejects_side_list_beyond_its_buffer`、`the_four_ambiguous_combinations_land_as_adjudicated`、`every_contested_attribute_bit_fails_closed`、`profile_only_attributes_fail_closed_without_a_pinned_profile`、`geometry_is_checked_for_ranges_that_are_emitted` 等 32 条。
**不保证什么**：类型 11/12 与 `RESERVED`/`UNUSABLE` 只是一种「不产出区间」的处置，它们**没有**进入任何禁止集合（禁止集合不存在）；诊断成员没有跨界载体。**输出分流相同 ≠ 语义相同**：`RESERVED` 在来源域之外（类型本身推不出普通 RAM 资格），`UNUSABLE` 在来源域之内、由类型资格排除；未来不能让实现者把两者合并成一个 `Excluded`。两者的差异要等 UEFI 原始描述符记录落地后才能兑现（O-12g 第二半）。
**结构校验的范围**：今天的几何检查（起点对齐、页数为零、末端回绕）只作用于**会输出区间的处置**（`geometry_is_checked_for_ranges_that_are_emitted`）。不输出区间的类型（`RESERVED`/`UNUSABLE`、类型 11/12）在结构上是否合法没有检查——这是 O-12h 要补的。
**一条必须说清的项目策略**：把引导服务代码/数据与 `ACPI_RECLAIM` 放进保留表并永不回收，是**第一版的选择**，不是 UEFI 的规范语义。规范允许它们在 `ExitBootServices` / ACPI 启用之后用于一般用途（见 O-01）。

### §2.7 主张-位置表（手工行号移出，符号锚点保留）

行号对应当前提交；复核时以**函数名**为准，行号只作定位。

| 主张 | 代码位置 |
|---|---|
| 两张表由同一次采集产出 | `os/libs/minix-boot/src/memmap_source.rs:282` `DtbMemoryMap::collect` |
| `/reserved-memory` 的查找与可见性检查 | 同文件 `:484` `lookup_reserved_memory` |
| UEFI 六步裁决 | `os/boot-shim/src/ebs_map.rs:479` `adjudicate` |
| 分类计数与提交 | 同文件 `:551` `classify`、`:619` `commit` |
| 恒等映射循环 | `os/kernel/src/lib.rs:934`（`while addr < IDENTITY_MAP_END`） |
| 引导根构造入口 | 同文件 `:894` `build_bootstrap_root_and_enable` |
| Step 4 直接映射（追加） | `os/kernel/src/dm_coverage.rs:97` `establish_boot_dm` |
| 配置表 DTB 的取得 | `os/boot-shim/src/uefi_helpers.rs:111` `find_platform_sources` |

### O-12j 状态表动态行（规范依据移 03）

| c 动态 `size` / `alignment` / `alloc-ranges` | **已实现**：`size`-only → `UnsupportedDynamicReservation`（`dtb_size_only_reservation_is_rejected_as_dynamic`）；无 `reg` 且无 `size` → 仍走 `UnsupportedReservedDeclaration`（`dtb_child_without_reg_and_without_size_is_an_unsupported_declaration`）；`reg`+`size` → 按 `reg` 处理、`size` 忽略（`reg_and_size_together_uses_reg_and_ignores_size`） | 形状分流与规范依据**均已闭合**（本轮取到主源原文）：`size` 独有=动态请求、「`reg`+`size` 按 `reg` 且忽略 `size`」、「Dynamic reserved memory regions must not be listed in the UEFI memory map because they are allocated by the OS after exiting firmware boot services」都是 DTSpec 3.5 逐字规定。因此 `UnsupportedDynamicReservation` 拒绝的是**规范允许、本项目不支持**的输入，不是非法 DTB。`alignment`/`alloc-ranges` 只在有 `reg` 时被忽略、未单独设错误——规范把它们定义为动态分配约束，静态处理时忽略是规定行为 |

### O-12c 四层约束与 APIC 包络论证

约束是：**每个 `DeviceAccessRequirement` 的地址都落在对应 `DeviceMappingCoverage` 内**；**coverage 的每一页都要有兼容的 `DeviceWindowPolicy` 授权**；未使用的 aperture 可以不映射；包络超出所需访问范围的部分仍要逐页通过授权与属性兼容检查。**不再写 `DeviceMappingRange ⊇ DeviceResourceRange`**——那会把「整个设备 aperture 必须映射」错当成普遍不变量。
**架构寄存器窗口、设备声明的 aperture 与页对齐映射包络是三件事**（x86 APIC 是最清楚的例子）：xAPIC 的寄存器接口只有 **4 KiB**，而当前政策授权是 `[0xFEE0_0000, 0xFF00_0000)`（**2 MiB**——上一版这里写 18 MiB，是我算错：`0xFF00_0000 − 0xFEE0_0000 = 0x20_0000`），IO-APIC 授权 `[0xFEC0_0000, 0xFEE0_0000)` 同样是 2 MiB。真正要回答的因此不是「授权范围太大」，而是**为什么 4 KiB 的实际访问需求要用一个 2 MiB 的设备映射叶子**。大范围映射本身不一定错：只要 profile 能证明整个 2 MiB 区域可按设备属性映射（不覆盖普通 RAM、地址空洞或属性不同的区域），设计就成立；但在 QEMU profile、实际 MMIO 资源描述与映射覆盖证据钉住之前，这个授权范围只是**待证平台假设**，不是已证事实。第一版若拿不出包络证明，就只映射实际需要的 4 KiB 区域、用更细粒度叶子。

### O-12c 观测记录带 VA 段

**观测记录必须带 VA，不能只有 PA**：`DeviceMappingCoverage` 若只是物理区间，就回答不了「这些页映射到哪个 VA」「访问点用的 VA 是否落在里面」「同一 PA 是否有多个别名、权限/属性/共享性是否兼容」。观测单元定义为 `ObservedDeviceMapping { VaRange, PaRange, LeafSize, Permissions, MemoryAttribute, Shareability, Tree, Phase }`，由它派生纯物理覆盖 `ObservedPaCoverage = Union(PaRange)`，审计仍写 `ObservedPaCoverage ⊆ DevicePolicyAuthorizedPaRange`。访问需求不能只拿 PA 包络比覆盖：每条 `DeviceAccessUse` 要能证明其访问 VA 经对应叶子解析到预期 PA，且宽度、读写与属性兼容。这与 O-05 的别名审计是同一条要求——不能在要求审计别名的同时让覆盖集合丢掉 VA。

### O-12c 64 KiB 分组规则逐条论证

**AArch64 的 64 KiB 分组属性规则：按物理地址分组、只比分组属性、限定 UEFI profile**：UEFI 规定若一个 64 KiB 物理区间含任一 4 KiB 子页属于 `RuntimeServicesCode`/`RuntimeServicesData`/`ReservedMemoryType`/`ACPIMemoryNVS`，则该区间内全部 4 KiB 子页必须采用相同的 ARM memory page attributes。规范文本里的 `EfiReserved` 就是本项目的 `EfiReservedMemoryType`，是同一类型的两种写法，不是两个类型。规范位置为 UEFI 2.11 §2.3.6（评审人已按该节核对：引用 Table 2.5，权限映射在 Table 2.6）；本方本环境抓取 uefi.org 返回 403，**未直接取证**——两侧分开记：规则内容有评审人核对支持，「本方独立取证」仍是缺口，因此不升级为跨入口的架构通则。四条精度约束：

- **分组键是物理地址**：`AttrGroup(PA) = floor(PA / 65536) * 65536`。同一 VA 在不同树里的别名不改变分组，不同 VA 映射到同一 `AttrGroup` 才需要比较；按 VA 分组会漏掉跨别名违例。
- **比较字段只限分组属性**：memory type（Normal/Device 及其子型）、inner/outer cacheability、shareability。`R/W/X`、`PXN`/`UXN`、`AF` 等**访问权限逐页独立**（Table 2.6 一支），不进一致性比较——把权限塞进分组会禁掉合法的「同组不同权限」拆分，反向也会放过属性不一致。
- **比较对象是叶子最终生效的属性，不是 `AttrIndx` 数字**：写成 `EffectiveArmMemoryAttributes = (ResolvedMAIRAttrByte, Shareability)`，或规范化成等价的内存类型 + 内/外层缓存属性 + 共享性语义。两个不同 `AttrIndx` 若解析出完全相同的有效属性，不因索引编号不同而判冲突；反过来，同一 `AttrIndx` 在不同 PE 上对应不同 MAIR 内容**不算通过**（只读回索引相等证明不了任何事）。权限仍逐页独立，这条与上一项合起来才既不放纵违例、也不误禁合法的同组不同权限映射。
- **适用性按 profile 判定，不是无条件全局**：这条约束随 AArch64 **UEFI 启动 profile** 的属性递交而来；非 UEFI 入口（`-kernel` + DTB）没有 EFI memory map 属性递交，规则不自动适用，但架构侧是否也按 64 KiB 粒度取属性属另一项取证，未取到之前记为 profile 条件而非普适事实。第一版锁定 4 KiB granule **不等于**规则不适用。
- **未映射子页要有组级覆盖登记**：一致性检查的对象是同一 `AttrGroup` 内**已建立叶子**的属性集合；组内其余子页尚未映射不构成违例，但审计须记 `GroupCoverage`（该组已映射哪些子页）。任一后续新映射（低 4 GiB 宽恒等映射拆分、改属性尤其）都受既有分组属性约束；若某子页必须取不同的分组属性，只能整组改属性或拒绝，不能局部改。

### O-12c x2APIC 审计细节

**x2APIC 是第三种访问域**：xAPIC 走 MMIO（`0xFEE0_0000` 页），x2APIC 走架构寄存器（RDMSR/WRMSR）——它既不属于 MMIO 窗口模型，也不属于 O-08 的端口 I/O 地址域。**MSR 编号不进共享层**：共享层只表达 `ApicRegisterAccessRequirement`（APIC 逻辑寄存器、读写方向、访问者、阶段、CPU 范围、适用模式）；x86 架构层用 `X2ApicMsrUse` 记录具体 MSR 编号、指令位置、包装函数与访问约束。同一 OS 语义（读 Local APIC ID、写 EOI）在 xAPIC 映射到 MMIO、在 x2APIC 映射到 MSR。**第一版若不实现 x2APIC 访问路径**，必须锁定 xAPIC profile 并在检测到 x2APIC 时拒绝启动；不得「允许 x2APIC 模式却保留 xAPIC 的 MMIO 访问需求」。O-10 现有的 in/out 负控不覆盖 RDMSR/WRMSR：**架构寄存器访问必须进入独立的原始操作审计**——允许出现原始 MSR 访问的函数与指令位置清单、MSR 编号与共享层逻辑寄存器的对应关系、在已批准的 `asm!` 块中加第二条 `rdmsr`/`wrmsr` 必须使审计失败、x2APIC 模式下不得残留 xAPIC LAPIC MMIO 访问义务。

### O-12c APIC 逐处理器核对

**x86 的 APIC 地址不是无条件硬件事实，而且核对是逐逻辑处理器的**：表中 `0xFEE0_0000`/`0xFEC0_0000` 只属于「本项目支持的某个 x86 QEMU profile」。`IA32_APIC_BASE` 是**每处理器 MSR**、xAPIC/x2APIC 模式也按处理器启用，因此不能只在 BSP 上核对一次——这条按「评审人已核的架构事实」使用，本方未取 Intel SDM 原文，登记为待核，不写成本方已验证的引用。启动与次级处理器入场时，对**每个参与内存访问与调度的逻辑处理器**分别核对：该处理器的 Local APIC 当前模式（xAPIC MMIO 还是 x2APIC MSR 路径）、该处理器的 `IA32_APIC_BASE` 基址与政策表一致、该处理器的 LAPIC 寄存器窗口是否已按对应模式获得映射或 MSR 路径准入。只读 BSP 不构成全平台证明——一个 AP 的基址不同，「政策授权一个 4 KiB 页」这条主张就不再对所有参与者成立。若解析 ACPI，MADT 的 Local APIC 地址、每个处理器条目的 processor ID 与 enabled flags、override 与 I/O APIC 项都要与政策表兼容；任一参与处理器不符合当前 profile 就**拒绝启动**或**阻止该处理器入场**。riscv 侧是同一形态的问题：PLIC 的 context 按 hart 列出（`interrupts-extended`），逐 hart 核对，不能由 hart 总数乘 2 推出来（见 PLIC 推导段）。

### O-12c PLIC 访问集合段

**PLIC 的访问集合与包络**：`DeviceAccessUseSet` 的每条记录至少含 `AccessUseId`、`DeviceId`、`ObjectId`、**`VirtualByteRange`（或其可静态解析出实际 VA 的访问点标识）**、`PhysicalByteRange`（预期 PA）、访问宽度、读/写、阶段与路径。**缺 VA 就无法证明访问需求**：指针实际落在哪个别名上，只有 VA 能回答；不能先由现有 PTE 推出 PA 再充作访问需求输入（那是循环证明）。`DeviceRequiredAccessEnvelope = Envelope(DeviceAccessUseSet)`，**不表示包络内每个 byte 都被访问**。没有完整访问清单时，包络只是保守上界，不能宣称访问需求已完整（与 O-07 相连）。PLIC 的 profile 还必须钉住具体的 QEMU 构建版本（`qemu-system-riscv64 --version` 读数与 machine version）；今天尚未记录该版本，属于待补证据。

### O-12c PLIC 推导与窗口教训合并

**PLIC 范围的推导与 profile 条件**（合并上一版两段重复表述）：`N_context = hart_count × 2` 不是通用公式——QEMU TCG 用奇数 context ID（`2·cpu+1`），KVM 用连续编号；不支持 S-mode 的 hart 还会造成 context 数差异。所需范围写成 `ContextBase + (max(实际使用的 context ID)+1) × Stride`；context ID 不能由 hart 数直接乘 2，也不能把 `interrupts-extended` 的 interrupt specifier 当成 PLIC 的 MMIO `addrid`。推导链：specifier → 解析 `interrupt-parent` phandle → 识别 hart 与特权模式 → 确定 context 排列 → 枚举稠密 `addrid` → 最大偏移。本项目的 `0x24_0000` **只在以下配置下成立**：QEMU `virt` + 固定 machine version + TCG + 单 socket + AIA 关闭 + 明确 CPU 型号 + `-smp ≤ 32` 且全部支持 S-mode；其它配置必须拒绝或重新推导。

### O-12g 附加要求四条

原始记录保留：声明所在节点、**每一级 `ranges` 转换的输入/输出与引用节点**、最终 CPU 物理地址、属性与来源身份。只保留最终地址会让审计无法解释地址是怎么来的。
**四条补充要求**：① **排除原因多重归属**——同一页可同时被普通保留、MRB、存活对象覆盖，PMM 只扣一次，但审计能列出全部 `SourceId`；② 固件前缀、内核镜像、boot shim、自举页表、栈、启动模块、DTB 原始对象与副本、登记表**不能合并成一个 Reserved 标记**，它们各自有阶段化读写/执行需求；③ **重叠规则**：**同一权威来源内部的同语义重复**（例如两个 `/memory` 节点描述同一段 RAM，或同一节点多个 `reg` 重复）集合去重、保留多个 `SourceId`；**跨来源证据**（UEFI 类型、平台分类、固件保留与 RAM 来源）对同一物理区域提出**不兼容主张**时进入冲突裁决，不能靠并集掩盖——不兼容即拒绝启动。`/memory` 本身只有 RAM 语义（`status` 非 `okay` 与 `hotpluggable` 已被准入拒绝），不存在「同源一条说 RAM、一条说不可用」的形态；④ **原始记录必须保留不输出区间的描述符**（含 `RESERVED`/`UNUSABLE`）——否则 O-06 的「`UNUSABLE` 属于来源域」没有可实现的输入；诊断成员表是面向内核的另一层，不能替代原始记录。

### O-12g SourceRef 定型段

**统一 `SourceRef` 必须在接通启动路径前定型**（未实现）：`SourceRef { input_object: InputObjectId, local_source: LocalSourceId }`；`FirmwarePrefix` 对 `Memory` 的引用也用它，不再保留裸 `memory_id`。**UEFI 的 `InputObjectId` 是快照实例身份，不是来源类别**：每次成功的 `GetMemoryMap()` 分配新的 `MapSnapshotId`（`MapKey` 只是该快照的固件版本凭据）；`PreflightMap` 不产生最终政策记录，`FinalMap` 的全部描述符同属一个快照 ID，stale-key 重试产生新 ID 且旧记录与旧投影整体废弃——否则三份快照里同号的 `DescriptorIndex` 会被当成同一条来源。这样每条政策、排除原因、禁止项、对象义务共用同一身份，也防止把两个输入对象里的 `SourceId(3)` 当成同一条来源。节点路径文本与逐级 `ranges` 转换链可分阶段补，不必为接通内存记录先做完整地址翻译。

### O-12i 保护清单与第一版二选一

第一版对每个 `PlatformMemoryProfile` 二选一：证明不存在未建模的额外保护结构（此时 `PlatformProtectionPermits` 由该证明通过），或把已声明结构纳入 `PlatformProtectionPermits` 逐区间判定。`completeness` 只能由 profile 提供，**不能默认为真**。
**证据稳定性：`RLB` 进稳定性证明，不是只登记**。`norm:mseccfg_rlb_op_warl` 说 `RLB` 提供「临时修改 *Locked* PMP 规则」的机制，即 `RLB=1` 时 `L=1` 的条目仍可被改，所以「`RLB` 不参与位解释」不能当作证据有效期依据。完整合同四条：① 依赖 `L=1` 保证稳定时，必须证明 `RLB=0` 且无法再置位；② 只要求 `RLB=0` 也不够——没有 `L` 的条目仍可由 M-mode 修改；③ 真正要证明的是**整份有效配置**在 `stability_until` 内不变，而且**事后通知不够**：provider 改完再通知，旧投影可能已被分配器或访问路径使用，事后递增代次无法撤销已经发生的非法访问。第一版只接受两种合同之一——**A 不可变配置合同**（provider 承诺从提供证据到生命周期结束不修改相关 PMP、Smepmp 与额外保护配置；第一版选它）或 **B 协调式变更合同**（变更**前**通知内核，内核停止相关访问、完成受影响对象检查与必要的迁移或隔离，再允许配置变化，完成后递增代次、重建投影、重新开放访问）；`stability_until` 因此是 provider 对「配置不可变」或「变更受协调」的**承诺**而非时间期限，没有该承诺的证据不得用于任何跨阶段分配或访问授权；④ 任何改变生效配置的事件（M-mode 改写 `pmpcfg*`/`pmpaddr*`、写 `mseccfg`、hart 上线使该 hart 的配置进入参与集合）都必须递增 `evidence_generation` 并使全部依赖投影失效。拿不到承诺与通知的 provider，`stability_until` 只能记「未声明」，其证据不得跨阶段缓存。

### O-12c PLIC 窗口教训（内容已并入 PLIC 推导段）

**PLIC 窗口的教训（本轮修正）**：context 区从 `+0x20_0000` 开始，窗口大小应为 `0x200000 + N_context×0x1000`，**不能写固定 2 MiB**——原来的 2 MiB 恰好结束在 context 区起点，claim/complete 寄存器拿不到 `DEVICE` 映射。QEMU 实测 DTB：`plic@c000000` 的 `reg` 为 `0xc000000 + 0x600000`。窗口按内核 SMP 上限 32 hart 取 `0x24_0000`，仍在 QEMU 的 PLIC 区间内。

### O-11 顶端边界段去本轮记号

**统一物理范围类型 `PhysicalRange { start: u64, end_exclusive: u128 }`**（`start < 2^64`、`start < end_exclusive ≤ 2^64`）：DTB 的 `address + size` 与 UEFI 描述符都能表达 `[0xffff_ffff_ffff_f000, 2^64)`，统一来源记录不得因 Rust 字段类型丢掉这个合法边界。原始输入字段保留各自编码（DTB 的 address+size、UEFI 的 start+pages），规范化层才转成 `PhysicalRange`；`UnsupportedPhysicalAddressEnd` 只在项目明确选择拒绝该边界时使用，**不能成为 `u64` 加法的副作用**——DTB 腿本轮已按这条实现（`physical_end` 用 `u128` 求和并分名两类拒绝，见 O-12i 的三层表）。这条与 O-12h 结构校验、O-12i `FullPages` 是同一条边界合同；DTB 记录层**已按此落地**，UEFI 侧的 `SourceRecord` 尚未接入。

### O-01 三层不得串层段的 §6 指向改写

**三层各自的说法不得串层**：第一层只有「Not usable」这一个事实，它既不承诺硬件会拦访问，也不等于本项目的 `DenyOsUse`；第二层才是项目政策的具名主张；第三层是「类型本身不兑现保护，映射隔离与访问阻断由内核建立」。两个方向的越层都要防：把第一层写成「硬件禁止访问」是拔高，把第三层「类型本身不兑现保护」反过来读成「所以第一层其实没那么强」是压低——后者尤其危险，因为 `DenyOsUse` 的强度来自第二层政策，不依赖硬件是否拦得住。UEFI 原文在本环境抓取返回 403，「Not usable」措辞与 `GetMemoryMap()` 的两段区分都由**评审人按 UEFI 2.11 核对**，本方独立取证仍是缺口，登记在 §6，不升级成本方已验证的引用。

### O-12i A/D 取文记号

**RISC-V 的 A/D 更新方案决定页表页准入**（不能无条件要求 `RsrvEventual`）。规范原文本轮已从主源 `riscv/riscv-isa-manual` 的 `src/priv/supervisor.adoc` 与 `src/priv/machine.adoc` 直接取到，逐字引句带规范锚点：

### O-12i 负控表反例说明去上一版

| **Svadu 已实现、`ADUE=1`、Svade 也已实现** | **硬件更新**——这条就是上一版布尔式的反例，必须能失败旧公式 |

### O-12i adapter 边界段裁定记号

**共享层与架构 adapter 的边界（本轮裁定，属重构项而非 RISC-V 算法错误）**：通用内存资格模型的接口只表达 OS 需要的**语义**——普通物理访问要求、页表读取与更新要求、原子操作的宽度/原子性/进展、执行访问要求、参与访问的 CPU 集合、证据代次与有效期；具体机制名属于各架构 adapter：参与访问的 CPU 集合（RISC-V hart / AArch64 PE / x86 逻辑处理器）、页表更新要求（`Svade`/`Svadu`/`ADUE` 与 `RsrvEventual` / 硬件 `AF`+dirty / 硬件 A/D）、原子能力（`AMO(family)`/`Lr`/`Sc` 与预约集几何 / `exclusive`+`LSE` / 锁定 `lock` 前缀族）、物理访问保护（PMP+PMA / MAIR 与层级保护 / PAT+MTRR 与分页）。

## 五十五、第八十三轮：现状冲突的代码证据与本轮门读数

| 项 | 读数（命令与输出） |
|---|---|
| 恒等段宽度是常量而非按来源裁剪 | `os/arch/src/lib.rs` 的 `pub const BOOT_IDENTITY_MAP_END: u64 = 0x1_0000_0000;`（全仓唯一赋值点，三架构共用） |
| 恒等循环不消费 memmap 与禁止集 | `os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable` 内 `while addr < IDENTITY_MAP_END` 的循环体只调用 `identity_device_windows()` / `identity_block_touches_device` / `identity_piece_is_device`，无 memmap 引用；`map_huge` 的 `Result` 被 `let _ =` 显式忽略 |
| memmap 在内核启动路径的两处消费 | 同文件 `support.phys_addr_limit() >= IDENTITY_MAP_END` 断言，以及 boot allocator 候选区挑选（`min(IDENTITY_MAP_END, VM_DIRECT_MAP_SIZE)` 上界）——两者都不参与建叶子裁决 |
| 设备窗口来自静态表 | `fn identity_device_windows`：aarch64 用 `MMIO_PA_BASE`/`MMIO_WINDOW_LEN`，riscv64 用 `minix_arch::kernel_identity_mmio_regions()` |
| 恒等段无撤销路径 | `grep -n "unmap" os/kernel/src/lib.rs os/kernel/src/dm_coverage.rs` 只命中 heap 窗口驱逐与注释，无恒等段拆表代码 |
| 表征测试钉住宽映射 | 宿主测试 `identity_piece_policy_covers_low_4gib_and_marks_device_windows` 断言覆盖至 `IDENTITY_MAP_END`、`id_flags` 含 `EXECUTABLE`、不含 `USER_ACCESSIBLE`（本表为源码读取，未在本轮重跑 minix-kernel 宿主套件） |
| 门读数 | 正文 1681 行（上限 1700）、125,876 字符；围栏 48（偶）；表格 56 块、列数不一致 0 处；`doc-style-lint --diff` error 0 |
| 零丢失对账 | `tmp/i29-r83compress/check.py diff`：符号名 240、测试名 56、`norm:` 锚点 30 → lost 0（尺子与被比文件在 `tmp/`，不入库；逐字移出原文见 `02` 第七节与本节所属批次） |
| 本轮移出正文的原文 | O-06「已知属性位的处置」与「属性按语义分三类」两段合并（原文逐字进 `02` 第七节）；16 处段落空行为纯格式合并，无文字移出 |

## 五十六、第八十四轮：甲案代码证据、判定核抽出与门读数

| 项 | 读数（命令与输出） |
|---|---|
| 必需叶子失败即拒绝的先例在同一个函数里 | `os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`：Step 1 恒等循环 `let _ = paging.map_huge(...)`（注释理由为「部分范围可能没有物理内存支撑」）；Step 2 高半内核映射 `.expect("kernel map: map_huge failed — kernel image mismatch")`。甲案换成授权驱动输入后，前者容忍失败的理由消失 |
| 建表 API 的失败种类 | `os/arch/src/arch/paging_ext.rs`：`fn map_huge(&mut self, VirBytes, PhysBytes, PageSize, PageFlags) -> Result<(), PageTableError>`；`os/arch/src/arch/paging.rs`：`pub enum PageTableError { InvalidAddress, AlreadyMapped, NotMapped, AllocationFailed, PermissionDenied, NotSupported }`（三架构实现分别位于 `os/arch/src/{riscv64,x86_64,arm64}/paging.rs`） |
| 执行闭包不是「只映射当前页」的实证 | O-12j 跳转表段：预扫逻辑代码页已在映射内，`switch` 被编译为 `.rodata` 跳转表 + `jr` 绝对地址仍触发 RISC-V instruction access fault（`cause=1`）；`-C jump-tables=no` 后原故障复现消失 |
| 本轮新增符号（正文实测计数） | `AccessRequirementsEligible` **5**、`PageTableRequirementsComplete` **3**、`PageTableAccessRequirements` **3**；残留 `progress_guarantee=Eventual` **0**（改为 `ReservationEventual`） |
| 复核方式的更正 | 上一轮按 `progress_guarantee = Eventual`（带空格）grep 得出「残留已清」是漏检；本轮改为 `grep -n "Eventual"` 全量列出逐处判读，命中 20 处中仅 1 处为未定义成员，已改 |
| 篇幅与门 | 正文 1689 行（上限 1700，余 11 行）、232,200 字节；围栏 48（偶）；表格 57 块、列数不一致 0 处；破表（表格首行前无空行）计数 **0**；`doc-style-lint --diff` error 0；零丢失对账 lost 0 |
| 证据基线（送审绑定） | 上一轮合并提交 `53c44e846`（§6 已显式写出该哈希；146/70 读数与之绑定，下一轮随合并刷新） |

## 五十七、节号体系与重复号说明（本轮核实后追加）

> **为什么不重排**：纪律 3 规定副文档只增不改，既有节号是历史引用的锚点——正文与其它档案都按节号指回对应材料，重排会让旧引用失效。本节只把混乱说明清楚，不动任何既有节号。
>
> **触发原因**：用户怀疑笔记树迁移弄丢了内容。核实结论是**没有丢失**，证据三条：`01` 与 `03` 的创建提交都晚于那次三棵树迁移，`rewrite-notes/MIGRATION.md` 里 `grep -c memmap` = 0，说明这套文件从未进过迁移对照表；`git log -S"## 十一、" -- 03…md` **无任何命中**，即 `十一~十三` 从未存在过，是追加时跳号而非删过内容；第二十二轮的材料在 `02` 与 `04-design-backlog-round28.md` 中都在。

**本册现有三套节号并存，各自含义如下**：

| 节号形态 | 覆盖范围 | 编号依据 | 已知问题 |
|---|---|---|---|
| `一、`…`十五、` | 从送审正文移出的材料与材料包 | 移入时的先后 | **跳号**：`十、` 之后直接 `十四、`；`## 三、` **出现两次**——一处在「本册的维护约定」，一处在「UEFI 内存图描述符的形状（原正文 §0.2，第十一轮移入）」，两者是不同的材料，不是重复条目 |
| `一b、`…`一r、` | 第二十七轮到第四十三轮的逐轮读数 | 母节 `一、` 的分项 | 只到 `一r`（第四十三轮）即停，之后改用下面的独立序号 |
| `十六、`…（本轮起 `五十七、`） | 第四十四轮起的逐轮读数、门读数与移出原文 | **独立递增**，与轮次号一一对应但不相等（例如 `五十五、` 记第八十三轮、`五十六、` 记第八十四轮） | 号与轮次号差一位以上，检索时必须按「第 N 轮」文字而不是按节号猜轮次 |

**今后约定（不回改历史）**：新增逐轮材料一律继续用本册递增的中文数字节号，并在节标题里同时写明**轮次或批次**（形如「五十八、第八十六轮：…」）；需要引用被移出的正文原文时，指向 `02` 第七节（过程痕迹与已撤销判断）或本册对应节号，不新建第四套编号体系。

## 五十八、第八十七轮：分配路径的更正读数与 arena 事实

> **本节推翻 `五十五、` 与 `五十六、` 中的两条读数**（那两节按只增不改保留原文）：其中「`boot_pt_alloc` 在非测试代码里零调用者，因此建根与 Step 4 不新建页表页」是用错了被 grep 的函数名——真实路径是 `pt_alloc::alloc_pt_page`，分配确实发生。

| 项 | 读数（命令与输出） |
|---|---|
| 2 MiB 叶子缺中间表时分配 | `os/arch/src/riscv64/paging.rs`：`// Allocate a new L1 page table.` 后接 `let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;`；同文件另有三处 `alloc_pt_page()` 调用（含根页安装） |
| 提供方是 bump 分配器 | `os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page` 读注册函数指针；`os/kernel/src/boot_alloc.rs` 注释逐字：「Called by Paging implementations via `pt_alloc::alloc_pt_page()`」；运行时提供方是 VM（`os/servers/vm/src/alloc_page.rs`） |
| 降级已有大叶子被拒 | 同文件：L2 项为 1 GB 叶子时 `return Err(PageTableError::AlreadyMapped)`，注释「Demoting it to a table pointer would silently corrupt the existing 1GB mapping」；该错误在恒等循环被 `let _ =` 吞掉 |
| arena 容量与身份 | `minix_boot::ARENA_CAP = 4 * 4096`（`os/libs/minix-boot/src/handoff_v1.rs`）＝16 KiB，四页 |
| arena 分配与传递 | `os/boot-shim/src/uefi_helpers.rs:fn alloc_arena()`：分配恰好 `ARENA_CAP` 字节的 `LOADER_DATA`、限定在恒等界以下；`let arena_pa = alloc_arena();` 后以 `x0`（aarch64）/`rbx`（x86_64，trampoline 移到 `rdi`）递进内核 |
| arena 使用区间 | 写：`os/kernel/src/lib.rs` 根激活后 `patch_runtime_fields_in(arena, …)`；再读：`pub fn arch_boot_resume_v1(arena_pa: u64)` 内 `from_raw_parts(arena_pa …, ARENA_CAP)`。**释放点未登记** |
| 复核方式教训 | 同一会话内第二次「检查手段太窄却当成结论成立」（前一次是 `progress_guarantee = Eventual` 的带空格 grep）。今后涉及「谁分配了什么」必须按**被调函数名**与**注册点**两处 grep 交叉验证，不能只查一个符号 |
| 门读数 | 正文 1689 行（≤1700）、围栏 48（偶）、表格 58 块列数不一致 0、破表 0、`doc-style-lint --diff` error 0、横幅门 PASS、零丢失对账 lost 0；无代码改动，沿用 146 / 70 |

## 六十、第八十九轮：范围回缩的读数与分页侧专项的交接清单

> 本节保存**从主文档撤出的分页机制取证读数**——它们技术上有效，只是不属于 I-29。分页侧专项开工时以本节为起点，不必重新取证。

| 事实 | 读数（本轮逐字复核） |
|---|---|
| 共享层降级接口 | `split_huge` 是 **trait 方法**：`os/arch/src/arch/paging.rs:fn split_huge`（默认实现），三架构各自覆盖：`riscv64/paging.rs`、`x86_64/paging.rs`、`arm64/paging.rs`；x86 与 arm64 各有降级算术的宿主测试。**`fn demote` 不存在**（`grep -rn "fn demote" os/arch/src` 零命中） |
| `map_huge` 的 `AlreadyMapped` 含义 | riscv64 `map_huge` 的 L2 分支注释逐字：`L2 entry is already a 1GB leaf ... Demoting it to a table pointer would silently corrupt the existing 1GB mapping. Return AlreadyMapped to signal the conflict`——即**它自己绝不降级**；降级必须显式调 `split_huge` |
| 缺中间表时的分配 | riscv64 与 x86_64 的 `map_huge` 都含 `let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;`；分配器是注册制函数指针，引导期提供方为 bump 分配器（`os/kernel/src/boot_alloc.rs` 注释：Called by Paging implementations via `pt_alloc::alloc_pt_page()`），bump 区按文件头注释是 identity mapping (VA = PA) |
| 建表顺序 | Step 4 的 `establish_boot_dm` 在 `paging.enable()` **之后**执行（`os/kernel/src/lib.rs` 的 `arch_boot` 与 `enter_kernel_image_v1` 两条路径皆然）；今天新表页可写，是因为宽恒等段恰好覆盖了 bump 区 |
| 产线降级先例 | `harden_kernel_text` 与 heap 窗口驱逐在活动根上调用 `split_huge` 并在失败时 `panic!` |
| 交回 I-29 的条件 | 分页侧专项只需回传一件事给本项：**授权计划是否可被完整兑现**（实际 PTE 审计的双向判据结果）。粒度、容量、地址通道的结论不需要再进主文档 |

## 六十一、第九十轮：PLIC 授权范围被叶子粒度反推的证据与门读数

| 项 | 读数（命令与输出） |
|---|---|
| 资源长度 | `os/arch/src/lib.rs`：`pub const PLIC_RESOURCE_LEN: u64 = 0x0020_0000 + 2 * PLIC_MAX_HARTS * 0x1000;`（注释：priority/pending/enable 各 `0x20_0000` 加每 hart 的 M/S 两个 context）；32 hart 时 `0x24_0000` |
| 授权窗口由粒度算出 | 同文件：`const PLIC_WINDOW_LEN: u64 = (PLIC_RESOURCE_LEN + 0x1F_FFFF) & !0x1F_FFFF;` 前置注释逐字「The Step-1 identity map emits device windows in 2 MiB pieces, so the authorized mapping range must be the 2 MiB envelope of the resource range — a window ending at `0x24_0000` is covered by a piece reaching `0x40_0000` anyway」；另有 `assert!(PLIC_WINDOW_LEN.is_multiple_of(0x20_0000))` 与 `<= 0x60_0000` 两条编译期断言 |
| 平台侧真实 aperture | QEMU virt DTB `plic@c000000` 的 `reg` = `0xc000000 + 0x600000`（见 O-12c 与 `03` 前轮读数），**不等于** 4 MiB 授权值 → 授权数值缺独立平台证明 |
| 窗口表来源 | `kernel_identity_mmio_regions()`（riscv64）返回 `&[(0x0C00_0000, PLIC_WINDOW_LEN), (0x1000_0000, 0x1000)]`；aarch64 返回空数组（GIC 走系统寄存器），x86_64 为 IO-APIC/LAPIC 各 4 KiB |
| 门读数 | 正文 1696 行（上限 1700）、243,995 字节、围栏 50（偶）、表格 59 块列数不一致 0、破表 0、`doc-style-lint --diff` error 0、横幅门 PASS、零丢失 lost 0；无代码改动，沿用 146 / 70 |

## 六十二、第九十一轮：自正文移入的属性位矩阵、PMP 与 Smepmp 取证、待补测试穷举

> 本节收纳**已裁定并已实现**的取证细节与**逐组测试穷举**：它们不再参与第二层「问题是否成立、候选是否覆盖失败形态」的裁决，正文只留结论与指向本节的指针。**移出不等于失效**，逐字保留。


### O-06 属性位矩阵、版本读数与两段叙述

**`EFI_MEMORY_HOT_PLUGGABLE` 与语义歧义位（两个不同的位）**：

- **冲突位域整体 fail-closed（已实现）**：`0x1000..=0x1_0000` 在两套官方编码里含义不同（见下方矩阵），任一读法都可能把必须排除的内存伪装成元数据——`0x4000` 在一套里是 `SP`（必须排除出 PMM）、另一套里是 `XP`（仅保护能力元数据）；`0x8000` 是 `CPU_CRYPTO` vs `NV`；`0x1000` 是 `MORE_RELIABLE` vs `WP`；`0x2000` 是 `RO` vs `RP`；`0x1_0000` 是 `HOT_PLUGGABLE` vs `MORE_RELIABLE`。整个 `0x1_F000` 因此在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed` 与 `the_ambiguous_bit_fails_closed_under_either_name`）。`0x0010_0000`（仓库编码的 `HOT_PLUGGABLE`）在 `uefi-raw 0.9` 无该常量，走未知位拒绝（`hot_pluggable_bit_fails_closed_as_unknown`）。
- **代价与恢复路径**：`NON_VOLATILE` 位现在也落进拒绝域——模型仍认识 NV 语义（`TYPE_INDEPENDENT` 保留该分支），但**当前分类器不接受带 NV 位的输入，该分支在真实输入上不可达**。恢复它需要**两步**：先钉住带版本证据的固件编码 profile，再把 `0x1_F000` 的无条件拒绝改为按 profile 解释——只要无条件拒绝还在，仅添加 profile 记录仍然解不开 `NV`。若决定后续版本也不支持该属性，应明说，不要让读者以为 profile 一到手就自动解锁。`SPECIAL_PURPOSE`（`0x4_0000`）属 `ProfileAttribute` 拒绝域、低位 `0x4000` 属冲突拒绝域，**当前同样不可达**。钉住的是「当前 `uefi-raw` 与 Linux 头文件一致的编码」这一事实，而不是某份 PDF 的表格；将来升级 `uefi-raw` 时**必须重核全部已识别属性位**，不能只迁移 hot-pluggable 一位。

**高位属性改为 profile 门控（已实现）**：`RO`/`SP`/`CPU_CRYPTO` **在 UEFI 2.10 原始发布版里就是 `0x2_0000`/`0x4_0000`/`0x8_0000`**，2.10 Errata A 与 2.11 PDF 把它们移到低一组。所以问题不是「已发布规范没有这些位置」，而是**两个正式发布版本族对同一属性采用了不同位值**，而内核无法从 `uefi-raw` 常量判断启动固件用哪一版。三者因此在 profile 未钉住前直接拒绝（`PolicyError::ProfileAttribute`，测试 `profile_only_attributes_fail_closed_without_a_pinned_profile`）；`0x10_0000` 仍由未知位闸拒绝。将来 `UefiMemoryAttributeProfile` 必须绑定**实际启动固件镜像身份**（x86 与 aarch64 分别记录，不能假设同源），并**同时钉住低位组与高位组的完整映射**——只授权三个高位会得到「部分属性可识别、其余冲突位永久拒绝」的半套 profile。PDF 低位组的 `0x100`–`0x800` 落在本项目已知位集合之外，同样被未知位闸拒绝；规范仓库 `main` 是活动分支，不能当固定版本。
**属性位完整矩阵（两组编码的全部相关位）**：

| 属性 | UEFI 2.10 原始版 | 2.10 Errata A / 2.11 PDF | EDK2 / Linux / `uefi-raw 0.9` | 本项目当前处置 |
|---|---|---|---|---|
| `WP` | `0x1000` | `0x100` | `0x1000` | 冲突域 → 拒绝 |
| `RP` | `0x2000` | `0x200` | `0x2000` | 冲突域 → 拒绝 |
| `XP` | `0x4000` | `0x400` | `0x4000` | 冲突域 → 拒绝 |
| `NV` | `0x8000` | `0x800` | `0x8000` | 冲突域 → 拒绝 |
| `MORE_RELIABLE` | `0x1_0000` | `0x1000` | `0x1_0000` | 冲突域 → 拒绝 |
| `RO` | `0x2_0000` | `0x2000` | `0x2_0000` | `ProfileAttribute` → 拒绝 |
| `SP` | `0x4_0000` | `0x4000` | `0x4_0000` | `ProfileAttribute` → 拒绝 |
| `CPU_CRYPTO` | `0x8_0000` | `0x8000` | `0x8_0000` | `ProfileAttribute` → 拒绝 |
| `HOT_PLUGGABLE` | 未定义 | `0x1_0000` | `0x10_0000` | `0x1_0000` 冲突域；`0x10_0000` 未知位 |

PDF 低位组的 `0x100`–`0x800` 在本项目里落入已知位集合之外，同样被未知位闸拒绝。规范仓库 `main` 是活动分支，不能当固定版本。

> 版本读数（本方直取的源与评审人核对结果一致）：UEFI 2.10 PDF（2022-08）`WP=0x1000`、`MORE_RELIABLE=0x10000`、未定义 `HOT_PLUGGABLE`；2.10 Errata A（2024-08）与 2.11 PDF（2024-12）为 `WP=0x100`、`MORE_RELIABLE=0x1000`，2.11 另给 `HOT_PLUGGABLE=0x10000`；EDK2 与 Linux 头文件同 2.10 原始版低位（`HOT_PLUGGABLE=0x100000`），**具体 commit/tag 尚未钉死**（`main` 会漂移，不能作长期冻结证据）；`uefi-raw 0.9` 的 crate 常量为 `WRITE_PROTECT=0x1000`、`MORE_RELIABLE=0x10000`，无 hot-pluggable 常量。

### O-08/O-09/O-10 三条已裁定的展开叙述

### O-08 端口 I/O 的地址域（已裁定：采用 A）

端口范围用独立的半开区间与 checked 运算，不做物理页量化、不进 PMM、不进页表、不进 MMIO 记录、不进物理地址跨界 ABI；`PortWindowPolicy` 与端口访问需求都是编译期静态输入。确需跨界时另立 ABI（候选 B，暂不设计）。
**判据**：端口与物理地址在任何集合与取整函数里都不混用。**消费者**：端口访问审计。

### O-09 诊断成员与样本的存储登记（已裁定：采用 A）

完整诊断成员表与操作者可见的有界样本表各自登记：存储范围、容量、写入者、读取者、最后使用阶段。防止切根后继续引用已失效的引导期缓冲。
**判据**：容量清单里每一项都能在对象目录里找到位置与生命周期。**消费者**：容量清单。

### O-10 R13-A 的第二枚负控（已裁定：采用 A）

逐个登记允许出现原始操作的函数；对函数体内原始操作的数量与位置做检查；新增原始操作必须显式更新审计合同。**两枚负控**：① 在已批准函数体内注入第二条原始操作；② 在**已有的 `asm!` 块内**加入第二条 `in`/`out` 指令。审计器必须检查展开后或结构化语法层面的真实操作，不能只数 `asm!` 宏调用次数。
**判据**：该负控生效。**消费者**：CI 审计器。

### PMP 权限表、三条配套、cache-block 与 Smepmp 取证

| 操作 | 查的位 | 有效特权级 | 依据锚点 |
|---|---|---|---|
| `Load` / `Lr` | **R** | S 或 U | `norm:pmp_load_fault` |
| `Store` / `Sc` / `Amo(·)` / cache-block zero | **W** | S 或 U | `norm:pmp_store_fault`（规范的故障归类把 AMO 与 cache-block zero 划到 store 侧） |
| `Fetch` | **X** | S 或 U | `norm:pmp_exec_fault` |
| 隐式页表**读** | **R** | **S**（固定） | `norm:pmp_check_pagetable_access` ＋ `norm:pmp_cfg_permissions` |
| 硬件 A/D **隐式写** | **W** | **S**（固定） | 地址翻译算法把这一步直接写成对 PTE 的 **store**（`src/priv/supervisor.adoc` 第 5 步，该句**无独立 norm 锚点**）＋ `norm:pmp_store_fault`。**强度差别要写清**：它是算法步骤里的一句话而非编号规范条目，且同一步要求「比较—置 A/D—store」原子完成——这正是硬件方案另要 `RsrvEventual` 与硬件页表写权限的原因 |
| 软件更新 PTE | 显式 `Write` 的 **W** | S | 同一张表，只是 `path = Explicit` |

三条配套（都是「一句话表达不出来」的地方）：

- **权限按写侧检查 ≠ 原子能力由 PMP 决定**。一次 AMO 同时含 load 与 store，但规范的故障归类把它划到 store 侧；同时 `norm:pmp_rwx_warl` 规定 R/W/X 里「R=0 且 W=1」是**保留编码**，因此一条可写的合法条目必然也可读。**AMO 的族（`AMONone`/`AMOSwap`/`AMOLogical`/`AMOArithmetic`，`norm:pma_amo_levels`）与支持的宽度、以及 `Ziccamoa`/`Ziccamoc` 这类主存义务全部由 `PmaPermits` 承担**——拿 PMP 位代替会造出「PMP 可写就以为支持该 AMO」的错误准入。
- **`privilege` 是有效特权级，合同只覆盖 `{S, U}`**（`norm:pmp_check_priv_modes`）。由此三条结论：① `MPRV=1 ∧ MPP∈{S,U}` 的 M-mode 数据访问落在本合同内，但内核第一版不在 M-mode 用 MPRV 访问普通主存，这一格记为「合同覆盖、当前无消费者」；② L 清零时 M-mode 命中条目即成功（`norm:pmp_l_bit_m_mode_enforcement`），**M-mode 能访问不构成 S/U 准入的证据**；③ 位判定按特权级分支：L 置位或特权级为 S/U 时看对应 R/W/X（`norm:pmp_rwx_check`），无条目命中时 M 成功、S/U 在「至少实现一条」时失败（`norm:pmp_no_entry_match`）。
- **PMP 与 PMA 并行且都要通过**：PMP 管有效权限，PMA 管访问类型、宽度、取指、页表读写与原子能力。缺任一项，公式就表达不出「PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝」这条已列入的负控。

**cache-block 三类操作的许可规则与普通读写不同**（`src/unpriv/cmo.adoc`）：cache-block management 在**读或写**任一许可时即允许访问对应物理地址（两者都不允许而仅取指允许时行为 UNSPECIFIED），**不是**「按读权限检查」（`norm:cbm_access`）；cache-block zero 要求 **store** 许可**且** PMA 表明支持该访问类型（`norm:cbz_access`）；cache-block prefetch 许可时按 load/store/fetch 任一判定，不许可时**不产生异常也不访问内存**（`norm:cbp_access`）。另有块级一致性约束：整个 cache block 内 PMP 位与 PMA 必须一致，且「写许可一旦给出，读许可也必须给出」（`norm:pmp_same`、`norm:pma_same`、`norm:cbo_rsv` 规定不满足时行为 UNSPECIFIED）。第一版三类均不支持，遇到即 `UnsupportedAccessOperation`；将来启用时准入必须按**整个 cache block** 判并检查块内一致性，只按单次访问 footprint 判会漏掉 UNSPECIFIED 分支。

**Smepmp 的支持边界（第一版裁定）**：`mseccfg.MML` 改变 `pmpcfg.L` 位的**解释**——置位后 `L=1` 表示 *M-mode-only*、`L=0` 表示 *S/U-mode-only*，原本保留的 `pmpcfg.RW=01` 与 `LRWX=1111` 变成 *Shared-Region* 编码；`MMWP` 把「无匹配条目的 M-mode 访问」的默认从 ignored 改成 denied；`RLB` 是临时绕过锁定的开关；三者复位为 0，除非平台规定别的复位值，且 `MML`/`MMWP` 是 sticky 位（锚点清单见 `03`）。**同一张 R/W/X 表在 `MML=1` 下整张是错的**——查表的位含义已经变了。第一版因此只接受 `smepmp_state = {MML=0, MMWP=0}`（`RLB` 只登记不参与位解释）；`MML=1`、`MMWP=1`，或 `mseccfg` 读不到/无法证明 → `SmepmpUnsupportedConfiguration`，在平台准入阶段拒绝，不做「按标准语义近似」。将来若要支持，得把 Shared-Region 的三种编码、`norm:mseccfg_mml_x_restrict`（不能新增可执行的 *M-mode-only* 或锁定的 *Shared-Region* 规则）与 `norm:pmp_rw01_rsv` 一并写进 `PmpPermits`，那是另一份合同。`mseccfg` 与 `pmpcfg*` 一样只能 M-mode 访问，所以这一格归 `PmpEvidence.provider`，不能靠探测。

### 待补测试的按主题穷举清单

**待补测试（按用途区分，不做「全局原子/全局可访问」的粗暴结论）**——完整穷举清单见 `03`，此处按主题给出必须存在的负控：

- **属性兑现**：MAIR 配置正确但叶子选错 `AttrIndx` → 失败；PAT 项正确但 MTRR 使有效类型非 WB → 失败。
- **基底与用途分层**：任一参与 hart 的有效 PMP 拒绝该区间的普通读写 → 不得进入 `PmmBaseAvailable`；仅拒绝隐式页表访问 → 基底保留、`PmmRoleRejected(PageTable)` 命中；PMA 允许读写但不允许页表访问 → 数据页可用、页表页拒绝；PMA 无 AMO 但普通读写可用 → 按用途处理；主内存被标为实现自定义排序 → 不得进入基底；硬件页表读未登记 → `PageTable` 用途不合格（即使显式写通过）。
- **起始地址与对齐**：同页内存在某个起始地址使 footprint 越过 PMP 条目边界 → 该页在此宽度下不合格，即使逐字节看全部被覆盖过；合格页尾部发起的非对齐半字/字访问越过条目边界 → 必须拒绝或走显式软件路径，不得按整页合格放行；一页只有一半落在准入域内 → 该用途不合格。
- **取指**：页尾 `page_end − 2` 处的 32 位取指——页级按保守片段集排除该起始，对象级按 `FetchFootprints` 同时判起始页与下一页；下一页不合格则失败。
- **快照与合同版本**：`FinalMap` 扩容后旧快照与投影全部失效；同一 `evidence_generation` 下 `legal_starts_contract` 从 A 切到 B → 全部投影失效；参与 hart 集合新增一个 PMP 更严的 hart → 旧 `ctx` 的全部投影失效并重建。
- **PMP 证据与保护清单**：`mseccfg.MML=1` 或 `MMWP=1` 或 `mseccfg` 不可读 → `SmepmpUnsupportedConfiguration`，不得套用 R/W/X 表；`Absent` 且平台拿不出「无其它保护结构」支持限制 → fail-closed；`Implemented` 但 profile 既未证明「无未建模结构」也未列出其它结构 → `PlatformProtectionPermits` 不得默认为真；provider 只承诺「变更后通知」→ 不接受；`RLB=1` 或配置变更未递增 `evidence_generation` → `stability_until` 不得用于跨阶段缓存。
- **顶端边界**：`a = 0xffff_ffff_ffff_fff8` 且宽度 8 必须合格（`end == 2^64`）；`a = 0xffff_ffff_ffff_fff9` 必须越界。
- **原子与几何**：LR/SC 算法依赖最终成功而区域仅 `RsrvNonEventual` → 该用途不合格（同页 `GeneralData` 仍合格），第一版遇 `RsrvNonEventual` 一律拒绝（fallback 分支未开放）；支持 LR/SC 且 `RsrvEventual` 但预约集几何不满足声明的 profile → 原子用途不合格；`HardwareManagedAd` 下缺 `ImplicitHardwarePteUpdate` 的任一项（隐式 W、硬件页表写、原子更新、`RsrvEventual`）→ 页表用途不合格；`CacheBlockManagement`/`CacheBlockZero` 在第一版一律 `UnsupportedAccessOperation`。
- **Profile**：声明 RVA23S64 而 `Svade` 未实现 → 不合格，即使通用公式会落到硬件更新。
- **空集前置**：`ctx.ParticipatingCpuSet = ∅`、`BaseAccessRequirements` 为空、`U = ∅`、任一 `req` 的 `LegalStarts` 为空、`RoleRequirementsComplete` 为假、或 `Executable` 的 `instruction_lengths` 未登记 → `InsufficientEvidence`，不得真空通过；但 `RoleSpecificRequirements(GeneralData) = ∅` 必须**不**触发失败（空集是定义），而把基底要求写成用途特有要求的写法必须失败。
- **对象访问与映射**：对象物理准入通过但来源政策未授权（MRB / `no-map` / `EfiReservedMemoryType`）→ `ObjectPolicyAccessEligible` 失败；连续 VA 映射到两段不连续 PA 而 `expected_translation` 只登记一段 → 失败；VA 起点正确但区间中段落到别名或属性不兼容映射 → `ObjectMappedAccessEligible` 失败；叶子落在正向授权内但与某条 `DenyMapping` 相交 → `MappingAudit` 失败；`ranges` 缺失与非空分别落到各自错误。


## 六十三、第九十二轮：自正文移入的 §2.7 定位表与 §6 复核命令

> 移入理由与正文一致：外部评审人看不到仓库，因此「带路径的函数定位表」与「本机 cargo 命令」对他既不可执行也不可复现，属取证而非待审内容。正文 §2.7 保留的是唯一一条只看文字就会做错的重构约束（磁盘镜像 DTB 不进入内存政策），§6 保留的是 146/70 三个读数必须一致的对账要求。

<!-- 自正文 §2.7 函数名定位段落与 FDT 五调用点表（正文只留重构约束那条事实） 移入 -->

### 2.7 关键代码位置（供复核 §2 的主张）

复核 §2 只需这几个位置：两张表由同一次采集产出（`os/libs/minix-boot/src/memmap_source.rs:fn collect`，`/reserved-memory` 的查找与可见性检查在同文件 `fn lookup_reserved_memory`）；UEFI 六步裁决在 `os/boot-shim/src/ebs_map.rs:fn adjudicate`，计数与提交在同文件 `fn classify` / `fn commit`；恒等映射循环与引导根构造入口在 `os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable`；Step 4 直接映射（追加）在 `os/kernel/src/dm_coverage.rs:fn establish_boot_dm`；配置表 DTB 的取得在 `os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources`。容器校验是 `os/libs/minix-boot/src/fdt_layout.rs:fn validate`。**行号会随代码漂移，一律以函数名为准**；带行号的定位表在 `03`。
**FDT 容器校验的五个调用点**（`fdt_layout::validate` 在解析库之前执行）：

| 调用点 | 输入来源 | 参与内存政策 | 校验结果用于解析 |
|---|---|---|---|
| `bootface()`（`kernel-image/src/bootface.rs`） | riscv 启动 ABI 递入的 DTB | 是 | 是 |
| `bootface_a64()`（`kernel-image/src/bootface_a64.rs`） | aarch64 直启 DTB | 是 | 是 |
| `RiscvUbootBootShim::prepare_boot`（`boot-shim/src/opensbi_helpers.rs`） | OpenSBI/U-Boot a1 | 是 | 是 |
| `UefiBootShim::prepare_boot`（`uefi_helpers.rs` 的磁盘 DTB 分支） | ESP 上的 `dumpdtb.dtb` | **否**（只作覆盖对照诊断） | 是（只用于诊断读取） |
| `load_dumpdtb_free`（`uefi_helpers.rs`） | 同上 | **否** | 是（只用于诊断读取） |

最后两行是磁盘镜像 DTB：它**不进入内存政策**，校验只保护诊断读取路径；重构时不得把它接进政策链。

复核方式：`os/libs/minix-boot/src/fdt_layout.rs` 的 `validate`（容器）与本节的函数和映射构造位置即可核对 §2 的每一条主张；`validate` 的五处实际调用点见本节表格。不需要整仓。

<!-- 自正文 §6 复核命令代码块与 git status 期望输出 移入 -->

**复核命令**（146 与 70 是该 HEAD、该工作树状态下的读数，§2.1、§2.2、本节三处数字必须一致；基线之后如有新提交必须重跑）：

```
git rev-parse HEAD
git status --short
cd os && cargo test -p minix-boot --lib
cd os && cargo test -p boot-shim --lib
```

`git status --short` 的期望输出只有用户自有文件 `AI-chats/daily.todo.md` 一行；出现其它改动即说明测试不是在干净基线上跑的。


## 六十四、第九十三轮：被分支式与完整性判据替换下来的原判定式

> 本节只保存「曾经写在正文里的公式与段落」，逐字未改。替换原因分别是：`GeneralData` 的空需求集合
> 被判定核的第一条拒绝（分支修正）；`InstanceSetComplete` 定义了却没有消费者（接通）；原子用途的
> 共享结构带着 RISC-V 预约集字段（移入 adapter）；PLIC 授权数值由叶子粒度反推（改为整段 aperture）。

### 第九十三轮代码坐标底账（正文只留符号锚点，行号在此）

> 读数由只读复核给出，绑定合并前的 `rewrite` 分支；行号会随代码漂移，核对时以上表的符号名为准。

| 事实 | 文件 | 行号 |
|---|---|---|
| 恒等循环起点（`while addr < IDENTITY_MAP_END`） | `os/kernel/src/lib.rs` | 934 |
| 恒等循环内的 `map_huge`（返回值被 `let _ =` 丢弃） | 同上 | 952 |
| 内核高半映射（`.expect`） | 同上 | 979 |
| Step 2b 设备窗口高半别名 | 同上 | 1022 |
| Step 2c 设备窗口段 | 同上 | 1064 |
| 根激活 `paging.enable()` | 同上 | 1080 |
| `set_current_root_phys(root_page)` | 同上 | 1092 |
| `establish_boot_dm` 四个调用点 | 同上 | 825、354、615、648 |
| Step 4 失败即 `panic` | 同上 | 826 |
| `fn harden_kernel_text` 定义 | 同上 | 1272 |
| 其唯一调用点（`bkl_lock` 之后、`smp_init` 之前） | 同上 | 1697（另 1691、1705） |
| `from_active_root` / `split_huge` / `update_flags` | 同上 | 1313 / 1322 / 1330 |
| heap 窗口驱逐段（含 `unmap`） | 同上 | 2641–2668（`unmap` 在 2664） |
| ELF 加载窗口 `split_huge`→`grant_user_walk`→`unmap` | `os/arch/src/arch/boot.rs` | 543 / 551 / 554 |
| x86-64 整槽替换既存叶子 | `os/arch/src/x86_64/paging.rs` | 1361 |
| x86-64 把既存 1 GiB 恒等叶子降级成空表 | 同上 | 1380–1388 |
| x86-64 在既存 PML4[0] 置 USER 位 | 同上 | 1329 |
| x86-64 `update_flags` 保留地址位、只换标志 | 同上 | 868 |
| riscv64 `map_huge` 遇已存在槽失败 | `os/arch/src/riscv64/paging.rs` | 1247、1255 |
| arm64 `map_huge`「没有降级路径」 | `os/arch/src/arm64/paging.rs` | 1215–1219 |
| trait `unmap` / `unmap_range` / `split_huge` 默认实现 | `os/arch/src/arch/paging.rs` | 377 / 512 / 400 |
| `fn validate_bootstrap_tree` 与其调用点 | `os/kernel/src/dm_coverage.rs` | 447 / 220 |
| `VM_HEAP_BASE` 窗口（x86 `0xC000_0000`） | `os/arch/src/arch/direct_map.rs` | 258 |


<!-- 自正文 对象访问资格式（未接 InstanceSetComplete 的版本） 移入 -->

ObjectAccessEligibility(object, ctx) =
    ObjectAccessInventoryState(object) ∈ { Complete(uses ≠ ∅, audit_scope), NoAccess(proof) }
                                                                  ← 空清单不得写成 Complete：那必须走 NoAccess + 机器可检查证明
    ∧ ∀ use ∈ ObjectAccessUses(object) :
          ActualAccesses(use) ≠ ∅                                  ← 每条 use 至少对应一个真实访问实例
          ∧ ObjectMappedAccessEligible(use, ctx)
ObjectAccessInventoryState(object) = Complete(uses, audit_scope) | NoAccess(machine_checkable_proof) | Incomplete
    ← NoAccess(proof) ⇒ ObjectAccessUses(object) = ∅，且 proof 必须覆盖适用阶段、适用树、全部访问路径与对象
      整个存活期间的访问面
    ← Complete 只表示「在给定 audit_scope 内，已登记访问点覆盖完整」：**不**表示裸指针、内联汇编、
      硬件隐式访问或未纳入工具链审计的访问也被证明完整（见 O-07）；audit_scope 还要证明该范围内的
      实际实例都被这些 use 覆盖。不得让 Complete 这个名字被当成全系统完整性证据

<!-- 自正文 ObjectPhysicalAccessEligible 首两行（非空判据表述不完整） 移入 -->

ObjectPhysicalAccessEligible(use, ctx) =
    ActualAccesses(use) ≠ ∅                                       ← 空实例集合会让全称量词真空通过

<!-- 自正文 InstanceSetComplete 定义（当时的定义没有消费者，也没有非空前置） 移入 -->

InstanceSetComplete(use) =
    Resolve(use.access_instance_set_id).source_evidence 覆盖 use 声明的 audit_scope
    ∧ ∀ i ∈ ActualAccesses(use) : InstanceMatchesRequirement(i, use.access_requirement)
    ← Expand 的合法职责只有两个：验证实例符合需求模板，或把模板展开成硬件访问类别；
      它没有 virtual_address、指令身份与 LR/SC 预约集证据，因此不能充当实例来源

<!-- 自正文 原子页级共享结构含 reservation_set_requirement 的旧形状 移入 -->

AtomicPageRoleRequirement {          ← 页级能力：候选页能否承担这类原子状态（**不含**具体物理地址）
    operation_family, access_width, progress_requirement, reservation_set_requirement, fallback_plan_id,
}
（对象级原子访问不再另立实例类型：它就是带 `reservation_set_evidence` 的 `ActualAccessInstance`，见对象访问一节）

ReservationSetRequirement { base_page_containment, profile_geometry }
    ← base_page_containment：页式 VM ∧ A 扩展 ⇒ 预约集整段落在单个基础 4 KiB 页内（`norm:lr_sc_rsrv_set_page`，第一版即成立）
    ← profile_geometry：声明 profile 时启用（Za64rs：连续、自然对齐、≤64 B）；未声明记「不适用」
ReservationSetGeometryCore(scope, cpus, isa_profile_id)     ← 页级与对象级共用的同一判定核心
    = AMO-only ⇒ NotApplicable；LR/SC ⇒ 基础页内约束 ∧ (声明 profile 时的连续 / 自然对齐 / ≤64 B 几何)
    ← 页级入参 =（候选页, 算法要求的预约集, 全部参与 CPU, profile）；对象级入参 =（实例的 reservation_set_evidence
      与实际物理范围, 执行该访问的 CPU, profile）——对象级必须真正消费它，页级通过不能替代对象级证明

<!-- 自正文 AtomicSharedStateEligible 直接调用 ReservationSetGeometryCore 的旧式 移入 -->

AtomicSharedStateEligible(page, ctx) =              ← 页级：只做「登记完整性 + 算法要求的进展等级 + LR/SC 几何」
    AtomicRequirementsComplete(ctx) ∧ RegisteredAtomicAlgorithms(ctx) ≠ ∅
    ∧ ∀ algorithm ∈ RegisteredAtomicAlgorithms(ctx) :
        ProgressOf(algorithm) = 若用 LR/SC 则 ReservationEventual，否则取该算法登记项 progress_requirement 显式给出的
                                OperationCompletion 或 NotRequired   ← 登记是合同输入，不允许实现者自行推断该选哪个
          ← 分型而非一个共用的 Eventual：AMO-only 不因它要求 RsrvEventual；其他架构的额外进展要求由 adapter 映射
        ∧ ( algorithm.uses_lr_sc ⇒ ∀ a ∈ LegalStarts(page, lrsc_req, AtomicSharedState, ctx) :
              ReservationSetGeometryCore(ReservationSetFor(a, algorithm), ctx.ParticipatingCpuSet, ctx.isa_profile_id) )
            ← 页级要量到该页所有合法 LR/SC 起始地址；对象级用实例自带的预约集证据——两层共享核心也共享量词形状
    ← 操作族、宽度与逐起始的硬件准入**只在** RoleSpecificRequirements + PhysicalAccessPermits 那条路上查一次；
      这里不再重复 AccessRequirementsSatisfied，否则两边会各自演化出不同的「进展是否满足」
    ← 对象级实际访问复用同一个几何核心（入参换成实例的预约集证据与实际物理范围）：两层共享判定，不共享名字

<!-- 自正文 Za64rs 段落（移入时的写法要求把几何补进共享必填字段） 移入 -->

**`Za64rs` 的预约集几何要有着落**：`Za64rs` 要求「Reservation sets are contiguous, naturally aligned, and a maximum of 64 bytes」（`rva23-profile.adoc` 97–98 行）。`AtomicAlgorithmRequirement` 现在只有操作族、宽度与进展要求，需补 `ReservationSetRequirement { max_bytes, contiguous, naturally_aligned }`；只写 `RsrvEventual` 不足以表达 profile 的 LR/SC 义务。第一版不声明 profile，该字段记「不适用」，但结构不能缺。

<!-- 自正文 GenericRolePageEligible 无条件调用判定核（P0 冲突式） 移入 -->

GenericRolePageEligible(page, role, ctx) =
    RoleRequirementsComplete(role, ctx)          ← 需求生成器没有因缺 profile / 缺代码扫描而「漏出」空集
    ∧ ( role = GeneralData ⇒ RoleSpecificRequirements(role, ctx) = ∅ )      ← 只有这一格按定义为空
    ∧ ( role ≠ GeneralData ⇒ RoleRequirementsApplicable(role, ctx) )        ← 适用性由 adapter 证明
    ∧ ( role = AtomicSharedState ⇒ AtomicSharedStateEligible(page, ctx) )   ← 原子用途的唯一展开入口
    ∧ AccessRequirementsEligible(page, role, RoleSpecificRequirements(role, ctx), ctx)

<!-- 自正文 名字一一对应段（未区分两种空集合含义） 移入 -->

**名字与定义必须一一对应，空集不得真空通过**：`RoleAdmit(role, page, ctx)` 是声明式接口，其操作化定义**只有** `RolePageEligible(page, role, ctx)` 一个展开式（分支形式），而三个消费者共用同一份 `AccessRequirementsEligible` 判定核——`CommonCpuAccessEligiblePages` 只消费 `BaseAccessRequirements(ctx)`，`GenericRolePageEligible` 只消费 `RoleSpecificRequirements(role, ctx)`，页表分支只消费 `PageTableAccessRequirements(ctx)`。判定核的第一条就是 `requirements ≠ ∅`：`GeneralData` 的用途特有集合**按定义为空**（基底已覆盖，这是唯一定义为空的一格），`PageTable` 因隐式遍历读与显式 PTE 写始终必需而非空，`Executable` 必须有完整取指模式，`AtomicSharedState` 必须有完整算法清单，其它用途出现空集一律 `InsufficientEvidence`。基底侧还另有三条前置：`ctx.ParticipatingCpuSet ≠ ∅`、`GeneralDataCompleteness(ctx) = Complete`、每条基底 `req` 的 `LegalStarts ≠ ∅`——宽度清单必须由**固定编译目标 + 编译器代码生成约束 + 已审计指令集合**三者合成，证不出完整就是 `InsufficientEvidence`，不是「已通过」；`Executable` 的 `instruction_lengths` 为空集同理，那等于宣称该页没有取指需求。把 `RoleAdmit` 存成一个布尔字段会丢掉 `req`、起始地址、CPU 三个量词维度。

<!-- 自正文 §2.3 的 Step 4 与「全仓没有撤销路径」两条旧事实表述 移入 -->

- Step 4 的直接映射覆盖是**在同一棵根上追加 PTE**，候选来源是 `可用表 ∪ 自举页表树 ∪ 启动模块 ∪ 保留表` 的并集过滤。
- **没有独立最终根、没有审计遍历、没有禁止集合、没有页表冻结**；全仓也没有任何撤销恒等段的路径（`os/kernel/src/lib.rs` 与 `os/kernel/src/dm_coverage.rs` 里的 `unmap` 只服务于 heap 窗口驱逐）。宿主表征测试 `identity_piece_policy_covers_low_4gib_and_marks_device_windows` 断言的正是「覆盖到 `IDENTITY_MAP_END`、含 `EXECUTABLE`、不含 `USER_ACCESSIBLE`」。

<!-- 自正文 PLIC 三分法段落（当时还留「须由 profile 独立证明」的未决口） 移入 -->

**但本项的「平台授权范围」数值目前不是独立证明的结果，而是被建表粒度反推出来的**——代码注释逐字写着：Step 1 按 2 MiB 出设备叶子，所以授权映射范围取资源范围的 2 MiB 包络，`PLIC_WINDOW_LEN = (PLIC_RESOURCE_LEN + 0x1F_FFFF) & !0x1F_FFFF`（并有 `is_multiple_of(0x20_0000)` 的编译期断言）。这正是「叶子大小倒推进授权政策」的实例，因此按三分法重记：**平台授权范围**须由 profile 独立证明（QEMU virt 的 DTB `reg` 给出的是 aperture `0x0C00_0000 + 0x60_0000`，不是 4 MiB）；**实际映射覆盖**由分页侧建表后观察；**实际访问需求**由设备使用点产生。三者数值可以相同，**不得互相推导**。同一问题也适用于 x86 LAPIC 的 2 MiB 授权（实际需求 4 KiB）与 aarch64 的 32 MiB 窗口——在拿到独立平台证据之前，它们都是**待证平台假设**，且授权范围必须覆盖内核真正使用的 context，而不是因为静态窗口表恰好够用就认定需求完整。本项目的 `0x24_0000` 只在 QEMU `virt` + 固定 machine version + TCG + 单 socket + AIA 关闭 + 明确 CPU 型号 + `-smp ≤ 32` 且全部支持 S-mode 下成立；其它配置必须拒绝或重新推导，且 profile 要钉住 `qemu-system-riscv64 --version` 读数与 machine version（今天尚未记录）。

<!-- 自正文 no-map 三项拆分段落（驱动例外第一版状态未写死） 移入 -->

**`no-map` 要拆成三项，不能简化成一个无条件 `DenyMapping`**：`DenyStandardSystemMapping`（禁止进入标准系统内存映射）、`DenyUncontrolledSpeculativeAccess`（禁止非受控推测访问）、可选的 `DriverControlledUse`（专属设备身份 + 专属访问路径的受控使用）。「有驱动申请」不是通用豁免，不能让 `no-map` 页进入内核直接映射。

<!-- 自正文 PhysicalRange 段落（顶端页支持边界未写成政策） 移入 -->

**统一物理范围类型 `PhysicalRange { start: u64, end_exclusive: u128 }`**（`start < 2^64`、`start < end_exclusive ≤ 2^64`）：DTB 的 `address + size` 与 UEFI 描述符都能表达 `[0xffff_ffff_ffff_f000, 2^64)`，统一来源记录不得因 Rust 字段类型丢掉这个合法边界。原始输入字段保留各自编码（DTB 的 address+size、UEFI 的 start+pages），规范化层才转成 `PhysicalRange`；`UnsupportedPhysicalAddressEnd` 只在项目明确选择拒绝该边界时使用，**不能成为 `u64` 加法的副作用**。这条与 O-12h 结构校验、O-12i `FullPages` 是同一条边界合同；DTB 腿已按此实现（分名规则见 §2.1 与 O-12i），UEFI 侧的 `SourceRecord` 尚未接入。


## 六十五、第九十五轮：按射程重划移出正文的读数表与记录

> 评审人撤销了「把叶子粒度、A/D 更新、PMP/PMA 页表访问证明、原子能力、冻结字段列为内存图交棒阻断项」的判断。下列内容逐字保留，作为 BootEntry 与分页侧专项的输入；它们不再构成 I-29 的验收条件，也未被否定。

<!-- 自正文 O-05 候选 A 的三行时点表（T1 写成激活前，与第九十三轮裁定冲突） 移入 -->

| 时点 | 必须完成 |
|---|---|
| T1 激活引导根前 | 引导根的覆盖、权限、语义、别名与禁止集合；含设备映射验收（R13-D） |
| T2 激活最终根前 | 最终根的同类审计 + 三类义务合并目标（R21） |
| 激活后 | 对**映射定义字段**持续验证 |

<!-- 自正文 第九十三轮的激活前后修改路径读数表（按射程重划移交分页侧专项） 移入 -->

**冻结时点与今天的活动根修改路径撞车（下表是代码读数，不是推测）**：

| 时点／路径 | 代码事实 | 改了什么 | 政策归属 |
|---|---|---|---|
| 根激活**之前** | `fn build_bootstrap_root_and_enable` 内按顺序执行：恒等循环（失败被 `let _ =` 吞）、内核高半映射（`.expect`）、设备窗口高半别名、设备窗口段；最后 `paging.enable()` 与 `set_current_root_phys` | 建立叶子 | **构造期**，是 T1 审计的正确对象 |
| Step 4 直接映射 | `establish_boot_dm` 在激活后运行；x86-64 还整槽替换并降级既存恒等叶子、置 USER 位 | 新增叶子；x86-64 另改页大小与权限 | **激活后修改**：不能说 T1 在它之前完成 |
| `fn harden_kernel_text` | 唯一调用点在 `fn kmain` 内、`bkl_lock` 之后、`smp_init` 之前；经 `from_active_root` 包住活动根后 `split_huge`，再 `update_flags` 置 RX | 页大小 1 GiB→4 KiB ∧ 权限（PA 由 `update_flags` 保留） | **激活后修改**：动的正是上表第一行的两个冻结字段 |
| heap 窗口驱逐 | `os/kernel/src/lib.rs` 的驱逐段，对 `[VM_HEAP_BASE, +VM_HEAP_SIZE)` 先 `split_huge` 再 `unmap`（`expect`，失败即 panic） | 撤销叶子 | 具名临时映射：按范围＋意图＋生命周期排除，不得笼统排除整棵活动根 |
| ELF 加载窗口 | `os/arch/src/arch/boot.rs:fn load_elf_into`：`split_huge` → `grant_user_walk` → `unmap` | 粒度、权限、撤销 | 同上 |

**所以 T1 不能写成「引导根激活之前」**：今天唯一像审计的检查是 `os/kernel/src/dm_coverage.rs:fn validate_bootstrap_tree`，它的调用点仍在激活之后，而激活之后还有上表四类修改。合同只有两种自洽写法——**要么**把 T1 定义为「引导根上全部结构修改完成之后」（于是 `enable()` 不是 T1，冻结记录必须带修改序列的终点标识），**要么**保持 T1 在激活前，把那四类路径全部划入「受控更新 + 先审后用」并逐类登记归属。第一版选前者，且**权限类（`update_flags`）与结构类（`split_huge`／`unmap`）分别归类重审**：「只拆小、不增权限」不构成豁免，因为叶子粒度本身就是冻结字段。具备拆分能力不等于某个过渡映射已安全撤销；活动根上存在修改路径也不等于这些修改自动符合冻结合同。

<!-- 自正文 RoleAdmissionDenial 记录（PMM 用途层归因，按射程重划移交 PMM／分页侧） 移入 -->

RoleAdmissionDenial {                 ← 用途层的归因记录，与基底排除分开（第九十三轮采纳，非第一层阻塞项）
    page_range, role, failed_requirement, cpu_scope, legal_start, footprint,
    evidence_id, admission_context_id,
}
    ← 「这一页不能作为 PageTable / Executable / AtomicSharedState」是用途层结论，**不得**记成「该页已从
      `PmmBaseAvailable` 排除」：两者字段不同、消费者不同。`legal_start` + `footprint` 指出是哪一个起始
      地址上的哪一类访问失败，将来解释 PMM 为什么拒绝某个用途才有据可查；缺这份记录时只剩一个布尔
ArchitectureAdmissionRejected {
    cache_policy_evidence,
    memory_ordering_evidence,
    hart_access_evidence,
    profile_memory_obligation_evidence,   ← ProfileMemoryEligiblePages 拒绝时必须有这一项
    state: DefinitelyIneligible | InsufficientEvidence,
}

<!-- 自正文 对象级判定直接调用 ReservationSetGeometryCore 的旧式（共享层再次认识预约集） 移入 -->

        ∧ ( i 是 LR/SC ⇒ ReservationSetGeometryCore(i.reservation_set_evidence,
                                                    Union(TranslatedAccessFragments(i)), subject_cpus, ctx.isa_profile_id) )

<!-- 自正文 AtomicSharedStateEligible 里用 uses_lr_sc 作前置条件的旧式（AMO-only 会绕过共享原子谓词） 移入 -->

        ∧ ( algorithm.uses_lr_sc ⇒ ∀ a ∈ LegalStarts(page, lrsc_req, AtomicSharedState, ctx) :
              AtomicCapabilityPermits(ctx.arch, algorithm, PageScope(page, a), ctx.ParticipatingCpuSet, ctx.isa_profile_id) )
            ← 共享层只问「该架构在这些 CPU 与该范围上是否满足该算法的原子能力与进展要求」；预约集几何
              是 RISC-V 对该问题的回答之一，量词形状三架构共享、机制名不共享

<!-- 自正文 共享层展开为 Read/Write/Fetch 的旧注释（枚举名与共享合取不一致） 移入 -->

        ← 共享层只用 OS 语义名；RISC-V adapter 展开为 Read/Write/Fetch/AMO(family)/LR/SC，

<!-- 自正文 冻结字段与例外表、FrozenBootMappingSet 段、候选 B、先审后用、政策审计与硬件同步 移入 -->

冻结的字段与例外：

| 字段 | 激活后 |
|---|---|
| VA 覆盖、PA 对应、页大小、有效权限、内存属性、共享性 | 不允许变化 |
| 架构 adapter 列出的受控元数据字段（`MutableMappingMetadata { field, update_agent, permitted_transition, synchronization_contract }`） | 按 `update_agent` 允许特定变化；RISC-V 的 A/D 随 `PteUpdateScheme` 取 `Hardware` 或 `FaultHandler`，AArch64/x86 由各自 adapter 定义；审计必须说明如何屏蔽或验证 |

**（激活前后修改路径的代码读数表已随射程重划移入 `03` 六十五节：它决定的是冻结合同的归属，不是交棒信息的完整性。）**

**冻结对象是 `FrozenBootMappingSet`，不是整棵页表**：T1/T2 审计过的是启动政策映射集合，该集合内的映射定义不得被未授权修改；其它页表区域（未来的 VM、动态映射、进程地址空间）经受控更新接口变化——准备修改 → 校验政策 → 发布 → 架构同步——这不违反冻结。**不接受的表述**：「启动后页表永久不可改」。
**候选 B**：不冻结，每次修改后重跑对应审计——可作为后续动态页表阶段的机制，不能替代切根前审计。
**无论 A/B，后续页表修改必须「先审后用」**：在未激活的副本上修改并审计后再发布，或经受控更新接口在映射对硬件可见之前完成验证；A/D 位的屏蔽必须逐字段定义，不能笼统忽略一组 PTE 位。
**政策审计与硬件同步是两份独立合同**：审计通过不等于新映射已在所有执行单元生效。架构层必须负责 PTE 写入的发布顺序、相应地址空间的 TLB 失效、以及多核之间的同步失效（shootdown）；共享层只表达「新映射必须在所有相关执行单元生效」这个语义，不出现 `INVLPG`/`SFENCE.VMA`/`TLBI` 之类机制。



## 六十六、第九十六轮：被 §2 现状读数与 §7 收紧替换下来的原文

> 被替换的 §7 把硬件准入类条款仍列为本项阻塞项，与 §1.6 的射程声明互相矛盾；O-13 那句「都未定」把已存在的
> v1 基线一并否认了。下列段落逐字保留。

<!-- 自正文 O-06 表格原来的「普通主存性质」列名 移入 -->

| 类型 | 属 `RamSourceDomain` | 普通主存性质 | 当前可映射 | 当前可分配 | 释放事件 | 来源叠加 |

<!-- 自正文 O-13 原来那句「字段编码、偏移、容量、版本与失败形状都未定」 移入 -->

字段编码、偏移、容量、版本与失败形状都未定；现在只有两张区间表的布局。

<!-- 自正文 第九十六轮重写前的整个 §7 移入 -->

## 7. 签字状态

| 层 | 结论 | 依据 |
|---|---|---|
| 第一层：语义与方向 | **可签** | 五条不变量；§4 的准入合同方向明确；§1.6 的三合同划界评审人已认可 |
| 第二层：集合与逐路径判据 | **按收窄后的射程不签** | 交棒侧未闭合项见下；公式级冲突已修（`GeneralData` 空集合分支、`InstanceSetComplete` 接入、`Complete` 类型级非空） |
| 第三层：固定布局 ABI | **不签** | 基线 v1 已存在，新增来源记录与对象信息的扩展未定（O-13a／O-13b） |

**先说清这一层现在管什么**：第二层的验收对象是「完整、准确、无歧义地把物理资源与平台资源信息交给 OS」。**不计入本项验收**的四类条件（内容仍留在正文作上下文，归属 BootEntry 与分页侧专项）：叶子粒度与拆分／降级路径、PTE 的 A/D 更新方案、页表访问的 PMP/PMA 证明、LR/SC／AMO／预约集等原子能力，以及 `FrozenBootMappingSet` 的页大小冻结与临时映射的结构影响范围。评审人本轮已自行撤销把这些列为交棒阻断项的判断，本项按撤销处理，不再逐项反驳。

**未闭合项**——共同形状是「文本已定、证据或实现为零」，外加一条**现状与合同相反**的项：

- **现状冲突（不是缺口，裁决已下）**：低 4 GiB 恒等映射不消费 `no-map`、MRB 与类型禁止政策，普通叶子带 `EXECUTABLE`，且没有撤销路径（§2.3 已把措辞收紧为「无条件尝试建立且失败被吞」，并声明没有实际 PTE 遍历证据）。**x86-64 的 Step 4 还会整槽替换并降级既存恒等叶子、在既存 PML4[0] 上置 USER 位**（§2.3 已按代码读数改写），所以「恒等段一次建好就不再变」在三架构上都说不通。O-02a 裁定甲案、否决乙案，也否决「必需叶子走授权、普通 RAM 继续走旧宽映射」的半拉子形态。**本项状态**：四段依赖链的前三段（来源进入政策、引导根负向投影、必需对象正向授权）是 I-29 的交付，均**尚无代码**——最硬的缺口是除 riscv bootface 外三条入口仍丢弃来源记录、UEFI 侧 `SourceRecord` 未产出。**第四段（建表侧消费与验收）属分页专项**，其完成度不计入本项交付，否则会误判 I-29 未交付。

- **准入上下文闭合（P0 阻塞）**：`ParticipatingCpuSet = Resolve(id, inventory_snapshot)` 的身份解析、`legal_starts_contract` 进 `AdmissionContextId`、`ProfileContextState` 四态、`SystemPteUpdateSchemeUniform` 在非 RISC-V 的 `NotApplicable` 分支——全部无实现。
- **取指与对象访问资格（P0 阻塞）**：页级保守片段集与对象级多片段证明均无实现；`InstanceSetComplete` 已接进 `ObjectAccessEligibility`（此前它定义了却没有消费者，而判定式只要求实例集合非空——非空不等于完整），`ObjectAccessInventoryState` 的 `Complete(uses, scope)` 也要与 `ObjectAccessUses(object)` 对同一份清单；实例展开、`reservation_set_evidence` 采集路径、`NoAccess` 的机器可检查证明形式未定；DMA/IOMMU、设备、固件等访问主体尚无合同（第一版一律 `UnsupportedAccessActor`）；x86 变长指令跨页仅按架构事实使用，本方未取 Intel SDM 原文。
- **整页合格判定**：`GeneralData` 的用途特有集合按定义为空，已改走 `GenericRolePageEligible` 的空分支（判定核第一条 `requirements ≠ ∅` 保留，专门用来挡其它用途的漏登记）；全称判定已收成唯一的 `AccessRequirementsEligible` 判定核（基底、通用用途、页表用途三处共用），起始地址量词的改写完成，但**无实现、无负控**。三个完整性门槛分属三条路径，不得混用也不得互相替代：`RoleRequirementsComplete` / `RoleRequirementsApplicable` 只管通用角色；`PageTableRequirementsComplete` 只管页表分支（因此页表**不**经过通用生成器检查）；`GeneralDataCompleteness` 管基底需求清单。`Fetch` 页尾排除无实现。
- **原子用途**：共享结构已删去 `reservation_set_requirement`，`ReservationSetRequirement`／`ReservationSetGeometryCore`／`reservation_set_evidence` 归 RISC-V adapter，共享层只经 `AtomicCapabilityPermits` 提问；`ExpandAtomic(RegisteredAtomicAlgorithms)` 的完整对应仍靠实现证明；`RsrvEventual` 与预约集几何的证据未落；`ReservationSetFor(a, algorithm)` 的 adapter 语义未定；`AtomicFallbackPlan` 与几何核心为实现预留。
- **页表用途**：`RolePageEligible(page, PageTable, ctx) = PageTableBackingEligible(page, ctx)` 的等价关系目前是人工核对；「页表只用普通幂等主存」是项目政策；三个审计共用的统一页表叶子观测记录字段未定义。
- **PMP 与平台保护**：无 provider、无变更前协调协议、无 `evidence_generation` 递增路径；`Absent` 的平台支持限制、`mseccfg` 证据来源、`PhysicalProtectionInventory.completeness` 均未落。
- **hart 清单与 A/D 方案**：`SystemHartSet` 的语义**已选定**为「平台上所有物理存在的 hart」，`fail` 条目按三分表处理（证明物理不存在才排除；证明物理存在即纳入；只能证明不可运行 → `InsufficientEvidence` 拒绝启动）。**未闭合的是平台事实而非模型**：各平台的完整 hart 清单与其物理存在性证据（`HartInventoryEvidence.completeness_claim`）与逐 hart 的 A/D 方案证明。
- **UEFI 输入与快照**：`PreflightMap` / `FinalMap` 合同无实现与负控；固件镜像身份与属性位编码 profile 未钉；`RESERVED`/`UNUSABLE` 的原始描述符记录未产出；`EfiReserved`、§4.6.1.3 与 AArch64 分组规则均为评审人核对，本方未独立取到原文。
- **设备窗口**：模型可签、数值未签——APIC 核对改为逐逻辑处理器，QEMU 构建版本与 machine version 未记录；今天没有页表遍历，覆盖列只能是预期值。PLIC 已按本轮裁决改为**整段 6 MiB aperture 授权、4 MiB 只作预期覆盖**，否决由 2 MiB 叶子反推授权；授权数值本身仍待 profile 独立证明，证明不成立即拒绝该 profile。
- **冻结时序（移出本项验收，改列 BootEntry／分页侧专项待办）**：今天活动根上至少有四类修改发生在激活之后（Step 4 追加、`fn harden_kernel_text`、heap 窗口驱逐、ELF 加载窗口），唯一像审计的检查也落在激活之后；检查点已拆成 T0／T1／T2，职责与归属见 O-05。留给该专项的三个未答问题：T0 在根未激活时如何遍历实际 PTE（拿不到就只能审计构造计划，证明强度必须如实降低）、临时映射的结构影响范围是否触及冻结字段、`split_huge` 拆分出的中间表页由谁记账。
- **交棒完整性（本项主责，部分已实现）**：内核 → VM 这一跳的五类扣减范围已有实现与测试（§2.4）；**未闭合在另外三处**——其一，UEFI 腿的 `SourceRecord` 仍未产出、除 riscv bootface 外三条入口丢弃来源记录（O-12g，本项最硬缺口）；其二，交接给内核的 memmap 文档逐字写着 **not filtered**，内核映像与启动模块占用在**第一跳**并不反映，只由第二跳的扣减记录补偿，两跳之间谁是权威需明确裁决；其三，O-13 固定布局 ABI 未开始，扣减记录容量 `VM_BOOT_HANDOFF_MAX_DEDUCTED` 的核算与溢出形状尚未按三架构分别验证。

**两个阻塞组（已按射程剔除归属专项的条目）**：输入真实性、来源保真与生命周期（O-11、O-12h、O-12k、O-12j、DTB 节点准入、O-01、O-06、O-12f、O-12g）；投影、禁止集合与交棒清单（O-12d、O-12a/b、O-02a/b、O-03、O-04、O-07、O-12e、O-13）。**移给专项**：O-12i 的硬件准入与原子能力部分、O-12c 的映射覆盖观测、O-05 的冻结字段与时序。

「禁止集合编译 + 引导根审计」有代码也只能作为第一轮实现复审的起点，不是第二层签字的条件——输入边界、来源完整性、PMM 资格与设备映射覆盖都是它的前置证明。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。


## 六十七、第九十七轮：三条分配路径与交接 ABI 的代码读数、被替换的 §2.8

> 读数由只读复核给出，绑定合并前的 `rewrite` 分支。行号会随代码漂移，核对以符号名为准。

### `FREE_MEMMAP` 的访问点穷举（判「有没有读者」用）

| 位置 | 形式 | 读还是写 |
|---|---|---|
| `os/kernel/src/globals.rs:286` | `static FREE_MEMMAP: SyncUnsafeCell<[MemMapEntry; MAXMEMMAP]>` | 声明 || `os/kernel/src/lib.rs:167` | `use globals::FREE_MEMMAP`（`#[cfg(not(feature = "mock"))]`） | 导入 || `os/kernel/src/lib.rs:1437` | `let mmap = &mut *FREE_MEMMAP.get();` | 写（Phase A.2 复制 memmap） || `os/kernel/src/lib.rs:1453`／`:1470`／`:1489` | `cut_memmap(...)` 模块／内核／DTB | 写 || `os/kernel/src/lib.rs:1617` | `add_memmap(..., bootstrap_start, ...)` | 写 || `os/kernel/src/lib.rs:2540`／`:2624` | `add_memmap(..., module.start, ...)` | 写 || `os/kernel/src/memmap.rs:99`／`:186` | 只暴露 `add_memmap`／`cut_memmap` | 无只读访问器 |
穷举结论：四处 `&mut` 写入，零处不可变读取。`vm_handoff.rs:122` 提到 `FREE_MEMMAP` 只是拿它的「写一次读多次」约定作类比，不是读它。

### 分配路径与 ABI 的行号

| 事实 | 文件 | 行号 |
|---|---|---|
| `select_multi` 两处生产调用（排除清单构建） | `os/kernel/src/lib.rs` | 2498–2512、2574–2588 || 候选来源是未裁剪的 `kernel_info.memmap()` | 同上 | 2512、2588 || `VmBootAllocator::new` 只断言 regions 非空 | `os/arch/src/arch/frame.rs` | 613 || 「不做重叠检测」「不校验保留区排除」注释 | 同上 | 112–113、201–204 || 测试腿用分类后的 `vm_pmm_free_regions()` | `os/kernel/src/vm_handoff.rs` | 142（另见 `test-sysboot/src/main.rs:240`） || DTB 长度只读 header 8 字节算 `totalsize` | `os/kernel/src/lib.rs` | 1230–1234、1488 || 解析后 `Fdt` 借用释放；Phase A.5 是最后读取点 | `os/plat/src/kind.rs` 55 → `os/plat/src/device_tree.rs` 125（注释见 22） | — |`cut_memmap` 起点向下、终点向上取整 | `os/kernel/src/memmap.rs` | 146–148 || UEFI 腿 bump／根页／arena 三处相交断言 | `os/boot-shim/src/uefi_helpers.rs` | 259–260、984–986（断言体 565–580） || 自引导腿 bump 取自固定模块池 | `os/kernel-image/src/bootface.rs` | 244–246、255–260（内核回退选择逻辑 `lib.rs:737–748`） || VM 侧「整个 32 MiB bump 池都在扣减之列」 | `os/servers/vm/src/boot.rs` | 333–336 || `DTB_SOURCE_MAX = 8+8+16+8 = 40` 与静态记录数组 | `os/libs/minix-boot/src/memmap_source.rs` | 157–159、348（溢出 456–457） || UEFI 存储 256 条、最小 stride 40、缓冲动态计算 | `os/boot-shim/src/ebs_map.rs` 98–114、384–385、414–415；`uefi_helpers.rs` 697–699 | — || `BootHandoffV1` 编译期尺寸／偏移／对齐断言 | `os/libs/minix-boot/src/handoff_v1.rs` | 155–194（`offset_of!` 逐个钉偏移；arena 算术 WIRE_PARAM_OFFSET 6848、WIRE_END 7872、SEMANTIC_END 14016） || `VmBootHandoff` 只有 `size_of <= 4096`，**无字段偏移断言** | `os/libs/minix-types/src/types/boot.rs` | 435–438（容量 32 的散文边界 94–96，逐项相加为 22） || VM 消费侧 magic/version 相等断言（不等即 panic） | `os/servers/vm/src/boot.rs` | 314–316 → 387–396（reconcile 463–491） |
<!-- 自正文 §2.8 第九十六轮版本 移入 -->

### 2.8 第一次把普通物理页交出去的时刻，以及那之前扣了什么（代码读数）

**已实现**：第一次面向一般对象的普通 RAM 交付发生在 `os/kernel/src/lib.rs:fn init_proc_and_boot` 里——`VmBootRegion::select_multi(kernel_info.memmap(), &exclusions)` 选出区域后 `VmBootAllocator::new(regions)`，紧接着由 `fn load_vm_elf` 消费。真正的 VM PMM 空闲表要到 `os/kernel/src/vm_handoff.rs:fn build_vm_handoff` 里的分类步骤才发布。测试：`test_classification_deducts_all_record_sources`、`test_classification_free_list_is_exact_complement`、`test_classification_clips_to_dm_window`、`test_classification_drops_out_of_window_region`。

**该时点之前已经扣除的只有两项**：内核映像（`kernel_info.kern_phys_base()/kern_size()`）与启动模块（逐条 `kernel_info.boot_modules()`）。另一份消费者 `FREE_MEMMAP` 在更早的阶段单独扣过模块、内核与 DTB 原始长度。除此之外：

| 仍存活的启动对象 | 在第一次交付前有没有保护 | 靠什么保护 |
|---|---|---|
| 四页交接 arena、UEFI `GetMemoryMap` 缓冲 | 不在这个排除表里 | UEFI 腿靠**分类**：两者都是 `LOADER_DATA`，天然不出现在 conventional 里 |
| DTB 原始对象（非 UEFI 三条腿） | **没有** | 只由后来的扣减记录补偿 |
| 启动 bump 区（含自举页表页） | **没有** | 只由后来的扣减记录补偿 |
| DTB 自有副本 | 不适用——**今天没有独立的自有副本实现**，解析发生在固件递交的地址上 | 见 O-11 |

`build_vm_handoff` 在这些分配**之后**才运行，所以那份五类范围的扣减记录证明的是「VM 收到的集合里没有仍被占用的页」，**不能倒推**「第一跳到第二跳之间没有发生过错误分配」。因此本项把两个集合显式命名并要求不得混用：**`ConventionalCandidateRanges`**（来源归一化后的候选范围，不承诺可立即分配）与 **`VmFreeRanges`**（扣除全部仍存活对象并经过接管政策处理后的实际交付集合）。「交接给内核的可用表」属于前者，把它称作最终可分配集合即是本合同的失败形态。

**未设计（ADR）**：第一次可分配之前对所有仍存活对象的排除保护、以及 `ConventionalCandidateRanges` 到 `VmFreeRanges` 的转换点，目前没有具名接口——它属 I-29 的验收条款（§7 第 4 项）。



<!-- 自正文 O-02a 交付终点段 移入 -->

**建表机制不在本工作项内**。I-29 的交付终点是**交出一份可被消费的授权计划与禁止投影**：哪些范围可以映射、以什么属性、在哪个阶段、由哪条来源负责、哪些必须禁止。


## 六十八、第九十八轮：被替换的四段责任表与 §2.8 旧表

> 本轮按评审人意见把责任归属收到唯一口径；被替换的原文逐字保留于此。文件哈希在合并提交之后补记。

<!-- 自正文 O-02a 的四段责任表（第 2、3 段仍写 I-29）与该节后文、§7 三处互相矛盾 移入 -->

**实施顺序是四段依赖链，本项只交付第一段**（第九十七轮与 §7 同步；第九十六轮的 §7 已改成「只交付第一段」，正文这一段没跟上）（修正来源：把四类授权输入当成「已经能供数的接口」是错的——既没有普通 RAM 的授权生产者，也没有把完整来源记录交给政策层的路径）：

| 段 | 交付内容 | 责任 |
|---|---|---|
| 1 来源进入政策 | 完整、带身份的来源记录与快照身份 | **I-29** |
| 2 引导根负向投影 | 带来源归因的禁止投影 | **I-29** |
| 3 必需对象正向授权 | 对象、范围、用途、权限、树与阶段构成的授权计划 | **I-29** |
| 4 建表侧消费与验收 | 两个建表点消费该计划、替换旧宽映射、审计实际叶子 | **分页专项** |

第 1 段的最小要求与第 2 段的输入清单各在其处定义（`SourceRef` 定型与记录进政策见 O-12g；`BootstrapRootDenyProjection` 消费的五类来源见 O-12a），此处不重复展开；两者共同的硬约束是：某种输入没有接入时**必须拒绝该启动形态**，不得假定没有这种政策，且负向投影**不是把四种来源压成一个无差别 deny 位**。第 4 段被划出去的不只是机制：分页专项负责「怎么更新」，本项负责「更新后允许出现什么」——Step 4 是**第二个政策消费者**，它新增的映射必须受同一份授权计划与禁止投影约束，否则第一处改对了、第二处仍可重新引入越权映射。

1. **来源进入政策**（O-12g 的必要子集）：`SourceRef { input_object, local_source }` 身份定型；每条当前支持的启动入口都把本次启动的来源记录交给政策层持有而不是采集后丢弃；DTB 侧保留会影响引导根裁决的来源（`no-map`、MRB）；UEFI 侧保留原始描述符身份与类型（含不进分类输出的 `RESERVED`/`UNUSABLE`，否则 x86_64 没有 DTB 时类型禁止无源可查），并绑定最终快照身份（不同快照里同号的 `DescriptorIndex` 不是同一条来源）。**不要求**为所有记录补人类可读节点路径、补全多级 `ranges` 转换链、或做 `reusable` 的 owner/回收授权。
2. **引导根负向投影**（O-12a 的子集，不含最终根）：`BootstrapRootDenyProjection(tree, phase)` 至少消费 `no-map`（禁标准系统映射、不被普通正向授权覆盖）、MRB（若经页表兑现则相交页不得出现在普通映射）、`EfiReservedMemoryType`（`DenyOsUse`，不因没有 DTB 记录而消失）、`UNUSABLE`（独立类型禁止，不能只从候选表扣除）、与当前引导阶段相关的对象负向义务；每项保留 `SourceRef` 与具体原因。第一版对 `no-map` 的驱动受控例外直接 fail-closed。**这不是把四种来源压成一个无差别 deny 位**——它们仍是不同来源不同语义，只是引导根消费者统一问「这个叶子是否命中任何不允许映射的条件」。
3. **必需对象正向授权**：内核映像（分代码/只读/可写）、`EarlyExecutionClosure`、逐对象登记的启动对象、有政策的设备窗口。
4. **替换建表并做实际叶子审计**：恒等循环与 Step 4 都不得产出授权集合之外或命中禁止项的叶子；审计对象是实际 PTE，不是输入计划。


<!-- 自正文 O-12i 只留互补等式、没有定义谁决定进入哪一侧的版本 移入 -->

**页域与两层 PMM 的正式定义**（基底用补集，用途用子集——用途层不构成第二次划分）：

```
RamSourcePageDomain =
    FullPages(Union(ResolveCompatibleClaims(RamSourceDomain)))   ← 先裁决兼容性主张，再并集，再取完整页

PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx) = RamSourcePageDomain
    ← ctx 在本项只带三样输入：阶段、来源快照集合身份、证据代次。原式右侧那串交集——类型资格 ∩
      缓存可表达性 ∩ 内存序 ∩ 与用途无关的物理访问准入 ∩ ISA Profile 区域义务，再减去显式保留、
      存活对象范围与平台拒绝——**属硬件准入与映射政策，已移入专项册**。本项对基底可用集合的承诺只到
      「来源页域上的互补划分」这一层：划分必须互补、排除必须唯一、多重归属必须可追溯。

PmmRoleRejected(role, ctx) ⊆ PmmBaseAvailable(ctx)
    ← 用途拒绝不缩小基底可用集合，也不得计入 PmmBaseExcluded；用途资格是消费者在分配时附加的约束。
```



<!-- 自正文 §2.8 的四条消费关系表（未给按实例展开的时间线） 移入 -->

**四条数据的实际消费关系**（生产者是谁、消费者是谁、各自扣了什么）：

| 数据 | 生产者 | 消费者 | 实际扣除 |
|---|---|---|---|
| `kernel_info.memmap()` | 交接 v1（第一跳） | `fn select_multi`（`os/kernel/src/lib.rs` 两处：一次给内核早期装载，一次在别处） | **只有内核映像 + 启动模块**（排除数组长度 `NR_BOOT_MODULES + 1`，先 `push_exclusion(kern_phys_base, kern_size)`，再逐条 `boot_modules()`） |
| `FREE_MEMMAP` | Phase A.2 从 `kernel_info.memmap()` 复制，再 cut 模块、内核、DTB | **今天零个读者**（四处访问点全是 `&mut` 写入；`add_memmap`/`cut_memmap` 之外没有只读入口） | 它确实扣了 DTB，但它是**并行的写而不读副本**，不是 `select_multi` 的输入 |
| `VmBootRegion` | 由上述两个来源之一喂入（第二次装载用 `fn vm_pmm_free_regions`，即分类后的扣减集合） | `VmBootAllocator` | 分配器自身**不做重叠检测**，也**不校验是否避开保留区**（`os/arch/src/arch/frame.rs` 的注释逐字如此） |
| `VmBootHandoff` | `fn build_vm_handoff` 的分类步骤 | VM 服务器 | 五类 `LiveBootstrap` 扣减 + `VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap` |


### 评审身份（第九十八轮）

> 分支头是移动引用，不能当评审身份；下表按**合并提交 `aece113f8` 的字节**计算。本节追加之后 `03` 自身的哈希自然变化，以 git 对象为准。

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `aece113f8`（第九十八轮合并提交） |
| `CodeEvidenceCommit` | `53c44e846`（146/70 与交叉编译的代码基线；其后各批无代码改动） |
| `DocumentRevision` | 第九十八轮 |
| `DocumentFileHash`（正文 sha256） | `afb050f4f8250a525990fb87d0ee8f597c3cfa258a27184f3c94a0a792e5cfb0` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `e2d82e46ee4e17d58f0f834ea9f93d5685549925450bbbc4c0882e987d020813` |
| 专项册 sha256（`01-design-note-hardware-admission-model.md`） | `a1271753924074bd4ab018837e801bf94dd8d4a76d447c8b7dfbc658df485e24` |
| `WorkingTreeDiffHash` | 不适用：本轮无未提交修改 |


## 六十九、第九十九轮：被替换的排除定义与 O-06 类型表


<!-- 自正文 O-12i 的 PmmSourceExcluded 四类并集版本（MRB 未单列、RejectBoot 混进集合、归因被并集吞掉） 移入 -->

**页域与排除贡献的正式定义**（基底用补集，用途用子集——用途层不构成第二次划分）。第九十八轮补齐：只写互补等式而不写**谁决定一页进入哪一侧**，等式就是空壳；而 O-06 的类型与生命周期处置不能因为硬件准入模型迁出就一起消失：

```
PmmSourceExcluded(ctx) =
    Union( StaticReservationExclusions(ctx),   ← 显式保留声明（含固件前缀、no-map 的分配侧排除）
           LiveObjectExclusions(ctx),          ← 仍存活对象占用的范围
           TypePolicyExclusions(ctx),          ← 类型资格：UNUSABLE 不得使用、RUNTIME_SERVICES_* 第一版永久排除、
           │                                       ACPI_NVS 第一版不许普通分配、PERSISTENT 不进普通 PMM、
           │                                       UNACCEPTED 拒绝启动
           LifecycleExclusions(ctx) )          ← 生命周期：按 O-06 的静态处置表（第一版采 B）
    每项贡献都带 (SourceRef, Reason, RawRange, PageProjection, Phase)
    ← 一页只排除一次，但多个来源可各自贡献该页的排除理由；`PageProjection` 负责向外量化到整页，
      `RawRange` 保留原始字节；`Reason` 区分「保留声明／类型政策／生命周期／存活对象」四类

PmmSourceCandidatePages(ctx) = RamSourcePageDomain \ PmmSourceExcluded(ctx)
    ← **本项的输出边界就到这一层**：它回答「来源与生命周期上这页能不能进 PMM 候选」，
      不回答「这台机器允许这次访问吗」。
```

**名字必须分开，否则读者会把「来源上可用」读成「最终可分配」**：本项交付 `PmmSourceCandidatePages`；消费者把硬件准入（缓存可表达性、内存序、与用途无关的物理访问准入、ISA Profile 区域义务）作为 `HardwareAdmissionExcluded` 再减一次，得到最终的 `PmmBaseAvailable`。互补等式仍然成立、只是排除集合多一块：

```
PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx) = RamSourcePageDomain
    其中 PmmBaseExcluded = PmmSourceExcluded ∪ HardwareAdmissionExcluded
PmmRoleRejected(role, ctx) ⊆ PmmBaseAvailable(ctx)
    ← 用途拒绝不缩小基底可用集合，也不得计入 PmmBaseExcluded；用途资格是消费者在分配时附加的约束
```

`TypeAllocatablePages(p)` 的角色随之明确：它是 `TypePolicyExclusions` 的构造输入，不是本项之外的另一套结论。

<!-- 自正文 O-06 表格（MemoryKind 填是/否、BootServices 与 ACPI reclaim 处置与 O-01 静态表冲突） 移入 -->

| 类型 | 属 `RamSourceDomain` | `MemoryKind` | 类型允许普通映射 | 类型允许普通分配 | 生命周期（事件后可变） | 来源叠加 |
|---|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | 是 | **条件**：类型层面是；第一版还要求缓存属性为 WB | **条件**：阶段资格 + 缓存资格 + 全部排除项通过 | 无 | 按兼容矩阵裁决 |
| `LOADER_CODE` | 是 | 是 | 是（存活期需 X；由对象义务提供） | 退出前否 | `ExitBootServices` + loader 最后使用点 | 按兼容矩阵裁决 |
| `LOADER_DATA` | 是 | 是 | 是（**默认 NX**；写需对象义务授权 W） | 退出前否 | 同上 | 按兼容矩阵裁决 |
| `BOOT_SERVICES_CODE` | 是 | 是 | 是（固件活跃期按代码义务） | 退出前否 | `ExitBootServices` | 见下方兼容矩阵 |
| `BOOT_SERVICES_DATA` | 是 | 是 | 是（默认 NX；仍被使用时按读写义务） | 退出前否 | 同上 | 同上 |
| `ACPI_RECLAIM` | 是 | 是 | 是 | ACPI 启用前否 | ACPI 启用 | 按兼容矩阵裁决 |
| `ACPI_NVS` | 是 | 否（第一版） | 否 | 否 | 无 | — |
| `PERSISTENT` | 是 | **是（非易失字节可寻址物理内存，存在 backing）** | 否（第一版普通 volatile 策略） | 否 | 无（第一版保守） | 是否支持需明确；「无普通资格」≠「无 backing」 |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | 否 | 否 | 否 | **第一版永久排除**（项目政策，不声称已提供 Runtime Services） | — |
| `RESERVED` | **否（采用 B）**：仅凭类型名推不出是系统 RAM；有额外平台证据时另行纳入 | 否 | 否 | 否 | 无 | — |
| `UNUSABLE` | **是**（固件描述它包含错误，但它仍是物理内存的描述） | 否 | 否 | 进 PMM 排除 | 无 | — |


<!-- 自正文 O-13b 的容量段（按 11 条模块推成 46、未证明 MAX_BOOT_REGIONS 作用域） 移入 -->

第二跳的容量推导按**最终 wire 记录数**算，不是按语义来源数（第九十八轮更正）：`deducted` 数组里存的是**与候选页域相交后实际参与扣减的范围片段**——内核映像、自身根页、bump 整段、模块 blob 各是一条**整段**记录，而**每个被触碰的池区间各产生一条已用后缀记录**（`MAX_BOOT_REGIONS = 32`）。所以最坏情况是 `3 + 32 + 11 = 46` 条，**大于 `VM_BOOT_HANDOFF_MAX_DEDUCTED = 32`**；散文里那句「逐项相加 22」只在「每个池只碰一个区间」时成立。当前 `record.push` 在超容量时直接命中 `deduction record overflow — LiveBootstrap is not enumerable within capacity` 的断言——**这是启动期 panic 路径，不是可接受的失败形状**。O-13b 的验收：给出按分片数的上界推导并把容量提到该上界（或把记录按语义合并后重新推导），

### 评审身份（第九十九轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `a7d9ab4fa78f3b635d819844b9d0480ad5d08937`（第九十九轮合并提交） |
| `CodeEvidenceCommit` | `53c44e846`（146/70 与交叉编译的代码基线；其后各批无代码改动） |
| `DocumentRevision` | 第九十九轮 |
| `DocumentFileHash`（正文 sha256，按合并提交） | `6628418e62755d6c150476ddf2154bfc2a0257c79d610e0db5a4d04d7624a4f9` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `e32bb7e30770f49c0e5c501436348adbc7c887544e38be82f325118dc683e5df` |
| 上一轮（第九十八轮）合并提交 | `aece113f8`，正文 `afb050f4f8…`、档案 `e2d82e46ee…` |
| `WorkingTreeDiffHash` | 不适用：本轮无未提交修改 |


## 七十、第一百轮：设备资源与平台窗口整段移交（逐字保留）

> 归属：设备资源发现（`reg`／`ranges`／ACPI／PLIC／APIC／UART／平台 profile）交 `04-platform-discovery.md`；
> `DeviceWindowPolicy`、授权范围、映射覆盖与实际 PTE 审计交分页／映射专项。本档只留原文，供接收方引用。

<!-- 自正文 §1.6 的三合同表与平台资源六行表（设备发现并入 04，窗口映射并入专项） 移入 -->

### 1.6 三个相邻合同：本项只裁决第一个

| 合同 | 它回答的问题 | 谁负责 |
|---|---|---|
| **物理内存图** | 哪些物理范围存在、可用、保留、不可访问，各带什么属性；bootloader 自己的占用是否全部反映 | **本项（I-29）** |
| **平台资源图** | 设备资源在哪、范围多大、来源身份是什么 | 本项只负责**资源发现与描述**（下表），不负责驱动访问点审计与映射政策 |
| **页表与引导页表切换** | 虚拟地址如何映射到物理、用 4 KiB／2 MiB／1 GiB、权限与属性怎么设、根切换怎么过渡 | **BootEntry 与分页侧专项**，不作本项验收条件 |

**平台资源图这一栏必须再切一刀**，否则 O-07 的实际访问完整性会悄悄变成第 7 项阻塞，而 §7 的六项里没有它：

| 内容 | 是否属 I-29 |
|---|---|
| DTB 设备 `reg` 与多级 `ranges` 解析 | **是**（资源发现；今天未开始，见 O-12j-c） |
| ACPI 或平台静态资源描述的来源与身份 | **是**，按第一版支持范围 |
| `DeviceResource` 与平台 profile 的对应关系 | **是** |
| 每个驱动使用点的 VA、宽度、读写方向、参与 CPU | **否**——属设备访问审计（O-12c 已把它与资源描述分开） |
| `DeviceWindowPolicy`（哪些资源允许在哪棵树、哪个阶段以什么属性映射） | **否**——平台映射政策，不是资源发现结果 |
| `DeviceMappingCoverage`／实际 PTE 审计 | **否**——分页专项 |

判据是一句可检验的话：**OS 决定完全不开大页，也不应要求 bootloader 换一种内存图交接方式**。页大小是分页策略，不是物理资源描述的组成部分；因此凡以「某种叶子粒度／某次 PTE 修改／某个硬件访问能力」为条件的条款，都不属于本项的验收集合。

本轮据此撤销此前被列为阻断项的四类要求（叶子粒度与 `split_huge`、PTE 的 A/D 更新机制、页表访问的 PMP/PMA 证明、LR/SC／AMO／预约集等原子能力，以及 `FrozenBootMappingSet` 的页大小冻结）：它们的**内容继续留在正文作为设计上下文**，但**不再是 I-29 的签字条件**，逐段归属见 §7。这不等于说它们不重要——它们各自成立，只是回答的是另一个合同。



<!-- 自正文 O-12c 整段（98 行：DeviceAperture／四层覆盖／PLIC 6 MiB／x2APIC／AArch64 64 KiB 分组／QEMU profile） 移入 -->

#### O-12c 的输入：三个概念要分开

| 概念 | 回答的问题 | 允许的来源 |
|---|---|---|
| `DeviceResource` | 设备资源在哪里 | DTB 设备节点 `reg`（经每级 `ranges` 转换）、ACPI、平台信息 |
| `DeviceWindowPolicy` | 哪个窗口允许被哪棵树、哪个阶段映射 | **可审计的平台/设备政策**，不能由一段地址自动推导 |
| `DeviceAccessRequirement` | 哪个使用点经什么路径访问 | 对象/设备使用点登记 |

**`reg` 只是资源位置描述，不是映射或访问授权**：它不推出「必须映射」「属性一定是 DEVICE」「任意代码可访问」「应进入恒等映射」。

| 入口 | 设备资源来源 | 今天实际生效的授权事实源 |
|---|---|---|
| x86_64 UEFI | ACPI（未解析）或平台静态表 | 内核内静态常量表 |
| aarch64 UEFI | 同上 | 静态常量表 |
| aarch64 直启 | DTB 设备节点 `reg` + 每级 `ranges` | 静态常量表 |
| riscv64 | DTB 设备节点 `reg` / OpenSBI 平台信息 | 静态常量表 |
| 端口 I/O | **不属于物理设备窗口**，走 O-08 | 独立地址域 |
| UEFI 类型 11/12 | **不作为设备授权来源**（11 是运行时服务 MMIO 请求；12 有 Itanium 限定） | 不产生窗口 |

**数值必须带证据状态，四种强度不得混在同一列里**（`ObservedInputResource` 本次启动输入实际解析所得 / `ProfileExpectedResource` 由钉住的 `PlatformMemoryProfile` 给出 / `StaticPolicyRange` 项目当前静态政策 / `ExpectedMappingCoverage` 从现有建表代码推导、非 PTE 实测）。按代码读数：**`DtbMemoryMap` 今天不解析设备节点的 `reg`**——只处理 `/memory`、`/reserved-memory` 与 MRB；`DeviceAperture` 这个名字在代码里不存在。因此 PLIC 的 `[0x0C00_0000, 0x0C60_0000)` 是 `StaticPolicyRange`（编译期常量）＋一份**离线** DTB dump 读数（`reg = 0xc000000 + 0x600000`、`interrupts-extended = <0x02 0x0b 0x02 0x09>`，见 `03`），**不是** `ObservedInputResource`；运行时解析只有 `minix-platform` 取到 PLIC 基址且 context 数硬编码为 1，长度未取。设备 `reg` 与多级 `ranges` 转换仍属 O-12j-c 未开始项。

QEMU 侧今天能给的读数：`rewrite-notes/concepts/qemu-environments.md` 记录 8.2.2 / 8.2.10 / 9.2.4 / 10.0.13 四个版本；`os/qemu-tests/run_qemu.sh` 里 aarch64 是 `-machine virt,gic-version=3 -cpu max -smp 4`，riscv64 是 `-machine virt,acpi=off -smp 4`（或 `-machine virt -smp 4 -bios default`），x86_64 只带 `-smp 4`、**没有 `-machine`/`-cpu`**；三条腿都**未记录 accelerator 参数**，DTB 由 QEMU `dumpdtb` 生成而非外部递交。所以「逐 profile 钉住」缺的是运行参数取证，不是模型问题；在它补齐之前，6 MiB 授权与 2.25 MiB 需求都保持候选状态。

**静态表必须作为今天的事实源被清点**。
**四层关系**（不是两层）：

| 概念 | 含义 | PLIC 例子 |
|---|---|---|
| `DeviceAperture` | 平台/设备描述声明的完整地址窗口 | DTB `reg`：`[0x0C00_0000, 0x0C60_0000)`（6 MiB） |
| `DeviceRequiredAccessEnvelope` | 当前内核实际需要访问的寄存器范围 | `[0x0C00_0000, 0x0C24_0000)`（按钉住的配置推导） |
| `DeviceWindowPolicy` | 授权某个范围以指定属性映射的政策 | 本项目静态表 |
| `DeviceMappingCoverage` | 页表实际叶子覆盖的范围 | `[0x0C00_0000, 0x0C40_0000)`（2 MiB 叶子包络） |

约束是：**每个 `DeviceAccessRequirement` 的地址都落在对应 `DeviceMappingCoverage` 内**；**coverage 的每一页都要有兼容的 `DeviceWindowPolicy` 授权**；未使用的 aperture 可以不映射；包络超出所需访问范围的部分仍要逐页通过授权与属性兼容检查。**不写 `DeviceMappingRange ⊇ DeviceResourceRange`**——那会把「整个设备 aperture 必须映射」错当成普遍不变量。
**架构寄存器窗口、设备声明的 aperture 与页对齐映射包络是三件事**，不能因为某个访问点落在 4 KiB 页里就把周围数 MiB 自动变成设备窗口授权。xAPIC 是最清楚的例子：寄存器接口只有 **4 KiB**，而当前政策授权是 `[0xFEE0_0000, 0xFF00_0000)`（**2 MiB**，即 `0xFF00_0000 − 0xFEE0_0000 = 0x20_0000`），IO-APIC 授权同样是 2 MiB。要回答的不是「授权范围太大」，而是**为什么 4 KiB 的实际访问需求要用一个 2 MiB 的设备映射叶子**：只要 profile 能证明整个 2 MiB 区域可按设备属性映射（不覆盖普通 RAM、地址空洞或属性不同的区域），设计就成立；在 QEMU profile、实际 MMIO 资源描述与映射覆盖证据钉住之前，这个授权范围只是**待证平台假设**。第一版若拿不出包络证明，就只映射实际需要的 4 KiB、用更细粒度叶子。

```
ArchitecturalRegisterAperture（架构规定的寄存器窗口，如 xAPIC 4 KiB）≠ PlatformDeviceAperture（平台/固件声明的窗口）≠ PageAlignedMappingEnvelope（实际叶子的对齐包络）
```

三者不能因为某个访问点落在 4 KiB 页里，就把周围数 MiB 自动变成设备窗口授权。

| 窗口 | `DeviceAperture`（设备声明） | `DeviceRequiredAccessEnvelope`（实际访问点） | `DevicePolicyAuthorizedRange`（政策授权） | `DeviceMappingCoverage`（预期；观测待实现） |
|---|---|---|---|---|
| LAPIC（**xAPIC MMIO 分支**） | **未知**（ACPI 未解析） | `[0xFEE0_0000, 0xFEE0_1000)` | `[0xFEE0_0000, 0xFF00_0000)` | 同授权范围 |
| LAPIC（**x2APIC 分支**） | 不适用：x2APIC 用 MSR 访问，MMIO 接口不可用 | **不产生 MMIO 访问需求，也不得伪造包络** | 无 MMIO 授权 | 无 |
| IO-APIC | **未知**（同上） | `[0xFEC0_0000, 0xFEC0_1000)` | `[0xFEC0_0000, 0xFEE0_0000)`（**与 LAPIC 相邻，不是同一个包络**） | 同授权范围 |
| 平台外设窗口 | 32 MiB（QEMU virt profile） | 从已登记的设备访问点派生；**未登记的设备不产生访问需求** | `[0x0800_0000, 0x0A00_0000)`（有意授权整段） | 32 MiB |
| PLIC | `[0x0C00_0000, 0x0C60_0000)`（DTB `reg`，6 MiB） | `[0x0C00_0000, 0x0C24_0000)` | `[0x0C00_0000, 0x0C60_0000)`（**整段 aperture，第九十三轮裁决**） | `[0x0C00_0000, 0x0C40_0000)`（预期覆盖，不等于授权） |
| `RiscvVirtPlatformMmioWindow` | 平台窗口（见下） | UART 控制台寄存器偏移（资源 256 B） | `[0x1000_0000, 0x1020_0000)`（2 MiB 平台窗口） | 同授权范围 |

**2 MiB 包络的授权对象是平台窗口，不是 UART**：QEMU virt 的 `[0x1000_0000, 0x1020_0000)` 同一段里还有 VirtIO MMIO（`0x1000_1000` 起）与 fw_cfg（`0x1010_0000`），所以这条政策登记为 `RiscvVirtPlatformMmioWindow`，UART 只是窗口内的一个资源条目（`DeviceAperture` = 256 B）。**通用规则：大页包络的授权对象必须是产生该包络的完整平台内存窗口，而不是包络里恰好存在的一个设备资源。**
**`RiscvVirtPlatformMmioWindow` 的资源清单**（QEMU virt，按 profile 记录）：UART0 `0x1000_0000`+`0x100`、VirtIO MMIO 8 个窗口自 `0x1000_1000` 起每 4 KiB、UART1 `0x1000_A000`+`0x100`（**仅在使用第二个串口时实例化，属 profile 条件资源**）、fw_cfg `0x1010_0000`+`0x18`。资源存在不等于访问需求：没有 `AccessUseId` 的设备不产生 `DeviceRequiredAccessEnvelope`。
**每条 `DeviceWindowPolicy` 的完整记录**（缺任一字段就只是地址白名单）：允许映射的整页范围、OS 语义属性（普通主存/设备）、`R/W/X`、共享性、适用树、适用阶段、`ProfileId`、证据出处；记录 OS 语义，不写 PAT/MAIR/PBMT 这类架构编码。
**四个数据不是同一件事**：`Aperture` 来自设备描述；`RequiredAccess` 来自已登记的访问点（`reg` 不能证明实际访问）；`PolicyAuthorizedRange` 来自预先审查的平台政策；`Coverage` 来自实际 PTE 遍历。审计公式：`Coverage ⊆ AuthorizedRange` 且 `RequiredAccess ⊆ Coverage`。
**观测记录必须带 VA，不能只有 PA**：`DeviceMappingCoverage` 若只是物理区间，就回答不了「这些页映射到哪个 VA」「访问点用的 VA 是否落在里面」「同一 PA 是否有多个别名、权限/属性/共享性是否兼容」。观测单元定义为 `ObservedDeviceMapping { VaRange, PaRange, LeafSize, Permissions, MemoryAttribute, Shareability, Tree, Phase }`，由它派生纯物理覆盖 `ObservedPaCoverage = Union(PaRange)`，审计仍写 `ObservedPaCoverage ⊆ DevicePolicyAuthorizedPaRange`；每条 `DeviceAccessUse` 要能证明其访问 VA 经对应叶子解析到预期 PA，且宽度、读写与属性兼容。这与 O-05 的别名审计是同一条要求——不能在要求审计别名的同时让覆盖集合丢掉 VA。
**AArch64 的 64 KiB 分组属性规则：按物理地址分组、只比分组属性、限定 UEFI profile**：UEFI 规定若一个 64 KiB 物理区间含任一 4 KiB 子页属于 `RuntimeServicesCode`/`RuntimeServicesData`/`ReservedMemoryType`/`ACPIMemoryNVS`，则该区间内全部 4 KiB 子页必须采用相同的 ARM memory page attributes。规范位置 UEFI 2.11 §2.3.6，评审人已按该节核对：引用 Table 2.5，权限映射在 Table 2.6；本方抓取 uefi.org 返回 403，**未直接取得原文**，两侧分开记。文本里的 `EfiReserved` 与本项目的 `EfiReservedMemoryType` 是同一类型的两种写法。四条精度约束：

- **分组键是物理地址**：`AttrGroup(PA) = floor(PA / 65536) * 65536`。同一 VA 在不同树里的别名不改变分组；按 VA 分组会漏掉跨别名违例。
- **比较字段只限分组属性**：memory type（Normal/Device 及其子型）、inner/outer cacheability、shareability。`R/W/X`、`PXN`/`UXN`、`AF` 等**访问权限逐页独立**（Table 2.6 一支），不进一致性比较——把权限塞进分组会禁掉合法的「同组不同权限」拆分，反向也会放过属性不一致。
- **比较对象是叶子最终生效的属性，不是 `AttrIndx` 数字**：写成 `EffectiveArmMemoryAttributes = (ResolvedMAIRAttrByte, Shareability)`。两个不同 `AttrIndx` 若解析出完全相同的有效属性，不因索引编号不同而判冲突；反过来，同一 `AttrIndx` 在不同 PE 上对应不同 MAIR 内容**不算通过**（只读回索引相等证明不了任何事）。
- **适用性按 profile 判定**：这条约束随 AArch64 **UEFI 启动 profile** 的属性递交而来；非 UEFI 入口（`-kernel` + DTB）没有 EFI memory map 属性递交，规则不自动适用，而架构侧是否也按 64 KiB 粒度取属性属另一项取证，未取到之前记为 profile 条件而非普适事实。第一版锁定 4 KiB granule **不等于**规则不适用。
- **未映射子页要有组级覆盖登记**：一致性检查的对象是同一 `AttrGroup` 内**已建立叶子**的属性集合，组内其余子页尚未映射不构成违例，但审计须记 `GroupCoverage`。任一后续新映射（低 4 GiB 宽恒等映射拆分、改属性尤其）都受既有分组属性约束；若某子页必须取不同的分组属性，只能整组改属性或拒绝，不能局部改。

它与 ACPI reclaim/NVS 描述符自身的 4 KiB 对齐约束是两条独立规则，不能混用。
**三个完整性主张强度不同，不能互相代替**：`Coverage ⊆ AuthorizedRange`（已映射区域未越权）、`RegisteredRequiredAccess ⊆ Coverage`（已登记需求可映射）、`ActualDeviceAccess ⊆ RegisteredRequiredAccess`（**未证明**：没有登记的设备访问是否存在，当前无任何证据）。最终报告必须分别陈述，不能拿前两项冒充第三项。

**数值相等 ≠ 来源相等**：表中若干行的授权范围与覆盖范围数值相同，但授权是**独立政策输入**，覆盖是**独立测量**；不能由实际叶子反推授权。PLIC 是显式例外：它的授权（6 MiB）比预期覆盖（4 MiB）**更宽**，两者不得再写成「同授权范围」。而且今天**没有页表审计遍历**，所以今天的覆盖只能叫 `ExpectedDeviceMappingCoverage`（从构造代码推导），`ObservedDeviceMappingCoverage`（PTE 遍历）等 O-02a 的引导根审计落地后才存在。表中「覆盖」列在观测实现前一律标为**预期**。

**x2APIC 是第三种访问域**：xAPIC 走 MMIO（`0xFEE0_0000` 页），x2APIC 走架构寄存器（RDMSR/WRMSR）——既不属于 MMIO 窗口模型，也不属于 O-08 的端口 I/O 地址域。**MSR 编号不进共享层**：共享层只表达 `ApicRegisterAccessRequirement`（APIC 逻辑寄存器、读写方向、访问者、阶段、CPU 范围、适用模式），x86 架构层用 `X2ApicMsrUse` 记录具体 MSR 编号、指令位置与包装函数。同一 OS 语义（读 Local APIC ID、写 EOI）在 xAPIC 映射到 MMIO、在 x2APIC 映射到 MSR。**第一版若不实现 x2APIC 访问路径**，必须锁定 xAPIC profile 并在检测到 x2APIC 时拒绝启动，不得「允许 x2APIC 模式却保留 xAPIC 的 MMIO 访问需求」。O-10 的 in/out 负控不覆盖 RDMSR/WRMSR：**架构寄存器访问必须进入独立的原始操作审计**——允许出现原始 MSR 访问的函数与指令位置清单、MSR 编号与共享层逻辑寄存器的对应关系、在已批准的 `asm!` 块中加第二条 `rdmsr`/`wrmsr` 必须使审计失败、x2APIC 模式下不得残留 xAPIC LAPIC MMIO 访问义务。

**x86 的 APIC 地址不是无条件硬件事实，而且核对是逐逻辑处理器的**：表中 `0xFEE0_0000`/`0xFEC0_0000` 只属于「本项目支持的某个 x86 QEMU profile」。`IA32_APIC_BASE` 是**每处理器 MSR**，xAPIC/x2APIC 模式也按处理器启用，因此不能只在 BSP 上核对一次（这条按「评审人已核的架构事实」使用，本方未取 Intel SDM 原文，登记为待核）。启动与次级处理器入场时，对**每个参与内存访问与调度的逻辑处理器**分别核对：当前模式（xAPIC MMIO 还是 x2APIC MSR）、该处理器的 `IA32_APIC_BASE` 基址与政策表一致、其 LAPIC 寄存器窗口是否已按对应模式获得映射或 MSR 路径准入——一个 AP 的基址不同，「政策授权一个 4 KiB 页」这条主张就不再对所有参与者成立。若解析 ACPI，MADT 的 Local APIC 地址、每个处理器条目的 ID 与 enabled flags、override 与 I/O APIC 项都要与政策表兼容；任一参与处理器不符合当前 profile 就**拒绝启动**或**阻止该处理器入场**。riscv 侧同形态：PLIC 的 context 按 hart 列出（`interrupts-extended`），逐 hart 核对，不能由 hart 总数乘 2 推出来。

**PLIC 的访问集合与包络**：`DeviceAccessUseSet` 的每条记录至少含 `AccessUseId`、`DeviceId`、`ObjectId`、**`VirtualByteRange`（或其可静态解析出实际 VA 的访问点标识）**、`PhysicalByteRange`（预期 PA）、访问宽度、读/写、阶段与路径。**缺 VA 就无法证明访问需求**：指针实际落在哪个别名上只有 VA 能回答；不能先由现有 PTE 推出 PA 再充作访问需求输入（那是循环证明）。`DeviceRequiredAccessEnvelope = Envelope(DeviceAccessUseSet)`，**不表示包络内每个 byte 都被访问**；没有完整访问清单时包络只是保守上界，不能宣称访问需求已完整（与 O-07 相连）。

**静态窗口必须绑定一个 `PlatformMemoryProfile`**：每个 profile 至少含 `ProfileId`、QEMU 构建版本、machine type/version、accelerator、CPU 型号、SMP 拓扑、窗口政策集合、预期资源集合与不支持的配置。表格只引用 profile ID；x86 APIC、aarch64 32 MiB、riscv PLIC/UART 四类窗口都适用——今天四者的 QEMU 构建版本都**尚未记录**，属于待补证据。

**aarch64 的 32 MiB 是 `DeviceWindowPolicy` 窗口，不是一个设备的资源**：它表示钉住的 QEMU virt profile 对该整段授予 `DEVICE` 映射资格；其中哪些设备、哪些空隙、哪些区段当前没有访问点，由资源清单与访问需求另行解释，不能由设备名称反推。

**`PLIC_RESOURCE_LEN` 里的 2 倍关系属于 TCG 形状，不是通用资源长度**（因此 2.25 MiB 至今是候选值，不得称「内核实际所需资源长度」，除非该 profile 的 context 编号布局被证明）：代码常量写成 `0x0020_0000 + 2 * PLIC_MAX_HARTS * 0x1000`（`os/arch/src/lib.rs`），配套注释逐字说的是「M-mode 与 S-mode context of every supported hart」，claim 寄存器按 `0x20_0000 + (2N+1)*0x1000` 定位——奇数 context ID 正是 QEMU TCG 路径的形状，KVM 用连续编号。所以通用合同只有一条：`RequiredEnd = ContextBase + (MaxActualContextID + 1) × Stride`，`2 × hart_count` 只能作为**具名 TCG profile 的推导结果**出现在该 profile 的记录里。另两点要分清：`PLIC_MAX_HARTS = 32` 是本项目 SMP 上限（注释要求「至少与内核 SMP 上限一致」），不是 QEMU 的最大 CPU 数；上游 QEMU virt 的 PLIC 窗口按最大 CPU 配置可达 6 MiB 这一条按「评审人已核」使用，**本方未取 QEMU 源码**。窗口长度 `PLIC_WINDOW_LEN` 是资源范围按 2 MiB 向上取整的结果并有 `PLIC_WINDOW_LEN <= 0x60_0000` 的编译期断言——它落在 aperture 内，但 aperture 本身不因它而被授权。

**PLIC 范围的推导与 profile 条件**：`N_context = hart_count × 2` 不是通用公式——QEMU TCG 用奇数 context ID（`2·cpu+1`），KVM 用连续编号，不支持 S-mode 的 hart 还会造成 context 数差异。所需范围写成 `ContextBase + (max(实际使用的 context ID)+1) × Stride`；context ID 不能由 hart 数直接乘 2，也不能把 `interrupts-extended` 的 interrupt specifier 当成 PLIC 的 MMIO `addrid`。内核实际需要的资源长度是 `PLIC_RESOURCE_LEN = 0x0020_0000 + 2 × PLIC_MAX_HARTS × 0x1000`（32 hart 时 `0x24_0000`）。

**本项「平台授权范围」的数值来源已裁决（第九十三轮）：否决按叶子粒度反推的 4 MiB 授权**。代码注释逐字写着 Step 1 按 2 MiB 出设备叶子，所以授权范围取资源范围的 2 MiB 包络，`PLIC_WINDOW_LEN = (PLIC_RESOURCE_LEN + 0x1F_FFFF) & !0x1F_FFFF`（并有 `is_multiple_of(0x20_0000)` 的编译期断言，`os/arch/src/lib.rs`）。先看到 2 MiB 叶子、再把所需范围向上取整成 4 MiB，这种生成方式不属于授权政策，正是本文自己确立的 `DeviceWindowPolicy` 与 `DeviceMappingCoverage` 分离原则所禁止的推导。第一版 PLIC 改按**完整 aperture 授权**：

| 层次 | 第一版裁决（钉住的 riscv QEMU virt profile） | 依据状态 |
|---|---|---|
| `DeviceAperture` | `[0x0C00_0000, 0x0C60_0000)`，6 MiB | DTB `reg` 的资源位置描述，不是映射许可 |
| `DeviceRequiredAccessEnvelope` | `[0x0C00_0000, 0x0C24_0000)`，2.25 MiB | 仍须按实际 context 证据确认（见上条推导规则） |
| `DevicePolicyAuthorizedRange` | `[0x0C00_0000, 0x0C60_0000)`，6 MiB | **须由 `PlatformMemoryProfile` 独立证明**，不是 `reg` 自动给的许可 |
| `DeviceMappingCoverage` | `[0x0C00_0000, 0x0C40_0000)`，4 MiB | **预期覆盖**，等实际 PTE 观测；它是 2 MiB 粒度的产物，不是授权 |

三条包含式必须同时成立：`DeviceRequiredAccess ⊆ DeviceMappingCoverage ⊆ DevicePolicyAuthorizedRange`，且 `DevicePolicyAuthorizedRange ⊆ ProfileVerifiedDeviceAperture`。**`DevicePolicyAuthorizedRange` 不得由 `PageSize`／`LeafSize` 推导**：换成 4 KiB 或别的粒度，授权范围不随之变化——这就是它与建表粒度脱钩的检验。两个边界：① 6 MiB 不因 DTB 写了 `reg` 就自动获得映射许可，必须由 profile 整段确认这些页都属 PLIC 设备窗口、每页都能以目标设备属性映射、不与 RAM／地址空洞／其它不兼容区冲突；**证据不成立就拒绝该 profile**，不得为了保住当前 2 MiB 建表方案强行放行（届时要么分页侧改用更细粒度，要么平台侧拿出独立子窗口证据）。② 不把「授权范围必须等于 aperture」提升为所有设备的普遍定律——普遍规则只有上面那三条包含式与「不得由叶子粒度推导」；若有独立平台证据证明某设备只需较小子窗口，可以只授权该子窗口，但**不能用 `AlignOut(RequiredAccess, LeafSize)` 代替那份证据**。本项 PLIC 数值取**6 MiB 授权、4 MiB 预期覆盖**。这项裁决不替代 PLIC context 的完整性证明：`max(实际 context ID)`、hart 与 context 的对应关系、QEMU 构建版本与 machine version 仍需钉住，`2 × hart_count` 不得回到通用公式。


这些条目今天来自编译期常量（`kernel_identity_mmio_regions()` 与 `MMIO_PA_BASE/MMIO_WINDOW_LEN`），**不是** DTB/ACPI 解析结果。
**每条窗口必须绑定可验证的版本与动态规则**：QEMU 版本或 machine version、固定地址依据、动态范围的计算式、与 DTB `reg` 的一致性检查、不支持的配置范围。窗口表复制自某个 QEMU 版本时，不能继续当作另一个版本的硬件事实。**DTB 与静态表不一致时的规则**（拒绝启动 / 以哪一方为准）必须写明；第一版倾向拒绝。



<!-- 自正文 §0 的十九行外部模型速览表（含 HardwareAdmissionExcluded／RoleAdmit／AtomicCapabilityPermits 等） 移入 -->

## 0. 现行模型速览（读 §5 前先拿住这些名字）

本节**不新增规则**，只把 §5 里反复出现的名字与等式集中一处，方便逐条评审时定位。下表里的名字目前**全部是候选合同，一行代码都没有**（代码现状见 §2）。

| 名字 | 它回答的问题 | 展开位置 |
|---|---|---|
| `RamSourceDomain` | 本入口的权威输入是否声称这段是物理主存（尚未应用保留声明与对象生命周期） | §3 第 2 条、O-12f |
| `RamSourcePageDomain` | 上述主张先经兼容性裁决、再并集、再取完整页 | O-12i |
| `PmmSourceExclusionContributions` / `PmmSourceExcludedPages` | 前者是**贡献记录**（`evidence_ref`／`reason`／`raw_range`／`page_projection`／`phase`，MRB 单列、类型政策只取 `ExcludeRange`），后者是这些贡献的页并集——归因与集合分开，页只扣一次 | O-12i |
| `PmmSourceCandidatePages` | **本项的输出边界**：来源域减去上述页集合；它不回答「这台机器允许这次访问吗」 | O-12i |
| `HardwareAdmissionExcluded` | 架构与平台物理访问能力不满足而排除的页（缓存可表达性／内存序／与用途无关的访问准入／Profile 义务）——**专项侧**，本项不实现 | 专项册 |
| `PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)` | 最终基底层的可用与排除互补且**只对页域成立**；`PmmBaseExcluded = PmmSourceExcludedPages ∪ HardwareAdmissionExcluded` | O-12i |
| `PmmRoleEligible(page, U, ctx)` | 把一页交给用途集合 `U` 的分配资格；用途未知时保守取 `AllRoles` | 专项册（非本项验收） |
| `RoleAdmit` ≡ `RolePageEligible` | 用途特有准入**只有一个展开式**，且它是分支：`PageTable` 走 backing，其余走通用体。三个消费者共用同一个判定核，各喂自己的需求集合——基底 `BaseAccessRequirements`、通用用途 `RoleSpecificRequirements`、页表用途 `PageTableAccessRequirements` | 专项册（非本项验收） |
| `PageTableBackingEligible` | 页表用途的唯一能力合同；新分配页与引导期既有页表共用（既有页表不经过 PMM，因此排序、Profile 区域义务、内存类别与幂等政策都必须自带在 backing 里） | 专项册（非本项验收） |
| `AccessRequirementsEligible`（唯一全称判定核） | `requirements ≠ ∅ ∧ ∀ req ∈ requirements : LegalStarts(page, req, role, ctx) ≠ ∅ ∧ ∀ a ∈ LegalStarts ∀ cpu ∈ ctx.ParticipatingCpuSet : PhysicalAccessPermits(cpu, req, Footprint(a, req))`，其中 `PhysicalAccessPermits = PmpPermits ∧ PmaPermits ∧ PlatformProtectionPermits`；`LegalStarts` 第一版锁**合同 A**（架构起始对齐 ∧ 该用途规定的完整 footprint 约束）。空需求集合与空起始集合都不允许让全称量词真空通过 | 专项册（非本项验收） |
| `VirtualAccessInstance → TranslatedAccessFragments → ActualMemoryOperations` | 对象访问的三层：源级一次访问、完整翻译片段、硬件实际执行的操作——**不得合并** | 专项册（非本项验收） |
| `ObjectPolicyAccessEligible` / `ObjectPhysicalAccessEligible` / `ObjectMappedAccessEligible` | 对象可用性的三面：来源授权、硬件准入、映射路径兑现 | 专项册（非本项验收） |
| `MemoryAdmissionContext` | 一切投影的输入：阶段、参与 CPU 集合身份、平台内存 profile、ISA profile、UEFI 属性位 profile、起始地址合同、证据代次、来源快照集合 | 专项册（非本项验收） |
| `SystemHartSet` / `ParticipatingHartSet` | 前者验证 A/D 更新方案的**全系统**一致性，后者计算普通物理访问准入的**逐 hart 交集**，不得互相替代 | 专项册（非本项验收） |
| `CommonBackingCapabilityEligible` + `PageTableSpecificAccessEligible` | 页表 backing 的两半：与用途无关的公共能力（含**缓存可表达性**）与页表特有的三类访问 | 专项册（非本项验收） |
| `AtomicCapabilityPermits` | 共享层唯一的原子能力谓词：只问「这些 CPU 在该范围上能否满足该算法的族／宽度／原子性／进展要求」。预约集是 RISC-V LR/SC 的承载形式，其几何与证据留在 adapter 侧；共享进展成员叫 `EventualAtomicProgress`，由 RISC-V adapter 兑现为 `RsrvEventual` | 专项册（非本项验收） |
| `SegmentsCorrespondToExpectedTranslation` / `OperationsRealizeAccess` | 对象访问三层之间的对应关系：逐段 VA→PA 对应（不是比物理并集）、操作集合是源级访问的忠实展开（adapter 证明） | 专项册（非本项验收） |
| `NoMapControlledAccessEligible` | 受控映射是必要非充分：谁能经哪条 VA 实际碰这些字节，另立访问侧合同 | O-03 |
| 甲案：授权驱动建表（**已裁定，属分页专项背景，非 I-29 交付**） | 恒等循环的输入换成四类已裁决授权对象（合法普通 RAM、内核映像、启动对象、设备窗口），外加根激活瞬间的 `EarlyExecutionClosure`；具名过渡映射 `BootTransitionMapping` 被否决 | O-02a |

> 上表标注「**专项册（非本项验收）**」的行，展开位置是同目录的 `01-design-note-hardware-admission-model.md`（第九十六轮逐字迁出）。它们描述硬件准入与用途资格，不是内存图交棒的签字条件；本项只保留页域、两层划分与「用途拒绝不等于基底排除」这三条与交接直接相关的事实（见 §5 末节的保留表）。

**共享层与架构 adapter 的分工**贯穿上表：共享层只表达 OS 需要的语义（普通物理访问、页表读取与更新、原子操作的宽度/原子性/进展、执行访问、参与访问的 CPU 集合、证据代次），机制名（PMP/PMA、`Svade`/`Svadu`/`ADUE`、AMO/LR/SC、`RsrvEventual`、MAIR、PAT/MTRR）留在各架构 adapter。本文的 RISC-V 段落因此是 **adapter 侧的展开示例**，不决定三架构共享接口的形状。



<!-- 自正文 O-06 的 MemoryKind 八成员表（ACPI_RECLAIM 被当成物理性质、Permanent 与可分配相撞） 移入 -->

| 类型 | 属 `RamSourceDomain` | `MemoryKind` | 类型允许普通映射 | 类型允许普通分配 | 规范生命周期 → **第一版处置** |
|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | `VolatileRam` | 是（**缓存属性另行判定**，不在类型列） | 是（阶段／属性／排除另行判定） | 无事件 → `Permanent` |
| `LOADER_CODE` | 是 | `VolatileRam` | 是（存活期需 X，由对象义务提供） | 否 | `ExitBootServices` → **`Permanent`（第一版不回收）** |
| `LOADER_DATA` | 是 | `VolatileRam` | 是（默认 NX；写需对象义务） | 否 | 同上 |
| `BOOT_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 是（固件活跃期按义务） | 否 | 规范：`ExitBootServices` 后可通用 → **第一版 `Permanent`** |
| `ACPI_RECLAIM` | 是 | `AcpiReclaim` | 是 | 否 | 规范：ACPI 启用后可回收 → **第一版 `Permanent`**（没有 `AcpiEnabled` 消费者） |
| `ACPI_NVS` | 是 | `AcpiNvs` | 否 | 否 | 无 → `Permanent` |
| `PERSISTENT` | 是 | `PersistentMemory` | 否（第一版普通 volatile 策略） | 否 | 无 |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 否 | 否 | 第一版永久排除 |
| `RESERVED` | 否（采用 B）：仅凭类型名推不出是系统 RAM | 不承诺 | 否 | 否 | 无 |
| `UNUSABLE` | 是 | `UnusableRam` | 否 | 否 | 无 |
| `UNACCEPTED` | 是 | `UnacceptedRam` | 否 | 否 | **`RejectBoot`**（不是页排除） |
| `PAL_CODE` | 不支持 | `Unsupported` | — | — | **`RejectBoot`** |
| 类型 11 / 12 | 否 | `NonRamPhysicalResource` | 否 | 否 | 无 |

**资格是分层的，不是三个布尔值**：`RamSourceDomain`（权威来源是否声明这是物理主存）、`MemoryKind`（普通易失 / 持久 / NVS / 未接受 / 不可用）、`OrdinaryRamMappingEligibility(tree, phase)`、`PmmEligibility(phase)`、`AccessRequirement/AccessAuthorization` 各自回答不同问题，不能互相推导——普通 RAM 可能因 `no-map` 禁止普通映射；设备 MMIO 不是普通 RAM 却有设备访问授权；普通 RAM 可能因 MRB 禁止访问；保留对象可能允许特定对象读却不许 PMM 分配。表头「普通主存性质」只描述 `MemoryKind`，不是「能否使用」的总结论。
**「来源叠加」不是无条件成立（兼容矩阵）**：DTSpec 要求静态 `/reserved-memory` 出现在 UEFI 内存图里——`no-map` 对应 `EfiReservedMemoryType`，其它静态保留对应 `EfiBootServicesData`。所以表里「按兼容矩阵裁决」指的就是下面三种组合：

| UEFI 描述符 | 与之重叠的 DTB 静态保留声明 | 结果 |
|---|---|---|
| `EfiBootServicesData` | 普通 / `reusable` | 兼容（规范给出的预期组合） |
| `EfiReservedMemoryType` | `no-map` | 兼容 |
| `EfiReservedMemoryType` | 普通 / `reusable` | **冲突，拒绝启动**——这不是待裁事项，O-11 权威表一已给结论 |
| 其它类型 | 任意静态保留 | 与预期类型不符：拒绝启动，除非有明确列出的项目扩展规则 |

三张表（本矩阵、O-11 权威表一、O-11 权威表二）里**只有 O-11 权威表一是裁决来源**；本表是它在 UEFI 类型一侧的投影，后续修改必须先改权威表一再同步本表，不得两处各写一套结论。

同一页同时被普通静态保留与 MRB 覆盖时，**两种类型要求冲突、第一版拒绝启动**（见 O-11 的同页裁决表）；不存在「较强类型覆盖较弱类型」的泛化规则，来源记录在拒绝诊断中分别保留。动态 `size` 保留：规范要求它**不出现在** UEFI 内存图（不是「不要求出现」）；`reg` 与 `size` 同时存在时按静态 `reg` 处理，`size` 不决定分配。
**第一版处置与统一模型语义分开**：「属 `RamSourceDomain`」是按统一模型填的，不等于今天的分类器已经支持它继续启动：

| 类型 | 统一模型中的含义 | 第一版处置 |
|---|---|---|
| `UNACCEPTED` | 有物理来源、尚未接受、无普通 backing | **拒绝启动**，不进入阶段化分配 |
| `PAL_CODE` | 固件/处理器专用的代码内存 | 拒绝启动（项目兼容性限制） |
| `PERSISTENT` | 非易失物理内存，有 backing | 不接入普通 volatile PMM 与直接映射，保留来源记录 |

**属性位：语义三类 + 当前处置**（不能只写「未知位拒绝」）。语义分三类：缓存能力（`WB`/`WT`/`WC`/`UC`/`UCE`）；物理保护能力（`WP`/`RP`/`XP`/`RO`）——`WP` 归此类：规范先称它 physical memory protection attribute，再说明它通常被用作 write-protected cacheability policy，所以它有特殊缓存语义，但既不是页表写权限，也不能与 `RO` 混为一谈；其它特征（`MORE_RELIABLE`、`CPU_CRYPTO`、`SP`、`NV`、`RUNTIME`）。当前处置：模型保留 `RUNTIME`/`NV`/`SP` 三种独立属性语义，但**分类器实际可达的只有 `RUNTIME`**——`NV` 属无条件冲突拒绝域，`SP` 的低位编码属冲突拒绝域、高位编码在 profile 未钉住时被 `ProfileAttribute` 拒绝，二者都不进入输出表；`PERSISTENT` 是独立的 UEFI 内存类型，不等于 `NV` 属性位，不能用它的测试证明 `NV` 分支可达。`RO`/`RP`/`XP`/`WP`/`CPU_CRYPTO` 是 **capability 能力位**（`MORE_RELIABLE` 因 `0x1_0000` 的官方文本歧义已改为 fail-closed），第一版只作来源元数据——**不直接等同于当前已生效的访问权限**，也不改变 RAM 来源域成员资格；后续映射策略可用其中的能力信息验证目标属性是否可兑现，**`WP` 不得误解为页表写权限**。**`SP` 不是 no-map**：第一版把它排除出普通 PMM 是项目保守选择，不得由 `SP` 自动生成映射禁令。任何**档外位**仍按第 1 步拒绝启动。

### 评审身份（第一百轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `371bc06fca4f0fee0951cf98825d1772fe2e1fff`（第一百轮合并提交） |
| `CodeEvidenceCommit` | `53c44e846`（146/70 与交叉编译的代码基线；其后各批无代码改动） |
| `DocumentRevision` | 第一百轮（**设计评审循环已冻结**，见正文 §7.1） |
| `DocumentFileHash`（正文 sha256，按合并提交） | `fa1dcc02611952943c266378b4e61354b18151ec510039d24b5844b69b9ce29b` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `580c35cef0cd82908d12f31631ffc8393722c63f2077b8727fb304a42e9e8b6c` |
| 上一轮（第九十九轮）合并提交 | `a7d9ab4fa`，正文 `6628418e62…`、档案 `e32bb7e307…` |
| `WorkingTreeDiffHash` | 不适用：本轮无未提交修改 |

### 评审身份（第一百零一轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `3683a34cacd4f309c7b6797cd2936b9a600235e5`（第一百零一轮合并提交；本轮含代码） |
| `CodeEvidenceCommit` | `3683a34cacd4f309c7b6797cd2936b9a600235e5`（146/70、861 与三架构交叉构建均在本提交上取得） |
| `DocumentRevision` | 第一百零一轮 |
| `DocumentFileHash`（正文 sha256，按合并提交） | `3eac16ae357abc980b50ae8a782cf9b93fa4785a44a3ba45ff6970dcc308386c` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `7cbfece6cdbc0ae3fb349df8d1e7e3eeab58a1a7f8a935867fad36f15fe1a8f9` |
| 上一轮（第一百轮）合并提交 | `371bc06fc`，正文 `fa1dcc0261…`、档案 `580c35cef0…` |
| `WorkingTreeDiffHash` | 不适用：本轮无未提交修改 |


## 七十一、第一百零二轮：下游专项展开段的逐字存档

> 这些段落回答的是「怎样映射／怎样访问／怎样证明硬件允许」，已按射程压成正文里的边界句。

<!-- 自正文 §1.5 三架构准入表（PAT/MTRR、MAIR、PMA/PMP 的逐架构展开） 移入 -->

### 1.5 三架构的差异

| 架构 | 普通主存的兑现方式 | 额外准入 |
|---|---|---|
| x86-64 | 内核自建 `PAT`，普通页显式取写回索引；`MTRR` 只读分析 | **有效类型 = `Resolve(PAT_Entry, MTRR_Type(physical_range))`**，只看 PTE 选择 WB 不足；大页跨不同 MTRR 区间必须拆分或拒绝；每核 `CR0.CD=0`、`CR0.NW=0`；**MTRR 读取结果也进入逐核有效类型一致性证明**，不能假定所有处理器配置天然一致 |
| AArch64 | `MAIR` 两个索引（普通/设备）；第一版锁定 4 KiB translation granule | 两棵树各自遍历实际叶子；每核读回 `MAIR` |
| RISC-V | 按物理区间的 PMA 能力（宽度/对齐、取指、页表读写、原子、排序、一致性）；**能力值来源必须写明并绑定 machine/SoC 型号与版本**：平台静态表 / 固件或 M-mode 信息 / SoC 寄存器 / 已验证来源，不存在通用查询接口；**不得把「未发现限制」当成「支持普通主存访问」** | PMP 是另一层：约束 S 模式取指、数据访问**与页表遍历** |



<!-- 自正文 §3 第 3～5 条（映射禁止、义务键、设备访问≠设备映射的判定式） 移入 -->

3. **每棵树各有独立的映射禁止集合；访问禁止另有独立集合，按对象、阶段与访问路径裁决。禁止映射与禁止访问不得互相代替；禁止与分配正交**。`no-map` 的两项子语义——`DenyStandardSystemMapping` 与 `DenyUncontrolledSpeculativeAccess`——也各自独立，不因为都会影响页表映射就被压平成一条；受控驱动例外只属于后者，不能顺带解除前者。
4. **义务键 =（对象｜设备窗口，树，阶段，路径）**；缺任一维不构成义务。
5. **设备访问 ≠ 设备映射**：MMIO、端口 I/O 与 CPU 架构寄存器访问是**三种不同的访问接口/命名域**（第三种不属于物理地址集合，不得进入 PMM、页表或端口范围集合）；映射义务与访问合同是两份记录。



<!-- 自正文 §4 的五行表（三架构准入、恒等 VA、effective_pa_limit、保留映射权限、设备访问边界） 移入 -->

## 4. 已裁定、但代码未实现

| 合同 | 内容 | 为什么现在还不能当现状 |
|---|---|---|
| 三架构普通主存准入 | 见 §1.5 表；x86 的 `PAT`/`MTRR` 分析与每核检查、AArch64 的双树遍历与 `MAIR` 读回、RISC-V 的区间能力清单与 PMP | 没有代码，也没有真实板卡证据 |
| 恒等 VA 合法性 | `VA = PA` 必须落在该树分页配置的合法虚拟地址范围，且不与既有布局冲突 | 未实现；`effective_pa_limit` 不能替代它 |
| `effective_pa_limit` 的含义 | **物理地址可表示上界**（格式位宽 ∩ 平台位宽），**不承诺**上界以下可访问 | 已裁定；PMP 与 PMA 是独立准入维度 |
| 保留映射区的权限 | **已经获得普通 RAM 映射授权的保留对象**默认 `R`、`NX`（不适用于 `no-map`、MRB、类型政策禁止映射的区域，也不适用于走独立 `DEVICE` 属性的 MMIO 窗口）；需要 `W` 或 `X` 的存活对象（loader 代码、仍被读取的数据）必须独立登记用途、生命周期与权限，并通过对应树的叶子审计；第一版不支持的权限需求**拒绝启动** | 未实现；今天保留表没有权限字段 |
| 设备访问边界 | 原始 MMIO/端口操作只允许出现在逐个列举的包装函数；调用点带稳定 `AccessUseId`；CI 静态审计 + 两枚负控 | 未实现；今天 raw 端口 asm 只在 `arch/`、`plat/`、`boot-shim/` 共 5 个文件，`kernel/`、`servers/` 零命中 |



<!-- 自正文 O-02b 的候选与判据展开 移入 -->

### O-02b 普通保留声明是否产生映射禁令

**问题**：`/reserved-memory` 的普通声明只要求「从正常内存用途中排除」；只有 `no-map` 禁止标准系统映射，MRB 才禁止访问。三者强度不同。
**失败形态**：一条普通保留声明被当成 `no-map`，产生规范不要求的映射禁令，后续合法的设备窗口映射被判失败。
**候选**：

- **A**：普通保留声明**只产生 PMM 排除**，不自动产生映射禁止或访问禁止；若某对象需要映射，必须另有对象/设备映射授权。`no-map`（禁止标准系统映射与非受控推测访问，**设备驱动受控使用例外**）、MRB（未授权不得访问）、类型政策分别生成各自的禁止语义；`reusable` 是另一种语义且与 `no-map` 互斥。
- **B**：所有保留声明都进禁止集合（更保守，会误禁）。

**判据**：每条禁止项都能指回 `no-map` / MRB / 类型政策之一；不存在「因为没 backing 所以禁止」。



<!-- 自正文 O-03 的候选 A／冲突规则／来源叠加矩阵 移入 -->

### O-03 禁止项的来源分级

**问题**：禁止映射与禁止访问是两个集合，来源不同：`no-map` 禁止标准映射；MRB 禁止访问；类型政策禁止映射；对象义务禁止某个对象在某个阶段映射或访问。
**失败形态**：把保留声明整类塞进禁止集合（普通保留被误禁），或只处理 `no-map`（MRB 的访问禁止丢失）。
**候选**：

- **A**：每条禁止项带来源标签（`no-map` / MRB / 类型 / 对象），两份集合各自生成、互不代替。
- **B**：单一集合 + 来源位图，由消费者过滤。

**判据**：每条禁止项可追溯到一个来源条目；两份集合不能互相替代。
**冲突组合规则（按评审人给的六行，本项目采纳为候选规则）**：

| 情况 | 结果 |
|---|---|
| 普通映射授权 vs `no-map` | 拒绝普通映射 |
| 普通访问授权 vs MRB | 拒绝访问 |
| 设备驱动受控使用 vs `no-map` | 只有满足规范允许的受控用途且具备专属授权时才允许例外 |
| `EfiReservedMemoryType` + `no-map` + `DriverControlledUse` | **拒绝**：该页命中 UEFI 的 `DenyOsUse`（OS 不应访问）与项目不开放驱动例外的合同；不是「UEFI 类型已提供硬件访问屏障」。这不等于「所有 `no-map` 一律禁止驱动受控使用」——只是这一来源组合多了一条更强的类型政策 |
| 未获启动程序授权的访问 vs MRB | 拒绝访问 |
| 类型政策绝对禁止 vs 对象授权 | 拒绝 |
| 对象 `MustNotMap` vs 一般 `MustMap` | 拒绝启动，除非设计明确列出例外 |

原则不是「全局优先级数字」，而是：**一般正向授权不能覆盖具体的负向限制；例外必须具有更具体的对象身份、范围、路径与阶段；来源语义未明确允许的例外一律拒绝启动**。`no-map` 的例外与 MRB 的例外不是同一机制。
**`no-map` 要拆成三项，不能简化成一个无条件 `DenyMapping`**：`DenyStandardSystemMapping`（禁止进入标准系统内存映射）、`DenyUncontrolledSpeculativeAccess`（禁止非受控推测访问）、`DriverControlledUse`（专属设备身份 + 专属访问路径的受控使用）。第三项的第一版状态必须写死：`NoMapControlledUse { ModelCapability = ReservedForFuture, FirstEditionPolicy = Reject }`——**第一版没有任何可通过的 `DriverControlledUse` 政策实例**，实现者不得自行开放这条尚未兑现的路径；规范允许的未来扩展保留在模型能力里，不等于当前政策已开放。「有驱动申请」不是通用豁免，不能让 `no-map` 页进入内核直接映射。
**受控映射是必要条件，不是受控访问的充分证明**——访问侧另立一份合同：

```
NoMapControlledAccessEligible(use, device_id) =
    指定设备身份 ∧ 指定对象身份 ∧ 指定访问路径 ∧ 指定阶段
    ∧ 不存在绕过该受控路径的别名或其它访问主体
    ∧ 不违反非受控推测访问限制
    ← 映射侧只回答「这个叶子允许存在吗」；本式回答「谁能经哪条 VA 实际碰这些字节」。
      一个叶子带 DriverControlledMapping 标记，不能自动证明全部访问都由指定驱动控制
```

**MRB 的部分页**：若靠页表兑现 MRB 访问禁止，则该页整体不得出现在普通映射中；同页还有其它对象需要映射时，拒绝冲突或提供独立受控机制。精确 byte-range 检查不能代替页表兑现。
**`no-map` 与 MRB 在页表粒度上的保守投影相似，但原始语义与例外机制不同**：`no-map` 的普通系统映射禁止按相交页向外投影；MRB 是「未经授权不得访问」，两者不能共用一套例外规则。`DriverControlledUse` **不自动解除整页限制**；若专用映射的实际叶子覆盖超出 `no-map` 资源范围，必须证明整叶覆盖区的属性、权限、别名与访问控制均兼容，否则拒绝启动或改用更细页粒度。测试要求：`no-map` 只覆盖 4 KiB 页的部分字节时，普通直接映射不得出现该页，驱动例外也不得未经整页审查放行。



<!-- 自正文 O-04 的投影候选与判据 移入 -->

### O-04 对象负向义务的投影

**问题**：`MustNotMap` 与 `MustNotAccess` 不同：前者禁止某棵树在某个阶段建立映射，后者禁止访问路径。今天的禁止集合构造式里没有它们的位置。
**失败形态**：某对象被裁定「在引导根、S2 禁止映射」，但不进任何集合，宽恒等映射照旧覆盖它。
**候选**：

- **A**：两个投影名（映射禁止 / 访问禁止），流式贡献，保留对象、树、阶段、路径四维。
- **B**：第一版规定对象矩阵不出现负向态，并加空集断言。

**DTB 首读与 `no-map` 的先后关系必须裁定（第九十五轮：采用启动输入例外，不采用 ABI 前置保证）**：自引导形态的顺序是「ABI 递入 DTB 指针 → 读 header → 复制完整 DTB → 解析 `/reserved-memory` → 才知道有没有 `no-map`」。若 DTB 自己的物理范围恰好落在它自己声明的 `no-map` 区里，内核在**读出声明之前已经访问了该区域**——单靠 DTB 的内容无法在首读前证明不相交，所以「要求启动程序保证 DTB 不与 `no-map` 相交」这种 ABI 前置保证在第一版拿不到，拿不到就不受支持不是可选方案。裁定：`BootInputReadUse` 是**独立具名的一次性启动读取合同**——严格限定（指定对象、指定字节范围、只读、指定阶段与路径、复制到自有副本即失效），它**不属于标准系统映射，也不等价于 `DriverControlledUse`**，因此与 `FirstEditionPolicy = Reject` 不冲突；随后发现 DTB 与 `no-map` 冲突时拒绝继续启动。三条边界不许含糊：MRB 的启动读取依据启动程序明确授权的用途；`no-map` 的读取依据本项目自己规定的一次性启动读取合同；**两者不得因为都发生在 DTB 读取阶段就共用一个宽泛的 `BootInputProvenance` 例外**。

**`BootInputReadUse` 的准入式与它的证明责任分层（第九十七轮）**：一次性只读本身**不能**证明没有非受控推测访问——DTSpec 对 `no-map` 的要求是「不得进入标准系统映射」且「不得允许非设备驱动控制下的推测访问」，两者都是访问侧条件。所以准入式必须写成五项，且最后一项不许只是布尔标签：

```
BootInputReadEligible(use, source_policy, platform_profile) =
    provenance_valid(use, platform_profile)                 ← 输入对象身份与来源快照成立
    ∧ reachability_valid(use, platform_profile)             ← 端到端可读到这段字节
    ∧ explicit_access_is_bounded(use)                       ← 范围、宽度、方向、失效点写死
    ∧ no_standard_system_mapping_is_created(use)            ← 不因这次读取建立标准映射
    ∧ uncontrolled_speculative_access_is_prevented(use, platform_profile)
```

前四项由本项负责（都是启动输入合同的文字条款）；**第五项是每条启动入口与架构的实际保证**，本项只负责把它列为准入条件并要求具名依据；给出保证的机制（是否依赖地址转换未开启、是否受 PMP/PMA 约束、是否需要在读取前建立某种隔离）属硬件准入，见专项册。证明不出来时的正确动作是**拒绝这种输入布局**，而不是因为「这是启动读取」就默认豁免。

**MRB 一侧的四问必须逐条 ABI 回答**（DTSpec 允许在启动程序给出明确用途信息时访问 MRB，未授权前不得访问）：① 该启动程序是否明确说明递交的地址就是 DTB；② 是否明确授权内核读取该对象的**全部内容**；③ 授权是否只覆盖这一个对象、而不覆盖 MRB 其余字节；④ 读取授权是否在复制到自有副本之后失效。**单纯存在一个地址寄存器，不自动等价于它所指向的所有物理范围都可任意访问**；若启动 ABI 的语义确实是「把这份 DTB 交给内核消费」，这句话要写进 ABI 合同并给出来源（今天四条入口的这条合同仍是假设，见 §6）。

**判据**：R19a 的输入清单与两份禁止集合的构造式逐项对得上；`DenyMapping` 不被 `DenyAccess` 代替。



<!-- 自正文 O-05 的 T0/T1/T2 责任表与冻结字段表 移入 -->

### O-05 审计时点与「冻结」的含义

**问题**：按字面「T1 前完成最终根审计」不可能（最终根到 S3 才建）；「冻结 PTE」若理解为字节不变，会与硬件 `Accessed`/`Dirty` 更新冲突。
**失败形态**：实现把最终根的空遍历当作已审计；或把硬件置 A/D 判成权限变化。
**候选 A（已裁定，待实现）**：按时点拆开——

| 检查点 | 时点 | 责任 | 归属 |
|---|---|---|---|
| T0 `RootActivationPreflight` | `paging.enable()` **之前** | 初始根的实际叶子、禁止项命中情况、根激活瞬间的执行闭包是否已覆盖 | 引导页表切换合同（BootEntry 侧） |
| T1 `FrozenBootMappingSet` | 引导根上**全部结构修改完成之后** | 最终引导映射集合的覆盖、权限、属性、别名与禁止集合；含设备映射验收（R13-D） | 分页侧专项 |
| T2 `FinalRootPreflight` | 最终根激活之前 | 另一棵树的同类审计 + 三类义务合并目标（R21） | 分页侧专项 |

**这张表替代此前的「T1 在激活引导根前」**：第九十三轮已裁定 `enable()` 不是 T1，时点表若继续写「激活前」就与裁定自相矛盾。拆开之后各点责任不再互相冒充——**T0 不能被 T1 的事后通过替代**，因为 T1 检查的是经过修改后的稳定集合，而 T0 检查的是「激活这一刻必须成立」的条件。T0 有一条落地前必须回答的问题：根尚未激活时如何遍历实际 PTE——若只能审计构造计划而不能检查已写入的 PTE，该阶段的证明强度必须如实降低，不得写成「已观测」。

**本项在这三个检查点里只有一件事**：交出可枚举的物理范围清单（§2.4 的扣减记录 + O-02a 的授权输入）。哪个叶子粒度、A/D 由谁更新、PMP/PMA 是否放行页表访问、临时映射的结构影响范围是否触及冻结字段——全部由 BootEntry 与分页侧专项承接，第九十三轮那张「激活前后修改路径」读数表已随重划移入 `03` 六十五节（逐字保留，作为该专项的输入，不再是本项验收条件）。

**冻结字段本身（VA 覆盖、PA 对应、页大小、有效权限、内存属性、共享性与 `MutableMappingMetadata`）属 T1/T2 的合同内容，随射程重划整体移入 `03` 六十五节**——本项不裁决「哪种修改算破坏冻结」，只保证交接清单里每个物理范围的来源可枚举。

**判据**：每个时点能列出「必须已完成的审计」与「允许的硬件更新」。
**第一个消费者**：根切换代码（今天只有一次根建立，没有切换）。



<!-- 自正文 O-07 的 R20 判据与完整性来源 移入 -->

### O-07 R20 的「实际访问需求」与完整性来源

**问题**：R20 检查「禁止访问集合」与「实际访问需求」不相交，但后者没有定义；而且静态清单本身不能证明「没有遗漏的访问」。
**失败形态**：某对象用未登记的裸指针访问了 MRB 区域；清单自身完整，R20 通过，禁令形同虚设。
**候选**：

- **A**：从对象登记表与使用点派生 `StartupObjectAccessRequirements`（对象身份、阶段、范围、路径、读写/取指），第一版是封闭静态清单；**并追加一条完整性来源**：受控访问包装、编译期可检查的访问 API、或明确承认裸指针不在证明范围。
- **B**：把 R20 限定为「已登记访问点」，并在合同里写明证明边界。

**判据**：每条实际访问点能指回登记项；R20 的通过条件写明它证明了什么、没证明什么。**第一版取 A 的渐进形式**：封闭清单 + 代码审计或 API 约束提高覆盖率；完整性未证明前，合同范围只覆盖已登记访问点，**不得宣称系统没有访问 MRB**——裸指针、`copy_nonoverlapping`、`volatile`、内联汇编与硬件隐式访问都可能落在清单外。
**第一个消费者**：对象登记表。



<!-- 自正文 O-08 的端口地址域候选与判据 移入 -->

### O-08 端口 I/O 的地址域（已裁定：采用 A）

端口范围用独立的半开区间与 checked 运算，不做物理页量化、不进 PMM、不进页表、不进 MMIO 记录、不进物理地址跨界 ABI；`PortWindowPolicy` 与端口访问需求都是编译期静态输入。确需跨界时另立 ABI（候选 B，暂不设计）。**判据**：端口与物理地址在任何集合与取整函数里都不混用。**消费者**：端口访问审计。



<!-- 自正文 O-10 的负控展开 移入 -->

### O-10 R13-A 的第二枚负控（已裁定：采用 A）

逐个登记允许出现原始操作的函数，检查函数体内原始操作的数量与位置，新增原始操作必须显式更新审计合同。**两枚负控**：① 在已批准函数体内注入第二条原始操作；② 在**已有 `asm!` 块内**加入第二条 `in`/`out`——审计器必须看展开后的真实操作，不能只数宏调用次数。**判据**：该负控生效。**消费者**：CI 审计器。

## 七十二、第一百零六轮：来源记录跨第一跳与 FREE_MEMMAP 清理的代码证据

### 线上协议（wire v2）

- 信封：`BootHandoffV1` 168 → **176 B**（新增 `sources_byte_len`，偏移 168），`ENVELOPE_CAP = 256` 不变；`BOOT_HANDOFF_V1_VERSION` 1 → **2**，`decode_envelope` 按精确相等比较（`HandoffV1ReadError::UnsupportedVersion`）。
- 分区：`WIRE_SOURCES_OFFSET = SEMANTIC_END = 14016`，`WIRE_SOURCES_CAP = 40`，`WIRE_SOURCE_RECORD_SIZE = 48`，`WIRE_SOURCES_END = 15936`，`ARENA_CAP = 16384`（余 448 B）。编译期门：`size_of::<WireSourceRecord>() == 48`、`align_of == 8`、`WIRE_SOURCES_OFFSET % 8 == 0`、`WIRE_SOURCES_END <= ARENA_CAP`、`offset_of!(BootHandoffV1, sources_byte_len) == 168`。
- 记录布局：`start u64 | end_lo u64 | end_hi u64 | id u16 | kind u8 | origin_tag u8 | a u16 | b u16 | cut u64 | reserved u64`。`end_hi ∈ {0,1}`；编码器恒写 0 的 `reserved` 非零即 `BadSourceRecord`；未知 `kind`/`origin_tag` 同判。写侧 `write_sources` 超容量返回 `TooManySourceRecords`，不截断。

### 消费者边界

- `KernelInfo.memmap_sources: &'static [SourceRecord]`；`KernelInfo::validate` 第 6 步（非空时）重跑 `audit_source_coverage(memmap, reserved_regions, memmap_sources)`，失败 `panic!("KernelInfo: source records do not cover the projected map: …")`；另有 `source_audit()` 返回 `Option<SourceAuditError>` 供诊断。
- 三条 DTB 腿：`bootface.rs` / `bootface_a64.rs` 直接赋 `kernel_info.memmap_sources = sources`；`opensbi_helpers.rs` 的 `prepare_boot` 把记录放进 `BootPrepareResult.kernel_info`，`uboot_main` 调 `write_sources`，接收端 `arch_boot_resume_v1` 调 `decode_sources` 落进 `globals::SOURCE_RECORD_STORE`（40 × 56 B 的 `.bss` 落地垫，已进 `BklProtected` 白名单）。
- UEFI 腿仍为空：`memmap_sources: &[]`，`validate` 跳过该步。

### 测试读数（本提交，docker `minix-ci:1.94`，`-j 1`）

```
$ cargo test -p minix-boot             → 150 passed; 0 failed
$ cargo test -p boot-shim              → 70 passed; 5 passed（集成）; 0 failed
$ cargo test -p minix-kernel --features mock → 863 passed; 0 failed; 8 ignored
```

新增用例：`decoded_payloads_survive_arena_poison`、`round_trip_sources`、`rejects_sources_over_capacity`、`rejects_source_record_with_nonzero_reserved_word`、`test_validate_accepts_covering_source_records`、`test_validate_rejects_span_without_source_record`、`test_bootstrap_frames_are_recorded_per_region_not_per_frame`。

### 链接期 `KERN_SIZE` 断言（rust-lld 双例实测）

两个自引导脚本（`os/kernel-image/riscv64.ld`、`aarch64.ld`）尾部：

```
ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000,
       "kernel-image memsz exceeds KERN_SIZE (0x400000) — update KERN_SIZE in src/bootface.rs / bootface_a64.rs")
```

用 `rust-lld -flavor gnu -T` 对同一脚本做两例（`.bss` 1 KiB 与 5 MiB）实测：小镜像 `SMALL_LINK=0`；大镜像输出 `rust-lld: error: kernel-image memsz exceeds KERN_SIZE (0x400000) - update KERN_SIZE`。第一版表达式误用 `KERNEL_PHYS_BASE`（VMA 减 PA 溢出）被该实测当场抓出，改为 VMA 基准。

### `FREE_MEMMAP` 删除的读数

删除前 `grep -rn "FREE_MEMMAP" --include=*.rs os/` 的全部命中：`globals.rs` 定义一处、`lib.rs` 五个写点（Phase A.2 初始化/模块切分、Phase F、两处 VM 模块回收）、四条文内提及。**零读取点**。删除后同 grep 仅余文档性提及（已同步改写为真实消费者）。

### 明确未验证

- `kernel-image`（aarch64/riscv64）与 `boot-shim`（riscv64）交叉构建：本机 CI 镜像只安装 `x86_64-unknown-linux-gnu` target，`cargo build --target …` 报 `E0463`（找不到 `core`）；本轮改动只在 faces 中新增两处字段赋值与一处 `mut`，类型核对依赖 `build_kernel_info` 的宿主编译与字段声明。
- QEMU 腿：镜像无 qemu，未运行。

## 七十三、第一百零七轮：消费者边界收口与配置表 DTB 三态的代码证据

### VM 侧删除平台地址硬编码

删除前 `os/servers/vm/src/boot.rs` 的 `read_boot_params`：

```
let root_deduct_base = 0x8200_0000;
let root_deduct_end   = 0x8400_0000;
let mut deduct: Vec<(u64,u64)> = vec![(root_deduct_base, root_deduct_end)];
for m in handoff.modules { ... deduct.push((m_base, m_end)); }
```
`deduct` 在 `reconcile` **之前**把空闲表切成片段，因此 `reconcile` 的第 4 条（`free ∩ deducted = ∅`）只能看到“已被修剪过”的空闲集：真正的记账缺陷会被静默吞掉。`0x8200_0000` 是 riscv64 QEMU 的 `MODULE_REGION_BASE`；aarch64 DRAM 从 `0x4000_0000` 起，该字面量落在普通 RAM 内。

删除后：`adopt_free_regions(&handoff.free_regions[..count])` 原样映射为 `BootMemRegion`，`reconcile` 保持四条判定不变。新增 `test_adopt_free_regions_keeps_every_region_verbatim` 钉住“0x8200_0000 的区间原样保留”。`minix-vm` 548 通过 / 0 失败。

### aarch64 UEFI 配置表 DTB 三态

新函数 `os/boot-shim/src/uefi_helpers.rs::cross_validate_config_dtb`，两个 UEFI 入口各调一次：

- 内联内核路径：`build_memmaps()` 与两条 `assert_bootstrap_outside_memmap` 之后；
- 跨镜像路径：EBS 分类产出的 `memmap` 之后、`assert_bootstrap_outside_memmap` 之前。

门序与 OpenSBI 腿一致：`dtb_blob_aligned` → `dtb_len_ok(total: u32)` → `checked_add` → `fdt_layout::validate` → `Fdt::new` → `DtbMemoryMap::new(&fdt, 0).collect()` → `dtb_covers_conventional`（既有谓词，自身有单测）。分支：

| 状态 | 动作 |
|---|---|
| 配置表无 DTB（x86-64 恒为此态） | 打印 “platform DTB absent — UEFI map is the only source”，返回 |
| 有 DTB 且可解析 | 覆盖通过：打印 “cross-validated”；有跨度不被覆盖：`panic!` 并报出 `[base,+len)` |
| 有 DTB 但不可用 | 对齐／长度／容器／解析／图拒绝各自 `assert!`/`expect` 停机 |

`cargo check -p boot-shim --features uefi` 通过；`cargo test -p boot-shim --features uefi` 70 + 5 通过。**QEMU 腿未跑**：第三态与第二态的“覆盖失败”分支只有真机或 CI 的 aarch64 UEFI 腿会执行。

### 明确未验证

QEMU（本机镜像无 qemu）；kernel-image / uboot 交叉构建（镜像只装 host target）。

## 七十四、第一百零八轮：第二跳交接页合同与测试的代码证据

### 新增测试（`os/libs/minix-types/src/types/boot.rs`）

| 用例 | 断言点 |
|---|---|
| `test_handoff_validate_accepts_current_version` | 现行版本 + 对齐根页 + 零计数 → 通过 |
| `test_handoff_validate_rejects_bad_magic` | `magic` 不符 → panic（消息含 bad magic） |
| `test_handoff_validate_rejects_wrong_version` | `version + 1` → panic（精确比较，不向后兼容解释） |
| `test_handoff_validate_rejects_unaligned_root` | `root_paddr = 0x90_0001` → panic |
| `test_handoff_validate_rejects_over_capacity_counts` | `free_region_count = MAX + 1` → panic |
| `test_handoff_validate_rejects_over_capacity_deductions` | `deducted_count = MAX + 1` → panic |
| `test_handoff_validate_rejects_over_capacity_ident` | `kern_ident_count = MAX + 1` → panic |

### 编译期守卫

```
const _: () = assert!(VM_BOOT_HANDOFF_VA < (1 << 38), "...Sv39 user-VA cap...");
const _: () = assert!(KERNINFO_USER_VA   < (1 << 38), "...Sv39 user-VA cap...");
```

2^38 是 Sv39 用户虚拟地址上限，同时也是三架构里最严的一个（x86-64 四级页表与 aarch64 48 位 VA 的用户上限都更宽）。

### 读数

`cargo test -p minix-types` **339 通过 / 0 失败**。仅宿主测试，无 QEMU 依赖。

## 七十五、第一百零九轮：UEFI 逐描述符来源记录（代码证据）

### 容量与布局

```
ARENA_CAP = 7 * 4096 = 28672
WIRE_SOURCES_CAP = 256          // = MAX_MAP_DESCRIPTORS
WIRE_SOURCE_RECORD_SIZE = 48
WIRE_SOURCES_OFFSET = SEMANTIC_END = 14016
WIRE_SOURCES_END = 14016 + 256*48 = 26304      // 编译期断言
spare = 28672 - 26304 = 2368
```

### 生产点（`os/boot-shim/src/ebs_map.rs::Plan::commit`）

| Disposition | 普通表条目 | 来源记录 |
|---|---|---|
| `Allocate` | `push_memmap` | `SourceKind::Memory`，origin = 描述符索引 |
| `ReservedMapped` | `push_reserved` | `SourceKind::ReservedOrdinary`，origin = 描述符索引 |
| `Side` | 侧表 | 无（不进入两份投影） |
| `Excluded` / `DevicePath` | 无（`len == 0`） | 无（不产出投影字节） |

记录范围 = `MemoryRegion { base, len }`，即分类器实际发出的区间；预检 `classify()` 在写之前检查 `conventional + reserved <= WIRE_SOURCES_CAP`，超限返回 `TooManySourceRecords`。重试时 `Serializer::reset_map_partitions` 同时清空来源分区与 `sources_byte_len`。

### 测试

| 用例 | 断言 |
|---|---|
| `source_records_cover_the_classified_map_per_descriptor` | 3 描述符 → 3 记录；origin 0/1/2；kind Memory/ReservedOrdinary/Memory；`audit_source_coverage` 对两份投影通过 |
| `source_partition_resets_between_ebs_attempts` | 首次 EBS 被拒、第二份地图仅 1 条 LOADER → 1 条记录，origin 0，range 0x30_0000 |

读数：`minix-boot` 152、`boot-shim --features uefi` 72 + 5、`minix-kernel --features mock` 863 / 0 失败 / 8 忽略。

### 明确未验证

QEMU 腿（本机镜像无 qemu）：真实固件的描述符数量、`Excluded`/`DevicePath` 的实际分布与 256 上限只有在 aarch64/x86 UEFI 腿上执行；三架构交叉构建同样未在本机重跑。

## 七十六、第一百一十轮：I-29 差异评审记录（537f5c16b..36fb62c35）

### 结论

| 级别 | 数量 | 内容 |
|---|---|---|
| P0 | 0 | — |
| P1 | 2 | ① `ARENA_CAP` 4→7 后四处注释仍写 four-page（`uefi_helpers.rs` ×1、`kernel/src/lib.rs` ×3）；② 教学构件承载门判 §2 为 P1 承载缺码（分析章无代码）。两者本轮已修。 |
| P2 | 2 | ① `WireSourceRecord` 的 `reserved: u64` 尾字当前恒零（协议预留，占 8 B/条）；② UEFI `ReservedMapped` 统一映射为 `SourceKind::ReservedOrdinary`，未按固件类型细分（记录的是“角色”，细分类随类型政策）。均登记不修。 |

### 代码维度结论（gate-evidence-code）

- **错误处理**：新增路径全部失败关闭——wire 解码对未知标签/非零保留字返回 `BadSourceRecord`，超容量返回 `TooManySourceRecords`/`DescriptorOverflowsPartition`；内核侧 `KernelInfo::validate` 覆盖审计失败 panic；配置表 DTB 的四种不可用形状各自停机。无静默截断。
- **资源**：arena +3 页 `LOADER_DATA`（UEFI 腿）／+3 页 bump（U-Boot 腿，该分区不写）；内核 `.bss` +8 KiB（`SOURCE_RECORD_STORE` 256 条）。均固定大小、无堆。
- **并发**：`SOURCE_RECORD_STORE` 为 write-once-then-read，已进 `BklProtected` 白名单；写侧单线程引导。
- **测试覆盖**：新增 11 枚用例（wire 往返/超容量/坏记录、validate 覆盖与拒绝、空闲表原样采纳、UEFI 逐描述符与重试清空、交接页七枚失败语义）；未覆盖分支仅剩“需要真实固件”的两条（配置表 DTB 交叉校验、描述符分布）。
- **文档同步**：四处注释与主文档/02/03/todo 同改；`doc-style-lint --diff` error 0。

### 复跑读数

`minix-types` 339、`minix-boot` 152、`boot-shim --features uefi` 72+5、`minix-kernel --features mock` 863 / 0 / 8、`minix-vm` 548；`boot-shim --features uefi` 编译通过。

## 七十七、第一百一十一轮：QEMU 全套件上机读数（四条启动链）

### 环境

`qemu-system-{x86_64,aarch64,riscv64}` 8.2.2（Debian 1:8.2.2+ds-0ubuntu1.18）、AAVMF、OpenSBI、`u-boot-qemu`、`dtc`/`fdtput`、`mtools`/`dosfstools`、宿主 `rustc 1.94.1`（与 CI 镜像同版本，`x86_64-unknown-uefi` / `aarch64-unknown-uefi` / `aarch64-unknown-none` / `riscv64gc-unknown-none-elf` / `x86_64-unknown-none` 均已安装）。

### 交叉构建

```
cargo build -j 1 -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none --release       ✅
cargo build -j 1 -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none --release ✅
boot-shim 三目标由 QEMU 脚本与 xtask image 构建链覆盖                                                      ✅
```

### 单腿读数

| 脚本 | 结果行 |
|---|---|
| `test-kernel-image-riscv64-bootface.sh` | `### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###`（B1/B2/B3 全 PASS） |
| `test-riscv64-uboot-shim.sh` | `RESULT: PASS (D-73 U-Boot → boot-shim → kernel-uboot full chain: 6/6 markers, entering scheduler)` |
| `test-shim-bootmarks-aarch64.sh` | 两行路标 + `platform DTB absent — UEFI map is the only source` + `arena=0x5c336000 conv=13 res=20` |
| `test-shim-bootmarks-x86.sh` | 两行路标 + `platform DTB absent` + `arena=0x1dfd7000 conv=13 res=113` + `entering scheduler` |
| `test-sysboot.sh` | `RESULT: PASS (multi-process boot + first user↔user IPC on real machine — S42 ④/C-27)` |
| `test-e5-ds.sh` | `E5f-LABELS 12/12` + `RESULT: PASS (DS 传输链四腿真机通电…)` |
| `test-riscv64-boot-full.sh` | `### TEST_RESULT: PASS riscv64 full boot (marker reached) ###` |

### 全量套件

`run_all.sh`：**38 passed / 1 failed / 0 skipped / 0 build failures**。唯一失败：

```
### TEST_RESULT: FAIL test-rt-birth-aarch64 ###
[svc] fatal: not an SVC ec=0x24 esr=0x92000047 elr=0x140003628 far=0x7ffffffff000
```

**基线对照**：在会话起点 `537f5c16b` 的独立工作树重跑同一脚本，得到逐字同签名（同 `elr`/`esr`/`far`）→ 既有红，不是本轮改动引入。该测试此前曾在全量套件中全绿，历史见 `rewrite-notes/coordination/PARALLEL-20261018-C-KERN-QEMU.md`。

### 覆盖边界（未走到的分支）

- 配置表 DTB「存在且可解析」正例与「存在但坏」负例：AAVMF 本机不提供配置表 DTB（日志逐字 `platform DTB absent`），三态只执行了「不存在」；
- UEFI 真实固件描述符数达到 256 上限：容量由宿主测试与写前预检覆盖；
- `Excluded`/`DevicePath` 的真实分布：不产记录，宿主测试覆盖计数。

## 七十八、第一百一十二轮：配置表 DTB 三态的固件侧取证

### 配置表 GUID 转储（aarch64 AAVMF 2024.02，默认 ACPI 模式）

`find_platform_sources` 的临时探针（已撤除）打印了 `with_config_table` 的全部 8 条项。按固件顺序，各条 Guid 的 `time_low` 原始值为：

```
4229680560, 95237306, 2000286284, 1276707999,
1226124919, 4076672324, 3707408669, 2288576625
```

最后一条 `2288576625 = 0x8868e871` 即 ACPI 2.0 的 `EFI_ACPI_20_TABLE_GUID`；其余为厂商表。**没有任何一条的 `time_low` 是 `0xb1b621d5`**，即没有 `EFI_DTB_TABLE_GUID`（`b1b621d5-f19c-41a5-830b-d9152c69aae0`），所以默认腿的日志是 `platform DTB absent — UEFI map is the only source`。**这是 EDK2 `ArmVirtPkg/PlatformHasAcpiDtDxe` 的行为**：ACPI 可用时发布 ACPI 表、不发布 DTB 表；ACPI 不可用时反过来。

### 强制 DT 模式

`-machine virt,gic-version=3,acpi=off` 下，配置表里的第 7 项变为
`Guid { time_low: 2981503445 (0xb1b621d5), time_mid: [156, 241], … node: [217, 21, 44, 105, 170, 224] }`，
即 `EFI_DTB_TABLE_GUID`；shim 随后打印：

```
boot-shim: platform DTB cross-validated against the UEFI map
```

核在该模式下继续走到 rc 与 init（日志尾部可见 `/etc/rc` 与 `pid=… sys=y`），说明交叉校验后的启动路径完好。

### 固化为测试腿

`os/qemu-tests/test-shim-bootmarks-aarch64.sh` 新增 `DTB_CFGTABLE=1` 旋钮：机器参数追加 `,acpi=off`，判定追加 `platform DTB cross-validated against the UEFI map`。两条腿现跑：

```
$ bash test-shim-bootmarks-aarch64.sh                  → PASS（absent 分支）
$ DTB_CFGTABLE=1 bash test-shim-bootmarks-aarch64.sh   → PASS（cross-validated 分支）
```

### 未覆盖

「存在但不可用」的负例：固件发布的 DTB 由其自身构造，外部无法注入损坏副本；该分支只有宿主单测与代码审阅覆盖。

## 八十、第一百一十四轮：复审后全面回归读数

### 宿主

```
cargo test -p minix-types                 → 339 passed / 0 failed
cargo test -p minix-boot                  → 163 passed / 0 failed
cargo test -p boot-shim                   → 82 passed / 0 failed
cargo test -p boot-shim --features uefi   → 82 passed / 0 failed（+5 集成）
cargo test -p minix-kernel --features mock → 863 passed / 0 failed / 8 ignored
cargo test -p minix-vm                    → 548 passed / 0 failed
```

### 交叉构建（宿主 rustc 1.94.1）

```
kernel-image --target aarch64-unknown-none --features fw-aarch64-none --release        ✅
kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none --release  ✅
boot-shim --target x86_64-unknown-uefi --no-default-features --features fw-x86-uefi     ✅
boot-shim --target aarch64-unknown-uefi --no-default-features --features fw-aarch64-uefi ✅
boot-shim --target riscv64gc-unknown-none-elf --no-default-features --features fw-riscv-uboot ✅
```

### QEMU（QEMU 8.2.2）

```
run_all.sh → 38 passed / 1 failed / 0 skipped / 0 build failures
            唯一失败 test-rt-birth-aarch64（与基线 537f5c16b 同签名，既有红）

test-shim-bootmarks-x86.sh                    → PASS
test-shim-bootmarks-aarch64.sh                → PASS（platform DTB absent）
DTB_CFGTABLE=1 test-shim-bootmarks-aarch64.sh → PASS（platform DTB cross-validated）
test-kernel-image-riscv64-bootface.sh         → PASS（B1/B2/B3）
test-riscv64-uboot-shim.sh                    → PASS（6/6 标记，进入 scheduler）
test-riscv64-boot-full.sh                     → PASS（rc 标记）
```

### 说明

- `DTB_CFGTABLE=1` 腿同时走 `resv_claims::capture`（`find_platform_sources` 内，EBS 前）与 `check_current`（分类后、EBS 前），即 O-11 类型覆盖合同在真实固件路径上被实际执行；默认 ACPI 腿无配置表 DTB，按空主张通过。
- 复审新增的 20 条单测（`minix-boot` 11 条、`boot-shim` 10 条，含 `resv_claims` 9 条）已在宿主读数中。
- 合并后 HEAD 复跑受影响包（另一线 concurrent 的 clippy 批次只做 import 归位与 rustfmt）：`minix-kernel --features mock` 863 / 0 / 8、`minix-vm` 548、`minix-vfs` 538、`minix-sys` 345，全部通过，确认无行为变化。

## 八十四、第一百一十八轮：wire v4 代码上的全面回归

### 宿主（本树现跑）

```
cargo test -p minix-types                 → 341 passed / 0 failed
cargo test -p minix-boot                  → 169 passed / 0 failed
cargo test -p boot-shim                   → 91 passed / 0 failed
cargo test -p boot-shim --features uefi   → 91 passed / 0 failed（+5 集成）
cargo test -p minix-kernel --features mock → 863 passed / 0 failed / 8 ignored
cargo test -p minix-vm                    → 548 passed / 0 failed
```

### 交叉构建（宿主 rustc 1.94.1）

```
kernel-image aarch64-unknown-none (fw-aarch64-none)            ✅
kernel-image riscv64gc-unknown-none-elf (fw-riscv64-none)      ✅
boot-shim x86_64-unknown-uefi (fw-x86-uefi)                    ✅
boot-shim aarch64-unknown-uefi (fw-aarch64-uefi)               ✅
boot-shim riscv64gc-unknown-none-elf (fw-riscv-uboot)          ✅
```

### QEMU（QEMU 8.2.2）

```
run_all.sh → 38 passed / 1 failed / 0 skipped / 0 build failures
            唯一失败 test-rt-birth-aarch64（与基线 537f5c16b 同签名，既有红）

test-shim-bootmarks-x86.sh                    → PASS
test-shim-bootmarks-aarch64.sh                → PASS（platform DTB absent）
DTB_CFGTABLE=1 test-shim-bootmarks-aarch64.sh → PASS（负例：
    boot-shim: EBS pairing loop failed: DtbPages(NotAcpiReclaim { page: 1206870016 })）
test-kernel-image-riscv64-bootface.sh         → PASS（B1/B2/B3）
test-riscv64-uboot-shim.sh                    → PASS（6/6 标记，进入 scheduler）
test-riscv64-boot-full.sh                     → PASS（rc 标记）
```

### 说明

- `DTB_CFGTABLE=1` 腿的语义在 A1 之后正式成为「存在但不可用」负例：EDK2 把机器 DTB 发布在非 `EfiACPIReclaimMemory` 页，`dtb_copy` 的页类型预检在 `ExitBootServices` 前拒绝启动。「存在且可解析」的正例只剩宿主单测（`resv_claims` 与 `dtb_copy` 的单元测试）。
- 启动对象分区（16 槽）不在 `reset_map_partitions` 的清空范围内，EBS 重试不影响它；UEFI 腿恰好写满 16 条（内核 + 12 模块 + 根页 + bump + arena），超限两道断言分别在生产侧与序列化侧。

### 评审身份（第一百零二轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `29cee8cb31f503fc0f0df59d972b4494631fc9e7`（第一百零二轮合并提交） |
| `CodeEvidenceCommit` | `3683a34ca`（第一百零一轮代码切片；本轮为纯文档） |
| `DocumentRevision` | 第一百零二轮 |
| `DocumentFileHash`（正文 sha256） | `9ecccee0c2118c748a4520c44cb65eea2df79a99ee32776a69a340db93e9b607` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `8062200785f89337fea2c9725406cd85cfaaf4c053220bee6e19235ccba2b4e7` |
| 上一轮（第一百零一轮）合并提交 | `3683a34ca`，正文 `3eac16ae35…`、档案 `7cbfece6cd…` |

### 评审身份（第一百零三轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | `14760c794aa93d0f04676200539fd7ddf8ca6c0e`（第一百零三轮合并提交，含代码） |
| `CodeEvidenceCommit` | `14760c794aa93d0f04676200539fd7ddf8ca6c0e`（146/70/332/861 与两条交叉构建均在本提交上取得） |
| `DocumentRevision` | 第一百零三轮 |
| `DocumentFileHash`（正文 sha256） | `5165abb556ed1091556d83561e615817bca8f1fcb4b7feed861def8dbee45145` |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | `61d0d60b80f1a06f3d850ef70cadf2728950815799c6669a4097d8af2fa8d6cc` |
| 上一轮（第一百零二轮）合并提交 | `29cee8cb3`，正文 `9ecccee0c2…`、档案 `8062200785…` |

### 评审身份（第一百零六轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十三笔登记（代码提交 `6c2a26abb`、清理提交 `c3208001e` 为其父链） |
| `CodeEvidenceCommit` | `6c2a26abb`（150/70/863 与 rust-lld 双例均在此提交取得） |
| `DocumentRevision` | 第一百零六轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百零五轮）合并提交 | `7e37ef58c` |

### 评审身份（第一百零七轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十四笔登记（代码提交 `d71db61d5`、`5acb4dbb6` 为其父链） |
| `CodeEvidenceCommit` | `5acb4dbb6`（minix-vm 548 与 boot-shim --features uefi 70+5 在此提交取得） |
| `DocumentRevision` | 第一百零七轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百零六轮）合并提交 | `3fef6483f` |

### 评审身份（第一百零八轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十五笔登记（代码提交 `668f942c8` 为其父链） |
| `CodeEvidenceCommit` | `668f942c8`（minix-types 339 在此提交取得） |
| `DocumentRevision` | 第一百零八轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百零七轮）合并提交 | `b1b96feff` |

### 评审身份（第一百零九轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十六笔登记（代码提交 `42af28d01` 为其父链） |
| `CodeEvidenceCommit` | `42af28d01`（152 / 72+5 / 863 在此提交取得） |
| `DocumentRevision` | 第一百零九轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百零八轮）合并提交 | `fa894ca84` |

### 评审身份（第一百一十轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十七笔登记（修正提交 `d4746c044` 为其父链） |
| `CodeEvidenceCommit` | `d4746c044`（本轮为注释与文档修正，读数沿用 `42af28d01`） |
| `DocumentRevision` | 第一百一十轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百零九轮）合并提交 | `36fb62c35` |

### 评审身份（第一百一十一轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十八笔登记 |
| `CodeEvidenceCommit` | `4567ed736`（QEMU 38/1/0/0 与交叉构建在此提交的树上取得） |
| `DocumentRevision` | 第一百一十一轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百一十轮）合并提交 | `c2d767d4a` |

### 评审身份（第一百一十二轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百一十九笔登记 |
| `CodeEvidenceCommit` | 本轮测试腿提交（`test-shim-bootmarks-aarch64.sh` 旋钮 + 文档） |
| `DocumentRevision` | 第一百一十二轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百一十一轮）合并提交 | `e6a6eb724` |

## 七十九、第一百一十三轮：I-29 实现复审修复的读数（代码证据）

**范围**：`acd1302d3..7a464dc82` 的 I-29 实现（四条启动腿、来源记录、两跳交接）双轴复审后的修复提交：`6b4f5d1f4`、`c19c03fd0`、`1c05632f0`、`54aeb74ad`、`4c004e861`、`b5921d616`、`b4dc9e8ce`。

**宿主读数（本机 rustc/clippy 1.94.1）**：

```
$ cargo test -p minix-boot --lib                 → 163 passed / 0 failed
$ cargo test -p boot-shim --lib                  → 82 passed / 0 failed
$ cargo test -p minix-kernel --features mock     → 863 passed / 0 failed / 8 ignored
$ cargo test -p minix-types --lib                → 339 passed / 0 failed
$ cargo test -p minix-vm --lib                   → 548 passed / 0 failed
```

**交叉构建**：`cargo check --target {riscv64gc-unknown-none-elf, aarch64-unknown-none, x86_64-unknown-none, x86_64-unknown-uefi, aarch64-unknown-uefi} -p minix-boot -p boot-shim` 全 OK；`boot-shim --no-default-features --features fw-riscv-uboot --target riscv64gc-unknown-none-elf`、`fw-x86-uefi`、`fw-aarch64-uefi` 三组合 OK（`resv_claims` 跟随 `uefi` 特性门）。

**QEMU（代码变更后复跑）**：`test-kernel-image-riscv64-bootface.sh` PASS（B1/B2/B3）；`test-shim-bootmarks-x86.sh` PASS（conv=13 res=113）；`test-shim-bootmarks-aarch64.sh` PASS（conv=13 res=20，`platform DTB absent`）。

**新增负控/正控**：`dtb_truncated_memory_reg_is_rejected`、`dtb_zero_size_memory_reg_is_rejected`、`dtb_truncated_reserved_reg_is_rejected`、`dtb_zero_size_reservation_is_rejected`、`dtb_mrb_and_ordinary_reservation_overlap_is_rejected`、`dtb_mrb_and_reusable_reservation_overlap_is_rejected`、`dtb_mrb_and_reservation_on_the_same_page_conflict`、`dtb_mrb_and_no_map_reservation_are_compatible`、`accepts_exactly_the_collector_reservation_bound`、`test_validate_requiring_sources_rejects_an_empty_partition`、`test_validate_requiring_sources_accepts_covering_records`、`resv_claims::tests::*`（10 条，含几何溢出负控 `overflowing_descriptor_geometry_is_ignored`）。

**未取得读数**：`tools/check-clippy-ratchet.sh` 在本批基线 `7a464dc82` 上因无关的 `minix-service` 在制品（`E0063 missing fields io/irq/nr_io`，修复在主树未提交）编译失败，棘轮未出数；本批新增的 `resv_claims` 经 `cargo clippy -p boot-shim` 复核为零告警。

**复审自检追加（`fc7759e16`、`eaa7d55fd`）**：① `resv_claims::check` 对 `number_of_pages` 改 `checked_mul`，几何溢出的描述符（只可能来自 `Excluded`/`DevicePath`）视为不覆盖，补负控；② `DtbMemoryMap` 的静态存储是进程级的，`load_dumpdtb` 早先返回的 `'static` 切片会被后续采集覆盖——改为保存 blob、在打印点重新 `collect` 并立即消费；aarch64 UEFI 腿（含 `dumpdtb.dtb`）复跑 PASS。

**仍开放**：O-11「复制到自有缓冲再解析」；UEFI 真机固件证据（本机 AAVMF 默认不发布配置表 DTB）。

### 评审身份（第一百一十四轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百二十四笔登记 |
| `CodeEvidenceCommit` | `d09fff501`（复审修复后的代码；本轮回归在此树上取得） |
| `DocumentRevision` | 第一百一十四轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上上轮（第一百一十二轮）合并提交 | `6bb26349a`（第一百一十三轮为另一会话的复审批次 `d09fff501`） |

## 八十一、第一百一十五轮：A1 复制到自有缓冲与页类型预检（代码证据）

**提交**：`d858c05f3`（A1 代码 + 测试 + 腿改造）。

**实现**：`boot-shim/src/dtb_copy.rs` 三段式——`prepare`（8 字节 header + 自有缓冲，先于取图）→ `authorize_and_copy`（配对快照核对全部页 `EfiACPIReclaimMemory`，然后 memcpy）→ `owned`（只读副本）；`resv_claims::capture` 改从副本构建；`cross_validate_config_dtb` 只解析副本。配对循环新增 `PairError::DtbPages`。

**读数**：`cargo test -p boot-shim --lib` **88 passed**（dtb_copy 6 条）；`minix-boot` 163、`minix-kernel --features mock` 863；`x86_64-unknown-uefi`/`aarch64-unknown-uefi`/`riscv64gc-none(fw-riscv-uboot)` 构建 OK。

**QEMU**：aarch64 默认腿 PASS（`platform DTB absent`）；`DTB_CFGTABLE=1` 腿 PASS（负例：`DtbPages(NotAcpiReclaim { page: 1206870016 })`）；x86 UEFI 腿 PASS。

**证据降级**：配置表 DTB「存在且可解析」的正例不再有固件路径（AAVMF 强制 DT 不符合 UEFI 2.11 §4.6.1.3 的页类型要求），正例由 `dtb_copy` 页类型单测覆盖；「存在但不可用」负例由真机腿覆盖。

## 八十二、第一百一十六轮：A2 记录身份与 Attribute/VirtualStart（代码证据）

**实现**：`SourceRecord` 增 `input_object`（`DtbBootInput`/`UefiMemoryMap`）、`local_source`、`snapshot_id`、`attribute`、`virtual_start`；`WireSourceRecord` 48→72 B；`DescriptorRef` 增 `virtual_start`（EFI 偏移 16）；UEFI `Plan::commit` 写 MapKey 与描述符原值；`MapStorage` 保存快照 id；arena 7→9 页、`WIRE_SOURCES_END=32448`、版本 3。

**读数**：`cargo test -p minix-boot --lib` **165 passed**（+2：`round_trip_identity_fields`、`rejects_nonzero_snapshot_on_a_dtb_record`）；`boot-shim --lib` **89 passed**（+1：`records_carry_descriptor_identity`）；`minix-kernel --features mock` **863**；三组特性构建 OK。

**QEMU**：`test-shim-bootmarks-x86` PASS；`test-shim-bootmarks-aarch64`（`DTB_CFGTABLE=1` 负例）PASS；`test-riscv64-uboot-shim` PASS（6/6 markers，wire v3 跨第一跳）。

**追加（`address_domain`）**：记录增 `AddressDomain`（`Physical`/`Virtual`），wire 第 67 字节由保留位改为域标签，解码拒绝未知标签；v1 生产者全部写 `Physical`。minix-boot 165、boot-shim 89；x86 UEFI 与 aarch64 强制 DT 负例腿复跑 PASS。

**B2（容量边界正例）**：`accepts_exactly_the_descriptor_and_partition_caps`——恰好 256 条描述符（128 `CONVENTIONAL` + 64 `LOADER_DATA` + 64 `ACPI_NVS`）解析并分类为 `(128, 64, 64)`，同时钉住 `MAX_MAP_DESCRIPTORS`、`WIRE_MEMMAP_CAP` 与 `SIDE_CAP` 三个上界；boot-shim 90。

**B1（拒绝诊断记录）**：`DescriptorRejection { index, raw_type, physical_start, virtual_start, number_of_pages, attribute, failed }` 随 `MapError::RejectedDescriptor` 返回；新增 `a_refused_descriptor_is_named_in_the_error`，五条既有 Policy 断言改为匹配 `failed`；boot-shim 91。

### 评审身份（第一百一十七轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百三十笔登记 |
| `CodeEvidenceCommit` | 本轮代码提交（代码与文档同提交；宿主读数、交叉构建与 QEMU 腿在此树上取得） |
| `DocumentRevision` | 第一百一十七轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百一十六轮）合并提交 | 台账第三百二十六至三百二十九笔（A2 主体 `98ef0094a`、收尾 `133940b89`） |

## 八十三、第一百一十七轮：A3 启动对象记录独立分区（代码证据）

**实现**：`SourceRecord` 的种类枚举增 `SourceKind::StartupObject`（tag 6）、`SourceOrigin::StartupObject(u16)`（tag 5）、`InputObjectId::StartupObjects`（tag 2）；`startup_record(index, start, end)` 是 `const fn`，可直接初始化 `.bss` 静态数组。`BootHandoffV1` 增 `startup_byte_len`（偏移 176，信封 184 B）；`WIRE_STARTUP_CAP = 16`、`WIRE_STARTUP_OFFSET = 32448`、`WIRE_STARTUP_END = 33600`，`ARENA_CAP` 仍 9 页（余 3,264 B）；`BOOT_HANDOFF_V1_VERSION = 4`。写侧 `Serializer::write_startup` 与读侧 `decode_startup_objects` 成对；`reset_map_partitions` 不清该分区。内核侧 `KernelInfo.startup_objects` + `STARTUP_RECORD_STORE`（`WIRE_STARTUP_CAP` 槽），`arch_boot_resume_v1` 解码后赋值；`audit_source_coverage` 明确跳过启动对象记录。

**生产者**：`kernel-image/src/bootface.rs`、`bootface_a64.rs`、`boot-shim/src/opensbi_helpers.rs` 各写内核映像 + 每个模块（`.bss` 静态数组）；`uefi_helpers` 在 EBS 成功后写内核/模块/根页/bump/arena，并经 `uboot_main` 的 `write_startup` 跨第一跳。

**读数**：`cargo test -p minix-boot --lib` **169 passed**（+4）；`boot-shim --lib` **91 passed**；`minix-kernel --features mock` **863 passed / 8 ignored**；交叉构建 `kernel-image` aarch64/riscv64/riscv64-uboot 与 `boot-shim` x86_64-uefi/aarch64-uefi/riscv-uboot 全部 OK。

**QEMU**：`test-shim-bootmarks-x86` PASS；`test-shim-bootmarks-aarch64`（ACPI）PASS；`DTB_CFGTABLE=1`（DT 负例）PASS；`test-kernel-image-riscv64-bootface` PASS；`test-riscv64-uboot-shim` PASS（6/6 markers）。

**复审修复（同一轮）**：`uefi_helpers` 的 `push_startup` 在超容量时 `assert!` 停机（静默返回会丢掉最后几条，当前恰好是 arena）；三条 DTB 腿在写 `.bss` 静态数组前断言 `1 + modules.len() <= WIRE_STARTUP_CAP`，模块数增长不会越界。UEFI 腿当前记录数为 16（内核 + 12 模块 + 根页 + bump + arena），正好等于 `WIRE_STARTUP_CAP`。

### 评审身份（第一百一十八轮）

| 身份 | 值 |
|---|---|
| `ReviewedRepositoryCommit` | 合并提交在台账第三百三十二笔登记 |
| `CodeEvidenceCommit` | `c22f12954`（A1～B2 全部合入后的树；本轮回归在此取得） |
| `DocumentRevision` | 第一百一十八轮 |
| `DocumentFileHash`（正文 sha256） | 见合并后回填 |
| `03ArchiveFileHash`（本档 sha256，按合并提交） | 见合并后回填 |
| 上一轮（第一百一十七轮）文档提交 | `b6f715f3d` 之后的 A3 文档批次 |
