# NK4-C 接续 PROMPT（20261006q → qorder）：三架构 SMP 线收口后的接手入口

> 接手第一句：「读 NK4C-WORKLOG.md 顶部状态块（一屏读完），再读本文件全文，然后按 §三 队列开工。」

## 〇、你会接手什么

前一棒（glm，2026-10-06 会话，起点 e4b46a5aa^）在 rewrite 分支交付了 **七个增量（§续-403..409，全部已 commit）＋一节勘察（§续-410，纯读码零代码）**。主线成果＝**三架构 SMP 线整体收口**＋P-ALL-08 打下 T6/T7 两格。你接手时的 HEAD＝c8d7ba87d（§续-409），工作树干净、qemu/cargo 无残留。

各增量一句话（细节都在 WORKLOG 对应节，本文件不重复）：

| § | commit | 成果 |
|---|--------|------|
| 403 | e4b46a5aa | riscv SMP 洪流根因定谳（汇聚点 A3 写 KDM 窗 VA 未映射 fault × OpenSBI hart_start 交接 stvec=start_addr＝洪流放大器）＋A3 改恒等 PA＋riscv 专用门两连绿 |
| 404 | 2cd48d539 | 选举彩票收口：smp_init BSP 身份改读 kernel-image 选举格真值（BOOT_HART_ELECTED-1），六轮门采样覆盖彩票两分支零 ret=-6 |
| 405 | a6a78d37b | aarch64 半生产形状（桩体/接线/DTB 通道管线/专用门）＋AAVMF 交付缺陷定谳 |
| 405补 | d64032826 | virtualization=on 当轮证伪（AAVMF 转 EL2 启动内核，arch_boot 非 EL2-aware） |
| 406 | ff1e69a10 | **aarch64 SMP 门四连绿**：QEMU 直核 -kernel 引导＋bootface_a64 自举面＋KPHYS/VA_DELTA 双 typo＋fdt find_node 缺陷＋FPEN 等价化四笔修复 |
| 407 | 38514a26a | **P-RV-01 达成**：riscv ATF 全量 36/36 与 cmd-smoke 18 阶段在 -smp 2 真机双 PASS；修复两门 dumpdtb 缺 -smp（拓扑真值=DTB 教训二次重演） |
| 408 | e612e62d5 | P-ALL-08 T6 结案（定谳型）：sef_init_reply 不填 m_source 与 C 生产行为零差异——内核投递盖章是权威语义（ipc.rs:1588/1986 两路径各有测试）；NONE 哨兵形状锚 |
| 409 | c8d7ba87d | P-ALL-08 T7 结案（形状审计）：出生回报腿原语钉进 CI（一般服务 send_rec / VM asynsend / builder 三断言），防「易复发」复发 |
| 410 | （本 commit） | T1 勘察：信号请求双形态 C 语义全链钉死＋落地配方（正文即实施件） |

## 一、不可协商的纪律（八条，全部有先例教训背书）

1. **一小步一 commit 一自评审一 WORKLOG**。每增量：对照 C 真源定谳 → 最小改动 → 宿主回归（docker 配方见 §四）→ 真机门（如适用）→ WORKLOG §续-NXX（顶块同步更新）→ doc-style-lint --diff 零 error → commit。WORKLOG 节号顺延，**下一节＝§续-411**。
2. **fix-guard**：改任何文件前先读目标行 ±5 行＋grep 确认现状，不凭记忆和报告修；一次只修一条。
3. **两份案卷绝不触碰**：`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md` 与 `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` 是文档会话在制文件，**不 add、不改**；misc_concepts.md 同。
4. **纸上数字必须机器复核**：任何常量/地址/阈值先 python 算一遍再落盘。先例三连：§续-379 统计门槛抄错、§续-403① 桩 PA 心算进位错位、§续-406 KPHYS=0x0402_0000 下划线错位（16 倍误差把 AP 送到垃圾地址）。**§续-406 一轮里同一个 typo 活在两个常量里（KPHYS 与 VA_DELTA）——修 typo 后必须全仓 grep 同族字面量**。
5. **拓扑真值=DTB**：任何 riscv/aarch64 门的 dumpdtb 与点火命令的 `-smp` 必须同值（§续-407 假多核教训：dumpdtb 缺 -smp ⇒ 36/36 PASS 是单核路径行为）。
6. **测试可达性**：架构目录内 `#[cfg(test)]` 宿主不编译＝假绿；新测试必须在宿主 target 真实编译并跑过。
7. **qemu 收尾清零**：每增量结束 `pgrep -fa qemu-system` 必须为空。
8. **需人裁决项不代决**（§三列出）；条文原文标「取舍：交人」的不许自己拍板。

