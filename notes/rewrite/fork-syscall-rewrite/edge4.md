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
| C-3 | edge3 S9 D-02 kernel 臂（并入 S42 ② 执行） | edge3（需求方） | `os/kernel`（SYS_GETMONPARAMS 对端 + boot_image 表真值；GET_IMAGE 臂已在 987773bb3）+ `os/libs/minix-sys` wrapper | 无 | ✅ 销账（2026-09-20，S42 批一：kernel `getinfo_mon_params` 真拷贝——恒 `sizeof(kinfo.param_buf)`=1024 零填充[do_getinfo.c:143-146]，`kernel_info()` 缺席仍 EINVAL；minix-sys 补 `sys_getmonparams`/`sys_getimage`（syslib.h:187/:184 宏对位）+wire 测试；boot_image 表真值半无需改动——`build_boot_image` + `getinfo_image` 既有即真值） |
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
| C-15 | edge3 S23：TTY fkey 客户端 wrapper | edge3（需求方） | `os/libs/minix-sys/src/tty.rs`（新增）：`fkey_ctl_via`——C libsys `fkey_ctl`（fkey_ctl.c:11-28）的 `_taskcall(TTY, TTY_FKEY_CONTROL)` 直译（请求/回复双向 wire 已在 minix-types ipc/tty.rs 与 message.rs 臂）；IS 的 `FkeyCtlTransport` 生产实现消费（S23 片 2） | edge3 | ✅ 销账（2026-09-20 卡L 复核：`fkey_ctl_via` 自 S23 片 2 在库且被 IS `SysFkeyCtl` 消费、input 侧 `do_fkey_ctl` 对端落地 bcfa02514，通道两端齐备） |
| C-16 | edge3 S23 片 3b：getsysinfo 快照类型上移 minix-types（A-4 单一权威）+ 四生产者对齐 | edge3（需求方） | `os/libs/minix-types`（`types/mproc.rs` 增 `MProcSnap`、新 `types/rs_snap.rs`、新 `types/ds_store.rs`、`types/vfs_snap.rs` 增 `DmapSnap`；四者 `[ARCH: A-4]` 单一权威）；消费方 `os/servers/pm`、`os/servers/rs`、`os/servers/ds`、`os/servers/vfs` 生产者按快照序列化；IS `dump_pm/dump_rs/dump_ds/dump_vfs` 改 import（删本地副本） | edge3 | ✅ 销账（四段全落：PM `MProcSnap` ab3e05128、DS `DsEntrySnap` 08d0b825d、RS 两表 `RprocpubSnap`/`RprocSnap` 39b4d06f5（VM `RS_INIT` 握手同步改按同一行解码、C 偏移表 rproc/rprocpub 652 行随删）、VFS `DmapSnap` 生产者 29ed365a5；IS 五腿客户端 0ca5ef2bd 端到端待 VFS 应答复位＝S12） |
| C-17 | edge3 S25 余件：devman 客户端生产传输 | edge3（需求方） | `os/libs/minix-sys/src/devman_client.rs`：新增 `SysClientTransport`（`ClientTransport` 的生产实现——`grant` 走 `GrantTable::grant_direct`（C `cpf_grant_direct` CPF_READ）、`revoke` 走 `GrantTable::revoke`、`sendrec` 走 `DirectTrapTransport`）；仅新增类型与 impl，不改既有签名 | edge3 | 🔄 登记即动 |
| C-18 | edge3 S12 对话臂：magic grant（VFS 把用户缓冲交给 FS 写） | edge3（需求方） | `os/libs/minix-sys/src/grant.rs`：新增 `GrantTable::grant_magic`（C `cpf_grant_magic` — safecopies.c:198-220：`who_from`/`who_to` 分离 + `CPF_MAGIC` 提交字，与既有 `grant_direct` 同纪律）；消费方是 VFS 的 REQ_READ/REQ_STAT 臂（request.c:844/1087） | edge3 | 🔄 登记即动 |
| C-19 | edge2 L15 E-FSVMCACHE 消费侧：mfs 零拷贝二级缓存注入 | edge2 | `os/fs/mfs/`：内部签名里 `NoSecondLevel` → 本线新类型 `MfsSecondLevel`（策略枚举：关 / VM 面）；新增 `src/vm_wire.rs`（`VmCacheTransport` 的 minix-sys 实现：薄适配 `map_cacheblock_via`/`set_cacheblock_via`/`forget_cacheblock_via`/`clear_cache_via` + `mmap_via`/`munmap_via`）；装配按既有 `BootConfig.use_vmcache` 选策略，unmount 的 `vm_clear_cache` 经 `invalidate_device` 出。**前置核验（2026-09-19）**：edge3.md S38 行仍 ☐，但其交付物（VM 侧四 handler + minix-sys 四封装）在 HEAD 已存在并全绿（`os/servers/vm/src/ipc/cache_handlers.rs` 四 handler、dispatcher.rs:694-697 路由、minix-sys vm.rs:618-706 四封装；`cargo test -p minix-vm --lib cache` 38 passed、minix-sys cacheblock 3 passed），故 L15 按「前置在代码中已满足」开工 | 无 | ✅ 销账（2026-09-19，8b8b495a7：mfs 内部签名全量换 `MfsSecondLevel`，新增 `src/second_level.rs`（`VmWire` 线上真货 + 策略枚举 + 测试替身），装配按 `BootConfig.use_vmcache`，卸载清空经 `invalidate_device` 出；mfs 131 测试全绿） |
| C-21 | edge3 卡C S7/S10 SCHED 客户端 wrapper 补 | edge3（需求方） | `os/libs/minix-sys/src/pm.rs`：新增 `sched_inherit_via`/`sched_set_nice_via`（同既有 `sched_start_via` 纪律） | edge3 | ✅ 撤回（2026-09-20 卡C 盘点更正：PM 侧惯例是**手工组消息 + 本地 `ipc::IpcTransport` trait**——`vm_fork` 先例，PM 的 transport 不绑 minix_sys trait，wrapper 无消费方；消息结构 `MessLsysSchedSchedulingStart`/`MessPmSchedSchedulingSetNice` 已在 minix-types，够用） |
| C-20 | edge2 L15 常量单点：VMC_NO_INODE/VMSF_ONCE 上收 minix-types | edge2 | `os/servers/vm/src/page_cache.rs`：本地 `pub(crate) const VMC_NO_INODE`/`VMSF_ONCE`（page_cache.rs:41-48）改为再导出 `minix_types` 权威（C 侧二者同出 `minix3/minix/include/minix/vm.h:90,93`，VM 与 FS 共用同一头文件）。仅换来源、零语义变更：`crate::page_cache::{...}` 消费点（ipc/cache_handlers.rs:20）与 crate 内引用不动 | 无 | ✅ 销账（2026-09-19，8b8b495a7：page_cache.rs 两常量改 `pub(crate) use minix_types::vm_cache::{...}`，消费点零改动；minix-vm 522 测试全绿） |
| C-21 | edge3 卡 J（S39）：minix-crypt 新成员 + init 的 no_std 特性面 | edge3（需求方） | `os/Cargo.toml`（workspace 成员新增 `commands/sbin/init` 域的 `minix-crypt` crate；minix-rt 的 workspace 级 `default-features` 声明裁决——init no_std 翻转的 E0152 死结见 09-stage-init/todo.md P1-2 ③）；`os/libs/minix-rt`（仅当裁决走 workspace 级特性声明时由 edge2 执行，init 不直接触碰） | edge3 | ✅ 全销账（2026-09-20 卡 J 收口会话：minix-rt workspace 级声明随 6fd4d2ec9 落地——workspace.dependencies 翻 default-features=false，五个 std 消费方均显式 features=["std"] 特性集不变，freestanding 二进制继承无 std 形态；init 侧 no_std 翻转随 7147b945c——cfg_attr 门控 target_os=none，minix-rt 按 target 分形态依赖〔hosted std / none alloc-global〕，两形态均不取 panic-handler〔init 自带 handler〕。minix-rt 源码零改动，禁触清单完好。移交注记：minix-rt 在 not(std)+无 panic-handler 特性下 lib.rs:64 的 use core::panic::PanicInfo 空置告警，归 edge2 随件清理） |
| C-21 | edge3 卡D S29：fs 服务器运行时 crate（`os/fs/fs-rt` 新成员） | edge3（需求方） | `os/Cargo.toml`（workspace 新增成员 `fs/fs-rt`，§1 规则 3 登记同轮）；minix-fs/minix-sef/minix-sys 均只消费不改动（minix-fs 的 `FsTransport` 缝按其库内文档即"服务器运行时"所有，生产 transport 落 edge3 侧新 crate） | edge3 | 🔄 登记即动（2026-09-20，卡 D 开工） |
| C-22 | edge3 卡 I（S33）：MIB 取表半的 wire 权威扩展 | edge3（需求方） | `os/libs/minix-types`：`MProcSnap` 尾部追加 `mp_started`（76→88 字节）、KERN_PROC2 尾段 `mp_svuid`/`mp_svgid`/`mp_child_utime`/`mp_child_stime`/`mp_ngroups`/`mp_sgroups[16]`（→184）与 KERN_PROC_ARGS 尾段 `mp_endpoint`/`mp_frame_addr`/`mp_frame_len`（→200 字节，既有偏移均不动）+ `mp_flags` 位值权威上收（PM crate 本地表改 re-export）+ `proc_info.rs` 的 RTS 位值修正（SENDING/RECEIVING 误记 0x100/0x200 → C 真值 0x04/0x08，当时无消费者）+ 补全 RTS 全组常量 + `KERN_PROC_TTY_NODEV/REVOKE` 哨兵与 `ARG_MAX`/`PAGE_SIZE` 权威、`PS_STRINGS_SIZE`（ps_strings.rs）。**kernel 行扩展（`p_dequeued`/`p_cpuavg`/`p_magic` 生产半）挂 edge1**：`os/kernel/src/misc.rs` 的 `ProcInfoStruct::from_kprocess` 需填真值——落地前 MIB 的 `l_slptime`/`l_pctcpu`/`l_cpticks` 如实回 0（A-7 登记，非静默分歧）；VFS light 行 wire 权威（`FprocLightSnap` + `misc.rs` ProcLightTab 生产臂）挂 S33 后续批次 | edge3 | 🔄 登记即动（2026-09-20，卡 I 开工；minix-types 三件 + PM 随动本批落，kernel 半待 edge1） |
| C-23 | edge3 卡 H（S35 批次二十五，13 篇 termctl）：termios wire 权威 + tty ioctl 面 | edge3（需求方） | `os/libs/minix-types`：新增 `types/termios.rs`（`Termios` 44 字节：`c_iflag/c_oflag/c_cflag/c_lflag` 四 `u32` + `c_cc[20]` + `c_ispeed/c_ospeed` 两 `i32`，`termios.h:192-200`；stty 十六旗标位与 `VMIN`/`VTIME` 槽位常量，逐位钉值测试）——仅新增文件；`os/libs/minix-sys`：顶层新增 `TIOCGETA`/`TIOCSETA` 请求号（`ttycom.h:88-89` 的 `_IOR/_IOW('t',…,struct termios)` 编码）与 `tcgetattr`/`tcsetattr` 便利封装——仅新增常量与函数，不改既有 | edge3 | 🔄 登记即动（2026-09-20，卡 H 批次二十五；真机 tty 往返验证挂 E5，宿主以决定半测试与请求号钉值为准） |
| C-24 | edge3 卡 E（S31 批三）：smoltcp 外部依赖引入（N1-P1-3 裁决的栈本体落地） | edge3（需求方） | `os/Cargo.toml`（workspace 依赖表新增 `smoltcp`，§1 规则 3 登记同轮；只加一行，不动既有条目）+ `os/net/lwip/Cargo.toml`（消费该依赖）+ `Cargo.lock`（当前不入库，无需提交）。离线策略已盘点：宿主与 minix-ci:1.94 容器均可达 crates.io（index 200），共享 CARGO_HOME 缓存持久（docker `-v $HOME/.cargo:/usr/local/cargo`），**走 crates.io 直连，不建 vendor 目录**；断网重建靠缓存，版本以 Cargo.lock 缓存面为准 | edge3 | 🔄 登记即动（2026-09-20，卡 E 批三开工） |
| C-25 | edge3 卡 I（S33 收口）：GET_PROCTAB 行扩面的**生产者半**（C-22 前半的内核侧） | edge3（需求方；edge1 已收线不活跃，由本线执行） | `os/kernel/src/`：①`misc.rs` 的 `getinfo_proc_tab`/`ProcInfoStruct::from_kprocess` 填四格（`p_kipc_cycles`/`p_kcall_cycles`/`p_dequeued`/`p_cpuavg`，C `do_getinfo.c:96-100` 的 GET_PROCTAB 是整 `struct proc` 拷贝，Rust 侧 A-4 窄行缺这四格）；②`add_kipc_cycles` 生产记账点（现仅测试调用）；③`cpuavg_increment` 的时钟 tick 钩子（C `kernel/arch/i386/arch_clock.c:306`）。只增不改既有导出面 | edge3 | ✅ 销账（2026-09-20 卡 I：`from_kprocess` 填四格、`KBILL_IPC` 标记 + 两处 context_stop 等价消费、cpuavg tick 钩子（`p_cycles.tick` 累加 + `tpt = tsc_per_ms*1000/hz`）；kernel 777 测试全绿。**余**：用户进程的 cpuavg 记账（C 走汇编中断入口 `mpx.S:77-90`）在 Rust 无对应站点，留作设计裁决）|

