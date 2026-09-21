# new_todo_HY4.md — 三架构 QEMU 启动 minix-rs 并跑通 18-stage-commands 全部程序的缺口清单（HY4 独立扫描）

> **作者标识**：HY4。本文件是两份（或多份）独立扫描产物之一，**不引用、不合并、不依赖**任何其它 `new_todo_*.md`，自包含、以 `HY4` 后缀隔离。
> **日期**：2026-09-20。工作树位于 `rewrite` 分支（HEAD 在 S42 批五之后的一串提交附近）。
> **目标（用户给出）**：在 x86_64 / aarch64 / riscv64 三架构 QEMU 上启动 minix-rs，并运行 `18-stage-commands` 的全部程序。
> **优先级口径**：第一优先级是「先确保有」——把启动到命令执行的链路打通；第二优先级才是「再确保好」——非 rewrite 的 translate 漂移与代码缺陷。
> **方法**：直接对 `os/` 生产代码做 grep/逐文件核对（本次未使用子代理，全部锚点由 HY4 本人当场 `grep`/`sed`/`read` 复核）；对照真值 `minix3/` C 源。**锚点是快照，会漂移——执行每条前仍须按 fix-guard 重读目标行 ±5 行核实，不依赖本文锚点的时效性。**
> **编号约定**：`H-P0-nn` = 硬阻断（不通则下游全部不可达）；`H-P1-nn` = 目标必需但存在更短替代路径；`H-P2-nn` = 第二优先级（translate 漂移、卫生、保真度缺陷）。
> **Rewrite 边界**：项目强调 rewrite not translate。下文的修复一律不得改变外部可观察行为；凡需改变 C 可观察行为的，按 `[ARCH: ...]` 三处一致（doc + design + code）登记，不擅自裁决。

---

## 0. 一句话结论：链断在哪

**宿主（hosted）"决定半"远远领先于真机"可运行半"。** 仓内已有大量忠实于 C 的决策逻辑与宿主单测（66 个命令二进制、11 个服务器库、9 个 fs 库、数千测试），但把"决定半"变成真机上跑起来的进程所需的**四个工程面**基本不存在：

| # | 工程面 | 现状 | 断点 |
|---|---|---|---|
| 1 | 可启动二进制 | 只有 `minix-init` 有 `target_os="none"` 条款；PM/VFS 库连 `no_std` 都没声明；53/55 个驱动的 `main.rs` 是 7 行 `loop {}` 桩 | 没有东西可以装进 boot image |
| 2 | boot image 组装 | `xtask image` / `xtask qemu` 打印 `[skeleton]` 后返回 Ok | 没有从 ELF 到镜像到 QEMU 的机械路径 |
| 3 | 出生握手（RS ↔ 服务） | `minix-sef` 定义了 `SefEvent::Init` 但从不构造；PM/VFS/DS/MIB 从不回 `RS_INIT`；VM 收到后返回 `Suspend` 从不回 | RS step 3 永久阻塞 |
| 4 | 根文件系统 + 控制台字节路径 | VFS `do_init_root` 只翻相位旗标不挂载；MFS 块源恒 `EIO`；memory 驱动不存在；tty 后端无真实 console/serial | 命令即使起来也读不到文件、写不出屏幕 |

三架构（aarch64/riscv64）在此之上**再加一层**：这两个架构根本没有生产级陷阱入口（诊断 `wfi`/`wfe` 停机桩），`arch` 门面的 `install_trap_stubs`/`register_trap_dispatchers`/`syscall_entry_va` 在非 x86_64 下全是空实现或 0。

**与既有台账的关系**：`edge_todo.md` 已登记的条目（E1/E5/E-FSBDEV/E-FSRUNTIME/E-FSCMDS/E-INITSYS/E-CMDSYSFACE/E-BOOTFRAME/E-VMTLB/E-IPCWIRE 等）本文只**引用**不复述；标 **NEW** 的是本次首次以锚点钉到启动链上的阻断项，或此前埋在 stage 笔记里、现在显式定位到生产路径的项。

---

## 1. P0 硬阻断（按依赖顺序）

### W1. 可启动二进制与镜像装配（一切的前提）

#### H-P0-1（NEW）boot-shim 的模块清单与内核 boot module 契约在数量、顺序、映射规则三方面都不一致

- **事实**：
  - `os/boot-shim/src/loader.rs:54`：`pub const MODULE_NAMES: &[&str] = &["vm", "pm", "vfs", "rs", "ds", "inet"];`（**6 项**）
  - `os/kernel/src/proc.rs:81`：`pub const NR_BOOT_MODULES: usize = 12;`
  - `os/kernel/src/proc.rs:106-119`：`BOOT_MODULE_PROC_NRS` = C `image[]` 顺序 DS(6), RS(2), PM(0), SCHED(4), VFS(1), MEM(3), TTY(5), MIB(7), VM(8), PFS(9), MFS(10), INIT(11)
  - `os/kernel/src/lib.rs:1131-1137`：`assert_eq!(kernel_info.boot_modules().len(), NR_BOOT_MODULES)` — 不等即 panic
  - `os/kernel/src/lib.rs:1185-1194`：槽位赋值是 `BOOT_MODULE_PROC_NRS[i]` 的**纯位置映射**，`proc.set_boot_name(module.name)` 只拷名字、不校验该名字是否属于该槽
  - `os/boot-shim/src/loader.rs:259-264`：缺模块静默跳过 → 随后必然撞上上面的 assert
