# D01 suggestions（R26）

> 本文件只收**清单外偏好/非缺陷建议**：不进 `defect-ledger.md`、不进 `metrics.md`、不阻塞收敛。
> 来源：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round26/*.md` 的 `## SUGGESTION` 段。
> 下一次 edition 边界可以把反复出现的模式晋升为规则或读者验收清单项。

## 已收（按角色）

- P-FACT-A：§4.6 引文循环头改写标注；§2.7 ARM32 省略指针收窄；§4.6 x86_64 空表主语补全。
- P-FACT-B：§4.2.2 `KNOWN` 的“16 位”措辞；§3.4.1 直核 startup 枚举措辞；§3.8 `pt_free` 回收链措辞。
- P-READER-A：§2.8 五条 bullet 化；§3.4.3 `repr(C)` 理由半句；§2.4 交叉引用“紧随其后”；§4.5 容量分组判据；文档/画像的 §1.3 导航句对齐。
- P-READER-B：见 `reader-b.md` 的 8 条 SUGGESTION（标题/缩写表/术语展开等）。
- P-READER-C：见 `reader-c.md` 的 10 条 SUGGESTION（表格摘要、全局计数口径、v9 注释措辞等）。
- P-DESIGN-D2：6 条“拒绝修改”的理由已写在 `design-d2.md` 的 VERDICT（避免下轮重复提）。

## 处置

- 本轮不修；不声称是缺陷。
- 下一 edition 边界按出现频次与读者影响排序，最多晋升 2 条为规则/清单项。

## R32/R33 补正入账（2026-11-11）

> V3.3 §4.0c/§4.0e：读者与设计 P3 默认 suggestions。以下条目在原轮已按建议处理，
> 本表补齐合法出口；不进入修复队列、不阻塞收敛。

| ID | 面板摘要 | 原处置 |
|---|---|---|
| RD32-DESIGN-D2-01 | build_kernel_info` 的 `memmap`/`reserved_regions` 是相邻同型参数，互换可静默通过全部校验门 | 已修（文档） |
| RD32-DESIGN-D2-02 | BootStorage` 的「半提交不可流出」守卫只覆盖两张内存表；文档 §3.4.5 声称覆盖全部 `*_slots_mut`，且把仅测试构建存在的 `set_startup_len` 列为生产 one-shot 提交口 | 已修（文档） |
| RD32-DESIGN-D2-03 | uboot_main.rs` 声称 `uboot_layout::MODULE_NAMES` 与规范表「经 opensbi_helpers 的编译期断言单一来源」，该断言不存在；U-Boot 腿因此带着一份无编译期钉住的名字表副本 | 已修 |
| RD32-DESIGN-D2-04 | 模块名落入 `BOOT_MODULE_NAME_STORE` 有两份实现、两套契约强度：`store_kernel_info` 截断到 15 字节并回退 `"?"`，v1 wire 解码收 16 字节并对非法 UTF-8 直接 panic； | 已修 |
| RD32-DESIGN-D2-05 | 固件指针一次性安装口用 `static mut` + 仅 debug 构建的重复安装检查，与同一仓内同类「写一次」机制的既定标准（两种构建档都 fail-closed、避免 `static mut`）不一致 | 已修 |
| RD32-DESIGN-D2-06 | DESIGN-D2-17 的裸 `(u64,u64)` 白名单与窗口组装层的实际形态不符：`push_common_boot_windows` 的 `bump: Option<(u64,u64)>` 是白名单之外的第四处裸元组签名，模块/平 | 已修 |
| RD32-DESIGN-D2-07 | DTB 腿上 DTB blob 位于 conventional 内存、是交接后的活输入，却既不在 startup-object 规范集合、也不在扣除记录里；与 arena 的处理不对称——arena 有成文生命周期合同与 poison 测试 | 已修 |
| RD32-READER-A-02 | **位置**：§3.4.2 `reserved_regions` 展开条目（约 1692 行）。 | 已修 |
| RD32-READER-A-03 | **位置**：§2.6.4 首段（约 1159 行）；`prot_init` 全文首次出现。 | 已修 |
| RD32-READER-A-04 | **位置**：五处正文将评审轮次/裁决 token 用作事实或时间锚点；对照 §1.4 图例的自设纪律（约 147 行）。 | 已修 |
| RD32-READER-A-05 | **位置**：§1.2 结构图（约 49–50 行）与读图规则段（约 71 行）。 | 已修 |
| RD32-READER-A-06 | **位置**：§3.1.1 接线状态表（约 1450 行）对照 §3.7/§4.8/§5.2.3 的同一产物称呼。 | 已修 |
| RD32-READER-A-07 | **位置**：§6.4 第三层 GDB 命令注释（约 3482 行）对照 §4.8（约 3079 行）。 | 已修 |
| RD32-READER-A-08 | **位置**：§6.1 里程碑表 `v0–v4` 行（约 3351 行）；相关：§3.4.3 的 v1 协议族与信封版本 v8/v9。 | 已修 |
| RD32-READER-A-09 | **位置**：boot2 取证边界的复述位置：§1.1 两段（约 32、34 行）、§1.4 边界声明（约 154 行）、§2.1（约 180 行）、§5.1 M1（约 3129 行）。 | 已修 |
| RD33-DESIGN-D2-01 | **位置**：文档 §4.6（rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2906）与 §4.2.1 动作 9 表行（同文件:2413）；源码 `os/boot-shim/ | 已修 |
| RD33-DESIGN-D2-02 | **位置**：文档 §5.1 M2 行（01-boot-shim-bootstrap.md:3130，记载 D01-R1 D2-05 的"只收有序子集"决定）；源码 `os/libs/minix-boot/src/kernel_info.r | 已修 |
| RD33-DESIGN-D2-03 | **位置**：文档 §3.4.5（01-boot-shim-bootstrap.md:1768："set_modules_len/set_platform_len/set_param_len/set_memmap_sources_len/s | 已修 |
| RD33-DESIGN-D2-04 | **位置**：文档 §3.4.5（01-boot-shim-bootstrap.md:1768：其余五族"半提交状态依赖调用纪律，不靠 report() 挡"）；源码 `os/libs/minix-boot/src/boot_storage | 已修 |
| RD33-DESIGN-D2-05 | **位置**：文档 §4.4 校验门表（01-boot-shim-bootstrap.md:2718-2734，自称"共十三行：十二行是门"的门枚举）；源码 `os/kernel/src/lib.rs:2633-2639`（`init_pr | 已修 |
| RD33-DESIGN-D2-06 | **位置**：源码 `os/boot-shim/src/opensbi_helpers.rs:494-507`（`install_boot_file_table` 的双安装检查是 `debug_assert!`，注释自认"release b | 已修 |
| RD33-DESIGN-D2-07 | **位置**：文档 §3.2（01-boot-shim-bootstrap.md:1517："U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度，loader 的二次修剪是幂等 | 已修 |
| RD33-DESIGN-D2-08 | **位置**：源码 `os/libs/minix-boot/src/handoff_v1_write.rs:158-169`（`write_envelope(info: &KernelInfo)` 的文档注释："receiver-side  | 已修 |
| RD33-READER-A-03 | **位置**：§2.6.3，memory_init 引文之后、createpde 引文之前（L1113） | 已修 |
| RD33-READER-A-04 | **位置**：§3.6，`plan_kernel_mapping` 引文前的括注（L1910）；对照 §3.7 引文头（L1981）与引文内省略行（L1999） | 已修 |
| RD33-READER-A-05 | **位置**：§3.4.1 出处索引图例（L147）；正文 L1647、L1648 | 已修 |
| RD33-READER-A-06 | **位置**：§2.4.1.3（L729、L731、L735） | 已修 |
| RD33-READER-A-07 | **位置**：§5.2.3 深拷探针段（L3270） | 已修 |
| RD33-READER-B-03 | - **类别**：SUGGESTION（声明顺序自相矛盾） | 已修 |
| RD33-READER-B-04 | - **类别**：SUGGESTION（权威声明枚举不全） | 已修 |
| RD33-READER-B-05 | - **类别**：SUGGESTION（参见清单覆盖缺口 + 命名观察） | 已修 |
| RD33-READER-B-06 | - **类别**：SUGGESTION（死端指针链，两节互不知情） | 已修 |
| RD33-READER-B-07 | - **类别**：SUGGESTION（同句冗余） | 已修 |
| RD33-READER-B-08 | - **类别**：SUGGESTION（图前图后重复授课） | 已修 |
| RD33-READER-B-09 | - **类别**：SUGGESTION（跨节逐字重复） | 已修 |
| RD33-READER-B-10 | - **类别**：SUGGESTION（否认无中生有的构造） | 已修 |
| RD33-READER-B-11 | - **类别**：SUGGESTION（列表结构被顶格段落打断） | 已修 |
| RD33-READER-B-12 | - **类别**：SUGGESTION（标题排版一致性） | 已修 |
| RD33-READER-B-13 | - **类别**：SUGGESTION（指代含混） | 已修 |
| RD33-READER-B-14 | - **类别**：SUGGESTION（节内四连复述） | 已修 |
| RD33-READER-B-15 | - **类别**：SUGGESTION（表格单元巨型化） | 已修 |
| RD33-READER-B-16 | - **类别**：SUGGESTION（代码摘录的未解码符号 + 过渡段归属含混） | 已修 |
| RD33-READER-C-02 | - **位置**：§2.2 开头（L256）与文档头教学目标 ①（L7）。 | 已修 |
| RD33-READER-C-03 | - **位置**：§2.3 "同一个 off-by-one 的第二种表现"段（L464）。 | 已修 |
| RD33-READER-C-04 | - **位置**：§6.4 第三层 GDB 命令注释（L3482）。 | 已修 |
| RD33-READER-C-05 | - **位置**：§4.3 防线总述（L2500）。 | 已修 |
| RD33-READER-C-06 | - **位置**：§3.4.2 十条可观察分歧之"其五"（L1704），及其二（L1701）与之九（L1708）。 | 已修 |
| RD33-READER-C-07 | - **位置**：§5.4 自检（L3309）与 §5.1 M6 覆盖状态（L3134）。 | 已修 |
| RD33-READER-C-08 | - **位置**：§6.2 panic 路径第一约束（L3412）。 | 已修 |
| RD33-READER-C-09 | - **位置**：§2.7 `setup_mbi` 解码的 cmdline 条目（L1312）。 | 已修 |
| RD33-READER-C-10 | - **位置**：§4.4 校验门表（L2730）与校验层次第 4 项（L2714）。 | 已修 |
| RD33-READER-C-11 | - **位置**：§2.2 第 6 条（L286）与 §1.4 边界声明（L153）、§2.6.4（L1161）。 | 已修 |
| RD33-READER-C-12 | - **位置**：§2.6 结算（L1190）与 §2.8 换栈边界（L1385）。 | 已修 |

## R34 确认轮（设计 P3 / 读者 SUGGESTION）

> 设计 P3 7 条由 DESIGN-D2 报出，按 §4.0c 严重度门槛分流；读者三角色另有 8+11+7 条 SUGGESTION，
> 原文在各 `panel/round34/reader-*.md`，不进台账。

| ID | 摘要 | 处置 |
|---|---|---|
| RD34-DESIGN-D2-01 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-06 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-07 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-08 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-09 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-10 | （面板标题缺） | 不实施（建议级） |
| RD34-DESIGN-D2-11 | （面板标题缺） | 不实施（建议级） |

## R31 补清点（读者/设计 P3，未入修复队列）

| ID | 摘要 | 处置 |
|---|---|---|
| RD-R31-06（DESIGN-D2-43/44/45） | 直核腿模块落点缺「目标 bump 池 vs 内核跨度」断言；startup 记录身份未强制；UEFI 腿 startup 集合用栈上局部数组 | 设计 P3，随 `R35-design-batch` 评估 |
| RD-R31-08（READER-A-04/05/08/11/12、READER-B-06..11、READER-B-14..16、READER-C-03/04） | 术语首次定义位置、引用点清单三处不一致、编辑史括注、重复展开、取舍未补述 | 读者 P3；其中 B-15/B-16 已顺手修 |
| 补充：mock 替身语义分叉 | `MockPaging::map_huge` 在任意已映射页返回 `AlreadyMapped`，三架构真实实现只在 1 GiB 叶冲突时返回 | 测试替身缺口，下一 code batch 统一 |

## R35 确认轮（读者/设计 P3）

| ID | 摘要 | 处置 |
|---|---|---|
| RD35-READER-A-04 | `KinfoStruct` 的 48 字节缺字段宽度加法（已补，仍按 P3 登记） | 已顺手修 |
| RD35-A-01..A-14 | 读者 A 的 14 条措辞/指针/示例建议 | 登记不实施（读者 P3） |
| RD35-S-01..S-09 | 读者 B 的 9 条结构编辑建议（节边界、重复指针化、过程词、记号过载等） | 登记不实施 |
| RD35-READER-C-S01..S11 | 读者 C 的 11 条一致性建议（设栈范围、门链措辞、差集算式、术语冲突等） | 登记不实施 |
| RD35-DESIGN-D2-07..13 | 设计 P3 七项（裸 u64 可互换、腿特定切片、startup 分区、`InputObjectId` 双角色、`free_pt_page` 静默、`write_envelope` 注释、派生标量重复） | 随 `R35-design-batch`（D-060）评估 |

## R36 确认轮建议（V3.4）

> 来源：`panel/round36/` 的 33 条 SUGGESTION + 主 agent 按 §4.0e 降级的三条 reader 结构项（READER-B-05/06/08）+ 两条设计 P3（DESIGN-D2-03/09）。全部不阻断收敛，登记不实施；下一 edition 或专门清理批评估。

- **FACT-A 4 条**：手写 `file:line` 锚点纪律（33 处，31 处一致、2 处已在本轮修）；「改写/示意」引文块的符号存在性检查（ghost API 类的机械门缺口）；§2.4.2 VM 回收措辞限定为「其余 11 个模块」；v10 术语双关拆分（已随 R36-01 改写，建议保留为写法约定）。
- **FACT-B 4 条**：§6.4 把「U-Boot shim」改成「U-Boot 接收镜像」；§6.3 shim 栈来源改 `shim_stack_window()`（本轮已顺手改，建议保留）；§4.4 `boot_validate_and_prepare` 引文头补 `kern_stack_top` 对齐断言省略；§6.1 的 `step4` 行限定为 aarch64 接收腿（本轮已顺手改）。
- **READER-A S01–S10**：读图规则与编号元信息前置；`KernelInfo`/`v10` 首次出现补回指；§2.8 调用树限定 x86-32 主链；`_C_LABEL` 解释；编号图例补 `I-<nn>`；正文过程注记与图例位置；§6.2「四条约束」与分点对齐；§6.4 U-Boot 接收镜像措辞；`ESP` 与 `%esp` 撞名；「非空悬挂指针」措辞。
- **READER-B S-B-01–S10**：读图规则与 D 编号前置；§3.7 计数表位置；§6.4 接收腿措辞；§1.1 boot2 边界重复；§2.6.3 指代；§5.2.1 分界线兑现；§3.2 收益列表混入单腿事实；§2.4.1.4 C 分析章展开 Rust 容量；正文指向 review-eval 协调目录；§2.6.4 大页三条去向重复。
- **READER-B-05（原 P2，降 P3）**：启动路径计数在 §1.4/§3.7/§4.8/§5.2.3/§6.4 重复展开；本轮已把 §3.7 的「唯一权威」改成「换算基准」，保留各节自包含复述。进一步收敛为指针属下一轮编辑。
- **READER-B-06（原 P2，降 P3）**：Ch5 的 §5.1 覆盖列、§5.2 证据层、§5.3 TODO 详情对 M4/M8/M12 重复展开同一批测试；建议改为「状态 token + 证据指针」，需结构性重排。
- **READER-B-08（原 P2，降 P3）**：§1.4 混装范围/术语/编号/边界六类主题；建议拆成 §1.4 范围与 §1.5 术语编号索引，需章节重排与锚点迁移。
- **READER-C S01–S05**：§2.5「第一次」悬空指代；评审过程工件（OPEN-DEBT/suggestions/F-R34-01）进正文；设计稿指针缺路径；§7.1 `smp_gpt_v3.md` 自指；交接寄存器事实六处重复。
- **DESIGN-D2-03（P3）**：删 `startup_record()` 与无生产者的 Startup 枚举分支，wire 数值保留为保留位；用 `EMPTY_SOURCE_RECORD` 替 `ZERO_RECORD`。
- **DESIGN-D2-09（P3）**：`PAIR_STORAGE` 与 `resv_claims` 的五个 `static mut` 改调用方局部 + `&mut`；需先实测 boot-shim 栈余量（约 11 KiB）。

### R36 建议编号（便于追溯）

| 建议 ID | 来源 | 内容摘要 | 去向 |
|---|---|---|---|
| R36-FACT-A-S1 | fact-a SUGG 1 | 手写 `file:line` 锚点纪律 | 下一 edition 锚点门 |
| R36-FACT-A-S2 | fact-a SUGG 2 | 改写/示意引文块的符号存在性检查 | 工具候选 |
| R36-FACT-A-S3 | fact-a SUGG 3 | §2.4.2 VM 回收限定为其余 11 个模块 | 下一轮文风/事实 |
| R36-FACT-A-S4 | fact-a SUGG 4 | v10 术语双关拆分 | 已在 R36-01 部分落地 |
| R36-FACT-B-S1 | fact-b SUGG 1 | §6.4 U-Boot 接收镜像措辞 | 已在本轮顺手修 |
| R36-FACT-B-S2 | fact-b SUGG 2 | §6.3 shim 栈来源 | 已在本轮顺手修 |
| R36-FACT-B-S3 | fact-b SUGG 3 | §4.4 引文头补对齐断言省略 | 下一轮文风 |
| R36-FACT-B-S4 | fact-b SUGG 4 | §6.1 step4 限定 aarch64 接收腿 | 已在本轮顺手修 |
