# minix-rs 的 QEMU 运行环境

这份文档回答一个问题：这台开发机上并存着四个 QEMU 版本——它们分别在哪、怎么调用、换一台机器怎么重建。

它存在的直接原因：riscv64 瞬态页表缺陷（项目登记为 (A)）的版本矩阵实验要求同一份内核镜像在不同 QEMU
上复跑，QEMU 本体是唯一变化的变量。四个版本的安装方式各不相同（系统包、源码自编译、Docker 容器），
任何一处的路径或版本号丢失，实验就不可复核，所以逐版本记录。每个事实断言都附取证命令或证据文件，
读者可以照着复跑。

## 1. 总览

| 版本 | 来源 | 安装位置 | 覆盖架构 | 用途 |
|------|------|----------|----------|------|
| 8.2.2 | Ubuntu 系统包 | `/usr/bin/qemu-system-*` | x86_64、aarch64、riscv64 | 全部测试脚本的默认版本 |
| 8.2.10 | qemu.org 发布包源码编译 | `~/opt/qemu/8.2.10/bin/` | x86_64、aarch64、riscv64 | riscv 取证版本矩阵 |
| 9.2.4 | qemu.org 发布包源码编译 | `~/opt/qemu/9.2.4/bin/` | x86_64、aarch64、riscv64 | riscv 取证版本矩阵 |
| 10.0.13 | Debian trixie 系统包，装在 Docker 容器内 | 容器内 `/usr/bin/` | 全架构 | riscv 取证版本矩阵 |

四个版本里只有 8.2.2 由系统包管理器维护；8.2.10 与 9.2.4 是用户主目录里的自编译安装树，
10.0.13 在长期运行的容器里，三者都不在 git 仓库内，换机器都会消失，重建方法见第 3、4 节。

## 2. 默认版本：系统 QEMU 8.2.2

系统是 Ubuntu 24.04.5 LTS（`/etc/os-release` 取证），QEMU 由 Ubuntu 包提供，
三个目标架构的二进制都在 PATH 上直接调用。取证：

```
$ qemu-system-riscv64 --version
QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.18)
$ which qemu-system-x86_64 qemu-system-aarch64 qemu-system-riscv64
/usr/bin/qemu-system-x86_64
/usr/bin/qemu-system-aarch64
/usr/bin/qemu-system-riscv64
```

`os/qemu-tests/` 与 `os/arch/tests/` 下的全部测试脚本都使用这组二进制：脚本内直接写
`qemu-system-<架构>` 的名称，没有版本覆盖开关（对这两个目录检索 `QEMU_BIN` 零命中，取证命令：
`grep -rn QEMU_BIN os/qemu-tests/ os/arch/tests/`）。因此系统包升级会同时改变所有脚本的行为——
四个版本里只有这一个会"自己变"。

## 3. 宿主自编译版本：8.2.10 与 9.2.4

### 3.1 位置与验证

两个版本用发布包源码编译，安装前缀分别是 `~/opt/qemu/8.2.10` 与 `~/opt/qemu/9.2.4`，
各带三个目标架构的 `qemu-system-*`，与系统包互不干扰。取证：

```
$ ~/opt/qemu/8.2.10/bin/qemu-system-riscv64 --version
QEMU emulator version 8.2.10
$ ~/opt/qemu/9.2.4/bin/qemu-system-riscv64 --version
QEMU emulator version 9.2.4
$ ls ~/opt/qemu/8.2.10/bin/
qemu-edid  qemu-ga  qemu-img  qemu-io  qemu-nbd  qemu-pr-helper
qemu-storage-daemon  qemu-system-aarch64  qemu-system-riscv64  qemu-system-x86_64
```

安装树里的二进制与源码树的构建目录 `build-minix-rs/` 逐字节一致（对账命令
`md5sum ~/opt/qemu/8.2.10/bin/qemu-system-riscv64 ~/src/qemu/qemu-8.2.10/build-minix-rs/qemu-system-riscv64`，
两处同为 `aa2bb81608dd9044f96b358ca4760b3b`；9.2.4 同理同为 `a02e89c6a880f06efb6fa1ec6ced9050`）。
每份源码树里还有另一个构建目录 `build/`，它不是安装来源，对账时以 md5 相同者为准。

