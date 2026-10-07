# NK4-C 增量评审报告 R3.2（2026-10-04）

- **范围**：`4e3cab8d5..92fa446c3` = **286 笔**（截断面冻结于评审启动时刻；此后主线在制改动归 R3.3）
- **段构成**：评审线 SELF 5 笔（8d0a177ed R3.1 报告 + 79a1523ed R3.1 待办登记 + C-65 三连 d37282725/ea62373ff/92fa446c3 pattern-gate 增量轮三）；主线 AGT ~281 笔，其中 **62 笔触 os/ 生产码**（深审候选）
- **方法**：R3.1 协议延续 + code-excellence 四件套；P1 机械对账（python 权威扫描）+ 五域分批深审（3 个只读子代理收证 + 评审人裁决与 4 项载荷抽查）
- **状态**：**定稿**（P0-P5 全完成）
- **增量协议**：R3.3 = 自 `92fa446c3` 起；主线在制（`.wt/docfix-boot-chain` 文档分支 040909d9b + 主树 2 个 03-stage-rs 文档脏文件）出界
- **产物**：本报告 + `NK4C-R32-P1-AUDIT-TABLE-20261004.txt`（286 笔机器初筛表）

---

## 结论先行

1. **无虚构取证、无验证虚报**：286 笔 message 声明与 diff 对账零矛盾；保护路径（minix3/、AI-chats/、os/target/、os/.dockercargo）与规范源**零触碰**；TEMP 探针回滚干净（截断面树探针残渣 **0 命中**；a2d 探针 +104/−89 逐文件互逆、`git diff 3fafdbe04^ 484b5a12b -- os/` = 0 字节实证）。
2. **零 P0 / 零 P1-正确性**：46 项深审判定（覆盖 62 笔触码 commit + C-65 三连 + atf-compat 抽样）= **36 PASS + 10 PASS-with-NIT + 0 FAIL**。全部发现为 P2 级（G1-G10 + NIT）。
3. **里程碑全部独立复证为真**（评审自建仓外 worktree @ `92fa446c3`）：
   - docker 三件套 **kernel 833 / arch 243 / vm 535，0 failed**（R3.1 基线 831/243/531，只增不减 ✓）
   - x86_64 `-smp4` 真机 ×2 轮 **marker=2/2、panic=0、pfVM=0、vec6=0**
   - `test-cmd-smoke-aarch64.sh` **PASS**（目标② aarch64：marker + ls /bin + cat /etc/rc 走 VFS IPC）
   - `test-atf-aarch64.sh` **rc=0：36/36 = 34 passed + 2 skipped + 0 failed/broken**（目标③ aarch64 达成复证）
4. **三目标进程核对**（截至断面）：①marker x86✅/aarch64✅（续-132）/riscv❌（卡缺陷 (A)，TRANSIENT-PTE 理论取证中）；②命令面 x86✅/aarch64✅（续-252）/riscv 受 (A) 门控；③minix3 tests 上机 **aarch64 ✅（续-286，本轮复证）**，riscv 构建面 18/18 可链、上机受 (A) 门控。

---

## §一 P1 全量机械对账（286 笔）

### 1.1 方法与工具事件

R3.1 v2 方法复用（`git show --numstat` 四判据 + merge fallback + python 权威复核）。**v2 新增树级判据**：对截断面 `92fa446c3` 的 os/ 生产文件（rs/toml/ld/sh，排除 tests/qemu-tests）做探针残渣模式扫描——因为本区间主线的探针形态从「TEMP 用后即滚」扩展为三类：TEMP 已滚 / **入库仪表**（续-138附 声明入库）/ **台账待滚**（续-191 族登记 T8/AF-11 台账，(A) 结案统一滚除）。

工具链注记：本机 grep=ugrep 不可信（R3.1 已登记），全部安全攸关扫描走 python3 字节级；本轮另踩 `git show --numstat` 对非 ASCII 文件名输出 C 引号转义（`"...\346\242..."` 形态），初版 docs-claim 扫描误报 1 笔（c8eb3dd81，实为纯文档笔），已澄清。

