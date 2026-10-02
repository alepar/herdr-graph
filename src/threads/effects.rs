//! Threads effect family for the reconciler (spec §7.1-7.3): channel creation, required invitations,
//! participation, notifications, membership tracking and occupancy-keyed cleanup.
//!
//! `ThreadsSource` turns committed state into *wants* (a pure function of the graph and a few journal
//! facts); each want becomes a deterministic effect row whose payload lives in the journal's `meta` table
//! keyed by effect id. `ThreadsExecutor` performs them through `ThreadsPort`, records what threads actually
//! reported as bookkeeping mutations, and never records acceptance threads did not report.
use super::mapping::PaneSeatMap;
use crate::journal::Journal;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::{CloneRecord, Invitation, InvitationState, InviteConstraint, ThreadsLink};
use crate::model::common::{Channel, CloneLifecycle, CommitId, Lifecycle};
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, CloneId, EffectId, IdKind, OpId, PlanId, SeatId, TeamspaceId, Timestamp};
use crate::plan::store::PlanStore;
use crate::ports::store::StoreError;
use crate::ports::threads::*;
use crate::reconcile::{DiffCx, EffectExecutor, EffectSource, ExecCx, ExecOutcome, PlannedEffect};
use crate::store::layout;
use crate::store::tree::TreeRead;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// `EffectKind::Custom` name of the membership poll.
pub const POLL_KIND: &str = "threads.membership";
const PAYLOAD_PREFIX: &str = "threads:payload:";
const TOPIC_PREFIX: &str = "threads:topic:";
const RELOAD_PREFIX: &str = "threads:reload:";
/// Plan-intent notifications are only raised for ops that committed this recently.
const INTENT_WINDOW: chrono::Duration = chrono::Duration::hours(24);
/// Committed ops scanned for plan-intent notifications.
const INTENT_SCAN: usize = 64;

// ---------------------------------------------------------------------------------------------
// graph snapshot
// ---------------------------------------------------------------------------------------------

/// Every teamspace, seat and clone record of one committed revision.
#[derive(Default)]
pub(crate) struct Graph {
    pub teamspaces: BTreeMap<TeamspaceId, TeamspaceRecord>,
    pub seats: BTreeMap<SeatId, SeatRecord>,
    pub clones: BTreeMap<CloneId, CloneRecord>,
}

impl Graph {
    pub fn load(tree: &dyn TreeRead) -> Result<Self, StoreError> {
        let mut g = Graph::default();
        for (_, t) in layout::list_teamspaces(tree)? {
            g.teamspaces.insert(t.id.clone(), t);
        }
        for (_, s) in layout::all_seats(tree)? {
            g.seats.insert(s.id.clone(), s);
        }
        for (_, c) in layout::all_clones(tree)? {
            g.clones.insert(c.id.clone(), c);
        }
        Ok(g)
    }

    fn ts_active(&self, id: &TeamspaceId) -> bool {
        self.teamspaces.get(id).is_some_and(|t| t.lifecycle == Lifecycle::Active)
    }

    pub fn seat_active(&self, s: &SeatRecord) -> bool {
        s.lifecycle == Lifecycle::Active && self.ts_active(&s.teamspace)
    }

    pub fn clone_active(&self, c: &CloneRecord) -> bool {
        c.lifecycle == CloneLifecycle::Active && self.seats.get(&c.seat).is_some_and(|s| self.seat_active(s))
    }

    /// Native session record of the clone's occupant, when the clone is active and occupied.
    fn occupant_of(&self, c: &CloneRecord) -> Option<String> {
        if !self.clone_active(c) {
            return None;
        }
        c.occupant.as_ref().map(|o| o.native_session.to_string())
    }

    fn rev_of(&self, object: &AnyId) -> Option<u64> {
        match object.kind() {
            IdKind::Teamspace => TeamspaceId::parse(object.as_str()).ok().and_then(|i| self.teamspaces.get(&i)).map(|r| r.rev),
            IdKind::Seat => SeatId::parse(object.as_str()).ok().and_then(|i| self.seats.get(&i)).map(|r| r.rev),
            IdKind::Clone => CloneId::parse(object.as_str()).ok().and_then(|i| self.clones.get(&i)).map(|r| r.rev),
            _ => None,
        }
    }

