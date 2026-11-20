# 01-todo — 01 篇关联 todo 总台账

> **重写**: 归拢版（九轮评审终态后重制，取代 2026-10-20 版本）。

> **来源与方法**：本文件归拢 `01-boot-shim-bootstrap.md`（下称「01 篇」）射程内的**全部** todo 项——代码项与文档项都含，凡该篇 §7.3 关联代码清单与正文 TODO 标记涉及的条目一律在册。三个来源：
> ① **本会话九轮外部评审终态**：`01-code-design.md` v2→v9外部评审迭代至冻结；七份调查报告 `01-forensics-round3.md` 至 `01-forensics-round9.md`——**T1–T5 设计已冻结（v9）**，原「待裁决」条目全部拿到裁决，剩余工作收敛为实施、直修与文档同步；
> ② **stage 权威台账** `todo.md` §7（D/I/T 系条目的现状读数、修法、DoD、现算命令以其为权威，本文件只给摘要 + 指针 + 状态）；
> ③ **01 篇正文与 §5.3 交接表**（正文 TODO 标记与 TODO-01~07，唯一登记处在篇内，本文件管状态与批次）。**行号注记**：早期版本引用的 L1378/L1408/L1424/L1488/L1532 五个行号在现行文档（3582 行）里已随多轮重写失效，按符号锚点重新定位；其中 L1488/L1532 已按 D-068 关闭。
> **口径**：`minix3/` C 源为真值；`os/` Rust 实现的偏差分三类——**注释失真**（改注释即闭环）、**抽象收口**（设计已裁决、待动码）、**测试缺口**（补测试即闭环）。行号为登记日快照，动手前以符号锚点重新定位。

---

## 0. 使用说明（给代码迭代 agent）

1. **一次一条**（todo-fix 纪律）：每条自带现状与验收；动手前先跑证据命令复核（源码可能已前移）。
2. **裁决已全部完成**：§2 各条的设计载体（`01-code-design.md` v9）已冻结，**直接按裁决实施**，不再需要外部评审轮次；只有实现证伪某项既定前提时才重开对应局部裁决（v9 冻结条款）。
3. **每条做完的通用验收**：`cargo test -p <对应 crate>` 通过；涉共享契约层（`os/libs/minix-boot`）的条目加三架构 `cargo build` 与 qemu 引导测试（`os/qemu-tests/run_all.sh`）回归；受影响的 01 篇章节同批更新（Doc Code Sync）。
4. **硬约束**：`#![no_std]`（测试除外）；错误映射 Minix3 errno；注释全英文自包含（`os/` 下）；三档术语 `[ARCH: ...]` 三处一致。

---

## 1. 总览（全部条目 × 当前状态 × 去向）

| 条目 | 一句话 | 类型 | 当前状态 | 去向 |
|---|---|---|---|---|
| **T1（D-67 吸收 + D-69 同批）** | 跨镜像 wire 协议 + handoff arena + 方案乙配对循环 | ✅ 已实施（01TODO-T1CUT，2026-10-30） | v9 冻结；类型/序列化/分类/配对/解码/失败族/builder/getter 与 2c/3b 联合切根全部落地；aarch64 v1 arena 端到端到 `/etc/rc`，旧 blob 删除，run_all 39/0/0 | §2.2 |
| **T2（I-1/NK1/OQ-N6）** | x86 切独立镜像（入口契约全套已裁） | ✅ 已实施（01TODO-T2X86，2026-10-30） | 入口契约落地：`_start` 建栈/清 `.bss`/`cli`/`cld`/`rbx` 交接；`arch_boot_resume_v1` x86 分支；`reserved_regions` 恒等重放半退场；模块 blob 上界；x86 冒烟门 PASS，run_all 39/0/0 | §2.5 |
| **T3（D-66）** | `param_buf` 生产侧接入 | ✅ 已实施（01TODO-T3PARAM，2026-10-30） | 统一 normalizer + UEFI LoadOptions + DTB `/chosen/bootargs`；aarch64 直核读数 `bytes=0x15 first=ARCH=aarch64`，UEFI 腿 `bytes=13`（无 LoadOptions）；内核零改动 | §2.4 |
| **T4（D-68 + DTB 扣除）** | bootface_a64 第四链迁移 + DTB 永久扣除 | 设计已冻结 → ✅ 已实施（01TODO-B3） | 独立小批完成；`cut_memmap` 真实 totalsize 记账测试与 aarch64 直核冒烟在案 | §2.1 |
| **T5（D-70）** | 落地区补 `platform_sources`/`param_buf` | ✅ 已实施（01TODO-T5） | 五块落地区在位；容量 4/1024 与 fail-fast 测试在案 | §2.3 |
| **D-59** | 大页能力/选页策略四层抽象 | ✅ 五阶段全部实施（D59PAGE/D59SIZE/D59POLICY/D59DEFEND/D59RENAME；合并 4dc6a797f/8d99bb774/f9bab95c2/5479b8083） | 启动选页先能力后对齐、`PageSize`/统一能力 API、事实与纯规划拆分、预检式安装、`map_huge` 防御、机械重命名全部落地；末轮三架构 `run_all` 39/0/0。覆盖损失门按 §2.6 未关闭验收门独立立项，不随迁移闭合 | §2.6 |
| **D-72（本轮新登记）** | bump 池耗尽结构债（kerninfo 注入链每新根抽页、池无回收、真机 ~40 次绑定死亡） | ✅ 已实施（01TODO-D72VM，2026-10-30） | 映射责任移交 VM（候选 A）：`VmBootHandoff` v7 加 `kerninfo_pa`，VM 建根统一映射，内核侧注入块删除；源码序钉 2/0 | §2.7 |
| **D-73（本轮新登记）** | riscv64 独立 boot-shim 腿：三架构统一主线 = shim + 独立内核镜像，自引导面降为特定嵌入式形态（FW_PAYLOAD / `-kernel` 测试） | ✅ U-Boot → boot-shim → kernel-uboot 全链 PASS（6/6 路标到 entering scheduler）；直核腿不回归；门读数见 §2.8 实施记录 | 交付=`riscv64-uboot-shim`（物理链接 VMA=LMA=0x84500000，自带 bootstrap 栈与 MNXBOOT1 表自建）+ `kernel-uboot` 接收镜像（`fw-riscv64-uboot` 特性，`arch_boot_resume_v1` 加性恢复）；入口 ABI=`bootelf -p <img> <dtb_addr>` 的 `argv[0]`=DTB 物理地址（表由 shim 在 .bss 自建，避开 staging 共变问题）；三处实施期修正：jump_to_kmain 执行位置探针（接收腿不加链接偏移）、shim memmap `firmware_reserve_end`=0x80200000（bootface 同源，堵 OpenSBI 足迹漏进 VM 空闲表）、门脚本 build 失败不再被 `set +x` 掩盖；门=`test-riscv64-uboot-shim.sh` | §2.8 |
| **N-1~N-5（本轮新登记）** | `BootAlloc::resume` 无区间校验 / vm_handoff 头注释与实现不一致 / 零消费者便利包装 / bootface 双份名字池与常量 / 池页零化机制归因未定案 | 直修或取证 | N-1/N-2/N-3 ✅（01TODO-B1）；N-4 ✅（01TODO-B4）；N-5 ✅（01TODO-N5 探针：映射重放成立、复用前提未出现） | §3 |
| D-60 | `IDENTITY_MAP_END` 双定义 | 闭合 | ✅ 双定义归一为全仓单定义（回勾确认）；01 篇 §4.6 措辞随修（见 §5） | §3 |
| I-22 | `MemoryMapSource` trait 收口三链 | 代码已落地 | ✅ 代码落地；文档连带 ①–⑦ 已逐项核（⑤ bootface 头注随 01TODO-B2 修正）；遗留 = D-68（T4 批） | §3 |
| I-28（本轮新登记） | `MemoryMapSource` trait 无多态消费者（无泛型约束、无 `dyn`；两个 impl 无活消费者） | 设计裁决 | ✅ **closed（本批落码，grep 判据归零，真机门 `cd os/qemu-tests && bash run_all.sh` → 37 passed / 2 failed / 0 skipped / 0 build failures（本树现跑；两条红 `test-smp-aps`/`test-smp-ipi` 在同一台机器上用**未改动的 rewrite 尖子 `2b72a1e62` 重新构建同一载体**复现同一 `### PANIC … arch/src/x86_64/protection.rs:0x1c3`，判为既有、非本批引入））**：删除 trait 与空占位，保留 `DtbMemoryMap` 具体实现与内存图语义说明，正式 supersede I-22（supersede 记录属实施动作，随原型批执行） | §3 |
| I-29（设计） | UEFI 内存图分类双份实现；分类判据用地址代替类型；`Attribute` 未读且三架构缓存属性机制各不相同 | 设计裁决 + 测试改造 | ✅ **设计完成（v12 冻结）**：唯一解析/分类规则、零分配 + 预分配输出、`ram_top` 删除、方案 A、PA/DM 两合同、`DescriptorRef` 含 Attribute、side list 语义、五步裁决顺序（I-29-17）全部冻结。**本节只覆盖设计部分；实施见下一行 I-29-impl** | §3 |
| I-29-impl（实施中：门①③已落，门④路径 A 已落，门②待专项） | 按 v12 冻结设计把分类器落地并过四个验收门 | 代码实施 | **门④路径 A 已落**：`classify_map_bytes_into_test_tables`（唯一分类器抽出）+ `exit_boot_services_and_classify`（保留退出返回的 MemoryMapOwned、退出后分类）+ 12 条 UEFI 测试内核迁移到路径 A 流程；曾阻塞门链验证的 I-43 x86 载体 `enable()` 静默停已由 I-43 二层专项关闭（2026-11-20，hello-boot 新鲜二进制 PASS），门④端到端六项验收（§2.7）已可跑。详见 §3 的 I-29-impl 补记 | §3 |
| I-34（已实施） | `BootStorage` 半提交防护：删 `map_dirty`，改 `fill_*` 闭包 + `MapCounts` | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：六族 `fill_*` + `MapCounts`；`map_dirty` 与四个单表入口删除，25 处生产调用点重写；**五条 QEMU 门链全 PASS**。mini review 报出 `push_*` 死子系统（登记 I-42）。内容见 §3 的 I-34 | §3 |
| I-44（**实施中：步①–④已落，步⑤ E+R1 已关闭、R2 待真机**） | 页表页归还的所有权模型（原 `R35/DESIGN-D2-11`）：问题 C/D 成立、问题 B 撤回；设计收束为一个 `PagingState` 三态 + 五条入口断言 | 代码改造 | **设计已通过、可以实施（文档第十三版 §4ter 为基线）**：① 状态字段与构造函数迁移（含 `adopt_active_root` 合并、`SimPaging`）② `destroy(self)` 入口断言 ③ 继承路径 ④ 写入口范围保护 + 三架构测试 ⑤ 独立关闭 E / R1 / R2；**不得以编译通过替代生产安全验收** | §3 的 I-44（设计文档 `01-review-request-page-table-free.md` 第十三版） |
| I-42（已实施） | `BootStorage` 的 `push_*` 死子系统——**射程核实后只剩 `BootStorageFull`**（前两个符号已不存在） | 代码删除 | ✅ **已实施（2026 年 11 月 20 日，D-080 采 A）**：删 `boot_storage.rs` 的注释与 struct、`lib.rs` 的 re-export。否决 B（D-067 已定 `fill_*` 方向，留着就是两套可不一致事实）| §3 的 I-42 |
| I-43（已裁决·三层中两层闭合） | 编译失败修好后暴露的测试内核运行期缺陷：全零 12 槽 `BOOT_MODULES` 整组当 `boot_modules` 交出 | 测试载体修正 / 定位 | **D-081 采 A**（载体侧真实 span）。**一层已闭合**（12 载体全部 `PLACEHOLDER_SPAN` + 全 12 槽交出）。**二层已闭合（2026-11-20）**：`enable()` 后静默停的真因是双重的——载体硬编码 `kern_phys_base`/`kern_size` vs 固件运行期装载地址（修法：20 载体改用生产同款 `shim_image_window()`，LoadedImage 协议精确足迹），加上直核腿窗口清单漏了「调用者栈」类别（修法：`arch_boot_impl` 补压当前栈窗口，`shim_stack_window` 同型；`cfg(not(test))` 排除宿主假阳性）。**实证**：x86 14 条中 13 条 PASS（faultloop 的失败是其专用故障臂的深层缺陷、已现形）；aarch64 五条基础门全 PASS；riscv `hello-boot` PASS。三层残留（VM 加载 trap、深跑载体故障臂路由、rt-birth-riscv64 自身 bump 断言）各自需要专项、现已全部可诊断。另登记 I-45（宿主套件存量红）。详见 §3 的 I-43 | §3 |
| I-35（部分实施） | `KERNEL_INFO` 的 write-once 承诺与实际二次写（`kmain:1738` 冗余 store）+ `IPC_FILTER_POOL` 注释失真 | 设计裁决 + 注释修正 | 已裁决（D-071，用户于 2026 年 11 月 20 日）。**(b) 注释项已实施（2026 年 11 月 20 日）**：`IPC_FILTER_POOL` 的「Only written once during boot」改为「初始化一次、此后在 BKL 下可变」，并核对三处可变借用（`proc_table.rs` + `syscall.rs` ×2）。**(a) `KERNEL_INFO` 四步方案仍不实施**（用户明确要求）。第三项 `Frozen::freeze` 已由 I-32 销账。详见 §3 的 I-35 | §3 |
| I-36（已裁决·待实施） | `FileLoader` 抽象里藏着 ELF 语义（U-Boot 腿的 `read` 做修剪），腿间 `BootModule.len` 不同 | 设计裁决 | **已裁决（D-082，用户 2026 年 11 月 20 日）采「删重复 + 修剪上移」**：① 删 `read` 里的 `segment_iter`（实测输入的 `entry.len` 已是修剪值，纯冗余）② 修剪上移到 `load_boot_modules_into` ③ 复用 `minix_elf::segment_iter`，不新增第三个解析器。**否决「抽象修剪 trait + 空实现」**（无哪个腿不需要修剪）。② 落地前须验证 12 个模块的 PT_LOAD 覆盖真实内容 | §3 的 I-36 |
| I-45（已裁决·待实施） | `SegmentIter` 把畸形 ELF 静默当成"没有这个段"（五条畸形 `continue` 与一条正常过滤无法区分） | 代码改造 | **已裁决（D-084，用户 2026 年 11 月 20 日）梭哈 A**：`Item` 直接改 `Result<LoadSegment, ElfError>`，不走 B 过渡（无外部消费者、保留旧函数等于给缺陷留后门）；`filesz > memsz` 检查收进迭代器；`load_segments_into_buffer` 返回类型同批改 | §3 的 I-45 |
| I-46（待裁决·**被 I-45 阻塞**） | `load_segments_into_phys_memory` 的 Safety 前提无类型载体（函数不自查段表）+ UEFI 腿绕过 wrapper | 设计裁决 | **未裁决，且有硬性前置依赖**：实质风险会被 I-45（`SegmentIter` 改 `Result`）顺手消掉，故必须在 I-45 落地并验证后才能裁决；裁决时只剩三件事（删 SAFETY 第 2 条 / 文档失真重新定位 / UEFI 腿是否改 wrapper）。面板建议的 `ValidatedKernelImage` 与改 wrapper 形状两案均倾向于不做 | §3 的 I-46 |
| I-47（待裁决·**被 I-45 阻塞**） | `load_segments_into_buffer` 的计数与语义三处不一致（文档说 loaded、`:258` 无条件计数、`:235` 越界段不计数；且缺段间重叠检测） | 代码改造 | **未裁决，有硬性前置依赖**：与 I-45 在同一函数上重叠，I-45 改迭代器协议后本函数的 `match` 必须重写；裁决时按候选 A（返回改名 + 补重叠检测 + 文档改「测试投影」）。面板的 `copy_segment(seg, sink)` 抽象不采纳（覆盖不了整组重叠检测）| §3 的 I-47 |
| I-37（已实施） | `prepare_kernel_image_v1`/`jump_kernel_image_v1` 的裸 `u64` 对改 `PreparedJump` 值对象（接反后仍能编译，交换后果=取指故障） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：四步 + mini review 修正两处（`private_interfaces`、mock 孤儿）；五条 QEMU 门链全 PASS。验收与鉴别力双向验证见 §3 的 I-37 | §3 |
| I-39（已实施） | `KernelInfo` 的空 getter 段（注释承诺 getter、下面 12 行空白）与 `assert_eq!(x,x)` 恒真测试（17 条）+ 文档把它计入 31/32 覆盖 | 测试清理 | ✅ **已实施（2026 年 11 月 20 日）**：D-077 三项 + mini review 补拓两处文档计数（32→31、196→195）；**先于 I-31** 完成。验收与同源残骸登记见 §3 的 I-39 | §3 |
| I-38（已实施） | `boot_validate_and_prepare` 的位置实参改 `BootScalars` 值对象 | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：三字段 `BootScalars` + 两个命名构造函数；**I-31 已先落，故按三字段形态一次定型**。五条 QEMU 门链全 PASS。内容见 §3 的 I-38 | §3 |
| I-40（已实施） | 两处 ghost API 注释：`KernelInfo::boot_modules()`（`kernel_info.rs:583`）与 `KernelInfo::memmap()`（`vm_handoff.rs:108`）——`KernelInfo` 上没有这两个方法（同名方法只存在于 `BootStorage`） | 注释修正 | ✅ **已实施（2026 年 11 月 20 日，D-079 采 A）**：① 去掉 `()` 指字段 ② 补 `field` 后缀。否决 B（造访问器会制造两套等价 API）。第三处 `kern_stack_top()` 经核实**不是** ghost，已排除 | §3 的 I-40 |
| I-30（已实施） | aarch64 形态判别改链接期分叉（删 `arena_pa == 0` 运行期哨兵） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：新增 `fw-aarch64-uefi-recv` 特性、两条 `rust_image_main` 各归一个 cfg、xtask 盘面特性切换；接收臂的 `x0==0` 改为诚实停机。mini review 修掉 8 条死码；结构鉴别力已证（接收产物里编不进 `bootface_a64`）。验收见 §3 的 I-30 | §3 |
| I-31（已实施 + 三轮补漏） | wire v10：删 `syscall_entry` / `user_sp`+`kern_stack_top` / `startup_objects` 分区（`BOOT_HANDOFF_V1_VERSION` 9→10） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：六层同批落地（契约 9→10、信封 160→128、`KernelInfo` 14→10 字段、五处生产者、五处消费者、12 条测试删除 + 1 条等价负控替代）；**五条生产 QEMU 门链全 PASS**。**两个 mini review 补漏批**：① `uboot_main` bin 未编（`[[bin]]` 不在默认 check 面）；② **30 个 QEMU 测试内核全部编译失败**（同因 `required-features` 遮蔽，`run_all.sh` 实测当时 5 个 build failure）。两批均已落地并全数验证；**第三批（`minix-arch` 测试面）同日补修**，见 §3。验收读数与三处口径偏差见 §3 的 I-31 | §3 |
| I-32（已实施） | U-Boot 腿两个安装型全局：`debug_assert!` → `assert!`（含 D-071 并账的 `Frozen::freeze`） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：四处改动 + `tests/install_write_once.rs` 两条负控 + D04 四处文档回写；验收读数与鉴别力验证见 §3 的 I-32「实施记录」 | §3 |
| I-33（⚠ 改回待裁决） | 直核腿统一到 owned 路径，删 `DTB_*` 第二套静态表 | 设计裁决 + 代码改造 | **原裁决（D-065，用户 2026 年 11 月 20 日）已被实施受阻推翻**：实测发现第四张 `DTB_PANORAMA` 仍有生产消费者（`resv_claims.rs` 的 EBS 后 claim 捕获），且 `DtbMapOwned` 不带 panorama、测试改写面 62 处。三选一待裁决（A 扩权到 panorama / B 缩小为「只统一直核腿」/ C 拆成两项）。代码改动已全部回退——详见 §3 的 I-33 | §3 |
| I-30（本轮新登记） | 01 篇正文补「内存图生产的四层责任」概念讲解 | 文档教学批 | ✅ 已实施（R32 批，代理裁决）：§3.4 开头新增四层责任段（固件事务 → 描述符解析 → 语义分类 → 映射分配），政策细节句指向设计笔记 v12 对应节；涉及 I-29 政策矩阵批准的行文均标注"待 I-29 实施批同批校准" | §5 |
| D-71 | `BootForm::source_path` 悬空路径 | 注释/登记回修 | ✅ 已修（01TODO-B1） | §3 |
| I-26 | opensbi_helpers 孤儿注释 + "hardcoded" 顶注 | 注释回修 | ✅ 已修（01TODO-B1） | §3 |
| I-27 + L1424 | 01 篇 L1422/L2624「还没裁决」口径 + 张力指针 | 文档回修 | ✅ 已修（01TODO-B1；OQ-N6 口径与张力指针同批） | §3 |
| TODO-01~07 | 01 篇 §5.3 交接表七条测试缺口 | 测试缺口 | TODO-01/02/03/06/07 ✅；TODO-04 ✅（文本级，T1 接线后升级）；TODO-05 ⑤ 真机窗口不可得（QEMU-only 环境）；取证路径 = 真机 2 MiB 启动或 x86 `-cpu qemu64,-pse` 能力压制腿（另行立项，01-todo §4） | §4 |
| I-17/I-18/I-19 | 01 篇 §2.6 拆节 / 章头体检 / 讲解条续扫 | 文档结构 | ✅ 已改（01TODO-DOC） | §5 |
| I-23 / I-24 + L1378/L1408 | §3.7 fw_payload 段 / §3.1 否决表拆行 | 文档内容 | ✅ 已改（01TODO-DOC；设计侧 [ARCH] 镜像在主树工作区，未代提交） | §5 |
| L1488 / L1532 | 01 篇读者待办（`reserved_regions` 退场学习项 / 张力块性质澄清） | 读者侧 | ✅ 已关闭（D-068，用户 2026-11-20）：两项均是首次人工审阅文档时留下的待办；此后 review 流程已加入对抗性读者视角（P-READER A/B/C 独立冷读），讲解可读性由该机制持续覆盖，且相关代码形态已大幅变动（x86 已切独立镜像，`kern_ident` 已删除，旧内联窗口场景不复存在）。用户明确表示无需再理解旧设计 | §5 |
| I-25 | `os/` 全仓注释语言与锚点整改 | 代码注释 | ✅ 全仓完成：1,549 个 Rust 文件注释 CJK=0、广义锚点=0；654 个纯注释文件非注释 token 与基线零差异，另 1 个 `SimPaging` 测试替身按 D-59 补齐（合并 e754ea77a/9c9c24bbe/f933b53ab）。审计 `tmp/i25-final-audit.md`，构建 `run_all` 39/0/0、workspace 8,284 passed/0 failed | §5 |
| I-21 | 篇头「路径约定」重复上收 | 文档结构 | ✅ 已上收（I21HOIST 批，用户授权关闭） | §5 |
| I-20 | boot-shim 再导出注释 | 已闭合 | ✅（commit b0da9074e） | — |

---

## 2. 实施批次（设计已冻结，按既定次序 T4 → T1 → T5 → T3 → T2）

> 本节每条的**完整裁决内容**在 `01-code-design.md`（v9 状态收紧表 §10 + 各节）；本文件只给实施要点、验收锚点与现状复核命令。九轮评审的关键终态：wire 载荷走**单一 handoff arena**（信封宿主 + 语义工作分区双写 + 仅 `byte_len` 描述符 + `arena_len` 字段，独立 bump blob 页移除）；内存图权威走**方案乙**（`uefi::table::system_table_raw()` raw 配对循环，分类在 Boot Services 活着的规范明文窗口，MapKey 显式配对）；解码边界为**受控复制五步序列**（`KernelInfoView`/`BootReadPhase` 退役）；x86 入口契约全套已裁（镜像内 bootstrap 栈二段式、`rbx` 传 blob 指针、显式 `cli`/`cld`、red-zone 由目标规格闭合无需旗标、`load_elf_into` 机制前提归边界）。

### 2.1 批次一 —— T4（独立小批，最先）

**D-68：`bootface_a64` 第四条 memmap 链迁移**（权威：todo.md §7.1；裁决：01-code-design §4.3）

- 删 `os/kernel-image/src/bootface_a64.rs` 的 `fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE`，改调 `DtbMemoryMap::new(&fdt, 0).collect()`；`fw_end = 0` 带形态前提成文（该腿无固件常驻区于 DRAM），与内核侧 `cut_memmap` 两机制不替岗。
- **v5 定案增补**：DTB 永久扣除——Phase A.2 补一笔 `cut_memmap(dtb_pa, totalsize)`（与 D-64① 内核镜像自切同型，损耗 ≤ 2 页）；**腿侧无需预对齐**（round-5 C 批核实：`cut_memmap` 自动页化扩展 `[floor_P(start), ceil_P(start+len))`，带 saturating/回绕保护，非页对齐测试已在库 `test_cut_memmap_alignment_*`）。
- 验收：aarch64 target 构建通过；直核冒烟；`grep -n "MEMMAP_STORAGE\|clip_firmware" os/kernel-image/src/bootface_a64.rs` 归零；DTB 扣除端到端检查（真实 `dtb_pa + totalsize` 传入后记账与预期页区间一致）。**状态**：✅ 01TODO-B3——`MEMMAP_STORAGE`/`clip_firmware`/`fill_memmap` 三归零；Phase A.2 Step 4 用 `dtb_blob_len`（8 字节头）取真实 totalsize 后 `cut_memmap`；宿主测试用真实 QEMU blob（7699B @ 0x4000_0000）断言切除后自由段 `[0x4000_2000, +512MiB-0x2000)`；aarch64 kernel-image（SD-23 RUSTFLAGS 门）0 error；`test-smp-aps-aarch64.sh` 直核全链 PASS（ap-arrived=1）。

### 2.2 批次二 —— T1（wire 协议 + arena + 方案乙；九轮评审的主体的落地）

**T1 实施拆步（裁决载体：01-code-design §1.3/§1.4/§1.5（四）（六）（七）/§9.2–9.4）**：

1. **协议类型层**：`BootHandoffV1` + `Wire*` 元素类型（`WireMemoryRegion`/`WireBootModule` 16B 定长名槽/`WirePlatformDescSource`）；信封内切片描述符**仅 `byte_len: u64`**（`WireSlice{addr,byte_len}` 已在 v8 退役——固定分区布局下地址由 arena 基址 + 协议偏移推导，三类指向错误不可表达）；协议头 `magic`/`version`/`size`（= 信封结构大小）+ **`arena_len`**（= 协议常量 `ARENA_CAP`，接收方验证相等）；`free_upper_idx` 用 `u64::MAX` 哨兵；`MemoryRegion.len` → u64。
2. **arena 布局常量**（编译期 + 进布局门禁基准）：自基址起 = 信封（≤ 256 B 预算）→ wire 分区（128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7,616 B）→ 语义工作分区（`MemoryRegion` 双写供给 shim 侧，6,144 B）≈ **4 页（16 KiB）**；`Wire*` 元素 size/align/offset + 分区偏移逐目标交叉编译断言。
3. **方案乙配对循环**（`prepare_boot` 末节替换现「快照 A + helper EBS」）：`uefi::table::system_table_raw()` → raw `boot_services` → 外部缓冲 `GetMemoryMap`（自管 buffer：`memory_map_size()` 实查 + `desc_size×8` 余量，EBS 前 LOADER_DATA 单独分配）→ 分类**双写**（`WireMemoryRegion` 进 wire 分区、`MemoryRegion` 进语义分区；非内存图载荷——模块名/平台来源/param——在此前已序列化）→ 填信封静态字段 → raw `ExitBootServices(image_handle, map_key)`。
4. **四条硬性实施契约**（缺一不实施，round-9 + 九轮终审）：① **EBS 后三零**（零分配/零释放/零 BS 调用；RAII 清理在循环前完成——`prepare_boot` 链 Drop 面盘点为零；失败与终止路径同受约束）；② **失败原因分派**（仅 `EFI_INVALID_PARAMETER` 重试；其他状态立即失败；总尝试 2 次超限走不依赖 BS 的冷复位；缓冲三段处置——EBS 前可扩容 / 失败后只复用 / 复用不足 fail-closed；不沿用旧分类结果）；③ **描述符遍历**（`descriptor_size` 步长；引用建立前验证——version=1、desc_size ≥ size_of、**desc_size 为对齐整数倍**、首地址对齐、边界与整除；对齐不满足不强行建引用；循环体零 fmt/日志）；④ **区间 checked 算术**（`checked_mul`/`checked_add` + 页数非零 + PA 范围验证后才生成 `MemoryRegion`）。
5. **接收端受控复制**（镜像侧 `store_kernel_info` 的 wire 版五步）：受限原始读取验头（通过前不构造 wire 引用）→ 复制信封 → 逐载荷验证（容量 + 编码守恒两条；溢出/对齐/⊆ 已由布局构造保证）→ 受保读取阶段复制进 `.bss` 落地区 → 从落地区构造 `KernelInfo`。**切根后**：arena 信封原地补运行期字段（bump 三件套）、跳转 `x0 = arena 基址`（独立 blob 页删除）；验收改「arena 不占 bump 区 + 信封游标 = 跳转前真值」。
6. **九面失败测试族**（§9.3）：无效 version / desc_size 不足 / 对齐不合法 / 不整除 / 扩展步长 / 区间溢出与零页数 / 复用仍不足 fail-closed / 第二次 EBS 失败 / 非 `INVALID_PARAMETER` 状态不重试。

**D-67 存留件**（原信封合一 a–e 的 v9 后形态）：模块注释 "only data crossing the boundary" 失真句随实施删除；`BootPrepareResult` 保留为进程内类型（合一已裁不做）；builder 统一（D-69）随本批——`build_kernel_info` 收敛单一构造面、统一签名带 `reserved_regions` 形参；getter 家族补 `param_buf`。

**D-69**：两处 `build_kernel_info`（opensbi 8 参 vs uefi 9 参）随本批收敛；证据 `grep -rn "fn build_kernel_info" os --include="*.rs"`。

**T1 落地后 01 篇需同步的章节清单**（本轮 review 补列，避免实施者漏改）：§3.1 交接表「内存布局」行（快照两份列表 → 配对循环 + 终图）；§3.4 L1422 段的「切片深拷」叙述（受控复制五步）；§4.1/A. 主流程骨架（`prepare_boot` 末节换配对循环、blob 页移除）；§4.2 八步表（步骤 6「两次列表在同一快照分流」与「列表页快照后分配」的不变量整段改写——方案乙下不再有「快照后分配」类别）；§4.5 落地区表（三块 → 五块随 T5、加「wire 版受控复制」）；§4.7 bump 游标段（blob 页移除后的 `bump_next` 语义）；§5.1 M4/M12/M13 的断言与覆盖状态；§6.1 里程碑清单（新增配对循环路标、`memmap ok (final pre-EBS snapshot)` 字样替换）。

**子步进度**：1a ✅（01TODO-T1A）`handoff_v1.rs` 类型/arena 常量/布局门禁；1b ✅（01TODO-T1B）`handoff_v1_write.rs` 生产者序列化器；2a ✅（01TODO-T1C）`ebs_map.rs` 描述符走查分类；2b ✅（01TODO-T1D）`ebs_pair.rs` 配对循环可测核心；3a ✅（01TODO-T1E）`handoff_v1_read.rs` 接收端解码器；5a ✅（01TODO-T1F）D-69 单一 builder + D-67 旧口径；5b ✅（01TODO-T1G）`param_buf()` getter；**2c/3b ✅（01TODO-T1CUT，2026-10-30）**——aarch64 `prepare_and_enter_kernel_image`（arena + 配对循环 + 建根 + 原地补字段 + `x0=arena`）与 `enter_kernel_image_v1`/`arch_boot_resume_v1`（五块解码进 T5 落地区 + additive resume）落地，旧一页 `BootHandoff` blob 与 `bootstrap_to_kernel_image` 删除。**4 九面失败测试 ✅**（映射：无版本→`rejects_unsupported_version`；步长不足→`rejects_descriptor_smaller_than_prefix`；不对齐→`rejects_unaligned_descriptor_size`；不整除→`rejects_map_size_not_multiple`+`rejects_non_divisible_descriptor`；扩展步长→`padded_descriptor_stride_is_honored`；区间溢出/零页→`rejects_page_count_overflow`/`rejects_range_overflow`/`rejects_zero_page_count_on_produced_region`；复用不足 fail-closed→`too_small_on_retry_is_fail_closed`；二次 EBS 失败→`two_invalid_parameters_exhaust_the_retry_budget`；非 INVALID 不重试→`other_ebs_status_fails_immediately`）。**2026-10-30 补**：TODO-04 已升级为两条入口的文本级审计（`prepare_boot` 与 `prepare_and_enter_kernel_image`，5/0）；T1 的配对循环状态码修正是 `EFI_ERROR_BIT | code` 高位编码，原 2/5 只命中低字节。

**2c/3b 联合切根实施方案（下一笔开工前先读；T5 已备齐五块落地区）**：

- **载体**：aarch64 专属 `uefi_helpers::prepare_and_enter_kernel_image(bump_pages)`（只在 `target_arch = "aarch64"` 编译），不复用 `BootPrepareResult`——避免给 38 处构造点加字段；x86 内联路径保持到 T2 再统一。
- **发送顺序**：装载 kernel/模块 → 平台源 → root/bump 分配 → arena（`AllocateMaxAddress(bootstrap_alloc_limit(4 页)), LOADER_DATA, 4`）→ map buffer（`AllocatePool`，`memory_map_size()` 实查 + 余量）→ 非内存图载荷序列化（信封/模块/平台源/param）→ 配对循环（raw `get_memory_map` → `classify_map` → raw `exit_boot_services`；uefi-raw 0.9 的表字段签名无 `This`，状态码用 EFI_STATUS 高位编码）→ 建根开分页 → 原地补 runtime 字段 → `x0 = arena` 跳转。
- **接收端**：kernel 侧 `arch_boot_resume_v1(arena_pa)`（aarch64）：identity 读 arena → `decode_envelope` → 五块分区解码进 T5 落地区（memmap/reserved/modules+名池/platform/param）→ 构造 `KernelInfo` → `validate` → `set_current_root_phys` → `establish_boot_dm` → `jump_to_kmain`；`kernel-image/src/main.rs` 的 `x0` 非空支路改为传 arena（类型从 `*const BootHandoff` 改 `usize`）。
- **删除项**：`bootstrap_to_kernel_image` 的 blob 页分配与 `BootHandoff` 写入在切根落地后清理；01 篇 §3.4/§3.7/§4.7/§4.8 的 blob 叙述同批改写。
- **验证**：`test-shim-bootmarks-aarch64.sh` 先验 shim 两路标；`test-cmd-smoke-aarch64.sh` 全链；`run_all` 39 段不回归（x86 路径未动）。
- **风险**：raw EBS 与固件 ABI（aarch64 上 `efiapi == C`）；arena 必须落在 bootstrap 恒等窗口内；map buffer 的分配必须在 EBS 前完成，循环内零分配。

**2026-10-30 实施完成（01TODO-T1CUT）**：上述载体、发送顺序、接收端与删除项全部落地；实现偏离一处——`uefi` 0.33 的 `memory_map_size` 是 crate 私有，map buffer 改为 raw `GetMemoryMap` 空缓冲探测加余量、循环内按 `BUFFER_TOO_SMALL` 扩容；`uefi-raw 0.9` 作为 boot-shim 直接依赖引入（与 `uefi` 同一版本，`Cargo.lock` 未变）。门读数：minix-boot 57/0、boot-shim test-all 53/0 加边界审计 5/0、kernel mock 845/0/8、三架构 kernel/kernel-image check 0 error、`test-shim-bootmarks-aarch64` PASS、`run_all` 39/0/0；`test-cmd-smoke-aarch64` 到达 `/etc/rc`，`du`/`ls` 目录读腿在 `f03aa8ffc` 基线同样失败（既有 A 板问题，非本批回归）。

### 2.3 批次三 —— T5（先于 T3）

**D-70**（裁决：01-code-design §5，采案 1）：`globals.rs` 增 `PLATFORM_SOURCE_STORE`（4 条，**容量 4 = 已核实真实上限 2 × 有意余量**）与 `PARAM_BUF_STORE`（1024，清零再复制语义）；`store_kernel_info` 五块切片全落地；验收 = `test_store_kernel_info_lands_firmware_slices` 扩断言 + 超容量 fail-fast 测试（兼 TODO-03 第五块）。**状态**：✅ 01TODO-T5。两 pad 已在 `globals.rs`（platform 4 / param 1024，前者补 `BklProtected` 审计条目）；`store_kernel_info` 五块全部重指向；落地区测试扩平台源/param（含幂等再存指针稳定），超容量负控 5 枚；01 篇 §4.5 三块→五块、M8/TODO-03 行、§5.2 计数 4→6 同批；kernel mock 845/0/8、三架构生产 check 0 error。

### 2.4 批次四 —— T3（D-66）

- UEFI 腿读当前镜像 `EFI_LOADED_IMAGE_PROTOCOL` 的 `LoadOptions`（非 `Boot####` 枚举）；规范化四步统一规则；**ARCH 有可靠来源（编译目标）则生成、BOARD 有真实值才生成、不能确认显式省略**（v5 废除占位；结论范围 = 未发现仓库内消费者）；截断按整条记录丢弃；SBI/直核腿来源 = `/chosen/bootargs`，验收须实测本仓 QEMU 链 `-append` → DTB 节点通路（或诚实结论 = 该腿长期空切片）。
- 验收：`GET_MONPARAMS` 与 C 同形（1024 定长零填充）；01 篇 §3.4 param_buf 表述同步。

