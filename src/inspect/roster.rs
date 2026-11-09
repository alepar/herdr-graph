//! `roster` (spec §2.2): the seat/clone roster body and its one builder.
use crate::bootstrap::instructions::{RenderedInstructions, render_instructions};
use crate::inspect::inventory::{Inventory, RuntimeView, TranscriptRef};
use crate::inspect::output::{InstructionsStatus, InstructionsView, View};
use crate::inspect::scope::{Member, Scope};
use crate::model::clone::SeatFactState;
use crate::model::harness::Harness;
use crate::model::manager::Pointers;
use crate::model::{
    CloneId, CloneLifecycle, CommitId, Lifecycle, NsId, OpId, SeatId, TeamspaceId, Timestamp,
};
use crate::ports::store::{Store, StoreError};
use crate::store::GitStore;
use crate::store::tree::CommitView;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// The roster body: seats and clones with pane, occupant, threads state and instruction status.
///
/// Owns: the roster shape and the one builder ([`build`]) for both the daemon-free and the `--live`
/// modes, retired filtering (`--all`), and the manager and `reload_required` columns. It reserves every
/// later bead's slot up front (`instructions`, `last_change`, `last_change_scanned`, `live`,
/// `threads_live`, `unmanaged`, `live_unavailable`, `scan`) so those beads fill a provider and add no
/// top-level field.
///
/// Must not: enumerate records or join parents (that is `Inventory`), compute instruction status
/// (LoadedInstructions, hg-blw.12), walk history (HistoryWalker, through hg-blw.9) or resolve HEAD (it takes
/// the invocation's pinned `CommitId`).
///
/// Where new behavior goes: live-only data goes into the overlay slots (`clones[].live`, `threads_live`,
/// `unmanaged`, `live_unavailable`) that only the daemon's `roster.live` handler fills (hg-blw.6); the
/// daemon never builds a second roster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Roster {
    /// hg-blw.6: `{herdr?, threads?}` when part of the live read failed; always serialized (null here).
    pub live_unavailable: Option<serde_json::Value>,
    /// hg-blw.9: the walker's scan report; absent unless the last_change pass ran.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan: Option<serde_json::Value>,
    /// hg-blw.6: `{seat_refusals, seat_lost}`; absent outside --live.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub threads_live: Option<serde_json::Value>,
    /// hg-blw.6: `{workspaces, tabs, panes}`; absent without --unmanaged.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub unmanaged: Option<serde_json::Value>,
    pub teamspaces: Vec<RosterTeamspace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterTeamspace {
    pub id: TeamspaceId,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub seats: Vec<RosterSeat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterSeat {
    pub id: SeatId,
    pub name: String,
    pub lifecycle: Lifecycle,
    /// `None` for the node manager and when the manager chain is an integrity error.
    pub manager: Option<ManagerRef>,
    pub is_node_manager: bool,
    pub reload_required: bool,
    /// hg-blw.9 fills; null here.
    pub last_change: Option<LastChange>,
    /// hg-blw.9 fills; null here.
    pub last_change_scanned: Option<bool>,
    pub clones: Vec<RosterClone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagerRef {
    pub id: SeatId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastChange {
    pub at: Timestamp,
    pub op: Option<OpId>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterClone {
    pub id: CloneId,
    pub name: String,
    pub lifecycle: CloneLifecycle,
    /// Inventory's runtime binding, copied.
    pub runtime: RuntimeView,
    pub occupant: Option<OccupantView>,
    pub threads: Option<ThreadsView>,
    /// The shared type: unknown here, error on a failed render, filled by hg-blw.12.
    pub instructions: InstructionsView,
    /// hg-blw.6 retypes to `Option<CloneLive>`; always serialized (null).
    pub live: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OccupantView {
    pub harness: Harness,
    pub native_session: NsId,
    pub native_session_id: Option<String>,
    pub transcript: Option<TranscriptRef>,
    pub since: Timestamp,
}

/// `state` is the `SeatFactState` word (`resolved`, `unresolved`, `key_conflict`, `retired`), null without a
/// seat fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadsView {
    pub seat: String,
    pub state: Option<String>,
    pub read_at: Option<Timestamp>,
}

pub struct RosterQuery {
    pub scope: Scope,
    pub all: bool,
}

fn state_word(s: &SeatFactState) -> &'static str {
    match s {
        SeatFactState::Resolved { .. } => "resolved",
        SeatFactState::Unresolved => "unresolved",
        SeatFactState::KeyConflict { .. } => "key_conflict",
        SeatFactState::Retired { .. } => "retired",
    }
}

/// The one roster builder (daemon-free CLI and the daemon's roster.live). `at` is the invocation's pinned HEAD.
pub fn build(store: &GitStore, at: &CommitId, q: &RosterQuery) -> Result<Roster, StoreError> {
    let inv = Inventory::load(store, at, &q.scope)?;
    let view = CommitView {
        store: store as &dyn Store,
        at: at.clone(),
    };
    let pointers = Pointers::load(&view)?;
    let node_manager = pointers.live_node_manager();

    let mut teamspaces = Vec::new();
    for t in &inv.teamspaces {
        let mut seats = Vec::new();
        for s in &t.seats {
            if s.record.lifecycle == Lifecycle::Retired && !q.all {
                continue;
            }
            let manager = pointers
                .effective_manager(&s.record.id)
                .ok()
                .flatten()
                .map(|id| ManagerRef {
                    name: pointers.name(&id).unwrap_or(id.as_str()).to_owned(),
                    id,
                });
            // hg-blw.12 compares the loaded record against this render; keep it per seat.
            let rendered: Result<RenderedInstructions, StoreError> =
                render_instructions(&view, at, &s.record.id);
            let instructions = match &rendered {
                Ok(_) => InstructionsView::unknown(),
                Err(e) => InstructionsView::error(e.to_string()),
            };
            let clones = s
                .clones
                .iter()
                .filter(|c| q.all || c.record.lifecycle != CloneLifecycle::Retired)
                .map(|c| {
                    let r = &c.record;
                    let occupant = r.occupant.as_ref().map(|o| {
                        let cur = c.current();
                        OccupantView {
                            harness: o.harness,
                            native_session: o.native_session.clone(),
                            native_session_id: cur.map(|e| e.session.native_session_id.clone()),
                            transcript: cur.and_then(|e| e.transcript.clone()),
                            since: o.since,
                        }
                    });
                    let threads = r.threads_seat.as_ref().map(|seat| ThreadsView {
                        seat: seat.clone(),
                        state: r
                            .seat_fact
                            .as_ref()
                            .map(|f| state_word(&f.state).to_owned()),
                        read_at: r.seat_fact.as_ref().map(|f| f.read_at),
                    });
                    RosterClone {
                        id: r.id.clone(),
                        name: r.name.clone(),
                        lifecycle: r.lifecycle,
                        runtime: c.runtime.clone(),
                        occupant,
                        threads,
                        instructions: instructions.clone(),
                        live: None,
                    }
                })
                .collect();
            seats.push(RosterSeat {
                id: s.record.id.clone(),
                name: s.record.name.clone(),
                lifecycle: s.record.lifecycle,
                manager,
                is_node_manager: node_manager.as_ref() == Some(&s.record.id),
                reload_required: s.record.reload_required,
                last_change: None,
                last_change_scanned: None,
                clones,
            });
        }
        let own = q.scope.contains(Member::Teamspace(&t.record.id))
            && matches!(q.scope, Scope::Teamspace(_));
        if !seats.is_empty() || own {
            teamspaces.push(RosterTeamspace {
                id: t.record.id.clone(),
                name: t.record.name.clone(),
                lifecycle: t.record.lifecycle,
                seats,
            });
        }
    }
    Ok(Roster {
        live_unavailable: None,
        scan: None,
        threads_live: None,
        unmanaged: None,
        teamspaces,
    })
}

fn word<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => "?".into(),
    }
}

fn instructions_text(i: &InstructionsView) -> String {
    let mut s = match i.status {
        InstructionsStatus::Current => match &i.loaded_rev {
            Some(r) => format!("current @{}", &r[..r.len().min(12)]),
            None => "current".into(),
        },
        InstructionsStatus::Stale => format!("stale ({} parts)", i.stale_parts.len()),
        InstructionsStatus::Unknown => "unknown".into(),
        InstructionsStatus::Error => {
            format!("error: {}", i.message.as_deref().unwrap_or("unknown error"))
        }
    };
    if i.truncated {
        s.push_str(" truncated");
    }
    s
}

fn clone_line(c: &RosterClone) -> String {
    let rt = &c.runtime;
    let pane = if rt.workspace.is_none() && rt.tab.is_none() && rt.pane.is_none() {
        "-".to_owned()
    } else {
        let part = |v: Option<String>| v.unwrap_or_else(|| "?".into());
        format!(
            "{}:{}:{}",
            part(rt.workspace.as_ref().map(|w| w.to_string())),
            part(rt.tab.as_ref().map(|t| t.to_string())),
            part(rt.pane.as_ref().map(|p| p.to_string()))
        )
    };
    let occupant = match &c.occupant {
        None => "-".to_owned(),
        Some(o) => {
            let native = o.native_session_id.as_deref().unwrap_or("-");
            let transcript = o
                .transcript
                .as_ref()
                .map_or("unresolved".to_owned(), |t| t.path.display().to_string());
            format!(
                "{} {} ({native}, {transcript}) since {}",
                word(&o.harness),
                o.native_session,
                o.since.format("%H:%MZ")
            )
        }
    };
    let (tseat, tstate) = match &c.threads {
        None => ("-".to_owned(), "-".to_owned()),
        Some(t) => (
            t.seat.clone(),
            t.state.clone().unwrap_or_else(|| "-".into()),
        ),
    };
    format!(
        "{} ({})  {}  {}  {}  threads {}  {}  instructions: {}",
        c.name,
        c.id,
        word(&rt.availability),
        pane,
        occupant,
        tseat,
        tstate,
        instructions_text(&c.instructions)
    )
}

impl View for Roster {
    fn render_text(&self, out: &mut String) {
        for t in &self.teamspaces {
            let _ = writeln!(out, "{} ({}) {}", t.name, t.id, word(&t.lifecycle));
            for s in &t.seats {
                let mut line = format!("  {} ({}) {}", s.name, s.id, word(&s.lifecycle));
                if let Some(m) = &s.manager {
                    let _ = write!(line, "  manager: {}", m.name);
                }
                if s.is_node_manager {
                    line.push_str("  node-manager");
                }
                if s.reload_required {
                    line.push_str("  reload_required");
                }
                let _ = writeln!(out, "{line}");
                for c in &s.clones {
                    let _ = writeln!(out, "    {}", clone_line(c));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::{Fx, session, ts};
    use crate::inspect::output::Envelope;
    use crate::inspect::pin_head;
    use crate::model::clone::SeatFact;
    use crate::model::seat::TemplateRef;
    use crate::model::{
        Availability, Binding, HerdrPaneId, HerdrTabId, HerdrWorkspaceId, Incarnation, MemberId,
        Runtime, TemplateId,
    };
    use serde_json::json;

    fn roster(fx: &Fx, scope: Scope, all: bool) -> Roster {
        let at = pin_head(&fx.store).unwrap();
        build(&fx.store, &at, &RosterQuery { scope, all }).unwrap()
    }

    struct World {
        fx: Fx,
        t: TeamspaceId,
        active: SeatId,
        dormant: SeatId,
        retired: SeatId,
        present: CloneId,
        absent: CloneId,
    }

    fn world() -> World {
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let active = fx.seat(&t, "foreman", Lifecycle::Active);
        let dormant = fx.seat(&t, "researcher", Lifecycle::Dormant);
        let retired = fx.archived_seat(&t, "old");

        let tr_session = session(
            Harness::Claude,
            "3f2a",
            "2026-11-01T10:42:00Z",
            Some("/t/3f2a.jsonl"),
            None,
        );
        let occ = tr_session.id.clone();
        let present = fx.clone_rec(
            &active,
            "main",
            CloneLifecycle::Active,
            vec![tr_session],
            Some(occ),
        );
        fx.set_runtime(
            &present,
            Runtime {
                availability: Availability::Present,
                bound: Some(Binding {
                    token: None,
                    workspace_id: Some(HerdrWorkspaceId("w3".into())),
                    tab_id: Some(HerdrTabId("t2".into())),
                    pane_id: Some(HerdrPaneId("p7".into())),
                    terminal_id: None,
                    incarnation: Incarnation::default(),
                }),
                observed_at: Some(ts("2026-11-03T00:00:00Z")),
            },
        );
        {
            let c = fx.clone_mut(&present);
            c.threads_seat = Some("sn1".into());
            c.seat_fact = Some(SeatFact {
                seat: "sn1".into(),
                state: SeatFactState::Resolved {
                    pane: HerdrPaneId("p7".into()),
                },
                read_at: ts("2026-11-03T00:00:00Z"),
            });
        }
        let absent = fx.clone_rec(&active, "alt", CloneLifecycle::Active, vec![], None);
        fx.clone_rec(&retired, "gone", CloneLifecycle::Retired, vec![], None);
        fx.commit("c");
        World {
            fx,
            t,
            active,
            dormant,
            retired,
            present,
            absent,
        }
    }

    #[test]
    fn shapes_and_columns() {
        let w = world();
        let r = roster(&w.fx, Scope::Instance, false);
        assert_eq!(r.teamspaces.len(), 1);
        let seats = &r.teamspaces[0].seats;
        let names: Vec<_> = seats.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foreman") && names.contains(&"researcher"));
        assert!(
            !names.contains(&"old"),
            "retired seat needs --all: {names:?}"
        );
        let foreman = seats.iter().find(|s| s.id == w.active).unwrap();
        assert_eq!(foreman.lifecycle, Lifecycle::Active);
        assert_eq!(
            seats.iter().find(|s| s.id == w.dormant).unwrap().lifecycle,
            Lifecycle::Dormant
        );
        let main = foreman.clones.iter().find(|c| c.id == w.present).unwrap();
        assert_eq!(main.runtime.availability, Availability::Present);
        assert_eq!(main.runtime.pane, Some(HerdrPaneId("p7".into())));
        assert_eq!(main.runtime.tab, Some(HerdrTabId("t2".into())));
        assert_eq!(main.runtime.workspace, Some(HerdrWorkspaceId("w3".into())));
        assert_eq!(main.runtime.observed_at, Some(ts("2026-11-03T00:00:00Z")));
        let occ = main.occupant.as_ref().unwrap();
        assert_eq!(occ.harness, Harness::Claude);
        assert_eq!(occ.native_session_id.as_deref(), Some("3f2a"));
        assert_eq!(
            occ.transcript.as_ref().unwrap().path.to_str(),
            Some("/t/3f2a.jsonl")
        );
        assert_eq!(occ.since, ts("2026-11-01T00:00:00Z"));
        let th = main.threads.as_ref().unwrap();
        assert_eq!(th.seat, "sn1");
        assert_eq!(th.state.as_deref(), Some("resolved"));
        assert_eq!(th.read_at, Some(ts("2026-11-03T00:00:00Z")));
        let alt = foreman.clones.iter().find(|c| c.id == w.absent).unwrap();
        assert_eq!(alt.runtime.availability, Availability::Unknown);
        assert!(alt.occupant.is_none() && alt.threads.is_none());

        let all = roster(&w.fx, Scope::Instance, true);
        let seats = &all.teamspaces[0].seats;
        let old = seats.iter().find(|s| s.id == w.retired).unwrap();
        assert_eq!(old.lifecycle, Lifecycle::Retired);
        assert_eq!(old.clones.len(), 1);
        // a retired clone of a kept seat is also hidden by default
        let mut fx = Fx::new();
        let t = fx.teamspace("p", Lifecycle::Active);
        let s = fx.seat(&t, "s", Lifecycle::Active);
        fx.clone_rec(&s, "live", CloneLifecycle::Active, vec![], None);
        fx.clone_rec(&s, "dead", CloneLifecycle::Retired, vec![], None);
        fx.commit("c");
        let n = |all| {
            roster(&fx, Scope::Instance, all).teamspaces[0].seats[0]
                .clones
                .len()
        };
        assert_eq!((n(false), n(true)), (1, 2));
    }

    #[test]
    fn scopes_filter() {
        let mut w = world();
        let t2 = w.fx.teamspace("other", Lifecycle::Active);
        let s2 = w.fx.seat(&t2, "elsewhere", Lifecycle::Active);
        w.fx.teamspace("empty", Lifecycle::Active);
        w.fx.commit("more");
        assert!(roster(&w.fx, Scope::Global, true).teamspaces.is_empty());
        let only = roster(&w.fx, Scope::Teamspace(w.t.clone()), false);
        assert_eq!(only.teamspaces.len(), 1);
        assert_eq!(only.teamspaces[0].id, w.t);
        let seat = roster(&w.fx, Scope::Seat(s2.clone()), false);
        assert_eq!(seat.teamspaces.len(), 1);
        assert_eq!(seat.teamspaces[0].id, t2);
        assert_eq!(seat.teamspaces[0].seats.len(), 1);
        assert_eq!(seat.teamspaces[0].seats[0].id, s2);
        // a seatless teamspace named by the scope still prints
        let empty = w.fx.teamspace("alone", Lifecycle::Active);
        w.fx.commit("alone");
        let r = roster(&w.fx, Scope::Teamspace(empty.clone()), false);
        assert_eq!(r.teamspaces.len(), 1);
        assert!(r.teamspaces[0].seats.is_empty());
        // ... but is absent from the whole-instance roster
        let all = roster(&w.fx, Scope::Instance, false);
        assert!(all.teamspaces.iter().all(|t| t.id != empty));
    }

    #[test]
    fn manager_and_flags() {
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let nm = fx.seat(&t, "node-manager", Lifecycle::Active);
        let worker = fx.seat(&t, "worker", Lifecycle::Active);
        let reloader = fx.seat(&t, "reloader", Lifecycle::Dormant);
        fx.seat_mut(&worker).manager = Some(nm.clone());
        fx.seat_mut(&reloader).reload_required = true;
        fx.set_node_manager(&nm);
        fx.commit("c");
        let r = roster(&fx, Scope::Instance, false);
        let seat = |id: &SeatId| r.teamspaces[0].seats.iter().find(|s| &s.id == id).unwrap();
        assert_eq!(
            seat(&worker).manager,
            Some(ManagerRef {
                id: nm.clone(),
                name: "node-manager".into()
            })
        );
        assert!(!seat(&worker).is_node_manager);
        assert!(seat(&nm).is_node_manager);
        assert_eq!(seat(&nm).manager, None);
        assert!(seat(&reloader).reload_required);
        assert!(!seat(&worker).reload_required);
    }

    #[test]
    fn instructions_unknown_and_error_row() {
        let mut w = world();
        let bad = w.fx.seat(&w.t, "broken", Lifecycle::Active);
        w.fx.clone_rec(&bad, "main", CloneLifecycle::Active, vec![], None);
        w.fx.seat_mut(&bad).template_ref = Some(TemplateRef {
            template: TemplateId::new(),
            member: MemberId::new(),
        });
        w.fx.commit("broken");
        let r = roster(&w.fx, Scope::Instance, false);
        let mut seen_ok = 0;
        for s in &r.teamspaces[0].seats {
            for c in &s.clones {
                let v = serde_json::to_value(&c.instructions).unwrap();
                if s.id == bad {
                    assert_eq!(v["status"], json!("error"));
                    assert!(!v["message"].as_str().unwrap().is_empty());
                } else {
                    seen_ok += 1;
                    assert_eq!(v["status"], json!("unknown"));
                    assert_eq!(v["parts"], json!([]));
                    assert_eq!(v["omitted"], json!([]));
                }
            }
        }
        assert_eq!(seen_ok, 2, "both foreman clones render normally");
    }

    #[test]
    fn reserved_slots_serialize() {
        let w = world();
        let at = pin_head(&w.fx.store).unwrap();
        let r = roster(&w.fx, Scope::Instance, false);
        let v = serde_json::to_value(Envelope::new(&at, r).with_live(false)).unwrap();
        assert_eq!(v["schema"], json!(1));
        assert_eq!(v["head"], json!(at.0));
        assert_eq!(v["live"], json!(false));
        assert_eq!(v["live_unavailable"], json!(null));
        let obj = v.as_object().unwrap();
        for k in ["scan", "threads_live", "unmanaged"] {
            assert!(!obj.contains_key(k), "{k}");
        }
        let mut clones = 0;
        for s in v["teamspaces"][0]["seats"].as_array().unwrap() {
            assert_eq!(s["last_change"], json!(null));
            assert_eq!(s["last_change_scanned"], json!(null));
            assert!(s["manager"].is_null());
            assert!(s["is_node_manager"].is_boolean() && s["reload_required"].is_boolean());
            for c in s["clones"].as_array().unwrap() {
                clones += 1;
                assert_eq!(c["live"], json!(null));
                for k in ["availability", "pane", "tab", "workspace", "observed_at"] {
                    assert!(c["runtime"].as_object().unwrap().contains_key(k), "{k}");
                }
                assert!(c["instructions"]["parts"].is_array());
            }
        }
        assert_eq!(clones, 2);
        let occ = v["teamspaces"][0]["seats"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["clones"].as_array().unwrap())
            .map(|c| &c["occupant"])
            .find(|o| !o.is_null())
            .expect("one clone has an occupant");
        assert_eq!(occ["native_session_id"], json!("3f2a"));
        assert_eq!(occ["transcript"]["path"], json!("/t/3f2a.jsonl"));
    }

    #[test]
    fn round_trips() {
        let w = world();
        let r = roster(&w.fx, Scope::Instance, true);
        let back: Roster = serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn render_text_pins_one_seat_block() {
        let w = world();
        let r = roster(&w.fx, Scope::Seat(w.active.clone()), false);
        let mut out = String::new();
        r.render_text(&mut out);
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines.len(), 4, "{out}");
        assert!(lines[0].starts_with("proj (ts_") && lines[0].ends_with(") active"));
        assert!(lines[1].starts_with("  foreman (st_") && lines[1].ends_with(") active"));
        let main = lines.iter().find(|l| l.contains("main (cl_")).unwrap();
        assert!(
            main.starts_with("    main (cl_")
                && main.contains(")  present  w3:t2:p7  claude ns_")
                && main.contains("(3f2a, /t/3f2a.jsonl) since 00:00Z  threads sn1  resolved  instructions: unknown"),
            "{main}"
        );
        let alt = lines.iter().find(|l| l.contains("alt (cl_")).unwrap();
        assert!(
            alt.contains(")  unknown  -  -  threads -  -  instructions: unknown"),
            "{alt}"
        );
    }

    #[test]
    fn instructions_text_forms() {
        let mut v = InstructionsView::unknown();
        assert_eq!(instructions_text(&v), "unknown");
        v.status = InstructionsStatus::Current;
        v.loaded_rev = Some("1e01e11c4a2b9999".into());
        assert_eq!(instructions_text(&v), "current @1e01e11c4a2b");
        v.truncated = true;
        assert_eq!(instructions_text(&v), "current @1e01e11c4a2b truncated");
        assert_eq!(
            instructions_text(&InstructionsView::error("boom")),
            "error: boom"
        );
    }
}
