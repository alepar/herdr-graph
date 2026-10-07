//! Shipped example templates and `init --with-examples` (spec §9).
//!
//! `init` runs before any daemon exists, so the templates are written straight into the instance and
//! committed (a second commit, `init: example templates`), the same way `init_instance` makes the first.
use crate::model::CommitId;
use crate::ports::store::StoreError;
use crate::store::git::graph_signature;
use git2::Repository;
use std::io::ErrorKind;
use std::path::Path;

/// Where the examples live in an instance.
pub const EXAMPLES_DIR: &str = "templates";

/// (instance-relative path, content): the template records and their member instructions.
pub const EXAMPLE_FILES: &[(&str, &str)] = &[
    (
        "templates/project-team/members/engineer/AGENTS.md",
        include_str!("../../templates/project-team/members/engineer/AGENTS.md"),
    ),
    (
        "templates/engineer/template.toml",
        include_str!("../../templates/engineer/template.toml"),
    ),
    (
        "templates/engineer/AGENTS.md",
        include_str!("../../templates/engineer/AGENTS.md"),
    ),
    (
        "templates/reviewer/template.toml",
        include_str!("../../templates/reviewer/template.toml"),
    ),
    (
        "templates/reviewer/AGENTS.md",
        include_str!("../../templates/reviewer/AGENTS.md"),
    ),
    (
        "templates/researcher/template.toml",
        include_str!("../../templates/researcher/template.toml"),
    ),
    (
        "templates/researcher/AGENTS.md",
        include_str!("../../templates/researcher/AGENTS.md"),
    ),
    (
        "templates/designer/template.toml",
        include_str!("../../templates/designer/template.toml"),
    ),
    (
        "templates/designer/AGENTS.md",
        include_str!("../../templates/designer/AGENTS.md"),
    ),
    (
        "templates/system-summarizer/template.toml",
        include_str!("../../templates/system-summarizer/template.toml"),
    ),
    (
        "templates/system-summarizer/members/summarizer/AGENTS.md",
        include_str!("../../templates/system-summarizer/members/summarizer/AGENTS.md"),
    ),
    (
        "templates/project-team/template.toml",
        include_str!("../../templates/project-team/template.toml"),
    ),
    (
        "templates/project-team/members/foreman/AGENTS.md",
        include_str!("../../templates/project-team/members/foreman/AGENTS.md"),
    ),
    (
        "templates/project-team/members/researcher/AGENTS.md",
        include_str!("../../templates/project-team/members/researcher/AGENTS.md"),
    ),
    (
        "templates/feature-team/template.toml",
        include_str!("../../templates/feature-team/template.toml"),
    ),
    (
        "templates/feature-team/members/engineer/AGENTS.md",
        include_str!("../../templates/feature-team/members/engineer/AGENTS.md"),
    ),
    (
        "templates/feature-team/members/reviewer/AGENTS.md",
        include_str!("../../templates/feature-team/members/reviewer/AGENTS.md"),
    ),
];

fn git_err(e: git2::Error) -> StoreError {
    StoreError::Git(e.to_string())
}

/// Copy the example templates into `instance` and commit them on `main`. Refuses when any target file
/// already exists, so an instance's own templates are never overwritten.
pub fn install_examples(instance: &Path) -> Result<CommitId, StoreError> {
    for (rel, _) in EXAMPLE_FILES {
        if instance.join(rel).exists() {
            return Err(StoreError::Io(std::io::Error::new(
                ErrorKind::AlreadyExists,
                format!("{} already exists in {}", rel, instance.display()),
            )));
        }
    }
    let repo = Repository::open(instance).map_err(git_err)?;
    for (rel, text) in EXAMPLE_FILES {
        let target = instance.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, text)?;
    }
    let mut index = repo.index().map_err(git_err)?;
    for (rel, _) in EXAMPLE_FILES {
        index.add_path(Path::new(rel)).map_err(git_err)?;
    }
    index.write().map_err(git_err)?;
    let tree = repo
        .find_tree(index.write_tree().map_err(git_err)?)
        .map_err(git_err)?;
    let parent = repo
        .head()
        .map_err(git_err)?
        .peel_to_commit()
        .map_err(git_err)?;
    let sig = graph_signature().map_err(git_err)?;
    let oid = repo
        .commit(
            Some("refs/heads/main"),
            &sig,
            &sig,
            "init: example templates",
            &tree,
            &[&parent],
        )
        .map_err(git_err)?;
    Ok(CommitId(oid.to_string()))
}
