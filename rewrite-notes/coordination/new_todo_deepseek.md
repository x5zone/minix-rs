# new_todo_deepseek — 从当前工作树到「三架构 QEMU 启动 minix-rs 并运行 18-stage-commands 程序」的续做清单

> **作者标记**：deepseek。本文件是两名 AI 并行扫描中的一份独立产出，与同目录下其它 `new_todo_*.md` 互不引用、互不合并，以保证覆盖度相互独立。
> **日期**：2026-09-20。
> **目标（用户给定）**：在三架构（x86_64 / aarch64 / riscv64）的 QEMU 环境里启动 minix-rs，并运行 18-stage-commands 的程序。第一优先「先确保有」（能起来、能跑），第二优先「再确保好」（非 rewrite 的 translate 倾向与代码缺陷）。
> **判定口径**：本清单的每条都以「挡不挡启动链」为排序依据，而不是以「设计是否漂亮」为依据。挡路 = 按当前代码往下走，目标无法达成。
> **锚点纪律**：本文件所有事实断言都带 `文件:行` 锚点，全部为本次实测（2026-09-20 工作树，HEAD = f5fcef73f）。凡属**既有账本已登记**的条目，条文里注明登记位置；凡属**本次新发现**的，注明「新」。执行任何一条之前仍须按 fix-guard 重新读目标行 ±5 行核实——锚点会随提交漂移。

---

## §1 结论摘要

目标被五个断点分成了五段，任何一段不通，后面全部不可达：

| 段 | 内容 | 现状 | 断点 |
|---|---|---|---|
| ① 固件 → 内核 | boot-shim 装模块、内核建进程表、装载 VM | 内核侧与 x86_64 载体已通（test-user-trap / test-rt-birth / test-sysboot 单镜像） | **生产 boot-shim 的模块清单与内核契约不一致**（D-01），生产镜像必 panic |
| ② VM → RS → 各服务 | RS 发 RS_INIT、各服务答 RS_INIT、RS 收齐 | RS 发送腿与 RS 接收臂都在（`servers/rs/src/boot.rs:1031-1138`、`lib.rs:286-291`） | **只有 sched / devman / fs-rt 会答**，PM、VFS、DS、MIB、TTY、MEM 都不答或不正确作答（D-02），step3 永久阻塞；VM 的答还差一个授权（D-03） |
| ③ 服务自身的可引导形态 | 每个服务要有一个能被装载进 guest 的 ELF | 只有测试载体（rt-birth / sysboot）能产出 guest ELF | **11 个服务器、fs 家族、驱动家族的二进制目标没有任何一个能在裸机 target 上构建**（D-05），`minix-pm` 与 `minix-vfs` 连库都过不去 |
| ④ 根文件系统与控制台 | 根挂载成功、命令能读写 stdin/stdout | VFS 根挂载决策件都在但从不执行；memory 驱动是七行占位 | **VFS 从不挂根**（D-08）、**memory 驱动进程不存在**（D-07）、**MFS 块源恒 EIO**（D-09）、**控制台输出链四段全断**（D-10） |
| ⑤ 命令面 | 命令二进制的镜像构建与执行 | 65 个 bin 有真实逻辑，但全是宿主 std 形态 | **命令 bin 无 freestanding 构建面**（D-15）、**无镜像装配链**（D-06）、**覆盖 65/328**（D-16）、**libc 原语缺 25 类**（D-17） |

一句话状态：**设计文档与宿主测试的完成度远高于"真机可运行"的完成度**。宿主测试跑的是"决定半"，而把决定半串成真机进程所需的四类工程面——可引导二进制、启动镜像装配、根文件系统链、控制台字节通路——目前整体缺席。三架构（T5）在此之上还缺一层：aarch64/riscv64 没有生产用户态 trap 入口（D-13）。

---

## §2 第一优先级：让 x86_64 真机跑起来（启动链 P0）

### D-01 生产 boot-shim 的模块清单与内核 boot 模块契约不一致（新）

**是什么**：UEFI 侧装载器声明 6 个模块且顺序自定，内核按 C 的 boot 镜像顺序做**位置映射**，两侧对不上。

**证据**：
- `os/boot-shim/src/loader.rs:54`：`pub const MODULE_NAMES: &[&str] = &["vm", "pm", "vfs", "rs", "ds", "inet"];`。
- `os/kernel/src/proc.rs:81`：`NR_BOOT_MODULES = 12`；`os/kernel/src/proc.rs:106` 的 `BOOT_MODULE_PROC_NRS` 顺序是 DS、RS、PM、SCHED、VFS、MEM、TTY、MIB、VM、PFS、MFS、INIT（C `table.c:15-30`）。
- `os/kernel/src/lib.rs:1131-1137`：`init_proc_and_boot` 断言 `kernel_info.boot_modules().len() == NR_BOOT_MODULES`，不等即 panic。
- `os/boot-shim/src/loader.rs:44-53` 的注释声称「内核的 `proc_init` 会按 SYSTEM 标志重排」——**该重排在内核里不存在**，内核是严格按位置映射的（`kernel/src/lib.rs:1185` 起遍历下标）。
- 测试之所以没暴露：`os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs:88-100` 手工构造 12 项零长度模块数组，绕开了装载器。

**为什么挡路**：这是目标链最上游的一击——生产镜像一进 `kmain` 就 panic；即使把数量凑齐，按当前顺序 `vm` 会被装进 DS 的进程槽（endpoint 6），VM 不再回答任何握手。

**方案对比**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | 装载器按 C 顺序列出 12 个模块名，文件按名装载、缺失即报错；注释中的「内核会重排」改为事实描述 | 与 C 的 `kinfo.module_list` 语义一致（`minix3/minix/kernel/main.c:171`），改动只在 boot-shim 一处；三架构装载器（UEFI / OpenSBI）共用同一清单 |
| B | 内核改为按名字查找模块 | 改的是内核契约，`BOOT_MODULE_PROC_NRS` 的位置语义是 C 的行为，改成名字查找属超出 Rewrite 边界的演进 |
| C | 放开 12 项断言、内核按存在的模块建表 | 会让 TTY/MEM/MFS/INIT 这些 boot 表成员缺席，RS step2/step3 的计数与 boot 表不一致，问题被推迟而不是解决 |

**落地要点**：`loader.rs` 的 `MODULE_NAMES` 换成 C 顺序的 12 项；`load_boot_modules_with_loader` 对缺失模块的处理从「跳过」改为「报错」（生产镜像缺一件就是镜像装配错误，静默跳过等于把 panic 推到内核）；同步修 `os/boot-shim/src/opensbi_helpers.rs:85` 注释里引用的同一份清单。
**验收**：新增一个用真 boot-shim 镜像（kernel.elf + 12 模块）跑的 QEMU 冒烟；把 `test-sysboot/src/main.rs:88-100` 的假模块数组改成从装载器清单生成，消除"测试替身掩盖生产缺陷"。

