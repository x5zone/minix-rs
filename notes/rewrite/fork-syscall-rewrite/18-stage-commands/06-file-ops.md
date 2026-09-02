# 06-文件操作命令

> **状态**: pending（最小骨架，待改写）
> **定位**: 文件操作命令
> **源码**: `minix3/bin/{cat,chmod,cp,df,echo,expr,ln,ls,mkdir,mv,pwd,rm,rmdir,sync,test}/、usr.bin/{basename,dirname,du,false,find,flock,mkfifo,mktemp,pathchk,printf,stat,touch,true,xargs,xinstall}/、usr.sbin/{chroot,link,unlink}/、minix/commands/truncate/`
> **Rust 模块**: `os/commands/bin/{cat,cp,echo,ls,mv,rm}`
> **draft 素材**: 无（新建）

## 核心点

- - 文件操作命令契约表（34 命令）：权限（chmod/chown）、链接（ln/link/unlink）、复制（cp/xinstall）、查找（find/ls）、空间（df/du/stat/truncate）
- - 判断与表达式：test/expr/true/false/printf/echo 的 POSIX 语义
- - 同步面：sync/flock/mkfifo/mktemp/pathchk
- - [ARCH] A-9：64 位文件大小/偏移（df/du/stat）
- - 每命令契约：选项/输入输出/退出码/错误面（§3.6 命令契约表格式）

## 边界

- - **前置依赖**: 05（shell 内建面）、14-stage-runtime（文件 syscall）
- - **不覆盖（移交）**: 文本处理（07）、存储管理（14~17）