### 1.2 结果（python 权威扫描）

| 判据 | 结果 |
|------|------|
| 保护文件触碰 | **零**（286 笔全扫；本区间 .dockercargo 无涉） |
| 规范源触碰 | CLAUDE.md / AGENTS.md / prompt/ / .claude/ 等**零触碰** |
| TEMP 探针残渣（截断面树） | **0 命中**——「已滚」声明全部属实；台账内仪表另计（见 G1） |
| docs 声明 vs diff | **0 真实矛盾**（1 误报=非 ASCII 路径转义） |
| 体量异常 | 1 笔 BIG+3184（f797120f3 续-251：WORKLOG +18 + tmp/nk4a gdb 取证转储 3098 + 扫描 harness 69——纯证据入库，裁决 ok） |
| merge 笔 | 0 |

### 1.3 旗标行裁决（86 行）

- **OS-CODE ×60+**：深审候选集（批A-E），归 §二。
- **CLAIM? ×8**：逐笔澄清——均为「WORKLOG+探针」类笔（探针入生产文件属声明内容，非零代码笔），**正则误报，ok**。
- **BIG ×1**：f797120f3（见 1.2）**ok**。
- **SELF ×5**：8d0a177ed / 79a1523ed notes-only；C-65 三连中 ea62373ff 触 tools/pattern-gate.sh + 证据文件（P2_TESTS 对账见 §2.5），**ok**。

---

## §二 深审判定（46 项）

> 行号锚点基准 = 截断面 `92fa446c3`（仓外隔离树实读 + `git show` 双通道）。载荷断言抽查（评审人本人复核）：续-109 SIE bit1 注释与实现、RegionMap 兜底 3 次封顶现态、atf 门判定逻辑（EXPECTED/n_skip/fail 否决/诚实分账）、P2_TESTS 43→64 数组行级解析——全部证实。

### 2.1 批A riscv64 装机面+启动链（8 笔：4 PASS + 4 PASS-with-NIT）

| commit | 判定 | 关键证据 |
|--------|------|----------|
| 0d9032f77 续-88 bootface | **PASS-with-NIT** | table.c:44-64 12 模块名序逐字对位 + NAME_POOL const 断言；`.rodata` 堆实修（IMAGE_HEAP static mut）；NIT=dtb_pa+total 未 checked_add（续-89 已闭 in_memmap_range 侧） |
| 13ee80fe0 续-89 boot-full | **PASS** | mfs build.rs arch 门三架构展开正确；源直拷两道闸 SAFETY 自洽 |
| ecde58862 续-90 split_huge | **PASS-with-NIT** | Sv39 非叶 R=W=X=0 合规格；child_pa 移位括号防优先级；NIT=无专属单测（宿主不编译结构性）+ leaf_bits 高位继承（Svpbmt 面规格瑕疵，QEMU 无害） |
| 8293eaaec 续-91 OpenSBI 保留 | **PASS-with-NIT** | clip_firmware + 3 条 const 断言编译期真钉；NIT=**message「删死条件 len==0」与 diff 不符**（实改 MEMMAP_MAX return→continue，len==0 仍在）——G4 |
| a943eac9f 续-103 RS 出生链 | **PASS-with-NIT** | medany `la`+delta 标准手法；text X 三架构增量论证成立；NIT=W^X 违例（N2 已自登记未闭） |
| 93bde0b6d 续-105 PLIC claim | **PASS** | stval 算术逐字复算成立（0xC000000+0x200004+1×0x1000=0xC201004）；S-mode timer 不跨 PLIC 为规范事实；x86 claim()=None 对位 |
| a5009c62e 续-109 SIE bit | **PASS** | **sstatus 位图核符**：SIE=bit1/SPIE=bit5/SPP=bit8（特权规格卷 II）；INIT_TASK_SSTATUS=0x100→sret 后 SIE=0 的差分闭环；`csrs`+wfi 语义正确 |
| 44c400229 续-125 asm 硬化 | **PASS** | checked_sub expect 与 release overflow-checks 解耦，启动路径正确硬化姿势 |

