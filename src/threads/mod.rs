//! herdr-threads adapter implementing crate::ports::threads::ThreadsPort, plus FakeThreads (spec §7). Owned by hg-zmi.11.
//!
//! * `adapter::ServiceThreads` — the real port over the persistent service client;
//! * `fake::FakeThreads` — the in-memory port every other test uses;
//! * `effects` — the reconciler effect family (channels, required/ordinary invitations, release, notify);
//! * `mapping` — clone pane -> threads seat;
//! * this module — registration, bookkeeping mutations, `who` and the pending-invitations query.
pub mod adapter;
pub mod discovery;
pub mod effects;
pub mod fake;
pub mod mapping;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod tests;

pub use adapter::{Discovered, ServiceThreads, discover};
pub use effects::{ThreadsExecutor, ThreadsSource};
pub use fake::FakeThreads;
pub use mapping::{FakePaneSeatMap, PaneSeatMap, ThreadsSeatMap};

use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::model::clone::{CloneRecord, InvitationState, InviteConstraint, ThreadsLink};
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{AnyId, CloneId, IdKind, SeatId};
use crate::ports::store::{Store, StoreError};
use crate::ports::threads::ThreadsPort;
use crate::reconcile::Reconciler;
use crate::store::Record;
use crate::store::tree::{CommitView, TreeRead};
use crate::writer::{Applied, Mutation, MutationCx, MutationError, MutationRegistry, Reject};
use effects::{Graph, GraphCache};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub const CHANNEL_KEY: &str = "bookkeeping.channel";
pub const INVITATION_KEY: &str = "bookkeeping.invitation";

/// Register the threads effect source and executor with the reconciler.
pub fn register_with(reconciler: &Reconciler, threads: Arc<dyn ThreadsPort>, mapping: Arc<dyn PaneSeatMap>) {
    let cache = Arc::new(GraphCache::default());
    reconciler.register_source(Arc::new(ThreadsSource::new(reconciler.instance(), cache.clone())));
    reconciler.register_executor(Arc::new(effects::ThreadsExecutor::new(
        threads,
        mapping,
        reconciler.journal().clone(),
        cache,
    )));
}

// ---------------------------------------------------------------------------------------------
// bookkeeping mutations
// ---------------------------------------------------------------------------------------------

fn bug(e: impl std::fmt::Display) -> MutationError {
    MutationError::Bug(e.to_string())
}

fn gone(object: &AnyId) -> MutationError {
    MutationError::Reject(Reject {
        reason: "object_missing".into(),
        explanation: format!("{object} does not exist at the committed head"),
        current_revs: vec![],
    })
}

fn edit<R: Record>(
    cx: &mut MutationCx<'_>,
    object: &AnyId,
    f: impl FnOnce(&mut R) -> Result<(), MutationError>,
) -> Result<(), MutationError> {
    let loc = cx.tree.locate(object)?.ok_or_else(|| gone(object))?;
    let mut rec: R = cx.tree.read_record(&loc.record_path)?.ok_or_else(|| gone(object))?;
    f(&mut rec)?;
    cx.tree.put_record(loc.record_path, &mut rec)?;
    Ok(())
}

fn args<T: DeserializeOwned>(cx: &MutationCx<'_>) -> Result<T, MutationError> {
    serde_json::from_value(cx.request.args.clone()).map_err(bug)
}

#[derive(Deserialize)]
struct ChannelArgs {
    object: AnyId,
    thread_id: String,
}

/// `bookkeeping.channel`: store the thread id of a teamspace or seat channel.
struct SetChannel;
impl Mutation for SetChannel {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: ChannelArgs = args(cx)?;
        let set = |ch: &mut crate::model::common::Channel| ch.thread_id = Some(a.thread_id.clone());
        match a.object.kind() {
            IdKind::Teamspace => edit::<TeamspaceRecord>(cx, &a.object, |r| {
                set(&mut r.channel);
                Ok(())
            })?,
            IdKind::Seat => edit::<SeatRecord>(cx, &a.object, |r| {
                set(&mut r.channel);
                Ok(())
            })?,
            other => return Err(bug(format!("{other:?} objects have no channel"))),
        }
        Ok(Applied { summary: format!("channel {} = {}", a.object, a.thread_id), action: None })
    }
}

#[derive(Deserialize)]
struct InvitationArgs {
    clone: CloneId,
    thread: String,
    constraint: InviteConstraint,
    state: InvitationState,
    #[serde(default)]
    link: Option<ThreadsLink>,
    /// Apply only while the stored invitation still belongs to this occupant; absent = unconditional.
    #[serde(default)]
    expect_occupant: Option<String>,
}

/// `bookkeeping.invitation`: record what threads reported about one invitation of a clone.
struct SetInvitation;
impl Mutation for SetInvitation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: InvitationArgs = args(cx)?;
        edit::<CloneRecord>(cx, &a.clone.to_any(), |r| {
            let existing = r.invitations.iter_mut().find(|i| i.thread == a.thread && i.constraint == a.constraint);
            if let (Some(expect), Some(i)) = (&a.expect_occupant, &existing)
                && i.link.as_ref().and_then(|l| l.occupant.as_ref()) != Some(expect)
            {
                return Ok(());
            }
            match existing {
                Some(i) => {
                    i.state = a.state;
                    if a.link.is_some() {
                        i.link = a.link.clone();
                    }
                }
                None => r.invitations.push(crate::model::clone::Invitation {
                    thread: a.thread.clone(),
                    constraint: a.constraint,
                    state: a.state,
                    link: a.link.clone(),
                }),
            }
            Ok(())
        })?;
        Ok(Applied { summary: format!("invitation {} {} {:?}", a.clone, a.thread, a.state), action: None })
    }
}

