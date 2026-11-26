//! `seat release <seat>` and `teamspace release <ts>`: the way back out of an adoption (design 2026-11-07 G4,
//! section 3). A release retires the records with mechanism `released` (history kept), drops their bindings and
//! touches nothing in Herdr or threads: the tab, panes and agents keep running and the threads seat keeps its
//! memberships. Only adopted objects can be released; graph-created ones are retired.
//!
//! Undo of an adoption is a release too (`undo_plan` / `undo_mutate`, called by the undo kind), so it never
//! closes a tab.
use super::core_kinds::{
    Acc, SeatFacts, TsInfo, absent, basename, do_retire_seat, mm, parse_args, read_seat, read_ts,
    rev_of, write_action,
};
use super::grammar::{self, Scope, positional};
use super::kind::{OrgKind, PlanBody, PlanCx, PlanError};
use super::managers::reparent::reparent_effects;
use super::types::{Plan, PlanEffect, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::change::RequestKind;
use crate::model::common::{CloneLifecycle, Lifecycle, RetireMechanism, Retirement};
use crate::model::{ActionId, AnyId, SeatId, TeamspaceId};
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError};
use serde::Deserialize;
use serde_json::json;

/// The live `adopt` action that bound `id` (a seat or a teamspace): its compensation names the object and nothing
/// has undone it. A seat adoption also records its teamspace, so a teamspace is matched only by an action that
/// names no seat.
pub(crate) fn adoption(
    tree: &dyn TreeRead,
    id: &AnyId,
) -> Result<Option<ActionRecord>, StoreError> {
    let want = id.to_string();
    let key = if matches!(id.kind(), crate::model::IdKind::Seat) {
        "seat"
    } else {
        "teamspace"
    };
    Ok(layout::list_actions(tree)?
        .into_iter()
        .map(|(_, a)| a)
        .find(|a| {
            a.kind == ActionKind::Adopt
                && a.undone_by.is_empty()
                && a.compensation.get(key).and_then(|v| v.as_str()) == Some(want.as_str())
                && (key == "seat" || !a.compensation.contains_key("seat"))
        }))
}

/// Refuse a seat that graph created or added to, or that is already gone.
fn check_seat(tree: &dyn TreeRead, f: &SeatFacts) -> Result<(), PlanError> {
    if f.rec.lifecycle == Lifecycle::Retired {
        return Err(PlanError::Invalid(format!(
            "seat {} is already retired",
            f.rec.name
        )));
    }
    if adoption(tree, &f.rec.id.to_any())?.is_none() {
        return Err(PlanError::Invalid(format!(
            "seat {} was created by graph, not adopted; use `seat retire`, which closes its tab",
            f.rec.name
        )));
    }
    if let Some((_, c)) = f
        .clones
        .iter()
        .find(|(_, c)| c.lifecycle != CloneLifecycle::Retired && !c.keyless)
    {
        return Err(PlanError::Invalid(format!(
            "clone {} was added by graph; seat {} cannot be released",
            c.id, f.rec.name
        )));
    }
    Ok(())
}

/// `seat.release` plus one `clone.release` per live clone. Never a `runtime.*` effect.
fn seat_effects(f: &SeatFacts) -> Vec<PlanEffect> {
    let archive = layout::archived_seat_dir(&f.ts.loc.folder, basename(&f.loc.folder), &f.rec.id);
    let mut effects = vec![PlanEffect::new(
        "seat.release",
        f.rec.id.clone(),
        json!({ "name": f.rec.name, "path_from": f.loc.folder.as_str(), "path_to": archive.as_str() }),
    )];
    for (_, c) in f.active_clones() {
        effects.push(PlanEffect::new(
            "clone.release",
            c.id.clone(),
            json!({ "seat": f.rec.id, "name": c.name }),
        ));
    }
    effects
}

fn seat_warnings() -> Vec<String> {
    vec![
        "the Herdr tab, its panes and agents keep running; graph forgets them".to_owned(),
        "graph releases its own channel invitations; the threads seat and its other memberships stay".to_owned(),
        "graph's token stays on the Herdr panes; graph ignores a released object's token".to_owned(),
    ]
}