### 2.2 批B aarch64 里程碑（3 笔：3 PASS-with-NIT）

| commit | 判定 | 关键证据 |
|--------|------|----------|
| 15841fb17 续-129 FPSIMD | **PASS-with-NIT** | C proc.c:1922-1959 copr_not_available_handler 逐段对位；极性核实（C enable=置 TS 陷阱 vs Rust enable=可用，注释自洽）；v8-v15 豁免附机器码级论证；NIT=豁免不变量无回归测试（自登记 AF-1）+ release 不可达路径静默 enable |
| 9fc7c88f7 续-130 屏障+EINVAL | **PASS-with-NIT** | 续-22 同族第四腿补齐（send/sendrec/sendnb/kernel_call 五腿全集；receive 反向不需裁决有据）；被删 CPACR orr 0b11 实为真 bug（diff 原文可证）；NIT=未接线 init 的保留代码（登记形状） |
| 2a6c55912 续-132 PL011 | **PASS-with-NIT** | DR@0x00/FR@0x18/TXFF=bit5 与 ARM PL011 DDI 0183 逐字一致；wire 对偶 m_lsys_vm_map_phys 与 C ipc.h:1504-1510 同布局；**NIT=页映射为 Normal cacheable 无 DMB/device 属性**（MAIR 只配 Attr0、PTE ATTRINDX=0——QEMU TCG 有效、真硬件潜伏，未显式登记平台偏差）——G6 |

### 2.3 批C riscv (A) 取证链+生产码混编（14 项：12 PASS + 2 PASS-with-NIT）

| 项 | 判定 | 要点 |
|----|------|------|
| 1cb096071 续-149 乒乓止血 | **PASS-with-NIT** | 止血补丁带真实回归面（吞 PM_EXEC_NEW），已被续-153 收窄取代；**NIT=本笔 C 锚点注释错误**（称 C 不回 ENOSYS，实回 main.c:101-110）——G5 |
| 8a9a44933 续-153 收窄 | **PASS** | 丢弃条件收窄 + C 锚点注释纠正 + 两向测试钉（43 穿透/未注册静默） |
| 934af1bb2 续-160 RegionMap 兜底 | **PASS-with-NIT** | **专项裁决见 §三.2**；兜底/重建触发条件=键序不变量破坏或区域重叠（两病不可分辨），3 次打点后永久静默 |
| a10c6d52e/cb11d2eef/af7085f0a 续-168/175/180 扣减三连 | **PASS ×3** | 同一真因渐进逼近终态（整池硬编码扣减）；gh46/53/49 真机证据链完整；G3=池界魔数与 bootface 双写（无共享常量） |
| 33ed4eb45 续-179/186 守卫+自填 | **PASS** | C manager.c:2108 数组写无检查 vs Rust 越界 panic——守卫方向正确；VM 自填对位 pagefaults.c:88-89；P2=pub_wire 对称守卫缺位（G7） |
| e6aa68f07 续-187 classify | **PASS** | VM blob 不再让渡 exec 复用（gh53/58 链）；C 无对位（Rust boot 架构自有缺陷），代价 625KB 已注明 |
| f90416ccf 续-192 SIGSEGV 真身 | **PASS** | U 态 ecall 误判 IPC 机理实读闭环（early_console.rs 裸 SBI ecall + paging.rs S/U 共用）；净删 -46 全为陈旧探针；**EarlyConsole 特权护栏 CONSIDER 未落（G1 组成）** |
| ef3c40c6b 续-197 AF-8/9/10 | **PASS** | 与 RETRO-AUDIT 原文三方对账一致（条目↔diff↔测试）；AF-10 纯函数测试改回 Initial 两断言均红 |
| 50447d08e/1d98ef785 诊断增强 | **PASS** | 纯打点零行为变化（已入 T8 滚除台账） |
| a2d 探针回滚链 | **PASS** | +104/−89 逐文件互逆，`git diff 3fafdbe04^ 484b5a12b -- os/`=0 字节 |
| 续-191/191b/191c 探针族 | **PASS（注册态）** | +189 行至截断面未滚——**但已登记 T8/AF-11 台账**（(A) 结案统一滚除 + `git show --stat` 证代码真动纪律）；规模实测 33 文件/193 处 nk4a: 标记——G1 |
| f89b7c731 续-133 拆分 | **PASS** | 生产码（懒 FPU/FP-free 内核门/NS16550A）与探针（rvfpu-trap，台账内）边界清晰 |