    /// Attribution op of an object (spec §4.4): seat -> its activation op, clone -> its seat's, teamspace ->
    /// the newest among its seats, otherwise the nil op.
    fn op_for(&self, object: &AnyId) -> OpId {
        let nil = OpId::from_ulid(ulid::Ulid::nil());
        match object.kind() {
            IdKind::Seat => SeatId::parse(object.as_str())
                .ok()
                .and_then(|i| self.seats.get(&i))
                .and_then(|s| s.activation.last_op.clone())
                .unwrap_or(nil),
            IdKind::Clone => CloneId::parse(object.as_str())
                .ok()
                .and_then(|i| self.clones.get(&i))
                .and_then(|c| self.seats.get(&c.seat))
                .and_then(|s| s.activation.last_op.clone())
                .unwrap_or(nil),
            IdKind::Teamspace => {
                let Ok(t) = TeamspaceId::parse(object.as_str()) else { return nil };
                self.seats.values().filter(|s| s.teamspace == t).filter_map(|s| s.activation.last_op.clone()).max().unwrap_or(nil)
            }
            _ => nil,
        }
    }

    /// A teamspace or seat channel: scope, record channel, desired topic and whether the owner is active.
    fn channel(&self, object: &AnyId) -> Option<(ChannelScope, &Channel, String, bool)> {
        match object.kind() {
            IdKind::Teamspace => {
                let t = self.teamspaces.get(&TeamspaceId::parse(object.as_str()).ok()?)?;
                Some((ChannelScope::Teamspace, &t.channel, t.name.clone(), t.lifecycle == Lifecycle::Active))
            }
            IdKind::Seat => {
                let s = self.seats.get(&SeatId::parse(object.as_str()).ok()?)?;
                let ts = self.teamspaces.get(&s.teamspace)?;
                Some((ChannelScope::Seat, &s.channel, format!("{}/{}", ts.name, s.name), self.seat_active(s)))
            }
            _ => None,
        }
    }

    /// The channel a notification about `object` goes to: the object's own for seats and teamspaces, the
    /// clone's seat's for clones.
    fn notify_channel_owner(&self, object: &AnyId) -> Option<AnyId> {
        match object.kind() {
            IdKind::Clone => self.clones.get(&CloneId::parse(object.as_str()).ok()?).map(|c| c.seat.to_any()),
            _ => Some(object.clone()),
        }
    }
}

/// Shares the last loaded graph between the source and the executor (one load per committed revision).
#[derive(Default)]
pub(crate) struct GraphCache(Mutex<Option<(CommitId, Arc<Graph>)>>);

impl GraphCache {
    pub fn get(&self, tree: &dyn TreeRead, head: &CommitId) -> Result<Arc<Graph>, StoreError> {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, g)) = slot.as_ref()
            && at == head
        {
            return Ok(g.clone());
        }
        let g = Arc::new(Graph::load(tree)?);
        *slot = Some((head.clone(), g.clone()));
        Ok(g)
    }
}

// ---------------------------------------------------------------------------------------------
// wants
// ---------------------------------------------------------------------------------------------

/// What an effect row means beyond its kind and object; stored as JSON under `threads:payload:<effect id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum Payload {
    None,
    Invite { thread: String, constraint: InviteConstraint },
    Release { thread: String, constraint: InviteConstraint },
    /// `leaving`: an ordinary participation being left; polled until threads reports the member gone.
    Poll { thread: String, constraint: InviteConstraint, leaving: bool },
    Notify { purpose: String, severity: Severity, body: String, key: String },
}

