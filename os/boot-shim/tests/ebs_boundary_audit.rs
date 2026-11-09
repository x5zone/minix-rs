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

/// Return a function body from its signature through the matching closing
/// brace. Brace counting (not "first `}` at indent ≤ 4") is required because
/// the function contains nested loops/blocks; the old indent heuristic ended
/// the slice at the pairing loop's closing brace and left the post-EBS tail
/// unscanned.
fn function_body<'a>(source: &'a str, sig: &str) -> &'a str {
    let start = source
        .find(sig)
        .unwrap_or_else(|| panic!("TODO-04 audit: signature not found: {sig}"));
    let rest = &source[start..];
    let open = rest
        .find('{')
        .unwrap_or_else(|| panic!("TODO-04 audit: opening brace not found: {sig}"));
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_line = false;
    let mut in_str = false;
    let mut in_char = false;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_line {
            if c == b'\n' {
                in_line = false;
            }
            i += 1;
            continue;
        }
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if in_char {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                in_line = true;
                i += 2;
                continue;
            }
            b'"' => in_str = true,
            b'\'' => in_char = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..=i];
                }
            }
            _ => {}
        }
        i += 1;
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

#[test]
fn pairing_audit_detects_an_allocate_pages_violation() {
    let synthetic = "\
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    raw_serial(\"boot services exited (paired)\");
    let _ = boot::allocate_pages(uefi::AllocateType::AnyPages, uefi::MemoryType::LOADER_DATA, 1);
}
";
    let body = prepare_and_enter_body(synthetic);
    let found = violations_after_marker(body, "boot services exited (paired)");
    assert!(
        found.iter().any(|v| v.contains("allocate_pages")),
        "negative control must flag the injected allocation: {found:?}"
    );
}

#[test]
fn pairing_audit_scans_past_nested_blocks() {
    let synthetic = "\
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    for _ in 0..1 {
        foo();
    }
    raw_serial(\"boot services exited (paired)\");
    uefi::println!(\"tail after nested block\");
}
";
    let body = prepare_and_enter_body(synthetic);
    let found = violations_after_marker(body, "boot services exited (paired)");
    assert!(
        found.iter().any(|v| v.contains("uefi::")),
        "tail after a nested block must be scanned: {found:?}"
    );
}

#[test]
fn comment_only_mentions_are_exempt() {
    let synthetic = "\
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    raw_serial(\"boot services exited (paired)\");
    // uefi::println! would be a violation here; the comment is prose.
    park();
}
";
    let body = prepare_and_enter_body(synthetic);
    let found = violations_after_marker(body, "boot services exited (paired)");
    assert!(
        found.is_empty(),
        "comments naming a banned path must not count as calls: {found:?}"
    );
}
