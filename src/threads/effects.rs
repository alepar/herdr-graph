//! Threads effect family for the reconciler (spec §7.1-7.3): channel creation, required invitations,
//! participation, notifications, membership tracking and occupancy-keyed cleanup.
//!
//! `ThreadsSource` turns committed state into *wants* (a pure function of the graph and a few journal
//! facts); each want becomes a deterministic effect row whose payload lives in the journal's `meta` table
//! keyed by effect id. `ThreadsExecutor` performs them through `ThreadsPort`, records what threads actually
//! reported as bookkeeping mutations, and never records acceptance threads did not report.
use super::assignment::{SeatTransition, admit_threads_seat};
use super::mapping::{PaneSeatMap, SeatResolution};
use crate::journal::Journal;
use crate::model::change::{ChangeRequest, RequestKind, Requester};
use crate::model::clone::{
    CloneRecord, Invitation, InvitationState, InviteConstraint, SeatIntent, ThreadsLink,
};
use crate::model::common::{
    Availability, Channel, CloneLifecycle, CommitId, Lifecycle, NameSource,
};
use crate::model::effect::{EffectKind, EffectRecord, EffectStatus};
use crate::model::launch::graph_token;
use crate::model::operation::OpState;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{
    AnyId, CloneId, EffectId, HerdrPaneId, IdKind, OpId, SeatId, TeamspaceId, Timestamp,
};
use crate::plan::store::PlanStore;
use crate::ports::store::StoreError;
use crate::ports::threads::*;
use crate::reconcile::planner::threads_answered_key;
use crate::reconcile::{DiffCx, EffectExecutor, EffectSource, ExecCx, ExecOutcome, PlannedEffect};
use crate::store::layout;
use crate::store::tree::TreeRead;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// `EffectKind::Custom` name of the membership poll.
pub const POLL_KIND: &str = "threads.membership";
/// `EffectKind::Custom` name of the seat resolution of a bound clone without a `threads_seat`.
pub const RESOLVE_KIND: &str = "threads.resolve_seat";
/// `EffectKind::Custom` name of the seat resolution of an unseated clone from its key reservation.
pub const RESOLVE_RESERVED_KIND: &str = "threads.resolve_reserved_seat";
/// `EffectKind::Custom` name of the clear-on-reattach of a finished Restore intent (spec §14.4).
pub const CLEAR_RESTORE_KIND: &str = "threads.clear_restore";
/// `EffectKind::Custom` name of the seat succession of an undone or resurrected clone (spec §2, §8 interim).
pub const SUCCEED_KIND: &str = "threads.succeed_seat";
/// `EffectKind::Custom` name of the acknowledgement of a Restore whose seat needs no succession.
pub const ACK_RESTORE_KIND: &str = "threads.ack_restore";
/// `EffectKind::Custom` name of the un-retire of a Restore intent's reversibly retired seat (spec §8, D2).
pub const UNRETIRE_KIND: &str = "threads.unretire_seat";
/// `EffectKind::Custom` name of the finalization of a `Finalize{seat}` purge tombstone (spec §14.2).
pub const FINALIZE_KIND: &str = "threads.finalize_seat";
/// `EffectKind::Custom` name of the key release of a `ReleaseKey{key}` purge tombstone (spec §14.3).
pub const RELEASE_KEY_KIND: &str = "threads.release_key";
/// `EffectKind::Custom` name of the seatless continuity-key reservation of an active clone (spec §5, §16.5). The
/// key-link want (hg-66u.68) is the only want that binds a key to a seat.
pub const RESERVE_KEY_KIND: &str = "threads.reserve_key";
/// `EffectKind::Custom` name of the key-link want: binds a recorded reservation to the clone's target seat
/// (spec §14.12). It is the only want that issues a seat-bearing `ReserveContinuityKey`.
pub const KEY_LINK_KIND: &str = "threads.key_link";
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
    pub epoch: String,
    namespace_op: Option<OpId>,
    pub teamspaces: BTreeMap<TeamspaceId, TeamspaceRecord>,
    pub seats: BTreeMap<SeatId, SeatRecord>,
    pub clones: BTreeMap<CloneId, CloneRecord>,
}

impl Graph {
    pub fn load(tree: &dyn TreeRead) -> Result<Self, StoreError> {
        let marker = super::namespace::read(tree)?;
        let mut g = Graph {
            epoch: marker.as_ref().map(|t| t.epoch.clone()).unwrap_or_default(),
            namespace_op: marker.and_then(|t| t.op.parse().ok()),
            ..Default::default()
        };
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
        self.teamspaces
            .get(id)
            .is_some_and(|t| t.lifecycle == Lifecycle::Active)
    }

    pub fn seat_active(&self, s: &SeatRecord) -> bool {
        s.lifecycle == Lifecycle::Active && self.ts_active(&s.teamspace)
    }

    pub fn clone_active(&self, c: &CloneRecord) -> bool {
        c.lifecycle == CloneLifecycle::Active
            && self.seats.get(&c.seat).is_some_and(|s| self.seat_active(s))
    }

    /// The threads seat whose memberships this clone holds (spec §3, §14.11): its recorded `threads_seat` while
    /// the clone is active, or retired and not purged. Occupancy does not matter: invitations wait for the
    /// first SessionStart and survive an agent exit, `/clear` or relaunch on the same pane.
    fn membership_seat(&self, c: &CloneRecord) -> Option<String> {
        let seat = c.threads_seat.as_ref()?;
        let live = match c.lifecycle {
            CloneLifecycle::Active => self.clone_active(c),
            CloneLifecycle::Retired => !c.is_tombstone(),
        };
        live.then(|| seat.clone())
    }

    fn rev_of(&self, object: &AnyId) -> Option<u64> {
        match object.kind() {
            IdKind::Teamspace => TeamspaceId::parse(object.as_str())
                .ok()
                .and_then(|i| self.teamspaces.get(&i))
                .map(|r| r.rev),
            IdKind::Seat => SeatId::parse(object.as_str())
                .ok()
                .and_then(|i| self.seats.get(&i))
                .map(|r| r.rev),
            IdKind::Clone => CloneId::parse(object.as_str())
                .ok()
                .and_then(|i| self.clones.get(&i))
                .map(|r| r.rev),
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
                let Ok(t) = TeamspaceId::parse(object.as_str()) else {
                    return nil;
                };
                self.seats
                    .values()
                    .filter(|s| s.teamspace == t)
                    .filter_map(|s| s.activation.last_op.clone())
                    .max()
                    .unwrap_or(nil)
            }
            _ => nil,
        }
    }

    /// A teamspace or seat channel: scope, record channel, desired topic and whether the owner is active.
    fn channel(&self, object: &AnyId) -> Option<(ChannelScope, &Channel, String, bool)> {
        match object.kind() {
            IdKind::Teamspace => {
                let t = self
                    .teamspaces
                    .get(&TeamspaceId::parse(object.as_str()).ok()?)?;
                Some((
                    ChannelScope::Teamspace,
                    &t.channel,
                    t.name.clone(),
                    t.lifecycle == Lifecycle::Active,
                ))
            }
            IdKind::Seat => {
                let s = self.seats.get(&SeatId::parse(object.as_str()).ok()?)?;
                let ts = self.teamspaces.get(&s.teamspace)?;
                Some((
                    ChannelScope::Seat,
                    &s.channel,
                    format!("{}/{}", ts.name, s.name),
                    self.seat_active(s),
                ))
            }
            _ => None,
        }
    }

    /// The channel a notification about `object` goes to: the object's own for seats and teamspaces, the
    /// clone's seat's for clones.
    fn notify_channel_owner(&self, object: &AnyId) -> Option<AnyId> {
        match object.kind() {
            IdKind::Clone => self
                .clones
                .get(&CloneId::parse(object.as_str()).ok()?)
                .map(|c| c.seat.to_any()),
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
    Invite {
        thread: String,
        constraint: InviteConstraint,
    },
    Release {
        thread: String,
        constraint: InviteConstraint,
    },
    /// `leaving`: an ordinary participation being left; polled until threads reports the member gone.
    Poll {
        thread: String,
        constraint: InviteConstraint,
        leaving: bool,
    },
    Notify {
        purpose: String,
        severity: Severity,
        body: String,
        key: String,
    },
    /// Resolve the threads seat of the clone's verified pane (a different pane is a different effect).
    ResolveSeat {
        pane: HerdrPaneId,
    },
    /// Resolve the seat of an unseated clone from its recorded key reservation (`key`); no pane is needed.
    ResolveReserved {
        key: String,
    },
    /// Drop the Restore intent of `epoch`: the restored agent reattached its seat.
    ClearRestore {
        epoch: OpId,
    },
    /// Resolve the reopened `pane` to a fresh seat and succeed the final-retired seat `from` with it.
    SucceedSeat {
        pane: HerdrPaneId,
        from: String,
        epoch: OpId,
    },
    /// The Restore of `epoch` needs no threads command: its seat is not retired. Sets `threads_done`.
    AckRestore {
        epoch: OpId,
    },
    /// Un-retire `seat`, the reversibly retired seat of the Restore intent of `epoch`.
    UnretireSeat {
        seat: String,
        epoch: OpId,
    },
    /// Finalize the retirement of `seat` for its `Finalize` purge tombstone.
    FinalizeSeat {
        seat: String,
    },
    /// Release `key` for its `ReleaseKey` purge tombstone, or route it to its seat's finalization.
    ReleaseKey {
        key: String,
    },
    /// Reserve the clone's continuity key (its id) with threads, bound to no seat.
    ReserveKey,
    /// Bind the clone's reserved `key` to `seat` (the key-link want).
    KeyLink {
        key: String,
        seat: String,
    },
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
    /// The `ReleaseRequirement` of this clone's invitation to this thread: the old seat's episode ends
    /// before the new seat's begins.
    Release(AnyId, String),
}

/// The thread id graph derives from an object id (also what the port derives from `ensure:<object>`).
#[cfg(test)]
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
    Want {
        op: g.op_for(&object),
        kind,
        object,
        identity_rev,
        fencing_rev,
        payload,
        after: Vec::new(),
    }
}

