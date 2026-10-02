//! Name → directory-slug rules (spec §2.1).
use std::collections::BTreeSet;

pub const MAX_SLUG: usize = 48;

/// Spec §2.1: lowercase, [a-z0-9-], others → '-', runs collapsed, trimmed, max 48; empty → "x".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c)
        } else if !out.ends_with('-') {
            out.push('-')
        }
    }
    let mut s = out.trim_matches('-').to_string();
    if s.len() > MAX_SLUG {
        s.truncate(MAX_SLUG);
        s = s.trim_end_matches('-').to_string();
    }
    if s.is_empty() { "x".into() } else { s }
}

/// Collision within the parent → append `-<last 6 of id, lowercased>`.
pub fn unique_slug(name: &str, id_suffix6: &str, taken: &BTreeSet<String>) -> String {
    let base = slugify(name);
    if !taken.contains(&base) {
        base
    } else {
        format!("{base}-{}", id_suffix6.to_ascii_lowercase())
    }
}
