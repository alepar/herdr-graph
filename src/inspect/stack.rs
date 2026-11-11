//! `instructions stack` (spec §2.3): the HEAD instruction manifest of one seat, the hook-cap simulation,
//! harness-native files and per-clone instruction status.
//!
//! Owns: the stack view shape and its one builder ([`build`]). Owns no status rule: the per-clone
//! `instructions` object is the shared [`InstructionsView`], filled by hg-blw.12 through LoadedInstructions
//! (`unknown` here).
//!
//! Must not: render or digest parts itself (that is the instruction manifest, `bootstrap::instructions`),
//! discover harness files itself (that is `harness_files`), walk history except through one
//! `HistoryWalker::last_touches` call, read file contents, or resolve HEAD (it takes the pinned `CommitId`).
//!
//! Where new behavior goes: a new per-part fact is added to the manifest `Part` and mapped into
//! [`StackPart`]; a new per-clone fact goes through `InstructionsView`.
use crate::bootstrap::instructions::{
    HOOK_RENDER_CAP, RenderedInstructions, fit_hook, render_instructions, short_rev,
};
use crate::inspect::harness_files::{HarnessFile, HarnessName, UserDirs, discover};
use crate::inspect::inventory::Inventory;
use crate::inspect::loaded;
use crate::inspect::output::{InstructionsStatus, InstructionsView, View};
use crate::inspect::roster::instructions_text;
use crate::inspect::scope::Scope;
use crate::model::effective::resolve_in;
use crate::model::harness::Harness;
use crate::model::launch::resolve_cwd;
use crate::model::{CloneId, CloneLifecycle, CommitId, Lifecycle, SeatId};
use crate::ports::store::RepoPath;
use crate::store::GitStore;
use crate::store::history::{HistoryWalker, Touch};
use crate::store::instruction_source::InstructionError;
use crate::store::tree::CommitView;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

pub const HARNESS_FILES_NOTE: &str = "present, not necessarily loaded";

#[derive(Debug, Serialize)]
pub struct StackView {
    pub seat: SeatHead,
    /// The pinned HEAD the manifest was rendered at (equals the envelope `head`).
    pub rev: String,
    /// The per-seat error row: the seat's stack does not resolve (spec §2.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `[]` on error.
    pub parts: Vec<StackPart>,
    /// `null` on error.
    pub hook: Option<HookView>,
    pub harness_files: Vec<HarnessFile>,
    pub harness_files_note: &'static str,
    pub imports_followed: bool,
    pub clones: Vec<StackClone>,
}

#[derive(Debug, Serialize)]
pub struct SeatHead {
    pub id: SeatId,
    pub name: String,
    pub lifecycle: Lifecycle,
    /// `null` when the seat's effective config does not resolve.
    pub harness: Option<Harness>,
}

#[derive(Debug, Serialize)]
pub struct StackPart {
    pub label: String,
    pub path: String,
    pub bytes: usize,
    pub digest: String,
    /// The newest commit touching `path`; `null` beyond the scan cap. For seat sections and responsibility
    /// the path is a whole record file.
    pub changed: Option<Touch>,
}

