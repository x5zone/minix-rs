# edge4 — 协调板：并发规则 · 认领锁 · 依赖状态 · 集成验收（三线并行的共享底座）

> **定位**：本文件就是 edge_lock.md（锁与规则）——不另建 edge_lock 文件——外加跨线依赖状态板、E5 端到端联调编排与最终验收阶梯。edge1/edge2/edge3 是三条可并发执行的工作线，本文件不承载开发条目，只承载**规则、锁、状态与验收**。本文件与三个 edgeX.md 均为并行化加速修 todo 的**临时工具**；条目权威描述一律在原 todo 文件（[edge_todo.md](edge_todo.md) 与各 stage todo.md）。
>
> 三条线：[edge1.md](edge1.md)（内核·架构·QEMU）｜ [edge2.md](edge2.md)（共享库·运行时·驱动框架）｜ [edge3.md](edge3.md)（服务器·FS·net·命令）。

---

## §1 并发规则（三条线共同遵守，违反即 P0-process-violation）

1. **文件所有权**：每条线只允许修改自己文件头部「所有权」清单内的路径。清单外的触碰 = **跨界改点**，必须先在 §2 认领板登记（写明线、条目、意图、预计触碰文件），登记后才能动，动完销账。同一跨界改点同一时间只允许一条线持有。
2. **依赖等待**：条目标注的前置属于其他线时，开工前先读对方 edgeX.md 对应条目的状态列。不是 ✅ 就跳过它先做别的；**不允许代替对方实现其所有权内的前置**。等待期间在状态列标 ⏸ 并注明等谁。
3. **共享文件串行化**：以下文件任何线的修改都先在 §2 认领板登记一句话（避免同时编辑冲突）：
   - `os/Cargo.toml`（workspace 成员/依赖表；Cargo.lock 随动，机械冲突自行 rebase）
   - `os/qemu-tests/run_all.sh`（测试矩阵；目录内新增独立测试内核文件不需要登记）
   - `notes/rewrite/fork-syscall-rewrite/edge_todo.md` 与 `00-master-plan/`
   - `tools/`（守卫脚本、coverage-extract）
4. **进度记账**：每条完成后在本线 edgeX.md 状态列标 ✅ + 日期 + commit；解锁了其他线条目的，同时到 §3 状态板把对应行勾掉。**禁止直接回写 edge_todo.md / 各 stage todo.md**——那是当年为避免并发修改冲突才设立的单一入口，现在由 edge4 在里程碑节点（§8）批量收敛回写。
5. **回归纪律**：每条修完跑 `cargo test -p <触碰 crate>`；每收工跑一次本线所辖 crate 的全量回归；**测试内存铁律：`ulimit -v 3G` + `cargo test -j 1`**（防 WSL 宿主崩溃）。不整仓 `cargo fmt`（工具链漂移会污染 diff），只对新增代码手工保格式。clippy 对账：触碰 crate 的告警数不得高于既有基线。
6. **闭单勿领**：各线文件的「已闭单勿领」清单里的条目不再执行；对状态有疑义时以 edge_todo.md 内**最新进度注记**为准（stage todo.md 头部状态普遍滞后）。
7. **fix-guard / todo-fix 三段式照旧**：修复前读目标行 ±5 行、grep 核实现状、一次一条、讲明白（是什么/为什么）→ 多方案对比（Linux/Redox/OS 理论）→ 实施 + 测试 5 维自查。新发现的跨线条目：追加进所属线的 edgeX.md 并在 §3 状态板登记，不写进 edge_todo.md（收尾时统一并回）。

## §2 认领板（跨界改点 + 共享文件登记）

### 跨界改点（编号与 edgeX.md 引用一致）

