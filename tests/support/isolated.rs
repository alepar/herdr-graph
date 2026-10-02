//! `TestRoot`: the isolation fixture for every test that spawns a process (hg-zmi.77).
//!
//! One short root under `/private/tmp` holds HOME, the XDG dirs, the Claude config, the Herdr plugin dirs,
//! the Herdr socket and the graph instance. Children get `env_clear()` plus [`scrubbed_env`], which also
//! sets `HG_TEST_ROOT=<root>`: the library tripwire refuses (exit 97) any path a child resolves outside
//! it, and `Drop` sweeps every process still carrying the marker, so a setsid daemon or a grandchild does
//! not outlive the test, whether it passed, failed, panicked or timed out.
#![allow(dead_code)]
use herdr_graph::daemon::client::Client;
use herdr_graph::daemon::lock;
use herdr_graph::herdr::isolation::{self, IsolationGuard, PATH_VARS, TEST_ROOT_VAR, VIOLATIONS_LOG, scrubbed_env};
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Roots this fixture family creates (`PrivateHerdr` uses `hg-`, `TestRoot` uses `hgt-`).
const ROOT_PREFIXES: &[&str] = &["/private/tmp/hgt-", "/private/tmp/hg-"];

pub struct TestRoot {
    root: PathBuf,
}

