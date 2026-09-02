# 20-Minix 特有与系统信息

> **状态**: pending（最小骨架，待改写）
> **定位**: Minix 特有与系统信息
> **源码**: `minix3/minix/commands/{version,readclock,lspci,intr,devsize,dhrystone,worldstone,sprofalyze,sprofdiff,srccrc,printroot,profile,playwave,recwave}/、sbin/sysctl/、usr.sbin/{i2cscan,zdump,zic}/、usr.bin/ldd/、minix/usr.bin/{eepromread,trace}/、etc/system.conf`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 系统信息命令契约表（20 命令）：version/readclock/lspci/intr/printroot/devsize/eepromread
- - 基准面：dhrystone/worldstone；性能剖析：sprofalyze/sprofdiff/profile（sprof 族）
- - 系统参数面：sysctl（sbin + usr.sbin）/system.conf；时区：zdump/zic
- - ldd：静态链接面语义变化（[ARCH] A-1，无动态链接器）
- - 音频面：playwave/recwave（[ARCH] A-13 defer 候选）

## 边界

- - **前置依赖**: 00
- - **不覆盖（移交）**: 硬件信息获取接口（16-stage-drivers）

