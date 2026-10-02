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

/// Spec §1: no instance → NoInstance (caller exits 0). Socket answers hello → AlreadyRunning.
/// Else spawn detached (setsid, log to .graph-local/daemon.log) and poll hello every 100 ms up to `timeout`.
pub fn ensure(env: &Env, timeout: Duration) -> anyhow::Result<EnsureOutcome> {
    use anyhow::Context;
    let Some((root, _)) = locate_instance(env, &plugin_config_dir_via_herdr) else {
        return Ok(EnsureOutcome::NoInstance);
    };
    let paths = InstancePaths::new(&root);
    if let Some(reply) = hello(&paths.socket) {
        return Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) });
    }
    if env.herdr_socket.is_none() {
        anyhow::bail!("daemon unavailable: not inside Herdr");
    }
    std::fs::create_dir_all(&paths.local)?;
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
        if let Some(reply) = hello(&paths.socket) {
            return Ok(EnsureOutcome::Started { pid: pid_of(&reply) });
        }
        if let Some(status) = child.try_wait()? {
            // The child exited without serving. A concurrent ensure may have won the lock instead.
            if let Some(reply) = hello(&paths.socket) {
                return Ok(EnsureOutcome::AlreadyRunning { pid: pid_of(&reply) });
            }
            anyhow::bail!("daemon exited ({status}); see {}", paths.log.display());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("daemon did not answer within {}s; see {}", timeout.as_secs(), paths.log.display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