---

### D-02 出生面应答覆盖不全：PM / VFS / DS / MIB / TTY / MEM 不答 RS_INIT（部分已登记，本体为新）

**是什么**：C 的机制是「RS 在 step2 对 boot 表里每个 SYS_PROC 条目发 RS_INIT，step3 阻塞到全部答复为止」（`minix3/minix/servers/rs/main.c:375-407`）。Rust 侧 step2/step3 忠实移植了（`os/servers/rs/src/boot.rs:1031-1138`、`1128-1138`、`1153-1155`），但**应答方**只有三家实现。

**证据（已实现侧）**：
- sched：`os/servers/sched/src/kernel_api/transport.rs:173-190`（在事件分类之前拦截 `m_type == RS_INIT && source == RS`，回 `RS_INIT + OK`）。
- devman：`os/servers/devman/src/ipc/minix.rs:278-296`。
- fs 家族（mfs/pfs）：`os/fs/fs-rt` 的出生面（`run_birth`）。
- VM：`os/servers/vm/src/vm_server.rs:1315-1330`（收到 RS_INIT 后解包并继续握手）。

**证据（缺失侧）**：
| 服务 | 现状锚点 | 实际行为 |
|---|---|---|
| PM | `os/servers/pm/src/ipc/dispatcher.rs:103-114`（两条 ENOSYS 出口在 `:110` 与 `:114`） | 非 PM 调用号 → `ReplyIntent::Reply(ENOSYS)` |
| VFS | `os/servers/vfs/src/main_loop.rs:694-697`（分类）+ `:747-751`（回 ENOSYS） | 回 ENOSYS；另 `:7153` 有一条 `SefEvent::Init` 死臂（见 D-18） |
| DS | `os/servers/ds/src/server.rs:169-175` | 通知类一律回 `EINVAL` |
| MIB | `os/servers/mib/src/dispatch.rs:98-105`（`default_outcome`）+ `server.rs:173-177` | 非 sendrec 即 `EDONTREPLY`，静默丢弃 |
| IS | `os/servers/is/src/lib.rs:173-186` | 分类到 Suppress + warn，不回 |
| input | `os/servers/input/src/serve.rs:186-188` | `classify` 的 `_ => None` 丢弃 |
| ipc-server | `os/servers/ipc-server/src/dispatch.rs:66-78` | 未知调用 → ENOSYS |

**根因**：`minix-sef` 声明了 `SefEvent::Init(i32)`（`os/libs/minix-sef/src/lib.rs:63-71`，常量 `SEF_INIT_REQUEST_TYPE = RS_INIT` 在 `:57`），但 `sef_receive_status` 从不产生该事件——只在 `:151` 产 `Signal`、`:167` 产 `Call`。因此 RS_INIT 以普通 `Call(0x714)` 落到各服务的普通分发表上。

**C 的契约形状（供落地参照）**：`minix3/minix/lib/libsys/sef.c:118-137`——服务在进主循环**之前**阻塞在 `ipc_receive(RS_PROC_NR, …)` 上等 SEF 初始化请求，收到后由 `do_sef_init_request` 处理（`minix3/minix/lib/libsys/sef_init.c:193` 起），回 `m_type = RS_INIT; m_rs_init.result = 结果`，并顺带做 `cpf_reload()` / `senda_reload()` / `sys_statectl(SET_STATE_TABLE)`（`sef_init.c:128-142`）。

**方案对比**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | `minix-sef` 补出生面：`sef_receive_status` 识别 `RS_INIT` 且来源为 RS 时产出 `SefEvent::Init`，并提供共享的回 `RS_INIT+result` 助手（含 source 门与 fresh/LU/RESTART 分流）；已有三家（sched/devman/fs-rt）改为消费该事件 | 一处修好七个消费方，与 C 的 libsys 分层同构——C 里这正是库的职责；避免每加一个服务就复制一遍拦截 |
| B | 每个服务各自在分发表前加拦截 | 现状的延伸，七份重复实现，且新服务容易漏（input/ipc-server 已经漏了） |
| C | 在 `minix-driver-rt` / `minix-fs-rt` / 各家族运输层各加一遍 | 把库级契约下放到框架层，同样重复；driver-rt 与 fs-rt 已经各写了一遍（S29/S25 就是这么修的），再加就遍地开花 |

**落地要点**：①`minix-sef` 增 `SefEvent::Init` 的产生与默认应答助手；②七个服务按其 C 的 `sef_cb_init_fresh` 等价内初始化段接上（PM/VFS/DS/MIB 的 fresh 初始化动作本来就在各自的 `init_fresh` 路径上，只需多一步应答）；③LU/RESTART 按现有先例诚实拒 `ENOSYS` 并停机（sched 的形态；`transport.rs:183-195`）。
**验收**：`cargo test -p minix-sef -p minix-pm -p minix-vfs -p minix-ds -p minix-mib -p minix-is -p minix-input -p minix-ipc-server`；每条附一个"收到 RS_INIT 即回 RS_INIT+OK 且不进业务分发表"的接缝测试；真机验收归 T2 载体（见 D-22）。

---

### D-03 rproctab 授权在生产路径没有创建点，VM 答不上出生面（新）

**是什么**：RS_INIT 消息里带的 `rproctab_gid` 在 C 里是 boot 阶段用 `cpf_grant_direct` 建的一次性读授权（`minix3/minix/servers/rs/main.c:185`），VM 靠它把 RS 的公开进程表读进来完成握手。Rust 侧该字段恒为 `None`，编码时映射成 `-1`。

**证据**：
- `os/servers/rs/src/boot.rs:954`：`self.rinit = RinitState::default();`，注释自述「grant 系统调用属 19 的接线范围，字段保持 None」。
- `os/servers/rs/src/ready.rs:86-89`（字段声明）、`:116`（`None → -1`）、`:386-391`（测试正钉这个哨兵）。
- VM 侧：`os/servers/vm/src/vm_server.rs:1436-1443` 用 `init.rproctab_gid` 做 `sys_safecopyfrom(Endpoint::RS, rproctab_gid, 0, …)`（实现在 `:1654-1679`）；读到 `-1` 必然失败，而失败分支（`:1327-1336`）**丢弃 RS_INIT 且不回复**——RS 的 step3 于是永久阻塞。

**为什么挡路**：即便 D-02 把其它服务都修好，VM 这一件仍会让 `nr_uncaught_init_srvs` 永远减不到 0。

