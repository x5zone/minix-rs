# Minix-RS /etc 配置面

对应 minix3 的 `etc/`（系统配置）。本目录是宿主侧源；装机面
（`cargo run -p xtask image`）把它经 mkfs 原型播种进 imgrd 根盘
（消费 `os/xtask/src/image.rs` 的 `generate_etc_proto`）。

## 已落地（最小集，NS8）

- `rc` — init 进入 multi-user 前执行的启动脚本（init 侧消费见
  runcom 状态机，S42 批五）。
- `ttys` — 终端行配置；最小集只有 console 一行且状态 "off"
  （系统尚无 getty 二进制，"on" 会让 init 重生不存在的程序；
  getty 落地后翻 "on secure"）。
- `dev/console` 节点不经本目录——它是装机原型里的 char special
  行（major 4 = C dmap.h:25 TTY_MAJOR，minor 0），随 imgrd 落盘。

**内容口径**：OQ-3 盘上文件面的最小集草案，**待用户过目**；
扩项（rc.conf/fstab/passwd/hostname/profile…）按消费方就绪节奏
逐项上收，不预放占位文件。