pub(crate) fn seat_plan(
    tree: &dyn TreeRead,
    seat: &SeatId,
    reserved: &mut Reserved,
) -> Result<PlanBody, PlanError> {
    let f = read_seat(tree, seat)?;
    check_seat(tree, &f)?;
    let _act: ActionId = reserved.get_or_mint("act");
    let mut effects = seat_effects(&f);
    effects.extend(reparent_effects(tree, std::slice::from_ref(&f.rec.id))?);
    Ok(PlanBody {
        effects,
        relied_on: vec![rev_of(f.rec.id.clone(), f.rec.rev)],
        warnings: seat_warnings(),
        repair_required: None,
        summary: format!("release seat {}", f.rec.name),
    })
}

/// Drop the bindings of a released seat and of the clones this release retired.
fn drop_bindings(
    cx: &mut MutationCx<'_>,
    seat: &SeatId,
    act: &ActionId,
) -> Result<(), MutationError> {
    let f = read_seat(&cx.tree, seat).map_err(mm)?;
    for (loc, c) in &f.clones {
        if c.retired
            .as_ref()
            .is_some_and(|r| r.action.as_ref() == Some(act))
        {
            let mut c = c.clone();
            c.runtime = absent();
            cx.tree.put_record(loc.record_path.clone(), &mut c)?;
        }
    }
    let mut rec = f.rec.clone();
    rec.runtime = absent();
    cx.tree.put_record(f.loc.record_path.clone(), &mut rec)?;
    Ok(())
}

fn released_by(cx: &MutationCx<'_>) -> Retirement {
    Retirement {
        op: cx.op.clone(),
        action: None,
        at: cx.now,
        mechanism: RetireMechanism::Released,
    }
}

pub(crate) fn seat_mutate(
    cx: &mut MutationCx<'_>,
    seat: &SeatId,
    act: &ActionId,
) -> Result<Applied, MutationError> {
    let f = read_seat(&cx.tree, seat).map_err(mm)?;
    check_seat(&cx.tree, &f).map_err(mm)?;
    let mut acc = Acc::default();
    do_retire_seat(cx, seat, act, RetireMechanism::Released, &mut acc, false)?;
    drop_bindings(cx, seat, act)?;
    let mut comp = toml::Table::new();
    comp.insert("seat".into(), toml::Value::String(seat.to_string()));
    write_action(cx, act, ActionKind::Release, acc, comp)?;
    Ok(Applied {
        summary: format!("release seat {}", f.rec.name),
        action: Some(act.clone()),
    })
}

/// Live seats of a teamspace, in listing order.
fn live_seats(tree: &dyn TreeRead, ts: &TsInfo) -> Result<Vec<SeatFacts>, PlanError> {
    let mut out = Vec::new();
    for (_, s) in layout::list_seats(tree, &ts.loc.folder)? {
        if s.lifecycle != Lifecycle::Retired {
            out.push(read_seat(tree, &s.id)?);
        }
    }
    Ok(out)
}

fn check_teamspace(tree: &dyn TreeRead, ts: &TsInfo) -> Result<Vec<SeatFacts>, PlanError> {
    if ts.rec.lifecycle == Lifecycle::Retired {
        return Err(PlanError::Invalid(format!(
            "teamspace {} is already retired",
            ts.rec.name
        )));
    }
    if adoption(tree, &ts.rec.id.to_any())?.is_none() {
        return Err(PlanError::Invalid(format!(
            "teamspace {} was created by graph; use `teamspace retire`",
            ts.rec.name
        )));
    }
    let seats = live_seats(tree, ts)?;
    let mut graph_made = Vec::new();
    for f in &seats {
        if adoption(tree, &f.rec.id.to_any())?.is_none() {
            graph_made.push(f.rec.name.clone());
        }
    }
    if !graph_made.is_empty() {
        return Err(PlanError::Invalid(format!(
            "teamspace {} has seats that graph created: {}; release or retire these first",
            ts.rec.name,
            graph_made.join(", ")
        )));
    }
    for f in &seats {
        check_seat(tree, f)?;
    }
    Ok(seats)
}