**落地要点**：boot step0 真调 `cpf_grant_direct(ANY, rproctab_addr, size, CPF_READ)` 并落进 `rinit.rproctab_gid`。需要 `KernelApi` 增一个 grant 动词（RS 的 `SysApi` 在自升级路径已有 `cpf_reload` 先例，见 edge3 S18），并核对内核侧 `dispatch_setgrant` 对 RS 的 magic-grant 门是否放行该用途（`os/kernel/src/grant.rs`；edge_todo.md E-MIBGRANT 记录过该文件端点常量与 C 不符的历史问题，动工前先 grep 现状）。
**验收**：RS 侧钉「step0 之后 `rproctab_gid.is_some()`」；VM 侧钉「gid 非法时握手失败并回 RS_INIT 带错」；两处接缝测试 + 真机（D-22 载体）。

---

### D-04 RS 的 read_exec 在生产路径是空实现，服务起不来也活不过来（已登记，本体为新）

**是什么**：C 里 RS 通过 `read_exec` 让子实例读入可执行文件（`minix3/minix/servers/rs/manager.c:1372-1420`）；Rust 生产路径注入的是 noop。

**证据**：
- `os/servers/rs/src/lib.rs:436`（重启路径 `read_exec: &mut noop_exec`）、`os/servers/rs/src/recovery.rs:924`/`:944`、`os/servers/rs/src/shell_request.rs:1479`。
- `os/servers/rs/src/exec.rs:21-30` 自述「read_exec 是内核调用缝，在此之前本校验器无接线点」。
- 账本登记：edge3.md S42 批五末段（「RS 盘面接口 read_exec 真实化随 VFS 通电，seam 注入改造登记为 RS 内部设计项」）。

**为什么挡路**：boot 表的 12 个服务由内核装载 + VM exec，不经过 read_exec；但 **`service up` 启动（IS/MIB/devman/input/ipc-server 这些非 boot 表服务）与崩溃重启**全走这条路。目标里的"运行 18-stage 命令"不直接依赖它，但"系统能自我维持"依赖它——而且 init 之后的所有服务都靠它。

**落地要点**：`read_exec` 的 C 实现是 `srv_fork` + `srv_execve` 两跳（子实例读文件 → exec）；Rust 侧需要 VFS 的 `read_exec` 请求面（`VFS read_exec` / `FS image read`）就绪。落地顺序上它排在 D-08（VFS 挂根）之后，属 T3 段。
**验收**：`cargo test -p minix-rs`（既有 924/944 的 noop 用例改真件后需重写），加一条「up 一个真实二进制并收到 RS_INIT」的宿主接缝测试。

---

### D-05 服务与驱动的二进制没有裸机构建面（新，系统级）

**是什么**：目标要求每个服务作为独立进程在 guest 里跑，因此每个服务都必须能编出一个裸机 ELF。当前**只有 `minix-init` 一个 crate 有 freestanding 配置**，其余全部是宿主 std 程序。

**证据（本次实测）**：
- `cargo check -p minix-pm --lib --target x86_64-unknown-none` → **1103 个错误**；根因是 `os/servers/pm/src/lib.rs` 根本没有 `#![no_std]` 声明（对照：`servers/vm/src/lib.rs:5`、`servers/rs/src/lib.rs:1`、`servers/sched/src/lib.rs:1` 等十家都有 `#![cfg_attr(not(test), no_std)]`）。
- `cargo check -p minix-vfs --lib --target x86_64-unknown-none` → **29 个错误**；根因是 `os/libs/minix-sockdriver/src/sdev.rs:38` 起的三处 `derive` 在无 std 时不可用（该 crate 缺 `no_std` 声明）。
- 其余九家 lib 该 target 下零错误（实测：rs/vm/sched/ds/mib/is/devman/input/ipc-server）。
- 所有 `os/servers/*/src/main.rs` 都没有 `cfg_attr(target_os = "none", …)` 条款——它们的 `no_std` 只出现在注释里（例：`servers/sched/src/main.rs:9-12` 的注释 + `#[cfg(test)]` 全局分配器）。
- 唯一的模板是 init：`os/commands/sbin/init/Cargo.toml:33-37`（按 `cfg(target_os="none")` 分域依赖 `minix-rt` 的 `alloc-global` 特性）+ `src/main.rs:19`（`#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]`）。
- 工作区 rustflags 已备：`os/.cargo/config.toml:33-34`（`x86_64-unknown-none` 取 `relocation-model=static` + `code-model=large`，内核 ELF 装载器要求 ET_EXEC）。

**方案对比**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | 逐 crate 复制 init 模板：Cargo.toml 按 target 分域依赖 + `main.rs` 加 `cfg_attr` 三元（`no_std`/`no_main`/`panic_handler`）+ 链接脚本（照 `qemu-tests/test-kernels/user/rt-birth/build.rs` + `link.ld` 的形态） | 与已验证的 rt-birth 载体同形，风险最低；按 crate 逐个 `cargo check --target` 收敛，每步可回归 |
| B | 在 workspace 层统一 `default-features = false` 一把梭 | 已知会撞「Cargo 特性统一」陷阱：`-p minix-arch` 与 boot-shim/测试内核同命令跑必然类型错配（历史已实测），必须分次调用；一次性改 workspace 会让宿主测试全红，违背"树必须常绿" |
| C | 把服务做成"宿主可跑 + 真机可跑双形态"的 feature 门 | 双向维护成本高，且宿主态本来就不需要 main.rs 参与（测试直接驱动 lib），收益不抵成本 |

**落地要点**：按依赖顺序分批——先 `minix-pm`（补 `no_std` 声明，是唯一连库都过不去的）、再 `minix-vfs`（补 minix-sockdriver 的 `no_std`），然后十一家的 `main.rs` 统一加目标条款；每加一家跑一次宿主回归（`cargo test -p X`）与一次 `cargo check -p X --target x86_64-unknown-none`。
**验收**：`for p in pm vfs rs vm sched ds mib is devman input ipc-server; do cargo check -p minix-$p --target x86_64-unknown-none; done` 全零错误；且 `cargo build -p minix-pm --target x86_64-unknown-none` 能产出 ELF（当前没有链接链，需随链接脚本一起落地）。

---

### D-06 根文件系统镜像的生产链整体缺席（部分已登记）

**是什么**：从"有一个 mfs 镜像文件"到"guest 里 `/` 挂着它"之间，缺三件：镜像装配脚本、装机清单、载体通道。

**证据**：
- 装配骨架是空的：`os/xtask/src/main.rs:123-142` 的 `image()` / `qemu()` 只有 `TODO(skeleton)` 与注释里的计划（收集 boot 镜像 ELF → 打包 → "生成 rootfs/ramdisk 镜像（mkfs + fstab…）"）。
- 格式化引擎是齐的：`os/fs/mfs/src/mkfs.rs:725-770` 支持 `d`/`b`/`c`/`s` 四种 proto 行与宿主文件填充；`os/commands/sbin/diskfmt/src/bin/mkfs_mfs.rs` 是宿主工具（用 `std::fs` 读源文件）。
- **没有 proto 文件**：全仓 `find -name "*.proto"` 只命中无关的 `protocol.rs`；C 侧的对位是 `minix3/releasetools/image.functions:236-262`（`nbtoproto` 生成 `proto.root`）。
- `os/etc/` 只有 `README.md`（自述占位），没有 rc / fstab / passwd / ttys。
- 装机清单不存在：`os/commands/sbin/diskfmt/Cargo.toml:15-16` 与 `mountinfo/Cargo.toml:16` 只在注释里写着"安装清单待定"。
- 账本登记：edge3.md S32（"装机面挂 E-FSBDEV/T4"）、edge4.md §3（T4 依赖 S32）。

