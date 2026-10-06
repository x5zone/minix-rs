//! T7 出生应答原语形状审计（P-ALL-08 §5.2 待补测试）。
//!
//! # 契约（C 真源）
//!
//! 出生回报腿的 IPC 原语选型由 C 真源钉死、两种并存：
//! - **一般服务**（RS 收侧等 reply）：默认回调
//!   `SEF_CB_INIT_RESPONSE_DEFAULT = sef_cb_init_response_rs_reply`
//!   （sef.h:90）＝ `ipc_sendrec(RS_PROC_NR, m)`（sef_init.c:458-466）——
//!   **同步阻塞 sendrec**，send 半停在 RS 的 receive、reply 半等 RS 的
//!   OK 唤醒。
//! - **VM 特例**：`rs_asynsend(rp, &m, 0)`（utility.c:62）＝ 异步一次，
//!   VM 在 vm/main.c:225-229 换回调后 asynsend3(AMF_NOREPLY)。
//!
//! # 为什么钉形状而不只钉行为
//!
//! 原语选型错位在生产行为上不立刻爆：send 会被 RS 的 receive 吃掉但
//! RS 等的是 sendrec 的 reply 半（RS 停在 receive 等 OK 唤醒）→ 回声
//! 投回 → step3 panic；sendnb 则 BS 出口竞速。本项目已修过一次（B9b），
//! TODO-3ARCH §P-ALL-08 标「易复发」——此审计把形状钉进 CI，回归时
//! 当场红而不是真机 step3 挂死后再查。
//!
//! 审计范式：源码扫描（对位 `riscv64_ap_early_entry_pin.rs`）。

use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../")
}

fn read_server(rel: &str, name: &str) -> String {
    let full = crate_root().join(rel);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{} 源码可读: {}", name, e))
}

/// Birth 臂切片：从锚点起，到下一个 handler 边界或文件尾。
fn reply_leg_after<'a>(src: &'a str, anchor: &str, name: &str) -> &'a str {
    let zone = src
        .split(anchor)
        .nth(1)
        .unwrap_or_else(|| panic!("{} 缺锚点 {}", name, anchor));
    // 回报腿与 builder 之间不隔其他语句（C sef_init.c:113-117 的形状：
    // 构造 → 立即 response）。取紧邻 6 行作窗口，防扫进别 handler。
    let lines: Vec<&str> = zone.lines().take(6).collect();
    &zone[..lines.iter().map(|l| l.len() + 1).sum()]
}

const GENERAL_SERVICE_BIRTH_SITES: &[(&str, &str)] = &[
    ("servers/is/src/lib.rs", "IS"),
    ("servers/mib/src/server.rs", "MIB"),
    ("servers/ds/src/server.rs", "DS"),
    ("servers/ipc-server/src/server.rs", "IPC"),
];

#[test]
fn birth_reply_leg_is_sendrec_for_general_services() {
    // 一般服务（IS/MIB/DS/IPC）的出生回报腿必须是 `send_rec(Endpoint::RS`
    // ——C 默认回调 sef_cb_init_response_rs_reply = ipc_sendrec（sef_init.c:
    // 463）。用 `asynsend` 是 VM 特例漂移。
    for (rel, name) in GENERAL_SERVICE_BIRTH_SITES {
        let src = read_server(rel, name);
        let arm = reply_leg_after(&src, "sef_init_reply", name);
        // 形状契约：原语必须是 send_rec（C ipc_sendrec 同形），去往
        // RS——显式 Endpoint::RS（IS/MIB）或 caller（DS：Birth 臂的
        // caller 恒为 RS，由上层的 m_source==RS 判定保证）皆可。
        assert!(
            arm.contains("send_rec("),
            "{} 的出生回报腿必须用 send_rec（C sef_init.c:463 ipc_sendrec 同形）；现臂：\n{}",
            name,
            arm
        );
        assert!(
            arm.contains("Endpoint::RS") || arm.contains("send_rec(caller,"),
            "{} 的出生回报腿必须显式去往 RS（Endpoint::RS 或 Birth 臂 caller）；现臂：\n{}",
            name,
            arm
        );
        assert!(
            !arm.contains("asynsend("),
            "{} 的出生回报腿不得用 asynsend（那是 VM 特例，utility.c:62）",
            name
        );
        assert!(
            !arm.contains("sendnb(") && !arm.contains("send_nonblocking"),
            "{} 的出生回报腿不得用 sendnb（reply 半竞速，B9b 之前的现场）",
            name
        );
    }
}

#[test]
fn vm_birth_reply_leg_is_asynsend_once() {
    // VM 特例：RS_INIT 回报走 asynsend（C utility.c:62 rs_asynsend；
    // vm/main.c:225-229 换回调）。同步 send_rec 会撞 RS boot 阶段的
    // receive 半（B9b 之前的现场）。
    let src = read_server("servers/vm/src/vm_server.rs", "VM");
    assert!(
        src.contains("asynsend(RS_PROC_NR, &minix_sef::sef_init_reply"),
        "VM 的 RS_INIT 回报腿必须是 asynsend(RS_PROC_NR, sef_init_reply(...))（C rs_asynsend 特例）"
    );
}

#[test]
fn sef_init_reply_builder_shape_is_untouched() {
    // T7+T6 联合锚：builder 产 RS_INIT+result、m_source=NONE（内核盖章）。
    // 形状漂移（如有人把 m_type 换成自定义值、或手工填 m_source）当场红。
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("minix-sef lib.rs 可读");
    let body = src
        .split("pub fn sef_init_reply")
        .nth(1)
        .expect("sef_init_reply 缺失");
    let end = body.find("\n}").unwrap_or(body.len());
    let body = &body[..end];
    assert!(
        body.contains("m_type: SEF_INIT_REQUEST_TYPE"),
        "sef_init_reply 必须用 SEF_INIT_REQUEST_TYPE 作 m_type（C: m_type = RS_INIT）"
    );
    assert!(
        body.contains("m_rs_init.result = result"),
        "sef_init_reply 必须经 m_rs_init.result 载荷回结果（C: m.m_rs_init.result）"
    );
    assert!(
        !body.contains("m_source ="),
        "sef_init_reply 不得手填 m_source——内核投递盖章是权威语义（T6 定谳，见 doc）"
    );
}
