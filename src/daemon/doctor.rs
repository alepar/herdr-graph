//! `doctor`: installation and instance diagnostics (spec §10).
use super::client::{Client, hello};
use crate::config::{Env, InstanceSource, plugin_config_dir_via_herdr, socket_path};
use crate::threads::discovery::{DiscoveryInputs, Source, resolve_state_dir};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Check {
    Check { name: name.into(), ok, detail: detail.into() }
}

/// Paths recorded in `.graph-local/worktree_dirty`: JSON lines `{"path":…,"op":…,"at":…}` (written by the writer).
/// Unparsable lines are skipped; a missing file is "no entries".
pub fn read_worktree_dirty(file: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["path"].as_str().map(str::to_owned))
        .collect()
}

/// The `threads` check: where the herdr-threads state dir was found (or why not), then the daemon's own view
/// of the connection. With a daemon, `ok` is its `connected`; without one, `ok` is "the state dir was found".
fn threads_check(resolved: Result<Option<(PathBuf, Source)>, String>, component: Option<&Value>) -> Check {
    let (found, mut detail) = match &resolved {
        Ok(Some((dir, source))) => (true, format!("state dir {} ({source})", dir.display())),
        Ok(None) => (
            false,
            "not found (no herdr-threads state directory in the default places; install/start herdr-threads or set \
             threads_state_dir in config.toml)"
                .to_string(),
        ),
        Err(why) => (false, format!("not found ({why})")),
    };
    let Some(c) = component else {
        return check("threads", found, detail);
    };
    let connected = c["connected"].as_bool().unwrap_or(false);
    if connected {
        detail.push_str(&format!("; daemon connected, capability {}", c["capability"]));
    } else {
        let why = c["error"].as_str().or(c["note"].as_str()).unwrap_or("no reason reported");
        detail.push_str(&format!("; daemon not connected: {why}"));
    }
    check("threads", connected, detail)
}