- **为什么是硬阻断**：生产镜像在 `kmain` 的 assert 处直接死。即使绕过 assert，索引 0 的 `vm` ELF 会落进 DS 槽（`ProcNr(6)`）、`pm` 落进 RS 槽……`inet` 在 C 里根本没有 boot 槽。**全部端点错位**，RS step 1 的 `lookup_image` 立刻失败。
- **为什么现有测试没暴露**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs:88-101` 手工伪造了 12 项 C 顺序表，绕过了 boot-shim。
- **设计对比**（对照 C `minix3/minix/kernel/main.c:171` 的 `kinfo.module_list` 语义）：
  - **A（推荐）**：boot-shim 按 C 顺序列 12 个名字、按名加载、缺件报错而非静默跳过；同时删掉 `loader.rs:38-53` 那段"内核按 SYSTEM 旗标重排、顺序解耦"的注释（该说法与 `lib.rs:1188` 的纯位置映射矛盾，属于文档说谎）。改动限于 boot-shim。
  - **B（否决）**：改内核按名查找。这会重写内核契约，`BOOT_MODULE_PROC_NRS` 的位置语义本身是 C 行为，改它是架构演进不是 rewrite。
  - **C（否决）**：放宽 12 项 assert、建部分表。TTY/MEM/MFS/INIT 的缺席会在 RS step 2/3 的计数处爆，只是把失败点挪到下游。
- **验收**：一个用真实 boot-shim 产物（kernel.elf + 12 模块）启动的 QEMU 冒烟，能到达 VM/RS 握手；`test-sysboot` 载体停止手工伪造模块表，改为从 loader 清单派生。

#### H-P0-2（NEW）服务器二进制没有 freestanding 构建面；PM/VFS 库甚至是 std-only

- **事实**（2026-09-20 实测）：
  - `os/servers/pm/src/lib.rs` 与 `os/servers/vfs/src/lib.rs` **没有任何** `cfg_attr(not(test), no_std)`；其余 9 个服务器库（devman/ds/input/ipc-server/is/mib/rs/sched/vm）都有。
  - `os/libs/minix-sockdriver/src/lib.rs` 同样无 `no_std` 声明（P1，只影响网络面）。
  - 全仓 `grep -rln 'target_os = "none"' --include=Cargo.toml` **只有一条命中**：`os/commands/sbin/init/Cargo.toml:33-37`。11 个服务器的 `main.rs` 都没有该条款。
  - 唯一模板就是 init：`[target.'cfg(not(target_os = "none"))'.dependencies] minix-rt = { features = ["std"] }` + `[target.'cfg(target_os = "none")'.dependencies] minix-rt = { features = ["alloc-global"] }`，`os/commands/sbin/init/src/main.rs:19` 起。
  - workspace 侧已备好：`os/.cargo/config.toml:33-34`（static 重定位 + large code model，为内核 ELF 加载器服务）。
- **为什么是硬阻断**：`cargo check -p minix-pm --target x86_64-unknown-none` 在库链接 std 时不可能通过，VFS 同理。没有 freestanding 服务器就没有 boot image 内容。
- **修复方向**：按 init 模板逐 crate 复制（target 门控的依赖分叉、`cfg_attr` 三件套 `no_std`/`no_main`/panic handler、rt-birth 形状的链接脚本）。顺序：PM 先（唯一连库都编不过的），VFS 次之，然后 11 个 `main.rs`。每步保持宿主测试绿，**不要**翻 workspace 级默认值——Cargo feature 统一陷阱会在 `-p minix-arch` 与 boot-shim/测试内核共用一次调用时咬人，分开 invocation 构建。
- **验收**：11 个服务器 `cargo check -p minix-<srv> --target x86_64-unknown-none` 全清，且各自产出一个可链接 ELF。

#### H-P0-3（NEW）12 个 boot module 中绝大多数没有可运行入口

- **事实**：
  - **驱动**：`find drivers -name main.rs | wc -l` = 55，其中 **53 个是 7 行桩**，形如 `fn main() { minix_driver_memory::init(); loop {} }`（`os/drivers/storage/memory/src/main.rs` 全文 7 行；`virtio_blk`、`virtio_net`、全部 net/audio/bus/power 同款）。`os/drivers/storage/memory/src/` 的库本体有 1312 行真实逻辑，但入口不跑事件循环。
  - **TTY（MEM/TTY 两个 boot 槽）**：`os/drivers/tty/tty/src/main.rs:29` 是真实形状，但 `:16-18` 自陈 `let self_endpoint = Endpoint::NONE;` —— "直到 RS 在 SEF startup 期间分配（真实启动，edge E5）"。
  - **MFS/PFS**：`os/fs/mfs/src/main.rs:11-12` 自陈块源是 `PendingBlockSource`；`os/fs/fs-rt/src/source.rs:19-31` 该类型的 `read_block`/`write_block` **恒返回 EIO**。
  - **PM/VFS/INIT**：PM `main.rs` 无 RS 握手也无 `VFS_PM_INIT` 同步（`os/servers/pm/src/main.rs:8-28` 注释自陈"无 RS_INIT 握手"）；VFS `main.rs` 全文 7 行 `main_loop::run()`。
- **为什么是硬阻断**：C 的 boot image 12 项里 MEM（ramdisk 根设备）、TTY（控制台）、MFS/PFS（根文件系统）、PM/VFS（进程与文件）全是必需的。今天这 12 槽里只有 init 有真形状。
- **修复方向**：MEM、TTY、MFS 三个是最小启动集（对应 C `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, ...)`，`os/servers/vfs/src/main_loop.rs:529-534` 注释已记录该调用形状）。驱动统一走 `minix-driver-rt` 的 `DriverRuntime::serve`（tty 已是样板），把 53 个 7 行桩换成该模板。

#### H-P0-4（NEW）`xtask image` / `xtask qemu` 是骨架，没有 proto 文件、没有安装清单、没有 rootfs

- **事实**：
  - `os/xtask/src/main.rs:123-133`：`fn image()` 打印 `[skeleton]` 并 `Ok(())`；注释自列四项待做（收集 boot image ELF、按 `minix3/releasetools/mkboot` 打包、mkfs + fstab 生成 rootfs、输出到 `os/target/image/`）。
  - `os/xtask/src/main.rs:136-140`：`fn qemu()` 同样 `[skeleton]`，"待实现：参考 os/qemu-tests/run_qemu.sh"。
  - 全仓 `find -name "*.proto"` 零命中；唯一的 fstab 命中是**库**（`mountinfo/src/fstab.rs`），不是 66 个二进制到 `bin`/`sbin`/`usr.bin` 层的映射。
  - `os/etc/` 只有一个 `README.md`（自述占位）。
- **为什么是硬阻断**：即使上面三条都修好，也没有机械路径把 ELF 变成镜像再变成 QEMU 命令行。
- **修复方向**：对齐 C 的 `minix3/releasetools/mkboot` + `distrib/common/bootimage/fstab.in`。分两步：先做"boot image 打包 + QEMU 启动"（只需内核 + 12 模块），再做"rootfs + 命令安装清单"（需要 mkfs，依赖 H-P0-11/12）。

---

### W2. RS 出生握手（服务进不了运行态）

#### H-P0-5（NEW）`minix-sef` 定义了 `SefEvent::Init` 却从不构造它

- **事实**：
  - `os/libs/minix-sef/src/lib.rs:63-71` 定义 `Init(i32)`（含 fresh/LU/restart 三态注释）。
  - `os/libs/minix-sef/src/lib.rs:130-169` 的 `sef_receive_status` 循环只有三条出路：`:151` 返回 `Signal`、`:154-159` 命中 ping 后 pong 并 continue、`:167` 返回 `Call(m_type)`。**没有任何一条构造 `Init`。**
  - `:154` 的判据是 `source == RS_ENDPOINT && m_type == SEF_PING_REQUEST_TYPE`（即 `NOTIFY_MESSAGE`）；`RS_INIT`（0x714）不等于它，所以 RS_INIT 直接落到 `:167` 当普通消息交给业务分发表。
- **对照 C**：`minix3/minix/lib/libsys/sef.c:197` 有 `case SEF_INIT_REQUEST_TYPE:` 分支 → `:134` `do_sef_init_request(&m)`；`sef_init.c:115` 回 `m.m_type = RS_INIT`。C 把出生拦截放在 libsys 里（主循环之前阻塞等 RS、应答 RS_INIT + result、重载表），Rust 侧没有等价的库行为。
- **后果**：每个服务器里所有 `SefEvent::Init` 臂今天都是死代码。
- **设计对比**：
  - **A（推荐）**：在 `minix-sef` 加出生面——按 RS 端点 + `SEF_INIT_REQUEST_TYPE` 识别，产出 `SefEvent::Init(type)`，并提供一个共享应答 helper（source 门 + fresh/LU/restart 三分支）。一处修好服务全部消费方，与 C 的 libsys 分层同构。
  - **B（否决）**：每个服务各抄一份拦截。这就是现状的延长线，新服务会继续漏（input、ipc-server 已经漏了）。
- **验收**：每服务器一个单测——RS 来的 RS_INIT 产出 Init 事件且绝不进入业务分发表；受影响 crate 的宿主回归保持绿。

#### H-P0-6（NEW）出生应答覆盖面：PM、VFS、DS、MIB、IS、input、ipc-server 从不回 RS_INIT

- **事实**（2026-09-20 实测，grep `RS_INIT` 全仓，排除注释）：
  - **会答的**（4 处）：sched `os/servers/sched/src/kernel_api/transport.rs:173-188`（类型 + source 双门，回 `RS_INIT`+OK）；devman `os/servers/devman/src/ipc/minix.rs:278-295`；fs-rt `os/fs/fs-rt/src/transport.rs:139-167`；VM `os/servers/vm/src/vm_server.rs:1317-1339`（**只消费、不回**，见 H-P0-7）。
  - **零命中的**：`os/servers/pm/src/`（唯一命中是 `main.rs:11` 注释）、`os/servers/vfs/src/`（**整个 vfs crate 零命中**）、`os/servers/ds/src/`、`os/servers/mib/src/`、`servers/is`、`servers/input`、`servers/ipc-server`（后三者只有 sef 注释里的提及）。
  - `net/lwip/src/server.rs:361-403` 与 `net/uds/src/server.rs` 有 `RS_INIT` 但走的是 `ScriptedIpc` 测试脚本面。
- **对照 C**：`minix3/minix/servers/rs/main.c:375-407` —— step 2 给每个 SYS_PROC 发 RS_INIT，step 3 阻塞等全部应答；每个服务主循环前先跑 `sef_startup`（如 `pm/main.c:115-125`、`vfs/main.c:387`）。
- **为什么是硬阻断**：`os/servers/rs/src/boot.rs:1133-1136` 的 `while self.nr_uncaught_init_srvs > 0 { catch_boot_init_ready(ANY) }` —— 少一个应答就永久阻塞；`:1153-1166` 消息类型不对或非 0 result 直接 panic。
- **修复方向**：优先补 PM 与 VFS（boot image 必需），再 DS/MIB，再 IS/input/ipc-server。与 H-P0-5 同批做可以少一半重复代码。
- **验收**：每服务一个"RS 来的 RS_INIT → 回 RS_INIT+OK，业务分发表不受影响"的 seam 测试；RS step 3 在宿主模拟下对 12 个 boot 成员全部排空。

#### H-P0-7（NEW）VM 收到 RS_INIT 后返回 `Suspend`，从不向 RS 回 RS_INIT + result

- **事实**：`os/servers/vm/src/vm_server.rs:1317-1339`：
  - `:1320` `Ok(()) => return DispatchAction::Suspend`
  - `:1321-1336` `Err(e) => … return DispatchAction::NoReply`（丢消息 + 计数 + 审计）
  - 两条路**都不向 RS 发应答**。
- **为什么是硬阻断**：`os/servers/rs/src/boot.rs:1074-1077`：VM 走 `init_service` 后 `nr_uncaught_init_srvs += 1`；step 3 就等这一个应答。VM 永远不答 → **整条启动链在此永久挂死**。即使 H-P0-6 全修好也不够。
- **对照 C**：`minix3/minix/servers/vm/main.c:249-255` —— `sef_cb_init_fresh` 走完 rproctab 后由 `sef_init.c:115-121` 的 process_init 尾部统一发 `RS_INIT + result`。Rust 侧缺的就是这半。
- **修复方向**：VM 握手成功后补发 `RS_INIT + OK` 给 RS；失败路径保留 fail-closed 但需与 RS 的 watchdog 语义对账（C 是 panic，Rust 侧 [ARCH: A-14] 已裁决停机，需确认 RS 是否把它当崩溃处理）。
- **注意**：`:1170` 已有 `if m.m_source != Endpoint::VM { sys.reply(...) }` —— RS 侧知道 VM 是异步的，但**它仍然把 VM 计入 `nr_uncaught_init_srvs`**，说明 C 里 VM 是发了消息的（`main.c:369-370` 注释即"VM will still send an RS_INIT message"）。Rust 侧没发，是漏实现不是设计。

#### H-P0-8（NEW）rproctab grant 没有生产创建点，VM 永远拿不到 boot ACL

- **事实**：
  - `os/servers/rs/src/boot.rs:951-954`：`// C: rinit.rproctab_gid = cpf_grant_direct(...) — main.c:185. // Creation point. DEFERRED: grant syscall wiring is 19's scope; the field stays None until then` → `self.rinit = RinitState::default();`
  - `:1044` `let gid = self.rinit.rproctab_gid;`（None）→ `:2271` / `:2639` / `:2721` 断言编码为 `-1`（GRANT_INVALID）
  - trait 面已有 `grant_read` / `grant_revoke`（`:297`/`:300`），但生产实现是空桩（`:522-526`）。
