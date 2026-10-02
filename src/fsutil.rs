//! Crash-atomic file writes (roast C5) and process-wide libgit2 durability.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Write `bytes` to `path` atomically: a temp file in `tmp_dir` (same filesystem as `path`), `sync_all`, rename
/// over `path`, then fsync `path`'s parent directory. A crash leaves either the old or the new content.
pub fn write_atomic_in(tmp_dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(tmp_dir)?;
    let name = path.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = tmp_dir.join(format!(".{name}.{}.{nanos}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        // Persist the rename itself; a directory that cannot be opened for sync is not a write failure.
        if let Ok(d) = std::fs::File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// `write_atomic_in` with the temp file next to `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    write_atomic_in(dir, path, bytes)
}

/// `dest`, or `dest.1`, `dest.2`, … : the first path that does not exist.
pub fn unique_path(dest: &Path) -> PathBuf {
    if std::fs::symlink_metadata(dest).is_err() {
        return dest.to_path_buf();
    }
    let mut n = 1u64;
    loop {
        let mut s = dest.as_os_str().to_owned();
        s.push(format!(".{n}"));
        let cand = PathBuf::from(s);
        if std::fs::symlink_metadata(&cand).is_err() {
            return cand;
        }
        n += 1;
    }
}

/// Make libgit2 fsync object and ref writes (GIT_OPT_ENABLE_FSYNC_GITDIR). Idempotent (a `Once`).
pub fn enable_git_fsync() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        libgit2_sys::init();
        // SAFETY: GIT_OPT_ENABLE_FSYNC_GITDIR takes one int argument; we pass exactly one c_int, after
        // libgit2 is initialised, and the option only flips a process-global flag.
        let rc = unsafe {
            libgit2_sys::git_libgit2_opts(libgit2_sys::GIT_OPT_ENABLE_FSYNC_GITDIR as std::os::raw::c_int, 1 as std::os::raw::c_int)
        };
        if rc != 0 {
            eprintln!("herdr-graph: could not enable libgit2 gitdir fsync (rc={rc})");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.txt");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two-longer").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two-longer");
        let names: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("a.txt")]);
    }

    #[test]
    fn write_atomic_in_uses_tmp_dir() {
        let d = tempfile::tempdir().unwrap();
        let tmp = d.path().join("t");
        let p = d.path().join("out.txt");
        write_atomic_in(&tmp, &p, b"x").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"x");
        assert!(tmp.is_dir(), "tmp dir was created");
        assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "temp renamed away");
        // a failing rename (destination is a non-empty directory) removes its temp
        let blocked = d.path().join("blocked");
        std::fs::create_dir_all(blocked.join("child")).unwrap();
        assert!(write_atomic_in(&tmp, &blocked, b"y").is_err());
        assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "temp removed on error");
    }

    #[test]
    fn unique_path_suffixes_on_collision() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.txt");
        assert_eq!(unique_path(&a), a);
        std::fs::write(&a, "1").unwrap();
        assert_eq!(unique_path(&a), d.path().join("a.txt.1"));
        std::fs::write(d.path().join("a.txt.1"), "2").unwrap();
        assert_eq!(unique_path(&a), d.path().join("a.txt.2"));
    }

    #[test]
    fn enable_git_fsync_is_idempotent() {
        enable_git_fsync();
        enable_git_fsync();
        let d = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(d.path()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "m", &tree, &[]).unwrap();
    }
}
