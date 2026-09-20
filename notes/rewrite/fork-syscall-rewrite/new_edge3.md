# new_edge3 — 服务器 · FS · net · 命令线（新一轮三线并行之三）

> **定位**：新一轮临时分工索引之三（前轮 [edge3.md](edge3.md) 已于 2026-09-20 冻结；S42 批一二三五 ✅、批四半程携带；S30-S37 开放项携带）。条目权威描述在 [edge_todo.md](edge_todo.md)（2026-09-20 节）与各 stage todo。归档规则见 [new_edge4.md](new_edge4.md) §8。
>
> **所有权（本线可独占修改）**：`os/servers/`（rs/vm/pm/vfs/ds/is/mib/devman/input/ipc-server/sched）、`os/fs/`（mfs/pfs/ext2/.../fs-rt）、`os/net/`、`os/commands/`、`os/etc/`、`os/xtask/`、`notes/rewrite/fork-syscall-rewrite/` 的 02/03/04/05/09/15/17/18-stage 文档。`os/Cargo.toml`、`os/qemu-tests/run_all.sh`、`edge_todo.md` 等共享文件触碰走 new_edge4 §2 认领板。
>
> **并发规则**：见 [new_edge4.md](new_edge4.md) §1。FIXLOG：`.review/zcode/edge3/FIXLOG.md`（2026-09-20 已归档重组为 263 行 + FIXLOG_archive.md；**只写增量**，编号从 #122 续，追加前跑守卫自检）。
> **领取规则**：开工任何条目前先 `tools/claim.sh claim <ID> <owner>` 领取——分支 `claim/<ID>-<owner>` 即排他锁（同名/同 ID 已存在即被领走，`list` 看全量），claim 同时自动建 `.wt/<id>-<owner>/` 专属工作树并打印 `cd` 路径——**本会话只在那棵树内改码/构建/测试**。每次从本文件重选/新领条目，先重读 [new_edge4.md](new_edge4.md) §1（尤其规则 7 并发隔离）并跑一次 `tools/claim.sh verify` 确认位置。文件头规则是提示，分支才是锁，工作树才是壳：共享主树内禁 checkout/reset --force（C-35 事故判例：NK5 会话主树换分支销毁了 C-28/NL6 两组在制品）；new_edgeX.md 的修改改完即 commit，勿留未跟踪状态。完成后合入主线再 `release` 销账（自动删树）；状态列同步标 🔄。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## NS 组条目（T2 通电链 → T3 init → T4 命令，按依赖排序）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| NS1 | E-BIRTHFACE 服务器余量：RS_INIT 应答臂 ×6 | [edge_todo.md](edge_todo.md) E-BIRTHFACE | PM/VFS/DS/MIB/IS/ipc-server 补 RS_INIT 应答（库根 SefEvent::Init 由 new_edge2 NL2 供给）；**VM 半**：消费后回 Suspend 不回 RS_INIT+result（vm_server.rs:1317-1339，C 对位 vm/main.c:249-255/:369-370）。同批 X-5（PM 缺 VFS_PM_INIT 同步发送半，main.rs:8-28）+ X-6（rs/main.rs:67 返回值丢弃、:48-67 过时注释） | new_edge2 NL2 | ✅ 2026-09-20（zcode_glm_4，74414dd1e）：六服务器出生应答臂（PM/VFS/DS/MIB/IS/ipc-server，按各自 C 注册面定 result 经 `sef_init_reply` 回 RS，C process_init 尾部 sef_init.c:113-117）+ VM 应答半（rs_handshake 成功分支应答后再 Suspend——C 的 SUSPEND 只压第二回复）；X-6 修毕（`let _ = server.run()` 吞 Err → panic + :48-55 注释按 TrapKernelApi 现状改写）；X-5 复核为过期误报（模式 70：VFS_PM_INIT 发送半在位 pm init.rs `vfs_init_sync`）。docker 8 crate 2303 全绿；clippy 59→59 零新增。**登记**：各服务器 LU 臂 ENOSYS 诚实拒绝（C VFS 有 sef_cb_init_lu 注册，Rust LU 机制挂通电）；DS fresh 锚点 ≡ 构造态，shadow 半（rproctab grant 消费）挂 E-RPROCTAB |
| NS2 | E-RPROCTAB：rproctab 授权创建点 + VM 消费翻转 | [edge_todo.md](edge_todo.md) E-RPROCTAB | RS boot step0 补 `rproctab_gid = grant_direct(...)`（boot.rs:951-954 DEFERRED 自认；C main.c:185）；`grant_read`/`grant_revoke` 空桩（:522-526）实化；VM 侧 safecopyfrom 失败分支补回复（vm_server.rs:1436-1443）。动工前核对 E-MIBGRANT 门语义 | NS1（boot 链走到 step0 有意义） | ✅ 2026-09-21（zcode_glm_4，代码 5917f2d51 / merge 2ed5278e6）：①RS 建权链——`RProcTable` 增 `pub_wire` 镜像（`sync_pub_wire`/`pub_wire_bytes`，64×40B `RprocpubSnap` 行 = C 静态数组 `rprocpub[]` 的替身，doc 02 §3.1.1）+ step0 表重置后经 `grant_read` 建 `READ|ANY` 授权（Rust 授权钉镜像地址故重置先行，行内容 step1 `sync_pub_wire` 填充）+ step1 注册循环后同步；②VM 失败应答臂——Err 分支先回 `sef_init_reply(e.to_errno())`（C process_init **无条件**应答 sef_init.c:110-119，VM asynsend 变体 main.c:229；失败经 RS 既有 panic 臂 boot.rs:1163-1166 可观测收尾），后保留 [A-14] 丢弃+计数+审计（main.c:151 panic 偏差不翻案）；③**E-MIBGRANT 门核对实产出**：内核 grant.rs/syscall_copy.rs 端点哨兵 `SELF/NONE/ANY`=-2/-1/-3 与权威不符（endpoint.h:51-56+com.h:55 → 31742/31743/31744），who_to=ANY 授权被 verify_grant grantee 门 EPERM 拒——改由 `minix_types::Endpoint::*` 派生（C-41 跨界，new_edge4 §2 销账）。**过期前提更正**：grant_read/revoke 传输半 S18 已真（trap_api.rs:569-590），NS2 行"空桩实化"未动 UnimplementedKernelApi（fail-closed 设计）。docker kernel 788/rs 343/vm 522 全绿；clippy 120/34/217 与基线零新增。FIXLOG Fix #130。**登记**：clock 面 `syscall_clock.rs:69 SELF=-2` 同族实断（ipc-server/is 传权威值）→ new_edge1 NK8，未顺手修 |
| NS3 | 【bug】PM do_exec 门错位修复 | [edge_todo.md](edge_todo.md) E-EXECFILE ｜ [04-stage-pm/todo.md](04-stage-pm/todo.md) 2026-09-20 节 | 删 do_exec 的 VFS|RS 门（exec.rs:100-116；C do_exec exec.c:38-56 无门），门保留在 ExecNew/ExecRestart 臂；翻转固化偏离的两个测试（exec.rs:331-352）；`ExecError::Perm` 语义分道（H-20）。**T3 第一阻塞 bug** | 无（todo-fix 三段式） | ✅ 2026-09-20 abb1f2e8b |
| NS4 | VFS 根挂载编排（18-mount W 批提级） | [05-stage-vfs/todo.md](05-stage-vfs/todo.md) ｜ [edge_todo.md](edge_todo.md) E-MEMDRV/E-FSBDEV | do_init_root（main_loop.rs:517-540）真发 req_readsuper → mfs on DEV_IMGRD；root_fs_e 赋值面（mount.c:326-328 对位）；dmap/装配收尾 | new_edge2 NL6（memory 驱动）+ NS6（块源） | 🔄 2026-09-21 zcode_glm_3 |
| NS5 | VFS exec worker | [edge_todo.md](edge_todo.md) E-EXECFILE ｜ [05-stage-vfs/todo.md](05-stage-vfs/todo.md) 2026-09-20 节 | ipc/dispatcher.rs:200-215 桩 → 真 worker：open + read_header（#! ESCRIPT 分支可后置）+ ELF 段装载 + PM_EXEC_NEW 载荷（wire 已备 0x906/0x986；VM→kernel 半已真）。C 对位 vfs/exec.c | NS4（有根才能 open） | ☐ |
| NS6 | mfs 块源装配：imgrd 内存盘 BlockSource | [edge_todo.md](edge_todo.md) E-FSBDEV 最小半 ｜ [15-stage-fs/todo.md](15-stage-fs/todo.md) 2026-09-20 节 | fs-rt/source.rs PendingBlockSource（恒 EIO）→ imgrd 内存盘实现（boot 传入基址/尺寸；对端 new_edge2 NL6）；BdevBlockSource 已在 minix-fs（bdev_bridge.rs:44-165）缺 mfs bin 消费切换 | new_edge2 NL6 | ✅ 2026-09-21（zcode_glm_2，cb47de65d）：fs-rt 新增 `ImgrdBlockSource`（镜像字节内存盘：几何=镜像本身 memory.c:148-150 对位、越界裁剪+整块越界读零 memory.c:442-477 对位；bdev IPC 生产 transport 未通电，进程内直供为 E-FSBDEV 最小半的登记适配，通道落地后切 BdevBlockSource）+ `BootBlockSource` 出生期二选一；mfs bin 消费切换（`BOOT_IMGRD` 静态装机槽——NS8 镜像装配填，空槽保持 Pending EIO 臂不假装有盘）。docker `-j1`：fs-rt 22 绿（+10 测试）/ mfs 177 绿（+1 mkfs 镜像经 imgrd 源挂载读根集成测试），clippy 20→20 零新增。**宿主半已验；QEMU 真机半挂 T2 boot 链**（装机槽填入=borrow NS8） |
| NS7 | E-CONSOLE VFS 半 | [edge_todo.md](edge_todo.md) E-CONSOLE ｜ [05-stage-vfs/todo.md](05-stage-vfs/todo.md) 2026-09-20 节 | 字符设备写臂 Nosys → cdev 路由（syscalls.rs:306-307/:418-419）；dmap 补 tty 槽（device_map.rs:137-146；DS 事件填充机制已有）。驱动半归 new_edge2 NL1 | new_edge2 NL1/NL6 | ☐ |
| NS8 | E-IMGPKG：镜像装机面 | [edge_todo.md](edge_todo.md) E-IMGPKG ｜ S32 ｜ [18-stage-commands/todo.md](18-stage-commands/todo.md) 2026-09-20 节 | xtask `image()`/`qemu()` 实装（build() 扩全量 + default-features=false 生产开关 + 三架构布局）；12 模块装机清单（对位 OQ-N2 裁决结果；缺件不再静默跳过，loader.rs:259-264）；/etc 最小内容（rc + /dev/console 节点 + TTYS；OQ-3 盘上文件面裁决，不等 A-6）；mkfs_mfs proto 填充做装机消费；占位 bin 排除（X-11） | NK4（OQ-N2）+ NL6 | ✅ 2026-09-21（zcode_glm_1，dfb910f37）：xtask image()/qemu() 实装（装配计划/执行分离，dry-run 可打印）+ 装机清单三重锁（计数绑 NR_BOOT_MODULES/顺序对 boot-shim MODULE_NAMES 源文本/包目录守卫；X-11 白名单语义）+ /etc 最小集（rc + ttys + console 节点经原型；**OQ-3 口径草案待用户过目**）+ mkfs_mfs 装机消费（真实播种 EXIT=0，2048×4KiB）+ ESP 组装（mtools）+ build() 扩全量。x86_64 全链；aarch64/riscv64 honest bail（产出链缺口注明）。**登记见本轮新登记 NS8-A（kernel.elf 生产者缺口，edge1 面）与 NS8-B（模块 guest 构建首阻塞点，NL5②/NS12 波次）**；真机启动验证随 NK4-A 消费（QEMU 串行纪律未占） |
| NS9 | RS read_exec 生产化 | S42 批五登记 ｜ [03-stage-rs/todo.md](03-stage-rs/todo.md) | exec.rs:21-31 noop 注入 → 真读盘（C manager.c:1372-1420）；service up / 崩溃重启自持 | NS4+NS5（VFS 通电） | ⏸ NS4/NS5 |
| NS10 | PM 批次 E/G + Reboot 臂 | [04-stage-pm/todo.md](04-stage-pm/todo.md) §11.1.1 ｜ [edge_todo.md](edge_todo.md) E-EXECFILE | 批次 E（exec wire 装配，与 NS5 对接）/G（misc）；`PmCall::Reboot` 臂（catch-all ENOSYS → 最小 sys_abort 等价；init minixreboot 依赖） | NS3/NS5 | ☐ |
| NS11 | init 残余面 | [09-stage-init/todo.md](09-stage-init/todo.md) 2026-09-20 节 | main 签名 `()` → `-> i32`（main.rs:64，crt0 契约；exit code 读垃圾修复）；securitylevel（host.rs:288）/init_root（:292，随 NS4 解锁）；trampoline 填真（host.rs:296-305，等 new_edge2 NL3③） | NL3③、NS4 | ☐ |
| NS12 | 命令 freestanding 批量 + T4 判定集 | [18-stage-commands/todo.md](18-stage-commands/todo.md) ｜ [edge_todo.md](edge_todo.md) E-SVCFREE/E-EXECFILE | 65 bin 批量切 no_std 目标（echo 双 seam 模板；riscv64gc/aarch64-unknown-none 矩阵）；**sh/cat/ls 接线**（sh 前置 pipe2 = new_edge2 NL4 + NS5）；18-stage 冒烟脚本（QEMU 内执行断言输出） | NL4/NL5、NS5、NS7、NS8 | ☐ |
| NS13 | X-10：06-file-ops.md:182 漂移修正 | [edge_todo.md](edge_todo.md) 缺陷批 X-10 | cat 阻塞描述改"guest 二进制形态 + exec 递送链" | 无 | ✅ 2026-09-20 |

