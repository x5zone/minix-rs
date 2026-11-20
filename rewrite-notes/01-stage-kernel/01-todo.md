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
| I-29-impl（实施） | 按 v12 冻结设计把分类器落地并过四个验收门 | 代码实施 | **待实施（D-069，用户 2026-11-20 批准拆分）**：四个门 ① 属性政策矩阵逐行批准（凭原型 ①）② 三架构普通 WB 合同（I-29-16）③ Pass1/Pass2 具体 API（归原型 ①）④ 路径 A 端到端运行验收（原型 ②）。执行序列见 §3 的 I-29 段末（原型 ① → 原型 ② ∥ 三架构 WB → 政策矩阵 → 冻结实现规格进生产） | §3 |
| I-34（本轮新登记） | `BootStorage` 半提交防护：删 `map_dirty`，改 `fill_*` 闭包 + `MapCounts` | 设计裁决 + 代码改造 | 已裁决（D-067，用户 2026-11-20）；待实施——详见 §3 的 I-34 | §3 |
| I-35（部分实施） | `KERNEL_INFO` 的 write-once 承诺与实际二次写（`kmain:1738` 冗余 store）+ `IPC_FILTER_POOL` 注释失真 | 设计裁决 + 注释修正 | 已裁决（D-071，用户于 2026 年 11 月 20 日）。**(b) 注释项已实施（2026 年 11 月 20 日）**：`IPC_FILTER_POOL` 的「Only written once during boot」改为「初始化一次、此后在 BKL 下可变」，并核对三处可变借用（`proc_table.rs` + `syscall.rs` ×2）。**(a) `KERNEL_INFO` 四步方案仍不实施**（用户明确要求）。第三项 `Frozen::freeze` 已由 I-32 销账。详见 §3 的 I-35 | §3 |
| I-36（本轮新登记） | `UbootFileLoader::read` 的 ELF 修剪上移（`FileLoader` 抽象里藏着 ELF 语义，腿间 `BootModule.len` 不同）+ 一次核查事故记录 | 设计裁决 | **未裁决**（三候选 A/B/C 待用户选）——含主 agent 误判「已实施」的事故记录，见 §3 的 I-36 | §3 |
| I-37（已实施） | `prepare_kernel_image_v1`/`jump_kernel_image_v1` 的裸 `u64` 对改 `PreparedJump` 值对象（接反后仍能编译，交换后果=取指故障） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：四步 + mini review 修正两处（`private_interfaces`、mock 孤儿）；五条 QEMU 门链全 PASS。验收与鉴别力双向验证见 §3 的 I-37 | §3 |
| I-39（已实施） | `KernelInfo` 的空 getter 段（注释承诺 getter、下面 12 行空白）与 `assert_eq!(x,x)` 恒真测试（17 条）+ 文档把它计入 31/32 覆盖 | 测试清理 | ✅ **已实施（2026 年 11 月 20 日）**：D-077 三项 + mini review 补拓两处文档计数（32→31、196→195）；**先于 I-31** 完成。验收与同源残骸登记见 §3 的 I-39 | §3 |
| I-38（本轮新登记） | `boot_validate_and_prepare` 的四个位置实参改 `BootScalars` 值对象（两个 `VirBytes` 可互换；三个调用点各抄四字段） | 设计裁决 + 代码改造 | 已裁决（D-074，用户 2026-11-20）：采 A + 命名构造函数 + 等式 `debug_assert`；**前置依赖 I-31**（否则 `BootScalars` 定型两次）。待实施 | §3 |
| I-40（本轮新登记） | 两处 ghost API 注释：`KernelInfo::boot_modules()`（`minix-boot/src/kernel_info.rs:640`）与 `KernelInfo::memmap()`（`kernel/src/vm_handoff.rs:108`）——`KernelInfo` 上没有这两个方法（同名方法只存在于 `BootStorage`） | 注释修正 | **未裁决**（I-39 mini review 中发现，与 D-077 同一缺陷类但位置不同、超出该裁决授权）；待用户裁决是否并入 I-39 口径直接删/改写，见 §3 的 I-40 | §3 |
| I-30（已实施） | aarch64 形态判别改链接期分叉（删 `arena_pa == 0` 运行期哨兵） | 代码改造 | ✅ **已实施（2026 年 11 月 20 日）**：新增 `fw-aarch64-uefi-recv` 特性、两条 `rust_image_main` 各归一个 cfg、xtask 盘面特性切换；接收臂的 `x0==0` 改为诚实停机。mini review 修掉 8 条死码；结构鉴别力已证（接收产物里编不进 `bootface_a64`）。验收见 §3 的 I-30 | §3 |
| I-31（已裁决·可开工） | wire v10：删 `syscall_entry` / `user_sp`+`kern_stack_top` / `startup_objects` 分区（`BOOT_HANDOFF_V1_VERSION` 9→10） | 协议契约变更 + 代码改造 | 已裁决（D-062，用户 2026-11-20）：三条一次升版。待实施——验收 grep 六项、文档同步含 §4.1 五族句（D-072 补强），详见 §3 的 I-31 | §3 |
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

**为什么现在不做**：D01 的 R35 是确认轮，按 §3.6 确认轮不实施 ⑥。裁决已生效，实施留待下一 design/code batch。

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