| 编号 | 改点 | 属主线 | 需要触碰的他人领地 | 当前持锁 | 状态 |
|---|---|---|---|---|---|
| C-1 | edge2 L5 E-DEVWIRE 消费侧 | edge2 | `os/servers/vfs/src/cdev.rs`、`bdev.rs`（删本地常量副本改 import，小改） | 无 | ✅ 销账（2026-09-18，aecd4cd1c：vfs 删 12 个本地常量改消费 minix-types types/device.rs，u8 类型漂移裁正） |
| C-2 | edge2 L8 E-SDEVOWN vfs 副本 | edge2 | `os/servers/vfs/src/sdev.rs`（删 923 行副本，改消费 `minix-sockdriver`）+ `os/Cargo.toml`（新增 workspace 成员 `libs/minix-sockdriver`，§1 规则 3 登记流水同轮） | edge2 | 🔄 持锁（2026-09-18，方案 A2 新 crate 裁决：C 世界 libbdev/libsockdriver 两库并列，Rust 已有 minix-bdev，镜像位新建） |
| C-3 | edge3 S9 D-02 kernel 臂 | edge3（需求方） | `os/kernel`（SYS_GETMONPARAMS/GETIMAGE 对端，edge1 认领实现）+ `os/libs/minix-sys` wrapper（edge2 认领） | 无 | ☐ 待 edge1/edge2 排期 |
| C-4 | edge1 K17 E5(d) qemu 载体消费 VM | edge1 | 仅读 edge3 的 VM 语义/接口，不改 `os/servers/vm`；发现 VM 缺口回 edge3 状态板登记 | 无 | ✅ 销账（2026-09-19，载体 `test-paging-faultloop` 真机 PASS。VM 缺口登记：`minix-vm` 的 `pagetable`/`vm_self_map` 均 `pub(crate)`（lib.rs 导出面仅 VmServer/BootMemRegion/boot 类型），测试内核不可链接——载体以 arch 分页原语复现同一回路（adopt → map/query/unmap → #PF → 处理臂写硬件 PTE → 重执行），生产 `page_fault` helper（RTS 旗标 + VM_PAGEFAULT 消息）在载体臂真实走查；**edge3 待办**：真机联调（E5(d) 完整体）需要 VM 侧暴露可链接的分页冒烟面，届时载体臂可平滑换成真实 VM 参战） |
| C-5 | edge2 L2 E-SYSCALL-SIGN 回迁消费侧 | edge2 | `os/servers/is/src/acquire.rs`（`vfs_proc_tab_via` 撤本地符号归一，改走共享 `perform_taskcall`）+ 四处命令侧"待 sign 修复"注释卫生（`os/commands/bin/fileops/src/bin/echo.rs`、`os/commands/usr-bin/regex/src/bin_support.rs`、`os/commands/usr-bin/textfilter/src/bin_support.rs`、`os/commands/games/stdio-games/src/bin_support.rs`，仅注释）。IS 注释自证等待本裁决（acquire.rs:649-651） | edge2 | ✅ 销账（2026-09-18，0cca4247d） |
| C-6 | edge3 S6 D-16 DumpCore wire 按值携带名字（OQ-5 裁决 2026-09-19） | edge3（需求方） | `os/libs/minix-types`（`VfsCall::DumpCore` 成员改造：i32 path → 按值 name+len）+ `os/servers/vfs` 消费侧 | 无 | ☐ S6 开工时 edge3 持锁 |
| C-7 | edge3 S19 A-6 诊断缝共享 helper | edge3（需求方） | `os/libs/minix-sys/src/syscall.rs`（`sys_diagctl` 旁新增 `sys_diagctl_write` 字符串便利封装，仅加函数不改既有）+ `os/servers/rs`（SysApi::diag_write 缝） | 无 | ✅ 销账（2026-09-19，1f72d8ffe） |
| C-8 | edge3 S17 换装 minix-sys 补 | edge3（需求方） | `os/libs/minix-sys`：pm.rs 新增 `setuid_via`（PM_SETUID=5，raw 载荷）+ vm.rs `vm_rs_memctl_via` 扩 (addr,len) 参数（原版丢 HeapPrealloc/MapPrealloc 的地址对，零调用方扩参无涟漪） | 无 | ✅ 销账（2026-09-19，0e33c276c） |
| C-9 | edge3 S24 mib_get_label 接线 minix-sys 补 | edge3（需求方） | `os/libs/minix-sys`：ds.rs 新增 `DsClient::retrieve_label_name`（C ds.c:92-101：DS_RETRIEVE_LABEL + val_in.ep + key 写授权收标签名）；`os/servers/mib`：SysServices 持 DsClient 并接真实动词 | 无 | ✅ 销账（2026-09-19，bd5c3d008） |
| C-10 | edge2 L4 E-CDRCONV 判定核上收 | edge2 | `os/servers/input`：`src/framework.rs` 删除（判定核上收 minix-chardriver：GateVerdict/gate_character_request/AnnounceEffect/announce_effects/SELECT_*/ACCESS_*/TRANSFER_* 旗标），lib.rs/dispatcher.rs/init.rs/handlers.rs 改消费共享库，Cargo.toml 加 minix-chardriver 依赖 | edge2 | ✅ 销账（2026-09-18） |
| C-11 | edge1 K10 第四轮 SSWI：平台描述暴露 + arch IPI 消费 | edge1 | `os/libs/minix-boot/src/platform.rs`（PlatformDesc trait 增 `sswi_setip_base()` 默认方法）+ `os/libs/minix-platform`（device_tree.rs aclint-sswi/aclint-mtimer 解析 + global.rs 转发）。两库在三线所有权清单均未登记、功能上属内核 boot/platform 面；edge1 以 K10（riscv64 S-10 IPI 路径，smp_todo 归属）持锁 | 无 | ✅ 销账（2026-09-19，回路真机 PASS 5/5：trait 默认方法 + aclint-sswi/mtimer 解析 + global 转发 + arch `send_sched_ipi` SSWI 直写优先/SBI 兜底；hosted platform 17/boot 13/arch 237 全绿，riscv64gc target check 零错误） |
| C-12 | edge3 S26 RMIB 协议走查器：minix-sys walker 函数节点回调 | edge3（需求方） | `os/libs/minix-sys/src/rmib.rs`：`rmib_call` 增函数节点 handler 回调参（C `rmib.c:809-811` 的 `node->func(call,node,oldp,newp)` 面，现版此分支恒 `EOPNOTSUPP`）；`os/servers/ipc-server/src/boundary.rs` 的 `mib_process` 消费（COMMON_MIB_INFO/CALL 分发 + COMMON_MIB_REPLY 两分回复，libsys `rmib.c:1037-1080`） | edge3 | ✅ 销账（2026-09-19，53c709e09+a6d1fe2fd+本批 ipc-server 提交） |
| C-13 | edge3 S13 W5：ds_check 事件 key 回拷 | edge3（需求方） | `os/libs/minix-sys/src/ds.rs`：`DsClient::check` 在有事件时把 WRITE grant 登记的本进程内存（DS 写入的事件 key）回拷给调用者缓冲——C `ds_check`（ds.c:209-219）的 key 参数即此通道，VFS `ds_event` 靠 key 前缀分类 | edge3 | ✅ 销账（2026-09-19，S13 W5 批） |
| C-14 | edge1 K12b 双腿：诞生链入口 + trap ABI（riscv64 ecall / aarch64 svc）；编号正名——先登记者 edge3 的 C-13（S13 W5 ds_check key 回拷）居原号，本行自 edge1 提交信息中的 "C-13" 改列 C-14 | edge1 | `os/libs/minix-rt/src/{crt0.rs,init.rs}`（riscv64+aarch64 `_start` naked 入口与 rt_birth/DirectTrapSource 门拓宽）+ `os/libs/minix-sys/src/{arch_trap.rs,ipc.rs,syscall.rs}`（riscv64 ecall 与 aarch64 svc 传输：call nr 寄存器、0=KERNEL_CALL 消息腿、操作数/回程对）。minix-rt/minix-sys 属 edge2 领地；x86 现状（int-0x21/LSTAR 双腿）不动，仅新增两架构分支 | 无 | ✅ 销账（2026-09-19，双腿诞生链真机 PASS 各 3/3：argv/kerninfo=ready/MAIN OK/panic render 五标记全过；x86 腿零改动——hosted rt 53 + sys 229 全绿回归佐证） |
| C-15 | edge3 S23：TTY fkey 客户端 wrapper | edge3（需求方） | `os/libs/minix-sys/src/tty.rs`（新增）：`fkey_ctl_via`——C libsys `fkey_ctl`（fkey_ctl.c:11-28）的 `_taskcall(TTY, TTY_FKEY_CONTROL)` 直译（请求/回复双向 wire 已在 minix-types ipc/tty.rs 与 message.rs 臂）；IS 的 `FkeyCtlTransport` 生产实现消费（S23 片 2） | edge3 | 🔄 登记即动（2026-09-19） |
| C-16 | edge3 S23 片 3b：getsysinfo 快照类型上移 minix-types（A-4 单一权威）+ 四生产者对齐 | edge3（需求方） | `os/libs/minix-types`（`types/mproc.rs` 增 `MProcSnap`、新 `types/rs_snap.rs`、新 `types/ds_store.rs`、`types/vfs_snap.rs` 增 `DmapSnap`；四者 `[ARCH: A-4]` 单一权威）；消费方 `os/servers/pm`、`os/servers/rs`、`os/servers/ds`、`os/servers/vfs` 生产者按快照序列化；IS `dump_pm/dump_rs/dump_ds/dump_vfs` 改 import（删本地副本） | edge3 | 🔄 部分销账：PM 段已落（`MProcSnap` 上移 + PM 生产者换行 + IS 重导出，ab3e05128）；RS/DS/VFS-DMAP 三段随 S33 生产者对账（IS 侧五腿客户端 0ca5ef2bd 已备，只等生产者上行宽对齐） |

