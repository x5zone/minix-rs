#!/usr/bin/env bash
# NK4-B P3 M3.3 决策一：bin 的门（required-features）确实决定 aarch64 腿能否产出工件。
# 上一版用 `cargo clean -p boot-shim` 想造「无工件」现场，实测它并没有删掉
# target/aarch64-unknown-uefi/ 下的 .efi（三次 ls 的时间戳全是同一次构建），
# 因此那次的「负例」什么也没证明。本脚本改用显式删工件 + 重建：
#   第 1 步 = 证明「删掉后重建会再生」，否则第 2 步的「不再生」没有意义。
set -u
REPO=/home/xzhao/github/minix-rs
W="$REPO/tmp/nk4a"
A64="$REPO/os/target/aarch64-unknown-uefi/release/boot-shim.efi"
cd "$REPO/os" || exit 1

build_a64() {
    (ulimit -v 3145728; cargo build -j 1 -p boot-shim --target aarch64-unknown-uefi \
        --features fw-aarch64-uefi --release > "$1" 2>&1); echo "$?"
}
state() { # 工件存在性 + 时间戳（区分"没重建"与"重建了"）
    if [ -e "$A64" ]; then echo "存在 $(date -r "$A64" +%H:%M:%S)"; else echo "不存在"; fi
}

cp boot-shim/Cargo.toml "$W/bs-cargo.bak"

echo "### 1) 前置：删掉工件后重建必须再生（否则后面的「不再生」无意义）"
rm -f "$A64"; echo "  删后: $(state)"
sleep 1
echo "  正例门（fw-uefi-image）重建 EXIT=$(build_a64 "$W/p1.log")，工件: $(state)"

echo "### 2) 负例：把门改成架构专属名 fw-x86-uefi，同一命令必须产不出工件"
sed -i 's/^required-features = \["fw-uefi-image"\]/required-features = ["fw-x86-uefi"]/' boot-shim/Cargo.toml
grep -m1 '^required-features' boot-shim/Cargo.toml | sed 's/^/  变异后: /'
rm -f "$A64"; sleep 1
echo "  负例重建 EXIT=$(build_a64 "$W/p2.log")（退出码 0：bin 被 cargo 静默跳过，不报错）"
echo "  工件: $(state) ← 必须「不存在」，才证明门真的在起作用"
grep -c "Compiling boot-shim" "$W/p2.log" | sed 's/^/  boot-shim 被编译次数: /'

echo "### 3) 还原复跑：门改回 fw-uefi-image → 工件必须再生"
cp "$W/bs-cargo.bak" boot-shim/Cargo.toml
diff -q "$W/bs-cargo.bak" boot-shim/Cargo.toml >/dev/null && echo "  RESTORE-OK"
rm -f "$A64"; sleep 1
echo "  还原重建 EXIT=$(build_a64 "$W/p3.log")，工件: $(state)"
git -C "$REPO" status --short os/boot-shim/

rm -f "$W/bs-cargo.bak" "$W/p1.log" "$W/p2.log" "$W/p3.log"
