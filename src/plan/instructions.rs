//! `plan instructions set|clear`: the one way an instruction file (global/teamspace rules, seat AGENTS.md and
//! rules) changes (spec §2.2, §2.2a, §2.4, §4.4). Planning reads the committed revision; apply writes or
//! deletes the file and records an `InstructionsEdit` action whose compensation is
//! `{file, before|before_b64, after|after_b64}` (full text, or base64 when the bytes are not UTF-8; key
//! omitted when absent). Staleness is the generic `same_effects` check of `OrgMutation`.
use super::core_kinds::{Acc, mm, parse_args, write_action};
use super::kind::{OrgKind, PlanBody, PlanCx, PlanError};
use super::line_diff;
use super::types::{Plan, PlanEffect, Reserved};
use crate::bootstrap::instructions::{PatchedTree, cap_warning};
use crate::daemon::registry::CallerInfo;
use crate::model::ActionId;
use crate::model::action::{ActionKind, AffectedObject};
use crate::model::b64::{b64_decode, b64_encode};
use crate::model::change::{ReliedOn, RequestKind, Version};
use crate::model::common::BlobHash;
use crate::store::instruction_source::{
    self as source, InstructionError, InstructionFile, InstructionTarget, ScopeKind,
};
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError};
use serde::{Deserialize, Serialize};
use serde_json::json;

const USAGE: &str = "instructions set|clear <scope> [<target>] <file> [--from <path>]";
const ABSENT: &str = "absent";

pub struct InstructionsKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Args {
    /// `set` | `clear`
    action: String,
    /// `global` | `teamspace` | `seat`
    scope: String,
    /// The word the caller gave (name or id); resolved at the plan's revision.
    target: Option<String>,
    /// `rules/<n>.md` | `AGENTS.md`
    file: String,
    /// New text for `set`; null for `clear`.
    content: Option<String>,
    /// Raw bytes for `set`, base64: what an undo restores. Never set by `parse`; it skips `check_content`
    /// because undo puts back exactly what was committed before (rendered as a placeholder if inadmissible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_b64: Option<String>,
}

impl Args {
    fn is_set(&self) -> bool {
        self.action == "set"
    }

    /// The bytes `set` writes: `content_b64` decoded, else `content` checked; None for `clear`.
    fn after_bytes(&self, path: &str) -> Result<Option<Vec<u8>>, PlanError> {
        if !self.is_set() {
            return Ok(None);
        }
        match (&self.content, &self.content_b64) {
            (Some(text), None) => {
                source::check_content(text.as_bytes())
                    .map_err(|e| PlanError::Invalid(format!("{path}: {e}")))?;
                Ok(Some(text.as_bytes().to_vec()))
            }
            (None, Some(b64)) => b64_decode(b64)
                .map(Some)
                .map_err(|e| PlanError::Invalid(format!("content_b64: {e}"))),
            _ => Err(PlanError::Invalid(
                "set carries exactly one of content and content_b64".into(),
            )),
        }
    }
}

/// Compensation entry for `bytes`: `<key>` as text when UTF-8, else `<key>_b64`.
fn put_bytes(comp: &mut toml::Table, key: &str, bytes: &[u8]) {
    match std::str::from_utf8(bytes) {
        Ok(t) => comp.insert(key.into(), toml::Value::String(t.to_owned())),
        Err(_) => comp.insert(format!("{key}_b64"), toml::Value::String(b64_encode(bytes))),
    };
}

fn ie(e: InstructionError) -> PlanError {
    match e {
        InstructionError::Usage(m) => PlanError::Usage(m),
        InstructionError::Store(s) => PlanError::Store(s),
        other => PlanError::Invalid(other.to_string()),
    }
}

fn scope_kind(word: &str) -> Result<ScopeKind, PlanError> {
    match word {
        "global" => Ok(ScopeKind::Global),
        "teamspace" => Ok(ScopeKind::Teamspace),
        "seat" => Ok(ScopeKind::Seat),
        other => Err(PlanError::Invalid(format!(
            "unknown instruction scope {other:?}"
        ))),
    }
}

fn blob_sha(bytes: &[u8]) -> Result<String, PlanError> {
    git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .map(|o| o.to_string())
        .map_err(|e| PlanError::Store(crate::ports::store::StoreError::Git(e.to_string())))
}

/// Resolve the args at `tree`: the target, its file path and the object the effect is recorded against.
struct Resolved {
    target: InstructionTarget,
    path: crate::ports::store::RepoPath,
    object: crate::model::AnyId,
}

fn resolve(tree: &dyn TreeRead, a: &Args) -> Result<Resolved, PlanError> {
    let kind = scope_kind(&a.scope)?;
    let file = InstructionFile::parse(kind, &a.file).map_err(ie)?;
    let scope = source::resolve_scope(tree, kind, a.target.as_deref()).map_err(ie)?;
    let object = scope.object_id(tree).map_err(ie)?;
    let target = InstructionTarget { scope, file };
    let path = source::path(tree, &target).map_err(ie)?;
    Ok(Resolved {
        target,
        path,
        object,
    })
}

fn scope_label(a: &Args) -> String {
    match &a.target {
        Some(t) => format!("{} {t}", a.scope),
        None => a.scope.clone(),
    }
}

