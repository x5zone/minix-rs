//! The post-EBS boundary audit for the UEFI boot surface.
//!
//! After `ExitBootServices` succeeds, no Boot Services call, allocation or
//! free may occur — the three-zero discipline (design §1.5 (6).4). A full
//! call-graph check needs the pairing loop's final shape; this is the
//! text-level guard that may land first (a lint script or text-level check
//! before the full one): it slices the `prepare_and_enter_kernel_image` body
//! from the source and asserts that the statements after the pairing-loop
//! success marker contain no `uefi::`/`boot::` path, no allocator entry points
//! and no destructor calls.
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

/// Return a function body (from its signature to the first closing brace
/// indented by at most four spaces — the real source indents it by four,
/// synthetic fixtures may use column zero).
fn function_body<'a>(source: &'a str, sig: &str) -> &'a str {
    let start = source
        .find(sig)
        .unwrap_or_else(|| panic!("TODO-04 audit: signature not found: {sig}"));
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
    panic!("TODO-04 audit: function body end not found: {sig}");
}

fn prepare_and_enter_body(source: &str) -> &str {
    function_body(
        source,
        "pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {",
    )
}

fn violations_after_marker(body: &str, marker: &str) -> Vec<String> {
    let at = body
        .rfind(marker)
        .unwrap_or_else(|| panic!("TODO-04 audit: marker not found: {marker}"));
    let after = &body[at + marker.len()..];
    let banned = [
        "uefi::",
        "uefi::boot::",
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
                found.push(format!("line +{}: {needle} in `{code}`", offset + 1));
            }
        }
    }
    found
}

#[test]
fn prepare_and_enter_has_no_calls_after_the_pairing_loop() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/uefi_helpers.rs");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("TODO-04 audit: cannot read {}: {err}", path.display()));
    let body = prepare_and_enter_body(&source);
    let found = violations_after_marker(body, "boot services exited (paired)");
    assert!(
        found.is_empty(),
        "TODO-04: post-pairing statements must be call-free (three-zero discipline); violations:\n{}",
        found.join("\n")
    );
}

#[test]
fn pairing_audit_detects_an_injected_violation() {
    let synthetic = "\
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    foo();
    raw_serial(\"boot services exited (paired)\");
    uefi::println!(\"after EBS\");
}
";
    let body = prepare_and_enter_body(synthetic);
    let found = violations_after_marker(body, "boot services exited (paired)");
    assert!(
        found.iter().any(|v| v.contains("uefi::")),
        "negative control must flag the injected call: {found:?}"
    );
}
