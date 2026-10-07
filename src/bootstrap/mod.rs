//! /seat resolution, show/list/path, content write, setup claude hook/skill installation, shipped example
//! templates (spec §9, §8.3). Owned by hg-zmi.13.
//!
//! Daemon side: `register_commands` adds IPC `seat.resolve` and `content.write`; `register_mutations` adds the
//! writer mutation `content_write`. Both are wired by `daemon::compose` (hg-zmi.18).
use crate::daemon::registry::{CallerInfo, CommandCtx, CommandError, Registry};
use crate::journal::Journal;
use crate::model::application::ApplicationRecord;
use crate::model::clone::CloneRecord;
use crate::model::seat::SeatRecord;
use crate::model::template::TemplateRecord;
use crate::model::{CloneId, PlanId, SeatId};
use crate::plan::commands::{PlanDeps, create_plan, wait_result};
use crate::plan::ops::pending_for;
use crate::ports::herdr::{HerdrApi, PaneInfo};
use crate::ports::store::{ObjectLocation, RepoPath, Store, StoreError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::slug::slugify;
use crate::store::tree::{CommitView, TreeRead};
use crate::threads::pending_invitations;
use crate::writer::MutationRegistry;
use crate::writer::worktree::{read_dirty, view_rev};
use resolve::{Proposal, Resolution, proposal_words, resolve_caller};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

pub mod content;
pub mod examples;
pub mod resolve;
pub mod setup_claude;
pub mod show;

#[cfg(test)]
mod tests;

pub struct BootstrapDeps {
    pub plan: Arc<PlanDeps>,
    pub journal: Arc<Journal>,
    /// Herdr snapshot source for the caller pane's agent session id; `None` skips that resolution step.
    pub herdr: Option<Arc<dyn HerdrApi>>,
}

/// Writer mutations: `content_write`.
pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register(content::MUTATION_KEY, Arc::new(content::ContentWrite));
}

fn internal(e: impl std::fmt::Display) -> CommandError {
    CommandError::internal(e.to_string())
}

/// IPC commands: `seat.resolve`, `content.write`.
pub fn register_commands(reg: &mut Registry, deps: BootstrapDeps) {
    let deps = Arc::new(deps);
    let d = deps.clone();
    reg.command("seat.resolve", move |cx: CommandCtx, _args: Value| {
        let d = d.clone();
        async move {
            let pane = snapshot_pane(&d, &cx.caller).await;
            tokio::task::spawn_blocking(move || seat_reply(&d, &cx.caller, pane.as_ref()))
                .await
                .map_err(internal)?
        }
    });
    let d = deps;
    reg.command("content.write", move |cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let op = {
                let d = d.clone();
                tokio::task::spawn_blocking(move || {
                    content::admit_write(&d.plan, &cx.caller, &args)
                })
                .await
                .map_err(internal)??
            };
            wait_result(&d.plan, &op).await
        }
    });
}

async fn snapshot_pane(d: &BootstrapDeps, caller: &CallerInfo) -> Option<PaneInfo> {
    let (herdr, pane) = (d.herdr.as_ref()?, caller.pane_id.as_deref()?);
    let snap = herdr.snapshot().await.ok()?;
    snap.workspaces
        .into_iter()
        .flat_map(|w| w.tabs)
        .flat_map(|t| t.panes)
        .find(|p| p.id.0 == pane)
}

fn abs(root: &Path, p: &RepoPath) -> String {
    root.join(p.as_str()).display().to_string()
}

/// Regular files directly under `dir` whose name ends in `.md`, as absolute working-tree paths.
fn markdown_in(
    tree: &dyn TreeRead,
    root: &Path,
    dir: &RepoPath,
) -> Result<Vec<String>, StoreError> {
    let mut out = Vec::new();
    for e in tree.list_dir(dir)? {
        if e.kind == crate::ports::store::EntryKind::File && e.name.ends_with(".md") {
            out.push(abs(root, &dir.join(&e.name)?));
        }
    }
    out.sort();
    Ok(out)
}

fn files_below(
    tree: &dyn TreeRead,
    root: &Path,
    dir: &RepoPath,
    out: &mut Vec<String>,
) -> Result<(), StoreError> {
    for e in tree.list_dir(dir)? {
        let child = dir.join(&e.name)?;
        if e.kind == crate::ports::store::EntryKind::Dir {
            files_below(tree, root, &child, out)?;
        } else {
            out.push(abs(root, &child));
        }
    }
    Ok(())
}