pub fn doctor(env: &Env) -> DoctorReport {
    let mut checks = Vec::new();
    let Some((root, source)) = crate::config::locate_instance(env, &plugin_config_dir_via_herdr) else {
        checks.push(check("instance", false, "no instance configured — run herdr-graph init"));
        return DoctorReport { checks };
    };
    let src = match &source {
        InstanceSource::EnvVar => format!("{} via HERDR_GRAPH_INSTANCE", crate::config::ENV_INSTANCE),
        InstanceSource::PluginConfig(p) | InstanceSource::UserConfig(p) => format!("via {}", p.display()),
    };
    checks.push(check("instance", true, format!("{} ({src})", root.display())));

    match crate::store::GitStore::open(&root) {
        Ok(_) => checks.push(check("instance repo", true, "opens, graph.toml on main")),
        Err(e) => checks.push(check("instance repo", false, e.to_string())),
    }

    let sock = socket_path(&root);
    let hello_reply = hello(&sock);
    match &hello_reply {
        Some(h) => checks.push(check("daemon", true, format!("pid {} on {}", h["pid"], sock.display()))),
        None => checks.push(check("daemon", false, format!("not reachable at {}", sock.display()))),
    }

    match (&env.herdr_socket, hello_reply.as_ref().and_then(|h| h["herdr_socket"].as_str())) {
        (Some(mine), Some(theirs)) if Path::new(theirs) == mine.as_path() => {
            checks.push(check("daemon herdr socket", true, theirs.to_string()));
        }
        (Some(mine), Some(theirs)) => checks.push(check(
            "daemon herdr socket",
            false,
            format!("daemon uses {theirs}; this pane uses {}", mine.display()),
        )),
        (None, Some(theirs)) => checks.push(check("daemon herdr socket", true, format!("{theirs} (HERDR_SOCKET_PATH unset here)"))),
        (_, None) => checks.push(check("daemon herdr socket", false, "no daemon to compare")),
    }

    match &env.herdr_socket {
        None => checks.push(check("herdr socket", false, "HERDR_SOCKET_PATH unset (not inside Herdr)")),
        Some(p) => match std::os::unix::net::UnixStream::connect(p) {
            Ok(_) => checks.push(check("herdr socket", true, format!("{} reachable", p.display()))),
            Err(e) => checks.push(check("herdr socket", false, format!("{}: {e}", p.display()))),
        },
    }

    let dirty = read_worktree_dirty(&crate::config::InstancePaths::new(&root).worktree_dirty);
    if dirty.is_empty() {
        checks.push(check("worktree_dirty", true, "no entries"));
    } else {
        checks.push(check("worktree_dirty", false, format!("{} entries: {}", dirty.len(), dirty.join(", "))));
    }

    let status = Client::connect(&sock, Duration::from_secs(2)).ok().and_then(|mut c| c.call("status", serde_json::json!({})).ok());
    let comp = |name: &str| status.as_ref().and_then(|s| s["components"].get(name)).cloned();
    match comp("writer") {
        Some(w) => {
            let halted = w["writer_halted"].as_bool().unwrap_or(false);
            checks.push(check("writer", !halted, if halted { format!("halted: {w}") } else { "running".into() }));
        }
        None => checks.push(check("writer", true, "not reported by daemon")),
    }
    let inputs = DiscoveryInputs {
        env_state_dir: env.threads_state_dir.clone(),
        config_state_dir: crate::config::read_threads_state_dir(env, &plugin_config_dir_via_herdr),
        own_plugin_state_dir: env.plugin_state_dir.clone(),
        xdg_state_home: env.xdg_state_home.clone(),
        home: env.home.clone(),
    };
    checks.push(threads_check(resolve_state_dir(&inputs), comp("threads").as_ref()));
    match comp("journal") {
        Some(j) => checks.push(check("journal", true, j.to_string())),
        None => checks.push(check("journal", true, "no counts reported")),
    }
    DoctorReport { checks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_dirty_parses_lines_and_skips_garbage() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("worktree_dirty");
        std::fs::write(
            &f,
            "{\"path\":\"teamspaces/a/ts.toml\",\"op\":\"write\",\"at\":\"2026-01-01T00:00:00Z\"}\nnot json\n{\"op\":\"x\"}\n{\"path\":\"b\"}\n",
        )
        .unwrap();
        assert_eq!(read_worktree_dirty(&f), vec!["teamspaces/a/ts.toml", "b"]);
        assert!(read_worktree_dirty(&t.path().join("missing")).is_empty());
    }

    #[test]
    fn threads_check_connected_ok() {
        let found = Ok(Some((PathBuf::from("/s/herdr-threads"), Source::HomeDefault)));
        let c = threads_check(found, Some(&serde_json::json!({"connected": true, "capability": "service_ack"})));
        assert!(c.ok, "{c:?}");
        assert!(c.detail.contains("state dir /s/herdr-threads"), "{}", c.detail);
        assert!(c.detail.contains("daemon connected"), "{}", c.detail);
        // No daemon: ok follows the discovery result.
        assert!(threads_check(Ok(Some((PathBuf::from("/s"), Source::EnvVar))), None).ok);
    }

    #[test]
    fn threads_check_not_found_fails_with_hint() {
        let c = threads_check(Ok(None), None);
        assert!(!c.ok);
        assert!(c.detail.contains("not found") && c.detail.contains("threads_state_dir"), "{}", c.detail);
        let c = threads_check(Err("both a and b exist".into()), None);
        assert!(!c.ok);
        assert!(c.detail.contains("both a and b exist"), "{}", c.detail);
    }

    #[test]
    fn threads_check_daemon_disconnected_fails() {
        let found = Ok(Some((PathBuf::from("/s/herdr-threads"), Source::XdgDefault)));
        let c = threads_check(found, Some(&serde_json::json!({"connected": false, "error": "refused"})));
        assert!(!c.ok, "a found dir does not make a disconnected daemon ok");
        assert!(c.detail.contains("refused") && c.detail.contains("/s/herdr-threads"), "{}", c.detail);
    }

    #[test]
    fn doctor_without_instance_fails_first_check() {
        let home = tempfile::tempdir().unwrap();
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        let r = doctor(&env);
        assert_eq!(r.checks.len(), 1);
        assert!(!r.all_ok());
        assert_eq!(r.checks[0].name, "instance");
    }
}