struct Want {
    kind: EffectKind,
    object: AnyId,
    op: OpId,
    /// Goes into the effect identity (`ef_ = hash(op, object, kind, rev)`).
    identity_rev: u64,
    /// The object's committed revision (what the reconciler fences against).
    fencing_rev: u64,
    payload: Payload,
    /// Wants that must finish first.
    after: Vec<After>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum After {
    /// The `EnsureThread` of this teamspace or seat.
    Ensure(AnyId),
    /// The `ReleaseRequirement` of this clone's invitation to this thread: the old occupant's episode ends
    /// before the new occupant's begins.
    Release(AnyId, String),
}

/// The thread id graph derives from an object id (also what the port derives from `ensure:<object>`).
pub(crate) fn derived_thread(object: &AnyId) -> String {
    super::fake::thread_for_ensure_key(&OpKey(format!("ensure:{object}"))).0
}

impl Want {
    /// Several wants of one kind can share an object and revision (one invite per thread), so a payload
    /// is folded into the revision half of the identity.
    fn identity_rev(&self) -> u64 {
        if self.payload == Payload::None {
            return self.identity_rev;
        }
        let mut h = Sha256::new();
        h.update(self.identity_rev.to_be_bytes());
        h.update(serde_json::to_vec(&self.payload).unwrap_or_default());
        u64::from_be_bytes(h.finalize()[..8].try_into().expect("8 bytes"))
    }
}

fn topic_generation(topic: &str) -> u64 {
    let d = Sha256::digest(topic.as_bytes());
    u64::from_be_bytes(d[..8].try_into().expect("8 bytes"))
}

fn constraint_name(c: InviteConstraint) -> &'static str {
    match c {
        InviteConstraint::Required => "required",
        InviteConstraint::Ordinary => "ordinary",
    }
}

fn want(g: &Graph, kind: EffectKind, object: AnyId, identity_rev: u64, payload: Payload) -> Want {
    let fencing_rev = g.rev_of(&object).unwrap_or(identity_rev);
    Want { op: g.op_for(&object), kind, object, identity_rev, fencing_rev, payload, after: Vec::new() }
}

/// Channel creation and topic maintenance for every active seat and teamspace.
fn channel_wants(g: &Graph, journal: &Journal, out: &mut Vec<Want>) {
    let objects: Vec<AnyId> =
        g.teamspaces.keys().map(|t| t.to_any()).chain(g.seats.keys().map(|s| s.to_any())).collect();
    for object in objects {
        let Some((_, channel, topic, active)) = g.channel(&object) else { continue };
        if !active {
            continue;
        }
        let rev = g.rev_of(&object).unwrap_or(0);
        if channel.thread_id.is_none() {
            out.push(want(g, EffectKind::EnsureThread, object, rev, Payload::None));
            continue;
        }
        let last = journal.meta_get(&format!("{TOPIC_PREFIX}{object}")).ok().flatten();
        if last.as_deref() != Some(topic.as_str()) {
            out.push(want(g, EffectKind::SetTopic, object, topic_generation(&topic), Payload::None));
        }
    }
}

/// Required and ordinary invitations, requirement release and membership polling for every clone.
fn clone_wants(g: &Graph, out: &mut Vec<Want>) {
    for c in g.clones.values() {
        let object = c.id.to_any();
        let occupant = g.occupant_of(c);
        let seat = g.seats.get(&c.seat);

        // Desired memberships of the current occupant.
        let mut desired: Vec<(String, InviteConstraint, Option<AnyId>)> = Vec::new();
        if occupant.is_some()
            && let Some(seat) = seat
        {
            for owner in [seat.id.to_any(), seat.teamspace.to_any()] {
                if let Some((_, channel, _, _)) = g.channel(&owner) {
                    let ensure = channel.thread_id.is_none().then(|| owner.clone());
                    let thread = channel.thread_id.clone().unwrap_or_else(|| derived_thread(&owner));
                    desired.push((thread, InviteConstraint::Required, ensure));
                }
            }
            for t in &seat.participation.seat_wide {
                if !c.opt_outs.contains(t) {
                    desired.push((t.clone(), InviteConstraint::Ordinary, None));
                }
            }
        }
        for (thread, constraint, ensure) in desired {
            let held = c.invitations.iter().find(|i| i.thread == thread && i.constraint == constraint);
            let same_occupant = |i: &Invitation| i.link.as_ref().and_then(|l| l.occupant.as_ref()) == occupant.as_ref();
            let satisfied = match (held, constraint) {
                // A Required episode is settled for the occupant it was issued for, whatever became of it.
                (Some(i), InviteConstraint::Required) => same_occupant(i),
                (Some(i), InviteConstraint::Ordinary) => {
                    same_occupant(i) && matches!(i.state, InvitationState::Pending | InvitationState::Accepted)
                }
                (None, _) => false,
            };
            if !satisfied {
                let mut w = want(g, EffectKind::Invite, object.clone(), c.rev, Payload::Invite { thread: thread.clone(), constraint });
                w.after.extend(ensure.map(After::Ensure));
                w.after.push(After::Release(object.clone(), thread));
                out.push(w);
            }
        }

        for i in &c.invitations {
            let Some(link) = &i.link else { continue };
            let current = link.occupant == occupant;
            // Occupancy-keyed cleanup: the requirement belongs to an occupant that is gone (or to a retired or
            // deactivated clone); release it.
            if i.constraint == InviteConstraint::Required
                && matches!(i.state, InvitationState::Pending | InvitationState::Accepted)
                && !current
            {
                let payload = Payload::Release { thread: i.thread.clone(), constraint: i.constraint };
                out.push(want(g, EffectKind::ReleaseRequirement, object.clone(), c.rev, payload));
                continue;
            }
            if !current {
                continue;
            }
            let leaving = i.constraint == InviteConstraint::Ordinary
                && i.state == InvitationState::Accepted
                && !seat.is_some_and(|s| s.participation.seat_wide.contains(&i.thread));
            if i.state == InvitationState::Pending || leaving {
                let payload = Payload::Poll { thread: i.thread.clone(), constraint: i.constraint, leaving };
                out.push(want(g, EffectKind::Custom(POLL_KIND.into()), object.clone(), c.rev, payload));
            }
        }
    }
}

