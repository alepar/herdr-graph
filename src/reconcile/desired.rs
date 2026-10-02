//! Desired runtime: what Herdr should look like for one committed revision (spec §4.4).
use crate::model::clone::CloneRecord;
use crate::model::common::{Availability, Binding, CloneLifecycle, Lifecycle, Occupant};
use crate::model::effective::{EffectiveSeatConfig, resolve_in};
use crate::model::harness::{Harness, profile};
use crate::model::launch::{launch_env, resolve_cwd};
use crate::model::native_session::NativeSession;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, CloneId, OpId, SeatId, TeamspaceId};
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::tree::TreeRead;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredWorkspace {
    pub ts: TeamspaceId,
    pub name: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub bound: Option<Binding>,
    pub availability: Availability,
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredTab {
    pub seat: SeatId,
    pub ts: TeamspaceId,
    pub name: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub moved_out: bool,
    pub bound: Option<Binding>,
    pub availability: Availability,
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredPane {
    pub clone: CloneId,
    pub seat: SeatId,
    pub ts: TeamspaceId,
    pub name: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub harness: Harness,
    pub model: Option<String>,
    pub args: Vec<String>,
    /// Latest ended native session of the effective harness, when that harness supports resume.
    pub resume: Option<String>,
    pub occupant: Option<Occupant>,
    /// Native id of the occupant's session (from `clone.sessions`), for session replacement.
    pub occupant_native_id: Option<String>,
    pub bound: Option<Binding>,
    pub availability: Availability,
    pub rev: u64,
}

/// A bound object whose availability is `unknown` gets no create effects (spec §4.3.2).
pub fn is_unknown(bound: &Option<Binding>, availability: Availability) -> bool {
    bound.is_some() && availability == Availability::Unknown
}

impl DesiredWorkspace {
    pub fn is_unknown(&self) -> bool {
        is_unknown(&self.bound, self.availability)
    }
}
impl DesiredTab {
    pub fn is_unknown(&self) -> bool {
        is_unknown(&self.bound, self.availability)
    }
}
impl DesiredPane {
    pub fn is_unknown(&self) -> bool {
        is_unknown(&self.bound, self.availability)
    }
    /// The `{harness, model, args}` shape a replacement compares against `launched:<clone>`.
    pub fn launch_shape(&self) -> serde_json::Value {
        serde_json::json!({ "harness": self.harness, "model": self.model, "args": self.args })
    }
}

/// Everything the planner and the executors need from one committed revision.
#[derive(Debug, Clone, Default)]
pub struct DesiredRuntime {
    pub instance: PathBuf,
    /// Every record at the revision, retired and archived ones included.
    pub teamspaces: BTreeMap<TeamspaceId, TeamspaceRecord>,
    pub seats: BTreeMap<SeatId, SeatRecord>,
    pub clones: BTreeMap<CloneId, CloneRecord>,
    pub workspaces: Vec<DesiredWorkspace>,
    pub tabs: Vec<DesiredTab>,
    pub panes: Vec<DesiredPane>,
}

fn latest_ended(sessions: &[NativeSession], harness: Harness) -> Option<&NativeSession> {
    sessions.iter().filter(|s| s.harness == harness && s.ended.is_some()).max_by_key(|s| s.ended)
}

impl DesiredRuntime {
    pub fn load(tree: &dyn TreeRead, instance: &Path) -> Result<Self, StoreError> {
        let mut d = DesiredRuntime { instance: instance.to_path_buf(), ..Default::default() };
        for (_, t) in layout::list_teamspaces(tree)? {
            d.teamspaces.insert(t.id.clone(), t);
        }
        for (_, s) in layout::all_seats(tree)? {
            d.seats.insert(s.id.clone(), s);
        }
        for (_, c) in layout::all_clones(tree)? {
            d.clones.insert(c.id.clone(), c);
        }
        for ts in d.teamspaces.values().filter(|t| t.lifecycle == Lifecycle::Active) {
            d.workspaces.push(DesiredWorkspace {
                ts: ts.id.clone(),
                name: ts.name.clone(),
                cwd: ts.project_repo.clone().unwrap_or_else(|| instance.to_path_buf()),
                env: launch_env(instance, None, None),
                bound: ts.runtime.bound.clone(),
                availability: ts.runtime.availability,
                rev: ts.rev,
            });
        }
        let seats: Vec<SeatRecord> = d.seats.values().cloned().collect();
        for seat in seats {
            let Some(ts) = d.teamspaces.get(&seat.teamspace).cloned() else { continue };
            if ts.lifecycle != Lifecycle::Active || seat.lifecycle != Lifecycle::Active {
                continue;
            }
            let cfg: EffectiveSeatConfig = resolve_in(tree, &seat)?;
            let (cwd, _) =
                resolve_cwd(cfg.cwd.as_deref(), ts.project_repo.as_deref(), instance, &seat.id);
            d.tabs.push(DesiredTab {
                seat: seat.id.clone(),
                ts: ts.id.clone(),
                name: seat.name.clone(),
                cwd: cwd.clone(),
                env: launch_env(instance, Some(&seat.id), None),
                moved_out: seat.moved_out,
                bound: seat.runtime.bound.clone(),
                availability: seat.runtime.availability,
                rev: seat.rev,
            });
            let clones: Vec<CloneRecord> =
                d.clones.values().filter(|c| c.seat == seat.id && c.lifecycle == CloneLifecycle::Active).cloned().collect();
            for c in clones {
                let resume = profile(cfg.harness)
                    .supports_resume()
                    .then(|| latest_ended(&c.sessions, cfg.harness).map(|s| s.native_session_id.clone()))
                    .flatten();
                let occupant_native_id = c.occupant.as_ref().and_then(|o| {
                    c.sessions.iter().find(|s| s.id == o.native_session).map(|s| s.native_session_id.clone())
                });
                d.panes.push(DesiredPane {
                    clone: c.id.clone(),
                    seat: seat.id.clone(),
                    ts: ts.id.clone(),
                    name: c.name.clone(),
                    cwd: cwd.clone(),
                    env: launch_env(instance, Some(&seat.id), Some(&c.id)),
                    harness: cfg.harness,
                    model: cfg.model.clone(),
                    args: cfg.args.clone(),
                    resume,
                    occupant: c.occupant.clone(),
                    occupant_native_id,
                    bound: c.runtime.bound.clone(),
                    availability: c.runtime.availability,
                    rev: c.rev,
                });
            }
        }
        Ok(d)
    }

    pub fn workspace(&self, ts: &TeamspaceId) -> Option<&DesiredWorkspace> {
        self.workspaces.iter().find(|w| &w.ts == ts)
    }
    pub fn tab(&self, seat: &SeatId) -> Option<&DesiredTab> {
        self.tabs.iter().find(|t| &t.seat == seat)
    }
    pub fn pane(&self, clone: &CloneId) -> Option<&DesiredPane> {
        self.panes.iter().find(|p| &p.clone == clone)
    }
    pub fn panes_of<'a>(&'a self, seat: &SeatId) -> impl Iterator<Item = &'a DesiredPane> + use<'a> {
        let seat = seat.clone();
        self.panes.iter().filter(move |p| p.seat == seat)
    }

    /// Object revision (any lifecycle), None when the object is not a teamspace, seat or clone here.
    pub fn rev_of(&self, object: &AnyId) -> Option<u64> {
        if let Ok(t) = TeamspaceId::parse(object.as_str()) {
            return self.teamspaces.get(&t).map(|r| r.rev);
        }
        if let Ok(s) = SeatId::parse(object.as_str()) {
            return self.seats.get(&s).map(|r| r.rev);
        }
        if let Ok(c) = CloneId::parse(object.as_str()) {
            return self.clones.get(&c).map(|r| r.rev);
        }
        None
    }

    /// Attribution op (spec §4.4): seat -> its activation op; clone -> its seat's; teamspace -> the newest
    /// among its seats; otherwise the nil op.
    pub fn op_for(&self, object: &AnyId) -> OpId {
        let nil = OpId::from_ulid(ulid::Ulid::nil());
        if let Ok(s) = SeatId::parse(object.as_str()) {
            return self.seats.get(&s).and_then(|r| r.activation.last_op.clone()).unwrap_or(nil);
        }
        if let Ok(c) = CloneId::parse(object.as_str()) {
            let seat = self.clones.get(&c).and_then(|c| self.seats.get(&c.seat));
            return seat.and_then(|r| r.activation.last_op.clone()).unwrap_or(nil);
        }
        if let Ok(t) = TeamspaceId::parse(object.as_str()) {
            return self
                .seats
                .values()
                .filter(|s| s.teamspace == t)
                .filter_map(|s| s.activation.last_op.clone())
                .max()
                .unwrap_or(nil);
        }
        nil
    }
}

/// `reconcile::effect_op`: the attribution op of `object` at the revision `tree` shows.
pub fn effect_op(tree: &dyn TreeRead, object: &AnyId) -> Result<OpId, StoreError> {
    let nil = OpId::from_ulid(ulid::Ulid::nil());
    let Some(loc) = layout::locate(tree, object)? else { return Ok(nil) };
    let seat_op = |id: &SeatId| -> Result<Option<OpId>, StoreError> {
        let Some(l) = layout::locate(tree, &id.to_any())? else { return Ok(None) };
        let rec: Option<SeatRecord> = crate::store::record::read_toml(tree, &l.record_path)?;
        Ok(rec.and_then(|r| r.activation.last_op))
    };
    match object.kind() {
        crate::model::IdKind::Seat => {
            let id = SeatId::parse(object.as_str()).expect("kind checked");
            Ok(seat_op(&id)?.unwrap_or(nil))
        }
        crate::model::IdKind::Clone => {
            let rec: Option<CloneRecord> = crate::store::record::read_toml(tree, &loc.record_path)?;
            match rec {
                Some(c) => Ok(seat_op(&c.seat)?.unwrap_or(nil)),
                None => Ok(nil),
            }
        }
        crate::model::IdKind::Teamspace => {
            let id = TeamspaceId::parse(object.as_str()).expect("kind checked");
            let mut best: Option<OpId> = None;
            for (_, s) in layout::all_seats(tree)? {
                if s.teamspace == id
                    && let Some(op) = s.activation.last_op
                {
                    best = Some(best.map_or(op.clone(), |b| b.max(op)));
                }
            }
            Ok(best.unwrap_or(nil))
        }
        _ => Ok(nil),
    }
}