### 3.2 构建参数（从 config.log 恢复）

两份源码树的构建目录各留有一份 configure 日志，命令行原样如下：

```
# ~/src/qemu/qemu-8.2.10/build-minix-rs/config.log
../configure --prefix=/home/xzhao/opt/qemu/8.2.10 \
    --target-list=x86_64-softmmu,aarch64-softmmu,riscv64-softmmu \
    --enable-plugins --disable-docs --disable-werror

# ~/src/qemu/qemu-9.2.4/build-minix-rs/config.log
../configure --prefix=/home/xzhao/opt/qemu/9.2.4 \
    --target-list=x86_64-softmmu,aarch64-softmmu,riscv64-softmmu \
    --enable-plugins --disable-docs --disable-werror
```

参数含义：`--target-list` 一次编出三个目标架构的系统模拟器；`--enable-plugins` 打开 TCG 插件接口
（跟踪类分析工具要用）；`--disable-docs` 跳过文档构建节省时间；`--disable-werror` 避免较新编译器
把告警升级为错误而中断构建。

安装前缀是固定值，QEMU 据此在安装树内查找自带固件：`-bios default` 使用的 OpenSBI 就在
`~/opt/qemu/<版本>/share/qemu/` 目录（`opensbi-riscv64-generic-fw_dynamic.bin` 等），
因此从任意目录调用都能正常引导。各版本内嵌的 OpenSBI 版本见第 6 节。

### 3.3 调用方式

riscv 取证脚本 `tmp/nk4a/riscv_halt_dump.sh`（以及变体 `tmp/nk4a/riscv_halt_dump_bios.sh`）
通过环境变量选择 QEMU：不设置时用系统 8.2.2，设置 `QEMU_BIN` 时用指定二进制。例如用 8.2.10
跑一轮冻结现场取证：

```
QEMU_BIN=$HOME/opt/qemu/8.2.10/bin/qemu-system-riscv64 \
    bash tmp/nk4a/riscv_halt_dump.sh <标签> <等待秒数>
```

### 3.4 重建步骤（换机器迁移）

发布包 tarball 留档在源码树旁边，可直接复用，不必重新下载：
`~/src/qemu/qemu-8.2.10.tar.xz`（约 132 MB）与 `~/src/qemu/qemu-9.2.4.tar.xz`（约 134 MB），
`tar -tJf` 首条均为 `qemu-<版本>/`，是标准发布包布局。重建命令（以 8.2.10 为例，
9.2.4 把版本号与前缀替换即可）：

```
mkdir -p ~/src/qemu && cd ~/src/qemu
tar xf qemu-8.2.10.tar.xz
mkdir -p qemu-8.2.10/build-minix-rs && cd qemu-8.2.10/build-minix-rs
../configure --prefix="$HOME/opt/qemu/8.2.10" \
    --target-list=x86_64-softmmu,aarch64-softmmu,riscv64-softmmu \
    --enable-plugins --disable-docs --disable-werror
ninja
ninja install
```

构建依赖按安装出的二进制的动态链接关系反推（`ldd ~/opt/qemu/8.2.10/bin/qemu-system-riscv64`
列出 libfdt、libpixman-1、libgio-2.0、libz）：Ubuntu 侧需要 ninja-build、python3-venv、
libglib2.0-dev、libpixman-1-dev、libfdt-dev。［待验证：首次构建时实际安装的软件包清单未留档，
以上按链接依赖列出。］

## 4. 容器版本：QEMU 10.0.13

### 4.1 容器形态

10.0.13 来自 Debian trixie 的系统包；宿主是 Ubuntu，没有这个版本，因此它装在长期运行的
Docker 容器 `minix-rs-qemu-10.0.13` 里。容易记错的一点：容器镜像不是专门的 QEMU 镜像，
而是官方 Rust 镜像 `rust:trixie`（Rust 1.99.0，Debian 13 基座）——以 `docker inspect` 输出为准。取证：