**方案对比（镜像如何进 guest）**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | 照 C：镜像链进 memory 驱动的二进制（`minix3/releasetools/image.functions:352-355` 的 `objcopy -Ibinary` 手法），root 从 MEM 设备读 | 与 C 的 `DEV_IMGRD = 0x0106`（内存驱动 major 1 / minor 6）完全对齐（`minix3/minix/include/minix/dmap.h:113`），不需要 QEMU 加磁盘，也不需要块驱动进程；改动集中在 memory 驱动与 xtask |
| B | 镜像作为一个 boot 模块，由内核转交 | 需要在 boot 表/模块表里加一个非进程条目，与 C 的 `kinfo.modlist` 语义冲突，且要改内核装载面 |
| C | QEMU 挂一块 virtio-blk，做真块驱动 | 需要 PCI 枚举 + virtio 传输 + DMA，链路最长；作为后续（D-25）保留，不作为首个里程碑路径 |

**落地要点**：①写 proto 生成器（可以先用 Rust 直接生成文本 proto，不必复刻 `nbtoproto` 的 C 实现）；②`xtask image` 真做四件事——构建 target 形态的命令与服务 ELF、生成 proto、调 `mkfs` 引擎出镜像、封装成 memory 驱动可用的 blob；③镜像里放 `/dev/console` 等设备节点（mkfs 引擎已支持 `c` 行）。
**验收**：`cargo run -p xtask -- image` 产出 `os/target/image/` 下的内核 ELF + 12 个模块 + 根镜像，且能在宿主上用 fsck 引擎读回（`minix-fs-mfs::fsck` 已有，S32 卡G 落地）。

---

### D-07 memory 驱动进程不存在，root 设备无人服务（新，与 D-06 绑定）

**是什么**：C 的根设备是内存驱动（major 1），镜像链在其二进制里。Rust 的 memory 驱动策略库很完整，但**进程壳是七行占位**，镜像背书也没有。

**证据**：
- `os/drivers/storage/memory/src/main.rs` 全文 7 行（`init(); loop {}` 形态）。这是 57 个驱动 main 的通用形态（本次实测：53 个 main.rs 恰为 7 行占位，只有 tty 与 pckbd 是真实服务循环）。
- `os/drivers/storage/memory/src/device.rs:119-140`：`ImageDisk` 的 extent 留空，注释说"由服务 crate 在启动时提供"——**该服务 crate 逻辑不存在**。
- memory 的库面是齐的：1312 行、24 测试（minors/geometry/块面与字符面/传输计划），见 edge2.md L17 的盘点。

**落地要点**：`main.rs` 换成与 `os/drivers/tty/tty/src/main.rs:12-28` 同形的进程（`DriverRuntime::new(...)` + `serve(...)`），启动段把内嵌镜像的物理区间填进 `ImageDisk`，然后按驱动家族协议宣告（announce）与答 RS_INIT（与 D-02 同批）。
**验收**：`cargo test -p minix-driver-memory`；真机：内核装载 12 模块后 `memory` 进程能答 RS_INIT 并被 VFS 映射为 major 1。

---

### D-08 VFS 从不挂根：`do_init_root` 只改状态不做事（已登记为 DEFERRED，本体为新）

**是什么**：`os/servers/vfs/src/main_loop.rs:517-540` 的 `do_init_root` 只做三件事：置 `BootPhase::Mounting`、`set_accept_requests(false)`、再置回 `Running`。注释自述 `mount_pfs()` / `mount_fs()` 的 IPC "DEFERRED to 18-mount"。

**证据**：
- 决策件齐全但无调用方：`req_readsuper` 的编码/解码与 `FsSuperblock` 生产读者在 `os/servers/vfs/src/mount.rs:217-252`（S14 已落）。
- 启动段 `map_service` 循环同样缺席：`os/servers/vfs/src/main_loop.rs:496-506` 的注释写明「`sys_safecopyfrom(RS_PROC_NR, rproctab) + map_service()`（归 19，DEFERRED）」。
- dmap 只有 CTTY 一个槽：`os/servers/vfs/src/device_map.rs:137-146`。
- C 对位：`minix3/minix/servers/vfs/main.c:501-522`（`do_init_root` → `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, …)`）。

**为什么挡路**：`/` 是空 vnode 时，任何路径解析都失败——`sh` 启动、`exec /bin/echo`、读 `/etc/rc` 全部不可达。

**落地要点**：①接 `map_service`（RS 侧 `mapdriver` 面目前也未接：`os/servers/rs/src/publish.rs:9-13` 明写「仍未接：`mapdriver`」）——两处是同一件事的两半，必须同批；②`do_init_root` 真发 `req_readsuper` 给 MFS 并登记 vmnt；③PFS 的挂载（`mount_pfs`）为 `pipe2` 与 `ptyfs` 铺路，可同批或紧随。
**验收**：VFS 侧接缝测试（假 FS 端答 readsuper，断言 vmnt 与 vnode 落座）+ 真机（挂根成功后才能进 T4）。

---

### D-09 MFS 的块源是 `PendingBlockSource`，一切读写回 EIO（已登记 E-FSBDEV）

**是什么**：MFS 进程跑的是 fs-rt 的 serve 循环，但块源接的是"一切 EIO"的占位实现。

**证据**：`os/fs/mfs/src/main.rs:26-46`（`PendingBlockSource`）；`os/fs/fs-rt/src/source.rs:19-32`（每个 read/write 都回 EIO）。账本：edge_todo.md E-FSBDEV（L914 起）、edge2.md L14（`BdevBlockSource` 已落，缺消费方）。

**落地要点**：把 `BdevBlockSource` 接上 memory 驱动端点（BDEV 请求 → memory 的块面）。它与 D-07/D-08 是同一张挂载图的三条边，应同批验收。
**验收**：`cargo test -p minix-fs-mfs`；真机：读根目录的 `REQ_GETDENTS` 往返成功。

---

### D-10 控制台输出链四段全断，stdout 到不了屏幕（部分已登记）

**是什么**：用户进程 `write(1, …)` 要走 ①VFS 认字符设备 → ②dmap 里有 tty major → ③tty 进程收 `CDEV_WRITE` → ④后端把字节写到硬件。四段现在全断。

