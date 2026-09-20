# new_edge3 — 服务器 · FS · net · 命令线（新一轮三线并行之三）

> **定位**：新一轮临时分工索引之三（前轮 [edge3.md](edge3.md) 已于 2026-09-20 冻结；S42 批一二三五 ✅、批四半程携带；S30-S37 开放项携带）。条目权威描述在 [edge_todo.md](edge_todo.md)（2026-09-20 节）与各 stage todo。归档规则见 [new_edge4.md](new_edge4.md) §8。
>
> **所有权（本线可独占修改）**：`os/servers/`（rs/vm/pm/vfs/ds/is/mib/devman/input/ipc-server/sched）、`os/fs/`（mfs/pfs/ext2/.../fs-rt）、`os/net/`、`os/commands/`、`os/etc/`、`os/xtask/`、`notes/rewrite/fork-syscall-rewrite/` 的 02/03/04/05/09/15/17/18-stage 文档。`os/Cargo.toml`、`os/qemu-tests/run_all.sh`、`edge_todo.md` 等共享文件触碰走 new_edge4 §2 认领板。
>
> **并发规则**：见 [new_edge4.md](new_edge4.md) §1。FIXLOG：`.review/zcode/edge3/FIXLOG.md`（2026-09-20 已归档重组为 263 行 + FIXLOG_archive.md；**只写增量**，编号从 #122 续，追加前跑守卫自检）。
> **领取规则**：开工任何条目前先 `tools/claim.sh claim <ID> <owner>` 领取——分支 `claim/<ID>-<owner>` 即排他锁（同名/同 ID 已存在即被领走，`list` 看全量，完成后 `release` 销账）；状态列同步标 🔄。文件头规则是提示，分支才是锁。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## NS 组条目（T2 通电链 → T3 init → T4 命令，按依赖排序）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| NS1 | E-BIRTHFACE 服务器余量：RS_INIT 应答臂 ×6 | [edge_todo.md](edge_todo.md) E-BIRTHFACE | PM/VFS/DS/MIB/IS/ipc-server 补 RS_INIT 应答（库根 SefEvent::Init 由 new_edge2 NL2 供给）；**VM 半**：消费后回 Suspend 不回 RS_INIT+result（vm_server.rs:1317-1339，C 对位 vm/main.c:249-255/:369-370）。同批 X-5（PM 缺 VFS_PM_INIT 同步发送半，main.rs:8-28）+ X-6（rs/main.rs:67 返回值丢弃、:48-67 过时注释） | new_edge2 NL2 | ☐ |
| NS2 | E-RPROCTAB：rproctab 授权创建点 + VM 消费翻转 | [edge_todo.md](edge_todo.md) E-RPROCTAB | RS boot step0 补 `rproctab_gid = grant_direct(...)`（boot.rs:951-954 DEFERRED 自认；C main.c:185）；`grant_read`/`grant_revoke` 空桩（:522-526）实化；VM 侧 safecopyfrom 失败分支补回复（vm_server.rs:1436-1443）。动工前核对 E-MIBGRANT 门语义 | NS1（boot 链走到 step0 有意义） | ☐ |
| NS3 | 【bug】PM do_exec 门错位修复 | [edge_todo.md](edge_todo.md) E-EXECFILE ｜ [04-stage-pm/todo.md](04-stage-pm/todo.md) 2026-09-20 节 | 删 do_exec 的 VFS|RS 门（exec.rs:100-116；C do_exec exec.c:38-56 无门），门保留在 ExecNew/ExecRestart 臂；翻转固化偏离的两个测试（exec.rs:331-352）；`ExecError::Perm` 语义分道（H-20）。**T3 第一阻塞 bug** | 无（todo-fix 三段式） | 🔄 2026-09-20 qoder |
| NS4 | VFS 根挂载编排（18-mount W 批提级） | [05-stage-vfs/todo.md](05-stage-vfs/todo.md) ｜ [edge_todo.md](edge_todo.md) E-MEMDRV/E-FSBDEV | do_init_root（main_loop.rs:517-540）真发 req_readsuper → mfs on DEV_IMGRD；root_fs_e 赋值面（mount.c:326-328 对位）；dmap/装配收尾 | new_edge2 NL6（memory 驱动）+ NS6（块源） | ☐ |
| NS5 | VFS exec worker | [edge_todo.md](edge_todo.md) E-EXECFILE ｜ [05-stage-vfs/todo.md](05-stage-vfs/todo.md) 2026-09-20 节 | ipc/dispatcher.rs:200-215 桩 → 真 worker：open + read_header（#! ESCRIPT 分支可后置）+ ELF 段装载 + PM_EXEC_NEW 载荷（wire 已备 0x906/0x986；VM→kernel 半已真）。C 对位 vfs/exec.c | NS4（有根才能 open） | ☐ |
| NS6 | mfs 块源装配：imgrd 内存盘 BlockSource | [edge_todo.md](edge_todo.md) E-FSBDEV 最小半 ｜ [15-stage-fs/todo.md](15-stage-fs/todo.md) 2026-09-20 节 | fs-rt/source.rs PendingBlockSource（恒 EIO）→ imgrd 内存盘实现（boot 传入基址/尺寸；对端 new_edge2 NL6）；BdevBlockSource 已在 minix-fs（bdev_bridge.rs:44-165）缺 mfs bin 消费切换 | new_edge2 NL6 | ☐ |
| NS7 | E-CONSOLE VFS 半 | [edge_todo.md](edge_todo.md) E-CONSOLE ｜ [05-stage-vfs/todo.md](05-stage-vfs/todo.md) 2026-09-20 节 | 字符设备写臂 Nosys → cdev 路由（syscalls.rs:306-307/:418-419）；dmap 补 tty 槽（device_map.rs:137-146；DS 事件填充机制已有）。驱动半归 new_edge2 NL1 | new_edge2 NL1/NL6 | ☐ |
| NS8 | E-IMGPKG：镜像装机面 | [edge_todo.md](edge_todo.md) E-IMGPKG ｜ S32 ｜ [18-stage-commands/todo.md](18-stage-commands/todo.md) 2026-09-20 节 | xtask `image()`/`qemu()` 实装（build() 扩全量 + default-features=false 生产开关 + 三架构布局）；12 模块装机清单（对位 OQ-N2 裁决结果；缺件不再静默跳过，loader.rs:259-264）；/etc 最小内容（rc + /dev/console 节点 + TTYS；OQ-3 盘上文件面裁决，不等 A-6）；mkfs_mfs proto 填充做装机消费；占位 bin 排除（X-11） | NK4（OQ-N2）+ NL6 | ☐ |
| NS9 | RS read_exec 生产化 | S42 批五登记 ｜ [03-stage-rs/todo.md](03-stage-rs/todo.md) | exec.rs:21-31 noop 注入 → 真读盘（C manager.c:1372-1420）；service up / 崩溃重启自持 | NS4+NS5（VFS 通电） | ⏸ NS4/NS5 |
| NS10 | PM 批次 E/G + Reboot 臂 | [04-stage-pm/todo.md](04-stage-pm/todo.md) §11.1.1 ｜ [edge_todo.md](edge_todo.md) E-EXECFILE | 批次 E（exec wire 装配，与 NS5 对接）/G（misc）；`PmCall::Reboot` 臂（catch-all ENOSYS → 最小 sys_abort 等价；init minixreboot 依赖） | NS3/NS5 | ☐ |
| NS11 | init 残余面 | [09-stage-init/todo.md](09-stage-init/todo.md) 2026-09-20 节 | main 签名 `()` → `-> i32`（main.rs:64，crt0 契约；exit code 读垃圾修复）；securitylevel（host.rs:288）/init_root（:292，随 NS4 解锁）；trampoline 填真（host.rs:296-305，等 new_edge2 NL3③） | NL3③、NS4 | ☐ |
| NS12 | 命令 freestanding 批量 + T4 判定集 | [18-stage-commands/todo.md](18-stage-commands/todo.md) ｜ [edge_todo.md](edge_todo.md) E-SVCFREE/E-EXECFILE | 65 bin 批量切 no_std 目标（echo 双 seam 模板；riscv64gc/aarch64-unknown-none 矩阵）；**sh/cat/ls 接线**（sh 前置 pipe2 = new_edge2 NL4 + NS5）；18-stage 冒烟脚本（QEMU 内执行断言输出） | NL4/NL5、NS5、NS7、NS8 | ☐ |
| NS13 | X-10：06-file-ops.md:182 漂移修正 | [edge_todo.md](edge_todo.md) 缺陷批 X-10 | cat 阻塞描述改"guest 二进制形态 + exec 递送链" | 无 | ☐ |

