# NK4-B TODO — 三架构启动 + 18-stage-commands 命令面（qwen 任务书，2026-09-22）

> 接续 NK4-A（x86_64 生产启动链，迭代18 后移交执行）。本任务书配合同目录
> `NK4B-OPENING-PROMPT.md` 使用。维护纪律：每完成一个 Task 打勾并注明
> commit；逐任务工作记录写 `NK4B-WORKLOG.md`（模板 §6）。
> 前一份任务书 `NK4A-TODO.md` 的 §5 技术陷阱 / §6 记录格式 / §7 命令 /
> §8 取证循环 **全部继续适用**，本文件只写增量。

---

## §0 总目标与里程碑总览

总目标（用户原话）：**三架构都能运行起 OS，然后 18-stage-commands 中的
命令都能在 OS 上运行。**

| Phase | 里程碑 | 退出判据（可客观验证） |
|-------|--------|------------------------|
| P0 | 核账 | WORKLOG 有事实基线表（不评价前棒） |
| P1 | x86_64 rc marker | 串口 `minix-rs rc: minimal boot script marker`（若 P0 已达成则跳过） |
| P2 | x86_64 命令面 | rc 在 marker 后真实执行命令且判据通过（冒烟绿） |
| P3 | aarch64 生产链 | M3.1→M3.6 逐里程碑，终判=aarch64 rc marker |
| P4 | riscv64 生产链 | M4.1→M4.5 逐里程碑，终判=riscv64 rc marker |
| P5 | 三架构命令面 | 三架构各出一套命令冒烟判据并全绿 |
| P6 | run_all 接线 | 三架构生产链冒烟入 `os/qemu-tests/run_all.sh` |

**停止点纪律**：每个 Phase 结束 = 一次汇报点（WORKLOG 写 Phase 完成节 +
最终汇报）；probe 去留裁决与账本 ✅ 仍是评审方职责，不许做。

---

## §1 P0 核账（强制第一步；只记录事实，不评价前棒）

- [x] T0.1 `git log --oneline -15` + `git status --short` 原样抄进 WORKLOG。
- [x] T0.2 宿主六包基线实测（命令见 NK4A-TODO §7），计数抄进 WORKLOG
      （kernel/arch/vm/rs/rt/sys；允许与 808/241/525/350/57/315 不同，
      如实记录即可——后续只要求"不减"以你实测的数为基线）。
- [x] T0.3 跑一次 x86_64 冒烟：
      `cd /home/xzhao/github/minix-rs && SMOKE_SKIP_BOOT=0 bash os/qemu-tests/test-cmd-smoke.sh`
      完整记录 exit code 与串口到达序列（哪 stage 过/挂），原样进 WORKLOG。
- [x] T0.4 读 `NK4A-QWEN-WORKLOG.md` 与 FIXLOG 尾部：只提炼「事实清单」
      （改了哪些文件、声称修了什么、哪些有真机证据），**不写评价**。
- [x] T0.5 判定：T0.3 是否已到 rc marker？
      - 已到 → P1 跳过，直接 P2；
      - 未到 → P1 按 NK4A-TODO §4 Task A-E 与 §8 取证循环修到 rc marker
        （不评价前棒——断点就是断点，修就是了）。

## §2 P2 x86_64 命令面

- [ ] T2.1 读 `os/etc/rc` 与冒烟脚本判据（`os/qemu-tests/test-cmd-smoke.sh`
      的 T4 窗口），搞清 rc marker 之后 rc 还执行什么、imgrd 里有什么命令
      可用（xtask 的 imgrd 装配逻辑，搜 `imgrd`）。
- [ ] T2.2 在 rc 的 **marker 行之后**追加真实命令执行（如 `echo`、`ls`、
      `cat <文件>`），**禁止改动 marker 行及其之前内容**。每条命令输出
      可辨识标记（如 `cmd-echo-ok`）。
