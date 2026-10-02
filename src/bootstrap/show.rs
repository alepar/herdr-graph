//! `show`, `list`, `path`: direct reads of the committed graph, no daemon needed (spec §9).
//! Paths printed are absolute working-tree paths of the committed revision's locations.
use crate::cli::template::render_template;
use crate::model::template::TemplateRecord;
use crate::model::{AnyId, IdKind};
use crate::plan::grammar;
use crate::ports::store::{EntryKind, ObjectLocation, RepoPath, StoreError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ShowError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Invalid(String),
}

/// One `list` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub lifecycle: String,
    pub path: PathBuf,
}

impl Row {
    /// `id  name  lifecycle  path`.
    pub fn render(&self) -> String {
        format!("{}  {}  {}  {}", self.id, self.name, self.lifecycle, self.path.display())
    }
}

pub const KINDS: &[&str] = &["teamspaces", "seats", "clones", "applications", "templates"];

fn lc<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| "-".into())
}

/// The path of the object's folder for folder objects, of its record file otherwise.
fn object_path(root: &Path, loc: &ObjectLocation) -> PathBuf {
    let folder_object = matches!(loc.id.kind(), IdKind::Teamspace | IdKind::Seat | IdKind::Clone | IdKind::Template);
    root.join(if folder_object { loc.folder.as_str() } else { loc.record_path.as_str() })
}

/// Rows of the listed kinds (`None` = teamspaces and seats), each kind in tree order.
pub fn list(tree: &dyn TreeRead, root: &Path, kind: Option<&str>) -> Result<Vec<Row>, ShowError> {
    let kinds: Vec<&str> = match kind {
        None => vec!["teamspaces", "seats"],
        Some(k) if KINDS.contains(&k) => vec![k],
        Some(k) => {
            return Err(ShowError::Invalid(format!("unknown kind {k:?}; expected one of {}", KINDS.join(", "))));
        }
    };
    let mut rows = Vec::new();
    for k in kinds {
        match k {
            "teamspaces" => {
                for (loc, r) in layout::list_teamspaces(tree)? {
                    rows.push(row(root, &loc, &r.name, lc(&r.lifecycle)));
                }
            }
            "seats" => {
                for (loc, r) in layout::all_seats(tree)? {
                    rows.push(row(root, &loc, &r.name, lc(&r.lifecycle)));
                }
            }
            "clones" => {
                for (loc, r) in layout::all_clones(tree)? {
                    rows.push(row(root, &loc, &r.name, lc(&r.lifecycle)));
                }
            }
            "applications" => {
                for (loc, r) in layout::list_applications(tree)? {
                    rows.push(row(root, &loc, &r.name, lc(&r.lifecycle)));
                }
            }
            _ => {
                for (loc, r) in layout::list_templates(tree)? {
                    rows.push(row(root, &loc, &r.name, "-".into()));
                }
            }
        }
    }
    Ok(rows)
}

fn row(root: &Path, loc: &ObjectLocation, name: &str, lifecycle: String) -> Row {
    Row { id: loc.id.to_string(), name: name.to_owned(), lifecycle, path: object_path(root, loc) }
}

/// Resolve an id, or a unique live name, to its current location.
fn locate_target(tree: &dyn TreeRead, target: &str) -> Result<ObjectLocation, ShowError> {
    let id = match AnyId::parse(target) {
        Ok(id) => id,
        Err(_) => grammar::resolve_object(tree, target).map_err(|e| ShowError::Invalid(e.to_string()))?,
    };
    layout::locate(tree, &id)?.ok_or_else(|| ShowError::Invalid(format!("no object {id}")))
}

/// The absolute working-tree path of the object's current folder (record file for file objects).
pub fn path(tree: &dyn TreeRead, root: &Path, id: &str) -> Result<PathBuf, ShowError> {
    Ok(object_path(root, &locate_target(tree, id)?))
}

/// `show <id|path>`: the record's TOML (a template as its summary), or the committed file or directory at
/// an instance-relative path.
pub fn show(tree: &dyn TreeRead, root: &Path, target: &str) -> Result<String, ShowError> {
    if AnyId::parse(target).is_err()
        && let Some(text) = show_path(tree, root, target)?
    {
        return Ok(text);
    }
    let loc = locate_target(tree, target)?;
    let bytes = tree
        .read_file(&loc.record_path)?
        .ok_or_else(|| ShowError::Invalid(format!("record {} is missing", loc.record_path.as_str())))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| ShowError::Invalid(format!("{} is not UTF-8: {e}", loc.record_path.as_str())))?;
    if loc.id.kind() == IdKind::Template
        && let Some(rec) = read_toml::<TemplateRecord>(tree, &loc.record_path)?
    {
        return Ok(render_template(&rec));
    }
    Ok(text)
}

/// A committed file or directory at `target` (instance-relative, or absolute under `root`).
fn show_path(tree: &dyn TreeRead, root: &Path, target: &str) -> Result<Option<String>, ShowError> {
    let rel = match Path::new(target).strip_prefix(root) {
        Ok(r) => r.to_string_lossy().into_owned(),
        Err(_) => target.to_owned(),
    };
    let Ok(p) = RepoPath::new(&rel) else { return Ok(None) };
    if let Some(bytes) = tree.read_file(&p)? {
        return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
    }
    let entries = tree.list_dir(&p)?;
    if entries.is_empty() {
        return Ok(None);
    }
    let mut out = String::new();
    for e in entries {
        out.push_str(&e.name);
        if e.kind == EntryKind::Dir {
            out.push('/');
        }
        out.push('\n');
    }
    Ok(Some(out))
}
