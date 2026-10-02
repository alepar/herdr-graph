//! `doctor`: installation and instance diagnostics (spec §10).
use super::client::{Client, hello};
use crate::config::{Env, InstanceSource, plugin_config_dir_via_herdr, socket_path};
use serde::Serialize;
use std::path::Path;
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
    match comp("threads") {
        Some(t) => checks.push(check("threads", true, t.to_string())),
        None => checks.push(check("threads", true, "not wired")),
    }
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
    fn doctor_without_instance_fails_first_check() {
        let home = tempfile::tempdir().unwrap();
        let env = Env { home: Some(home.path().into()), ..Default::default() };
        let r = doctor(&env);
        assert_eq!(r.checks.len(), 1);
        assert!(!r.all_ok());
        assert_eq!(r.checks[0].name, "instance");
    }
}