/// Channel creation and topic maintenance for every active seat and teamspace.
fn channel_wants(g: &Graph, journal: &Journal, out: &mut Vec<Want>) {
    let objects: Vec<AnyId> = g
        .teamspaces
        .keys()
        .map(|t| t.to_any())
        .chain(g.seats.keys().map(|s| s.to_any()))
        .collect();
    for object in objects {
        let Some((_, channel, topic, active)) = g.channel(&object) else {
            continue;
        };
        if !active {
            continue;
        }
        let rev = g.rev_of(&object).unwrap_or(0);
        if channel.thread_id.is_none() {
            out.push(want(
                g,
                EffectKind::EnsureThread,
                object,
                rev,
                Payload::None,
            ));
            continue;
        }
        let last = journal
            .meta_get(&super::namespace::salt(
                &g.epoch,
                &format!("{TOPIC_PREFIX}{object}"),
            ))
            .ok()
            .flatten();
        if last.as_deref() != Some(topic.as_str()) {
            out.push(want(
                g,
                EffectKind::SetTopic,
                object,
                topic_generation(&topic),
                Payload::None,
            ));
        }
    }
}

/// Required and ordinary invitations, requirement release and membership polling for every clone.
/// The pane to resolve a threads seat for: the clone is active, has no `threads_seat` and no pending Restore
/// intent (the succession want owns that case), and its binding is verified (present, in a pane, stamped
/// with the clone's own token).
fn resolvable_pane(g: &Graph, c: &CloneRecord) -> Option<HerdrPaneId> {
    if c.threads_seat.is_some() || matches!(c.seat_intent, Some(SeatIntent::Restore { .. })) {
        return None;
    }
    verified_pane(g, c)
}

/// The key to resolve an unseated clone's seat from: the clone is active, has no `threads_seat` and no pending
/// Restore intent and holds a recorded key reservation. The key is the evidence, so no pane binding is needed
/// (spec §2 "Unseated clone after D1").
fn resolvable_key(g: &Graph, c: &CloneRecord) -> Option<String> {
    if c.threads_seat.is_some()
        || !g.clone_active(c)
        || c.is_tombstone()
        || matches!(c.seat_intent, Some(SeatIntent::Restore { .. }))
    {
        return None;
    }
    c.key_reservation.as_ref().map(|r| r.key.clone())
}

/// An unseated reserved clone with a verified pane (which keeps its pane want) still asks its key once after
/// each restart: the answer is how the restart trigger learns that threads is reachable.
fn awaiting_restart_answer(journal: &Journal, c: &CloneRecord) -> bool {
    let parse = |raw: Option<String>| {
        chrono::DateTime::parse_from_rfc3339(&raw?)
            .ok()
            .map(|t| t.to_utc())
    };
    let Some(changed) = parse(journal.meta_get("incarnation:changed_at").ok().flatten()) else {
        return false;
    };
    let answered = parse(
        journal
            .meta_get(&threads_answered_key(&c.id))
            .ok()
            .flatten(),
    );
    answered.is_none_or(|at| at <= changed)
}

/// The clone's pane when its binding is verified: the clone is active, present, and the pane carries the
/// clone's own token.
fn verified_pane(g: &Graph, c: &CloneRecord) -> Option<HerdrPaneId> {
    if !g.clone_active(c) || c.runtime.availability != Availability::Present {
        return None;
    }
    let bound = c.runtime.bound.as_ref()?;
    (bound.token.as_deref() == Some(graph_token(&c.id.to_any()).as_str()))
        .then(|| bound.pane_id.clone())
        .flatten()
}

/// An active clone with no recorded key reservation: the reserve-key want reserves its key, seatless (§16.5).
fn reservable(g: &Graph, c: &CloneRecord) -> bool {
    g.clone_active(c) && !c.is_tombstone() && c.key_reservation.is_none()
}

/// The key-link want's `(key, target seat)` (spec §14.12): the clone is active, its reservation is recorded and
/// not bound to the target seat. The target is `Restore.seat` while a Restore intent is pending, else
/// `threads_seat`. A pending Restore whose threads step is not done has a retired seat, which threads refuses a
/// seat-bearing reserve; the succession or unretire want moves it first.
pub(crate) fn key_link_target(g: &Graph, c: &CloneRecord) -> Option<(String, String)> {
    if !g.clone_active(c) || c.is_tombstone() {
        return None;
    }
    let reservation = c.key_reservation.as_ref()?;
    let target = match &c.seat_intent {
        Some(SeatIntent::Restore {
            threads_done: false,
            ..
        }) => return None,
        Some(SeatIntent::Restore { seat, .. }) => seat,
        _ => c.threads_seat.as_ref()?,
    };
    (!reservation.bound_to(target)).then(|| (reservation.key.clone(), target.clone()))
}

/// What an unacknowledged Restore intent asks of threads (spec §2 succession rule, §8 interim).
#[derive(Debug, PartialEq, Eq)]
enum Succession {
    /// The Restore's seat is final-retired: resolve the reopened pane (`pane`) to a fresh seat.
    Succeed {
        pane: HerdrPaneId,
        from: String,
        epoch: OpId,
    },
    /// Threads reads the Restore's seat on the clone's own verified pane (Resolved there, or Unresolved once the
    /// pane is bound): nothing to succeed, record `threads_done`.
    Acknowledge { epoch: OpId },
}

/// The succession a clone's pending Restore calls for, read from the committed `seat_fact`. No fact (threads
/// never read, or unreachable since), a key conflict, or a reversible retirement (D2) asks for nothing.
fn succession(g: &Graph, c: &CloneRecord) -> Option<Succession> {
    use crate::model::clone::SeatFactState as F;
    let Some(SeatIntent::Restore {
        seat,
        epoch,
        threads_done: false,
        ..
    }) = &c.seat_intent
    else {
        return None;
    };
    if !g.clone_active(c) || c.threads_seat.as_ref() != Some(seat) {
        return None;
    }
    let fact = c.seat_fact.as_ref().filter(|f| &f.seat == seat)?;
    match fact.state {
        F::Retired { reversible: false } => Some(Succession::Succeed {
            pane: verified_pane(g, c)?,
            from: seat.clone(),
            epoch: epoch.clone(),
        }),
        // The acknowledgement lost to a crash (§8 step 2, §16.4) is read on the clone's own reopened pane. A read
        // taken before threads has caught up with a close (the node manager's Restore is written by the closing
        // cascade itself) must not stand in for it: wait for Retired (succession/unretire) or for this pane.
        F::Resolved { ref pane } if verified_pane(g, c).as_ref() == Some(pane) => {
            Some(Succession::Acknowledge {
                epoch: epoch.clone(),
            })
        }
        F::Unresolved if verified_pane(g, c).is_some() => Some(Succession::Acknowledge {
            epoch: epoch.clone(),
        }),
        F::Unresolved | F::Resolved { .. } => None,
        F::Retired { reversible: true } | F::KeyConflict { .. } => None,
    }
}