/// Everything the committed graph implies, minus what is already settled.
fn system_wants(g: &Graph, journal: &Journal) -> Vec<Want> {
    let mut out = Vec::new();
    channel_wants(g, journal, &mut out);
    clone_wants(g, &mut out);
    out
}

const RELOAD_TEXT: &str = "Your instructions changed on disk. Reload your session (restart the agent or /clear) so it \
                           picks them up.";

/// `reload_required` flags -> one warning per flag episode (cleared when the flag is).
fn reload_wants(g: &Graph, journal: &Journal) -> Vec<Want> {
    let mut out = Vec::new();
    let mut flagged: Vec<(AnyId, u64, bool)> = Vec::new();
    for s in g.seats.values() {
        flagged.push((s.id.to_any(), s.rev, s.reload_required && g.seat_active(s)));
    }
    for c in g.clones.values() {
        flagged.push((c.id.to_any(), c.rev, c.reload_required && g.clone_active(c)));
    }
    for (object, rev, on) in flagged {
        let key = format!("{RELOAD_PREFIX}{object}");
        let notified = journal.meta_get(&key).ok().flatten().is_some();
        if !on {
            if notified {
                let _ = journal.meta_delete(&key);
            }
            continue;
        }
        if notified {
            continue;
        }
        let payload = Payload::Notify {
            purpose: "reload_required".into(),
            severity: Severity::Warn,
            body: RELOAD_TEXT.into(),
            key: format!("notify:reload_required:{object}:{rev}"),
        };
        out.push(want(g, EffectKind::Notify, object, rev, payload));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// source
// ---------------------------------------------------------------------------------------------

pub struct ThreadsSource {
    plans: PlanStore,
    cache: Arc<GraphCache>,
}

impl ThreadsSource {
    pub(crate) fn new(instance: &Path, cache: Arc<GraphCache>) -> Self {
        Self { plans: PlanStore::new(instance.join(".graph-local").join("plans")), cache }
    }

    /// `threads.notify_rename` and `participation.leave_instruction` plan effects of recently committed ops.
    fn intent_wants(&self, g: &Graph, journal: &Journal, now: Timestamp) -> Vec<Want> {
        let mut out = Vec::new();
        let rows = journal.list(&[OpState::Committed], INTENT_SCAN).unwrap_or_default();
        for row in rows {
            if !matches!(row.request.kind, RequestKind::TeamspaceRename | RequestKind::ParticipationLeave)
                || row.updated_at < now - INTENT_WINDOW
            {
                continue;
            }
            let Some(plan_id) = row.request.args.get("_plan").and_then(|v| v.as_str()).and_then(|s| s.parse::<PlanId>().ok())
            else {
                continue;
            };
            let Some(stored) = self.plans.get(&plan_id).ok().flatten() else { continue };
            let moved = stored.plan.effects.iter().find(|e| e.kind == "teamspace.rename").map(|e| {
                let p = |k: &str| e.detail.get(k).and_then(|v| v.as_str()).unwrap_or("?").to_owned();
                (p("path_from"), p("path_to"))
            });
            for e in &stored.plan.effects {
                let text = |k: &str| e.detail.get(k).and_then(|v| v.as_str()).unwrap_or("?").to_owned();
                let (purpose, body) = match e.kind.as_str() {
                    "threads.notify_rename" => {
                        let mut body = format!("Teamspace {:?} was renamed to {:?}.", text("from"), text("to"));
                        if let Some((from, to)) = &moved
                            && from != to
                        {
                            body.push_str(&format!(" Its repository folder moved from {from} to {to}."));
                        }
                        ("rename", body)
                    }
                    "participation.leave_instruction" => {
                        let seat = text("seat");
                        let rev = e.detail.get("rev").and_then(|v| v.as_u64()).unwrap_or(0);
                        let thread = text("thread");
                        let instruction = serde_json::json!({ "op": row.op, "seat": seat, "rev": rev });
                        (
                            "leave_instruction",
                            format!(
                                "This seat no longer participates in thread {thread}. Delayed instruction {instruction}: each clone: run \
                                 `herdr-graph check-instruction {} {seat} {rev}`; if current, leave the thread yourself \
                                 (`herdr-threads leave {thread}`).",
                                row.op
                            ),
                        )
                    }
                    _ => continue,
                };
                let object = e.object.clone();
                let Some(seat) = SeatId::parse(object.as_str()).ok().and_then(|i| g.seats.get(&i)) else { continue };
                if !g.seat_active(seat) {
                    continue;
                }
                let payload = Payload::Notify {
                    purpose: purpose.into(),
                    severity: Severity::Info,
                    body,
                    key: format!("notify:{purpose}:{}:{object}", row.op),
                };
                out.push(Want {
                    kind: EffectKind::Notify,
                    object,
                    op: row.op.clone(),
                    identity_rev: 0,
                    fencing_rev: seat.rev,
                    payload,
                    after: seat.channel.thread_id.is_none().then(|| After::Ensure(seat.id.to_any())).into_iter().collect(),
                });
            }
        }
        out
    }
}

fn payload_key(id: &EffectId) -> String {
    format!("{PAYLOAD_PREFIX}{id}")
}

fn payload_of(journal: &Journal, id: &EffectId) -> Option<Payload> {
    serde_json::from_str(&journal.meta_get(&payload_key(id)).ok().flatten()?).ok()
}

/// Effect rows of this kind on this object that are still open.
fn open_rows(journal: &Journal, object: &AnyId, kind: &EffectKind) -> Vec<EffectRecord> {
    journal
        .effects_for_object(object)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| &r.kind == kind && matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown))
        .collect()
}

