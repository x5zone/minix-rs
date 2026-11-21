# D02 follow-ups —— 本轮转出项与去处

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D02-F01 | DESIGN-03 | `jump_to_kmain(kinfo, stack_top)` 允许 `stack_top != kinfo.kern_stack_top`；删第二个参数或加 `debug_assert_eq!` | code batch；改动 3 个 impl + 4 个调用点 |
| D02-F02 | DESIGN-04 | `kern_stack_top == kern_virt_base + kern_size` 没有被校验；在 `check_boot_postconditions` 加等式或类型化 `KernelSpan` | 用户裁决（涉及跨镜像结构与 70+ 处夹具） |
| D02-F03 | DESIGN-05 | `os/kernel/src/arch/*/link.ld` 无生产链接读者，且与生产脚本有四处不等价（x86_64 首段 VMA 低 2 MiB、无 2 MiB 收口、保留 `k_initial_stktop`、缺收段） | 用户确认仓外无消费者后删除；否则修正并让锁步测试比较首段 VMA 与跨距 |
| D02-F04 | DESIGN-06 | `load_segments_into_buffer` 与 `load_segments_into_phys_memory` 是两份循环，被测的是缓冲区版，生产走物理地址版 | code batch：抽同一区间计算纯函数，两版共用 |
| D02-F05 | DESIGN-07 | `load_segments_into_phys_memory` 依赖调用者先跑 `compute_kernel_layout`，类型上不成立 | code batch：解析产出 `KernelImage`，摆放只接受已验证值 |
| D02-F06 | DESIGN-08 | 2 MiB 收口只有脚本检查与启动期 panic；三份生产链接脚本可加 `ASSERT((__bss_end - BASE) % 0x200000 == 0, ...)` | code batch（低风险，需一次构建确认 lld 求值次序） |
| D02-F07 | DESIGN-11 | `arch_boot_resume_high_half` / `resume_after_v1_decode` 三份几乎逐行相同 | code batch：收成泛型 `resume<A: BootArch, H: HigherHalf>`，三处 cfg 只选类型与打印 |
| D02-F08 | DESIGN-13 | `bootface.rs` 与 `bootface_a64.rs` 流程同构，`dtb_bounded`/`place_modules`/`say`/静态对象各有两份 | 用户裁决；代码侧抽共享流程，两个薄壳只给常量与入口参数 |
| D02-F09 | DESIGN-10/15 | `Paging`/`HugePages`/`BootArch` 保留；翻译残留分类清单作为 code-excellence 输入 | 无动作（拒绝项/分类项） |
| D02-F10 | READER-A/B/C 转出项 | 读者路径反例、§2.5 差异清单、§3.4 契约表、§5.4 三表一致性、gdb 命令、`delta` 角色互换等 36 条 | 下一篇闭环轮逐条复核；按下面三组分别处理，不合并成一个大桶 |
| D02-F10a | READER-A 10 条 | A-01/02/03/04/05/06/07/09/11/12：反例不可达、`push 0`、四条前置条件、gdb 命令、载荷存活、保护结构状态 | 闭环轮；优先 A-01/A-02（反例不可达）与 A-05（载荷存活表述） |
| D02-F10b | READER-B 13 条 | B-01/02/03/05/06/07/08/09/10/11/12/13/14：职责重叠、外部事实未锚、标题与正文、容量段、重复句 | 教学债批次；优先 B-01/B-03/B-07 |
| D02-F10c | READER-C 11 条 | C-04/06/07/08/09/11/12/13/14/16：悬空反例、两套机制表、四条路径、panic 前缀、外部符号、角色互换、2 MiB 档 | 闭环轮；优先 C-04/C-08/C-11 |
| D02-F11 | DESIGN-12 | `check-layout.sh` L9 只对 riscv64 做等值对账；aarch64 `bootface_a64.rs` 的 `KERN_SIZE` 只有上界断言 | ✅ CLOSED（按 `D-107`）：L9 按架构选文件，三架构 `check-layout.sh all` PASS |
| D02-F12 | 复验 | `cargo test -p minix-arch` 的测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |

## R1 设计 P3（suggestions，不阻断）