**2026-10-30 实施完成（01TODO-T3PARAM）**：`minix-boot` 新增 `boot_params.rs`（`normalize_param_buf` 唯一规则：NUL/空白分词、可打印 ASCII、内部键拒绝、重复键首条胜出、整条记录截断）；`build_kernel_info` 增第 10 个 `param_buf` 形参；UEFI 腿读 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`（非 UCS-2/二进制不猜，逐条拒绝），OpenSBI 与两个直核面读 DTB `/chosen/bootargs`（`all_nodes` 走查，避开 `find_node` 的 SD-38 缺陷）。`KernelInfo` 与内核零改动。读数：minix-boot 65/0、boot-shim test-all 53/0、kernel mock 845/0/8、三架构 kernel/kernel-image check 0 error；aarch64 直核注入 `/chosen/bootargs "ARCH=probe BOARD=qemu FOO=bar"` 后串口 `param_buf bytes=0x15 first=ARCH=aarch64`（ARCH 13 字节 + `FOO=bar` 8 字节；BOARD 无来源按规则省略，外部同名键被拒）；UEFI 腿 `param_buf bytes=13 rejected=0 dropped=0`（AAVMF 启动管理器未传 LoadOptions，生成 ARCH 一条）。riscv 直核面复用同一 `bootface.rs` 代码，其现有测试夹具的模块表为空、走不到 `build_kernel_info`，该腿的端到端读数留待 D-73 shim 腿。

### 2.5 批次五 —— T2（I-1/NK1/OQ-N6：x86 切独立镜像）

- OQ-N6 已裁（每进程独立根原语）；本批 = NK1（C-29）实施收口。全部入口契约已冻结：trampoline 状态清单（镜像内 bootstrap 栈起步、**禁止验证前读 `kern_stack_top`**、显式 `cli`/`cld`、16B 对齐、`rbx` 持 blob 指针跨 `.bss` 清零）；red-zone **无需任何旗标**（`x86_64-unknown-none`/`x86_64-unknown-uefi` 规格 `disable-redzone: true`，round-5 B 批实测）；`load_elf_into` 前提归边界成文（不改签名不加 trait）；`reserved_regions` 按消费语义拆分输入（`dm_coverage` 保留、`vm_handoff` kern_ident 输入删除）；模块 blob 分配改 `MaxAddress(bootstrap_alloc_limit)` + 区间末端验证；QEMU 先行短期双轨后翻产线删旧路径。
- 联动：01 篇 §3.7 三形态表 x86 行、§4.8、L1422/L2624/L1424 张力块（同批，见 §3）、`os/kernel-image/src/main.rs` 头注「交付边界」段更新。

**2026-10-30 实施完成（01TODO-T2X86）**：`prepare_and_enter_kernel_image` 与 `enter_kernel_image_v1`/`arch_boot_resume_v1` 的 cfg 扩到 `any(aarch64, x86_64)`；x86 的最终跳转走 `__minix_x86_enter_image` 全局汇编（`cli`/`cld`、`rbx=arena`、`jmp entry`），镜像 `_start` 建栈、清 `.bss`（只碰 rdi/rax/rcx）、`and rsp,-16`、`mov rdi,rbx`。**实现中发现并修复一处 ABI 真 bug**：`extern "C"` 在 `x86_64-unknown-uefi` 上是 Win64 约定（rcx/rdx），与 stub 的 SysV 取值不符，曾把 `KernelInfo` 指针当入口地址跳进栈；改为 `extern "sysv64"` 后 x86 门稳定 PASS（连跑三次）。`reserved_regions` 的 kern_ident 输入已从 `vm_handoff::build_identity_windows` 删除（只剩架构 MMIO 恒等窗口），`dm_coverage` 源保留；`alloc_module_pages` 改 `MaxAddress(bootstrap_alloc_limit(size))` 并验证区间末端；`load_elf_into` 的恒等可读前提成文。新增 `test-shim-bootmarks-x86.sh`（五路标：三 shim 行 + 接收端行 + `entering scheduler`），`test-cmd-smoke.sh` 的 M14 交接行改为 `[raw] entering kernel image v1`。门读数：kernel mock 845/0/8、minix-boot 65/0、boot-shim 53/0 + 边界审计 5/0、三架构 kernel/kernel-image check 0 error、x86 冒烟门 PASS（三次）、aarch64 冒烟门 PASS、`test-cmd-smoke.sh` 到达 `/etc/rc`（仅 `du` 既有 A 板红）、`run_all` 39/0/0。

### 2.6 独立线 —— D-59（`boot-dm-hugepage-design.md`，不进 01-code-design）

- 设计文档已定稿（2,249 行；五轮评审收尾；lint 0 error）；**待实施**，迁移五阶段（先启动选页 → `PageSize`/能力 API → 事实/规划拆分与策略 → `map_huge`/SMP/预检 → 机械重命名）。
- 开放问题（§10）现状：大多已决（`BootDmInstaller` 封口、物理上界路线 A + 统一取小、MMU 模式变更时重算）；剩小项 = `test-support` feature 打包方式、`DmPagePolicy` 构造边界、`KernelLayout::from_info` 取舍、`supports_huge_page` 命名——随实施定。
- **未关闭验收门（独立立项，不随迁移闭合）**：候选区间整段丢弃的覆盖损失（reserved 与候选部分重叠时非重叠尾部失去 kernel-DM 叶子）——按优先级区间差集归一化另立专项 + 真机证明；本项闭合前集合级不变量不承诺「所有有资格映射的字节都被保留」。
- 与 01 篇的交点：TODO-05（2 MiB 回退档真机启动）随本条实施 + NK1 一并验。

### 2.7 D-72（新登记）——bump 池耗尽的结构债：1024 页是 runway，不是终态
> **裁决（八轮评审，通过候选 A）**：映射责任移交 VM；所有权三分——`KERNINFO_PAGE` 归内核（长期保留、不得被 VM 当普通帧回收）、每个地址空间的 `KERNINFO_USER_VA` 映射归 VM（建根建立、随页表销毁）、中间页表页归 VM 的页表分配器；`VmBootHandoff` 加字段并**升载荷版本**（VM 拒绝不支持版本，不得静默漏映射）；`fork`/`exec` 分别验收；4 MiB 启动池预算在候选 A 后按实测峰值复核。**仓库侧前置已核实**：`init_kerninfo` 在 Phase B.5、`build_vm_handoff` 在 Phase C——加字段无需倒置初始化顺序；`fork`（`vm/src/fork.rs:do_fork` 的 `pt_new` + `fork_regions`）与 `exec`（`vm_server.rs:exec_bootproc`）都新建根、不继承映射，实现点应放在 VM「根创建」统一函数。详见 `01-design-note-riscv-unify-d72.md` §5.2/§5.4。

**现状（代码注释自陈，属 01 篇射程：`prepare_boot(N)` 的预算正是 §4.1 讲的那个池）**：

- 预算构成：boot 期约 100 页（恒等/内核映射、DM 覆盖、引导身份窗口逐页拆解、交接链），其余余量给**每次新根绑定（exec 重建根）的 kerninfo 注入链**留 1–3 页/次。证据：`os/boot-shim/src/main.rs:fn main` 的 `prepare_boot(1024)` 上方注释（NK4-C 1.55 B30 段）。
- **池无回收路径**：`BootAlloc` 是单调 bump（`alloc` 只前进、失败回滚一格），页表页从不归还；真机（bn9m）在 **~40 次绑定后 `AllocationFailed` 死亡**——注释原文 "the pool has no free path … died AllocationFailed after ~40 binds"。
- **代价已量化且落在 VM 可分配总量上**：`vm_handoff` 按 `boot_alloc_region()` **整段**扣除（用掉的与剩下的都算内核保留），1024 页 = **4 MiB 每次启动从 VM 可选内存里永久扣掉**。
- 注释自陈不是修好了："Before the structural fix lands (**reclaimable page-table pages / moving the kerninfo mapping responsibility to VM per kerninfo.rs doc** — WORKLOG 1.55 follow-up) this pool only postpones the deterministic panic." `kerninfo.rs` 模块 doc 侧对应口径：本次重写不移植 `.usermapped` 段，`init_kerninfo` 自建一页并在**每个新根**里复刻该映射（即注入链的来源）。
- **未登记核实**：`todo.md` 与旧 `01-todo.md` 全量检索 `reclaimable|kerninfo-injection|bn9m|1.55` 只命中 I-25 的锚点样例——本债此前**没有条目**；01 篇 §4.1 只描述了预算构成，未写耗尽故障与结构修复缺口。

**待裁决（两候选，实施前需一次小裁决）**：

1. **可回收页表页**：给引导期页表池加回收路径（C 的 `pt_free` 语义在 boot 侧的对应物），或改用 VM 的页表池承担新根建表；
2. **kerninfo 映射责任移交 VM**：内核只发布 kerninfo 页的物理位置，由 VM 在建每个进程根时把该页映射进去（与 C 的 `.usermapped` 段语义更近；`kerninfo.rs` doc 已把这个方向写成"待结构修复"）。

**登记理由**：这是 01 篇射程内唯一的"已跑通过的临时方案被当作终态"的结构债——不影响当前启动，但在 exec 密集负载下是确定性上限（~40 次绑定）。**验收方向**：设计裁决落文（01-code-design 增补一节或独立小设计）→ 实施 → 真机验证"exec 重建根 N 次（N ≫ 40）不耗尽"或"bump 池不再承担每绑定开销"。

**证据命令**：

```
sed -n '38,62p' os/boot-shim/src/main.rs          # 预算构成与 bn9m 读数
grep -n "init_kerninfo\|new root\|replicating" os/kernel/src/kerninfo.rs | head
grep -n "boot_alloc_region" os/kernel/src/vm_handoff.rs | head -3
```

**2026-10-30 实施完成（01TODO-D72VM，候选 A）**：`VmBootHandoff` 升到 v7 并加 `kerninfo_pa`（非零/页对齐校验；v6 及以下拒收）；`minix_types` 增 `KERNINFO_USER_VA` 单一权威，内核 `kerninfo.rs` 改为再导出；`kernel/vm_handoff.rs` 在 Phase C 填 `kerninfo_page_phys()`；`kernel/syscall.rs` 的 `vmctl_set_addr_space` 每根注入块删除（源码序钉 `os/kernel/tests/set_addr_space_no_boot_alloc_pin.rs` 2/0 钉住"不再出现 `boot_pt_alloc`/`kerninfo_page_phys`/`KERNINFO_USER_VA`"）；VM 侧 `BootParams` 加 `kerninfo_pa`、`init_page_table` 在 `map_kernel` 之后把该页只读映射到 `KERNINFO_USER_VA`（失败走既有 `free_page_table` 回滚）。`fork` 与 `exec` 都经 `init_page_table` 新建根，一处覆盖两条路径。门读数：minix-types 331/0、minix-vm 543/0、kernel mock 845/0/8、三架构 check 0 error、`run_all` 39/0/0（含 rt-birth 的 `kerninfo=ready` 查询）。4 MiB 预算按实测复核：映射不再从启动池取页，池只承担启动期本身，1024 页维持不变（净值见 `kernel_allocated_dynamic` 交接字段）。

### 2.8 D-73（新登记）——riscv64 独立 boot-shim 腿：三架构统一为主线

> **裁决（八轮评审，有条件通过）**：方向通过；先闭合 **入口契约闭合门（输入契约）**再接线——① U-Boot 版本与 `bootelf` 支持面（官方文档：`bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`，`image_addr` 之后的参数以 `argc/argv` 交给应用，`-d` 按地址设置设备树）；② shim 入口适配层解析 argc/argv（设备树 + 启动清单）并校验，不依赖 OpenSBI 的 `a0/a1` 语义；③ kernel.elf 与 12 模块由谁装载（沿用 U-Boot 预载 + `BootFileTable` 契约，或 shim 自读）与地址/长度/内存范围进契约；④ 页池/设备树/镜像/模块/交接载荷互不重叠（`MODULE_REGION_BASE` 等 QEMU 常量改为 DTB `/memory` 推导或构建期注入）；⑤ 两内核产物入口契约（`kernel-direct.elf` 直核：`a0`=hartid、`a1`=DTB；`kernel-shim.elf`：只收交接载荷），且两入口适配完成后必须进入同一套公共内核启动语义流程（汇合点验收）；⑥ `-d`/参数传递依赖 U-Boot 构建配置（`CONFIG_CMD_ELF_FDT_SETUP` 一类），实际版本须实测核实；⑦ 入口 ABI / 固件服务 / 文件与启动信息来源三个维度分开表达，不固化成同一个"固件实现"选择开关。**分道 = 两个构建产物**（不做运行时分道猜测）。直核自引导面称「辅助启动路径」，保留并继续验收；QEMU 通过 ≠ 真板契约成立。rustc 现无 riscv64 UEFI 目标（`*-unknown-uefi` 列表无 riscv64），UEFI-on-riscv 只作未来扩展。详见 `01-design-note-riscv-unify-d72.md` §五。

**方向（提出方立场，已获有条件通过）**：三架构共用同一条主线——**独立 boot-shim 可执行体 + 独立内核镜像 + 交接载荷**。boot-shim 的职责是为内核屏蔽固件形态（UEFI、U-Boot/OpenSBI 或将来其它）；riscv64 也因此收编进主线，自引导面（`bootface`/`bootface_a64`）**降级为特定嵌入式形态**（riscv 的 `FW_PAYLOAD`、QEMU `-kernel` 测试通道），保留但不作主产线。依据：`BootShim` trait 本就与固件无关（`UefiBootShim` / `OpenSbiBootShim` 两个实现产同一 `KernelInfo`）；riscv 的 shim 腿主体（`OpenSbiBootShim`/`UbootFileLoader`/`BootFileTable`/`bump_alloc_pages`）已在仓内，只是**零接线**（bin 只按 `fw-uefi-image` 构建，`OpenSbiBootShim::prepare_boot` 无调用方）——当前"riscv 产线走自引导面"是历史缺线，不是架构结论。

**已知缺口（实施前要解决）**：

1. **bin 与 feature 词表**：`os/boot-shim/Cargo.toml` 需增 riscv64 bin 词表（如 `fw-riscv-opensbi`），`main.rs` 按 feature 选固件实现；目标 triple `riscv64gc-unknown-none-elf`。
2. **入口参数通道**：U-Boot `bootelf` 不按 OpenSBI 的 `a1` 传设备树（现有 `install_dtb_ptr(a1)` 与 `rust_image_main(a0/a1/a2)` 约定只对 OpenSBI 直载成立）——需裁决取树通道（候选：`go` 带参 / 约定内存槽 / 扫 FDT 魔数 / 构建期注入）。
3. **交接泛化**：`bootstrap_to_kernel_image` 与 `arch_boot_resume_high_half` 当前 `#[cfg(target_arch = "aarch64")]`，需放宽到 riscv64（架构差异仅跳转指令 `fence.i`+`jr` 与寄存器；`BootHandoff` 类型本身架构无关）。
4. **内核镜像两形分道**：直核（`bootface`）与 shim 交接共存；aarch64 的「`x0==0` 走自引导」判据在 riscv 不成立（直核 `a0` 是 hartid）——需新判据（候选：两个独立 ELF 入口符号 / 魔数载荷 / 专用寄存器约定）。
5. **池基去 QEMU 化**：`opensbi_helpers.rs` 的 `MODULE_REGION_BASE = 0x8000_0000 + 32 MiB` 与 `BUMP_END` 编译期断言是 QEMU 布局硬编码，真板不可用——改为从设备树 `/memory` 推导或构建期注入 + 运行期检查。

**裁决载体**：`rewrite-notes/01-stage-kernel/01-design-note-riscv-unify-d72.md`（完全自包含，含议题二 D-72；外部评审已完成并冻结，裁决见其 §五）。裁决回填本条目与 `01-code-design.md`（T2 的「riscv64 直核腿不需要 shim 侧跳转」表述随裁决更新）。

**验收方向**：QEMU 上 U-Boot `fatload` 载 `boot-shim.elf` → `bootelf` → shim 读 12 模块 → 建表 → 跳 `kernel.elf` 高半 → 内核首个路标；`-kernel` 直核形态不回归；真板窗口另计（`FW_PAYLOAD` 形态保持可用）。

**入口契约实测结论（D-73，GPT 裁决核定后的更正版）**：早前的「bootelf 无 argv」结论**已被探针证伪**——`help` 的 usage 文本只写 `[address]` 不足以证明参数通道不存在。本机 U-Boot 2025.10（`/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf`，OpenSBI 之下 S-mode）用最小 RISC-V 探针（`_start(argc, argv, a2)`，SBI console 打印）实测：**`bootelf -p <img> [args…]` 会把 image 地址之后的参数以 `(extra_argc, extra_argv)` 交给应用入口**——`bootelf -p $loadaddr` → `argc=0`；加 `87654321` → `argc=1, argv[0]="87654321"`；加 `aaaa bbbb` → `argc=2, argv[0]="aaaa", argv[1]="bbbb"`；a2 是 U-Boot 堆内指针（随调用变化），**不是 FDT**。两个此前没看见的门：① `autostart` 环境变量默认未定义时 bootelf 只装载不执行（必须 `setenv autostart yes`）；② 不带 `-p` 时本 build 的 extra-arg 形状未通过（`bootelf $loadaddr deadbeef` 报 `## No elf image at address 0x80200000`），契约应钉在 `-p` 上。`-d` 未在 help 列出且本设计不需要它（DTB 地址进 BootFileTable，由 shim 自己读）。

早前 hello-boot 的 `bootelf` `Load access fault`（EPC `0x8f73d74e`）的真因也修正为：**staging == LMA 自重叠**——默认 `loadaddr=0x80200000` 既是 fatload 落点又是 hello-boot 的 PT_LOAD 目标，bootelf 边读边覆盖装载头。探针把 LMA 放到 0x84000000、staging 留在 0x80200000 后两次全链干净执行。`bdinfo` 布局证据（256M 配置）：DRAM `0x80000000-0x8fffffff`；lmb reserved = OpenSBI `0x80000000-0x8005ffff` + `0x8deaa000-0x8deacfff` + U-Boot 区 `0x8dead8d0-0x8fffffff`（relocaddr `0x8f6cf000`、malloc `0x8eeaf000-0x8f6cf000`、fdt_blob `0x8eead8e0`）；可用带 = `0x80060000-0x8dea9fff`，默认 staging `loadaddr=0x80200000`、boot.scr 执行拷贝 `0x8c100000`。

**去向（按裁决更新为首选路线）**：保留独立 BootShim 方向；下一步是物理地址链接的 `riscv64-uboot-shim.elf` 探针→产物（VMA=LMA=物理入口、自带 bootstrap 栈），入口 ABI=U-Boot `bootelf -p <img> <BootFileTable 地址>` 的 `(extra_argc, extra_argv)`，`argv[0]` 传 MNXBOOT1 表地址；DTB/kernel/12 模块由 U-Boot 预载、表承载；**不需要新增固定 handoff 槽**（固定槽仅作板级备用 ABI）。LMA 安全带从 bdinfo/lmb 推导并由 D-73 专属布局配置生成，不硬编码 `0x90000000`。`BootHandoffV1` 冻结协议不动；U-Boot 的入口问题由 shim 适配层消化。完整探针证据：`/tmp/d73/{probe.elf,argv-base2.txt,argv-plus.txt,argv-matrix.txt}` 与本段命令。

**2026-10-31 实施完成（D-73 全链 PASS）**：新增 `os/boot-shim/src/uboot_main.rs` + `riscv64-uboot.ld`（物理链接 VMA=LMA=0x84500000，`.text.boot` 入口自清 SIE、自清 .bss 含 `.lbss`、自立 64KB bootstrap 栈）+ `build.rs`（特性门挂链接脚本）；`RiscvUbootBootShim`（原 `OpenSbiBootShim` 更名）经 `prepare_boot` 用 MNXBOOT1 表读文件——表由 shim 在自身 .bss 用 `uboot_layout` 常量自建（kernel 0x86000000/2MB、模块 0x87000000+1MB×12、DTB 0x8c000000），boot.scr 只 fatload 文件，绕开预建表的 staging 共变问题；arena 走 v1 序列化后 `enter_kernel_image_v1` 跳接收镜像。`kernel-image` 增 `fw-riscv64-uboot` 特性：`rust_image_main(arena_pa)` → `arch_boot_resume_v1`（riscv64 臂与 x86_64 同构）。实施期三处修正：① `Riscv64HigherHalf::jump_to_kmain` 加 `auipc` 执行位置探针——接收腿已被跳到高半链接地址，medany 的 PC 相对 `la` 已解析真值，链接↔装载偏移不得再加（故障读数：EPC=0xffffff7f7fe0477a = kmain 链接址 0xffffffc00000477a + delta 0xffffffbf7ffe0000，算术精确闭合）；② shim 的 `DtbMemoryMap::new` 第二参从 0 改为 `KERN_PHYS_BASE=0x80200000`（bootface 同源语义）——此前 `[0x80000000,0x80200000)` OpenSBI 足迹漏进 VM 空闲表，VM 把它选作 PMM 元数据供体，向 DM(0x80000000) 写入页 fault（stval=0x0000001080000000 = DM 基 64GiB + PA 0x80000000），而内核 DM 计划正确地拒绝映射固件区间；③ 门脚本 Stage 1 的 `set +x` 曾把构建失败掩盖成旧二进制续跑（`|| fail` 检查到的是 `set +x` 的退出码），改 `set -e -x`。门读数：`test-riscv64-uboot-shim.sh` PASS（6/6 路标：shim enter/表建/模块装载/进 v1/接收 resume/entering scheduler；空闲表修正后 n=2 无固件区间）；直核腿 `test-riscv64-boot-full.sh` PASS（rc marker）；docker kernel mock 850/0/8；boot-shim test-all 58/0；三架构 kernel check 0 error。用户腿取证诊断扩展（`ura/usp/ua0/ua2` 进 riscv64 用户腿 trap 面板）随本批保留。
---

## 3. 直修批（无设计依赖，可立即可做）

### I-22 收尾回勾（代码已落地，三件尾巴）

1. `todo.md` §7.1 的 I-22 条目改记「代码已落地（丁案 A–F 除 aarch64 直核腿外），遗留 D-68 与文档连带」；
2. **文档连带 ①–⑦ 逐一核对**（todo.md I-22 条目所列：01 篇 L1265/L1945/L2288 三表、opensbi_helpers 顶注（→I-26）、bootface 头注、§3.7/§4.8 合流句、04 篇平台发现分工段）——未见专查记录，回勾时逐项过；
3. D-68 之外的连带（名字池/常量合并）不在本条射程。

**状态**：✅ 01TODO-B2——①–⑦ 逐项核过：① §3.1 L1265 表、② L1961「内存与参数准备」行、③ L2288 `memmap` 行、④ opensbi 顶注（I-26 已修）、⑤ bootface 头注（本批修正对已删 `build_memmap` 的指涉）、⑥ §3.7/§4.8 合流句、⑦ 04 篇 §3.1 平台发现分工段，全部已按新口径在位；`grep -cE '硬编码的 QEMU virt DRAM 布局|单条 DRAM'` → 0。第 1 件（`todo.md` §7.1 改记）已在本批台账行同步；遗留仅 D-68（T4 批）。

### I-28（本轮新登记）`MemoryMapSource` trait 零多态载荷：删除与否待设计专项裁决

**现状**：`os/libs/minix-boot/src/memmap_source.rs` 的 [`trait MemoryMapSource`](os/libs/minix-boot/src/memmap_source.rs:trait MemoryMapSource) 全仓**无任何多态使用**——既无泛型约束也无 `dyn`；四个调用点写 `use MemoryMapSource as _` 或 `<DtbMemoryMap as MemoryMapSource>::collect`，只为把方法引入作用域，改成固有方法后这四处可原样删。三个 impl 的活死状态：`DtbMemoryMap` 活跃（riscv 直核面 [`fn bootface`](os/kernel-image/src/bootface.rs:fn bootface)、aarch64 直核面 [`fn bootface_a64`](os/kernel-image/src/bootface_a64.rs:fn bootface_a64)、OpenSBI/U-Boot shim [`impl RiscvUbootBootShim`](os/boot-shim/src/opensbi_helpers.rs:impl RiscvUbootBootShim) 三处产出内存图；aarch64 UEFI 生产入口 [`fn prepare_and_enter_kernel_image`](os/boot-shim/src/uefi_helpers.rs:fn prepare_and_enter_kernel_image) 另有一处调用，用途是 DTB 越界校验、不产出内存图）；`UefiMemoryMap` 的唯一消费者是 [`impl BootShim for UefiBootShim`](os/boot-shim/src/uefi_helpers.rs:impl BootShim for UefiBootShim) 的 `prepare_boot`，而该函数**全仓零调用方**；`MultibootMemoryMap` 返回空表，零引用。注意 `build_memmaps`/`build_memmap` 不能删——14 个 QEMU 测试内核经 `build_memmap()` 调用它，死的是包在外面的那层 `UefiMemoryMap` 壳。

**为什么算问题而不只是"没用到"**（v2 按评审修正表述）：这条 trait 确实提供**静态接口约束**（`impl` 的方法与签名受编译器检查）与**静态分发**（两处全限定调用），但**没有任何消费者依赖它所提供的抽象边界**——调用方都知道自己是哪个具体类型，抽象既没降低调用方的类型依赖，也没统一它宣称要统一的两侧。UEFI 生产链接不进来不是惰性，是签名表达力不足——分类必须与 `ExitBootServices` 共用同一个 MapKey（[`fn pair_and_classify`](os/boot-shim/src/ebs_pair.rs:fn pair_and_classify) 把两者显式配对）、产物必须直接写进 arena 分区（[`fn classify_map`](os/boot-shim/src/ebs_map.rs:fn classify_map) 的 `serializer.push_memmap`）、失败面是 `BUFFER_TOO_SMALL` 与 `EFI_INVALID_PARAMETER` 重试。`fn collect(&self) -> (两张表)` 这三样都装不下。于是这条 trait 的多态维度实际永远只有 DTB 一支 + 一个死实现 + 一个空占位；留着会让人以为接新来源只需 impl 一下。

**待裁决（用户已开设计专项；第三方评审 v1 已给方向：删除）**：方向 = 删除 trait，`collect` 降为 `DtbMemoryMap` 的固有方法，删 `MultibootMemoryMap`，随 A1 去留一并处理 `UefiMemoryMap`，把「可用表 / 保留表」语义搬进 `KernelInfo` 字段文档（或共享结果类型），并改写模块头注不再宣称「唯一出口」。**必须走正式 supersede，不得伪装成顺手清理**：记录需答六问（当初为何引入 / 假设的消费者与实现 / 当前实际能力 / 哪些假设未兑现 / 本次为何撤销 / 语义文档去向）。重新引入的判据：**第二种真实来源出现且存在真实消费者依赖共同语义时**，再按真实差异设计。保留方案 B（只改注释）与 C（真正接线 UEFI）已由评审否决，理由分别见文档 §1.4 与 §1.2。

**验收（裁决后）**：`grep -rn "MemoryMapSource" os --include="*.rs"` 结果与裁决方向一致（删除则只余零命中）；三架构 `cargo build` 与 `os/qemu-tests/run_all.sh` 回归；代码侧不留"统一出口"这类与事实不符的注释表述。

### I-29（本轮新登记）UEFI 内存图分类双份实现：`classify_map` 与 `build_memmaps`

> **状态拆分（D-069，用户 2026-11-20）**：本条在总览里拆成两行——`I-29（设计）` 已完成（v12 冻结），`I-29-impl（实施）` 待实施。本节的冻结内容仍是设计部分的权威记录；实施按四个验收门推进，四个门与执行序列见本节末的「正式冻结宣告」与「v11 剩余工作」两段。

> **I-29-impl 实施补记（门④路径 A 落地）**：
>
> **四门现状**：门①（原型 ① 生产解析器 + 预分配 sink + 五步裁决 + 零分配 + 描述符携带属性）**已落生产代码**（`ebs_map.rs` 的 `adjudicate` / `DescriptorRef.attribute` / `parse_raw_map`，基线 acd1302d3 批）；门③（Pass1/Pass2 API）随门①已落；**门④路径 A 本批落地**；门②（三架构普通 WB 合同，I-29-16）未动——x86 PAT 读/初始化与 riscv PBMTE 启用链在全仓 grep 零命中，需要三架构各自的硬件前提取证，不属于纯代码改造。
>
> **门④落地内容**：① `uefi_helpers::build_memmaps` 的分类逻辑整体抽到 `classify_map_bytes_into_test_tables(buffer, map_size, desc_size, desc_version)`——全树唯一分类器现在同时服务退出前（`build_memmaps`，仅存于不退出的 inspect-only 载体）与退出后两条路径；② 新增 `exit_boot_services_and_classify()`：**保留** `boot::exit_boot_services` 返回的 `MemoryMapOwned`（旧 `exit_boot_services()` 以 `let _mmap` 丢弃它），用 `meta().map_size`（非 `buffer().len()`——前者是有效长度，后者含冗余，设计文档 §2.7 前提一）取原始字节交唯一分类器，写入一次性表后返回 `'static` 切片；③ 12 条 UEFI 测试内核迁移到路径 A 流程（先分配 → 退出+分类 → 建 `KernelInfo`），编译全 0 error。
>
> **端到端验证阻塞已解除（2026-11-20）**：hello-boot（x86）新鲜二进制在 `step1+2 mappings ok` 后静默停的问题，已由 I-43 二层专项关闭（真因＝载体硬编码装载基址 + 直核腿缺调用者栈窗口，见 §3 的 I-43 二层闭合记录；stash 对照当时即确认非路径 A 引入）。hello-boot 现以新鲜二进制 **PASS**。门④的端到端六项验收（§2.7）已可跑。
>
> **迁移名单（12 条，全部编译 0 error）**：hello-boot{,-aarch64}、test-higher-half{,-aarch64}、test-kernel-map{,-aarch64}、test-paging-enable{,-aarch64}、test-protection{,-aarch64}、test-shutdown-aarch64、test-smp-shutdown。未迁移：test-memmap{,-aarch64}（inspect-only，不退出，pre-exit 图即测试主体）；其余手绘 MEMMAP 载体（test-timer-irq 等）不消费 `build_memmap`。

**现状**：UEFI 侧有两份功能相同的分流实现——生产路径的 [`fn classify_map`](os/boot-shim/src/ebs_map.rs:fn classify_map)（经 [`fn pair_and_classify`](os/boot-shim/src/ebs_pair.rs:fn pair_and_classify) 调用）与 [`fn build_memmaps`](os/boot-shim/src/uefi_helpers.rs:fn build_memmaps)（只被辅助入口 A1 与 14 个 QEMU 测试内核经单数形式 `build_memmap()` 调用）。三条分流规则逐条相同（`CONVENTIONAL` 进可用表；`MMIO`/端口空间丢弃；其余且起点低于 RAM 顶进保留表），**校验强度与一致性保证不同**：`classify_map` 有版本/尺寸/对齐/边界/零页数/溢出/物理上限七类自建校验与“同一次读图取键退出”的结构配对；`build_memmaps` 无自建校验（解析交给第三方 `uefi` 库）、不判 `PA_LIMIT`（该常量就在同文件相邻处却只被生产路径使用）、零页数会产出长度 0 的区间，且分类快照与退出是先后两次读图，一致性靠“其间不分配”的顺序纪律维持。

**测试面的后果（v3 已按第二轮评审重写）**：见下方 v3 补记。要点是：问题不止「两份实现重复」，更在于**测试路径拿到的那张内存图不反映快照之后的分配**。

**v3 核心发现（三类快照后分配）**：① `build_memmaps` 的 `Vec` 与两次 `Box::leak` 发生在它取到快照之后；② 12 个夹具内核在 `build_memmap()` 之后才 `alloc_root_page`/`alloc_bump_region`；③ `uefi` 0.33.0 的 `exit_boot_services` 内部先 `allocate_pool` 建缓冲、再重读图取键。三类都不会反映在分类结果里。最危险的是 ①：内核拿到的 `memmap` 切片本身住在被误报为可用的内存里，一旦 PMM 开始分配就可能覆盖内存图自身。生产路径没有此问题——读图与退出之间**零分配**（分类只往 arena 写），且 `assert_bootstrap_outside_memmap` 三次守护。v2 的修复「先分配后快照再断言」只覆盖 ②，不够。

**断言分工（修正 v2 §2.5/§2.6 的内部矛盾）**：12 个夹具内核**不验证完整分类语义，但必须验证本测试自举占用不进入可用表**；完整分类规则由专门测试负责；宿主单测覆盖非法输入；真实 UEFI 路径新增输出内容断言。

**v3 候选（两条完整修复路径）**：路径 A（推荐先研究）——保留 `exit_boot_services` 返回的 `MemoryMapOwned`（当前被 `let _mmap =` 丢弃），退出后用它分类并写入预分配存储，天然满足「分类图 = 退出图」；路径 B——测试走生产式配对事务（学得到重试路径，但接线重）。分工建议：夹具内核用 A，验证配对重试另立专门测试。

**② 段现状更正**：v2 写「当前已唯一」不准确——生产路径只有一份项目自有的原始字节解析器，**测试路径尚未复用它**（用第三方库解析结果）。两个目标：一=只统一 ③（承认测试不验证原始解析器），二=连解析一起统一（优先考察）。

**输出接缝细化**：输入必须是**可重复遍历的视图**（分类扫两遍算 RAM 顶），不能是一次性迭代器、不能分配中间数组；输出在回调与私有 sink 之间比较（不优先返回两个大数组）；容量 128/256 属交接协议契约，测试不得复制一份。

**v4 核心发现（`ram_top` 边界 + 类型生命周期 + 反向偏差）**：

- **I-29-7 `ram_top` 边界（最高优先）**：第二遍分类判据是 `desc.physical_start < ram_top`（严格小于）。自举分配用 `AllocateMaxAddress(bootstrap_alloc_limit(..))`，只规定上界不规定取位取向，因此根页表页/bump 池/arena 很可能落在**剩余常规区间的最高端**，其起点**等于** `ram_top` → 该描述符**两张表都不进**。`assert_bootstrap_outside_memmap` 只查可用表，抓不到。**核实结论：当前未见功能性故障**——`dm_coverage` 的 source 2（`bootstrap_tree_candidates`）用交接携带的根页表地址与 `boot_alloc` 记录的 bump 区间单独登记，不依赖保留表；arena 与读图缓冲不被内核访问；被丢弃的条目不在可用表内也不会被分配出去。但「非常规低于 RAM 顶都进保留表」这条成文不变量为假，且 12 条宿主单测无「起点恰等于 `ram_top`」用例。三选项待裁决：`<=` / 显式承认不完备 / 改判定依据（与任何 RAM 区间相交或按 RAM 类型）。
- **I-29-8 类型生命周期政策**：当前把 `BOOT_SERVICES_CODE/DATA`、`ACPI_RECLAIM` 一并永久放保留表 = 永不复用，属保守政策需成文；`PERSISTENT`/`UNACCEPTABLE` 是「碰巧进保留表」，无显式论证。
- **I-29-9 反向偏差**：`boot::memory_map` 返回的 `MemoryMapOwned` 在 `build_memmaps` 内是局部变量，函数返回时 `Drop` 在引导服务仍存活的前提下 `free_pool` **释放**了那块缓冲，而快照里它仍是 `LOADER_DATA` → 保留表**多报**一块实际已可用的内存。生产路径的读图缓冲用裸 `allocate_pool` 且从不释放，不受此影响。

**路径 A 前提已核实（v4）**：`uefi` 0.33.0 的 `trait MemoryMap` 公开 `meta()` / `buffer()` / `entries()`，`MemoryMapOwned` 实现该 trait → 退出后可把原始字节交给生产解析器（**目标二可行**）；注意 `meta().map_size` 是有效长度、`buffer().len()` 含冗余条目，不可混用。`MemoryMapBackingMemory::Drop` 先判 `are_boot_services_active()`，退出后不释放。补失败契约：退出后分类出错走不依赖引导服务与动态分配的 fail-stop（裸串口 + 停机）。

**`0.33` 版本下的 `Drop` 与实现位置**：`MemoryMapBackingMemory` 在 `src/mem/memory_map/impl_.rs`，`MemoryMapOwned` 同文件；`boot::memory_map` 在 `src/boot.rs` 返回 `MemoryMapOwned`。

**v5 核心发现（类型/属性/生命周期政策 + 规范与实现的区分 + 对象–页级区分）**：