### 2.4 批D 目标③ aarch64 上机 pipeline（14 笔：13 PASS + 1 PASS-with-NIT）

| commit | 判定 | 要点 |
|----|------|------|
| d33c24fd7 续-277 上机腿 | **PASS** | NoReply 臂=system.c:76 EDONTREPLY 契约的合法形态（三架构一致）；真机 _exit 兑底腿 panic 定谳在案 |
| 84398284a 续-277b stat 翻译层 | **PASS** | P0 根因（FS 直写 picolibc 120B 对象越界 32B 砸 saved x29/x30）与修法（本帧 raw+赋值式翻译）正确；越界不可能化 |
| 85c12c6e2 续-277c 对账测试 | **PASS** | host 测试逐宏 assert offset_of/size_of——真钉布局漂移；补回被重构删掉的 _Static_assert（诚实补账） |
| 51ac9e5c5/7e8875975 续-278/278b | **PASS ×2** | ATF_SUITE 提取器 assert 拒静默少播；门进度信号改终集 total（语义对齐） |
| f2619227e/5216a20d6 续-278c/279a | **PASS ×2** | 纯诊断面修复；检查位置（拷贝后）正确 |
| 3053c03e3 续-279c 池 2048 | **PASS** | C 锚点真实（mfs Makefile:11 DEFAULT_NR_BUFS=1024）；预算恒等式论证成立 |
| 196a17cd9 续-279d x86 分档 | **PASS-with-NIT** | 真机二分干净；**分档=诚实的回归遏制而非架构属性**（x86 限点机制未坐实，注释已自降级）——G8 技术债追踪 |
| f56689ce9 续-279e-note | **PASS** | 两处过度声称降级=样本级诚实性修复 |
| 4994feddd 续-279m brk+split | **PASS** | region.c:391 hint/：1163-1182 split memtype 逐行对位；split P0 修复带 2 回归测试；本区间最大笔中质量最高 |
| c8eacdb91 续-279m-b errno | **PASS** | reply_to_errno 正 errno 车道核实（vm_server.rs:1724+encode.rs:94）；C sbrk.c:20 回绕保护对位 |
| f9e300302 续-286 atf 门 | **PASS** | ^skipped 行首锚定防假绿；fail/broken 一票否决+诚实分账；**本轮独立复跑 rc=0（§五）** |
| ec6ba05d3 续-286-b 回执 | **PASS** | 两 P2 修复落点核实在脚本现码（:185-186） |

### 2.5 批E 脚本/tools/抽样（7 项：全 PASS）

