//! `content write`: opaque files in an object's folder, through the writer (spec §3.5).
//!
//! The folder is resolved when the op APPLIES, never when it is asked for, so a rename in between does not
//! matter; an object that does not resolve rejects the op. Record files are never writable this way, and a
//! retired object's archived folder accepts only `summaries/…` (late summaries of a finished seat).
use crate::daemon::registry::CallerInfo;
use crate::daemon::registry::CommandError;
use crate::model::change::{ChangeRequest, ReliedOn, RequestKind, Requester, Version};
use crate::model::{AnyId, BlobHash, CloneId, IdKind, OpId, SeatId};
use crate::plan::commands::PlanDeps;
use crate::ports::store::{EntryKind, RepoPath};
use crate::store::tree::TreeRead;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, Reject};
use serde_json::{Value, json};

/// Writer mutation key for `RequestKind::ContentWrite` without a `sub`.
pub const MUTATION_KEY: &str = "content_write";
/// The only prefix a retired object accepts.
pub const RETIRED_ALLOWED_PREFIX: &str = "summaries/";
/// Record files: written only by organizational kinds.
const RECORD_FILES: &[&str] = &[
    "teamspace.toml",
    "seat.toml",
    "clone.toml",
    "template.toml",
    "graph.toml",
];

/// A `--rel` value that is safe to join under an object's folder: relative, no `.`/`..`/empty parts, no
/// backslash, not inside `.git`, and not a record file at any depth (nested objects live below their parents).
pub fn validate_rel(rel: &str) -> Result<String, String> {
    if rel.is_empty() {
        return Err("rel is empty".into());
    }
    if rel.starts_with('/') {
        return Err(format!(
            "rel {rel:?} must be relative to the object's folder"
        ));
    }
    if rel.contains('\\') || rel.contains('\0') {
        return Err(format!("rel {rel:?} contains a forbidden character"));
    }
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(format!("rel {rel:?} has an empty, `.` or `..` component"));
        }
        if part == ".git" {
            return Err(format!("rel {rel:?} points into .git"));
        }
    }
    let file = rel.rsplit('/').next().unwrap_or(rel);
    if RECORD_FILES.contains(&file) {
        return Err(format!(
            "{file} is a record file; change it with plan/apply, not content write"
        ));
    }
    Ok(rel.to_owned())
}

fn reject(reason: &str, explanation: impl Into<String>) -> MutationError {
    MutationError::Reject(Reject {
        reason: reason.into(),
        explanation: explanation.into(),
        current_revs: vec![],
    })
}

/// A folder is archived when any component is `archive` (retired teamspaces and seats move there).
fn archived(folder: &RepoPath) -> bool {
    folder.as_str().split('/').any(|c| c == "archive")
}

pub struct ContentWrite;

impl Mutation for ContentWrite {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let args = &cx.request.args;
        let object = args
            .get("object")
            .and_then(Value::as_str)
            .ok_or_else(|| MutationError::Bug("content_write without object".into()))
            .and_then(|s| AnyId::parse(s).map_err(|e| MutationError::Bug(e.to_string())))?;
        let rel = args
            .get("rel")
            .and_then(Value::as_str)
            .ok_or_else(|| MutationError::Bug("content_write without rel".into()))?;
        let bytes = args
            .get("bytes_b64")
            .and_then(Value::as_str)
            .ok_or_else(|| MutationError::Bug("content_write without bytes_b64".into()))
            .and_then(|s| b64_decode(s).map_err(MutationError::Bug))?;
        let rel = validate_rel(rel).map_err(|why| reject("invalid_path", why))?;