- **I-29-8 扩充：UEFI 类型必须分类，不能靠地址**。分类器只显式认识类型 7/11/12，其余全部走「低于 RAM 顶即进保留表」。按 `uefi-raw` 0.9.0 的枚举逐项对照，**三处语义冲突**：`RESERVED`(0) 与 `UNUSABLE`(8) 规范说「不可用」却进「可访问」的保留表；`UNACCEPTED`(15) 必须被接受后才能使用，把它当普通可访问内存是错的（机密计算平台上读取会直接故障）；≥16 的 OEM/OS-loader 区语义未知却被默认收进保留表。另 `PERSISTENT`(14)、`PAL_CODE`(13) 属「碰巧进保留表」，无显式论证。`BOOT_SERVICES_*`(3/4) 与 `ACPI_RECLAIM`(9) 永久不复用是**政策**，必须成文。采纳四轮评审的五类分组（立即可用/保留但可访问/延迟可回收/需特殊处理/不可用不应映射），原则：**不要用同一个 reserved 标签同时表达「不可分配」「需要 DM」「暂时不能回收」「根本不能访问」四种含义**。建议判据顺序改为「先类型策略、再地址过滤」。
- **I-29-8 新增：`Attribute` 字段完全未读**。`RawMemoryDescriptor` 只有 ty/physical_start/number_of_pages 三个字段（偏移 0/8/24），`Attribute`（偏移 32）从未访问；而直接映射两个窗口用固定页标志（`PRESENT|WRITABLE|GLOBAL` / `PRESENT|WRITABLE|USER`），**统一写回缓存，不区分设备内存**。对普通 RAM 无碍，对运行时服务数据/持久内存/未接受内存是空白。
- **I-29-7 收窄：`ram_top` 边界拆成三命题**。命题一（规范可能性：`AllocateMaxAddress` 只约束区间最高地址，不规定取向）无平台依赖；命题二（OVMF 实际落点）需真机样本；命题三（若出现起点等于 `ram_top` 则会被丢弃）已由读码确认。**不批准把 `<` 改成 `<=`**（四轮评审否决理由：只把边界上的所有非常规类型放进保留表，没证明它们是适合 DM 的 RAM）。边界用例应先补，固定当前行为。
- **`bootstrap_alloc_limit` 算术已核对**：三种规范读法（限 base / 限区间最高地址含 / 限末端不含）下传 `bound - size` 都**不越界**；差别只是读法 B/C 下少用约 `size` 字节。**代码安全，注释只证明了读法 A**——注释需改，非缺陷。
- **I-29-9 收窄：对象分配 ≠ 页级内存图变化**。`AllocatePool` 只在池扩容时才从常规内存取页、才产生新描述符；`FreePool` 也只归还池对象，整页回收取决于池的空闲块合并。因此 §2.5.1 的 ①③ 是**条件后果**（只有 ② root/bump 是必然后果），§2.5.8 从「已确认」降级为「待六步探针验证」。全局分配器 `UefiPoolAllocator` 每次 `alloc` 都走 `allocate_pool`、`dealloc` 为空实现。
- **路径 A 析构审查已闭合**：`MemoryMapBackingMemory::drop` 在退出分支仍调 `log::debug!`，但**两层保障**——① 本项目全仓未调用 `uefi::helpers::init()`，无 logger 注册；② `exit_boot_services` 首句 `helpers::exit()` → `logger::disable()` 把输出指针置空，`Logger::log()` 遇空指针直接返回。`boot-shim` 只启用 `uefi` 的 `alloc` 特性，无 `logger`/`log-debugcon`。

**v6 定稿修订（第五轮评审 + 本文自主判断）**：

- **I-28 可冻结**：删除 trait 与空占位、保留 `DtbMemoryMap` 与语义文档、正式 supersede I-22。实施顺序注意：`UefiMemoryMap` / `build_memmaps` 的删除范围跟随 I-29 结果，不作为 I-28 独立清理。
- **I-29-8 仍为主阻塞项（尚未收口）**：类型 → 访问策略的显式映射。v6 修正 v5 的一处事实错误并补三架构证据：① 类型数值区间分四段（`0..15` 标准 / `0x10..0x6FFFFFFF` 保留数值区**非 OEM** / `0x70000000..0x7FFFFFFF` OEM / `0x80000000..0xFFFFFFFF` OS loader——v5 的「≥16 = OEM/OS-loader」是事实错误）；② 三架构缓存属性机制各不相同：aarch64 由 `MAIR_EL1=0xFF` 的 Attr0 **显式**定 Normal WBWA；x86-64 的 `flags_to_pte` 不设 PCD/PWT 且全仓不编程 IA32_PAT，落回默认 PAT0=WB（依赖「固件未改写 PAT0」前提）；**riscv64 的 Sv39 PTE 没有缓存位**，缓存性由平台 PMA 决定——v5「统一 WB」在 RISC-V 上是假设不是结论，已改；③ `EFI_MEMORY_RUNTIME` 是独立于 MemoryType 的属性（用运行时服务就必须处理其映射要求）。采纳四状态处置（`Available`/`ReservedMapped`/`Excluded`/`Special`）+ 第一版政策表草稿（§2.5.7.1），`RESERVED`/`UNUSABLE`/`UNACCEPTED`/未知类型不得默认进普通保留表。
- **I-29-7 定性升级**：`ram_top` 是「起点过滤启发式」——只查起点，不证明描述符整体在 RAM 包络内（形态二：越界尾部整段收下不截断），也拦不住包络内的非 RAM 类型（形态三：包络内的 `RESERVED` 照样进保留表）。**暂不裁决，不批准 `<=`**；边界用例以 characterization test 固定当前行为，contract test 跟随最终政策表，勿让旧行为变成新契约。
- **`bootstrap_alloc_limit` 算术更正**：规范只有一种读法（分配区间最高地址 ≤ 输入值），v5 的「三种读法」不成立。按规范传 `bound - 1` 即足够；当前传 `bound - size` 更严格（差额 `size - 1` 字节），**可能造成不必要的分配失败**（v5 未写出的后果）。两个选项待裁决（改 `bound - 1` / 保留并写成显式保守策略）；无论选哪个，现有注释「保证 base 不超过 limit」的错误规范读法必须删。
- **路径 A 两处前提降级**：① `MemoryMapOwned` 缓冲——`allocate_pool` 无地址上界，`LOADER_DATA` 只证明资源归属，不证明落在自建页表覆盖内；退出后至分类期间的访问需核实（测试内核顺序上大概率成立，但那是顺序事实非结构保证）；分类完成后无需纳入内核 DM。② 静态输出数组——「出现在快照里」≠「内核消费期间可访问且不被 PMM 分配」；且镜像区间可能恰落 §2.5.6.1 形态一边界（不在可用表也不在保留表），「不在可用表」断言会通过但 DM 访问会缺映射。验收从五项增到**六项**（新增「输出生命周期」）；夹具自举检查扩到输出数组地址范围，不只查 root/bump。
- **析构审查维持闭合**（五轮评审认可两层保障）；实现约束措辞收紧：「EBS 成功返回后，分类期间允许持有并读取 `MemoryMapOwned`；其析构不得尝试调用 Boot Services；分类器、输出容器与 fail-stop 路径不得触发动态分配或依赖 Boot Services 的日志输出」。
- **v6 本文自主判断**（§2.5.7.2，超出评审意见的三条）：① `Excluded`/`Special` 处置必须可观测（计数 + 类型分布走返回值），否则重演「描述符静默消失」；② 类型→处置写成编译期穷尽 `match`（枚举 + 三个后备变体），新增标准类型变编译错误而非运行时漂移；③ `ram_top` 降级为「结果裁剪」而非「准入门槛」——先按类型得集合，再用 RAM 包络裁剪结果，三种形态各有归宿且不需新抽象。另补 §2.5.9 两处注释修正：分配器注释「固件整体回收」错误（退出后是 loader-owned 交给 OS，不自动变常规内存——该误解易被下游误用为「shim 占的内存会自动还回来」）；`build_memmaps` 注释把「内核先深拷」说成万无一失（只保证内容不被覆盖，不保证可用表干净）。
- **测试规范同步**：§2.6.4 的「保留表符合 RAM 顶规则」已删除，改为按政策表的六条验收（只有 Conventional 进可用表 / 只有明确允许的类型进保留表 / 排他类型不得进 / MMIO 走设备路径 / 区间契约 / 类型×属性组合符合政策）。

**研究顺序（v6 按五轮评审重排，§7）**：① 修文档规范事实（v6 已完成文档侧，代码注释待改）→ ② 定策略表（草稿 §2.5.7.1，待裁决）→ ③ 核三架构映射落实（部分已核，PAT/MTRR 初始化与 RISC-V PMA 假设待核）→ ④ QEMU+OVMF/AAVMF 运行样本（不称「真机」；四时点含 `Attribute`；探针用预分配输出避免自污染）+ 路径 A 六项验收实验 → ⑤ 最小接缝原型。**样本不能回答政策问题**（未知类型是否有资格进 DM、Runtime 描述符如何处理、三架构如何落实属性）——所以 ②③ 在 ④ 之前。bootmarks 路标合并不在阻塞项内。

**v7 定稿修订（第六轮评审；一条是本文撤回自己的提议）**：

- **v7 撤回 v6 的「`ram_top` 结果裁剪」提议（§2.5.7.3），定稿为从分类器中删除**。评审的反驳成立且关键在于：v6 没说清 RAM 包络用什么数据构造。两种取法都错——① 包络 = Conventional 并集：`AllocatePages` 本来就把页移出 Conventional，刚分配的根页表页 `[B,C)` 与包络交集**恰为空**，最需要保护的自举内存反而被裁掉；② 包络 = `[0, ram_top)`：不证明区间内每个地址是 RAM，且 `ram_top` 只反映**剩余可用**内存的末端，不代表系统 RAM 最高地址（高地址 RAM 可已被固件占用）。更深一层：UEFI 内存图本身就是更直接的分类来源（无硬件支持的地址无描述；不可访问区报 `RESERVED`），类型字段承接 `ram_top` 的保护职责（芯片组空洞 → `Excluded`），地址维度只剩 `PA_LIMIT`（架构能力检查）。
- **I-29-8 政策表升为 v7 讨论基线**：四状态补充**两个正交维度**（分配策略 × 映射策略）——`ReservedMapped` 不再同时表达「不可分配」与「需要普通 DM」；`RUNTIME_SERVICES_*` 定为 `Special`（当前不支持运行时服务调用，其内存不可分配但不因此自动进普通 DM）；`UNACCEPTED`/`PAL_CODE` 收紧为「**存在即拒绝启动**」（与 §1.5 批评空表占位同一原则：边界处大声失败好过深处静默出错）；未知类型 = 分类错误。
- **可观测性细化**：`Excluded` 允许统计排除（计数 + 字节 + `first_n_diagnostics` 固定容量数组：raw_type/physical_start/number_of_pages/attribute）；`Special` **不能只返回计数**（地址范围会丢，将来无法从统计数恢复）；未知类型直接触发分类错误。
- **穷尽 `match` 的前提修正**（v6 的写法不成立）：`_ =>` 通配会吞掉新增标准类型。四条件缺一不可：`KnownMemoryType` 枚举（0–15 各一变体）+ 从 `u32` 安全转换（不得直接转枚举）+ 分类函数**无通配**穷尽 `match` + 未知值单独携带原始值。另按评审用词：`0x10..0x6FFFFFFF` 称「未分配／保留数值范围」，不称「未来标准类型集合」。
- **`bootstrap_alloc_limit` 裁决选 A**：改 `bound - 1`（`bound` 已是安全上界，无需再损失约一个请求大小的空间）。四前提核对（bound>0 / size 页对齐非零 / 返回页对齐故规范约束落在最高字节 / bound 是半开末端而 UEFI `Memory` 含末字节——两者差一故传 `bound - 1`）。随附两条单测。
- **`AllocatePages` 措辞再修**：必然**改变内存图内容**，但不保证描述符数量增加（整段覆盖时固件可直接改写该条类型）；§2.5.1/§2.5.2 已同步。
- **三架构证据再收紧**：aarch64 基本闭合，留一项待查（所有 DM 叶子的 `AttrIndx` 是否都为 0）；x86-64 的「落回默认 PAT0=WB」不能写成运行时事实——boot-shim 运行在固件之后，固件可能改写过 PAT，且有效类型需与 MTRR 共同决定；riscv64 的「PTE 无缓存位」过于绝对——Svpbmt 定义了 PBMT 位（62–61），准确说法是「本仓当前编码不使用 PBMT」，PMA 平台相关并非默认 WB。OS 应自行建立依赖的内存类型环境（读/重设 IA32_PAT、检查 MTRR），不把固件状态当永久合同。
- **`Attribute` 消费决策（新增 I-29-10）**：方案 A（仅准入校验，输出保持 `{base,len}`，不改交接协议）为第一版推荐，显式限制成文；**来源分层**（新增 I-29-11）：UEFI 类型政策与 DTB 来源政策各自成文汇入 OS 统一内存语义（§2.5.7.6 分层图）——`MemoryType+Attribute` 是 UEFI 适配器输入，不是三架构共同输入；RISC-V 走设备树不经此分类。DTB 侧政策尚未成文，需补。
- **v7 剩余工作收敛为四件**（§7）：① 删 `ram_top`（随政策同批）；② 定稿政策表；③ 定稿三架构普通内存映射合同（与②并行，可能反向修改政策表——架构表达不了就降级 `Excluded`）；④ 明确 `Attribute` 交接方式（方案 A/B）。不再开新议题；I-28 可冻结；I-29 设计原则冻结、实施设计待四件完成。

**v8 定稿修订（第七轮评审；七处，一处规范事实 + 一处会影响分类器行为）**：

- **§2.5.7.3 规范推论修正**：无物理硬件支持的地址空洞**通常没有描述符**，不是「一定报告为 `RESERVED`」；「有硬件但 OS 不应访问」才必须报 `RESERVED`。删除 `ram_top` 的结论不变；i440fx 的 `RESERVED` 空洞是单点固件样本，不能推广成「所有空洞都报 RESERVED」。
- **政策表定稿为三列**（分配策略 × 普通 DM × 失败处置），消除两处不一致：① `ACPI_NVS` 原写「ReservedMapped + 按消费者决定映射」与定义自相矛盾——改为「分配=保留；普通 DM 由 ACPI NVS 消费者存在性决定（当前无消费者→不建）」；② `Special` 各行补失败政策——`RUNTIME_SERVICES_*` **不得因当前不调用运行时服务就拒绝启动**（普通 UEFI 机器正常出现，保证不进 PMM 即可）；`PERSISTENT` 保留完整范围信息（不能只留计数）；`UNACCEPTED`/`PAL_CODE` 维持存在即拒绝启动；未知类型 = 分类错误。四状态保留为**文档术语**，代码承载分配/映射/支持状态三个独立判断。
- **`Attribute` 准入具体化（I-29-10）**：进 `Available`/`ReservedMapped` 必须证明 `EFI_MEMORY_WB` 已设置（不得仅凭类型是 CONVENTIONAL/LOADER_DATA 就按 WB 映射）；`RUNTIME` 独立于类型判断（`CONVENTIONAL + RUNTIME` 组合不得进 `Available`，宿主测试必测）；`NV`/`SP` 独立处理；`RO/RP/XP` 是能力位不是当前权限；ISA 位不得静默忽略；`EFI_MEMORY_ATTRIBUTES_TABLE` 登记为未来边界。
- **RISC-V 补 PBMTE 启用链（I-29-8 三架构合同）**：使用 PBMT 需同时验证扩展实现 + `menvcfg.PBMTE=1`（S-mode 可用性由 M-mode 位控制）+ OpenSBI 是否已建立；且 PBMT 取值只有 PMA/NC/IO，**没有「强制 WB」的取值**——普通 RAM 缓存性仍依赖平台 PMA。
- **取消 `ram_top` 的 characterization test**（v7 曾提议保留）：旧行为已被 §2.5.7.3 判定为错误而非「待裁决」，保留测试席位反而增加维护成本。改为 contract test 直接验证新政策（5 条用例，「旧 `ram_top`」仅作测试数据构造参数）。
- **两遍扫描改出新理由**：旧理由（第一遍算 `ram_top`）随删除失效；新理由是事务性（先验证全部待输出描述符再写输出）。是否保留两遍归输出接缝原型决定。
- **`bootstrap_alloc_limit` 保留 `size` 参数**（v7 示例丢掉了原 `checked_sub(size)` 的请求校验，v8 修正）：`size > 0` / 页对齐 / `size <= bound` 三断言 + `bound - 1`；`PA_LIMIT` 与 `bound` 统一半开末端语义。
- **DTB 来源政策明确为 I-29-11 验收**：分层图中 DTB 侧已核事实只有 `/memory` 的 `reg` 解析 + 固件前缀裁剪；`reserved-memory` 子树、`no-map`、重叠检查**无证据**，且 DTB `reserved-memory` 不能直接等同 UEFI `ReservedMapped`。登记为后续独立问题，不阻塞 UEFI 侧实现。

**v9 定稿修订（第八轮评审；四个实现缺口 + 两个事实修正 + 三处残留清理）**：

- **I-29-13 新增：数据合同贯通**。v8 要求属性准入却没把 `attribute` 放进 `DescriptorRef`——「唯一解析器」与「属性准入」两条要求没接上。v9 定稿数据合同：`DescriptorRef { raw_type, physical_start, number_of_pages, attribute }`，由唯一生产解析器完成字段边界验证与 `physical_start` 4 KiB 对齐校验（UEFI 对描述符字段有明确对齐要求，现有代码只查了 `descriptor_size` 对齐）。**I-29-2/3 冻结前必须补上**。
- **I-29-14 新增：特殊范围输出落点**。三行「保留但不映射」（ACPI_NVS / RUNTIME_SERVICES / PERSISTENT）不属于现有两格；v9 裁决加 side list 或第三种 sink 写入承载完整范围，`PERSISTENT` 必须有范围清单（不采用「有意遗忘」选项）。**与接缝原型互为前置**——结果形状定了才能定政策表。
- **I-29-12 新增：PA 表达能力 vs DM 覆盖分两个合同**。`PA_LIMIT` 只查架构页表可表达范围（半开语义）；DM 覆盖由映射规划/验收层独立保证。`ReservedMapped` 与 `Available` 的每个区间都需 DM 覆盖验收（可分配性与可访问性不能脱节）；分类器不做覆盖检查，但交接与映射合同必须补上。
- **I-29-10 补穷尽属性表（§2.5.7.7）**：补 `WP`（记录，不静默忽略）、`MORE_RELIABLE`/`CPU_CRYPTO`（登记「当前不解释」）；`ISA_VALID` **出现即分类错误**（fail-closed，三架构都不解释 ISA 掩码）；`WB` 缺失逐类处置——`CONVENTIONAL` 不进 `Available`，`LOADER_DATA` 不能一刀切（引导装载仍需读取，走恒等窗口是否可行需逐腿核实）；`CONVENTIONAL + RUNTIME` 完整结果 = 不进 PMM、不建普通 DM、保留完整范围（`RUNTIME` 不禁止普通 DM，但项目当前不支持运行时服务）。
- **`AllocatePool` 扩容来源按规范收紧**：UEFI 明确扩容从 `EfiConventionalMemory` 取页——§2.5.3 三情形收成两行，「扩容取自非常规区域」不是规范允许的分支。风险更确定：扩容一旦发生，backing pages 必然落在旧快照可用表里。待测问题收窄为三个：是否扩容、扩多少页、是否落入变化窗口。
- **三处版本残留清理**：§6.11 / §4 事实 23 / 附录 A 的 RISC-V 旧说法统一为「本仓 `flags_to_pte` 不编码 PBMT（Svpbmt 定义了 62–61 位，S-mode 受 `menvcfg.PBMTE` 控制）」；x86 统一为条件句「若 PAT0 未被固件改写且 MTRR 允许，才表现为 WB」；§6.9 旧公式解释改为「当前代码快照，目标实现 `bound - 1`，真正的不变量是 `base + size <= bound`」——不再为 `bound - size` 背书。
- **§5.2 文档矛盾修正**：I-29-8 从「v8 政策表定稿 + 待逐行批准」的冲突表述统一为「政策基线已形成，尚待最终批准」。

**v9 剩余工作（第八轮评审明确：不再泛化抽象设计）**：① 修正分类输入与输出合同（I-29-13/14/12）；② 完成属性政策矩阵（穷尽到「五位必居其一」）；③ 清理事实摘录与注释（§6.11/事实23/附录A/§6.9，v8-v9 已完成文档侧）；④ 两个小原型——生产解析器接静态 sink（确认零分配与容量错误）、路径 A 跑通（退出后缓冲、输出数组生命周期、映射覆盖）。

**v10 定稿修订（第九轮评审；一处规范事实错误 + 四项合同补齐）**：

- **`EFI_MEMORY_WP` 规范归类修正**：它是**缓存性能力位**（write-protected cacheability policy，UEFI 2.5+ 用法），不是与 `RO`/`RP`/`XP` 同类的物理保护状态——v9 把它放在「物理内存保护属性」下又与 RO/RP/XP 区分，是分类错误。确立术语纪律：不得把「保护能力」与「缓存策略能力」混称权限属性（否则讨论 PAT/MTRR 与 MAIR 时两种语义会搅在一起）。
- **规范版本基线显式定为 UEFI 2.10**：穷尽表只承诺覆盖 2.10；2.11+ 新位（如 `EFI_MEMORY_HOT_PLUGGABLE`）按未知属性政策 fail-closed，不为「穷尽」引进整个 2.11 体系；升级是独立政策决定。补 `ISA_MASK` 非零而 `ISA_VALID` 为零的组合同样判分类错误。
- **`WB` 缺失逐类处置重写（无后门）**：v8 的「其余类型不新增后果」违反总原则——`LOADER_CODE`/`BOOT_SERVICES_*`/`ACPI_RECLAIM` 缺 WB 仍会进普通保留映射。v10 补全逐行：全部「不建普通 DM + 范围进 side list」，明确**不存在从属性准入失败绕回普通映射的后门**。属性裁决顺序成文：先未知/不支持条件，再独立属性（`RUNTIME`/`NV`/`SP`）覆盖，最后 WB 准入。
- **side list 生命周期修正**：v8 的「为未来保留」不成立——不进 KernelInfo 也不传内核的清单没有为未来内核功能保留范围。v10 定稿：**shim 内部诊断输出，不构成跨引导边界的保留协议**；未来内核消费这些区域必须扩展 KernelInfo 或另建交接途径。成员由分配 × 映射策略决定，不写死三个类型名。
- **I-29-15 新增：两遍扫描的 preflight 合同**。Pass 1 必须预检**所有**第二遍可能失败的条件（可用/保留/side list 容量、wire+semantic 双写容量、整数转换）——v8 的「两遍天然事务性」未闭合；Pass 2 不可失败的提交合同。给 sink 示意合同（`preflight`/`push_available`/`push_reserved_mapped`/`push_unmapped_reserved`/`finish`）。
- 版本标记清理：§2.6.2/§2.5.7.7/§2.5.7.8/§5.2/§7 的「v8 修正」字样更新为 v9/v10。

**v10 剩余工作（五件）**：§5.3 表更新——① 输入输出合同（I-29-13/14/12）；② 属性政策矩阵（基线 2.10 + WP 归类 + ISA 组合 + 裁决顺序 + 无后门）；③ 事实摘录清理（v8/v9 已完成文档侧）；④ 两个小原型；⑤ preflight 合同（随原型 ① 落地）。**政策表逐行批准时按「任何 Type × Attribute 组合必须唯一确定输出」验收**。

**v11 定稿修订（第十轮评审；五类修订 + 宣告冻结分层——设计方向收口，转原型）**：

- **补齐独立属性的「最终普通映射结果」（§2.5.7.7 新表）**：v10 只写清了 `CONVENTIONAL + RUNTIME` 的完整结果，`NV`/`SP` 只定分配策略未定映射策略，`RUNTIME` 未逐类展开——与「side list 成员由分配策略 × 映射策略决定」自相矛盾。v11 定稿统一规则：`RUNTIME`/`NV`/`SP` 任一命中 → 不进 PMM + 不建普通 DM + 范围进 side list，对**所有** MemoryType 统一适用；评审点名的三个组合（`CONVENTIONAL+NV`、`CONVENTIONAL+SP`、`LOADER_DATA+RUNTIME`）逐行落定。
- **属性组合测试系统化（§7）**：v10 的「随机采样若干组合」降为补充手段；政策定义过的维度交叉（WB 有无 / `RUNTIME` / `NV` / `SP` / `ISA_VALID`×`ISA_MASK` 合法非法组合 / 未支持属性位 × 全部受影响 MemoryType）必须穷测——否则「任何组合唯一确定输出」只是设计宣言。
- **preflight 合同失败语义修正（§2.6.2）**：v10 示意合同的 `finish(self) -> Result<Classification, OutputError>` 与「Pass 2 不可失败提交」在类型表达上自相矛盾。v11 裁决为**类型层面表达**（`finish` 预检全过后返回普通结果，或 Pass 1 产出经验证的提交凭证再进入写入），具体形态不在文档预决，归原型 ① 比较。
- **`WP` 规范背景补全（§2.5.7.7）**：UEFI 2.10 仍保留 `EFI_MEMORY_WP` 历史上的 "Physical memory protection attribute" 标签，同时注明它通常用于缓存策略并区分 `RO`/`WP` 的现代用途——保留这一规范背景，避免读者误以为规范从未如此归类；v10 的「缓存性能力位」归类结论不变。
- **清理 `ram_top` 残留句（§2.5.6.1）**：「最多在类型策略定案后作为辅助过滤」与 §2.5.7.3 的撤回裁决矛盾，属版本残留，已删。
- **I-29-16 新增（§5.2）：三架构普通 WB 映射的最低保证**。UEFI 2.10 的 `Attribute` 是区域支持的属性能力而非当前设置，`EFI_MEMORY_WB` 只证明固件声明支持，不证明项目最终建立的映射是 WB（x86 PAT/MTRR 未读取校验、aarch64 DM 叶子 `AttrIndx` 未确认、riscv64 不编码 PBMT 且 PMA 平台相关）。三架构各自写成可验收合同（x86：PAT/MTRR 前提的读取或自行初始化；aarch64：全部叶子 `AttrIndx` 核查；riscv64：PMA 声明），与原型并行——该合同成立前政策表不得宣告完全冻结。

**冻结分层（第十轮评审裁决，本文采纳）**——**已冻结**：I-28（含正式 supersede I-22）；I-29 的唯一生产解析器/唯一分类规则、零分配分类、预分配输出、`ram_top` 从分类器删除、UEFI 与 DTB 各自适配汇入共同 OS 内存语义、`PA_LIMIT` 与 DM 覆盖两个合同分离、`DescriptorRef` 含原始 `Attribute` 并统一几何校验、属性准入方案 A（不扩展公共交接协议）、side list 语义（shim 内部诊断输出，非内核交接协议）。**暂不冻结**：UEFI 2.10 属性政策矩阵（等逐行批准 + 原型 ① 验证）、三架构普通 WB 映射合同（I-29-16）、Pass 1/Pass 2 具体 API（目标合同冻结，API 随原型）；side list 的具体存储/容量/sink API 随原型确定；DTB `reserved-memory`/`no-map` 仍为 I-29-11 后续独立问题，不阻塞 UEFI 分类器。**v11 封存为阶段性基线，不再有第十一轮纯文档讨论。**

**v11 剩余工作（三件事 + 两个原型）**：① 补齐 `NV`/`SP`/`RUNTIME` 的映射去向（§2.5.7.7 表已落文，随政策表逐行批准收口）；② 三架构普通内存映射最低保证 → 可验收合同（I-29-16，与原型并行）；③ preflight API 失败语义的类型层面表达 + 原型验证。原型 ① = 生产解析器接静态 sink（零分配、容量错误、side list 第三种写入形状、preflight 两种形态比较）；原型 ② = 路径 A 端到端（退出后缓冲、输出数组生命周期、映射覆盖，§2.7 六项验收）。代码侧待改随实施：`bootstrap_alloc_limit` 改 `bound - 1` + 删错误注释；`UefiPoolAllocator`/`build_memmaps` 注释修正（§2.5.9）；`classify_map` 落地类型政策与属性准入；I-22 正式 supersede 记录。

**v12 终审修订（第十一轮终审；三处修订 + 正式冻结宣告——设计讨论阶段结束）**：

- **裁决顺序细化（I-29-17 新增，目标合同冻结）**：v11 的「任意 Type + RUNTIME/NV/SP」与 §2.5.7.1 的类型强制处置之间优先级悬空（`RESERVED+NV`/`UNUSABLE+SP`/`MMIO+RUNTIME`/`PAL_CODE+NV` 四组合无答案）。v12 落定五步顺序：① 未知/ISA fail-closed → ② `UNACCEPTED`/`PAL_CODE` 拒绝启动 → ③ 类型强制处置（`RESERVED`/`UNUSABLE` 排除、`MMIO`/`MMIO_PORT_SPACE` 设备路径）——**类型胜过属性**（类型回答「是什么」，属性回答「怎么用」；属性无法把「不可触碰」改判成「因政策不映射」，误进 side list 会把排除原因记成属性原因）→ ④ 普通内存候选 × 独立属性（候选外延 = `CONVENTIONAL`/`LOADER_*`/`BOOT_SERVICES_*`/`ACPI_*`/`RUNTIME_SERVICES_*`/`PERSISTENT`）→ ⑤ WB 准入。四组合逐行落定：`Excluded`/`Excluded`/设备路径/拒绝启动。
- **ISA 测试维度与政策表对齐（§7）**：v11 维度表把 `ISA_VALID=1` 标成「合法」与 §2.5.7.7「出现即分类错误」矛盾；v12 改三分支（`ISA_VALID` 出现无论掩码 / `ISA_MASK`≠0 且 `ISA_VALID`=0 / 两者均未出现）并分别写明预期结果（前两分支分类错误，第三分支正常裁决）。
- **§7 工作清单与 §5.3 对齐**：删除残留旧「四件事」清单（含已冻结的「删 `ram_top`」）；§7 重写为冻结后收敛路径 = 五步执行序列（原型 ① → 原型 ② ∥ 三架构 WB 合同 → 政策矩阵逐行批准 → 冻结实现规格进生产实现）+ 穷测矩阵（补「裁决顺序组合」维度行，四具名用例必测）。
- **正式冻结宣告（终审裁决落文）**：**I-28 设计冻结**（删除 trait 与空占位、保留 `DtbMemoryMap` 与语义说明、正式 supersede I-22——记录属实施动作）；**I-29 架构设计与目标合同冻结**（唯一解析/分类、零分配 + 预分配输出、`ram_top` 删除不留 characterization test、方案 A、PA/DM 两合同、`DescriptorRef` 含 attribute、side list 语义、五步裁决顺序）。**暂缓冻结（各有明确验收门，不是架构没想清楚）**：UEFI 2.10 属性政策矩阵逐行批准（凭原型 ① + 三架构证据）、三架构普通 WB 合同（I-29-16）、Pass 1/Pass 2 具体类型形态（目标合同已冻结，归原型 ①）、路径 A 端到端运行验收（API 可行性已核实，归原型 ②）。**不再有纯文档讨论轮，剩余问题由真实代码和实验裁决。**

**证据与方案展开**：`rewrite-notes/01-stage-kernel/01-design-note-memmap-consolidation.md`（**v12 冻结版**：篇头冻结宣告、§2.5.7.7 五步裁决顺序 + 四组合表、§5.2 I-29-17、§5.3 执行序列、§7 穷测矩阵三分支 ISA + 裁决顺序行）。**状态**：I-28 设计冻结；I-29 架构设计与目标合同冻结；属性政策矩阵 / 三架构 WB 合同 / 具体 API / 路径 A 验收暂缓（各有验收门）；**下一步 = 原型 ① 与 ②**。

### I-33（⚠ 待裁决——实施受阻，原裁决的前提不完整）直核腿统一到 owned 路径，删第二套静态表

**裁决**：用户 2026-11-20，判决行 `D-065`。**状态：已批准待实施**。来源：`RD34-DESIGN-D2-03`（P2，[MODERN]）。用户表述：「统一总是我的最优选」。

**现状**：`DtbMemoryMap` 有三个输出入口，五条腿分成两套。

| 腿 | 现在用 | 数据落点 |
|---|---|---|
| x86 UEFI、aarch64 UEFI、U-Boot shim | `collect_owned()` | `BootStorage` 槽位 |
| **riscv 直核、aarch64 直核** | `collect_with_sources()` | `DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES` 静态表 |

改后五条腿全部走 owned。

**做法**：

1. `os/kernel-image/src/bootface.rs:213` 与 `bootface_a64.rs:203-204` 的 `collect_with_sources()` 改成 `collect_owned()`；
2. 按 U-Boot 腿的三步（`opensbi_helpers.rs:299-320` 是模板）：`owned` → `audit_source_coverage` → 拷入 `storage.map_tables_mut()` / `set_memmap_sources_len` 提交；
3. `build_kernel_info` 的三个实参（`bootface.rs:428-430` 的 `memmap`/`reserved`/`sources`）改读 `storage.*()`；
4. 删 `memmap_source.rs` 的 `DTB_FREE`（`:160`）/`DTB_RESERVED`（`:164`）/`DTB_SOURCES`（`:619`）、三个 `AtomicUsize`（`:223-224`/`:632`）、`collect()`/`collect_with_sources()` 与第二套一次性守卫（`:1360-1395`）；
5. 文档 §3.4.5 删掉「让 DTB 收集器保持单一写入点」这条理由——**它是错的**（U-Boot 腿用 `collect_owned` 同时做到了单一写入点与全量 owner）。

**关键澄清（讨论中查实，纠正面板估算）**：

- **不是「增加 9 KiB」，是「净减 3 KiB」**。两个直核腿**早已持有完整 `BootStorage`**（`bootface.rs:147`、`bootface_a64.rs:148` 的 `static STORAGE: BootStorageCell`），其中 memmap(128)/reserved(256)/sources(40) ≈ 9024 B 一直闲放在 `.bss`；改后只是从「空着」变成「写入」，不增一字节。删掉的三张 `DTB_*` 静态表约 3136 B 释放，**净减约 3 KiB**。对省空间的嵌入式场景是改善。
- **顺带消除一个行为差异**：直核腿 free 表上限 `DTB_MEMMAP_MAX = 8`（`memmap_source.rs:46`），v1 腿 `WIRE_MEMMAP_CAP = 128`（`handoff_v1.rs:375`）。同一份 `/memory` 描述超过 8 段的板子，直核启动 halt 而 v1 腿正常启动。`DtbMemMapError::TooManyFreeRegions` 的注释明写容量是策略输入、不允许静默截断。

**验收**：
- `grep -rn "DTB_FREE\|DTB_RESERVED\|DTB_SOURCES\|collect_with_sources" os/ --include="*.rs"` → 0；
- 两个直核腿的 `KernelInfo.memmap` 指向 `BootStorage` 落地区（可在 bootface 里断言切片指针落在 storage 范围内）；
- `cargo test -p minix-boot`、`cargo test -p boot-shim --features test-all` 全绿；
- riscv 直核 bootface 门与 aarch64 直核 SMP 门 PASS（含 `-kernel` 直核形态不回归）；
- `bootface_common.rs:16` 的 16 KiB `.bss` 断言仍通过（改为净减后应更宽松）；
- 文档 §3.4.5 不再有「单一写入点」这条理由。

**风险与回退**：直核腿从「发布静态表」变成「写入 storage」，须确认没有其他代码依赖 `DTB_*` 的符号存在（`grep` 已确认 `os/kernel-image` 与 `os/kernel` 零命中）。若某腿的 `/memory` 段数超过 `WIRE_*_CAP`，会从原来的 `TooManyFreeRegions` halt 变成 storage 容量断言 halt——两者都是 fail-closed，但阈值不同（8 → 128，放宽）。

**⚠ 2026 年 11 月 20 日实施受阻 —— 状态改回「待裁决」**：

按本规格动了两个直核腿（`bootface.rs` / `bootface_a64.rs` 的 `collect_with_sources()` → `collect_owned()` + 拷入 `storage`），代码可编译、`cargo test -p minix-boot` 195 通过 / `cargo test -p boot-shim --features test-all --lib` 136 通过。但继续做「删第二套静态表」时发现规格的前提不完整，**改动已全部回退**（`git checkout` 复原，工作树无本条改动）。下面是受阻的硬证据。

**规格的第 1 处遗漏：不是三张静态表，是四张，且它们共用一个写入点。**

规格列出的删除目标是 `DTB_FREE` / `DTB_RESERVED` / `DTB_SOURCES`，但 `memmap_source.rs` 里还有第四张 `DTB_PANORAMA`（`:638`），它和前三张在同一个 `commit_static()`（`:1363`）里一起写：

```
:1367            DTB_FREE = arrays.free;
:1368            DTB_RESERVED = arrays.reserved;
:1369            DTB_SOURCES = arrays.sources;
:1370            DTB_PANORAMA = arrays.panorama;
```

三个 `collect` 入口（`collect` `:925`、`collect_with_panorama` `:940`、`collect_with_sources` `:967`）全部走 `collect_inner` → `commit_static`（`:1438`）。只删三张而不动第四张，`commit_static` 仍在、机制没消失。

**规格的第 2 处遗漏：`DTB_PANORAMA` 有活的生产消费者。**

`os/boot-shim/src/resv_claims.rs:295`：

```rust
let Ok((_, _, sources, panorama)) =
    minix_boot::DtbMemoryMap::new(&fdt, 0).collect_with_panorama()
```

它只要 `sources` + `panorama`（`free`/`reserved` 用 `_` 丢弃），跑在 `ebs_pair.rs:193` 的 EBS 重试循环里（`EBS_ATTEMPT_LIMIT = 2`，`:55`）、每次 attempt 前先归零自己的记录。这是「EBS 之后的一次性静态表」——`collect_inner` 的 `DTB_COLLECT_TAKEN` 守卫（`:1427-1433`，`cfg(not(test))`）对它会构成第二次 `collect` 的直接冲突。

**规格的第 3 处遗漏：`DtbMapOwned` 不带 panorama，所以「全部走 owned」不是一行改动。**

`DtbMapOwned`（`:192-199`）只有 `free`/`reserved`/`sources` 三个切片，没有 `panorama`；而 `DtbArrays`（`:176-186`）里已经有 `panorama`/`panorama_n`。要让它也走 owned，需要给 `DtbMapOwned` 加两个字段与一个访问器，再改 `resv_claims::capture` 的调用形状。这是**规格未授权的类型扩展**（D-065 只裁了「直核腿统一到 owned」）。

**规格的第 4 处遗漏：测试改写面是 62 处，不是「测试里的等价物按新 API 重写」一句能覆盖的。**

- `memmap_source.rs` 内 `.collect()` 调用 **62 处**，其中 `DtbMemoryMap::new(&fdt[N])`.collect()` 形态 **42 处**；
- `collect_with_panorama` / `collect_with_sources` 在测试里还有 **17 处**。

全部要改成 `collect_owned()` 并接受 `DtbMapOwned` 的访问器形态——这比「删三个 static」是多一个数量级的工作量。

**规格的第 5 处遗漏（我在实施中撞到的真实编译约束）**：直核腿改成 owned 之后，`build_kernel_info`（`os/boot-shim/src/lib.rs:101-113`）的十个参数里有五个要求 `&'static`：

