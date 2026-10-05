# NK4-C 接续 PROMPT（2026-10-05b · 污染写者收窄末段交接版）

> 把本文件整段作为新会话的开场任务说明。它是**入口**，不是全部——取证流水在
> `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md`（前沿=§续-369），
> 独立审查文档=**`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`**（同目录，GPT 审查：
> 排除分辨力边界/结构性事实/实验设计/验收判据——**新会话必读**，本文不重复）。
> 三份文档冲突时以 WORKLOG 为准。
>
> `/goal` 继续：**直至三条终目标全部完成**；当前唯一核心阻塞=「riscv64 非确定性内存
> 污染」（本文第四~七章）。

---

## 一、三终目标台账（本文写作时实测态）

| 目标 | 状态 | 证据 |
| --- | --- | --- |
| ① 三架构启动 marker | ✅ | x86 `test-cmd-smoke.sh` / aarch64 `test-shim-bootmarks-aarch64.sh` / riscv `test-riscv64-boot-full.sh` 全 PASS |
| ② 18-stage 命令面 | ✅ | 三架构 `test-cmd-smoke*.sh` PASS |
| ③ minix3 ATF C 上机 | aarch64 ✅（34 passed+2 skipped）/ **riscv 受阻**：36 案全量门在概率性内存污染下不可靠（第 1 次采样即 VM 野 VA panic 零终态，§续-364）；t_memcpy 单案概率性失败（机上正确可达：t64 全对账跑 RESULT=7b405d24 一次，§续-363） |

---

## 二、先按顺序读

1. 本文。
2. **`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`**（GPT 审查：第 4 章排除分辨力边界、4.3 三结构事实、第 6 章九类候选排序、第 7 章实验设计、第 8 章验收判据）。
3. WORKLOG banner（前沿=§续-369）+ 文末 §续-342..369。
4. `git log --oneline -30`（注意并发会话：C-66 文档线；BUG-TRANSIENT-PTE.md 有并发会话在改——勿动）。

---

## 三、本案五次修正后的终稿画像（每条带锚点）

1. **计算层完全洗清**：裸机 semihosting 直跑（不经内核/加载器）3/3 逐位 MATCH=7b405d24（§续-342；工具 `tmp/nk4a/tmemcpy_bisect/check_bare*.c`）。排除：riscv 交叉编译器、BSD random、picolibc memcpy、我方 md5.c。
2. **机上正确可达**：t64 探针（全 12 对+全 64B dump，1.5MB 串口）一次 boot **RESULT=7b405d24 与裸机逐位一致**（§续-363）。⇒ 非系统性算法错，是 per-boot 是否被击中的差别。
3. **腐蚀=per-boot 随机「小而准」写**：命中对象不定（probe .data 摘要上下文/共享 RWX 页 48 字节记录/模块镜像/VM boot 数据），单次写几字节到几十字节；同页其他对象可无恙（§续-349：mc 改而同页相距 264 字节的 randtbl 完好）。率随暴露时长单调上升（不称正比，GPT 3.3）。
4. **冻结/爬行=插桩打印量的时序副作用**（§续-361 破案）：高打印探针必然在 245699 字节附近停滞（四份停滞日志长度全等内容各异=kdst copy pa 相邻页差一页，§续-365 GPT 复核）。**探针一律用低打印形态（v1/v2l/cli）**。
5. **本会话 os/ 代码全族排除**：arch 回退臂（§续-359）+ pre-T13 三树回退臂（§续-360）皆复现冻结；且 pre-T13 内核时代 run2 已腐蚀（§续-342）⇒ 腐蚀与 T13 与走表变更全部无关。GPT 4.1 方法论批评成立：双臂回退用的同一高打印探针=混杂变量，对「内核清白」无分辨力——**实验甲（低打印双臂）需重做**。
6. **野 VA 家族双形状**（§续-365 + GPT 第 5 章修正）：Shape A=run8 `0x7ffff48b124e6`（38-50 位全置，LP64 栈指针形状；era_323 应移出证据集——系旧栈顶时代样本，GPT 5 章论证）；Shape B=gate1 `0x1bfffe61e730`（低 38 合法+散布高位 {38,39,40,41,43,44}）与 bit38 族。静态审计：riscv 生产路径零活体 48-bit 常量、零可疑截断。

---

## 四、结构性事实（GPT 4.3，已逐一核实入账 §续-367）

1. **单核**（-smp 1）：跨核 TLB 陈旧整类机制不可达。
2. **ASID 未启用**：生产切换全走 `set_active_root_tracked` → `TlbArch::set_active_root`（satp+全量 sfence ✓）；`paging.rs:switch()` 裸写 satp = **riscv 生产零调用的死代码**（登记待清理，非活体缺陷）。
3. **无 DMA 设备**：仅 loader/serial/display——外设写内存整类配置性排除。

---

## 五、实验丁执行结果（§续-368，GPT 四结果表判读）

关中断臂（`NK4C_CLI` 旗标：DIAGCTL 魔术串置/清 + restore_to_user 跳过 SPIE 置位 + 根切换自动清零安全网）3 boot：**2 PASS + 1 中段腐蚀**（cli3 前 5 对 MATCH、分歧 pair(2,0)=两次 printf 之间）。

⇒ **定时器陷阱路径降权**；嫌疑收窄=**per-kcall 用户内存写**（finw 回执写 / DIAGCTL diagbuf 拷贝——printf 边界命中、率∝kcall 量、关中断不灭=同步 ecall 仍走、aarch64 实现不同）。

