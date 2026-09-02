# 10-文档排版、man 与开发辅助

> **状态**: pending（最小骨架，待改写）
> **定位**: 文档排版、man 与开发辅助
> **源码**: `minix3/usr.bin/{pr,fmt,nl,colcrt,deroff,checknr,indent,ctags,cal,calendar,gencat,lorder,mkstr,xstr,asa,fpr,fsplit,menuc,msgc,m4,soelim,tsort,ul,what,man,apropos,whatis,whereis,locale,mklocale,mkesdb,mkcsmapper}/、minix/commands/{cawf,spell,prep}/、etc/man.conf`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 排版工具契约表（35 命令：usr.bin 32 + minix/commands 3）：pr/fmt/nl/colcrt/deroff/checknr/indent/cawf/spell/m4/soelim/…
- - man 面：man/apropos/whatis/whereis + makewhatis（libexec）+ man.conf；[ARCH] A-11 man 数据体积决策
- - 开发辅助：ctags/gencat/lorder/mkstr/xstr/menuc/msgc/fpr/fsplit/asa
- - i18n 面：locale/mklocale/mkesdb/mkcsmapper；[ARCH] A-8 defer 候选
- - 时间/信息工具：cal/calendar/units

## 边界

- - **前置依赖**: 07/08
- - **不覆盖（移交）**: 文档内容数据（24 游戏文本面）