```rust
memmap: &'static [minix_boot::MemoryRegion],
reserved_regions: &'static [minix_boot::MemoryRegion],
...
memmap_sources: &'static [minix_boot::SourceRecord],
startup_objects: &'static [minix_boot::SourceRecord],
```

所以直核腿仍要在末尾用 `from_raw_parts` 从 `storage` 造五个 `'static` 视图（与既有的 `modules_ptr` 块同位置）。而这三个 memmap 视图不能在早段（`in_memmap_range` / `place_modules` 要用）就建，否则 `storage.module_slots_mut()`（`:295`）、`set_modules_len`（`:314`）、`platform_slots_mut()`（`:325`）、`push_startup` 全部撞 `E0502`（借出冲突）。正确形状是「早段读 `owned` 的借用切片、末尾建 `'static` 视图」，这使实现比规格设想的「改三个实参」复杂，也引入两套名字（`memmap_early` / `memmap`）。

**待用户裁决的问题（三选一）**：

| 选项 | 内容 | 代价 |
|---|---|---|
| **A. 扩权到 panorama** | 给 `DtbMapOwned` 加 `panorama`（字段 + 访问器），`resv_claims::capture` 改 `collect_owned()`，然后四张静态表 + `commit_static` + 三个 `collect_*` 入口一次删净；62 处测试改 `collect_owned` | 规格未授权的类型扩展 + cross-crate（`minix-boot` / `boot-shim`）+ 大批测试改写；但只有这样才真正「一套机制」 |
| **B. 缩小 I-33 范围** | 只做「两个直核腿改 owned」（可编译可测，已验证），静态表与 `collect_*` 入口**明确保留**并改注释说明「panorama 消费者仍在」；I-33 的验收 grep 从「`DTB_*` → 0」改为「直核腿不再调 `collect_with_sources`」 | 保持规格边界，但「第二套机制」的抽象泄漏只修一半；`collect_with_sources` 变成零生产调用（仅测试用），迟早要再裁一次 |
| **C. 另立新项** | 本条只保留直核腿统一（同 B）；把「panorama 消费者改 owned + 删四张静态表 + 62 处测试」立为新条目，单独裁决与排期 | 与 B 的实施内容相同，区别是 backlog 形态更诚实（一个大项拆成两个可独立验收的项） |

**我的判断**：倾向 **C**。理由是本条规格的「关键澄清」已经把两件事混在一句里（「五条腿全部走 owned」与「删第二套静态表」），而它们的实际耦合度比规格假设的高：静态表服务于 panorama 消费者，不服务于直核腿。拆开后，第一件（直核腿统一）今天就能独立验收，第二件（去静态表）需要连 `resv_claims` 一起裁，值得单独一轮。

**当前状态**：代码改动已回退（`git checkout -- os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs`），工作树无本条改动。`grep -c "collect_with_sources" os/kernel-image/src/bootface.rs` → 1（原样）。

**实施记录（2026 年 11 月 20 日，实施后回退，`⚠ 待裁决`）**：仅记录受阻过程与上述五处遗漏，不改变代码。

**为什么此前不做**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥。裁决已生效，实施留待下一 design/code batch——现在因为上述前提缺失，需要先补裁再排期。

### I-32（已裁决·可立即开工）U-Boot 腿两个安装型全局：检查强度对齐

**裁决**：用户 2026-11-20，判决行 `D-064`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**——`OPEN-DEBT.md` 的 `R34-design-batch` 行已注记。来源：`RD34-DESIGN-D2-02`（P2）。

**做法（路线 B）**：把两个 `debug_assert!` 改成 `assert!`，并给 `DTB_PTR` 补同形检查；结构不动。

```rust
// os/boot-shim/src/opensbi_helpers.rs:490-506（现状；debug_assert! 在 :500）
pub unsafe fn install_boot_file_table(addr: u64) {
    debug_assert!(BOOT_FILE_TABLE_PTR == 0, "install_boot_file_table called twice");
    unsafe { BOOT_FILE_TABLE_PTR = addr as usize; }
}

// 改为：assert!（release 下同样 fail closed）
pub unsafe fn install_boot_file_table(addr: u64) {
    assert!(BOOT_FILE_TABLE_PTR == 0, "boot-shim: install_boot_file_table called twice");
    unsafe { BOOT_FILE_TABLE_PTR = addr as usize; }
}
```

`install_dtb_ptr`（`:547`，单行 `debug_assert!`）同形改法。

**为什么选路线 B 而不是参数化**：用户约束是「多架构函数签名尽可能一致」。`prepare_boot` 是 U-Boot 腿唯一入口，与 UEFI 腿的 `prepare_and_enter_kernel_image` 各自面对不同固件、输入本就不同；一致性由数据契约层（`FileLoader` trait / `BootStorage` / `BootPrepareResult` / `build_kernel_info` / `arch_boot`）保证，不由各链入口签名保证。且两条路线**安全性等价**——都只解决「指针被第二次 install 偷换」，`assert!` 在 release 下同样停机。

**附带澄清（讨论中确认）**：传参不解决内容安全。表内容有效性仍靠 magic 校验，内容不被改写仍靠「`SHIM_TABLE` 在 `.bss` 只写一次」的纪律；`debug_assert!` 从来没防过这两条。因此路线 A 的额外收益仅为结构上少两个 unsafe 全局，不构成安全收益。

**为什么现状算问题**：同一份启动契约里，`BootStorageCell::take`（`boot-shim/src/lib.rs:164-172`）与 `pt_alloc::register`（`arch/src/arch/pt_alloc.rs:97-101`，注释明写「so fail closed in release too」）都用两档 `assert!`，唯独这两个安装型全局用 `debug_assert!`。release 下第二次安装会静默改写指针，而 `FileBytes::Borrowed` 的「底层存储不得被覆盖」合同正依赖这张表的地址稳定。

**验收**：
- `grep -n "debug_assert" os/boot-shim/src/opensbi_helpers.rs` → 0（该文件内不再有 debug-only 的一次性检查）；
- 新增负控测试：同一安装函数调用两次，第二次 `#[should_panic]`（若测试环境能构造；否则在注释里写明为何不可测——单线程启动期无重入路径）；
- `cargo test -p boot-shim --features test-all` 全绿；
- riscv U-Boot shim 门链（`os/qemu-tests/test-riscv64-uboot.sh`）PASS；
- 文档 §3.4.5 的所有权段若提及这两个全局，补一句检查强度已对齐（当前文档未点名，可能无需改）。

**风险与回退**：`assert!` 在 release 下保留检查，会增加两条分支（启动期一次性执行，无性能影响）。若某天出现「表在更早的汇编跳板里装好」的合法两次安装形态 → 回退本裁决并重开 D-064（该条件已写进判决行 `reopen_condition`）。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

改动四处：
1. `os/boot-shim/src/opensbi_helpers.rs:503` `install_boot_file_table` 与 `:552` `install_dtb_ptr` 的 `debug_assert!` → `assert!`（消息保持原形，不加 crate 前缀——panic handler 自己会打 `boot-shim(uboot) panic: `，兄弟守卫 `BootStorageCell::take`/`pt_alloc::register` 也只是 `X called twice`）；
2. `os/libs/minix-types/src/types/cell.rs:238` `Frozen::freeze` 的 `debug_assert!` → `assert!`（D-071 并账项），Safety contract 注释 `(debug builds assert)` → `(every build asserts)`，`freeze` 的文档补 `# Panics` 段；
3. `os/boot-shim/tests/install_write_once.rs` 新建（两条负控，见下）；
4. `rewrite-notes/01-stage-kernel/04-platform-discovery.md` 四处回写（D04 逐字引了 `Frozen` 的 Safety contract，又在 §3.7/§1/§5.4 明写「`freeze` 里只有 `debug_assert!`，发布构建静默覆盖」；R04-T07 建议行改「已销」）。D04 的 `FREEZE.md` 追加了冻结后同步段（行数与 sha 更新）。

