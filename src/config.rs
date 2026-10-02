//! Instance location chain (HERDR_GRAPH_INSTANCE → plugin config-dir → ~/.config/herdr-graph) (spec §1). Owned by hg-zmi.4.
use sha2::{Digest, Sha256};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const ENV_INSTANCE: &str = "HERDR_GRAPH_INSTANCE";
/// Unix socket paths must stay below the platform limit (104 bytes on macOS).
const SOCKET_PATH_LIMIT: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct Env {
    pub instance: Option<String>,
    pub herdr_bin: Option<String>,
    pub home: Option<PathBuf>,
    pub herdr_socket: Option<PathBuf>,
    pub pane_id: Option<String>,
    pub graph_clone: Option<String>,
    pub graph_seat: Option<String>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl Env {
    pub fn from_process() -> Self {
        Self {
            instance: var(ENV_INSTANCE),
            herdr_bin: var("HERDR_BIN_PATH"),
            home: var("HOME").map(PathBuf::from),
            herdr_socket: var("HERDR_SOCKET_PATH").map(PathBuf::from),
            pane_id: var("HERDR_PANE_ID"),
            graph_clone: var("HERDR_GRAPH_CLONE"),
            graph_seat: var("HERDR_GRAPH_SEAT"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceSource {
    EnvVar,
    PluginConfig(PathBuf),
    UserConfig(PathBuf),
}

/// First hit of: HERDR_GRAPH_INSTANCE → instance key in <plugin config dir>/config.toml → <home>/.config/herdr-graph/config.toml.
/// `plugin_config_dir` is injected (tests) — production passes `plugin_config_dir_via_herdr`.
pub fn locate_instance(
    env: &Env,
    plugin_config_dir: &dyn Fn(&Env) -> Option<PathBuf>,
) -> Option<(PathBuf, InstanceSource)> {
    if let Some(i) = &env.instance {
        return Some((PathBuf::from(i), InstanceSource::EnvVar));
    }
    if let Some(dir) = plugin_config_dir(env) {
        let cfg = dir.join("config.toml");
        if let Some(root) = read_instance_key(&cfg) {
            return Some((root, InstanceSource::PluginConfig(cfg)));
        }
    }
    if let Some(home) = &env.home {
        let cfg = user_config_path(home);
        if let Some(root) = read_instance_key(&cfg) {
            return Some((root, InstanceSource::UserConfig(cfg)));
        }
    }
    None
}

/// Runs `$HERDR_BIN_PATH plugin config-dir herdr-graph` (2 s timeout), returns trimmed stdout as a path.
pub fn plugin_config_dir_via_herdr(env: &Env) -> Option<PathBuf> {
    let bin = env.herdr_bin.as_ref()?;
    let mut child = Command::new(bin)
        .args(["plugin", "config-dir", "herdr-graph"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    let out = out.trim();
    (!out.is_empty()).then(|| PathBuf::from(out))
}

/// `instance = "<abs path>"` from a config.toml; None when missing, unparsable, empty or relative.
pub fn read_instance_key(config_toml: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(config_toml).ok()?;
    let table: toml::Table = text.parse().ok()?;
    let p = PathBuf::from(table.get("instance")?.as_str()?);
    (p.is_absolute() && !p.as_os_str().is_empty()).then_some(p)
}

pub fn user_config_path(home: &Path) -> PathBuf {
    home.join(".config/herdr-graph/config.toml")
}

#[derive(Debug, Clone)]
pub struct InstancePaths {
    pub root: PathBuf,
    pub local: PathBuf,
    pub journal: PathBuf,
    pub lock: PathBuf,
    pub socket: PathBuf,
    pub log: PathBuf,
    pub plans: PathBuf,
    pub baseline: PathBuf,
    pub threads_intents: PathBuf,
    pub worktree_dirty: PathBuf,
}

impl InstancePaths {
    pub fn new(root: &Path) -> Self {
        let local = root.join(".graph-local");
        Self {
            root: root.to_path_buf(),
            journal: local.join("journal.sqlite3"),
            lock: local.join("daemon.lock"),
            socket: socket_path(root),
            log: local.join("daemon.log"),
            plans: local.join("plans"),
            baseline: local.join("baseline.json"),
            threads_intents: local.join("threads-intents"),
            worktree_dirty: local.join("worktree_dirty"),
            local,
        }
    }
}

/// `<root>/.graph-local/daemon.sock`; if that path is ≥ 100 bytes → `/private/tmp/herdr-graph-<uid>/<hash16>.sock`
/// with hash16 = first 16 hex chars of sha256(root as bytes). uid = libc::getuid().
pub fn socket_path(root: &Path) -> PathBuf {
    let direct = root.join(".graph-local/daemon.sock");
    if direct.as_os_str().len() < SOCKET_PATH_LIMIT {
        return direct;
    }
    let digest = Sha256::digest(root.as_os_str().as_bytes());
    let hash16: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/private/tmp/herdr-graph-{uid}/{hash16}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cfg(dir: &Path, instance: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("config.toml"), format!("instance = \"{instance}\"\n")).unwrap();
    }

    #[test]
    fn locate_env_var_wins() {
        let t = tempfile::tempdir().unwrap();
        write_cfg(t.path(), "/from/plugin");
        let env = Env { instance: Some("/from/env".into()), home: Some(t.path().into()), ..Default::default() };
        let got = locate_instance(&env, &|_| Some(t.path().into())).unwrap();
        assert_eq!(got, (PathBuf::from("/from/env"), InstanceSource::EnvVar));
    }

    #[test]
    fn locate_plugin_config_before_user_config() {
        let plugin = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write_cfg(plugin.path(), "/from/plugin");
        write_cfg(&home.path().join(".config/herdr-graph"), "/from/user");
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        let (root, src) = locate_instance(&env, &|_| Some(plugin.path().into())).unwrap();
        assert_eq!(root, PathBuf::from("/from/plugin"));
        assert_eq!(src, InstanceSource::PluginConfig(plugin.path().join("config.toml")));
    }

    #[test]
    fn locate_user_config_last() {
        let home = tempfile::tempdir().unwrap();
        write_cfg(&home.path().join(".config/herdr-graph"), "/from/user");
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        // plugin dir resolves but has no config.toml → falls through
        let empty = tempfile::tempdir().unwrap();
        let (root, src) = locate_instance(&env, &|_| Some(empty.path().into())).unwrap();
        assert_eq!(root, PathBuf::from("/from/user"));
        assert!(matches!(src, InstanceSource::UserConfig(_)));
    }

    #[test]
    fn locate_none() {
        let home = tempfile::tempdir().unwrap();
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        assert!(locate_instance(&env, &|_| None).is_none());
    }

    #[test]
    fn read_instance_key_parses_and_rejects_garbage() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("config.toml");
        std::fs::write(&f, "instance = \"/abs/path\"\n").unwrap();
        assert_eq!(read_instance_key(&f), Some(PathBuf::from("/abs/path")));
        std::fs::write(&f, "instance = \"relative/path\"\n").unwrap();
        assert_eq!(read_instance_key(&f), None);
        std::fs::write(&f, "instance = 5\n").unwrap();
        assert_eq!(read_instance_key(&f), None);
        std::fs::write(&f, "this is [not toml").unwrap();
        assert_eq!(read_instance_key(&f), None);
        assert_eq!(read_instance_key(&t.path().join("missing.toml")), None);
    }

    #[test]
    fn socket_path_short_root_is_in_instance() {
        let root = Path::new("/tmp/inst");
        assert_eq!(socket_path(root), root.join(".graph-local/daemon.sock"));
        assert_eq!(InstancePaths::new(root).socket, root.join(".graph-local/daemon.sock"));
    }

    #[test]
    fn socket_path_long_root_falls_back_under_private_tmp() {
        let root = PathBuf::from(format!("/{}", "a".repeat(119)));
        let p = socket_path(&root);
        assert!(p.to_str().unwrap().starts_with("/private/tmp/herdr-graph-"), "{p:?}");
        assert!(p.as_os_str().len() < 100);
        assert_eq!(p, socket_path(&root));
        let other = PathBuf::from(format!("/{}", "b".repeat(119)));
        assert_ne!(p, socket_path(&other));
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), 16 + ".sock".len());
    }
}