#[derive(Debug, Serialize)]
pub struct HookView {
    pub cap: usize,
    pub kept: Vec<String>,
    pub omitted: Vec<String>,
    pub cut: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StackClone {
    pub id: CloneId,
    pub name: String,
    pub lifecycle: CloneLifecycle,
    pub instructions: InstructionsView,
}

/// Build the stack of `seat` at `at`. `clone` keeps one clone (it must belong to the seat). `instance` is the
/// instance root, for the cwd fallback.
pub fn build(
    store: &GitStore,
    at: &CommitId,
    seat: &SeatId,
    clone: Option<&CloneId>,
    user: &UserDirs,
    instance: &Path,
) -> anyhow::Result<StackView> {
    build_with(store, at, seat, clone, user, instance, &|paths| {
        HistoryWalker::new(store, at, HistoryWalker::DEFAULT_MAX_SCAN).last_touches(paths)
    })
}

/// [`build`] with the one history lookup (manifest parts and every clone's stale parts, one batched call)
/// supplied by the caller. A failed lookup leaves `changed` and `path_last_touched` null.
fn build_with(
    store: &GitStore,
    at: &CommitId,
    seat: &SeatId,
    clone: Option<&CloneId>,
    user: &UserDirs,
    instance: &Path,
    lookup: &loaded::HistoryLookup<'_>,
) -> anyhow::Result<StackView> {
    let inv = Inventory::load(store, at, &Scope::Seat(seat.clone()))?;
    let (ts, entry) = inv
        .teamspaces
        .iter()
        .find_map(|t| {
            t.seats
                .iter()
                .find(|s| s.record.id == *seat)
                .map(|s| (t, s))
        })
        .ok_or_else(|| InstructionError::Usage(format!("unknown seat {seat}")))?;
    if let Some(c) = clone
        && !entry.clones.iter().any(|e| e.record.id == *c)
    {
        anyhow::bail!("clone {c} is not a clone of seat {}", entry.record.name);
    }

    let tree = CommitView {
        store,
        at: at.clone(),
    };
    let cfg = resolve_in(&tree, &entry.record).ok();
    let (cwd, _) = resolve_cwd(
        cfg.as_ref()
            .map(|c| c.cwd.as_deref())
            .unwrap_or(entry.record.overrides.cwd.as_deref()),
        ts.record.project_repo.as_deref(),
        instance,
        seat,
    );
    let harness_files = discover(&cwd, user);

    let rendered = render_instructions(&tree, at, seat);
    let history = loaded::HistoryPaths::default();
    if let Ok(r) = &rendered {
        history.add(&manifest_paths(r)?);
    }
    let kept: Vec<_> = entry
        .clones
        .iter()
        .filter(|c| clone.is_none_or(|id| c.record.id == *id))
        .collect();
    let view_of = |c: &&crate::inspect::inventory::CloneEntry,
                   touches: &dyn Fn(&[RepoPath]) -> BTreeMap<RepoPath, Touch>| {
        match &rendered {
            Err(e) => InstructionsView::error(e.to_string()),
            Ok(head) => loaded::clone_view(
                store,
                &c.record.id,
                c.record
                    .occupant
                    .as_ref()
                    .and_then(|_| c.current())
                    .map(|e| e.session.native_session_id.as_str()),
                head,
                touches,
            ),
        }
    };
    let mut views: Vec<InstructionsView> = kept
        .iter()
        .map(|c| view_of(c, &history.recorder()))
        .collect();
    let touches = history.resolve(lookup);
    let (error, parts, hook) = match &rendered {
        Err(e) => (Some(e.to_string()), vec![], None),
        Ok(r) => {
            let (parts, hook) = manifest(r, &touches);
            (None, parts, Some(hook))
        }
    };
    for (c, v) in kept.iter().zip(&mut views) {
        if v.status == InstructionsStatus::Stale {
            *v = view_of(c, &|_| touches.clone());
        }
    }
    let clones = kept
        .iter()
        .zip(views)
        .map(|(c, instructions)| StackClone {
            id: c.record.id.clone(),
            name: c.record.name.clone(),
            lifecycle: c.record.lifecycle,
            instructions,
        })
        .collect();
    Ok(StackView {
        seat: SeatHead {
            id: seat.clone(),
            name: entry.record.name.clone(),
            lifecycle: entry.record.lifecycle,
            harness: cfg.map(|c| c.harness),
        },
        rev: at.0.clone(),
        error,
        parts,
        hook,
        harness_files,
        harness_files_note: HARNESS_FILES_NOTE,
        imports_followed: false,
        clones,
    })
}

/// The manifest's distinct part paths, for the one batched history lookup.
fn manifest_paths(r: &RenderedInstructions) -> anyhow::Result<Vec<RepoPath>> {
    let mut paths: Vec<RepoPath> = Vec::new();
    for p in &r.parts {
        let rp = RepoPath::new(&p.path)?;
        if !paths.contains(&rp) {
            paths.push(rp);
        }
    }
    Ok(paths)
}

fn manifest(
    r: &RenderedInstructions,
    touches: &BTreeMap<RepoPath, Touch>,
) -> (Vec<StackPart>, HookView) {
    let fit = fit_hook(r, HOOK_RENDER_CAP);
    let parts = r
        .parts
        .iter()
        .map(|p| StackPart {
            label: p.label.clone(),
            path: p.path.clone(),
            bytes: p.bytes,
            digest: p.digest.clone(),
            changed: RepoPath::new(&p.path)
                .ok()
                .and_then(|rp| touches.get(&rp).cloned()),
        })
        .collect();
    // `omitted` is a prefix of the render order and `cut` the last part.
    let end = r.parts.len() - usize::from(fit.cut.is_some());
    let kept = r.parts[fit.omitted.len().min(end)..end]
        .iter()
        .map(|p| p.label.clone())
        .collect();
    (
        parts,
        HookView {
            cap: HOOK_RENDER_CAP,
            kept,
            omitted: fit.omitted,
            cut: fit.cut,
        },
    )
}

/// `loaded <rev> into <id> via hook at HH:MMZ`, when the clone has a record.
fn loaded_line(i: &InstructionsView) -> Option<String> {
    let rev = i.loaded_rev.as_deref()?;
    let mut line = format!(
        "loaded {} into {}",
        &rev[..rev.len().min(12)],
        i.loaded_session
            .as_deref()
            .unwrap_or("session not identified")
    );
    if let Some(src) = &i.source {
        let _ = write!(line, " via {src}");
    }
    if let Some(at) = i.loaded_at {
        let _ = write!(line, " at {}", at.format("%H:%MZ"));
    }
    if !i.omitted.is_empty() {
        let _ = write!(line, "  omitted: {}", i.omitted.join(", "));
    }
    Some(line)
}

fn change_word(c: crate::inspect::output::PartChange) -> &'static str {
    use crate::inspect::output::PartChange;
    match c {
        PartChange::Modified => "modified",
        PartChange::Added => "added",
        PartChange::Removed => "removed",
    }
}

