# Minix-RS /etc 配置占位

对应 minix3 的 `etc/`（系统配置，实装于 18-stage-commands 后）。

规划清单（占位，内容待实装时逐项填充）：
- `rc` / `rc.conf` / `rc.subr` — 启动脚本（对应 `minix3/etc/rc`）
- `fstab` — 挂载表（对应 `minix3/etc/newfstab.sh`）
- `passwd` / `group` / `master.passwd` — 用户与组
- `hostname` — 主机名
- `TTYS` — 终端行配置（对应 `minix3/etc/ttys`）
- `profile` — shell 配置文件
- `dev/` — 设备节点（devman 动态生成 + MAKEDEV 静态兜底）

状态: 占位，未实装。
