#!/usr/bin/env python3
"""NK4-B P3 M3.3：证明「把 raw_serial_line 从 bin 搬进库」对 x86 腿零语义变化。

两个比对：
1. `lib.rs` 里 x86 的 `raw_serial` 函数体（去空白后）与 HEAD 逐字符相同；
2. x86 的发射指令（`asm!` / 端口号 / 轮询上限那几行）在 HEAD 的
   `main.rs::raw_serial_line` 与当前的 `lib.rs::emit_byte` 里逐字符相同。
任一不同即退出码 1。
"""
import re
import subprocess
import sys


def show(rev, path):
    return subprocess.run(['git', 'show', f'{rev}:{path}'],
                          capture_output=True, text=True, check=True).stdout


def brace_body(src, anchor):
    """返回 `anchor` 之后与之配对的 `{...}` 原文（不修剪空白）。"""
    i = src.index(anchor)
    j = src.index('{', i)
    depth = 0
    for k in range(j, len(src)):
        if src[k] == '{':
            depth += 1
        elif src[k] == '}':
            depth -= 1
            if depth == 0:
                return src[j:k + 1]
    raise AssertionError('括号未配对：' + anchor)


head_main = show('HEAD', 'os/boot-shim/src/main.rs')
head_lib = show('HEAD', 'os/boot-shim/src/lib.rs')
cur_lib = open('os/boot-shim/src/lib.rs').read()

ANCHOR_RS = ('pub fn raw_serial(text: &str) {\n    for byte in text.bytes() {\n'
             '        // SAFETY: COM1 数据口')
old_rs = re.sub(r'\s+', '', brace_body(head_lib, ANCHOR_RS))
new_rs = re.sub(r'\s+', '', brace_body(cur_lib, ANCHOR_RS))

KEYS = ('asm!', 'spins', 'lsr', '0x3f')


def key_lines(text):
    out = []
    for ln in text.splitlines():
        t = ln.strip()
        if any(k in t for k in KEYS):
            out.append(re.sub(r'\s+', '', t))
    return out


old_emit = key_lines(brace_body(head_main, 'fn raw_serial_line(text: &str) {'))
new_emit = key_lines(brace_body(cur_lib, 'fn emit_byte(b: u8) {'))

print('比对项 1：x86 raw_serial 函数体           ', '相同' if old_rs == new_rs and old_rs else '不同')
print('比对项 2：x86 发射指令行')
print('   HEAD main.rs::raw_serial_line :', old_emit)
print('   当前 lib.rs::emit_byte        :', new_emit)

ok = (old_rs == new_rs and bool(old_rs) and old_emit == new_emit
      and len(old_emit) >= 3)
print('RESULT:', 'PASS' if ok else 'FAIL')
sys.exit(0 if ok else 1)
