# NK4-C 接续 PROMPT（2026-10-03 · riscv 优先版）

> 把本文件整段作为新会话的开场任务说明。它是**入口**，不是全部——真正的取证细节在
> `rewrite-notes/coordination/NK4C-WORKLOG.md`（git 跟踪的活体权威载体，前沿=§续-279n/-b）。
> 本 Prompt 与前作 `NK4C-接续PROMPT-20261003.md` 的关系：**前作的解锁条件 3（范围裁决：转推 aarch64）
> 已被执行完毕并大获全收**；本版本把主攻方向切回 **(A) riscv**，并携带 aarch64 战役期间新增的
> 资产、教训与台账。三终目标不变、`/goal` 永续持续推进，**riscv 最先**。

---

## 一、这个项目在做什么，你要达成什么（三终目标，`/goal` active 直至全部完成）

minix-rs = Minix3 微内核的 **Rust 语义重写**（x86_64/aarch64/riscv64，`#![no_std]`，
Kernel=SMP+BKL，用户服务器单线程事件循环）。三条终目标：

1. **三架构启动 marker**：x86_64 ✅、aarch64 ✅、**riscv64 ❌（被 (A) 门住——本会话主攻）**。
2. **18-stage 命令面**：x86 ✅、aarch64 ✅、riscv ❌（同被 (A) 门住）。
3. **586 个 minix3 C 测试（atf-c）上机跑通**：aarch64 已实证 **33 passed/36 用例**（§续-279n，
   三日 9→29→33；仅剩 memcpy D1×1 台账 fail + D3b 相关 2 无结果行案），riscv 链面 18/18 但上机被 (A) 门住。

目标不是「修一个 bug」，是**全部达成后才 mark complete**。遇卡点自选可推进路线，不许中途停。

---

## 二、先按顺序读这些权威状态源

1. 本文件。
2. `NK4C-WORKLOG.md` **顶部 banner（§续-279n/-b 前沿）** → 需要细节再翻文末对应 §。
3. `git log --oneline -25`（近三笔：`f5d6f11a6` 279n-b 评审修复轮 / `f1dbc079f` 279n strerror 接管 /
   `cf0d7d7cf` 279m-close）。
4. `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` + `riscv瞬态页表崩溃取证方法论.md`（(A) 本体取证全史，必读）。
5. 前作 `NK4C-接续PROMPT-20261003.md` 的 §三~§五（(A) 认知状态/死路清单/迭代协议）——**仍然有效**，
   本文件只更新差量。

---

## 三、(A) riscv 的确切认知状态（最重要；别当已解决、别重走死路）

**定性**：riscv64 `fork` 路径 **timing-sensitive 竞态**——子进程地址空间中某父级页表槽在
被 walk 读的瞬间持有 PPN>RAM 的**瞬态值**，内核 walk 后 `panic!("pagefault in VM")` 或直接崩。
非确定性 + **QEMU 插件拖慢即消失**（观测者效应），因此：

- **三条解锁条件（任一满足即可重新推进 (A)，前作 §四原文有效）**：
  1. 能观测故障现场的手段（本机 QEMU 8.2.2 无寄存器/内存读 API 插件面；`tb-exec` dump 会扰动，须能滚回）；
  2. 非插桩的确定性复现（每次逐字节相同）——§7 验收「≥3 轮独立复现证无」只有在此形态下才可达；
  3. ~~范围裁决转 aarch64~~（**已执行完毕**，即 §续-276..279 战役；现在这条不再是逃逸路线，主攻=回到 1/2 的争取）。

- **已排除候选（勿重走）**：分配器双发、耐久 PTE 写坏、低 carve-out 别名（boot_pt_alloc 区
  [0x80000000,0x80100000) 与崩帧高 RAM 不相交）、setaddrspace/map kerninfo 与 C setcr3 的结构差
  （§续-274 已审计同构）。
- **收窄后的嫌疑面**：VM 高 RAM 帧生命周期（`ptalloc-reuse-DATA` 腿，`os/servers/vm/src/alloc_page.rs`）
  × 内核调度/CPU 切换时机的交互。VM 单线程 ⇒ VM 内部只能是逻辑时序 bug；插件拖慢令 race 消失
  ⇒ 涉及内核何时切 CPU。
