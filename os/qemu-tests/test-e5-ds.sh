#!/bin/bash
# test-e5-ds.sh — E5(f) 真机半：DS 数据服务传输链的上机验收门。
#
# 判据本体是盘上的用户态载体 `/bin/e5-ds`（源码
# `os/qemu-tests/test-kernels/user/e5-ds/src/main.rs`）：它在真机 CPL3 经
# minix-sys `DsClient` 向 DS（端点 6）发六次真任务调用，逐条断言 C 的拒绝码
# （`minix3/minix/servers/ds/store.c` 的源闸/查表分支）。一次往返同时立起
# 四条腿——SYS_SETGRANT 授权表登记、int-33 `_taskcall`、DS 服务器收发环、
# 服务器侧 sys_safecopyfrom/sys_safecopyto——所以"载体退出码 0 + 段标记齐全"
# 就是这四条腿的真机终态，不是宿主 mock 的自我确认。
#
# 串口判据（载体逐段落盘，门逐条核）：
#   e5-ds: begin                       载体被 rc 拉起并跑到第一条调用前
#   e5-ds: step chain begin            即将发第一条 taskcall（卡在这里＝DS 没在服务）
#   e5-ds: chain ok                    六条 C 语义拒绝码逐条对上
#   e5-ds: E5f-LABELS n/12             RS→DS 的 label 植入读数（观察项，见下）
#   ### TEST_RESULT: PASS e5-ds ###    载体终态
#
# `E5f-LABELS n/12` 是判据2：C 在 DS 冷启动时用 sef_cb_init_fresh 拉 RS 的
# rprocpub 整表植入 label（store.c:267-279），本树该 shadow 半已落地，门要求
# 12/12。发布/删除/订阅/通知四族对本载体不可达（无 label 的进程被
# C 的源闸挡下，正是门断言的内容），其真机半需要一个 RS 拉起的具名服务。
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# 环境变量：
#   SKIP_BUILD=1     复用已装配的 minix.img（迭代时用，跳过 xtask image）
#   TIMEOUT_BOOT     等 "entering scheduler" 的秒数（默认 180）
#   TIMEOUT_TEST     等载体终态标记的秒数（默认 180）

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-180}"
TIMEOUT_TEST="${TIMEOUT_TEST:-180}"

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"; exit 2
fi
if ! command -v mdir &>/dev/null || ! command -v mcopy &>/dev/null; then
    echo "SKIP: mtools (mdir/mcopy) not found"; exit 2
fi

FW=""; FW_SRC_VARS=""
for c in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$c" ] && FW="$c" && break
done
for c in /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done
if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
    echo "SKIP: OVMF firmware not found"; exit 2
fi

# ── 1. 装配镜像（含 e5-ds 播种），或直接复用 ──
IMG="$ROOT/target/image/x86_64/minix.img"
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "assembling bootable image (xtask image --arch x86_64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch x86_64 --release ) || {
        echo "FAIL: xtask image assembly failed"; exit 1
    }
fi
[ -f "$IMG" ] || { echo "FAIL: $IMG not present (run without SKIP_BUILD)"; exit 1; }

# 播种事实先行核对。`/bin` 不在 FAT 的 ESP 上，它在 `EFI/minix/imgrd` 这块
# MFS 根盘里（mdir 读不了 MFS）；播种的唯一真源是 mkfs 用的 proto 清单
# （`target/image/x86_64/imgrd.proto` 的 `/bin` 段），盘上没有它就无从谈起。
PROTO="$ROOT/target/image/x86_64/imgrd.proto"
[ -f "$PROTO" ] || { echo "FAIL: 缺 proto 清单 $PROTO（先跑一次不带 SKIP_BUILD）"; exit 1; }
grep -q "^e5-ds " "$PROTO" || {
    echo "FAIL: /bin/e5-ds 未播种进 imgrd（播种名单真源 xtask E5_DS_READY）"
    echo "      proto 的 /bin 段：$(grep -c '^ .* ---755' "$PROTO") 行，无 e5-ds"
    exit 1
}
echo "image ok: $IMG（proto 已列 /bin/e5-ds）"

