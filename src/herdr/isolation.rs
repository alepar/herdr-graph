//! Test isolation guard and scrubbed child environment (spec §11).
//!
//! Tests that start a private Herdr (or anything that reads Herdr, threads or Claude state) must never
//! reach the user's live resources. [`scrubbed_env`] builds the only environment a private child gets;
//! [`IsolationGuard`] refuses any path outside the private root or inside a live user resource.
use std::path::{Component, Path, PathBuf};

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
pub fn live_resources_from(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(PathBuf, &'static str)> {
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
            "CLAUDE_CONFIG_DIR" => live.push((path, "the user's Claude config")),
            _ if k.starts_with("HERDR_THREADS_") && v.starts_with('/') => live.push((path, "live herdr-threads state")),
            _ if v.contains("memory-observer") && v.starts_with('/') => live.push((path, "the memory observer")),
            _ => {}
        }
    }
    if let Some(h) = home {
        live.push((h.join(".config/herdr"), "the user's default Herdr session"));
        live.push((h.join(".local/state/herdr-threads"), "live herdr-threads state"));
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
        Self { root: resolve(root), live: live.into_iter().map(|(p, why)| (resolve(&p), why)).collect() }
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
    "CLAUDE_CONFIG_DIR",
    "HERDR_GRAPH_INSTANCE",
];

/// Host variables that are harmless to pass on (no state, no credentials).
const PASS_THROUGH: &[&str] = &["PATH", "LANG", "LC_ALL", "TERM", "SHELL", "USER", "LOGNAME"];
/// The only credentials a private agent may receive: explicit API keys (the macOS keychain is not
/// HOME-based and needs no variable). The user's settings, hooks, skills and observer config are never copied.
const CREDENTIALS: &[&str] = &["ANTHROPIC_API_KEY", "OPENAI_API_KEY"];

/// [`scrubbed_env`] over an explicit variable source (testable without mutating the process env).
pub fn scrubbed_env_from(root: &Path, vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
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
        ("CLAUDE_CONFIG_DIR".into(), p("claude")),
        ("HERDR_GRAPH_INSTANCE".into(), p("graph")),
    ];
    for (k, v) in vars {
        if (PASS_THROUGH.contains(&k.as_str()) || CREDENTIALS.contains(&k.as_str())) && !v.is_empty() {
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