- **§续-217 首个正面线索**：0x800c 栈链引用的帧 0x9d2ae000 内容含 `/etc/rc`+`autoboot` 串
  （数据↔映射帧混淆实物证据）。**下一步 a2d（登记未做）**：riscv-gated U-safe 探针
  （`bootmark::mark`/sys_diagctl，**绝不用 `CurrentEarlyConsole`**=记忆 a298c18a）在 walk 读 l2e 且
  `!leaf && paddr>=0xA0000000` 时抓 (walked_root, level, idx, raw)，读取瞬间定帧，再设写 watchpoint 抓写者。
- **未坐实不成修**：任何改动前先拿到 1/2 形态的现场；否则只做纯静态审计+取证工具建设。

**aarch64 战役给 riscv 攻坚带来的三件新武器（§续-279m 后新增）**：
1. **VM brk/region 模型已与 C 对齐**（绝对断点直译 `map_region_extend_upto_v`）——riscv 若因
   旧 brk 模型制造过程序异常，现在这条混淆源已除；riscv 测试 ELF 的 `sbrk` 真腿也已就位
   （posix-stubs.c 双 arch 同链，objdump 实测 ecall 形正确）。
2. **split 丢 memtype P0 已修**（生产级，三架构共享）——旧 riscv 取证里若有「region 在但
   NoMemType 死」形态，可能同因，重读旧现场时以此复核。
3. **2048 页 minix-rt 池仅在非 x86 生效**（§续-279d 分档），**riscv 从未在 2048 态 boot 过**——
   riscv 上机前这是一处未验证面，可能引入新行为（正面或负面都可能），首轮 riscv boot 取证时留意。

---

## 四、当前精确读数（2026-10-03 §续-279n 后）

- 真机 aarch64：**33 passed + 1 failed（唯一=t_memcpy.c:99 D1 台账）+ 2 无结果行（D3b 相关台账）**
  （36 用例中 34 出结果行）。
- 已知台账（都不算新缺陷）：
  - **D1**：`memcpy_basic` 对 NetBSD random 期望串失败（p5 探针定谳 picolibc random=LCG `_rand_next`
    与 BSD 序列必异）→ **已非口径决策，真源已定位**（§续-279n 末尾取证）：BSD random 真源 =
    `minix3/common/lib/libc/stdlib/random.c`（530 行，randtbl/TYPE_3/DEG_3 全在；lib/libc/stdlib/
    Makefile.inc 引用的 random.c 缺件即由此补）。修法=random/srandom 接管进 atf-c-compat
    （picolibc 两符号各居单符号专属成员 libc_stdlib_{random,srandom}.c.o，命令行对象抢占即达，
    sbrk/sysconf/strerror 先例同形；t_memcpy 依赖 random+MD5 两口，MD5 已在链）。host 侧可先用
    t_memcpy 的 runTest+MD5 逻辑对 goodResult `7b405d24bc03195474c70ddae9e1f8fb` 做离线对账再上机。
  - ~~**strerror 族 ×4**~~：**已清零**（§续-279n：compat 全接管 strerror/strerror_r/strerror_l 三口 +
    sys_nerr 边界差一修正，sys_nerr=135 与 C compat_errlist.c:153 公式同形；易错模式=边界必须按真源
    公式取、探针必须覆盖边界双侧、裸名引用必须由裸名定义满足——详 WORKLOG §续-279n）。
  - **D3b**：子进程 SIGSEGV 异退后，父 `sh` 的 `waitpid` SendRec 未获回复（ESRCH 车道挂父）→
    正修=PM exit/event 唤醒语义（§续-279k 登记，未动；尾部 2 个无结果行案与此相关待辨）。
  - gh152 瞬态 boot 死（同镜像重跑即好）——登记未追，若 riscv/aarch64 再现再立案。
- 正式门 `os/qemu-tests/test-atf-aarch64.sh` 现判 FAIL rc=1 ——**这是门的正确行为**（D1 真 fail
  不被掩盖；stall 判定与 2 无结果行案对账一致），全绿条件=D1 口径+D3b。