**证据**：
- ①`os/servers/vfs/src/syscalls.rs:306-307`（读非普通文件即 `Nosys`）、`:418-419`（写同理），`Nosys` 映射为 `-ENOSYS`（`main_loop.rs:6766`）。
- ②dmap 无 tty 槽（`os/servers/vfs/src/device_map.rs:137-146` 只映 CTTY=5）；tty 的 major 4 在 RS 的 boot dev 表里是有记录的（`os/servers/rs/src/table.rs:195-204`，两条：TTY→4、MEM→1），但 `mapdriver` 未接（`publish.rs:9-13`）。
- ③tty 进程壳已进程化（`os/drivers/tty/tty/src/main.rs:12-28`），但自端点是 `Endpoint::NONE`（`main.rs:13-16` 自述等待 RS 分配），且是 dev 表里唯一还没被 map 的。
- ④后端是空对象：`os/drivers/tty/tty/src/backend.rs:10-23`（自述"真实 console 后端未落地"，串口不可达、显存映射需要活的内存服务器）；`os/drivers/tty/tty/src/lib.rs:46-48` 的 `init()` 硬接 `NullBackend`。
- C 对位：VGA 文本内存 + COM1 双写（`minix3/minix/drivers/tty/tty/arch/i386/console.c:159-214`，`ser_putc` 在首控制台）。
- 账本登记：edge2.md L17 批B（"console 真机输出后端三条出口……如实登记缺口"）。

**方案对比（后端字节通路）**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | 串口优先：驱动进程用端口 I/O（x86_64 走 `SYS_DEVIO`/`SYS_VOUTB`，三架构各自有 MMIO 等价物）轮询写 COM1；QEMU 的 `-serial stdio` 立刻可见 | 与 C 的 `ser_putc` 同形、链路最短（不依赖 VM 映射显存、不依赖内存服务器活着），且 QEMU 三个架构都提供 16550 或等价串口 |
| B | 显存优先：VM 映射 VGA 文本缓冲，字符面直接写 0xB8000 窗口 | 更接近 C 的"主"通道（VGA 是 console 的默认），但要求 VM 的物理映射与 memory 后端同时可用，把两条未通的链耦在一起 |
| C | 走 `SYS_DIAGCTL` 诊断缝 | 语义错位——该缝是"服务器诊断打印"的下沉通道，不是终端数据通路（tty 侧自己的注记：`backend.rs:19-21`），拿它当 stdout 会把用户数据混进诊断流 |

**落地要点**：先做 A 打通 stdout（命令能看见自己输出，调试闭环立刻成立），B 随后补（对齐 C 的 `cons_write` 双写），SYS_DEVIO 的端口权限由 RS 的 priv 表下发（`boot_image_sys_table` 已在 `table.rs:161-186`）。
**验收**：真机：一个用户程序 `write(1, "hello\n")` 经 VFS → tty → 串口出现在 QEMU 输出里；同时 `tcgetattr/tcsetattr`（已有 wire，见 edge3 S35 批次二十五）仍要通。

---

### D-11 boot 表要求的 TTY / MEM 驱动必须先进程化（新，D-05/D-07 的子集）

**是什么**：boot priv 表 12 项里 TTY 与 MEM 是 SRV_F（含 SYS_PROC，见 `os/servers/rs/src/privilege.rs:111-113`），因此 RS step2 会给它们发 RS_INIT 并计入 `nr_uncaught_init_srvs`（`boot.rs:1074-1107`）。它们不答，step3 就停。

**证据**：`os/servers/rs/src/table.rs:91-152` 的 12 项；其中 TTY 对应 `os/drivers/tty/tty/`（已进程化但 `Endpoint::NONE`）、MEM 对应 `os/drivers/storage/memory/`（七行占位，见 D-07）。其余 55 个驱动的 main.rs 全是 `init(); loop {}`（本次实测逐文件计数）。

**要点**：最小启动集只需要 tty 与 memory 两个驱动进程；log / random / readclock / fb / pckbd / pty 属"随后波次"，不挡首个里程碑（它们不在 boot 表里，靠 `service up` 启动，而那条路依赖 D-04）。

---

### D-12 服务的自端点在启动段硬编码为 `Endpoint::NONE`（部分已登记）

**是什么**：C 的服务启动第一件事是 `sys_whoami` 问自己的 endpoint（`minix3/minix/lib/libsys/sef.c:78-82`，`sef_startup` 开头）；Rust 侧没有 whoami 的 wrapper，服务把 `NONE` 硬写进传输构造。

**证据**：`os/drivers/tty/tty/src/main.rs:13-16`（自述"直到 RS 分配落地前用 NONE"）、`os/servers/input/src/main.rs:36`（`self_ep = Endpoint::NONE`，`serve.rs:79-82` 用它做回复源）；账本登记：edge3.md S28（"self_ep=NONE 待 RS 分配归 E5"）与 edge2.md L17 批C。
**为什么挡路**：回复的 `m_source` 为 NONE 时对端无法正确路由（input 的回复会错址），RS 的 ping 也无法被正确应答。
**要点**：`minix-sys` 补 whoami wrapper（内核 `sys_getinfo` 的 `WhoAmI` 子请求已存在：`os/kernel/src/misc.rs:64-106` 的枚举里），各服务启动段改问自己。

---

### D-13 x86_64 之外的 SYSCALL 快路径返回未实现（已知边界，需登记为里程碑风险）

**锚点**：`os/kernel/src/lib.rs:3010-3021`——`ReturnSequence::FastSyscall` 分支直接 `panic!("restore_user_context: fast-syscall return sequence lands with S-8")`。
**现状判断**：注释自述"当前没有任何入口路径会记录 Syscall 风格"，int-33 腿（test-user-trap 真机通过）不经过该分支。因此它**不挡**当前里程碑，但它是"SYSCALL 腿一旦启用就立即停机"的地雷，应作为已知边界登记（而非 P0）。

---

## §3 第二优先级：命令面（先有，再好）

### D-14 命令二进制的 freestanding 化（新，命令侧最大的一块）

**证据**：24 个域 crate 的**库**都是 `no_std`-movable（`#![cfg_attr(not(test), no_std)]` 在各 `src/lib.rs:1`），但 **66 个 bin 目标里只有 `minix-init` 一份 freestanding 配置**；其余全部无条件 `use std::env` / `std::process`（部分还用 `std::fs`）。
- 例：`os/commands/bin/fileops/src/bin/echo.rs:7-21` 自述"argv/exit 走 std，直到 no_std 程序镜像落地"；同 crate 的 `Cargo.toml:16` 固定 `minix-rt` 的 `std` 特性（regex `:16`、textfilter `:16`、stdio-games `:16` 同形）。
- 模板：`os/commands/sbin/init/Cargo.toml:33-37` + `src/main.rs:19`。
- 另外 minix-rt 侧期望一个**不修饰的** `main` 符号：`os/libs/minix-rt/src/crt0.rs:293-294` 声明 `unsafe extern "Rust" { fn main() -> i32; }`，而 `os/commands/sbin/init/src/main.rs:64` 的 `fn main()` 既无 `#[unsafe(no_mangle)]` 也返回 `()`——init 自己的 guest 链接态其实还没跑通过（`target/x86_64-unknown-none/` 下只有 check 产物，没有链接出的 ELF）。