pub(crate) fn teamspace_plan(
    tree: &dyn TreeRead,
    id: &TeamspaceId,
    reserved: &mut Reserved,
) -> Result<PlanBody, PlanError> {
    let ts = read_ts(tree, id)?;
    let seats = check_teamspace(tree, &ts)?;
    let _act: ActionId = reserved.get_or_mint("act");
    let archive = layout::archived_teamspace_dir(basename(&ts.loc.folder), id);
    let mut effects = vec![PlanEffect::new(
        "teamspace.release",
        id.clone(),
        json!({ "name": ts.rec.name, "path_from": ts.loc.folder.as_str(), "path_to": archive.as_str() }),
    )];
    let mut relied_on = vec![rev_of(id.clone(), ts.rec.rev)];
    let mut releasing = Vec::new();
    for f in &seats {
        effects.extend(seat_effects(f));
        relied_on.push(rev_of(f.rec.id.clone(), f.rec.rev));
        releasing.push(f.rec.id.clone());
    }
    effects.extend(reparent_effects(tree, &releasing)?);
    let mut warnings = seat_warnings();
    warnings[0] =
        "the Herdr workspace, its tabs, panes and agents keep running; graph forgets them"
            .to_owned();
    Ok(PlanBody {
        effects,
        relied_on,
        warnings,
        repair_required: None,
        summary: format!("release teamspace {}", ts.rec.name),
    })
}

pub(crate) fn teamspace_mutate(
    cx: &mut MutationCx<'_>,
    id: &TeamspaceId,
    act: &ActionId,
) -> Result<Applied, MutationError> {
    let ts = read_ts(&cx.tree, id).map_err(mm)?;
    let seats = check_teamspace(&cx.tree, &ts).map_err(mm)?;
    let mut acc = Acc::default();
    acc.retiring = seats.iter().map(|f| f.rec.id.clone()).collect();
    for f in &seats {
        do_retire_seat(
            cx,
            &f.rec.id,
            act,
            RetireMechanism::Released,
            &mut acc,
            false,
        )?;
        drop_bindings(cx, &f.rec.id, act)?;
    }
    // Re-read: releasing the seats may have reparented this teamspace's own default_manager.
    let mut rec = read_ts(&cx.tree, id).map_err(mm)?.rec;
    let before = if rec.lifecycle == Lifecycle::Active {
        "active"
    } else {
        "dormant"
    };
    rec.lifecycle = Lifecycle::Retired;
    rec.retired = Some(Retirement {
        action: Some(act.clone()),
        ..released_by(cx)
    });
    rec.runtime = absent();
    cx.tree.put_record(ts.loc.record_path.clone(), &mut rec)?;
    acc.retired.insert(0, id.to_any());
    acc.changed(id.to_any(), before, "retired");
    cx.tree.move_dir(
        &ts.loc.folder,
        &layout::archived_teamspace_dir(basename(&ts.loc.folder), id),
    )?;
    let mut comp = toml::Table::new();
    comp.insert("teamspace".into(), toml::Value::String(id.to_string()));
    write_action(cx, act, ActionKind::Release, acc, comp)?;
    Ok(Applied {
        summary: format!("release teamspace {}", ts.rec.name),
        action: Some(act.clone()),
    })
}

/// What an adopt action bound: the seat it names, else the teamspace.
enum Adopted {
    Seat(SeatId),
    Teamspace(TeamspaceId),
}

fn adopted(orig: &ActionRecord) -> Result<Adopted, PlanError> {
    let s = |k: &str| orig.compensation.get(k).and_then(|v| v.as_str());
    let bad = || {
        PlanError::Invalid(format!(
            "adopt action {} names no seat or teamspace",
            orig.id
        ))
    };
    if let Some(seat) = s("seat") {
        return Ok(Adopted::Seat(seat.parse().map_err(|_| bad())?));
    }
    Ok(Adopted::Teamspace(
        s("teamspace").ok_or_else(bad)?.parse().map_err(|_| bad())?,
    ))
}

