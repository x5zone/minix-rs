# 05-review-trajectory — 时钟与中断初始化 Review 轨迹

- **对象**：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
- **基线哈希**：sha256 `8cacc3c86db660b06a3986c1bc15f241c992d3e77e0cda6245c571aae96de7fe`
- **基线 HEAD**：`2320f7959a63d80a54dc80c932248856eea03c7e`
- **性质**：只审不修；所有原始命令与输出在 `.review/zcode/01-stage-kernel/05-clock-interrupt-init/trace/logs/`
- **R1 判定**：NOT CONVERGED（P0=11 open；Gate E/H FAIL；Gate G CONCERN）；R2 修复后见 §8

## 1. 阶段时间线

| 时间 | 阶段 | 动作 | 产出/判定 |
|---|---|---|---|
| 22:59 | R0 | 冻结 HEAD/工作树/文档哈希；跑锚点、doc-code-map、doc-ref、density、style | 108/108、28/28、PASS、PASS、error=0 |
| 22:59 | R0 | 盘点 v2 design / v3 outline 与历史线索 | 发现 v2 说 `boot_init_timer` 已删除，与当前代码冲突 |
| 23:00 | R1.1 | 读 `ClockArch`/`PerCpu`/`TIMER_IRQ`/`boot_init_timer`/`init_clock_and_interrupts` | 候选：12/5/4/3 条 `use`、五步 vs 实际注入 |
| 23:01 | R1.1 | 读 spurious 路径、载体、测试 | 候选：空链未 complete、riscv FAIL 分支 |
| 23:03 | R1.2 | 对 6 处“按原文”引文做 diff | 6 处漂移 |
| 23:04 | R1.3 | C ground truth：cstart/init_clock/intr_init/arch_init/boot_cpu_init_timer | C 侧断言基本正确 |
| 23:05 | R1.4 | 测试计数与实跑 | kernel 856、arch 289、plat 7；pin 3/4/3 |
| 23:07 | R1.5 | 发现 PIC 从片 ICW2 0x58 vs IDT 0x70 | 升级 P0-code-bug-01 |
| 23:10 | R0.5 | 重推 `05-outline.v4.md` / `05-outline-review.v4.md` / `05-design.v3.md` | Gate H.1/H.2 PASS；H.3/H.6 FAIL |
| 23:20 | R2 | 启动学生冷读、结构冷读、独立 CodeReview 子代理 | 分别返回 5/6/10 项发现 |
| 23:30 | R3 | 汇总 Issue List；写 scan/structure/test-audit/excellence | 33 条 issue |
| 23:41 | R3 | `verify-check.py` 抽样 16 条；人工重判 | 16/16 = 100%；Gate G CONCERN |
| 23:50 | 交付 | 写 `05-review.md` + 轨迹；更新 STATE 05 行 | NOT CONVERGED |

## 2. 关键命令（按阶段）

### R0 基线

```bash
git rev-parse HEAD
git status --short
wc -lc rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
sha256sum rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
ls rewrite-notes/01-stage-kernel/.design/05-*.v*.md
bash tools/design-coverage-check.sh 01-stage-kernel
```

### R1 机械门

```bash
bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md --check
AWK=gawk bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-code-density-check.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md
```

### R1 代码/测试

```bash
cd os
cargo test -p minix-kernel --features mock --lib
cargo test -p minix-arch --features runtime-window --lib
cargo test -p minix-plat --lib
cargo test -p minix-arch --test arm64_arch_init_pin
cargo test -p minix-arch --test riscv64_arch_init_pin
cargo test -p minix-arch --test wake_timer_pin
cargo check -p minix-kernel --features mock
```

### R1 关键取证

```bash
nl -ba os/plat/src/x86_64/interrupt.rs | sed -n '63,100p'
nl -ba os/arch/src/x86_64/trap_entry.rs | sed -n '236,246p'
nl -ba os/arch/src/x86_64/trap_stub.rs | sed -n '188,205p'
nl -ba os/arch/src/arch/clock.rs | sed -n '95,175p'
nl -ba os/plat/src/interrupt.rs | sed -n '145,245p'
nl -ba os/arch/src/arm64/arch_init.rs | sed -n '38,100p'
nl -ba os/plat/src/riscv64/interrupt.rs | sed -n '250,305p'
nl -ba os/kernel/src/lib.rs | sed -n '2150,2285p'
nl -ba os/kernel/src/lib.rs | sed -n '3435,3520p'
nl -ba os/kernel/src/irq_manager.rs | sed -n '726,792p'
nl -ba os/kernel/src/trap_dispatch.rs | sed -n '2600,2618p'
nl -ba os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq-riscv64/src/main.rs | sed -n '330,375p'
```