**负控为何不在 `opensbi_helpers` 的 `mod tests` 里**（本条规格原写「`#[should_panic]`，若测试环境能构造；否则注释写明」）：`DTB_PTR`/`BOOT_FILE_TABLE_PTR` 是进程级 `static mut`，`#[should_panic]` 会在 panic 展开后把安装值永久留在静态里，而同进程的 `test_boot_file_table_validates_magic` 等测试与未来任何 `boot_file_table()` 调用都会读到它——本文件的注释原本就写着「测试不能安全地装全局」。所以放进 `tests/install_write_once.rs`（集成测试有独立进程，从干净状态起步），并用 `catch_unwind` 而不是 `#[should_panic]`——这样「第二次被拒」之后还能继续断言「被拒的那次没有覆盖已装值」，与 `os/arch/tests/pt_alloc_write_once.rs` 同一范式。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -c "debug_assert" os/boot-shim/src/opensbi_helpers.rs` | **0** |
| A2 | `grep -rn "debug_assert" os/libs os/arch --include=*.rs \| grep -ic "freeze\|called twice"` | **0**（write-once 家族已无残留；该目录另有 29 条热路径不变量检查用 `debug_assert`，**不属本条射程**——D-071 判决行「`grep -rn "debug_assert" os/libs os/arch` 归零」的字面口径与这批热路径冲突，按裁决意图（write-once 守卫对齐）取本行为准） |
| A3 | `cargo test -p boot-shim --features test-all --lib` | **136 passed / 0 failed** |
| A4 | `cargo test -p boot-shim --features test-all --test install_write_once` | **2 passed / 0 failed** |
| A5 | `cargo test -p boot-shim --features test-all --release`（含 lib + 集成） | **136 + 6 + 2 全绿** |
| A6 | `./os/qemu-tests/test-riscv64-uboot-shim.sh` | **PASS**（D-73 全链 6/6 标记，进入调度器） |
| A7 | `cargo test -p minix-types --lib types::cell` | **8 passed / 0 failed**（含新增 3 条 `frozen_tests`） |
| A8 | `cargo test -p minix-types` | **339 passed / 0 failed** |
| A9 | `cargo test -p minix-kernel --features mock` | **866 passed / 0 failed / 8 ignored** |
| A10 | `cargo test -p minix-platform` | **18 passed / 0 failed** |
| A11 | D04 文档门（改动后） | anchor 77/77；structure/ref/style(error=0) 通过；citation 87 块 `failed=0`；doc-code-map 54/54；`eval-trace-check --round R3` PASS(11/2/0) |

**鉴别力验证（删检查即变绿的反向证明）**：把两处 `assert!` 临时还原成 `debug_assert!` → `cargo test -p boot-shim --features test-all --release --test install_write_once` **0 passed / 2 failed**（`double install_* must panic in all build profiles`）；`Frozen` 同理 → `cargo test -p minix-types --lib types::cell --release` **7 passed / 1 failed**（`a second freeze must panic in all build profiles`）。恢复后两者 release 全绿。debug 构建对 `debug_assert!` 不敏感（debug 下 `debug_assert!` 也生效），所以这条验证必须用 `--release`。

**范围外发现（不修，留记录）**：`globals.rs:545` 的 `IPC_FILTER_POOL` 注释失真属 D-071 的 (b) 并账项，裁决归入 I-35；`Frozen` 的 `unsafe impl<T: Send> Sync` 缺 `T: Sync` 属 D04 的 `D04-design-batch`（DESIGN-01），均未在本条改动中触碰。

**本条在确认轮结束后由用户单独指示实施**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥，故在 2026 年 11 月 20 日之前一直是「已裁决待实施」。

### I-31（已裁决·可立即开工）wire v10：删除三个字段/分区

**裁决**：用户 2026-11-20，判决行 `D-062`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**——`OPEN-DEBT.md` 的 `WIRE-v10-batch` 行同步。

**一次升版三条**（`BOOT_HANDOFF_V1_VERSION` 9→10；`VM_BOOT_HANDOFF_VERSION` 是另一个独立计数器，本批不动）：

| # | 删什么 | 为什么 | 来源 |
|---|---|---|---|
| ① | `KernelInfo.syscall_entry` | 生产侧填 `kern_virt_base`（`boot-shim/src/lib.rs:126`），不是系统调用入口；内核零读者（真值来自 `syscall_entry_va()`，`kernel/src/lib.rs:2276`）；strict 门却断言它必须等于 `kern_virt_base`（`kernel_info.rs:459`），把错值固化成不变量。C 的 `kinfo_t` 没有此字段（C 的 `tss_init` 直接用汇编标签，`protect.c:189-205`） | R35/D2-03 |
| ② | `user_sp` + `kern_stack_top` | 两端都能精确算出：`user_sp = miniz_types::USER_STACK_TOP`（每架构编译期常量）、`kern_stack_top = kern_virt_base + kern_size`。传输、解码、断言三段都在证明同一常量等于它自己；`build_kernel_info` 的 11 个参数里有 3 个只能传推导值。C 的 `kinfo_t` 没有 `kern_stack_top`；`user_sp` 在 C 里是编译期常量 `USR_STACKTOP`（`pre_init.c:156`）而非交接载荷 | R34/D2-04 |
| ③ | `startup_objects` 分区（20 槽 = 1440 B） | 唯一读者是 strict 门一条 `any()` 断言（"至少一条记录盖住内核镜像跨度"，`kernel_info.rs:516-532`）。经核实**不是**预留 debug 结构：11 个字段只有 `range` 是真值（`startup_record()` 里 `raw_type`/`snapshot_id`/`attribute`/`virtual_start` 恒 0，`input_object`/`kind`/`address_domain` 恒同值，`local_source` 恒等于 `id`）；`input_object: InputObjectId::StartupObjects` 自指，无法回溯任何原始输入；全仓零 debug 输出路径；它想提供的调试信息用 `kern_phys_base`/`kern_size`/`boot_modules`/`boot_alloc_region()` 这些具名活数据拿得更准 | R35/D2-09 |

**等价断言（③ 的必要前提，已实跑验证）**：删掉 startup 分区后，strict 门改为检查「内核物理跨度落在某条 `memmap` 区间或某条 `reserved_regions` 区间内」。五条腿全部成立：

| 腿 | 内核跨度落点 | 依据 |
|---|---|---|
| x86 UEFI | `reserved_regions`（`ReservedMapped`） | `reserve_kernel_span` 用 `AllocateAddress(LOADER_DATA)` 预订（`uefi_helpers.rs:69-81`）；分类器把 `LOADER_DATA` 映射到 `ReservedMapped`（`ebs_map.rs:647`） |
| aarch64 UEFI | 同上 | 同一实现 |
| U-Boot shim | `memmap` | `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)`；`KERN_PHYS_BASE = 0x8020_0000`（`opensbi_helpers.rs:81`），内核在其之上，不被裁 |
| riscv 直核 | `memmap` | 同上（`bootface.rs:213`） |
| aarch64 直核 | `memmap` | `DtbMemoryMap::new(&fdt, 0)`（`bootface_a64.rs:203`），`fw_end = 0` 不裁前缀 |

**实施顺序**（建议按此分批提交，每批独立可编译）：

1. **契约层**：`handoff_v1.rs` 的 `BOOT_HANDOFF_V1_VERSION` 9→10；`envelope_offsets` 的编译期断言按新布局重算；`wire_*` 分区常量同步。
2. **类型层**：`KernelInfo` 的三个字段（`user_sp`/`kern_stack_top`/`syscall_entry`）删除或降为接收侧派生；`startup_objects` 字段与 `BootStorage` 的 20 槽删除。
3. **生产者**：五个写入点（`uefi_helpers.rs:1155-1176`、`opensbi_helpers.rs:434-444`、`bootface.rs:368-398`、`bootface_a64.rs:343-370`、以及 `boot-shim/src/lib.rs:101-132` 的 `report()`）删对应赋值；`push_startup`/`startup_record` 一并删除。
4. **消费者**：`resume_after_v1_decode` 按公式补 `user_sp`/`kern_stack_top`；strict 门三条 `assert_eq!` 删除、改为 ③ 的等价区间检查。
5. **文档**：§3.4.1（字段表）、§3.4.3（wire 结构表与字节账）、§4.4（strict 门表）、§4.5（落地区容量表）同步；**§4.1 的「其余五族（modules/platform/param/sources/startup）的 `push_*`/`set_*_len` 各自有 one-shot 提交断言」必须改写为「四族」**（D-072 补入：startup 族的 one-shot 断言是死代码，`startup_committed` 永不置真，对四族为真、对 startup 为假——同质化误述，I-31 落地后该句否则仍是假的）。

**验收**：
- `grep -rn "syscall_entry\|kern_stack_top" os/libs os/boot-shim os/kernel-image --include="*.rs"` → 0（`kernel/src` 侧可保留派生值，但不得从 wire 读）；
- `grep -rn "startup_objects\|push_startup\|startup_record\|startup_committed\|startup_slots_mut\|set_startup_len" os/ --include="*.rs"` → 0（**D-072 补强**：原口径只查前三项，漏掉 `startup_committed`/`startup_slots_mut`/`set_startup_len` 三个不含 `startup_objects` 子串的名字——它们随族删除一并消失，但不被原 grep 看见）；
- wire 字节账重算：删 startup 分区后 `WIRE_STARTUP_END` 从 30624 降为 29184（= `WIRE_SOURCES_END`，少 20×72=1440）；三个标量各 8 B 在信封内（`user_sp`@48、`kern_stack_top`@56、`syscall_entry`@64），删掉后 `root_page` 从 72 前移到 48，其后 `bump_end`/`memmap_byte_len`/`param_buf_byte_len`/`sources_byte_len`/`startup_byte_len` 全部前移 24 B，信封尺寸缩小 24 B；`handoff_v1.rs:440-455` 的 `offset_of!` 断言与 `WIRE_*_OFFSET` 常量全部按新布局重算；
- `cargo test -p minix-boot`、`cargo test -p boot-shim --features test-all`、`cargo test -p minix-kernel --features mock` 全绿；
- 五条 QEMU 门链（x86 UEFI、aarch64 UEFI、aarch64 直核 SMP、riscv U-Boot shim、riscv 直核 bootface）PASS；
- 文档六门（anchor/citation/structure/ref/style/density）+ `doc-symbol-drift.sh --from-diff` 零命中。

**风险与回退**：wire 版本升级是协议契约变更，收发两侧必须同批改——各自单独提交会导致一侧拒绝另一侧。若某条腿回归且定位不到，回退到 v9 并重开 D-062。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

三条一次升版全部落地，`BOOT_HANDOFF_V1_VERSION` 9 → 10。契约、类型、生产者、消费者、测试、文档六层同批改完——wire 版本升级不能拆批（收发两侧各自单独提交会导致一侧拒收另一侧），所以下面这些改动是一个原子提交的内容。

| 层 | 改动 |
|---|---|
| 契约 | `handoff_v1.rs`：版本 9→10；信封 struct 删 `user_sp`/`kern_stack_top`/`syscall_entry`（各 8 B）与 `startup_byte_len`（8 B），`size_of` 160 → **128**；`envelope_offsets` 删四个常量，`ROOT_PAGE` 72→48、`BUMP_END` 96→72、`MEMMAP_BYTE_LEN` 104→80、`PARAM_BYTE_LEN` 136→112、`SOURCES_BYTE_LEN` 144→120；`WIRE_STARTUP_CAP/OFFSET/END` 三常量删除，`WIRE_SOURCES_END`（29184）成为报文末端，arena 余量 2144 → **3584** |
| 类型 | `KernelInfo` 删 4 个字段（14 → 10 个），新增 `pub fn kern_stack_top() -> VirBytes`（`kern_virt_base + kern_size`，checked_add，溢出 panic）；`BootStorage` 删 startup 槽 + `startup_len` + `startup_committed` + `push_startup`/`startup_slots_mut`/`set_startup_len`/`startup_objects()`；`BootReport` 删 `startup_objects` 切片；`HandoffScalars`/`HandoffDescriptors` 删对应成员；`decode_startup_objects` 删除 |
| 生产 | 五处写入点：`uefi_helpers` 的 EBS 后 startup 集合写入段、`opensbi_helpers` 的 `push_startup_record` 与五条调用、`uboot_main` 的 `write_startup`、`bootface`/`bootface_a64` 的 startup 集合与 `build_kernel_info` 的 `startup_objects` 实参。`build_kernel_info` 11 → 10 参（同时少了三个派生标量，实际是 14 个入参减到 10） |
| 消费 | `arch/src/arch/boot.rs` 的用户栈映射改读 `USER_STACK_TOP` 常量；`vm_handoff` 的 `build_vm_handoff` 同改；`kerninfo` 的 `kui_user_sp` 同改；`misc` 的 `GET_KINFO` 同改；kernel 内 5 处 `kern_stack_top` 改调访问器；strict 门删三条派生值 `assert_eq!`（改由访问器/常量表达），`startup_objects` 的 `any()` 换成「内核跨度落在 memmap 或 reserved 区间」 |
| 测试 | 12 条字段专属测试删除（4 条后置条件、3 条 strict 派生值、1 条 startup 覆盖、2 条 startup 分区、1 条 alignment、1 条 startup 落地区溢出）；新增 1 条等价负控 `test_validate_requiring_sources_rejects_kernel_span_outside_map`；`minix-boot` 里 8 处硬编码 envelope offset 改为引用 `offsets::` 常量（这一项顺带修掉了一个既有的脆弱点：布局再变不会再让测试静默漂移） |

**mini review 抓到的两个真漏项（都是我自己漏的）**：

1. **`uboot_main.rs` 的 `write_startup` 没被抓到**：它是 `[[bin]]`，`cargo check -p boot-shim --features test-all` 只编 lib 不编 bin。补跑 `cargo check -p boot-shim --features fw-riscv-uboot --bin riscv64-uboot-shim --target riscv64gc-unknown-none-elf` 才发现（2 条 error）。这类「bin 不在默认 check 面」的坑以后每批都要显式补跑。
2. **`VmBootHandoff` 的 `user_sp` 被我的批量正则误删过一次**：它是内核→VM 的上层交接契约（todo §6 明写「不在 01 篇射程」），不是 `KernelInfo` 的字段。症状是 `missing field user_sp in initializer of VmBootHandoff`，当批修回。教训是批量替换前要先确认同名符号有几个不同身份。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -rn "startup_objects\|push_startup\|startup_committed\|startup_slots_mut\|set_startup_len" os/ --include=*.rs` | **0**（`startup_record` 仍在：它是 `memmap_source` 的通用记录构造器，与 startup 分区无关） |
| A2 | 信封尺寸 | `size_of::<BootHandoffV1>()` 编译期断言 **== 128**（原 160） |
| A3 | 报文末端 | `WIRE_SOURCES_END == 29184`（原 `WIRE_STARTUP_END == 30624`）；`ARENA_CAP - WIRE_SOURCES_END == 3584`（原 2144） |
| A4 | `cargo test -p minix-boot` | **183 passed / 0 failed**（原 195，删 12 条） |
| A5 | `cargo test -p minix-kernel --features mock` | **864 passed / 0 failed / 8 ignored**（原 866，删 2 条） |
| A6 | `cargo test -p boot-shim --features test-all` | **136 + 6 + 2 passed / 0 failed** |
| A7 | `cargo check --workspace` | **0 error** |
| A8 | `cargo check -p kernel-image` 四个变体（riscv 直核 / aarch64 直核 / aarch64 recv / riscv U-Boot） | 全部 **0 error** |
| A9 | **五条 QEMU 门链** | x86 UEFI **PASS**；aarch64 UEFI **PASS**；riscv U-Boot shim **PASS**（6/6 标记）；aarch64 直核 SMP **PASS**；riscv 直核 bootface **PASS** |
| A10 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`（修 3 处引文）；structure/style(error=0)/density(**承载门通过**) 通过；doc-code-map **54/54** |

**验收口径的一处偏差（如实记录）**：规格原文写 `grep -rn "syscall_entry\|kern_stack_top" os/libs os/boot-shim os/kernel-image --include="*.rs"` **→ 0**。实测做不到，因为 `os/libs` 里有**四个同名异义的对象**：`VmBootHandoff.user_sp`（内核→VM 上层契约，射程外）、`KuserInfo.kui_user_sp`（系统调用页字段）、`KinfoStruct.user_sp`（GET_KINFO 载荷）、`minix-sys/src/stack.rs` 的 `user_sp` 函数参数（用户态栈工具）。`KernelInfo` 侧的那三个字段已经清零，但全目录字面归零会误伤另外四个。这与 I-32 的 `debug_assert` 口径、I-30 的 `arena_pa == 0` 口径属**同一类问题**：规格给的 grep 是「按符号名全目录扫」，没有先把同名异义者排除。本条按裁决意图（`KernelInfo` 不再携带这三个值）判定达成，并把这个口径问题记在此处，供后续规格起草时参考。

**为什么此前不做**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥。本条在裁决后由用户单独指示实施。

**补漏批二（2026 年 11 月 20 日）：30 个 QEMU 测试内核编译失败**：

这是本条第二个「不在默认 check 面」的漏项，比 `uboot_main` 那批大得多。

- **现象**：30 个 `os/qemu-tests/test-kernels/kernel/bootstrap/*/src/main.rs` 仍在 `KernelInfo` 字面量里写 `user_sp:` / `kern_stack_top:` / `syscall_entry:`，字段已删 → `error[E0560]: struct KernelInfo has no field named ...`。
- **为什么第一轮没发现**：这些包的目标带 `required-features`（`fw-x86-uefi` / `fw-aarch64-uefi` / `fw-riscv64-none`），所以 `cargo check --workspace` 与 `cargo check -p kernel-image` 都**不编译它们**。而五条生产 QEMU 门链跑的是 `kernel-image` 的四个产物、不是这些测试内核，所以门链全绿也没暴露。全仓带 `required-features` 的目标共 **46 个**（`tomllib` 实查），这批只是其中一部分。
- **修法**：按 `kern_stack_top` 的字面量是否等价于派生值 `kern_virt_base + kern_size` 分三类：

| 类 | 条数 | 处理 | 依据 |
|---|---|---|---|
| **A 等价** | 26 | 删三行字面量，消费点改 `.kern_stack_top()` | 字面量就是 `kern_virt_base + kern_size`（`0xFFFF_8000_0020_0000` / `kern_virt_base + kern_size` / riscv 的 `DRAM_BASE + 0x200_000`，其中 `DRAM_BASE == kern_virt_base`） |
| **B 不等价（物理基址）** | 3（`test-rt-birth-riscv64`、`test-timer-irq-riscv64`、`test-timer-irq-aarch64`） | 同 A 删字面量 | 它们写的是 `KERNEL_LOAD_PA + size` / `image_base + size`。实测这三个值在该内核里**只用于 16 字节对齐断言**，而派生值同样 16 对齐 → 断言照过，无行为影响 |
| **C 刻意偏离** | 1（`test-rt-birth-aarch64`） | 同 A 删字面量 + 把原注释改写为说明「值已由 `rt_birth_kstack_top` 承担」 | 原值 `0x4781_0000` 是载体自己的 32 KiB 内核栈顶（`KSTACK` 数组，`global_asm` 里 `rt_birth_kstack_top = KSTACK + 0x8000`）。它本来就不该走 `KernelInfo`——字段删除恰好让这个事实显式化 |

- **验证**：46 个带 `required-features` 的目标**逐个 `cargo check`**（带对应 target 与 feature），全部 **0 error**。其中 `test-rt-birth` / `test-sysboot` 另有 `RT_BIRTH_ELF_PATH` / `SYSBOOT_*_ELF_PATH` 环境变量依赖（由门脚本注入），单独 `cargo check` 会报 env-var 缺失，与本条无关。
- **QEMU 全套对照（`run_all.sh`）**：改动前 **13 passed / 26 failed / 5 build failures**，改动后 **11 passed / 28 failed / 1 build failure**。build failure 从 5 降到 1（剩 `test-protection-riscv64`）。
- **数字下降的解释（不是回归）**：`test-timer-irq-riscv64` 在改前那份读数里显示 PASS，但同一份读数也把它列在 `build-failed packages` 里——它当时**编译失败**，PASS 来自 `target/` 里残留的旧二进制（`run_all.sh` 的 `run_test` 在二进制存在时直接加载）。我的改动让它真正编译成功，新二进制跑起来暴露了它自身的既有缺陷：

```
### PANIC in test-timer-irq-riscv64: kernel/src/lib.rs boot identity window is empty or reversed: [0x0,0x0)
```

**补漏批三（同日）：`minix-arch` 的测试面**

第三批漏项，与前两批同源但载体不同：`os/arch/src/arch/boot.rs` 的 `#[cfg(test)]` 段里有 **7 处** `KernelInfo` 字面量仍在写 `user_sp:` / `kern_stack_top:` / `syscall_entry:`（合计 21 个 `E0560`）。其中两处用变量简写（`user_sp,`）而非 `user_sp: ...`，批量正则没覆盖到，单独补修。

- **为什么前两批没扫到**：`minix-arch` 的测试不在 `cargo check -p boot-shim --features test-all`（编 boot-shim）也不在 `cargo check -p minix-kernel --features mock`（编 kernel）的编译面；而 `cargo check --workspace` **默认不编测试代码**。要用 `cargo test -p minix-arch --no-run` 才看得见。
- **修法**：删 7 处三字段字面量；其中一处把 `let user_sp = VirBytes(0x5_0000)` 改成 `VirBytes(minix_types::USER_STACK_TOP)`——因为生产代码 `:661` 早已只读常量、不再读字段，而该测试的断言（`assert_eq!(result.sp.0, user_sp.0 - 64)`）会跟着常量走。
- **读数**：`cargo test -p minix-arch --no-run` 的 error 数 **60 → 39**，我的 21 个归零。
- **剩余 39 个不是代码缺陷，是跑测试时没带对 feature**（当日续查结论，推翻上一版的判断）：
  - `MockPaging` 挂在 `os/arch/src/arch/paging.rs` 的 `#[cfg(feature = "runtime-window")]` 下；`cargo test -p minix-arch`（默认 feature）看不到它，于是 7 个用 `MockPaging` 的 `#[test]` 全数编译失败，连带 `vec!`/`Box`/`Vec`/`std` 的缺失错误（那些测试正经由 `alloc` 路径）。
  - 正确命令是 **`cargo test -p minix-arch --features runtime-window`**：该命令 **0 error、288 passed / 0 failed**。
  - 顺带纠正一处文档错误：`02-stage-vm/draft/07`、`06` 里写的 `cargo test -p minix-arch --features mock`，`minix-arch` **没有 `mock` 这个 feature**（有它的是 `minix-kernel`/`minix-plat`）——照抄会报 "does not contain this feature"。
- **这条链路的验证面还有两个既有缺口，都登记不改**（不是本条职责）：① `cargo test -p minix-arch` 裸跑会失败（需显式带 `runtime-window`），crate 自己没有机制让自己的测试自动开自己的 feature；② CI 的 `vm-tests.yml` 里 `os/libs/minix-arch/**` 这个 path filter 指向**不存在的路径**（实际目录是 `os/arch/`），且它只跑 `minix-vm`，**完全不跑 `minix-arch` 的测试**——因此上面那个 feature 问题长期无人发现。
- **本轮实测结论**：带对 feature 后 `minix-arch` **288 passed / 0 failed**，其中包含本条补漏批三改动的两处 `user_sp` 断言——**它们现在是真跑过并通过的**，不再是"静态推理、无法验证"。

- **那条 panic 的真因（既有缺陷，与本条无关）**：`BOOT_MODULES: [BootModule; 12]` 是全零数组，内核只填 `mods[8]` 一项，却把**整个 12 槽数组**当成 `boot_modules` 切片交出。`push_module_windows` 逐项 push，遍历到第 0 槽（`start=0, len=0`）就撞 `BootWindows::push` 的 `assert!(start < end)`。同一形态出现在 `test-rt-birth-aarch64` / `test-rt-birth-riscv64` / `test-timer-irq-aarch64`。这四条的修法（要么让测试内核交出真实前缀 `&mods[8..9]`，要么让 `push_module_windows` 跳过空模块）需要单独裁决——**不在本条射程**，已登记为 I-43。


### I-30（已裁决·可立即开工）aarch64 形态判别改链接期分叉

**裁决**：用户 2026-11-19 选择「向 riscv64 看齐」，判决行 `D-061`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**——裁决本身已生效，实施尚未开始；`OPEN-DEBT.md` 的 `ARCH-01-aarch64-link-split` 行同步。

**现状（为什么这是个问题）**：`os/kernel-image/src/main.rs:376-382` 的 aarch64 入口用一个运行期判断分流两条腿：

```rust
#[cfg(target_arch = "aarch64")]
fn rust_image_main(arena_pa: usize) -> ! {
    if arena_pa == 0 { bootface_a64::bootface_a64(); }
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}
```

`arena_pa` 就是 `x0`。同一个 `kernel.elf` 靠「`x0` 是否为零」区分直核 `-kernel` 启动与 boot-shim v1 交接。而 `x0` 零值**不是任何规范承诺**——文档 §4.8（第 3046 行）自述「QEMU 观察到不设 `x0` 参数——非规范保证，仅本仓 QEMU 虚机实测；真板 `-kernel` 行为以板级文档为准」。任何在 `x0` 放非零值的加载器（aarch64 U-Boot 的 `booti`、真板固件、QEMU 未来版本改默认值）都会把直核启动误判成 v1 接收。

**失效症状**：`kernel-image: handoff v1 envelope invalid` panic。它发生在 `early_console` 初始化**之后**——串口能看到消息，但指向的是「交接协议坏了」，而不是「入口形态没识别」。误导方向。

**做法（与 riscv64 同构，不是新机制）**：riscv64 已经是编译期分叉的：

- `os/kernel-image/Cargo.toml:35` — `fw-riscv64-uboot = ["fw-none-image"]`
- `os/kernel-image/src/main.rs:398` — `#[cfg(all(target_arch = "riscv64", not(feature = "fw-riscv64-uboot")))]`
- `os/kernel-image/src/main.rs:407` — `#[cfg(all(target_arch = "riscv64", feature = "fw-riscv64-uboot"))]`

aarch64 照此加一个 feature 门（如 `fw-aarch64-uefi-recv`），两条 `rust_image_main` 各归一个 `cfg`：直核变体只编 `bootface_a64::bootface_a64()`，v1 变体只编 `arch_boot_resume_v1`。入口签名不再需要 `arena_pa == 0` 这个哨兵。

**三处一致计划**（D-061 已记）：
1. 文档：§3.7 的启动路径计数表与 §4.8 的 aarch64 分道段改为「形态由构建产物决定」；§3.1.1 的接线状态表补上两个 aarch64 变体。
2. 设计契约：`kernel-image` 的 aarch64 入口 ABI（`_start` 与 `rust_image_main` 的签名）与链接脚本选择；`xtask image` 的 aarch64 产物清单。
3. 代码：`os/kernel-image/src/main.rs` 的分道、`bootface_a64` 的触发点、`os/qemu-tests/test-smp-aps-aarch64.sh` 的镜像选择、UEFI 门脚本的产物路径。

**验收**：
- `grep -n "arena_pa == 0" os/kernel-image/src/main.rs` → 0（aarch64 下不再有运行期哨兵判别）；
- 两个 aarch64 产物各自构建成功，`xtask image` 产物清单含两项；
- aarch64 直核门（`test-smp-aps-aarch64.sh`）与 aarch64 UEFI 门链均 PASS；
- §3.7 / §4.8 / §3.1.1 三处文档与实现同形；
- `cargo build` 三架构不回归。

**风险与回退**：新增一个构建变体与产物清单；改一个入口签名、一个链接脚本选择点、两个门脚本。若任一 aarch64 门链回归 → 回退到运行期判别并重开 D-061（该条件已写进判决行的 `reopen_condition`）。

**为什么现在不做**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥（设计实施会让 certify 失效、基线在确认之下移动）。裁决已生效，实施留待下一 design/code batch。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

D-061 的三处一致计划全部落地：

| 层 | 改动 |
|---|---|
| 构建特性 | `os/kernel-image/Cargo.toml` 新增 `fw-aarch64-uefi-recv = ["fw-none-image"]`（与 `fw-riscv64-uboot` 同形） |
| 代码分道 | `os/kernel-image/src/main.rs`：原来一条 `#[cfg(target_arch = "aarch64")] fn rust_image_main` 拆成两条——`all(aarch64, feature = "fw-aarch64-uefi-recv")` 的接收臂（只编 `arch_boot_resume_v1`）与 `all(aarch64, not(feature = ...))` 的直核臂（只编 `bootface_a64::bootface_a64()`） |
| 产物清单 | `os/xtask/src/image.rs` 的 `Arch::Aarch64` 盘面特性从 `fw-aarch64-none` 改成 `fw-aarch64-uefi-recv`（UEFI 盘启动必然是 v1 接收形态）；同步改写它自己的计划断言与字段注释 |

**行为变化（三处，都必须知道）**：

1. **接收臂的 `arena_pa == 0` 语义变了**：改造前它表示「该走直核自举」，现在只表示「门面没交接 arena」——所以是诚实停机（打印 `no handoff arena (x0 == 0), halting`），不再静默转 `bootface_a64`。这是 D-061 的核心：入口形态由构建产物决定，不再从寄存器猜。
2. **直核臂不再读 `x0`**：形参写成 `_arena_pa`，函数体直接调 `bootface_a64`。旧代码「先判 `x0` 再决定要不要自举」的结构消失。
3. **`mod bootface_common` / `mod bootface_a64` 的 cfg 收窄**：接收臂不用这两个模块，编译进去会留一片 unreachable 代码（见下 mini review）。

**mini review 抓到的一处真问题（我自己引入的）**：

第一次改完跑 `cargo check` 两个变体都 0 error，但**接收变体比直核变体多 8 条 `never used` 警告**——`bootface_common` 的 `place_modules`/`PlacementPool`/`dtb_bounded`/`fdt_table_pa`/`fdt_bootargs`/`in_memmap_range`/`table_magic_ok`/`be_cell` 在接收臂里一个调用者都没有。修法：把 `mod bootface_common` 的 cfg 从 `any(riscv64, aarch64)` 收窄为 `any(riscv64, all(aarch64, not(fw-aarch64-uefi-recv)))`，与 `bootface_a64` 同门。修后两个变体都是 19 条（riscv 直核 18 条），**警告数对齐**。

**鉴别力验证（结构级，比行为级更强）**：

在接收变体里故意写回旧分道——把 `halt()` 那段换成 `bootface_a64::bootface_a64()`：

```
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `bootface_a64`
```

**编译期就失败**。这不是「测试失败」而是「这个形态在接收产物里根本无法表达」——比运行期断言强，因为连代码都编不进去。恢复后 0 error。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -n "arena_pa == 0" os/kernel-image/src/main.rs` | **4 处**——x86（`:350`，无直核面，诚实失败）、aarch64 接收臂（`:385`，诚实失败）、riscv 接收臂（`:428`，诚实失败），另 1 处是类型文档的说明句。**aarch64 的「形态判别」语义已消除**；规格原文的「→ 0」口径把 x86/riscv 的合法诚实失败也算了进去，与 I-32 的 grep 口径同属「字面归零不可达」，此处按裁决意图（不再用 `x0` 猜形态）判定 |
| A2 | `cargo check -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none` | **0 error** |
| A3 | 同上 `--features fw-aarch64-uefi-recv` | **0 error**，且 dead-code 警告数与直核变体同为 **19** |
| A4 | `cargo check -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none` | **0 error**，dead-code **18**（未回归） |
| A5 | `cargo run -p xtask -- image --arch aarch64 --release` | **镜像就绪**（`BOOTAA64.EFI` + `kernel.elf` + 12 模块） |
| A6 | `test-shim-bootmarks-aarch64.sh`（盘面/接收形态） | **PASS** |
| A7 | `test-smp-aps-aarch64.sh`（`-kernel` 直核形态） | **PASS**（AP entered scheduler; SGI+timer gates open） |
| A8 | `check-layout.sh aarch64` | **PASS**（L0–L8 全过，入口 `0xffff800000000000`） |
| A9 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`；structure/style(error=0) 通过 |
| A10 | 引文逐字核对 | 两个 cfg 臂的 8 处关键行与源码逐字一致（Python 逐行比对） |

**范围外发现（不修，留记录）**：`cargo test -p xtask` 的 `manifest::tests::boot_module_order_matches_boot_shim_module_names` 失败——**在我改动之前（HEAD）就已失败**（用 `git stash` 暂存我的改动后复跑确认），原因是它从 `boot-shim/src/loader.rs` 抽 `MODULE_NAMES` 时解析失败，与本条无关，也未在本条改动中触碰。

**为什么此前不做**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥（设计实施会让 certify 失效、基线在确认之下移动）。本条在裁决后由用户单独指示实施。

### D-71：`BootForm::source_path` 悬空 + 测试无存在性检查

路径改 `os/libs/minix-boot/src/handoff.rs`；`test_each_boot_form_has_distinct_source` 补五条落点的存在性断言；顺带核其余四条落点。证据：`ls os/boot-shim/src/handoff.rs`（不存在）vs `ls os/libs/minix-boot/src/handoff.rs`。验收：`cargo test -p minix-boot` 全绿。**状态**：✅ 01TODO-B1（路径已改，五落点 `repo_root.join(source_path()).exists()` 断言入测试；首跑以错拼 `../..` 的红验证断言有鉴别力）。

### I-26：opensbi_helpers 孤儿注释 + "hardcoded" 顶注

删两条孤儿 doc 注释（L61–L64）；顶注改 "Memory map → `DtbMemoryMap::collect(dtb_ptr())` (DTB `/memory`; missing DTB halts)"；grep 全文件清剩余 "hardcoded"。验收：`cargo check -p boot-shim`（opensbi feature）通过。**状态**：✅ 01TODO-B1（孤儿注释已删，两句 hardcoded 归零，opensbi 形态 27/0）。

### I-27 + L1424（同批，三处都动 §3/§4 现状 vs 终态叙述）

L1422/L2624「还没裁决/尚未定案」→「OQ-N6 已裁（每进程独立根原语，aarch64 `BootHandoff` 交接即其实现）；x86 腿实施收口 = NK1（C-29）待认领」；L1424 张力块按候选 ① 补「当前产线形态与终态的差距见 §3.4」指针；`os/kernel-image/src/main.rs` 头注顺一句「OQ-N6 已裁」。证据：`grep -n "还没裁决\|尚未定案" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。**状态**：✅ 01TODO-B1（两句归零；§3.1/§3.3 现状标注与 main.rs 头注同批，doc-lint error 0）。

### N-1 `BootAlloc::resume` 与 `region()` 缺区间自洽校验（跨镜像 wire 字段未验证即信任）

**现状（本轮新登记）**：`os/kernel/src/boot_alloc.rs:fn resume` 三值裸 store，无 `base <= next <= end` 断言；`fn region` 只判 `end == 0`，不判 `base < end`。`alloc` 的越界判据只有 `pa >= end` 回滚——**不检查 `pa < base`**。跨镜像路径上 `bump_base/bump_next/bump_end` 来自 wire 载荷（对端同批构建，现可信；但"可信"是工序纪律不是结构保证）。后果推演：若 `next < base` 成立，`alloc` 会从声明区间**之外**发页（在 `end` 之前一路成功）——页表页落进未登记区域，扣除表与 DM 覆盖都不含它；若 `base > end`，`region()` 返回倒置区间，DM 候选与扣除两侧都按错区间算。

**修法（两处补一道，不重复）**：① `resume`/`region` 入口断言 `base < end && base <= next && next <= end`（引导期单线程，fail-fast 与 `KernelInfo::validate` 同风格）；② **T1 的 wire 验证必须覆盖同一检查**（§2.2 第 5 步受控复制的"逐载荷验证"清单里加"bump 三件套区间自洽"）——wire 字段不该只靠发送端纪律。两条不必都做，但②是 T1 的必做项、①是现状的可选加固（若做，宿主测试补三类倒置/越界输入）。

**证据命令**：`sed -n '60,90p' os/kernel/src/boot_alloc.rs`；`grep -n "fn resume\|fn region" os/kernel/src/boot_alloc.rs`。**状态**：✅ 01TODO-B1（① 已做：两入口断言 + 五枚宿主测试——1 枚合法 resume + 3 类倒置/越界 + 1 枚 region 侧倒置 init；② wire 侧检查仍为 T1 必做项）。

### N-2 `vm_handoff.rs` 模块头注释与扣除实现不一致（注释回修）

**现状**：模块 doc 的扣除表行写 "reserved module blobs | `KernelInfo.boot_modules()` **minus the VM module** (reclaimed after ELF copy, C: protect.c:450-451)"；而实现 `os/kernel/src/vm_handoff.rs:fn classify`（"Kernel-reserved: every boot module blob **INCLUDING VM's own**"——该处成修注释记录了完整理由：VM 启动前其段帧从本 free list 取出，回收即启动前腐坏）与交接模块清单（"every module except VM's"，C `kinfo.module_list[]` 口径）是**两份不同清单**。01 篇 §3.8 已如实指出"表头注释的旧口径"，但代码注释本身未改——读者先读模块 doc 会得到错的前提。

**修法**：模块 doc 该行改为两行分述——「扣除记录（`classify`）：全部模块含 VM 自身（成修理由：VM 启动前其段帧从本 free list 取，回收即启动前腐坏）」与「交接模块清单（`VmBootHandoff.modules`）：除 VM 外各模块（C `kinfo.module_list[]` 语义）」。验收：`grep -n "minus the VM module" os/kernel/src/vm_handoff.rs` 归零。**状态**：✅ 01TODO-B1（模块头与 `classify` doc 两处均改为两清单分述；01 篇 §3.8 的引文与说明句同批）。

### N-3 零消费者便利包装（`uefi_helpers::load_kernel_elf` / `load_boot_modules`）

**现状**：`os/boot-shim/src/uefi_helpers.rs` L575/L580 两个 `pub fn` 文档称"供集成测试等直接调用 UEFI 路径"，全仓检索**零调用方**（`grep -rn "load_kernel_elf\|load_boot_modules\b" os --include="*.rs"` 只命中定义与一处 doc 引用）。死公共 API 会让读者以为存在另一条产线入口。

**修法**：删除两函数（连同其 doc 注释）；`minix-elf` doc 里对 `uefi_helpers::load_kernel_elf` 的指涉改为指 `loader::load_kernel_with_loader`。验收：上述 grep 归零（`load_boot_modules_with_loader` 保留）。**边界**：`build_memmap()`（单数）**保留**——两个 qemu 测试内核（`test-higher-half`/`test-protection`）在用。**状态**：✅ 01TODO-B1（两函数已删，全仓 grep 归零；`alloc_module_pages`/`build_memmap`/`load_boot_modules_with_loader` 保留）。

### N-4 `bootface.rs` ↔ `bootface_a64.rs` 名字池与常量的重复收敛（独立清理增量）

**现状**：`bootface_a64.rs` 头注自陈"两门全绿后的去重收敛留给独立清理增量"，D-68 的边界也写着"名字池/常量合并超出本条"——但台账里**没有独立条目**接住这件事（只在两处"边界"句里被提及）。重复面：模块名池、`MODULE_NAMES` 同源常量、`DTB_PA`/`KERN_SIZE` 类布局常量、串口打印辅助。风险：两份副本各自漂移（与 I-22 的削界算术双份同型，只是对象换成名字池/常量）。

**修法（D-68 落地后）**：先 `grep -n "NAME_POOL\|MODULE_NAMES\|DTB_PA\|KERN_SIZE" os/kernel-image/src/bootface*.rs` 列全重复面；能下沉到 `boot-shim` 库（`opensbi_helpers` 已有的名字表）就下沉，不能下沉的（架构差异的真常量）留两份并在双方注释互指。**验收**：重复面清单落盘 + 每处按"下沉/互指"处置；不追求两文件结构合并（头注与公理 3 已裁不合并引导形态层）。**状态**：✅ 01TODO-B4。清单与处置——**下沉**到新模块 `os/kernel-image/src/bootface_common.rs`（`cfg(any(aarch64, riscv64))`）：`NAME_SLOT`/`NAME_POOL`、`NAME_POOL↔MODULE_NAMES` 编译期对账、`be_cell`、`fdt_table_pa`、`in_memmap`、`in_memmap_range`、`table_magic_ok`（两文件逐字同形）；**留两份互指**：`say`/`say_hex`（`early_console` 实现按架构不同）、`place_modules`（本地 bump 池与 `MODULE_TABLE` 胶水，差异仅池基）、布局常量组（`DRAM_BASE`/`KERN_PHYS_BASE`/`DTB_PA`/`KERN_SIZE`/`BUMP_*`/`PT_BUMP_PAGES`）。偏离说明：工单原写「能下沉到 boot-shim 库就下沉」，实际落在 kernel-image 包内的共享模块——这批 helper 是 kernel-image 侧零堆/物址视图专用，放进 boot-shim 公共面会为一个消费者扩大库边界；两架构直核冒烟（`test-smp-aps-aarch64`、`test-kernel-image-riscv64-bootface`）均 PASS。

### N-5 池页零化机制归因未定案（开放调查项，非代码缺陷）

**现状**：01 篇 §3.4 明说两组真机读数（`res=127 zeroed=127`；"DM 建立时完好、分类点读零"）指向同一设计结论（入口尽早深拷），但**机制归因未定案**——两条候选（页表不重放 1:1 映射 / 被当空闲复用清零）各带可检验推论，文档标注"可作为后续取证的切入口"。T1 的受控复制设计**不依赖**该归因（拷贝先于一切分配），所以不是 T1 的阻塞项；但证据链缺最后一环，且它决定"池页可读窗口"的教科书式解释。

**修法（定向调查）**：按文档给的两条推论各做一次定向读数——（a）若"页表不重放"：在切根前后各读同一池页地址，读数应在切根时刻突变；（b）若"复用清零"：在 `store_kernel_info` 之前人为推迟消费（测试内核）并在首个 `pg_alloc_page` 之后回读，读数应随复用时点零化。落点：`tmp/` 取证或 qemu 测试内核探针。**验收**：归因结论落盘（一句话 + 读数），01 篇 §3.4/§4.5 相应句从"未定案"改为定案或保留双候选并写明判别结果。**状态**：✅ 01TODO-N5。探针在 `test-kernel-map`：快照之后分配一枚低于 4GiB 的 LOADER_DATA 页、填位置标记，EBS 前/后/开分页后三次整页校验和；读数 `pa=0x5e9b000`、`pre=post_ebs=post_enable=0x4a00_0000_0001_ff00`、`verdict=replayed`、`in_snapshot_free=0`。判别：①"页表不重放 1:1"对该形态不成立（映射重放、字节未变）；②"被当空闲复用"的结构前提未出现；历史零化读数的剩余解释是深拷之前的入口引用链（NK4-A），与深拷落地后症状消失一致。01 篇 §3.4 两处已按读数改写，探针保留为可复跑证据。



### I-35（已裁决·待实施·行号与前提已复核）`KERNEL_INFO` 的 write-once 承诺与二次写 + 两处注释失真

**裁决**：用户 2026-11-20，判决行 `D-071`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**（用户明确要求「只记账不实施」）。来源：`R34/DESIGN-D2-05`（P2，[MODERN]）。**编号注记**：本条是 `I-35`，与本节下方的 `D-71`（`BootForm::source_path`，已修）不是一回事——后者是历史编号，前者是 2026-11-20 的判决行。

**现状（为什么这是个问题）**：`globals.rs:271-277` 的 `KERNEL_INFO` 承诺两句话——「stored once during kmain, read-only thereafter」与「Only written once during boot (single-threaded, before BKL needed)」。实际每条生产腿写两次：`arch_boot_finish:225`（或 v1 接收腿的 `resume_additive:352`）一次，`kmain:1738` 再一次。

**全仓 write-once 承诺审计（2026-11-20，共 11 处实质承诺）**——这是本条的取证面，结论是 `KERNEL_INFO` 是唯一「承诺一次、实际二次」的真缺陷：

| # | 位置 | 承诺 | 实际 | 判定 |
|---|---|---|---|---|
| 1 | `globals.rs:271-275` `KERNEL_INFO` | stored/only written once | 每腿 2 次 + 测试 9 处 | **违背**（本条主体）|
| 2 | `globals.rs:545` `IPC_FILTER_POOL` | only written once | 初始化 1 次，但 `ipc_filter_pool_with():3689` 返回 `&'static mut`，3 处可变借用 | 注释失真（并入本条）|
| 3 | `globals.rs:621` `MINIX_KERNINFO_USER` | written once by init_kerninfo | 未见二次写 | 符合 |
| 4 | `globals.rs:621` `CURRENT_ROOT_PHYS` | written once by arch_boot_impl | 两腿各 1 次、互斥（直核 `:1413` / v1 `:348`）| 符合（措辞可收窄到"每腿一次"）|
| 5 | `globals.rs:180` GDT/TSS/IDT | written once by init_protection | `set_kernel_stack` 只写 sp0，不写表体 | 符合 |
| 6 | `vm_handoff.rs:122-131` `VM_PMM_FREE` | write-once at classification | 唯一调用点 `lib.rs:2972`，不在循环内 | 符合 |
| 7 | `memmap_source.rs:1361` `commit_static` | written once per boot face | **有守卫**：`DTB_COLLECT_TAKEN.swap+assert`（`:1432-1435`，`cfg(not(test))`）| 符合（实现最好）|
| 8 | `pt_alloc.rs:49/77` `PtAllocSlot` | written exactly once | **有守卫**：`PT_CLAIMED.swap+assert`（`:100-101`/`:152`）| 符合 |
| 9 | `types/cell.rs:176/228` `Frozen<T>` | written exactly once | `debug_assert!`（release 失效）→ **已改 `assert!`**（2026 年 11 月 20 日，随 I-32） | **已修**（原「同类缺陷」）|
| 10 | `resv_claims.rs:256` `DTB_RECORDS` | write-once inside capture | `capture` 在 EBS 重试循环内（`ebs_pair.rs:193`），每 attempt 前先归零 | 注释可收窄为「write-once per attempt」 |
| 11 | `bootface.rs:320` / `bootface_a64.rs:295` | written once, read-only | 直核腿入口只跑一次 | 符合 |

**二次写的必要性（用户问「后面的写不是必要的吗」的答案：不必要）**：`kmain:1738` 的实参是 `:1687` 刚从全局槽读出的同一个 `&'static`；`store_kernel_info` 内部 `let mut k = *kernel_info`（`:1551`，14 字段拷栈）+ 7 次同址指针重定向 + `*KERNEL_INFO.get() = Some(k)`（`:1562`）写回同槽——产出物与输入完全相同，无副作用也无收益。它的存在是 `30b66fd3a` 把 `store_kernel_info` 返回类型从 `()` 改成 `&'static KernelInfo` 时编译器要求接收返回值的产物；作者在 `:1682` 已自注「This shadow can be deleted once the diagnostics below confirm it」。**另需注意**：`kmain:1673` 的形参在函数体内从未被读（`:1687` 用同名局部变量遮蔽它），而 `:1738` 那行恰好"用"了形参、吃掉了 `unused_variables` 警告——删行后会暴露警告，所以第 ② 步必须同步改名。

**做法（四步）**：

1. 删 `lib.rs:1738` 的 `let kernel_info: &KernelInfo = store_kernel_info(kernel_info);`——`kmain` 内 `:1687` 已取全局引用。
2. `kmain:1673` 形参改 `_kernel_info`（如实表达「不可信」并消除警告）。
3. `store_kernel_info`（`:1453`）开头加一次性守卫：

```rust
assert!(
    KERNEL_INFO.get().is_none(),
    "store_kernel_info: a second store would rewrite a live &'static slot"
);
```

4. `globals.rs:271-275` 注释改为「written once per leg before `kmain`; `kmain` reads only」与对应的 SAFETY 措辞。

**测试适配**：`lib.rs:5878` 的 `store_kernel_info(stored)` re-store 断言改成 `#[should_panic(expected = "second store")]` 负控——把「禁令」变成有鉴别力的测试（删掉 assert 后它变绿，证明测试有效）。

**并账两项**：
- **并入 I-32（D-064）**：`Frozen::freeze`（`os/libs/minix-types/src/types/cell.rs:228`）原用 `debug_assert!`，release 下二次 freeze 静默改写。D-064 只点了 `opensbi_helpers` 两处，未覆盖公共类型；I-32 的验收 grep 扩为 `grep -rn "debug_assert" os/libs os/arch` 归零。**已实施（2026 年 11 月 20 日，随 I-32 一并落码）**：该守卫改为 `assert!`，实测字面口径不可达（该目录另有 29 条热路径不变量检查仍用 `debug_assert`，与 write-once 无关），实取 write-once 家族归零；详见 I-32 的「实施记录」A2 行。
- **本条内一并**：`globals.rs:545` 的 `IPC_FILTER_POOL` 注释「Only written once during boot」与 `ipc_filter_pool_with():3689` 返回 `&'static mut`、`proc_table.rs:1277`/`syscall.rs:760,958` 三处可变借用矛盾。改注释为「初始化一次，此后在 BKL 下可变」。
- **登记不改（注释可收窄，无害）**：`resv_claims.rs:256` 的 `DTB_RECORDS` —— `capture` 在 EBS 重试循环内（最多 2 次 attempt），每次 attempt 前先 `DTB_RECORDS_N = 0` 归零再写，实际安全；注释可改成「write-once per attempt」。

**2026-11-20 复核（RD3/DESIGN-D2-01 同一缺陷的第二次面板发现）**：

D02 的 `R3/DESIGN-D2-01`（P1）从**别名模型**角度报告同一处代码——`store_kernel_info` 是安全函数却返回 `&'static` 并在内部原地改写同一槽（`*KERNEL_INFO.get() = Some(k)`）。这是 D-071 已裁决缺陷的重复发现，**不另立判决行**，记为 `dup-of: D-071`。

复核同时修正了本条的三处过期事实（I-31 / I-38 合并后行号全面漂移）：

| 项 | 登记时 | 复核实测 |
|---|---|---|
| 二次 store 位置 | `lib.rs:1738` | **`lib.rs:1858`** |
| `kmain` 的全局读 | `lib.rs:1687` | **`lib.rs:1807-1808`** |
| 唯一写入点 | `lib.rs:1561`/`:1562` | **`lib.rs:1676`** |
| `store_kernel_info` 定义 | `lib.rs:1453` | **`lib.rs:1570`** |
| 落地区溢出断言 | `:1538` | `:1547` |
| 测试 re-store | `lib.rs:5878` | **`lib.rs:5991`** |

**关于「形参从未被读」的前提**：复核确认**仍然成立**，但成因比原文写的更精确——`kmain:1807` 的 `let kernel_info: &KernelInfo = crate::kernel_info().expect(...)` **遮蔽了形参**，所以形参从 `:1807` 起在函数体内再无读者；`:1858` 那行是它唯一的"使用"，恰好吃掉 `unused_variables` 警告。因此第 ② 步（形参改 `_kernel_info`）**必须与第 ① 步同时做**——只删行会立刻暴露警告。

**与 Minix3 C 的对照（裁决理由的补强）**：`minix3/minix/kernel/arch/i386/pre_init.c:20` 的 `kinfo_t kinfo` 是内核自己的 `.bss` 全局；`:224` 的 `get_parameters(ebx, &kinfo)` 从 boot 提供的 multiboot info **拷贝一次**（`:123` 的 `memcpy((void *)mbi, (void *)ebx, sizeof(*mbi))`），`:237` 返回 `&kinfo`，此后只读。**C 侧拷贝只发生一次，且没有"再存一次"的合法需求。** Rust 侧第二次 `store_kernel_info` 的源已是 `.bss` 里的自身（`land_slice` 的指针相等守卫 `:1549` 跳过元素拷贝），净效果是"把 A 拷到栈再写回 A"——与 C 模型不符。

**验收**：
- `sed -n '1858p' os/kernel/src/lib.rs` 不再含 `store_kernel_info`；
- `grep -n "assert!(KERNEL_INFO.get().is_none()" os/kernel/src/lib.rs` → 1；
- `grep -rn "debug_assert" os/libs os/arch` → 0；
- `grep -c "Only written once during boot" os/kernel/src/globals.rs` → 0（`KERNEL_INFO` 与 `IPC_FILTER_POOL` 两处措辞都已改）；
- `cargo test -p minix-kernel --features mock`、`cargo test -p minix-types`、`cargo test -p minix-platform` 全绿；
- 新增/改造的 `#[should_panic]` 负控在删 assert 后变绿（鉴别力验证）；
- 五条 QEMU 门链不回归（`kmain` 改动触及所有腿）。

**风险与回退**：`kmain` 的形参是 ABI（`jump_to_kmain` 传值），只改名不删参；若某腿依赖「`kmain` 入口形参」做诊断打印，改名会破坏该打印（已核：只有 `#[cfg(target_arch = "x86_64")]` 的取证块读寄存器，不读形参）。若一次性 assert 在既有 9 处测试调用上无法适配（它们需要多次 store），退化为 `#[cfg(not(test))]` 门控并重开 D-071。

**为什么现在不做**：用户 2026-11-20 明确要求「不实施，只记账」。

**实施记录（2026 年 11 月 20 日，(b) 注释项已完成；(a) 主项仍不实施）**：

D-071 把本条拆成三项，其中两项已有归属：`Frozen::freeze` 随 I-32 落码（已记在并账项里），`KERNEL_INFO` 四步方案用户明确「不实施」。本轮只做第三项 **`IPC_FILTER_POOL` 注释失真**——它是纯注释修正，不动代码，不改变任何行为。

| # | 改动 | 位置 |
|---|---|---|
| ① | 「Only written once during boot」→「initialized once during boot」+ 明确说明此后池**仍可变**、每次访问经 `ipc_filter_pool_with(&BklSection)`、并列出三个改动调用者 | `os/kernel/src/globals.rs` 的 `IPC_FILTER_POOL` SAFETY 段 |

**为什么这条注释是错的（核实证据，逐条）**：

| 事实 | 命令 / 位置 | 读数 |
|---|---|---|
| 池在 boot 之后仍被**可变**访问 | `os/kernel/src/lib.rs:3734` 的 `unsafe { &mut *IPC_FILTER_POOL.get() }`（返回值类型 `&'static mut IpcFilterPool`） | 确认返回可变引用 |
| 三个改动调用者 | `grep -rn "ipc_filter_pool_with" os/kernel/src/proc_table.rs os/kernel/src/syscall.rs` | **3 处**（`proc_table.rs:1277`、`syscall.rs:760`、`syscall.rs:958`），与 D-071 判决行的记录逐字吻合 |
| 没有绕过 witness 的写路径 | `grep -rn "IPC_FILTER_POOL" os/ --include=*.rs`（排除注释） | 只有 4 处：定义（`globals.rs:551`）、`use` 列表（`lib.rs:161`）、boot 期初始化（`lib.rs:1876`）、witness 访问器（`lib.rs:3734`）。初始化那一处是启动期单线程，符合新注释 |
| witness 是证据类型而不是锁 | `os/kernel/src/smp.rs:1995-2040` 的 `BklSection` 定义与 `from_held` 风格构造 | 证明「锁在某处被持有」，不证明每次写入都持锁。所以新注释写「each pass a BKL witness」而不是「因此安全」——后者会是对类型能力的过度声称 |

**mini review 抓到的一处措辞风险（已修）**：初稿写「three mutating callers each **hold** the BKL witness」，读起来像「持有锁」的强断言。`BklSection` 可以被 `from_held` 凭上游约定构造，实质是「证据在某处成立」而非「此处必然持锁」——改为「each **pass** a BKL witness」，与类型的实际能力对齐。

**范围外发现（未改，留记录）**：`os/kernel/src/lib.rs` 的 `ipc_filter_pool_with` 文档有两处瑕疵——① 「Get a mutable reference to the global IPC filter pool.」出现两次（一次在函数头、一次紧邻签名）；② 其中一句写「Caller must ensure BKL is held」而函数签名收 `&BklSection`（witness 已经表达这层前提，措辞冗余）。这两处不在 D-071 (b) 的授权文本内（该授权只点名「`globals.rs:545` 的注释」），按 fix-guard 一次一条不顺手改，登记待后续批次。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -c "Only written once during boot" os/kernel/src/globals.rs` | **1**（`KERNEL_INFO` 那处仍在——那条属 (a) 主项，用户明确不实施；`IPC_FILTER_POOL` 那处已改） |
| A2 | `cargo check -p minix-kernel --features mock` | **0 error** |
| A3 | `cargo test -p minix-kernel --features mock` | **866 passed / 0 failed / 8 ignored** |
| A4 | 改动性质 | 纯注释，零行为变化（diff 只有 `globals.rs` 的 4 行 SAFETY 文本） |

**为什么 (a) 主项仍不做**：用户 2026 年 11 月 20 日明确要求「不实施，只记账」。(b) 之所以能做，是因为它是 D-071 明确授权的独立小项、且不触碰 `KERNEL_INFO` 的四步方案。





### I-39（已裁决·待实施，先于 I-31）`KernelInfo` 的空 getter 段与恒真测试

**裁决**：用户 2026-11-20，判决行 `D-077`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**。**实施顺序：先于 I-31**。来源：`R35/DESIGN-D2-06`（P2，[DEL]）。

**现状（三处残骸，均已核实）**：

| # | 位置 | 内容 |
|---|---|---|
| ① | `os/libs/minix-boot/src/kernel_info.rs:631-635` | 注释「Getter methods (preferred API over direct field access)」+「These getters are the recommended API」 |
| ② | 同文件 `:636-647` | **12 行空白**，`:648` 直接 `}` 闭合 `impl`——全文件无任何 `pub fn <字段名>` 形态的 getter |
| ③ | 同文件 `:1092-1121` | `test_getters_return_field_values` 的 **17 条断言全部是 `assert_eq!(x, x)`**（左右操作数为同一表达式，永真） |
| ④ | `01-boot-shim-bootstrap.md:3249` | 把它计入「该文件 32 条」，覆盖列声称验证「getter 与字段一致」——**声称的接口不存在** |

**性质**：`test-audit` 定义的「虚构测试」（对账双方不在场）。一条永不可能失败的测试被当作覆盖证据，稀释真正的不变量覆盖。

**做法（A：删两处残骸）**：

1. 删 `kernel_info.rs:631-648`（注释段 + 12 行空白，保留 `}` 闭合）。
2. 删整个 `test_getters_return_field_values`（`:1092-1121`，含其后的空行）。
3. 文档 `01-boot-shim-bootstrap.md:3249`：`kernel_info.rs` 测试计数 **32 → 31**，覆盖列删掉「getter 与字段一致」（该行现文为 `test_getters_return_field_values`（1 条）= 该文件 32 条 | ... getter 与字段一致`）。

**不做的事（裁决明确）**：
- **否决 B（真造 getter）**：`KernelInfo` 字段已全 `pub`，注释自述「Direct field access remains `pub` for backward compatibility」。造 getter 会制造两套等价 API——与 D-067/I-34 刚消灭「两个可不一致的事实」的方向相反。
- **否决 C（改名保留）**：`assert_eq!(x,x)` 改名成「可读可复制」仍是恒真；`Copy` 已由 `#[derive(Clone, Copy)]` 编译器保证，测试无增量价值。
- **不新增机械防护**：`assert_eq!(a, a)` 扫描作为 test-audit 的工具候选另行登记（本批不做）。

**与 I-31 的关系（为什么先做）**：I-31 删 `user_sp`/`kern_stack_top`/`syscall_entry` 三字段后，该测试里三条 `info.user_sp == info.user_sp` 之类会**因字段不存在而编译失败**。A 是纯删除，先做则 I-31 少改一处；反序则 I-31 必须顺手处理这个测试，等于同一段改两遍。

**验收**：
- `grep -c "Getter methods" os/libs/minix-boot/src/kernel_info.rs` → 0；
- `grep -c "test_getters_return_field_values" os/libs/minix-boot/src/kernel_info.rs` → 0；
- `grep -n "assert_eq!(info." os/libs/minix-boot/src/kernel_info.rs` 无 `x, x` 恒真形（可人工核）；
- `cargo test -p minix-boot` 全绿；`grep -c '#\[test\]' os/libs/minix-boot/src/kernel_info.rs` → 31，与文档 §5.2.1 的 31 一致；
- 文档六门（anchor/citation/structure/ref/style/density）在受影响的 01 篇上复跑全绿。

**风险与回退**：纯删除，零行为变化。唯一风险是文档计数改错（用 `grep -c '#\[test\]'` 实跑核对，不靠手算）。若某处代码真的在调用 getter（当前全仓零命中）→ 回退并重开 D-077 评估 B。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

D-077 授权的三项全部落地；实现时发现文档计数不止 §5.2.1 一处，同批补拓两处（见下表 3b/3c）：

| # | 改动 | 位置 |
|---|---|---|
| ① | 删 12 行空 getter 注释段与 11 行空白，`impl` 以 `}` 闭合 | `os/libs/minix-boot/src/kernel_info.rs`（原 `:631-648`） |
| ② | 删 `test_getters_return_field_values`（17 条 `assert_eq!(x,x)`） | 同文件（原 `:1092-1121`） |
| ③a | 逐文件表计数 32 → 31 | `01-boot-shim-bootstrap.md:3214` |
| ③b | §5.2.1 覆盖行去掉「`test_getters_return_field_values`（1 条）」与「getter 与字段一致」，合计 32 → 31 | 同篇 `:3249` |
| ③c | **mini review 补拓**：同一篇的转录块 `running 196 tests` → `running 195 tests`、逐文件复算句 `minix-boot 的 196 = kernel_info.rs 的 32` → `195 = 31`，并注明「删去 D-077 的恒真测试 −1」 | 同篇 `:3183-3184`、`:3228` |

3b/3c 超出一句授权文本，但不是扩范围：§5.2.1 的 31 若只改覆盖行而不改同节的逐文件表与合计式，文档内部三个数会互相矛盾——按 D-077 的 `reopen_condition`（「删测试后 §5.2.1 计数与 `grep -c '#[test]'` 对账不上 → 按 test-audit 全文件复核」）这正是要求同批对账的情形。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -c "Getter methods" os/libs/minix-boot/src/kernel_info.rs` | **0** |
| A2 | `grep -c "test_getters_return_field_values" os/libs/minix-boot/src/kernel_info.rs` | **0** |
| A3 | 全文恒真 `assert_eq` 扫描（Python 逐行比对左右操作数文本） | **0 命中** |
| A4 | `grep -c '#\[test\]' os/libs/minix-boot/src/kernel_info.rs` | **31**（与文档 §5.2.1 一致） |
| A5 | 逐文件复算 31+8+8+74+2+10+21+29+2+5+5 | **195**（与 `cargo test` 实跑一致） |
| A6 | `cargo test -p minix-boot` | **195 passed / 0 failed** |
| A7 | `cargo test -p minix-boot -p minix-elf --release` | **195 + 23 passed / 0 failed** |
| A8 | `cargo check --workspace` | **0 error**（`kernel_info` 相关新警告 0） |
| A9 | `cargo clippy -p minix-boot --all-targets` | **0 error** |
| A10 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`；structure/ref/style(error=0)/density 通过；doc-code-map **54/54**；`eval-trace-check --round R35` **PASS(13/6/0)** |

**mini review 发现并登记的两处同源残骸（未修，超出 D-077 授权）**：`kernel_info.rs:640` 的 `/// deliver, in \`KernelInfo::boot_modules()\`.` 与 `os/kernel/src/vm_handoff.rs:108` 的「from the raw \`KernelInfo::memmap()\`」——`KernelInfo` 上这两个方法都不存在（`boot_modules` 是 pub 字段；`pub fn boot_modules()` 只在 `BootStorage` 上，`memmap` 同理）。与 D-077 处理的 getter 段是**同一缺陷类**（注释承诺不存在的 API），但位置不同、不属于本次裁决授权。已登记为 **I-40**（见 §3）待裁决，不在本次修改中改动。

**取证方法（对照 D-077 判决行记的三次失误）**：D-077 的前身失误模式是「从注释推断代码、未逐行核对」。本次反其道执行：① 删测试前先用 Python 逐行比对 `assert_eq!` 左右操作数文本，独立确认 17 条全为 `x == x`（没有只读测试顶部的 `// compare lengths and first elements` 注释就下结论）；② 删之前先用 `grep -c '#\[test\]'` 实跑拿到 32（与 §5.2.1 记录一致），删后再跑拿到 31；③ `cargo test` 实跑 196→195，不是按减一推算。三处读数全部来自命令输出。

**为什么此前不做**：用户于 2026 年 11 月 20 日批准方案但只做记账；本条在裁决后由用户单独指示实施，并按 D-077 的排序要求**先于 I-31** 完成。

### I-40（已裁决·I-39 的实现副产物）两处 ghost API 注释

**来源**：I-39 实施时的 mini review。D-077 修掉的是「`impl` 末尾承诺一段 getter，实际一个都没有」；实施后按同一判据（注释是否点名一个不存在的方法）扫全文件，又发现两处：

| # | 位置 | 现文 | 事实 |
|---|---|---|---|
| ① | `os/libs/minix-boot/src/kernel_info.rs:640` | `/// deliver, in \`KernelInfo::boot_modules()\`.` | `KernelInfo` 的 `boot_modules` 是 **pub 字段**；`pub fn boot_modules()` 只存在于 `BootStorage`（`boot_storage.rs:336`） |
| ② | `os/kernel/src/vm_handoff.rs:108` | `/// select from this list, not from the raw \`KernelInfo::memmap()\`: the` | 同上：`memmap` 在 `KernelInfo` 上是字段，方法只在 `BootStorage` 上 |

**为什么没有顺手改**：D-077 是用户裁决，授权范围写明是「空 getter 段 + 恒真测试 + 文档计数 32→31」三件；这两处虽属同一缺陷类（注释点名不存在的 API，读者会白搜一次），但位置与对象都不同。按 `fix-guard` 的「一次一条」，混入本次提交会让裁决边界变模糊——尤其 ② 在 `minix-kernel` 而不在 `minix-boot`，跨 crate。

**裁决（D-079，用户 2026-11-20）：采 A——改写措辞指回真实载体。**

| # | 现文 | 改为 |
|---|---|---|
| ① | `/// deliver, in \`KernelInfo::boot_modules()\`.` | `/// deliver, in \`KernelInfo::boot_modules\`.` |
| ② | `/// select from this list, not from the raw \`KernelInfo::memmap()\`: the` | `/// select from this list, not from the raw \`KernelInfo::memmap\` field: the` |

**否决 B（真加两个访问器）**：与 D-077 否决 B 的理由完全相同——`KernelInfo` 字段已全 `pub`，造 getter 会制造两套等价 API，与 I-34/D-067 消灭「两个可不一致的事实」的方向相反。

**范围界定（重要）**：全仓扫 `KernelInfo::[a-z_]*(` 共三处命中，第三处 `os/boot-shim/src/lib.rs:103` 的 `KernelInfo::kern_stack_top()` **不是 ghost**——`os/libs/minix-boot/src/kernel_info.rs:200` 确有 `pub fn kern_stack_top(&self) -> VirBytes`（I-31 删字段后补的派生方法）。主 agent 初判时曾误以为第三处也是 ghost，核实后排除。

**验收**：

```bash
grep -rn "KernelInfo::boot_modules()" os/ --include=*.rs     # 期望 0 行
grep -rn "KernelInfo::memmap()" os/ --include=*.rs           # 期望 0 行
grep -rn "KernelInfo::kern_stack_top()" os/ --include=*.rs   # 期望仍命中 boot-shim/src/lib.rs:103
grep -n "pub fn kern_stack_top" os/libs/minix-boot/src/kernel_info.rs   # 期望命中 :200
cargo check --workspace --all-targets                        # 纯注释，0 error
cargo test -p minix-boot && cargo test -p minix-kernel --features mock
```

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：两处注释按 D-079 采 A 改写（去掉 `()`，指回字段）。验收逐条：

| 项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -rn "KernelInfo::boot_modules()" os/ --include=*.rs` | **0** |
| A2 | `grep -rn "KernelInfo::memmap()" os/ --include=*.rs` | **0** |
| A3 | `grep -rn "KernelInfo::kern_stack_top()" os/ --include=*.rs` | 仍命中 `boot-shim/src/lib.rs:103`（非 ghost，按裁决保留） |
| A4 | `grep -n "pub fn kern_stack_top" os/libs/minix-boot/src/kernel_info.rs` | 命中 `:200`（方法确实存在） |
| A5 | `cargo check --workspace` | **0 error**（纯注释） |
| A6 | `cargo test -p minix-boot` / `-p minix-kernel --features mock` | **183 / 864 passed** |

**为什么合并到一次提交**：D-079 与 D-080 都是纯删除/纯注释、零行为变化、都在并发会话的同一天裁决；分两次提交只增加记账成本，不增加可追溯性（两段 §3 记录各自独立）。

**风险与回退**：纯注释，无。若将来 `KernelInfo` 真的加访问器接口 → 重开本条评估 B。

### I-38（已裁决·待实施，前置 I-31）`boot_validate_and_prepare` 的四个位置实参改 `BootScalars`

**裁决**：用户 2026-11-20，判决行 `D-074`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**（用户批准方案，本条只记账）。**前置依赖：I-31（D-062）**。来源：`R35/DESIGN-D2-04`（P2，[MODERN]）。

**为什么有前置依赖（I-31 先做）**：

`BootScalars` 的字段集取决于 `KernelInfo`/`BootReport` 在 I-31 之后还剩什么。I-31 会删除 `kern_stack_top`（连同 `user_sp`/`syscall_entry`）。所以：

| 顺序 | `BootScalars` 定型 |
|---|---|
| **I-31 先**（推荐） | **三字段**：`kern_virt_base` / `kern_phys_base` / `kern_size`；`boot_validate_and_prepare` 内部派生 `kern_stack_top = kern_virt_base + kern_size` |
| D2-04 先 | 四字段；I-31 落地后再改一次，`BootScalars` 定型两次 |

**现状（为什么这是个问题）**：

```rust
// os/kernel/src/lib.rs:839-844
fn boot_validate_and_prepare<A: BootArch>(
    kern_virt_base: VirBytes,     // ← VirBytes
    kern_phys_base: PhysBytes,
    kern_size: u64,
    kern_stack_top: VirBytes,     // ← VirBytes（与第一个同类型，可互换）
) -> BootPageSupport<A>
```

三个调用点全是「从结构体抄四字段」，四行抄三遍：

| 行 | 调用者 | 来源 |
|---|---|---|
| `:334-339` | `resume_additive` | `kernel_info: &KernelInfo` |
| `:446-451` | `prepare_kernel_image_v1` | `report: &BootReport` |
| `:929-934` | `arch_boot_impl` | `kernel_info: &KernelInfo` |

**为什么函数调用比结构体字面量更危险（用户提问的回答，记录在案）**：Rust 结构体字面量按**字段名**配对，书写顺序无关——「错序」不存在；「名字对、值错」存在但 A 也拦不住。函数调用的形参名**不出现在调用点**，调用者只能按记忆排位置，属**被动犯错**；结构体字面量必须名字与值写在同一行，属**主动犯错**。

**A 的真正价值是「收敛暴露面」而非「防错」**：现状 3 个调用点 × 4 个位置槽 = 3 个独立出错点（每处还有两个相邻同类型槽）；A 之后是 1 个构造函数内的 4 行「名字 + 值同行」配对，出错面缩为 1 处。三个调用点的四行抄字段也随之消失。

**现有防线为什么拦不住**：函数内两条对齐断言（`:874-882`）——`kv.is_multiple_of(0x1000)` 与 `kern_stack_top.0.is_multiple_of(16)`——在交换后**都仍通过**（栈顶通常也页对齐、基址 2 MiB 对齐必然 16 对齐）。而 `KernelInfo` 的严格模式断言（`kernel_info.rs:465-470`）检查的是**结构体自身字段关系**，与**传参顺序**无关，同样拦不住。

**后果评估（比面板描述的轻）**：面板称「内核按错误虚拟基址规划映射，现场与页表建错难以区分」。**核实后不成立**——映射规划的 `KernelLayout` 是从 `report`/`kernel_info` **直接取字段**构造的（`:456-462`），不经过该函数的形参；该函数返回 `BootPageSupport<A>`（`paging_ext.rs:130-136`，四个字段全是平台能力，不含那四参数）。所以接反的真实后果限于「断言检查了错误对象」（且两个都对齐，仍通过）。**但这不降低改造价值**：它是隐患——函数名叫 validate *and prepare*，将来若真用 `kv` 算布局，接反立刻变真故障。

**做法**：

1. 新增值对象（放 `os/kernel/src/lib.rs`，紧邻 `boot_validate_and_prepare`）：

```rust
#[derive(Debug, Clone, Copy)]
struct BootScalars {
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    // I-31 落地后此字段删除，改为在 `boot_validate_and_prepare` 内部派生
    kern_stack_top: VirBytes,
}

impl BootScalars {
    /// Named constructors, not `From`: the two sources would read ambiguously
    /// as `BootScalars::from(x)`.
    fn from_kernel_info(ki: &KernelInfo) -> Self {
        let s = Self {
            kern_virt_base: ki.kern_virt_base,
            kern_phys_base: ki.kern_phys_base,
            kern_size: ki.kern_size,
            kern_stack_top: ki.kern_stack_top,
        };
        // The only hole `BootScalars` cannot close: same-typed fields bound to
        // each other's values. This equality is checked in `KernelInfo`'s
        // strict mode (`kernel_info.rs:465-470`); repeat it here so the
        // constructor is self-contained.
        debug_assert_eq!(
            s.kern_stack_top.0,
            s.kern_virt_base.0 + s.kern_size,
            "BootScalars: kern_stack_top must equal kern_virt_base + kern_size"
        );
        s
    }
    fn from_report(r: &minix_boot::BootReport<'_>) -> Self { /* 同形 */ }
}
```

2. `boot_validate_and_prepare<A>(scalars: BootScalars) -> BootPageSupport<A>`，函数体内 `let kv = scalars.kern_virt_base.0;` 等。
3. 三个调用点改为 `boot_validate_and_prepare::<X>(BootScalars::from_kernel_info(kernel_info))` / `(BootScalars::from_report(report))`。
4. **I-31 落地后的收尾**：删 `kern_stack_top` 字段，`boot_validate_and_prepare` 内 `let kern_stack_top = VirBytes(kv.checked_add(ks).expect(...))`；`from_*` 的等式断言随之删除（派生值天然成立）。

**验收**：
- `grep -n "fn boot_validate_and_prepare" -A 6 os/kernel/src/lib.rs` 形参为单个 `BootScalars`；
- `grep -c "kernel_info.kern_virt_base\|report.kern_virt_base" os/kernel/src/lib.rs` 在三个调用点处归零（改为构造函数内）；
- `cargo test -p minix-kernel --features mock`、三架构 `cargo check -p minix-kernel`（生产形态）全绿；
- 五条 QEMU 门链 PASS（本改动触及全部启动腿）；
- 文档若引用该函数签名，同批更新。

**风险与回退**：`BootScalars` 私有、单 crate、不改外部 ABI。唯一风险是 `KernelInfo.kern_size` 与 `BootReport.kern_size` 若类型不同则构造函数无法共用（已核：**两者都是 `u64`**，`kernel_info.rs:97`/`boot_storage.rs:416`）。若将来变成不同 newtype，构造函数各自持有字段类型即可。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

I-31 已先落地，所以本条按**三字段**形态一次定型（不需要"四字段再改三字段"的二次定型）。

| # | 改动 | 位置 |
|---|---|---|
| ① | 新增 `struct BootScalars { kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64 }` | `os/kernel/src/lib.rs`，紧邻被改造函数 |
| ② | 两个命名构造函数 `from_kernel_info(&KernelInfo)` / `from_report(&BootReport)` | 同上 |
| ③ | `boot_validate_and_prepare<A>(scalars: BootScalars)`，函数体开头 `let kv = scalars.kern_virt_base.0; …`，并就地派生 `kern_stack_top = kv.checked_add(ks)`（溢出 panic） | 同上 |
| ④ | 三个调用点改 `BootScalars::from_kernel_info(..)` / `::from_report(..)` | `:334` 的 `resume_additive`、`:473` 的 `prepare_kernel_image_v1`、`:1007` 的 `arch_boot_impl` |

**一处必须如实记录的论证变化（我实施时才确认）**：D-074 的分析写在本条裁决时，那时 `boot_validate_and_prepare` 是**四个**位置实参，其中 `kern_virt_base` 与 `kern_stack_top` **同为 `VirBytes`**——交换后仍能编译，这是本条原始风险的来源。但 **I-31 先落地删掉了 `kern_stack_top`**，剩余三个参数是 `VirBytes` / `PhysBytes` / `u64`，**三个不同类型，交换即编译失败**。也就是说：

- **原来的「同类型槽互换」风险已经由 I-31 消除**，不是由本条消除的；
- 本条的实际收益只剩「收敛暴露面」：三个调用点各抄三字段（共 9 行重复 → 2 个构造函数内各 3 行「名字 + 值同行」）；
- 交叉赋值仍拦得住（`kern_virt_base: report.kern_phys_base` → `E0308`，已实测），但那是 `VirBytes`/`PhysBytes` 类型差异的功劳，I-31 之前也有。

按 D-074 的 `reopen_condition`（「出现 `boot_validate_and_prepare` 需要第五个标量且该标量来自不同来源」）没有触发，所以本条仍按裁决实施；但**如果当初按「I-31 先做」的真实收益重算，本条的优先级会低于裁决时的判断**。这一点留在记录里，供后续同类裁决参考：**值对象的收益取决于同类型槽的数量，而削参数本身可能先把同类型槽消灭掉**。

**为什么 `BootReport` 的三个派生字段没有一起删（登记不改）**：`BootReport` 上仍有 `user_sp`/`kern_stack_top`/`syscall_entry` 三个字段，且 `BootReport::report()` 仍在算它们（`boot_storage.rs:335-338`）。实测它们的**生产侧读者为零**——`grep -rn "report.user_sp\|report.kern_stack_top\|report.syscall_entry" os/` 除 `boot_storage.rs` 自己的三条测试断言外无命中，且 I-38 改造后 `report.kern_stack_top` 连 `boot_validate_and_prepare` 也不再读。这与 I-31 删 `KernelInfo` 三字段是**同一缺陷类**（发送侧视图保留了两端可算的派生值），但 D-074 的授权范围只有 `boot_validate_and_prepare` 的形参。按 fix-guard「一次一条」登记为 **I-41**（见 §3），不在本次改动里动。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -n "fn boot_validate_and_prepare" -A 3 os/kernel/src/lib.rs` | 形参为单个 `scalars: BootScalars` |
| A2 | 三个调用点的 `kernel_info.kern_virt_base` / `report.kern_virt_base` 抄字段 | **归零**（改为构造函数内），三处均变为 `boot_validate_and_prepare::<X>(BootScalars::from_*(..))` |
| A3 | `cargo test -p minix-kernel --features mock` | **864 passed / 0 failed / 8 ignored**（与 I-31 后一致，本条零测试增减） |
| A4 | `cargo check -p minix-kernel` 三架构生产形态 | 全部 **0 error** |
| A5 | `cargo check --workspace` | **0 error** |
| A6 | **五条 QEMU 门链** | x86 UEFI **PASS**；aarch64 UEFI **PASS**；riscv U-Boot shim **PASS**（6/6 标记）；aarch64 直核 SMP **PASS**；riscv 直核 bootface **PASS** |
| A7 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`；structure/style(error=0) 通过 |

**鉴别力验证**：在 `from_report` 里故意写 `kern_virt_base: report.kern_phys_base` → `error[E0308]: mismatched types`（编译期拒绝）。如上面的论证变化所述，这条鉴别力来自 newtype 类型差异，不是本条新增的。

**为什么此前不做**：用户 2026 年 11 月 20 日批准方案但只做记账；且本条**必须等 I-31 落地**后实施，否则 `BootScalars` 要定型两次。I-31 于同日完成后本条随即实施。

### I-41（待裁决·I-38 的实施副产物）`BootReport` 上三个零读者的派生字段

**来源**：I-38 实施时的 mini review。

**现状**：`BootReport`（`os/libs/minix-boot/src/boot_storage.rs`）仍有三个字段，`BootReport::report()` 每次调用都算：

| 字段 | 当前来源 | 生产侧读者 |
|---|---|---|
| `user_sp` | `VirBytes(minix_types::USER_STACK_TOP)`（`:336` 附近） | **零**（唯一命中是 `boot_storage.rs:440` 的测试断言） |
| `kern_stack_top` | `VirBytes(kern_virt_base.0 + kern_size)` | **零**（`:438` 的测试断言；I-38 改造后连 `boot_validate_and_prepare` 也不再读） |
| `syscall_entry` | `VirBytes(kern_virt_base.0)` | **零**（`:439` 的测试断言） |

它们是 I-31 删掉 `KernelInfo` 同名三字段后的**发送侧残留**：`BootReport` 是 shim 进程内的借用视图（`&BootReport<'_>` 传给 `enter_kernel_image_v1`），不是 wire 类型，所以 I-31 的 v10 升版没有波及它；但它同样在「把恒等式抄一遍」。

**待裁决的两个选项**：

- **A（推荐）**：删三个字段与 `report()` 里的三行赋值，连同三条测试断言。纯删除，零行为变化——生产侧读者实测为零。
- **B**：保留，等将来真出现第三个消费者再说。

**验收（若走 A）**：

- `grep -rn "report.user_sp\|report.kern_stack_top\|report.syscall_entry" os/ --include=*.rs` → 0；
- `cargo test -p minix-boot`、`cargo test -p boot-shim --features test-all`、`cargo test -p minix-kernel --features mock` 全绿；
- 五条 QEMU 门链不回归（`BootReport` 参与全部 v1 发送腿）。

**为什么没有并入 I-38**：D-074 的授权范围是 `boot_validate_and_prepare` 的形参改造，不含 `minix-boot` 的类型删减（跨 crate）。按 fix-guard「一次一条」登记。

### I-43（未裁决·I-31 补漏副产物）编译修好后暴露的测试内核运行期缺陷

**来源**：I-31 补漏批二（修 30 个测试内核编译失败）时，编译修好后跑 QEMU 暴露。

**现象**：四条内核在 `arch_boot_impl` 的 `step0 validate ok` 之后立刻 panic：

```
### PANIC in test-timer-irq-riscv64: kernel/src/lib.rs boot identity window is empty or reversed: [0x0,0x0)
```

**真因（已定位）**：这四条内核用同一个构造模式——`static mut BOOT_MODULES: [BootModule; 12]` 是全零数组，内核只填 `mods[8]` 一项，却把**整个 12 槽数组**当成 `boot_modules` 切片交出。`push_common_boot_windows` 里的 `push_module_windows` 逐项 `windows.push(start, start + len)`，遍历到第 0 槽（`0, 0`）就撞 `BootWindows::push` 的 `assert!(start < end)`。

**为什么此前没人发现**：I-31 之前这四条**编译不过**，`run_all.sh` 记 build failure、不跑 QEMU。唯一在读数里显示 PASS 的 `test-timer-irq-riscv64` 是 `target/` 里残留旧二进制被直接加载的结果（同一份读数把它同时列在 `build-failed packages` 里，自相矛盾）。

**裁决（D-081，用户 2026-11-20）：采 A——测试载体侧交出真实前缀 / 真实 span。**

**否决 B（内核侧跳过空槽）**：`boot_modules` 在生产腿上**永远是已提交的真实前缀**（boot-shim 按 `MODULE_NAMES` 逐项填，`NR_BOOT_MODULES` 是编译期常量），不存在"空槽哨兵"这个概念。B 会把一个生产中不存在的语义写进内核，并把真实的生产不变量（"每个模块窗口都必须非空"）降级成"可以有例外"。

**射程（裁决时实测，与历史记录不同）**：全仓 **12 个**测试内核都用 `static mut BOOT_MODULES: [BootModule; 12]` 模式，其中 **6 条已按 A 修过**（带 `PLACEHOLDER_SPAN`，见下方"家族级定论"段），**另 6 条仍为全零槽 + `mods` 整组交出，未修**：

| 已修 | 未修（全零槽 + `mods` 整组交出）|
|---|---|
| `test-timer-irq` / `-aarch64` / `-riscv64` | `test-sysboot`（`:143`）|
| `test-rt-birth-aarch64` / `-riscv64` | `test-rt-birth`（`:134`）|
| `test-proc-init` | `test-smp-ipi`（`:133`）|
| | `test-paging-faultloop`（`:521`）|
| | `test-user-trap`（`:208`）|
| | `test-smp-aps`（`:206`）|

**未修六条的形态与已修六条完全相同**，只是它们当时不在 QEMU 门链的必跑集合里，所以没被暴露。**跟进项：把这六条也改成 `PLACEHOLDER_SPAN` 形态。**

**建议统一到 `PLACEHOLDER_SPAN`，而不是 `&mods[8..9]`**：`test-sysboot` 填了 `mods[1]` 与 `mods[8]` 两个**不连续**槽，`&mods[8..9]` 套不上；`PLACEHOLDER_SPAN` 让 12 槽全部带有效 span，不依赖连续性，也不容易在新增模块时写错。

两种写法在已修六条上并存（实施时的历史遗留）：`test-timer-irq-aarch64:239`、`test-rt-birth-riscv64:520`、`test-rt-birth-aarch64:597` 用 `&mods[8..9]`；`test-timer-irq`、`test-timer-irq-riscv64`、`test-proc-init` 用 12 槽全有效。

**验收**：

```bash
cd os/qemu-tests && bash run_all.sh    # 期望：零 "boot identity window is empty or reversed"
cargo check --workspace --all-targets  # 三架构 0 error
```

**追加第 5 条（同日发现，形态不同）**：`test-protection-riscv64` 也是「编译失败掩盖运行期缺陷」的受害者，但症状不同——它**没有 panic，而是静默卡住**。串口停在 `kernel: step1+2 mappings ok`，之后 `paging.enable()` 与返回都不出声，测试自己的 `"  Sv39 MMU enabled\n"` 从未出现。它的 I-31 相关改动只有一处、且在 `arch_boot_impl` **返回之后**（`trap.configure_syscall(minix_arch::syscall_entry_va())`），卡点在 `arch_boot_impl` **内部**，所以与本条无关。需要单独定位「riscv 直核腿 `paging.enable()` 之后的静默停」。

**实施进展（2026 年 11 月 20 日）：4 条空槽 `BOOT_MODULES` 已按 A 修掉，但暴露了第二层缺陷。**

- **改动**：四条内核把 `mods`（整个 12 槽数组）改成 `&mods[8..9]`（只交出真正填过的那一槽），并在注释里写明为什么。四条均编译 0 error。
- **效果**：`boot identity window is empty or reversed: [0x0,0x0)` **不再出现**（grep 零命中）——这一层已闭合。
- **暴露的第二层（`test-timer-irq-riscv64`）**：`rust_main` 在 `paging enabled` 之后**被重新进入**——串口紧接重打印标题行 `### test_timer_irq (riscv64): …`。内核在 `arch_boot_impl` 返回之后、`init_protection` 之前崩溃/异常并跳回 `_start`，反复 64 次把 32 MB 的 bump 区耗尽，最终 `bump(256)` 返回 `None` 触发 `panic!("bump region")`。
- **为什么诊断是必要的**：`bump()` 在源码里只有 2 处调用、都无循环，但诊断打印出现 64 次且 `cur` 单调递增——这个矛盾只有「`rust_main` 被反复重入」能解释。诊断用 `cur/need/end` 三次打印取证，之后**已完全撤除**（`grep -c "DBG bump"` → 0）。
- **这一层不属于本条**：它不是「空槽模块」问题，是 `arch_boot_impl` 之后的路径在 riscv 直核腿上崩溃并重入。需要单独定位（`init_protection` / `init_clock_and_interrupts` / 进程表 三者之一，或 `arch_boot_impl` 返回时的栈/寄存器状态）。已在本条内记录现象与取证方法，待后续专项。

**家族级定论与全量修复（2026 年 11 月 20 日续）**：

`git log -S` 确认 `push_module_windows` 自 BOOT-SCOPE 2a（提交 `4f2e9e786`）引入起**从不跳过空槽**——「12 零槽 + stub」载体家族从那天起就是运行期坏的，历次 PASS 全部来自陈旧二进制。I-31 只是用强制重编揭开了它。

实证范围（全部用新编译的二进制直跑 QEMU 复核）：

| 载体 | 修前（新二进制） | 修复后 |
|---|---|---|
| `test-proc-init`（x86） | `[0x0,0x0)` panic | 过窗口层，`step1+2` 后静默挂（下一层，见下） |
| `test-timer-irq`（x86） | 同上 | 同上 |
| `test-timer-irq-riscv64` | 同上 | **推进到 `init_proc_and_boot` 内部**：DTB 窗口 + 12 有效 span 后过 `protection live`，现困在 VM 加载路径 |
| `test-rt-birth-aarch64` / `-riscv64` / `test-timer-irq-aarch64` | 同上 | 编译 0 error，未逐一跑门链 |

**修法（按裁决 A 落地，六条全改）**：12 槽全部带有效 span——11 个 name-only 槽用编译期常量占位（x86 `0x100_0000`、aarch64 `0x4100_0000`、riscv `DRAM_BASE + 0x0100_0000`，都在各自 MEMMAP 内），vm 槽运行时指向真实 stub。占位 span 不是伪造数据：name-only 槽在 Phase C 是 `EntrySpec::DEFERRED`（`kernel/src/lib.rs` else 分支明写「Other user processes have no ELF loaded at boot. RS will load them at runtime」），span 只用于恒等窗口与扣减记录。

**test-timer-irq-riscv64 的第二层定位**：DTB 解析崩溃的根因是 **DTB 物理地址不在任何恒等窗口内**——载体把 `platform_sources` 留空，而 `push_platform_windows` 只为声明过的 source 建窗。修法：运行时填 `PLATFORM_SOURCES[0] = PlatformDescSource::new(DTB, PhysBytes(dtb_phys))` 再交给 `KernelInfo`（生产 `bootface.rs` 的同构做法，且生产顺序是先解析后 `arch_boot`）。此后 `platform desc installed (DTB)` 与 `protection live` 均通过。

**剩余两层（各自需要专项，均已定位到具体位置）**：

1. **`test-timer-irq-riscv64` 的 VM 加载 trap**：`init_proc_and_boot` 的 VM 分支（`load_vm_elf`）触发 store page fault，`stval = 0xffffffc0_cffff000`——**生产 riscv 内核的高半基址形态**（`riscv64.ld` 的 `KERNEL_VIRT_BASE`）。生产 VM 加载路径假设内核链在高半；这条载体链在 `0x80200000` 物理视图（VMA=LMA），路径里有代码按高半假设写内存。需要专项核对 `frame_virt`/`CurrentPaging` 在低链载体上的地址换算。
2. **x86 两载体的静默挂**：`test-proc-init` / `test-timer-irq` 过窗口层后停在 `paging.enable()`（`step1+2` 之后无输出、无 panic）。同 `test-protection-riscv64` 的形态——`enable()` 换根之后串口路径或栈假设失效。三个载体同一症状，指向**载体链址假设 vs 当前 `enable()` 实现的系统性漂移**，不是各 Carrier 的孤立 bug。

**给后续专项的边界**：这两层都是「载体从未在新鲜二进制下运行过」的累积漂移，修复载体不会触碰内核生产路径；每条都需要 QEMU 单步（`-d int`）级取证，不适合在本条内继续剥。

**二层闭合（2026-11-20 续）：`enable()` 后静默停的真因定位与修复。**

上文「剩余两层」第 2 条对 x86 静默挂的描述（「载体链址假设 vs 当前 `enable()` 实现的系统性漂移」）是笼统猜测；本轮用串口诊断打印（`rsp` + `image_base`，取证后撤除）拿到了精确根因，是**两个独立缺陷的叠加**：

1. **载体侧：硬编码 `kern_phys_base`/`kern_size` vs 固件运行期装载地址。** hello-boot 实证：`image_base=0x5E0_0000`（OVMF 把 PE 装到 94 MiB 处，硬编码 `0x200_000` 完全错误）。BOOT-SCOPE 2a 之后窗口清单是最小集，真实代码从此不在窗口内。先修成 `main` 锚点 + 固定 2 MiB（test-timer-irq-aarch64 既有模式），随后发现它罩不住大镜像载体（见 2 的伴生证据），最终升级为**生产同款**：`boot-shim::uefi_helpers::shim_image_window()` 改 `pub`，20 个 UEFI 载体的 `kern_phys_base`/`kern_size` 从 LoadedImage 协议的精确足迹派生（基址 2 MiB 下取、终点 2 MiB 上取，满足 `boot_policy::layout_satisfies` 的倍数约束）。伴生证据：test-proc-init 内嵌 2 MiB `.bss` 堆，固定 2 MiB 窗口下过 `enable()` 后在 step4 打印前静默三重故障（无 IDT 阶段任何 fault 都无声）；换成精确足迹后 **PASS**。

2. **内核侧：直核腿窗口清单漏了「调用者栈」类别。** 诊断实证：hello-boot 的固件栈 `rsp=0x7E6_6470`，镜像窗口 `[0x5E0_0000,0x600_0000)` 不含它——CR3 装载后的第一个栈访问（`ret`）即 fault。窗口清单自己的文档（`os/kernel/src/lib.rs` `arch_boot_impl` 段）列明类别含「shim image/stack」；生产 v1 腿由发送方 `shim_stack_window` 自压，直核腿没有发送方、调用者的栈就是 boot path 自己的栈，却无人压窗。修法：`arch_boot_impl` 在 `push_common_boot_windows` 后补压当前栈窗口（SP 页对齐向上取整后向下 4 MiB、2 MiB 下取，与 `shim_stack_window` 完全同型），三架构内联 asm 读 SP；`BOOT_WINDOW_FLOOR` 注释与产能同步 +1。宿主 mock 测试经 `cfg(not(test))` 排除（宿主 SP 是 Linux 用户栈地址、远超 boot VA 天花板，会触发断言假阳性；先例：`restart_local_timer`）。生产固件上若未来有栈超天花板的端口，断言保持 fail-closed——静默跳过会重新引入本窗口要防的静默停。

3. **一层收尾（D-081-A 执行完毕）**：上文「未修六条」全部按 `PLACEHOLDER_SPAN` 修掉（`test-sysboot` 此前已修）；三种 `&mods[8..9]` 写法（`test-timer-irq-aarch64` / `test-rt-birth-aarch64` / `test-rt-birth-riscv64`）改为 `&mods[..]` 全 12 槽——`init_proc_and_boot` 断言生产契约 `NR_BOOT_MODULES == 12`（main.c:160-162 C 对等），前缀切片永远过不了；这批载体的历史 PASS 同样全部来自陈旧二进制。

**二层闭合的门链证据（全部新鲜二进制）**：

| 腿 | 结果 |
|---|---|
| x86 generic 门 ×14 | **13 PASS**（hello-boot / proc-init / timer-irq / protection / paging-enable / higher-half / smp-topo / ap-alive / memmap / kernel-map / smp-aps / smp-ipi / smp-shutdown）；`test-paging-faultloop` 走专用脚本（其 generic 运行的 PANIC 是它的故障臂深层缺陷，见下） |
| aarch64 基础门 ×5 | hello-boot / higher-half / paging-enable / protection / shutdown 全 **PASS** |
| riscv64 | `hello-boot-riscv64` **PASS**（直核静默停顺带闭合） |
| 专用脚本 | `test-rt-birth.sh`：推进到调度器入口（`RESULT: FAIL`，深层）；`test-sysboot.sh`：过 proc 表 + smp_init 后精确 page fault（vector 14，cr2 有值）；`test-paging-faultloop.sh`：CPL3 载荷按设计在 `FAULT_VA` fault，故障臂把它当 kernel exception PANIC（路由缺陷现形）；`test-user-trap.sh`：CPL3 三页映射完成、载荷已跑，随后 kernel 模式 page fault 在 dispatch PANIC（同族路由缺陷，`trap_dispatch.rs` 同一行号） |

**三层残留清单（全部从「不可诊断的静默停」变为「带完整寄存器现场的可诊断缺陷」，各自专项）**：riscv VM 加载 trap（`stval=0xffffffc0_cffff000` 高半基址形态，同上文第 1 条）；aarch64 `test-timer-irq` 深跑 sync exception（`far=0xffff80804ffff000`）；x86 `test-sysboot` 深跑 page fault（`cr2=0x7ba7000`）；`test-rt-birth-riscv64` 载体自身 `bump(256)` 断言（先于本修复即存在）；faultloop 故障臂路由。

**宿主套件存量红（登记为 I-45，非本条射程）**：HEAD 上 `cargo test -p minix-kernel --features mock` 就红且非确定——8 MiB 默认测试线程栈下多个家族轮流栈溢出（misc::dispatch / syscall_clock 等，失败集逐轮漂移）；`RUST_MIN_STACK=16MiB` 下 861 通过 / 3 失败（BKL 持有不变量族 `smp.rs:2043` + `switch_address_space`，同样逐轮漂移）。CI（`kernel-tests.yml`）无 `RUST_MIN_STACK` 且 2026-10-12 全绿——其后某次改动放大了宿主测试栈消耗或环境差异。本条改动在 `cfg(test)` 下与 HEAD 代码逐字节一致（栈窗口块整体编译剔除），不可能是其诱因；stash 对照实证。

**验收（若走 A）**：四条各自 `run_all.sh` 对应测试 PASS；`cargo check` 三架构 0 error。

### I-45（未裁决·I-43 二层工作时的存量发现）宿主内核测试套件在 HEAD 上红且非确定

**来源**：I-43 二层验证时跑 `cargo test -p minix-kernel --features mock` 发现。**与本会话改动无关**：stash 对照实证 HEAD 同样红；本条改动在 `cfg(test)` 构建下与 HEAD 逐字节一致。

**现象（两层）**：

1. **默认环境（8 MiB 测试线程栈）**：多个测试家族轮流 `stack overflow`（已见 `misc::tests::test_dispatch_*`、`syscall_clock::tests::test_t6_setalarm_*`、`proc_table::tests::test_process_misc_flags_*`），溢出即 SIGABRT 整个测试进程，失败集逐轮漂移（并行调度 + 共享全局态）。
2. **`RUST_MIN_STACK=16MiB`**：859–861 通过 / 3–5 失败，失败集中在 BKL 持有不变量族（`smp.rs:2043` 的 `assume_held` 断言，`test_finish_and_restore_*` / `test_idle_*` / `test_interrupt_stop_*`）与 `test_switch_address_space_kernel_task_is_noop`（`lib.rs` root-mirror 断言）、`vm_handoff::test_bootstrap_frames_*`——同样逐轮漂移。

**对照事实**：CI `kernel-tests.yml` 无 `RUST_MIN_STACK` 设置且 2026-10-12 跑出 831 通过 / 0 失败；本机 WSL2、rustc `e408947bfd200af42db322daf0fadfe7e26d3bd1`。其间大量提交落库（I-29/31/34/35/37/38/42/44 等），某个改动放大了宿主测试的栈消耗或引入共享态竞争，也可能纯是环境差异（CI 容器栈上限不同）。

**需要裁决的方向**：a) 先在 CI 同构环境（docker `minix-ci:1.94`）复跑，分清「代码回归」vs「环境差异」；b) 若是代码回归，用 `git bisect` 定位提交；c) BKL 不变量族失败与栈溢出是否同根（深递归把栈打到限，`lock_boot_globals` 串行化不足）。

**验收**：CI 同构环境全绿，或失败集收敛且逐条归因。

### I-37（已裁决·待实施）`prepare_kernel_image_v1` / `jump_kernel_image_v1` 的裸 `u64` 对改 `PreparedJump`

**裁决**：用户 2026-11-20，判决行 `D-073`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**（用户「按照你的建议来」即批准方案；本条为记账，不实施）。来源：`R35/DESIGN-D2-07`（P3，[MODERN]）。

**现状（为什么这是个问题）**：

```rust
// os/kernel/src/lib.rs:427
pub fn prepare_kernel_image_v1(..) -> (u64, u64) {
    let entry = core::hint::black_box(report.kern_virt_base.0);   // :510
    (entry, arena_pa)                                            // :511 两个裸 u64
}
// :546
pub fn jump_kernel_image_v1(entry: u64, arena_pa: u64) -> ! { .. }
// :529-530（唯一调用点）
let (entry, arena) = prepare_kernel_image_v1(arena_pa, report, root_page, shim_windows);
jump_kernel_image_v1(entry, arena)
```

| 值 | 语义 | 应有类型 |
|---|---|---|
| `entry` | **虚拟**入口（`report.kern_virt_base`）——跳转目标 | `VirBytes` |
| `arena_pa` | **物理** arena 基址——传给接收镜像 | `PhysBytes` |

两者都是 `u64`，**交换后仍能编译**。交换后果：跳转目标变成 arena 的物理地址 → 分页刚开、跳转后第一次取指就故障；`scause`/`CR2` 指向的地址与真实原因无关，排查者会往页表方向查，而真实原因是参数顺序。**属最难归因的启动故障。**

**两点核实**：
- **不是固件边界豁免**：`shim_windows: &[(u64, u64)]`（`:427`）是外部 ABI 形状，文档允许裸地址对；本处是内核内部调用，两端都是本仓代码，无豁免理由。
- **`black_box` 必须保留**：`:510` 的 `core::hint::black_box(report.kern_virt_base.0)` 是编译器屏障（防 LLVM 把跳转目标折叠成编译期字面量，破坏内联汇编的 `in(reg)` 约束）。改造后位置变成 `VirBytes(core::hint::black_box(report.kern_virt_base.0))`。

**先例（同一模式已有两次）**：
- `PlacementPool`（`os/kernel-image/src/bootface_common.rs:151-167`），文档原话：「They used to be three adjacent bare `u64` parameters, so swapping the name-pool address with a bump bound still compiled and produced a plausible-but-wrong placement.」
- `MapCounts { memmap, reserved }`（D-067）。

**做法（A）**：

1. 新增私有值对象（放 `os/kernel/src/lib.rs`，紧邻两个函数）：

```rust
#[derive(Debug, Clone, Copy)]
struct PreparedJump {
    /// Virtual entry of the standalone image's `_start`.
    entry: VirBytes,
    /// Physical base of the handoff arena.
    arena_pa: PhysBytes,
}
```

2. `prepare_kernel_image_v1` 签名改返回 `PreparedJump`，末尾 `(entry, arena_pa)` 改为 `PreparedJump { entry: VirBytes(entry), arena_pa: PhysBytes(arena_pa) }`（`black_box` 保留在 `entry` 的取值处）。
3. `jump_kernel_image_v1(jump: PreparedJump)`，函数体内取 `jump.entry.0` / `jump.arena_pa.0` 供三段 `#[cfg]` 汇编臂使用——**asm 块本身不变**。
4. `enter_kernel_image_v1`（`:529-530`）改为 `let jump = prepare_kernel_image_v1(..); jump_kernel_image_v1(jump)`。

**不做的事（裁决明确）**：
- **保持两个函数分开**：拆分理由（TODO-09 的 post-root/pre-jump 路标位置）虽已由 D-029 判决「不强制增加该诊断打印」而失效，但拆分本身无害，留着不花钱；合并会丢掉将来插路标的位置。
- **`shim_windows: &[(u64, u64)]` 不动**：固件边界是外部 ABI 形状，类型化会引入与固件定义的对齐负担。

**验收**：
- `grep -n "-> (u64, u64)" os/kernel/src/lib.rs` 在 `prepare_kernel_image_v1` 处归零；
- `grep -n "fn jump_kernel_image_v1" -A 1 os/kernel/src/lib.rs` 形参为 `PreparedJump`；
- `grep -c "core::hint::black_box(report.kern_virt_base.0)" os/kernel/src/lib.rs` → 1（保留）；
- `cargo test -p minix-kernel --features mock`、三架构 `cargo check -p minix-kernel`（含生产形态）全绿；
- 五条 QEMU 门链（x86 UEFI、aarch64 UEFI、aarch64 直核 SMP、riscv U-Boot shim、riscv 直核 bootface）PASS——本改动触及全部跳转路径，门链不可省；
- 文档 §4.8 若引用这两个函数的签名，同批更新。

**风险与回退**：`PreparedJump` 不改变外部 ABI；唯一风险是某架构的汇编臂读取字段时类型不匹配（`VirBytes.0` / `PhysBytes.0` 都是 `u64`，直接取 `.0` 即可）。若某臂需要裸值做别的用途 → 在臂内局部解构，不改结构体形状。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

D-073 的四步全部落地，另有两处 mini review 的修正：