- **为什么是硬阻断**：VM 的 `rs_handshake`（`vm_server.rs:1319`）拿到 gid=-1 必然失败 → 走 H-P0-7 的 NoReply 分支 → RS 挂死。
- **修复方向**：boot step 0 通过新的 `KernelApi` grant 动词创建（C `cpf_grant_direct` 等价物，检查内核 `dispatch_setgrant` 的门），落进 `rinit.rproctab_gid`。两侧用 seam 测试钉住（RS 断言 step 0 后 `is_some`；VM 断言非法 gid 时给出干净的失败应答）。

---

### W3. 三架构用户态入口

#### H-P0-9（NEW）aarch64 / riscv64 没有生产级陷阱入口，arch 门面在非 x86_64 下是空实现

- **事实**：
  - riscv64：`os/arch/src/riscv64/trap_entry.rs:66-78` 的 `trap_vector` 读 `scause`/`stval`/`sepc` 后调 `riscv64_trap_diag`（`:88`）打印并 `wfi` 死循环（`:101`）。文件自身注释 `:61-64` 把寄存器保存、`sscratch` 栈切换、scause 分派（timer/IPI/ecall 三腿）明确指派给"未来的 bring-up lane"。
  - arm64：`os/arch/src/arm64/trap_entry.rs:84-102` 十六个槽里**十二个是 `exc_bad_mode`**（含全部 lower-EL 槽），`:112`/`:132` 是 `1: wfe` 停机；`:140` 起是诊断 reporter。
  - `os/arch/src/lib.rs:247-255` 的注释更狠：两架构的 `load()` 路径引用了"已声明但从未定义"的 asm 符号（`exc_vector_table` / `trap_vector`，两个模块里零 `global_asm!`），目前只因无人调用 `load()` 被死代码消除掩盖。
  - 门面：`lib.rs:262-263`（非 x86 `install_trap_stubs` 空）、`:324-327`（非 x86 `syscall_entry_va()` 返回 0）、`:346-351`（非 x86 `register_trap_dispatchers` 空）、`:295-296`（非 x86 `save_frame_to_context` 空）、`:309-310`（`sync_status_register_to_frame` 空）、`:320-323`（`ipc_return_code` 返回 0）。
  - 内核侧：`os/kernel/src/lib.rs:71-73` `#[cfg(target_arch = "x86_64")] pub mod trap_dispatch;`，`:68-70` 注释自陈"aarch64/riscv64 没有生产分派入口，其 bootstrap 载体自带 handler"。