impl EffectSource for ThreadsSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let g = match self.cache.get(cx.tree, cx.head) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("herdr-graph: threads: cannot read the graph at {}: {e}", cx.head.0);
                return Vec::new();
            }
        };
        let mut wants = system_wants(&g, cx.journal);
        wants.extend(reload_wants(&g, cx.journal));
        wants.extend(self.intent_wants(&g, cx.journal, cx.now));

        // First pass: identities, dropping wants that an open row already covers.
        let mut ids: Vec<Option<EffectId>> = Vec::with_capacity(wants.len());
        let mut ensure_ids: BTreeMap<AnyId, EffectId> = BTreeMap::new();
        let mut release_ids: BTreeMap<(AnyId, String), EffectId> = BTreeMap::new();
        for w in &wants {
            let open = open_rows(cx.journal, &w.object, &w.kind)
                .into_iter()
                .find(|r| matches!(w.payload, Payload::None) || payload_of(cx.journal, &r.id).as_ref() == Some(&w.payload));
            let id = EffectRecord::identity(&w.op, &w.object, &w.kind, w.identity_rev());
            if w.kind == EffectKind::EnsureThread {
                ensure_ids.insert(w.object.clone(), open.as_ref().map_or(id.clone(), |r| r.id.clone()));
            }
            if let Payload::Release { thread, .. } = &w.payload {
                release_ids.insert((w.object.clone(), thread.clone()), open.as_ref().map_or(id.clone(), |r| r.id.clone()));
            }
            ids.push(open.is_none().then_some(id));
        }

        let mut planned = Vec::new();
        for (w, id) in wants.iter().zip(ids) {
            let Some(id) = id else { continue };
            if !matches!(w.payload, Payload::None)
                && let Ok(json) = serde_json::to_string(&w.payload)
            {
                let _ = cx.journal.meta_set(&payload_key(&id), &json);
            }
            let deps = w
                .after
                .iter()
                .filter_map(|a| match a {
                    After::Ensure(o) => ensure_ids.get(o),
                    After::Release(o, t) => release_ids.get(&(o.clone(), t.clone())),
                })
                .cloned()
                .collect();
            planned.push(PlannedEffect {
                record: EffectRecord {
                    id,
                    op: w.op.clone(),
                    object: w.object.clone(),
                    kind: w.kind.clone(),
                    object_rev: w.identity_rev(),
                    fencing_rev: w.fencing_rev,
                    status: EffectStatus::Pending,
                    predicted: vec![],
                    nonce_label: None,
                    attempts: 0,
                    last_error: None,
                    updated_at: cx.now,
                },
                deps,
            });
        }
        planned
    }
}

