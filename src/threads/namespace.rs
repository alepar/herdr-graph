//! Persistent fence between graph references/intents and a threads node.
//! Never resets a namespace: an operator recovery must preserve and reconcile old state first.
use herdr_threads::node::NodeUuid;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u32,
    node: NodeUuid,
}

/// Sibling of the intent directory, shared by the lazy service and pane resolver.
pub fn binding_path(intents: &Path) -> PathBuf {
    intents.with_extension("node.json")
}

fn recovery(detail: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!(
        "threads namespace recovery required: {detail}; graph references, pending work and intents were preserved. Restore the original threads node/store, or use the reviewed graph reconnect recovery procedure; do not delete the binding or intents"
    ))
}

/// Bind only an empty journal (and, when provided, an unused graph), or verify the exact prior node.
/// `graph_root` is provided by production before either service registration or pane resolution.
/// Publishing uses a synced temporary file and no-clobber hard link, so racing first callers agree.
pub fn ensure(intents: &Path, node: NodeUuid, graph_root: Option<&Path>) -> io::Result<()> {
    let path = binding_path(intents);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let saved: Binding = serde_json::from_slice(&bytes).map_err(recovery)?;
            return if saved.version == 1 && saved.node == node {
                Ok(())
            } else {
                Err(recovery(format!(
                    "{} binds node {}, discovered {node}",
                    path.display(),
                    saved.node
                )))
            };
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if match std::fs::read_dir(intents) {
        Ok(mut entries) => entries.next().transpose()?.is_some(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    } {
        return Err(recovery("unbound legacy intent journal is not empty"));
    }
    if let Some(root) = graph_root {
        // Read the authoritative committed tree, never infer safety from a stale checkout.
        // A graph with any organizational/history records requires explicit assessment even
        // when the old client had already forgotten all completed service envelopes.
        if root.join(".git").exists() {
            let repo = git2::Repository::open(root).map_err(recovery)?;
            let tree = repo
                .find_reference("refs/heads/main")
                .and_then(|r| r.peel_to_tree())
                .map_err(recovery)?;
            let mut populated = false;
            tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob)
                    && entry.name().is_some_and(|name| {
                        name.ends_with(".toml") && !(dir.is_empty() && name == "graph.toml")
                    })
                {
                    populated = true;
                }
                git2::TreeWalkResult::Ok
            })
            .map_err(recovery)?;
            if populated {
                return Err(recovery(
                    "unbound graph has committed records and may contain old threads references",
                ));
            }
        } else if root.join("graph.toml").exists() {
            return Err(recovery(
                "graph repository is unavailable; cannot verify legacy references",
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("binding has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".threads-node-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        serde_json::to_writer(&mut file, &Binding { version: 1, node })?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        match std::fs::hard_link(&temp, &path) {
            Ok(()) => std::fs::File::open(parent)?.sync_all(),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => ensure(intents, node, graph_root),
            Err(e) => Err(e),
        }
    })();
    let _ = std::fs::remove_file(temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_binds_but_legacy_graph_and_pending_artifacts_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("graph");
        crate::store::init::init_instance(&root).unwrap();
        let intents = root.join(".graph-local/threads-intents");
        let node = NodeUuid::from_bytes([1; 16]);
        ensure(&intents, node, Some(&root)).unwrap();
        let binding = std::fs::read(binding_path(&intents)).unwrap();
        assert!(ensure(&intents, NodeUuid::from_bytes([2; 16]), Some(&root)).is_err());
        assert_eq!(std::fs::read(binding_path(&intents)).unwrap(), binding);

        // A historical graph might have no remaining service intent files.
        std::fs::remove_file(binding_path(&intents)).unwrap();
        let repo = git2::Repository::open(&root).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let blob = repo.blob(b"thread_id = 'old-channel'\n").unwrap();
        let mut tree = repo.treebuilder(Some(&parent.tree().unwrap())).unwrap();
        tree.insert("historical-record.toml", blob, 0o100644)
            .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let signature = git2::Signature::now("test", "test@example.invalid").unwrap();
        let head = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "old refs",
                &tree,
                &[&parent],
            )
            .unwrap();
        assert!(
            ensure(&intents, node, Some(&root))
                .unwrap_err()
                .to_string()
                .contains("committed records")
        );
        assert_eq!(repo.head().unwrap().target(), Some(head));
        assert_eq!(
            repo.find_blob(blob).unwrap().content(),
            b"thread_id = 'old-channel'\n"
        );
        assert!(!binding_path(&intents).exists());

        std::fs::create_dir_all(&intents).unwrap();
        let pending = intents.join("legacy-pending.json");
        std::fs::write(&pending, b"unresolved old envelope").unwrap();
        assert!(ensure(&intents, node, None).is_err());
        assert_eq!(std::fs::read(pending).unwrap(), b"unresolved old envelope");
        assert!(!binding_path(&intents).exists());
    }

    #[test]
    fn corrupt_binding_is_not_replaced_and_racing_nodes_cannot_both_bind() {
        let dir = tempfile::tempdir().unwrap();
        let intents = dir.path().join("intents");
        std::fs::write(binding_path(&intents), "invalid").unwrap();
        assert!(ensure(&intents, NodeUuid::from_bytes([1; 16]), None).is_err());
        assert_eq!(std::fs::read(binding_path(&intents)).unwrap(), b"invalid");
        std::fs::remove_file(binding_path(&intents)).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = [1, 2]
            .into_iter()
            .map(|n| {
                let intents = intents.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    ensure(&intents, NodeUuid::from_bytes([n; 16]), None)
                })
            })
            .collect();
        let successes = handles
            .into_iter()
            .map(|h| h.join().unwrap().is_ok())
            .filter(|ok| *ok)
            .count();
        assert_eq!(successes, 1);
    }
}
