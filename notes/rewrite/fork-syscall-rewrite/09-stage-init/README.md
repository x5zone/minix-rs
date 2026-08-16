# 09-stage-init — INIT 文档目录

> **状态**: 骨架就绪（plan.md 定稿 2026-08-16；各 doc 为最小骨架，待按 plan.md 改写）
> **主线**: init 状态机启动顺序（single_user → runcom → read_ttys → multi_user 稳态 ↔ clean_ttys/catatonia/death）
> **Ground truth**: `minix3/sbin/init/`（init.c 1902 行）

## 文档清单（16 篇）

| 编号 | 文档 | 语义模块 |
|------|------|---------|
| 00 | `00-init-overview.md` | 总览：init 是什么、boot 链位置、状态机主线图、导航 |
| 01 | `01-init-main-entry.md` | main() 入口：身份/setsid/getopt/mfs_dev/信号注册/securelevel 探测 |
| 02 | `02-init-state-machine.md` | 状态机骨架：transition/requested_transition/信号转换 |
| 03 | `03-init-logging-failure.md` | 日志（stall/warning/emergency/syslog）+ 致命信号 disaster |
| 04 | `04-init-single-user.md` | 状态 's'：单用户 shell |
| 05 | `05-init-runcom.md` | 状态 'r'：/etc/rc 执行 |
| 06 | `06-init-read-ttys.md` | 状态 't'：/etc/ttys 解析与会话重建 |
| 07 | `07-init-session-model.md` | session_t 结构与生命周期 |
| 08 | `08-init-session-db.md` | 会话数据库（ARCH A-1：Berkeley DB → HashMap） |
| 09 | `09-init-multi-user.md` | 状态 'm'（稳态）：getty 启动 + waitpid 主循环 |
| 10 | `10-init-clean-ttys.md` | 状态 'T'：ttys 重读与下线处理 |
| 11 | `11-init-shutdown.md` | 状态 'c'/'d'：catatonia + death |
| 12 | `12-init-sysctl-interaction.md` | securelevel + CHROOT init.root（sysctl 交互） |
| 13 | `13-init-utmp.md` | utmp/utmpx 会话日志（ARCH A-2） |
| 14 | `14-init-external-contracts.md` | 对外契约：reboot/powerdown/孤儿收养/USR_F |
| 99 | `99-init-global-concepts.md` | 常量/路径/全局状态总表 |

## 关键文件

- `plan.md` — 文档重组计划（定稿，含覆盖契约 §5 与 ARCH 清单 §4）
- `draft/` — 旧占位素材（README）
- `checklist.md` — 函数级基线（实现期创建，参照 02-stage-vm/checklist.md 模式）

## 启动链路位置

```
kernel → VM → RS → PM/SCHED/VFS/DS/MIB → IS/DEVMAN/INPUT/IPC → INIT（本目录，boot 终点）
```
