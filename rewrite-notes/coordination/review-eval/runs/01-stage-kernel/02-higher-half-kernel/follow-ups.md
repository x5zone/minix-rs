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
| D02-F11 | DESIGN-12 | `check-layout.sh` L9 只对 riscv64 做等值对账；aarch64 `bootface_a64.rs` 的 `KERN_SIZE` 只有上界断言 | code/test batch：L9 按架构选文件，riscv64 读 `bootface.rs`、aarch64 读 `bootface_a64.rs` |
| D02-F12 | 复验 | `cargo test -p minix-arch` 的测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |
