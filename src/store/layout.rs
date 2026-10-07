//! Instance path layout (spec §2.3), enumerators and id → location resolution.
use super::record::{Record, parse_toml, read_toml};
use super::tree::TreeRead;
use crate::model::action::ActionRecord;
use crate::model::application::ApplicationRecord;
use crate::model::clone::CloneRecord;
use crate::model::graph::GraphRecord;
use crate::model::operation::OperationRecord;
use crate::model::request::ProcessingRequest;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::template::TemplateRecord;
use crate::model::transcript::TranscriptRecord;
use crate::model::{
    ActionId, AnyId, AppId, IdKind, OpId, RequestId, SeatId, TeamspaceId, Timestamp, TranscriptId,
};
use crate::ports::store::{EntryKind, ObjectLocation, RepoPath, StoreError};
use std::collections::BTreeSet;

fn p(s: &str) -> RepoPath {
    RepoPath::new(s).expect("static layout path is valid")
}
fn j(dir: &RepoPath, rest: &str) -> RepoPath {
    dir.join(rest).expect("layout path component is valid")
}

pub fn graph_toml() -> RepoPath {
    p("graph.toml")
}
pub fn teamspaces_root() -> RepoPath {
    p("teamspaces")
}
pub fn teamspace_dir(slug: &str) -> RepoPath {
    j(&teamspaces_root(), slug)
}
pub fn archived_teamspace_dir(slug: &str, id: &TeamspaceId) -> RepoPath {
    p(&format!(
        "archive/teamspaces/{slug}-{}",
        id.suffix6().to_ascii_lowercase()
    ))
}
pub fn teamspace_record(dir: &RepoPath) -> RepoPath {
    j(dir, "teamspace.toml")
}
pub fn seat_dir(ts_dir: &RepoPath, slug: &str) -> RepoPath {
    j(ts_dir, &format!("seats/{slug}"))
}
pub fn archived_seat_dir(ts_dir: &RepoPath, slug: &str, id: &SeatId) -> RepoPath {
    j(
        ts_dir,
        &format!("archive/seats/{slug}-{}", id.suffix6().to_ascii_lowercase()),
    )
}
pub fn seat_record(dir: &RepoPath) -> RepoPath {
    j(dir, "seat.toml")
}
pub fn clone_dir(seat_dir: &RepoPath, slug: &str) -> RepoPath {
    j(seat_dir, &format!("clones/{slug}"))
}
pub fn clone_record(dir: &RepoPath) -> RepoPath {
    j(dir, "clone.toml")
}
pub fn template_dir(slug: &str) -> RepoPath {
    p(&format!("templates/{slug}"))
}
pub fn template_record(dir: &RepoPath) -> RepoPath {
    j(dir, "template.toml")
}
pub fn member_agents_md(tpl_dir: &RepoPath, member_slug: &str) -> RepoPath {
    j(tpl_dir, &format!("members/{member_slug}/AGENTS.md"))
}
pub fn application_record(id: &AppId) -> RepoPath {
    p(&format!("applications/{id}.toml"))
}
/// Legacy destination, retained for compatibility fixtures and historical records.
/// New registrations must use `transcript_registration_record`.
pub fn transcript_record(seat: &SeatId, tr: &TranscriptId) -> RepoPath {
    p(&format!("transcripts/{seat}/{tr}.toml"))
}
/// Names are fixed at registration; full stable IDs preserve identity across renames and moves.
pub fn transcript_registration_record(
    team_name: &str,
    team: &TeamspaceId,
    seat_name: &str,
    seat: &SeatId,
    tr: &TranscriptId,
) -> RepoPath {
    p(&format!(
        "transcripts/{}-{team}/{}-{seat}/{tr}.toml",
        super::slug::slugify(team_name),
        super::slug::slugify(seat_name),
    ))
}

pub fn request_record(rq: &RequestId) -> RepoPath {
    p(&format!("mutations/transcript-processing/{rq}.toml"))
}
pub fn action_record(at: Timestamp, act: &ActionId) -> RepoPath {
    p(&format!(
        "mutations/undoable-actions/{}/{act}.toml",
        at.format("%Y-%m")
    ))
}
pub fn operation_record(at: Timestamp, op: &OpId) -> RepoPath {
    p(&format!(
        "mutations/graph-changes/{}/{op}.toml",
        at.format("%Y-%m")
    ))
}