struct BoundPaths {
    value: Value,
    reload_required: bool,
}

/// The `paths` object for a bound caller (absolute working-tree paths of the committed revision's locations).
fn bound_paths(
    tree: &dyn TreeRead,
    root: &Path,
    seat_id: &SeatId,
    clone_id: &CloneId,
) -> Result<BoundPaths, StoreError> {
    let seat_loc = layout::locate(tree, &seat_id.to_any())?.ok_or_else(|| missing(seat_id))?;
    let seat: SeatRecord =
        read_toml(tree, &seat_loc.record_path)?.ok_or_else(|| missing(seat_id))?;
    let clone_loc = layout::locate(tree, &clone_id.to_any())?.ok_or_else(|| missing(clone_id))?;
    let clone: CloneRecord =
        read_toml(tree, &clone_loc.record_path)?.ok_or_else(|| missing(clone_id))?;
    let ts_loc =
        layout::locate(tree, &seat.teamspace.to_any())?.ok_or_else(|| missing(&seat.teamspace))?;

    let mut templates = Vec::new();
    let mut applications = Vec::new();
    let mut push_template =
        |loc: &ObjectLocation, member: Option<&str>| -> Result<(), StoreError> {
            let rec = abs(root, &loc.record_path);
            if !templates.contains(&rec) {
                templates.push(rec);
            }
            let reusable = loc.folder.join("AGENTS.md")?;
            if tree.read_file(&reusable)?.is_some() {
                let reference = abs(root, &reusable);
                if !templates.contains(&reference) {
                    templates.push(reference);
                }
            }
            if let Some(name) = member {
                let md = layout::member_agents_md(&loc.folder, &slugify(name));
                if tree.read_file(&md)?.is_some() {
                    let reference = abs(root, &md);
                    if !templates.contains(&reference) {
                        templates.push(reference);
                    }
                }
            }
            Ok(())
        };
    if let Some(r) = &seat.template_ref
        && let Some(loc) = layout::locate(tree, &r.template.to_any())?
    {
        let member = read_toml::<TemplateRecord>(tree, &loc.record_path)?
            .and_then(|t| t.members.into_iter().find(|m| m.id == r.member));
        if let Some(shared) =
            crate::model::effective::referenced_seat_template(tree, member.as_ref())?
        {
            let shared_loc =
                layout::locate(tree, &shared.id.to_any())?.ok_or_else(|| missing(&shared.id))?;
            push_template(&shared_loc, None)?;
        }
        push_template(&loc, member.as_ref().map(|m| m.name.as_str()))?;
    }
    for app in &seat.applications {
        let Some(app_loc) = layout::locate(tree, &app.to_any())? else {
            continue;
        };
        applications.push(abs(root, &app_loc.record_path));
        let Some(app) = read_toml::<ApplicationRecord>(tree, &app_loc.record_path)? else {
            continue;
        };
        if let Some(loc) = layout::locate(tree, &app.template.to_any())? {
            let template: TemplateRecord =
                read_toml(tree, &loc.record_path)?.ok_or_else(|| missing(&app.template))?;
            push_template(&loc, None)?;
            for member in template
                .members
                .iter()
                .filter(|m| app.member_map.get(&m.id) == Some(seat_id))
            {
                if let Some(shared) =
                    crate::model::effective::referenced_seat_template(tree, Some(member))?
                {
                    let shared_loc = layout::locate(tree, &shared.id.to_any())?
                        .ok_or_else(|| missing(&shared.id))?;
                    push_template(&shared_loc, None)?;
                }
                push_template(&loc, Some(&member.name))?;
            }
        }
    }

    let agents = seat_loc.folder.join("AGENTS.md")?;
    let mut clone_files = Vec::new();
    files_below(tree, root, &clone_loc.folder, &mut clone_files)?;
    clone_files.sort();
    let value = json!({
        "templates": templates,
        "applications": applications,
        "rules": {
            "global": markdown_in(tree, root, &RepoPath::new("rules")?)?,
            "team": markdown_in(tree, root, &ts_loc.folder.join("rules")?)?,
            "seat": markdown_in(tree, root, &seat_loc.folder.join("rules")?)?,
        },
        "seat_agents_md": tree.read_file(&agents)?.is_some().then(|| abs(root, &agents)),
        "seat_record": abs(root, &seat_loc.record_path),
        "seat_folder": abs(root, &seat_loc.folder),
        "clone_folder": abs(root, &clone_loc.folder),
        "clone_files": clone_files,
    });
    Ok(BoundPaths {
        value,
        reload_required: clone.reload_required || seat.reload_required,
    })
}