- [ ] T2.3 扩展冒烟判据：T2.2 的标记进 smoke 窗口（改 smoke 脚本属允许，
      但不得放松既有 stage 判据）。冒烟全绿 + 两次复跑 + FIXLOG/WORKLOG。
- [ ] T2.4 若命令执行暴露用户态栈缺陷（init/sh 的 exec/加载器问题），
      按 §8 取证循环修；用户态模块 ELF 可离线符号化（NK4A-TODO §5.7）。

## §3 P3 aarch64 生产链（里程碑制，M3.x 顺序不可跳）

**已知事实（NK3 遗产，直接可用）**：
- aarch64 生产 trap 腿已通（VBAR 16 槽；NK3 真机 test-timer-irq-aarch64）；
- GICD/GICR +0x10000 偏移与 IGROUP0=0xFFFFFFFF 已修；**AAVMF 的 ACPI
  GICR 恒 0——载体必须用 QemuVirtDesc + `gic-version=3` 绕行**（
  `os/qemu-tests/test-timer-irq-aarch64.sh` 有现成 qemu 参数可抄）；
- aarch64 UEFI 载体面已存在（fw-aarch64-uefi 载体 10 个；
  aarch64-unknown-uefi target 宿主已装，构建走宿主 ulimit）；
- rt-birth aarch64 载体（`os/qemu-tests/test-rt-birth-aarch64.sh`）=
  aarch64 上 rt init 全链已点过电。
- arm64 分页：TTBR0/TTBR1 同根页；内核半映射见
  `os/arch/src/aarch64/`（paging/boot）与 `inherit_supervisor_half`。

- [x] **M3.1 载体现状点电**：跑 `test-rt-birth-aarch64.sh`，记录现状
      （过/挂、串口序列）。这是 P3 的事实基线。
- [x] **M3.2 kernel-image aarch64 产出**：`os/kernel-image/` 目前
      x86_64 专用（fw-x86-none 门 + x86_64.ld）。扩展出 aarch64 的
      链接脚本与入口约定（设计要点先写 WORKLOG：arm64 内核虚拟基址
      选择、入口符号、段布局——**对照 os/arch/src/aarch64 既有分页
      实现**，不要发明新架构）。验证=宿主可测：产出 ELF + readelf
      断言（entry/段布局）进 cargo test 或独立脚本。
- [x] **M3.3 boot-shim aarch64 装载**：boot-shim 已有 aarch64 UEFI 面
      （fw-aarch64-uefi）。让 shim 在 aarch64 上装载 kernel.elf +
      12 模块 + imgrd（复用 x86_64 路径的 LOADER_DATA/memmap 快照
      结构；arm64 特有处=入口跳转方式与 KernelInfo 交接的字段语义）。
      里程碑判据：AAVMF 下 shim 打印 `kernel loaded` + `12 boot modules
      loaded`（现有路标）。
- [ ] **M3.4 aarch64 内核点电**：跳进生产内核 arch_boot → kmain
      Phase A（memmap/模块切割）。判据：串口出现内核 kmain 路标
      （NK4A 的 boot_stage 体系是三架构的吗？若 x86 专属则补 aarch64
      等效路标——探针纪律照旧）。
- [ ] **M3.5 VM handoff + 首模块用户态**：VM 在 aarch64 上接管、
      exec 首个模块、该模块用户态执行（RS birth enter 等价物）。
- [ ] **M3.6 aarch64 rc marker**。

每个 M3.x：设计要点先写 WORKLOG → 实现 → 宿主可测部分测试全绿 →
真机点电 → FIXLOG/WORKLOG。**遇到架构级裁决点**（如内核虚拟基址、
arm64 内核栈布局）→ 写进 WORKLOG「上交裁决」小节，给 2-3 方案与
推荐项，等评审方，不自行定案。

## §4 P4 riscv64 生产链（镜像 P3 结构，M4.x 同构）

