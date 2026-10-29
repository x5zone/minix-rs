//! TODO-04: the post-EBS boundary audit for the UEFI boot surface.
//!
//! After `ExitBootServices` succeeds, no Boot Services call, allocation or
//! free may occur — the three-zero discipline (design §1.5 (六).4). A full
//! call-graph check needs the pairing loop's final shape; this is the
//! text-level guard the work order allows to land first ("lint 脚本或文本级
//! 检查先行"): it slices the `prepare_boot` body from the source and asserts
//! that the statements after the `exit_boot_services()` call contain no
//! `uefi::`/`boot::` path, no allocator entry points and no destructor calls.
//!
//! Comments are stripped before scanning: the source legitimately names
//! `uefi::println!` in prose to explain why it is not used there.

use std::path::PathBuf;

/// Strip a line comment (`//`) and surrounding whitespace.
fn code_part(line: &str) -> &str {
    let line = match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    };
    line.trim()
}

/// Return the `prepare_boot` body (from its signature to the first closing
/// brace indented by at most four spaces — the real source indents it by
/// four, synthetic fixtures may use column zero).
fn prepare_boot_body(source: &str) -> &str {
    let sig = "fn prepare_boot(bump_pages: usize) -> BootPrepareResult {";
    let start = source
        .find(sig)
        .unwrap_or_else(|| panic!("TODO-04 audit: prepare_boot signature not found"));
    let rest = &source[start..];
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if line.trim() == "}" && indent <= 4 {
            let end = offset + indent;
            return &rest[..end];
        }
        offset += line.len();
    }
    panic!("TODO-04 audit: prepare_boot body end not found");
}

/// Offending calls after the EBS boundary: Boot Services paths plus the
/// allocation/free/drop entry points the crates expose.
fn violations_after_ebs(body: &str) -> Vec<String> {
    let ebs = body
        .rfind("exit_boot_services();")
        .unwrap_or_else(|| panic!("TODO-04 audit: EBS call not found in prepare_boot"));
    let after = &body[ebs + "exit_boot_services();".len()..];
    let banned = [
        "uefi::",
        "boot::",
        "allocate_pool",
        "allocate_pages",
        "free_pool",
        "free_pages",
        "Box::new",
        "Vec::",
        "alloc::",
        ".drop(",
        "drop(",
    ];
    let mut found = Vec::new();
    for (offset, raw) in after.lines().enumerate() {
        let code = code_part(raw);
        if code.is_empty() {
            continue;
        }
        for needle in banned {
            if code.contains(needle) {
                found.push(format!(
                    "line +{}: {needle} in `{code}`",
                    offset + 1
                ));
            }
        }
    }
    found
}

#[test]
fn prepare_boot_has_no_calls_after_exit_boot_services() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/uefi_helpers.rs");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("TODO-04 audit: cannot read {}: {err}", path.display()));
    let body = prepare_boot_body(&source);
    let found = violations_after_ebs(body);
    assert!(
        found.is_empty(),
        "TODO-04: post-EBS statements must be call-free (three-zero discipline); violations:\n{}",
        found.join("\n")
    );
}

#[test]
fn audit_detects_an_injected_violation() {
    // Negative control: the same scanner must flag a Boot Services call
    // appended after the boundary.
    let synthetic = "\
fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
    foo();
    exit_boot_services();
    uefi::println!(\"after EBS\");
}
";
    let body = prepare_boot_body(synthetic);
    let found = violations_after_ebs(body);
    assert!(
        found.iter().any(|v| v.contains("uefi::")),
        "negative control must flag the injected call: {found:?}"
    );
}

#[test]
fn comments_after_ebs_are_not_flagged() {
    // Prose may name the banned paths; only code is scanned.
    let synthetic = "\
fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
    exit_boot_services();
    // uefi::println! must not be used after EBS (explained here).
    raw_serial(\"ok\");
}
";
    assert!(violations_after_ebs(prepare_boot_body(synthetic)).is_empty());
}