### 共享文件登记流水（append-only，登记 → 改 → 销账）

| 日期 | 线 | 文件 | 意图 | 销账 |
|---|---|---|---|---|
| 2026-09-18 | edge1 | `os/qemu-tests/run_all.sh` | K12：test-user-trap / test-rt-birth 纳入一键回归（特殊协议脚本区 + user-trap 入构建清单；rt-birth 内核因 `include_bytes!(env!)` 由其脚本自建，不入普通构建清单） | ✅ 同日 |
| 2026-09-18 | edge1 | `notes/rewrite/fork-syscall-rewrite/00-master-plan/` | K15：15-todo-fixes.md 阶段 2/3 状态对账回写（❌→✅ + 实际落地对账节；阶段 4/5/6 待 edge3 结论传递） | ✅ 同日 |
| 2026-09-18 | edge1 | `os/Cargo.toml` + `os/qemu-tests/run_all.sh` | K10：新增 test-smp-ipi-riscv64 carrier | 部分——Cargo.toml 成员已加（carrier 不入 run_all 矩阵：SSIE 回路未绿，见 edge1 K10 🔄 注记；绿后再接 run_all） |
| 2026-09-18 | edge1 | `tools/` | K16：新增 `tools/review-line-check.sh`（文档行号锚点批量反向核查：行存在性 + 当行内容回显） | ✅ 同日 |
| 2026-09-18 | edge1 | `os/Cargo.toml` + `os/qemu-tests/run_all.sh` | K17：test-paging-faultloop（E5(d) 缺页完整回路载体）入 workspace 成员 + x86_64 构建清单 + 特殊协议脚本区（gdbstub 邮箱断言，test-user-trap 同款） | ✅ 同日（真机 PASS 后接线完成并验证） |
| 2026-09-19 | edge1 | `os/qemu-tests/run_all.sh` | K10：test-smp-ipi-riscv64 回路真机绿后入 riscv64 构建清单 + 特殊协议脚本区（aclint=on 串口判定） | ✅ 同日 |
| 2026-09-19 | edge1 | `os/Cargo.toml` + `os/qemu-tests/run_all.sh` | K12b：test-rt-birth-riscv64 入 workspace 成员 + riscv64 构建清单 + 特殊协议脚本区（诞生链串口五标记判定） | ✅ 同日（真机 PASS 3/3） |
| 2026-09-19 | edge1 | `os/Cargo.toml` + `os/qemu-tests/run_all.sh` | K12b aarch64 腿：test-rt-birth-aarch64 入 workspace 成员 + aarch64 构建清单 + 特殊协议脚本区（诞生链串口五标记判定） | ✅ 同日（真机 PASS 3/3） |
| 2026-09-19 | edge1 | `os/Cargo.toml` + `os/qemu-tests/run_all.sh` | K11：test-shutdown-aarch64 / test-shutdown-riscv64 入 workspace 成员 + 两架构构建清单 + 特殊协议脚本区（exit code 双断言） | ✅ 同日（三架构真机全过） |