- **为什么是硬阻断**：这两架构上任何陷阱（SVC/ecall、定时器、IPI、缺页）都停机。现有的 `test-rt-birth-aarch64` / `test-rt-birth-riscv64` 之所以 PASS，是因为**每个载体自带向量**，不是生产路径。
- **设计对比**：
  - **A（推荐）**：每架构先做一条腿（aarch64 `svc` + `VBAR_EL1` lower-EL 槽保存 x0-x30/ELR/SPSR/SP_EL0 进 `CpuContext`；riscv64 统一 `trap_vector` + sscratch 切换 + scause 分派），复用既有 `CpuContext`/`TrapFrame` 抽象；随后补 fault/IRQ 腿。与 C 的 per-arch 形状一致。
  - **B（否决）**：只做 syscall 腿不做硬件中断。服务器需要定时器与设备中断才能跑，半条腿启动不了系统。
  - **C（否决）**：延续"每个进程自带 handler"的载体模式。这会让测试替身成为生产模型，且违背 C。
- **验收**：每架构一个 `test-user-trap` 等价物（陷阱往返，不只是出生），跑生产向量。

#### H-P0-10（NEW）x86_64 的异常腿与 fast-syscall 返回路径都会 panic

- **事实**：
  - `os/kernel/src/lib.rs:3016-3023`：`match return_seq { Some(FullContext) => {} ... Some(FastSyscall) => panic!("restore_user_context: fast-syscall return sequence lands with S-8") ... None => panic!("no entry trap style known") }`
  - `os/kernel/src/trap_dispatch.rs:196-251`：异常分派的分类（`:210` `ForwardToVm`、`:243` `Signal`）已写出但**无人消费**；`other` 臂 panic "needs per-CPU process context (S-6/S-7)"。
- **为什么是硬阻断**：任何用户缺页、异常、信号、惰性 FPU 陷阱都会杀掉内核；任何经 SYSCALL（LSTAR）进入的进程无法返回用户态。`rt-birth` 之所以过，是因为 `load_vm_elf` 预建了每一页、且走的是 int-33 腿。真实命令必然踩到这两条。
- **修复方向**：fast-syscall 返回序列（S-8 的 asm 出口工作）；异常腿接 `ForwardToVm`/`Signal` 到 VM 与 PM 的信号投递。

---

### W4. 根文件系统与控制台

#### H-P0-11（NEW）VFS `do_init_root` 只翻相位旗标，什么都没挂

- **事实**：`os/servers/vfs/src/main_loop.rs:517-539`：
  - `:523` `boot_phase = Mounting` → `:526` `set_accept_requests(false)` → **`:528-534` 全段是注释**（`main.c:505` 的 `mount_pfs()`、`main.c:508-518` 的 `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, "mfs", "fs_imgrd")`）→ `:537` `set_accept_requests(true)` → `:539` `boot_phase = Running`
  - `:528` 注释自陈"执行编排归 18，决策件 mount.rs 已备"；`:362`/`:405` `root_fs_e: Endpoint::NONE`
- **为什么是硬阻断**：根不挂载，任何路径解析都失败，一个命令都跑不起来。
- **修复方向**：`mount.rs` 的决策件（`:430-453` `is_mount_root` 分支）与 `fs-rt` 的 `req_readsuper` 往返件（`main_loop.rs:531-533` 注释说是 S14 已备）都在，缺的是把 `mount_fs` 的编排真正执行。依赖 H-P0-12/13。

#### H-P0-12（NEW）MFS 的块设备源恒返回 EIO

- **事实**：`os/fs/mfs/src/main.rs:11-12` 自陈"block source is `PendingBlockSource` — every block touch answers `EIO` until the block-driver seam (the tracked E-FSBDEV item) wires a real device channel"；`:32` `MfsServer::new(minix_fs_rt::source::PendingBlockSource)`。`os/fs/fs-rt/src/source.rs:21-31` 该 impl 两个方法都 `Err(Errno::from_i32(EIO))`。
- **为什么是硬阻断**：即使根挂上，读一个块就 EIO。

#### H-P0-13（NEW）memory 驱动（根 ramdisk 设备、MEM boot 槽）不存在

- **事实**：`os/drivers/storage/memory/src/main.rs` 全文 7 行：`fn main() { // TODO: 实装为真实服务进程（事件循环 + RS 启动协议）。 minix_driver_memory::init(); loop {} }`。库本体（`device.rs` 351 行 / `transfer.rs` 482 行 / `block_face.rs` 254 行）有逻辑，入口不跑。
- **对照 C**：`minix3/minix/drivers/memory/` 是 boot ramdisk（`DEV_IMGRD`）的后端，`main_loop.rs:529` 的挂载调用就是挂它。
- **修复方向**：按 tty 的 `DriverRuntime::serve` 模板换成真服务进程；同时确认 `INIT` 之前 ramdisk 内容从哪来（boot-shim 传入的模块之一，需与 H-P0-1 的模块表一起定）。

#### H-P0-14（NEW）控制台字节路径在四跳上全断

- **事实**：
  - `os/drivers/tty/tty/src/backend.rs:15-23` 注释自陈：候选输出面中**串口从驱动不可达**（没有 port read/write wrapper），`dev_write`（`:38`/`:107`）只有 Loopback 之类的模型实现。
  - `os/drivers/tty/tty/src/main.rs:16-18`：`self_endpoint = Endpoint::NONE`（等 RS 分配）。
  - 内核侧有 early console（`kernel/src/kmess.rs:130` `console_write_str`、`kernel/src/debug.rs`），但那是**内核诊断**通道（COM1），不是用户态 `write(1, ...)` 的路径。
  - 全仓无 `console` 驱动目录（`find -type d -name "*console*"` 在 `drivers/` 下零命中）。
- **为什么是硬阻断**：命令的输出到不了屏幕，交互类命令（sh、ed、login、games）完全不可用，"运行所有程序"无法验收。
- **修复方向**：最小集 = tty 驱动的真实 console 后端（QEMU 下走 COM1 串口或 VGA）。需要 `minix-driver-rt` 提供端口 I/O wrapper（这是 16-stage-drivers 的缺口，cross-stage）。