## 二、本会话沉淀的关键机制事实（接手后的调试地图）

- **QEMU 直核 `-kernel`（riscv/aarch64 同理）**：非 Linux ELF 走 `do_cpu_reset` 的 `!is_linux` 臂＝**全 CPU 进同一入口**（选举语义成立）；`load_elf_as(clear_lma=1)` 把 e_entry 平移到 LMA（elf_ops.h:527）。次级核交付＝QEMU 内建 PSCI CPU_ON（直核链无固件记账层，干净）。
- **UEFI/AAVMF 链的 SMP 缺陷已定谳但未解**：EDK2 DXE 期把次级核唤醒进自家 MP trampoline，ExitBootServices 后被 boot-shim 装载覆写，AP 陷「异常→VBAR0+0x200（恰为 QEMU mrom 停车笔）→重读旧邮箱」循环；CPU_ON 交付不了已记账 ON 的核（§续-405④ HMP 实证）。**绕行已落地**＝aarch64 门走直核引导。
- **aarch64 直核链的 FPEN 陷阱**：QEMU reset 后 CPACR_EL1.FPEN=00，SD-23 已知残差（rustup 预编译 core 的 NEON memset）在 kmain 后即 undef；修复＝arch_init 写 FPEN=0b11（§续-406④）。任何新 aarch64 载体都会踩同一点。
- **fdt 0.1.5 的 find_node 缺陷**：对 QEMU 直核链重建的 blob 恒 None（all_nodes 宿主/真机双验证都能看到节点）；`Fdt::cpus()`/`find_node("/chosen")` 同症。**共享代码 device_tree.rs 已绕开**（all_nodes 形态），新写 DTB 解析必须用 all_nodes 或先宿主验证。
- **GDB 编排配方**（riscv/aarch64 通用）：`timeout -s INT N gdb-multiarch -batch -ex ...`（SIGINT 到点中断并继续跑后续 -ex）＋ QEMU `-s -S`；**aarch64+pflash 链上 gdb 状态镜像是陈旧的不可信**，用 QEMU monitor `cpu N`+`info registers`（经 unix socket，见 /tmp/monreg.py 式脚本）。
- **OpenSBI hart_start 交接把 stvec 设成 start_addr**：任何会 fault 的 AP 早期代码都会被放大成「弹回桩入口」的循环——AP 早期代码的第一条 fault 即死循环，标记要在 fault 前写。

## 三、待办队列（按此顺序；判据与红线随条）

1. **P-ALL-08-T1（下一步，配方已定稿在 §续-410 正文）**：minix-types 补 SIGKMEM=71..SIGKSIG=74 常量；`sef_receive_status` 补 notify 形 sigset 位集解析＋管理器消息形（m_type==SIGS_SIGNAL_RECEIVED && source>=INIT_PROC_NR）；CannedSefIpc 宿主测试四件（位集展开/单 signum 直通/71..74 边界）；消费方盘点顺手完成 T2 勘察半。判据：全部宿主测试绿＋riscv ATF 门回归 36/36。
2. **P-ALL-08-T2/T3**：T2＝出生事件接住深度（六服务 on_signal 空闭包盘点→逐服务对照 C sef_signal.c 填实）；T3＝process_init 六段落点（SYS_STATE_CLEAR_IPC_FILTERS 无调用方）。各一小步一 commit。
3. **P-ALL-08-T4/T5**：条文明写 T4（热更新/状态搬移/覆盖统计/故障注入四类拦截建模）**需人裁决**、T5（三套 trait 收敛）**交代码卓越度裁决**——登记，不代决。
4. **P-ALL-03（真多核撤钳三件套）**：条文原文「方案乙，维持钳主核并显式把真多核列为本阶段不纳入。取舍：乙是范围裁决（**交人**）」。若人择甲：每核运行队列→IPI 三架构中断路径→per-CPU 抢占下发，对照 C `smp_schedule`/`arch_clock.c`；红线＝撤钳不先补每核队列＝进程被安到无队列的核（§续-383 的「永挂」先例）。当前状态：AP wfi 驻留、两门 PASS 实证多核拓扑对单核调度器零干扰（§续-407③）。
5. **P-ALL-07（57 服务接线）/ P-ALL-11（SD-1/SD-4 启动链结构债）**：第四档演进级，TODO 自标「不挡现有终目标」。P-ALL-07 按条文可先接命令面关键服务器（VFS/PM/VM）。
6. **需人裁决清单（向用户呈报，不代决）**：P-X86-01（方案取舍）、P-ALL-02（硬件错页地址来源）、P-A64-03（模式①幻影判定）、P-ALL-03（甲/乙）、P-ALL-08-T4/T5。

