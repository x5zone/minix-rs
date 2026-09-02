# 18-stage-commands — 命令与系统配置（占位）

> **状态**: 占位（crate 已建，待实装）
> **定位**: 用户态命令补齐 + 系统配置落地

## 范围
- games 24 个（`os/commands/games/`，已有 3 个 + 新占位 21 个）
  - 纯 stdio 类（factor/primes/bcd/morse/number/pig/arithmetic/caesar/banner/ppt）— 最早可验收
  - 终端控制类（worm/worms/rain/colorbars/tetris/snake/rogue）— 需 termios 落地
  - 文本类（adventure/monop/fortune/fish/wargames/wtf）
- bin 补齐（`os/commands/bin/`）、sbin 补齐（`os/commands/sbin/`）
- `service` / `svrctl`（RS 动态服务管理）
- `os/etc/` 配置：rc/rc.conf/fstab/passwd/group/hostname/TTYS/profile
- `/dev` 设备节点（devman 动态 + MAKEDEV 静态兜底）
- 登录链路: getty（`minix3/libexec/getty/`）+ login（多用户阶段）
- 终端库决策: terminfo/curses（minix3 games 链接 `-lterminfo`）vs 转义序列

## C 对应
- `minix3/games/`、`minix3/minix/commands/`、`minix3/sbin/`、`minix3/usr.bin/`、`minix3/usr.sbin/`、`minix3/etc/`
