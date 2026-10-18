#!/usr/bin/env python3
"""colrm 两根腿的判据矩阵（A8）：：{读腿干净, 读腿先给字节后失手} × {输出小, 输出大}，写洞一律 /dev/full。

为什么要这张表：C 报哪条腿取决于它在自己处理顺序里先撞上哪个错——
- 读坏且输出还没攒满 stdout 缓冲：那次写根本没落到 fd 上，只有读错可见 → 报 stdin（`colrm.c:99` 的 `case EOF: check(stdin)`）；
- 输出攒满过：刷缓冲撞 ENOSPC，`putchar(ch) == EOF` 那条支路（`:117-119`）当场 `check(stdout)` → 报 stdout；
- 读干净而写坏：`check(stdin)` 见 `feof` 就 `exit(0)`，退出时 stdio 静默刷缓冲，**写错根本不报、退码 0**。
最后一形是本轮才发现的，它和本壳现状（写坏必报 stdout 并退 1）直接相对。

翻转点的字节数本身是承载差异：Minix 的 stdio 缓冲是 `BUFSIZ`＝1024（`minix3/include/stdio.h:182`），
宿主 glibc 这里是 4096。所以只有两侧都同意的那两形（远小于 1024、远大于 4096）能当门用。
"""
import os
import pty

os.makedirs("/tmp/colrm-probe", exist_ok=True)
import subprocess
import sys

LINE = b"abcdefghij\n"
SIZES = [(200, "输出约 140 字节（远小于 1024）"), (9000, "输出约 6300 字节（大于 4096）")]
TARGETS = [("C 参照件", sys.argv[1]), ("本壳", sys.argv[2])]


def feed(slave, size):
    data = LINE * (size // len(LINE) + 1)
    written = 0
    try:
        while written < len(data):
            written += os.write(slave, data[written:])
    except OSError:
        pass


def run(binary, size, broken_read, sink):
    """broken_read 用 pty（先给字节再 EIO）；否则用真文件。sink 是 stdout 的落点。"""
    out = os.open(sink, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    if broken_read:
        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [binary, "3", "6"], stdin=master, stdout=out, stderr=subprocess.PIPE
        )
        os.close(master)
        feed(slave, size)
        os.close(slave)
    else:
        path = f"/tmp/colrm-probe/in_{size}.txt"
        with open(path, "wb") as fh:
            fh.write(LINE * (size // len(LINE) + 1))
        stdin_fd = os.open(path, os.O_RDONLY)
        proc = subprocess.Popen(
            [binary, "3", "6"], stdin=stdin_fd, stdout=out, stderr=subprocess.PIPE
        )
        os.close(stdin_fd)
    _, stderr = proc.communicate(timeout=20)
    os.close(out)
    written_bytes = os.path.getsize(sink) if sink.endswith(".txt.out") else -1
    return proc.returncode, stderr.decode(errors="replace").strip(), written_bytes


for label, binary in TARGETS:
    for size, note in SIZES:
        for broken, rname in [(True, "读坏"), (False, "读净")]:
            code, errtxt, _ = run(binary, size, broken, "/dev/full")
            print(
                f"{label} | {rname} + {note} + 写坏(/dev/full): 退码={code} stderr=[{errtxt}]"
            )
