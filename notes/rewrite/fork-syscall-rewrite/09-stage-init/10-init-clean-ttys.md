# 10-init-clean-ttys: ttys 重读与下线处理（'T'）

> **状态**: pending（最小骨架，待改写）
> **定位**: 状态机 `'T'`——SIGHUP 触发：重读 /etc/ttys，关停下线行，启动新开行
> **源码**: `minix3/sbin/init/init.c`：`clean_ttys`（1569-1632）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- 全部会话 `SE_PRESENT` 清除 → 重读 /etc/ttys（do_setttyent）逐行匹配
- 匹配逻辑：`ty_name` vs `se_device + sizeof(_PATH_DEV)-1`（去掉 `/dev/` 前缀）；`se_index` 变化告警
- 下线（`TTY_ON` 未置或 `ty_getty` 空）→ `SE_SHUTDOWN` + `kill(se_process, SIGHUP)`；上线 → `SE_PRESENT` 恢复；新行 → `new_session`
- `setupargv` 解析失败 → SE_SHUTDOWN + SIGHUP
- 收尾：仍无 SE_PRESENT 的会话 → SE_SHUTDOWN + SIGHUP；n² 算法（注释 "We hope it isn't run often..."）
- 返回 `multi_user`

## 边界

- **前置依赖**: 07（会话模型）
- **不覆盖（移交）**: multi_user 主循环（09）、death 的三轮 kill（11）