- 回归基线：host 4 包集 0 failed、VM host 535/0、check-layout all PASS、x86 smoke PASS、
  aarch64 bootmarks rc=0、双 arch 18/18 链面零警告（§续-279n-b 复验）。
- **工作区注记**：`rewrite-notes/03-stage-rs/{00-rs-overview,doc_rerank_qwen}.md`
  有两处**非本会话所改**的脏文件（21→22 篇文档计数），留给用户/RS 文档线处置，**勿顺手 commit**。

---

## 五、迭代协议（每小步执行，与前作 §五逐字有效，摘要）

1. **commit**：message 以 `NK4C 续-NNN` 开头（本会话从 **§续-280** 起编）。
2. **CodeReview**：每笔含代码改动的 commit 派 **ONE** CodeReview subagent（`review_scope=explicit_target`
   指向该 commit），完整性/正确性/影响面三合一；纯文档记「无代码可审」；评审出 P0/P1 必开修复轮
   （先例格式：`续-NNNb`）。
3. **WORKLOG**：顶部 banner 前沿 + 文末 `§续-NNN` 详节（分析过程/思路/如何解决/负结果证据）。
   **WORKLOG 必须追上代码**——每轮收官前核对 banner/正文/HEAD commit 三者一致。
4. **fix-guard**：改前读 ±5、grep 确认现状、一次一修、改后 grep 验证。
5. **Ground Truth 优先链**：Minix3 C 源码 > 设计文档 > Rust 代码 > 笔记；不确定就 grep `minix3/`。
   （§续-279m 的战例：笔记登记的「C 靠 RS VM_MMAP_DATA 腿」被全树 grep 零命中证伪——
   **候选地图本身也要被真源审计**，勿盲信上一轮的下一步。）
6. **code-excellence 全程加载**：死代码消除、多方案对比、非法态封堵。

---

## 六、回归底线（任何生产码改动 commit 后必跑）

```bash
cd os && cargo test -q -p minix-kernel -p minix-arch -p minix-boot -p minix-types   # 禁 --workspace；0 failed
cd os && cargo test -q -p minix-vm                                                  # 基线 535 passed / 0 failed
bash os/kernel-image/check-layout.sh all                                             # 三架构 PASS
# 两 marker 不回归：x86 smoke PASS（os/qemu-tests/test-cmd-smoke.sh，真机）
#                 + aarch64 bootmarks rc=0（os/qemu-tests/test-shim-bootmarks-aarch64.sh）
```

**每轮收尾**：`pgrep -cf "[q]emu-system"` 须为 0 → `git status --short | grep -vE '^\?\?'` 净
（§四的工作区注记两项除外）→ 跑回归底线。qemu 一律**按 WORK 唯一路径/端口锚定精确杀**，绝不裸 pkill。

---

## 七、可复用资产（直接点火，别重造）

**riscv 取证（承自前作，全部有效）**：
- `os/qemu-tests/test-riscv64-boot-full.sh`：**首选现场工具**——它自行从当前 release 二进制重生成
  table.bin+模块 packing，2026-10-03 实测已把新基线指纹拿在手里（崩点仍在 `pagefault in VM`，
  sepc=0x14060 同族，共享码变更后首读；详见 WORKLOG §续-279m-close）。
- `tmp/nk4a/riscv_fingerprint.sh <tag> <port>`：**已陈旧——本轮实测坐实**它顺序 packing 不重生
  table.bin，模块尺寸漂移后表/实错位（报 `pfs not valid ELF`，非 (A) 新线索）。用前须先把它的
  packing 换成 boot-full 同源再生逻辑，或直接改用 boot-full 门。
- `tmp/nk4a/watch_store_plugin.c/.so` + `riscv_plugin_run.sh <tag>`：已修好可编；注意观测者效应。
- `tmp/nk4a/riscv_gdb_*.sh` / `riscv_watch_*.sh` 族：gdb 断点/watchpoint 载体。

**aarch64 战役新增（§续-276..279m）**：
- `tools/build-atf-test.sh <test.c> [arch]`：单案交叉编链（riscv64/aarch64/x86_64 三路，双 arch 零警告）；
  依赖 `tools/build-libatf-c.sh` 产物与 `tools/vendor-atf-toolchain.sh`（免 root picolibc vendor，
  sudo 不可用环境用 `apt-get download`+`dpkg -x` 的既成模式）。
