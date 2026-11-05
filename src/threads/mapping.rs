//! Clone pane -> threads seat (spec §7.1): a clone's occupant is invited under the threads seat that
//! herdr-threads resolves for the clone's exact Herdr host and pane.
use super::adapter::{api_error, call_budget, operation_id};
use crate::model::HerdrPaneId;
use crate::ports::threads::{OpKey, ThreadsError, ThreadsSeatRef};
use herdr_threads::client::local::LocalSocketClient;
use herdr_threads::protocol::commands::{Command, ResolveSeat};
use herdr_threads::protocol::ids::{HostLocator, HostTargetId, OperationId};
use herdr_threads::protocol::results::{CommandResult, ErrorCode};
use herdr_threads::protocol::time::Clock as ThreadsClock;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Outcome of a seat-lifecycle resolution of one pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatResolution {
    Seat(ThreadsSeatRef),
    /// Threads refused for now (restore hold, held pane, pending structural reconciliation); retry later
    /// with the same operation key. Never a reason to allocate a fresh seat.
    NotYet(String),
}

#[async_trait::async_trait]
pub trait PaneSeatMap: Send + Sync {
    /// The threads seat currently mapped to `pane`. `Some` does not imply an enrolled agent.
    /// The real map may allocate a bare seat; it never enrolls an agent binding.
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError>;

    /// Seat-lifecycle resolution of `pane` under the stable operation key `op`: replays with the same key
    /// return the same answer. A threads refusal is `NotYet`, never an error.
    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError>;
}

/// Resolve a graph-managed pane through the host-qualified, restore-guarded public API.
/// This may allocate a bare local seat, but never checks in, accepts invitations or ACKs.
/// A lost response can leave a bare seat; a fresh resolution returns its unique current owner.
pub struct ThreadsSeatMap {
    client: LocalSocketClient,
    clock: Arc<dyn ThreadsClock>,
    timeout: Duration,
    host: HostLocator,
}

impl ThreadsSeatMap {
    pub fn new(
        socket: PathBuf,
        instance: herdr_threads::node::NodeUuid,
        clock: Arc<dyn ThreadsClock>,
        host: HostLocator,
    ) -> Self {
        Self {
            client: LocalSocketClient::new(socket, clock.clone(), instance, None),
            clock,
            timeout: Duration::from_secs(5),
            host,
        }
    }
}

#[async_trait::async_trait]
impl PaneSeatMap for ThreadsSeatMap {
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
        let target =
            HostTargetId::parse(pane.0.clone()).map_err(|e| ThreadsError::Rejected(e.into()))?;
        // A new operation is essential: replaying a pane-keyed resolution could return
        // the previous occupant's seat after a rebind. Allocation itself is unique per host/pane.
        let query = ResolveSeat {
            target,
            host: self.host.clone(),
            operation: OperationId::new(format!("graph-resolve-{}", uuid::Uuid::new_v4())),
            locator_hint: None,
            tmux_hint: None,
        };
        let budget = call_budget(&*self.clock, self.timeout);
        match self
            .client
            .call_async(Command::ResolveSeat(query), &budget)
            .await
        {
            Ok(CommandResult::SeatResolved(seat)) => {
                Ok(Some(ThreadsSeatRef(seat.as_str().to_owned())))
            }
            Ok(_) => Err(ThreadsError::Rejected(
                "unexpected result for host-qualified seat resolution".into(),
            )),
            Err(e) => Err(api_error(e)),
        }
    }

    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError> {
        let target =
            HostTargetId::parse(pane.0.clone()).map_err(|e| ThreadsError::Rejected(e.into()))?;
        let query = ResolveSeat {
            target,
            host: self.host.clone(),
            operation: operation_id(op),
            locator_hint: None,
            tmux_hint: None,
        };
        let budget = call_budget(&*self.clock, self.timeout);
        match self
            .client
            .call_async(Command::ResolveSeat(query), &budget)
            .await
        {
            Ok(CommandResult::SeatResolved(seat)) => Ok(SeatResolution::Seat(ThreadsSeatRef(
                seat.as_str().to_owned(),
            ))),
            Ok(_) => Err(ThreadsError::Rejected(
                "unexpected result for host-qualified seat resolution".into(),
            )),
            // herdr-threads v3.0.0 refuses a restore hold (explicit or baseline), a target with an unresolved
            // seat mapping and an owner needing structural reconciliation with TargetUnresolved
            // (store/seats/resolution.rs); all of them lift without graph action.
            Err(e) if e.code == ErrorCode::TargetUnresolved => Ok(SeatResolution::NotYet(e.detail)),
            Err(e) => Err(api_error(e)),
        }
    }
}

/// Test map: explicit pane -> seat entries.
#[derive(Default)]
pub struct FakePaneSeatMap {
    seats: Mutex<BTreeMap<String, String>>,
    failing: Mutex<bool>,
    not_yet: Mutex<BTreeMap<String, String>>,
    resolve_ops: Mutex<Vec<OpKey>>,
}

impl FakePaneSeatMap {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&self, pane: &HerdrPaneId, seat: &str) {
        self.seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pane.0.clone(), seat.to_owned());
    }
    pub fn clear(&self, pane: &HerdrPaneId) {
        self.seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pane.0);
    }
    /// `resolve_seat` of `pane` is refused with `reason` until the entry is cleared with `clear_not_yet`.
    pub fn set_not_yet(&self, pane: &HerdrPaneId, reason: &str) {
        self.not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pane.0.clone(), reason.to_owned());
    }
    pub fn clear_not_yet(&self, pane: &HerdrPaneId) {
        self.not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pane.0);
    }
    /// Every operation key `resolve_seat` was called with, in order.
    pub fn resolve_ops(&self) -> Vec<OpKey> {
        self.resolve_ops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    /// While failing, lookups report a disconnected service.
    pub fn set_failing(&self, failing: bool) {
        *self.failing.lock().unwrap_or_else(|e| e.into_inner()) = failing;
    }
}

#[async_trait::async_trait]
impl PaneSeatMap for FakePaneSeatMap {
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
        if *self.failing.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(ThreadsError::Disconnected(
                "fake seat map unavailable".into(),
            ));
        }
        Ok(self
            .seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
            .cloned()
            .map(ThreadsSeatRef))
    }

    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError> {
        self.resolve_ops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(op.clone());
        if *self.failing.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(ThreadsError::Disconnected(
                "fake seat map unavailable".into(),
            ));
        }
        if let Some(reason) = self
            .not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
        {
            return Ok(SeatResolution::NotYet(reason.clone()));
        }
        match self
            .seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
        {
            Some(seat) => Ok(SeatResolution::Seat(ThreadsSeatRef(seat.clone()))),
            None => Ok(SeatResolution::NotYet(format!("no seat for pane {pane}"))),
        }
    }
}