**方案对比**：与 D-05 同（A 逐个复制 init/rt-birth 模板）。命令侧的额外要求是 argv/env 访问器换到 `minix_rt::crt0`（`argv_count`/`argv_bytes`/`envs`，`crt0.rs:150-200`），退出走 `minix_sys::exit`。
**落地要点**：先修 init 的 `main` 签名与链接脚本（它是"第一个能跑的 guest 命令"的样板，也是 init 真启动的前提），再按 §6 波次逐域推。
**验收**：`cargo build -p minix-fileops --target x86_64-unknown-none --bin echo` 产出 ELF；`cargo build -p minix-init --target x86_64-unknown-none` 产出 ELF。

---

### D-15 命令覆盖 65/328，`sh` 与基础文件工具全缺（已登记为 S35 长尾）

**证据**：
- 现有 bin 65 个（本次实测 `os/commands/**/src/bin/*.rs` 计数 105 项含 `[[bin]]` 声明，去重后的命令名集合与 plan §5.2 对账命中 65 个）。
- plan.md §5.2（`rewrite-notes/18-stage-commands/plan.md:253-380`）列了约 327 个命令名，**261 个没有 bin**；其中首当其冲的是 `sh`、`cat`、`ls`、`cp`、`mv`、`rm`、`mkdir`、`pwd`、`test`、`chmod`、`ps`、`mount`、`date`、`sleep`。
- `sh` 的执行器缺口在文档里已经点名：`18-stage-commands/todo.md`（批次二十六）列出三件前置——运行时 `execve` 面挂 C-21、`pipe2` 走 `VFS_PIPE2`、`F_DUPFD` 常量组。
- 账本登记：edge3.md S35（🔴 长尾批次）、S36（Requires 回填）。

**要点**：目标里的"运行 18-stage-commands 的所有程序"需要先**定义一个可验收的命令集**。建议分两级：**冒烟集**（约 20 个：`sh`、`echo`、`cat`、`ls`、`pwd`、`cp`、`mv`、`rm`、`mkdir`、`test`、`sort`、`wc`、`head`、`grep`、`sed`、`ps`、`kill`、`date`、`stty`、`ed`）与**全量集**（328）。首个里程碑按冒烟集验收，全量集作为持续波次。理由：冒烟集覆盖"fork/exec/pipe/重定向/文件读写/终端"六类原语，这六类一旦打通，后续命令基本是决定半的接线工作。
**验收**：QEMU 内跑一个脚本，逐条执行冒烟集命令并比对退出码与输出（脚本形态见 D-21）。

---

### D-16 `minix-sys` 顶层缺少命令层所需的 25 类原语（新，逐项实测）

**证据（本次实测，按 `pub fn` 名与方法名核对 `os/libs/minix-sys/src/`）**：
- 已有：`send/receive/sendrec/notify`、`fork/exec/exit/sigreturn/waitpid/kill/getpid/getuid/setuid`、`open/close/read/write`、`stat/lstat/fstat`、`ioctl/fcntl/getdents`、`tcgetattr/tcsetattr`、`lseek`、`dup2`、`chroot`、`gettimeofday`、`mmap`、`setitimer`（在 `pm.rs:541` 起）、`nanosleep_via`（`misc.rs:276`）。
- **缺失**：`pipe`/`pipe2`、`mkdir`、`rmdir`、`unlink`、`rename`、`link`、`symlink`、`chmod`、`chown`、`utime`、`chdir`、`getcwd`、`access`、`umask`、`isatty`、`truncate`/`ftruncate`、`sync`、`mount`/`umount`、`mkfifo`、`getpriority`、`uname`、`reboot`/`shutdown`、socket 家族（`os/libs/minix-sys/src/socket.rs` 只有两个纯函数助手，没有请求封装）。

**为什么挡路**：plan §5.2 的多数命令以这些为依赖；S36（Requires 回填）会把这些依赖显式化，但现在就能看出 `ls`/`cat` 之外的基础命令都缺料。
**要点**：按命令波次的顺序补（先 `sh`/`ls`/`cat` 用到的一组：`pipe2`、`dup2`(有)、`mkdir`、`unlink`、`rmdir`、`chmod`、`chdir`、`getcwd`、`access`、`isatty`、`fstat`(有)）。这一条与 edge2 的 L10/L11/L12 属同一面，新增 wrapper 走 edge 登记（`minix-types` 的 wire 布局若需新增，按 C-6 认领）。
**验收**：每组 wrapper 附 wire 形状测试 + 宿主 EIO 契约测试（照 L10 的先例）。

---

### D-17 命令层仍需一个真正的镜像/装机清单（与 D-06 同一件事的命令侧半）

**证据**：`os/commands/sbin/diskfmt/Cargo.toml:15-16` 与 `mountinfo/Cargo.toml:16` 只在注释里写"安装清单待 E-FSBDEV"；`os/xtask/` 没有任何命令名数组（`src/main.rs:123-135` 只有 TODO 骨架）。
**要点**：清单要回答三件事——哪些 bin 进镜像、装到哪个 PATH 层（bin/sbin/usr.bin/usr.sbin）、哪些需要软链（`mkfs.mfs`、`fsck.mfs` 的安装名在 `Cargo.toml` 注释里已提到）。
**验收**：`xtask image` 产出镜像后，用 fsck 引擎读回并断言清单里的每个文件与设备节点都在。

---

## §4 第三优先级：三架构（T5）

### D-18 aarch64 / riscv64 没有生产用户态 trap 入口（已登记 K9/K12b 的边界，本体为新）

**证据**：
- `os/arch/src/lib.rs:262-263`（`install_trap_stubs` 空实现）、`:324-327`（`syscall_entry_va()` 返回 0）、`:346-351`（`register_trap_dispatchers` 空实现），三条都是 `any(target_arch = "aarch64", target_arch = "riscv64")` 分支。
- `os/kernel/src/lib.rs:72-73`：`trap_dispatch` 模块只在 x86_64 下编译。
- aarch64：`os/arch/src/arm64/trap_entry.rs:93-97` 的 Lower-EL 槽全部跳到 `exc_bad_mode`；`:146-168` 只有一条打印并停机的诊断处理。
- riscv64：`os/arch/src/riscv64/trap_entry.rs:52-104` 是单入口的打印并停机向量，没有寄存器保存、没有 `sscratch` 栈切换、没有 `scause` 分派。
- 现状的"用户态可跑"只发生在测试载体里（载体自持向量表与 ecall/svc 处理：`os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:135-168`、`test-rt-birth-riscv64/src/main.rs:156-342`）。
- 账本：edge1.md K9（向量表已落）、K12b（两架构 rt-birth 真机 PASS）——两者都是"载体级"，**生产入口从未登记为待做**。