/// The seat and epoch of a pending Restore whose seat the committed `seat_fact` reads reversibly retired: the
/// unretire want asks threads to bring it back. Only a Restore intent asks (an ordinary pane close is never
/// reversed), and only against a reversible read; a final one is the succession's.
fn unretirable(g: &Graph, c: &CloneRecord) -> Option<(String, OpId)> {
    use crate::model::clone::SeatFactState as F;
    let Some(SeatIntent::Restore {
        seat,
        epoch,
        threads_done: false,
        ..
    }) = &c.seat_intent
    else {
        return None;
    };
    if !g.clone_active(c) || c.threads_seat.as_ref() != Some(seat) {
        return None;
    }
    let fact = c.seat_fact.as_ref().filter(|f| &f.seat == seat)?;
    matches!(fact.state, F::Retired { reversible: true }).then(|| (seat.clone(), epoch.clone()))
}

/// The epoch of a Restore intent that is finished: threads acknowledged it, threads reads the clone's seat
/// `Resolved` under the Restore's seat, and an agent occupies the pane again (it checked in).
fn reattached_restore(g: &Graph, c: &CloneRecord) -> Option<OpId> {
    let Some(SeatIntent::Restore {
        seat,
        epoch,
        threads_done: true,
        ..
    }) = &c.seat_intent
    else {
        return None;
    };
    let resolved = c.seat_fact.as_ref().is_some_and(|f| {
        &f.seat == seat && matches!(f.state, crate::model::clone::SeatFactState::Resolved { .. })
    });
    (g.clone_active(c) && c.threads_seat.as_ref() == Some(seat) && resolved && c.occupant.is_some())
        .then(|| epoch.clone())
}

fn clone_wants(g: &Graph, journal: &Journal, out: &mut Vec<Want>) {
    for c in g.clones.values() {
        let object = c.id.to_any();
        if let Some(pane) = resolvable_pane(g, c) {
            out.push(want(
                g,
                EffectKind::Custom(RESOLVE_KIND.into()),
                object.clone(),
                c.rev,
                Payload::ResolveSeat { pane },
            ));
        }
        if let Some(key) = resolvable_key(g, c)
            .filter(|_| verified_pane(g, c).is_none() || awaiting_restart_answer(journal, c))
        {
            out.push(want(
                g,
                EffectKind::Custom(RESOLVE_RESERVED_KIND.into()),
                object.clone(),
                c.rev,
                Payload::ResolveReserved { key },
            ));
        }
        match succession(g, c) {
            Some(Succession::Succeed { pane, from, epoch }) => out.push(want(
                g,
                EffectKind::Custom(SUCCEED_KIND.into()),
                object.clone(),
                c.rev,
                Payload::SucceedSeat { pane, from, epoch },
            )),
            Some(Succession::Acknowledge { epoch }) => out.push(want(
                g,
                EffectKind::Custom(ACK_RESTORE_KIND.into()),
                object.clone(),
                c.rev,
                Payload::AckRestore { epoch },
            )),
            None => {}
        }
        if let Some((seat, epoch)) = unretirable(g, c) {
            out.push(want(
                g,
                EffectKind::Custom(UNRETIRE_KIND.into()),
                object.clone(),
                c.rev,
                Payload::UnretireSeat { seat, epoch },
            ));
        }
        match &c.seat_intent {
            Some(SeatIntent::Finalize { seat }) => out.push(want(
                g,
                EffectKind::Custom(FINALIZE_KIND.into()),
                object.clone(),
                c.rev,
                Payload::FinalizeSeat { seat: seat.clone() },
            )),
            Some(SeatIntent::ReleaseKey { key }) => out.push(want(
                g,
                EffectKind::Custom(RELEASE_KEY_KIND.into()),
                object.clone(),
                c.rev,
                Payload::ReleaseKey { key: key.clone() },
            )),
            _ => {}
        }
        if reservable(g, c) {
            out.push(want(
                g,
                EffectKind::Custom(RESERVE_KEY_KIND.into()),
                object.clone(),
                c.rev,
                Payload::ReserveKey,
            ));
        }
        if let Some((key, seat)) = key_link_target(g, c) {
            out.push(want(
                g,
                EffectKind::Custom(KEY_LINK_KIND.into()),
                object.clone(),
                c.rev,
                Payload::KeyLink { key, seat },
            ));
        }
        if let Some(epoch) = reattached_restore(g, c) {
            out.push(want(
                g,
                EffectKind::Custom(CLEAR_RESTORE_KIND.into()),
                object.clone(),
                c.rev,
                Payload::ClearRestore { epoch },
            ));
        }
        let threads_seat = g.membership_seat(c);
        let seat = g.seats.get(&c.seat);

        // Desired memberships of the clone's threads seat.
        let mut desired: Vec<(String, InviteConstraint, Option<AnyId>)> = Vec::new();
        if threads_seat.is_some()
            && let Some(seat) = seat
        {
            for owner in [seat.id.to_any(), seat.teamspace.to_any()] {
                if let Some((_, channel, _, _)) = g.channel(&owner) {
                    let ensure = channel.thread_id.is_none().then(|| owner.clone());
                    let thread = channel.thread_id.clone().unwrap_or_else(|| {
                        super::fake::thread_for_ensure_key(&OpKey(super::namespace::salt(
                            &g.epoch,
                            &format!("ensure:{owner}"),
                        )))
                        .0
                    });
                    desired.push((thread, InviteConstraint::Required, ensure));
                }
            }
            for t in &seat.participation.seat_wide {
                if !c.opt_outs.contains(t) {
                    desired.push((t.clone(), InviteConstraint::Ordinary, None));
                }
            }
        }
        for (thread, constraint, ensure) in desired.iter().cloned() {
            let held = c
                .invitations
                .iter()
                .find(|i| i.thread == thread && i.constraint == constraint);
            let same_seat =
                |i: &Invitation| i.link.as_ref().map(|l| &l.seat) == threads_seat.as_ref();
            // Spec §3, every constraint: a link satisfies the want only on the recorded seat and Pending or
            // Accepted; a Released or Retired row there is unsatisfied, so an active clone's want re-invites it.
            let satisfied = held.is_some_and(|i| {
                same_seat(i)
                    && matches!(
                        i.state,
                        InvitationState::Pending | InvitationState::Accepted
                    )
            });
            // Invitations are for an active clone; a retired one keeps what it holds and gains nothing.
            if !satisfied && g.clone_active(c) {
                let mut w = want(
                    g,
                    EffectKind::Invite,
                    object.clone(),
                    c.rev,
                    Payload::Invite {
                        thread: thread.clone(),
                        constraint,
                    },
                );
                w.after.extend(ensure.map(After::Ensure));
                w.after.push(After::Release(object.clone(), thread));
                out.push(w);
            }
        }

        for i in &c.invitations {
            let Some(link) = &i.link else { continue };
            // Seat-keyed cleanup: a requirement is released only when the seat it was issued for is no longer
            // the clone's (succession) or the want is gone (opt-out, channel removed). Occupancy and
            // retirement release nothing; a purge ends the memberships (threads finalizes the seat).
            let Some(current_seat) = &threads_seat else {
                continue;
            };
            let current = &link.seat == current_seat;
            let wanted = desired
                .iter()
                .any(|(t, k, _)| *t == i.thread && *k == i.constraint);
            if i.constraint == InviteConstraint::Required
                && matches!(
                    i.state,
                    InvitationState::Pending | InvitationState::Accepted
                )
                && (!current || !wanted)
            {
                let payload = Payload::Release {
                    thread: i.thread.clone(),
                    constraint: i.constraint,
                };
                out.push(want(
                    g,
                    EffectKind::ReleaseRequirement,
                    object.clone(),
                    c.rev,
                    payload,
                ));
                continue;
            }
            if !current {
                continue;
            }
            let leaving = i.constraint == InviteConstraint::Ordinary
                && i.state == InvitationState::Accepted
                && !seat.is_some_and(|s| s.participation.seat_wide.contains(&i.thread));
            if i.state == InvitationState::Pending || leaving {
                let payload = Payload::Poll {
                    thread: i.thread.clone(),
                    constraint: i.constraint,
                    leaving,
                };
                out.push(want(
                    g,
                    EffectKind::Custom(POLL_KIND.into()),
                    object.clone(),
                    c.rev,
                    payload,
                ));
            }
        }
    }
}

/// Everything the committed graph implies, minus what is already settled.
fn system_wants(g: &Graph, journal: &Journal) -> Vec<Want> {
    let mut out = Vec::new();
    channel_wants(g, journal, &mut out);
    clone_wants(g, journal, &mut out);
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
        let key = super::namespace::salt(&g.epoch, &format!("{RELOAD_PREFIX}{object}"));
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
    instance: PathBuf,
    plans: PlanStore,
    cache: Arc<GraphCache>,
}

impl ThreadsSource {
    pub(crate) fn new(instance: &Path, cache: Arc<GraphCache>) -> Self {
        Self {
            instance: instance.to_path_buf(),
            plans: PlanStore::new(instance.join(".graph-local").join("plans")),
            cache,
        }
    }