**已知事实**：riscv64 生产 stvec 双腿/timer 臂/ecall KERNEL_CALL 腿已通
（NK3 真机）；PMP 写除已删（OpenSBI Domain0 已授权 S/U RWX）；
载体 `test-riscv64-uboot.sh` / `test-timer-irq-riscv64.sh` 现成；
Sv39 用户限界 0x0000_0040_0000_0000（user_copy_range 已按 arch 分界）。

- [x] **M4.1 载体现状点电**（uboot 载体跑通记录）。
- [x] **M4.2 kernel-image riscv64 产出**（Sv39 布局 + 入口约定；
      设计要点先写 WORKLOG）。
- [x] **M4.3 装载链**：uboot/OpenSBI 侧装载 kernel.elf + 模块
      （对照 uboot 载体的既有加载方式）。
- [ ] **M4.4 内核点电 → VM handoff → 首模块用户态**。
- [ ] **M4.5 riscv64 rc marker**。

## §5 P5 三架构命令面

- [ ] T5.1 为 aarch64/riscv64 各出一套命令冒烟（拷贝 x86_64 冒烟骨架，
      换 qemu 参数与判据；rc 脚本同一份——rc 是架构无关的 imgrd 内容）。
- [ ] T5.2 三架构冒烟全绿 + 各两次复跑。

## §6 P6 run_all 接线

- [ ] T6.1 三架构生产链冒烟接入 `os/qemu-tests/run_all.sh`
      （共享文件触碰按协作规则登记；qemu-tests 脚本自带 cargo 的
      `--target`+`--features` 成对陷阱见 NK3 Fix #3）。

## §7 记录格式（与 NK4A 相同，两份缺一即返工）

### 7.1 WORKLOG（`notes/rewrite/fork-syscall-rewrite/NK4B-WORKLOG.md`，每 Task/Milestone 一节）

```markdown
## <Task/Milestone ID> — <标题>（YYYY-MM-DD）
- 状态：DONE | BLOCKED | PARTIAL
- 定性结论：<数据说话>
- 改动文件：<文件:行区间>
- 命令与结果：<测试计数/IMG-EXIT/轮次名>
- 真机证据：<serial 文件名> 关键行（≤5 行原文）
- commit：<hash>
- 上交裁决：<架构级未决，2-3 方案+推荐；无则写"无">
- 未决问题：<如有>
- 自检：fix-guard ✅/❌；计数不减 ✅/❌；两次复跑 ✅/❌；FIXLOG ✅/❌
```

### 7.2 FIXLOG（`.review/zcode/edge1/FIXLOG.md`，只追加；动手前
`grep -c "^# edge1 线修复日志" FIXLOG.md` 必须为 1）

每修复一条，**必须含「测试（防回归）」小节**（判别性断言为什么能抓住
缺陷——这是后续测试学习素材）。格式沿用既有条目（修前现状 ±5 行 /
根因与 C 对位 / 修法 / 测试 / 验证 / 边界登记）。

## §8 铁律（NK4A 十条继续适用 + 增补）

1. NK4A-TODO §5 的 13 条陷阱全部有效（docker 测试/ulimit 构建/QEMU
   方括号/探针纪律/时序敏感/符号化边界/整仓 fmt 禁止/git add -f 归档…）。
2. `os/etc/rc`：**只允许在 marker 行之后追加**；marker 行与其之前内容
   一个字节都不许动。
3. 冒烟脚本允许扩展判据，**不允许放松/删除既有 stage 判据**。
4. 架构级裁决上交（见 §3），不自行定案；实现级小决策自行做。
5. 三架构任何一个的修复都按 NK4A-TODO §8 取证循环走；fix-guard 不豁免。
6. P3/P4 每个 Milestone 独立 commit；M 未完成不冒进下一 M。
7. 证据归档：`evidence/<YYYYMMDD>-nk4b-<说明>/`，`git add -f`。
8. 会话结束交付检查清单同 NK4A-QWEN-OPENING-PROMPT §5（WORKLOG/FIXLOG/
   本地 commit/证据归档/Task 粒度最终汇报）。