**为什么挡路**：三架构目标里，非 x86 的服务器与命令要跑，必须先有生产 trap 入口与 syscall 分派；否则第一个用户态指令就停机。

**方案对比**：

| 方案 | 内容 | 判断 |
|---|---|---|
| A（推荐） | 把 x86_64 的两条腿（int 门 + SYSCALL 快路径）在 aarch64/riscv64 上各实现一条：aarch64 用 `svc` + `VBAR_EL1` 的 Lower-EL 槽（保存 `x0..x30`/`ELR`/`SPSR`/`SP_EL0` → 转 `CpuContext`）；riscv64 用统一 `trap_vector` + `scause` 分派 + `sscratch` 换栈（U-mode ecall 与中断共用入口） | 与各架构的 C 实现同形（C 的 earm/earm 也是单入口），复用已有 `CpuContext`/`TrapFrame` 抽象 |
| B | 先只做 `svc`/`ecall`（不做硬件中断） | 服务器要跑必须收定时器与设备中断，半条腿不解决问题 |
| C | 沿用测试载体的"自带 handler"形态 | 把测试替身当生产，等于每个进程都要自持向量表，与 C 的模型背离 |

**落地要点**：分两条腿——先 syscall 腿（拿到"用户程序能进内核并回来"），再 trap 腿（缺页/时钟/设备中断）；缺页回路在 riscv64 有 `scause=12/13/15` 的直译，aarch64 有 `EC=0x24/0x25`。
**验收**：非 x86 各自的 `test-user-trap` 等价载体（现在的 test-rt-birth 只是"诞生"，需要"trap 往返"）；随后是 D-22 的全系统载体。

---

### D-19 服务与命令的交叉构建矩阵（T5 的工程面）

**是什么**：T5 要求三架构复跑 T2~T4。这需要每个 crate 在三种 target 下都能构建；现在连 x86_64-unknown-none 都过不去（D-05/D-14），aarch64/riscv64 更无从谈起。
**要点**：在 D-05/D-14 完成后，把 `cargo check --target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}` 做成一个脚本（新增独立脚本不需登记，属 edge1 领地），纳入 CI（对照 K13 的 riscv64 U-Boot job 形态）。
**验收**：三 target × 全 crate 的构建矩阵全绿。

---

## §5 第四优先级：非 rewrite 的 translate 倾向与代码缺陷（"再确保好"）

> 判定依据：`.claude/rules/review-core.md:27`（Translate = 1:1 C→Rust，禁止）与 `prompt/review-rules/review-patterns.md` 模式 16（裸整数表达语义）、模式 65（Translate 倾向反模式）。以下只列本次扫描实际命中的、且**影响可达性或在生产路径上真实发生**的项；纯风格项不在本清单（留给 full-review）。

### D-20 生产路径上的调试残留与吞错（缺陷，逐条可修）

| 编号 | 缺陷 | 锚点 | 说明 |
|---|---|---|---|
| D-20a | `SYS_DIAGCTL` 分发里留了 5 处 `TEMP-DEBUG` 串口打印 | `os/kernel/src/syscall.rs:2659`、`:2662`、`:2718`、`:2733`、`:2740` | 每次诊断调用往串口吐调试行，污染控制台（D-10 打通后尤其明显）；属调试残留，直接删 |
| D-20b | RS 主循环返回值被丢弃 | `os/servers/rs/src/main.rs:67`（`let _ = server.run();`） | 根服务器静默退出，系统无守护者；C 的形态是 panic |
| D-20c | boot-shim 注释声明了一条不存在的内核行为 | `os/boot-shim/src/loader.rs:44-53`（"内核按 SYSTEM 标志重排模块"） | 与 D-01 同源；注释与代码不一致属 P1（文档-代码同步门） |
| D-20d | 测试载体伪造 12 项零长度模块，掩盖生产清单缺陷 | `os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs:88-100` | 修 D-01 后应改为从装载器清单派生 |
| D-20e | VFS 有一条实际不可达的 `SefEvent::Init` 臂 | `os/servers/vfs/src/main_loop.rs:7153` | 臂体是 `state.init_fresh()`，写法无害，但 minix-sef 从不产生该事件（D-02 的根因），所以这条臂至今没被执行过——RS_INIT 实际落到 `:694-697` 的 Enosys 分类上。修 D-02 时一并消解 |
| D-20f | 未知 VMCTL 参数回 `ENOSYS`，C 回 `EINVAL` | `os/kernel/src/syscall.rs:2219`（附近注释自述是**有意的**区分） | 代码里写明「为与 valid-but-unimplemented 区分而回 ENOSYS」。这与 C 的 arch_do_vmctl 行为不同，属显式偏离；按 ground-truth 链（C > design > code）应由 design 侧给出理由并三处标注，否则按 P1 收口 |
| D-20g | fork 的父进程 FPU 卸载是 no-op | `os/kernel/src/proc.rs:1670` | 自述"per-CPU FPU 归属路径未接"；SMP 上父子 FPU 状态可能互相污染 |
| D-20h | 非 BSP 的 TSS `sp0` 是占位 | `os/arch/src/x86_64/protection.rs:370` | 自述等 per-AP `set_kernel_stack`；SMP 下 AP 上的中断会用到错误的核栈 |
| D-20i | DS / input / ipc-server 用裸 `receive`，没有 SEF 的 ping 透明与 init 拦截 | `os/servers/ds/src/server.rs:163-180`、`os/servers/input/src/serve.rs:79-82`、`os/servers/ipc-server/src/boundary.rs:490-499` | C 里 ping 应答与 init 拦截由 libsys 的 sef 层统一承担；裸 receive 的服务会被 RS 判死（ping 无应答）——这是 D-02 的同一根因在传输层的表现 |

### D-21 `static mut` 与 unsafe 密度的局部过热（translate 味道，需逐点裁决）

**本次实测**：全 `os/` 非测试代码 `static mut` 共 81 处，多数落在内核启动/中断的合理边界（例：`os/arch/src/x86_64/protection.rs:324` 的 IST 栈、`os/kernel/src/smp.rs:1047` 的 AP 核栈，都是被汇编直接使用的静态区）。unsafe 密度超 3/100 行的文件 21 个，最高的是 `os/libs/minix-usb/src/wire.rs`（10/134）、`os/servers/pm/src/ipc/decode.rs`（31/461）、`os/arch/src/x86_64/trap_stub.rs`（58/846）。
**判断**：内核/架构面的高密度属可接受（页表与 trap 帧必然如此）；**PM 的 `decode.rs`（6%）值得单独裁决**——账本记过"S11 已把 decode 单点化"（edge3.md S11），但 31 处 unsafe 说明内联解码仍在。属"每处给理由或消除"的清单项，不影响可达性。
**要点**：不批量清理（会污染 diff 且风险高），按触及原则逐步收；本条只作为**登记**，供后续 code-excellence 会话领。