- `tmp/nk4a/atf_a64_run.sh <tag> <wait>`：上机 harness（serial 落盘+定点杀）；**riscv 同型改造是
  本会话可选建设项**——(A) 解锁当天即可全速上机。
- `os/qemu-tests/test-atf-aarch64.sh`：套件正式门（manifest 终集定案/SIGPIPE 防线/stale 防线），
  riscv 解锁后按它派生 `test-atf-riscv64.sh`。
- `os/xtask/src/image.rs`：ATF_SUITE 18 项表、extract_atf_tc_names、rc 变体注入、atf-plan manifest、
  `ATF_BOOT_LEG_READY=&["aarch64"]` 架构门——**riscv 解锁后把门加上 "riscv64" 即接通全套件接线**。
- 易错模式库（都在 WORKLOG 相应 §）：pipefail×grep -q×大输出=SIGPIPE-141 假失败；aarch64/riscv
  隐式声明 sxtw 截指针；Debian 加固双毒（paciasp/__stack_chk_init）；IPC 占位失效前不先拆旧路径；
  VM 回执=正 errno 而 C `_syscall` 路=负 errno；测试夹具必须取真源存在的形。

---

## 八、本会话（riscv 优先）的建议作战序

1. **先复基准（已替你做了一步）**：2026-10-03 用 boot-full 门在新共享码上拿到首读——崩点仍在
   `pagefault in VM`（sepc=0x14060 同族，证据存 `tmp/nk4a/` 与 /tmp/rvboot_279m.log 同源串），
   崩点未因 279m 迁移；你接手后先跑 `test-riscv64-boot-full.sh` 确认可重现，再比形态分布。
2. **争取解锁条件 1/2**：按 §三 a2d 登记落地 riscv-gated U-safe 瞬间探针（读 l2e 且 paddr 越 RAM
   时抓 walked_root/level/idx/raw，first-N 封顶）；同轮试验非插桩确定复现（同镜像重跑 N 次比对
   指纹逐字节性——§续-279m 已有 aarch64 瞬态对照法可抄）。
3. **若探针/复现到手**：钉写者 → 对位 C（`minix3/sys/kern/` + `pagetable.c` + vm `fork.c`
   map_proc_copy）找 happens-before 偏离 → 坐实 → 成修 → §7 验收（≥3 轮证无+两 marker+host 全绿）。
4. **若拿不到**（工具缺口仍在）：riscv 侧保持 (A) open，同轮做不依赖 riscv 的确定收益：
   D1 真源直译（random 接管，真源 `minix3/common/lib/libc/stdlib/random.c` 已定位，见 §四）→
   D3b（PM exit/event 唤醒链）→ aarch64 门全绿 → 然后继续
   回到 2 的变体尝试（不同 QEMU 版本/不同 smp 度/关定时器扰动的对照矩阵）。
5. **结案滚除义务在你手里**：(A) 成修或结案时，全树 `nk4a:`/`nk4c:` 探针一次性滚除
   （内核 pf-exit/pfa/oomrt + VM vmm-h/brk/pf-region + 其余套件期探针），WORKLOG 已有登记。

---

## 九、诚实边界（交接时必须知道的未完事项）

- **(A) 根因未坐实**：无指令级定性，无成修；上面所有「嫌疑面」都是候选不是结论。
- **riscv 从未在 2048 池态 boot 过**（§续-279e-note 明文）；riscv 测试 ELFs 已换真 sbrk 腿但
  **从未在 riscv 上机跑过**——首轮 riscv boot 读数若与旧取证不同，先排除这两个变量。
- **D3b/尾部 2 案**：都在 §四台账，未动码；D1 真源已定位未动码（接管方案与离线对账法见 §四）。
- **诊断 mark 残留**：见 §八.5，结案前统一滚除。
- 收官判定：三终目标逐项拿**当前态**证据（不许引用历史读数），才 `/goal` complete。

读完本 Prompt + WORKLOG banner，你就具备接续态。开始推进，勿中途停止。