- **260d3daf4+af5332b0d** aarch64 cmd smoke：假绿防三处实在（SKIP 语义/marker 后窗口/-net none）——本轮独立复跑 PASS。
- **6d485d4ca** AF-5 mock 修复：暴露 CI 盲区（-p 子集掩盖 tests/ 断裂）登记正确。
- **719716dab** AF-12 文案：三处锚点 off-by-one 全树改齐。
- **atf-compat 抽样 3 组**：MD5Update 进位（与 RFC1321 回绕检测等价性数学核验）；bm.c 逐字 port（与 minix3/lib/libc/string/bm.c 全文件比对属实）；sys_nerr 边界（C errlist.awk:105 公式对账，135=表项数、[0,sys_nerr) 判域）。
- **f797120f3** BIG 笔：notes/tmp 证据入库，os/ 零足迹。
- **C-65 三连**：触面 tools/pattern-gate.sh+证据+notes；**P2_TESTS 43→64（+21）逐名解析对账成立**，新增 21 名与 R3.2 深审各笔测试一一对应（brk×9/split×2/PL011×4/stat 对账/lazy_fpu/atf suite）——跨线互钉良好。注：证据文件落仓库根 `tmp/evidence/20261004-c65-pattern-gate/`（R3.1 F7 同款位置偏离延续，G10 附记）。

---

## §三 架构专项与自审抽查

### 3.1 [ARCH] 合规

本区间 os/ 新增 `[ARCH:]` 标注 2 处，均引用**既有**登记项 A-12（brk resize 腿，续-279m）——非新架构演进声明，引用合规。续-88/90/129 等装机面与 FPSIMD 落地未标 [ARCH] 属正确判定（镜像既定三架构设计/补全非演进）。

### 3.2 RegionMap 防御性兜底专项裁决（code-excellence）

**裁决：取证期正确、终态必须清场（G2）。** 理由链：①兜底触发即「range 派生与 iter 线性扫不一致」=BTreeMap 键序不变量破坏或区域重叠，两病不可分辨——在「真损坏」世界观下 panic-fail-fast 是正解；②但后续证据链（续-163 sas 探针→续-192→TRANSIENT-PTE 理论 v2）把损坏源收敛到**视图层瞬态**（RAM 恒净），兜底实际补偿的是读故障——取证期保活+打点是正确工程决策（panic 会在真修落地前打死机器）；③终态问题：3 次打点封顶后**永久静默**=可观测性最弱组合，且「键序破坏」与「插入期重叠 bug」在兜底处不可分辨——若理论有误（真损坏复发），第 4 次起完全失明。**处置建议**：(A) 结案清场时二选一——删（理论定谳则兜底是对非 bug 的 workaround）或改 panic fail-fast；最低限解封计数+持续遥测。已入 T8 台账范围，评审登记确保不被遗忘。

### 3.3 主线自审抽查（防虚构）

- **RETRO-AUDIT（untracked 本机件，203 行）**：AF-5/8/9/10/12 五项闭环与落地笔一一核实（6d485d4ca/ef3c40c6b/719716dab）；AF-11（探针清理）诚实未做；AF-1/2/3/4/6/7 登记/观察态与代码现态一致。**抽样无虚构**。
- **续-286-b 评审回执**：两 P2 修复落点核实在 atf 门现码；判定逻辑零改动声明与 diff 一致。
- **SD 总册 / TRANSIENT-PTE**：体量核实（1405/795 行）；C-65 已做 untracked 首扫。**耐久性风险见 G10**。

### 3.4 R3.1 待办消费核对

79a1523ed 将 R3.1 发现登记为 WORKLOG 待办（F1-F10+6 NIT，声明「择机顺路认领本轮不动」）。核对：区间内 riscv-reviewlog 零改动（F1 未动）、ipc.rs:1150 注释原样（F3 未动）、kernel-image 交付边界节未更新且 boot-handoff 字面 0 命中（F4 未动）——**登记声明与事实一致**，全部待办仍开放，顺延至主线排期。

---

## §四 卫生审计

1. 保护文件与规范源零触碰（§1.2）。
2. 探针三类形态分明：TEMP 已滚（树残渣 0）/入库仪表（声明）/台账待滚（T8/AF-11，33 文件 193 处）——「探针已滚」不再是全局单一声明，P1 判据已按三类区分（Rule Discovery #3）。
3. message 精确度：145→286 笔规模下仅 1 笔过度声明（G4 续-91）+1 处 C 锚点注释错误（G5 续-149，后笔自纠）——精确度持续高于基线。
4. docker 构建面 1 条存量警告（minix-types ipc/vfs.rs:782 unnecessary unsafe，R3.1 截断面已在，非本区间新增）。

