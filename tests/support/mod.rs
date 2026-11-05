//! Shared support for tier-3 tests that run against a private Herdr server (spec §11).
//! Include with `mod support;` from an integration test.
#![allow(dead_code)]

pub mod isolated;
pub mod private_herdr;

use std::path::PathBuf;

/// Integration-test twin of `plan::managers::testing::opt_out` (that one is `#[cfg(test)]`, so integration
/// targets cannot call it): commit a graph.toml whose `node_manager_seat` names a seat that does not exist,
/// before any writer or daemon opens `root`. First-seat assignment never fires and `live_node_manager` is None.
pub fn node_manager_opt_out(root: &std::path::Path) {
    let path = root.join("graph.toml");
    let mut g: herdr_graph::model::graph::GraphRecord =
        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    g.node_manager_seat = Some(herdr_graph::model::SeatId::new());
    std::fs::write(&path, toml::to_string(&g).unwrap()).unwrap();
    let repo = git2::Repository::open(root).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("graph.toml")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = herdr_graph::store::git::graph_signature().unwrap();
    repo.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "test: node manager opt-out",
        &tree,
        &[&parent],
    )
    .unwrap();
}

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
