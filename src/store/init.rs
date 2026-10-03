//! Instance creation (`herdr-graph init`) and the user-level config pointer (spec §1).
use super::git::graph_signature;
use crate::model::graph::{GraphDefaults, GraphRecord};
use crate::model::{CommitId, SCHEMA_VERSION};
use crate::ports::store::StoreError;
use git2::{Repository, RepositoryInitOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

fn git_err(e: git2::Error) -> StoreError {
    StoreError::Git(e.to_string())
}

/// Create a graph instance at `path` (spec §1, §2.3). Refuses an existing instance (graph.toml present) with
/// StoreError::Io(AlreadyExists). Creates a git repo with initial branch `main`, graph.toml, .gitignore
/// (`/.graph-local/`), placeholder dirs and the ignored `.graph-local/`; commits on refs/heads/main.
pub fn init_instance(path: &Path) -> Result<CommitId, StoreError> {
    if path.join("graph.toml").exists() {
        return Err(StoreError::Io(std::io::Error::new(
            ErrorKind::AlreadyExists,
            format!("{} is already a graph instance", path.display()),
        )));
    }
    std::fs::create_dir_all(path)?;
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(path, &opts).map_err(git_err)?;

    let graph = GraphRecord {
        schema: SCHEMA_VERSION,
        instance_id: ulid::Ulid::new().to_string(),
        schema_version: 1,
        summarizer_seat: None,
        defaults: GraphDefaults::default(),
    };
    let graph_toml = toml::to_string(&graph).map_err(|e| StoreError::Corrupt {
        path: "graph.toml".into(),
        reason: e.to_string(),
    })?;
    std::fs::write(path.join("graph.toml"), graph_toml)?;
    std::fs::write(path.join(".gitignore"), "/.graph-local/\n")?;
    let mut files = vec!["graph.toml", ".gitignore"];
    for dir in ["teamspaces", "templates", "rules"] {
        std::fs::create_dir_all(path.join(dir))?;
        std::fs::write(path.join(dir).join(".gitkeep"), "")?;
    }
    files.extend([
        "teamspaces/.gitkeep",
        "templates/.gitkeep",
        "rules/.gitkeep",
    ]);
    std::fs::create_dir_all(path.join(".graph-local"))?;

    let mut index = repo.index().map_err(git_err)?;
    for f in files {
        index.add_path(Path::new(f)).map_err(git_err)?;
    }
    index.write().map_err(git_err)?;
    let tree = repo
        .find_tree(index.write_tree().map_err(git_err)?)
        .map_err(git_err)?;
    let sig = graph_signature().map_err(git_err)?;
    let oid = repo
        .commit(
            Some("refs/heads/main"),
            &sig,
            &sig,
            "init: herdr-graph instance",
            &tree,
            &[],
        )
        .map_err(git_err)?;
    Ok(CommitId(oid.to_string()))
}

#[derive(Debug, PartialEq, Eq)]
pub enum UserConfigOutcome {
    Written(PathBuf),
    AlreadyPointsHere(PathBuf),
    LeftExisting(PathBuf),
}

/// Write `<home>/.config/herdr-graph/config.toml` with `instance = "<abs path>"` unless one exists
/// (spec §1 locate chain, last link). Never overwrites an existing config pointing elsewhere.
pub fn write_user_config(home: &Path, instance: &Path) -> std::io::Result<UserConfigOutcome> {
    let cfg = crate::config::user_config_path(home);
    if cfg.exists() {
        let points_here = std::fs::read_to_string(&cfg)
            .ok()
            .and_then(|t| t.parse::<toml::Table>().ok())
            .and_then(|t| {
                t.get("instance")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
            })
            .is_some_and(|p| p == instance);
        return Ok(if points_here {
            UserConfigOutcome::AlreadyPointsHere(cfg)
        } else {
            UserConfigOutcome::LeftExisting(cfg)
        });
    }
    std::fs::create_dir_all(cfg.parent().expect("config path has a parent"))?;
    let mut table = toml::Table::new();
    table.insert(
        "instance".into(),
        toml::Value::String(instance.to_string_lossy().into_owned()),
    );
    let text = toml::to_string(&table).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(&cfg, text)?;
    Ok(UserConfigOutcome::Written(cfg))
}