type Listed<R> = Result<Vec<(ObjectLocation, R)>, StoreError>;

fn sub_dirs(tr: &dyn TreeRead, parent: &RepoPath) -> Result<Vec<RepoPath>, StoreError> {
    let mut out = Vec::new();
    for e in tr.list_dir(parent)? {
        if e.kind == EntryKind::Dir {
            out.push(parent.join(&e.name)?);
        }
    }
    Ok(out)
}

fn sub_files(tr: &dyn TreeRead, parent: &RepoPath) -> Result<Vec<RepoPath>, StoreError> {
    let mut out = Vec::new();
    for e in tr.list_dir(parent)? {
        if e.kind == EntryKind::File && e.name.ends_with(".toml") {
            out.push(parent.join(&e.name)?);
        }
    }
    Ok(out)
}

fn loc_of<R: Record>(path: RepoPath, folder: RepoPath, rec: &R) -> ObjectLocation {
    ObjectLocation {
        id: rec.any_id(),
        record_path: path,
        folder,
    }
}

/// Folder records: every `<parent>/*/<file>`; subdirs without the record file are ignored.
fn folder_records<R: Record>(tr: &dyn TreeRead, parents: &[RepoPath], file: &str) -> Listed<R> {
    let mut out = Vec::new();
    for parent in parents {
        for dir in sub_dirs(tr, parent)? {
            let rec_path = dir.join(file)?;
            if let Some(rec) = read_toml::<R>(tr, &rec_path)? {
                out.push((loc_of(rec_path, dir, &rec), rec));
            }
        }
    }
    out.sort_by(|a, b| a.0.record_path.cmp(&b.0.record_path));
    Ok(out)
}

/// File records: every `<dir>/*.toml` for each dir.
fn file_records<R: Record>(tr: &dyn TreeRead, dirs: &[RepoPath]) -> Listed<R> {
    let mut out = Vec::new();
    for dir in dirs {
        for path in sub_files(tr, dir)? {
            if let Some(rec) = read_toml::<R>(tr, &path)? {
                out.push((loc_of(path, dir.clone(), &rec), rec));
            }
        }
    }
    out.sort_by(|a, b| a.0.record_path.cmp(&b.0.record_path));
    reject_duplicate_ids(&out)?;
    Ok(out)
}

fn reject_duplicate_ids<R>(records: &[(ObjectLocation, R)]) -> Result<(), StoreError> {
    let mut seen = std::collections::BTreeMap::new();
    for (loc, _) in records {
        if let Some(first) = seen.insert(loc.id.clone(), &loc.record_path) {
            return Err(StoreError::Corrupt {
                path: loc.record_path.as_str().to_owned(),
                reason: format!("duplicate ID {} also at {}", loc.id, first.as_str()),
            });
        }
    }
    Ok(())
}

pub fn list_teamspaces(tr: &dyn TreeRead) -> Listed<TeamspaceRecord> {
    folder_records(
        tr,
        &[teamspaces_root(), p("archive/teamspaces")],
        "teamspace.toml",
    )
}
pub fn list_seats(tr: &dyn TreeRead, ts_dir: &RepoPath) -> Listed<SeatRecord> {
    folder_records(
        tr,
        &[j(ts_dir, "seats"), j(ts_dir, "archive/seats")],
        "seat.toml",
    )
}
pub fn all_seats(tr: &dyn TreeRead) -> Listed<SeatRecord> {
    let mut out = Vec::new();
    for (loc, _) in list_teamspaces(tr)? {
        out.extend(list_seats(tr, &loc.folder)?);
    }
    out.sort_by(|a, b| a.0.record_path.cmp(&b.0.record_path));
    Ok(out)
}
pub fn list_clones(tr: &dyn TreeRead, seat_dir: &RepoPath) -> Listed<CloneRecord> {
    folder_records(tr, &[j(seat_dir, "clones")], "clone.toml")
}
pub fn all_clones(tr: &dyn TreeRead) -> Listed<CloneRecord> {
    let mut out = Vec::new();
    for (loc, _) in all_seats(tr)? {
        out.extend(list_clones(tr, &loc.folder)?);
    }
    out.sort_by(|a, b| a.0.record_path.cmp(&b.0.record_path));
    Ok(out)
}
pub fn list_templates(tr: &dyn TreeRead) -> Listed<TemplateRecord> {
    folder_records(tr, &[p("templates")], "template.toml")
}
pub fn list_applications(tr: &dyn TreeRead) -> Listed<ApplicationRecord> {
    file_records(tr, &[p("applications")])
}
// Both legacy seat directories and the new team/seat directories are index roots.
fn transcript_dirs(tr: &dyn TreeRead) -> Result<Vec<RepoPath>, StoreError> {
    let mut dirs = sub_dirs(tr, &p("transcripts"))?;
    for team in dirs.clone() {
        dirs.extend(sub_dirs(tr, &team)?);
    }
    Ok(dirs)
}