    /// `.graph-local/worktree_dirty` entries (spec §3.5) -> one warning per (op, file) to the channel of the
    /// active seat whose folder contains the file. The effect identity folds in the key, so the journal's
    /// known-id check makes it once per file per op, across restarts. Files outside every seat folder are
    /// left to `doctor`.
    fn dirty_wants(&self, g: &Graph, tree: &dyn TreeRead) -> Vec<Want> {
        let entries = crate::writer::worktree::read_dirty_all(&self.instance);
        if entries.is_empty() {
            return Vec::new();
        }
        let Ok(seats) = layout::all_seats(tree) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries {
            let owner = seats
                .iter()
                .filter(|(loc, _)| {
                    let f = loc.folder.as_str();
                    !f.is_empty()
                        && entry
                            .path
                            .strip_prefix(f)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
                .max_by_key(|(loc, _)| loc.folder.as_str().len());
            let Some((_, rec)) = owner else { continue };
            let Some(seat) = g.seats.get(&rec.id) else {
                continue;
            };
            if !g.seat_active(seat) {
                continue;
            }
            let op = entry
                .op
                .clone()
                .unwrap_or_else(|| OpId::from_ulid(ulid::Ulid::nil()));
            let path = &entry.path;
            let payload = Payload::Notify {
                purpose: "worktree_dirty".into(),
                severity: Severity::Warn,
                body: format!(
                    "Your local edit to {path} was left in place: graph op {op} changed that file and did not overwrite it \
                     (see `herdr-graph doctor`). Commit your change through a plan (herdr-graph plan instructions set …, then herdr-graph apply) or discard it."
                ),
                key: format!("notify:worktree_dirty:{op}:{path}"),
            };
            out.push(Want {
                kind: EffectKind::Notify,
                object: seat.id.to_any(),
                op,
                identity_rev: 0,
                fencing_rev: seat.rev,
                payload,
                after: seat
                    .channel
                    .thread_id
                    .is_none()
                    .then(|| After::Ensure(seat.id.to_any()))
                    .into_iter()
                    .collect(),
            });
        }
        out
    }

    /// `threads.notify_rename` and `participation.leave_instruction` plan effects of recently committed ops.
    fn intent_wants(&self, g: &Graph, journal: &Journal, now: Timestamp) -> Vec<Want> {
        let mut out = Vec::new();
        let rows = journal
            .list(&[OpState::Committed], INTENT_SCAN)
            .unwrap_or_default();
        for row in rows {
            if let Some(op) = &g.namespace_op {
                if journal
                    .get(op)
                    .ok()
                    .flatten()
                    .is_none_or(|boundary| row.seq <= boundary.seq)
                {
                    continue;
                }
            }
            if !matches!(
                row.request.kind,
                RequestKind::TeamspaceRename
                    | RequestKind::SeatRename
                    | RequestKind::ParticipationLeave
            ) || row.updated_at < now - INTENT_WINDOW
            {
                continue;
            }
            let Some(plan_id) = row.request.confirmed.as_ref().map(|c| c.plan.clone()) else {
                continue;
            };
            let Some(stored) = self.plans.get(&plan_id).ok().flatten() else {
                continue;
            };
            let moved = stored
                .plan
                .effects
                .iter()
                .find(|e| e.kind == "teamspace.rename")
                .map(|e| {
                    let p = |k: &str| {
                        e.detail
                            .get(k)
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_owned()
                    };
                    (p("path_from"), p("path_to"))
                });
            for e in &stored.plan.effects {
                let text = |k: &str| {
                    e.detail
                        .get(k)
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_owned()
                };
                let (purpose, body) = match e.kind.as_str() {
                    "threads.notify_rename" => {
                        if e.detail.get("seat").is_some() {
                            let mut body =
                                format!("Seat {:?} was renamed to {:?}.", text("from"), text("to"));
                            let (from, to) = (text("path_from"), text("path_to"));
                            if from != to {
                                body.push_str(&format!(" Its folder moved from {from} to {to}."));
                            }
                            ("rename", body)
                        } else {
                            let mut body = format!(
                                "Teamspace {:?} was renamed to {:?}.",
                                text("from"),
                                text("to")
                            );
                            if let Some((from, to)) = &moved
                                && from != to
                            {
                                body.push_str(&format!(
                                    " Its repository folder moved from {from} to {to}."
                                ));
                            }
                            ("rename", body)
                        }
                    }
                    "participation.leave_instruction" => {
                        let seat = text("seat");
                        let rev = e.detail.get("rev").and_then(|v| v.as_u64()).unwrap_or(0);
                        let thread = text("thread");
                        let instruction =
                            serde_json::json!({ "op": row.op, "seat": seat, "rev": rev });
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
                let Some(seat) = SeatId::parse(object.as_str())
                    .ok()
                    .and_then(|i| g.seats.get(&i))
                else {
                    continue;
                };
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
                    after: seat
                        .channel
                        .thread_id
                        .is_none()
                        .then(|| After::Ensure(seat.id.to_any()))
                        .into_iter()
                        .collect(),
                });
            }
        }
        out
    }

    /// Observed renames (a tab, workspace or pane relabelled in Herdr) of recently committed ops -> a rename
    /// notice, with the new folder path, to each affected active seat channel. The old name comes from the
    /// committed `name_history` entry this op wrote; an op that recorded nothing (same name, retired object)
    /// notifies nobody.
    fn observed_rename_wants(
        &self,
        g: &Graph,
        tree: &dyn TreeRead,
        journal: &Journal,
        now: Timestamp,
    ) -> Vec<Want> {
        let mut out = Vec::new();
        let rows = journal
            .list(&[OpState::Committed], INTENT_SCAN)
            .unwrap_or_default();
        for row in rows {
            if let Some(op) = &g.namespace_op {
                if journal
                    .get(op)
                    .ok()
                    .flatten()
                    .is_none_or(|boundary| row.seq <= boundary.seq)
                {
                    continue;
                }
            }
            let args = &row.request.args;
            if row.request.kind != RequestKind::Observed
                || args.get("sub").and_then(|v| v.as_str()) != Some("rename")
                || row.updated_at < now - INTENT_WINDOW
            {
                continue;
            }
            let Some(object) = args
                .get("object")
                .and_then(|v| serde_json::from_value::<AnyId>(v.clone()).ok())
            else {
                continue;
            };
            let Some(new) = args.get("new").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(observed_at) = args
                .get("observed_at")
                .and_then(|v| serde_json::from_value::<Timestamp>(v.clone()).ok())
            else {
                continue;
            };
            let history = |h: &[crate::model::common::NameChange]| {
                h.iter()
                    .rev()
                    .find(|c| {
                        c.source == NameSource::Observed
                            && c.new == new
                            && c.observed_at == observed_at
                    })
                    .map(|c| c.old.clone())
            };
            let path_of = |id: &AnyId| {
                layout::locate(tree, id)
                    .ok()
                    .flatten()
                    .map(|l| l.folder.as_str().to_owned())
            };
            let mut notes: Vec<(&SeatRecord, String)> = Vec::new();
            match object.kind() {
                IdKind::Teamspace => {
                    let Some(ts) = TeamspaceId::parse(object.as_str())
                        .ok()
                        .and_then(|i| g.teamspaces.get(&i))
                    else {
                        continue;
                    };
                    let Some(old) = history(&ts.name_history) else {
                        continue;
                    };
                    let Some(ts_path) = path_of(&object) else {
                        continue;
                    };
                    for seat in g
                        .seats
                        .values()
                        .filter(|s| s.teamspace == ts.id && g.seat_active(s))
                    {
                        let Some(seat_path) = path_of(&seat.id.to_any()) else {
                            continue;
                        };
                        notes.push((
                            seat,
                            format!(
                                "Teamspace {old:?} was renamed to {new:?} in Herdr. Its folder is now {ts_path}; this seat's folder is now {seat_path}."
                            ),
                        ));
                    }
                }
                IdKind::Seat => {
                    let Some(seat) = SeatId::parse(object.as_str())
                        .ok()
                        .and_then(|i| g.seats.get(&i))
                    else {
                        continue;
                    };
                    let Some(old) = history(&seat.name_history) else {
                        continue;
                    };
                    if !g.seat_active(seat) {
                        continue;
                    }
                    let Some(path) = path_of(&object) else {
                        continue;
                    };
                    notes.push((seat, format!("Seat {old:?} was renamed to {new:?} in Herdr. Its folder is now {path}.")));
                }
                IdKind::Clone => {
                    let Some(clone) = CloneId::parse(object.as_str())
                        .ok()
                        .and_then(|i| g.clones.get(&i))
                    else {
                        continue;
                    };
                    let Some(old) = history(&clone.name_history) else {
                        continue;
                    };
                    let Some(seat) = g.seats.get(&clone.seat).filter(|s| g.seat_active(s)) else {
                        continue;
                    };
                    let Some(path) = path_of(&object) else {
                        continue;
                    };
                    notes.push((
                        seat,
                        format!("Clone {old:?} of this seat was renamed to {new:?} in Herdr. Its folder is now {path}."),
                    ));
                }
                _ => continue,
            }
            out.extend(
                notes
                    .into_iter()
                    .map(|(seat, body)| rename_want(seat, &row.op, body)),
            );
        }
        out
    }
}

/// A rename notice for one seat channel; once per (op, seat) through the effect identity.
fn rename_want(seat: &SeatRecord, op: &OpId, body: String) -> Want {
    Want {
        kind: EffectKind::Notify,
        object: seat.id.to_any(),
        op: op.clone(),
        identity_rev: 0,
        fencing_rev: seat.rev,
        payload: Payload::Notify {
            purpose: "rename".into(),
            severity: Severity::Info,
            body,
            key: format!("notify:rename:{op}:{}", seat.id),
        },
        after: seat
            .channel
            .thread_id
            .is_none()
            .then(|| After::Ensure(seat.id.to_any()))
            .into_iter()
            .collect(),
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
        .filter(|r| {
            &r.kind == kind && matches!(r.status, EffectStatus::Pending | EffectStatus::Unknown)
        })
        .collect()
}

impl EffectSource for ThreadsSource {
    fn effects(&self, cx: &DiffCx<'_>) -> Vec<PlannedEffect> {
        let g = match self.cache.get(cx.tree, cx.head) {
            Ok(g) => g,
            Err(e) => {
                eprintln!(
                    "herdr-graph: threads: cannot read the graph at {}: {e}",
                    cx.head.0
                );
                return Vec::new();
            }
        };
        let mut wants = system_wants(&g, cx.journal);
        wants.extend(reload_wants(&g, cx.journal));
        wants.extend(self.intent_wants(&g, cx.journal, cx.now));
        wants.extend(self.observed_rename_wants(&g, cx.tree, cx.journal, cx.now));
        wants.extend(self.dirty_wants(&g, cx.tree));

        // First pass: identities, dropping wants that an open row already covers.
        let mut ids: Vec<Option<EffectId>> = Vec::with_capacity(wants.len());
        let mut ensure_ids: BTreeMap<AnyId, EffectId> = BTreeMap::new();
        let mut release_ids: BTreeMap<(AnyId, String), EffectId> = BTreeMap::new();
        for w in &wants {
            let open = open_rows(cx.journal, &w.object, &w.kind)
                .into_iter()
                .find(|r| {
                    r.nonce_label == epoch_label(&g.epoch)
                        && (matches!(w.payload, Payload::None)
                            || payload_of(cx.journal, &r.id).as_ref() == Some(&w.payload))
                });
            let id = EffectRecord::identity(
                &w.op,
                &w.object,
                &w.kind,
                epoch_rev(&g.epoch, w.identity_rev()),
            );
            if w.kind == EffectKind::EnsureThread {
                ensure_ids.insert(
                    w.object.clone(),
                    open.as_ref().map_or(id.clone(), |r| r.id.clone()),
                );
            }
            if let Payload::Release { thread, .. } = &w.payload {
                release_ids.insert(
                    (w.object.clone(), thread.clone()),
                    open.as_ref().map_or(id.clone(), |r| r.id.clone()),
                );
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
                    nonce_label: epoch_label(&g.epoch),
                    attempts: 0,
                    last_error: None,
                    updated_at: cx.now,
                    sched: Default::default(),
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
    /// The finalize acknowledgement read: a refused finalization is done when the seat reads final-retired.
    seat_view: Arc<dyn ThreadsSeatView>,
    journal: Arc<Journal>,
    cache: Arc<GraphCache>,
}

/// Remember the threads refusal of a pending Restore's step on the intent, for status (§14.7). Best effort:
/// a writer error changes nothing, and an unchanged reason is not written again.
fn record_refusal(cx: &ExecCx<'_>, clone: &CloneRecord, epoch: &OpId, reason: &str) {
    if matches!(
        &clone.seat_intent,
        Some(SeatIntent::Restore { last_refusal: Some(r), .. }) if r == reason
    ) {
        return;
    }
    let _ = admit_threads_seat(
        cx.writer,
        &clone.id,
        &SeatTransition::RecordRefusal {
            epoch: epoch.clone(),
            reason: reason.to_owned(),
        },
    );
}

fn port_outcome(e: ThreadsError) -> ExecOutcome {
    match e {
        // Backoff, never takeover: the reconciler schedules the retry.
        ThreadsError::ServiceBusy => ExecOutcome::Transient("threads service busy".into()),
        ThreadsError::Disconnected(m) => ExecOutcome::Transient(m),
        ThreadsError::Unsupported => {
            ExecOutcome::Failed("operation unsupported by this threads service".into())
        }
        ThreadsError::Rejected(m) => ExecOutcome::Failed(m),
    }
}

fn admit_bookkeeping(cx: &ExecCx<'_>, mut args: serde_json::Value) -> Result<(), ExecOutcome> {
    super::namespace::scoped_stamp(&mut args);
    let request = ChangeRequest {
        kind: RequestKind::Bookkeeping,
        args,
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    };
    cx.writer
        .admit(request)
        .map(|_| ())
        .map_err(|e| ExecOutcome::Transient(format!("bookkeeping write: {e}")))
}

/// One invitation write-back. `expect_seat`: only apply while the stored invitation still belongs to the
/// threads seat the effect acted for, so a late write about a previous seat never overwrites the new one's.
struct InvWrite<'a> {
    clone: &'a CloneId,
    thread: &'a str,
    constraint: InviteConstraint,
    state: InvitationState,
    link: &'a ThreadsLink,
    expect_seat: Option<&'a str>,
}

fn short(id: &EffectId) -> &str {
    id.suffix6()
}

impl ThreadsExecutor {
    pub(crate) fn new(
        threads: Arc<dyn ThreadsPort>,
        mapping: Arc<dyn PaneSeatMap>,
        seat_view: Arc<dyn ThreadsSeatView>,
        journal: Arc<Journal>,
        cache: Arc<GraphCache>,
    ) -> Self {
        Self {
            threads,
            mapping,
            seat_view,
            journal,
            cache,
        }
    }

    fn graph(&self, cx: &ExecCx<'_>) -> Result<Arc<Graph>, ExecOutcome> {
        self.cache
            .get(cx.tree, cx.head)
            .map_err(|e| ExecOutcome::Transient(format!("cannot read the graph: {e}")))
    }

    fn payload(&self, e: &EffectRecord) -> Option<Payload> {
        payload_of(&self.journal, &e.id)
    }

    fn record_invitation(&self, cx: &ExecCx<'_>, w: InvWrite<'_>) -> Result<(), ExecOutcome> {
        admit_bookkeeping(
            cx,
            serde_json::json!({
                "sub": "invitation", "clone": w.clone, "thread": w.thread, "constraint": w.constraint,
                "state": w.state, "link": w.link, "expect_seat": w.expect_seat,
            }),
        )
    }

    /// Resolve the clone's pane to its threads seat and record it write-once. A threads refusal defers (the
    /// operation key is stable, so a retry replays the same resolution); a rejected assignment is surfaced,
    /// never retried with another seat.
    fn clear_restore(&self, cx: &ExecCx<'_>, e: &EffectRecord, epoch: &OpId) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if reattached_restore(&g, clone).as_ref() != Some(epoch) {
            return ExecOutcome::Obsolete;
        }
        match admit_threads_seat(
            cx.writer,
            &clone.id,
            &SeatTransition::ClearOnReattach {
                epoch: epoch.clone(),
            },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("seat intent write: {err}")),
        }
    }

    fn ack_restore(&self, cx: &ExecCx<'_>, e: &EffectRecord, epoch: &OpId) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if succession(&g, clone)
            != Some(Succession::Acknowledge {
                epoch: epoch.clone(),
            })
        {
            return ExecOutcome::Obsolete;
        }
        match admit_threads_seat(
            cx.writer,
            &clone.id,
            &SeatTransition::Acknowledge {
                epoch: epoch.clone(),
            },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("seat intent write: {err}")),
        }
    }

    /// The clone record of a purge tombstone effect, or None when the effect is no longer implied.
    fn tombstone<'g>(g: &'g Graph, e: &EffectRecord) -> Option<&'g CloneRecord> {
        CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
            .filter(|c| c.is_tombstone())
    }