## 3. 关键改判/调和

| 项 | 初判 | 终判 | 原因 |
|---|---|---|---|
| `ClockArch` 方法数 | P1（可能只是漏一个方法） | P0-fact-01 | 文档明确写“十一个方法”，且漏的 `restart_local_timer` 有 `wake_timer_pin` 测试；属事实错误 |
| `init_clock_and_interrupts` 五步 | P1（覆盖不全） | P0-fact-06 | 省略号藏的是 PD-22 check-and-reject 与 aarch64 SGI，不是注释；安全论证被破坏 |
| aarch64 空链 claim | P1（潜在） | P0-code-bug-02 | 违反 claim/complete 配对协议；SGI 已特判，其他 INTID 无保护 |
| PIC 从片 0x58 | P0-code-bug | P0-code-bug | 从片线当前全屏蔽，属潜在缺陷；但函数契约与 IDT 直接冲突，保留 P0 |
| 引文漂移 6 处 | 6 个 P0-fact | 1 个 P0-fact 组（P0-fact-07） | 同一根因“历史注释已清理”；修复时逐块重录 |
| `pic_init` “before init_timer”注释 | P2 | P1-code-comment-01 | 与调用点直接矛盾，且文档 §5.4 未披露 |
| Gate G | PASS（16/16） | CONCERN | Gate E/H 本身 FAIL，不能判 PASS；一致性 100% 只代表抽样 |

## 4. 对抗性装置记录

### 4.1 学生型冷读

- 只给文档，不给源码/规则/主会话结论。
- 独立发现：0/2 三重编号；§1.2 “每条线屏蔽”与主片 IRQ2 矛盾；§2.6 “两个动作/三段”；PIT/LAPIC 分叉；D-59/D-10/K8 未解码。
- 采纳：三重编号（P2-doc-02）、全屏蔽/IRQ2（P2-doc-01）、两个动作/三段（P2-fact-03）、PIT/LAPIC（P2-doc-02）、编号未解码（P2-fact-05 的扩展）。

### 4.2 结构编辑冷读

- 独立发现：§3.7/§4.7 重复；`system_hz` 三写；三架构对照碎成四片；结尾无收束；§5.3 是 §5.1 倒排。
- 采纳：P1-struct-01、P2-fact-02/03、结构建议写入 `structure.md`。

### 4.3 独立 CodeReview

- 给文档 + 允许源文件清单，不给主会话结论。
- 独立发现 10 项：方法数、§3.3 方法集、§4.1 注入、§4.6 三动作、riscv claim 委托、PIC 0x58、riscv 载体 FAIL、§5.1 漏 12 pin、`pic_init` 注释矛盾、空链 claim 未回执。
- 主会话重放 10 项，全部采纳；其中 `arch_small_files_audit` 2 条 pin 是新增覆盖，并入 P1-fact-05。

### 4.4 负抽样

- 抽 §6.1（向量算式）、§6.2（命令 3 的 9 行）、§7（链接）。
- 结果：全部正确，无新缺陷；未触发 DRAFT。

## 5. 工具/环境异常

| 项 | 现象 | 处置 |
|---|---|---|
| `rg` 不支持 lookbehind | Sync-6 的正则报错 | 改用 `grep -P` 并人工判断命令块内的 `cd os` 相对路径 |
| `cargo check` 输出含 E0133 文本 | 但 exit 0、Finished | 复核无 error；记为既有 warning，不作门 |
| `verify-check.py` 首跑 Gate A FAIL | gate block 缺 `Coverage Summary` 字样 | 补关键字后复跑 exit 0 |
| 旧 design v2 与代码冲突 | 说 `boot_init_timer` 已删除 | 重推 v3；v2 标 SUPERSEDED，不作 Gate H 依据 |
| `STATE.md` 被并行会话更新 | 04 行出现 | 只追加 05 行/节，不覆盖 01–04 数据 |

## 6. 耗时与成本（粗记）

| 阶段 | 时长（约） | 说明 |
|---|---|---|
| R0 | 10 min | 基线 + 机械门 |
| R0.5 | 75 min | 设计快照重推 |
| R1 | 110 min | C/Rust 对账、测试、代码维度 |
| R2 | 40 min | 冷读团/学生/独立 CodeReview |
| R3 | 45 min | Issue List、Gate G、报告 |
| 合计 | 约 4.5 h | 含子代理等待；未跑 QEMU |

