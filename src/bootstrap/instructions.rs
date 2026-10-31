//! The verbatim, ordered instruction stack of one seat at one commit (spec §4.1); reads only the committed
//! tree; never truncates (the hook does, through `fit_hook`).
use super::missing;
use crate::model::application::ApplicationRecord;
use crate::model::seat::SeatRecord;
use crate::model::template::{TemplateMember, TemplateRecord};
use crate::model::{CommitId, SeatId};
use crate::ports::store::{DirEntry, EntryKind, ObjectLocation, RepoPath, StoreError};
use crate::store::instruction_source::{
    self as source, InstructionError, InstructionScope, ListedFile, check_content,
};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::slug::slugify;
use crate::store::tree::TreeRead;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Conservative hook-output cap in bytes (spec §4.4). No documented Claude Code hook output limit was found
/// (`claude --help`, installed binary), so the 9000 default stands.
pub const HOOK_RENDER_CAP: usize = 9000;

const AGENTS_MD: &str = "AGENTS.md";

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub label: String,
    pub path: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedInstructions {
    pub rev: String,
    pub seat: SeatId,
    pub seat_name: String,
    pub parts: Vec<Part>,
    pub text: String,
}

/// One template taking part in a seat's instruction stack: a shared reusable seat template (`shared`), an
/// applied template root (`member: None`) or a team template with the member bound to the seat.
#[derive(Debug, Clone)]
pub struct Participation {
    pub template: ObjectLocation,
    pub record: TemplateRecord,
    pub member: Option<TemplateMember>,
    pub shared: bool,
}

struct Walk {
    out: Vec<Participation>,
    seen: BTreeSet<(String, Option<String>)>,
}

impl Walk {
    fn push(
        &mut self,
        template: &ObjectLocation,
        record: &TemplateRecord,
        member: Option<&TemplateMember>,
        shared: bool,
    ) {
        let key = (record.id.to_string(), member.map(|m| m.id.to_string()));
        if self.seen.insert(key) {
            self.out.push(Participation {
                template: template.clone(),
                record: record.clone(),
                member: member.cloned(),
                shared,
            });
        }
    }

    /// The member's shared seat template (if any), then the member itself.
    fn push_member(
        &mut self,
        tree: &dyn TreeRead,
        template: &ObjectLocation,
        record: &TemplateRecord,
        member: Option<&TemplateMember>,
    ) -> Result<(), StoreError> {
        if let Some(shared) = crate::model::effective::referenced_seat_template(tree, member)? {
            let shared_loc =
                layout::locate(tree, &shared.id.to_any())?.ok_or_else(|| missing(&shared.id))?;
            self.push(&shared_loc, &shared, None, true);
        }
        self.push(template, record, member, false);
        Ok(())
    }
}

/// The ordered, de-duplicated templates (and members) that take part in `seat`'s instructions: its own
/// template reference first, then each application's template and the members mapped to the seat.
pub fn template_participation(
    tree: &dyn TreeRead,
    seat: &SeatRecord,
) -> Result<Vec<Participation>, StoreError> {
    let mut walk = Walk {
        out: Vec::new(),
        seen: BTreeSet::new(),
    };
    if let Some(r) = &seat.template_ref
        && let Some(loc) = layout::locate(tree, &r.template.to_any())?
        && let Some(record) = read_toml::<TemplateRecord>(tree, &loc.record_path)?
    {
        let member = record.members.iter().find(|m| m.id == r.member).cloned();
        walk.push_member(tree, &loc, &record, member.as_ref())?;
    }
    for app in &seat.applications {
        let Some(app_loc) = layout::locate(tree, &app.to_any())? else {
            continue;
        };
        let Some(app) = read_toml::<ApplicationRecord>(tree, &app_loc.record_path)? else {
            continue;
        };
        if let Some(loc) = layout::locate(tree, &app.template.to_any())? {
            let record: TemplateRecord =
                read_toml(tree, &loc.record_path)?.ok_or_else(|| missing(&app.template))?;
            walk.push(&loc, &record, None, false);
            for member in record
                .members
                .iter()
                .filter(|m| app.member_map.get(&m.id) == Some(&seat.id))
            {
                walk.push_member(tree, &loc, &record, Some(member))?;
            }
        }
    }
    Ok(walk.out)
}