    fn drop_tombstone(&self, cx: &ExecCx<'_>, clone: &CloneId) -> ExecOutcome {
        match admit_threads_seat(cx.writer, clone, &SeatTransition::TombstoneDone) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("tombstone delete: {err}")),
        }
    }

    /// Ask threads to un-retire the reversibly retired seat of a pending Restore, then record `threads_done`. A
    /// refusal keeps the intent, remembers why for status and waits: the next read may show the seat final (the
    /// succession's), or threads may lift the refusal. The operation key is stable per (clone, epoch).
    async fn unretire_seat(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        seat: &str,
        epoch: &OpId,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if unretirable(&g, clone).as_ref() != Some(&(seat.to_owned(), epoch.clone())) {
            return ExecOutcome::Obsolete;
        }
        let op = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("unretire:{}:{epoch}", clone.id),
        ));
        match self
            .threads
            .unretire_seat(&ThreadsSeatRef(seat.to_owned()), &op)
            .await
        {
            Ok(()) => match admit_threads_seat(
                cx.writer,
                &clone.id,
                &SeatTransition::Acknowledge {
                    epoch: epoch.clone(),
                },
            ) {
                Ok(()) => ExecOutcome::Done,
                Err(err) => ExecOutcome::Transient(format!("seat intent write: {err}")),
            },
            Err(ThreadsError::Rejected(msg)) => {
                record_refusal(cx, clone, epoch, &msg);
                ExecOutcome::Deferred(msg)
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Finalize the retirement of the tombstone's seat (threads releases its key with it), then delete the
    /// tombstone. A refusal is an acknowledgement when the seat reads final-retired anyway, or when threads no
    /// longer knows the seat (the read omits it); otherwise the tombstone waits and the want is issued again.
    async fn finalize_seat(&self, cx: &ExecCx<'_>, e: &EffectRecord, seat: &str) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = Self::tombstone(&g, e) else {
            return ExecOutcome::Obsolete;
        };
        if !matches!(&clone.seat_intent, Some(SeatIntent::Finalize { seat: s }) if s == seat) {
            return ExecOutcome::Obsolete;
        }
        let seat_ref = ThreadsSeatRef(seat.to_owned());
        let op = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("finalize:{}:{seat}", clone.id),
        ));
        match self.threads.finalize_retirement(&seat_ref, &op).await {
            Ok(()) => self.drop_tombstone(cx, &clone.id),
            Err(ThreadsError::Rejected(msg)) => {
                match self
                    .seat_view
                    .inspect(std::slice::from_ref(&seat_ref))
                    .await
                {
                    Ok(read)
                        if matches!(
                            read.get(&seat_ref),
                            Some(SeatStatus::Retired {
                                kind: RetirementKind::Final
                            })
                        ) =>
                    {
                        self.drop_tombstone(cx, &clone.id)
                    }
                    // The read omits a seat only on a seat-scoped refusal (NotFound, mapping.rs SEAT_SCOPED): threads
                    // no longer knows the seat, so there is nothing left to finalize.
                    Ok(read) if !read.contains_key(&seat_ref) => self.drop_tombstone(cx, &clone.id),
                    Ok(_) => ExecOutcome::Deferred(msg),
                    Err(err) => port_outcome(err),
                }
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Resolve a `ReleaseKey` tombstone's key to its binding: a key bound to a seat becomes that seat's
    /// `Finalize` tombstone (finalization releases it); an unbound key is released and the tombstone deleted.
    async fn release_key_tombstone(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        key: &str,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = Self::tombstone(&g, e) else {
            return ExecOutcome::Obsolete;
        };
        if !matches!(&clone.seat_intent, Some(SeatIntent::ReleaseKey { key: k }) if k == key) {
            return ExecOutcome::Obsolete;
        }
        // A fresh key per attempt: threads replays the stored answer for a repeated key, and the first answer
        // can predate the keyed check-in (spec §2, the ResolveSeat discipline). `release:{clone}` below is the
        // mutation and keeps its stable key.
        let ask = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("release-ask:{}:{}", clone.id, uuid::Uuid::new_v4()),
        ));
        match self.threads.reserve_continuity_key(key, None, &ask).await {
            Ok(ReserveOutcome::Reserved { seat: Some(seat) }) => match admit_threads_seat(
                cx.writer,
                &clone.id,
                &SeatTransition::TombstoneFinalize { seat: seat.0 },
            ) {
                Ok(()) => ExecOutcome::Done,
                Err(err) => ExecOutcome::Transient(format!("tombstone rewrite: {err}")),
            },
            Ok(ReserveOutcome::Reserved { seat: None }) => {
                let op = OpKey(super::namespace::salt(
                    &g.epoch,
                    &format!("release:{}", clone.id),
                ));
                match self.threads.release_continuity_key(key, &op).await {
                    Ok(()) => self.drop_tombstone(cx, &clone.id),
                    Err(err) => port_outcome(err),
                }
            }
            Ok(ReserveOutcome::Conflict) => {
                ExecOutcome::Deferred("threads reports a continuity key conflict".into())
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Resolve the reopened pane to a fresh seat and succeed the final-retired one with it. The operation key
    /// is stable per (clone, epoch), so a retry replays the same resolution; a refusal defers and is never
    /// retried as a fresh allocation.
    async fn succeed_seat(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        pane: &HerdrPaneId,
        from: &str,
        epoch: &OpId,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        let wanted = Succession::Succeed {
            pane: pane.clone(),
            from: from.to_owned(),
            epoch: epoch.clone(),
        };
        if succession(&g, clone).as_ref() != Some(&wanted) {
            return ExecOutcome::Obsolete;
        }
        let key = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("succeed:{}:{epoch}", clone.id),
        ));
        let seat = match self.mapping.resolve_seat(pane, &key).await {
            Ok(SeatResolution::Seat(seat)) => seat,
            Ok(SeatResolution::NotYet(reason)) => {
                record_refusal(cx, clone, epoch, &reason);
                return ExecOutcome::Deferred(reason);
            }
            Err(err) => {
                if let ThreadsError::Rejected(msg) = &err {
                    record_refusal(cx, clone, epoch, msg);
                }
                return port_outcome(err);
            }
        };
        if seat.0 == from {
            return ExecOutcome::NeedsRevision(format!(
                "threads resolved the retired seat {from} again for {}",
                clone.id
            ));
        }
        if let Some(holder) = g.clones.values().find(|o| {
            o.id != clone.id
                && o.lifecycle != CloneLifecycle::Retired
                && !o.is_tombstone()
                && o.threads_seat.as_deref() == Some(seat.0.as_str())
        }) {
            return ExecOutcome::NeedsRevision(format!(
                "threads seat {} resolved for {} is held by live clone {}",
                seat.0, clone.id, holder.id
            ));
        }
        match admit_threads_seat(
            cx.writer,
            &clone.id,
            &SeatTransition::Succeed {
                seat: seat.0,
                from: from.to_owned(),
                epoch: epoch.clone(),
            },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("threads_seat write: {err}")),
        }
    }

    /// Reserve the clone's continuity key (its id) with threads, bound to no seat, and record the answer. The
    /// operation key is stable per clone, so a retry after an unreachable threads replays the same request; a
    /// `Conflict` records nothing and waits (the status alert is hg-66u.40's).
    async fn reserve_key(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if !reservable(&g, clone) {
            return ExecOutcome::Obsolete;
        }
        let key = clone.id.to_string();
        let op = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("reserve:{}", clone.id),
        ));
        match self.threads.reserve_continuity_key(&key, None, &op).await {
            Ok(ReserveOutcome::Reserved { seat }) => match admit_threads_seat(
                cx.writer,
                &clone.id,
                &SeatTransition::RecordReservation {
                    key,
                    seat: seat.map(|s| s.0),
                },
            ) {
                Ok(()) => ExecOutcome::Done,
                Err(err) => ExecOutcome::Transient(format!("key reservation write: {err}")),
            },
            Ok(ReserveOutcome::Conflict) => {
                ExecOutcome::Deferred("threads reports a continuity key conflict".into())
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Bind the clone's reserved key to `seat`. The operation key is stable per (clone, seat), so every retry of
    /// one binding replays one request. A `Conflict` or another binding records nothing and defers: the
    /// reconciler re-checks a deferred row with backoff, which is the level-triggered retry; the key-conflict
    /// alert is SeatObservation's `seat_fact`, not written here.
    async fn key_link(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        key: &str,
        seat: &str,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if key_link_target(&g, clone).as_ref() != Some(&(key.to_owned(), seat.to_owned())) {
            return ExecOutcome::Obsolete;
        }
        let op = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("key_link:{}:{seat}", clone.id),
        ));
        match self
            .threads
            .reserve_continuity_key(key, Some(&ThreadsSeatRef(seat.to_owned())), &op)
            .await
        {
            Ok(ReserveOutcome::Reserved { seat: Some(bound) }) if bound.0 == seat => {
                match admit_threads_seat(
                    cx.writer,
                    &clone.id,
                    &SeatTransition::RecordReservation {
                        key: key.to_owned(),
                        seat: Some(seat.to_owned()),
                    },
                ) {
                    Ok(()) => ExecOutcome::Done,
                    Err(err) => ExecOutcome::Transient(format!("key binding write: {err}")),
                }
            }
            Ok(ReserveOutcome::Reserved { seat: other }) => ExecOutcome::Deferred(format!(
                "continuity key {key} bound to {:?}, not {seat}",
                other.map(|s| s.0)
            )),
            Ok(ReserveOutcome::Conflict) => {
                ExecOutcome::Deferred(format!("continuity key conflict on seat {seat}"))
            }
            Err(err) => port_outcome(err),
        }
    }

    async fn resolve_seat(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        pane: &HerdrPaneId,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if resolvable_pane(&g, clone).as_ref() != Some(pane) {
            return ExecOutcome::Obsolete;
        }
        let key = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("resolve:{}:{pane}", clone.id),
        ));
        let seat = match self.mapping.resolve_seat(pane, &key).await {
            Ok(SeatResolution::Seat(seat)) => seat,
            Ok(SeatResolution::NotYet(reason)) => return ExecOutcome::Deferred(reason),
            Err(err) => return port_outcome(err),
        };
        if let Some(holder) = g.clones.values().find(|o| {
            o.id != clone.id
                && o.lifecycle != CloneLifecycle::Retired
                && !o.is_tombstone()
                && o.threads_seat.as_deref() == Some(seat.0.as_str())
        }) {
            return ExecOutcome::NeedsRevision(format!(
                "threads seat {} resolved for {} is held by live clone {}",
                seat.0, clone.id, holder.id
            ));
        }
        match admit_threads_seat(
            cx.writer,
            &clone.id,
            &SeatTransition::Assign { seat: seat.0 },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("threads_seat write: {err}")),
        }
    }

    /// Resolve an unseated clone's seat from its key reservation: re-ask the reservation and admit the seat the
    /// answer carries. A key not bound yet, or in conflict, defers. Each attempt is a fresh ask with its own
    /// operation key, because threads replays a stored answer for a repeated key and the first answer can
    /// predate the keyed check-in (spec §2, the ResolveSeat discipline). Op keys are stable for mutations,
    /// where a replay is the intent, and fresh per attempt for read-like asks.
    async fn resolve_reserved(&self, cx: &ExecCx<'_>, e: &EffectRecord, key: &str) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if resolvable_key(&g, clone).as_deref() != Some(key) {
            return ExecOutcome::Obsolete;
        }
        let op = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("reserve:{}:none:{}", clone.id, uuid::Uuid::new_v4()),
        ));
        let answer = self.threads.reserve_continuity_key(key, None, &op).await;
        if matches!(answer, Ok(ReserveOutcome::Reserved { .. })) {
            let _ = self
                .journal
                .meta_set(&threads_answered_key(&clone.id), &cx.now.to_rfc3339());
        }
        let seat = match answer {
            Ok(ReserveOutcome::Reserved { seat: Some(seat) }) => seat,
            Ok(ReserveOutcome::Reserved { seat: None }) => {
                return ExecOutcome::Deferred("continuity key not bound to a seat yet".into());
            }
            Ok(ReserveOutcome::Conflict) => {
                return ExecOutcome::Deferred("threads reports a continuity key conflict".into());
            }
            Err(err) => return port_outcome(err),
        };
        if let Some(holder) = g.clones.values().find(|o| {
            o.id != clone.id
                && o.lifecycle != CloneLifecycle::Retired
                && !o.is_tombstone()
                && o.threads_seat.as_deref() == Some(seat.0.as_str())
        }) {
            return ExecOutcome::NeedsRevision(format!(
                "threads seat {} resolved for {} is held by live clone {}",
                seat.0, clone.id, holder.id
            ));
        }
        match admit_threads_seat(
            cx.writer,
            &clone.id,
            &SeatTransition::Assign { seat: seat.0 },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(err) => ExecOutcome::Transient(format!("threads_seat write: {err}")),
        }
    }

    async fn ensure(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some((scope, channel, topic, active)) = g.channel(&e.object) else {
            return ExecOutcome::Obsolete;
        };
        if !active {
            return ExecOutcome::Obsolete;
        }
        if channel.thread_id.is_some() {
            return ExecOutcome::Done;
        }
        let key = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("ensure:{}", e.object),
        ));
        match self.threads.ensure_thread(scope, &topic, &key).await {
            Ok(thread) => {
                if let Err(o) = admit_bookkeeping(
                    cx,
                    serde_json::json!({ "sub": "channel", "object": e.object, "thread_id": thread.0 }),
                ) {
                    return o;
                }
                let _ = self.journal.meta_set(
                    &super::namespace::salt(&g.epoch, &format!("{TOPIC_PREFIX}{}", e.object)),
                    &topic,
                );
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
        let Some((_, channel, topic, active)) = g.channel(&e.object) else {
            return ExecOutcome::Obsolete;
        };
        if !active {
            return ExecOutcome::Obsolete;
        }
        let Some(thread) = channel.thread_id.clone() else {
            return ExecOutcome::Deferred("channel not created yet".into());
        };
        let key = OpKey(super::namespace::salt(
            &g.epoch,
            &format!("topic:{}:{:016x}", e.object, topic_generation(&topic)),
        ));
        match self
            .threads
            .set_topic(&ThreadRef(thread), &topic, &key)
            .await
        {
            Ok(()) => {
                let _ = self.journal.meta_set(
                    &super::namespace::salt(&g.epoch, &format!("{TOPIC_PREFIX}{}", e.object)),
                    &topic,
                );
                ExecOutcome::Done
            }
            Err(err) => port_outcome(err),
        }
    }

    async fn invite(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        thread: &str,
        constraint: InviteConstraint,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        if clone.threads_seat.is_none() {
            return ExecOutcome::Deferred("no threads seat yet".into());
        }
        if !g.clone_active(clone) {
            return ExecOutcome::Obsolete;
        }
        let Some(seat) = g.membership_seat(clone).map(ThreadsSeatRef) else {
            return ExecOutcome::Obsolete;
        };
        let thread_ref = ThreadRef(thread.to_owned());
        let key = OpKey(format!(
            "invite:{thread}:{}:{}:{}",
            seat.0,
            constraint_name(constraint),
            short(&e.id)
        ));
        if let Err(err) = self
            .threads
            .invite(&thread_ref, &seat, constraint, &key)
            .await
        {
            return port_outcome(err);
        }
        // What threads reports now; a failed read only delays the ids, never invents a state.
        let detail = self
            .threads
            .membership_detail(&thread_ref, &seat)
            .await
            .ok()
            .flatten();
        let state = detail
            .as_ref()
            .map_or(InvitationState::Pending, |d| d.state);
        let link = ThreadsLink {
            seat: seat.0.clone(),
            occupant: clone
                .occupant
                .as_ref()
                .map(|o| o.native_session.to_string()),
            invitation: detail.as_ref().and_then(|d| d.invitation.clone()),
            requirement: detail.as_ref().and_then(|d| d.requirement.clone()),
            revision: detail.as_ref().and_then(|d| d.revision),
        };
        match self.record_invitation(
            cx,
            InvWrite {
                clone: &clone.id,
                thread,
                constraint,
                state,
                link: &link,
                expect_seat: None,
            },
        ) {
            Ok(()) => ExecOutcome::Done,
            Err(o) => o,
        }
    }

    async fn release(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        thread: &str,
        constraint: InviteConstraint,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        let Some(inv) = clone
            .invitations
            .iter()
            .find(|i| i.thread == thread && i.constraint == constraint)
        else {
            return ExecOutcome::Obsolete;
        };
        let Some(link) = inv.link.clone() else {
            return ExecOutcome::Obsolete;
        };
        let key = OpKey(format!(
            "release:{thread}:{}:{}",
            link.seat,
            link.requirement.as_deref().unwrap_or("-")
        ));
        match self
            .threads
            .release_requirement(
                &ThreadRef(thread.to_owned()),
                &ThreadsSeatRef(link.seat.clone()),
                &key,
            )
            .await
        {
            Ok(()) => match self.record_invitation(
                cx,
                InvWrite {
                    clone: &clone.id,
                    thread,
                    constraint,
                    state: InvitationState::Released,
                    link: &link,
                    expect_seat: Some(link.seat.as_str()),
                },
            ) {
                Ok(()) => ExecOutcome::Done,
                Err(o) => o,
            },
            Err(err) => port_outcome(err),
        }
    }

    async fn notify(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        purpose: &str,
        severity: Severity,
        body: &str,
        key: &str,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let thread = g.notify_channel_owner(&e.object).and_then(|owner| {
            g.channel(&owner)
                .and_then(|(_, c, _, _)| c.thread_id.clone())
        });
        let Some(thread) = thread else {
            return ExecOutcome::Deferred("channel not created yet".into());
        };
        match self
            .threads
            .notify(&ThreadRef(thread), severity, body, &OpKey(key.to_owned()))
            .await
        {
            Ok(()) => {
                if purpose == "reload_required" {
                    let _ = self.journal.meta_set(
                        &super::namespace::salt(&g.epoch, &format!("{RELOAD_PREFIX}{}", e.object)),
                        "1",
                    );
                }
                ExecOutcome::Done
            }
            Err(err) => port_outcome(err),
        }
    }

    /// Membership polling: record what threads reports, and only that.
    async fn poll(
        &self,
        cx: &ExecCx<'_>,
        e: &EffectRecord,
        thread: &str,
        constraint: InviteConstraint,
        leaving: bool,
    ) -> ExecOutcome {
        let g = match self.graph(cx) {
            Ok(g) => g,
            Err(o) => return o,
        };
        let Some(clone) = CloneId::parse(e.object.as_str())
            .ok()
            .and_then(|i| g.clones.get(&i))
        else {
            return ExecOutcome::Obsolete;
        };
        let Some(inv) = clone
            .invitations
            .iter()
            .find(|i| i.thread == thread && i.constraint == constraint)
        else {
            return ExecOutcome::Obsolete;
        };
        let Some(link) = inv.link.clone() else {
            return ExecOutcome::Obsolete;
        };
        let detail = match self
            .threads
            .membership_detail(
                &ThreadRef(thread.to_owned()),
                &ThreadsSeatRef(link.seat.clone()),
            )
            .await
        {
            Ok(d) => d,
            Err(err) => return port_outcome(err),
        };
        // No membership yet: the invitation is still pending on the threads side.
        let state = detail
            .as_ref()
            .map_or(InvitationState::Pending, |d| d.state);
        let mut fresh = link.clone();
        if let Some(d) = &detail {
            fresh.invitation = d.invitation.clone().or(fresh.invitation);
            fresh.requirement = d.requirement.clone().or(fresh.requirement);
            fresh.revision = d.revision.or(fresh.revision);
        }
        if (state != inv.state || fresh != link)
            && let Err(o) = self.record_invitation(
                cx,
                InvWrite {
                    clone: &clone.id,
                    thread,
                    constraint,
                    state,
                    link: &fresh,
                    expect_seat: Some(link.seat.as_str()),
                },
            )
        {
            return o;
        }
        let settled = match state {
            InvitationState::Pending => false,
            InvitationState::Accepted => !leaving,
            InvitationState::Released | InvitationState::Retired => true,
        };
        if settled {
            ExecOutcome::Done
        } else {
            ExecOutcome::Transient(format!("{thread}: still {state:?}").to_lowercase())
        }
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
            EffectKind::Custom(k) => [
                POLL_KIND,
                RESOLVE_KIND,
                RESOLVE_RESERVED_KIND,
                CLEAR_RESTORE_KIND,
                SUCCEED_KIND,
                ACK_RESTORE_KIND,
                UNRETIRE_KIND,
                FINALIZE_KIND,
                RELEASE_KEY_KIND,
                RESERVE_KEY_KIND,
                KEY_LINK_KIND,
            ]
            .contains(&k.as_str()),
            _ => false,
        }
    }

    fn is_implied(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> bool {
        let Ok(g) = self.graph(cx) else { return true };
        if e.nonce_label != epoch_label(&g.epoch) {
            return false;
        }
        let payload = self.payload(e).unwrap_or(Payload::None);
        match (&e.kind, &payload) {
            (_, Payload::Notify { purpose, .. }) => {
                let alive = g
                    .notify_channel_owner(&e.object)
                    .and_then(|o| g.channel(&o))
                    .is_some_and(|c| c.3);
                if purpose == "reload_required" {
                    let seat = SeatId::parse(e.object.as_str())
                        .ok()
                        .and_then(|i| g.seats.get(&i))
                        .is_some_and(|s| s.reload_required);
                    let clone = CloneId::parse(e.object.as_str())
                        .ok()
                        .and_then(|i| g.clones.get(&i))
                        .is_some_and(|c| c.reload_required);
                    alive && (seat || clone)
                } else {
                    alive
                }
            }
            _ => {
                let wants = system_wants(&g, &self.journal);
                wants
                    .iter()
                    .any(|w| w.kind == e.kind && w.object == e.object && w.payload == payload)
            }
        }
    }

    async fn execute(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        let epoch = match super::namespace::epoch(cx.tree) {
            Ok(e) => e,
            Err(e) => return ExecOutcome::Failed(e.to_string()),
        };
        if e.nonce_label != epoch_label(&epoch) {
            return ExecOutcome::Obsolete;
        }
        super::namespace::EXPECTED_EPOCH
            .scope(epoch, self.execute_scoped(cx, e))
            .await
    }
}
impl ThreadsExecutor {
    async fn execute_scoped(&self, cx: &ExecCx<'_>, e: &EffectRecord) -> ExecOutcome {
        match (&e.kind, self.payload(e)) {
            (EffectKind::EnsureThread, _) => self.ensure(cx, e).await,
            (EffectKind::SetTopic, _) => self.set_topic(cx, e).await,
            (EffectKind::Invite, Some(Payload::Invite { thread, constraint })) => {
                self.invite(cx, e, &thread, constraint).await
            }
            (EffectKind::ReleaseRequirement, Some(Payload::Release { thread, constraint })) => {
                self.release(cx, e, &thread, constraint).await
            }
            (
                EffectKind::Notify,
                Some(Payload::Notify {
                    purpose,
                    severity,
                    body,
                    key,
                }),
            ) => {
                self.notify(
                    cx,
                    e,
                    &purpose,
                    severity,
                    &body,
                    &super::namespace::salt(
                        &super::namespace::EXPECTED_EPOCH.with(Clone::clone),
                        &key,
                    ),
                )
                .await
            }
            (EffectKind::Custom(_), Some(Payload::ResolveSeat { pane })) => {
                self.resolve_seat(cx, e, &pane).await
            }
            (EffectKind::Custom(_), Some(Payload::ResolveReserved { key })) => {
                self.resolve_reserved(cx, e, &key).await
            }
            (EffectKind::Custom(_), Some(Payload::ClearRestore { epoch })) => {
                self.clear_restore(cx, e, &epoch)
            }
            (EffectKind::Custom(_), Some(Payload::AckRestore { epoch })) => {
                self.ack_restore(cx, e, &epoch)
            }
            (EffectKind::Custom(_), Some(Payload::UnretireSeat { seat, epoch })) => {
                self.unretire_seat(cx, e, &seat, &epoch).await
            }
            (EffectKind::Custom(_), Some(Payload::FinalizeSeat { seat })) => {
                self.finalize_seat(cx, e, &seat).await
            }
            (EffectKind::Custom(_), Some(Payload::ReleaseKey { key })) => {
                self.release_key_tombstone(cx, e, &key).await
            }
            (EffectKind::Custom(_), Some(Payload::ReserveKey)) => self.reserve_key(cx, e).await,
            (EffectKind::Custom(_), Some(Payload::KeyLink { key, seat })) => {
                self.key_link(cx, e, &key, &seat).await
            }
            (EffectKind::Custom(_), Some(Payload::SucceedSeat { pane, from, epoch })) => {
                self.succeed_seat(cx, e, &pane, &from, &epoch).await
            }
            (
                EffectKind::Custom(_),
                Some(Payload::Poll {
                    thread,
                    constraint,
                    leaving,
                }),
            ) => self.poll(cx, e, &thread, constraint, leaving).await,
            (kind, _) => {
                ExecOutcome::Failed(format!("{} effect {} has no payload", kind.as_str(), e.id))
            }
        }
    }
}

pub(crate) fn epoch_label(epoch: &str) -> Option<String> {
    if epoch.is_empty() {
        None
    } else {
        Some(format!("threads-epoch:{epoch}"))
    }
}
pub(crate) fn epoch_rev(epoch: &str, rev: u64) -> u64 {
    if epoch.is_empty() {
        return rev;
    }
    let mut h = Sha256::new();
    h.update(epoch.as_bytes());
    h.update(rev.to_be_bytes());
    u64::from_be_bytes(h.finalize()[..8].try_into().unwrap())
}