# ── 2. 起机，串口进证据文件（不删，门失败时可回看）──
EVIDENCE_DIR="$ROOT/../tmp/evidence"
mkdir -p "$EVIDENCE_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
SERIAL_LOG="$EVIDENCE_DIR/e5-ds-serial-$STAMP.log"
FW_VARS="$(mktemp /tmp/e5_ds_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
MON_SOCK="$(mktemp /tmp/e5_ds_mon.XXXXXX.sock)"
rm -f "$MON_SOCK"

qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp 1 \
    -m 512M \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "file=$IMG,format=raw,media=disk" \
    -serial "file:$SERIAL_LOG" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$FW_VARS" "$MON_SOCK"
    # 串口证据保留（tmp/evidence/），只清临时固件与 socket——门失败要能回看。
    echo "evidence: $SERIAL_LOG"
}
trap cleanup EXIT

# ── 3. 等调度器交接（T1 契约），再等载体终态 ──
reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "entering scheduler" "$SERIAL_LOG" 2>/dev/null; then
        reached=1
        break
    fi
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: guest never reached the scheduler hand-off（全系统启动腿断了，非本门射程）"
    tail -8 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi
echo "serial: scheduler hand-off reached — waiting for the e5-ds carrier"

verdict=""
for _ in $(seq 1 "$TIMEOUT_TEST"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "TEST_RESULT: \(PASS\|FAIL\) e5-ds" "$SERIAL_LOG" 2>/dev/null; then
        verdict=1
        break
    fi
    sleep 1
done

echo "--- carrier evidence (e5-ds lines) ---"
grep -a "e5-ds: \|E5f-LABELS" "$SERIAL_LOG" 2>/dev/null | tr -d '\r' | sed 's/^/  /' || true

if [ -z "$verdict" ]; then
    echo "FAIL: ${TIMEOUT_TEST}s 内没有 e5-ds 终态标记"
    if grep -qa "e5-ds: step chain begin" "$SERIAL_LOG" 2>/dev/null; then
        echo "      已发出首条 taskcall 而无回复 ⇒ DS 未在服务（阻塞在 sendrec）"
    elif grep -qa "rc: minimal boot script marker" "$SERIAL_LOG" 2>/dev/null; then
        echo "      rc 已跑起来却没见载体 begin ⇒ /bin/e5-ds 装载或 exec 腿断"
    else
        echo "      rc marker 也没到 ⇒ init/runcom 链断，先查命令面门"
    fi
    tail -12 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi

# ── 4. 逐条判据 ──
pass=1
grep -qa "e5-ds: begin" "$SERIAL_LOG"            || { pass=0; echo "FAIL: 载体没打印 begin"; }
grep -qa "e5-ds: step chain begin" "$SERIAL_LOG" || { pass=0; echo "FAIL: 没走到首条 taskcall"; }
grep -qa "e5-ds: chain ok" "$SERIAL_LOG"         || { pass=0; echo "FAIL: 传输链段未通过（见上面逐条 want/got）"; }
grep -qa "### TEST_RESULT: PASS e5-ds ###" "$SERIAL_LOG" \
                                               || { pass=0; echo "FAIL: 终态标记不是 PASS"; }
grep -qa "E5f-LABELS " "$SERIAL_LOG"            || { pass=0; echo "FAIL: 没有 E5f-LABELS 读数行"; }

LABELS="$(grep -ao 'E5f-LABELS [0-9]*/12' "$SERIAL_LOG" | tail -1 | sed 's@.*LABELS \([0-9]*\)/.*@\1@')"
echo "label 植入读数: ${LABELS:-none}/12"
# 判据2（E5(f) 第一条）：12 条服务 label 全部可解析——DS 冷启动经 RS 的
# rproctab grant 植入（C store.c:267-279）的真机回归点。名字面 MFS 那一行
# 叫 fs_imgrd（RS 启动表与 C table.c:27 同名）。
if [ "${LABELS:-0}" != "12" ]; then
    pass=0
    echo "FAIL: label 植入不等于 12/12（读数 ${LABELS:-none}）"
fi

if [ "$pass" -eq 1 ]; then
    echo "RESULT: PASS (DS 传输链四腿真机通电：SYS_SETGRANT + int-33 taskcall + DS 服务环 + safecopy 双腿 — E5(f)/E-DSWIRE)"
    exit 0
fi
echo "RESULT: FAIL"
exit 1