        let Some(loc) = cx.tree.locate(&object)? else {
            return Err(reject(
                "unknown_object",
                format!("{object} does not resolve to a live or archived object"),
            ));
        };
        if !matches!(
            object.kind(),
            IdKind::Teamspace | IdKind::Seat | IdKind::Clone | IdKind::Template
        ) {
            return Err(reject(
                "not_a_folder_object",
                format!("{object} has no folder of its own"),
            ));
        }
        let retired = archived(&loc.folder)
            || crate::store::record::read_toml::<toml::Table>(&cx.tree, &loc.record_path)?
                .and_then(|t| {
                    t.get("lifecycle")
                        .and_then(|v| v.as_str())
                        .map(|l| l == "retired")
                })
                .unwrap_or(false);
        if retired && !rel.starts_with(RETIRED_ALLOWED_PREFIX) {
            return Err(reject(
                "retired_object",
                format!(
                    "{object} is retired; only {RETIRED_ALLOWED_PREFIX}… may be written, not {rel}"
                ),
            ));
        }

        let target = loc.folder.join(&rel)?;
        // A file cannot become a directory or the reverse.
        let mut prefix = loc.folder.clone();
        let parts: Vec<&str> = rel.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            prefix = prefix.join(part)?;
            if cx.tree.read_file(&prefix)?.is_some() {
                return Err(reject(
                    "invalid_path",
                    format!("{} is a file, not a directory", prefix.as_str()),
                ));
            }
        }
        if cx.tree.list_dir(&target)?.iter().any(|e| {
            matches!(
                e.kind,
                EntryKind::File | EntryKind::Dir | EntryKind::Symlink
            )
        }) {
            return Err(reject("invalid_path", format!("{rel} is a directory")));
        }
        cx.tree.put_file(target, bytes);
        Ok(Applied {
            summary: format!("content write {object} {rel}"),
            action: None,
        })
    }
}

/// Admit a `content.write` request: best-effort validation, then the writer queue. The writer re-resolves
/// the object and re-validates when it applies the op.
pub fn admit_write(
    deps: &PlanDeps,
    caller: &CallerInfo,
    args: &Value,
) -> Result<OpId, CommandError> {
    let object = args
        .get("object")
        .and_then(Value::as_str)
        .ok_or_else(|| CommandError::bad_request("content.write needs {object}"))?;
    let object =
        AnyId::parse(object).map_err(|e| CommandError::bad_request(format!("object: {e}")))?;
    let rel = args
        .get("rel")
        .and_then(Value::as_str)
        .ok_or_else(|| CommandError::bad_request("content.write needs {rel}"))?;
    let rel = validate_rel(rel).map_err(CommandError::bad_request)?;
    let bytes_b64 = args
        .get("bytes_b64")
        .and_then(Value::as_str)
        .ok_or_else(|| CommandError::bad_request("content.write needs {bytes_b64}"))?;
    b64_decode(bytes_b64).map_err(|e| CommandError::bad_request(format!("bytes_b64: {e}")))?;
    let relied_on = match args.get("expect").and_then(Value::as_str) {
        Some(h) if !h.is_empty() => vec![ReliedOn {
            object: object.clone(),
            version: Version::Blob(BlobHash(h.to_owned())),
        }],
        _ => vec![],
    };
    let req = ChangeRequest {
        kind: RequestKind::ContentWrite,
        // No "sub": the mutation key is "content_write". `path` names the file for the blob precondition.
        args: json!({ "object": object, "rel": rel, "bytes_b64": bytes_b64, "path": rel }),
        relied_on,
        requester: requester_for(caller),
        supersedes: None,
        confirmed: None,
    };
    deps.writer.admit(req).map_err(|e| match e {
        crate::ports::writer::WriterError::Invalid(m) => CommandError::bad_request(m),
        other => CommandError::unavailable(other.to_string()),
    })
}

fn requester_for(caller: &CallerInfo) -> Requester {
    Requester {
        teamspace: None,
        seat: caller
            .graph_seat
            .as_deref()
            .and_then(|s| s.parse::<SeatId>().ok()),
        clone: caller
            .graph_clone
            .as_deref()
            .and_then(|s| s.parse::<CloneId>().ok()),
        native_session: None,
        human: caller.tty,
    }
}

// ---------------------------------------------------------------------------------------------
// base64 (standard alphabet, padded): the IPC carries file bytes inside JSON
// ---------------------------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = B64
            .iter()
            .position(|&b| b == c)
            .ok_or_else(|| format!("invalid base64 character {:?}", c as char))?
            as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}
