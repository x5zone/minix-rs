#!/usr/bin/env bash
# NK4-B P3 M3.3 决策四：证明 xtask 的 aarch64 装机表真的在起作用。
# 每条变异都必须让 `plan_aarch64_switches_every_uefi_slot` 失败——否则该
# 断言只是「整串里出现过某个字符串」的弱断言，挡不住把 x86 值写回去。
# 宿主 cargo 镜像跑 xtask（纯 std 包，不需要裸机 target）。
set -u
REPO=/home/xzhao/github/minix-rs
F="$REPO/os/xtask/src/image.rs"
W="$REPO/tmp/nk4a"
T=plan_aarch64_switches_every_uefi_slot

run_test() { # $1 = 日志后缀
    docker run --rm -v "$REPO:/work" -w /work/os -m 2g minix-ci:1.94 \
        cargo test -j 1 -p xtask > "$W/xt-$1.log" 2>&1; echo "$?"
}
verdict() { # 打印 test result 行 + 目标测试是否失败
    grep -E "^test result" "$W/xt-$1.log" | tail -1 | sed 's/^/    /'
    if grep -qE "test image::tests::$T \.\.\. FAILED" "$W/xt-$1.log"; then
        echo "    $T → FAILED"
    else
        echo "    $T → passed/未运行"
    fi
}

cp "$F" "$W/img.bak"

echo "### 1) 正例：全表生效时 11 passed"
echo "  EXIT=$(run_test pos)"; verdict pos

echo "### 2) 变异 A：aarch64 的 ESP 加载项名改回 BOOTX64.EFI"
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = '                loader: "BOOTAA64.EFI",'
assert s.count(old) == 1, s.count(old)
open(p, 'w').write(s.replace(old, '                loader: "BOOTX64.EFI",'))
PY
echo "  EXIT=$(run_test mutA)"; verdict mutA
cp "$W/img.bak" "$F"

echo "### 3) 变异 B：kernel-image 构建步的 --target 写回 x86 三元组"
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = '''            "-p",
            "kernel-image",
            "--target",
            arch.module_target(),'''
assert s.count(old) == 1, s.count(old)
new = old.replace("arch.module_target()", '"x86_64-unknown-none"')
open(p, 'w').write(s.replace(old, new))
PY
echo "  EXIT=$(run_test mutB)"; verdict mutB
cp "$W/img.bak" "$F"

echo "### 4) 变异 C：boot-shim 构建步的 --target 写回 x86 三元组"
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = '''            "--target",
            uefi.shim_target,'''
assert s.count(old) == 1, s.count(old)
open(p, 'w').write(s.replace(old, '            "--target",\n            "x86_64-unknown-uefi",'))
PY
echo "  EXIT=$(run_test mutC)"; verdict mutC
cp "$W/img.bak" "$F"

echo "### 5) 还原复跑：必须回到 11 passed"
diff -q "$W/img.bak" "$F" >/dev/null && echo "  RESTORE-OK"
echo "  EXIT=$(run_test restore)"; verdict restore
git -C "$REPO" status --short os/xtask/

rm -f "$W/img.bak"
