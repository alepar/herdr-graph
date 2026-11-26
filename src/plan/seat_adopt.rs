//! `seat adopt <tab>`: bind a live tab of an adopted workspace to a new seat, and each of its agent panes to a
//! keyless clone, without launching anything (design 2026-11-07 G1, G2, G3, G5). The seat's threads seat is
//! the one threads already maps to the pane: `plan.create` resolves it (ResolveSeat) before planning, `plan.apply`
//! resolves it again at admission, and the mutation refuses an apply whose pane now maps to another seat.
//!
//! Planning has no Herdr access and the writer recomputes the plan from its args alone, so `bind_live_facts`
//! folds the resolved tab, its panes, the observed run and the seats into the args.
use super::core_kinds::{
    Acc, TsInfo, ensure_ts_live, lc_value, mm, new_clone_record, parse_args, read_ts, rev_of,
    seat_slug_for, write_action,
};
use super::grammar::{self, Scope, flag, positional};
use super::kind::{LiveFacts, OrgKind, PlanBody, PlanCx, PlanError};
use super::managers::first_seat;
use super::types::{Plan, PlanEffect, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, AffectedObject};
use crate::model::change::RequestKind;
use crate::model::common::{
    Availability, Binding, CloneLifecycle, Incarnation, Lifecycle, Runtime,
};
use crate::model::harness::{Harness, for_agent_kind, profile};
use crate::model::launch::parse_graph_token;
use crate::model::seat::SeatRecord;
use crate::model::{
    ActionId, CloneId, HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId, IdKind,
    SCHEMA_VERSION, SeatId, TeamspaceId,
};
use crate::ports::herdr::{HerdrSnapshot, PaneInfo, TabInfo, WorkspaceInfo};
use crate::reconcile::planner::TOKEN_KEY;
use crate::store::layout;
use crate::store::slug::unique_slug;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError, Reject};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// One pane of the tab, as `bind_live_facts` resolved it.
#[derive(Deserialize)]
struct PaneArg {
    pane_id: HerdrPaneId,
    #[serde(default)]
    terminal_id: Option<HerdrTerminalId>,
    /// The raw graph token in the pane's metadata, if any.
    #[serde(default)]
    token: Option<String>,
    /// False: the pane stays unmanaged inside the seat's tab (a mixed tab).
    adopt: bool,
    /// The threads seat threads maps to the pane; None for a seatless (shell) pane.
    #[serde(default)]
    seat: Option<String>,
}

/// The args `bind_live_facts` produces: everything the plan and the mutation need.
#[derive(Deserialize)]
struct SeatAdoptArgs {
    tab_id: HerdrTabId,
    tab_label: String,
    workspace_id: HerdrWorkspaceId,
    workspace_label: String,
    #[serde(default)]
    workspace_token: Option<String>,
    incarnation: Incarnation,
    name: String,
    #[serde(default)]
    teamspace: Option<String>,
    harness: Harness,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    cwd: Option<String>,
    panes: Vec<PaneArg>,
    #[serde(default)]
    notes: Vec<String>,
}

impl SeatAdoptArgs {
    fn adopted(&self) -> impl Iterator<Item = &PaneArg> {
        self.panes.iter().filter(|p| p.adopt)
    }
}

/// Only what the plan-time checks derive from the tree.
struct Checked {
    ts: TsInfo,
}

pub(crate) struct SeatAdopt;

impl SeatAdopt {
    /// The refusals, against one tree. `plan` runs them, and so does `mutate` against the overlay.
    fn check(tree: &dyn TreeRead, a: &SeatAdoptArgs) -> Result<Checked, PlanError> {
        let label = &a.tab_label;
        let stamped = a
            .workspace_token
            .as_deref()
            .and_then(parse_graph_token)
            .and_then(|id| TeamspaceId::try_from(id).ok());
        let mut found = None;
        let teamspaces = layout::list_teamspaces(tree)?;
        for (_, ts) in &teamspaces {
            if ts.lifecycle == Lifecycle::Retired {
                continue;
            }
            let bound = ts
                .runtime
                .bound
                .as_ref()
                .and_then(|b| b.workspace_id.as_ref())
                == Some(&a.workspace_id);
            if bound {
                found = Some(ts.id.clone());
                break;
            }
            if found.is_none() && stamped.as_ref() == Some(&ts.id) {
                found = Some(ts.id.clone());
            }
        }
        let ts_id = found.ok_or_else(|| {
            PlanError::Invalid(format!(
                "workspace {} is not a teamspace; plan teamspace adopt {} first",
                a.workspace_label, a.workspace_id
            ))
        })?;
        let ts = read_ts(tree, &ts_id)?;
        if let Some(want) = &a.teamspace {
            let want_id = grammar::resolve_teamspace(tree, want, Scope::Live)?;
            if want_id != ts_id {
                return Err(PlanError::Invalid(format!(
                    "tab {label} is in teamspace {}'s workspace, not {want}",
                    ts.rec.name
                )));
            }
        }
        ensure_ts_live(&ts)?;
        let seats = layout::all_seats(tree)?;
        if let Some((_, s)) = seats.iter().find(|(_, s)| {
            s.lifecycle != Lifecycle::Retired
                && s.runtime.bound.as_ref().and_then(|b| b.tab_id.as_ref()) == Some(&a.tab_id)
        }) {
            return Err(PlanError::Invalid(format!(
                "tab {label} is already seat {}",
                s.name
            )));
        }
        let clones = layout::all_clones(tree)?;
        for p in &a.panes {
            let named = p
                .token
                .as_deref()
                .and_then(parse_graph_token)
                .filter(|id| id.kind() == IdKind::Clone);
            if let Some(any) = named
                && let Some((_, c)) = clones
                    .iter()
                    .find(|(_, c)| c.id.to_any() == any && c.lifecycle != CloneLifecycle::Retired)
            {
                return Err(PlanError::Invalid(format!(
                    "pane {} is managed by graph (clone {})",
                    p.pane_id, c.id
                )));
            }
            if p.adopt
                && let Some(seat) = &p.seat
                && let Some((_, c)) = clones.iter().find(|(_, c)| {
                    c.lifecycle != CloneLifecycle::Retired
                        && !c.is_tombstone()
                        && c.threads_seat.as_deref() == Some(seat.as_str())
                })
            {
                return Err(PlanError::Invalid(format!(
                    "threads seat {seat} of pane {} is held by clone {}",
                    p.pane_id, c.id
                )));
            }
        }
        Ok(Checked { ts })
    }
}