fn dated_dirs(tr: &dyn TreeRead, legacy: &str, current: &str) -> Result<Vec<RepoPath>, StoreError> {
    let mut dirs = sub_dirs(tr, &p(legacy))?;
    dirs.extend(sub_dirs(tr, &p(current))?);
    Ok(dirs)
}

pub fn list_transcripts(tr: &dyn TreeRead) -> Listed<TranscriptRecord> {
    file_records(tr, &transcript_dirs(tr)?)
}
pub fn list_requests(tr: &dyn TreeRead) -> Listed<ProcessingRequest> {
    file_records(tr, &[p("requests"), p("mutations/transcript-processing")])
}
pub fn list_actions(tr: &dyn TreeRead) -> Listed<ActionRecord> {
    file_records(
        tr,
        &dated_dirs(tr, "actions", "mutations/undoable-actions")?,
    )
}
pub fn list_operations(tr: &dyn TreeRead) -> Listed<OperationRecord> {
    file_records(
        tr,
        &dated_dirs(tr, "operations", "mutations/graph-changes")?,
    )
}

pub fn read_graph(tr: &dyn TreeRead) -> Result<GraphRecord, StoreError> {
    let path = graph_toml();
    let bytes = tr.read_file(&path)?.ok_or_else(|| StoreError::Corrupt {
        path: path.as_str().into(),
        reason: "missing".into(),
    })?;
    parse_toml(&path, &bytes)
}

fn find_by_id<R: Record>(l: Listed<R>, id: &AnyId) -> Result<Option<ObjectLocation>, StoreError> {
    Ok(l?.into_iter().map(|(loc, _)| loc).find(|loc| &loc.id == id))
}

/// First `<dir>/<id>.toml` that exists, for each dir in `dirs`.
fn find_file(
    tr: &dyn TreeRead,
    dirs: &[RepoPath],
    id: &AnyId,
) -> Result<Option<ObjectLocation>, StoreError> {
    for dir in dirs {
        let path = dir.join(&format!("{id}.toml"))?;
        if tr.read_file(&path)?.is_some() {
            return Ok(Some(ObjectLocation {
                id: id.clone(),
                record_path: path,
                folder: dir.clone(),
            }));
        }
    }
    Ok(None)
}

/// Resolve any id to its current location (spec §2.1: paths are never cached as authority).
/// Operational records scan both current and legacy layouts and reject duplicate IDs.
/// app_ uses its direct path; other standalone objects scan their supported roots.
/// mem_/ns_/ef_/pl_ → Ok(None) (not standalone objects). `folder` = object dir (folder records) or the file's parent.
pub fn locate(tr: &dyn TreeRead, id: &AnyId) -> Result<Option<ObjectLocation>, StoreError> {
    match id.kind() {
        IdKind::Teamspace => find_by_id(list_teamspaces(tr), id),
        IdKind::Seat => find_by_id(all_seats(tr), id),
        IdKind::Clone => find_by_id(all_clones(tr), id),
        IdKind::Template => find_by_id(list_templates(tr), id),
        IdKind::Application => find_file(tr, &[p("applications")], id),
        IdKind::Request => find_by_id(list_requests(tr), id),
        IdKind::Transcript => find_by_id(list_transcripts(tr), id),
        IdKind::Action => find_by_id(list_actions(tr), id),
        IdKind::Operation => find_by_id(list_operations(tr), id),
        IdKind::NativeSession | IdKind::Member | IdKind::Effect | IdKind::Plan => Ok(None),
    }
}

/// Sibling slugs already taken under a dir (for unique_slug): the names of its subdirectories.
pub fn taken_slugs(tr: &dyn TreeRead, parent: &RepoPath) -> Result<BTreeSet<String>, StoreError> {
    Ok(tr
        .list_dir(parent)?
        .into_iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| e.name)
        .collect())
}