---

### W5. 命令执行面

#### H-P0-15（NEW）没有 `sh` 二进制；shell crate 零 bin

- **事实**：`os/commands/bin/shell/src/bin/` 目录不存在；65 个 `src/bin/*.rs` 里没有 `sh`。`18-stage-commands/plan.md` §5.2 把 `sh、ksh、csh、hostname、domainname` 归 05 篇。
- **为什么是硬阻断**：目标写的是"运行 18-stage-commands 的所有程序"。没有 shell 就无法交互式跑；即使改为 init 直接拉起，§5.2 里 `env/getopt/machine/pagesize/printenv/uname` 等也归 05，同样缺。
- **现状**：`minix-shell` 的文字层有 36 个测试（`todo.md:197` 迭代 34 记录），执行器组装原则已预记在 05 篇 §3.5，缺的是执行层——`execve` 面挂 C-21 minix-rt 栈帧裁决、`pipe2` 封装走 `VFS_PIPE2`、`F_DUPFD` 进 fcntl 常量组（同 `todo.md:197` 三件缺口）。

#### H-P0-16（NEW，且是**行为偏离**而非缺实现）PM 的 `do_exec` 加了 C 没有的调用者门，用户 `execve` 会被一律 EPERM

- **事实**：
  - `os/servers/pm/src/exec.rs:100-116`：`do_exec` 开头 `let caller_ep = table.procs[caller.get()].endpoint(); if caller_ep != Endpoint::VFS && caller_ep != Endpoint::RS { return Err(ExecError::Perm); }`
  - **对照 C**：`minix3/minix/servers/pm/exec.c:36-56` 的 `do_exec` 全文**没有任何调用者门**——它只做 `memset` + 组装 `VFS_PM_EXEC` + `tell_vfs` + `return SUSPEND`。EPERM 门在 `do_newexec`（`exec.c:62`，`:70-71` `if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR) return EPERM;`）。
  - **谁在发 PM_EXEC**：`minix3/minix/lib/libc/sys/execve.c:53` —— `(void) _syscall(PM_PROC_NR, PM_EXEC, &m);`，即**用户进程直接发给 PM**。C 的 `pm/table.c:28` 注释也写明 `CALL(PM_EXEC) = do_exec, /* execve(2) */`。
  - Rust 侧 `do_newexec`（`:175`）的门是**正确的**（对位 C exec.c:70-71）；`do_exec` 的门是**多出来的**。
- **为什么是硬阻断**：真实用户 `execve` 由普通用户进程发起，`caller_ep` 是用户自己的端点 → 必然 `EPERM`。init fork 后 exec 任何命令都会失败。"运行所有程序"直接落空。
- **为什么现有测试没抓到**：`exec.rs:331-341` 的 `test_do_exec_forwards` 显式把 `table.procs[0].identity.endpoint = Endpoint::RS` **制造**了一个合法调用者；`:345-352` `test_do_exec_caller_gate` 把这个门当**正确行为**断言了。测试把偏离固化成了契约（test-audit 第 4 维"无效测试"的变体）。
- **修复方向**：删 `exec.rs:110-113` 的门，并把 `test_do_exec_caller_gate` 改写为"任意调用者均可转发"（对照 `exec.c:36-56`）。两个测试一起改，不能只改代码（否则测试红）。修完回头检查 `VfsExec::forward_exec` 的生产实现是否补上了 `tell_vfs` + `VFS_CALL` + `SUSPEND` 语义（`:117-119` 注释说在生产实现内部编码）。

#### H-P0-17（NEW）66 个命令二进制全部是 hosted-std 形态，除 init 外无 freestanding 条款

- **事实**：
  - 命令二进制总数 66（65 个 `src/bin/*.rs` + `minix-init`）。
  - `grep -rln 'target_os = "none"'` 全仓只有 `commands/sbin/init/Cargo.toml`。
  - 命令 crate 的 `minix-rt` 依赖普遍写作 `features = ["std"]`（fileops/regex/textfilter/stdio-games 的 Cargo.toml 实测）。
  - `minix-rt/Cargo.toml` 的 `default = ["std"]`，`std` 形态**不提供** `_start`（`lib.rs:29-36` 自陈）；freestanding 需 `--no-default-features` + `alloc-global`。
- **为什么是硬阻断**：命令 ELF 必须是 freestanding 的才能被内核 ELF 加载器装入 guest。
- **修复方向**：init 模板 + `minix-rt` 的 `default` 已关（`os/Cargo.toml` 的 C-21 注记），逐 crate 加 target 门控依赖分叉。与 H-P0-2 同一批次做，避免两遍改 Cargo。

#### H-P0-18（NEW）`/etc` 配置面零落地，rc/ttys/passwd 全无

- **事实**：`os/etc/` 只有一个 `README.md`。`18-stage-commands/todo.md:266-272` 的 C-6 已登记：`plan.md` §5.3 给 49 项配置文件逐项分配了文档，[ARCH] A-6（rc 脚本 vs 编译期静态 Rust 数据面）是 OQ，悬置未决。
- **为什么是硬阻断**：init 的 rc 驱动状态机（`runcom.rs`、`state_machine.rs`）今天只能对空数据面跑单测；login/passwd 类命令（03 篇）同样压在这个决策上。
- **建议**：不要等 A-6 的"重大决策"裁决完。按"先确保有"：**先做一个最小静态数据面**（`/etc/ttys` 一行 console + `/etc/rc` 拉起 sh），让链路通；A-6 的完整形态留 P1。

---

## 2. P1 — 目标必需，但存在更短路径

#### H-P1-1（NEW）三架构的 boot-shim 覆盖不对称

- **事实**：`os/boot-shim/src/` 有 `uefi_helpers.rs`（x86_64/aarch64 走 UEFI）与 `opensbi_helpers.rs`（riscv64）。`qemu-tests/run_all.sh` 的 aarch64/riscv64 段只构建 `hello-boot/test-memmap/.../test-rt-birth/test-shutdown` 一类**测试载体**，不构建 boot-shim 产物。
- **为什么是 P1**：H-P0-1 修好后，boot-shim 本身在 aarch64/riscv64 是否有对等的模块加载路径需要逐个核实。若缺失，三架构目标只剩 x86_64。
- **验收**：三架构各跑一次"boot-shim 产物 + 12 模块"的冒烟。

#### H-P1-2（NEW）`minix-sockdriver` 缺 `no_std`（网络面）

- **事实**：`os/libs/minix-sockdriver/src/lib.rs` 无 `no_std` 声明；`os/servers/vfs/src/lib.rs` 也无，且 VFS 是 boot 必需——所以 VFS 的 no_std 化会连带撞上它。
- **为什么是 P1**：只影响 18/19 篇的网络命令；但它是 H-P0-2（VFS freestanding）的前置之一，实际执行时要一起做。