/// A unique tab by exact id, else by label across the snapshot.
fn find_tab<'a>(
    snap: &'a HerdrSnapshot,
    target: &str,
) -> Result<(&'a WorkspaceInfo, &'a TabInfo), PlanError> {
    let tabs = || {
        snap.workspaces
            .iter()
            .flat_map(|w| w.tabs.iter().map(move |t| (w, t)))
    };
    if let Some(found) = tabs().find(|(_, t)| t.id.0 == target) {
        return Ok(found);
    }
    let by_label: Vec<_> = tabs().filter(|(_, t)| t.label == target).collect();
    match by_label.as_slice() {
        [] => Err(PlanError::Invalid(format!("no Herdr tab {target:?}"))),
        [one] => Ok(*one),
        many => Err(PlanError::Invalid(format!(
            "Herdr tab {target:?} is ambiguous: {}; use the id",
            many.iter()
                .map(|(w, t)| format!("{} ({})", t.id, w.label))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn parse_harness(s: &str) -> Result<Harness, PlanError> {
    serde_json::from_value(json!(s)).map_err(|_| PlanError::Usage(format!("unknown harness {s:?}")))
}

/// The seat's harness and which panes it adopts: panes running an agent of the seat's harness, every pane of a
/// seat without one. Agent panes of different harnesses, and a `--harness` that contradicts the agents, are refused.
struct Classified<'a> {
    harness: Harness,
    /// `(pane, adopt)` in tab order.
    panes: Vec<(&'a PaneInfo, bool)>,
}

fn classify<'a>(tab: &'a TabInfo, flagged: Option<Harness>) -> Result<Classified<'a>, PlanError> {
    let agent_of = |p: &PaneInfo| p.agent.as_ref().and_then(|a| for_agent_kind(&a.kind));
    let detected: BTreeSet<Harness> = tab.panes.iter().filter_map(agent_of).collect();
    let harness = match (detected.len(), flagged) {
        (0, Some(h)) => h,
        (0, None) => Harness::Shell,
        (1, flagged) => {
            let h = *detected.first().expect("one");
            if let Some(f) = flagged
                && f != h
            {
                return Err(PlanError::Invalid(format!(
                    "--harness {} conflicts with the {} agent in tab {}",
                    harness_name(f),
                    harness_name(h),
                    tab.label
                )));
            }
            h
        }
        _ => {
            return Err(PlanError::Invalid(format!(
                "tab {} mixes harnesses ({}); one seat has one harness",
                tab.label,
                detected
                    .iter()
                    .map(|h| harness_name(*h))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };
    let panes = tab
        .panes
        .iter()
        .map(|p| (p, detected.is_empty() || agent_of(p) == Some(harness)))
        .collect();
    Ok(Classified { harness, panes })
}

fn harness_name(h: Harness) -> String {
    json!(h).as_str().unwrap_or_default().to_owned()
}

fn clone_slugs(
    tree: &dyn TreeRead,
    seat_dir: &crate::ports::store::RepoPath,
    name: &str,
    ids: &[CloneId],
) -> Result<Vec<String>, PlanError> {
    let mut out = Vec::new();
    let mut taken = layout::taken_slugs(tree, &seat_dir.join("clones")?)?;
    for id in ids {
        let slug = unique_slug(name, id.suffix6(), &taken);
        taken.insert(slug.clone());
        out.push(slug);
    }
    Ok(out)
}

impl OrgKind for SeatAdopt {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatAdopt
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "adopt")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let tab = positional(words, 0).ok_or_else(|| {
            PlanError::Usage(
                "seat adopt <tab-id|tab-label> [--teamspace <ts>] [--name <name>] [--harness <h>]"
                    .into(),
            )
        })?;
        let harness = flag(words, "--harness")
            .map(|h| parse_harness(&h))
            .transpose()?;
        Ok(json!({
            "tab": tab,
            "name": flag(words, "--name"),
            "teamspace": flag(words, "--teamspace"),
            "harness": harness,
        }))
    }
    fn needs_live(&self) -> bool {
        true
    }
    fn live_panes(
        &self,
        args: &serde_json::Value,
        snap: &HerdrSnapshot,
    ) -> Vec<(HerdrPaneId, bool)> {
        let flagged = args["harness"].as_str().and_then(|h| parse_harness(h).ok());
        let Ok((_, tab)) = find_tab(snap, args["tab"].as_str().unwrap_or_default()) else {
            return vec![];
        };
        let Ok(c) = classify(tab, flagged) else {
            return vec![];
        };
        c.panes
            .into_iter()
            .map(|(p, adopt)| (p.id.clone(), adopt && c.harness != Harness::Shell))
            .collect()
    }
    fn bind_live(
        &self,
        args: serde_json::Value,
        snap: &HerdrSnapshot,
    ) -> Result<serde_json::Value, PlanError> {
        self.bind_live_facts(args, &LiveFacts::new(snap.clone()))
    }
    fn bind_live_facts(
        &self,
        args: serde_json::Value,
        live: &LiveFacts,
    ) -> Result<serde_json::Value, PlanError> {
        let target = args["tab"].as_str().unwrap_or_default();
        let (ws, tab) = find_tab(&live.snap, target)?;
        let flagged = args["harness"].as_str().map(parse_harness).transpose()?;
        let c = classify(tab, flagged)?;
        let mut notes = Vec::new();
        let mut panes = Vec::new();
        for (p, adopt) in &c.panes {
            let seat = if *adopt && c.harness != Harness::Shell {
                match live.seats.get(&p.id) {
                    Some(Ok(Some(s))) => Some(s.clone()),
                    Some(Ok(None)) | None => {
                        return Err(PlanError::Invalid(format!(
                            "threads resolved no seat for pane {}: threads maps no seat to it",
                            p.id
                        )));
                    }
                    Some(Err(e)) => {
                        return Err(PlanError::Invalid(format!(
                            "threads resolved no seat for pane {}: {e}",
                            p.id
                        )));
                    }
                }
            } else {
                None
            };
            panes.push(json!({
                "pane_id": p.id,
                "terminal_id": p.terminal_id,
                "token": p.metadata.get(TOKEN_KEY),
                "adopt": adopt,
                "seat": seat,
            }));
        }
        let adopted: Vec<&PaneInfo> = c
            .panes
            .iter()
            .filter(|(_, a)| *a)
            .map(|(p, _)| *p)
            .collect();
        let first = adopted.first().copied();
        let (mut model, mut seat_args) = (None, None);
        if c.harness != Harness::Shell
            && let Some(p) = first
        {
            let run = match live.processes.get(&p.id) {
                Some(Ok(info)) => profile(c.harness).observed_run(&info.foreground_argv),
                Some(Err(e)) => {
                    notes.push(format!("could not read process info of pane {}: {e}", p.id));
                    None
                }
                None => None,
            };
            match run {
                Some((m, a)) => {
                    model = m;
                    seat_args = (!a.is_empty()).then_some(a);
                }
                None => notes.push(format!(
                    "could not read the agent's args from pane {}",
                    p.id
                )),
            }
        }
        let cwd = first.and_then(|p| p.cwd.clone());
        if adopted
            .iter()
            .map(|p| p.cwd.as_ref())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            notes.push(
                "the adopted panes have different cwds; the seat records the first pane's".into(),
            );
        }
        let name = args["name"]
            .as_str()
            .map_or_else(|| tab.label.clone(), str::to_owned);
        Ok(json!({
            "tab": target,
            "tab_id": tab.id,
            "tab_label": tab.label,
            "workspace_id": ws.id,
            "workspace_label": ws.label,
            "workspace_token": ws.metadata.get(TOKEN_KEY),
            "incarnation": live.snap.incarnation,
            "name": name,
            "teamspace": args["teamspace"],
            "harness": c.harness,
            "model": model,
            "args": seat_args,
            "cwd": cwd,
            "panes": panes,
            "notes": notes,
        }))
    }
    fn admission_seat_panes(&self, args: &serde_json::Value) -> Vec<HerdrPaneId> {
        parse_args::<SeatAdoptArgs>(args)
            .map(|a| {
                a.adopted()
                    .filter(|p| p.seat.is_some())
                    .map(|p| p.pane_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: SeatAdoptArgs = parse_args(args)?;
        let Checked { ts } = Self::check(cx.tree, &a)?;
        let seat_id: SeatId = reserved.get_or_mint("seat");
        let adopted: Vec<&PaneArg> = a.adopted().collect();
        let clone_ids: Vec<CloneId> = (0..adopted.len())
            .map(|i| reserved.get_or_mint(&format!("clone:{i}")))
            .collect();
        let _act: ActionId = reserved.get_or_mint("act");
        let slug = seat_slug_for(cx.tree, &ts.loc.folder, &a.name, &seat_id, None)?;
        let seat_dir = layout::seat_dir(&ts.loc.folder, &slug);
        let slugs = clone_slugs(cx.tree, &seat_dir, &a.name, &clone_ids)?;
        let mut effects = vec![PlanEffect::new(
            "seat.adopt",
            seat_id.clone(),
            json!({
                "name": a.name,
                "teamspace": ts.rec.id,
                "path": seat_dir.as_str(),
                "tab_id": a.tab_id,
                "tab_label": a.tab_label,
                "harness": a.harness,
                "model": a.model,
                "args": a.args,
                "cwd": a.cwd,
            }),
        )];
        for ((p, id), slug) in adopted.iter().zip(&clone_ids).zip(&slugs) {
            effects.push(PlanEffect::new(
                "clone.adopt",
                id.clone(),
                json!({
                    "seat": seat_id,
                    "name": a.name,
                    "path": layout::clone_dir(&seat_dir, slug).as_str(),
                    "pane_id": p.pane_id,
                    "threads_seat": p.seat,
                }),
            ));
        }
        let relabel = a.name != a.tab_label;
        if relabel {
            effects.push(PlanEffect::new(
                "runtime.rename_tab",
                seat_id.clone(),
                json!({ "name": a.name }),
            ));
        }
        let mut relied_on = vec![rev_of(ts.rec.id.clone(), ts.rec.rev)];
        let assign = first_seat::assignment(cx.tree, Some(&seat_id))?;
        let node_manager = assign.is_some();
        if let Some((effect, graph)) = assign {
            effects.push(effect);
            relied_on.push(graph);
        }
        let mut warnings = vec![
            "closing the tab or one of its panes in Herdr retires the seat or clone (undoable)"
                .to_owned(),
            "renaming the tab in Herdr renames the seat; renaming the seat relabels the tab"
                .to_owned(),
        ];
        if a.harness != Harness::Shell {
            warnings.push("if the agent exits with the pane kept, graph starts one in it, resuming its last captured session".to_owned());
        }
        warnings.push("the seat is keyless: closing its tab and undoing that gives the pane a fresh threads seat; memberships graph does not own are not restored".to_owned());
        if node_manager {
            warnings.push(
                format!(
                    "this seat becomes the node manager: graph reopens it when it is closed; it can be released while it is the graph's only live seat, otherwise move the role first with {}",
                    crate::plan::managers::NODE_MANAGER_SET_USAGE
                ),
            );
        }
        for p in a.panes.iter().filter(|p| !p.adopt) {
            warnings.push(format!(
                "pane {} has no agent and stays unmanaged inside the seat's tab; closing the seat's tab closes it",
                p.pane_id
            ));
        }
        if relabel {
            warnings.push(format!(
                "the Herdr tab is relabeled {:?} -> {:?}",
                a.tab_label, a.name
            ));
        }
        warnings.extend(a.notes.iter().cloned());
        Ok(PlanBody {
            effects,
            relied_on,
            warnings,
            repair_required: None,
            summary: format!("adopt Herdr tab {} as seat {}", a.tab_label, a.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: SeatAdoptArgs = parse_args(args).map_err(mm)?;
        let Checked { ts } = Self::check(&cx.tree, &a).map_err(mm)?;
        let observed = cx.request.confirmed.as_ref().map(|c| &c.observed);
        for p in a.adopted() {
            let Some(planned) = &p.seat else { continue };
            let reject = |reason: &str, explanation: String| {
                MutationError::Reject(Reject {
                    reason: reason.into(),
                    explanation,
                    current_revs: vec![],
                })
            };
            match observed
                .and_then(|o| o.get("threads_seats"))
                .and_then(|m| m.get(&p.pane_id.0))
                .and_then(|v| v.as_str())
            {
                Some(now) if now == planned => {}
                Some(now) => {
                    return Err(reject(
                        "threads_seat_changed",
                        format!(
                            "pane {}'s threads seat is now {now} (planned {planned}); plan again",
                            p.pane_id
                        ),
                    ));
                }
                None => {
                    return Err(reject(
                        "threads_seat_unresolved",
                        format!(
                            "threads seat of pane {} was not resolved at admission; plan again",
                            p.pane_id
                        ),
                    ));
                }
            }
        }
        let seat_id: SeatId = plan
            .reserved
            .get("seat")
            .ok_or_else(|| MutationError::Bug("plan reserved no seat id".into()))?;
        let act: ActionId = plan
            .reserved
            .get("act")
            .ok_or_else(|| MutationError::Bug("plan reserved no action id".into()))?;
        let adopted: Vec<&PaneArg> = a.adopted().collect();
        let mut clone_ids = Vec::new();
        for i in 0..adopted.len() {
            clone_ids.push(
                plan.reserved
                    .get::<CloneId>(&format!("clone:{i}"))
                    .ok_or_else(|| MutationError::Bug(format!("plan reserved no clone:{i} id")))?,
            );
        }
        let slug = seat_slug_for(&cx.tree, &ts.loc.folder, &a.name, &seat_id, None).map_err(mm)?;
        let seat_dir = layout::seat_dir(&ts.loc.folder, &slug);
        let slugs = clone_slugs(&cx.tree, &seat_dir, &a.name, &clone_ids).map_err(mm)?;
        let binding = |pane: Option<(&PaneArg,)>| Binding {
            token: None,
            workspace_id: Some(a.workspace_id.clone()),
            tab_id: Some(a.tab_id.clone()),
            pane_id: pane.map(|(p,)| p.pane_id.clone()),
            terminal_id: pane.and_then(|(p,)| p.terminal_id.clone()),
            incarnation: a.incarnation.clone(),
        };
        let present = |b: Binding| Runtime {
            availability: Availability::Present,
            bound: Some(b),
            observed_at: Some(cx.now),
        };
        let mut rec = SeatRecord {
            schema: SCHEMA_VERSION,
            id: seat_id.clone(),
            rev: 0,
            name: a.name.clone(),
            name_history: vec![],
            teamspace: ts.rec.id.clone(),
            lifecycle: Lifecycle::Active,
            retired: None,
            system_duty: None,
            template_ref: None,
            manager: None,
            applications: vec![],
            overrides: Default::default(),
            participation: Default::default(),
            activation: Default::default(),
            runtime: present(binding(None)),
            channel: Default::default(),
            reload_required: false,
            moved_out: false,
        };
        rec.activation.last_op = Some(cx.op.clone());
        rec.overrides.harness = Some(a.harness);
        rec.overrides.model = a.model.clone();
        rec.overrides.args = a.args.clone();
        rec.overrides.cwd = a.cwd.as_ref().map(PathBuf::from);
        let mut acc = Acc::default();
        acc.affected.push(AffectedObject {
            object: seat_id.to_any(),
            before: None,
            after: Some(lc_value("active")),
        });
        for ((p, id), slug) in adopted.iter().zip(&clone_ids).zip(&slugs) {
            let mut clone = new_clone_record(&seat_id, &a.name, id.clone());
            clone.keyless = true;
            clone.threads_seat = p.seat.clone();
            clone.runtime = present(binding(Some((p,))));
            cx.tree.put_record(
                layout::clone_record(&layout::clone_dir(&seat_dir, slug)),
                &mut clone,
            )?;
            acc.affected.push(AffectedObject {
                object: id.to_any(),
                before: None,
                after: Some(lc_value("active")),
            });
        }
        cx.tree
            .put_record(layout::seat_record(&seat_dir), &mut rec)?;
        first_seat::apply_assignment(&mut cx.tree, &plan.effects)?;
        let mut comp = toml::Table::new();
        comp.insert("seat".into(), toml::Value::String(seat_id.to_string()));
        comp.insert(
            "teamspace".into(),
            toml::Value::String(ts.rec.id.to_string()),
        );
        comp.insert("tab_id".into(), toml::Value::String(a.tab_id.to_string()));
        comp.insert(
            "clones".into(),
            toml::Value::Array(
                clone_ids
                    .iter()
                    .map(|c| toml::Value::String(c.to_string()))
                    .collect(),
            ),
        );
        write_action(cx, &act, ActionKind::Adopt, acc, comp)?;
        Ok(Applied {
            summary: format!("adopt Herdr tab {} as seat {}", a.tab_label, a.name),
            action: Some(act),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands::{
        LiveSources, admit_apply_with, create_plan_facts, create_plan_live, gather_live,
    };
    use super::super::kinds_extra::testkit::{Fx, fx_fresh_with, fx_with, words};
    use super::super::types::StoredPlan;
    use super::*;
    use crate::daemon::registry::CallerInfo;
    use crate::journal::OpRow;
    use crate::model::PlanId;
    use crate::model::clone::CloneRecord;
    use crate::model::operation::OpState;
    use crate::ports::herdr::{AgentInfo, AgentStatus, ProcessInfo};
    use crate::ports::store::Store;
    use crate::store::tree::CommitView;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn pane(id: &str, agent: Option<&str>, cwd: &str) -> PaneInfo {
        PaneInfo {
            id: HerdrPaneId(id.into()),
            terminal_id: Some(HerdrTerminalId(format!("t-{id}"))),
            label: None,
            cwd: Some(PathBuf::from(cwd)),
            metadata: BTreeMap::new(),
            agent: agent.map(|k| AgentInfo {
                kind: k.into(),
                status: AgentStatus::Idle,
                session: None,
            }),
        }
    }

    fn snap(tabs: Vec<(&str, Vec<PaneInfo>)>) -> HerdrSnapshot {
        HerdrSnapshot {
            incarnation: Incarnation {
                generation: 1,
                server_pid: Some(42),
                server_started: None,
            },
            workspaces: vec![WorkspaceInfo {
                id: HerdrWorkspaceId("w1".into()),
                label: "legacy".into(),
                metadata: BTreeMap::new(),
                tabs: tabs
                    .into_iter()
                    .map(|(l, panes)| TabInfo {
                        id: HerdrTabId(format!("w1:{l}")),
                        label: l.into(),
                        panes,
                    })
                    .collect(),
            }],
        }
    }

    fn mixed_snap() -> HerdrSnapshot {
        snap(vec![
            (
                "legacy",
                vec![
                    pane("p1", Some("claude"), "/w/legacy"),
                    pane("p2", None, "/w/legacy"),
                ],
            ),
            ("tools", vec![pane("p3", None, "/w/tools")]),
        ])
    }

    fn facts(s: HerdrSnapshot, seats: &[(&str, &str)]) -> LiveFacts {
        let mut f = LiveFacts::new(s);
        let ids: Vec<HerdrPaneId> = f
            .snap
            .workspaces
            .iter()
            .flat_map(|w| {
                w.tabs
                    .iter()
                    .flat_map(|t| t.panes.iter().map(|p| p.id.clone()))
            })
            .collect();
        for id in ids {
            f.processes.insert(
                id.clone(),
                Ok(ProcessInfo {
                    foreground_pid: Some(7),
                    foreground_argv: [
                        "claude",
                        "--model",
                        "opus",
                        "--dangerously-skip-permissions",
                    ]
                    .map(String::from)
                    .to_vec(),
                    is_shell: false,
                }),
            );
        }
        for (p, s) in seats {
            f.seats
                .insert(HerdrPaneId((*p).into()), Ok(Some((*s).to_owned())));
        }
        f
    }

    fn kinds(k: &mut super::super::kind::KindRegistry) {
        crate::templates::register_kinds(k);
        crate::undo::register_kinds(k);
    }

    fn adopted_fx(fresh: bool) -> Fx {
        let fx = if fresh {
            fx_fresh_with(kinds)
        } else {
            fx_with(kinds)
        };
        let v = create_plan_live(
            &fx.deps,
            &CallerInfo::default(),
            words("teamspace adopt legacy"),
            Some(&mixed_snap()),
        )
        .unwrap();
        let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        let sp = fx.deps.plans.get(&id).unwrap().unwrap();
        let row = apply(&fx, &sp, &[]);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        fx
    }

    fn plan_facts(fx: &Fx, change: &str, f: &LiveFacts) -> Result<StoredPlan, String> {
        let v = create_plan_facts(&fx.deps, &CallerInfo::default(), words(change), Some(f))
            .map_err(|e| e.message)?;
        let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        Ok(fx.deps.plans.get(&id).unwrap().unwrap())
    }

    fn apply(fx: &Fx, sp: &StoredPlan, seats: &[(&str, &str)]) -> OpRow {
        let mut extra = serde_json::Map::new();
        if !seats.is_empty() {
            let m: serde_json::Map<String, serde_json::Value> = seats
                .iter()
                .map(|(p, s)| ((*p).to_owned(), json!(s)))
                .collect();
            extra.insert("threads_seats".into(), serde_json::Value::Object(m));
        }
        let op = admit_apply_with(
            &fx.deps,
            &CallerInfo::default(),
            sp.plan.id.as_str(),
            Some(&sp.hash),
            "relay",
            extra,
        )
        .unwrap_or_else(|e| panic!("admit: {}", e.message));
        fx.w.drain().unwrap();
        fx.w.journal().get(&op).unwrap().unwrap()
    }

    fn view(fx: &Fx) -> CommitView<'_> {
        CommitView {
            store: &*fx.store,
            at: fx.store.head().unwrap(),
        }
    }

    fn seats(fx: &Fx) -> Vec<SeatRecord> {
        layout::all_seats(&view(fx))
            .unwrap()
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    fn clones(fx: &Fx) -> Vec<CloneRecord> {
        layout::all_clones(&view(fx))
            .unwrap()
            .into_iter()
            .map(|(_, c)| c)
            .collect()
    }

    fn kinds_of(sp: &StoredPlan) -> Vec<&str> {
        sp.plan.effects.iter().map(|e| e.kind.as_str()).collect()
    }

    #[test]
    fn seat_adopt_writes_a_keyless_seated_clone_bound_to_the_pane() {
        let fx = adopted_fx(false);
        let f = facts(mixed_snap(), &[("p1", "s1")]);
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        assert_eq!(kinds_of(&sp), ["seat.adopt", "clone.adopt"]);
        let row = apply(&fx, &sp, &[("p1", "s1")]);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        let seat = &seats(&fx)[0];
        assert_eq!(seat.lifecycle, Lifecycle::Active);
        assert_eq!(seat.name, "legacy");
        let b = seat.runtime.bound.as_ref().unwrap();
        assert_eq!(b.tab_id, Some(HerdrTabId("w1:legacy".into())));
        assert_eq!(seat.overrides.harness, Some(Harness::Claude));
        assert_eq!(seat.overrides.model.as_deref(), Some("opus"));
        assert_eq!(
            seat.overrides.args,
            Some(vec!["--dangerously-skip-permissions".to_owned()])
        );
        assert_eq!(seat.overrides.cwd, Some(PathBuf::from("/w/legacy")));
        let cs = clones(&fx);
        assert_eq!(cs.len(), 1, "the shell pane has no clone");
        let c = &cs[0];
        assert!(c.keyless);
        assert_eq!(c.threads_seat.as_deref(), Some("s1"));
        assert!(c.key_reservation.is_none());
        let cb = c.runtime.bound.as_ref().unwrap();
        assert_eq!(cb.pane_id, Some(HerdrPaneId("p1".into())));
        assert_eq!(cb.incarnation, mixed_snap().incarnation);
        let act = row.action.expect("an adopt action");
        let rec = crate::undo::candidates::read_action(&view(&fx), &act)
            .unwrap()
            .unwrap();
        assert_eq!(rec.kind, ActionKind::Adopt);
        assert_eq!(rec.compensation["seat"].as_str(), Some(seat.id.as_str()));
        assert!(
            sp.plan
                .warnings
                .iter()
                .any(|w| w.contains("pane p2 has no agent"))
        );
    }

    #[test]
    fn seat_adopt_of_a_shell_tab_is_a_seatless_shell_seat() {
        let fx = adopted_fx(false);
        let live = mixed_snap();
        let tab_panes = live_panes_of(&live, "tools");
        assert_eq!(tab_panes, vec![(HerdrPaneId("p3".into()), false)]);
        let f = facts(live, &[]);
        let sp = plan_facts(&fx, "seat adopt tools", &f).unwrap();
        let row = apply(&fx, &sp, &[]);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        assert_eq!(seats(&fx)[0].overrides.harness, Some(Harness::Shell));
        let cs = clones(&fx);
        assert_eq!(cs.len(), 1);
        assert!(cs[0].threads_seat.is_none() && cs[0].keyless);
        assert!(
            !sp.plan
                .warnings
                .iter()
                .any(|w| w.contains("graph starts one"))
        );
    }

    fn live_panes_of(s: &HerdrSnapshot, tab: &str) -> Vec<(HerdrPaneId, bool)> {
        SeatAdopt.live_panes(&json!({ "tab": tab, "harness": null }), s)
    }

    #[test]
    fn seat_adopt_with_harness_flag_seats_every_pane() {
        let fx = adopted_fx(false);
        let live = mixed_snap();
        assert_eq!(
            SeatAdopt.live_panes(&json!({ "tab": "tools", "harness": "claude" }), &live),
            vec![(HerdrPaneId("p3".into()), true)]
        );
        let f = facts(live, &[("p3", "s3")]);
        let sp = plan_facts(&fx, "seat adopt tools --harness claude", &f).unwrap();
        let row = apply(&fx, &sp, &[("p3", "s3")]);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        assert_eq!(seats(&fx)[0].overrides.harness, Some(Harness::Claude));
        assert_eq!(clones(&fx)[0].threads_seat.as_deref(), Some("s3"));
    }

    #[test]
    fn seat_adopt_refuses_mixed_harnesses_an_unadopted_workspace_a_bound_tab_and_a_managed_pane() {
        let fx = adopted_fx(false);
        let mixed = snap(vec![(
            "both",
            vec![
                pane("a", Some("claude"), "/x"),
                pane("b", Some("codex"), "/x"),
            ],
        )]);
        let e = plan_facts(
            &fx,
            "seat adopt both",
            &facts(mixed, &[("a", "s1"), ("b", "s2")]),
        )
        .unwrap_err();
        assert!(e.contains("mixes harnesses (claude, codex)"), "{e}");
        let e = plan_facts(
            &fx,
            "seat adopt legacy --harness codex",
            &facts(mixed_snap(), &[("p1", "s1")]),
        )
        .unwrap_err();
        assert!(e.contains("conflicts"), "{e}");
        let e = plan_facts(&fx, "seat adopt nope", &facts(mixed_snap(), &[])).unwrap_err();
        assert!(e.contains("no Herdr tab \"nope\""), "{e}");
        let e = plan_facts(&fx, "seat adopt legacy", &facts(mixed_snap(), &[])).unwrap_err();
        assert!(e.contains("threads resolved no seat for pane p1"), "{e}");

        // A workspace that is not a teamspace.
        let fresh = fx_with(kinds);
        let e = plan_facts(&fresh, "seat adopt tools", &facts(mixed_snap(), &[])).unwrap_err();
        assert!(
            e.contains("workspace legacy is not a teamspace; plan teamspace adopt w1 first"),
            "{e}"
        );

        // Another teamspace named.
        crate::plan::kinds_extra::testkit::commit(&fx, "teamspace create other");
        let e = plan_facts(
            &fx,
            "seat adopt tools --teamspace other",
            &facts(mixed_snap(), &[]),
        )
        .unwrap_err();
        assert!(e.contains("not other"), "{e}");

        // A bound tab.
        let sp = plan_facts(&fx, "seat adopt tools", &facts(mixed_snap(), &[])).unwrap();
        assert_eq!(apply(&fx, &sp, &[]).state, OpState::Committed);
        let e = plan_facts(&fx, "seat adopt tools", &facts(mixed_snap(), &[])).unwrap_err();
        assert!(e.contains("tab tools is already seat tools"), "{e}");

        // A pane managed by a live clone.
        let clone = clones(&fx)[0].id.clone();
        let mut s = mixed_snap();
        s.workspaces[0].tabs[0].panes[1].metadata.insert(
            TOKEN_KEY.into(),
            crate::model::launch::graph_token(&clone.to_any()),
        );
        let e = plan_facts(&fx, "seat adopt legacy", &facts(s, &[("p1", "s1")])).unwrap_err();
        assert!(
            e.contains(&format!("pane p2 is managed by graph (clone {clone})")),
            "{e}"
        );
    }

    #[test]
    fn seat_adopt_refuses_a_seat_held_by_another_clone() {
        let fx = adopted_fx(false);
        let sp = plan_facts(
            &fx,
            "seat adopt tools --harness claude",
            &facts(mixed_snap(), &[("p3", "s9")]),
        )
        .unwrap();
        assert_eq!(apply(&fx, &sp, &[("p3", "s9")]).state, OpState::Committed);
        let again = snap(vec![("other", vec![pane("q1", Some("claude"), "/x")])]);
        let mut again = again;
        again.workspaces[0].tabs[0].id = HerdrTabId("w1:other".into());
        let e = plan_facts(&fx, "seat adopt other", &facts(again, &[("q1", "s9")])).unwrap_err();
        assert!(
            e.contains("threads seat s9 of pane q1 is held by clone"),
            "{e}"
        );
    }

    #[test]
    fn seat_adopt_rejects_a_changed_seat_at_apply() {
        let fx = adopted_fx(false);
        let f = facts(mixed_snap(), &[("p1", "s1")]);
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        let row = apply(&fx, &sp, &[("p1", "s2")]);
        assert_eq!(row.state, OpState::Rejected);
        assert_eq!(
            row.rejection.as_ref().unwrap().reason,
            "threads_seat_changed"
        );
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        let row = apply(&fx, &sp, &[]);
        assert_eq!(
            row.rejection.as_ref().unwrap().reason,
            "threads_seat_unresolved"
        );
        assert!(seats(&fx).is_empty() && clones(&fx).is_empty());
    }

    #[test]
    fn seat_adopt_honours_first_seat() {
        let fx = adopted_fx(true);
        let f = facts(mixed_snap(), &[("p1", "s1")]);
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        assert_eq!(
            kinds_of(&sp),
            ["seat.adopt", "clone.adopt", "graph.node_manager_seat"]
        );
        assert!(
            sp.plan
                .warnings
                .iter()
                .any(|w| w.contains("becomes the node manager"))
        );
        let row = apply(&fx, &sp, &[("p1", "s1")]);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        assert_eq!(
            layout::read_graph(&view(&fx)).unwrap().node_manager_seat,
            Some(seats(&fx)[0].id.clone())
        );
    }

    #[test]
    fn seat_adopt_prints_the_g5_warnings() {
        let fx = adopted_fx(false);
        let sp = plan_facts(
            &fx,
            "seat adopt legacy",
            &facts(mixed_snap(), &[("p1", "s1")]),
        )
        .unwrap();
        assert_eq!(
            sp.plan.warnings,
            [
                "closing the tab or one of its panes in Herdr retires the seat or clone (undoable)",
                "renaming the tab in Herdr renames the seat; renaming the seat relabels the tab",
                "if the agent exits with the pane kept, graph starts one in it, resuming its last captured session",
                "the seat is keyless: closing its tab and undoing that gives the pane a fresh threads seat; memberships graph does not own are not restored",
                "pane p2 has no agent and stays unmanaged inside the seat's tab; closing the seat's tab closes it",
            ]
        );
    }

    #[test]
    fn seat_adopt_name_flag_relabels() {
        let fx = adopted_fx(false);
        let sp = plan_facts(
            &fx,
            "seat adopt legacy --name boss",
            &facts(mixed_snap(), &[("p1", "s1")]),
        )
        .unwrap();
        assert!(kinds_of(&sp).contains(&"runtime.rename_tab"));
        assert!(
            sp.plan
                .warnings
                .iter()
                .any(|w| w.contains("relabeled \"legacy\" -> \"boss\""))
        );
        assert_eq!(apply(&fx, &sp, &[("p1", "s1")]).state, OpState::Committed);
        assert_eq!(seats(&fx)[0].name, "boss");
    }

    #[test]
    fn seat_adopt_notes_unreadable_args() {
        let fx = adopted_fx(false);
        let mut f = facts(mixed_snap(), &[("p1", "s1")]);
        f.processes.insert(
            HerdrPaneId("p1".into()),
            Ok(ProcessInfo {
                foreground_pid: None,
                foreground_argv: vec!["node".into(), "x.js".into()],
                is_shell: false,
            }),
        );
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        assert!(
            sp.plan
                .warnings
                .iter()
                .any(|w| w.contains("could not read the agent's args from pane p1"))
        );
    }

    #[tokio::test]
    async fn seat_adopt_needs_threads() {
        use crate::ports::herdr::{CreateTab, CreateWorkspace, HerdrApi};
        use crate::threads::FakePaneSeatMap;
        let fake = crate::herdr::fake::FakeHerdr::new();
        let ws = fake
            .create_workspace(CreateWorkspace {
                label: "legacy".into(),
                cwd: "/".into(),
                env: vec![],
            })
            .await
            .unwrap();
        let tab = fake
            .create_tab(CreateTab {
                workspace: ws.workspace.clone().unwrap(),
                label: "work".into(),
                cwd: "/".into(),
                env: vec![],
            })
            .await
            .unwrap();
        let pane = tab.pane.clone().unwrap();
        fake.set_agent(
            &pane,
            Some(AgentInfo {
                kind: "claude".into(),
                status: AgentStatus::Idle,
                session: None,
            }),
        );
        let args = json!({ "tab": "work", "harness": null });
        let mut src = LiveSources {
            herdr: Some(fake.clone() as Arc<dyn HerdrApi>),
            seats: None,
        };
        let e = gather_live(&src, &SeatAdopt, &args).await.unwrap_err();
        assert!(e.message.contains("no threads connection"), "{}", e.message);
        let map = Arc::new(FakePaneSeatMap::new());
        map.set(&pane, "s7");
        src.seats = Some(map);
        let f = gather_live(&src, &SeatAdopt, &args).await.unwrap();
        assert_eq!(f.seats[&pane], Ok(Some("s7".to_owned())));
        assert!(f.processes.contains_key(&pane));
    }
}