> 设计角色 P3 按 §4.0c 分流；不实施，下一 edition 边界按频次评估。

| ID | 摘要 | 处置 |
|---|---|---|
| RD1-DESIGN-D2-07 | - 位置：`/home/xzhao/github/minix-rs/os/kernel/src/boot/higher_half.rs:40-48`；文档 §3.4 `:562-568` | 不实施（建议级） |
| RD1-DESIGN-D2-08 | - 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:6065-6099`；`os/kernel/src/boot/higher_half.rs:21-30` | 不实施（建议级） |
| RD1-DESIGN-D2-09 | - 位置：`/home/xzhao/github/minix-rs/os/kernel/src/boot_policy.rs:107-120`；`os/arch/src/arch/boot.rs:461-464`、`734` | 不实施（建议级） |
| RD1-DESIGN-D2-10 | - 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:236-300`、`785-795` | 不实施（建议级） |
| RD1-DESIGN-D2-11 | - 位置：`/home/xzhao/github/minix-rs/os/kernel/src/arch/x86_64/link.ld`、`aarch64/link.ld`、`riscv64/link.ld`（三份均不在 27 文件内，仅按文档与测试引用判断）；`/home/xzhao/github/minix-rs/ | 不实施（建议级） |
| RD1-DESIGN-D2-12 | - 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:119-121`、`679-695`、`467-481`；`os/boot-shim/src/loader.rs:519-560` | 不实施（建议级） |
| RD1-DESIGN-D2-15 | - 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:187-266`；`os/kernel/src/lib.rs:217-233`；`os/kernel-image/src/bootface.rs:440`；测试载体 `os/q | 不实施（建议级） |

## 本次状态核实（D-107 清零口径）

| ID | 当前状态 | 说明 |
|---|---|---|
| D02-F01 | OPEN | `jump_to_kmain(kinfo, stack_top)` 仍保留冗余形参；清零批删除第二个参数并改 3 impl/4 调用点 |
| D02-F02 | CLOSED（对象消失） | wire v10 / D-062 删除 `kern_stack_top` 字段，等式校验不再有 wire 对象 |
| D02-F03 | 并入 R4 D2-06 | 三份 arch `link.ld` 按 D-107 删除，连同 `link_ld_lockstep.rs` |
| D02-F04 | OPEN | buffer/phys 两版循环仍各写一份；清零批抽共享区间纯函数 |
| D02-F05 | OPEN | 物理装载仍无 `KernelImage`/已验证类型前置；清零批引入已验证值对象 |
| D02-F06 | OPEN | x86 生产链接脚本仍无 2 MiB 收口 `ASSERT`；清零批补并跑 `check-layout all` |
| D02-F07 | CLOSED | `resume_additive` 已把三条 additive-resume 收成泛型 |
| D02-F08 | OPEN（用户裁决：抽共享） | 抽 bootface 共享流程，两薄壳只留常量/入口；D-107 |
| D02-F09 | WONTFIX（拒绝项/分类项） | 保留 `Paging`/`HugePages`/`BootArch`，翻译残留清单作为 code-excellence 输入 |
| D02-F10 | OPEN（用户裁决：全部实现） | 读者 36 条全部实现；D-107 取消“建议不实施”出口 |
| D02-F11 | CLOSED | 与 D01 `F-R34-01` 同项已实施：L9 三架构等值对账；证据 `bash os/kernel-image/check-layout.sh all` → 三条 L9 PASS |
| D02-F12 | CLOSED | `cargo test -p minix-arch` 已由 D03 的测试卫生批修好（266/0） |

### D02 §5.3 测试缺口（R02-T）

| ID | 当前状态 | 验收 |
|---|---|---|
| R02-T01 | OPEN | 段归属 / `k_initial_stktop` 落段的机械断言或静态检查 |
| R02-T02 | CLOSED | `link_ld_lockstep.rs` 已接住；按 D-107 将随 D2-06 删除，删除后由生产脚本门替代 |
| R02-T03 | OPEN | riscv64 载体 PC 高半判据 + `delta` 路径断言 |
| R02-T04 | OPEN | 生产 `kmain` 入口 ABI 栈形的架构相关断言 |
