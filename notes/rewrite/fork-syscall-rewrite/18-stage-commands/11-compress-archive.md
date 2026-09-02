# 11-压缩与归档

> **状态**: pending（最小骨架，待改写）
> **定位**: 压缩与归档
> **源码**: `minix3/usr.bin/{gzip,bzip2,bzip2recover,unzip,pax,shar,uuencode,uudecode,bdes}/、minix/commands/compress/`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 压缩命令契约表（9 命令）：gzip/bzip2/bzip2recover/compress/unzip 的格式与选项面
- - 归档：pax（POSIX 归档，bin/pax）/shar（shell 归档）
- - 编解码：uuencode/uudecode（MIME 面）/bdes（DES 面）
- - [ARCH] A-5：压缩错误面 Result + errno 映射
- - 压缩算法库归属决策（14-stage-runtime 或独立库）

## 边界

- - **前置依赖**: 06
- - **不覆盖（移交）**: 压缩算法库实现（14-stage-runtime 或独立库）