## §3 依赖状态板（跨线前置一览；各线开工前查这里）

| 上游条目 | 线 | 依赖它的条目 | 状态 |
|---|---|---|---|
| E1 trap 层 / E2 wrapper | 已闭环 | edge3 全部"通电"类（S1/S12/S17/S22/S25/S26/S27 等） | ✅ 2026-09-16/17 |
| edge2 L1 E-CMDSYSFACE | edge2 | edge3 S35/S36/S39（命令参数面、init no_std） | ✅ 2026-09-18 7cafb6233 |
| edge2 L2 E-SYSCALL-SIGN | edge2 | edge3 S35（宿主冒烟可信化）、S39 | ✅ 2026-09-18 0cca4247d |
| edge2 L10 文件族 wrapper | edge2 | edge3 S35（open 路径类命令批） | ✅ 2026-09-18 df145274c |
| edge2 L11/L12 sigreturn+panic-handler | edge2 | edge3 S39（init no_std 收口） | ✅ 2026-09-18 fda5a708c（sigreturn 函数半+panic 形式定形；裸桩地址半挂 edge3 S3/edge1 帧偏移，edge2 L11 行有登记） |
| edge2 L4→L5 E-CDRCONV→E-DEVWIRE | edge2 | edge3（vfs/input 消费侧，经 C-1）；16-stage G6 驱动 main | ✅ 2026-09-18（L4=bd06cab21 判定核单点；L5=aecd4cd1c 常量片+338bfdf9f A10 钩子 Result 化；C-1/C-10 销账） |
| edge2 L9 E-DMABUF 契约 | edge2 | edge3 S37（vm 实现） | ✅ 2026-09-18 757398407（契约就绪，S37 持 DmaMemory 行为实现即可） |
| edge3 S37 vm 传输面 | edge3 | edge2 L15（FS 二级缓存升级） | ✅ 2026-09-19 7ed891a9b（DmaMemory 行为实现落位；edge2 L15 可开工） |
| edge3 S17 RS 换装（枢纽） | edge3 | edge3 内部链 + edge4 E5 全族（RS 启动各服务器） | ✅ 2026-09-19（S17 主体 0e33c276c 起多批收口,edge3.md 已 ✅） |
| edge3 S19 A-6 裁决 | edge3 | edge3 S23（IS main 替换） | ✅ 2026-09-19 1f72d8ffe（diag 缝 = sys_diagctl code1 单汇点） |
| edge3 S27 sched 通电 | edge3 | edge3 S7/S10（PM 调度臂）+ edge4 E5(e) | ☐（真待做,S7/S10 随其后） |
| edge3 S29 通用 startup 框架 | edge3 | edge3 S31（lwip/uds） | ☐ |
| edge1 K9 向量表 | edge1 | edge1 K12b（三架构用户态）、edge4 T5 | ☐ |
| edge1 K1/K2/K3 SMP 面 | edge1 | edge4 E5(e)/E5 SMP 冒烟 | ☐ |

