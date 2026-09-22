#!/usr/bin/env bash
# NK4-B P3 M3.3：PL011 节流宿主测试的反向实测（临时变异 → 跑宿主测试 → 还原）。
# 目的：证明新增的 3 条宿主测试不是「跟着实现同义反复」，而是真能抓住回归。
# 每个变异都必须在宿主上失败（或挂死），还原后必须全绿。输出由调用方 tee 归档。
set -u

REPO=/home/xzhao/github/minix-rs
SRC="$REPO/os/plat/src/early_console.rs"
BAK="$REPO/tmp/nk4a/ec-orig-backup.rs"
OUT="$REPO/tmp/nk4a/mutant-out.txt"

run_host() { # $1 = 描述
    echo "--- $1 ---"
    cd "$REPO/os" || return 1
    timeout 240 docker run --rm -v "$PWD:/work" -w /work -m 2g \
        minix-ci:1.94 cargo test -j 1 -p minix-plat early_console -- --test-threads=1 \
        > "$OUT" 2>&1
    local st=$?
    grep -E "^test |test result|panicked at|^error" "$OUT" | head -20
    echo "RUN-EXIT=$st（124 = 超时 = 测试挂死）"
}

cp "$SRC" "$BAK"
trap 'cp "$BAK" "$SRC"; rm -f "$OUT"' EXIT

# 变异 A：把「先等后写」退回修复前的原实现（完全不等发送侧腾空）。
# 写成 `&& false` 而不是删掉循环：保留循环体才能让 `break` 合法存在，
# 又让条件恒假 → 语义 = 拿到一次「满」也直接写。
python3 - "$SRC" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = "    while tx_full() {"
new = "    while tx_full() && false {"
assert s.count(old) == 1, "变异 A 定位失败"
open(p, 'w').write(s.replace(old, new))
PY
run_host "变异 A：完全不等待（= 修复前行为）→ 期望「到限仍写」「等满才放行」两条 FAIL"
cp "$BAK" "$SRC"

# 变异 B：删掉轮询上限（真机上 = early console 死循环风险）。
python3 - "$SRC" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = """        if spins >= limit {
            break;
        }"""
new = """        let _ = limit;"""
assert s.count(old) == 1, "变异 B 定位失败"
open(p, 'w').write(s.replace(old, new))
PY
run_host "变异 B：去掉上限 → 期望 RUN-EXIT=124（第一条测试就挂死）"
cp "$BAK" "$SRC"

run_host "还原后复跑 → 期望 3 passed"
echo "=== 还原核对：SRC 应与变异前的备份逐字节相同 ==="
if diff -q "$BAK" "$SRC" >/dev/null; then echo "RESTORE-OK（变异已全部撤销）"; else echo "RESTORE-DRIFT！"; diff "$BAK" "$SRC" | head; fi
echo "=== 本轮真实改动面（只有已宣布的文件）==="
cd "$REPO" && git status --short os/
