# NK4-B qwen 接手开局 Prompt（复制本文件分隔线之后全文给 agent）

---

你在 /home/xzhao/github/minix-rs 工作（rewrite 分支）。上一个会话完成了
NK4-A（x86_64 生产启动链）的执行阶段；你接手后续弧线 **NK4-B：三架构
（x86_64/aarch64/riscv64）都能运行起 OS，然后 18-stage-commands 中的
命令都能在 OS 上运行**。你的能力有限制，本 prompt 与任务书把工作拆成
小粒度里程碑：**严格按序执行，每步完成后再进下一步，不要跳步，不要
自由发挥**。

## 0. 开局动作（按序，缺一不可）

1. 读 `notes/rewrite/fork-syscall-rewrite/NK4B-TODO.md` 全文（本弧线的
   任务书：Phase/Milestone 分解、判据、已知架构事实、记录格式、铁律）。
2. 读 `notes/rewrite/fork-syscall-rewrite/NK4A-TODO.md` 的 §5（13 条技术
   陷阱）、§6（记录格式）、§7（常用命令）、§8（取证循环）——全部继续
   有效。
3. 读 `.review/zcode/edge1/FIXLOG.md` 最后 300 行（前人修复链与记录
   风格，你的修复按此记录）。
4. **P0 核账**（NK4B-TODO §1，强制）：git 现场抄录 → 宿主六包计数实测 →
   x86_64 冒烟实跑记录（exit code + 串口到达序列）→ 前棒 WORKLOG/FIXLOG
   的事实提炼（**只记事实，不写评价**）→ 判定 P1 是否跳过。
5. 在 `notes/rewrite/fork-syscall-rewrite/NK4B-WORKLOG.md` 写会话开场，
   然后按核账结果进入 P1 或 P2。

## 1. 总目标与停止点

- 终目标：三架构各自的 rc marker（`minix-rs rc: minimal boot script
  marker`）+ 命令冒烟全绿 + run_all 接线。
- 每个 Phase 结束 = 一次汇报点。**probe 去留裁决与账本 ✅ 不是你的
  工作**；架构级裁决点写 WORKLOG「上交裁决」，不自行定案。
- 单个问题尝试 3 轮仍无定性 → WORKLOG 写 BLOCKED（含证据与已试方案），
  转下一个独立任务；不许用 stub/删检查制造完成。

## 2. 铁律（浓缩版；全文见 NK4B-TODO §8 与 NK4A-TODO §5）

1. fix-guard：修前读 ±5 行 + grep；一次一修；修后 grep 验证。
2. 一逻辑单元一 commit；宿主测试全绿才 commit；计数以你 P0 实测为基线
   只许涨。
3. 修 bug 必须对照 C（`minix3/minix/`）或仓内既有三架构实现写明对位。
4. FIXLOG 每修复一条（含「测试（防回归）」判别性断言小节）；WORKLOG
   每 Task/Milestone 一节。
5. 探针限次 + `#[cfg(not(feature = "mock"))]` + 注明「task1-close 裁决
   删除」；不删既有探针与 FIXLOG 历史。
6. 禁止：push/reset/rebase/force-push；动 `minix3/`、`nk4a-agent-wip`、
   `nk4a-review-docs`；整仓 cargo fmt。
7. `os/etc/rc` 只允许在 marker 行之后追加；marker 行及之前一个字节不许动。
   冒烟脚本只许加判据，不许放松既有判据。
8. IMG-EXIT=0 才跑 QEMU；起跑前 `pkill -f '[q]emu-system'`（方括号）。
9. 修复验证 = 真机两次独立复跑（时序敏感）。
10. 证据归档 `evidence/<日期>-nk4b-<说明>/` + `git add -f`（*.log 被忽略）。

## 3. 关键环境事实（省你摸索）

- 宿主测试：`cd os && docker run --rm -v "$PWD:/work" -w /work -m 2g
  minix-ci:1.94 cargo test -j 1 -p <pkg>`；
- 镜像组装（x86_64）：`(ulimit -v 3145728; cargo run -q -p xtask --
  image --arch x86_64 --release)`；uefi/none 构建走宿主 ulimit；
- aarch64：`aarch64-unknown-none`/`aarch64-unknown-uefi` target 宿主已装；
  AAVMF 载体必须 `gic-version=3`（抄
  `os/qemu-tests/test-timer-irq-aarch64.sh` 的 qemu 参数）；
- riscv64：`riscv64gc-unknown-none-elf`；载体
  `test-riscv64-uboot.sh`/`test-timer-irq-riscv64.sh` 现成；
- RS/命令模块 ELF 可离线符号化（vaddr=执行视图）；内核本体不可；
- 常用命令全文见 NK4A-TODO §7。

## 4. 执行顺序

P0 核账 →（P1 x86_64 rc marker，若未达）→ P2 x86_64 命令面 →
P3 aarch64（M3.1→M3.6）→ P4 riscv64（M4.1→M4.5）→ P5 三架构命令面 →
P6 run_all 接线。每个 Phase/里程碑的判据、步骤、已知架构事实见
NK4B-TODO §1-§6。**上下文若逼近耗尽：先写完 WORKLOG/FIXLOG 与最终
汇报再收尾**——记录完整性优先于多做一个里程碑。

## 5. 会话结束交付检查

- [ ] WORKLOG 按 Task/Milestone 逐节，状态不撒谎（DONE/BLOCKED/PARTIAL）；
- [ ] FIXLOG 每修复一条（含测试判别小节）；
- [ ] commit 全部本地（未 push）、一逻辑单元一 commit；
- [ ] 真机证据已 `git add -f` 归档并在 WORKLOG 引用文件名；
- [ ] 最终汇报：Phase/Milestone 粒度的 状态/根因/修法/commit/证据行/
      上交裁决项/下一步建议。

---

分隔线之前是给用户的使用说明，不用粘贴。