## 携带的前轮开放项（状态照旧，详情见前轮 edge3.md 对应行）

| 编号 | 条目 | 前轮状态 | 本轮处置 |
|---|---|---|---|
| S30 | （前轮 🔄 项） | 🔄 | 本轮批次：F3c-2 写半后段 ✅ 2026-09-21 zcode_glm_2（98d6c6a68，合主线 2e095e154）——领取时核查：⏳ 主张（create/mkdir 等消费 alloc_inode）已由 17c88be0c 全额落地（模式 70 CTOS 过期账面）；真实剩余 rename tail 已补完（C fs_rename link.c:268-445 决策树对位：跨父 `..` 重指+双父链接账、覆写摘目标、祖先环 EINVAL、EMLINK、同名无操作、类型错配两拒、换名保留类型字节；测试 43→52 全绿，clippy 零告警，doc 22 §5.3 同步）。同批登记三项未修见 FIXLOG #131（link 类型字节/缺 EMLINK 门、doc 22 快速链接行过期）；S30 余项（procfs/ptyfs 缺口、F3 文档面）不动 |
| S31 | （⏸ S29 通用框架） | ⏸ | S29 若已 ✅ 则解锁，开工前复核 |
| S32 | 根镜像装机面 | ~~⏸~~ 已解锁 | 并入 NS8 执行 |
| S33 | E-MIBPROD/E-ISPROD 余项 | 🔄（余外部依赖） | 余项=run_dump A-6 输出面真机联调（E5(g)），挂 T2 后 |
| S35 | 18-stage C-1 长尾 | 🔄（等跨线原语） | 跨线原语即 NL4/NS5/NS7，解锁后按批推进 |
| S36 | Requires 回填 + POSIX 基准 | ☐ | 随 S35/NS12 各批同步 |
| S37 | E-DMABUF VM 侧 | 已解锁（L9 闭环） | 按 16-stage 驱动消费方节奏 |
| S42 批四 | 全系统载体 | 半程（C-27/C-29） | 载体收口随 new_edge1 NK1；NS1/NS2 供 step3 应答 |

