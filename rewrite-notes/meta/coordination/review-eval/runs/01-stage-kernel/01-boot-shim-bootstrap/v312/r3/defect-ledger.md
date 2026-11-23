# 01-stage 01 R3 缺陷台账（V3.12，结构轮）

- edition: V3.12
- 轮次: R3 full（第 3 个修改轮，结构轮）
- 基线: doc sha `98b7cdaf…`（3,049 行），commit `9f1327187`（不是 `a6a94b942`；后者是 R2 终态 `cbc105c7…`）
- 面板 FINDING：READER-A 8（P2 4 / P3 4）、READER-B 5（P3 5）、READER-C 2（P2 2）+ 附带 2（P3 2）、P-FACT 3（P2 2 / P3 1）、DESIGN-D2 4（P2 2 / P3 2）；P-REFUTE 1 个 P2 观察 + 2 条非阻断观察。
- 处置：① 28 行 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 3（P-REFUTE CLAIM 01/02/06）；未闭环 0。发现 ID 共 30 个：22 主 FINDING + 2 附带 + 3 P-REFUTE 观察 + 3 复验新发现；A-01/B-01、A-04/B-02 两组合并成同一行，故台账 28 行。根因列共 20 个不同 ID（R3-01..R3-20）。
- 轮型: full；轮结果: ROUND-CLOSED（修复、复验与关账检查后写入）。

## 面板 FINDING 主表

| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |
|---|---|---|---|---|---|---|
| READER-A-01、READER-B-01 | 1.0 | ① | §1.0 小结第 2 条首次使用“框 1/框 2”时补括注定义，不再要求先跳到术语块。 | `rg -n '可用内存清单（框 1）' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1，命中 §1.0 小结） | R3-01 | READER-A/B R3 |
| READER-A-04、READER-B-02 | 3.4.3、3.2.2.2、2.3 | ① | §3.4.3 首次出现处补 `TTBR1`/`Sv39`/`satp` 的定义；§1.0 术语补 IS/RS/PM 全称；§3.2.2.2 首次使用处展开。 | `rg -n 'TTBR1 是 AArch64|Sv39 是 RISC-V|satp 是 RISC-V' 文档`（期望各 1）；`rg -n 'PM 是进程管理器|RS 是重生服务器|IS 是信息服务' 文档`（期望 3） | R3-01 | READER-A/B R3 |
| READER-A-05 | 3.6、4.0、4.5 | ① | §3.6 的“包装因同址成为 no-op”改为“跳过字节拷贝但仍构造 `KernelInfo` 并写槽，第二次调用 panic”；§4.0 术语同步。 | `rg -n '包装跳过字节拷贝|首次同址跳过复制' 文档`（期望 ≥2） | R3-03 | READER-A R3 |
| READER-A-06 | 2.4.2.5 | ① | 自指指针改为“见本节上表的‘还 VM 镜像’行”。 | `rg -n '本节上表' 文档`（期望 ≥1） | R3-04 | READER-A R3 |
| READER-A-07 | 3.5.2 | ① | “标题里/本节标题本身”改为“本节论点里/本节论证的单向依赖原则本身”。 | `rg -n '标题里|本节标题' 文档`（期望 0）；`rg -n '本节论点里|单向依赖原则本身' 文档`（期望 ≥2） | R3-04 | READER-A R3 |
| READER-A-02 | 3.7 | ① | §3.7“四类错误”改为“五类错误”，与分号列出的五项一致。 | `rg -n '五类错误各有落点' 文档`（期望 1） | R3-02 | READER-A R3 |
| READER-A-03 | 3.2.1 | ① | 字段表“删除 22 个字段”前补口径：22 是 C `kinfo_t` 累计退场数，不是 Rust 结构字段数。 | `rg -n '22 是 C `kinfo_t` 的累计退场' 文档`（期望 1） | R3-02 | READER-A R3 |
| READER-A-08、READER-C-S03 | 5.1 | ① | §5.1 开头把“三档/四个计数”改写成互斥的腿名单与覆盖深度，不再让“四条腿”与 aarch64 UEFI 并列。 | `rg -n '有启动门链的四条腿' 文档`（期望 1） | R3-10 | READER-A/C R3 |
| READER-B-03 | 3.2.2.3 | ① | 标题“四个字段的补充”改为“五处字段补充”；“四个读者任务”列举补齐为四项。 | `rg -n '五处字段补充|四个读者任务分组：字段去向' 文档`（期望各 1） | R3-02 | READER-B R3 |
| READER-B-04 | 2.4.2.4 | ① | §2.4.2.4 的 `VmBootHandoff` 版本指针改为就地给常量名与当前值，并指向 `minix-types` 定义。 | `rg -n 'VM_BOOT_HANDOFF_VERSION' 文档`（期望 ≥1，含当前值 9） | R3-04 | READER-B R3 |
| READER-B-05 | 2.3、3.2.2.4 | ① | §2.3 删除扫描段补空键的字节推演；§3.2.2.4 其八回指该段。 | `rg -n '空键' 文档`（期望 ≥2，含 `namelen = 0` 推演） | R3-06 | READER-B R3 |
| READER-C-01 | 3.2.2.4 | ① | “其一”补“非首条记录还可能被中段匹配误删”，与 §2.3 的 `xboard=2` 推演一致。 | `rg -n '中段匹配误删|xboard=2' 文档`（期望 ≥2） | R3-06 | READER-C R3 |
| READER-C-02 | 1.1、3.2.1 | ① | §1.1 的“只能由发送侧解析 ELF”限定为三条 v1 腿；直核腿用 `KERN_*` 常量。 | `rg -n '三条 v1 腿的接收侧没有 ELF 头可读' 文档`（期望 1） | R3-05 | READER-C R3 |
| READER-C-A01 | 3.2.1、4.4 | ① | “三架构直核面”改为“两个生产直核面 + x86_64 测试直核入口”，与 §3.6 的“无对应形态”一致。 | `rg -n 'x86_64 测试直核入口' 文档`（期望 ≥1） | R3-07 | READER-C R3 |
| READER-C-A02 | 1.2、3.7 | ① | I3 行与 §3.7 所有权段补两个直核腿的六类载荷归栈上 `BootStorage`。 | `rg -n '两个直核腿的六类载荷|两个直核腿各拥有一份栈上的' 文档`（期望 2：I3 行与 §3.7 所有权段） | R3-07 | READER-C R3 |
| REFUTE-I4 | 3.7 | ① | §3.7 总起句加“接收侧”限定；发送侧建根只依赖装载/分类门，见 §5.1。 | `rg -n '接收侧校验在消费' 文档`（期望 1） | R3-08 | P-REFUTE R3 |
| REFUTE-OBS1 | 4.5 | ① | §4.5 的“written exactly once”旁补“守卫是 check-then-write，依赖 BKL 之前的单线程启动”。 | `rg -n 'check-then-write' 文档`（期望 1） | R3-03 | P-REFUTE R3 |
| REFUTE-OBS2 | 3.6、4.4 | ① | “五条腿都只构造 `BootReport`”补“形状收敛；UEFI 腿的 `source_records` 视图不完整，wire 才是权威”。 | `rg -n '形状收敛，不保证发送侧视图完整' 文档`（期望 1） | R3-09 | P-REFUTE R3 |
| FACT-01 | 4.3 | ① | §4.3 的 12 项复算命令改到 `BOOT_MODULE_IDENTITIES` 范围（实测输出 12）。 | 按文档新命令复跑，输出 12 | R3-11 | P-FACT R3 |
| FACT-02 | 2.1 | ① | §2.1 的 `exec.c:736` 手写行号改为符号锚点 `exec.c:exec_multiboot`，文字绑定 `snprintf`。 | `rg -n 'exec.c:exec_multiboot' 文档`（期望 ≥2）；`tools/anchor-resolve.sh --check 文档`（87/87） | R3-12 | P-FACT R3 |
| FACT-03 | 3.3 | ① | §3.3 的 `SourceRecord` 字段清单补全为 11 个（按身份/来源范围/固件原值分组）。 | `rg -n '`raw_type`' 文档`（期望 ≥1）；字段数与 `memmap_source.rs` 一致 | R3-13 | P-FACT R3 |
| DESIGN-01 | 3.7、4.4.1（代码 bootface_common.rs/lib.rs） | ① | 直核发送侧的 `authority` 仍是两条独立实参；在 `arch_boot_finish` 的 `store_boot_report` 前加可信链断言：记录非空时，至少一条记录的 `input_object` 等于传入 authority。补宿主负控。 | `rg -n 'authority.input_object\\(\\)' os/kernel/src/lib.rs`（期望 ≥1）；`cargo test -p minix-kernel --features mock`（PASS） | R3-14 | DESIGN-D2 R3 |
| DESIGN-02 | kmain（代码） | ① | 生产 `kmain` 的 `_kernel_info` 参数被全局遮蔽：加 `ptr::eq` 可信链断言、恢复参数名、删除“Evidence capture (temporary, v3)”块与“can be deleted”过程注释；入口合同改为“参数与全局同址，全局是唯一权威”。 | `rg -n 'ptr::eq\\(kernel_info' os/kernel/src/lib.rs`（期望 1）；`rg -n 'Evidence capture|can be deleted' os/kernel/src/lib.rs`（期望 0）；QEMU x86 shim 门 PASS | R3-15 | DESIGN-D2 R3 |
| DESIGN-03 | handoff_v1_write.rs（代码） | ① | 删除 `Serializer` 的 `modules_n`/`platform_n`/`param_n` 三个死计数器及其赋值；`byte_len` 仍由输入长度写。 | `rg -n 'modules_n|platform_n|param_n' os/libs/minix-boot/src/handoff_v1_write.rs`（期望 0）；`cargo test -p minix-boot --lib`（PASS） | R3-16 | DESIGN-D2 R3 |
| DESIGN-04 | 五处源码注释（代码） | ① | 改五处过期注释：`main.rs` 的七个分区、`bootface_common.rs` 两处发送侧 `KernelInfo`、`bootface_a64.rs` 的旧形状与 cleanup 延期语、`kernel_info.rs` 的 `validate` 调用说明、`lib.rs` 的 `bootstrap_len` 旧字段描述。 | `rg -n 'seven payload|left to a separate cleanup|Called by `kmain` at entry|bootstrap_len==0'`（期望 0） | R3-17 | DESIGN-D2 R3 |
| NEW-01 | 4.4.1 | ① | 独立文档复验发现本轮新增的时间序表把 v1 的 `store_kernel_info` 写在 `boot_validate` 之前；按源码 `resume_additive` 调序改为 `resume_boot_pt_alloc → boot_validate → store_kernel_info → jump`。 | `sed -n '/按时间顺序把两条腿并排看/,/下表按门/p' 文档`（v1 列第 4 行为 `resume_boot_pt_alloc`、第 5 行为 boot_validate、第 6 行为 store） | R3-18 | 文档复验 R3 |
| NEW-02 | 3.3 | ① | `validate_requiring_sources` 的 strict 分支实际调用 `audit_source_coverage_for(..., Some(authority.input_object()))`，不是无参 `audit_source_coverage`；正文改为两个入口并点名后者。 | `rg -n 'audit_source_coverage_for' 文档`（期望 ≥1） | R3-19 | 文档复验 R3 |
| NEW-03 | 1.1.1 | ① | `RamAuthority` 是单字段包装 + 两个关联常量，不是枚举；类型速查改为“两个关联常量（不是枚举变体）”。 | `rg -n '两个关联常量' 文档`（期望 ≥1） | R3-20 | 文档复验 R3 |

## P-REFUTE 附加

| claim ID | 结果 | 处置 | 根因 |
|---|---|---|---|
| REFUTE-01（CLAIM 01：I1 值可跨镜像） | 反证失败（成立） | FP；不改文档 | — |
| REFUTE-02（CLAIM 02：I2 来源可追溯） | 反证失败（成立） | FP；不改文档 | — |
| REFUTE-03（CLAIM 03：I3 所有权明确） | 生产路径成立；附非阻断观察 | OBS1 改写 §4.5 | R3-03 |
| REFUTE-04（CLAIM 04：I4 校验先于消费） | 接收侧成立；发送侧措辞过宽（P2） | ① 加接收侧限定 | R3-08 |
| REFUTE-05（CLAIM 05：五腿收敛） | 反证失败（成立）；附非阻断观察 | OBS2 补形状收敛限定 | R3-09 |
| REFUTE-06（CLAIM 06：C 基线） | 反证失败（成立） | FP；不改文档 | — |

## 建议（不进入主表）

本轮按“对初学者可读性有直接收益、且不改变事实/结构风险低”的标准采纳了 13 条建议，其余为纯风格取舍，不进入 FINDING 口径：

| 建议 ID | 处置 |
|---|---|
| READER-A-S01/S02/S04/S05/S06/S09/S10 | 采纳：拆分 `mem_high_phys` 长句、补 arena 身份、补负存在断言的重放命令、补 160→128 的 32 B 分解、明确“第一个区间”、补 §3.6 换算表指针、去 `memmap` 重复短语 |
| READER-B-S04/S07/S09/S10/S11/S12/S13/S14 | 采纳：去重、补时间序表、DM 缩写、审计两层关系、补 8→9 说明、注册计数口径、去重复 02 指针、“前两个字段” |
| READER-C-S01/S02/S03/S05/S06/S08 | 采纳：跳板大小的测量口径、`setup_mbi` 调用序、§5.1 点名四条腿、`mmap_size` 语义、§7.5 DTB 路径、`arch=i386` 限定 |
| FACT 建议 1/2 | 采纳：§4.4 节选补省略说明、`add_memmap` 补首参 |
| 其余 | 未采纳：涉及整节搬迁（§3.2.2 拆节、§4.1 分配器段移 §4.6）或纯排版偏好；不构成缺陷，不改正文 |