## §4 在制避让（2026-09-18 工作树现状）

工作树存在未提交修改，下列条目领取前先确认在制工作已落地，避免双持：

- `os/libs/minix-chardriver/src/driver.rs`、`os/libs/minix-blockdriver/src/driver.rs` 已修改 → 影响 **edge2 L4/L5**（T7 后续工作刚提交 7dfa450c2，疑有连 续在制）。
- `os/servers/ipc-server/tests/integration.rs` 已修改 → 影响 **edge3 S26**。**已落地**：fa668079d（2026-09-19，测试侧追平 CallHandler `&mut Message` 签名，S26 前置清除）。
- `tools/coverage-extract/ds-semantic-map.json` 已修改 → 工具面，edge4 侧知悉即可。

## §5 E5 端到端联调包编排（条目主体在 [edge_todo.md](edge_todo.md) E5；此处只做跨线编排）

| 子项 | 内容 | 前置（哪条线交付什么） | 执行载体 | 状态 |
|---|---|---|---|---|
| E5(a) | PM↔VM fork 全链路（走 minix-sys 消息层） | edge3 S1+S20+S17；内核 eager-CoW 已备 | `os/tests/`（宿主）+ 真机挂 T2 | ☐ |
| E5(b) | VM↔VFS fdclose 往返 | edge3 S12 + S20 | 同上 | ☐ S20 VM 侧已通电（2026-09-19），等 S12 |
| E5(c) | RS live-update 全链（PREPARE→UPDATE→resume） | edge3 S17+S18+S20 | 同上 | ☐ |
| E5(d) | QEMU VM paging 冒烟（含缺页完整回路 + VM 写 PTE） | edge3 S20 + edge1 K17 载体 | `os/qemu-tests/`（edge1 实现，edge4 验收） | ☐ |
| E5(e) | PM↔SCHED 调度链（START/INHERIT/NO_QUANTUM 回环） | edge3 S27 + edge1 K1/K2 | 真机（T2 后） | ☐ |
| E5(f) | DS 发布/订阅三链 + regex pattern 用例 | edge3 S22 + S17 | 真机（T2 后） | ☐ |
| E5(g) | MIB/sysctl 四链 + rmibtest 契约 | edge3 S24/S26/S33 + E-RMIBWIRE 通电 | 真机（T2 后） | ☐ |
| E5(h) | devman 生命周期四链 | edge3 S25 + S17 | 真机（T2 后） | ☐ |
| E5-SMP | 「fork 后父子并发写 CoW 页」陈旧 TLB 用例 | edge1 K3 + 真拓扑 | `os/qemu-tests/` -smp 4 | ☐ |
| E5-rt | 首个 no_std minix-rt 二进制三架构化 | edge1 K12b | qemu-tests | ☐ |