/// Undo of an adoption, plan side: the release of what the action adopted.
pub(crate) fn undo_plan(
    tree: &dyn TreeRead,
    orig: &ActionRecord,
    reserved: &mut Reserved,
) -> Result<PlanBody, PlanError> {
    match adopted(orig)? {
        Adopted::Seat(id) => seat_plan(tree, &id, reserved),
        Adopted::Teamspace(id) => teamspace_plan(tree, &id, reserved),
    }
}

/// Undo of an adoption, mutation side.
pub(crate) fn undo_mutate(
    cx: &mut MutationCx<'_>,
    orig: &ActionRecord,
    act: &ActionId,
) -> Result<Applied, MutationError> {
    match adopted(orig).map_err(mm)? {
        Adopted::Seat(id) => seat_mutate(cx, &id, act),
        Adopted::Teamspace(id) => teamspace_mutate(cx, &id, act),
    }
}

#[derive(Deserialize)]
struct SeatArgs {
    seat: String,
}

#[derive(Deserialize)]
struct TeamspaceArgs {
    teamspace: String,
}

fn act_of(plan: &Plan) -> Result<ActionId, MutationError> {
    plan.reserved
        .get("act")
        .ok_or_else(|| MutationError::Bug("plan reserved no act id".into()))
}

pub(crate) struct SeatRelease;

