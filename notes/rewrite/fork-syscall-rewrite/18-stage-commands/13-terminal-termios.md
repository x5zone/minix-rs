# 13-终端控制与终端数据库

> **状态**: pending（最小骨架，待改写）
> **定位**: 终端控制与终端数据库
> **源码**: `minix3/usr.bin/{stty,tput,tic,infocmp}/、minix/commands/{term,termcap,tget,loadfont,loadkeys,screendump}/、etc/{termcap,termcap.big,fonts}`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 终端命令契约表（10 命令）：stty（termios 设置面）/tput/tic/infocmp（terminfo 编译查询）
- - term/termcap/tget：termcap 兼容查询面
- - 键盘/字体/屏幕：loadfont/loadkeys/screendump + etc/fonts
- - [ARCH] A-2 重大决策：terminfo 数据+解析器 vs 仅转义序列封装（决定 23 的实现面）
- - termios ABI 面：依赖 14-stage-runtime 的 termios 落地

## 边界

- - **前置依赖**: 12、14-stage-runtime（termios ABI）
- - **不覆盖（移交）**: 终端驱动（16-stage-drivers）