```
$ docker inspect minix-rs-qemu-10.0.13 --format '{{.Config.Image}} {{.Created}} {{.State.Status}} {{.Config.WorkingDir}} {{.Config.Cmd}}'
rust:trixie 2026-10-04T07:40:48.792348555Z running /work [bash]
$ docker inspect minix-rs-qemu-10.0.13 --format '{{json .Mounts}}'
[{"Type":"bind","Source":"/home/xzhao/github/minix-rs","Destination":"/work","RW":true}]
$ docker exec minix-rs-qemu-10.0.13 qemu-system-riscv64 --version
QEMU emulator version 10.0.13 (Debian 1:10.0.13+ds-0+deb13u1)
```

容器内还装有 Rust 1.99.0 工具链（`rustc --version` 输出 `rustc 1.99.0 (b940084d7 ...)`），
以及 Debian 的整套 `qemu-system` 软件包（`dpkg -l` 取证包含 qemu-system-riscv、qemu-system-arm、
qemu-system-misc、qemu-system-common、qemu-system-data、qemu-system-gui 等，版本均为
`1:10.0.13+ds-0+deb13u1`），因此容器内 x86_64、aarch64、riscv64 都能跑。
注意与构建镜像区分：minix-rs 的构建用镜像是 `minix-ci:1.94`，两者互不相关（`docker images` 可同时看到）。

### 4.2 调用方式

裸调用就是透传一条命令：

```
docker exec -i minix-rs-qemu-10.0.13 qemu-system-riscv64 <参数...>
```

为省去手写容器名，`~/opt/qemu/bin/qemu-riscv64-10.0.13-docker` 是现成的包装脚本
（全部内容两行）：

```
#!/bin/sh
exec docker exec -i minix-rs-qemu-10.0.13 qemu-system-riscv64 "$@"
```

包装脚本只覆盖 riscv64；x86_64 与 aarch64 目前没有对应包装。`tmp/nk4a/riscv_halt_dump.sh`
也内置了容器分支：设置 `DOCKER_IMG` 环境变量（非空即生效）后，脚本把仓库根路径映射到容器内
`/work` 再调用 `docker exec`；容器名 `minix-rs-qemu-10.0.13` 硬编码在脚本里。

### 4.3 重建步骤

按 inspect 观察到的配置等价重建：

```
docker run -dit --name minix-rs-qemu-10.0.13 \
    -v /home/xzhao/github/minix-rs:/work -w /work \
    rust:trixie bash
docker exec minix-rs-qemu-10.0.13 sh -c 'apt-get update && apt-get install -y qemu-system'
```

`-dit` 让交互式 bash 常驻成为容器主进程（inspect 中 `Tty=true`、`OpenStdin=true` 与
`Cmd=[bash]` 即由此而来），容器不退出，`docker exec` 随时可用。［待验证：容器创建时的
原始命令行未留档，"安装 qemu-system 元包"是按容器内的软件包集合推断的等价重建步骤。］

### 4.4 注意事项

- **root 属主文件**：容器以 root 运行，它写进挂载目录的产物（串口日志、监视器套接字、
  内存镜像）属主是 root，宿主用户不能直接删除或覆盖。现场中已有实例：
  `tmp/nk4a/a280/mon_q35.sock` 与 `serial.q35.log` 属主为 root、大小为 0。
  清理时用 `sudo rm` 或在容器内删除。
- **容器名被脚本引用**：`riscv_halt_dump.sh` 的容器分支硬编码了容器名，重建容器必须用同名。
- **镜像 tag 会移动**：`rust:trixie` 是滚动 tag，本次实例的 image id 是 `5d05167b28ce`；
  在新机器上按上面的命令重建时拉到的可能是更新的 Rust 镜像，QEMU 版本取决于 Debian 仓库
  当时提供的 `1:10.0.13+ds-0+deb13u1`——若 Debian 已更新，需要显式指定包版本才能复现
  10.0.13 的精确行为。

## 5. 版本选择的脚本覆盖现状

