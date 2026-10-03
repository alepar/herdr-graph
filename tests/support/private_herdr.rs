//! `PrivateHerdr`: a throwaway `herdr server` whose HOME, XDG dirs, config, API socket, threads state and
//! Claude config all live under one fresh root. Rust port of herdr-threads' `private_host.py`.
//!
//! Safety rules: the child gets `env_clear()` plus [`scrubbed_env`] (no inherited `HERDR_*`), every path in
//! that env is checked by the [`IsolationGuard`] before anything starts, the server runs in its own process
//! group, and teardown signals only the pid this fixture spawned, after verifying its argv and environment.
//! `herdr server stop` is never used, and the user's default session is never contacted.
use herdr_graph::herdr::HerdrClient;
use herdr_graph::herdr::isolation::{IsolationGuard, PATH_VARS, scrubbed_env};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct PrivateHerdr {
    pub root: PathBuf,
    pub socket: PathBuf,
    pub env: Vec<(String, String)>,
    herdr: PathBuf,
    child: Child,
    pid: u32,
    guard: IsolationGuard,
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `pid` is a `herdr server` whose environment names `root` (macOS `ps eww` prints argv + environment).
fn is_private_server(pid: u32, root: &Path) -> bool {
    let p = pid.to_string();
    let command = output("/bin/ps", &["-o", "command=", "-p", &p]).unwrap_or_default();
    let env = output("/bin/ps", &["eww", "-o", "command=", "-p", &p]).unwrap_or_default();
    command.contains("herdr")
        && command.contains("server")
        && env.contains(root.to_string_lossy().as_ref())
}

fn spawn_server(herdr: &Path, root: &Path, env: &[(String, String)]) -> anyhow::Result<Child> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("herdr.log"))?;
    let child = Command::new(herdr)
        .arg("server")
        .env_clear()
        .envs(env.iter().cloned())
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()?;
    Ok(child)
}

fn wait_for_socket(child: &mut Child, socket: &Path) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("private herdr server exited early: {status}");
        }
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!(
        "private herdr server did not accept connections on {} within 15 s",
        socket.display()
    )
}

impl PrivateHerdr {
    /// root = /private/tmp/hg-<ulid lower>. Errors when `herdr` is not installed.
    pub fn start() -> anyhow::Result<Self> {
        let herdr =
            super::herdr_binary().ok_or_else(|| anyhow::anyhow!("herdr is not installed"))?;
        let root = PathBuf::from(format!(
            "/private/tmp/hg-{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ));
        for d in [
            "home",
            "config",
            "state",
            "data",
            "cache",
            "runtime",
            "threads-state",
            "claude",
            "plugin-config",
            "graph",
        ] {
            std::fs::create_dir_all(root.join(d))?;
        }
        std::fs::write(root.join("config/herdr.toml"), "")?;
        herdr_graph::herdr::isolation::arm();
        let env = scrubbed_env(&root);
        let guard = IsolationGuard::new(&root);
        for (k, v) in &env {
            if PATH_VARS.contains(&k.as_str()) {
                guard
                    .check(Path::new(v))
                    .map_err(|e| anyhow::anyhow!("isolation guard refused {k}: {e}"))?;
            }
        }
        let socket = PathBuf::from(
            &env.iter()
                .find(|(k, _)| k == "HERDR_SOCKET_PATH")
                .expect("socket in env")
                .1,
        );
        let mut child = spawn_server(&herdr, &root, &env)?;
        let pid = child.id();
        if let Err(e) = wait_for_socket(&mut child, &socket) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        Ok(Self {
            root,
            socket,
            env,
            herdr,
            child,
            pid,
            guard,
        })
    }

    pub fn client(&self) -> HerdrClient {
        HerdrClient::new(self.socket.clone())
    }

    /// Pid of the running server.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Stop the server this fixture started: verify identity, SIGTERM, wait up to 5 s.
    fn terminate(&mut self) -> anyhow::Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        anyhow::ensure!(
            is_private_server(self.pid, &self.root),
            "refusing to signal pid {}: not this fixture's private herdr server",
            self.pid
        );
        Command::new("/bin/kill")
            .args(["-TERM", &self.pid.to_string()])
            .status()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Still alive after SIGTERM: same pid, identity re-verified, last resort.
        anyhow::ensure!(
            is_private_server(self.pid, &self.root),
            "pid {} changed identity",
            self.pid
        );
        Command::new("/bin/kill")
            .args(["-KILL", &self.pid.to_string()])
            .status()?;
        self.child.wait()?;
        Ok(())
    }

    /// Restart in place (same root, config and socket) for restart spikes and tests.
    pub fn restart(&mut self) -> anyhow::Result<()> {
        self.terminate()?;
        let _ = std::fs::remove_file(&self.socket);
        let mut child = spawn_server(&self.herdr, &self.root, &self.env)?;
        wait_for_socket(&mut child, &self.socket)?;
        self.pid = child.id();
        self.child = child;
        Ok(())
    }

    /// A command running with the private env and nothing else (e.g. `herdr plugin link`).
    pub fn command(&self, program: &Path) -> Command {
        let mut c = Command::new(program);
        c.env_clear()
            .envs(self.env.iter().cloned())
            .current_dir(&self.root);
        c
    }

    /// The isolation guard for this root (tests may check extra paths).
    pub fn guard(&self) -> &IsolationGuard {
        &self.guard
    }

    pub fn herdr_path(&self) -> &Path {
        &self.herdr
    }
}

impl Drop for PrivateHerdr {
    fn drop(&mut self) {
        if let Err(e) = self.terminate() {
            eprintln!("PrivateHerdr teardown: {e}");
        }
        let _ = self.child.wait();
        // Pane shells, startup-hook daemons and anything else that inherited the marker.
        super::isolated::reap_root(&self.root);
        if std::env::var("HG_KEEP_PRIVATE_ROOT").as_deref() != Ok("1") {
            let _ = std::fs::remove_dir_all(&self.root);
        } else {
            eprintln!("kept private root {}", self.root.display());
        }
    }
}