QEMU 测试内核与脚本统一由 edge1 在 `os/qemu-tests/` 实现（新增独立文件不需登记）；edge4 只持断言清单与 PASS 判定。

## §6 OQ 队列（等用户/联合裁决，任何线不得代决）

| OQ | 内容 | 影响条目 | 状态 |
|---|---|---|---|
| OQ-1 | endpoint_t 编码演进方案（A 减位宽 / B 扩 64 位 / C 解耦，Q1-Q10 十问） | 全线 IPC ABI；冻结期不动消息布局 | 待用户 |
| OQ-2 | E7 PmCall 枚举是否上移 minix-types（调用号收敛归属） | edge2 L3 后半 | 待用户 |
| OQ-3 | C-6 / [ARCH] A-6：/etc 配置面（rc 脚本 vs 编译期 Rust 数据面，结合 05 shell 交付裁决） | edge3 S35（05 批）、S39（init rc 链）、S34 载体 | ✅ 已裁决（2026-09-19 用户）：**盘上文件面**——/etc/rc、/etc/system.conf 等价物为盘上文件，VFS 通电后 init/RS 同源读取（C 对齐，Minix3 init exec /bin/sh 执行 rc）；落地时机随 T3/T4 阶梯 |
| OQ-4 | E-THREAD-MODEL 立项归属（14-stage-runtime 或 04-stage-pm） | edge3 S40 | ✅ 已裁决（2026-09-19 用户）：**归 14-stage-runtime 立项**（用户态线程模型 + libmthread 等价物；Minix3 libc 是 thread-stub、libmthread 是绿线程）；futex（全树零命中，属 [ARCH] 演进）为立项内子项，需 PM/内核协同再开跨界改点 |
| OQ-5 | D-16 core dump 路径契约（VFS 契约重设计） | edge3 S6 | ✅ 已裁决（2026-09-19 用户）：**`VfsCall::DumpCore` 按值携带进程名（name+len）**——内核内部协议（非用户可见 ABI）重设计属 Rewrite 合法自由，Rust 类型安全无裸指针问题，外部行为与 C 一致；minix-types 触碰经 §2 C-6 登记 |
| OQ-6 | GETUPTIME 的 C 对账（com.h:315-345 GET_* 家族无此号） | edge3 S4（ClockSource 生产实现） | ✅ 已裁决（2026-09-19 用户）：**C 对齐**——callnr.h 实证 7=PM_STIME、28=PM_GETTIMEOFDAY、33~36=PM_CLOCK_GETRES/GETTIME/SETTIME/GETRUSAGE；C 无名为 GETUPTIME 的 PM 调用，uptime 数值作 clock/times 内部供数（libsys/getuptime.c 语义），wire 中 GETUPTIME 命名按 C 真名修正 |
| OQ-7 | 04-stage [ARCH] A-2（13 篇 termios/terminfo 决策） | edge3 S35（05 sh 批先行） | ✅ 已裁决（2026-09-19 用户）：**移植 terminfo 解析器（Rust 重写）+ 盘上数据**（usr/share/terminfo 等价物，与 OQ-3 盘上文件面同构）；tput/tic/infocmp/6 游戏全语义，caps.rs 为两种结局共用地基 |
| OQ-8 | 规则集维护批：Pattern #84 落地 + Gate D 双实现模式 + 模式 85/86 + CSL 候选 + 05-stage §9.6 规则落档 | review 规则文件（工具面，edge4 代管） | 待批量维护会话 |

