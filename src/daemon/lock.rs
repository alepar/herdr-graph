//! Per-instance daemon flock (`<instance>/.graph-local/daemon.lock`); the lock file also records who holds it.
use crate::model::Timestamp;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub herdr_socket: PathBuf,
    pub socket: PathBuf,
    pub started_at: Timestamp,
}

/// Holds the flock for as long as it lives (released on drop or process exit).
pub struct DaemonLock {
    _file: std::fs::File,
}

impl DaemonLock {
    /// flock(LOCK_EX | LOCK_NB). Ok(None) when another process holds it. On success truncates and writes LockInfo JSON.
    pub fn try_acquire(path: &Path, info: &LockInfo) -> std::io::Result<Option<DaemonLock>> {
        let mut file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).read(true).open(path)?;
        // SAFETY: fd is a valid open file descriptor owned by `file`.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            return if err.kind() == std::io::ErrorKind::WouldBlock { Ok(None) } else { Err(err) };
        }
        file.set_len(0)?;
        file.write_all(&serde_json::to_vec(info).map_err(std::io::Error::other)?)?;
        file.flush()?;
        Ok(Some(DaemonLock { _file: file }))
    }
}

/// True iff another open file description currently holds the flock. Never truncates or writes the file.
pub fn is_held(path: &Path) -> bool {
    let Ok(file) = std::fs::OpenOptions::new().read(true).open(path) else {
        return false;
    };
    // SAFETY: fd is a valid open file descriptor owned by `file`.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock;
    }
    // SAFETY: as above; releases the probe lock we just took.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    false
}

pub fn read_info(path: &Path) -> Option<LockInfo> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pid: u32) -> LockInfo {
        LockInfo {
            pid,
            herdr_socket: "/h.sock".into(),
            socket: "/d.sock".into(),
            started_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn second_acquire_fails_while_held() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("daemon.lock");
        let first = DaemonLock::try_acquire(&p, &info(1)).unwrap().expect("first acquire");
        // A second open file description conflicts even within one process.
        assert!(DaemonLock::try_acquire(&p, &info(2)).unwrap().is_none());
        // The loser must not have clobbered the holder's info.
        assert_eq!(read_info(&p).unwrap().pid, 1);
        drop(first);
        assert!(DaemonLock::try_acquire(&p, &info(3)).unwrap().is_some());
        assert_eq!(read_info(&p).unwrap().pid, 3);
    }

    #[test]
    fn is_held_reports_holder() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("daemon.lock");
        assert!(!is_held(&p), "no file means not held");
        let l = DaemonLock::try_acquire(&p, &info(1)).unwrap().unwrap();
        assert!(is_held(&p));
        assert_eq!(read_info(&p).unwrap().pid, 1, "probe must not clobber the holder's info");
        drop(l);
        assert!(!is_held(&p));
    }

    #[test]
    fn info_roundtrip() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("daemon.lock");
        let _l = DaemonLock::try_acquire(&p, &info(42)).unwrap().unwrap();
        assert_eq!(read_info(&p), Some(info(42)));
        assert_eq!(read_info(&t.path().join("missing")), None);
    }
}