## 四、工具资产与配方（全部在本仓，已验证）

- **宿主测试铁律**：docker `minix-ci:1.94`，`-m 2g -u $(id -u):$(id -g) -v /home/xzhao/github/minix-rs:/home/xzhao/github/minix-rs -w .../os`，`cargo test -q -j 1 -p <crate>`。若 target/debug 有 root 属主：先 `docker run --rm -u 0 ... chown -R $(id -u):$(id -g) target/debug`。
- **riscv SMP 专用门**：`RUN=xx bash os/qemu-tests/test-smp-aps-riscv64.sh`（SKIP_BUILD=1 复用构建；ap-arrived 判据）。**aarch64 直核门**：`test-smp-aps-aarch64.sh`（同形；smpd11..14 四连绿基线）。
- **套件级多核门**：`RUN=xx RS_ATF_WORK=/tmp/atf_work_xx bash os/qemu-tests/test-atf-riscv64.sh`（-smp 2 基线 36/36=34+2+0；串口看 sbi-hs/ap-arrived）。
- **文风门**：`bash tools/doc-style-lint.sh --diff` 必须零 error（SL-5 禁「修复前」等修复史叙事词——叙述现状用「其时/该状态」）。
- **fmt 核验**：`rustup run nightly rustfmt --edition 2024 --check <file>`（宿主 stable 不认 let-chains；整仓 fmt 会污染 diff——只 check 自己碰的文件）。
- **cargo 特性统一陷阱**：裸 `-p minix-arch` 等混合形态调用会报 std 缺失；aarch64 内核构建必须带 `RUSTFLAGS="-C target-feature=-neon,-fp-armv8"`（SD-23）＋`--features fw-aarch64-none`；riscv 同理 `-f,-d`＋fw-riscv64-none。
- **会话 shell cwd 漂移**：多次后台任务后 cwd 会漂——**每条命令显式 `cd /home/xzhao/github/minix-rs`**（§续-401 的 git checkout 静默失败先例）。
- gtest 式「纸上断言先窄后宽」：写形状 pin 测试时窗口贴回报点（首版 §续-409 扫进别 handler 假红）、断言按真实形状族写宽（DS 的 caller 形）。

## 五、诚实边界（前任留下的已知未判处）

- aarch64/riscv 的 rc 全链 marker 未在 SMP 门窗判内到达（-smp 2 TCG 变慢，系统存活非停滞；§续-403⑤/§续-407 登记）。
- -smp ≥3 未验（kernel-image 停车邮箱单记录单停车 hart 边界，§续-402 登记；多核接线 per-hart 化=P-ALL-03 甲的一部分）。
- stvec=桩 洪流放大器加固候选未做（记录 v2 加 stvec 字段，ABI 变更需评审；A3 修复后主路径无已知 fault 点）。
- run_once_integration 2 失败为基线既有（-78/78、-10/10 符号断言）。
- 问题乙（riscv 非确定性内存腐蚀根因）**保持未结**——本轮零命中可能只是运气（§续-385 诚实边界原文有效）。

## 六、开始

1. `cd /home/xzhao/github/minix-rs && git log --oneline -3` 确认 HEAD 在 c8d7ba87d 或其后。
2. 读 NK4C-WORKLOG.md 顶部状态块。
3. 开工 §三.1（T1），按 §续-410 配方实施；每完成一小步执行 §一.1 的完整循环。
