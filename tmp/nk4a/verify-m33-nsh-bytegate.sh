#!/usr/bin/env bash
# NK4-B P3 M3.3 评审闭环：证明 startup.nsh 的逐字节契约测试真的在守字节。
# 变异 = 把本批一度引入的「尾部多一个反斜杠」写回去（EDK2 Shell 里两种写法
# 行为等价，所以「能不能启动」测不到它——这正是这条测试存在的理由）。
set -u
REPO=/home/xzhao/github/minix-rs
F="$REPO/os/xtask/src/image.rs"
W="$REPO/tmp/nk4a"
T=startup_nsh_bytes_are_the_frozen_template_per_loader_name

run_test() {
    docker run --rm -v "$REPO:/work" -w /work/os -m 2g minix-ci:1.94 \
        cargo test -j 1 -p xtask > "$W/xt-$1.log" 2>&1; echo "$?"
}
verdict() {
    grep -E "^test result" "$W/xt-$1.log" | tail -1 | sed 's/^/    /'
    if grep -qE "test image::tests::$T \.\.\. FAILED" "$W/xt-$1.log"; then
        echo "    $T → FAILED"
    else
        echo "    $T → passed"
    fi
}

cp "$F" "$W/img.bak"
echo "### 1) 正例：12 passed"
echo "  EXIT=$(run_test p2fix)"; verdict p2fix

echo "### 2) 变异：cd 行尾多一个反斜杠（行为等价、字节不等价）"
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = 'cd EFI\\\\BOOT\\r\\n{}'
assert s.count(old) == 1, s.count(old)
open(p, 'w').write(s.replace(old, 'cd EFI\\\\BOOT\\\\\\r\\n{}'))
PY
grep -n "let startup_nsh" "$F" | sed 's/^/  变异后: /'
echo "  EXIT=$(run_test p2mut)"; verdict p2mut

echo "### 3) 还原复跑"
cp "$W/img.bak" "$F"
diff -q "$W/img.bak" "$F" >/dev/null && echo "  RESTORE-OK"
echo "  EXIT=$(run_test p2back)"; verdict p2back
git -C "$REPO" status --short os/xtask/
rm -f "$W/img.bak"