| C-26 | edge3 卡F3d（S30 余量）：sffs 语义核心入库（`os/libs/minix-sffs` 扩展） | edge3（需求方） | `os/libs/minix-sffs/src/`：新增 `attr.rs`（SffsAttr + SFFS_ATTR_* 掩码）、`table.rs`（SffsTable trait——C `struct sffs_table` 15 动词的 Rust 直译，句柄 u64）、`server.rs`（SffsServer: FsDriver 只读半 + 节点树/惰性句柄，C libsffs inode.c/handle.c/lookup.c 对位）；Cargo.toml 增 `minix-fs` 依赖（FsDriver/DentryEncoder 面）。既有 params/path/name/verify 模块只消费不改动 | edge3 | 🔄 登记即动（2026-09-20，卡F3d 开工） |
| C-26 | edge3 卡 I 余项：**中断入口的 context_stop**（用户进程的周期账与 quantum 消费） | edge3（需求方；edge1 已收线，由本线执行） | `os/kernel/src/`：①`lib.rs` 把 `context_stop` 的消费账（状态桶+总周期+cpuavg）并成单点 `account_process_stop`（顺手消除与 `account_kernel_stop` 的双计风险）；②新增 `account_interrupt_stop(_with)` 并在 `irq_manager.rs` 的 `dispatch_hardware_irq` 接线——以 BKL 的 acquired/inherited 两态对应 C `hwint_master` 的 `TEST_INT_IN_KERNEL`（打断用户态→结清被打断者，打断内核态→留给切换站点）；③quantum 从此真正被消费，`check_quantum`/抢占链复活。只增不改既有导出面 | edge3 | ✅ 销账（2026-09-20，用户裁决选 A 后同轮落地：`account_process_stop` 单点三账、`account_interrupt_stop(_with)` + `dispatch_hardware_irq` 接线；kernel 780 测试全绿（+3 端到端：quantum 消费/抢占政策/空转安全）。**余**：非 timer 中断与 SMP 多 CPU 的站点归属随 SMP 收口一并核）|
| C-27 | edge3 S42 ④ 全系统自举载体 | edge3（需求方） | `os/qemu-tests/`：多进程 boot 形态载体（kernel → VM/RS → 服务集 → init；现生产链只有测试内核手工 `load_vm_elf` 形态）。edge1 已收线，由 edge3 线经本认领执行（C-25/C-26 先例）；新增独立测试内核文件按 §1 规则 3 免登记，`run_all.sh` 接线需登记 | 无 | 🔄 载体半程（2026-09-20，S42 批四）：`test-sysboot` 载体 + `sysboot-rx`/`sysboot-tx` 双载荷 + 判定脚本入库（构建全过、真机 boot 到 smp_init、VM 槽单镜像装载验证通过）；双镜像交换撞 boot loader 栈窗口缺口 → **C-29 登记**，PASS 接线 run_all 随 C-29 落地（修法裁决：quick param vs per-process 根，待认领时定） |
| C-28 | edge4 E5(c) 前哨批A 附带发现：srv_fork 子的 PRIV_PROC 保留未落地 | edge3（需求方；edge4 只登记不改） | `os/servers/pm`：`mproc/fork.rs` 的 `Process::srv_fork_from` 现落 `Privilege::User(注入凭证)`，C（`forkexit.c:199-200`）与设计（`04-stage-pm/08-pm-srv-fork.md` §2.4/§4.2）均要求保留 `PRIV_PROC`（`is_kernel_process()==true`）——设计引用的 `SRV_FORK_INHERIT_FLAGS` 全仓无此符号，`RemainingFlags` 亦无该位。修法需 `Privilege::Kernel` 承载凭证（六字段注入无处安放），属 PM 类型设计裁决；行为后果在 `is_kernel_process()` 的退出/致命信号/事件订阅/调度接管四处判据。**登记即持锁（PM 生产代码归 edge3）**；edge4 侧以测试偏差钉钉住现值（`os/tests/srv_fork.rs`） | edge3 | ☐ 待认领（2026-09-20 由 edge4 批A 发现并登记，细节见 `.review/zcode/edge4/FIXLOG.md`） |
| C-29 | edge3 S42 ④ 载体半程发现：boot loader 的 per-process 地址空间/栈放置缺失 | edge3（需求方；kernel/arch 面待登记执行） | `os/arch/src/arch/boot.rs` `load_vm_elf`（:471 按**全局** `kernel_info.user_sp()` 定栈）+ `os/kernel/src/lib.rs` `init_proc_and_boot`（仅 VM 分支装载）。真机实测（2026-09-20，test-sysboot 载体）：第二用户镜像装载撞 RX 已占的 64 KiB 栈窗口 → `MappingFailed`（RX 5 GiB 段面 + TX 6 GiB 段面无碰撞，栈是唯一冲突）。深层：Rust bootstrap 全部用户镜像共享**单一** bootstrap 根；C `arch_boot_proc`（protect.c:388）为每个 boot 进程建独立地址空间、栈私有，同 VA 无碰撞。修法候选：①`load_vm_elf` 增 per-process `stack_high` 参（快，但共享根下段面互撞问题仍在）；②boot loader 建 per-process 根（C 对位，动 kernel/arch 两层，设计裁决先行）。载体 `test-sysboot` + 载荷已入库（构建过、boot 到 smp_init、单镜像装载即 RX 验证通过），PASS 判定脚本就绪，C-29 落地后即插即跑 | 无 | ☐ 待认领（2026-09-20 S42 批四登记；真机证据链见 edge3 FIXLOG Fix #120） |
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
| 2026-09-20 | edge4 E5(a) | `os/tests/`（`pm_vm_fork.rs` 重写 + `Cargo.toml` 依赖换 minix-vfs/minix-sys + 死壳 `pm_vm_fork_test.rs` 删除） | E5(a) 宿主联调复活（旧停用注释的复活条件 E1/E2 已满足）；本线自持，无跨界认领 | ✅ 同日（430803d4f；minix-tests 2 passed，pm 502 + vfs 全绿） |
| 2026-09-20 | edge4 E5(c) 前哨 | `os/tests/`（新增 `srv_fork.rs` + `Cargo.toml` 增一条 `[[test]]` 声明） | 批A srv_fork 链宿主半（RS→PM→VM→VFS）；本线自持，生产代码零改动 | ✅ 同日（fbd33bcae；srv_fork 2 passed，pm 405+11 + vfs 502 全绿） |
| 2026-09-20 | edge4 E5(b) | `os/tests/`（新增 `vm_vfs_fdclose.rs` + `Cargo.toml` 增一条 `[[test]]` 声明） | 批B VM↔VFS FDCLOSE 往返宿主半；本线自持，生产代码零改动 | ✅ 同日（79d82ed91；vm_vfs_fdclose 2 passed，pm/vfs 全绿） |
| 2026-09-20 | edge4 E5(f) | `os/tests/`（新增 `ds_publish_subscribe.rs` + `Cargo.toml` 增一条 `[[test]]` 声明与 `minix-ds` 依赖） | 批C DS 发布/订阅/取回三链宿主半；本线自持，生产代码零改动 | ✅ 同日（65bd107a6；ds_publish_subscribe 2 passed，ds 112 + pm/vfs 全绿） |

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
| edge3 S38 vm 传输面（原行误记为 S37） | edge3 | edge2 L15（FS 二级缓存升级） | ✅ 2026-09-19 由消费方核验落地（**更正**：本行原记「S37 / 7ed891a9b」，即 E-DMABUF vm 侧，与 L15 无关；L15 的真前置是 edge3.md 的 S38「vm_map_cacheblock/vm_set_cacheblock 等价传输面」，其交付物在 HEAD 已存在并全绿——VM 侧 `os/servers/vm/src/ipc/cache_handlers.rs` 四 handler + dispatcher.rs:694-697 路由 + minix-sys vm.rs:618-706 四封装，38+3 测试过。edge3.md 的 S38 行状态列仍 ☐，属其账本待回勾项；L15 已在 8b8b495a7 按「前置在代码中已满足」完成）。**edge3 已回勾（2026-09-20 卡M）**：复跑验收 38+3 全绿，S38 行已翻 ✅ |
| edge3 S17 RS 换装（枢纽） | edge3 | edge3 内部链 + edge4 E5 全族（RS 启动各服务器） | ✅ 2026-09-19（S17 主体 0e33c276c 起多批收口,edge3.md 已 ✅） |
| edge3 S19 A-6 裁决 | edge3 | edge3 S23（IS main 替换） | ✅ 2026-09-19 1f72d8ffe（diag 缝 = sys_diagctl code1 单汇点） |
| edge3 S27 sched 通电 | edge3 | edge3 S7/S10（PM 调度臂）+ edge4 E5(e) | ✅ 2026-09-20（通电半：接收半装 SEF 层 + 参战场景宿主断言，84 passed；真机参战半移交 E5(e)——S7/S10 解锁，PM 侧臂按 sched 消息面契约开工） |
| edge3 S29 通用 startup 框架 | edge3 | edge3 S30/S31/S25 余件（fs-rt 共享面：生产 FsTransport + RS_INIT 出生 + serve 装配） | ✅ 2026-09-20 卡D（minix-fs-rt；S30/S31/S25-K 可开工） |
| edge2 L17 G6 波次 1（驱动进程层起步） | edge2（已收线） | T2 交互面（console/键盘）、edge4 E5 真机联调 | ☐ 待认领执行（edge2 线不活跃） |
| edge3 S42 启动装配（RS init 发送腿 + boot 真值链 + RS boot tab + 全系统载体 + init/rc） | edge3 | edge4 T2/T3；S9（②并入）；C-3/C-27 | ✅ 2026-09-20 S42 卡（五批：①②③⑤落地——sched 出生应答补全/GET_MONPARAMS+GET_IMAGE 真值链+PM·RS acquire/boot tab 序列+SYNCH_BOOT 计数 P0 修复/boot 参数读取面；④载体半程——test-sysboot 入库+真机确诊 boot loader per-process 栈放置缺口→**C-29 挂账**；真机 rc 链随 T3/T4。五笔提交 8650fb26e…见 edge3 FIXLOG #117-121） |
| edge1 K9 向量表 | edge1 | edge1 K12b（三架构用户态）、edge4 T5 | ✅ 2026-09-20 复核（edge1.md K9 行：arm64 `exc_vector_table` 16×128B `.align 11` + 4 类 diag stub 已落；K12b 双腿真机 PASS 各 3/3 —— 本行原记 ☐ 系状态板滞后，按 edge1.md 更正） |
| edge1 K1/K2/K3 SMP 面 | edge1 | edge4 E5(e)/E5 SMP 冒烟 | ✅ 2026-09-20 复核（edge1.md：K1 生产壳读 `try_smp_state`+参数化 `sched_enqueue_with`、K2 环①按 §3.5.3 冻结决策记账、K3 本地臂/IPI 臂缺口中修双设 VMINHIBIT+FLUSH_TLB，三者均 2026-09-18 收口；**E5(e)/E5-SMP 的 edge1 侧前置已清**，只剩真机载体的 edge3 侧条目） |