// ---------------------------------------------------------------------------------------------
// executor
// ---------------------------------------------------------------------------------------------

pub struct ThreadsExecutor {
    threads: Arc<dyn ThreadsPort>,
    mapping: Arc<dyn PaneSeatMap>,
    journal: Arc<Journal>,
    cache: Arc<GraphCache>,
}

fn port_outcome(e: ThreadsError) -> ExecOutcome {
    match e {
        // Backoff, never takeover: the reconciler schedules the retry.
        ThreadsError::ServiceBusy => ExecOutcome::Transient("threads service busy".into()),
        ThreadsError::Disconnected(m) => ExecOutcome::Transient(m),
        ThreadsError::Unsupported => ExecOutcome::Failed("operation unsupported by this threads service".into()),
        ThreadsError::Rejected(m) => ExecOutcome::Failed(m),
    }
}

fn admit_bookkeeping(cx: &ExecCx<'_>, args: serde_json::Value) -> Result<(), ExecOutcome> {
    let request = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
    };
    cx.writer.admit(request).map(|_| ()).map_err(|e| ExecOutcome::Transient(format!("bookkeeping write: {e}")))
}

/// One invitation write-back. `expect_occupant`: only apply while the stored invitation still belongs to the
/// occupant the effect acted for, so a late write about a previous occupant never overwrites the new one's.
struct InvWrite<'a> {
    clone: &'a CloneId,
    thread: &'a str,
    constraint: InviteConstraint,
    state: InvitationState,
    link: &'a ThreadsLink,
    expect_occupant: Option<&'a str>,
}

fn short(id: &EffectId) -> &str {
    id.suffix6()
}

impl ThreadsExecutor {
    pub(crate) fn new(
        threads: Arc<dyn ThreadsPort>,
        mapping: Arc<dyn PaneSeatMap>,
        journal: Arc<Journal>,
        cache: Arc<GraphCache>,
    ) -> Self {
        Self { threads, mapping, journal, cache }
    }

    fn graph(&self, cx: &ExecCx<'_>) -> Result<Arc<Graph>, ExecOutcome> {
        self.cache.get(cx.tree, cx.head).map_err(|e| ExecOutcome::Transient(format!("cannot read the graph: {e}")))
    }

    fn payload(&self, e: &EffectRecord) -> Option<Payload> {
        payload_of(&self.journal, &e.id)
    }

    fn record_invitation(&self, cx: &ExecCx<'_>, w: InvWrite<'_>) -> Result<(), ExecOutcome> {
        admit_bookkeeping(
            cx,
            serde_json::json!({
                "sub": "invitation", "clone": w.clone, "thread": w.thread, "constraint": w.constraint,
                "state": w.state, "link": w.link, "expect_occupant": w.expect_occupant,
            }),
        )
    }

    async fn ensure(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some((scope, channel, topic, active)) = g.channel(&e.object) else { return ExecOutcome::Obsolete };
        if !active {
            return ExecOutcome::Obsolete;
        }
        if channel.thread_id.is_some() {
            return ExecOutcome::Done;
        }
        let key = OpKey(format!("ensure:{}", e.object));
        match self.threads.ensure_thread(scope, &topic, &key).await {
            Ok(thread) => {
                if let Err(o) = admit_bookkeeping(cx, serde_json::json!({ "sub": "channel", "object": e.object, "thread_id": thread.0 })) {
                    return o;
                }
                let _ = self.journal.meta_set(&format!("{TOPIC_PREFIX}{}", e.object), &topic);
                ExecOutcome::Done
            }
            Err(err) => port_outcome(err),
        }
    }