impl OrgKind for InstructionsKind {
    fn kind(&self) -> RequestKind {
        RequestKind::InstructionsEdit
    }

    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("instructions", "")]
    }

    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || PlanError::Usage(USAGE.into());
        let action = words.first().ok_or_else(usage)?.clone();
        if action != "set" && action != "clear" {
            return Err(usage());
        }
        let mut rest: Vec<String> = Vec::new();
        let mut from: Option<String> = None;
        let mut it = words[1..].iter();
        while let Some(w) = it.next() {
            if w == "--from" {
                from = Some(it.next().ok_or_else(usage)?.clone());
            } else {
                rest.push(w.clone());
            }
        }
        let r = source::parse_words(&rest, true).map_err(ie)?;
        let file = r
            .file
            .as_ref()
            .map(InstructionFile::rel)
            .ok_or_else(usage)?;
        let content = match (action.as_str(), from) {
            ("set", Some(f)) => {
                let path = match &caller.cwd {
                    Some(cwd) => cwd.join(&f),
                    None => std::path::PathBuf::from(&f),
                };
                let bytes = std::fs::read(&path).map_err(|e| {
                    PlanError::Invalid(format!("cannot read {}: {e}", path.display()))
                })?;
                let text = String::from_utf8(bytes).map_err(|_| {
                    PlanError::Invalid(format!("{} is not valid UTF-8", path.display()))
                })?;
                source::check_content(text.as_bytes())
                    .map_err(|e| PlanError::Invalid(format!("{}: {e}", path.display())))?;
                Some(text)
            }
            ("set", None) => {
                return Err(PlanError::Usage(format!(
                    "set needs --from <path>; {USAGE}"
                )));
            }
            (_, Some(_)) => {
                return Err(PlanError::Usage(format!("clear takes no --from; {USAGE}")));
            }
            _ => None,
        };
        Ok(serde_json::to_value(Args {
            action,
            scope: r.scope.word().to_owned(),
            target: r.target,
            file,
            content,
            content_b64: None,
        })
        .expect("args serialize"))
    }

    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: Args = parse_args(args)?;
        let res = resolve(cx.tree, &a)?;
        let _act: ActionId = reserved.get_or_mint("act");
        let before = cx.tree.read_file(&res.path)?;
        let after = a.after_bytes(res.path.as_str())?;
        if a.is_set() {
            if before == after {
                return Err(PlanError::Invalid(format!(
                    "no change: {} already has this content",
                    res.path.as_str()
                )));
            }
        } else if before.is_none() {
            return Err(PlanError::Invalid(format!(
                "no such instruction file {}",
                res.path.as_str()
            )));
        }
        let before_sha = before.as_deref().map(blob_sha).transpose()?;
        let after_sha = after.as_deref().map(blob_sha).transpose()?;
        let diff =
            line_diff::instruction_diff(res.path.as_str(), before.as_deref(), after.as_deref());
        let effect = PlanEffect::new(
            &format!("instructions.{}", a.action),
            res.object.clone(),
            json!({
                "file": a.file,
                "before_sha": before_sha.clone().unwrap_or_else(|| ABSENT.into()),
                "after_sha": after_sha.clone().unwrap_or_else(|| ABSENT.into()),
                "diff": diff,
            }),
        );
        let relied_on = before_sha
            .map(|h| {
                vec![ReliedOn {
                    object: res.object.clone(),
                    version: Version::Blob(BlobHash(h)),
                }]
            })
            .unwrap_or_default();

        let seats = source::affected_seats(cx.tree, &res.target.scope).map_err(ie)?;
        let names = if seats.is_empty() {
            "(none)".to_owned()
        } else {
            seats
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut warnings = vec![format!(
            "takes effect at each affected seat's next /seat (start, clear, compaction); a live or resumed \
             session may still hold the previous text until its context is reset: {names}"
        )];
        let mut patched = PatchedTree::new(cx.tree);
        match after {
            Some(b) => patched.put(res.path.clone(), b),
            None => patched.delete(res.path.clone()),
        }
        for s in &seats {
            if let Some(w) = cap_warning(&patched, &cx.at, &s.id)? {
                warnings.push(w);
            }
        }
        Ok(PlanBody {
            effects: vec![effect],
            relied_on,
            warnings,
            repair_required: None,
            summary: format!("instructions {} {} {}", a.action, scope_label(&a), a.file),
        })
    }

    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: Args = parse_args(args).map_err(mm)?;
        let res = resolve(&cx.tree, &a).map_err(mm)?;
        let act: ActionId = plan
            .reserved
            .get("act")
            .ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))?;
        let before = cx.tree.read_file(&res.path)?;
        let after = a.after_bytes(res.path.as_str()).map_err(mm)?;
        match &after {
            Some(b) => cx.tree.put_file(res.path.clone(), b.clone()),
            None => cx.tree.delete_file(&res.path),
        }
        let mut comp = toml::Table::new();
        comp.insert("file".into(), toml::Value::String(a.file.clone()));
        if let Some(b) = &before {
            put_bytes(&mut comp, "before", b);
        }
        if let Some(b) = &after {
            put_bytes(&mut comp, "after", b);
        }
        let acc = Acc {
            affected: vec![AffectedObject {
                object: res.object,
                before: None,
                after: None,
            }],
            ..Acc::default()
        };
        write_action(cx, &act, ActionKind::InstructionsEdit, acc, comp)?;
        Ok(Applied {
            summary: format!("instructions {} {} {}", a.action, scope_label(&a), a.file),
            action: Some(act),
        })
    }
}