---

## §五 独立验证（隔离 worktree @ 92fa446c3）

**环境事故与披露**：首轮验证全败——根因链 = `os/Cargo.lock` **不入库**（主树靠磁盘 lock+本地缓存离线解析）× 当前环境**网络不通**（crates.io 30s 超时，docker 与宿主同败）× atf 门需 vendored picolibc 工具链（脚本路径在仓库根 tools/ 而非 os/tools）。经三轮修复（拷主树 Cargo.lock + `CARGO_NET_OFFLINE=true` + 挂 host cargo registry 进镜像 `CARGO_HOME=/usr/local/cargo` + 拷 tools/vendor 与预构建 atf 产物）后全绿。此事件本身构成 Rule Discovery #1（可复现性债）。

| 项 | 结果 | 判据 |
|----|------|------|
| docker 三件套 | **kernel 833 / arch 243 / vm 535，0 failed** | 只增不减：831+/243+/531+ ✅ |
| x86_64 -smp4 ×2 | r1/r2 **marker=2/2**，panic=0 pfVM=0 vec6=0（9887/9891 行） | marker≥1、三零 ✅ |
| aarch64 cmd smoke | **RESULT: PASS**（marker + ls /bin + cat /etc/rc，VFS IPC 路径） | 目标② aarch64 复证 ✅ |
| test-atf-aarch64.sh | **rc=0：36/36 = 34 passed + 2 skipped + 0 failed/broken** | 目标③ aarch64 达成复证 ✅ |

---

## §六 结论与移交

### 6.1 总判定

- **零 P0 / 零 P1-正确性 / 零违规**；深审 46 项：36 PASS + 10 PASS-with-NIT + 0 FAIL。
- 本区间是项目迄今质量密度最高的增量之一：三目标从 1.5/3 推进到实质 2.5/3（aarch64 三线全通），riscv64 从零到 12 模块 exec；46 项深审中 C 锚点/硬件规格核验（RISC-V sstatus 位图、PLIC 算术、PL011 寄存器图、Sv39 非叶语义、ARM DDI 0183）逐字实征比例极高；两处 P0（stat 越界、split 丢 memtype）根因与修法均带回归测试与真机定谳。
- 发现全部为 P2 级（G1-G10 + NIT），其中 **G1（探针台账）与 G2（RegionMap 兜底）是 (A) 结案清场的强制项**，建议主线把二者绑入结案清单防遗忘。

### 6.2 登记待办