## 携带的前轮开放项（状态照旧，详情见前轮 edge3.md 对应行）

| 编号 | 条目 | 前轮状态 | 本轮处置 |
|---|---|---|---|
| S30 | （前轮 🔄 项） | 🔄 | 继续按前轮要点执行 |
| S31 | （⏸ S29 通用框架） | ⏸ | S29 若已 ✅ 则解锁，开工前复核 |
| S32 | 根镜像装机面 | ~~⏸~~ 已解锁 | 并入 NS8 执行 |
| S33 | E-MIBPROD/E-ISPROD 余项 | 🔄（余外部依赖） | 余项=run_dump A-6 输出面真机联调（E5(g)），挂 T2 后 |
| S35 | 18-stage C-1 长尾 | 🔄（等跨线原语） | 跨线原语即 NL4/NS5/NS7，解锁后按批推进 |
| S36 | Requires 回填 + POSIX 基准 | ☐ | 随 S35/NS12 各批同步 |
| S37 | E-DMABUF VM 侧 | 已解锁（L9 闭环） | 按 16-stage 驱动消费方节奏 |
| S42 批四 | 全系统载体 | 半程（C-27/C-29） | 载体收口随 new_edge1 NK1；NS1/NS2 供 step3 应答 |

## 已闭单勿领（以 edge_todo.md 最新进度注记为准）

S1-S29 ✅ 各行、S34、S38、S39、S42 批一/二/三/五；E1-E9、E-FSRUNTIME 装配半（V1-P0-1）、五路扫描确认的既有闭环（E-SYSCALL-SIGN 双腿、E-CMDSYSFACE、PM 信号/uid 臂——勿按 qwen P2 滞后主张重开）。

## 本线在验收阶梯中的位置（全文见 [new_edge4.md](new_edge4.md) §7）

T2 = NS1/NS2/NS6/NS7（+NL1/NL2/NL6）；T3 = NS3/NS4/NS5/NS10/NS11 + NS8 的 /etc 半；T4 = NS8/NS12。