## 7. R1 未做/下一轮

- R1 未修任何 P0/P1/P2；只审不修。
- R1 未跑 QEMU；载体结论为静态审计。R2 已跑三架构，见 §8。
- 未做跨 agent 全量复核；Gate G 只做同 agent 抽样 + 命令重放。
- R1 下一轮建议已由 R2 执行：先修 P0-code-bug-01/02 + 补测试，再修 P0-fact-01..07，最后跑第二轮 full review 与 QEMU。
- R2 后仍保留 R05-T01/T05/T06 测试深度 backlog。

---

**轨迹状态**：R1 完结；R2 修复与回归见下节。报告 `05-review.md` 与轨迹绑定同一基线哈希。

---

## 8. R2 修复与回归（2026-11-03）

| 时间 | 阶段 | 动作 | 结果 |
|---|---|---|---|
| 00:00 | R2-0 | 冻结 R1 哈希/HEAD；逐条建修复台账 | 11 P0 / 13 P1 / 8 P2 |
| 00:05 | R2-1 | 修 PIC 从片 ICW2 0x58→`IRQ8_VECTOR=0x70`；加 host 测试 | `minix-plat` 8 passed |
| 00:15 | R2-2 | 修 `dispatch_claimed` 空链 `Some` 回执；加 Mock 测试 | `irq_manager` 10 passed；kernel 858 |
| 00:25 | R2-3 | 修文档 7 类 P0-fact：12 方法、aarch64 三动作、riscv claim、riscv FAIL、0x70、§4.1 2a/2b/2c、6 处引文 | 旧串 grep 全 0 |
| 00:45 | R2-4 | 修 13 个 P1 与 8 个 P2：方法集、`use` 计数、pin 表、注释债务、结构重复、口径 | doc-style error=0 |
| 01:00 | R2-5 | 机械门复跑：锚点/映射/引用/密度/风格/覆盖率 | 106/106、32/32、PASS、PASS、0、99/151 |
| 01:10 | R2-6 | 宿主测试：kernel/arch/plat/pin | 858/289/8/3/4/3/5，0 failed |
| 01:20 | R2-7 | 构建并跑三架构 QEMU `test-timer-irq` | x86/aarch64/riscv 全 PASS |
| 01:35 | R2-8 | 写 §17、`VERIFY-CHECK-R2`、更新 scan/STATE | 12/12=100%；CONVERGED_WITH_WARNING_BACKLOG |

### R2 关键命令

```bash
cd os
cargo test -p minix-kernel --features mock --lib
cargo test -p minix-arch --features runtime-window --lib
cargo test -p minix-plat --lib
cargo test -p minix-arch --test arm64_arch_init_pin
cargo test -p minix-arch --test riscv64_arch_init_pin
cargo test -p minix-arch --test wake_timer_pin
cargo test -p minix-arch --test arch_small_files_audit
cargo build -p test-timer-irq --target x86_64-unknown-uefi --features fw-x86-uefi --release
cargo build -p test-timer-irq-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi --release
cargo build -p test-timer-irq-riscv64 --target riscv64gc-unknown-none-elf --features fw-riscv64-none --release
bash qemu-tests/run_qemu.sh x86_64 target/x86_64-unknown-uefi/release/test-timer-irq.efi
bash qemu-tests/test-timer-irq-aarch64.sh
bash qemu-tests/test-timer-irq-riscv64.sh
```

### R2 改判/意外

- `pic_init` 从片向量修复后，x86 QEMU 仍 PASS；该缺陷在从片线全屏蔽时是潜在的，载体不能证明其触发，只能证明主片/IOAPIC/PIT 链未回退。
- `dispatch_claimed` 修复后，kernel 测试从 856 增至 858；`minix-plat` 从 7 增至 8。
- 修复过程中修掉 4 处未登记的注释债务（`read_tsc`、`intr_init_8254`、`boot_init_timer` 的 IOAPIC 引脚、riscv `TimerIrqGate` 的 MMIO/SBI 表述）。
- 三架构 QEMU 均 PASS，但位级寄存器、用户态 tick、风暴注入仍未证；R05-T01/T05/T06 保留 backlog。

### R2 最终判定

- P0=0、P1=0、P2=0 open；Layer 1 PASS；Layer 2 PASS_WITH_BACKLOG；文档 **CONVERGED_WITH_WARNING_BACKLOG**。
