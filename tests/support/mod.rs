//! Shared support for tier-3 tests that run against a private Herdr server (spec §11).
//! Include with `mod support;` from an integration test.
#![allow(dead_code)]

pub mod isolated;
pub mod private_herdr;

use std::path::PathBuf;

/// First executable named `herdr` on `PATH`.
pub fn herdr_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HG_TEST_HERDR_BINARY") {
        return Some(validated_binary(PathBuf::from(path)));
    }
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join("herdr"))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Report a skipped test and return; the caller returns right after.
pub fn skip(reason: &str) {
    assert!(
        std::env::var("HG_HERDR_MATRIX").as_deref() != Ok("1"),
        "matrix qualification cannot skip: {reason}"
    );
    eprintln!("SKIPPED: {reason}");
}

/// Explicit fixture overrides must never silently fall back to another executable.
pub fn validated_binary(path: PathBuf) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    assert!(
        path.is_absolute(),
        "fixture binary must be absolute: {}",
        path.display()
    );
    let metadata = path
        .metadata()
        .unwrap_or_else(|e| panic!("invalid fixture binary {}: {e}", path.display()));
    assert!(
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        "fixture binary is not executable: {}",
        path.display()
    );
    path.canonicalize().expect("fixture binary canonicalizes")
}