### I-35（已裁决·待实施）`KERNEL_INFO` 的 write-once 承诺与二次写 + 两处注释失真

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

**验收**：
- `sed -n '1738p' os/kernel/src/lib.rs` 不再含 `store_kernel_info`；
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

### I-40（未裁决·I-39 的实现副产物）两处 ghost API 注释

**来源**：I-39 实施时的 mini review。D-077 修掉的是「`impl` 末尾承诺一段 getter，实际一个都没有」；实施后按同一判据（注释是否点名一个不存在的方法）扫全文件，又发现两处：

| # | 位置 | 现文 | 事实 |
|---|---|---|---|
| ① | `os/libs/minix-boot/src/kernel_info.rs:640` | `/// deliver, in \`KernelInfo::boot_modules()\`.` | `KernelInfo` 的 `boot_modules` 是 **pub 字段**；`pub fn boot_modules()` 只存在于 `BootStorage`（`boot_storage.rs:336`） |
| ② | `os/kernel/src/vm_handoff.rs:108` | `/// select from this list, not from the raw \`KernelInfo::memmap()\`: the` | 同上：`memmap` 在 `KernelInfo` 上是字段，方法只在 `BootStorage` 上 |

**为什么没有顺手改**：D-077 是用户裁决，授权范围写明是「空 getter 段 + 恒真测试 + 文档计数 32→31」三件；这两处虽属同一缺陷类（注释点名不存在的 API，读者会白搜一次），但位置与对象都不同。按 `fix-guard` 的「一次一条」，混入本次提交会让裁决边界变模糊——尤其 ② 在 `minix-kernel` 而不在 `minix-boot`，跨 crate。

**待裁决的两个选项**：

- **A（推荐）**：直接改写措辞，把它们指回真实载体——① 改「`in \`BootStorage::boot_modules()\``」，② 改「`from the raw \`KernelInfo::memmap\` field`」。纯注释，零行为变化，与 D-077 采 A（删/改残骸而不造 API）同一取向。
- **B**：给 `KernelInfo` 真的加这两个访问器。与 D-077 否决 B 的理由相同（字段已全 `pub`，造 getter 会制造两套等价 API，与 I-34/D-067 消灭「两个可不一致的事实」的方向相反），不推荐。

**验收（若走 A）**：

- `grep -rn "KernelInfo::boot_modules()\|KernelInfo::memmap()" --include=*.rs os/` → 0；
- 全 workspace `cargo check` 0 error（纯注释改动）；
- `cargo test -p minix-boot -p minix-kernel --features minix-kernel/mock` 全绿。

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

**为什么现在不做**：用户 2026-11-20 批准方案但只做记账；且本条**必须等 I-31 落地**后实施，否则 `BootScalars` 要定型两次。

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

### I-36（未裁决·待裁决）`UbootFileLoader::read` 的 ELF 修剪上移（+ 一次核查事故记录）

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

（`:1518` 是 R35 批修的，当时把「幂等 no-op」改成「对已在槽内的输入是 no-op」，但仍保留了「差别在字节来源」这个错误前提。）

**三个候选（待用户裁决）**：

| 选项 | 做法 | 代价 | 收益 |
|---|---|---|---|
| **A. 修剪上移**（面板建议） | 把 ELF 修剪移到共享消费者（`load_boot_modules_into` 或 `load_kernel_with_loader`），`read` 只返回原始字节 | `loader.rs` 一处新增 + `opensbi_helpers.rs` 一处删除 + `staged_elf_used_len` 并入 | 接口恢复「只处理字节来源」；腿间 `len` 差异消失；文档不用改 |
| **B. 只改文档** | 把 `:1502`/`:1518` 改成「U-Boot 腿的 `read` 额外做装载视图修剪」 | 两处文字 | 零风险；但腿间差异仍然存在，且"文件来源接口藏着 ELF 语义"这个抽象泄漏保留 |
| **C. UEFI 侧补齐修剪** | 让 UEFI 的 `read` 也做同一修剪，两腿对齐 | `uefi_helpers.rs` 一处 + 复用修剪逻辑 | 腿间差异消失（对外可观察行为统一）；但仍把 ELF 语义留在 `read` 里 |

**我的建议：A。** 理由：这条的根因不是"两腿行为不同"（那可以用 C 对齐），而是**`FileLoader` 这个抽象里藏了 ELF 语义**——面板原文写的是「`read` 不只是字节来源」。A 修的是抽象边界，C 只是让两边一致地泄漏。而且 `staged_elf_used_len` 与 `read` 的修剪是同一逻辑的两份实现（`uboot_main.rs:267-320` 与 `opensbi_helpers.rs:211-216`），A 能顺带合并它们。

**为什么现在不裁决/不实施**：本条不在用户的裁决队列里（用户此前只要求先处理 D2-01 与 D2-05 的记账）；且 A 的改动触及所有五条腿的模块装载路径，需要与 I-31/I-33 的实施顺序协调。

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

**为什么现在不做**：本条与 D-062（wire v10 删 `startup` 族）和 D-065（直核腿统一 owned 路径，sources 族写入者从 1 条腿变成 3 条腿）改同一批行。实施顺序由后续统筹规划决定（族数与写入者数量在 D-062/D-065 落地后才定稿），本条不写顺序约束。

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