impl OrgKind for SeatRelease {
    fn kind(&self) -> RequestKind {
        RequestKind::SeatRelease
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("seat", "release")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let s =
            positional(words, 0).ok_or_else(|| PlanError::Usage("seat release <seat>".into()))?;
        Ok(json!({ "seat": s }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: SeatArgs = parse_args(args)?;
        let id = grammar::resolve_seat(cx.tree, &a.seat, Scope::Live)?;
        seat_plan(cx.tree, &id, reserved)
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: SeatArgs = parse_args(args).map_err(mm)?;
        let id = grammar::resolve_seat(&cx.tree, &a.seat, Scope::Live).map_err(mm)?;
        seat_mutate(cx, &id, &act_of(plan)?)
    }
}

pub(crate) struct TeamspaceRelease;

impl OrgKind for TeamspaceRelease {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceRelease
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "release")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let ts = positional(words, 0)
            .ok_or_else(|| PlanError::Usage("teamspace release <teamspace>".into()))?;
        Ok(json!({ "teamspace": ts }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: TeamspaceArgs = parse_args(args)?;
        let id = grammar::resolve_teamspace(cx.tree, &a.teamspace, Scope::Live)?;
        teamspace_plan(cx.tree, &id, reserved)
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: TeamspaceArgs = parse_args(args).map_err(mm)?;
        let id = grammar::resolve_teamspace(&cx.tree, &a.teamspace, Scope::Live).map_err(mm)?;
        teamspace_mutate(cx, &id, &act_of(plan)?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands::{
        admit_apply_with, create_plan, create_plan_facts, create_plan_live,
    };
    use super::super::kind::LiveFacts;
    use super::super::kinds_extra::testkit::{Fx, commit, fx_fresh_with, fx_with, words};
    use super::super::types::StoredPlan;
    use super::*;
    use crate::journal::OpRow;
    use crate::model::clone::CloneRecord;
    use crate::model::common::Incarnation;
    use crate::model::operation::OpState;
    use crate::model::seat::SeatRecord;
    use crate::model::teamspace::TeamspaceRecord;
    use crate::model::{HerdrPaneId, HerdrTabId, HerdrTerminalId, HerdrWorkspaceId, PlanId};
    use crate::ports::herdr::{
        AgentInfo, AgentStatus, HerdrSnapshot, PaneInfo, ProcessInfo, TabInfo, WorkspaceInfo,
    };
    use crate::ports::store::Store;
    use crate::reconcile::planner::TOKEN_KEY;
    use crate::store::tree::CommitView;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn pane(id: &str, agent: Option<&str>, token: Option<String>) -> PaneInfo {
        PaneInfo {
            id: HerdrPaneId(id.into()),
            terminal_id: Some(HerdrTerminalId(format!("t-{id}"))),
            label: None,
            cwd: Some(PathBuf::from("/w")),
            metadata: token
                .map(|t| BTreeMap::from([(TOKEN_KEY.to_owned(), t)]))
                .unwrap_or_default(),
            agent: agent.map(|k| AgentInfo {
                kind: k.into(),
                status: AgentStatus::Idle,
                session: None,
            }),
        }
    }

    /// Workspace `legacy` with an agent tab `legacy` (pane p1) and a shell tab `tools` (pane p3).
    fn snap(p1_token: Option<String>) -> HerdrSnapshot {
        let tab = |l: &str, panes| TabInfo {
            id: HerdrTabId(format!("w1:{l}")),
            label: l.into(),
            panes,
        };
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
                tabs: vec![
                    tab("legacy", vec![pane("p1", Some("claude"), p1_token)]),
                    tab("tools", vec![pane("p3", None, None)]),
                ],
            }],
        }
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
                id,
                Ok(ProcessInfo {
                    foreground_pid: Some(7),
                    foreground_argv: ["claude", "--model", "opus"].map(String::from).to_vec(),
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

    fn stored(fx: &Fx, v: serde_json::Value) -> StoredPlan {
        let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        fx.deps.plans.get(&id).unwrap().unwrap()
    }

    /// A plan of a kind that needs no live facts.
    fn plan(fx: &Fx, change: &str) -> Result<StoredPlan, String> {
        create_plan(&fx.deps, &CallerInfo::default(), words(change))
            .map(|v| stored(fx, v))
            .map_err(|e| e.message)
    }

    fn plan_facts(fx: &Fx, change: &str, f: &LiveFacts) -> Result<StoredPlan, String> {
        create_plan_facts(&fx.deps, &CallerInfo::default(), words(change), Some(f))
            .map(|v| stored(fx, v))
            .map_err(|e| e.message)
    }

    fn committed(row: &OpRow) {
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
    }

    /// An adopted teamspace over workspace `legacy`, and its adopt action.
    fn adopted_ts(fresh: bool) -> (Fx, ActionId) {
        let fx = if fresh {
            fx_fresh_with(kinds)
        } else {
            fx_with(kinds)
        };
        let v = create_plan_live(
            &fx.deps,
            &CallerInfo::default(),
            words("teamspace adopt legacy"),
            Some(&snap(None)),
        )
        .unwrap();
        let sp = stored(&fx, v);
        let row = apply(&fx, &sp, &[]);
        committed(&row);
        (fx, row.action.unwrap())
    }

    /// Adopt the agent tab `legacy` as a seat; returns its adopt action.
    fn adopt_seat(fx: &Fx) -> ActionId {
        let sp = plan_facts(fx, "seat adopt legacy", &facts(snap(None), &[("p1", "s1")])).unwrap();
        let row = apply(fx, &sp, &[("p1", "s1")]);
        committed(&row);
        row.action.unwrap()
    }

    fn view(fx: &Fx) -> CommitView<'_> {
        CommitView {
            store: &*fx.store,
            at: fx.store.head().unwrap(),
        }
    }

    fn seat(fx: &Fx, name: &str) -> SeatRecord {
        layout::all_seats(&view(fx))
            .unwrap()
            .into_iter()
            .map(|(_, s)| s)
            .find(|s| s.name == name)
            .unwrap()
    }

    fn clones(fx: &Fx) -> Vec<CloneRecord> {
        layout::all_clones(&view(fx))
            .unwrap()
            .into_iter()
            .map(|(_, c)| c)
            .collect()
    }

    fn teamspace(fx: &Fx) -> TeamspaceRecord {
        layout::list_teamspaces(&view(fx)).unwrap().remove(0).1
    }

    fn kinds_of(sp: &StoredPlan) -> Vec<&str> {
        sp.plan.effects.iter().map(|e| e.kind.as_str()).collect()
    }

    fn action(fx: &Fx, id: &ActionId) -> ActionRecord {
        layout::list_actions(&view(fx))
            .unwrap()
            .into_iter()
            .map(|(_, a)| a)
            .find(|a| a.id == *id)
            .unwrap()
    }

    #[test]
    fn seat_release_retires_as_released_and_drops_bindings() {
        let (fx, _) = adopted_ts(false);
        adopt_seat(&fx);
        let sp = plan(&fx, "seat release legacy").unwrap();
        assert_eq!(kinds_of(&sp), ["seat.release", "clone.release"]);
        let row = apply(&fx, &sp, &[]);
        committed(&row);
        let s = seat(&fx, "legacy");
        assert_eq!(s.lifecycle, Lifecycle::Retired);
        assert_eq!(
            s.retired.as_ref().unwrap().mechanism,
            RetireMechanism::Released
        );
        assert!(s.runtime.bound.is_none());
        let loc = layout::locate(&view(&fx), &s.id.to_any()).unwrap().unwrap();
        assert!(
            loc.folder.as_str().contains("archive/seats"),
            "{}",
            loc.folder.as_str()
        );
        let cs = clones(&fx);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        assert_eq!(c.lifecycle, CloneLifecycle::Retired);
        assert!(c.retired.as_ref().unwrap().released());
        assert!(c.runtime.bound.is_none());
        assert_eq!(c.threads_seat.as_deref(), Some("s1"));
        let a = action(&fx, row.action.as_ref().unwrap());
        assert_eq!(a.kind, ActionKind::Release);
        assert!(a.retired.contains(&s.id.to_any()));
    }

    #[test]
    fn seat_release_refuses_a_graph_created_seat() {
        let (fx, _) = adopted_ts(false);
        commit(&fx, "seat create made --teamspace legacy");
        let e = plan(&fx, "seat release made").unwrap_err();
        assert!(e.contains("use `seat retire`"), "{e}");
        // A graph-added keyed clone on an adopted seat.
        adopt_seat(&fx);
        commit(&fx, "clone add legacy");
        let e = plan(&fx, "seat release legacy").unwrap_err();
        assert!(e.contains("was added by graph"), "{e}");
    }

    #[test]
    fn teamspace_release_releases_its_adopted_seats() {
        let (fx, _) = adopted_ts(false);
        adopt_seat(&fx);
        let sp = plan(&fx, "teamspace release legacy").unwrap();
        assert_eq!(
            kinds_of(&sp),
            ["teamspace.release", "seat.release", "clone.release"]
        );
        let row = apply(&fx, &sp, &[]);
        committed(&row);
        let ts = teamspace(&fx);
        assert_eq!(ts.lifecycle, Lifecycle::Retired);
        assert!(ts.retired.as_ref().unwrap().released());
        assert!(ts.runtime.bound.is_none());
        let s = seat(&fx, "legacy");
        assert!(s.retired.as_ref().unwrap().released());
        assert!(s.runtime.bound.is_none());
        assert!(clones(&fx)[0].retired.as_ref().unwrap().released());
        assert_eq!(
            action(&fx, row.action.as_ref().unwrap()).kind,
            ActionKind::Release
        );
    }

    #[test]
    fn teamspace_release_refuses_with_a_graph_created_seat() {
        let (fx, _) = adopted_ts(false);
        adopt_seat(&fx);
        commit(&fx, "seat create made --teamspace legacy");
        let e = plan(&fx, "teamspace release legacy").unwrap_err();
        assert!(
            e.contains("made") && e.contains("release or retire these first"),
            "{e}"
        );
        let e = plan(&fx, "teamspace release nonesuch").unwrap_err();
        assert!(!e.is_empty());
    }

    #[test]
    fn teamspace_release_refuses_a_graph_created_teamspace() {
        let fx = fx_with(kinds);
        commit(&fx, "teamspace create mine");
        let e = plan(&fx, "teamspace release mine").unwrap_err();
        assert!(e.contains("use `teamspace retire`"), "{e}");
    }

    #[test]
    fn release_of_the_node_manager_is_refused() {
        let (fx, _) = adopted_ts(true);
        adopt_seat(&fx);
        let e = plan(&fx, "seat release legacy").unwrap_err();
        assert!(e.contains("cannot be released"), "{e}");
        let e = plan(&fx, "teamspace release legacy").unwrap_err();
        assert!(e.contains("cannot be released"), "{e}");
    }

    #[test]
    fn released_seat_cannot_be_resurrected() {
        let (fx, _) = adopted_ts(false);
        adopt_seat(&fx);
        let row = apply(&fx, &plan(&fx, "seat release legacy").unwrap(), &[]);
        committed(&row);
        let e = plan(&fx, "seat resurrect legacy").unwrap_err();
        assert!(
            e.contains("was released") && e.contains("`seat adopt`"),
            "{e}"
        );
        let row = apply(&fx, &plan(&fx, "teamspace release legacy").unwrap(), &[]);
        committed(&row);
        let e = plan(&fx, "teamspace resurrect legacy").unwrap_err();
        assert!(
            e.contains("was released") && e.contains("`teamspace adopt`"),
            "{e}"
        );
    }

    #[test]
    fn undo_of_a_seat_adoption_is_a_release() {
        let (fx, _) = adopted_ts(false);
        let adopt = adopt_seat(&fx);
        let sp = plan(&fx, &format!("undo {adopt}")).unwrap();
        assert!(
            kinds_of(&sp).contains(&"seat.release"),
            "{:?}",
            kinds_of(&sp)
        );
        assert!(
            !kinds_of(&sp).iter().any(|k| k.starts_with("runtime.")),
            "{:?}",
            kinds_of(&sp)
        );
        committed(&apply(&fx, &sp, &[]));
        let s = seat(&fx, "legacy");
        assert_eq!(
            s.retired.as_ref().unwrap().mechanism,
            RetireMechanism::Released
        );
        assert_eq!(
            clones(&fx)[0].retired.as_ref().unwrap().mechanism,
            RetireMechanism::Released
        );
        assert!(s.runtime.bound.is_none());
        assert!(!action(&fx, &adopt).undone_by.is_empty());
    }

    #[test]
    fn undo_of_a_teamspace_adoption_releases_it() {
        let (fx, adopt) = adopted_ts(false);
        let sp = plan(&fx, &format!("undo {adopt}")).unwrap();
        assert_eq!(kinds_of(&sp), ["teamspace.release"]);
        committed(&apply(&fx, &sp, &[]));
        let ts = teamspace(&fx);
        assert_eq!(ts.lifecycle, Lifecycle::Retired);
        assert_eq!(
            ts.retired.as_ref().unwrap().mechanism,
            RetireMechanism::Released
        );
        assert!(ts.runtime.bound.is_none());
        assert!(!action(&fx, &adopt).undone_by.is_empty());
    }

    #[test]
    fn readoption_after_release() {
        let (fx, _) = adopted_ts(false);
        adopt_seat(&fx);
        committed(&apply(&fx, &plan(&fx, "seat release legacy").unwrap(), &[]));
        let released = clones(&fx)[0].id.clone();
        let token = crate::model::launch::graph_token(&released.to_any());
        let f = facts(snap(Some(token)), &[("p1", "s1")]);
        let sp = plan_facts(&fx, "seat adopt legacy", &f).unwrap();
        committed(&apply(&fx, &sp, &[("p1", "s1")]));
        let live: Vec<_> = clones(&fx)
            .into_iter()
            .filter(|c| c.lifecycle == CloneLifecycle::Active)
            .collect();
        assert_eq!(live.len(), 1);
        assert_ne!(live[0].id, released);
    }
}
