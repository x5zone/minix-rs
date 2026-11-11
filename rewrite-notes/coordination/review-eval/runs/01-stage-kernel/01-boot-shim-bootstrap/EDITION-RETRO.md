# D01 EDITION-RETRO（V3.0，R26→R27）

> 触发条件：V3 §7.5 的 3 轮试验窗口。R26 是 V3.0 第 1 轮，R27 是第 2 轮；R27 的完整 survey
> 找到了 R26 修复引入的 P0 与 70 条新发现，说明 R26 的 `CONVERGED` 不成立。本文件记录
> 为什么没有继续刷第 3 轮线级补丁，以及下一次 edition 必须改什么。

## 一、事实

- R26：完整 survey 59 条，全部处置/认证，`--round R26` PASS，曾判 `CONVERGED`。
- R27：同一组 7 个角色重读当前文档，找到 **70 条**：
  - FACT-A 7（P0 1 / P1 3 / P2 3）、FACT-B 10（P1 2 / P2 6 / P3 2）；
  - READER-A 12、READER-B 17、READER-C 11；
  - REFUTE 6 主张（阻断 3 / 非阻断 1 / 反证失败 2）；
  - DESIGN-D2 7（P0 1 / P1 2 / P2 4）。
- **R27 的头号 P0 是 R26 修复引入的**：DESIGN-09 的 `BootStorage` 合并先把模块/平台/参数
  前缀洗成 `&'static`，随后又对同一 `BootStorage` 调 `set_param_len`/`push_startup`
  （`&mut self`），按 Stacked Borrows 是 UB。R26 的 certify 没有抓到，因为它是“改动段回读”，
  不是“改动的所有权模型重新推演”。
- 已在本轮修复：
  - D2-13：把四个 `'static` 切片的创建推迟到所有写入之后，先取裸指针、结束 `&mut`、再
    建只读切片；两个 bootface 都已改，riscv64 bootface 与 aarch64 SMP AP QEMU 重跑 PASS。
  - D2-14：补 `WIRE_MODULE_CAP >= NR_BOOT_MODULES` 编译期断言。
  - D2-15 / FACT-A-01 / FACT-A-02：§4.5 `store_kernel_info` 引文改为新签名/`ptr::copy`/返回
    值；§4.6 Step 1 循环头改为 `for range in windows.0`。
  - FACT-A-03：`push %eax` 行号 84→86。
  - FACT-A-04 / FACT-B-01：§2.3 字节图改为 8 列，去掉幻影“记录尾”列，B=4、`=`=5。
  - FACT-A-05：§3.4.5 说明 startup 走 `push_startup`，不是 `set_*_len`。
  - FACT-A-06：`KNOWN` 改为 16 个属性常量、覆盖 27 个位位置。
  - FACT-B-02：删除已不存在的 Step 1 NOTE 引文。
  - FACT-B-03/04/05/06/07/08/09/10：行号、6b 拍号、容量口径、三条链/三条腿措辞、
    `checked_shape` 的 Step 2 早退、`pt_free` 回收链、`pg_map` 单次挂页表述。
  - READER-A-01/02/04、READER-B-01/02/03/06、READER-C-01/03/04/05/10、
    REFUTE 的 §3.9 ③ 与 §3.8 因果链：已按报告修。
- 未修（R27 backlog，主要是 P2/P3 读者/结构项）：READER-A-03/05/06/07/08/09/10/11/12、
  READER-B-04/05/07/08/09/10/11/12/13/14/15/16/17、READER-C-02/06/07/08/09/11、
  DESIGN-D2-16..19。逐条位置见 `panel/round27/*.md`。

## 二、为什么没有收敛

不是“文档缺陷无限”，而是**这一版流程的修复单位错了**：

1. **线级补丁扰动邻句**。R26 的 59 条里 35 条是读者项，修法是改一句/加一个指针；每改一句，
   相邻段落的口径就变一次。R27 的 70 条里大部分是这种“改动段邻句”问题，而不是新根因。
2. **certify 的闭集太窄**。R26 的 certify 只读本轮修复 hunk + 改动段；它没有重新推演
   “改动的所有权模型/接口顺序”，所以 D2-13 这种“合并不是行级改动”的 P0 漏了。
   R27 的完整 survey 才抓到。
3. **读者清单没有冻结成验收表**。读者角色每次都能从同一段读出新的“我卡住了”，因为清单
   第 9 条（可复述）没有可执行的通过判据；于是同一段在 R26 修完、R27 又被报。
4. **设计快照仍被当作合同读**。R26 已加 R26 裁决节，但 Ch2–Ch4 的旧条款还在；R27 REFUTE
   仍能逐条找出“合同与文档相反”。R26 已加“历史定位”行，但审查角色仍把它当输入合同。

## 三、下一次 edition 必须改什么（候选规则，进下一个边界裁决）

1. **B 类修复必须结构重写，不是线级补丁**：同一根因第二次出现（R26→R27 的邻句类）→
   重写该段/该小节，并在台账写 `rewrite`；不允许再改一句。判据：R27 的 70 条里，凡
   `origin=R26` 的邻句项，下一次必须整段重写。