/// Repo path of a member's specialization file.
pub fn member_agents_path(p: &Participation, member: &TemplateMember) -> RepoPath {
    layout::member_agents_md(&p.template.folder, &slugify(&member.name))
}

fn instruction_err(e: InstructionError) -> StoreError {
    match e {
        InstructionError::Store(s) => s,
        other => StoreError::Corrupt {
            path: "instructions".into(),
            reason: other.to_string(),
        },
    }
}

fn placeholder(path: &str) -> String {
    format!("({path}: skipped, not an admissible instruction file — see herdr-graph doctor)\n")
}

fn newline_terminated(mut body: String) -> String {
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body
}

/// Admissible UTF-8 text or the placeholder; `None` when the file is absent.
fn read_text(tree: &dyn TreeRead, path: &RepoPath) -> Result<Option<String>, StoreError> {
    Ok(tree
        .read_file(path)?
        .map(|bytes| match check_content(&bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => placeholder(path.as_str()),
        }))
}

struct Collector {
    parts: Vec<Part>,
}

impl Collector {
    fn add(&mut self, label: String, path: &str, body: String) {
        if body.trim().is_empty() {
            return;
        }
        self.parts.push(Part {
            label,
            path: path.to_owned(),
            body: newline_terminated(body),
        });
    }

    fn listed(
        &mut self,
        tree: &dyn TreeRead,
        scope_word: &str,
        f: &ListedFile,
    ) -> Result<(), StoreError> {
        let path = f.path.as_str();
        let body = match &f.name {
            Err(_) => Some(placeholder(path)),
            Ok(_) => read_text(tree, &f.path)?,
        };
        if let Some(body) = body {
            self.add(format!("{scope_word} {}", f.rel), path, body);
        }
        Ok(())
    }
}

fn block(p: &Part) -> String {
    format!("## {} — {}\n{}", p.label, p.path, p.body)
}

pub fn short_rev(at: &str) -> &str {
    at.get(..12).unwrap_or(at)
}

pub fn revision_line(name: &str, seat: &SeatId, short_rev: &str) -> String {
    format!(
        "Instructions for seat {name} ({seat}) at graph revision {short_rev}. This replaces any earlier herdr-graph instruction render in this conversation. Sections are ordered general to specific; on conflict the later section wins."
    )
}