/// Registers `bookkeeping.channel` and `bookkeeping.invitation`.
pub fn register_mutations(reg: &mut MutationRegistry) {
    reg.register(CHANNEL_KEY, Arc::new(SetChannel));
    reg.register(INVITATION_KEY, Arc::new(SetInvitation));
}

// ---------------------------------------------------------------------------------------------
// pending invitations (used by /seat)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingInvitation {
    pub thread: String,
    pub constraint: InviteConstraint,
    /// The exact command line the clone's agent runs in its own pane.
    pub accept_command: String,
}

fn accept_command(thread: &str, constraint: InviteConstraint, link: Option<&ThreadsLink>) -> String {
    match constraint {
        InviteConstraint::Ordinary => format!("herdr-threads accept {thread}"),
        InviteConstraint::Required => match link {
            Some(ThreadsLink { invitation: Some(inv), requirement: Some(req), revision: Some(rev), .. }) => {
                format!("herdr-threads accept-required {thread} --invitation {inv} --requirement {req} --revision {rev}")
            }
            // The episode ids are not recorded yet: the agent reads them from the thread.
            _ => format!("herdr-threads thread participants {thread}"),
        },
    }
}

/// The clone's invitations that are still waiting for its agent, with the command that answers each.
/// An unreadable clone has none.
pub fn pending_invitations(tree: &dyn TreeRead, clone: &CloneId) -> Vec<PendingInvitation> {
    let Ok(Some(loc)) = crate::store::layout::locate(tree, &clone.to_any()) else { return Vec::new() };
    let Ok(Some(rec)) = crate::store::record::read_toml::<CloneRecord>(tree, &loc.record_path) else {
        return Vec::new();
    };
    rec.invitations
        .iter()
        .filter(|i| i.state == InvitationState::Pending)
        .map(|i| PendingInvitation {
            thread: i.thread.clone(),
            constraint: i.constraint,
            accept_command: accept_command(&i.thread, i.constraint, i.link.as_ref()),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// who
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedId {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WhoReply {
    pub teamspace: Option<NamedId>,
    pub seat: Option<NamedId>,
    pub clone: Option<NamedId>,
    /// Native session record id of the clone's occupant.
    pub native_session: Option<String>,
}

impl WhoReply {
    pub fn is_bound(&self) -> bool {
        self.seat.is_some() || self.clone.is_some()
    }

    /// `teamspace/seat/clone/ns`, stopping where the identity does; `unbound` when nothing matched.
    pub fn render(&self) -> String {
        if !self.is_bound() {
            return "unbound".into();
        }
        let parts: Vec<String> = [
            self.teamspace.as_ref().map(|n| n.name.clone()),
            self.seat.as_ref().map(|n| n.name.clone()),
            self.clone.as_ref().map(|n| n.name.clone()),
            self.native_session.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        parts.join("/")
    }
}

/// Which graph objects a Herdr pane id, a threads seat id, or a graph clone/seat id refers to.
pub fn who(tree: &dyn TreeRead, target: &str) -> Result<WhoReply, StoreError> {
    let g = Graph::load(tree)?;
    let found = g
        .clones
        .values()
        .filter(|c| c.runtime.bound.as_ref().and_then(|b| b.pane_id.as_ref()).is_some_and(|p| p.0 == target))
        .max_by_key(|c| (c.lifecycle == crate::model::common::CloneLifecycle::Active, c.rev))
        .or_else(|| {
            g.clones.values().find(|c| c.invitations.iter().any(|i| i.link.as_ref().is_some_and(|l| l.seat == target)))
        })
        .or_else(|| CloneId::parse(target).ok().and_then(|id| g.clones.get(&id)))
        .cloned();
    let seat = match &found {
        Some(c) => g.seats.get(&c.seat),
        None => SeatId::parse(target).ok().and_then(|id| g.seats.get(&id)),
    };
    let named = |id: String, name: &String| NamedId { id, name: name.clone() };
    let teamspace = seat.and_then(|s| g.teamspaces.get(&s.teamspace)).map(|t| named(t.id.to_string(), &t.name));
    let native_session = found.as_ref().and_then(|c| c.occupant.as_ref()).map(|o| o.native_session.to_string());
    Ok(WhoReply {
        teamspace,
        seat: seat.map(|s| named(s.id.to_string(), &s.name)),
        clone: found.as_ref().map(|c| named(c.id.to_string(), &c.name)),
        native_session,
    })
}

/// Registers the `who` IPC command.
pub fn register_commands(reg: &mut Registry, store: Arc<dyn Store>) {
    reg.command("who", move |_cx: CommandCtx, args: serde_json::Value| {
        let store = store.clone();
        async move {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CommandError::bad_request("who needs {target}"))?
                .to_owned();
            let head = store.head().map_err(|e| CommandError::internal(e.to_string()))?;
            let view = CommitView { store: &*store, at: head };
            let reply = who(&view, &target).map_err(|e| CommandError::internal(e.to_string()))?;
            Ok(serde_json::json!(reply))
        }
    });
}