impl TestRoot {
    /// A fresh root `/private/tmp/hgt-<ulid lowercase>` (short: the `sockaddr_un` limit).
    pub fn new() -> Self {
        reap_orphans();
        let root = PathBuf::from(format!("/private/tmp/hgt-{}", ulid::Ulid::new().to_string().to_lowercase()));
        for d in [
            "home", "config", "state", "data", "cache", "runtime", "threads-state", "claude", "plugin-config", "graph",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("config/herdr.toml"), "").unwrap();
        let guard = IsolationGuard::new(&root);
        for (k, v) in scrubbed_env(&root) {
            if PATH_VARS.contains(&k.as_str()) {
                guard.check(Path::new(&v)).unwrap_or_else(|e| panic!("isolation guard refused {k}: {e}"));
            }
        }
        isolation::arm();
        TestRoot { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// `HERDR_GRAPH_INSTANCE` in [`TestRoot::env`].
    pub fn instance(&self) -> PathBuf {
        self.root.join("graph")
    }

    /// `HERDR_SOCKET_PATH` in [`TestRoot::env`].
    pub fn herdr_socket(&self) -> PathBuf {
        self.root.join("herdr.sock")
    }

    pub fn claude_dir(&self) -> PathBuf {
        self.root.join("claude")
    }

    pub fn env(&self) -> Vec<(String, String)> {
        scrubbed_env(&self.root)
    }

    /// `env_clear()` + [`TestRoot::env`] + `current_dir(root)`: the only way a test builds a child command.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut c = Command::new(program);
        c.env_clear().envs(self.env()).current_dir(&self.root);
        c
    }

    /// Spawn into its own process group; the guard signals and reaps the group on drop.
    pub fn spawn(&self, cmd: &mut Command) -> ChildGuard {
        cmd.process_group(0);
        let child = cmd.spawn().unwrap_or_else(|e| panic!("spawn {:?}: {e}", cmd.get_program()));
        let pgid = child.id() as i32;
        ChildGuard { child: Some(child), pgid, reaped: false }
    }

    /// Contents of `<root>/isolation-violations.log` (empty when nothing tripped).
    pub fn violations(&self) -> String {
        std::fs::read_to_string(self.root.join(VIOLATIONS_LOG)).unwrap_or_default()
    }

    /// Every daemon lock under the root (instances may sit in sub-directories), best effort.
    fn daemon_sockets(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
            if depth == 0 {
                return;
            }
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name().is_some_and(|n| n == "daemon.lock") {
                    if let Some(info) = lock::read_info(&p) {
                        out.push(info.socket);
                    }
                } else if e.file_type().is_ok_and(|t| t.is_dir()) {
                    walk(&p, depth - 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, 6, &mut out);
        out
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        // 1. Ask every instance daemon to shut down gracefully.
        for sock in self.daemon_sockets() {
            if let Ok(mut c) = Client::connect(&sock, Duration::from_secs(2)) {
                let _ = c.call("shutdown", serde_json::json!({}));
            }
        }
        // 2. Sweep everything that still carries the marker (setsid daemon, grandchildren, pane shells).
        reap_root(&self.root);
        // 3. + 4. Remember violations, remove the root, then fail the test if any were logged.
        let violations = self.violations();
        if std::env::var("HG_KEEP_PRIVATE_ROOT").as_deref() == Ok("1") {
            eprintln!("kept test root {}", self.root.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
        if !violations.trim().is_empty() {
            let msg = format!("isolation violations under {}:\n{violations}", self.root.display());
            if std::thread::panicking() {
                eprintln!("{msg}");
            } else {
                panic!("{msg}");
            }
        }
    }
}

/// A spawned child in its own process group. Dropping signals the group (SIGTERM, then SIGKILL after 3 s)
/// and reaps the leader.
pub struct ChildGuard {
    child: Option<Child>,
    pgid: i32,
    reaped: bool,
}

impl ChildGuard {
    pub fn id(&self) -> u32 {
        self.child.as_ref().expect("child").id()
    }

    pub fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child")
    }

    /// Wait for the leader to exit; `None` on timeout. A reaped leader is no longer signalled on drop.
    pub fn wait_timeout(&mut self, d: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + d;
        loop {
            if let Some(status) = self.child_mut().try_wait().expect("try_wait") {
                self.reaped = true;
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else { return };
        if self.reaped {
            return;
        }
        // Not yet reaped by us, so the pgid cannot have been reused (a zombie leader holds it).
        // SAFETY: killpg on the group this guard created.
        unsafe { libc::killpg(self.pgid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // SAFETY: as above; the leader is still unreaped.
        unsafe { libc::killpg(self.pgid, libc::SIGKILL) };
        let _ = child.wait();
    }
}

/// `(pid, line)` of every process in `ps eww -ax` (argv followed by the environment) that is not this one.
fn ps_lines() -> Vec<(u32, String)> {
    let out = Command::new("/bin/ps")
        .args(["eww", "-ax", "-o", "pid=,command="])
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else { return Vec::new() };
    let me = std::process::id();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let (pid, rest) = l.split_once(char::is_whitespace)?;
            let pid: u32 = pid.parse().ok()?;
            (pid != me).then(|| (pid, rest.to_string()))
        })
        .collect()
}

/// The values of every ` HG_TEST_ROOT=<v>` token in a `ps eww` line (the value ends at a space or the end).
fn markers(line: &str) -> Vec<&str> {
    let needle = format!(" {TEST_ROOT_VAR}=");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = line[from..].find(&needle) {
        let start = from + i + needle.len();
        let end = line[start..].find(' ').map_or(line.len(), |e| start + e);
        out.push(&line[start..end]);
        from = end;
    }
    out
}

/// Pids [`reap_root`] would signal: the exact marker `HG_TEST_ROOT=<root>`, never a prefix match.
pub fn processes_under(root: &Path) -> Vec<u32> {
    let root = root.to_string_lossy();
    ps_lines().into_iter().filter(|(_, l)| markers(l).contains(&root.as_ref())).map(|(p, _)| p).collect()
}

fn signal_all(pids: &[u32], sig: i32) {
    for &p in pids {
        // SAFETY: the pid was listed with the exact root marker in its environment.
        unsafe { libc::kill(p as i32, sig) };
    }
}

fn sweep(find: impl Fn() -> Vec<u32>) {
    let began = Instant::now();
    let mut killed = false;
    let mut termed = false;
    loop {
        let pids = find();
        if pids.is_empty() || began.elapsed() > Duration::from_secs(10) {
            return;
        }
        if !termed {
            signal_all(&pids, libc::SIGTERM);
            termed = true;
        } else if !killed && began.elapsed() >= Duration::from_secs(3) {
            signal_all(&pids, libc::SIGKILL);
            killed = true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// SIGTERM, then after 3 s SIGKILL, every other process whose environment carries `HG_TEST_ROOT=<root>`;
/// repeated until none remain or 10 s pass.
pub fn reap_root(root: &Path) {
    sweep(|| processes_under(root));
}

/// Same, for processes whose `HG_TEST_ROOT` names a test root that no longer exists (a SIGKILLed run left them).
pub fn reap_orphans() {
    sweep(|| {
        ps_lines()
            .into_iter()
            .filter(|(_, l)| {
                markers(l).iter().any(|m| ROOT_PREFIXES.iter().any(|p| m.starts_with(p)) && !Path::new(m).exists())
            })
            .map(|(p, _)| p)
            .collect()
    });
}