## §4 在制避让（2026-09-18 工作树现状）

工作树存在未提交修改，下列条目领取前先确认在制工作已落地，避免双持：

- `os/libs/minix-chardriver/src/driver.rs`、`os/libs/minix-blockdriver/src/driver.rs` 已修改 → 影响 **edge2 L4/L5**（T7 后续工作刚提交 7dfa450c2，疑有连 续在制）。
- `os/servers/ipc-server/tests/integration.rs` 已修改 → 影响 **edge3 S26**。**已落地**：fa668079d（2026-09-19，测试侧追平 CallHandler `&mut Message` 签名，S26 前置清除）。
- `tools/coverage-extract/ds-semantic-map.json` 已修改 → 工具面，edge4 侧知悉即可。

## §5 E5 端到端联调包编排（条目主体在 [edge_todo.md](edge_todo.md) E5；此处只做跨线编排）

| 子项 | 内容 | 前置（哪条线交付什么） | 执行载体 | 状态 |
|---|---|---|---|---|
| E5(a) | PM↔VM fork 全链路（走 minix-sys 消息层） | edge3 S1+S20+S17；内核 eager-CoW 已备 | `os/tests/`（宿主）+ 真机挂 T2 | 🔄 **宿主半 ✅（2026-09-20，430803d4f）**：`os/tests/pm_vm_fork.rs` 复活重写——旧停用注释的复活条件（E1/E2 闭环）已满足；PM `do_fork` 真状态机 × VM wire 契约（`VmForkIn` 回解 / `VmForkOut` 构造，互证）× VFS `VfsPmHandler::handle` 真分发臂（`child_pid` 取自消息 m7i3 闭环）；2 测试（成功链 / VM 拒绝回滚），死壳 `pm_vm_fork_test.rs` 删除。**真机半挂 T2** |
| E5(b) | VM↔VFS fdclose 往返 | edge3 S12 + S20 | 同上 | 🔄 **宿主半 ✅（2026-09-20，79d82ed91）**：`os/tests/vm_vfs_fdclose.rs`——VM 发送半 `pub(crate)` 不可链接，按同一 wire 结构 `MessVmVfsCall` 构造请求；`decode_vm_call` 真解码器解六域 → 按 C `do_vm_call` 的 FDCLOSE 分支走 `close_fd` 真语义（关的是 VM 自己的 fd 表，消息 `endpoint` 域只属 FDLOOKUP）→ 应答经 VM 的真解码器 `VmVfsReplyIn::decode_message` 读回（`reqid`/`result`/`endpoint` 三域闭环，失败路径携 `-EBADF` 与原请求号）。2 测试（成功往返 / 无效 fd）。**真机半挂 T2**；VM 侧缺口登记见下行注 |
| E5(c) | RS live-update 全链（PREPARE→UPDATE→resume） | edge3 S17+S18+S20 | 同上 | 🔄 **前哨段宿主半 ✅（2026-09-20，fbd33bcae）**：`os/tests/srv_fork.rs`——RS 门负路径（非 RS 端点 → `NotPermitted` 且零出站，门在 `vm_fork` 之前）× 成功链三支出站（VM_FORK 经 `VmForkIn` 回解 / `VFS_PM_SRV_FORK` 的 `REUID`/`REGID` 为真实 uid/gid / 给子进程的立即 OK）× VFS `VfsPmHandler::handle(VfsCall::SrvFork)` 真分发臂（复制后追加 setuid/setgid 两步）；2 测试。**附带发现 C-28**（子进程 `PRIV_PROC` 保留未落地，偏差钉在测试里）。**PREPARE→UPDATE→resume 主体与真机半挂 T2** |
| E5(d) | QEMU VM paging 冒烟（含缺页完整回路 + VM 写 PTE） | edge3 S20 + edge1 K17 载体 | `os/qemu-tests/`（edge1 实现，edge4 验收） | ☐ |
| E5(e) | PM↔SCHED 调度链（START/INHERIT/NO_QUANTUM 回环） | edge3 S27 + edge1 K1/K2 | 真机（T2 后） | ☐ |
| E5(f) | DS 发布/订阅三链 + regex pattern 用例 | edge3 S22 + S17 | 真机（T2 后） | 🔄 **宿主半 ✅（2026-09-20，65bd107a6）**：`os/tests/ds_publish_subscribe.rs`——DS 的事件循环开在 `DsIpc`/`DsKernel` 两个 trait 上，宿主态跑真服务器 `DsServer::run_once`：`boot::apply_boot_map` 铺标签 → 订阅（`plan_subscribe` 模式编译 + `apply_subscribe` 落座）→ 发布（`plan_publish` 裁决 + 提交 + `notify::apply_update` 扫描唤醒）→ 取回（`plan_check` + grant 写回 + `apply_check` 消费）；对端接缝断言 DS 落座的 key/值咬合 VFS 的 `classify_ds_key` 与 `ds_event_action`（`drv.blk.0` + `DS_DRIVER_UP` → dmap 上线）。2 测试（三链全通 / 模式不匹配不唤醒）。**真机半挂 T2** |
| E5(g) | MIB/sysctl 四链 + rmibtest 契约 | edge3 S24/S26/S33 + E-RMIBWIRE 通电 | 真机（T2 后） | ☐ |
| E5(h) | devman 生命周期四链 | edge3 S25 + S17 | 真机（T2 后） | ☐ |
| E5-SMP | 「fork 后父子并发写 CoW 页」陈旧 TLB 用例 | edge1 K3 + 真拓扑 | `os/qemu-tests/` -smp 4 | ☐ |
| E5-rt | 首个 no_std minix-rt 二进制三架构化 | edge1 K12b | qemu-tests | ✅ 2026-09-19（x86/riscv64/aarch64 三腿真机 PASS，见 edge1 K12b） |
| E5-ARCH | 三架构全系统复跑：服务器/命令 bins 交叉构建矩阵（riscv64gc/aarch64-unknown-none）+ 非 x86 多进程 boot harness + T2~T4 梯次复跑编排 | T2~T4 达成 + edge3 S42 | `os/qemu-tests/`（经 C-27 由 edge3 线执行）+ 各线构建面 | ☐ |