### D-22 已知的语义偏差登记（不修，但要记住）

| 项 | 锚点 | 偏差 |
|---|---|---|
| VM 冷重启直接 panic | `os/servers/vm/src/main.rs:41-49` | VM 不接受重启（C 有 is_first_time 分流）；影响"reincarnation"完整性，不影响首启 |
| `KernPhysMap/KernMapReply` 回 ENOSYS | `os/kernel/src/syscall.rs:2331` | 32 位专用调用，属既有 WONTFIX |
| `SYS_IOPENABLE`/端口 IO 在非 x86 回 BadCall | `os/kernel/src/syscall.rs:377-383` | 非 x86 驱动走 MMIO，可接受；但意味着非 x86 的驱动面与 x86 不同形 |
| `sys_statectl` 无 wrapper | `os/libs/minix-sys/src/` 无命中 | C 的 `process_init` 会设状态表（`sef_init.c:141`）；状态迁移只在 live update 用，首启不受影响 |

---

## §6 建议执行波次

排序原则：**先让一条最短路径真机走通，再拓宽**；每一波结束后系统仍可构建、宿主回归仍绿。

| 波次 | 内容 | 依赖 | 出口判据 |
|---|---|---|---|
| W1 契约与阻塞 | D-01（模块清单）、D-02（出生面事件）、D-03（rproctab 授权）、D-20a/b/c/d | 无 | 宿主回归绿；RS step3 在宿主模拟里能收齐全部 12 家的答复 |
| W2 可引导二进制 | D-05（服务裸机化）、D-14（init/命令裸机化，先做 init + echo）、D-12（whoami） | 无（与 W1 并行安全，触碰面不同） | 三 target 下 `cargo build` 产出 ELF |
| W3 启动镜像 | D-06（xtask image + proto）、D-07（memory 进程）、D-11（tty 进程） | W1、W2 | 产出完整镜像目录：kernel + 12 模块 + 根镜像 |
| W4 文件系统链 | D-08（挂根 + map_service）、D-09（真块源） | W3 | 真机：VFS 挂根成功、MFS 能读根目录 |
| W5 控制台 | D-10（字符设备读写 + 串口后端） | W3、W4 | 真机：用户程序写 stdout 出现在串口 |
| W6 命令冒烟 | D-15 冒烟集、D-16（对应原语）、D-17（装机清单） | W5 | 真机：冒烟集命令逐条执行并比对输出 |
| W7 init 与 rc | D-04（read_exec）、init 的 runcom 真跑 | W6 | 真机：init 进 multi-user，能 exec /etc/rc 与 login |
| W8 三架构 | D-18（非 x86 trap 入口）、D-19（构建矩阵） | W6（x86 全通后） | 三架构各自的 T2~T4 复跑 |

与既有账本的对应：W1 覆盖 edge4 §3 状态板里「S42 ④ 载体半程 / C-29」之外**未被登记的**两块（模块清单、出生面应答覆盖）；W3/W4/W5 是 edge3.md S32「装机面」与 edge2.md L17「波次 1」的具体化；W8 对应 T5。

---

## §7 验收与测试载体建议

1. **生产 boot 冒烟**（W1 出口）：新测试内核或脚本，用**真 boot-shim 产物**（kernel.elf + 12 模块）启动，断言 VM 起来并进入 RS 握手。现在没有任何测试覆盖生产装载路径——这是 D-01 能潜伏至今的原因。
2. **服务出生面覆盖测试**（W1 出口）：对 12 个 boot 表成员逐一断言"收到 RS_INIT（source=RS）→ 回 RS_INIT+result"。宿主接缝测试即可，不必真机。
3. **真机多进程载体**（W3/W4 出口）：`test-sysboot`（已入库，`os/qemu-tests/test-sysboot.sh`）是现成的形态，待 C-29 与 W3 落地后即可纳入 `run_all.sh`（现在它故意不在 CI 里）。
4. **命令冒烟脚本**（W6 出口）：QEMU 里跑 `sh -c` 序列或逐条 exec，逐条比对退出码与 stdout；脚本形态可照 `os/qemu-tests/test-sysboot.sh` 的串口标记判定（不做 gdbstub 信箱，降低环境依赖）。
5. **三架构矩阵**（W8 出口）：`run_all.sh` 增加构建矩阵 job + 各自的命令冒烟。

---

## §8 本次扫描的方法与范围（供复核）

**扫描范围**：`os/`（kernel、arch、plat、boot-shim、servers×11、libs、fs、net、drivers×57、commands×24、qemu-tests、xtask）+ `rewrite-notes/` 的账本（edge1/2/3/4、edge_todo、18-stage todo/plan、16-stage todo）+ `minix3/` 对照（`servers/rs/main.c`、`lib/libsys/sef*.c`、`releasetools/image.functions`、`servers/vfs/main.c`、`drivers/tty/tty/arch/i386/console.c`）。

**实测命令（择要）**：
- `cargo check -p minix-pm --lib --target x86_64-unknown-none -j 1` → 1103 errors（lib 无 `no_std`）。
- `cargo check -p minix-vfs --lib --target x86_64-unknown-none -j 1` → 29 errors（`minix-sockdriver/src/sdev.rs:38` 起 `derive`）。
- 其余九家 servers lib 同 target 零错误。
- `grep -rn "RS_INIT" os/servers/ os/libs/ os/net/` → 应答实现只有 sched / devman / fs-rt / net 两家 + VM 握手。
- `grep -rn "caller: &mut KProcess"`、`static mut` 计数、unsafe 密度统计、`.proto` 全仓检索、`MODULE_NAMES` 与 `NR_BOOT_MODULES` 对账、`pub fn` 逐名核对 libc 面。

**本清单**未覆盖（受扫描预算限制，如实登记）：各 stage `doc_rerank_*.md` 的重建蓝图内容、05-stage-vfs 的 64 条臂逐条残余缺口、net 家族（lwip/uds）的后续批、16-stage 的 NIC 波次细节。这些在既有账本里已有条目，本清单只在它们挡住启动链时才收录。

**与既有账本的关系**：D-01、D-02、D-03、D-05、D-12、D-14、D-18、D-20 各条为**本次新发现或首次指名**（已在条文里逐条注明）；其余是既有登记条目在本清单里的可执行化（给出了具体的下一步与验收方式）。建议在执行前把新条目按 edge4 §1 规则 7 追加到所属线（D-01/D-13/D-18 → edge1；D-02/D-03/D-14/D-16 → edge2；D-04/D-06~D-12/D-15/D-17 → edge3）。