双走表对账探针（cross_space_copy 首页 + finw 每页，`vm.rs::nk4a_walk_reconcile`）4 boot **零 mismatch**——**判据边界（防误读）**：零命中只排除「同次拷贝内两次走表不一致」，不排除确定性错译（两次同错）/DIAGCTL 拷贝/Some→None 翻转。

---

## 六、下一会话实验队列（按信息增益排序）

1. **采样至 ≥10 boot**（cli/v1 低打印探针，每 boot ~2 分钟）：统计命中率；抓首个「腐蚀+对账零命中」boot。
2. **三方对账**（腐蚀再现且对账零命中时）：walk 返回值 + 页表内存原值直读（读 PTE 本身）+ 手工三级解析比对——解耦「walk 代码错」vs「页表内存错」。
3. **DIAGCTL 拷贝路径守卫**（`dispatch_diagctl` data_copy_vmcheck 段——唯一未覆盖的 per-kcall 用户内存访问）。
4. **GPT 丁子件二（从未测过）**：陷帧守护校验（sret 前比对 sp/ra/a0-a7/sepc/sstatus）。
5. **GPT 实验甲**：低打印探针双臂（现内核 vs pre-T13）重测内核清白，判据=P(0,1) 逐次开机恒定性（现有三跑三值=内核侧未清白的初步信号）。
6. 腐蚀率受控后：riscv ATF 门全量重跑 + aarch64 ATF 门回归 → 目标③收口。

**验收判据（GPT 第 8 章，强制）**：双判据=确定性恒定判据（固定低打印探针跨 boot 恒定）+ 统计判据（k 由 (1-p)^k<1% 反推，p 小需 k 大；**禁用被响亮故障截断的跑当统计点**）。

---

## 七、工具资产（直接点火，别重造）

- **裸机对账**：`qemu-system-riscv64 -machine virt -bios none -semihosting-config enable=on,target=native -kernel <elf>` 直跑 picolibc 程序（秒级）；三坑=RISC-V semihost 魔法序列必须未压缩 ebreak（c.ebreak 不触发拦截）、stdout 在 libsemihost iob.c.o 需 --start-group、探针打印消费 random 流必须重播种。
- **探针族**（`tmp/nk4a/tmemcpy_bisect/`）：`t_memcpy_v1`（12 P 行+RESULT，低打印默认形态）/`v2l`（+S8 流头 8 值）/`t03`/`t64`（全 64B dump，高打印勿用于长跑）/`cli`（v2l+关中断钩子）；参考态 `cbv4.txt`/`ct64.txt`/`reference_baremetal.txt`。
- **`boot_probe.sh`**：单案点火驱动（monitor socket+双快照 T1@marker/T2@STALL 经 stop→pmemsave、STALL 90s 零进展检测、SIGKILL 兜底、时间戳串口名）。QEMU_BIN 可覆盖（8.2.2 系统 / 9.2.4 `~/opt/qemu`）。
- **`riscv_halt_dump.sh`**：短跑快取证（~2 分钟 marker 即退，从未观察 p1 exec 相）。
- **模块区 diff**：table.bin 解析+pmemsave RAM 对 12 模块逐字节比对（健康对照全 EXACT）。
- **walk_reconcile**：cross_space_copy 首页 + finw 每页双走对账（CAP=32）。
- **WALK_HARDENING**：arch 特性门（默认关=旧语义；ON=规范性+错粒度巨叶 fail-closed）+ `riscv64_walk.rs` 252 对抗测试（纯逻辑层 walk_read_with 闭包注入，宿主可测）。
- **NK4C_CLI**：关中断实验旗标（见 §续-368）。
- 回归底线：`cd os && cargo test -q -j 2 -p minix-types -p minix-arch -p minix-kernel -p minix-vm`（基线 0 failed）+ `-p minix-vfs` + boot-shim test-all 27 + check-layout all + 三架构门。

---

## 八、迭代协议（用户纪律，强制）

1. **一小步一 commit 一 CodeReview 一 WORKLOG**：代码/脚本改动必须当轮记 WORKLOG（分析过程/负结果证据都写）并提交；派 ONE CodeReview，P0/P1 开修复轮。
2. **code-excellence 全程加载**：死代码消除、多方案对比、非法态封堵。
3. **fix-guard**：改前读 ±5、grep 确认现状、一次一修、改后验证。
4. doc-style-lint --diff 零 error 才提交（SL-4 日期/SL-3 编号字样已多次踩）。
5. 只 `git add` 自己的文件（并发会话有文档在制；BUG-TRANSIENT-PTE.md 勿动）。
6. 收尾：qemu 进程清零（pkill 模式会自匹配杀掉自己 shell——用精确 PID）、tracked 树净。

---

## 九、诚实边界与遗留

- 写者未定位；「腐蚀可能不止一个写者」（GPT 3.7 强度限定）。
- 对账探针盲区=确定性错译（同错不可见）。
- aarch64 干净=走表/陷入实现不同的旁证，非证明。
- 模拟器缺陷未完全否证（两版本同族但可能共享代码路径）。
- T13 余量（A9 夹具 57 行/A10）与 Phase E（探针滚除）排队；结构债 SD-17/9/16/6/8 排队（见 STRUCTURAL-DEBT-REGISTER-20260930.md）。
- 探针资产全部在 tmp/（不入库）；本笔入库的=GPT 审查文档（用户提供）。