    async fn set_topic(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some((_, channel, topic, active)) = g.channel(&e.object) else { return ExecOutcome::Obsolete };
        if !active {
            return ExecOutcome::Obsolete;
        }
        let Some(thread) = channel.thread_id.clone() else {
            return ExecOutcome::Deferred("channel not created yet".into());
        };
        let key = OpKey(format!("topic:{}:{:016x}", e.object, topic_generation(&topic)));
        match self.threads.set_topic(&ThreadRef(thread), &topic, &key).await {
            Ok(()) => {
                let _ = self.journal.meta_set(&format!("{TOPIC_PREFIX}{}", e.object), &topic);
                ExecOutcome::Done
            }
            Err(err) => port_outcome(err),
        }
    }

    async fn invite(&self, cx: &ExecCx<'_>, e: &EffectRecord, thread: &str, constraint: InviteConstraint) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str()).ok().and_then(|i| g.clones.get(&i)) else {
            return ExecOutcome::Obsolete;
        };
        let Some(occupant) = g.occupant_of(clone) else { return ExecOutcome::Obsolete };
        let Some(pane) = clone.runtime.bound.as_ref().and_then(|b| b.pane_id.clone()) else {
            return ExecOutcome::Deferred("clone has no pane binding yet".into());
        };
        let seat = match self.mapping.seat_for(&pane).await {
            Ok(Some(s)) => s,
            Ok(None) => return ExecOutcome::Deferred(format!("pane {pane} is not a threads seat yet")),
            Err(err) => return port_outcome(err),
        };
        let thread_ref = ThreadRef(thread.to_owned());
        let key = OpKey(format!("invite:{thread}:{}:{}:{}", seat.0, constraint_name(constraint), short(&e.id)));
        if let Err(err) = self.threads.invite(&thread_ref, &seat, constraint, &key).await {
            return port_outcome(err);
        }
        // What threads reports now; a failed read only delays the ids, never invents a state.
        let detail = self.threads.membership_detail(&thread_ref, &seat).await.ok().flatten();
        let state = detail.as_ref().map_or(InvitationState::Pending, |d| d.state);
        let link = ThreadsLink {
            seat: seat.0.clone(),
            occupant: Some(occupant),
            invitation: detail.as_ref().and_then(|d| d.invitation.clone()),
            requirement: detail.as_ref().and_then(|d| d.requirement.clone()),
            revision: detail.as_ref().and_then(|d| d.revision),
        };
        match self.record_invitation(cx, InvWrite { clone: &clone.id, thread, constraint, state, link: &link, expect_occupant: None }) {
            Ok(()) => ExecOutcome::Done,
            Err(o) => o,
        }
    }

    async fn release(&self, cx: &ExecCx<'_>, e: &EffectRecord, thread: &str, constraint: InviteConstraint) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str()).ok().and_then(|i| g.clones.get(&i)) else {
            return ExecOutcome::Obsolete;
        };
        let Some(inv) = clone.invitations.iter().find(|i| i.thread == thread && i.constraint == constraint) else {
            return ExecOutcome::Obsolete;
        };
        let Some(link) = inv.link.clone() else { return ExecOutcome::Obsolete };
        let key = OpKey(format!(
            "release:{thread}:{}:{}",
            link.seat,
            link.requirement.as_deref().unwrap_or("-")
        ));
        match self.threads.release_requirement(&ThreadRef(thread.to_owned()), &ThreadsSeatRef(link.seat.clone()), &key).await {
            Ok(()) => match self.record_invitation(
                cx,
                InvWrite { clone: &clone.id, thread, constraint, state: InvitationState::Released, link: &link, expect_occupant: link.occupant.as_deref() },
            ) {
                Ok(()) => ExecOutcome::Done,
                Err(o) => o,
            },
            Err(err) => port_outcome(err),
        }
    }

    async fn notify(&self, cx: &ExecCx<'_>, e: &EffectRecord, purpose: &str, severity: Severity, body: &str, key: &str) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let thread = g
            .notify_channel_owner(&e.object)
            .and_then(|owner| g.channel(&owner).and_then(|(_, c, _, _)| c.thread_id.clone()));
        let Some(thread) = thread else { return ExecOutcome::Deferred("channel not created yet".into()) };
        match self.threads.notify(&ThreadRef(thread), severity, body, &OpKey(key.to_owned())).await {
            Ok(()) => {
                if purpose == "reload_required" {
                    let _ = self.journal.meta_set(&format!("{RELOAD_PREFIX}{}", e.object), "1");
                }
                ExecOutcome::Done
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Membership polling: record what threads reports, and only that.
    async fn poll(&self, cx: &ExecCx<'_>, e: &EffectRecord, thread: &str, constraint: InviteConstraint, leaving: bool) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str()).ok().and_then(|i| g.clones.get(&i)) else {
            return ExecOutcome::Obsolete;
        };
        let Some(inv) = clone.invitations.iter().find(|i| i.thread == thread && i.constraint == constraint) else {
            return ExecOutcome::Obsolete;
        };
        let Some(link) = inv.link.clone() else { return ExecOutcome::Obsolete };
        let detail = match self.threads.membership_detail(&ThreadRef(thread.to_owned()), &ThreadsSeatRef(link.seat.clone())).await {
            Ok(d) => d,
            Err(err) => return port_outcome(err),
        };
        // No membership yet: the invitation is still pending on the threads side.
        let state = detail.as_ref().map_or(InvitationState::Pending, |d| d.state);
        let mut fresh = link.clone();
        if let Some(d) = &detail {
            fresh.invitation = d.invitation.clone().or(fresh.invitation);
            fresh.requirement = d.requirement.clone().or(fresh.requirement);
            fresh.revision = d.revision.or(fresh.revision);
        }
        if (state != inv.state || fresh != link)
            && let Err(o) = self.record_invitation(
                cx,
                InvWrite { clone: &clone.id, thread, constraint, state, link: &fresh, expect_occupant: link.occupant.as_deref() },
            )
        {
            return o;
        }
        let settled = match state {
            InvitationState::Pending => false,
            InvitationState::Accepted => !leaving,
            InvitationState::Released | InvitationState::Retired => true,
        };
        if settled { ExecOutcome::Done } else { ExecOutcome::Transient(format!("{thread}: still {state:?}").to_lowercase()) }
    }
}

#[async_trait::async_trait]
impl EffectExecutor for ThreadsExecutor {
    fn handles(&self, kind: &EffectKind) -> bool {
        match kind {
            EffectKind::EnsureThread
            | EffectKind::Invite
            | EffectKind::ReleaseRequirement
            | EffectKind::Notify
            | EffectKind::SetTopic => true,
            EffectKind::Custom(k) => k == POLL_KIND,
            _ => false,
        }
    }

    fn is_implied(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> bool {
        let Ok(g) = self.graph(cx) else { return true };
        let payload = self.payload(e).unwrap_or(Payload::None);
        match (&e.kind, &payload) {
            (_, Payload::Notify { purpose, .. }) => {
                let alive = g.notify_channel_owner(&e.object).and_then(|o| g.channel(&o)).is_some_and(|c| c.3);
                if purpose == "reload_required" {
                    let seat = SeatId::parse(e.object.as_str()).ok().and_then(|i| g.seats.get(&i)).is_some_and(|s| s.reload_required);
                    let clone = CloneId::parse(e.object.as_str()).ok().and_then(|i| g.clones.get(&i)).is_some_and(|c| c.reload_required);
                    alive && (seat || clone)
                } else {
                    alive
                }
            }
            _ => {
                let wants = system_wants(&g, &self.journal);
                wants.iter().any(|w| w.kind == e.kind && w.object == e.object && w.payload == payload)
            }
        }
    }

    async fn execute(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        match (&e.kind, self.payload(e)) {
            (EffectKind::EnsureThread, _) => self.ensure(cx, e).await,
            (EffectKind::SetTopic, _) => self.set_topic(cx, e).await,
            (EffectKind::Invite, Some(Payload::Invite { thread, constraint })) => self.invite(cx, e, &thread, constraint).await,
            (EffectKind::ReleaseRequirement, Some(Payload::Release { thread, constraint })) => {
                self.release(cx, e, &thread, constraint).await
            }
            (EffectKind::Notify, Some(Payload::Notify { purpose, severity, body, key })) => {
                self.notify(cx, e, &purpose, severity, &body, &key).await
            }
            (EffectKind::Custom(_), Some(Payload::Poll { thread, constraint, leaving })) => {
                self.poll(cx, e, &thread, constraint, leaving).await
            }
            (kind, _) => ExecOutcome::Failed(format!("{} effect {} has no payload", kind.as_str(), e.id)),
        }
    }
}
