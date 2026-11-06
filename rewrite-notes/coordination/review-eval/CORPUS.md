# CORPUS —— review 工作流固定评测语料

> 规则：语料一旦进入本表，不允许为了“让本轮好看”而改写期望；新增条目可以，修改期望必须写版本记录。
> 每次跑同一批案例，是判断“工作流是否进步”的前提；换一篇更容易的文档只会制造进步假象。

## 一、语料三类

| 类型 | 作用 | 何时跑 |
|---|---|---|
| `D01`–`D10` | 真实篇目：01-stage-kernel 的 10 篇主文档及其关联代码 | 每篇循环 |
| `R01`–`R12` | 历史回归切片：从既有评审中抢救出的具体缺陷 | 每个版本边界 |
| `T01`–`T08` | 对抗样本：合成阳性、阴性、范围陷阱 | 每个版本边界 |

每条必填：ID、类型、制品路径、形态、关联代码、期望发现、负期望、成本预算、上次结果。期望发现引用 `BLIND-SPOTS.md` 的盲区编号；负期望写“不许报什么”。

## 二、真实篇目 D01–D10

关联代码数量由 `tools/doc-code-map.sh <doc> --format=plain | wc -l` 在基线 `e364d5f92` 上实测。

| ID | 文档 | 行数 | 形态 | 关联代码 | 历史读数 | 已知缺陷主题 | 期望探测器 |
|---|---|---:|---|---:|---|---|---|
| D01 | `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 3086 | 启动链 / boot shim / 三架构 | 44 | DOC-ITER 6 轮；zcode R1 14 P0、32 P1、69 P2；R2 新增 4 P1、9 P2 | 生产路径写错（`UefiBootShim::prepare_boot` 对 `prepare_and_enter_kernel_image`）、OpenSBI 调用方、`build_kernel_info` 参数个数、测试计数、引文保真 | P-FACT + P-READER + P-DESIGN |
| D02 | `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 1267 | 地址空间 / higher-half | 26 | DOC-ITER 2 轮；zcode R1 11 P0、47 P1、13 P2；最终 0 P0、0 P1、4 P2 | 段归属断言、riscv64 PC 判据、生产 kmain 入口断言 | P-FACT + P-READER + P-DESIGN |
| D03 | `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 1375 | 启动链 / C 入口 / trap entry | 21 | DOC-ITER 2 轮；zcode 6 P0、24 P1、14 P2；最终 0/0/0 | `Tss64` 字段行、riscv64 两条腿、陷阱入口、自引入“腿” | P-FACT + P-READER + P-DESIGN |
| D04 | `rewrite-notes/01-stage-kernel/04-platform-discovery.md` | 2682 | 平台发现 / 多架构 | 51 | DOC-ITER 2 轮；zcode 最终 0/0/0，但 R01 66 行台账、69 处编辑 | `vm_running` 极性、`sdt_count` 无符号提升、ACLINT 与 aarch64 DTB 载体、端口义 | P-FACT + P-READER + P-DESIGN |
| D05 | `rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` | 2196 | 时钟与中断 / 三架构 | 32 | DOC-ITER 2 轮；zcode R1 11 P0、13 P1、8 P2；最终 0/0/0 | PIC 从片 `0x70`、aarch64 空链回执、位级寄存器、用户态 tick、风暴注入 | P-FACT + P-READER + P-DESIGN |
| D06 | `rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md` | 2976 | 进程初始化 / boot proc | 23 | DOC-ITER 2 轮；R01 三项结构残余、R02 回读 13 处修复 | 进程槽状态、boot proc 身份、Ch5 改造、结构残余 | P-FACT + P-READER + P-DESIGN |
| D07 | `rewrite-notes/01-stage-kernel/07-cross-space-init.md` | 1322 | 地址空间切换 / direct map | 10 | DOC-ITER 2 轮；R01 27 行台账、24 行修复 | 旧注释与偏离登记双过期、riscv64 双错、`Called-once` 过期、`setcr3` 极性 | P-FACT + P-READER + P-DESIGN |
| D08 | `rewrite-notes/01-stage-kernel/08-system-init-boot-finish.md` | 2230 | 系统初始化收尾 | 15 | DOC-ITER 2 轮；R01 26 行台账、42 处编辑 | fpu 第 12 位极性、三处重排编号、十二拍口径、C 行号族、const 断言端点、release 丢弃静默、oxpcie 极性 | P-FACT + P-READER + P-DESIGN |
| D09 | `rewrite-notes/01-stage-kernel/09-vm-boot-protocol.md` | 2151 | VM 启动协议 / 跨模块 | 11 | DOC-ITER 2 轮；R01 20 行台账、19 处编辑 | 20→64 页、127→128、`res_rejected` 死值、调度器判据、EINVAL/ENOSYS 归属、bootmark、只读→可写、VMSUSPEND 归属、五族口径 | P-FACT + P-READER + P-DESIGN |
| D10 | `rewrite-notes/01-stage-kernel/10-switch-to-user.md` | 1708 | 陷入返回 / 切换到用户 | 15 | DOC-ITER 2 轮；R01 63 行台账、80 余处编辑 | §6.2 Stage 行号 +49、831→832、BKL 取锁点在 `context_stop` 而非 `mpx.S`、记录位现状 | P-FACT + P-READER + P-DESIGN |

## 三、历史回归切片 R01–R12

这些切片来自已经发生过的真实缺陷，用来验证“工作流改版后还能不能重新发现”。切片不是全文重跑；每条只要求定位、取证、给处置。

| ID | 切片 | 历史证据 | 期望发现 | 负期望 |
|---|---|---|---|---|
| R01 | 01 篇 §5.2.1 测试计数与实测不符 | `rewrite-notes/01-stage-kernel/01-review-trajectory.md` §1 | P-FACT 重跑测试计数，报读数漂移；命中 B-08 | 不许把“数字看起来自洽”当通过 |
| R02 | 01 篇把 `UefiBootShim::prepare_boot` 写成 x86_64 生产路径 | 同 R01 | P-FACT 对照 `main.rs:65`，报文档与代码路径不一致；命中 B-08 | 不许只改措辞而不核对调用方 |
| R03 | 01 篇把 `build_kernel_info` 写成 9 参，实际 10 参 | 同 R01 | P-FACT 核对签名；命中 B-08 | 不许用旧注释当证据 |
| R04 | 01 篇称 OpenSBI shim 无调用方，实际 `uboot_main.rs:333` 调用 | 同 R01 | P-FACT 追踪调用方；命中 B-08 | 不许只查 `rg` 名称命中就下结论 |
| R05 | 10 篇测试计数 831 对 832 | `rewrite-notes/coordination/DOC-ITER-LEDGER.md` 的 10 篇 R01 行 | P-FACT 重跑 `cargo test`；命中 B-08 | 不许把上一轮读数直接抄过来 |
| R06 | 14 篇引文块被 doc-gen 改写，声明逐字 | 同台账 14 篇 R01 行 | P-FACT 逐块 diff；命中 B-12 | 不许只看标签不看内容 |
| R07 | 01 篇 v11 的三处跨节矛盾 | `prompt/WORKFLOW-OPEN-ITEMS.md:289-297` | P-READER C 逐条追复述；命中 B-03 | 不许凭通读印象说“没有矛盾” |
| R08 | 01 篇 Ch1–Ch2 的八处读者理解障碍 | `prompt/WORKFLOW-OPEN-ITEMS.md:43-49` | P-READER A 报卡点与前置缺失；命中 B-04 | 不许把“作者知道”当成“读者应该知道” |
| R09 | 7 篇文档中 341 处 Rust 带 kind 前缀符号链接渲染失败 | `prompt/WORKFLOW-OPEN-ITEMS.md:252-264` | P-FACT 检查链接目标是否含空格；命中 B-06 | 不许只检查锚点能否解析 |
| R10 | 架构移植义的“端口”误译 | `prompt/WORKFLOW-OPEN-ITEMS.md:73-81` | P-READER A/P-FACT 按词义消歧；命中 B-07 | 不许全局替换网络端口义 |
| R11 | 不存在的 ARM32 反事实被写成对照 | `prompt/WORKFLOW-OPEN-ITEMS.md:61-71` | P-FACT 先证伪前提；命中 B-05 | 不许因为标签格式正确就算通过 |
| R12 | 正文出现 `I-NN`/`SD-NN` 流程编号 | `prompt/WORKFLOW-OPEN-ITEMS.md:266-273` | P-FACT 扫描并报流程痕迹；命中 B-09 | 不许把编号当普通文字放过 |

## 四、对抗样本 T01–T08

样本放在 `traps/`，期望判定放在 `expected/`；两者物理分离，审查者不得读 `expected/`。

| ID | 样本 | 类型 | 期望发现 | 负期望 |
|---|---|---|---|---|
| T01 | `traps/T01-translation-residue.rs` | 阳性 | C 形状的进程结构、单实现 trait、非法状态可表示；命中 B-01 | 不许只说“命名不好” |
| T02 | `traps/T02-dead-abstraction.rs` | 阳性 | 单实现、单调用、没有语义差异的 trait；命中 B-02 | 不许机械地要求删除；必须问“是否测试接缝” |
| T03 | `traps/T03-arch-leak.rs` | 阳性 | 共享层出现 `init_tss`、`cr3` 与内联汇编；命中 B-13 | 不许把它降级成命名问题 |
| T04 | `traps/T04-duplicate-state.rs` | 阳性 | `present` 与 `state` 表达同一事实；命中 B-01 | 不许只改字段名 |
| T05 | `traps/T05-sentinel-error.rs` | 阳性 | `-1` 与 `0` 哨兵代替 `Result`/`Option`；命中 B-01 | 不许保留哨兵只加注释 |
| T06 | `traps/T06-doc-incomprehensible.md` | 阳性 | 未定义术语、前置缺失、结论先于证据；命中 B-04 | 不许只提“可以写得更清楚” |
| T07 | `traps/T07-frozen-decision.md` | 阴性 | 正确行为是不报：四章架构属于 D-001 冻结决策；命中 B-10 | 不许提出重排章节 |
| T08 | `traps/T08-adjacent-scope.md` | 阴性 | 正确行为是登记为 follow-up，不作为当前阻塞；命中 B-11 | 不许把相邻模块问题升级为 P0 |

## 五、成本预算

| 场景 | 预算 |
|---|---|
| 单篇循环 | 面板 5–8 个子 agent；主 agent 修复轮最多 2 轮；meta-review 1 个 |
| 版本边界 | 陷阱 8 个 + 回归切片 12 条；meta-review 1 份汇总报告 |
| 超出预算 | 必须在 `loop.md §8` 写明原因与收益，否则版本边界不允许升版 |

## 六、上次运行结果

| 批次 | 版本 | 结果 | 备注 |
|---|---|---|---|
| 基线脚手架 | V1 | 未运行 | Phase 0 建立语料，Phase 1 首跑 D01 |
