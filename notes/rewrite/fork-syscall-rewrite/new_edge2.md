# new_edge2 — 共享库 · 用户态运行时 · 驱动框架线（新一轮三线并行之二）

> **定位**：新一轮临时分工索引之二（前轮 [edge2.md](edge2.md) 已于 2026-09-20 冻结，L1-L16 全 ✅、L17 携带）。条目权威描述在 [edge_todo.md](edge_todo.md)（2026-09-20 节）与 14/16-stage todo；本线是三条线的**依赖下游汇聚点**——new_edge3 的服务器与命令消费本线 API。归档规则见 [new_edge4.md](new_edge4.md) §8。
>
> **所有权（本线可独占修改）**：`os/libs/`（minix-types、minix-sys、minix-rt、minix-sef、minix-chardriver、minix-blockdriver、minix-netdriver、minix-bdev、minix-driver-rt、minix-fs、minix-vtreefs、minix-sffs、minix-sockdriver）、`os/drivers/`、`notes/rewrite/fork-syscall-rewrite/14-stage-runtime/`、`16-stage-drivers/`。
>
> **并发规则**：见 [new_edge4.md](new_edge4.md) §1。新增 API 纯增量优先；破坏性变更在 new_edge4 §2 公告。
> **领取规则**：开工任何条目前先 `tools/claim.sh claim <ID> <owner>` 领取——分支 `claim/<ID>-<owner>` 即排他锁（同名/同 ID 已存在即被领走，`list` 看全量），claim 同时自动建 `.wt/<id>-<owner>/` 专属工作树并打印 `cd` 路径——**本会话只在那棵树内改码/构建/测试**。每次从本文件重选/新领条目，先重读 [new_edge4.md](new_edge4.md) §1（尤其规则 7 并发隔离）并跑一次 `tools/claim.sh verify` 确认位置。文件头规则是提示，分支才是锁，工作树才是壳：共享主树内禁 checkout/reset --force（C-35 事故判例：NK5 会话主树换分支销毁了 C-28/NL6 两组在制品）；new_edgeX.md 的修改改完即 commit，勿留未跟踪状态。完成后合入主线再 `release` 销账（自动删树）；状态列同步标 🔄。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## NL 组条目

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| NL1 | L17 波次 1 携带：console 输出后端（批B） | [edge2.md](edge2.md) L17 ｜ [edge_todo.md](edge_todo.md) E-CONSOLE | 批A（tty 进程化 c94fc26d5）/批C（pckbd 794d3eb91 系）已落；批B 卡**输出通道选型**（串口直程需 port I/O 权限面 / video-text 需活 mem server / 系统任务中转）——裁决挂 OQ-N3，裁决后执行字节管道 + 行输出缓冲。VFS 半（cdev 臂/dmap 槽）归 new_edge3 NS7 | OQ-N3 | ⏸ OQ-N3 |
| NL2 | E-BIRTHFACE 库根：minix-sef 构造 SefEvent::Init | [edge_todo.md](edge_todo.md) E-BIRTHFACE | minix-sef 从不构造 Init（grep 零命中）——各服务器 Init 臂全是死代码。方案 A：sef_receive_status 补出生面（C sef.c:197/sef_init.c:115 对位）+ 共享应答助手；顺带 H-22 `SefEvent::PingInvalid` 无人构造（lib.rs:75）。一处修好，6+ 服务器翻活 | 无 | ✅ 2026-09-20 e31a86ab5（库根半：sef_receive_status 产 Init/PingInvalid + sef_init_reply 助手 + Message::rs_init_type；服务器应答臂翻活归 new_edge3 NS1）|
| NL3 | E-RTHEAP：minix-rt 三件 | [edge_todo.md](edge_todo.md) E-RTHEAP ｜ [14-stage-runtime/todo.md](14-stage-runtime/todo.md) 2026-09-20 节 | ①VM 页供应商（alloc.rs 静态 64KiB 池 → brk/mmap 通道，选型 OQ-N4）；②panic-exit-PM（lib.rs:279-308 自旋 → 经 minix_sys::exit/PM 终止）；③sigreturn trampoline 符号（C `__sigreturn` 对位；消费方 init host.rs:296-305 填 0 处） | ③ 消费臂在 new_edge3 NS11 | ✅ 2026-09-20（zcode_glm_3，72f1cf70f——②panic 尾 `minix_sys::exit(1)`（C panic.c:54 对位，A-8 三步全落）+ ③`signals::__sigreturn` naked 桩 x86_64 + 帧槽契约文档（C __sigreturn.S:7-10 对位；消费臂 NS11、PM 投递半按约接线）+ minix-rt 55 测试全绿/no_std 形态/clippy 零新增；rt-birth 注释 C-39。**①VM 页供应商未做：挂 OQ-N4 不代决，维持登记**；aarch64/riscv64 trampoline 腿随各自投递臂波次；真机半挂 T2（QEMU 串行未占） |
| NL4 | minix-sys 顶层 API 缺口批 | [14-stage-runtime/todo.md](14-stage-runtime/todo.md) 2026-09-20 节 ｜ [edge_todo.md](edge_todo.md) E-CMDSYSFACE 先例 | 清单（grep 零命中实证）：**pipe/pipe2（sh 硬前置，优先）**、文件操作族（mkdir/rmdir/chmod/chown/link/symlink/unlink/rename/mknod/truncate/ftruncate/utime/umask/mkfifo）、chdir/getcwd、access、isatty、getppid/times/pathconf、getpriority、sync、brk/sbrk、whoami、mount/umount、reboot/shutdown、socket 族（17-stage 域）。既有 module-only（dup2_via/lseek_via/getpid_via/nanosleep_via 等）顶层化。每件配 Canned 回放测试 | 无（逐件独立） | 🔄 2026-09-20 qorder_3（首件 pipe/pipe2 已在 claim/NL4-qorder_3 落地：`minix_sys::pipe/pipe2` + `pipe2_via` + wire 回放 3 测；余件逐件推进） |
| NL5 | E-SVCFREE 库半：freestanding 构建面 | [edge_todo.md](edge_todo.md) E-SVCFREE | ①minix-sockdriver 补 no_std（VFS 连库 29 错的根）；②minix-rt 三架构目标姿态批量验证（crt0 `_start` ×3 已备）；③feature 统一陷阱纪律落 xtask/CI（拆 invocation；kernel `default=["mock"]` umbrella 的生产开关显式化）；④crt0 `main()->i32` 契约文档化（init 签名失配的消费侧归 new_edge3 NS11） | 无 | ☐ |
| NL6 | E-MEMDRV：memory 驱动进程化 | [edge_todo.md](edge_todo.md) E-MEMDRV ｜ [16-stage-drivers/todo.md](16-stage-drivers/todo.md) 2026-09-20 节 | `os/drivers/storage/memory/src/main.rs` 7 行桩 → 进程壳（出生面照 S29/S25/S27 范式：minix-driver-rt serve + RS_INIT 应答——依赖 NL2）。库本体已真。同批：tty 自端点 NONE 消费 whoami wrapper（NL4 的 whoami 件 + RS 端点分配接线）。**boot priv 表 SRF_SYS_PROC 槽，T2 关键路径** | NL2 | ✅ 2026-09-20 qorder_2 c0a9b81aa（main.rs 进程壳 + service.rs 双面消息泵 + driver-rt 双 announce；memory 34 绿/rt 13 绿，tty33/pckbd45 回归绿，clippy 零新增）；SRF_SYS_PROC 槽已在位（memory 行 flags=SRV_F 含 SYS_PROC，table.rs:127-131）无需改；tty whoami wrapper 子项 ⏸ 依赖 NL4（qorder_3 持有，whoami 件全仓零命中未落地，不代做）|
| NL7 | X-1：边界守卫回归修复 | [edge_todo.md](edge_todo.md) 缺陷批 X-1 ｜ [18-stage-commands/todo.md](18-stage-commands/todo.md) C-7 | 40 violations 两类：termctl stty 测试代码 minix_types 直用（守卫豁免 `#[cfg(test)]` 的裁决挂 OQ-N5）；init host.rs/execve.rs、text-games/random.rs 直用 DirectTrapTransport（改走顶层封装，缺件登记 NL4） | OQ-N5 | ☐ |
| NL8 | X-9：static mut / unsafe 密度裁决 | [edge_todo.md](edge_todo.md) 缺陷批 X-9 | 81 处 static mut、21 文件 unsafe 密度超标；pm/ipc/decode.rs（31/461）单独裁决。code-excellence 窗口，不清就不阻塞 | 无 | 🚫（登记） |

## 携带的前轮遗留

- L1-L16 全 ✅（含 L14 BdevBlockSource、L15 二级缓存、L17 批A/批C）；"已闭单勿领"清单照旧。
- L17 波次 1 的批B 缺口已并入 NL1；存储/网络/USB 波次按消费方另立（NL6 是存储波次第一件）。

## 本线在验收阶梯中的位置（全文见 [new_edge4.md](new_edge4.md) §7）

T2 = NL1/NL2/NL6（出生面+console+memory 是 boot 关键链）；T3 = NL3（init 信号/堆）；T4 = NL4（命令 libc 面）；T5 = NL5。