2. **certify 增加“所有权/接口顺序”复核**：改动涉及 `&mut`/`&'static`/生命周期/跨 crate
   签名时，certify 必须跑一次“写后读”顺序推演（或 Miri/Stacked Borrows 等价检查），
   不能只读 diff。D2-13 是这条的判例。
3. **读者验收清单加可执行判据**：第 9 条“可复述”改成固定三问（本节的不变量是什么、
   它的前提是什么、它的证据在哪），读者必须逐节给出答案；答不出才是 FINDING，答得出
   但“更喜欢另一种写法”进 suggestions。
4. **设计快照降为历史输入**：审查不再把 `.design/*.md` 当合同；合同=文档+源码+DECISIONS。
   R26 已加历史定位行，下一次边界应把这条写进 `review-evals.md`，并考虑给快照加
   `superseded: true` 的机械标记。
5. **完整 survey 前先跑“修复 diff 的机械再对账”**：R27 的 FACT-A-01/02 是 R26 改了代码
   但没回填引文，任何一次 commit 级 `doc-citation-check` 都能抓到。下一次 L4 的 certify
   必须在**最终 HEAD**上跑 citation/anchor/symbol-drift，不是只跑改动段。
6. **3 轮试验的第 3 轮必须是结构轮**：R28 若仍是线级补丁，直接判 `NOT CONVERGING`；
   R28 应只做“按 EDITION-RETRO 的结构重写 + 机械再对账”，然后才允许再开完整 survey。

## 四、状态

- R27：`NOT CONVERGED`；P0 已修，P1/P2 事实项已修，P2/P3 读者/结构项进 backlog。
- V3.0 edition：R26 的 `CONVERGED` 被 R27 推翻，按 §7.5 记 `NOT CONVERGING` 候选；
  下一个边界必须裁决本文件第三节的 6 条，才允许 R28。
- 代码：riscv64/aarch64 kernel-image 构建通过；riscv64 bootface、aarch64 SMP AP 两个
  受影响 QEMU 门重跑 PASS（`logs/round27-*.log`）。x86/aarch64/riscv U-Boot shim 与
  riscv kernel-image 未受 D2-13 影响，沿用 `logs/round26-*.log` 的 PASS。

## 五、R28/R29 之后（V3.1 的实测）

- R28（V3.1 第 1 轮，结构轮）：按本文件第三节 6 条结构重写 R27 backlog，37 个发现
  ID / 32 个根因全部关闭（2 条 FP、4 条 ⑥ 设计实施、31 条 ④ 结构重写）；trace
  `--round R28` PASS；六道文档门 + 三目标 `cargo check` + 空窗口宿主测试通过。
  R28 不判 `CONVERGED`，因为它不是新的完整 survey。
- R29（V3.1 第 2 轮，完整 survey）：6 个角色找到 **55 条**新发现——FACT-A 3、
  FACT-B 8、READER-A 10、READER-B 15、READER-C 10、DESIGN-D2 9；其中 3 条 P1：
  ① §4.6 把 x86 的 `kernel_identity_mmio_regions()` 说成空表；② §5.2.3 把 aarch64
  直核面算进启动门链；③ §6.1 把 `step1+2` 的打印者与 `[raw] entering` 混在一起。
- R29 的 55 条已全部进入 `defect-ledger.md`：① 10、② 0、④ 35、⑤ 0、⑥ 9、FP 1；
  `--round R29` PASS，未闭环 0、certify 新准入 0。② 0 条；D2-20/21/23 已在 R29 内实施（BootStorage 半提交断言、删除 records_in_wire、SHIM_HEAP 64 KiB）。
- 结论：R27 的 34 条 backlog 经结构重写后，R29 仍找到 55 条（含 3 条 P1）。这不是
  “文档缺陷无限”，而是这一版流程的读者/设计检查面仍不足以接住长尾。V3.1 的 3 轮
  窗口用了 2 轮（R28、R29）；第 3 轮没有跑，而是按用户目标里的 5 轮总预算与
  “不要用同一 edition 无限刷”原则，记录 `FREEZE-WITH-BACKLOG`
  （`FREEZE-WITH-BACKLOG.md`）。冻结不把 R28/R29 宣传成“已收敛”，也不掩盖
  R27 台账已回填 70 行；QEMU 6 条中 5 条 PASS，riscv64 U-Boot 直载测试链被宿主 U-Boot 2025.10 bootflow 挡住（内核未被装载），该缺口不写成 PASS。

## 六、R30 结果（2026-11-10）

用户要求“再执行一轮”后，V3.1 第 3 轮 R30 作为完整 survey 执行：6 个角色 57 条发现
（FACT-A 4、FACT-B 6、READER-A 12、READER-B 15、READER-C 12、DESIGN-D2 8），
全部进入 `defect-ledger.md` 并在本轮关闭（① 10 / ④ 39 / ⑥ 8 / FP 0 / ② 0 / ⑤ 0）。
R30 的三条自引入（R29 改代码/表项未同步文档）被面板抓住并修掉。

R30 `--round` PASS；文档六门全绿；kernel 866 / boot-shim 136 / minix-boot 196 通过；
QEMU 5 PASS + 1 环境 SKIP。最终结论见 `CONVERGED.md`：按代理口径 A（已登记项全部
关闭）判 `CONVERGED`，不声称零未知缺陷。V3.1 的三轮窗口（R28/R29/R30）已用完。