/// The seat's full instruction stack at `at`, read from the committed `tree` only.
pub fn render_instructions(
    tree: &dyn TreeRead,
    at: &CommitId,
    seat: &SeatId,
) -> Result<RenderedInstructions, StoreError> {
    let seat_loc = layout::locate(tree, &seat.to_any())?.ok_or_else(|| missing(seat))?;
    let rec: SeatRecord = read_toml(tree, &seat_loc.record_path)?.ok_or_else(|| missing(seat))?;
    let mut c = Collector { parts: Vec::new() };

    for f in source::list_files(tree, &InstructionScope::Global).map_err(instruction_err)? {
        c.listed(tree, "global", &f)?;
    }
    let team = InstructionScope::Teamspace(rec.teamspace.clone());
    for f in source::list_files(tree, &team).map_err(instruction_err)? {
        c.listed(tree, "teamspace", &f)?;
    }

    let mut roots = BTreeSet::new();
    let mut members = BTreeSet::new();
    for p in template_participation(tree, &rec)? {
        let tpl = &p.record.name;
        if roots.insert(p.record.id.to_string()) {
            let path = p.template.folder.join(AGENTS_MD)?;
            if let Some(body) = read_text(tree, &path)? {
                let kind = if p.shared {
                    "seat template"
                } else {
                    "template"
                };
                c.add(format!("{kind} {tpl}"), path.as_str(), body);
            }
        }
        let Some(member) = &p.member else { continue };
        if !members.insert(member.id.to_string()) {
            continue;
        }
        let md = member_agents_path(&p, member);
        let mut body = read_text(tree, &md)?;
        let path = if body.is_some() {
            md
        } else {
            p.template.record_path.clone()
        };
        if let Some(resp) = &member.responsibility {
            let line = match check_content(resp.as_bytes()) {
                Ok(_) => format!("responsibility: {resp}\n"),
                Err(_) => placeholder(path.as_str()),
            };
            let mut text = newline_terminated(body.take().unwrap_or_default());
            if text.trim().is_empty() {
                text.clear();
            }
            body = Some(text + &line);
        }
        if let Some(body) = body {
            c.add(
                format!("template {tpl} member {}", member.name),
                path.as_str(),
                body,
            );
        }
    }

    if !rec.overrides.instructions_sections.is_empty() {
        let composed: String = rec
            .overrides
            .instructions_sections
            .iter()
            .map(|s| newline_terminated(format!("### {}\n{}", s.name, s.body)))
            .collect();
        let body = match check_content(composed.as_bytes()) {
            Ok(_) => composed,
            Err(_) => placeholder(seat_loc.record_path.as_str()),
        };
        c.add("seat sections".into(), seat_loc.record_path.as_str(), body);
    }

    let own =
        source::list_files(tree, &InstructionScope::Seat(seat.clone())).map_err(instruction_err)?;
    for f in own.iter().filter(|f| f.rel != AGENTS_MD) {
        c.listed(tree, "seat", f)?;
    }
    for f in own.iter().filter(|f| f.rel == AGENTS_MD) {
        c.listed(tree, "seat", f)?;
    }

    let rev = at.0.clone();
    let mut text = revision_line(&rec.name, seat, short_rev(&rev));
    text.push('\n');
    for p in &c.parts {
        text.push('\n');
        text.push_str(&block(p));
    }
    Ok(RenderedInstructions {
        rev,
        seat: seat.clone(),
        seat_name: rec.name,
        parts: c.parts,
        text,
    })
}

/// The hook's first line (spec §4.4).
pub fn hook_header(r: &RenderedInstructions) -> String {
    format!(
        "herdr-graph: this pane is graph seat {}. Your instructions follow (graph revision {}); follow them. Also run /seat now for pending invitations, proposals, operations and Beads work.",
        r.seat_name,
        short_rev(&r.rev)
    )
}

/// Untruncated hook output size in bytes: header, newline, render.
pub fn hook_size(r: &RenderedInstructions) -> usize {
    hook_header(r).len() + 1 + r.text.len()
}

#[derive(Debug, Clone)]
pub struct HookFit {
    pub text: String,
    pub full_size: usize,
    /// Labels of the parts left out (general ones first), in render order.
    pub omitted: Vec<String>,
    /// Label of the single most specific part when it alone had to be cut.
    pub cut: Option<String>,
}

fn marker(labels: &[String]) -> String {
    format!(
        "[truncated: omitted {}; run herdr-graph seat --instructions for the full text]",
        labels.join(", ")
    )
}

/// Hook output within `cap` bytes (spec §4.4): the most specific parts that fit, in render order.
pub fn fit_hook(r: &RenderedInstructions, cap: usize) -> HookFit {
    let full_size = hook_size(r);
    let head = format!(
        "{}\n{}\n",
        hook_header(r),
        revision_line(&r.seat_name, &r.seat, short_rev(&r.rev))
    );
    let blocks: Vec<String> = r.parts.iter().map(|p| format!("\n{}", block(p))).collect();
    let labels: Vec<String> = r.parts.iter().map(|p| p.label.clone()).collect();
    if full_size <= cap || r.parts.is_empty() {
        return HookFit {
            text: format!("{}\n{}", hook_header(r), r.text),
            full_size,
            omitted: vec![],
            cut: None,
        };
    }
    let tail_len = |omitted: &[String]| 2 + marker(omitted).len();
    let mut start = blocks.len();
    let mut used = head.len();
    while start > 0 {
        let i = start - 1;
        let need = used + blocks[i].len() + if i > 0 { tail_len(&labels[..i]) } else { 0 };
        if need > cap {
            break;
        }
        used += blocks[i].len();
        start = i;
    }
    if start < blocks.len() {
        let omitted = labels[..start].to_vec();
        let mut text = head + &blocks[start..].concat();
        text.push('\n');
        text.push_str(&marker(&omitted));
        text.push('\n');
        return HookFit {
            text,
            full_size,
            omitted,
            cut: None,
        };
    }
    let last = blocks.len() - 1;
    let mut named = labels[..last].to_vec();
    named.push(format!("{} (cut)", labels[last]));
    let budget = cap.saturating_sub(head.len() + tail_len(&named));
    let mut end = budget.min(blocks[last].len());
    while !blocks[last].is_char_boundary(end) {
        end -= 1;
    }
    let kept = &blocks[last][..end];
    let kept = kept.rfind('\n').map_or("", |i| &kept[..=i]);
    let mut text = head + kept;
    text.push('\n');
    text.push_str(&marker(&named));
    text.push('\n');
    HookFit {
        text,
        full_size,
        omitted: labels[..last].to_vec(),
        cut: Some(labels[last].clone()),
    }
}

