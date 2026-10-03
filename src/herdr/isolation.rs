//! Test isolation guard and scrubbed child environment (spec §11).
//!
//! Tests that start a private Herdr (or anything that reads Herdr, threads or Claude state) must never
//! reach the user's live resources. [`scrubbed_env`] builds the only environment a private child gets;
//! [`IsolationGuard`] refuses any path outside the private root or inside a live user resource.
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set on every test child (by `scrubbed_env`): every resolved config/state/socket path must lie under it.
pub const TEST_ROOT_VAR: &str = "HG_TEST_ROOT";
/// Exit status of a child that resolved a path outside its test root.
pub const VIOLATION_EXIT: i32 = 97;
/// Appended (one line per violation) under the test root; `TestRoot::drop` fails the test when it is non-empty.
pub const VIOLATIONS_LOG: &str = "isolation-violations.log";

static ARMED: AtomicBool = AtomicBool::new(false);
static LIVE: OnceLock<Vec<(PathBuf, &'static str)>> = OnceLock::new();

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum IsolationError {
    #[error("path {0} is outside the private root {1}")]
    OutsideRoot(PathBuf, PathBuf),
    #[error("path {0} is a live user resource ({1})")]
    LiveResource(PathBuf, &'static str),
}

pub struct IsolationGuard {
    root: PathBuf,
    live: Vec<(PathBuf, &'static str)>,
}

/// Canonicalize `p`; when it does not exist yet, canonicalize its longest existing prefix and apply the
/// missing tail on top (`.` skipped, `..` pops), so a symlinked parent is still resolved and a `..` in
/// the missing part cannot climb out unnoticed.
fn resolve(p: &Path) -> PathBuf {
    let comps: Vec<Component> = p.components().collect();
    for i in (1..=comps.len()).rev() {
        let prefix: PathBuf = comps[..i].iter().collect();
        if let Ok(mut out) = prefix.canonicalize() {
            for c in &comps[i..] {
                match c {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        out.pop();
                    }
                    other => out.push(other.as_os_str()),
                }
            }
            return out;
        }
    }
    p.to_path_buf()
}

/// The real user's resources, from the real environment (`vars`) before any scrubbing: the user's
/// `HERDR_SOCKET_PATH` and default Herdr config/socket dir, the herdr-threads state dir and any
/// `HERDR_THREADS_*` / `HERDR_PLUGIN_STATE_DIR` path, `~/.claude`, and the memory observer dir
/// (`~/.claude-mem` and any variable value containing "memory-observer").
pub fn live_resources_from(
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<(PathBuf, &'static str)> {
    let mut live: Vec<(PathBuf, &'static str)> = Vec::new();
    let mut home: Option<PathBuf> = None;
    for (k, v) in vars {
        if v.is_empty() {
            continue;
        }
        let path = PathBuf::from(&v);
        match k.as_str() {
            "HOME" => home = Some(path),
            "HERDR_SOCKET_PATH" => live.push((path, "the user's Herdr API socket")),
            "HERDR_PLUGIN_STATE_DIR" => live.push((path, "live herdr-threads state")),
            "HERDR_PLUGIN_CONFIG_DIR" => live.push((path, "the user's Herdr plugin config")),
            "CLAUDE_CONFIG_DIR" => live.push((path, "the user's Claude config")),
            _ if k.starts_with("HERDR_THREADS_") && v.starts_with('/') => {
                live.push((path, "live herdr-threads state"))
            }
            _ if v.contains("memory-observer") && v.starts_with('/') => {
                live.push((path, "the memory observer"))
            }
            _ => {}
        }
    }
    if let Some(h) = home {
        live.push((h.join(".config/herdr"), "the user's default Herdr session"));
        live.push((
            h.join(".config/herdr-graph"),
            "the user's herdr-graph config",
        ));
        live.push((
            h.join(".local/state/herdr-threads"),
            "live herdr-threads state",
        ));
        live.push((h.join(".local/state/herdr"), "live herdr-threads state"));
        live.push((h.join(".claude"), "the user's Claude config"));
        live.push((h.join(".claude-mem"), "the memory observer"));
    }
    live
}

impl IsolationGuard {
    /// `live` is computed from the REAL environment of this process.
    pub fn new(root: &Path) -> Self {
        Self::with_live(root, live_resources_from(std::env::vars()))
    }

    pub fn with_live(root: &Path, live: Vec<(PathBuf, &'static str)>) -> Self {
        Self {
            root: resolve(root),
            live: live
                .into_iter()
                .map(|(p, why)| (resolve(&p), why))
                .collect(),
        }
    }

    /// Canonicalized `p` must be under the root and neither equal to nor under any live resource.
    /// Live resources are checked first, so a symlink inside the root that points at one is reported as
    /// the live resource it really is.
    pub fn check(&self, p: &Path) -> Result<(), IsolationError> {
        let real = resolve(p);
        for (live, why) in &self.live {
            if real.starts_with(live) {
                return Err(IsolationError::LiveResource(real, why));
            }
        }
        if !real.starts_with(&self.root) {
            return Err(IsolationError::OutsideRoot(real, self.root.clone()));
        }
        Ok(())
    }
}

/// Variables whose value is a filesystem path the private root must contain.
pub const PATH_VARS: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "HERDR_CONFIG_PATH",
    "HERDR_SOCKET_PATH",
    "HERDR_PLUGIN_STATE_DIR",
    "HERDR_PLUGIN_CONFIG_DIR",
    "CLAUDE_CONFIG_DIR",
    "HERDR_GRAPH_INSTANCE",
];

/// Host variables that are harmless to pass on (no state, no credentials).
const PASS_THROUGH: &[&str] = &["PATH", "LANG", "LC_ALL", "TERM", "SHELL", "USER", "LOGNAME"];
/// The only credentials a private agent may receive: explicit API keys (the macOS keychain is not
/// HOME-based and needs no variable). The user's settings, hooks, skills and observer config are never copied.
const CREDENTIALS: &[&str] = &["ANTHROPIC_API_KEY", "OPENAI_API_KEY"];

/// [`scrubbed_env`] over an explicit variable source (testable without mutating the process env).
pub fn scrubbed_env_from(
    root: &Path,
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    let p = |rel: &str| root.join(rel).to_string_lossy().into_owned();
    let mut env: Vec<(String, String)> = vec![
        ("HOME".into(), p("home")),
        ("XDG_CONFIG_HOME".into(), p("config")),
        ("XDG_STATE_HOME".into(), p("state")),
        ("XDG_DATA_HOME".into(), p("data")),
        ("XDG_CACHE_HOME".into(), p("cache")),
        ("XDG_RUNTIME_DIR".into(), p("runtime")),
        ("HERDR_CONFIG_PATH".into(), p("config/herdr.toml")),
        ("HERDR_SOCKET_PATH".into(), p("herdr.sock")),
        ("HERDR_PLUGIN_STATE_DIR".into(), p("threads-state")),
        ("HERDR_PLUGIN_CONFIG_DIR".into(), p("plugin-config")),
        ("CLAUDE_CONFIG_DIR".into(), p("claude")),
        ("HERDR_GRAPH_INSTANCE".into(), p("graph")),
        (TEST_ROOT_VAR.into(), root.to_string_lossy().into_owned()),
    ];
    for (k, v) in vars {
        if (PASS_THROUGH.contains(&k.as_str()) || CREDENTIALS.contains(&k.as_str()))
            && !v.is_empty()
        {
            env.push((k, v));
        }
    }
    env
}

/// The env every private child gets: no inherited `HERDR_*` / `CLAUDE_*` / `XDG_*`; HOME, XDG dirs,
/// Herdr config/socket, threads state, Claude config and the graph instance all under `root`; PATH and a
/// few inert variables kept; credentials only from explicit `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`.
pub fn scrubbed_env(root: &Path) -> Vec<(String, String)> {
    scrubbed_env_from(root, std::env::vars())
}

/// Arm the in-process tripwire for this whole process (integration-test fixtures call it; lib unit tests are
/// always armed). Captures the live set from the REAL environment on first use.
pub fn arm() {
    LIVE.get_or_init(|| live_resources_from(std::env::vars()));
    ARMED.store(true, Ordering::SeqCst);
}

fn armed() -> bool {
    cfg!(test) || ARMED.load(Ordering::SeqCst)
}

/// Pure verdict: `root` (from `HG_TEST_ROOT`) wins when present; otherwise `live` must not contain `p`.
/// Paths are resolved first (symlinks, `..`), so neither can slip past.
pub fn tripwire_verdict(
    p: &Path,
    root: Option<&Path>,
    live: &[(PathBuf, &'static str)],
) -> Result<(), String> {
    let real = resolve(p);
    if let Some(root) = root {
        return if real.starts_with(resolve(root)) {
            Ok(())
        } else {
            Err(format!(
                "path {} is outside the test root {}",
                real.display(),
                root.display()
            ))
        };
    }
    for (res, why) in live {
        if real.starts_with(resolve(res)) {
            return Err(format!(
                "path {} is a live user resource ({why})",
                real.display()
            ));
        }
    }
    Ok(())
}

/// Call wherever a config, state, Claude or socket path is RESOLVED (before it is read, written or connected).
/// `HG_TEST_ROOT` set: outside the root, append to `<root>/isolation-violations.log`, print, exit 97.
/// Armed (`cfg(test)` or [`arm`]): a live resource panics. Otherwise a no-op (production).
pub fn tripwire(p: &Path, what: &str) {
    if let Some(root) = std::env::var_os(TEST_ROOT_VAR)
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
    {
        if let Err(msg) = tripwire_verdict(p, Some(&root), &[]) {
            use std::io::Write;
            let line = format!("{what}: {msg}");
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join(VIOLATIONS_LOG))
            {
                let _ = writeln!(f, "{line}");
            }
            eprintln!("isolation tripwire: {line}");
            std::process::exit(VIOLATION_EXIT);
        }
        return;
    }
    if armed() {
        let live = LIVE.get_or_init(|| live_resources_from(std::env::vars()));
        if let Err(msg) = tripwire_verdict(p, None, live) {
            panic!("isolation tripwire: {what}: {msg}");
        }
    }
}
