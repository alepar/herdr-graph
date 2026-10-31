//! `instructions show` (spec §2.2): read-only, committed head, no daemon.
use crate::daemon::client::{EXIT_NO_INSTANCE, instance_or_none};
use crate::ports::store::{RepoPath, Store};
use crate::store::instruction_source::{
    InstructionTarget, check_content, list_files, parse_words, path, resolve_scope,
};
use crate::store::tree::{CommitView, TreeRead};
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Read-only views of instruction files outside templates.
    Instructions {
        #[command(subcommand)]
        action: InstructionsAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum InstructionsAction {
    /// Print `<scope> [<target>] [<file>]` from the committed head: global | teamspace <t> | seat <s>.
    Show {
        #[arg(required = true, num_args = 1..=3)]
        words: Vec<String>,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Some(root) = instance_or_none() else {
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let store = crate::store::GitStore::open(&root)?;
    let view = CommitView {
        store: &store,
        at: store.head()?,
    };
    let Commands::Instructions { action } = cmd;
    match action {
        InstructionsAction::Show { words } => {
            let mut text = show_text(&view, &words)?;
            if !text.ends_with('\n') {
                text.push('\n');
            }
            print!("{text}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A file's text, or the placeholder line when it is not admissible.
fn file_text(view: &dyn TreeRead, path: &RepoPath) -> anyhow::Result<String> {
    let bytes = view
        .read_file(path)?
        .ok_or_else(|| anyhow::anyhow!("no such instruction file {}", path.as_str()))?;
    Ok(match check_content(&bytes) {
        Ok(text) => text.to_owned(),
        Err(e) => placeholder(path, &e.to_string()),
    })
}

fn placeholder(path: &RepoPath, reason: &str) -> String {
    format!(
        "({}: not an admissible instruction file: {reason})",
        path.as_str()
    )
}

pub fn show_text(view: &dyn TreeRead, words: &[String]) -> anyhow::Result<String> {
    let r = parse_words(words, false)?;
    let scope = resolve_scope(view, r.scope, r.target.as_deref())?;
    if let Some(file) = r.file {
        let p = path(view, &InstructionTarget { scope, file })?;
        return file_text(view, &p);
    }
    let listed = list_files(view, &scope)?;
    if listed.is_empty() {
        return Ok(format!("(no instruction files in {})", r.scope.word()));
    }
    let mut out = String::new();
    for f in listed {
        let body = match &f.name {
            Ok(_) => file_text(view, &f.path)?,
            Err(e) => placeholder(&f.path, &e.to_string()),
        };
        out.push_str(&format!("== {} ==\n{body}", f.path.as_str()));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CommitId;
    use crate::model::seat::SeatRecord;
    use crate::model::teamspace::TeamspaceRecord;
    use crate::model::{Lifecycle, SCHEMA_VERSION, SeatId, TeamspaceId};
    use crate::store::git::graph_signature;
    use crate::store::{EditSet, GitStore, layout, record};
    use clap::Parser;

    struct Fx {
        _t: tempfile::TempDir,
        dir: std::path::PathBuf,
        store: GitStore,
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    /// Instance with teamspace `alpha`, seat `a1`, and the given extra (repo path, bytes) files committed.
    fn fixture(files: &[(&str, &[u8])]) -> Fx {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("inst");
        crate::store::init::init_instance(&dir).unwrap();
        let store = GitStore::open(&dir).unwrap();
        let ts = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: TeamspaceId::new(),
            rev: 1,
            name: "alpha".into(),
            name_history: vec![],
            lifecycle: Lifecycle::Active,
            retired: None,
            runtime: Default::default(),
            project_repo: None,
            channel: Default::default(),
        };
        let seat = SeatRecord {
            schema: SCHEMA_VERSION,
            id: SeatId::new(),
            rev: 1,
            name: "a1".into(),
            name_history: vec![],
            teamspace: ts.id.clone(),
            lifecycle: Lifecycle::Active,
            retired: None,
            system_duty: None,
            template_ref: None,
            applications: vec![],
            overrides: Default::default(),
            participation: Default::default(),
            activation: Default::default(),
            runtime: Default::default(),
            channel: Default::default(),
            reload_required: false,
            moved_out: false,
        };
        let td = layout::teamspace_dir("alpha");
        let sd = layout::seat_dir(&td, "a1");
        let mut e = EditSet::default();
        e.put(
            layout::teamspace_record(&td),
            record::to_toml_bytes(&ts).unwrap(),
        );
        e.put(
            layout::seat_record(&sd),
            record::to_toml_bytes(&seat).unwrap(),
        );
        for (p, b) in files {
            e.put(RepoPath::new(p).unwrap(), b.to_vec());
        }
        let head = store.head().unwrap();
        let tree = store.build_tree(&head, &e).unwrap();
        store
            .with_repo(|r| {
                let sig = graph_signature()?;
                let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
                let oid = r.commit(None, &sig, &sig, "fixture", &r.find_tree(tree)?, &[&parent])?;
                r.reference("refs/heads/main", oid, true, "fixture")?;
                Ok(CommitId(oid.to_string()))
            })
            .unwrap();
        Fx { _t: t, dir, store }
    }

    fn show(fx: &Fx, s: &str) -> anyhow::Result<String> {
        let view = CommitView {
            store: &fx.store,
            at: fx.store.head().unwrap(),
        };
        show_text(&view, &words(s))
    }

    const SEAT: &str = "teamspaces/alpha/seats/a1";

    #[test]
    fn show_one_file_prints_committed_text() {
        let fx = fixture(&[
            ("rules/x.md", b"global rule\n"),
            ("teamspaces/alpha/rules/t.md", b"team rule\r\nline2"),
            ("teamspaces/alpha/seats/a1/AGENTS.md", b"seat agents\n"),
        ]);
        assert_eq!(show(&fx, "global rules/x.md").unwrap(), "global rule\n");
        assert_eq!(
            show(&fx, "teamspace alpha rules/t.md").unwrap(),
            "team rule\r\nline2"
        );
        assert_eq!(show(&fx, "seat a1 AGENTS.md").unwrap(), "seat agents\n");
    }

    #[test]
    fn show_scope_lists_every_file() {
        let fx = fixture(&[
            ("teamspaces/alpha/seats/a1/AGENTS.md", b"agents\n"),
            ("teamspaces/alpha/seats/a1/rules/b.md", b"bee\n"),
            ("teamspaces/alpha/seats/a1/rules/a.md", b"ay"),
        ]);
        let out = show(&fx, "seat a1").unwrap();
        assert_eq!(
            out,
            format!(
                "== {SEAT}/AGENTS.md ==\nagents\n== {SEAT}/rules/a.md ==\nay\n== {SEAT}/rules/b.md ==\nbee\n"
            )
        );
        assert_eq!(
            show(&fx, "teamspace alpha").unwrap(),
            "(no instruction files in teamspace)"
        );
    }

    #[test]
    fn show_errors() {
        let fx = fixture(&[]);
        for bad in ["bogus x", "seat nobody AGENTS.md", "global AGENTS.md"] {
            assert!(show(&fx, bad).is_err(), "{bad}");
        }
        let e = show(&fx, "seat nobody AGENTS.md").unwrap_err().to_string();
        assert!(e.contains("nobody"), "{e}");
        let e = show(&fx, "seat a1 rules/absent.md")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("no such instruction file") && e.contains("rules/absent.md"),
            "{e}"
        );
    }

    #[test]
    fn show_ignores_working_tree() {
        let fx = fixture(&[("rules/x.md", b"committed\n")]);
        std::fs::create_dir_all(fx.dir.join("rules")).unwrap();
        std::fs::write(fx.dir.join("rules/x.md"), "dirty\n").unwrap();
        std::fs::write(fx.dir.join("rules/y.md"), "untracked\n").unwrap();
        assert_eq!(show(&fx, "global rules/x.md").unwrap(), "committed\n");
        assert_eq!(
            show(&fx, "global").unwrap(),
            "== rules/x.md ==\ncommitted\n"
        );
    }

    #[test]
    fn inadmissible_file_is_not_printed() {
        let fx = fixture(&[
            ("rules/x.md", "tag\u{E0041}\n".as_bytes()),
            ("rules/Bad.md", b"hi\n"),
        ]);
        let one = show(&fx, "global rules/x.md").unwrap();
        assert!(
            one.starts_with("(rules/x.md: not an admissible instruction file: ")
                && one.contains("E0041")
                && !one.contains("tag"),
            "{one}"
        );
        let all = show(&fx, "global").unwrap();
        assert!(all.contains("== rules/Bad.md ==\n(rules/Bad.md: not an admissible instruction file: invalid instruction file name"), "{all}");
        assert!(!all.contains("hi\n"), "{all}");
    }

    #[test]
    fn instructions_show_parses() {
        crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "show", "seat", "a1"])
            .unwrap();
        assert!(crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "show"]).is_err());
    }
}