| # | 改动 | 说明 |
|---|---|---|
| ① | 新增 `PreparedJump { entry: VirBytes, arena_pa: PhysBytes }` | 紧邻两个函数，带 `#[cfg(all(not(feature="mock"), any(三个 target_arch)))]`——**规格没写这条 cfg**（见下 mini review 第 2 条） |
| ② | `prepare_kernel_image_v1` 返回 `PreparedJump` | `black_box` 保留在内层 `u64` 上，包在 `VirBytes(...)` 里（顺序不能反：屏障要作用在值上） |
| ③ | `jump_kernel_image_v1(jump: PreparedJump)` | 函数体开头用结构体解构 `let PreparedJump { entry: VirBytes(entry), arena_pa: PhysBytes(arena_pa) } = jump;`，**三段汇编块逐字未动** |
| ④ | `enter_kernel_image_v1` 改为 `let jump = ...; jump_kernel_image_v1(jump)` | 消除了原来的 `let (entry, arena) = ...` 元组解构 |
| ⑤ | 文档 §4.8 引文块同步 | 签名三处、`PreparedJump` 定义段、以及块前的说明文字 |

**mini review 修正的两处（都是我自己引入的，不是规格问题）**：

1. **`private_interfaces` 警告 ×2**（三个生产目标各报两条）。规格与 D-073 都写「私有值对象」，但那两个函数是 `pub`——`pub fn` 的签名里出现私有类型，外部 crate 拿到返回值却无法命名它，rustc 报 `private_interfaces`。修法：类型改为 `pub`（字段也 `pub`），并在类型文档里写明为什么不能更私有。这不是放弃封装——该类型不是 wire 类型，只在本仓两处相邻调用之间传递。
2. **mock 形态 `never constructed`**：两个函数被 `not(feature = "mock")` cfg 掉，结构体就成了孤儿。修法：给 `PreparedJump` 加与函数完全相同的 `#[cfg]`。

**鉴别力验证（两个方向，第二个才是 I-37 的目标）**：

| 方向 | 做法 | 结果 |
|---|---|---|
| 反向（应失败） | 把 `enter_kernel_image_v1` 改回旧形状 `let (entry, arena) = prepare_...(..); jump_kernel_image_v1(entry, arena)` | **编译失败**：`E0308`（元组无法匹配 `PreparedJump`）+ `E0061`（`jump_kernel_image_v1` 只收 1 个参数，给了 2 个）——旧的位置传参形状已被类型系统拒绝，这正是 I-37 要拦的错序类 |
| 正向（应通过，但实际也通过） | 故意把字段值写反：`entry: VirBytes(arena_pa), arena_pa: PhysBytes(entry.0)` | **编译通过**。这不是缺陷：`VirBytes`/`PhysBytes` 都是 `u64` 的 newtype，防的是「位置错序」（被动犯错），不防「显式写错字段值」（主动犯错）。D-074 的判决行对 `BootScalars` 记的是同一结论（「结构体字面量可防错序不可防错值」），那一项由等式 `debug_assert` 补；`PreparedJump` 没有可供断言的等式关系（`entry` 与 `arena_pa` 本就无关），所以没有对应的补丁，也不该有 |

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -n -F -- "-> (u64, u64)" os/kernel/src/lib.rs` | `prepare_kernel_image_v1` 处**已归零**（另两处命中是类型文档的说明句与无关函数 `require_boot_region`） |
| A2 | `grep -n "fn jump_kernel_image_v1"` | `pub fn jump_kernel_image_v1(jump: PreparedJump) -> !` |
| A3 | `grep -c "core::hint::black_box(report.kern_virt_base.0)"` | **1**（保留） |
| A4 | `cargo test -p minix-kernel --features mock` | **866 passed / 0 failed / 8 ignored** |
| A5 | `cargo check -p minix-kernel` 三架构（x86_64/aarch64/riscv64 none 目标） | **0 error**，且 `PreparedJump` 相关警告 **0** |
| A6 | x86 UEFI 门 `test-shim-bootmarks-x86.sh` | **PASS** |
| A7 | aarch64 UEFI 门 `test-shim-bootmarks-aarch64.sh` | **PASS** |
| A8 | aarch64 直核 SMP 门 `test-smp-aps-aarch64.sh` | **PASS**（AP 进入调度器，SGI+定时器门打开） |
| A9 | riscv U-Boot shim 门 `test-riscv64-uboot-shim.sh` | **PASS**（6/6 标记，进入调度器） |
| A10 | riscv 直核门 `test-kernel-image-riscv64-bootface.sh` | **PASS** |
| A11 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`；structure/style(error=0) 通过；`doc-code-map` 54/54 |
| A12 | 引文逐字核对 | `PreparedJump` 定义段 5 行、签名/调用/解构 10 处全部与源码逐字一致（Python 逐行比对，非目视） |

**为什么此前不做**：用户于 2026 年 11 月 20 日明确只做记账；本条在裁决后由用户单独指示实施。

### I-36（已裁决·待裁决队列已清）`FileLoader` 的 ELF 修剪上移（+ 一次核查事故记录）

**来源**：`R35/DESIGN-D2-05`（P2，[MODERN]）。**状态：未裁决**（本条待用户裁决，见文末三选项）。

**核查事故（必须记录在此，防复发）**：2026-11-20 主 agent 在讨论 B2 残项时曾向用户报告「D2-05 已实施（`uboot_main.rs` 已无 `segment_iter`），待回勾销账」。**该判断是错的**：

- `UbootFileLoader` 定义在 `os/boot-shim/src/opensbi_helpers.rs:198`，**不在** `uboot_main.rs`；
- 其 `read` 在 `:211-216` 仍在跑 `minix_elf::segment_iter` 做修剪；
- `uboot_main.rs:267-320` 的 `staged_elf_used_len` 是**生产者侧的第二个修剪实现**（在 `build_shim_table` 里算槽长），不是"修剪被上移"的证据。

**方法论教训**：grep 一个符号时若命中数为 0，必须同时确认该符号是否**定义在别的文件**，不能仅凭单文件零命中就判定"已删除"。这与 D-067 讨论中"我误把 `0xffff7fffbfe00000` 当栈地址"是同一类错误：**单点证据 + 未交叉验证**。

**现状（为什么这是个问题）**：

| 位置 | 行为 |
|---|---|
| `opensbi_helpers.rs:198-224` 的 `UbootFileLoader::read` | 对每个读到的文件跑 `segment_iter`，长度修剪到最后一个 PT_LOAD 的文件尾；非 ELF 回退整个暂存槽 |
| `uefi_helpers.rs:1247-1259` 的 `UefiFileLoader::read` | 返回固件给出的**确切文件长度**，不修剪 |
| `loader.rs:465-468` | `BootModule.len = data.len()`——即**直接采用 `read` 返回的长度** |

后果：同一个模块文件在两条腿产生**不同的 `BootModule.len`**（U-Boot 裁到装载视图、UEFI 保留完整长度），而这个长度会进入：
- 恒等窗口（`push_module_windows`）
- VM 扣除记录（`classify` 的模块行）
- 模块 blob 的边界

**当前 kernel 与 12 个模块都是 ELF**，所以没有现行故障；但接口契约比文档承诺的窄。

**文档的两处矛盾（都已核实）**：
- `01-boot-shim-bootstrap.md:1502` 写「两个实现都在 `read` 里**只处理字节来源**」——对 U-Boot 腿为假；
- `:1518` 写「两条 `read` 的差别在字节来源」——同一句也是假的（差别还包含 ELF 语义）。

**裁决（D-082，用户 2026-11-20）：采「删重复 + 修剪上移」，不引入新 trait。**

**裁决时核实出的决定性事实（推翻了本条登记时的两条判断）**：

| 登记时的说法 | 实测 |
|---|---|
| 「表存的是槽容量，不是文件长度」（`opensbi_helpers.rs:206-209` 的注释）| **注释是错的**。`uboot_main.rs:build_shim_table` 写表时用 `let used = staged_elf_used_len(addr, slot).unwrap_or(slot)`，**写进去的已经是 `max(p_offset + p_filesz)` 的修剪值**（kernel 槽 `:build_shim_table` 内、模块槽同段）|
| 「`staged_elf_used_len` 与 `read` 的修剪是同一逻辑的两份实现」| `read` 的 `segment_iter` 修剪的是**已经修剪过的数据**——**纯重复工作**，不是"第二份实现" |

**因此 `UbootFileLoader::read` 的 `segment_iter` 是冗余的**：输入的 `entry.len` 已是修剪值，再跑一遍 `segment_iter` 是幂等的无效功。

**采纳的三处改动（零新类型、零新 trait）**：

| # | 改动 | 效果 |
|---|---|---|
| ① | 删 `opensbi_helpers.rs:211-216` 的 `segment_iter` 段；`read` 改为返回 `&slice[..entry.len as usize]` | `read` 恢复纯字节来源；同时删掉那段错误的「表存的是槽容量」注释 |
| ② | `loader.rs` 的 `load_boot_modules_into` 加一行修剪：`let used = elf_file_end(data.as_slice()).unwrap_or(data.len());`，写进 `BootModule.len` | 修剪移到唯一真正需要 ELF 语义的层；U-Boot 腿幂等（输入已是修剪值），UEFI 腿从此开始修剪 |
| ③ | `elf_file_end` 直接用现成的 `minix_elf::segment_iter`（`loader.rs` 已依赖 `minix-elf`，见 `:175-195` 的 `compute_kernel_layout`）| **不新增第三个手写解析器** |

**`staged_elf_used_len` 保持原样不动**：它受裸地址输入限制（读 `pa + off` 而非 `&[u8]`），无法复用 `segment_iter`；且它做的事（`fatload` 之后、拷进模块区之前算长度）在架构上必要。

**否决「抽象一个修剪 trait + 不需要的给空实现」**：核实后确认**没有哪个腿「不需要修剪」**——上移之后两条腿都走同一条路，U-Boot 腿只是输入恰好已是修剪值（幂等）。空实现无处可放；且 `FileLoader` 只有一个生产实现（UEFI）+ 一个（U-Boot）+ 一个测试替身，构造点不足以支撑新 trait（V3.6 C-33）。

**否决旧候选 B（只改文档）/ C（UEFI 侧补齐修剪）**：B 保留抽象泄漏；C 只是让两边一致地泄漏，仍把 ELF 语义留在 `read` 里。

**② 的前置验证（必须做，否则不能落 ②）**：
`load_boot_modules_into` 开始给 UEFI 腿修剪后，`BootModule.len` 会变小。必须确认 **12 个模块的 ELF 里最后一个 PT_LOAD 之后没有需要保留的数据**（签名段 / `.comment` / 调试段）。若将来某个模块加了签名段，② 会**静默裁掉它**。验证方式：对 12 个模块逐个 `readelf -l` 确认 `max(p_offset+p_filesz)` 到文件尾之间无非 PT_LOAD 内容。

**验收**：
- `grep -c "segment_iter" os/boot-shim/src/opensbi_helpers.rs` → 0（① 之后）；
- `grep -n "fn elf_file_end" os/boot-shim/src/loader.rs` → 命中 1（③）；
- 五条 QEMU 门链全 PASS（本改动触及所有腿的模块装载路径，门链不可省）；
- `cargo test -p boot-shim --features test-all` 全绿。

**风险与回退**：U-Boot 腿行为不变（幂等）；UEFI 腿 `BootModule.len` 变小——若某个模块的 PT_LOAD 未覆盖真实内容，该模块会被部分裁掉，表现为运行期缺数据。回退方式：删 ② 的一行即可恢复原状。

### I-45（已裁决·待实施）`SegmentIter` 把畸形 ELF 静默当成"没有这个段"

**来源**：`R3/DESIGN-D2-02`（P1，[MODERN]），D02 的 R3 面板。**状态：已裁决**（`D-084`）。

**问题**：`SegmentIter::next`（`os/libs/minix-elf/src/lib.rs:352-399`）有**六条 `continue`**，只有一条是正常过滤，其余五条都是"这个 ELF 坏了"：

| 行 | 分支 | 真实含义 |
|---|---|---|
| `:359-361` | `index.checked_mul(e_phentsize)` 溢出 | **畸形** |
| `:363-366` | `e_phoff + entry_off` 溢出 | **畸形** |
| `:373-376` | `parse_phdr` 返回 `Err` | **畸形** |
| `:381-383` | `p_offset + p_filesz` 溢出 | **畸形** |
| `:385-387` | `seg_end > image.len()` | **畸形**（截断或损坏）|
| `:379` | `p_type != PT_LOAD` | **正常过滤** |

**五条畸形与一条过滤，全都表现为 `next()` 返回 `None`**——调用方看到的是"段比预期少"，无法区分"这个 ELF 本来没有这个段"和"这个段坏了"。

**后果（四个消费者逐一核实）**：

| 消费者 | 位置 | 当前行为 |
|---|---|---|
| 引导 `compute_kernel_layout` | `loader.rs:176-215` | 只挡 `!found_load`（一段都没有）；**中间段坏被跳过 → `kern_size` 偏小、少拷一段、内核照常启动** |
| `load_segments_into_buffer` | `loader.rs:224-240` | `Err(_) => return 0`；**返回 `usize`，调用方无法区分"装载 0 段"与"ELF 坏了"** |
| `load_segments_into_phys_memory` | `loader.rs:286-291` | `.collect()` 成 `Vec<LoadSegment>`——**若 `Item` 变成 `Result`，`Vec<Result<..>>` 会静默保留 `Err` 元素**，必须显式处理 |
| rs exec | `rs/trap_api.rs:443-455` | 只挡 `!copied_any`；**少拷一段代码/数据后继续 exec** |
| VM bootstrap | `vm_server.rs:890-893` | `.collect()` 成偏小的段表，照它建映射 |
| `arch_boot` 的 `load_elf_into` | `arch/boot.rs:461` | 只 `map_err`，不检查段数 |
| `opensbi_helpers` 的槽长修剪 | `opensbi_helpers.rs:211-216` | `.ok().and_then(max)`，畸形时回退整个槽 |

**另有一处只有引导路径检查的危险条件**：`filesz > memsz`（`loader.rs:188-191` 有检查，其余六处**都没有**）。后果不是"少段"而是**越界写**——`sys_datacopy(seg.filesz)` 会写到 `vaddr + filesz`，超过声明的 `memsz`。

**C 侧对照**：C 把畸形镜像的拒绝交给**外部装载器**；Rust 侧已建了 `ElfError` 枚举（`TooShort` / `BadMagic` / `PhdrOutOfBounds` / `NoLoadSegments` …），但它只覆盖"打开文件"阶段，**遍历阶段的畸形没有错误路径**。

**裁决（D-084，用户 2026-11-20）：梭哈 A——`Item` 直接改 `Result`，不走 B 过渡。**

**否决 B（新增 `try_segment_iter` + 旧函数标 `deprecated`）**，三条理由：

1. **没有过渡窗口**：5 个依赖 crate 全在本仓、7 个生产调用点可同批改完，不存在"暂时改不完"的场景。
2. **B 会让缺陷继续存活**：只要 `segment_iter` 还在就有人用它（尤其 `opensbi_helpers.rs:211` 那种"我只用 `max()`，无所谓"的用法）。保留一个已知有缺陷的公开函数等于给缺陷留后门。
3. **A 才是本条的核心价值**：`Item` 改成 `Result` 之后，**每个调用点必须显式回答"遇到畸形 ELF 怎么办"**。当前 7 个点的答案是沉默；A 之后每个点都要写出自己的策略（boot 冒泡 `Err` / exec 转 `ENOEXEC` / VM 拒绝 boot image / 槽长修剪显式回退）。B 做不到这件事——用旧函数的路径可以继续沉默。

**A 的形态**：

```rust
impl<'a> Iterator for SegmentIter<'a> {
    type Item = Result<LoadSegment, ElfError>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.ehdr.e_phnum {
            // 非 PT_LOAD → continue（正常过滤，不进结果集）
            // 畸形 → return Some(Err(...))
            // 正常 → return Some(Ok(seg))
        }
        None
    }
}
```

**外加把 `filesz > memsz` 检查收进迭代器**（不留在调用方），让七个消费者免费获得越界写防护。

**同批必须改的下游签名**：`load_segments_into_buffer`（`loader.rs:224`）现返回 `usize`，`Err(_) => return 0`。改成 `Result` 后它**自己的签名也要跟着改**（`Result<usize, ElfError>`），否则它会把"坏段"再次压成"0 段"——**这是同一条缺陷在下一层的复制，只改迭代器等于把缺陷往下推一层**。

**`load_segments_into_phys_memory`（`loader.rs:286`）的 `.collect()` 必须显式处理 `Err` 元素**（当前 `Vec<LoadSegment>`；改成 `Item = Result` 后会变成 `Vec<Result<..>>`，若不处理就会静默保留坏段）。

**验收**：

1. 面板核心测试：构造一个「中间一个 PT_LOAD 的 `p_offset + p_filesz` 越界」的 ELF，断言 **boot / exec / VM 三个消费者都拒绝**而不是少段启动。
2. 补一条 `filesz > memsz` 的构造 ELF，断言三个消费者都拒绝（当前只有 boot 检查）。
3. `minix-elf` 自身单测：畸形 phdr 时返回 `Some(Err(..))` 而不是提前 `None` 结束。
4. `grep -rn "minix_elf::segment_iter(" os/ --include=*.rs` → 每处都已处理 `Result`；`loader.rs:288` 的 `.collect()` 已改为显式 `Result` 收集。
5. `cargo test -p minix-elf`（23 条基础 + 新增）、`cargo check --workspace --all-targets`、五条 QEMU 门链全 PASS。

**风险与回退**：`minix-elf` 的公开 API 破坏性变更，但无外部消费者。回退方式：`Item` 改回 `LoadSegment` 并恢复五条 `continue`（不推荐——那会回到本缺陷）。

### I-46（待裁决·**有前置依赖 I-45，前置未落地前禁止裁决**）`load_segments_into_phys_memory` 的 Safety 前提无类型载体 + UEFI 腿绕过 wrapper

**来源**：`R3/DESIGN-D2-03`（P2，[MODERN]），D02 的 R3 面板。**状态：待裁决，但被 I-45（D-084）硬性阻塞。**

> **⛔ 前置依赖（原文规定）**：本条的实质风险（"函数不自查段表、依赖调用者先 `compute_kernel_layout`"）会**被 I-45 顺手消掉**——I-45 落地后 `load_segments_into_phys_memory` 内部的 `segment_iter(...)?` 会用 `Result` 自查，`filesz > memsz` 与所有畸形段都不再依赖调用者。**因此必须在 I-45 落地并验证之后才能裁决本条**；在此之前任何裁决都是对过期事实的判断。

**问题（按 I-45 落地前的现状描述）**：

`loader.rs:286` 的 `pub unsafe fn load_segments_into_phys_memory` 的 SAFETY 契约（`:275-279`）要求两件事：

1. 目标物理地址已映射、可写、不与 boot-shim / 固件结构重叠；
2. **调用者必须先跑 `compute_kernel_layout`（或等价校验）**。

第 2 条它自己承认做不到（`:276-279` 原文）：

> this function itself does not reject `filesz > memsz` or address arithmetic overflow

**唯一的 `filesz > memsz` 拒绝在 `compute_kernel_layout`（`loader.rs:188-191`）**。没有它，`filesz > memsz` 会让拷贝长度大于声明区间，把字节写到 `paddr + memsz` 之外。

**调用面实测（三条路径）**：

| 调用点 | 走 wrapper？ | 说明 |
|---|---|---|
| `loader.rs:406` | —— | wrapper 内部直调 |
| `opensbi_helpers.rs:362` | **是**（`load_kernel_with_loader_gated`）| gate 里做 `reserve_kernel_span` / LMA 上界断言 / bump 池不相交断言 |
| `uefi_helpers.rs:1030` | **否**（直调）| 自己在 `:1000` compute、`:1004-1007` 断言 entry、`:1023-1024` reserve+record、`:1027` alloc_root、`:1028` alloc_bump、`:1029` init_pt_alloc，然后 `:1030` copy |

**UEFI 腿的顺序靠书写纪律维持**：它凑巧是对的（compute 在 copy 之前 30 行），但无机制保证将来重排仍对。

**为什么 UEFI 腿用不了 wrapper（这是面板没讲透的关键）**：

`load_kernel_with_loader_gated` 的 gate 签名是 `G: FnOnce(&KernelLoadResult)`——**只读回调，不能返回东西**。而 UEFI 腿在 compute 之后、copy 之前**还要做三步分配**（`alloc_root_page` / `alloc_bump_region` / `init_boot_pt_alloc`），这三步发生在 gate 之后。当前 wrapper 的形状表达不了"gate 之后还能继续分配再 copy"。`opensbi_helpers` 能用是因为它的 gate 只做区间校验、不分配。

**面板建议的两个修法**（我记录但暂不采纳，理由见"裁决时的待办"）：

| 选项 | 做法 | 我的评估 |
|---|---|---|
| 面板首选 | `compute_kernel_layout` 返回不透明 `ValidatedKernelImage<'a>`，`load_segments_into_phys_memory` 只接受它 | **收益会被 I-45 消掉**——该类型只能证明"layout 算过"，而 I-45 之后函数自查；且它证明不了 SAFETY 第 1 条（物理地址已映射），`unsafe` 仍要保留 |
| 面板备选 | UEFI 腿改走 wrapper（需把 gate 改成可返回值的形状）| **为一个调用点改通用 API 形状**，且 I-45 之后 wrapper 的 gate 对 UEFI 腿已无必要 |

**文档修改项（本条的未来动作，待裁决后执行）**：

面板称「文档 §4.2 第 870 行说『生产路径由 `load_kernel_with_loader` 保证这个顺序』，与源码不符」。**裁决前实测：该句在 `01-boot-shim-bootstrap.md` 中已不存在**——全文 `grep -c "load_kernel_with_loader"` 仅 1 处命中（`:3166` 的 M16 覆盖行），且不涉及"生产路径保证顺序"的声称。**因此这一项需在裁决时重新定位**：若确认无可改之处则销账，若发现同类失真表述（例如把 UEFI 腿的 `reserve_kernel_span` 说成"由 wrapper 保证"）则按实际位置改写。

**第二份拷贝（同一断言的两次实现）**：

`loader.rs:396-401`（wrapper 内）与 `uefi_helpers.rs:1004-1007` 是**同一个 entry 断言的两份实现**：

```rust
assert_eq!(layout.entry_point, layout.kern_virt_base.0,
    "boot-shim: ELF entry does not equal the first PT_LOAD VMA");
```

属 D-067/I-34 想消灭的"两套可不一致事实"。**但该断言很小，两份拷贝的风险低于改 wrapper 形状的成本**——是否合并留待裁决时权衡。

**前置依赖落地后（I-45 完成）本条应只剩三件事**：

| # | 待办 | 是否仍成立 |
|---|---|---|
| ① | SAFETY 契约删掉第 2 条（"必须先 compute"），改为"本函数自查段表，调用者只需保证目标物理地址已映射且不重叠" | **必做**（I-45 之后第 2 条自动失效，留着是错误文档）|
| ② | 文档失真表述核查与修正（须重新定位，见上） | **待核实** |
| ③ | UEFI 腿是否改走 wrapper / 是否合并 entry 断言第二份拷贝 | **待权衡，倾向不做** |

**禁止裁决的原因**：若在 I-45 之前裁决，会基于"函数不自查"这个即将消失的事实去设计类型（`ValidatedKernelImage`），落地后发现多余。

**解锁条件**：I-45（D-084）实施完成并验证（`cargo test -p minix-elf` + 五条 QEMU 门链 PASS）→ 本条解除阻塞，重新裁决。

### I-47（待裁决·**有前置依赖 I-45，前置未落地前禁止裁决**）`load_segments_into_buffer` 的计数与语义三处不一致

**来源**：`R3/DESIGN-D2-04`（P2，[MODERN]），D02 的 R3 面板。**状态：待裁决，由 I-45（D-084）阻塞。**

> **⛔ 前置依赖（用户裁定）**：本条的改动面与 I-45 在**同一个函数**上重叠——I-45 把 `segment_iter` 的 `Item` 改成 `Result` 后，本函数的 `match minix_elf::segment_iter(elf_data) { Ok(i) => i, Err(_) => return 0 }`（`loader.rs:225-228`）**必须重写**。因此裁决与实施都排在 I-45 之后；在此之前裁决会基于"迭代器返回 `None`"这个即将消失的行为去设计返回值。

**问题（三处互相矛盾）**：

`loader.rs:224` 的 `pub fn load_segments_into_buffer(...) -> usize`：

| # | 位置 | 事实 |
|---|---|---|
| ① | `:223` 文档 | 写「Returns the number of segments loaded」 |
| ② | `:258` | `count += 1` **无条件执行**——`:246-248` 的 `if file_end <= buf.len()` 只决定"要不要拷"，装不下的段**仍被计入** |
| ③ | `:235` | `seg.paddr < buf_base_paddr` 的段 `continue`，**在 count 之前，不计入** |

**所以返回值既不是"看到的段数"也不是"装载成功的段数"**——它是第三种东西，没有任何名字。

**两版不是同一个循环的两个后端（核实）**：

| 校验项 | 缓冲版（`:224`，测试）| 物理版（`:286`，生产）|
|---|---|---|
| 段间重叠检测 | **没有** | **有**（`first_overlapping_pair`，循环之前对整组段）|
| 目标长度上界 | **有**（`file_end <= buf.len()`）| 无（物理地址不需要）|
| `filesz > memsz` | 无 | 无（依赖 `compute_kernel_layout`，见 I-46）|

**调用面实测**：`load_segments_into_buffer` **只被测试调用**——`mod tests` 起于 `:490`，唯一调用点 `:786`（测"文件字节拷到正确位置"+"BSS 被清零"）。

**为什么这个孪生存在是有理由的**：物理版写物理地址，宿主上不可构造；缓冲版是唯一能在宿主验证"段拷贝 + BSS 清零"语义的东西。**但它的校验行为与生产版不同，导致"测试通过"不能证明"生产路径通过"。**

**面板建议的修法（`copy_segment(seg, sink)` 抽象）我不采纳**，理由：物理版的段间重叠检测在**循环之前对整组段**做（`first_overlapping_pair`），不在段拷贝内部；抽象到 `copy_segment` 之后那个"整组检查"仍要在外面做——**抽象只覆盖拷贝动作本身，覆盖不了真正的差异，还会让孪生"看起来更等价"。**

**候选修法（A，待 I-45 落地后裁决）**：

| # | 改动 | 理由 |
|---|---|---|
| ① | 返回值改名 `segments_copied`，且**只在 copy 与 zero-fill 都完成后** `+= 1`；越界段不计入 | 让名字、文档、行为三者一致 |
| ② | **补上段间重叠检测**（复用 `first_overlapping_pair`）| 让孪生真的映射生产语义 |
| ③ | 文档改为"测试投影"：说明两版的差异只有 sink（物理版写 `paddr`，本版写 `buf`），并列出缓冲版**额外**有的长度上界 | 消除"测试通过 = 生产通过"的错觉 |

**待裁决时还要定的**（面板给的验收）：补一个「`buf` 只能容纳第一段、第二段越界」的测试，断言返回**不是** 2。I-45 落地后这个测试的形态会变成"断言返回 `Err` 而不是 `Ok(2)`"。

**禁止裁决的原因**：I-45 会改变本函数对"整个 ELF 打不开"的表达方式（`Err(_) => return 0` → 必须重写），此时若先裁决计数语义，落地的返回值类型可能与 I-45 之后的协议冲突。

**解锁条件**：I-45（D-084）实施完成并验证 → 本条解除阻塞，按 A 裁决。

### I-34（已裁决·可立即开工）`BootStorage` 半提交防护：删 `map_dirty`，改用 `fill_*` 闭包 + `MapCounts`

**裁决**：用户 2026-11-20，判决行 `D-067`（`rewrite-notes/coordination/review-eval/DECISIONS.md`）。**状态：已批准待实施**——`OPEN-DEBT.md` 的 `BOOTSTORAGE-fill-api` 行同步。来源：`R35/DESIGN-D2-02`（P2，[MODERN]）。

**现状（为什么这是个问题）**：`BootStorage` 存七类载荷，每族都是「槽数组 + 一个 `usize` 长度」两个独立事实。唯一的一致性防护是 `map_dirty`，它只覆盖内存图两族；其余五族（modules / platform / param / sources / startup）没有任何拦截。文档 `01-boot-shim-bootstrap.md:1767` 却写「报告永远看不到未写槽」，同一段 `:1769` 又如实写「其余五族……没有 `map_dirty` 级的 report 拦截」——两句互相拆台。

**关键纠正（讨论中查实，推翻了面板的最小修复）**：面板建议「每族补一个 dirty 位」，但那个方案**拦不住它自己的反例**。反例是：

```rust
platform_slots_mut()[0] = 真源;   // 只写第 0 条
set_platform_len(4);              // 容量 4，断言全过 → dirty 被清、committed 置位
```

按 A 的规格（访问器置位、`set_*_len` 清位、`report` 断言全假），第 2 步就把 dirty 清掉了，`report()` 照样放行。**现有 `map_dirty` 同理**：写 128 条后 `set_memmap_len(128)` 也会清位。结论：dirty 机制防的是「忘了提交」，而真实故障是「提交了错的数」。

后果链（不是内存损坏，是误导性故障）：3 条 `ZERO_SOURCE = PlatformDescSource::new(DTB, PhysBytes(0))`（`boot_storage.rs:54`，**占位不是全零**）被当真实源 → 内核 `push_platform_windows`（`kernel/src/lib.rs:1123-1141`）对 `has_len()==false` 的源调 `push_2m(0)` → 为物理地址 0 建 2 MiB 恒等窗口 → 解析地址 0 的「DTB」失败 → 停机，但报错指向 DTB 解析而非真实原因（与 D-061 的 `x0==0` 判别同一类「症状误导」）。

**为什么删 `map_dirty` 是安全的**：borrow checker 已经免费禁掉了「边写边读」。`memmap_slots_mut(&mut self) -> &mut [MemoryRegion]` 借用期间，`report(&self)` 无法编译（E0502）。它剩的窗口只有「写手作用域结束、commit 未调用的那一小段」，而实际代码里 `map_tables_mut()` 之后紧接 `commit_map_tables()`（`uefi_helpers.rs:1117` → `:1184`），窗口为零。生产路径上 `map_dirty` 从未触发过——它是纯保险，而它保的险它又防不住。

**做法（E：`fill_*` 闭包）**：

1. 每族加一个 `fill_*` 方法，闭包拿到槽数组、返回实际写入量、方法内部当场提交：

```rust
// boot_storage.rs —— 形状示意
pub fn fill_platform<F>(&mut self, write: F)
where F: FnOnce(&mut [PlatformDescSource]) -> usize,
{
    assert!(!self.platform_committed, "platform already committed");
    let written = write(&mut self.platform_sources);
    assert!(written <= WIRE_PLATFORM_CAP, "over capacity");
    self.platform_len = written;
    self.platform_committed = true;
}
```

两个直核腿的 `platform_slots_mut()[0] = …; set_platform_len(1)` 变成 `fill_platform(|s| { s[0] = …; 1 })`；UEFI/U-Boot 两条腿的 `find_platform_sources_into(storage.platform_slots_mut())` + `set_platform_len(n)` 变成 `fill_platform(|s| find_platform_sources_into(s))`。闭包内可加断言。

2. 删 `map_dirty`（`boot_storage.rs:84` 字段、`:111` 初值、`:137`/`:144`/`:154`/`:169`/`:174`/`:189` 六处赋值、`:362-365` 的 `report()` 断言），并删 `memmap_slots_mut`/`reserved_slots_mut`/`set_memmap_len`/`set_reserved_len` 四个「单表」入口（生产必须用 `fill_map_tables`，单表形态已无存在理由）。

3. 内存图两族用一次提交两表的 `fill_map_tables<F>(&mut self, write: F) where F: FnOnce(&mut [MemoryRegion], &mut [MemoryRegion]) -> MapCounts`，**替代** `map_tables_mut` + `commit_map_tables`。

4. 新增值对象（放 `boot_storage.rs`，紧跟 `BootStorage`）：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCounts { pub memmap: usize, pub reserved: usize }
```

`commit_map_tables(memmap_len: usize, reserved_len: usize)` 改收 `MapCounts`。两处生产调用点显式写字段名：

```rust
// uefi_helpers.rs:1184
storage.commit_map_tables(MapCounts { memmap: classification.conventional, reserved: classification.reserved });
// opensbi_helpers.rs:324
storage.commit_map_tables(MapCounts { memmap: owned.free().len(), reserved: owned.reserved().len() });
```

5. **`ParamStats` 不动**（pub 字段保留）。裁决理由：`stats.bytes = 1024` 这类改法是调用方主动改自己刚收到的返回值，肉眼可见，由评审覆盖；为它私有化字段 + getter 是用类型系统防一个一眼可见的手误，成本大于收益。

6. **文档同批**：§4.1 把 `:1767` 的「报告永远看不到未写槽」与 `:1769` 的「五族无拦截」合成一句准确表述——**写完即提交，故报告只看得到已提交的前缀**（不再声称某种"屏障"，因为不再存在两段式状态）。

**验收**：
- `grep -rn "map_dirty" os/ --include="*.rs"` → 0；
- `grep -rn "memmap_slots_mut\|reserved_slots_mut\|set_memmap_len\|set_reserved_len\|map_tables_mut" os/ --include="*.rs"` → 0（测试里的等价物按新 API 重写）；
- 五族的 `*_slots_mut` / `*_buf_mut` 全部消失，替换为 `fill_*`；
- `cargo test -p minix-boot`、`cargo test -p boot-shim --features test-all`、`cargo test -p minix-kernel --features mock` 全绿；
- 五条 QEMU 门链（x86 UEFI、aarch64 UEFI、aarch64 直核 SMP、riscv U-Boot shim、riscv 直核 bootface）PASS；
- 文档六门（anchor/citation/structure/ref/style/density）+ `doc-symbol-drift.sh --from-diff` 零命中。

**风险与回退**：`fill_*` 借用 `&mut self` 期间无法再取另一族的 `&mut`，因此**禁止「先写 A 族、再写 B 族、最后一起提交」的交错风格**。已核实当前五个生产调用点都是「写一族 → 提交一族」的严格顺序，不冲突；若将来出现合法的交错需求，回退改用 B 的计数令牌或恢复 dirty 机制，并重开 D-067（该条件已写进判决行的 `reopen_condition`）。

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

前置已满足：I-31 已落（startup 族删除，族集由五族定稿为四族 + 内存图两表）；`sources` 族的写入者数量不影响 `fill_*` 的 API 形状（owned 路径三条腿早就在跑），所以 I-33 的受阻没有传导到本条。

| 层 | 改动 |
|---|---|
| 新增 | `MapCounts { memmap: usize, reserved: usize }`（值对象，两表计数按字段名带回）；六族 `fill_*`：`fill_map_tables`（闭包收两表、返回 `MapCounts`）、`fill_modules`、`fill_platform`、`fill_param`（闭包收 `&mut [u8; PARAM_BUF_CAP]`）、`fill_memmap_sources` |
| 删除 | `map_dirty` 字段与全部七处赋值；`report()` 里的 dirty 断言；`memmap_slots_mut` / `reserved_slots_mut` / `map_tables_mut` / `commit_map_tables` / `set_memmap_len` / `set_reserved_len` / `module_slots_mut` / `set_modules_len` / `platform_slots_mut` / `set_platform_len` / `param_buf_mut` / `set_param_len` / `memmap_source_slots_mut` / `set_memmap_sources_len` |
| 生产调用点 | 25 处重写：`uefi_helpers` 的模块/平台/参数/内存图四族，`opensbi_helpers` 的四族（含把 `push_platform_source` 的 if-let 并入闭包），两个直核腿的模块/平台/参数三族 |

**一处需要设计判断的改造（mini review 记录）**：`uefi_helpers` 的 EBS 配对循环原本是「循环外 `map_tables_mut()` 取两表、循环反复调 `pair_and_classify`、循环后 `commit_map_tables`」。改成 `fill_map_tables` 后，**整个重试循环被搬进了闭包里**——因为闭包独占 `&mut self`，而循环内的 `NeedBiggerBuffer` 分支还要重新分配缓冲、重新进入。这是正确的：一次失败的 attempt 不应提交任何计数，只有成功的那次才带 `MapCounts` 出来，与「写与提交是一步」的语义一致。`classification` 通过闭包外的 `Option` 带出。

**mini review 报出的死子系统（登记为 I-42，未删）**：`fill_*` 落地后，`push_module` / `push_platform_source` / `BootStorageFull` 三者的调用者归零（`grep` 实测零命中；`BootStorageFull` 只被这两个 `push_*` 的返回类型引用）。D-067 的授权文本只点名「五族的 `*_slots_mut`/`*_buf_mut` 全部消失，替换为 `fill_*`」，没提 `push_*`；它们在 I-31 删掉 startup 后也只剩这两族。按 fix-guard「一次一条」登记为 I-42，不在本次改动里删。

**验收读数（本机实测，2026 年 11 月 20 日）**：

| 验收项 | 命令 | 读数 |
|---|---|---|
| A1 | `grep -rn "map_dirty" os/ --include=*.rs` | **0** |
| A2 | `grep -rn "memmap_slots_mut\|reserved_slots_mut\|set_memmap_len\|set_reserved_len\|map_tables_mut\|commit_map_tables"` | **0** |
| A3 | 全族 `*_slots_mut` / `*_buf_mut` / 对应 `set_*_len` | **0**（生产与测试均无残留） |
| A4 | `cargo test -p minix-boot` | **183 passed / 0 failed**（测试已按新 API 重写，条数不变） |
| A5 | `cargo test -p boot-shim --features test-all` | **136 + 6 + 2 passed** |
| A6 | `cargo test -p minix-kernel --features mock` | **864 passed / 8 ignored** |
| A7 | `cargo check --workspace` | **0 error** |
| A8 | `cargo check -p kernel-image` 四个变体 | 全部 **0 error** |
| A9 | **五条 QEMU 门链** | x86 UEFI **PASS**；aarch64 UEFI **PASS**；riscv U-Boot shim **PASS**（6/6）；aarch64 直核 SMP **PASS**；riscv 直核 bootface **PASS** |
| A10 | 文档门（01 篇） | anchor **108/108**；citation `blocks=69 checked=29 failed=0`；structure/style(error=0)/density(承载门通过) 通过 |

**为什么此前不做**：本条与 D-062（wire v10 删 `startup` 族）和 D-065（直核腿统一 owned 路径）改同一批行；族数与写入者数量在它们落地后才定稿。D-062 已落（I-31），D-065 虽受阻（I-33 待裁决）但不影响本条的 API 形状，故本条可在 I-31 之后独立实施。

### I-44（已裁决·待指派实施）页表页归还的所有权模型（原 `R35/DESIGN-D2-11`）

**来源**：`R35` 面板 `DESIGN-D2-11`（P3，`panel/round35/design-d2.md:167`），原结论为"拒绝（本期不动）"——理由是"改动会牵连测试内核的销毁路径，收益仅是防未来回归"。用户于 2026-11-20 要求重启，并以**十轮外部独立评审**收敛为完整的所有权模型。

**裁决**：`D-078`。**概念设计冻结通过；代码实施现在开始；生产安全验收有条件通过。**

**设计文档（唯一合同源，自包含、可交外部评审）**：
`rewrite-notes/01-stage-kernel/01-review-request-page-table-free.md`（第十版，2532 行，`tools/doc-style-lint.sh` 0 error）

**十轮评审的结论摘要**（细节与证据链见文档 §3、§4bis、§4ter、§4quater、§4quinquies、§7）：

| 议题 | 结论 |
|---|---|
| 问题 B（跨域归还）| **撤回**。`minix_arch::pt_alloc` 的 `static PT_ALLOC`/`PT_FREE` 在"内核映像"与"VM 映像"里各有一份；`os/kernel/src` 与 `os/kernel-image/src` 的 `register_free` **零命中** |
| 问题 C（共享子树被递归归还）| **成立**。`inherit_supervisor_half` 复制顶层 PTE 建共享子树；`free_child_tables` 遍历 0..512 无跳过；三架构同形。注释 "sound because user mappings never touch the upper half" 只证明**映射**语义，不证明**释放**语义 |
| 问题 D（根页所有权）| **成立，且比 C 更基础**。`destroy()` 末尾无条件 `free_pt_page(root)`，而 `new_from_page` 的**四种**真实根页来源（UEFI 分配服务 / boot-shim bump 游标 / `bootface_a64` 私有 `BUMP_PTR` / `VmBootAllocator`）**全都不在 `pt_alloc` 释放域** |
| `free_pt_page` 的 Acquire | 原判"无效"经外部评审自纠后**撤回**——Acquire 语义不依赖返回值被使用 |
| `destroy()` 无 free 钩子时 | **不是空操作，是清零根页**。对借用型 handle 等于当场抹掉活动页表，比跨域归还更直接 |
| `trait Paging`/`OwnedPaging` 拆分 | **方案被证伪**。Rust trait 实现关联于类型；`X86_64Paging` 只有 `root_paddr`/`channel` 两字段，四个构造函数都返回 `Self`，故任何借用型实例仍可调 `destroy` |
| 最终 API | 三层：`Paging`（操作）/ `PagingBackend`（架构机制，`pub(crate)`/sealed）/ 三种 handle（`OwnedPageTable`、`CallerManagedPageTable<'root, S, P>`、`BorrowedPageTable<'a, P>`）；类型状态 `PrivateSupervisor` / `BorrowedSupervisor<'source>`；两类凭证 `BorrowedRootMut<'a>`（改权）/ `SharedPageTableSource<'source>`（保活） |