/// The §4.4 plan warning for one seat when its hook output on `tree` (already patched) exceeds the cap.
pub fn cap_warning(
    tree: &dyn TreeRead,
    at: &CommitId,
    seat: &SeatId,
) -> Result<Option<String>, StoreError> {
    let r = render_instructions(tree, at, seat)?;
    let fit = fit_hook(&r, HOOK_RENDER_CAP);
    if fit.full_size <= HOOK_RENDER_CAP {
        return Ok(None);
    }
    let head = format!(
        "hook output for {} would exceed the cap ({} > {HOOK_RENDER_CAP} bytes)",
        r.seat_name, fit.full_size
    );
    let omitted = (!fit.omitted.is_empty()).then(|| {
        format!(
            "at session start these parts will be omitted: {}",
            fit.omitted.join(", ")
        )
    });
    let cut = fit.cut.map(|l| format!("at session start {l} will be cut"));
    let tail = [omitted, cut]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ");
    Ok(Some(format!("{head}; {tail}")))
}

/// A committed tree with pending file edits laid over it (reads only; used to size hooks before a plan applies).
pub struct PatchedTree<'a> {
    base: &'a dyn TreeRead,
    patches: BTreeMap<RepoPath, Option<Vec<u8>>>,
}

impl<'a> PatchedTree<'a> {
    pub fn new(base: &'a dyn TreeRead) -> Self {
        Self {
            base,
            patches: BTreeMap::new(),
        }
    }
    pub fn put(&mut self, p: RepoPath, b: Vec<u8>) {
        self.patches.insert(p, Some(b));
    }
    pub fn delete(&mut self, p: RepoPath) {
        self.patches.insert(p, None);
    }
}

impl TreeRead for PatchedTree<'_> {
    fn read_file(&self, p: &RepoPath) -> Result<Option<Vec<u8>>, StoreError> {
        match self.patches.get(p) {
            Some(patch) => Ok(patch.clone()),
            None => self.base.read_file(p),
        }
    }

    fn list_dir(&self, p: &RepoPath) -> Result<Vec<DirEntry>, StoreError> {
        let mut entries: BTreeMap<String, EntryKind> = self
            .base
            .list_dir(p)?
            .into_iter()
            .map(|e| (e.name, e.kind))
            .collect();
        let prefix = if p.as_str().is_empty() {
            String::new()
        } else {
            format!("{}/", p.as_str())
        };
        for (path, patch) in &self.patches {
            let Some(rest) = path.as_str().strip_prefix(&prefix) else {
                continue;
            };
            match (rest.split_once('/'), patch) {
                (None, Some(_)) => {
                    entries.insert(rest.to_owned(), EntryKind::File);
                }
                (None, None) => {
                    entries.remove(rest);
                }
                (Some((dir, _)), Some(_)) => {
                    entries.entry(dir.to_owned()).or_insert(EntryKind::Dir);
                }
                (Some(_), None) => {}
            }
        }
        Ok(entries
            .into_iter()
            .map(|(name, kind)| DirEntry { name, kind })
            .collect())
    }
}
