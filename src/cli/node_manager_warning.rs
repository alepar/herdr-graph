//! The unassigned node-manager warning (spec §4.10). Reads graph.toml at HEAD straight from the instance store,
//! for a warning only: it answers no query, so it is not a daemon fallback (B1). Any failure prints nothing.
use crate::cli::{Command, seat};
use crate::config::{Env, plugin_config_dir_via_herdr};
use crate::model::manager::Pointers;
use crate::plan::managers::NODE_MANAGER_SET_USAGE;
use crate::store::tree::TreeRead;

pub fn warning_text() -> String {
    format!("herdr-graph: warning: no node manager is assigned; run {NODE_MANAGER_SET_USAGE}")
}

/// False for init, setup, daemon and `seat --hook-prompt`.
pub fn applies(cmd: &Command) -> bool {
    !matches!(
        cmd,
        Command::Init(_)
            | Command::Setup(_)
            | Command::Daemon(_)
            | Command::Seat(seat::Commands::Seat {
                hook_prompt: true,
                ..
            })
    )
}

/// The warning for the committed tree, or None: a non-retired seat exists and no live node manager is set
/// (unset, missing or retired). Read errors give None.
pub fn check(tree: &dyn TreeRead) -> Option<String> {
    let p = Pointers::load(tree).ok()?;
    let has_live_seat = p.seats.values().any(|s| !s.retired);
    (has_live_seat && p.live_node_manager().is_none()).then(warning_text)
}

/// Locate the instance (no instance, nothing), open its GitStore at HEAD, `check`, eprintln once.
pub fn warn(cmd: &Command) {
    if !applies(cmd) {
        return;
    }
    let env = Env::from_process();
    let Some((root, _)) = crate::config::locate_instance(&env, &plugin_config_dir_via_herdr) else {
        return;
    };
    let Ok(store) = crate::store::GitStore::open(&root) else {
        return;
    };
    let Ok(head) = crate::ports::store::Store::head(&store) else {
        return;
    };
    let view = crate::store::tree::CommitView {
        store: &store,
        at: head,
    };
    if let Some(w) = check(&view) {
        eprintln!("{w}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::store::GitStore;
    use crate::store::init::init_instance;
    use crate::store::tree::CommitView;
    use clap::Parser;

    fn cmd(argv: &[&str]) -> Command {
        let mut v = vec!["herdr-graph"];
        v.extend_from_slice(argv);
        Cli::try_parse_from(v).unwrap().command
    }

    #[test]
    fn applies_skips_init_setup_daemon_hook_prompt() {
        for argv in [
            &["init", "/tmp/x"][..],
            &["setup", "claude"],
            &["daemon"],
            &["daemon", "--ensure"],
            &["seat", "--hook-prompt"],
            &["seat", "--hook-prompt", "--codex"],
        ] {
            assert!(!applies(&cmd(argv)), "{argv:?}");
        }
        for argv in [
            &["list"][..],
            &["plan", "--json", "seat", "create", "x", "--teamspace", "t"],
            &["seat", "--json"],
            &["seat"],
            &["status"],
        ] {
            assert!(applies(&cmd(argv)), "{argv:?}");
        }
    }

    #[test]
    fn warning_uses_usage_constant() {
        let w = warning_text();
        assert!(w.contains(NODE_MANAGER_SET_USAGE), "{w}");
        assert_eq!(
            w,
            "herdr-graph: warning: no node manager is assigned; run herdr-graph plan node-manager set <seat>"
        );
    }

    const SEAT: &str = "st_01JABCDEFGHJKMNPQRSTVWXYZ0";
    const TS: &str = "ts_01JABCDEFGHJKMNPQRSTVWXYZ0";
    const GHOST: &str = "st_01JABCDEFGHJKMNPQRSTVWXYZ1";

    fn commit(dir: &std::path::Path, files: &[(&str, String)]) {
        let repo = git2::Repository::open(dir).unwrap();
        let mut index = repo.index().unwrap();
        for (rel, text) in files {
            let target = dir.join(rel);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, text).unwrap();
            index.add_path(std::path::Path::new(rel)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = crate::store::git::graph_signature().unwrap();
        repo.commit(Some("refs/heads/main"), &sig, &sig, "t", &tree, &[&parent])
            .unwrap();
    }

    fn seat_files(lifecycle: &str) -> Vec<(&'static str, String)> {
        vec![
            (
                "teamspaces/t/teamspace.toml",
                format!(
                    "schema = 1\nid = \"{TS}\"\nrev = 1\nname = \"t\"\nlifecycle = \"active\"\n"
                ),
            ),
            (
                "teamspaces/t/seats/s/seat.toml",
                format!(
                    "schema = 1\nid = \"{SEAT}\"\nrev = 1\nname = \"s\"\nteamspace = \"{TS}\"\nlifecycle = \"{lifecycle}\"\n"
                ),
            ),
        ]
    }

    fn with_node_manager(dir: &std::path::Path, id: &str) -> (&'static str, String) {
        let mut g: crate::model::graph::GraphRecord =
            toml::from_str(&std::fs::read_to_string(dir.join("graph.toml")).unwrap()).unwrap();
        g.node_manager_seat = Some(id.parse().unwrap());
        ("graph.toml", toml::to_string(&g).unwrap())
    }

    fn checked(dir: &std::path::Path) -> Option<String> {
        let store = GitStore::open(dir).unwrap();
        let view = CommitView {
            store: &store,
            at: crate::ports::store::Store::head(&store).unwrap(),
        };
        check(&view)
    }

    #[test]
    fn check_cases() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("inst");
        init_instance(&dir).unwrap();
        assert_eq!(checked(&dir), None, "empty graph");

        commit(&dir, &seat_files("active"));
        assert_eq!(checked(&dir), Some(warning_text()), "live seat, unset");

        let nm = with_node_manager(&dir, SEAT);
        commit(&dir, &[nm]);
        assert_eq!(checked(&dir), None, "assigned");

        let nm = with_node_manager(&dir, GHOST);
        commit(&dir, &[nm]);
        assert_eq!(checked(&dir), Some(warning_text()), "missing seat");

        let nm = with_node_manager(&dir, SEAT);
        let mut files = seat_files("retired");
        files.push(nm);
        commit(&dir, &files);
        assert_eq!(checked(&dir), None, "retired seat only: nothing to assign");
    }

    #[test]
    fn retired_node_manager_with_another_live_seat_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("inst");
        init_instance(&dir).unwrap();
        let mut files = seat_files("retired");
        files.push((
            "teamspaces/t/seats/u/seat.toml",
            format!("schema = 1\nid = \"{GHOST}\"\nrev = 1\nname = \"u\"\nteamspace = \"{TS}\"\nlifecycle = \"active\"\n"),
        ));
        commit(&dir, &files);
        let nm = with_node_manager(&dir, SEAT);
        commit(&dir, &[nm]);
        assert_eq!(checked(&dir), Some(warning_text()));
    }
}