能按版本切换的只有 riscv 取证脚本两处（`tmp/nk4a/riscv_halt_dump.sh`、
`tmp/nk4a/riscv_halt_dump_bios.sh`），机制是环境变量 `QEMU_BIN`（宿主二进制）与
`DOCKER_IMG`（容器）。`os/qemu-tests/` 与 `os/arch/tests/` 下的测试脚本没有这个能力：
全部硬编码 `qemu-system-<架构>`（对两目录检索 `QEMU_BIN` 零命中）。这意味着 x86_64 与
aarch64 的版本矩阵目前只能手动调用二进制完成；若要在脚本层支持，需要给这些脚本补同样的覆盖参数。

## 6. 各版本已知行为

各版本内嵌的 OpenSBI（`-bios default` 实际使用的固件）随 QEMU 版本不同，跑 riscv 实验
对照时这是变量之一：

| QEMU | 内嵌 OpenSBI | 证据 |
|------|--------------|------|
| 8.2.2 | v1.3 | `tmp/nk4a/a280/serial.w5.log` 首屏 |
| 8.2.10 | v1.3.1 | `tmp/nk4a/a280/serial.V8210.log` 首屏 |
| 9.2.4 | v1.5.1 | `tmp/nk4a/a280/serial.V924.log` 首屏 |
| 10.0.13 | v1.6 | 来源：BUG 文档第 10.6 节（本机暂无对应串口留档） |

最近一次加窗复跑（等待窗口 1800 秒）测得：8.2.10 与 9.2.4 都能跑完与 8.2.2 相同的完整轨迹，
耗时相当，终态事件计数与终态串口序列逐字节相同。参照量取"串口从首写到静默"的时长：

| 跑批 | QEMU | 静默时长 | 终态 |
|------|------|----------|------|
| w5 | 8.2.2 | 约 143 秒 | exec=12、故障 32 次（6 次早期 + 26 次反复）、no-region=1、杀进程 |
| w6 | 8.2.2 | 约 254 秒 | 同上 |
| V8210 | 8.2.10 | 约 160 秒 | 同上 |
| V924 | 9.2.4 | 约 246 秒 | 同上 |

口径说明：静默不等于 QEMU 退出——取证脚本会在窗口结束后经监视器保全内存再退出；
"静默时长"取串口文件创建到末次写入的间隔。这四轮使用的内核构建带有内核代读旁路
（内核侧替 VM 完成页表读取），因此它们测量的是旁路之后的残留症状（被测进程反复页故障、
页表簿记查找失败、最终被杀），不是缺陷 (A) 本体；完整实验记录与判读见 BUG 文档第 10 章。

注意：BUG 文档第 10.6 节记载 8.2.10 与 9.2.4 在 600 秒窗口内"卡死"（比基线慢约三个数量级）；
加窗复跑不支持这个记载，引用该节时以本节列出的串口证据为准。

## 7. 十秒自检

四条命令一次确认四个版本都在位：

```
$ qemu-system-riscv64 --version                                     # 期望 8.2.2
$ $HOME/opt/qemu/8.2.10/bin/qemu-system-riscv64 --version           # 期望 8.2.10
$ $HOME/opt/qemu/9.2.4/bin/qemu-system-riscv64 --version            # 期望 9.2.4
$ docker exec minix-rs-qemu-10.0.13 qemu-system-riscv64 --version   # 期望 10.0.13
```

任何一条报"找不到命令/容器"时，对应的重建步骤在第 3.4 节与第 4.3 节。

## 8. 相关文档

- 缺陷 (A) 的实验记录与分析：`rewrite-notes/coordination/NK4C-BUG-RISCV64-TRANSIENT-PTE.md`（先读第 10 章）
- 取证流水账：`rewrite-notes/coordination/NK4C-WORKLOG.md`
- 取证方法论：`rewrite-notes/coordination/riscv瞬态页表崩溃取证方法论.md`
- 换机迁移交接：`rewrite-notes/coordination/NK4C-MIGRATION-20260930.md`
- 取证脚本与工具：`tmp/nk4a/riscv_halt_dump.sh`、`tmp/nk4a/riscv_halt_dump_bios.sh`、`tmp/nk4a/matrix_judge.py`