#### H-P1-3（NEW）`cargo` feature 统一陷阱会污染 freestanding 构建

- **事实**：`os/Cargo.toml` 是单一 workspace，`kernel`/`boot-shim`/`qemu-tests/*` 与各服务器共享 feature 解析域。
- **为什么是 P1**：实践经验（`os/Cargo.toml` 的 C-21 注记与 `commands/sbin/init/Cargo.toml:19-28`）显示 `-p minix-arch` 与 boot-shim/测试内核共用一次 invocation 会因 feature 统一而取错形态。
- **修复方向**：在 xtask 或 CI 脚本里把 freestanding 构建拆成独立 invocation，先于 H-P0-2 落地，否则会反复踩。

#### H-P1-4（NEW）18-stage-commands 的命令覆盖：66 已有 vs 计划表全量

- **事实**（按 `18-stage-commands/plan.md` §5.2 归属表核对现有 65 个 bin）：
  - **06 篇 fileops 已有**：basename、dirname、echo、expr、false、pathchk、printf、true。**缺**：cat、chmod、cp、df、ln、ls、mkdir、mv、pwd、rm、rmdir、sync、test、chown、du、find、mkfifo、mktemp、touch、xargs、stat、xinstall、chroot、link、unlink、truncate。
  - **05 篇 shell 已有**：无。**缺**：sh、ksh、csh、hostname、domainname、env、getopt、machine、pagesize、printenv、uname、sysenv。
  - **12 篇 proctools 已有**：kill、who。**缺**：date、ps、sleep、finger、from、ipcrm、ipcs、last、leave、lock、logger、logname、mesg、nice、nohup、renice、time、tty、users、w、wall、write。
  - **14 篇 mountinfo 已有**：fsck_mfs。**缺**：fsck、fsck_ext2fs、mount、umount。
  - **15 篇 diskfmt 已有**：mkfs_mfs。**缺**：newfs_ext2fs、newfs_msdos、newfs_udf、newfs_v7fs、makefs、fdisk、format、part、partition、repartition、autopart、devsize。
  - **10 篇 doctools 已有**：cal、whatis、apropos。**缺**：man、whereis、fmt、nl、pr（pr 已在 textfilter）、colcrt、ctags、deroff、indent、m4、cawf、prep、spell 等排版族（`todo.md:199` 已声明为留白面）。
  - **11 篇 compress 已有**：compress。**缺**：pax、bzip2、gzip、shar、unzip、uudecode、uuencode、bdes。
  - **13 篇 termctl 已有**：stty。**缺**：infocmp、tic、tput、loadfont、loadkeys、screendump、term、termcap、tget。
  - **01 篇 init 已有**：minix-init。**缺**：rcorder、reboot、shutdown、setup。
  - **03 篇 login 已有**：无。**缺**：login、passwd、su、chpass、id、nologin、pwhash、newgrp、pwd_mkdb、user、vipw。
  - **16/17 篇**：dd、vnconfig、diskimg 面。
  - **18/19 篇（网络）**：ifconfig、ping、route、netstat、arp、ftp、telnet、inetd、syslogd 等全部缺（依赖 17-stage-net）。
- **为什么是 P1**：目标是"运行所有程序"，但没必要等全部 328 个命令写完才启动。**最短路径**：先让 `sh` + `ls`/`cat`/`echo` + `init` 跑起来（约 5 个 bin），再按 §6 的批次顺序长尾补齐。
- **注意**：C 侧 `minix3/minix/commands/` 有 83 个目录，`comm -23` 得出 C 有而 Rust 无的约 75 项（含 `mount`、`umount`、`update`、`fdisk`、`part`、`setup`、`at`、`cron` 等）。这是"全量"口径的差距。

#### H-P1-5 `minix-sys` 顶层 API 面：已有 14 个基础函数 + open/stat/getdents/ioctl/fcntl/mmap，但仍有缺口

- **事实**（`os/libs/minix-sys/src/lib.rs` 实测）：顶层有 `send/receive/sendrec/notify`(`:123-147`)、`fork/exec/exit/sigreturn/waitpid/kill`(`:163-213`)、`open/close/read/write`(`:229-305`)、`stat/lstat/fstat`(`:318-339`)、`ioctl/fcntl/getdents/mmap`(`:347-379`)、`tcgetattr/tcsetattr`(`:250-287`)。
- **仍缺**（对照 `14-stage-runtime/todo.md:60` 已登记的 `stat`/`getdents`/`ioctl`/`fcntl` 一族——其中部分现已落地，需重新对账）：`dup2`、`pipe`/`pipe2`、`getpid`/`getppid`、`gettimeofday`、`uname`、`link`/`unlink`/`rename`、`mkdir`/`rmdir`、`chdir`/`getcwd`、`chmod`/`chown`、`umask`、`access`、`execve`（顶层只有 `exec`，需 `PreparedExec` 由 minix-rt 准备）。
- **为什么是 P1**：`sh` 的执行器要 `pipe2`+`fork`+`execve`+`waitpid`；`ls`/`cat`/`cp` 要 `open`+`getdents`/`read`+`stat`。命令接线每批都会撞到。

#### H-P1-6（NEW）`test-sysboot`/`test-rt-birth` 是"载体"而非生产路径，无全系统启动载体

- **事实**：`os/qemu-tests/run_all.sh` 列出的 15 个 x86_64 载体 + 若干 aarch64/riscv64 载体，每个都自带模块表/向量/用户镜像。**没有任何一个**是"kernel + VM + RS + PM + VFS + DS + MIB + TTY + MEM + MFS + INIT 一起起来"的载体。`os/qemu-tests/` 下无 `test-fullboot` 一类脚本。
- **为什么是 P1**：这是最终验收面。等 W1-W4 打通后必须有它，否则"启动了"无法证明。
- **验收**：三架构各一个 `test-fullboot-<arch>.sh`，断言 = init 起来 → sh 起来 → 跑通 N 个命令并比对输出。

---

## 3. P2 — 第二优先级（非 rewrite 的 translate 漂移、代码缺陷、卫生）

> 本节按用户要求"也可以一并扫描出，但这是第二优先级"。全部已带锚点，可独立修复，不阻塞 W1-W5。

#### H-P2-1 boot-shim 静默跳过缺失模块（`os/boot-shim/src/loader.rs:259-264`）

缺失模块被跳过而不报错，把失败推迟到内核 assert。改为 fail-loud。同 H-P0-1 一起修。

#### H-P2-2 `loader.rs:38-53` 的注释与内核实现对不上（文档说谎）

注释声称"内核的 `proc_init` 按 SYSTEM 旗标重排、盘上顺序与 IPC 顺序解耦"，但 `kernel/src/lib.rs:1188` 是纯位置映射 `BOOT_MODULE_PROC_NRS[i]`，没有任何重排。属模式"文档描述了不存在的机制"。修 H-P0-1 时一并删。

#### H-P2-3 53 个驱动 `main.rs` 是 `loop {}` 空转桩