**冻结范围**（第十版评审措辞，照录）：
> 冻结所有权模型、共享子树语义、三层 API 架构与安全合同；具体 Rust 方法签名、泛型边界与调用点迁移，**允许在实现中根据编译器和测试结果调整**。

**三项不再退回争论**：① 不再用单个 `RootKind` 枚举替代三种 handle；② 不再"一律跳过上半场"；③ 不再把 borrowed 当只读（四个活动根调用点全部会写页表）。

**实施分期**（文档 §7.8，A–D 可立即启动）：

| 阶段 | 工作 | 状态 |
|---|---|---|
| A | 三层 API 与 handle 骨架（含 `HugePages` 等扩展 trait 代理、`enable` 归位）| 可开始 |
| B | 构造、根页凭证、终结能力（borrowed 不可 destroy）| 可开始 |
| C | 共享状态与 backend 根层遍历（内部测试验证物理页归属）| 可开始（真实 bootstrap 接入依赖 R1）|
| D | 修改范围检查（全部公开修改入口，批量操作先整体检查）| 可开始 |
| E | 活动根与共享树同步（四个调用点各答：锁 / CPU 参与集合 / TLB 失效覆盖）| **D+E 合并验收** |
| F | bootstrap／TX 显式生命周期交接（`PrivateSupervisor` 与 `BorrowedSupervisor<'static>` 两条路径都闭合）| 有条件 |
| G | TX 专用保留区（容量实测、区间不重叠）| 真实 VM 接入前阻塞 |
| H | 未来 `retire_owned_subtrees` | 暂缓 |

**两项独立实施门**（不得被本次重构掩盖）：

- **R1 共享源结构稳定性**：`harden_kernel_text`（`lib.rs:1707`）与 heap-window eviction（`lib.rs:2928`）都修改活动根，**是否会在继承者存活期间替换或释放共享子树，现有材料未证明**。关闭条件：四个问题逐一核实（写入的是共享子树的叶 PTE 还是源根顶层入口 / 是否清除仍被继承者直接引用的入口 / 释放的物理页是否仍在继承者可达集合内 / TLB 失效覆盖范围）。
- **R2 TX 与 VM free-list 快照重叠**：`test-sysboot:242` 读的是 `:203` 由 `init_proc_and_boot` 发布的快照（write-once），`:254` 之后 TX 从该快照消耗的帧**从未 cut 出去**。当前未触发是因为该 test kernel 的 VM 槽跑桩镜像 `sysboot-rx`（不消费 free list）；真实 VM 服务接入前必须修。首选方案：`Boot reserved region` 拆 `PT allocator range` + `TX image range`，但**容量数据缺失**（512 页 bump 区要同时装 PT allocator 高水位 + TX 镜像，未实测）。

**验收（实施阶段）**：文档 §5 的六项三架构测试清单（私有上半场释放 / 共享上半场不释放 / 根页归属 / 范围拒绝 / 批量操作原子性 / 源共享行为与陈旧映射判定）。

**状态**：**设计已通过，可以实施**（文档第十三版 §4ter 为实施基线）。

文档（第十三版，实施基线终版，1782 行）新增 §0「设计原则」并做了减法审查，外部评审逐项裁决，**四处自查全部确认为过度设计**：

| 自查项 | 裁决 | 最终处理 |
|---|---|---|
| `BorrowedRootMut` / `SharedPageTableSource` 双凭证 | **过度，应删** | 删除——零业务字段 + `PhantomData`，不建立真实借用关系 |
| `PrivateSupervisor` / `BorrowedSupervisor` 类型状态 | **过度，应删** | 改为内部运行期状态 + 状态转换 |
| `classify_range` + `PagingRangeClass` | **过度，应删** | 改为架构谓词 `is_supervisor_va` + 首尾两端点断言 |
| 三种 handle | **当前过度** | 保留单个具体 `Paging` 类型 + `destroy` 入口断言 |
| `PagingBackend` 层 | **过度，应删** | 没有 wrapper 要保护，不必单独成层 |

**最终方案（§4ter）**：一个私有 `PagingState` 三态（`Owned` / `External` / `Inherited`）+ 五条入口断言：

1. 构造时记状态（`new()` → `Owned`；`new_from_page()` / `from_active_root()` → `External`）
2. `destroy(self)` 消费型 + `assert_eq!(state, Owned)`，**在任何内存操作之前**
3. `inherit_supervisor_half` 只许 `External` 进入 + 检查目标半场为空 + 复制后才转 `Inherited`
4. `Inherited` 状态下所有写入口拒绝进入共享上半场（首尾两端点检查，批量操作改前先查）
5. 不公开任何能销毁 / 回收 `External` / `Inherited` 的接口

**关键减法**：共享子树回收**不实现跳过机制**，用前置约束解决——`Owned` 不继承、`inherit` 只许 `External`、`destroy` 只接受 `Owned`，所以 `free_child_tables` 只会作用于自有私有树，继续遍历全部顶层条目即可。

**实测比例**：七个操作、合计 **11** 个生产调用点；原方案要 **12** 个新命名项，现已全部废止。

**第十三版补的四处实施细节**（评审在"设计通过"后点出的，都是实施时会立刻撞上的）：

| 项 | 结论 |
|---|---|
| `adopt_active_root()` 迁移 | **合并进 `from_active_root(root, channel)`**，删除旧构造函数；VM 自映射调用点 → `External`。这样"三个构造函数"才准确 |
| `PagingState` 的 derive | **补 `Debug`**——伪代码用 `assert_eq!`，缺它会编译失败 |
| 目标半场"为空"判据 | **按槽位为零判**，不是只查 Present 位（某些页表格式允许非 Present 条目携带软件状态）|
| `inherit` 的 `# Safety` | **四条**：目标根≠源根 / 未被 CPU 激活 / 无未协调修改者 / 源树满足 R1 |
| 范围检查 | **只增不改**：canonical / 对齐 / `validate_virtual_range` 全部保留 |
| `PteChannel` 归属 | 三架构各自的**私有 enum**；跨架构统一须上升为 `Paging` 层，建议改语义化入口 |

**下一步**：按文档 §5.1 的 **5 步实施次序**推进——① 状态字段与构造函数迁移（含 `adopt_active_root` 合并与 `SimPaging`）→ ② `destroy(self)` 入口断言 → ③ 继承路径 → ④ 全部写入口范围保护 + 三架构测试 → ⑤ 独立关闭 E / R1 / R2。**不得把基础 API 编译通过当作生产安全验收通过。**

**实施记录（2026 年 11 月 20 日，步①②③完成——状态字段、构造函数迁移、destroy 断言、继承路径）**：

| 项 | 内容 |
|---|---|
| 共享层 | `PteChannel` 从三处私有 enum 上收为 `arch/src/arch/paging.rs` 的共享 `pub enum`（三处定义逐字相同）；trait 的 `adopt_active_root` 删除，`from_active_root(root_phys)` → `from_active_root(root_phys, channel)`；`destroy(&mut self)` → `destroy(self)` 消费型 |
| 三架构 | `X86_64Paging` / `AArch64Paging` / `Riscv64Paging` 各加私有 `PagingState`（Owned/External/Inherited）与 `state` 字段；构造函数按状态转移表设值（new→Owned，new_from_page / from_active_root → External）；`inherit_supervisor_half` 三断言（External-only → 逐槽空白校验【全 64 位槽值，非仅 Present 位】→ 复制后转 Inherited） |
| 内存模型 | `MockPaging`（arch）与 `SimPaging`（vm server）同语义迁移；SimPaging 的 `PagingState` 带 `#[default] Owned`（宿主 `#[derive(Default)]` 需要） |
| 消费型 destroy 的调用方适配 | `vmproc_handle.rs` / `vmproc.rs` 的 `assume_init_mut()` → `assume_init_read()`（从 MaybeUninit 读出拥有值，断言才能在拥有语义上运行） |
| 命门测试（§5.4 第 3 项） | MockPaging 三条（External destroy panic / new_from_page destroy panic / Owned 正控）；x86 一条「断言先于访存」证明——External 句柄的 root_paddr 指向**未映射**宿主内存，若断言挪到清零之后，测试将以 SIGSEGV 而非干净 panic 失败 |
| E 门调用点适配 | `init_kerninfo`（kerninfo.rs）、`harden_kernel_text`（lib.rs:1707）、heap-window eviction（lib.rs:2928）三处 `from_active_root` 补 `KernelDm`；VM 自映射（vm_self_map.rs）与 test-proc-init 走 `VmDm` |

**读数**：workspace 0 error；minix-arch(runtime-window) **292** passed（原 288 + 4 条状态测试 − 移除 1 条 SIGSEGV 正控）、minix-kernel mock 864、minix-boot 183、boot-shim 136+6+2、minix-vm **548** 全绿；三架构生产 target 各 0 error。

**两个实施发现（如实记录）**：
1. **x86 Owned 正控测试 SIGSEGV 已移除**：`destroy` 的 `free_pt_page(PhysBytes(0))` 在宿主测试进程里撞未注册/错配的释放钩子——正控由 MockPaging 覆盖（同语义），x86 侧保留命门测试即可。
2. **宿主 target/debug 残留 83 个 root 属主伪影**（docker 权威编译门以 root 跑的遗留），`cargo test -p minix-vm` 的依赖 `.d` 文件写入被拒——已清理，属环境问题非代码问题。

**步④（2026 年 11 月 20 日完成——全部写入口范围保护 + 三架构谓词 + 命门测试）**：

| 项 | 内容 |
|---|---|
| 共享谓词 | trait 新增 `is_supervisor_va(&self, va)`（三架构各自实现：x86 `≥0xFFFF_8000_0000_0000`、aarch64 `≥0xFFFF_0000_0000_0000`（T1SZ=16）、riscv `≥0xFFFF_FFC0_0000_0000`（kernel link base））与 `is_inherited(&self)`（读私有 state）；共享 guard `ensure_writable_range(start, len)`——首尾两端点检查 |
| 写入口覆盖 | `map` / `remap` / `unmap` / `update_flags` / `split_huge` / `grant_user_walk` / `map_huge`（HugePages，`size.bytes()`）每入口插 guard；批量 `map_range` / `unmap_range` 在**任何修改之前**做整段端点检查（trait 默认实现内联——保持对象安全） |
| 命门测试 | x86 三条：① Inherited `map` 高半 panic（root 指向未映射内存——若 guard 挪到写后即 SIGSEGV）；② Inherited 批量跨界（私有→高半）拒绝 + 原子性（guard 在 validation phase 之前，按构造无任何写入；断言 panic 消息来自 guard）；③ MockPaging 三条状态测试（前批已落） |

**读数**：minix-arch(runtime-window) **294** passed；workspace 0 error；三架构生产 target 各 0 error；minix-kernel mock 864。

**实施教训（如实记录）**：一个 Python 批处理脚本因循环变量覆盖把 riscv64 实现内容写进了共享 trait 文件（`os/arch/src/arch/paging.rs` 被整体覆盖为 riscv 实现）——已用 `git checkout HEAD` 恢复共享文件后以 Edit 工具重放三处步④改动。教训与 D-077 的「从注释推断代码」同类但相反面：**批量脚本里承载不同文件内容的变量必须每轮重新读取**，不能假设上一次的内容还在。

**未做（按 §5.1 边界）**：步⑤ E / R1 / R2 三个独立证据门。E 门四个调用点各答三问（锁/CPU 参与集/TLB 失效覆盖）需要启动顺序与 SMP 证据；R1 需核实 `harden_kernel_text` 与 heap-window eviction 是否会在继承者存活期替换/释放共享子树；R2 需 TX 专用保留区容量实测。三者都是证据责任，不是代码改造，**不得与步④混合验收**。

**步⑤ E 门 + R1 关闭（同日，证据落文）**：

**E 门：四个调用点各答三问**

| 调用点 | Q1 防并发修改的约束 | Q2 可能在使用该根的 CPU | Q3 TLB 失效覆盖 |
|---|---|---|---|
| `init_kerninfo`（kerninfo.rs:143） | kmain Phase B.5，BKL 持有、smp_init 之前——单 CPU（BSP） | 仅 BSP；无 AP 已释放 | 新 USER_VA 从未映射过，无任何 CPU 持有它的陈旧 TLB 项；无需失效 |
| `harden_kernel_text`（lib.rs:1974） | kmain 内、BKL 持有、smp_init 之前（源码注释原文「no AP has loaded the kernel root before its release」） | 仅 BSP；进程根已建但全部 PROC_STOP，从未装入任何 MMU | PTE 写路径的本地失效已覆盖；被改的是共享子树里的条目，但无 inheritor 有任何缓存翻译（从未 dispatched） |
| heap-window eviction（lib.rs:2928，init_proc_and_boot 内） | Phase C，BKL 持有、单 CPU、VM 尚未执行 | 仅 BSP；VM 进程已创建但 BOOTINHIBIT+PROC_STOP | `unmap` 内建本地失效覆盖 BSP；无其它 CPU |
| VM 自映射（vm_self_map.rs:102） | VM 单线程（SAFETY 注释原文）；`VM_SELF_PT_STORAGE` 单写者 | 仅 VM 自身（用户态单线程进程） | 句柄自带 flush_tlb 计数；单线程无跨 CPU 需求 |

**E 门结论**：四处的 Q1 都是「kmain 启动顺序 + BKL + smp_init 尚未运行」或「VM 单线程」；Q2 都是「除当前 CPU 外无其它 CPU 持有该根或共享子树的翻译」；Q3 都是「本地失效足够 / 新映射无需失效」。**锁解决并发修改、TLB 失效解决旧翻译——两合同各自成立。**

**R1：源根结构稳定性（四问逐一核实）**

`harden_kernel_text` 与 heap-window eviction 都修改 **bootstrap 根**的 supervisor 半（共享子树所在）。关键时序（生产 kmain，`lib.rs:1749`）：`init_proc_and_boot`（`:1897`，Phase C，建进程根并 `inherit_supervisor_half`）→ `harden_kernel_text`（`:1974`）→ `smp_init`。

| # | 问题 | 答案 |
|---|---|---|
| 1 | 写的是共享子树的叶 PTE 还是源根顶层入口？ | `split_huge` 在共享父表中把 2 MiB 叶替换为新 4 KiB 表指针（继承者走共享子树会看到新表）；`update_flags` 只写**新表内**的叶 PTE（该表 split 后才分配，split 前 inherit 者不存在→非共享）；heap eviction 的 `unmap` 写共享父表条目（清掉 boot-identity 遗留） |
| 2 | 是否清除仍被继承者直接引用的入口？ | 否——旧 2 MiB 叶的物理页是内核 text/data 本体，split 后由新 4 KiB 页继续映射（同一物理页集），无映射消失；heap eviction 清掉的 boot-identity 条目按设计就是「boot 期间专用」的遗留，Phase C 后无消费者 |
| 3 | 释放的物理页是否仍在继承者可达集合内？ | `split_huge` 与 heap eviction **均不释放**物理页（`free_child_tables` 的 PS-bit 跳过规则；unmap 只清条目不归还页）——无释放即无回收冲突 |
| 4 | TLB 失效覆盖继承者？ | 全部进程根处于 PROC_STOP + 从未装入任何 CPU → 继承者 TLB 为空；BSP 本地失效覆盖自己 |

**R1 结论**：两个源根修改路径都**不**在继承者存活期替换/释放继承者可达的页——安全性由 kmain 的启动顺序（修改发生在 Phase C 之后的窗口、smp_init 之前、全部继承者 stopped）与「不释放只重写」的形态共同保证。**R1 关闭**。

**R2 代码修复落地（2026 年 11 月 20 日，本批）**：采用**切除法**（替代原「拆保留区」首选方案——后者需要 512 页 bump 区容量实测前置，切除法按 C parity 直接把消费掉的 span 从发布列表中去掉，无容量前置）：

| 项 | 内容 |
|---|---|
| `vm_handoff::cut_consumed_span(start, end)` | 从 `VM_PMM_FREE` 原地裁掉被消费的 span（trim 头/尾、拆分跨界、整删全消费），count 收缩同步 Release 发布。单线程 boot 写入——写-一次合同更新为「分类发布 + 每 loader 一次 cut」 |
| `test-sysboot` 接线 | TX `load_process_elf` 之后，按分配器的 `current_region_idx()`/`cursor()` 算出全部已消费 span，逐个调 `cut_consumed_span`。下一个消费者（真实 VM 服务）看到的 free list 已不含 TX 用掉的帧 |

修复后 QEMU 验证被 I-43 二层阻塞（`test-sysboot` 新鲜二进制在 `enable()` 后静默停——与 test-proc-init / test-timer-irq / hello-boot 同型，stash 对照确认为既有漂移非本批引入），但代码按构造正确：cut 在 TX 消费完成后立即执行，下一个读者必然看到裁剪后的列表；容量前置消失（不需要预保留区——消费多少切多少）。

**R2 状态更新**：代码修复已落；**真机/端到端验收**仍待 I-43 二层关闭（需要能跑起来的新鲜载体）。

**边界重申**：E/R1 的证据是代码时序与启动顺序的静态核实；QEMU `-d int` 级动态取证（TLB 实际行为）留给首个真机/SMP 门。

**门②（I-29-16 三架构 WB 合同）落地（同日，本批）**：

| 腿 | 落地形态 | 位置 |
|---|---|---|
| x86 | **PAT 前提运行期校验**：`enable()` 读 IA32_PAT (MSR 0x277)，断言 PA0 = 0x6 (Write-Back)。PAT 是每-CPU MSR，enable 也是每-CPU 路径，覆盖面一致。固件改写 PA0 会让全部 RAM 叶子的 WB 承诺静默失效——现在当场停机并给出坐标。MTRR 默认覆盖不在此校验（QEMU virt 无变异条目；真机属证据责任） | `x86_64/paging.rs` 的 `enable()` |
| aarch64 | **AttrIndx 映射点合同声明**：`flags_to_pte` 是「叶子→缓存属性」的唯一映射点（RAM 叶子 AttrIndx=0 选 MAIR Attr0 Normal WBWA，仅 DEVICE 翻到 Attr1 Device-nGnRnE；MAIR 由 `enable` 写 0xFF）。曾尝试编译期 const 断言，被 bitflags 非 const 运算符挡住（E0015）——合同以函数文档声明，enforcement 即 DEVICE 分支本身 | `arm64/paging.rs` 的 `flags_to_pte` |
| riscv64 | **PMA 前提声明**（模块级文档块）：Sv39 PTE 无缓存属性位，WB 是平台 PMA 前提而非编码事实；QEMU virt DDR 在 PMA 声明为可缓存，memmap 全部落在该区间；本项目**不使能** Svpbmt（mencfg.PBMTE 保持 0，PTE[62:61]=0=PMA 模式），**Sv39 不存在「强制 WB」的 PBMT 取值**——WB 只能来自 PMA；真机换板需按该板 PMA 核对 DDR 缓存性 | `riscv64/paging.rs` 模块级文档块 |

读数：三架构生产 target 各 0 error。I-29-16 合同自此在三架构各自的最小可验收形态落地；政策表的「完全冻结」仍按设计文档等原型证据与真机验收（不因本批宣告）。

### I-42（已裁决·I-34 的实施副产物）`BootStorage` 的 `push_*` 死子系统：实际只剩 `BootStorageFull`

**来源**：I-34 实施时的 mini review。

**现状（裁决时实测，射程比条目登记时更小）**：`fill_*` 落地后登记了三个符号，核实发现前两个**已经不存在**：

| 符号 | 裁决时实测 |
|---|---|
| `push_module` | **已不存在**（定义与调用都无）|
| `push_platform_source` | **已不存在** |
| `BootStorageFull` | **仍存在**，两处：定义 `boot_storage.rs:291-293`（含文档注释 `/// The fixed list a push_* call targeted is full.`）、re-export `lib.rs:31` |

**所以本条的实际射程只有一件：删掉这个为已删函数而留的空结构体。**

它的文档注释仍在描述一个不存在的 `push_*`，与 D-077（空 getter 段）、D-079（ghost 注释）属同一类残骸，取向也一致：**删掉，不造新东西**。

**注意**：`kernel/src/lib.rs:1183` 的 `push_module_windows` 是**内核侧的同名不同物**（窗口表构造函数），与本条无关。

**为什么不是 I-34 的一部分**：D-067 的授权文本点名的是「五族的 `*_slots_mut`/`*_buf_mut` 全部消失，替换为 `fill_*`」，`push_*` 不在其中。I-34 实施后它们才变成死代码，属实施副产物。

**裁决（D-080，用户 2026-11-20）：采 A——删掉 `BootStorageFull`。**

具体改动两处：

| # | 位置 | 动作 |
|---|---|---|
| ① | `os/libs/minix-boot/src/boot_storage.rs:291-293` | 删文档注释 + `pub struct BootStorageFull;` |
| ② | `os/libs/minix-boot/src/lib.rs:31` | `pub use` 去掉 `BootStorageFull` |

`boot_storage.rs:288` 的 `impl Default for BootStorage` **不动**——它服务于 `BootStorage`，与本条无关。

**否决 B（保留）**：D-067 的裁决已把 `push_*` 整体换成 `fill_*` 闭包，方向已定；保留一个指向已废弃入口的错误类型，正是 D-067 想消灭的"两套可不一致事实"。

**一个必须写明的边界**：`BootStorageFull` 虽零消费者，但**已通过 `pub use` 暴露**，`minix-boot` 被 `boot-shim` / `kernel` / `kernel-image` 三方依赖——删一个 `pub` 符号属 API 表面变动。当前零消费者，但需 `cargo check --workspace --all-targets` 全绿才算证明。

**验收**：

```bash
grep -rn "BootStorageFull" os/ --include=*.rs        # 删除后应为 0
grep -rn "push_module_windows" os/kernel/src/lib.rs  # 应仍命中（同名不同物，不受影响）
cargo check --workspace --all-targets                # 必须 0 error
cargo test -p minix-boot                             # 含 boot_storage 自身的 fill_* 测试
```

**实施记录（2026 年 11 月 20 日完成，`✅ 已实施`）**：

| 项 | 动作 |
|---|---|
| ① | 删 `os/libs/minix-boot/src/boot_storage.rs` 的 `/// The fixed list a \`push_*\` call targeted is full.` 与 `pub struct BootStorageFull;`（含 `#[derive(...)]`） |
| ② | `os/libs/minix-boot/src/lib.rs:31` 的 `pub use boot_storage::{...}` 去掉 `BootStorageFull` |

验收：`grep -rn "BootStorageFull" os/ --include=*.rs` → **0**；`cargo check --workspace` → **0 error**；`cargo test -p minix-boot` 183 / `-p minix-kernel --features mock` 864 / `-p boot-shim --features test-all` 136+6 全绿。

### I-34（原依赖说明，保留作历史）本条与 D-062 / D-065 的关系

---

## 4. 测试缺口批（01 篇 §5.3 TODO-01~07，归拢时点复核）

| ID | 机制 | 断言 | 复核现状 | 去向 |
|---|---|---|---|---|
| TODO-01 | M4 深拷先于一切动用空闲清单者 | 三入口在首个页分配前完成 `store_kernel_info` | ✅ 已覆盖：四入口源码序钉（`store_before_alloc_pin.rs`）；运行期断言经 QEMU 测试内核直接调 `arch_boot_impl` 分阶段验证证明不成立，已撤（判据写在钉注释） | 已交接完成 |
| TODO-02 | M7 分配器写一次 | 二次 `register` debug 失败、release 拒改写 | ✅ 已覆盖：`pt_alloc_write_once.rs` 2 条（`register`/`register_free`，debug panic、release 拒改写且指针不变） | 已交接完成 |
| TODO-03 | M8 落地区容量 fail-fast | 超容量输入 fail-fast（不截断） | ✅ 已覆盖：三枚超容量 `should_panic`（reserved 257 / memmap 129 / modules 13，各持 boot-globals 锁） | 已交接完成 |
| TODO-04 | M12 EBS 阶段边界 | EBS 后路径不出现 `uefi::` 调用 | ✅ 已覆盖：`os/boot-shim/tests/ebs_boundary_audit.rs` 文本级审计（切函数体、去注释、扫 EBS 后语句；注入负控 + 注释豁免自检）；T1CUT 接线后已按最终形状升级为两条入口（`prepare_boot` 与 `prepare_and_enter_kernel_image`）各一套，5/0 | 已交接完成 |
| TODO-05 | §4.6 大页回退档 | 2MiB 回退档下真机启动 | 仍 open：当前环境 QEMU-only（无真机，窗口不可得），真机断言保持开放；软件路径的 QEMU 等价覆盖存在候选——能力压制腿（如 x86 `-cpu qemu64,-pse` 关 2MiB PSE，逐架构类推）可逼 `plan_dm_policy` 走回退档，尚未实施、未排期 | 真机窗口项（等硬件）；候选软路径腿另行立项 |
| TODO-06 | M14 x86_64 路标链 | `kernel loaded`/`12 boot modules loaded`/`handing control` 三路标 | ✅ 已覆盖：`test-cmd-smoke.sh` Stage 3b 逐条 `grep -qF`；本批实跑三行全在。脚本后续 du/stat/find 腿在主树 HEAD 亦失败，属既有问题不在本条 | 已交接完成 |
| TODO-07 | §4.2 UEFI 侧宿主可测面 | 两列表互斥完备、root/bump 不与 free 相交 | ✅ 纯函数已覆盖：`assert_bootstrap_outside_memmap` 3 条（相离/贴边通过、包含/跨界 panic）；`build_memmaps` 调用点单根，可测面随 T1 分类循环纯函数化后重定义 | 纯函数三案完成；其余随 T1 |

---

## 5. 文档批（01 篇正文，改文档不动码；文档线程执行）

| 条目 | 内容 | 同批 |
|---|---|---|
| I-17 | §2.6 拆节（`alloc_lowest` 缺陷分析、静态页表池两段独立成 §2.6.3；纯移动不删内容；§2.6 现 347 行占章 31%） | I-18 |
| I-18 | §1、§3–§6 章头导语与 ≥300 行巨节体检 | I-17 |
| I-19 | §3–§6 讲解条续扫「只报事实不报道理」（§2 已扫 40 条修 4；§3–§6 约 90 条待过） | — |
| I-23 | §3.7 补 `fw_payload` 真板用途段 + 三架构落位矩阵（~10 行，`[ARCH]` 三处一致；现 `fw_payload` 术语零命中） | I-22 文档连带、I-24（§3.7 一次改完） |
| I-24 + L1378/L1408 | §3.1 否决表拆两行（UEFI 形态被否 / opensbi 形态采纳）；正文 L1378「OpenSbiBootShim 无调用方」叙述与 L1408 依赖方向 TODO 同批收敛 | I-22、I-23 |
| L1488 | 「x86_64 的待退场，此处待核实，并透彻学习」——`reserved_regions` 内联窗口退场路径。**✅ 已关闭（D-068，用户 2026-11-20）**：该场景已不存在（x86 切独立镜像后内核不在低地址执行，`vm_handoff` 不再携带 identity、`kern_ident` 已删）；用户明确无需再理解旧设计。讲解可读性由 P-READER 冷读机制持续覆盖 | 已关闭 |
| L1532 | 「这个是交互式文档类TODO我读不懂」——L1424 块的性质是**文档-代码张力登记**（正文以终态口径写、代码实况是过渡形态，要求正文标注）。**✅ 已关闭（D-068，用户 2026-11-20）**：该类块的写法已由 I-27 批规范化，用户的困惑已记录在案；无需保留为开放项 | 已关闭 |
| I-25 | `os/` 全仓注释语言与锚点整改（低优；01 篇射程内样例：uefi_helpers D-64②/F8 段、bootface 头注等，随各条顺手按 a/b 口径处理） | 全仓批 |
| **§5.2.1 测试计数漂移**（本轮新登记） | `opensbi_helpers.rs` 文档记 13、`grep -c '#\[test\]'` 实际 **12**（连带"15+13+1=29"的算式应为 15+12+1=28）；`x86_64/paging.rs` 文档记 14、实际 **15**。按复算命令重跑后改数（其余核过：loader 15、uefi_helpers 1、kernel_info 9、platform 8、boot_form 6、minix-elf 23、memmap 16、boot_alloc 5、vm_handoff 7、dm_coverage 10 均一致） | 文档线程；与 §5.1 M6 行的"paging.rs 14 条"同处改 |
| **§4.6 `IDENTITY_MAP_END` 措辞**（本轮新登记） | 正文写"`IDENTITY_MAP_END`（lib.rs 里固定的低地址上界）"——D-60 闭合后 kernel 侧已是 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END` 别名，"lib.rs 里固定"失真；改为"唯一权威在 `minix-arch`（`BOOT_IDENTITY_MAP_END`），kernel 侧是别名" | D-60 回勾行（§3） |
| **I-25 追加样例：`globals.rs` 陈旧注释**（本轮新登记） | `FREE_MEMMAP` 上 `#[allow(dead_code)] // SMP memmap infra (checklist D-17); not yet wired to all call sites`——"checklist D-17"与 `todo.md` 的 D-17（IPC 过滤）不同指、锚点不可达；且 `FREE_MEMMAP` 已被 `kmain` A.2 使用，"not yet wired" 多半过期（`allow` 现在可能只为 mock 构建保留）。落地时核实 `allow` 是否仍必要，注释按实况改写 | I-25 |
| I-21 | 篇头「路径约定」13 篇重复上收 | ✅ 已上收（I21HOIST 批，用户授权关闭）：全局化入 README「全局阅读约定」，13 篇篇头改一句指针 |
| **I-30（本轮新登记）** | **01 篇正文补「内存图生产的四层责任」概念讲解**。第十轮评审收尾语点名了这件事："你这份文档已经把一个很容易被混淆的问题拆得相当清楚了——UEFI 固件事务、描述符解析、OS 内存语义分类、内核映射与分配，是不同层次的责任。现在需要做的是让这些合同经受代码和实验检验，而不是继续给它们添加抽象层。" 设计笔记 v11 已把四层拆开（§0.3 MapKey 机制 + §1.2 三条结构原因 = 固件事务层；§2.6.2 数据合同 = 解析层；§2.5.7.1 政策表 + §2.5.7.6 分层图 + §2.5.7.7 属性表 = 分类层；I-29-12 两合同 + §0.4 消费方 = 映射与分配层），但 01 篇正式文档还没有这段教学：§3.4「内存图交付成两张表」段（L1474-1487）只讲两张表的形状与五条腿来源，§4.2 只给三条现行分流规则——四层各自的责任边界（分类判据为何不能靠地址 / 固件事务为何绑死分类时机 / DM 覆盖与可分配性为何是另一个合同）未成段讲解。**落点**：§3.4 该段扩写或紧随其后新增小节。**时序约束**：四层概念讲解可先行（分层模型已获评审确认、属冻结方向），但涉及政策细节的句子不得先于 I-29 政策表批准承诺未冻结内容——先行段落只讲责任边界与判据，具体政策行文指设计笔记 v11 对应节；I-29 实施落地后 §4.2 分流规则叙述按四层模型同批改写（属性准入 / side list / WB 缺失处置都会改现行三条规则）。**验收**：读者不开设计笔记即可从 01 篇理解四层责任划分及其理由；事实断言带锚点（设计笔记节号 + 代码符号）；doc-style-lint / doc-ref-check --check / anchor-resolve 全绿 | I-29 实施批（§4.2 同批部分） |

> **2026-10-29 回勾（01TODO-B1）**：§5.2.1 计数漂移与 §4.6 `IDENTITY_MAP_END` 措辞两项已在正文闭合。前者除登记的三处（opensbi 13→12、`15+13+1=29`→`15+12+1=28`、paging 14→15）外，复算还纠正了 minix-boot 总数（23→32，原算式漏 `memmap_source.rs` 的 9 条）并把 boot-shim 的 28 口径写准（须带 `--features test-all`）；后者两处（§4.6 Step 1 与 §1 的同一措辞）均改为「唯一权威在 `minix-arch` 的 `BOOT_IDENTITY_MAP_END`，kernel 侧是别名」。

> **2026-10-29 回勾（01TODO-DOC）**：I-17（§2.6 内部拆五节，最大节 176 行；顶层不改号以保 prompt/ 与 02 篇的历史锚点）、I-18（§4/§6 补章头导语；≥300 行巨节体检：仅 §2.6 入选且已拆，§2.4 287 行未达阈值记录不改）、I-19（§3–§6 讲解条续扫 ~90 条，修 4 条纯事实条目）、I-23（§3.7 FW_PAYLOAD 落位矩阵 + `[ARCH]` 三处一致）、I-24 + L1378/L1408（否决表拆两行、TODO 块改状态叙述、I-22 终态句）、I-25 射程样例（FREE_MEMMAP 陈旧锚点、D-64②/F8 不可达标签）六项闭合。设计侧 `[ARCH]` 镜像因该文件在主树是用户未提交脏版，落在工作区未代提交。

---

## 6. 射程外指针（防重查）

- **D-60** 已闭合（kernel 侧 `use minix_arch::BOOT_IDENTITY_MAP_END` 归一，全仓单定义）；**I-20** 已闭合（b0da9074e）。
- **I-1**（kernel 独立 ELF 构建接入 xtask）原 DEFERRED 的解除前置「入口交接 ABI 设计 + boot-shim 终局跳转」已由 T2 v9 全套裁决补齐——不再独立登记，随批次五实施（bin 目标 + build.rs 一并）。
- **I-5**（ACPI RSDP）DEFERRED：物理机 x86_64 支持时实现，当前三家族 QEMU virt 均不依赖。
- **`VmBootHandoff`**（`os/libs/minix-types/src/types/boot.rs`）是内核→VM 的运行期上层交接、带 magic/version，属另一层契约，**不在 01 篇射程**。
- **T-10**（riscv64 U-Boot 真实启动链集成）在 todo.md §7.3，非 01 篇正文登记（§7.4 有引用），随 CI 窗口处理。

---

## 7. 验收总则（每条通用）

1. **证据先行**：动手前跑该条证据命令确认现状；与工单不一致时以源码为准并写明偏离。
2. **裁决已冻结**：§2 各条直接按 `01-code-design.md` v9 实施；实现证伪既定前提才重开局部裁决，不开新设计轮次。
3. **文档同步**：涉 01 篇正文的同批更新（Doc Code Sync 七项）；§5.3 交接表回填覆盖状态。
4. **门禁**：`cargo test -p <crate>` 与 `cargo clippy` 通过；共享契约层（minix-boot）条目加三架构构建与 qemu 引导冒烟；文档五门在受影响文档复跑全绿。