fn missing(id: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt {
        path: id.to_string(),
        reason: "object not found at the committed revision".into(),
    }
}

/// `seat.resolve`: resolution, proposals (stored, never applied), paths, pending ops and invitations.
pub fn seat_reply(
    d: &BootstrapDeps,
    caller: &CallerInfo,
    pane: Option<&PaneInfo>,
) -> Result<Value, CommandError> {
    let store: &dyn Store = &*d.plan.store;
    let head = store.head().map_err(internal)?;
    let view = CommitView {
        store,
        at: head.clone(),
    };
    let root = d.plan.instance.as_path();

    let mut resolution = resolve_caller(&view, caller, pane);
    let mut proposal_error = None;
    if let Some(words) = proposal_words(&view, caller, pane, &resolution) {
        match create_plan(&d.plan, caller, words.clone()) {
            Ok(v) => {
                let plan_id = v["plan_id"].as_str().and_then(|s| s.parse::<PlanId>().ok());
                match plan_id {
                    Some(plan_id) => resolution.set_proposal(Some(Proposal {
                        plan_id,
                        hash: v["hash"].as_str().unwrap_or_default().to_owned(),
                        words,
                        rendered: v["rendered"].as_str().unwrap_or_default().to_owned(),
                    })),
                    None => proposal_error = Some("plan.create returned no plan id".to_owned()),
                }
            }
            Err(e) => proposal_error = Some(e.message),
        }
    }

    let global_rules =
        markdown_in(&view, root, &RepoPath::new("rules").map_err(internal)?).map_err(internal)?;
    let (paths, pending_ops, invitations, reload_required, beads_query, beads_labels) =
        match &resolution {
            Resolution::Bound {
                teamspace,
                seat,
                clone,
                native_session,
                ..
            } => {
                let bp = bound_paths(&view, root, seat, clone).map_err(internal)?;
                let ops = pending_for(&d.journal, Some(seat), None).map_err(internal)?;
                let mut labels = json!({
                    "ts": format!("hg-ts:{teamspace}"),
                    "seat": format!("hg-seat:{seat}"),
                    "clone": format!("hg-clone:{clone}"),
                });
                if let Some(ns) = native_session {
                    labels["ns"] = json!(format!("hg-ns:{ns}"));
                }
                (
                    bp.value,
                    serde_json::to_value(ops).map_err(internal)?,
                    serde_json::to_value(pending_invitations(&view, clone)).map_err(internal)?,
                    bp.reload_required,
                    json!(format!("bd list --label hg-seat:{seat}")),
                    labels,
                )
            }
            _ => (
                json!({ "templates": [], "rules": { "global": global_rules, "team": [], "seat": [] },
                    "seat_agents_md": null, "seat_folder": null, "clone_folder": null, "clone_files": [] }),
                json!([]),
                json!([]),
                false,
                Value::Null,
                Value::Null,
            ),
        };
    let dirty: Vec<String> = read_dirty(root).into_iter().map(|e| e.path).collect();
    let mut reply = json!({
        "resolution": resolution,
        "head": head,
        "view_rev": view_rev(root),
        "worktree_dirty": dirty,
        "instance": root.display().to_string(),
        "paths": paths,
        "pending_ops": pending_ops,
        "pending_invitations": invitations,
        "reload_required": reload_required,
        "beads_query": beads_query,
        "beads_labels": beads_labels,
    });
    if let Some(e) = proposal_error {
        reply["proposal_error"] = json!(e);
    }
    Ok(reply)
}

/// `seat --hook-prompt`: whether the SessionStart hook should print `run /seat`. True when `HERDR_GRAPH=1`
/// (the pane was launched by graph) or when the pane id resolves to a bound live clone (spec §9, r2).
pub fn wants_seat_prompt(
    graph_env: Option<&str>,
    tree: Option<&dyn TreeRead>,
    pane: Option<&str>,
) -> bool {
    if graph_env == Some("1") {
        return true;
    }
    let (Some(tree), Some(pane)) = (tree, pane) else {
        return false;
    };
    let caller = CallerInfo {
        pane_id: Some(pane.to_owned()),
        ..CallerInfo::default()
    };
    matches!(resolve_caller(tree, &caller, None), Resolution::Bound { via, .. } if via == "binding")
}

