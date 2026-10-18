//! Crash-recoverable namespace boundary. Git is authoritative; local bindings are published afterwards.
use crate::{
    ports::store::{RepoPath, StoreError},
    store::tree::TreeRead,
};
use herdr_threads::node::NodeUuid;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const MARKER: &str = "threads-namespace.json";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub version: u32,
    pub node: NodeUuid,
    pub epoch: String,
    pub from: String,
    pub previous_epoch: String,
    pub base_head: String,
    pub op: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u32,
    node: NodeUuid,
}

pub fn binding_path(intents: &Path) -> PathBuf {
    intents.with_extension("node.json")
}
pub fn default_intents(root: &Path) -> PathBuf {
    root.join(".graph-local/threads-intents")
}
fn recovery(detail: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!(
        "threads namespace recovery required: {detail}; graph references and intents preserved. Restore the matching original binding/store, or review `herdr-graph plan threads reconnect --from <node|legacy> --to <node> --head <current-main>` then apply that plan. Unbound legacy graphs always require reconnect; do not delete bindings or intents"
    ))
}
pub fn read(tree: &dyn TreeRead) -> Result<Option<Transition>, StoreError> {
    tree.read_file(&RepoPath::new(MARKER)?)?
        .map(|b| decode(&b).map_err(StoreError::Io))
        .transpose()
}
fn decode(bytes: &[u8]) -> io::Result<Transition> {
    let t: Transition = serde_json::from_slice(bytes).map_err(recovery)?;
    if t.version != 1
        || t.epoch.is_empty()
        || !t
            .epoch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(recovery("invalid committed namespace marker"));
    }
    Ok(t)
}
pub fn epoch(tree: &dyn TreeRead) -> Result<String, StoreError> {
    Ok(read(tree)?.map(|t| t.epoch).unwrap_or_default())
}
pub fn committed(root: &Path) -> io::Result<Option<Transition>> {
    let repo = git2::Repository::open(root).map_err(recovery)?;
    let tree = repo
        .find_reference("refs/heads/main")
        .and_then(|r| r.peel_to_tree())
        .map_err(recovery)?;
    match tree.get_path(Path::new(MARKER)) {
        Ok(e) => decode(repo.find_blob(e.id()).map_err(recovery)?.content()).map(Some),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(recovery(e)),
    }
}
pub fn bound(intents: &Path) -> io::Result<Option<NodeUuid>> {
    match std::fs::read(binding_path(intents)) {
        Ok(b) => {
            let b: Binding = serde_json::from_slice(&b).map_err(recovery)?;
            if b.version != 1 {
                return Err(recovery("invalid binding version"));
            }
            Ok(Some(b.node))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
/// File locks work across daemon processes; callers doing async work acquire via spawn_blocking.
pub fn lock(root: &Path, name: &str, exclusive: bool) -> io::Result<std::fs::File> {
    lock_file(&root.join(".graph-local").join(name), exclusive)
}
fn lock_file(path: &Path, exclusive: bool) -> io::Result<std::fs::File> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let dir = path
        .parent()
        .ok_or_else(|| recovery("lock path has no parent"))?;
    std::fs::create_dir_all(&dir)?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    if unsafe {
        libc::flock(
            f.as_raw_fd(),
            if exclusive {
                libc::LOCK_EX
            } else {
                libc::LOCK_SH
            },
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(f)
}
pub async fn call_guard(root: &Path) -> io::Result<std::fs::File> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || lock(&root, "threads-calls.lock", false))
        .await
        .map_err(io::Error::other)?
}
/// Shared production call boundary; the guard must cover the external call only.
pub async fn guard(root: &Path) -> io::Result<std::fs::File> {
    let guard = call_guard(root).await?;
    if root.join(".git").exists() {
        check_call(root)?;
    }
    Ok(guard)
}
tokio::task_local! { pub static EXPECTED_EPOCH: String; }
pub fn check_call(root: &Path) -> io::Result<()> {
    let current = committed(root)?.map(|t| t.epoch).unwrap_or_default();
    let expected = EXPECTED_EPOCH.try_with(Clone::clone).unwrap_or_default();
    if current != expected {
        return Err(recovery(
            "stale external call snapshot; retry from the current committed namespace",
        ));
    }
    Ok(())
}
pub fn stamp(args: &mut serde_json::Value, epoch: &str) {
    args["threads_epoch"] = epoch.into();
}
pub fn scoped_stamp(args: &mut serde_json::Value) {
    stamp(
        args,
        &EXPECTED_EPOCH.try_with(Clone::clone).unwrap_or_default(),
    );
}
pub fn salt(epoch: &str, key: &str) -> String {
    if epoch.is_empty() {
        key.into()
    } else {
        format!("{epoch}:{key}")
    }
}
pub fn intent_dir(intents: &Path, root: &Path) -> io::Result<PathBuf> {
    Ok(match committed(root)? {
        None => intents.to_owned(),
        Some(t) => intents.with_file_name(format!("threads-intents-{}", t.epoch)),
    })
}
fn publish(path: &Path, value: &impl Serialize) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path
        .parent()
        .ok_or_else(|| recovery("binding has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".threads-node-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        serde_json::to_writer(&mut f, value)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        std::fs::rename(&temp, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    let _ = std::fs::remove_file(temp);
    result
}
/// A retired directory is dormant even for direct low-level clients. The seal is outside
/// the journal so every original intent byte remains available for explicit recovery.
pub fn retired_path(intents: &Path) -> PathBuf {
    intents.with_extension("retired.json")
}
fn seal_previous(intents: &Path, transition: &Transition) -> io::Result<()> {
    let previous = if transition.previous_epoch.is_empty() {
        intents.to_owned()
    } else {
        intents.with_file_name(format!("threads-intents-{}", transition.previous_epoch))
    };
    let path = retired_path(&previous);
    match std::fs::read(&path) {
        Ok(bytes) => {
            if decode(&bytes)? != *transition {
                return Err(recovery(
                    "retired journal seal conflicts with committed transition",
                ));
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => publish(&path, transition),
        Err(e) => Err(e),
    }
}
/// Namespace publication and every writer commit share this lock. Historical records count as populated.
pub fn ensure(intents: &Path, node: NodeUuid, graph_root: Option<&Path>) -> io::Result<()> {
    let _guard = lock_file(
        &intents
            .parent()
            .unwrap_or(Path::new("."))
            .join("threads-binding.lock"),
        true,
    )?;
    let saved = bound(intents)?;
    if let Some(root) = graph_root
        && root.join(".git").exists()
        && let Some(t) = committed(root)?
    {
        if node != t.node {
            return Err(recovery(format!(
                "committed node {}, discovered {node}",
                t.node
            )));
        }
        if saved != Some(node) {
            let old = saved
                .map(|n| n.to_string())
                .unwrap_or_else(|| "legacy".into());
            if old != t.from {
                return Err(recovery(format!(
                    "transition expects previous binding {}, found {old}",
                    t.from
                )));
            }
        }
        // Seal first: a crash must never expose the old journal under the target binding.
        // The committed marker authorizes retry after the seal but before publication.
        seal_previous(intents, &t)?;
        if saved != Some(node) {
            publish(&binding_path(intents), &Binding { version: 1, node })?;
        }
        return Ok(());
    }
    match std::fs::symlink_metadata(retired_path(intents)) {
        Ok(_) => {
            return Err(recovery(
                "intent journal is sealed by a committed reconnect",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if let Some(old) = saved {
        return if old == node {
            Ok(())
        } else {
            Err(recovery(format!("binding node {old}, discovered {node}")))
        };
    }
    if match std::fs::read_dir(intents) {
        Ok(mut d) => d.next().transpose()?.is_some(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    } {
        return Err(recovery("unbound legacy intent journal is not empty"));
    }
    if let Some(root) = graph_root {
        if root.join(".git").exists() {
            let repo = git2::Repository::open(root).map_err(recovery)?;
            let mut walk = repo.revwalk().map_err(recovery)?;
            walk.push_ref("refs/heads/main").map_err(recovery)?;
            for oid in walk {
                let tree = repo
                    .find_commit(oid.map_err(recovery)?)
                    .and_then(|c| c.tree())
                    .map_err(recovery)?;
                let mut populated = false;
                tree.walk(git2::TreeWalkMode::PreOrder, |dir, e| {
                    if e.kind() == Some(git2::ObjectType::Blob)
                        && e.name().is_some_and(|n| {
                            n.ends_with(".toml") && !(dir.is_empty() && n == "graph.toml")
                        })
                    {
                        populated = true;
                    }
                    git2::TreeWalkResult::Ok
                })
                .map_err(recovery)?;
                if populated {
                    return Err(recovery(
                        "unbound graph has committed records in current state or history",
                    ));
                }
            }
        } else if root.join("graph.toml").exists() {
            return Err(recovery("graph repository unavailable"));
        }
    }
    publish(&binding_path(intents), &Binding { version: 1, node })
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
    fn empty_current_tree_with_historical_refs_requires_reconnect() {
        let dir = tempfile::tempdir().unwrap();
        crate::store::init::init_instance(dir.path()).unwrap();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let first = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = git2::Signature::now("test", "test@example.invalid").unwrap();
        let mut builder = repo.treebuilder(Some(&first.tree().unwrap())).unwrap();
        builder
            .insert("old.toml", repo.blob(b"thread_id='old'").unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let old = repo
            .commit(Some("HEAD"), &sig, &sig, "old", &tree, &[&first])
            .unwrap();
        let old = repo.find_commit(old).unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "empty again",
            &first.tree().unwrap(),
            &[&old],
        )
        .unwrap();
        let intents = dir.path().join(".graph-local/threads-intents");
        assert!(ensure(&intents, NodeUuid::from_bytes([2; 16]), Some(dir.path())).is_err());
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
