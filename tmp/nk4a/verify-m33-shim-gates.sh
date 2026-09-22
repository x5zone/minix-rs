#!/usr/bin/env bash
# NK4-B P3 M3.3 决策一：boot-shim 的 fw-uefi-image 内部特性 + aarch64 腿构建门。
# 判据全部实跑，退出码逐条打印（不把 cargo 接进管道，避免退出码失真）。
# 裸机目标腿走宿主 toolchain（aarch64-unknown-uefi 只装在宿主）；宿主测试走 CI 镜像。
set -u
REPO=/home/xzhao/github/minix-rs
W="$REPO/tmp/nk4a"
A64="$REPO/os/target/aarch64-unknown-uefi/release/boot-shim.efi"
cd "$REPO/os" || exit 1

build_a64() { # $1 = 日志
    (ulimit -v 3145728; cargo build -j 1 -p boot-shim --target aarch64-unknown-uefi \
        --features fw-aarch64-uefi --release > "$1" 2>&1); echo "$?"
}

echo "### 1) aarch64 UEFI 裸机 bin 构建（新放行的一腿）+ 工件形态"
echo "  A64-SHIM-EXIT=$(build_a64 "$W/a64shim.log")"
file "$A64" | sed 's/^/  /'
readelf -h "$A64" 2>/dev/null | grep -E "Type:|Machine:" | sed 's/^/  /'

echo "### 2) x86_64 UEFI 腿不回归（对外特性名仍是 fw-x86-uefi）"
(ulimit -v 3145728; cargo build -j 1 -p boot-shim --target x86_64-unknown-uefi \
    --features fw-x86-uefi --release > "$W/x64shim.log" 2>&1); echo "  X64-SHIM-EXIT=$?"
file target/x86_64-unknown-uefi/release/boot-shim.efi | sed 's/^/  /'

echo "### 3) 宿主裸机隔离未退化（X-2/NK5）+ 宿主测试计数（CI 镜像内跑，挂仓根为 /work）"
docker run --rm -v "$REPO:/work" -w /work/os -m 2g minix-ci:1.94 bash -c \
    'cargo build --workspace --bins > /work/tmp/nk4a/ws-bins.log 2>&1; echo "  WS-BINS-EXIT=$?";
     cargo test -j 1 -p boot-shim 2>&1 | grep -E "^test result|^error" | sed "s/^/  默认特性 /";
     cargo test -j 1 -p boot-shim --features test-all 2>&1 | grep -E "^test result|^error" | sed "s/^/  test-all /"'
grep -c -E "eh_personality|^error" "$W/ws-bins.log" | sed 's/^/  隔离破坏关键词数: /'

echo "### 4) 反向判别：把 bin 的门改回架构专属名，aarch64 腿必须产不出工件"
cp boot-shim/Cargo.toml "$W/bs-cargo.bak"
cargo clean -p boot-shim 2>/dev/null
echo "  正例（门 = fw-uefi-image）EXIT=$(build_a64 "$W/pos.log")，工件：$(ls -l "$A64" 2>&1 | tail -1)"
sed -i 's/^required-features = \["fw-uefi-image"\]/required-features = ["fw-x86-uefi"]/' boot-shim/Cargo.toml
grep -H required-features boot-shim/Cargo.toml | sed 's/^/  变异后: /'
cargo clean -p boot-shim 2>/dev/null
echo "  负例（门 = fw-x86-uefi）EXIT=$(build_a64 "$W/neg.log")（退出码 0 但无工件 = bin 被跳过）"
ls -l "$A64" 2>&1 | sed 's/^/  /'
grep -E "^error|Warning" "$W/neg.log" | head -3 | sed 's/^/  /'
cp "$W/bs-cargo.bak" boot-shim/Cargo.toml
diff -q "$W/bs-cargo.bak" boot-shim/Cargo.toml >/dev/null && echo "  RESTORE-OK"
cargo clean -p boot-shim 2>/dev/null
echo "  还原复跑 EXIT=$(build_a64 "$W/rerun.log")，工件：$(ls -l "$A64" 2>&1 | tail -1)"
rm -f "$W/bs-cargo.bak" "$W/pos.log" "$W/neg.log" "$W/rerun.log" "$W/a64shim.log" \
      "$W/x64shim.log" "$W/ws-bins.log"
