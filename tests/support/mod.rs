//! Shared support for tier-3 tests that run against a private Herdr server (spec §11).
//! Include with `mod support;` from an integration test.
#![allow(dead_code)]

pub mod isolated;
pub mod private_herdr;

use std::path::PathBuf;

/// First executable named `herdr` on `PATH`.
pub fn herdr_binary() -> Option<PathBuf> {
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
    eprintln!("SKIPPED: {reason}");
}