fn size_kb(bytes: usize) -> String {
    format!("{:.1} KB", bytes as f64 / 1000.0)
}

fn touched(t: &Option<Touch>) -> String {
    match t {
        Some(t) => format!(
            "{} {}",
            &t.commit.0[..7.min(t.commit.0.len())],
            t.at.format("%Y-%m-%d")
        ),
        None => "-".into(),
    }
}

impl View for StackView {
    fn render_text(&self, out: &mut String) {
        let s = &self.seat;
        let _ = write!(out, "seat {} ({})", s.name, s.id);
        match (&self.error, &self.hook) {
            (Some(e), _) => {
                let _ = writeln!(out, ": error: {e}");
            }
            (None, hook) => {
                let total: usize = self.parts.iter().map(|p| p.bytes).sum();
                let fit = match hook {
                    Some(h) if h.omitted.is_empty() && h.cut.is_none() => "all kept".to_owned(),
                    Some(h) => {
                        let mut t = format!("{} kept, {} omitted", h.kept.len(), h.omitted.len());
                        if let Some(c) = &h.cut {
                            let _ = write!(t, ", cut {c}");
                        }
                        t
                    }
                    None => String::new(),
                };
                let _ = writeln!(
                    out,
                    " at {} — {} parts, {}, hook cap {HOOK_RENDER_CAP}: {fit}",
                    short_rev(&self.rev),
                    self.parts.len(),
                    size_kb(total)
                );
                let _ = writeln!(out, "  # part — path  bytes  digest  path last touched");
                for (i, p) in self.parts.iter().enumerate() {
                    let digest: String = p.digest.chars().take("sha256:".len() + 8).collect();
                    let _ = writeln!(
                        out,
                        "  {} {} — {}  {} B  {digest}…  path last touched {}",
                        i + 1,
                        p.label,
                        p.path,
                        p.bytes,
                        touched(&p.changed)
                    );
                }
            }
        }
        let own = s.harness.and_then(|h| match h {
            Harness::Claude => Some(HarnessName::Claude),
            Harness::Codex => Some(HarnessName::Codex),
            Harness::Shell => None,
        });
        let _ = writeln!(
            out,
            "harness files (not graph-managed; {HARNESS_FILES_NOTE}; @-imports not followed):"
        );
        if self.harness_files.is_empty() {
            let _ = writeln!(out, "  (none)");
        }
        for f in &self.harness_files {
            let names: Vec<String> = f
                .harnesses
                .iter()
                .map(|h| {
                    let w = serde_json::to_value(h)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default();
                    if Some(*h) == own { format!("{w}*") } else { w }
                })
                .collect();
            let mut line = format!(
                "  {}  {} B  modified {}  [{}]",
                f.path.display(),
                f.bytes,
                f.mtime
                    .map_or("?".into(), |t| t.format("%Y-%m-%d %H:%M").to_string()),
                names.join(",")
            );
            if let Some(n) = &f.note
                && let Ok(w) = serde_json::to_value(n)
            {
                let _ = write!(line, "  {}", w.as_str().unwrap_or(""));
            }
            if f.env.is_some() {
                line.push_str("  env: inspector");
            }
            let _ = writeln!(out, "{line}");
        }
        let _ = writeln!(out, "  (* marks the seat's own harness)");
        let _ = writeln!(out, "clones:");
        if self.clones.is_empty() {
            let _ = writeln!(out, "  (none)");
        }
        for c in &self.clones {
            let _ = writeln!(
                out,
                "  {} ({})  {}",
                c.name,
                c.id,
                instructions_text(&c.instructions)
            );
            if let Some(line) = loaded_line(&c.instructions) {
                let _ = writeln!(out, "    {line}");
            }
            for p in &c.instructions.stale_parts {
                let _ = write!(out, "    stale: {} {}", change_word(p.change), p.label);
                if let Some(c) = &p.path_last_touched {
                    let _ = write!(out, " (path last touched {})", &c[..c.len().min(7)]);
                }
                let _ = writeln!(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::Fx;
    use crate::model::Lifecycle;
    use crate::model::SeatId;
    use crate::ports::store::{RepoPath, StoreError};

    fn user() -> UserDirs {
        UserDirs::default()
    }

    fn build_for(fx: &Fx, seat: &SeatId, clone: Option<&CloneId>) -> anyhow::Result<StackView> {
        let at = crate::ports::store::Store::head(&fx.store).unwrap();
        build(
            &fx.store,
            &at,
            seat,
            clone,
            &user(),
            Path::new("/nonexistent-instance"),
        )
    }

    fn file(fx: &mut Fx, p: &str, body: &str) {
        fx.put_file(RepoPath::new(p).unwrap(), body.as_bytes().to_vec());
    }

    fn setup() -> (Fx, SeatId, CloneId) {
        let mut fx = Fx::new();
        let ts = fx.teamspace("proj", Lifecycle::Active);
        let seat = fx.seat(&ts, "foreman", Lifecycle::Active);
        let clone = fx.clone_rec(&seat, "main", CloneLifecycle::Active, vec![], None);
        (fx, seat, clone)
    }

    #[test]
    fn manifest_and_hook() {
        let (mut fx, seat, _) = setup();
        file(&mut fx, "rules/style.md", "global style\n");
        file(
            &mut fx,
            "teamspaces/proj/seats/foreman/AGENTS.md",
            &"s".repeat(12000),
        );
        let c = fx.commit("rules");
        let v = build_for(&fx, &seat, None).unwrap();
        assert!(v.error.is_none());
        assert_eq!(v.rev, c.0);
        let style = v.parts.iter().find(|p| p.path == "rules/style.md").unwrap();
        assert_eq!(style.bytes, 13);
        assert_eq!(
            style.digest,
            crate::bootstrap::instructions::part_digest("global style\n")
        );
        assert_eq!(style.changed.as_ref().unwrap().commit, c);
        let hook = v.hook.as_ref().unwrap();
        assert_eq!(hook.cap, 9000);
        // The 12 KB seat file alone exceeds the cap: it is cut and the general part is omitted.
        assert_eq!(hook.cut.as_deref(), Some("seat AGENTS.md"));
        assert_eq!(hook.omitted, vec!["global rules/style.md".to_owned()]);
        assert!(hook.kept.is_empty());
        let text = {
            let mut s = String::new();
            v.render_text(&mut s);
            s
        };
        assert!(text.contains("cut seat AGENTS.md"), "{text}");
        assert!(text.contains("path last touched"), "{text}");
        assert!(text.contains("present, not necessarily loaded"), "{text}");
        assert!(text.contains("@-imports not followed"), "{text}");
    }

    #[test]
    fn kept_labels_when_all_fit() {
        let (mut fx, seat, _) = setup();
        file(&mut fx, "rules/a.md", "a\n");
        file(&mut fx, "teamspaces/proj/seats/foreman/AGENTS.md", "seat\n");
        fx.commit("rules");
        let v = build_for(&fx, &seat, None).unwrap();
        let hook = v.hook.unwrap();
        assert_eq!(hook.kept, vec!["global rules/a.md", "seat AGENTS.md"]);
        assert!(hook.omitted.is_empty() && hook.cut.is_none());
    }

    #[test]
    fn other_seat_no_daemon() {
        let (mut fx, seat, _) = setup();
        let ts2 = fx.teamspace("other", Lifecycle::Active);
        let other = fx.seat(&ts2, "scout", Lifecycle::Active);
        file(
            &mut fx,
            "teamspaces/other/seats/scout/AGENTS.md",
            "scout rules\n",
        );
        fx.commit("x");
        // No daemon, no HERDR_* env involved: any seat resolves from the committed tree.
        let v = build_for(&fx, &other, None).unwrap();
        assert_eq!(v.seat.name, "scout");
        assert_eq!(v.seat.harness, Some(Harness::Claude));
        assert!(v.parts.iter().any(|p| p.label == "seat AGENTS.md"));
        let mine = build_for(&fx, &seat, None).unwrap();
        assert!(mine.parts.iter().all(|p| p.label != "seat AGENTS.md"));
    }

    #[test]
    fn error_row() {
        let (mut fx, seat, clone) = setup();
        let ts2 = fx.teamspace("other", Lifecycle::Active);
        let good = fx.seat(&ts2, "scout", Lifecycle::Active);
        fx.seat_mut(&seat).template_ref = Some(crate::model::seat::TemplateRef {
            template: crate::model::TemplateId::new(),
            member: crate::model::MemberId::new(),
        });
        fx.commit("dangling");
        let v = build_for(&fx, &seat, None).unwrap();
        let err = v.error.as_deref().unwrap();
        assert!(err.contains("template_ref"), "{err}");
        assert!(v.parts.is_empty() && v.hook.is_none());
        assert_eq!(v.clones.len(), 1);
        assert_eq!(v.clones[0].id, clone);
        assert_eq!(
            v.clones[0].instructions.status,
            crate::inspect::output::InstructionsStatus::Error
        );
        let mut text = String::new();
        v.render_text(&mut text);
        assert!(
            text.starts_with(&format!("seat foreman ({}): error: ", seat)),
            "{text}"
        );
        assert!(build_for(&fx, &good, None).unwrap().error.is_none());
    }

    #[test]
    fn json_shape_and_clone_filter() {
        let (mut fx, seat, clone) = setup();
        let other_clone = fx.clone_rec(&seat, "alt", CloneLifecycle::Active, vec![], None);
        fx.commit("x");
        let v = build_for(&fx, &seat, None).unwrap();
        let j = serde_json::to_value(&v).unwrap();
        for k in [
            "seat",
            "rev",
            "parts",
            "hook",
            "harness_files",
            "harness_files_note",
            "imports_followed",
            "clones",
        ] {
            assert!(j.get(k).is_some(), "{k}");
        }
        assert_eq!(j["harness_files_note"], "present, not necessarily loaded");
        assert_eq!(j["imports_followed"], false);
        assert_eq!(j["hook"]["cap"], 9000);
        assert_eq!(j["clones"].as_array().unwrap().len(), 2);
        for c in j["clones"].as_array().unwrap() {
            assert_eq!(
                c["instructions"],
                serde_json::to_value(InstructionsView::unknown()).unwrap()
            );
        }
        let one = build_for(&fx, &seat, Some(&other_clone)).unwrap();
        assert_eq!(one.clones.len(), 1);
        assert_eq!(one.clones[0].id, other_clone);
        let ts2 = fx.teamspace("o", Lifecycle::Active);
        let s2 = fx.seat(&ts2, "s2", Lifecycle::Active);
        let foreign = fx.clone_rec(&s2, "x", CloneLifecycle::Active, vec![], None);
        fx.commit("y");
        let e = build_for(&fx, &seat, Some(&foreign))
            .unwrap_err()
            .to_string();
        assert!(e.contains("is not a clone of seat foreman"), "{e}");
        let _ = clone;
    }

    #[test]
    fn one_history_pass_and_a_failed_lookup_degrades() {
        use crate::inspect::fixture::{session, ts};
        use crate::ports::store::Store;
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let seat = fx.seat(&t, "foreman", Lifecycle::Active);
        let mut clones = vec![];
        for (name, native) in [("main", "s-1"), ("alt", "s-2")] {
            let sess = session(Harness::Claude, native, "2026-11-01T10:00:00Z", None, None);
            let occ = sess.id.clone();
            clones.push((
                fx.clone_rec(&seat, name, CloneLifecycle::Active, vec![sess], Some(occ)),
                native,
            ));
        }
        file(&mut fx, "rules/a.md", "a1\n");
        let first = fx.commit("rules a1");
        let view = CommitView {
            store: &fx.store as &dyn Store,
            at: first.clone(),
        };
        let r = render_instructions(&view, &first, &seat).unwrap();
        let fit = fit_hook(&r, HOOK_RENDER_CAP);
        for (clone, native) in &clones {
            loaded::record(
                fx.store.root(),
                clone,
                &r,
                &fit,
                loaded::InjectionSource::Hook,
                Some((*native).into()),
                true,
                ts("2026-11-01T11:00:00Z"),
            );
        }
        file(&mut fx, "rules/a.md", "a2\n");
        let touched = fx.commit("rules a2");
        let at = fx.store.head().unwrap();
        let run = |lookup: &loaded::HistoryLookup<'_>| {
            build_with(
                &fx.store,
                &at,
                &seat,
                None,
                &user(),
                Path::new("/nonexistent-instance"),
                lookup,
            )
            .unwrap()
        };
        let calls = std::cell::Cell::new(0);
        let v = run(&|paths| {
            calls.set(calls.get() + 1);
            HistoryWalker::new(&fx.store, &at, 100).last_touches(paths)
        });
        assert_eq!(calls.get(), 1, "manifest and both clones share one pass");
        assert_eq!(v.parts[0].changed.as_ref().unwrap().commit, touched);
        for c in &v.clones {
            assert_eq!(c.instructions.status, InstructionsStatus::Stale);
            assert_eq!(
                c.instructions.stale_parts[0].path_last_touched.as_deref(),
                Some(touched.0.as_str())
            );
        }
        // A failing lookup: still a view, every history column null.
        let v = run(&|_| Err(StoreError::Git("boom".into())));
        assert!(v.error.is_none());
        assert!(v.parts.iter().all(|p| p.changed.is_none()));
        for c in &v.clones {
            assert_eq!(c.instructions.status, InstructionsStatus::Stale);
            assert_eq!(c.instructions.stale_parts[0].path_last_touched, None);
        }
    }
}