/// Render a `seat.resolve` reply as readable sections.
pub fn render_seat(reply: &Value) -> String {
    let mut out = String::new();
    let r = &reply["resolution"];
    let text = |v: &Value| v.as_str().unwrap_or("-").to_owned();
    match r["status"].as_str() {
        Some("bound") => out.push_str(&format!(
            "bound: teamspace {} seat {} clone {} (via {})\n",
            text(&r["teamspace"]),
            text(&r["seat"]),
            text(&r["clone"]),
            text(&r["via"])
        )),
        Some(other) => out.push_str(&format!(
            "{other}: this pane is not identified as a graph clone\n"
        )),
        None => out.push_str("unknown resolution\n"),
    }
    if let Some(cands) = r["candidates"].as_array().filter(|c| !c.is_empty()) {
        out.push_str("candidates:\n");
        for c in cands {
            out.push_str(&format!(
                "  {} (seat {}): {}\n",
                text(&c["clone"]),
                text(&c["seat"]),
                text(&c["reason"])
            ));
        }
    }
    if r["proposal"].is_object() {
        out.push_str("proposal (not applied; show it to the user first):\n");
        for line in text(&r["proposal"]["rendered"]).lines() {
            out.push_str(&format!("  {line}\n"));
        }
        out.push_str(&format!(
            "  to apply after an explicit yes: herdr-graph apply {} --confirm {} --confirmed-by user-relay\n",
            text(&r["proposal"]["plan_id"]),
            text(&r["proposal"]["hash"])
        ));
    }
    if let Some(e) = reply["proposal_error"].as_str() {
        out.push_str(&format!("no proposal could be planned: {e}\n"));
    }
    out.push_str(&format!(
        "instance: {}\nhead: {}\nview_rev: {}\n",
        text(&reply["instance"]),
        text(&reply["head"]),
        text(&reply["view_rev"])
    ));
    let list = |out: &mut String, title: &str, v: &Value| {
        let items: Vec<String> = v
            .as_array()
            .map(|a| a.iter().map(&text).collect())
            .unwrap_or_default();
        if !items.is_empty() {
            out.push_str(&format!("{title}:\n"));
            for i in items {
                out.push_str(&format!("  {i}\n"));
            }
        }
    };
    list(
        &mut out,
        "uncommitted local edits (worktree_dirty)",
        &reply["worktree_dirty"],
    );
    let p = &reply["paths"];
    list(&mut out, "templates", &p["templates"]);
    list(&mut out, "application member mappings", &p["applications"]);
    list(&mut out, "global rules", &p["rules"]["global"]);
    list(&mut out, "team rules", &p["rules"]["team"]);
    list(&mut out, "seat rules", &p["rules"]["seat"]);
    if let Some(a) = p["seat_agents_md"].as_str() {
        out.push_str(&format!("seat AGENTS.md: {a}\n"));
    }
    if let Some(a) = p["seat_record"].as_str() {
        out.push_str(&format!(
            "seat record (instance overrides and context): {a}\n"
        ));
    }
    if let Some(a) = p["seat_folder"].as_str() {
        out.push_str(&format!("seat folder: {a}\n"));
    }
    if let Some(a) = p["clone_folder"].as_str() {
        out.push_str(&format!("clone folder: {a}\n"));
    }
    list(&mut out, "clone files", &p["clone_files"]);
    if let Some(ops) = reply["pending_ops"].as_array().filter(|o| !o.is_empty()) {
        out.push_str("pending ops:\n");
        for o in ops {
            out.push_str(&format!(
                "  {} {} {}\n",
                text(&o["op"]),
                text(&o["state"]),
                text(&o["summary"])
            ));
        }
    }
    if let Some(inv) = reply["pending_invitations"]
        .as_array()
        .filter(|i| !i.is_empty())
    {
        out.push_str("pending invitations (run the commands):\n");
        for i in inv {
            out.push_str(&format!(
                "  {} [{}]: {}\n",
                text(&i["thread"]),
                text(&i["constraint"]),
                text(&i["accept_command"])
            ));
        }
    }
    if reply["reload_required"] == true {
        out.push_str("reload_required: start a fresh session for graph changes to take effect\n");
    }
    if let Some(q) = reply["beads_query"].as_str() {
        out.push_str(&format!("beads: {q}\n"));
    }
    out
}