## §7 最终验收阶梯（目标：三架构在 QEMU 跑起来并执行 18-stage cmds）

| 阶梯 | 内容 | 主责 | 判定 |
|---|---|---|---|
| T0（已达成） | x86_64：boot+SMP 四核+user-trap+rt-birth；aarch64/riscv64：boot 冒烟各 7 项 | — | test-user-trap/test-rt-birth PASS、run_all.sh 存量绿 |
| T1 | 三架构用户态门槛：user-trap/rt-birth 入主线（K12）→ 两架构向量表（K9）→ rt-birth 三架构化（K12b）→ SSIE/shutdown（K10/K11） | edge1 | run_all.sh 三架构全绿 + 新增测试 PASS |
| T2 | x86_64 服务器通电链：S17 RS 枢纽 → S22 DS / S27 sched / S1 PM → S20 VM → S12 VFS+mfs → S23 IS / S24 MIB / S25 devman / S26 ipc-server | edge3 | 各服务器 main 去 panic/停车，E5 对应子项 PASS |
| T3 | init 真实运行：S3 PM 信号臂 + S39 init 收口 + rc 链（OQ-3 裁决后） | edge3 | init 进 multi-user 雏形，waitpid/信号端到端 |
| T4 | 18-stage 命令执行：S35 命令批 + S32 根镜像装机面（mkfs/diskimg）→ QEMU 内执行 echo/ls/cat/sh 等真实命令 | edge3 | 18-stage 命令冒烟脚本 PASS（宿主冒烟由 edge2 L2 保证诚实） |
| T5 | 三架构复跑 T2~T4 梯次 + SMP 正确性（K1/K2/K3 + E5-SMP） | edge4 编排 | 三架构 × 全梯次 PASS → §8 收尾 |

## §8 收尾与归档规则

1. 每达成一个阶梯，edge4 在 §3/§5/§7 勾账，并**批量**把涉及条目的状态回写 edge_todo.md 与对应 stage todo.md（按各文件原有回写体例），消除双真相窗口。
2. T5 全绿后：三个 edgeX.md 与本文件的条目表冻结，全量对账一次（grep 原文件复核无漏领），然后整体并入 edge_todo.md 归档段（或 edge_todo_archive.md），四份临时文件删除或在头部标注 ARCHIVED。
3. 过程中新发现的条目一律先进所属线 edgeX.md（§1 规则 7），收尾时随批次并回权威文件。
