//! Idempotent `daemon --ensure` (spec §1): spawn a detached daemon unless one already answers.
use super::client::hello;
use crate::config::{Env, InstancePaths, locate_instance, plugin_config_dir_via_herdr};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum EnsureOutcome {
    NoInstance,
    AlreadyRunning { pid: u32 },
    Started { pid: u32 },
}

fn pid_of(reply: &serde_json::Value) -> u32 {
    reply["pid"].as_u64().unwrap_or(0) as u32
}

/// How long ensure waits for a daemon to answer: the daemon's first observer pass plus margin (bead hg-zmi.55).
pub const STARTUP_WAIT: Duration = Duration::from_secs(super::compose::FIRST_PASS_LIMIT.as_secs() + 10);

/// Advisory flock on `.graph-local/ensure.lock`, held from before the spawn until the daemon holds its own lock.
struct SpawnGuard(#[allow(dead_code)] std::fs::File);

impl SpawnGuard {
    /// None when the file cannot be opened or the lock stays contended until `deadline`; the caller proceeds anyway,
    /// since the daemon lock still guarantees a single daemon.
    fn acquire(path: &std::path::Path, deadline: Instant) -> Option<SpawnGuard> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
        loop {
            // SAFETY: fd is a valid open file descriptor owned by `file`.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Some(SpawnGuard(file));
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn poll_hello(paths: &InstancePaths, deadline: Instant) -> Option<serde_json::Value> {
    loop {
        if let Some(reply) = hello(&paths.socket) {
            return Some(reply);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Spec §1: no instance → NoInstance (caller exits 0). Socket answers hello (any state, including "starting")
/// → AlreadyRunning. A lock held by a live daemon that does not answer yet means it is still binding or composing:
/// wait for it, never spawn a second one. Else spawn detached (setsid, log to .graph-local/daemon.log) and poll
/// hello every 100 ms up to `timeout`.
pub fn ensure(env: &Env, timeout: Duration) -> anyhow::Result<EnsureOutcome> {
    use anyhow::Context;
    let Some((root, _)) = locate_instance(env, &plugin_config_dir_via_herdr) else {
        return Ok(EnsureOutcome::NoInstance);
    };
    let paths = InstancePaths::new(&root);
    if let Some(reply) = hello(&paths.socket) {
        return Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) });
    }
    let wait_for_holder = |paths: &InstancePaths| -> anyhow::Result<EnsureOutcome> {
        let pid = super::lock::read_info(&paths.lock).map(|i| i.pid).unwrap_or(0);
        match poll_hello(paths, Instant::now() + timeout) {
            Some(reply) => Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) }),
            None => anyhow::bail!(
                "daemon (pid {pid} from the lock) did not answer within {}s; see {}",
                timeout.as_secs(),
                paths.log.display()
            ),
        }
    };
    if super::lock::is_held(&paths.lock) {
        return wait_for_holder(&paths);
    }
    if env.herdr_socket.is_none() {
        anyhow::bail!("daemon unavailable: not inside Herdr");
    }
    std::fs::create_dir_all(&paths.local)?;
    // Serialize cold starts: two ensures must not both spawn before the first daemon has taken the lock.
    let mut guard = SpawnGuard::acquire(&paths.local.join("ensure.lock"), Instant::now() + timeout);
    if let Some(reply) = hello(&paths.socket) {
        return Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) });
    }
    if super::lock::is_held(&paths.lock) {
        return wait_for_holder(&paths);
    }
    let log = std::fs::OpenOptions::new().create(true).append(true).open(&paths.log)?;
    let mut cmd = Command::new(std::env::current_exe().context("locating herdr-graph executable")?);
    cmd.arg("daemon")
        .env(crate::config::ENV_INSTANCE, &root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: setsid is async-signal-safe and touches no Rust state; it detaches the child from our session
    // and controlling terminal, which is what the spec's double-fork achieves.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().context("spawning daemon")?;
    let deadline = Instant::now() + timeout;
    loop {
        if guard.is_some() && super::lock::is_held(&paths.lock) {
            guard = None; // the daemon owns the instance now; later ensures wait for it instead of spawning
        }
        if let Some(reply) = hello(&paths.socket) {
            return Ok(EnsureOutcome::Started { pid: pid_of(&reply) });
        }
        if let Some(status) = child.try_wait()? {
            // The child exited without serving. A concurrent ensure may have won the lock instead.
            if let Some(reply) = hello(&paths.socket) {
                return Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) });
            }
            if super::lock::is_held(&paths.lock) {
                // The winner is still starting; wait out the remaining budget instead of reporting a failure.
                return match poll_hello(&paths, deadline) {
                    Some(reply) => Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) }),
                    None => anyhow::bail!(
                        "daemon holding the lock did not answer within {}s; see {}",
                        timeout.as_secs(),
                        paths.log.display()
                    ),
                };
            }
            anyhow::bail!("daemon exited ({status}); see {}", paths.log.display());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("daemon did not answer within {}s; see {}", timeout.as_secs(), paths.log.display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