QEMU 测试内核与脚本统一由 edge1 在 `os/qemu-tests/` 实现（新增独立文件不需登记）；edge4 只持断言清单与 PASS 判定。

**E5(b) 的 VM 侧缺口（2026-09-20 批B 核实，如实登记不改）**：`minix-vm` 的对外导出面只有 `VmServer` 与 boot 类型（`os/servers/vm/src/lib.rs:97-100`），`vfs_queue` 全模块 `pub(crate)`——`VfsRequestQueue::take_pending_vfs_call`（发送半）与 `purge_by_owner`（死进程兜底，V13-P3-1之2）都链接不到。因此宿主半只能覆盖到 wire 形状，VM 队列自身行为由 `os/servers/vm/src/vfs_queue.rs:444-528` 的 crate 内测试覆盖（其中 `purge_drops_queued_of_owner_and_keeps_others` 即死进程兜底）。若要宿主侧也真跑 VM 队列，需 VM 侧暴露可链接面——那属 edge3 领地，本线不自行开口。

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
| T2 | x86_64 服务器通电链：**S42 启动装配** → S17 RS 枢纽 → S22 DS / S27 sched / S1 PM → S20 VM → S12 VFS+mfs → S23 IS / S24 MIB / S25 devman / S26 ipc-server（驱动进程波次 1 = edge2 L17） | edge3 + edge2 | 各服务器 main 去 panic/停车，E5 对应子项 PASS |
| T3 | init 真实运行：S3 PM 信号臂 + S39 init 收口 + S42 ⑤ rc 链（OQ-3 已裁决：盘上文件面） | edge3 | init 进 multi-user 雏形，waitpid/信号端到端 |
| T4 | 18-stage 命令执行：S35 命令批 + S32 根镜像装机面（mkfs/diskimg）→ QEMU 内执行 echo/ls/cat/sh 等真实命令 | edge3 | 18-stage 命令冒烟脚本 PASS（宿主冒烟由 edge2 L2 保证诚实） |
| T5 | 三架构复跑 T2~T4 梯次 + SMP 正确性（K1/K2/K3 + E5-SMP） | edge4 编排 | 三架构 × 全梯次 PASS → §8 收尾 |

## §8 收尾与归档规则

1. 每达成一个阶梯，edge4 在 §3/§5/§7 勾账，并**批量**把涉及条目的状态回写 edge_todo.md 与对应 stage todo.md（按各文件原有回写体例），消除双真相窗口。
2. T5 全绿后：三个 edgeX.md 与本文件的条目表冻结，全量对账一次（grep 原文件复核无漏领），然后整体并入 edge_todo.md 归档段（或 edge_todo_archive.md），四份临时文件删除或在头部标注 ARCHIVED。
3. 过程中新发现的条目一律先进所属线 edgeX.md（§1 规则 7），收尾时随批次并回权威文件。
| 2026-09-20 | edge3 (S42 批四/C-27) | `os/Cargo.toml` | test-sysboot 载体内核 + sysboot-rx/sysboot-tx 双载荷入 workspace 成员（测试内核文件本体按 §1 规则 3 免登记，成员表登记） | 部分——成员已入；run_all.sh 接线随 C-29 落地后补 |