| # | 级别 | 内容 | 锚点 |
|---|------|------|------|
| G1 | P2-hygiene（系统性） | nk4a 探针台账：33 文件/193 处（截断面实测）；T8/AF-11 已立滚除纪律但**触发点未定**；续-192 已实证陈旧探针可致命（U 态 SBI ecall→SIGSEGV）；EarlyConsole 特权护栏 CONSIDER 未落——建议：(A) 结案清场绑定 + 护栏提前落 | T8/AF-11 台账；early_console.rs |
| G2 | P2-design | RegionMap find 兜底/pop_first 重建：3 次打点后永久静默；(A) 结案必清（删或改 fail-fast，最低限解封遥测）——详见 §三.2 | region_map.rs:51-124 |
| G3 | P2-design | VM boot.rs 池界魔数 [0x82000000,0x84000000) 与 bootface 双写（无共享常量/handoff 字段；bootface 改布局则 VM 静默错扣） | boot.rs:296-298 |
| G4 | P2-doc(message) | 续-91 message「删死条件 len==0」与 diff 不符（实改 MEMMAP_MAX return→continue） | 8293eaaec |
| G5 | P2-doc | 续-149 C 锚点注释错误（称 C 不回 ENOSYS，实回 main.c:101-110；续-153 已纠正，正文注释已改但 message 留痕） | 1cb096071/8a9a44933 |
| G6 | P2-arch（平台偏差） | PL011 页映射 Normal cacheable、无 DMB/device 属性（QEMU TCG 有效、真硬件潜伏）——建议显式登记平台边界或落 Device 属性 | serial.rs:137-293；paging.rs MAIR |
| G7 | P2-test | FPSIMD v8-v15 豁免无回归测试（自登记 AF-1）；RS pub_wire 对称守卫缺位（slot∈[64,256) 仍 panic）；PL011 S2 PA≥4GiB EINVAL 无测试 | 各 commit |
| G8 | P2-arch | x86 池分档=回归遏制非资源约束（限点机制未坐实）；riscv 从未以 2048 boot——(A) 解锁后第一验证项；建议「删除分档」显式追踪 | alloc.rs:165-180 |
| G9 | P2-doc | R3.1 F1-F8 全部未动（本轮声明「不动」一致）；其中 F1（riscv-reviewlog §A 同步）/F3（ipc.rs:1150）/F4（kernel-image 双架构+boot-handoff 标注）复确认仍开 | riscv-reviewlog.md 等 |
| G10 | P2-hygiene（耐久性） | RETRO-AUDIT（203 行，驱动 P0 落地的编号锚）与 STRUCTURAL-DEBT-REGISTER（1405 行）**untracked**——主机迁移即失，且已提交 message（续-197 等）引用其编号；建议入库或显式归档裁决 | git status 未跟踪清单 |

### 6.3 R3.3 增量协议

- 已审界推进为 `92fa446c3`；范围 = `git rev-list --count 92fa446c3..<届时 HEAD>`；方法同本轮。
- 重点预告：主线在制的 riscv (A) 攻坚（TRANSIENT-PTE 理论验证与 z 批扩样）、docfix-boot-chain 分支合入、(A) 结案清场轮（G1/G2 验收点）。

### 6.4 Step 5.7 Rule Discovery

1. **os/Cargo.lock 不入库 × 离线环境 = 可复现性债**：任何新 checkout（新机迁移/CI/评审独立验证）在网络不可用时无法构建——本轮评审验证首轮全败即此因。建议：提交 Cargo.lock 或 vendor 依赖 + 文档化离线流程（`CARGO_NET_OFFLINE` + vendored 工具链位置）。
2. **止血-收窄链模式**：快速止血笔（续-149）常带瞬态回归与不完美注释，后笔（续-153）收窄并自纠——评审协议应显式按「链终态」判定并回溯标注中间笔，避免对单笔误判 FAIL。
3. **探针状态三分法**：TEMP 已滚 / 入库仪表 / 台账待滚（T8/AF-11）——「探针已滚」不再是全局单一声明；P1 机械对账的探针判据需区分三类，滚除验收已有纪律（`git show --stat` 证代码真动）但缺触发点绑定。

### 6.5 验证局限（同 agent 披露）

本评审为单 session 同 agent（zcode/GLM）：P1 权威扫描独立于 ugrep（python 重跑）；深审证据由 3 个只读子代理收集，评审人载荷抽查 4 项（SIE 位/RegionMap 现态/atf 门判据/P2_TESTS 数组）+ C-65 计数对账全部证实；独立验证（§五）为评审人自建环境实跑（含环境事故三轮修复）。跨 agent 交叉验证本轮不可用；§五 命令可重放（需先解决 Rule Discovery #1 的离线前提）。

### 6.6 产物清单

- 本报告：`NK4C-REVIEW-REPORT-R32-20261004.md`
- P1 对账表：`NK4C-R32-P1-AUDIT-TABLE-20261004.txt`（286 笔机器初筛 + 旗标）
- 评审线进度账：`.review/zcode/edge5/REVIEW-R31-STATE.md`（R3.2 段，本地 gitignored）

**lint 口径**：本报告标题日期行 1 条 SL-4 与 R3.1/R3 报告同款先例基线；P1 表 SL-4 全部来自 verbatim 引用的 commit message（保真优先）。