`os/drivers/storage/memory/src/main.rs`、`virtio_blk`、`virtio_net`、全部 net/audio/bus/power/sensors/usb 等，7 行 `init(); loop {}`。这不只是"没实现"——它**假装是一个进程**（占位 bin 会被装进镜像并占一个 PID），属于"说谎的二进制"。与 `18-stage-commands/todo.md:144` 记录的 31 个命令占位壳同类（那批已删）。建议：要么补 `DriverRuntime::serve` 模板，要么从 workspace 成员表移除，不留中间态。

#### H-P2-4 `os/drivers/tty/tty/src/main.rs:16-18` 的 `Endpoint::NONE` 自端点

注释说"直到 RS 在真实启动时分配"。这是诚实的占位，但它使驱动无法自报身份（`ds` 查询、`getepinfo` 都会拿到 NONE）。修 H-P0-3 时一并接上。

#### H-P2-5 PM `main.rs` 缺 `VFS_PM_INIT` 同步

`os/servers/pm/src/main.rs:8-28` 只做 `BootParams::acquire_from` + `server.init()` + `server.run()`，注释承认"无 RS_INIT 握手"。C 的 `pm/main.c:220-236` 在 `sef_cb_init_fresh` 里给 VFS 发 `VFS_PM_INIT` 做进程表交换。Rust 侧 `init.rs:372`/`:662` 有"第 6 步：VFS_PM_INIT 进程表同步"的登记但 `main.rs` 没调。VFS 侧接收半是有的（`main_loop.rs:430` `process_vfs_pm_init`、`:7088`）。**两半都有、没接。**

#### H-P2-6 `ExecError::Perm` 被 `do_exec` 与 `do_newexec` 共用，掩盖了两处语义差异

`os/servers/pm/src/exec.rs:55` `Self::Perm => EPERM` 是单一映射。C 里 `do_exec` 根本没有 Perm、`do_newexec` 才有。修 H-P0-16 后建议给两处各自的门起不同名字（如 `ExecError::Perm` 只留 `do_newexec`），避免下次又抄错。

#### H-P2-7 `perform_syscall` 的 Err 通道已修，但 `Ok(m_type)` 通道仍可能吞错误

`os/libs/minix-sys/src/syscall.rs:96-112`：`Err(status) => Err(Errno::from_i32(status.0))`（正 errno）已正确处理 TrapStatus 符号；但 `message.m_type < 0` 之外的路径一律 `Ok(m_type)`。若真实内核在成功时把结果放在别处（如 RAX），这里会返回垃圾 m_type 当成功值。真机通电时需实测确认（E-SYSCALL-SIGN 的余件）。

#### H-P2-8 `minix-sef` 的 `PingInvalid` 变体无人构造

`os/libs/minix-sef/src/lib.rs:75` 定义了 `SefEvent::PingInvalid`（C `sef.c:208-214` 的 break 路径），但 `sef_receive_status` 里没有构造点——`:142-160` 的 notify 分支只处理 SYSTEM 与 RS+ping，其它 notify 直接落到 `:167` 当普通消息。与 H-P0-5 同批修。

#### H-P2-9 `os/servers/rs/src/boot.rs:522-526` 的 `grant_read`/`grant_revoke` 生产实现是空桩

`fn grant_read(...) -> Result<GrantId, Errno>` 与 `fn grant_revoke(...)` 都是默认空体（在 testutil 里）。与 H-P0-8 一起修，但即使 H-P0-8 走了另一条路（新 KernelApi 动词），这两个空桩也应补上，否则是"声明了能力但没实现"。

#### H-P2-10 `os/servers/vfs/src/main.rs` 全文 7 行，无 SEF 启动面

`fn main() { main_loop::run(); }`。C 的 `vfs/main.c:387` 是 `sef_startup()`。对照 `rs/main.rs`（69 行，有 `BootTables::acquire_from` + `server.init(SefInitType::Fresh)`）与 `ds/main.rs`（37 行，有 `SysIpc::new(DirectTrapTransport)`）。VFS 的入口是三个服务器里最薄的，与 H-P0-6 一起补。

#### H-P2-11 命令层的 `minix_types` 直引守卫已全绿，但 init 是唯一带 target 分叉的 crate

`tools/check-command-boundary.sh` 存在且 `todo.md:222` 记录全绿。风险是：H-P0-17 给 66 个 crate 加 target 分叉时，很容易顺手引入 `minix-types` 直依赖（就像 `os/commands/sbin/init/Cargo.toml` 曾为 `EEXIST` 引入的那样）。每批接线后重跑该守卫。

#### H-P2-12 `os/servers/is/src/acquire.rs` 大面积 `-EIO` / `ENOSYS` 测试断言

`acquire.rs:1019/1081-1082/1187/1322-1338/1376-1378/1507-1511` 等处把"宿主构建下 transport 诚实返回 -EIO"**断言成期望行为**。这些测试本身是对的（fail-closed 不伪造成功），但它们固化了"永不通电"的现状。真机通电后这批测试会集体变红——不是 bug，是信号。建议在 H-P0-17 通电批次里预留"翻转这批断言"的动作，别到时误判为回归。

---

## 4. 已具备（不需要重新建，只缺接线）

记录这一节是为了避免重复劳动——本次扫描确认以下**决策半**已经就绪，缺的是把两半接起来：

| 能力 | 位置 | 状态 |
|---|---|---|
| RS 三步启动状态机 + step 3 排空 | `os/servers/rs/src/boot.rs:931-1138` | 决策全备，等应答方（H-P0-6/7）与 grant（H-P0-8） |
| RS_INIT 应答的三处样板 | sched `transport.rs:173-188`、devman `minix.rs:278-295`、fs-rt `transport.rs:139-167` | 可直接复制到 PM/VFS/DS/MIB |
| `rprocpub` / `rproc` wire | `minix-types::ipc`（edge E-RSWIRE 已闭环） | 就位 |
| VFS 挂载决策件 | `os/servers/vfs/src/mount.rs:430-453`、`vmnt.rs:266` | 就位，等编排（H-P0-11） |
| `req_readsuper` 往返件 | `main_loop.rs:531-533` 注释记录 S14 已备 | 就位 |
| VFS_PM_INIT 两半 | PM `init.rs:372/662`、VFS `main_loop.rs:430/7088` | 两半都在，没接（H-P2-5） |
| minix-rt 出生链 | `libs/minix-rt/src/crt0.rs` + `real-trap` feature | 真机跑通过（rt-birth，edge E1 记录） |
| minix-sys 顶层 API | `libs/minix-sys/src/lib.rs:123-418` | 14 基础 + open/stat/getdents/ioctl/fcntl/mmap |
| 命令决定半（约 700 测试） | 24 个域 crate | 就位，缺执行半的 freestanding 形态（H-P0-17） |
| 三架构 QEMU 编排 | `qemu-tests/run_all.sh` + `run_qemu.sh` | 就位，可复用 |
| 内核 boot 到用户态（int-33 腿） | `kernel/src/lib.rs:1112-1300`、`switch_to_user` `:3097` | x86_64 真机验证过 |
| 内核 ELF 加载器 | `load_vm_elf`（arch 门面） | 真机验证过（rt-birth） |

