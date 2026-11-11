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
    /// The seat's instruction manifest at HEAD, hook-cap fit, harness files and per-clone loaded status (no daemon).
    Stack {
        /// Seat (name or id) whose instruction stack to show; any seat, not only the caller's.
        seat: String,
        /// Only this clone's loaded status (name or id).
        #[arg(long)]
        clone: Option<String>,
        /// Print one JSON document (schema 1) on stdout; notices go to stderr.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Some(root) = instance_or_none() else {
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let store = crate::store::GitStore::open(&root)?;
    let Commands::Instructions { action } = cmd;
    if let InstructionsAction::Stack { seat, clone, json } = action {
        return Ok(
            match stack(
                &store,
                &root,
                &seat,
                clone.as_deref(),
                json,
                &mut std::io::stdout().lock(),
                &mut std::io::stderr().lock(),
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => crate::inspect::fail(&e),
            },
        );
    }
    let view = CommitView {
        store: &store,
        at: store.head()?,
    };
    match action {
        InstructionsAction::Stack { .. } => unreachable!("handled above"),
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

/// `instructions stack`: pins HEAD once, resolves the seat (and clone) at that commit and prints the view.
fn stack(
    store: &crate::store::GitStore,
    instance: &std::path::Path,
    seat: &str,
    clone: Option<&str>,
    json: bool,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> anyhow::Result<()> {
    use crate::inspect::scope::{AllowClone, Scope, ScopeArg};
    let head = crate::inspect::pin_head(store)?;
    let words = |kind: &str, name: &str| vec![kind.to_owned(), name.to_owned()];
    let Scope::Seat(seat_id) = Scope::resolve(
        &ScopeArg::parse(&words("seat", seat), AllowClone::No)?,
        store,
        &head,
    )?
    else {
        unreachable!("a seat word resolves to a seat scope")
    };
    let clone_id = match clone {
        None => None,
        Some(c) => {
            let arg = ScopeArg::parse(&words("clone", c), AllowClone::Yes)?;
            let Scope::Clone(id) = Scope::resolve(&arg, store, &head)? else {
                unreachable!("a clone word resolves to a clone scope")
            };
            Some(id)
        }
    };
    let view = crate::inspect::stack::build(
        store,
        &head,
        &seat_id,
        clone_id.as_ref(),
        &crate::inspect::harness_files::UserDirs::from_env(),
        instance,
    )?;
    crate::inspect::output::emit(
        &crate::inspect::output::Envelope::new(&head, view),
        crate::inspect::output::Format::from_flag(json),
        &[],
        out,
        err,
    )?;
    Ok(())
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
            default_manager: None,
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
            manager: None,
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

    fn snapshot(store: &GitStore) -> (String, Vec<(String, String)>, Vec<(String, u32)>) {
        use crate::ports::store::Store;
        let head = store.head().unwrap().0;
        store
            .with_repo(|r| {
                let idx = r.index()?;
                let entries = idx
                    .iter()
                    .map(|e| {
                        (
                            String::from_utf8_lossy(&e.path).into_owned(),
                            e.id.to_string(),
                        )
                    })
                    .collect();
                let st = r
                    .statuses(None)?
                    .iter()
                    .map(|e| (e.path().unwrap_or("").to_owned(), e.status().bits()))
                    .collect();
                Ok((head.clone(), entries, st))
            })
            .unwrap()
    }

    #[test]
    fn stack_is_read_only() {
        let fx = fixture(&[("teamspaces/alpha/seats/a1/AGENTS.md", b"seat agents\n")]);
        let before = snapshot(&fx.store);
        for json in [false, true] {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            stack(&fx.store, &fx.dir, "a1", None, json, &mut out, &mut err).unwrap();
            assert_eq!(snapshot(&fx.store), before, "json={json}");
            let text = String::from_utf8(out).unwrap();
            if json {
                let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(v["schema"], 1);
                assert_eq!(v["head"], before.0.as_str());
                assert_eq!(v["parts"][0]["path"], "teamspaces/alpha/seats/a1/AGENTS.md");
                assert_eq!(text.trim_end().lines().count(), 1);
                assert!(err.is_empty());
            } else {
                assert!(text.starts_with("seat a1 ("), "{text}");
                assert!(text.contains("path last touched"), "{text}");
            }
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let e = stack(
            &fx.store, &fx.dir, "nosuch", None, false, &mut out, &mut err,
        )
        .unwrap_err();
        assert!(e.to_string().contains("nosuch"), "{e}");
    }

    #[test]
    fn instructions_stack_parses() {
        crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "stack", "a1", "--json"])
            .unwrap();
        assert!(crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "stack"]).is_err());
    }

    #[test]
    fn instructions_show_parses() {
        crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "show", "seat", "a1"])
            .unwrap();
        assert!(crate::cli::Cli::try_parse_from(["herdr-graph", "instructions", "show"]).is_err());
    }
}