## 已闭单勿领（以 edge_todo.md 最新进度注记为准）

S1-S29 ✅ 各行、S34、S38、S39、S42 批一/二/三/五；E1-E9、E-FSRUNTIME 装配半（V1-P0-1）、五路扫描确认的既有闭环（E-SYSCALL-SIGN 双腿、E-CMDSYSFACE、PM 信号/uid 臂——勿按 qwen P2 滞后主张重开）。

## 本轮新登记（NS8 交付时发现，归 edge4 收敛；复核命令绿 = 已闭可划掉）

| 标记 | 发现 | 现状证据 | 复核命令 | 归属 |
|---|---|---|---|---|
| NS8-A | 生产 kernel.elf 无产出者：minix-kernel 是纯 lib（`os/kernel/Cargo.toml` 无 `[[bin]]`），而 boot-shim 按契约读 `/EFI/minix/kernel.elf`（`os/boot-shim/src/loader.rs:31`）→ NS8 的 image() 按 `--kernel` 路径取件（缺省 `target/x86_64-unknown-none/release/kernel.elf`），缺件即 fail-fast，不代产。需 kernel bin + 链接脚本决策（与 C-29/NK1 地址空间设计相邻） | `grep -c "\[\[bin\]\]" os/kernel/Cargo.toml` = 0；`cargo run -p xtask image --dry-run` 计划含 kernel.elf 取件步 | 有产出者后 `cargo build -p <kernel-bin> --target x86_64-unknown-none` 出 ELF 且 `xtask image` 走通取件步 → 划掉 | **new_edge1**（os/kernel 所有权）**🔄 2026-09-21 zcode_glm_1（claim/NS8-A-zcode_glm_1）** |
| NS8-B | 12 模块 guest 目标（x86_64-unknown-none）构建首个阻塞点实测 = minix-ds 链接期 `#[panic_handler] function required`（deps 全过，bin 链接失败）——freestanding bin 需要 minix-rt `panic-handler`/`alloc-global` feature 或自备 handler 的接线决策 | `cargo build -p minix-ds --target x86_64-unknown-none`（宿主 ulimit）→ E0463 panic_handler | minix-ds guest 构建 Finished → 逐包推进至 12/12（feature 接线方案 = echo 双 seam 模板的推广）→ 划掉 | new_edge2 NL5② / new_edge3 NS12 |

## 本线在验收阶梯中的位置（全文见 [new_edge4.md](new_edge4.md) §7）

T2 = NS1/NS2/NS6/NS7（+NL1/NL2/NL6）；T3 = NS3/NS4/NS5/NS10/NS11 + NS8 的 /etc 半；T4 = NS8/NS12。