---

## 5. 建议执行序（依赖波次）

> 每条走 todo-fix 三段式（讲明白 → 多方案对比 Linux/Redox/OS 理论 → 实施）+ fix-guard + 文档-代码同步 + 测试 5 维自查 + 回归 review。

| 波次 | 条目 | 产出 | 出口条件 |
|---|---|---|---|
| **A. 内核正确性快修（半天级，独立可合）** | H-P0-16（PM do_exec 门，含两个测试一起改）、H-P2-1、H-P2-2、H-P2-6 | 4 个小 commit | `cargo test -p minix-pm` 绿 + 新测试断言"任意调用者可转发" |
| **B. 可构建面** | H-P1-3（构建拆分）→ H-P0-2（PM/VFS no_std + 11 个 freestanding main）→ H-P0-17（66 个命令 target 分叉） | freestanding ELF 产出能力 | `cargo check --target x86_64-unknown-none` 对 11 服务器 + 66 命令全清 |
| **C. 出生握手** | H-P0-5（minix-sef Init 面）→ H-P0-6（PM/VFS/DS/MIB 应答）→ H-P0-8（rproctab grant）→ H-P0-7（VM 回应答） | RS step 3 可排空 | 宿主模拟下 RS step 3 对 12 个 boot 成员排空 |
| **D. 镜像与启动** | H-P0-1（boot-shim 12 模块）→ H-P0-4（xtask image/qemu）→ H-P0-3（MEM/TTY/MFS 真入口） | 能组装并启动的真实镜像 | QEMU 启动到 RS step 3 之后 |
| **E. 根与控制台** | H-P0-13（memory 驱动）→ H-P0-12（MFS 块源）→ H-P0-11（do_init_root 真挂载）→ H-P0-14（tty console 后端） | 根可读、输出可见 | init 起来并能 `write` 到串口 |
| **F. 三架构** | H-P0-9（aarch64/riscv64 陷阱腿）→ H-P0-10（x86_64 异常腿 + fast-syscall 返回）→ H-P1-1（三架构 boot-shim） | 三架构同链路 | 三架构各一个 `test-user-trap` 等价物 |
| **G. 命令面** | H-P0-15（sh）→ H-P0-18（最小 /etc）→ H-P1-5（minix-sys 缺口 API）→ H-P1-4 长尾批次 | sh 可交互、命令可跑 | H-P1-6 的全系统载体：init → sh → N 个命令 |
| **H. 长尾** | H-P1-4 剩余批次、P2 全节 | 覆盖度收敛 | `18-stage-commands/plan.md` §5.2 逐行对账 |

**波内依赖**：B 必须在 D 之前（没有 ELF 就没有镜像内容）；C 与 D 可并行推进但 C 的验收依赖 D 的载体；E 强依赖 D；F 与 E 可并行（不同架构/不同层）；G 依赖 E + F。

---

## 6. 本次扫描的实证命令清单（便于复核）

```bash
# 模块契约不一致
grep -n "MODULE_NAMES" os/boot-shim/src/loader.rs
grep -n "NR_BOOT_MODULES\|BOOT_MODULE_PROC_NRS" os/kernel/src/proc.rs
sed -n '1129,1195p' os/kernel/src/lib.rs

# freestanding 面
grep -rln 'target_os = "none"' --include=Cargo.toml os/
for d in os/servers/*/; do grep -q no_std $d/src/lib.rs || echo "NO no_std: $d"; done
for d in os/fs/*/ os/libs/*/; do grep -q no_std $d/src/lib.rs 2>/dev/null || echo "NO no_std: $d"; done

# 出生握手
grep -rn "RS_INIT" --include=*.rs os/servers/ os/fs/ | grep -v "^\S*:\s*//"
sed -n '130,170p' os/libs/minix-sef/src/lib.rs          # 无 Init 构造
sed -n '1314,1340p' os/servers/vm/src/vm_server.rs      # Suspend / NoReply
sed -n '940,960p;1030,1140p' os/servers/rs/src/boot.rs  # gid=None；step3 排空

# 根文件系统
sed -n '505,540p' os/servers/vfs/src/main_loop.rs
sed -n '15,32p' os/fs/fs-rt/src/source.rs               # PendingBlockSource EIO
cat os/drivers/storage/memory/src/main.rs               # 7 行桩

# 三架构
grep -n "wfi\|wfe\|exc_bad_mode" os/arch/src/riscv64/trap_entry.rs os/arch/src/arm64/trap_entry.rs
sed -n '236,352p' os/arch/src/lib.rs
sed -n '3008,3035p' os/kernel/src/lib.rs                # FastSyscall panic

# 命令面
find os/commands -path "*/src/bin/*" -name "*.rs" | wc -l          # 65 (+minix-init = 66)
ls os/commands/bin/shell/src/bin 2>/dev/null || echo "shell: no bin"  # 无 sh
grep -rn "PM_EXEC" minix3/minix/lib/libc/sys/execve.c              # 用户直发
sed -n '36,72p' minix3/minix/servers/pm/exec.c                     # do_exec 无门
sed -n '100,120p' os/servers/pm/src/exec.rs                        # do_exec 有门
sed -n '331,352p' os/servers/pm/src/exec.rs                        # 两个把偏离固化的测试

# xtask
sed -n '120,141p' os/xtask/src/main.rs                             # 两处 [skeleton]
ls os/etc/                                                          # 只有 README
```

---

## 7. 未决问题（OQ，上交用户裁决，不擅自决定）

- **OQ-HY4-1**：boot image 的 12 槽里，`MEM`（ramdisk）与 `MFS` 的关系——C 是 `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, ...)`，即 memory 驱动提供块设备、MFS 提供文件系统。本仓 `memory` 驱动全无入口、`mfs` 块源恒 EIO。**是先做 memory 驱动真入口，还是先给 mfs 接一个临时的内存块源把链路打通？** 倾向后者（更短路径），但它会在 `fs-rt` 里留下一个非 C 形状的通道，需标 `[ARCH]`。
- **OQ-HY4-2**：`/etc` 形态（H-P0-18）。`18-stage-commands/todo.md` 的 C-6 已把 A-6 上交为 OQ 且至今未决。**本次建议先做最小静态数据面**（不等 A-6），需用户确认这个"先有后好"的取舍。
- **OQ-HY4-3**：命令覆盖的验收口径。目标是"运行 18-stage-commands 的所有程序"——是 `plan.md` §5.2 归属表的全量（约 328 项），还是 C `minix3/minix/commands/` 的 83 个目录，还是现有的 66 个已建 bin？**口径不同，工作量差一个量级。** 建议以"现有 66 个 bin 先在真机跑通"为第一验收门，全量覆盖作为长尾。
